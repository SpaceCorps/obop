//! The mobile class (the D64 amendment): a native app registers with a redirect of its own kind,
//! signs in through the system browser, and gets an access token for `/api` that is the person
//! narrowed to the groups they handed over, with a `device` record for the sign-in. The default
//! class is tested in the file next door and is not touched by anything here.

use super::*;
use obop_commands::devices as dev;
use obop_storage::Fields;
use obop_storage::fault;
use obop_storage::testing::{Flaky, Mode};

pub(super) const APP: &str = "org.obop.mileage:/oauth";
pub(super) const LOOP4: &str = "http://127.0.0.1/callback";
pub(super) const DEVICE: [(&str, &str); 3] = [
    ("device_name", "Pixel 8"),
    ("device_platform", "Android"),
    ("device_app_version", "1.4.2"),
];

pub(super) fn api() -> String {
    format!("{BASE}/api")
}

impl Rig {
    pub(super) fn mobile_with(&self, redirects: &[&str]) -> Result<String, OAuthError> {
        self.mobile_declaring(redirects, "base.user sales.user")
    }

    /// A mobile client that declares `scope` (the groups it will ask for) when it registers.
    pub(super) fn mobile_declaring(
        &self,
        redirects: &[&str],
        scope: &str,
    ) -> Result<String, OAuthError> {
        self.o
            .register(
                "10.0.0.1",
                &json!({
                    "client_name": "Mileage",
                    "obop_client_class": "mobile",
                    "redirect_uris": redirects,
                    "scope": scope,
                }),
            )
            .map(|d| d["client_id"].as_str().unwrap().to_string())
    }

    pub(super) fn mobile_client(&self) -> String {
        self.mobile_with(&[APP, LOOP4]).unwrap()
    }

    pub(super) fn mobile_params(&self, client: &str, redirect: &str) -> AuthorizeParams {
        AuthorizeParams {
            resources: vec![api()],
            ..self.params(client, redirect)
        }
    }

    pub(super) fn mobile_code(&self, client: &str, groups: &[&str]) -> String {
        let loc = self.consent(&self.mobile_params(client, APP), groups, 30);
        assert!(loc.starts_with(&format!("{APP}?code=oac_")), "{loc}");
        query(&loc, "code")
    }

    pub(super) fn mobile_token(
        &self,
        client: &str,
        code: &str,
        verifier: &str,
        redirect: &str,
        extra: &[(&str, &str)],
    ) -> Result<J, OAuthError> {
        let mut pairs = vec![
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", client),
            ("code_verifier", verifier),
            ("redirect_uri", redirect),
            ("resource", "https://inst.test/api"),
        ];
        pairs.extend_from_slice(extra);
        self.o.token(BASE, "10.0.0.3", &self.form(&pairs))
    }

    /// A signed-in app: the client and the answer to the token request.
    pub(super) fn mobile_tokens(&self) -> (String, J) {
        let c = self.mobile_client();
        let code = self.mobile_code(&c, &["base.user", "sales.user"]);
        let t = self
            .mobile_token(&c, &code, &verifier(), APP, &DEVICE)
            .unwrap();
        (c, t)
    }

    pub(super) fn devices(&self) -> Vec<(String, Fields)> {
        self.backend.list(dev::MODEL).unwrap()
    }

    pub(super) fn the_device(&self) -> (String, Fields) {
        let mut all = self.devices();
        assert_eq!(all.len(), 1, "{all:?}");
        all.remove(0)
    }

    pub(super) fn events(&self, kind: &str) -> Vec<String> {
        self.backend
            .list("security_event")
            .unwrap()
            .into_iter()
            .filter(|(_, f)| f["kind"] == kind)
            .map(|(_, f)| f["detail"].as_str().unwrap_or("").to_string())
            .collect()
    }
}

pub(super) fn live_mobile_tokens(r: &Rig) -> usize {
    r.backend
        .list("auth_token")
        .unwrap()
        .into_iter()
        .filter(|(_, t)| t.get("mobile") == Some(&json!(true)) && t["revoked"] != true)
        .count()
}

pub(super) fn state_of(r: &Rig, id: &str) -> String {
    r.backend.get(dev::MODEL, id).unwrap().unwrap()["state"]
        .as_str()
        .unwrap()
        .to_string()
}

// ---- registration and redirects ----------------------------------------------------------

#[test]
fn a_mobile_client_registers_only_redirects_that_follow_rfc_8252() {
    // Every try counts against the limits of the registration, so they are lifted for this table.
    let r = rig_with(OAuthConfig {
        register_per_ip_hour: 10_000,
        register_global_hour: 10_000,
        ..OAuthConfig::default()
    });
    for ok in [
        "org.obop.mileage:/oauth",
        "org.obop.mileage:/",
        "com.example.app-2:/cb/done",
        "http://127.0.0.1/cb",
        "http://127.0.0.1:53124/cb",
        "http://[::1]/cb",
        "http://[::1]:8080/cb",
    ] {
        assert!(r.mobile_with(&[ok]).is_ok(), "{ok}");
    }
    for bad in [
        // A web address of any kind: a mobile client takes no https redirect (the review of the
        // D64 amendment: a self-registered client with a web redirect is a phishing page with
        // our consent screen in front of it; verified app links are a later item, ISSUES).
        "https://app.example.test/mileage/callback",
        "https://app.example.test/",
        "https://example.com/cb",
        "HTTPS://app.example.test/cb",
        // Not a scheme of the app's own: a web address, a name that can be pointed elsewhere,
        // or a scheme anybody can claim because it has no reverse domain form.
        "http://example.com/cb",
        "http://localhost:3000/cb",
        "http://localhost/cb",
        "https://localhost/cb",
        "https://127.0.0.1/cb",
        "http://0.0.0.0/cb",
        "claude://callback",
        "mileage:/oauth",
        "javascript:alert(1)",
        "data:text/html,x",
        "file:///etc/passwd",
        "ftp://example.com/cb",
        // The shape of the private-use URI.
        "org.obop.mileage://host/oauth",
        "org.obop.mileage:oauth",
        "org.obop.mileage:",
        "org.obop.mileage:/oauth#frag",
        "org.obop.mileage:/oauth?x=1",
        "org.obop.mileage:/o*",
        "org.obop.mileage:/o auth",
        "org.obop.mileage:/o\\auth",
        "Org.Obop.Mileage:/oauth",
        "org..obop:/oauth",
        "-org.obop:/oauth",
        "org.obop-:/oauth",
        "1org.obop:/oauth",
        // What no redirect may be.
        "https://*.example.test/cb",
        "https://user@example.test/cb",
        "https://example.test/cb#frag",
        "//evil.test/cb",
        "/relative",
        "",
    ] {
        let e = r.mobile_with(&[bad]).unwrap_err();
        assert_eq!(
            (e.status, e.error),
            (400, "invalid_redirect_uri"),
            "{bad:?} must be refused"
        );
    }
    // A private-use scheme is no redirect for a client of the default class, which is unchanged:
    // it still takes `localhost`, which a mobile client does not.
    let mcp = |u: &str| {
        r.o.register(
            "10.0.0.9",
            &json!({"client_name": "x", "redirect_uris": [u]}),
        )
    };
    assert_eq!(mcp(APP).unwrap_err().error, "invalid_redirect_uri");
    assert!(mcp("http://localhost:3000/cb").is_ok());
    // The class itself is checked.
    for bad in [
        json!("phone"),
        json!(true),
        json!(["mobile"]),
        json!(""),
        json!(null),
    ] {
        let e =
            r.o.register(
                "10.0.0.8",
                &json!({"redirect_uris": [APP], "obop_client_class": bad}),
            )
            .unwrap_err();
        assert_eq!(e.error, "invalid_client_metadata", "{bad}");
    }
}

#[test]
fn the_class_is_written_only_when_it_is_not_the_default() {
    let r = rig();
    let plain = r.client();
    let rec = r.backend.get("oauth_client", &plain).unwrap().unwrap();
    assert!(
        !rec.contains_key("class"),
        "an MCP client record is what it was: {rec:?}"
    );
    let doc =
        r.o.register("10.0.0.2", &json!({"redirect_uris": [REDIRECT]}))
            .unwrap();
    assert!(doc.get("obop_client_class").is_none());
    let m = r.mobile_client();
    assert_eq!(
        r.backend.get("oauth_client", &m).unwrap().unwrap()["class"],
        "mobile"
    );
    assert!(r.o.client(&m).unwrap().class.is_mobile());
    assert_eq!(r.o.client(&plain).unwrap().class, Class::Mcp);
    // An explicit "mcp" is the default.
    let explicit =
        r.o.register(
            "10.0.0.3",
            &json!({"redirect_uris": [REDIRECT], "obop_client_class": "mcp"}),
        )
        .unwrap();
    let id = explicit["client_id"].as_str().unwrap();
    assert!(
        !r.backend
            .get("oauth_client", id)
            .unwrap()
            .unwrap()
            .contains_key("class")
    );
    // Registering a mobile client is limited like every registration.
    let tight = rig_with(OAuthConfig {
        register_per_ip_hour: 2,
        ..OAuthConfig::default()
    });
    assert!(tight.mobile_with(&[APP]).is_ok());
    assert!(tight.mobile_with(&[APP]).is_ok());
    assert_eq!(tight.mobile_with(&[APP]).unwrap_err().status, 429);
}

#[test]
fn the_presented_redirect_must_be_the_registered_one() {
    let r = rig();
    let c = r.mobile_client();
    let ok = |u: &str| r.o.validate_authorize(BASE, &r.mobile_params(&c, u));
    for same in [APP, LOOP4] {
        assert!(ok(same).is_ok(), "{same}");
    }
    // The loopback port may change (RFC 8252 7.3), nothing else may.
    assert!(ok("http://127.0.0.1:53124/callback").is_ok());
    for other in [
        "org.obop.mileage:/oauth/",
        "org.obop.mileage:/oauth2",
        "org.obop.mileage:/",
        "org.obop.other:/oauth",
        "org.obop.mileage.evil:/oauth",
        // A web address is never the redirect of a mobile client, whatever was registered.
        "https://app.example.test/mileage/callback",
        "https://app.example.test.evil.test/mileage/callback",
        "http://app.example.test/mileage/callback",
        "http://127.0.0.1:53124/other",
        "http://localhost:53124/callback",
        "http://[::1]:53124/callback",
        "https://127.0.0.1/callback",
    ] {
        assert_eq!(
            r.o.validate_authorize(BASE, &r.mobile_params(&c, other))
                .unwrap_err(),
            AuthzError::Page {
                key: "err.redirect"
            },
            "{other}"
        );
    }
    // Without a redirect_uri the only registered one is used, as for every client.
    let one = r.mobile_with(&[APP]).unwrap();
    let mut p = r.mobile_params(&one, APP);
    p.redirect_uri = None;
    assert_eq!(r.o.validate_authorize(BASE, &p).unwrap().redirect_uri, APP);
    // PKCE stays mandatory and S256 only.
    for (ch, m) in [
        (None, Some("S256")),
        (Some(s256(&verifier())), None),
        (Some(s256(&verifier())), Some("plain")),
    ] {
        let mut p = r.mobile_params(&c, APP);
        p.code_challenge = ch;
        p.code_challenge_method = m.map(String::from);
        assert!(matches!(
            r.o.validate_authorize(BASE, &p),
            Err(AuthzError::Redirect {
                error: "invalid_request",
                ..
            })
        ));
    }
    // A redirect error of the app goes to its own address.
    let mut p = r.mobile_params(&c, APP);
    p.response_type = Some("token".into());
    let Err(AuthzError::Redirect { redirect_uri, .. }) = r.o.validate_authorize(BASE, &p) else {
        panic!("expected a redirect");
    };
    assert_eq!(redirect_uri, APP);
    assert!(
        r.o.error_redirect(BASE, APP, Some("s"), "access_denied", "no")
            .starts_with("org.obop.mileage:/oauth?error=access_denied")
    );
}

#[test]
fn the_resource_of_a_mobile_app_is_the_api_endpoint_and_that_of_an_mcp_client_is_not() {
    let r = rig();
    let m = r.mobile_client();
    let mut p = r.mobile_params(&m, APP);
    p.resources = vec![format!("{BASE}/mcp")];
    assert!(matches!(
        r.o.validate_authorize(BASE, &p),
        Err(AuthzError::Redirect {
            error: "invalid_target",
            ..
        })
    ));
    p.resources.clear();
    assert_eq!(r.o.validate_authorize(BASE, &p).unwrap().resource, api());
    p.resources = vec![format!("{BASE}/api/")];
    assert!(r.o.validate_authorize(BASE, &p).is_ok());
    // And the other way round.
    let c = r.client();
    let mut p = r.params(&c, REDIRECT);
    p.resources = vec![api()];
    assert!(matches!(
        r.o.validate_authorize(BASE, &p),
        Err(AuthzError::Redirect {
            error: "invalid_target",
            ..
        })
    ));
    // At the token endpoint: the wrong resource does not spend the code.
    let code = r.mobile_code(&m, &["base.user"]);
    let e =
        r.o.token(
            BASE,
            "10.0.0.3",
            &r.form(&[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("client_id", &m),
                ("code_verifier", &verifier()),
                ("redirect_uri", APP),
                ("resource", "https://inst.test/mcp"),
            ]),
        )
        .unwrap_err();
    assert_eq!(e.error, "invalid_target");
    assert!(r.mobile_token(&m, &code, &verifier(), APP, &[]).is_ok());
    // The code of an MCP client is not exchanged for the /api resource either.
    let code = r.code(&c, &["sales.user"]);
    let e =
        r.o.token(
            BASE,
            "10.0.0.3",
            &r.form(&[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("client_id", &c),
                ("code_verifier", &verifier()),
                ("redirect_uri", REDIRECT),
                ("resource", "https://inst.test/api"),
            ]),
        )
        .unwrap_err();
    assert_eq!(e.error, "invalid_target");
    assert!(
        r.exchange(&c, &code, &verifier(), REDIRECT).is_ok(),
        "not spent"
    );
    // A token of another base address is no token of this one.
    let code = r.mobile_code(&m, &["base.user"]);
    let e =
        r.o.token(
            "https://other.test",
            "10.0.0.3",
            &r.form(&[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("client_id", &m),
                ("code_verifier", &verifier()),
                ("redirect_uri", APP),
            ]),
        )
        .unwrap_err();
    assert_eq!(e.error, "invalid_target");
}

// ---- the flow --------------------------------------------------------------------------------

#[test]
fn the_whole_mobile_flow_gives_the_person_narrowed_with_an_api_audience_and_a_device() {
    let r = rig();
    let (c, t) = r.mobile_tokens();
    assert_eq!(t["token_type"], "Bearer");
    assert_eq!(t["expires_in"], 3600);
    assert_eq!(t["scope"], "base.user sales.user");
    let access = t["access_token"].as_str().unwrap();
    let (p, audience) = r.auth.resolve_with_audience(access).unwrap();
    assert_eq!(audience.as_deref(), Some("https://inst.test/api"));
    // The person, not an agent: the approval gates that hold an agent do not hold the app.
    assert_eq!(
        (p.id.as_str(), p.kind),
        ("ada", obop_commands::PrincipalKind::User)
    );
    assert!(!p.is_person(), "but it is not the person at the keyboard");
    assert!(p.allows("base.user") && p.allows("sales.user"));
    assert!(
        !p.allows("platform.admin") && !p.allows("crm.user"),
        "only the groups handed over"
    );
    // The device, in the same write as the family.
    let (id, d) = r.the_device();
    assert_eq!(
        (
            d["user"].as_str(),
            d["name"].as_str(),
            d["platform"].as_str(),
            d["app_version"].as_str(),
            d["client_name"].as_str(),
            d["state"].as_str()
        ),
        (
            Some("ada"),
            Some("Pixel 8"),
            Some("android"),
            Some("1.4.2"),
            Some("Mileage"),
            Some("active")
        )
    );
    assert_eq!(d["registered_at"], d["last_seen"]);
    assert!(d.get("push_token").is_none(), "declared and never written");
    let grants = r.backend.list("oauth_grant").unwrap();
    assert_eq!(grants.len(), 1);
    let (gid, g) = &grants[0];
    assert_eq!(g["device"], id.as_str());
    assert_eq!(g["class"], "mobile");
    assert_eq!(g["resource"], api().as_str());
    assert_eq!(d["grant"], gid.as_str());
    // Listed as a device, not as a connected application.
    assert!(r.o.grants_of("ada").is_empty());
    // Nothing secret is stored in the clear.
    let dump = format!(
        "{:?}{:?}{:?}",
        r.backend.list("oauth_grant").unwrap(),
        r.devices(),
        r.backend.list("security_event").unwrap()
    );
    assert!(!dump.contains(access) && !dump.contains(t["refresh_token"].as_str().unwrap()));
    // The sign-in is in the security log, with the device and without any secret.
    let ev = r.events("device_registered");
    assert_eq!(ev.len(), 1, "{ev:?}");
    assert!(ev[0].contains("Pixel 8") && ev[0].contains("android") && ev[0].contains(&id));
    // A refresh keeps the audience, the device and the narrowing.
    let t2 = r.refresh(&c, t["refresh_token"].as_str().unwrap()).unwrap();
    let (p2, audience) = r
        .auth
        .resolve_with_audience(t2["access_token"].as_str().unwrap())
        .unwrap();
    assert_eq!(audience.as_deref(), Some("https://inst.test/api"));
    assert!(p2.allows("base.user") && !p2.allows("crm.user"));
    assert!(
        r.auth.resolve(access).is_none(),
        "the old access token ends with the rotation"
    );
    assert_eq!(r.devices().len(), 1, "a refresh is not a new device");
}

#[test]
fn a_device_without_metadata_gets_plain_defaults_and_an_mcp_client_makes_no_device() {
    let r = rig();
    let c = r.mobile_client();
    let code = r.mobile_code(&c, &["base.user"]);
    r.mobile_token(&c, &code, &verifier(), APP, &[]).unwrap();
    let (_, d) = r.the_device();
    assert_eq!(d["name"], dev::DEFAULT_NAME);
    assert_eq!(d["platform"], dev::DEFAULT_PLATFORM);
    assert_eq!(d["app_version"], "");
    // The default class takes no device and ignores what only a mobile app sends.
    let m = r.client();
    let code = r.code(&m, &["sales.user"]);
    let t =
        r.o.token(
            BASE,
            "10.0.0.3",
            &r.form(&[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("client_id", &m),
                ("code_verifier", &verifier()),
                ("redirect_uri", REDIRECT),
                ("device_name", "ignored\u{0}even though invalid"),
            ]),
        )
        .unwrap();
    assert_eq!(r.devices().len(), 1);
    let (p, audience) = r
        .auth
        .resolve_with_audience(t["access_token"].as_str().unwrap())
        .unwrap();
    assert_eq!(audience.as_deref(), Some("https://inst.test/mcp"));
    assert_eq!(p.kind, obop_commands::PrincipalKind::Agent);
    assert_eq!(r.o.grants_of("ada").len(), 1);
}

#[test]
fn what_an_app_says_about_its_device_is_checked_before_the_code_is_spent() {
    let r = rig();
    let c = r.mobile_client();
    let code = r.mobile_code(&c, &["base.user"]);
    for bad in [
        ("device_name", "bell\u{7}"),
        ("device_name", "two\nlines"),
        ("device_name", "evil\u{202e}name"),
        ("device_name", "x".repeat(81).as_str()),
        ("device_platform", "and roid"),
        ("device_platform", "9ios"),
        ("device_platform", "android;drop table"),
        ("device_platform", "a".repeat(25).as_str()),
        ("device_app_version", "1.0 beta"),
        ("device_app_version", "<script>"),
        ("device_app_version", "1".repeat(33).as_str()),
    ] {
        let e = r
            .mobile_token(&c, &code, &verifier(), APP, &[bad])
            .unwrap_err();
        assert_eq!((e.status, e.error), (400, "invalid_request"), "{bad:?}");
    }
    assert!(r.devices().is_empty());
    // Empty values are no values, and the code was never spent.
    r.mobile_token(
        &c,
        &code,
        &verifier(),
        APP,
        &[
            ("device_name", " "),
            ("device_platform", ""),
            ("device_app_version", ""),
        ],
    )
    .unwrap();
    assert_eq!(r.the_device().1["name"], dev::DEFAULT_NAME);
    // A name keeps its spaces inside, loses them at the ends, and may be long and not English.
    let code = r.mobile_code(&c, &["base.user"]);
    r.mobile_token(
        &c,
        &code,
        &verifier(),
        APP,
        &[("device_name", "  Äli's Telefon \u{1F4F1}  ")],
    )
    .unwrap();
    assert!(
        r.devices()
            .iter()
            .any(|(_, d)| d["name"] == "Äli's Telefon \u{1F4F1}")
    );
}

#[test]
fn a_mobile_app_is_never_offered_or_given_an_administrative_group() {
    let r = rig();
    let c = r.mobile_client();
    // The person also administers devices: the group of an administrator of one area, which a lost
    // phone must not hold either.
    let mut held = Fields::new();
    held.insert(
        "groups".into(),
        json!([
            "base.user",
            "sales.user",
            "platform.admin",
            "api.device_admin"
        ]),
    );
    r.backend.update("auth_user", "ada", &held).unwrap();
    let v =
        r.o.validate_authorize(BASE, &r.mobile_params(&c, APP))
            .unwrap();
    let (id, binder) = r.o.begin(&v, "10.0.0.2", "en").unwrap();
    let csrf = r.o.view(&id, &binder).unwrap().csrf;
    r.o.login(&id, &binder, &csrf, "10.0.0.2", "ada", PW)
        .unwrap();
    let ctx = r.o.view(&id, &binder).unwrap();
    assert!(ctx.class.is_mobile());
    assert_eq!(
        ctx.groups,
        ["base.user", "sales.user"],
        "no platform.admin: {:?}",
        ctx.groups
    );
    assert_eq!(ctx.redirect_host, "org.obop.mileage");
    assert_eq!(ctx.redirect_origin, "org.obop.mileage:");
    let page = crate::page::consent_page(&ctx, None);
    assert!(
        page.csp.contains("form-action 'self' org.obop.mileage:"),
        "{}",
        page.csp
    );
    assert!(page.html.contains("mobile app") && page.html.contains("org.obop.mileage"));
    assert!(page.html.contains("Customize, Security, Devices"));
    assert!(!page.html.contains("platform.admin") && !page.html.contains("device_admin"));
    // A post that names one anyway is refused, and the flow is still open.
    let g = |gs: &[&str]| gs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(
        r.o.decide(
            BASE,
            &id,
            &binder,
            &csrf,
            true,
            &g(&["base.user", "platform.admin"]),
            30
        )
        .unwrap(),
        DecideOut::Retry("err.mobile_admin")
    );
    assert_eq!(
        r.o.decide(
            BASE,
            &id,
            &binder,
            &csrf,
            true,
            &g(&["base.user", "api.device_admin"]),
            30
        )
        .unwrap(),
        DecideOut::Retry("err.mobile_admin")
    );
    assert!(matches!(
        r.o.decide(BASE, &id, &binder, &csrf, true, &g(&["base.user"]), 30)
            .unwrap(),
        DecideOut::Redirect(_)
    ));
    // The page of an MCP client is the one it always was.
    let m = r.client();
    let v =
        r.o.validate_authorize(BASE, &r.params(&m, REDIRECT))
            .unwrap();
    let (id, binder) = r.o.begin(&v, "10.0.0.2", "en").unwrap();
    let csrf = r.o.view(&id, &binder).unwrap().csrf;
    r.o.login(&id, &binder, &csrf, "10.0.0.2", "ada", PW)
        .unwrap();
    let ctx = r.o.view(&id, &binder).unwrap();
    assert!(ctx.groups.contains(&"platform.admin".to_string()));
    assert!(
        !crate::page::consent_page(&ctx, None)
            .html
            .contains("mobile app")
    );
}

#[test]
fn the_owner_losing_a_group_narrows_the_app_and_a_refresh_never_widens_it() {
    let r = rig();
    let (c, t) = r.mobile_tokens();
    let refresh = t["refresh_token"].as_str().unwrap();
    let widen = r.o.token(
        BASE,
        "10.0.0.3",
        &r.form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh),
            ("client_id", &c),
            ("scope", "sales.user crm.user"),
        ]),
    );
    assert_eq!(widen.unwrap_err().error, "invalid_scope");
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
    let access = narrow["access_token"].as_str().unwrap();
    let p = r.auth.resolve(access).unwrap();
    assert!(p.allows("base.user") && !p.allows("sales.user"));
    // The person loses base.user: the token no longer carries it, whatever it was given.
    let set_groups = |g: &[&str]| {
        let mut f = Fields::new();
        f.insert("groups".into(), json!(g));
        r.backend.update("auth_user", "ada", &f).unwrap();
    };
    set_groups(&["sales.user"]);
    let p = r.auth.resolve(access).unwrap();
    assert!(
        !p.allows("base.user"),
        "a group the owner lost is gone from the app"
    );
    // At the refresh the scope is what the owner still holds of what was handed over ...
    let t3 = r
        .refresh(&c, narrow["refresh_token"].as_str().unwrap())
        .unwrap();
    assert_eq!(t3["scope"], "sales.user");
    // ... and when nothing is left, the sign-in ends.
    set_groups(&["crm.user"]);
    let again = r
        .refresh(&c, t3["refresh_token"].as_str().unwrap())
        .unwrap_err();
    assert_eq!(again.error, "invalid_grant", "{again:?}");
}

#[test]
fn a_refresh_marks_the_device_as_seen_and_takes_the_version_of_the_app() {
    let r = rig();
    let (c, t) = r.mobile_tokens();
    let (id, before) = r.the_device();
    r.o.advance_clock_for_test(300);
    let t2 =
        r.o.token(
            BASE,
            "10.0.0.3",
            &r.form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", t["refresh_token"].as_str().unwrap()),
                ("client_id", &c),
                ("device_app_version", "1.5.0"),
                // Name and platform are said once, at sign-in; a refresh does not change them.
                ("device_name", "Not my name"),
            ]),
        )
        .unwrap();
    let after = r.backend.get(dev::MODEL, &id).unwrap().unwrap();
    assert!(after["last_seen"].as_str() > before["last_seen"].as_str());
    assert_eq!(after["registered_at"], before["registered_at"]);
    assert_eq!(after["app_version"], "1.5.0");
    assert_eq!(after["name"], "Pixel 8");
    // A version that is refused is refused before anything is rotated.
    let e =
        r.o.token(
            BASE,
            "10.0.0.3",
            &r.form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", t2["refresh_token"].as_str().unwrap()),
                ("client_id", &c),
                ("device_app_version", "bad version"),
            ]),
        )
        .unwrap_err();
    assert_eq!(e.error, "invalid_request");
    r.o.advance_clock_for_test(60);
    let t3 = r
        .refresh(&c, t2["refresh_token"].as_str().unwrap())
        .unwrap();
    let seen = r.backend.get(dev::MODEL, &id).unwrap().unwrap()["last_seen"].clone();
    assert_ne!(seen, after["last_seen"], "that refresh moved it");
    // An API call does not write a record: the device is read, not changed.
    let access = t3["access_token"].as_str().unwrap();
    r.o.advance_clock_for_test(60);
    for _ in 0..3 {
        assert!(r.auth.resolve(access).is_some());
    }
    let later = r.backend.get(dev::MODEL, &id).unwrap().unwrap()["last_seen"].clone();
    assert_eq!(later, seen, "using the access token writes nothing");
}

#[test]
fn reuse_of_a_refresh_token_ends_the_family_and_marks_the_device() {
    let r = rig();
    let (c, t) = r.mobile_tokens();
    let (id, _) = r.the_device();
    let (a1, r1) = (
        t["access_token"].as_str().unwrap(),
        t["refresh_token"].as_str().unwrap(),
    );
    let t2 = r.refresh(&c, r1).unwrap();
    let a2 = t2["access_token"].as_str().unwrap();
    assert_eq!(state_of(&r, &id), "active");
    assert!(r.auth.resolve(a1).is_none() && r.auth.resolve(a2).is_some());
    // The first token is presented again: both parties hold it.
    assert_eq!(r.refresh(&c, r1).unwrap_err().error, "invalid_grant");
    assert_eq!(state_of(&r, &id), "revoked");
    assert!(
        r.auth.resolve(a2).is_none(),
        "the access token in circulation is dead"
    );
    assert_eq!(
        r.refresh(&c, t2["refresh_token"].as_str().unwrap())
            .unwrap_err()
            .error,
        "invalid_grant"
    );
    let rec = r.backend.get(dev::MODEL, &id).unwrap().unwrap();
    assert!(rec["revoked_at"].as_str().is_some());
    assert!(r.events("oauth_refresh_reuse").len() == 1 && r.events("device_revoked").len() == 1);
    assert!(r.events("device_revoked")[0].contains("Pixel 8"));
}

#[test]
fn a_bad_verifier_a_replayed_code_and_an_expired_code_leave_no_device_or_end_the_family() {
    let r = rig();
    let c = r.mobile_client();
    let code = r.mobile_code(&c, &["base.user"]);
    let e = r
        .mobile_token(&c, &code, &"w".repeat(50), APP, &DEVICE)
        .unwrap_err();
    assert_eq!(e.error, "invalid_grant");
    assert!(
        r.devices().is_empty(),
        "no device for a request that failed"
    );
    assert_eq!(
        r.mobile_token(&c, &code, &verifier(), APP, &DEVICE)
            .unwrap_err()
            .error,
        "invalid_grant",
        "a failed attempt burns the code"
    );
    // The code is bound to the redirect URI of the request.
    let code = r.mobile_code(&c, &["base.user"]);
    let e = r
        .mobile_token(&c, &code, &verifier(), LOOP4, &DEVICE)
        .unwrap_err();
    assert_eq!(e.error, "invalid_grant");
    // Expired: sixty seconds.
    let code = r.mobile_code(&c, &["base.user"]);
    r.o.advance_clock_for_test(61);
    assert_eq!(
        r.mobile_token(&c, &code, &verifier(), APP, &DEVICE)
            .unwrap_err()
            .error,
        "invalid_grant"
    );
    assert!(r.devices().is_empty());
    // Replay of a code that was exchanged ends what it produced, device included.
    let code = r.mobile_code(&c, &["base.user"]);
    let t = r
        .mobile_token(&c, &code, &verifier(), APP, &DEVICE)
        .unwrap();
    let access = t["access_token"].as_str().unwrap();
    assert!(r.auth.resolve(access).is_some());
    let (id, _) = r.the_device();
    assert_eq!(
        r.mobile_token(&c, &code, &verifier(), APP, &DEVICE)
            .unwrap_err()
            .error,
        "invalid_grant"
    );
    assert!(r.auth.resolve(access).is_none());
    assert_eq!(state_of(&r, &id), "revoked");
    assert_eq!(r.events("oauth_code_reuse").len(), 1);
    // Another client cannot use the code of this one.
    let other = r.mobile_client();
    let code = r.mobile_code(&c, &["base.user"]);
    assert_eq!(
        r.mobile_token(&other, &code, &verifier(), APP, &DEVICE)
            .unwrap_err()
            .error,
        "invalid_grant"
    );
}

#[test]
fn revoking_a_device_ends_the_family_and_the_access_token_and_says_so_once() {
    let r = rig();
    let (c, t) = r.mobile_tokens();
    let (id, _) = r.the_device();
    let (access, refresh) = (
        t["access_token"].as_str().unwrap(),
        t["refresh_token"].as_str().unwrap(),
    );
    assert!(r.auth.resolve(access).is_some());
    r.o.revoke_device(&id, "ada", None).unwrap();
    assert!(
        r.auth.resolve(access).is_none(),
        "the access token stops at once"
    );
    assert_eq!(r.refresh(&c, refresh).unwrap_err().error, "invalid_grant");
    assert_eq!(state_of(&r, &id), "revoked");
    let rec = r.backend.get(dev::MODEL, &id).unwrap().unwrap();
    assert!(rec["revoked_at"].as_str().unwrap().ends_with('Z'));
    let ev = r.events("device_revoked");
    assert_eq!(ev.len(), 1, "{ev:?}");
    assert!(ev[0].contains("revoked by ada") && !ev[0].contains(access));
    // Asking again changes nothing and says nothing more.
    r.o.revoke_device(&id, "ada", None).unwrap();
    assert_eq!(r.events("device_revoked").len(), 1);
    assert_eq!(
        r.o.revoke_device("no-such-device", "ada", None)
            .unwrap_err()
            .status,
        404
    );
    // A new sign-in of the same app is a new device.
    let code = r.mobile_code(&c, &["base.user"]);
    r.mobile_token(&c, &code, &verifier(), APP, &DEVICE)
        .unwrap();
    assert_eq!(r.devices().len(), 2);
    // The application revoking its own token (RFC 7009) ends its device too.
    let t = {
        let code = r.mobile_code(&c, &["base.user"]);
        r.mobile_token(&c, &code, &verifier(), APP, &DEVICE)
            .unwrap()
    };
    r.o.revoke_presented(
        "10.0.0.4",
        &r.form(&[
            ("token", t["refresh_token"].as_str().unwrap()),
            ("client_id", &c),
        ]),
    )
    .unwrap();
    assert_eq!(
        r.devices()
            .iter()
            .filter(|(_, d)| d["state"] == "active")
            .count(),
        1,
        "two of the three devices are ended"
    );
    // The person ending the grant from the connected apps route ends the device as well.
    let (gid, _) = r
        .backend
        .list("oauth_grant")
        .unwrap()
        .into_iter()
        .find(|(_, g)| g["revoked"] != true)
        .unwrap();
    assert!(r.o.revoke_grant("ada", &gid).unwrap());
    assert!(r.devices().iter().all(|(_, d)| d["state"] == "revoked"));
}

#[test]
fn a_device_that_is_not_in_force_is_no_principal_and_an_unreadable_one_is_none_either() {
    let (flaky, backend) = Flaky::memory();
    let r = rig_on(backend, "ada", OAuthConfig::default());
    let (c, t) = r.mobile_tokens();
    let access = t["access_token"].as_str().unwrap();
    assert!(r.auth.resolve(access).is_some());
    // The store refuses the read of the device: fail closed, never "no revocation, so allowed".
    fault::begin_request(Default::default());
    flaky.refuse_model(dev::MODEL, Mode::Reads);
    assert!(r.auth.resolve(access).is_none());
    assert!(r.auth.resolve_with_audience(access).is_none());
    let out = fault::end_request();
    assert_eq!(
        out.fault,
        Some(fault::Fault::Busy),
        "so the request is answered 503"
    );
    flaky.allow_models();
    assert!(
        r.auth.resolve(access).is_some(),
        "and it works again once the store answers"
    );
    // The record says revoked although the token's own record was not touched: still no principal.
    let (id, _) = r.the_device();
    let mut patch = Fields::new();
    patch.insert("state".into(), json!("revoked"));
    r.backend.update(dev::MODEL, &id, &patch).unwrap();
    assert!(r.auth.resolve(access).is_none());
    // A record that is gone is the same.
    r.backend.delete(dev::MODEL, &id).unwrap();
    assert!(r.auth.resolve(access).is_none());
    assert_eq!(
        r.refresh(&c, t["refresh_token"].as_str().unwrap())
            .unwrap_err()
            .error,
        "invalid_grant"
    );
    assert_eq!(
        live_mobile_tokens(&r),
        0,
        "and a refresh ends the family of a device that is gone"
    );
}

#[test]
fn a_family_whose_grant_ended_by_date_is_forgotten_with_its_device_a_week_later() {
    let r = rig();
    let c = r.mobile_client();
    let loc = r.consent(&r.mobile_params(&c, APP), &["base.user"], 1);
    r.mobile_token(&c, &query(&loc, "code"), &verifier(), APP, &DEVICE)
        .unwrap();
    assert_eq!(r.devices().len(), 1);
    r.o.advance_clock_for_test(86_400 + 1);
    r.o.purge_grants();
    assert_eq!(
        r.devices().len(),
        1,
        "a day after it ended it is still listed"
    );
    r.o.advance_clock_for_test(7 * 86_400);
    r.o.purge_grants();
    assert!(r.devices().is_empty() && r.backend.list("oauth_grant").unwrap().is_empty());
}

// ---- the store refuses (D74) ---------------------------------------------------------------

#[test]
fn a_mobile_code_exchange_refused_at_any_call_is_retryable_and_leaves_no_device_or_token() {
    // Wherever in the exchange the store starts refusing, the client is told to retry, the code is
    // still good, and neither a device, a grant nor a live token is left behind by the refusal.
    let (flaky, backend) = Flaky::memory();
    let r = rig_on(
        backend,
        "ada",
        OAuthConfig {
            max_devices_per_user: 1000,
            ..OAuthConfig::default()
        },
    );
    let c = r.mobile_client();
    let mut refused_somewhere = 0;
    for k in 1..=16 {
        let code = r.mobile_code(&c, &["base.user"]);
        let (devices, grants, tokens) = (
            r.devices().len(),
            r.backend.list("oauth_grant").unwrap().len(),
            live_mobile_tokens(&r),
        );
        fault::begin_request(Default::default());
        flaky.refuse_n_after_the_next_write(k);
        let first = r.mobile_token(&c, &code, &verifier(), APP, &DEVICE);
        let out = fault::end_request();
        flaky.allow_models();
        if out.fault.is_none() {
            assert!(first.is_ok(), "k={k}: {first:?}");
            assert_eq!(r.devices().len(), devices + 1, "k={k}");
            continue;
        }
        refused_somewhere += 1;
        match first {
            Err(e) => {
                assert_eq!(
                    (e.status, e.error),
                    (503, "temporarily_unavailable"),
                    "k={k}"
                );
                assert_eq!(
                    (
                        r.devices().len(),
                        r.backend.list("oauth_grant").unwrap().len(),
                        live_mobile_tokens(&r)
                    ),
                    (devices, grants, tokens),
                    "k={k}: a refused exchange leaves nothing behind"
                );
                let again = r.mobile_token(&c, &code, &verifier(), APP, &DEVICE);
                assert!(
                    again.is_ok(),
                    "k={k}: the retry of the same code: {again:?}"
                );
                assert_eq!(
                    r.devices().len(),
                    devices + 1,
                    "k={k}: one device, no orphan"
                );
                assert_eq!(live_mobile_tokens(&r), tokens + 1, "k={k}");
            }
            // The refusal hit a step the exchange does not need and it kept its answer.
            Ok(_) => assert_eq!(r.devices().len(), devices + 1, "k={k}"),
        }
    }
    assert!(refused_somewhere >= 3, "the sweep reached the exchange");

    // And one refused call at every position of the exchange in turn, up to the first that is past
    // its last call: the whole exchange is covered, not a floor of it.
    let mut past_the_end = false;
    for skip in 0..80 {
        let code = r.mobile_code(&c, &["base.user"]);
        let (devices, grants, tokens) = (
            r.devices().len(),
            r.backend.list("oauth_grant").unwrap().len(),
            live_mobile_tokens(&r),
        );
        fault::begin_request(Default::default());
        flaky.refuse_call_after(skip);
        let first = r.mobile_token(&c, &code, &verifier(), APP, &DEVICE);
        let out = fault::end_request();
        flaky.allow_models();
        match first {
            Ok(_) => {
                assert_eq!(r.devices().len(), devices + 1, "skip={skip}");
                if out.fault.is_none() {
                    past_the_end = true;
                    break;
                }
            }
            Err(e) => {
                assert_eq!(
                    (e.status, e.error),
                    (503, "temporarily_unavailable"),
                    "skip={skip}"
                );
                assert_eq!(
                    (
                        r.devices().len(),
                        r.backend.list("oauth_grant").unwrap().len(),
                        live_mobile_tokens(&r)
                    ),
                    (devices, grants, tokens),
                    "skip={skip}: a refused exchange leaves nothing behind"
                );
                r.mobile_token(&c, &code, &verifier(), APP, &DEVICE)
                    .unwrap_or_else(|e| panic!("skip={skip}: the retry: {e:?}"));
                assert_eq!(r.devices().len(), devices + 1, "skip={skip}");
            }
        }
    }
    assert!(
        past_the_end,
        "the last position is past the final call of the exchange"
    );
}

#[test]
fn a_mobile_refresh_refused_at_any_call_keeps_the_old_token_and_makes_no_orphan() {
    let (flaky, backend) = Flaky::memory();
    let r = rig_on(backend, "ada", OAuthConfig::default());
    let (c, t) = r.mobile_tokens();
    let mut refresh = t["refresh_token"].as_str().unwrap().to_string();
    let mut reached = 0;
    let mut past_the_end = false;
    for skip in 0..24 {
        let tokens = live_mobile_tokens(&r);
        fault::begin_request(Default::default());
        flaky.refuse_call_after(skip);
        let first = r.refresh(&c, &refresh);
        let out = fault::end_request();
        flaky.allow_models();
        match first {
            Ok(t2) => {
                past_the_end |= out.fault.is_none();
                refresh = t2["refresh_token"].as_str().unwrap().to_string();
                assert_eq!(
                    live_mobile_tokens(&r),
                    1,
                    "skip={skip}: the old access token is replaced"
                );
            }
            Err(e) => {
                reached += 1;
                assert_eq!(
                    (e.status, e.error),
                    (503, "temporarily_unavailable"),
                    "skip={skip}: {e:?}"
                );
                assert!(out.fault.is_some(), "skip={skip}");
                assert_eq!(
                    live_mobile_tokens(&r),
                    tokens,
                    "skip={skip}: no orphan access token"
                );
                // Nothing was used up: the same refresh token works.
                let t2 = r
                    .refresh(&c, &refresh)
                    .unwrap_or_else(|e| panic!("skip={skip}: {e:?}"));
                refresh = t2["refresh_token"].as_str().unwrap().to_string();
            }
        }
        assert_eq!(r.devices().len(), 1);
        assert_eq!(state_of(&r, &r.the_device().0), "active", "skip={skip}");
    }
    assert!(reached >= 3, "{reached}");
    assert!(
        past_the_end,
        "the last positions are past the final call of the refresh: the sweep covered all of it"
    );
}

#[test]
fn ending_a_device_refused_at_any_call_is_never_reported_as_done_and_asking_again_finishes_it() {
    // A refusal of one call at every position, and a store that stays refused for longer than a
    // step is tried again from a few positions.
    let sweep = (0..60)
        .map(|k| (k, 1))
        .chain([1, 3, 5, 8, 12, 16].map(|k| (k, 6)));
    let mut refused = 0;
    let mut past_the_end = false;
    for (skip, stays) in sweep {
        let (flaky, backend) = Flaky::memory();
        let r = rig_on(backend, "ada", OAuthConfig::default());
        let (c, t) = r.mobile_tokens();
        let (id, _) = r.the_device();
        let (access, refresh) = (
            t["access_token"].as_str().unwrap().to_string(),
            t["refresh_token"].as_str().unwrap().to_string(),
        );
        fault::begin_request(Default::default());
        // Refused from this position on, for longer than a step is tried again.
        flaky.refuse_calls_after(skip, stays);
        let done = r.o.revoke_device(&id, "ada", None);
        let out = fault::end_request();
        flaky.allow_models();
        match done {
            Ok(()) => {
                past_the_end |= stays == 1 && out.fault.is_none();
                assert!(
                    r.auth.resolve(&access).is_none(),
                    "skip={skip}: said done, so it is"
                );
                assert_eq!(state_of(&r, &id), "revoked", "skip={skip}");
                assert_eq!(r.refresh(&c, &refresh).unwrap_err().error, "invalid_grant");
            }
            Err(e) => {
                refused += 1;
                assert_eq!(
                    (e.status, e.error),
                    (503, "temporarily_unavailable"),
                    "skip={skip}"
                );
                assert_eq!(e.retry_after, Some(1), "a 503 always says when to retry");
                assert!(out.fault.is_some(), "skip={skip}");
                r.o.revoke_device(&id, "ada", None)
                    .unwrap_or_else(|e| panic!("skip={skip}: {e:?}"));
            }
        }
        // After the repeat everything is ended, and nothing is left alive.
        assert!(r.auth.resolve(&access).is_none(), "skip={skip}");
        assert_eq!(state_of(&r, &id), "revoked", "skip={skip}");
        assert_eq!(live_mobile_tokens(&r), 0, "skip={skip}");
        assert_eq!(
            r.refresh(&c, &refresh).unwrap_err().error,
            "invalid_grant",
            "skip={skip}"
        );
        assert!(
            r.events("device_revoked").len() <= 1,
            "skip={skip}: one entry at most, however many tries"
        );
    }
    assert!(refused >= 3, "the sweep reached the revocation: {refused}");
    assert!(
        past_the_end,
        "the last positions are past the final call of the revocation: the sweep covered all of it"
    );
}

#[test]
fn a_refused_write_of_the_device_still_ends_the_token_and_a_refused_token_still_ends_the_device() {
    // The two halves of ending a device go on independently: the access token stops if either
    // goes through, and the caller is told when one did not.
    let (flaky, backend) = Flaky::memory();
    let r = rig_on(backend, "ada", OAuthConfig::default());
    let (_, t) = r.mobile_tokens();
    let (id, _) = r.the_device();
    let access = t["access_token"].as_str().unwrap();
    fault::begin_request(Default::default());
    flaky.refuse_model(dev::MODEL, Mode::Updates);
    let e = r.o.revoke_device(&id, "ada", None).unwrap_err();
    fault::end_request();
    flaky.allow_models();
    assert_eq!(e.status, 503);
    assert!(
        r.auth.resolve(access).is_none(),
        "the token record was revoked"
    );
    assert_eq!(
        state_of(&r, &id),
        "active",
        "the record is still to be marked"
    );
    r.o.revoke_device(&id, "ada", None).unwrap();
    assert_eq!(state_of(&r, &id), "revoked");

    let (flaky, backend) = Flaky::memory();
    let r = rig_on(backend, "ada", OAuthConfig::default());
    let (_, t) = r.mobile_tokens();
    let (id, _) = r.the_device();
    let access = t["access_token"].as_str().unwrap();
    fault::begin_request(Default::default());
    flaky.refuse_model("auth_token", Mode::Updates);
    let e = r.o.revoke_device(&id, "ada", None).unwrap_err();
    fault::end_request();
    flaky.allow_models();
    assert_eq!(e.status, 503);
    assert_eq!(state_of(&r, &id), "revoked");
    assert!(
        r.auth.resolve(access).is_none(),
        "the device is the kill switch when the token record cannot be written"
    );
    r.o.revoke_device(&id, "ada", None).unwrap();
    assert_eq!(live_mobile_tokens(&r), 0);
}

#[test]
fn a_refresh_that_meets_a_revoked_device_ends_the_family_and_is_final() {
    let r = rig();
    let (c, t) = r.mobile_tokens();
    let (id, _) = r.the_device();
    // A device marked revoked by something that did not end its grant (a restore, a hand edit).
    let mut patch = Fields::new();
    patch.insert("state".into(), json!("revoked"));
    r.backend.update(dev::MODEL, &id, &patch).unwrap();
    assert_eq!(
        r.refresh(&c, t["refresh_token"].as_str().unwrap())
            .unwrap_err()
            .error,
        "invalid_grant"
    );
    let (_, g) = r.backend.list("oauth_grant").unwrap().remove(0);
    assert_eq!(g["revoked"], true, "the family is ended with the device");
}

/// The same flow on PostgreSQL, when `OBOP_TEST_POSTGRES_URL` is set: the batch that writes the
/// device with its family, the compare-and-set of a rotation that also moves `last_seen`, and the
/// revocation.
#[test]
fn the_mobile_flow_on_postgres() {
    let Ok(url) = std::env::var("OBOP_TEST_POSTGRES_URL") else {
        return;
    };
    let user = format!("pgm{}", crate::util::rand_hex::<6>());
    let r = rig_on(
        obop_storage::open(&url).unwrap(),
        &user,
        OAuthConfig::default(),
    );
    // The limits of the registration live in the shared database: an address of its own.
    let ip = format!("10.8.{}.{}", rand_byte(), rand_byte());
    let c = r
        .o
        .register(
            &ip,
            &json!({"client_name": "Mileage", "obop_client_class": "mobile", "redirect_uris": [APP], "scope": "base.user"}),
        )
        .unwrap()["client_id"]
        .as_str()
        .unwrap()
        .to_string();
    let loc = r.consent(&r.mobile_params(&c, APP), &["base.user"], 30);
    let t = r
        .mobile_token(&c, &query(&loc, "code"), &verifier(), APP, &DEVICE)
        .unwrap();
    let (a1, r1) = (
        t["access_token"].as_str().unwrap(),
        t["refresh_token"].as_str().unwrap(),
    );
    let p = r.auth.resolve(a1).unwrap();
    assert_eq!(p.id, user);
    let mine = |r: &Rig| {
        r.backend
            .find(dev::MODEL, "user", &J::from(user.as_str()))
            .unwrap()
    };
    let devices = mine(&r);
    assert_eq!(devices.len(), 1);
    let id = devices[0].0.clone();
    let t2 = r.refresh(&c, r1).unwrap();
    assert!(r.auth.resolve(a1).is_none());
    assert_eq!(r.refresh(&c, r1).unwrap_err().error, "invalid_grant");
    assert_eq!(state_of(&r, &id), "revoked");
    assert!(
        r.auth
            .resolve(t2["access_token"].as_str().unwrap())
            .is_none()
    );
    // A second sign-in, ended by revocation.
    let loc = r.consent(&r.mobile_params(&c, APP), &["base.user"], 30);
    let t = r
        .mobile_token(&c, &query(&loc, "code"), &verifier(), APP, &DEVICE)
        .unwrap();
    let second = mine(&r)
        .into_iter()
        .find(|(i, _)| *i != id)
        .map(|(i, _)| i)
        .unwrap();
    r.o.revoke_device(&second, &user, None).unwrap();
    assert!(
        r.auth
            .resolve(t["access_token"].as_str().unwrap())
            .is_none()
    );
    assert_eq!(state_of(&r, &second), "revoked");
}

#[test]
fn the_page_texts_of_the_mobile_class_exist_in_every_language() {
    for key in [
        "consent.who_body_mobile",
        "consent.who_body_mobile_loopback",
        "consent.f1_mobile",
        "consent.f2_mobile",
        "consent.f3_mobile",
        "consent.mobile_none",
        "err.mobile_admin",
        "err.mobile_scope",
        "err.devices_full",
    ] {
        for l in crate::page::LANGS {
            assert!(!crate::page::t(l, key).is_empty(), "{key} in {l}");
        }
    }
}

fn rand_byte() -> u32 {
    u32::from_str_radix(&crate::util::rand_hex::<1>(), 16).unwrap()
}
