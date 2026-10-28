//! The message of an assertion in the language of the request (D52 amendment): the label slot
//! `assert.<key>`, the request scope that carries the language, and what `errors::classify`
//! names (`code`, `assert_key`). Without a scope (MCP, the command line, a handler called
//! directly) every refusal is the manifest's own text, byte for byte.

use crate::errors::{self, Kind};
use crate::{CallError, Dispatcher, Principal, Store, Surface, Value, Vault, locale};
use obop_registry::{CommandDef, Effect, FieldDef as F, FieldKind, ModelDef, Module};
use obop_types::DataClass;
use serde_json::json;

const U: &str = "lg.user";

const LABELS: &str = r#"
[labels.sv.model.lg_doc.action.release.assert.positive_total]
message = "Ett dokument behover en positiv summa"

[labels.de.model.lg_doc.action.release.assert.positive_total]
message = "Ein Dokument braucht eine positive Summe"

[labels.de.model.lg_doc.action.release.assert.has_title]
message = "Ein Dokument braucht einen Titel"
"#;

fn f(name: &'static str) -> F {
    F::new(name, "d", DataClass::None, false, U)
}

fn module() -> Module {
    raw().translated(&[LABELS])
}

/// The module without its label tables.
fn raw() -> Module {
    let doc = ModelDef {
        name: "lg_doc",
        description: "d",
        read_group: U,
        write_group: U,
        access: None,
        fields: vec![
            f("title"),
            f("total").kind(FieldKind::Number),
            f("state").select(&["draft", "released"]),
        ],
        commands: vec![
            CommandDef::new("release", "Release", U)
                .transition("state", Some("draft"), "released")
                .assertion_keyed(
                    "total > 0",
                    "A document needs a positive total",
                    "positive_total",
                )
                .assertion_keyed("is_set(title)", "A document needs a title", "has_title")
                .assertion_msg("total < 1000", "The total is too large")
                // A key and no message: the key is named, there is no text to translate.
                .assertion("total != 13")
                .assertion_keyed("total != 14", "Never 14", "not_fourteen"),
        ],
        ext: Default::default(),
    };
    let batch = ModelDef {
        name: "lg_batch",
        description: "d",
        read_group: U,
        write_group: U,
        access: None,
        fields: vec![
            f("doc_id").reference("lg_doc"),
            f("state").select(&["open", "closed"]),
        ],
        commands: vec![
            CommandDef::new("close", "Close", U)
                .transition("state", Some("open"), "closed")
                .effect(Effect::set_state_ref("lg_doc", "doc_id", "release")),
        ],
        ext: Default::default(),
    };
    let mut m = Module {
        id: "lg".into(),
        name: "Lang".into(),
        models: vec![doc, batch],
        ..Module::default()
    };
    // `total != 13` is keyed after the fact: a key without a message.
    m.models[0].commands[0].asserts[3].key = Some("unlucky".into());
    m
}

struct Rig {
    d: Dispatcher,
}

impl Rig {
    fn new() -> Self {
        let module = module();
        let v = obop_registry::check_module(&module);
        assert!(v.is_empty(), "{v:?}");
        let store = Store::open("memory:").unwrap();
        let d = Dispatcher::new();
        d.register_crud(&module, &store);
        for mo in &module.models {
            for c in &mo.commands {
                d.register_transition(&module, mo.name, c.name, &store);
            }
        }
        Self { d }
    }

    fn call(&self, c: &str, input: Value) -> Result<Value, CallError> {
        let who = Principal::user("ann", &[U]);
        self.d
            .call(Surface::Rest, &who, &mut Vault::new(), c, &input)
            .map(|r| r.value)
    }

    fn make(&self, model: &str, pairs: &[(&str, Value)]) -> String {
        let input = Value::Map(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        );
        match self.call(&format!("{model}.create"), input).unwrap() {
            Value::Map(m) => match &m["id"] {
                Value::Text(t) => t.clone(),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    /// A document in the draft state with this total and title.
    fn doc(&self, total: i64, title: &str) -> String {
        self.make(
            "lg_doc",
            &[("total", Value::Int(total)), ("title", Value::text(title))],
        )
    }

    /// The refusal of the action `c` on record `id`, with the classification the surface
    /// would answer with (inside the scope that is open).
    fn refuse(&self, c: &str, id: &str) -> (String, errors::Classified) {
        let input = Value::map([("id", Value::text(id))]);
        match self.call(c, input) {
            Err(e @ CallError::Handler(_)) => {
                let CallError::Handler(m) = &e else {
                    unreachable!()
                };
                (m.clone(), errors::classify(&e))
            }
            other => panic!("{c}: expected a refusal, got {other:?}"),
        }
    }
}

const EN_TOTAL: &str = "A document needs a positive total (assertion total > 0 failed)";

#[test]
fn without_a_scope_every_refusal_is_the_manifests_own_text() {
    let r = Rig::new();
    let id = r.doc(0, "x");
    let (m, c) = r.refuse("lg_doc.release", &id);
    assert_eq!(m, EN_TOTAL);
    // Nothing is named: the plain 400 of a handler string, as it always was.
    assert_eq!((c.status, c.code), (400, "bad_request"));
    assert_eq!(c.body, json!({"error": EN_TOTAL}));
    assert_eq!(c.cli, format!("Handler({EN_TOTAL:?})"));
}

#[test]
fn a_request_in_a_language_gets_the_translated_message_with_the_same_suffix() {
    let r = Rig::new();
    let id = r.doc(0, "x");
    {
        let _s = locale::scope(Some("sv"));
        let (m, c) = r.refuse("lg_doc.release", &id);
        assert_eq!(
            m,
            "Ett dokument behover en positiv summa (assertion total > 0 failed)"
        );
        assert_eq!(c.kind, Kind::Failed);
        assert_eq!((c.status, c.code), (400, "assertion_failed"));
        assert_eq!(c.body["error"], json!(m));
        assert_eq!(c.body["code"], "assertion_failed");
        assert_eq!(c.body["assert_key"], "positive_total");
        assert!(c.body["hint"].is_string());
        // The line of the command line shows the text the engine made.
        assert_eq!(c.cli, format!("Handler({m:?})"));
    }
    {
        let _s = locale::scope(Some("de"));
        let (m, c) = r.refuse("lg_doc.release", &id);
        assert_eq!(
            m,
            "Ein Dokument braucht eine positive Summe (assertion total > 0 failed)"
        );
        assert_eq!(c.body["assert_key"], "positive_total");
    }
    // The record did not move.
    let state = r.call("lg_doc.get", Value::map([("id", Value::text(&id))]));
    let Ok(Value::Map(row)) = state else {
        panic!("{state:?}")
    };
    assert_eq!(row["state"], Value::text("draft"));
}

#[test]
fn a_language_without_a_message_and_the_manifests_own_language_fall_back_to_the_manifest() {
    let r = Rig::new();
    let id = r.doc(0, "x");
    for lang in ["en", "fi", "fr", "pl", "es"] {
        let _s = locale::scope(Some(lang));
        let (m, c) = r.refuse("lg_doc.release", &id);
        assert_eq!(m, EN_TOTAL, "{lang}");
        // The refusal is still named, so a client that translates by itself can.
        assert_eq!(c.body["code"], "assertion_failed", "{lang}");
        assert_eq!(c.body["assert_key"], "positive_total", "{lang}");
    }
    // A scope that asked for no language is the same.
    let _s = locale::scope(None);
    assert_eq!(r.refuse("lg_doc.release", &id).0, EN_TOTAL);
}

#[test]
fn each_assertion_is_translated_by_its_own_key() {
    let r = Rig::new();
    let _s = locale::scope(Some("de"));
    // The title is missing and the total is fine: the second assertion refuses.
    let id = r.doc(5, "");
    let (m, c) = r.refuse("lg_doc.release", &id);
    assert_eq!(
        m,
        "Ein Dokument braucht einen Titel (assertion is_set(title) failed)"
    );
    assert_eq!(c.body["assert_key"], "has_title");
    // `sv` has no message for it: the manifest's.
    drop(_s);
    let _s = locale::scope(Some("sv"));
    let (m, c) = r.refuse("lg_doc.release", &id);
    assert_eq!(
        m,
        "A document needs a title (assertion is_set(title) failed)"
    );
    assert_eq!(c.body["assert_key"], "has_title");
}

#[test]
fn an_assertion_without_a_key_is_never_translated_and_names_no_key() {
    let r = Rig::new();
    let _s = locale::scope(Some("sv"));
    let id = r.doc(5000, "x");
    let (m, c) = r.refuse("lg_doc.release", &id);
    assert_eq!(m, "The total is too large (assertion total < 1000 failed)");
    assert_eq!((c.status, c.code), (400, "assertion_failed"));
    assert!(c.body.get("assert_key").is_none(), "{}", c.body);
}

#[test]
fn a_key_without_a_message_is_named_and_has_nothing_to_translate() {
    let r = Rig::new();
    let _s = locale::scope(Some("sv"));
    let id = r.doc(13, "x");
    let (m, c) = r.refuse("lg_doc.release", &id);
    assert_eq!(m, "assertion failed: total != 13");
    assert_eq!(c.body["assert_key"], "unlucky");
    // And one with a message and a key but no table for the language reads as the manifest.
    let id = r.doc(14, "x");
    let (m, c) = r.refuse("lg_doc.release", &id);
    assert_eq!(m, "Never 14 (assertion total != 14 failed)");
    assert_eq!(c.body["assert_key"], "not_fourteen");
}

#[test]
fn an_action_started_by_an_effect_and_an_implicit_transition_answer_in_the_language_too() {
    let r = Rig::new();
    let id = r.doc(0, "x");
    let batch = r.make(
        "lg_batch",
        &[("doc_id", Value::text(&id)), ("state", Value::text("open"))],
    );
    let _s = locale::scope(Some("sv"));
    let (m, c) = r.refuse("lg_batch.close", &batch);
    assert!(
        m.ends_with(
            "on the related record: Ett dokument behover en positiv summa (assertion total > 0 failed)"
        ),
        "{m}"
    );
    assert_eq!((c.status, c.code), (400, "assertion_failed"), "{m}");
    assert_eq!(c.body["assert_key"], "positive_total");
    assert_eq!(c.body["error"], json!(m));

    // `update` of the governed field runs the same action.
    let input = Value::map([("id", Value::text(&id)), ("state", Value::text("released"))]);
    match r.call("lg_doc.update", input) {
        Err(e @ CallError::Handler(_)) => {
            let CallError::Handler(m) = &e else {
                unreachable!()
            };
            assert!(m.contains("Ett dokument behover en positiv summa"), "{m}");
            assert_eq!(errors::classify(&e).body["assert_key"], "positive_total");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_successful_action_and_other_refusals_are_untouched_by_the_scope() {
    let r = Rig::new();
    let _s = locale::scope(Some("sv"));
    let id = r.doc(5, "x");
    r.call("lg_doc.release", Value::map([("id", Value::text(&id))]))
        .unwrap();
    // Releasing again is refused because of the state, in English, and is no assertion.
    let (m, c) = r.refuse("lg_doc.release", &id);
    assert!(!m.contains("assertion"), "{m}");
    assert_eq!((c.status, c.code), (400, "bad_request"));
    assert_eq!(c.body, json!({"error": m}));
}

#[test]
fn a_note_of_an_earlier_request_does_not_name_a_later_one() {
    let r = Rig::new();
    let id = r.doc(0, "x");
    {
        let _s = locale::scope(Some("sv"));
        let _ = r.refuse("lg_doc.release", &id);
    }
    let _s = locale::scope(Some("sv"));
    // Same thread, new scope: a refusal of another kind is a plain 400.
    let c = errors::classify(&CallError::Handler(EN_TOTAL.into()));
    assert_eq!(c.code, "bad_request");
}

#[test]
fn an_unknown_key_in_a_label_table_is_refused_when_the_module_is_built() {
    let bad = std::panic::catch_unwind(|| {
        raw().translated(&[
            "[labels.sv.model.lg_doc.action.release.assert.positive_totall]\nmessage = \"x\"",
        ])
    });
    assert!(bad.is_err());
    // A key that has no message (`unlucky`) is not one a table can translate either.
    let bad = std::panic::catch_unwind(|| {
        raw().translated(&[
            "[labels.sv.model.lg_doc.action.release.assert.unlucky]\nmessage = \"x\"",
        ])
    });
    assert!(bad.is_err());
}
