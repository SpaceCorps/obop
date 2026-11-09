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
        // The other headers are the ones it had.
        for h in [
            "cache-control",
            "x-frame-options",
            "x-content-type-options",
            "referrer-policy",
            "x-robots-tag",
            "content-type",
        ] {
            assert_eq!(after.h(h), before.h(h), "{name}: {h}");
        }
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
        let r = f.get(&at(&second), &from).await;
        assert_eq!(r.status, 410);
        assert_branded(&f, &r, lang, &format!("{lang}/revoked"));
        // GET of the PDF: a wrong link and a revoked one are pages of the door too.
        let r = f.get(&at(&format!("{wrong}/document.pdf")), &from).await;
        assert_eq!(r.status, 404);
        assert_branded(&f, &r, lang, &format!("{lang}/pdf unknown"));
        let r = f.get(&at(&format!("{second}/document.pdf")), &from).await;
        assert_eq!(r.status, 410);
        assert_branded(&f, &r, lang, &format!("{lang}/pdf revoked"));
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
        let r = post_text(&f, &at(&path), &from).await;
        assert_eq!(r.status, 400);
        assert_branded(&f, &r, lang, &format!("{lang}/not a form"));
        let r = f.post(&path, &from, &form(&f, "", lang)).await;
        assert_eq!(r.status, 400);
        assert!(r.text().contains("<form"), "the form is shown again");
        assert!(r.text().contains("role=\"alert\""), "with the error");
        assert_branded(&f, &r, lang, &format!("{lang}/form again"));
        // POST to a link that is over: the refusal of the post.
        let r = f.post(&second, &from, &form(&f, "Jane Test", lang)).await;
        assert_eq!(r.status, 410);
        assert_branded(&f, &r, lang, &format!("{lang}/post revoked"));
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
    let get = |p: String| {
        send(
            &router,
            Request::get(p)
                .header("host", HOST)
                .body(Body::empty())
                .unwrap(),
        )
    };
    let plain = get(format!("/sign/abc/{tok}")).await;
    assert_eq!(plain.status, 404);
    assert_plain(&plain, "no brand");
    set_brand(&app);
    let shown = app.dispatcher.brand().current();
    for lang in LANGS {
        let line = brand_line(&shown, lang);
        for (what, r) in [
            ("page", get(format!("/sign/abc/{tok}?lang={lang}")).await),
            (
                "pdf",
                get(format!("/sign/abc/{tok}/document.pdf?lang={lang}")).await,
            ),
            (
                "post",
                send(
                    &router,
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

/// The consent id is hashed from the texts the signer relied on, and a brand has no word in them:
/// the id a signature records is the same on an instance with a brand, with and without the history
/// of the file, and it is the id the texts give.
#[tokio::test]
async fn a_signature_made_with_a_brand_records_the_same_consent_id() {
    // Without a brand and with one, the same request in two languages.
    let mut ids = Vec::new();
    for (branded, lang) in [(false, "en"), (true, "en"), (false, "de"), (true, "de")] {
        let (f, _, path, _) = sent().await;
        if branded {
            set_brand(&f.app);
        }
        let r = f.post(&path, &ip(100), &form(&f, "Jane Test", lang)).await;
        assert_eq!(r.status, 303, "{}", r.text());
        let id = f.record("sign_request", &f.req)["consent_version"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(id, obop_http::sign_text::consent_version_for(lang));
        ids.push((branded, lang, id));
    }
    assert_eq!(ids[0].2, ids[1].2, "English: a brand moved the consent id");
    assert_eq!(ids[2].2, ids[3].2, "German: a brand moved the consent id");
    assert_ne!(ids[0].2, ids[2].2, "the languages have ids of their own");
    // The ids written out (the unit test of `sign_text.rs` pins the same literals).
    assert_eq!(ids[1].2, "v1-draft/en/a05c68c3");
    assert_eq!(ids[3].2, "v1-draft/de/9ebff173");

    // The request that shows the history: the id with the mark, with a brand as without.
    let mut with_history = Vec::new();
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
        let mut pairs = form(&f, "Alice Example", "en");
        pairs.push(("hist", "1"));
        let r = f.post(&link, &ip(101), &pairs).await;
        assert_eq!(r.status, 303, "{}", r.text());
        with_history.push(
            f.record("sign_request", &id)["consent_version"]
                .as_str()
                .unwrap()
                .to_string(),
        );
    }
    assert_eq!(with_history[0], with_history[1]);
    assert_eq!(with_history[1], "v1-draft/en/e2880db0/history");
    assert_eq!(
        with_history[1],
        obop_http::sign_text::consent_version_with_history_for("en")
    );
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
