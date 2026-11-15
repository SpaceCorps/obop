//! The D77 command-layer guard through the dispatcher, on the memory and SQLite backends (and
//! PostgreSQL when `OBOP_TEST_POSTGRES_URL` is set): a frozen record refuses update and delete
//! except its mutable fields and its exit, the children of a frozen parent are refused, a lock date
//! from closed periods freezes and refuses dates, an effect writer is held to it, an append-only
//! model never changes, and meta serves the guard.

use crate::{CallError, Dispatcher, Principal, Response, Store, Surface, Value, Vault, errors};
use obop_registry::{
    CommandDef, Effect, FieldDef as F, FieldKind, Guard, ModelDef, Module, intern,
};
use obop_types::DataClass;
use std::sync::Arc;

const U: &str = "grd.user";

fn m(pairs: Vec<(&str, Value)>) -> Value {
    Value::Map(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}
fn t(s: &str) -> Value {
    Value::text(s)
}
fn text(v: &Value, k: &str) -> String {
    match v {
        Value::Map(m) => match m.get(k) {
            Some(Value::Text(s)) => s.clone(),
            other => panic!("{k} is not text: {other:?}"),
        },
        other => panic!("not a map: {other:?}"),
    }
}

fn module(p: &str) -> Module {
    let n = |s: &str| intern(&format!("{p}_{s}"));
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
    let order = n("order");
    let period = n("period");
    Module {
        id: p.into(),
        name: "G".into(),
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
            model(
                order,
                vec![
                    f("name"),
                    f("note"),
                    f("reason"),
                    f("date").kind(FieldKind::Date),
                    f("state").select(&["draft", "confirmed", "cancelled"]),
                ],
                vec![
                    CommandDef::new("confirm", "Confirm", U).transition(
                        "state",
                        Some("draft"),
                        "confirmed",
                    ),
                    CommandDef::new("cancel", "Cancel", U)
                        .transition("state", None, "cancelled")
                        .needs(&["reason"]),
                    CommandDef::new("reopen", "Reopen", U).transition(
                        "state",
                        Some("cancelled"),
                        "draft",
                    ),
                ],
            )
            .with_guard(
                Guard::frozen_in("state", &["confirmed"])
                    .mutable(&["note"])
                    .exit("cancelled", "cancel", &["reason"])
                    .no_delete_in("state", &["cancelled"])
                    .lock_by_closed("date", period)
                    .message("A confirmed order cannot be changed."),
            ),
            model(
                n("order_line"),
                vec![f("order_id").reference(order), f("what")],
                vec![],
            )
            .with_guard(Guard::child_of("order_id")),
            // Not a child: its action writes the order through an effect.
            model(
                n("task"),
                vec![
                    f("order_id").reference(order),
                    f("state").select(&["new", "done"]),
                ],
                vec![
                    CommandDef::new("touch", "Touch", U)
                        .transition("state", Some("new"), "done")
                        .effect(Effect::update_ref(
                            order,
                            "order_id",
                            &[("name", "touched")],
                        )),
                ],
            ),
            model(n("log"), vec![f("line")], vec![]).with_guard(Guard::append_only()),
            // Fixed once created: only the level changes afterwards.
            model(n("stock"), vec![f("product"), f("level")], vec![])
                .with_guard(Guard::default().mutable(&["level"])),
        ],
        network_destinations: vec![],
        description: String::new(),
        translations: Default::default(),
        meta: Default::default(),
    }
}

struct Rig {
    d: Arc<Dispatcher>,
    p: String,
    who: Principal,
}

fn each(f: impl Fn(&Rig)) {
    let mut urls = vec!["memory:".to_string(), "sqlite::memory:".to_string()];
    if let Ok(u) = std::env::var("OBOP_TEST_POSTGRES_URL") {
        urls.push(u);
    }
    for url in urls {
        let p = format!("g{}", &uuid::Uuid::now_v7().simple().to_string()[22..]);
        let store = Store::open(&url).unwrap();
        let d = Arc::new(Dispatcher::new());
        let module = module(&p);
        let v = obop_registry::check_module(&module);
        assert!(v.is_empty(), "{v:?}");
        d.register_crud(&module, &store);
        for mo in &module.models {
            for c in &mo.commands {
                d.register_transition(&module, mo.name, c.name, &store);
            }
        }
        let who = Principal::user(&format!("u-{p}"), &[U]);
        f(&Rig { d, p, who });
    }
}

impl Rig {
    fn call(&self, model: &str, c: &str, input: Value) -> Result<Response, CallError> {
        let id = format!("{}_{model}.{c}", self.p);
        self.d
            .call(Surface::Rest, &self.who, &mut Vault::new(), &id, &input)
    }
    fn ok(&self, model: &str, c: &str, input: Value) -> Value {
        match self.call(model, c, input) {
            Ok(r) => r.value,
            Err(e) => panic!("{model}.{c} failed: {e:?}"),
        }
    }
    /// A refusal that must be the guard's: 409 `immutable_record`.
    fn frozen(&self, model: &str, c: &str, input: Value) -> String {
        match self.call(model, c, input) {
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
        text(&self.ok(model, "create", m(pairs)), "id")
    }
    fn act(&self, model: &str, c: &str, id: &str) -> Value {
        self.ok(model, c, m(vec![("id", t(id))]))
    }
    fn order(&self, state_after: &str) -> String {
        let id = self.create("order", vec![("name", t("O")), ("date", t("2026-05-04"))]);
        match state_after {
            "draft" => {}
            "confirmed" => {
                self.act("order", "confirm", &id);
            }
            other => panic!("{other}"),
        }
        id
    }
}

#[test]
fn a_frozen_record_refuses_update_and_delete_but_keeps_its_mutable_fields() {
    each(|r| {
        let id = r.order("confirmed");
        let why = r.frozen("order", "update", m(vec![("id", t(&id)), ("name", t("X"))]));
        assert!(why.contains("is confirmed"), "{why}");
        assert!(
            why.contains(&format!("{}_order.name is fixed", r.p)),
            "{why}"
        );
        // The record sent back as it is changes nothing and passes.
        r.ok("order", "update", m(vec![("id", t(&id)), ("name", t("O"))]));
        // A mutable field.
        let v = r.ok(
            "order",
            "update",
            m(vec![("id", t(&id)), ("note", t("late"))]),
        );
        assert_eq!(text(&v, "note"), "late");
        let why = r.frozen("order", "delete", m(vec![("id", t(&id))]));
        assert!(why.contains("cannot be deleted"), "{why}");
        // A draft order is not frozen.
        let draft = r.order("draft");
        r.ok(
            "order",
            "update",
            m(vec![("id", t(&draft)), ("name", t("Y"))]),
        );
        r.ok("order", "delete", m(vec![("id", t(&draft))]));
    });
}

#[test]
fn the_exit_moves_a_frozen_record_with_what_it_may_change() {
    each(|r| {
        let id = r.order("confirmed");
        // The reason comes with the action (D95 needs) and is in the exit's may_change.
        let v = r.ok(
            "order",
            "cancel",
            m(vec![("id", t(&id)), ("reason", t("Customer withdrew"))]),
        );
        assert_eq!(text(&v, "state"), "cancelled");
        let got = r.ok("order", "get", m(vec![("id", t(&id))]));
        assert_eq!(text(&got, "reason"), "Customer withdrew");
        // Cancelled is not frozen, but is in no_delete_when.
        let why = r.frozen("order", "delete", m(vec![("id", t(&id))]));
        assert!(why.contains("is cancelled and cannot be deleted"), "{why}");
        // Reopened, it is a draft again and can change.
        r.act("order", "reopen", &id);
        r.ok(
            "order",
            "update",
            m(vec![("id", t(&id)), ("name", t("Again"))]),
        );
    });
}

#[test]
fn the_children_of_a_frozen_parent_are_refused() {
    each(|r| {
        let open = r.order("draft");
        let line = r.create("order_line", vec![("order_id", t(&open)), ("what", t("A"))]);
        let shut = r.order("confirmed");
        let kept = r.create("order_line", vec![("order_id", t(&open)), ("what", t("B"))]);
        // Moving a line onto a frozen order.
        let why = r.frozen(
            "order_line",
            "update",
            m(vec![("id", t(&kept)), ("order_id", t(&shut))]),
        );
        assert!(why.contains("no longer be changed"), "{why}");
        r.act("order", "confirm", &open);
        // Now its lines are frozen: add, change, delete.
        let why = r.frozen(
            "order_line",
            "create",
            m(vec![("order_id", t(&open)), ("what", t("C"))]),
        );
        assert!(why.contains("so no"), "{why}");
        let why = r.frozen(
            "order_line",
            "update",
            m(vec![("id", t(&line)), ("what", t("Z"))]),
        );
        assert!(why.contains("belongs to"), "{why}");
        r.frozen("order_line", "delete", m(vec![("id", t(&line))]));
        // bulk_create is held to the same rule; as for every row refusal, the answer lists the
        // rows (400) and does not carry a row's code.
        let rows = Value::List(vec![m(vec![("order_id", t(&open)), ("what", t("D"))])]);
        let Err(e) = r.call("order_line", "bulk_create", m(vec![("rows", rows)])) else {
            panic!("bulk_create onto a frozen order went through");
        };
        let k = errors::classify(&e);
        assert!(
            k.body["error"]
                .as_str()
                .unwrap_or_default()
                .contains("can no longer be changed"),
            "{:?}",
            k.body
        );
    });
}

#[test]
fn a_closed_period_freezes_its_records_and_refuses_its_dates() {
    each(|r| {
        let early = r.create(
            "order",
            vec![("name", t("March")), ("date", t("2026-03-15"))],
        );
        let period = r.create(
            "period",
            vec![("start_on", t("2026-03-01")), ("end_on", t("2026-03-31"))],
        );
        // While the period is open nothing is locked.
        r.ok(
            "order",
            "update",
            m(vec![("id", t(&early)), ("name", t("M"))]),
        );
        r.act("period", "close", &period);
        let why = r.frozen(
            "order",
            "update",
            m(vec![("id", t(&early)), ("name", t("N"))]),
        );
        assert!(why.contains("locked period"), "{why}");
        // No new record and no changed date inside the closed period.
        let why = r.frozen(
            "order",
            "create",
            m(vec![("name", t("Back")), ("date", t("2026-03-31"))]),
        );
        assert!(why.contains(".date falls in a locked period"), "{why}");
        let april = r.create(
            "order",
            vec![("name", t("April")), ("date", t("2026-04-01"))],
        );
        r.frozen(
            "order",
            "update",
            m(vec![("id", t(&april)), ("date", t("2026-03-02"))]),
        );
        r.ok(
            "order",
            "update",
            m(vec![("id", t(&april)), ("date", t("2026-04-02"))]),
        );
    });
}

#[test]
fn an_effect_writer_is_held_to_the_guard() {
    each(|r| {
        let id = r.order("confirmed");
        let task = r.create("task", vec![("order_id", t(&id))]);
        let why = r.frozen("task", "touch", m(vec![("id", t(&task))]));
        assert!(why.contains("effect 1"), "{why}");
        // Nothing of the action stayed.
        let got = r.ok("task", "get", m(vec![("id", t(&task))]));
        assert_eq!(text(&got, "state"), "new");
        // On a draft order the same action goes through.
        let draft = r.order("draft");
        let task = r.create("task", vec![("order_id", t(&draft))]);
        r.act("task", "touch", &task);
        let got = r.ok("order", "get", m(vec![("id", t(&draft))]));
        assert_eq!(text(&got, "name"), "touched");
    });
}

#[test]
fn a_guard_of_mutable_fields_alone_fixes_the_rest_once_created() {
    each(|r| {
        let id = r.create("stock", vec![("product", t("drill")), ("level", t("3"))]);
        let v = r.ok(
            "stock",
            "update",
            m(vec![("id", t(&id)), ("level", t("5"))]),
        );
        assert_eq!(text(&v, "level"), "5");
        let why = r.frozen(
            "stock",
            "update",
            m(vec![("id", t(&id)), ("product", t("saw"))]),
        );
        assert!(why.contains("keeps what it was created with"), "{why}");
        assert!(why.contains("only level can still change"), "{why}");
        r.frozen("stock", "delete", m(vec![("id", t(&id))]));
    });
}

#[test]
fn an_append_only_record_never_changes() {
    each(|r| {
        let id = r.create("log", vec![("line", t("one"))]);
        r.frozen("log", "update", m(vec![("id", t(&id)), ("line", t("two"))]));
        r.frozen("log", "delete", m(vec![("id", t(&id))]));
    });
}

#[test]
fn meta_serves_the_guard() {
    each(|r| {
        let meta = r.d.meta(Surface::Rest, &r.who);
        let model = meta
            .iter()
            .flat_map(|x| x.models.iter())
            .find(|x| x.name == format!("{}_order", r.p))
            .unwrap();
        let g = model.guard.as_ref().unwrap();
        assert_eq!(g.immutable_when.as_ref().unwrap().values, vec!["confirmed"]);
        assert_eq!(g.message, Some("A confirmed order cannot be changed."));
        let line = meta
            .iter()
            .flat_map(|x| x.models.iter())
            .find(|x| x.name == format!("{}_order_line", r.p))
            .unwrap();
        assert_eq!(line.guard.as_ref().unwrap().children_of, Some("order_id"));
    });
}

#[test]
fn every_surface_gets_the_same_refusal() {
    each(|r| {
        let id = r.order("confirmed");
        let cmd = format!("{}_order.update", r.p);
        let input = m(vec![("id", t(&id)), ("name", t("X"))]);
        for surface in [Surface::Rest, Surface::Mcp, Surface::Cli] {
            let e =
                r.d.call(surface, &r.who, &mut Vault::new(), &cmd, &input)
                    .err()
                    .unwrap_or_else(|| panic!("{surface:?}: the update went through"));
            let k = errors::classify(&e);
            assert_eq!((k.code, k.status), ("immutable_record", 409), "{surface:?}");
        }
    });
}
