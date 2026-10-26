//! Cross-origin access to `/api` (D68) against a real `App`: off by default, exact allow-list,
//! correct preflight, no credentials, and the OAuth pages untouched.

use crate::app::{App, DevAuth};
use axum::{
    body::Body,
    http::{HeaderMap, Request},
};
use http_body_util::BodyExt;
use obop_http::mcp::McpConfig;
use std::sync::Arc;
use tower::ServiceExt;

const APP: &str = "https://app.example.com";
const DEV: &str = "http://localhost:5173";

struct Resp {
    status: u16,
    headers: HeaderMap,
}

impl Resp {
    fn h(&self, name: &str) -> &str {
        self.headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    }
    fn has(&self, name: &str) -> bool {
        self.headers.contains_key(name)
    }
    fn vary(&self) -> String {
        self.headers
            .get_all("vary")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .collect::<Vec<_>>()
            .join(", ")
            .to_lowercase()
    }
    fn has_any_cors_header(&self) -> bool {
        self.headers
            .keys()
            .any(|k| k.as_str().starts_with("access-control-"))
    }
}

fn router(origins: &[&str]) -> axum::Router {
    let app = App::new();
    app.seed_demo();
    obop_http::router(
        obop_http::HttpState::with_mcp(
            app.dispatcher.clone(),
            Arc::new(DevAuth),
            McpConfig {
                allowed_origins: origins.iter().map(|s| s.to_string()).collect(),
                ..McpConfig::default()
            },
        ),
        None,
    )
}

async fn send(r: &axum::Router, req: Request<Body>) -> Resp {
    let resp = r.clone().oneshot(req).await.unwrap();
    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    let _ = resp.into_body().collect().await;
    Resp { status, headers }
}

fn get(path: &str, origin: Option<&str>) -> Request<Body> {
    let mut r = Request::get(path).header("authorization", "Bearer dev-admin");
    if let Some(o) = origin {
        r = r.header("origin", o);
    }
    r.body(Body::empty()).unwrap()
}

fn preflight(path: &str, origin: &str, method: &str, headers: Option<&str>) -> Request<Body> {
    let mut r = Request::options(path)
        .header("origin", origin)
        .header("access-control-request-method", method);
    if let Some(h) = headers {
        r = r.header("access-control-request-headers", h);
    }
    r.body(Body::empty()).unwrap()
}

#[tokio::test]
async fn by_default_no_origin_gets_any_cors_header_and_preflights_are_not_answered() {
    let r = router(&[]);
    let plain = send(&r, get("/api/commands", None)).await;
    let cross = send(&r, get("/api/commands", Some(APP))).await;
    assert_eq!(
        (plain.status, cross.status),
        (200, 200),
        "requests still work"
    );
    assert!(!cross.has_any_cors_header());
    assert!(
        !cross.vary().contains("origin"),
        "headers are exactly as before"
    );
    let pre = send(
        &r,
        preflight("/api/commands", APP, "GET", Some("authorization")),
    )
    .await;
    assert!(
        pre.status >= 400,
        "a preflight is not granted: {}",
        pre.status
    );
    assert!(!pre.has_any_cors_header());
}

#[tokio::test]
async fn a_listed_origin_gets_its_own_origin_back_and_never_credentials() {
    let r = router(&[APP, DEV]);
    for origin in [APP, DEV] {
        let resp = send(&r, get("/api/commands", Some(origin))).await;
        assert_eq!(resp.status, 200);
        assert_eq!(resp.h("access-control-allow-origin"), origin);
        assert!(resp.vary().contains("origin"));
        let exposed = resp.h("access-control-expose-headers");
        for name in [
            "Link",
            "Retry-After",
            "X-Obop-Applied",
            "WWW-Authenticate",
            "X-Request-Id",
            "ETag",
            "Idempotent-Replayed",
        ] {
            assert!(exposed.contains(name), "{name} in {exposed}");
        }
        assert!(
            !resp.has("access-control-allow-credentials"),
            "bearer tokens only: no cookies, no credentialed requests"
        );
    }
}

#[tokio::test]
async fn an_origin_that_is_not_listed_gets_no_cors_headers_but_a_caching_hint() {
    let r = router(&[APP]);
    for origin in [
        "https://evil.example",
        "https://app.example.com.evil.test",
        "https://evil.app.example.com",
        "http://app.example.com",
        "https://app.example.com:8443",
        "https://APP.example.com",
        "null",
    ] {
        let resp = send(&r, get("/api/commands", Some(origin))).await;
        assert!(
            !resp.has_any_cors_header(),
            "{origin} must get nothing: {:?}",
            resp.headers
        );
        assert!(resp.vary().contains("origin"));
    }
    // And a caller with no Origin header (a script, curl, an agent) is untouched.
    let resp = send(&r, get("/api/commands", None)).await;
    assert_eq!(resp.status, 200);
    assert!(!resp.has_any_cors_header());
}

#[tokio::test]
async fn the_origin_is_never_reflected_from_the_request() {
    let r = router(&[APP]);
    for origin in [
        "https://anything.test",
        "*",
        "https://app.example.com/",
        " https://app.example.com",
    ] {
        let resp = send(&r, get("/api/commands", Some(origin))).await;
        assert_ne!(resp.h("access-control-allow-origin"), origin, "{origin}");
        assert_ne!(resp.h("access-control-allow-origin"), "*");
    }
}

#[tokio::test]
async fn a_preflight_from_a_listed_origin_is_answered_with_methods_headers_and_max_age() {
    let r = router(&[APP]);
    let p = send(
        &r,
        preflight(
            "/api/commands",
            APP,
            "POST",
            Some("Authorization, Content-Type"),
        ),
    )
    .await;
    assert_eq!(p.status, 204);
    assert_eq!(p.h("access-control-allow-origin"), APP);
    assert_eq!(p.h("access-control-allow-methods"), "GET, POST, OPTIONS");
    assert_eq!(
        p.h("access-control-allow-headers"),
        "Authorization, Content-Type, Accept, Accept-Language, Idempotency-Key, If-Match, X-Request-Id, X-App-Version"
    );
    assert_eq!(p.h("access-control-max-age"), "600");
    assert!(!p.has("access-control-allow-credentials"));
    let vary = p.vary();
    assert!(
        vary.contains("origin")
            && vary.contains("access-control-request-method")
            && vary.contains("access-control-request-headers"),
        "{vary}"
    );
    // Every API path is covered, including ones with parameters and the blob upload.
    for path in [
        "/api/sales_order/create",
        "/api/blobs",
        "/api/auth/login",
        "/api/meta",
    ] {
        let p = send(&r, preflight(path, APP, "POST", Some("authorization"))).await;
        assert_eq!(p.status, 204, "{path}");
    }
}

#[tokio::test]
async fn a_preflight_for_the_headers_of_retries_and_concurrency_control_is_granted() {
    let r = router(&[APP]);
    let p = send(
        &r,
        preflight(
            "/api/sales_order/create",
            APP,
            "POST",
            Some("Authorization, Idempotency-Key, If-Match, X-Request-Id, X-App-Version"),
        ),
    )
    .await;
    assert_eq!(p.status, 204);
    for name in [
        "idempotency-key",
        "if-match",
        "x-request-id",
        "x-app-version",
    ] {
        assert!(
            p.h("access-control-allow-headers")
                .to_ascii_lowercase()
                .contains(name),
            "{name}"
        );
    }
    // The list is exact: a header that is not on it is still refused.
    let p = send(
        &r,
        preflight(
            "/api/x/create",
            APP,
            "POST",
            Some("Idempotency-Key, X-Api-Key"),
        ),
    )
    .await;
    assert_eq!(p.status, 403);
    assert!(!p.has_any_cors_header());
}

#[tokio::test]
async fn a_preflight_from_an_unlisted_origin_is_refused_without_cors_headers() {
    let r = router(&[APP]);
    let p = send(
        &r,
        preflight(
            "/api/commands",
            "https://evil.example",
            "GET",
            Some("authorization"),
        ),
    )
    .await;
    assert_eq!(p.status, 403);
    assert!(!p.has_any_cors_header());
    assert!(p.vary().contains("origin"));
}

#[tokio::test]
async fn a_preflight_asking_for_an_unsupported_method_or_header_is_refused() {
    let r = router(&[APP]);
    for (m, h) in [
        ("DELETE", None),
        ("PUT", Some("authorization")),
        ("POST", Some("x-api-key")),
        ("GET", Some("authorization, x-custom")),
    ] {
        let p = send(&r, preflight("/api/commands", APP, m, h)).await;
        assert_eq!(p.status, 403, "{m} {h:?}");
        assert!(!p.has_any_cors_header(), "{m} {h:?}");
    }
}

#[tokio::test]
async fn an_options_request_without_the_preflight_header_is_not_a_preflight() {
    let r = router(&[APP]);
    let resp = send(
        &r,
        Request::options("/api/commands")
            .header("origin", APP)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_ne!(resp.status, 204);
}

fn header_pairs(r: &Resp) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = r
        .headers
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    v.sort();
    v
}

#[tokio::test]
async fn the_oauth_and_consent_endpoints_answer_exactly_as_they_did_without_a_list() {
    let off = router(&[]);
    let on = router(&[APP]);
    type Make = Box<dyn Fn() -> Request<Body>>;
    let requests: Vec<(&str, Make)> = vec![
        (
            "authorize",
            Box::new(|| get("/oauth/authorize?response_type=code", Some(APP))),
        ),
        (
            "login",
            Box::new(|| {
                Request::post("/oauth/authorize/login")
                    .header("origin", APP)
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from("flow=x&username=a&password=b"))
                    .unwrap()
            }),
        ),
        (
            "decision",
            Box::new(|| {
                Request::post("/oauth/authorize/decision")
                    .header("origin", APP)
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from("flow=x&decision=allow"))
                    .unwrap()
            }),
        ),
        (
            "login preflight",
            Box::new(|| preflight("/oauth/authorize/login", APP, "POST", Some("content-type"))),
        ),
        (
            "decision preflight",
            Box::new(|| {
                preflight(
                    "/oauth/authorize/decision",
                    APP,
                    "POST",
                    Some("content-type"),
                )
            }),
        ),
        (
            "well-known",
            Box::new(|| get("/.well-known/oauth-authorization-server", Some(APP))),
        ),
        ("health", Box::new(|| get("/healthz", Some(APP)))),
    ];
    for (name, make) in requests {
        let (a, b) = (send(&off, make()).await, send(&on, make()).await);
        assert_eq!(a.status, b.status, "{name}");
        assert_eq!(header_pairs(&a), header_pairs(&b), "{name}");
        assert!(!b.has("access-control-allow-credentials"), "{name}");
    }
    // The consent pages themselves never name the origin.
    let page = send(&on, get("/oauth/authorize?response_type=code", Some(APP))).await;
    assert_ne!(page.h("access-control-allow-origin"), APP);
}

#[tokio::test]
async fn the_mcp_endpoint_keeps_using_the_same_list() {
    let r = router(&[APP]);
    let ok = send(
        &r,
        Request::options("/mcp")
            .header("origin", APP)
            .header("access-control-request-method", "POST")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(ok.status, 204);
    assert_eq!(ok.h("access-control-allow-origin"), APP);
    let no = send(
        &r,
        Request::options("/mcp")
            .header("origin", "https://evil.example")
            .header("access-control-request-method", "POST")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(no.status, 403);
}

#[tokio::test]
async fn a_hand_built_list_cannot_allow_more_than_the_rules_do() {
    // Wildcards and plain-http remote origins handed straight to the state are dropped.
    let r = router(&["*", "http://evil.example", "https://", APP]);
    for origin in ["https://evil.example", "http://evil.example", "*"] {
        let resp = send(&r, get("/api/commands", Some(origin))).await;
        assert!(!resp.has("access-control-allow-origin"), "{origin}");
    }
    let resp = send(&r, get("/api/commands", Some(APP))).await;
    assert_eq!(resp.h("access-control-allow-origin"), APP);
}

#[tokio::test]
async fn the_public_site_router_applies_cors_to_api_paths_only() {
    let app = App::new();
    app.seed_demo();
    let state = obop_http::HttpState::with_mcp(
        app.dispatcher.clone(),
        Arc::new(DevAuth),
        McpConfig {
            allowed_origins: vec![APP.into()],
            ..McpConfig::default()
        },
    );
    let site = Arc::new(obop_site::Site::new(
        obop_commands::PublicApi::new(app.dispatcher.clone(), app.store.clone()),
        obop_site::Config::default(),
    ));
    let r = obop_http::public::router(
        state,
        None,
        site,
        obop_http::public::PublicConfig::default(),
    );
    let api = send(&r, get("/api/commands", Some(APP))).await;
    assert_eq!(api.h("access-control-allow-origin"), APP);
    let page = send(&r, get("/", Some(APP))).await;
    assert!(!page.has("access-control-allow-origin"));
    let pre = send(
        &r,
        preflight("/api/commands", APP, "GET", Some("authorization")),
    )
    .await;
    assert_eq!(pre.status, 204);
}
