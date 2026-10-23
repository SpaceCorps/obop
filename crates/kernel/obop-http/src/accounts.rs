//! Account administration routes (D69) under `/api/auth`: create a user, set a user's groups,
//! offboard a user, and the groups that can be given. Only a signed-in administrator's session
//! reaches them (an agent token sees the same 404 as for any hidden route; an agent proposes the
//! same changes through the `user_account` commands, which an administrator approves).

use super::mfa::{human, mfa_error};
use super::{HttpState, blocking};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use obop_commands::{AccountsApi, MfaError, NewUser};
use serde_json::{Value as J, json};
use std::sync::Arc;

type Shared = State<Arc<HttpState>>;

pub(crate) fn routes() -> Router<Arc<HttpState>> {
    Router::new()
        .route("/api/auth/groups", get(groups))
        .route("/api/auth/users", post(create))
        .route("/api/auth/users/{name}/groups", post(set_groups))
        .route("/api/auth/users/{name}/offboard", post(offboard))
}

/// A list of group names from a JSON array of strings, or one comma separated string.
pub(crate) fn group_list(v: &J) -> Result<Vec<String>, MfaError> {
    match v {
        J::Null => Ok(Vec::new()),
        J::String(s) => Ok(s.split(',').map(|g| g.trim().to_string()).collect()),
        J::Array(a) => a
            .iter()
            .map(|g| {
                g.as_str()
                    .map(String::from)
                    .ok_or_else(|| MfaError::Invalid("groups must be a list of names".into()))
            })
            .collect(),
        _ => Err(MfaError::Invalid("groups must be a list of names".into())),
    }
}

/// The administrator behind the request and the accounts port, or the response to send.
#[allow(clippy::result_large_err)]
fn admin<'a>(
    s: &'a HttpState,
    h: &HeaderMap,
) -> Result<(obop_commands::Principal, &'a dyn AccountsApi), Response> {
    let (p, _, _) = human(&*s.auth, h)?;
    let api = s
        .auth
        .accounts()
        .ok_or_else(|| mfa_error(MfaError::NotFound))?;
    Ok((p, api))
}

/// The groups an account can be given: every group the registered models and commands name.
async fn groups(State(s): Shared, h: HeaderMap) -> Response {
    blocking(move || match admin(&s, &h) {
        Ok((p, _)) if p.groups.contains(obop_commands::ADMIN_GROUP) && !p.mfa_limited => {
            Json(json!({"groups": s.dispatcher.known_groups()})).into_response()
        }
        Ok(_) => mfa_error(MfaError::NotFound),
        Err(r) => r,
    })
    .await
}

async fn create(State(s): Shared, h: HeaderMap, Json(b): Json<J>) -> Response {
    blocking(move || {
        let (p, api) = match admin(&s, &h) {
            Ok(x) => x,
            Err(r) => return r,
        };
        let groups = match group_list(&b["groups"]) {
            Ok(g) => g,
            Err(e) => return mfa_error(e),
        };
        let password = b["password"].as_str().filter(|x| !x.is_empty());
        let new = NewUser {
            name: b["username"].as_str().unwrap_or(""),
            password,
            groups: &groups,
            must_change_password: b["must_change_password"].as_bool().unwrap_or(true),
        };
        api.create_user(&p, &new, &s.dispatcher.known_groups())
            .map_or_else(mfa_error, |j| Json(j).into_response())
    })
    .await
}

async fn set_groups(
    State(s): Shared,
    h: HeaderMap,
    Path(name): Path<String>,
    Json(b): Json<J>,
) -> Response {
    blocking(move || {
        let (p, api) = match admin(&s, &h) {
            Ok(x) => x,
            Err(r) => return r,
        };
        let groups = match group_list(&b["groups"]) {
            Ok(g) => g,
            Err(e) => return mfa_error(e),
        };
        api.set_user_groups(&p, &name, &groups, &s.dispatcher.known_groups())
            .map_or_else(mfa_error, |j| Json(j).into_response())
    })
    .await
}

async fn offboard(
    State(s): Shared,
    h: HeaderMap,
    Path(name): Path<String>,
    body: Option<Json<J>>,
) -> Response {
    blocking(move || {
        let (p, api) = match admin(&s, &h) {
            Ok(x) => x,
            Err(r) => return r,
        };
        let note = body
            .as_ref()
            .and_then(|b| b["note"].as_str())
            .unwrap_or("")
            .to_string();
        api.offboard_user(&p, &name, &note)
            .map_or_else(mfa_error, |j| Json(j).into_response())
    })
    .await
}
