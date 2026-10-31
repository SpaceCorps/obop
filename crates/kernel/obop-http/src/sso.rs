//! Sign-in with Microsoft Entra over HTTP (D91, Proposed): the browser round trip of the
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
//! There is no cookie session: the bearer token goes to the page and stays in `sessionStorage`
//! as after a password. There is no `next` or return address; the target of the redirect is the
//! root of this origin and cannot be chosen by a request.

use super::oauth::{client_ip, cookie_value, harden, pair, parse_pairs, see_other};
use super::{HttpState, api_error, blocking, mfa};
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use obop_commands::{MfaError, SsoApi, SsoEndpoints, SsoError, SsoLeg};
use obop_connectors::net::Request;
use obop_connectors::oauth::pkce_pair;
use obop_keys::{hex, random};
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
/// The most calls to Microsoft that one server process has in the air at once (the fetches and
/// the exchange of a code). More are answered "busy" at once and hold no thread.
const MAX_IN_FLIGHT: usize = 8;
/// How long a call to Microsoft may take (the egress client's own limit is 20 seconds, which is
/// for connectors; a person is not kept waiting that long, and the blocking threads that serve
/// the break-glass sign-in are not either).
const SSO_TIMEOUT: Duration = Duration::from_secs(6);
/// How often the key set may be fetched again because a token named a key the cache lacks
/// (all server processes together): a stream of tokens with made-up key names cannot make this
/// server hammer the provider.
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
}

/// One of the calls to Microsoft this process may have in the air; given back when dropped.
pub(crate) struct Permit<'a>(&'a std::sync::atomic::AtomicUsize);

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
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
fn host_of(authority: &str) -> String {
    let rest = authority.split("://").nth(1).unwrap_or("");
    let hostport = rest.split('/').next().unwrap_or("");
    let host = match hostport.strip_prefix('[') {
        Some(h) => h.split(']').next().unwrap_or(""),
        None => hostport.rsplit_once(':').map_or(hostport, |(h, _)| h),
    };
    host.to_ascii_lowercase()
}

/// A short, log-safe piece of what the provider said about an error (its `error` code; never the
/// description, which the provider may fill with anything).
fn clean(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        .take(48)
        .collect()
}

fn describe(e: &SsoError) -> String {
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

    fn discovery(
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
        let _one = self.discovery_flight.lock();
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
        let got = fetch(rt, &url, &host_of(&leg.authority))
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

    fn jwks(
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
        let _one = self.jwks_flight.lock();
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
        match fetch(rt, &ep.jwks, &host_of(&leg.authority)) {
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

    fn forget_jwks(&self) {
        if let Ok(mut g) = self.jwks.lock() {
            *g = None;
        }
    }
}

/// A GET through the hardened egress client. The text of an error has no URL, header or body in it.
fn fetch(rt: &obop_connectors::Runtime, url: &str, host: &str) -> Result<Vec<u8>, String> {
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
            SSO_TIMEOUT,
        )
        .map_err(|e| e.to_string())?;
    if r.status != 200 {
        return Err(format!("the provider answered {}", r.status));
    }
    Ok(r.body)
}

fn limited(s: &HttpState, what: &str, ip: &str, max: u64) -> bool {
    obop_storage::limit::hit(
        &*s.shared.backend,
        &format!("rl:sso:{what}:{ip}"),
        max,
        60_000,
    )
    .is_err()
}

fn flow_key(flow: &str) -> String {
    format!("{FLOW_PREFIX}{}", hex(&Sha256::digest(flow.as_bytes())))
}

fn hash_of(secret: &str) -> String {
    hex(&Sha256::digest(
        format!("obop-sso-verifier\n{secret}").as_bytes(),
    ))
}

fn flow_cookie(flow: &str) -> String {
    format!("{FLOW_COOKIE}{}", flow.get(..12).unwrap_or("x"))
}

fn set_cookie(name: &str, value: &str, max_age: u64, secure: bool) -> String {
    format!(
        "{name}={value}; Max-Age={max_age}; Path={COOKIE_PATH}; HttpOnly; SameSite=Lax{}",
        if secure { "; Secure" } else { "" }
    )
}

fn with_cookies(mut r: Response, cookies: &[String]) -> Response {
    for c in cookies {
        if let Ok(v) = HeaderValue::from_str(c) {
            r.headers_mut().append(header::SET_COOKIE, v);
        }
    }
    r
}

/// The redirect of every failure: the root of this origin and a fixed word.
fn back(s: &HttpState, error: &str, cookies: &[String]) -> Response {
    with_cookies(
        see_other(&format!("{}#oidc_error={error}", s.sso_return())),
        cookies,
    )
}

fn secure_of(redirect_uri: &str) -> bool {
    redirect_uri.starts_with("https://")
}

/// Whether the request is a navigation of the browser (a click, a typed address, a redirect), and
/// not an image, a script or a `fetch` that some page made. A browser says so in `Sec-Fetch-Mode`
/// and `Sec-Fetch-Dest`; a client that sends neither (a script, an older browser) passes, as it
/// could not be told from the person's own click.
fn is_navigation(parts: &Parts) -> bool {
    let get = |n: &str| parts.headers.get(n).and_then(|v| v.to_str().ok());
    get("sec-fetch-mode").is_none_or(|m| m == "navigate")
        && get("sec-fetch-dest").is_none_or(|d| d == "document")
}

fn not_a_navigation() -> Response {
    let mut r = (
        StatusCode::BAD_REQUEST,
        "this address is for the sign-in page only\n",
    )
        .into_response();
    r.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

fn is_flow_id(s: &str) -> bool {
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

// ---- GET /api/auth/oidc/start ---------------------------------------------------------------

pub(crate) async fn start(State(s): Shared, parts: Parts) -> Response {
    blocking(move || start_blocking(&s, &parts)).await
}

fn start_blocking(s: &HttpState, parts: &Parts) -> Response {
    if !is_navigation(parts) {
        return not_a_navigation();
    }
    let ip = client_ip(s, parts);
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
            api.audit_throttled(&ip, "oidc_error", &format!("start: {}", describe(&e)));
            return back(s, "unavailable", &[]);
        }
    };
    let ep = match s.sso.discovery(api, rt, &leg) {
        Ok(ep) => ep,
        Err(e) => {
            api.audit_throttled(
                &ip,
                "oidc_error",
                &format!("start: the discovery document could not be used: {e}"),
            );
            return back(s, "unavailable", &[]);
        }
    };
    let (verifier, challenge) = pkce_pair();
    let flow = hex(&random::<32>());
    let nonce = hex(&random::<24>());
    let redirect_uri = format!("{}/api/auth/oidc/callback", s.mcp.base_url(&parts.headers));
    // The row expires in the store after ten minutes and says so itself: a store that keeps an
    // expired row (a lazy clean-up) does not make an old flow good again.
    let rec = json!({
        "nonce": nonce,
        "redirect_uri": redirect_uri,
        "verifier_hash": hash_of(&verifier),
        "expires_at_ms": now_ms() + u128::from(FLOW_TTL_MS),
    });
    if s.shared
        .backend
        .kv_put(&flow_key(&flow), &rec.to_string(), Some(FLOW_TTL_MS))
        .is_err()
    {
        return back(s, "unavailable", &[]);
    }
    let url = format!(
        "{}?client_id={}&response_type=code&redirect_uri={}&response_mode=query&scope=openid%20profile&state={}&nonce={}&code_challenge={}&code_challenge_method=S256",
        ep.authorization,
        pct(&leg.client_id),
        pct(&redirect_uri),
        flow,
        nonce,
        challenge,
    );
    // One line per address and ten minutes, not one per request: a start is a free call.
    api.audit_throttled(&ip, "oidc_start", "a sign-in with Microsoft was begun");
    with_cookies(
        see_other(&url),
        &[set_cookie(
            &flow_cookie(&flow),
            &verifier,
            FLOW_TTL_MS / 1000,
            secure_of(&redirect_uri),
        )],
    )
}

// ---- GET /api/auth/oidc/callback ------------------------------------------------------------

pub(crate) async fn callback(State(s): Shared, parts: Parts) -> Response {
    blocking(move || callback_blocking(&s, &parts)).await
}

/// Spend the flow: read it and delete it, and trust it only if the delete was ours.
fn take_flow(s: &HttpState, flow: &str) -> Option<J> {
    take_flow_from(&*s.shared.backend, flow)
}

/// [`take_flow`] on a store: the delete is the claim, so of several processes that read the same
/// row (D59) exactly one is told it spent the flow.
fn take_flow_from(backend: &dyn obop_storage::Backend, flow: &str) -> Option<J> {
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

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

fn callback_blocking(s: &HttpState, parts: &Parts) -> Response {
    if !is_navigation(parts) {
        return not_a_navigation();
    }
    let ip = client_ip(s, parts);
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
            "the sign-in flow is unknown, already used or expired",
        );
        return back(s, "failed", &[clear_flow]);
    };
    // Only a callback that spent a flow of this server counts against the address: it is the one
    // that goes on to call Microsoft.
    if limited(s, "cb", &ip, 60) {
        return back(s, "busy", &[clear_flow]);
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
            "the browser that came back is not the one that began the sign-in",
        );
        return back(s, "failed", &[clear_flow]);
    }
    if let Some(err) = pair(&pairs, "error") {
        let err = clean(err);
        // Only the person's own refusal is a cancellation; any other answer is the provider's
        // error, and is logged and worded as one.
        let cancelled = err == "access_denied";
        api.audit_throttled(
            &ip,
            if cancelled {
                "oidc_cancelled"
            } else {
                "oidc_error"
            },
            &format!("the provider ended the sign-in with {err:?}"),
        );
        let word = if cancelled { "cancelled" } else { "failed" };
        return back(s, word, &[clear_flow]);
    }
    let Some(code) = pair(&pairs, "code").filter(|c| !c.is_empty() && c.len() <= 4096) else {
        api.audit_throttled(
            &ip,
            "oidc_refused",
            "the provider's redirect carried no code",
        );
        return back(s, "failed", &[clear_flow]);
    };
    let leg = match api.leg() {
        Ok(Some(l)) => l,
        Ok(None) => {
            api.audit_throttled(
                &ip,
                "oidc_refused",
                "the provider was switched off during the sign-in",
            );
            return back(s, "failed", &[clear_flow]);
        }
        Err(e) => {
            api.audit_throttled(&ip, "oidc_error", &format!("callback: {}", describe(&e)));
            return back(s, "unavailable", &[clear_flow]);
        }
    };
    let ep = match s.sso.discovery(api, rt, &leg) {
        Ok(ep) => ep,
        Err(e) => {
            api.audit_throttled(
                &ip,
                "oidc_error",
                &format!("callback: the discovery document could not be used: {e}"),
            );
            return back(s, "unavailable", &[clear_flow]);
        }
    };
    // The code for the id token: from this server, with the client secret and the verifier.
    let form = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&client_secret={}&code_verifier={}",
        pct(code),
        pct(&redirect_uri),
        pct(&leg.client_id),
        pct(&leg.client_secret),
        pct(&verifier),
    );
    let Ok(_permit) = s.sso.permit() else {
        api.audit_throttled(
            &ip,
            "oidc_error",
            "too many calls to the provider are in the air; this sign-in was not tried",
        );
        return back(s, "busy", &[clear_flow]);
    };
    let resp = rt.client.send_within(
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
        SSO_TIMEOUT,
    );
    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            api.audit_throttled(
                &ip,
                "oidc_error",
                &format!("the token endpoint could not be reached: {e}"),
            );
            return back(s, "unavailable", &[clear_flow]);
        }
    };
    let answer: J = serde_json::from_slice(&resp.body).unwrap_or(J::Null);
    let id_token = answer["id_token"].as_str().unwrap_or("");
    if resp.status != 200 || id_token.is_empty() {
        api.audit_throttled(
            &ip,
            "oidc_refused",
            &format!(
                "the token endpoint answered {}{}",
                resp.status,
                answer["error"]
                    .as_str()
                    .map(|e| format!(" {:?}", clean(e)))
                    .unwrap_or_default()
            ),
        );
        return back(s, "failed", &[clear_flow]);
    }
    let nonce = flow["nonce"].as_str().unwrap_or("");
    let mut jwks = match s.sso.jwks(rt, &*s.shared.backend, &leg, &ep, false) {
        Ok(d) => d,
        Err(e) => {
            api.audit_throttled(
                &ip,
                "oidc_error",
                &format!("the key set could not be fetched: {e}"),
            );
            return back(s, "unavailable", &[clear_flow]);
        }
    };
    let mut verified = api.verify(id_token, &jwks, nonce);
    if verified == Err(SsoError::UnknownKey) {
        // The provider rotated its keys: fetch the set again, once, and look again.
        match s.sso.jwks(rt, &*s.shared.backend, &leg, &ep, true) {
            Ok(d) => {
                jwks = d;
                verified = api.verify(id_token, &jwks, nonce);
            }
            Err(e) => {
                api.audit_throttled(
                    &ip,
                    "oidc_error",
                    &format!("the key set could not be fetched again: {e}"),
                );
                return back(s, "unavailable", &[clear_flow]);
            }
        }
    }
    let identity = match verified {
        Ok(i) => i,
        Err(SsoError::Unavailable(m)) => {
            s.sso.forget_jwks();
            api.audit_throttled(
                &ip,
                "oidc_error",
                &format!("the key set could not be used: {m}"),
            );
            return back(s, "unavailable", &[clear_flow]);
        }
        Err(e) => {
            api.audit_throttled(
                &ip,
                "oidc_refused",
                &format!("the id token was refused: {}", describe(&e)),
            );
            return back(s, "failed", &[clear_flow]);
        }
    };
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
            api.audit_throttled(&ip, "oidc_error", &format!("callback: {m}"));
            back(s, "unavailable", &[clear_flow])
        }
        Err(_) => back(s, "failed", &[clear_flow]),
    }
}

// ---- POST /api/auth/oidc/exchange -----------------------------------------------------------

pub(crate) async fn exchange(State(s): Shared, parts: Parts, body: Bytes) -> Response {
    blocking(move || exchange_blocking(&s, &parts, &body)).await
}

fn exchange_blocking(s: &HttpState, parts: &Parts, body: &[u8]) -> Response {
    let ip = client_ip(s, parts);
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
    fn what_the_provider_says_about_an_error_is_cut_down_before_it_is_logged() {
        assert_eq!(clean("access_denied"), "access_denied");
        assert_eq!(clean("a b\nc<script>"), "abcscript");
        assert_eq!(clean(&"x".repeat(500)).len(), 48);
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

    #[test]
    fn cookies_are_http_only_lax_and_scoped_to_the_flow_routes() {
        let c = set_cookie("obop_sso_abc", "v", 600, true);
        assert!(c.contains("HttpOnly") && c.contains("SameSite=Lax") && c.contains("Secure"));
        assert!(c.contains("Path=/api/auth/oidc;"));
        assert!(!set_cookie("n", "v", 1, false).contains("Secure"));
        assert!(secure_of("https://x.example/cb") && !secure_of("http://127.0.0.1:1/cb"));
    }
}
