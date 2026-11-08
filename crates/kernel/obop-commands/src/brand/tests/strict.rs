//! `strict()`: the row alone, every time, for the text that is written into a person's own device
//! (docs/spec/branding.md, section 4 and section 13, "Fail closed" and "Cache").

use super::*;
use crate::Dispatcher;
use crate::brand::{UNREADABLE, strict};

fn spy_with(pairs: &[(&str, J)]) -> Arc<Spy> {
    let spy = Spy::memory();
    put_row(&*spy, pairs);
    spy
}

#[test]
fn no_row_is_not_a_failure() {
    let spy = Spy::memory();
    let b = strict(&*spy).unwrap();
    assert_eq!(b.name(), "obop");
    assert!(!b.is_set());
}

#[test]
fn the_stored_name_is_the_answer() {
    let spy = spy_with(&[("name", s("MOHV"))]);
    let b = strict(&*spy).unwrap();
    assert_eq!(b.name(), "MOHV");
    assert!(b.is_set());
}

#[test]
fn a_name_that_fails_the_safety_subset_is_a_failure_with_a_fixed_message() {
    for bad in [
        s("a<b"),
        s(""),
        s("a\u{200b}b"),
        s("a\u{202e}b"),
        s(&"x".repeat(41)),
        // A name of nothing but spaces is empty (it would be written on a device as a blank label).
        s(" "),
        s("    "),
        J::from(5),
    ] {
        let spy = spy_with(&[("name", bad.clone())]);
        let e = strict(&*spy).unwrap_err();
        assert!(e.starts_with(UNREADABLE), "{bad:?}: {e}");
        assert!(
            !e.contains('<') && !e.contains('\u{200b}') && !e.contains('\u{202e}'),
            "{e}"
        );
    }
    // A row with no name at all: the same.
    let spy = spy_with(&[("title_en", s("Hello"))]);
    assert!(strict(&*spy).unwrap_err().starts_with(UNREADABLE));
}

#[test]
fn another_field_that_fails_does_not_fail_strict() {
    let spy = spy_with(&[
        ("name", s("MOHV")),
        ("title_en", s("a<b")),
        ("slug", s("Foo/Bar")),
        ("logo_svg", s("<script>")),
    ]);
    assert_eq!(strict(&*spy).unwrap().name(), "MOHV");
}

#[test]
fn a_store_error_is_returned_unchanged_so_a_code_word_reaches_the_layer_above() {
    let spy = spy_with(&[("name", s("MOHV"))]);
    for text in [
        "storage_busy: the pool is full",
        "storage_unavailable: the connection broke",
        "storage_timeout: waited 5s",
        "a read error that is not transient",
    ] {
        spy.fail(MODEL, text);
        assert_eq!(strict(&*spy).unwrap_err(), text);
    }
}

#[test]
fn strict_reads_the_row_on_every_call_and_not_the_theme() {
    let spy = spy_with(&[("name", s("MOHV"))]);
    let (r, _) = reader(&spy, LONG);
    // After a successful current(): it does not answer from the cache.
    assert_eq!(r.current().brand().name(), "MOHV");
    let (rows, themes) = (spy.gets(MODEL), spy.gets(SETTING_MODEL));
    for n in 1..=3 {
        assert_eq!(r.strict().unwrap().name(), "MOHV");
        assert_eq!(strict(&*spy).unwrap().name(), "MOHV");
        assert_eq!(spy.gets(MODEL), rows + 2 * n);
    }
    assert_eq!(spy.gets(SETTING_MODEL), themes, "the theme is never read");
}

#[test]
fn strict_right_after_a_change_made_through_another_process_sees_the_new_name() {
    let spy = spy_with(&[("name", s("Old"))]);
    let (r, _) = reader(&spy, LONG);
    assert_eq!(r.current().brand().name(), "Old");
    put_row(&*spy, &[("name", s("New"))]);
    assert_eq!(
        r.current().brand().name(),
        "Old",
        "the cache still says the old name"
    );
    assert_eq!(
        r.strict().unwrap().name(),
        "New",
        "an enrolment never writes a stale name to a device"
    );
}

#[test]
fn a_failing_theme_read_does_not_refuse_strict() {
    let spy = spy_with(&[("name", s("MOHV"))]);
    spy.fail(SETTING_MODEL, "storage_timeout: waited 5s");
    assert_eq!(strict(&*spy).unwrap().name(), "MOHV");
}

#[test]
fn the_dispatcher_gives_an_inert_reader_until_register_admin_installs_one() {
    let d = Dispatcher::new();
    let inert = d.brand();
    assert!(inert.is_inert());
    assert_eq!(inert.current().brand().name(), "obop");
    assert_eq!(inert.strict().unwrap().name(), "obop");

    let spy = spy_with(&[("name", s("MOHV"))]);
    let installed = d.install_brand(spy.clone());
    assert!(!installed.is_inert());
    assert!(
        Arc::ptr_eq(&installed, &d.brand()),
        "one reader for the life of the dispatcher"
    );
    assert_eq!(d.brand().current().brand().name(), "MOHV");
    // A second call keeps the first reader.
    let other = d.install_brand(Spy::memory());
    assert!(Arc::ptr_eq(&installed, &other));
}

#[test]
fn fresh_reads_the_row_and_the_theme_on_every_call() {
    // What `get` uses: never the cache, so a `get` right after a `configure` made in another process
    // shows the new values.
    let spy = spy_with(&[("name", s("Old"))]);
    let (r, _) = reader(&spy, LONG);
    assert_eq!(r.current().brand().name(), "Old");
    let (rows, themes) = (spy.gets(MODEL), spy.gets(SETTING_MODEL));
    for n in 1..=3 {
        assert_eq!(r.fresh().unwrap().shown().brand().name(), "Old");
        assert_eq!(spy.gets(MODEL), rows + n);
        assert_eq!(spy.gets(SETTING_MODEL), themes + n);
    }
    put_row(&*spy, &[("name", s("New"))]);
    put_theme(&*spy, "#c2410c");
    let fresh = r.fresh().unwrap();
    assert_eq!(fresh.shown().brand().name(), "New");
    assert_eq!(fresh.shown().theme_accent(), Some("#c2410c"));
    assert_eq!(
        r.current().brand().name(),
        "Old",
        "while the cache still says the old name"
    );
}
