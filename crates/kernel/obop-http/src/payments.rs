//! The public door of the payment module (D62): `POST /hooks/pay/{provider}`.
//!
//! Not a bearer-token route: the delivery proves itself with the provider's signature, checked
//! on the raw body (with the timestamp tolerance and the replay record) before the body is read
//! as JSON. Every failure of that check answers the same `401`. The body is capped at 256 KiB
//! and the door is rate limited before any signature work.

use super::{HttpState, blocking};
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use obop_commands::HookReply;
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Requests per minute per provider name, counted before any signature work.
const PER_MINUTE: u32 = 600;

/// Largest body the door reads.
pub const MAX_BODY: usize = 256 * 1024;

fn limited(provider: &str) -> bool {
    static WINDOWS: OnceLock<Mutex<HashMap<String, (Instant, u32)>>> = OnceLock::new();
    let mut g = WINDOWS.get_or_init(Default::default).lock().unwrap();
    if g.len() > 1_000 {
        g.retain(|_, (t, _)| t.elapsed() < Duration::from_secs(60));
    }
    let e = g
        .entry(provider.chars().take(32).collect())
        .or_insert((Instant::now(), 0));
    if e.0.elapsed() >= Duration::from_secs(60) {
        *e = (Instant::now(), 0);
    }
    e.1 += 1;
    e.1 > PER_MINUTE
}

fn reply(status: StatusCode, v: serde_json::Value) -> Response {
    (status, axum::Json(v)).into_response()
}

/// `POST /hooks/pay/{provider}`.
pub async fn hook(
    State(s): State<Arc<HttpState>>,
    Path(provider): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    blocking(move || {
        let Some(h) = s.payments.get() else {
            return reply(StatusCode::NOT_FOUND, json!({"error": "not found"}));
        };
        if limited(&provider) {
            return reply(
                StatusCode::TOO_MANY_REQUESTS,
                json!({"error": "too many requests"}),
            );
        }
        let header = |name: &str| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(String::from)
        };
        match h.webhook(&provider, &header, &body) {
            HookReply::Done | HookReply::Duplicate | HookReply::Ignored => {
                reply(StatusCode::OK, json!({"received": true}))
            }
            HookReply::NotFound => reply(StatusCode::NOT_FOUND, json!({"error": "not found"})),
            HookReply::Unauthorized => {
                reply(StatusCode::UNAUTHORIZED, json!({"error": "unauthorized"}))
            }
            HookReply::Retry => reply(
                StatusCode::SERVICE_UNAVAILABLE,
                json!({"error": "try again later"}),
            ),
        }
    })
    .await
}
