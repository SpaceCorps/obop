//! D65 over HTTP: passkeys against the real router and the real `Auth`, with the software
//! authenticator of `obop-auth` (feature `test-support`) in the browser's place. The origin and the
//! relying party id come from the public URL or the request; every ceremony is checked there.

use crate::app::App;
use crate::mfa_tests::{PW, rig};
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use obop_auth::webauthn::soft::SoftAuthenticator;
use obop_auth::{Auth, AuthConfig};
use obop_http::mcp::McpConfig;
use obop_oauth::util::s256;
use serde_json::{Value as J, json};
use std::sync::Arc;
use tower::ServiceExt;

const ORIGIN: &str = "https://inst.test";

struct Resp {
    status: u16,
    headers: axum::http::HeaderMap,
    body: Vec<u8>,
}

impl Resp {
    fn json(&self) -> J {
        serde_json::from_slice(&self.body).unwrap_or(J::Null)
    }
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
    fn header(&self, n: &str) -> &str {
        self.headers
            .get(n)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    }
}

/// A request as a browser behind a TLS proxy sends it: host `inst.test`, forwarded protocol https.
async fn call(
    r: &axum::Router,
    method: &str,
    path: &str,
    token: Option<&str>,
    extra: &[(&str, &str)],
    content_type: &str,
    body: String,
) -> Resp {
    let mut req = Request::builder().method(method).uri(path);
    if !extra.iter().any(|(k, _)| *k == "host") {
        req = req.header("host", "inst.test");
    }
    if !extra.iter().any(|(k, _)| *k == "x-forwarded-proto") {
        req = req.header("x-forwarded-proto", "https");
    }
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    for (k, v) in extra {
        req = req.header(*k, *v);
    }
    let req = req
        .header("content-type", content_type)
        .body(Body::from(body))
        .unwrap();
    let resp = r.clone().oneshot(req).await.unwrap();
    let (status, headers) = (resp.status().as_u16(), resp.headers().clone());
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

async fn api(r: &axum::Router, method: &str, token: Option<&str>, path: &str, body: J) -> (u16, J) {
    let resp = call(
        r,
        method,
        path,
        token,
        &[],
        "application/json",
        body.to_string(),
    )
    .await;
    (resp.status, resp.json())
}

async fn post(r: &axum::Router, token: Option<&str>, path: &str, body: J) -> (u16, J) {
    api(r, "POST", token, path, body).await
}

async fn session(r: &axum::Router, user: &str) -> String {
    let (s, b) = post(
        r,
        None,
        "/api/auth/login",
        json!({"username": user, "password": PW}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    b["token"]
        .as_str()
        .unwrap_or_else(|| panic!("no session: {b}"))
        .to_string()
}

/// Register `auth` for the person behind `token` (their password, and a proof when a second
/// factor is already on).
async fn add(r: &axum::Router, token: &str, auth: &mut SoftAuthenticator, name: &str) -> (u16, J) {
    let (s, opts) = post(
        r,
        Some(token),
        "/api/auth/passkeys/register/options",
        json!({"password": PW}),
    )
    .await;
    if s != 200 {
        return (s, opts);
    }
    let cred = auth.create(&opts, ORIGIN);
    post(
        r,
        Some(token),
        "/api/auth/passkeys/register/finish",
        json!({"name": name, "credential": cred}),
    )
    .await
}

/// Prove possession of a passkey in the session, so the next re-authentication needs no code.
async fn prove(r: &axum::Router, token: &str, auth: &mut SoftAuthenticator) {
    let (s, opts) = post(
        r,
        Some(token),
        "/api/auth/passkeys/proof/options",
        json!({}),
    )
    .await;
    assert_eq!(s, 200, "{opts}");
    let (s, b) = post(
        r,
        Some(token),
        "/api/auth/passkeys/proof",
        json!({"credential": auth.get(&opts, ORIGIN)}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
}

async fn second_step(r: &axum::Router, user: &str, auth: &mut SoftAuthenticator) -> (u16, J) {
    let (s, b) = post(
        r,
        None,
        "/api/auth/login",
        json!({"username": user, "password": PW}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    let mt = b["mfa_token"].as_str().unwrap().to_string();
    let (s, opts) = post(
        r,
        None,
        "/api/auth/login/passkey/options",
        json!({"mfa_token": mt}),
    )
    .await;
    assert_eq!(s, 200, "{opts}");
    post(
        r,
        None,
        "/api/auth/login/passkey",
        json!({"mfa_token": mt, "credential": auth.get(&opts, ORIGIN)}),
    )
    .await
}

#[tokio::test]
async fn enrol_sign_in_rename_and_remove_over_rest() {
    let rig = rig(false);
    let r = &rig.r;
    let t = session(r, "ada").await;
    let (s, list) = api(r, "GET", Some(&t), "/api/auth/passkeys", J::Null).await;
    assert_eq!(s, 200, "{list}");
    assert_eq!(
        (list["available"].clone(), list["rp_id"].clone()),
        (json!(true), json!("inst.test"))
    );
    assert_eq!(list["passkeys"].as_array().unwrap().len(), 0);

    let mut key = SoftAuthenticator::es256();
    let (s, done) = add(r, &t, &mut key, "Work laptop").await;
    assert_eq!(s, 200, "{done}");
    assert_eq!(done["recovery_codes"].as_array().unwrap().len(), 10);
    assert_eq!(done["name"], "Work laptop");

    // The first step now asks for a second factor and says which.
    let (_, b) = post(
        r,
        None,
        "/api/auth/login",
        json!({"username": "ada", "password": PW}),
    )
    .await;
    assert_eq!(b["mfa_required"], json!(true));
    assert_eq!(b["methods"], json!(["passkey"]));
    assert!(b.get("token").is_none());

    // The passkey completes the sign-in.
    let (s, b) = second_step(r, "ada", &mut key).await;
    assert_eq!(s, 200, "{b}");
    let t2 = b["token"].as_str().unwrap().to_string();
    let (s, list) = api(r, "GET", Some(&t2), "/api/auth/passkeys", J::Null).await;
    assert_eq!(s, 200);
    let row = &list["passkeys"][0];
    assert_eq!(row["name"], "Work laptop");
    assert!(row["last_used_at"].is_string());
    let id = row["id"].as_str().unwrap().to_string();

    // Renaming and removing need the password and a second factor (here: a passkey proof).
    let (s, b) = post(
        r,
        Some(&t2),
        &format!("/api/auth/passkeys/{id}/rename"),
        json!({"name": "X", "password": PW}),
    )
    .await;
    assert_eq!(s, 400, "no second factor given: {b}");
    prove(r, &t2, &mut key).await;
    let (s, b) = post(
        r,
        Some(&t2),
        &format!("/api/auth/passkeys/{id}/rename"),
        json!({"name": "Renamed", "password": PW}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    let (_, b) = post(
        r,
        Some(&t2),
        &format!("/api/auth/passkeys/{id}/rename"),
        json!({"name": "X", "password": "wrong wrong wrong"}),
    )
    .await;
    assert_eq!(b["code"], "invalid_password");
    prove(r, &t2, &mut key).await;
    let (s, b) = post(
        r,
        Some(&t2),
        &format!("/api/auth/passkeys/{id}/remove"),
        json!({"password": PW}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    let (_, list) = api(r, "GET", Some(&t2), "/api/auth/passkeys", J::Null).await;
    assert_eq!(list["passkeys"].as_array().unwrap().len(), 0);
    // Password alone signs in again.
    let _ = session(r, "ada").await;
}

/// Sign in with a passkey alone: options without a pending sign-in, then the answer.
async fn usernameless(r: &axum::Router, key: &mut SoftAuthenticator) -> (u16, J) {
    let (s, opts) = post(r, None, "/api/auth/login/passkey/options", json!({})).await;
    assert_eq!(s, 200, "{opts}");
    assert_eq!(opts["publicKey"]["userVerification"], "required");
    post(
        r,
        None,
        "/api/auth/login/passkey",
        json!({"credential": key.get(&opts, ORIGIN)}),
    )
    .await
}

#[tokio::test]
async fn passwordless_sign_in_over_rest_needs_opt_in() {
    let rig = rig(false);
    let r = &rig.r;
    let t = session(r, "ada").await;
    let mut key = SoftAuthenticator::es256();
    add(r, &t, &mut key, "Phone").await;
    let (s, b) = usernameless(r, &mut key).await;
    assert_eq!(s, 401, "off by default: {b}");
    // Turn it on (proof, then the password) and sign in with nothing else.
    let t_ok = {
        let (_, b) = second_step(r, "ada", &mut key).await;
        b["token"].as_str().unwrap().to_string()
    };
    prove(r, &t_ok, &mut key).await;
    let (s, b) = post(
        r,
        Some(&t_ok),
        "/api/auth/passkeys/passwordless",
        json!({"enabled": true, "password": PW}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    let (s, b) = usernameless(r, &mut key).await;
    assert_eq!(s, 200, "{b}");
    let me = b["token"].as_str().unwrap();
    let (s, _) = api(r, "GET", Some(me), "/api/auth/passkeys", J::Null).await;
    assert_eq!(s, 200);
}

#[tokio::test]
async fn the_ceremony_site_comes_from_the_public_url_or_the_request_never_the_browser() {
    let app = App::new();
    app.seed_demo();
    let auth = Arc::new(Auth::with_test_config(
        app.store.backend(),
        AuthConfig::insecure_dev(),
    ));
    auth.create_user("ada", PW, &["base.user".to_string()])
        .unwrap();

    // A configured public URL wins over Host and forwarded headers.
    let state = obop_http::HttpState::with_mcp(
        app.dispatcher.clone(),
        auth.clone(),
        McpConfig {
            public_url: Some("https://pub.example".into()),
            ..McpConfig::default()
        },
    );
    state.set_trust_proxy(true);
    let r = obop_http::router(state, None);
    let t = session(&r, "ada").await;
    let resp = call(
        &r,
        "POST",
        "/api/auth/passkeys/register/options",
        Some(&t),
        &[("host", "evil.test"), ("x-forwarded-host", "evil.test")],
        "application/json",
        json!({"password": PW}).to_string(),
    )
    .await;
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(resp.json()["publicKey"]["rp"]["id"], "pub.example");
    assert_eq!(resp.header("cache-control"), "no-store");

    // Without a public URL: the forwarded host only behind a trusted proxy, else Host.
    for (trust, want) in [(true, "app.fwd.test"), (false, "inst.test")] {
        let state = obop_http::HttpState::new(app.dispatcher.clone(), auth.clone());
        state.set_trust_proxy(trust);
        let r = obop_http::router(state, None);
        let t = session(&r, "ada").await;
        let resp = call(
            &r,
            "POST",
            "/api/auth/passkeys/register/options",
            Some(&t),
            &[("x-forwarded-host", "app.fwd.test")],
            "application/json",
            json!({"password": PW}).to_string(),
        )
        .await;
        assert_eq!(resp.status, 200, "{}", resp.text());
        assert_eq!(
            resp.json()["publicKey"]["rp"]["id"],
            want,
            "trust_proxy={trust}"
        );
    }
}

#[tokio::test]
async fn a_passkey_does_not_work_at_another_address_or_origin() {
    let rig = rig(false);
    let r = &rig.r;
    let t = session(r, "ada").await;
    let mut key = SoftAuthenticator::es256();
    assert_eq!(add(r, &t, &mut key, "k").await.0, 200);
    let (_, b) = post(
        r,
        None,
        "/api/auth/login",
        json!({"username": "ada", "password": PW}),
    )
    .await;
    let mt = b["mfa_token"].as_str().unwrap().to_string();
    // The same instance reached under another name: refused, even with a genuine answer for it.
    let resp = call(
        r,
        "POST",
        "/api/auth/login/passkey/options",
        None,
        &[("host", "other.test")],
        "application/json",
        json!({"mfa_token": mt}).to_string(),
    )
    .await;
    assert_eq!(resp.status, 200);
    let answer = key.get(&resp.json(), "https://other.test");
    let resp = call(
        r,
        "POST",
        "/api/auth/login/passkey",
        None,
        &[("host", "other.test")],
        "application/json",
        json!({"mfa_token": mt, "credential": answer}).to_string(),
    )
    .await;
    assert_eq!(resp.status, 401, "{}", resp.text());
    assert!(resp.text().contains("different address"));
}

#[tokio::test]
async fn an_ip_address_or_plain_http_cannot_hold_passkeys() {
    let rig = rig(false);
    let r = &rig.r;
    let t = session(r, "ada").await;
    for (host, proto) in [("192.0.2.7", "https"), ("inst.test", "http")] {
        let resp = call(
            r,
            "POST",
            "/api/auth/passkeys/register/options",
            Some(&t),
            &[("host", host), ("x-forwarded-proto", proto)],
            "application/json",
            json!({"password": PW}).to_string(),
        )
        .await;
        assert_eq!(resp.status, 503, "{host} {proto}: {}", resp.text());
        let list = call(
            r,
            "GET",
            "/api/auth/passkeys",
            Some(&t),
            &[("host", host), ("x-forwarded-proto", proto)],
            "application/json",
            String::new(),
        )
        .await;
        assert_eq!(list.json()["available"], false);
        assert!(list.json()["unavailable_reason"].is_string());
    }
    // localhost over http is the development case.
    let resp = call(
        r,
        "POST",
        "/api/auth/passkeys/register/options",
        Some(&t),
        &[("host", "localhost:3000"), ("x-forwarded-proto", "http")],
        "application/json",
        json!({"password": PW}).to_string(),
    )
    .await;
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(resp.json()["publicKey"]["rp"]["id"], "localhost");
}

#[tokio::test]
async fn agents_and_strangers_have_no_passkey_routes() {
    let rig = rig(false);
    let r = &rig.r;
    let t = session(r, "ada").await;
    let (_, b) = post(
        r,
        Some(&t),
        "/api/auth/agent-token",
        json!({"scopes": ["base.user"], "label": "bot"}),
    )
    .await;
    let agent = b["token"].as_str().unwrap().to_string();
    for path in ["/api/auth/passkeys"] {
        assert_eq!(api(r, "GET", Some(&agent), path, J::Null).await.0, 404);
        assert_eq!(api(r, "GET", None, path, J::Null).await.0, 401);
    }
    assert_eq!(
        post(
            r,
            Some(&agent),
            "/api/auth/passkeys/register/options",
            json!({"password": PW})
        )
        .await
        .0,
        404
    );
    // A wrong password is refused before anything is issued.
    let (s, b) = post(
        r,
        Some(&t),
        "/api/auth/passkeys/register/options",
        json!({"password": "wrong wrong wrong"}),
    )
    .await;
    assert_eq!((s, b["code"].clone()), (401, json!("invalid_password")));
}

#[tokio::test]
async fn an_administrator_under_the_policy_enrols_with_a_passkey_and_cannot_drop_the_last_factor() {
    let rig = rig(true);
    let r = &rig.r;
    let t = session(r, "root").await;
    // Limited: commands are refused, enrolment is not.
    assert_eq!(post(r, Some(&t), "/api/party/list", json!({})).await.0, 403);
    let mut key = SoftAuthenticator::es256();
    let (s, b) = add(r, &t, &mut key, "Security key").await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(
        post(r, Some(&t), "/api/party/list", json!({})).await.0,
        200,
        "no longer limited"
    );
    // The next sign-in is two steps and the session is whole.
    let (s, b) = second_step(r, "root", &mut key).await;
    assert_eq!(s, 200, "{b}");
    let t2 = b["token"].as_str().unwrap().to_string();
    assert!(b.get("mfa_enrolment_required").is_none());
    // The last factor stays.
    let (_, list) = api(r, "GET", Some(&t2), "/api/auth/passkeys", J::Null).await;
    assert_eq!(list["policy_requires_second_factor"], true);
    let id = list["passkeys"][0]["id"].as_str().unwrap().to_string();
    prove(r, &t2, &mut key).await;
    let (s, b) = post(
        r,
        Some(&t2),
        &format!("/api/auth/passkeys/{id}/remove"),
        json!({"password": PW}),
    )
    .await;
    assert_eq!(s, 400, "{b}");
    assert!(
        b["error"]
            .as_str()
            .unwrap()
            .contains("requires a second factor")
    );
    assert_eq!(rig.auth.passkey_count("root"), 1);
}

#[tokio::test]
async fn the_administrators_reset_and_the_users_listing_know_about_passkeys() {
    let rig = rig(false);
    let r = &rig.r;
    let t = session(r, "ada").await;
    let mut key = SoftAuthenticator::es256();
    assert_eq!(add(r, &t, &mut key, "k").await.0, 200);
    let root = session(r, "root").await;
    let (s, users) = api(r, "GET", Some(&root), "/api/auth/users", J::Null).await;
    assert_eq!(s, 200);
    let ada = users["users"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["name"] == "ada")
        .unwrap();
    assert_eq!(
        (ada["mfa_enabled"].clone(), ada["passkeys"].clone()),
        (json!(true), json!(1))
    );
    let (s, b) = post(
        r,
        Some(&root),
        "/api/auth/mfa/reset",
        json!({"user": "ada"}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(rig.auth.passkey_count("ada"), 0);
    let _ = session(r, "ada").await;
    // The break-glass path does the same.
    let t = session(r, "bob").await;
    let mut key2 = SoftAuthenticator::es256();
    assert_eq!(add(r, &t, &mut key2, "k").await.0, 200);
    rig.auth.operator_reset_mfa("bob").unwrap();
    assert_eq!(rig.auth.passkey_count("bob"), 0);
    let _ = session(r, "bob").await;
}

// ---- the OAuth sign-in pages ------------------------------------------------------------------

const REDIRECT: &str = "https://claude.ai/api/mcp/auth_callback";
const VERIFIER: &str = "an-example-verifier-of-sufficient-length-0123456789";

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

fn form(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn hidden(html: &str, name: &str) -> String {
    let marker = format!("name=\"{name}\" value=\"");
    let i = html
        .find(&marker)
        .unwrap_or_else(|| panic!("no input {name} in {html}"));
    let rest = &html[i + marker.len()..];
    rest[..rest.find('"').unwrap()].to_string()
}

struct Page {
    flow: String,
    csrf: String,
    cookie: String,
    html: String,
    csp: String,
}

async fn start_flow(r: &axum::Router) -> Page {
    let reg = call(
        r,
        "POST",
        "/oauth/register",
        None,
        &[],
        "application/json",
        json!({"client_name": "Claude", "redirect_uris": [REDIRECT], "token_endpoint_auth_method": "none"}).to_string(),
    )
    .await;
    assert_eq!(reg.status, 201, "{}", reg.text());
    let client = reg.json()["client_id"].as_str().unwrap().to_string();
    let url = format!(
        "/oauth/authorize?response_type=code&client_id={}&redirect_uri={}&state=xyz&code_challenge={}&code_challenge_method=S256&resource={}",
        enc(&client),
        enc(REDIRECT),
        enc(&s256(VERIFIER)),
        enc("https://inst.test/mcp")
    );
    let resp = call(r, "GET", &url, None, &[], "text/plain", String::new()).await;
    assert_eq!(resp.status, 200, "{}", resp.text());
    let html = resp.text();
    Page {
        flow: hidden(&html, "flow"),
        csrf: hidden(&html, "csrf"),
        cookie: resp
            .header("set-cookie")
            .split(';')
            .next()
            .unwrap()
            .to_string(),
        csp: resp.header("content-security-policy").to_string(),
        html,
    }
}

impl Page {
    async fn post(&self, r: &axum::Router, path: &str, extra: &[(&str, &str)]) -> Resp {
        let mut pairs = vec![("flow", self.flow.as_str()), ("csrf", self.csrf.as_str())];
        pairs.extend_from_slice(extra);
        call(
            r,
            "POST",
            path,
            None,
            &[("origin", ORIGIN), ("cookie", &self.cookie)],
            "application/x-www-form-urlencoded",
            form(&pairs),
        )
        .await
    }
}

#[tokio::test]
async fn the_sign_in_page_carries_one_script_by_hash_and_everything_else_stays_strict() {
    let rig = rig(false);
    let page = start_flow(&rig.r).await;
    assert_eq!(page.html.matches("<script>").count(), 1);
    assert!(
        page.html.contains("id=\"pkbtn\"")
            && page.html.contains("action=\"/oauth/authorize/passkey\"")
    );
    assert!(page.csp.contains("default-src 'none'"));
    assert!(page.csp.contains("script-src 'sha256-") && page.csp.contains("connect-src 'self'"));
    assert!(!page.csp.contains("unsafe") && !page.csp.contains("nonce") && !page.csp.contains("*"));
    assert!(
        page.csp.contains("form-action 'self';") && page.csp.contains("frame-ancestors 'none'")
    );
}

#[tokio::test]
async fn the_consent_page_after_a_passkey_second_step_and_after_a_passwordless_sign_in() {
    let rig = rig(false);
    let r = &rig.r;
    let t = session(r, "ada").await;
    let mut key = SoftAuthenticator::es256();
    assert_eq!(add(r, &t, &mut key, "k").await.0, 200);

    // Password, then the passkey as the second factor.
    let page = start_flow(r).await;
    let mfa = page
        .post(
            r,
            "/oauth/authorize/login",
            &[("username", "ada"), ("password", PW)],
        )
        .await;
    assert_eq!(mfa.status, 200, "{}", mfa.text());
    let html = mfa.text();
    assert!(
        html.contains("id=\"pk\"") && html.contains("name=\"mfa_token\""),
        "{html}"
    );
    assert!(mfa.header("content-security-policy").contains("script-src"));
    let mt = hidden(&html, "mfa_token");
    let opts = page
        .post(r, "/oauth/authorize/passkey/options", &[("mfa_token", &mt)])
        .await;
    assert_eq!(opts.status, 200, "{}", opts.text());
    let answer = key.get(&opts.json(), ORIGIN).to_string();
    let consent = page
        .post(
            r,
            "/oauth/authorize/passkey",
            &[("mfa_token", &mt), ("credential", &answer)],
        )
        .await;
    assert_eq!(consent.status, 200, "{}", consent.text());
    assert!(
        consent.text().contains("name=\"decision\""),
        "{}",
        consent.text()
    );
    // The sign-in is spent: the same answer again finds nothing.
    let again = page
        .post(
            r,
            "/oauth/authorize/passkey",
            &[("mfa_token", &mt), ("credential", &answer)],
        )
        .await;
    assert!(!again.text().contains("name=\"decision\""));

    // Passwordless needs the opt-in first.
    let page = start_flow(r).await;
    let opts = page.post(r, "/oauth/authorize/passkey/options", &[]).await;
    assert_eq!(opts.status, 200, "{}", opts.text());
    let answer = key.get(&opts.json(), ORIGIN).to_string();
    let refused = page
        .post(r, "/oauth/authorize/passkey", &[("credential", &answer)])
        .await;
    assert_eq!(refused.status, 200);
    assert!(
        refused.text().contains("id=\"pkbtn\""),
        "back on the sign-in page"
    );
    assert!(
        refused.text().contains("class=\"error\""),
        "{}",
        refused.text()
    );

    let t2 = {
        let (_, b) = second_step(r, "ada", &mut key).await;
        b["token"].as_str().unwrap().to_string()
    };
    prove(r, &t2, &mut key).await;
    let (s, b) = post(
        r,
        Some(&t2),
        "/api/auth/passkeys/passwordless",
        json!({"enabled": true, "password": PW}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    let page = start_flow(r).await;
    let opts = page.post(r, "/oauth/authorize/passkey/options", &[]).await;
    let answer = key.get(&opts.json(), ORIGIN).to_string();
    let consent = page
        .post(r, "/oauth/authorize/passkey", &[("credential", &answer)])
        .await;
    assert!(
        consent.text().contains("name=\"decision\""),
        "{}",
        consent.text()
    );
    assert!(consent.text().contains("ada"));
}

#[tokio::test]
async fn the_page_endpoints_refuse_a_foreign_post_a_bad_csrf_and_garbage() {
    let rig = rig(false);
    let r = &rig.r;
    let t = session(r, "ada").await;
    let mut key = SoftAuthenticator::es256();
    assert_eq!(add(r, &t, &mut key, "k").await.0, 200);
    let page = start_flow(r).await;
    // Another origin.
    let resp = call(
        r,
        "POST",
        "/oauth/authorize/passkey/options",
        None,
        &[("origin", "https://evil.example"), ("cookie", &page.cookie)],
        "application/x-www-form-urlencoded",
        form(&[("flow", &page.flow), ("csrf", &page.csrf)]),
    )
    .await;
    assert_eq!(resp.status, 403);
    // A wrong CSRF token.
    let resp = call(
        r,
        "POST",
        "/oauth/authorize/passkey/options",
        None,
        &[("origin", ORIGIN), ("cookie", &page.cookie)],
        "application/x-www-form-urlencoded",
        form(&[("flow", &page.flow), ("csrf", "nope")]),
    )
    .await;
    assert_eq!(resp.status, 403);
    // No cookie: not the browser that started the flow.
    let resp = call(
        r,
        "POST",
        "/oauth/authorize/passkey/options",
        None,
        &[("origin", ORIGIN)],
        "application/x-www-form-urlencoded",
        form(&[("flow", &page.flow), ("csrf", &page.csrf)]),
    )
    .await;
    assert_eq!(resp.status, 403);
    // Garbage as the credential is a refusal on the sign-in page, not an error.
    let resp = page
        .post(
            r,
            "/oauth/authorize/passkey",
            &[("credential", "{not json")],
        )
        .await;
    assert_eq!(resp.status, 200);
    assert!(resp.text().contains("class=\"error\""));
}
