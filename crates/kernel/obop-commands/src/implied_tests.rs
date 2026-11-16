//! One `update` that moves a governed field and changes other fields is one unit of work (D48:
//! "a mixed update is applied completely or not at all"), also when the state it moves to is
//! frozen by the D77 guard. The other fields are edits of the record as it was, so they are
//! written first; the action the move implies runs after them, on the record as edited; and a
//! refusal of any part (a field, the guard, a wrapper of the update, the action's assertion)
//! leaves nothing stored, no number taken. An agent's call that needs approval changes nothing,
//! and the approved plan does all of it.
//!
//! Only an action that is generic all the way shares the unit. One that a module wraps or binds
//! to a handler of its own is run by itself: an update that would mix it with an edit or
//! another move is refused before anything is written, and an agent gets no plan for it. An
//! `update` that a module wrote by hand takes no move at all: the action is called by its name.
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
            // leaves only by `void`, which takes its reason. The exit also names `code`, which
            // the action does not need.
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
                    // The way back out of a cancelled record needs approval.
                    CommandDef::new("reopen", "Reopen", APPROVER)
                        .approval()
                        .transition("state", Some("cancelled"), "draft"),
                ],
            )
            .with_guard(
                Guard::frozen_in("state", &["posted", "cancelled"])
                    .mutable(&["note"])
                    .exit("void", "void", &["reason", "code"])
                    .exit("draft", "reopen", &[])
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
                        .approval()
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
        // A field the exit names but the action does not need is an edit like any other: it
        // is judged against the record before the move, where it is fixed.
        let why = r.frozen(
            &user(),
            "doc",
            "update",
            r.update(
                &id,
                vec![
                    ("state", t("void")),
                    ("reason", t("Wrong customer")),
                    ("code", t("C-1")),
                ],
            ),
        );
        assert!(why.contains("code is fixed"), "{why}");
        assert_eq!(r.field("doc", &id, "state"), t("posted"));
        assert_eq!(r.field("doc", &id, "code"), Value::Null);
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

        // An exit that needs approval, asked for by an agent together with an edit of a fixed
        // field: the frozen record is judged when the update runs, so the agent gets a plan,
        // and the refusal comes when a person approves it. Nothing is stored.
        let id = r.doc(5);
        r.ok(
            &user(),
            "doc",
            "update",
            r.update(&id, vec![("state", t("cancelled"))]),
        );
        let plan = plan_of(r.call(
            &bot(),
            "doc",
            "update",
            r.update(&id, vec![("state", t("draft")), ("name", t("X"))]),
        ));
        match r.d.approve(&approver(), &mut Vault::new(), &plan) {
            Err(e) => {
                let k = errors::classify(&e);
                assert_eq!(k.code, "immutable_record", "{:?}", k.body);
            }
            Ok(v) => panic!("expected a refusal, got {:?}", v.value),
        }
        assert_eq!(r.field("doc", &id, "state"), t("cancelled"));
        assert_eq!(r.field("doc", &id, "name"), t("Doc"));
        // The move by itself goes through when it is approved.
        let plan = plan_of(r.call(
            &bot(),
            "doc",
            "update",
            r.update(&id, vec![("state", t("draft"))]),
        ));
        r.d.approve(&approver(), &mut Vault::new(), &plan)
            .unwrap_or_else(|e| panic!("the approval: {e:?}"));
        assert_eq!(r.field("doc", &id, "state"), t("draft"));
    });
}

// ---- several moves, wrappers, handlers written by hand ---------------------------------------

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
fn the_wrapper_of_the_update_runs_before_anything_and_the_unit_inside_it() {
    each(|r| {
        // A module's rule on `update` (a wrapper) refuses before the action has run.
        r.d.around(
            &r.cmd("doc", "update"),
            Box::new(|_, input, inner| match get(input, "name") {
                Value::Text(s) if s == "forbidden" => Err("that name is not allowed".into()),
                _ => inner(input),
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
        // It lets the call through: the edit and the move are one unit inside it.
        r.ok(
            &user(),
            "doc",
            "update",
            r.update(&id, vec![("state", t("cancelled")), ("name", t("Fine"))]),
        );
        assert_eq!(r.field("doc", &id, "state"), t("cancelled"));
        assert_eq!(r.field("doc", &id, "name"), t("Fine"));
    });
}

/// The refusal of an update that would mix `command` (run for a change of `field`) with
/// something else.
fn not_mixed(e: &str, field: &str, command: &str) {
    assert!(
        e.contains(&format!(
            "`{field}` cannot be changed together with other fields"
        )) && e.contains(command)
            && e.contains("separate call"),
        "{e}"
    );
}

#[test]
fn an_action_with_a_wrapper_is_run_by_itself_and_never_mixed() {
    each(|r| {
        // The wrapper of `cancel` reads the store before and after the handler it wraps, as the
        // wrappers of the built-in modules do (a rule of the module, money returned).
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
        // With an edit: refused before the wrapper has run and before anything is written. The
        // wrapper would have judged the record without the edit.
        let id = r.doc(5);
        let e = r.err(
            &user(),
            "doc",
            "update",
            r.update(&id, vec![("state", t("cancelled")), ("name", t("Fine"))]),
        );
        not_mixed(&e, "state", "doc.cancel");
        r.untouched(&id, 5);
        assert!(seen.lock().unwrap().is_empty(), "the action did not run");
        // A form that saves the whole record and changes nothing but the state: the move alone.
        // It sends a null for a field the record never had, which is no change either.
        r.ok(
            &user(),
            "doc",
            "update",
            r.update(
                &id,
                vec![
                    ("state", t("cancelled")),
                    ("name", t("Doc")),
                    ("amount", n(5)),
                    ("note", Value::Null),
                    ("reason", Value::Null),
                ],
            ),
        );
        assert_eq!(
            *seen.lock().unwrap(),
            ["before:draft:Doc", "after:cancelled:Doc"]
        );
        assert_eq!(r.field("doc", &id, "state"), t("cancelled"));
        // The edit goes in a call of its own, before the move.
        let id = r.doc(5);
        r.ok(
            &user(),
            "doc",
            "update",
            r.update(&id, vec![("name", t("Fine"))]),
        );
        r.ok(
            &user(),
            "doc",
            "update",
            r.update(&id, vec![("state", t("cancelled"))]),
        );
        assert_eq!(r.field("doc", &id, "state"), t("cancelled"));
        assert_eq!(r.field("doc", &id, "name"), t("Fine"));

        // The wrapper decides what a refusal of its action means, as when the action is called
        // by itself: here it answers without one, and the state has not moved.
        r.d.around(
            &r.cmd("doc", "post"),
            Box::new(|_, input, inner| inner(input).or(Ok(Value::Null))),
        );
        let id = r.doc(0);
        let v = r.ok(
            &approver(),
            "doc",
            "update",
            r.update(&id, vec![("state", t("posted"))]),
        );
        assert_eq!(get(&v, "state"), &t("draft"));
        r.untouched(&id, 0);

        // Two moves, one of them with a wrapper: refused, also when it is the last of the two
        // and both would pass. One at a time they go through.
        r.d.around(
            &r.cmd("task", "close"),
            Box::new(|_, input, inner| inner(input)),
        );
        let id = r.create("task", vec![("title", t("T")), ("size", n(5))]);
        let e = r.err(
            &user(),
            "task",
            "update",
            r.update(&id, vec![("priority", t("urgent")), ("state", t("closed"))]),
        );
        not_mixed(&e, "state", "task.close");
        assert_eq!(r.field("task", &id, "priority"), t("normal"));
        assert_eq!(r.field("task", &id, "state"), t("open"));
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
            r.update(&id, vec![("state", t("closed"))]),
        );
        assert_eq!(r.field("task", &id, "priority"), t("urgent"));
        assert_eq!(r.field("task", &id, "state"), t("closed"));
        assert_eq!(r.field("task", &id, "title"), t("One"));
    });
}

#[test]
fn an_action_with_a_handler_of_its_own_is_run_by_itself_and_never_mixed() {
    each(|r| {
        // `ticket.close` writes the store itself. With an edit: refused, nothing stored, and the
        // handler has not run (it would have refused a stuck ticket in its own words).
        let stuck = r.create("ticket", vec![("title", t("stuck"))]);
        let e = r.err(
            &user(),
            "ticket",
            "update",
            r.update(&stuck, vec![("state", t("closed")), ("note", t("n"))]),
        );
        not_mixed(&e, "state", "ticket.close");
        assert_eq!(r.field("ticket", &stuck, "state"), t("open"));
        assert_eq!(r.field("ticket", &stuck, "note"), Value::Null);
        // The move alone: the handler runs, and its refusal is the answer.
        let e = r.err(
            &user(),
            "ticket",
            "update",
            r.update(&stuck, vec![("state", t("closed"))]),
        );
        assert!(e.contains("the ticket is stuck"), "{e}");
        assert_eq!(r.field("ticket", &stuck, "state"), t("open"));
        // The move alone and fine: stored, and the answer is the record as moved.
        let id = r.create("ticket", vec![("title", t("T"))]);
        let v = r.ok(
            &user(),
            "ticket",
            "update",
            r.update(&id, vec![("state", t("closed"))]),
        );
        assert_eq!(get(&v, "state"), &t("closed"));
        assert_eq!(r.field("ticket", &id, "state"), t("closed"));

        // With the generic action of the same model (`raise`, which comes after it): refused.
        let id = r.create("ticket", vec![("title", t("T"))]);
        let e = r.err(
            &user(),
            "ticket",
            "update",
            r.update(&id, vec![("state", t("closed")), ("urgency", t("high"))]),
        );
        not_mixed(&e, "state", "ticket.close");
        assert_eq!(r.field("ticket", &id, "state"), t("open"));
        assert_eq!(r.field("ticket", &id, "urgency"), t("low"));
        // The generic action by itself, with an edit, is one unit as on any model.
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
fn an_agent_gets_no_plan_for_an_update_that_can_only_be_refused() {
    each(|r| {
        // `post` needs approval, so an agent's update that posts is kept as a plan for a person.
        // With a wrapper around `post` the action is run by itself: a plan that mixes it with
        // an edit could only be refused when it is approved, and would be spent by that. The
        // agent is refused now, and no plan is made.
        r.d.around(
            &r.cmd("doc", "post"),
            Box::new(|_, input, inner| inner(input)),
        );
        let id = r.doc(5);
        // The answer goes by the fields the agent sent, not by their values: a value equal to
        // the stored one (the agent may be shown the field masked, so this would be a guess
        // that is right) is answered like any other, and so is a value the update would have
        // refused in its own words (a number the system assigns).
        for other in [
            ("name", t("X")),
            ("name", t("Doc")),
            ("note", Value::Null),
            ("number", t("D-9999")),
        ] {
            let e = r.err(
                &bot(),
                "doc",
                "update",
                r.update(&id, vec![("state", t("posted")), other]),
            );
            not_mixed(&e, "state", "doc.post");
        }
        r.untouched(&id, 5);
        // The move by itself: a plan, which does the move when a person approves it.
        let plan = plan_of(r.call(
            &bot(),
            "doc",
            "update",
            r.update(&id, vec![("state", t("posted"))]),
        ));
        r.untouched(&id, 5);
        r.d.approve(&approver(), &mut Vault::new(), &plan)
            .unwrap_or_else(|e| panic!("the approval: {e:?}"));
        assert_eq!(r.field("doc", &id, "state"), t("posted"));
        assert_eq!(r.field("doc", &id, "number"), t("D-0001"));
        // A record the agent cannot be asked about (there is none) is refused in those words,
        // and no plan is made either.
        let e = r.err(
            &bot(),
            "doc",
            "update",
            r.update("none", vec![("state", t("posted")), ("name", t("X"))]),
        );
        assert!(!e.contains("cannot be changed together"), "{e}");
    });
}

/// The refusal of a move through an `update` that a module wrote by hand.
fn by_hand(e: &str, field: &str, update: &str, command: &str) {
    assert!(
        e.contains(&format!("`{field}` cannot be changed by {update}"))
            && e.contains(&format!("Call {command}")),
        "{e}"
    );
}

#[test]
fn an_update_written_by_hand_takes_no_move() {
    each(|r| {
        // `memo.update` writes the store itself and knows nothing of the actions, so it and
        // the action could not be refused as a whole: whichever ran first would stay stored
        // when the other refused. A move through it is refused before the handler or the
        // action has run (the handler would have refused the title "no" in its own words).
        let (update, file) = (r.cmd("memo", "update"), r.cmd("memo", "file"));
        let id = r.create("memo", vec![("title", t("M")), ("size", n(1))]);
        let e = r.err(
            &user(),
            "memo",
            "update",
            r.update(&id, vec![("state", t("filed")), ("title", t("no"))]),
        );
        by_hand(&e, "state", &update, &file);
        // Also the move alone: the action has not run.
        let e = r.err(
            &user(),
            "memo",
            "update",
            r.update(&id, vec![("state", t("filed"))]),
        );
        by_hand(&e, "state", &update, &file);
        assert_eq!(r.field("memo", &id, "state"), t("open"));
        assert_eq!(r.field("memo", &id, "title"), t("M"));
        // The action is called by its name, and its refusal is its own.
        let empty = r.create("memo", vec![("title", t("E")), ("size", n(0))]);
        let e = r.err(&user(), "memo", "file", m(vec![("id", t(&empty))]));
        assert!(e.contains("Nothing to file"), "{e}");
        assert_eq!(r.field("memo", &empty, "state"), t("open"));
        r.ok(&user(), "memo", "file", m(vec![("id", t(&id))]));
        assert_eq!(r.field("memo", &id, "state"), t("filed"));
        // An edit alone goes to the handler as before.
        r.ok(
            &user(),
            "memo",
            "update",
            r.update(&id, vec![("title", t("N"))]),
        );
        assert_eq!(r.field("memo", &id, "title"), t("N"));
        let e = r.err(
            &user(),
            "memo",
            "update",
            r.update(&id, vec![("title", t("no"))]),
        );
        assert!(e.contains("the memo says no"), "{e}");

        // An agent, whose `file` needs approval, gets no plan for a move through this update:
        // the handler, which writes, is never asked. Nothing is stored.
        let other = r.create("memo", vec![("title", t("O")), ("size", n(1))]);
        for sent in [
            vec![("state", t("filed"))],
            vec![("state", t("filed")), ("title", t("P"))],
        ] {
            let e = r.err(&bot(), "memo", "update", r.update(&other, sent));
            by_hand(&e, "state", &update, &file);
        }
        assert_eq!(r.field("memo", &other, "state"), t("open"));
        assert_eq!(r.field("memo", &other, "title"), t("O"));
        // A wrapper of the update that refuses the caller comes after this refusal, so the
        // move has not run when the wrapper speaks.
        r.d.around(&update, Box::new(|_, _, _| Err("not you".into())));
        let e = r.err(
            &user(),
            "memo",
            "update",
            r.update(&other, vec![("state", t("filed"))]),
        );
        by_hand(&e, "state", &update, &file);
        assert_eq!(r.field("memo", &other, "state"), t("open"));
    });
}

#[test]
fn a_commit_that_the_store_refuses_stores_nothing_and_the_call_can_be_sent_again() {
    // The edit, the move and the number are one batch. When the store refuses it, nothing of
    // the call is stored and no number is taken.
    let (flaky, backend) = obop_storage::testing::Flaky::memory();
    let r = rig(Store::over(backend));
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
    assert!(out.is_err(), "{out:?}");
    assert_eq!(flaky.refused(), 1, "one batch, refused once");
    r.untouched(&id, 5);
    let v = match call() {
        Ok(r) => r.value,
        Err(e) => panic!("{e:?}"),
    };
    assert_eq!(get(&v, "state"), &t("posted"));
    assert_eq!(r.field("doc", &id, "state"), t("posted"));
    assert_eq!(r.field("doc", &id, "name"), t("Kept"));
    assert_eq!(r.field("doc", &id, "number"), t("D-0001"));
}
