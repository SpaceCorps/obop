//! Sign in with Microsoft Entra over HTTP (D91): the whole authorization code flow against a
//! stand-in for the provider on loopback (discovery, key set, token endpoint, signed id tokens),
//! with the real router, the real identity provider of `obop-auth` and the hardened egress client.
//! The stand-in is not Entra and proves nothing about Entra: it speaks the protocol as D91 reads
//! it. Named acceptance tests: `flow_*`, `failures_*`, `admin_*`.

use crate::app::App;
use crate::mfa_tests::{PW, Rig, T0, enrol, first_step, post, send};
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use obop_auth::oidc::soft::{Entra, StandIn};
use obop_auth::{Auth, AuthConfig};
use obop_commands::Authenticator;
use obop_connectors::{Config as ConnectorConfig, Runtime, SecretBox};
use serde_json::{Value as J, json};
use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

const TENANT: &str = "11111111-2222-3333-4444-555555555555";
const CLIENT: &str = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
const CLIENT_SECRET: &str = "http-test-client-secret-7c1d";
const ADA_OID: &str = "0f0f0f0f-1111-2222-3333-444444444444";
const BOB_OID: &str = "0b0b0b0b-1111-2222-3333-444444444444";
const STRANGER_OID: &str = "0c0c0c0c-1111-2222-3333-444444444444";
const BOSS_OID: &str = "0d0d0d0d-1111-2222-3333-444444444444";
const ACCESS_TOKEN: &str = "AT-the-provider-sends-this-and-it-must-not-be-kept";
const HOST: &str = "obop.test";
const CALLBACK: &str = "http://obop.test/api/auth/oidc/callback";
const CALLBACK_HTTPS: &str = "https://obop.test/api/auth/oidc/callback";

// ---- the stand-in provider ---------------------------------------------------------------------

type Edit = Arc<dyn Fn(&mut J) + Send + Sync>;

struct Issued {
    challenge: String,
    nonce: String,
    oid: String,
    redirect_uri: String,
    /// What the authorization request asked for.
    scope: String,
}

struct IdpState {
    issued: HashMap<String, Issued>,
    n: u32,
    /// Changes to the claims of the id tokens it signs next.
    edit: Option<Edit>,
    /// Changes to its discovery document.
    discovery: Option<Edit>,
    /// The `kid` the tokens name, and the key names the key set publishes.
    kid: String,
    published: Vec<String>,
    /// The status the token endpoint answers (200 unless a test says otherwise).
    token_status: u16,
    /// `METHOD /path` of every request that reached it, and the form of the token requests.
    requests: Vec<String>,
    forms: Vec<HashMap<String, String>>,
    /// Like Entra: the app registration has the optional claims `amr` and `auth_time` added to
    /// the ID token (without them the token has neither).
    optional_amr: bool,
    optional_auth_time: bool,
    /// Like Entra: the permissions for `profile` are consented to. Without consent the scope that
    /// is granted is `openid` only, and the token has no `oid` and no `preferred_username`.
    consent: bool,
    /// The `scope` of every authorization request that came to the authorization endpoint.
    scopes: Vec<String>,
    /// How long the discovery document takes to answer.
    discovery_delay_ms: u64,
    /// The status the key set answers (200 unless a test says otherwise).
    jwks_status: u16,
}

struct Idp {
    port: u16,
    st: Arc<Mutex<IdpState>>,
    stop: Arc<AtomicBool>,
}

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap_or(b'?'));
                i += 2;
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn pairs(q: &str) -> HashMap<String, String> {
    q.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (pct_decode(k), pct_decode(v))
        })
        .collect()
}

impl Idp {
    fn start() -> Idp {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let st = Arc::new(Mutex::new(IdpState {
            issued: HashMap::new(),
            n: 0,
            edit: None,
            discovery: None,
            kid: obop_auth::oidc::soft::KID.into(),
            published: vec![obop_auth::oidc::soft::KID.into()],
            token_status: 200,
            requests: Vec::new(),
            forms: Vec::new(),
            optional_amr: true,
            optional_auth_time: true,
            consent: true,
            scopes: Vec::new(),
            discovery_delay_ms: 0,
            jwks_status: 200,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let (state, halt) = (st.clone(), stop.clone());
        std::thread::spawn(move || {
            for s in l.incoming() {
                if halt.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut s) = s else { break };
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let n = s.read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf).to_string();
                    if let Some((head, body)) = text.split_once("\r\n\r\n") {
                        let want = head
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                            })
                            .unwrap_or(0);
                        if body.len() >= want {
                            break;
                        }
                    }
                }
                let req = String::from_utf8_lossy(&buf).to_string();
                let line = req.lines().next().unwrap_or("");
                let mut it = line.split(' ');
                let (method, path) = (it.next().unwrap_or(""), it.next().unwrap_or(""));
                let body = req.split("\r\n\r\n").nth(1).unwrap_or("");
                let (status, out) = Idp::answer(&state, port, method, path, body);
                let reply = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{out}",
                    out.len()
                );
                let _ = s.write_all(reply.as_bytes());
            }
        });
        Idp { port, st, stop }
    }

    fn authority(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// The provider goes away: its port refuses connections from now on.
    fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    fn edit(&self, f: impl Fn(&mut J) + Send + Sync + 'static) {
        self.st.lock().unwrap().edit = Some(Arc::new(f));
    }

    fn discovery(&self, f: impl Fn(&mut J) + Send + Sync + 'static) {
        self.st.lock().unwrap().discovery = Some(Arc::new(f));
    }

    fn reset(&self) {
        let mut s = self.st.lock().unwrap();
        s.edit = None;
        s.discovery = None;
        s.token_status = 200;
        s.kid = obop_auth::oidc::soft::KID.into();
        s.published = vec![obop_auth::oidc::soft::KID.into()];
        s.optional_amr = true;
        s.optional_auth_time = true;
        s.consent = true;
        s.discovery_delay_ms = 0;
        s.jwks_status = 200;
    }

    /// A registration without the optional claims `amr` and `auth_time` (Entra's default).
    fn without_optional_claims(&self) {
        let mut s = self.st.lock().unwrap();
        s.optional_amr = false;
        s.optional_auth_time = false;
    }

    fn hits(&self, what: &str) -> usize {
        self.st
            .lock()
            .unwrap()
            .requests
            .iter()
            .filter(|r| r.contains(what))
            .count()
    }

    fn answer(
        st: &Mutex<IdpState>,
        port: u16,
        method: &str,
        path: &str,
        body: &str,
    ) -> (u16, String) {
        let authority = format!("http://127.0.0.1:{port}");
        let mut s = st.lock().unwrap();
        s.requests.push(format!("{method} {path}"));
        let stand = StandIn::new();
        match (method, path) {
            ("GET", p) if p == format!("/{TENANT}/v2.0/.well-known/openid-configuration") => {
                if s.discovery_delay_ms > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(s.discovery_delay_ms));
                }
                let mut d: J = serde_json::from_str(&stand.discovery(&authority, TENANT)).unwrap();
                if let Some(f) = &s.discovery {
                    f(&mut d);
                }
                (200, d.to_string())
            }
            ("GET", p) if p == format!("/{TENANT}/discovery/v2.0/keys") => {
                if s.jwks_status != 200 {
                    return (s.jwks_status, json!({"error": "unavailable"}).to_string());
                }
                let keys: Vec<J> = s.published.iter().map(|k| stand.jwk_for(k)).collect();
                (200, StandIn::jwks_of(&keys))
            }
            ("POST", p) if p == format!("/{TENANT}/oauth2/v2.0/token") => {
                let f = pairs(body);
                s.forms.push(f.clone());
                if s.token_status != 200 {
                    return (s.token_status, json!({"error": "invalid_grant", "error_description": "AADSTS70000 secret text"}).to_string());
                }
                let code = f.get("code").cloned().unwrap_or_default();
                let Some(i) = s.issued.remove(&code) else {
                    return (400, json!({"error": "invalid_grant"}).to_string());
                };
                let ok = f.get("grant_type").map(String::as_str) == Some("authorization_code")
                    && f.get("client_id").map(String::as_str) == Some(CLIENT)
                    && f.get("client_secret").map(String::as_str) == Some(CLIENT_SECRET)
                    && f.get("redirect_uri") == Some(&i.redirect_uri)
                    && f.get("code_verifier")
                        .is_some_and(|v| obop_connectors::oauth::challenge_of(v) == i.challenge);
                if !ok {
                    return (400, json!({"error": "invalid_client"}).to_string());
                }
                // What Entra grants: the scope asked for, less `profile` where nobody consented, and
                // the claims that go with it. Nothing here leans on the scope being `openid`.
                let granted: Vec<&str> = i
                    .scope
                    .split(' ')
                    .filter(|w| *w != "profile" || s.consent)
                    .collect();
                let granted = granted.join(" ");
                let entra = Entra {
                    scope: granted.clone(),
                    optional_amr: s.optional_amr,
                    optional_auth_time: s.optional_auth_time,
                };
                let mut claims =
                    StandIn::claims_for(&authority, TENANT, CLIENT, &i.nonce, &i.oid, T0, &entra);
                if let Some(e) = &s.edit {
                    e(&mut claims);
                }
                let token = stand.sign(
                    &json!({"alg": "RS256", "typ": "JWT", "kid": s.kid}),
                    &claims,
                );
                (
                    200,
                    json!({
                        "token_type": "Bearer", "scope": granted, "expires_in": 3599,
                        "access_token": ACCESS_TOKEN, "id_token": token,
                    })
                    .to_string(),
                )
            }
            _ => (404, json!({"error": "not found"}).to_string()),
        }
    }

    /// The person at the provider's page says yes, as `oid`: checks what the authorization
    /// request asked for and returns the address the provider sends the browser back to.
    fn authorize(&self, location: &str, oid: &str) -> String {
        let (base, q) = location.split_once('?').expect("a query");
        assert_eq!(
            base,
            format!("{}/{TENANT}/oauth2/v2.0/authorize", self.authority())
        );
        let p = pairs(q);
        assert_eq!(p["client_id"], CLIENT);
        assert_eq!(p["response_type"], "code");
        assert_eq!(p["response_mode"], "query");
        let asked: Vec<&str> = p["scope"].split(' ').collect();
        assert!(
            asked.contains(&"openid"),
            "an OpenID Connect request: {asked:?}"
        );
        assert!(
            !asked.contains(&"offline_access") && !p["scope"].contains("User.Read"),
            "no refresh token, no access to anything: {asked:?}"
        );
        assert_eq!(p["code_challenge_method"], "S256");
        assert!(
            p["redirect_uri"] == CALLBACK || p["redirect_uri"] == CALLBACK_HTTPS,
            "{}",
            p["redirect_uri"]
        );
        assert_eq!(p["state"].len(), 64);
        assert!(p["nonce"].len() >= 32 && p["code_challenge"].len() >= 43);
        assert!(!p.contains_key("login_hint") && !p.contains_key("prompt"));
        let mut s = self.st.lock().unwrap();
        s.n += 1;
        let code = format!("idp-code-{}", s.n);
        s.scopes.push(p["scope"].clone());
        s.issued.insert(
            code.clone(),
            Issued {
                challenge: p["code_challenge"].clone(),
                nonce: p["nonce"].clone(),
                oid: oid.into(),
                redirect_uri: p["redirect_uri"].clone(),
                scope: p["scope"].clone(),
            },
        );
        format!("/api/auth/oidc/callback?code={code}&state={}", p["state"])
    }
}

// ---- requests and cookies ---------------------------------------------------------------------

struct Raw {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl Raw {
    fn all(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .collect()
    }
    fn location(&self) -> String {
        assert_eq!(self.status, 303, "{}", self.body);
        self.all("location")[0].to_string()
    }
    fn json(&self) -> J {
        serde_json::from_str(&self.body).unwrap_or_else(|_| panic!("not JSON: {}", self.body))
    }
}

#[derive(Default, Clone)]
struct Jar(BTreeMap<String, String>);

impl Jar {
    fn absorb(&mut self, r: &Raw) {
        for c in r.all("set-cookie") {
            let first = c.split(';').next().unwrap_or("");
            let Some((k, v)) = first.split_once('=') else {
                continue;
            };
            if c.contains("Max-Age=0") {
                self.0.remove(k);
            } else {
                self.0.insert(k.to_string(), v.to_string());
            }
        }
    }
    fn header(&self) -> Option<String> {
        (!self.0.is_empty()).then(|| {
            self.0
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("; ")
        })
    }
}

struct World {
    app: App,
    auth: Arc<Auth>,
    router: axum::Router,
    idp: Idp,
    seen: Mutex<Vec<String>>,
}

async fn world(configure: bool) -> World {
    world_with(configure, AuthConfig::insecure_dev(), None).await
}

async fn world_with(configure: bool, config: AuthConfig, prefix: Option<&str>) -> World {
    let idp = Idp::start();
    let app = App::new();
    app.seed_demo();
    let auth = Arc::new(Auth::with_test_config(app.store.backend(), config));
    auth.set_clock_for_test(T0);
    auth.set_oidc_authority_for_test(&idp.authority());
    for (name, group) in [
        ("root1", "platform.admin"),
        ("root2", "platform.admin"),
        ("root3", "platform.admin"),
        ("boss", "platform.admin"),
        ("ada", "base.user"),
        ("bob", "base.user"),
    ] {
        auth.create_user(name, PW, &[group.to_string()]).unwrap();
    }
    app.register_accounts(auth.clone() as Arc<dyn Authenticator>);
    let rt = Runtime::new(
        app.store.backend(),
        app.dispatcher.clone(),
        SecretBox::for_tests(),
        ConnectorConfig {
            allow_loopback: true,
            public_url: None,
        },
    );
    let state = obop_http::HttpState::new(app.dispatcher.clone(), auth.clone());
    state.set_connectors(rt);
    if let Some(p) = prefix {
        state.set_sso_return_prefix(p);
    }
    let router = obop_http::router(state, None);
    let w = World {
        app,
        auth,
        router,
        idp,
        seen: Mutex::new(Vec::new()),
    };
    let rig = Rig {
        r: w.router.clone(),
        auth: w.auth.clone(),
    };
    // Three administrators with an authenticator app: the break-glass ones.
    for n in ["root1", "root2", "root3"] {
        enrol(&rig, n).await;
    }
    if configure {
        w.auth
            .operator_sso_configure(TENANT, CLIENT, Some(CLIENT_SECRET), Some(true))
            .unwrap();
        w.auth.operator_sso_bind("ada", ADA_OID).unwrap();
        w.auth.operator_sso_bind("bob", BOB_OID).unwrap();
    }
    w
}

impl World {
    async fn send(&self, method: &str, uri: &str, jar: &Jar, body: Option<&str>) -> Raw {
        self.send_h(method, uri, jar, body, &[]).await
    }

    async fn send_h(
        &self,
        method: &str,
        uri: &str,
        jar: &Jar,
        body: Option<&str>,
        extra: &[(&str, &str)],
    ) -> Raw {
        let mut req = Request::builder()
            .method(method)
            .uri(uri)
            .header("host", HOST)
            .header("content-type", "application/json");
        if let Some(c) = jar.header() {
            req = req.header("cookie", c);
        }
        for (k, v) in extra {
            req = req.header(*k, *v);
        }
        let resp = self
            .router
            .clone()
            .oneshot(
                req.body(Body::from(body.unwrap_or("").to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status().as_u16();
        let headers = resp
            .headers()
            .iter()
            .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let body = String::from_utf8_lossy(&resp.into_body().collect().await.unwrap().to_bytes())
            .to_string();
        self.seen.lock().unwrap().push(body.clone());
        let raw = Raw {
            status,
            headers,
            body,
        };
        for (_, v) in &raw.headers {
            self.seen.lock().unwrap().push(v.clone());
        }
        raw
    }

    async fn start(&self, jar: &mut Jar) -> Raw {
        let r = self.send("GET", "/api/auth/oidc/start", jar, None).await;
        jar.absorb(&r);
        r
    }

    async fn callback(&self, jar: &mut Jar, uri: &str) -> Raw {
        let r = self.send("GET", uri, jar, None).await;
        jar.absorb(&r);
        r
    }

    /// Start, say yes at the provider as `oid`, and come back.
    async fn sign_in(&self, jar: &mut Jar, oid: &str) -> Raw {
        let s = self.start(jar).await;
        let uri = self.idp.authorize(&s.location(), oid);
        self.callback(jar, &uri).await
    }

    async fn exchange(&self, jar: &mut Jar, code: &str) -> Raw {
        let r = self
            .send(
                "POST",
                "/api/auth/oidc/exchange",
                jar,
                Some(&json!({"code": code}).to_string()),
            )
            .await;
        jar.absorb(&r);
        r
    }

    fn events(&self, kind: &str) -> Vec<String> {
        self.app
            .store
            .backend()
            .list("security_event")
            .unwrap()
            .into_iter()
            .filter(|(_, f)| f["kind"] == kind)
            .map(|(_, f)| f["detail"].as_str().unwrap_or("").to_string())
            .collect()
    }

    /// Nothing the person's browser, the log or the database holds contains `needle`.
    fn assert_nowhere(&self, needle: &str) {
        for body in self.seen.lock().unwrap().iter() {
            assert!(!body.contains(needle), "{needle} in a response: {body}");
        }
        self.assert_not_stored(needle);
    }

    /// The log and the database hold no `needle` (a value that is meant to reach the browser, such
    /// as the one-time code, is checked here only).
    fn assert_not_stored(&self, needle: &str) {
        let b = self.app.store.backend();
        for table in [
            "security_event",
            "auth_user",
            "auth_token",
            "auth_sso_provider",
            "auth_sso_link",
        ] {
            for (id, f) in b.list(table).unwrap() {
                let row = J::Object(f).to_string();
                assert!(
                    !row.contains(needle) && !id.contains(needle),
                    "{needle} stored in {table}/{id}"
                );
            }
        }
        // The short-lived values (flows, one-time codes, counters) as well.
        for (k, v) in b.kv_scan("").unwrap() {
            assert!(
                !k.contains(needle) && !v.contains(needle),
                "{needle} stored in the value {k}"
            );
        }
    }
}

fn code_of(r: &Raw) -> String {
    let l = r.location();
    l.strip_prefix("/#oidc=")
        .unwrap_or_else(|| panic!("not a code redirect: {l}"))
        .to_string()
}

fn failed(r: &Raw, word: &str) {
    assert_eq!(r.location(), format!("/#oidc_error={word}"));
    assert!(
        !r.all("set-cookie")
            .iter()
            .any(|c| c.starts_with("obop_sso_done=") && !c.contains("Max-Age=0")),
        "a failed sign-in must not leave a done cookie"
    );
}

/// Ada signs in once with the button, as the administrator asks before making the account
/// Microsoft-only, and is then made SSO-only, which the three break-glass administrators allow.
async fn ada_sso_only(w: &World) {
    let mut jar = Jar::default();
    let back = w.sign_in(&mut jar, ADA_OID).await;
    assert_eq!(w.exchange(&mut jar, &code_of(&back)).await.status, 200);
    w.auth.operator_sso_only("ada", true).unwrap();
}

// ---- the tests -----------------------------------------------------------------------------

#[tokio::test]
async fn flow_round_trip_ends_in_a_bearer_token_and_keeps_nothing_of_the_provider() {
    let w = world(true).await;
    ada_sso_only(&w).await;
    let first_forms = w.idp.st.lock().unwrap().forms.len();
    let none = Jar::default();
    // The button is offered.
    let s = w.send("GET", "/api/auth/sso", &none, None).await;
    assert_eq!((s.status, s.json()), (200, json!({"enabled": true})));

    let mut jar = Jar::default();
    let started = w.start(&mut jar).await;
    let location = started.location();
    assert!(location.starts_with(&format!(
        "{}/{TENANT}/oauth2/v2.0/authorize?",
        w.idp.authority()
    )));
    // The cookie of the flow: HttpOnly, Lax, scoped to the flow routes, and it is the verifier.
    let cookie = started.all("set-cookie")[0];
    assert!(cookie.starts_with("obop_sso_") && cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Lax") && cookie.contains("Path=/api/auth/oidc"));
    assert!(!cookie.contains("Secure"), "plain http in this test");
    let uri = w.idp.authorize(&location, ADA_OID);
    let back = w.callback(&mut jar, &uri).await;
    let code = code_of(&back);
    // The flow cookie is gone and the done cookie is set for this browser only.
    assert!(
        jar.0
            .keys()
            .all(|k| !k.starts_with("obop_sso_") || k == "obop_sso_done")
    );
    let done = back
        .all("set-cookie")
        .into_iter()
        .find(|c| c.starts_with("obop_sso_done="))
        .unwrap();
    assert!(done.contains("HttpOnly") && done.contains("Max-Age=60"));
    // The token request carried the secret, the redirect uri and the PKCE verifier.
    {
        let st = w.idp.st.lock().unwrap();
        let f = st.forms.last().unwrap();
        assert_eq!(f["client_secret"], CLIENT_SECRET);
        assert_eq!(f["redirect_uri"], CALLBACK);
        assert_eq!(f["grant_type"], "authorization_code");
        assert!(f["code_verifier"].len() >= 43);
    }
    let ex = w.exchange(&mut jar, &code).await;
    assert_eq!(ex.status, 200, "{}", ex.body);
    let token = ex.json()["token"].as_str().unwrap().to_string();
    // An SSO-only account whose provider reports a second factor gets the whole session.
    assert!(
        ex.json().get("mfa_enrolment_required").is_none()
            && ex.json().get("mfa_required").is_none()
    );
    assert!(!jar.0.contains_key("obop_sso_done"), "the cookie is spent");
    // The bearer token works as any other and is Ada's.
    let (s, meta) = send(&w.router, "GET", Some(&token), "/api/meta", json!({})).await;
    assert_eq!(s, 200, "{meta}");
    let (s, b) = send(&w.router, "GET", Some(&token), "/api/auth/mfa", json!({})).await;
    assert_eq!(s, 200, "{b}");
    // The code is spent.
    let again = w.exchange(&mut jar, &code).await;
    assert_eq!(again.status, 401);
    // The password of an SSO-only account is refused like a wrong password.
    let (s, b) = post(
        &w.router,
        None,
        "/api/auth/login",
        json!({"username": "ada", "password": PW}),
    )
    .await;
    let (s2, b2) = post(
        &w.router,
        None,
        "/api/auth/login",
        json!({"username": "ada", "password": "not the password"}),
    )
    .await;
    assert_eq!((s, b), (s2, b2));
    assert_eq!(s, 401);
    // The log has the sign-in and the refusal; nothing the provider sent is kept anywhere.
    // The first sign-in (before the account was Microsoft-only) and this one.
    assert_eq!(w.events("oidc_login").len(), 2);
    assert!(w.events("oidc_login")[1].contains("reports a second factor"));
    // A start is one line per address and ten minutes, not one per request.
    assert_eq!(w.events("oidc_start").len(), 1);
    assert!(!w.events("login_refused").is_empty());
    let id_token = {
        let st = w.idp.st.lock().unwrap();
        st.forms.len()
    };
    assert_eq!(id_token, first_forms + 1);
    // The profile claims the token carried (name, preferred_username) are read by nobody.
    w.assert_nowhere("Ada Example");
    w.assert_nowhere("ada@example.test");
    w.assert_nowhere(CLIENT_SECRET);
    w.assert_nowhere(ACCESS_TOKEN);
    w.assert_not_stored(&code); // only its hash is kept, and only for 60 seconds
    w.assert_nowhere("eyJ"); // no JWT anywhere
}

#[tokio::test]
async fn flow_a_person_with_an_obop_second_factor_still_gives_it() {
    // Ada is bound but not SSO-only and has an authenticator app: Microsoft's answer is the first
    // factor, the app is the second.
    let w = world(true).await;
    let rig = Rig {
        r: w.router.clone(),
        auth: w.auth.clone(),
    };
    let (secret, _) = enrol(&rig, "ada").await;
    let mut jar = Jar::default();
    let back = w.sign_in(&mut jar, ADA_OID).await;
    let ex = w.exchange(&mut jar, &code_of(&back)).await;
    assert_eq!(ex.status, 200, "{}", ex.body);
    let b = ex.json();
    assert_eq!(b["mfa_required"], true, "{b}");
    assert!(b.get("token").is_none());
    let t = b["mfa_token"].as_str().unwrap().to_string();
    w.auth.set_clock_for_test(T0 + 60);
    let code = obop_auth::totp::code_for_step(&secret, obop_auth::totp::step_at(T0 + 60));
    let (s, b) = post(
        &w.router,
        None,
        "/api/auth/login/mfa",
        json!({"mfa_token": t, "code": code}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    assert!(b["token"].is_string());
}

#[tokio::test]
async fn failures_are_one_redirect_and_the_reason_is_in_the_log_only() {
    let w = world(true).await;
    let expect_log = |w: &World, kind: &str, part: &str| {
        let lines = w.events(kind);
        assert!(
            lines.iter().any(|l| l.contains(part)),
            "no {kind} line with {part:?} in {lines:?}"
        );
    };

    // The browser that comes back is not the one that left: no cookie, and another flow's cookie.
    let mut jar = Jar::default();
    let s = w.start(&mut jar).await;
    let uri = w.idp.authorize(&s.location(), ADA_OID);
    failed(&w.callback(&mut Jar::default(), &uri).await, "failed");
    expect_log(&w, "oidc_refused", "not the one that began");
    // The flow was spent by that try: the right browser cannot use it afterwards.
    failed(&w.callback(&mut jar.clone(), &uri).await, "failed");
    expect_log(&w, "oidc_refused", "unknown, already used or expired");
    let mut other = Jar::default();
    let s2 = w.start(&mut other).await;
    let uri2 = w.idp.authorize(&s2.location(), ADA_OID);
    let mut wrong = Jar::default();
    let name = jar
        .0
        .keys()
        .find(|k| k.starts_with("obop_sso_"))
        .unwrap()
        .clone();
    wrong
        .0
        .insert(name.clone(), other.0.values().next().unwrap().clone());
    // (The cookie name of the second flow differs, so this is the missing-cookie case too.)
    failed(&w.callback(&mut wrong, &uri2).await, "failed");

    // State: malformed, unknown.
    failed(
        &w.callback(
            &mut Jar::default(),
            "/api/auth/oidc/callback?code=x&state=zz",
        )
        .await,
        "failed",
    );
    let unknown = format!("/api/auth/oidc/callback?code=x&state={}", "ab".repeat(32));
    failed(&w.callback(&mut Jar::default(), &unknown).await, "failed");

    // The person said no, or the provider failed; what it wrote is not repeated.
    let mut jar = Jar::default();
    let s = w.start(&mut jar).await;
    let state = pairs(s.location().split_once('?').unwrap().1)["state"].clone();
    let r = w
        .callback(
            &mut jar,
            &format!(
                "/api/auth/oidc/callback?error=access_denied&error_description=%3Cscript%3Ealert(1)%3C%2Fscript%3E&state={state}"
            ),
        )
        .await;
    failed(&r, "cancelled");
    expect_log(&w, "oidc_cancelled", "access_denied");
    assert!(
        !w.events("oidc_cancelled")
            .iter()
            .any(|l| l.contains("script")),
        "the provider's description is not logged"
    );
    let mut jar = Jar::default();
    let s = w.start(&mut jar).await;
    let state = pairs(s.location().split_once('?').unwrap().1)["state"].clone();
    failed(
        &w.callback(
            &mut jar,
            &format!("/api/auth/oidc/callback?error=server_error&state={state}"),
        )
        .await,
        "failed",
    );
    // Only the person's own "no" is a cancellation; the provider's error is an error.
    expect_log(&w, "oidc_error", "server_error");
    assert_eq!(w.events("oidc_cancelled").len(), 1);
    let mut jar = Jar::default();
    let s = w.start(&mut jar).await;
    let state = pairs(s.location().split_once('?').unwrap().1)["state"].clone();
    failed(
        &w.callback(&mut jar, &format!("/api/auth/oidc/callback?state={state}"))
            .await,
        "failed",
    );
    expect_log(&w, "oidc_refused", "no code");

    // A good token for a person nobody is bound to: the same answer as every other failure.
    failed(
        &w.sign_in(&mut Jar::default(), STRANGER_OID).await,
        "failed",
    );
    expect_log(&w, "oidc_refused", "no account is bound");
    // A bound account that is disabled.
    w.auth.operator_set_disabled("bob", true).unwrap();
    failed(&w.sign_in(&mut Jar::default(), BOB_OID).await, "failed");
    expect_log(&w, "oidc_refused", "disabled");
    w.auth.operator_set_disabled("bob", false).unwrap();

    // Tokens that are wrong in one way each.
    let idp_authority = w.idp.authority();
    let cases: Vec<(&str, Edit, &str)> = vec![
        (
            "nonce",
            Arc::new(|c| c["nonce"] = json!("replayed-nonce")),
            "nonce",
        ),
        (
            "audience",
            Arc::new(|c| c["aud"] = json!("99999999-0000-0000-0000-000000000000")),
            "audience",
        ),
        (
            // The issuer is the stand-in's own host with another tenant in it: only the tenant
            // is wrong.
            "issuer of another tenant",
            Arc::new(move |c| {
                c["iss"] = json!(format!(
                    "{idp_authority}/99999999-2222-3333-4444-555555555555/v2.0"
                ));
            }),
            "issuer",
        ),
        (
            "tid of another tenant",
            Arc::new(|c| c["tid"] = json!("99999999-2222-3333-4444-555555555555")),
            "tenant",
        ),
        (
            "expiry",
            Arc::new(|c| c["exp"] = json!(T0 - 3600)),
            "expired",
        ),
        (
            "no oid",
            Arc::new(|c| {
                c.as_object_mut().unwrap().remove("oid");
            }),
            "object id",
        ),
    ];
    for (what, edit, log) in cases {
        w.idp.edit(move |c| edit(c));
        failed(&w.sign_in(&mut Jar::default(), ADA_OID).await, "failed");
        expect_log(&w, "oidc_refused", log);
        w.idp.reset();
        let _ = what;
    }
    // A key the provider does not publish (not even after the key set is fetched again).
    {
        let mut st = w.idp.st.lock().unwrap();
        st.kid = "never-published".into();
    }
    failed(&w.sign_in(&mut Jar::default(), ADA_OID).await, "failed");
    expect_log(&w, "oidc_refused", "signing key is not known");
    w.idp.reset();
    // The token endpoint refuses the code; its text is not repeated.
    w.idp.st.lock().unwrap().token_status = 400;
    failed(&w.sign_in(&mut Jar::default(), ADA_OID).await, "failed");
    expect_log(&w, "oidc_refused", "token endpoint answered 400");
    assert!(
        !w.events("oidc_refused")
            .iter()
            .any(|l| l.contains("AADSTS"))
    );
    w.idp.reset();

    // After all that, a good sign-in still works, and nothing leaked.
    let mut jar = Jar::default();
    let back = w.sign_in(&mut jar, ADA_OID).await;
    let ex = w.exchange(&mut jar, &code_of(&back)).await;
    assert_eq!(ex.status, 200, "{}", ex.body);
    // Every refusal above left its own line (they differ in words, so none was folded into
    // another by the throttle).
    assert!(
        w.events("oidc_refused").len() >= 10,
        "{:?}",
        w.events("oidc_refused")
    );
    w.assert_nowhere(CLIENT_SECRET);
    w.assert_nowhere("AADSTS");
    w.assert_nowhere(ACCESS_TOKEN);
}

#[tokio::test]
async fn failures_an_unreachable_provider_is_unavailable_not_failed() {
    let w = world(true).await;
    // One good sign-in fills the caches; then the provider goes away.
    let mut jar = Jar::default();
    let uri = {
        let s = w.start(&mut jar).await;
        w.idp.authorize(&s.location(), ADA_OID)
    };
    w.idp.stop();
    failed(&w.callback(&mut jar, &uri).await, "unavailable");
    assert!(
        w.events("oidc_error")
            .iter()
            .any(|l| l.contains("token endpoint could not be reached"))
    );
    // A new start with no discovery cached would fail the same way once the cache is empty; the
    // cached one is still good for a while.
    let s = w.start(&mut Jar::default()).await;
    assert_eq!(s.status, 303);
}

#[tokio::test]
async fn flow_a_rotated_key_is_picked_up_and_a_stream_of_unknown_keys_is_not_followed() {
    let w = world(true).await;
    // First sign-in: the key set is fetched and cached.
    let mut jar = Jar::default();
    let back = w.sign_in(&mut jar, ADA_OID).await;
    assert!(back.location().starts_with("/#oidc="));
    assert_eq!(w.idp.hits("/discovery/v2.0/keys"), 1);
    // A second one uses the cache.
    let back = w.sign_in(&mut Jar::default(), ADA_OID).await;
    assert!(back.location().starts_with("/#oidc="));
    assert_eq!(w.idp.hits("/discovery/v2.0/keys"), 1, "cached");
    // The provider rotates: the new key is published and signs.
    {
        let mut st = w.idp.st.lock().unwrap();
        st.kid = "rotated-1".into();
        st.published = vec!["rotated-1".into()];
    }
    let back = w.sign_in(&mut Jar::default(), ADA_OID).await;
    assert!(
        back.location().starts_with("/#oidc="),
        "{}",
        back.location()
    );
    assert_eq!(w.idp.hits("/discovery/v2.0/keys"), 2, "fetched once more");
    // A key that never appears: the key set is fetched again at most a few times per minute.
    w.idp.st.lock().unwrap().kid = "never".into();
    // The budget is 6 forced fetches a minute, one of which the rotation above used: five more
    // sign-ins look again and are refused for the unknown key, then the key set is no longer
    // fetched and the answer is "unavailable" (try again later), not "failed".
    let mut words = Vec::new();
    for _ in 0..10 {
        let r = w.sign_in(&mut Jar::default(), ADA_OID).await;
        words.push(r.location().rsplit('=').next().unwrap().to_string());
    }
    assert_eq!(
        words,
        [
            "failed",
            "failed",
            "failed",
            "failed",
            "failed",
            "unavailable",
            "unavailable",
            "unavailable",
            "unavailable",
            "unavailable"
        ]
    );
    assert_eq!(
        w.idp.hits("/discovery/v2.0/keys"),
        2 + 5,
        "the key set is fetched at most six times a minute after the first fetch"
    );
    assert!(
        w.events("oidc_error")
            .iter()
            .any(|l| l.contains("fetched too often"))
    );
}

#[tokio::test]
async fn flow_the_one_time_code_belongs_to_the_browser_that_signed_in() {
    let w = world(true).await;
    let mut mine = Jar::default();
    let back = w.sign_in(&mut mine, ADA_OID).await;
    let code = code_of(&back);
    // Someone else's browser (login CSRF): no done cookie, or the wrong one.
    let theirs = w.exchange(&mut Jar::default(), &code).await;
    assert_eq!(theirs.status, 401);
    assert_eq!(theirs.json()["code"], "sso_failed");
    // The code is spent by that try, so the right browser cannot use it afterwards.
    assert_eq!(w.exchange(&mut mine.clone(), &code).await.status, 401);
    assert!(
        w.events("oidc_refused")
            .iter()
            .any(|l| l.contains("another browser"))
    );
    // A wrong done cookie.
    let back = w.sign_in(&mut Jar::default(), ADA_OID).await;
    let code = code_of(&back);
    let mut forged = Jar::default();
    forged.0.insert("obop_sso_done".into(), "0".repeat(64));
    assert_eq!(w.exchange(&mut forged, &code).await.status, 401);
    // Junk and big bodies.
    for body in ["", "not json", "{}", r#"{"code": 5}"#, r#"{"code": ""}"#] {
        let r = w
            .send(
                "POST",
                "/api/auth/oidc/exchange",
                &Jar::default(),
                Some(body),
            )
            .await;
        assert_eq!(r.status, 401, "{body}");
    }
    let big = json!({"code": "x".repeat(5000)}).to_string();
    let r = w
        .send(
            "POST",
            "/api/auth/oidc/exchange",
            &Jar::default(),
            Some(&big),
        )
        .await;
    assert_eq!(r.status, 413);
}

#[tokio::test]
async fn flow_is_offered_only_with_a_provider_and_a_burst_of_starts_is_limited() {
    let w = world(false).await;
    let none = Jar::default();
    let s = w.send("GET", "/api/auth/sso", &none, None).await;
    assert_eq!(s.json(), json!({"enabled": false}));
    failed(&w.start(&mut Jar::default()).await, "unavailable");
    w.auth
        .operator_sso_configure(TENANT, CLIENT, Some(CLIENT_SECRET), Some(true))
        .unwrap();
    assert_eq!(
        w.send("GET", "/api/auth/sso", &none, None).await.json(),
        json!({"enabled": true})
    );
    // Switched off, nothing is offered and nothing starts.
    w.auth
        .operator_sso_configure(TENANT, CLIENT, None, Some(false))
        .unwrap();
    assert_eq!(
        w.send("GET", "/api/auth/sso", &none, None).await.json(),
        json!({"enabled": false})
    );
    failed(&w.start(&mut Jar::default()).await, "unavailable");
    w.auth
        .operator_sso_configure(TENANT, CLIENT, None, Some(true))
        .unwrap();
    // The limit is per address and per minute.
    let mut words = Vec::new();
    for _ in 0..40 {
        let r = w.start(&mut Jar::default()).await;
        words.push(r.location().contains("oidc_error=busy"));
    }
    assert!(!words[0] && words[39], "starts are limited: {words:?}");
    assert_eq!(
        words.iter().filter(|b| !**b).count(),
        30 - 2,
        "30 per minute, two spent above"
    );
}

#[tokio::test]
async fn failures_a_discovery_document_that_points_elsewhere_is_not_followed() {
    let w = world(true).await;
    w.idp.discovery(|d| {
        d["token_endpoint"] = json!("https://evil.example/token");
    });
    failed(&w.start(&mut Jar::default()).await, "unavailable");
    assert!(
        w.events("oidc_error")
            .iter()
            .any(|l| l.contains("discovery document"))
    );
    assert_eq!(w.idp.hits("/token"), 0);
    // The failure is remembered for a short while: a second start does not ask again.
    let asked = w.idp.hits("openid-configuration");
    failed(&w.start(&mut Jar::default()).await, "unavailable");
    assert_eq!(w.idp.hits("openid-configuration"), asked);
    // Another wrong document, in a world with no memory of the first.
    let w = world(true).await;
    w.idp.discovery(|d| {
        d["issuer"] = json!("http://127.0.0.1:1/other/v2.0");
    });
    failed(&w.start(&mut Jar::default()).await, "unavailable");
}

#[tokio::test]
async fn admin_the_command_api_binds_configures_and_keeps_agents_out_of_the_provider() {
    let w = world(false).await;
    let boss = first_step(&w.router, "boss").await["token"]
        .as_str()
        .unwrap()
        .to_string();
    let r = &w.router;
    let ada = first_step(r, "ada").await["token"]
        .as_str()
        .unwrap()
        .to_string();
    // A member sees none of it.
    for (m, c) in [("sso_provider", "list"), ("sso_provider", "configure")] {
        assert_eq!(
            post(r, Some(&ada), &format!("/api/{m}/{c}"), json!({}))
                .await
                .0,
            404
        );
    }
    // An administrator configures the provider; the secret is sealed and never shown.
    let (s, b) = post(
        r,
        Some(&boss),
        "/api/sso_provider/configure",
        json!({"tenant": "common", "client_id": CLIENT, "client_secret": CLIENT_SECRET}),
    )
    .await;
    assert_eq!(s, 400, "{b}");
    assert!(b.to_string().contains("multi-tenant"), "{b}");
    let (s, b) = post(
        r,
        Some(&boss),
        "/api/sso_provider/configure",
        json!({"tenant": TENANT, "client_id": CLIENT, "client_secret": CLIENT_SECRET}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["value"]["secret_set"], true);
    assert_eq!(b["value"]["tenant"], TENANT);
    assert!(!b.to_string().contains(CLIENT_SECRET));
    let (_, g) = post(
        r,
        Some(&boss),
        "/api/sso_provider/get",
        json!({"id": "provider"}),
    )
    .await;
    assert_eq!(g["value"]["enabled"], true, "{g}");
    assert_eq!(g["value"]["break_glass_enough"], true, "{g}");
    // Bind, make SSO-only, and the listing shows it.
    let (s, b) = post(
        r,
        Some(&boss),
        "/api/user_account/bind_sso",
        json!({"id": "ada", "oid": ADA_OID}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["value"]["sso_oid"], ADA_OID);
    // Bound is not tried: the person signs in once with the button first.
    let (s, b) = post(
        r,
        Some(&boss),
        "/api/user_account/set_sso_only",
        json!({"id": "ada", "sso_only": true}),
    )
    .await;
    assert_ne!(s, 200, "{b}");
    assert!(b.to_string().contains("never signed in"), "{b}");
    let mut jar = Jar::default();
    let back = w.sign_in(&mut jar, ADA_OID).await;
    assert_eq!(w.exchange(&mut jar, &code_of(&back)).await.status, 200);
    let (s, b) = post(
        r,
        Some(&boss),
        "/api/user_account/set_sso_only",
        json!({"id": "ada", "sso_only": true}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["value"]["sso_only"], true);
    // Its password no longer signs in, and an administrator cannot set one.
    let (s, _) = post(
        r,
        None,
        "/api/auth/login",
        json!({"username": "ada", "password": PW}),
    )
    .await;
    assert_eq!(s, 401);
    let (s, b) = post(
        r,
        Some(&boss),
        "/api/auth/users/ada/reset-password",
        json!({"password": "a temporary password"}),
    )
    .await;
    assert_ne!(s, 200, "{b}");
    // An agent of the administrator can propose a binding but not touch the provider.
    let (_, t) = post(
        r,
        Some(&boss),
        "/api/auth/agent-token",
        json!({"scopes": ["platform.admin"]}),
    )
    .await;
    let agent = t["token"].as_str().unwrap().to_string();
    let (s, b) = post(
        r,
        Some(&agent),
        "/api/user_account/bind_sso",
        json!({"id": "bob", "oid": BOB_OID}),
    )
    .await;
    assert_eq!(
        (s, b["approval_required"].clone()),
        (202, json!(true)),
        "{b}"
    );
    let (s, b) = post(
        r,
        Some(&agent),
        "/api/sso_provider/configure",
        json!({"tenant": TENANT, "client_id": CLIENT, "client_secret": "attacker-secret"}),
    )
    .await;
    assert_ne!(s, 200, "{b}");
    assert!(
        b.to_string().contains("by a person"),
        "an agent cannot configure the provider: {b}"
    );
    let (s, b) = post(r, Some(&agent), "/api/sso_provider/clear", json!({})).await;
    assert_ne!(s, 200, "{b}");
    // The provider is still the one the administrator set.
    let (_, g) = post(
        r,
        Some(&boss),
        "/api/sso_provider/get",
        json!({"id": "provider"}),
    )
    .await;
    assert_eq!(g["value"]["tenant"], TENANT);
    // Over MCP the tool of the account commands is listed (D41: every surface or none).
    let admin_p = obop_commands::Principal::user("boss", &["platform.admin"]);
    let tools: Vec<String> = obop_mcp::schema::tools(&w.app.dispatcher, &admin_p)
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert!(
        tools.contains(&"user_account__bind_sso".to_string()),
        "{tools:?}"
    );
    w.assert_nowhere(CLIENT_SECRET);
    w.assert_nowhere("attacker-secret");
    assert_eq!(w.events("oidc_bind").len(), 1);
}

// ---- what the request asks for and what the token carries (scope, profile, optional claims) ---

fn flow_rows(w: &World) -> Vec<(String, J)> {
    w.app
        .store
        .backend()
        .kv_scan("auth.sso.flow:")
        .unwrap()
        .into_iter()
        .map(|(k, v)| (k, serde_json::from_str(&v).unwrap()))
        .collect()
}

fn oidc_rows(w: &World) -> usize {
    w.app
        .store
        .backend()
        .list("security_event")
        .unwrap()
        .into_iter()
        .filter(|(_, f)| f["kind"].as_str().is_some_and(|k| k.starts_with("oidc_")))
        .count()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[tokio::test]
async fn flow_asks_for_openid_and_profile_and_nothing_else_and_keeps_no_profile_claim() {
    // Microsoft sends the object id only with the `profile` scope; the name and the user name
    // that come with it are read by nobody and kept nowhere.
    let w = world(true).await;
    let mut jar = Jar::default();
    let s = w.start(&mut jar).await;
    let asked = pairs(s.location().split_once('?').unwrap().1);
    assert_eq!(asked["scope"], "openid profile");
    let uri = w.idp.authorize(&s.location(), ADA_OID);
    let back = w.callback(&mut jar, &uri).await;
    assert_eq!(w.exchange(&mut jar, &code_of(&back)).await.status, 200);
    assert_eq!(
        w.idp.st.lock().unwrap().scopes,
        vec!["openid profile".to_string()]
    );
    for needle in ["Ada Example", "ada@example.test", "preferred_username"] {
        w.assert_nowhere(needle);
    }
    // The binding holds the identity and when it was made and used, and nothing of the person.
    let links = w.app.store.backend().list("auth_sso_link").unwrap();
    assert_eq!(links.len(), 2);
    for (_, f) in links {
        let mut keys: Vec<&str> = f.keys().map(String::as_str).collect();
        keys.sort_unstable();
        let known = [
            "bound_at",
            "bound_by",
            "issuer",
            "last_login_at",
            "oid",
            "tid",
            "user",
        ];
        assert!(keys.iter().all(|k| known.contains(k)), "{keys:?}");
    }
}

#[tokio::test]
async fn failures_a_registration_without_the_profile_permission_gets_no_object_id_and_the_log_says_so()
 {
    // The scope is asked for, the permission is not consented to: the token has no `oid`. That is
    // what a real tenant does with "assignment required" before an administrator consents.
    let w = world(true).await;
    w.idp.st.lock().unwrap().consent = false;
    failed(&w.sign_in(&mut Jar::default(), ADA_OID).await, "failed");
    let lines = w.events("oidc_refused");
    assert!(
        lines.iter().any(|l| l.contains("no object id")
            && l.contains("profile scope")
            && l.contains("consent")),
        "{lines:?}"
    );
    // The person is told nothing more than the fixed word, and nobody has a session.
    assert_eq!(w.events("oidc_login").len(), 0);
    // With the consent given the same sign-in works.
    w.idp.reset();
    let back = w.sign_in(&mut Jar::default(), ADA_OID).await;
    assert!(back.location().starts_with("/#oidc="));
}

#[tokio::test]
async fn flow_without_the_optional_claims_nothing_stands_for_a_second_factor() {
    // The registration has no optional claims: the token has no `amr` and no `auth_time`, so
    // Microsoft is not heard to say that a second factor was used (D53 follows). An administrator
    // who signs in with Microsoft only is limited to enrolment, exactly as with a password.
    let cfg = AuthConfig {
        default_require_admins: true,
        ..AuthConfig::insecure_dev()
    };
    let w = world_with(true, cfg, None).await;
    w.auth.operator_sso_bind("boss", BOSS_OID).unwrap();
    // Before Microsoft-only: an administrator without a second factor is limited anyway.
    let mut jar = Jar::default();
    let back = w.sign_in(&mut jar, BOSS_OID).await;
    let ex = w.exchange(&mut jar, &code_of(&back)).await;
    assert_eq!(ex.json()["mfa_enrolment_required"], true, "{}", ex.body);
    w.auth.operator_sso_only("boss", true).unwrap();
    // The optional claims are configured: Microsoft's word stands for the second factor.
    let mut jar = Jar::default();
    let back = w.sign_in(&mut jar, BOSS_OID).await;
    let ex = w.exchange(&mut jar, &code_of(&back)).await;
    assert_eq!(ex.status, 200, "{}", ex.body);
    assert!(ex.json()["token"].is_string());
    assert!(
        ex.json().get("mfa_enrolment_required").is_none(),
        "{}",
        ex.body
    );
    // They are not: no second factor, a session that may only enrol.
    w.idp.without_optional_claims();
    let mut jar = Jar::default();
    let back = w.sign_in(&mut jar, BOSS_OID).await;
    let ex = w.exchange(&mut jar, &code_of(&back)).await;
    assert_eq!(ex.json()["mfa_enrolment_required"], true, "{}", ex.body);
    let lines = w.events("oidc_login");
    assert!(
        lines.last().unwrap().contains("does not report"),
        "{lines:?}"
    );
    assert!(lines[lines.len() - 2].contains("reports a second factor"));
    // And the other claims stay what they were: the same object id is the same person.
    assert_eq!(
        w.auth.operator_sso_status().unwrap()["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["sso_only"] == true)
            .count(),
        1
    );
}

// ---- lifetimes and single use -------------------------------------------------------------

#[tokio::test]
async fn flow_the_flow_lives_ten_minutes_and_the_one_time_code_one_minute() {
    let w = world(true).await;
    let mut jar = Jar::default();
    let started = w.start(&mut jar).await;
    let cookie = started.all("set-cookie")[0].to_string();
    assert!(cookie.contains("Max-Age=600;"), "{cookie}");
    // The row in the store is hashed by the state, and says itself when it ends.
    let rows = flow_rows(&w);
    assert_eq!(rows.len(), 1);
    let until = rows[0].1["expires_at_ms"].as_u64().unwrap();
    let now = now_ms();
    assert!(
        until > now + 590_000 && until < now + 610_000,
        "a finite lifetime of about ten minutes: {} s from now",
        (until as i64 - now as i64) / 1000
    );
    let uri = w.idp.authorize(&started.location(), ADA_OID);
    let state = pairs(uri.split_once('?').unwrap().1)["state"].clone();
    assert!(
        !rows[0].0.contains(&state),
        "the key is a hash of the state"
    );
    // A row that has outlived its time (a store that cleans up late) is not a flow, and the
    // cookie is cleared.
    let (key, mut rec) = rows[0].clone();
    rec["expires_at_ms"] = json!(1);
    let backend = w.app.store.backend();
    backend
        .kv_put(&key, &rec.to_string(), Some(600_000))
        .unwrap();
    let back = w.callback(&mut jar, &uri).await;
    failed(&back, "failed");
    assert!(
        back.all("set-cookie")
            .iter()
            .any(|c| c.starts_with("obop_sso_") && c.contains("Max-Age=0")),
        "the flow cookie is cleared: {:?}",
        back.all("set-cookie")
    );
    assert!(
        w.events("oidc_refused")
            .iter()
            .any(|l| l.contains("unknown, already used or expired"))
    );
    assert_eq!(
        w.idp.hits("/token"),
        0,
        "an expired flow never reaches the provider"
    );
    // A row with no expiry at all is no flow either.
    let mut jar = Jar::default();
    let started = w.start(&mut jar).await;
    let uri = w.idp.authorize(&started.location(), ADA_OID);
    let (key, mut rec) = flow_rows(&w).remove(0);
    rec.as_object_mut().unwrap().remove("expires_at_ms");
    backend
        .kv_put(&key, &rec.to_string(), Some(600_000))
        .unwrap();
    failed(&w.callback(&mut jar, &uri).await, "failed");
    assert_eq!(w.idp.hits("/token"), 0);

    // The one-time code: good at 59 seconds, gone at 60.
    let mut late = Jar::default();
    let code_a = code_of(&w.sign_in(&mut late, ADA_OID).await);
    w.auth.set_clock_for_test(T0 + 59);
    assert_eq!(w.exchange(&mut late, &code_a).await.status, 200);
    w.auth.set_clock_for_test(T0);
    let mut later = Jar::default();
    let code_b = code_of(&w.sign_in(&mut later, ADA_OID).await);
    w.auth.set_clock_for_test(T0 + 60);
    assert_eq!(w.exchange(&mut later, &code_b).await.status, 401);
    w.auth.set_clock_for_test(T0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn flow_a_flow_and_a_one_time_code_are_spent_once_when_two_requests_race() {
    let w = world(true).await;
    for round in 0..6 {
        let mut jar = Jar::default();
        let s = w.start(&mut jar).await;
        let uri = w.idp.authorize(&s.location(), ADA_OID);
        let (mut a, mut b) = (jar.clone(), jar.clone());
        let (x, y) = tokio::join!(w.callback(&mut a, &uri), w.callback(&mut b, &uri));
        let won = |r: &Raw| r.location().starts_with("/#oidc=");
        assert_eq!(
            usize::from(won(&x)) + usize::from(won(&y)),
            1,
            "round {round}: exactly one callback gets the flow: {} / {}",
            x.location(),
            y.location()
        );
        let (code, winner) = if won(&x) {
            (code_of(&x), a)
        } else {
            (code_of(&y), b)
        };
        let (mut c, mut d) = (winner.clone(), winner);
        let (p, q) = tokio::join!(w.exchange(&mut c, &code), w.exchange(&mut d, &code));
        assert_eq!(
            [p.status, q.status].iter().filter(|s| **s == 200).count(),
            1,
            "round {round}: exactly one exchange gets the code: {} / {}",
            p.status,
            q.status
        );
    }
    assert_eq!(
        w.idp.hits("/token"),
        6,
        "the provider was asked once per flow"
    );
}

#[tokio::test]
async fn failures_a_cookie_with_the_right_name_and_a_wrong_value_is_another_browser() {
    let w = world(true).await;
    let mut jar = Jar::default();
    let s = w.start(&mut jar).await;
    let uri = w.idp.authorize(&s.location(), ADA_OID);
    let name = jar
        .0
        .keys()
        .find(|k| k.starts_with("obop_sso_"))
        .unwrap()
        .clone();
    let mut wrong = Jar::default();
    wrong.0.insert(name, "A".repeat(43));
    failed(&w.callback(&mut wrong, &uri).await, "failed");
    assert!(
        w.events("oidc_refused")
            .iter()
            .any(|l| l.contains("not the one that began"))
    );
    assert_eq!(
        w.idp.hits("/token"),
        0,
        "refused before the provider is asked"
    );
}

#[tokio::test]
async fn flow_behind_https_the_cookies_are_secure_and_the_return_goes_to_the_back_office() {
    let w = world_with(true, AuthConfig::insecure_dev(), Some("/admin")).await;
    let https = [("x-forwarded-proto", "https")];
    let mut jar = Jar::default();
    let s = w
        .send_h("GET", "/api/auth/oidc/start", &jar, None, &https)
        .await;
    jar.absorb(&s);
    let cookie = s.all("set-cookie")[0].to_string();
    assert!(
        cookie.contains("; Secure") && cookie.contains("HttpOnly"),
        "{cookie}"
    );
    let asked = pairs(s.location().split_once('?').unwrap().1);
    assert_eq!(
        asked["redirect_uri"],
        "https://obop.test/api/auth/oidc/callback"
    );
    let uri = w.idp.authorize(&s.location(), ADA_OID);
    let back = w.send_h("GET", &uri, &jar, None, &https).await;
    jar.absorb(&back);
    // The return is the back office, not the root, and the cookies keep their flags.
    let l = back.location();
    assert!(l.starts_with("/admin/#oidc="), "{l}");
    let set = back.all("set-cookie");
    assert!(
        set.iter().all(|c| c.contains("; Secure")),
        "the cleared flow cookie and the done cookie are Secure: {set:?}"
    );
    assert!(
        set.iter()
            .any(|c| c.starts_with("obop_sso_done=") && c.contains("Max-Age=60;"))
    );
    let code = l.strip_prefix("/admin/#oidc=").unwrap().to_string();
    assert_eq!(w.exchange(&mut jar, &code).await.status, 200);
    // A failure goes to the same place.
    let r = w
        .callback(&mut Jar::default(), "/api/auth/oidc/callback?state=zz")
        .await;
    assert_eq!(r.location(), "/admin/#oidc_error=failed");
}

// ---- anonymous requests and the hardening of the calls to Microsoft ------------------------

#[tokio::test]
async fn failures_a_request_that_is_not_a_navigation_does_nothing() {
    let w = world(true).await;
    let as_image = [("sec-fetch-mode", "no-cors"), ("sec-fetch-dest", "image")];
    let r = w
        .send_h(
            "GET",
            "/api/auth/oidc/start",
            &Jar::default(),
            None,
            &as_image,
        )
        .await;
    assert_eq!(r.status, 400, "{}", r.body);
    assert!(flow_rows(&w).is_empty());
    assert_eq!(w.idp.hits("openid-configuration"), 0);
    // A click, a typed address and the redirect back from Microsoft are navigations.
    let nav = [
        ("sec-fetch-mode", "navigate"),
        ("sec-fetch-dest", "document"),
        ("sec-fetch-site", "cross-site"),
    ];
    let mut jar = Jar::default();
    let s = w
        .send_h("GET", "/api/auth/oidc/start", &jar, None, &nav)
        .await;
    jar.absorb(&s);
    let uri = w.idp.authorize(&s.location(), ADA_OID);
    // The same address inside a frame is not, and does not spend the flow.
    let framed = [("sec-fetch-mode", "navigate"), ("sec-fetch-dest", "iframe")];
    let r = w.send_h("GET", &uri, &jar, None, &framed).await;
    assert_eq!(r.status, 400, "{}", r.body);
    assert_eq!(flow_rows(&w).len(), 1, "the flow is still there");
    let back = w.send_h("GET", &uri, &jar, None, &nav).await;
    assert!(
        back.location().starts_with("/#oidc="),
        "{}",
        back.location()
    );
}

#[tokio::test]
async fn failures_anonymous_requests_do_not_fill_the_log_or_use_up_the_limits() {
    let w = world(true).await;
    let before = oidc_rows(&w);
    // A callback with no valid state is nothing: no line, no flow, no count against the address.
    for i in 0..200 {
        let uri = match i % 3 {
            0 => "/api/auth/oidc/callback",
            1 => "/api/auth/oidc/callback?state=zz&code=x",
            _ => "/api/auth/oidc/callback?error=access_denied",
        };
        failed(&w.callback(&mut Jar::default(), uri).await, "failed");
    }
    assert_eq!(
        oidc_rows(&w),
        before,
        "no line for requests with no valid state"
    );
    // The address still has its allowance.
    let mut jar = Jar::default();
    let back = w.sign_in(&mut jar, ADA_OID).await;
    assert!(back.location().starts_with("/#oidc="));
    // Starts: up to the limit, and one line for all of them.
    for _ in 0..28 {
        assert_eq!(w.start(&mut Jar::default()).await.status, 303);
    }
    assert_eq!(
        w.events("oidc_start").len(),
        1,
        "one line per address and ten minutes"
    );
    // A state that looks right and is unknown: one line for the lot, until the limit.
    let mut busy = 0;
    for i in 0..130 {
        let state = format!("{:064x}", i + 1);
        let r = w
            .callback(
                &mut Jar::default(),
                &format!("/api/auth/oidc/callback?code=x&state={state}"),
            )
            .await;
        if r.location().ends_with("busy") {
            busy += 1;
        }
    }
    assert!(
        busy > 0,
        "well-formed states that are no flow are limited per address"
    );
    let unknown: Vec<String> = w
        .events("oidc_refused")
        .into_iter()
        .filter(|l| l.contains("unknown, already used or expired"))
        .collect();
    assert_eq!(unknown.len(), 1, "{unknown:?}");
    assert!(unknown[0].contains("counted, not written"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn failures_a_cold_cache_and_many_starts_ask_the_provider_once() {
    let w = world(true).await;
    w.idp.st.lock().unwrap().discovery_delay_ms = 200;
    let (mut a, mut b, mut c, mut d) = (
        Jar::default(),
        Jar::default(),
        Jar::default(),
        Jar::default(),
    );
    let (r1, r2, r3, r4) = tokio::join!(
        w.start(&mut a),
        w.start(&mut b),
        w.start(&mut c),
        w.start(&mut d)
    );
    for r in [r1, r2, r3, r4] {
        assert_eq!(r.status, 303);
    }
    assert_eq!(
        w.idp.hits("openid-configuration"),
        1,
        "one fetch of the discovery document, the others waited for it"
    );
}

#[tokio::test]
async fn failures_a_key_set_that_cannot_be_fetched_is_remembered_for_a_short_while() {
    let w = world(true).await;
    w.idp.st.lock().unwrap().jwks_status = 503;
    let mut words = Vec::new();
    for _ in 0..4 {
        let r = w.sign_in(&mut Jar::default(), ADA_OID).await;
        words.push(r.location().rsplit('=').next().unwrap().to_string());
    }
    assert_eq!(words, ["unavailable"; 4]);
    assert_eq!(
        w.idp.hits("/discovery/v2.0/keys"),
        1,
        "the provider is not asked again for every sign-in"
    );
    assert!(
        w.events("oidc_error")
            .iter()
            .any(|l| l.contains("key set could not be fetched"))
    );
}

#[tokio::test]
async fn failures_exchanges_are_limited_per_address_and_the_limit_is_a_429() {
    let w = world(true).await;
    let before = oidc_rows(&w);
    let mut last = 0;
    let mut limited = 0;
    for _ in 0..40 {
        let r = w.exchange(&mut Jar::default(), "obopsso_nothing").await;
        last = r.status;
        if r.status == 429 {
            limited += 1;
            assert_eq!(r.json()["code"], "rate_limited");
        }
    }
    assert_eq!(last, 429);
    assert_eq!(limited, 10, "30 a minute, the rest answered at once");
    // The answer of a refused exchange is the same plain failure as for any other.
    assert_eq!(
        oidc_rows(&w),
        before,
        "an exchange of nothing writes no line"
    );
}
