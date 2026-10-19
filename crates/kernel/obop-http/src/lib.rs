//! REST surface (axum) over the one `Dispatcher`. Only this crate depends on axum (D29).
//!
//! `POST /api/{model}/{command}` with a JSON body, `GET /api/commands`, and
//! `POST /api/approve/{plan_id}`. Authentication is a bearer token resolved by the caller.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{any, get, post},
};
use obop_commands::{Authenticator, CallError, Dispatcher, Principal, Surface, Value};
use serde_json::{Value as J, json};
use std::sync::Arc;
use tower_http::services::{ServeDir, ServeFile};

pub mod blobs;
pub mod community;
pub mod connectors;
pub mod mcp;
pub mod mfa;
pub mod oauth;
pub mod openapi;
pub mod payments;
pub mod public;
pub(crate) mod shared;
pub mod website;

pub struct HttpState {
    dispatcher: Arc<Dispatcher>,
    auth: Arc<dyn Authenticator>,
    /// Vaults and MCP sessions live in the storage backend, shared by every server process.
    shared: Arc<shared::SharedState>,
    mcp: mcp::McpState,
    /// The OAuth authorization server (D64); `None` when the authenticator cannot issue its tokens.
    oauth: Option<Arc<obop_oauth::OAuth>>,
    /// Take the client address from `X-Forwarded-For` (behind a proxy you control).
    trust_proxy: std::sync::atomic::AtomicBool,
    /// The connectors engines, set once by the composition root (webhooks, OAuth redirect).
    connectors: std::sync::OnceLock<Arc<obop_connectors::Runtime>>,
    /// The payment module's webhook door (`/hooks/pay/{provider}`), set by the composition root.
    payments: std::sync::OnceLock<Arc<dyn obop_commands::PaymentHooks>>,
}

impl HttpState {
    pub fn new(dispatcher: Arc<Dispatcher>, auth: Arc<dyn Authenticator>) -> Arc<Self> {
        Self::with_mcp(dispatcher, auth, mcp::McpConfig::from_env())
    }

    /// As `new`, with explicit limits and origin allow-list for `/mcp` (tests, embedding).
    pub fn with_mcp(
        dispatcher: Arc<Dispatcher>,
        auth: Arc<dyn Authenticator>,
        mcp: mcp::McpConfig,
    ) -> Arc<Self> {
        let shared = shared::SharedState::for_dispatcher(&dispatcher);
        let oauth = obop_oauth::OAuth::new(
            shared.backend.clone(),
            auth.clone(),
            obop_oauth::OAuthConfig::default(),
        );
        Arc::new(Self {
            dispatcher,
            auth,
            oauth,
            trust_proxy: std::sync::atomic::AtomicBool::new(false),
            mcp: mcp::McpState::new(mcp, shared.clone()),
            shared,
            connectors: std::sync::OnceLock::new(),
            payments: std::sync::OnceLock::new(),
        })
    }

    /// Why a limited session is refused: it owes a new password (an administrator reset it) or
    /// two-factor enrolment.
    pub(crate) fn limited_rejection(&self, p: &Principal) -> AuthRejection {
        let owes_password = self
            .auth
            .mfa()
            .and_then(|m| m.status(p).ok())
            .is_some_and(|s| s["password_change_required"] == true);
        if owes_password {
            AuthRejection::PasswordChangeRequired
        } else {
            AuthRejection::MfaEnrolmentRequired
        }
    }

    /// Key the rate limits of the OAuth endpoints on the last `X-Forwarded-For` entry.
    pub fn set_trust_proxy(&self, on: bool) {
        self.trust_proxy
            .store(on, std::sync::atomic::Ordering::Relaxed);
    }

    /// Serve the payment providers' webhook door.
    pub fn set_payments(&self, hooks: Arc<dyn obop_commands::PaymentHooks>) {
        let _ = self.payments.set(hooks);
    }

    /// Serve the connectors' public doors (`/hooks/in/{id}`, `/oauth/callback`).
    pub fn set_connectors(&self, rt: Arc<obop_connectors::Runtime>) {
        let _ = self.connectors.set(rt);
    }

    /// One vault per bearer token, so mask tokens are stable inside a session (D40); it lives in
    /// the shared store, so the same token works on any server process. A session limited to
    /// two-factor enrolment (D53) is refused here with 403 `mfa_enrolment_required`.
    fn auth(&self, h: &HeaderMap) -> Result<(Principal, shared::VaultRef), AuthRejection> {
        let token = h
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(AuthRejection::Unauthorized)?;
        let (p, audience) = self
            .auth
            .resolve_with_audience(token)
            .ok_or(AuthRejection::Unauthorized)?;
        // A token issued by the OAuth flow is for /mcp only (RFC 8707, D64).
        if audience.is_some() {
            return Err(AuthRejection::Unauthorized);
        }
        if p.mfa_limited {
            return Err(self.limited_rejection(&p));
        }
        let agent = p.kind == obop_commands::PrincipalKind::Agent;
        Ok((p, self.shared.vault(token, agent)))
    }
}

type Shared = State<Arc<HttpState>>;

/// Why a bearer token did not get through.
pub(crate) enum AuthRejection {
    Unauthorized,
    MfaEnrolmentRequired,
    PasswordChangeRequired,
}

impl IntoResponse for AuthRejection {
    fn into_response(self) -> Response {
        match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED.into_response(),
            Self::MfaEnrolmentRequired => mfa::enrolment_required(),
            Self::PasswordChangeRequired => mfa::password_change_required(),
        }
    }
}

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
        .route("/api/openapi.json", get(instance_openapi))
        .route("/api/approve/{plan_id}", post(approve))
        .route(
            "/hooks/in/{id}",
            post(connectors::hook).layer(axum::extract::DefaultBodyLimit::max(
                obop_connectors::webhook::MAX_BODY,
            )),
        )
        .route("/oauth/callback", get(connectors::oauth_callback))
        .route(
            "/hooks/pay/{provider}",
            post(payments::hook).layer(axum::extract::DefaultBodyLimit::max(payments::MAX_BODY)),
        )
        .route(
            "/api/blobs",
            post(blobs::upload).layer(axum::extract::DefaultBodyLimit::disable()),
        )
        .route("/api/blobs/{id}", get(blobs::download))
        .route("/api/{model}/{command}", post(call))
        .route("/mcp", any(mcp::endpoint))
        .route("/mcp/server-card", get(mcp::server_card_canonical))
        .route(
            "/.well-known/mcp/server-card.json",
            get(mcp::server_card_json),
        )
        .route("/.well-known/ai-catalog.json", get(mcp::ai_catalog))
        .route(
            "/.well-known/oauth-protected-resource",
            get(mcp::oauth_protected_resource),
        )
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(mcp::oauth_protected_resource_mcp),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            get(oauth::authorization_server),
        )
        .route(
            "/oauth/register",
            post(oauth::register)
                .options(oauth::preflight)
                .layer(body_limit()),
        )
        .route(
            "/oauth/token",
            post(oauth::token)
                .options(oauth::preflight)
                .layer(body_limit()),
        )
        .route(
            "/oauth/revoke",
            post(oauth::revoke)
                .options(oauth::preflight)
                .layer(body_limit()),
        )
        .route("/oauth/authorize", get(oauth::authorize))
        .route(
            "/oauth/authorize/login",
            post(oauth::login).layer(body_limit()),
        )
        .route("/oauth/authorize/mfa", post(oauth::mfa).layer(body_limit()))
        .route(
            "/oauth/authorize/decision",
            post(oauth::decision).layer(body_limit()),
        )
        .route("/api/auth/oauth/grants", get(oauth::list_grants))
        .route(
            "/api/auth/oauth/grants/{id}/revoke",
            post(oauth::revoke_grant),
        )
        .with_state(state.clone())
        .merge(mfa::routes("/api/auth", state.auth.clone()))
}

fn body_limit() -> axum::extract::DefaultBodyLimit {
    axum::extract::DefaultBodyLimit::max(oauth::MAX_BODY)
}

/// Unknown API paths answer JSON, never the HTML shell, so agents can parse the error.
async fn api_not_found() -> Response {
    api_error(
        StatusCode::NOT_FOUND,
        "not found",
        "not_found",
        "The API is described at /api/openapi.json.",
    )
}

fn api_error(status: StatusCode, message: &str, code: &str, hint: &str) -> Response {
    (
        status,
        Json(json!({"error": message, "code": code, "hint": hint})),
    )
        .into_response()
}

async fn instance_openapi(State(s): Shared, h: HeaderMap) -> Response {
    blocking(move || instance_openapi_blocking(s, h)).await
}

/// The instance's OpenAPI document for the caller: with a valid token it lists the commands that
/// token may use; without one it lists only sign-in and the discovery endpoints.
fn instance_openapi_blocking(s: Arc<HttpState>, h: HeaderMap) -> Response {
    let base = s.mcp.base_url(&h);
    let modules = match s.auth(&h) {
        Ok((p, _)) => s.dispatcher.meta(Surface::Rest, &p),
        Err(_) => Vec::new(),
    };
    let mut doc = openapi::instance(&base, &modules);
    if s.oauth.is_some() {
        doc["x-mcp"]["authorization"] = J::from("oauth2 or bearer");
        doc["x-mcp"]["authorizationServerMetadata"] =
            J::from(format!("{base}/.well-known/oauth-authorization-server"));
    }
    let mut r = Json(doc).into_response();
    r.headers_mut().insert(
        axum::http::header::VARY,
        axum::http::HeaderValue::from_static("Authorization"),
    );
    r
}

pub fn router(state: Arc<HttpState>, static_dir: Option<&str>) -> Router {
    let api = api_routes(state);
    match static_dir {
        Some(dir) => api.fallback_service(spa_with_json_api_404(dir)),
        None => api.fallback(api_not_found),
    }
}

/// The back office single-page app, except that an unknown `/api/...` path answers JSON (agents
/// cannot parse the HTML shell) instead of falling through to it.
fn spa_with_json_api_404(dir: &str) -> Router {
    use tower::ServiceExt;
    let files = ServeDir::new(dir).fallback(ServeFile::new(format!("{dir}/index.html")));
    Router::new().fallback(move |req: axum::extract::Request| {
        let files = files.clone();
        async move {
            if req.uri().path().starts_with("/api/") {
                return api_not_found().await;
            }
            files.oneshot(req).await.into_response()
        }
    })
}

async fn login(State(s): Shared, Json(body): Json<J>) -> Response {
    blocking(move || login_blocking(s, body)).await
}

fn login_blocking(s: Arc<HttpState>, body: J) -> Response {
    let (u, p) = (
        body["username"].as_str().unwrap_or(""),
        body["password"].as_str().unwrap_or(""),
    );
    s.shared.maybe_purge();
    match s.auth.login_step(u, p) {
        Ok(out) => mfa::outcome(out),
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
    let mut r = Json(obop_mcp::meta_json(&s.dispatcher, Surface::Rest, &p)).into_response();
    let base = s.mcp.base_url(&h);
    if let Ok(v) = axum::http::HeaderValue::from_str(&format!(
        "<{base}/mcp>; rel=\"mcp\", <{base}/.well-known/mcp/server-card.json>; rel=\"mcp-server-card\""
    )) {
        r.headers_mut().insert(axum::http::header::LINK, v);
    }
    r
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
    match session.with(|vault| s.dispatcher.call(Surface::Rest, &p, vault, &id, &input)) {
        Ok(result) => respond(result),
        Err(e) => unavailable(&e),
    }
}

async fn approve(State(s): Shared, h: HeaderMap, Path(plan_id): Path<String>) -> Response {
    blocking(move || approve_blocking(s, h, plan_id)).await
}

fn approve_blocking(s: Arc<HttpState>, h: HeaderMap, plan_id: String) -> Response {
    let (p, session) = match s.auth(&h) {
        Ok(x) => x,
        Err(c) => return c.into_response(),
    };
    match session.with(|vault| s.dispatcher.approve(&p, vault, &plan_id)) {
        Ok(result) => respond(result),
        Err(e) => unavailable(&e),
    }
}

/// The shared store could not be reached or a lock was not granted in time.
fn unavailable(why: &str) -> Response {
    eprintln!("shared state unavailable: {why}");
    api_error(
        StatusCode::SERVICE_UNAVAILABLE,
        "the server is busy, try again",
        "unavailable",
        "A lock or the database did not answer in time. Retry the request.",
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
    /// First sign-in step; a second factor may be demanded (D53).
    fn login_step(
        &self,
        email: &str,
        password: &str,
    ) -> Result<obop_commands::LoginOutcome, String> {
        self.login(email, password)
            .map(|token| obop_commands::LoginOutcome::Session {
                token,
                mfa_enrolment_required: false,
                password_change_required: false,
            })
    }
    /// The authenticator behind the accounts, for the optional two-factor routes (D53).
    fn authenticator(&self) -> Option<Arc<dyn Authenticator>> {
        None
    }
    fn me(&self, token: &str) -> Result<String, String>;
    fn list(&self, token: &str) -> Result<Vec<J>, String>;
    fn create(&self, token: &str, name: &str) -> Result<J, String>;
    fn delete(&self, token: &str, id: &str) -> Result<J, String>;
    /// The published core releases (D63). Public.
    fn releases(&self) -> Result<J, String> {
        Err("this control plane publishes no releases".into())
    }
    /// The state of an instance's update, for the holder of its update token (D63).
    fn update_status(&self, _update_token: &str) -> Result<J, String> {
        Err("this control plane does not run updates".into())
    }
    /// Ask to be moved to a published release.
    fn update_start(
        &self,
        _update_token: &str,
        _version: &str,
        _from_version: &str,
    ) -> Result<J, String> {
        Err("this control plane does not run updates".into())
    }
    /// Ask to go back to the image before the last update.
    fn update_rollback(&self, _update_token: &str) -> Result<J, String> {
        Err("this control plane does not run updates".into())
    }
}

type Api = State<Arc<dyn PlatformApi>>;

fn bearer(h: &HeaderMap) -> &str {
    h.get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("")
}

fn err(status: StatusCode, e: String) -> Response {
    let (code, hint) = match status {
        StatusCode::UNAUTHORIZED => ("unauthorized", "Sign in, or pass a valid bearer token."),
        StatusCode::NOT_FOUND => (
            "not_found",
            "Check the id, or the API description at /openapi.json.",
        ),
        StatusCode::TOO_MANY_REQUESTS => ("rate_limited", "Wait a minute and try again."),
        StatusCode::BAD_REQUEST => (
            "invalid_input",
            "Fix the input described in the error and retry.",
        ),
        _ => ("error", "See the API description at /openapi.json."),
    };
    (
        status,
        Json(json!({"error": e, "code": code, "hint": hint})),
    )
        .into_response()
}

async fn platform_openapi(headers: HeaderMap) -> Response {
    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost");
    let proto = if headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        == Some("https")
    {
        "https"
    } else {
        "http"
    };
    Json(openapi::platform(&format!("{proto}://{host}"))).into_response()
}

/// Routes of the control plane: sign up, sign in and manage your instances.
pub fn platform_router(api: Arc<dyn PlatformApi>, static_dir: Option<&str>) -> Router {
    let r = Router::new()
        .route("/api/platform/signup", post(p_signup))
        .route("/api/platform/login", post(p_login))
        .route("/api/platform/me", get(p_me))
        .route("/api/platform/instances", get(p_list).post(p_create))
        .route("/api/platform/instances/{id}/delete", post(p_delete))
        .route("/api/releases", get(p_releases))
        .route("/api/platform/releases", get(p_releases))
        .route("/api/platform/update/status", get(p_update_status))
        .route("/api/platform/update/start", post(p_update_start))
        .route("/api/platform/update/rollback", post(p_update_rollback))
        .with_state(api.clone())
        .route("/openapi.json", get(platform_openapi))
        .route("/api/openapi.json", get(platform_openapi));
    // Optional TOTP for the owner's own account (D53): same routes as an instance, not enforced.
    let r = match api.authenticator() {
        Some(a) => r.merge(mfa::routes("/api/platform", a)),
        None => r,
    };
    match static_dir {
        // The control plane serves the marketing site: real 404s, clean-URL redirects and
        // Markdown for agents (docs/site/SITE.md, "Server requirements").
        Some(dir) => r.fallback_service(website::service(dir.into())),
        None => r,
    }
}

/// The release feed instances read (D63): public, cacheable for a minute.
async fn p_releases(State(api): Api) -> Response {
    blocking(move || match api.releases() {
        Ok(j) => ([("cache-control", "public, max-age=60")], Json(j)).into_response(),
        Err(e) => err(StatusCode::NOT_FOUND, e),
    })
    .await
}

/// What an update token's holder may do. The status code tells the cases apart without
/// revealing which part of a token was wrong.
fn update_answer(r: Result<J, String>) -> Response {
    match r {
        Ok(j) => Json(j).into_response(),
        Err(e) if e.contains("not a valid update token") => err(StatusCode::UNAUTHORIZED, e),
        Err(e) if e.contains("too many requests") => err(StatusCode::TOO_MANY_REQUESTS, e),
        Err(e) if e.contains("already") => err(StatusCode::CONFLICT, e),
        Err(e) => err(StatusCode::BAD_REQUEST, e),
    }
}

async fn p_update_status(State(api): Api, h: HeaderMap) -> Response {
    blocking(move || update_answer(api.update_status(bearer(&h)))).await
}

async fn p_update_start(State(api): Api, h: HeaderMap, Json(b): Json<J>) -> Response {
    blocking(move || {
        // Only a published version may be named: there is no field for an image.
        update_answer(api.update_start(
            bearer(&h),
            b["version"].as_str().unwrap_or(""),
            b["from_version"].as_str().unwrap_or(""),
        ))
    })
    .await
}

async fn p_update_rollback(State(api): Api, h: HeaderMap) -> Response {
    blocking(move || update_answer(api.update_rollback(bearer(&h)))).await
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
    match api.login_step(
        b["email"].as_str().unwrap_or(""),
        b["password"].as_str().unwrap_or(""),
    ) {
        Ok(out) => mfa::outcome(out),
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
