//! The two public doors of the connectors (D54): inbound webhooks and the OAuth redirect.
//!
//! Neither is a bearer-token route. A webhook proves itself with its signature (checked, with the
//! timestamp and the replay guard, before the body is read as JSON) and answers every failure of
//! that check alike. The OAuth callback proves itself with the single-use `state` it was started
//! with. Everything else about connectors is the generated command API.

use super::{HttpState, blocking};
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use obop_connectors::runtime::HookError;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

/// Most signed-or-not requests one connection's address takes per minute. The check happens before
/// the signature work, so a flood of garbage cannot burn CPU on HMAC.
const PER_MINUTE: u32 = 240;

fn limited(s: &HttpState, connection: &str) -> bool {
    // Counted in the storage backend, so every server process on the database shares it.
    obop_storage::limit::hit(
        &*s.shared.backend,
        &format!("rl:hook:{connection}"),
        u64::from(PER_MINUTE),
        60_000,
    )
    .is_err()
}

fn json_response(status: StatusCode, v: serde_json::Value) -> Response {
    (status, axum::Json(v)).into_response()
}

/// `POST /hooks/in/{connection}`.
pub async fn hook(
    State(s): State<Arc<HttpState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    blocking(move || hook_blocking(&s, &id, &headers, &body)).await
}

fn hook_blocking(s: &HttpState, id: &str, headers: &HeaderMap, body: &[u8]) -> Response {
    let Some(rt) = s.connectors.get() else {
        return json_response(StatusCode::NOT_FOUND, json!({"error": "not found"}));
    };
    if limited(s, id) {
        return json_response(
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
    match rt.receive_webhook(id, &header, body) {
        Ok(o) => json_response(
            StatusCode::ACCEPTED,
            json!({"accepted": o.accepted(), "proposed": o.proposed.len(), "rejected": o.rejected.len()}),
        ),
        Err(HookError::NotFound) => {
            json_response(StatusCode::NOT_FOUND, json!({"error": "not found"}))
        }
        Err(HookError::TooLarge) => json_response(
            StatusCode::PAYLOAD_TOO_LARGE,
            json!({"error": "payload too large"}),
        ),
        // A bad signature, a stale timestamp and a replay are the same answer.
        Err(HookError::Unauthorised) => json_response(
            StatusCode::UNAUTHORIZED,
            json!({"error": "invalid signature"}),
        ),
        Err(HookError::Refused(why)) => json_response(StatusCode::FORBIDDEN, json!({"error": why})),
        Err(HookError::Rejected(why)) => {
            json_response(StatusCode::UNPROCESSABLE_ENTITY, json!({"error": why}))
        }
    }
}

fn page(title: &str, text: &str) -> Response {
    let esc = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    let html = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{t}</title><style>body{{font:16px system-ui,sans-serif;max-width:32rem;margin:15vh auto;padding:0 1rem;line-height:1.5}}</style></head><body><h1>{t}</h1><p>{p}</p></body></html>",
        t = esc(title),
        p = esc(text)
    );
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
            (header::REFERRER_POLICY, "no-referrer"),
        ],
        html,
    )
        .into_response()
}

/// `GET /oauth/callback?state=..&code=..`.
pub async fn oauth_callback(
    State(s): State<Arc<HttpState>>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    blocking(move || {
        let Some(rt) = s.connectors.get() else {
            return json_response(StatusCode::NOT_FOUND, json!({"error": "not found"}));
        };
        if q.contains_key("error") {
            // The person said no, or the provider failed. Say so without echoing its text.
            return page("Not connected", "The provider did not authorise the connection. You can close this window and try again.");
        }
        match (q.get("state"), q.get("code")) {
            (Some(state), Some(code)) => match rt.oauth_callback(state, code) {
                Ok(_) => page("Connected", "The account is authorised. You can close this window and return to obop."),
                Err(_) => page("Not connected", "The authorization could not be completed: the link is invalid or has expired. Start again from the connection."),
            },
            _ => page("Not connected", "This address is the return point of an authorization and needs the answer of the provider."),
        }
    })
    .await
}
