//! The instance with the public site enabled, end to end over HTTP: the bundled `website` and
//! `ecommerce` packages are installed through the real installer, seeded, and served next to the
//! REST API and the backoffice build under `/admin`.

use crate::app::App;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use obop_auth::Auth;
use obop_pkg::{Bundled, Marketplace};
use serde_json::{Value as J, json};
use std::sync::Arc;
use tower::ServiceExt;

struct Fixture {
    router: axum::Router,
    token: String,
    app: App,
    _dir: std::path::PathBuf,
}

async fn fixture(public: bool) -> Fixture {
    let app = App::new();
    for id in ["website", "ecommerce"] {
        app.installer
            .install(&Bundled.fetch(id).unwrap(), "marketplace", "test")
            .unwrap();
    }
    obop_site::seed::seed(&app.store, &app.dispatcher).unwrap();
    let auth = Arc::new(Auth::with_test_params(app.store.backend()));
    auth.create_user(
        "owner",
        "a long enough password",
        &["platform.admin".into()],
    )
    .unwrap();
    let dir = std::env::temp_dir().join(format!(
        "obop-admin-{}-{}",
        std::process::id(),
        if public { "p" } else { "o" }
    ));
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(dir.join("index.html"), "<html>BACKOFFICE-SHELL</html>").unwrap();
    std::fs::write(dir.join("assets/app.js"), "console.log('admin')").unwrap();
    let state = obop_http::HttpState::new(app.dispatcher.clone(), auth);
    let router = if public {
        let site = Arc::new(obop_site::Site::new(
            obop_commands::PublicApi::new(app.dispatcher.clone(), app.store.clone()),
            obop_site::Config {
                settings_ttl: std::time::Duration::ZERO,
                ..obop_site::Config::default()
            },
        ));
        obop_http::public::router(
            state,
            dir.to_str(),
            site,
            obop_http::public::PublicConfig::default(),
        )
    } else {
        obop_http::router(state, dir.to_str())
    };
    let mut f = Fixture {
        router,
        token: String::new(),
        app,
        _dir: dir,
    };
    let (_, b, _) = send(
        &f.router,
        Request::post("/api/auth/login")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"username": "owner", "password": "a long enough password"}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    f.token = serde_json::from_str::<J>(&b).unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    f
}

async fn send(r: &axum::Router, req: Request<Body>) -> (u16, String, axum::http::HeaderMap) {
    let resp = r.clone().oneshot(req).await.unwrap();
    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        String::from_utf8_lossy(&bytes).into_owned(),
        headers,
    )
}

async fn get(r: &axum::Router, path: &str) -> (u16, String, axum::http::HeaderMap) {
    send(
        r,
        Request::get(path)
            .header("host", "shop.test")
            .body(Body::empty())
            .unwrap(),
    )
    .await
}

fn between<'a>(s: &'a str, start: &str, end: &str) -> &'a str {
    let a = s.find(start).unwrap_or_else(|| panic!("no {start} in {s}")) + start.len();
    &s[a..a + s[a..].find(end).unwrap()]
}

#[tokio::test]
async fn the_public_site_sits_at_the_root_and_the_backoffice_moves_to_admin() {
    let f = fixture(true).await;
    let (s, home, h) = get(&f.router, "/").await;
    assert_eq!(s, 200);
    assert!(home.contains("Fernhill Goods") && !home.contains("BACKOFFICE-SHELL"));
    assert!(
        h["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("default-src 'none'")
    );
    // The backoffice build is under /admin/, and unknown paths below it fall back to its index.
    for p in ["/admin/", "/admin", "/admin/shop/orders/42"] {
        let (s, body, _) = get(&f.router, p).await;
        assert_eq!(
            (s, body.as_str()),
            (200, "<html>BACKOFFICE-SHELL</html>"),
            "{p}"
        );
    }
    let (s, js, _) = get(&f.router, "/admin/assets/app.js").await;
    assert_eq!((s, js.as_str()), (200, "console.log('admin')"));
    // The REST API is untouched: 401 without a token, JSON 404 for unknown API paths.
    let (s, _, _) = get(&f.router, "/api/meta").await;
    assert_eq!(s, 401);
    let (s, body, h) = get(&f.router, "/api/nothing").await;
    assert_eq!(s, 404);
    assert_eq!(h["content-type"], "application/json");
    assert!(body.contains("not found"));
    // Public pages that are not the REST API.
    for p in [
        "/shop",
        "/blog",
        "/about",
        "/sitemap.xml",
        "/robots.txt",
        "/llms.txt",
        "/feed.xml",
    ] {
        assert_eq!(get(&f.router, p).await.0, 200, "{p}");
    }
    assert_eq!(get(&f.router, "/nope").await.0, 404);
}

#[tokio::test]
async fn without_the_public_site_the_backoffice_is_at_the_root_exactly_as_before() {
    let f = fixture(false).await;
    let (s, body, _) = get(&f.router, "/").await;
    assert_eq!((s, body.as_str()), (200, "<html>BACKOFFICE-SHELL</html>"));
    let (s, body, _) = get(&f.router, "/shop").await;
    assert_eq!(
        (s, body.as_str()),
        (200, "<html>BACKOFFICE-SHELL</html>"),
        "SPA fallback"
    );
    assert_eq!(get(&f.router, "/api/meta").await.0, 401);
}

#[tokio::test]
async fn an_order_placed_in_the_browser_shows_up_in_the_backoffice_api() {
    let f = fixture(true).await;
    // Pick a listing and a delivery method from the public site itself.
    let (_, shop, _) = get(&f.router, "/shop").await;
    let href = between(&shop, "<h3><a href=\"/shop/", "\"");
    let id = &href[..36];
    // The cart page hands out the CSRF cookie and the form tokens.
    let (s, cart, h) = get(&f.router, &format!("/cart?add={id}")).await;
    assert_eq!(s, 200);
    let cookie = h["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    assert!(cookie.starts_with("obop_csrf="));
    let csrf = between(&cart, "name=\"csrf\" value=\"", "\"");
    let idem = between(&cart, "name=\"idem\" value=\"", "\"");
    let method = between(&cart, "name=\"shipping_method_id\" value=\"", "\"");
    let body = format!(
        "csrf={csrf}&idem={idem}&id={id}&qty=2&customer_name=Test+Buyer&customer_email=buyer%40example.test&shipping_address=1+Example+Street%0A00000+Testville&shipping_method_id={method}&website="
    );
    let post = || {
        Request::post("/shop/checkout")
            .header("host", "shop.test")
            .header("origin", "http://shop.test")
            .header("cookie", cookie.clone())
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(body.clone()))
            .unwrap()
    };
    let (s, _, h) = send(&f.router, post()).await;
    assert_eq!(s, 303);
    let loc = h["location"].to_str().unwrap().to_string();
    // Pressing the button twice makes no second order.
    let (s2, _, h2) = send(&f.router, post()).await;
    assert_eq!((s2, h2["location"].to_str().unwrap()), (303, loc.as_str()));
    let (s, page, _) = get(&f.router, &loc).await;
    assert_eq!(s, 200);
    assert!(page.contains("Thank you for your order") && page.contains("buyer@example.test"));

    // The shop owner sees one pending order, with its lines, in the generated API.
    let call = |model: &str| {
        let req = Request::post(format!("/api/{model}/list"))
            .header("authorization", format!("Bearer {}", f.token))
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let router = f.router.clone();
        async move { serde_json::from_str::<J>(&send(&router, req).await.1).unwrap() }
    };
    let orders = call("ecommerce_order").await;
    let rows = orders["value"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{orders}");
    assert_eq!(rows[0]["state"], "pending");
    assert_eq!(rows[0]["customer_email"], "buyer@example.test");
    assert!(
        rows[0].get("access_token").is_none(),
        "the link token is never readable"
    );
    let lines = call("ecommerce_order_line").await;
    assert_eq!(lines["value"].as_array().unwrap().len(), 1);
    assert_eq!(lines["value"][0]["quantity"], 2);
    assert_eq!(f.app.store.count("ecommerce_order"), 1);
}
