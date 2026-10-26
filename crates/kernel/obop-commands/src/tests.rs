//! Tests for governed fields and implicit transitions (D48), delete, and server-side search
//! (D49). Every test runs on the memory and SQLite backends, and on PostgreSQL when
//! `OBOP_TEST_POSTGRES_URL` is set. Model names carry a random prefix so repeated runs against
//! one PostgreSQL database never see each other's records.

use crate::{CallError, Dispatcher, Principal, Response, Store, Surface, Value, Vault};
use obop_registry::{CommandDef, FieldDef as F, FieldKind, ModelDef, Module, intern};
use obop_types::DataClass;

mod polish;

struct Rig {
    d: Dispatcher,
    store: Store,
    p: String,
}

impl Rig {
    fn model(&self, name: &str) -> String {
        format!("{}_{name}", self.p)
    }

    fn cmd(&self, model: &str, c: &str) -> String {
        format!("{}.{c}", self.model(model))
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

    /// The handler error message of a call that must fail.
    fn err(&self, who: &Principal, model: &str, c: &str, input: Value) -> String {
        match self.call(who, model, c, input) {
            Err(CallError::Handler(m)) => m,
            other => panic!("{model}.{c}: expected a handler error, got {other:?}"),
        }
    }

    fn id(&self, v: &Value) -> String {
        let Value::Map(m) = v else { panic!("{v:?}") };
        let Value::Text(t) = &m["id"] else { panic!() };
        t.clone()
    }

    fn field(&self, model: &str, id: &str, f: &str) -> Value {
        self.store
            .field(&self.model(model), id, f)
            .unwrap_or(Value::Null)
    }

    /// Insert straight into the store, as an import or a seed would.
    fn seed(&self, model: &str, pairs: Vec<(&str, Value)>) -> String {
        self.store
            .insert(
                &self.model(model),
                pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            )
            .unwrap()
    }
}

fn m(pairs: Vec<(&str, Value)>) -> Value {
    Value::Map(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn t(s: &str) -> Value {
    Value::text(s)
}

fn by_id(id: &str) -> Value {
    m(vec![("id", t(id))])
}

const U: &str = "t.user";
const APPROVER: &str = "t.approver";

fn user() -> Principal {
    Principal::user("u", &[U])
}
fn approver() -> Principal {
    Principal::user("ap", &[U, APPROVER])
}
fn bot() -> Principal {
    Principal::agent("bot", &[U, APPROVER], &[U, APPROVER])
}

fn fd(name: &'static str, kind: FieldKind) -> F {
    F::new(name, "d", DataClass::None, false, U).kind(kind)
}

fn module(p: &str) -> Module {
    let n = |s: &str| intern(&format!("{p}_{s}"));
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
        name: "T".into(),
        models: vec![
            model(
                n("move"),
                vec![
                    fd("journal", FieldKind::Text).required(),
                    fd("date", FieldKind::Date),
                    fd("amount", FieldKind::Money),
                    fd("narration", FieldKind::LongText),
                    fd("state", FieldKind::Text).select(&[
                        "draft",
                        "posted",
                        "cancelled",
                        "reversed",
                    ]),
                ],
                vec![
                    CommandDef::new("post", "Post", APPROVER)
                        .approval()
                        .transition("state", Some("draft"), "posted"),
                    CommandDef::new("cancel", "Cancel", U).transition(
                        "state",
                        Some("draft"),
                        "cancelled",
                    ),
                    CommandDef::new("reverse", "Reverse", APPROVER)
                        .approval()
                        .transition("state", Some("posted"), "reversed"),
                ],
            ),
            model(
                n("task"),
                vec![
                    fd("title", FieldKind::Text).required(),
                    fd("size", FieldKind::Number),
                    fd("priority", FieldKind::Text).select(&["normal", "urgent", "critical"]),
                    fd("state", FieldKind::Text).select(&["open", "closed"]),
                ],
                vec![
                    CommandDef::new("escalate", "Escalate", U).transition(
                        "priority",
                        Some("normal"),
                        "urgent",
                    ),
                    CommandDef::new("go_critical", "Critical", APPROVER)
                        .approval()
                        .transition("priority", Some("urgent"), "critical"),
                    CommandDef::new("calm", "Calm", U).transition(
                        "priority",
                        Option::None,
                        "normal",
                    ),
                    CommandDef::new("close", "Close", U).transition(
                        "state",
                        Option::None,
                        "closed",
                    ),
                    // A second way to close, for a group that holds neither.
                    CommandDef::new("force_close", "Force close", APPROVER).transition(
                        "state",
                        Option::None,
                        "closed",
                    ),
                ],
            ),
            model(
                n("line"),
                vec![
                    fd("label", FieldKind::Text),
                    fd("move_id", FieldKind::Ref(n("move"))),
                    fd("other_move_id", FieldKind::Ref(n("move"))),
                    fd("parent_id", FieldKind::Ref(n("line"))),
                ],
                vec![],
            ),
            model(
                n("party"),
                vec![
                    F::new("name", "d", DataClass::PublicDirectory, true, U).required(),
                    F::new("city", "d", DataClass::None, false, U),
                    F::new("tax_id", "d", DataClass::Financial, true, U),
                    F::new("pin", "d", DataClass::Secret, false, U),
                    F::new("salary", "d", DataClass::None, false, "t.hr").kind(FieldKind::Money),
                    fd("age", FieldKind::Number),
                    fd("born", FieldKind::Date),
                    fd("vip", FieldKind::Bool),
                    fd("tier", FieldKind::Text).select(&["bronze", "silver", "gold"]),
                    fd("move_ref", FieldKind::Ref(n("move"))),
                ],
                vec![],
            ),
        ],
        network_destinations: vec![],
        description: String::new(),
        translations: Default::default(),
        meta: Default::default(),
    }
}

fn each_backend(f: impl Fn(&Rig)) {
    let mut urls = vec!["memory:".to_string(), "sqlite::memory:".to_string()];
    if let Ok(u) = std::env::var("OBOP_TEST_POSTGRES_URL") {
        urls.push(u);
    }
    for url in urls {
        let p = format!("t{}", &uuid::Uuid::now_v7().simple().to_string()[22..]);
        let store = Store::open(&url).unwrap();
        let d = Dispatcher::new();
        let module = module(&p);
        assert!(obop_registry::check_module(&module).is_empty());
        d.register_crud(&module, &store);
        for (model, cmds) in [
            ("move", vec!["post", "cancel", "reverse"]),
            (
                "task",
                vec!["escalate", "go_critical", "calm", "close", "force_close"],
            ),
        ] {
            for c in cmds {
                d.register_transition(&module, &format!("{p}_{model}"), c, &store);
            }
        }
        f(&Rig { d, store, p });
    }
}

fn plan_of(r: Result<Response, CallError>) -> String {
    match r {
        Err(CallError::ApprovalRequired { plan_id }) => plan_id,
        other => panic!("expected an approval plan, got {other:?}"),
    }
}

// ---- governed fields ----------------------------------------------------------------------

#[test]
fn create_may_only_start_in_the_first_state() {
    each_backend(|r| {
        let u = user();
        // Omitted or null: the first option is filled in.
        let a = r.ok(&u, "move", "create", m(vec![("journal", t("J"))]));
        assert_eq!(r.field("move", &r.id(&a), "state"), t("draft"));
        let b = r.ok(
            &u,
            "move",
            "create",
            m(vec![("journal", t("J")), ("state", Value::Null)]),
        );
        assert_eq!(r.field("move", &r.id(&b), "state"), t("draft"));
        r.ok(
            &u,
            "move",
            "create",
            m(vec![("journal", t("J")), ("state", t("draft"))]),
        );
        // A late state is refused, whoever asks, with a clear message.
        for who in [
            user(),
            approver(),
            Principal::user("root", &["platform.admin"]),
        ] {
            let e = r.err(
                &who,
                "move",
                "create",
                m(vec![("journal", t("J")), ("state", t("posted"))]),
            );
            assert!(
                e.contains("state starts as \"draft\"") && e.contains("actions"),
                "{e}"
            );
        }
        // Not an option at all, or not text: the ordinary validation message.
        let e = r.err(
            &u,
            "move",
            "create",
            m(vec![("journal", t("J")), ("state", t("bogus"))]),
        );
        assert!(
            e.contains("must be one of: draft, posted, cancelled, reversed"),
            "{e}"
        );
        let e = r.err(
            &u,
            "move",
            "create",
            m(vec![("journal", t("J")), ("state", Value::Int(1))]),
        );
        assert!(e.contains("must be one of"), "{e}");
        // The same for a governed field that is not called `state`.
        let e = r.err(
            &u,
            "task",
            "create",
            m(vec![("title", t("x")), ("priority", t("critical"))]),
        );
        assert!(e.contains("priority starts as \"normal\""), "{e}");
        let task = r.ok(&u, "task", "create", m(vec![("title", t("x"))]));
        assert_eq!(r.field("task", &r.id(&task), "priority"), t("normal"));
        assert_eq!(r.field("task", &r.id(&task), "state"), t("open"));
        // A required field is still required on create.
        let e = r.err(&u, "move", "create", m(vec![]));
        assert!(e.contains("journal is required"), "{e}");
        // The default is applied before approval gating: nothing about create needs a plan.
        assert!(
            r.call(&bot(), "move", "create", m(vec![("journal", t("J"))]))
                .is_ok()
        );
    });
}

#[test]
fn the_accounting_example_a_plain_user_cannot_post_by_update() {
    each_backend(|r| {
        let u = user();
        let id = r.id(&r.ok(&u, "move", "create", m(vec![("journal", t("J"))])));
        let e = r.err(
            &u,
            "move",
            "update",
            m(vec![("id", t(&id)), ("state", t("posted"))]),
        );
        assert!(
            e.contains("is the post action") && e.contains(APPROVER),
            "{e}"
        );
        assert_eq!(r.field("move", &id, "state"), t("draft"));
        // The action itself is refused the same way.
        assert!(matches!(
            r.call(&u, "move", "post", by_id(&id)),
            Err(CallError::Forbidden(_))
        ));
        // A human with the approver group may post by update, exactly as by the action.
        r.ok(
            &approver(),
            "move",
            "update",
            m(vec![("id", t(&id)), ("state", t("posted"))]),
        );
        assert_eq!(r.field("move", &id, "state"), t("posted"));
    });
}

#[test]
fn an_agent_proposing_an_implicit_transition_gets_a_plan_and_a_human_approves_it() {
    each_backend(|r| {
        let id = r.id(&r.ok(&user(), "move", "create", m(vec![("journal", t("J"))])));
        let plan = plan_of(r.call(
            &bot(),
            "move",
            "update",
            m(vec![
                ("id", t(&id)),
                ("state", t("posted")),
                ("narration", t("n")),
            ]),
        ));
        // Nothing happened yet, not even the ordinary field.
        assert_eq!(r.field("move", &id, "state"), t("draft"));
        assert_eq!(r.field("move", &id, "narration"), Value::Null);
        // An agent cannot approve; a human without the transition's group cannot either.
        let mut v = Vault::new();
        assert!(matches!(
            r.d.approve(&bot(), &mut v, &plan),
            Err(CallError::Forbidden(_))
        ));
        assert!(matches!(
            r.d.approve(&user(), &mut v, &plan),
            Err(CallError::Forbidden(_))
        ));
        assert_eq!(r.field("move", &id, "state"), t("draft"));
        r.d.approve(&approver(), &mut v, &plan).unwrap();
        assert_eq!(r.field("move", &id, "state"), t("posted"));
        assert_eq!(r.field("move", &id, "narration"), t("n"));
        // A plan runs once.
        assert!(matches!(
            r.d.approve(&approver(), &mut v, &plan),
            Err(CallError::UnknownPlan(_))
        ));
    });
}

#[test]
fn an_agent_without_the_transitions_group_is_refused_at_once() {
    each_backend(|r| {
        let id = r.id(&r.ok(&user(), "move", "create", m(vec![("journal", t("J"))])));
        let weak = Principal::agent("weak", &[U, APPROVER], &[U]); // scoped down
        let e = r.err(
            &weak,
            "move",
            "update",
            m(vec![("id", t(&id)), ("state", t("posted"))]),
        );
        assert!(e.contains(APPROVER), "{e}");
        // A transition that needs no approval runs at once for an agent.
        r.ok(
            &weak,
            "move",
            "update",
            m(vec![("id", t(&id)), ("state", t("cancelled"))]),
        );
        assert_eq!(r.field("move", &id, "state"), t("cancelled"));
    });
}

#[test]
fn a_proposal_is_checked_again_when_approved() {
    each_backend(|r| {
        let id = r.id(&r.ok(&user(), "move", "create", m(vec![("journal", t("J"))])));
        let plan = plan_of(r.call(
            &bot(),
            "move",
            "update",
            m(vec![("id", t(&id)), ("state", t("posted"))]),
        ));
        // Meanwhile somebody cancels the entry: posting is no longer a legal move.
        r.ok(&user(), "move", "cancel", by_id(&id));
        let e = match r.d.approve(&approver(), &mut Vault::new(), &plan) {
            Err(CallError::Handler(m)) => m,
            other => panic!("{other:?}"),
        };
        assert!(
            e.contains("cannot change from \"cancelled\" to \"posted\""),
            "{e}"
        );
        assert_eq!(r.field("move", &id, "state"), t("cancelled"));
    });
}

#[test]
fn updates_may_not_skip_states_or_leave_final_ones() {
    each_backend(|r| {
        let u = approver();
        let id = r.id(&r.ok(&u, "move", "create", m(vec![("journal", t("J"))])));
        // draft -> reversed skips posted.
        let e = r.err(
            &u,
            "move",
            "update",
            m(vec![("id", t(&id)), ("state", t("reversed"))]),
        );
        assert!(
            e.contains("cannot change from \"draft\" to \"reversed\"")
                && e.contains("it can only go to: posted, cancelled"),
            "{e}"
        );
        // Back to the first state is not a transition either.
        r.ok(
            &u,
            "move",
            "update",
            m(vec![("id", t(&id)), ("state", t("posted"))]),
        );
        let e = r.err(
            &u,
            "move",
            "update",
            m(vec![("id", t(&id)), ("state", t("draft"))]),
        );
        assert!(e.contains("it can only go to: reversed"), "{e}");
        r.ok(
            &u,
            "move",
            "update",
            m(vec![("id", t(&id)), ("state", t("reversed"))]),
        );
        let e = r.err(
            &u,
            "move",
            "update",
            m(vec![("id", t(&id)), ("state", t("posted"))]),
        );
        assert!(e.contains("no transition leaves \"reversed\""), "{e}");
        assert_eq!(r.field("move", &id, "state"), t("reversed"));
        // Clearing or mistyping the field is refused too.
        let e = r.err(
            &u,
            "move",
            "update",
            m(vec![("id", t(&id)), ("state", Value::Null)]),
        );
        assert!(e.contains("must be one of"), "{e}");
        let e = r.err(
            &u,
            "move",
            "update",
            m(vec![("id", t(&id)), ("state", Value::Bool(true))]),
        );
        assert!(e.contains("must be one of"), "{e}");
        let e = r.err(
            &u,
            "move",
            "update",
            m(vec![("id", t(&id)), ("state", t("nope"))]),
        );
        assert!(e.contains("must be one of"), "{e}");
    });
}

#[test]
fn kanban_style_updates_follow_the_transitions() {
    each_backend(|r| {
        let u = user();
        let id = r.id(&r.ok(&u, "move", "create", m(vec![("journal", t("J"))])));
        // The UI sends only the new state; draft -> cancelled has a transition for plain users.
        r.ok(
            &u,
            "move",
            "update",
            m(vec![("id", t(&id)), ("state", t("cancelled"))]),
        );
        assert_eq!(r.field("move", &id, "state"), t("cancelled"));
        // Sending the unchanged state along with other edits is not a transition.
        r.ok(
            &u,
            "move",
            "update",
            m(vec![
                ("id", t(&id)),
                ("state", t("cancelled")),
                ("narration", t("late note")),
            ]),
        );
        assert_eq!(r.field("move", &id, "narration"), t("late note"));
        // A `from`-less transition is reachable from every state, including a null one.
        let tid = r.id(&r.ok(&u, "task", "create", m(vec![("title", t("x"))])));
        r.ok(
            &u,
            "task",
            "update",
            m(vec![("id", t(&tid)), ("priority", t("urgent"))]),
        ); // escalate
        let e = r.err(
            &u,
            "task",
            "update",
            m(vec![("id", t(&tid)), ("priority", t("critical"))]),
        );
        assert!(e.contains("go_critical") && e.contains(APPROVER), "{e}");
        r.ok(
            &u,
            "task",
            "update",
            m(vec![("id", t(&tid)), ("priority", t("normal"))]),
        ); // calm
        r.ok(
            &u,
            "task",
            "update",
            m(vec![("id", t(&tid)), ("state", t("closed"))]),
        ); // close
        let nul = r.seed("task", vec![("title", t("legacy"))]); // no priority stored at all
        r.ok(
            &u,
            "task",
            "update",
            m(vec![("id", t(&nul)), ("priority", t("normal"))]),
        );
        assert_eq!(r.field("task", &nul, "priority"), t("normal"));
    });
}

#[test]
fn several_governed_fields_in_one_update_each_need_their_transition() {
    each_backend(|r| {
        let u = user();
        let tid = r.id(&r.ok(&u, "task", "create", m(vec![("title", t("x"))])));
        r.ok(
            &u,
            "task",
            "update",
            m(vec![
                ("id", t(&tid)),
                ("priority", t("urgent")),
                ("state", t("closed")),
            ]),
        );
        assert_eq!(r.field("task", &tid, "priority"), t("urgent"));
        assert_eq!(r.field("task", &tid, "state"), t("closed"));
        // One of them lacking permission fails the lot.
        let tid = r.id(&r.ok(&u, "task", "create", m(vec![("title", t("y"))])));
        r.ok(
            &u,
            "task",
            "update",
            m(vec![("id", t(&tid)), ("priority", t("urgent"))]),
        );
        let e = r.err(
            &u,
            "task",
            "update",
            m(vec![
                ("id", t(&tid)),
                ("priority", t("critical")),
                ("state", t("closed")),
            ]),
        );
        assert!(e.contains("go_critical"), "{e}");
        assert_eq!(
            r.field("task", &tid, "state"),
            t("open"),
            "applied atomically or not at all"
        );
        // An agent proposing both is told to wait for approval of the whole update.
        let plan = plan_of(r.call(
            &bot(),
            "task",
            "update",
            m(vec![
                ("id", t(&tid)),
                ("priority", t("critical")),
                ("state", t("closed")),
            ]),
        ));
        r.d.approve(&approver(), &mut Vault::new(), &plan).unwrap();
        assert_eq!(r.field("task", &tid, "priority"), t("critical"));
        assert_eq!(r.field("task", &tid, "state"), t("closed"));
    });
}

#[test]
fn a_mixed_update_applies_atomically_or_fails_as_a_whole() {
    each_backend(|r| {
        let u = user();
        let id = r.id(&r.ok(&u, "move", "create", m(vec![("journal", t("J"))])));
        // Governed change fine, an ordinary field invalid: nothing is written.
        let e = r.err(
            &u,
            "move",
            "update",
            m(vec![
                ("id", t(&id)),
                ("state", t("cancelled")),
                ("date", t("2023-02-30")),
            ]),
        );
        assert!(e.contains("move.date must be a real calendar date"), "{e}");
        assert_eq!(r.field("move", &id, "state"), t("draft"));
        // Governed change refused, ordinary field fine: nothing is written.
        r.err(
            &u,
            "move",
            "update",
            m(vec![
                ("id", t(&id)),
                ("state", t("posted")),
                ("narration", t("x")),
            ]),
        );
        assert_eq!(r.field("move", &id, "narration"), Value::Null);
        // Both fine: both applied.
        r.ok(
            &u,
            "move",
            "update",
            m(vec![
                ("id", t(&id)),
                ("state", t("cancelled")),
                ("amount", Value::Int(5)),
            ]),
        );
        assert_eq!(r.field("move", &id, "amount"), Value::Int(5));
        assert_eq!(r.field("move", &id, "state"), t("cancelled"));
        // Ordinary fields never need a transition.
        r.ok(
            &u,
            "move",
            "update",
            m(vec![("id", t(&id)), ("narration", t("z"))]),
        );
    });
}

#[test]
fn actions_and_implicit_transitions_refuse_records_with_empty_required_fields() {
    each_backend(|r| {
        // Seeded without the required journal, as legacy data could be.
        let id = r.seed("move", vec![("state", t("draft"))]);
        let e = r.err(&user(), "move", "cancel", by_id(&id));
        assert!(e.contains("move.journal is required"), "{e}");
        let e = r.err(
            &user(),
            "move",
            "update",
            m(vec![("id", t(&id)), ("state", t("cancelled"))]),
        );
        assert!(
            e.contains("move.journal is required") && e.contains("cancelled"),
            "{e}"
        );
        assert_eq!(r.field("move", &id, "state"), t("draft"));
        // A null required field counts as empty too.
        let id2 = r.seed(
            "move",
            vec![("state", t("draft")), ("journal", Value::Null)],
        );
        assert!(
            r.err(&user(), "move", "cancel", by_id(&id2))
                .contains("journal")
        );
        // The same update carrying the missing field fixes it in one go.
        r.ok(
            &user(),
            "move",
            "update",
            m(vec![
                ("id", t(&id)),
                ("journal", t("J")),
                ("state", t("cancelled")),
            ]),
        );
        assert_eq!(r.field("move", &id, "state"), t("cancelled"));
        // Ordinary updates are not blocked by it.
        r.ok(
            &user(),
            "move",
            "update",
            m(vec![("id", t(&id2)), ("narration", t("n"))]),
        );
    });
}

#[test]
fn actions_still_check_the_from_state_and_approval() {
    each_backend(|r| {
        let id = r.id(&r.ok(&user(), "move", "create", m(vec![("journal", t("J"))])));
        let e = r.err(&approver(), "move", "reverse", by_id(&id));
        assert_eq!(e, "state must be posted to do this, it is \"draft\"");
        // An agent's proposal that cannot run is refused before a plan is made.
        let e = r.err(&bot(), "move", "reverse", by_id(&id));
        assert!(e.contains("must be posted"), "{e}");
        let plan = plan_of(r.call(&bot(), "move", "post", by_id(&id)));
        r.d.approve(&approver(), &mut Vault::new(), &plan).unwrap();
        assert_eq!(r.field("move", &id, "state"), t("posted"));
        let e = r.err(&approver(), "move", "post", by_id(&id));
        assert!(e.contains("must be draft"), "{e}");
        let e = r.err(&approver(), "move", "post", by_id("ghost"));
        assert!(e.contains("no ") && e.contains("ghost"), "{e}");
    });
}

#[test]
fn of_two_transitions_to_one_state_the_caller_uses_one_they_may() {
    each_backend(|r| {
        let tid = r.id(&r.ok(&user(), "task", "create", m(vec![("title", t("x"))])));
        // `close` (everyone) and `force_close` (approvers) both reach closed.
        r.ok(
            &user(),
            "task",
            "update",
            m(vec![("id", t(&tid)), ("state", t("closed"))]),
        );
        assert_eq!(r.field("task", &tid, "state"), t("closed"));
    });
}

#[test]
fn bypass_attempts_through_other_doors_fail() {
    each_backend(|r| {
        let u = user();
        let id = r.id(&r.ok(&u, "move", "create", m(vec![("journal", t("J"))])));
        // Unknown fields and the id cannot smuggle a state in.
        assert!(
            r.call(
                &u,
                "move",
                "update",
                m(vec![("id", t(&id)), ("State", t("posted"))])
            )
            .is_err()
        );
        // A caller-supplied `_caller` is dropped, and cannot unlock anything.
        let e = r.err(
            &u,
            "move",
            "update",
            m(vec![
                ("id", t(&id)),
                ("state", t("posted")),
                ("_caller", t("platform.admin")),
            ]),
        );
        assert!(e.contains("post action"), "{e}");
        // Updating a record that does not exist is an error, not a silent create.
        let e = r.err(
            &u,
            "move",
            "update",
            m(vec![("id", t("ghost")), ("state", t("cancelled"))]),
        );
        assert!(e.contains("ghost"), "{e}");
        // Surfaces do not differ.
        for s in [Surface::Mcp, Surface::Cli] {
            let e = r.d.call(
                s,
                &u,
                &mut Vault::new(),
                &r.cmd("move", "update"),
                &m(vec![("id", t(&id)), ("state", t("posted"))]),
            );
            assert!(matches!(e, Err(CallError::Handler(_))), "{e:?}");
        }
        assert_eq!(r.field("move", &id, "state"), t("draft"));
    });
}

// ---- delete -------------------------------------------------------------------------------

#[test]
fn delete_removes_unreferenced_records_and_refuses_referenced_ones() {
    each_backend(|r| {
        let u = user();
        let mv = r.id(&r.ok(&u, "move", "create", m(vec![("journal", t("J"))])));
        let mut lines = Vec::new();
        for i in 0..7 {
            lines.push(r.id(&r.ok(
                &u,
                "line",
                "create",
                m(vec![("label", t(&format!("l{i}"))), ("move_id", t(&mv))]),
            )));
        }
        // A second reference field of the same model counts the record once.
        r.ok(
            &u,
            "line",
            "update",
            m(vec![("id", t(&lines[0])), ("other_move_id", t(&mv))]),
        );
        let party = r.id(&r.ok(
            &u,
            "party",
            "create",
            m(vec![("name", t("P")), ("move_ref", t(&mv))]),
        ));
        let e = r.err(&u, "move", "delete", by_id(&mv));
        let line_model = r.model("line");
        assert!(
            e.contains(&format!("cannot delete {} {mv}", r.model("move")))
                && e.contains(&format!("{line_model} (7): "))
                && e.contains(&format!("{} (1): {party}", r.model("party"))),
            "{e}"
        );
        // At most five ids are listed, then an ellipsis.
        let listed = e.split("(7): ").nth(1).unwrap().split(';').next().unwrap();
        assert_eq!(
            listed.matches(", ").count(),
            5,
            "five ids and an ellipsis: {listed}"
        );
        assert!(listed.ends_with("..."), "{listed}");
        assert_eq!(r.store.count(&r.model("move")), 1);
        // Removing the referrers frees the record.
        for l in &lines {
            r.ok(&u, "line", "delete", by_id(l));
        }
        assert!(r.err(&u, "move", "delete", by_id(&mv)).contains(&party));
        r.ok(&u, "party", "delete", by_id(&party));
        let gone = r.ok(&u, "move", "delete", by_id(&mv));
        assert_eq!(gone, m(vec![("id", t(&mv))]));
        assert_eq!(r.store.count(&r.model("move")), 0);
        assert!(r.err(&u, "move", "delete", by_id(&mv)).contains(&mv));
        assert!(r.err(&u, "move", "get", by_id(&mv)).contains("no "));
    });
}

#[test]
fn a_record_may_refer_to_itself_and_still_be_deleted() {
    each_backend(|r| {
        let u = user();
        let a = r.id(&r.ok(&u, "line", "create", m(vec![("label", t("a"))])));
        r.ok(
            &u,
            "line",
            "update",
            m(vec![("id", t(&a)), ("parent_id", t(&a))]),
        );
        let b = r.id(&r.ok(
            &u,
            "line",
            "create",
            m(vec![("label", t("b")), ("parent_id", t(&a))]),
        ));
        let e = r.err(&u, "line", "delete", by_id(&a));
        assert!(
            e.contains(&format!("(1): {b}")) && !e.contains(&format!(": {a}")),
            "{e}"
        );
        r.ok(&u, "line", "delete", by_id(&b));
        r.ok(&u, "line", "delete", by_id(&a));
    });
}

#[test]
fn an_agent_can_only_propose_a_delete() {
    each_backend(|r| {
        let id = r.id(&r.ok(&user(), "party", "create", m(vec![("name", t("P"))])));
        let plan = plan_of(r.call(&bot(), "party", "delete", by_id(&id)));
        assert_eq!(r.store.count(&r.model("party")), 1, "nothing deleted yet");
        let mut v = Vault::new();
        assert!(matches!(
            r.d.approve(&bot(), &mut v, &plan),
            Err(CallError::Forbidden(_))
        ));
        r.d.approve(&approver(), &mut v, &plan).unwrap();
        assert_eq!(r.store.count(&r.model("party")), 0);
        // A proposal that would break integrity is refused before a plan exists, and again at
        // approval if a reference appeared in between.
        let mv = r.id(&r.ok(&user(), "move", "create", m(vec![("journal", t("J"))])));
        let e = r.err(&bot(), "move", "delete", by_id("ghost"));
        assert!(e.contains("ghost"), "{e}");
        let plan = plan_of(r.call(&bot(), "move", "delete", by_id(&mv)));
        let party = r.id(&r.ok(
            &user(),
            "party",
            "create",
            m(vec![("name", t("P")), ("move_ref", t(&mv))]),
        ));
        let e = match r.d.approve(&approver(), &mut Vault::new(), &plan) {
            Err(CallError::Handler(m)) => m,
            other => panic!("{other:?}"),
        };
        assert!(e.contains(&party), "{e}");
        assert_eq!(r.store.count(&r.model("move")), 1);
    });
}

#[test]
fn delete_is_visible_in_meta_and_needs_the_write_group() {
    each_backend(|r| {
        let reader = Principal::user("ro", &[U]);
        let meta = r.d.meta(Surface::Rest, &reader);
        let model = meta[0]
            .models
            .iter()
            .find(|x| x.name == r.model("move"))
            .unwrap();
        assert!(model.can_delete && model.can_create && model.can_update);
        assert!(
            model
                .actions
                .iter()
                .all(|a| !["delete", "search"].iter().any(|g| a.id.ends_with(g))),
            "generated commands are not listed as actions"
        );
        assert!(
            model.actions.iter().all(|a| !a.id.ends_with(".post")),
            "the post action needs a group this user lacks"
        );
        let ap = r.d.meta(Surface::Rest, &approver());
        let model = ap[0]
            .models
            .iter()
            .find(|x| x.name == r.model("move"))
            .unwrap();
        let post = model
            .actions
            .iter()
            .find(|a| a.id.ends_with(".post"))
            .unwrap();
        let tr = post.transition.unwrap();
        assert_eq!(
            (tr.field, tr.from, tr.to),
            ("state", Some("draft"), "posted")
        );
        // The catalog (MCP tools, CLI help) includes the generated commands automatically.
        let cat = r.d.visible(Surface::Mcp, &user());
        for c in ["delete", "search", "list"] {
            assert!(cat.contains(&r.cmd("move", c)), "{c}");
        }
        let info = r.d.catalog(Surface::Mcp, &user());
        assert!(
            info.iter()
                .find(|c| c.id == r.cmd("move", "delete"))
                .unwrap()
                .needs_approval
        );
        // Not held: not callable.
        let nobody = Principal::user("n", &["other.group"]);
        assert!(matches!(
            r.call(&nobody, "move", "delete", by_id("x")),
            Err(CallError::Forbidden(_))
        ));
    });
}

// ---- search -------------------------------------------------------------------------------

fn parties(r: &Rig) -> Vec<String> {
    let u = user();
    // name, city, age, born, vip, tier, salary
    #[allow(clippy::type_complexity)]
    let rows: [(
        &str,
        Option<&str>,
        Option<i64>,
        Option<&str>,
        Option<bool>,
        Option<&str>,
        Option<i64>,
    ); 6] = [
        (
            "Ada",
            Some("Berlin"),
            Some(36),
            Some("1988-05-17"),
            Some(true),
            Some("gold"),
            Some(900_000),
        ),
        (
            "bob",
            Some("berlin"),
            Some(9),
            Some("2015-01-02"),
            Some(false),
            Some("bronze"),
            Some(5),
        ),
        (
            "Cy",
            Some("Paris"),
            Some(100),
            Some("1924-12-31"),
            None,
            Some("silver"),
            Some(-50),
        ),
        ("Dee", None, None, None, Some(true), None, None),
        (
            "Eve",
            Some("Rome"),
            Some(-3),
            Some("2000-02-29"),
            Some(false),
            Some("gold"),
            Some(12),
        ),
        (
            "Zed",
            Some("Oslo"),
            Some(10),
            Some("2000-03-01"),
            Some(true),
            Some("bronze"),
            Some(1_000),
        ),
    ];
    let mut ids = Vec::new();
    for (name, city, age, born, vip, tier, salary) in rows {
        let mut pairs = vec![("name", t(name))];
        if let Some(c) = city {
            pairs.push(("city", t(c)));
        }
        if let Some(a) = age {
            pairs.push(("age", Value::Int(a)));
        }
        if let Some(b) = born {
            pairs.push(("born", t(b)));
        }
        if let Some(v) = vip {
            pairs.push(("vip", Value::Bool(v)));
        }
        if let Some(x) = tier {
            pairs.push(("tier", t(x)));
        }
        if let Some(s) = salary {
            pairs.push(("salary", Value::Int(s)));
        }
        if name == "Ada" {
            pairs.push(("tax_id", t("T1")));
            pairs.push(("pin", t("9")));
        }
        ids.push(r.id(&r.ok(&u, "party", "create", m(pairs))));
    }
    ids
}

fn cond(field: &str, op: &str, value: Value) -> Value {
    m(vec![("field", t(field)), ("op", t(op)), ("value", value)])
}

fn names_of(r: &Rig, who: &Principal, input: Value) -> Vec<String> {
    let rows = rows_of(&r.ok(who, "party", "list", input));
    rows.iter()
        .map(|row| match row {
            Value::Map(m) => match &m["name"] {
                Value::Text(t) => t.clone(),
                other => panic!("{other:?}"),
            },
            _ => panic!(),
        })
        .collect()
}

fn rows_of(v: &Value) -> Vec<Value> {
    match v {
        Value::List(l) => l.clone(),
        other => panic!("not a list: {other:?}"),
    }
}

fn filter(conds: Vec<Value>) -> Value {
    m(vec![("filter", Value::List(conds))])
}

fn q(r: &Rig, conds: Vec<Value>) -> Vec<String> {
    names_of(r, &user(), filter(conds))
}

fn search_err(r: &Rig, who: &Principal, input: Value) -> String {
    r.err(who, "party", "search", input)
}

#[test]
fn list_without_options_is_unchanged_and_insertion_ordered() {
    each_backend(|r| {
        parties(r);
        assert_eq!(
            names_of(r, &user(), m(vec![])),
            ["Ada", "bob", "Cy", "Dee", "Eve", "Zed"]
        );
        // Empty option values mean "not given".
        assert_eq!(
            names_of(
                r,
                &user(),
                m(vec![
                    ("filter", Value::Null),
                    ("order", Value::Null),
                    ("limit", Value::Null)
                ])
            )
            .len(),
            6
        );
        assert_eq!(
            names_of(r, &user(), m(vec![("filter", Value::List(vec![]))])).len(),
            6
        );
    });
}

#[test]
fn every_operator_on_every_kind() {
    each_backend(|r| {
        let ids = parties(r);
        // text
        assert_eq!(q(r, vec![cond("name", "eq", t("Ada"))]), ["Ada"]);
        assert_eq!(
            q(r, vec![cond("name", "eq", t("ada"))]),
            Vec::<String>::new(),
            "eq is exact"
        );
        assert_eq!(
            q(r, vec![cond("city", "ne", t("Berlin"))]),
            ["bob", "Cy", "Dee", "Eve", "Zed"]
        );
        assert_eq!(
            q(r, vec![cond("city", "contains", t("BERL"))]),
            ["Ada", "bob"]
        );
        assert_eq!(
            q(
                r,
                vec![cond(
                    "name",
                    "in",
                    Value::List(vec![t("Cy"), t("Zed"), t("nobody")])
                )]
            ),
            ["Cy", "Zed"]
        );
        assert_eq!(
            q(r, vec![cond("city", "is_null", Value::Bool(true))]),
            ["Dee"]
        );
        assert_eq!(
            q(r, vec![cond("city", "is_null", Value::Null)]),
            ["Dee"],
            "value defaults to true"
        );
        assert_eq!(
            q(r, vec![cond("city", "is_null", Value::Bool(false))]).len(),
            5
        );
        assert_eq!(
            q(r, vec![cond("city", "gt", t("Paris"))]),
            ["bob", "Eve"],
            "text ranges are bytewise"
        );
        // number: numeric, not lexical (9 < 10 < 36 < 100, and negatives)
        assert_eq!(
            q(r, vec![cond("age", "gt", Value::Int(9))]),
            ["Ada", "Cy", "Zed"]
        );
        assert_eq!(
            q(r, vec![cond("age", "gte", Value::Int(10))]),
            ["Ada", "Cy", "Zed"]
        );
        assert_eq!(
            q(r, vec![cond("age", "lt", Value::Int(10))]),
            ["bob", "Eve"]
        );
        assert_eq!(
            q(r, vec![cond("age", "lte", Value::Int(10))]),
            ["bob", "Eve", "Zed"]
        );
        assert_eq!(q(r, vec![cond("age", "eq", Value::Int(-3))]), ["Eve"]);
        assert_eq!(
            q(
                r,
                vec![cond(
                    "age",
                    "in",
                    Value::List(vec![Value::Int(9), Value::Int(100)])
                )]
            ),
            ["bob", "Cy"]
        );
        assert_eq!(
            q(r, vec![cond("age", "ne", Value::Int(9))]),
            ["Ada", "Cy", "Dee", "Eve", "Zed"],
            "ne keeps empty values"
        );
        // money lives in a field the user can read here: chronological/numeric as well
        // date: chronological
        assert_eq!(
            q(r, vec![cond("born", "lt", t("2000-03-01"))]),
            ["Ada", "Cy", "Eve"]
        );
        assert_eq!(
            q(
                r,
                vec![
                    cond("born", "gte", t("2000-02-29")),
                    cond("born", "lte", t("2015-01-02"))
                ]
            ),
            ["bob", "Eve", "Zed"]
        );
        assert_eq!(q(r, vec![cond("born", "eq", t("1924-12-31"))]), ["Cy"]);
        // bool
        assert_eq!(
            q(r, vec![cond("vip", "eq", Value::Bool(true))]),
            ["Ada", "Dee", "Zed"]
        );
        assert_eq!(
            q(r, vec![cond("vip", "ne", Value::Bool(true))]),
            ["bob", "Cy", "Eve"]
        );
        assert_eq!(
            q(r, vec![cond("vip", "is_null", Value::Bool(true))]),
            ["Cy"]
        );
        // select: equality, membership, and ranges by option order
        assert_eq!(q(r, vec![cond("tier", "eq", t("gold"))]), ["Ada", "Eve"]);
        assert_eq!(
            q(
                r,
                vec![cond(
                    "tier",
                    "in",
                    Value::List(vec![t("bronze"), t("silver")])
                )]
            ),
            ["bob", "Cy", "Zed"]
        );
        assert_eq!(
            q(r, vec![cond("tier", "gte", t("silver"))]),
            ["Ada", "Cy", "Eve"]
        );
        assert_eq!(q(r, vec![cond("tier", "lt", t("silver"))]), ["bob", "Zed"]);
        // id
        assert_eq!(q(r, vec![cond("id", "eq", t(&ids[2]))]), ["Cy"]);
        assert_eq!(
            q(
                r,
                vec![cond("id", "in", Value::List(vec![t(&ids[0]), t(&ids[5])]))]
            ),
            ["Ada", "Zed"]
        );
        // conditions are ANDed
        assert_eq!(
            q(
                r,
                vec![
                    cond("vip", "eq", Value::Bool(true)),
                    cond("age", "gt", Value::Int(20))
                ]
            ),
            ["Ada"]
        );
        // An empty `in` matches nothing; a contained empty string matches everything with a value.
        assert!(q(r, vec![cond("name", "in", Value::List(vec![]))]).is_empty());
        assert_eq!(q(r, vec![cond("city", "contains", t(""))]).len(), 5);
    });
}

#[test]
fn ref_filters_and_the_operators_that_do_not_apply() {
    each_backend(|r| {
        let u = user();
        let mv = r.id(&r.ok(&u, "move", "create", m(vec![("journal", t("J"))])));
        let p = r.id(&r.ok(
            &u,
            "party",
            "create",
            m(vec![("name", t("R")), ("move_ref", t(&mv))]),
        ));
        r.ok(&u, "party", "create", m(vec![("name", t("S"))]));
        assert_eq!(q(r, vec![cond("move_ref", "eq", t(&mv))]), ["R"]);
        assert_eq!(
            q(r, vec![cond("move_ref", "is_null", Value::Bool(true))]),
            ["S"]
        );
        let _ = p;
        for (f, op, v, want) in [
            (
                "move_ref",
                "contains",
                t("x"),
                "contains only works on text",
            ),
            ("move_ref", "gt", t("x"), "does not apply to ref"),
            ("vip", "lt", Value::Bool(true), "does not apply to bool"),
            ("age", "contains", t("1"), "contains only works on text"),
            ("name", "like", t("a"), "unknown op"),
            ("name", "eq", Value::Null, "use is_null"),
            ("name", "eq", Value::Int(1), "must be a string"),
            ("age", "eq", t("9"), "must be an integer"),
            ("age", "eq", Value::Bool(true), "must be an integer"),
            ("born", "eq", t("2023-02-29"), "real calendar date"),
            ("born", "eq", t("1/1/2020"), "real calendar date"),
            ("tier", "eq", t("platinum"), "one of: bronze, silver, gold"),
            ("vip", "eq", t("true"), "true or false"),
            ("name", "in", t("Ada"), "needs an array"),
            ("name", "contains", Value::Int(1), "needs a string"),
            ("name", "is_null", t("yes"), "true or false"),
        ] {
            let e = search_err(r, &u, filter(vec![cond(f, op, v)]));
            assert!(e.contains(want), "{f} {op}: {e}");
        }
        // Structure errors.
        for (input, want) in [
            (m(vec![("filter", t("x"))]), "filter must be an array"),
            (filter(vec![t("x")]), "each filter must be an object"),
            (filter(vec![m(vec![("op", t("eq"))])]), "needs a field"),
            (filter(vec![m(vec![("field", t("name"))])]), "needs an op"),
            (
                filter(vec![m(vec![
                    ("field", t("name")),
                    ("op", t("eq")),
                    ("value", t("x")),
                    ("extra", t("y")),
                ])]),
                "unknown filter key",
            ),
            (m(vec![("order", t("x"))]), "order must be an array"),
            (
                m(vec![("order", Value::List(vec![m(vec![])]))]),
                "needs a field",
            ),
            (
                m(vec![(
                    "order",
                    Value::List(vec![m(vec![("field", t("name")), ("dir", t("up"))])]),
                )]),
                "dir must be",
            ),
            (m(vec![("limit", t("5"))]), "limit must be an integer"),
            (m(vec![("limit", Value::Int(0))]), "from 1 to 5000"),
            (m(vec![("limit", Value::Int(5001))]), "from 1 to 5000"),
            (m(vec![("limit", Value::Int(-1))]), "from 1 to 5000"),
            (
                m(vec![("offset", Value::Int(-1))]),
                "offset must be zero or more",
            ),
            (m(vec![("offset", t("1"))]), "offset must be an integer"),
            (m(vec![("limt", Value::Int(1))]), "unknown option"),
        ] {
            let e = search_err(r, &u, input);
            assert!(e.contains(want), "{want}: {e}");
        }
        // The same checks apply to list.
        let e = r.err(&u, "party", "list", m(vec![("limit", Value::Int(5001))]));
        assert!(e.contains("from 1 to 5000"), "{e}");
    });
}

#[test]
fn ordering_is_typed_stable_and_puts_empty_values_last() {
    each_backend(|r| {
        let ids = parties(r);
        let ord = |f: &str, dir: &str| {
            names_of(
                r,
                &user(),
                m(vec![(
                    "order",
                    Value::List(vec![m(vec![("field", t(f)), ("dir", t(dir))])]),
                )]),
            )
        };
        assert_eq!(
            ord("age", "asc"),
            ["Eve", "bob", "Zed", "Ada", "Cy", "Dee"],
            "numeric, empty last"
        );
        assert_eq!(
            ord("age", "desc"),
            ["Cy", "Ada", "Zed", "bob", "Eve", "Dee"],
            "empty last either way"
        );
        assert_eq!(
            ord("born", "asc"),
            ["Cy", "Ada", "Eve", "Zed", "bob", "Dee"],
            "chronological"
        );
        assert_eq!(
            ord("name", "asc"),
            ["Ada", "bob", "Cy", "Dee", "Eve", "Zed"],
            "text ignores case"
        );
        assert_eq!(
            ord("name", "desc"),
            ["Zed", "Eve", "Dee", "Cy", "bob", "Ada"]
        );
        assert_eq!(
            ord("tier", "asc"),
            ["bob", "Zed", "Cy", "Ada", "Eve", "Dee"],
            "select by option order, ties by insertion"
        );
        assert_eq!(
            ord("vip", "asc"),
            ["bob", "Eve", "Ada", "Dee", "Zed", "Cy"],
            "false before true"
        );
        let mut by_id: Vec<(String, &str)> = ids
            .iter()
            .cloned()
            .zip(["Ada", "bob", "Cy", "Dee", "Eve", "Zed"])
            .collect();
        by_id.sort();
        assert_eq!(
            ord("id", "asc"),
            by_id.iter().map(|x| x.1).collect::<Vec<_>>()
        );
        // Several keys; dir defaults to asc.
        let two = names_of(
            r,
            &user(),
            m(vec![(
                "order",
                Value::List(vec![
                    m(vec![("field", t("tier")), ("dir", t("desc"))]),
                    m(vec![("field", t("age"))]),
                ]),
            )]),
        );
        assert_eq!(two, ["Eve", "Ada", "Cy", "bob", "Zed", "Dee"]);
    });
}

#[test]
fn search_returns_rows_and_the_total_before_paging() {
    each_backend(|r| {
        parties(r);
        let s = |input: Value| -> (Vec<String>, i64) {
            let Value::Map(v) = r.ok(&user(), "party", "search", input) else {
                panic!()
            };
            let Value::Int(total) = v["total"] else {
                panic!()
            };
            let names = rows_of(&v["rows"])
                .iter()
                .map(|x| match x {
                    Value::Map(m) => match &m["name"] {
                        Value::Text(t) => t.clone(),
                        _ => panic!(),
                    },
                    _ => panic!(),
                })
                .collect();
            (names, total)
        };
        let by_age = Value::List(vec![m(vec![("field", t("age"))])]);
        assert_eq!(
            s(m(vec![])),
            (
                ["Ada", "bob", "Cy", "Dee", "Eve", "Zed"]
                    .map(String::from)
                    .to_vec(),
                6
            )
        );
        let page = |limit: i64, offset: i64| {
            s(m(vec![
                ("order", by_age.clone()),
                ("limit", Value::Int(limit)),
                ("offset", Value::Int(offset)),
            ]))
        };
        assert_eq!(page(2, 0), (vec!["Eve".into(), "bob".into()], 6));
        assert_eq!(page(2, 2), (vec!["Zed".into(), "Ada".into()], 6));
        assert_eq!(page(2, 4), (vec!["Cy".into(), "Dee".into()], 6));
        assert_eq!(
            page(2, 6),
            (vec![], 6),
            "past the end: no rows, total intact"
        );
        assert_eq!(page(2, 600), (vec![], 6));
        assert_eq!(page(5000, 0).1, 6, "the maximum limit is accepted");
        // The total counts matches only.
        let (rows, total) = s(m(vec![
            (
                "filter",
                Value::List(vec![cond("vip", "eq", Value::Bool(true))]),
            ),
            ("limit", Value::Int(1)),
        ]));
        assert_eq!((rows, total), (vec!["Ada".into()], 3));
        // `list` accepts the paging too and keeps returning a bare array.
        assert_eq!(
            names_of(
                r,
                &user(),
                m(vec![("limit", Value::Int(2)), ("offset", Value::Int(1))])
            ),
            ["bob", "Cy"]
        );
        // Nothing matches.
        assert_eq!(
            s(filter(vec![cond("name", "eq", t("nobody"))])),
            (vec![], 0)
        );
    });
}

#[test]
fn names_that_are_injection_attempts_are_just_unknown_fields() {
    each_backend(|r| {
        parties(r);
        let u = user();
        for bad in [
            "name; DROP TABLE obop_record",
            "name' OR '1'='1",
            "data->>'name'",
            "name\"",
            "1",
            "",
            "NAME",
            "name ",
            "__proto__",
            "$where",
        ] {
            let e = search_err(r, &u, filter(vec![cond(bad, "eq", t("x"))]));
            assert_eq!(e, format!("unknown field {}.{bad}", r.model("party")));
            let e = search_err(
                r,
                &u,
                m(vec![(
                    "order",
                    Value::List(vec![m(vec![("field", t(bad))])]),
                )]),
            );
            assert_eq!(e, format!("unknown field {}.{bad}", r.model("party")));
        }
        // Hostile values are data: they match nothing and break nothing.
        for v in [
            "'; DELETE FROM obop_record; --",
            "%",
            "_",
            "\\",
            "\u{1F600}",
            "{\"a\":1}",
        ] {
            assert!(q(r, vec![cond("name", "eq", t(v))]).is_empty());
            assert!(q(r, vec![cond("name", "contains", t(v))]).is_empty());
        }
        // A NUL cannot be stored in PostgreSQL and fails a statement that binds one, so no
        // backend takes it in a filter value (D76).
        for op in ["eq", "contains"] {
            let e = search_err(r, &u, filter(vec![cond("name", op, t("\0"))]));
            assert_eq!(
                e,
                format!(
                    "filter on {}.name: text cannot contain NUL",
                    r.model("party")
                )
            );
        }
        // LIKE wildcards are not wildcards.
        assert!(q(r, vec![cond("name", "contains", t("%"))]).is_empty());
        assert!(q(r, vec![cond("name", "contains", t("_"))]).is_empty());
        assert_eq!(r.store.count(&r.model("party")), 6);
    });
}

#[test]
fn filtering_and_sorting_on_fields_the_caller_cannot_read_is_refused_like_a_missing_field() {
    each_backend(|r| {
        parties(r);
        let missing = format!("unknown field {}.nonexistent", r.model("party"));
        let e = search_err(r, &user(), filter(vec![cond("nonexistent", "eq", t("x"))]));
        assert_eq!(e, missing);
        let same = |field: &str| format!("unknown field {}.{field}", r.model("party"));

        // 1. A field in a group the caller lacks (salary needs t.hr): users are refused...
        let u = user();
        for input in [
            filter(vec![cond("salary", "gt", Value::Int(10))]),
            filter(vec![cond("salary", "is_null", Value::Bool(true))]),
            m(vec![(
                "order",
                Value::List(vec![m(vec![("field", t("salary"))])]),
            )]),
        ] {
            assert_eq!(search_err(r, &u, input.clone()), same("salary"));
            assert_eq!(r.err(&u, "party", "list", input), same("salary"));
        }
        // ...and the response never names the salary field either.
        let rows = rows_of(&r.ok(&u, "party", "list", m(vec![])));
        assert!(
            rows.iter()
                .all(|x| matches!(x, Value::Map(m) if !m.contains_key("salary")))
        );
        // A holder of the group may use it, with numeric comparison.
        let hr = Principal::user("hr", &[U, "t.hr"]);
        assert_eq!(
            names_of(r, &hr, filter(vec![cond("salary", "gt", Value::Int(10))])),
            ["Ada", "Eve", "Zed"]
        );
        assert_eq!(
            names_of(
                r,
                &hr,
                m(vec![(
                    "order",
                    Value::List(vec![m(vec![("field", t("salary")), ("dir", t("desc"))])])
                )])
            ),
            ["Ada", "Zed", "Eve", "bob", "Cy", "Dee"]
        );

        // 2. A secret field is withheld from everybody, so nobody may probe it.
        for who in [
            u.clone(),
            hr.clone(),
            Principal::user("root", &["platform.admin"]),
        ] {
            assert_eq!(
                search_err(r, &who, filter(vec![cond("pin", "eq", t("1234"))])),
                same("pin")
            );
        }

        // 3. An agent sees personal data only as tokens: filtering on it would be an oracle.
        let agent = Principal::agent("bot", &[U], &[U]);
        assert_eq!(
            search_err(r, &agent, filter(vec![cond("name", "eq", t("Ada"))])),
            same("name")
        );
        assert_eq!(
            search_err(r, &agent, filter(vec![cond("name", "contains", t("A"))])),
            same("name")
        );
        assert_eq!(
            search_err(
                r,
                &agent,
                m(vec![(
                    "order",
                    Value::List(vec![m(vec![("field", t("name"))])])
                )])
            ),
            same("name")
        );
        //    ...also through a masked token restored by the dispatcher.
        let mut vault = Vault::new();
        let token = {
            let Value::List(rows) =
                r.d.call(
                    Surface::Mcp,
                    &agent,
                    &mut vault,
                    &r.cmd("party", "list"),
                    &m(vec![]),
                )
                .unwrap()
                .value
            else {
                panic!()
            };
            let Value::Map(first) = &rows[0] else {
                panic!()
            };
            let Value::Text(tok) = &first["name"] else {
                panic!()
            };
            tok.clone()
        };
        let probe = r.d.call(
            Surface::Mcp,
            &agent,
            &mut vault,
            &r.cmd("party", "search"),
            &filter(vec![cond("name", "eq", t(&token))]),
        );
        assert_eq!(probe, Err(CallError::Handler(same("name"))));
        //    ...and sensitive classes are denied to agents altogether.
        assert_eq!(
            search_err(
                r,
                &agent,
                filter(vec![cond("tax_id", "is_null", Value::Bool(true))])
            ),
            same("tax_id")
        );
        // Non-personal fields remain usable by the agent, and the id too.
        assert_eq!(
            names_of_masked(r, &agent, filter(vec![cond("age", "gt", Value::Int(20))])).len(),
            2
        );
        assert_eq!(
            names_of_masked(r, &agent, filter(vec![cond("city", "eq", t("Rome"))])).len(),
            1
        );
        // Users may filter on personal data they can read.
        assert_eq!(
            q(r, vec![cond("tax_id", "is_null", Value::Bool(false))]),
            ["Ada"]
        );
    });
}

fn names_of_masked(r: &Rig, who: &Principal, input: Value) -> Vec<Value> {
    let resp = r.call(who, "party", "list", input).unwrap();
    rows_of(&resp.value)
}

#[test]
fn search_masks_and_withholds_the_rows_it_returns() {
    each_backend(|r| {
        parties(r);
        let agent = Principal::agent("bot", &[U], &[U]);
        let resp = r
            .call(
                &agent,
                "party",
                "search",
                m(vec![
                    ("limit", Value::Int(2)),
                    ("order", Value::List(vec![m(vec![("field", t("id"))])])),
                ]),
            )
            .unwrap();
        let Value::Map(v) = &resp.value else { panic!() };
        assert_eq!(v["total"], Value::Int(6));
        let rows = rows_of(&v["rows"]);
        assert_eq!(rows.len(), 2);
        for row in rows {
            let Value::Map(row) = row else { panic!() };
            let Value::Text(name) = &row["name"] else {
                panic!()
            };
            assert!(name.starts_with("NAME_"), "{name}");
            assert!(
                !row.contains_key("tax_id")
                    && !row.contains_key("salary")
                    && !row.contains_key("pin")
            );
        }
        assert!(
            resp.denied
                .contains(&format!("{}.tax_id", r.model("party")))
        );
        assert!(
            resp.denied
                .contains(&format!("{}.salary", r.model("party")))
        );
    });
}

#[test]
fn search_is_a_generated_command_for_every_model_and_surface() {
    each_backend(|r| {
        for model in ["move", "task", "line", "party"] {
            for s in [Surface::Rest, Surface::Mcp, Surface::Cli] {
                let rows =
                    r.d.call(
                        s,
                        &user(),
                        &mut Vault::new(),
                        &r.cmd(model, "search"),
                        &m(vec![]),
                    )
                    .unwrap()
                    .value;
                assert_eq!(
                    rows,
                    m(vec![
                        ("rows", Value::List(vec![])),
                        ("total", Value::Int(0))
                    ])
                );
            }
        }
        // An unreadable model's search is as hidden as its list.
        let nobody = Principal::user("n", &["x.y"]);
        assert!(matches!(
            r.call(&nobody, "party", "search", m(vec![])),
            Err(CallError::Forbidden(_))
        ));
    });
}

#[test]
fn a_big_table_pages_correctly() {
    each_backend(|r| {
        let u = user();
        for i in 0..120 {
            r.ok(
                &u,
                "task",
                "create",
                m(vec![
                    ("title", t(&format!("t{i:03}"))),
                    ("size", Value::Int(i % 7)),
                ]),
            );
        }
        let mut seen = Vec::new();
        let mut offset = 0;
        loop {
            let Value::Map(v) = r.ok(
                &u,
                "task",
                "search",
                m(vec![
                    (
                        "filter",
                        Value::List(vec![cond("size", "lt", Value::Int(3))]),
                    ),
                    (
                        "order",
                        Value::List(vec![m(vec![("field", t("title")), ("dir", t("desc"))])]),
                    ),
                    ("limit", Value::Int(25)),
                    ("offset", Value::Int(offset)),
                ]),
            ) else {
                panic!()
            };
            let rows = rows_of(&v["rows"]);
            assert_eq!(v["total"], Value::Int(52), "sizes 0,1,2 of 120 rows");
            if rows.is_empty() {
                break;
            }
            for row in rows {
                let Value::Map(row) = row else { panic!() };
                let Value::Text(title) = &row["title"] else {
                    panic!()
                };
                seen.push(title.clone());
            }
            offset += 25;
        }
        assert_eq!(seen.len(), 52);
        let mut sorted = seen.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(seen, sorted);
    });
}

// ---- hooks that modules build on: add_guard, observe, register_read_only ---------------------

fn hooks_module() -> Module {
    Module {
        id: "hooks_t".into(),
        name: "Hooks".into(),
        models: vec![ModelDef {
            name: "hooks_item",
            description: "An item",
            read_group: "g",
            write_group: "g",
            access: None,
            fields: vec![
                F::new("name", "Name", DataClass::None, false, "g"),
                F::new("owner", "Owner", DataClass::None, false, "g"),
            ],
            commands: vec![],
            ext: Default::default(),
        }],
        network_destinations: vec![],
        description: String::new(),
        translations: Default::default(),
        meta: Default::default(),
    }
}

fn m_of(pairs: &[(&str, &str)]) -> Value {
    Value::Map(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), Value::text(v)))
            .collect(),
    )
}

#[test]
fn add_guard_runs_after_the_generated_guard_and_sees_what_it_produced() {
    use std::sync::{Arc, Mutex};
    let (d, store, m) = (Dispatcher::new(), Store::new(), hooks_module());
    d.register_crud(&m, &store);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    d.add_guard(
        "hooks_t",
        "hooks_item.list",
        false,
        Box::new(move |p, input| {
            // The generated query guard already refused bad queries; this one adds the caller's scope.
            log.lock().unwrap().push(p.id.clone());
            let Value::Map(mut mm) = input.clone() else {
                return Err("no".into());
            };
            mm.insert(
                "filter".into(),
                Value::List(vec![Value::map([
                    ("field", Value::text("owner")),
                    ("op", Value::text("eq")),
                    ("value", Value::text(&p.id)),
                ])]),
            );
            Ok(crate::Verdict {
                input: Some(Value::Map(mm)),
                ..Default::default()
            })
        }),
    );
    let u = Principal::user("ann", &["g"]);
    let mk = |name: &str, owner: &str| {
        d.call(
            Surface::Rest,
            &u,
            &mut Vault::new(),
            "hooks_item.create",
            &m_of(&[("name", name), ("owner", owner)]),
        )
        .unwrap();
    };
    mk("mine", "ann");
    mk("theirs", "bob");
    let r = d
        .call(
            Surface::Rest,
            &u,
            &mut Vault::new(),
            "hooks_item.list",
            &Value::map([]),
        )
        .unwrap();
    let Value::List(rows) = r.value else { panic!() };
    assert_eq!(rows.len(), 1, "only the caller's record");
    // The generated guard still runs first: a filter on an unknown field is refused before ours.
    let bad = Value::map([(
        "filter",
        Value::List(vec![Value::map([
            ("field", Value::text("nope")),
            ("op", Value::text("eq")),
            ("value", Value::text("x")),
        ])]),
    )]);
    assert!(
        matches!(d.call(Surface::Rest, &u, &mut Vault::new(), "hooks_item.list", &bad), Err(CallError::Handler(m)) if m.contains("unknown field"))
    );
    assert_eq!(
        seen.lock().unwrap().len(),
        1,
        "our guard did not run for the refused query"
    );
}

#[test]
fn observers_hear_about_successful_commands_only_and_go_with_their_module() {
    use std::sync::{Arc, Mutex};
    let (d, store, m) = (Dispatcher::new(), Store::new(), hooks_module());
    d.register_crud(&m, &store);
    let heard = Arc::new(Mutex::new(Vec::<String>::new()));
    let h = heard.clone();
    d.observe(
        "hooks_t",
        Box::new(move |id, _p, _input, out| {
            let Value::Map(o) = out else { return };
            h.lock().unwrap().push(format!(
                "{id}:{}",
                matches!(o.get("id"), Some(Value::Text(_)))
            ));
        }),
    );
    let u = Principal::user("ann", &["g"]);
    d.call(
        Surface::Rest,
        &u,
        &mut Vault::new(),
        "hooks_item.create",
        &m_of(&[("name", "a")]),
    )
    .unwrap();
    assert!(
        d.call(
            Surface::Rest,
            &u,
            &mut Vault::new(),
            "hooks_item.create",
            &m_of(&[("nope", "a")])
        )
        .is_err()
    );
    assert_eq!(heard.lock().unwrap().as_slice(), ["hooks_item.create:true"]);
    d.unregister_module("hooks_t");
    assert_eq!(heard.lock().unwrap().len(), 1);
}

#[test]
fn read_only_registration_exposes_get_list_and_search_and_ignores_actor_keys() {
    let (d, store, m) = (Dispatcher::new(), Store::new(), hooks_module());
    d.register_read_only_models(&m, &store, &["hooks_item"]);
    let u = Principal::user("ann", &["g"]);
    let id = store
        .insert(
            "hooks_item",
            [("name".to_string(), Value::text("a"))].into(),
        )
        .unwrap();
    let call = |c: &str, v: Value| {
        d.call(
            Surface::Rest,
            &u,
            &mut Vault::new(),
            &format!("hooks_item.{c}"),
            &v,
        )
    };
    assert!(call("get", Value::map([("id", Value::text(&id))])).is_ok());
    // Keys placed by guards (`_actor*`) are not options a client chose, so queries ignore them.
    assert!(call("list", Value::map([("_actor", Value::text("ann"))])).is_ok());
    assert!(call("search", Value::map([])).is_ok());
    for c in ["create", "update", "delete"] {
        assert!(
            matches!(call(c, Value::map([])), Err(CallError::UnknownCommand(_))),
            "{c} must not exist"
        );
    }
    assert!(!d.meta(Surface::Rest, &u)[0].models[0].can_create);
    assert!(d.model_fields("hooks_item").is_some() && d.model_def("hooks_item").is_some());
}
