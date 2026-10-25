//! Tests of the `decimal` and `datetime` kinds (D72) through the dispatcher: validation and the
//! canonical text on every write path, exact filters, ordering and aggregates checked against an
//! independent `i128` oracle, zone-aware buckets and the swappable time zone database, CSV
//! import with rounding, computed fields, effects (`$now`, `$today`), masking. Every test runs on
//! the memory and SQLite backends, and on PostgreSQL when `OBOP_TEST_POSTGRES_URL` is set.

use crate::{CallError, Dispatcher, Principal, Response, Store, Surface, Value, Vault};
use obop_registry::{
    CommandDef, Effect, FieldDef as F, FieldKind, ModelDef, Module, Round, intern,
};
use obop_types::DataClass;
use std::collections::BTreeMap;
use std::sync::Arc;

const U: &str = "k.user";

mod behaviour;
mod instants;
mod queries;

struct Rig {
    d: Arc<Dispatcher>,
    p: String,
    who: Principal,
}

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
fn int(v: &Value) -> i64 {
    match v {
        Value::Int(i) => *i,
        other => panic!("not an integer: {other:?}"),
    }
}
fn list(v: &Value) -> &[Value] {
    match v {
        Value::List(l) => l,
        other => panic!("not a list: {other:?}"),
    }
}
fn cond(field: &str, op: &str, value: Value) -> Value {
    m(vec![("field", t(field)), ("op", t(op)), ("value", value)])
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
    };
    Module {
        id: p.into(),
        name: "K".into(),
        models: vec![
            model(
                n("item"),
                vec![
                    f("name"),
                    f("tier").select(&["a", "b", "c"]),
                    f("price").decimal(12, 2),
                    f("ratio").decimal(10, 4),
                    f("amount").decimal(24, 8),
                    f("r_even").decimal_rounded(8, 2, Round::HalfEven),
                    f("r_up").decimal_rounded(8, 2, Round::HalfUp),
                    f("r_down").decimal_rounded(8, 2, Round::Down),
                    f("qty").kind(FieldKind::Number),
                    f("at").datetime(),
                    f("due").kind(FieldKind::Date),
                    // Personal data an agent sees as a token, and sensitive data it never sees.
                    F::new("fee", "d", DataClass::PublicDirectory, true, U).decimal(10, 2),
                    F::new("salary", "d", DataClass::Financial, true, U).decimal(10, 2),
                    f("total")
                        .decimal_rounded(14, 2, Round::HalfUp)
                        .computed("price * qty"),
                    f("third").decimal(12, 4).computed("price / 3"),
                    // A chain of computed decimals: each reads the rounded value of the last.
                    f("net").decimal(12, 2).computed("ratio * qty"),
                    f("vat").decimal(12, 2).computed("net * 0.19"),
                    f("gross").decimal(12, 2).computed("net + vat"),
                    f("sum_ok")
                        .kind(FieldKind::Bool)
                        .computed("0.1 + 0.2 == 0.3"),
                    f("lines_total")
                        .decimal(14, 2)
                        .computed("sum(lines.amount)"),
                    f("lines_max").decimal(12, 2).computed("max(lines.amount)"),
                    f("early")
                        .kind(FieldKind::Bool)
                        .computed("at < \"2026-01-01T00:00:00Z\""),
                    f("state").select(&["new", "stamped"]),
                ],
                vec![
                    CommandDef::new("stamp", "Stamp", U)
                        .transition("state", Some("new"), "stamped")
                        .effect(Effect::create(
                            &format!("{p}_log"),
                            &[
                                ("item_id", "$self.id"),
                                ("stamped_at", "$now"),
                                ("day", "$today"),
                                ("price_copy", "$self.price"),
                            ],
                        )),
                ],
            ),
            model(
                n("gate"),
                vec![
                    f("item_id").reference(&format!("{p}_item")),
                    f("state").select(&["open", "done"]),
                ],
                vec![
                    // Whole-number constants into decimal fields, in a create and an update.
                    CommandDef::new("close", "Close", U)
                        .transition("state", Some("open"), "done")
                        .effect(Effect::create(
                            &format!("{p}_log"),
                            &[("item_id", "$self.item_id"), ("price_copy", "5")],
                        ))
                        .effect(Effect::update_ref(
                            &format!("{p}_item"),
                            "item_id",
                            &[("price", "7"), ("ratio", "0")],
                        )),
                ],
            ),
            model(
                n("item_line"),
                vec![
                    f("item_id").reference(&format!("{p}_item")),
                    f("amount").decimal(12, 2),
                ],
                vec![],
            ),
            model(
                n("log"),
                vec![
                    f("item_id").reference(&format!("{p}_item")),
                    f("stamped_at").datetime(),
                    f("day").kind(FieldKind::Date),
                    f("price_copy").decimal(14, 4),
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

fn each(f: impl Fn(&Rig)) {
    let mut urls = vec!["memory:".to_string(), "sqlite::memory:".to_string()];
    if let Ok(u) = std::env::var("OBOP_TEST_POSTGRES_URL") {
        urls.push(u);
    }
    for url in urls {
        let p = format!("k{}", &uuid::Uuid::now_v7().simple().to_string()[22..]);
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
    fn cmd(&self, model: &str, c: &str) -> String {
        format!("{}_{model}.{c}", self.p)
    }
    fn call_as(
        &self,
        surface: Surface,
        who: &Principal,
        model: &str,
        c: &str,
        input: Value,
    ) -> Result<Response, CallError> {
        self.d
            .call(surface, who, &mut Vault::new(), &self.cmd(model, c), &input)
    }
    fn call(&self, model: &str, c: &str, input: Value) -> Result<Response, CallError> {
        self.call_as(Surface::Rest, &self.who, model, c, input)
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
    fn read(&self, model: &str, id: &str) -> Value {
        self.ok(model, "get", m(vec![("id", t(id))]))
    }
    fn search(&self, model: &str, input: Vec<(&str, Value)>) -> Value {
        self.ok(model, "search", m(input))
    }
    /// The `name`s of the rows a filter keeps, in the order returned.
    fn names(&self, filter: Vec<Value>, order: Option<Value>) -> Vec<String> {
        let mut input = vec![("filter", Value::List(filter))];
        if let Some(o) = order {
            input.push(("order", o));
        }
        let out = self.search("item", input);
        list(get(&out, "rows"))
            .iter()
            .map(|r| text(get(r, "name")))
            .collect()
    }
    fn count(&self, model: &str) -> i64 {
        int(get(
            &self.search(model, vec![("limit", Value::Int(1))]),
            "total",
        ))
    }
}

// ---- a tiny independent oracle ------------------------------------------------------------

/// xorshift64*: deterministic data without a dependency.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + self.below((hi - lo) as u64 + 1) as i64
    }
}

fn pow10(n: u32) -> i128 {
    (0..n).fold(1i128, |a, _| a * 10)
}

/// The canonical text of `units / 10^scale`, written with plain integer division.
fn show(units: i128, scale: u32) -> String {
    let sign = if units < 0 { "-" } else { "" };
    let a = units.abs();
    if scale == 0 {
        return format!("{sign}{a}");
    }
    let (int, frac) = (a / pow10(scale), a % pow10(scale));
    format!("{sign}{int}.{frac:0w$}", w = scale as usize)
}

/// `units` of a decimal text with `digits` fractional digits (digits and one point only).
fn units_of(s: &str) -> (i128, u32) {
    let (neg, body) = match s.strip_prefix('-') {
        Some(b) => (true, b),
        None => (false, s),
    };
    let (int, frac) = body.split_once('.').unwrap_or((body, ""));
    let digits: String = format!("{int}{frac}");
    let n: i128 = digits.parse().unwrap();
    (if neg { -n } else { n }, frac.len() as u32)
}

/// Numeric comparison of two decimal texts by cross multiplication.
fn cmp_text(a: &str, b: &str) -> std::cmp::Ordering {
    let ((ua, sa), (ub, sb)) = (units_of(a), units_of(b));
    (ua * pow10(sb)).cmp(&(ub * pow10(sa)))
}

/// A random canonical decimal of `precision` digits and `scale` after the point.
fn random_decimal(rng: &mut Rng, precision: u32, scale: u32) -> (i128, String) {
    let digits = 1 + rng.below(u64::from(precision)) as u32;
    let mut n: i128 = 0;
    for _ in 0..digits {
        n = n * 10 + i128::from(rng.below(10) as u8);
    }
    if rng.below(8) == 0 {
        n = -n;
    }
    (n, show(n, scale))
}

// ---- tests --------------------------------------------------------------------------------

#[test]
fn ten_thousand_random_decimals_round_trip_bit_exact() {
    each(|r| {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        let mut expect: BTreeMap<String, [String; 3]> = BTreeMap::new();
        let mut batch = Vec::new();
        for i in 0..10_000 {
            // A price like the amounts of an accounting system, a ratio to 4 decimals and a
            // 24-digit amount with 8 decimals.
            let row = [
                random_decimal(&mut rng, 12, 2).1,
                random_decimal(&mut rng, 10, 4).1,
                random_decimal(&mut rng, 24, 8).1,
            ];
            let name = format!("n{i:05}");
            batch.push(m(vec![
                ("name", t(&name)),
                ("price", t(&row[0])),
                ("ratio", t(&row[1])),
                ("amount", t(&row[2])),
            ]));
            expect.insert(name, row);
            if batch.len() == 1000 {
                let out = r.ok(
                    "item",
                    "bulk_create",
                    m(vec![("rows", Value::List(std::mem::take(&mut batch)))]),
                );
                assert_eq!(int(get(&out, "created")), 1000);
            }
        }
        let mut seen = 0;
        let mut offset = 0;
        loop {
            let page = r.ok(
                "item",
                "list",
                m(vec![
                    ("limit", Value::Int(2500)),
                    ("offset", Value::Int(offset)),
                    ("order", Value::List(vec![m(vec![("field", t("name"))])])),
                ]),
            );
            let rows = list(&page);
            if rows.is_empty() {
                break;
            }
            for row in rows {
                let want = &expect[&text(get(row, "name"))];
                assert_eq!(text(get(row, "price")), want[0]);
                assert_eq!(text(get(row, "ratio")), want[1]);
                assert_eq!(text(get(row, "amount")), want[2]);
                seen += 1;
            }
            offset += 2500;
        }
        assert_eq!(seen, 10_000);
    });
}

#[test]
fn every_surface_round_trips_the_same_text() {
    each(|r| {
        let mut rng = Rng(7);
        for surface in [Surface::Rest, Surface::Mcp, Surface::Cli] {
            for _ in 0..100 {
                let (_, price) = random_decimal(&mut rng, 12, 2);
                let (_, amount) = random_decimal(&mut rng, 24, 8);
                let made = r
                    .call_as(
                        surface,
                        &r.who,
                        "item",
                        "create",
                        m(vec![
                            ("name", t("x")),
                            ("price", t(&price)),
                            ("amount", t(&amount)),
                        ]),
                    )
                    .unwrap()
                    .value;
                assert_eq!(text(get(&made, "price")), price);
                let back = r
                    .call_as(
                        surface,
                        &r.who,
                        "item",
                        "get",
                        m(vec![("id", get(&made, "id").clone())]),
                    )
                    .unwrap()
                    .value;
                assert_eq!(text(get(&back, "price")), price);
                assert_eq!(text(get(&back, "amount")), amount);
            }
        }
    });
}

#[test]
fn valid_but_loose_decimals_are_stored_in_the_canonical_form() {
    each(|r| {
        for (given, stored, ratio) in [
            ("12", "12.00", "12.0000"),
            ("12.5", "12.50", "12.5000"),
            ("-0", "0.00", "0.0000"),
            ("007.10", "7.10", "7.1000"),
            ("-0.00", "0.00", "0.0000"),
        ] {
            let id = r.create("item", vec![("name", t("x")), ("price", t(given))]);
            assert_eq!(text(get(&r.read("item", &id), "price")), stored, "{given}");
            // The same on update, into a field of another scale.
            let out = r.ok(
                "item",
                "update",
                m(vec![("id", t(&id)), ("ratio", t(given))]),
            );
            assert_eq!(text(get(&out, "ratio")), ratio, "update with {given}");
        }
        // Text fields are untouched.
        let id = r.create("item", vec![("name", t(" 1.50 "))]);
        assert_eq!(text(get(&r.read("item", &id), "name")), " 1.50 ");
    });
}

#[test]
fn hostile_decimals_and_instants_are_refused_naming_the_field() {
    each(|r| {
        let before = r.count("item");
        let cases: Vec<(&str, Value, &str)> = vec![
            ("price", t("1e3"), "exponent"),
            ("price", t("1E3"), "exponent"),
            ("price", t("1,5"), "comma"),
            ("price", t("NaN"), "NaN"),
            ("price", t("Infinity"), "NaN"),
            ("price", t("0x10"), "hexadecimal"),
            ("price", t(" 1.5"), "spaces"),
            ("price", t("1.5 "), "spaces"),
            ("price", t("+1.5"), "leading +"),
            ("price", t(".5"), "before the point"),
            ("price", t("5."), "after the point"),
            ("price", t(""), "empty"),
            ("price", t("1_000"), "separators"),
            ("price", t("1.005"), "3 fractional digits"),
            ("price", t("1.500"), "3 fractional digits"),
            ("price", t("10000000000.00"), "too many digits"),
            ("price", t("-"), "minus"),
            ("price", t("1.5\u{0}"), "unexpected"),
            ("price", Value::Int(12), "never accepted"),
            ("price", Value::Num("12.5".into()), "never accepted"),
            ("price", Value::Bool(true), "decimal"),
            ("price", Value::List(vec![t("1")]), "decimal"),
            ("at", t("2026-02-30T10:00:00Z"), "calendar date"),
            ("at", t("2026-13-01T00:00:00Z"), "calendar date"),
            ("at", t("2026-03-29T01:30:00"), "no offset"),
            ("at", t("2026-03-29"), "not an RFC 3339"),
            ("at", t("2026-03-29 01:30:00Z"), "upper case T"),
            ("at", t("2026-12-31T23:59:60Z"), "leap second"),
            ("at", t("2026-03-29T01:30:00.1234567Z"), "more than 6"),
            ("at", t("2026-03-29T01:30:00+2:00"), "offset"),
            ("at", Value::Int(1_700_000_000), "RFC 3339 string"),
            ("at", Value::Num("1.7e9".into()), "RFC 3339 string"),
            ("at", Value::Bool(false), "RFC 3339"),
        ];
        let n = cases.len() as i64;
        for (field, value, want) in cases {
            let e = r.err(
                "item",
                "create",
                m(vec![("name", t("x")), (field, value.clone())]),
            );
            assert!(
                e.starts_with(&format!("{}_item.{field} must be ", r.p)) && e.contains(want),
                "{field}={value:?}: {e}"
            );
            // Update refuses it the same way.
            let id = r.create("item", vec![("name", t("ok"))]);
            let e = r.err(
                "item",
                "update",
                m(vec![("id", t(&id)), (field, value.clone())]),
            );
            assert!(
                e.contains(&format!("_item.{field} must be ")) && e.contains(want),
                "{e}"
            );
        }
        // Only the good record made for each refused update exists: a refused create writes
        // nothing.
        assert_eq!(r.count("item") - before, n);
    });
}

#[test]
fn a_float_in_json_never_becomes_a_decimal() {
    each(|r| {
        // The JSON `12.5` arrives as a number, not as the text "12.5".
        let input: serde_json::Value = serde_json::json!({"name": "x", "price": 12.5});
        let e = r.err("item", "create", Value::from_json(&input));
        assert!(e.contains("never accepted"), "{e}");
        let input: serde_json::Value = serde_json::json!({"name": "x", "price": 1e3});
        assert!(
            r.err("item", "create", Value::from_json(&input))
                .contains("never accepted")
        );
        let input: serde_json::Value = serde_json::json!({"name": "x", "price": "12.50"});
        r.ok("item", "create", Value::from_json(&input));
        // In a filter, too.
        let e = r.err(
            "item",
            "search",
            m(vec![(
                "filter",
                Value::List(vec![cond("price", "gt", Value::Num("1.5".into()))]),
            )]),
        );
        assert!(e.contains("filter on") && e.contains("price"), "{e}");
    });
}
