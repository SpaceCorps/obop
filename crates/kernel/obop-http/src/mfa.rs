//! Two-factor routes (D53): the second sign-in step and enrolment, disable, recovery codes and
//! the admin reset. Only a signed-in human session may call them; an agent token, `--dev-auth`
//! and an unknown token all get the same 404/401 as any other hidden route. A session limited to
//! enrolment may call the enrolment routes and nothing else.

use super::{api_error, blocking};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use obop_commands::{Authenticator, LoginOutcome, MfaApi, MfaError, Principal};
use serde_json::{Value as J, json};
use std::sync::Arc;

type Auth = State<Arc<dyn Authenticator>>;

/// The two-factor routes under `prefix` (`/api/auth` on an instance, `/api/platform` on the
/// control plane), bound to the authenticator they act on.
pub(crate) fn routes(prefix: &str, auth: Arc<dyn Authenticator>) -> Router {
    Router::new()
        .route(&format!("{prefix}/login/mfa"), post(login_mfa))
        .route(&format!("{prefix}/mfa"), get(status))
        .route(&format!("{prefix}/mfa/begin"), post(begin))
        .route(&format!("{prefix}/mfa/confirm"), post(confirm))
        .route(&format!("{prefix}/mfa/disable"), post(disable))
        .route(&format!("{prefix}/mfa/recovery"), post(recovery))
        .route(&format!("{prefix}/mfa/reset"), post(reset))
        .route(&format!("{prefix}/sessions"), get(sessions))
        .route(&format!("{prefix}/sessions/revoke-all"), post(revoke_all))
        .route(&format!("{prefix}/password"), post(change_password))
        .route(&format!("{prefix}/users"), get(users))
        .route(
            &format!("{prefix}/users/{{name}}/disable"),
            post(disable_user),
        )
        .route(
            &format!("{prefix}/users/{{name}}/enable"),
            post(enable_user),
        )
        .route(
            &format!("{prefix}/users/{{name}}/reset-password"),
            post(reset_password),
        )
        .with_state(auth)
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

/// 403 for a session that may only change the password an administrator reset.
pub(crate) fn password_change_required() -> Response {
    api_error(
        StatusCode::FORBIDDEN,
        "you must choose a new password before you can continue",
        "password_change_required",
        "An administrator set a temporary password. Call POST /api/auth/password with current_password (the temporary one) and new_password, then use the session again.",
    )
}

/// The JSON of a sign-in step.
pub(crate) fn outcome(auth: Option<&dyn Authenticator>, out: LoginOutcome) -> Response {
    outcome_json(auth, out).into_response()
}

/// The JSON of a sign-in step, for a caller that adds to it.
pub(crate) fn outcome_json(auth: Option<&dyn Authenticator>, out: LoginOutcome) -> Json<J> {
    match out {
        LoginOutcome::Session {
            token,
            mfa_enrolment_required: false,
            password_change_required: false,
        } => Json(json!({"token": token})),
        LoginOutcome::Session {
            token,
            password_change_required: true,
            ..
        } => Json(json!({
            "token": token,
            "password_change_required": true,
            "hint": "An administrator set a temporary password. This session can only change it: POST /api/auth/password with current_password and new_password."
        })),
        LoginOutcome::Session { token, .. } => Json(json!({
            "token": token,
            "mfa_enrolment_required": true,
            "hint": "This session can only set up two-factor authentication: POST /api/auth/mfa/begin and /api/auth/mfa/confirm."
        })),
        LoginOutcome::MfaRequired { mfa_token } => {
            // What can finish this sign-in: an authenticator app code and/or a passkey (D65).
            let methods = auth
                .and_then(|a| a.passkeys())
                .map(|p| p.methods(&mfa_token))
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| vec!["totp"]);
            Json(json!({
                "mfa_required": true,
                "mfa_token": mfa_token,
                "expires_in": 300,
                "methods": methods,
                "hint": "POST /api/auth/login/mfa with {mfa_token, code} or {mfa_token, recovery_code}; with a passkey, POST /api/auth/login/passkey/options and /api/auth/login/passkey with {mfa_token, credential}."
            }))
        }
    }
}

pub(crate) fn mfa_error(e: MfaError) -> Response {
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
        MfaError::BadPassword => api_error(
            StatusCode::UNAUTHORIZED,
            "the password is not right",
            "invalid_password",
            "Enter your current password.",
        ),
        MfaError::Expired => api_error(
            StatusCode::UNAUTHORIZED,
            "this sign-in has expired; enter your password again",
            "mfa_expired",
            "Start again with POST /api/auth/login.",
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
        // The store refused a call: busy or unreachable is the server's trouble, with a time to wait.
        MfaError::Unavailable(m) if obop_storage::fault::Fault::is_transient(&m) => {
            crate::unavailable(&m)
        }
        // The enrolment could not read the instance branding (a read error that is not a transient
        // fault, or a stored name that cannot be shown), so it would have written a default into the
        // person's own device: refused, and nothing is stored. After the transient arm, so that a
        // transient fault never reaches it, and before the generic one, whose hint names the wrong
        // cause. Waiting does not repair a stored name, so there is no `Retry-After`
        // (docs/spec/branding.md, section 4).
        MfaError::Unavailable(m) if m.starts_with(obop_commands::brand::UNREADABLE) => {
            // The cause is in the message (a stored name, a store that could not be read, a server
            // with no reader), and an operator who sees the code in an answer finds it here. The
            // store's own text, when there is one, was written by `enrolment_error`.
            super::log_line(&format!(
                "obop: an enrolment was refused: {}",
                super::log_safe(&m)
            ));
            api_error(
                StatusCode::SERVICE_UNAVAILABLE,
                &m,
                "brand_unavailable",
                "The instance branding could not be read, so nothing was enrolled. An administrator can see why with `obop admin brand status` at the terminal and repair it with `obop admin brand configure` or `obop admin brand clear` (docs/DEPLOY.md).",
            )
        }
        MfaError::Unavailable(m) => api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            &m,
            "mfa_unavailable",
            "Ask the operator to set OBOP_SECRET_KEY (see docs/DEPLOY.md).",
        ),
    }
}

/// The signed-in human behind the bearer token, limited sessions included, and the token.
#[allow(clippy::result_large_err)]
pub(crate) fn human<'a>(
    auth: &'a dyn Authenticator,
    h: &HeaderMap,
) -> Result<(Principal, String, &'a dyn MfaApi), Response> {
    let token = h
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| StatusCode::UNAUTHORIZED.into_response())?;
    let p = auth
        .resolve(token)
        .ok_or_else(|| StatusCode::UNAUTHORIZED.into_response())?;
    // Agents and authenticators without two-factor see no such route.
    let api = auth
        .mfa()
        .filter(|_| p.is_person())
        .ok_or_else(|| mfa_error(MfaError::NotFound))?;
    Ok((p, token.to_string(), api))
}

pub(crate) fn text<'a>(b: &'a J, k: &str) -> Option<&'a str> {
    b[k].as_str().filter(|s| !s.trim().is_empty())
}

async fn login_mfa(State(a): Auth, Json(b): Json<J>) -> Response {
    blocking(move || {
        let r = a.login_mfa(
            text(&b, "mfa_token").unwrap_or(""),
            text(&b, "code"),
            text(&b, "recovery_code"),
        );
        match r {
            Ok(out) => outcome(Some(&*a), out),
            Err(e) => mfa_error(e),
        }
    })
    .await
}

async fn status(State(a): Auth, h: HeaderMap) -> Response {
    blocking(move || match human(&*a, &h) {
        Ok((p, _, api)) => api
            .status(&p)
            .map_or_else(mfa_error, |j| Json(j).into_response()),
        Err(r) => r,
    })
    .await
}

async fn begin(State(a): Auth, h: HeaderMap) -> Response {
    blocking(move || match human(&*a, &h) {
        Ok((p, _, api)) => api
            .begin(&p)
            .map_or_else(mfa_error, |j| Json(j).into_response()),
        Err(r) => r,
    })
    .await
}

async fn confirm(State(a): Auth, h: HeaderMap, Json(b): Json<J>) -> Response {
    blocking(move || match human(&*a, &h) {
        Ok((p, token, api)) => api
            .confirm(&p, &token, text(&b, "code").unwrap_or(""))
            .map_or_else(mfa_error, |j| Json(j).into_response()),
        Err(r) => r,
    })
    .await
}

async fn disable(State(a): Auth, h: HeaderMap, Json(b): Json<J>) -> Response {
    blocking(move || match human(&*a, &h) {
        Ok((p, session, api)) => api
            .disable_in_session(
                &p,
                &session,
                b["password"].as_str().unwrap_or(""),
                text(&b, "code"),
                text(&b, "recovery_code"),
            )
            .map_or_else(mfa_error, |()| {
                Json(json!({"enabled": false})).into_response()
            }),
        Err(r) => r,
    })
    .await
}

async fn recovery(State(a): Auth, h: HeaderMap, Json(b): Json<J>) -> Response {
    blocking(move || match human(&*a, &h) {
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

async fn reset(State(a): Auth, h: HeaderMap, Json(b): Json<J>) -> Response {
    blocking(move || match human(&*a, &h) {
        Ok((p, _, api)) => api
            .admin_reset(&p, b["user"].as_str().unwrap_or(""))
            .map_or_else(mfa_error, |()| Json(json!({"reset": true})).into_response()),
        Err(r) => r,
    })
    .await
}

/// The person's own sessions: when they started and end, which one this is.
async fn sessions(State(a): Auth, h: HeaderMap) -> Response {
    blocking(move || match human(&*a, &h) {
        Ok((p, session, api)) => api
            .sessions(&p, &session)
            .map_or_else(mfa_error, |j| Json(j).into_response()),
        Err(r) => r,
    })
    .await
}

/// Sign out everywhere. The body may carry `{"keep_current": true}` to keep this session.
async fn revoke_all(State(a): Auth, h: HeaderMap, body: Option<Json<J>>) -> Response {
    blocking(move || match human(&*a, &h) {
        Ok((p, session, api)) => {
            let keep = body
                .as_ref()
                .is_some_and(|b| b["keep_current"].as_bool() == Some(true));
            api.revoke_sessions(&p, &session, keep)
                .map_or_else(mfa_error, |n| Json(json!({"revoked": n})).into_response())
        }
        Err(r) => r,
    })
    .await
}

/// Change one's own password: the current one, and a code when two-factor is on. Ends the
/// person's other sessions.
async fn change_password(State(a): Auth, h: HeaderMap, Json(b): Json<J>) -> Response {
    blocking(move || match human(&*a, &h) {
        Ok((p, session, api)) => api
            .change_password(
                &p,
                &session,
                b["current_password"].as_str().unwrap_or(""),
                b["new_password"].as_str().unwrap_or(""),
                text(&b, "code"),
                text(&b, "recovery_code"),
            )
            .map_or_else(mfa_error, |()| {
                Json(json!({"changed": true})).into_response()
            }),
        Err(r) => r,
    })
    .await
}

/// Administrators: the accounts.
async fn users(State(a): Auth, h: HeaderMap) -> Response {
    blocking(move || match human(&*a, &h) {
        Ok((p, _, api)) => api
            .users(&p)
            .map_or_else(mfa_error, |j| Json(j).into_response()),
        Err(r) => r,
    })
    .await
}

async fn set_disabled(
    a: Arc<dyn Authenticator>,
    h: HeaderMap,
    name: String,
    disabled: bool,
) -> Response {
    blocking(move || match human(&*a, &h) {
        Ok((p, _, api)) => api
            .set_user_disabled(&p, &name, disabled)
            .map_or_else(mfa_error, |()| {
                Json(json!({"user": name, "disabled": disabled})).into_response()
            }),
        Err(r) => r,
    })
    .await
}

async fn disable_user(State(a): Auth, h: HeaderMap, Path(name): Path<String>) -> Response {
    set_disabled(a, h, name, true).await
}

async fn enable_user(State(a): Auth, h: HeaderMap, Path(name): Path<String>) -> Response {
    set_disabled(a, h, name, false).await
}

/// An administrator sets a temporary password; the person must choose a new one at sign-in.
async fn reset_password(
    State(a): Auth,
    h: HeaderMap,
    Path(name): Path<String>,
    Json(b): Json<J>,
) -> Response {
    blocking(move || match human(&*a, &h) {
        Ok((p, _, api)) => api
            .reset_user_password(&p, &name, b["password"].as_str().unwrap_or(""))
            .map_or_else(mfa_error, |()| {
                Json(json!({"user": name, "must_change_password": true})).into_response()
            }),
        Err(r) => r,
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    /// The status, the headers and the JSON body of an answer.
    async fn answer(r: Response) -> (StatusCode, HeaderMap, J) {
        let (parts, body) = r.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        (
            parts.status,
            parts.headers,
            serde_json::from_slice(&bytes).unwrap_or(J::Null),
        )
    }

    /// The three 503 answers of `MfaError::Unavailable`, told apart by the message, in the order the
    /// arms are tried (docs/spec/branding.md, section 4): a transient fault of the store first (503
    /// with `Retry-After`), then the branding that could not be read (503 `brand_unavailable`, no
    /// `Retry-After`, a hint that names the branding), then the generic one (the server key).
    #[tokio::test]
    async fn the_arms_of_unavailable_are_told_apart_by_the_message() {
        // A transient fault: D74's 503 with a time to wait, and never the answer of the branding.
        for text in [
            "storage_busy: all database connections are in use, try again",
            "storage_unavailable: the connection broke",
            "storage_timeout: waited 5s",
        ] {
            let (status, headers, body) =
                answer(mfa_error(MfaError::Unavailable(text.into()))).await;
            assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{text}");
            assert!(headers.contains_key("retry-after"), "{text}");
            assert_ne!(body["code"], json!("brand_unavailable"), "{text}");
            assert_ne!(body["code"], json!("mfa_unavailable"), "{text}");
        }
        // The branding that could not be read.
        let message = format!(
            "{}: the stored name cannot be shown",
            obop_commands::brand::UNREADABLE
        );
        let (status, headers, body) =
            answer(mfa_error(MfaError::Unavailable(message.clone()))).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(!headers.contains_key("retry-after"));
        assert_eq!(body["code"], json!("brand_unavailable"));
        assert_eq!(body["error"], json!(message));
        let hint = body["hint"].as_str().unwrap();
        assert!(hint.contains("obop admin brand status"), "{hint}");
        assert!(!hint.contains("OBOP_SECRET_KEY"), "{hint}");
        // Any other message is the generic one, with its own hint, as before.
        for text in [
            "two-factor needs OBOP_SECRET_KEY",
            "something else",
            // The words in the middle of a message are not the prefix.
            "the store said: the instance branding could not be read",
        ] {
            let (status, headers, body) =
                answer(mfa_error(MfaError::Unavailable(text.into()))).await;
            assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{text}");
            assert!(!headers.contains_key("retry-after"), "{text}");
            assert_eq!(body["code"], json!("mfa_unavailable"), "{text}");
            assert!(
                body["hint"].as_str().unwrap().contains("OBOP_SECRET_KEY"),
                "{text}"
            );
        }
    }
}
