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
