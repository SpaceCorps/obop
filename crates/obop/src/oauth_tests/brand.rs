//! The authorization flow over HTTP with a brand (docs/spec/branding.md, slice S1c): the real router,
//! the real handlers and the real `Auth`, a brand written the way `instance_brand.configure` writes
//! it, and for the part of D74 a `Flaky` store under the router. The pages themselves, in every
//! language and every kind, are held by `obop-oauth` (`brand_page_tests.rs`); what is tested here is
//! that the handlers hand them the brand, once, first, and what a request does when that first read
//! fails.

use super::*;
use crate::app::App;
use crate::brand_row_tests::{FIELDS, set_brand};
use obop_auth::{Auth, AuthConfig};
use obop_commands::brand::{self, Action};
use obop_commands::{Authenticator, Store};
use obop_oauth::page::{LANGS, esc, t};
use obop_storage::testing::{Flaky, Mode};

struct Rig {
    r: axum::Router,
    auth: Arc<Auth>,
    app: App,
}

/// A real app with people in it. The address of a request is the one its `X-Forwarded-For` names
/// (`fresh_address`), so the walks below do not have to fit the per-address budget of the sign-in
/// pages.
fn rig_on(app: App) -> Rig {
    app.seed_demo();
    let auth = Arc::new(Auth::with_test_config(
        app.store.backend(),
        AuthConfig {
            default_require_admins: false,
            ..AuthConfig::insecure_dev()
        },
    ));
    app.register_accounts(auth.clone() as Arc<dyn Authenticator>);
    for (name, groups) in [
        ("ada", vec!["base.user", "sales.user"]),
        ("bob", vec!["base.user"]),
    ] {
        let groups: Vec<String> = groups.into_iter().map(String::from).collect();
        auth.create_user(name, PW, &groups).unwrap();
    }
    let state = obop_http::HttpState::new(app.dispatcher.clone(), auth.clone());
    state.set_trust_proxy(true);
    Rig {
        r: obop_http::router(state, None),
        auth,
        app,
    }
}

fn rig() -> Rig {
    rig_on(App::new())
}

fn flaky_rig() -> (Arc<Flaky>, Rig) {
    let (flaky, backend) = Flaky::memory();
    (flaky, rig_on(App::with_store(Store::over(backend))))
}

/// Write the brand row as `instance_brand.configure` does, and make the reader of the app's
/// dispatcher answer it.
fn configure(app: &App, pairs: &[(&str, &str)]) {
    let supplied = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    brand::apply(
        &*app.store.backend(),
        Some(&app.dispatcher.brand()),
        "operator",
        &Action::Configure(supplied),
        false,
    )
    .unwrap_or_else(|e| panic!("the brand was refused: {e}"));
}

fn clear(app: &App) {
    brand::apply(
        &*app.store.backend(),
        Some(&app.dispatcher.brand()),
        "operator",
        &Action::Clear,
        false,
    )
    .unwrap_or_else(|e| panic!("the brand was not cleared: {e}"));
}

/// What a person reads on the page: no script, no style, no tag.
fn visible(html: &str) -> String {
    let mut s = html.to_string();
    for (open, close) in [("<script", "</script>"), ("<style", "</style>")] {
        while let Some(a) = s.find(open) {
            let b = s[a..].find(close).map_or(s.len(), |i| a + i + close.len());
            s.replace_range(a..b, " ");
        }
    }
    let (mut out, mut in_tag) = (String::new(), false);
    for c in s.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' if in_tag => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// The text of `key` as a page writes it, with the application and the host filled in.
fn filled(lang: &str, key: &str, app: &str, host: &str) -> String {
    esc(t(lang, key))
        .replace("{app}", &format!("<strong>{}</strong>", esc(app)))
        .replace("{host}", &format!("<strong>{}</strong>", esc(host)))
}

/// The line above the card, as text: the logo and the name.
fn brand_line(html: &str) -> String {
    let a = html.find("<p class=\"brand\">").expect("a brand line");
    let end = html[a..].find("</p>").unwrap() + a;
    html[a..end].to_string()
}

fn policy(resp: &Resp) -> String {
    resp.header("content-security-policy").to_string()
}

/// A client of the MCP class, registered through the real registration under `name`.
async fn client_named(r: &axum::Router, name: &str) -> String {
    let ip = fresh_address();
    let c = call(
        r,
        "POST",
        "/oauth/register",
        &[("x-forwarded-for", &ip)],
        Some((
            "application/json",
            json!({"client_name": name, "redirect_uris": [REDIRECT],
                   "token_endpoint_auth_method": "none"})
            .to_string(),
        )),
    )
    .await;
    assert_eq!(c.status, 201, "{}", c.text());
    c.json()["client_id"].as_str().unwrap().to_string()
}

/// The consent page of `client`, in `lang`, for a person who signs in as `ada`.
async fn consent_of(r: &axum::Router, client: &str, lang: &str) -> Resp {
    let (_, b) = start(r, &authorize_url(client, REDIRECT, &[("lang", lang)])).await;
    let resp = b
        .post(
            r,
            "/oauth/authorize/login",
            &[("username", "ada"), ("password", PW)],
        )
        .await;
    assert_eq!(resp.status, 200, "{}", resp.text());
    resp
}

// ---- The pages of the flow with a brand -------------------------------------------------------------

/// The sign-in page and the consent page of a real app, in seven languages: before the brand is set
/// they name the product; with it they show the name and the logo, the tab icon and a policy that
/// allows it, and nothing names the product; once it is cleared they are the pages they were, byte
/// for byte.
#[tokio::test]
async fn with_a_brand_row_the_pages_of_the_flow_show_it_and_clear_restores_them() {
    let acc = crate::accounts_tests::rig();
    let r = &acc.rig.r;
    let was = flow_pages(r).await;
    assert_eq!(was.len(), 14);
    assert!(
        was.iter().all(|(_, html)| html.contains("obop")),
        "the pages are the ones that name the product today"
    );
    assert!(was.iter().all(|(_, html)| !html.contains("MOHV")));

    set_brand(&acc.app);
    let now = flow_pages(r).await;
    for (name, html) in &now {
        let text = visible(html);
        assert!(
            !text.to_lowercase().contains("obop"),
            "{name} names the product: {text}"
        );
        assert!(
            !html.to_lowercase().contains("obop"),
            "{name} holds obop outside the text"
        );
        let line = brand_line(html);
        assert!(
            line.contains("<svg") && line.contains("MOHV"),
            "{name}: {line}"
        );
        assert!(
            html.contains("<link rel=\"icon\" href=\"/api/brand/logo.svg?v="),
            "{name}"
        );
    }
    // The title of the language, the message of the language, and the fallbacks.
    let page = |name: &str| now.iter().find(|(n, _)| n == name).unwrap().1.clone();
    assert!(brand_line(&page("sv/sign_in")).ends_with("MOHV Körjournal"));
    assert!(page("sv/sign_in").contains("<p class=\"muted\">Välkommen till laget</p>"));
    assert!(brand_line(&page("de/sign_in")).ends_with("MOHV Fahrtenbuch"));
    assert!(page("de/sign_in").contains("<p class=\"muted\">Welcome to the team</p>"));
    assert!(brand_line(&page("fr/sign_in")).ends_with("MOHV Mileage"));
    assert!(page("fi/sign_in").contains("<p class=\"muted\">Tervetuloa joukkoon</p>"));
    // The consent page names the application twice in the claim, in every language.
    for lang in LANGS {
        let consent = page(&format!("{lang}/consent"));
        assert!(
            consent.contains(&filled(
                lang,
                "consent.who_body.neutral",
                "Claude",
                "claude.ai"
            )),
            "{lang}"
        );
        assert!(
            consent.contains(&esc(t(lang, "consent.f2.neutral"))),
            "{lang}"
        );
    }

    clear(&acc.app);
    let after = flow_pages(r).await;
    assert_eq!(after.len(), was.len());
    for ((name, before), (_, again)) in was.iter().zip(&after) {
        assert!(
            again == before,
            "{name} is not what it was after the brand was cleared"
        );
    }
}

/// The headers of the pages with a brand: the policy is the one it was with `img-src 'self'` added,
/// and the page is as hardened as before.
#[tokio::test]
async fn the_policy_of_a_branded_page_adds_the_image_source_and_nothing_else() {
    let acc = crate::accounts_tests::rig();
    let r = &acc.rig.r;
    let c = client(r).await;
    let url = authorize_url(&c, REDIRECT, &[]);
    let (plain, _) = start(r, &url).await;
    set_brand(&acc.app);
    let (branded, b) = start(r, &url).await;
    assert_eq!(
        policy(&branded),
        format!("{}; img-src 'self'", policy(&plain))
    );
    assert!(!policy(&plain).contains("img-src"));
    for h in [
        "x-frame-options",
        "cache-control",
        "referrer-policy",
        "x-content-type-options",
    ] {
        assert_eq!(branded.header(h), plain.header(h), "{h}");
    }
    assert_eq!(branded.header("cache-control"), "no-store");
    // The consent page too.
    let consent = b
        .post(
            r,
            "/oauth/authorize/login",
            &[("username", "ada"), ("password", PW)],
        )
        .await;
    assert!(
        policy(&consent).ends_with("; img-src 'self'"),
        "{}",
        policy(&consent)
    );
    assert!(policy(&consent).contains("form-action 'self' https://claude.ai"));
    // The icon address answers the logo with its own policy, so the tab icon loads.
    let v = branded.text();
    let at = v.find("/api/brand/logo.svg?v=").unwrap();
    let href: String = v[at..].chars().take_while(|c| *c != '"').collect();
    let logo = call(r, "GET", &href, &[], None).await;
    assert_eq!(logo.status, 200);
    assert_eq!(logo.header("content-type"), "image/svg+xml");
}

#[tokio::test]
async fn the_error_pages_of_the_flow_show_the_brand_too() {
    let acc = crate::accounts_tests::rig();
    let r = &acc.rig.r;
    set_brand(&acc.app);
    let c = client(r).await;
    // An application that is not registered, a flow that is gone, a post from another site.
    let unknown = call(
        r,
        "GET",
        &authorize_url("oc_not_registered", REDIRECT, &[]),
        &[],
        None,
    )
    .await;
    assert_eq!(unknown.status, 400);
    let (_, b) = start(r, &authorize_url(&c, REDIRECT, &[])).await;
    let gone = call(
        r,
        "POST",
        "/oauth/authorize/login",
        &b.headers(),
        Some(form(&[
            ("flow", &"0".repeat(32)),
            ("csrf", &b.csrf),
            ("username", "ada"),
            ("password", PW),
        ])),
    )
    .await;
    assert_eq!(gone.status, 410, "{}", gone.text());
    let foreign = call(
        r,
        "POST",
        "/oauth/authorize/login",
        &[
            ("origin", "https://evil.test"),
            ("cookie", &b.cookie),
            ("x-forwarded-for", &b.ip),
        ],
        Some(form(&[("flow", &b.flow), ("csrf", &b.csrf)])),
    )
    .await;
    assert_eq!(foreign.status, 403, "{}", foreign.text());
    for (what, resp) in [
        ("unknown client", unknown),
        ("gone", gone),
        ("foreign", foreign),
    ] {
        let html = resp.text();
        let line = brand_line(&html);
        assert!(
            line.contains("<svg") && line.contains("MOHV"),
            "{what}: {line}"
        );
        assert!(!visible(&html).to_lowercase().contains("obop"), "{what}");
        assert!(
            policy(&resp).ends_with("; img-src 'self'"),
            "{what}: {}",
            policy(&resp)
        );
        assert!(html.contains("rel=\"icon\""), "{what}");
    }
    // Without the brand they are the pages they were.
    clear(&acc.app);
    let again = call(
        r,
        "GET",
        &authorize_url("oc_not_registered", REDIRECT, &[]),
        &[],
        None,
    )
    .await;
    assert!(
        again.text().contains("<p class=\"brand\">obop</p>") && !policy(&again).contains("img-src")
    );
}

// ---- A client named like the instance ----------------------------------------------------------------

/// The real registration (`clean_text` of the name) and the real pages: a hostile client that
/// registers as the brand's name, with an invisible character or a no-break space in it, in another
/// case, in fullwidth letters, or as a title of the brand, gets the fixed sentence on the consent
/// page. A client that only contains the name, or has another, does not.
#[tokio::test]
async fn a_client_registered_under_the_name_of_the_instance_gets_the_sentence() {
    let rig = rig();
    let r = &rig.r;
    configure(&rig.app, &[("name", "Zed"), ("title_sv", "Zed Mileage")]);
    let hostile = [
        ("Zed", LANGS.to_vec()),
        ("zed", vec!["en", "de"]),
        ("ZED", vec!["fr"]),
        ("ＺＥＤ", vec!["pl", "en"]),
        ("Z\u{200D}e\u{200C}d", vec!["en", "sv"]),
        ("Z\u{00AD}ed", vec!["fi"]),
        ("Z\u{FE0F}ed", vec!["es", "en"]),
        ("Z\u{00A0}e\u{00A0}d", vec!["en"]),
        ("Z\u{200B}\u{2060}ed", vec!["sv"]),
        // A title of the brand in another language than the page's.
        ("Zed Mileage", vec!["en", "de"]),
        ("ZED\u{200D} MILEAGE", vec!["en"]),
    ];
    for (name, langs) in hostile {
        let c = client_named(r, name).await;
        for lang in langs {
            let consent = consent_of(r, &c, lang).await;
            let sentence = format!("<p>{}</p></div>", esc(t(lang, "consent.same_name")));
            assert!(
                consent.text().contains(&sentence),
                "{name:?} in {lang}: no sentence in {}",
                consent.text()
            );
            // The box, the host line and the buttons are there as for any client.
            let html = consent.text();
            assert!(
                html.contains("<strong>claude.ai</strong>"),
                "{name:?}/{lang}"
            );
            assert!(html.contains("name=\"decision\" value=\"allow\""));
            assert!(html.contains("name=\"decision\" value=\"deny\""));
        }
    }
    for name in ["Zed Support", "Acme", "Zed.", "Z-e-d", "Zed Mileage Plus"] {
        let c = client_named(r, name).await;
        for lang in ["en", "sv"] {
            let html = consent_of(r, &c, lang).await.text();
            assert!(
                !html.contains(&esc(t(lang, "consent.same_name"))),
                "{name:?} in {lang} was taken for the instance"
            );
            // The brand stays a label: the brand's name is in the line above the card and nowhere
            // else but in the client's own name.
            let rest = html
                .replacen(&brand_line(&html), "", 1)
                .replace(&esc(name), "");
            assert!(!rest.contains("Zed"), "{name:?} in {lang}: {rest}");
        }
    }
    // With no brand the sentence never appears, whatever the client is called.
    clear(&rig.app);
    let c = client_named(r, "Zed").await;
    let html = consent_of(r, &c, "en").await.text();
    assert!(!html.contains(&esc(t("en", "consent.same_name"))));
}

/// The three classes of client through the real registration and the real router: the consent page
/// of a brand names the application twice in the claim and shows where the result goes.
#[tokio::test]
async fn the_consent_page_of_each_class_of_client_keeps_the_claim_and_the_host_line() {
    let rig = rig();
    let r = &rig.r;
    configure(&rig.app, &FIELDS);
    // An MCP client, a mobile app that registered a scheme, and one that registered a loopback
    // address.
    let mcp = client_named(r, "Claude").await;
    let mobile = {
        let ip = fresh_address();
        let c = call(
            r,
            "POST",
            "/oauth/register",
            &[("x-forwarded-for", &ip)],
            Some((
                "application/json",
                json!({"client_name": "Mileage", "obop_client_class": "mobile",
                       "redirect_uris": ["se.mohv.app:/oauth", "http://127.0.0.1/callback"],
                       "scope": "base.user", "token_endpoint_auth_method": "none"})
                .to_string(),
            )),
        )
        .await;
        assert_eq!(c.status, 201, "{}", c.text());
        c.json()["client_id"].as_str().unwrap().to_string()
    };
    let api = "http://inst.test/api";
    for lang in LANGS {
        for (client, redirect, resource, key, host, app) in [
            (
                &mcp,
                REDIRECT,
                "http://inst.test/mcp",
                "consent.who_body.neutral",
                "claude.ai",
                "Claude",
            ),
            (
                &mobile,
                "se.mohv.app:/oauth",
                api,
                "consent.who_body_mobile.neutral",
                "se.mohv.app",
                "Mileage",
            ),
            (
                &mobile,
                "http://127.0.0.1/callback",
                api,
                "consent.who_body_mobile_loopback.neutral",
                "127.0.0.1",
                "Mileage",
            ),
        ] {
            let url = authorize_url(client, redirect, &[("lang", lang), ("resource", resource)]);
            let (_, b) = start(r, &url).await;
            let resp = b
                .post(
                    r,
                    "/oauth/authorize/login",
                    &[("username", "ada"), ("password", PW)],
                )
                .await;
            assert_eq!(resp.status, 200, "{}", resp.text());
            let html = resp.text();
            assert!(html.contains(&filled(lang, key, app, host)), "{lang} {key}");
            assert!(policy(&resp).ends_with("; img-src 'self'"));
            assert!(
                !visible(&html).to_lowercase().contains("obop"),
                "{lang} {key}"
            );
            assert!(
                html.contains(&format!("<strong>{host}</strong>")),
                "{lang} {key}: the host line"
            );
        }
    }
}

// ---- The first read of the request (D74) -------------------------------------------------------------

/// What a sign-in leaves in the store: the sessions it made (a sign-in makes one and ends it) and the
/// entries of the security log.
fn footprint(app: &App) -> (usize, usize) {
    let b = app.store.backend();
    (
        b.list("auth_token").unwrap().len(),
        b.list("security_event").unwrap().len(),
    )
}

/// The OAuth server of the app, as a test uses it to put a flow in a state without the HTTP layer
/// (so the handler under test has not read the brand yet).
fn server_of(rig: &Rig) -> Arc<obop_oauth::OAuth> {
    obop_oauth::OAuth::new(
        rig.app.store.backend(),
        rig.auth.clone(),
        obop_oauth::OAuthConfig::default(),
    )
    .expect("the OAuth server")
}

/// A flow begun without the HTTP layer: the browser's view of it, and its id and secret.
fn begun(o: &obop_oauth::OAuth) -> (Browser, String) {
    let ip = fresh_address();
    let client = o
        .register(
            &ip,
            &json!({"client_name": "Claude", "redirect_uris": [REDIRECT],
                    "token_endpoint_auth_method": "none"}),
        )
        .unwrap()["client_id"]
        .as_str()
        .unwrap()
        .to_string();
    let p = obop_oauth::flow::AuthorizeParams {
        response_type: Some("code".into()),
        client_id: Some(client),
        redirect_uri: Some(REDIRECT.into()),
        state: Some("xyz".into()),
        code_challenge: Some(s256(VERIFIER)),
        code_challenge_method: Some("S256".into()),
        resources: vec!["http://inst.test/mcp".into()],
        ..Default::default()
    };
    let v = o.validate_authorize(BASE, &p).unwrap();
    let (flow, binder) = o.begin(&v, &ip, "en").unwrap();
    let csrf = o.view(&flow, &binder).unwrap().csrf;
    (
        Browser {
            cookie: format!("obop_oauth_{}={binder}", &flow[..12]),
            flow,
            csrf,
            ip,
        },
        binder,
    )
}

/// The answer of D74 to a request that met a store fault, in the shape a browser gets (a page; an
/// API client gets JSON): 503 with `Retry-After` and the word that nothing was applied.
fn assert_d74_503(resp: &Resp) {
    assert_eq!(resp.status, 503, "{}", resp.text());
    assert_eq!(resp.header("retry-after"), "1");
    assert_eq!(resp.header("x-obop-applied"), "none");
    let page = resp.text();
    assert!(
        page.contains("<title>Try again in a moment</title>"),
        "{page}"
    );
    assert!(
        !page.contains("class=\"brand\""),
        "not a page of the flow that says obop: {page}"
    );
    assert!(!page.to_lowercase().contains("obop"), "{page}");
}

/// A sign-in whose first brand read fails is answered by D74 and writes nothing: no session, no
/// entry of the log, the flow still waits for a person. The same post, once the store answers, signs
/// in as it always did, and the page it answers shows the brand.
#[tokio::test]
async fn a_sign_in_whose_first_brand_read_fails_writes_nothing_and_is_a_503() {
    let (flaky, rig) = flaky_rig();
    let o = server_of(&rig);
    let (b, binder) = begun(&o);
    let before = footprint(&rig.app);
    assert_eq!(o.view(&b.flow, &binder).unwrap().user, None);

    flaky.refuse_model("instance_brand", Mode::Reads);
    let refused_before = flaky.refused();
    let resp = b
        .post(
            &rig.r,
            "/oauth/authorize/login",
            &[("username", "ada"), ("password", PW)],
        )
        .await;
    assert_d74_503(&resp);
    assert!(
        flaky.refused() > refused_before,
        "the first read was the brand's"
    );
    assert_eq!(footprint(&rig.app), before, "nothing was written");
    assert_eq!(
        o.view(&b.flow, &binder).unwrap().user,
        None,
        "nobody is signed in on the flow"
    );
    // A wrong password is no different: the 503 is the answer, not the page that says it is wrong.
    let wrong = b
        .post(
            &rig.r,
            "/oauth/authorize/login",
            &[("username", "ada"), ("password", "nope")],
        )
        .await;
    assert_d74_503(&wrong);
    assert_eq!(footprint(&rig.app), before);

    // The store answers: the post that read the brand writes as usual.
    flaky.allow_models();
    configure(&rig.app, &FIELDS);
    let ok = b
        .post(
            &rig.r,
            "/oauth/authorize/login",
            &[("username", "ada"), ("password", PW)],
        )
        .await;
    assert_eq!(ok.status, 200, "{}", ok.text());
    assert!(ok.text().contains("name=\"decision\""));
    assert!(brand_line(&ok.text()).contains("MOHV"));
    assert_ne!(footprint(&rig.app), before, "it wrote");
    assert_eq!(
        o.view(&b.flow, &binder).unwrap().user.as_deref(),
        Some("ada")
    );
}

/// The decision is the same: a failing first read of the brand leaves the flow where it was and no
/// code is issued; after the store recovers the same post redirects with a code.
#[tokio::test]
async fn a_decision_whose_first_brand_read_fails_issues_nothing_and_is_a_503() {
    let (flaky, rig) = flaky_rig();
    let o = server_of(&rig);
    let (b, binder) = begun(&o);
    let outcome = o
        .login(&b.flow, &binder, &b.csrf, &b.ip, "ada", PW)
        .unwrap();
    assert_eq!(outcome, obop_oauth::LoginOut::Consent);

    let allow = [
        ("decision", "allow"),
        ("days", "30"),
        ("group", "base.user"),
    ];
    flaky.refuse_model("instance_brand", Mode::Reads);
    let refused = b.post(&rig.r, "/oauth/authorize/decision", &allow).await;
    assert_d74_503(&refused);
    assert!(
        refused.header("location").is_empty(),
        "no redirect, no code"
    );
    // The flow is still there, signed in, and was not consumed.
    assert_eq!(
        o.view(&b.flow, &binder).unwrap().user.as_deref(),
        Some("ada")
    );

    flaky.allow_models();
    let done = b.post(&rig.r, "/oauth/authorize/decision", &allow).await;
    let loc = location(&done);
    assert!(loc.starts_with(&format!("{REDIRECT}?code=oac_")), "{loc}");
}

/// The start of a flow: a failing first read of the brand is a 503 and no page; the same request
/// afterwards is the sign-in page of the brand.
#[tokio::test]
async fn the_start_of_a_flow_whose_first_brand_read_fails_is_a_503_and_never_a_page_that_says_obop()
{
    let (flaky, rig) = flaky_rig();
    let r = &rig.r;
    let c = client(r).await;
    let url = authorize_url(&c, REDIRECT, &[]);
    flaky.refuse_model("instance_brand", Mode::Reads);
    let ip = fresh_address();
    let refused = call(r, "GET", &url, &[("x-forwarded-for", &ip)], None).await;
    assert_d74_503(&refused);
    assert!(refused.header("set-cookie").is_empty(), "no flow was begun");
    flaky.allow_models();
    configure(&rig.app, &FIELDS);
    let (resp, _) = start(r, &url).await;
    assert!(brand_line(&resp.text()).contains("MOHV"));
}

/// The pages of a request come from one read: a brand changed between two requests of one flow
/// shows at once in this process (the reader's cell is replaced by `apply`), and no page of one
/// request mixes the two.
#[tokio::test]
async fn a_change_of_the_brand_between_two_requests_of_a_flow_shows_in_the_next() {
    let rig = rig();
    let r = &rig.r;
    configure(&rig.app, &[("name", "First")]);
    let c = client(r).await;
    let (resp, b) = start(r, &authorize_url(&c, REDIRECT, &[])).await;
    assert!(brand_line(&resp.text()).ends_with("First"));
    configure(&rig.app, &[("name", "Second")]);
    let consent = b
        .post(
            r,
            "/oauth/authorize/login",
            &[("username", "ada"), ("password", PW)],
        )
        .await;
    let html = consent.text();
    assert!(
        brand_line(&html).ends_with("Second"),
        "{}",
        brand_line(&html)
    );
    assert!(!html.contains("First"));
}
