//! The signer's door with an instance brand (docs/spec/branding.md, slice S1d): the real router, the
//! real handlers and the real authenticator, a brand written the way `instance_brand.configure`
//! writes it, and for the part of D74 a `Flaky` store under the router. The pages themselves, in every
//! state and language, are held by `obop-http` (`sign_page.rs`); what is tested here is that the
//! handlers hand them the brand, once, first, from every arm that makes a page, and that nothing the
//! signer relies on (the consent id) moves with it.
//!
//! What an instance with **no** row sends is held by the golden pins of `obop-http` and is not
//! touched here; the tests that have no brand compare a page with the page of a brand only to say what
//! changed. Everything signed here is a made-up PDF and a made-up person.

use super::*;
use crate::brand_row_tests::set_brand;
use obop_commands::brand::{self, Action, Shown};
use obop_oauth::page::{LANGS, brand_line};
use obop_storage::testing::{Flaky, Mode};

/// The reader's brand, as a page of this instance shows it.
fn shown(f: &Fx) -> Arc<Shown> {
    f.app.dispatcher.brand().current()
}

/// The tab icon link of the brand in force.
fn icon(f: &Fx) -> String {
    format!(
        "<link rel=\"icon\" href=\"/api/brand/logo.svg?v={}\" type=\"image/svg+xml\">",
        shown(f).v()
    )
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

/// Every header of an answer but those named, as (name, value) pairs in name order.
fn headers_but(r: &Resp, skip: &[&str]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = r
        .headers
        .iter()
        .filter(|(k, _)| !skip.contains(&k.as_str()))
        .map(|(k, v)| {
            (
                k.to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    out.sort();
    out
}

/// Every header of a page but the two that a brand moves: the policy (it gains the image source) and
/// the length of the body (the tests compare the body whole).
fn other_headers(r: &Resp) -> Vec<(String, String)> {
    headers_but(r, &["content-security-policy", "content-length"])
}

/// A page of the door with a brand set: the line of the language and the tab icon, a policy that
/// ends with the image source and is the policy it was otherwise, one brand line and one icon, and
/// nothing that names the product, in the text or in the markup.
fn assert_branded(f: &Fx, r: &Resp, lang: &str, what: &str) {
    let html = r.text();
    let line = brand_line(&shown(f), lang);
    assert!(
        line.contains("<svg") && line.contains("MOHV"),
        "{what}: {line}"
    );
    assert!(html.contains(&line), "{what}: no brand line in {html}");
    assert!(html.contains(&icon(f)), "{what}: no tab icon");
    assert_eq!(html.matches("rel=\"icon\"").count(), 1, "{what}");
    assert_eq!(html.matches("class=\"brand\"").count(), 1, "{what}");
    let csp = r.h("content-security-policy");
    assert!(csp.ends_with("; img-src 'self'"), "{what}: {csp}");
    assert_eq!(csp.matches("img-src").count(), 1, "{what}: {csp}");
    assert!(
        !html.to_lowercase().contains("obop"),
        "{what} names the product: {html}"
    );
}

/// A page of the door with no brand set: the line it always had, no icon, no image source.
fn assert_plain(r: &Resp, what: &str) {
    let html = r.text();
    assert!(
        html.contains("<p class=\"brand\">obop</p>"),
        "{what}: {html}"
    );
    assert!(!html.contains("<link"), "{what}");
    assert!(
        !r.h("content-security-policy").contains("img-src"),
        "{what}"
    );
}

/// A page of the door says the words of the catalogue key in `lang`, in its paragraph: the handler
/// handed `message` that key (the pages of two refusals differ in nothing else).
fn assert_says(r: &Resp, lang: &str, key: &str, what: &str) {
    let want = format!("<p>{}</p>", obop_http::sign_text::html(lang, key, &[]));
    assert!(
        r.text().contains(&want),
        "{what}: the page does not say the words of {key}: {}",
        r.text()
    );
}

/// A form that signs, in `lang`.
fn form<'a>(f: &'a Fx, name: &'a str, lang: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![
        ("name", name),
        ("agree", "1"),
        ("doc", &f.file),
        ("lang", lang),
    ]
}

/// The path of the link with its secret changed: the right length, the wrong link.
fn wrong_link(path: &str) -> String {
    let (head, tok) = path.rsplit_once('/').unwrap();
    format!("{head}/{}", "1".repeat(tok.len()))
}

/// A request body that is not a form.
async fn post_text(f: &Fx, path: &str, from: &str) -> Resp {
    send(
        &f.router,
        Request::post(path)
            .header("host", HOST)
            .header("origin", ORIGIN)
            .header("content-type", "text/plain")
            .header("x-forwarded-for", from)
            .body(Body::from("name=Jane+Test"))
            .unwrap(),
    )
    .await
}

/// The form and the refusal of a wrong link, in every language, as `(language/what, answer)`.
async fn form_and_refusal_in_every_language(
    f: &Fx,
    path: &str,
    wrong: &str,
    from: &str,
) -> Vec<(String, Resp)> {
    let mut out = Vec::new();
    for lang in LANGS {
        for (what, p) in [("form", path), ("refused", wrong)] {
            let r = f.get(&format!("{p}?lang={lang}"), from).await;
            out.push((format!("{lang}/{what}"), r));
        }
    }
    out
}

/// The page of an instance with no brand, then with one, then with none again, for the link and the
/// wrong link, in every language: with a brand the line and the icon appear, the policy gains
/// the image source, and the text of the page is the text it was; once the brand is cleared the page
/// is the page it was before, byte for byte.
#[tokio::test]
async fn with_a_brand_row_the_signer_page_shows_it_and_clear_restores_the_page() {
    let (f, _, path, tok) = sent().await;
    let wrong = wrong_link(&path);
    assert_ne!(wrong, path);
    assert!(path.ends_with(&tok));
    // A failed lookup is counted for its address (20 in ten minutes), so each round has its own.
    let was = form_and_refusal_in_every_language(&f, &path, &wrong, &ip(31)).await;
    assert!(was.iter().any(|(_, r)| r.status == 200));
    for (name, r) in &was {
        assert_plain(r, name);
    }

    set_brand(&f.app);
    let now = form_and_refusal_in_every_language(&f, &path, &wrong, &ip(32)).await;
    assert_eq!(now.len(), was.len());
    for ((name, before), (_, after)) in was.iter().zip(&now) {
        let lang = name.split('/').next().unwrap();
        assert_eq!(after.status, before.status, "{name}");
        assert_branded(&f, after, lang, name);
        // The page is the page it was with the line and the icon put in, and nothing else.
        let expected = before
            .text()
            .replacen(
                "<p class=\"brand\">obop</p>",
                &brand_line(&shown(&f), lang),
                1,
            )
            .replacen(
                "</title><style>",
                &format!("</title>{}<style>", icon(&f)),
                1,
            );
        assert_eq!(
            after.text(),
            expected,
            "{name}: more than the line and the icon"
        );
        assert_eq!(
            after.h("content-security-policy"),
            format!("{}; img-src 'self'", before.h("content-security-policy")),
            "{name}"
        );
        // Every other header is the one it had, by name and value, and none was added: a brand
        // moves the policy and nothing else, and the page is still no cookie.
        assert_eq!(
            other_headers(after),
            other_headers(before),
            "{name}: more than the policy moved"
        );
        assert!(
            after.headers.get("set-cookie").is_none() && before.headers.get("set-cookie").is_none(),
            "{name}: the page sets a cookie"
        );
    }
    // The title of the language, the English title, and the name (FIELDS has three titles).
    let page = |name: &str| now.iter().find(|(n, _)| n == name).unwrap().1.text();
    assert!(page("sv/form").contains("</span> MOHV Körjournal</p>"));
    assert!(page("de/form").contains("</span> MOHV Fahrtenbuch</p>"));
    assert!(page("fr/form").contains("</span> MOHV Mileage</p>"));
    assert!(page("sv/refused").contains("</span> MOHV Körjournal</p>"));

    clear(&f.app);
    let after = form_and_refusal_in_every_language(&f, &path, &wrong, &ip(33)).await;
    for ((name, before), (_, again)) in was.iter().zip(&after) {
        assert!(
            again.text() == before.text()
                && again.status == before.status
                && again.h("content-security-policy") == before.h("content-security-policy"),
            "{name} is not what it was after the brand was cleared"
        );
    }
}

/// Every arm of the handlers that makes a page, in seven languages, with a brand set: the form, the
/// refusal of a wrong link, of a revoked one, of a PDF, of a post from another site, of a body that is
/// not a form, of a form that is not acceptable (the form again, with the error), of a post to a
/// revoked link, and the page after signing. A handler that handed one of them the default brand is
/// found by name (the review changed each call in turn).
#[tokio::test]
async fn every_arm_of_the_door_that_makes_a_page_shows_the_brand_in_seven_languages() {
    let (f, _, path, _) = sent().await;
    set_brand(&f.app);
    // A second link, revoked at once: the 410 of a link that is over.
    let (second_id, second) = f.mint("mia", &f.req.clone()).await;
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_link/update",
            json!({"id": second_id, "revoked_at": "2026-01-01T00:00:00Z"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let wrong = wrong_link(&path);
    let mut arms = 0;
    for (i, lang) in LANGS.into_iter().enumerate() {
        // One address for each language, so that no per-address limit is met on the way.
        let from = ip(60 + i as u8);
        let at = |p: &str| format!("{p}?lang={lang}");
        // GET: the form, a wrong link, a revoked link.
        let r = f.get(&at(&path), &from).await;
        assert_eq!(r.status, 200, "{}", r.text());
        assert_branded(&f, &r, lang, &format!("{lang}/form"));
        let r = f.get(&at(&wrong), &from).await;
        assert_eq!(r.status, 404);
        assert_branded(&f, &r, lang, &format!("{lang}/unknown"));
        assert_says(&r, lang, "unknown", &format!("{lang}/unknown"));
        let r = f.get(&at(&second), &from).await;
        assert_eq!(r.status, 410);
        assert_branded(&f, &r, lang, &format!("{lang}/revoked"));
        assert_says(&r, lang, "gone_revoked", &format!("{lang}/revoked"));
        // GET of the PDF: a wrong link and a revoked one are pages of the door too.
        let r = f.get(&at(&format!("{wrong}/document.pdf")), &from).await;
        assert_eq!(r.status, 404);
        assert_branded(&f, &r, lang, &format!("{lang}/pdf unknown"));
        assert_says(&r, lang, "unknown", &format!("{lang}/pdf unknown"));
        let r = f.get(&at(&format!("{second}/document.pdf")), &from).await;
        assert_eq!(r.status, 410);
        assert_branded(&f, &r, lang, &format!("{lang}/pdf revoked"));
        assert_says(&r, lang, "gone_revoked", &format!("{lang}/pdf revoked"));
        // POST: from another site, a body that is not a form, a form that is not acceptable.
        let r = f
            .post_with(
                &at(&path),
                &from,
                &form(&f, "Jane Test", lang),
                &[("origin", "http://evil.test")],
            )
            .await;
        assert_eq!(r.status, 403);
        assert_branded(&f, &r, lang, &format!("{lang}/forbidden"));
        assert_says(&r, lang, "forbidden", &format!("{lang}/forbidden"));
        let r = post_text(&f, &at(&path), &from).await;
        assert_eq!(r.status, 400);
        assert_branded(&f, &r, lang, &format!("{lang}/not a form"));
        assert_says(&r, lang, "err_name", &format!("{lang}/not a form"));
        let r = f.post(&path, &from, &form(&f, "", lang)).await;
        assert_eq!(r.status, 400);
        assert!(r.text().contains("<form"), "the form is shown again");
        assert!(r.text().contains("role=\"alert\""), "with the error");
        assert_branded(&f, &r, lang, &format!("{lang}/form again"));
        // POST to a link that is over: the refusal of the post.
        let r = f.post(&second, &from, &form(&f, "Jane Test", lang)).await;
        assert_eq!(r.status, 410);
        assert_branded(&f, &r, lang, &format!("{lang}/post revoked"));
        assert_says(&r, lang, "gone_revoked", &format!("{lang}/post revoked"));
        arms += 9;
    }
    assert_eq!(arms, 63);
    // Nothing was signed by any of that.
    assert_eq!(f.record("sign_request", &f.req)["state"], "sent");

    // Signed, the page after signing, in every language, and a second post changes nothing.
    let r = f.post(&path, &ip(90), &form(&f, "Jane Test", "en")).await;
    assert_eq!(r.status, 303, "{}", r.text());
    for (i, lang) in LANGS.into_iter().enumerate() {
        let r = f
            .get(&format!("{path}?lang={lang}"), &ip(91 + i as u8))
            .await;
        assert_eq!(r.status, 200);
        assert!(
            r.text()
                .contains(&obop_oauth::page::esc(obop_http::sign_text::text(
                    lang,
                    "done_title"
                ))),
            "{lang}: {}",
            r.text()
        );
        assert_branded(&f, &r, lang, &format!("{lang}/done"));
    }
}

/// The request that shows the signatures so far: its form lists those who signed the file, and the
/// page after signing does too; both are pages of the door with the brand.
#[tokio::test]
async fn the_pages_that_list_the_signatures_so_far_show_the_brand() {
    let (f, _, _, _) = sent().await;
    set_brand(&f.app);
    // Two requests about the same file, both asking to show the history.
    let mut links = Vec::new();
    for (title, who) in [
        ("History one", "Alice Example"),
        ("History two", "Bob Example"),
    ] {
        let (s, v) = f
            .api(
                "mia",
                "/api/sign_request/create",
                json!({"title": title, "signer_name": who, "document_id": f.doc,
                       "show_history": true}),
            )
            .await;
        assert_eq!(s, 200, "{v}");
        let id = v["value"]["id"].as_str().unwrap().to_string();
        let (s, v) = f
            .api("mia", "/api/sign_request/send_document", json!({"id": id}))
            .await;
        assert_eq!(s, 200, "{v}");
        links.push(f.mint("mia", &id).await.1);
    }
    // Alice signs the first (her page carries the sentence, so her form carries `hist`).
    let page = f.get(&links[0], &ip(70)).await;
    assert_eq!(page.status, 200, "{}", page.text());
    assert!(page.text().contains("name=\"hist\""), "{}", page.text());
    let mut alice = form(&f, "Alice Example", "en");
    alice.push(("hist", "1"));
    assert_eq!(f.post(&links[0], &ip(71), &alice).await.status, 303);

    for (i, lang) in LANGS.into_iter().enumerate() {
        // Bob's form lists Alice.
        let r = f
            .get(&format!("{}?lang={lang}", links[1]), &ip(72 + i as u8))
            .await;
        assert_eq!(r.status, 200, "{}", r.text());
        assert!(r.text().contains("Alice Example"), "{lang}: the list");
        assert_branded(&f, &r, lang, &format!("{lang}/form with history"));
        // Alice's own page after signing lists her.
        let r = f
            .get(&format!("{}?lang={lang}", links[0]), &ip(80 + i as u8))
            .await;
        assert_eq!(r.status, 200, "{}", r.text());
        assert!(r.text().contains("Alice Example"), "{lang}: the list");
        assert_branded(&f, &r, lang, &format!("{lang}/done with history"));
    }
    // Bob's form again after a refused try (no name), still with the list.
    let mut bob = form(&f, "", "sv");
    bob.push(("hist", "1"));
    let r = f.post(&links[1], &ip(79), &bob).await;
    assert_eq!(r.status, 400, "{}", r.text());
    assert!(r.text().contains("Alice Example"));
    assert_branded(&f, &r, "sv", "form again with history");
}

/// The 404 of a link on the router given, from the three handlers (the page, the PDF and the post),
/// in every language: the line of the brand, the tab icon, the image source in the policy, and no
/// word that names the product.
async fn assert_every_404_shows_the_brand(router: &axum::Router, shown: &Shown) {
    let tok = "a".repeat(64);
    for lang in LANGS {
        let line = brand_line(shown, lang);
        for (what, r) in [
            (
                "page",
                send(
                    router,
                    Request::get(format!("/sign/abc/{tok}?lang={lang}"))
                        .header("host", HOST)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await,
            ),
            (
                "pdf",
                send(
                    router,
                    Request::get(format!("/sign/abc/{tok}/document.pdf?lang={lang}"))
                        .header("host", HOST)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await,
            ),
            (
                "post",
                send(
                    router,
                    Request::post(format!("/sign/abc/{tok}?lang={lang}"))
                        .header("host", HOST)
                        .header("origin", ORIGIN)
                        .header("content-type", "application/x-www-form-urlencoded")
                        .body(Body::from("name=Jane+Test&agree=1"))
                        .unwrap(),
                )
                .await,
            ),
        ] {
            assert_eq!(r.status, 404, "{lang}/{what}");
            let html = r.text();
            assert!(html.contains(&line), "{lang}/{what}: {html}");
            assert!(html.contains("rel=\"icon\""), "{lang}/{what}");
            assert!(
                r.h("content-security-policy").ends_with("; img-src 'self'"),
                "{lang}/{what}"
            );
            assert!(!html.to_lowercase().contains("obop"), "{lang}/{what}");
        }
    }
}

/// An instance whose `sign` package is not installed has a door that answers 404 to every link; the
/// three handlers answer it with a page, and the page shows the brand.
#[tokio::test]
async fn the_door_of_an_instance_without_the_package_shows_the_brand_on_its_404() {
    let app = App::new();
    app.installer
        .install(&Bundled.fetch("documents").unwrap(), "marketplace", "test")
        .unwrap();
    let auth = Arc::new(Auth::with_test_params(app.store.backend()));
    let state = http_state(&app, auth);
    let router = obop_http::router(state, None);
    let tok = "a".repeat(64);
    let plain = send(
        &router,
        Request::get(format!("/sign/abc/{tok}"))
            .header("host", HOST)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(plain.status, 404);
    assert_plain(&plain, "no brand");
    set_brand(&app);
    assert_every_404_shows_the_brand(&router, &app.dispatcher.brand().current()).await;
}

/// A dispatcher that registered no module at all has no signer service either (the service comes
/// with the first module that is registered), and the three handlers take their first arm, before
/// the door is asked anything. That 404 shows the brand too, and shows the plain page with no row.
#[tokio::test]
async fn the_door_of_a_dispatcher_without_a_signer_service_shows_the_brand_on_its_404() {
    let app = App::new();
    let backend = app.store.backend();
    let dispatcher = Arc::new(obop_commands::Dispatcher::new());
    let reader = dispatcher.install_brand(backend.clone());
    assert!(
        dispatcher.sign_service().is_none(),
        "a dispatcher with no module has no signer service"
    );
    let auth = Arc::new(Auth::with_test_params(backend.clone()));
    let state =
        obop_http::HttpState::with_mcp(dispatcher, auth, obop_http::mcp::McpConfig::default());
    let router = obop_http::router(state, None);
    let tok = "a".repeat(64);
    let plain = send(
        &router,
        Request::get(format!("/sign/abc/{tok}"))
            .header("host", HOST)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(plain.status, 404);
    assert_plain(&plain, "no brand, no service");
    let action = Action::Configure(
        crate::brand_row_tests::FIELDS
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    );
    brand::apply(&*backend, Some(&reader), "operator", &action, false)
        .unwrap_or_else(|e| panic!("the brand row was refused: {e}"));
    assert_every_404_shows_the_brand(&router, &reader.current()).await;
}

/// The consent id is hashed from the texts the signer relied on, and a brand has no word in them:
/// the id a signature records is the same on an instance with a brand, with and without the history
/// of the file, in every one of the seven languages, and it is the id the texts give.
#[tokio::test]
async fn a_signature_made_with_a_brand_records_the_same_consent_id() {
    // Without a brand and with one, the same request in each language. A fresh instance for each
    // signature (a request is signed once), so the address can be the same.
    let mut ids = Vec::new();
    for lang in LANGS {
        let mut pair = Vec::new();
        for branded in [false, true] {
            let (f, _, path, _) = sent().await;
            if branded {
                set_brand(&f.app);
            }
            let r = f.post(&path, &ip(100), &form(&f, "Jane Test", lang)).await;
            assert_eq!(r.status, 303, "{lang} branded {branded}: {}", r.text());
            let id = f.record("sign_request", &f.req)["consent_version"]
                .as_str()
                .unwrap()
                .to_string();
            assert_eq!(
                id,
                obop_http::sign_text::consent_version_for(lang),
                "{lang} branded {branded}"
            );
            pair.push(id);
        }
        assert_eq!(pair[0], pair[1], "{lang}: a brand moved the consent id");
        ids.push((lang, pair.remove(1)));
    }
    // The languages have ids of their own.
    let mut all: Vec<&String> = ids.iter().map(|(_, id)| id).collect();
    all.sort();
    all.dedup();
    assert_eq!(all.len(), LANGS.len(), "seven languages, seven ids");
    // The ids written out (the unit test of `sign_text.rs` pins all seven).
    let id_of = |lang: &str| ids.iter().find(|(l, _)| *l == lang).unwrap().1.as_str();
    assert_eq!(id_of("en"), "v1-draft/en/a05c68c3");
    assert_eq!(id_of("de"), "v1-draft/de/9ebff173");

    // The request that shows the history: the id with the mark, with a brand as without, in every
    // language (the form carries the language and the box that says the page held the sentence).
    let mut with_history = Vec::new();
    for lang in LANGS {
        let mut pair = Vec::new();
        for branded in [false, true] {
            let (f, _, _, _) = sent().await;
            if branded {
                set_brand(&f.app);
            }
            let (s, v) = f
                .api(
                    "mia",
                    "/api/sign_request/create",
                    json!({"title": "History", "signer_name": "Alice Example", "document_id": f.doc,
                           "show_history": true}),
                )
                .await;
            assert_eq!(s, 200, "{v}");
            let id = v["value"]["id"].as_str().unwrap().to_string();
            let (s, v) = f
                .api("mia", "/api/sign_request/send_document", json!({"id": id}))
                .await;
            assert_eq!(s, 200, "{v}");
            let (_, link) = f.mint("mia", &id).await;
            let mut pairs = form(&f, "Alice Example", lang);
            pairs.push(("hist", "1"));
            let r = f.post(&link, &ip(101), &pairs).await;
            assert_eq!(r.status, 303, "{lang} branded {branded}: {}", r.text());
            pair.push(
                f.record("sign_request", &id)["consent_version"]
                    .as_str()
                    .unwrap()
                    .to_string(),
            );
        }
        assert_eq!(
            pair[0], pair[1],
            "{lang}: a brand moved the id with history"
        );
        assert_eq!(
            pair[1],
            obop_http::sign_text::consent_version_with_history_for(lang),
            "{lang}"
        );
        assert!(pair[1].ends_with("/history"), "{lang}: {}", pair[1]);
        with_history.push(pair.remove(1));
    }
    assert_eq!(with_history[0], "v1-draft/en/e2880db0/history");
    let mut all = with_history.clone();
    all.sort();
    all.dedup();
    assert_eq!(all.len(), LANGS.len(), "seven languages, seven ids");
    // The id of a request that shows the history is not the id of one that does not.
    for ((_, plain), with) in ids.iter().zip(&with_history) {
        assert_ne!(plain, with);
    }
}

/// A row written around the command: a name with every character that HTML needs escaped and that
/// the read check lets through (`&`, `"`, `'`), a title with a tag in it (it fails the read check and
/// falls back to the name). Neither reaches a page of the door raw, whatever the state, and
/// the connector page next to it is held the same way in `connectors_tests.rs`.
#[tokio::test]
async fn a_hostile_name_written_around_the_command_is_escaped_on_the_pages_of_the_door() {
    let (f, _, path, _) = sent().await;
    let evil = "Acme & \"Sons\" 'x'";
    f.app
        .store
        .backend()
        .apply_checked(&[obop_storage::Op::Put {
            model: brand::MODEL.into(),
            id: brand::ROW_ID.into(),
            fields: [
                ("name", json!(evil)),
                ("title_sv", json!("<script>alert(1)</script>")),
                ("title_de", json!("Tit & \"el\"")),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
        }])
        .unwrap();
    let wrong = wrong_link(&path);
    for (i, lang) in LANGS.into_iter().enumerate() {
        let from = ip(110 + i as u8);
        for (what, r) in [
            ("form", f.get(&format!("{path}?lang={lang}"), &from).await),
            (
                "unknown",
                f.get(&format!("{wrong}?lang={lang}"), &from).await,
            ),
        ] {
            let html = r.text();
            assert!(!html.contains("<script"), "{lang}/{what}: {html}");
            assert!(!html.contains(evil), "{lang}/{what}: the name raw");
            let want = if lang == "de" {
                obop_oauth::page::esc("Tit & \"el\"")
            } else {
                obop_oauth::page::esc(evil)
            };
            assert!(
                html.contains(&format!("</span> {want}</p>")),
                "{lang}/{what}: {}",
                brand_line(&shown(&f), lang)
            );
        }
    }
}

// ---- The first read of the request (D74) ----------------------------------------------------------

/// A fixture over a store whose reads can be refused, with its request sent and a link made. No page
/// has been asked for yet, so the reader of the instance has not read the brand.
async fn flaky_sent() -> (Arc<Flaky>, Fx, String) {
    let (flaky, backend) = Flaky::memory();
    let bytes = Arc::new(MemoryBytes::default());
    let store =
        Store::over(backend.clone()).with_blobs(Arc::new(Catalog::new(backend, bytes.clone())));
    let f = fixture_on(store, Some(bytes), None).await;
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/send_document",
            json!({"id": f.req}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let (_, path) = f.mint("mia", &f.req.clone()).await;
    (flaky, f, path)
}

/// The answer of D74 to a request that met a store fault: 503 with `Retry-After`, the word that
/// nothing was applied, and not a page of the door that says anything about the instance.
fn assert_d74(r: &Resp, what: &str) {
    assert_eq!(r.status, 503, "{what}: {}", r.text());
    assert_eq!(r.h("retry-after"), "1", "{what}");
    assert_eq!(r.h("x-obop-applied"), "none", "{what}");
    let page = r.text();
    assert!(!page.contains("class=\"brand\""), "{what}: {page}");
    assert!(!page.to_lowercase().contains("obop"), "{what}: {page}");
}

/// The post that signs reads the brand before it writes anything. With the first read refused it is
/// the 503 of D74, nothing is signed and nothing is logged; the same post once the store answers
/// signs, and the page after it shows the brand. The expiring counters are the exception, as for
/// the OAuth sign-in (the guard of the store does not cover them): the refused post spent one hit
/// of the link's hourly window of posts, which is pinned below by how many more the link takes.
/// This post is one the door would accept, so it could never be refused with a line in the log
/// either way; the requests that the door does log a refusal for are held by
/// `a_request_the_door_would_log_a_refusal_for_logs_none_when_its_first_brand_read_fails`.
#[tokio::test]
async fn a_post_whose_first_brand_read_fails_signs_nothing_logs_nothing_and_is_a_503() {
    let (flaky, f, path) = flaky_sent().await;
    let before = f.everything_stored();
    let writes = flaky.seen_writes().len();
    let hot = |f: &Fx| f.record("sign_request", &f.req);
    let request_before = hot(&f);

    flaky.refuse_model(brand::MODEL, Mode::Reads);
    let refused_before = flaky.refused();
    let r = f.post(&path, &ip(120), &form(&f, "Jane Test", "en")).await;
    assert_d74(&r, "the post");
    assert!(
        flaky.refused() > refused_before,
        "the read that was refused was the brand's"
    );
    assert_eq!(
        f.everything_stored(),
        before,
        "no record, entry or short-lived value was written"
    );
    assert_eq!(hot(&f), request_before, "the request is as it was");
    assert_eq!(
        f.events_of("sign_signed").len(),
        0,
        "nothing is logged as signed"
    );
    assert_eq!(
        f.events_of("sign_refused").len(),
        0,
        "nothing is logged as refused"
    );
    assert_eq!(
        flaky.seen_writes().len(),
        writes,
        "no batch reached the store"
    );

    // The store answers: the same post signs, and the page after it shows the brand.
    flaky.allow_models();
    set_brand(&f.app);
    let r = f.post(&path, &ip(121), &form(&f, "Jane Test", "en")).await;
    assert_eq!(r.status, 303, "{}", r.text());
    assert_eq!(f.record("sign_request", &f.req)["state"], "signed");
    assert_eq!(f.events_of("sign_signed").len(), 1);
    let r = f.get(&format!("{path}?lang=en"), &ip(122)).await;
    assert_eq!(r.status, 200);
    assert_branded(&f, &r, "en", "the page after signing");
}

/// The counter the refused post spent: a link takes ten posts an hour, so a link that had one of
/// them refused by the first read takes nine more.
#[tokio::test]
async fn the_post_that_met_the_fault_spent_one_hit_of_the_hourly_window_of_the_link() {
    let posts_a_link_takes = |failed_first: bool| async move {
        let (flaky, f, path) = flaky_sent().await;
        if failed_first {
            flaky.refuse_model(brand::MODEL, Mode::Reads);
            let r = f.post(&path, &ip(130), &form(&f, "Jane Test", "en")).await;
            assert_d74(&r, "the post");
            flaky.allow_models();
        }
        // Posts that are refused for a bad name (400) until the window says no (429), from a new
        // address each time so that the per-address window is not the one that speaks.
        let mut taken = 0;
        for n in 0..12u8 {
            let r = f.post(&path, &ip(140 + n), &form(&f, "", "en")).await;
            match r.status {
                400 => taken += 1,
                429 => break,
                other => panic!("post {n}: {other}: {}", r.text()),
            }
        }
        taken
    };
    let plain = posts_a_link_takes(false).await;
    let after_a_failed_one = posts_a_link_takes(true).await;
    assert_eq!(plain, 10, "a link takes ten posts an hour");
    assert_eq!(
        after_a_failed_one, 9,
        "the post that met the fault spent one of them"
    );
}

/// The `GET` of the page, and of the PDF, with the first read refused: the request that ran the read
/// is answered by D74 and not by a page that says the product's name; once the store answers they
/// are what they were. (The reader serves its last good value to a request that finds a refill
/// running, which `obop-commands` and the OAuth tests hold.)
#[tokio::test]
async fn a_get_whose_first_brand_read_fails_is_a_503_and_never_a_page_that_says_obop() {
    let (flaky, f, path) = flaky_sent().await;
    flaky.refuse_model(brand::MODEL, Mode::Reads);
    let r = f.get(&format!("{path}?lang=en"), &ip(150)).await;
    assert_d74(&r, "the page");
    let r = f.get(&format!("{path}/document.pdf"), &ip(151)).await;
    assert_d74(&r, "the PDF");
    assert!(
        r.body.is_empty() || !r.body.starts_with(b"%PDF"),
        "the PDF was not sent"
    );
    let r = f
        .get(&format!("{}?lang=en", wrong_link(&path)), &ip(152))
        .await;
    assert_d74(&r, "a wrong link");

    flaky.allow_models();
    let r = f.get(&format!("{path}?lang=en"), &ip(153)).await;
    assert_eq!(r.status, 200);
    assert_plain(&r, "the store answers: no brand");
    let r = f.get(&format!("{path}/document.pdf"), &ip(154)).await;
    assert_eq!(r.status, 200);
    assert_eq!(r.body, pdf(1));
}

/// A brand read once is served to the pages that follow within the bound, so a signer who loaded the
/// page and posts a moment later is not made to wait on a store that blinks: the form and the post
/// both show the same brand.
#[tokio::test]
async fn the_form_and_the_post_after_it_show_the_same_brand() {
    let (f, _, path, _) = sent().await;
    set_brand(&f.app);
    let first = f.get(&format!("{path}?lang=sv"), &ip(160)).await;
    assert_branded(&f, &first, "sv", "the form");
    let again = f.post(&path, &ip(161), &form(&f, "", "sv")).await;
    assert_eq!(again.status, 400);
    assert_branded(&f, &again, "sv", "the form again");
    // The same line in both, byte for byte.
    let line = |r: &Resp| {
        let h = r.text();
        let a = h.find("<p class=\"brand\">").unwrap();
        h[a..a + h[a..].find("</p>").unwrap()].to_string()
    };
    assert_eq!(line(&first), line(&again));
}

/// What a brand moves in the answers that are not a page of refusal or a form: the post that signs,
/// the PDF and the page after signing. The 303 goes to the address it always went to (the path of the
/// link and the language, no name in it), the PDF has the bytes and every header it had (its own
/// policy included), the other two have every header but the policy, and none of the three sets a
/// cookie.
#[tokio::test]
async fn the_post_the_pdf_and_the_page_after_signing_are_what_they_were_but_for_the_policy() {
    let mut seen = Vec::new();
    for branded in [false, true] {
        let (f, _, path, _) = sent().await;
        if branded {
            set_brand(&f.app);
        }
        let pdf_answer = f.get(&format!("{path}/document.pdf"), &ip(170)).await;
        assert_eq!(pdf_answer.status, 200, "branded {branded}");
        assert_eq!(pdf_answer.body, pdf(1));
        let post = f.post(&path, &ip(171), &form(&f, "Jane Test", "en")).await;
        assert_eq!(post.status, 303, "branded {branded}: {}", post.text());
        assert_eq!(
            post.h("location"),
            format!("{path}?lang=en"),
            "branded {branded}: where the post sends the browser"
        );
        let done = f.get(&format!("{path}?lang=en"), &ip(172)).await;
        assert_eq!(done.status, 200, "branded {branded}");
        assert_eq!(
            done.h("content-security-policy")
                .ends_with("; img-src 'self'"),
            branded,
            "branded {branded}: the policy of the page after signing"
        );
        for (what, r) in [("pdf", &pdf_answer), ("post", &post), ("done", &done)] {
            assert!(
                r.headers.get("set-cookie").is_none(),
                "branded {branded}/{what}: a cookie"
            );
        }
        // The link holds a secret that differs from one fixture to the next: name it, compare the rest.
        let norm = |r: &Resp, skip: &[&str]| {
            headers_but(r, skip)
                .into_iter()
                .map(|(k, v)| (k, v.replace(&path, "<link>")))
                .collect::<Vec<_>>()
        };
        seen.push((
            norm(&pdf_answer, &[]),
            pdf_answer.body.clone(),
            norm(&post, &[]),
            post.body.clone(),
            norm(&done, &["content-security-policy", "content-length"]),
        ));
    }
    assert!(
        seen[0] == seen[1],
        "a brand moved a header, an address or the bytes of the PDF:\n{:?}\n{:?}",
        seen[0],
        seen[1]
    );
}

/// A row written around the command with an artwork that the strict reader refuses (a script, a link,
/// an event handler, a file over 16 KiB, a byte order mark) is in no page of the door: the page shows
/// the generated tile in its place, with the tab icon link and the policy of a page with a brand, and
/// not a byte of the stored text.
#[tokio::test]
async fn an_artwork_the_reader_refuses_is_the_tile_on_the_pages_of_the_door() {
    let head = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\"";
    let rect = "<rect width=\"64\" height=\"64\"/>";
    let big = format!("{head}>{rect}<!-- {} --></svg>", "PLANTED ".repeat(2200));
    assert!(big.len() > 16 * 1024);
    let good = crate::brand_row_tests::FIELDS
        .iter()
        .find(|(k, _)| *k == "logo_svg")
        .unwrap()
        .1;
    let cases = [
        ("a script", format!("{head}><script>PLANTED</script></svg>")),
        (
            "a link",
            format!("{head}><a href=\"javascript:PLANTED\">{rect}</a></svg>"),
        ),
        (
            "an event handler",
            format!("{head} onload=\"PLANTED\">{rect}</svg>"),
        ),
        ("a file over 16 KiB", big),
        ("a byte order mark", format!("\u{FEFF}{good}")),
    ];
    for (what, artwork) in cases {
        let (f, _, path, _) = sent().await;
        f.app
            .store
            .backend()
            .apply_checked(&[obop_storage::Op::Put {
                model: brand::MODEL.into(),
                id: brand::ROW_ID.into(),
                fields: [("name", json!("MOHV")), ("logo_svg", json!(artwork))]
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
            }])
            .unwrap();
        let wrong = wrong_link(&path);
        for (state, r) in [
            ("form", f.get(&format!("{path}?lang=en"), &ip(190)).await),
            (
                "unknown",
                f.get(&format!("{wrong}?lang=en"), &ip(191)).await,
            ),
        ] {
            let html = r.text();
            let now = shown(&f);
            assert!(
                now.logo().is_some_and(|l| l.is_tile()),
                "{what}/{state}: the reader kept the artwork"
            );
            assert!(
                !html.contains("PLANTED") && !html.to_lowercase().contains("<script"),
                "{what}/{state}: stored text on the page: {html}"
            );
            assert!(
                html.contains(&brand_line(&now, "en")),
                "{what}/{state}: no brand line"
            );
            assert!(html.contains(&icon(&f)), "{what}/{state}: no tab icon");
            assert_eq!(html.matches("<svg").count(), 1, "{what}/{state}");
            assert!(
                r.h("content-security-policy").ends_with("; img-src 'self'"),
                "{what}/{state}"
            );
        }
    }
}

/// A request the door answers with a line in the security log, and a short-lived key that keeps it to
/// one in ten minutes, when the first read of the brand is good: the page of a request whose
/// Document changed, the PDF whose held copy was altered, and the post to a link that was revoked.
/// When that read is refused each one is the 503 of D74 and writes neither (the guard of the store
/// refuses the key, and without the key the door writes no entry); the same request once the store
/// answers is refused by the door and writes both. So the brand is read before the door decides, in
/// all three handlers, and the assertion that nothing is logged can fail.
#[tokio::test]
async fn a_request_the_door_would_log_a_refusal_for_logs_none_when_its_first_brand_read_fails() {
    use super::hardening::fields;
    use obop_storage::blob::BlobBytes;
    for (what, status, reason) in [
        ("page", 409, "changed"),
        ("pdf", 409, "tampered"),
        ("post", 410, "gone_post"),
    ] {
        let (flaky, f, path) = flaky_sent().await;
        let link = match what {
            "page" => {
                f.app
                    .store
                    .backend()
                    .update(
                        "documents_document",
                        &f.doc,
                        &fields(json!({"attachment": "0".repeat(64)})),
                    )
                    .unwrap();
                path
            }
            "pdf" => {
                f.bytes
                    .as_ref()
                    .unwrap()
                    .put(&f.file, b"%PDF-1.4\n% somebody wrote here\n")
                    .unwrap();
                path
            }
            _ => {
                let (id, second) = f.mint("mia", &f.req.clone()).await;
                let (s, v) = f
                    .api(
                        "mia",
                        "/api/sign_link/update",
                        json!({"id": id, "revoked_at": "2026-01-01T00:00:00Z"}),
                    )
                    .await;
                assert_eq!(s, 200, "{v}");
                second
            }
        };
        let ask = |from: String| {
            let (f, link) = (&f, link.clone());
            async move {
                match what {
                    "page" => f.get(&format!("{link}?lang=en"), &from).await,
                    "pdf" => f.get(&format!("{link}/document.pdf"), &from).await,
                    _ => f.post(&link, &from, &form(f, "Jane Test", "en")).await,
                }
            }
        };
        let logged = || {
            (
                f.events_of("sign_refused").len(),
                f.app
                    .store
                    .backend()
                    .kv_scan("sign:refused:")
                    .unwrap()
                    .len(),
            )
        };
        assert_eq!(logged(), (0, 0), "{what}: before");

        flaky.refuse_model(brand::MODEL, Mode::Reads);
        let r = ask(ip(180)).await;
        assert_d74(&r, what);
        assert_eq!(
            logged(),
            (0, 0),
            "{what}: a refusal was logged for a request that met the fault"
        );

        flaky.allow_models();
        let r = ask(ip(181)).await;
        assert_eq!(r.status, status, "{what}: {}", r.text());
        assert_eq!(logged(), (1, 1), "{what}: the door's own refusal is logged");
        let entry = f.events_of("sign_refused");
        assert!(
            entry[0]["detail"]
                .as_str()
                .unwrap()
                .contains(&format!("reason {reason};")),
            "{what}: {entry:?}"
        );
    }
}

// ---- A title of its own in every language ---------------------------------------------------------

/// A brand with a title in each of the seven languages (the shared `FIELDS` has three, so on the
/// other four the page cannot tell its own language from the English fallback).
fn set_titled_brand(app: &App) {
    let mut supplied = vec![("name".to_string(), "MOHV".to_string())];
    for lang in LANGS {
        supplied.push((
            format!("title_{lang}"),
            format!("MOHV Title {}", lang.to_uppercase()),
        ));
    }
    brand::apply(
        &*app.store.backend(),
        Some(&app.dispatcher.brand()),
        "operator",
        &Action::Configure(supplied),
        false,
    )
    .unwrap_or_else(|e| panic!("the brand row was refused: {e}"));
}

/// The text of the line beside the logo on a page of the door.
fn line_text(r: &Resp) -> String {
    let html = r.text();
    let a = html.find("<p class=\"brand\">").unwrap();
    let end = html[a..].find("</p>").unwrap() + a;
    let line = &html[a..end];
    line[line.rfind("</span> ").unwrap() + "</span> ".len()..].to_string()
}

/// The line of a page is the title of the page's own language, in all seven (Finnish, Polish, French
/// and Spanish too, which `FIELDS` sets no title for): on the form, the refusal of a wrong link and
/// the page after signing.
#[tokio::test]
async fn the_door_shows_the_title_of_the_pages_own_language_in_all_seven() {
    let (f, _, path, _) = sent().await;
    set_titled_brand(&f.app);
    let wrong = wrong_link(&path);
    for (i, lang) in LANGS.into_iter().enumerate() {
        let want = format!("MOHV Title {}", lang.to_uppercase());
        let from = ip(200 + i as u8);
        let r = f.get(&format!("{path}?lang={lang}"), &from).await;
        assert_eq!(r.status, 200, "{lang}: {}", r.text());
        assert_eq!(line_text(&r), want, "{lang}/form");
        let r = f.get(&format!("{wrong}?lang={lang}"), &from).await;
        assert_eq!(r.status, 404, "{lang}");
        assert_eq!(line_text(&r), want, "{lang}/unknown");
    }
    let r = f.post(&path, &ip(210), &form(&f, "Jane Test", "en")).await;
    assert_eq!(r.status, 303, "{}", r.text());
    for (i, lang) in LANGS.into_iter().enumerate() {
        let r = f
            .get(&format!("{path}?lang={lang}"), &ip(211 + i as u8))
            .await;
        assert_eq!(r.status, 200, "{lang}: {}", r.text());
        assert_eq!(
            line_text(&r),
            format!("MOHV Title {}", lang.to_uppercase()),
            "{lang}/done"
        );
    }
}

// ---- After a good read, and a read that fails in no class of D74 (D74) -----------------------------

/// A brand read once is kept for two seconds: a request inside the bound is served from it whatever
/// the store does, so a signer whose page loaded is not made to wait on a store that blinks. Once the
/// bound is over, the request that runs the refill and meets the fault is the 503 of D74, never a page
/// of the stand-in, and so is the one after it (a failed refill is not kept); a post under it signs
/// nothing; and once the store answers, the pages are the brand's again and the post signs.
#[tokio::test]
async fn after_a_good_read_a_failing_store_is_served_from_the_cell_and_then_a_503() {
    let (flaky, f, path) = flaky_sent().await;
    set_brand(&f.app);
    let good = f.get(&format!("{path}?lang=en"), &ip(220)).await;
    assert_eq!(good.status, 200, "{}", good.text());
    assert_branded(&f, &good, "en", "a good read");
    let filled = std::time::Instant::now();

    flaky.refuse_model(brand::MODEL, Mode::Reads);
    // Inside the bound: the brand of the cell, and the store is not asked.
    let refused_before = flaky.refused();
    let r = f.get(&format!("{path}?lang=en"), &ip(221)).await;
    if filled.elapsed() < std::time::Duration::from_millis(1500) {
        assert_eq!(r.status, 200, "{}", r.text());
        assert_branded(&f, &r, "en", "inside the bound");
        assert_eq!(flaky.refused(), refused_before, "the store was not asked");
    }

    // The bound is over: the request that refills meets the fault; so does the next.
    crate::oauth_tests::brand::wait_until_stale(filled);
    let before = f.everything_stored();
    for (round, from) in [ip(222), ip(223)].into_iter().enumerate() {
        let refused_before = flaky.refused();
        let r = f.get(&format!("{path}?lang=en"), &from).await;
        assert_d74(&r, &format!("round {round}: the page"));
        assert!(
            flaky.refused() > refused_before,
            "round {round}: read again"
        );
    }
    let r = f.post(&path, &ip(224), &form(&f, "Jane Test", "en")).await;
    assert_d74(&r, "the post");
    assert_eq!(f.record("sign_request", &f.req)["state"], "sent");
    assert_eq!(f.events_of("sign_signed").len(), 0);
    assert_eq!(
        f.everything_stored(),
        before,
        "nothing was written but the counters, which the store keeps apart"
    );

    flaky.allow_models();
    let r = f.get(&format!("{path}?lang=en"), &ip(225)).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_branded(&f, &r, "en", "the store answers");
    let r = f.post(&path, &ip(226), &form(&f, "Jane Test", "en")).await;
    assert_eq!(r.status, 303, "{}", r.text());
    assert_eq!(f.record("sign_request", &f.req)["state"], "signed");
}

/// A read of the brand that fails in none of the classes of D74 (the store is up and cannot answer:
/// a missing table, a row that cannot be decoded) is no fault the layer above knows. In a process that
/// has never had a good read the door shows the stand-in, the name of an instance with no brand, with
/// status 200 and no word of D74, and goes on: this is the case `docs/SIGN.md` says is not refused.
/// The words and the consent id do not depend on the brand, and once the store answers the pages are
/// the brand's.
#[tokio::test]
async fn a_first_brand_read_that_fails_in_no_class_of_d74_shows_the_stand_in_and_goes_on() {
    let (flaky, f, path) = flaky_sent().await;
    // The row is written around the reader (`set_brand` asks the reader, which would then have had a
    // good read), so that the first read of this process is the one that fails.
    f.app
        .store
        .backend()
        .apply_checked(&[obop_storage::Op::Put {
            model: brand::MODEL.into(),
            id: brand::ROW_ID.into(),
            fields: [("name", json!("MOHV"))]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        }])
        .unwrap();
    flaky.fail_gets_of(brand::MODEL, "relation \"record\" does not exist");
    let gets = flaky.gets_of(brand::MODEL);

    let r = f.get(&format!("{path}?lang=en"), &ip(230)).await;
    assert!(
        flaky.gets_of(brand::MODEL) > gets,
        "the read was asked and failed"
    );
    assert_eq!(r.status, 200, "{}", r.text());
    assert_plain(&r, "a read that failed in no class of D74");
    assert!(r.headers.get("retry-after").is_none() && r.headers.get("x-obop-applied").is_none());

    // The post goes on and signs, with the id it always records.
    let r = f.post(&path, &ip(231), &form(&f, "Jane Test", "en")).await;
    assert_eq!(r.status, 303, "{}", r.text());
    assert_eq!(
        f.record("sign_request", &f.req)["consent_version"],
        obop_http::sign_text::consent_version_for("en")
    );

    // The store answers: the failed read was not kept, and the page is the brand's.
    flaky.allow_models();
    let r = f.get(&format!("{path}?lang=en"), &ip(232)).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_branded(&f, &r, "en", "the store answers");
}
