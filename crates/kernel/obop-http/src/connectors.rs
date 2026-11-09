//! The two public doors of the connectors (D54): inbound webhooks and the OAuth redirect.
//!
//! Neither is a bearer-token route. A webhook proves itself with its signature (checked, with the
//! timestamp and the replay guard, before the body is read as JSON) and answers every failure of
//! that check alike. The OAuth callback proves itself with the single-use `state` it was started
//! with. Everything else about connectors is the generated command API.
//!
//! The return page of the OAuth callback names the instance (docs/spec/branding.md, row SP-11, slice
//! S1d): "Back to {name}" and "Taking you back to {name}." with the name of the instance brand, in
//! English only, because it is the administrator's own return page and English carries a name with
//! no case ending to get wrong. The callback reads the brand once and first, before it writes (it
//! consumes the single-use state and stores the tokens), and an instance with no brand has the
//! default name, so the page is what it always was, byte for byte.

use super::oauth::read_brand;
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
        // The store was busy: the sender retries (the claim of the event was given back).
        Err(HookError::Busy) => {
            let mut r = json_response(
                StatusCode::SERVICE_UNAVAILABLE,
                json!({"error": "try again later"}),
            );
            r.headers_mut().insert(
                axum::http::header::RETRY_AFTER,
                axum::http::HeaderValue::from_static("1"),
            );
            r
        }
        Err(HookError::Rejected(why)) => {
            json_response(StatusCode::UNPROCESSABLE_ENTITY, json!({"error": why}))
        }
    }
}

/// The page the callback ends on. `name` is the name of the instance, put into the link text and
/// escaped there; the sentence `text` is escaped whole, so a name that a caller put into it is
/// escaped once. The refresh carries the path `back` only: the name is never in an address.
fn page(title: &str, text: &str, back: Option<&str>, name: &str) -> Response {
    let esc = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };
    // `back` is a path of this instance (built by the server, never taken from the request).
    let (link, refresh) = match back {
        Some(b) => (
            format!(
                "<p><a href=\"{b}\">Back to {n}</a></p>",
                b = esc(b),
                n = esc(name)
            ),
            format!(
                "<meta http-equiv=\"refresh\" content=\"3;url={b}\">",
                b = esc(b)
            ),
        ),
        None => (String::new(), String::new()),
    };
    let html = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">{refresh}<title>{t}</title><style>body{{font:16px system-ui,sans-serif;max-width:32rem;margin:15vh auto;padding:0 1rem;line-height:1.5}}</style></head><body><h1>{t}</h1><p>{p}</p>{link}</body></html>",
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
        // The brand this page shows, asked once and first: the callback writes (the state is
        // consumed and the tokens are stored), so it reads before it does anything. A failed read
        // is the 503 of D74 and the state is left as it was.
        let brand = read_brand(&s);
        let name = brand.brand().name();
        let Some(rt) = s.connectors.get() else {
            return json_response(StatusCode::NOT_FOUND, json!({"error": "not found"}));
        };
        let base = s.back_office.get().map_or("/", String::as_str).trim_end_matches('/');
        let back_page = |app: &str| format!("{base}/#/connectors/{app}");
        if q.contains_key("error") {
            // The person said no, or the provider failed. Say so without echoing its text.
            return page("Not connected", "The provider did not authorise the connection. You can close this window and try again.", Some(&back_page("catalog")), name);
        }
        match (q.get("state"), q.get("code")) {
            (Some(state), Some(code)) => match rt.oauth_callback(state, code) {
                Ok(id) => {
                    let to = if rt
                        .connection(&id)
                        .is_ok_and(|(_, p)| p.id == obop_connectors::drive::PROVIDER)
                    {
                        back_page("google_drive")
                    } else {
                        back_page("connection")
                    };
                    page("Connected", &format!("The account is authorised. Taking you back to {name}."), Some(&to), name)
                }
                Err(_) => page("Not connected", "The authorization could not be completed: the link is invalid or has expired, or the provider refused. Start again from the connection; its page shows the last error.", Some(&back_page("catalog")), name),
            },
            _ => page("Not connected", "This address is the return point of an authorization and needs the answer of the provider.", None, name),
        }
    })
    .await
}
