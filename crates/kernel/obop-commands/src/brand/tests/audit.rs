//! The audit of `apply` (docs/spec/branding.md, section 3 "Audit" and section 13, "Audit"): one
//! event per changed field, in the same atomic batch as the change, and the exact text of the
//! detail, pinned.

use super::*;
use crate::brand::{Action, apply};
use obop_brand::logo;

fn cfg(pairs: &[(&str, &str)]) -> Action {
    Action::Configure(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    )
}

/// The events of the log, oldest first: `(kind, user, actor, detail)`.
fn events(spy: &Spy) -> Vec<(String, String, String, String)> {
    spy.list("security_event")
        .unwrap()
        .into_iter()
        .map(|(_, f)| {
            let g = |k: &str| f[k].as_str().unwrap().to_string();
            (g("kind"), g("user"), g("actor"), g("detail"))
        })
        .collect()
}

fn details(spy: &Spy) -> Vec<String> {
    events(spy).into_iter().map(|e| e.3).collect()
}

#[test]
fn one_event_per_changed_field_with_the_old_and_the_new_value() {
    let spy = Spy::memory();
    apply(
        &*spy,
        None,
        "ada",
        &cfg(&[
            ("name", "MOHV"),
            ("title_sv", "Logga in"),
            ("message_en", "Welcome"),
        ]),
        false,
    )
    .unwrap();
    let got = events(&spy);
    assert_eq!(got.len(), 3);
    for (kind, user, actor, _) in &got {
        assert_eq!(kind, "brand_configured");
        assert_eq!(user, "(instance)", "the user of each event");
        assert_eq!(actor, "ada");
    }
    // Golden: the detail names the field, then the old and the new value as quoted text.
    assert_eq!(
        details(&spy),
        [
            "name: \"\" -> \"MOHV\"",
            "title_sv: \"\" -> \"Logga in\"",
            "message_en: \"\" -> \"Welcome\"",
        ]
    );
    // A second change: only the field that changed, with both values.
    apply(
        &*spy,
        None,
        "operator",
        &cfg(&[("name", "MOHV"), ("title_sv", "Anmäl dig")]),
        false,
    )
    .unwrap();
    let got = events(&spy);
    assert_eq!(got.len(), 4, "the name was not changed, so it has no event");
    assert_eq!(got[3].3, "title_sv: \"Logga in\" -> \"Anmäl dig\"");
    assert_eq!(got[3].2, "operator", "the actor is operator at a terminal");
}

#[test]
fn the_artwork_is_recorded_as_its_version_never_as_text() {
    let spy = Spy::memory();
    apply(&*spy, None, "ada", &cfg(&[("name", "MOHV")]), false).unwrap();
    apply(&*spy, None, "ada", &cfg(&[("logo_svg", M_ARTWORK)]), false).unwrap();
    let v = logo::version(M_ARTWORK.as_bytes());
    assert_eq!(details(&spy)[1], format!("logo_svg: none -> v={v}"));
    // Swapping it for another records both versions.
    let other = M_ARTWORK.replace("#495670", "#c2410c");
    let v2 = logo::version(other.as_bytes());
    apply(
        &*spy,
        None,
        "ada",
        &cfg(&[("logo_svg", other.as_str())]),
        false,
    )
    .unwrap();
    assert_eq!(details(&spy)[2], format!("logo_svg: v={v} -> v={v2}"));
    // And removing it.
    apply(&*spy, None, "ada", &cfg(&[("logo_svg", "")]), false).unwrap();
    assert_eq!(details(&spy)[3], format!("logo_svg: v={v2} -> none"));
    for d in details(&spy) {
        assert!(
            !d.contains("<svg") && !d.contains("viewBox") && d.len() < 120,
            "{d}"
        );
    }
}

#[test]
fn a_stored_artwork_the_reader_refuses_is_recorded_as_unreadable() {
    let spy = Spy::memory();
    put_row(
        &*spy,
        &[("name", s("MOHV")), ("logo_svg", s("<script>x</script>"))],
    );
    apply(&*spy, None, "ada", &cfg(&[("logo_svg", M_ARTWORK)]), false).unwrap();
    let v = logo::version(M_ARTWORK.as_bytes());
    assert_eq!(details(&spy), [format!("logo_svg: unreadable -> v={v}")]);
}

#[test]
fn the_row_and_its_events_are_one_batch() {
    let spy = Spy::memory();
    apply(
        &*spy,
        None,
        "ada",
        &cfg(&[("name", "MOHV"), ("title_en", "Hi")]),
        false,
    )
    .unwrap();
    let batches = spy.batches();
    assert_eq!(batches.len(), 1, "one atomic batch");
    let ops = &batches[0];
    assert_eq!(ops.len(), 3, "the row and one event per changed field");
    assert!(matches!(&ops[0], Op::Put { model, id, .. } if model == MODEL && id == ROW_ID));
    assert!(
        ops[1..]
            .iter()
            .all(|o| matches!(o, Op::Insert { model, .. } if model == "security_event"))
    );
}

#[test]
fn a_refused_batch_leaves_neither_the_record_nor_the_events() {
    let spy = Spy::memory();
    spy.refuse_next_apply("storage_busy: the pool is full");
    let e = apply(
        &*spy,
        None,
        "ada",
        &cfg(&[("name", "MOHV"), ("title_en", "Hi")]),
        false,
    )
    .unwrap_err();
    assert_eq!(e.to_string(), "storage_busy: the pool is full");
    assert!(spy.get(MODEL, ROW_ID).unwrap().is_none());
    assert!(events(&spy).is_empty());
    // The same for a clear.
    apply(&*spy, None, "ada", &cfg(&[("name", "MOHV")]), false).unwrap();
    spy.refuse_next_apply("storage_unavailable: gone");
    assert!(apply(&*spy, None, "ada", &Action::Clear, false).is_err());
    assert!(spy.get(MODEL, ROW_ID).unwrap().is_some());
    assert_eq!(events(&spy).len(), 1, "only the event of the first call");
}

#[test]
fn clear_writes_one_event_per_non_empty_stored_field_with_its_old_value() {
    let spy = Spy::memory();
    apply(
        &*spy,
        None,
        "ada",
        &cfg(&[
            ("name", "MOHV"),
            ("title_sv", "Logga in"),
            ("logo_svg", M_ARTWORK),
        ]),
        false,
    )
    .unwrap();
    let before = events(&spy).len();
    let a = apply(&*spy, None, "operator", &Action::Clear, false).unwrap();
    assert!(a.cleared);
    let all = events(&spy);
    let got = &all[before..];
    let v = logo::version(M_ARTWORK.as_bytes());
    assert_eq!(
        got.iter().map(|e| e.3.as_str()).collect::<Vec<_>>(),
        [
            "name: \"MOHV\" -> removed".to_string(),
            "title_sv: \"Logga in\" -> removed".to_string(),
            format!("logo_svg: v={v} -> removed"),
        ]
    );
    for (kind, user, actor, _) in got {
        assert_eq!(kind, "brand_cleared");
        assert_eq!(user, "(instance)");
        assert_eq!(actor, "operator");
    }
    // The one batch: Delete, then the events.
    let last = spy.batches().pop().unwrap();
    assert!(matches!(&last[0], Op::Delete { model, id } if model == MODEL && id == ROW_ID));
    assert_eq!(last.len(), 4);
}

#[test]
fn clear_with_no_row_writes_no_event() {
    let spy = Spy::memory();
    apply(&*spy, None, "ada", &Action::Clear, false).unwrap();
    assert!(events(&spy).is_empty());
    assert!(spy.batches().is_empty());
}

#[test]
fn a_dry_run_logs_nothing() {
    let spy = Spy::memory();
    apply(&*spy, None, "ada", &cfg(&[("name", "MOHV")]), true).unwrap();
    apply(&*spy, None, "ada", &Action::Clear, true).unwrap();
    assert!(events(&spy).is_empty());
}

#[test]
fn a_value_written_around_the_command_cannot_forge_or_reorder_a_line_of_the_log() {
    // A clear writes the old value of every field. One that was put into the store directly may hold
    // a newline, a bidirectional override or a very long run: the line has none of them raw, and is
    // cut.
    let spy = Spy::memory();
    let hostile = "x\nbrand_cleared: forged \u{202e}evil \"quoted\" \\ \u{1b}[31m";
    let long = "y".repeat(500);
    put_row(
        &*spy,
        &[
            ("name", s(hostile)),
            ("message_en", s(&long)),
            ("title_en", s("Caf\u{e9}")),
        ],
    );
    apply(&*spy, None, "operator", &Action::Clear, false).unwrap();
    let d = details(&spy);
    assert_eq!(d.len(), 3);
    for line in &d {
        assert!(
            !line.contains('\n') && !line.contains('\u{202e}') && !line.contains('\u{1b}'),
            "{line:?}"
        );
    }
    assert_eq!(
        d[0],
        "name: \"x\\u{000A}brand_cleared: forged \\u{202E}evil \\\"quoted\\\" \\\\ \\u{001B}[31m\" -> removed"
    );
    // A readable value is shown as it is, with its non-ASCII letters.
    assert_eq!(d[1], "title_en: \"Caf\u{e9}\" -> removed");
    assert!(d[2].starts_with("message_en: \"yyyy"), "{}", &d[2][..40]);
    assert!(
        d[2].contains("...") && d[2].len() < 360,
        "cut: {} bytes",
        d[2].len()
    );
}
