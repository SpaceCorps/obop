//! Mounts the server-rendered forum (`/community`) and status page (`/status`) of
//! `obop-community` on an axum router. This is the only place the framework-neutral
//! `Req`/`Resp` of that crate meet axum (D29).
//!
//! Merge it into the control-plane router; its routes are exact prefixes, so `/api/platform/*`
//! and the static fallback are unaffected:
//!
//! ```ignore
//! let router = obop_http::platform_router(api, static_dir).merge(obop_http::community::router(app));
//! ```

use super::blocking;
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Request, State},
    http::{HeaderName, HeaderValue, StatusCode, header},
    response::Response,
    routing::any,
};
use obop_community::{Community, MAX_BODY, Req};
use std::sync::Arc;

/// Request headers handed to the application (everything else is dropped).
const FORWARDED: [&str; 9] = [
    "accept",
    "accept-language",
    "cookie",
    "host",
    "if-none-match",
    "if-modified-since",
    "origin",
    "x-forwarded-proto",
    "content-type",
];

pub fn router(app: Arc<Community>) -> Router {
    Router::new()
        .route("/community", any(handle))
        .route("/community/", any(handle))
        .route("/community/{*rest}", any(handle))
        .route("/status", any(handle))
        .route("/status/", any(handle))
        .route("/status/{*rest}", any(handle))
        .with_state(app)
        // A refused database call is a 503 with Retry-After here too (D74).
        .layer(axum::middleware::from_fn(crate::ops::storage_faults))
}

async fn handle(State(app): State<Arc<Community>>, req: Request) -> Response {
    let (parts, body) = req.into_parts();
    let bytes = match to_bytes(body, MAX_BODY + 1).await {
        Ok(b) => b,
        Err(_) => return too_large(),
    };
    let target = parts.uri.path_and_query().map_or("/", |p| p.as_str());
    let mut r = Req::new(parts.method.as_str(), target);
    for name in FORWARDED {
        let vals: Vec<&str> = parts
            .headers
            .get_all(name)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .collect();
        if !vals.is_empty() {
            let sep = if name == "cookie" { "; " } else { ", " };
            r.headers.insert(name.to_string(), vals.join(sep));
        }
    }
    r.body = bytes.to_vec();
    let resp = blocking(move || app.handle(&r)).await;
    let mut out = Response::builder()
        .status(StatusCode::from_u16(resp.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR));
    for (k, v) in &resp.headers {
        if let (Ok(k), Ok(v)) = (
            HeaderName::from_bytes(k.as_bytes()),
            HeaderValue::from_str(v),
        ) {
            out = out.header(k, v);
        }
    }
    out.body(Body::from(resp.body)).unwrap_or_else(|_| {
        Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::empty())
            .expect("static response")
    })
}

fn too_large() -> Response {
    Response::builder()
        .status(StatusCode::PAYLOAD_TOO_LARGE)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from("request body too large\n"))
        .expect("static response")
}
