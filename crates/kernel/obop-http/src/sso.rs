//! Sign-in with Microsoft Entra over HTTP (D91, ratified by OR-38): the browser round trip of the
//! authorization code flow and the two calls this server makes to the provider (its token
//! endpoint, and the discovery document and key set it reads). Everything that decides who the
//! person is lives behind `SsoApi` in `obop-auth`; this module carries requests, keeps the state
//! of a flow, and never mints a session or reads a claim.
//!
//! The routes, all under `/api/auth` and none of them behind a bearer token:
//!
//! - `GET /sso`: `{"enabled": bool}`, whether the sign-in page shows the button.
//! - `GET /oidc/start`: begins a flow and redirects the browser to the provider. The flow's state,
//!   nonce and a hash of the PKCE verifier are kept in the shared store for ten minutes, under the
//!   hash of the flow id (the `state` parameter). The verifier itself is the value of an HttpOnly
//!   cookie of the browser that started the flow, so it is stored nowhere on the server.
//! - `GET /oidc/callback`: the provider's redirect. The flow is spent first (one try per flow),
//!   the cookie must be the one of the flow, the code is exchanged at the token endpoint, the id
//!   token is verified and dropped (nothing the provider sends is stored), and the browser is
//!   sent to the fixed address `/#oidc=<one-time code>`. Every failure is a redirect to
//!   `/#oidc_error=<cancelled|unavailable|busy|failed>` and nothing else: the reason is in the
//!   security log, not in the address bar.
//! - `POST /oidc/exchange`: `{"code"}` for the answer of the password step (a bearer token, or the
//!   demand for an obop second factor). The code is single use, lives 60 seconds and is worth
//!   something only to the browser that holds the `obop_sso_done` cookie the callback set.
//!
//! A round trip that an app's sign-in page began (`GET /oauth/authorize/microsoft`, in `oauth.rs`)
//! uses the same start-up, the same registered redirect address and this callback. Its stored row
//! says `purpose: "oauth"` and names the OAuth flow, and the callback then ends differently: it
//! hands the verified identity to the OAuth flow by id (`OAuth::microsoft_done`: the person, or one
//! of a fixed set of words, never a code, a token or a claim) and sends the browser to
//! `/oauth/authorize/return?flow=<id>`. The back office's fragment, its one-time code and its
//! cookies are not used for it.
//!
//! There is no cookie session: the bearer token goes to the page and stays in `sessionStorage`
//! as after a password. There is no `next` or return address; the target of the redirect is the
//! root of this origin and cannot be chosen by a request.

use super::oauth::{client_ip, cookie_value, harden, pair, parse_pairs, see_other, with_cookies};
use super::{HttpState, api_error, blocking, mfa};
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use obop_commands::{AnonLine, MfaError, SsoApi, SsoEndpoints, SsoError, SsoIdentity, SsoLeg};
use obop_connectors::net::Request;
use obop_connectors::oauth::pkce_pair;
use obop_keys::{hex, random};
use obop_oauth::MicrosoftOutcome;
use obop_oauth::util::{ct_eq, pct};
use serde_json::{Value as J, json};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

type Shared = State<Arc<HttpState>>;

/// The size of the exchange body (a one-time code).
pub(crate) const MAX_BODY: usize = 4 * 1024;

const FLOW_PREFIX: &str = "auth.sso.flow:";
const FLOW_TTL_MS: u64 = 10 * 60 * 1000;
const FLOW_COOKIE: &str = "obop_sso_";
const DONE_COOKIE: &str = "obop_sso_done";
const DONE_SECS: u64 = 60;
const COOKIE_PATH: &str = "/api/auth/oidc";
const DISCOVERY_TTL: Duration = Duration::from_secs(10 * 60);
const JWKS_TTL: Duration = Duration::from_secs(60 * 60);
/// How long a failed fetch of the discovery document or the key set is answered from memory
/// before Microsoft is asked again: a provider that does not answer is not asked once per request.
const NEGATIVE_TTL: Duration = Duration::from_secs(45);
/// The most calls to Microsoft that one server process has in the air at once (the fetches of
/// the discovery document and the key set, and the exchange of a code; each holds one place only
/// while its own call is in the air). More are refused at once and hold no thread: the exchange of
/// a code answers "busy", the fetches fail as "unavailable".
const MAX_IN_FLIGHT: usize = 8;
/// The most requests that wait for the fetch of the discovery document or the key set that
/// another request has in the air. More are refused at once ("unavailable"), holding no thread.
const MAX_WAITING: usize = 8;
/// How long a call to Microsoft may take once its connection is being made, and how long a
/// request waits for the fetch another request has in the air (the egress client's own limit is
/// 20 seconds, which is for connectors; a person is not kept waiting that long). The system
/// resolver is not under this limit: `Client::send_within` resolves the host name first, and a
/// resolver that hangs holds the thread of the request that is resolving it. Of those there are
/// at most `MAX_IN_FLIGHT`, and `MAX_WAITING` more that wait up to this long: at most sixteen
/// blocking threads of the server belong to the Microsoft sign-in at any time, plus, for the
/// moment between taking the right to fetch and asking for a place (which is refused at once when
/// none is free), the two requests that hold the right of the discovery document and of the key
/// set: at most eighteen.
const SSO_TIMEOUT: Duration = Duration::from_secs(6);
/// How often the key set may be fetched again because a token named a key the cache lacks
/// (all server processes together). The token comes only in the provider's own answer to the
/// exchange of a code, so no caller can bring a key name of its own: the cap is for a provider
/// that rotates its keys or misbehaves, so that this server does not hammer it.
const JWKS_REFETCH_PER_MIN: u64 = 6;

/// What this process remembers of the provider: the checked addresses of its discovery document
/// and its key set. Both are only caches; a restart or a change of the configuration empties them.
#[derive(Default)]
pub(crate) struct Cache {
    discovery: Mutex<Option<(String, Instant, SsoEndpoints)>>,
    jwks: Mutex<Option<(String, Instant, Vec<u8>)>>,
    /// The last failure of each fetch, with the time and the key it was for.
    discovery_failed: Mutex<Option<(String, Instant, String)>>,
    jwks_failed: Mutex<Option<(String, Instant, String)>>,
    /// One fetch of each at a time: the others wait for its answer (and find it in the cache).
    discovery_flight: Mutex<()>,
    jwks_flight: Mutex<()>,
    in_flight: std::sync::atomic::AtomicUsize,
    /// Requests that wait for a fetch another request has in the air.
    waiting: std::sync::atomic::AtomicUsize,
    /// The limit of a call, in milliseconds; 0 is `SSO_TIMEOUT` (a test shortens it).
    timeout_ms: std::sync::atomic::AtomicU64,
}

/// A place among the requests that wait for a fetch; given back when dropped.
struct Waiting<'a>(&'a std::sync::atomic::AtomicUsize);

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// One of the calls to Microsoft this process may have in the air; given back when dropped.
pub(crate) struct Permit<'a>(&'a std::sync::atomic::AtomicUsize);

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

impl HttpState {
    /// For tests: shorten the limit of a call to Microsoft (and of the wait for another request's
    /// call), so that a slow stand-in does not make a test wait six seconds.
    #[doc(hidden)]
    pub fn set_sso_timeout_for_test(&self, limit: Duration) {
        self.sso.timeout_ms.store(
            limit.as_millis().max(1) as u64,
            std::sync::atomic::Ordering::SeqCst,
        );
    }

    /// For tests: take the right to fetch the discovery document, as a fetch that is stuck in the
    /// air holds it (the limit of a call does not cover the system resolver, so a fetch can be
    /// stuck for longer than the limit). Given back when the guard is dropped.
    #[doc(hidden)]
    pub fn hold_sso_discovery_fetch_for_test(&self) -> std::sync::MutexGuard<'_, ()> {
        self.sso
            .discovery_flight
            .lock()
            .unwrap_or_else(|p| p.into_inner())
    }

    /// For tests: the same for the fetch of the key set.
    #[doc(hidden)]
    pub fn hold_sso_key_set_fetch_for_test(&self) -> std::sync::MutexGuard<'_, ()> {
        self.sso
            .jwks_flight
            .lock()
            .unwrap_or_else(|p| p.into_inner())
    }

    /// For tests: how many requests wait now for a fetch another request has in the air (a test
    /// that holds a fetch lets go of it when the places for waiting are taken, not after a guess
    /// of how long that takes on a busy machine).
    #[doc(hidden)]
    pub fn sso_waiting_for_test(&self) -> usize {
        self.sso.waiting.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// For tests: forget the discovery document, the key set and the remembered failures, as ten
    /// minutes (or a restart) would.
    #[doc(hidden)]
    pub fn forget_sso_cache_for_test(&self) {
        for slot in [&self.sso.discovery_failed, &self.sso.jwks_failed] {
            if let Ok(mut g) = slot.lock() {
                *g = None;
            }
        }
        if let Ok(mut g) = self.sso.discovery.lock() {
            *g = None;
        }
        if let Ok(mut g) = self.sso.jwks.lock() {
            *g = None;
        }
    }
}

fn failed_recently(slot: &Mutex<Option<(String, Instant, String)>>, key: &str) -> Option<String> {
    let g = slot.lock().ok()?;
    let (k, at, why) = g.as_ref()?;
    (k == key && at.elapsed() < NEGATIVE_TTL).then(|| why.clone())
}

fn remember_failure(slot: &Mutex<Option<(String, Instant, String)>>, key: &str, why: &str) {
    if let Ok(mut g) = slot.lock() {
        *g = Some((key.to_string(), Instant::now(), why.to_string()));
    }
}

fn cache_key(leg: &SsoLeg) -> String {
    format!("{}|{}", leg.authority, leg.tenant)
}

/// The host of an authority, as the egress client's allow-list wants it: no scheme, no port, no
/// brackets of an IPv6 literal.
pub(crate) fn host_of(authority: &str) -> String {
    let rest = authority.split("://").nth(1).unwrap_or("");
    let hostport = rest.split('/').next().unwrap_or("");
    let host = match hostport.strip_prefix('[') {
        Some(h) => h.split(']').next().unwrap_or(""),
        None => hostport.rsplit_once(':').map_or(hostport, |(h, _)| h),
    };
    host.to_ascii_lowercase()
}

/// The twelve `error` codes that the log keeps by name, taken from those of OAuth 2.0 (RFC 6749
/// sections 4.1.2.1 and 5.2), of OpenID Connect and of the errors that Microsoft documents for
/// its two endpoints. Any other code, also a documented one such as `unsupported_response_type`
/// or `invalid_resource`, is logged as `other`.
const KNOWN_ERRORS: [&str; 12] = [
    "access_denied",
    "interaction_required",
    "login_required",
    "consent_required",
    "invalid_request",
    "invalid_scope",
    "invalid_client",
    "invalid_grant",
    "unauthorized_client",
    "unsupported_grant_type",
    "server_error",
    "temporarily_unavailable",
];

/// What the log keeps of an `error` that came back to the callback or from the token endpoint: one
/// word out of [`KNOWN_ERRORS`], or `other`. The code in the query of the callback is whatever
/// the caller wrote, so no text of it reaches a line or a key; the description is never read.
pub(crate) fn error_class(code: &str) -> &'static str {
    KNOWN_ERRORS
        .iter()
        .find(|k| **k == code)
        .copied()
        .unwrap_or("other")
}

/// The address a request is counted and logged under: an IPv6 address by its /64 (one subscriber
/// has 2^64 of them), an IPv4-mapped one as the IPv4 address, anything else as it is.
fn source_bucket(addr: &str) -> String {
    match addr.parse::<std::net::Ipv6Addr>() {
        Ok(a) => match a.to_ipv4_mapped() {
            Some(v4) => v4.to_string(),
            None => {
                let g = a.segments();
                format!("{:x}:{:x}:{:x}:{:x}::/64", g[0], g[1], g[2], g[3])
            }
        },
        Err(_) => addr.to_string(),
    }
}

pub(crate) fn source_of(s: &HttpState, parts: &Parts) -> String {
    source_bucket(&client_ip(s, parts))
}

pub(crate) fn describe(e: &SsoError) -> String {
    match e {
        SsoError::Refused(m) | SsoError::Invalid(m) | SsoError::Unavailable(m) => m.clone(),
        SsoError::UnknownKey => "the signing key is not known".into(),
        SsoError::NotFound => "not allowed".into(),
    }
}

impl Cache {
    /// A place among the calls to Microsoft this process has in the air, or `Err`: too many at
    /// once, and nothing waits for one.
    pub(crate) fn permit(&self) -> Result<Permit<'_>, String> {
        use std::sync::atomic::Ordering;
        if self.in_flight.fetch_add(1, Ordering::SeqCst) >= MAX_IN_FLIGHT {
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            return Err("too many calls to the provider are in the air".into());
        }
        Ok(Permit(&self.in_flight))
    }

    /// The limit of a call to Microsoft, and of the wait for another request's call.
    pub(crate) fn timeout(&self) -> Duration {
        match self.timeout_ms.load(std::sync::atomic::Ordering::SeqCst) {
            0 => SSO_TIMEOUT,
            ms => Duration::from_millis(ms),
        }
    }

    /// The right to fetch: one request at a time holds `lock`. A request that finds it taken waits
    /// for the fetch in the air, at most `wait`, and at most `MAX_WAITING` requests wait at all;
    /// the others get an error at once and hold no thread. (A request that gets in finds the
    /// answer of the fetch it waited for in the cache, good or bad.)
    fn one_at_a_time<'a>(
        &self,
        lock: &'a Mutex<()>,
        wait: Duration,
    ) -> Result<std::sync::MutexGuard<'a, ()>, String> {
        use std::sync::TryLockError;
        use std::sync::atomic::Ordering;
        match lock.try_lock() {
            Ok(g) => return Ok(g),
            Err(TryLockError::Poisoned(p)) => return Ok(p.into_inner()),
            Err(TryLockError::WouldBlock) => {}
        }
        if self.waiting.fetch_add(1, Ordering::SeqCst) >= MAX_WAITING {
            self.waiting.fetch_sub(1, Ordering::SeqCst);
            return Err("too many requests are waiting for the provider".into());
        }
        let _place = Waiting(&self.waiting);
        let until = Instant::now() + wait;
        loop {
            match lock.try_lock() {
                Ok(g) => return Ok(g),
                Err(TryLockError::Poisoned(p)) => return Ok(p.into_inner()),
                Err(TryLockError::WouldBlock) => {
                    if Instant::now() >= until {
                        return Err(
                            "the provider's answer to another request did not come in time".into(),
                        );
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
    }

    pub(crate) fn discovery(
        &self,
        api: &dyn SsoApi,
        rt: &obop_connectors::Runtime,
        leg: &SsoLeg,
    ) -> Result<SsoEndpoints, String> {
        let key = cache_key(leg);
        let cached = || {
            self.discovery.lock().ok().and_then(|g| {
                g.as_ref()
                    .filter(|(k, at, _)| *k == key && at.elapsed() < DISCOVERY_TTL)
                    .map(|(_, _, ep)| ep.clone())
            })
        };
        if let Some(ep) = cached() {
            return Ok(ep);
        }
        if let Some(why) = failed_recently(&self.discovery_failed, &key) {
            return Err(format!("{why} (not tried again yet)"));
        }
        // One fetch at a time: the requests that wait find its answer, good or bad, when they get in.
        let _one = self.one_at_a_time(&self.discovery_flight, self.timeout())?;
        if let Some(ep) = cached() {
            return Ok(ep);
        }
        if let Some(why) = failed_recently(&self.discovery_failed, &key) {
            return Err(format!("{why} (not tried again yet)"));
        }
        let _permit = self.permit()?;
        let url = format!(
            "{}/{}/v2.0/.well-known/openid-configuration",
            leg.authority, leg.tenant
        );
        let got = fetch(rt, &url, &host_of(&leg.authority), self.timeout())
            .and_then(|body| api.endpoints(&body).map_err(|e| describe(&e)));
        match got {
            Ok(ep) => {
                if let Ok(mut g) = self.discovery.lock() {
                    *g = Some((key, Instant::now(), ep.clone()));
                }
                Ok(ep)
            }
            Err(e) => {
                remember_failure(&self.discovery_failed, &key, &e);
                Err(e)
            }
        }
    }

    pub(crate) fn jwks(
        &self,
        rt: &obop_connectors::Runtime,
        backend: &dyn obop_storage::Backend,
        leg: &SsoLeg,
        ep: &SsoEndpoints,
        force: bool,
    ) -> Result<Vec<u8>, String> {
        let key = cache_key(leg);
        let cached = || {
            if force {
                return None;
            }
            self.jwks.lock().ok().and_then(|g| {
                g.as_ref()
                    .filter(|(k, at, _)| *k == key && at.elapsed() < JWKS_TTL)
                    .map(|(_, _, doc)| doc.clone())
            })
        };
        if let Some(doc) = cached() {
            return Ok(doc);
        }
        if let Some(why) = failed_recently(&self.jwks_failed, &key) {
            return Err(format!("{why} (not tried again yet)"));
        }
        let _one = self.one_at_a_time(&self.jwks_flight, self.timeout())?;
        if let Some(doc) = cached() {
            return Ok(doc);
        }
        if let Some(why) = failed_recently(&self.jwks_failed, &key) {
            return Err(format!("{why} (not tried again yet)"));
        }
        if force
            && obop_storage::limit::hit(backend, "rl:sso:jwks", JWKS_REFETCH_PER_MIN, 60_000)
                .is_err()
        {
            return Err("the key set was fetched too often; not fetched again".into());
        }
        let _permit = self.permit()?;
        match fetch(rt, &ep.jwks, &host_of(&leg.authority), self.timeout()) {
            Ok(doc) => {
                if let Ok(mut g) = self.jwks.lock() {
                    *g = Some((key, Instant::now(), doc.clone()));
                }
                Ok(doc)
            }
            Err(e) => {
                remember_failure(&self.jwks_failed, &key, &e);
                Err(e)
            }
        }
    }

    pub(crate) fn forget_jwks(&self) {
        if let Ok(mut g) = self.jwks.lock() {
            *g = None;
        }
    }
}

/// A GET through the hardened egress client. The text of an error has no URL, header or body in it.
fn fetch(
    rt: &obop_connectors::Runtime,
    url: &str,
    host: &str,
    limit: Duration,
) -> Result<Vec<u8>, String> {
    let r = rt
        .client
        .send_within(
            Request {
                method: "GET",
                url,
                headers: vec![("Accept".into(), "application/json".into())],
                body: None,
                allowed_hosts: &[host.to_string()],
            },
            limit,
        )
        .map_err(|e| e.to_string())?;
    if r.status != 200 {
        return Err(format!("the provider answered {}", r.status));
    }
    Ok(r.body)
}

pub(crate) fn limited(s: &HttpState, what: &str, ip: &str, max: u64) -> bool {
    obop_storage::limit::hit(
        &*s.shared.backend,
        &format!("rl:sso:{what}:{ip}"),
        max,
        60_000,
    )
    .is_err()
}

pub(crate) fn flow_key(flow: &str) -> String {
    format!("{FLOW_PREFIX}{}", hex(&Sha256::digest(flow.as_bytes())))
}

pub(crate) fn hash_of(secret: &str) -> String {
    hex(&Sha256::digest(
        format!("obop-sso-verifier\n{secret}").as_bytes(),
    ))
}

pub(crate) fn flow_cookie(flow: &str) -> String {
    format!("{FLOW_COOKIE}{}", flow.get(..12).unwrap_or("x"))
}

pub(crate) fn set_cookie(name: &str, value: &str, max_age: u64, secure: bool) -> String {
    format!(
        "{name}={value}; Max-Age={max_age}; Path={COOKIE_PATH}; HttpOnly; SameSite=Lax{}",
        if secure { "; Secure" } else { "" }
    )
}

/// The redirect of every failure: the root of this origin and a fixed word.
pub(crate) fn back(s: &HttpState, error: &str, cookies: &[String]) -> Response {
    with_cookies(
        see_other(&format!("{}#oidc_error={error}", s.sso_return())),
        cookies,
    )
}

pub(crate) fn secure_of(redirect_uri: &str) -> bool {
    redirect_uri.starts_with("https://")
}

/// Whether the request is a navigation of the browser (a click, a typed address, a redirect), and
/// not an image, a script or a `fetch` that some page made. A browser says so in `Sec-Fetch-Mode`
/// and `Sec-Fetch-Dest`; a client that sends neither (a script, an older browser) passes, as it
/// could not be told from the person's own click.
pub(crate) fn is_navigation(parts: &Parts) -> bool {
    let get = |n: &str| parts.headers.get(n).and_then(|v| v.to_str().ok());
    get("sec-fetch-mode").is_none_or(|m| m == "navigate")
        && get("sec-fetch-dest").is_none_or(|d| d == "document")
}

pub(crate) fn not_a_navigation() -> Response {
    let mut r = (
        StatusCode::BAD_REQUEST,
        "this address is for the sign-in page only\n",
    )
        .into_response();
    r.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

pub(crate) fn is_flow_id(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit())
}

// ---- GET /api/auth/sso ----------------------------------------------------------------------

pub(crate) async fn status(State(s): Shared) -> Response {
    blocking(move || {
        let enabled = s.connectors.get().is_some() && s.auth.sso().is_some_and(|a| a.offered());
        let mut r = axum::Json(json!({"enabled": enabled})).into_response();
        r.headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        r
    })
    .await
}

// ---- the round trip, in pieces both callers use -----------------------------------------------

/// A step of the round trip that ended it: the one word the person is told (the fixed words of
/// the redirect: `cancelled`, `unavailable`, `busy`, `failed`) and the line the security log gets.
/// The text is fixed or built from a word out of a fixed set, never from what the caller sent; the
/// sort is one of the closed set of [`AnonLine`], which decides the budget the line is counted
/// against. Built here, written by [`Refusal::log`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refusal {
    pub(crate) word: &'static str,
    pub(crate) kind: &'static str,
    pub(crate) line: AnonLine,
    pub(crate) text: String,
}

impl Refusal {
    fn new(word: &'static str, kind: &'static str, line: AnonLine, text: String) -> Self {
        Self {
            word,
            kind,
            line,
            text,
        }
    }

    /// Write the line (at most one per address, kind and sort in ten minutes) and hand back the
    /// word for the person.
    pub(crate) fn log(&self, api: &dyn SsoApi, ip: &str) -> &'static str {
        api.audit_throttled(ip, self.kind, self.line, &self.text);
        self.word
    }
}

/// The address of the provider's page that begins a sign-in. Fixed: `openid profile` and nothing
/// else, the PKCE challenge, and no `prompt`, `max_age` or `login_hint` (D91).
pub(crate) fn authorization_url(
    authorization: &str,
    client_id: &str,
    redirect_uri: &str,
    flow: &str,
    nonce: &str,
    challenge: &str,
) -> String {
    format!(
        "{authorization}?client_id={}&response_type=code&redirect_uri={}&response_mode=query&scope=openid%20profile&state={flow}&nonce={nonce}&code_challenge={challenge}&code_challenge_method=S256",
        pct(client_id),
        pct(redirect_uri),
    )
}

/// A flow that was begun: where the browser goes, and the cookie that holds the PKCE verifier.
pub(crate) struct Begun {
    pub(crate) url: String,
    pub(crate) cookie: String,
}

/// Begin a flow: the state, nonce and a hash of the verifier are kept in the shared store for ten
/// minutes under the hash of the flow id; the verifier itself is the value of the cookie.
/// `extra` adds fields to the stored row (the OAuth leg names its purpose and flow there; the
/// back office's own flow has none). `None` when the store refused the row.
pub(crate) fn begin_flow(
    s: &HttpState,
    leg: &SsoLeg,
    ep: &SsoEndpoints,
    redirect_uri: &str,
    extra: Option<J>,
) -> Option<Begun> {
    let (verifier, challenge) = pkce_pair();
    let flow = hex(&random::<32>());
    let nonce = hex(&random::<24>());
    // The row expires in the store after ten minutes and says so itself: a store that keeps an
    // expired row (a lazy clean-up) does not make an old flow good again.
    let mut rec = json!({
        "nonce": nonce,
        "redirect_uri": redirect_uri,
        "verifier_hash": hash_of(&verifier),
        "expires_at_ms": now_ms() + u128::from(FLOW_TTL_MS),
    });
    if let (Some(J::Object(more)), Some(row)) = (extra, rec.as_object_mut()) {
        for (k, v) in more {
            row.insert(k, v);
        }
    }
    s.shared
        .backend
        .kv_put(&flow_key(&flow), &rec.to_string(), Some(FLOW_TTL_MS))
        .ok()?;
    Some(Begun {
        url: authorization_url(
            &ep.authorization,
            &leg.client_id,
            redirect_uri,
            &flow,
            &nonce,
            &challenge,
        ),
        cookie: set_cookie(
            &flow_cookie(&flow),
            &verifier,
            FLOW_TTL_MS / 1000,
            secure_of(redirect_uri),
        ),
    })
}

/// The discovery document of the pinned tenant, through the process-wide cache. `what` names the
/// caller in the log line (`start`, `callback`).
pub(crate) fn discover(
    s: &HttpState,
    api: &dyn SsoApi,
    rt: &obop_connectors::Runtime,
    leg: &SsoLeg,
    what: &str,
) -> Result<SsoEndpoints, Refusal> {
    s.sso.discovery(api, rt, leg).map_err(|e| {
        Refusal::new(
            "unavailable",
            "oidc_error",
            AnonLine::Discovery,
            format!("{what}: the discovery document could not be used: {e}"),
        )
    })
}

/// The provider's own `error` on the redirect to the callback. Only the person's refusal is a
/// cancellation; any other answer is the provider's error, and is logged and worded as one.
pub(crate) fn provider_answer(code: &str) -> Refusal {
    let err = error_class(code);
    let cancelled = err == "access_denied";
    Refusal::new(
        if cancelled { "cancelled" } else { "failed" },
        if cancelled {
            "oidc_cancelled"
        } else {
            "oidc_error"
        },
        AnonLine::Provider,
        format!("the provider ended the sign-in with {err:?}"),
    )
}

/// The provider's settings for a callback: switched off meanwhile, or unreadable.
pub(crate) fn callback_leg(got: Result<Option<SsoLeg>, SsoError>) -> Result<SsoLeg, Refusal> {
    match got {
        Ok(Some(l)) => Ok(l),
        Ok(None) => Err(Refusal::new(
            "failed",
            "oidc_refused",
            AnonLine::SwitchedOff,
            "the provider was switched off during the sign-in".into(),
        )),
        Err(e) => Err(Refusal::new(
            "unavailable",
            "oidc_error",
            AnonLine::Settings,
            format!("callback: {}", describe(&e)),
        )),
    }
}

/// What the token endpoint answered, as the id token it carried or the reason there is none. 429
/// and 5xx: the provider is not well, it has not said no. Like the other outages (the fetches, an
/// endpoint that cannot be reached) that is "try again", not "ask the administrator".
pub(crate) fn id_token_of(status: u16, body: &[u8]) -> Result<String, Refusal> {
    if status == 429 || status >= 500 {
        return Err(Refusal::new(
            "unavailable",
            "oidc_error",
            AnonLine::TokenUnwell,
            format!("the token endpoint answered {status}"),
        ));
    }
    let answer: J = serde_json::from_slice(body).unwrap_or(J::Null);
    let id_token = answer["id_token"].as_str().unwrap_or("");
    if status != 200 || id_token.is_empty() {
        return Err(Refusal::new(
            "failed",
            "oidc_refused",
            AnonLine::TokenRefused,
            format!(
                "the token endpoint answered {status}{}",
                answer["error"]
                    .as_str()
                    .map(|e| format!(" {:?}", error_class(e)))
                    .unwrap_or_default()
            ),
        ));
    }
    Ok(id_token.to_string())
}

/// Redeem the code at the token endpoint: from this server, with the client secret and the
/// verifier. Returns the id token; nothing else of the answer is read.
pub(crate) fn redeem(
    s: &HttpState,
    rt: &obop_connectors::Runtime,
    leg: &SsoLeg,
    ep: &SsoEndpoints,
    code: &str,
    redirect_uri: &str,
    verifier: &str,
) -> Result<String, Refusal> {
    let form = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&client_secret={}&code_verifier={}",
        pct(code),
        pct(redirect_uri),
        pct(&leg.client_id),
        pct(&leg.client_secret),
        pct(verifier),
    );
    // The place among the calls in the air is held for this call only: the key set, which may
    // need a place of its own, is fetched after it is given back.
    let resp = {
        let Ok(_permit) = s.sso.permit() else {
            return Err(Refusal::new(
                "busy",
                "oidc_error",
                AnonLine::Busy,
                "too many calls to the provider are in the air; this sign-in was not tried".into(),
            ));
        };
        rt.client.send_within(
            Request {
                method: "POST",
                url: &ep.token,
                headers: vec![
                    (
                        "Content-Type".into(),
                        "application/x-www-form-urlencoded".into(),
                    ),
                    ("Accept".into(), "application/json".into()),
                ],
                body: Some(form.into_bytes()),
                allowed_hosts: &[host_of(&leg.authority)],
            },
            s.sso.timeout(),
        )
    };
    let resp = resp.map_err(|e| {
        Refusal::new(
            "unavailable",
            "oidc_error",
            AnonLine::TokenUnreachable,
            format!("the token endpoint could not be reached: {e}"),
        )
    })?;
    id_token_of(resp.status, &resp.body)
}

/// Verify an id token against a key set, fetching the key set again once when the token names a
/// key the cached one lacks (the provider rotated its keys). The three closures are what this
/// needs of the world: `fetch(force)` gives the key set (`force`: not from the cache), `verify`
/// checks a token against one, `forget` drops the cached one (it could not be used).
pub(crate) fn verify_with_refetch(
    mut fetch: impl FnMut(bool) -> Result<Vec<u8>, String>,
    verify: impl Fn(&[u8]) -> Result<SsoIdentity, SsoError>,
    forget: impl Fn(),
) -> Result<SsoIdentity, Refusal> {
    let key_set = |text: String| Refusal::new("unavailable", "oidc_error", AnonLine::KeySet, text);
    let mut jwks =
        fetch(false).map_err(|e| key_set(format!("the key set could not be fetched: {e}")))?;
    let mut verified = verify(&jwks);
    if verified == Err(SsoError::UnknownKey) {
        // The provider rotated its keys: fetch the set again, once, and look again.
        jwks = fetch(true)
            .map_err(|e| key_set(format!("the key set could not be fetched again: {e}")))?;
        verified = verify(&jwks);
    }
    match verified {
        Ok(i) => Ok(i),
        Err(SsoError::Unavailable(m)) => {
            forget();
            Err(Refusal::new(
                "unavailable",
                "oidc_error",
                AnonLine::KeySetUnusable,
                format!("the key set could not be used: {m}"),
            ))
        }
        Err(e) => Err(Refusal::new(
            "failed",
            "oidc_refused",
            AnonLine::IdToken,
            format!("the id token was refused: {}", describe(&e)),
        )),
    }
}

/// Everything between the redirect of the provider and a verified identity: the token endpoint,
/// the key set, the checks of the id token.
#[allow(clippy::too_many_arguments)]
pub(crate) fn identity_of(
    s: &HttpState,
    api: &dyn SsoApi,
    rt: &obop_connectors::Runtime,
    leg: &SsoLeg,
    ep: &SsoEndpoints,
    code: &str,
    redirect_uri: &str,
    verifier: &str,
    nonce: &str,
) -> Result<SsoIdentity, Refusal> {
    let id_token = redeem(s, rt, leg, ep, code, redirect_uri, verifier)?;
    verify_with_refetch(
        |force| s.sso.jwks(rt, &*s.shared.backend, leg, ep, force),
        |jwks| api.verify(&id_token, jwks, nonce),
        || s.sso.forget_jwks(),
    )
}

// ---- GET /api/auth/oidc/start ---------------------------------------------------------------

pub(crate) async fn start(State(s): Shared, parts: Parts) -> Response {
    blocking(move || start_blocking(&s, &parts)).await
}

fn start_blocking(s: &HttpState, parts: &Parts) -> Response {
    if !is_navigation(parts) {
        return not_a_navigation();
    }
    let ip = source_of(s, parts);
    s.shared.maybe_purge();
    if limited(s, "start", &ip, 30) {
        return back(s, "busy", &[]);
    }
    let (Some(api), Some(rt)) = (s.auth.sso(), s.connectors.get()) else {
        return back(s, "unavailable", &[]);
    };
    let leg = match api.leg() {
        Ok(Some(l)) => l,
        Ok(None) => return back(s, "unavailable", &[]),
        Err(e) => {
            api.audit_throttled(
                &ip,
                "oidc_error",
                AnonLine::Settings,
                &format!("start: {}", describe(&e)),
            );
            return back(s, "unavailable", &[]);
        }
    };
    let ep = match discover(s, api, rt, &leg, "start") {
        Ok(ep) => ep,
        Err(r) => return back(s, r.log(api, &ip), &[]),
    };
    let redirect_uri = format!("{}/api/auth/oidc/callback", s.mcp.base_url(&parts.headers));
    let Some(begun) = begin_flow(s, &leg, &ep, &redirect_uri, None) else {
        return back(s, "unavailable", &[]);
    };
    // One line per address and ten minutes, not one per request: a start is a free call.
    api.audit_throttled(
        &ip,
        "oidc_start",
        AnonLine::Start,
        "a sign-in with Microsoft was begun",
    );
    with_cookies(see_other(&begun.url), &[begun.cookie])
}

// ---- GET /api/auth/oidc/callback ------------------------------------------------------------

pub(crate) async fn callback(State(s): Shared, parts: Parts) -> Response {
    blocking(move || callback_blocking(&s, &parts)).await
}

/// Spend the flow: read it and delete it, and trust it only if the delete was ours.
pub(crate) fn take_flow(s: &HttpState, flow: &str) -> Option<J> {
    take_flow_from(&*s.shared.backend, flow)
}

/// [`take_flow`] on a store: the delete is the claim, so of several processes that read the same
/// row (D59) exactly one is told it spent the flow.
pub(crate) fn take_flow_from(backend: &dyn obop_storage::Backend, flow: &str) -> Option<J> {
    let key = flow_key(flow);
    let raw = backend.kv_get(&key).ok().flatten();
    let claimed = backend.kv_delete(&key).unwrap_or(false);
    if !claimed {
        return None;
    }
    let rec: J = serde_json::from_str(&raw?).ok()?;
    // A row with no expiry, or past it, is no flow.
    let until = u128::from(rec["expires_at_ms"].as_u64()?);
    (until > now_ms()).then_some(rec)
}

pub(crate) fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

/// Where a callback ends: the back office (a fixed word in the fragment of its root) or, for a
/// round trip that an app's sign-in page began, the return address of that flow after the word has
/// been written into it.
struct Ending<'a> {
    s: &'a HttpState,
    /// The OAuth flow of an app's round trip (`purpose == "oauth"`), `None` for the back office.
    oauth_flow: Option<&'a str>,
}

impl Ending<'_> {
    /// The ending of a round trip that did not sign anyone in, for one of the fixed words.
    fn fail(&self, word: &str, cookies: &[String]) -> Response {
        match self.oauth_flow {
            None => back(self.s, word, cookies),
            Some(flow) => {
                if let Some(o) = self.s.oauth.as_ref() {
                    // A flow that is gone or busy is found out at the return address.
                    let _ = o.microsoft_done(flow, MicrosoftOutcome::Failed(word.to_string()));
                }
                with_cookies(see_other(&return_address(flow)), cookies)
            }
        }
    }
}

/// The address an app's round trip returns to. The flow id is the 32 hex digits the OAuth server
/// made (checked before it is stored in the row, and again by `microsoft_done`).
fn return_address(flow: &str) -> String {
    format!("/oauth/authorize/return?flow={flow}")
}

fn callback_blocking(s: &HttpState, parts: &Parts) -> Response {
    if !is_navigation(parts) {
        return not_a_navigation();
    }
    let ip = source_of(s, parts);
    s.shared.maybe_purge();
    let (Some(api), Some(rt)) = (s.auth.sso(), s.connectors.get()) else {
        return back(s, "unavailable", &[]);
    };
    let pairs = parts.uri.query().and_then(parse_pairs).unwrap_or_default();
    // A request with no valid state does nothing, costs nothing and is not written down: it is
    // not counted against the address either, so it cannot use up the allowance of a colleague.
    let Some(state) = pair(&pairs, "state").filter(|f| is_flow_id(f)) else {
        return back(s, "failed", &[]);
    };
    let clear_flow = set_cookie(&flow_cookie(state), "", 0, false);
    // One try per flow: spent before anything else is looked at.
    let Some(flow) = take_flow(s, state) else {
        // A well-formed state that is not a flow of ours costs a read and a delete: counted, with
        // a ceiling of its own (a real callback counts against the lower one below).
        if limited(s, "cbx", &ip, 120) {
            return back(s, "busy", &[clear_flow]);
        }
        api.audit_throttled(
            &ip,
            "oidc_refused",
            AnonLine::NoFlow,
            "the sign-in flow is unknown, already used or expired",
        );
        return back(s, "failed", &[clear_flow]);
    };
    // What the row says the round trip is for. The row is the server's own record, written by one
    // of the two start routes, and carries one of two values; anything else is the back office's.
    let oauth_flow = (flow["purpose"].as_str() == Some("oauth"))
        .then(|| flow["oauth_flow"].as_str())
        .flatten()
        .filter(|f| f.len() == 32 && f.bytes().all(|c| c.is_ascii_hexdigit()));
    if flow["purpose"].as_str() == Some("oauth") && oauth_flow.is_none() {
        // A row for an app's round trip that names no flow cannot be finished anywhere.
        return back(s, "failed", &[clear_flow]);
    }
    let end = Ending { s, oauth_flow };
    // Only a callback that spent a flow of this server counts against the address: it is the one
    // that goes on to call Microsoft. An app's round trips have a window of their own.
    if limited(
        s,
        if oauth_flow.is_some() {
            "oauth_cb"
        } else {
            "cb"
        },
        &ip,
        60,
    ) {
        return end.fail("busy", &[clear_flow]);
    }
    let redirect_uri = flow["redirect_uri"].as_str().unwrap_or("").to_string();
    let clear_flow = set_cookie(&flow_cookie(state), "", 0, secure_of(&redirect_uri));
    // The browser that comes back must be the one that left.
    let verifier = cookie_value(&parts.headers, &flow_cookie(state)).unwrap_or_default();
    if verifier.is_empty()
        || !ct_eq(
            &hash_of(&verifier),
            flow["verifier_hash"].as_str().unwrap_or(""),
        )
    {
        api.audit_throttled(
            &ip,
            "oidc_refused",
            AnonLine::Browser,
            "the browser that came back is not the one that began the sign-in",
        );
        return end.fail("failed", &[clear_flow]);
    }
    if let Some(err) = pair(&pairs, "error") {
        return end.fail(provider_answer(err).log(api, &ip), &[clear_flow]);
    }
    let Some(code) = pair(&pairs, "code").filter(|c| !c.is_empty() && c.len() <= 4096) else {
        api.audit_throttled(
            &ip,
            "oidc_refused",
            AnonLine::NoCode,
            "the provider's redirect carried no code",
        );
        return end.fail("failed", &[clear_flow]);
    };
    let identity = callback_leg(api.leg()).and_then(|leg| {
        let ep = discover(s, api, rt, &leg, "callback")?;
        identity_of(
            s,
            api,
            rt,
            &leg,
            &ep,
            code,
            &redirect_uri,
            &verifier,
            flow["nonce"].as_str().unwrap_or(""),
        )
    });
    let identity = match identity {
        Ok(i) => i,
        Err(r) => return end.fail(r.log(api, &ip), &[clear_flow]),
    };
    if let Some(oauth_flow) = oauth_flow {
        return app_ending(s, api, &ip, oauth_flow, &identity, clear_flow);
    }
    // From here only this browser can spend the one-time code.
    let done = hex(&random::<32>());
    match api.begin(&identity, &done) {
        Ok(one_time) => with_cookies(
            see_other(&format!("{}#oidc={one_time}", s.sso_return())),
            &[
                clear_flow,
                set_cookie(DONE_COOKIE, &done, DONE_SECS, secure_of(&redirect_uri)),
            ],
        ),
        // The reason is in the log already (`begin` wrote it); the person is told nothing more.
        Err(SsoError::Unavailable(m)) => {
            api.audit_throttled(
                &ip,
                "oidc_error",
                AnonLine::Begin,
                &format!("callback: {m}"),
            );
            back(s, "unavailable", &[clear_flow])
        }
        Err(_) => back(s, "failed", &[clear_flow]),
    }
}

/// The end of an app's round trip: the verified identity is signed in for the OAuth flow (no code
/// is made, nothing of the identity is kept) and the browser goes to the return address. Anything
/// that is not a person ends as one of the fixed words; the reason is in the security log already.
fn app_ending(
    s: &HttpState,
    api: &dyn SsoApi,
    ip: &str,
    oauth_flow: &str,
    identity: &SsoIdentity,
    clear_flow: String,
) -> Response {
    let end = Ending {
        s,
        oauth_flow: Some(oauth_flow),
    };
    let Some(o) = s.oauth.as_ref() else {
        return back(s, "failed", &[clear_flow]);
    };
    match api.sign_in_for_flow(identity) {
        Ok(signed) => {
            // A flow that is gone, or that someone else changed three times in a row, is found out
            // at the return address.
            let _ = o.microsoft_done(oauth_flow, MicrosoftOutcome::SignedIn(signed));
            with_cookies(see_other(&return_address(oauth_flow)), &[clear_flow])
        }
        Err(MfaError::Unavailable(m)) => {
            api.audit_throttled(ip, "oidc_error", AnonLine::Begin, &format!("callback: {m}"));
            end.fail("unavailable", &[clear_flow])
        }
        Err(_) => end.fail("failed", &[clear_flow]),
    }
}

// ---- POST /api/auth/oidc/exchange -----------------------------------------------------------

pub(crate) async fn exchange(State(s): Shared, parts: Parts, body: Bytes) -> Response {
    blocking(move || exchange_blocking(&s, &parts, &body)).await
}

fn exchange_blocking(s: &HttpState, parts: &Parts, body: &[u8]) -> Response {
    let ip = source_of(s, parts);
    let clear = set_cookie(DONE_COOKIE, "", 0, false);
    let r = if limited(s, "ex", &ip, 30) {
        api_error(
            StatusCode::TOO_MANY_REQUESTS,
            "too many attempts, try again in a minute",
            "rate_limited",
            "Wait a minute and sign in with Microsoft again.",
        )
    } else {
        exchange_inner(s, parts, body)
    };
    let mut r = with_cookies(r, &[clear]);
    r.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

fn exchange_inner(s: &HttpState, parts: &Parts, body: &[u8]) -> Response {
    let failed = || {
        api_error(
            StatusCode::UNAUTHORIZED,
            "the sign-in with Microsoft could not be completed",
            "sso_failed",
            "Start the sign-in again.",
        )
    };
    let Some(api) = s.auth.sso() else {
        return failed();
    };
    let code = serde_json::from_slice::<J>(body)
        .ok()
        .and_then(|v| v["code"].as_str().map(String::from))
        .unwrap_or_default();
    if code.is_empty() || code.len() > 256 {
        return failed();
    }
    let binder = cookie_value(&parts.headers, DONE_COOKIE).unwrap_or_default();
    match api.exchange(&code, &binder) {
        Ok((user, out)) => {
            let mut body = mfa::outcome_json(Some(&*s.auth), out);
            body.0["user"] = J::from(user);
            let mut r = body.into_response();
            harden(&mut r);
            r
        }
        Err(MfaError::Unavailable(_)) => api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "try again later",
            "unavailable",
            "The sign-in could not be completed right now.",
        ),
        Err(_) => failed(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_waits_for_another_requests_fetch_a_little_and_only_so_many_wait() {
        use std::sync::atomic::Ordering;
        let c = Cache::default();
        assert_eq!(MAX_WAITING, 8, "the number the documents give");
        let lock = Mutex::new(());
        // Nobody has the fetch in the air: the place is given at once.
        drop(c.one_at_a_time(&lock, Duration::from_millis(50)).unwrap());
        let held = lock.lock().unwrap();
        // A request waits for the fetch that is in the air, at most the limit it was given.
        let t = Instant::now();
        assert!(c.one_at_a_time(&lock, Duration::from_millis(150)).is_err());
        let waited = t.elapsed();
        assert!(
            waited >= Duration::from_millis(120) && waited < Duration::from_secs(3),
            "{waited:?}"
        );
        assert_eq!(
            c.waiting.load(Ordering::SeqCst),
            0,
            "the place was given back"
        );
        // MAX_WAITING requests wait; the next one is refused at once and holds no thread.
        std::thread::scope(|s| {
            let waiters: Vec<_> = (0..MAX_WAITING)
                .map(|_| s.spawn(|| c.one_at_a_time(&lock, Duration::from_millis(1500)).is_err()))
                .collect();
            let t = Instant::now();
            while c.waiting.load(Ordering::SeqCst) < MAX_WAITING {
                assert!(
                    t.elapsed() < Duration::from_secs(5),
                    "the waiters did not arrive"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            let t = Instant::now();
            let refused = c.one_at_a_time(&lock, Duration::from_millis(1500));
            assert!(refused.is_err());
            assert!(
                t.elapsed() < Duration::from_millis(300),
                "{:?}",
                t.elapsed()
            );
            assert_eq!(c.waiting.load(Ordering::SeqCst), MAX_WAITING);
            for w in waiters {
                assert!(w.join().unwrap(), "each waiter gave up at its limit");
            }
        });
        assert_eq!(c.waiting.load(Ordering::SeqCst), 0);
        // The fetch ends: the next request gets in.
        drop(held);
        assert!(c.one_at_a_time(&lock, Duration::from_millis(50)).is_ok());
    }

    #[test]
    fn a_flow_is_spent_by_exactly_one_of_several_racing_callbacks() {
        // Many readers of one row: only the one whose delete removed it may go on (D59: several
        // server processes on one database). Many rounds, many threads released at once.
        let (_flaky, db) = obop_storage::testing::Flaky::memory();
        for round in 0..300 {
            let flow = hex(&random::<32>());
            let rec = json!({
                "nonce": "n",
                "redirect_uri": "http://127.0.0.1/cb",
                "verifier_hash": "h",
                "expires_at_ms": now_ms() + 60_000,
            });
            db.kv_put(&flow_key(&flow), &rec.to_string(), Some(60_000))
                .unwrap();
            let gate = std::sync::Barrier::new(8);
            let winners = std::thread::scope(|s| {
                let racers: Vec<_> = (0..8)
                    .map(|_| {
                        s.spawn(|| {
                            gate.wait();
                            take_flow_from(&*db, &flow).is_some()
                        })
                    })
                    .collect();
                racers
                    .into_iter()
                    .map(|h| h.join().unwrap())
                    .filter(|won| *won)
                    .count()
            });
            assert_eq!(winners, 1, "round {round}: one winner");
        }
    }

    #[test]
    fn the_host_of_an_authority_is_what_the_egress_allow_list_wants() {
        assert_eq!(
            host_of("https://login.microsoftonline.com"),
            "login.microsoftonline.com"
        );
        assert_eq!(host_of("http://127.0.0.1:4455"), "127.0.0.1");
        assert_eq!(host_of("http://[::1]:4455"), "::1");
        assert_eq!(host_of("nonsense"), "");
    }

    #[test]
    fn what_the_provider_says_about_an_error_is_one_word_of_a_fixed_set_before_it_is_logged() {
        for known in KNOWN_ERRORS {
            assert_eq!(error_class(known), known);
        }
        assert_eq!(KNOWN_ERRORS.len(), 12);
        // Anything else, whatever it looks like, is "other": no text of the caller's survives.
        for other in [
            "",
            "a b\nc<script>",
            "Access_Denied",
            "access_denied ",
            "access_denied2",
            &"x".repeat(500),
        ] {
            assert_eq!(error_class(other), "other", "{other:?}");
        }
    }

    #[test]
    fn an_ipv6_address_is_counted_by_its_64_and_an_ipv4_mapped_one_as_ipv4() {
        assert_eq!(source_bucket("203.0.113.9"), "203.0.113.9");
        assert_eq!(source_bucket("::ffff:203.0.113.9"), "203.0.113.9");
        assert_eq!(source_bucket(""), "");
        assert_eq!(source_bucket("not an address"), "not an address");
        let a = source_bucket("2001:db8:1:2:aaaa:bbbb:cccc:dddd");
        assert_eq!(a, "2001:db8:1:2::/64");
        assert_eq!(a, source_bucket("2001:db8:1:2::1"));
        assert_ne!(a, source_bucket("2001:db8:1:3::1"));
        assert_eq!(source_bucket("::1"), "0:0:0:0::/64");
    }

    #[test]
    fn a_flow_id_is_64_hex_digits_and_nothing_else() {
        assert!(is_flow_id(&"ab".repeat(32)));
        for bad in ["", "ab", &"zz".repeat(32), &"ab".repeat(33)] {
            assert!(!is_flow_id(bad));
        }
    }

    #[test]
    fn at_most_eight_calls_to_the_provider_are_in_the_air_and_a_place_is_given_back() {
        let c = Cache::default();
        let held: Vec<Permit<'_>> = (0..MAX_IN_FLIGHT)
            .map(|_| c.permit().expect("a place"))
            .collect();
        assert_eq!(MAX_IN_FLIGHT, 8);
        assert!(c.permit().is_err(), "the ninth is answered at once");
        assert!(c.permit().is_err(), "and does not take a place by trying");
        drop(held);
        let again: Vec<_> = (0..MAX_IN_FLIGHT).map(|_| c.permit()).collect();
        assert!(again.iter().all(Result::is_ok), "all places came back");
    }

    #[test]
    fn a_failed_fetch_is_remembered_for_its_key_and_for_a_short_while_only() {
        let slot = Mutex::new(None);
        assert!(failed_recently(&slot, "k").is_none());
        remember_failure(&slot, "k", "the key set answered 503");
        assert_eq!(
            failed_recently(&slot, "k").as_deref(),
            Some("the key set answered 503")
        );
        assert!(
            failed_recently(&slot, "another tenant").is_none(),
            "a failure of one provider is not another's"
        );
        assert!(NEGATIVE_TTL >= Duration::from_secs(30) && NEGATIVE_TTL <= Duration::from_secs(60));
        assert!(SSO_TIMEOUT >= Duration::from_secs(5) && SSO_TIMEOUT <= Duration::from_secs(8));
        // Old enough, it is asked again.
        if let Some(old) = Instant::now().checked_sub(NEGATIVE_TTL + Duration::from_secs(1)) {
            *slot.lock().unwrap() = Some(("k".into(), old, "old".into()));
            assert!(failed_recently(&slot, "k").is_none());
        }
    }

    #[test]
    fn only_a_navigation_of_the_browser_passes() {
        let parts = |h: &[(&str, &str)]| {
            let mut r = axum::http::Request::builder().uri("/x");
            for (k, v) in h {
                r = r.header(*k, *v);
            }
            r.body(()).unwrap().into_parts().0
        };
        assert!(
            is_navigation(&parts(&[])),
            "no header: a script or an old browser"
        );
        assert!(is_navigation(&parts(&[
            ("sec-fetch-mode", "navigate"),
            ("sec-fetch-dest", "document"),
            ("sec-fetch-site", "cross-site"),
        ])));
        for bad in [
            [("sec-fetch-mode", "cors"), ("sec-fetch-dest", "empty")],
            [("sec-fetch-mode", "no-cors"), ("sec-fetch-dest", "image")],
            [("sec-fetch-mode", "navigate"), ("sec-fetch-dest", "iframe")],
            [("sec-fetch-mode", "navigate"), ("sec-fetch-dest", "script")],
        ] {
            assert!(!is_navigation(&parts(&bad)), "{bad:?}");
        }
    }

    // ---- the pieces of the round trip (WP1): each has its word and its sort ----------------------

    /// A `SsoApi` that records the lines written to the anonymous log and nothing else.
    #[derive(Default)]
    struct Recorder(Mutex<Vec<(String, String, AnonLine, String)>>);

    impl SsoApi for Recorder {
        fn offered(&self) -> bool {
            unimplemented!()
        }
        fn provider(&self) -> Result<Option<obop_commands::SsoProvider>, SsoError> {
            unimplemented!()
        }
        fn leg(&self) -> Result<Option<SsoLeg>, SsoError> {
            unimplemented!()
        }
        fn endpoints(&self, _: &[u8]) -> Result<SsoEndpoints, SsoError> {
            unimplemented!()
        }
        fn verify(&self, _: &str, _: &[u8], _: &str) -> Result<SsoIdentity, SsoError> {
            unimplemented!()
        }
        fn begin(&self, _: &SsoIdentity, _: &str) -> Result<String, SsoError> {
            unimplemented!()
        }
        fn exchange(
            &self,
            _: &str,
            _: &str,
        ) -> Result<(String, obop_commands::LoginOutcome), MfaError> {
            unimplemented!()
        }
        fn sign_in_for_flow(
            &self,
            _: &SsoIdentity,
        ) -> Result<obop_commands::SsoFlowSignIn, MfaError> {
            unimplemented!()
        }
        fn second_step_for_flow(&self, _: &str, _: Option<&str>) -> Result<String, MfaError> {
            unimplemented!()
        }
        fn audit(&self, _: &str, _: &str, _: &str) {
            unimplemented!()
        }
        fn audit_throttled(&self, source: &str, kind: &str, line: AnonLine, detail: &str) {
            self.0
                .lock()
                .unwrap()
                .push((source.into(), kind.into(), line, detail.into()));
        }
        fn configure(
            &self,
            _: &obop_commands::Principal,
            _: &str,
            _: &str,
            _: Option<&str>,
            _: Option<bool>,
        ) -> Result<obop_commands::SsoProvider, SsoError> {
            unimplemented!()
        }
        fn clear(&self, _: &obop_commands::Principal) -> Result<(), SsoError> {
            unimplemented!()
        }
        fn bind(&self, _: &obop_commands::Principal, _: &str, _: &str) -> Result<J, SsoError> {
            unimplemented!()
        }
        fn unbind(&self, _: &obop_commands::Principal, _: &str) -> Result<J, SsoError> {
            unimplemented!()
        }
        fn set_sso_only(
            &self,
            _: &obop_commands::Principal,
            _: &str,
            _: bool,
        ) -> Result<J, SsoError> {
            unimplemented!()
        }
        fn bindings(&self, _: &obop_commands::Principal) -> Result<J, SsoError> {
            unimplemented!()
        }
    }

    fn shape(r: &Refusal) -> (&'static str, &'static str, AnonLine) {
        (r.word, r.kind, r.line)
    }

    #[test]
    fn a_refusal_writes_its_one_line_under_its_kind_and_sort_and_hands_back_its_word() {
        let api = Recorder::default();
        let r = Refusal::new(
            "unavailable",
            "oidc_error",
            AnonLine::TokenUnwell,
            "the token endpoint answered 503".into(),
        );
        assert_eq!(r.log(&api, "203.0.113.9"), "unavailable");
        assert_eq!(
            api.0.lock().unwrap().as_slice(),
            [(
                "203.0.113.9".to_string(),
                "oidc_error".to_string(),
                AnonLine::TokenUnwell,
                "the token endpoint answered 503".to_string()
            )]
        );
    }

    #[test]
    fn the_address_of_the_provider_is_fixed_and_asks_for_nothing_else() {
        let url = authorization_url(
            "https://login.example/tenant/oauth2/v2.0/authorize",
            "client id",
            "https://obop.test/api/auth/oidc/callback",
            "STATE",
            "NONCE",
            "CHALLENGE",
        );
        assert_eq!(
            url,
            "https://login.example/tenant/oauth2/v2.0/authorize?client_id=client%20id&response_type=code&redirect_uri=https%3A%2F%2Fobop.test%2Fapi%2Fauth%2Foidc%2Fcallback&response_mode=query&scope=openid%20profile&state=STATE&nonce=NONCE&code_challenge=CHALLENGE&code_challenge_method=S256"
        );
        for never in [
            "prompt",
            "max_age",
            "login_hint",
            "offline_access",
            "User.Read",
        ] {
            assert!(!url.contains(never), "{never}");
        }
    }

    #[test]
    fn the_providers_own_error_is_a_cancellation_only_when_the_person_said_no() {
        let no = provider_answer("access_denied");
        assert_eq!(
            shape(&no),
            ("cancelled", "oidc_cancelled", AnonLine::Provider)
        );
        assert!(no.text.contains("\"access_denied\""));
        for other in ["server_error", "interaction_required", "invalid_scope"] {
            let r = provider_answer(other);
            assert_eq!(
                shape(&r),
                ("failed", "oidc_error", AnonLine::Provider),
                "{other}"
            );
            assert!(r.text.contains(other), "{other}");
        }
        // Whatever else the caller wrote is the word "other": none of it reaches a line.
        let r = provider_answer("access_denied\nforged: line <b>");
        assert_eq!(shape(&r), ("failed", "oidc_error", AnonLine::Provider));
        assert!(r.text.contains("\"other\"") && !r.text.contains("forged"));
    }

    #[test]
    fn a_provider_switched_off_or_unreadable_during_the_callback_has_its_word_and_sort() {
        let off = callback_leg(Ok(None)).unwrap_err();
        assert_eq!(
            shape(&off),
            ("failed", "oidc_refused", AnonLine::SwitchedOff)
        );
        let bad = callback_leg(Err(SsoError::Unavailable("no key ring".into()))).unwrap_err();
        assert_eq!(
            shape(&bad),
            ("unavailable", "oidc_error", AnonLine::Settings)
        );
        assert_eq!(bad.text, "callback: no key ring");
        let leg = SsoLeg {
            authority: "http://127.0.0.1:1".into(),
            tenant: "t".into(),
            client_id: "c".into(),
            client_secret: "s".into(),
        };
        assert_eq!(callback_leg(Ok(Some(leg))).unwrap().client_id, "c");
    }

    #[test]
    fn the_token_endpoint_is_unwell_or_says_no_or_gives_the_id_token_and_nothing_else_is_read() {
        for status in [429, 500, 502, 503, 599] {
            let r = id_token_of(status, b"{}").unwrap_err();
            assert_eq!(
                shape(&r),
                ("unavailable", "oidc_error", AnonLine::TokenUnwell),
                "{status}"
            );
            assert_eq!(r.text, format!("the token endpoint answered {status}"));
        }
        // A 200 without an id token, a 4xx with or without an error: the provider said no.
        for (status, body) in [
            (200, r#"{"access_token":"a"}"#),
            (200, r#"{"id_token":""}"#),
            (200, "not json"),
            (400, r#"{"error":"invalid_grant"}"#),
            (401, ""),
        ] {
            let r = id_token_of(status, body.as_bytes()).unwrap_err();
            assert_eq!(
                shape(&r),
                ("failed", "oidc_refused", AnonLine::TokenRefused),
                "{status} {body}"
            );
        }
        let r = id_token_of(
            400,
            br#"{"error":"invalid_grant","error_description":"AADSTS70000 secret text"}"#,
        )
        .unwrap_err();
        assert_eq!(r.text, "the token endpoint answered 400 \"invalid_grant\"");
        let r = id_token_of(400, br#"{"error":"made up by <someone>"}"#).unwrap_err();
        assert_eq!(r.text, "the token endpoint answered 400 \"other\"");
        // An id token on a 4xx is not taken; on a 200 it is the only thing that is read.
        assert!(id_token_of(400, br#"{"id_token":"x.y.z"}"#).is_err());
        assert_eq!(
            id_token_of(200, br#"{"id_token":"x.y.z","access_token":"never read"}"#).unwrap(),
            "x.y.z"
        );
    }

    fn who() -> SsoIdentity {
        SsoIdentity {
            issuer: "i".into(),
            tenant: "t".into(),
            oid: "o".into(),
            amr: vec![],
            auth_time: None,
        }
    }

    #[test]
    fn an_unknown_key_fetches_the_key_set_again_once_and_not_twice() {
        use std::cell::{Cell, RefCell};
        // A rotated key: the first look fails on the key name, the second succeeds.
        let fetched = RefCell::new(Vec::<bool>::new());
        let looks = Cell::new(0);
        let got = verify_with_refetch(
            |force| {
                fetched.borrow_mut().push(force);
                Ok(b"{}".to_vec())
            },
            |_| {
                looks.set(looks.get() + 1);
                if looks.get() == 1 {
                    Err(SsoError::UnknownKey)
                } else {
                    Ok(who())
                }
            },
            || panic!("a usable key set is not forgotten"),
        );
        assert_eq!(got.unwrap(), who());
        assert_eq!(
            *fetched.borrow(),
            [false, true],
            "once from the cache, once forced"
        );
        // A key that stays unknown is refused as an id token, after one forced fetch and no more.
        let fetched = RefCell::new(Vec::<bool>::new());
        let r = verify_with_refetch(
            |force| {
                fetched.borrow_mut().push(force);
                Ok(vec![])
            },
            |_| Err(SsoError::UnknownKey),
            || {},
        )
        .unwrap_err();
        assert_eq!(shape(&r), ("failed", "oidc_refused", AnonLine::IdToken));
        assert_eq!(
            r.text,
            "the id token was refused: the signing key is not known"
        );
        assert_eq!(*fetched.borrow(), [false, true]);
    }

    #[test]
    fn a_key_set_that_cannot_be_fetched_or_used_is_unavailable_and_a_bad_token_is_failed() {
        use std::cell::Cell;
        // The first fetch fails.
        let r = verify_with_refetch(|_| Err("503".into()), |_| unreachable!(), || {}).unwrap_err();
        assert_eq!(shape(&r), ("unavailable", "oidc_error", AnonLine::KeySet));
        assert_eq!(r.text, "the key set could not be fetched: 503");
        // The fetch again (after an unknown key) fails.
        let n = Cell::new(0);
        let r = verify_with_refetch(
            |_| {
                n.set(n.get() + 1);
                if n.get() == 1 {
                    Ok(vec![])
                } else {
                    Err("429".into())
                }
            },
            |_| Err(SsoError::UnknownKey),
            || {},
        )
        .unwrap_err();
        assert_eq!(shape(&r), ("unavailable", "oidc_error", AnonLine::KeySet));
        assert_eq!(r.text, "the key set could not be fetched again: 429");
        // A key set that is not one: dropped from the cache, and unavailable.
        let forgot = Cell::new(0);
        let r = verify_with_refetch(
            |_| Ok(vec![]),
            |_| Err(SsoError::Unavailable("no keys".into())),
            || forgot.set(forgot.get() + 1),
        )
        .unwrap_err();
        assert_eq!(
            shape(&r),
            ("unavailable", "oidc_error", AnonLine::KeySetUnusable)
        );
        assert_eq!(r.text, "the key set could not be used: no keys");
        assert_eq!(forgot.get(), 1);
        // A token that fails a check: failed, and the key set stays.
        let r = verify_with_refetch(
            |_| Ok(vec![]),
            |_| {
                Err(SsoError::Refused(
                    "the nonce is not the one of this sign-in".into(),
                ))
            },
            || panic!("a good key set is not forgotten"),
        )
        .unwrap_err();
        assert_eq!(shape(&r), ("failed", "oidc_refused", AnonLine::IdToken));
        assert_eq!(
            r.text,
            "the id token was refused: the nonce is not the one of this sign-in"
        );
    }

    #[test]
    fn cookies_are_http_only_lax_and_scoped_to_the_flow_routes() {
        let c = set_cookie("obop_sso_abc", "v", 600, true);
        assert!(c.contains("HttpOnly") && c.contains("SameSite=Lax") && c.contains("Secure"));
        assert!(c.contains("Path=/api/auth/oidc;"));
        assert!(!set_cookie("n", "v", 1, false).contains("Secure"));
        assert!(secure_of("https://x.example/cb") && !secure_of("http://127.0.0.1:1/cb"));
    }
}
