//! The health routes of the control plane (I6): `/healthz` and `/readyz` on `platform_router`.

use super::*;
use axum::body::Body;
use axum::http::Request as HttpRequest;
use std::sync::atomic::{AtomicBool, Ordering};
use tower::ServiceExt;

#[derive(Default)]
struct Plane {
    down: AtomicBool,
}

impl PlatformApi for Plane {
    fn signup(&self, _: &str, _: &str) -> Result<String, String> {
        Err("no".into())
    }
    fn login(&self, _: &str, _: &str) -> Result<String, String> {
        Err("no".into())
    }
    fn me(&self, _: &str) -> Result<String, String> {
        Err("no".into())
    }
    fn list(&self, _: &str) -> Result<Vec<J>, String> {
        Ok(Vec::new())
    }
    fn create(&self, _: &str, _: &str) -> Result<J, String> {
        Err("no".into())
    }
    fn delete(&self, _: &str, _: &str) -> Result<J, String> {
        Err("no".into())
    }
    fn ready(&self) -> Result<(), String> {
        if self.down.load(Ordering::Relaxed) {
            Err("connection refused (127.0.0.1:5432)".into())
        } else {
            Ok(())
        }
    }
}

async fn get_text(r: &Router, path: &str) -> (StatusCode, String) {
    let req = HttpRequest::builder()
        .uri(path)
        .body(Body::empty())
        .unwrap();
    let resp = r.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 16)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

#[tokio::test]
async fn the_control_plane_answers_healthz_and_readyz_with_its_database() {
    let plane = Arc::new(Plane::default());
    let r = platform_router(plane.clone(), None);
    assert_eq!(
        get_text(&r, "/healthz").await,
        (StatusCode::OK, "{\"status\":\"ok\"}".to_string())
    );
    assert_eq!(
        get_text(&r, "/readyz").await,
        (StatusCode::OK, "{\"status\":\"ready\"}".to_string())
    );
    plane.down.store(true, Ordering::Relaxed);
    let (s, body) = get_text(&r, "/readyz").await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert!(body.contains("database unreachable"), "{body}");
    assert!(!body.contains("127.0.0.1"), "no host in the answer: {body}");
    // The process itself still answers.
    assert_eq!(get_text(&r, "/healthz").await.0, StatusCode::OK);
}

async fn get_with(r: &Router, path: &str, bearer: Option<&str>) -> (StatusCode, String) {
    let mut b = HttpRequest::builder().uri(path);
    if let Some(t) = bearer {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    let resp = r
        .clone()
        .oneshot(b.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

#[tokio::test]
async fn the_control_plane_serves_metrics_to_whoever_the_operator_opened_them_to() {
    let plane = Arc::new(Plane::default());
    // Closed by default, as on an instance: the route answers as if it did not exist.
    let closed = platform_router(plane.clone(), None);
    assert_eq!(
        get_with(&closed, "/metrics", None).await.0,
        StatusCode::NOT_FOUND
    );
    let token = "plane-metrics-token-0123456789";
    let r = platform_router_with_metrics(plane.clone(), None, ops::MetricsAccess::token(token));
    assert_eq!(
        get_with(&r, "/metrics", None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        get_with(&r, "/metrics", Some("wrong-token-0123456789"))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    // Two answered requests before the scrape are counted.
    get_text(&r, "/healthz").await;
    get_text(&r, "/readyz").await;
    let (s, body) = get_with(&r, "/metrics", Some(token)).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    for series in [
        "obop_build_info{core_version=",
        "obop_process_uptime_seconds ",
        "obop_http_request_duration_seconds_count ",
        "obop_db_up 1",
        "obop_db_ping_seconds ",
    ] {
        assert!(body.contains(series), "{series} missing:\n{body}");
    }
    let twoxx: u64 = body
        .lines()
        .find_map(|l| l.strip_prefix("obop_http_requests_total{class=\"2xx\"} "))
        .and_then(|n| n.parse().ok())
        .unwrap();
    assert!(twoxx >= 2, "{body}");
    plane.down.store(true, Ordering::Relaxed);
    let (_, body) = get_with(&r, "/metrics", Some(token)).await;
    assert!(body.contains("obop_db_up 0"), "{body}");
    assert!(
        !body.contains("127.0.0.1"),
        "no host in the metrics: {body}"
    );
}
