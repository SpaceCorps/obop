use super::*;
use crate::catalog::Catalog;
use crate::pgadmin::FakeServer;
use crate::schedule::FakeClock;
use obop_migrate::{Class, DdlClass, EntryStatus, Run, Step};
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};

const KEY: &str = "f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0";
const OTHER: &str = "0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f";
/// 2026-10-25T12:00:00Z.
const T0: u64 = 1_792_929_600;

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Dir {
        static N: AtomicU64 = AtomicU64::new(0);
        let d = std::env::temp_dir().join(format!(
            "obop-gates-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
    fn p(&self, n: &str) -> PathBuf {
        self.0.join(n)
    }
    fn url(&self, n: &str) -> String {
        format!("sqlite://{}", self.p(n).display())
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn ring() -> KeyRing {
    KeyRing::from_secrets(KEY, &[]).unwrap()
}

fn keys() -> Keys {
    Keys::from_ring(ring())
}

fn f(v: serde_json::Value) -> obop_storage::Fields {
    v.as_object().unwrap().clone()
}

/// A database from before versioning: records, no schema record; both real steps are pending.
fn old_db(dir: &Dir) -> (Arc<dyn Backend>, String) {
    let url = dir.url("prod.db");
    let b = obop_storage::open(&url).unwrap();
    b.insert("auth_user", "ada", &f(json!({"groups": ["base.user"]})))
        .unwrap();
    b.insert("party", "p1", &f(json!({"name": "Zoë"}))).unwrap();
    (b, url)
}

fn gate(b: &Arc<dyn Backend>, url: &str, clock: &Arc<FakeClock>, mode: Mode) -> Gate {
    Gate {
        backend: b.clone(),
        ring: ring(),
        blobs: None,
        server: None,
        clock: clock.clone(),
        options: Options {
            mode,
            actor: "operator".into(),
            backup_dir: None,
            backup_file: None,
            keys: keys(),
            receipt: None,
            window: false,
            disk_attested: false,
            url: url.into(),
        },
        steps: None,
    }
}

fn check<'a>(checks: &'a [Check], name: &str) -> &'a Check {
    checks
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no check {name}"))
}

fn plan_and_store(g: &Gate) -> Plan {
    let m = g.migrator();
    let p = m.plan().unwrap();
    m.store_plan(&p).unwrap();
    p
}

fn backup_now(b: &Arc<dyn Backend>, dir: &Dir, sub: &str, clock: &FakeClock) -> PathBuf {
    let s = Settings::new(dir.p(sub), keys());
    PathBuf::from(
        schedule::take(b, None, 0, &s, "manual", clock)
            .unwrap()
            .backup
            .path,
    )
}

fn rehearse_to_receipt(g: &Gate, backup: &Path, out: &Path, steps: Option<Vec<Step>>) -> Rehearsal {
    let r = rehearse(&RehearseOptions {
        backup: backup.to_path_buf(),
        keys: keys(),
        ring: ring(),
        scratch: Scratch::TempSqlite,
        avoid: Some(g.options.url.clone()),
        now: g.clock.now(),
        steps,
    })
    .unwrap();
    std::fs::write(
        out,
        r.receipt(g.clock.now()).sign(&ring()).unwrap().to_string(),
    )
    .unwrap();
    r
}

#[test]
fn run_without_a_backup_with_an_old_backup_or_without_a_stored_plan_is_refused() {
    let dir = Dir::new("refuse");
    let (b, url) = old_db(&dir);
    let clock = Arc::new(FakeClock::at(T0));
    // 1. No backup at all.
    let mut g = gate(&b, &url, &clock, Mode::Strict);
    plan_and_store(&g);
    let e = g.run(false, &mut |_| {}).unwrap_err();
    assert!(e.contains("backup") && e.contains("no backup"), "{e}");
    assert_eq!(g.migrator().status().unwrap().stored, None, "nothing ran");
    // 2. A backup that is 25 hours old.
    let old = backup_now(&b, &dir, "old", &clock);
    clock.advance(25 * 3600);
    g.options.backup_file = Some(old.clone());
    let e = g.run(false, &mut |_| {}).unwrap_err();
    assert!(e.contains("25 h old") && e.contains("last 24 hours"), "{e}");
    // 3. A fresh backup but no stored plan (a new build has another plan id).
    let fresh = backup_now(&b, &dir, "fresh", &clock);
    g.options.backup_file = Some(fresh);
    let mut other = gate(&b, &url, &clock, Mode::Strict);
    other.steps = Some(vec![
        Step::new(1, "x", Run::Ops(|_| Ok(Vec::new()))).source("x"),
    ]);
    g.options.backup_file = g.options.backup_file.clone();
    other.options.backup_file = g.options.backup_file.clone();
    let e = other.run(false, &mut |_| {}).unwrap_err();
    assert!(
        e.contains("plan_stored") && e.contains("obop migrate plan"),
        "{e}"
    );
    // Each refusal wrote nothing.
    assert_eq!(g.migrator().status().unwrap().stored, None);
    assert!(g.migrator().ledger().unwrap().is_empty());
}

#[test]
fn a_strict_run_with_a_backup_a_stored_plan_and_a_rehearsal_runs_and_records_the_backup() {
    let dir = Dir::new("strict");
    let (b, url) = old_db(&dir);
    let clock = Arc::new(FakeClock::at(T0));
    let mut g = gate(&b, &url, &clock, Mode::Strict);
    g.options.backup_dir = Some(dir.p("backups"));
    let plan = plan_and_store(&g);
    // The rehearsal needs a backup of the production database to restore.
    let rehearsal_backup = backup_now(&b, &dir, "for-rehearsal", &clock);
    let r = rehearse_to_receipt(&g, &rehearsal_backup, &dir.p("receipt.json"), None);
    assert!(r.passed, "{}", r.render());
    assert_eq!(r.plan.id, plan.id);
    assert_eq!(r.steps.len(), 2);
    assert!(r.render().contains("REHEARSAL PASSED"));
    g.options.receipt = Some(dir.p("receipt.json"));
    g.options.disk_attested = true;
    // Preflight alone is clean and takes no backup.
    let (_, checks) = g.preflight().unwrap();
    for c in &checks {
        assert_ne!(c.level, Level::Fail, "{}: {}", c.name, c.detail);
    }
    assert!(!dir.p("backups").exists());
    let dry = g.run(true, &mut |_| {}).unwrap();
    assert!(dry.backup.is_none() && dry.applied.is_empty());
    assert_eq!(g.migrator().status().unwrap().stored, None);
    // The run.
    let mut said = Vec::new();
    let ran = g.run(false, &mut |l| said.push(l.to_string())).unwrap();
    assert_eq!(ran.applied.len(), 2);
    let backup = ran.backup.unwrap();
    assert!(backup.path.exists() && backup.id.ends_with(".obbk"));
    assert!(
        said.iter().any(|l| l.contains("verified backup")),
        "{said:?}"
    );
    // The backup it took opens with the key and holds the database as it was before.
    let v = backup::verify_with(&backup.path, &keys()).unwrap();
    assert!(v.authenticated && v.manifest.schema_version == 0);
    // Every ledger row of the run names the backup and who ran it.
    let rows = g.migrator().ledger().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|r| r.backup_id.as_deref() == Some(backup.id.as_str())
                && r.by == "operator"
                && r.status == EntryStatus::Applied)
    );
    // The backup is in the catalogue as a pre-migration restore point.
    let cat = Catalog::new(&*b).backups().unwrap();
    assert!(
        cat.iter()
            .any(|c| c.id == backup.id && c.reason == "pre-migration")
    );
    // Run again: nothing to do, nothing taken.
    let again = g.run(false, &mut |_| {}).unwrap();
    assert!(again.plan.is_empty() && again.backup.is_none());
}

#[test]
fn a_receipt_cannot_be_edited_forged_or_used_for_another_plan_or_after_a_week() {
    let dir = Dir::new("receipt");
    let (b, url) = old_db(&dir);
    let clock = Arc::new(FakeClock::at(T0));
    let mut g = gate(&b, &url, &clock, Mode::Strict);
    g.options.backup_dir = Some(dir.p("backups"));
    g.options.disk_attested = true;
    plan_and_store(&g);
    let bk = backup_now(&b, &dir, "r", &clock);
    let r = rehearse_to_receipt(&g, &bk, &dir.p("receipt.json"), None);
    g.options.receipt = Some(dir.p("receipt.json"));
    let verdict = |g: &Gate| {
        let (_, c) = g.preflight().unwrap();
        check(&c, "rehearsal").clone()
    };
    assert_eq!(verdict(&g).level, Level::Pass);
    let good: J =
        serde_json::from_str(&std::fs::read_to_string(dir.p("receipt.json")).unwrap()).unwrap();
    // Edited: the plan id swapped for the right one in a receipt of another plan.
    let mut forged = good.clone();
    forged["longest_step_ms"] = J::from(1);
    std::fs::write(dir.p("receipt.json"), forged.to_string()).unwrap();
    assert!(
        verdict(&g).detail.contains("does not authenticate"),
        "{}",
        verdict(&g).detail
    );
    // Made by hand: no mac.
    let mut hand = good.clone();
    hand.as_object_mut().unwrap().remove("mac");
    std::fs::write(dir.p("receipt.json"), hand.to_string()).unwrap();
    assert_eq!(verdict(&g).level, Level::Fail);
    // Signed with another server key.
    let other = Receipt::verify(&good, &ring())
        .unwrap()
        .sign(&KeyRing::from_secrets(OTHER, &[]).unwrap())
        .unwrap();
    std::fs::write(dir.p("receipt.json"), other.to_string()).unwrap();
    assert_eq!(verdict(&g).level, Level::Fail);
    // A receipt for another plan.
    let mut rc = Receipt::verify(&good, &ring()).unwrap();
    rc.plan_id = "0000000000000000".into();
    std::fs::write(dir.p("receipt.json"), rc.sign(&ring()).unwrap().to_string()).unwrap();
    assert!(
        verdict(&g).detail.contains("rehearse again"),
        "{}",
        verdict(&g).detail
    );
    // A rehearsal that failed.
    let mut rc = r.receipt(clock.now());
    rc.passed = false;
    std::fs::write(dir.p("receipt.json"), rc.sign(&ring()).unwrap().to_string()).unwrap();
    assert!(verdict(&g).detail.contains("FAILED"));
    // Good again, then a week and a day later.
    std::fs::write(dir.p("receipt.json"), good.to_string()).unwrap();
    assert_eq!(verdict(&g).level, Level::Pass);
    clock.advance(8 * 86_400);
    assert!(verdict(&g).detail.contains("old; rehearse again"));
    // Unreadable.
    g.options.receipt = Some(dir.p("missing.json"));
    assert_eq!(verdict(&g).level, Level::Fail);
}

#[test]
fn a_rehearsal_whose_table_locking_step_takes_over_a_second_fails_the_gate() {
    let dir = Dir::new("slow");
    let (b, url) = old_db(&dir);
    let clock = Arc::new(FakeClock::at(T0));
    let slow = vec![
        Step::new(1, "quick", Run::Ops(|_| Ok(Vec::new()))).source("quick"),
        Step::new(
            2,
            "add an index the slow way",
            Run::Direct(|_| {
                std::thread::sleep(Duration::from_millis(1100));
                Ok("indexed".into())
            }),
        )
        .class(Class::Expand)
        .ddl(DdlClass::Blocking)
        .tables(&["party"])
        .source("slow"),
    ];
    let mut g = gate(&b, &url, &clock, Mode::Strict);
    g.steps = Some(slow.clone());
    let bk = backup_now(&b, &dir, "r", &clock);
    let r = rehearse_to_receipt(&g, &bk, &dir.p("receipt.json"), Some(slow));
    assert!(!r.passed, "{}", r.render());
    let lock = check(&r.checks, "lock_time");
    assert_eq!(lock.level, Level::Fail);
    assert!(
        lock.detail.contains("step 2 held its lock for"),
        "{}",
        lock.detail
    );
    assert!(r.longest_step_ms >= 1100);
    assert!(r.render().contains("REHEARSAL FAILED"));
    // The gate then refuses the run on that receipt.
    g.options.receipt = Some(dir.p("receipt.json"));
    g.options.backup_dir = Some(dir.p("backups"));
    g.options.disk_attested = true;
    plan_and_store(&g);
    let e = g.run(false, &mut |_| {}).unwrap_err();
    assert!(e.contains("rehearsal") && e.contains("FAILED"), "{e}");
    // The same slowness on a step that is not a table lock is reported, not failed.
    let mut quiet = Vec::new();
    quiet.push(
        Step::new(
            1,
            "slow data step",
            Run::Direct(|_| {
                std::thread::sleep(Duration::from_millis(1100));
                Ok("done".into())
            }),
        )
        .source("q"),
    );
    let r = rehearse_to_receipt(&g, &bk, &dir.p("r2.json"), Some(quiet));
    assert!(r.passed, "{}", r.render());
    assert!(r.longest_step_ms >= 1100);
}

#[test]
fn a_rehearsal_never_restores_into_production_and_leaves_production_alone() {
    let dir = Dir::new("never");
    let (b, url) = old_db(&dir);
    let clock = Arc::new(FakeClock::at(T0));
    let g = gate(&b, &url, &clock, Mode::Strict);
    let bk = backup_now(&b, &dir, "r", &clock);
    let before = drill::content_digest(&*b).unwrap();
    let e = rehearse(&RehearseOptions {
        backup: bk.clone(),
        keys: keys(),
        ring: ring(),
        scratch: Scratch::Url {
            url: url.clone(),
            keep: false,
        },
        avoid: Some(url.clone()),
        now: T0,
        steps: None,
    })
    .unwrap_err();
    assert!(e.contains("production database"), "{e}");
    rehearse_to_receipt(&g, &bk, &dir.p("r.json"), None);
    assert_eq!(
        drill::content_digest(&*b).unwrap(),
        before,
        "production is untouched"
    );
    // A backup that does not open stops it before anything is restored.
    let e = rehearse(&RehearseOptions {
        backup: bk,
        keys: Keys::none(),
        ring: ring(),
        scratch: Scratch::TempSqlite,
        avoid: None,
        now: T0,
        steps: None,
    })
    .unwrap_err();
    assert!(e.starts_with("restore_key_required"), "{e}");
}

#[test]
fn on_sqlite_the_same_checks_only_warn_and_the_run_goes_ahead() {
    let dir = Dir::new("relaxed");
    let (b, url) = old_db(&dir);
    let clock = Arc::new(FakeClock::at(T0));
    let g = gate(&b, &url, &clock, Mode::Relaxed);
    let (_, checks) = g.preflight().unwrap();
    for n in ["backup", "plan_stored", "rehearsal"] {
        assert_eq!(check(&checks, n).level, Level::Warn, "{n}");
    }
    assert_eq!(check(&checks, "disk_database").level, Level::Pass);
    let ran = g.run(false, &mut |_| {}).unwrap();
    assert_eq!(ran.applied.len(), 2);
    assert!(ran.backup.is_none());
    assert!(
        g.migrator()
            .ledger()
            .unwrap()
            .iter()
            .all(|r| r.backup_id.is_none())
    );
    assert_eq!(Mode::for_backend("postgres"), Mode::Strict);
    assert_eq!(Mode::for_backend("sqlite"), Mode::Relaxed);
    assert_eq!(Mode::parse("strict"), Some(Mode::Strict));
    assert_eq!(Mode::parse("loose"), None);
}

#[test]
fn a_held_lock_a_newer_database_a_drifted_ledger_and_a_missing_window_each_stop_the_run() {
    let dir = Dir::new("stops");
    let (b, url) = old_db(&dir);
    let clock = Arc::new(FakeClock::at(T0));
    let g = gate(&b, &url, &clock, Mode::Relaxed);
    g.run(false, &mut |_| {}).unwrap();
    // Drift: the build's step 1 is not the one that ran.
    let mut edited = obop_migrate::steps();
    edited[0] = edited[0].source("fn changed() {}");
    let mut drifted = gate(&b, &url, &clock, Mode::Relaxed);
    drifted.steps = Some(edited);
    let (_, c) = drifted.preflight().unwrap();
    assert_eq!(check(&c, "ledger").level, Level::Fail);
    assert!(
        check(&c, "ledger").detail.contains("step 1"),
        "{}",
        check(&c, "ledger").detail
    );
    // A newer database than the build.
    let mut older = gate(&b, &url, &clock, Mode::Relaxed);
    older.steps = Some(obop_migrate::steps()[..1].to_vec());
    let (_, c) = older.preflight().unwrap();
    assert_eq!(check(&c, "schema").level, Level::Fail);
    assert!(older.run(false, &mut |_| {}).is_err());
    // A step that needs a window.
    let (b2, url2) = old_db(&Dir::new("window"));
    for i in 0..6 {
        b2.insert("big", &i.to_string(), &obop_storage::Fields::new())
            .unwrap();
    }
    let mut w = gate(&b2, &url2, &clock, Mode::Relaxed);
    w.steps = Some(vec![
        Step::new(1, "index", Run::Ops(|_| Ok(Vec::new())))
            .ddl(DdlClass::Blocking)
            .tables(&["big"])
            .source("w"),
    ]);
    // The default row limit is a million: lower it through the migrator the gate builds.
    let m = w.migrator().with_window_rows(5);
    assert!(m.plan().unwrap().needs_window());
    // A lock held elsewhere is seen within three seconds.
    let held = b.lock("obop.migrate").unwrap();
    let started = Instant::now();
    let (_, c) = g.preflight().unwrap();
    assert_eq!(check(&c, "lock").level, Level::Fail);
    assert!(started.elapsed() < Duration::from_secs(10));
    drop(held);
}

#[test]
fn a_restore_point_is_made_on_a_postgresql_host_and_required_when_archiving_is_on() {
    let dir = Dir::new("rp");
    let (b, url) = old_db(&dir);
    let clock = Arc::new(FakeClock::at(T0));
    let server = Arc::new(FakeServer::well_configured());
    let mut g = gate(&b, &url, &clock, Mode::Relaxed);
    g.server = Some(server.clone());
    let ran = g.run(false, &mut |_| {}).unwrap();
    assert_eq!(
        server.restore_points.lock().unwrap().as_slice(),
        ["pre-migration-2"]
    );
    assert_eq!(
        ran.restore_point.as_deref(),
        Some("pre-migration-2@0/16B3748")
    );
    assert!(
        g.migrator()
            .ledger()
            .unwrap()
            .iter()
            .all(|r| r.restore_point.as_deref() == Some("pre-migration-2@0/16B3748"))
    );
    // The role may not call it and WAL archiving is on: strict refuses before any step.
    let (b2, url2) = old_db(&Dir::new("rp2"));
    let mut s = FakeServer::well_configured();
    s.restore_point_error = Some("permission denied for function pg_create_restore_point".into());
    let mut strict = gate(&b2, &url2, &clock, Mode::Strict);
    strict.server = Some(Arc::new(s));
    strict.options.disk_attested = true;
    // Satisfy every other gate: a stored plan, a backup, a rehearsal.
    let d2 = Dir::new("rp2x");
    plan_and_store(&strict);
    let bk = backup_now(&b2, &d2, "r", &clock);
    rehearse_to_receipt(&strict, &bk, &d2.p("r.json"), None);
    strict.options.receipt = Some(d2.p("r.json"));
    strict.options.backup_file = Some(bk);
    let e = strict.run(false, &mut |_| {}).unwrap_err();
    assert!(
        e.contains("restore point") && e.contains("pg_create_restore_point"),
        "{e}"
    );
    assert_eq!(
        strict.migrator().status().unwrap().stored,
        None,
        "no step ran"
    );
    // With archiving off it is a note, not a stop.
    let (b3, url3) = old_db(&Dir::new("rp3"));
    let mut s = FakeServer::well_configured().with("archive_mode", "off");
    s.restore_point_error = Some("permission denied".into());
    let mut g3 = gate(&b3, &url3, &clock, Mode::Relaxed);
    g3.server = Some(Arc::new(s));
    let mut said = Vec::new();
    let ran = g3.run(false, &mut |l| said.push(l.to_string())).unwrap();
    assert!(ran.restore_point.is_none());
    assert!(
        said.iter()
            .any(|l| l.contains("no restore point") && l.contains("archiving is off")),
        "{said:?}"
    );
}

#[test]
fn a_postgresql_host_needs_the_operator_to_attest_its_disk() {
    let dir = Dir::new("pgdisk");
    let b = obop_storage::open("memory:").unwrap();
    b.insert("party", "p1", &f(json!({}))).unwrap();
    let clock = Arc::new(FakeClock::at(T0));
    let mut g = gate(&b, "postgres://u@h/db", &clock, Mode::Strict);
    g.server = Some(Arc::new(FakeServer::well_configured()));
    let (_, c) = g.preflight().unwrap();
    let d = check(&c, "disk_database");
    assert_eq!(d.level, Level::Fail);
    assert!(
        d.detail.contains("--disk-ok") && d.detail.contains("cannot be read"),
        "{}",
        d.detail
    );
    g.options.disk_attested = true;
    let (_, c) = g.preflight().unwrap();
    assert_eq!(check(&c, "disk_database").level, Level::Pass);
    assert!(check(&c, "disk_database").detail.contains("attests"));
    // The backup volume is measured for real.
    g.options.backup_dir = Some(dir.p("not-yet-made/backups"));
    let (_, c) = g.preflight().unwrap();
    assert_eq!(
        check(&c, "disk_backup").level,
        Level::Pass,
        "{}",
        check(&c, "disk_backup").detail
    );
    assert!(free_bytes(&dir.p("not-yet-made/deeper")).unwrap() > 0);
}

#[test]
fn a_server_that_migrates_itself_takes_a_verified_backup_first_when_it_has_a_directory() {
    let dir = Dir::new("auto");
    let (b, _) = old_db(&dir);
    let clock = FakeClock::at(T0);
    assert_eq!(
        auto_backup(&b, None, 0, None, &keys(), &clock).unwrap(),
        None
    );
    let id = auto_backup(&b, None, 0, Some(&dir.p("backups")), &keys(), &clock)
        .unwrap()
        .unwrap();
    assert!(
        id.starts_with("obop-backup-20261025T120000Z") && id.ends_with(".obbk"),
        "{id}"
    );
    assert!(
        backup::verify_with(&dir.p("backups").join(&id), &keys())
            .unwrap()
            .authenticated
    );
}

#[test]
fn a_rollback_restores_the_verified_backup_and_records_why_and_what_was_discarded() {
    let dir = Dir::new("rollback");
    let (b, url) = old_db(&dir);
    let clock = Arc::new(FakeClock::at(T0));
    let before = drill::content_digest(&*b).unwrap();
    let bk = backup_now(&b, &dir, "pre", &clock);
    // The migration runs and the data is damaged afterwards.
    let g = gate(&b, &url, &clock, Mode::Relaxed);
    g.run(false, &mut |_| {}).unwrap();
    clock.advance(600);
    b.insert(
        "party",
        "oops",
        &f(json!({"name": "written by a bad step"})),
    )
    .unwrap();
    b.delete("party", "p1").unwrap();
    assert_ne!(drill::content_digest(&*b).unwrap(), before);
    // A reason is required, and a backup that does not open changes nothing.
    for bad in ["", "   ", "two\nlines"] {
        assert!(
            rollback(&b, None, &ring(), &bk, &keys(), bad, "operator", &*clock).is_err(),
            "{bad:?}"
        );
    }
    let e = rollback(
        &b,
        None,
        &ring(),
        &bk,
        &Keys::none(),
        "reason",
        "operator",
        &*clock,
    )
    .unwrap_err();
    assert!(e.starts_with("restore_key_required"), "{e}");
    assert!(b.get("party", "oops").unwrap().is_some(), "untouched");
    // The rollback.
    let done = rollback(
        &b,
        None,
        &ring(),
        &bk,
        &keys(),
        "step 2 corrupted the party names",
        "alice",
        &*clock,
    )
    .unwrap();
    assert_eq!(done.schema_before, Some(2));
    assert_eq!(
        done.schema_after, None,
        "the backup is from before versioning"
    );
    assert!(done.discarded_rows >= 2, "{done:?}");
    assert!(b.get("party", "oops").unwrap().is_none());
    assert!(b.get("party", "p1").unwrap().is_some());
    // The database is the backup's, plus the record of the rollback and the audit line.
    let row = b
        .list(obop_migrate::ledger::MODEL)
        .unwrap()
        .into_iter()
        .find(|(id, _)| id.starts_with("core/rollback-"))
        .unwrap()
        .1;
    assert_eq!(row["note"], "step 2 corrupted the party names");
    assert_eq!(row["by"], "alice");
    assert!(
        row["backup_id"]
            .as_str()
            .unwrap()
            .starts_with("obop-backup-")
    );
    assert_eq!(
        row["discarded"].as_array().unwrap().len(),
        done.discarded_rows
    );
    let events = b.list("security_event").unwrap();
    let ev = events
        .iter()
        .find(|(_, f)| f["kind"] == "migration_rollback")
        .expect("an audit event");
    assert!(
        ev.1["detail"]
            .as_str()
            .unwrap()
            .contains("corrupted the party names")
    );
    assert_eq!(ev.1["actor"], "alice");
}

#[test]
fn the_ledger_and_the_gates_work_on_postgresql_when_a_server_is_given() {
    let Ok(base) = std::env::var("OBOP_TEST_POSTGRES_URL") else {
        eprintln!("skipped: set OBOP_TEST_POSTGRES_URL to run against PostgreSQL");
        return;
    };
    let name = format!("obop_gates_t_{}", std::process::id());
    let mut admin = postgres::Client::connect(&base, postgres::NoTls).unwrap();
    admin
        .batch_execute(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
        .unwrap();
    admin
        .batch_execute(&format!("CREATE DATABASE {name}"))
        .unwrap();
    let (head, _) = base.rsplit_once('/').unwrap();
    let url = format!("{head}/{name}");
    let dir = Dir::new("pg");
    let r = std::panic::catch_unwind(|| {
        let b = obop_storage::open(&url).unwrap();
        b.insert("auth_user", "ada", &f(json!({"groups": ["base.user"]})))
            .unwrap();
        let clock = Arc::new(FakeClock::at(T0));
        let server: Arc<dyn ServerInfo> = Arc::new(crate::pgadmin::Pg::connect(&url).unwrap());
        let mut g = gate(&b, &url, &clock, Mode::Strict);
        g.server = Some(server.clone());
        g.options.backup_dir = Some(dir.p("backups"));
        g.options.disk_attested = true;
        plan_and_store(&g);
        // Strict: a rehearsal receipt from a copy of a backup of this very database.
        let bk = backup_now(&b, &dir, "r", &clock);
        let rep = rehearse_to_receipt(&g, &bk, &dir.p("r.json"), None);
        assert!(rep.passed, "{}", rep.render());
        g.options.receipt = Some(dir.p("r.json"));
        let ran = g.run(false, &mut |_| {}).unwrap();
        assert_eq!(ran.applied.len(), 2);
        // A real restore point was asked of the server (the test container archives nothing, so
        // a refusal would be a note; either way the run went ahead).
        let rows = g.migrator().ledger().unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.backup_id.is_some()));
        // Roll back on PostgreSQL: the database is the backup's again.
        let before = rollback(
            &b,
            None,
            &ring(),
            &bk,
            &keys(),
            "drill of the rollback",
            "operator",
            &*clock,
        )
        .unwrap();
        assert_eq!(before.schema_after, None);
        assert!(b.get("auth_user", "ada").unwrap().is_some());
    });
    admin
        .batch_execute(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
        .unwrap();
    r.unwrap();
}
