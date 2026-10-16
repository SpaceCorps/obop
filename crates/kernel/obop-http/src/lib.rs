//! REST surface (axum) over the one `Dispatcher`. Only this crate depends on axum (D29).
//!
//! `POST /api/{model}/{command}` with a JSON body, `GET /api/commands`, and
//! `POST /api/approve/{plan_id}`. Authentication is a bearer token resolved by the caller.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use obop_commands::{Authenticator, CallError, Dispatcher, Principal, Surface, Value, Vault};
use serde_json::{Value as J, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tower_http::services::{ServeDir, ServeFile};

pub mod community;
pub mod public;
pub mod website;

type Session = Arc<Mutex<Vault>>;

pub struct HttpState {
    dispatcher: Arc<Dispatcher>,
    auth: Arc<dyn Authenticator>,
    sessions: Mutex<HashMap<String, Session>>,
}

impl HttpState {
    pub fn new(dispatcher: Arc<Dispatcher>, auth: Arc<dyn Authenticator>) -> Arc<Self> {
        Arc::new(Self {
            dispatcher,
            auth,
            sessions: Mutex::new(HashMap::new()),
        })
    }

    /// One vault per bearer token, so mask tokens are stable inside a session (D40).
    fn auth(&self, h: &HeaderMap) -> Result<(Principal, Session), StatusCode> {
        let token = h
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(StatusCode::UNAUTHORIZED)?;
        let p = self.auth.resolve(token).ok_or(StatusCode::UNAUTHORIZED)?;
        let s = self
            .sessions
            .lock()
            .unwrap()
            .entry(token.to_string())
            .or_default()
            .clone();
        Ok((p, s))
    }
}

type Shared = State<Arc<HttpState>>;

/// Storage backends block (the PostgreSQL client runs its own runtime, which panics if used on an
/// async worker), so every handler that touches the dispatcher, the authenticator or the platform
/// runs on the blocking pool.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(f)
        .await
        .expect("handler panicked")
}

/// The REST routes under `/api`, without any static or public fallback.
pub(crate) fn api_routes(state: Arc<HttpState>) -> Router {
    Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/agent-token", post(agent_token))
        .route("/api/commands", get(commands))
        .route("/api/meta", get(meta))
        .route("/api/approve/{plan_id}", post(approve))
        .route("/api/{model}/{command}", post(call))
        .with_state(state)
}

pub fn router(state: Arc<HttpState>, static_dir: Option<&str>) -> Router {
    let api = api_routes(state);
    match static_dir {
        Some(dir) => api.fallback_service(
            ServeDir::new(dir).fallback(ServeFile::new(format!("{dir}/index.html"))),
        ),
        None => api,
    }
}

async fn login(State(s): Shared, Json(body): Json<J>) -> Response {
    blocking(move || login_blocking(s, body)).await
}

fn login_blocking(s: Arc<HttpState>, body: J) -> Response {
    let (u, p) = (
        body["username"].as_str().unwrap_or(""),
        body["password"].as_str().unwrap_or(""),
    );
    match s.auth.login(u, p) {
        Ok(token) => Json(json!({"token": token})).into_response(),
        Err(e) if e.contains("too many") => {
            (StatusCode::TOO_MANY_REQUESTS, Json(json!({"error": e}))).into_response()
        }
        Err(e) => (StatusCode::UNAUTHORIZED, Json(json!({"error": e}))).into_response(),
    }
}

/// A signed-in human issues a scoped token for an agent. The token is shown once.
async fn agent_token(State(s): Shared, h: HeaderMap, Json(body): Json<J>) -> Response {
    blocking(move || agent_token_blocking(s, h, body)).await
}

fn agent_token_blocking(s: Arc<HttpState>, h: HeaderMap, body: J) -> Response {
    let (p, _) = match s.auth(&h) {
        Ok(x) => x,
        Err(c) => return c.into_response(),
    };
    let scopes: Vec<String> = body["scopes"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    match s
        .auth
        .issue_agent_token(&p, &scopes, body["label"].as_str().unwrap_or("agent"))
    {
        Ok(token) => Json(json!({"token": token})).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response(),
    }
}

async fn meta(State(s): Shared, h: HeaderMap) -> Response {
    blocking(move || meta_blocking(s, h)).await
}

/// Everything the UI renders from: modules, models, readable fields and actions for this caller.
fn meta_blocking(s: Arc<HttpState>, h: HeaderMap) -> Response {
    let (p, _) = match s.auth(&h) {
        Ok(x) => x,
        Err(c) => return c.into_response(),
    };
    let modules: Vec<J> = s
        .dispatcher
        .meta(Surface::Rest, &p)
        .into_iter()
        .map(|m| {
            json!({
                "id": m.id,
                "name": m.name,
                "models": m.models.iter().map(|x| json!({
                    "name": x.name,
                    "description": x.description,
                    "can_create": x.can_create,
                    "can_update": x.can_update,
                    "can_delete": x.can_delete,
                    "fields": x.fields.iter().map(|f| {
                        let mut o = json!({
                            "name": f.name, "description": f.description,
                            "class": f.class, "personal_data": f.personal_data,
                            "kind": f.kind, "required": f.required,
                        });
                        if f.kind == "select" {
                            o["options"] = json!(f.options);
                        }
                        if let Some(to) = f.to {
                            o["to"] = json!(to);
                        }
                        o
                    }).collect::<Vec<_>>(),
                    "actions": x.actions.iter().map(|a| json!({
                        "id": a.id, "description": a.description, "needs_approval": a.needs_approval,
                        "field": a.transition.map(|t| t.field),
                        "from": a.transition.and_then(|t| t.from),
                        "to": a.transition.map(|t| t.to),
                    })).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    Json(json!({"modules": modules})).into_response()
}

async fn commands(State(s): Shared, h: HeaderMap) -> Response {
    blocking(move || commands_blocking(s, h)).await
}

fn commands_blocking(s: Arc<HttpState>, h: HeaderMap) -> Response {
    let (p, _) = match s.auth(&h) {
        Ok(x) => x,
        Err(c) => return c.into_response(),
    };
    let list: Vec<J> = s
        .dispatcher
        .catalog(Surface::Rest, &p)
        .into_iter()
        .map(|c| json!({"id": c.id, "description": c.description, "needs_approval": c.needs_approval}))
        .collect();
    Json(json!({"commands": list})).into_response()
}

async fn call(
    State(s): Shared,
    h: HeaderMap,
    Path((model, command)): Path<(String, String)>,
    body: Option<Json<J>>,
) -> Response {
    blocking(move || call_blocking(s, h, model, command, body)).await
}

fn call_blocking(
    s: Arc<HttpState>,
    h: HeaderMap,
    model: String,
    command: String,
    body: Option<Json<J>>,
) -> Response {
    let (p, session) = match s.auth(&h) {
        Ok(x) => x,
        Err(c) => return c.into_response(),
    };
    let input = Value::from_json(&body.map_or(json!({}), |Json(j)| j));
    let id = format!("{model}.{command}");
    let result = s
        .dispatcher
        .call(Surface::Rest, &p, &mut session.lock().unwrap(), &id, &input);
    respond(result)
}

async fn approve(State(s): Shared, h: HeaderMap, Path(plan_id): Path<String>) -> Response {
    blocking(move || approve_blocking(s, h, plan_id)).await
}

fn approve_blocking(s: Arc<HttpState>, h: HeaderMap, plan_id: String) -> Response {
    let (p, session) = match s.auth(&h) {
        Ok(x) => x,
        Err(c) => return c.into_response(),
    };
    respond(
        s.dispatcher
            .approve(&p, &mut session.lock().unwrap(), &plan_id),
    )
}

fn respond(r: Result<obop_commands::Response, CallError>) -> Response {
    match r {
        Ok(r) => Json(json!({"value": r.value.to_json(), "denied": r.denied})).into_response(),
        Err(CallError::ApprovalRequired { plan_id }) => (
            StatusCode::ACCEPTED,
            Json(json!({"approval_required": true, "plan_id": plan_id})),
        )
            .into_response(),
        // Unknown, hidden and forbidden look the same, so a caller cannot probe for commands.
        Err(
            CallError::Forbidden(_) | CallError::UnknownCommand(_) | CallError::NotOnSurface(_),
        ) => (StatusCode::NOT_FOUND, Json(json!({"error": "not found"}))).into_response(),
        Err(CallError::UnknownPlan(_)) => {
            (StatusCode::NOT_FOUND, Json(json!({"error": "not found"}))).into_response()
        }
        Err(CallError::Restore(_)) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid token"})),
        )
            .into_response(),
        Err(CallError::Handler(m)) => {
            (StatusCode::BAD_REQUEST, Json(json!({"error": m}))).into_response()
        }
    }
}

pub async fn serve(addr: &str, app: Router) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    // The peer address is what the public site keys its rate limits on.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
}

/// What the control-plane routes need from the platform (implemented by `obop-platform`).
pub trait PlatformApi: Send + Sync {
    fn signup(&self, email: &str, password: &str) -> Result<String, String>;
    fn login(&self, email: &str, password: &str) -> Result<String, String>;
    fn me(&self, token: &str) -> Result<String, String>;
    fn list(&self, token: &str) -> Result<Vec<J>, String>;
    fn create(&self, token: &str, name: &str) -> Result<J, String>;
    fn delete(&self, token: &str, id: &str) -> Result<J, String>;
}

type Api = State<Arc<dyn PlatformApi>>;

fn bearer(h: &HeaderMap) -> &str {
    h.get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("")
}

fn err(status: StatusCode, e: String) -> Response {
    (status, Json(json!({"error": e}))).into_response()
}

/// Routes of the control plane: sign up, sign in and manage your instances.
pub fn platform_router(api: Arc<dyn PlatformApi>, static_dir: Option<&str>) -> Router {
    let r = Router::new()
        .route("/api/platform/signup", post(p_signup))
        .route("/api/platform/login", post(p_login))
        .route("/api/platform/me", get(p_me))
        .route("/api/platform/instances", get(p_list).post(p_create))
        .route("/api/platform/instances/{id}/delete", post(p_delete))
        .with_state(api);
    match static_dir {
        // The control plane serves the marketing site: real 404s, clean-URL redirects and
        // Markdown for agents (docs/site/SITE.md, "Server requirements").
        Some(dir) => r.fallback_service(website::service(dir.into())),
        None => r,
    }
}

async fn p_signup(State(api): Api, Json(b): Json<J>) -> Response {
    blocking(move || p_signup_blocking(api, b)).await
}

fn p_signup_blocking(api: Arc<dyn PlatformApi>, b: J) -> Response {
    match api.signup(
        b["email"].as_str().unwrap_or(""),
        b["password"].as_str().unwrap_or(""),
    ) {
        Ok(t) => Json(json!({"token": t})).into_response(),
        Err(e) => err(StatusCode::BAD_REQUEST, e),
    }
}

async fn p_login(State(api): Api, Json(b): Json<J>) -> Response {
    blocking(move || p_login_blocking(api, b)).await
}

fn p_login_blocking(api: Arc<dyn PlatformApi>, b: J) -> Response {
    match api.login(
        b["email"].as_str().unwrap_or(""),
        b["password"].as_str().unwrap_or(""),
    ) {
        Ok(t) => Json(json!({"token": t})).into_response(),
        Err(e) if e.contains("too many") => err(StatusCode::TOO_MANY_REQUESTS, e),
        Err(e) => err(StatusCode::UNAUTHORIZED, e),
    }
}

async fn p_me(State(api): Api, h: HeaderMap) -> Response {
    blocking(move || p_me_blocking(api, h)).await
}

fn p_me_blocking(api: Arc<dyn PlatformApi>, h: HeaderMap) -> Response {
    match api.me(bearer(&h)) {
        Ok(email) => Json(json!({"email": email})).into_response(),
        Err(e) => err(StatusCode::UNAUTHORIZED, e),
    }
}

async fn p_list(State(api): Api, h: HeaderMap) -> Response {
    blocking(move || p_list_blocking(api, h)).await
}

fn p_list_blocking(api: Arc<dyn PlatformApi>, h: HeaderMap) -> Response {
    match api.list(bearer(&h)) {
        Ok(l) => Json(json!({"instances": l})).into_response(),
        Err(e) => err(StatusCode::UNAUTHORIZED, e),
    }
}

async fn p_create(State(api): Api, h: HeaderMap, Json(b): Json<J>) -> Response {
    blocking(move || p_create_blocking(api, h, b)).await
}

fn p_create_blocking(api: Arc<dyn PlatformApi>, h: HeaderMap, b: J) -> Response {
    match api.create(bearer(&h), b["name"].as_str().unwrap_or("")) {
        Ok(i) => (StatusCode::ACCEPTED, Json(i)).into_response(),
        Err(e) if e == "not signed in" => err(StatusCode::UNAUTHORIZED, e),
        Err(e) => err(StatusCode::BAD_REQUEST, e),
    }
}

async fn p_delete(State(api): Api, h: HeaderMap, Path(id): Path<String>) -> Response {
    blocking(move || p_delete_blocking(api, h, id)).await
}

fn p_delete_blocking(api: Arc<dyn PlatformApi>, h: HeaderMap, id: String) -> Response {
    match api.delete(bearer(&h), &id) {
        Ok(i) => (StatusCode::ACCEPTED, Json(i)).into_response(),
        Err(e) if e == "not signed in" => err(StatusCode::UNAUTHORIZED, e),
        Err(e) if e == "no such instance" => err(StatusCode::NOT_FOUND, e),
        Err(e) => err(StatusCode::BAD_REQUEST, e),
    }
}
