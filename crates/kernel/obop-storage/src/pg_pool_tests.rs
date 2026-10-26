//! The connection pool against a real PostgreSQL (D74). The configuration tests run everywhere;
//! the rest needs `OBOP_TEST_POSTGRES_URL` (and says so when it is missing). The tests that stop
//! and start servers or put PgBouncer in front need Docker as well, and start containers of
//! their own (never the shared test database).
//!
//! Timing assertions leave room for a busy machine; the numbers they print (`--nocapture`) are
//! the ones quoted in docs/DECISIONS.md.

use super::*;
use crate::fault::Fault;
use crate::pg::Pg;
use crate::pgpool::{Class, PgPoolConfig, Pooler};
use crate::tls::PgConnector;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

fn unique(tag: &str) -> String {
    static N: AtomicUsize = AtomicUsize::new(0);
    format!(
        "{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}

fn fields(n: i64) -> Fields {
    let mut m = Fields::new();
    m.insert("n".into(), J::from(n));
    m
}

/// A database of this test's own on the server `OBOP_TEST_POSTGRES_URL` names, dropped afterwards
/// (with its sessions): other tests that run beside these, in this crate or another, build
/// indexes and fill tables in the shared database, which would make a statement here wait.
struct Scratch {
    admin: String,
    name: String,
    url: String,
}

impl Scratch {
    fn new(base: &str) -> Scratch {
        let name = unique("obop_pool_unit").replace('-', "_");
        let mut c = PgConnector::new(base).unwrap().connect().unwrap();
        c.batch_execute(&format!("CREATE DATABASE {name}")).unwrap();
        let (head, tail) = base.rsplit_once('/').unwrap();
        // Keep any query string (sslmode and the like) of the base address.
        let query = tail
            .split_once('?')
            .map_or(String::new(), |(_, q)| format!("?{q}"));
        Scratch {
            admin: base.to_string(),
            url: format!("{head}/{name}{query}"),
            name,
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if let Ok(mut c) = PgConnector::new(&self.admin).and_then(|c| c.connect()) {
            let _ = c.batch_execute(&format!(
                "DROP DATABASE IF EXISTS {} WITH (FORCE)",
                self.name
            ));
        }
    }
}

/// `(url, guard)` of a scratch database, or return from the test (saying so) without a server.
macro_rules! pg_db {
    () => {
        match std::env::var("OBOP_TEST_POSTGRES_URL") {
            Ok(base) => {
                let db = Scratch::new(&base);
                (db.url.clone(), db)
            }
            Err(_) => {
                eprintln!("skipped: set OBOP_TEST_POSTGRES_URL to run against PostgreSQL");
                return;
            }
        }
    };
}

/// The tests that measure time or fill the pool take turns, so they do not disturb each other
/// (the others run in parallel as usual).
fn serial() -> std::sync::MutexGuard<'static, ()> {
    static TURN: Mutex<()> = Mutex::new(());
    TURN.lock().unwrap_or_else(|e| e.into_inner())
}

fn connect(url: &str, cfg: PgPoolConfig) -> Pg {
    Pg::connect_with(url, cfg).unwrap()
}

fn sleep_query(c: &mut postgres::Client, secs: u32) -> crate::pgpool::PR<()> {
    c.batch_execute(&format!("SELECT pg_sleep({secs})"))?;
    Ok(())
}

// ---- configuration -------------------------------------------------------------------------

fn lookup(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
    move |k| {
        pairs
            .iter()
            .find(|(n, _)| *n == k)
            .map(|(_, v)| (*v).to_string())
    }
}

#[test]
fn the_defaults_are_the_documented_ones() {
    // Every default of docs/DEPLOY.md as a literal, read from an empty environment (a default that
    // drifts in code must fail here, not be compared with itself).
    let c = PgPoolConfig::from_lookup(&|_| None).unwrap();
    assert_eq!(c, PgPoolConfig::default());
    assert_eq!(c.size, 8);
    assert_eq!(c.acquire_timeout, Duration::from_millis(2_000));
    assert_eq!(c.idle_timeout, Duration::from_secs(300));
    assert_eq!(c.max_lifetime, Duration::from_secs(1_800));
    assert_eq!(c.health_check_idle, Duration::from_millis(1_000));
    assert_eq!(c.connect_timeout, Duration::from_millis(10_000));
    assert_eq!(c.max_waiters, 256);
    assert_eq!(c.max_long, 4);
    assert_eq!(
        (c.point_ms, c.list_ms, c.aggregate_ms, c.batch_ms),
        (Some(5_000), Some(10_000), Some(15_000), Some(60_000))
    );
    assert_eq!(c.idle_in_tx_ms, Some(120_000));
    assert_eq!(c.pooler, Pooler::Direct);
    assert_eq!((c.lock_size, c.blob_size), (8, 4));
    assert_eq!(c.lock_url, None);
}

#[test]
fn a_database_opened_for_a_moment_does_not_take_the_process_settings() {
    // The control plane seeds the database of each new instance through `open_with`; the
    // `OBOP_DB_*` and `OBOP_LOCK_DATABASE_URL` variables describe the control plane's own
    // database (a lock address that is another database's would be refused at start-up).
    let c = PgPoolConfig::one_shot();
    assert_eq!((c.size, c.lock_size, c.blob_size, c.max_long), (1, 1, 1, 1));
    assert_eq!(c.lock_url, None);
    assert_eq!(c.pooler, Pooler::Direct);
    assert_eq!(c.acquire_timeout, PgPoolConfig::default().acquire_timeout);
    // And it is what `open_with` takes: this one opens (a second database on the test server).
    let (url, _db) = pg_db!();
    let b = open_with(&url, Some(c)).unwrap();
    b.insert("oneshot", "a", &fields(1)).unwrap();
    assert_eq!(b.get("oneshot", "a").unwrap().unwrap()["n"], 1);
}

#[test]
fn every_setting_is_read_from_its_variable() {
    let c = PgPoolConfig::from_lookup(&lookup(&[
        ("OBOP_DB_POOL_SIZE", "20"),
        ("OBOP_DB_ACQUIRE_TIMEOUT_MS", "750"),
        ("OBOP_DB_IDLE_TIMEOUT_SECS", "60"),
        ("OBOP_DB_MAX_LIFETIME_SECS", "600"),
        ("OBOP_DB_HEALTH_CHECK_IDLE_MS", "0"),
        ("OBOP_DB_CONNECT_TIMEOUT_MS", "3000"),
        ("OBOP_DB_MAX_WAITERS", "10"),
        ("OBOP_DB_LONG_READS", "3"),
        ("OBOP_DB_STATEMENT_TIMEOUT_POINT_MS", "1"),
        ("OBOP_DB_STATEMENT_TIMEOUT_LIST_MS", "2"),
        ("OBOP_DB_STATEMENT_TIMEOUT_AGGREGATE_MS", "0"),
        ("OBOP_DB_STATEMENT_TIMEOUT_BATCH_MS", "4"),
        ("OBOP_DB_IDLE_IN_TRANSACTION_MS", "0"),
        ("OBOP_DB_POOLER", "Transaction"),
        ("OBOP_LOCK_DATABASE_URL", " postgres://u@h/db "),
        ("OBOP_DB_LOCK_POOL_SIZE", "3"),
        ("OBOP_DB_BLOB_POOL_SIZE", "2"),
    ]))
    .unwrap();
    assert_eq!(c.size, 20);
    assert_eq!(c.acquire_timeout, Duration::from_millis(750));
    assert_eq!(c.idle_timeout, Duration::from_secs(60));
    assert_eq!(c.max_lifetime, Duration::from_secs(600));
    assert_eq!(c.health_check_idle, Duration::ZERO);
    assert_eq!(c.connect_timeout, Duration::from_secs(3));
    assert_eq!((c.max_waiters, c.max_long), (10, 3));
    assert_eq!(
        (c.point_ms, c.list_ms, c.aggregate_ms, c.batch_ms),
        (Some(1), Some(2), None, Some(4)),
        "0 means: leave the server's own limit alone"
    );
    assert_eq!(c.idle_in_tx_ms, None);
    assert_eq!(c.pooler, Pooler::Transaction);
    assert_eq!(c.lock_url.as_deref(), Some("postgres://u@h/db"));
    assert_eq!((c.lock_size, c.blob_size), (3, 2));
}

#[test]
fn a_bad_value_stops_start_up_and_names_the_variable_without_echoing_a_url() {
    for (name, value) in [
        ("OBOP_DB_POOL_SIZE", "0"),
        ("OBOP_DB_POOL_SIZE", "65"),
        ("OBOP_DB_POOL_SIZE", "many"),
        ("OBOP_DB_ACQUIRE_TIMEOUT_MS", "10"),
        ("OBOP_DB_LOCK_POOL_SIZE", "9"),
        ("OBOP_DB_STATEMENT_TIMEOUT_POINT_MS", "-5"),
        ("OBOP_DB_MAX_WAITERS", "1.5"),
        ("OBOP_DB_POOLER", "pgcat"),
    ] {
        let get = |k: &str| (k == name).then(|| value.to_string());
        let e = PgPoolConfig::from_lookup(&get).unwrap_err();
        assert!(e.contains(name), "{name}={value}: {e}");
    }
    // A database address put into the wrong variable is not echoed (it carries a password).
    for name in ["OBOP_DB_POOL_SIZE", "OBOP_DB_POOLER", "OBOP_DB_MAX_WAITERS"] {
        let secret = "postgres://obop:hunter2-secret@db.internal/obop";
        let get = |k: &str| (k == name).then(|| secret.to_string());
        let e = PgPoolConfig::from_lookup(&get).unwrap_err();
        assert!(
            e.contains(name) && !e.contains("hunter2") && !e.contains("db.internal"),
            "{e}"
        );
    }
    // A pooler in transaction mode without a direct address for the locks is refused: locks taken
    // through it exclude nobody.
    let e = PgPoolConfig::from_lookup(&lookup(&[("OBOP_DB_POOLER", "transaction")])).unwrap_err();
    assert!(e.contains("OBOP_LOCK_DATABASE_URL"), "{e}");
    assert!(
        PgPoolConfig::from_lookup(&lookup(&[
            ("OBOP_DB_POOLER", "transaction"),
            ("OBOP_LOCK_DATABASE_URL", "postgres://u@direct/db"),
        ]))
        .is_ok()
    );
    // The same refusal when the configuration is built by hand, before any connection is tried;
    // and a lock address that is the pooler's own address is no direct address.
    let url = "postgres://u:p@127.0.0.1:1/db";
    let hand = |lock_url: Option<&str>| PgPoolConfig {
        pooler: Pooler::Transaction,
        lock_url: lock_url.map(str::to_string),
        ..PgPoolConfig::default()
    };
    let Err(e) = Pg::connect_with(url, hand(None)) else {
        panic!("a pooler without a lock address must be refused");
    };
    assert!(e.contains("OBOP_LOCK_DATABASE_URL"), "{e}");
    let Err(e) = Pg::connect_with(url, hand(Some(url))) else {
        panic!("the pooler's own address is no lock address");
    };
    assert!(e.contains("same address") && !e.contains(":p@"), "{e}");
    // The lock pool can never be larger than eight, the long-read lane never larger than the pool.
    let c = PgPoolConfig::from_lookup(&lookup(&[
        ("OBOP_DB_POOL_SIZE", "2"),
        ("OBOP_DB_LONG_READS", "9"),
    ]))
    .unwrap();
    assert_eq!(c.max_long, 2);
}

// ---- the acceptance tests -------------------------------------------------------------------

#[test]
fn a_five_second_read_does_not_block_a_concurrent_get() {
    let _turn = serial();
    let (url, _db) = pg_db!();
    let pg = Arc::new(connect(&url, PgPoolConfig::default()));
    let model = unique("sleep");
    pg.insert(&model, "a", &fields(1)).unwrap();
    // Two connections are open, so the gets below do not pay for opening one.
    let hold = pg.main.lease(false).unwrap();
    pg.get(&model, "a").unwrap();
    drop(hold);
    // What a get costs on this machine right now, with nothing else going on.
    let mut base: Vec<Duration> = (0..20)
        .map(|_| {
            let t = Instant::now();
            pg.get(&model, "a").unwrap().unwrap();
            t.elapsed()
        })
        .collect();
    base.sort();
    let base_median = base[base.len() / 2];
    let sleeper = {
        let pg = pg.clone();
        std::thread::spawn(move || {
            let t = Instant::now();
            // A 5 s read on a pooled connection (the aggregate limit is 15 s).
            pg.main
                .read(Class::Aggregate, |c| sleep_query(c, 5))
                .unwrap();
            t.elapsed()
        })
    };
    while pg.main.stats().in_use == 0 {
        std::thread::sleep(Duration::from_millis(5));
    }
    let started = Instant::now();
    let mut took = Vec::new();
    while started.elapsed() < Duration::from_secs(3) {
        let t = Instant::now();
        assert_eq!(pg.get(&model, "a").unwrap().unwrap()["n"], 1);
        took.push(t.elapsed());
        std::thread::sleep(Duration::from_millis(50));
    }
    took.sort();
    let (median, worst) = (took[took.len() / 2], *took.last().unwrap());
    eprintln!(
        "D74 get during a 5 s read: {} gets, median {median:?}, worst {worst:?}          (without it: median {base_median:?})",
        took.len()
    );
    // Under 50 ms on a quiet machine. On a busy one a get is slow with or without the long read,
    // so the test compares: a get that waited behind the read would take about five seconds.
    assert!(
        median < Duration::from_millis(50).max(base_median * 5),
        "median {median:?}"
    );
    assert!(worst < Duration::from_secs(2), "worst {worst:?}");
    assert!(
        pg.main.stats().in_use >= 1,
        "the long read was still running while the gets were served"
    );
    let slept = sleeper.join().unwrap();
    // (`pg_sleep(5)` can return a few tens of milliseconds early.)
    assert!(slept >= Duration::from_millis(4500), "{slept:?}");
}

#[test]
fn a_streaming_scan_holds_one_connection_and_nothing_else() {
    let _turn = serial();
    let (url, _db) = pg_db!();
    let pg = Arc::new(connect(&url, PgPoolConfig::default()));
    let model = unique("scanhold");
    let rows: Vec<_> = (0..20)
        .map(|i| (model.clone(), format!("{i:03}"), fields(i)))
        .collect();
    pg.insert_many(&rows).unwrap();
    let scanning = Arc::new(AtomicUsize::new(0));
    let scanner = {
        let (pg, model, scanning) = (pg.clone(), model.clone(), scanning.clone());
        std::thread::spawn(move || {
            let mut seen = 0;
            pg.scan(&model, &mut |_, _| {
                seen += 1;
                scanning.store(1, Ordering::SeqCst);
                // The consumer is slow: the connection stays checked out, the snapshot open.
                std::thread::sleep(Duration::from_millis(150));
                Ok(true)
            })
            .unwrap();
            seen
        })
    };
    while scanning.load(Ordering::SeqCst) == 0 {
        std::thread::sleep(Duration::from_millis(5));
    }
    // The scan holds exactly one connection of the main pool and nothing of the lock pool.
    assert_eq!(
        pg.main.stats().in_use,
        1,
        "the scan's connection and no other"
    );
    assert_eq!(pg.lock_pool.report().in_use, 0);
    let t = Instant::now();
    for i in 0..20 {
        assert_eq!(pg.get(&model, &format!("{i:03}")).unwrap().unwrap()["n"], i);
    }
    pg.insert(&model, "late", &fields(99)).unwrap();
    assert_eq!(
        pg.main.stats().in_use,
        1,
        "the others went back; the scan still holds its own"
    );
    assert!(
        pg.main.stats().connections_opened <= 2,
        "one connection for the scan, one for everything else"
    );
    assert!(
        t.elapsed() < Duration::from_secs(2),
        "reads and writes went on while the scan was running: {:?}",
        t.elapsed()
    );
    assert_eq!(
        scanner.join().unwrap(),
        20,
        "and the scan did not see the late record"
    );
}

#[test]
fn a_scan_sees_one_snapshot_from_its_first_row_to_its_last() {
    let (url, _db) = pg_db!();
    let pg = Arc::new(connect(&url, PgPoolConfig::default()));
    let model = unique("snapshot");
    // More than two fetches of 500 rows, so the changes below happen between fetches.
    let rows: Vec<_> = (0..1300)
        .map(|i| (model.clone(), format!("{i:05}"), fields(i)))
        .collect();
    pg.insert_many(&rows).unwrap();
    let mut seen = Vec::new();
    let mut changed = false;
    pg.scan(&model, &mut |id, f| {
        if !changed {
            changed = true;
            // Another connection changes the table while this scan is in the middle of it.
            pg.insert(&model, "zzzzz-new", &fields(-1)).unwrap();
            pg.delete(&model, "01200").unwrap();
            pg.update(&model, "01250", &fields(777)).unwrap();
        }
        seen.push((id.to_string(), f["n"].as_i64().unwrap()));
        Ok(true)
    })
    .unwrap();
    assert_eq!(
        seen.len(),
        1300,
        "the deleted record is still seen, the new one is not"
    );
    assert_eq!(seen[1200], ("01200".to_string(), 1200));
    assert_eq!(
        seen[1250],
        ("01250".to_string(), 1250),
        "the update is not seen either"
    );
    assert!(seen.iter().all(|(id, _)| id != "zzzzz-new"));
    assert!(
        seen.windows(2).all(|w| w[0].0 < w[1].0),
        "in insertion order"
    );
    // A later scan sees the changes.
    assert_eq!(pg.count(&model).unwrap(), 1300);
    let mut after = 0;
    pg.scan(&model, &mut |_, _| {
        after += 1;
        Ok(true)
    })
    .unwrap();
    assert_eq!(after, 1300);
    // Stopping early is not an error, and the connection is usable afterwards.
    let mut first = 0;
    pg.scan(&model, &mut |_, _| {
        first += 1;
        Ok(false)
    })
    .unwrap();
    assert_eq!(first, 1);
    assert_eq!(pg.get(&model, "00000").unwrap().unwrap()["n"], 0);
}

#[test]
fn two_hundred_requests_on_a_pool_of_eight_refuse_the_surplus_within_the_acquire_timeout() {
    let _turn = serial();
    let (url, _db) = pg_db!();
    let pg = Arc::new(connect(&url, PgPoolConfig::default()));
    pg.counter_get("warm").unwrap();
    let start = Arc::new(std::sync::Barrier::new(200));
    let peak_waiting = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let sampler = {
        let (pg, peak, stop) = (pg.clone(), peak_waiting.clone(), stop.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                peak.fetch_max(pg.main.stats().waiting, Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(10));
            }
        })
    };
    let threads: Vec<_> = (0..200)
        .map(|_| {
            let (pg, start) = (pg.clone(), start.clone());
            std::thread::spawn(move || {
                start.wait();
                let t = Instant::now();
                // Each request holds its connection for 3 s.
                let r = pg.main.run(Class::Point, |c| sleep_query(c, 3));
                (r, t.elapsed())
            })
        })
        .collect();
    let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    stop.store(true, Ordering::Relaxed);
    sampler.join().unwrap();
    let served = results.iter().filter(|(r, _)| r.is_ok()).count();
    let refused: Vec<_> = results
        .iter()
        .filter_map(|(r, took)| r.as_ref().err().map(|e| (e.clone(), *took)))
        .collect();
    let slowest = refused.iter().map(|(_, t)| *t).max().unwrap();
    let fastest = refused.iter().map(|(_, t)| *t).min().unwrap();
    eprintln!(
        "D74 200 requests, pool 8: {served} served, {} refused after {fastest:?} to {slowest:?}, \
         at most {} waiting",
        refused.len(),
        peak_waiting.load(Ordering::Relaxed)
    );
    assert_eq!(served, 8, "exactly the pool size ran");
    assert_eq!(refused.len(), 192);
    for (e, _) in &refused {
        assert_eq!(Fault::of_message(e), Some(Fault::Busy), "{e}");
    }
    assert!(
        fastest >= Duration::from_millis(1900),
        "they waited their turn: {fastest:?}"
    );
    assert!(
        slowest < Duration::from_millis(3000),
        "and no longer: {slowest:?}"
    );
    assert!(
        peak_waiting.load(Ordering::Relaxed) <= 256,
        "the queue never grew past its bound"
    );
    let s = pg.main.stats();
    assert_eq!((s.in_use, s.waiting), (0, 0), "nothing is left behind");
    assert_eq!(s.timeouts, 192);
    assert!(pg.main.stats().max_size == 8 && s.connections_opened <= 8);
    // And it serves again at once.
    assert_eq!(pg.counter_get("warm").unwrap(), 0);
}

#[test]
fn a_full_queue_refuses_without_waiting_and_parks_no_more_than_its_bound() {
    let _turn = serial();
    let (url, _db) = pg_db!();
    let cfg = PgPoolConfig {
        size: 2,
        max_waiters: 5,
        acquire_timeout: Duration::from_secs(2),
        ..PgPoolConfig::default()
    };
    let pg = Arc::new(connect(&url, cfg));
    let holders: Vec<_> = (0..2)
        .map(|_| {
            let pg = pg.clone();
            std::thread::spawn(move || pg.main.run(Class::Point, |c| sleep_query(c, 3)))
        })
        .collect();
    while pg.main.stats().in_use < 2 {
        std::thread::sleep(Duration::from_millis(5));
    }
    let start = Arc::new(std::sync::Barrier::new(300));
    let threads: Vec<_> = (0..300)
        .map(|_| {
            let (pg, start) = (pg.clone(), start.clone());
            std::thread::spawn(move || {
                start.wait();
                let t = Instant::now();
                (pg.counter_get("q"), t.elapsed())
            })
        })
        .collect();
    let mut peak = 0;
    let t = Instant::now();
    while t.elapsed() < Duration::from_millis(1500) {
        peak = peak.max(pg.main.stats().waiting);
        std::thread::sleep(Duration::from_millis(5));
    }
    let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    for h in holders {
        h.join().unwrap().unwrap();
    }
    let s = pg.main.stats();
    let immediate = results
        .iter()
        .filter(|(r, took)| r.is_err() && *took < Duration::from_millis(1000))
        .count();
    eprintln!(
        "D74 300 callers, 2 connections, queue bound 5: peak waiting {peak}, {immediate} refused at once, {} timed out",
        s.timeouts
    );
    assert!(peak <= 5, "peak {peak}");
    assert!(immediate >= 290, "{immediate} were refused at once");
    assert_eq!(s.rejected as usize, immediate);
    assert!(
        results
            .iter()
            .filter_map(|(r, _)| r.as_ref().err())
            .all(|e| Fault::of_message(e) == Some(Fault::Busy))
    );
}

#[test]
fn the_lock_pool_does_not_take_interactive_capacity_and_the_other_way_round() {
    let _turn = serial();
    let (url, _db) = pg_db!();
    let cfg = PgPoolConfig {
        size: 2,
        lock_size: 8,
        acquire_timeout: Duration::from_millis(1500),
        ..PgPoolConfig::default()
    };
    let pg = Arc::new(connect(&url, cfg));
    let model = unique("lockpool");
    pg.insert(&model, "a", &fields(1)).unwrap();
    // Eight different locks held at once, on eight connections.
    let (held_tx, held_rx) = std::sync::mpsc::channel();
    let release = Arc::new(std::sync::Barrier::new(9));
    let holders: Vec<_> = (0..8)
        .map(|i| {
            let (pg, tx, release) = (pg.clone(), held_tx.clone(), release.clone());
            let key = unique(&format!("held{i}"));
            std::thread::spawn(move || {
                let g = pg.lock(&key).unwrap();
                tx.send(()).unwrap();
                release.wait();
                drop(g);
            })
        })
        .collect();
    for _ in 0..8 {
        held_rx.recv_timeout(Duration::from_secs(30)).unwrap();
    }
    assert_eq!(pg.lock_pool.report().in_use, 8, "eight lock connections");
    // Interactive traffic is not affected: both main connections are free.
    let t = Instant::now();
    for _ in 0..50 {
        assert_eq!(pg.get(&model, "a").unwrap().unwrap()["n"], 1);
    }
    assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
    assert_eq!(pg.main.stats().timeouts, 0);
    // ... and a ninth lock holder is refused (busy), not queued without bound.
    let t = Instant::now();
    let ninth = pg.lock(&unique("ninth"));
    let Err(err) = ninth else {
        panic!("a ninth lock connection does not exist");
    };
    assert_eq!(Fault::of_message(&err), Some(Fault::Busy), "{err}");
    assert!(t.elapsed() < Duration::from_secs(4), "{:?}", t.elapsed());
    release.wait();
    for h in holders {
        h.join().unwrap();
    }
    // The other way round: with every interactive connection busy a lock is still granted.
    let sleepers: Vec<_> = (0..2)
        .map(|_| {
            let pg = pg.clone();
            std::thread::spawn(move || pg.main.run(Class::Point, |c| sleep_query(c, 2)))
        })
        .collect();
    while pg.main.stats().in_use < 2 {
        std::thread::sleep(Duration::from_millis(5));
    }
    let t = Instant::now();
    let g = pg.lock(&unique("free")).unwrap();
    assert!(
        t.elapsed() < Duration::from_millis(1000),
        "{:?}",
        t.elapsed()
    );
    assert!(
        pg.get(&model, "a").is_err(),
        "the interactive pool is the one that is full"
    );
    drop(g);
    for s in sleepers {
        s.join().unwrap().unwrap();
    }
}

#[test]
fn nested_locks_of_one_thread_share_a_connection_so_a_small_lock_pool_cannot_deadlock() {
    let _turn = serial();
    let (url, _db) = pg_db!();
    let app = unique("nested");
    let cfg = PgPoolConfig {
        lock_size: 2,
        acquire_timeout: Duration::from_secs(20),
        ..PgPoolConfig::default()
    };
    let sep = if url.contains('?') { '&' } else { '?' };
    let pg = Arc::new(connect(&format!("{url}{sep}application_name={app}"), cfg));
    // Sixteen threads, each holding three different locks at once (a vault lock, the write lock
    // and a session lock, as an agent's write does): with two connections for locks and no
    // sharing, two threads would each hold one and wait for another, forever.
    let done = Arc::new(AtomicUsize::new(0));
    let started = Instant::now();
    std::thread::scope(|s| {
        for t in 0..16 {
            let (pg, done, app) = (pg.clone(), done.clone(), app.clone());
            s.spawn(move || {
                for round in 0..5 {
                    let a = pg.lock(&format!("{app}-vault-{t}")).unwrap();
                    let b = pg.lock(&format!("{app}-write")).unwrap();
                    let c = pg.lock(&format!("{app}-session-{t}-{round}")).unwrap();
                    std::thread::sleep(Duration::from_millis(2));
                    drop((c, b, a));
                }
                done.fetch_add(1, Ordering::SeqCst);
            });
        }
    });
    assert_eq!(done.load(Ordering::SeqCst), 16);
    eprintln!(
        "D74 16 threads x 5 rounds x 3 nested locks on 2 connections: {:?}",
        started.elapsed()
    );
    let s = pg.lock_pool.report();
    assert!(
        s.connections_opened <= 2,
        "{} lock connections were opened",
        s.connections_opened
    );
    assert_eq!(s.timeouts + s.rejected, 0);
    // The server agrees: at most two sessions of this application hold locks, plus the main one.
    let mut admin = PgConnector::new(&url).unwrap().connect().unwrap();
    let n: i64 = admin
        .query_one(
            "SELECT count(*) FROM pg_stat_activity WHERE application_name = $1",
            &[&app],
        )
        .unwrap()
        .get(0);
    assert!(n <= 3, "{n} sessions");
    // Every lock was released: a fresh handle takes the same keys at once.
    let other = connect(&url, PgPoolConfig::default());
    let _g = other.lock(&format!("{app}-write")).unwrap();
}

#[test]
fn time_limits_follow_the_command_class_and_never_leak_between_callers() {
    let (url, _db) = pg_db!();
    let cfg = PgPoolConfig {
        size: 1,
        point_ms: Some(1111),
        list_ms: Some(2222),
        aggregate_ms: Some(3333),
        batch_ms: Some(4444),
        ..PgPoolConfig::default()
    };
    let pg = connect(&url, cfg);
    let show = |class: Class| -> String {
        pg.main
            .run(class, |c| {
                Ok(c.query_one("SHOW statement_timeout", &[])?
                    .get::<_, String>(0))
            })
            .unwrap()
    };
    // One connection serves all of them, one after the other, in a mixed order.
    for class in [
        Class::Point,
        Class::List,
        Class::Point,
        Class::Aggregate,
        Class::Batch,
        Class::List,
        Class::Point,
    ] {
        let want = match class {
            Class::Point => "1111ms",
            Class::List => "2222ms",
            Class::Aggregate => "3333ms",
            Class::Batch => "4444ms",
        };
        assert_eq!(show(class), want, "{class:?}");
    }
    assert_eq!(
        pg.main.stats().connections_opened,
        1,
        "it was one connection all along"
    );
    // A transaction uses SET LOCAL: its own limit inside, the session's own limit outside.
    let inside = pg
        .main
        .tx(Class::Batch, |c| {
            Ok(c.query_one("SHOW statement_timeout", &[])?
                .get::<_, String>(0))
        })
        .unwrap();
    assert_eq!(inside, "4444ms");
    assert_eq!(
        show(Class::Point),
        "1111ms",
        "SET LOCAL ended with the transaction"
    );
    // A snapshot (scan, export) has its own as well.
    let in_snapshot = pg
        .main
        .snapshot(Class::Aggregate, |tx| {
            Ok(tx
                .query_one("SHOW statement_timeout", &[])?
                .get::<_, String>(0))
        })
        .unwrap();
    assert_eq!(in_snapshot, "3333ms");
    assert_eq!(show(Class::List), "2222ms");
    // A class set to zero leaves the server's own limit in place, also after another class
    // changed it on this very connection.
    let cfg0 = PgPoolConfig {
        size: 1,
        point_ms: Some(1111),
        list_ms: None,
        ..PgPoolConfig::default()
    };
    let pg0 = connect(&url, cfg0);
    let server_default: String = PgConnector::new(&url)
        .unwrap()
        .connect()
        .unwrap()
        .query_one("SHOW statement_timeout", &[])
        .unwrap()
        .get(0);
    let get0 = |class: Class| -> String {
        pg0.main
            .run(class, |c| {
                Ok(c.query_one("SHOW statement_timeout", &[])?
                    .get::<_, String>(0))
            })
            .unwrap()
    };
    assert_eq!(get0(Class::Point), "1111ms");
    assert_eq!(
        get0(Class::List),
        server_default,
        "RESET, not a left-over of the point class"
    );
    // The same for a transaction and a snapshot of a class without a limit, run on a connection
    // whose session still carries the point class's limit: `SET LOCAL ... TO DEFAULT`.
    assert_eq!(get0(Class::Point), "1111ms");
    let in_tx = pg0
        .main
        .tx(Class::List, |c| {
            Ok(c.query_one("SHOW statement_timeout", &[])?
                .get::<_, String>(0))
        })
        .unwrap();
    assert_eq!(in_tx, server_default, "a transaction of an unlimited class");
    assert_eq!(
        get0(Class::Point),
        "1111ms",
        "and the session was not touched"
    );
    let in_snapshot = pg0
        .main
        .snapshot(Class::List, |tx| {
            Ok(tx
                .query_one("SHOW statement_timeout", &[])?
                .get::<_, String>(0))
        })
        .unwrap();
    assert_eq!(
        in_snapshot, server_default,
        "a snapshot of an unlimited class"
    );
    assert_eq!(
        pg0.main.stats().connections_opened,
        1,
        "all on one connection"
    );
}

#[test]
fn a_cancelled_statement_is_recognised_by_its_code_not_by_the_words_of_the_server() {
    let (url, _db) = pg_db!();
    let pg = connect(&url, PgPoolConfig::default());
    // A server whose messages are in another language (lc_messages) words a cancelled statement
    // differently; what it always sends is SQLSTATE 57014.
    fault::clear();
    let e = pg
        .main
        .run(Class::Point, |c| {
            c.batch_execute(
                "DO $$ BEGIN RAISE EXCEPTION 'storniere Anfrage wegen Zeit\u{fc}berschreitung der Anfrage' USING ERRCODE = '57014'; END $$",
            )?;
            Ok(())
        })
        .unwrap_err();
    assert_eq!(Fault::of_message(&e), Some(Fault::Timeout), "{e}");
    assert_eq!(fault::take(), Some(Fault::Timeout));
    // Another server error is neither a timeout nor an outage, and says what it was.
    let e = pg
        .main
        .run(Class::Point, |c| {
            c.batch_execute("DO $$ BEGIN RAISE EXCEPTION 'nope' USING ERRCODE = '23505'; END $$")?;
            Ok(())
        })
        .unwrap_err();
    assert!(
        Fault::of_message(&e).is_none() && e.contains("23505") && e.contains("nope"),
        "{e}"
    );
    assert_eq!(fault::take(), None);
}

#[test]
fn batches_that_name_the_same_records_in_opposite_order_do_not_deadlock() {
    let _turn = serial();
    let (url, _db) = pg_db!();
    // Two handles on one database and several threads on each: the same shape as one handle in
    // two processes, and as the one process that used to serialise them behind one connection.
    let a = open(&url).unwrap();
    let b = open(&url).unwrap();
    crate::shared_tests::concurrent_batches(a, b);
}

#[test]
fn a_batch_takes_at_most_the_stripe_count_of_advisory_locks_whatever_its_size() {
    use crate::pg::{BATCH_LOCK_STRIPES, batch_lock_keys};
    let ops = |n: usize| -> Vec<Op> {
        (0..n)
            .map(|i| Op::Insert {
                model: "m".into(),
                id: format!("id{i}"),
                fields: Fields::new(),
            })
            .collect()
    };
    let big = ops(100_000);
    let keys = batch_lock_keys(&big);
    assert!(keys.len() <= BATCH_LOCK_STRIPES as usize, "{}", keys.len());
    assert!(
        keys.len() > BATCH_LOCK_STRIPES as usize * 9 / 10,
        "the records spread over the stripes ({})",
        keys.len()
    );
    assert!(
        keys.windows(2).all(|w| w[0] < w[1]),
        "ascending and distinct"
    );
    assert!(
        keys.iter()
            .all(|k| (0..BATCH_LOCK_STRIPES as i32).contains(k))
    );
    // The order of the steps does not change which locks are taken, nor their order.
    let mut reversed = big.clone();
    reversed.reverse();
    assert_eq!(batch_lock_keys(&reversed), keys);
    // A small batch takes a small number of locks (fewer when two records share a stripe).
    assert!((1..=3).contains(&batch_lock_keys(&big[..3]).len()));
    assert!(batch_lock_keys(&[]).is_empty());
}

#[test]
fn the_locks_of_a_thousand_row_batch_in_the_server_are_the_stripes_not_the_records() {
    use crate::pg::{BATCH_LOCK_STRIPES, batch_lock_keys, lock_batch};
    let (url, _db) = pg_db!();
    let pg = connect(&url, PgPoolConfig::default());
    let ops: Vec<Op> = (0..1000)
        .map(|i| Op::Insert {
            model: "m".into(),
            id: format!("id{i}"),
            fields: Fields::new(),
        })
        .collect();
    let keys = batch_lock_keys(&ops);
    // The server's own count of the advisory locks the transaction of the REAL `apply_checked`
    // holds after it took its locks (the backend asks, in test builds, inside that path): one per
    // stripe. One per record would be 1,000, and this fails for a backend that goes back to that.
    pg.apply_checked(&ops).unwrap();
    let held = crate::pg::BATCH_LOCKS_HELD.with(std::cell::Cell::get);
    assert_eq!(held as usize, keys.len(), "{held}");
    // The number the record documents, not the constant of the code under test.
    assert!(held <= 128, "{held}");
    const { assert!(BATCH_LOCK_STRIPES <= 128) };
    // A batch of three records holds one to three.
    let few: Vec<Op> = (0..3)
        .map(|i| Op::Insert {
            model: "m".into(),
            id: format!("few{i}"),
            fields: Fields::new(),
        })
        .collect();
    pg.apply_checked(&few).unwrap();
    assert!((1..=3).contains(&crate::pg::BATCH_LOCKS_HELD.with(std::cell::Cell::get)));
    // The helper itself takes what it is given, in one statement.
    let held = pg
        .main
        .tx_retry(Class::Batch, |c| {
            lock_batch(c, &keys)?;
            let r = c.query_one(
                "SELECT count(*) FROM pg_locks WHERE locktype = 'advisory' AND pid = pg_backend_pid()",
                &[],
            )?;
            Ok(r.get::<_, i64>(0))
        })
        .unwrap();
    assert_eq!(held as usize, keys.len());
}

#[test]
fn a_bulk_write_makes_the_small_writes_and_reads_of_its_process_wait_and_never_busy() {
    let _turn = serial();
    let (url, _db) = pg_db!();
    // A small pool with a short acquire timeout: if the small batches parked a connection each at
    // the stripes the bulk write holds, three of them would fill it and the rest (and every read)
    // would be refused as busy.
    let cfg = PgPoolConfig {
        size: 4,
        acquire_timeout: Duration::from_millis(300),
        ..PgPoolConfig::default()
    };
    let pg = Arc::new(connect(&url, cfg));
    pg.insert("seed", "r", &fields(0)).unwrap();
    let model = unique("mixed");
    let bulk: Vec<Op> = (0..1000)
        .map(|i| Op::Insert {
            model: model.clone(),
            id: format!("bulk{i}"),
            fields: fields(i),
        })
        .collect();
    let started = Arc::new(std::sync::Barrier::new(2));
    let big = {
        let (pg, started) = (pg.clone(), started.clone());
        std::thread::spawn(move || {
            started.wait();
            pg.apply_checked(&bulk).map_err(|e| e.to_string())
        })
    };
    started.wait();
    std::thread::sleep(Duration::from_millis(50));
    let smalls: Vec<_> = (0..9)
        .map(|t| {
            let (pg, model) = (pg.clone(), model.clone());
            std::thread::spawn(move || {
                pg.apply_checked(&[Op::Insert {
                    model: format!("{model}-small"),
                    id: format!("s{t}"),
                    fields: fields(t),
                }])
                .map_err(|e| format!("small write {t}: {e}"))
            })
        })
        .collect();
    // Reads of the same process are not refused while the bulk write runs.
    for _ in 0..20 {
        pg.get("seed", "r").expect("a read while a bulk write runs");
        std::thread::sleep(Duration::from_millis(20));
    }
    big.join().unwrap().unwrap();
    for s in smalls {
        s.join().unwrap().unwrap();
    }
    assert_eq!(pg.count(&model).unwrap(), 1000);
    assert_eq!(pg.count(&format!("{model}-small")).unwrap(), 9);
}

#[test]
fn thousand_row_batches_from_many_threads_do_not_fill_the_servers_lock_table() {
    let _turn = serial();
    let (url, _db) = pg_db!();
    let model = unique("bulk");
    // Two handles stand for two server processes: 8 batches of 1,000 records at once. One advisory
    // lock per record would be 8,000 slots of the shared lock table (6,400 with the server's
    // defaults) if they ran together, and the server would answer "out of shared memory". The
    // batches name other records, so only the stripes make them wait for each other, which they
    // do (the next test counts the locks the server holds).
    let backends: Vec<Arc<Pg>> = (0..2)
        .map(|_| Arc::new(connect(&url, PgPoolConfig::default())))
        .collect();
    let mut admin = PgConnector::new(&url).unwrap().connect().unwrap();
    let mut show = |name: &str| -> String {
        admin
            .query_one(&format!("SHOW {name}"), &[])
            .unwrap()
            .get::<_, String>(0)
    };
    eprintln!(
        "server: max_locks_per_transaction={} max_connections={}",
        show("max_locks_per_transaction"),
        show("max_connections")
    );
    let barrier = Arc::new(std::sync::Barrier::new(8));
    let handles: Vec<_> = (0..8)
        .map(|t| {
            let (b, barrier, model) = (backends[t % 2].clone(), barrier.clone(), model.clone());
            std::thread::spawn(move || {
                barrier.wait();
                let ops: Vec<Op> = (0..1000)
                    .map(|i| Op::Insert {
                        model: model.clone(),
                        id: format!("t{t}-{i}"),
                        fields: fields(i),
                    })
                    .collect();
                b.apply_checked(&ops)
                    .map_err(|e| format!("thread {t}: {e}"))
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap().unwrap();
    }
    assert_eq!(backends[0].count(&model).unwrap(), 8 * 1000);
}

#[test]
fn large_batches_that_name_the_same_records_in_opposite_order_do_not_deadlock() {
    let _turn = serial();
    let (url, _db) = pg_db!();
    let model = unique("opposite");
    let (a, b) = (
        Arc::new(connect(&url, PgPoolConfig::default())),
        Arc::new(connect(&url, PgPoolConfig::default())),
    );
    let ids: Vec<String> = (0..120).map(|i| format!("r{i:03}")).collect();
    let batch = |reverse: bool| -> Vec<Op> {
        let mut ids = ids.clone();
        if reverse {
            ids.reverse();
        }
        ids.iter()
            .map(|id| Op::add(&model, id, "n", 1).or_create(fields(0)))
            .collect()
    };
    // Four threads on each handle; half name the records ascending, half descending.
    let handles: Vec<_> = (0..8)
        .map(|t| {
            let (backend, ops) = (if t % 2 == 0 { a.clone() } else { b.clone() }, batch(t < 4));
            std::thread::spawn(move || {
                for round in 0..4 {
                    backend
                        .apply_checked(&ops)
                        .map_err(|e| format!("thread {t} round {round}: {e}"))?;
                }
                Ok::<(), String>(())
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap().unwrap();
    }
    // Every batch was applied once, whichever way it was named: 8 threads of 4 rounds.
    for id in &ids {
        assert_eq!(a.get(&model, id).unwrap().unwrap()["n"], 32, "{id}");
    }
}

#[test]
fn processes_that_open_the_blob_store_together_on_a_fresh_database_all_start() {
    use crate::blob::DbBytes;
    // Two replicas started together on a new database: both create the table of the blob store.
    // Without the schema lock one of them failed with a duplicate-object error that came out as
    // "db error".
    let Ok(base) = std::env::var("OBOP_TEST_POSTGRES_URL") else {
        eprintln!("skipped: set OBOP_TEST_POSTGRES_URL to run against PostgreSQL");
        return;
    };
    for round in 0..4 {
        let db = Scratch::new(&base);
        let barrier = Arc::new(std::sync::Barrier::new(6));
        let hs: Vec<_> = (0..6)
            .map(|t| {
                let (url, barrier) = (db.url.clone(), barrier.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    DbBytes::open_with(&url, Some(PgPoolConfig::one_shot()))
                        .map(|_| ())
                        .map_err(|e| format!("round {round} opener {t}: {e}"))
                })
            })
            .collect();
        for h in hs {
            h.join().unwrap().unwrap();
        }
    }
}

#[test]
fn a_transaction_the_server_rolled_back_for_a_deadlock_is_run_again() {
    let (url, _db) = pg_db!();
    let pg = Arc::new(connect(&url, PgPoolConfig::default()));
    let model = unique("deadlock");
    pg.insert(&model, "p", &fields(0)).unwrap();
    pg.insert(&model, "q", &fields(0)).unwrap();
    // Two swaps that update the two records in opposite order with a pause between the two
    // updates: the server finds the deadlock, rolls one of them back, and the pool runs it again.
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let runs = Arc::new(AtomicUsize::new(0));
    let handles: Vec<_> = [("p", "q"), ("q", "p")]
        .into_iter()
        .map(|(first, second)| {
            let (pg, barrier, runs, model) =
                (pg.clone(), barrier.clone(), runs.clone(), model.clone());
            std::thread::spawn(move || {
                pg.main.tx_retry(Class::Batch, |c| {
                    let attempt = runs.fetch_add(1, Ordering::SeqCst);
                    for (i, id) in [first, second].into_iter().enumerate() {
                        c.execute(
                            "UPDATE obop_record SET data = data || '{\"n\": 1}'::jsonb \
                             WHERE model = $1 AND id = $2",
                            &[&model, &id],
                        )?;
                        if i == 0 && attempt < 2 {
                            // Both hold their first record before either asks for the second.
                            barrier.wait();
                            std::thread::sleep(Duration::from_millis(100));
                        }
                    }
                    Ok(())
                })
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap().unwrap();
    }
    assert!(runs.load(Ordering::SeqCst) >= 3, "one of the two ran again");
    assert_eq!(pg.get(&model, "p").unwrap().unwrap()["n"], 1);
    assert_eq!(pg.get(&model, "q").unwrap().unwrap()["n"], 1);
}

#[test]
fn a_statement_over_its_limit_is_cancelled_and_the_connection_stays_in_the_pool() {
    let _turn = serial();
    let (url, _db) = pg_db!();
    let cfg = PgPoolConfig {
        size: 1,
        point_ms: Some(1500),
        aggregate_ms: Some(1500),
        ..PgPoolConfig::default()
    };
    let pg = connect(&url, cfg);
    let before = crate::statement_timeouts();
    fault::clear();
    let t = Instant::now();
    let e = pg
        .main
        .run(Class::Point, |c| sleep_query(c, 5))
        .unwrap_err();
    assert!(t.elapsed() < Duration::from_secs(4), "{:?}", t.elapsed());
    assert_eq!(Fault::of_message(&e), Some(Fault::Timeout), "{e}");
    assert_eq!(fault::take(), Some(Fault::Timeout));
    assert!(crate::statement_timeouts() > before);
    // A cancelled statement is the server's answer, not a broken connection: same connection.
    assert_eq!(pg.main.stats().connections_opened, 1);
    assert_eq!(pg.main.stats().connections_closed, 0);
    pg.insert(&unique("after"), "a", &fields(1)).unwrap();
    // Also inside a snapshot, and then the pool is still fine.
    let e = pg
        .main
        .snapshot(Class::Aggregate, |tx| {
            tx.batch_execute("SELECT pg_sleep(5)")?;
            Ok(())
        })
        .unwrap_err();
    assert_eq!(Fault::of_message(&e), Some(Fault::Timeout), "{e}");
    assert_eq!(pg.main.stats().connections_closed, 0);
    assert_eq!(pg.counter_get("after-timeout").unwrap(), 0);
}

#[test]
fn a_slow_consumer_is_not_cut_off_by_the_statement_limit_but_a_stalled_one_is_ended() {
    let _turn = serial();
    let (url, _db) = pg_db!();
    let model = unique("slowscan");
    let cfg = PgPoolConfig {
        aggregate_ms: Some(1000),
        idle_in_tx_ms: Some(1500),
        ..PgPoolConfig::default()
    };
    let pg = connect(&url, cfg);
    let rows: Vec<_> = (0..1200)
        .map(|i| (model.clone(), format!("{i:05}"), fields(i)))
        .collect();
    pg.insert_many(&rows).unwrap();
    // The consumer takes 1.2 s between two fetches, more than the 1 s statement limit: each
    // fetch has its own limit, so the scan completes and sees every row.
    let mut n = 0;
    pg.scan(&model, &mut |_, _| {
        n += 1;
        if n == 501 {
            std::thread::sleep(Duration::from_millis(1200));
        }
        Ok(true)
    })
    .unwrap();
    assert_eq!(n, 1200);
    // A consumer that stalls for longer than the idle-in-transaction limit loses its snapshot:
    // the server ends the session, so a stuck consumer cannot hold a snapshot (and the vacuum
    // behind it) for ever. The pool drops that connection and goes on.
    let mut n = 0;
    let e = pg
        .scan(&model, &mut |_, _| {
            n += 1;
            if n == 501 {
                std::thread::sleep(Duration::from_millis(2500));
            }
            Ok(true)
        })
        .unwrap_err();
    assert_eq!(Fault::of_message(&e), Some(Fault::Unavailable), "{e}");
    assert!(
        n < 1200,
        "the scan did not run to its end, it was ended at {n}"
    );
    assert_eq!(pg.count(&model).unwrap(), 1200);
}

#[test]
fn a_connection_killed_by_the_server_is_noticed_and_replaced() {
    let (url, _db) = pg_db!();
    let sep = if url.contains('?') { '&' } else { '?' };
    let kill = |app: &str| {
        let mut admin = PgConnector::new(&url).unwrap().connect().unwrap();
        admin
            .execute(
                "SELECT pg_terminate_backend(pid) FROM pg_stat_activity \
                 WHERE application_name = $1 AND pid <> pg_backend_pid()",
                &[&app],
            )
            .unwrap();
        std::thread::sleep(Duration::from_millis(200));
    };
    let model = unique("killed");
    // 1. With a health check on every checkout the dead connection is never handed out.
    let app = unique("kill-health");
    let cfg = PgPoolConfig {
        health_check_idle: Duration::ZERO,
        ..PgPoolConfig::default()
    };
    let pg = connect(&format!("{url}{sep}application_name={app}"), cfg);
    pg.insert(&model, "a", &fields(1)).unwrap();
    kill(&app);
    assert_eq!(pg.get(&model, "a").unwrap().unwrap()["n"], 1);
    assert!(pg.main.stats().connections_closed >= 1);
    // 2. Without it (the connection was used a moment ago) a read finds out by itself and is
    // repeated once on a fresh connection.
    let app = unique("kill-read");
    let cfg = PgPoolConfig {
        health_check_idle: Duration::from_secs(3600),
        ..PgPoolConfig::default()
    };
    let pg = connect(&format!("{url}{sep}application_name={app}"), cfg);
    pg.insert(&model, "b", &fields(2)).unwrap();
    assert_eq!(pg.get(&model, "b").unwrap().unwrap()["n"], 2);
    kill(&app);
    assert_eq!(
        pg.get(&model, "b").unwrap().unwrap()["n"],
        2,
        "the read was repeated"
    );
    assert!(pg.main.stats().connections_closed >= 1);
    // 3. A write is not repeated (it may have been applied): it fails once, says so, and the
    // pool has healed for the next one.
    let app = unique("kill-write");
    let pg = connect(
        &format!("{url}{sep}application_name={app}"),
        PgPoolConfig {
            health_check_idle: Duration::from_secs(3600),
            ..PgPoolConfig::default()
        },
    );
    pg.insert(&model, "c", &fields(3)).unwrap();
    kill(&app);
    fault::clear();
    let e = pg.insert(&model, "d", &fields(4)).unwrap_err();
    assert_eq!(Fault::of_message(&e), Some(Fault::Unavailable), "{e}");
    assert_eq!(fault::take(), Some(Fault::Unavailable));
    pg.insert(&model, "d", &fields(4)).unwrap();
}

// ---- containers: a restart, and PgBouncer in front -------------------------------------------

fn docker(args: &[&str]) -> Option<String> {
    let o = Command::new("docker").args(args).output().ok()?;
    o.status
        .success()
        .then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

struct Container(String);

impl Drop for Container {
    fn drop(&mut self) {
        let _ = Command::new("docker").args(["rm", "-f", &self.0]).output();
    }
}

struct Network(String);

impl Drop for Network {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["network", "rm", &self.0])
            .output();
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// A throwaway `postgres:17` of this test's own, on a port that stays the same across restarts.
struct OwnPg {
    name: String,
    port: u16,
    _guard: Container,
}

impl OwnPg {
    fn start(network: Option<&str>) -> Option<Self> {
        docker(&["info", "--format", "{{.ServerVersion}}"])?;
        let name = unique("obop-pool-pg");
        let port = free_port();
        let publish = format!("127.0.0.1:{port}:5432");
        let mut args = vec![
            "run",
            "-d",
            "--name",
            &name,
            "-e",
            "POSTGRES_PASSWORD=obop-test",
            "-p",
            &publish,
        ];
        if let Some(n) = network {
            args.extend(["--network", n]);
        }
        args.push("postgres:17");
        let started = docker(&args);
        let guard = Container(name.clone());
        started?;
        let me = OwnPg {
            name,
            port,
            _guard: guard,
        };
        me.wait_ready().then_some(me)
    }

    fn wait_ready(&self) -> bool {
        for _ in 0..120 {
            if PgConnector::new(&self.url())
                .unwrap()
                .connect()
                .is_ok_and(|mut c| c.simple_query("SELECT 1").is_ok())
            {
                return true;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        false
    }

    fn url(&self) -> String {
        format!(
            "postgres://postgres:obop-test@127.0.0.1:{}/postgres",
            self.port
        )
    }
}

macro_rules! docker_pg {
    () => {
        match OwnPg::start(None) {
            Some(p) => p,
            None => {
                eprintln!("skipping: docker (and the postgres:17 image) are not available");
                return;
            }
        }
    };
}

#[test]
fn after_a_server_restart_the_pool_serves_again_without_restarting_the_process() {
    let _turn = serial();
    let pg_server = docker_pg!();
    let cfg = PgPoolConfig {
        acquire_timeout: Duration::from_secs(2),
        ..PgPoolConfig::default()
    };
    let b = open_with(&pg_server.url(), Some(cfg)).unwrap();
    let model = unique("restart");
    b.insert(&model, "a", &fields(1)).unwrap();
    for _ in 0..20 {
        b.get(&model, "a").unwrap();
    }
    let held = b.lock("survives?").unwrap();
    drop(held);
    // Restart the database under a running pool. How long the server itself needs to come back
    // is the machine's business (it is probed on a connection of its own); what is measured is
    // the pool: how long after the database accepts connections again it serves, and that it
    // answers quickly while the database is away.
    let restarted = Instant::now();
    let name = pg_server.name.clone();
    let restarter = std::thread::spawn(move || docker(&["restart", "-t", "5", &name]).is_some());
    let probe = PgConnector::new(&pg_server.url()).unwrap();
    let mut outage_errors = 0;
    let mut slowest_error = Duration::ZERO;
    let mut first_error: Option<Duration> = None;
    let mut database_back: Option<Duration> = None;
    let back = loop {
        if first_error.is_some()
            && database_back.is_none()
            && probe
                .connect()
                .is_ok_and(|mut c| c.simple_query("SELECT 1").is_ok())
        {
            database_back = Some(restarted.elapsed());
        }
        let t = Instant::now();
        fault::clear();
        match b.get(&model, "a") {
            Ok(Some(f)) => {
                assert_eq!(f["n"], 1, "the data is where it was");
                // Only an answer after the outage began counts as "served again".
                if first_error.is_some() {
                    break restarted.elapsed();
                }
            }
            Ok(None) => panic!("the record is gone"),
            Err(e) => {
                outage_errors += 1;
                first_error.get_or_insert(restarted.elapsed());
                slowest_error = slowest_error.max(t.elapsed());
                let kind = Fault::of_message(&e);
                assert!(
                    matches!(kind, Some(Fault::Unavailable | Fault::Busy)),
                    "an outage is reported as a storage fault, got {e}"
                );
                assert_eq!(fault::take(), kind);
            }
        }
        assert!(
            restarted.elapsed() < Duration::from_secs(300),
            "no recovery in five minutes ({outage_errors} refused calls)"
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    assert!(restarter.join().unwrap(), "docker restart");
    let database_back = database_back.unwrap_or(back);
    let after_database = back.saturating_sub(database_back);
    eprintln!(
        "D74 restart: the first refused call {:?} after the restart began, the database accepted \
         connections again after {database_back:?}, the pool served {after_database:?} later \
         ({back:?} in all); {outage_errors} refused calls, the slowest took {slowest_error:?}",
        first_error.unwrap()
    );
    assert!(
        after_database < Duration::from_secs(30),
        "the pool took {after_database:?} to serve once the database was back"
    );
    assert!(
        slowest_error < Duration::from_secs(5),
        "a call during the outage answers quickly, it does not hang: {slowest_error:?}"
    );
    // Everything works again: writes, counters, the batch path, locks, a scan. (A step may need a
    // second go while the freshly restarted server is still busy recovering; none may need more
    // than thirty seconds.)
    fn eventually<T>(what: &str, mut f: impl FnMut() -> Result<T>) -> T {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match f() {
                Ok(v) => return v,
                Err(e) if Instant::now() < deadline => {
                    eprintln!("{what}: not yet ({e})");
                    std::thread::sleep(Duration::from_millis(200));
                }
                Err(e) => panic!("{what} did not work again within 30 s: {e}"),
            }
        }
    }
    eventually("insert", || b.insert(&model, "b", &fields(2)));
    assert_eq!(
        eventually("counter", || b.counter_add(&unique("c"), 60_000, false)),
        1
    );
    eventually("batch", || {
        b.apply(&[Op::Merge {
            model: model.clone(),
            id: "a".into(),
            patch: fields(5),
        }])
    });
    eventually("lock", || b.lock("after-restart").map(drop));
    let n = eventually("scan", || {
        let mut n = 0;
        b.scan(&model, &mut |_, _| {
            n += 1;
            Ok(true)
        })
        .map(|()| n)
    });
    assert_eq!(n, 2);
}

/// The PgBouncer image the tests run, by digest: `edoburu/pgbouncer:latest` as pulled on
/// 2026-10-25, which reports PgBouncer 1.26.1 (the test asks it).
const PGBOUNCER_IMAGE: &str =
    "edoburu/pgbouncer@sha256:8638aa563607256d3c7440437534195f1066a92b68015d89ae9d6b935b444a6d";

/// PgBouncer in transaction mode with two server connections, in front of a PostgreSQL of this
/// test's own; both on a network of their own. `None` (after saying why) without Docker or the
/// image.
struct Bouncer {
    pg: OwnPg,
    port: u16,
    _bouncer: Container,
    _net: Network,
}

impl Bouncer {
    /// `max_prepared`: PgBouncer's `max_prepared_statements` (`None` keeps its default, which
    /// supports protocol-level prepared statements in transaction mode; `0` turns that off).
    fn start(max_prepared: Option<&str>) -> Option<Self> {
        docker(&["info", "--format", "{{.ServerVersion}}"])?;
        let net_name = unique("obop-pool-net");
        docker(&["network", "create", &net_name])?;
        let net = Network(net_name.clone());
        let pg = OwnPg::start(Some(&net_name))?;
        let name = unique("obop-pool-bouncer");
        let port = free_port();
        let publish = format!("127.0.0.1:{port}:5432");
        let database = format!(
            "DATABASE_URL=postgres://postgres:obop-test@{}:5432/postgres",
            pg.name
        );
        let mut args: Vec<String> = [
            "run",
            "-d",
            "--name",
            &name,
            "--network",
            &net_name,
            "-e",
            &database,
            "-e",
            "POOL_MODE=transaction",
            "-e",
            "DEFAULT_POOL_SIZE=2",
            "-e",
            "MAX_CLIENT_CONN=200",
            "-e",
            "AUTH_TYPE=scram-sha-256",
            "-p",
            &publish,
        ]
        .map(String::from)
        .into();
        if let Some(n) = max_prepared {
            args.extend(["-e".into(), format!("MAX_PREPARED_STATEMENTS={n}")]);
        }
        args.push(PGBOUNCER_IMAGE.into());
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let started = docker(&args);
        let guard = Container(name);
        started?;
        let me = Bouncer {
            pg,
            port,
            _bouncer: guard,
            _net: net,
        };
        for _ in 0..60 {
            if PgConnector::new(&me.url())
                .unwrap()
                .connect()
                .is_ok_and(|mut c| c.simple_query("SELECT 1").is_ok())
            {
                return Some(me);
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        None
    }

    fn url(&self) -> String {
        format!(
            "postgres://postgres:obop-test@127.0.0.1:{}/postgres",
            self.port
        )
    }
}

/// The parts of the shared-state contract that do not hang on a few milliseconds. (The full
/// `shared_tests::shared_contract` lets counters and values live 50 to 120 ms, which a pooler in
/// a container, three round trips per call, cannot honour; it is run unchanged against direct
/// connections by `postgres_shared_contract_between_two_connections`.)
fn shared_state_checks(a: Arc<dyn Backend>, b: Arc<dyn Backend>) {
    // Counters are shared and never lose an increment.
    let k = unique("bctr");
    assert_eq!(a.counter_add(&k, 600_000, false).unwrap(), 1);
    assert_eq!(b.counter_add(&k, 600_000, false).unwrap(), 2);
    std::thread::scope(|s| {
        for i in 0..8 {
            let h = if i % 2 == 0 { a.clone() } else { b.clone() };
            let k = k.clone();
            s.spawn(move || {
                for _ in 0..10 {
                    h.counter_add(&k, 600_000, false).unwrap();
                }
            });
        }
    });
    assert_eq!(a.counter_get(&k).unwrap(), 82);
    // Short-lived values: put-if-absent, compare-and-set, scan.
    let k = unique("bkv");
    assert!(a.kv_put_new(&k, "one", Some(600_000)).unwrap());
    assert!(!b.kv_put_new(&k, "two", Some(600_000)).unwrap());
    assert!(!b.kv_swap(&k, "wrong", "x").unwrap());
    assert!(b.kv_swap(&k, "one", "three").unwrap());
    assert_eq!(a.kv_get(&k).unwrap().as_deref(), Some("three"));
    assert_eq!(a.kv_scan(&k).unwrap(), vec![(k.clone(), "three".into())]);
    assert!(a.kv_delete(&k).unwrap());
    // Locks exclude each other across the two handles (they live on direct connections).
    let key = unique("block");
    let inside = Arc::new(AtomicUsize::new(0));
    let worst = Arc::new(AtomicUsize::new(0));
    std::thread::scope(|s| {
        for i in 0..6 {
            let h = if i % 2 == 0 { a.clone() } else { b.clone() };
            let (key, inside, worst) = (key.clone(), inside.clone(), worst.clone());
            s.spawn(move || {
                for _ in 0..5 {
                    h.with_lock(&key, || {
                        let now = inside.fetch_add(1, Ordering::SeqCst) + 1;
                        worst.fetch_max(now, Ordering::SeqCst);
                        std::thread::sleep(Duration::from_millis(2));
                        inside.fetch_sub(1, Ordering::SeqCst);
                    })
                    .unwrap();
                }
            });
        }
    });
    assert_eq!(worst.load(Ordering::SeqCst), 1, "never two holders");
    // A batch is all or nothing, seen by the other handle.
    let model = unique("bapply");
    a.insert(&model, "keep", &fields(1)).unwrap();
    assert!(
        b.apply(&[
            Op::Put {
                model: model.clone(),
                id: "half".into(),
                fields: fields(2),
            },
            Op::Merge {
                model: model.clone(),
                id: "missing".into(),
                patch: fields(3),
            },
        ])
        .is_err()
    );
    assert!(a.get(&model, "half").unwrap().is_none(), "rolled back");
    b.apply(&[Op::Put {
        model: model.clone(),
        id: "whole".into(),
        fields: fields(4),
    }])
    .unwrap();
    assert_eq!(a.get(&model, "whole").unwrap().unwrap()["n"], 4);
    // Compare-and-set of fields, import and the snapshot export.
    let mut batches = vec![vec![(model.clone(), "imp".to_string(), fields(5))]].into_iter();
    assert_eq!(b.import(false, &mut || Ok(batches.next())).unwrap(), 1);
    let mut seen = 0;
    a.export(2, &mut |batch| {
        seen += batch.iter().filter(|(m, _, _)| *m == model).count();
        Ok(())
    })
    .unwrap();
    assert_eq!(seen, 3);
}

#[test]
fn behind_pgbouncer_in_transaction_mode_everything_works_with_locks_on_a_direct_address() {
    let _turn = serial();
    let Some(b) = Bouncer::start(None) else {
        eprintln!("skipping: docker, postgres:17 or edoburu/pgbouncer are not available");
        return;
    };
    let cfg = || PgPoolConfig {
        pooler: Pooler::Transaction,
        lock_url: Some(b.pg.url()),
        acquire_timeout: Duration::from_secs(60),
        ..PgPoolConfig::default()
    };
    // The whole storage contract, then the shared state between two handles that stand in for two
    // server processes, all through the pooler.
    let one = open_with(&b.url(), Some(cfg())).unwrap();
    crate::tests::contract(&*one);
    let (x, y) = (
        open_with(&b.url(), Some(cfg())).unwrap(),
        open_with(&b.url(), Some(cfg())).unwrap(),
    );
    shared_state_checks(x, y);
    // Eight client connections work through two server connections: no session-level state
    // (a SET, an advisory lock) was left on a server connection another client then uses.
    let pg = Arc::new(connect(&b.url(), cfg()));
    let model = unique("bounced");
    std::thread::scope(|s| {
        for t in 0..16 {
            let (pg, model) = (pg.clone(), model.clone());
            s.spawn(move || {
                for i in 0..25 {
                    pg.insert(&model, &format!("{t}-{i}"), &fields(i)).unwrap();
                    assert!(pg.get(&model, &format!("{t}-{i}")).unwrap().is_some());
                    pg.counter_add(&format!("bounce-{t}"), 60_000, false)
                        .unwrap();
                }
            });
        }
    });
    assert_eq!(pg.count(&model).unwrap(), 400);
    let mut seen = 0;
    pg.scan(&model, &mut |_, _| {
        seen += 1;
        Ok(true)
    })
    .unwrap();
    assert_eq!(seen, 400);
    let mut admin = PgConnector::new(&b.pg.url()).unwrap().connect().unwrap();
    let servers: i64 = admin
        .query_one(
            "SELECT count(*) FROM pg_stat_activity WHERE datname = 'postgres'              AND backend_type = 'client backend' AND pid <> pg_backend_pid()              AND application_name <> 'obop-lock'",
            &[],
        )
        .unwrap()
        .get(0);
    // The version the documentation says it was tried against, asked of PgBouncer itself.
    let console = format!(
        "postgres://postgres:obop-test@127.0.0.1:{}/pgbouncer",
        b.port
    );
    let version = PgConnector::new(&console)
        .unwrap()
        .connect()
        .unwrap()
        .simple_query("SHOW VERSION")
        .unwrap()
        .iter()
        .find_map(|m| match m {
            postgres::SimpleQueryMessage::Row(r) => r.get(0).map(str::to_string),
            _ => None,
        })
        .unwrap_or_default();
    assert!(version.contains("1.26"), "PgBouncer says {version:?}");
    eprintln!(
        "D74 PgBouncer {version}: {} client connections of the main pool shared {servers} server connections",
        pg.main.stats().connections_opened
    );
    assert!(
        servers <= 2,
        "{servers} server connections for the application"
    );
    // The locks went around the pooler: their sessions are on the server under the lock name.
    let g = pg.lock("bounced-lock").unwrap();
    let locks: i64 = admin
        .query_one(
            "SELECT count(*) FROM pg_stat_activity WHERE application_name = 'obop-lock'",
            &[],
        )
        .unwrap()
        .get(0);
    assert!(locks >= 1);
    drop(g);
    // Statement limits are per transaction behind the pooler: SET LOCAL, nothing left on a session.
    let timeouts_ok = pg.main.run(Class::Point, |c| {
        Ok(c.query_one("SHOW statement_timeout", &[])?
            .get::<_, String>(0))
    });
    assert_eq!(timeouts_ok.unwrap(), "5s");
    let e = pg
        .main
        .run(Class::Point, |c| sleep_query(c, 8))
        .unwrap_err();
    assert_eq!(Fault::of_message(&e), Some(Fault::Timeout), "{e}");
}

#[test]
fn pgbouncer_without_protocol_level_prepared_statements_works_too() {
    let _turn = serial();
    // `max_prepared_statements = 0`: PgBouncer forwards no named statement from one transaction
    // to the next, so every statement must be prepared and run inside one transaction. That is
    // what the transaction mode of the pool does.
    let Some(b) = Bouncer::start(Some("0")) else {
        eprintln!("skipping: docker, postgres:17 or edoburu/pgbouncer are not available");
        return;
    };
    let cfg = PgPoolConfig {
        pooler: Pooler::Transaction,
        lock_url: Some(b.pg.url()),
        acquire_timeout: Duration::from_secs(60),
        ..PgPoolConfig::default()
    };
    let one = open_with(&b.url(), Some(cfg)).unwrap();
    crate::tests::contract(&*one);
    let mut seen = 0;
    one.scan("no-such-model", &mut |_, _| {
        seen += 1;
        Ok(true)
    })
    .unwrap();
    assert_eq!(seen, 0);
}

#[test]
fn a_lock_address_that_reaches_another_database_is_refused_at_start_up() {
    let (url, _db) = pg_db!();
    let (head, _) = url.rsplit_once('/').unwrap();
    let other = format!("{head}/postgres");
    if other == url {
        eprintln!("skipped: the test database is already `postgres`");
        return;
    }
    let cfg = PgPoolConfig {
        lock_url: Some(other),
        ..PgPoolConfig::default()
    };
    let e = Pg::connect_with(&url, cfg).err().expect("must be refused");
    assert!(e.contains("OBOP_LOCK_DATABASE_URL"), "{e}");
    assert!(!e.contains("obop-test"), "no password in the message: {e}");
    // The same database through a separate address is accepted, and the lock sessions use it.
    let sep = if url.contains('?') { '&' } else { '?' };
    let app = unique("directlock");
    let cfg = PgPoolConfig {
        lock_url: Some(format!("{url}{sep}application_name={app}")),
        ..PgPoolConfig::default()
    };
    let pg = Pg::connect_with(&url, cfg).unwrap();
    let g = pg.lock(&unique("k")).unwrap();
    let mut admin = PgConnector::new(&url).unwrap().connect().unwrap();
    let n: i64 = admin
        .query_one(
            "SELECT count(*) FROM pg_stat_activity WHERE application_name = $1",
            &[&app],
        )
        .unwrap()
        .get(0);
    assert_eq!(n, 1, "the lock connection came from OBOP_LOCK_DATABASE_URL");
    drop(g);
    // A malformed lock address is refused with the variable's name and without its text.
    let cfg = PgPoolConfig {
        lock_url: Some("mysql://secret-user:secret-pw@h/db".into()),
        ..PgPoolConfig::default()
    };
    let e = Pg::connect_with(&url, cfg).err().unwrap();
    assert!(e.contains("OBOP_LOCK_DATABASE_URL"), "{e}");
    assert!(!e.contains("secret"), "{e}");
}
