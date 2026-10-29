//! Field limits (D90) through the dispatcher: `max_length` and `min_length` of text and longtext
//! (in characters, not bytes), the default length of a text field, `min` and `max` of number,
//! money and decimal (exact), on create, update, `bulk_create` (the row is named), the effects of
//! an action, and the rule for a record stored above a limit that was lowered later. Every test
//! runs on the memory and SQLite backends, and on PostgreSQL when `OBOP_TEST_POSTGRES_URL` is set.

use crate::{CallError, Dispatcher, Principal, Response, Store, Surface, Value, Vault, errors};
use obop_registry::{
    Bound, CommandDef, DEFAULT_LONGTEXT_MAX, DEFAULT_TEXT_MAX, Effect, FieldDef as F, FieldKind,
    Limits, ModelDef, Module, intern,
};
use obop_types::DataClass;
use obop_values::Decimal;
use std::sync::Arc;

const U: &str = "lim.user";
const HR: &str = "lim.hr";

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
fn list(v: &Value) -> &[Value] {
    match v {
        Value::List(l) => l,
        other => panic!("not a list: {other:?}"),
    }
}
fn dec(s: &str, scale: u8) -> Bound {
    Bound::Dec(Decimal::parse_for(s, 12, scale, None).unwrap())
}

struct Rig {
    d: Arc<Dispatcher>,
    p: String,
    who: Principal,
    store: Store,
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
        name: "L".into(),
        models: vec![
            model(
                n("doc"),
                vec![
                    // Declared: 2 to 10 characters.
                    f("title").with_limits(Limits {
                        min_length: Some(2),
                        max_length: Some(10),
                        ..Default::default()
                    }),
                    // Nothing declared: the default of the kind applies.
                    f("plain"),
                    f("body").kind(FieldKind::LongText),
                    // A longtext with a much lower limit than its default.
                    f("tiny").kind(FieldKind::LongText).with_limits(Limits {
                        max_length: Some(5),
                        ..Default::default()
                    }),
                    f("qty").kind(FieldKind::Number).with_limits(Limits {
                        min: Some(Bound::Int(0)),
                        max: Some(Bound::Int(100)),
                        ..Default::default()
                    }),
                    f("price").kind(FieldKind::Money).with_limits(Limits {
                        min: Some(Bound::Int(0)),
                        max: Some(Bound::Int(100_000)),
                        ..Default::default()
                    }),
                    f("rate").decimal(12, 7).with_limits(Limits {
                        min: Some(dec("0.1000000", 7)),
                        max: Some(dec("0.9000000", 7)),
                        ..Default::default()
                    }),
                    // Only a lower bound, only an upper bound.
                    f("floor").kind(FieldKind::Number).with_limits(Limits {
                        min: Some(Bound::Int(-5)),
                        ..Default::default()
                    }),
                    f("ceiling").kind(FieldKind::Number).with_limits(Limits {
                        max: Some(Bound::Int(5)),
                        ..Default::default()
                    }),
                    f("state").select(&["new", "done"]),
                    // A person's name: an agent sees it only as a masked token.
                    F::new("person", "d", DataClass::PublicDirectory, true, U),
                ],
                vec![
                    // An effect that writes the title of the record into another model.
                    CommandDef::new("finish", "Finish", U)
                        .transition("state", Some("new"), "done")
                        .effect(Effect::create(
                            &format!("{p}_log"),
                            &[("short", "$self.plain"), ("title", "$self.title")],
                        )),
                ],
            ),
            model(
                n("log"),
                vec![
                    f("title"),
                    // At most 3 characters.
                    f("short").with_limits(Limits {
                        max_length: Some(3),
                        ..Default::default()
                    }),
                    // At most 100.
                    f("cap").kind(FieldKind::Money).with_limits(Limits {
                        max: Some(Bound::Int(100)),
                        ..Default::default()
                    }),
                    // At most 0.90.
                    f("ratio").decimal(8, 2).with_limits(Limits {
                        max: Some(dec("0.90", 2)),
                        ..Default::default()
                    }),
                ],
                vec![],
            ),
            model(
                n("payroll"),
                vec![
                    // A field of the HR group: nobody else reads it.
                    F::new("salary", "d", DataClass::None, false, HR).kind(FieldKind::Money),
                    F::new("bonus", "d", DataClass::None, false, HR).decimal(8, 2),
                    f("state").select(&["new", "paid"]),
                ],
                vec![
                    // The same with a decimal: the bonus into a field that holds at most 0.90.
                    CommandDef::new("grant", "Grant", U)
                        .transition("state", Some("new"), "paid")
                        .effect(Effect::create(
                            &format!("{p}_log"),
                            &[("ratio", "$self.bonus")],
                        )),
                    // Copies the salary, which the caller may not read, into a field that holds
                    // at most 100.
                    CommandDef::new("pay", "Pay", U)
                        .transition("state", Some("new"), "paid")
                        .effect(Effect::create(
                            &format!("{p}_log"),
                            &[("cap", "$self.salary")],
                        )),
                ],
            ),
            model(
                n("batch"),
                vec![
                    f("counter_id").reference(&format!("{p}_counter")),
                    f("state").select(&["open", "run"]),
                ],
                vec![
                    // Starts `counter.apply`, whose adjust effect meets the bound of the document.
                    CommandDef::new("run", "Run", U)
                        .transition("state", Some("open"), "run")
                        .effect(Effect::set_state_ref(
                            &format!("{p}_counter"),
                            "counter_id",
                            "apply",
                        )),
                ],
            ),
            model(
                n("counter"),
                vec![
                    f("doc_id").reference(&format!("{p}_doc")),
                    f("by").kind(FieldKind::Number),
                    f("state").select(&["open", "applied"]),
                ],
                vec![
                    // Adds `by` to the quantity of the document, which stops at 100.
                    CommandDef::new("apply", "Apply", U)
                        .transition("state", Some("open"), "applied")
                        .effect(Effect::adjust_ref(
                            &format!("{p}_doc"),
                            "doc_id",
                            "qty",
                            false,
                            "$self.by",
                        )),
                ],
            ),
        ],
        network_destinations: vec![],
        description: String::new(),
        translations: Default::default(),
        meta: Default::default(),
    }
}

fn each(f: impl Fn(&Rig)) {
    let mut urls = vec!["memory:".to_string(), "sqlite::memory:".to_string()];
    if let Ok(u) = std::env::var("OBOP_TEST_POSTGRES_URL") {
        urls.push(u);
    }
    for url in urls {
        let p = format!("l{}", &uuid::Uuid::now_v7().simple().to_string()[22..]);
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
        f(&Rig { d, p, who, store });
    }
}

impl Rig {
    fn cmd(&self, model: &str, c: &str) -> String {
        format!("{}_{model}.{c}", self.p)
    }
    fn call(&self, model: &str, c: &str, input: Value) -> Result<Response, CallError> {
        self.d.call(
            Surface::Rest,
            &self.who,
            &mut Vault::new(),
            &self.cmd(model, c),
            &input,
        )
    }
    /// A person who holds the HR group (reads and writes the salary).
    fn hr(&self) -> Principal {
        Principal::user(&format!("hr-{}", self.p), &[U, HR])
    }
    /// An agent acting for a person with the ordinary group; it sees personal data as tokens.
    fn bot(&self) -> Principal {
        Principal::agent(&format!("bot-{}", self.p), &[U], &[U])
    }
    fn call_as(
        &self,
        who: &Principal,
        surface: Surface,
        vault: &mut Vault,
        model: &str,
        c: &str,
        input: Value,
    ) -> Result<Response, CallError> {
        self.d
            .call(surface, who, vault, &self.cmd(model, c), &input)
    }
    /// The handler error of a call as `who` over `surface` with `vault`.
    fn err_as(
        &self,
        who: &Principal,
        surface: Surface,
        vault: &mut Vault,
        model: &str,
        c: &str,
        input: Value,
    ) -> String {
        match self.call_as(who, surface, vault, model, c, input) {
            Err(CallError::Handler(m)) => m,
            other => panic!("{model}.{c}: expected a handler error, got {other:?}"),
        }
    }
    fn ok(&self, model: &str, c: &str, input: Value) -> Value {
        match self.call(model, c, input) {
            Ok(r) => r.value,
            Err(e) => panic!("{model}.{c} failed: {e:?}"),
        }
    }
    fn err(&self, model: &str, c: &str, input: Value) -> String {
        match self.call(model, c, input) {
            Err(CallError::Handler(m)) => m,
            other => panic!("{model}.{c}: expected a handler error, got {other:?}"),
        }
    }
    fn create(&self, model: &str, pairs: Vec<(&str, Value)>) -> String {
        text(get(&self.ok(model, "create", m(pairs)), "id"))
    }
    fn refused(&self, model: &str, pairs: Vec<(&str, Value)>) -> String {
        self.err(model, "create", m(pairs))
    }
    fn update(&self, id: &str, pairs: Vec<(&str, Value)>) -> Result<Response, CallError> {
        let mut all = vec![("id", t(id))];
        all.extend(pairs);
        self.call("doc", "update", m(all))
    }
    fn read(&self, model: &str, id: &str) -> Value {
        self.ok(model, "get", m(vec![("id", t(id))]))
    }
    fn count(&self, model: &str) -> usize {
        self.store.count(&format!("{}_{model}", self.p))
    }
    /// Write a field of a stored record behind the commands' back, as it was before a limit was
    /// lowered, or by a package version that had none.
    fn corrupt(&self, model: &str, id: &str, field: &str, value: serde_json::Value) {
        let patch = [(field.to_string(), value)].into_iter().collect();
        assert!(
            self.store
                .backend()
                .update(&format!("{}_{model}", self.p), id, &patch)
                .unwrap()
        );
    }
}

/// `limit_violated: <model>.<field> <what>`: the code, the field and the numbers.
fn assert_limit(msg: &str, field: &str, what: &str) {
    assert!(
        msg.starts_with("limit_violated: "),
        "not a limit_violated refusal: {msg}"
    );
    assert!(
        msg.contains(&format!(".{field} must be {what}")),
        "wanted `.{field} must be {what}` in: {msg}"
    );
}

#[test]
fn the_default_lengths_are_the_documented_constants() {
    assert_eq!(DEFAULT_TEXT_MAX, 2_048);
    assert_eq!(DEFAULT_LONGTEXT_MAX, 100_000);
}

#[test]
fn a_text_is_counted_in_characters_not_bytes() {
    each(|r| {
        // Ten characters of two bytes each, and ten of four bytes: both fit a limit of 10.
        r.create("doc", vec![("title", t(&"å".repeat(10)))]);
        r.create("doc", vec![("title", t(&"\u{1F600}".repeat(10)))]);
        // Eleven do not, however short they are in bytes for ASCII.
        for (s, n) in [("å", 11), ("\u{1F600}", 11), ("a", 11)] {
            let e = r.refused("doc", vec![("title", t(&s.repeat(n)))]);
            assert_limit(&e, "title", &format!("at most 10 characters; this has {n}"));
        }
        // A combining sequence is several characters: e + U+0301 is two.
        let e = r.refused("doc", vec![("title", t(&"e\u{301}".repeat(6)))]);
        assert_limit(&e, "title", "at most 10 characters; this has 12");
        assert_eq!(r.count("doc"), 2);
    });
}

#[test]
fn the_limit_is_inclusive_at_both_ends_and_a_message_names_the_field_and_the_numbers() {
    each(|r| {
        for ok in ["ab", "abcdefghij"] {
            r.create("doc", vec![("title", t(ok))]);
        }
        let e = r.refused("doc", vec![("title", t("abcdefghijk"))]);
        assert_limit(&e, "title", "at most 10 characters; this has 11");
        assert!(e.contains(&format!("{}_doc.title", r.p)), "{e}");
        let e = r.refused("doc", vec![("title", t("a"))]);
        assert_limit(&e, "title", "at least 2 characters; this has 1");
        // An empty string is a value, and too short; an absent field is not checked.
        let e = r.refused("doc", vec![("title", t(""))]);
        assert_limit(&e, "title", "at least 2 characters; this has 0");
        r.create("doc", vec![("plain", t("no title at all"))]);
        // The singular reads as a singular.
        assert_eq!(r.count("doc"), 3);
    });
}

#[test]
fn whitespace_counts_as_characters_and_nothing_is_trimmed() {
    each(|r| {
        // Two spaces satisfy min_length 2: the value is counted as sent (D90 says so plainly).
        let id = r.create("doc", vec![("title", t("  "))]);
        assert_eq!(text(get(&r.read("doc", &id), "title")), "  ");
        // Ten characters with a trailing space fit; eleven do not.
        r.create("doc", vec![("title", t("abcdefghi "))]);
        let e = r.refused("doc", vec![("title", t("abcdefghi  "))]);
        assert_limit(&e, "title", "at most 10 characters; this has 11");
    });
}

#[test]
fn a_text_field_without_a_declared_limit_has_the_default() {
    each(|r| {
        r.create(
            "doc",
            vec![("plain", t(&"x".repeat(DEFAULT_TEXT_MAX as usize)))],
        );
        let e = r.refused(
            "doc",
            vec![("plain", t(&"x".repeat(DEFAULT_TEXT_MAX as usize + 1)))],
        );
        assert_limit(
            &e,
            "plain",
            &format!(
                "at most {DEFAULT_TEXT_MAX} characters; this has {}",
                DEFAULT_TEXT_MAX + 1
            ),
        );
        // 2,048 characters of four bytes are 8 KiB and still fit: the count is of characters.
        r.create(
            "doc",
            vec![("plain", t(&"\u{1F600}".repeat(DEFAULT_TEXT_MAX as usize)))],
        );
        assert_eq!(r.count("doc"), 2);
    });
}

#[test]
fn a_longtext_field_has_its_own_larger_default_and_a_declared_limit_can_be_lower() {
    each(|r| {
        r.create(
            "doc",
            vec![("body", t(&"x".repeat(DEFAULT_LONGTEXT_MAX as usize)))],
        );
        let e = r.refused(
            "doc",
            vec![("body", t(&"x".repeat(DEFAULT_LONGTEXT_MAX as usize + 1)))],
        );
        assert_limit(
            &e,
            "body",
            &format!(
                "at most {DEFAULT_LONGTEXT_MAX} characters; this has {}",
                DEFAULT_LONGTEXT_MAX + 1
            ),
        );
        r.create("doc", vec![("tiny", t("12345"))]);
        let e = r.refused("doc", vec![("tiny", t("123456"))]);
        assert_limit(&e, "tiny", "at most 5 characters; this has 6");
        assert_eq!(r.count("doc"), 2);
    });
}

#[test]
fn number_money_and_decimal_hold_their_inclusive_bounds() {
    each(|r| {
        for (field, ok, below, above) in [
            ("qty", [0i64, 100], -1i64, 101i64),
            ("price", [0, 100_000], -1, 100_001),
        ] {
            for v in ok {
                r.create("doc", vec![(field, Value::Int(v))]);
            }
            let (lo, hi) = (ok[0], ok[1]);
            let e = r.refused("doc", vec![(field, Value::Int(below))]);
            assert_limit(&e, field, &format!("at least {lo}; this is {below}"));
            let e = r.refused("doc", vec![(field, Value::Int(above))]);
            assert_limit(&e, field, &format!("at most {hi}; this is {above}"));
        }
        // One bound only.
        r.create(
            "doc",
            vec![("floor", Value::Int(-5)), ("ceiling", Value::Int(5))],
        );
        r.create(
            "doc",
            vec![
                ("floor", Value::Int(i64::MAX)),
                ("ceiling", Value::Int(i64::MIN)),
            ],
        );
        assert_limit(
            &r.refused("doc", vec![("floor", Value::Int(-6))]),
            "floor",
            "at least -5; this is -6",
        );
        assert_limit(
            &r.refused("doc", vec![("ceiling", Value::Int(6))]),
            "ceiling",
            "at most 5; this is 6",
        );
        assert_eq!(r.count("doc"), 6);
    });
}

#[test]
fn a_decimal_bound_is_exact_to_the_last_digit() {
    each(|r| {
        for ok in ["0.1000000", "0.1000001", "0.5", "0.9000000"] {
            r.create("doc", vec![("rate", t(ok))]);
        }
        let e = r.refused("doc", vec![("rate", t("0.0999999"))]);
        assert_limit(&e, "rate", "at least 0.1000000; this is 0.0999999");
        let e = r.refused("doc", vec![("rate", t("0.9000001"))]);
        assert_limit(&e, "rate", "at most 0.9000000; this is 0.9000001");
        // Written another way: the canonical form is compared, not the text.
        let e = r.refused("doc", vec![("rate", t("1"))]);
        assert_limit(&e, "rate", "at most 0.9000000; this is 1.0000000");
        let e = r.refused("doc", vec![("rate", t("-0.1"))]);
        assert_limit(&e, "rate", "at least 0.1000000; this is -0.1000000");
        assert_eq!(r.count("doc"), 4);
    });
}

#[test]
fn a_refusal_leaves_the_store_alone() {
    each(|r| {
        let id = r.create("doc", vec![("title", t("first")), ("qty", Value::Int(1))]);
        let before = r.read("doc", &id);
        // A create with one bad field among good ones writes nothing.
        let e = r.refused("doc", vec![("title", t("fine")), ("qty", Value::Int(1000))]);
        assert_limit(&e, "qty", "at most 100; this is 1000");
        assert_eq!(r.count("doc"), 1);
        // An update with one bad field among good ones changes nothing.
        let Err(CallError::Handler(e)) = r.update(
            &id,
            vec![("title", t("changed")), ("qty", Value::Int(1000))],
        ) else {
            panic!("the update should have been refused")
        };
        assert_limit(&e, "qty", "at most 100; this is 1000");
        assert_eq!(r.read("doc", &id), before);
    });
}

#[test]
fn update_holds_a_changed_value_to_the_limit() {
    each(|r| {
        let id = r.create("doc", vec![("title", t("first"))]);
        let Err(CallError::Handler(e)) = r.update(&id, vec![("title", t("abcdefghijk"))]) else {
            panic!("refused expected")
        };
        assert_limit(&e, "title", "at most 10 characters; this has 11");
        let Err(CallError::Handler(e)) = r.update(&id, vec![("title", t("x"))]) else {
            panic!("refused expected")
        };
        assert_limit(&e, "title", "at least 2 characters; this has 1");
        r.update(&id, vec![("title", t("second"))]).unwrap();
        assert_eq!(text(get(&r.read("doc", &id), "title")), "second");
    });
}

#[test]
fn a_record_stored_above_a_lowered_limit_stays_editable_in_its_other_fields() {
    each(|r| {
        let id = r.create("doc", vec![("title", t("first")), ("qty", Value::Int(5))]);
        // Before the limits existed these were fine.
        r.corrupt(
            "doc",
            &id,
            "title",
            serde_json::json!("a title far longer than ten"),
        );
        r.corrupt("doc", &id, "qty", serde_json::json!(5000));
        r.corrupt(
            "doc",
            &id,
            "tiny",
            serde_json::json!("much longer than five"),
        );

        // Another field can be changed ...
        r.update(&id, vec![("plain", t("edited"))]).unwrap();
        // ... also by a form that sends the whole record back, unchanged fields included.
        r.update(
            &id,
            vec![
                ("title", t("a title far longer than ten")),
                ("qty", Value::Int(5000)),
                ("tiny", t("much longer than five")),
                ("plain", t("edited again")),
            ],
        )
        .unwrap();
        let row = r.read("doc", &id);
        assert_eq!(text(get(&row, "plain")), "edited again");
        assert_eq!(text(get(&row, "title")), "a title far longer than ten");

        // A changed value is held to the limit, even when it is still above it.
        let Err(CallError::Handler(e)) =
            r.update(&id, vec![("title", t("a title far longer than eleven"))])
        else {
            panic!("refused expected")
        };
        assert_limit(&e, "title", "at most 10 characters; this has 30");
        let Err(CallError::Handler(e)) = r.update(&id, vec![("qty", Value::Int(5001))]) else {
            panic!("refused expected")
        };
        assert_limit(&e, "qty", "at most 100; this is 5001");
        // Going back inside the limit is always possible.
        r.update(&id, vec![("title", t("short")), ("qty", Value::Int(7))])
            .unwrap();
        // The other checks still run on an unchanged value: a stored kind error is still one.
        r.corrupt("doc", &id, "qty", serde_json::json!("not a number"));
        assert!(r.update(&id, vec![("qty", t("not a number"))]).is_err());
    });
}

#[test]
fn bulk_create_names_the_row_and_the_limit_without_the_error_code() {
    each(|r| {
        let rows = Value::List(vec![
            m(vec![("title", t("fine"))]),
            m(vec![("title", t("abcdefghijk"))]),
            m(vec![("title", t("fine too")), ("qty", Value::Int(101))]),
            m(vec![("rate", t("0.0999999"))]),
        ]);
        let msg = r.err("doc", "bulk_create", m(vec![("rows", rows.clone())]));
        assert!(
            msg.contains("3 of 4 row(s) are invalid") && msg.contains("nothing was created"),
            "{msg}"
        );
        assert!(
            msg.contains("row 2")
                && msg.contains(".title must be at most 10 characters; this has 11")
                && msg.contains("row 3")
                && msg.contains(".qty must be at most 100; this is 101")
                && msg.contains("row 4")
                && msg.contains(".rate must be at least 0.1000000; this is 0.0999999"),
            "{msg}"
        );
        assert!(
            !msg.contains("limit_violated"),
            "a row error lists the sentence, not the code: {msg}"
        );
        assert_eq!(r.count("doc"), 0);

        // Partial mode keeps the row that is fine and reports the others by number.
        let report = r.ok(
            "doc",
            "bulk_create",
            m(vec![("rows", rows.clone()), ("mode", t("partial"))]),
        );
        assert_eq!(r.count("doc"), 1);
        let errs: Vec<(Value, String)> = list(get(&report, "errors"))
            .iter()
            .map(|e| (get(e, "row").clone(), text(get(e, "error"))))
            .collect();
        assert_eq!(
            errs.iter().map(|e| e.0.clone()).collect::<Vec<_>>(),
            [Value::Int(2), Value::Int(3), Value::Int(4)]
        );
        assert!(
            errs[0]
                .1
                .ends_with("must be at most 10 characters; this has 11"),
            "{errs:?}"
        );
        assert!(
            errs.iter().all(|e| !e.1.contains("limit_violated")),
            "{errs:?}"
        );

        // A dry run reports and writes nothing.
        let report = r.ok(
            "doc",
            "bulk_create",
            m(vec![
                ("rows", rows),
                ("dry_run", Value::Bool(true)),
                ("mode", t("partial")),
            ]),
        );
        assert_eq!(get(&report, "failed"), &Value::Int(3));
        assert_eq!(r.count("doc"), 1);
    });
}

#[test]
fn csv_rows_go_through_the_same_check() {
    each(|r| {
        let csv = "title,qty\nfine,5\nabcdefghijk,5\nfine,101\n";
        let msg = r.err("doc", "bulk_create", m(vec![("csv", t(csv))]));
        assert!(
            msg.contains("row 2") && msg.contains("this has 11"),
            "{msg}"
        );
        assert!(
            msg.contains("row 3") && msg.contains("this is 101"),
            "{msg}"
        );
        assert_eq!(r.count("doc"), 0);
    });
}

#[test]
fn what_an_action_writes_is_held_to_the_limits_of_the_target() {
    each(|r| {
        // `finish` copies `plain` into `log.short`, which holds three characters.
        let fits = r.create("doc", vec![("title", t("ok")), ("plain", t("abc"))]);
        r.ok("doc", "finish", m(vec![("id", t(&fits))]));
        assert_eq!(r.count("log"), 1);

        let id = r.create("doc", vec![("title", t("long")), ("plain", t("abcd"))]);
        let e = r.err("doc", "finish", m(vec![("id", t(&id))]));
        // What an action writes is the action's, not the caller's: the refusal says what the
        // field takes and not how long the value was (D90 Decision 5).
        assert!(e.contains(".short must be at most 3 characters"), "{e}");
        assert!(!e.contains("this has"), "{e}");
        // The action did not run at all: no log, and the state did not move.
        assert_eq!(r.count("log"), 1);
        assert_eq!(text(get(&r.read("doc", &id), "state")), "new");
    });
}

#[test]
fn an_adjustment_cannot_push_a_number_past_its_bound() {
    each(|r| {
        let doc = r.create("doc", vec![("title", t("stock")), ("qty", Value::Int(90))]);
        let c1 = r.create("counter", vec![("doc_id", t(&doc)), ("by", Value::Int(10))]);
        r.ok("counter", "apply", m(vec![("id", t(&c1))]));
        assert_eq!(get(&r.read("doc", &doc), "qty"), &Value::Int(100));

        let c2 = r.create("counter", vec![("doc_id", t(&doc)), ("by", Value::Int(1))]);
        let e = r.err("counter", "apply", m(vec![("id", t(&c2))]));
        assert!(e.contains(".qty must be at most 100"), "{e}");
        assert!(!e.contains("this is"), "{e}");
        // Nothing moved: the quantity and the state are as before.
        assert_eq!(get(&r.read("doc", &doc), "qty"), &Value::Int(100));
        assert_eq!(text(get(&r.read("counter", &c2), "state")), "open");
    });
}

#[test]
fn the_refusal_is_a_400_with_its_own_code_and_a_hint() {
    each(|r| {
        let msg = r.refused("doc", vec![("title", t("abcdefghijk"))]);
        let c = errors::classify(&CallError::Handler(msg.clone()));
        assert_eq!((c.status, c.code), (400, "limit_violated"));
        // The body carries the sentence without the code, the code and the hint.
        assert_eq!(
            c.body["error"],
            msg.strip_prefix("limit_violated: ").unwrap()
        );
        assert_eq!(c.body["code"], "limit_violated");
        assert!(
            c.body["hint"].as_str().unwrap().contains("max_length"),
            "{}",
            c.body
        );
        // Every other kind of error still reads as before: no code.
        let c = errors::classify(&CallError::Handler("lim_doc.qty must be an integer".into()));
        assert_eq!((c.status, c.code), (400, "bad_request"));
        assert!(c.body.get("code").is_none(), "{}", c.body);
    });
}

#[test]
fn the_plain_text_of_a_row_error_drops_only_a_code_of_the_table() {
    assert_eq!(
        errors::plain("limit_violated: a.b must be at most 1 character; this has 2"),
        "a.b must be at most 1 character; this has 2"
    );
    assert_eq!(errors::plain("a.b is required"), "a.b is required");
    // A colon in an ordinary message is not a code.
    assert_eq!(
        errors::plain("note: a.b is required"),
        "note: a.b is required"
    );
}

// ---- the review of PR 33 -------------------------------------------------------------------

/// The answer of an action whose effect met a limit: `limit_violated` first and once, the place
/// it happened after it, and on every surface the status, code and hint of the table (F0, F8).
fn assert_effect_limit(e: &str, context: &str) {
    assert!(e.starts_with("limit_violated: "), "code not first: {e}");
    assert_eq!(e.matches("limit_violated").count(), 1, "code twice: {e}");
    assert!(e.contains(context), "wanted `{context}` in: {e}");
    let c = errors::classify(&CallError::Handler(e.to_string()));
    assert_eq!((c.status, c.code), (400, "limit_violated"), "{e}");
    assert_eq!(c.body["code"], "limit_violated");
    let hint = c.body["hint"].as_str().expect("a hint");
    assert!(hint.contains("max_length"), "{hint}");
    // The sentence of the body has no code in it, and the JSON-RPC error of /mcp carries the hint.
    let said = c.body["error"].as_str().unwrap();
    assert!(!said.contains("limit_violated"), "{said}");
    assert!(said.contains(context), "{said}");
    assert_eq!(c.hint, Some(hint));
    assert_eq!(c.rpc_data()["hint"], hint);
    assert_eq!(c.exit, 1);
}

#[test]
fn an_effect_that_meets_a_limit_keeps_the_code_and_the_hint() {
    each(|r| {
        // 1. A create effect: `finish` copies four characters into a field that holds three.
        let id = r.create("doc", vec![("title", t("long")), ("plain", t("abcd"))]);
        let e = r.err("doc", "finish", m(vec![("id", t(&id))]));
        assert_effect_limit(
            &e,
            &format!(
                "{}: effect 1 (create) failed, nothing was changed: {}_log.short must be at most 3 characters",
                r.cmd("doc", "finish"),
                r.p
            ),
        );

        // 2. An adjust effect: `apply` pushes the quantity past its bound.
        let doc = r.create("doc", vec![("title", t("stock")), ("qty", Value::Int(100))]);
        let c = r.create("counter", vec![("doc_id", t(&doc)), ("by", Value::Int(1))]);
        let e = r.err("counter", "apply", m(vec![("id", t(&c))]));
        assert_effect_limit(
            &e,
            &format!(
                "{}: effect 1 (adjust) failed, nothing was changed: {}_doc.qty must be at most 100",
                r.cmd("counter", "apply"),
                r.p
            ),
        );

        // 3. A nested `set_state`: `batch.run` starts `counter.apply`, whose adjust meets the bound.
        let b = r.create("batch", vec![("counter_id", t(&c))]);
        let e = r.err("batch", "run", m(vec![("id", t(&b))]));
        assert_effect_limit(
            &e,
            &format!(
                "{}: effect 1 (set_state) failed, nothing was changed: {}_counter.apply on the related record: {}: effect 1 (adjust) failed",
                r.cmd("batch", "run"),
                r.p,
                r.cmd("counter", "apply"),
            ),
        );
        // None of the three changed anything.
        assert_eq!(r.count("log"), 0);
        assert_eq!(get(&r.read("doc", &doc), "qty"), &Value::Int(100));
        assert_eq!(text(get(&r.read("counter", &c), "state")), "open");
        assert_eq!(text(get(&r.read("batch", &b), "state")), "open");
    });
}

#[test]
fn an_error_of_an_effect_that_is_not_a_limit_is_wrapped_as_before() {
    // Only `limit_violated` moves to the front: the wrapper of every other refusal is unchanged.
    let outer = "x.y: effect 1 (create) failed, nothing was changed";
    assert_eq!(
        errors::wrap(outer, "no lead nope"),
        format!("{outer}: no lead nope")
    );
    assert_eq!(
        errors::wrap(outer, "assertion_failed: stays where it was"),
        format!("{outer}: assertion_failed: stays where it was")
    );
    assert_eq!(
        errors::wrap(outer, "limit_violated: a.b must be at most 1 character"),
        "limit_violated: x.y: effect 1 (create) failed, nothing was changed: a.b must be at most 1 character"
    );
}

/// A name of 40 characters (an agent never reads it, only its token).
const LONG_NAME: &str = "Aleksandra Katarzyna Kowalska-Nowakowska";

#[test]
fn an_agent_is_not_told_how_long_a_value_it_only_holds_as_a_token_is() {
    assert_eq!(LONG_NAME.chars().count(), 40);
    each(|r| {
        let doc = r.create("doc", vec![("title", t("ok")), ("person", t(LONG_NAME))]);
        let bot = r.bot();
        let mut vault = Vault::new();
        // The agent reads the record and holds the name as a token.
        let row = r
            .call_as(
                &bot,
                Surface::Mcp,
                &mut vault,
                "doc",
                "get",
                m(vec![("id", t(&doc))]),
            )
            .unwrap()
            .value;
        let token = text(get(&row, "person"));
        assert_ne!(token, LONG_NAME, "the agent must hold a token");

        // It writes the token where three characters fit. The handler sees the real name, 40
        // characters; the refusal must not say so.
        let e = r.err_as(
            &bot,
            Surface::Mcp,
            &mut vault,
            "log",
            "create",
            m(vec![("short", t(&token))]),
        );
        assert_eq!(
            e,
            format!(
                "limit_violated: {}_log.short must be at most 3 characters",
                r.p
            )
        );
        // The same through update, bulk_create and a number the agent wrote itself.
        let e = r.err_as(
            &bot,
            Surface::Mcp,
            &mut vault,
            "doc",
            "update",
            m(vec![("id", t(&doc)), ("title", t(&token))]),
        );
        assert_eq!(
            e,
            format!(
                "limit_violated: {}_doc.title must be at most 10 characters",
                r.p
            )
        );
        let e = r.err_as(
            &bot,
            Surface::Mcp,
            &mut vault,
            "doc",
            "create",
            m(vec![("title", t("ok")), ("qty", Value::Int(1000))]),
        );
        assert_eq!(
            e,
            format!("limit_violated: {}_doc.qty must be at most 100", r.p)
        );
        let e = r.err_as(
            &bot,
            Surface::Mcp,
            &mut vault,
            "doc",
            "bulk_create",
            m(vec![(
                "rows",
                Value::List(vec![m(vec![("title", t(&token))])]),
            )]),
        );
        assert!(e.contains("must be at most 10 characters"), "{e}");
        assert!(!e.contains("this has") && !e.contains("40"), "{e}");
        assert_eq!(r.count("log"), 0);

        // A person who typed the value, on the other hand, is told how long it was.
        let e = r.refused("log", vec![("short", t(LONG_NAME))]);
        assert_limit(&e, "short", "at most 3 characters; this has 40");
    });
}

#[test]
fn an_action_never_states_the_value_it_copied_from_a_field_the_caller_cannot_read() {
    each(|r| {
        // The salary is in the HR group; `pay` copies it into `log.cap`, which holds at most 100.
        let hr = r.hr();
        let id = text(get(
            &r.call_as(
                &hr,
                Surface::Rest,
                &mut Vault::new(),
                "payroll",
                "create",
                m(vec![("salary", Value::Int(73_250))]),
            )
            .unwrap()
            .value,
            "id",
        ));
        let want = format!(
            "limit_violated: {}: effect 1 (create) failed, nothing was changed: {}_log.cap must be at most 100",
            r.cmd("payroll", "pay"),
            r.p
        );
        let people = [
            ("an ordinary user", r.who.clone(), Surface::Rest),
            ("an agent", r.bot(), Surface::Mcp),
            // Even the person who can read the salary: what the action wrote is not theirs.
            ("the HR person", hr.clone(), Surface::Rest),
        ];
        for (who, p, surface) in people {
            let e = r.err_as(
                &p,
                surface,
                &mut Vault::new(),
                "payroll",
                "pay",
                m(vec![("id", t(&id))]),
            );
            assert_eq!(e, want, "{who}");
        }
        // Nothing was written, and the state did not move.
        assert_eq!(r.count("log"), 0);
        let row = r
            .call_as(
                &hr,
                Surface::Rest,
                &mut Vault::new(),
                "payroll",
                "get",
                m(vec![("id", t(&id))]),
            )
            .unwrap()
            .value;
        assert_eq!(text(get(&row, "state")), "new");
    });
}

#[test]
fn a_decimal_breach_states_no_value_to_an_agent_or_from_an_effect() {
    each(|r| {
        // A person who typed the decimal is told what it was.
        let e = r.refused("log", vec![("ratio", t("1.00"))]);
        assert_limit(&e, "ratio", "at most 0.90; this is 1.00");
        // An agent is not, even for what it typed itself.
        let e = r.err_as(
            &r.bot(),
            Surface::Mcp,
            &mut Vault::new(),
            "log",
            "create",
            m(vec![("ratio", t("1.00"))]),
        );
        assert_eq!(
            e,
            format!("limit_violated: {}_log.ratio must be at most 0.90", r.p)
        );
        // An action that copies a decimal the caller may not read into the bounded field never
        // states it, whoever runs it.
        let hr = r.hr();
        let id = text(get(
            &r.call_as(
                &hr,
                Surface::Rest,
                &mut Vault::new(),
                "payroll",
                "create",
                m(vec![("bonus", t("12.50"))]),
            )
            .unwrap()
            .value,
            "id",
        ));
        let want = format!(
            "limit_violated: {}: effect 1 (create) failed, nothing was changed: {}_log.ratio must be at most 0.90",
            r.cmd("payroll", "grant"),
            r.p
        );
        for (who, p, surface) in [
            ("an ordinary user", r.who.clone(), Surface::Rest),
            ("an agent", r.bot(), Surface::Mcp),
            ("the HR person", hr.clone(), Surface::Rest),
        ] {
            let e = r.err_as(
                &p,
                surface,
                &mut Vault::new(),
                "payroll",
                "grant",
                m(vec![("id", t(&id))]),
            );
            assert_eq!(e, want, "{who}");
        }
        assert_eq!(r.count("log"), 0);
    });
}

#[test]
fn a_state_move_with_an_over_limit_field_is_refused_before_the_action_runs() {
    each(|r| {
        // `state` is governed: `update` moves it only as `finish`, which writes a log record.
        let id = r.create("doc", vec![("title", t("ok")), ("plain", t("ab"))]);
        let Err(CallError::Handler(e)) =
            r.update(&id, vec![("state", t("done")), ("title", t("abcdefghijk"))])
        else {
            panic!("the update should have been refused")
        };
        assert_limit(&e, "title", "at most 10 characters; this has 11");
        // The action did not run: no log record, the state did not move, the title is as it was.
        assert_eq!(
            r.count("log"),
            0,
            "the action ran before the update was refused"
        );
        let row = r.read("doc", &id);
        assert_eq!(text(get(&row, "state")), "new");
        assert_eq!(text(get(&row, "title")), "ok");
        // An agent that holds a name as a token and sends it as the title along with the move
        // is refused the same way and is not told how long the name behind the token is.
        let named = r.create("doc", vec![("title", t("ok")), ("person", t(LONG_NAME))]);
        let (bot, mut vault) = (r.bot(), Vault::new());
        let seen = r
            .call_as(
                &bot,
                Surface::Mcp,
                &mut vault,
                "doc",
                "get",
                m(vec![("id", t(&named))]),
            )
            .unwrap()
            .value;
        let token = text(get(&seen, "person"));
        assert_ne!(token, LONG_NAME, "the agent must hold a token");
        let e = r.err_as(
            &bot,
            Surface::Mcp,
            &mut vault,
            "doc",
            "update",
            m(vec![
                ("id", t(&id)),
                ("state", t("done")),
                ("title", t(&token)),
            ]),
        );
        assert_eq!(
            e,
            format!(
                "limit_violated: {}_doc.title must be at most 10 characters",
                r.p
            )
        );
        assert_eq!(r.count("log"), 0, "the action ran for the agent");
        assert_eq!(text(get(&r.read("doc", &id), "state")), "new");
        // Nothing wrong with the same move once the title fits.
        r.update(&id, vec![("state", t("done")), ("title", t("fits"))])
            .unwrap();
        assert_eq!(r.count("log"), 1);
        assert_eq!(text(get(&r.read("doc", &id), "state")), "done");
    });
}

#[test]
fn a_state_move_that_sends_back_an_over_limit_stored_value_is_not_refused_for_it() {
    each(|r| {
        let id = r.create("doc", vec![("title", t("ok")), ("plain", t("ab"))]);
        // Stored before a limit was lowered, and sent back by a form with the state change.
        r.corrupt("doc", &id, "title", serde_json::json!("longer than ten"));
        r.update(
            &id,
            vec![("state", t("done")), ("title", t("longer than ten"))],
        )
        .unwrap();
        assert_eq!(r.count("log"), 1);
        let row = r.read("doc", &id);
        assert_eq!(text(get(&row, "state")), "done");
        assert_eq!(text(get(&row, "title")), "longer than ten");
    });
}

#[test]
fn a_decimal_stored_above_a_lowered_bound_stays_editable_whatever_its_spelling() {
    each(|r| {
        // `rate` holds 0.1 to 0.9 now; 5.0 was fine before the bound was set.
        let id = r.create("doc", vec![("title", t("first")), ("rate", t("0.5"))]);
        r.corrupt("doc", &id, "rate", serde_json::json!("5.0000000"));
        r.update(&id, vec![("plain", t("edited"))]).unwrap();
        // The same number written as 5, 5.0 or in full is the stored value: not refused.
        for (i, spelling) in ["5", "5.0", "5.0000000"].into_iter().enumerate() {
            r.update(
                &id,
                vec![("rate", t(spelling)), ("plain", t(&format!("e{i}")))],
            )
            .unwrap_or_else(|e| panic!("{spelling}: {e:?}"));
        }
        assert_eq!(text(get(&r.read("doc", &id), "rate")), "5.0000000");
        // A different value is held to the bound, however close to the stored one.
        for other in ["6", "5.0000001", "0.9000001"] {
            let Err(CallError::Handler(e)) = r.update(&id, vec![("rate", t(other))]) else {
                panic!("{other} should have been refused")
            };
            assert_limit(&e, "rate", "at most 0.9000000; this is ");
        }
        // Going back inside is always possible.
        r.update(&id, vec![("rate", t("0.3"))]).unwrap();
        assert_eq!(text(get(&r.read("doc", &id), "rate")), "0.3000000");
    });
}
