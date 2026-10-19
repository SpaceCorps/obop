//! The HTTP side of the OAuth authorization server (D64): the metadata document, dynamic client
//! registration, the authorization endpoint with its sign-in and consent pages, the token and
//! revocation endpoints, and the owner's list of connected applications. The rules and the state
//! are in `obop-oauth`; this module only carries requests, sets the headers and checks where a
//! browser request came from.

use super::{HttpState, blocking};
use axum::{
    body::Bytes,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use obop_commands::PrincipalKind;
use obop_oauth::flow::AuthorizeParams;
use obop_oauth::page::{self, Rendered};
use obop_oauth::{AuthzError, DecideOut, FlowCtx, FlowError, Form, LoginOut, OAuthError};
use serde_json::{Value as J, json};
use std::net::SocketAddr;
use std::sync::Arc;

type Shared = State<Arc<HttpState>>;

/// Size of the bodies these endpoints read.
pub(crate) const MAX_BODY: usize = 16 * 1024;

// ---- plumbing ------------------------------------------------------------------------------

/// The address rate limits are keyed on: the peer, or behind a trusted proxy the nearest
/// `X-Forwarded-For` entry (the same rule as the public site).
pub(crate) fn client_ip(s: &HttpState, parts: &Parts) -> String {
    if s.trust_proxy.load(std::sync::atomic::Ordering::Relaxed)
        && let Some(v) = parts
            .headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
        && let Some(last) = v.rsplit(',').next()
    {
        return last.trim().chars().take(64).collect();
    }
    parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip().to_string())
        .unwrap_or_default()
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
fn parse_pairs(s: &str) -> Option<Vec<(String, String)>> {
    s.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            Some((pct_decode(k)?, pct_decode(v)?))
        })
        .collect()
}

fn pair<'a>(pairs: &'a [(String, String)], k: &str) -> Option<&'a str> {
    pairs.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str())
}

/// Headers every page and every token response carries.
fn harden(r: &mut Response) {
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
        HeaderValue::from_static("Retry-After"),
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

fn see_other(location: &str) -> Response {
    let mut r = StatusCode::SEE_OTHER.into_response();
    if let Ok(v) = HeaderValue::from_str(location) {
        r.headers_mut().insert(header::LOCATION, v);
    }
    harden(&mut r);
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

fn cookie_value(h: &HeaderMap, name: &str) -> Option<String> {
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
fn same_origin(h: &HeaderMap, base: &str) -> bool {
    match h.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        Some(o) if o != "null" => o.trim_end_matches('/').eq_ignore_ascii_case(base),
        _ => h.get("sec-fetch-site").and_then(|v| v.to_str().ok()) == Some("same-origin"),
    }
}

fn flow_failure(e: FlowError, lang: &str) -> Response {
    match e {
        FlowError::Gone => html(StatusCode::GONE, page::error_page(lang, "err.gone"), None),
        FlowError::Forbidden => html(
            StatusCode::FORBIDDEN,
            page::error_page(lang, "err.forbidden"),
            None,
        ),
        FlowError::Busy(wait) => {
            let mut r = html(
                StatusCode::TOO_MANY_REQUESTS,
                page::error_page(lang, "err.busy"),
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
    let base = s.mcp.base_url(&h);
    super::mcp::public_json(
        "application/json",
        &obop_oauth::authorization_server_metadata(&base),
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
    let base = s.mcp.base_url(&parts.headers);
    let pairs = parts.uri.query().and_then(parse_pairs).unwrap_or_default();
    let lang = lang_of(&parts.headers, pair(&pairs, "lang"));
    let Ok(p) = params_of(&pairs) else {
        return html(
            StatusCode::BAD_REQUEST,
            page::error_page(lang, "err.client"),
            None,
        );
    };
    let v = match o.validate_authorize(&base, &p) {
        Ok(v) => v,
        Err(AuthzError::Page { key }) => {
            return html(StatusCode::BAD_REQUEST, page::error_page(lang, key), None);
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
                page::error_page(lang, "err.busy"),
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
    match o.view(&id, &binder) {
        Ok(ctx) => html(StatusCode::OK, page::login_page(&ctx, None), Some(cookie)),
        Err(e) => flow_failure(e, lang),
    }
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
}

fn step(s: &HttpState, parts: &Parts, body: &Bytes) -> Result<Step, Box<Response>> {
    let lang = lang_of(&parts.headers, None);
    let Some(o) = s.oauth.clone() else {
        return Err(Box::new(disabled()));
    };
    let base = s.mcp.base_url(&parts.headers);
    if !same_origin(&parts.headers, &base) {
        return Err(Box::new(html(
            StatusCode::FORBIDDEN,
            page::error_page(lang, "err.forbidden"),
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
            page::error_page(lang, "err.forbidden"),
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
    })
}

fn consent_or_gone(st: &Step, error: Option<&str>) -> Response {
    match st.o.view(&st.flow, &st.binder) {
        Ok(ctx) => html(StatusCode::OK, page::consent_page(&ctx, error), None),
        Err(e) => flow_failure(e, st.lang),
    }
}

fn login_again(st: &Step, ctx: &FlowCtx, error: Option<&str>) -> Response {
    let _ = st;
    html(StatusCode::OK, page::login_page(ctx, error), None)
}

fn after_login(st: &Step, out: Result<LoginOut, FlowError>) -> Response {
    let ctx = match st.o.view(&st.flow, &st.binder) {
        Ok(c) => c,
        Err(e) => return flow_failure(e, st.lang),
    };
    let lang = ctx.lang.clone();
    match out {
        Ok(LoginOut::Consent) => html(StatusCode::OK, page::consent_page(&ctx, None), None),
        Ok(LoginOut::Mfa(t)) => {
            let pk = st.o.mfa_methods(&t).contains(&"passkey");
            html(StatusCode::OK, page::mfa_page(&ctx, &t, None, pk), None)
        }
        Ok(LoginOut::MfaRetry(t)) => {
            let pk = st.o.mfa_methods(&t).contains(&"passkey");
            html(
                StatusCode::OK,
                page::mfa_page(&ctx, &t, Some("err.code"), pk),
                None,
            )
        }
        Ok(LoginOut::Failed(key)) => login_again(st, &ctx, Some(key)),
        Err(e) => flow_failure(e, &lang),
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
        after_login(&st, out)
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
        after_login(&st, out)
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
                Ok(()) => match st.o.view(&st.flow, &st.binder) {
                    Ok(ctx) => html(StatusCode::OK, page::login_page(&ctx, None), None),
                    Err(e) => flow_failure(e, st.lang),
                },
                Err(e) => flow_failure(e, st.lang),
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
                    Err(e) => flow_failure(e, st.lang),
                }
            }
            _ => html(
                StatusCode::BAD_REQUEST,
                page::error_page(st.lang, "err.forbidden"),
                None,
            ),
        }
    })
    .await
}

// ---- the owner's connected applications ------------------------------------------------------

/// A signed-in person (not an agent, not a session limited to enrolment).
fn person(s: &HttpState, h: &HeaderMap) -> Result<obop_commands::Principal, Box<Response>> {
    match s.auth(h) {
        Ok((p, _)) if p.kind == PrincipalKind::User => Ok(p),
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
            Ok(p) if o.revoke_grant(&p.id, &id) => {
                axum::Json(json!({"revoked": true})).into_response()
            }
            Ok(_) => StatusCode::NOT_FOUND.into_response(),
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
                page::error_page(st.lang, "err.busy"),
                None,
            );
        };
        let mfa = pair(&st.pairs, "mfa_token").filter(|t| !t.is_empty());
        let credential: J = pair(&st.pairs, "credential")
            .and_then(|c| serde_json::from_str(c).ok())
            .unwrap_or(J::Null);
        let out =
            st.o.login_passkey(&c, &st.flow, &st.binder, &st.csrf, mfa, &credential);
        after_login(&st, out)
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
}
