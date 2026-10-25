//! `obop storage check`: is this database set up so that the promises of docs/DEPLOY.md can be
//! kept (OBR-DAT-046 to 049 and 056, D73)?
//!
//! The checks are plain functions of what the server and the catalogue say, so they run against a
//! fake server in tests and against the real one from the command. They check **configuration and
//! evidence, not the world**: that `archive_mode` is on and the archiver has archived a segment a
//! minute ago says nothing about whether the store it archives to is encrypted, locked or in
//! another failure domain; that is what the attestation of the operator, and the restore drills,
//! are for. Nothing here has been run against a managed PostgreSQL service.
//!
//! Three levels. `Fail` is a promise that cannot be kept (no WAL archiving, no backup in 26
//! hours, no drill in 35 days, no attestation); `Warn` is a weakness or a development profile;
//! `Pass`. `obop storage check` exits non-zero on any `Fail`. A database that is not PostgreSQL
//! is a development database (D44): the host checks are skipped and the evidence checks only warn.

use crate::catalog::Catalog;
use crate::pgadmin::{ServerInfo, seconds};
use crate::schedule::{self, OVERDUE_AFTER_SECONDS};
use obop_storage::Backend;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Pass,
    Warn,
    Fail,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Pass => "pass",
            Level::Warn => "warn",
            Level::Fail => "FAIL",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub name: &'static str,
    /// The requirement or promise the check belongs to.
    pub about: &'static str,
    pub level: Level,
    pub detail: String,
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub checks: Vec<Check>,
}

impl Report {
    fn add(
        &mut self,
        name: &'static str,
        about: &'static str,
        level: Level,
        detail: impl Into<String>,
    ) {
        self.checks.push(Check {
            name,
            about,
            level,
            detail: detail.into(),
        });
    }

    pub fn failed(&self) -> bool {
        self.checks.iter().any(|c| c.level == Level::Fail)
    }

    pub fn worst(&self) -> Level {
        self.checks
            .iter()
            .map(|c| c.level)
            .max()
            .unwrap_or(Level::Pass)
    }

    pub fn find(&self, name: &str) -> Option<&Check> {
        self.checks.iter().find(|c| c.name == name)
    }

    pub fn render(&self) -> String {
        let mut s = String::new();
        for c in &self.checks {
            s.push_str(&format!(
                "{:>5}  {:<22} {}\n",
                c.level.as_str(),
                c.name,
                c.detail
            ));
        }
        s
    }
}

/// How long a drill may be old before the check fails: 35 days.
pub const DRILL_MAX_AGE: u64 = 35 * 86_400;
/// How long an attestation of storage encryption lasts: 12 months.
pub const ATTESTATION_MAX_AGE: u64 = 365 * 86_400;
/// The longest the archiver may have been silent.
pub const ARCHIVE_LAG_MAX: u64 = 300;
/// `archive_timeout` the recovery point of 5 minutes needs.
pub const ARCHIVE_TIMEOUT_MAX: u64 = 60;

pub struct Inputs<'a> {
    pub backend: &'a dyn Backend,
    /// The database is PostgreSQL (a deployment, D44), so every promise is checked as one.
    pub production: bool,
    /// The server, when the database is PostgreSQL and could be asked.
    pub server: Option<&'a dyn ServerInfo>,
    /// `sslmode` of the database URL, or `None` when absent.
    pub sslmode: Option<&'a str>,
    /// Whether the host of the database URL is this machine or a private name.
    pub host_is_local: bool,
    pub plane: &'a str,
    pub now: u64,
}

fn on(v: &Option<String>) -> bool {
    matches!(v.as_deref(), Some("on" | "true" | "yes" | "always" | "1"))
}

/// Run every check.
pub fn check(i: &Inputs) -> Result<Report, String> {
    let mut r = Report::default();
    let production = i.production;
    let dev = |level: Level| if production { level } else { Level::Warn };
    if let Some(s) = i.server.filter(|_| production) {
        host(&mut r, s, i)?;
    } else if production {
        r.add(
            "server",
            "OBR-DAT-048",
            Level::Fail,
            "the PostgreSQL server could not be asked about its settings",
        );
    } else {
        r.add(
            "server",
            "OBR-DAT-048",
            Level::Warn,
            format!(
                "{} is a development database (D44): WAL archiving and point-in-time recovery exist on PostgreSQL only",
                i.backend.kind()
            ),
        );
    }
    let stats = schedule::stats(i.backend, i.now)?;
    match stats.backup_age_seconds {
        None => r.add(
            "backup",
            "OBR-DAT-047",
            dev(Level::Fail),
            "no verified backup is recorded (obop backup run, or set OBOP_BACKUP_SCHEDULE)",
        ),
        Some(a) if a > OVERDUE_AFTER_SECONDS => r.add(
            "backup",
            "OBR-DAT-047",
            dev(Level::Fail),
            format!(
                "the newest verified backup is {} old; a nightly job should keep it under 26 hours",
                human(a)
            ),
        ),
        Some(a) => r.add(
            "backup",
            "OBR-DAT-047",
            Level::Pass,
            format!("newest verified backup {} old", human(a)),
        ),
    }
    let backups = Catalog::new(i.backend).backups()?;
    if let Some(last) = backups.iter().rfind(|b| b.verified) {
        if last.format == 1 {
            r.add(
                "backup_encrypted",
                "OBR-DAT-046",
                dev(Level::Fail),
                "the newest backup is plain (format 1): every record is readable by whoever holds the file. Set OBOP_BACKUP_KEY or OBOP_SECRET_KEY",
            );
        } else {
            r.add(
                "backup_encrypted",
                "OBR-DAT-046",
                Level::Pass,
                format!(
                    "sealed (format 2) with key {}, authenticated when it was read back: {}",
                    last.key_id.as_deref().unwrap_or("?"),
                    last.authenticated
                ),
            );
        }
    }
    match stats.copy_age_seconds {
        None => r.add(
            "backup_offsite",
            "OBR-DAT-047",
            dev(Level::Fail),
            "no backup has a verified copy off this host (set OBOP_BACKUP_COPY_DIR, or run your own uploader and record it)",
        ),
        Some(a) if a > OVERDUE_AFTER_SECONDS => r.add(
            "backup_offsite",
            "OBR-DAT-047",
            dev(Level::Fail),
            format!("the newest backup with a verified off-host copy is {} old", human(a)),
        ),
        Some(a) => r.add(
            "backup_offsite",
            "OBR-DAT-047",
            Level::Pass,
            format!("newest off-host copy {} old, read back and hashed", human(a)),
        ),
    }
    match stats.drill_age_seconds {
        None => r.add(
            "restore_drill",
            "OBR-DAT-049",
            dev(Level::Fail),
            "no restore drill has passed (obop drill BACKUP --scratch URL --record-in URL; monthly)",
        ),
        Some(a) if a > DRILL_MAX_AGE => r.add(
            "restore_drill",
            "OBR-DAT-049",
            dev(Level::Fail),
            format!("the last passed restore drill is {} old; the limit is 35 days", human(a)),
        ),
        Some(a) => r.add(
            "restore_drill",
            "OBR-DAT-049",
            Level::Pass,
            format!("last passed restore drill {} ago", human(a)),
        ),
    }
    if let Some(f) = &stats.last_failure {
        r.add(
            "backup_failure",
            "OBR-DAT-047",
            Level::Warn,
            format!("the last scheduled run reported: {f}"),
        );
    }
    match Catalog::new(i.backend).attestation(i.plane)? {
        None => r.add(
            "storage_attestation",
            "OBR-DAT-056",
            dev(Level::Fail),
            format!("no attestation that the storage of plane {:?} is encrypted at rest (obop storage attest ...)", i.plane),
        ),
        Some(a) => {
            let age = i.now.saturating_sub(a.at);
            let level = if age > ATTESTATION_MAX_AGE {
                dev(Level::Fail)
            } else if age > ATTESTATION_MAX_AGE - 30 * 86_400 {
                Level::Warn
            } else {
                Level::Pass
            };
            r.add(
                "storage_attestation",
                "OBR-DAT-056",
                level,
                format!(
                    "{} on {} ({}), keys held by {}, readable by {}; attested {} ago by {}; evidence: {}{}",
                    a.mechanism,
                    a.host,
                    a.provider,
                    a.key_custody,
                    a.key_readers,
                    human(age),
                    a.attested_by,
                    a.evidence,
                    if age > ATTESTATION_MAX_AGE { " (EXPIRED: renew every 12 months and after a host change)" } else { "" }
                ),
            );
        }
    }
    Ok(r)
}

fn host(r: &mut Report, s: &dyn ServerInfo, i: &Inputs) -> Result<(), String> {
    let get = |n: &str| s.setting(n);
    if s.in_recovery()? {
        r.add(
            "server",
            "OBR-DAT-048",
            Level::Warn,
            "this server is a standby replaying WAL; check the primary",
        );
    }
    let wal_level = get("wal_level")?;
    match wal_level.as_deref() {
        Some("replica" | "logical") => r.add(
            "wal_level",
            "OBR-DAT-048",
            Level::Pass,
            wal_level.unwrap_or_default(),
        ),
        other => r.add(
            "wal_level",
            "OBR-DAT-048",
            Level::Fail,
            format!(
                "wal_level is {:?}; archiving and point-in-time recovery need replica or logical",
                other.unwrap_or("unknown")
            ),
        ),
    }
    let mode = get("archive_mode")?;
    if on(&mode) {
        r.add(
            "archive_mode",
            "OBR-DAT-048",
            Level::Pass,
            mode.unwrap_or_default(),
        );
    } else {
        r.add(
            "archive_mode",
            "OBR-DAT-048",
            Level::Fail,
            format!("archive_mode is {:?}: set it on (needs a restart) and an archive_command, or use a managed service with documented point-in-time recovery", mode.unwrap_or_default()),
        );
    }
    let cmd = get("archive_command")?.unwrap_or_default();
    let lib = get("archive_library")?.unwrap_or_default();
    let useless = cmd.trim().is_empty()
        || cmd.contains("(disabled)")
        || matches!(
            cmd.trim(),
            "false" | "/bin/false" | "true" | "/bin/true" | ":"
        );
    if lib.trim().is_empty() && useless {
        r.add(
            "archive_command",
            "OBR-DAT-048",
            Level::Fail,
            "no archive_command (or archive_library) that copies WAL segments off the host: pgBackRest or wal-g, see docs/DEPLOY.md",
        );
    } else {
        // Never print the command: it can carry credentials.
        r.add(
            "archive_command",
            "OBR-DAT-048",
            Level::Pass,
            "set (not shown: it may hold credentials)",
        );
    }
    let timeout = get("archive_timeout")?.as_deref().and_then(seconds);
    match timeout {
        Some(t) if (1..=ARCHIVE_TIMEOUT_MAX).contains(&t) => r.add(
            "archive_timeout",
            "OBR-DAT-048",
            Level::Pass,
            format!("{t} s"),
        ),
        other => r.add(
            "archive_timeout",
            "OBR-DAT-048",
            Level::Fail,
            format!(
                "archive_timeout is {}; 60 s or less gives the recovery point of 5 minutes",
                other.map_or("off".to_string(), |t| format!("{t} s"))
            ),
        ),
    }
    let a = s.archiver()?;
    match (a.archived_count, a.last_archived_age) {
        (0, _) | (_, None) => r.add(
            "archiver",
            "OBR-DAT-048",
            Level::Fail,
            "no WAL segment has ever been archived (pg_stat_archiver)",
        ),
        (_, Some(age)) if age > ARCHIVE_LAG_MAX => r.add(
            "archiver",
            "OBR-DAT-048",
            Level::Fail,
            format!("the last segment was archived {age} s ago; the alert threshold is 300 s"),
        ),
        (n, Some(age)) => {
            let failing = a.last_failed_age.is_some_and(|f| f < age);
            r.add(
                "archiver",
                "OBR-DAT-048",
                if failing { Level::Fail } else { Level::Pass },
                format!(
                    "{n} segments archived, last {age} s ago{}",
                    if failing {
                        "; the most recent attempt FAILED"
                    } else {
                        ""
                    }
                ),
            )
        }
    }
    for (n, about) in [("fsync", "durability"), ("full_page_writes", "durability")] {
        let v = get(n)?;
        if v.as_deref().is_some_and(|v| v != "on") {
            r.add(
                "durability",
                "OBR-DAT-048",
                Level::Fail,
                format!("{n} is off ({about}): a crash can corrupt the database"),
            );
        }
    }
    if get("data_checksums")?.as_deref() != Some("on") {
        r.add(
            "data_checksums",
            "OBR-DAT-056",
            Level::Warn,
            "data checksums are off: silent disk corruption would be read as data (initdb --data-checksums)",
        );
    }
    let ssl = get("ssl")?;
    let mode = i.sslmode.unwrap_or("disable");
    let encrypted_client = matches!(mode, "require" | "verify-ca" | "verify-full");
    if i.host_is_local && !encrypted_client {
        r.add(
            "tls",
            "D66",
            Level::Pass,
            "the database is on this machine or a private name; the connection is plain",
        );
    } else if !on(&ssl) {
        r.add("tls", "D66", Level::Fail, "the server has ssl off: turn it on (ssl, ssl_cert_file, ssl_key_file) and use sslmode=verify-full");
    } else if !encrypted_client {
        r.add("tls", "D66", Level::Fail, format!("the server offers TLS but the connection uses sslmode={mode}: add sslmode=require (verify-full when the CA is known) to the database URL"));
    } else if mode == "require" {
        r.add("tls", "D66", Level::Warn, "sslmode=require encrypts but does not prove the server's identity: verify-full with sslrootcert is stronger");
    } else {
        r.add("tls", "D66", Level::Pass, format!("sslmode={mode}"));
    }
    Ok(())
}

pub fn human(secs: u64) -> String {
    match secs {
        0..=119 => format!("{secs} s"),
        120..=7199 => format!("{} min", secs / 60),
        7200..=172_799 => format!("{} h", secs / 3600),
        _ => format!("{} days", secs / 86_400),
    }
}

#[cfg(test)]
mod tests;
