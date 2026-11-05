//! A store that fails, blocks, answers slowly or panics (docs/spec/branding.md, section 13, "The
//! display read and D74" and "A blocked store, a panic, hostile text"). The part under the real HTTP
//! layer (the 503 with `Retry-After` for the request that ran the read) is tested where the
//! handlers are; here is what the reader itself does.

use super::*;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

fn seeded() -> Arc<Spy> {
    let spy = Spy::memory();
    put_row(&*spy, &[("name", s("Acme"))]);
    spy
}

fn name(r: &Reader) -> String {
    r.current().brand().name().to_string()
}

/// A thread that asks the reader and sends the name it got, and when.
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
    // A hundred callers arrive while the refill is stuck in the store: none of them waits.
    let started = Instant::now();
    let callers: Vec<_> = (0..100).map(|_| asker(&r, &tx, "caller")).collect();
    for c in callers {
        c.join().unwrap();
    }
    assert!(started.elapsed() < Duration::from_secs(5));
    let mut got = Vec::new();
    while let Ok(x) = rx.try_recv() {
        got.push(x);
    }
    assert_eq!(
        got.len(),
        100,
        "only the callers answered, the owner is still parked"
    );
    assert!(got.iter().all(|(who, n)| *who == "caller" && n == "Acme"));
    assert_eq!(
        spy.gets(MODEL),
        2,
        "one read for the first fill, one parked refill: none for the callers"
    );
    spy.release();
    owner.join().unwrap();
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
    thread::sleep(Duration::from_millis(100));
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
    thread::sleep(Duration::from_millis(100));
    assert!(rx.try_recv().is_err());
    spy.release();
    owner.join().unwrap();
    for w in waiters {
        w.join().unwrap();
    }
    let got: Vec<_> = rx.try_iter().collect();
    assert!(got.iter().all(|(_, n)| n == "Acme"), "{got:?}");
    assert_eq!(spy.gets(MODEL), 1);
}

#[test]
fn the_first_caller_of_a_process_waits_the_pools_timeout_and_the_caller_behind_it_the_flight() {
    // The pool's own timeout error after a shortened bound: the first caller runs the refill and is
    // the request that waits for the store; the one behind it waits for the same flight and gets the
    // default when it ends.
    let spy = seeded();
    spy.slow_failure(
        MODEL,
        Duration::from_millis(400),
        "storage_timeout: waited 400ms for the statement",
    );
    let (r, _) = reader(&spy, LONG);
    let (tx, rx) = mpsc::channel();
    let begun = Instant::now();
    let owner = asker(&r, &tx, "owner");
    while spy.gets(MODEL) == 0 {
        thread::sleep(Duration::from_millis(1));
    }
    let arrived = Instant::now();
    let waiter = asker(&r, &tx, "waiter");
    owner.join().unwrap();
    waiter.join().unwrap();
    assert!(begun.elapsed() >= Duration::from_millis(400));
    assert!(
        arrived.elapsed() >= Duration::from_millis(200),
        "the waiter waited for the flight"
    );
    let got: Vec<_> = rx.try_iter().collect();
    assert_eq!(got.len(), 2);
    assert!(got.iter().all(|(_, n)| n == "obop"), "{got:?}");
    assert_eq!(spy.gets(MODEL), 1, "one flight read the store");
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
    thread::sleep(Duration::from_millis(100));
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
