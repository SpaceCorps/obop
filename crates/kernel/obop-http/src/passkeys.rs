//! Passkey routes (WebAuthn, D65) under `/api/auth`: the person's passkey list and its
//! management, the ceremony options and answers, and the two sign-in routes. Everything that
//! changes a passkey needs the password again (and the second factor when one is on). The origin
//! and the relying party id come from the public URL or the request, never from the browser's own
//! claim; a session limited to enrolment may add its first passkey.

use super::mfa::{human, mfa_error, text};
use super::oauth::client_ip;
use super::{HttpState, blocking};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use obop_commands::{PasskeyApi, Reauth, WebauthnCtx};
use serde_json::{Value as J, json};
use std::sync::Arc;

type Shared = State<Arc<HttpState>>;

/// Bodies here carry a credential (a few kilobytes at most).
const MAX_BODY: usize = 32 * 1024;

pub(crate) fn routes() -> Router<Arc<HttpState>> {
    let limit = axum::extract::DefaultBodyLimit::max(MAX_BODY);
    Router::new()
        .route("/api/auth/passkeys", get(list))
        .route(
            "/api/auth/passkeys/register/options",
            post(register_options).layer(limit),
        )
        .route(
            "/api/auth/passkeys/register/finish",
            post(register_finish).layer(limit),
        )
        .route("/api/auth/passkeys/passwordless", post(passwordless))
        .route("/api/auth/passkeys/proof/options", post(proof_options))
        .route("/api/auth/passkeys/proof", post(proof).layer(limit))
        .route("/api/auth/passkeys/{id}/rename", post(rename))
        .route("/api/auth/passkeys/{id}/remove", post(remove))
        .route(
            "/api/auth/login/passkey/options",
            post(login_options).layer(limit),
        )
        .route("/api/auth/login/passkey", post(login).layer(limit))
}

/// The ceremony site of this request.
#[allow(clippy::result_large_err)]
fn ctx(s: &HttpState, parts: &Parts) -> Result<WebauthnCtx, Response> {
    let trust = s.trust_proxy.load(std::sync::atomic::Ordering::Relaxed);
    let Some((origin, rp_id)) = s.mcp.webauthn_origin(&parts.headers, trust) else {
        return Err(super::api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "passkeys need the address of this instance, which the request did not give",
            "passkeys_unavailable",
            "Set OBOP_PUBLIC_URL (docs/DEPLOY.md).",
        ));
    };
    Ok(WebauthnCtx {
        rp_id,
        origin,
        rp_name: "obop".into(),
        ip: client_ip(s, parts),
    })
}

#[allow(clippy::result_large_err)]
fn api(s: &HttpState) -> Result<&dyn PasskeyApi, Response> {
    s.auth
        .passkeys()
        .ok_or_else(|| mfa_error(obop_commands::MfaError::NotFound))
}

fn reauth(b: &J) -> Reauth<'_> {
    Reauth {
        password: b["password"].as_str().unwrap_or(""),
        code: text(b, "code"),
        recovery_code: text(b, "recovery_code"),
    }
}

fn no_store(mut r: Response) -> Response {
    r.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    r
}

/// The signed-in human, the ceremony site and the passkey service, or the response that says why not.
#[allow(clippy::result_large_err)]
fn setup<'a>(
    s: &'a HttpState,
    parts: &Parts,
) -> Result<
    (
        obop_commands::Principal,
        String,
        WebauthnCtx,
        &'a dyn PasskeyApi,
    ),
    Response,
> {
    let (p, token, _) = human(&*s.auth, &parts.headers)?;
    let c = ctx(s, parts)?;
    Ok((p, token, c, api(s)?))
}

async fn list(State(s): Shared, parts: Parts) -> Response {
    blocking(move || match setup(&s, &parts) {
        Ok((p, _, c, api)) => api
            .list(&p, &c)
            .map_or_else(mfa_error, |j| no_store(Json(j).into_response())),
        Err(r) => r,
    })
    .await
}

async fn register_options(State(s): Shared, parts: Parts, Json(b): Json<J>) -> Response {
    blocking(move || match setup(&s, &parts) {
        Ok((p, _, c, api)) => api
            .register_options(&p, &c, &reauth(&b))
            .map_or_else(mfa_error, |j| no_store(Json(j).into_response())),
        Err(r) => r,
    })
    .await
}

async fn register_finish(State(s): Shared, parts: Parts, Json(b): Json<J>) -> Response {
    blocking(move || match setup(&s, &parts) {
        Ok((p, token, c, api)) => api
            .register_finish(
                &p,
                &token,
                &c,
                b["name"].as_str().unwrap_or(""),
                &b["credential"],
            )
            .map_or_else(mfa_error, |j| no_store(Json(j).into_response())),
        Err(r) => r,
    })
    .await
}

async fn rename(
    State(s): Shared,
    parts: Parts,
    Path(id): Path<String>,
    Json(b): Json<J>,
) -> Response {
    blocking(move || match setup(&s, &parts) {
        Ok((p, _, _, api)) => api
            .rename(&p, &id, b["name"].as_str().unwrap_or(""), &reauth(&b))
            .map_or_else(mfa_error, |()| {
                Json(json!({"renamed": true})).into_response()
            }),
        Err(r) => r,
    })
    .await
}

async fn remove(
    State(s): Shared,
    parts: Parts,
    Path(id): Path<String>,
    Json(b): Json<J>,
) -> Response {
    blocking(move || match setup(&s, &parts) {
        Ok((p, token, _, api)) => api
            .remove(&p, &token, &id, &reauth(&b))
            .map_or_else(mfa_error, |()| {
                Json(json!({"removed": true})).into_response()
            }),
        Err(r) => r,
    })
    .await
}

async fn passwordless(State(s): Shared, parts: Parts, Json(b): Json<J>) -> Response {
    blocking(move || match setup(&s, &parts) {
        Ok((p, _, _, api)) => {
            let on = b["enabled"].as_bool() == Some(true);
            api.set_passwordless(&p, on, &reauth(&b))
                .map_or_else(mfa_error, |()| {
                    Json(json!({"passwordless": on})).into_response()
                })
        }
        Err(r) => r,
    })
    .await
}

async fn proof_options(State(s): Shared, parts: Parts) -> Response {
    blocking(move || match setup(&s, &parts) {
        Ok((p, _, c, api)) => api
            .proof_options(&p, &c)
            .map_or_else(mfa_error, |j| no_store(Json(j).into_response())),
        Err(r) => r,
    })
    .await
}

async fn proof(State(s): Shared, parts: Parts, Json(b): Json<J>) -> Response {
    blocking(move || match setup(&s, &parts) {
        Ok((p, _, c, api)) => api
            .proof(&p, &c, &b["credential"])
            .map_or_else(mfa_error, |()| {
                Json(json!({"proved": true})).into_response()
            }),
        Err(r) => r,
    })
    .await
}

/// `{mfa_token}` asks for the options of the second step; no token asks for usernameless ones.
async fn login_options(State(s): Shared, parts: Parts, Json(b): Json<J>) -> Response {
    blocking(move || {
        let c = match ctx(&s, &parts) {
            Ok(c) => c,
            Err(r) => return r,
        };
        let api = match api(&s) {
            Ok(a) => a,
            Err(r) => return r,
        };
        api.passkey_login_options(&c, text(&b, "mfa_token"))
            .map_or_else(mfa_error, |j| no_store(Json(j).into_response()))
    })
    .await
}

/// `{credential}` signs in without a password; with `mfa_token` it completes the second step.
async fn login(State(s): Shared, parts: Parts, Json(b): Json<J>) -> Response {
    blocking(move || {
        let c = match ctx(&s, &parts) {
            Ok(c) => c,
            Err(r) => return r,
        };
        let api = match api(&s) {
            Ok(a) => a,
            Err(r) => return r,
        };
        s.shared.maybe_purge();
        match api.passkey_login(&c, text(&b, "mfa_token"), &b["credential"]) {
            Ok(out) => {
                // The page shows who signed in, which a usernameless sign-in does not say.
                let who = match &out {
                    obop_commands::LoginOutcome::Session { token, .. } => {
                        s.auth.resolve(token).map(|p| p.id)
                    }
                    _ => None,
                };
                let mut body = super::mfa::outcome_json(Some(&*s.auth), out);
                if let Some(u) = who {
                    body.0["user"] = J::from(u);
                }
                no_store(body.into_response())
            }
            Err(e) => mfa_error(e),
        }
    })
    .await
}
