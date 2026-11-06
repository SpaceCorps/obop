//! The cache of the reader (docs/spec/branding.md, section 13, "Cache"): how often the store is
//! read, what a failed refill leaves, what `apply` does to the cell.

use super::*;
use crate::brand::{Action, apply};
use std::sync::Barrier;
use std::thread;

fn seeded() -> Arc<Spy> {
    let spy = Spy::memory();
    put_row(&*spy, &[("name", s("Acme"))]);
    put_theme(&*spy, "#c2410c");
    spy
}

fn reads(spy: &Spy) -> (usize, usize) {
    (spy.gets(MODEL), spy.gets(SETTING_MODEL))
}

fn name(r: &Reader) -> String {
    r.current().brand().name().to_string()
}

fn configure(spy: &Spy, r: Option<&Reader>, fields: &[(&str, &str)]) {
    let fields = fields
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    apply(spy, r, "ada", &Action::Configure(fields), false).unwrap();
}

#[test]
fn a_hundred_requests_within_the_bound_cause_one_read_of_the_row_and_one_of_the_theme() {
    let spy = seeded();
    let (r, _) = reader(&spy, LONG);
    for _ in 0..100 {
        assert_eq!(name(&r), "Acme");
    }
    assert_eq!(reads(&spy), (1, 1));
}

#[test]
fn a_hundred_concurrent_requests_at_expiry_cause_one_read() {
    // The store double answers at once, so a caller that read the stale time just before the
    // previous flight stored its outcome gets the lock right after it. The owner looks at the cell
    // again after it takes the lock and reads nothing: one expiry costs one read, not two. Repeated
    // so that the race is met.
    let spy = seeded();
    let (r, _) = reader(&spy, LONG);
    assert_eq!(name(&r), "Acme");
    for round in 1..=25 {
        r.expire();
        let barrier = Arc::new(Barrier::new(100));
        let threads: Vec<_> = (0..100)
            .map(|_| {
                let (r, b) = (r.clone(), barrier.clone());
                thread::spawn(move || {
                    b.wait();
                    name(&r)
                })
            })
            .collect();
        for t in threads {
            assert_eq!(t.join().unwrap(), "Acme");
        }
        assert_eq!(reads(&spy), (1 + round, 1 + round), "round {round}");
    }
}

#[test]
fn the_owner_of_a_refill_looks_at_the_cell_again_and_reads_nothing_when_a_flight_just_stored() {
    // A caller saw the cell stale; before it takes the refill lock another flight completes and
    // stores. The caller gets the lock at once (nobody holds it) and finds the cell fresh: no second
    // read. `after_stale` is the step of `current()` that follows the stale look.
    let spy = seeded();
    let (r, _) = reader(&spy, LONG);
    assert_eq!(name(&r), "Acme");
    put_row(&*spy, &[("name", s("Zed"))]);
    r.expire();
    // The other flight.
    assert_eq!(name(&r), "Zed", "the other flight read the new row");
    assert_eq!(reads(&spy), (2, 2));
    // The late caller, which had seen the stale time before that.
    let late = r.after_stale(&*spy);
    assert_eq!(late.brand().name(), "Zed");
    assert_eq!(reads(&spy), (2, 2), "the late owner read nothing");
}

#[test]
fn a_failed_refill_is_not_kept() {
    let spy = seeded();
    let (r, _) = reader(&spy, LONG);
    assert_eq!(name(&r), "Acme");
    put_row(&*spy, &[("name", s("Zed"))]);
    r.expire();
    spy.fail(MODEL, "storage_unavailable: the connection broke");
    // Each call after the failure starts a refill of its own, and each is answered with the last
    // good brand: nothing stale or default is pinned.
    for n in 1..=3 {
        assert_eq!(name(&r), "Acme");
        assert_eq!(
            spy.gets(MODEL),
            1 + n,
            "the failed read is repeated, not kept"
        );
    }
    spy.heal();
    assert_eq!(
        name(&r),
        "Zed",
        "the first call after the store recovers reads again"
    );
    let after = reads(&spy);
    assert_eq!(name(&r), "Zed");
    assert_eq!(reads(&spy), after, "and the good outcome is kept");
}

#[test]
fn a_refill_fails_as_a_whole_when_the_theme_read_fails() {
    let spy = seeded();
    let (r, _) = reader(&spy, LONG);
    assert_eq!(name(&r), "Acme");
    put_row(&*spy, &[("name", s("Zed"))]);
    r.expire();
    spy.fail(SETTING_MODEL, "storage_timeout: waited 5s");
    assert_eq!(
        name(&r),
        "Acme",
        "the new row is not shown with a theme that was not read"
    );
    assert_eq!(
        spy.gets(MODEL),
        2,
        "the row was read, and its outcome thrown away"
    );
    spy.heal();
    assert_eq!(name(&r), "Zed");
}

#[test]
fn the_default_is_the_answer_only_in_a_process_that_never_had_a_good_read() {
    let spy = seeded();
    spy.fail(MODEL, "storage_unavailable: down");
    let (r, _) = reader(&spy, LONG);
    let shown = r.current();
    assert!(!shown.is_set(), "no good read yet: the default");
    assert_eq!(shown.brand().name(), "obop");
    spy.heal();
    assert_eq!(name(&r), "Acme", "the failure was not kept");
    // Now there is a last good value, of any age: a later failure never shows the default.
    r.expire();
    spy.fail(MODEL, "storage_unavailable: down");
    for _ in 0..3 {
        assert_eq!(name(&r), "Acme");
    }
}

#[test]
fn the_reader_built_with_a_zero_bound_re_reads() {
    let spy = seeded();
    let (r, _) = reader(&spy, Duration::ZERO);
    for n in 1..=4 {
        assert_eq!(name(&r), "Acme");
        assert_eq!(reads(&spy), (n, n));
    }
    put_row(&*spy, &[("name", s("Zed"))]);
    assert_eq!(name(&r), "Zed");
}

#[test]
fn a_change_made_through_another_process_shows_after_the_bound() {
    // Another process on the same database writes the row: this process shows it when its cell
    // expires, and not before.
    let spy = seeded();
    let (r, _) = reader(&spy, LONG);
    assert_eq!(name(&r), "Acme");
    put_row(&*spy, &[("name", s("Zed"))]);
    assert_eq!(name(&r), "Acme");
    r.expire();
    assert_eq!(name(&r), "Zed");
}

#[test]
fn a_configure_in_the_same_process_shows_at_once_and_reads_nothing() {
    let spy = seeded();
    let (r, _) = reader(&spy, LONG);
    assert_eq!(name(&r), "Acme");
    configure(&spy, Some(&r), &[("name", "Zed"), ("title_sv", "Logga in")]);
    let before = reads(&spy);
    let shown = r.current();
    assert_eq!(shown.brand().name(), "Zed");
    assert_eq!(shown.brand().title("sv"), "Logga in");
    assert_eq!(reads(&spy), before, "no extra read");
    // The cell keeps its theme part: the tile still follows the accent that was read.
    assert_eq!(shown.theme_accent(), Some("#c2410c"));
    let tile = obop_brand::tile::svg("Zed", Some("#c2410c"), None);
    assert_eq!(shown.logo().unwrap().served(), tile);
    // And its age: it expires when the refill that filled it would have.
    r.expire();
    assert_eq!(name(&r), "Zed");
    assert_eq!(spy.gets(MODEL) - before.0, 1, "one refill, when it expires");
}

#[test]
fn apply_replaces_the_brand_in_the_cell_and_leaves_its_age_alone() {
    // The cell was filled a second ago, with the bound of two. A change now is shown at once, and
    // the cell still expires when it would have: after a second more the next call reads the store,
    // which a change that renewed the age would have put off by another second. (The age is moved
    // by the test, not waited for.)
    let spy = seeded();
    let r = Arc::new(Reader::new(spy.clone()));
    assert_eq!(name(&r), "Acme");
    r.back_date(Duration::from_secs(1));
    configure(&spy, Some(&r), &[("name", "Zed")]);
    let before = spy.gets(MODEL);
    assert_eq!(name(&r), "Zed");
    assert_eq!(spy.gets(MODEL), before, "shown at once, with no read");
    r.back_date(Duration::from_millis(1500));
    assert_eq!(name(&r), "Zed");
    assert_eq!(
        spy.gets(MODEL),
        before + 1,
        "the cell expired on the age of its refill"
    );
}

#[test]
fn the_bound_is_two_seconds() {
    assert_eq!(super::super::reader::DEFAULT_BOUND, Duration::from_secs(2));
    let spy = seeded();
    let r = Arc::new(Reader::new(spy.clone()));
    assert_eq!(name(&r), "Acme");
    put_row(&*spy, &[("name", s("Zed"))]);
    r.back_date(Duration::from_secs(1));
    assert_eq!(name(&r), "Acme", "a second old: still the cell");
    r.back_date(Duration::from_millis(1500));
    assert_eq!(name(&r), "Zed", "two and a half seconds old: read again");
}

#[test]
fn a_slow_read_does_not_keep_the_row_longer_than_the_bound() {
    // The age of what a refill stores is counted from the start of its read. A read that is held in
    // the store for longer than the bound has stored a value that is already stale, so the next
    // call reads again and a change made meanwhile shows at once.
    let spy = seeded();
    let (r, _) = reader(&spy, Duration::from_millis(100));
    spy.park_next(MODEL);
    let owner = {
        let r = r.clone();
        thread::spawn(move || name(&r))
    };
    spy.wait_parked(1);
    put_row(&*spy, &[("name", s("Zed"))]);
    // Time passes in the store: the bound is gone before the read returns.
    thread::sleep(Duration::from_millis(300));
    spy.release();
    assert_eq!(owner.join().unwrap(), "Acme", "it read the row before");
    assert_eq!(name(&r), "Zed", "and what it stored was stale on arrival");
}

#[test]
fn a_refill_that_fails_after_a_change_serves_the_brand_that_was_just_written() {
    let spy = seeded();
    let (r, _) = reader(&spy, LONG);
    assert_eq!(name(&r), "Acme");
    configure(&spy, Some(&r), &[("name", "Zed")]);
    r.expire();
    spy.fail(MODEL, "storage_unavailable: down");
    assert_eq!(name(&r), "Zed", "never the one it replaced");
    // After a clear it is the default, never the one that was cleared.
    spy.heal();
    apply(&*spy, Some(&r), "ada", &Action::Clear, false).unwrap();
    r.expire();
    spy.fail(MODEL, "storage_unavailable: down");
    let shown = r.current();
    assert!(!shown.is_set());
    assert_eq!(shown.brand().name(), "obop");
    assert!(shown.logo().is_none());
}

#[test]
fn a_process_that_has_not_read_yet_has_nothing_to_replace_and_its_first_read_sees_the_new_row() {
    let spy = seeded();
    let (r, _) = reader(&spy, LONG);
    configure(&spy, Some(&r), &[("name", "Zed")]);
    let before = reads(&spy);
    assert_eq!(name(&r), "Zed");
    assert_eq!(
        reads(&spy),
        (before.0 + 1, before.1 + 1),
        "the first read reads the new row"
    );
}

#[test]
fn the_counter_moves_only_with_a_change_that_committed() {
    let spy = seeded();
    let (r, _) = reader(&spy, LONG);
    assert_eq!(r.generation(), 0);
    configure(&spy, Some(&r), &[("name", "Zed")]);
    assert_eq!(r.generation(), 1);
    // The same values again: nothing changes, nothing is bumped.
    configure(&spy, Some(&r), &[("name", "Zed")]);
    assert_eq!(r.generation(), 1);
    // A refused call, a dry run and a batch the store refuses leave it alone.
    assert!(
        apply(
            &*spy,
            Some(&r),
            "ada",
            &Action::Configure(vec![("name".into(), "a<b".into())]),
            false
        )
        .is_err()
    );
    apply(
        &*spy,
        Some(&r),
        "ada",
        &Action::Configure(vec![("name".into(), "Dry".into())]),
        true,
    )
    .unwrap();
    assert_eq!(r.generation(), 1);
    spy.refuse_next_apply("storage_busy: the pool is full");
    let e = apply(
        &*spy,
        Some(&r),
        "ada",
        &Action::Configure(vec![("name".into(), "Busy".into())]),
        false,
    )
    .unwrap_err();
    assert_eq!(e.to_string(), "storage_busy: the pool is full");
    assert_eq!(
        r.generation(),
        1,
        "a batch that fails to commit leaves the cell as it was"
    );
    assert_eq!(name(&r), "Zed");
}

#[test]
fn an_older_refill_cannot_store_after_apply() {
    // A refill that read the old row before a change and finishes after it gives its outcome to its
    // own caller and keeps nothing, so it cannot put the replaced brand back.
    for filled in [false, true] {
        let spy = seeded();
        let (r, _) = reader(&spy, LONG);
        if filled {
            assert_eq!(name(&r), "Acme");
            r.expire();
        }
        spy.park_next(MODEL);
        let older = {
            let r = r.clone();
            thread::spawn(move || name(&r))
        };
        spy.wait_parked(1);
        // The change commits while the refill holds the old row.
        configure(&spy, Some(&r), &[("name", "Zed")]);
        spy.release();
        assert_eq!(
            older.join().unwrap(),
            "Acme",
            "its own caller got what it read"
        );
        assert_eq!(
            name(&r),
            "Zed",
            "filled={filled}: the older refill kept nothing"
        );
        assert_eq!(name(&r), "Zed");
    }
}

#[test]
fn a_caller_that_waited_for_a_flight_that_apply_overtook_does_not_get_the_replaced_brand() {
    // A process that never read: its first flight reads the old row, `apply` commits meanwhile, and
    // a caller that arrives now has no last good value. It must not be handed the old row.
    let spy = seeded();
    let (r, _) = reader(&spy, LONG);
    spy.park_next(MODEL);
    let owner = {
        let r = r.clone();
        thread::spawn(move || name(&r))
    };
    spy.wait_parked(1);
    configure(&spy, Some(&r), &[("name", "Zed")]);
    let waiter = {
        let r = r.clone();
        thread::spawn(move || name(&r))
    };
    // The waiter is about to block behind the owner: a condition, not a time.
    wait_waiting(&r, 1);
    spy.release();
    assert_eq!(owner.join().unwrap(), "Acme");
    assert_eq!(waiter.join().unwrap(), "Zed");
}

#[test]
fn a_refill_that_fails_after_an_overlapping_change_serves_the_brand_just_written() {
    // The refill read the row, a change committed while it was in the store, and then it failed:
    // its caller gets the brand that was just written (after a clear, the default), not the one
    // that was in the cell when the read began.
    for clear in [false, true] {
        let spy = seeded();
        let (r, _) = reader(&spy, LONG);
        assert_eq!(name(&r), "Acme");
        r.expire();
        spy.park_next(MODEL);
        let owner = {
            let r = r.clone();
            thread::spawn(move || r.current())
        };
        spy.wait_parked(1);
        if clear {
            apply(&*spy, Some(&r), "ada", &Action::Clear, false).unwrap();
        } else {
            configure(&spy, Some(&r), &[("name", "Zed")]);
        }
        spy.fail(MODEL, "storage_unavailable: down");
        spy.release();
        let served = owner.join().unwrap();
        if clear {
            assert!(!served.is_set(), "clear={clear}: never the cleared brand");
            assert_eq!(served.brand().name(), "obop");
        } else {
            assert_eq!(
                served.brand().name(),
                "Zed",
                "clear={clear}: never the replaced brand"
            );
        }
    }
}

#[test]
fn the_accent_of_a_cell_filled_with_no_row_survives_the_first_configure_and_a_clear() {
    // The theme part of the cell is what its refill read. With no row the brand is the default and
    // the accent is still kept, so the tile of the first `configure` and of what follows a `clear`
    // has the colour of the theme.
    let spy = Spy::memory();
    put_theme(&*spy, "#c2410c");
    let (r, _) = reader(&spy, LONG);
    let shown = r.current();
    assert!(!shown.is_set());
    assert_eq!(shown.theme_accent(), Some("#c2410c"), "kept with no row");
    configure(&spy, Some(&r), &[("name", "Zed")]);
    let shown = r.current();
    assert_eq!(shown.theme_accent(), Some("#c2410c"));
    assert_eq!(
        shown.logo().unwrap().served(),
        obop_brand::tile::svg("Zed", Some("#c2410c"), None)
    );
    apply(&*spy, Some(&r), "ada", &Action::Clear, false).unwrap();
    let shown = r.current();
    assert!(!shown.is_set());
    assert_eq!(shown.theme_accent(), Some("#c2410c"), "kept after a clear");
}

#[test]
fn apply_builds_its_value_with_the_cell_free_and_keeps_a_refill_that_stored_meanwhile() {
    // `applied` bumps the counter, then builds the new value with no lock held, then puts it in. The
    // test runs a whole refill, on another thread, in the gap between: it must not be blocked by
    // `applied`, it read the new row (the batch has committed) and a theme change made just before,
    // and `applied` must not put a value that was built from the older cell over it.
    let spy = seeded();
    let (r, _) = reader(&spy, LONG);
    assert_eq!(name(&r), "Acme");
    assert_eq!(r.current().theme_accent(), Some("#c2410c"));
    let (hook_spy, hook_reader) = (spy.clone(), r.clone());
    r.on_applied_gap(Arc::new(move || {
        put_theme(&*hook_spy, "#0f766e");
        hook_reader.expire();
        let (tx, rx) = std::sync::mpsc::channel();
        let me = hook_reader.clone();
        thread::spawn(move || tx.send(me.current()).unwrap());
        let shown = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("a refill in the gap was blocked: the cell was held while the value was built");
        assert_eq!(shown.brand().name(), "Zed", "it read the committed row");
        assert_eq!(shown.theme_accent(), Some("#0f766e"), "and the new accent");
    }));
    configure(&spy, Some(&r), &[("name", "Zed")]);
    let shown = r.current();
    assert_eq!(shown.brand().name(), "Zed");
    assert_eq!(
        shown.theme_accent(),
        Some("#0f766e"),
        "the value the refill stored stays: the older accent of the cell is not put over it"
    );
}

#[test]
fn a_refill_that_began_before_a_change_cannot_store_once_the_counter_moved() {
    // The counter moves before the new value is built, not after: a refill that began before the
    // change and stores in the gap is refused, and `applied` then puts the new value in.
    let spy = seeded();
    let (r, _) = reader(&spy, LONG);
    assert_eq!(name(&r), "Acme");
    r.expire();
    spy.park_next(MODEL);
    let older = {
        let r = r.clone();
        thread::spawn(move || name(&r))
    };
    spy.wait_parked(1);
    let hook_spy = spy.clone();
    let hook_older = std::sync::Mutex::new(Some(older));
    r.on_applied_gap(Arc::new(move || {
        // The older refill finishes here, in the gap: it read the old row.
        hook_spy.release();
        let older = hook_older.lock().unwrap().take().unwrap();
        assert_eq!(older.join().unwrap(), "Acme");
    }));
    configure(&spy, Some(&r), &[("name", "Zed")]);
    assert_eq!(name(&r), "Zed", "the older refill kept nothing");
}

#[test]
fn an_inert_reader_answers_the_default_and_reads_nothing() {
    let r = Reader::inert();
    assert!(r.is_inert());
    let shown = r.current();
    assert!(!shown.is_set());
    assert_eq!(shown.brand().name(), "obop");
    assert_eq!(r.strict().unwrap().name(), "obop");
    let fresh = r.fresh().unwrap();
    assert!(!fresh.set());
    assert!(!fresh.shown().is_set());
}
