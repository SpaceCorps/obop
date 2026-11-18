//! K63: an update cannot tell an agent whether a value it sends equals the stored one for a field
//! it is shown masked. A value sent in clear for such a field counts as a change whatever it is;
//! only the token the agent was shown stands for the value unchanged. Both rules that compare a
//! sent value with the stored one answer the same for a right and a wrong guess: the frozen guard
//! of D77 and the rule that a wrapped action is not mixed with an edit.

use crate::{CallError, Dispatcher, Principal, Store, Surface, Value, Vault, errors};
use obop_registry::{CommandDef, FieldDef as F, Guard, ModelDef, Module};
use obop_types::DataClass;

const U: &str = "mg.user";

fn module() -> Module {
    let f =
        |name: &'static str, personal: bool| F::new(name, "d", DataClass::Internal, personal, U);
    let doc = ModelDef {
        name: "mg_doc",
        description: "d",
        read_group: U,
        write_group: U,
        access: None,
        fields: vec![
            f("title", false),
            f("holder", true),
            f("state", false).select(&["draft", "posted"]),
        ],
        commands: vec![
            CommandDef::new("post", "Post", U).transition("state", Some("draft"), "posted"),
            CommandDef::new("reopen", "Reopen", U)
                .transition("state", Some("posted"), "draft")
                .approval(),
        ],
        ext: Default::default(),
    }
    .with_guard(
        Guard::frozen_in("state", &["posted"])
            .mutable(&["title"])
            .exit("draft", "reopen", &[]),
    );
    Module {
        id: "mg".into(),
        name: "Guess".into(),
        models: vec![doc],
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
        d.register_transition(&module, "mg_doc", "post", &store);
        d.register_transition(&module, "mg_doc", "reopen", &store);
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

    fn text(v: &Value, k: &str) -> String {
        match v {
            Value::Map(m) => match &m[k] {
                Value::Text(t) => t.clone(),
                other => panic!("{k}: {other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }
}

fn update(id: &str, field: &str, value: &str) -> Value {
    Value::map([("id", Value::text(id)), (field, Value::text(value))])
}

#[test]
fn a_guess_of_a_masked_value_of_a_frozen_record_is_refused_right_or_wrong() {
    let r = Rig::new();
    let mut none = Vault::new();
    let doc = r
        .call(
            &r.person,
            &mut none,
            "mg_doc.create",
            Value::map([
                ("title", Value::text("T")),
                ("holder", Value::text("Alice")),
            ]),
        )
        .unwrap();
    let id = Rig::text(&doc, "id");
    r.call(
        &r.person,
        &mut none,
        "mg_doc.post",
        Value::map([("id", Value::text(&id))]),
    )
    .unwrap();

    // The agent reads the record and is shown the holder masked.
    let mut vault = Vault::new();
    let seen = r
        .call(
            &r.agent,
            &mut vault,
            "mg_doc.get",
            Value::map([("id", Value::text(&id))]),
        )
        .unwrap();
    let token = Rig::text(&seen, "holder");
    assert_ne!(token, "Alice", "the agent is shown a token");

    // A right and a wrong guess get the same answer: the frozen record refuses both.
    let mut answers = Vec::new();
    for guess in ["Alice", "Bob"] {
        let e = r
            .call(
                &r.agent,
                &mut vault,
                "mg_doc.update",
                update(&id, "holder", guess),
            )
            .unwrap_err();
        let c = errors::classify(&e);
        assert_eq!(c.code, "immutable_record", "{guess}: {:?}", c.body);
        answers.push(c.status);
    }
    assert_eq!(answers[0], answers[1]);
    // The token it was shown stands for the value unchanged, and a mutable field still changes.
    let input = Value::map([
        ("id", Value::text(&id)),
        ("holder", Value::text(&token)),
        ("title", Value::text("New")),
    ]);
    r.call(&r.agent, &mut vault, "mg_doc.update", input)
        .unwrap();
    // A person sees the value in clear and may send it back unchanged.
    r.call(
        &r.person,
        &mut none,
        "mg_doc.update",
        update(&id, "holder", "Alice"),
    )
    .unwrap();
}

#[test]
fn a_plan_an_agent_proposed_keeps_what_it_sent_in_clear_until_a_person_approves_it() {
    // K63, the rest: an update that reopens a posted record needs a person's approval, and the
    // agent puts a guess of the masked holder in it. Approving the plan answers the same for a
    // right and a wrong guess.
    let r = Rig::new();
    let mut none = Vault::new();
    let mut answers = Vec::new();
    for guess in ["Alice", "Bob"] {
        let doc = r
            .call(
                &r.person,
                &mut none,
                "mg_doc.create",
                Value::map([
                    ("title", Value::text("T")),
                    ("holder", Value::text("Alice")),
                ]),
            )
            .unwrap();
        let id = Rig::text(&doc, "id");
        r.call(
            &r.person,
            &mut none,
            "mg_doc.post",
            Value::map([("id", Value::text(&id))]),
        )
        .unwrap();
        let mut vault = Vault::new();
        let input = Value::map([
            ("id", Value::text(&id)),
            ("state", Value::text("draft")),
            ("holder", Value::text(guess)),
        ]);
        let plan = match r.call(&r.agent, &mut vault, "mg_doc.update", input) {
            Err(CallError::ApprovalRequired { plan_id }) => plan_id,
            other => panic!("{guess}: expected a plan, got {other:?}"),
        };
        let e =
            r.d.approve(&r.person, &mut Vault::new(), &plan)
                .map(|_| ())
                .unwrap_err();
        answers.push(errors::classify(&e).code);
    }
    assert_eq!(answers[0], answers[1], "{answers:?}");
    assert_eq!(answers[0], "immutable_record");
}
