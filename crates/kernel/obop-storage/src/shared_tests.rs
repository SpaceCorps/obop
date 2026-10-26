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
/// `destructive` also tests the wiping import, which empties the database: only for databases
/// nothing else is using.
pub(crate) fn shared_contract(a: Arc<dyn Backend>, b: Arc<dyn Backend>, destructive: bool) {
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

    // ---- compare-and-set of fields and values ----
    let model = unique("swap");
    let mut f = Fields::new();
    f.insert("secret".into(), J::from("old"));
    f.insert("other".into(), J::from(1));
    a.insert(&model, "r1", &f).unwrap();
    a.insert(&model, "r2", &f).unwrap();
    let swap = |id: &str, old: &str, new: &str| Swap {
        model: model.clone(),
        id: id.into(),
        field: "secret".into(),
        old: old.into(),
        new: new.into(),
    };
    // One swap is stale (wrong old value), one record is missing: only the good one applies.
    let n = b
        .swap_fields(&[
            swap("r1", "old", "new1"),
            swap("r2", "stale", "never"),
            swap("gone", "old", "x"),
        ])
        .unwrap();
    assert_eq!(n, 1);
    assert_eq!(a.get(&model, "r1").unwrap().unwrap()["secret"], "new1");
    assert_eq!(
        a.get(&model, "r1").unwrap().unwrap()["other"],
        1,
        "the rest is kept"
    );
    assert_eq!(a.get(&model, "r2").unwrap().unwrap()["secret"], "old");
    let k = unique("kv-swap");
    a.kv_put(&k, "v1", Some(60_000)).unwrap();
    assert!(!b.kv_swap(&k, "wrong", "v2").unwrap());
    assert!(b.kv_swap(&k, "v1", "v2").unwrap());
    assert_eq!(a.kv_get(&k).unwrap().as_deref(), Some("v2"));
    assert!(!a.kv_swap(&unique("absent"), "a", "b").unwrap());

    // ---- apply: several writes, all or nothing ----
    let model = unique("apply");
    let mut f1 = Fields::new();
    f1.insert("n".into(), J::from(1));
    let mut patch = Fields::new();
    patch.insert("n".into(), J::from(2));
    a.insert(&model, "keep", &f1).unwrap();
    a.insert(&model, "gone", &f1).unwrap();
    b.apply(&[
        Op::Put {
            model: model.clone(),
            id: "new".into(),
            fields: f1.clone(),
        },
        Op::Put {
            model: model.clone(),
            id: "keep".into(),
            fields: patch.clone(),
        },
        Op::Merge {
            model: model.clone(),
            id: "new".into(),
            patch: patch.clone(),
        },
        Op::Delete {
            model: model.clone(),
            id: "gone".into(),
        },
    ])
    .unwrap();
    assert_eq!(a.get(&model, "new").unwrap().unwrap()["n"], 2);
    assert_eq!(
        a.get(&model, "keep").unwrap().unwrap()["n"],
        2,
        "Put replaces"
    );
    assert!(a.get(&model, "gone").unwrap().is_none());
    // A failing batch leaves nothing behind.
    assert!(
        b.apply(&[
            Op::Put {
                model: model.clone(),
                id: "half".into(),
                fields: f1.clone()
            },
            Op::Delete {
                model: model.clone(),
                id: "keep".into()
            },
            Op::Merge {
                model: model.clone(),
                id: "missing".into(),
                patch
            },
        ])
        .is_err()
    );
    assert!(a.get(&model, "half").unwrap().is_none());
    assert!(a.get(&model, "keep").unwrap().is_some(), "rolled back");

    // ---- import: one transaction, optionally replacing everything ----
    let model = unique("imp");
    let mk = |n: i64| -> Row { (model.clone(), format!("{n:03}"), fields(n)) };
    let mut batches = vec![vec![mk(1), mk(2)], vec![mk(3)]].into_iter();
    // A failure in the middle of the stream rolls the whole import back.
    let mut calls = 0;
    let failed = b.import(false, &mut || {
        calls += 1;
        if calls == 2 {
            return Err("source broke".into());
        }
        Ok(batches.next())
    });
    assert_eq!(failed.unwrap_err(), "source broke");
    assert_eq!(
        a.count(&model).unwrap(),
        0,
        "nothing of a failed import is kept"
    );
    let mut batches = vec![vec![mk(1), mk(2)], vec![mk(3)]].into_iter();
    assert_eq!(b.import(false, &mut || Ok(batches.next())).unwrap(), 3);
    assert_eq!(a.count(&model).unwrap(), 3);
    // A duplicate fails the import as a whole.
    let mut again = vec![vec![mk(9), mk(1)]].into_iter();
    assert!(a.import(false, &mut || Ok(again.next())).is_err());
    assert!(b.get(&model, "009").unwrap().is_none());
    // A wiping import replaces everything, short-lived values included.
    if destructive {
        let k = unique("kv-wipe");
        a.kv_put(&k, "x", None).unwrap();
        let mut only = vec![vec![mk(7)]].into_iter();
        assert_eq!(b.import(true, &mut || Ok(only.next())).unwrap(), 1);
        assert_eq!(a.total().unwrap(), 1);
        assert_eq!(a.kv_get(&k).unwrap(), None);
        assert_eq!(a.get(&model, "007").unwrap().unwrap()["n"], 7);
    }

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

    concurrent_batches(a, b);
}

/// Batches that run at the same moment, on one handle or two, behave as if they ran one after the
/// other (D74: the PostgreSQL backend no longer serialises a process's calls on one connection,
/// so it orders and repeats them itself).
pub(crate) fn concurrent_batches(a: Arc<dyn Backend>, b: Arc<dyn Backend>) {
    let model = unique("conc");
    let put = |db: &Arc<dyn Backend>, id: &str| {
        db.insert(&model, id, &fields(0)).unwrap();
    };
    put(&a, "x");
    put(&a, "y");
    let merge = |id: &str, n: i64| Op::Merge {
        model: model.clone(),
        id: id.into(),
        patch: fields(n),
    };
    // Two kinds of batch name the same two records in opposite order. Taking row locks in the
    // order of the steps would let two of them deadlock.
    std::thread::scope(|s| {
        for t in 0..6 {
            let h = if t % 2 == 0 { a.clone() } else { b.clone() };
            let (merge, model) = (&merge, &model);
            s.spawn(move || {
                for round in 0..25 {
                    let ops = if t % 3 == 0 {
                        vec![merge("x", round), merge("y", round)]
                    } else {
                        vec![
                            merge("y", round),
                            merge("x", round),
                            Op::add(model, "x", "n", 0),
                        ]
                    };
                    h.apply_checked(&ops)
                        .unwrap_or_else(|e| panic!("thread {t} round {round}: {e}"));
                }
            });
        }
    });
    // A counter created by whoever comes first: every increment counts, none is lost, no caller
    // fails because another created the record at the same moment.
    let ctr = Fields::new();
    std::thread::scope(|s| {
        for t in 0..6 {
            let h = if t % 2 == 0 { a.clone() } else { b.clone() };
            let (model, ctr) = (&model, &ctr);
            s.spawn(move || {
                for _ in 0..20 {
                    h.apply_checked(&[Op::add(model, "counter", "n", 1).or_create(ctr.clone())])
                        .unwrap_or_else(|e| panic!("thread {t}: {e}"));
                }
            });
        }
    });
    assert_eq!(a.get(&model, "counter").unwrap().unwrap()["n"], 120);
    // Several callers insert the same new record at the same moment: exactly one wins and the
    // others are told the record exists (a conflict, not a storage failure).
    let winners = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for t in 0..6 {
            let h = if t % 2 == 0 { a.clone() } else { b.clone() };
            let (model, winners) = (&model, &winners);
            s.spawn(move || {
                let r = h.apply_checked(&[Op::Insert {
                    model: model.clone(),
                    id: "once".into(),
                    fields: fields(t),
                }]);
                match r {
                    Ok(()) => {
                        winners.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(ApplyError::Conflict { .. }) => {}
                    Err(e) => panic!("thread {t}: a refusal was expected, not {e}"),
                }
            });
        }
    });
    assert_eq!(winners.load(Ordering::SeqCst), 1);
}

#[test]
fn memory_shared_contract() {
    let a = open("memory:").unwrap();
    shared_contract(a.clone(), a, true);
}

#[test]
fn sqlite_shared_contract_between_two_handles_on_one_file() {
    let path = std::env::temp_dir().join(unique("obop-shared").replace('/', "_") + ".db");
    let url = format!("sqlite://{}", path.display());
    shared_contract(open(&url).unwrap(), open(&url).unwrap(), true);
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
    shared_contract(open(&url).unwrap(), open(&url).unwrap(), false);
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
