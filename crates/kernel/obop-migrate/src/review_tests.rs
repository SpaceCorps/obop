//! Tests written for the review of D73: adoption after a rollback, databases that only operator
//! commands have written to, the name of a database, and the backup hook of a server that
//! migrates itself.

use super::tests::{marking, mem, ring, version};
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn rows(b: &Arc<dyn Backend>) -> Vec<Entry> {
    ledger::read_all(&**b).unwrap()
}

#[test]
fn a_database_holding_only_operator_records_is_still_new_and_gets_stamped() {
    // `obop migrate plan`, `obop storage attest` and `obop backup run` write before the first
    // start. They must not make a new database look older than versioning.
    let b = mem();
    for (model, id) in [
        (ledger::PLAN_MODEL, "p1"),
        ("obop_backup", "b1"),
        ("obop_restore_drill", "d1"),
        ("obop_storage_attestation", "a1"),
    ] {
        b.insert(model, id, &Fields::new()).unwrap();
    }
    let m = Migrator::new(b.clone(), ring());
    let st = m.status().unwrap();
    assert!(st.empty, "{st:?}");
    assert!(m.stamp_if_fresh().unwrap());
    assert_eq!(version(&b), Some(u64::from(m.latest())));
    assert_eq!(rows(&b).len(), m.latest() as usize);
    // One real record and it is an old database again.
    let old = mem();
    old.insert(ledger::PLAN_MODEL, "p1", &Fields::new())
        .unwrap();
    old.insert("auth_user", "ada", &Fields::new()).unwrap();
    let m = Migrator::new(old, ring());
    assert!(!m.status().unwrap().empty);
    assert!(!m.stamp_if_fresh().unwrap());
}

#[test]
fn a_rollback_row_does_not_stand_in_for_the_rows_of_the_steps() {
    // A pre-ledger database restored by `obop migrate rollback` has a rollback row, which is a
    // "core" row, and still needs its steps adopted so that drift protection works.
    let b = mem();
    let mut f = Fields::new();
    f.insert("version".into(), J::from(3));
    b.insert(META_MODEL, META_ID, &f).unwrap();
    let m = Migrator::with_steps(b.clone(), ring(), marking());
    m.record_rollback("because", "obop-backup-old.jsonl.gz", Some(3), Some(3), &[])
        .unwrap();
    assert_eq!(rows(&b).len(), 1);
    assert!(m.check_for_serve(false, "x").unwrap().is_empty());
    let ids: Vec<String> = rows(&b).into_iter().map(|r| r.id).collect();
    for v in 1..=3 {
        assert!(ids.contains(&ledger::core_id(v)), "{v}: {ids:?}");
    }
    // And now an edited step is refused at start.
    let mut edited = marking();
    edited[1] = edited[1].source("fn two() { changed }");
    let e = Migrator::with_steps(b, ring(), edited)
        .check_for_serve(false, "x")
        .unwrap_err();
    assert!(e.contains("step 2"), "{e}");
}

#[test]
fn a_database_gets_one_name_which_is_never_changed() {
    let b = mem();
    let m = Migrator::new(b.clone(), ring());
    assert_eq!(m.instance_id().unwrap(), None);
    m.stamp_if_fresh().unwrap();
    let id = m.instance_id().unwrap().expect("a new database is named");
    assert_eq!(id.len(), 32);
    assert_eq!(m.ensure_instance_id().unwrap(), id);
    // A database from before names is named by the first run or start, once.
    let old = mem();
    old.insert("auth_user", "ada", &Fields::new()).unwrap();
    let m = Migrator::with_steps(old.clone(), ring(), marking());
    m.run(false).unwrap();
    let named = m.instance_id().unwrap().expect("a run names the database");
    m.run(false).unwrap();
    assert_eq!(m.ensure_instance_id().unwrap(), named);
    // A dry run writes nothing, not even a name.
    let dry = mem();
    dry.insert("auth_user", "ada", &Fields::new()).unwrap();
    Migrator::with_steps(dry.clone(), ring(), marking())
        .run(true)
        .unwrap();
    assert_eq!(
        Migrator::with_steps(dry, ring(), marking())
            .instance_id()
            .unwrap(),
        None
    );
}

fn counting_hook(calls: &Arc<AtomicUsize>) -> Before {
    let calls = calls.clone();
    Arc::new(move |p: &Pending| {
        calls.fetch_add(1, Ordering::SeqCst);
        assert!(!p.steps.is_empty());
        Ok(Some("backup-taken-under-the-lock".to_string()))
    })
}

#[test]
fn the_backup_hook_runs_under_the_lock_and_only_when_a_step_will_run() {
    let calls = Arc::new(AtomicUsize::new(0));
    let b = mem();
    b.insert("auth_user", "ada", &Fields::new()).unwrap();
    let m = Migrator::with_steps(b.clone(), ring(), marking()).by("server");
    // The lock is held while the hook runs: a second taker waits behind it.
    let probe = b.clone();
    let held = Arc::new(AtomicUsize::new(0));
    let held2 = held.clone();
    let hook: Before = Arc::new(move |_p| {
        let p = probe.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(p.lock("obop.migrate").map(drop).is_ok());
        });
        if rx
            .recv_timeout(std::time::Duration::from_millis(300))
            .is_err()
        {
            held2.store(1, Ordering::SeqCst);
        }
        Ok(Some("id".into()))
    });
    m.run_with(&RunOptions {
        before: Some(hook),
        ..RunOptions::default()
    })
    .unwrap();
    assert_eq!(
        held.load(Ordering::SeqCst),
        1,
        "the hook ran under the lock"
    );
    // Every ledger row of the run names the backup the hook returned.
    assert!(
        rows(&b)
            .iter()
            .all(|r| r.backup_id.as_deref() == Some("id"))
    );

    // Nothing pending: the hook is not called, and no backup is wasted.
    let opts = RunOptions {
        before: Some(counting_hook(&calls)),
        ..RunOptions::default()
    };
    let done = mem();
    Migrator::with_steps(done.clone(), ring(), marking())
        .run(false)
        .unwrap();
    Migrator::with_steps(done, ring(), marking())
        .run_with(&opts)
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    // A dry run does not call it either.
    Migrator::with_steps(mem(), ring(), marking())
        .run_with(&RunOptions {
            dry_run: true,
            ..opts.clone()
        })
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    // A hook that fails stops the run before any write.
    let failing = mem();
    failing.insert("auth_user", "ada", &Fields::new()).unwrap();
    let m = Migrator::with_steps(failing.clone(), ring(), marking());
    let e = m
        .run_with(&RunOptions {
            before: Some(Arc::new(|_| Err("no room for the backup".into()))),
            ..RunOptions::default()
        })
        .unwrap_err();
    assert_eq!(e, "no room for the backup");
    assert_eq!(version(&failing), None);
    assert!(rows(&failing).is_empty());
}

#[test]
fn a_start_that_will_be_refused_never_reaches_the_backup() {
    let calls = Arc::new(AtomicUsize::new(0));
    // Drift: step 1 ran, then its source was edited, and step 3 is pending.
    let b = mem();
    Migrator::with_steps(b.clone(), ring(), marking()[..2].to_vec())
        .run(false)
        .unwrap();
    let mut edited = marking();
    edited[0] = edited[0].source("fn one() { changed }");
    let m = Migrator::with_steps(b, ring(), edited);
    let with_hook = RunOptions {
        before: Some(counting_hook(&calls)),
        ..RunOptions::default()
    };
    let e = m.check_for_serve_with(true, "x", &with_hook).unwrap_err();
    assert!(e.contains("step 1"), "{e}");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "no backup for a refused start"
    );
    // A database from the future is refused the same way.
    let future = mem();
    let mut f = Fields::new();
    f.insert("version".into(), J::from(9));
    future.insert(META_MODEL, META_ID, &f).unwrap();
    let e = Migrator::with_steps(future, ring(), marking())
        .check_for_serve_with(true, "x", &with_hook)
        .unwrap_err();
    assert!(e.contains("newer"), "{e}");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn the_start_refusal_names_the_gated_procedure_and_the_plan() {
    let b = mem();
    b.insert("auth_user", "ada", &Fields::new()).unwrap();
    let e = Migrator::with_steps(b, ring(), marking())
        .check_for_serve(false, "postgres://***@h/db")
        .unwrap_err();
    for needle in [
        "obop migrate plan --db postgres://***@h/db",
        "--rehearsal",
        "--disk-ok",
        "docs/DEPLOY.md",
        "--auto-migrate",
    ] {
        assert!(e.contains(needle), "{needle}: {e}");
    }
    assert!(!e.contains("Take a backup, then run"), "{e}");
}
