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
use obop_storage::Backend;
use obop_storage::testing::{Flaky, Mode};
use std::sync::atomic::{AtomicBool, Ordering};

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

/// A client of the mobile class (a scheme and a loopback address), registered through the real
/// registration as `Mileage`.
async fn mobile_client(r: &axum::Router) -> String {
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

/// The third way a client's name can be the instance's (spec section 10): its slug. The name of the
/// brand here is `Zed Co`, whose slug (`zed-co`) differs from the folded name (`zedco`), so only the
/// slug can make these match. Through the real registration and the real pages: the slug in another
/// case, in fullwidth letters, with an invisible character in it, or with white space around its hyphen
/// gets the fixed sentence; a name that only contains it, or that is not it, does not. Once the row
/// stores another slug, that one is the effective slug and the one derived from the name is not.
#[tokio::test]
async fn a_client_registered_under_the_slug_of_the_instance_gets_the_sentence() {
    let rig = rig();
    let r = &rig.r;
    configure(&rig.app, &[("name", "Zed Co")]);
    let gets_it = |name: String, lang: &'static str, want: bool| {
        let router = r.clone();
        async move {
            let c = client_named(&router, &name).await;
            let html = consent_of(&router, &c, lang).await.text();
            let sentence = format!("<p>{}</p></div>", esc(t(lang, "consent.same_name")));
            assert_eq!(html.contains(&sentence), want, "{name:?} in {lang}: {html}");
        }
    };
    for (name, lang) in [
        ("zed-co", "en"),
        ("ZED-CO", "de"),
        ("Zed-Co", "sv"),
        ("ＺＥＤ-ＣＯ", "en"),
        ("z\u{200D}ed-co", "fr"),
        ("Zed - Co", "en"),
        ("zed\u{00A0}-\u{00A0}co", "es"),
        // The name itself still matches (the fold ignores the space), as before.
        ("Zed Co", "en"),
    ] {
        gets_it(name.to_string(), lang, true).await;
    }
    for name in ["zed-co2", "co-zed", "zed.co", "zed_co", "Zed", "Zed Coop"] {
        gets_it(name.to_string(), "en", false).await;
    }
    // A slug the row stores is the effective one: the slug derived from the name no longer is.
    configure(&rig.app, &[("slug", "zed-shop")]);
    gets_it("zed-shop".into(), "en", true).await;
    gets_it("ZED-SHOP".into(), "sv", true).await;
    gets_it("zed-co".into(), "en", false).await;
    // With no brand the sentence never appears.
    clear(&rig.app);
    gets_it("zed-shop".into(), "en", false).await;
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
    let mobile = mobile_client(r).await;
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

/// What a sign-in leaves in the store: the sessions it made (a sign-in makes one and ends it), the
/// entries of the security log, and every short-lived value (the flows, the counters of the
/// per-address limits, the lockout counters the store keeps).
type Footprint = (usize, usize, Vec<(String, String)>);

fn footprint(app: &App) -> Footprint {
    let b = app.store.backend();
    (
        b.list("auth_token").unwrap().len(),
        b.list("security_event").unwrap().len(),
        b.kv_scan("").unwrap(),
    )
}

/// The expiring counters a request of the flow can move, which `footprint` cannot see (a counter is
/// no record and no value of the key-value store): the hits of an address's window of the start,
/// the hits of its window of the sign-in pages, and the failures that count toward the lockout of an
/// account. A request whose first brand read fails writes none of the things `footprint` holds but
/// still moves these: `fault::before_write` does not guard `counter_add` (see `read_brand` in
/// `obop-http/src/oauth.rs`).
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct Counters {
    /// `oauth:rl:authz:<address>`, spent by a start.
    start: u64,
    /// `oauth:rl:login:<address>`, spent by a password step, a second step and a passkey.
    login: u64,
    /// `auth.fail:<user>`, spent by a wrong password (`Auth::failure_key`, which is private).
    failures: u64,
}

fn counters(app: &App, ip: &str, user: &str) -> Counters {
    let b = app.store.backend();
    let window = |kind: &str| {
        b.counter_get(&format!(
            "oauth:rl:{kind}:{}",
            &obop_oauth::util::sha_hex(ip)[..24]
        ))
        .unwrap()
    };
    Counters {
        start: window("authz"),
        login: window("login"),
        failures: b.counter_get(&format!("auth.fail:{user}")).unwrap(),
    }
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

/// The flow as a page would show it for an instance with no brand set: for the tests that look at
/// who is signed in on a flow, which the brand does not change.
fn peek(o: &obop_oauth::OAuth, flow: &str, binder: &str) -> obop_oauth::flow::FlowCtx {
    o.view(
        flow,
        binder,
        &Arc::new(obop_commands::brand::Shown::not_set(None)),
    )
    .unwrap()
}

/// A flow begun without the HTTP layer: the browser's view of it, and its id and secret.
fn begun(o: &obop_oauth::OAuth) -> (Browser, String) {
    begun_as(
        o,
        &json!({"client_name": "Claude", "redirect_uris": [REDIRECT],
                "token_endpoint_auth_method": "none"}),
        REDIRECT,
        "http://inst.test/mcp",
    )
}

/// [`begun`] for a client that registers with `registration`, and the redirect and resource it
/// asks for.
fn begun_as(
    o: &obop_oauth::OAuth,
    registration: &serde_json::Value,
    redirect: &str,
    resource: &str,
) -> (Browser, String) {
    let ip = fresh_address();
    let client = o.register(&ip, registration).unwrap()["client_id"]
        .as_str()
        .unwrap()
        .to_string();
    let p = obop_oauth::flow::AuthorizeParams {
        response_type: Some("code".into()),
        client_id: Some(client),
        redirect_uri: Some(redirect.into()),
        state: Some("xyz".into()),
        code_challenge: Some(s256(VERIFIER)),
        code_challenge_method: Some("S256".into()),
        resources: vec![resource.into()],
        ..Default::default()
    };
    let v = o.validate_authorize(BASE, &p).unwrap();
    let (flow, binder) = o.begin(&v, &ip, "en").unwrap();
    let csrf = peek(o, &flow, &binder).csrf;
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

/// The registration of a mobile app that has one private-use redirect (`Mileage`), as the tests of
/// the Microsoft step make it.
fn mobile_registration() -> serde_json::Value {
    json!({"client_name": "Mileage", "obop_client_class": "mobile",
           "redirect_uris": ["se.mohv.app:/oauth"], "scope": "base.user",
           "token_endpoint_auth_method": "none"})
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

/// A sign-in whose first brand read fails is answered by D74 and leaves the store as it was: no
/// session, no entry of the log, no record written, no short-lived value changed, and the flow still
/// waits for a person. The expiring counters are the exception, and they are pinned in both
/// directions: every such request spends exactly one hit of the sign-in window of its address (a
/// right password too), and only a wrong password adds a failure toward the lockout of the account
/// (the test after this one takes that to the lockout). The same post, once the store answers,
/// signs in as it always did, and the page it answers shows the brand.
#[tokio::test]
async fn a_sign_in_whose_first_brand_read_fails_leaves_the_store_as_it_was_and_is_a_503() {
    let (flaky, rig) = flaky_rig();
    let o = server_of(&rig);
    let (b, binder) = begun(&o);
    let before = footprint(&rig.app);
    let spent = counters(&rig.app, &b.ip, "ada");
    assert_eq!(spent.start, 1, "the start spent a hit of its own window");
    let writes_before = flaky.seen_writes().len();
    assert_eq!(peek(&o, &b.flow, &binder).user, None);

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
    assert_eq!(
        footprint(&rig.app),
        before,
        "no session, entry, record or short-lived value was written"
    );
    assert_eq!(
        flaky.seen_writes().len(),
        writes_before,
        "no batch reached the store"
    );
    assert_eq!(
        peek(&o, &b.flow, &binder).user,
        None,
        "nobody is signed in on the flow"
    );
    // The counters are the exception: the right password spent one hit of the window of its
    // address and no failure.
    assert_eq!(
        counters(&rig.app, &b.ip, "ada"),
        Counters {
            login: spent.login + 1,
            ..spent
        },
        "a right password under a failing first read spends one hit of the sign-in window"
    );
    // A wrong password is no different for the store: the 503 is the answer, not the page that says
    // it is wrong, and the store is as it was. It spends a second hit of the window and counts one
    // failure toward the lockout of the account.
    let wrong = b
        .post(
            &rig.r,
            "/oauth/authorize/login",
            &[("username", "ada"), ("password", "nope")],
        )
        .await;
    assert_d74_503(&wrong);
    assert_eq!(footprint(&rig.app), before);
    assert_eq!(flaky.seen_writes().len(), writes_before);
    assert_eq!(
        counters(&rig.app, &b.ip, "ada"),
        Counters {
            login: spent.login + 2,
            failures: spent.failures + 1,
            ..spent
        },
        "a wrong password spends a hit of the window and counts one failure"
    );

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
    assert_eq!(peek(&o, &b.flow, &binder).user.as_deref(), Some("ada"));
}

/// A wrong password under a failing first read is answered 503 and leaves the records and the
/// short-lived values as they were (the test above), but the counters move: the lockout counter of
/// the account, which `Auth` keeps for itself and which counts a wrong guess whatever else the
/// request meets, and the sign-in window of the address. After four such guesses the right password
/// still signs in; after five it meets the lockout (`err.locked`), as five wrong guesses on a
/// healthy store would. The claim "writes nothing" is therefore not made for the counters; this test
/// and the one above are what say what is true.
#[tokio::test]
async fn a_wrong_password_under_a_failing_first_read_still_counts_toward_the_lockout() {
    async fn right_password_after(wrong: usize) -> Resp {
        let (flaky, rig) = flaky_rig();
        let o = server_of(&rig);
        let (b, _) = begun(&o);
        flaky.refuse_model("instance_brand", Mode::Reads);
        for _ in 0..wrong {
            let resp = b
                .post(
                    &rig.r,
                    "/oauth/authorize/login",
                    &[("username", "ada"), ("password", "nope")],
                )
                .await;
            assert_d74_503(&resp);
        }
        flaky.allow_models();
        configure(&rig.app, &FIELDS);
        b.post(
            &rig.r,
            "/oauth/authorize/login",
            &[("username", "ada"), ("password", PW)],
        )
        .await
    }
    let four = right_password_after(4).await;
    assert_eq!(four.status, 200, "{}", four.text());
    assert!(four.text().contains("name=\"decision\""), "signed in");
    let five = right_password_after(5).await;
    assert_eq!(five.status, 200, "{}", five.text());
    assert!(
        five.text().contains(&esc(t("en", "err.locked"))),
        "locked out: {}",
        five.text()
    );
    assert!(!five.text().contains("name=\"decision\""));
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
    let counted = counters(&rig.app, &b.ip, "ada");
    flaky.refuse_model("instance_brand", Mode::Reads);
    let refused = b.post(&rig.r, "/oauth/authorize/decision", &allow).await;
    assert_d74_503(&refused);
    assert!(
        refused.header("location").is_empty(),
        "no redirect, no code"
    );
    // The flow is still there, signed in, and was not consumed.
    assert_eq!(peek(&o, &b.flow, &binder).user.as_deref(), Some("ada"));
    // A decision counts in no window and no lockout: not even the counters move.
    assert_eq!(counters(&rig.app, &b.ip, "ada"), counted);

    flaky.allow_models();
    let done = b.post(&rig.r, "/oauth/authorize/decision", &allow).await;
    let loc = location(&done);
    assert!(loc.starts_with(&format!("{REDIRECT}?code=oac_")), "{loc}");
}

/// The start of a flow: a failing first read of the brand is a 503 and no page, no flow and no
/// cookie (but one hit of the window of the address, a counter); the same request afterwards is the
/// sign-in page of the brand.
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
    // The start counts before it writes the flow, and a counter is not guarded: the refused start
    // spent one hit of the window of its address (60 in ten minutes), and nothing else moved.
    assert_eq!(
        counters(&rig.app, &ip, "ada"),
        Counters {
            start: 1,
            login: 0,
            failures: 0
        },
        "a refused start spends a hit of its window"
    );
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

/// The Microsoft step of a mobile app's flow has two requests of its own that make a page: the link
/// on the sign-in page and the return address. On an instance with no Microsoft provider the first
/// answers with the sign-in page again (a word under the lead) and the second with the page for a
/// flow that still waits. A failing first read of the brand is the 503 of D74 on either; once the
/// store answers both are pages of the brand, made from the one read at the start of the request;
/// with no row they are the pages they were. (This flow has nothing pending, so the return writes
/// nothing here whatever its order; that the return reads the brand before it writes anything is
/// held by `sso_tests::phone`, on a flow with a pending second step.)
#[tokio::test]
async fn the_two_requests_of_the_microsoft_step_show_the_brand_and_read_it_first() {
    let (flaky, rig) = flaky_rig();
    let r = &rig.r;
    let o = server_of(&rig);
    // The flow is begun without the HTTP layer, so that the reader of the app has not read the brand
    // when the first request below arrives (a read within its bound would be served from its cell).
    let (b, _) = begun_as(
        &o,
        &json!({"client_name": "Mileage", "obop_client_class": "mobile",
                "redirect_uris": ["se.mohv.app:/oauth"], "scope": "base.user",
                "token_endpoint_auth_method": "none"}),
        "se.mohv.app:/oauth",
        "http://inst.test/api",
    );
    let paths = [
        format!("/oauth/authorize/microsoft?flow={}", b.flow),
        format!("/oauth/authorize/return?flow={}", b.flow),
    ];
    flaky.refuse_model("instance_brand", Mode::Reads);
    for path in &paths {
        let refused = call(r, "GET", path, &b.headers(), None).await;
        assert_d74_503(&refused);
    }
    flaky.allow_models();
    configure(&rig.app, &FIELDS);
    for path in &paths {
        let resp = call(r, "GET", path, &b.headers(), None).await;
        assert_eq!(resp.status, 200, "{path}: {}", resp.text());
        let html = resp.text();
        let line = brand_line(&html);
        assert!(
            line.contains("<svg") && line.contains("MOHV"),
            "{path}: {line}"
        );
        assert!(!visible(&html).to_lowercase().contains("obop"), "{path}");
        assert!(policy(&resp).ends_with("; img-src 'self'"), "{path}");
        assert!(html.contains("rel=\"icon\""), "{path}");
    }
    clear(&rig.app);
    for path in &paths {
        let resp = call(r, "GET", path, &b.headers(), None).await;
        assert_eq!(resp.status, 200, "{path}: {}", resp.text());
        let html = resp.text();
        assert!(html.contains("<p class=\"brand\">obop</p>"), "{path}");
        assert!(!policy(&resp).contains("img-src"), "{path}");
        assert!(!html.contains("rel=\"icon\""), "{path}");
    }
}

// ---- Every page a handler writes when it refuses (review of S1c) ------------------------------------

/// What every page of the flow must be with a brand set, whichever handler wrote it: the logo and
/// the name above the card, the tab icon and the policy that allows it, and nothing that names the
/// product. Returns what is wrong with the page, so a test can list every arm that fails.
fn unbranded(resp: &Resp) -> Option<String> {
    unbranded_page(&resp.text(), &policy(resp))
}

/// [`unbranded`] for a page and its policy header, so that a test with a router of its own (the
/// Microsoft step is held in `sso_tests`, which has the stand-in provider) asks the same question.
pub(crate) fn unbranded_page(html: &str, policy: &str) -> Option<String> {
    if !html.contains("<p class=\"brand\">") {
        return Some("no brand line".into());
    }
    let line = brand_line(html);
    if !(line.contains("<svg") && line.contains("MOHV")) {
        return Some(format!("the brand line is {line}"));
    }
    if html.to_lowercase().contains("obop") {
        return Some("the page names the product".into());
    }
    if !policy.ends_with("; img-src 'self'") {
        return Some(format!("the policy is {policy}"));
    }
    if !html.contains("rel=\"icon\"") {
        return Some("no tab icon".into());
    }
    None
}

/// Run `arms` (a name, the status the arm must answer and the response) and list the arms whose
/// page does not show the brand, or whose status is not the one expected.
fn failures_of(arms: Vec<(&str, u16, Resp)>) -> Vec<String> {
    let mut failures = Vec::new();
    for (name, status, resp) in arms {
        if resp.status != status {
            failures.push(format!("{name}: status {} (want {status})", resp.status));
        }
        if let Some(why) = unbranded(&resp) {
            failures.push(format!("{name}: {why}"));
        }
    }
    failures
}

/// The arms of the handlers that answer with a page and that a request can reach on its own, with a
/// brand set: the error pages of the start (a parameter twice, the 429 of the address), of a form
/// post (a wrong token, a body that is no form, a junk decision, the 429 of the sign-in), of the
/// decision (a wrong token, a flow nobody is signed in on, a flow that is gone, a second answer to
/// a flow that was answered; each for allow, deny and "Not you?"), of the Microsoft step (a flow
/// that does not exist, on the link and on the return, and the link past its window), of the
/// passkey post that has no origin to work with; and the pages that follow a refused decision and
/// the "Not you?" button. Each shows the logo and the name and names no product. A handler that
/// falls back to the page of an instance with no brand fails here by name.
///
/// What this list does not reach is held by the two tests after it and by `sso_tests`: a flow that
/// is lost between two reads of one request (`a_flow_lost_between_two_reads...`), and the arms of
/// the Microsoft link that need a provider (`phone_the_refusals_of_the_microsoft_link...`). One arm
/// is not held by any test, because no test can see it (`phone_a_store_that_refuses_the_row...`).
#[tokio::test]
async fn every_page_a_refusal_makes_shows_the_brand() {
    let rig = rig();
    let r = &rig.r;
    configure(&rig.app, &FIELDS);
    let c = client(r).await;
    let url = authorize_url(&c, REDIRECT, &[]);
    let o = server_of(&rig);
    let mut arms: Vec<(&str, u16, Resp)> = Vec::new();

    // The start: a parameter given twice, and the 61st start from one address in ten minutes.
    let twice = format!("{url}&client_id={c}");
    arms.push((
        "client_id twice",
        400,
        call(
            r,
            "GET",
            &twice,
            &[("x-forwarded-for", &fresh_address())],
            None,
        )
        .await,
    ));
    let busy_ip = fresh_address();
    let mut busy = None;
    for _ in 0..61 {
        busy = Some(call(r, "GET", &url, &[("x-forwarded-for", &busy_ip)], None).await);
    }
    arms.push(("61st start from one address", 429, busy.unwrap()));

    // Form posts of a browser that has a flow: a wrong token, a body that is no form, a decision
    // that is none of the three.
    let (b, binder) = begun(&o);
    arms.push((
        "wrong csrf",
        403,
        call(
            r,
            "POST",
            "/oauth/authorize/login",
            &b.headers(),
            Some(form(&[
                ("flow", &b.flow),
                ("csrf", "not-the-token"),
                ("username", "ada"),
                ("password", PW),
            ])),
        )
        .await,
    ));
    arms.push((
        "a body that is no form",
        400,
        call(
            r,
            "POST",
            "/oauth/authorize/login",
            &b.headers(),
            Some(("application/json", "{\"flow\":1}".into())),
        )
        .await,
    ));
    arms.push((
        "decision=maybe",
        400,
        b.post(r, "/oauth/authorize/decision", &[("decision", "maybe")])
            .await,
    ));

    // The pages that follow a refused decision and the "Not you?" button: signed in, allow with no
    // group ticked (the consent page again, with the word), then switch (the sign-in page again).
    assert_eq!(
        o.login(&b.flow, &binder, &b.csrf, &b.ip, "ada", PW)
            .unwrap(),
        obop_oauth::LoginOut::Consent
    );
    let refused = b
        .post(
            r,
            "/oauth/authorize/decision",
            &[("decision", "allow"), ("days", "30")],
        )
        .await;
    assert!(
        refused.text().contains(&esc(t("en", "err.no_scope"))),
        "the word: {}",
        refused.text()
    );
    arms.push(("allow with no group", 200, refused));
    let switched = b
        .post(r, "/oauth/authorize/decision", &[("decision", "switch")])
        .await;
    assert!(switched.text().contains("name=\"username\""));
    arms.push(("switch", 200, switched));

    // A decision that cannot be taken, for each of the three answers: a token that is not the
    // flow's, a flow that is gone, and (allow and deny) a flow nobody has signed in on. "Not you?"
    // needs no one signed in, so it has no third arm.
    let (w, _) = begun(&o);
    let gone = "0".repeat(32);
    for (d, wrong_csrf, flow_gone, nobody_in) in [
        (
            "allow",
            "allow, wrong csrf",
            "allow, flow gone",
            Some("allow, nobody signed in"),
        ),
        (
            "deny",
            "deny, wrong csrf",
            "deny, flow gone",
            Some("deny, nobody signed in"),
        ),
        ("switch", "switch, wrong csrf", "switch, flow gone", None),
    ] {
        let post =
            |flow: &str, csrf: &str| Some(form(&[("flow", flow), ("csrf", csrf), ("decision", d)]));
        let path = "/oauth/authorize/decision";
        arms.push((
            wrong_csrf,
            403,
            call(
                r,
                "POST",
                path,
                &w.headers(),
                post(&w.flow, "not-the-token"),
            )
            .await,
        ));
        arms.push((
            flow_gone,
            410,
            call(r, "POST", path, &w.headers(), post(&gone, "whatever")).await,
        ));
        if let Some(name) = nobody_in {
            arms.push((
                name,
                403,
                call(r, "POST", path, &w.headers(), post(&w.flow, &w.csrf)).await,
            ));
        }
    }
    // A second allow after the first one was taken: the flow is finished, the page says so.
    let (dbl, dbl_binder) = begun(&o);
    assert_eq!(
        o.login(&dbl.flow, &dbl_binder, &dbl.csrf, &dbl.ip, "ada", PW)
            .unwrap(),
        obop_oauth::LoginOut::Consent
    );
    let allow = [
        ("decision", "allow"),
        ("days", "30"),
        ("group", "base.user"),
    ];
    let first = dbl.post(r, "/oauth/authorize/decision", &allow).await;
    assert_eq!(first.status, 303, "{}", first.text());
    arms.push((
        "allow, the second time",
        410,
        dbl.post(r, "/oauth/authorize/decision", &allow).await,
    ));

    // The link of the Microsoft step past its window of 30 a minute from one address: the sign-in
    // page again, with the word. (This instance has no provider, and the window is asked first.)
    let (m, _) = begun_as(
        &o,
        &mobile_registration(),
        "se.mohv.app:/oauth",
        "http://inst.test/api",
    );
    let link = format!("/oauth/authorize/microsoft?flow={}", m.flow);
    let mut over = None;
    for _ in 0..31 {
        over = Some(call(r, "GET", &link, &m.headers(), None).await);
    }
    let over = over.unwrap();
    assert!(
        over.text().contains(&esc(t("en", "err.ms_busy"))),
        "the word of the 31st: {}",
        over.text()
    );
    arms.push(("microsoft link, 31st from one address", 200, over));

    // The 31st sign-in from one address in ten minutes: a flow of its own for each.
    let ip = fresh_address();
    let mut last = None;
    for _ in 0..31 {
        let (mut f, _) = begun(&o);
        f.ip = ip.clone();
        last = Some(
            f.post(
                r,
                "/oauth/authorize/login",
                &[("username", "ada"), ("password", PW)],
            )
            .await,
        );
    }
    arms.push(("31st sign-in from one address", 429, last.unwrap()));

    // The Microsoft step for a flow that does not exist: the link and the return address.
    let nobody = "0".repeat(32);
    for (what, path) in [
        ("microsoft link, no flow", "/oauth/authorize/microsoft"),
        ("microsoft return, no flow", "/oauth/authorize/return"),
    ] {
        arms.push((
            what,
            410,
            call(
                r,
                "GET",
                &format!("{path}?flow={nobody}"),
                &[("x-forwarded-for", &fresh_address())],
                None,
            )
            .await,
        ));
    }

    // The passkey post of a browser whose host names no relying party (`:80`): the 503 page.
    let (p, _) = begun(&o);
    arms.push((
        "passkey, no origin",
        503,
        call(
            r,
            "POST",
            "/oauth/authorize/passkey",
            &[
                ("host", ":80"),
                ("origin", "http://:80"),
                ("cookie", &p.cookie),
                ("x-forwarded-for", &p.ip),
            ],
            Some(form(&[
                ("flow", &p.flow),
                ("csrf", &p.csrf),
                ("credential", "{}"),
            ])),
        )
        .await,
    ));

    let failures = failures_of(arms);
    assert!(
        failures.is_empty(),
        "pages of the flow that do not show the brand:\n{}",
        failures.join("\n")
    );
}

/// The pages that follow a password step when the account has an authenticator app: the page that
/// asks for the code (`LoginOut::Mfa`) and the page that asks again after a wrong one
/// (`LoginOut::MfaRetry`), the two arms of `finish_login` that none of the lists above reaches.
/// With a brand set each shows the logo and the name, the tab icon and the policy that allows it,
/// in every language, and nothing that names the product; the right code then gives the consent
/// page of the brand. A handler that builds either page from the context of an instance with no
/// brand fails here by name.
///
/// The hidden field of the page holds the token the server minted, and a real token starts with the
/// product's wire prefix (`obopmfa_`, `obop-auth/src/mfa.rs`): a secret nobody reads and the spec
/// does not ask to rename (section 13 asks for no obop that a person sees). So the check takes the
/// value out of the page first, and the test says that it was there.
#[tokio::test]
async fn the_second_factor_pages_of_the_flow_show_the_brand() {
    let rig = rig();
    let r = &rig.r;
    rig.auth.set_clock_for_test(T0);
    let (secret, _) = enrol(
        &crate::mfa_tests::Rig {
            r: rig.r.clone(),
            auth: rig.auth.clone(),
        },
        "ada",
    )
    .await;
    rig.auth.set_clock_for_test(T0 + 30);
    configure(&rig.app, &FIELDS);
    let c = client(r).await;
    let mut arms: Vec<(String, u16, String, String)> = Vec::new();
    let mut last = None;
    for lang in LANGS {
        let (_, b) = start(r, &authorize_url(&c, REDIRECT, &[("lang", lang)])).await;
        let asks = b
            .post(
                r,
                "/oauth/authorize/login",
                &[("username", "ada"), ("password", PW)],
            )
            .await;
        let html = asks.text();
        assert!(html.contains("name=\"mfa_token\""), "{lang}: {html}");
        assert!(
            !html.contains("name=\"decision\""),
            "{lang}: no consent yet"
        );
        let token = hidden(&html, "mfa_token");
        assert!(token.starts_with("obopmfa_"), "{lang}: {token}");
        arms.push((
            format!("{lang}: the code is asked for"),
            asks.status,
            html.replace(&token, "TOKEN"),
            policy(&asks),
        ));
        // A wrong code asks again, in two of the languages (the token allows five tries).
        if ["en", "sv"].contains(&lang) {
            let wrong = b
                .post(
                    r,
                    "/oauth/authorize/mfa",
                    &[("mfa_token", &token), ("code", "000000")],
                )
                .await;
            let html = wrong.text();
            assert!(
                html.contains("name=\"mfa_token\"") && html.contains(&esc(t(lang, "err.code"))),
                "{lang}: {html}"
            );
            arms.push((
                format!("{lang}: the code was wrong"),
                wrong.status,
                html.replace(&token, "TOKEN"),
                policy(&wrong),
            ));
        }
        last = Some((b, token));
    }
    // The right code, on the last flow: the consent page of the brand.
    let (b, token) = last.unwrap();
    let good = b
        .post(
            r,
            "/oauth/authorize/mfa",
            &[("mfa_token", &token), ("code", &code(&secret, T0 + 30))],
        )
        .await;
    assert!(good.text().contains("name=\"decision\""), "{}", good.text());
    arms.push((
        "the right code".into(),
        good.status,
        good.text(),
        policy(&good),
    ));
    let mut failures = Vec::new();
    for (name, status, html, policy) in &arms {
        if *status != 200 {
            failures.push(format!("{name}: status {status}"));
        }
        if let Some(why) = unbranded_page(html, policy) {
            failures.push(format!("{name}: {why}"));
        }
    }
    assert!(
        failures.is_empty(),
        "second-factor pages that do not show the brand:\n{}",
        failures.join("\n")
    );
    assert_eq!(arms.len(), LANGS.len() + 2 + 1);
}

/// Lose every flow right before the `nth` read of a flow (counting from 0) that the next request
/// makes: another request finishing the flow, or its time running out, between two reads of one
/// request. The answer says whether the request made that many reads. The hook is re-armed from
/// inside itself `nth` times, so no read has to be counted by hand.
fn lose_the_flow_at(flaky: &Arc<Flaky>, nth: usize) -> Arc<AtomicBool> {
    fn arm(flaky: Arc<Flaky>, lost: Arc<AtomicBool>, left: usize) {
        let again = flaky.clone();
        let flag = lost.clone();
        flaky.before_the_next_kv_get_of(
            "oauth:flow:",
            Box::new(move |b: &dyn Backend| {
                if left == 0 {
                    for (key, _) in b.kv_scan("oauth:flow:").unwrap() {
                        let _ = b.kv_delete(&key);
                    }
                    flag.store(true, Ordering::SeqCst);
                } else {
                    arm(again, flag, left - 1);
                }
            }),
        );
    }
    let lost = Arc::new(AtomicBool::new(false));
    arm(flaky.clone(), lost.clone(), nth);
    lost
}

/// The requests that read a flow more than once, and what each does when the flow is gone at each
/// of its reads in turn: the start, the password step, the three answers of the consent page (the
/// refused allow among them), and the two requests of the Microsoft step. Whatever page comes back
/// shows the brand. This reaches the arms that name a flow that was there a moment ago: the pages
/// after a step (`finish_login`, the consent page again, the sign-in page after "Not you?" and the
/// one with a word of the Microsoft step), which no request on its own reaches.
#[tokio::test]
async fn a_flow_lost_between_two_reads_of_a_request_still_shows_the_brand() {
    let (flaky, rig) = flaky_rig();
    let (r, o) = (&rig.r, server_of(&rig));
    configure(&rig.app, &FIELDS);
    let c = client(r).await;
    let kinds = [
        "start",
        "password",
        "allow, no group",
        "allow",
        "deny",
        "switch",
        "microsoft link",
        "microsoft return",
    ];
    let mut pages: Vec<(String, Resp)> = Vec::new();
    for kind in kinds {
        let mut reached = 0;
        for nth in 0..6 {
            let mobile = kind.starts_with("microsoft");
            let (b, binder) = if mobile {
                begun_as(
                    &o,
                    &mobile_registration(),
                    "se.mohv.app:/oauth",
                    "http://inst.test/api",
                )
            } else {
                begun(&o)
            };
            if ["allow, no group", "allow", "deny", "switch"].contains(&kind) {
                assert_eq!(
                    o.login(&b.flow, &binder, &b.csrf, &b.ip, "ada", PW)
                        .unwrap(),
                    obop_oauth::LoginOut::Consent
                );
            }
            let lost = lose_the_flow_at(&flaky, nth);
            let decision = "/oauth/authorize/decision";
            let resp = match kind {
                "start" => {
                    let ip = fresh_address();
                    let url = authorize_url(&c, REDIRECT, &[]);
                    call(r, "GET", &url, &[("x-forwarded-for", &ip)], None).await
                }
                "password" => {
                    let who = [("username", "ada"), ("password", PW)];
                    b.post(r, "/oauth/authorize/login", &who).await
                }
                "allow, no group" => {
                    b.post(r, decision, &[("decision", "allow"), ("days", "30")])
                        .await
                }
                "allow" => {
                    let all = [
                        ("decision", "allow"),
                        ("days", "30"),
                        ("group", "base.user"),
                    ];
                    b.post(r, decision, &all).await
                }
                "deny" => b.post(r, decision, &[("decision", "deny")]).await,
                "switch" => b.post(r, decision, &[("decision", "switch")]).await,
                "microsoft link" => {
                    let link = format!("/oauth/authorize/microsoft?flow={}", b.flow);
                    call(r, "GET", &link, &b.headers(), None).await
                }
                _ => {
                    let back = format!("/oauth/authorize/return?flow={}", b.flow);
                    call(r, "GET", &back, &b.headers(), None).await
                }
            };
            // Whatever the hook has not used is not left for the next request.
            flaky.before_the_next_kv_get_of("oauth:flow:", Box::new(|_| {}));
            if !lost.load(Ordering::SeqCst) {
                break;
            }
            reached += 1;
            pages.push((format!("{kind}, flow lost at read {nth}"), resp));
        }
        assert!(reached >= 1, "{kind}: the request never read a flow");
    }
    let mut failures = Vec::new();
    for (name, resp) in &pages {
        // An answer that redirects has no page of ours.
        if resp.status != 303
            && let Some(why) = unbranded(resp)
        {
            failures.push(format!("{name}: status {}: {why}", resp.status));
        }
    }
    assert!(
        failures.is_empty(),
        "pages of the flow that do not show the brand:
{}",
        failures.join(
            "
"
        )
    );
    // Not vacuous: a request whose flow was lost ended in a page that says so, for each kind.
    for kind in kinds {
        assert!(
            pages
                .iter()
                .any(|(n, p)| n.starts_with(kind) && matches!(p.status, 403 | 410)),
            "{kind}: no read of the flow ended in an error page; {:?}",
            pages
                .iter()
                .filter(|(n, _)| n.starts_with(kind))
                .map(|(n, p)| (n.clone(), p.status))
                .collect::<Vec<_>>()
        );
    }
}

/// The probe above is not vacuous: the same arms with no brand are the pages of an instance that
/// sets nothing (`unbranded` finds each of them wanting).
#[tokio::test]
async fn the_probe_of_the_refusal_pages_sees_a_page_that_says_obop() {
    let rig = rig();
    let r = &rig.r;
    let c = client(r).await;
    let twice = format!("{}&client_id={c}", authorize_url(&c, REDIRECT, &[]));
    let resp = call(
        r,
        "GET",
        &twice,
        &[("x-forwarded-for", &fresh_address())],
        None,
    )
    .await;
    assert_eq!(resp.status, 400);
    let why = unbranded(&resp).expect("a page of an instance with no brand is not branded");
    assert!(why.contains("brand line is"), "{why}");
}

// ---- After a good read (D74, review of S1c) ----------------------------------------------------------

/// The reader of the app keeps a good read for two seconds (`DEFAULT_BOUND` in
/// `obop-commands/src/brand/reader.rs`, which a test of this crate cannot shorten): wait until the
/// cell that the read before `filled` made is stale. The loop is on the clock, so an early wake-up
/// does not end it early. If the constant grows, the request that the test expects to be refused is
/// answered 200 and the test fails loudly.
pub(crate) fn wait_until_stale(filled: std::time::Instant) {
    let stale = std::time::Duration::from_millis(2050);
    while filled.elapsed() <= stale {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// After a good read, a store that then fails: once the cell has expired, the request that runs the
/// refill meets the fault and is answered by D74 on a start, a sign-in and a decision alike, never
/// with a page that says obop, with the store as it was and no code issued; the next request reads
/// again (a failed refill is not kept) and is answered the same way; after the store recovers the
/// pages are the brand's again.
#[tokio::test]
async fn after_a_good_read_a_failing_store_is_a_503_on_a_start_a_sign_in_and_a_decision() {
    let (flaky, rig) = flaky_rig();
    let (r, o) = (&rig.r, server_of(&rig));
    configure(&rig.app, &FIELDS);
    // A good read through the page itself, so that the cell holds the brand.
    let c = client(r).await;
    let url = authorize_url(&c, REDIRECT, &[]);
    let (good, _) = start(r, &url).await;
    assert_eq!(unbranded(&good), None);
    let filled = std::time::Instant::now();
    // Two flows made without the HTTP layer: one waits for a sign-in, one for a decision.
    let (sign, sign_binder) = begun(&o);
    let (decide, binder) = begun(&o);
    assert_eq!(
        o.login(&decide.flow, &binder, &decide.csrf, &decide.ip, "ada", PW)
            .unwrap(),
        obop_oauth::LoginOut::Consent
    );
    let before = footprint(&rig.app);
    let writes = flaky.seen_writes().len();

    flaky.refuse_model("instance_brand", Mode::Reads);
    wait_until_stale(filled);
    for round in 0..2 {
        let refused_before = flaky.refused();
        let started = call(
            r,
            "GET",
            &url,
            &[("x-forwarded-for", &fresh_address())],
            None,
        )
        .await;
        assert_d74_503(&started);
        assert!(started.header("set-cookie").is_empty(), "no flow was begun");
        assert!(
            flaky.refused() > refused_before,
            "round {round}: the refill read the store again"
        );
        let signed = sign
            .post(
                r,
                "/oauth/authorize/login",
                &[("username", "ada"), ("password", PW)],
            )
            .await;
        assert_d74_503(&signed);
        let decided = decide
            .post(
                r,
                "/oauth/authorize/decision",
                &[
                    ("decision", "allow"),
                    ("days", "30"),
                    ("group", "base.user"),
                ],
            )
            .await;
        assert_d74_503(&decided);
        assert!(decided.header("location").is_empty(), "no code was issued");
    }
    assert_eq!(footprint(&rig.app), before, "the store is as it was");
    assert_eq!(flaky.seen_writes().len(), writes);
    assert_eq!(peek(&o, &sign.flow, &sign_binder).user, None);
    assert_eq!(peek(&o, &decide.flow, &binder).user.as_deref(), Some("ada"));

    flaky.allow_models();
    let (back, _) = start(r, &url).await;
    assert_eq!(unbranded(&back), None);
}

/// While the refill of an expired cell is parked in the store, a page of the flow is answered at
/// once with the last good brand (the caller does not wait for the owner of the refill, spec
/// section 4), and the owner is answered with the brand when it is let go. Every wait is on a
/// condition and has a bound.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn while_a_refill_is_parked_the_pages_of_the_flow_show_the_last_good_brand() {
    let (flaky, rig) = flaky_rig();
    let r = rig.r.clone();
    configure(&rig.app, &FIELDS);
    let c = client(&r).await;
    let url = authorize_url(&c, REDIRECT, &[]);
    let (good, _) = start(&r, &url).await;
    assert_eq!(unbranded(&good), None);
    let filled = std::time::Instant::now();
    tokio::task::spawn_blocking(move || wait_until_stale(filled))
        .await
        .unwrap();

    // The refill that the next request runs parks in the store after it has read the row.
    let (parked_tx, parked_rx) = std::sync::mpsc::channel::<()>();
    let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
    flaky.after_the_get_of(
        "instance_brand",
        0,
        Box::new(move |_| {
            parked_tx.send(()).unwrap();
            // Bounded, and let go at once when the test fails (the sender is dropped with it).
            let _ = go_rx.recv_timeout(std::time::Duration::from_secs(30));
        }),
    );
    let owner = {
        let (r, url, ip) = (r.clone(), url.clone(), fresh_address());
        tokio::spawn(async move { call(&r, "GET", &url, &[("x-forwarded-for", &ip)], None).await })
    };
    let parked = tokio::task::spawn_blocking(move || {
        parked_rx.recv_timeout(std::time::Duration::from_secs(20))
    })
    .await
    .unwrap();
    assert!(
        parked.is_ok(),
        "the request never reached the store: the cell was not stale"
    );

    let bound = std::time::Duration::from_secs(20);
    let started = std::time::Instant::now();
    let (second, _) = start(&r, &url).await;
    assert!(
        started.elapsed() < bound,
        "a page waited for the parked refill: {:?}",
        started.elapsed()
    );
    assert_eq!(unbranded(&second), None, "the last good brand");
    assert!(
        brand_line(&second.text()).contains("MOHV"),
        "the last good name"
    );
    assert!(!owner.is_finished(), "the owner is still in the store");

    go_tx.send(()).unwrap();
    let owner = owner.await.unwrap();
    assert_eq!(owner.status, 200, "{}", owner.text());
    assert_eq!(unbranded(&owner), None);
}
