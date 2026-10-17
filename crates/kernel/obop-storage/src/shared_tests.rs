//! The shared-state contract (locks, counters, short-lived values, export) on every backend. The
//! file and PostgreSQL runs open the database twice, as two server processes would.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

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

/// `a` and `b` are two handles on the same database.
fn shared_contract(a: Arc<dyn Backend>, b: Arc<dyn Backend>) {
    // ---- counters: fixed window, shared between handles, expiry restarts at 1 ----
    let k = unique("ctr");
    assert_eq!(a.counter_get(&k).unwrap(), 0);
    assert_eq!(a.counter_add(&k, 60_000, false).unwrap(), 1);
    assert_eq!(b.counter_add(&k, 60_000, false).unwrap(), 2);
    assert_eq!(a.counter_get(&k).unwrap(), 2);
    let left = b.counter_ttl_ms(&k).unwrap().unwrap();
    assert!(left > 50_000 && left <= 60_000, "{left}");
    b.counter_reset(&k).unwrap();
    assert_eq!(a.counter_get(&k).unwrap(), 0);
    let k = unique("ctr-exp");
    assert_eq!(a.counter_add(&k, 60, false).unwrap(), 1);
    assert_eq!(b.counter_add(&k, 60, false).unwrap(), 2);
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(a.counter_get(&k).unwrap(), 0, "expired reads as zero");
    assert_eq!(b.counter_add(&k, 60, false).unwrap(), 1, "and starts again");
    // `extend` pushes the expiry out on every hit.
    let k = unique("ctr-ext");
    for _ in 0..4 {
        a.counter_add(&k, 120, true).unwrap();
        std::thread::sleep(Duration::from_millis(60));
    }
    assert_eq!(a.counter_get(&k).unwrap(), 4);
    // 0 never expires.
    let k = unique("ctr-seq");
    assert_eq!(a.counter_add(&k, 0, false).unwrap(), 1);
    assert_eq!(b.counter_add(&k, 0, false).unwrap(), 2);
    assert_eq!(b.counter_ttl_ms(&k).unwrap(), None);

    // Many threads on two handles never lose an increment.
    let k = unique("ctr-par");
    std::thread::scope(|s| {
        for i in 0..8 {
            let h = if i % 2 == 0 { a.clone() } else { b.clone() };
            let k = k.clone();
            s.spawn(move || {
                for _ in 0..10 {
                    h.counter_add(&k, 60_000, false).unwrap();
                }
            });
        }
    });
    assert_eq!(a.counter_get(&k).unwrap(), 80);

    // ---- short-lived values ----
    let k = unique("kv");
    assert_eq!(a.kv_get(&k).unwrap(), None);
    assert!(a.kv_put_new(&k, "one", Some(60_000)).unwrap());
    assert!(
        !b.kv_put_new(&k, "two", Some(60_000)).unwrap(),
        "already there"
    );
    assert_eq!(b.kv_get(&k).unwrap().as_deref(), Some("one"));
    b.kv_put(&k, "three", None).unwrap();
    assert_eq!(a.kv_get(&k).unwrap().as_deref(), Some("three"));
    assert!(a.kv_delete(&k).unwrap());
    assert!(!a.kv_delete(&k).unwrap());
    assert_eq!(b.kv_get(&k).unwrap(), None);
    let k = unique("kv-exp");
    a.kv_put(&k, "x", Some(50)).unwrap();
    std::thread::sleep(Duration::from_millis(120));
    assert_eq!(b.kv_get(&k).unwrap(), None, "expired");
    assert!(
        b.kv_put_new(&k, "y", Some(60_000)).unwrap(),
        "an expired key is free again"
    );
    assert_eq!(a.kv_get(&k).unwrap().as_deref(), Some("y"));
    // scan by prefix lists live entries only
    let p = unique("scan");
    a.kv_put(&format!("{p}/1"), "a", None).unwrap();
    b.kv_put(&format!("{p}/2"), "b", None).unwrap();
    a.kv_put(&format!("{p}/3"), "c", Some(30)).unwrap();
    a.kv_put(&unique("other"), "z", None).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let got = b.kv_scan(&format!("{p}/")).unwrap();
    assert_eq!(
        got,
        vec![
            (format!("{p}/1"), "a".into()),
            (format!("{p}/2"), "b".into())
        ]
    );
    // purge removes what expired
    let k = unique("kv-purge");
    a.kv_put(&k, "x", Some(10)).unwrap();
    a.counter_add(&unique("ctr-purge"), 10, false).unwrap();
    std::thread::sleep(Duration::from_millis(60));
    assert!(a.purge_expired().unwrap() >= 2);

    // ---- locks: mutual exclusion across handles ----
    let key = unique("lock");
    let inside = Arc::new(AtomicUsize::new(0));
    let max_inside = Arc::new(AtomicUsize::new(0));
    let total = Arc::new(AtomicUsize::new(0));
    std::thread::scope(|s| {
        for i in 0..6 {
            let h = if i % 2 == 0 { a.clone() } else { b.clone() };
            let (key, inside, max_inside, total) = (
                key.clone(),
                inside.clone(),
                max_inside.clone(),
                total.clone(),
            );
            s.spawn(move || {
                for _ in 0..5 {
                    h.with_lock(&key, || {
                        let now = inside.fetch_add(1, Ordering::SeqCst) + 1;
                        max_inside.fetch_max(now, Ordering::SeqCst);
                        std::thread::sleep(Duration::from_millis(2));
                        total.fetch_add(1, Ordering::SeqCst);
                        inside.fetch_sub(1, Ordering::SeqCst);
                    })
                    .unwrap();
                }
            });
        }
    });
    assert_eq!(total.load(Ordering::SeqCst), 30);
    assert_eq!(max_inside.load(Ordering::SeqCst), 1, "never two holders");
    // A lock is free again after its holder drops it, and different keys do not block.
    let g1 = a.lock(&key).unwrap();
    let g2 = b.lock(&unique("other-lock")).unwrap();
    drop(g1);
    drop(g2);
    let _again = b.lock(&key).unwrap();

    // ---- export: whole database in batches, in insertion order, a stable snapshot ----
    let model = unique("exp");
    let rows: Vec<_> = (0..25)
        .map(|i| (model.clone(), format!("{i:03}"), fields(i)))
        .collect();
    a.insert_many(&rows).unwrap();
    let before = b.total().unwrap();
    assert!(before >= 25);
    let mut seen = Vec::new();
    let mut batches = 0;
    b.export(10, &mut |batch| {
        assert!(batch.len() <= 10);
        batches += 1;
        for (m, id, f) in batch {
            if *m == model {
                seen.push((id.clone(), f["n"].as_i64().unwrap()));
            }
        }
        Ok(())
    })
    .unwrap();
    assert!(batches >= 3);
    assert_eq!(seen.len(), 25);
    assert_eq!(seen[0], ("000".to_string(), 0));
    assert_eq!(seen[24], ("024".to_string(), 24));
    // A failing sink stops the export and reports its error.
    assert_eq!(
        a.export(5, &mut |_| Err("stop".into())).unwrap_err(),
        "stop"
    );
}

#[test]
fn memory_shared_contract() {
    let a = open("memory:").unwrap();
    shared_contract(a.clone(), a);
}

#[test]
fn sqlite_shared_contract_between_two_handles_on_one_file() {
    let path = std::env::temp_dir().join(unique("obop-shared").replace('/', "_") + ".db");
    let url = format!("sqlite://{}", path.display());
    shared_contract(open(&url).unwrap(), open(&url).unwrap());
    for entry in std::fs::read_dir(std::env::temp_dir()).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(path.file_name().unwrap().to_str().unwrap()) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[test]
fn postgres_shared_contract_between_two_connections() {
    let Ok(url) = std::env::var("OBOP_TEST_POSTGRES_URL") else {
        eprintln!("skipped: set OBOP_TEST_POSTGRES_URL to run against PostgreSQL");
        return;
    };
    shared_contract(open(&url).unwrap(), open(&url).unwrap());
}

#[test]
fn a_panicking_holder_releases_its_lock() {
    // A guard dropped by unwinding releases the lock.
    let b = open("memory:").unwrap();
    let b2 = b.clone();
    let r = std::thread::spawn(move || {
        let _g = b2.lock("boom").unwrap();
        panic!("holder dies");
    })
    .join();
    assert!(r.is_err());
    let _g = b.lock("boom").unwrap();
}
