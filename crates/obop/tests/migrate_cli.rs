//! The migration gates, scheduled backups, the restore drill and the storage checks through the
//! real binary (D73): `obop migrate plan|run|rehearse|rollback|verify`, `obop backup run|list`,
//! `obop drill`, `obop storage check|attest|show`. SQLite stands in for the database; the
//! PostgreSQL paths of the same code are covered by the unit tests that take
//! `OBOP_TEST_POSTGRES_URL`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const KEY: &str = "abababababababababababababababababababababababababababababababab";
const OTHER: &str = "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd";

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("obop-mig-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
    fn p(&self, n: &str) -> PathBuf {
        self.0.join(n)
    }
    fn s(&self, n: &str) -> String {
        self.p(n).display().to_string()
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

fn run(env: &[(&str, &str)], args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_obop"))
        .env_remove("OBOP_DATABASE_URL")
        .env_remove("OBOP_SECRET_KEY")
        .env_remove("OBOP_SECRET_KEY_PREVIOUS")
        .env_remove("OBOP_BACKUP_KEY")
        .env_remove("OBOP_BACKUP_KEY_PREVIOUS")
        .env_remove("OBOP_BACKUP_DIR")
        .env_remove("OBOP_BACKUP_COPY_DIR")
        .env_remove("OBOP_BACKUP_SCHEDULE")
        .env_remove("OBOP_MIGRATION_GATES")
        .env_remove("OBOP_AUTO_MIGRATE")
        .env_remove("OBOP_DEV_INSECURE_KEY")
        .env("OBOP_REQUIRE_MFA_FOR_ADMINS", "0")
        .envs(env.iter().copied())
        .args(args)
        .output()
        .expect("run obop")
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

fn ok(o: &Output) -> String {
    assert!(o.status.success(), "{}", text(o));
    text(o)
}

/// A database from before versioning: records and no schema record.
fn old_db(dir: &Dir, name: &str) -> String {
    let url = dir.url(name);
    let b = obop_storage::open(&url).unwrap();
    let f = |v: serde_json::Value| v.as_object().unwrap().clone();
    b.insert(
        "auth_user",
        "ada",
        &f(serde_json::json!({"groups": ["base.user"]})),
    )
    .unwrap();
    b.insert(
        "party",
        "p1",
        &f(serde_json::json!({"name": "Ada Lovelace"})),
    )
    .unwrap();
    url
}

fn only_file(dir: &Path, ext: &str) -> PathBuf {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(ext))
        .collect();
    assert_eq!(v.len(), 1, "{v:?}");
    v.remove(0)
}

#[test]
fn the_strict_gates_refuse_then_allow_a_migration_that_is_then_rolled_back_with_a_reason() {
    let dir = Dir::new("strict");
    let url = old_db(&dir, "prod.db");
    let env = [("OBOP_BACKUP_KEY", KEY), ("OBOP_SECRET_KEY", KEY)];
    // Without a backup, a stored plan or a rehearsal the strict run is refused.
    let o = run(&env, &["migrate", "run", "--gates", "strict", "--db", &url]);
    assert!(!o.status.success());
    let t = text(&o);
    assert!(
        t.contains("not allowed to run")
            && t.contains("backup")
            && t.contains("plan_stored")
            && t.contains("rehearsal"),
        "{t}"
    );
    assert!(ok(&run(&env, &["migrate", "status", "--db", &url])).contains("2 step(s) pending"));
    // Read the plan (it is stored), take a backup, rehearse it on a copy.
    let t = ok(&run(&env, &["migrate", "plan", "--db", &url]));
    assert!(
        t.contains("stored") && t.contains("step 1") && t.contains("step 2"),
        "{t}"
    );
    let bdir = dir.s("backups");
    ok(&run(&env, &["backup", "--db", &url, "--out", &bdir]));
    let backup = only_file(&dir.p("backups"), ".obbk");
    let receipt = dir.s("receipt.json");
    let t = ok(&run(
        &env,
        &[
            "migrate",
            "rehearse",
            "--from",
            backup.to_str().unwrap(),
            "--db",
            &url,
            "--out",
            &receipt,
        ],
    ));
    assert!(
        t.contains("REHEARSAL PASSED") && t.contains("receipt written"),
        "{t}"
    );
    // A receipt made with another server key is refused.
    let alien = dir.s("alien.json");
    ok(&run(
        &[("OBOP_BACKUP_KEY", KEY), ("OBOP_SECRET_KEY", OTHER)],
        &[
            "migrate",
            "rehearse",
            "--from",
            backup.to_str().unwrap(),
            "--out",
            &alien,
        ],
    ));
    let o = run(
        &env,
        &[
            "migrate",
            "run",
            "--gates",
            "strict",
            "--backup",
            backup.to_str().unwrap(),
            "--rehearsal",
            &alien,
            "--db",
            &url,
        ],
    );
    assert!(
        !o.status.success() && text(&o).contains("the receipt cannot be checked"),
        "{}",
        text(&o)
    );
    // The real run: backup named, plan stored, rehearsal receipt. A dry run first.
    let args = [
        "migrate",
        "run",
        "--gates",
        "strict",
        "--backup",
        backup.to_str().unwrap(),
        "--rehearsal",
        &receipt,
        "--db",
        &url,
    ];
    let mut dry = args.to_vec();
    dry.push("--dry-run");
    let t = ok(&run(&env, &dry));
    assert!(
        t.contains("dry run") && t.contains("nothing was taken or run"),
        "{t}"
    );
    let t = ok(&run(&env, &args));
    assert!(
        t.contains("ran 1:") && t.contains("ran 2:") && t.contains("done: schema version 2"),
        "{t}"
    );
    // Status prints the ledger with the backup; verify is clean.
    let t = ok(&run(&env, &["migrate", "status", "--db", &url]));
    assert!(
        t.contains("core/0001")
            && t.contains("core/0002")
            && t.contains("applied")
            && t.contains("backup obop-backup-"),
        "{t}"
    );
    assert!(ok(&run(&env, &["migrate", "verify", "--db", &url])).contains("agree"));
    // The way back needs --yes and a reason, and says what it replaces.
    let o = run(
        &env,
        &[
            "migrate",
            "rollback",
            "--from",
            backup.to_str().unwrap(),
            "--reason",
            "tried it",
            "--db",
            &url,
        ],
    );
    assert_eq!(o.status.code(), Some(2), "{}", text(&o));
    assert!(text(&o).contains("--yes"));
    let t = ok(&run(
        &env,
        &[
            "migrate",
            "rollback",
            "--from",
            backup.to_str().unwrap(),
            "--reason",
            "the rehearsal said it was safe; it was not",
            "--yes",
            "--db",
            &url,
        ],
    ));
    assert!(
        t.contains("rolled back") && t.contains("schema version 2 became none"),
        "{t}"
    );
    let t = ok(&run(&env, &["migrate", "status", "--db", &url]));
    assert!(
        t.contains("core/rollback-") && t.contains("the rehearsal said it was safe"),
        "{t}"
    );
    // Without the key the rollback stops with the stable code and changes nothing.
    let o = run(
        &[],
        &[
            "migrate",
            "rollback",
            "--from",
            backup.to_str().unwrap(),
            "--reason",
            "x",
            "--yes",
            "--db",
            &url,
        ],
    );
    assert_eq!(o.status.code(), Some(3), "{}", text(&o));
}

#[test]
fn on_sqlite_the_gates_only_warn_and_a_changed_step_cannot_be_run_around() {
    let dir = Dir::new("relaxed");
    let url = old_db(&dir, "dev.db");
    let o = run(&[], &["migrate", "run", "--db", &url]);
    let t = ok(&o);
    assert!(t.contains("ran 1:") && t.contains("ran 2:"), "{t}");
    assert!(
        text(&o).contains("warn"),
        "the gates say what they would have wanted: {t}"
    );
    assert!(ok(&run(&[], &["migrate", "run", "--db", &url])).contains("up to date"));
    assert!(
        run(&[], &["migrate", "verify", "--db", &url])
            .status
            .success()
    );
    // Drift: edit the ledger row's checksum as if the step had been edited after it ran.
    let b = obop_storage::open(&url).unwrap();
    b.update(
        "obop_migration",
        "core/0002",
        &serde_json::json!({"checksum": "00".repeat(32)})
            .as_object()
            .unwrap()
            .clone(),
    )
    .unwrap();
    drop(b);
    let o = run(&[], &["migrate", "verify", "--db", &url]);
    assert!(
        !o.status.success()
            && text(&o).contains("step 2")
            && text(&o).contains("was changed after it ran"),
        "{}",
        text(&o)
    );
    // Starting anything on that database stops and names the step.
    let o = run(&[], &["admin", "users", "--db", &url]);
    assert!(
        !o.status.success() && text(&o).contains("step 2"),
        "{}",
        text(&o)
    );
}

#[test]
fn a_server_that_migrates_itself_takes_a_verified_backup_first_when_it_has_a_directory() {
    let dir = Dir::new("auto");
    let url = old_db(&dir, "auto.db");
    let bdir = dir.s("backups");
    let o = run(
        &[("OBOP_BACKUP_DIR", &bdir), ("OBOP_BACKUP_KEY", KEY)],
        &["admin", "users", "--db", &url],
    );
    let t = text(&o);
    assert!(o.status.success(), "{t}");
    assert!(
        t.contains("backup before migrating") && t.contains("migrated to 2"),
        "{t}"
    );
    only_file(&dir.p("backups"), ".obbk");
    let t = ok(&run(&[], &["migrate", "status", "--db", &url]));
    assert!(
        t.contains("backup obop-backup-") && t.contains("by server"),
        "{t}"
    );
    // Without a directory it migrates, and says it had no backup.
    let url2 = old_db(&dir, "auto2.db");
    let o = run(&[], &["admin", "users", "--db", &url2]);
    assert!(text(&o).contains("without a backup"), "{}", text(&o));
}

#[test]
fn the_scheduled_job_backs_up_copies_off_the_host_and_applies_retention() {
    let dir = Dir::new("job");
    let url = old_db(&dir, "src.db");
    let env = [("OBOP_BACKUP_KEY", KEY)];
    let (out, off) = (dir.s("local"), dir.s("offsite"));
    let t = ok(&run(
        &env,
        &[
            "backup",
            "run",
            "--db",
            &url,
            "--out",
            &out,
            "--copy-to",
            &off,
            "--keep-daily",
            "2",
        ],
    ));
    assert!(
        t.contains("written and verified and authenticated")
            && t.contains("read back and hash-checked"),
        "{t}"
    );
    assert_eq!(
        std::fs::read_dir(&off).unwrap().count(),
        2,
        "data file and manifest"
    );
    let t = ok(&run(&env, &["backup", "list", "--db", &url]));
    assert!(
        t.contains("authenticated") && t.contains("fs:") && t.contains("scheduled"),
        "{t}"
    );
    // The off-host copy alone restores somewhere else (the drill from the copy).
    let copy = only_file(Path::new(&off), ".obbk");
    let dst = dir.url("restored.db");
    ok(&run(
        &env,
        &["restore", "--db", &dst, "--from", copy.to_str().unwrap()],
    ));
    assert!(ok(&run(&env, &["admin", "users", "--db", &dst])).contains("ada"));
    // A manual backup is never pruned.
    ok(&run(
        &env,
        &[
            "backup",
            "run",
            "--db",
            &url,
            "--out",
            &out,
            "--reason",
            "manual",
            "--keep-daily",
            "0",
            "--keep-monthly",
            "0",
        ],
    ));
    let t = ok(&run(
        &env,
        &[
            "backup",
            "prune",
            "--db",
            &url,
            "--dry-run",
            "--keep-daily",
            "0",
            "--keep-monthly",
            "0",
        ],
    ));
    assert!(
        t.contains("would keep") && t.contains("policy: 0 daily"),
        "{t}"
    );
    // A bad number is an error, not a default.
    let o = run(
        &env,
        &[
            "backup",
            "run",
            "--db",
            &url,
            "--out",
            &out,
            "--keep-daily",
            "lots",
        ],
    );
    assert!(
        !o.status.success() && text(&o).contains("whole number"),
        "{}",
        text(&o)
    );
}

#[test]
fn a_drill_checks_the_copy_records_itself_and_fails_loudly_with_the_wrong_key() {
    let dir = Dir::new("drill");
    let url = old_db(&dir, "src.db");
    let env = [("OBOP_BACKUP_KEY", KEY), ("OBOP_SECRET_KEY", KEY)];
    // Make it a current database first so the schema check has nothing pending.
    ok(&run(&env, &["migrate", "run", "--db", &url]));
    let bdir = dir.s("b");
    ok(&run(&env, &["backup", "--db", &url, "--out", &bdir]));
    let backup = only_file(&dir.p("b"), ".obbk");
    let b = backup.to_str().unwrap();
    // The storage check starts out failing on the evidence it lacks.
    let o = run(&env, &["storage", "check", "--db", &url]);
    let t = text(&o);
    assert!(
        t.contains("restore_drill") && t.contains("storage_attestation"),
        "{t}"
    );
    // Drill, record it, attest, and the evidence is there.
    let t = ok(&run(&env, &["drill", b, "--record-in", &url]));
    assert!(
        t.contains("DRILL PASSED")
            && t.contains("content_digest")
            && t.contains("recorded as drill-"),
        "{t}"
    );
    ok(&run(
        &env,
        &[
            "storage",
            "attest",
            "--db",
            &url,
            "--host",
            "db1",
            "--provider",
            "Acme",
            "--mechanism",
            "provider-managed volume encryption, AES-256",
            "--custody",
            "Acme KMS",
            "--readers",
            "the KMS only",
            "--evidence",
            "ticket 42",
        ],
    ));
    let o = run(
        &env,
        &[
            "storage",
            "attest",
            "--db",
            &url,
            "--host",
            "h",
            "--provider",
            "p",
            "--mechanism",
            "rot13",
            "--custody",
            "c",
            "--readers",
            "r",
            "--evidence",
            "e",
        ],
    );
    assert!(
        !o.status.success() && text(&o).contains("AES-256"),
        "{}",
        text(&o)
    );
    let t = ok(&run(&env, &["storage", "show", "--db", &url]));
    assert!(
        t.contains("attestation main") && t.contains("drill ") && t.contains("passed"),
        "{t}"
    );
    // Injected failures: the wrong key exits non-zero with the stable code in the output.
    let o = run(&[("OBOP_BACKUP_KEY", OTHER)], &["drill", b]);
    assert!(
        !o.status.success()
            && text(&o).contains("restore_key_required")
            && text(&o).contains("DRILL FAILED"),
        "{}",
        text(&o)
    );
    // A corrupt archive.
    let mut bytes = std::fs::read(&backup).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 1;
    std::fs::write(dir.p("b/corrupt.obbk"), &bytes).unwrap();
    std::fs::copy(
        format!("{b}.manifest.json"),
        dir.p("b/corrupt.obbk.manifest.json"),
    )
    .unwrap();
    let o = run(&env, &["drill", dir.p("b/corrupt.obbk").to_str().unwrap()]);
    assert!(
        !o.status.success() && text(&o).contains("DRILL FAILED"),
        "{}",
        text(&o)
    );
    // A truncated archive.
    bytes = std::fs::read(&backup).unwrap();
    std::fs::write(dir.p("b/cut.obbk"), &bytes[..bytes.len() - 30]).unwrap();
    std::fs::copy(
        format!("{b}.manifest.json"),
        dir.p("b/cut.obbk.manifest.json"),
    )
    .unwrap();
    let o = run(&env, &["drill", dir.p("b/cut.obbk").to_str().unwrap()]);
    assert!(!o.status.success(), "{}", text(&o));
    // No manifest: it cannot even start.
    std::fs::copy(&backup, dir.p("b/lonely.obbk")).unwrap();
    let o = run(&env, &["drill", dir.p("b/lonely.obbk").to_str().unwrap()]);
    assert!(
        !o.status.success() && text(&o).contains("manifest"),
        "{}",
        text(&o)
    );
    // It never restores into the database it protects.
    let o = run(&env, &["drill", b, "--scratch", &url, "--record-in", &url]);
    assert!(
        !o.status.success() && text(&o).contains("production database"),
        "{}",
        text(&o)
    );
}
