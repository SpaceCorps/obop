//! The control-plane login answers a store fault as one (UB2): a 503 with a time to wait, never a
//! 401 that tells the person their password was wrong, and never the store's own text.

use super::*;
use axum::body::Body;
use axum::http::Request as HttpRequest;
use tower::ServiceExt;

/// A control plane whose login answers with `answer`.
struct Plane {
    answer: &'static str,
}

impl PlatformApi for Plane {
    fn signup(&self, _: &str, _: &str) -> Result<String, String> {
        Err("no".into())
    }
    fn login(&self, _: &str, _: &str) -> Result<String, String> {
        Err(self.answer.into())
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
}

async fn login(answer: &'static str) -> (StatusCode, Option<String>, String) {
    let r = platform_router(Arc::new(Plane { answer }), None);
    let req = HttpRequest::builder()
        .method("POST")
        .uri("/api/platform/login")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"email":"a@example.com","password":"x"}"#))
        .unwrap();
    let resp = r.oneshot(req).await.unwrap();
    let status = resp.status();
    let wait = resp
        .headers()
        .get(axum::http::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 16)
        .await
        .unwrap();
    (status, wait, String::from_utf8_lossy(&bytes).to_string())
}

#[tokio::test]
async fn a_refused_read_of_the_account_is_a_503_and_never_a_wrong_password() {
    let (s, wait, body) =
        login("storage_unavailable: connection refused (db.internal:5432)").await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(wait.is_some(), "a time to wait");
    assert!(!body.contains("db.internal"), "no store text: {body}");
    let (s, _, body) = login("storage_busy: every connection is in use").await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(!body.contains("every connection"), "no store text: {body}");
    // A wrong password is still a 401, and the lockout a 429.
    assert_eq!(
        login("invalid email or password").await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login("too many failed attempts, try again later").await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
}
