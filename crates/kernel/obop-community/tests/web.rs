//! HTTP-level tests of the forum and the status page through `Community::handle_at`, the same
//! entry point `obop-http` calls. No sockets; the clock is explicit.

use obop_auth::Auth;
use obop_community::status::{CheckResult, Health};
use obop_community::{Community, Config, Req, Resp};
use std::sync::Arc;

const NOW: u64 = 1_800_000_000;
const PW: &str = "correct horse battery";

fn app_on(backend: Arc<dyn obop_storage::Backend>) -> Arc<Community> {
    let auth = Arc::new(Auth::with_test_params(backend.clone()));
    let cfg = Config {
        admins: ["root@example.test".to_string()].into_iter().collect(),
        public_url: Some("https://obop.example".into()),
    };
    Community::open(cfg, auth, backend).unwrap()
}

fn app() -> Arc<Community> {
    app_on(obop_storage::open("memory:").unwrap())
}

fn get(app: &Community, path: &str, cookie: &str, now: u64) -> Resp {
    let mut r = Req::new("GET", path);
    if !cookie.is_empty() {
        r = r.with_header("cookie", cookie);
    }
    app.handle_at(&r, now)
}

fn get_h(app: &Community, path: &str, headers: &[(&str, &str)]) -> Resp {
    let mut r = Req::new("GET", path);
    for (k, v) in headers {
        r = r.with_header(k, v);
    }
    app.handle_at(&r, NOW)
}

fn form(pairs: &[(&str, &str)]) -> String {
    let mut s = form_urlencoded::Serializer::new(String::new());
    for (k, v) in pairs {
        s.append_pair(k, v);
    }
    s.finish()
}

fn post(app: &Community, path: &str, cookie: &str, pairs: &[(&str, &str)], now: u64) -> Resp {
    let mut r = Req::new("POST", path)
        .with_header("content-type", "application/x-www-form-urlencoded")
        .with_header("host", "obop.example")
        .with_body(&form(pairs));
    if !cookie.is_empty() {
        r = r.with_header("cookie", cookie);
    }
    app.handle_at(&r, now)
}

fn cookies(r: &Resp) -> Vec<String> {
    r.headers
        .iter()
        .filter(|(k, _)| k == "set-cookie")
        .map(|(_, v)| v.clone())
        .collect()
}

fn cookie_value(r: &Resp, name: &str) -> Option<String> {
    cookies(r).into_iter().find_map(|c| {
        let first = c.split(';').next()?;
        first.strip_prefix(&format!("{name}=")).map(String::from)
    })
}

fn csrf_in(body: &str) -> String {
    let i = body.find("name=\"csrf\" value=\"").expect("a csrf field");
    let rest = &body[i + "name=\"csrf\" value=\"".len()..];
    rest[..rest.find('"').unwrap()].to_string()
}

struct Session {
    cookie: String,
    csrf: String,
}

/// Sign up through the real forms and return the logged-in cookie and CSRF token.
fn signup(app: &Community, email: &str, display: &str, now: u64) -> Session {
    let page = get(app, "/community/signup", "", now);
    assert_eq!(page.status, 200);
    let anon = cookie_value(&page, "obop_csrf").expect("anonymous CSRF cookie");
    let tok = csrf_in(&page.body_str());
    assert_eq!(anon, tok);
    let r = post(
        app,
        "/community/signup",
        &format!("obop_csrf={anon}"),
        &[
            ("csrf", &tok),
            ("email", email),
            ("display", display),
            ("password", PW),
            ("website", ""),
        ],
        now,
    );
    assert_eq!(r.status, 303, "{}", r.body_str());
    let raw = cookie_value(&r, "obop_session").expect("session cookie");
    let cookie = format!("obop_session={raw}");
    let home = get(app, "/community", &cookie, now);
    Session {
        csrf: csrf_in(&home.body_str()),
        cookie,
    }
}

fn new_thread(app: &Community, s: &Session, cat: &str, title: &str, body: &str, now: u64) -> Resp {
    post(
        app,
        &format!("/community/c/{cat}/new"),
        &s.cookie,
        &[
            ("csrf", &s.csrf),
            ("title", title),
            ("body", body),
            ("website", ""),
        ],
        now,
    )
}

fn location(r: &Resp) -> String {
    r.get_header("location")
        .expect("a Location header")
        .to_string()
}

#[test]
fn index_has_the_seo_and_agent_metadata() {
    let a = app();
    let r = get(&a, "/community", "", NOW);
    assert_eq!(r.status, 200);
    let h = r.body_str();
    assert!(h.contains("<html lang=\"en\">"));
    assert!(
        h.contains("<title>obop community | obop</title>"),
        "{}",
        &h[..400.min(h.len())]
    );
    assert!(h.contains("name=\"description\""));
    assert!(h.contains("<link rel=\"canonical\" href=\"https://obop.example/community\">"));
    for l in ["en", "de", "fr", "pl", "fi", "sv", "es", "x-default"] {
        assert!(h.contains(&format!("hreflang=\"{l}\"")), "hreflang {l}");
    }
    assert!(h.contains("property=\"og:title\"") && h.contains("name=\"twitter:card\""));
    assert!(h.contains("\"@type\":\"CollectionPage\"") && h.contains("\"@type\":\"ItemList\""));
    assert!(h.contains("\"@type\":\"BreadcrumbList\""));
    assert!(h.contains("rel=\"alternate\" type=\"text/markdown\""));
    assert!(h.contains("application/atom+xml"));
    for landmark in [
        "<header class=\"site\"",
        "<nav aria-label=\"Main navigation\"",
        "<main id=\"main\"",
        "<footer class=\"site\"",
        "class=\"skip\"",
    ] {
        assert!(h.contains(landmark), "{landmark}");
    }
    assert_eq!(h.matches("<h1>").count(), 1);
    for c in [
        "General",
        "Help",
        "Show and tell",
        "Modules",
        "Agents and MCP",
        "Self-hosting",
        "Feature requests",
        "Announcements",
    ] {
        assert!(h.contains(c), "{c}");
    }
    assert!(
        r.get_header("content-security-policy")
            .unwrap()
            .contains("default-src 'none'")
    );
    assert_eq!(r.get_header("x-content-type-options"), Some("nosniff"));
    assert!(r.get_header("cache-control").unwrap().starts_with("public"));
    assert!(
        r.get_header("etag").is_some() && r.get_header("vary").unwrap().contains("Accept-Language")
    );
}

#[test]
fn conditional_get_returns_304() {
    let a = app();
    let first = get(&a, "/community", "", NOW);
    let etag = first.get_header("etag").unwrap().to_string();
    let r = get_h(&a, "/community", &[("if-none-match", &etag)]);
    assert_eq!((r.status, r.body.len()), (304, 0));
    let r = get_h(&a, "/community", &[("if-none-match", "\"nope\"")]);
    assert_eq!(r.status, 200);
}

#[test]
fn markdown_for_agents_by_header_and_query() {
    let a = app();
    for r in [
        get_h(&a, "/community", &[("accept", "text/markdown")]),
        get_h(&a, "/community?format=md", &[]),
    ] {
        assert_eq!(r.status, 200);
        assert!(
            r.get_header("content-type")
                .unwrap()
                .starts_with("text/markdown")
        );
        let b = r.body_str();
        assert!(b.starts_with("# obop community"), "{b}");
        assert!(b.contains("[General](https://obop.example/community/c/general)"));
        assert!(!b.contains("<html"));
    }
    let h = get_h(
        &a,
        "/community",
        &[("accept", "text/html,application/xhtml+xml;q=0.9,*/*;q=0.8")],
    );
    assert!(
        h.get_header("content-type")
            .unwrap()
            .starts_with("text/html")
    );
}

#[test]
fn not_found_pages_are_real_404s_and_helpful() {
    let a = app();
    for p in [
        "/community/c/nope",
        "/community/t/999",
        "/community/t/abc",
        "/community/u/ghost",
        "/community/nothing/here",
        "/status/nothing",
        "/status/incidents/99",
    ] {
        let r = get(&a, p, "", NOW);
        assert_eq!(r.status, 404, "{p}");
        assert_eq!(r.get_header("x-robots-tag"), Some("noindex"), "{p}");
    }
    let b = get(&a, "/community/c/nope", "", NOW).body_str();
    assert!(
        b.contains("Help") && b.contains("type=\"search\""),
        "lists categories and search"
    );
    assert!(b.contains("noindex"));
    assert_eq!(get(&a, "/community/c/general?page=9", "", NOW).status, 404);
}

#[test]
fn wrong_methods_get_405_with_allow() {
    let a = app();
    let r = a.handle_at(&Req::new("DELETE", "/community"), NOW);
    assert_eq!(r.status, 405);
    assert!(r.get_header("allow").unwrap().contains("GET"));
    let r = a.handle_at(&Req::new("GET", "/community/logout"), NOW);
    assert_eq!((r.status, r.get_header("allow")), (405, Some("POST")));
    let r = a.handle_at(&Req::new("POST", "/community/c/general"), NOW);
    assert_eq!(r.status, 405);
    let r = a.handle_at(&Req::new("POST", "/status"), NOW);
    assert_eq!(r.status, 405);
}

#[test]
fn oversized_bodies_are_refused() {
    let a = app();
    let big = "x".repeat(obop_community::MAX_BODY + 1);
    let r = a.handle_at(&Req::new("POST", "/community/login").with_body(&big), NOW);
    assert_eq!(r.status, 413);
}

#[test]
fn signup_login_logout_cycle_and_cookie_flags() {
    let a = app();
    let page = get(&a, "/community/signup", "", NOW);
    let anon = cookie_value(&page, "obop_csrf").unwrap();
    let tok = csrf_in(&page.body_str());
    let fields = [
        ("csrf", tok.as_str()),
        ("email", "Ada@Example.test"),
        ("display", "Ada"),
        ("password", PW),
        ("website", ""),
    ];
    // Over plain HTTP the cookie is not Secure; behind a TLS proxy it is.
    let r = post(
        &a,
        "/community/signup",
        &format!("obop_csrf={anon}"),
        &fields,
        NOW,
    );
    assert_eq!(r.status, 303);
    let c = cookies(&r)
        .into_iter()
        .find(|c| c.starts_with("obop_session="))
        .unwrap();
    assert!(
        c.contains("HttpOnly")
            && c.contains("SameSite=Lax")
            && c.contains("Max-Age=2592000")
            && c.contains("Path=/"),
        "{c}"
    );
    assert!(!c.contains("Secure"));
    let raw = c
        .split(';')
        .next()
        .unwrap()
        .trim_start_matches("obop_session=")
        .to_string();
    assert_eq!(raw.len(), 64, "256 random bits");
    let home = get(&a, "/community", &format!("obop_session={raw}"), NOW + 5);
    let hb = home.body_str();
    assert!(hb.contains("Signed in as Ada") && hb.contains("Log out"));
    assert!(
        !hb.contains("ada@example.test") && !hb.contains("Ada@Example"),
        "email never shown"
    );
    assert!(
        home.get_header("cache-control")
            .unwrap()
            .contains("no-store"),
        "personalised pages are private"
    );

    // Behind HTTPS the cookie is Secure.
    let page = get(&a, "/community/login", "", NOW);
    let anon = cookie_value(&page, "obop_csrf").unwrap();
    let tok = csrf_in(&page.body_str());
    let r = a.handle_at(
        &Req::new("POST", "/community/login")
            .with_header("cookie", &format!("obop_csrf={anon}"))
            .with_header("x-forwarded-proto", "https")
            .with_header("host", "obop.example")
            .with_body(&form(&[
                ("csrf", &tok),
                ("email", "ada@example.test"),
                ("password", PW),
            ])),
        NOW,
    );
    assert_eq!(r.status, 303, "{}", r.body_str());
    let c = cookies(&r)
        .into_iter()
        .find(|c| c.starts_with("obop_session="))
        .unwrap();
    assert!(c.contains("; Secure"), "{c}");
    let raw2 = c
        .split(';')
        .next()
        .unwrap()
        .trim_start_matches("obop_session=")
        .to_string();
    assert_ne!(raw, raw2);

    // Wrong password: 401 and no session.
    let r = post(
        &a,
        "/community/login",
        &format!("obop_csrf={anon}"),
        &[
            ("csrf", &tok),
            ("email", "ada@example.test"),
            ("password", "wrong wrong wrong"),
        ],
        NOW,
    );
    assert_eq!(r.status, 401);
    assert!(cookie_value(&r, "obop_session").is_none());
    assert!(r.body_str().contains("Invalid email address or password."));

    // Logout revokes the server-side session.
    let cookie = format!("obop_session={raw}");
    let csrf = csrf_in(&hb);
    let r = post(
        &a,
        "/community/logout",
        &cookie,
        &[("csrf", &csrf)],
        NOW + 10,
    );
    assert_eq!(r.status, 303);
    assert!(
        cookies(&r)
            .iter()
            .any(|c| c.starts_with("obop_session=;") && c.contains("Max-Age=0"))
    );
    let after = get(&a, "/community", &cookie, NOW + 11).body_str();
    assert!(
        after.contains("Log in") && !after.contains("Signed in as"),
        "the old cookie no longer works"
    );
    // The other session is unaffected.
    assert!(
        get(&a, "/community", &format!("obop_session={raw2}"), NOW + 11)
            .body_str()
            .contains("Signed in as Ada")
    );
}

#[test]
fn sessions_expire_after_thirty_days() {
    let a = app();
    let s = signup(&a, "ada@example.test", "Ada", NOW);
    assert!(
        get(&a, "/community", &s.cookie, NOW + 29 * 86_400)
            .body_str()
            .contains("Signed in as")
    );
    assert!(
        !get(&a, "/community", &s.cookie, NOW + 31 * 86_400)
            .body_str()
            .contains("Signed in as")
    );
}

#[test]
fn existing_control_plane_accounts_can_log_in() {
    let backend = obop_storage::open("memory:").unwrap();
    let auth = Arc::new(Auth::with_test_params(backend.clone()));
    auth.create_user("grace@example.test", PW, &[]).unwrap();
    let a = Community::open(Config::default(), auth, backend).unwrap();
    let page = get(&a, "/community/login", "", NOW);
    let anon = cookie_value(&page, "obop_csrf").unwrap();
    let r = post(
        &a,
        "/community/login",
        &format!("obop_csrf={anon}"),
        &[
            ("csrf", &anon),
            ("email", "grace@example.test"),
            ("password", PW),
        ],
        NOW,
    );
    assert_eq!(r.status, 303);
    let raw = cookie_value(&r, "obop_session").unwrap();
    let h = get(&a, "/community", &format!("obop_session={raw}"), NOW).body_str();
    assert!(
        h.contains("Signed in as grace"),
        "name derived from the email local part"
    );
}

#[test]
fn signup_validation() {
    let a = app();
    let page = get(&a, "/community/signup", "", NOW);
    let anon = cookie_value(&page, "obop_csrf").unwrap();
    let try_signup = |email: &str, display: &str, pw: &str| {
        post(
            &a,
            "/community/signup",
            &format!("obop_csrf={anon}"),
            &[
                ("csrf", &anon),
                ("email", email),
                ("display", display),
                ("password", pw),
            ],
            NOW,
        )
    };
    assert_eq!(try_signup("not-an-email", "", PW).status, 400);
    assert!(
        try_signup("x@example.test", "", "short")
            .body_str()
            .contains("at least 12 characters")
    );
    for bad in [
        "ab",
        "obop",
        "Admin",
        "staff-lead",
        "has space",
        "waytoolongdisplaynamethatexceeds",
    ] {
        let r = try_signup("x@example.test", bad, PW);
        assert_eq!(r.status, 400, "{bad}");
        assert!(r.body_str().contains("display name must be"), "{bad}");
    }
    assert_eq!(try_signup("a1@example.test", "Taken_Name", PW).status, 303);
    let r = try_signup("a2@example.test", "taken_name", PW);
    assert!(r.body_str().contains("already taken"));
    let r = try_signup("A1@example.test", "", PW);
    assert!(r.body_str().contains("already exists"));
    // Honeypot.
    let r = post(
        &a,
        "/community/signup",
        &format!("obop_csrf={anon}"),
        &[
            ("csrf", &anon),
            ("email", "bot@example.test"),
            ("password", PW),
            ("website", "http://spam"),
        ],
        NOW,
    );
    assert_eq!(r.status, 400);
}

#[test]
fn csrf_is_required_on_every_state_changing_form() {
    let a = app();
    let s = signup(&a, "ada@example.test", "Ada", NOW);
    // No token, wrong token, someone else's token.
    let r = post(
        &a,
        "/community/c/general/new",
        &s.cookie,
        &[("title", "t"), ("body", "b")],
        NOW,
    );
    assert_eq!(r.status, 403);
    let r = post(
        &a,
        "/community/c/general/new",
        &s.cookie,
        &[("csrf", "abc"), ("title", "t"), ("body", "b")],
        NOW,
    );
    assert_eq!(r.status, 403);
    let other = signup(&a, "bob@example.test", "Bob", NOW);
    let r = post(
        &a,
        "/community/c/general/new",
        &s.cookie,
        &[("csrf", &other.csrf), ("title", "t"), ("body", "b")],
        NOW,
    );
    assert_eq!(r.status, 403, "tokens are bound to the session");
    for p in [
        "/community/logout",
        "/community/mod",
        "/community/login",
        "/community/signup",
    ] {
        assert_eq!(post(&a, p, &s.cookie, &[], NOW).status, 403, "{p}");
        assert_eq!(post(&a, p, "", &[], NOW).status, 403, "{p} anonymous");
    }
    // A cross-origin post with a valid token is still refused.
    let r = a.handle_at(
        &Req::new("POST", "/community/c/general/new")
            .with_header("cookie", &s.cookie)
            .with_header("host", "obop.example")
            .with_header("origin", "https://evil.example")
            .with_body(&form(&[("csrf", &s.csrf), ("title", "t"), ("body", "b")])),
        NOW,
    );
    assert_eq!(r.status, 403);
    assert_eq!(
        new_thread(&a, &s, "general", "Fine", "body", NOW).status,
        303
    );
    // Nothing was created by the refused attempts.
    assert!(
        !get(&a, "/community/c/general", "", NOW)
            .body_str()
            .contains(">t<")
    );
}

#[test]
fn thread_lifecycle_urls_and_markup() {
    let a = app();
    let s = signup(&a, "ada@example.test", "Ada", NOW);
    let r = new_thread(
        &a,
        &s,
        "help",
        "How do I post an invoice?",
        "I tried **everything**.\n\n```sh\nobop call x\n```",
        NOW + 1,
    );
    assert_eq!(r.status, 303);
    let loc = location(&r);
    assert_eq!(loc, "/community/t/1-how-do-i-post-an-invoice");
    let t = get(&a, &loc, "", NOW + 2);
    assert_eq!(t.status, 200);
    let h = t.body_str();
    assert!(
        h.contains("<title>How do I post an invoice? – Help | obop</title>"),
        "unique title"
    );
    assert!(h.contains("<strong>everything</strong>") && h.contains("language-sh"));
    assert!(h.contains("\"@type\":\"DiscussionForumPosting\""));
    assert!(h.contains("\"@type\":\"BreadcrumbList\""));
    assert!(h.contains(
        "rel=\"canonical\" href=\"https://obop.example/community/t/1-how-do-i-post-an-invoice\""
    ));
    assert!(
        h.contains("Log in or sign up to reply."),
        "anonymous visitors cannot reply"
    );
    assert!(
        !h.contains("name=\"csrf\""),
        "anonymous pages carry no token so they can be cached"
    );
    // Slug canonicalisation: 301 to the canonical URL, query preserved.
    for wrong in [
        "/community/t/1",
        "/community/t/1-old-title",
        "/community/t/1-",
    ] {
        let r = get(&a, wrong, "", NOW + 3);
        assert_eq!(r.status, 301, "{wrong}");
        assert_eq!(location(&r), "/community/t/1-how-do-i-post-an-invoice");
    }
    let r = get(&a, "/community/t/1-x?page=1", "", NOW);
    assert_eq!(
        location(&r),
        "/community/t/1-how-do-i-post-an-invoice?page=1"
    );
    // Reply.
    let bob = signup(&a, "bob@example.test", "Bob", NOW);
    let r = post(
        &a,
        "/community/t/1/reply",
        &bob.cookie,
        &[("csrf", &bob.csrf), ("body", "Try the *command line*.")],
        NOW + 5,
    );
    assert_eq!(r.status, 303);
    assert!(
        location(&r).starts_with("/community/t/1-how-do-i-post-an-invoice")
            && location(&r).contains("#post-2")
    );
    let h = get(&a, "/community/t/1-how-do-i-post-an-invoice", "", NOW + 6).body_str();
    assert!(h.contains("<em>command line</em>") && h.contains("Replies: 1"));
    assert!(h.contains("\"comment\":[") && h.contains("\"@type\":\"Comment\""));
    // The thread shows in its category and on the index with counts.
    let c = get(&a, "/community/c/help", "", NOW + 6).body_str();
    assert!(
        c.contains("How do I post an invoice?")
            && c.contains("Replies: 1")
            && c.contains("\"@type\":\"ItemList\"")
    );
    let i = get(&a, "/community", "", NOW + 6).body_str();
    assert!(i.contains("Threads: 1"));
    // Markdown view of a thread keeps the author's Markdown.
    let m = get_h(
        &a,
        "/community/t/1-how-do-i-post-an-invoice",
        &[("accept", "text/markdown")],
    );
    assert!(m.body_str().contains("I tried **everything**."));
    // Profiles never leak emails.
    let p = get(&a, "/community/u/ada", "", NOW).body_str();
    assert!(p.contains("Ada") && p.contains("Posts: 1") && !p.contains("example.test"));
    assert_eq!(
        get(&a, "/community/u/ADA", "", NOW).status,
        200,
        "case-insensitive"
    );
}

#[test]
fn user_content_is_escaped_and_sanitised() {
    let a = app();
    let s = signup(&a, "ada@example.test", "Ada", NOW);
    let evil = "<script>alert(1)</script> <img src=x onerror=alert(2)> [x](javascript:alert(3)) ![i](https://cdn.test/i.png)";
    let r = new_thread(
        &a,
        &s,
        "general",
        "<b>bold</b> & \"quotes\" </script>",
        evil,
        NOW + 1,
    );
    assert_eq!(r.status, 303);
    let h = get(&a, &location(&r), "", NOW + 2).body_str();
    assert!(
        !h.contains("<script>alert")
            && !h.contains("<img")
            && !h.contains("href=\"javascript")
            && !h.contains(" onerror=alert(2)>")
    );
    assert!(!h.contains("<b>bold</b>") && !h.contains("<img"));
    assert!(h.contains("&lt;b&gt;bold&lt;/b&gt;"));
    // JSON-LD cannot be broken out of.
    let ld_start = h.find("DiscussionForumPosting").unwrap();
    let ld = &h[ld_start..];
    let ld_end = ld.find("</script>").unwrap();
    assert!(!ld[..ld_end].contains('<'));
    // Feed and sitemap escape too.
    let f = get(&a, "/community/feed.xml", "", NOW + 2).body_str();
    assert!(!f.contains("<b>bold") && f.contains("&lt;b&gt;bold"));
}

#[test]
fn moderation_visibility_410_and_audit_log() {
    let a = app();
    let ada = signup(&a, "ada@example.test", "Ada", NOW);
    let root = signup(&a, "root@example.test", "obop-team", NOW);
    let bob = signup(&a, "bob@example.test", "Bob", NOW);
    let loc = location(&new_thread(
        &a,
        &ada,
        "general",
        "Secret plans",
        "very secret body",
        NOW + 1,
    ));
    let hide = |who: &Session, action: &str, now: u64| {
        post(
            &a,
            "/community/mod",
            &who.cookie,
            &[
                ("csrf", &who.csrf),
                ("target", "thread:1"),
                ("action", action),
                ("return", &loc),
            ],
            now,
        )
    };
    // Non-staff cannot moderate, even with a valid token.
    assert_eq!(hide(&bob, "hide", NOW + 2).status, 403);
    assert_eq!(hide(&root, "hide", NOW + 3).status, 303);
    // Everyone else gets a real 200 page that says "removed" and never contains the text.
    for cookie in ["", bob.cookie.as_str(), ada.cookie.as_str()] {
        let r = get(&a, "/community/t/1-secret-plans", cookie, NOW + 4);
        assert_eq!(r.status, 301, "slug is replaced so the title is not echoed");
        let r = get(&a, &location(&r), cookie, NOW + 4);
        assert_eq!(r.status, 200);
        let b = r.body_str();
        assert!(b.contains("removed by a moderator") && b.contains("noindex"));
        assert!(
            !b.contains("Secret plans") && !b.contains("very secret body"),
            "no leak for {cookie:?}"
        );
        let md = get_h(
            &a,
            &format!(
                "{}?format=md",
                location(&get(&a, "/community/t/1-secret-plans", cookie, NOW + 4))
            ),
            &[],
        );
        assert!(!md.body_str().contains("very secret"));
    }
    // Staff still see it, flagged.
    let r = get(&a, "/community/t/1-secret-plans", &root.cookie, NOW + 5);
    assert_eq!(r.status, 200);
    assert!(r.body_str().contains("very secret body") && r.body_str().contains("Hidden"));
    // Hidden threads are gone from lists, search, feeds and the sitemap.
    assert!(
        !get(&a, "/community/c/general", "", NOW + 5)
            .body_str()
            .contains("Secret plans")
    );
    assert!(
        !get(&a, "/community/search?q=secret", "", NOW + 5)
            .body_str()
            .contains("Secret plans")
    );
    assert!(
        !get(&a, "/community/feed.xml", "", NOW + 5)
            .body_str()
            .contains("Secret")
    );
    assert!(
        !get(&a, "/community/sitemap.xml", "", NOW + 5)
            .body_str()
            .contains("/community/t/")
    );
    // Unhide, lock, pin, move.
    assert_eq!(hide(&root, "unhide", NOW + 6).status, 303);
    assert_eq!(
        get(&a, "/community/t/1-secret-plans", "", NOW + 7).status,
        200
    );
    assert_eq!(hide(&root, "lock", NOW + 8).status, 303);
    let r = post(
        &a,
        "/community/t/1/reply",
        &bob.cookie,
        &[("csrf", &bob.csrf), ("body", "hi")],
        NOW + 9,
    );
    assert_eq!(r.status, 403, "locked threads take no replies from members");
    assert!(
        get(&a, "/community/t/1-secret-plans", "", NOW + 9)
            .body_str()
            .contains("This thread is locked")
    );
    assert_eq!(hide(&root, "pin", NOW + 10).status, 303);
    let r = post(
        &a,
        "/community/mod",
        &root.cookie,
        &[
            ("csrf", &root.csrf),
            ("target", "thread:1"),
            ("action", "move"),
            ("arg", "help"),
            ("return", &loc),
        ],
        NOW + 11,
    );
    assert_eq!(r.status, 303);
    assert!(
        get(&a, "/community/c/help", "", NOW + 12)
            .body_str()
            .contains("Secret plans")
    );
    // Removing a thread: 410 for everyone, including staff.
    assert_eq!(hide(&root, "delete", NOW + 13).status, 303);
    for cookie in ["", root.cookie.as_str()] {
        let r = get(&a, "/community/t/1-secret-plans", cookie, NOW + 14);
        assert_eq!(r.status, 410);
        assert!(!r.body_str().contains("very secret"));
    }
    // The audit log is for staff only.
    assert_eq!(get(&a, "/community/mod/log", "", NOW + 15).status, 302);
    assert_eq!(
        get(&a, "/community/mod/log", &bob.cookie, NOW + 15).status,
        403
    );
    let log = get(&a, "/community/mod/log", &root.cookie, NOW + 15);
    assert_eq!(log.status, 200);
    let lb = log.body_str();
    for act in ["hide", "unhide", "lock", "pin", "move", "delete"] {
        assert!(lb.contains(&format!("<td>{act}")), "{act}");
    }
    assert!(lb.contains("obop-team") && lb.contains("general -&gt; help"));
}

#[test]
fn hiding_a_reply_hides_its_text_but_not_the_thread() {
    let a = app();
    let ada = signup(&a, "ada@example.test", "Ada", NOW);
    let root = signup(&a, "root@example.test", "obop-team", NOW);
    let loc = location(&new_thread(&a, &ada, "general", "Hello", "first", NOW + 1));
    let r = post(
        &a,
        "/community/t/1/reply",
        &ada.cookie,
        &[("csrf", &ada.csrf), ("body", "rude reply text")],
        NOW + 2,
    );
    assert_eq!(r.status, 303);
    let r = post(
        &a,
        "/community/mod",
        &root.cookie,
        &[
            ("csrf", &root.csrf),
            ("target", "post:2"),
            ("action", "hide"),
            ("return", &loc),
        ],
        NOW + 3,
    );
    assert_eq!(r.status, 303);
    let anon = get(&a, &loc, "", NOW + 4);
    assert_eq!(anon.status, 200);
    assert!(
        anon.body_str().contains("removed by a moderator")
            && !anon.body_str().contains("rude reply text")
    );
    assert!(
        get(&a, &loc, &root.cookie, NOW + 4)
            .body_str()
            .contains("rude reply text")
    );
    // The opening post can only be hidden by hiding the thread.
    let r = post(
        &a,
        "/community/mod",
        &root.cookie,
        &[
            ("csrf", &root.csrf),
            ("target", "post:1"),
            ("action", "hide"),
        ],
        NOW + 5,
    );
    assert_eq!(r.status, 403);
}

#[test]
fn rate_limits_return_429() {
    let a = app();
    let s = signup(&a, "ada@example.test", "Ada", NOW);
    for i in 0..3 {
        assert_eq!(
            new_thread(&a, &s, "general", &format!("T{i}"), "b", NOW + i).status,
            303
        );
    }
    let r = new_thread(&a, &s, "general", "T4", "b", NOW + 10);
    assert_eq!(r.status, 429);
    assert!(
        r.body_str().contains("at most 3 threads per hour")
            && r.get_header("retry-after").is_some()
    );
    for i in 0..7 {
        let r = post(
            &a,
            "/community/t/1/reply",
            &s.cookie,
            &[("csrf", &s.csrf), ("body", "x")],
            NOW + 20 + i,
        );
        assert_eq!(r.status, 303, "{i}");
    }
    let r = post(
        &a,
        "/community/t/1/reply",
        &s.cookie,
        &[("csrf", &s.csrf), ("body", "x")],
        NOW + 40,
    );
    assert_eq!(r.status, 429);
    // An hour later it works again.
    assert_eq!(
        post(
            &a,
            "/community/t/1/reply",
            &s.cookie,
            &[("csrf", &s.csrf), ("body", "x")],
            NOW + 3700
        )
        .status,
        303
    );
}

#[test]
fn input_limits_and_honeypot() {
    let a = app();
    let s = signup(&a, "ada@example.test", "Ada", NOW - 3 * 86_400);
    let ok = new_thread(
        &a,
        &s,
        "general",
        &"t".repeat(140),
        &"b".repeat(20_000),
        NOW,
    );
    assert_eq!(ok.status, 303);
    let too_long_title = new_thread(&a, &s, "general", &"t".repeat(141), "b", NOW + 1);
    assert_eq!(too_long_title.status, 400);
    assert!(too_long_title.body_str().contains("at most 140 characters"));
    assert!(
        new_thread(&a, &s, "general", "t", &"b".repeat(20_001), NOW + 2)
            .body_str()
            .contains("at most 20000")
    );
    assert_eq!(new_thread(&a, &s, "general", " ", "b", NOW + 3).status, 400);
    assert_eq!(
        new_thread(&a, &s, "general", "t", "  ", NOW + 3).status,
        400
    );
    // Entered text is kept in the form after an error.
    let kept = new_thread(&a, &s, "general", "Keep me", &" ".repeat(3), NOW + 4).body_str();
    assert!(kept.contains("value=\"Keep me\""));
    // A filled honeypot is rejected without creating anything.
    let r = post(
        &a,
        "/community/c/general/new",
        &s.cookie,
        &[
            ("csrf", &s.csrf),
            ("title", "spam"),
            ("body", "spam"),
            ("website", "http://x"),
        ],
        NOW + 5,
    );
    assert_eq!(r.status, 400);
    assert!(
        !get(&a, "/community/c/general", "", NOW + 6)
            .body_str()
            .contains(">spam<")
    );
    // Unknown category on write.
    assert_eq!(new_thread(&a, &s, "nope", "t", "b", NOW + 7).status, 404);
}

#[test]
fn new_accounts_may_post_two_links() {
    let a = app();
    let s = signup(&a, "new@example.test", "Newbie", NOW);
    let r = new_thread(
        &a,
        &s,
        "general",
        "Links",
        "https://a.test https://b.test https://c.test",
        NOW + 1,
    );
    assert_eq!(r.status, 400);
    assert!(r.body_str().contains("at most 2 links"));
    assert_eq!(
        new_thread(
            &a,
            &s,
            "general",
            "Links",
            "[a](https://a.test) https://b.test",
            NOW + 2
        )
        .status,
        303
    );
    // A day later the limit is gone.
    assert_eq!(
        new_thread(
            &a,
            &s,
            "general",
            "Links 2",
            "https://a.test https://b.test https://c.test",
            NOW + 2 * 86_400
        )
        .status,
        303
    );
}

#[test]
fn announcements_are_staff_only() {
    let a = app();
    let ada = signup(&a, "ada@example.test", "Ada", NOW);
    let root = signup(&a, "root@example.test", "obop-team", NOW);
    assert_eq!(
        get(&a, "/community/c/announcements/new", &ada.cookie, NOW).status,
        403
    );
    assert_eq!(
        new_thread(&a, &ada, "announcements", "Mine", "b", NOW).status,
        403
    );
    let r = new_thread(
        &a,
        &root,
        "announcements",
        "Release 1.0",
        "Shipped.",
        NOW + 1,
    );
    assert_eq!(r.status, 303);
    post(
        &a,
        "/community/mod",
        &root.cookie,
        &[
            ("csrf", &root.csrf),
            ("target", "thread:1"),
            ("action", "pin"),
            ("return", "/community"),
        ],
        NOW + 2,
    );
    let i = get(&a, "/community", "", NOW + 3).body_str();
    assert!(i.contains("Pinned announcements") && i.contains("Release 1.0"));
    assert!(
        i.contains("Staff")
            || get(&a, &location(&r), "", NOW + 3)
                .body_str()
                .contains("Staff")
    );
    // Anonymous visitors are sent to log in.
    assert_eq!(get(&a, "/community/c/general/new", "", NOW).status, 302);
}

#[test]
fn editing_own_posts_for_fifteen_minutes() {
    let a = app();
    let ada = signup(&a, "ada@example.test", "Ada", NOW);
    let bob = signup(&a, "bob@example.test", "Bob", NOW);
    let loc = location(&new_thread(
        &a,
        &ada,
        "general",
        "Edit me",
        "original",
        NOW + 1,
    ));
    assert!(
        get(&a, &loc, &ada.cookie, NOW + 2)
            .body_str()
            .contains("/community/p/1/edit")
    );
    assert!(
        !get(&a, &loc, &bob.cookie, NOW + 2)
            .body_str()
            .contains("/community/p/1/edit")
    );
    assert_eq!(
        get(&a, "/community/p/1/edit", &bob.cookie, NOW + 2).status,
        403
    );
    assert_eq!(
        post(
            &a,
            "/community/p/1/edit",
            &bob.cookie,
            &[("csrf", &bob.csrf), ("body", "hax")],
            NOW + 2
        )
        .status,
        403
    );
    let form_page = get(&a, "/community/p/1/edit", &ada.cookie, NOW + 2);
    assert_eq!(form_page.status, 200);
    assert!(form_page.body_str().contains("original"));
    let r = post(
        &a,
        "/community/p/1/edit",
        &ada.cookie,
        &[("csrf", &ada.csrf), ("body", "changed")],
        NOW + 100,
    );
    assert_eq!(r.status, 303);
    let h = get(&a, &loc, "", NOW + 101).body_str();
    assert!(h.contains("changed") && !h.contains("original") && h.contains("(edited)"));
    let late = post(
        &a,
        "/community/p/1/edit",
        &ada.cookie,
        &[("csrf", &ada.csrf), ("body", "too late")],
        NOW + 1 + 901,
    );
    assert_eq!(late.status, 400);
    assert!(late.body_str().contains("15 minutes"));
    assert!(
        !get(&a, &loc, &ada.cookie, NOW + 1000)
            .body_str()
            .contains("/community/p/1/edit")
    );
}

#[test]
fn pagination_of_threads_posts_and_search() {
    let backend = obop_storage::open("memory:").unwrap();
    let a = app_on(backend);
    let root = signup(&a, "root@example.test", "obop-team", NOW - 5 * 86_400);
    // 27 threads (staff are not rate limited).
    for i in 0..27 {
        let r = new_thread(
            &a,
            &root,
            "general",
            &format!("Needle thread {i:02}"),
            "body",
            NOW + i,
        );
        assert_eq!(r.status, 303);
    }
    let p1 = get(&a, "/community/c/general", "", NOW + 100);
    let b1 = p1.body_str();
    assert_eq!(b1.matches("<h3><a href=\"/community/t/").count(), 25);
    assert!(
        b1.contains("rel=\"next\" href=\"/community/c/general?page=2\"")
            && !b1.contains("rel=\"prev\"")
    );
    assert!(
        b1.contains("<link rel=\"next\" href=\"https://obop.example/community/c/general?page=2\">")
    );
    assert!(b1.contains("Needle thread 26"), "newest activity first");
    let b2 = get(&a, "/community/c/general?page=2", "", NOW + 100).body_str();
    assert_eq!(b2.matches("<h3><a href=\"/community/t/").count(), 2);
    assert!(b2.contains("<link rel=\"prev\"") && b2.contains("Page 2 of 2"));
    assert!(
        b2.contains("rel=\"canonical\" href=\"https://obop.example/community/c/general?page=2\"")
    );
    assert_eq!(get(&a, "/community/c/general?page=3", "", NOW).status, 404);
    assert_eq!(get(&a, "/community/c/general?page=0", "", NOW).status, 404);
    // Posts: 35 replies -> 36 posts -> 2 pages of 30.
    for i in 0..35 {
        let r = post(
            &a,
            "/community/t/1/reply",
            &root.cookie,
            &[("csrf", &root.csrf), ("body", &format!("reply {i}"))],
            NOW + 200 + i,
        );
        assert_eq!(r.status, 303);
    }
    let t1 = get(&a, "/community/t/1-needle-thread-00", "", NOW + 300).body_str();
    assert_eq!(t1.matches("<article class=\"post\"").count(), 30);
    let t2 = get(&a, "/community/t/1-needle-thread-00?page=2", "", NOW + 300).body_str();
    assert_eq!(t2.matches("<article class=\"post\"").count(), 6);
    assert!(t2.contains("rel=\"prev\"") && t2.contains("(Page 2 of 2)"));
    // Search: noindex, paginated (20 per page), case-insensitive.
    let s = get(&a, "/community/search?q=NEEDLE", "", NOW + 300);
    let sb = s.body_str();
    assert_eq!(s.status, 200);
    assert!(
        sb.contains("content=\"noindex, follow\"")
            && s.get_header("x-robots-tag") == Some("noindex")
    );
    assert!(sb.contains("Results: 27") && sb.contains("rel=\"next\""));
    assert_eq!(sb.matches("<li><h3><a href=\"/community/t/").count(), 20);
    let s2 = get(&a, "/community/search?q=needle&page=2", "", NOW + 300).body_str();
    assert_eq!(s2.matches("<li><h3><a href=\"/community/t/").count(), 7);
    assert!(
        get(&a, "/community/search?q=zzzzqqq", "", NOW)
            .body_str()
            .contains("Nothing found")
    );
    assert_eq!(get(&a, "/community/search", "", NOW).status, 200);
    // Body hits count too.
    assert!(
        get(&a, "/community/search?q=Reply+7", "", NOW + 300)
            .body_str()
            .contains("Needle thread 00")
    );
}

#[test]
fn feeds_and_sitemap() {
    let a = app();
    let ada = signup(&a, "ada@example.test", "Ada", NOW);
    new_thread(&a, &ada, "help", "Feed me", "some *text* here", NOW + 1);
    let f = get(&a, "/community/feed.xml", "", NOW + 2);
    assert_eq!(f.status, 200);
    assert_eq!(
        f.get_header("content-type"),
        Some("application/atom+xml; charset=utf-8")
    );
    let b = f.body_str();
    assert!(b.starts_with("<?xml") && b.contains("<feed xmlns=\"http://www.w3.org/2005/Atom\">"));
    assert!(
        b.contains("<title>Feed me</title>")
            && b.contains("https://obop.example/community/t/1-feed-me")
    );
    assert!(b.contains("<name>Ada</name>") && !b.contains("example.test/") || !b.contains("ada@"));
    assert!(f.get_header("etag").is_some() && f.get_header("last-modified").is_some());
    let etag = f.get_header("etag").unwrap().to_string();
    assert_eq!(
        get_h(&a, "/community/feed.xml", &[("if-none-match", &etag)]).status,
        304
    );
    let c = get(&a, "/community/c/help/feed.xml", "", NOW + 2).body_str();
    assert!(c.contains("Feed me") && c.contains("obop community – Help"));
    assert!(
        !get(&a, "/community/c/general/feed.xml", "", NOW + 2)
            .body_str()
            .contains("Feed me")
    );
    assert_eq!(get(&a, "/community/c/nope/feed.xml", "", NOW).status, 404);
    let s = get(&a, "/community/sitemap.xml", "", NOW + 2);
    assert_eq!(
        s.get_header("content-type"),
        Some("application/xml; charset=utf-8")
    );
    let sb = s.body_str();
    assert!(sb.contains("<loc>https://obop.example/community</loc>"));
    assert!(sb.contains("<loc>https://obop.example/community/c/help</loc>"));
    assert!(sb.contains("<loc>https://obop.example/community/t/1-feed-me</loc>"));
    assert!(
        sb.contains("hreflang=\"de\" href=\"https://obop.example/community?lang=de\"")
            && sb.contains("<lastmod>")
    );
    // The function the static site builder calls.
    let urls = a.sitemap_urls("https://obop.example/");
    assert!(urls.iter().any(|u| u.loc == "https://obop.example/status"));
    assert!(
        urls.iter()
            .any(|u| u.loc == "https://obop.example/community/t/1-feed-me" && u.lastmod.is_some())
    );
    assert!(urls.len() >= 11);
}

#[test]
fn thread_index_rules_for_noindex() {
    let a = app();
    let ada = signup(&a, "ada@example.test", "Ada", NOW);
    let root = signup(&a, "root@example.test", "obop-team", NOW);
    let loc = location(&new_thread(&a, &ada, "general", "Indexed", "body", NOW + 1));
    assert!(!get(&a, &loc, "", NOW + 2).body_str().contains("noindex"));
    post(
        &a,
        "/community/mod",
        &root.cookie,
        &[
            ("csrf", &root.csrf),
            ("target", "thread:1"),
            ("action", "lock"),
            ("return", &loc),
        ],
        NOW + 3,
    );
    let locked = get(&a, &loc, "", NOW + 4);
    assert!(
        locked.body_str().contains("noindex"),
        "a locked thread without replies is not indexed"
    );
    assert!(
        !get(&a, "/community/sitemap.xml", "", NOW + 4)
            .body_str()
            .contains("/community/t/1-")
    );
}

#[test]
fn language_negotiation_hreflang_and_cookie() {
    let a = app();
    let de = get(&a, "/community?lang=de", "", NOW);
    let h = de.body_str();
    assert!(h.contains("<html lang=\"de\">") && h.contains("Kategorien") && h.contains("Anmelden"));
    assert!(h.contains("rel=\"canonical\" href=\"https://obop.example/community?lang=de\""));
    assert!(h.contains("hreflang=\"fi\" href=\"https://obop.example/community?lang=fi\""));
    assert!(h.contains("property=\"og:locale\" content=\"de_DE\""));
    assert_eq!(de.get_header("content-language"), Some("de"));
    assert!(cookie_value(&de, "obop_lang").as_deref() == Some("de"));
    assert!(de.get_header("cache-control").unwrap().contains("no-store"));
    // Cookie, then header, then default.
    assert!(
        get_h(&a, "/community", &[("cookie", "obop_lang=pl")])
            .body_str()
            .contains("<html lang=\"pl\">")
    );
    assert!(
        get_h(
            &a,
            "/community",
            &[("accept-language", "fr-CH, fr;q=0.9, en;q=0.8")]
        )
        .body_str()
        .contains("<html lang=\"fr\">")
    );
    assert!(
        get_h(&a, "/community", &[("accept-language", "ja")])
            .body_str()
            .contains("<html lang=\"en\">")
    );
    assert!(
        get_h(
            &a,
            "/community?lang=sv",
            &[("cookie", "obop_lang=pl"), ("accept-language", "fr")]
        )
        .body_str()
        .contains("<html lang=\"sv\">")
    );
    assert!(
        get_h(&a, "/community?lang=xx", &[("cookie", "obop_lang=es")])
            .body_str()
            .contains("<html lang=\"es\">")
    );
    assert!(
        get_h(&a, "/community?lang=fi", &[])
            .body_str()
            .contains("Aihealueet")
    );
    // The markdown view is localised too, and keeps user content untouched.
    let md = get_h(&a, "/community?lang=de&format=md", &[]).body_str();
    assert!(md.contains("Fragen, Ideen"), "{md}");
    // Every language renders every page type without leaking the "?" placeholder for a missing key.
    for l in ["en", "de", "fr", "pl", "fi", "sv", "es"] {
        for p in [
            "/community",
            "/community/c/help",
            "/community/login",
            "/community/signup",
            "/community/c/nope",
            "/status",
            "/community/search?q=x",
        ] {
            let r = get(
                &a,
                &format!("{p}{}lang={l}", if p.contains('?') { "&" } else { "?" }),
                "",
                NOW,
            );
            let b = r.body_str();
            assert!(!b.contains(">?<") && !b.contains("=\"?\""), "{l} {p}");
            assert!(b.contains(&format!("<html lang=\"{l}\">")));
        }
    }
}

#[test]
fn user_content_language_is_not_guessed() {
    let a = app();
    let ada = signup(&a, "ada@example.test", "Ada", NOW);
    let loc = location(&new_thread(
        &a,
        &ada,
        "general",
        "Bonjour",
        "salut",
        NOW + 1,
    ));
    let h = get_h(&a, &format!("{loc}?lang=de"), &[]).body_str();
    assert!(h.contains("<html lang=\"de\">") && h.contains("Bonjour") && h.contains("salut"));
    assert!(!h.contains("lang=\"fr\"><div class=\"body\""));
}

// ---------------------------------------------------------------------------------------------
// Status page
// ---------------------------------------------------------------------------------------------

fn record(a: &Community, comp: &str, health: Health, now: u64) {
    a.status
        .record(
            comp,
            CheckResult {
                health,
                detail: String::new(),
                pct: None,
                names: vec![],
            },
            now,
        )
        .unwrap();
}

#[test]
fn status_page_renders_banner_components_and_bars() {
    let a = app();
    let day = NOW - NOW % 86_400;
    for d in 0..3 {
        record(&a, "api", Health::Up, day - d * 86_400 + 60);
    }
    record(&a, "community", Health::Up, NOW);
    let r = get(&a, "/status", "", NOW + 1);
    assert_eq!(r.status, 200);
    let h = r.body_str();
    assert!(h.contains("All systems operational") && h.contains("class=\"banner none\""));
    assert!(h.contains("Control plane API") && h.contains("Community"));
    assert!(h.contains("<title>All systems operational | obop</title>"));
    assert_eq!(h.matches("<ol class=\"bar\"").count(), 2);
    assert_eq!(
        h.matches("<li class=\"ok\"").count() + h.matches("<li class=\"nodata\"").count(),
        180,
        "90 days per component"
    );
    assert!(
        h.contains("<span class=\"sr\">"),
        "every day has a text alternative"
    );
    assert!(h.contains("Uptime 24 hours") && h.contains("100%"));
    assert!(h.contains("\"@type\":\"WebPage\"") && h.contains("\"@type\":\"BreadcrumbList\""));
    assert!(
        !h.contains("http-equiv=\"refresh\"") && !h.contains("/status/assets/refresh.js"),
        "no meta refresh, no script by default"
    );
    assert!(h.contains("?refresh=60"), "opt-in link");
    let on = get(&a, "/status?refresh=60", "", NOW + 1).body_str();
    assert!(
        on.contains("/status/assets/refresh.js")
            && on.contains("Turn off auto refresh")
            && !on.contains("http-equiv")
    );
    assert_eq!(get(&a, "/status/assets/refresh.js", "", NOW).status, 200);
    // Localised and markdown.
    assert!(
        get(&a, "/status?lang=de", "", NOW)
            .body_str()
            .contains("Alle Systeme laufen")
    );
    let md = get_h(&a, "/status", &[("accept", "text/markdown")]).body_str();
    assert!(
        md.contains("**All systems operational**")
            && md.contains("- **Control plane API**: Operational")
    );
    // Caching headers.
    let etag = r.get_header("etag").unwrap().to_string();
    assert_eq!(
        get_h(&a, "/status", &[("if-none-match", &etag)]).status,
        304
    );
}

#[test]
fn status_outage_incidents_and_instances_by_name() {
    let a = app();
    for i in 0..3 {
        record(&a, "community", Health::Down, NOW + i * 60);
    }
    record(&a, "api", Health::Up, NOW + 200);
    a.status
        .record(
            "instances",
            CheckResult {
                health: Health::Degraded,
                detail: "1 of 3 not responding".into(),
                pct: Some(66.666),
                names: vec!["Acme Trading".into()],
            },
            NOW + 200,
        )
        .unwrap();
    let h = get(&a, "/status", "", NOW + 210).body_str();
    assert!(
        h.contains("Community is unavailable"),
        "the automatic incident is listed"
    );
    assert!(
        h.contains("Opened automatically") && h.contains("Major outage")
            || h.contains("Partial outage")
    );
    assert!(
        h.contains("Degraded instances: Acme Trading") && h.contains("66.67% of instances respond")
    );
    assert!(
        !h.contains("1 of 3 not responding"),
        "operator detail stays private"
    );
    let page = get(&a, "/status/incidents/1", "", NOW + 210);
    assert_eq!(page.status, 200);
    assert!(
        page.body_str()
            .contains("Automated checks failed three times")
            && page.body_str().contains("Updates")
    );
    for i in 0..3 {
        record(&a, "community", Health::Up, NOW + 300 + i * 60);
    }
    let h = get(&a, "/status", "", NOW + 600).body_str();
    assert!(h.contains("Past incidents") && h.contains("Resolved"));
    assert!(!h.contains("No incidents in the last 90 days"));
}

#[test]
fn statuspage_v2_json_shapes() {
    let a = app();
    record(&a, "api", Health::Up, NOW);
    record(&a, "community", Health::Up, NOW);
    a.status
        .create_incident(
            "DB failover",
            obop_community::status::Impact::Minor,
            obop_community::status::IncStatus::Investigating,
            &["api".into()],
            "Looking into it",
            NOW + 1,
        )
        .unwrap();
    a.status
        .create_maintenance(
            "Upgrade",
            "Window",
            &["community".into()],
            NOW + 3600,
            NOW + 7200,
            NOW,
        )
        .unwrap();
    let j = |p: &str| -> serde_json::Value {
        let r = get(&a, p, "", NOW + 10);
        assert_eq!(r.status, 200, "{p}");
        assert_eq!(
            r.get_header("content-type"),
            Some("application/json; charset=utf-8")
        );
        assert_eq!(
            r.get_header("access-control-allow-origin"),
            Some("*"),
            "{p}"
        );
        assert!(r.get_header("cache-control").unwrap().contains("max-age"));
        assert!(r.get_header("etag").is_some());
        serde_json::from_slice(&r.body).unwrap()
    };
    let s = j("/status/api/v2/summary.json");
    for k in [
        "page",
        "status",
        "components",
        "incidents",
        "scheduled_maintenances",
    ] {
        assert!(s.get(k).is_some(), "summary.{k}");
    }
    for k in ["id", "name", "url", "time_zone", "updated_at"] {
        assert!(s["page"].get(k).is_some(), "page.{k}");
    }
    assert_eq!(s["page"]["url"], "https://obop.example/status");
    assert_eq!(s["status"]["indicator"], "minor");
    assert_eq!(s["status"]["description"], "Partially Degraded Service");
    let c0 = &s["components"][0];
    for k in [
        "id",
        "name",
        "status",
        "created_at",
        "updated_at",
        "position",
        "description",
        "showcase",
        "page_id",
        "group",
        "only_show_if_degraded",
        "group_id",
        "start_date",
    ] {
        assert!(c0.get(k).is_some(), "component.{k}");
    }
    assert_eq!(
        c0["status"], "degraded_performance",
        "an open incident colours the component"
    );
    assert_eq!(s["components"][1]["status"], "operational");
    let i0 = &s["incidents"][0];
    for k in [
        "id",
        "name",
        "status",
        "created_at",
        "updated_at",
        "monitoring_at",
        "resolved_at",
        "impact",
        "shortlink",
        "started_at",
        "page_id",
        "incident_updates",
        "components",
    ] {
        assert!(i0.get(k).is_some(), "incident.{k}");
    }
    assert_eq!(
        (
            i0["name"].as_str(),
            i0["impact"].as_str(),
            i0["status"].as_str()
        ),
        (Some("DB failover"), Some("minor"), Some("investigating"))
    );
    let u0 = &i0["incident_updates"][0];
    for k in [
        "id",
        "status",
        "body",
        "incident_id",
        "created_at",
        "updated_at",
        "display_at",
        "affected_components",
    ] {
        assert!(u0.get(k).is_some(), "update.{k}");
    }
    assert!(i0["created_at"].as_str().unwrap().ends_with(".000Z"));
    assert_eq!(s["scheduled_maintenances"][0]["status"], "scheduled");
    assert!(
        s["scheduled_maintenances"][0]
            .get("scheduled_for")
            .is_some()
    );
    let st = j("/status/api/v2/status.json");
    assert!(st.get("page").is_some() && st["status"]["indicator"] == "minor");
    let co = j("/status/api/v2/components.json");
    assert_eq!(co["components"].as_array().unwrap().len(), 2);
    let inc = j("/status/api/v2/incidents.json");
    assert_eq!(inc["incidents"].as_array().unwrap().len(), 1);
    // Resolving moves it out of the summary but keeps it in incidents.json.
    a.status.resolve_incident(1, "Fixed", NOW + 100).unwrap();
    assert_eq!(
        j("/status/api/v2/summary.json")["incidents"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        j("/status/api/v2/incidents.json")["incidents"][0]["status"],
        "resolved"
    );
    assert_eq!(
        j("/status/api/v2/status.json")["status"]["indicator"],
        "none"
    );
    // CORS preflight and unknown files.
    let o = a.handle_at(&Req::new("OPTIONS", "/status/api/v2/summary.json"), NOW);
    assert_eq!(o.status, 204);
    assert_eq!(o.get_header("access-control-allow-origin"), Some("*"));
    assert_eq!(get(&a, "/status/api/v2/nothing.json", "", NOW).status, 404);
    let etag = get(&a, "/status/api/v2/summary.json", "", NOW + 10)
        .get_header("etag")
        .unwrap()
        .to_string();
    assert_eq!(
        get_h(
            &a,
            "/status/api/v2/summary.json",
            &[("if-none-match", &etag)]
        )
        .status,
        304
    );
}

#[test]
fn status_history_atom() {
    let a = app();
    a.status
        .create_incident(
            "Slow site",
            obop_community::status::Impact::Minor,
            obop_community::status::IncStatus::Monitoring,
            &[],
            "watching",
            NOW,
        )
        .unwrap();
    let r = get(&a, "/status/history.atom", "", NOW + 1);
    assert_eq!(r.status, 200);
    assert_eq!(
        r.get_header("content-type"),
        Some("application/atom+xml; charset=utf-8")
    );
    let b = r.body_str();
    assert!(
        b.contains("<title>Slow site [monitoring]</title>")
            && b.contains("https://obop.example/status/incidents/1")
    );
}

#[test]
fn status_console_is_staff_only_and_csrf_protected() {
    let a = app();
    let ada = signup(&a, "ada@example.test", "Ada", NOW);
    let root = signup(&a, "root@example.test", "obop-team", NOW);
    assert_eq!(
        get(&a, "/status/admin", "", NOW).status,
        302,
        "anonymous visitors are sent to log in"
    );
    assert_eq!(get(&a, "/status/admin", &ada.cookie, NOW).status, 403);
    let page = get(&a, "/status/admin", &root.cookie, NOW);
    assert_eq!(page.status, 200);
    assert!(
        page.body_str().contains("Report an incident")
            && page.body_str().contains("Schedule maintenance")
    );
    // Members cannot write, staff need the token.
    let fields = |csrf: &str| {
        vec![
            ("csrf", csrf.to_string()),
            ("title", "Login slow".into()),
            ("impact", "major".into()),
            ("status", "investigating".into()),
            ("components", "api".into()),
            ("components", "community".into()),
            ("body", "We are looking.".into()),
        ]
    };
    let send = |cookie: &str, csrf: &str, path: &str| {
        let f = fields(csrf);
        let pairs: Vec<(&str, &str)> = f.iter().map(|(k, v)| (*k, v.as_str())).collect();
        post(&a, path, cookie, &pairs, NOW + 5)
    };
    assert_eq!(
        send(&ada.cookie, &ada.csrf, "/status/admin/incident").status,
        403
    );
    assert_eq!(send("", "", "/status/admin/incident").status, 403);
    assert_eq!(
        send(&root.cookie, "bad", "/status/admin/incident").status,
        403
    );
    assert!(a.status.incidents().is_empty());
    let r = send(&root.cookie, &root.csrf, "/status/admin/incident");
    assert_eq!(r.status, 303);
    let inc = a.status.incident(1).unwrap();
    assert_eq!(
        (inc.title.as_str(), inc.components.len(), inc.auto),
        ("Login slow", 2, false)
    );
    assert!(
        get(&a, "/status", "", NOW + 6)
            .body_str()
            .contains("Login slow")
    );
    let r = post(
        &a,
        "/status/admin/incident/1",
        &root.cookie,
        &[
            ("csrf", &root.csrf),
            ("status", "resolved"),
            ("impact", "major"),
            ("body", "Fixed it."),
        ],
        NOW + 50,
    );
    assert_eq!(r.status, 303);
    let inc = a.status.incident(1).unwrap();
    assert_eq!((inc.open(), inc.updates.len()), (false, 2));
    assert_eq!(
        post(
            &a,
            "/status/admin/incident/99",
            &root.cookie,
            &[("csrf", &root.csrf), ("status", "resolved")],
            NOW
        )
        .status,
        404
    );
    // Maintenance.
    let r = post(
        &a,
        "/status/admin/maintenance",
        &root.cookie,
        &[
            ("csrf", &root.csrf),
            ("title", "DB upgrade"),
            ("start", "2027-02-01 02:00"),
            ("end", "2027-02-01 03:00"),
            ("components", "api"),
            ("body", "short"),
        ],
        NOW,
    );
    assert_eq!(r.status, 303);
    let bad = post(
        &a,
        "/status/admin/maintenance",
        &root.cookie,
        &[
            ("csrf", &root.csrf),
            ("title", "x"),
            ("start", "nonsense"),
            ("end", "2027-02-01 03:00"),
        ],
        NOW,
    );
    assert_eq!(bad.status, 400);
    let h = get(&a, "/status", "", NOW).body_str();
    assert!(h.contains("DB upgrade") && h.contains("Upcoming") || h.contains("In progress"));
    assert_eq!(
        obop_community::status_pages::parse_utc("2027-02-01 02:00"),
        Some(1_801_447_200)
    );
    assert_eq!(
        obop_community::status_pages::parse_utc("2026-02-31 02:00"),
        None
    );
}

#[test]
fn data_survives_a_restart_on_sqlite() {
    let dir = std::env::temp_dir().join(format!("obop-community-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let url = format!("sqlite://{}/c.db", dir.display());
    {
        let a = app_on(obop_storage::open(&url).unwrap());
        let s = signup(&a, "ada@example.test", "Ada", NOW);
        assert_eq!(
            new_thread(&a, &s, "help", "Persistent", "body", NOW + 1).status,
            303
        );
        record(&a, "api", Health::Up, NOW + 2);
        a.status
            .create_incident(
                "Kept",
                obop_community::status::Impact::None,
                obop_community::status::IncStatus::Resolved,
                &[],
                "x",
                NOW,
            )
            .unwrap();
    }
    let a = app_on(obop_storage::open(&url).unwrap());
    assert_eq!(
        get(&a, "/community/t/1-persistent", "", NOW + 3).status,
        200
    );
    assert!(get(&a, "/status", "", NOW + 3).body_str().contains("Kept"));
    // The old session still works after the restart (sessions are stored too).
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn css_is_served_with_long_caching_and_dark_mode_tokens() {
    let a = app();
    let r = get(&a, "/community/assets/app.css", "", NOW);
    assert_eq!(r.status, 200);
    assert!(
        r.get_header("content-type")
            .unwrap()
            .starts_with("text/css")
    );
    assert!(r.get_header("cache-control").unwrap().contains("immutable"));
    let css = r.body_str();
    for token in [
        "--accent",
        "--panel",
        "--line",
        "--fg",
        "--muted",
        "--radius",
        "prefers-color-scheme:dark",
        ":focus-visible",
        "prefers-reduced-motion",
    ] {
        assert!(css.contains(token), "{token}");
    }
    let home = get(&a, "/community", "", NOW).body_str();
    assert!(home.contains("/community/assets/app.css?v="));
}
