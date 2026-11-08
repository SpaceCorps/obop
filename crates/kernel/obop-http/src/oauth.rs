//! The HTTP side of the OAuth authorization server (D64): the metadata document, dynamic client
//! registration, the authorization endpoint with its sign-in and consent pages, the token and
//! revocation endpoints, and the owner's list of connected applications. The rules and the state
//! are in `obop-oauth`; this module only carries requests, sets the headers and checks where a
//! browser request came from.

use super::{HttpState, blocking};
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use obop_commands::brand::Shown;
use obop_oauth::flow::AuthorizeParams;
use obop_oauth::page::{self, Rendered};
use obop_oauth::{
    AuthzError, DecideOut, FlowCtx, FlowError, Form, LoginOut, OAuth, OAuthError, microsoft_key,
};
use serde_json::{Value as J, json};
use std::sync::Arc;

type Shared = State<Arc<HttpState>>;

/// Size of the bodies these endpoints read.
pub(crate) const MAX_BODY: usize = 16 * 1024;

// ---- plumbing ------------------------------------------------------------------------------

/// The address rate limits are keyed on: the peer, or behind a trusted proxy the nearest
/// `X-Forwarded-For` entry, which is the last element of the last header line (a proxy that adds
/// its own line, or extends the one it got, puts its entry at the end; the entries before it are
/// whatever the client sent). The same rule as the public site (`public.rs`) and the signer door.
pub(crate) fn client_ip(s: &HttpState, parts: &Parts) -> String {
    super::addr::client_address(
        s.trust_proxy.load(std::sync::atomic::Ordering::Relaxed),
        &parts.headers,
        &parts.extensions,
    )
}

fn pct_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' => {
                let h = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
                out.push(u8::from_str_radix(h, 16).ok()?);
                i += 2;
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8(out).ok()
}

/// `a=1&b=2` into pairs; `None` when it is not valid percent-encoded UTF-8.
pub(crate) fn parse_pairs(s: &str) -> Option<Vec<(String, String)>> {
    s.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            Some((pct_decode(k)?, pct_decode(v)?))
        })
        .collect()
}

pub(crate) fn pair<'a>(pairs: &'a [(String, String)], k: &str) -> Option<&'a str> {
    pairs.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str())
}

/// Headers every page and every token response carries.
pub(crate) fn harden(r: &mut Response) {
    let h = r.headers_mut();
    for (k, v) in [
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "same-origin"),
        ("cache-control", "no-store"),
        ("pragma", "no-cache"),
        ("x-frame-options", "DENY"),
        ("cross-origin-opener-policy", "same-origin"),
        (
            "permissions-policy",
            "camera=(), microphone=(), geolocation=()",
        ),
    ] {
        h.insert(
            axum::http::HeaderName::from_static(k),
            HeaderValue::from_static(v),
        );
    }
}

fn cors(r: &mut Response) {
    let h = r.headers_mut();
    h.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    h.insert(
        header::ACCESS_CONTROL_EXPOSE_HEADERS,
        HeaderValue::from_static("Retry-After, X-Obop-Applied"),
    );
}

fn html(status: StatusCode, page: Rendered, cookie: Option<String>) -> Response {
    let mut r = (status, page.html).into_response();
    r.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    if let Ok(v) = HeaderValue::from_str(&page.csp) {
        r.headers_mut().insert(header::CONTENT_SECURITY_POLICY, v);
    }
    if let Some(c) = cookie
        && let Ok(v) = HeaderValue::from_str(&c)
    {
        r.headers_mut().insert(header::SET_COOKIE, v);
    }
    harden(&mut r);
    r
}

pub(crate) fn see_other(location: &str) -> Response {
    let mut r = StatusCode::SEE_OTHER.into_response();
    if let Ok(v) = HeaderValue::from_str(location) {
        r.headers_mut().insert(header::LOCATION, v);
    }
    harden(&mut r);
    r
}

/// Add cookies to a response, one `Set-Cookie` header each (the redirects of the Microsoft round
/// trip set or clear more than one; `html` sets at most one by replacing).
pub(crate) fn with_cookies(mut r: Response, cookies: &[String]) -> Response {
    for c in cookies {
        if let Ok(v) = HeaderValue::from_str(c) {
            r.headers_mut().append(header::SET_COOKIE, v);
        }
    }
    r
}

fn oauth_json(status: StatusCode, body: &J) -> Response {
    let mut r = (status, body.to_string()).into_response();
    r.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    harden(&mut r);
    cors(&mut r);
    r
}

fn oauth_error(e: &OAuthError) -> Response {
    let mut r = oauth_json(
        StatusCode::from_u16(e.status).unwrap_or(StatusCode::BAD_REQUEST),
        &e.json(),
    );
    if let Some(w) = e.retry_after
        && let Ok(v) = HeaderValue::from_str(&w.to_string())
    {
        r.headers_mut().insert(header::RETRY_AFTER, v);
    }
    r
}

fn disabled() -> Response {
    oauth_json(
        StatusCode::NOT_FOUND,
        &json!({"error": "not_found", "error_description": "this instance runs without an OAuth authorization server"}),
    )
}

fn lang_of(h: &HeaderMap, param: Option<&str>) -> &'static str {
    page::pick_lang(
        param,
        h.get(header::ACCEPT_LANGUAGE).and_then(|v| v.to_str().ok()),
    )
}

fn cookie_name(flow: &str) -> String {
    format!("obop_oauth_{}", flow.get(..12).unwrap_or("x"))
}

pub(crate) fn cookie_value(h: &HeaderMap, name: &str) -> Option<String> {
    h.get(header::COOKIE)
        .and_then(|v| v.to_str().ok())?
        .split(';')
        .filter_map(|c| c.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_string())
}

/// A browser form post must come from this origin: `Origin` when it names one, else
/// `Sec-Fetch-Site`. (A browser sends `Origin: null` for sandboxed or opaque origins and when the
/// page's referrer policy is `no-referrer`; fetch metadata tells those cases apart.)
pub(crate) fn same_origin(h: &HeaderMap, base: &str) -> bool {
    match h.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        Some(o) if o != "null" => o.trim_end_matches('/').eq_ignore_ascii_case(base),
        _ => h.get("sec-fetch-site").and_then(|v| v.to_str().ok()) == Some("same-origin"),
    }
}

/// The brand every page of this request shows. A handler asks for it once, first, in the four places
/// where a request of the authorization flow begins (`authorize_blocking`, `step`,
/// `microsoft_blocking` and `microsoft_return`), and hands that one value to every page it
/// renders, so the name and the logo of a page come from one read (docs/spec/branding.md,
/// section 4). The handlers run on a blocking thread, where the reader may read the store.
///
/// When the refill meets a store fault, the pool has noted it on this request: `ops::storage_faults`
/// answers the request with the 503 of D74 whatever the handler sends, and `fault::before_write`
/// refuses the writes it would make. So a sign-in or a decision whose first brand read fails makes
/// no session and no token, issues no code, writes no entry of the security log and leaves the flow
/// and the short-lived values of the store as they were, and is answered 503; one whose read
/// succeeds writes as it always did.
///
/// The expiring counters are the exception, because `fault::before_write` does not guard
/// `Backend::counter_add` (`obop-storage/src/guard.rs`) and the handler goes on to the steps that
/// count: a request of this kind still spends one hit of the window of its address (`oauth:rl:authz`
/// on a start, 60 in ten minutes; `oauth:rl:login` on a password step, a second step and a passkey,
/// 30 in ten minutes; `rl:sso:oauth_start` on the Microsoft link, 30 in a minute; the housekeeping
/// lease of the OAuth server), and a wrong password counts toward the lockout of the account, as
/// every wrong guess does. The right password counts no failure. So while the store refuses, every
/// retry of a person, a right password included, uses a hit of the window of their address, and the
/// allowance is back when the window ends (the answer of a spent window is the 429 with
/// `Retry-After`). Two tests pin the numbers:
/// `a_sign_in_whose_first_brand_read_fails_leaves_the_store_as_it_was_and_is_a_503` and
/// `a_wrong_password_under_a_failing_first_read_still_counts_toward_the_lockout`. The read is not
/// run inside `fault::detached` to hide any of this: a request that cannot read the store cannot do
/// what it came for either. (Putting the counters under `before_write` would make "writes nothing"
/// literal; that is a change in `obop-storage` and outside this slice.)
fn read_brand(s: &HttpState) -> Arc<Shown> {
    s.dispatcher.brand().current()
}

fn flow_failure(e: FlowError, lang: &str, brand: &Shown) -> Response {
    match e {
        FlowError::Gone => html(
            StatusCode::GONE,
            page::error_page_with(brand, lang, "err.gone"),
            None,
        ),
        FlowError::Forbidden => html(
            StatusCode::FORBIDDEN,
            page::error_page_with(brand, lang, "err.forbidden"),
            None,
        ),
        FlowError::Busy(wait) => {
            let mut r = html(
                StatusCode::TOO_MANY_REQUESTS,
                page::error_page_with(brand, lang, "err.busy"),
                None,
            );
            if let Ok(v) = HeaderValue::from_str(&wait.to_string()) {
                r.headers_mut().insert(header::RETRY_AFTER, v);
            }
            r
        }
    }
}

// ---- metadata ------------------------------------------------------------------------------

pub(crate) async fn authorization_server(State(s): Shared, h: HeaderMap) -> Response {
    if s.oauth.is_none() {
        return disabled();
    }
    // An anonymous document: the brand is asked first and once, as the discovery documents do.
    let brand = super::mcp::document_brand(&s).await;
    let base = s.mcp.base_url(&h);
    super::mcp::public_json(
        "application/json",
        &obop_oauth::authorization_server_metadata(&base, &brand),
    )
}

// ---- registration, token, revocation ---------------------------------------------------------

pub(crate) async fn preflight() -> Response {
    let mut r = StatusCode::NO_CONTENT.into_response();
    let h = r.headers_mut();
    h.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    h.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("POST, OPTIONS"),
    );
    h.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("Content-Type, Authorization"),
    );
    h.insert(
        header::ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_static("600"),
    );
    r
}

pub(crate) async fn register(State(s): Shared, parts: Parts, body: Bytes) -> Response {
    blocking(move || {
        let Some(o) = s.oauth.clone() else {
            return disabled();
        };
        let body: J = match serde_json::from_slice(&body) {
            Ok(b) => b,
            Err(_) => {
                return oauth_error(&OAuthError::new(
                    400,
                    "invalid_client_metadata",
                    "the body must be JSON",
                ));
            }
        };
        match o.register(&client_ip(&s, &parts), &body) {
            Ok(doc) => oauth_json(StatusCode::CREATED, &doc),
            Err(e) => oauth_error(&e),
        }
    })
    .await
}

fn form_body(parts: &Parts, body: &Bytes) -> Result<Form, OAuthError> {
    let ct = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !ct
        .to_ascii_lowercase()
        .starts_with("application/x-www-form-urlencoded")
    {
        return Err(OAuthError::new(
            400,
            "invalid_request",
            "send application/x-www-form-urlencoded",
        ));
    }
    std::str::from_utf8(body)
        .ok()
        .and_then(parse_pairs)
        .map(Form)
        .ok_or_else(|| OAuthError::new(400, "invalid_request", "the body is not valid form data"))
}

pub(crate) async fn token(State(s): Shared, parts: Parts, body: Bytes) -> Response {
    blocking(move || {
        let Some(o) = s.oauth.clone() else {
            return disabled();
        };
        let form = match form_body(&parts, &body) {
            Ok(f) => f,
            Err(e) => return oauth_error(&e),
        };
        let base = s.mcp.base_url(&parts.headers);
        match o.token(&base, &client_ip(&s, &parts), &form) {
            Ok(doc) => oauth_json(StatusCode::OK, &doc),
            Err(e) => oauth_error(&e),
        }
    })
    .await
}

pub(crate) async fn revoke(State(s): Shared, parts: Parts, body: Bytes) -> Response {
    blocking(move || {
        let Some(o) = s.oauth.clone() else {
            return disabled();
        };
        let form = match form_body(&parts, &body) {
            Ok(f) => f,
            Err(e) => return oauth_error(&e),
        };
        match o.revoke_presented(&client_ip(&s, &parts), &form) {
            Ok(()) => oauth_json(StatusCode::OK, &json!({})),
            Err(e) => oauth_error(&e),
        }
    })
    .await
}

// ---- the authorization endpoint --------------------------------------------------------------

fn params_of(pairs: &[(String, String)]) -> Result<AuthorizeParams, ()> {
    // Each parameter at most once, except `resource` (RFC 8707 allows several).
    for (i, (k, _)) in pairs.iter().enumerate() {
        if k != "resource" && pairs[..i].iter().any(|(o, _)| o == k) {
            return Err(());
        }
    }
    let one = |k: &str| pair(pairs, k).map(String::from);
    Ok(AuthorizeParams {
        response_type: one("response_type"),
        client_id: one("client_id"),
        redirect_uri: one("redirect_uri"),
        scope: one("scope"),
        state: one("state"),
        code_challenge: one("code_challenge"),
        code_challenge_method: one("code_challenge_method"),
        resources: pairs
            .iter()
            .filter(|(k, _)| k == "resource")
            .map(|(_, v)| v.clone())
            .collect(),
        lang: one("lang"),
    })
}

pub(crate) async fn authorize(State(s): Shared, parts: Parts) -> Response {
    blocking(move || authorize_blocking(&s, &parts)).await
}

fn authorize_blocking(s: &HttpState, parts: &Parts) -> Response {
    let Some(o) = s.oauth.clone() else {
        return disabled();
    };
    let brand = read_brand(s);
    let base = s.mcp.base_url(&parts.headers);
    let pairs = parts.uri.query().and_then(parse_pairs).unwrap_or_default();
    let lang = lang_of(&parts.headers, pair(&pairs, "lang"));
    let Ok(p) = params_of(&pairs) else {
        return html(
            StatusCode::BAD_REQUEST,
            page::error_page_with(&brand, lang, "err.client"),
            None,
        );
    };
    let v = match o.validate_authorize(&base, &p) {
        Ok(v) => v,
        Err(AuthzError::Page { key }) => {
            return html(
                StatusCode::BAD_REQUEST,
                page::error_page_with(&brand, lang, key),
                None,
            );
        }
        Err(AuthzError::Redirect {
            redirect_uri,
            state,
            error,
            description,
        }) => {
            return see_other(&o.error_redirect(
                &base,
                &redirect_uri,
                state.as_deref(),
                error,
                description,
            ));
        }
    };
    let (id, binder) = match o.begin(&v, &client_ip(s, parts), lang) {
        Ok(x) => x,
        Err(e) => {
            return html(
                StatusCode::from_u16(e.status).unwrap_or(StatusCode::TOO_MANY_REQUESTS),
                page::error_page_with(&brand, lang, "err.busy"),
                None,
            );
        }
    };
    let secure = if base.starts_with("https://") {
        "; Secure"
    } else {
        ""
    };
    let cookie = format!(
        "{}={binder}; Max-Age={}; Path=/oauth; HttpOnly; SameSite=Lax{secure}",
        cookie_name(&id),
        o.config().flow_ttl_secs
    );
    match flow_view(s, &o, &id, &binder, &brand) {
        Ok(ctx) => html(StatusCode::OK, page::login_page(&ctx, None), Some(cookie)),
        Err(e) => flow_failure(e, lang, &brand),
    }
}

/// Whether this instance can send a person to Microsoft: the connectors runtime exists and the
/// provider is configured and enabled (the same test as `GET /api/auth/sso`).
pub(crate) fn microsoft_offered(s: &HttpState) -> bool {
    s.connectors.get().is_some() && s.auth.sso().is_some_and(|a| a.offered())
}

/// The page context of a flow, with the Microsoft step offered when the flow is a mobile app's and
/// the instance has it, and with the brand the handler read at the start of its request.
/// (`obop-oauth` cannot see the provider or the reader; an MCP client is never offered the step.)
fn flow_view(
    s: &HttpState,
    o: &OAuth,
    id: &str,
    binder: &str,
    brand: &Arc<Shown>,
) -> Result<FlowCtx, FlowError> {
    let mut ctx = o.view(id, binder, brand)?;
    ctx.microsoft = ctx.class.is_mobile() && microsoft_offered(s);
    Ok(ctx)
}

/// What every POST step shares: the origin check, the form, the flow id and its browser secret.
struct Step {
    o: Arc<obop_oauth::OAuth>,
    base: String,
    pairs: Vec<(String, String)>,
    flow: String,
    csrf: String,
    binder: String,
    ip: String,
    lang: &'static str,
    /// What the request read at its start; every page of the request shows this one value.
    brand: Arc<Shown>,
}

fn step(s: &HttpState, parts: &Parts, body: &Bytes) -> Result<Step, Box<Response>> {
    let lang = lang_of(&parts.headers, None);
    let Some(o) = s.oauth.clone() else {
        return Err(Box::new(disabled()));
    };
    let brand = read_brand(s);
    let base = s.mcp.base_url(&parts.headers);
    if !same_origin(&parts.headers, &base) {
        return Err(Box::new(html(
            StatusCode::FORBIDDEN,
            page::error_page_with(&brand, lang, "err.forbidden"),
            None,
        )));
    }
    let ct = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let pairs = if ct.starts_with("application/x-www-form-urlencoded") {
        std::str::from_utf8(body).ok().and_then(parse_pairs)
    } else {
        None
    };
    let Some(pairs) = pairs else {
        return Err(Box::new(html(
            StatusCode::BAD_REQUEST,
            page::error_page_with(&brand, lang, "err.forbidden"),
            None,
        )));
    };
    let flow = pair(&pairs, "flow").unwrap_or("").to_string();
    let csrf = pair(&pairs, "csrf").unwrap_or("").to_string();
    let binder = cookie_value(&parts.headers, &cookie_name(&flow)).unwrap_or_default();
    Ok(Step {
        o,
        base,
        pairs,
        flow,
        csrf,
        binder,
        ip: client_ip(s, parts),
        lang,
        brand,
    })
}

fn consent_or_gone(st: &Step, error: Option<&str>) -> Response {
    match st.o.view(&st.flow, &st.binder, &st.brand) {
        Ok(ctx) => html(StatusCode::OK, page::consent_page(&ctx, error), None),
        Err(e) => flow_failure(e, st.lang, &st.brand),
    }
}

fn login_again(ctx: &FlowCtx, error: Option<&str>) -> Response {
    html(StatusCode::OK, page::login_page(ctx, error), None)
}

fn after_login(s: &HttpState, st: &Step, out: Result<LoginOut, FlowError>) -> Response {
    finish_login(s, &st.o, &st.flow, &st.binder, st.lang, &st.brand, out)
}

/// The page that follows a sign-in step, for the flow and the browser it was made for. Shared by
/// the password, second-step and passkey posts and by the return from Microsoft.
fn finish_login(
    s: &HttpState,
    o: &OAuth,
    flow: &str,
    binder: &str,
    fallback_lang: &str,
    brand: &Arc<Shown>,
    out: Result<LoginOut, FlowError>,
) -> Response {
    let ctx = match flow_view(s, o, flow, binder, brand) {
        Ok(c) => c,
        Err(e) => return flow_failure(e, fallback_lang, brand),
    };
    let lang = ctx.lang.clone();
    match out {
        Ok(LoginOut::Consent) => html(StatusCode::OK, page::consent_page(&ctx, None), None),
        Ok(LoginOut::Mfa(t)) => {
            let pk = o.mfa_methods(&t).contains(&"passkey");
            html(StatusCode::OK, page::mfa_page(&ctx, &t, None, pk), None)
        }
        Ok(LoginOut::MfaRetry(t)) => {
            let pk = o.mfa_methods(&t).contains(&"passkey");
            html(
                StatusCode::OK,
                page::mfa_page(&ctx, &t, Some("err.code"), pk),
                None,
            )
        }
        Ok(LoginOut::Failed(key)) => login_again(&ctx, Some(key)),
        Err(e) => flow_failure(e, &lang, brand),
    }
}

pub(crate) async fn login(State(s): Shared, parts: Parts, body: Bytes) -> Response {
    blocking(move || {
        let st = match step(&s, &parts, &body) {
            Ok(st) => st,
            Err(r) => return *r,
        };
        let out = st.o.login(
            &st.flow,
            &st.binder,
            &st.csrf,
            &st.ip,
            pair(&st.pairs, "username").unwrap_or(""),
            pair(&st.pairs, "password").unwrap_or(""),
        );
        after_login(&s, &st, out)
    })
    .await
}

pub(crate) async fn mfa(State(s): Shared, parts: Parts, body: Bytes) -> Response {
    blocking(move || {
        let st = match step(&s, &parts, &body) {
            Ok(st) => st,
            Err(r) => return *r,
        };
        let out = st.o.login_mfa(
            &st.flow,
            &st.binder,
            &st.csrf,
            &st.ip,
            pair(&st.pairs, "mfa_token").unwrap_or(""),
            pair(&st.pairs, "code").unwrap_or(""),
        );
        after_login(&s, &st, out)
    })
    .await
}

pub(crate) async fn decision(State(s): Shared, parts: Parts, body: Bytes) -> Response {
    blocking(move || {
        let st = match step(&s, &parts, &body) {
            Ok(st) => st,
            Err(r) => return *r,
        };
        match pair(&st.pairs, "decision").unwrap_or("") {
            "switch" => match st.o.switch_user(&st.flow, &st.binder, &st.csrf) {
                Ok(()) => match flow_view(&s, &st.o, &st.flow, &st.binder, &st.brand) {
                    Ok(ctx) => html(StatusCode::OK, page::login_page(&ctx, None), None),
                    Err(e) => flow_failure(e, st.lang, &st.brand),
                },
                Err(e) => flow_failure(e, st.lang, &st.brand),
            },
            d @ ("allow" | "deny") => {
                let groups: Vec<String> = st
                    .pairs
                    .iter()
                    .filter(|(k, _)| k == "group")
                    .map(|(_, v)| v.clone())
                    .collect();
                let days = pair(&st.pairs, "days")
                    .and_then(|d| d.parse::<u32>().ok())
                    .unwrap_or(0);
                match st.o.decide(
                    &st.base,
                    &st.flow,
                    &st.binder,
                    &st.csrf,
                    d == "allow",
                    &groups,
                    days,
                ) {
                    Ok(DecideOut::Redirect(loc)) => see_other(&loc),
                    Ok(DecideOut::Retry(key)) => consent_or_gone(&st, Some(key)),
                    Err(e) => flow_failure(e, st.lang, &st.brand),
                }
            }
            _ => html(
                StatusCode::BAD_REQUEST,
                page::error_page_with(&st.brand, st.lang, "err.forbidden"),
                None,
            ),
        }
    })
    .await
}

// ---- the Microsoft step of a mobile app's sign-in -----------------------------------------------

/// The sign-in page again, with the text of a word of the Microsoft step. Nothing is stored: the
/// start has not begun anything yet.
fn login_with_word(
    s: &HttpState,
    o: &OAuth,
    flow: &str,
    binder: &str,
    lang: &str,
    brand: &Arc<Shown>,
    word: &str,
) -> Response {
    match flow_view(s, o, flow, binder, brand) {
        Ok(ctx) => login_again(&ctx, Some(microsoft_key(word))),
        Err(e) => flow_failure(e, lang, brand),
    }
}

/// `GET /oauth/authorize/microsoft?flow=<id>`: the link on the sign-in page of a mobile app. It
/// begins the round trip to the provider and redirects the browser there. The flow must be a
/// mobile app's, nobody may be signed in on it yet, and the browser must hold its cookie (a link
/// is no form, so there is no token to check; what it starts is only a round trip). The registered
/// redirect address is the one of the back office (`/api/auth/oidc/callback`); the OAuth flow's id
/// goes into the stored row of the round trip, and nothing else of it.
pub(crate) async fn microsoft(State(s): Shared, parts: Parts) -> Response {
    blocking(move || microsoft_blocking(&s, &parts)).await
}

fn microsoft_blocking(s: &HttpState, parts: &Parts) -> Response {
    let lang = lang_of(&parts.headers, None);
    let Some(o) = s.oauth.clone() else {
        return disabled();
    };
    let brand = read_brand(s);
    if !super::sso::is_navigation(parts) {
        return super::sso::not_a_navigation();
    }
    let pairs = parts.uri.query().and_then(parse_pairs).unwrap_or_default();
    let flow = pair(&pairs, "flow").unwrap_or("");
    let binder = cookie_value(&parts.headers, &cookie_name(flow)).unwrap_or_default();
    let lang = match o.microsoft_start(flow, &binder) {
        Ok(l) => page::pick_lang(Some(&l), None),
        Err(e) => return flow_failure(e, lang, &brand),
    };
    let ip = super::sso::source_of(s, parts);
    s.shared.maybe_purge();
    // Its own windows, so that phones signing in from one address do not use up the allowance of
    // the back office's link.
    if super::sso::limited(s, "oauth_start", &ip, 30) {
        return login_with_word(s, &o, flow, &binder, lang, &brand, "busy");
    }
    let (Some(api), Some(rt)) = (s.auth.sso(), s.connectors.get()) else {
        return login_with_word(s, &o, flow, &binder, lang, &brand, "unavailable");
    };
    let leg = match api.leg() {
        Ok(Some(l)) => l,
        Ok(None) => return login_with_word(s, &o, flow, &binder, lang, &brand, "unavailable"),
        Err(e) => {
            api.audit_throttled(
                &ip,
                "oidc_error",
                obop_commands::AnonLine::Settings,
                &format!("start (app): {}", super::sso::describe(&e)),
            );
            return login_with_word(s, &o, flow, &binder, lang, &brand, "unavailable");
        }
    };
    let ep = match super::sso::discover(s, api, rt, &leg, "start (app)") {
        Ok(ep) => ep,
        Err(r) => {
            let word = r.log(api, &ip);
            return login_with_word(s, &o, flow, &binder, lang, &brand, word);
        }
    };
    let redirect_uri = format!("{}/api/auth/oidc/callback", s.mcp.base_url(&parts.headers));
    let Some(begun) = super::sso::begin_flow(
        s,
        &leg,
        &ep,
        &redirect_uri,
        Some(json!({"purpose": "oauth", "oauth_flow": flow})),
    ) else {
        return login_with_word(s, &o, flow, &binder, lang, &brand, "unavailable");
    };
    api.audit_throttled(
        &ip,
        "oidc_start",
        obop_commands::AnonLine::Start,
        "a sign-in with Microsoft was begun from an app",
    );
    with_cookies(see_other(&begun.url), &[begun.cookie])
}

/// `GET /oauth/authorize/return?flow=<id>`: where the callback sends the browser back to. It reads
/// what the callback left in the flow and writes nothing into it, so reloading it, or going Back
/// to it, shows the same page and spends no flow state. The one outcome that writes elsewhere is a
/// pending second factor: each call counts one hit of the per-address sign-in window and either
/// mints a fresh `mfa_token` or, when the second step is refused, writes one `oidc_refused` line.
pub(crate) async fn microsoft_return(State(s): Shared, parts: Parts) -> Response {
    blocking(move || {
        let lang = lang_of(&parts.headers, None);
        let Some(o) = s.oauth.clone() else {
            return disabled();
        };
        let brand = read_brand(&s);
        if !super::sso::is_navigation(&parts) {
            return super::sso::not_a_navigation();
        }
        let pairs = parts.uri.query().and_then(parse_pairs).unwrap_or_default();
        let flow = pair(&pairs, "flow").unwrap_or("");
        let binder = cookie_value(&parts.headers, &cookie_name(flow)).unwrap_or_default();
        let out = o.microsoft_return(flow, &binder, &client_ip(&s, &parts));
        finish_login(&s, &o, flow, &binder, lang, &brand, out)
    })
    .await
}

// ---- the owner's connected applications ------------------------------------------------------

/// A signed-in person (not an agent, not a session limited to enrolment).
fn person(s: &HttpState, h: &HeaderMap) -> Result<obop_commands::Principal, Box<Response>> {
    match s.auth(h) {
        Ok((p, _)) if p.is_person() => Ok(p),
        Ok(_) => Err(Box::new(StatusCode::NOT_FOUND.into_response())),
        Err(r) => Err(Box::new(r.into_response())),
    }
}

pub(crate) async fn list_grants(State(s): Shared, h: HeaderMap) -> Response {
    blocking(move || {
        let Some(o) = s.oauth.clone() else {
            return disabled();
        };
        match person(&s, &h) {
            Ok(p) => axum::Json(json!({"grants": o.grants_of(&p.id)})).into_response(),
            Err(r) => *r,
        }
    })
    .await
}

pub(crate) async fn revoke_grant(
    State(s): Shared,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    blocking(move || {
        let Some(o) = s.oauth.clone() else {
            return disabled();
        };
        match person(&s, &h) {
            Ok(p) => match o.revoke_grant(&p.id, &id) {
                Ok(true) => axum::Json(json!({"revoked": true})).into_response(),
                Ok(false) => StatusCode::NOT_FOUND.into_response(),
                Err(e) => oauth_error(&e),
            },
            Err(r) => *r,
        }
    })
    .await
}

/// The ceremony site of a page request (the same rule as the REST routes).
fn webauthn_ctx(s: &HttpState, parts: &Parts, st: &Step) -> Option<obop_commands::WebauthnCtx> {
    let trust = s.trust_proxy.load(std::sync::atomic::Ordering::Relaxed);
    let (origin, rp_id) = s.mcp.webauthn_origin(&parts.headers, trust)?;
    Some(obop_commands::WebauthnCtx {
        rp_id,
        origin,
        rp_name: "obop".into(),
        ip: st.ip.clone(),
    })
}

/// A script on the sign-in page asks for a passkey challenge (form-encoded: `flow`, `csrf` and,
/// for the second step, `mfa_token`; the flow cookie comes along). Answers JSON.
pub(crate) async fn passkey_options(State(s): Shared, parts: Parts, body: Bytes) -> Response {
    blocking(move || {
        let st = match step(&s, &parts, &body) {
            Ok(st) => st,
            Err(r) => return *r,
        };
        let Some(c) = webauthn_ctx(&s, &parts, &st) else {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        };
        let mfa = pair(&st.pairs, "mfa_token").filter(|t| !t.is_empty());
        match st
            .o
            .passkey_options(&c, &st.flow, &st.binder, &st.csrf, mfa)
        {
            Ok(j) => oauth_json(StatusCode::OK, &j),
            Err(FlowError::Busy(wait)) => {
                let mut r = StatusCode::TOO_MANY_REQUESTS.into_response();
                if let Ok(v) = HeaderValue::from_str(&wait.to_string()) {
                    r.headers_mut().insert(header::RETRY_AFTER, v);
                }
                harden(&mut r);
                r
            }
            Err(_) => {
                let mut r = StatusCode::FORBIDDEN.into_response();
                harden(&mut r);
                r
            }
        }
    })
    .await
}

/// The page's form posts the browser's answer (`credential`, JSON) with `flow`, `csrf` and
/// optionally `mfa_token`.
pub(crate) async fn passkey(State(s): Shared, parts: Parts, body: Bytes) -> Response {
    blocking(move || {
        let st = match step(&s, &parts, &body) {
            Ok(st) => st,
            Err(r) => return *r,
        };
        let Some(c) = webauthn_ctx(&s, &parts, &st) else {
            return html(
                StatusCode::SERVICE_UNAVAILABLE,
                page::error_page_with(&st.brand, st.lang, "err.busy"),
                None,
            );
        };
        let mfa = pair(&st.pairs, "mfa_token").filter(|t| !t.is_empty());
        let credential: J = pair(&st.pairs, "credential")
            .and_then(|c| serde_json::from_str(c).ok())
            .unwrap_or(J::Null);
        let out =
            st.o.login_passkey(&c, &st.flow, &st.binder, &st.csrf, mfa, &credential);
        after_login(&s, &st, out)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forms_decode_and_reject_garbage() {
        let p = parse_pairs("a=1&b=x%20y+z&c=%C3%A4&d=").unwrap();
        assert_eq!(pair(&p, "b"), Some("x y z"));
        assert_eq!(pair(&p, "c"), Some("ä"));
        assert_eq!(pair(&p, "d"), Some(""));
        assert!(parse_pairs("a=%zz").is_none());
        assert!(parse_pairs("a=%ff").is_none());
    }

    #[test]
    fn origin_check_needs_an_origin_or_fetch_metadata() {
        let mut h = HeaderMap::new();
        assert!(!same_origin(&h, "https://a.test"));
        h.insert("origin", HeaderValue::from_static("https://a.test"));
        assert!(same_origin(&h, "https://a.test"));
        h.insert("origin", HeaderValue::from_static("https://evil.test"));
        assert!(!same_origin(&h, "https://a.test"));
        h.remove("origin");
        h.insert("sec-fetch-site", HeaderValue::from_static("cross-site"));
        assert!(!same_origin(&h, "https://a.test"));
        h.insert("sec-fetch-site", HeaderValue::from_static("same-origin"));
        assert!(same_origin(&h, "https://a.test"));
        // `Origin: null` is judged by fetch metadata, never accepted by itself.
        h.insert("origin", HeaderValue::from_static("null"));
        assert!(same_origin(&h, "https://a.test"));
        h.insert("sec-fetch-site", HeaderValue::from_static("cross-site"));
        assert!(!same_origin(&h, "https://a.test"));
        h.remove("sec-fetch-site");
        assert!(!same_origin(&h, "https://a.test"));
    }

    /// An identity provider that issues nothing and answers no one, only enough for the OAuth
    /// server to exist (it needs the provider's `oauth()` port).
    struct Idp;

    impl obop_commands::OAuthTokens for Idp {
        fn principal_of(&self, _: &str) -> Option<obop_commands::Principal> {
            None
        }
        fn issue_oauth_access(
            &self,
            _: &str,
            _: &[String],
            _: &str,
            _: u64,
            _: &str,
            _: &str,
        ) -> Result<(String, String), String> {
            Err("not in this test".into())
        }
        fn revoke_token_id(&self, _: &str) -> bool {
            false
        }
        fn revoke_session(&self, _: &str) -> bool {
            false
        }
        fn grant_of(&self, _: &str) -> Option<String> {
            None
        }
        fn audit(&self, _: &str, _: &str, _: &str, _: &str) {}
    }

    impl obop_commands::Authenticator for Idp {
        fn resolve(&self, _: &str) -> Option<obop_commands::Principal> {
            None
        }
        fn login(&self, _: &str, _: &str) -> Result<String, String> {
            Err("not in this test".into())
        }
        fn issue_agent_token(
            &self,
            _: &obop_commands::Principal,
            _: &[String],
            _: &str,
        ) -> Result<String, String> {
            Err("not in this test".into())
        }
        fn oauth(&self) -> Option<&dyn obop_commands::OAuthTokens> {
            Some(self)
        }
    }

    /// The passkey steps of the OAuth pages build the ceremony site with the relying party name
    /// "obop" (S1a-0 of the branding work: with no brand row that is what an instance sends). The
    /// name reaches an authenticator only through the registration options, which the REST
    /// routes make (pinned in `passkey_tests` of the `obop` crate); this holds the same literal
    /// here, where `webauthn_ctx` is visible, together with the site it builds.
    #[test]
    fn pins_the_relying_party_name_of_the_oauth_pages_ceremony_site() {
        let state = crate::HttpState::with_mcp(
            Arc::new(obop_commands::Dispatcher::new()),
            Arc::new(Idp),
            crate::mcp::McpConfig {
                public_url: Some("https://inst.test".into()),
                ..crate::mcp::McpConfig::default()
            },
        );
        let st = Step {
            o: state.oauth.clone().expect("the OAuth server exists"),
            base: "https://inst.test".into(),
            pairs: Vec::new(),
            flow: String::new(),
            csrf: String::new(),
            binder: String::new(),
            ip: "198.51.100.7".into(),
            lang: "en",
            brand: Arc::new(Shown::not_set(None)),
        };
        let parts = axum::http::Request::builder()
            .uri("/oauth/authorize/passkey/options")
            .body(())
            .unwrap()
            .into_parts()
            .0;
        let ctx = webauthn_ctx(&state, &parts, &st).expect("a ceremony site");
        assert_eq!(ctx.rp_name, "obop");
        assert_eq!(ctx.rp_id, "inst.test");
        assert_eq!(ctx.origin, "https://inst.test");
        assert_eq!(ctx.ip, "198.51.100.7");
    }
}
