//! The message of an assertion in the language of the request (D52 amendment), end to end over
//! HTTP against a real `App` with a package installed from text: the order of the sources
//! (`?lang=`, the `obop_lang` cookie, `Accept-Language`), the fallback to the manifest's text, the
//! `code` and `assert_key` of the 400, and what MCP and the command line keep.

use crate::app::App;
use axum::{
    body::Body,
    http::{HeaderMap, Request},
};
use http_body_util::BodyExt;
use obop_commands::{ADMIN_GROUP, Authenticator, CallError, Principal, Surface, Value, Vault};
use serde_json::{Value as J, json};
use std::sync::Arc;
use tower::ServiceExt;

const TOKEN: &str = "root-token";

/// One token, one administrator: the package's own groups need not be in a fixed list.
struct Root;

impl Authenticator for Root {
    fn resolve(&self, token: &str) -> Option<Principal> {
        (token == TOKEN).then(|| Principal::user("root", &[ADMIN_GROUP]))
    }
    fn login(&self, _: &str, _: &str) -> Result<String, String> {
        Err("no login in this test".into())
    }
    fn issue_agent_token(&self, _: &Principal, _: &[String], _: &str) -> Result<String, String> {
        Err("no agent tokens in this test".into())
    }
}

const EN_AMOUNT: &str = "An entry needs a positive amount (assertion amount > 0 failed)";
const SV_AMOUNT: &str = "En post behover ett positivt belopp (assertion amount > 0 failed)";
const DE_AMOUNT: &str = "Ein Eintrag braucht einen positiven Betrag (assertion amount > 0 failed)";
const EN_TITLE: &str = "An entry needs a title (assertion is_set(title) failed)";

/// The package without any label table: a keyed assertion and an unkeyed one.
fn manifest() -> String {
    r#"
[module]
id = "ledger"
version = "0.1.0"
name = "Ledger"
description = "Entries"
requires_core = "^0.2"

[[model]]
name = "ledger_entry"
description = "An entry"
read_group = "ledger.user"
write_group = "ledger.user"

  [[model.field]]
  name = "title"
  description = "What it is"
  class = "none"

  [[model.field]]
  name = "amount"
  description = "How much"
  class = "none"
  kind = "number"

  [[model.field]]
  name = "state"
  description = "draft or posted"
  class = "none"
  kind = "select"
  options = ["draft", "posted"]

  [[model.action]]
  name = "post"
  description = "Post a draft"
  field = "state"
  from = "draft"
  to = "posted"
  assert = [
    { expr = "amount > 0", message = "An entry needs a positive amount", key = "positive_amount" },
    { expr = "is_set(title)", message = "An entry needs a title" },
  ]
"#
    .to_string()
}

/// The package with the keyed assertion translated to `sv` and `de`, and `labels` after it.
fn package(labels: &str) -> String {
    format!(
        r#"{}
[labels.sv.model.ledger_entry.action.post.assert.positive_amount]
message = "En post behover ett positivt belopp"

[labels.de.model.ledger_entry.action.post.assert.positive_amount]
message = "Ein Eintrag braucht einen positiven Betrag"
{labels}
"#,
        manifest()
    )
}

struct Rig {
    app: App,
    router: axum::Router,
}

impl Rig {
    fn new() -> Self {
        Self::with(&package(""))
    }

    fn with(text: &str) -> Self {
        let app = App::new();
        app.installer
            .install(text, "upload", "root")
            .unwrap_or_else(|e| panic!("the package: {e}"));
        let router = obop_http::router(
            obop_http::HttpState::new(app.dispatcher.clone(), Arc::new(Root)),
            None,
        );
        Self { app, router }
    }

    /// POST `path` as the administrator with these extra headers.
    async fn post(&self, path: &str, body: J, headers: &[(&str, &str)]) -> (u16, HeaderMap, J) {
        let mut req = Request::post(path)
            .header("host", "inst.test")
            .header("authorization", format!("Bearer {TOKEN}"))
            .header("content-type", "application/json");
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let resp = self
            .router
            .clone()
            .oneshot(req.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        let (status, headers) = (resp.status().as_u16(), resp.headers().clone());
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json = serde_json::from_slice(&bytes).unwrap_or(J::Null);
        (status, headers, json)
    }

    /// A draft entry with this amount and title; the id.
    async fn entry(&self, amount: i64, title: &str) -> String {
        let (s, _, b) = self
            .post(
                "/api/ledger_entry/create",
                json!({"amount": amount, "title": title}),
                &[],
            )
            .await;
        assert_eq!(s, 200, "{b}");
        b["value"]["id"].as_str().unwrap().to_string()
    }

    /// Post the entry `id` with these headers and a `?lang=` if given: the 400, or a panic.
    async fn refused(&self, id: &str, query: &str, headers: &[(&str, &str)]) -> J {
        let (s, _, b) = self
            .post(
                &format!("/api/ledger_entry/post{query}"),
                json!({ "id": id }),
                headers,
            )
            .await;
        assert_eq!(s, 400, "{b}");
        b
    }
}

#[tokio::test]
async fn a_request_without_a_language_is_answered_as_it_always_was() {
    let r = Rig::new();
    let id = r.entry(0, "x").await;
    let b = r.refused(&id, "", &[]).await;
    // The text is byte for byte the manifest's, with the suffix.
    assert_eq!(b["error"], EN_AMOUNT);
    assert_eq!(b["code"], "assertion_failed");
    assert_eq!(b["assert_key"], "positive_amount");
    assert!(b["hint"].is_string());
    // An assertion without a key has a code and no key.
    let id = r.entry(5, "").await;
    let b = r.refused(&id, "", &[]).await;
    assert_eq!(b["error"], EN_TITLE);
    assert_eq!(b["code"], "assertion_failed");
    assert!(b.get("assert_key").is_none(), "{b}");
    // And the record did not move.
    let (_, _, got) = r
        .post("/api/ledger_entry/get", json!({"id": id}), &[])
        .await;
    assert_eq!(got["value"]["state"], "draft");
}

#[tokio::test]
async fn accept_language_picks_the_swedish_text_and_english_otherwise() {
    let r = Rig::new();
    let id = r.entry(0, "x").await;
    for (accept, want) in [
        ("sv", SV_AMOUNT),
        ("sv-SE", SV_AMOUNT),
        ("sv-SE,sv;q=0.9,en;q=0.5", SV_AMOUNT),
        ("de;q=0.4, sv;q=0.8", SV_AMOUNT),
        ("de", DE_AMOUNT),
        ("en", EN_AMOUNT),
        ("en-US,en;q=0.9,sv;q=0.8", EN_AMOUNT),
        // q=0 means "not acceptable".
        ("sv;q=0, en", EN_AMOUNT),
        // A language nobody ships falls through; so does one that is shipped but not translated.
        ("xx, yy;q=0.4", EN_AMOUNT),
        ("fi", EN_AMOUNT),
        ("pt-BR, fr", EN_AMOUNT),
        ("xx, sv;q=0.2", SV_AMOUNT),
    ] {
        let b = r.refused(&id, "", &[("accept-language", accept)]).await;
        assert_eq!(b["error"], want, "Accept-Language: {accept}");
        // The same body shape in every language.
        assert_eq!(b["code"], "assertion_failed", "{accept}");
        assert_eq!(b["assert_key"], "positive_amount", "{accept}");
    }
}

#[tokio::test]
async fn the_query_beats_the_cookie_which_beats_the_header() {
    let r = Rig::new();
    let id = r.entry(0, "x").await;
    // ?lang= over cookie over header.
    let b = r
        .refused(
            &id,
            "?lang=de",
            &[("cookie", "obop_lang=sv"), ("accept-language", "fi")],
        )
        .await;
    assert_eq!(b["error"], DE_AMOUNT);
    // The cookie over the header.
    let b = r
        .refused(
            &id,
            "",
            &[
                ("cookie", "a=b; obop_lang=sv; c=d"),
                ("accept-language", "de"),
            ],
        )
        .await;
    assert_eq!(b["error"], SV_AMOUNT);
    // The header when there is neither.
    let b = r.refused(&id, "", &[("accept-language", "de")]).await;
    assert_eq!(b["error"], DE_AMOUNT);
    // A source that names no language of ours falls through to the next one.
    let b = r
        .refused(
            &id,
            "?lang=xx",
            &[("cookie", "obop_lang=sv"), ("accept-language", "de")],
        )
        .await;
    assert_eq!(b["error"], SV_AMOUNT);
    let b = r
        .refused(
            &id,
            "?lang=",
            &[("cookie", "obop_lang=zz"), ("accept-language", "de")],
        )
        .await;
    assert_eq!(b["error"], DE_AMOUNT);
    // English asked for in the query is English, whatever the others say.
    let b = r
        .refused(
            &id,
            "?lang=en",
            &[("cookie", "obop_lang=sv"), ("accept-language", "de")],
        )
        .await;
    assert_eq!(b["error"], EN_AMOUNT);
    assert_eq!(b["assert_key"], "positive_amount");
    // A region is read as its language, in the query and the cookie too.
    let b = r.refused(&id, "?lang=sv-SE", &[]).await;
    assert_eq!(b["error"], SV_AMOUNT);
    let b = r.refused(&id, "", &[("cookie", "obop_lang=de-AT")]).await;
    assert_eq!(b["error"], DE_AMOUNT);
}

#[tokio::test]
async fn a_language_without_a_message_for_the_assertion_reads_as_the_manifest() {
    let r = Rig::new();
    // `fi` is a shipped language and has no table: the manifest's text, still named by its key.
    let id = r.entry(0, "x").await;
    let b = r.refused(&id, "?lang=fi", &[]).await;
    assert_eq!(b["error"], EN_AMOUNT);
    assert_eq!(b["assert_key"], "positive_amount");
    // The second assertion has no key, so no table can reach it.
    let id = r.entry(5, "").await;
    for lang in ["sv", "de", "fi"] {
        let b = r.refused(&id, &format!("?lang={lang}"), &[]).await;
        assert_eq!(b["error"], EN_TITLE, "{lang}");
    }
}

#[tokio::test]
async fn the_approval_of_a_plan_answers_in_the_language_too() {
    // An approver pressing "approve" on a plan whose action no longer holds is refused the same.
    let r =
        Rig::with(&package("").replace("name = \"post\"", "name = \"post\"\n  approval = true"));
    let id = r.entry(0, "x").await;
    let agent = r
        .app
        .dispatcher
        .call(
            Surface::Rest,
            &Principal::agent("bot", &["ledger.user"], &["ledger.user"]),
            &mut Vault::new(),
            "ledger_entry.post",
            &Value::map([("id", Value::text(&id))]),
        )
        .unwrap_err();
    // The assertions are checked when the plan runs, not when it is proposed.
    let CallError::ApprovalRequired { plan_id } = agent else {
        panic!("{agent:?}")
    };
    let (s, _, b) = r
        .post(
            &format!("/api/approve/{plan_id}"),
            json!({}),
            &[("accept-language", "sv")],
        )
        .await;
    assert_eq!(s, 400, "{b}");
    assert_eq!(b["error"], SV_AMOUNT);
    assert_eq!(b["assert_key"], "positive_amount");
}

#[tokio::test]
async fn mcp_keeps_the_manifests_message_whatever_the_header_says() {
    let r = Rig::new();
    let id = r.entry(0, "x").await;
    let post = |body: J, session: Option<&str>| {
        let mut req = Request::post("/mcp")
            .header("host", "inst.test")
            .header("authorization", format!("Bearer {TOKEN}"))
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .header("accept-language", "sv")
            .header("cookie", "obop_lang=sv");
        if let Some(s) = session {
            req = req.header("mcp-session-id", s);
        }
        let router = r.router.clone();
        async move {
            let resp = router
                .oneshot(req.body(Body::from(body.to_string())).unwrap())
                .await
                .unwrap();
            let sid = resp
                .headers()
                .get("mcp-session-id")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            let bytes = resp.into_body().collect().await.unwrap().to_bytes();
            (sid, serde_json::from_slice::<J>(&bytes).unwrap_or(J::Null))
        }
    };
    let (sid, _) = post(
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion": "2025-11-25", "capabilities": {},
            "clientInfo": {"name": "test", "version": "0"}}}),
        None,
    )
    .await;
    assert!(!sid.is_empty());
    let _ = post(
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        Some(&sid),
    )
    .await;
    let (_, answer) = post(
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name": "ledger_entry__post", "arguments": {"id": id}}}),
        Some(&sid),
    )
    .await;
    let text = answer.to_string();
    assert!(text.contains(EN_AMOUNT), "{text}");
    assert!(!text.contains("positivt belopp"), "{text}");
}

#[test]
fn the_dispatcher_outside_rest_keeps_the_manifests_message() {
    // The command line and the stdio MCP server call the dispatcher with no request scope.
    let r = Rig::new();
    let who = Principal::user("root", &[ADMIN_GROUP]);
    let created = r
        .app
        .dispatcher
        .call(
            Surface::Cli,
            &who,
            &mut Vault::new(),
            "ledger_entry.create",
            &Value::map([("amount", Value::Int(0)), ("title", Value::text("x"))]),
        )
        .unwrap();
    let Value::Map(m) = created.value else {
        panic!()
    };
    let Value::Text(id) = &m["id"] else { panic!() };
    for surface in [Surface::Cli, Surface::Mcp] {
        let e = r
            .app
            .dispatcher
            .call(
                surface,
                &who,
                &mut Vault::new(),
                "ledger_entry.post",
                &Value::map([("id", Value::text(id))]),
            )
            .unwrap_err();
        assert_eq!(e, CallError::Handler(EN_AMOUNT.to_string()));
        // Classified, it is the plain 400 it always was: no code, no key.
        let c = obop_commands::errors::classify_with(&e, &Default::default());
        assert_eq!(c.body, json!({"error": EN_AMOUNT}));
        assert_eq!(c.cli, format!("Handler({EN_AMOUNT:?})"));
    }
}

#[test]
fn an_unknown_key_in_a_label_table_is_refused_at_install() {
    let bad = package(
        "[labels.sv.model.ledger_entry.action.post.assert.positive_amout]\nmessage = \"x\"",
    );
    let app = App::new();
    let e = app.installer.install(&bad, "upload", "root").unwrap_err();
    assert!(
        e.contains("labels.sv.model.ledger_entry.action.post.assert.positive_amout")
            && e.contains("no assertion with a message and the key \"positive_amout\""),
        "{e}"
    );
    assert!(app.installer.list().iter().all(|p| p.id != "ledger"));
    // The one the package declares installs.
    assert!(
        app.installer
            .install(&package(""), "upload", "root")
            .is_ok()
    );
}

/// `obop modules check --complete` lists a keyed assertion with no message in a language, the
/// template of `obop modules labels` carries the messages, and an unknown key is an error.
#[test]
fn the_check_command_and_the_template_know_the_assertion_messages() {
    let dir = std::env::temp_dir().join(format!("obop-assert-lang-{}", std::process::id()));
    let ldir = dir.join("labels").join("sv");
    std::fs::create_dir_all(&ldir).unwrap();
    let pkg = dir.join("ledger.toml");
    std::fs::write(&pkg, manifest()).unwrap();
    let file = ldir.join("ledger.toml");
    let builtin = App::new().modules;
    let check = |text: &str, complete: bool| {
        std::fs::write(&file, text).unwrap();
        match obop_pkg::files::read_source(&file).unwrap() {
            obop_pkg::files::Source::Labels {
                stem,
                text,
                manifest,
                allow,
            } => obop_pkg::files::check_labels_source(
                &stem,
                &text,
                manifest.as_deref(),
                &allow,
                complete,
                &builtin,
            ),
            _ => panic!("not a labels file"),
        }
    };
    let slot = "ledger_entry.post.assert.positive_amount.message (sv)";

    // A language with a table but no message for the keyed assertion: listed as missing.
    let without = "[labels.sv.module]
name = \"Huvudbok\"
";
    assert!(check(without, false).is_empty());
    let e = check(without, true);
    assert!(e.iter().any(|x| x == &format!("{slot}: missing")), "{e:?}");
    // The assertion without a key is not a slot: there is nothing to find its text by.
    assert!(
        !e.iter()
            .any(|x| x.contains("is_set") || x.contains("needs a title")),
        "{e:?}"
    );

    // With the message it is not listed any more.
    let with = format!(
        "{without}
[labels.sv.model.ledger_entry.action.post.assert.positive_amount]
message = \"En post behover ett positivt belopp\"
"
    );
    assert!(check(&with, false).is_empty());
    let e = check(&with, true);
    assert!(!e.iter().any(|x| x.contains("assert.")), "{e:?}");

    // Left in English, it is reported like any other text.
    let english = with.replace(
        "En post behover ett positivt belopp",
        "An entry needs a positive amount",
    );
    let e = check(&english, true);
    assert!(
        e.iter()
            .any(|x| x.starts_with(slot) && x.contains("identical to English")),
        "{e:?}"
    );

    // An unknown key, a key of another action, a message too long, and a placeholder.
    let typo = "[labels.sv.model.ledger_entry.action.post.assert.positive_amout]
message = \"x\"
";
    let e = check(typo, false);
    assert!(
        e.len() == 1
            && e[0].contains("assert.positive_amout")
            && e[0].contains("no assertion with a message and the key"),
        "{e:?}"
    );
    let long = format!(
        "[labels.sv.model.ledger_entry.action.post.assert.positive_amount]
message = \"{}\"
",
        "x".repeat(401)
    );
    assert!(check(&long, false)[0].contains("message: value is too long"));
    let ok400 = long.replace(&"x".repeat(401), &"x".repeat(400));
    assert!(check(&ok400, false).is_empty());
    let braces = "[labels.sv.model.ledger_entry.action.post.assert.positive_amount]
message = \"Belopp {x}\"
";
    assert!(check(braces, false)[0].contains("placeholder"));

    // The whole-file entry point reports failure for the typo and success for a good file.
    let path = file.to_string_lossy().to_string();
    std::fs::write(&file, typo).unwrap();
    assert!(crate::modules_check_files_with(&[&path], false, false).is_err());
    std::fs::write(&file, &with).unwrap();
    assert!(crate::modules_check_files_with(&[&path], false, false).is_ok());
    // A package file with the typo in its own tables is refused too.
    let bad_pkg = dir.join("bad.toml");
    std::fs::write(&bad_pkg, package(typo)).unwrap();
    let bad = bad_pkg.to_string_lossy().to_string();
    assert!(crate::modules_check_files_with(&[&bad], false, false).is_err());
    let good_pkg = dir.join("good.toml");
    std::fs::write(&good_pkg, package("")).unwrap();
    let good = good_pkg.to_string_lossy().to_string();
    assert!(crate::modules_check_files_with(&[&good], false, false).is_ok());

    // `obop modules labels FILE LANG`: the message is in the template, ready to translate.
    let t = obop_pkg::files::template_for(&pkg.to_string_lossy(), "fi", &builtin).unwrap();
    assert!(
        t.contains(
            "[labels.fi.model.ledger_entry.action.post.assert.positive_amount]
message = \"An entry needs a positive amount\"
"
        ),
        "{t}"
    );
    assert!(!t.contains("needs a title"), "{t}");
    // The template checks against the package.
    std::fs::write(&file, t.replace("labels.fi", "labels.sv")).unwrap();
    assert!(check(&t.replace("labels.fi", "labels.sv"), false).is_empty());
    let _ = std::fs::remove_dir_all(dir);
}
