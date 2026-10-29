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
                ],
                vec![],
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
        assert!(
            e.contains(".short must be at most 3 characters; this has 4"),
            "{e}"
        );
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
        assert!(e.contains(".qty must be at most 100; this is 101"), "{e}");
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
