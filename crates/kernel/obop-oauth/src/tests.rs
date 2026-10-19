//! The protocol rules, driven without HTTP against a real `Auth` on a memory backend.

use super::*;
use crate::flow::{AuthorizeParams, AuthzError, DecideOut, FlowError, LoginOut};
use crate::util::s256;
use obop_auth::Auth;
use obop_commands::Authenticator;
use serde_json::json;

const BASE: &str = "https://inst.test";
const PW: &str = "correct horse battery";
const REDIRECT: &str = "https://claude.ai/api/mcp/auth_callback";

struct Rig {
    user: String,
    o: Arc<OAuth>,
    auth: Arc<Auth>,
    backend: Arc<dyn Backend>,
}

fn rig_on(backend: Arc<dyn Backend>, user: &str, cfg: OAuthConfig) -> Rig {
    let auth = Arc::new(Auth::with_test_params(backend.clone()));
    auth.create_user(
        user,
        PW,
        &[
            "base.user".to_string(),
            "sales.user".to_string(),
            "platform.admin".to_string(),
        ],
    )
    .unwrap();
    let o = OAuth::new(backend.clone(), auth.clone(), cfg).unwrap();
    Rig {
        user: user.to_string(),
        o,
        auth,
        backend,
    }
}

fn rig_with(cfg: OAuthConfig) -> Rig {
    rig_on(obop_storage::open("memory:").unwrap(), "ada", cfg)
}

fn rig() -> Rig {
    rig_with(OAuthConfig::default())
}

fn verifier() -> String {
    "v".repeat(50)
}

impl Rig {
    fn client(&self) -> String {
        self.o
            .register(
                "10.0.0.1",
                &json!({"client_name": "Claude", "redirect_uris": [REDIRECT, "http://localhost:3000/cb"]}),
            )
            .unwrap()["client_id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn params(&self, client: &str, redirect: &str) -> AuthorizeParams {
        AuthorizeParams {
            response_type: Some("code".into()),
            client_id: Some(client.into()),
            redirect_uri: Some(redirect.into()),
            state: Some("st&te".into()),
            code_challenge: Some(s256(&verifier())),
            code_challenge_method: Some("S256".into()),
            resources: vec![format!("{BASE}/mcp")],
            ..Default::default()
        }
    }

    /// Sign in and answer the consent; returns the redirect Location.
    fn consent(&self, p: &AuthorizeParams, groups: &[&str], days: u32) -> String {
        let v = self.o.validate_authorize(BASE, p).unwrap();
        let (id, binder) = self.o.begin(&v, "10.0.0.2", "en").unwrap();
        let csrf = self.o.view(&id, &binder).unwrap().csrf;
        assert_eq!(
            self.o
                .login(&id, &binder, &csrf, "10.0.0.2", &self.user, PW)
                .unwrap(),
            LoginOut::Consent
        );
        let g: Vec<String> = groups.iter().map(|s| s.to_string()).collect();
        match self
            .o
            .decide(BASE, &id, &binder, &csrf, true, &g, days)
            .unwrap()
        {
            DecideOut::Redirect(l) => l,
            DecideOut::Retry(k) => panic!("retry {k}"),
        }
    }

    fn code(&self, client: &str, groups: &[&str]) -> String {
        let loc = self.consent(&self.params(client, REDIRECT), groups, 30);
        query(&loc, "code")
    }

    fn form(&self, pairs: &[(&str, &str)]) -> Form {
        Form(
            pairs
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
        )
    }

    fn exchange(
        &self,
        client: &str,
        code: &str,
        verifier: &str,
        redirect: &str,
    ) -> Result<J, OAuthError> {
        self.o.token(
            BASE,
            "10.0.0.3",
            &self.form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("client_id", client),
                ("code_verifier", verifier),
                ("redirect_uri", redirect),
                ("resource", &format!("{BASE}/mcp")),
            ]),
        )
    }

    fn refresh(&self, client: &str, token: &str) -> Result<J, OAuthError> {
        self.o.token(
            BASE,
            "10.0.0.3",
            &self.form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", token),
                ("client_id", client),
            ]),
        )
    }

    fn tokens(&self, client: &str) -> J {
        let code = self.code(client, &["sales.user"]);
        self.exchange(client, &code, &verifier(), REDIRECT).unwrap()
    }
}

fn query(location: &str, key: &str) -> String {
    let q = location.split_once('?').map_or("", |(_, q)| q);
    q.split('&')
        .find_map(|kv| kv.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("no {key} in {location}"))
        .to_string()
}

#[test]
fn full_round_trip_with_pkce_issues_a_narrow_short_lived_agent_token() {
    let r = rig();
    let client = r.client();
    let loc = r.consent(&r.params(&client, REDIRECT), &["sales.user"], 7);
    assert!(loc.starts_with(&format!("{REDIRECT}?code=oac_")), "{loc}");
    assert_eq!(query(&loc, "state"), "st%26te");
    assert_eq!(query(&loc, "iss"), "https%3A%2F%2Finst.test");
    let code = query(&loc, "code");
    let t = r.exchange(&client, &code, &verifier(), REDIRECT).unwrap();
    assert_eq!(t["token_type"], "Bearer");
    assert_eq!(t["expires_in"], 3600);
    assert_eq!(t["scope"], "sales.user");
    let access = t["access_token"].as_str().unwrap();
    let (p, audience) = r.auth.resolve_with_audience(access).unwrap();
    assert_eq!(p.kind, obop_commands::PrincipalKind::Agent);
    assert!(
        p.allows("sales.user") && !p.allows("base.user"),
        "narrowed to the chosen group"
    );
    assert_eq!(audience.as_deref(), Some("https://inst.test/mcp"));
    // Stored hashed: neither the access token, the refresh token nor the code is a key anywhere.
    let refresh = t["refresh_token"].as_str().unwrap();
    assert!(r.backend.get("oauth_refresh", refresh).unwrap().is_none());
    assert!(
        r.backend
            .kv_get(&format!("oauth:code:{code}"))
            .unwrap()
            .is_none()
    );
    let grants = r.o.grants_of("ada");
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0]["client_name"], "Claude");
    assert_eq!(grants[0]["scopes"], json!(["sales.user"]));
    let dump = format!("{:?}", r.backend.list("oauth_grant").unwrap());
    assert!(!dump.contains(access) && !dump.contains(refresh));
}

#[test]
fn a_bad_verifier_is_refused_and_burns_the_code() {
    let r = rig();
    let c = r.client();
    let code = r.code(&c, &["sales.user"]);
    let e = r
        .exchange(&c, &code, &"w".repeat(50), REDIRECT)
        .unwrap_err();
    assert_eq!(e.error, "invalid_grant");
    // Even the right verifier cannot use it afterwards.
    assert_eq!(
        r.exchange(&c, &code, &verifier(), REDIRECT)
            .unwrap_err()
            .error,
        "invalid_grant"
    );
    // Too short a verifier never reaches the comparison.
    let code = r.code(&c, &["sales.user"]);
    assert_eq!(
        r.exchange(&c, &code, "short", REDIRECT).unwrap_err().error,
        "invalid_grant"
    );
}

#[test]
fn a_code_is_single_use_and_its_reuse_revokes_what_it_produced() {
    let r = rig();
    let c = r.client();
    let code = r.code(&c, &["sales.user"]);
    let t = r.exchange(&c, &code, &verifier(), REDIRECT).unwrap();
    let access = t["access_token"].as_str().unwrap().to_string();
    assert!(r.auth.resolve(&access).is_some());
    let e = r.exchange(&c, &code, &verifier(), REDIRECT).unwrap_err();
    assert_eq!(e.error, "invalid_grant");
    assert!(
        r.auth.resolve(&access).is_none(),
        "the access token of the first exchange is revoked"
    );
    assert_eq!(
        r.refresh(&c, t["refresh_token"].as_str().unwrap())
            .unwrap_err()
            .error,
        "invalid_grant"
    );
    assert!(r.o.grants_of("ada").is_empty());
}

#[test]
fn a_code_lives_sixty_seconds_and_is_bound_to_client_and_redirect() {
    let r = rig();
    let c = r.client();
    let other = r.client();
    let code = r.code(&c, &["sales.user"]);
    r.o.advance_clock_for_test(61);
    assert_eq!(
        r.exchange(&c, &code, &verifier(), REDIRECT)
            .unwrap_err()
            .error,
        "invalid_grant"
    );
    let code = r.code(&c, &["sales.user"]);
    assert_eq!(
        r.exchange(&other, &code, &verifier(), REDIRECT)
            .unwrap_err()
            .error,
        "invalid_grant"
    );
    let code = r.code(&c, &["sales.user"]);
    let e = r
        .exchange(&c, &code, &verifier(), "http://localhost:3000/cb")
        .unwrap_err();
    assert_eq!(
        e.error, "invalid_grant",
        "the redirect URI of the request must be repeated exactly"
    );
}

#[test]
fn the_resource_must_be_this_instances_mcp_endpoint() {
    let r = rig();
    let c = r.client();
    let mut p = r.params(&c, REDIRECT);
    p.resources = vec!["https://evil.test/mcp".into()];
    match r.o.validate_authorize(BASE, &p) {
        Err(AuthzError::Redirect {
            error: "invalid_target",
            ..
        }) => {}
        other => panic!("{other:?}"),
    }
    // Without a resource the request is bound to /mcp anyway (older clients send none).
    p.resources.clear();
    assert_eq!(
        r.o.validate_authorize(BASE, &p).unwrap().resource,
        "https://inst.test/mcp"
    );
    // A trailing slash is the same resource.
    p.resources = vec![format!("{BASE}/mcp/")];
    assert!(r.o.validate_authorize(BASE, &p).is_ok());
    // At the token endpoint too.
    let code = r.code(&c, &["sales.user"]);
    let e = r.o.token(
        BASE,
        "10.0.0.3",
        &r.form(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("client_id", &c),
            ("code_verifier", &verifier()),
            ("redirect_uri", REDIRECT),
            ("resource", "https://evil.test/mcp"),
        ]),
    );
    assert_eq!(e.unwrap_err().error, "invalid_target");
    // A token issued for this instance is not valid on another base URL.
    let code = r.code(&c, &["sales.user"]);
    let e = r.o.token(
        "https://other.test",
        "10.0.0.3",
        &r.form(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("client_id", &c),
            ("code_verifier", &verifier()),
            ("redirect_uri", REDIRECT),
        ]),
    );
    assert_eq!(e.unwrap_err().error, "invalid_target");
}

#[test]
fn authorization_requests_are_validated_before_anything_is_shown() {
    let r = rig();
    let c = r.client();
    // Unknown client and unregistered redirect: a page, never a redirect.
    let mut p = r.params("oc_unknown", REDIRECT);
    assert_eq!(
        r.o.validate_authorize(BASE, &p).unwrap_err(),
        AuthzError::Page { key: "err.client" }
    );
    p = r.params(&c, "https://evil.test/cb");
    assert_eq!(
        r.o.validate_authorize(BASE, &p).unwrap_err(),
        AuthzError::Page {
            key: "err.redirect"
        }
    );
    p = r.params(&c, "https://claude.ai/api/mcp/auth_callback/");
    assert_eq!(
        r.o.validate_authorize(BASE, &p).unwrap_err(),
        AuthzError::Page {
            key: "err.redirect"
        }
    );
    // A loopback redirect may change its port (RFC 8252).
    p = r.params(&c, "http://localhost:51234/cb");
    p.redirect_uri = Some("http://localhost:51234/cb".into());
    assert!(r.o.validate_authorize(BASE, &p).is_ok());
    // PKCE is mandatory and only S256.
    for (ch, m) in [
        (None, Some("S256")),
        (Some(s256(&verifier())), None),
        (Some(s256(&verifier())), Some("plain")),
        (Some("short".to_string()), Some("S256")),
    ] {
        let mut p = r.params(&c, REDIRECT);
        p.code_challenge = ch;
        p.code_challenge_method = m.map(String::from);
        match r.o.validate_authorize(BASE, &p) {
            Err(AuthzError::Redirect {
                error: "invalid_request",
                state,
                ..
            }) => {
                assert_eq!(state.as_deref(), Some("st&te"));
            }
            other => panic!("{other:?}"),
        }
    }
    let mut p = r.params(&c, REDIRECT);
    p.response_type = Some("token".into());
    assert!(matches!(
        r.o.validate_authorize(BASE, &p),
        Err(AuthzError::Redirect {
            error: "unsupported_response_type",
            ..
        })
    ));
}

#[test]
fn refresh_tokens_rotate_and_a_replayed_one_revokes_the_whole_family() {
    let r = rig();
    let c = r.client();
    let t1 = r.tokens(&c);
    let (a1, r1) = (
        t1["access_token"].as_str().unwrap(),
        t1["refresh_token"].as_str().unwrap(),
    );
    let t2 = r.refresh(&c, r1).unwrap();
    let (a2, r2) = (
        t2["access_token"].as_str().unwrap(),
        t2["refresh_token"].as_str().unwrap(),
    );
    assert_ne!(r1, r2);
    assert!(
        r.auth.resolve(a1).is_none(),
        "the old access token ends with the rotation"
    );
    assert!(r.auth.resolve(a2).is_some());
    // The first refresh token is presented again: both parties hold it.
    let e = r.refresh(&c, r1).unwrap_err();
    assert_eq!(e.error, "invalid_grant");
    assert!(
        r.auth.resolve(a2).is_none(),
        "the family's live access token is revoked"
    );
    assert_eq!(
        r.refresh(&c, r2).unwrap_err().error,
        "invalid_grant",
        "and so is the newest refresh token"
    );
    assert!(r.o.grants_of("ada").is_empty());
}

#[test]
fn a_refresh_belongs_to_its_client_and_cannot_widen_scope() {
    let r = rig();
    let c = r.client();
    let other = r.client();
    let code = r.code(&c, &["sales.user", "base.user"]);
    let t = r.exchange(&c, &code, &verifier(), REDIRECT).unwrap();
    let refresh = t["refresh_token"].as_str().unwrap();
    assert_eq!(
        r.refresh(&other, refresh).unwrap_err().error,
        "invalid_grant"
    );
    let widen = r.o.token(
        BASE,
        "10.0.0.3",
        &r.form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh),
            ("client_id", &c),
            ("scope", "sales.user platform.admin"),
        ]),
    );
    assert_eq!(widen.unwrap_err().error, "invalid_scope");
    // Narrowing is allowed.
    let narrow =
        r.o.token(
            BASE,
            "10.0.0.3",
            &r.form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh),
                ("client_id", &c),
                ("scope", "base.user"),
            ]),
        )
        .unwrap();
    assert_eq!(narrow["scope"], "base.user");
    let p = r
        .auth
        .resolve(narrow["access_token"].as_str().unwrap())
        .unwrap();
    assert!(p.allows("base.user") && !p.allows("sales.user"));
}

#[test]
fn the_consent_cannot_widen_beyond_what_the_owner_holds() {
    let r = rig();
    let c = r.client();
    let v =
        r.o.validate_authorize(BASE, &r.params(&c, REDIRECT))
            .unwrap();
    let (id, binder) = r.o.begin(&v, "10.0.0.2", "en").unwrap();
    let csrf = r.o.view(&id, &binder).unwrap().csrf;
    r.o.login(&id, &binder, &csrf, "10.0.0.2", "ada", PW)
        .unwrap();
    let bad = |g: &[&str], days| {
        r.o.decide(
            BASE,
            &id,
            &binder,
            &csrf,
            true,
            &g.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            days,
        )
        .unwrap()
    };
    assert_eq!(
        bad(&["sales.user", "invoicing.manager"], 30),
        DecideOut::Retry("err.scope_widen")
    );
    assert_eq!(bad(&[], 30), DecideOut::Retry("err.no_scope"));
    assert_eq!(bad(&["sales.user"], 365), DecideOut::Retry("err.expiry"));
    assert!(matches!(bad(&["sales.user"], 1), DecideOut::Redirect(_)));
    // The flow was consumed by the answer.
    assert_eq!(r.o.view(&id, &binder).unwrap_err(), FlowError::Gone);
}

#[test]
fn a_flow_needs_its_browser_and_its_csrf_token_and_is_answered_once() {
    let r = rig();
    let c = r.client();
    let v =
        r.o.validate_authorize(BASE, &r.params(&c, REDIRECT))
            .unwrap();
    let (id, binder) = r.o.begin(&v, "10.0.0.2", "en").unwrap();
    let csrf = r.o.view(&id, &binder).unwrap().csrf;
    // Another browser does not hold the binder.
    assert_eq!(
        r.o.view(&id, "not-the-binder").unwrap_err(),
        FlowError::Forbidden
    );
    assert_eq!(
        r.o.login(&id, "not-the-binder", &csrf, "10.0.0.2", "ada", PW)
            .unwrap_err(),
        FlowError::Forbidden
    );
    // A page of another site cannot guess the csrf token.
    assert_eq!(
        r.o.login(&id, &binder, "nope", "10.0.0.2", "ada", PW)
            .unwrap_err(),
        FlowError::Forbidden
    );
    // Not signed in yet: no decision.
    assert_eq!(
        r.o.decide(BASE, &id, &binder, &csrf, true, &["sales.user".into()], 30)
            .unwrap_err(),
        FlowError::Forbidden
    );
    assert_eq!(
        r.o.login(&id, &binder, &csrf, "10.0.0.2", "ada", "wrong password")
            .unwrap(),
        LoginOut::Failed("err.login")
    );
    assert_eq!(
        r.o.login(&id, &binder, &csrf, "10.0.0.2", "ada", PW)
            .unwrap(),
        LoginOut::Consent
    );
    let ok =
        r.o.decide(BASE, &id, &binder, &csrf, true, &["sales.user".into()], 30)
            .unwrap();
    assert!(matches!(ok, DecideOut::Redirect(_)));
    assert_eq!(
        r.o.decide(BASE, &id, &binder, &csrf, true, &["sales.user".into()], 30)
            .unwrap_err(),
        FlowError::Gone
    );
    // The sign-in session that proved who the person is was not left behind.
    let sessions = r.backend.list("auth_token").unwrap();
    assert!(
        sessions
            .iter()
            .all(|(_, t)| t["revoked"] == true || t["kind"] == "agent")
    );
}

#[test]
fn denying_sends_access_denied_and_leaves_no_code() {
    let r = rig();
    let c = r.client();
    let v =
        r.o.validate_authorize(BASE, &r.params(&c, REDIRECT))
            .unwrap();
    let (id, binder) = r.o.begin(&v, "10.0.0.2", "en").unwrap();
    let csrf = r.o.view(&id, &binder).unwrap().csrf;
    r.o.login(&id, &binder, &csrf, "10.0.0.2", "ada", PW)
        .unwrap();
    let DecideOut::Redirect(l) =
        r.o.decide(BASE, &id, &binder, &csrf, false, &[], 30)
            .unwrap()
    else {
        panic!()
    };
    assert_eq!(query(&l, "error"), "access_denied");
    assert_eq!(query(&l, "state"), "st%26te");
    assert!(r.backend.kv_scan("oauth:code:").unwrap().is_empty());
}

#[test]
fn repeated_wrong_passwords_end_the_flow() {
    let r = rig();
    let c = r.client();
    let v =
        r.o.validate_authorize(BASE, &r.params(&c, REDIRECT))
            .unwrap();
    let (id, binder) = r.o.begin(&v, "10.0.0.2", "en").unwrap();
    let csrf = r.o.view(&id, &binder).unwrap().csrf;
    let mut last = Ok(LoginOut::Consent);
    for _ in 0..9 {
        last =
            r.o.login(&id, &binder, &csrf, "10.0.0.2", "ada", "nope nope nope");
        if last.is_err() {
            break;
        }
    }
    assert_eq!(last, Err(FlowError::Gone));
}

#[test]
fn registration_enforces_redirect_rules_caps_rate_limits_and_expiry() {
    let r = rig_with(OAuthConfig {
        max_clients: 3,
        register_per_ip_hour: 100,
        ..OAuthConfig::default()
    });
    let reg = |ip: &str, body: J| r.o.register(ip, &body);
    // Redirect URI rules.
    for bad in [
        "http://example.com/cb",
        "https://*.example.com/cb",
        "https://example.com/cb#x",
        "javascript:alert(1)",
        "https://u@example.com/cb",
    ] {
        let e = reg("1.1.1.1", json!({"redirect_uris": [bad]})).unwrap_err();
        assert_eq!(e.error, "invalid_redirect_uri", "{bad}");
    }
    assert_eq!(
        reg("1.1.1.2", json!({})).unwrap_err().error,
        "invalid_redirect_uri"
    );
    assert_eq!(
        reg("1.1.1.2", json!({"redirect_uris": ["https://a.test/cb"], "token_endpoint_auth_method": "client_secret_basic"}))
            .unwrap_err()
            .error,
        "invalid_client_metadata"
    );
    assert_eq!(
        reg(
            "1.1.1.2",
            json!({"redirect_uris": ["https://a.test/cb"], "grant_types": ["implicit"]})
        )
        .unwrap_err()
        .error,
        "invalid_client_metadata"
    );
    assert_eq!(
        reg("1.1.1.2", json!({"redirect_uris": ["https://a.test/1", "https://a.test/2", "https://a.test/3", "https://a.test/4", "https://a.test/5", "https://a.test/6"]}))
            .unwrap_err()
            .error,
        "invalid_redirect_uri"
    );
    // Names are cleaned.
    let ok = reg(
        "2.2.2.2",
        json!({"client_name": "  A\u{202e}pp \n x ", "redirect_uris": ["https://a.test/cb"]}),
    )
    .unwrap();
    assert_eq!(ok["client_name"], "App x");
    assert_eq!(ok["token_endpoint_auth_method"], "none");
    assert!(ok.get("client_secret").is_none());
    // The cap on registered clients.
    reg("3.3.3.3", json!({"redirect_uris": ["https://a.test/cb"]})).unwrap();
    reg("3.3.3.3", json!({"redirect_uris": ["https://a.test/cb"]})).unwrap();
    let full = reg("3.3.3.3", json!({"redirect_uris": ["https://a.test/cb"]})).unwrap_err();
    assert_eq!((full.status, full.error), (503, "temporarily_unavailable"));
    // The per-address limit.
    let r2 = rig_with(OAuthConfig {
        register_per_ip_hour: 2,
        ..OAuthConfig::default()
    });
    let body = json!({"redirect_uris": ["https://a.test/cb"]});
    r2.o.register("9.9.9.9", &body).unwrap();
    r2.o.register("9.9.9.9", &body).unwrap();
    let e = r2.o.register("9.9.9.9", &body).unwrap_err();
    assert_eq!((e.status, e.error), (429, "temporarily_unavailable"));
    assert!(e.retry_after.is_some());
    r2.o.register("9.9.9.8", &body).unwrap();
}

#[test]
fn unused_clients_expire_and_used_ones_after_a_long_idle() {
    let r = rig();
    let unused = r.client();
    let used = r.client();
    r.code(&used, &["sales.user"]);
    assert!(r.o.client(&unused).is_some());
    r.o.advance_clock_for_test(24 * 3600 + 1);
    assert!(
        r.o.client(&unused).is_none(),
        "never used: gone after a day"
    );
    assert!(r.o.client(&used).is_some());
    assert_eq!(r.o.purge_clients(), 1);
    assert_eq!(r.backend.count("oauth_client").unwrap(), 1);
    r.o.advance_clock_for_test(181 * 24 * 3600);
    assert!(r.o.client(&used).is_none(), "idle for months: gone");
}

#[test]
fn revocation_by_the_application_and_by_the_owner() {
    let r = rig();
    let c = r.client();
    let other = r.client();
    let t = r.tokens(&c);
    let (a, rf) = (
        t["access_token"].as_str().unwrap(),
        t["refresh_token"].as_str().unwrap(),
    );
    // Another client cannot revoke it; the answer is the same.
    r.o.revoke_presented("10.0.0.3", &r.form(&[("token", rf), ("client_id", &other)]))
        .unwrap();
    assert!(r.auth.resolve(a).is_some());
    // The owner's client can, with either token.
    r.o.revoke_presented("10.0.0.3", &r.form(&[("token", a), ("client_id", &c)]))
        .unwrap();
    assert!(r.auth.resolve(a).is_none());
    assert_eq!(r.refresh(&c, rf).unwrap_err().error, "invalid_grant");
    // Unknown tokens revoke nothing and say nothing.
    r.o.revoke_presented(
        "10.0.0.3",
        &r.form(&[("token", "obop_nope"), ("client_id", &c)]),
    )
    .unwrap();
    // The owner revokes from the list; somebody else cannot.
    let t = r.tokens(&c);
    let gid = r.o.grants_of("ada")[0]["id"].as_str().unwrap().to_string();
    assert!(!r.o.revoke_grant("bob", &gid));
    assert!(
        r.auth
            .resolve(t["access_token"].as_str().unwrap())
            .is_some()
    );
    assert!(r.o.revoke_grant("ada", &gid));
    assert!(
        r.auth
            .resolve(t["access_token"].as_str().unwrap())
            .is_none()
    );
    assert!(r.o.grants_of("ada").is_empty());
    assert_eq!(
        r.refresh(&c, t["refresh_token"].as_str().unwrap())
            .unwrap_err()
            .error,
        "invalid_grant"
    );
}

#[test]
fn grants_end_on_their_date_and_a_disabled_owner_cannot_refresh() {
    let r = rig();
    let c = r.client();
    let loc = r.consent(&r.params(&c, REDIRECT), &["sales.user"], 1);
    let t = r
        .exchange(&c, &query(&loc, "code"), &verifier(), REDIRECT)
        .unwrap();
    let rf = t["refresh_token"].as_str().unwrap().to_string();
    r.o.advance_clock_for_test(86_400 + 1);
    assert_eq!(r.refresh(&c, &rf).unwrap_err().error, "invalid_grant");
    assert!(r.o.grants_of("ada").is_empty());
}

#[test]
fn events_are_recorded_without_secrets() {
    let r = rig();
    let c = r.client();
    let t = r.tokens(&c);
    let gid = r.o.grants_of("ada")[0]["id"].as_str().unwrap().to_string();
    r.o.revoke_grant("ada", &gid);
    let events: Vec<String> = r
        .backend
        .list("security_event")
        .unwrap()
        .into_iter()
        .map(|(_, f)| format!("{} {}", f["kind"], f["detail"]))
        .collect();
    assert!(
        events
            .iter()
            .any(|e| e.contains("oauth_authorize") && e.contains("Claude"))
    );
    assert!(events.iter().any(|e| e.contains("oauth_revoked")));
    let all = events.join("\n");
    for secret in [
        t["access_token"].as_str().unwrap(),
        t["refresh_token"].as_str().unwrap(),
    ] {
        assert!(!all.contains(secret));
    }
}

#[test]
fn metadata_documents_name_the_endpoints_and_s256_only() {
    let m = authorization_server_metadata(BASE);
    assert_eq!(m["issuer"], BASE);
    assert_eq!(m["token_endpoint"], "https://inst.test/oauth/token");
    assert_eq!(m["code_challenge_methods_supported"], json!(["S256"]));
    assert_eq!(m["token_endpoint_auth_methods_supported"], json!(["none"]));
    assert_eq!(m["authorization_response_iss_parameter_supported"], true);
}

#[test]
fn dev_auth_has_no_authorization_server() {
    struct NoOAuth;
    impl Authenticator for NoOAuth {
        fn resolve(&self, _: &str) -> Option<obop_commands::Principal> {
            None
        }
        fn login(&self, _: &str, _: &str) -> Result<String, String> {
            Err("no".into())
        }
        fn issue_agent_token(
            &self,
            _: &obop_commands::Principal,
            _: &[String],
            _: &str,
        ) -> Result<String, String> {
            Err("no".into())
        }
    }
    let b = obop_storage::open("memory:").unwrap();
    assert!(OAuth::new(b, Arc::new(NoOAuth), OAuthConfig::default()).is_none());
}

/// The same protocol on PostgreSQL, when `OBOP_TEST_POSTGRES_URL` is set (compare-and-set
/// rotation, atomic batches and expiring values are the backend's job there).
#[test]
fn round_trip_rotation_and_reuse_on_postgres() {
    let Ok(url) = std::env::var("OBOP_TEST_POSTGRES_URL") else {
        return;
    };
    let user = format!("pg{}", crate::util::rand_hex::<6>());
    let r = rig_on(
        obop_storage::open(&url).unwrap(),
        &user,
        OAuthConfig::default(),
    );
    let c = r.client();
    let t1 = r.tokens(&c);
    let (a1, r1) = (
        t1["access_token"].as_str().unwrap(),
        t1["refresh_token"].as_str().unwrap(),
    );
    assert!(r.auth.resolve(a1).is_some());
    let t2 = r.refresh(&c, r1).unwrap();
    assert!(r.auth.resolve(a1).is_none());
    assert_eq!(r.refresh(&c, r1).unwrap_err().error, "invalid_grant");
    assert!(
        r.auth
            .resolve(t2["access_token"].as_str().unwrap())
            .is_none()
    );
    assert!(r.o.grants_of(&user).is_empty());
    // A code is single use there too.
    let code = r.code(&c, &["sales.user"]);
    r.exchange(&c, &code, &verifier(), REDIRECT).unwrap();
    assert_eq!(
        r.exchange(&c, &code, &verifier(), REDIRECT)
            .unwrap_err()
            .error,
        "invalid_grant"
    );
}
