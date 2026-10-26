//! Migration gates: backup, preflight, rehearsal and rollback (OBR-DAT-053, D73).
//!
//! A migration is forward-only (D59): the way back is a backup. These gates make sure there is one
//! before anything is written, that the operator has read the plan that will run, that the plan
//! has run to its end on a copy, and that going back is one command with a recorded reason.
//!
//! **Mode.** On PostgreSQL the gates are *strict*: [`Gate::run`] refuses unless (a) a verified
//! backup of *this database* no older than 24 hours is named or can be taken now and its id goes
//! into the ledger, (b) `obop migrate plan` stored a plan for exactly this database state and
//! build, (c) there is room on the disk (the backup directory and, for SQLite, the database
//! file's; PostgreSQL's own volume cannot be seen from a client, so the operator attests it), (d)
//! a rehearsal of the same plan, on a backup of this database, passed, shown by a receipt the
//! server key authenticates, and the ledger is clean and no other migration holds the lock. On
//! SQLite and in memory (development, D44) the same checks run and a failing safeguard is a
//! warning: a developer's database is not a deployment. A run with nothing pending needs none of
//! it: it says "up to date" and checks only that the ledger is clean and the database is not
//! newer than this build.
//!
//! "This database" is a name, not a comparison of contents: every database carries a random id
//! (`obop_meta/instance`, written once) that its backups and rehearsal receipts repeat, and a
//! backup or receipt that names another database is refused. A backup or receipt made before the
//! database had an id cannot be tied to it and is refused in strict mode as well: take a new one.
//!
//! A server that migrates itself at start (`--auto-migrate`, the choice of a one-process hosted
//! instance) takes the verified backup first when a backup directory is configured
//! ([`auto_backup_hook`], called under the migration lock once steps are certain to run, reusing
//! the backup of an attempt that failed instead of writing another) and runs without the plan and
//! rehearsal; an operator who wants all four uses `obop migrate run` with auto-migrate off.

use crate::backup::{self, Keys};
use crate::catalog::{BackupRecord, Catalog};
use crate::doctor::{Check, Level, human};
use crate::drill::{self, Scratch};
use crate::pgadmin::ServerInfo;
use crate::schedule::{self, Clock, Settings};
use obop_keys::KeyRing;
use obop_migrate::{EntryStatus, Migrator, Plan, RunOptions};
use obop_storage::Backend;
use obop_storage::blob::Blobs;
use serde_json::{Value as J, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How old the backup before a migration may be: 24 hours.
pub const MAX_BACKUP_AGE: Duration = Duration::from_secs(24 * 3600);
/// How long a rehearsal receipt counts: a week, so it belongs to this release and this data.
pub const RECEIPT_MAX_AGE: u64 = 7 * 86_400;
/// The longest a statement that locks a table may take in a rehearsal: one second.
pub const MAX_LOCK_MS: u64 = 1000;
/// How long the preflight waits for the migration lock before it says another process holds it.
pub const LOCK_WAIT: Duration = Duration::from_secs(3);
const RECEIPT_PURPOSE: &str = "migration-rehearsal";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A failing check refuses the run.
    Strict,
    /// A failing check is a warning (development databases).
    Relaxed,
}

impl Mode {
    /// Strict on PostgreSQL, relaxed elsewhere (D44).
    pub fn for_backend(kind: &str) -> Mode {
        if kind == "postgres" {
            Mode::Strict
        } else {
            Mode::Relaxed
        }
    }

    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "strict" => Some(Mode::Strict),
            "relaxed" => Some(Mode::Relaxed),
            _ => None,
        }
    }
}

/// What an operator gives to `obop migrate run`.
pub struct Options {
    pub mode: Mode,
    /// Who the ledger names.
    pub actor: String,
    /// Where a backup before the run is taken (`OBOP_BACKUP_DIR`).
    pub backup_dir: Option<PathBuf>,
    /// An existing backup to use instead of taking one (verified, at most 24 hours old).
    pub backup_file: Option<PathBuf>,
    pub keys: Keys,
    /// A rehearsal receipt (`obop migrate rehearse --out`).
    pub receipt: Option<PathBuf>,
    /// The operator names a maintenance window for a long-lock step on a big table.
    pub window: bool,
    /// The operator attests that the database host has free disk of at least twice its largest
    /// table (PostgreSQL's disk cannot be seen from here).
    pub disk_attested: bool,
    /// The database URL, to find the SQLite file.
    pub url: String,
    /// How long the preflight waits for the migration lock ([`LOCK_WAIT`]).
    pub lock_wait: Duration,
    /// The table size above which a long-lock step needs `window` (the product's limit when
    /// unset; tests lower it).
    pub window_rows: Option<u64>,
}

pub struct Gate {
    pub backend: Arc<dyn Backend>,
    pub ring: KeyRing,
    pub blobs: Option<Arc<dyn Blobs>>,
    pub server: Option<Arc<dyn ServerInfo>>,
    pub clock: Arc<dyn Clock>,
    pub options: Options,
    /// Other steps than this build's (tests).
    pub steps: Option<Vec<obop_migrate::Step>>,
}

fn add(v: &mut Vec<Check>, name: &'static str, level: Level, detail: impl Into<String>) {
    v.push(Check {
        name,
        about: "OBR-DAT-053",
        level,
        detail: detail.into(),
    });
}

/// A failing check is a failure in strict mode and a warning in relaxed mode.
fn gated(mode: Mode, fail: bool) -> Level {
    match (fail, mode) {
        (false, _) => Level::Pass,
        (true, Mode::Strict) => Level::Fail,
        (true, Mode::Relaxed) => Level::Warn,
    }
}

/// The nearest ancestor of `p` that exists (a backup directory is made when needed).
fn existing_ancestor(p: &Path) -> PathBuf {
    let mut p = p.to_path_buf();
    while !p.exists() {
        match p.parent() {
            Some(q) if !q.as_os_str().is_empty() => p = q.to_path_buf(),
            _ => return PathBuf::from("."),
        }
    }
    p
}

/// Free bytes on the volume that holds `p`.
pub fn free_bytes(p: &Path) -> Result<u64, String> {
    fs4::available_space(existing_ancestor(p))
        .map_err(|e| format!("cannot read the free space of {}: {e}", p.display()))
}

fn sqlite_file(url: &str) -> Option<PathBuf> {
    let p = url
        .strip_prefix("sqlite://")
        .or_else(|| url.strip_prefix("sqlite:"))?;
    (!p.is_empty() && p != ":memory:").then(|| PathBuf::from(p))
}

/// The signed statement that a rehearsal of one plan passed, on a backup of one database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    pub plan_id: String,
    pub core_version: String,
    pub passed: bool,
    pub at: u64,
    pub backup_id: String,
    pub longest_step_ms: u64,
    pub total_ms: u64,
    /// The database the backup came from (`obop_meta/instance`); `None` for a backup of a
    /// database that had no id yet.
    pub database_id: Option<String>,
    /// The engine of the scratch database the plan ran on (`sqlite`, `postgres`, `memory`).
    pub engine: String,
    /// Records in the rehearsed copy before the plan ran.
    pub records: u64,
}

impl Receipt {
    fn body(&self, key_id: &str) -> J {
        json!({
            "kind": "obop-rehearsal-receipt", "plan_id": self.plan_id,
            "core_version": self.core_version, "passed": self.passed, "at": self.at,
            "backup_id": self.backup_id, "longest_step_ms": self.longest_step_ms,
            "total_ms": self.total_ms, "key_id": key_id,
            "database_id": self.database_id, "engine": self.engine, "records": self.records,
        })
    }

    /// The receipt as JSON with an HMAC under the server key: a receipt cannot be written by hand.
    pub fn sign(&self, ring: &KeyRing) -> Result<J, String> {
        let key = ring
            .current()
            .ok_or("a receipt is signed with the server key: set OBOP_SECRET_KEY")?;
        let mut j = self.body(key.id());
        let tag = ring
            .mac(key.id(), RECEIPT_PURPOSE, j.to_string().as_bytes())
            .map_err(|e| e.to_string())?;
        j["mac"] = J::from(obop_keys::hex(&tag));
        Ok(j)
    }

    pub fn verify(j: &J, ring: &KeyRing) -> Result<Receipt, String> {
        let s = |k: &str| {
            j[k].as_str()
                .map(String::from)
                .ok_or_else(|| format!("the receipt has no {k}"))
        };
        let u = |k: &str| {
            j[k].as_u64()
                .ok_or_else(|| format!("the receipt has no {k}"))
        };
        let r = Receipt {
            plan_id: s("plan_id")?,
            core_version: s("core_version")?,
            passed: j["passed"].as_bool().ok_or("the receipt has no passed")?,
            at: u("at")?,
            backup_id: s("backup_id")?,
            longest_step_ms: u("longest_step_ms")?,
            total_ms: u("total_ms")?,
            database_id: j["database_id"].as_str().map(String::from),
            engine: s("engine")?,
            records: u("records")?,
        };
        if j["kind"] != "obop-rehearsal-receipt" {
            return Err("not a rehearsal receipt".into());
        }
        let key_id = s("key_id")?;
        let mac = obop_keys::unhex(&s("mac")?).ok_or("the receipt's mac is not hex")?;
        let mut body = j.clone();
        body.as_object_mut().map(|o| o.remove("mac"));
        if !ring
            .mac_matches(&key_id, RECEIPT_PURPOSE, body.to_string().as_bytes(), &mac)
            .map_err(|e| format!("the receipt cannot be checked: {e}"))?
        {
            return Err(
                "the receipt does not authenticate: it was edited or made with another server key"
                    .into(),
            );
        }
        Ok(r)
    }
}

/// A taken or named backup.
#[derive(Debug, Clone)]
pub struct BackupRef {
    pub id: String,
    pub path: PathBuf,
    pub age_seconds: u64,
}

/// Whether the backup and the database it is used for are the same database: `Ok` with a note
/// when the answer is "not checked" (neither has an id: a database from before ids), `Err` when
/// they differ or the backup cannot be tied to this database.
fn same_database(backup: &Option<String>, target: &Option<String>) -> Result<(), String> {
    match (backup, target) {
        (Some(b), Some(t)) if b == t => Ok(()),
        (Some(b), Some(t)) => Err(format!(
            "it is a backup of another database (id {}), this database is {}",
            &b[..b.len().min(8)],
            &t[..t.len().min(8)]
        )),
        (None, None) => Ok(()),
        (None, Some(_)) => Err(
            "it carries no database id (it was made before this database was named), so it cannot be tied to this database: take a new one with `obop backup`"
                .into(),
        ),
        (Some(_), None) => Err(
            "it names a database, but this database has no id: this is not the database it came from"
                .into(),
        ),
    }
}

/// A pre-migration backup that an earlier, failed attempt already used (the ledger names it on a
/// `failed` or `partial` row), that still verifies, is under 24 hours old, was taken at or before
/// the schema version the database is at now, and after which nothing was applied that does not
/// name it too: the data has not moved on since except by the steps of that same migration, so it
/// is the backup before the migration still, and writing another would only fill the volume.
pub fn reuse_pre_migration(
    backend: &Arc<dyn Backend>,
    settings: &Settings,
    stored: u32,
    clock: &dyn Clock,
) -> Option<BackupRecord> {
    let rows = obop_migrate::ledger::read_all(&**backend).ok()?;
    let used: std::collections::BTreeSet<&str> = rows
        .iter()
        .filter(|r| matches!(r.status, EntryStatus::Failed | EntryStatus::Partial))
        .filter_map(|r| r.backup_id.as_deref())
        .collect();
    if used.is_empty() {
        return None;
    }
    let now = clock.now();
    Catalog::new(&**backend)
        .backups()
        .ok()?
        .into_iter()
        .rev()
        .find(|b| {
            b.reason == "pre-migration"
                && b.verified
                && used.contains(b.id.as_str())
                && b.schema_version <= u64::from(stored)
                && now.saturating_sub(b.created_at) <= MAX_BACKUP_AGE.as_secs()
                && !rows.iter().any(|r| {
                    r.status == EntryStatus::Applied
                        && r.at >= b.created_at
                        && r.backup_id.as_deref() != Some(b.id.as_str())
                })
                && schedule::is_ours(b, settings)
                && Path::new(&b.path).exists()
                && backup::verify_with(Path::new(&b.path), &settings.keys).is_ok()
        })
}

impl Gate {
    fn schema(&self) -> u64 {
        self.migrator()
            .status()
            .map(|s| u64::from(s.stored.unwrap_or(0)))
            .unwrap_or(0)
    }

    fn migrator(&self) -> Migrator {
        let m = match &self.steps {
            Some(s) => Migrator::with_steps(self.backend.clone(), self.ring.clone(), s.clone()),
            None => Migrator::new(self.backend.clone(), self.ring.clone()),
        }
        .by(&self.options.actor);
        match self.options.window_rows {
            Some(n) => m.with_window_rows(n),
            None => m,
        }
    }

    /// An existing backup named by the operator: verified (opened with its key when sealed), of
    /// this database, and young enough.
    fn existing_backup(&self, file: &Path) -> Result<BackupRef, String> {
        let v = backup::verify_with(file, &self.options.keys)?;
        let created =
            backup::parse_rfc3339(&v.manifest.created_at).ok_or("the backup has no usable date")?;
        let age = self.clock.now().saturating_sub(created);
        if age > MAX_BACKUP_AGE.as_secs() {
            return Err(format!(
                "the backup {} is {} old; a migration needs one from the last 24 hours",
                file.display(),
                human(age)
            ));
        }
        if v.manifest.schema_version > u64::from(obop_migrate::steps().len() as u32) {
            return Err("the backup is from a newer schema than this build".into());
        }
        let target = self.migrator().instance_id()?;
        same_database(&v.manifest.database_id, &target)
            .map_err(|e| format!("the backup {}: {e}", file.display()))?;
        Ok(BackupRef {
            id: file
                .file_name()
                .map(|n| {
                    n.to_string_lossy()
                        .trim_end_matches(".manifest.json")
                        .to_string()
                })
                .unwrap_or_default(),
            path: file.to_path_buf(),
            age_seconds: age,
        })
    }

    /// Take the backup before the run: sealed when a key is configured, read back and (when
    /// sealed) authenticated, and recorded. The backup of an earlier attempt that failed is used
    /// again instead (see [`reuse_pre_migration`]).
    fn take_backup(&self, dir: &Path) -> Result<BackupRef, String> {
        let settings = Settings::new(dir, self.options.keys.clone());
        let stored = self.migrator().status()?.stored.unwrap_or(0);
        if let Some(b) = reuse_pre_migration(&self.backend, &settings, stored, &*self.clock) {
            return Ok(BackupRef {
                id: b.id,
                path: PathBuf::from(b.path),
                age_seconds: self.clock.now().saturating_sub(b.created_at),
            });
        }
        let blobs: Option<&dyn Blobs> = self.blobs.as_deref();
        let o = schedule::take(
            &self.backend,
            blobs,
            self.schema(),
            &settings,
            "pre-migration",
            &*self.clock,
        )?;
        Ok(BackupRef {
            id: o.backup.id.clone(),
            path: PathBuf::from(&o.backup.path),
            age_seconds: 0,
        })
    }

    /// The checks that read and write nothing. Returns the plan they were made for. With nothing
    /// to run, only the checks that concern the database itself are made (it is not newer than
    /// this build, its ledger is clean): no backup, plan, rehearsal or disk is asked for.
    pub fn preflight(&self) -> Result<(Plan, Vec<Check>), String> {
        let m = self.migrator();
        let o = &self.options;
        let mut v = Vec::new();
        let st = m.status()?;
        add(
            &mut v,
            "schema",
            if st.ahead { Level::Fail } else { Level::Pass },
            st.describe(),
        );
        if st.ahead {
            // Nothing else can be said about a database this build does not understand.
            let plan = Plan {
                id: String::new(),
                from: st.stored.unwrap_or(0),
                to: st.latest,
                core_version: obop_types::CORE_VERSION.into(),
                steps: Vec::new(),
                created_at: self.clock.now(),
            };
            return Ok((plan, v));
        }
        let verify = m.verify()?;
        add(
            &mut v,
            "ledger",
            if verify.ok() {
                Level::Pass
            } else {
                Level::Fail
            },
            if verify.ok() {
                format!("clean ({} row(s))", m.ledger()?.len())
            } else {
                verify.problems.join("; ")
            },
        );
        let plan = m.plan()?;
        if plan.is_empty() {
            return Ok((plan, v));
        }
        add(
            &mut v,
            "versions",
            if plan.core_version == obop_types::CORE_VERSION {
                Level::Pass
            } else {
                Level::Fail
            },
            format!(
                "plan {} is for schema {} to {} with obop {}; this is obop {}",
                plan.id,
                plan.from,
                plan.to,
                plan.core_version,
                obop_types::CORE_VERSION
            ),
        );
        if plan.downtime() {
            add(
                &mut v,
                "downtime",
                Level::Warn,
                "a step is not compatible with the previous release: stop every process before running",
            );
        }
        let lock = lock_level(&self.backend, o.lock_wait);
        add(
            &mut v,
            "lock",
            lock,
            if lock == Level::Pass {
                "no other migration holds the lock"
            } else {
                "another process holds the migration lock (a migration is running, or a server is starting): wait for it"
            },
        );
        let window_needed = plan.needs_window();
        add(
            &mut v,
            "window",
            if window_needed && !o.window {
                Level::Fail
            } else {
                Level::Pass
            },
            if window_needed {
                if o.window {
                    "a long-lock step on a big table runs in the named window".into()
                } else {
                    format!(
                        "a step holds a long lock on a table above {} records: run in a maintenance window with --window",
                        o.window_rows.unwrap_or(obop_migrate::ddl::WINDOW_ROWS)
                    )
                }
            } else {
                "no step holds a long lock on a big table".to_string()
            },
        );
        // Stored plan.
        let stored = m.stored_plan(&plan.id)?.is_some();
        add(
            &mut v,
            "plan_stored",
            gated(o.mode, !stored),
            if stored {
                format!("plan {} was produced by `obop migrate plan`", plan.id)
            } else {
                format!(
                    "no stored plan {}: run `obop migrate plan` and read it first",
                    plan.id
                )
            },
        );
        // Backup.
        let (level, detail) = match (&o.backup_file, &o.backup_dir) {
            (Some(f), _) => match self.existing_backup(f) {
                Ok(b) => (Level::Pass, format!("{} is verified, of this database and {} old", b.id, human(b.age_seconds))),
                Err(e) => (gated(o.mode, true), e),
            },
            (None, Some(d)) => (Level::Pass, format!("a verified backup will be taken into {} before the first step (or the one a failed attempt already took is used again)", d.display())),
            (None, None) => (
                gated(o.mode, true),
                "no backup: give --backup FILE (verified, of this database, under 24 hours old) or set OBOP_BACKUP_DIR so one is taken first".into(),
            ),
        };
        add(&mut v, "backup", level, detail);
        add(
            &mut v,
            "backup_encrypted",
            if o.keys.can_seal() {
                Level::Pass
            } else {
                Level::Warn
            },
            if o.keys.can_seal() {
                format!("the backup will be sealed ({})", o.keys.describe())
            } else {
                "no OBOP_BACKUP_KEY or OBOP_SECRET_KEY: the backup will be plain".to_string()
            },
        );
        if self.is_postgres() && self.server.is_none() {
            // Without it there is no restore point and no size of the database: say so, and do
            // not let a failed second connection quietly drop those gates.
            add(
                &mut v,
                "server",
                gated(o.mode, true),
                "the PostgreSQL server could not be asked about itself (the connection failed): the restore point and the size of the database are part of the gate; fix the connection (a connection limit, a pooler that refuses a session) and run again, or --gates relaxed to go ahead without them",
            );
        }
        self.disk_checks(&mut v, &plan);
        // Rehearsal.
        let target_id = m.instance_id()?;
        let rehearsed = match &o.receipt {
            None => Err("no rehearsal receipt: run `obop migrate rehearse --from BACKUP --out receipt.json` on a copy, then pass --rehearsal receipt.json".to_string()),
            Some(p) => std::fs::read_to_string(p)
                .map_err(|e| format!("cannot read the receipt {}: {e}", p.display()))
                .and_then(|t| serde_json::from_str::<J>(&t).map_err(|e| format!("the receipt is not JSON: {e}")))
                .and_then(|j| Receipt::verify(&j, &self.ring))
                .and_then(|r| {
                    if r.plan_id != plan.id {
                        Err(format!("the receipt is for plan {}, this database needs plan {}: rehearse again", r.plan_id, plan.id))
                    } else if !r.passed {
                        Err("the rehearsal of this plan FAILED".into())
                    } else if self.clock.now().saturating_sub(r.at) > RECEIPT_MAX_AGE {
                        Err(format!("the receipt is {} old; rehearse again", human(self.clock.now().saturating_sub(r.at))))
                    } else if let Err(e) = same_database(&r.database_id, &target_id) {
                        Err(format!("the rehearsal was made on a backup that is not of this database ({e}); rehearse a backup of this database"))
                    } else {
                        Ok(r)
                    }
                }),
        };
        match rehearsed {
            Ok(r) => {
                add(
                    &mut v,
                    "rehearsal",
                    Level::Pass,
                    format!(
                        "plan {} passed a rehearsal of {} records in {} ms (longest step {} ms) on {}",
                        r.plan_id, r.records, r.total_ms, r.longest_step_ms, r.engine
                    ),
                );
                if r.engine != self.backend.kind() {
                    add(
                        &mut v,
                        "rehearsal_engine",
                        Level::Warn,
                        format!(
                            "the rehearsal ran on {}, this database is {}: the time of a statement on this engine, and its locks, were not measured (give `obop migrate rehearse --scratch URL` a scratch database of the same kind)",
                            r.engine,
                            self.backend.kind()
                        ),
                    );
                }
            }
            Err(e) => add(&mut v, "rehearsal", gated(o.mode, true), e),
        }
        Ok((plan, v))
    }

    fn disk_checks(&self, v: &mut Vec<Check>, plan: &Plan) {
        let o = &self.options;
        let largest = plan.largest_table();
        // The volume the backup goes to, when a backup will be taken.
        if o.backup_file.is_none()
            && let Some(dir) = &o.backup_dir
        {
            let need = self.estimated_backup_bytes();
            match free_bytes(dir) {
                Ok(free) => add(
                    v,
                    "disk_backup",
                    gated(o.mode, free < need.saturating_mul(2)),
                    format!(
                        "{} free where the backup goes; the backup needs about {} (twice that is asked)",
                        mib(free),
                        mib(need)
                    ),
                ),
                Err(e) => add(v, "disk_backup", Level::Warn, e),
            }
        }
        // The volume the database lives on.
        if let Some(f) = sqlite_file(&o.url) {
            let size = std::fs::metadata(&f).map_or(0, |m| m.len());
            match free_bytes(&f) {
                Ok(free) => add(
                    v,
                    "disk_database",
                    gated(o.mode, free < size.saturating_mul(2)),
                    format!(
                        "{} free beside the database file of {} (twice its size is asked)",
                        mib(free),
                        mib(size)
                    ),
                ),
                Err(e) => add(v, "disk_database", Level::Warn, e),
            }
        } else if self.is_postgres() {
            // The volume of a PostgreSQL host cannot be seen from here; the operator says. A
            // server that cannot be asked at all (its connection failed) does not excuse this.
            let how = match self.server.as_ref().map(|s| s.database_bytes()) {
                Some(Ok(size)) => format!("the database is {}", mib(size)),
                Some(Err(e)) => format!("the size of the database could not be read: {e}"),
                None => "the PostgreSQL server could not be asked about itself".to_string(),
            };
            add(
                v,
                "disk_database",
                if o.disk_attested {
                    Level::Pass
                } else {
                    gated(o.mode, true)
                },
                if o.disk_attested {
                    format!(
                        "the operator attests free disk of at least twice the largest table the plan touches ({largest} records; {how})"
                    )
                } else {
                    format!(
                        "the free disk of the PostgreSQL host cannot be read from here ({how}; the plan touches tables of up to {largest} records): check it is at least twice the largest table, then pass --disk-ok"
                    )
                },
            );
        }
    }

    fn is_postgres(&self) -> bool {
        let u = &self.options.url;
        self.backend.kind() == "postgres"
            || u.starts_with("postgres://")
            || u.starts_with("postgresql://")
    }

    fn estimated_backup_bytes(&self) -> u64 {
        if let Some(f) = sqlite_file(&self.options.url) {
            return std::fs::metadata(f).map_or(0, |m| m.len());
        }
        self.server
            .as_ref()
            .and_then(|s| s.database_bytes().ok())
            .unwrap_or(0)
    }
}

fn lock_level(backend: &Arc<dyn Backend>, wait: Duration) -> Level {
    let b = backend.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let r = b.lock("obop.migrate").map(drop);
        let _ = tx.send(r);
    });
    match rx.recv_timeout(wait) {
        Ok(Ok(())) => Level::Pass,
        _ => Level::Fail,
    }
}

fn mib(b: u64) -> String {
    format!("{:.1} MiB", b as f64 / 1_048_576.0)
}

/// What a gated run did.
#[derive(Debug, Clone)]
pub struct Ran {
    pub plan: Plan,
    pub checks: Vec<Check>,
    pub backup: Option<BackupRef>,
    pub restore_point: Option<String>,
    pub applied: Vec<(u32, String, String)>,
    /// A dry run: the checks that would refuse a real run (`name: why`), and what each step would
    /// do. A real run that is refused is an `Err` instead.
    pub would_refuse: Vec<String>,
    pub dry_run: bool,
}

impl Gate {
    /// The gated `obop migrate run`: preflight, a verified backup, a restore point, the steps.
    ///
    /// With nothing pending it only checks the database and says so. With `dry_run` it is a
    /// preview that needs no gate: it names the steps, runs every check and lists those that
    /// would refuse a real run, and takes and writes nothing.
    pub fn run(&self, dry_run: bool, progress: &mut dyn FnMut(&str)) -> Result<Ran, String> {
        let (plan, checks) = self.preflight()?;
        let blocked: Vec<String> = checks
            .iter()
            .filter(|c| c.level == Level::Fail)
            .map(|c| format!("  {}: {}", c.name, c.detail))
            .collect();
        let mut ran = Ran {
            plan: plan.clone(),
            checks,
            backup: None,
            restore_point: None,
            applied: Vec::new(),
            would_refuse: Vec::new(),
            dry_run,
        };
        if dry_run {
            ran.would_refuse = blocked;
            if !plan.is_empty() {
                // What each step would do, as the migrator reports it (nothing is written).
                match self.migrator().run_with(&RunOptions {
                    dry_run: true,
                    window: true,
                    ..RunOptions::default()
                }) {
                    Ok(r) => {
                        ran.applied = r
                            .applied
                            .iter()
                            .map(|(v, n, d)| (*v, (*n).to_string(), d.clone()))
                            .collect();
                    }
                    Err(e) => ran.would_refuse.push(format!("  steps: {e}")),
                }
            }
            return Ok(ran);
        }
        if !blocked.is_empty() {
            return Err(format!(
                "the migration is not allowed to run:\n{}",
                blocked.join("\n")
            ));
        }
        if plan.is_empty() {
            // Up to date. A database from before the ledger, or restored from a backup of one,
            // still gets the rows of the steps it has run, and a name.
            let m = self.migrator();
            match m.adopt().and_then(|_| m.ensure_instance_id()) {
                Ok(_) => {}
                Err(e) => progress(&format!("note: could not record the ledger rows or the database id: {e}")),
            }
            return Ok(ran);
        }
        // A verified backup first.
        let o = &self.options;
        if let Some(f) = &o.backup_file {
            ran.backup = Some(self.existing_backup(f)?);
        } else if let Some(d) = &o.backup_dir {
            progress("taking a verified backup first");
            ran.backup = Some(self.take_backup(d)?);
        }
        if let Some(b) = &ran.backup {
            progress(&format!("backup {} verified", b.id));
        }
        // A restore point on PostgreSQL, so point-in-time recovery can come back to this moment.
        if let Some(server) = &self.server {
            let name = format!("pre-migration-{}", plan.to);
            let archiving = server
                .setting("archive_mode")
                .ok()
                .flatten()
                .is_some_and(|m| matches!(m.as_str(), "on" | "always"));
            match server.create_restore_point(&name) {
                Ok(lsn) => {
                    progress(&format!("restore point {name} at {lsn}"));
                    ran.restore_point = Some(format!("{name}@{lsn}"));
                }
                Err(e) if archiving && o.mode == Mode::Strict => {
                    return Err(format!(
                        "cannot create the restore point {name}: {e}. WAL archiving is on, so the recovery point is part of the gate; grant the role EXECUTE on pg_create_restore_point or run as a role that may"
                    ));
                }
                Err(e) => progress(&format!(
                    "no restore point ({e}); WAL archiving is {}",
                    if archiving { "on" } else { "off" }
                )),
            }
        }
        let report = self.migrator().run_with(&RunOptions {
            dry_run: false,
            backup_id: ran.backup.as_ref().map(|b| b.id.clone()),
            restore_point: ran.restore_point.clone(),
            window: o.window,
            ..RunOptions::default()
        });
        match report {
            Ok(r) => {
                ran.applied = r
                    .applied
                    .iter()
                    .map(|(v, n, d)| (*v, (*n).to_string(), d.clone()))
                    .collect();
                Ok(ran)
            }
            Err(e) => Err(format!(
                "{e}\nthe way back: `obop migrate rollback --from {} --reason \"...\"`{}",
                ran.backup
                    .as_ref()
                    .map_or("<the backup>".to_string(), |b| b.path.display().to_string()),
                ran.restore_point
                    .as_ref()
                    .map_or(String::new(), |p| format!(
                        ", or recover to the restore point {p} from the WAL archive"
                    )),
            )),
        }
    }
}

/// What a server that migrates itself at start (`--auto-migrate`) does about a backup.
pub struct AutoBackup {
    /// `OBOP_BACKUP_DIR`; `None`: the migration goes ahead and the caller says so.
    pub dir: Option<PathBuf>,
    pub keys: Keys,
    /// About how many bytes the backup will take (the database file, or the size PostgreSQL
    /// reports), to check the volume has room before writing.
    pub need_bytes: Option<u64>,
}

/// The backup a server takes before it migrates itself at start, as a hook for
/// `RunOptions::before`: called under the migration lock once steps are certain to run, so a
/// start that is refused (drift, a newer database, a step that needs a window) takes none, two
/// servers starting together take one, and a crash loop reuses the backup of the attempt that
/// failed instead of writing another every few seconds. A volume without room for it stops the
/// migration with that said. `say` is told what happened (and any warning, such as a backup that
/// is not encrypted).
pub fn auto_backup_hook(
    backend: Arc<dyn Backend>,
    blobs: Option<Arc<dyn Blobs>>,
    cfg: AutoBackup,
    clock: Arc<dyn Clock>,
    say: Arc<dyn Fn(&str) + Send + Sync>,
) -> obop_migrate::Before {
    Arc::new(move |p: &obop_migrate::Pending| {
        let Some(dir) = &cfg.dir else {
            say("warning: migrating without a backup: set OBOP_BACKUP_DIR so a verified backup is taken first (docs/DEPLOY.md)");
            return Ok(None);
        };
        let settings = Settings::new(dir, cfg.keys.clone());
        if let Some(b) = reuse_pre_migration(&backend, &settings, p.stored, &*clock) {
            say(&format!(
                "backup before migrating: {} (verified; taken by an earlier attempt of this migration and used again)",
                b.id
            ));
            return Ok(Some(b.id));
        }
        if let Some(need) = cfg.need_bytes {
            match free_bytes(dir) {
                Ok(free) if free < need.saturating_mul(2) => {
                    return Err(format!(
                        "the backup before the migration needs about {} and {} has {} free (twice the size is asked): free some space or point OBOP_BACKUP_DIR at a larger volume; nothing was migrated",
                        mib(need),
                        dir.display(),
                        mib(free)
                    ));
                }
                Ok(_) => {}
                Err(e) => say(&format!("note: {e}")),
            }
        }
        let o = schedule::take(
            &backend,
            blobs.as_deref(),
            u64::from(p.stored),
            &settings,
            "pre-migration",
            &*clock,
        )
        .map_err(|e| {
            format!("the backup before the migration failed, so nothing was migrated: {e}")
        })?;
        say(&format!("backup before migrating: {} (verified)", o.backup.id));
        for n in &o.notes {
            say(&format!("warning: {n}"));
        }
        Ok(Some(o.backup.id))
    })
}

// ---- rehearsal -----------------------------------------------------------------------------

pub struct RehearseOptions {
    pub backup: PathBuf,
    pub keys: Keys,
    pub ring: KeyRing,
    pub scratch: Scratch,
    /// The production URL, to refuse restoring into it.
    pub avoid: Option<String>,
    pub now: u64,
    /// Other steps than this build's (tests).
    pub steps: Option<Vec<obop_migrate::Step>>,
}

#[derive(Debug, Clone)]
pub struct Rehearsal {
    pub plan: Plan,
    pub steps: Vec<(u32, String, u64)>,
    pub longest_step_ms: u64,
    pub total_ms: u64,
    pub records_before: usize,
    pub records_after: usize,
    pub checks: Vec<Check>,
    pub passed: bool,
    pub backup_id: String,
    /// The database the rehearsed backup came from, if it had an id.
    pub database_id: Option<String>,
    /// The engine of the scratch database.
    pub engine: String,
}

impl Rehearsal {
    pub fn receipt(&self, now: u64) -> Receipt {
        Receipt {
            plan_id: self.plan.id.clone(),
            core_version: obop_types::CORE_VERSION.into(),
            passed: self.passed,
            at: now,
            backup_id: self.backup_id.clone(),
            longest_step_ms: self.longest_step_ms,
            total_ms: self.total_ms,
            database_id: self.database_id.clone(),
            engine: self.engine.clone(),
            records: self.records_before as u64,
        }
    }

    pub fn render(&self) -> String {
        let mut s = format!(
            "rehearsal of plan {} (schema {} to {}) on a copy of {} in {}:\n",
            self.plan.id, self.plan.from, self.plan.to, self.backup_id, self.engine
        );
        for (v, name, ms) in &self.steps {
            s.push_str(&format!("  step {v}: {name}: {ms} ms\n"));
        }
        s.push_str(&format!(
            "  {} records before, {} after; total {} ms; longest single transaction {} ms\n",
            self.records_before, self.records_after, self.total_ms, self.longest_step_ms
        ));
        for c in &self.checks {
            s.push_str(&format!(
                "  {:>5}  {:<18} {}\n",
                c.level.as_str(),
                c.name,
                c.detail
            ));
        }
        s.push_str(if self.passed {
            "REHEARSAL PASSED\n"
        } else {
            "REHEARSAL FAILED\n"
        });
        s
    }
}

/// Restore `backup` into a scratch database, run the plan there, run the checks of a drill and a
/// fixed read and write set, and report the time, the longest single transaction and the records
/// touched. Never touches the database the backup came from.
pub fn rehearse(o: &RehearseOptions) -> Result<Rehearsal, String> {
    let backup_id = drill::data_name(&o.backup);
    let verified = backup::verify_with(&o.backup, &o.keys)?;
    let database_id = verified.manifest.database_id.clone();
    let scratch = o.scratch.open(o.avoid.as_deref())?;
    let engine = scratch.backend.kind().to_string();
    let latest = obop_migrate::steps().len() as u64;
    backup::restore_sealed(
        &scratch.backend,
        None,
        latest,
        &o.backup,
        backup::RestoreOptions::default(),
        &o.keys,
        &mut |_| {},
    )?;
    let m = match &o.steps {
        Some(s) => Migrator::with_steps(scratch.backend.clone(), o.ring.clone(), s.clone()),
        None => Migrator::new(scratch.backend.clone(), o.ring.clone()),
    }
    .by("rehearsal");
    let plan = m.plan()?;
    let before = scratch.backend.total()?;
    let mut checks = Vec::new();
    let started = Instant::now();
    let run = m.run_with(&RunOptions {
        window: true,
        ..RunOptions::default()
    });
    let total_ms = started.elapsed().as_millis() as u64;
    match &run {
        Ok(r) => add(
            &mut checks,
            "run",
            Level::Pass,
            format!("{} step(s) ran on the copy", r.applied.len()),
        ),
        Err(e) => add(&mut checks, "run", Level::Fail, e.clone()),
    }
    // Per-step times come from the ledger the run just wrote.
    let ledger = m.ledger()?;
    let steps: Vec<(u32, String, u64)> = plan
        .steps
        .iter()
        .filter_map(|s| {
            ledger
                .iter()
                .find(|r| r.id == obop_migrate::ledger::core_id(s.version) && r.by == "rehearsal")
                .map(|r| (s.version, s.name.clone(), r.duration_ms))
        })
        .collect();
    let longest = steps.iter().map(|s| s.2).max().unwrap_or(0);
    // A statement that locks a table for longer than a second fails the gate; a data step is
    // reported but is one transaction on row locks.
    let slow: Vec<String> = plan
        .steps
        .iter()
        .filter(|s| s.ddl.holds_a_long_lock())
        .filter_map(|s| {
            steps
                .iter()
                .find(|t| t.0 == s.version && t.2 > MAX_LOCK_MS)
                .map(|t| format!("step {} held its lock for {} ms", t.0, t.2))
        })
        .collect();
    add(
        &mut checks,
        "lock_time",
        if slow.is_empty() {
            Level::Pass
        } else {
            Level::Fail
        },
        if slow.is_empty() {
            format!(
                "no table-locking step took longer than {MAX_LOCK_MS} ms (longest transaction {longest} ms)"
            )
        } else {
            slow.join("; ")
        },
    );
    let vr = m.verify()?;
    add(
        &mut checks,
        "ledger",
        if vr.ok() { Level::Pass } else { Level::Fail },
        if vr.ok() {
            "clean after the run".to_string()
        } else {
            vr.problems.join("; ")
        },
    );
    let models: Vec<String> = drill::counts(&*scratch.backend)?.keys().cloned().collect();
    match drill::smoke(&scratch.backend, &models) {
        Ok(s) => add(&mut checks, "smoke", Level::Pass, s),
        Err(e) => add(&mut checks, "smoke", Level::Fail, e),
    }
    if o.ring.current().is_some() {
        match obop_keys::reseal::run(
            &scratch.backend,
            &o.ring,
            obop_keys::reseal::Options {
                dry_run: true,
                skip_unopenable: true,
                ..Default::default()
            },
            &mut |_| {},
        ) {
            Ok(r) if r.unopenable.is_empty() => add(
                &mut checks,
                "secrets_open",
                Level::Pass,
                format!("{} sealed value(s) open", r.scanned),
            ),
            Ok(r) => add(
                &mut checks,
                "secrets_open",
                Level::Fail,
                format!("{} sealed value(s) cannot be opened", r.unopenable.len()),
            ),
            Err(e) => add(&mut checks, "secrets_open", Level::Fail, e),
        }
    }
    let after = scratch.backend.total()?;
    let passed = checks.iter().all(|c| c.level != Level::Fail);
    Ok(Rehearsal {
        plan,
        steps,
        longest_step_ms: longest,
        total_ms,
        records_before: before,
        records_after: after,
        checks,
        passed,
        backup_id,
        database_id,
        engine,
    })
}

// ---- rollback ------------------------------------------------------------------------------

/// What a rollback did.
#[derive(Debug, Clone)]
pub struct RolledBack {
    pub backup_id: String,
    pub schema_before: Option<u32>,
    pub schema_after: Option<u32>,
    pub records: usize,
    pub discarded_rows: usize,
    /// The backup was opened with its key and authenticated; a plain one is not.
    pub authenticated: bool,
    /// The backup carried no database id, so it could not be checked to be this database's.
    pub unnamed: bool,
}

/// Roll back by restore: verify the backup, replace the database with it in one transaction, and
/// record why. The reason is required and kept in the ledger of the restored database together
/// with what the discarded database's ledger said, and in the security log: both rows are written
/// in the same transaction as the restore, so the restored database never exists without them.
/// The migration lock is held throughout, so no `migrate run` and no starting server migrates
/// beside it (stop every process first all the same: a server that is running keeps its
/// connections). The older obop binary is the one to start afterwards if the backup is from an
/// older schema.
///
/// A backup of another database (its id differs from this database's) is refused unless
/// `other_database` says it is meant. A backup from before databases had ids restores, and says
/// it could not be checked; the database then keeps the id it had.
#[allow(clippy::too_many_arguments)]
pub fn rollback(
    backend: &Arc<dyn Backend>,
    blobs: Option<&dyn Blobs>,
    ring: &KeyRing,
    backup_file: &Path,
    keys: &Keys,
    reason: &str,
    actor: &str,
    other_database: bool,
    clock: &dyn Clock,
) -> Result<RolledBack, String> {
    let reason = reason.trim();
    if reason.is_empty() || reason.len() > 500 || reason.chars().any(char::is_control) {
        return Err("a rollback needs a reason of 1 to 500 characters, on one line".into());
    }
    let latest = obop_migrate::steps().len() as u64;
    // Nobody migrates while the database is being replaced.
    let _lock = backend.lock("obop.migrate")?;
    // Everything that can fail without writing happens first (restore does the same).
    let v = backup::verify_with(backup_file, keys)?;
    let before = Migrator::new(backend.clone(), ring.clone());
    let target_id = before.instance_id().ok().flatten();
    match (&v.manifest.database_id, &target_id) {
        (Some(b), Some(t)) if b != t && !other_database => {
            return Err(format!(
                "the backup is of another database (id {}), this database is {}. A rollback replaces this database with its own earlier state; pass --other-database if you mean to replace it with a copy of another",
                &b[..b.len().min(8)],
                &t[..t.len().min(8)]
            ));
        }
        _ => {}
    }
    let schema_before = before.status().ok().and_then(|s| s.stored);
    let created = backup::parse_rfc3339(&v.manifest.created_at).unwrap_or(0);
    let discarded: Vec<obop_migrate::Entry> = before
        .ledger()
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.at >= created)
        .collect();
    let backup_id = drill::data_name(backup_file);
    let schema_after = u32::try_from(v.manifest.schema_version)
        .ok()
        .filter(|s| *s > 0);
    let unnamed = v.manifest.database_id.is_none();
    // The rows that go in with the restore.
    let migrator = Migrator::new(backend.clone(), ring.clone()).by(actor);
    let (row_id, row) =
        migrator.rollback_row(reason, &backup_id, schema_before, schema_after, &discarded);
    let detail = format!(
        "restored {backup_id} (schema {} to {}); reason: {reason}",
        schema_before.map_or("?".into(), |v| v.to_string()),
        schema_after.map_or("?".into(), |v| v.to_string())
    );
    let (event_id, event) =
        crate::audit_row(clock.now(), actor, "migration_rollback", &detail);
    let mut extra = vec![
        (obop_migrate::ledger::MODEL.to_string(), row_id, row),
        ("security_event".to_string(), event_id, event),
    ];
    if unnamed && let Some(t) = &target_id {
        // The backup predates ids: the database keeps its name, so that its other backups still
        // belong to it.
        let mut f = obop_storage::Fields::new();
        f.insert("id".into(), J::from(t.as_str()));
        f.insert("created_at".into(), J::from(clock.now()));
        extra.push((
            obop_migrate::META_MODEL.to_string(),
            obop_migrate::INSTANCE_ID.to_string(),
            f,
        ));
    }
    let r = backup::restore_with_rows(
        backend,
        blobs,
        latest,
        backup_file,
        backup::RestoreOptions {
            force: true,
            dry_run: false,
        },
        keys,
        extra,
        &mut |_| {},
    )?;
    Ok(RolledBack {
        backup_id,
        schema_before,
        schema_after,
        records: r.records,
        discarded_rows: discarded.len(),
        authenticated: r.authenticated,
        unnamed,
    })
}

#[cfg(test)]
mod tests;
