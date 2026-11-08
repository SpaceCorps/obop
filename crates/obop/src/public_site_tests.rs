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
    fixture_with(public, obop_http::public::PublicConfig::default()).await
}

async fn fixture_with(public: bool, cfg: obop_http::public::PublicConfig) -> Fixture {
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
        "obop-admin-{}-{}{}",
        std::process::id(),
        if public { "p" } else { "o" },
        if cfg.trust_proxy { "t" } else { "" }
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
        obop_http::public::router(state, dir.to_str(), site, cfg)
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

async fn ask(
    r: &axum::Router,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
) -> (u16, String, axum::http::HeaderMap) {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "shop.test");
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    send(r, req.body(Body::empty()).unwrap()).await
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

/// A back office build in a directory of its own, removed again when the guard is dropped.
struct Build(std::path::PathBuf);

impl Drop for Build {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn the_back_office_carries_the_default_language_where_it_is_mounted_and_nowhere_else() {
    const SHELL: &str =
        "<html><head><title>obop</title></head><body>BACKOFFICE-SHELL</body></html>";
    const TAG: &str = "<meta name=\"obop-default-language\" content=\"sv\">";
    const SCRIPT: &str = "console.log('admin')";
    let build = Build(std::env::temp_dir().join(format!("obop-admin-lang-{}", std::process::id())));
    std::fs::create_dir_all(build.0.join("assets")).unwrap();
    std::fs::write(build.0.join("index.html"), SHELL).unwrap();
    std::fs::write(build.0.join("assets/app.js"), SCRIPT).unwrap();
    for (public, language) in [
        (true, Some("sv")),
        (false, Some("sv")),
        (true, None),
        (false, None),
    ] {
        let app = App::new();
        let auth = Arc::new(Auth::with_test_params(app.store.backend()));
        let state = obop_http::HttpState::new(app.dispatcher.clone(), auth);
        state.set_default_language(language);
        let dir = build.0.to_str();
        let (router, mount) = if public {
            let site = Arc::new(obop_site::Site::new(
                obop_commands::PublicApi::new(app.dispatcher.clone(), app.store.clone()),
                obop_site::Config::default(),
            ));
            let cfg = obop_http::public::PublicConfig::default();
            (obop_http::public::router(state, dir, site, cfg), "/admin")
        } else {
            (obop_http::router(state, dir), "")
        };
        for path in [
            format!("{mount}/"),
            format!("{mount}/index.html"),
            format!("{mount}/shop/orders/42"),
        ] {
            let (s, body, h) = get(&router, &path).await;
            assert_eq!(s, 200, "public={public} {path}");
            match language {
                Some(_) => {
                    assert_eq!(
                        body.matches(TAG).count(),
                        1,
                        "public={public} {path}: {body}"
                    );
                    assert_eq!(body.matches("obop-default-language").count(), 1);
                }
                None => assert_eq!(body, SHELL, "public={public} {path}"),
            }
            assert_eq!(h["cache-control"], "no-cache", "public={public} {path}");
        }
        let (s, js, _) = get(&router, &format!("{mount}/assets/app.js")).await;
        assert_eq!((s, js.as_str()), (200, SCRIPT));
        // A script keeps its 304, 206 and HEAD, with or without a default language.
        let script = format!("{mount}/assets/app.js");
        let (_, _, h) = ask(&router, "GET", &script, &[]).await;
        let modified = h["last-modified"].to_str().unwrap().to_string();
        let (s, body, _) = ask(&router, "GET", &script, &[("if-modified-since", &modified)]).await;
        assert_eq!((s, body.as_str()), (304, ""), "public={public} {script}");
        let (s, body, h) = ask(&router, "GET", &script, &[("range", "bytes=0-9")]).await;
        assert_eq!((s, body.as_str()), (206, &SCRIPT[..10]), "public={public}");
        assert_eq!(
            h["content-range"].to_str().unwrap(),
            format!("bytes 0-9/{}", SCRIPT.len())
        );
        let (s, body, h) = ask(&router, "HEAD", &script, &[]).await;
        assert_eq!((s, body.as_str()), (200, ""), "public={public} {script}");
        assert_eq!(
            h["content-length"].to_str().unwrap(),
            SCRIPT.len().to_string()
        );
        assert_eq!(h["last-modified"].to_str().unwrap(), modified);
        // The shell: a copy that claims to be current is a 304 only where the server adds nothing.
        let shell_path = format!("{mount}/");
        let current = [("if-modified-since", "Fri, 01 Jan 2100 00:00:00 GMT")];
        let (s, body, _) = ask(&router, "GET", &shell_path, &current).await;
        match language {
            Some(_) => {
                assert_eq!(
                    s, 200,
                    "public={public}: the shell is never a 304 without its tag"
                );
                assert_eq!(body.matches(TAG).count(), 1, "public={public}: {body}");
            }
            None => assert_eq!((s, body.as_str()), (304, ""), "public={public}"),
        }
        let (s, body, _) = ask(&router, "GET", &shell_path, &[("range", "bytes=0-9")]).await;
        match language {
            Some(_) => {
                assert_eq!(
                    s, 200,
                    "public={public}: the shell is never a 206 without its tag"
                );
                assert_eq!(body.matches(TAG).count(), 1, "public={public}: {body}");
            }
            None => assert_eq!((s, body.as_str()), (206, &SHELL[..10]), "public={public}"),
        }
        if public {
            // The public site is not the back office and never carries the tag.
            let (_, home, _) = get(&router, "/").await;
            assert!(!home.contains("obop-default-language"), "{home}");
            assert!(!home.contains("BACKOFFICE-SHELL"));
        }
        // An unknown API path is the JSON 404, not the shell.
        let (s, body, _) = get(&router, "/api/nothing").await;
        assert_eq!(s, 404);
        assert!(!body.contains("obop-default-language"));
    }
}

/// How many times `GET /shop/pay/x` is answered before the site's per-address limit (60 in ten
/// minutes) answers 429, for requests that carry the given `X-Forwarded-For` lines. A path with a
/// token that cannot exist is counted under the address before it is looked up (a 404), so this
/// reads which address the router handed to the site without any order.
async fn hits_until_limited(f: &Fixture, lines: &[&str]) -> usize {
    for n in 0..70 {
        let mut b = Request::get("/shop/pay/x").header("host", "shop.test");
        for l in lines {
            b = b.header("x-forwarded-for", *l);
        }
        let (s, _, _) = send(&f.router, b.body(Body::empty()).unwrap()).await;
        match s {
            404 => {}
            429 => return n,
            other => panic!("unexpected {other}"),
        }
    }
    panic!("no limit within 70 requests");
}

#[tokio::test]
async fn the_public_router_hands_the_site_the_last_forwarded_address_only_behind_a_trusted_proxy() {
    // Behind a trusted proxy the limit is per the proxy's entry, the last: a client that varies
    // what it wrote before it (the first entry, or a first line) stays in one bucket, and one that
    // is behind another address has its own.
    let trusted = fixture_with(
        true,
        obop_http::public::PublicConfig {
            trust_proxy: true,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(
        hits_until_limited(&trusted, &["1.1.1.1, 9.9.9.9"]).await,
        60,
        "the first bucket takes its 60"
    );
    for lines in [
        &["2.2.2.2, 9.9.9.9"][..],
        &["3.3.3.3", "4.4.4.4, 9.9.9.9"][..],
        &["9.9.9.9:51000"][..],
    ] {
        let (s, _, _) = send(
            &trusted.router,
            lines
                .iter()
                .fold(
                    Request::get("/shop/pay/x").header("host", "shop.test"),
                    |b, l| b.header("x-forwarded-for", *l),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(
            s, 429,
            "{lines:?} is the same address as the proxy's entry 9.9.9.9"
        );
    }
    assert_eq!(
        hits_until_limited(&trusted, &["1.1.1.1, 8.8.8.8"]).await,
        60,
        "another proxy entry is another bucket, whatever the client wrote first"
    );

    // An entry that is not an address is the peer, as if there were no header: varying it does not
    // give the client a bucket of its own (in the test rig there is no socket, so the peer is
    // nothing; the point is that both spellings meet in one bucket).
    let first = fixture_with(
        true,
        obop_http::public::PublicConfig {
            trust_proxy: true,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(hits_until_limited(&first, &["not-an-address-a"]).await, 60);
    assert_eq!(
        hits_until_limited(&first, &["not-an-address-b"]).await,
        0,
        "a made-up entry does not choose the bucket"
    );
    assert_eq!(
        hits_until_limited(&first, &["1.1.1.1,"]).await,
        0,
        "an empty last entry does not choose the bucket"
    );

    // Without a trusted proxy nothing the client wrote counts: one bucket for every spelling.
    let plain = fixture(true).await;
    assert_eq!(hits_until_limited(&plain, &["5.5.5.5"]).await, 60);
    assert_eq!(
        hits_until_limited(&plain, &["6.6.6.6"]).await,
        0,
        "the header is ignored unless the operator said the proxy is theirs"
    );
}

// ---- Golden pins of the logo address (S1a-0 of the branding work) --------------------------------

/// `/api/brand/logo.svg` with no brand row, in the two other layouts the router tests build: the
/// back office build served next to the API (`static`) and the instance with the public site
/// (`public`). Slice S1b added the route and moved these rows on purpose; the first commit of the
/// work held the answers while no route owned the path (the generic command route matched it for a
/// `POST` only):
///
/// - `GET` was a `405` with `Allow: POST` and no body (75580546...); it is now the answer of the
///   layout to an unknown path of the API, since there is no brand row: the JSON with a code and a
///   hint beside the back office (fd0f798e...), the short JSON of the public site's router
///   (23fee73b...).
/// - `POST` was a `401` without a token (31f8cb6f...) and the `404` of a command that does not
///   exist with the owner's token (23fee73b...); it is now the `405` of the route that owns the
///   path in both layouts, with `Allow: GET,HEAD` and no body (39252dd2...).
///
/// The layout with the API alone is pinned in `mcp_tests`.
#[tokio::test]
async fn pins_what_the_logo_address_answers_in_the_back_office_and_public_site_layouts() {
    use crate::mfa_tests::pin;
    let mut now = Vec::new();
    for (layout, public) in [("static", false), ("public", true)] {
        let f = fixture(public).await;
        for (name, method, token) in [
            ("get", "GET", None),
            ("post", "POST", None),
            ("post_with_token", "POST", Some(f.token.as_str())),
        ] {
            now.push((
                format!("{layout}/{name}"),
                pin::sha(&pin::exchange_as(&f.router, method, "/api/brand/logo.svg", token).await),
            ));
        }
    }
    pin::check("LOGO_ADDRESS_LAYOUT_PINS", LOGO_ADDRESS_LAYOUT_PINS, &now);
}

// Printed by `OBOP_PRINT_PINS=1` on origin/main 12a59a79 and pasted here; the six rows were printed
// again on the branch of slice S1b (the route exists, no brand row), the digests they had are in the
// comment above.
#[rustfmt::skip]
const LOGO_ADDRESS_LAYOUT_PINS: &[(&str, &str)] = &[
    ("static/get", "fd0f798e1d7f498e3d2d38a0c65f7cfbf2875753e6387a380404fdc9f0ca69b4"),
    ("static/post", "39252dd2a34eca81eb92a358f89d76949b1d25f4060f41e7ee88ab4477df73e1"),
    ("static/post_with_token", "39252dd2a34eca81eb92a358f89d76949b1d25f4060f41e7ee88ab4477df73e1"),
    ("public/get", "23fee73b6a79601cb87a949221e7445d2fd143719ef46f3cb185a6b15465c5c3"),
    ("public/post", "39252dd2a34eca81eb92a358f89d76949b1d25f4060f41e7ee88ab4477df73e1"),
    ("public/post_with_token", "39252dd2a34eca81eb92a358f89d76949b1d25f4060f41e7ee88ab4477df73e1"),
];

// ---- With a brand row (S1a of the branding work) --------------------------------------------------

/// One answer as one text: the status, every header sorted by name, a blank line and the body.
async fn shown(r: &axum::Router, path: &str) -> String {
    let (status, body, headers) = get(r, path).await;
    let mut lines: Vec<String> = headers
        .iter()
        .map(|(k, v)| format!("{k}: {}", String::from_utf8_lossy(v.as_bytes())))
        .collect();
    lines.sort();
    format!("{status}\n{}\n\n{body}", lines.join("\n"))
}

/// The brand row is in the store. The shop's files and pages go through `PublicApi::site_settings`
/// on every request here (the fixture's cache lives zero seconds), the one door the site reads
/// settings by, and the back office index is the file of the build. Since slice S1f the door fills
/// what the seeded settings leave unset (the logo address, the tab icon), so the home page and the
/// tab icon move with a row (`with_a_brand_row_the_shop_shows_the_logo_and_the_artwork`), and every
/// file that nothing of the brand feeds answers the bytes it answered without a row, headers
/// included: the stylesheet (the seed sets the accent, and an explicit key wins), the robots and
/// sitemap documents and the back office. The back office, with the public site alone, is not read
/// by anything yet.
#[tokio::test]
async fn with_a_brand_row_the_shop_files_the_brand_does_not_feed_and_the_back_office_index_are_what_they_were()
 {
    for (public, paths) in [
        (
            true,
            vec![
                "/assets/site.css",
                "/robots.txt",
                "/sitemap.xml",
                "/admin/",
                "/admin/index.html",
            ],
        ),
        (false, vec!["/", "/index.html", "/assets/app.js"]),
    ] {
        let f = fixture(public).await;
        let mut was = Vec::new();
        for p in &paths {
            was.push(shown(&f.router, p).await);
        }
        assert!(
            was[0].starts_with("200\n"),
            "{}: the first file is served: {}",
            paths[0],
            was[0]
        );
        crate::brand_row_tests::set_brand(&f.app);
        for (p, before) in paths.iter().zip(&was) {
            assert_eq!(
                &shown(&f.router, p).await,
                before,
                "{p} (public site: {public}) moved when a brand row was written"
            );
        }
    }
}

// ---- The shop falls back to the brand (S1f of the branding work) -------------------------------

use obop_brand::logo::{LOGO_CACHE_CONTROL, LOGO_CONTENT_TYPE, LOGO_CSP, LOGO_NOSNIFF};
use obop_storage::testing::{Flaky, Mode};

/// The headers of the logo route that the shop's tab icon must carry, by name.
const LOGO_HEADER_NAMES: [&str; 5] = [
    "content-type",
    "x-content-type-options",
    "content-security-policy",
    "cache-control",
    "etag",
];

/// Remove the artwork of the brand that `set_brand` wrote, through the command: the brand keeps its
/// name and the logo is the generated tile.
fn drop_the_artwork(app: &crate::app::App) {
    let reader = app.dispatcher.brand();
    obop_commands::brand::apply(
        &*app.store.backend(),
        Some(&reader),
        "operator",
        &obop_commands::brand::Action::Configure(vec![("logo_svg".into(), String::new())]),
        false,
    )
    .unwrap();
}

/// With a brand row the seeded shop (it sets a title, a tagline, an accent and a footer, and no
/// logo address) keeps what it set, takes the logo route for its logo and shows the artwork on the
/// tab icon address, with the bytes and the headers of the logo route itself.
#[tokio::test]
async fn with_a_brand_row_the_shop_shows_the_logo_and_the_artwork() {
    let f = fixture(true).await;
    let (_, home_before, _) = get(&f.router, "/").await;
    assert!(!home_before.contains("/api/brand/logo.svg"));
    crate::brand_row_tests::set_brand(&f.app);
    let (s, logo, logo_headers) = get(&f.router, "/api/brand/logo.svg").await;
    assert_eq!(s, 200);
    let v = logo_headers["etag"]
        .to_str()
        .unwrap()
        .trim_matches('"')
        .to_string();

    let (s, home, _) = get(&f.router, "/").await;
    assert_eq!(s, 200);
    // The owner's title wins over the name; the logo address was unset, so the record's is there.
    assert!(
        home.contains("Fernhill Goods") && !home.contains("MOHV"),
        "{home}"
    );
    assert!(
        home.contains(&format!(
            "<img src=\"/api/brand/logo.svg?v={v}\" alt=\"\" height=\"34\">"
        )),
        "{home}"
    );
    // The tab icon is the artwork, answered as the logo route answers it.
    for path in ["/favicon.svg", "/favicon.ico"] {
        let (s, body, h) = get(&f.router, path).await;
        assert_eq!(s, 200, "{path}");
        assert_eq!(body, logo, "{path}: the bytes of the logo route");
        for name in LOGO_HEADER_NAMES {
            assert_eq!(h[name], logo_headers[name], "{path}: {name}");
        }
        assert_eq!(h["content-type"], LOGO_CONTENT_TYPE);
        assert_eq!(h["x-content-type-options"], LOGO_NOSNIFF);
        assert_eq!(h["content-security-policy"], LOGO_CSP);
        assert_eq!(h["cache-control"], LOGO_CACHE_CONTROL);
        assert!(h.get("set-cookie").is_none() && h.get("x-frame-options").is_none());
    }
    // A matching validator is a 304 on the tab icon, as on the logo route.
    let tag = format!("\"{v}\"");
    let (s, body, h) = ask(&f.router, "GET", "/favicon.svg", &[("if-none-match", &tag)]).await;
    assert_eq!((s, body.as_str()), (304, ""));
    assert_eq!(h["etag"], tag.as_str());
    assert!(h.get("content-type").is_none());

    // A brand with no artwork: the shop keeps its own tile (the tile of the brand is not its tab icon).
    drop_the_artwork(&f.app);
    let (_, icon, h) = get(&f.router, "/favicon.svg").await;
    assert!(icon.contains("<text"), "the shop's own tile: {icon}");
    assert_ne!(h["content-security-policy"], LOGO_CSP);
}

/// What a client of a socket reads: the status line, the header lines (the names in lower case, as
/// the server sends them) and the body.
struct Wire {
    status: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Wire {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// The names of the headers, sorted, without `date` and `connection`, which the server sets.
    fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .headers
            .iter()
            .map(|(k, _)| k.as_str())
            .filter(|k| !matches!(*k, "date" | "connection"))
            .collect();
        names.sort_unstable();
        names
    }
}

/// The router served on a loopback socket the way `obop_http::serve` serves it.
async fn on_a_socket(router: axum::Router) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    addr
}

/// One request over TCP, read to the end of the connection (it says `Connection: close`), with a
/// bound on every read, so that a server that does not answer fails the test and does not hang it.
async fn on_the_wire(
    addr: std::net::SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
) -> Wire {
    use std::io::{Read, Write};
    const CRLF: &str = "\r\n";
    let mut request =
        format!("{method} {path} HTTP/1.1{CRLF}Host: shop.test{CRLF}Connection: close{CRLF}");
    for (k, v) in headers {
        request.push_str(&format!("{k}: {v}{CRLF}"));
    }
    request.push_str(CRLF);
    tokio::task::spawn_blocking(move || {
        let mut s = std::net::TcpStream::connect(addr).unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(20)))
            .unwrap();
        s.write_all(request.as_bytes()).unwrap();
        let mut raw = Vec::new();
        s.read_to_end(&mut raw).expect("the server answered");
        let split = raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .expect("a response head");
        let head = String::from_utf8(raw[..split].to_vec()).unwrap();
        let mut lines = head.split(CRLF);
        let status = lines.next().unwrap().to_string();
        let headers = lines
            .map(|l| {
                let (k, v) = l.split_once(':').expect("a header line");
                (k.trim().to_ascii_lowercase(), v.trim().to_string())
            })
            .collect();
        Wire {
            status,
            headers,
            body: raw[split + 4..].to_vec(),
        }
    })
    .await
    .unwrap()
}

/// What a client sees of the tab icon's 304 over a real socket, because a router called without a
/// server shows the headers the handler set and not the ones the server sends. The shop sets the
/// length of the 200 on its 304 as the logo route does; the server sends it with a `HEAD` 304 and
/// leaves it out of a `GET` 304 (RFC 9110 section 8.6 allows both). Without it a `HEAD` 304 says
/// `Content-Length: 0`, which says the artwork is empty. The headers of the tab icon are the logo
/// route's, on a 200, a `HEAD` 200, a `GET` 304 and a `HEAD` 304, name for name.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_tab_icons_304_on_the_wire_is_the_logo_routes_and_a_head_304_carries_the_length() {
    let f = fixture(true).await;
    crate::brand_row_tests::set_brand(&f.app);
    let addr = on_a_socket(f.router.clone()).await;
    let logo = on_the_wire(addr, "GET", "/api/brand/logo.svg", &[]).await;
    assert_eq!(logo.status, "HTTP/1.1 200 OK");
    let length = logo.body.len().to_string();
    let etag = logo.header("etag").expect("an ETag").to_string();
    let revalidating = [("if-none-match", etag.as_str())];
    for path in ["/favicon.svg", "/favicon.ico"] {
        for (method, extra, status, has_length, has_body) in [
            ("GET", &[][..], "HTTP/1.1 200 OK", true, true),
            ("HEAD", &[][..], "HTTP/1.1 200 OK", true, false),
            (
                "GET",
                &revalidating[..],
                "HTTP/1.1 304 Not Modified",
                false,
                false,
            ),
            (
                "HEAD",
                &revalidating[..],
                "HTTP/1.1 304 Not Modified",
                true,
                false,
            ),
        ] {
            let what = format!("{method} {path} {extra:?}");
            let icon = on_the_wire(addr, method, path, extra).await;
            let route = on_the_wire(addr, method, "/api/brand/logo.svg", extra).await;
            assert_eq!(icon.status, status, "{what}");
            assert_eq!(
                icon.body.is_empty(),
                !has_body,
                "{what}: {}",
                String::from_utf8_lossy(&icon.body)
            );
            if has_body {
                assert_eq!(icon.body, logo.body, "{what}: the bytes of the logo route");
            }
            assert_eq!(
                icon.header("content-length"),
                has_length.then_some(length.as_str()),
                "{what}: the length of the 200 where the server sends one"
            );
            assert_eq!(icon.header("etag"), Some(etag.as_str()), "{what}");
            assert_eq!(
                icon.names(),
                route.names(),
                "{what}: the headers of the logo route on the wire"
            );
            assert_eq!(route.status, status, "{what}: the logo route");
        }
    }
}

/// `setting.update` writes any key of the `site` object. A key with the reserved name that an
/// administrator writes through the real command reaches no page and not the tab icon, with a brand
/// that has an artwork, with one that has none, and with none.
#[tokio::test]
async fn a_reserved_key_written_through_setting_update_reaches_no_page_and_not_the_tab_icon() {
    let hostile = r#"<svg xmlns="http://www.w3.org/2000/svg" onload="alert(1)"><rect width="9" height="9"/></svg>"#;
    let write = |key: bool| {
        let mut site = json!({"title": "Fernhill Goods", "accent": "#2d6a4f"});
        if key {
            site[obop_commands::SITE_FAVICON_KEY] =
                json!({"svg": hostile, "v": "0123456789abcdef"});
        }
        json!({"value": json!({"site": site}).to_string()})
    };
    let files = |f: &Fixture| {
        let router = f.router.clone();
        async move {
            let mut out = Vec::new();
            for p in [
                "/",
                "/favicon.svg",
                "/favicon.ico",
                "/assets/site.css",
                "/feed.xml",
            ] {
                out.push(shown(&router, p).await);
            }
            out.join("\n====\n")
        }
    };
    let post = |f: &Fixture, body: J| {
        let router = f.router.clone();
        let token = f.token.clone();
        async move {
            let (s, b, _) = send(
                &router,
                Request::post("/api/setting/update")
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await;
            assert_eq!(s, 200, "{b}");
        }
    };

    // No brand.
    let f = fixture(true).await;
    post(&f, write(false)).await;
    let clean = files(&f).await;
    post(&f, write(true)).await;
    assert_eq!(files(&f).await, clean, "no brand: the key changes nothing");
    assert!(!clean.contains("onload"));

    // A brand with an artwork: the tab icon is the logo route's answer, never the stored object.
    crate::brand_row_tests::set_brand(&f.app);
    let (_, logo, _) = get(&f.router, "/api/brand/logo.svg").await;
    for p in ["/favicon.svg", "/favicon.ico"] {
        let (s, body, _) = get(&f.router, p).await;
        assert_eq!((s, body.as_str()), (200, logo.as_str()), "{p}");
    }
    assert!(!files(&f).await.contains("onload"));
    post(&f, write(false)).await;
    let (_, without, _) = get(&f.router, "/favicon.svg").await;
    assert_eq!(without, logo);
    post(&f, write(true)).await;

    // A brand with no artwork: the shop's own tile, with the key or without it.
    drop_the_artwork(&f.app);
    let with_key = files(&f).await;
    post(&f, write(false)).await;
    assert_eq!(files(&f).await, with_key);
    assert!(with_key.contains("<text") && !with_key.contains("onload"));
}

// ---- A store that fails under the shop (D74) --------------------------------------------------------

/// The seeded shop over a store behind a double that refuses reads of a model on demand, with the
/// site's own cache lived zero seconds so that every request asks the door.
async fn flaky_shop() -> (Arc<Flaky>, crate::app::App, axum::Router) {
    flaky_shop_keeping(std::time::Duration::ZERO).await
}

/// The same with the site's own cache kept for `ttl` (production keeps it 5 seconds): what the shop
/// computed from a read that failed must not be what it serves after the store is back.
async fn flaky_shop_keeping(
    ttl: std::time::Duration,
) -> (Arc<Flaky>, crate::app::App, axum::Router) {
    let (flaky, backend) = Flaky::memory();
    let app = crate::app::App::with_store(obop_commands::Store::over(backend));
    for id in ["website", "ecommerce"] {
        app.installer
            .install(&Bundled.fetch(id).unwrap(), "marketplace", "test")
            .unwrap();
    }
    obop_site::seed::seed(&app.store, &app.dispatcher).unwrap();
    let auth = Arc::new(Auth::with_test_params(app.store.backend()));
    let state = obop_http::HttpState::new(app.dispatcher.clone(), auth);
    let site = Arc::new(obop_site::Site::new(
        obop_commands::PublicApi::new(app.dispatcher.clone(), app.store.clone()),
        obop_site::Config {
            settings_ttl: ttl,
            ..obop_site::Config::default()
        },
    ));
    let router = obop_http::public::router(
        state,
        None,
        site,
        obop_http::public::PublicConfig::default(),
    );
    (flaky, app, router)
}

/// The bound of the reader's cell (`brand/reader.rs`, two seconds), as the tests of the logo route
/// hold it: the dispatcher's reader has this bound and a test of this crate cannot shorten it.
const READER_BOUND: std::time::Duration = obop_commands::brand::DEFAULT_BOUND;

fn wait_until_stale(filled: std::time::Instant) {
    let stale = READER_BOUND + std::time::Duration::from_millis(50);
    while filled.elapsed() <= stale {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// After a good read, a store that then fails: once the reader's cell has run out, the request that
/// runs the refill meets the fault and is answered by D74, 503 with `Retry-After`, and never by a
/// shop page that has lost its brand; nothing is kept, so the next request reads again; when the
/// store answers, the brand is back. (The reader serves the last good brand to a caller that does not
/// run the refill, `public/tests.rs` in `obop-commands`; the layer above refuses the request that
/// met the fault whatever the page would have been.)
#[tokio::test]
async fn after_a_good_read_a_failing_store_answers_the_shop_request_that_runs_the_refill_with_a_503()
 {
    let (flaky, app, router) = flaky_shop().await;
    crate::brand_row_tests::set_brand(&app);
    let (s, good, _) = get(&router, "/favicon.svg").await;
    assert_eq!(s, 200);
    assert!(good.contains("<svg"));
    let (_, home, _) = get(&router, "/").await;
    assert!(home.contains("/api/brand/logo.svg?v="), "{home}");
    let filled = std::time::Instant::now();

    flaky.refuse_model("instance_brand", Mode::Reads);
    wait_until_stale(filled);
    for _ in 0..2 {
        let before = flaky.refused();
        let (s, body, h) = get(&router, "/").await;
        assert_eq!(s, 503, "{body}");
        assert_eq!(h["retry-after"], "1");
        assert!(!body.contains("Fernhill Goods"), "not a shop page: {body}");
        assert!(
            flaky.refused() > before,
            "a failed refill is not kept: it read again"
        );
    }
    let (s, body, _) = get(&router, "/favicon.svg").await;
    assert_eq!(s, 503, "{body}");

    flaky.allow_models();
    let (s, home, _) = get(&router, "/").await;
    assert_eq!(s, 200);
    assert!(
        home.contains("/api/brand/logo.svg?v="),
        "the brand is back: {home}"
    );
    let (s, icon, _) = get(&router, "/favicon.svg").await;
    assert_eq!((s, icon.as_str()), (200, good.as_str()));
}

/// The brand row, written to the store around the reader of this process (as a restore, another
/// process or a direct edit would write it), so that the reader has read nothing yet: `set_brand`
/// ends by asking the reader, which then holds a good brand for its two seconds.
fn put_brand_around_the_reader(app: &crate::app::App) {
    let fields: obop_storage::Fields = crate::brand_row_tests::FIELDS
        .iter()
        .map(|(k, v)| (k.to_string(), J::String(v.to_string())))
        .collect();
    app.store
        .backend()
        .apply_checked(&[obop_storage::Op::Put {
            model: "instance_brand".into(),
            id: "instance".into(),
            fields,
        }])
        .unwrap();
}

/// A process that never had a good read of the brand: the request that runs the first refill meets
/// the fault and is answered by D74. Nothing is pinned, neither the failure nor the default, not even
/// by the site's own cache, which here keeps what it computes for an hour: when the store answers the
/// shop is what the store holds at once (the brand's, or, with no row, the shop as it always was).
#[tokio::test]
async fn a_shop_that_never_had_a_good_read_of_the_brand_is_answered_by_d74_and_pins_nothing() {
    // The shop's own tile, as another process that reads the store well sends it.
    let (_, _app, plain_router) = flaky_shop().await;
    let (_, plain, _) = get(&plain_router, "/favicon.svg").await;
    for branded in [false, true] {
        let (flaky, app, router) = flaky_shop_keeping(std::time::Duration::from_secs(3600)).await;
        if branded {
            put_brand_around_the_reader(&app);
        }
        flaky.refuse_model("instance_brand", Mode::Reads);
        let (s, body, h) = get(&router, "/").await;
        assert_eq!(s, 503, "branded: {branded}: {body}");
        assert_eq!(h["retry-after"], "1", "branded: {branded}");
        assert!(!body.contains("Fernhill Goods"), "not a shop page: {body}");
        let (s, body, _) = get(&router, "/favicon.svg").await;
        assert_eq!(s, 503, "branded: {branded}: {body}");
        let refused = flaky.refused();
        let (s, _, _) = get(&router, "/").await;
        assert_eq!(s, 503, "branded: {branded}: the failure is not kept");
        assert!(flaky.refused() > refused, "it read again");

        flaky.allow_models();
        let (s, home, _) = get(&router, "/").await;
        assert_eq!(s, 200, "branded: {branded}");
        assert!(home.contains("Fernhill Goods"), "{home}");
        let (s, icon, _) = get(&router, "/favicon.svg").await;
        assert_eq!(s, 200, "branded: {branded}");
        if branded {
            assert!(
                home.contains("/api/brand/logo.svg?v="),
                "the brand, not the default: {home}"
            );
            let (_, logo, _) = get(&router, "/api/brand/logo.svg").await;
            assert_eq!(icon, logo, "the artwork");
        } else {
            assert!(!home.contains("/api/brand/logo.svg"));
            assert_eq!(icon, plain, "the shop's own tile, as before");
        }
    }
}

/// The same for a settings row that cannot be read, with and without a brand: 503 for the request
/// that met the fault, and the owner's explicit keys (the seeded shop sets a title and an accent)
/// back as soon as the store answers, not after the site's cache time. The door reads this row
/// itself, before the reader does, so this is the door's own read.
#[tokio::test]
async fn a_settings_row_that_could_not_be_read_is_answered_by_d74_and_pins_nothing() {
    for branded in [false, true] {
        let (flaky, app, router) = flaky_shop_keeping(std::time::Duration::from_secs(3600)).await;
        if branded {
            crate::brand_row_tests::set_brand(&app);
        }
        let (_, plain, _) = {
            let (_, _app, plain_router) = flaky_shop().await;
            get(&plain_router, "/assets/site.css").await
        };
        flaky.refuse_model("obop_setting", Mode::Reads);
        let (s, body, h) = get(&router, "/").await;
        assert_eq!(s, 503, "branded: {branded}: {body}");
        assert_eq!(h["retry-after"], "1", "branded: {branded}");
        flaky.allow_models();
        let (s, home, _) = get(&router, "/").await;
        assert_eq!(s, 200, "branded: {branded}");
        assert!(
            home.contains("Fernhill Goods") && !home.contains("My Shop") && !home.contains("MOHV"),
            "branded: {branded}: the owner's title: {home}"
        );
        let (_, css, _) = get(&router, "/assets/site.css").await;
        assert_eq!(css, plain, "branded: {branded}: the owner's accent");
        // And it is kept now: a good answer is cached.
        let reads = flaky.refused();
        let (s, _, _) = get(&router, "/").await;
        assert_eq!((s, flaky.refused()), (200, reads));
    }
}
