use super::*;
use crate::{FieldDef, FieldKind, ModelDef};
use obop_types::DataClass;

fn f(name: &'static str) -> FieldDef {
    FieldDef::new(name, "d", DataClass::None, false, "t.user")
}

fn booking() -> ModelDef {
    ModelDef {
        name: "t_booking",
        description: "d",
        read_group: "t.user",
        write_group: "t.user",
        fields: vec![
            f("resource").kind(FieldKind::Text),
            f("day").kind(FieldKind::Date),
            f("start").kind(FieldKind::Number),
            f("end").kind(FieldKind::Number),
            f("total").kind(FieldKind::Money),
            f("note").kind(FieldKind::LongText),
            FieldDef::new("guest", "d", DataClass::Internal, true, "t.user"),
            f("state").kind(FieldKind::Select(vec!["open", "cancelled"].into())),
        ],
        commands: vec![],
        access: None,
        ext: Default::default(),
    }
}

fn none() -> impl Fn(&str) -> Option<Arc<ModelDef>> {
    |_| None
}

fn rule(m: ModelDef, key: &str, src: &str, msg: &str) -> ModelDef {
    let mut m = m;
    m.ext
        .rules
        .push(ModelRule::parse(key, src, msg, Applies::Touched).unwrap());
    m
}

#[test]
fn a_rule_over_own_fields_passes_the_gate() {
    let m = booking().rule(
        "ends_after_start",
        "end >= start",
        "The booking ends before it starts",
    );
    assert_eq!(problems(&m, &none()), Vec::<String>::new());
    assert_eq!(m.model_rules()[0].reads(), ["end", "start"]);
    assert_eq!(
        m.model_rules()[0].failure_in(None),
        "The booking ends before it starts (rule ends_after_start failed)"
    );
    assert_eq!(
        m.model_rules()[0].failure_in(Some("de")),
        "The booking ends before it starts (rule ends_after_start failed)",
        "no German label: the English message"
    );
    let labels = crate::labels::Translations::parse_labels(
        "[labels.de.model.t_booking.rule.ends_after_start]
message = \"Endet vor dem Beginn\"",
    )
    .unwrap();
    let mut models = vec![m];
    attach_messages(&mut models, &Translations::from_labels(&labels));
    assert_eq!(
        models[0].model_rules()[0].failure_in(Some("de")),
        "Endet vor dem Beginn (rule ends_after_start failed)"
    );
}

#[test]
fn the_gate_refuses_what_a_rule_may_not_be() {
    let cases = [
        ("Bad", "end >= start", "ok", "lowercase letters"),
        ("k", "end + start", "ok", "true/false"),
        ("k", "ghost > 1", "ok", "no field `ghost`"),
        ("k", "guest != \"x\"", "ok", "personal"),
        ("k", "end >= start", "limit_violated: no", "error code"),
        ("k", "end >= start", "a <b> c", "markup"),
        (
            "k",
            "state != \"cancelled\"",
            "ok",
            "moves only through actions",
        ),
    ];
    for (key, src, msg, want) in cases {
        let mut m = rule(booking(), key, src, msg);
        // `state` is governed only when an action moves it.
        m.commands
            .push(crate::CommandDef::new("cancel", "d", "t.user").transition(
                "state",
                Some("open"),
                "cancelled",
            ));
        let p = problems(&m, &none());
        assert!(
            p.iter().any(|x| x.contains(want)),
            "{key} {src} {msg}: {want:?} not in {p:#?}"
        );
    }
    let m = rule(
        rule(booking(), "k", "end >= start", "a"),
        "k",
        "end > 0",
        "b",
    );
    assert!(
        problems(&m, &none())
            .iter()
            .any(|x| x.contains("used twice"))
    );
    let mut m = booking();
    for i in 0..9 {
        m = rule(m, &format!("k{}", "x".repeat(i)), "end >= start", "a");
    }
    assert!(
        problems(&m, &none())
            .iter()
            .any(|x| x.contains("at most 8"))
    );
}

#[test]
fn a_no_overlap_rule_is_checked_against_the_model() {
    let ok = booking().with_no_overlap(no_overlap(
        "no_double_booking",
        &["resource".into(), "day".into()],
        "start",
        "end",
        None,
        &["cancelled".into()],
        "This resource is already booked at that time",
    ));
    assert_eq!(problems(&ok, &none()), Vec::<String>::new());
    assert!(ok.has_rules(), "a no-overlap model takes the write lock");
    assert_eq!(ok.rule_keys(), ["no_double_booking"]);
    assert_eq!(
        ok.rule_message("no_double_booking"),
        Some("This resource is already booked at that time")
    );

    let bad = booking().with_no_overlap(no_overlap(
        "k",
        &["ghost".into()],
        "start",
        "note",
        None,
        &["gone".into()],
        "m",
    ));
    let p = problems(&bad, &none());
    for want in [
        "no field `ghost`",
        "number, date or datetime",
        "no option \"gone\"",
    ] {
        assert!(p.iter().any(|x| x.contains(want)), "{want:?} not in {p:#?}");
    }
}
