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
//! running. A failure is kept in the store (`backup:last_failure`, written by [`take`] itself, so
//! a cron run and the timer report the same way) and the timer waits longer after each one
//! (a minute, two, four, up to an hour) instead of writing a backup a minute.
//!
//! A backup that did not get a verified copy in every sink is copied later, on the next run and,
//! once an hour, by the timer: a share that was down for a night is caught up, and a sink moved
//! to another path receives the backups it missed.
//!
//! Retention applies to scheduled backups only. A backup taken by hand, before a migration or
//! before an update is a restore point somebody asked for and stays until a person removes it.
//! A scheduled backup is never deleted from this host before every configured sink holds a
//! verified copy of it, proved again by reading the sink at the moment of deletion; a file that
//! could not be removed stays in the catalogue.

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
/// `<failures in a row>:<unix time of the last>`, for the back-off of the timer.
const FAILS_KEY: &str = "backup:failures";
/// Throttles the catch-up copy of the timer to once an hour.
const CATCHUP_KEY: &str = "backup:catch_up";
/// A running job holds the lease this long at most; a crashed one lets go by itself.
const LEASE_MS: u64 = 3 * 3600 * 1000;
/// A backup stamped later than now by more than this was written by a clock that was wrong.
pub const SKEW_TOLERANCE: u64 = 300;

/// The identity of this machine for the catalogue, if the operator gave one (`OBOP_HOST_ID`):
/// several machines can share one database, and a backup file is on the disk of the machine
/// that took it, so only that machine removes it. Without it, a record belongs to whichever
/// process has its file under its own backup directory.
pub fn this_host() -> Option<String> {
    std::env::var("OBOP_HOST_ID")
        .ok()
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty())
}

pub struct Settings {
    /// Where backups are written on this host. Empty: not known (a command that only prunes).
    pub dir: PathBuf,
    /// Where each is copied; a copy counts only when it reads back.
    pub sinks: Vec<Arc<dyn BackupSink>>,
    /// How long after the last verified scheduled backup the next is due.
    pub every: Duration,
    pub retention: Retention,
    /// A scheduled backup older than this many seconds is not kept (see `retention`).
    pub max_age: Option<u64>,
    pub keys: Keys,
    /// Prune although catalogue records name sinks this process was not given. Without it, a
    /// shell that does not have the sink configured removes nothing.
    pub ignore_recorded_sinks: bool,
    /// The identity of this machine in the catalogue (`OBOP_HOST_ID`).
    pub host: Option<String>,
}

impl Settings {
    pub fn new(dir: impl Into<PathBuf>, keys: Keys) -> Settings {
        Settings {
            dir: dir.into(),
            sinks: Vec::new(),
            every: Duration::from_secs(24 * 3600),
            retention: Retention::default(),
            max_age: Some(retention::DEFAULT_MAX_AGE_DAYS * 86_400),
            keys,
            ignore_recorded_sinks: false,
            host: this_host(),
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
    /// Things worth saying: a sink that refused deletion, an unencrypted backup, a file that
    /// could not be removed, an upload that is damaged.
    pub notes: Vec<String>,
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn manifest_name(id: &str) -> String {
    format!("{id}.manifest.json")
}

fn absolute(p: &Path) -> PathBuf {
    std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf())
}

fn remove_pair(data: &Path, manifest: &Path) {
    let _ = std::fs::remove_file(manifest);
    let _ = std::fs::remove_file(data);
}

/// Take one backup now, as `reason`, with everything that follows from it. A `scheduled` or
/// `manual` run keeps the outcome in the store (`backup:last_failure`): the metrics, the doctor
/// and `obop backup list` read it, whether the timer or cron ran the job.
pub fn take(
    backend: &Arc<dyn Backend>,
    blobs: Option<&dyn Blobs>,
    schema_version: u64,
    settings: &Settings,
    reason: &str,
    clock: &dyn Clock,
) -> Result<Outcome, String> {
    let result = take_inner(backend, blobs, schema_version, settings, reason, clock);
    if matches!(reason, "scheduled" | "manual") {
        match &result {
            Ok(o) if o.copy_failures.is_empty() => clear_failure(&**backend),
            Ok(o) => note_failure(
                &**backend,
                clock.now(),
                &o.copy_failures
                    .iter()
                    .map(|(s, e)| format!("{s}: {e}"))
                    .collect::<Vec<_>>()
                    .join("; "),
            ),
            Err(e) => note_failure(&**backend, clock.now(), e),
        }
    }
    result
}

fn note_failure(backend: &dyn Backend, now: u64, what: &str) {
    let _ = backend.kv_put(FAILURE_KEY, &format!("{now}: {what}"), None);
    let count = failures(backend).map_or(0, |(n, _)| n);
    let _ = backend.kv_put(FAILS_KEY, &format!("{}:{now}", count + 1), None);
}

fn clear_failure(backend: &dyn Backend) {
    let _ = backend.kv_delete(FAILURE_KEY);
    let _ = backend.kv_delete(FAILS_KEY);
}

/// `(failures in a row, unix time of the last)`.
fn failures(backend: &dyn Backend) -> Option<(u32, u64)> {
    let v = backend.kv_get(FAILS_KEY).ok().flatten()?;
    let (n, at) = v.split_once(':')?;
    Some((n.parse().ok()?, at.parse().ok()?))
}

/// How long the timer waits after `n` failures in a row: a minute, doubling, up to an hour.
pub fn backoff_seconds(n: u32) -> u64 {
    (60u64 << n.saturating_sub(1).min(6)).min(3600)
}

fn take_inner(
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
    // From here the files exist. Unless they end up in the catalogue as a verified backup they
    // are removed: a file nobody has recorded is never pruned and never counted.
    let v = match backup::verify_with(&c.data_path, &settings.keys) {
        Ok(v) => v,
        Err(e) => {
            remove_pair(&c.data_path, &c.manifest_path);
            return Err(format!("the backup was written but does not verify: {e}"));
        }
    };
    let id = file_name(&c.data_path);
    let mut rec = BackupRecord {
        id: id.clone(),
        path: absolute(&c.data_path).display().to_string(),
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
        host: settings.host.clone(),
    };
    let catalog = Catalog::new(&**backend);
    if let Err(e) = catalog.record_backup(&rec) {
        remove_pair(&c.data_path, &c.manifest_path);
        return Err(format!(
            "the backup could not be recorded, so it was removed again: {e}"
        ));
    }
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
    if !c.manifest.blobs_damaged.is_empty() {
        out.notes.push(format!(
            "{} uploaded file(s) no longer hash to their ids on disk and are not in this backup: {}",
            c.manifest.blobs_damaged.len(),
            c.manifest.blobs_damaged.join(", ")
        ));
    }
    for s in &settings.sinks {
        let copied = sink::copy_verified(&**s, &id, &c.data_path).and_then(|sha| {
            sink::copy_verified(&**s, &manifest_name(&id), &c.manifest_path).map(|_| sha)
        });
        match copied {
            Ok(sha256) => rec.copies.push(Copy {
                sink: s.name(),
                name: id.clone(),
                verified_at: clock.now(),
                sha256: Some(sha256),
            }),
            Err(e) => out.copy_failures.push((s.name(), e)),
        }
    }
    catalog.record_backup(&rec)?;
    out.backup = rec;
    // Backups an earlier run could not copy are copied now.
    if !settings.sinks.is_empty() {
        let (n, failed) = catch_up(backend, settings, clock)?;
        if n > 0 {
            out.notes
                .push(format!("{n} earlier backup(s) copied off the host as well"));
        }
        for (s, e) in failed {
            if !out.copy_failures.iter().any(|(x, _)| *x == s) {
                out.notes.push(format!("catch-up copy to {s} failed: {e}"));
            }
        }
    }
    if reason == "scheduled" {
        let (pruned, notes) = prune(backend, settings, clock)?;
        out.pruned = pruned;
        out.notes.extend(notes);
    }
    Ok(out)
}

/// Copy the backups that lack a verified copy in a configured sink, and whose files are on this
/// host, to that sink. Returns how many backups received a copy and the `(sink, error)` of the
/// ones that did not.
pub fn catch_up(
    backend: &Arc<dyn Backend>,
    settings: &Settings,
    clock: &dyn Clock,
) -> Result<(usize, Vec<(String, String)>), String> {
    let catalog = Catalog::new(&**backend);
    let (mut copied, mut failed) = (0usize, Vec::new());
    for b in catalog.backups()? {
        // Only scheduled backups are caught up: they are the ones retention holds back until a
        // copy exists. A restore point somebody asked for is copied when its run had a sink.
        if !b.verified || b.reason != "scheduled" || !is_ours(&b, settings) {
            continue;
        }
        let data = PathBuf::from(&b.path);
        if !data.is_absolute() || !data.exists() {
            continue;
        }
        let mut rec = b.clone();
        for s in &settings.sinks {
            if rec.copies.iter().any(|c| c.sink == s.name()) {
                continue;
            }
            let manifest = PathBuf::from(format!("{}.manifest.json", b.path));
            let done = sink::copy_verified(&**s, &b.id, &data).and_then(|sha| {
                sink::copy_verified(&**s, &manifest_name(&b.id), &manifest).map(|_| sha)
            });
            match done {
                Ok(sha256) => rec.copies.push(Copy {
                    sink: s.name(),
                    name: b.id.clone(),
                    verified_at: clock.now(),
                    sha256: Some(sha256),
                }),
                Err(e) => failed.push((s.name(), format!("{}: {e}", b.id))),
            }
        }
        if rec.copies.len() != b.copies.len() {
            catalog.record_backup(&rec)?;
            copied += 1;
        }
    }
    Ok((copied, failed))
}

/// Whether this process may remove (or copy) the file of `b`: it was taken by this machine
/// (`OBOP_HOST_ID`, when either side has one) and the file is under the backup directory this
/// process was given (when it was given one).
pub fn is_ours(b: &BackupRecord, settings: &Settings) -> bool {
    if b.host.is_some() && b.host != settings.host {
        return false;
    }
    if settings.dir.as_os_str().is_empty() {
        return true;
    }
    Path::new(&b.path).starts_with(absolute(&settings.dir))
}

/// What a retention run would do, from the same rules as the real run (a dry run prints this).
#[derive(Debug, Clone, Default)]
pub struct PrunePlan {
    /// Backups past retention that are safe to remove now.
    pub remove: Vec<BackupRecord>,
    /// Why the others stay, and anything else worth saying.
    pub notes: Vec<String>,
    /// `(backup id, sink)` pairs whose recorded copy is no longer in the sink: the record is
    /// corrected when the plan is carried out, and the next run copies it again.
    pub stale_copies: Vec<(String, String)>,
}

/// Decide what retention removes: the policy over the scheduled backups of the catalogue, minus
/// the ones that are not safe to delete.
///
/// A backup stays if any configured sink does not hold a verified copy of it, proved now by
/// reading the sink (not by the record of an earlier run); if this process was given no sink but
/// the catalogue knows backups that were copied somewhere, because a shell without the sink
/// configured would delete the only copy of backups the sink never received (unless
/// `ignore_recorded_sinks`); and if another machine took it.
pub fn prune_plan(
    backend: &Arc<dyn Backend>,
    settings: &Settings,
    clock: &dyn Clock,
) -> Result<PrunePlan, String> {
    let catalog = Catalog::new(&**backend);
    let all = catalog.backups()?;
    let scheduled: Vec<&BackupRecord> = all.iter().filter(|b| b.reason == "scheduled").collect();
    let list: Vec<(String, u64)> = scheduled
        .iter()
        .map(|b| (b.id.clone(), b.created_at))
        .collect();
    let d = retention::decide_at(
        settings.retention,
        &list,
        settings.max_age.map(|a| (clock.now(), a)),
    );
    let mut plan = PrunePlan::default();
    let recorded_sinks = all.iter().any(|b| !b.copies.is_empty());
    if settings.sinks.is_empty() && recorded_sinks && !settings.ignore_recorded_sinks {
        if !d.delete.is_empty() {
            plan.notes.push(format!(
                "{} backup(s) are past retention, but the catalogue records copies off this host and this run was given no sink (--copy-to or OBOP_BACKUP_COPY_DIR): nothing is removed, because a backup goes only once every sink holds a verified copy. Give the sink, or pass --no-sinks to remove them anyway",
                d.delete.len()
            ));
        }
        return Ok(plan);
    }
    for id in &d.delete {
        let Some(b) = scheduled.iter().find(|b| &b.id == id) else {
            continue;
        };
        if !is_ours(b, settings) {
            plan.notes.push(format!(
                "{id} is past retention but its file is not on this host's backup directory, so it is left to the host that took it"
            ));
            continue;
        }
        let mut safe = true;
        for s in &settings.sinks {
            let Some(c) = b.copies.iter().find(|c| c.sink == s.name()) else {
                plan.notes.push(format!(
                    "{id} is past retention but has no verified copy in every sink, so it is kept"
                ));
                safe = false;
                break;
            };
            // Prove it again: the record is from the day of the copy.
            match s.digest(&b.id) {
                Ok((h, _)) if c.sha256.as_ref().is_none_or(|r| *r == h) => {}
                Ok(_) | Err(_) => {
                    plan.notes.push(format!(
                        "{id} is past retention but its copy in {} is not there or has changed, so it is kept and will be copied again",
                        s.name()
                    ));
                    plan.stale_copies.push((b.id.clone(), s.name()));
                    safe = false;
                    break;
                }
            }
        }
        if safe {
            plan.remove.push((*b).clone());
        }
    }
    Ok(plan)
}

/// Apply the retention policy to the scheduled backups of the catalogue. Returns the ids removed
/// from this host and notes. A backup stays in the catalogue unless its file is gone: a file that
/// could not be removed is reported and tried again next time.
pub fn prune(
    backend: &Arc<dyn Backend>,
    settings: &Settings,
    clock: &dyn Clock,
) -> Result<(Vec<String>, Vec<String>), String> {
    let catalog = Catalog::new(&**backend);
    let plan = prune_plan(backend, settings, clock)?;
    let (mut pruned, mut notes) = (Vec::new(), plan.notes.clone());
    for (id, sink) in &plan.stale_copies {
        if let Some(mut b) = catalog.backups()?.into_iter().find(|b| &b.id == id) {
            b.copies.retain(|c| &c.sink != sink);
            catalog.record_backup(&b)?;
        }
    }
    for b in &plan.remove {
        let id = &b.id;
        let data = Path::new(&b.path);
        let gone = match std::fs::remove_file(data) {
            Ok(()) => true,
            // Not there. For a file this process owns under an absolute path that means it was
            // removed by someone else; anything else may be a path that means something else
            // on another machine.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if data.is_absolute() && !settings.dir.as_os_str().is_empty() {
                    true
                } else {
                    notes.push(format!(
                        "{id}: the file {} is not found from here, so the record is kept",
                        b.path
                    ));
                    false
                }
            }
            Err(e) => {
                notes.push(format!(
                    "cannot remove {}: {e}; the record is kept and the removal is tried again",
                    b.path
                ));
                false
            }
        };
        if !gone {
            continue;
        }
        if let Err(e) = std::fs::remove_file(format!("{}.manifest.json", b.path))
            && e.kind() != std::io::ErrorKind::NotFound
        {
            notes.push(format!("cannot remove the manifest of {id}: {e}"));
        }
        catalog.forget_backup(id)?;
        pruned.push(id.clone());
        for s in &settings.sinks {
            for name in [id.clone(), manifest_name(id)] {
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
    /// there is none. A backup stamped in the future (by a clock that was wrong when it was
    /// taken) is not the newest: it would otherwise silence the schedule until real time caught
    /// up.
    pub fn due(&self, backend: &Arc<dyn Backend>) -> Result<bool, String> {
        let now = self.clock.now();
        let last = Catalog::new(&**backend).backups()?.into_iter().rfind(|b| {
            b.verified && b.reason == "scheduled" && b.created_at <= now + SKEW_TOLERANCE
        });
        Ok(match last {
            None => true,
            Some(b) => now.saturating_sub(b.created_at) >= self.settings.every.as_secs(),
        })
    }

    /// Take the backup if one is due and nobody else is taking it. `None` when nothing was done.
    pub fn tick(
        &self,
        backend: &Arc<dyn Backend>,
        blobs: Option<&dyn Blobs>,
        schema_version: u64,
    ) -> Result<Option<Outcome>, String> {
        // After a failure the timer waits (a minute, then two, up to an hour): a persistent fault
        // must not write a backup a minute.
        if let Some((n, at)) = failures(&**backend)
            && self.clock.now() < at.saturating_add(backoff_seconds(n))
        {
            return Ok(None);
        }
        if !self.due(backend)? {
            // Not due: catch up copies a sink missed, at most once an hour.
            if !self.settings.sinks.is_empty()
                && backend.kv_put_new(CATCHUP_KEY, "1", Some(3600 * 1000))?
            {
                catch_up(backend, &self.settings, &*self.clock)?;
            }
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
    /// Verified backups stamped later than now: written by a clock that was wrong. They are not
    /// counted as the newest backup.
    pub future_backups: usize,
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
    let sane = |b: &&BackupRecord| b.created_at <= now + SKEW_TOLERANCE;
    let last = backups.iter().filter(sane).rfind(|b| b.verified);
    let copied = backups
        .iter()
        .filter(sane)
        .rfind(|b| b.verified && !b.copies.is_empty());
    Ok(Stats {
        backup_age_seconds: last.map(|b| now.saturating_sub(b.created_at)),
        backup_bytes: last.map(|b| b.bytes),
        backup_duration_seconds: last.map(|b| b.duration_ms.div_ceil(1000)),
        copy_age_seconds: copied.map(|b| now.saturating_sub(b.created_at)),
        drill_age_seconds: catalog
            .latest_passed_drill()?
            .map(|d| now.saturating_sub(d.at)),
        last_failure: backend.kv_get(FAILURE_KEY)?,
        future_backups: backups
            .iter()
            .filter(|b| b.verified && b.created_at > now + SKEW_TOLERANCE)
            .count(),
    })
}

#[cfg(test)]
mod tests;
