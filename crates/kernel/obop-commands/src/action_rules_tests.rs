//! The checks an action makes before it runs, at every depth: an action that an effect starts is
//! held to its `from` state and to the fields it `needs`, as the action the caller named is (K56);
//! and an action whose assertions fail names every one of them in one refusal (Q17).

use crate::errors::{self, Kind};
use crate::{CallError, Dispatcher, Principal, Store, Surface, Value, Vault, locale};
use obop_registry::{CommandDef, Effect, FieldDef as F, FieldKind, ModelDef, Module};
use obop_types::DataClass;

const U: &str = "ar.user";

fn f(name: &'static str) -> F {
    F::new(name, "d", DataClass::None, false, U)
}

fn model(name: &'static str, fields: Vec<F>, commands: Vec<CommandDef>) -> ModelDef {
    ModelDef {
        name,
        description: "d",
        read_group: U,
        write_group: U,
        access: None,
        fields,
        commands,
        ext: Default::default(),
    }
}

fn module() -> Module {
    let doc = model(
        "ar_doc",
        vec![
            f("title"),
            f("total").kind(FieldKind::Number),
            f("reason"),
            f("state").select(&["draft", "released", "held"]),
        ],
        vec![
            CommandDef::new("release", "Release", U).transition("state", Some("draft"), "released"),
            CommandDef::new("hold", "Hold", U)
                .transition("state", None, "held")
                .needs(&["reason"]),
            CommandDef::new("check", "Check", U)
                .transition("state", Some("draft"), "released")
                .assertion_keyed("total > 0", "A document needs a positive total", "positive")
                .assertion_keyed("is_set(title)", "A document needs a title", "titled")
                .assertion_msg("total < 1000", "The total is too large"),
        ],
    );
    let batch = model(
        "ar_batch",
        vec![
            f("doc_id").reference("ar_doc"),
            f("card_id").reference("ar_card"),
            f("state").select(&["open", "closed"]),
        ],
        vec![
            CommandDef::new("close", "Close", U)
                .transition("state", Some("open"), "closed")
                .effect(Effect::set_state_ref("ar_doc", "doc_id", "release")),
            CommandDef::new("pause", "Pause", U)
                .transition("state", Some("open"), "closed")
                .effect(Effect::set_state_ref("ar_doc", "doc_id", "hold")),
            CommandDef::new("seal_card", "Seal the card", U)
                .transition("state", Some("open"), "closed")
                .effect(Effect::set_state_ref("ar_card", "card_id", "seal")),
        ],
    );
    let card = model(
        "ar_card",
        vec![
            f("label").required(),
            f("state").select(&["draft", "sealed"]),
        ],
        vec![CommandDef::new("seal", "Seal", U).transition("state", None, "sealed")],
    );
    Module {
        id: "ar".into(),
        name: "Rules".into(),
        models: vec![doc, batch, card],
        ..Module::default()
    }
}

struct Rig {
    d: Dispatcher,
    store: Store,
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
        Self { d, store }
    }

    fn call(&self, c: &str, input: Value) -> Result<Value, CallError> {
        let who = Principal::user("ann", &[U]);
        self.d
            .call(Surface::Rest, &who, &mut Vault::new(), c, &input)
            .map(|r| r.value)
    }

    fn id(v: Value) -> String {
        match v {
            Value::Map(m) => match &m["id"] {
                Value::Text(t) => t.clone(),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    fn make(&self, model: &str, pairs: &[(&str, Value)]) -> String {
        let input = Value::Map(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        );
        Self::id(self.call(&format!("{model}.create"), input).unwrap())
    }

    fn act(&self, c: &str, id: &str) -> Result<Value, CallError> {
        self.call(c, Value::map([("id", Value::text(id))]))
    }

    fn state(&self, model: &str, id: &str) -> String {
        match self.act(&format!("{model}.get"), id).unwrap() {
            Value::Map(m) => match &m["state"] {
                Value::Text(t) => t.clone(),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    fn refusal(&self, c: &str, id: &str) -> (String, errors::Classified) {
        match self.act(c, id) {
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

/// K56: the effect starts `release` on a document that is no longer a draft.
#[test]
fn an_action_started_by_an_effect_is_held_to_its_from_state() {
    let r = Rig::new();
    let doc = r.make("ar_doc", &[("title", Value::text("t"))]);
    r.act("ar_doc.release", &doc).unwrap();
    let batch = r.make(
        "ar_batch",
        &[
            ("doc_id", Value::text(&doc)),
            ("state", Value::text("open")),
        ],
    );
    let (m, c) = r.refusal("ar_batch.close", &batch);
    assert!(m.contains("state must be draft to do this"), "{m}");
    assert_eq!(c.kind, Kind::Failed);
    // Nothing of the action stayed.
    assert_eq!(r.state("ar_batch", &batch), "open");
    assert_eq!(r.state("ar_doc", &doc), "released");
    // On a draft the same effect goes through.
    let draft = r.make("ar_doc", &[("title", Value::text("t"))]);
    let batch = r.make(
        "ar_batch",
        &[
            ("doc_id", Value::text(&draft)),
            ("state", Value::text("open")),
        ],
    );
    r.act("ar_batch.close", &batch).unwrap();
    assert_eq!(r.state("ar_doc", &draft), "released");
}

/// K56: the effect starts `hold`, which needs a reason the document does not have.
#[test]
fn an_action_started_by_an_effect_is_held_to_the_fields_it_needs() {
    let r = Rig::new();
    let doc = r.make("ar_doc", &[("title", Value::text("t"))]);
    let batch = r.make(
        "ar_batch",
        &[
            ("doc_id", Value::text(&doc)),
            ("state", Value::text("open")),
        ],
    );
    let (m, _) = r.refusal("ar_batch.pause", &batch);
    assert!(m.contains("ar_doc.hold needs reason"), "{m}");
    assert_eq!(r.state("ar_doc", &doc), "draft");
    assert_eq!(r.state("ar_batch", &batch), "open");
}

/// K56: the effect starts `seal` on a card whose required label is empty (a record stored before
/// the field was required): the action is refused as the same action called by name would be.
#[test]
fn an_action_started_by_an_effect_is_held_to_the_required_fields() {
    let r = Rig::new();
    let empty = [("state".to_string(), Value::text("draft"))];
    let card = r
        .store
        .insert("ar_card", empty.into_iter().collect())
        .unwrap();
    let batch = r.make(
        "ar_batch",
        &[
            ("card_id", Value::text(&card)),
            ("state", Value::text("open")),
        ],
    );
    let (m, _) = r.refusal("ar_batch.seal_card", &batch);
    assert!(
        m.contains("ar_card.label is required and must be set before state can change"),
        "{m}"
    );
    assert_eq!(r.state("ar_card", &card), "draft");
    assert_eq!(r.state("ar_batch", &batch), "open");
    // With its label the same effect seals the card.
    let card = r.make("ar_card", &[("label", Value::text("C1"))]);
    let batch = r.make(
        "ar_batch",
        &[
            ("card_id", Value::text(&card)),
            ("state", Value::text("open")),
        ],
    );
    r.act("ar_batch.seal_card", &batch).unwrap();
    assert_eq!(r.state("ar_card", &card), "sealed");
}

/// Q17: every assertion that fails is named, in the order they are declared.
#[test]
fn an_action_names_every_assertion_that_fails() {
    let r = Rig::new();
    let _s = locale::scope(None);
    let doc = r.make("ar_doc", &[("total", Value::Int(0))]);
    let (m, c) = r.refusal("ar_doc.check", &doc);
    assert_eq!(
        m,
        "A document needs a positive total (assertion total > 0 failed); A document needs a title (assertion is_set(title) failed)"
    );
    assert_eq!((c.status, c.code), (400, "assertion_failed"));
    assert_eq!(c.body["assert_key"], "positive");
    assert_eq!(
        c.body["message"],
        "A document needs a positive total. A document needs a title."
    );
    // One that fails alone is named alone, as before.
    let doc = r.make(
        "ar_doc",
        &[("total", Value::Int(5000)), ("title", Value::text("t"))],
    );
    let (m, c) = r.refusal("ar_doc.check", &doc);
    assert_eq!(m, "The total is too large (assertion total < 1000 failed)");
    assert_eq!(c.body["message"], "The total is too large");
    assert!(c.body.get("assert_key").is_none(), "{}", c.body);
}
