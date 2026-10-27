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
            lock_wait: Duration::from_secs(5),
            window_rows: None,
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
    assert!(dry.backup.is_none() && dry.would_refuse.is_empty());
    assert_eq!(
        dry.applied.len(),
        2,
        "a dry run says what each step would do"
    );
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
    // A value that differs from the measured one whatever it was (a fixed 1 equals it when the
    // longest step really took 1 ms, and the "forged" receipt would then authenticate).
    forged["longest_step_ms"] = J::from(good["longest_step_ms"].as_u64().unwrap() + 1);
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
fn a_held_lock_a_newer_database_and_a_drifted_ledger_each_stop_the_run() {
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
    // A lock held elsewhere is seen within the wait the gate was given, not after the full 60 s
    // a migration would wait: the preflight is a question, not a queue. The wait is injected, so
    // a loaded machine cannot make the test flaky.
    // (The migrated database above has nothing pending, which asks nothing of the lock: use one
    // with steps to run.)
    // Bound to a name: a temporary Dir is dropped, and its directory removed, at the end of the
    // statement; an open file keeps the directory alive on Windows only.
    let held_dir = Dir::new("lockheld");
    let (b3, url3) = old_db(&held_dir);
    let mut g3 = gate(&b3, &url3, &clock, Mode::Relaxed);
    g3.options.lock_wait = Duration::from_millis(200);
    let held = b3.lock("obop.migrate").unwrap();
    let (_, c3) = g3.preflight().unwrap();
    assert_eq!(check(&c3, "lock").level, Level::Fail, "{c3:?}");
    assert!(
        g3.run(false, &mut |_| {}).is_err(),
        "a held lock stops the run"
    );
    drop(held);
    g3.options.lock_wait = Duration::from_secs(5);
    assert_eq!(check(&g3.preflight().unwrap().1, "lock").level, Level::Pass);
}

/// A long-lock step on a table above the limit stops the run through the gate itself (the check
/// and the `--window` plumbing), and the named window lets it through.
#[test]
fn a_missing_window_stops_the_run_through_the_gate_and_a_named_window_lets_it_through() {
    let dir = Dir::new("window");
    let (b, url) = old_db(&dir);
    for i in 0..6 {
        b.insert("big", &i.to_string(), &obop_storage::Fields::new())
            .unwrap();
    }
    let clock = Arc::new(FakeClock::at(T0));
    let mut g = gate(&b, &url, &clock, Mode::Relaxed);
    g.steps = Some(vec![
        Step::new(1, "index", Run::Ops(|_| Ok(Vec::new())))
            .ddl(DdlClass::Blocking)
            .tables(&["big"])
            .source("w"),
    ]);
    g.options.window_rows = Some(5);
    let (_, c) = g.preflight().unwrap();
    assert_eq!(check(&c, "window").level, Level::Fail);
    assert!(check(&c, "window").detail.contains("--window"));
    let e = g.run(false, &mut |_| {}).unwrap_err();
    assert!(e.contains("window"), "{e}");
    assert_eq!(g.migrator().status().unwrap().stored, None, "nothing ran");
    g.options.window = true;
    let (_, c) = g.preflight().unwrap();
    assert_eq!(check(&c, "window").level, Level::Pass);
    let ran = g.run(false, &mut |_| {}).unwrap();
    assert_eq!(ran.applied.len(), 1);
    // Below the limit no window is asked for.
    let small_dir = Dir::new("window-small");
    let (b2, url2) = old_db(&small_dir);
    let mut small = gate(&b2, &url2, &clock, Mode::Relaxed);
    small.steps = g.steps.clone();
    small.options.window_rows = Some(5);
    assert_eq!(
        check(&small.preflight().unwrap().1, "window").level,
        Level::Pass
    );
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
    let rp2_dir = Dir::new("rp2");
    let (b2, url2) = old_db(&rp2_dir);
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
    let rp3_dir = Dir::new("rp3");
    let (b3, url3) = old_db(&rp3_dir);
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
fn a_setting_the_server_would_not_give_is_not_archiving_off_and_a_size_not_given_is_not_zero() {
    let clock = Arc::new(FakeClock::at(T0));
    // The role may not make the restore point, and the server cannot say whether it archives: the
    // strict gate keeps the recovery point part of the run (it went on with "archiving is off").
    let dir = Dir::new("rpx");
    let (b, url) = old_db(&dir);
    let mut s = FakeServer::well_configured();
    s.setting_error = Some("the connection was lost".into());
    s.restore_point_error = Some("permission denied".into());
    let mut strict = gate(&b, &url, &clock, Mode::Strict);
    strict.server = Some(Arc::new(s));
    strict.options.disk_attested = true;
    let d2 = Dir::new("rpxx");
    plan_and_store(&strict);
    let bk = backup_now(&b, &d2, "r", &clock);
    rehearse_to_receipt(&strict, &bk, &d2.p("r.json"), None);
    strict.options.receipt = Some(d2.p("r.json"));
    strict.options.backup_file = Some(bk);
    let e = strict.run(false, &mut |_| {}).unwrap_err();
    assert!(e.contains("restore point") && e.contains("unknown"), "{e}");
    assert_eq!(strict.migrator().status().unwrap().stored, None);
    // The relaxed gate goes on and says that it does not know.
    let dir3 = Dir::new("rpy");
    let (b3, url3) = old_db(&dir3);
    let mut s = FakeServer::well_configured();
    s.setting_error = Some("the connection was lost".into());
    s.restore_point_error = Some("permission denied".into());
    let mut relaxed = gate(&b3, &url3, &clock, Mode::Relaxed);
    relaxed.server = Some(Arc::new(s));
    let mut said = Vec::new();
    relaxed
        .run(false, &mut |l| said.push(l.to_string()))
        .unwrap();
    assert!(
        said.iter()
            .any(|l| l.contains("no restore point") && l.contains("unknown")),
        "{said:?}"
    );
    // The size of the database is not known: the check says so and does not pass on "needs 0.0".
    let dir4 = Dir::new("sizeless");
    let (b4, _) = old_db(&dir4);
    let backups = Dir::new("sizeless-bk");
    let mut s = FakeServer::well_configured();
    s.bytes_error = Some("the connection was lost".into());
    let mut g = gate(&b4, "postgres://u@h/db", &clock, Mode::Strict);
    g.server = Some(Arc::new(s));
    g.options.backup_dir = Some(backups.0.clone());
    let (_, c) = g.preflight().unwrap();
    let d = check(&c, "disk_backup");
    assert_eq!(d.level, Level::Warn, "{d:?}");
    assert!(d.detail.contains("could not be read"), "{}", d.detail);
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
    let clock: Arc<dyn Clock> = Arc::new(FakeClock::at(T0));
    let said = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let say = {
        let said = said.clone();
        Arc::new(move |l: &str| said.lock().unwrap().push(l.to_string()))
            as Arc<dyn Fn(&str) + Send + Sync>
    };
    let pending = obop_migrate::Pending {
        stored: 0,
        steps: vec![(1, "adopt")],
    };
    let hook = |dir: Option<PathBuf>, need: Option<u64>| {
        auto_backup_hook(
            b.clone(),
            None,
            AutoBackup {
                dir,
                keys: keys(),
                need_bytes: need,
            },
            clock.clone(),
            say.clone(),
        )
    };
    // No directory: no backup, and the caller is told so.
    assert_eq!(hook(None, None)(&pending).unwrap(), None);
    assert!(
        said.lock()
            .unwrap()
            .iter()
            .any(|l| l.contains("without a backup"))
    );
    let id = hook(Some(dir.p("backups")), None)(&pending)
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
    // A volume with no room for it stops the migration before anything is written.
    let e = hook(Some(dir.p("backups2")), Some(u64::MAX / 4))(&pending).unwrap_err();
    assert!(
        e.contains("free") && e.contains("nothing was migrated"),
        "{e}"
    );
    assert!(
        !dir.p("backups2").exists() || std::fs::read_dir(dir.p("backups2")).unwrap().count() == 0
    );
}

/// A server that cannot migrate must not write a full backup at every start: a refused start
/// takes none, and a failing step is retried on the backup the first attempt took.
#[test]
fn a_crash_loop_takes_one_backup_not_one_per_start() {
    let dir = Dir::new("loop");
    let (b, _) = old_db(&dir);
    let clock: Arc<dyn Clock> = Arc::new(FakeClock::at(T0));
    let mut failing = obop_migrate::steps();
    failing[1] = Step::new(
        2,
        "always fails",
        Run::Ops(|_| Err("a sealed value cannot be opened".into())),
    )
    .source("fail");
    let backups = dir.p("backups");
    let start = |steps: Vec<Step>| {
        let m = Migrator::with_steps(b.clone(), ring(), steps).by("server");
        let say: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(|_| {});
        m.check_for_serve_with(
            true,
            "x",
            &RunOptions {
                before: Some(auto_backup_hook(
                    b.clone(),
                    None,
                    AutoBackup {
                        dir: Some(backups.clone()),
                        keys: keys(),
                        need_bytes: None,
                    },
                    clock.clone(),
                    say,
                )),
                ..RunOptions::default()
            },
        )
    };
    let n_backups = || {
        std::fs::read_dir(&backups)
            .map(|d| {
                d.flatten()
                    .filter(|e| e.file_name().to_string_lossy().ends_with(".obbk"))
                    .count()
            })
            .unwrap_or(0)
    };
    for attempt in 1..=5 {
        let e = start(failing.clone()).unwrap_err();
        assert!(e.contains("step 2"), "{e}");
        assert_eq!(n_backups(), 1, "attempt {attempt}: one backup, reused");
    }
    // A start that is refused before any step (drift) takes none at all.
    let mut drifted = obop_migrate::steps();
    drifted[0] = drifted[0].source("fn changed() {}");
    let e = start(drifted).unwrap_err();
    assert!(e.contains("step 1"), "{e}");
    assert_eq!(n_backups(), 1);
    // The fault is fixed: it migrates on that same backup, and the ledger names it.
    start(obop_migrate::steps()).unwrap();
    assert_eq!(n_backups(), 1);
    let ledger = Migrator::new(b.clone(), ring()).ledger().unwrap();
    let id = Catalog::new(&*b).backups().unwrap().remove(0).id;
    assert!(
        ledger
            .iter()
            .filter(|r| r.status == EntryStatus::Applied)
            .all(|r| r.backup_id.as_deref() == Some(id.as_str())),
        "{ledger:?}"
    );
    // Up to date: starting again takes nothing.
    start(obop_migrate::steps()).unwrap();
    assert_eq!(n_backups(), 1);
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
            rollback(
                &b,
                None,
                &ring(),
                &bk,
                &keys(),
                bad,
                "operator",
                false,
                &*clock
            )
            .is_err(),
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
        false,
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
        false,
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

// ---- review of D73 -------------------------------------------------------------------------

/// A PostgreSQL-shaped, strict gate over a database that is up to date.
fn strict_up_to_date() -> (Gate, Arc<FakeClock>) {
    let b = obop_storage::open("memory:").unwrap();
    b.insert("party", "p1", &f(json!({}))).unwrap();
    let clock = Arc::new(FakeClock::at(T0));
    let mut g = gate(&b, "postgres://u@h/db", &clock, Mode::Strict);
    g.server = Some(Arc::new(FakeServer::well_configured()));
    g.run(false, &mut |_| {}).unwrap_err(); // refused: nothing it needs was given
    g.options.mode = Mode::Relaxed;
    g.run(false, &mut |_| {}).unwrap();
    g.options.mode = Mode::Strict;
    (g, clock)
}

#[test]
fn a_strict_run_with_nothing_pending_is_a_successful_no_op_that_asks_for_nothing() {
    let (g, _) = strict_up_to_date();
    assert!(g.migrator().status().unwrap().pending.is_empty());
    // No backup, no stored plan, no receipt, no --disk-ok, no backup directory.
    for dry in [false, true] {
        let ran = g.run(dry, &mut |_| {}).unwrap();
        assert!(ran.plan.is_empty() && ran.backup.is_none() && ran.applied.is_empty());
        assert!(ran.would_refuse.is_empty());
        let names: Vec<&str> = ran.checks.iter().map(|c| c.name).collect();
        assert_eq!(
            names,
            ["schema", "ledger"],
            "only what concerns the database"
        );
    }
    // It still refuses a database that is newer than this build, and a drifted ledger.
    let mut older = Gate {
        steps: Some(obop_migrate::steps()[..1].to_vec()),
        ..gate(
            &g.backend,
            "postgres://u@h/db",
            &Arc::new(FakeClock::at(T0)),
            Mode::Strict,
        )
    };
    older.server = Some(Arc::new(FakeServer::well_configured()));
    assert!(older.run(false, &mut |_| {}).is_err());
}

#[test]
fn a_dry_run_is_a_preview_that_lists_what_would_refuse_and_what_each_step_would_do() {
    let dir = Dir::new("dry");
    let (b, url) = old_db(&dir);
    let clock = Arc::new(FakeClock::at(T0));
    let g = gate(&b, &url, &clock, Mode::Strict);
    let ran = g.run(true, &mut |_| {}).unwrap();
    assert!(ran.dry_run && ran.backup.is_none());
    let refused: String = ran.would_refuse.join("\n");
    for n in ["plan_stored", "backup", "rehearsal"] {
        assert!(refused.contains(n), "{n}: {refused}");
    }
    assert_eq!(
        ran.applied.len(),
        2,
        "what each step would do: {:?}",
        ran.applied
    );
    assert!(ran.applied.iter().all(|(_, _, what)| !what.is_empty()));
    // Nothing was written: no version, no ledger row, no plan, no name.
    assert_eq!(g.migrator().status().unwrap().stored, None);
    assert!(g.migrator().ledger().unwrap().is_empty());
    assert!(g.migrator().instance_id().unwrap().is_none());
    assert_eq!(b.count(obop_migrate::ledger::PLAN_MODEL).unwrap(), 0);
}

#[test]
fn a_backup_and_a_receipt_of_another_database_do_not_satisfy_the_gate() {
    let dir = Dir::new("identity");
    // This database, named; another one, named differently, with different records.
    let (b, url) = old_db(&dir);
    let other = obop_storage::open(&dir.url("other.db")).unwrap();
    other
        .insert("party", "x1", &f(json!({"name": "Else"})))
        .unwrap();
    for db in [&b, &other] {
        Migrator::new(db.clone(), ring())
            .ensure_instance_id()
            .unwrap();
    }
    let clock = Arc::new(FakeClock::at(T0));
    let theirs = backup_now(&other, &dir, "theirs", &clock);
    let mine = backup_now(&b, &dir, "mine", &clock);
    let mut g = gate(&b, &url, &clock, Mode::Strict);
    g.options.disk_attested = true;
    g.options.backup_dir = Some(dir.p("backups"));
    plan_and_store(&g);
    // (a) The backup of the other database.
    g.options.backup_file = Some(theirs.clone());
    let (_, c) = g.preflight().unwrap();
    assert_eq!(check(&c, "backup").level, Level::Fail);
    assert!(
        check(&c, "backup").detail.contains("another database"),
        "{}",
        check(&c, "backup").detail
    );
    // (b) A rehearsal receipt made from the backup of the other database.
    let rr = rehearse_to_receipt(&g, &theirs, &dir.p("r-theirs.json"), None);
    assert!(rr.passed);
    g.options.backup_file = Some(mine.clone());
    g.options.receipt = Some(dir.p("r-theirs.json"));
    let (_, c) = g.preflight().unwrap();
    assert_eq!(check(&c, "backup").level, Level::Pass);
    assert_eq!(check(&c, "rehearsal").level, Level::Fail);
    assert!(
        check(&c, "rehearsal")
            .detail
            .contains("not of this database"),
        "{}",
        check(&c, "rehearsal").detail
    );
    let e = g.run(false, &mut |_| {}).unwrap_err();
    assert!(e.contains("rehearsal"), "{e}");
    assert_eq!(g.migrator().status().unwrap().stored, None, "nothing ran");
    // With the right backup and the right receipt it runs.
    rehearse_to_receipt(&g, &mine, &dir.p("r-mine.json"), None);
    g.options.receipt = Some(dir.p("r-mine.json"));
    let ran = g.run(false, &mut |_| {}).unwrap();
    assert_eq!(ran.applied.len(), 2);
    // A receipt says what it rehearsed on.
    let j: J =
        serde_json::from_str(&std::fs::read_to_string(dir.p("r-mine.json")).unwrap()).unwrap();
    assert_eq!(j["engine"], "sqlite");
    assert!(j["records"].as_u64().unwrap() >= 2);
    assert_eq!(j["database_id"].as_str().unwrap().len(), 32);
}

#[test]
fn a_backup_from_before_databases_had_names_cannot_be_tied_to_one_and_is_refused_in_strict_mode() {
    let dir = Dir::new("unnamed");
    let (b, url) = old_db(&dir);
    let clock = Arc::new(FakeClock::at(T0));
    // Taken before the database had an id: the manifest names none.
    let old = backup_now(&b, &dir, "old", &clock);
    Migrator::new(b.clone(), ring())
        .ensure_instance_id()
        .unwrap();
    let mut g = gate(&b, &url, &clock, Mode::Strict);
    g.options.backup_file = Some(old);
    let (_, c) = g.preflight().unwrap();
    assert_eq!(check(&c, "backup").level, Level::Fail);
    assert!(
        check(&c, "backup").detail.contains("no database id"),
        "{}",
        check(&c, "backup").detail
    );
    // Relaxed (development) mode only warns.
    g.options.mode = Mode::Relaxed;
    assert_eq!(
        check(&g.preflight().unwrap().1, "backup").level,
        Level::Warn
    );
}

#[test]
fn a_rehearsal_on_another_engine_than_production_is_said_and_the_gate_warns() {
    let dir = Dir::new("engine");
    let (b, url) = old_db(&dir);
    let clock = Arc::new(FakeClock::at(T0));
    Migrator::new(b.clone(), ring())
        .ensure_instance_id()
        .unwrap();
    let bk = backup_now(&b, &dir, "r", &clock);
    let mut g = gate(&b, &url, &clock, Mode::Relaxed);
    plan_and_store(&g);
    let r = rehearse_to_receipt(&g, &bk, &dir.p("r.json"), None);
    assert_eq!(r.engine, "sqlite");
    assert!(r.render().contains("in sqlite"));
    g.options.receipt = Some(dir.p("r.json"));
    let (_, c) = g.preflight().unwrap();
    assert!(
        c.iter().all(|x| x.name != "rehearsal_engine"),
        "same engine: nothing to say"
    );
    // The same receipt offered to a production of another kind.
    let pg_like = obop_storage::open("memory:").unwrap();
    let id = Migrator::new(b.clone(), ring())
        .instance_id()
        .unwrap()
        .unwrap();
    pg_like
        .insert(
            obop_migrate::META_MODEL,
            obop_migrate::INSTANCE_ID,
            &f(json!({"id": id})),
        )
        .unwrap();
    let mut p = gate(&pg_like, "memory:", &clock, Mode::Relaxed);
    p.options.receipt = Some(dir.p("r.json"));
    // The memory backend is "memory", the receipt says "sqlite".
    let plan = p.migrator().plan().unwrap();
    assert_eq!(
        plan.id, r.plan.id,
        "same pending steps, so the plan id is the same"
    );
    let (_, c) = p.preflight().unwrap();
    assert_eq!(check(&c, "rehearsal_engine").level, Level::Warn);
    assert!(
        check(&c, "rehearsal_engine").detail.contains("sqlite")
            && check(&c, "rehearsal_engine").detail.contains("memory")
    );
}

#[test]
fn a_postgresql_gate_does_not_fail_open_when_the_server_cannot_be_asked() {
    let b = obop_storage::open("memory:").unwrap();
    b.insert("party", "p1", &f(json!({}))).unwrap();
    let clock = Arc::new(FakeClock::at(T0));
    let g = gate(&b, "postgres://u@h/db", &clock, Mode::Strict);
    let (_, c) = g.preflight().unwrap();
    assert_eq!(check(&c, "server").level, Level::Fail);
    assert!(check(&c, "server").detail.contains("could not be asked"));
    // The attestation of the disk is still asked for, whether or not the server answers.
    assert_eq!(check(&c, "disk_database").level, Level::Fail);
    assert!(
        check(&c, "disk_database")
            .detail
            .contains("could not be asked about itself")
    );
    // Relaxed: a warning.
    let mut r = gate(&b, "postgres://u@h/db", &clock, Mode::Relaxed);
    r.options.disk_attested = true;
    assert_eq!(
        check(&r.preflight().unwrap().1, "server").level,
        Level::Warn
    );
}

#[test]
fn a_backup_directory_with_too_little_room_blocks_a_strict_run_and_only_warns_a_relaxed_one() {
    let dir = Dir::new("room");
    let (b, url) = old_db(&dir);
    let clock = Arc::new(FakeClock::at(T0));
    // A database file far larger than the volume can hold twice is not made here; the check
    // compares free space with twice the size, so a sparse file of 1 EiB is enough.
    let big = std::fs::File::create(dir.p("prod.db.big")).unwrap();
    drop(big);
    let mut g = gate(&b, &url, &clock, Mode::Strict);
    g.options.backup_dir = Some(dir.p("backups"));
    // Pretend the database is gigantic through the estimate: a PostgreSQL-like server.
    let mut pg = gate(&b, "postgres://u@h/db", &clock, Mode::Strict);
    let mut server = FakeServer::well_configured();
    server.bytes = u64::MAX / 4;
    pg.server = Some(Arc::new(server));
    pg.options.backup_dir = Some(dir.p("backups"));
    pg.options.disk_attested = true;
    let (_, c) = pg.preflight().unwrap();
    assert_eq!(check(&c, "disk_backup").level, Level::Fail);
    pg.options.mode = Mode::Relaxed;
    let (_, c) = pg.preflight().unwrap();
    assert_eq!(
        check(&c, "disk_backup").level,
        Level::Warn,
        "{}",
        check(&c, "disk_backup").detail
    );
    // Not asked at all when a backup file is given instead.
    pg.options.backup_file = Some(dir.p("whatever"));
    let (_, c) = pg.preflight().unwrap();
    assert!(c.iter().all(|x| x.name != "disk_backup"));
    drop(g);
}

#[test]
fn a_rollback_is_one_transaction_with_its_record_and_is_refused_for_another_database() {
    let dir = Dir::new("rb2");
    let (b, url) = old_db(&dir);
    let clock = Arc::new(FakeClock::at(T0));
    Migrator::new(b.clone(), ring())
        .ensure_instance_id()
        .unwrap();
    gate(&b, &url, &clock, Mode::Relaxed)
        .run(false, &mut |_| {})
        .unwrap();
    b.insert("party", "oops", &f(json!({"name": "bad"})))
        .unwrap();
    // A backup of another database is refused, whatever the flags of the CLI, and changes nothing.
    let other = obop_storage::open(&dir.url("other.db")).unwrap();
    other.insert("party", "x1", &f(json!({}))).unwrap();
    Migrator::new(other.clone(), ring())
        .ensure_instance_id()
        .unwrap();
    let theirs = backup_now(&other, &dir, "theirs", &clock);
    let e = rollback(
        &b,
        None,
        &ring(),
        &theirs,
        &keys(),
        "wrong file",
        "operator",
        false,
        &*clock,
    )
    .unwrap_err();
    assert!(
        e.contains("another database") && e.contains("--other-database"),
        "{e}"
    );
    assert!(b.get("party", "oops").unwrap().is_some(), "untouched");
    // On purpose, with the flag, it goes ahead.
    let done = rollback(
        &b,
        None,
        &ring(),
        &theirs,
        &keys(),
        "replace by a copy",
        "operator",
        true,
        &*clock,
    )
    .unwrap();
    assert!(done.authenticated);
    assert!(b.get("party", "x1").unwrap().is_some());
    // The restore and its two records are one transaction: a failure after the import commits
    // cannot happen between them, because there is no "after". If the restore fails, neither
    // the ledger row nor the security event is there.
    let flaky = crate::testkit::Flaky::new(obop_storage::open("memory:").unwrap());
    let fb: Arc<dyn Backend> = flaky.clone();
    fb.insert("party", "p1", &f(json!({}))).unwrap();
    let bkm = backup_now(&fb, &dir, "m", &clock);
    fb.insert("party", "p2", &f(json!({}))).unwrap();
    let e = rollback(
        &fb,
        None,
        &ring(),
        &bkm,
        &Keys::none(),
        "no key",
        "operator",
        false,
        &*clock,
    )
    .unwrap_err();
    assert!(e.starts_with("restore_key_required"), "{e}");
    assert!(fb.list("security_event").unwrap().is_empty());
    assert!(
        fb.list(obop_migrate::ledger::MODEL)
            .unwrap()
            .iter()
            .all(|(id, _)| !id.starts_with("core/rollback-"))
    );
    // And the migration lock is held while it runs: another taker cannot interleave.
    let held = fb.lock("obop.migrate").unwrap();
    let fb2 = fb.clone();
    let (bkm2, clock2) = (bkm.clone(), clock.clone());
    let t = std::thread::spawn(move || {
        rollback(
            &fb2,
            None,
            &ring(),
            &bkm2,
            &keys(),
            "waits",
            "operator",
            false,
            &*clock2,
        )
    });
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !t.is_finished(),
        "the rollback waits for the migration lock"
    );
    drop(held);
    t.join().unwrap().unwrap();
    assert!(fb.get("party", "p2").unwrap().is_none());
    assert_eq!(fb.list("security_event").unwrap().len(), 1);
}

#[test]
fn a_rollback_does_not_skip_the_identity_check_when_the_id_of_this_database_cannot_be_read() {
    // The id of this database is read to see that the backup is of this database. A refused read
    // was taken for "this database has no id", the comparison was skipped, and the database was
    // replaced by another one's backup.
    let dir = Dir::new("rb3");
    let (b, _url) = old_db(&dir);
    let clock = Arc::new(FakeClock::at(T0));
    Migrator::new(b.clone(), ring())
        .ensure_instance_id()
        .unwrap();
    let other = obop_storage::open(&dir.url("other3.db")).unwrap();
    other.insert("party", "x1", &f(json!({}))).unwrap();
    Migrator::new(other.clone(), ring())
        .ensure_instance_id()
        .unwrap();
    let theirs = backup_now(&other, &dir, "theirs3", &clock);
    b.insert("party", "oops", &f(json!({"name": "bad"})))
        .unwrap();
    let flaky = crate::testkit::Flaky::new(b.clone());
    flaky
        .refuse_reads
        .lock()
        .unwrap()
        .push(obop_migrate::META_MODEL.to_string());
    let fb: Arc<dyn Backend> = flaky.clone();
    let e = rollback(
        &fb,
        None,
        &ring(),
        &theirs,
        &keys(),
        "wrong file",
        "operator",
        false,
        &*clock,
    )
    .unwrap_err();
    assert!(e.starts_with("storage_busy"), "{e}");
    assert!(b.get("party", "oops").unwrap().is_some(), "untouched");
    assert!(b.get("party", "x1").unwrap().is_none());
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
            false,
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

/// Two databases that both predate ids cannot be compared: the gate does not pretend it did, and
/// does not refuse either (that would block every upgrade of an old database): it warns.
#[test]
fn a_backup_and_a_database_that_both_predate_ids_are_not_called_compared() {
    let dir = Dir::new("noids");
    let (b, url) = old_db(&dir);
    let clock = Arc::new(FakeClock::at(T0));
    let bk = backup_now(&b, &dir, "old", &clock);
    let mut g = gate(&b, &url, &clock, Mode::Strict);
    g.options.backup_file = Some(bk.clone());
    plan_and_store(&g);
    let (_, c) = g.preflight().unwrap();
    assert_eq!(
        check(&c, "backup").level,
        Level::Warn,
        "{}",
        check(&c, "backup").detail
    );
    assert!(check(&c, "backup").detail.contains("could not be tied"));
    // A receipt of the same unnamed pair says so too.
    rehearse_to_receipt(&g, &bk, &dir.p("r.json"), None);
    g.options.receipt = Some(dir.p("r.json"));
    let (_, c) = g.preflight().unwrap();
    assert_eq!(check(&c, "rehearsal_identity").level, Level::Warn);
    // Once the database is named, the same unnamed backup is refused.
    Migrator::new(b.clone(), ring())
        .ensure_instance_id()
        .unwrap();
    let (_, c) = g.preflight().unwrap();
    assert_eq!(check(&c, "backup").level, Level::Fail);
}

/// Ids in messages are cut by characters: a manifest cannot make a message panic, and a manifest
/// whose database id is not a database id is not a backup this obop wrote.
#[test]
fn a_database_id_is_cut_by_characters_and_a_manifest_with_a_foreign_one_is_refused() {
    assert_eq!(
        short("\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}")
            .chars()
            .count(),
        8
    );
    assert_eq!(short("ab"), "ab");
    let dir = Dir::new("foreignid");
    let (b, _) = old_db(&dir);
    Migrator::new(b.clone(), ring())
        .ensure_instance_id()
        .unwrap();
    let clock = Arc::new(FakeClock::at(T0));
    let bk = backup_now(&b, &dir, "named", &clock);
    let manifest = PathBuf::from(format!("{}.manifest.json", bk.display()));
    let mut j: J = serde_json::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
    for bad in [
        J::from("\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}"),
        J::from(7),
        J::from("g".repeat(33)),
        J::from("a".repeat(65)),
    ] {
        j["database_id"] = bad.clone();
        std::fs::write(&manifest, j.to_string()).unwrap();
        let e = backup::verify_with(&bk, &keys()).unwrap_err();
        assert!(e.contains("not a database id"), "{bad}: {e}");
    }
}
