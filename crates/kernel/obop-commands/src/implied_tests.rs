//! One `update` that moves a governed field and changes other fields is one unit of work (D48:
//! "a mixed update is applied completely or not at all"), also when the state it moves to is
//! frozen by the D77 guard. The other fields are edits of the record as it was, so they are
//! written first; the action the move implies runs after them, on the record as edited; and a
//! refusal of any part (a field, the guard, a wrapper, the action's assertion) leaves nothing
//! stored, no number taken. An agent's call that needs approval changes nothing, and the approved
//! plan does all of it.
//!
//! Every test runs on the memory and SQLite backends, and on PostgreSQL when
//! `OBOP_TEST_POSTGRES_URL` is set.

use crate::{CallError, Dispatcher, Principal, Response, Store, Surface, Value, Vault, errors};
use obop_registry::{CommandDef, FieldDef as F, FieldKind, Guard, ModelDef, Module, Reset, intern};
use obop_types::DataClass;
use std::sync::{Arc, Mutex};

const U: &str = "imp.user";
const APPROVER: &str = "imp.approver";

fn m(pairs: Vec<(&str, Value)>) -> Value {
    Value::Map(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}
fn t(s: &str) -> Value {
    Value::text(s)
}
fn n(i: i64) -> Value {
    Value::Int(i)
}
fn get<'a>(v: &'a Value, k: &str) -> &'a Value {
    match v {
        Value::Map(m) => m.get(k).unwrap_or(&Value::Null),
        other => panic!("not a map: {other:?}"),
    }
}

fn user() -> Principal {
    Principal::user("u", &[U])
}
fn approver() -> Principal {
    Principal::user("ap", &[U, APPROVER])
}
fn bot() -> Principal {
    Principal::agent("bot", &[U, APPROVER], &[U, APPROVER])
}

fn module(p: &str) -> Module {
    let name = |s: &str| intern(&format!("{p}_{s}"));
    let f = |name: &'static str| F::new(name, "d", DataClass::None, false, U);
    let model = |name: &'static str, fields: Vec<F>, commands: Vec<CommandDef>| ModelDef {
        name,
        description: "d",
        read_group: U,
        write_group: U,
        access: None,
        fields,
        commands,
        ext: Default::default(),
    };
    let (doc, period) = (name("doc"), name("period"));
    Module {
        id: p.into(),
        name: "I".into(),
        models: vec![
            model(
                period,
                vec![
                    f("start_on").kind(FieldKind::Date),
                    f("end_on").kind(FieldKind::Date),
                    f("state").select(&["open", "closed"]),
                ],
                vec![CommandDef::new("close", "Close", U).transition(
                    "state",
                    Some("open"),
                    "closed",
                )],
            ),
            // Posting is final and numbers the record from a gapless series; a posted record
            // leaves only by `void`, which takes its reason.
            model(
                doc,
                vec![
                    f("name"),
                    f("note"),
                    f("code").unique(),
                    f("reason"),
                    f("amount").kind(FieldKind::Number),
                    f("date").kind(FieldKind::Date),
                    f("state").select(&["draft", "posted", "cancelled", "void"]),
                    f("number")
                        .sequence("D-{seq:04}", true, Reset::Never)
                        .assign_on("post"),
                ],
                vec![
                    CommandDef::new("post", "Post", APPROVER)
                        .approval()
                        .transition("state", Some("draft"), "posted")
                        .assertion_msg("amount > 0", "A document needs an amount"),
                    CommandDef::new("cancel", "Cancel", U).transition(
                        "state",
                        Some("draft"),
                        "cancelled",
                    ),
                    CommandDef::new("void", "Void", U)
                        .transition("state", Some("posted"), "void")
                        .needs(&["reason"]),
                ],
            )
            .with_guard(
                Guard::frozen_in("state", &["posted", "cancelled"])
                    .mutable(&["note"])
                    .exit("void", "void", &["reason"])
                    .lock_by_closed("date", period),
            ),
            // A child with a state of its own.
            model(
                name("item"),
                vec![
                    f("doc_id").reference(doc),
                    f("what"),
                    f("state").select(&["open", "done"]),
                ],
                vec![CommandDef::new("finish", "Finish", U).transition(
                    "state",
                    Some("open"),
                    "done",
                )],
            )
            .with_guard(Guard::child_of("doc_id")),
            // Two governed fields; `priority` is moved first (the fields go in name order).
            model(
                name("task"),
                vec![
                    f("title"),
                    f("size").kind(FieldKind::Number),
                    f("priority").select(&["normal", "urgent"]),
                    f("state").select(&["open", "closed"]),
                ],
                vec![
                    CommandDef::new("escalate", "Escalate", U)
                        .transition("priority", Some("normal"), "urgent")
                        .assertion_msg("size > 0", "Nothing to escalate"),
                    CommandDef::new("close", "Close", U)
                        .transition("state", Some("open"), "closed")
                        .assertion_msg("size < 10", "Too large to close"),
                ],
            ),
            // `close` is bound to a handler that writes the store itself; `raise` has the
            // generic one and comes after it (the fields go in name order).
            model(
                name("ticket"),
                vec![
                    f("title"),
                    f("note"),
                    f("code").unique(),
                    f("state").select(&["open", "closed"]),
                    f("urgency").select(&["low", "high"]),
                ],
                vec![
                    CommandDef::new("close", "Close", U).transition(
                        "state",
                        Some("open"),
                        "closed",
                    ),
                    CommandDef::new("raise", "Raise", U).transition("urgency", Some("low"), "high"),
                ],
            ),
            // Its `update` is bound to a handler that writes the store itself.
            model(
                name("memo"),
                vec![
                    f("title"),
                    f("size").kind(FieldKind::Number),
                    f("state").select(&["open", "filed"]),
                ],
                vec![
                    CommandDef::new("file", "File", U)
                        .transition("state", Some("open"), "filed")
                        .assertion_msg("size > 0", "Nothing to file"),
                ],
            ),
        ],
        network_destinations: vec![],
        description: String::new(),
        translations: Default::default(),
        meta: Default::default(),
    }
}

struct Rig {
    d: Dispatcher,
    store: Store,
    p: String,
}

fn each(f: impl Fn(&Rig)) {
    let mut urls = vec!["memory:".to_string(), "sqlite::memory:".to_string()];
    if let Ok(u) = std::env::var("OBOP_TEST_POSTGRES_URL") {
        urls.push(u);
    }
    for url in urls {
        f(&rig(Store::open(&url).unwrap()));
    }
}

/// The module of these tests registered over `store`, under a prefix of its own.
fn rig(store: Store) -> Rig {
    {
        let p = format!("i{}", &uuid::Uuid::now_v7().simple().to_string()[22..]);
        let d = Dispatcher::new();
        let module = module(&p);
        let v = obop_registry::check_module(&module);
        assert!(v.is_empty(), "{v:?}");
        d.register_crud(&module, &store);
        for mo in &module.models {
            for c in &mo.commands {
                // `ticket.close` is bound by hand below.
                if mo.name.ends_with("_ticket") && c.name == "close" {
                    continue;
                }
                d.register_transition(&module, mo.name, c.name, &store);
            }
        }
        // `ticket.close` by hand: it reads and writes the store directly, as the handlers of the
        // connectors module do, and refuses a ticket titled "stuck".
        let ticket = format!("{p}_ticket");
        let (s, model) = (store.clone(), ticket.clone());
        d.register(
            &module,
            &ticket,
            "close",
            Box::new(move |input| {
                let Value::Map(i) = input else {
                    return Err("expected an object".into());
                };
                let Some(Value::Text(id)) = i.get("id") else {
                    return Err("missing id".into());
                };
                if s.field(&model, id, "title") == Some(t("stuck")) {
                    return Err("the ticket is stuck".into());
                }
                s.update(
                    &model,
                    id,
                    [("state".to_string(), t("closed"))].into_iter().collect(),
                )?;
                Ok(m(vec![("id", t(id)), ("state", t("closed"))]))
            }),
        );
        // `memo.update` by hand: it writes the store itself, knows nothing of the actions an
        // update implies, and refuses the title "no".
        let memo = format!("{p}_memo");
        let (s, model) = (store.clone(), memo.clone());
        d.register(
            &module,
            &memo,
            "update",
            Box::new(move |input| {
                let Value::Map(i) = input else {
                    return Err("expected an object".into());
                };
                let Some(Value::Text(id)) = i.get("id") else {
                    return Err("missing id".into());
                };
                if i.get("title") == Some(&t("no")) {
                    return Err("the memo says no".into());
                }
                let fields = i
                    .iter()
                    .filter(|(k, _)| k.as_str() != "id" && !k.starts_with('_'))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                s.update(&model, id, fields)?;
                Ok(m(vec![("id", t(id))]))
            }),
        );
        Rig { d, store, p }
    }
}

impl Rig {
    fn cmd(&self, model: &str, c: &str) -> String {
        format!("{}_{model}.{c}", self.p)
    }
    fn call(
        &self,
        who: &Principal,
        model: &str,
        c: &str,
        input: Value,
    ) -> Result<Response, CallError> {
        self.d.call(
            Surface::Rest,
            who,
            &mut Vault::new(),
            &self.cmd(model, c),
            &input,
        )
    }
    fn ok(&self, who: &Principal, model: &str, c: &str, input: Value) -> Value {
        match self.call(who, model, c, input) {
            Ok(r) => r.value,
            Err(e) => panic!("{model}.{c} failed: {e:?}"),
        }
    }
    /// The message of a call that must be refused by a handler or a guard.
    fn err(&self, who: &Principal, model: &str, c: &str, input: Value) -> String {
        match self.call(who, model, c, input) {
            Err(CallError::Handler(m)) => m,
            other => panic!("{model}.{c}: expected a refusal, got {other:?}"),
        }
    }
    /// A refusal that must be the D77 guard's: 409 `immutable_record`.
    fn frozen(&self, who: &Principal, model: &str, c: &str, input: Value) -> String {
        match self.call(who, model, c, input) {
            Err(e) => {
                let k = errors::classify(&e);
                assert_eq!(
                    (k.code, k.status),
                    ("immutable_record", 409),
                    "{model}.{c}: {:?}",
                    k.body
                );
                k.body["error"].as_str().unwrap_or_default().to_string()
            }
            Ok(r) => panic!("{model}.{c}: expected a refusal, got {:?}", r.value),
        }
    }
    fn create(&self, model: &str, pairs: Vec<(&str, Value)>) -> String {
        match get(&self.ok(&user(), model, "create", m(pairs)), "id") {
            Value::Text(id) => id.clone(),
            other => panic!("{other:?}"),
        }
    }
    /// A draft document named "Doc" with this amount, dated outside every period of the tests.
    fn doc(&self, amount: i64) -> String {
        self.create(
            "doc",
            vec![
                ("name", t("Doc")),
                ("amount", n(amount)),
                ("date", t("2026-04-10")),
            ],
        )
    }
    /// What is stored, read straight from the store.
    fn field(&self, model: &str, id: &str, f: &str) -> Value {
        self.store
            .field(&format!("{}_{model}", self.p), id, f)
            .unwrap_or(Value::Null)
    }
    /// The document is the draft `doc` made: no move, no number, no edit.
    fn untouched(&self, id: &str, amount: i64) {
        assert_eq!(self.field("doc", id, "state"), t("draft"));
        assert_eq!(self.field("doc", id, "number"), Value::Null);
        assert_eq!(self.field("doc", id, "name"), t("Doc"));
        assert_eq!(self.field("doc", id, "amount"), n(amount));
        assert_eq!(self.field("doc", id, "date"), t("2026-04-10"));
        assert_eq!(self.field("doc", id, "code"), Value::Null);
        assert_eq!(self.field("doc", id, "note"), Value::Null);
    }
    fn update(&self, id: &str, mut pairs: Vec<(&'static str, Value)>) -> Value {
        pairs.push(("id", t(id)));
        m(pairs)
    }
}

fn plan_of(r: Result<Response, CallError>) -> String {
    match r {
        Err(CallError::ApprovalRequired { plan_id }) => plan_id,
        other => panic!("expected an approval plan, got {other:?}"),
    }
}

// ---- the move and the edit are one unit ------------------------------------------------------

#[test]
fn a_move_into_a_frozen_state_with_an_edit_stores_both() {
    each(|r| {
        let id = r.doc(5);
        let v = r.ok(
            &approver(),
            "doc",
            "update",
            r.update(
                &id,
                vec![
                    ("state", t("posted")),
                    ("name", t("Renamed")),
                    ("code", t("A-1")),
                ],
            ),
        );
        // The answer is the record after all of it.
        assert_eq!(get(&v, "state"), &t("posted"));
        assert_eq!(get(&v, "name"), &t("Renamed"));
        assert_eq!(get(&v, "number"), &t("D-0001"));
        // And so is what is stored.
        assert_eq!(r.field("doc", &id, "state"), t("posted"));
        assert_eq!(r.field("doc", &id, "name"), t("Renamed"));
        assert_eq!(r.field("doc", &id, "code"), t("A-1"));
        assert_eq!(r.field("doc", &id, "number"), t("D-0001"));
        // From now on it is frozen.
        let why = r.frozen(
            &approver(),
            "doc",
            "update",
            r.update(&id, vec![("name", t("Again"))]),
        );
        assert!(why.contains("is posted"), "{why}");
    });
}

#[test]
fn an_edit_the_record_refuses_stores_nothing_and_takes_no_number() {
    each(|r| {
        r.create("doc", vec![("name", t("Other")), ("code", t("TAKEN"))]);
        let id = r.doc(5);
        // A value another record has: the update's own check, made in its unit.
        let e = r.err(
            &approver(),
            "doc",
            "update",
            r.update(&id, vec![("state", t("posted")), ("code", t("TAKEN"))]),
        );
        assert!(e.contains("code must be unique"), "{e}");
        r.untouched(&id, 5);
        // A number the system assigns.
        let e = r.err(
            &approver(),
            "doc",
            "update",
            r.update(&id, vec![("state", t("posted")), ("number", t("D-9999"))]),
        );
        assert!(e.contains("is a number the system assigns"), "{e}");
        r.untouched(&id, 5);
        // No number of the gapless series was taken by the refused calls.
        let v = r.ok(
            &approver(),
            "doc",
            "update",
            r.update(&id, vec![("state", t("posted"))]),
        );
        assert_eq!(get(&v, "number"), &t("D-0001"));
    });
}

#[test]
fn the_action_runs_on_the_record_as_edited_and_its_refusal_stores_nothing() {
    each(|r| {
        // The edit makes the assertion of the action fail: nothing is stored, the edit included.
        let id = r.doc(5);
        let e = r.err(
            &approver(),
            "doc",
            "update",
            r.update(
                &id,
                vec![("state", t("posted")), ("amount", n(0)), ("name", t("N"))],
            ),
        );
        assert!(e.contains("A document needs an amount"), "{e}");
        r.untouched(&id, 5);
        // The assertion fails on the record as it is: an edit of another field is not stored.
        let empty = r.doc(0);
        let e = r.err(
            &approver(),
            "doc",
            "update",
            r.update(&empty, vec![("state", t("posted")), ("name", t("N"))]),
        );
        assert!(e.contains("A document needs an amount"), "{e}");
        r.untouched(&empty, 0);
        // The edit makes the assertion hold: both are stored.
        r.ok(
            &approver(),
            "doc",
            "update",
            r.update(&empty, vec![("state", t("posted")), ("amount", n(7))]),
        );
        assert_eq!(r.field("doc", &empty, "state"), t("posted"));
        assert_eq!(r.field("doc", &empty, "amount"), n(7));
        assert_eq!(r.field("doc", &empty, "number"), t("D-0001"));
    });
}

#[test]
fn an_agents_call_changes_nothing_until_a_person_approves_and_then_does_all_of_it() {
    each(|r| {
        let id = r.doc(5);
        let plan = plan_of(r.call(
            &bot(),
            "doc",
            "update",
            r.update(
                &id,
                vec![("state", t("posted")), ("name", t("By the agent"))],
            ),
        ));
        r.untouched(&id, 5);
        let v =
            r.d.approve(&approver(), &mut Vault::new(), &plan)
                .unwrap_or_else(|e| panic!("the approval: {e:?}"))
                .value;
        assert_eq!(get(&v, "state"), &t("posted"));
        assert_eq!(get(&v, "name"), &t("By the agent"));
        assert_eq!(r.field("doc", &id, "state"), t("posted"));
        assert_eq!(r.field("doc", &id, "name"), t("By the agent"));
        assert_eq!(r.field("doc", &id, "number"), t("D-0001"));

        // A plan whose edit is refused when it is approved (another record took the code since)
        // changes nothing either.
        let other = r.create("doc", vec![("name", t("Other"))]);
        let id = r.doc(5);
        let plan = plan_of(r.call(
            &bot(),
            "doc",
            "update",
            r.update(&id, vec![("state", t("posted")), ("code", t("LATE"))]),
        ));
        r.ok(
            &user(),
            "doc",
            "update",
            r.update(&other, vec![("code", t("LATE"))]),
        );
        match r.d.approve(&approver(), &mut Vault::new(), &plan) {
            Err(CallError::Handler(e)) => assert!(e.contains("code must be unique"), "{e}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
        r.untouched(&id, 5);
    });
}

#[test]
fn an_update_that_only_moves_the_state_works_as_before() {
    each(|r| {
        let id = r.doc(5);
        let v = r.ok(
            &approver(),
            "doc",
            "update",
            r.update(&id, vec![("state", t("posted"))]),
        );
        assert_eq!(get(&v, "state"), &t("posted"));
        assert_eq!(get(&v, "number"), &t("D-0001"));
        assert_eq!(r.field("doc", &id, "name"), t("Doc"));
        // A form that sends the whole record back with the new state: values that are as stored
        // are not edits.
        let id = r.doc(5);
        let Value::Map(mut whole) = r.ok(&user(), "doc", "get", m(vec![("id", t(&id))])) else {
            panic!("a record")
        };
        whole.insert("state".into(), t("posted"));
        let v = r.ok(&approver(), "doc", "update", Value::Map(whole));
        assert_eq!(get(&v, "number"), &t("D-0002"));
        // Without the action's group, and with an assertion that fails, nothing moves.
        let id = r.doc(5);
        let e = r.err(
            &user(),
            "doc",
            "update",
            r.update(&id, vec![("state", t("posted"))]),
        );
        assert!(e.contains(APPROVER), "{e}");
        r.untouched(&id, 5);
        let empty = r.doc(0);
        let e = r.err(
            &approver(),
            "doc",
            "update",
            r.update(&empty, vec![("state", t("posted"))]),
        );
        assert!(e.contains("A document needs an amount"), "{e}");
        r.untouched(&empty, 0);
        // An agent proposes, a person approves.
        let plan = plan_of(r.call(
            &bot(),
            "doc",
            "update",
            r.update(&id, vec![("state", t("posted"))]),
        ));
        r.untouched(&id, 5);
        r.d.approve(&approver(), &mut Vault::new(), &plan).unwrap();
        assert_eq!(r.field("doc", &id, "number"), t("D-0003"));
        // An update without a move is the plain update it always was.
        let id = r.doc(5);
        let v = r.ok(
            &user(),
            "doc",
            "update",
            r.update(&id, vec![("name", t("Plain"))]),
        );
        assert_eq!(get(&v, "name"), &t("Plain"));
        assert_eq!(get(&v, "state"), &t("draft"));
    });
}

// ---- the other ways a guard freezes ----------------------------------------------------------

#[test]
fn a_date_in_a_closed_period_refuses_the_move_that_came_with_it() {
    each(|r| {
        let period = r.create(
            "period",
            vec![("start_on", t("2026-03-01")), ("end_on", t("2026-03-31"))],
        );
        r.ok(&user(), "period", "close", m(vec![("id", t(&period))]));
        let id = r.doc(5);
        let why = r.frozen(
            &approver(),
            "doc",
            "update",
            r.update(&id, vec![("state", t("posted")), ("date", t("2026-03-15"))]),
        );
        assert!(why.contains(".date falls in a locked period"), "{why}");
        r.untouched(&id, 5);
        // A date outside the period goes in with the move.
        r.ok(
            &approver(),
            "doc",
            "update",
            r.update(&id, vec![("state", t("posted")), ("date", t("2026-04-11"))]),
        );
        assert_eq!(r.field("doc", &id, "state"), t("posted"));
        assert_eq!(r.field("doc", &id, "date"), t("2026-04-11"));
        assert_eq!(r.field("doc", &id, "number"), t("D-0001"));
        // A record dated inside the closed period is frozen: neither the move nor the edit.
        let early = r.store.insert(
            &format!("{}_doc", r.p),
            [
                ("name".to_string(), t("March")),
                ("amount".to_string(), n(5)),
                ("date".to_string(), t("2026-03-02")),
                ("state".to_string(), t("draft")),
            ]
            .into_iter()
            .collect(),
        );
        let early = early.unwrap();
        let why = r.frozen(
            &approver(),
            "doc",
            "update",
            r.update(&early, vec![("state", t("posted")), ("name", t("N"))]),
        );
        assert!(why.contains("locked period"), "{why}");
        assert_eq!(r.field("doc", &early, "state"), t("draft"));
        assert_eq!(r.field("doc", &early, "name"), t("March"));
        assert_eq!(r.field("doc", &early, "number"), Value::Null);
    });
}

#[test]
fn a_child_is_not_moved_onto_a_frozen_parent_while_its_own_state_moves() {
    each(|r| {
        let (open, shut) = (r.doc(5), r.doc(5));
        r.ok(
            &approver(),
            "doc",
            "update",
            r.update(&shut, vec![("state", t("posted"))]),
        );
        let item = r.create("item", vec![("doc_id", t(&open)), ("what", t("A"))]);
        let why = r.frozen(
            &user(),
            "item",
            "update",
            r.update(&item, vec![("state", t("done")), ("doc_id", t(&shut))]),
        );
        assert!(why.contains("can no longer be changed"), "{why}");
        assert_eq!(r.field("item", &item, "state"), t("open"));
        assert_eq!(r.field("item", &item, "doc_id"), t(&open));
        // Onto a parent that is not frozen, with an edit: all of it.
        let other = r.doc(5);
        r.ok(
            &user(),
            "item",
            "update",
            r.update(
                &item,
                vec![
                    ("state", t("done")),
                    ("doc_id", t(&other)),
                    ("what", t("B")),
                ],
            ),
        );
        assert_eq!(r.field("item", &item, "state"), t("done"));
        assert_eq!(r.field("item", &item, "doc_id"), t(&other));
        assert_eq!(r.field("item", &item, "what"), t("B"));
        // The child of a frozen parent neither moves nor changes.
        let kept = r.create("item", vec![("doc_id", t(&other)), ("what", t("C"))]);
        r.ok(
            &approver(),
            "doc",
            "update",
            r.update(&other, vec![("state", t("posted"))]),
        );
        r.frozen(
            &user(),
            "item",
            "update",
            r.update(&kept, vec![("state", t("done")), ("what", t("D"))]),
        );
        assert_eq!(r.field("item", &kept, "state"), t("open"));
        assert_eq!(r.field("item", &kept, "what"), t("C"));
    });
}

#[test]
fn leaving_a_frozen_state_takes_what_the_exit_allows_and_nothing_else() {
    each(|r| {
        let id = r.doc(5);
        r.ok(
            &approver(),
            "doc",
            "update",
            r.update(&id, vec![("state", t("posted"))]),
        );
        // The record is frozen when the call begins, so its name is fixed: the call is refused as
        // a whole, the move and the reason included.
        let why = r.frozen(
            &user(),
            "doc",
            "update",
            r.update(
                &id,
                vec![
                    ("state", t("void")),
                    ("reason", t("Wrong customer")),
                    ("name", t("X")),
                ],
            ),
        );
        assert!(why.contains("name is fixed"), "{why}");
        assert_eq!(r.field("doc", &id, "state"), t("posted"));
        assert_eq!(r.field("doc", &id, "reason"), Value::Null);
        assert_eq!(r.field("doc", &id, "name"), t("Doc"));
        // What the action needs (the exit lets it change that) and a mutable field go with it.
        r.ok(
            &user(),
            "doc",
            "update",
            r.update(
                &id,
                vec![
                    ("state", t("void")),
                    ("reason", t("Wrong customer")),
                    ("note", t("see the mail")),
                ],
            ),
        );
        assert_eq!(r.field("doc", &id, "state"), t("void"));
        assert_eq!(r.field("doc", &id, "reason"), t("Wrong customer"));
        assert_eq!(r.field("doc", &id, "note"), t("see the mail"));
    });
}

// ---- wrappers, several moves, handlers written by hand ---------------------------------------

#[test]
fn the_wrappers_of_the_update_and_of_the_action_both_run_and_in_that_order() {
    each(|r| {
        // A module's rule on `update` (a wrapper) refuses before the action has run.
        r.d.around(
            &r.cmd("doc", "update"),
            Box::new(|_, input, inner| match get(input, "name") {
                Value::Text(s) if s == "forbidden" => Err("that name is not allowed".into()),
                _ => inner(input),
            }),
        );
        // The wrapper of the action sees the action run; when the handler it wraps has answered,
        // the whole update is stored (it may then do what cannot be taken back).
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let (log, store, model) = (seen.clone(), r.store.clone(), format!("{}_doc", r.p));
        r.d.around(
            &r.cmd("doc", "cancel"),
            Box::new(move |_, input, inner| {
                let Value::Text(id) = get(input, "id").clone() else {
                    return Err("missing id".into());
                };
                let stored = |f: &str| match store.field(&model, &id, f) {
                    Some(Value::Text(s)) => s,
                    _ => String::new(),
                };
                log.lock()
                    .unwrap()
                    .push(format!("before:{}:{}", stored("state"), stored("name")));
                let out = inner(input)?;
                log.lock()
                    .unwrap()
                    .push(format!("after:{}:{}", stored("state"), stored("name")));
                Ok(out)
            }),
        );
        let id = r.doc(5);
        let e = r.err(
            &user(),
            "doc",
            "update",
            r.update(
                &id,
                vec![("state", t("cancelled")), ("name", t("forbidden"))],
            ),
        );
        assert_eq!(e, "that name is not allowed");
        r.untouched(&id, 5);
        assert!(seen.lock().unwrap().is_empty(), "the action did not run");
        r.ok(
            &user(),
            "doc",
            "update",
            r.update(&id, vec![("state", t("cancelled")), ("name", t("Fine"))]),
        );
        assert_eq!(
            *seen.lock().unwrap(),
            ["before:draft:Doc", "after:cancelled:Fine"]
        );
    });
}

#[test]
fn a_wrapper_that_answers_for_a_refused_action_leaves_the_edits_to_the_update() {
    each(|r| {
        // The wrapper decides what a refusal of its action means. Here it answers without one:
        // the update then stores its edits, as it did before, and the state has not moved.
        r.d.around(
            &r.cmd("doc", "post"),
            Box::new(|_, input, inner| inner(input).or(Ok(Value::Null))),
        );
        let id = r.doc(0);
        let v = r.ok(
            &approver(),
            "doc",
            "update",
            r.update(&id, vec![("state", t("posted")), ("name", t("Kept"))]),
        );
        assert_eq!(get(&v, "state"), &t("draft"));
        assert_eq!(r.field("doc", &id, "state"), t("draft"));
        assert_eq!(r.field("doc", &id, "name"), t("Kept"));
        assert_eq!(r.field("doc", &id, "number"), Value::Null);
    });
}

#[test]
fn two_moves_and_an_edit_in_one_update_are_one_unit() {
    each(|r| {
        let task = |size: i64| r.create("task", vec![("title", t("T")), ("size", n(size))]);
        let both = |id: &str| {
            r.update(
                id,
                vec![
                    ("priority", t("urgent")),
                    ("state", t("closed")),
                    ("title", t("Edited")),
                ],
            )
        };
        let as_made = |id: &str| {
            assert_eq!(r.field("task", id, "priority"), t("normal"));
            assert_eq!(r.field("task", id, "state"), t("open"));
            assert_eq!(r.field("task", id, "title"), t("T"));
        };
        // The second action refuses: the first move and the edit are not stored.
        let id = task(20);
        let e = r.err(&user(), "task", "update", both(&id));
        assert!(e.contains("Too large to close"), "{e}");
        as_made(&id);
        // The first refuses.
        let id = task(0);
        let e = r.err(&user(), "task", "update", both(&id));
        assert!(e.contains("Nothing to escalate"), "{e}");
        as_made(&id);
        // Both hold: all three changes.
        let id = task(5);
        let v = r.ok(&user(), "task", "update", both(&id));
        assert_eq!(get(&v, "priority"), &t("urgent"));
        assert_eq!(get(&v, "state"), &t("closed"));
        assert_eq!(get(&v, "title"), &t("Edited"));
        assert_eq!(r.field("task", &id, "priority"), t("urgent"));
        assert_eq!(r.field("task", &id, "state"), t("closed"));
        assert_eq!(r.field("task", &id, "title"), t("Edited"));
    });
}

#[test]
fn an_action_with_a_hand_written_handler_runs_after_the_checks_of_the_edits() {
    each(|r| {
        r.create("ticket", vec![("title", t("Other")), ("code", t("TAKEN"))]);
        // An edit the update refuses: the action has not run.
        let id = r.create("ticket", vec![("title", t("T"))]);
        let e = r.err(
            &user(),
            "ticket",
            "update",
            r.update(&id, vec![("state", t("closed")), ("code", t("TAKEN"))]),
        );
        assert!(e.contains("code must be unique"), "{e}");
        assert_eq!(r.field("ticket", &id, "state"), t("open"));
        assert_eq!(r.field("ticket", &id, "code"), Value::Null);
        // The action refuses: the edit is not stored.
        let stuck = r.create("ticket", vec![("title", t("stuck"))]);
        let e = r.err(
            &user(),
            "ticket",
            "update",
            r.update(&stuck, vec![("state", t("closed")), ("note", t("n"))]),
        );
        assert!(e.contains("the ticket is stuck"), "{e}");
        assert_eq!(r.field("ticket", &stuck, "state"), t("open"));
        assert_eq!(r.field("ticket", &stuck, "note"), Value::Null);
        // Both fine: both stored, and the answer is the record after both.
        let v = r.ok(
            &user(),
            "ticket",
            "update",
            r.update(&id, vec![("state", t("closed")), ("note", t("done"))]),
        );
        assert_eq!(get(&v, "state"), &t("closed"));
        assert_eq!(get(&v, "note"), &t("done"));
        assert_eq!(r.field("ticket", &id, "state"), t("closed"));
        assert_eq!(r.field("ticket", &id, "note"), t("done"));
    });
}

#[test]
fn an_update_written_by_hand_stores_its_edits_and_the_move_follows_it() {
    each(|r| {
        // Both fine: the handler stores the edit, and the action then moves the record. The
        // handler never sees the governed field.
        let id = r.create("memo", vec![("title", t("M")), ("size", n(1))]);
        r.ok(
            &user(),
            "memo",
            "update",
            r.update(&id, vec![("state", t("filed")), ("title", t("N"))]),
        );
        assert_eq!(r.field("memo", &id, "state"), t("filed"));
        assert_eq!(r.field("memo", &id, "title"), t("N"));
        // The handler refuses: the action has not run.
        let kept = r.create("memo", vec![("title", t("K")), ("size", n(1))]);
        let e = r.err(
            &user(),
            "memo",
            "update",
            r.update(&kept, vec![("state", t("filed")), ("title", t("no"))]),
        );
        assert!(e.contains("the memo says no"), "{e}");
        assert_eq!(r.field("memo", &kept, "state"), t("open"));
        assert_eq!(r.field("memo", &kept, "title"), t("K"));
        // The action refuses: the handler stored its edit before (it is its own unit of work),
        // and the record has not moved. Never the other way round: a moved record whose edits
        // were refused.
        let empty = r.create("memo", vec![("title", t("E")), ("size", n(0))]);
        let e = r.err(
            &user(),
            "memo",
            "update",
            r.update(&empty, vec![("state", t("filed")), ("title", t("F"))]),
        );
        assert!(e.contains("Nothing to file"), "{e}");
        assert_eq!(r.field("memo", &empty, "state"), t("open"));
        assert_eq!(r.field("memo", &empty, "title"), t("F"));
    });
}

#[test]
fn an_action_that_stores_before_it_answers_is_not_followed_by_another() {
    each(|r| {
        let task = |size: i64| r.create("task", vec![("title", t("T")), ("size", n(size))]);
        let both = |id: &str| {
            r.update(
                id,
                vec![
                    ("priority", t("urgent")),
                    ("state", t("closed")),
                    ("title", t("Edited")),
                ],
            )
        };
        let as_made = |id: &str| {
            assert_eq!(r.field("task", id, "priority"), t("normal"));
            assert_eq!(r.field("task", id, "state"), t("open"));
            assert_eq!(r.field("task", id, "title"), t("T"));
        };
        // A wrapper on the last action: the first hands the unit on and the last commits for
        // both, so the call is still one unit.
        r.d.around(
            &r.cmd("task", "close"),
            Box::new(|_, input, inner| inner(input)),
        );
        let id = task(20);
        let e = r.err(&user(), "task", "update", both(&id));
        assert!(e.contains("Too large to close"), "{e}");
        as_made(&id);
        let id = task(5);
        r.ok(&user(), "task", "update", both(&id));
        assert_eq!(r.field("task", &id, "priority"), t("urgent"));
        assert_eq!(r.field("task", &id, "state"), t("closed"));
        assert_eq!(r.field("task", &id, "title"), t("Edited"));

        // A wrapper on the first action: its handler would store the first move and the edit
        // before the second action has run, and a refusal of the second could not take them
        // back. The call is refused before anything runs, also when both actions would pass.
        r.d.around(
            &r.cmd("task", "escalate"),
            Box::new(|_, input, inner| inner(input)),
        );
        for size in [20, 5] {
            let id = task(size);
            let e = r.err(&user(), "task", "update", both(&id));
            assert!(
                e.contains("task.escalate") && e.contains("separate calls"),
                "{e}"
            );
            as_made(&id);
        }
        // Sent one at a time, each move with an edit is one unit as before.
        let id = task(5);
        r.ok(
            &user(),
            "task",
            "update",
            r.update(&id, vec![("priority", t("urgent")), ("title", t("One"))]),
        );
        r.ok(
            &user(),
            "task",
            "update",
            r.update(&id, vec![("state", t("closed")), ("title", t("Two"))]),
        );
        assert_eq!(r.field("task", &id, "priority"), t("urgent"));
        assert_eq!(r.field("task", &id, "state"), t("closed"));
        assert_eq!(r.field("task", &id, "title"), t("Two"));

        // The same for an action whose handler was written by hand: it writes the store itself.
        let id = r.create("ticket", vec![("title", t("T"))]);
        let e = r.err(
            &user(),
            "ticket",
            "update",
            r.update(
                &id,
                vec![
                    ("state", t("closed")),
                    ("urgency", t("high")),
                    ("note", t("n")),
                ],
            ),
        );
        assert!(
            e.contains("ticket.close") && e.contains("separate calls"),
            "{e}"
        );
        assert_eq!(r.field("ticket", &id, "state"), t("open"));
        assert_eq!(r.field("ticket", &id, "urgency"), t("low"));
        assert_eq!(r.field("ticket", &id, "note"), Value::Null);
        // The generic action of the same model by itself, with an edit.
        r.ok(
            &user(),
            "ticket",
            "update",
            r.update(&id, vec![("urgency", t("high")), ("note", t("n"))]),
        );
        assert_eq!(r.field("ticket", &id, "urgency"), t("high"));
        assert_eq!(r.field("ticket", &id, "note"), t("n"));
    });
}

#[test]
fn a_commit_that_fails_ends_the_update_whatever_the_wrapper_of_the_action_makes_of_it() {
    // The commit of the edit and the move is one batch. When the store refuses it, the edit is
    // gone with the unit: the update must not answer as if it had worked (a wrapper that answers
    // for the refusal of its action), and the move must not be stored alone (a wrapper that runs
    // its handler a second time).
    for again in [false, true] {
        let (flaky, backend) = obop_storage::testing::Flaky::memory();
        let r = rig(Store::over(backend));
        r.d.around(
            &r.cmd("doc", "post"),
            Box::new(move |_, input, inner| {
                let first = inner(input);
                if again {
                    first.or_else(|_| inner(input))
                } else {
                    first.or(Ok(Value::Null))
                }
            }),
        );
        let id = r.doc(5);
        let call = || {
            r.call(
                &approver(),
                "doc",
                "update",
                r.update(&id, vec![("state", t("posted")), ("name", t("Kept"))]),
            )
        };
        flaky.refuse_applies(1);
        let out = call();
        assert!(out.is_err(), "again {again}: {out:?}");
        assert_eq!(flaky.refused(), 1, "again {again}: one batch, refused once");
        r.untouched(&id, 5);
        // Nothing of the call was stored, so it can be sent again, and no number was taken.
        let v = match call() {
            Ok(r) => r.value,
            Err(e) => panic!("again {again}: {e:?}"),
        };
        assert_eq!(get(&v, "state"), &t("posted"));
        assert_eq!(r.field("doc", &id, "state"), t("posted"));
        assert_eq!(r.field("doc", &id, "name"), t("Kept"));
        assert_eq!(r.field("doc", &id, "number"), t("D-0001"));
    }
}
