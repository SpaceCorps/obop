//! The ledger, drift, adoption and plan tests (OBR-DAT-051, D73).

use super::tests::{marking, mem, put_log, ring, version};
use super::*;

fn rows(b: &Arc<dyn Backend>) -> Vec<Entry> {
    ledger::read_all(&**b).unwrap()
}

fn row(b: &Arc<dyn Backend>, version: u32) -> Entry {
    rows(b)
        .into_iter()
        .find(|r| r.id == ledger::core_id(version))
        .unwrap_or_else(|| panic!("no ledger row for step {version}"))
}

#[test]
fn the_checksums_of_the_shipped_steps_are_pinned() {
    // An applied step is never edited. If this fails you changed a step that databases already
    // ran: restore it and add a new step instead (docs/DEPLOY.md, "The migration ledger").
    let sums: Vec<String> = steps().iter().map(Step::checksum).collect();
    assert_eq!(
        sums,
        [
            "44d3dbae2a33a92b862ff0d4495ce55aaf7f304404c55c164a1494e3d52fe7b0",
            "e5798bd543c3658eb404b7b1ee74bdce867ddb4ab16c805696e9a5c85b7c4fc8",
        ]
    );
    assert!(check_declarations(&steps()).is_ok());
}

#[test]
fn every_step_that_runs_leaves_a_ledger_row_with_its_checksum_and_who_ran_it() {
    let b = mem();
    let m = Migrator::with_steps(b.clone(), ring(), marking()).by("tester");
    m.run_with(&RunOptions {
        backup_id: Some("obop-backup-x.jsonl.gz".into()),
        ..RunOptions::default()
    })
    .unwrap();
    let all = rows(&b);
    assert_eq!(all.len(), 3, "one row per step: {all:?}");
    for (i, s) in marking().iter().enumerate() {
        let r = row(&b, i as u32 + 1);
        assert_eq!(r.checksum, s.checksum());
        assert_eq!(r.status, EntryStatus::Applied);
        assert_eq!(
            (r.module.as_str(), r.kind.as_str(), r.by.as_str()),
            ("core", "rust", "tester")
        );
        assert_eq!(r.role, "application");
        assert_eq!(r.app_version, obop_types::CORE_VERSION);
        assert_eq!(r.backup_id.as_deref(), Some("obop-backup-x.jsonl.gz"));
        assert!(r.at > 1_700_000_000, "{}", r.at);
        assert_eq!(r.version, Some(i as u32 + 1));
    }
    assert_eq!(row(&b, 2).class, Class::Backfill);
    assert_eq!(row(&b, 2).note, "2 write(s)");
    // The checksum depends on the definition: a changed class or source is another checksum.
    let s = marking()[0];
    assert_ne!(s.checksum(), s.class(Class::Contract).checksum());
    assert_ne!(s.checksum(), s.source("fn one() { x }").checksum());
    // A dry run writes no row at all.
    let fresh = mem();
    Migrator::with_steps(fresh.clone(), ring(), marking())
        .run(true)
        .unwrap();
    assert!(rows(&fresh).is_empty());
}

#[test]
fn editing_an_applied_step_blocks_start_and_names_the_step() {
    let b = mem();
    Migrator::with_steps(b.clone(), ring(), marking())
        .run(false)
        .unwrap();
    // Step 2's source is edited after it ran.
    let mut edited = marking();
    edited[1] = edited[1].source("fn two() { more }");
    let m = Migrator::with_steps(b.clone(), ring(), edited.clone());
    let d = m.drift().unwrap();
    assert_eq!(d.len(), 1);
    assert_eq!((d[0].version, d[0].name.as_str()), (2, "two"));
    let e = m.check_for_serve(true, "x").unwrap_err();
    assert!(
        e.contains("step 2") && e.contains("(two)") && e.contains("changed"),
        "{e}"
    );
    assert!(m.run(false).unwrap_err().contains("step 2"));
    let v = m.verify().unwrap();
    assert!(!v.ok() && v.problems[0].contains("step 2"), "{v:?}");
    // Reformatting and a new comment are not an edit.
    let mut tidy = marking();
    tidy[1] = tidy[1].source("// a note\nfn two() {}   \n\n");
    let m = Migrator::with_steps(b.clone(), ring(), tidy);
    assert!(m.drift().unwrap().is_empty());
    assert!(m.check_for_serve(false, "x").unwrap().is_empty());
    // A different name counts: it is part of the definition too.
    let mut renamed = marking();
    renamed[0].name = "uno";
    assert_eq!(
        Migrator::with_steps(b, ring(), renamed).drift().unwrap()[0].version,
        1
    );
}

#[test]
fn a_step_that_fails_inside_its_transaction_leaves_a_failed_row_and_no_step_row() {
    let b = mem();
    let mut steps = marking();
    steps[1].run = Run::Ops(|_| {
        Ok(vec![
            put_log("half"),
            Op::Merge {
                model: "log".into(),
                id: "nope".into(),
                patch: Fields::new(),
            },
        ])
    });
    let m = Migrator::with_steps(b.clone(), ring(), steps);
    let e = m.run(false).unwrap_err();
    assert!(e.contains("step 2"), "{e}");
    let all = rows(&b);
    assert!(
        all.iter()
            .any(|r| r.id == ledger::core_id(1) && r.status == EntryStatus::Applied)
    );
    assert!(
        !all.iter().any(|r| r.id == ledger::core_id(2)),
        "no row for the step itself"
    );
    let failed: Vec<_> = all
        .iter()
        .filter(|r| r.status == EntryStatus::Failed)
        .collect();
    assert_eq!(failed.len(), 1);
    assert!(
        failed[0].id.starts_with("core/0002/failed-") && !failed[0].note.is_empty(),
        "{:?}",
        failed[0]
    );
    assert!(
        b.get("log", "half").unwrap().is_none(),
        "database as before"
    );
    assert_eq!(version(&b), Some(1));
}

static INTERRUPTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

#[test]
fn an_interrupted_direct_step_leaves_partial_and_resumes() {
    let b = mem();
    let mut steps = marking();
    steps[2].run = Run::Direct(|c| {
        c.backend.apply(&[put_log("batch-1")])?;
        if INTERRUPTED.swap(false, std::sync::atomic::Ordering::SeqCst) {
            return Err("the process was killed".into());
        }
        c.backend.apply(&[put_log("batch-2")])?;
        Ok("finished".into())
    });
    let m = Migrator::with_steps(b.clone(), ring(), steps);
    assert!(m.run(false).is_err());
    assert_eq!(row(&b, 3).status, EntryStatus::Partial);
    assert_eq!(version(&b), Some(2), "the version does not move");
    let v = m.verify().unwrap();
    assert!(v.ok(), "{v:?}");
    assert!(v.notes.iter().any(|n| n.contains("resumes")), "{v:?}");
    // The next run resumes the same step and finishes it: one row, now applied.
    let r = m.run(false).unwrap();
    assert_eq!(r.applied.len(), 1);
    assert_eq!(row(&b, 3).status, EntryStatus::Applied);
    assert_eq!(
        rows(&b)
            .iter()
            .filter(|r| r.id == ledger::core_id(3))
            .count(),
        1
    );
    assert_eq!(version(&b), Some(3));
    assert!(b.get("log", "batch-2").unwrap().is_some());
}

#[test]
fn a_database_from_before_the_ledger_is_adopted_without_replaying_anything() {
    let b = mem();
    // What the previous build left: the version record and a line per step, no ledger.
    b.apply(&[
        Op::Put {
            model: META_MODEL.into(),
            id: META_ID.into(),
            fields: serde_json::from_str(r#"{"version":2,"core":"0.2.0","step":"two","at":100}"#)
                .unwrap(),
        },
        Op::Put {
            model: META_MODEL.into(),
            id: "step-0001".into(),
            fields: serde_json::from_str(r#"{"version":1,"at":111}"#).unwrap(),
        },
        Op::Put {
            model: META_MODEL.into(),
            id: "step-0002".into(),
            fields: serde_json::from_str(r#"{"version":2,"at":222}"#).unwrap(),
        },
    ])
    .unwrap();
    let m = Migrator::with_steps(b.clone(), ring(), marking());
    assert!(
        m.verify()
            .unwrap()
            .notes
            .iter()
            .any(|n| n.contains("no ledger row"))
    );
    // Serving a database that is behind refuses, and adopting happens in the run.
    assert!(m.check_for_serve(false, "x").is_err());
    m.run(false).unwrap();
    for v in [1, 2] {
        let r = row(&b, v);
        assert_eq!(r.status, EntryStatus::Adopted, "{v}");
        assert_eq!(r.checksum, marking()[v as usize - 1].checksum());
    }
    assert_eq!(row(&b, 1).at, 111, "the time the old build recorded");
    assert_eq!(row(&b, 3).status, EntryStatus::Applied);
    assert!(
        b.get("log", "one").unwrap().is_none(),
        "steps 1 and 2 were not replayed"
    );
    assert!(b.get("log", "three").unwrap().is_some());
    // An up-to-date database from before the ledger is adopted at the next start.
    let up = mem();
    up.apply(&[Op::Put {
        model: META_MODEL.into(),
        id: META_ID.into(),
        fields: serde_json::from_str(r#"{"version":3,"at":1}"#).unwrap(),
    }])
    .unwrap();
    let m = Migrator::with_steps(up.clone(), ring(), marking());
    assert!(m.check_for_serve(false, "x").unwrap().is_empty());
    assert_eq!(rows(&up).len(), 3);
    assert!(rows(&up).iter().all(|r| r.status == EntryStatus::Adopted));
    assert!(m.verify().unwrap().ok());
    // Adopting twice adds nothing.
    assert!(m.check_for_serve(false, "x").unwrap().is_empty());
    assert_eq!(rows(&up).len(), 3);
}

#[test]
fn a_fresh_database_records_its_history_as_adopted() {
    let b = mem();
    let m = Migrator::with_steps(b.clone(), ring(), marking());
    assert!(m.stamp_if_fresh().unwrap());
    let all = rows(&b);
    assert_eq!(all.len(), 3);
    assert!(
        all.iter()
            .all(|r| r.status == EntryStatus::Adopted && r.note.contains("created at this version"))
    );
    assert!(m.verify().unwrap().ok());
    assert!(m.run(false).unwrap().applied.is_empty());
}

#[test]
fn verify_reports_a_build_older_than_the_ledger_and_a_row_left_unfinished() {
    let b = mem();
    Migrator::with_steps(b.clone(), ring(), marking())
        .run(false)
        .unwrap();
    // An older build reading a newer database.
    let older = Migrator::with_steps(b.clone(), ring(), marking()[..2].to_vec());
    let v = older.verify().unwrap();
    assert!(v.problems.iter().any(|p| p.contains("newer")), "{v:?}");
    // A row that says unfinished under a version that is past it.
    let mut e = row(&b, 2);
    e.status = EntryStatus::Partial;
    b.apply(&[e.put()]).unwrap();
    let v = Migrator::with_steps(b.clone(), ring(), marking())
        .verify()
        .unwrap();
    assert!(v.problems.iter().any(|p| p.contains("unfinished")), "{v:?}");
}

#[test]
fn two_runners_started_together_leave_one_ledger_row_per_step() {
    let b = mem();
    let steps = marking();
    std::thread::scope(|s| {
        for _ in 0..4 {
            let (b, steps) = (b.clone(), steps.clone());
            s.spawn(move || {
                Migrator::with_steps(b, ring(), steps).run(false).unwrap();
            });
        }
    });
    let all = rows(&b);
    assert_eq!(all.len(), 3, "{all:?}");
    assert!(all.iter().all(|r| r.status == EntryStatus::Applied));
}

#[test]
fn a_plan_lists_the_pending_steps_has_a_stable_id_and_is_kept_for_the_run() {
    let b = mem();
    let m = Migrator::with_steps(b.clone(), ring(), marking());
    let p = m.plan().unwrap();
    assert_eq!((p.from, p.to, p.steps.len()), (0, 3, 3));
    assert_eq!(p.id.len(), 16);
    assert_eq!(
        p.id,
        m.plan().unwrap().id,
        "the same database and build, the same plan"
    );
    assert!(
        p.describe().contains("step 2: two [backfill, ddl none"),
        "{}",
        p.describe()
    );
    assert!(m.stored_plan(&p.id).unwrap().is_none());
    m.store_plan(&p).unwrap();
    let back = m.stored_plan(&p.id).unwrap().unwrap();
    assert_eq!(back.steps, p.steps);
    assert_eq!((back.from, back.to), (0, 3));
    // Once a step has run the plan is another plan.
    Migrator::with_steps(b.clone(), ring(), marking()[..1].to_vec())
        .run(false)
        .unwrap();
    let p2 = m.plan().unwrap();
    assert_eq!((p2.from, p2.steps.len()), (1, 2));
    assert_ne!(p2.id, p.id);
    // A changed step is another plan.
    let mut edited = marking();
    edited[2] = edited[2].source("fn three() { x }");
    assert_ne!(
        Migrator::with_steps(b.clone(), ring(), edited)
            .plan()
            .unwrap()
            .id,
        p2.id
    );
    // Nothing pending says so.
    m.run(false).unwrap();
    assert!(m.plan().unwrap().is_empty());
    assert!(m.plan().unwrap().describe().contains("nothing to do"));
}

#[test]
fn a_blocking_step_on_a_big_table_runs_only_in_a_window() {
    let b = mem();
    for i in 0..6 {
        b.insert("big", &i.to_string(), &Fields::new()).unwrap();
    }
    let steps = vec![
        Step::new(1, "index", Run::Ops(|_| Ok(Vec::new())))
            .ddl(DdlClass::Blocking)
            .tables(&["big"])
            .source("x"),
    ];
    let m = Migrator::with_steps(b.clone(), ring(), steps.clone()).with_window_rows(5);
    let p = m.plan().unwrap();
    assert!(p.needs_window() && p.steps[0].tables == [("big".to_string(), 6)]);
    assert!(p.describe().contains("NEEDS --window"));
    let e = m.run(false).unwrap_err();
    assert!(e.contains("--window") && e.contains("step 1"), "{e}");
    assert_eq!(version(&b), None, "nothing ran");
    m.run_with(&RunOptions {
        window: true,
        ..RunOptions::default()
    })
    .unwrap();
    assert_eq!(version(&b), Some(1));
    assert_eq!(row(&b, 1).ddl, DdlClass::Blocking);
    // A concurrent step on the same table needs no window.
    let ok = Migrator::with_steps(
        b.clone(),
        ring(),
        vec![steps[0].ddl(DdlClass::Concurrent).tables(&["big"])],
    )
    .with_window_rows(5);
    assert!(!ok.plan().unwrap().needs_window());
}

#[test]
fn a_step_that_is_not_compatible_with_the_previous_release_must_declare_downtime() {
    let ok = Step::new(1, "x", Run::Ops(|_| Ok(Vec::new())));
    assert!(check_declarations(&[ok]).is_ok());
    let mut bad = ok;
    bad.compat_n_minus_1 = false;
    let e = check_declarations(&[bad]).unwrap_err();
    assert!(e.contains("step 1") && e.contains("downtime"), "{e}");
    assert!(check_declarations(&[ok.needs_downtime()]).is_ok());
    let m = Migrator::with_steps(mem(), ring(), vec![ok.needs_downtime()]);
    let p = m.plan().unwrap();
    assert!(p.downtime() && p.describe().contains("needs downtime"));
}

#[test]
fn a_rollback_is_recorded_with_its_reason_and_what_it_discarded() {
    let b = mem();
    let m = Migrator::with_steps(b.clone(), ring(), marking());
    m.run(false).unwrap();
    let before = rows(&b);
    m.record_rollback(
        "step 3 corrupted the index",
        "obop-backup-1.jsonl.gz",
        Some(3),
        Some(2),
        &before,
    )
    .unwrap();
    let r = b
        .list(ledger::MODEL)
        .unwrap()
        .into_iter()
        .find(|(id, _)| id.starts_with("core/rollback-"))
        .unwrap()
        .1;
    assert_eq!(r["note"], "step 3 corrupted the index");
    assert_eq!(r["backup_id"], "obop-backup-1.jsonl.gz");
    assert_eq!(
        (r["schema_before"].clone(), r["version"].clone()),
        (3.into(), 2.into())
    );
    assert_eq!(r["discarded"].as_array().unwrap().len(), 3);
    // The ledger still reads: the rollback row is not a step.
    assert!(m.verify().unwrap().ok());
}

#[test]
fn the_ledger_works_on_a_sqlite_file_shared_by_two_handles() {
    let path = std::env::temp_dir().join(format!("obop-ledger-{}.db", std::process::id()));
    let url = format!("sqlite://{}", path.display());
    let (a, b) = (
        obop_storage::open(&url).unwrap(),
        obop_storage::open(&url).unwrap(),
    );
    let steps = marking();
    std::thread::scope(|s| {
        for h in [a.clone(), b.clone()] {
            let steps = steps.clone();
            s.spawn(move || {
                Migrator::with_steps(h, ring(), steps).run(false).unwrap();
            });
        }
    });
    assert_eq!(rows(&a).len(), 3);
    assert!(
        Migrator::with_steps(b.clone(), ring(), steps)
            .verify()
            .unwrap()
            .ok()
    );
    drop((a, b));
    for ext in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
    }
    let _ = std::fs::remove_file(format!("{}.lock-obop.migrate", path.display()));
}

/// A scratch database on the server of `OBOP_TEST_POSTGRES_URL`, dropped afterwards.
struct PgScratch {
    admin: String,
    name: String,
    url: String,
}

impl PgScratch {
    fn new(base: &str) -> PgScratch {
        let name = format!("obop_migrate_t_{}", std::process::id());
        let mut c = postgres::Client::connect(base, postgres::NoTls).unwrap();
        c.batch_execute(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
            .unwrap();
        c.batch_execute(&format!("CREATE DATABASE {name}")).unwrap();
        let (head, _) = base.rsplit_once('/').unwrap();
        PgScratch {
            admin: base.to_string(),
            url: format!("{head}/{name}"),
            name,
        }
    }
}

impl Drop for PgScratch {
    fn drop(&mut self) {
        if let Ok(mut c) = postgres::Client::connect(&self.admin, postgres::NoTls) {
            let _ = c.batch_execute(&format!(
                "DROP DATABASE IF EXISTS {} WITH (FORCE)",
                self.name
            ));
        }
    }
}

/// The ledger on PostgreSQL, when a server is given: the same run, the same rows, two runners.
#[test]
fn the_ledger_is_written_in_the_step_transaction_on_postgresql_too() {
    let Ok(base) = std::env::var("OBOP_TEST_POSTGRES_URL") else {
        eprintln!("skipped: set OBOP_TEST_POSTGRES_URL to run against PostgreSQL");
        return;
    };
    let scratch = PgScratch::new(&base);
    let (a, b) = (
        obop_storage::open(&scratch.url).unwrap(),
        obop_storage::open(&scratch.url).unwrap(),
    );
    let steps = marking();
    std::thread::scope(|s| {
        for h in [a.clone(), b.clone()] {
            let steps = steps.clone();
            s.spawn(move || {
                Migrator::with_steps(h, ring(), steps).run(false).unwrap();
            });
        }
    });
    assert_eq!(rows(&a).len(), 3, "one row per step with two runners");
    assert!(rows(&a).iter().all(|r| r.status == EntryStatus::Applied));
    assert_eq!(a.counter_get("ran:three").unwrap(), 1);
    let m = Migrator::with_steps(a.clone(), ring(), marking());
    assert!(m.verify().unwrap().ok());
    // Drift is seen on PostgreSQL as anywhere else.
    let mut edited = marking();
    edited[0] = edited[0].source("fn one() { changed }");
    let e = Migrator::with_steps(b.clone(), ring(), edited)
        .check_for_serve(true, "x")
        .unwrap_err();
    assert!(e.contains("step 1"), "{e}");
    // A failing step leaves a failed row and no step row, and the database as before.
    let mut failing = marking();
    failing.push(
        Step::new(
            4,
            "four",
            Run::Ops(|_| {
                Ok(vec![
                    put_log("never"),
                    Op::Merge {
                        model: "log".into(),
                        id: "ghost".into(),
                        patch: Fields::new(),
                    },
                ])
            }),
        )
        .source("fn four() {}"),
    );
    let m = Migrator::with_steps(a.clone(), ring(), failing);
    assert!(m.run(false).unwrap_err().contains("step 4"));
    assert!(a.get("log", "never").unwrap().is_none());
    assert!(
        rows(&a)
            .iter()
            .any(|r| r.status == EntryStatus::Failed && r.id.starts_with("core/0004/failed-"))
    );
    assert!(!rows(&a).iter().any(|r| r.id == ledger::core_id(4)));
}
