//! The MCP Streamable HTTP transport (D51): `POST /mcp` with JSON-RPC, sessions, `DELETE /mcp`,
//! DNS-rebinding protection, size and rate limits, the OAuth protected-resource metadata
//! (RFC 9728) and the MCP server card. All tool logic lives in `obop-mcp`; this module only
//! carries messages, so permissions, masking and approvals are the ones REST uses.
//!
//! Revisions `2025-11-25`, `2025-06-18` and `2025-03-26`; answers are always `application/json`
//! (no SSE stream); `GET /mcp` answers 405 as the specification allows.

use super::shared::{SharedState, token_id};
use super::{HttpState, blocking};
use axum::{
    body::{Body, to_bytes},
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use obop_commands::Principal;
use obop_mcp::{Reply, Session};
use serde_json::{Value as J, json};
use std::sync::Arc;
use std::time::Duration;

const SESSION_HEADER: &str = "mcp-session-id";
const VERSION_HEADER: &str = "mcp-protocol-version";

/// Limits and the origin allow-list of the MCP endpoint.
#[derive(Clone, Debug)]
pub struct McpConfig {
    /// Browser origins allowed to call `/mcp`, besides the instance's own public URL and (for a
    /// loopback host) same-origin. Requests without an `Origin` header are not browsers and pass.
    pub allowed_origins: Vec<String>,
    /// The external URL of the instance (`https://acme.example.com`), when a proxy hides it.
    pub public_url: Option<String>,
    pub max_body_bytes: usize,
    /// Requests per token per minute.
    pub rate_per_minute: u32,
    pub session_ttl: Duration,
    pub max_sessions: usize,
    pub max_sessions_per_token: usize,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            allowed_origins: Vec::new(),
            public_url: None,
            max_body_bytes: 1024 * 1024,
            rate_per_minute: 600,
            session_ttl: Duration::from_secs(2 * 3600),
            max_sessions: 10_000,
            max_sessions_per_token: 25,
        }
    }
}

impl McpConfig {
    /// `OBOP_PUBLIC_URL`, `OBOP_ALLOWED_ORIGINS` (comma separated) and `OBOP_MCP_RATE_PER_MINUTE`.
    pub fn from_env() -> Self {
        let d = Self::default();
        Self {
            public_url: std::env::var("OBOP_PUBLIC_URL")
                .ok()
                .map(|u| u.trim().trim_end_matches('/').to_string())
                .filter(|u| split_origin(u).is_some()),
            // Validated at start-up by `obop serve` (cors::from_env); a bad list allows nothing.
            allowed_origins: crate::cors::from_env().unwrap_or_default(),
            rate_per_minute: std::env::var("OBOP_MCP_RATE_PER_MINUTE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(d.rate_per_minute),
            ..d
        }
    }
}

/// MCP sessions and rate limits live in the storage backend (B1), so a client may send its
/// `Mcp-Session-Id` to any server process on the database. A session is a sealed record (it holds
/// the masking vault) with an expiry that every request pushes out; it is bound to the token that
/// opened it. While a request runs, the session is held under a cross-process lock, so two
/// requests of one session never lose each other's vault entries.
pub(crate) struct McpState {
    cfg: McpConfig,
    shared: Arc<SharedState>,
}

/// A session handed out by `find`: the state, saved back by `finish`.
pub(crate) struct Entry {
    id: String,
    pub state: Session,
    _lock: obop_storage::LockGuard,
}

/// Milliseconds since the epoch, never repeated within the process: sessions opened in the same
/// millisecond still have an order, so the cap drops the one that really is the oldest.
fn now_ms() -> u128 {
    static LAST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    let prev = LAST
        .try_update(
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
            |last| Some(now.max(last + 1)),
        )
        .unwrap_or(0);
    u128::from(now.max(prev + 1))
}

fn session_key(id: &str) -> String {
    format!("mcp:s:{id}")
}

fn index_key(token_id: &str, id: &str) -> String {
    format!("mcp:i:{token_id}:{id}")
}

impl McpState {
    pub(crate) fn new(cfg: McpConfig, shared: Arc<SharedState>) -> Self {
        Self { cfg, shared }
    }

    /// The external base URL: the configured public URL, or scheme and host of the request.
    pub(crate) fn base_url(&self, h: &HeaderMap) -> String {
        if let Some(u) = &self.cfg.public_url {
            return u.clone();
        }
        let host = h
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
            .filter(|h| {
                !h.is_empty()
                    && h.len() <= 255
                    && h.chars()
                        .all(|c| c.is_ascii_alphanumeric() || ".-:[]".contains(c))
            })
            .unwrap_or("localhost");
        let proto = if h.get("x-forwarded-proto").and_then(|v| v.to_str().ok()) == Some("https") {
            "https"
        } else {
            "http"
        };
        format!("{proto}://{host}")
    }

    /// The origin a browser reports for this instance, and the relying party id that goes with
    /// it (the host, no port), for WebAuthn (D65). The configured public URL wins; without one,
    /// behind a proxy you control (`trust_proxy`) the forwarded host and protocol, else `Host`.
    /// Returns `None` when no host can be told.
    pub(crate) fn webauthn_origin(
        &self,
        h: &HeaderMap,
        trust_proxy: bool,
    ) -> Option<(String, String)> {
        let origin = if self.cfg.public_url.is_none()
            && trust_proxy
            && let Some(host) = h
                .get("x-forwarded-host")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.split(',').next())
                .map(str::trim)
                .filter(|v| {
                    !v.is_empty()
                        && v.len() <= 255
                        && v.chars()
                            .all(|c| c.is_ascii_alphanumeric() || ".-:[]".contains(c))
                }) {
            let proto = match h.get("x-forwarded-proto").and_then(|v| v.to_str().ok()) {
                Some(p) if p.split(',').next().map(str::trim) == Some("https") => "https",
                _ => "http",
            };
            format!("{proto}://{host}")
        } else {
            self.base_url(h)
        };
        let origin = origin.trim_end_matches('/').to_ascii_lowercase();
        let (_, authority) = split_origin(&origin)?;
        let rp_id = host_part(authority).to_string();
        (!rp_id.is_empty()).then(|| (origin.clone(), rp_id))
    }

    /// Requests per token per minute, counted across all server processes.
    fn rate_ok(&self, token: &str) -> Result<(), u64> {
        if self.cfg.rate_per_minute == 0 {
            return Ok(());
        }
        obop_storage::limit::hit(
            &*self.shared.backend,
            &format!("rl:mcp:{}", token_id(token)),
            u64::from(self.cfg.rate_per_minute),
            60_000,
        )
    }

    fn ttl_ms(&self) -> u64 {
        self.cfg.session_ttl.as_millis() as u64
    }

    fn save(&self, id: &str, token: &str, session: &Session) -> Result<(), String> {
        let tid = token_id(token);
        let mut doc = session.to_json();
        doc["token"] = J::from(tid);
        let sealed = self.shared.seal_json(&session_key(id), &doc)?;
        self.shared
            .backend
            .kv_put(&session_key(id), &sealed, Some(self.ttl_ms()))
    }

    fn open(&self, token: &str, state: Session) -> Result<String, ()> {
        self.shared.maybe_purge();
        let b = &self.shared.backend;
        let tid = token_id(token);
        let all = b.kv_scan("mcp:i:").map_err(|_| ())?;
        let mine_prefix = format!("mcp:i:{tid}:");
        let mut mine: Vec<(&String, u64)> = all
            .iter()
            .filter(|(k, _)| k.starts_with(&mine_prefix))
            .map(|(k, v)| (k, v.parse().unwrap_or(0)))
            .collect();
        let mut total = all.len();
        if mine.len() >= self.cfg.max_sessions_per_token {
            mine.sort_by_key(|(_, t)| *t);
            if let Some((oldest, _)) = mine.first() {
                let id = oldest.rsplit(':').next().unwrap_or("");
                let _ = b.kv_delete(oldest);
                let _ = b.kv_delete(&session_key(id));
                total = total.saturating_sub(1);
            }
        }
        if total >= self.cfg.max_sessions {
            return Err(());
        }
        let mut raw = [0u8; 32];
        getrandom::fill(&mut raw).expect("operating system randomness");
        let id: String = raw.iter().map(|b| format!("{b:02x}")).collect();
        self.save(&id, token, &state).map_err(|_| ())?;
        b.kv_put(
            &index_key(&tid, &id),
            &now_ms().to_string(),
            Some(self.ttl_ms()),
        )
        .map_err(|_| ())?;
        Ok(id)
    }

    /// The session `id` of this token, locked until `finish`. `None` when unknown, expired or
    /// opened by another token.
    fn find(&self, id: &str, token: &str) -> Option<Entry> {
        // Ids are 64 hex digits; anything else cannot be a key we issued.
        if id.len() != 64 || !id.bytes().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let lock = self
            .shared
            .backend
            .lock(&format!("mcp-session:{id}"))
            .ok()?;
        let sealed = self.shared.backend.kv_get(&session_key(id)).ok()??;
        let doc = self.shared.open_json(&session_key(id), &sealed)?;
        if doc["token"].as_str() != Some(token_id(token).as_str()) {
            return None;
        }
        Some(Entry {
            id: id.to_string(),
            state: Session::from_json(&doc),
            _lock: lock,
        })
    }

    /// Save the session after a request (this also pushes its expiry out) and release it.
    ///
    /// The session carries the mask vault: if it cannot be saved, the answer that was built from it
    /// names tokens nobody stored. The save is tried again, and if it still fails the request is
    /// answered 503 (`applied` says whether a change had committed), never with those tokens.
    fn finish(&self, entry: Entry, token: &str) {
        use obop_storage::fault;
        let pause = std::time::Duration::from_millis(150);
        let saved = fault::patiently(4, pause, || {
            fault::bookkeeping(|| self.save(&entry.id, token, &entry.state))
        });
        // The sliding-expiry marker is an optimisation of the sweep; losing it costs nothing.
        let _ = fault::bookkeeping(|| {
            self.shared.backend.kv_put(
                &index_key(&token_id(token), &entry.id),
                &now_ms().to_string(),
                Some(self.ttl_ms()),
            )
        });
        if let Err(e) = saved {
            eprintln!("mcp: the session could not be saved: {e}");
            if fault::state().wrote {
                fault::mark_partial();
            } else if fault::noted().is_none() {
                fault::note(fault::Fault::Busy);
            }
        }
    }

    fn close(&self, id: &str, token: &str) -> bool {
        let Some(entry) = self.find(id, token) else {
            return false;
        };
        let b = &self.shared.backend;
        let _ = b.kv_delete(&session_key(id));
        let _ = b.kv_delete(&index_key(&token_id(token), id));
        drop(entry);
        true
    }
}

// ---- origins -------------------------------------------------------------------------------

/// `scheme://authority` split, or `None` if `o` is not a bare origin.
fn split_origin(o: &str) -> Option<(&str, &str)> {
    let (scheme, rest) = o.split_once("://")?;
    let bad = |c: char| matches!(c, '/' | '?' | '#' | '@' | ' ');
    (matches!(scheme, "http" | "https") && !rest.is_empty() && !rest.contains(bad))
        .then_some((scheme, rest))
}

fn host_part(authority: &str) -> &str {
    if let Some(rest) = authority.strip_prefix('[') {
        return rest.split(']').next().unwrap_or("");
    }
    authority.split(':').next().unwrap_or("")
}

fn is_loopback(authority: &str) -> bool {
    let h = host_part(authority);
    matches!(h, "localhost" | "127.0.0.1" | "::1") || h.ends_with(".localhost")
}

/// DNS-rebinding protection: an `Origin` header must be on the allow-list, be the instance's
/// public URL, or be same-origin on a loopback or configured host. A page served from a rebound
/// host name is none of these.
pub fn origin_allowed(cfg: &McpConfig, origin: &str, host: Option<&str>) -> bool {
    let o = origin.trim_end_matches('/').to_ascii_lowercase();
    let Some((_, authority)) = split_origin(&o) else {
        return false; // includes "null"
    };
    if cfg.allowed_origins.contains(&o) {
        return true;
    }
    let public = cfg
        .public_url
        .as_deref()
        .map(|u| u.to_ascii_lowercase())
        .and_then(|u| split_origin(&u).map(|(s, a)| (s.to_string(), a.to_string())));
    if let Some((s, a)) = &public
        && format!("{s}://{a}") == o
    {
        return true;
    }
    let Some(host) = host.map(str::to_ascii_lowercase) else {
        return false;
    };
    authority == host && (is_loopback(&host) || public.as_ref().is_some_and(|(_, a)| *a == host))
}

// ---- responses ------------------------------------------------------------------------------

fn rpc_error(status: StatusCode, code: i64, message: &str) -> Response {
    (status, axum::Json(obop_mcp::error(&J::Null, code, message))).into_response()
}

fn with_cors(mut r: Response, origin: Option<&str>) -> Response {
    if let Some(o) = origin.and_then(|o| HeaderValue::from_str(o).ok()) {
        let h = r.headers_mut();
        h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, o);
        h.insert(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            HeaderValue::from_static("Mcp-Session-Id, WWW-Authenticate"),
        );
        h.append(header::VARY, HeaderValue::from_static("Origin"));
    }
    r
}

fn unauthorized(base: &str, presented: bool) -> Response {
    let challenge = if presented {
        format!(
            "Bearer error=\"invalid_token\", error_description=\"unknown, expired, revoked or wrongly addressed token\", resource_metadata=\"{base}/.well-known/oauth-protected-resource/mcp\""
        )
    } else {
        format!("Bearer resource_metadata=\"{base}/.well-known/oauth-protected-resource/mcp\"")
    };
    let mut r = (
        StatusCode::UNAUTHORIZED,
        axum::Json(json!({
            "error": "unauthorized",
            "code": "unauthorized",
            "hint": "Send Authorization: Bearer <token> on every request. Get a token from POST /api/auth/login, or a scoped agent token from POST /api/auth/agent-token.",
        })),
    )
        .into_response();
    if let Ok(v) = HeaderValue::from_str(&challenge) {
        r.headers_mut().insert(header::WWW_AUTHENTICATE, v);
    }
    r
}

fn bearer_token(h: &HeaderMap) -> Option<&str> {
    h.get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.get(..7)
                .filter(|p| p.eq_ignore_ascii_case("bearer "))
                .map(|_| v[7..].trim())
        })
        .filter(|t| !t.is_empty())
}

fn accepts_json(h: &HeaderMap) -> bool {
    match h.get(header::ACCEPT).and_then(|v| v.to_str().ok()) {
        None => true,
        Some(a) => a.split(',').any(|part| {
            let m = part.split(';').next().unwrap_or("").trim();
            matches!(m, "application/json" | "application/*" | "*/*")
        }),
    }
}

// ---- the endpoint ---------------------------------------------------------------------------

enum Gate {
    Pass {
        principal: Principal,
        token: String,
        cors: Option<String>,
    },
    Stop(Response),
}

/// Origin check, authentication and rate limit, shared by POST, GET and DELETE.
fn gate(s: &HttpState, h: &HeaderMap) -> Gate {
    let m = &s.mcp;
    let origin = h.get(header::ORIGIN).and_then(|v| v.to_str().ok());
    if let Some(o) = origin {
        let host = h.get(header::HOST).and_then(|v| v.to_str().ok());
        if !origin_allowed(&m.cfg, o, host) {
            return Gate::Stop(rpc_error(
                StatusCode::FORBIDDEN,
                -32000,
                "Forbidden: Origin not allowed",
            ));
        }
    }
    let cors = origin.map(str::to_string);
    let base = m.base_url(h);
    let Some(token) = bearer_token(h) else {
        return Gate::Stop(with_cors(unauthorized(&base, false), cors.as_deref()));
    };
    let Some((principal, audience)) = s.auth.resolve_with_audience(token) else {
        return Gate::Stop(with_cors(unauthorized(&base, true), cors.as_deref()));
    };
    // A token issued through the OAuth flow is for one resource only (RFC 8707, D64).
    if audience.is_some_and(|a| a != format!("{base}/mcp")) {
        return Gate::Stop(with_cors(unauthorized(&base, true), cors.as_deref()));
    }
    if principal.mfa_limited {
        return Gate::Stop(with_cors(
            s.limited_rejection(&principal).into_response(),
            cors.as_deref(),
        ));
    }
    if let Err(retry) = m.rate_ok(token) {
        let mut r = rpc_error(StatusCode::TOO_MANY_REQUESTS, -32000, "rate limit exceeded");
        if let Ok(v) = HeaderValue::from_str(&retry.max(1).to_string()) {
            r.headers_mut().insert(header::RETRY_AFTER, v);
        }
        return Gate::Stop(with_cors(r, cors.as_deref()));
    }
    Gate::Pass {
        principal,
        token: token.to_string(),
        cors,
    }
}

pub(crate) async fn endpoint(State(s): State<Arc<HttpState>>, req: Request) -> Response {
    let (parts, body) = req.into_parts();
    if parts.method == Method::OPTIONS {
        return preflight(&s, &parts.headers);
    }
    let h = parts.headers.clone();
    let st = s.clone();
    let gated = blocking(move || gate(&st, &h)).await;
    let (principal, token, cors) = match gated {
        Gate::Stop(r) => return r,
        Gate::Pass {
            principal,
            token,
            cors,
        } => (principal, token, cors),
    };
    let r = match parts.method {
        Method::POST => {
            if !accepts_json(&parts.headers) {
                rpc_error(
                    StatusCode::NOT_ACCEPTABLE,
                    -32000,
                    "Not Acceptable: Accept must allow application/json",
                )
            } else if parts
                .headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|c| {
                    !c.trim()
                        .to_ascii_lowercase()
                        .starts_with("application/json")
                })
            {
                rpc_error(
                    StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    -32000,
                    "Unsupported Media Type: Content-Type must be application/json",
                )
            } else {
                match to_bytes(body, s.mcp.cfg.max_body_bytes).await {
                    Err(_) => rpc_error(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        -32600,
                        "request body too large",
                    ),
                    Ok(bytes) => {
                        let h = parts.headers.clone();
                        blocking(move || post_blocking(&s, &principal, &token, &h, &bytes)).await
                    }
                }
            }
        }
        Method::DELETE => {
            let h = parts.headers.clone();
            blocking(move || delete_blocking(&s, &token, &h)).await
        }
        Method::GET => {
            let mut r = rpc_error(
                StatusCode::METHOD_NOT_ALLOWED,
                -32000,
                "Method Not Allowed: this server does not offer a server-to-client stream; POST JSON-RPC messages",
            );
            r.headers_mut().insert(
                header::ALLOW,
                HeaderValue::from_static("POST, DELETE, OPTIONS"),
            );
            r
        }
        _ => {
            let mut r = rpc_error(StatusCode::METHOD_NOT_ALLOWED, -32000, "Method Not Allowed");
            r.headers_mut().insert(
                header::ALLOW,
                HeaderValue::from_static("POST, DELETE, OPTIONS"),
            );
            r
        }
    };
    with_cors(r, cors.as_deref())
}

fn preflight(s: &HttpState, h: &HeaderMap) -> Response {
    let origin = h.get(header::ORIGIN).and_then(|v| v.to_str().ok());
    let host = h.get(header::HOST).and_then(|v| v.to_str().ok());
    if origin.is_some_and(|o| !origin_allowed(&s.mcp.cfg, o, host)) {
        return rpc_error(
            StatusCode::FORBIDDEN,
            -32000,
            "Forbidden: Origin not allowed",
        );
    }
    let mut r = Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header(header::ALLOW, "POST, DELETE, OPTIONS")
        .body(Body::empty())
        .expect("static response");
    if origin.is_some() {
        let hs = r.headers_mut();
        hs.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("POST, DELETE, OPTIONS"),
        );
        hs.insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static(
                "Authorization, Content-Type, Accept, Mcp-Session-Id, Mcp-Protocol-Version, Last-Event-ID",
            ),
        );
        hs.insert(
            header::ACCESS_CONTROL_MAX_AGE,
            HeaderValue::from_static("600"),
        );
    }
    with_cors(r, origin)
}

fn json_response(status: StatusCode, body: &J) -> Response {
    (status, axum::Json(body.clone())).into_response()
}

fn post_blocking(
    s: &HttpState,
    p: &Principal,
    token: &str,
    h: &HeaderMap,
    bytes: &[u8],
) -> Response {
    let payload: J = match serde_json::from_slice(bytes) {
        Ok(v) => v,
        Err(_) => return rpc_error(StatusCode::BAD_REQUEST, -32700, "parse error"),
    };
    if obop_mcp::is_initialize_request(&payload) {
        let mut session = Session::new();
        let reply = obop_mcp::handle_payload(&s.dispatcher, p, &mut session, &payload);
        let Reply::Json(j) = reply else {
            return StatusCode::ACCEPTED.into_response();
        };
        if j.get("error").is_some() {
            return json_response(status_for(&j), &j);
        }
        return match s.mcp.open(token, session) {
            Ok(id) => {
                let mut r = json_response(StatusCode::OK, &j);
                if let Ok(v) = HeaderValue::from_str(&id) {
                    r.headers_mut().insert(SESSION_HEADER, v);
                }
                r
            }
            Err(()) => {
                let mut r = rpc_error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    -32000,
                    "too many open sessions; try again later",
                );
                r.headers_mut()
                    .insert(header::RETRY_AFTER, HeaderValue::from_static("30"));
                r
            }
        };
    }
    if let Some(v) = h.get(VERSION_HEADER).and_then(|v| v.to_str().ok())
        && !obop_mcp::is_supported(v)
    {
        return rpc_error(
            StatusCode::BAD_REQUEST,
            -32600,
            &format!(
                "unsupported MCP-Protocol-Version {v:?}; this server speaks {}",
                obop_mcp::SUPPORTED_VERSIONS.join(", ")
            ),
        );
    }
    let mut entry = match session_entry(s, token, h) {
        Ok(e) => e,
        Err(r) => return *r,
    };
    let reply = obop_mcp::handle_payload(&s.dispatcher, p, &mut entry.state, &payload);
    // A tool call that met a refusal of the store is a tool error with HTTP 200 in the protocol.
    // The 503 layer turns that into the answer REST gives, but only when it knows a step failed
    // after a change committed: say so, so an agent is told "part of it was applied", never just
    // "try again" for a change that was made (D74).
    if let Reply::Json(j) = &reply
        && refused_by_the_store(j)
    {
        obop_storage::fault::mark_partial();
    }
    s.mcp.finish(entry, token);
    match reply {
        Reply::Accepted => StatusCode::ACCEPTED.into_response(),
        Reply::Json(j) => json_response(status_for(&j), &j),
    }
}

/// Is this reply a tool result whose error is the store's refusal (`storage_busy: ...`)?
fn refused_by_the_store(j: &J) -> bool {
    j["result"]["isError"] == true
        && j["result"]["structuredContent"]["error"]
            .as_str()
            .is_some_and(obop_storage::fault::Fault::is_transient)
}

/// HTTP status of a reply: protocol-level rejections of the request itself are 400, everything
/// else (including JSON-RPC errors for a well-formed request) is 200.
fn status_for(j: &J) -> StatusCode {
    match obop_mcp::error_code(j) {
        Some(-32700 | -32600) => StatusCode::BAD_REQUEST,
        _ => StatusCode::OK,
    }
}

fn session_entry(s: &HttpState, token: &str, h: &HeaderMap) -> Result<Entry, Box<Response>> {
    let Some(id) = h.get(SESSION_HEADER).and_then(|v| v.to_str().ok()) else {
        return Err(Box::new(rpc_error(
            StatusCode::BAD_REQUEST,
            -32000,
            "Mcp-Session-Id header required: send initialize first and repeat the id it returns",
        )));
    };
    s.mcp.find(id, token).ok_or_else(|| {
        Box::new(rpc_error(
            StatusCode::NOT_FOUND,
            -32001,
            "Session not found: start a new one with initialize",
        ))
    })
}

fn delete_blocking(s: &HttpState, token: &str, h: &HeaderMap) -> Response {
    let Some(id) = h.get(SESSION_HEADER).and_then(|v| v.to_str().ok()) else {
        return rpc_error(
            StatusCode::BAD_REQUEST,
            -32000,
            "Mcp-Session-Id header required",
        );
    };
    if s.mcp.close(id, token) {
        StatusCode::NO_CONTENT.into_response()
    } else {
        rpc_error(StatusCode::NOT_FOUND, -32001, "Session not found")
    }
}

// ---- discovery documents --------------------------------------------------------------------

pub(crate) fn public_json(content_type: &'static str, doc: &J) -> Response {
    let mut r = (StatusCode::OK, doc.to_string()).into_response();
    let h = r.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=300"),
    );
    h.insert(header::VARY, HeaderValue::from_static("Host"));
    h.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    h.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET"),
    );
    r
}

/// RFC 9728 metadata. `authorization_servers` is named only when this instance runs the
/// authorization server (D64); with `--dev-auth` it is left out rather than faked.
pub(crate) fn protected_resource(base: &str, resource: &str, oauth: bool) -> J {
    let mut doc = json!({
        "resource": resource,
        "resource_name": "obop instance MCP server",
        "bearer_methods_supported": ["header"],
        "resource_documentation": "https://obop.dev/docs/agents/mcp/",
        "x-obop-token-endpoints": {
            "login": format!("{base}/api/auth/login"),
            "agent_token": format!("{base}/api/auth/agent-token"),
        },
    });
    if oauth {
        doc["authorization_servers"] = json!([base]);
    } else {
        doc["$comment"] = J::from(
            "This instance runs without an OAuth authorization server, so authorization_servers is deliberately absent. Present a bearer token from POST /api/auth/login or POST /api/auth/agent-token in the Authorization header of every request.",
        );
    }
    doc
}

pub(crate) async fn oauth_protected_resource(
    State(s): State<Arc<HttpState>>,
    h: HeaderMap,
) -> Response {
    let base = s.mcp.base_url(&h);
    public_json(
        "application/json",
        &protected_resource(&base, &base, s.oauth.is_some()),
    )
}

pub(crate) async fn oauth_protected_resource_mcp(
    State(s): State<Arc<HttpState>>,
    h: HeaderMap,
) -> Response {
    let base = s.mcp.base_url(&h);
    public_json(
        "application/json",
        &protected_resource(&base, &format!("{base}/mcp"), s.oauth.is_some()),
    )
}

/// A host name usable inside the card's `name` (`namespace/name`, namespace `[a-zA-Z0-9.-]`).
fn card_namespace(base: &str) -> String {
    let host = split_origin(base).map_or("localhost", |(_, a)| host_part(a));
    let clean: String = host
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if clean.is_empty() {
        "localhost".into()
    } else {
        clean
    }
}

/// The MCP server card (SEP-2127, an extension still in incubation) plus the plain fields the
/// earlier draft and some scanners look for. Public and identical for every caller.
pub fn server_card(base: &str) -> J {
    let url = format!("{base}/mcp");
    let versions = obop_mcp::SUPPORTED_VERSIONS;
    json!({
        "$schema": "https://static.modelcontextprotocol.io/schemas/v1/server-card.schema.json",
        "$comment": "MCP Server Cards are an extension under incubation (SEP-2127, experimental-ext-server-card); the shape may still change. Fields after _meta are plain convenience fields (serverInfo, transport, endpoint, capabilities, authentication) from the earlier SEP-1649 draft, kept for scanners that read them.",
        "name": format!("{}/obop", card_namespace(base)),
        "version": obop_types::CORE_VERSION,
        "title": "obop instance",
        "description": "Business data and actions of this obop instance as MCP tools, with masking and approvals.",
        "websiteUrl": base,
        "remotes": [{
            "type": "streamable-http",
            "url": url,
            "supportedProtocolVersions": versions,
            "headers": [{
                "name": "Authorization",
                "description": "Bearer token: a scoped agent token from POST /api/auth/agent-token, or a session token from POST /api/auth/login.",
                "isRequired": true,
                "isSecret": true,
                "format": "string",
                "placeholder": "Bearer <token>",
            }],
        }],
        "_meta": {
            "dev.obop/auth": {
                "protectedResourceMetadata": format!("{base}/.well-known/oauth-protected-resource/mcp"),
                "authorizationServer": null,
                "tokenEndpoints": {
                    "login": format!("{base}/api/auth/login"),
                    "agentToken": format!("{base}/api/auth/agent-token"),
                },
            },
        },
        "serverInfo": {"name": "obop", "version": obop_types::CORE_VERSION},
        "transport": {"type": "streamable-http", "endpoint": url},
        "endpoint": url,
        "capabilities": {"tools": {"listChanged": false}},
        "authentication": {
            "type": "bearer",
            "header": "Authorization",
            "description": "Bearer token on every request; no OAuth authorization server yet.",
        },
    })
}

/// Name the authorization server in the card when the instance runs one (D64).
pub(crate) fn with_oauth(mut card: J, base: &str, oauth: bool) -> J {
    if oauth {
        card["_meta"]["dev.obop/auth"]["authorizationServer"] = J::from(base);
        card["_meta"]["dev.obop/auth"]["authorizationServerMetadata"] =
            J::from(format!("{base}/.well-known/oauth-authorization-server"));
        card["authentication"] = json!({
            "type": "oauth2",
            "header": "Authorization",
            "description": "OAuth 2.1 authorization code with PKCE (S256) and dynamic client registration, or a bearer token on every request.",
        });
    }
    card
}

pub(crate) async fn server_card_json(State(s): State<Arc<HttpState>>, h: HeaderMap) -> Response {
    let base = s.mcp.base_url(&h);
    public_json(
        "application/json",
        &with_oauth(server_card(&base), &base, s.oauth.is_some()),
    )
}

pub(crate) async fn server_card_canonical(
    State(s): State<Arc<HttpState>>,
    h: HeaderMap,
) -> Response {
    let base = s.mcp.base_url(&h);
    public_json(
        "application/mcp-server-card+json",
        &with_oauth(server_card(&base), &base, s.oauth.is_some()),
    )
}

/// AI Catalog (draft) pointing at the card, for domain-level discovery.
pub(crate) async fn ai_catalog(State(s): State<Arc<HttpState>>, h: HeaderMap) -> Response {
    let base = s.mcp.base_url(&h);
    let ns = card_namespace(&base);
    public_json(
        "application/ai-catalog+json",
        &json!({
            "specVersion": "1.0",
            "entries": [{
                "identifier": format!("urn:air:obop:{}:mcp", ns.replace('.', "-")),
                "type": "application/mcp-server-card+json",
                "url": format!("{base}/mcp/server-card"),
            }],
        }),
    )
}

/// What `x-mcp` in the OpenAPI document and the `Link` header of `/api/meta` advertise.
pub fn x_mcp(base: &str) -> J {
    json!({
        "url": format!("{base}/mcp"),
        "transport": "streamable-http",
        "protocolVersions": obop_mcp::SUPPORTED_VERSIONS,
        "authorization": "bearer",
        "serverCard": format!("{base}/.well-known/mcp/server-card.json"),
        "protectedResourceMetadata": format!("{base}/.well-known/oauth-protected-resource/mcp"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> McpConfig {
        McpConfig {
            allowed_origins: vec!["https://app.example".into()],
            public_url: Some("https://acme.example.com".into()),
            ..McpConfig::default()
        }
    }

    /// An MCP session on a store that can be made to refuse calls; a request that commits a record
    /// and then saves the session (it carries the mask vault) with the next `n` calls refused.
    fn finish_after_a_commit(n: i64) -> (McpState, obop_storage::fault::Outcome, String) {
        let (flaky, backend) = obop_storage::testing::Flaky::memory();
        let shared = Arc::new(SharedState {
            backend: backend.clone(),
            ring: obop_keys::KeyRing::for_tests(),
        });
        let st = McpState::new(cfg(), shared);
        let id = st.open("obop_agent", Session::new()).unwrap();
        let entry = st.find(&id, "obop_agent").expect("the session is there");
        obop_storage::fault::begin_request(Default::default());
        flaky.refuse_n_after_the_next_write(n);
        backend
            .insert("note", "n1", &obop_storage::Fields::new())
            .unwrap();
        st.finish(entry, "obop_agent");
        (st, obop_storage::fault::end_request(), id)
    }

    #[test]
    fn a_session_save_refused_for_a_moment_is_tried_again() {
        let (st, out, id) = finish_after_a_commit(2);
        assert!(out.wrote && !out.partial, "{out:?}");
        assert!(st.find(&id, "obop_agent").is_some());
    }

    #[test]
    fn a_session_save_that_stays_refused_after_a_commit_is_a_partial_failure() {
        let (_st, out, _id) = finish_after_a_commit(50);
        assert!(out.wrote && out.partial, "{out:?}");
        assert!(out.fault.is_some());
    }

    #[test]
    fn origins_are_checked_against_the_list_public_url_and_loopback() {
        let c = cfg();
        assert!(origin_allowed(
            &c,
            "https://app.example",
            Some("acme.example.com")
        ));
        assert!(origin_allowed(&c, "https://app.example/", None));
        assert!(origin_allowed(
            &c,
            "https://acme.example.com",
            Some("acme.example.com")
        ));
        // Same-origin on a loopback host.
        let none = McpConfig::default();
        assert!(origin_allowed(
            &none,
            "http://localhost:8080",
            Some("localhost:8080")
        ));
        assert!(origin_allowed(
            &none,
            "http://127.0.0.1:1",
            Some("127.0.0.1:1")
        ));
        // DNS rebinding: the page's origin equals the (attacker) host, which is not loopback.
        assert!(!origin_allowed(
            &none,
            "http://evil.test",
            Some("evil.test")
        ));
        assert!(!origin_allowed(
            &none,
            "http://evil.test:8080",
            Some("localhost:8080")
        ));
        assert!(!origin_allowed(
            &c,
            "https://evil.example",
            Some("acme.example.com")
        ));
        assert!(!origin_allowed(&c, "null", None));
        assert!(!origin_allowed(&c, "file://x", None));
        assert!(!origin_allowed(
            &none,
            "http://localhost:1@evil.test",
            Some("localhost:1")
        ));
    }

    #[test]
    fn bearer_tokens_are_parsed_case_insensitively() {
        let mut h = HeaderMap::new();
        h.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("bearer abc"),
        );
        assert_eq!(bearer_token(&h), Some("abc"));
        h.insert(header::AUTHORIZATION, HeaderValue::from_static("Basic abc"));
        assert_eq!(bearer_token(&h), None);
        h.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer "));
        assert_eq!(bearer_token(&h), None);
    }

    #[test]
    fn accept_must_allow_json() {
        let mut h = HeaderMap::new();
        assert!(accepts_json(&h));
        h.insert(
            header::ACCEPT,
            HeaderValue::from_static("application/json, text/event-stream"),
        );
        assert!(accepts_json(&h));
        h.insert(header::ACCEPT, HeaderValue::from_static("*/*"));
        assert!(accepts_json(&h));
        h.insert(
            header::ACCEPT,
            HeaderValue::from_static("text/event-stream"),
        );
        assert!(!accepts_json(&h));
    }

    #[test]
    fn the_card_name_follows_the_schema_pattern() {
        let c = server_card("https://acme-1.sliplane.app");
        assert_eq!(c["name"], "acme-1.sliplane.app/obop");
        assert_eq!(c["remotes"][0]["url"], "https://acme-1.sliplane.app/mcp");
        assert!(c["description"].as_str().unwrap().len() <= 100);
        assert_eq!(server_card("http://[::1]:8080")["name"], "--1/obop");
    }
}
