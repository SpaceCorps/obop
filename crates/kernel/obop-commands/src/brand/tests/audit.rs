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

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[test]
fn every_event_of_a_call_is_dated_now_and_all_alike() {
    let spy = Spy::memory();
    let before = now();
    assert!(before > 1_700_000_000, "the clock of this machine");
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
    apply(&*spy, None, "ada", &Action::Clear, false).unwrap();
    let after = now();
    let list = spy.list("security_event").unwrap();
    assert_eq!(list.len(), 6);
    // The id of an event starts with its Unix time; the field `at` is the same instant as text.
    let secs: Vec<u64> = list
        .iter()
        .map(|(id, _)| id.split('-').next().unwrap().parse().unwrap())
        .collect();
    for ((_, f), s) in list.iter().zip(&secs) {
        assert!(
            (before..=after).contains(s),
            "{s} is not between {before} and {after}"
        );
        assert_eq!(f["at"].as_str().unwrap(), crate::audit::rfc3339(*s));
    }
    assert!(
        secs[..3].windows(2).all(|w| w[0] == w[1]) && secs[3..].windows(2).all(|w| w[0] == w[1]),
        "one instant for the events of one call: {secs:?}"
    );
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
fn clear_writes_one_event_per_stored_key_with_its_old_value() {
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
    assert_eq!(
        d[2],
        format!("message_en: \"{}\"... -> removed", "y".repeat(300)),
        "cut at 300 characters, with the mark after the closing quote"
    );
}

#[test]
fn a_value_is_cut_at_exactly_300_characters_and_marked_only_when_it_was_cut() {
    for (len, marked) in [(299, false), (300, false), (301, true)] {
        let spy = Spy::memory();
        let value = "y".repeat(len);
        put_row(&*spy, &[("name", s("MOHV")), ("message_en", s(&value))]);
        apply(&*spy, None, "ada", &Action::Clear, false).unwrap();
        let shown = "y".repeat(len.min(300));
        let mark = if marked { "..." } else { "" };
        assert_eq!(
            details(&spy)[1],
            format!("message_en: \"{shown}\"{mark} -> removed"),
            "{len} characters"
        );
    }
}

#[test]
fn a_long_value_with_multibyte_characters_is_cut_between_characters_never_inside_one() {
    // 401 characters, 801 bytes, and byte 300 is inside a character: a cut by bytes would panic.
    // The value is one a message must not hold on a read (over 240 characters), so it is written
    // with every non-ASCII character as an escape, 299 of them after the first letter.
    let spy = Spy::memory();
    let value = format!("a{}", "\u{e9}".repeat(400));
    put_row(&*spy, &[("name", s("MOHV")), ("message_en", s(&value))]);
    apply(&*spy, None, "operator", &Action::Clear, false).unwrap();
    assert_eq!(
        details(&spy)[1],
        format!("message_en: \"a{}\"... -> removed", "\\u{00E9}".repeat(299))
    );
}

// ---- clear accounts for every key of the row ------------------------------------------------------

/// A row written around the command (a restore, an older or newer binary, a direct edit) can hold
/// a field whose value is not text and keys this binary does not know. The delete removes all of
/// them, so the log has an event for each, and none of them writes the value: `<not text>` and
/// `<unknown>` stand for it.
#[test]
fn clear_logs_a_value_that_is_not_text_and_a_key_it_does_not_know_without_the_value() {
    let spy = Spy::memory();
    let long_key = "k".repeat(50);
    put_row(
        &*spy,
        &[
            ("name", serde_json::json!(5)),
            ("title_sv", s("Logga in")),
            (
                "message_en",
                serde_json::json!(["a list that must not be printed"]),
            ),
            (
                "logo_svg",
                serde_json::json!({"nested": "an object that must not be printed"}),
            ),
            ("hide_product_docs", serde_json::json!(true)),
            ("zz-secret", s("a value that must not be printed")),
            ("a\nb", s("x")),
            (long_key.as_str(), s("y")),
        ],
    );
    let a = apply(&*spy, None, "operator", &Action::Clear, false).unwrap();
    assert!(a.cleared);
    // The known fields in the order of the fields, then the other keys in the order of their names.
    let got = details(&spy);
    assert_eq!(
        got,
        [
            "name: <not text> -> removed".to_string(),
            "title_sv: \"Logga in\" -> removed".to_string(),
            "message_en: <not text> -> removed".to_string(),
            "logo_svg: <not text> -> removed".to_string(),
            "a?b: <unknown> -> removed".to_string(),
            "hide_product_docs: <unknown> -> removed".to_string(),
            format!("{}...: <unknown> -> removed", "k".repeat(40)),
            "zz-secret: <unknown> -> removed".to_string(),
        ]
    );
    for line in &got {
        assert!(!line.contains("must not be printed"), "{line}");
        assert!(!line.contains('\n'), "{line:?}");
    }
    for (kind, user, actor, _) in events(&spy) {
        assert_eq!(
            (kind.as_str(), user.as_str(), actor.as_str()),
            ("brand_cleared", "(instance)", "operator")
        );
    }
    // The answer says which changes carry no value, and the command line prints the same lines.
    assert_eq!(a.changes.iter().filter(|c| c.withheld).count(), 7);
    assert_eq!(
        a.changes
            .iter()
            .map(|c| crate::brand::change_line(c, true))
            .collect::<Vec<_>>(),
        got
    );
    // The row and the events went in one batch.
    let last = spy.batches().pop().unwrap();
    assert!(matches!(&last[0], Op::Delete { model, id } if model == MODEL && id == ROW_ID));
    assert_eq!(last.len(), 1 + got.len());
    assert!(spy.get(MODEL, ROW_ID).unwrap().is_none());
}

#[test]
fn a_row_with_keys_and_no_text_value_writes_one_event_per_key() {
    for (row, expected) in [
        (
            vec![("hide_product_docs", serde_json::json!(true))],
            vec!["hide_product_docs: <unknown> -> removed"],
        ),
        (
            vec![("name", serde_json::json!(5))],
            vec!["name: <not text> -> removed"],
        ),
        (
            vec![("slug", serde_json::json!(false)), ("future-field", s("v"))],
            vec![
                "slug: <not text> -> removed",
                "future-field: <unknown> -> removed",
            ],
        ),
        // A key that holds the empty text or null: nothing to lose, but the key is accounted for.
        (
            vec![("name", s("")), ("title_en", serde_json::Value::Null)],
            vec!["name: \"\" -> removed", "title_en: \"\" -> removed"],
        ),
        (vec![("logo_svg", s(""))], vec!["logo_svg: none -> removed"]),
    ] {
        let spy = Spy::memory();
        put_row(&*spy, &row);
        let a = apply(&*spy, None, "operator", &Action::Clear, false).unwrap();
        assert!(a.cleared, "{row:?}");
        assert_eq!(details(&spy), expected, "{row:?}");
        assert!(spy.get(MODEL, ROW_ID).unwrap().is_none(), "{row:?}");
    }
}

/// A row written around the command can hold any number of keys this binary does not know. The
/// first 50, in the order of their names, are accounted for one by one; the rest are one event with
/// their number, so the batch (and the lock it is written under) does not grow with the row.
#[test]
fn clear_accounts_for_at_most_fifty_unknown_keys_one_by_one_and_the_rest_in_one_event() {
    for (count, summary) in [
        (49, None),
        (50, None),
        (51, Some("and 1 more key: <unknown> -> removed")),
        (200, Some("and 150 more keys: <unknown> -> removed")),
    ] {
        let spy = Spy::memory();
        let keys: Vec<String> = (0..count).map(|i| format!("future_{i:03}")).collect();
        let mut pairs: Vec<(&str, serde_json::Value)> = vec![("name", s("MOHV"))];
        pairs.extend(
            keys.iter()
                .map(|k| (k.as_str(), s("a value of a newer binary"))),
        );
        put_row(&*spy, &pairs);
        let a = apply(&*spy, None, "operator", &Action::Clear, false).unwrap();
        assert!(a.cleared, "{count}");
        let got = details(&spy);
        let itemised = count.min(50);
        assert_eq!(
            got.len(),
            1 + itemised + usize::from(summary.is_some()),
            "{count}: the name, the first {itemised} keys, and one more event when there are more"
        );
        assert_eq!(got[0], "name: \"MOHV\" -> removed");
        // The keys in the order of their names; the ones after the fiftieth are only counted.
        for (i, line) in got[1..=itemised].iter().enumerate() {
            assert_eq!(
                *line,
                format!("future_{i:03}: <unknown> -> removed"),
                "{count}"
            );
        }
        if let Some(line) = summary {
            assert_eq!(got.last().unwrap(), line, "{count}");
        }
        assert!(
            got.iter().all(|l| !l.contains("a value of a newer binary")),
            "{count}: no value is written"
        );
        // The answer carries the same entries, and the row is gone with its events in one batch.
        assert_eq!(a.changes.len(), got.len(), "{count}");
        assert!(spy.get(MODEL, ROW_ID).unwrap().is_none(), "{count}");
        let last = spy.batches().pop().unwrap();
        assert_eq!(
            last.len(),
            1 + got.len(),
            "{count}: the delete and the events"
        );
    }
}

/// The limit of the log, written down next to `show_key`: keys that differ only in characters it
/// does not show are shown alike, and the summary line of the cap cannot be a key of a row (it has
/// spaces, which no shown key has).
#[test]
fn keys_that_differ_only_in_characters_that_are_not_shown_are_shown_alike() {
    use crate::brand::show_key;
    assert_eq!(
        show_key(
            "a
b"
        ),
        "a?b"
    );
    assert_eq!(show_key("a b"), "a?b");
    assert_eq!(show_key("a?b"), "a?b");
    assert!(!show_key("and 3 more keys").contains(' '));
}

#[test]
fn a_row_with_no_key_at_all_is_deleted_with_no_event() {
    let spy = Spy::memory();
    put_row(&*spy, &[]);
    assert!(
        spy.get(MODEL, ROW_ID).unwrap().is_some(),
        "a row that holds no key"
    );
    let a = apply(&*spy, None, "operator", &Action::Clear, false).unwrap();
    assert!(a.cleared && a.changes.is_empty());
    assert!(spy.get(MODEL, ROW_ID).unwrap().is_none());
    assert!(events(&spy).is_empty());
    let last = spy.batches().pop().unwrap();
    assert!(matches!(last.as_slice(), [Op::Delete { .. }]), "{last:?}");
}

#[test]
fn a_dry_run_of_clear_lists_the_same_changes_and_writes_nothing() {
    let spy = Spy::memory();
    put_row(
        &*spy,
        &[
            ("name", s("MOHV")),
            ("title_en", serde_json::json!(3)),
            ("hide_product_docs", serde_json::json!(true)),
        ],
    );
    let before = spy.get(MODEL, ROW_ID).unwrap();
    let sent = spy.batches().len();
    let a = apply(&*spy, None, "operator", &Action::Clear, true).unwrap();
    assert!(a.cleared && a.dry_run);
    assert_eq!(
        a.changes
            .iter()
            .map(|c| crate::brand::change_line(c, true))
            .collect::<Vec<_>>(),
        [
            "name: \"MOHV\" -> removed",
            "title_en: <not text> -> removed",
            "hide_product_docs: <unknown> -> removed",
        ]
    );
    assert_eq!(spy.get(MODEL, ROW_ID).unwrap(), before);
    assert!(events(&spy).is_empty());
    assert_eq!(spy.batches().len(), sent, "nothing was sent to the store");
}
