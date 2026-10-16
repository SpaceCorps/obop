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
use obop_commands::{CallError, Dispatcher, Principal, Surface, Value, Vault};
use serde_json::{Value as J, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tower_http::services::{ServeDir, ServeFile};

type Resolver = Arc<dyn Fn(&str) -> Option<Principal> + Send + Sync>;
type Session = Arc<Mutex<Vault>>;

pub struct HttpState {
    dispatcher: Arc<Dispatcher>,
    resolve: Resolver,
    sessions: Mutex<HashMap<String, Session>>,
}

impl HttpState {
    pub fn new(dispatcher: Arc<Dispatcher>, resolve: Resolver) -> Arc<Self> {
        Arc::new(Self {
            dispatcher,
            resolve,
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
        let p = (self.resolve)(token).ok_or(StatusCode::UNAUTHORIZED)?;
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

pub fn router(state: Arc<HttpState>, static_dir: Option<&str>) -> Router {
    let api = Router::new()
        .route("/api/commands", get(commands))
        .route("/api/approve/{plan_id}", post(approve))
        .route("/api/{model}/{command}", post(call))
        .with_state(state);
    match static_dir {
        Some(dir) => api.fallback_service(
            ServeDir::new(dir).fallback(ServeFile::new(format!("{dir}/index.html"))),
        ),
        None => api,
    }
}

async fn commands(State(s): Shared, h: HeaderMap) -> Response {
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
    axum::serve(listener, app).await
}
