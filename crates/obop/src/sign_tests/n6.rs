//! The fixes of stream n6 (docs/ISSUES.md O31, O35, O59, UB14, U60): the revoke and its log line
//! in one batch, the entry of a signature read by its key, the effects of other packages on the
//! requests, the consent id the page showed, and the tests of O31 that were not written.

use super::*;
use obop_storage::blob::BlobBytes;
use obop_storage::testing::Flaky;

/// A fixture over a store whose reads are counted and whose writes can be refused, with the
/// request sent and one link made: (fixture, the store, link id, path).
async fn sent_flaky() -> (Fx, Arc<Flaky>, String, String) {
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
    let (id, path) = f.mint("mia", &f.req.clone()).await;
    (f, flaky, id, path)
}

fn hidden(html: &str, name: &str) -> String {
    let at = html
        .find(&format!("name=\"{name}\" value=\""))
        .unwrap_or_else(|| panic!("no hidden field {name}"));
    let rest = &html[at + name.len() + 15..];
    rest[..rest.find('"').unwrap()].to_string()
}

/// The signer's post with exactly these fields (the helper adds nothing to a form that names
/// `words` or `wid`).
fn signed_form<'a>(f: &'a Fx, extra: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut pairs = vec![
        ("name", "Jane Test"),
        ("agree", "1"),
        ("doc", f.file.as_str()),
    ];
    pairs.extend_from_slice(extra);
    pairs
}

// ---- UB14: the revoke and its line are stored together or not at all -------------------------

#[tokio::test]
async fn a_revoke_is_not_stored_without_its_line_in_the_security_log() {
    for who in ["mia", "admin"] {
        let (f, flaky, link, path) = sent_flaky().await;
        // A log that refuses a line on its own (the best-effort path of before) loses nothing:
        // the line is part of the batch of the revoke.
        flaky.fail_inserts_into("security_event", "the log is full");
        let before = f.events_of("sign_link_revoked").len();
        let body = json!({"id": link, "revoked_at": "2030-01-01T00:00:00Z"});
        let (s, v) = f.api(who, "/api/sign_link/update", body).await;
        assert_eq!(s, 200, "{who}: {v}");
        let revoked = !f.record("sign_link", &link)["revoked_at"].is_null();
        assert!(revoked, "{who}");
        assert_eq!(
            f.events_of("sign_link_revoked").len(),
            before + 1,
            "{who}: the line was written with the revoke"
        );
        assert_eq!(f.get(&path, &ip(31)).await.status, 410, "{who}");

        // A store that refuses the batch refuses both.
        let (f, flaky, link, _) = sent_flaky().await;
        let before = f.events_of("sign_link_revoked").len();
        flaky.refuse_applies(100);
        let body = json!({"id": link, "revoked_at": "2030-01-01T00:00:00Z"});
        let (s, _) = f.api(who, "/api/sign_link/update", body).await;
        flaky.refuse_applies(0);
        let revoked = !f
            .record("sign_link", &link)
            .get("revoked_at")
            .is_none_or(J::is_null);
        let logged = f.events_of("sign_link_revoked").len() - before;
        assert_ne!(s, 200, "{who}");
        assert!(!revoked, "{who}: the link still works");
        assert_eq!(logged, 0, "{who}: and no line says it was revoked");
    }
}

// ---- U60: the entry of a signature is read by its key ------------------------------------------

#[tokio::test]
async fn the_evidence_check_reads_the_entry_of_the_signature_by_its_key() {
    let (f, flaky, _, path) = sent_flaky().await;
    let get = || async {
        let (s, v) = f
            .api("mia", "/api/sign_request/get", json!({"id": f.req}))
            .await;
        assert_eq!(s, 200, "{v}");
        v["value"].get("evidence_check").cloned().unwrap_or(J::Null)
    };
    // Many other lines in the log, of other requests and of other kinds.
    for n in 0..50 {
        obop_commands::audit::record_event(
            &*f.app.store.backend(),
            1_800_000_000 + n,
            &format!("sign_request:other-{n}"),
            "sign_signed",
            "signer:link:x",
            "request other; link x; digest y",
        );
    }
    let r = f.post(&path, &ip(60), &signed_form(&f, &[])).await;
    assert_eq!(r.status, 303, "{}", r.text());
    flaky.reset_reads();
    assert_eq!(get().await, "matches");
    let reads = flaky.reads();
    // One `find` is left: the links of the request; the log is read by key.
    assert_eq!(reads.finds, 1, "{reads:?}");
    assert!(reads.find_rows <= 1, "{reads:?}");
    assert!(flaky.gets_of("security_event") >= 1, "{reads:?}");

    // An entry of before (a random key) is still found, by the search of the log.
    let backend = f.app.store.backend();
    let (key, entry) = backend
        .find(
            "security_event",
            "user",
            &J::from(format!("sign_request:{}", f.req)),
        )
        .unwrap()
        .into_iter()
        .find(|(_, e)| e["kind"] == "sign_signed")
        .unwrap();
    backend.delete("security_event", &key).unwrap();
    assert_eq!(get().await, "no_entry");
    backend
        .insert("security_event", "000000000001-aaaaaaaaaaaa", &entry)
        .unwrap();
    assert_eq!(get().await, "matches");
}

// ---- O35: an effect of another package does not write the requests -----------------------------

/// A package whose action changes the request it refers to.
fn writer(effect: &str) -> String {
    format!(
        r#"
[module]
id = "probe"
version = "0.1.0"
name = "Probe"
description = "A probe"
requires_core = "^0.2"

[module.dependencies]
sign = ">=0.3, <1"

[[model]]
name = "probe_note"
description = "A note"
read_group = "sign.user"
write_group = "sign.user"

  [[model.field]]
  name = "title"
  description = "The title"
  class = "none"

  [[model.field]]
  name = "request_id"
  description = "The request"
  class = "none"
  kind = "ref"
  to = "sign_request"

  [[model.field]]
  name = "state"
  description = "new or done"
  class = "none"
  kind = "select"
  options = ["new", "done"]

  [[model.action]]
  name = "apply"
  description = "Apply the note"
  field = "state"
  from = "new"
  to = "done"

    [[model.action.effect]]
    {effect}
"#
    )
}

#[tokio::test]
async fn a_package_whose_effect_writes_a_request_or_a_link_is_refused_at_install() {
    let f = fixture().await;
    for effect in [
        r#"kind = "update"
    model = "sign_request"
    ref = "request_id"
    set = { signed_name = "$self.title" }"#,
        r#"kind = "create"
    model = "sign_link"
    map = { request_id = "$self.request_id" }"#,
    ] {
        let e = f
            .app
            .installer
            .install(&writer(effect), "upload", "test")
            .expect_err(effect);
        assert!(
            e.contains("which only the actions of its own module may change"),
            "{e}"
        );
    }
    // The same package with an effect on its own model installs.
    f.app
        .installer
        .install(
            &writer(
                r#"kind = "create"
    model = "probe_note"
    map = { title = "$self.title" }"#,
            ),
            "upload",
            "test",
        )
        .unwrap();
}

#[tokio::test]
async fn an_effect_of_another_module_registered_around_the_gate_is_refused_when_it_runs() {
    use obop_commands::{Surface, Vault};
    use obop_registry::{CommandDef, Effect, FieldDef, ModelDef, Module};
    let f = fixture().await;
    let g = "sign.user";
    let field = |n: &'static str| FieldDef::new(n, "d", obop_types::DataClass::None, false, g);
    let module = Module {
        id: "forced".into(),
        name: "forced".into(),
        models: vec![ModelDef {
            name: "forced_note",
            description: "d",
            read_group: g,
            write_group: g,
            access: None,
            fields: vec![
                field("title"),
                field("request_id").reference("sign_request"),
                field("state").select(&["new", "done"]),
            ],
            commands: vec![
                CommandDef::new("apply", "Write the request", g)
                    .transition("state", Some("new"), "done")
                    .effect(Effect::update_ref(
                        "sign_request",
                        "request_id",
                        &[("signed_name", "$self.title")],
                    )),
            ],
            ext: Default::default(),
        }],
        network_destinations: vec![],
        description: String::new(),
        translations: Default::default(),
        meta: Default::default(),
    };
    let d = &f.app.dispatcher;
    d.register_crud(&module, &f.app.store);
    d.register_transition(&module, "forced_note", "apply", &f.app.store);
    let who = obop_commands::Principal::user("admin", &["platform.admin"]);
    let input = |pairs: &[(&str, &str)]| {
        obop_commands::Value::Map(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), obop_commands::Value::text(v)))
                .collect(),
        )
    };
    let made = d
        .call(
            Surface::Rest,
            &who,
            &mut Vault::new(),
            "forced_note.create",
            &input(&[
                ("title", "Forged"),
                ("request_id", &f.req),
                ("state", "new"),
            ]),
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
    let id = made.value.to_json()["id"].as_str().unwrap().to_string();
    let e = d
        .call(
            Surface::Rest,
            &who,
            &mut Vault::new(),
            "forced_note.apply",
            &input(&[("id", &id)]),
        )
        .expect_err("the effect writes a request");
    assert!(
        format!("{e:?}").contains("changed by the actions of its own module only"),
        "{e:?}"
    );
    assert!(
        f.record("sign_request", &f.req)
            .get("signed_name")
            .is_none_or(J::is_null)
    );
}

// ---- O59: the consent id of the page is compared at the post -----------------------------------

#[tokio::test]
async fn a_post_whose_page_showed_other_texts_or_says_nothing_of_them_is_shown_the_page_again() {
    let (f, _, path, _) = sent().await;
    let page = f.get(&path, &ip(59)).await;
    assert_eq!(page.status, 200);
    let html = page.text();
    let (words, wid) = (hidden(&html, "words"), hidden(&html, "wid"));
    assert_eq!(wid, obop_http::sign_text::consent_version_for("en"));
    // The texts the post applies differ from the ones the page showed (a deploy in between,
    // here the page of another language posted as English).
    let other = obop_http::sign_text::consent_version_for("de");
    for (case, extra) in [
        (
            "other texts",
            vec![("words", words.as_str()), ("wid", other.as_str())],
        ),
        ("no wid", vec![("words", words.as_str()), ("wid", "")]),
        ("no words", vec![("words", ""), ("wid", wid.as_str())]),
    ] {
        let r = f.post(&path, &ip(59), &signed_form(&f, &extra)).await;
        assert_eq!(r.status, 400, "{case}: {}", r.text());
        assert!(
            r.text()
                .contains("The page has changed since you opened it"),
            "{case}"
        );
        assert_eq!(f.record("sign_request", &f.req)["state"], "sent", "{case}");
        assert!(f.events_of("sign_signed").is_empty(), "{case}");
    }
    // The page shown again carries the fields, and signs.
    let r = f
        .post(
            &path,
            &ip(59),
            &signed_form(&f, &[("words", &words), ("wid", &wid)]),
        )
        .await;
    assert_eq!(r.status, 303, "{}", r.text());
    assert_eq!(
        f.record("sign_request", &f.req)["consent_version"],
        J::from(wid)
    );
}

// ---- O31: the tests that were not written ------------------------------------------------------

#[tokio::test]
async fn the_limiter_lets_a_signer_through_when_the_store_refuses_its_counts() {
    // A pool that is busy or gone is a fault of the request (D74): the page is answered 503 with
    // nothing applied, as every other request is, and the limiter is not what decides it.
    let (f, flaky, _, path) = sent_flaky().await;
    flaky.refuse_counters_busy(1_000);
    assert_eq!(f.get(&path, &ip(80)).await.status, 503);
    assert_eq!(f.record("sign_request", &f.req)["state"], "sent");
    // A counter that fails on its own (the statement, not the pool) is let through: the 256-bit
    // link is the defence, not the limiter (docs/SIGN.md).
    let (f, flaky, _, path) = sent_flaky().await;
    flaky.refuse_counters(1_000);
    let page = f.get(&path, &ip(80)).await;
    assert_eq!(page.status, 200, "{}", page.text());
    let r = f.post(&path, &ip(80), &signed_form(&f, &[])).await;
    assert_eq!(r.status, 303, "{}", r.text());
    assert_eq!(f.record("sign_request", &f.req)["state"], "signed");
    assert!(flaky.refused() > 0, "the counts were refused");
}

#[tokio::test]
async fn a_held_copy_whose_entry_says_another_type_or_size_is_not_shown() {
    for (key, value) in [
        ("content_type", J::from("text/plain")),
        ("size", J::from(26_u64 * 1024 * 1024 + 1)),
    ] {
        let (f, _, path, _) = sent().await;
        let backend = f.app.store.backend();
        let mut entry = backend
            .get(obop_storage::blob::MODEL, &f.file)
            .unwrap()
            .unwrap();
        entry.insert(key.into(), value);
        backend
            .update(obop_storage::blob::MODEL, &f.file, &entry)
            .unwrap();
        assert_eq!(f.get(&path, &ip(81)).await.status, 409, "{key}");
        let got: Vec<String> = f
            .events_of("sign_refused")
            .iter()
            .map(|e| e["detail"].as_str().unwrap().to_string())
            .collect();
        assert!(
            got.iter().any(|d| d.contains("reason copy_missing;")),
            "{key}: {got:?}"
        );
    }
}

#[tokio::test]
async fn a_link_is_not_made_for_a_held_copy_whose_bytes_changed() {
    let (f, _, _, _) = sent().await;
    // The bytes under the held copy's name are not the file that was sent.
    f.bytes
        .as_ref()
        .unwrap()
        .put(&f.file, b"%PDF-1.4 something else")
        .unwrap();
    let (s, v) = f
        .api("mia", "/api/sign_link/create", json!({"request_id": f.req}))
        .await;
    assert_ne!(s, 200, "{v}");
    assert!(
        v.to_string()
            .contains("the stored file does not match its hash"),
        "{v}"
    );
}
