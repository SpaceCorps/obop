//! The restore drill (OBR-DAT-049, D73): prove that a backup can be restored, and that what comes
//! back is the database that was backed up.
//!
//! A drill restores a backup into a **scratch** database (a temporary SQLite file, or an empty
//! PostgreSQL database the caller made) and runs checks on the copy:
//!
//! 1. the backup verifies (for a sealed one: authenticated with its key);
//! 2. it restores;
//! 3. the records per model equal the manifest;
//! 4. a digest over every restored record, in order, equals the backup's content digest (so the
//!    copy is not merely the same size, it holds the same bytes of JSON);
//! 5. the schema version is one this build serves (or, with `migrate`, the pending steps run on
//!    the copy and succeed: the drill of an upgrade);
//! 6. the migration ledger of the copy is clean (no drift, nothing unfinished);
//! 7. the accounts came back;
//! 8. the sealed secrets open with the keys given;
//! 9. uploaded files came back and hash to their ids;
//! 10. a fixed small write and read set works on the copy.
//!
//! The result is a [`DrillRecord`] with the **measured** recovery point (the age of the backup)
//! and recovery time (how long restore and checks took), kept in the production database so that
//! `obop storage check` can say how long ago a drill last passed. Those numbers are what a drill
//! measured on its machine with its data; they are not promises (docs/RUNBOOK-RECOVERY.md).
//!
//! A check that cannot run says so (`ok` with a "not checked" detail) rather than passing
//! silently. The server check ("start `obop serve` on the copy and ask it a question") needs a
//! process and stays in `deploy/restore-drill.sh`.

use crate::backup::{self, Keys, RestoreOptions};
use crate::catalog::{DrillCheck, DrillRecord};
use obop_keys::KeyRing;
use obop_keys::reseal;
use obop_migrate::Migrator;
use obop_storage::Backend;
use obop_storage::blob::Blobs;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Where a drill (or a rehearsal) restores to.
#[derive(Debug, Clone)]
pub enum Scratch {
    /// A temporary SQLite file, removed afterwards.
    TempSqlite,
    /// An **empty** database the caller made (`postgres://...` or `sqlite://...`). It is emptied
    /// again afterwards unless `keep` is set.
    Url { url: String, keep: bool },
}

/// An open scratch database; temporary files go away with it.
pub struct ScratchDb {
    pub backend: Arc<dyn Backend>,
    pub url: String,
    /// The private directory of a temporary scratch database (removed with everything in it).
    temp: Option<PathBuf>,
    clear_on_drop: bool,
    /// The keys of the byte store of uploaded files before the restore, so that afterwards only
    /// what the restore wrote is removed from a database the caller lent.
    files_before: Option<std::collections::BTreeSet<String>>,
}

impl ScratchDb {
    /// Note which uploaded files the scratch database holds now, before a restore writes more.
    fn note_files(&mut self) {
        if self.clear_on_drop {
            self.files_before = obop_storage::blob::open_bytes(&self.url, None)
                .and_then(|b| b.keys())
                .ok()
                .map(|k| k.into_iter().collect());
        }
    }

    /// Remove the uploaded files the restore wrote: they are not records, so emptying the
    /// records leaves them. Returns what could not be removed.
    fn forget_files(&self) -> Vec<String> {
        let (Some(before), Ok(bytes)) = (
            &self.files_before,
            obop_storage::blob::open_bytes(&self.url, None),
        ) else {
            return Vec::new();
        };
        let mut left = Vec::new();
        for k in bytes.keys().unwrap_or_default() {
            if !before.contains(&k) && bytes.delete(&k).is_err() {
                left.push(k);
            }
        }
        left
    }
}

impl Drop for ScratchDb {
    fn drop(&mut self) {
        if let Some(p) = &self.temp {
            // Let go of the open file first (Windows will not remove a file that is open).
            self.backend = obop_storage::open("memory:").expect("a memory database always opens");
            let _ = std::fs::remove_dir_all(p);
        } else if self.clear_on_drop {
            // Leave nothing of the restored data behind in a database the caller lent us: the
            // records, and the uploaded files the restore wrote beside them.
            let left = self.forget_files();
            if !left.is_empty() {
                eprintln!(
                    "warning: {} uploaded file(s) of the drilled backup could not be removed from the scratch database {}",
                    left.len(),
                    self.url
                );
            }
            if let Err(e) = self.backend.import(true, &mut || Ok(None)) {
                eprintln!(
                    "warning: the scratch database {} could not be emptied again: {e}",
                    self.url
                );
            }
            if let Some(dir) = self.url.strip_prefix("sqlite://") {
                // The directory of a file store that held only what the restore wrote.
                let _ = std::fs::remove_dir(format!("{dir}.blobs"));
            }
        }
    }
}

/// Temporary scratch directories of runs that were killed, older than this, are removed.
const STALE_SCRATCH: std::time::Duration = std::time::Duration::from_secs(6 * 3600);

fn sweep_scratch(tmp: &Path) {
    let Ok(rd) = std::fs::read_dir(tmp) else {
        return;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !name.starts_with("obop-scratch-") {
            continue;
        }
        let idle = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| std::time::SystemTime::now().duration_since(t).ok());
        if idle.is_some_and(|i| i >= STALE_SCRATCH) {
            let _ = std::fs::remove_dir_all(e.path());
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Compare two database URLs by host and database, ignoring user, password and parameters.
fn same_database(a: &str, b: &str) -> bool {
    let key = |u: &str| {
        let rest = u.split_once("://").map_or(u, |(_, r)| r);
        let rest = rest.rsplit_once('@').map_or(rest, |(_, r)| r);
        rest.split('?')
            .next()
            .unwrap_or("")
            .trim_end_matches('/')
            .to_ascii_lowercase()
    };
    key(a) == key(b)
}

impl Scratch {
    /// Open the scratch database. `avoid` is the production URL: restoring into it is refused,
    /// because a drill that overwrites production is worse than no drill.
    pub fn open(&self, avoid: Option<&str>) -> Result<ScratchDb, String> {
        static N: AtomicU64 = AtomicU64::new(0);
        match self {
            Scratch::TempSqlite => {
                // A restored sealed backup is in the clear in this database: it lives in a
                // directory of its own, readable by the owner only, under a name nobody can
                // guess (so nobody can plant a file there), and goes away with the drill.
                let tmp = std::env::temp_dir();
                sweep_scratch(&tmp);
                let dir = tmp.join(format!(
                    "obop-scratch-{}-{}-{}",
                    std::process::id(),
                    N.fetch_add(1, Ordering::SeqCst),
                    obop_keys::hex(&obop_keys::random::<8>())
                ));
                crate::fsutil::create_dir_private(&dir).map_err(|e| {
                    format!("cannot make the scratch directory {}: {e}", dir.display())
                })?;
                let url = format!("sqlite://{}", dir.join("scratch.db").display());
                let backend = match obop_storage::open(&url) {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = std::fs::remove_dir_all(&dir);
                        return Err(e);
                    }
                };
                Ok(ScratchDb {
                    backend,
                    url,
                    temp: Some(dir),
                    clear_on_drop: false,
                    files_before: None,
                })
            }
            Scratch::Url { url, keep } => {
                if !(url.starts_with("postgres://")
                    || url.starts_with("postgresql://")
                    || url.starts_with("sqlite://"))
                {
                    return Err(
                        "the scratch database must be a postgres:// or sqlite:// URL".into(),
                    );
                }
                if avoid.is_some_and(|a| same_database(a, url)) {
                    return Err(
                        "the scratch database is the production database: a drill restores into a copy, never into the database it protects"
                            .into(),
                    );
                }
                let backend = obop_storage::open(url)?;
                let n = backend.total()?;
                if n > 0 {
                    return Err(format!(
                        "the scratch database is not empty ({n} records): give a fresh, empty database"
                    ));
                }
                Ok(ScratchDb {
                    backend,
                    url: url.clone(),
                    temp: None,
                    clear_on_drop: !keep,
                    files_before: None,
                })
            }
        }
    }
}

pub struct Options {
    pub backup: PathBuf,
    pub keys: Keys,
    /// The server key ring: opens the sealed secrets of the copy.
    pub ring: KeyRing,
    pub scratch: Scratch,
    /// The production URL, to refuse restoring into it.
    pub avoid: Option<String>,
    pub plane: String,
    /// Seconds since the epoch now (injected so tests are exact).
    pub now: u64,
    /// Run pending migration steps on the copy (the drill of an upgrade).
    pub migrate: bool,
}

#[derive(Debug, Clone)]
pub struct Report {
    pub record: DrillRecord,
}

impl Report {
    pub fn passed(&self) -> bool {
        self.record.passed
    }

    /// The stable code (`restore_key_required`, `backup_not_sealed`) of the failure that stopped
    /// the drill, if it was one of those: a caller maps it to the exit status of `verify` and
    /// `restore`.
    pub fn error_code(&self) -> Option<&'static str> {
        self.record
            .checks
            .iter()
            .filter(|c| !c.ok)
            .find_map(|c| backup::error_code(&c.detail))
    }

    pub fn render(&self) -> String {
        let r = &self.record;
        let mut s = String::new();
        for c in &r.checks {
            s.push_str(&format!(
                "{:>5}  {:<20} {}\n",
                if c.ok { "pass" } else { "FAIL" },
                c.name,
                c.detail
            ));
        }
        s.push_str(&format!(
            "backup {} ({} records): measured recovery point {}, recovery time {}: {}\n",
            r.backup_id,
            r.records,
            crate::doctor::human(r.rpo_seconds),
            crate::doctor::human(r.rto_seconds),
            if r.passed {
                "DRILL PASSED"
            } else {
                "DRILL FAILED"
            }
        ));
        s
    }
}

/// The digest `backup` computes over the record lines, taken over a live database in insertion
/// order.
pub fn content_digest(backend: &dyn Backend) -> Result<String, String> {
    let mut h = Sha256::new();
    backend.export(1000, &mut |batch| {
        for (m, i, d) in batch {
            let line = format!("{}\n", serde_json::json!({"m": m, "i": i, "d": d}));
            h.update(line.as_bytes());
        }
        Ok(())
    })?;
    Ok(obop_keys::hex(&h.finalize()))
}

/// Record counts per model, from the database.
pub fn counts(backend: &dyn Backend) -> Result<std::collections::BTreeMap<String, usize>, String> {
    let mut m = std::collections::BTreeMap::new();
    backend.export(1000, &mut |batch| {
        for (model, _, _) in batch {
            *m.entry(model.clone()).or_default() += 1;
        }
        Ok(())
    })?;
    Ok(m)
}

struct Checks(Vec<DrillCheck>);

impl Checks {
    fn add(&mut self, name: &str, ok: bool, detail: impl Into<String>) {
        self.0.push(DrillCheck {
            name: name.into(),
            ok,
            detail: detail.into(),
        });
    }
}

/// The fixed read and write set: a record is written, read, changed and removed, and every model
/// can be listed.
pub fn smoke(backend: &Arc<dyn Backend>, models: &[String]) -> Result<String, String> {
    use serde_json::json;
    let probe = |v: serde_json::Value| v.as_object().cloned().unwrap_or_default();
    let (m, id) = ("obop_drill_probe", "p1");
    backend.insert(m, id, &probe(json!({"n": 1})))?;
    if backend.get(m, id)?.map(|f| f["n"].clone()) != Some(json!(1)) {
        return Err("a record written to the copy did not read back".into());
    }
    backend.update(m, id, &probe(json!({"n": 2})))?;
    if backend.get(m, id)?.map(|f| f["n"].clone()) != Some(json!(2)) {
        return Err("a record changed on the copy did not read back".into());
    }
    backend.delete(m, id)?;
    if backend.get(m, id)?.is_some() {
        return Err("a record removed from the copy is still there".into());
    }
    let mut listed = 0;
    for model in models {
        listed += backend.count(model)?;
    }
    Ok(format!(
        "wrote, changed and removed a record; counted {listed} records in {} models",
        models.len()
    ))
}

/// Run a drill. `Err` only when it could not start (no scratch database, no manifest); every
/// failed check, including a backup that does not verify, is a failed drill with a record.
pub fn run(o: &Options) -> Result<Report, String> {
    let manifest = backup::manifest_of(&o.backup)?;
    let mut c = Checks(Vec::new());
    let mut scratch = o.scratch.open(o.avoid.as_deref())?;
    scratch.note_files();
    let started = Instant::now();
    let finish = |c: Checks, started: Instant, records: u64| {
        let passed = c.0.iter().all(|x| x.ok);
        Report {
            record: DrillRecord {
                id: format!(
                    "drill-{:012}-{}",
                    o.now,
                    obop_keys::hex(&obop_keys::random::<3>())
                ),
                at: o.now,
                plane: o.plane.clone(),
                backup_id: o.backup.file_name().map_or(String::new(), |n| {
                    n.to_string_lossy()
                        .trim_end_matches(".manifest.json")
                        .to_string()
                }),
                rpo_seconds: o
                    .now
                    .saturating_sub(backup::parse_rfc3339(&manifest.created_at).unwrap_or(o.now)),
                rto_seconds: started.elapsed().as_secs(),
                records,
                checks: c.0,
                passed,
            },
        }
    };
    // 1. The backup itself.
    let v = match backup::verify_with(&o.backup, &o.keys) {
        Ok(v) => {
            c.add(
                "backup_verifies",
                true,
                if v.authenticated {
                    format!(
                        "format {}, every chunk and the manifest authenticated",
                        v.manifest.format
                    )
                } else {
                    format!(
                        "format {} (plain: checksums only, not authenticated)",
                        v.manifest.format
                    )
                },
            );
            v
        }
        Err(e) => {
            c.add("backup_verifies", false, e);
            return Ok(finish(c, started, 0));
        }
    };
    let m = v.manifest;
    // 2. Restore.
    let blobs = scratch_blobs(&scratch)?;
    let latest = obop_migrate::steps().len() as u64;
    let restore_started = Instant::now();
    let restored = backup::restore_sealed(
        &scratch.backend,
        Some(&blobs),
        latest,
        &o.backup,
        RestoreOptions::default(),
        &o.keys,
        &mut |_| {},
    );
    let r = match restored {
        Ok(r) => {
            c.add(
                "restore",
                true,
                format!(
                    "{} records in {} ms",
                    r.records,
                    restore_started.elapsed().as_millis()
                ),
            );
            r
        }
        Err(e) => {
            c.add("restore", false, e);
            return Ok(finish(c, started, 0));
        }
    };
    // 3 and 4. Counts and content digest.
    let have = counts(&*scratch.backend)?;
    if have == m.models {
        c.add(
            "record_counts",
            true,
            format!(
                "{} records in {} models equal the manifest",
                m.records,
                have.len()
            ),
        );
    } else {
        let diff: Vec<String> = m
            .models
            .keys()
            .chain(have.keys())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .filter(|k| m.models.get(*k) != have.get(*k))
            .map(|k| {
                format!(
                    "{k}: backup {} copy {}",
                    m.models.get(k).unwrap_or(&0),
                    have.get(k).unwrap_or(&0)
                )
            })
            .collect();
        c.add("record_counts", false, diff.join("; "));
    }
    let digest = content_digest(&*scratch.backend)?;
    match (&m.records_sha256, m.blobs) {
        (Some(want), _) => c.add(
            "content_digest",
            &digest == want,
            if &digest == want {
                format!("sha256 {}... of every record equals the backup's", &digest[..12])
            } else {
                "the restored records do not hash to the backup's record digest".to_string()
            },
        ),
        // A backup from before the record digest existed: its only digest covers the files too.
        (None, 0) => c.add(
            "content_digest",
            digest == m.content_sha256,
            if digest == m.content_sha256 {
                format!("sha256 {}... of every record equals the backup's", &digest[..12])
            } else {
                "the restored records do not hash to the backup's content digest".to_string()
            },
        ),
        (None, _) => c.add(
            "content_digest",
            true,
            "not checked: this backup predates the record digest and holds files, which its only digest also covers",
        ),
    }
    // 5. Schema.
    let migrator = Migrator::new(scratch.backend.clone(), o.ring.clone()).by("drill");
    let st = migrator.status()?;
    if st.ahead {
        c.add(
            "schema_version",
            false,
            "the backup is newer than this obop",
        );
    } else if !st.pending.is_empty() && !o.migrate {
        c.add(
            "schema_version",
            false,
            format!("the backup is from schema {}, this build writes {}: drill again with --migrate to drill the upgrade", st.stored.unwrap_or(0), st.latest),
        );
    } else if !st.pending.is_empty() {
        match migrator.run(false) {
            Ok(rep) => c.add(
                "schema_version",
                true,
                format!("migrated the copy: {} step(s) ran", rep.applied.len()),
            ),
            Err(e) => c.add(
                "schema_version",
                false,
                format!("migrating the copy failed: {e}"),
            ),
        }
    } else {
        c.add(
            "schema_version",
            true,
            format!("schema version {}", st.stored.unwrap_or(st.latest)),
        );
    }
    // 6. Ledger.
    match migrator.verify() {
        Ok(vr) if vr.ok() => c.add(
            "migration_ledger",
            true,
            format!("{} row(s), no drift", migrator.ledger()?.len()),
        ),
        Ok(vr) => c.add("migration_ledger", false, vr.problems.join("; ")),
        Err(e) => c.add("migration_ledger", false, e),
    }
    // 7. Accounts.
    let in_backup = m.models.get("auth_user").copied().unwrap_or(0);
    let on_copy = scratch.backend.count("auth_user")?;
    if in_backup == 0 {
        c.add(
            "accounts",
            true,
            "not checked: the backup holds no accounts",
        );
    } else {
        c.add(
            "accounts",
            on_copy == in_backup,
            format!("{on_copy} of {in_backup} account(s) came back"),
        );
    }
    // 8. Sealed secrets.
    if o.ring.current().is_none() {
        c.add(
            "secrets_open",
            true,
            "not checked: no OBOP_SECRET_KEY given, so sealed secrets could not be opened",
        );
    } else {
        match reseal::run(
            &scratch.backend,
            &o.ring,
            reseal::Options {
                dry_run: true,
                skip_unopenable: true,
                ..reseal::Options::default()
            },
            &mut |_| {},
        ) {
            Ok(rep) if rep.unopenable.is_empty() => c.add(
                "secrets_open",
                true,
                format!("{} sealed value(s) open with the keys given", rep.scanned),
            ),
            Ok(rep) => c.add(
                "secrets_open",
                false,
                format!(
                    "{} sealed value(s) cannot be opened, first: {}",
                    rep.unopenable.len(),
                    rep.unopenable[0].place
                ),
            ),
            Err(e) => c.add("secrets_open", false, e),
        }
    }
    // 9. Uploaded files.
    if m.blobs == 0 {
        c.add(
            "files",
            true,
            "not checked: the backup holds no uploaded files",
        );
    } else {
        let mut n = 0usize;
        let mut bad = Vec::new();
        match blobs.export() {
            Ok(it) => {
                for b in it {
                    match b {
                        Ok(b) => {
                            n += 1;
                            if obop_keys::hex(&Sha256::digest(&b.bytes)) != b.info.id {
                                bad.push(b.info.id);
                            }
                        }
                        Err(e) => bad.push(e),
                    }
                }
                c.add(
                    "files",
                    bad.is_empty() && n == m.blobs,
                    format!(
                        "{n} of {} uploaded file(s) came back and hash to their ids{}",
                        m.blobs,
                        if bad.is_empty() {
                            String::new()
                        } else {
                            format!("; damaged: {}", bad.join(", "))
                        }
                    ),
                );
            }
            Err(e) => c.add("files", false, e),
        }
    }
    // 10. Smoke.
    let models: Vec<String> = have.keys().cloned().collect();
    match smoke(&scratch.backend, &models) {
        Ok(s) => c.add("smoke", true, s),
        Err(e) => c.add("smoke", false, e),
    }
    Ok(finish(c, started, r.records as u64))
}

/// The store of uploaded files for a scratch database: next to a SQLite file, in the database
/// for PostgreSQL.
fn scratch_blobs(s: &ScratchDb) -> Result<obop_storage::blob::Catalog, String> {
    let bytes = obop_storage::blob::open_bytes(&s.url, None)?;
    Ok(obop_storage::blob::Catalog::new(s.backend.clone(), bytes))
}

/// The path of a data file for a given manifest path (helper for the command).
pub fn data_name(p: &Path) -> String {
    p.file_name()
        .map(|n| {
            n.to_string_lossy()
                .trim_end_matches(".manifest.json")
                .to_string()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
