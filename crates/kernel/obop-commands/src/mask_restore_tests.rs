//! What an agent is shown of a masked field, and what a token it sends back becomes (D40).
//!
//! O39: a null stays null, and a value that is not text gets a token that restores to the same
//! typed value, so an agent that reads a record and sends it back writes what was there.
//! O37: a token is not restored into a field the agent is shown in clear, where it would read the
//! value the token hides (and so would everyone who reads that field).

use crate::{CallError, Dispatcher, Principal, Store, Surface, Value, Vault};
use obop_registry::{FieldDef as F, FieldKind, ModelDef, Module};
use obop_types::DataClass;

const U: &str = "mr.user";

fn module() -> Module {
    let personal = |name: &'static str| F::new(name, "d", DataClass::Internal, true, U);
    let plain = |name: &'static str| F::new(name, "d", DataClass::None, false, U);
    let card = ModelDef {
        name: "mr_card",
        description: "d",
        read_group: U,
        write_group: U,
        access: None,
        fields: vec![
            personal("holder"),
            personal("nickname"),
            personal("score").kind(FieldKind::Number),
            personal("vip").kind(FieldKind::Bool),
            plain("title"),
            plain("rank").kind(FieldKind::Number),
        ],
        commands: vec![],
        ext: Default::default(),
    };
    Module {
        id: "mr".into(),
        name: "Restore".into(),
        models: vec![card],
        ..Module::default()
    }
}

struct Rig {
    d: Dispatcher,
    person: Principal,
    agent: Principal,
}

impl Rig {
    fn new() -> Self {
        let module = module();
        let v = obop_registry::check_module(&module);
        assert!(v.is_empty(), "{v:?}");
        let store = Store::open("memory:").unwrap();
        let d = Dispatcher::new();
        d.register_crud(&module, &store);
        Self {
            d,
            person: Principal::user("ann", &[U]),
            agent: Principal::agent("ann/agent", &[U], &[U]),
        }
    }

    fn call(
        &self,
        who: &Principal,
        vault: &mut Vault,
        c: &str,
        input: Value,
    ) -> Result<Value, CallError> {
        self.d
            .call(Surface::Rest, who, vault, c, &input)
            .map(|r| r.value)
    }

    fn get(&self, who: &Principal, vault: &mut Vault, id: &str) -> Value {
        self.call(
            who,
            vault,
            "mr_card.get",
            Value::map([("id", Value::text(id))]),
        )
        .unwrap()
    }

    /// A card with a holder, a score and an empty nickname and vip, made by a person.
    fn card(&self) -> String {
        let v = self
            .call(
                &self.person,
                &mut Vault::new(),
                "mr_card.create",
                Value::map([
                    ("holder", Value::text("Ada Lovelace")),
                    ("score", Value::Int(42)),
                    ("title", Value::text("Gold")),
                    ("rank", Value::Int(1)),
                ]),
            )
            .unwrap();
        field(&v, "id").as_text().to_string()
    }
}

fn field(v: &Value, k: &str) -> Value {
    match v {
        Value::Map(m) => m.get(k).cloned().unwrap_or(Value::Null),
        other => panic!("{other:?}"),
    }
}

trait AsText {
    fn as_text(&self) -> &str;
}

impl AsText for Value {
    fn as_text(&self) -> &str {
        match self {
            Value::Text(t) => t,
            other => panic!("not a text: {other:?}"),
        }
    }
}

/// O39: the agent's read; and the same record sent back whole changes nothing.
#[test]
fn an_agent_is_shown_null_as_null_and_a_number_as_a_token_of_that_number() {
    let r = Rig::new();
    let id = r.card();
    let mut vault = Vault::new();
    let seen = r.get(&r.agent, &mut vault, &id);
    assert!(field(&seen, "holder").as_text().starts_with("HOLDER_"));
    assert_eq!(
        field(&seen, "nickname"),
        Value::Null,
        "a null is not a secret"
    );
    assert_eq!(field(&seen, "vip"), Value::Null);
    let score = field(&seen, "score");
    assert!(score.as_text().starts_with("SCORE_"), "{score:?}");
    // Sent back whole (the id, the tokens, the nulls and the plain fields as read).
    let Value::Map(mut back) = seen else { panic!() };
    back.retain(|k, _| k != "row_version");
    r.call(&r.agent, &mut vault, "mr_card.update", Value::Map(back))
        .unwrap();
    let now = r.get(&r.person, &mut Vault::new(), &id);
    assert_eq!(field(&now, "holder"), Value::text("Ada Lovelace"));
    assert_eq!(
        field(&now, "score"),
        Value::Int(42),
        "the number, not a text"
    );
    assert_eq!(
        field(&now, "nickname"),
        Value::Null,
        "not the text [masked]"
    );
    assert_eq!(field(&now, "vip"), Value::Null);
}

/// O37: a token goes back into a masked field, never into one the agent reads in clear.
#[test]
fn an_agent_cannot_write_a_token_into_a_field_it_reads_in_clear() {
    let r = Rig::new();
    let id = r.card();
    let mut vault = Vault::new();
    let seen = r.get(&r.agent, &mut vault, &id);
    let holder = field(&seen, "holder");
    let score = field(&seen, "score");
    for (target, token) in [("title", &holder), ("rank", &score)] {
        let e = r
            .call(
                &r.agent,
                &mut vault,
                "mr_card.update",
                Value::map([("id", Value::text(&id)), (target, token.clone())]),
            )
            .unwrap_err();
        let CallError::Handler(m) = &e else {
            panic!("{e:?}")
        };
        assert!(m.contains(&format!("mr_card.{target}")), "{m}");
        let e = r
            .call(
                &r.agent,
                &mut vault,
                "mr_card.create",
                Value::map([("holder", Value::text("x")), (target, token.clone())]),
            )
            .unwrap_err();
        assert!(matches!(e, CallError::Handler(_)), "{e:?}");
        let e = r
            .call(
                &r.agent,
                &mut vault,
                "mr_card.bulk_create",
                Value::map([(
                    "rows",
                    Value::List(vec![Value::map([
                        ("holder", Value::text("x")),
                        (target, token.clone()),
                    ])]),
                )]),
            )
            .unwrap_err();
        assert!(matches!(e, CallError::Handler(_)), "{e:?}");
    }
    let now = r.get(&r.person, &mut Vault::new(), &id);
    assert_eq!(field(&now, "title"), Value::text("Gold"), "nothing written");
    // Into a field it is shown masked, the token is restored (copying without seeing, D40).
    r.call(
        &r.agent,
        &mut vault,
        "mr_card.update",
        Value::map([("id", Value::text(&id)), ("nickname", holder.clone())]),
    )
    .unwrap();
    let now = r.get(&r.person, &mut Vault::new(), &id);
    assert_eq!(field(&now, "nickname"), Value::text("Ada Lovelace"));
    // A person is never refused for this (a person has no tokens in clear fields to send).
    r.call(
        &r.person,
        &mut Vault::new(),
        "mr_card.update",
        Value::map([("id", Value::text(&id)), ("title", Value::text("HOLDER_1"))]),
    )
    .unwrap();
}
