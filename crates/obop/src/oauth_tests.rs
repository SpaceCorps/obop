//! D64 over HTTP: the MCP authorization flow against the real router and the real `Auth`. A client
//! registers, sends a person through sign in and consent, exchanges the code, calls `/mcp` with
//! the token, refreshes it, and the person revokes it. The negative cases sit next to the
//! round trip: bad verifier, reused code, wrong redirect, wrong resource, expiry, refresh reuse,
//! widening, forged and foreign posts, non-https redirects.

use crate::app::{App, DevAuth};
use crate::mfa_tests::{PW, T0, code, enrol, rig};
use axum::{
    body::Body,
    http::{HeaderMap, Request},
};
use http_body_util::BodyExt;
use obop_oauth::util::s256;
use serde_json::{Value as J, json};
use std::sync::Arc;
use tower::ServiceExt;

mod mobile;

const HOST: &str = "inst.test";
const BASE: &str = "http://inst.test";
const REDIRECT: &str = "https://claude.ai/api/mcp/auth_callback";
const VERIFIER: &str = "an-example-verifier-of-sufficient-length-0123456789";

struct Resp {
    status: u16,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl Resp {
    fn json(&self) -> J {
        serde_json::from_slice(&self.body).unwrap_or_else(|_| panic!("not JSON: {}", self.text()))
    }
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
    fn header(&self, name: &str) -> &str {
        self.headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    }
}

async fn call(
    r: &axum::Router,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<(&str, String)>,
) -> Resp {
    let mut req = Request::builder().method(method).uri(path);
    if !headers.iter().any(|(k, _)| *k == "host") {
        req = req.header("host", HOST);
    }
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let body = match body {
        Some((ct, b)) => {
            req = req.header("content-type", ct);
            Body::from(b)
        }
        None => Body::empty(),
    };
    let resp = r.clone().oneshot(req.body(body).unwrap()).await.unwrap();
    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    let body = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    Resp {
        status,
        headers,
        body,
    }
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

fn form(pairs: &[(&str, &str)]) -> (&'static str, String) {
    (
        "application/x-www-form-urlencoded",
        pairs
            .iter()
            .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
            .collect::<Vec<_>>()
            .join("&"),
    )
}

async fn register(r: &axum::Router, redirect_uris: J) -> Resp {
    call(
        r,
        "POST",
        "/oauth/register",
        &[],
        Some((
            "application/json",
            json!({"client_name": "Claude", "redirect_uris": redirect_uris,
                   "token_endpoint_auth_method": "none"})
            .to_string(),
        )),
    )
    .await
}

async fn client(r: &axum::Router) -> String {
    let c = register(r, json!([REDIRECT])).await;
    assert_eq!(c.status, 201, "{}", c.text());
    c.json()["client_id"].as_str().unwrap().to_string()
}

fn authorize_url(client: &str, redirect: &str, extra: &[(&str, &str)]) -> String {
    let mut pairs = vec![
        ("response_type", "code"),
        ("client_id", client),
        ("redirect_uri", redirect),
        ("state", "xyz"),
        ("code_challenge", ""),
        ("code_challenge_method", "S256"),
        ("resource", "http://inst.test/mcp"),
    ];
    let challenge = s256(VERIFIER);
    pairs[4].1 = &challenge;
    for (k, v) in extra {
        pairs.retain(|(n, _)| n != k);
        pairs.push((k, v));
    }
    let q: Vec<String> = pairs
        .iter()
        .map(|(k, v)| format!("{k}={}", enc(v)))
        .collect();
    format!("/oauth/authorize?{}", q.join("&"))
}

/// The value of a hidden input.
fn hidden(html: &str, name: &str) -> String {
    let marker = format!("name=\"{name}\" value=\"");
    let i = html
        .find(&marker)
        .unwrap_or_else(|| panic!("no input {name} in {html}"));
    let rest = &html[i + marker.len()..];
    rest[..rest.find('"').unwrap()].to_string()
}

/// A browser in the middle of a flow: the flow id, the CSRF token and the flow cookie.
struct Browser {
    flow: String,
    csrf: String,
    cookie: String,
}

impl Browser {
    fn headers(&self) -> Vec<(&str, &str)> {
        vec![("origin", BASE), ("cookie", &self.cookie)]
    }

    async fn post(&self, r: &axum::Router, path: &str, extra: &[(&str, &str)]) -> Resp {
        let mut pairs = vec![("flow", self.flow.as_str()), ("csrf", self.csrf.as_str())];
        pairs.extend_from_slice(extra);
        call(r, "POST", path, &self.headers(), Some(form(&pairs))).await
    }
}

async fn start(r: &axum::Router, url: &str) -> (Resp, Browser) {
    let resp = call(r, "GET", url, &[], None).await;
    assert_eq!(resp.status, 200, "{}", resp.text());
    let html = resp.text();
    let cookie = resp
        .header("set-cookie")
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let b = Browser {
        flow: hidden(&html, "flow"),
        csrf: hidden(&html, "csrf"),
        cookie,
    };
    (resp, b)
}

fn location(resp: &Resp) -> String {
    assert_eq!(resp.status, 303, "{}", resp.text());
    resp.header("location").to_string()
}

fn param(url: &str, k: &str) -> String {
    let q = url.split_once('?').unwrap().1;
    let v = q
        .split('&')
        .find_map(|kv| kv.strip_prefix(&format!("{k}=")))
        .unwrap_or_else(|| panic!("no {k} in {url}"));
    let mut out = Vec::new();
    let b = v.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            out.push(u8::from_str_radix(&v[i + 1..i + 3], 16).unwrap());
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
}

async fn sign_in_and_allow(
    r: &axum::Router,
    user: &str,
    url: &str,
    groups: &[&str],
    days: &str,
) -> Resp {
    let (_, b) = start(r, url).await;
    let consent = b
        .post(
            r,
            "/oauth/authorize/login",
            &[("username", user), ("password", PW)],
        )
        .await;
    assert_eq!(consent.status, 200, "{}", consent.text());
    assert!(
        consent.text().contains("name=\"decision\""),
        "{}",
        consent.text()
    );
    let mut extra: Vec<(&str, &str)> = vec![("decision", "allow"), ("days", days)];
    for g in groups {
        extra.push(("group", g));
    }
    b.post(r, "/oauth/authorize/decision", &extra).await
}

async fn token(r: &axum::Router, pairs: &[(&str, &str)]) -> Resp {
    call(r, "POST", "/oauth/token", &[], Some(form(pairs))).await
}

async fn exchange(r: &axum::Router, client: &str, code: &str) -> Resp {
    token(
        r,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", client),
            ("redirect_uri", REDIRECT),
            ("code_verifier", VERIFIER),
            ("resource", "http://inst.test/mcp"),
        ],
    )
    .await
}

async fn refresh(r: &axum::Router, client: &str, rt: &str) -> Resp {
    token(
        r,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", rt),
            ("client_id", client),
        ],
    )
    .await
}

async fn mcp(r: &axum::Router, bearer: &str, session: Option<&str>, msg: J) -> Resp {
    let mut hs: Vec<(&str, &str)> = vec![
        ("authorization", bearer),
        ("accept", "application/json, text/event-stream"),
    ];
    if let Some(s) = session {
        hs.push(("mcp-session-id", s));
    }
    call(
        r,
        "POST",
        "/mcp",
        &hs,
        Some(("application/json", msg.to_string())),
    )
    .await
}

/// The tool names a bearer token sees over MCP.
async fn tool_names(r: &axum::Router, access: &str) -> Vec<String> {
    let bearer = format!("Bearer {access}");
    let init = mcp(
        r,
        &bearer,
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}),
    )
    .await;
    assert_eq!(init.status, 200, "{}", init.text());
    let sid = init.header("mcp-session-id").to_string();
    let list = mcp(
        r,
        &bearer,
        Some(&sid),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    )
    .await;
    assert_eq!(list.status, 200, "{}", list.text());
    list.json()["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect()
}

fn eve(rig: &crate::mfa_tests::Rig) {
    rig.auth
        .create_user(
            "eve",
            PW,
            &["base.user".into(), "sales.user".into(), "crm.user".into()],
        )
        .unwrap();
}

#[tokio::test]
async fn metadata_challenge_and_registration() {
    let rig = rig(false);
    let r = &rig.r;
    let https = [("x-forwarded-proto", "https")];
    let prm = call(
        r,
        "GET",
        "/.well-known/oauth-protected-resource/mcp",
        &https,
        None,
    )
    .await;
    assert_eq!(prm.status, 200);
    let prm = prm.json();
    assert_eq!(prm["resource"], "https://inst.test/mcp");
    assert_eq!(prm["authorization_servers"], json!(["https://inst.test"]));
    let root = call(
        r,
        "GET",
        "/.well-known/oauth-protected-resource",
        &https,
        None,
    )
    .await
    .json();
    assert_eq!(root["authorization_servers"], json!(["https://inst.test"]));
    let m = call(
        r,
        "GET",
        "/.well-known/oauth-authorization-server",
        &https,
        None,
    )
    .await;
    assert_eq!(m.status, 200);
    let m = m.json();
    assert_eq!(m["issuer"], "https://inst.test");
    assert_eq!(
        m["authorization_endpoint"],
        "https://inst.test/oauth/authorize"
    );
    assert_eq!(
        m["registration_endpoint"],
        "https://inst.test/oauth/register"
    );
    assert_eq!(m["code_challenge_methods_supported"], json!(["S256"]));
    // The challenge points at the metadata of the /mcp resource.
    let none = mcp(r, "", None, json!({})).await;
    assert_eq!(none.status, 401);
    assert_eq!(
        none.header("www-authenticate"),
        "Bearer resource_metadata=\"http://inst.test/.well-known/oauth-protected-resource/mcp\""
    );
    // The card and the OpenAPI document name the server.
    let card = call(r, "GET", "/.well-known/mcp/server-card.json", &https, None)
        .await
        .json();
    assert_eq!(
        card["_meta"]["dev.obop/auth"]["authorizationServer"],
        "https://inst.test"
    );
    let api = call(r, "GET", "/api/openapi.json", &https, None)
        .await
        .json();
    assert_eq!(api["x-mcp"]["authorization"], "oauth2 or bearer");

    // Registration: a public client, no secret.
    let c = register(r, json!([REDIRECT, "http://127.0.0.1:8765/cb"])).await;
    assert_eq!(c.status, 201);
    assert_eq!(c.header("cache-control"), "no-store");
    let c = c.json();
    assert!(c["client_id"].as_str().unwrap().starts_with("oc_"));
    assert!(c.get("client_secret").is_none());
    assert_eq!(c["token_endpoint_auth_method"], "none");
    // Redirect URIs: https or loopback only, no wildcards, no fragments.
    for bad in [
        "http://example.com/cb",
        "https://*.example.com/cb",
        "https://example.com/cb#f",
        "myapp://cb",
        "javascript:alert(1)",
    ] {
        let e = register(r, json!([bad])).await;
        assert_eq!(e.status, 400, "{bad}");
        assert_eq!(e.json()["error"], "invalid_redirect_uri", "{bad}");
    }
    let e = call(
        r,
        "POST",
        "/oauth/register",
        &[],
        Some(("application/json", "not json".into())),
    )
    .await;
    assert_eq!(
        (e.status, e.json()["error"].clone()),
        (400, json!("invalid_client_metadata"))
    );
    // CORS for browser-based clients.
    let pre = call(
        r,
        "OPTIONS",
        "/oauth/token",
        &[("origin", "https://app.test")],
        None,
    )
    .await;
    assert_eq!(pre.status, 204);
    assert_eq!(pre.header("access-control-allow-origin"), "*");
}

#[tokio::test]
async fn registration_is_rate_limited() {
    let rig = rig(false);
    let mut last = 0;
    for _ in 0..25 {
        last = register(&rig.r, json!([REDIRECT])).await.status;
        if last == 429 {
            break;
        }
    }
    assert_eq!(last, 429);
    let e = register(&rig.r, json!([REDIRECT])).await;
    assert!(!e.header("retry-after").is_empty());
}

#[tokio::test]
async fn the_whole_flow_gives_a_narrow_token_that_works_on_mcp_only() {
    let rig = rig(false);
    eve(&rig);
    let r = &rig.r;
    let c = client(r).await;
    let url = authorize_url(&c, REDIRECT, &[]);

    // The first page: sign in, with every hardening header.
    let (page, b) = start(r, &url).await;
    let h = page.header("content-security-policy").to_string();
    assert!(
        h.contains("default-src 'none'") && h.contains("frame-ancestors 'none'"),
        "{h}"
    );
    assert!(h.contains("form-action 'self'"));
    assert_eq!(page.header("x-content-type-options"), "nosniff");
    assert_eq!(page.header("referrer-policy"), "same-origin");
    assert_eq!(page.header("cache-control"), "no-store");
    assert_eq!(page.header("x-frame-options"), "DENY");
    let cookie = page.header("set-cookie");
    assert!(
        cookie.contains("HttpOnly")
            && cookie.contains("SameSite=Lax")
            && cookie.contains("Path=/oauth"),
        "{cookie}"
    );
    let html = page.text();
    assert!(html.contains("<html lang=\"en\"") && html.contains("<label for=\"u\">Username"));
    assert!(html.contains("Claude"));

    // A wrong password stays on the sign-in page, with a message.
    let bad = b
        .post(
            r,
            "/oauth/authorize/login",
            &[("username", "eve"), ("password", "wrong password!")],
        )
        .await;
    assert_eq!(bad.status, 200);
    assert!(bad.text().contains("role=\"alert\"") && bad.text().contains("not right"));

    // Signed in: the consent page lists what eve may hand over, not what she lacks.
    let consent = b
        .post(
            r,
            "/oauth/authorize/login",
            &[("username", "eve"), ("password", PW)],
        )
        .await;
    let html = consent.text();
    for g in ["base.user", "sales.user", "crm.user"] {
        assert!(html.contains(&format!("value=\"{g}\"")), "{g}");
    }
    assert!(!html.contains("invoicing.manager") && !html.contains("platform.admin"));
    assert!(html.contains("cannot approve its own proposals"));
    assert!(html.contains("claude.ai"), "where the browser will go");
    assert!(
        consent
            .header("content-security-policy")
            .contains("form-action 'self' https://claude.ai")
    );

    // Allow only sales.user.
    let allowed = b
        .post(
            r,
            "/oauth/authorize/decision",
            &[
                ("decision", "allow"),
                ("days", "7"),
                ("group", "sales.user"),
            ],
        )
        .await;
    let loc = location(&allowed);
    assert!(loc.starts_with(&format!("{REDIRECT}?code=oac_")), "{loc}");
    assert_eq!(param(&loc, "state"), "xyz");
    assert_eq!(param(&loc, "iss"), BASE);

    // The code buys a token; PKCE decides.
    let t = exchange(r, &c, &param(&loc, "code")).await;
    assert_eq!(t.status, 200, "{}", t.text());
    assert_eq!(t.header("cache-control"), "no-store");
    let t = t.json();
    assert_eq!(t["token_type"], "Bearer");
    assert_eq!(t["scope"], "sales.user");
    let access = t["access_token"].as_str().unwrap();

    // The token works on /mcp and sees fewer tools than eve herself.
    let agent_tools = tool_names(r, access).await;
    let eve_session = crate::mfa_tests::token(r, "eve").await;
    let eve_tools = tool_names(r, &eve_session).await;
    assert!(
        !agent_tools.is_empty() && agent_tools.len() < eve_tools.len(),
        "{} vs {}",
        agent_tools.len(),
        eve_tools.len()
    );
    assert!(
        agent_tools.iter().all(|t| eve_tools.contains(t)),
        "an agent only narrows"
    );

    // It is not a REST token: audience-bound to /mcp.
    let rest = call(
        r,
        "POST",
        "/api/party/list",
        &[("authorization", &format!("Bearer {access}"))],
        Some(("application/json", "{}".into())),
    )
    .await;
    assert_eq!(rest.status, 401);
    let meta = call(
        r,
        "GET",
        "/api/meta",
        &[("authorization", &format!("Bearer {access}"))],
        None,
    )
    .await;
    assert_eq!(meta.status, 401);
    // ... and not valid for the same server under another name.
    let other = call(
        r,
        "POST",
        "/mcp",
        &[
            ("authorization", &format!("Bearer {access}")),
            ("host", "other.test"),
        ],
        Some((
            "application/json",
            json!({"jsonrpc":"2.0","id":1,"method":"ping"}).to_string(),
        )),
    )
    .await;
    assert_eq!(other.status, 401);
    assert!(other.header("www-authenticate").contains("invalid_token"));

    // The existing bearer tokens keep working, unchanged, on /api and /mcp.
    assert_eq!(
        call(
            r,
            "POST",
            "/api/party/list",
            &[("authorization", &format!("Bearer {eve_session}"))],
            Some(("application/json", "{}".into()))
        )
        .await
        .status,
        200
    );
    let scoped = call(
        r,
        "POST",
        "/api/auth/agent-token",
        &[("authorization", &format!("Bearer {eve_session}"))],
        Some((
            "application/json",
            json!({"scopes":["sales.user"]}).to_string(),
        )),
    )
    .await
    .json();
    let plain_agent = scoped["token"].as_str().unwrap();
    assert!(!tool_names(r, plain_agent).await.is_empty());
    assert_eq!(
        call(
            r,
            "POST",
            "/api/party/list",
            &[("authorization", &format!("Bearer {plain_agent}"))],
            Some(("application/json", "{}".into()))
        )
        .await
        .status,
        404,
        "a plain agent token is still a REST token (this command is outside its scope)"
    );

    // Eve sees the connection, and revoking it stops the token and the refresh token.
    let bearer = format!("Bearer {eve_session}");
    let list = call(
        r,
        "GET",
        "/api/auth/oauth/grants",
        &[("authorization", &bearer)],
        None,
    )
    .await;
    assert_eq!(list.status, 200);
    let grants = list.json()["grants"].clone();
    assert_eq!(grants.as_array().unwrap().len(), 1);
    assert_eq!(grants[0]["client_name"], "Claude");
    assert_eq!(grants[0]["scopes"], json!(["sales.user"]));
    assert!(
        !list.text().contains(access)
            && !list.text().contains(t["refresh_token"].as_str().unwrap())
    );
    // Somebody else sees nothing and cannot revoke it.
    let bob = crate::mfa_tests::token(r, "bob").await;
    let gid = grants[0]["id"].as_str().unwrap();
    let path = format!("/api/auth/oauth/grants/{gid}/revoke");
    let bob_bearer = format!("Bearer {bob}");
    assert_eq!(
        call(
            r,
            "GET",
            "/api/auth/oauth/grants",
            &[("authorization", &bob_bearer)],
            None
        )
        .await
        .json()["grants"],
        json!([])
    );
    assert_eq!(
        call(r, "POST", &path, &[("authorization", &bob_bearer)], None)
            .await
            .status,
        404
    );
    // An agent cannot manage them at all.
    let agent_bearer = format!("Bearer {plain_agent}");
    assert_eq!(
        call(
            r,
            "GET",
            "/api/auth/oauth/grants",
            &[("authorization", &agent_bearer)],
            None
        )
        .await
        .status,
        404
    );
    assert_eq!(
        call(r, "GET", "/api/auth/oauth/grants", &[], None)
            .await
            .status,
        401
    );
    let gone = call(r, "POST", &path, &[("authorization", &bearer)], None).await;
    assert_eq!(gone.status, 200);
    assert_eq!(
        mcp(
            r,
            &format!("Bearer {access}"),
            None,
            json!({"jsonrpc":"2.0","id":1,"method":"ping"})
        )
        .await
        .status,
        401
    );
    assert_eq!(
        refresh(r, &c, t["refresh_token"].as_str().unwrap())
            .await
            .status,
        400
    );
    assert_eq!(
        call(
            r,
            "GET",
            "/api/auth/oauth/grants",
            &[("authorization", &bearer)],
            None
        )
        .await
        .json()["grants"],
        json!([])
    );

    // The security log has it, without a secret in it.
    let root = crate::mfa_tests::token(r, "root").await;
    let events = call(
        r,
        "POST",
        "/api/security_event/list",
        &[("authorization", &format!("Bearer {root}"))],
        Some(("application/json", "{}".into())),
    )
    .await;
    let text = events.text();
    assert!(
        text.contains("oauth_authorize") && text.contains("oauth_revoked"),
        "{text}"
    );
    assert!(!text.contains(access) && !text.contains(t["refresh_token"].as_str().unwrap()));
}

#[tokio::test]
async fn bad_verifier_reused_code_and_wrong_redirect_and_resource() {
    let rig = rig(false);
    eve(&rig);
    let r = &rig.r;
    let c = client(r).await;
    let url = authorize_url(&c, REDIRECT, &[]);
    let loc = location(&sign_in_and_allow(r, "eve", &url, &["sales.user"], "30").await);
    let code = param(&loc, "code");

    // A wrong verifier.
    let e = token(
        r,
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("client_id", &c),
            ("redirect_uri", REDIRECT),
            ("code_verifier", &"x".repeat(60)),
        ],
    )
    .await;
    assert_eq!(
        (e.status, e.json()["error"].clone()),
        (400, json!("invalid_grant"))
    );
    // The code is burnt even for the right verifier.
    assert_eq!(exchange(r, &c, &code).await.status, 400);

    // Reuse of a good code revokes what it produced.
    let loc = location(&sign_in_and_allow(r, "eve", &url, &["sales.user"], "30").await);
    let code = param(&loc, "code");
    let ok = exchange(r, &c, &code).await;
    assert_eq!(ok.status, 200);
    let access = ok.json()["access_token"].as_str().unwrap().to_string();
    assert!(!tool_names(r, &access).await.is_empty());
    assert_eq!(exchange(r, &c, &code).await.status, 400);
    assert_eq!(
        mcp(
            r,
            &format!("Bearer {access}"),
            None,
            json!({"jsonrpc":"2.0","id":1,"method":"ping"})
        )
        .await
        .status,
        401
    );

    // Wrong redirect at the token endpoint.
    let loc = location(&sign_in_and_allow(r, "eve", &url, &["sales.user"], "30").await);
    let e = token(
        r,
        &[
            ("grant_type", "authorization_code"),
            ("code", &param(&loc, "code")),
            ("client_id", &c),
            ("redirect_uri", "https://claude.ai/other"),
            ("code_verifier", VERIFIER),
        ],
    )
    .await;
    assert_eq!(e.json()["error"], "invalid_grant");

    // Wrong resource at the token endpoint.
    let loc = location(&sign_in_and_allow(r, "eve", &url, &["sales.user"], "30").await);
    let e = token(
        r,
        &[
            ("grant_type", "authorization_code"),
            ("code", &param(&loc, "code")),
            ("client_id", &c),
            ("redirect_uri", REDIRECT),
            ("code_verifier", VERIFIER),
            ("resource", "https://evil.test/mcp"),
        ],
    )
    .await;
    assert_eq!(e.json()["error"], "invalid_target");

    // Wrong resource at the authorization endpoint: the error goes to the registered redirect.
    let bad = call(
        r,
        "GET",
        &authorize_url(&c, REDIRECT, &[("resource", "https://evil.test/mcp")]),
        &[],
        None,
    )
    .await;
    let loc = location(&bad);
    assert_eq!(param(&loc, "error"), "invalid_target");
    assert_eq!(param(&loc, "state"), "xyz");

    // A redirect URI that was not registered gets a page, never a redirect.
    for evil in [
        "https://evil.test/cb",
        "https://claude.ai/api/mcp/auth_callback/",
        "http://claude.ai/api/mcp/auth_callback",
    ] {
        let bad = call(r, "GET", &authorize_url(&c, evil, &[]), &[], None).await;
        assert_eq!(bad.status, 400, "{evil}");
        assert_eq!(bad.header("location"), "", "{evil}");
        assert!(bad.text().contains("not one the application registered"));
    }
    // An unknown client likewise.
    let bad = call(
        r,
        "GET",
        &authorize_url("oc_nope", REDIRECT, &[]),
        &[],
        None,
    )
    .await;
    assert_eq!((bad.status, bad.header("location")), (400, ""));
    // No PKCE, or plain PKCE.
    let bad = call(
        r,
        "GET",
        &authorize_url(&c, REDIRECT, &[("code_challenge_method", "plain")]),
        &[],
        None,
    )
    .await;
    assert_eq!(param(&location(&bad), "error"), "invalid_request");
    let no_challenge = format!(
        "/oauth/authorize?response_type=code&client_id={c}&redirect_uri={}",
        enc(REDIRECT)
    );
    let bad = call(r, "GET", &no_challenge, &[], None).await;
    assert_eq!(param(&location(&bad), "error"), "invalid_request");
}

#[tokio::test]
async fn refresh_rotation_reuse_detection_and_no_widening() {
    let rig = rig(false);
    eve(&rig);
    let r = &rig.r;
    let c = client(r).await;
    let url = authorize_url(&c, REDIRECT, &[]);
    let loc =
        location(&sign_in_and_allow(r, "eve", &url, &["sales.user", "base.user"], "30").await);
    let t1 = exchange(r, &c, &param(&loc, "code")).await.json();
    let (a1, r1) = (
        t1["access_token"].as_str().unwrap(),
        t1["refresh_token"].as_str().unwrap(),
    );

    // Asking for more than was granted is refused; asking for less works.
    let widen = token(
        r,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", r1),
            ("client_id", &c),
            ("scope", "sales.user crm.user"),
        ],
    )
    .await;
    assert_eq!(widen.json()["error"], "invalid_scope");

    let t2 = refresh(r, &c, r1).await;
    assert_eq!(t2.status, 200, "{}", t2.text());
    let t2 = t2.json();
    let (a2, r2) = (
        t2["access_token"].as_str().unwrap(),
        t2["refresh_token"].as_str().unwrap(),
    );
    assert_ne!(r1, r2);
    assert_eq!(
        mcp(
            r,
            &format!("Bearer {a1}"),
            None,
            json!({"jsonrpc":"2.0","id":1,"method":"ping"})
        )
        .await
        .status,
        401,
        "rotation ends the old access token"
    );
    assert!(!tool_names(r, a2).await.is_empty());

    // Another client cannot use it.
    let other = client(r).await;
    assert_eq!(refresh(r, &other, r2).await.status, 400);

    // Replaying the first refresh token ends the whole family.
    assert_eq!(refresh(r, &c, r1).await.status, 400);
    assert_eq!(
        mcp(
            r,
            &format!("Bearer {a2}"),
            None,
            json!({"jsonrpc":"2.0","id":1,"method":"ping"})
        )
        .await
        .status,
        401
    );
    assert_eq!(refresh(r, &c, r2).await.status, 400);
    let eve_session = crate::mfa_tests::token(r, "eve").await;
    let list = call(
        r,
        "GET",
        "/api/auth/oauth/grants",
        &[("authorization", &format!("Bearer {eve_session}"))],
        None,
    )
    .await;
    assert_eq!(list.json()["grants"], json!([]));
    let root = crate::mfa_tests::token(r, "root").await;
    let events = call(
        r,
        "POST",
        "/api/security_event/list",
        &[("authorization", &format!("Bearer {root}"))],
        Some(("application/json", "{}".into())),
    )
    .await;
    assert!(events.text().contains("oauth_refresh_reuse"));
}

#[tokio::test]
async fn expired_access_tokens_stop_and_refresh_brings_a_new_one() {
    let rig = rig(false);
    eve(&rig);
    let r = &rig.r;
    let c = client(r).await;
    let url = authorize_url(&c, REDIRECT, &[]);
    let loc = location(&sign_in_and_allow(r, "eve", &url, &["sales.user"], "30").await);
    let t = exchange(r, &c, &param(&loc, "code")).await.json();
    assert_eq!(t["expires_in"], 3600);
    let a = t["access_token"].as_str().unwrap();
    assert!(!tool_names(r, a).await.is_empty());
    rig.auth.set_clock_for_test(T0 + 3601);
    assert_eq!(
        mcp(
            r,
            &format!("Bearer {a}"),
            None,
            json!({"jsonrpc":"2.0","id":1,"method":"ping"})
        )
        .await
        .status,
        401
    );
    let t2 = refresh(r, &c, t["refresh_token"].as_str().unwrap()).await;
    assert_eq!(t2.status, 200);
    assert!(
        !tool_names(r, t2.json()["access_token"].as_str().unwrap())
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn the_consent_post_needs_the_browser_the_origin_and_the_csrf_token() {
    let rig = rig(false);
    eve(&rig);
    let r = &rig.r;
    let c = client(r).await;
    let url = authorize_url(&c, REDIRECT, &[]);
    let (_, b) = start(r, &url).await;
    b.post(
        r,
        "/oauth/authorize/login",
        &[("username", "eve"), ("password", PW)],
    )
    .await;
    let allow = [
        ("decision", "allow"),
        ("days", "7"),
        ("group", "sales.user"),
    ];
    let pairs = |csrf: &str| {
        let mut p = vec![
            ("flow", b.flow.as_str()),
            ("csrf", csrf.to_string().leak() as &str),
        ];
        p.extend_from_slice(&allow);
        p
    };

    // Another site's page: wrong Origin.
    let r1 = call(
        r,
        "POST",
        "/oauth/authorize/decision",
        &[("origin", "https://evil.test"), ("cookie", &b.cookie)],
        Some(form(&pairs(&b.csrf))),
    )
    .await;
    assert_eq!(r1.status, 403);
    // No Origin and no fetch metadata (not a browser form post).
    let r2 = call(
        r,
        "POST",
        "/oauth/authorize/decision",
        &[("cookie", &b.cookie)],
        Some(form(&pairs(&b.csrf))),
    )
    .await;
    assert_eq!(r2.status, 403);
    // Without the cookie of the browser that started it.
    let r3 = call(
        r,
        "POST",
        "/oauth/authorize/decision",
        &[("origin", BASE)],
        Some(form(&pairs(&b.csrf))),
    )
    .await;
    assert_eq!(r3.status, 403);
    let r3b = call(
        r,
        "POST",
        "/oauth/authorize/decision",
        &[("origin", BASE), ("cookie", "obop_oauth_xxxxxxxxxxxx=1")],
        Some(form(&pairs(&b.csrf))),
    )
    .await;
    assert_eq!(r3b.status, 403);
    // A wrong CSRF token.
    let r4 = call(
        r,
        "POST",
        "/oauth/authorize/decision",
        &b.headers(),
        Some(form(&pairs("0000"))),
    )
    .await;
    assert_eq!(r4.status, 403);
    // A JSON body is not a form post.
    let r5 = call(
        r,
        "POST",
        "/oauth/authorize/decision",
        &b.headers(),
        Some(("application/json", "{}".into())),
    )
    .await;
    assert_eq!(r5.status, 400);
    for resp in [&r1, &r2, &r3, &r4] {
        assert!(resp.header("location").is_empty());
        assert!(
            resp.header("content-security-policy")
                .contains("default-src 'none'")
        );
    }
    // None of those consumed the flow: the real answer still works, once.
    let ok = b.post(r, "/oauth/authorize/decision", &allow).await;
    assert_eq!(ok.status, 303);
    let again = b.post(r, "/oauth/authorize/decision", &allow).await;
    assert_eq!(again.status, 410);
}

#[tokio::test]
async fn consent_cannot_widen_deny_and_not_you() {
    let rig = rig(false);
    eve(&rig);
    let r = &rig.r;
    let c = client(r).await;
    let url = authorize_url(&c, REDIRECT, &[]);
    let (_, b) = start(r, &url).await;
    let not_signed = b
        .post(
            r,
            "/oauth/authorize/decision",
            &[("decision", "allow"), ("days", "7"), ("group", "base.user")],
        )
        .await;
    assert_eq!(not_signed.status, 403, "no consent before sign-in");
    b.post(
        r,
        "/oauth/authorize/login",
        &[("username", "eve"), ("password", PW)],
    )
    .await;
    // A group eve does not hold, in a forged post.
    let widen = b
        .post(
            r,
            "/oauth/authorize/decision",
            &[
                ("decision", "allow"),
                ("days", "7"),
                ("group", "base.user"),
                ("group", "invoicing.manager"),
            ],
        )
        .await;
    assert_eq!(widen.status, 200);
    assert!(widen.header("location").is_empty());
    assert!(widen.text().contains("only hand over groups you hold"));
    let none = b
        .post(
            r,
            "/oauth/authorize/decision",
            &[("decision", "allow"), ("days", "7")],
        )
        .await;
    assert!(none.text().contains("Choose at least one group"));
    let days = b
        .post(
            r,
            "/oauth/authorize/decision",
            &[
                ("decision", "allow"),
                ("days", "9999"),
                ("group", "base.user"),
            ],
        )
        .await;
    assert!(days.text().contains("how long"));
    // Not you: back to sign in as somebody else.
    let sw = b
        .post(r, "/oauth/authorize/decision", &[("decision", "switch")])
        .await;
    assert!(sw.text().contains("name=\"username\""));
    b.post(
        r,
        "/oauth/authorize/login",
        &[("username", "bob"), ("password", PW)],
    )
    .await;
    let deny = b
        .post(r, "/oauth/authorize/decision", &[("decision", "deny")])
        .await;
    let loc = location(&deny);
    assert!(loc.starts_with(REDIRECT));
    assert_eq!(
        (param(&loc, "error").as_str(), param(&loc, "state").as_str()),
        ("access_denied", "xyz")
    );
    assert!(!loc.contains("code="));
    // The flow ended with the answer.
    assert_eq!(
        b.post(r, "/oauth/authorize/decision", &[("decision", "deny")])
            .await
            .status,
        410
    );
}

#[tokio::test]
async fn the_consent_page_is_localised_and_escapes_what_the_client_sent() {
    let rig = rig(false);
    eve(&rig);
    let r = &rig.r;
    let reg = call(
        r,
        "POST",
        "/oauth/register",
        &[],
        Some((
            "application/json",
            json!({"client_name": "<img src=x onerror=alert(1)>", "redirect_uris": [REDIRECT]})
                .to_string(),
        )),
    )
    .await
    .json();
    let c = reg["client_id"].as_str().unwrap();
    let de = call(
        r,
        "GET",
        &authorize_url(c, REDIRECT, &[]),
        &[("accept-language", "de-DE,de;q=0.9,en;q=0.5")],
        None,
    )
    .await;
    let html = de.text();
    assert!(
        html.contains("<html lang=\"de\"")
            && html.contains("Benutzername")
            && html.contains("Passwort"),
        "{html}"
    );
    assert!(!html.contains("<img") && html.contains("&lt;img"));
    // The language sticks through the flow.
    let b = Browser {
        flow: hidden(&html, "flow"),
        csrf: hidden(&html, "csrf"),
        cookie: de
            .header("set-cookie")
            .split(';')
            .next()
            .unwrap()
            .to_string(),
    };
    let consent = b
        .post(
            r,
            "/oauth/authorize/login",
            &[("username", "eve"), ("password", PW)],
        )
        .await
        .text();
    assert!(
        consent.contains("lang=\"de\"")
            && consent.contains("Erlauben")
            && consent.contains("Ablehnen"),
        "{consent}"
    );
    // The query parameter wins over the header.
    let fr = call(
        r,
        "GET",
        &format!("{}&lang=fr", authorize_url(c, REDIRECT, &[])),
        &[("accept-language", "de")],
        None,
    )
    .await;
    assert!(fr.text().contains("Nom d&#39;utilisateur"));
}

#[tokio::test]
async fn a_second_factor_is_asked_for_in_the_flow() {
    let rig = rig(false);
    let (secret, _codes) = enrol(&rig, "ada").await;
    rig.auth.set_clock_for_test(T0 + 30);
    let r = &rig.r;
    let c = client(r).await;
    let url = authorize_url(&c, REDIRECT, &[]);
    let (_, b) = start(r, &url).await;
    let step2 = b
        .post(
            r,
            "/oauth/authorize/login",
            &[("username", "ada"), ("password", PW)],
        )
        .await;
    let html = step2.text();
    assert!(
        html.contains("name=\"mfa_token\"") && html.contains("one-time-code"),
        "{html}"
    );
    assert!(
        !html.contains("name=\"decision\""),
        "no consent before the code"
    );
    let tok = hidden(&html, "mfa_token");
    let wrong = b
        .post(
            r,
            "/oauth/authorize/mfa",
            &[("mfa_token", &tok), ("code", "000000")],
        )
        .await;
    assert!(wrong.text().contains("name=\"mfa_token\"") && wrong.text().contains("not right"));
    let good = b
        .post(
            r,
            "/oauth/authorize/mfa",
            &[("mfa_token", &tok), ("code", &code(&secret, T0 + 30))],
        )
        .await;
    assert!(good.text().contains("name=\"decision\""), "{}", good.text());
    let ok = b
        .post(
            r,
            "/oauth/authorize/decision",
            &[("decision", "allow"), ("days", "1"), ("group", "base.user")],
        )
        .await;
    assert_eq!(ok.status, 303);
}

#[tokio::test]
async fn an_account_that_must_enrol_first_cannot_connect_an_app() {
    let rig = rig(true);
    let r = &rig.r;
    let c = client(r).await;
    let url = authorize_url(&c, REDIRECT, &[]);
    let (_, b) = start(r, &url).await;
    let out = b
        .post(
            r,
            "/oauth/authorize/login",
            &[("username", "root"), ("password", PW)],
        )
        .await;
    assert_eq!(out.status, 200);
    assert!(
        out.text().contains("requires two-factor") && !out.text().contains("name=\"decision\"")
    );
}

#[tokio::test]
async fn a_flow_is_limited_to_a_few_password_guesses() {
    let rig = rig(false);
    eve(&rig);
    let r = &rig.r;
    let c = client(r).await;
    let (_, b) = start(r, &authorize_url(&c, REDIRECT, &[])).await;
    let mut last = 0;
    for _ in 0..10 {
        last = b
            .post(
                r,
                "/oauth/authorize/login",
                &[("username", "eve"), ("password", "wrong wrong wrong")],
            )
            .await
            .status;
        if last != 200 {
            break;
        }
    }
    assert_eq!(last, 410, "the flow is gone after repeated failures");
}

#[tokio::test]
async fn behind_a_tls_proxy_the_issuer_and_cookie_are_https() {
    use obop_http::mcp::McpConfig;
    let app = App::new();
    app.seed_demo();
    let auth = Arc::new(obop_auth::Auth::with_test_params(app.store.backend()));
    auth.create_user("eve", PW, &["sales.user".into()]).unwrap();
    let r = obop_http::router(
        obop_http::HttpState::with_mcp(
            app.dispatcher.clone(),
            auth,
            McpConfig {
                public_url: Some("https://acme.example.com".into()),
                ..McpConfig::default()
            },
        ),
        None,
    );
    let m = call(
        &r,
        "GET",
        "/.well-known/oauth-authorization-server",
        &[("host", "10.0.0.5:8080")],
        None,
    )
    .await
    .json();
    assert_eq!(m["issuer"], "https://acme.example.com");
    let reg = register(&r, json!([REDIRECT])).await.json();
    let c = reg["client_id"].as_str().unwrap();
    let url = format!(
        "/oauth/authorize?response_type=code&client_id={c}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&resource={}",
        enc(REDIRECT),
        s256(VERIFIER),
        enc("https://acme.example.com/mcp")
    );
    let page = call(&r, "GET", &url, &[], None).await;
    assert_eq!(page.status, 200);
    assert!(page.header("set-cookie").contains("Secure"));
    // The resource of the public URL is accepted, the internal one is not.
    let wrong = url.replace("acme.example.com", "10.0.0.5");
    let bad = call(&r, "GET", &wrong, &[], None).await;
    assert_eq!(param(&location(&bad), "error"), "invalid_target");
}

#[tokio::test]
async fn dev_auth_runs_no_authorization_server() {
    let app = App::new();
    app.seed_demo();
    let r = obop_http::router(
        obop_http::HttpState::new(app.dispatcher.clone(), Arc::new(DevAuth)),
        None,
    );
    for path in [
        "/.well-known/oauth-authorization-server",
        "/oauth/authorize",
    ] {
        assert_eq!(call(&r, "GET", path, &[], None).await.status, 404, "{path}");
    }
    assert_eq!(register(&r, json!([REDIRECT])).await.status, 404);
    let prm = call(
        &r,
        "GET",
        "/.well-known/oauth-protected-resource",
        &[],
        None,
    )
    .await
    .json();
    assert!(prm.get("authorization_servers").is_none());
}

#[tokio::test]
async fn offboarding_ends_connected_apps_for_good_and_enabling_does_not_revive_them() {
    // D69: the real flow, then the offboarding of the person who connected the app.
    let rig = rig(false);
    eve(&rig);
    let r = &rig.r;
    let c = client(r).await;
    let url = authorize_url(&c, REDIRECT, &[]);
    let loc = location(&sign_in_and_allow(r, "eve", &url, &["sales.user"], "30").await);
    let t = exchange(r, &c, &param(&loc, "code")).await.json();
    let (access, refresh_token) = (
        t["access_token"].as_str().unwrap().to_string(),
        t["refresh_token"].as_str().unwrap().to_string(),
    );
    assert!(!tool_names(r, &access).await.is_empty());
    let done = rig.auth.operator_offboard("eve", "left").unwrap();
    assert_eq!(done["revoked_apps"], 1);
    let ping = json!({"jsonrpc":"2.0","id":1,"method":"ping"});
    assert_eq!(
        mcp(r, &format!("Bearer {access}"), None, ping.clone())
            .await
            .status,
        401
    );
    assert_eq!(refresh(r, &c, &refresh_token).await.status, 400);
    // Enabled again: the person is back, the application is not.
    rig.auth.operator_set_disabled("eve", false).unwrap();
    assert_eq!(
        mcp(r, &format!("Bearer {access}"), None, ping).await.status,
        401
    );
    assert_ne!(refresh(r, &c, &refresh_token).await.status, 200);
    // A new connection, made on purpose, works.
    let loc = location(&sign_in_and_allow(r, "eve", &url, &["sales.user"], "30").await);
    let t = exchange(r, &c, &param(&loc, "code")).await.json();
    assert!(
        !tool_names(r, t["access_token"].as_str().unwrap())
            .await
            .is_empty()
    );
}
