//! The condition of `shows_option_of` (D52 amendment) is evaluated by the back office in the
//! browser, never by the server, so the server's expression language and the browser's evaluator
//! must agree on every condition the install gate lets through. This file runs the shared cases of
//! `web/apps/backoffice/src/displayCondition.fixtures.json` through the server's parser, gate and
//! evaluator; `displayCondition.test.ts` runs the same file through the browser's. A change to
//! either side that makes them disagree fails the test of the other.

use crate::engine::computed::to_v;
use obop_registry::expr::{self, AggFn, Env, Expr, Scope, Ty, V};
use obop_registry::{FieldDef, FieldKind, ModelDef};
use obop_types::DataClass;
use serde_json::{Map, Value as J};

const FIXTURES: &str =
    include_str!("../../../../web/apps/backoffice/src/displayCondition.fixtures.json");

/// A model whose fields have the kinds the fixture lists.
fn model(fields: &Map<String, J>) -> ModelDef {
    let fields = fields
        .iter()
        .map(|(name, kind)| {
            let f = FieldDef::new(
                obop_registry::intern(name),
                "d",
                DataClass::None,
                false,
                "t.user",
            );
            match kind.as_str().unwrap() {
                "text" => f,
                "longtext" => f.kind(FieldKind::LongText),
                "number" => f.kind(FieldKind::Number),
                "money" => f.kind(FieldKind::Money),
                "bool" => f.kind(FieldKind::Bool),
                "date" => f.kind(FieldKind::Date),
                "select" => f.select(&["open", "done"]),
                "ref" => f.reference("t_other"),
                // A sequence is a text field the system numbers; an image or a file holds the id
                // of its blob. All three are strings in a row.
                "sequence" => f,
                "image" => f.kind(FieldKind::Image(0)),
                "file" => f.kind(FieldKind::File(0)),
                other => panic!("the fixture has a field of kind {other}"),
            }
        })
        .collect();
    ModelDef {
        name: "t_entry",
        description: "d",
        read_group: "t.user",
        write_group: "t.user",
        access: None,
        fields,
        commands: vec![],
        ext: Default::default(),
    }
}

/// A record as the expression language reads it, which is how the engine reads a stored one.
struct Record<'a> {
    def: &'a ModelDef,
    rec: &'a Map<String, J>,
}

impl Env for Record<'_> {
    fn field(&self, name: &str) -> Result<V, String> {
        let kind = self
            .def
            .fields
            .iter()
            .find(|f| f.name == name)
            .map(|f| &f.kind);
        Ok(to_v(kind, self.rec.get(name)))
    }
    fn aggregate(
        &self,
        _: AggFn,
        _: &str,
        _: Option<&str>,
        _: Option<&str>,
        _: Option<&Expr>,
    ) -> Result<V, String> {
        Err("a display condition has no child records".into())
    }
}

/// What the install gate says about a condition on `def`: the parser, the subset the browser
/// evaluates and the type check, in the order `check_rules` runs them.
fn gate(def: &ModelDef, src: &str) -> Result<Expr, String> {
    let e = Expr::parse(src)?;
    expr::check_display_subset(&e, def)?;
    let look = |_: &str| None;
    let scope = Scope {
        model: def,
        lookup: &look,
        allow_computed: true,
    };
    match expr::check(&e, &scope)? {
        Ty::Bool => Ok(e),
        t => Err(format!("it is {}, not true or false", t.name())),
    }
}

#[test]
fn every_shared_case_holds_or_not_as_the_fixture_says() {
    let fixtures: J = serde_json::from_str(FIXTURES).unwrap();
    let def = model(fixtures["fields"].as_object().unwrap());
    let cases = fixtures["cases"].as_array().unwrap();
    assert!(cases.len() > 50, "the fixture lost its cases");
    for c in cases {
        let (name, when) = (c["name"].as_str().unwrap(), c["when"].as_str().unwrap());
        let e = gate(&def, when)
            .unwrap_or_else(|err| panic!("{name}: the gate refuses {when:?}: {err}"));
        let record = c["record"].as_object().unwrap();
        let got = expr::eval(
            &e,
            &Record {
                def: &def,
                rec: record,
            },
        )
        .unwrap();
        // A condition holds when it is exactly true, as an assertion or the `when` of an effect does.
        assert_eq!(
            got == V::Bool(true),
            c["expected"].as_bool().unwrap(),
            "{name}: {when:?} on {record:?} is {got:?}"
        );
    }
}

#[test]
fn every_invalid_source_is_refused_by_the_gate() {
    let fixtures: J = serde_json::from_str(FIXTURES).unwrap();
    let def = model(fixtures["fields"].as_object().unwrap());
    let invalid = fixtures["invalid"].as_array().unwrap();
    assert!(invalid.len() > 20, "the fixture lost its invalid sources");
    for src in invalid {
        let src = src.as_str().unwrap();
        assert!(gate(&def, src).is_err(), "the gate lets {src:?} through");
    }
}

#[test]
fn the_gate_names_what_a_display_condition_may_use() {
    let def = model(
        serde_json::json!({"n": "number", "kind": "text", "state": "select"})
            .as_object()
            .unwrap(),
    );
    for (src, want) in [
        ("n + 1 > 2", "arithmetic"),
        ("-1 < n", "a minus sign"),
        ("kind == 1.5", "the decimal number 1.5"),
        ("if(true, true, false)", "`if`"),
        ("concat(kind) == \"a\"", "`concat`"),
        ("count(lines) > 1", "`count` over child records"),
        ("n == 9007199254740992", "larger than 9007199254740991"),
    ] {
        let e = gate(&def, src).unwrap_err();
        assert!(e.contains(want), "{src:?}: {e}");
    }
    let e = gate(&def, "n + 1 > 2").unwrap_err();
    assert!(e.contains("it may use fields of the record"), "{e}");
    // Types are the language's own.
    assert!(
        gate(&def, "n == \"a\"")
            .unwrap_err()
            .contains("cannot compare")
    );
    assert!(gate(&def, "n").unwrap_err().contains("not true or false"));
    assert!(
        gate(&def, "ghost == 1")
            .unwrap_err()
            .contains("no field `ghost`")
    );
}

#[test]
fn a_condition_cannot_read_a_decimal_or_an_instant() {
    let mut def = model(serde_json::json!({"kind": "text"}).as_object().unwrap());
    def.fields
        .push(FieldDef::new("rate", "d", DataClass::None, false, "t.user").decimal(10, 2));
    def.fields
        .push(FieldDef::new("seen", "d", DataClass::None, false, "t.user").datetime());
    let e = gate(&def, "rate > 1").unwrap_err();
    assert!(e.contains("rate is a decimal number"), "{e}");
    let e = gate(&def, "is_set(seen)").unwrap_err();
    assert!(e.contains("seen is a date and time"), "{e}");
}

/// The browser is a user, and REST shaping never sends a user a field it denies. A condition that
/// reads such a field would see an empty value, so the install gate refuses it: this keeps the
/// gate in step with `mode_for` of obop-masking for every class.
#[test]
fn a_condition_cannot_read_a_field_the_browser_is_never_sent() {
    use obop_masking::{PrincipalKind, mode_for};
    use obop_types::MaskMode;
    let classes = [
        DataClass::None,
        DataClass::PublicDirectory,
        DataClass::Internal,
        DataClass::ConfidentialHr,
        DataClass::SpecialCategory,
        DataClass::Financial,
        DataClass::Secret,
    ];
    let mut refused = 0;
    for class in classes {
        for personal in [false, true] {
            let module = obop_registry::Module {
                id: "t".into(),
                name: "T".into(),
                models: vec![ModelDef {
                    name: "t_entry",
                    description: "d",
                    read_group: "t.user",
                    write_group: "t.user",
                    access: None,
                    fields: vec![
                        FieldDef::new("status", "d", DataClass::None, false, "t.user")
                            .select(&["a", "b"]),
                        FieldDef::new("pin", "d", class, personal, "t.user"),
                        FieldDef::new("text", "d", DataClass::None, false, "t.user")
                            .shows_option_of("t_entry.status")
                            .shows_option_when("is_set(pin)"),
                    ],
                    commands: vec![],
                    ext: Default::default(),
                }],
                ..Default::default()
            };
            let problems: Vec<String> = obop_registry::check_module(&module)
                .into_iter()
                .map(|v| v.problem)
                .filter(|p| p.contains("is_set(pin)"))
                .collect();
            let never_sent = mode_for(PrincipalKind::User, class, personal) != MaskMode::Allow;
            assert_eq!(
                !problems.is_empty(),
                never_sent,
                "{class:?} personal {personal}: {problems:?}"
            );
            if never_sent {
                refused += 1;
                assert!(
                    problems[0].contains("t_entry.pin, which has class secret")
                        && problems[0].contains("the browser is never sent a secret field"),
                    "{problems:?}"
                );
            }
        }
    }
    assert!(refused > 0, "no class is refused: this test lost its point");
    // An unclassed field is treated as secret everywhere (`unwrap_or(Secret)` of the shaping).
    let mut unclassed = FieldDef::new("pin", "d", DataClass::None, false, "t.user");
    unclassed.class = None;
    let mut def = model(serde_json::json!({"status": "select"}).as_object().unwrap());
    def.fields.push(unclassed);
    def.fields.push(
        FieldDef::new("text", "d", DataClass::None, false, "t.user")
            .shows_option_of("t_entry.status")
            .shows_option_when("is_set(pin)"),
    );
    let module = obop_registry::Module {
        id: "t".into(),
        name: "T".into(),
        models: vec![def],
        ..Default::default()
    };
    assert!(
        obop_registry::check_module(&module)
            .iter()
            .any(|v| v.problem.contains("which has class secret"))
    );
}
