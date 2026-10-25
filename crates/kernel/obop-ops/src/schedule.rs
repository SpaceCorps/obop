//! Scheduled backups with an off-host copy and retention (OBR-DAT-047, D73).
//!
//! [`take`] is the whole job once: write a backup (sealed when a key is configured), read it back
//! and authenticate it, record it in the catalogue, copy it to every sink and prove each copy by
//! reading it back, then apply the retention policy. It is the same code whether a person runs
//! `obop backup run` from cron, or the server's own timer calls [`Schedule::tick`].
//!
//! The timer is a process-level hook, not a job runner (that is step 5 of the plan): `obop serve`
//! starts one thread that calls `tick` every minute. `tick` asks the catalogue when the last
//! verified scheduled backup was taken and does nothing until a full interval has passed; the
//! decision therefore survives restarts and is the same on every process, and a lease in the
//! shared store (`backup:lease`) makes sure only one process takes the backup when several are
//! running. A failure is kept in the store (`backup:last_failure`) for the status commands and
//! the metrics, and tried again at the next minute.
//!
//! Retention applies to scheduled backups only. A backup taken by hand, before a migration or
//! before an update is a restore point somebody asked for and stays until a person removes it.
//! A scheduled backup is never deleted from this host before every configured sink holds a
//! verified copy of it.

use crate::backup::{self, Keys};
use crate::catalog::{BackupRecord, Catalog, Copy};
use crate::retention::{self, Retention};
use crate::sink::{self, BackupSink};
use obop_storage::Backend;
use obop_storage::blob::Blobs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub trait Clock: Send + Sync {
    /// Seconds since the Unix epoch.
    fn now(&self) -> u64;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    }
}

/// A clock a test moves by hand.
#[derive(Default)]
pub struct FakeClock(AtomicU64);

impl FakeClock {
    pub fn at(t: u64) -> FakeClock {
        FakeClock(AtomicU64::new(t))
    }
    pub fn advance(&self, secs: u64) {
        self.0.fetch_add(secs, Ordering::SeqCst);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

pub const LEASE_KEY: &str = "backup:lease";
pub const FAILURE_KEY: &str = "backup:last_failure";
/// A running job holds the lease this long at most; a crashed one lets go by itself.
const LEASE_MS: u64 = 3 * 3600 * 1000;

pub struct Settings {
    /// Where backups are written on this host.
    pub dir: PathBuf,
    /// Where each is copied; a copy counts only when it reads back.
    pub sinks: Vec<Arc<dyn BackupSink>>,
    /// How long after the last verified scheduled backup the next is due.
    pub every: Duration,
    pub retention: Retention,
    pub keys: Keys,
}

impl Settings {
    pub fn new(dir: impl Into<PathBuf>, keys: Keys) -> Settings {
        Settings {
            dir: dir.into(),
            sinks: Vec::new(),
            every: Duration::from_secs(24 * 3600),
            retention: Retention::default(),
            keys,
        }
    }
}

/// What one run did.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub backup: BackupRecord,
    /// `(sink, error)` for each sink that did not get a verified copy.
    pub copy_failures: Vec<(String, String)>,
    /// Backups removed from this host by the retention policy.
    pub pruned: Vec<String>,
    /// Things worth saying: a sink that refused deletion, an unencrypted backup.
    pub notes: Vec<String>,
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Take one backup now, as `reason`, with everything that follows from it.
pub fn take(
    backend: &Arc<dyn Backend>,
    blobs: Option<&dyn Blobs>,
    schema_version: u64,
    settings: &Settings,
    reason: &str,
    clock: &dyn Clock,
) -> Result<Outcome, String> {
    let started = Instant::now();
    // The name carries the clock's time, so a simulated clock gives distinct, ordered names.
    let target = settings
        .dir
        .join(backup::default_name(clock.now(), settings.keys.can_seal()));
    let c = backup::create_at(
        backend,
        blobs,
        schema_version,
        &target,
        &settings.keys,
        clock.now(),
        &mut |_| {},
    )?;
    let v = backup::verify_with(&c.data_path, &settings.keys)
        .map_err(|e| format!("the backup was written but does not verify: {e}"))?;
    let id = file_name(&c.data_path);
    let mut rec = BackupRecord {
        id: id.clone(),
        path: c.data_path.display().to_string(),
        created_at: clock.now(),
        format: c.manifest.format,
        key_id: c.manifest.encryption.as_ref().map(|e| e.key_id.clone()),
        bytes: c.manifest.data_bytes,
        records: c.manifest.records as u64,
        schema_version,
        duration_ms: started.elapsed().as_millis() as u64,
        verified: true,
        authenticated: v.authenticated,
        reason: reason.to_string(),
        copies: Vec::new(),
    };
    let catalog = Catalog::new(&**backend);
    catalog.record_backup(&rec)?;
    let mut out = Outcome {
        backup: rec.clone(),
        copy_failures: Vec::new(),
        pruned: Vec::new(),
        notes: Vec::new(),
    };
    if c.manifest.format == backup::FORMAT {
        out.notes
            .push("the backup is not encrypted: set OBOP_BACKUP_KEY (or OBOP_SECRET_KEY)".into());
    }
    for s in &settings.sinks {
        let copied = sink::copy_verified(&**s, &id, &c.data_path).and_then(|_| {
            sink::copy_verified(&**s, &file_name(&c.manifest_path), &c.manifest_path)
        });
        match copied {
            Ok(_) => rec.copies.push(Copy {
                sink: s.name(),
                name: id.clone(),
                verified_at: clock.now(),
            }),
            Err(e) => out.copy_failures.push((s.name(), e)),
        }
    }
    catalog.record_backup(&rec)?;
    out.backup = rec;
    if reason == "scheduled" {
        let (pruned, notes) = prune(backend, settings, clock)?;
        out.pruned = pruned;
        out.notes.extend(notes);
    }
    Ok(out)
}

/// Apply the retention policy to the scheduled backups of the catalogue. Returns the ids removed
/// from this host and notes. A backup without a verified copy in every sink stays.
pub fn prune(
    backend: &Arc<dyn Backend>,
    settings: &Settings,
    _clock: &dyn Clock,
) -> Result<(Vec<String>, Vec<String>), String> {
    let catalog = Catalog::new(&**backend);
    let scheduled: Vec<BackupRecord> = catalog
        .backups()?
        .into_iter()
        .filter(|b| b.reason == "scheduled")
        .collect();
    let list: Vec<(String, u64)> = scheduled
        .iter()
        .map(|b| (b.id.clone(), b.created_at))
        .collect();
    let d = retention::decide(settings.retention, &list);
    let (mut pruned, mut notes) = (Vec::new(), Vec::new());
    for id in &d.delete {
        let Some(b) = scheduled.iter().find(|b| &b.id == id) else {
            continue;
        };
        let safe = settings
            .sinks
            .iter()
            .all(|s| b.copies.iter().any(|c| c.sink == s.name()));
        if !safe {
            notes.push(format!(
                "{id} is past retention but has no verified copy in every sink, so it is kept"
            ));
            continue;
        }
        for f in [b.path.clone(), format!("{}.manifest.json", b.path)] {
            match std::fs::remove_file(&f) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    notes.push(format!("cannot remove {f}: {e}"));
                }
            }
        }
        catalog.forget_backup(id)?;
        pruned.push(id.clone());
        for s in &settings.sinks {
            for name in [id.clone(), format!("{id}.manifest.json")] {
                if let Err(e) = s.delete(&name) {
                    let n = format!("{}: {e}", s.name());
                    if !notes.contains(&n) {
                        notes.push(n);
                    }
                }
            }
        }
    }
    Ok((pruned, notes))
}

/// The process-level timer's decision and action.
pub struct Schedule {
    pub settings: Settings,
    pub clock: Arc<dyn Clock>,
}

impl Schedule {
    pub fn new(settings: Settings, clock: Arc<dyn Clock>) -> Schedule {
        Schedule { settings, clock }
    }

    /// Whether a scheduled backup is due: the newest verified one is older than the interval, or
    /// there is none.
    pub fn due(&self, backend: &Arc<dyn Backend>) -> Result<bool, String> {
        let last = Catalog::new(&**backend)
            .backups()?
            .into_iter()
            .rfind(|b| b.verified && b.reason == "scheduled");
        Ok(match last {
            None => true,
            Some(b) => {
                self.clock.now().saturating_sub(b.created_at) >= self.settings.every.as_secs()
            }
        })
    }

    /// Take the backup if one is due and nobody else is taking it. `None` when nothing was done.
    pub fn tick(
        &self,
        backend: &Arc<dyn Backend>,
        blobs: Option<&dyn Blobs>,
        schema_version: u64,
    ) -> Result<Option<Outcome>, String> {
        if !self.due(backend)? {
            return Ok(None);
        }
        let holder = format!("{}:{}", std::process::id(), self.clock.now());
        if !backend.kv_put_new(LEASE_KEY, &holder, Some(LEASE_MS))? {
            return Ok(None);
        }
        let result = (|| {
            // Another process may have finished while the lease was being taken.
            if !self.due(backend)? {
                return Ok(None);
            }
            take(
                backend,
                blobs,
                schema_version,
                &self.settings,
                "scheduled",
                &*self.clock,
            )
            .map(Some)
        })();
        let _ = backend.kv_delete(LEASE_KEY);
        match &result {
            Ok(Some(o)) if o.copy_failures.is_empty() => {
                let _ = backend.kv_delete(FAILURE_KEY);
            }
            Ok(Some(o)) => {
                let _ = backend.kv_put(
                    FAILURE_KEY,
                    &format!(
                        "{}: {}",
                        self.clock.now(),
                        o.copy_failures
                            .iter()
                            .map(|(s, e)| format!("{s}: {e}"))
                            .collect::<Vec<_>>()
                            .join("; ")
                    ),
                    None,
                );
            }
            Ok(None) => {}
            Err(e) => {
                let _ = backend.kv_put(FAILURE_KEY, &format!("{}: {e}", self.clock.now()), None);
            }
        }
        result
    }
}

/// A backup is overdue when the newest verified one is older than this: 26 hours for a nightly
/// job (OBR-DAT-047 item 3). The alert rule of docs/DEPLOY.md is the same number.
pub const OVERDUE_AFTER_SECONDS: u64 = 26 * 3600;

/// What the metrics and `obop storage check` report about backups.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stats {
    /// Seconds since the newest verified backup; `None` when there is none.
    pub backup_age_seconds: Option<u64>,
    pub backup_bytes: Option<u64>,
    pub backup_duration_seconds: Option<u64>,
    /// Seconds since the newest verified backup that has a verified copy off the host.
    pub copy_age_seconds: Option<u64>,
    /// Seconds since the newest restore drill that passed.
    pub drill_age_seconds: Option<u64>,
    pub last_failure: Option<String>,
}

impl Stats {
    /// No verified backup at all, or one older than [`OVERDUE_AFTER_SECONDS`].
    pub fn backup_overdue(&self) -> bool {
        self.backup_age_seconds
            .is_none_or(|a| a > OVERDUE_AFTER_SECONDS)
    }
}

pub fn stats(backend: &dyn Backend, now: u64) -> Result<Stats, String> {
    let catalog = Catalog::new(backend);
    let backups = catalog.backups()?;
    let last = backups.iter().rfind(|b| b.verified);
    let copied = backups.iter().rfind(|b| b.verified && !b.copies.is_empty());
    Ok(Stats {
        backup_age_seconds: last.map(|b| now.saturating_sub(b.created_at)),
        backup_bytes: last.map(|b| b.bytes),
        backup_duration_seconds: last.map(|b| b.duration_ms.div_ceil(1000)),
        copy_age_seconds: copied.map(|b| now.saturating_sub(b.created_at)),
        drill_age_seconds: catalog
            .latest_passed_drill()?
            .map(|d| now.saturating_sub(d.at)),
        last_failure: backend.kv_get(FAILURE_KEY)?,
    })
}

#[cfg(test)]
mod tests;
