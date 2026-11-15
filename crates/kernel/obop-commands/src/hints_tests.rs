//! The D95 rules through the dispatcher, on the memory and SQLite backends (and PostgreSQL when
//! `OBOP_TEST_POSTGRES_URL` is set): defaults and `fill_from` on create (generated, bulk and an
//! effect's), `readonly`, `ref_filter`, an action's `needs` in one call, `not_by`, the self-set
//! effect, and what `meta` says (the hints, children, plurals, `can_delete` of `refuse_always`).

use crate::{CallError, Dispatcher, Principal, Response, Store, Surface, Value, Vault, errors};
use obop_registry::ui::{ChildDecl, ModelUi};
use obop_registry::{CommandDef, Effect, FieldDef as F, FieldKind, ModelDef, Module, intern};
use obop_types::DataClass;
use std::sync::Arc;

const U: &str = "hin.user";
const HR: &str = "hin.hr";
/// 2026-11-15T00:00:00Z.
const NOW: i64 = 1_763_164_800 + 365 * 86_400;

fn m(pairs: Vec<(&str, Value)>) -> Value {
    Value::Map(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}
fn t(s: &str) -> Value {
    Value::text(s)
}
fn get<'a>(v: &'a Value, k: &str) -> &'a Value {
    match v {
        Value::Map(m) => m.get(k).unwrap_or(&Value::Null),
        other => panic!("not a map: {other:?}"),
    }
}
fn text(v: &Value) -> String {
    match v {
        Value::Text(s) => s.clone(),
        other => panic!("not text: {other:?}"),
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
    Module {
        id: p.into(),
        name: "H".into(),
        models: vec![
            model(
                n("project"),
                vec![
                    f("name"),
                    f("active").kind(FieldKind::Bool),
                    f("rate").kind(FieldKind::Money),
                    // Only the HR group reads it.
                    F::new("budget", "d", DataClass::None, false, HR).kind(FieldKind::Money),
                    // A person's name: an agent sees a token.
                    F::new("lead", "d", DataClass::PublicDirectory, true, U),
                ],
                vec![],
            ),
            model(
                n("session"),
                vec![f("name"), f("state").select(&["open", "closed"])],
                vec![CommandDef::new("close", "Close", U).transition(
                    "state",
                    Some("open"),
                    "closed",
                )],
            ),
            model(
                n("task"),
                vec![
                    f("title"),
                    f("kind").default_value("chore"),
                    f("points").kind(FieldKind::Number).default_int(3),
                    f("urgent").kind(FieldKind::Bool).default_bool(false),
                    f("start").kind(FieldKind::Date).default_value("today"),
                    f("due").kind(FieldKind::Date).default_value("today+3"),
                    f("owner").default_value("$user"),
                    f("currency").default_value("$instance.currency"),
                    f("session_id")
                        .reference(&format!("{p}_session"))
                        .default_value(&format!("$open:{p}_session")),
                    f("project_id")
                        .reference(&format!("{p}_project"))
                        .ref_filter("active = true"),
                    f("rate")
                        .kind(FieldKind::Money)
                        .fill_from("project_id.rate"),
                    f("budget")
                        .kind(FieldKind::Money)
                        .fill_from("project_id.budget"),
                    f("lead").fill_from("project_id.lead"),
                    f("stamp").readonly(),
                    f("reason"),
                    f("reviewer"),
                    f("published_on").kind(FieldKind::Date),
                    f("state").select(&["new", "done", "rejected"]),
                ],
                vec![
                    CommandDef::new("finish", "Finish", U)
                        .transition("state", Some("new"), "done")
                        .not_by("reviewer")
                        .effect(
                            Effect::set_self(&format!("{p}_task"), &[("published_on", "$today")])
                                .when("!is_set(published_on)"),
                        )
                        .effect(Effect::create(
                            &format!("{p}_note"),
                            &[("task_id", "$self.id")],
                        )),
                    CommandDef::new("reject", "Reject", U)
                        .transition("state", Some("new"), "rejected")
                        .needs(&["reason"])
                        .danger()
                        .assertion_msg("is_set(reason)", "Say why"),
                ],
            ),
            model(
                n("note"),
                vec![
                    f("task_id").reference(&format!("{p}_task")),
                    f("day").kind(FieldKind::Date).default_value("today"),
                    f("stamp").readonly(),
                ],
                vec![],
            ),
            // A child by the rule: `<task>_line` with one ref to the task.
            model(
                n("task_line"),
                vec![f("task_id").reference(&format!("{p}_task")), f("what")],
                vec![],
            ),
            // A child only by declaration.
            model(
                n("attachment"),
                vec![f("of_task").reference(&format!("{p}_task")), f("name")],
                vec![],
            ),
        ],
        network_destinations: vec![],
        description: String::new(),
        translations: Default::default(),
        meta: Default::default(),
    }
}

/// The module with the declared child, the plural and a `[model.ui]` on the task.
fn full(p: &str) -> Module {
    let mut md = module(p);
    let task = md
        .models
        .iter_mut()
        .find(|x| x.name == format!("{p}_task"))
        .unwrap();
    *task = task.clone().with_ui(ModelUi {
        title: Some("{title}".into()),
        default_view: Some("kanban".into()),
        children: vec![ChildDecl {
            model: format!("{p}_attachment"),
            via: "of_task".into(),
            shape: Some("list".into()),
        }],
        ..Default::default()
    });
    let labels = format!(
        "[labels.en.model.{p}_task]\nname = \"Task\"\nplural = \"Tasks\"\n[labels.de.model.{p}_task]\nplural = \"Aufgaben\"\n"
    );
    md.translated(&[&labels])
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
        let p = format!("h{}", &uuid::Uuid::now_v7().simple().to_string()[22..]);
        let store = Store::open(&url).unwrap();
        let d = Arc::new(Dispatcher::new());
        d.set_clock(Arc::new(|| NOW));
        let module = full(&p);
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
    fn cmd(&self, model: &str, c: &str) -> String {
        format!("{}_{model}.{c}", self.p)
    }
    fn call_as(
        &self,
        who: &Principal,
        surface: Surface,
        model: &str,
        c: &str,
        input: Value,
    ) -> Result<Response, CallError> {
        self.d
            .call(surface, who, &mut Vault::new(), &self.cmd(model, c), &input)
    }
    fn ok(&self, model: &str, c: &str, input: Value) -> Value {
        match self.call_as(&self.who, Surface::Rest, model, c, input) {
            Ok(r) => r.value,
            Err(e) => panic!("{model}.{c} failed: {e:?}"),
        }
    }
    fn err(&self, model: &str, c: &str, input: Value) -> errors::Classified {
        match self.call_as(&self.who, Surface::Rest, model, c, input) {
            Err(e) => errors::classify(&e),
            Ok(r) => panic!("{model}.{c}: expected a refusal, got {:?}", r.value),
        }
    }
    fn create(&self, model: &str, pairs: Vec<(&str, Value)>) -> String {
        text(get(&self.ok(model, "create", m(pairs)), "id"))
    }
    fn read(&self, model: &str, id: &str) -> Value {
        self.ok(model, "get", m(vec![("id", t(id))]))
    }
    fn project(&self, active: bool) -> String {
        self.create(
            "project",
            vec![
                ("name", t("P")),
                ("active", Value::Bool(active)),
                ("rate", Value::Int(9500)),
                ("budget", Value::Int(100_000)),
                ("lead", t("Ada Lovelace")),
            ],
        )
    }
}

#[test]
fn defaults_fill_what_a_create_leaves_out_on_every_surface() {
    each(|r| {
        let session = r.create("session", vec![("name", t("Till 1"))]);
        for surface in [Surface::Rest, Surface::Mcp, Surface::Cli] {
            let v = r
                .call_as(
                    &r.who,
                    surface,
                    "task",
                    "create",
                    m(vec![("title", t("A"))]),
                )
                .unwrap()
                .value;
            assert_eq!(get(&v, "kind"), &t("chore"), "{surface:?}");
            assert_eq!(get(&v, "points"), &Value::Int(3));
            assert_eq!(get(&v, "urgent"), &Value::Bool(false));
            assert_eq!(get(&v, "start"), &t("2026-11-15"));
            assert_eq!(get(&v, "due"), &t("2026-11-18"));
            assert_eq!(get(&v, "owner"), &t(&format!("u-{}", r.p)));
            assert_eq!(get(&v, "currency"), &t("EUR"));
            assert_eq!(get(&v, "session_id"), &t(&session));
        }
        // What the caller sends is the caller's, null included.
        let v = r.ok(
            "task",
            "create",
            m(vec![
                ("title", t("B")),
                ("kind", t("bug")),
                ("points", Value::Null),
                ("session_id", Value::Null),
            ]),
        );
        assert_eq!(get(&v, "kind"), &t("bug"));
        assert_eq!(get(&v, "points"), &Value::Null);
        assert_eq!(get(&v, "session_id"), &Value::Null);
        // Another instance currency; two open sessions are no single open one.
        r.d.set_currency("SEK").unwrap();
        r.create("session", vec![("name", t("Till 2"))]);
        let v = r.ok("task", "create", m(vec![("title", t("C"))]));
        assert_eq!(get(&v, "currency"), &t("SEK"));
        assert_eq!(get(&v, "session_id"), &Value::Null);
        assert!(r.d.set_currency("sek").is_err());
    });
}

#[test]
fn defaults_apply_to_bulk_rows_and_to_the_records_an_effect_creates() {
    each(|r| {
        let v = r.ok(
            "task",
            "bulk_create",
            m(vec![(
                "rows",
                Value::List(vec![m(vec![("title", t("X"))]), m(vec![("title", t("Y"))])]),
            )]),
        );
        let Value::List(ids) = get(&v, "ids") else {
            panic!("{v:?}")
        };
        for id in ids {
            let row = r.read("task", &text(id));
            assert_eq!(get(&row, "kind"), &t("chore"));
            assert_eq!(get(&row, "due"), &t("2026-11-18"));
        }
        let task = r.create("task", vec![("title", t("E"))]);
        r.ok("task", "finish", m(vec![("id", t(&task))]));
        let notes = r.ok("note", "list", m(vec![]));
        let Value::List(notes) = notes else { panic!() };
        assert_eq!(notes.len(), 1);
        assert_eq!(get(&notes[0], "day"), &t("2026-11-15"));
    });
}

#[test]
fn fill_from_copies_only_what_the_caller_may_read_in_clear() {
    each(|r| {
        let proj = r.project(true);
        // A person with the ordinary group: the rate and the lead, not the budget (HR only).
        let v = r.ok(
            "task",
            "create",
            m(vec![("title", t("A")), ("project_id", t(&proj))]),
        );
        assert_eq!(get(&v, "rate"), &Value::Int(9500));
        assert_eq!(get(&v, "lead"), &t("Ada Lovelace"));
        assert_eq!(get(&v, "budget"), &Value::Null);
        // A person of HR gets the budget too.
        let hr = Principal::user(&format!("hr-{}", r.p), &[U, HR]);
        let v = r
            .call_as(
                &hr,
                Surface::Rest,
                "task",
                "create",
                m(vec![("title", t("B")), ("project_id", t(&proj))]),
            )
            .unwrap()
            .value;
        assert_eq!(get(&v, "budget"), &Value::Int(100_000));
        // An agent sees the lead as a token, so it is not copied for it.
        let bot = Principal::agent(&format!("bot-{}", r.p), &[U], &[U]);
        let v = r
            .call_as(
                &bot,
                Surface::Mcp,
                "task",
                "create",
                m(vec![("title", t("C")), ("project_id", t(&proj))]),
            )
            .unwrap()
            .value;
        let id = text(get(&v, "id"));
        let stored = r.read("task", &id);
        assert_eq!(get(&stored, "lead"), &Value::Null);
        assert_eq!(get(&stored, "rate"), &Value::Int(9500));
        // A value the caller gives wins.
        let v = r.ok(
            "task",
            "create",
            m(vec![
                ("title", t("D")),
                ("project_id", t(&proj)),
                ("rate", Value::Int(1)),
            ]),
        );
        assert_eq!(get(&v, "rate"), &Value::Int(1));
    });
}

#[test]
fn readonly_fields_are_refused_on_create_and_update_and_written_by_the_server() {
    each(|r| {
        let c = r.err(
            "task",
            "create",
            m(vec![("title", t("A")), ("stamp", t("x"))]),
        );
        assert_eq!((c.status, c.code), (400, "readonly_field"));
        assert!(
            c.body["error"].as_str().unwrap().contains("stamp"),
            "{}",
            c.body
        );
        let id = r.create("task", vec![("title", t("A"))]);
        let c = r.err("task", "update", m(vec![("id", t(&id)), ("stamp", t("x"))]));
        assert_eq!(c.code, "readonly_field");
        // The record sent back as it is passes.
        r.ok(
            "task",
            "update",
            m(vec![
                ("id", t(&id)),
                ("title", t("A2")),
                ("stamp", Value::Null),
            ]),
        );
        // Bulk rows are held to it too.
        let c = r.err(
            "task",
            "bulk_create",
            m(vec![(
                "rows",
                Value::List(vec![m(vec![("title", t("X")), ("stamp", t("y"))])]),
            )]),
        );
        assert_eq!(c.status, 400, "{}", c.body);
    });
}

#[test]
fn a_ref_filter_refuses_a_record_that_does_not_fit_on_create_and_update() {
    each(|r| {
        let on = r.project(true);
        let off = r.project(false);
        let id = r.create("task", vec![("title", t("A")), ("project_id", t(&on))]);
        let c = r.err(
            "task",
            "create",
            m(vec![("title", t("B")), ("project_id", t(&off))]),
        );
        assert_eq!((c.status, c.code), (400, "ref_not_allowed"));
        assert!(c.body["error"].as_str().unwrap().contains("active = true"));
        let c = r.err(
            "task",
            "update",
            m(vec![("id", t(&id)), ("project_id", t(&off))]),
        );
        assert_eq!(c.code, "ref_not_allowed");
        // A project that became inactive does not lock the tasks that name it.
        r.ok(
            "project",
            "update",
            m(vec![("id", t(&on)), ("active", Value::Bool(false))]),
        );
        r.ok(
            "task",
            "update",
            m(vec![("id", t(&id)), ("title", t("A2"))]),
        );
    });
}

#[test]
fn an_action_takes_the_fields_it_needs_in_one_call() {
    each(|r| {
        let id = r.create("task", vec![("title", t("A"))]);
        // Without the reason: refused before anything runs, saying what is needed.
        let c = r.err("task", "reject", m(vec![("id", t(&id))]));
        assert!(
            c.body["error"].as_str().unwrap().contains("needs reason"),
            "{}",
            c.body
        );
        // With it: one call writes the reason and moves the state.
        let v = r.ok(
            "task",
            "reject",
            m(vec![("id", t(&id)), ("reason", t("Duplicate"))]),
        );
        assert_eq!(get(&v, "state"), &t("rejected"));
        let row = r.read("task", &id);
        assert_eq!(get(&row, "reason"), &t("Duplicate"));
        assert_eq!(get(&row, "state"), &t("rejected"));
    });
}

#[test]
fn not_by_keeps_the_named_person_from_running_the_action() {
    each(|r| {
        let me = format!("u-{}", r.p);
        let id = r.create("task", vec![("title", t("A")), ("reviewer", t(&me))]);
        let c = r.err("task", "finish", m(vec![("id", t(&id))]));
        assert_eq!((c.status, c.code), (403, "denied"), "{}", c.body);
        let other = Principal::user("someone-else", &[U]);
        r.call_as(
            &other,
            Surface::Rest,
            "task",
            "finish",
            m(vec![("id", t(&id))]),
        )
        .unwrap();
    });
}

#[test]
fn a_set_effect_changes_the_record_itself_under_its_condition() {
    each(|r| {
        let id = r.create("task", vec![("title", t("A"))]);
        r.ok("task", "finish", m(vec![("id", t(&id))]));
        assert_eq!(get(&r.read("task", &id), "published_on"), &t("2026-11-15"));
        // Set before: the condition keeps it.
        let id = r.create(
            "task",
            vec![("title", t("B")), ("published_on", t("2026-01-01"))],
        );
        r.ok("task", "finish", m(vec![("id", t(&id))]));
        assert_eq!(get(&r.read("task", &id), "published_on"), &t("2026-01-01"));
    });
}

#[test]
fn meta_serves_the_hints_children_plurals_and_what_is_always_refused() {
    each(|r| {
        r.d.refuse_always(&r.p, &r.cmd("note", "delete"), "a note is kept");
        let meta = r.d.meta(Surface::Rest, &r.who);
        let module = meta.iter().find(|x| x.id == r.p).unwrap();
        let by = |n: &str| {
            module
                .models
                .iter()
                .find(|x| x.name == format!("{}_{n}", r.p))
                .unwrap()
        };
        let task = by("task");
        let kids: Vec<(String, String, Option<String>)> = task
            .children
            .iter()
            .map(|c| (c.model.clone(), c.via.clone(), c.shape.clone()))
            .collect();
        assert_eq!(
            kids,
            vec![
                (
                    format!("{}_task_line", r.p),
                    "task_id".into(),
                    Some("lines".into())
                ),
                (
                    format!("{}_attachment", r.p),
                    "of_task".into(),
                    Some("list".into())
                ),
            ]
        );
        assert_eq!(task.plurals.get("en").map(String::as_str), Some("Tasks"));
        assert_eq!(task.plurals.get("de").map(String::as_str), Some("Aufgaben"));
        assert_eq!(
            task.ui.as_ref().and_then(|u| u.default_view.as_deref()),
            Some("kanban")
        );
        let due = task.fields.iter().find(|f| f.name == "due").unwrap();
        assert!(due.hints.as_ref().is_some_and(|h| h.default.is_some()));
        let reject = task
            .actions
            .iter()
            .find(|a| a.id.ends_with(".reject"))
            .unwrap();
        assert_eq!(
            reject.ui.as_ref().unwrap().needs,
            vec!["reason".to_string()]
        );
        assert!(!by("note").can_delete);
        assert!(by("task").can_delete);
        // The server refuses it as the metadata says.
        let id = r.create("note", vec![]);
        let c = r.err("note", "delete", m(vec![("id", t(&id))]));
        assert!(
            c.body["error"].as_str().unwrap().contains("kept"),
            "{}",
            c.body
        );
    });
}

/// The gate refuses hints that cannot work, naming the field.
#[test]
fn the_gate_checks_every_hint() {
    let p = "hgate";
    let f = |name: &'static str| F::new(name, "d", DataClass::None, false, U);
    let bad = |field: F, want: &str| {
        let mut md = module(p);
        md.models[2].fields.push(field);
        let v = obop_registry::check_module(&md);
        assert!(v.iter().any(|x| x.problem.contains(want)), "{want}: {v:?}");
    };
    bad(
        f("x1").kind(FieldKind::Number).default_value("today"),
        "does not fit",
    );
    bad(f("x2").fill_from("title"), "ref.field");
    bad(f("x3").fill_from("nope_id.rate"), "has no field nope_id");
    bad(f("x4").ref_filter("active = true"), "only for a ref field");
    bad(
        f("x5")
            .reference(&format!("{p}_project"))
            .ref_filter("nope = 1"),
        "has no field nope",
    );
    bad(f("x6").display("percent"), "does not fit");
    bad(
        f("x7").kind(FieldKind::Number).aggregate("max"),
        "aggregate must be one of",
    );
    bad(
        f("x8").readonly().required(),
        "needs a default or fill_from",
    );
    bad(
        f("x9")
            .reference(&format!("{p}_project"))
            .default_value(&format!("$open:{p}_project")),
        "no state field",
    );
    let mut md = module(p);
    md.models[2].commands.push(
        CommandDef::new("odd", "Odd", U)
            .transition("state", Some("new"), "done")
            .needs(&["nope"])
            .primary()
            .danger(),
    );
    let v = obop_registry::check_module(&md);
    assert!(v.iter().any(|x| x.problem.contains("needs: ")), "{v:?}");
    assert!(
        v.iter().any(|x| x.problem.contains("primary or danger")),
        "{v:?}"
    );
    let mut md = module(p);
    md.meta = md.meta.home("nowhere");
    assert!(
        obop_registry::check_module(&md)
            .iter()
            .any(|x| x.problem.contains("home must be")),
    );
}
