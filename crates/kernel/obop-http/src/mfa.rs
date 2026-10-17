//! Two-factor routes (D53): the second sign-in step and enrolment, disable, recovery codes and
//! the admin reset. Only a signed-in human session may call them; an agent token, `--dev-auth`
//! and an unknown token all get the same 404/401 as any other hidden route. A session limited to
//! enrolment may call the enrolment routes and nothing else.

use super::{HttpState, Shared, api_error, blocking};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use obop_commands::{LoginOutcome, MfaApi, MfaError, Principal, PrincipalKind};
use serde_json::{Value as J, json};
use std::sync::Arc;

pub(crate) fn routes() -> Router<Arc<HttpState>> {
    Router::new()
        .route("/api/auth/login/mfa", post(login_mfa))
        .route("/api/auth/mfa", get(status))
        .route("/api/auth/mfa/begin", post(begin))
        .route("/api/auth/mfa/confirm", post(confirm))
        .route("/api/auth/mfa/disable", post(disable))
        .route("/api/auth/mfa/recovery", post(recovery))
        .route("/api/auth/mfa/reset", post(reset))
}

/// 403 for a session that may only enrol.
pub(crate) fn enrolment_required() -> Response {
    api_error(
        StatusCode::FORBIDDEN,
        "two-factor authentication must be set up before you can continue",
        "mfa_enrolment_required",
        "Your administrator requires two-factor authentication. Call POST /api/auth/mfa/begin, then POST /api/auth/mfa/confirm with a code from your authenticator app (in the app: Customize, Security), then use the session again.",
    )
}

/// The JSON of a sign-in step.
pub(crate) fn outcome(out: LoginOutcome) -> Response {
    match out {
        LoginOutcome::Session {
            token,
            mfa_enrolment_required: false,
        } => Json(json!({"token": token})).into_response(),
        LoginOutcome::Session { token, .. } => Json(json!({
            "token": token,
            "mfa_enrolment_required": true,
            "hint": "This session can only set up two-factor authentication: POST /api/auth/mfa/begin and /api/auth/mfa/confirm."
        }))
        .into_response(),
        LoginOutcome::MfaRequired { mfa_token } => Json(json!({
            "mfa_required": true,
            "mfa_token": mfa_token,
            "expires_in": 300,
            "hint": "POST /api/auth/login/mfa with {mfa_token, code} or {mfa_token, recovery_code}."
        }))
        .into_response(),
    }
}

fn mfa_error(e: MfaError) -> Response {
    match e {
        MfaError::Invalid(m) => api_error(
            StatusCode::BAD_REQUEST,
            &m,
            "invalid_input",
            "Fix the input described in the error and retry.",
        ),
        MfaError::Denied(m) => api_error(
            StatusCode::UNAUTHORIZED,
            &m,
            "invalid_code",
            "Check the code, or use a recovery code.",
        ),
        MfaError::Locked(m) => api_error(
            StatusCode::TOO_MANY_REQUESTS,
            &m,
            "rate_limited",
            "Wait a minute and try again.",
        ),
        MfaError::NotFound => api_error(
            StatusCode::NOT_FOUND,
            "not found",
            "not_found",
            "The API is described at /api/openapi.json.",
        ),
        MfaError::Unavailable(m) => api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            &m,
            "mfa_unavailable",
            "Ask the operator to set OBOP_SECRET_KEY (see docs/DEPLOY.md).",
        ),
    }
}

/// The signed-in human behind the bearer token, limited sessions included, and the token.
fn human<'a>(s: &'a HttpState, h: &HeaderMap) -> Result<(Principal, String, &'a dyn MfaApi), Response> {
    let token = h
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| StatusCode::UNAUTHORIZED.into_response())?;
    let p = s
        .auth
        .resolve(token)
        .ok_or_else(|| StatusCode::UNAUTHORIZED.into_response())?;
    // Agents and authenticators without two-factor see no such route.
    let api = s
        .auth
        .mfa()
        .filter(|_| p.kind == PrincipalKind::User)
        .ok_or_else(|| mfa_error(MfaError::NotFound))?;
    Ok((p, token.to_string(), api))
}

fn text<'a>(b: &'a J, k: &str) -> Option<&'a str> {
    b[k].as_str().filter(|s| !s.trim().is_empty())
}

async fn login_mfa(State(s): Shared, Json(b): Json<J>) -> Response {
    blocking(move || {
        let r = s.auth.login_mfa(
            text(&b, "mfa_token").unwrap_or(""),
            text(&b, "code"),
            text(&b, "recovery_code"),
        );
        match r {
            Ok(out) => outcome(out),
            Err(e) => mfa_error(e),
        }
    })
    .await
}

async fn status(State(s): Shared, h: HeaderMap) -> Response {
    blocking(move || match human(&s, &h) {
        Ok((p, _, api)) => api.status(&p).map_or_else(mfa_error, |j| Json(j).into_response()),
        Err(r) => r,
    })
    .await
}

async fn begin(State(s): Shared, h: HeaderMap) -> Response {
    blocking(move || match human(&s, &h) {
        Ok((p, _, api)) => api.begin(&p).map_or_else(mfa_error, |j| Json(j).into_response()),
        Err(r) => r,
    })
    .await
}

async fn confirm(State(s): Shared, h: HeaderMap, Json(b): Json<J>) -> Response {
    blocking(move || match human(&s, &h) {
        Ok((p, token, api)) => api
            .confirm(&p, &token, text(&b, "code").unwrap_or(""))
            .map_or_else(mfa_error, |j| Json(j).into_response()),
        Err(r) => r,
    })
    .await
}

async fn disable(State(s): Shared, h: HeaderMap, Json(b): Json<J>) -> Response {
    blocking(move || match human(&s, &h) {
        Ok((p, _, api)) => api
            .disable(
                &p,
                b["password"].as_str().unwrap_or(""),
                text(&b, "code"),
                text(&b, "recovery_code"),
            )
            .map_or_else(mfa_error, |()| Json(json!({"enabled": false})).into_response()),
        Err(r) => r,
    })
    .await
}

async fn recovery(State(s): Shared, h: HeaderMap, Json(b): Json<J>) -> Response {
    blocking(move || match human(&s, &h) {
        Ok((p, _, api)) => api
            .regenerate_recovery(
                &p,
                b["password"].as_str().unwrap_or(""),
                text(&b, "code"),
                text(&b, "recovery_code"),
            )
            .map_or_else(mfa_error, |j| Json(j).into_response()),
        Err(r) => r,
    })
    .await
}

async fn reset(State(s): Shared, h: HeaderMap, Json(b): Json<J>) -> Response {
    blocking(move || match human(&s, &h) {
        Ok((p, _, api)) => api
            .admin_reset(&p, b["user"].as_str().unwrap_or(""))
            .map_or_else(mfa_error, |()| Json(json!({"reset": true})).into_response()),
        Err(r) => r,
    })
    .await
}
