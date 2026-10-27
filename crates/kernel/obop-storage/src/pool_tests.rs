//! The pool without a database: a fake manager makes numbered connections that can be told to
//! fail, to be slow, or to be dead when pinged.
//!
//! Timing assertions are wide on purpose (the tests run on shared, busy machines): they prove an
//! order of magnitude, not a benchmark. Tests that are not about opening connections seed the
//! pool, so no helper thread has to be scheduled before the clock starts.

use crate::fault::{self, Fault};
use crate::pool::*;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering::SeqCst};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Fake {
    made: AtomicUsize,
    pings: AtomicUsize,
    fail: AtomicBool,
    delay_ms: AtomicU64,
    dead: Mutex<HashSet<usize>>,
    /// The next connect panics (once).
    panic_connect: AtomicBool,
    /// A ping hangs for as long as it is allowed to, then reports failure.
    hang_ping: AtomicBool,
}

impl Manager for Arc<Fake> {
    type Conn = usize;
    fn connect(&self) -> Result<usize, String> {
        std::thread::sleep(Duration::from_millis(self.delay_ms.load(SeqCst)));
        if self.panic_connect.swap(false, SeqCst) {
            panic!("the driver could not build its runtime");
        }
        if self.fail.load(SeqCst) {
            return Err("connection refused".into());
        }
        Ok(self.made.fetch_add(1, SeqCst))
    }
    fn ping(&self, conn: &mut usize, timeout: Duration) -> bool {
        self.pings.fetch_add(1, SeqCst);
        if self.hang_ping.load(SeqCst) {
            std::thread::sleep(timeout);
            return false;
        }
        !self.dead.lock().unwrap().contains(conn)
    }
}

fn settings(name: &'static str, size: usize, acquire: Duration) -> Settings {
    let mut s = Settings::new(name, size);
    s.acquire_timeout = acquire;
    s.health_check_idle = Duration::from_secs(3600);
    s.connect_backoff = Duration::from_millis(50);
    s
}

/// A pool whose connections are made on demand; the timeout is generous.
fn pool(name: &'static str, size: usize) -> (Pool<Arc<Fake>>, Arc<Fake>) {
    let fake = Arc::new(Fake::default());
    (
        Pool::new(fake.clone(), settings(name, size, Duration::from_secs(5))),
        fake,
    )
}

/// A pool that already holds `size` idle connections (ids 100, 101, ...).
fn seeded(name: &'static str, size: usize, acquire: Duration) -> Pool<Arc<Fake>> {
    seeded_with(settings(name, size, acquire))
}

fn seeded_with(cfg: Settings) -> Pool<Arc<Fake>> {
    let size = cfg.max_size;
    let p = Pool::new(Arc::new(Fake::default()), cfg);
    for i in 0..size {
        p.seed(100 + i);
    }
    p
}

fn mine(name: &str) -> PoolStats {
    stats().into_iter().find(|s| s.name == name).unwrap()
}

fn wait_for(what: &str, mut f: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(10);
    while !f() {
        assert!(Instant::now() < until, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn connections_are_opened_on_demand_up_to_the_maximum_and_reused() {
    let (p, fake) = pool("t-demand", 3);
    assert_eq!(fake.made.load(SeqCst), 0, "nothing is opened in advance");
    let a = p.get(false).unwrap();
    let b = p.get(false).unwrap();
    let c = p.get(false).unwrap();
    assert!(!a.reused() && !b.reused() && !c.reused(), "all fresh");
    assert_eq!(fake.made.load(SeqCst), 3);
    let s = mine("t-demand");
    assert_eq!((s.in_use, s.idle, s.max_size), (3, 0, 3));
    let id_b = *b;
    drop(b);
    let again = p.get(false).unwrap();
    assert_eq!(*again, id_b, "the connection that came back is reused");
    assert!(again.reused());
    assert_eq!(fake.made.load(SeqCst), 3, "no fourth connection");
    drop((a, c, again));
    let s = mine("t-demand");
    assert_eq!((s.in_use, s.idle), (0, 3));
}

#[test]
fn exhaustion_answers_busy_after_the_acquire_timeout_and_notes_the_fault() {
    let p = seeded("t-exhaust", 2, Duration::from_millis(200));
    let _a = p.get(false).unwrap();
    let _b = p.get(false).unwrap();
    fault::clear();
    let t = Instant::now();
    let r = p.get(false);
    let took = t.elapsed();
    assert_eq!(r.err(), Some(PoolError::Timeout));
    assert!(
        took >= Duration::from_millis(190) && took < Duration::from_millis(1500),
        "{took:?}"
    );
    assert_eq!(fault::take(), Some(Fault::Busy));
    let m = PoolError::Timeout.message();
    assert!(m.starts_with("storage_busy:"), "{m}");
    assert_eq!(Fault::of_message(&m), Some(Fault::Busy));
    let s = mine("t-exhaust");
    assert_eq!(s.timeouts, 1);
    assert_eq!(s.waiting, 0, "a caller that gave up no longer waits");
}

#[test]
fn the_wait_queue_is_bounded_and_the_surplus_is_refused_at_once() {
    let mut cfg = settings("t-queue", 1, Duration::from_secs(8));
    cfg.max_waiters = 3;
    let p = seeded_with(cfg);
    let held = p.get(false).unwrap();
    let results = Arc::new(Mutex::new(Vec::new()));
    let mut threads = Vec::new();
    for _ in 0..10 {
        let (p, results) = (p.clone(), results.clone());
        threads.push(std::thread::spawn(move || {
            let t = Instant::now();
            let r = p.get(false).map(|c| {
                std::thread::sleep(Duration::from_millis(20));
                drop(c);
            });
            results.lock().unwrap().push((r, t.elapsed()));
        }));
    }
    // Everyone arrives: three wait, seven are refused without waiting.
    wait_for("the surplus to be refused", || {
        mine("t-queue").rejected == 7
    });
    let s = mine("t-queue");
    assert_eq!(s.waiting, 3, "never more waiters than the bound");
    drop(held);
    for t in threads {
        t.join().unwrap();
    }
    let res = results.lock().unwrap();
    let refused: Vec<_> = res
        .iter()
        .filter(|(r, _)| matches!(r, Err(PoolError::QueueFull)))
        .collect();
    let served = res.iter().filter(|(r, _)| r.is_ok()).count();
    assert_eq!(refused.len(), 7);
    assert_eq!(served, 3, "the three who waited were served in turn");
    assert!(
        refused
            .iter()
            .all(|(_, took)| *took < Duration::from_secs(4)),
        "refusals do not wait out the timeout"
    );
    assert_eq!(mine("t-queue").waiting, 0);
    assert_eq!(
        PoolError::QueueFull.fault(),
        Fault::Busy,
        "a full queue is also storage_busy"
    );
}

#[test]
fn a_connection_that_comes_back_wakes_a_waiter() {
    let p = seeded("t-wake", 1, Duration::from_secs(8));
    let held = p.get(false).unwrap();
    let waiter = {
        let p = p.clone();
        std::thread::spawn(move || {
            let t = Instant::now();
            let c = p.get(false).unwrap();
            (*c, t.elapsed())
        })
    };
    wait_for("the waiter to wait", || mine("t-wake").waiting == 1);
    let id = *held;
    drop(held);
    let (got, waited) = waiter.join().unwrap();
    assert_eq!(got, id);
    assert!(waited < Duration::from_secs(5), "{waited:?}");
}

#[test]
fn many_threads_share_a_small_pool_and_never_exceed_it() {
    let fake = Arc::new(Fake::default());
    let mut cfg = settings("t-share", 4, Duration::from_secs(30));
    cfg.max_waiters = 1000;
    let p = Pool::new(fake.clone(), cfg);
    let inside = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    std::thread::scope(|s| {
        for _ in 0..32 {
            let (p, inside, peak) = (p.clone(), inside.clone(), peak.clone());
            s.spawn(move || {
                for _ in 0..40 {
                    let c = p.get(false).unwrap();
                    let now = inside.fetch_add(1, SeqCst) + 1;
                    peak.fetch_max(now, SeqCst);
                    std::thread::yield_now();
                    inside.fetch_sub(1, SeqCst);
                    drop(c);
                }
            });
        }
    });
    assert!(peak.load(SeqCst) <= 4, "{}", peak.load(SeqCst));
    assert!(fake.made.load(SeqCst) <= 4, "{}", fake.made.load(SeqCst));
    let s = mine("t-share");
    assert_eq!(s.in_use, 0);
    assert_eq!(s.acquire_count, 32 * 40);
    assert_eq!(s.timeouts + s.rejected, 0);
    let in_buckets: u64 = s.acquire_buckets.iter().sum();
    assert!(in_buckets <= s.acquire_count);
}

#[test]
fn a_broken_connection_and_a_panicking_holder_are_never_put_back() {
    let (p, fake) = pool("t-broken", 2);
    let mut a = p.get(false).unwrap();
    let id = *a;
    a.discard();
    drop(a);
    assert_eq!(mine("t-broken").idle, 0);
    let b = p.get(false).unwrap();
    assert_ne!(*b, id, "a new connection, not the discarded one");
    drop(b);
    // A holder that panics: the connection is in an unknown state and is closed.
    let before = fake.made.load(SeqCst);
    let p2 = p.clone();
    let r = std::thread::spawn(move || {
        let _c = p2.get(false).unwrap();
        panic!("holder dies");
    })
    .join();
    assert!(r.is_err());
    let s = mine("t-broken");
    assert_eq!((s.in_use, s.idle), (0, 0), "the panicking holder's is gone");
    let _kept = p.get(false).unwrap();
    let c = p.get(false).unwrap();
    assert!(!c.reused());
    assert_eq!(fake.made.load(SeqCst), before + 2, "two new connections");
}

#[test]
fn idle_connections_are_closed_after_the_idle_timeout() {
    let fake = Arc::new(Fake::default());
    let mut cfg = settings("t-idle", 4, Duration::from_secs(5));
    cfg.idle_timeout = Duration::from_millis(200);
    let p = Pool::new(fake.clone(), cfg);
    let (a, b) = (p.get(false).unwrap(), p.get(false).unwrap());
    drop((a, b));
    assert_eq!(mine("t-idle").idle, 2);
    wait_for("the sweeper to close the idle connections", || {
        mine("t-idle").idle == 0
    });
    assert_eq!(mine("t-idle").connections_closed, 2);
    // And the pool still works: it opens a new one.
    let c = p.get(false).unwrap();
    assert!(!c.reused());
    assert_eq!(fake.made.load(SeqCst), 3);
}

#[test]
fn a_connection_past_its_lifetime_is_closed_instead_of_reused() {
    let fake = Arc::new(Fake::default());
    let mut cfg = settings("t-life", 2, Duration::from_secs(5));
    cfg.max_lifetime = Duration::from_millis(100);
    let p = Pool::new(fake.clone(), cfg);
    let a = p.get(false).unwrap();
    std::thread::sleep(Duration::from_millis(250));
    drop(a);
    assert_eq!(mine("t-life").idle, 0, "too old to go back");
    let b = p.get(false).unwrap();
    assert!(!b.reused());
    assert_eq!(fake.made.load(SeqCst), 2);
}

#[test]
fn the_health_check_replaces_a_dead_idle_connection_on_checkout() {
    let fake = Arc::new(Fake::default());
    let mut cfg = settings("t-health", 3, Duration::from_secs(5));
    cfg.health_check_idle = Duration::ZERO;
    let p = Pool::new(fake.clone(), cfg);
    let (a, b) = (p.get(false).unwrap(), p.get(false).unwrap());
    let (ia, ib) = (*a, *b);
    drop((a, b));
    // The server restarted: both idle connections are dead.
    fake.dead.lock().unwrap().extend([ia, ib]);
    let c = p.get(false).unwrap();
    assert!(*c != ia && *c != ib, "a fresh connection was made");
    assert_eq!(
        fake.pings.load(SeqCst),
        1,
        "the other dead one was dropped too, not pinged one by one"
    );
    assert_eq!(mine("t-health").idle, 0);
    drop(c);
}

#[test]
fn a_recently_used_connection_is_not_pinged() {
    let (p, fake) = pool("t-noping", 2);
    drop(p.get(false).unwrap());
    for _ in 0..5 {
        drop(p.get(false).unwrap());
    }
    assert_eq!(fake.pings.load(SeqCst), 0);
}

#[test]
fn an_unreachable_database_is_unavailable_before_the_timeout_and_the_pool_recovers() {
    let fake = Arc::new(Fake::default());
    let p = Pool::new(
        fake.clone(),
        settings("t-outage", 3, Duration::from_secs(4)),
    );
    fake.fail.store(true, SeqCst);
    fault::clear();
    let t = Instant::now();
    let r = p.get(false);
    assert!(matches!(r, Err(PoolError::Unavailable(_))), "{:?}", r.err());
    assert!(
        t.elapsed() < Duration::from_secs(3),
        "a refused connect is reported when it happens, not after the timeout: {:?}",
        t.elapsed()
    );
    assert_eq!(fault::take(), Some(Fault::Unavailable));
    let m = r.err().unwrap().message();
    assert_eq!(Fault::of_message(&m), Some(Fault::Unavailable), "{m}");
    // While the database is down callers keep getting a quick answer, and the pool does not
    // hammer it: attempts are spaced by the backoff.
    for _ in 0..5 {
        assert!(matches!(p.get(false), Err(PoolError::Unavailable(_))));
    }
    assert!(mine("t-outage").connect_failures < 6);
    fake.fail.store(false, SeqCst);
    let deadline = Instant::now() + Duration::from_secs(10);
    let c = loop {
        match p.get(false) {
            Ok(c) => break c,
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => panic!("did not recover: {e}"),
        }
    };
    drop(c);
    assert!(p.get(false).is_ok());
}

#[test]
fn a_slow_connect_does_not_hold_the_caller_past_its_timeout() {
    let fake = Arc::new(Fake::default());
    fake.delay_ms.store(1500, SeqCst);
    let p = Pool::new(
        fake.clone(),
        settings("t-slow", 2, Duration::from_millis(300)),
    );
    let t = Instant::now();
    let r = p.get(false);
    assert_eq!(r.err(), Some(PoolError::Timeout));
    assert!(
        t.elapsed() < Duration::from_millis(1400),
        "{:?}",
        t.elapsed()
    );
    // The connection that was being opened is not lost: it arrives and serves the next caller,
    // which is not given a second one.
    fake.delay_ms.store(0, SeqCst);
    wait_for("the slow connection to arrive", || mine("t-slow").idle == 1);
    let c = p.get(false).unwrap();
    assert_eq!(*c, 0, "the very connection that was being opened");
    assert_eq!(fake.made.load(SeqCst), 1, "and no other was made for it");
}

#[test]
fn a_panic_while_connecting_gives_the_slot_back() {
    let fake = Arc::new(Fake::default());
    let p = Pool::new(
        fake.clone(),
        settings("t-panic", 1, Duration::from_millis(500)),
    );
    fake.panic_connect.store(true, SeqCst);
    // The only slot was reserved for a connect that panicked: the caller is told the database
    // cannot be reached, not left to wait for a slot that never comes back.
    let r = p.get(false);
    assert!(matches!(r, Err(PoolError::Unavailable(_))), "{:?}", r.err());
    // ... and the next caller gets that slot.
    let deadline = Instant::now() + Duration::from_secs(5);
    let c = loop {
        match p.get(false) {
            Ok(c) => break c,
            Err(e) if Instant::now() < deadline => {
                let _ = e;
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => panic!("the slot was never given back: {e}"),
        }
    };
    drop(c);
    let s = mine("t-panic");
    assert_eq!(s.connect_failures, 1);
    assert_eq!(s.in_use, 0);
}

#[test]
fn a_waiter_tries_again_when_the_pause_after_a_failed_connect_is_over() {
    let fake = Arc::new(Fake::default());
    let mut cfg = settings("t-retry", 2, Duration::from_secs(5));
    cfg.connect_backoff = Duration::from_millis(100);
    let p = Pool::new(fake.clone(), cfg);
    // One connection is out, so the waiter does not give up on the first failed attempt ...
    let held = p.get(false).unwrap();
    fake.fail.store(true, SeqCst);
    let waiter = {
        let p = p.clone();
        std::thread::spawn(move || {
            let t = Instant::now();
            let r = p.get(false).map(|c| *c);
            (r, t.elapsed())
        })
    };
    wait_for("the first attempt to fail", || {
        mine("t-retry").connect_failures >= 1
    });
    // ... the database comes back, and nothing else happens in the pool: no connection is
    // returned, no new caller arrives. The waiter must wake by itself and open one.
    fake.fail.store(false, SeqCst);
    let (r, took) = waiter.join().unwrap();
    assert!(r.is_ok(), "{r:?}");
    assert!(took < Duration::from_secs(3), "{took:?}");
    drop(held);
}

#[test]
fn a_seeded_connection_is_checked_like_any_other_that_sat_idle() {
    let fake = Arc::new(Fake::default());
    let mut cfg = settings("t-seedping", 2, Duration::from_secs(5));
    cfg.health_check_idle = Duration::ZERO;
    let p = Pool::new(fake.clone(), cfg);
    p.seed(77);
    fake.dead.lock().unwrap().insert(77);
    let c = p.get(false).unwrap();
    assert_ne!(*c, 77, "the dead seeded connection was replaced");
    assert_eq!(fake.pings.load(SeqCst), 1);
    // And a read that finds a seeded connection dead may be repeated: it counts as having served.
    let p2 = Pool::new(
        fake.clone(),
        settings("t-seedreuse", 1, Duration::from_secs(5)),
    );
    p2.seed(5);
    assert!(p2.get(false).unwrap().reused());
}

#[test]
fn a_hanging_ping_holds_the_caller_no_longer_than_its_time_allows() {
    let fake = Arc::new(Fake::default());
    let mut cfg = settings("t-ping", 2, Duration::from_millis(400));
    cfg.health_check_idle = Duration::ZERO;
    let p = Pool::new(fake.clone(), cfg);
    drop(p.get(false).unwrap());
    fake.hang_ping.store(true, SeqCst);
    let t = Instant::now();
    let _ = p.get(false);
    assert!(
        t.elapsed() < Duration::from_millis(1500),
        "the ping was allowed three seconds although the caller had 400 ms: {:?}",
        t.elapsed()
    );
}

#[test]
fn long_reads_are_capped_and_leave_room_for_interactive_callers() {
    let mut cfg = settings("t-long", 4, Duration::from_millis(200));
    cfg.max_long = 2;
    let p = seeded_with(cfg);
    let l1 = p.get(true).unwrap();
    let l2 = p.get(true).unwrap();
    // A third long read is refused although two connections are free ...
    assert_eq!(p.get(true).err(), Some(PoolError::Timeout));
    // ... and interactive callers still get both of them.
    let i1 = p.get(false).unwrap();
    let i2 = p.get(false).unwrap();
    drop((i1, i2));
    // Giving a long connection back frees its permit, also when it was discarded.
    let mut l1 = l1;
    l1.discard();
    drop(l1);
    let l3 = p.get(true).unwrap();
    drop((l2, l3));
    // A long caller that gave up did not keep a permit.
    let a = p.get(true).unwrap();
    let b = p.get(true).unwrap();
    assert!(p.get(true).is_err());
    drop((a, b));
    assert!(p.get(true).is_ok());
}

#[test]
fn purge_idle_closes_every_idle_connection_and_frees_the_room() {
    let (p, fake) = pool("t-purge", 3);
    let (a, b, c) = (
        p.get(false).unwrap(),
        p.get(false).unwrap(),
        p.get(false).unwrap(),
    );
    drop((a, b, c));
    assert_eq!(mine("t-purge").idle, 3);
    p.purge_idle();
    let s = mine("t-purge");
    assert_eq!((s.idle, s.connections_closed), (0, 3));
    let d = p.get(false).unwrap();
    assert!(!d.reused());
    assert_eq!(fake.made.load(SeqCst), 4);
}

#[test]
fn a_seeded_connection_serves_first() {
    let (p, fake) = pool("t-seed", 2);
    p.seed(99);
    let c = p.get(false).unwrap();
    assert_eq!(*c, 99);
    assert_eq!(fake.made.load(SeqCst), 0);
}

#[test]
fn pools_of_the_same_name_add_up_in_the_report() {
    let (p1, _) = pool("t-sum", 2);
    let (p2, _) = pool("t-sum", 3);
    let _a = p1.get(false).unwrap();
    let _b = p2.get(false).unwrap();
    let s = mine("t-sum");
    assert_eq!((s.max_size, s.in_use), (5, 2));
    drop((p1, p2));
}

#[test]
fn a_dropped_pool_leaves_the_report_and_lets_go_of_its_manager() {
    let (p, fake) = pool("t-gone", 1);
    drop(p.get(false).unwrap());
    assert!(stats().iter().any(|s| s.name == "t-gone"));
    drop(p);
    wait_for("the pool to leave the report", || {
        !stats().iter().any(|s| s.name == "t-gone")
    });
    // The sweeper holds the pool only weakly, so nothing keeps the manager (and its connections)
    // alive once the pool is gone.
    wait_for("the manager to be released", || {
        Arc::strong_count(&fake) == 1
    });
}

#[test]
fn a_connection_that_could_not_be_had_is_unavailable_and_nothing_was_in_flight() {
    // Whether an approved plan may be put back after a refused run rests on this: a pool that
    // could not hand out a connection sent nothing, so the change is not in doubt. Only a
    // statement on a connection that then broke is (`note_in_flight`, pgpool `describe`).
    let (p, fake) = pool("noflight", 1);
    fake.fail.store(true, SeqCst);
    fault::begin_request(Default::default());
    let r = p.get(false);
    let out = fault::end_request();
    assert!(r.is_err());
    assert_eq!(out.fault, Some(Fault::Unavailable), "{out:?}");
    assert!(
        !out.in_flight,
        "a failed connect is not a statement in flight"
    );
    // A full pool (the second get has to wait and gives up) is busy, and also sends nothing.
    let p2 = Pool::new(
        Arc::new(Fake::default()),
        settings("noflight-full", 1, Duration::from_millis(100)),
    );
    let held = p2.get(false).unwrap();
    fault::begin_request(Default::default());
    let r = p2.get(false);
    let out = fault::end_request();
    assert!(r.is_err());
    assert_eq!(out.fault, Some(Fault::Busy), "{out:?}");
    assert!(!out.in_flight, "{out:?}");
    drop(held);
}
