//! Model rules (D88, first slice) and the no-overlap rule through the dispatcher: create,
//! update, `bulk_create` and an action's effect, the refusal's code and key, the translated
//! message, and a record stored before the rule that stays editable. Memory and SQLite, and
//! PostgreSQL when `OBOP_TEST_POSTGRES_URL` is set.

use crate::errors::{self, Kind};
use crate::{CallError, Dispatcher, Principal, Store, Surface, Value, Vault, locale};
use obop_registry::model_rules::no_overlap;
use obop_registry::{
    Bound, CommandDef, Effect, FieldDef as F, FieldKind, Limits, ModelDef, Module, Pattern, intern,
};
use obop_types::DataClass;
use serde_json::json;
use std::collections::BTreeMap;

const U: &str = "mr.user";
const EN: &str = "The booking ends before it starts";
const CLASH: &str = "This resource is already booked at that time";

fn module(p: &str) -> Module {
    let n = |s: &str| intern(&format!("{p}_{s}"));
    let f = |name: &'static str| F::new(name, "d", DataClass::None, false, U);
    let booking = ModelDef {
        name: n("booking"),
        description: "d",
        read_group: U,
        write_group: U,
        access: None,
        fields: vec![
            f("resource"),
            f("day").kind(FieldKind::Date),
            f("start").kind(FieldKind::Number),
            f("end").kind(FieldKind::Number),
            f("note"),
            f("state").select(&["open", "done", "cancelled"]),
            f("iban").with_limits(Limits {
                pattern: Some(Pattern::Iban),
                ..Default::default()
            }),
            f("currency"),
            f("paid_on").kind(FieldKind::Date).with_limits(Limits {
                min: Bound::date("1900-01-01"),
                max: Bound::date("2199-12-31"),
                ..Default::default()
            }),
        ],
        commands: vec![
            CommandDef::new("cancel", "Cancel", U).transition("state", Some("open"), "cancelled"),
            CommandDef::new("complete", "Complete", U).transition("state", Some("open"), "done"),
            CommandDef::new("reopen", "Reopen", U).transition("state", Some("cancelled"), "open"),
        ],
        ext: Default::default(),
    }
    .rule("ends_after_start", "end >= start", EN)
    .with_no_overlap(no_overlap(
        "no_double_booking",
        &["resource".into(), "day".into()],
        "start",
        "end",
        None,
        &["cancelled".into()],
        CLASH,
    ));
    let request = ModelDef {
        name: n("request"),
        description: "d",
        read_group: U,
        write_group: U,
        access: None,
        fields: vec![
            f("resource"),
            f("day").kind(FieldKind::Date),
            f("start").kind(FieldKind::Number),
            f("end").kind(FieldKind::Number),
            f("state").select(&["new", "booked"]),
        ],
        commands: vec![
            CommandDef::new("book", "Book", U)
                .transition("state", Some("new"), "booked")
                .effect(Effect::create(
                    &format!("{p}_booking"),
                    &[
                        ("resource", "$self.resource"),
                        ("day", "$self.day"),
                        ("start", "$self.start"),
                        ("end", "$self.end"),
                    ],
                )),
        ],
        ext: Default::default(),
    };
    let labels = format!(
        "[labels.de.model.{p}_booking.rule.ends_after_start]\nmessage = \"Die Buchung endet vor dem Beginn\"\n"
    );
    Module {
        id: p.into(),
        name: "R".into(),
        models: vec![booking, request],
        ..Module::default()
    }
    .translated(&[Box::leak(labels.into_boxed_str())])
}

struct Rig {
    d: Dispatcher,
    p: String,
    store: Store,
}

fn each(f: impl Fn(&Rig)) {
    let mut urls = vec!["memory:".to_string(), "sqlite::memory:".to_string()];
    if let Ok(u) = std::env::var("OBOP_TEST_POSTGRES_URL") {
        urls.push(u);
    }
    for url in urls {
        let p = format!("r{}", &uuid::Uuid::now_v7().simple().to_string()[22..]);
        let module = module(&p);
        let v = obop_registry::check_module(&module);
        assert!(v.is_empty(), "{v:?}");
        let store = Store::open(&url).unwrap();
        let d = Dispatcher::new();
        d.register_crud(&module, &store);
        for mo in &module.models {
            for c in &mo.commands {
                d.register_transition(&module, mo.name, c.name, &store);
            }
        }
        f(&Rig { d, p, store });
    }
}

fn m(pairs: &[(&str, Value)]) -> Value {
    Value::Map(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}

fn booking(resource: &str, day: &str, start: i64, end: i64) -> Value {
    m(&[
        ("resource", Value::text(resource)),
        ("day", Value::text(day)),
        ("start", Value::Int(start)),
        ("end", Value::Int(end)),
    ])
}

impl Rig {
    fn call(&self, c: &str, input: Value) -> Result<Value, CallError> {
        let who = Principal::user("ann", &[U]);
        self.d
            .call(
                Surface::Rest,
                &who,
                &mut Vault::new(),
                &format!("{}_{c}", self.p),
                &input,
            )
            .map(|r| r.value)
    }
    fn id(&self, c: &str, input: Value) -> String {
        match self.call(c, input) {
            Ok(Value::Map(m)) => match &m["id"] {
                Value::Text(t) => t.clone(),
                o => panic!("{o:?}"),
            },
            o => panic!("{c}: {o:?}"),
        }
    }
    fn refuse(&self, c: &str, input: Value) -> (String, errors::Classified) {
        match self.call(c, input) {
            Err(e @ CallError::Handler(_)) => {
                let CallError::Handler(msg) = &e else {
                    unreachable!()
                };
                (msg.clone(), errors::classify(&e))
            }
            o => panic!("{c}: expected a refusal, got {o:?}"),
        }
    }
    fn count(&self) -> usize {
        match self.call("booking.list", m(&[])) {
            Ok(Value::List(l)) => l.len(),
            o => panic!("{o:?}"),
        }
    }
}

#[test]
fn a_create_or_update_that_breaks_a_rule_is_refused_with_its_key() {
    each(|r| {
        let _s = locale::scope(None);
        let (msg, c) = r.refuse("booking.create", booking("room", "2026-12-02", 600, 540));
        assert_eq!(msg, format!("{EN} (rule ends_after_start failed)"));
        assert_eq!(c.kind, Kind::Failed);
        assert_eq!((c.status, c.code), (400, "assertion_failed"));
        assert_eq!(c.body["assert_key"], "ends_after_start");
        assert_eq!(c.body["message"], json!(EN));
        assert_eq!(r.count(), 0, "nothing was stored");

        let id = r.id("booking.create", booking("room", "2026-12-02", 600, 660));
        let (msg, _) = r.refuse(
            "booking.update",
            m(&[("id", Value::text(&id)), ("end", Value::Int(500))]),
        );
        assert!(msg.ends_with("(rule ends_after_start failed)"), "{msg}");
        // An end equal to the start holds.
        r.call(
            "booking.update",
            m(&[("id", Value::text(&id)), ("end", Value::Int(600))]),
        )
        .unwrap();
    });
}

#[test]
fn the_refusal_is_in_the_language_of_the_request() {
    each(|r| {
        let _s = locale::scope(Some("de"));
        let (msg, c) = r.refuse("booking.create", booking("room", "2026-12-02", 600, 540));
        assert_eq!(
            msg,
            "Die Buchung endet vor dem Beginn (rule ends_after_start failed)"
        );
        assert_eq!(c.body["assert_key"], "ends_after_start");
    });
}

#[test]
fn a_record_stored_before_the_rule_stays_editable_in_other_fields() {
    each(|r| {
        // Written straight to the store, as a record from before the rule was declared.
        let mut raw = BTreeMap::new();
        for (k, v) in [
            ("resource", Value::text("room")),
            ("day", Value::text("2026-12-09")),
            ("start", Value::Int(700)),
            ("end", Value::Int(600)),
            ("state", Value::text("open")),
        ] {
            raw.insert(k.to_string(), v);
        }
        let id = r.store.insert(&format!("{}_booking", r.p), raw).unwrap();
        r.call(
            "booking.update",
            m(&[
                ("id", Value::text(&id)),
                ("note", Value::text("call first")),
            ]),
        )
        .expect("a field the rule does not read");
        let (msg, _) = r.refuse(
            "booking.update",
            m(&[("id", Value::text(&id)), ("start", Value::Int(650))]),
        );
        assert!(msg.contains("rule ends_after_start"), "{msg}");
    });
}

#[test]
fn bulk_rows_and_effects_are_held_to_the_rule() {
    each(|r| {
        let rows = Value::List(vec![
            booking("room", "2026-12-04", 600, 660),
            booking("room", "2026-12-05", 600, 500),
        ]);
        let (msg, _) = r.refuse("booking.bulk_create", m(&[("rows", rows)]));
        assert!(msg.contains("rule ends_after_start failed"), "{msg}");
        assert_eq!(r.count(), 0, "all or nothing");

        let mut req = booking("room", "2026-12-02", 900, 800);
        if let Value::Map(x) = &mut req {
            x.remove("state");
        }
        let req = r.id("request.create", req);
        let (msg, _) = r.refuse("request.book", m(&[("id", Value::text(&req))]));
        assert!(msg.contains("rule ends_after_start failed"), "{msg}");
        assert_eq!(r.count(), 0, "the effect wrote nothing");
    });
}

#[test]
fn two_bookings_of_one_resource_may_not_overlap() {
    each(|r| {
        let _s = locale::scope(None);
        let first = r.id("booking.create", booking("room", "2026-12-02", 600, 660));
        let (msg, c) = r.refuse("booking.create", booking("room", "2026-12-02", 630, 690));
        assert_eq!(msg, format!("{CLASH} (rule no_double_booking failed)"));
        assert_eq!(c.body["assert_key"], "no_double_booking");
        assert_eq!(c.code, "assertion_failed");
        // Inside, around and touching.
        r.refuse("booking.create", booking("room", "2026-12-02", 610, 620));
        r.refuse("booking.create", booking("room", "2026-12-02", 500, 700));
        let next = r.id("booking.create", booking("room", "2026-12-02", 660, 720));
        r.id("booking.create", booking("room", "2026-12-02", 540, 600));
        // Another resource, another day.
        r.id("booking.create", booking("hall", "2026-12-02", 600, 660));
        r.id("booking.create", booking("room", "2026-12-03", 600, 660));
        // Moving a booking onto another one is refused; a cancelled one frees its time.
        r.refuse(
            "booking.update",
            m(&[("id", Value::text(&next)), ("start", Value::Int(650))]),
        );
        r.call("booking.cancel", m(&[("id", Value::text(&first))]))
            .unwrap();
        r.id("booking.create", booking("room", "2026-12-02", 600, 660));
        // Within one bulk call, two new rows clash with each other.
        let rows = Value::List(vec![
            booking("room", "2026-12-10", 600, 660),
            booking("room", "2026-12-10", 630, 690),
        ]);
        let (msg, _) = r.refuse("booking.bulk_create", m(&[("rows", rows)]));
        assert!(msg.contains("rule no_double_booking failed"), "{msg}");
    });
}

#[test]
fn bookings_that_overlapped_before_the_rule_can_still_be_completed() {
    each(|r| {
        let _s = locale::scope(None);
        // Written straight to the store, as two bookings from before the rule was declared.
        let raw = |start: i64, end: i64, state: &str| {
            let mut x = BTreeMap::new();
            for (k, v) in [
                ("resource", Value::text("room")),
                ("day", Value::text("2026-12-20")),
                ("start", Value::Int(start)),
                ("end", Value::Int(end)),
                ("state", Value::text(state)),
            ] {
                x.insert(k.to_string(), v);
            }
            r.store.insert(&format!("{}_booking", r.p), x).unwrap()
        };
        let a = raw(780, 840, "open");
        let b = raw(800, 860, "open");
        let c = raw(790, 850, "cancelled");
        // A move between two states that are not ignored adds no clash.
        r.call("booking.complete", m(&[("id", Value::text(&a))]))
            .expect("completing a booking that already overlapped");
        // Moving its time, or taking up a cancelled one that clashes, is still refused.
        let (msg, _) = r.refuse(
            "booking.update",
            m(&[("id", Value::text(&b)), ("end", Value::Int(870))]),
        );
        assert!(msg.contains("rule no_double_booking failed"), "{msg}");
        let (msg, _) = r.refuse("booking.reopen", m(&[("id", Value::text(&c))]));
        assert!(msg.contains("rule no_double_booking failed"), "{msg}");
    });
}

#[test]
fn formats_and_date_bounds_are_held_on_every_write() {
    each(|r| {
        let _s = locale::scope(None);
        let with = |k: &str, v: &str| {
            let mut b = booking("room", "2026-12-20", 600, 660);
            if let Value::Map(x) = &mut b {
                x.insert(k.to_string(), Value::text(v));
            }
            b
        };
        for (k, bad, want) in [
            ("iban", "123123", "must be a valid IBAN"),
            ("currency", "12y12", "must be a currency code"),
            ("paid_on", "0313-12-13", "must be at least 1900-01-01"),
            ("paid_on", "2313-12-13", "must be at most 2199-12-31"),
        ] {
            let (msg, c) = r.refuse("booking.create", with(k, bad));
            assert!(msg.contains(want), "{k}: {msg}");
            assert_eq!(c.code, "limit_violated", "{k}");
        }
        let id = r.id(
            "booking.create",
            with("iban", "DE89 3704 0044 0532 0130 00"),
        );
        r.call(
            "booking.update",
            m(&[
                ("id", Value::text(&id)),
                ("currency", Value::text("SEK")),
                ("paid_on", Value::text("2026-11-17")),
            ]),
        )
        .unwrap();
        let (msg, _) = r.refuse(
            "booking.update",
            m(&[("id", Value::text(&id)), ("currency", Value::text("sek"))]),
        );
        assert!(msg.contains("currency code"), "{msg}");
    });
}
