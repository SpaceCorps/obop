//! The first administrator of a new instance, made in the browser (D96, PROPOSED). Two routes
//! beside `/api/auth/login`, outside the command catalogue as it is, with no bearer token (no
//! account exists yet) and no MCP form (no principal exists to call one):
//!
//! - `GET /api/auth/setup`: `{"open": false}` once any account exists, else
//!   `{"open": true, "code_active": bool, "needs_secret_key": bool}`. Never a code.
//! - `POST /api/auth/setup` `{"code", "username", "password", "instance_name"?}`: the answer of a
//!   password sign-in (`mfa::outcome`), or a refusal: 409 `setup_closed`, 503
//!   `setup_needs_secret_key`, 429 `rate_limited` with `Retry-After`, 403 `setup_code_invalid` (one
//!   answer for every code that is not in force), 400 `setup_instance_name_invalid` for a name the
//!   brand refuses, 400 `invalid_input` for the rest, 503 when the store refused.
//!
//! Both answer 404 when the authenticator has no set-up (`--dev-auth`, the control plane). The
//! rules and their order are in `obop-auth` (`setup.rs`); this file only carries the request. No
//! https rule of its own: the route asks what `/api/auth/login` asks, no more and no less.

use super::sso::source_of;
use super::{HttpState, api_error, api_not_found_response, blocking, mfa};
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use obop_commands::{SetupError, SetupRequest};
use serde_json::{Value as J, json};
use std::sync::Arc;

type Shared = State<Arc<HttpState>>;

/// The body is a handful of short fields.
pub(crate) const MAX_BODY: usize = 4 * 1024;
/// Status reads per address in a minute: the page asks once per load.
const STATUS_PER_MINUTE: u64 = 120;

fn no_store(mut r: Response) -> Response {
    r.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

pub(crate) async fn status(State(s): Shared, parts: Parts) -> Response {
    blocking(move || no_store(status_blocking(&s, &parts))).await
}

fn status_blocking(s: &HttpState, parts: &Parts) -> Response {
    let Some(api) = s.auth.setup() else {
        return api_not_found_response();
    };
    let key = format!("rl:setup:status:{}", source_of(s, parts));
    if obop_storage::limit::hit(&*s.shared.backend, &key, STATUS_PER_MINUTE, 60_000).is_err() {
        return api_error(
            StatusCode::TOO_MANY_REQUESTS,
            "too many requests, try again in a minute",
            "rate_limited",
            "Wait a minute and load the page again.",
        );
    }
    status_answer(api.status())
}

/// The answer of the status route. A refusal of the store is a refusal (503), never "closed".
fn status_answer(st: Result<obop_commands::SetupStatus, SetupError>) -> Response {
    match st {
        Ok(st) if !st.open => axum::Json(json!({"open": false})).into_response(),
        Ok(st) => axum::Json(json!({
            "open": true,
            "code_active": st.code_active,
            "needs_secret_key": st.needs_secret_key,
        }))
        .into_response(),
        Err(e) => refusal(e),
    }
}

pub(crate) async fn complete(State(s): Shared, parts: Parts, body: Bytes) -> Response {
    blocking(move || no_store(complete_blocking(&s, &parts, &body))).await
}

fn complete_blocking(s: &HttpState, parts: &Parts, body: &[u8]) -> Response {
    let Some(api) = s.auth.setup() else {
        return api_not_found_response();
    };
    // A body that is not an object of texts is read as empty fields: the order of the checks
    // decides the answer (an empty code is a wrong code), never the shape of the body.
    let v: J = serde_json::from_slice(body).unwrap_or(J::Null);
    let text = |k: &str| v.get(k).and_then(J::as_str).unwrap_or("");
    let source = source_of(s, parts);
    let name = text("instance_name");
    let req = SetupRequest {
        source: &source,
        code: text("code"),
        username: text("username"),
        password: text("password"),
        instance_name: (!name.trim().is_empty()).then_some(name),
    };
    s.shared.maybe_purge();
    match api.complete(&req) {
        Ok(out) => mfa::outcome(Some(&*s.auth), out),
        Err(e) => refusal(e),
    }
}

fn refusal(e: SetupError) -> Response {
    match e {
        SetupError::Closed => api_error(
            StatusCode::CONFLICT,
            "this instance already has an account",
            "setup_closed",
            "Sign in with POST /api/auth/login.",
        ),
        SetupError::NeedsSecretKey => api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "the server has no OBOP_SECRET_KEY, so the first administrator could not set up two-factor sign-in",
            "setup_needs_secret_key",
            "Set OBOP_SECRET_KEY in the environment of the server, restart it, and use the new set-up code it prints (docs/DEPLOY.md).",
        ),
        SetupError::RateLimited(wait) => {
            let mut r = api_error(
                StatusCode::TOO_MANY_REQUESTS,
                "too many attempts, try again later",
                "rate_limited",
                "Wait for the time in Retry-After.",
            );
            r.headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from(wait));
            r
        }
        SetupError::CodeInvalid => api_error(
            StatusCode::FORBIDDEN,
            "the set-up code is not right, or it has run out",
            "setup_code_invalid",
            "Use the code from the server log; restart the server to print a new one.",
        ),
        SetupError::Invalid(m) => api_error(
            StatusCode::BAD_REQUEST,
            &m,
            "invalid_input",
            "Fix the input described in the error and retry.",
        ),
        SetupError::InstanceNameRefused(m) => api_error(
            StatusCode::BAD_REQUEST,
            &format!("the instance name is refused: {m}"),
            "setup_instance_name_invalid",
            "Choose another instance name, or leave it out and set it later under Customize, Identity.",
        ),
        SetupError::Unavailable(m) => super::unavailable(&m),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rule 2: a store that refused is a 503 on both routes, whatever the store noted. Never a 200
    /// `{"open": false}` and never a 409 "closed": both would say an account exists.
    #[test]
    fn a_refusal_of_the_store_is_503_on_both_routes() {
        let refused = || SetupError::Unavailable("storage_busy: the pool is full".into());
        assert_eq!(refusal(refused()).status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            status_answer(Err(refused())).status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[test]
    fn each_refusal_has_its_status() {
        for (e, want) in [
            (SetupError::Closed, StatusCode::CONFLICT),
            (SetupError::NeedsSecretKey, StatusCode::SERVICE_UNAVAILABLE),
            (SetupError::RateLimited(5), StatusCode::TOO_MANY_REQUESTS),
            (SetupError::CodeInvalid, StatusCode::FORBIDDEN),
            (SetupError::Invalid("x".into()), StatusCode::BAD_REQUEST),
            (
                SetupError::InstanceNameRefused("name: x".into()),
                StatusCode::BAD_REQUEST,
            ),
        ] {
            assert_eq!(refusal(e.clone()).status(), want, "{e:?}");
        }
    }
}
