//! Tests of the model engine (D56 to D58): effects and assertions, computed fields, sequences,
//! uniqueness and row-level access. Every test runs on the memory and SQLite backends, and on
//! PostgreSQL when `OBOP_TEST_POSTGRES_URL` is set.

use crate::{CallError, Dispatcher, Principal, Response, Store, Surface, Value, Vault};
use obop_registry::{
    Access, CommandDef, Effect, FieldDef as F, FieldKind, ModelDef, Module, Reset, intern,
};
use obop_types::DataClass;
use std::sync::Arc;

const U: &str = "w.user";
const MGR: &str = "w.manager";
const HR: &str = "w.hr";

struct Rig {
    d: Arc<Dispatcher>,
    store: Store,
    p: String,
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
fn user() -> Principal {
    Principal::user("ann", &[U])
}
fn manager() -> Principal {
    Principal::user("mia", &[U, MGR])
}
fn bot() -> Principal {
    Principal::agent("mia", &[U, MGR], &[U, MGR])
}

fn f(name: &'static str) -> F {
    F::new(name, "d", DataClass::None, false, U)
}
fn num(name: &'static str) -> F {
    f(name).kind(FieldKind::Number)
}
fn money(name: &'static str) -> F {
    f(name).kind(FieldKind::Money)
}

fn model(
    p: &str,
    name: &str,
    fields: Vec<F>,
    commands: Vec<CommandDef>,
    access: Option<Access>,
) -> ModelDef {
    ModelDef {
        name: intern(&format!("{p}_{name}")),
        description: "d",
        read_group: U,
        write_group: U,
        access,
        fields,
        commands,
    }
}

/// An order that, once confirmed, creates an invoice with a line per order line and takes the
/// goods out of stock; an invoice with a gapless yearly number given when it is posted.
fn workshop(p: &str) -> Module {
    let n = |s: &str| format!("{p}_{s}");
    let r = |s: &str| intern(&n(s));
    let models = vec![
        model(
            p,
            "order",
            vec![
                f("customer").required(),
                f("note"),
                money("total").computed("sum(lines.amount)"),
                num("bulk").computed("count(lines where qty >= 2)"),
                num("share").computed("if(count(lines) == 0, 0, bulk * 100 / count(lines))"),
                f("state").select(&["draft", "confirmed", "cancelled"]),
            ],
            vec![
                CommandDef::new("confirm", "Confirm", U)
                    .transition("state", Some("draft"), "confirmed")
                    .effect(
                        Effect::create(
                            &n("invoice"),
                            &[("customer", "$self.customer"), ("order_id", "$self.id")],
                        )
                        .named("inv"),
                    )
                    .effect(
                        Effect::create(
                            &n("invoice_line"),
                            &[
                                ("invoice_id", "$created.inv"),
                                ("label", "$line.label"),
                                ("qty", "$line.qty"),
                                ("price", "$line.price"),
                            ],
                        )
                        .for_each("lines"),
                    )
                    .effect(
                        Effect::adjust_match(
                            &n("stock"),
                            &[("sku", "$line.sku")],
                            "qty",
                            true,
                            "$line.qty",
                            true,
                        )
                        .for_each("lines"),
                    ),
                CommandDef::new("cancel", "Cancel", U).transition("state", None, "cancelled"),
            ],
            None,
        ),
        model(
            p,
            "order_line",
            vec![
                f("order_id").reference(&n("order")).required(),
                f("label"),
                f("sku"),
                num("qty"),
                money("price"),
                money("amount").computed("qty * price"),
            ],
            vec![],
            None,
        ),
        model(
            p,
            "invoice",
            vec![
                f("customer"),
                f("order_id").reference(&n("order")),
                f("number")
                    .sequence("INV-{year}-{seq:03}", true, Reset::Year)
                    .assign_on("post"),
                money("total").computed("sum(lines.amount)"),
                f("state").select(&["draft", "posted", "cancelled"]),
            ],
            vec![
                CommandDef::new("post", "Post", MGR)
                    .approval()
                    .transition("state", Some("draft"), "posted")
                    .assertion_msg("total > 0", "an invoice needs a positive total"),
                CommandDef::new("cancel", "Cancel", U).transition(
                    "state",
                    Some("draft"),
                    "cancelled",
                ),
            ],
            None,
        ),
        model(
            p,
            "invoice_line",
            vec![
                f("invoice_id").reference(&n("invoice")).required(),
                f("label"),
                num("qty"),
                money("price"),
                money("amount").computed("qty * price"),
            ],
            vec![],
            None,
        ),
        model(
            p,
            "stock",
            vec![f("sku").unique().required(), num("qty")],
            vec![],
            None,
        ),
        model(
            p,
            "tag",
            vec![f("name").unique_with(&["company"]), f("company")],
            vec![],
            None,
        ),
        model(
            p,
            "ticket",
            vec![
                f("number").sequence("T-{seq}", false, Reset::Never),
                f("subject"),
            ],
            vec![],
            None,
        ),
        model(
            p,
            "leave",
            vec![
                f("employee"),
                num("days"),
                f("state").select(&["requested", "approved"]),
            ],
            vec![CommandDef::new("approve", "Approve", HR).transition(
                "state",
                Some("requested"),
                "approved",
            )],
            Some(Access {
                own: Some("employee"),
                see_all: vec![HR],
                ..Access::default()
            }),
        ),
        // A shipment: marking it shipped marks its order lines' invoice posted, which needs a
        // manager and approval, and fails on purpose when the note says so.
        model(
            p,
            "batch",
            vec![
                f("order_id").reference(&n("order")),
                f("invoice_id").reference(&n("invoice")),
                f("state").select(&["open", "closed"]),
            ],
            vec![
                CommandDef::new("close", "Close", U)
                    .transition("state", Some("open"), "closed")
                    .effect(Effect::set_state_ref(&n("invoice"), "invoice_id", "post")),
            ],
            None,
        ),
    ];
    let _ = r;
    Module {
        id: p.into(),
        name: "Workshop".into(),
        models,
        ..Module::default()
    }
}

fn each_rig(f: impl Fn(&Rig)) {
    let mut urls = vec!["memory:".to_string(), "sqlite::memory:".to_string()];
    if let Ok(u) = std::env::var("OBOP_TEST_POSTGRES_URL") {
        urls.push(u);
    }
    for url in urls {
        let p = format!("w{}", &uuid::Uuid::now_v7().simple().to_string()[22..]);
        let store = Store::open(&url).unwrap();
        let d = Arc::new(Dispatcher::new());
        let module = workshop(&p);
        let v = obop_registry::check_module(&module);
        assert!(v.is_empty(), "{v:?}");
        d.register_crud(&module, &store);
        for mo in &module.models {
            for c in &mo.commands {
                d.register_transition(&module, mo.name, c.name, &store);
            }
        }
        f(&Rig { d, store, p });
    }
}

impl Rig {
    fn d_now(&self) -> i64 {
        (self.d.clock.0.read().unwrap())()
    }
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
    fn err(&self, who: &Principal, model: &str, c: &str, input: Value) -> String {
        match self.call(who, model, c, input) {
            Err(CallError::Handler(m)) => m,
            other => panic!("{model}.{c}: expected a handler error, got {other:?}"),
        }
    }
    fn id(v: &Value) -> String {
        let Value::Map(m) = v else { panic!("{v:?}") };
        let Value::Text(t) = &m["id"] else { panic!() };
        t.clone()
    }
    fn make(&self, who: &Principal, model: &str, pairs: Vec<(&str, Value)>) -> String {
        Self::id(&self.ok(who, model, "create", m(pairs)))
    }
    fn get(&self, who: &Principal, model: &str, id: &str) -> Value {
        self.ok(who, model, "get", by_id(id))
    }
    fn fld(&self, who: &Principal, model: &str, id: &str, field: &str) -> Value {
        match self.get(who, model, id) {
            Value::Map(mut m) => m.remove(field).unwrap_or(Value::Null),
            _ => panic!(),
        }
    }
    fn list(&self, who: &Principal, model: &str, input: Value) -> Vec<Value> {
        match self.ok(who, model, "list", input) {
            Value::List(l) => l,
            other => panic!("{other:?}"),
        }
    }
    fn search_total(&self, who: &Principal, model: &str, input: Value) -> i64 {
        match self.ok(who, model, "search", input) {
            Value::Map(mut m) => match m.remove("total") {
                Some(Value::Int(n)) => n,
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }
    fn count(&self, model: &str) -> usize {
        self.store.count(&format!("{}_{model}", self.p))
    }
    /// An order with lines `(sku, qty, price)`.
    fn order(&self, lines: &[(&str, i64, i64)]) -> String {
        let o = self.make(&user(), "order", vec![("customer", t("Acme"))]);
        for (sku, qty, price) in lines {
            self.make(
                &user(),
                "order_line",
                vec![
                    ("order_id", t(&o)),
                    ("label", t(&format!("item {sku}"))),
                    ("sku", t(sku)),
                    ("qty", Value::Int(*qty)),
                    ("price", Value::Int(*price)),
                ],
            );
        }
        o
    }
}

fn int(v: Value) -> i64 {
    match v {
        Value::Int(n) => n,
        other => panic!("not a number: {other:?}"),
    }
}

// ---- computed fields ----------------------------------------------------------------------

#[test]
fn totals_follow_the_lines_and_are_read_only() {
    each_rig(|r| {
        let o = r.order(&[("a", 2, 500), ("b", 1, 250)]);
        assert_eq!(int(r.fld(&user(), "order", &o, "total")), 1250);
        assert_eq!(
            int(r.fld(&user(), "order", &o, "bulk")),
            1,
            "only the line with qty 2"
        );
        assert_eq!(
            int(r.fld(&user(), "order", &o, "share")),
            50,
            "a share of the lines, in percent"
        );
        let lines = r.list(
            &user(),
            "order_line",
            m(vec![(
                "filter",
                Value::List(vec![m(vec![
                    ("field", t("order_id")),
                    ("op", t("eq")),
                    ("value", t(&o)),
                ])]),
            )]),
        );
        assert_eq!(lines.len(), 2);
        let Value::Map(first) = &lines[0] else {
            panic!()
        };
        assert_eq!(
            first["amount"],
            Value::Int(1000),
            "a line's amount is computed too"
        );
        // Changing a line changes the total; so does adding and deleting one.
        let l1 = Rig::id(&lines[0]);
        r.ok(
            &user(),
            "order_line",
            "update",
            m(vec![("id", t(&l1)), ("qty", Value::Int(3))]),
        );
        assert_eq!(int(r.fld(&user(), "order", &o, "total")), 1750);
        r.ok(&user(), "order_line", "delete", by_id(&l1));
        assert_eq!(int(r.fld(&user(), "order", &o, "total")), 250);
        // An order without lines has a total of zero.
        let empty = r.make(&user(), "order", vec![("customer", t("Z"))]);
        assert_eq!(int(r.fld(&user(), "order", &empty, "total")), 0);
        // Writes to a computed field are refused; sending the current value back is a no-op.
        let e = r.err(
            &user(),
            "order",
            "update",
            m(vec![("id", t(&o)), ("total", Value::Int(1))]),
        );
        assert!(e.contains("total is computed and read-only"), "{e}");
        let e = r.err(
            &user(),
            "order",
            "create",
            m(vec![("customer", t("x")), ("total", Value::Int(1))]),
        );
        assert!(e.contains("computed and read-only"), "{e}");
        r.ok(
            &user(),
            "order",
            "update",
            m(vec![
                ("id", t(&o)),
                ("total", Value::Int(250)),
                ("note", t("n")),
            ]),
        );
        assert_eq!(r.fld(&user(), "order", &o, "note"), t("n"));
    });
}

#[test]
fn computed_fields_can_be_filtered_sorted_and_counted() {
    each_rig(|r| {
        let small = r.order(&[("a", 1, 100)]);
        let big = r.order(&[("a", 10, 100)]);
        let mid = r.order(&[("a", 5, 100)]);
        let filter = |op: &str, v: i64| {
            m(vec![(
                "filter",
                Value::List(vec![m(vec![
                    ("field", t("total")),
                    ("op", t(op)),
                    ("value", Value::Int(v)),
                ])]),
            )])
        };
        let ids = |rows: Vec<Value>| -> Vec<String> { rows.iter().map(Rig::id).collect() };
        assert_eq!(
            ids(r.list(&user(), "order", filter("gte", 500))),
            [big.clone(), mid.clone()]
        );
        assert_eq!(r.search_total(&user(), "order", filter("lt", 500)), 1);
        let sorted = r.list(
            &user(),
            "order",
            m(vec![(
                "order",
                Value::List(vec![m(vec![("field", t("total")), ("dir", t("desc"))])]),
            )]),
        );
        assert_eq!(ids(sorted), [big, mid, small]);
    });
}

#[test]
fn computed_values_reach_the_get_after_create_and_update_responses() {
    each_rig(|r| {
        let o = r.order(&[("a", 2, 50)]);
        let Value::Map(row) = r.ok(
            &user(),
            "order",
            "update",
            m(vec![("id", t(&o)), ("note", t("x"))]),
        ) else {
            panic!()
        };
        assert_eq!(row["total"], Value::Int(100));
    });
}

// ---- sequences ----------------------------------------------------------------------------

fn confirmed_invoice(r: &Rig, lines: &[(&str, i64, i64)]) -> String {
    let o = r.order(lines);
    r.ok(&user(), "order", "confirm", by_id(&o));
    let inv = r.list(&user(), "invoice", m(vec![]));
    let found = inv
        .iter()
        .find(|row| matches!(row, Value::Map(m) if m.get("order_id") == Some(&t(&o))))
        .expect("invoice created");
    Rig::id(found)
}

fn number_of(r: &Rig, inv: &str) -> Value {
    r.fld(&manager(), "invoice", inv, "number")
}

#[test]
fn posting_numbers_invoices_without_gaps_and_agents_only_propose() {
    each_rig(|r| {
        let year = crate::engine::civil(r.d_now()).year;
        let a = confirmed_invoice(r, &[("a", 1, 100)]);
        let b = confirmed_invoice(r, &[("a", 1, 200)]);
        assert_eq!(number_of(r, &a), Value::Null, "no number before posting");
        // The agent only proposes; nothing is numbered until a human approves.
        let plan = match r.call(&bot(), "invoice", "post", by_id(&b)) {
            Err(CallError::ApprovalRequired { plan_id }) => plan_id,
            other => panic!("{other:?}"),
        };
        assert_eq!(number_of(r, &b), Value::Null);
        r.ok(&manager(), "invoice", "post", by_id(&a));
        assert_eq!(number_of(r, &a), t(&format!("INV-{year}-001")));
        r.d.approve(&manager(), &mut Vault::new(), &plan).unwrap();
        assert_eq!(number_of(r, &b), t(&format!("INV-{year}-002")));
        // The number is read-only and a numbered invoice cannot be deleted.
        let e = r.err(
            &manager(),
            "invoice",
            "update",
            m(vec![("id", t(&a)), ("number", t("X"))]),
        );
        assert!(e.contains("number the system assigns"), "{e}");
        let e = r.err(&manager(), "invoice", "delete", by_id(&a));
        assert!(e.contains("gapless number"), "{e}");
        // An unnumbered draft can still be deleted.
        let c = r.make(&user(), "invoice", vec![("customer", t("x"))]);
        r.ok(&manager(), "invoice", "delete", by_id(&c));
    });
}

#[test]
fn a_failed_posting_does_not_consume_a_number() {
    each_rig(|r| {
        let year = crate::engine::civil(r.d_now()).year;
        // An invoice with no total fails its assertion after nothing was written.
        let empty = r.make(&user(), "invoice", vec![("customer", t("x"))]);
        let e = r.err(&manager(), "invoice", "post", by_id(&empty));
        assert!(e.contains("an invoice needs a positive total"), "{e}");
        let a = confirmed_invoice(r, &[("a", 1, 100)]);
        r.ok(&manager(), "invoice", "post", by_id(&a));
        assert_eq!(
            number_of(r, &a),
            t(&format!("INV-{year}-001")),
            "the failure used no number"
        );
        // A batch closing posts its invoice through an effect; when that fails the batch stays
        // open, and again no number is used.
        let bad = r.make(&user(), "invoice", vec![("customer", t("y"))]);
        let batch = r.make(&user(), "batch", vec![("invoice_id", t(&bad))]);
        let e = r.err(&manager(), "batch", "close", by_id(&batch));
        assert!(
            e.contains("failed, nothing was changed") && e.contains("positive total"),
            "{e}"
        );
        assert_eq!(r.fld(&manager(), "batch", &batch, "state"), t("open"));
        let b = confirmed_invoice(r, &[("a", 1, 300)]);
        r.ok(&manager(), "invoice", "post", by_id(&b));
        assert_eq!(number_of(r, &b), t(&format!("INV-{year}-002")));
    });
}

#[test]
fn numbers_restart_each_year_when_asked_to() {
    each_rig(|r| {
        let at = Arc::new(std::sync::atomic::AtomicI64::new(1_798_000_000)); // December 2026
        let c = at.clone();
        r.d.set_clock(Arc::new(move || {
            c.load(std::sync::atomic::Ordering::SeqCst)
        }));
        let a = confirmed_invoice(r, &[("a", 1, 1)]);
        let b = confirmed_invoice(r, &[("a", 1, 1)]);
        r.ok(&manager(), "invoice", "post", by_id(&a));
        r.ok(&manager(), "invoice", "post", by_id(&b));
        assert_eq!(number_of(r, &b), t("INV-2026-002"));
        at.store(1_800_000_000, std::sync::atomic::Ordering::SeqCst); // January 2027
        let c = confirmed_invoice(r, &[("a", 1, 1)]);
        r.ok(&manager(), "invoice", "post", by_id(&c));
        assert_eq!(number_of(r, &c), t("INV-2027-001"));
    });
}

#[test]
fn numbers_given_at_creation_are_assigned_by_create() {
    each_rig(|r| {
        let a = r.make(&user(), "ticket", vec![("subject", t("a"))]);
        let b = r.make(&user(), "ticket", vec![("subject", t("b"))]);
        assert_eq!(r.fld(&user(), "ticket", &a, "number"), t("T-1"));
        assert_eq!(r.fld(&user(), "ticket", &b, "number"), t("T-2"));
        let e = r.err(&user(), "ticket", "create", m(vec![("number", t("T-9"))]));
        assert!(e.contains("number the system assigns"), "{e}");
    });
}

#[test]
fn backfill_numbers_old_records_in_order_and_only_once() {
    each_rig(|r| {
        let seed = |model: &str, pairs: Vec<(&str, Value)>| {
            r.store
                .insert(
                    &format!("{}_{model}", r.p),
                    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
                )
                .unwrap()
        };
        let a = seed("ticket", vec![("subject", t("a"))]);
        let b = seed("ticket", vec![("subject", t("b"))]);
        // One draft and one posted invoice, written straight into the store.
        let draft = seed("invoice", vec![("state", t("draft"))]);
        let posted = seed("invoice", vec![("state", t("posted"))]);
        let done = r.d.backfill_sequences(&r.store, None, None).unwrap();
        let count = |m: &str, f: &str| {
            done.iter()
                .find(|(mm, ff, _)| mm == &format!("{}_{m}", r.p) && ff == f)
                .map(|x| x.2)
        };
        assert_eq!(count("ticket", "number"), Some(2));
        assert_eq!(
            count("invoice", "number"),
            Some(1),
            "only the posted invoice"
        );
        assert_eq!(r.fld(&user(), "ticket", &a, "number"), t("T-1"));
        assert_eq!(r.fld(&user(), "ticket", &b, "number"), t("T-2"));
        assert_eq!(r.fld(&manager(), "invoice", &draft, "number"), Value::Null);
        let year = crate::engine::civil(r.d_now()).year;
        assert_eq!(number_of(r, &posted), t(&format!("INV-{year}-001")));
        // A second run has nothing to do, and a new ticket continues the series.
        let again =
            r.d.backfill_sequences(&r.store, Some(&format!("{}_ticket", r.p)), None)
                .unwrap();
        assert_eq!(again[0].2, 0);
        let c = r.make(&user(), "ticket", vec![("subject", t("c"))]);
        assert_eq!(r.fld(&user(), "ticket", &c, "number"), t("T-3"));
    });
}

#[test]
fn parallel_postings_get_distinct_gapless_numbers() {
    each_rig(|r| {
        let invoices: Vec<String> = (0..16)
            .map(|i| confirmed_invoice(r, &[("a", 1, 100 + i)]))
            .collect();
        std::thread::scope(|s| {
            for inv in &invoices {
                let (d, name) = (r.d.clone(), r.cmd("invoice", "post"));
                s.spawn(move || {
                    d.call(
                        Surface::Rest,
                        &manager(),
                        &mut Vault::new(),
                        &name,
                        &by_id(inv),
                    )
                    .unwrap();
                });
            }
        });
        let mut got: Vec<String> = invoices
            .iter()
            .map(|i| match number_of(r, i) {
                Value::Text(t) => t,
                other => panic!("{other:?}"),
            })
            .collect();
        got.sort();
        let year = crate::engine::civil(r.d_now()).year;
        let want: Vec<String> = (1..=16).map(|n| format!("INV-{year}-{n:03}")).collect();
        assert_eq!(got, want);
    });
}

// ---- uniqueness ---------------------------------------------------------------------------

#[test]
fn unique_values_are_refused_with_a_clear_message() {
    each_rig(|r| {
        let a = r.make(
            &user(),
            "stock",
            vec![("sku", t("X-1")), ("qty", Value::Int(1))],
        );
        let e = r.err(&user(), "stock", "create", m(vec![("sku", t("X-1"))]));
        assert!(
            e.contains("stock.sku must be unique") && e.contains("\"X-1\""),
            "{e}"
        );
        let b = r.make(&user(), "stock", vec![("sku", t("X-2"))]);
        let e = r.err(
            &user(),
            "stock",
            "update",
            m(vec![("id", t(&b)), ("sku", t("X-1"))]),
        );
        assert!(e.contains("must be unique"), "{e}");
        // Saving a record with its own value is fine, and so is changing another field.
        r.ok(
            &user(),
            "stock",
            "update",
            m(vec![
                ("id", t(&a)),
                ("sku", t("X-1")),
                ("qty", Value::Int(5)),
            ]),
        );
        // Freed values can be used again.
        r.ok(&user(), "stock", "delete", by_id(&a));
        r.make(&user(), "stock", vec![("sku", t("X-1"))]);
    });
}

#[test]
fn unique_with_scopes_the_constraint() {
    each_rig(|r| {
        r.make(
            &user(),
            "tag",
            vec![("name", t("vip")), ("company", t("A"))],
        );
        r.make(
            &user(),
            "tag",
            vec![("name", t("vip")), ("company", t("B"))],
        );
        let e = r.err(
            &user(),
            "tag",
            "create",
            m(vec![("name", t("vip")), ("company", t("A"))]),
        );
        assert!(
            e.contains("tag.name must be unique") && e.contains("same company"),
            "{e}"
        );
        // No company is a scope of its own; empty names never collide.
        r.make(&user(), "tag", vec![("name", t("vip"))]);
        assert!(
            r.call(&user(), "tag", "create", m(vec![("name", t("vip"))]))
                .is_err()
        );
        r.make(&user(), "tag", vec![("name", t(""))]);
        r.make(&user(), "tag", vec![("name", t(""))]);
        r.make(&user(), "tag", vec![("company", t("A"))]);
        r.make(&user(), "tag", vec![("company", t("A"))]);
    });
}

#[test]
fn of_parallel_creates_with_one_value_exactly_one_wins() {
    each_rig(|r| {
        let wins = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|s| {
            for i in 0..12 {
                let (d, name, wins) = (r.d.clone(), r.cmd("stock", "create"), &wins);
                s.spawn(move || {
                    let input = m(vec![("sku", t("SAME")), ("qty", Value::Int(i))]);
                    if d.call(Surface::Rest, &user(), &mut Vault::new(), &name, &input)
                        .is_ok()
                    {
                        wins.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    }
                });
            }
        });
        assert_eq!(wins.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(r.count("stock"), 1);
    });
}

// ---- effects and assertions ---------------------------------------------------------------

#[test]
fn confirming_an_order_creates_the_invoice_with_its_lines_and_moves_stock() {
    each_rig(|r| {
        let o = r.order(&[("a", 2, 500), ("b", 3, 100)]);
        r.make(
            &manager(),
            "stock",
            vec![("sku", t("a")), ("qty", Value::Int(10))],
        );
        let before_lines = r.count("invoice_line");
        let out = r.ok(&user(), "order", "confirm", by_id(&o));
        let Value::Map(out) = out else { panic!() };
        assert_eq!(out["state"], t("confirmed"));
        assert_eq!(r.count("invoice_line") - before_lines, 2);
        let inv = r.list(&user(), "invoice", m(vec![]));
        assert_eq!(inv.len(), 1);
        let Value::Map(inv) = &inv[0] else { panic!() };
        assert_eq!(inv["customer"], t("Acme"));
        assert_eq!(inv["state"], t("draft"));
        assert_eq!(
            inv["total"],
            Value::Int(1300),
            "the total sums the copied lines"
        );
        // Stock: a was 10, now 8; b did not exist and went to -3.
        let stock = r.list(&user(), "stock", m(vec![]));
        let qty = |sku: &str| -> i64 {
            stock
                .iter()
                .find_map(|s| match s {
                    Value::Map(m) if m["sku"] == t(sku) => Some(int(m["qty"].clone())),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!((qty("a"), qty("b")), (8, -3));
        // Confirming again is refused by the state, and creates nothing more.
        assert!(r.call(&user(), "order", "confirm", by_id(&o)).is_err());
        assert_eq!(r.count("invoice"), 1);
    });
}

#[test]
fn an_action_changes_everything_or_nothing() {
    each_rig(|r| {
        // The second line has no sku, so the third effect (stock) cannot find its record, after
        // the invoice and its lines were created and the first line took stock.
        let o = r.order(&[("a", 1, 100), ("", 1, 100)]);
        let (invoices, lines, stocks) = (
            r.count("invoice"),
            r.count("invoice_line"),
            r.count("stock"),
        );
        let e = r.err(&user(), "order", "confirm", by_id(&o));
        assert!(
            e.contains("effect 3 (adjust) failed, nothing was changed"),
            "{e}"
        );
        assert!(
            e.contains("the value to match") && e.contains("is empty"),
            "{e}"
        );
        assert_eq!(r.fld(&user(), "order", &o, "state"), t("draft"));
        assert_eq!(
            (
                r.count("invoice"),
                r.count("invoice_line"),
                r.count("stock")
            ),
            (invoices, lines, stocks),
            "the invoice and its lines and the stock row for `a` were rolled back"
        );
    });
}

#[test]
fn effects_never_exceed_the_callers_permissions() {
    each_rig(|r| {
        let o = r.order(&[("a", 1, 100)]);
        // Without the write group of invoice the whole action is refused before it runs.
        let nobody = Principal::user("nobody", &[]);
        assert!(matches!(
            r.call(&nobody, "order", "confirm", by_id(&o)),
            Err(CallError::Forbidden(_))
        ));
        assert_eq!(r.fld(&user(), "order", &o, "state"), t("draft"));
    });
}

#[test]
fn an_effect_that_runs_an_approval_action_makes_an_agent_propose_everything() {
    each_rig(|r| {
        let inv = confirmed_invoice(r, &[("a", 1, 100)]);
        let batch = r.make(&user(), "batch", vec![("invoice_id", t(&inv))]);
        // The batch's own action needs no approval, but it posts the invoice, which does.
        let plan = match r.call(&bot(), "batch", "close", by_id(&batch)) {
            Err(CallError::ApprovalRequired { plan_id }) => plan_id,
            other => panic!("{other:?}"),
        };
        assert_eq!(r.fld(&manager(), "batch", &batch, "state"), t("open"));
        assert_eq!(r.fld(&manager(), "invoice", &inv, "state"), t("draft"));
        // The approver must hold the group of the action the effect starts.
        let plain = Principal::user("pat", &[U]);
        assert!(matches!(
            r.d.approve(&plain, &mut Vault::new(), &plan),
            Err(CallError::Forbidden(_))
        ));
        r.d.approve(&manager(), &mut Vault::new(), &plan).unwrap();
        assert_eq!(r.fld(&manager(), "batch", &batch, "state"), t("closed"));
        assert_eq!(r.fld(&manager(), "invoice", &inv, "state"), t("posted"));
        assert_ne!(
            number_of(r, &inv),
            Value::Null,
            "the nested action numbered the invoice"
        );
        // A user without the manager group cannot press the batch button at all.
        let inv2 = confirmed_invoice(r, &[("a", 1, 100)]);
        let batch2 = r.make(&user(), "batch", vec![("invoice_id", t(&inv2))]);
        let e = r.err(&user(), "batch", "close", by_id(&batch2));
        assert!(e.contains("which needs the w.manager group"), "{e}");
    });
}

#[test]
fn an_assertion_stops_the_action_with_its_message() {
    each_rig(|r| {
        let inv = r.make(&user(), "invoice", vec![("customer", t("x"))]);
        let e = r.err(&manager(), "invoice", "post", by_id(&inv));
        assert_eq!(
            e,
            "an invoice needs a positive total (assertion total > 0 failed)"
        );
        assert_eq!(r.fld(&manager(), "invoice", &inv, "state"), t("draft"));
        r.make(
            &user(),
            "invoice_line",
            vec![
                ("invoice_id", t(&inv)),
                ("qty", Value::Int(1)),
                ("price", Value::Int(9)),
            ],
        );
        r.ok(&manager(), "invoice", "post", by_id(&inv));
        // The implicit transition through update runs the same checks.
        let inv2 = r.make(&user(), "invoice", vec![]);
        let e = r.err(
            &manager(),
            "invoice",
            "update",
            m(vec![("id", t(&inv2)), ("state", t("posted"))]),
        );
        assert!(e.contains("positive total"), "{e}");
    });
}

#[test]
fn too_many_children_stop_an_action_cleanly() {
    each_rig(|r| {
        let o = r.make(&user(), "order", vec![("customer", t("big"))]);
        let rows: Vec<(String, obop_storage::Fields)> = (0..501)
            .map(|i| {
                let mut f = obop_storage::Fields::new();
                f.insert("order_id".into(), serde_json::json!(o));
                f.insert("sku".into(), serde_json::json!(format!("s{i}")));
                f.insert("qty".into(), serde_json::json!(1));
                (uuid::Uuid::now_v7().to_string(), f)
            })
            .collect();
        for (id, f) in &rows {
            r.store
                .backend()
                .insert(&format!("{}_order_line", r.p), id, f)
                .unwrap();
        }
        let e = r.err(&user(), "order", "confirm", by_id(&o));
        assert!(e.contains("at most 500"), "{e}");
        assert_eq!(r.fld(&user(), "order", &o, "state"), t("draft"));
        assert_eq!(r.count("invoice"), 0);
    });
}

// ---- row-level access ---------------------------------------------------------------------

fn emp(name: &str) -> Principal {
    Principal::user(name, &[U])
}
fn hr() -> Principal {
    Principal::user("hanna", &[U, HR])
}

#[test]
fn people_see_and_change_only_their_own_records() {
    each_rig(|r| {
        let mine = r.make(&emp("eve"), "leave", vec![("days", Value::Int(2))]);
        let his = r.make(&emp("bob"), "leave", vec![("days", Value::Int(5))]);
        assert_eq!(
            r.fld(&emp("eve"), "leave", &mine, "employee"),
            t("eve"),
            "own defaults to the creator"
        );
        // Lists, searches and counts show only what the caller may see.
        assert_eq!(r.list(&emp("eve"), "leave", m(vec![])).len(), 1);
        assert_eq!(r.search_total(&emp("eve"), "leave", m(vec![])), 1);
        assert_eq!(r.list(&hr(), "leave", m(vec![])).len(), 2);
        assert_eq!(r.search_total(&hr(), "leave", m(vec![])), 2);
        // A record of somebody else answers exactly like one that does not exist.
        let ghost = r.err(&emp("eve"), "leave", "get", by_id("no-such-id"));
        assert_eq!(
            r.err(&emp("eve"), "leave", "get", by_id(&his)),
            ghost.replace("no-such-id", &his)
        );
        for c in ["update", "delete"] {
            let e = r.err(
                &emp("eve"),
                "leave",
                c,
                m(vec![("id", t(&his)), ("days", Value::Int(9))]),
            );
            assert_eq!(e, format!("no {}_leave {his}", r.p), "{c}");
        }
        assert_eq!(int(r.fld(&hr(), "leave", &his, "days")), 5);
        // Actions go through the same door; the manager of the group may press them.
        let e = r.err(&hr(), "leave", "approve", by_id("ghost"));
        assert!(e.contains("no "), "{e}");
        r.ok(&hr(), "leave", "approve", by_id(&his));
        // Filtering for somebody else's id finds nothing, and the total does not count it.
        let filter = m(vec![(
            "filter",
            Value::List(vec![m(vec![
                ("field", t("employee")),
                ("op", t("eq")),
                ("value", t("bob")),
            ])]),
        )]);
        assert_eq!(r.search_total(&emp("eve"), "leave", filter.clone()), 0);
        assert_eq!(r.search_total(&hr(), "leave", filter), 1);
        // The administrator sees everything.
        assert_eq!(
            r.list(
                &Principal::user("root", &[crate::ADMIN_GROUP]),
                "leave",
                m(vec![])
            )
            .len(),
            2
        );
    });
}

#[test]
fn a_record_cannot_be_created_for_or_moved_to_someone_else_by_a_person_who_cannot_see_it() {
    each_rig(|r| {
        let e = r.err(
            &emp("eve"),
            "leave",
            "create",
            m(vec![("employee", t("bob"))]),
        );
        assert!(
            e.contains("would not be visible to you") && e.contains("set employee to yourself"),
            "{e}"
        );
        let mine = r.make(&emp("eve"), "leave", vec![]);
        let e = r.err(
            &emp("eve"),
            "leave",
            "update",
            m(vec![("id", t(&mine)), ("employee", t("bob"))]),
        );
        assert!(e.contains("would not be visible to you"), "{e}");
        // HR may file one for somebody else.
        let theirs = r.make(&hr(), "leave", vec![("employee", t("bob"))]);
        assert_eq!(r.list(&emp("bob"), "leave", m(vec![])).len(), 1);
        assert_eq!(r.fld(&emp("bob"), "leave", &theirs, "employee"), t("bob"));
    });
}

#[test]
fn an_agent_sees_what_its_owner_sees_within_its_scopes() {
    each_rig(|r| {
        r.make(&emp("eve"), "leave", vec![]);
        r.make(&emp("bob"), "leave", vec![]);
        let agent = Principal::agent("eve", &[U], &[U]);
        assert_eq!(
            r.list(&agent, "leave", m(vec![])).len(),
            1,
            "the owner's records"
        );
        // An agent for HR whose scopes leave HR out gets no more than a plain user would.
        let narrow = Principal::agent("hanna", &[U, HR], &[U]);
        assert_eq!(
            r.list(&narrow, "leave", m(vec![])).len(),
            0,
            "hanna owns none and HR is out of scope"
        );
        let wide = Principal::agent("hanna", &[U, HR], &[U, HR]);
        assert_eq!(r.list(&wide, "leave", m(vec![])).len(), 2);
    });
}

/// Property: for random people, records and filters, list, search and get agree with a naive
/// oracle that applies the same rule by hand.
#[test]
fn row_level_access_matches_a_naive_oracle_for_random_principals() {
    each_rig(|r| {
        let people = ["ann", "bob", "cy", "dee"];
        // A tiny xorshift: no dependency, reproducible.
        let mut s: u64 = 0x2545_F491_4F6C_DD1D;
        let mut rnd = |n: u64| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s % n
        };
        let mut oracle: Vec<(String, String, i64)> = Vec::new(); // id, employee, days
        for _ in 0..40 {
            let who = people[rnd(4) as usize];
            let days = rnd(10) as i64;
            let id = r.make(
                &hr(),
                "leave",
                vec![("employee", t(who)), ("days", Value::Int(days))],
            );
            oracle.push((id, who.to_string(), days));
        }
        let mut principals: Vec<(Principal, Option<&str>)> =
            people.iter().map(|p| (emp(p), Some(*p))).collect();
        principals.push((hr(), None));
        principals.push((Principal::agent("ann", &[U], &[U]), Some("ann")));
        principals.push((Principal::user("zed", &[U]), Some("zed")));
        for (p, owner) in &principals {
            for _ in 0..6 {
                let min = rnd(10) as i64;
                let filter = m(vec![(
                    "filter",
                    Value::List(vec![m(vec![
                        ("field", t("days")),
                        ("op", t("gte")),
                        ("value", Value::Int(min)),
                    ])]),
                )]);
                let page = m(vec![
                    ("limit", Value::Int(7)),
                    ("offset", Value::Int(rnd(5) as i64)),
                    (
                        "filter",
                        match &filter {
                            Value::Map(f) => f["filter"].clone(),
                            _ => unreachable!(),
                        },
                    ),
                ]);
                let seen: Vec<&(String, String, i64)> = oracle
                    .iter()
                    .filter(|(_, e, d)| *d >= min && (owner.is_none_or(|o| o == e)))
                    .collect();
                assert_eq!(
                    r.search_total(p, "leave", filter.clone()),
                    seen.len() as i64,
                    "{p:?} total"
                );
                let got: Vec<String> = r.list(p, "leave", filter).iter().map(Rig::id).collect();
                assert_eq!(
                    got,
                    seen.iter().map(|x| x.0.clone()).collect::<Vec<_>>(),
                    "{p:?} rows"
                );
                let _ = page;
            }
            for (id, e, _) in &oracle {
                let visible = owner.is_none_or(|o| o == e);
                assert_eq!(
                    r.call(p, "leave", "get", by_id(id)).is_ok(),
                    visible,
                    "{p:?} get {id}"
                );
            }
        }
    });
}

#[test]
fn refs_to_records_you_cannot_see_read_like_refs_to_nothing_and_ids_do_not_leak() {
    // The batch refers to invoices; `leave` has the rules, so use a small module of our own.
    let p = format!("a{}", &uuid::Uuid::now_v7().simple().to_string()[22..]);
    let n = |s: &str| format!("{p}_{s}");
    let secret = model(
        &p,
        "secret",
        vec![f("owner"), f("body")],
        vec![],
        Some(Access {
            own: Some("owner"),
            ..Access::default()
        }),
    );
    let pointer = model(
        &p,
        "pointer",
        vec![f("secret_id").reference(&n("secret"))],
        vec![],
        None,
    );
    let module = Module {
        id: p.clone(),
        name: "A".into(),
        models: vec![secret, pointer],
        ..Module::default()
    };
    assert!(obop_registry::check_module(&module).is_empty());
    let store = Store::new();
    let d = Dispatcher::new();
    d.register_crud(&module, &store);
    let go =
        |who: &Principal, c: &str, i: Value| d.call(Surface::Rest, who, &mut Vault::new(), c, &i);
    let Ok(Response {
        value: Value::Map(s),
        ..
    }) = go(
        &emp("bob"),
        &format!("{}.create", n("secret")),
        m(vec![("body", t("hush"))]),
    )
    else {
        panic!()
    };
    let Value::Text(sid) = s["id"].clone() else {
        panic!()
    };
    let make = |who: &Principal, id: &str| {
        go(
            who,
            &format!("{}.create", n("pointer")),
            m(vec![("secret_id", t(id))]),
        )
    };
    let msg = |r: Result<Response, CallError>| match r {
        Err(CallError::Handler(m)) => m,
        other => panic!("{other:?}"),
    };
    let e_real = msg(make(&emp("eve"), &sid));
    let e_fake = msg(make(&emp("eve"), "does-not-exist"));
    assert_eq!(
        e_real.replace(&sid, "X"),
        e_fake.replace("does-not-exist", "X")
    );
    assert!(make(&emp("bob"), &sid).is_ok(), "the owner may point at it");
    // Deleting the secret is refused, and the pointer's id is not named for somebody who cannot
    // see the pointer's model rules (here the pointer is visible, so it is named to its owner).
    let e = msg(go(
        &emp("bob"),
        &format!("{}.delete", n("secret")),
        by_id(&sid),
    ));
    assert!(e.contains("still referenced by"), "{e}");
}
