//! The switch `hide_product_docs` (docs/spec/branding.md, sections 2, 3, 4 and 13, S2): a boolean
//! that is stored as the boolean true, off when it is absent, read as a boolean only, set by an
//! explicit true or false, and logged as the words `true` and `false`.

use super::*;
use crate::brand::{Action, Applied, Failure, SWITCH, apply};

fn cfg(pairs: &[(&str, &str)]) -> Action {
    Action::Configure(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    )
}

fn ok(spy: &Spy, pairs: &[(&str, &str)]) -> Applied {
    apply(spy, None, "ada", &cfg(pairs), false).unwrap_or_else(|e| panic!("refused: {e}"))
}

fn stored(spy: &Spy) -> Fields {
    spy.get(MODEL, ROW_ID).unwrap().expect("a row")
}

fn details(spy: &Spy) -> Vec<String> {
    spy.list("security_event")
        .unwrap()
        .iter()
        .map(|(_, f)| f["detail"].as_str().unwrap().to_string())
        .collect()
}

fn read(row: Option<&Fields>) -> (bool, Vec<(String, String)>) {
    let shown = crate::brand::row::read_row(row, None);
    (
        shown.hide_product_docs(),
        shown
            .fallbacks()
            .iter()
            .map(|f| (f.field.clone(), f.reason.clone()))
            .collect(),
    )
}

#[test]
fn the_switch_is_stored_as_the_boolean_true_and_false_removes_the_key() {
    let spy = Spy::memory();
    let a = ok(&spy, &[("name", "MOHV"), (SWITCH, "true")]);
    let changed: Vec<_> = a
        .changes
        .iter()
        .map(|c| (c.field.as_str(), c.old.as_str(), c.new.as_str()))
        .collect();
    assert_eq!(
        changed,
        [("name", "", "MOHV"), (SWITCH, "false", "true")],
        "the words true and false, not quoted texts"
    );
    assert_eq!(stored(&spy).get(SWITCH), Some(&J::Bool(true)));
    assert_eq!(read(Some(&stored(&spy))), (true, vec![]));

    // False removes the key: off is the absence of the switch, never a stored false.
    let a = ok(&spy, &[(SWITCH, "false")]);
    assert_eq!(a.changes.len(), 1);
    assert!(!stored(&spy).contains_key(SWITCH));
    assert_eq!(read(Some(&stored(&spy))), (false, vec![]));
}

#[test]
fn the_same_value_twice_changes_nothing_and_writes_no_event() {
    let spy = Spy::memory();
    ok(&spy, &[("name", "MOHV"), (SWITCH, "true")]);
    let events = details(&spy).len();
    let batches = spy.batches().len();
    let again = ok(&spy, &[(SWITCH, "true")]);
    assert!(again.changes.is_empty(), "{:?}", again.changes);
    // Off over off is nothing too, with the key absent.
    ok(&spy, &[(SWITCH, "false")]);
    let events_after_off = details(&spy).len();
    assert_eq!(events_after_off, events + 1, "the switch off is one event");
    let again = ok(&spy, &[(SWITCH, "false")]);
    assert!(again.changes.is_empty());
    assert_eq!(details(&spy).len(), events_after_off);
    assert!(spy.batches().len() > batches);
}

#[test]
fn the_audit_line_names_the_switch_and_its_old_and_new_words() {
    let spy = Spy::memory();
    ok(&spy, &[("name", "MOHV")]);
    ok(&spy, &[(SWITCH, "true")]);
    ok(&spy, &[(SWITCH, "false")]);
    let got = details(&spy);
    assert!(
        got.contains(&"hide_product_docs: false -> true".to_string()),
        "{got:?}"
    );
    assert!(
        got.contains(&"hide_product_docs: true -> false".to_string()),
        "{got:?}"
    );
}

#[test]
fn anything_but_true_or_false_is_refused_by_name_and_nothing_is_stored() {
    for bad in ["", "yes", "1", "TRUE", "on", " true", "null"] {
        let spy = Spy::memory();
        match apply(
            &*spy,
            None,
            "ada",
            &cfg(&[("name", "MOHV"), (SWITCH, bad)]),
            false,
        ) {
            Err(Failure::Refused(list)) => {
                assert_eq!(list.len(), 1, "{bad:?}: {list:?}");
                assert_eq!(list[0].field, SWITCH);
                assert!(list[0].rule.contains("true or false"), "{}", list[0].rule);
            }
            other => panic!("{bad:?}: expected a refusal, got {other:?}"),
        }
        assert!(spy.get(MODEL, ROW_ID).unwrap().is_none(), "{bad:?}");
        assert!(details(&spy).is_empty(), "{bad:?}");
    }
}

#[test]
fn a_stored_value_that_is_not_a_boolean_reads_as_off_and_is_a_fallback() {
    for (stored_value, label) in [
        (s("true"), "the text true"),
        (s("yes"), "the text yes"),
        (serde_json::json!(1), "the number 1"),
        (serde_json::json!(["true"]), "a list"),
        (serde_json::json!({"on": true}), "an object"),
    ] {
        let row = fields(&[("name", s("MOHV")), (SWITCH, stored_value)]);
        let (on, fallbacks) = read(Some(&row));
        assert!(!on, "{label} must read as off");
        assert_eq!(
            fallbacks,
            vec![(SWITCH.to_string(), "not_boolean".to_string())],
            "{label}"
        );
    }
    // A boolean false, null and no key at all are off with no fallback.
    for stored_value in [J::Bool(false), J::Null] {
        let row = fields(&[("name", s("MOHV")), (SWITCH, stored_value)]);
        assert_eq!(read(Some(&row)), (false, vec![]));
    }
    assert_eq!(read(Some(&fields(&[("name", s("MOHV"))]))), (false, vec![]));
    // No row: off.
    assert_eq!(read(None), (false, vec![]));
}

#[test]
fn a_supplied_value_repairs_a_stored_one_that_is_not_a_boolean_and_logs_the_word() {
    let spy = Spy::memory();
    put_row(&*spy, &[("name", s("MOHV")), (SWITCH, s("yes"))]);
    // Even the value that equals the reading (off) is a change: the stored text is replaced.
    let a = ok(&spy, &[(SWITCH, "false")]);
    assert_eq!(a.changes.len(), 1, "{:?}", a.changes);
    assert_eq!(
        (a.changes[0].old.as_str(), a.changes[0].new.as_str()),
        ("not_boolean", "false")
    );
    assert!(!stored(&spy).contains_key(SWITCH));
    assert!(
        details(&spy).contains(&"hide_product_docs: not_boolean -> false".to_string()),
        "{:?}",
        details(&spy)
    );
}

#[test]
fn a_field_that_is_not_supplied_keeps_the_switch_through_a_merge() {
    let spy = Spy::memory();
    ok(&spy, &[("name", "MOHV"), (SWITCH, "true")]);
    ok(&spy, &[("title_sv", "Logga in")]);
    assert_eq!(stored(&spy).get(SWITCH), Some(&J::Bool(true)));
}

#[test]
fn a_dry_run_reports_the_switch_and_writes_nothing() {
    let spy = Spy::memory();
    ok(&spy, &[("name", "MOHV")]);
    let sent = spy.batches().len();
    let a = apply(&*spy, None, "ada", &cfg(&[(SWITCH, "true")]), true).unwrap();
    assert!(a.dry_run && a.refusals.is_empty());
    assert_eq!(a.changes.len(), 1);
    assert!(!stored(&spy).contains_key(SWITCH));
    assert_eq!(spy.batches().len(), sent);
    // And the refusal of a bad value is typed on a dry run, as for every field.
    let a = apply(&*spy, None, "ada", &cfg(&[(SWITCH, "maybe")]), true).unwrap();
    assert_eq!(a.refusals.len(), 1);
    assert_eq!(a.refusals[0].field, SWITCH);
}
