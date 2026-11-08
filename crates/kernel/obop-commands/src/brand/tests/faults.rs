//! A store that fails, blocks or panics (docs/spec/branding.md, section 13, "The display read and
//! D74" and "A blocked store, a panic, hostile text"). The part under the real HTTP layer (the 503
//! with `Retry-After` for the request that ran the read) is tested where the handlers are; here is
//! what the reader itself does.
//!
//! Nothing here waits for a time to pass: a caller that has to be blocked behind the owner of a
//! flight is waited for as a condition (`wait_waiting`), and a caller that must not block is
//! collected through a channel with a timeout, so a regression fails the test and does not hang it.

use super::*;
use obop_storage::fault::{self, Fault};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc;
use std::thread;

fn seeded() -> Arc<Spy> {
    let spy = Spy::memory();
    put_row(&*spy, &[("name", s("Acme"))]);
    spy
}

fn name(r: &Reader) -> String {
    r.current().brand().name().to_string()
}

/// A thread that asks the reader and sends the name it got.
fn asker(
    r: &Arc<Reader>,
    out: &mpsc::Sender<(&'static str, String)>,
    who: &'static str,
) -> thread::JoinHandle<()> {
    let (r, out) = (r.clone(), out.clone());
    thread::spawn(move || {
        let n = name(&r);
        out.send((who, n)).unwrap();
    })
}

#[test]
fn callers_with_a_last_good_value_are_served_at_once_while_the_owner_is_parked() {
    let spy = seeded();
    let (r, _) = reader(&spy, LONG);
    assert_eq!(name(&r), "Acme");
    put_row(&*spy, &[("name", s("Zed"))]);
    r.expire();
    spy.park_next(MODEL);
    let (tx, rx) = mpsc::channel();
    let owner = asker(&r, &tx, "owner");
    spy.wait_parked(1);
    // A hundred callers arrive while the refill is stuck in the store: none of them waits. Their
    // answers are collected before the owner is let go, so a caller that waited fails the test
    // instead of hanging it.
    let callers: Vec<_> = (0..100).map(|_| asker(&r, &tx, "caller")).collect();
    let got = take(
        &rx,
        100,
        &spy,
        "a caller with a last good value waited for the refill",
    );
    assert!(got.iter().all(|(who, n)| *who == "caller" && n == "Acme"));
    assert_eq!(
        spy.gets(MODEL),
        2,
        "one read for the first fill, one parked refill: none for the callers"
    );
    spy.release();
    owner.join().unwrap();
    for c in callers {
        c.join().unwrap();
    }
    assert_eq!(rx.recv().unwrap(), ("owner", "Zed".to_string()));
    // After the release the next call is fresh.
    assert_eq!(name(&r), "Zed");
}

#[test]
fn in_a_process_that_never_had_a_good_read_a_caller_behind_the_owner_waits_and_gets_the_default() {
    let spy = seeded();
    spy.park_next(MODEL);
    spy.fail(MODEL, "storage_timeout: waited 5s for the statement");
    let (r, _) = reader(&spy, LONG);
    let (tx, rx) = mpsc::channel();
    let owner = asker(&r, &tx, "owner");
    spy.wait_parked(1);
    let waiters: Vec<_> = (0..5).map(|_| asker(&r, &tx, "waiter")).collect();
    wait_waiting(&r, 5);
    assert!(
        rx.try_recv().is_err(),
        "nobody is answered before the flight ends: a waiter has no value"
    );
    spy.release();
    owner.join().unwrap();
    for w in waiters {
        w.join().unwrap();
    }
    let got: Vec<_> = rx.try_iter().collect();
    assert_eq!(got.len(), 6);
    assert!(
        got.iter().all(|(_, n)| n == "obop"),
        "the default, for a failed refill: {got:?}"
    );
    assert_eq!(
        spy.gets(MODEL),
        1,
        "the waiters read nothing: they share the outcome of the flight"
    );
    // The failure was not kept.
    spy.heal();
    assert_eq!(name(&r), "Acme");
}

#[test]
fn the_waiters_of_a_good_flight_share_its_value() {
    let spy = seeded();
    spy.park_next(MODEL);
    let (r, _) = reader(&spy, LONG);
    let (tx, rx) = mpsc::channel();
    let owner = asker(&r, &tx, "owner");
    spy.wait_parked(1);
    let waiters: Vec<_> = (0..5).map(|_| asker(&r, &tx, "waiter")).collect();
    wait_waiting(&r, 5);
    assert!(rx.try_recv().is_err());
    spy.release();
    owner.join().unwrap();
    for w in waiters {
        w.join().unwrap();
    }
    let got: Vec<_> = rx.try_iter().collect();
    assert_eq!(got.len(), 6);
    assert!(got.iter().all(|(_, n)| n == "Acme"), "{got:?}");
    assert_eq!(spy.gets(MODEL), 1);
}

#[test]
fn the_first_caller_of_a_process_runs_the_flight_and_the_caller_behind_it_waits_for_it() {
    // The pool's own timeout error: the first caller runs the refill and is the request that waits
    // for the store; the one behind it waits for the same flight and gets the default when it ends.
    // The flight is held in the store by the double, however long that takes.
    let spy = seeded();
    spy.park_next(MODEL);
    spy.fail(MODEL, "storage_timeout: waited 5s for the statement");
    let (r, _) = reader(&spy, LONG);
    let (tx, rx) = mpsc::channel();
    let owner = asker(&r, &tx, "owner");
    spy.wait_parked(1);
    let waiter = asker(&r, &tx, "waiter");
    wait_waiting(&r, 1);
    assert!(rx.try_recv().is_err(), "the waiter waits for the flight");
    spy.release();
    owner.join().unwrap();
    waiter.join().unwrap();
    let got: Vec<_> = rx.try_iter().collect();
    assert_eq!(got.len(), 2);
    assert!(got.iter().all(|(_, n)| n == "obop"), "{got:?}");
    assert_eq!(spy.gets(MODEL), 1, "one flight read the store");
}

#[test]
fn a_caller_that_arrives_after_a_failed_flight_does_not_read_its_slot() {
    // The first flight of a process fails and leaves its failure in the slot. The store heals. The
    // refill lock is held again, not by a running flight but by a waiter that is reading that slot
    // (the test holds it). A caller that arrives now finds the cell stale and the lock taken: it
    // blocks, and when it gets the lock the slot is not its answer, since the flight ended before it
    // came. It reads the store, which has healed, and does not show the default of the failure.
    let spy = seeded();
    let (r, _) = reader(&spy, LONG);
    spy.fail(MODEL, "storage_unavailable: down");
    assert_eq!(name(&r), "obop", "the first flight of the process fails");
    spy.heal();
    assert_eq!(spy.gets(MODEL), 1);
    let held = r.hold_refill();
    let (tx, rx) = mpsc::channel();
    let late = asker(&r, &tx, "late");
    wait_waiting(&r, 1);
    drop(held);
    late.join().unwrap();
    assert_eq!(rx.recv().unwrap(), ("late", "Acme".to_string()));
    assert_eq!(spy.gets(MODEL), 2, "it read the healed store itself");
}

#[test]
fn a_panic_in_a_refill_poisons_the_mutex_but_the_next_call_refills() {
    // The refill mutex is poisoned by the panic. The next call gets `Poisoned` from `try_lock`,
    // takes it as the lock obtained, and refills: it is not taken for "a refill is running", or the
    // last good brand would be served for ever.
    let spy = seeded();
    let (r, _) = reader(&spy, LONG);
    assert_eq!(name(&r), "Acme");
    put_row(&*spy, &[("name", s("Zed"))]);
    r.expire();
    spy.panic_next(MODEL);
    let died = catch_unwind(AssertUnwindSafe(|| r.current()));
    assert!(died.is_err(), "the store double panicked inside the refill");
    assert_eq!(name(&r), "Zed", "the next call read the store again");
    assert_eq!(name(&r), "Zed");
    assert_eq!(spy.gets(MODEL), 3);
    // And again: the lock is not poisoned for good by a recovery.
    put_row(&*spy, &[("name", s("Yan"))]);
    r.expire();
    assert_eq!(name(&r), "Yan");
}

#[test]
fn a_panic_in_the_first_refill_of_a_process_leaves_no_slot_to_read() {
    let spy = seeded();
    let (r, _) = reader(&spy, LONG);
    spy.panic_next(MODEL);
    assert!(catch_unwind(AssertUnwindSafe(|| r.current())).is_err());
    assert_eq!(name(&r), "Acme");
}

#[test]
fn a_flight_clears_the_slot_of_the_one_before_it_so_a_flight_that_panics_leaves_none() {
    // The slot belongs to one flight. The first flight of the process fails and leaves its failure
    // there; the store heals; the next flight panics in the store. It must have cleared the failure
    // when it started: what is left is nothing, not the failure of the flight before.
    let spy = seeded();
    let (r, _) = reader(&spy, LONG);
    spy.fail(MODEL, "storage_unavailable: down");
    assert_eq!(name(&r), "obop");
    assert!(!r.slot_is_empty(), "the failed flight left its outcome");
    spy.heal();
    spy.panic_next(MODEL);
    assert!(catch_unwind(AssertUnwindSafe(|| r.current())).is_err());
    assert!(
        r.slot_is_empty(),
        "the flight that panicked did not leave the failure of the one before it"
    );
}

#[test]
fn a_waiter_behind_an_owner_that_panics_is_not_served_the_failure_of_the_flight_before() {
    let spy = seeded();
    let (r, _) = reader(&spy, LONG);
    spy.fail(MODEL, "storage_unavailable: down");
    assert_eq!(name(&r), "obop", "flight 1 fails");
    spy.heal();
    // Flight 2: its owner is parked in the store and panics when it is let go; a caller with no last
    // good value waits behind it.
    spy.park_next(MODEL);
    spy.panic_after_park();
    let owner = {
        let r = r.clone();
        thread::spawn(move || catch_unwind(AssertUnwindSafe(|| r.current())).is_err())
    };
    spy.wait_parked(1);
    let (tx, rx) = mpsc::channel();
    let waiter = asker(&r, &tx, "waiter");
    wait_waiting(&r, 1);
    spy.release();
    assert!(owner.join().unwrap(), "the owner panicked");
    waiter.join().unwrap();
    assert_eq!(
        rx.recv().unwrap(),
        ("waiter", "Acme".to_string()),
        "the waiter refilled; it was not given the failure of flight 1"
    );
    assert_eq!(
        spy.gets(MODEL),
        3,
        "flight 1, the panicked read, the waiter"
    );
}

#[test]
fn waiters_behind_an_owner_that_panics_refill_once_between_them() {
    // The owner is parked in the store and panics when it is let go. Three callers with no last good
    // value are waiting on the poisoned lock: the first to get it finds an empty slot and refills;
    // the others share that outcome.
    let spy = seeded();
    spy.park_next(MODEL);
    spy.panic_after_park();
    let (r, _) = reader(&spy, LONG);
    let owner = {
        let r = r.clone();
        thread::spawn(move || catch_unwind(AssertUnwindSafe(|| r.current())).is_err())
    };
    spy.wait_parked(1);
    let (tx, rx) = mpsc::channel();
    let waiters: Vec<_> = (0..3).map(|_| asker(&r, &tx, "waiter")).collect();
    wait_waiting(&r, 3);
    spy.release();
    assert!(owner.join().unwrap(), "the owner panicked");
    for w in waiters {
        w.join().unwrap();
    }
    let got: Vec<_> = rx.try_iter().collect();
    assert_eq!(got.len(), 3);
    assert!(got.iter().all(|(_, n)| n == "Acme"), "{got:?}");
    assert_eq!(
        spy.gets(MODEL),
        2,
        "the owner's read and one refill by the first waiter"
    );
}

#[test]
fn a_poisoned_cell_mutex_is_recovered() {
    let spy = seeded();
    let (r, _) = reader(&spy, LONG);
    assert_eq!(name(&r), "Acme");
    r.poison_cell();
    assert_eq!(name(&r), "Acme", "the hit path recovers the cell");
    r.expire();
    put_row(&*spy, &[("name", s("Zed"))]);
    assert_eq!(name(&r), "Zed", "the refill recovers it too");
    // `apply` takes the same mutex.
    crate::brand::apply(
        &*spy,
        Some(&r),
        "ada",
        &crate::brand::Action::Configure(vec![("name".into(), "Yan".into())]),
        false,
    )
    .unwrap();
    assert_eq!(name(&r), "Yan");
}

#[test]
fn a_store_that_fails_gives_the_error_to_get_and_never_not_set() {
    let spy = seeded();
    let (r, _) = reader(&spy, LONG);
    spy.fail(MODEL, "storage_unavailable: the connection broke");
    let err = r.fresh().err().expect("the read failed");
    assert_eq!(
        err, "storage_unavailable: the connection broke",
        "the text is unchanged"
    );
    spy.heal();
    spy.fail(SETTING_MODEL, "storage_busy: the pool is full");
    let err = r.fresh().err().expect("the theme read failed");
    assert!(err.starts_with("storage_busy"), "{err}");
    spy.heal();
    assert!(r.fresh().unwrap().set());
}

#[test]
fn the_display_read_is_not_detached_so_the_request_that_met_the_fault_carries_it() {
    // D74: the pool notes the fault on the thread, `ops::storage_faults` answers the request with
    // the 503, and `before_write` refuses its later writes. The reader must not hide that with
    // `fault::detached`, which would drop the fault the read noted.
    for with_last_good in [false, true] {
        let spy = seeded();
        spy.note_faults();
        let (r, _) = reader(&spy, LONG);
        if with_last_good {
            assert_eq!(name(&r), "Acme");
            r.expire();
        }
        spy.fail(MODEL, "storage_unavailable: the connection broke");
        fault::begin_request(fault::Outcome::default());
        let shown = r.current();
        let state = fault::state();
        let refused = fault::before_write();
        fault::end_request();
        assert_eq!(
            shown.brand().name(),
            if with_last_good { "Acme" } else { "obop" }
        );
        assert_eq!(
            state.fault,
            Some(Fault::Unavailable),
            "with_last_good={with_last_good}: the request carries the fault of the read"
        );
        assert!(
            refused.is_err(),
            "with_last_good={with_last_good}: its later writes are refused"
        );
    }
}

#[test]
fn a_sink_that_panics_cannot_lose_the_outcome_of_the_refill() {
    // The lines about a field that fell back are written after the outcome is stored and the refill
    // lock is let go. A sink that panics (the default one cannot) costs its caller the call and
    // nothing else: the value is in the cell, and the lock is not poisoned.
    let spy = Spy::memory();
    put_row(&*spy, &[("name", s("Acme")), ("title_en", s("a<b"))]);
    let sink: Sink = Arc::new(|_| panic!("a sink that panics"));
    let r = Arc::new(Reader::with_bound(spy.clone(), LONG).with_sink(sink));
    assert!(catch_unwind(AssertUnwindSafe(|| r.current())).is_err());
    assert_eq!(spy.gets(MODEL), 1);
    assert_eq!(name(&r), "Acme", "the outcome was stored before the line");
    assert_eq!(spy.gets(MODEL), 1, "and the call after it is a hit");
    assert!(
        !r.refill_is_poisoned(),
        "the refill lock was not held across the sink"
    );
}
