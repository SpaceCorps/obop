//! `obop backup`, `obop backup verify` and `obop restore` (B4, D59, D73): the command line of
//! `obop_ops::backup`, including the sealed (format 2) backups of OBR-DAT-046.

use crate::ops::{Exit, blob_catalog, db_url, fail, has, open_existing, redact, take, usage};
use obop_keys::KeyRing;
use obop_migrate::Migrator;
use obop_ops::backup::{self, Keys, RestoreOptions};
use obop_ops::catalog::Catalog;
use obop_ops::retention::Retention;
use obop_ops::schedule::{self, Clock, Schedule, Settings, SystemClock};
use obop_ops::sink::{BackupSink, FsSink};
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

/// The process exit status of a restore or verify that needs a key that is not configured. Stable:
/// scripts test for it (OBR-DAT-046).
pub const EXIT_KEY_REQUIRED: u8 = 3;
/// A plain backup offered to an instance that seals its own (`backup_not_sealed`; `--allow-plain`).
pub const EXIT_NOT_SEALED: u8 = 4;
/// A restore whose records are in but whose uploaded files could not all be written
/// (`restore_files_failed`; run it again with `--force`).
pub const EXIT_FILES_FAILED: u8 = 5;

/// The keys of this process for backups: `OBOP_BACKUP_KEY`, else the server key from the
/// environment (never a key generated into the database; see `Keys::from_env`).
pub fn backup_keys() -> Result<Keys, String> {
    let server = KeyRing::from_env()?;
    Keys::from_env(&server)
}

/// As [`backup_keys`], for a command that opens backups: `--allow-plain` says the operator
/// vouches for a plain (format 1) backup although this instance seals its own.
pub fn backup_keys_for(args: &mut Vec<String>) -> Result<Keys, String> {
    let allow = has(args, "--allow-plain");
    backup_keys().map(|k| k.allowing_plain(allow))
}

/// The exit status of a failure with a stable code (`restore_key_required` 3, `backup_not_sealed`
/// 4, `restore_files_failed` 5), 1 for anything else.
pub(crate) fn exit_for(code: Option<&str>) -> ExitCode {
    match code {
        Some(backup::KEY_REQUIRED) => ExitCode::from(EXIT_KEY_REQUIRED),
        Some(backup::NOT_SEALED) => ExitCode::from(EXIT_NOT_SEALED),
        Some(backup::FILES_FAILED) => ExitCode::from(EXIT_FILES_FAILED),
        _ => ExitCode::FAILURE,
    }
}

/// A failed backup or restore as an exit status (see [`exit_for`]).
pub(crate) fn failure_of(what: &str, e: &str) -> ExitCode {
    eprintln!("{what}: {e}");
    exit_for(backup::error_code(e))
}

/// The backup gate of `module.upgrade` and `module.rollback` when they rewrite records
/// (OBR-DAT-053): a verified backup is taken into `OBOP_BACKUP_DIR` first and its id goes into the
/// ledger rows. On PostgreSQL (strict) there must be a directory, or the update is refused with
/// nothing changed; elsewhere it goes ahead with a warning. `OBOP_MIGRATION_GATES=relaxed` lets a
/// PostgreSQL instance proceed without (its operator then owns the backup). A tenant of a managed
/// instance cannot set either: the refusal says whom to ask.
pub struct ModuleGate {
    pub backend: Arc<dyn obop_storage::Backend>,
    pub blobs: Arc<dyn obop_storage::blob::Blobs>,
    pub settings: GateSettings,
}

/// What a [`ModuleGate`] decides by: read from the environment once, handed in by tests.
#[derive(Clone)]
pub struct GateSettings {
    pub strict: bool,
    pub dir: Option<std::path::PathBuf>,
    pub keys: Keys,
}

impl GateSettings {
    /// `OBOP_MIGRATION_GATES` (strict or relaxed; an unknown value is an error, as for
    /// `obop migrate run`), `OBOP_BACKUP_DIR` and the backup keys.
    pub fn from_env(backend_kind: &str) -> Result<GateSettings, String> {
        let chosen = std::env::var("OBOP_MIGRATION_GATES").ok();
        let strict = match chosen.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            None => backend_kind == "postgres",
            Some(s) => match obop_ops::gates::Mode::parse(s) {
                Some(m) => m == obop_ops::gates::Mode::Strict,
                None => {
                    return Err(format!(
                        "OBOP_MIGRATION_GATES takes strict or relaxed, not {s:?}"
                    ));
                }
            },
        };
        Ok(GateSettings {
            strict,
            dir: std::env::var("OBOP_BACKUP_DIR")
                .ok()
                .filter(|d| !d.trim().is_empty())
                .map(std::path::PathBuf::from),
            keys: backup_keys()?,
        })
    }
}

impl ModuleGate {
    pub fn new(
        backend: Arc<dyn obop_storage::Backend>,
        blobs: Arc<dyn obop_storage::blob::Blobs>,
        settings: GateSettings,
    ) -> ModuleGate {
        ModuleGate {
            backend,
            blobs,
            settings,
        }
    }
}

impl obop_pkg::MigrationGate for ModuleGate {
    fn before(&self, what: &obop_pkg::GateRequest) -> Result<obop_pkg::GateReceipt, String> {
        let s = &self.settings;
        let Some(dir) = &s.dir else {
            return if s.strict {
                Err(format!(
                    "{} rewrites stored records ({}) and there is no place for the verified backup that must come first. The operator of this instance sets OBOP_BACKUP_DIR to a volume (docs/DEPLOY.md), or OBOP_MIGRATION_GATES=relaxed if they take their own: ask them; a user of the instance cannot change it",
                    what.command,
                    what.modules.join(", ")
                ))
            } else {
                eprintln!(
                    "warning: {} rewrites records and no backup was taken: set OBOP_BACKUP_DIR",
                    what.command
                );
                Ok(obop_pkg::GateReceipt::default())
            };
        };
        // A refused read of the schema version is an error: a backup labelled version 0 would be
        // restored as if nothing had ever been migrated.
        let schema = Migrator::new(self.backend.clone(), KeyRing::default())
            .status()
            .map(|st| u64::from(st.stored.unwrap_or(0)))
            .map_err(|e| {
                format!(
                    "the backup before {} cannot be taken: the schema version cannot be read: {e}",
                    what.command
                )
            })?;
        let settings = Settings::new(dir, s.keys.clone());
        let o = schedule::take(
            &self.backend,
            Some(&*self.blobs),
            schema,
            &settings,
            "pre-migration",
            &SystemClock,
        )
        .map_err(|e| format!("the backup before {} failed: {e}", what.command))?;
        eprintln!(
            "backup before {} ({}): {} verified",
            what.command,
            what.modules.join(", "),
            o.backup.id
        );
        for n in &o.notes {
            eprintln!("warning: {n}");
        }
        Ok(obop_pkg::GateReceipt {
            backup_id: Some(o.backup.id),
        })
    }
}
/// The schema version a scheduled backup is labelled with, or why it has to wait: a version the
/// store would not give is not version 0 (a backup labelled 0 would be restored as if nothing had
/// ever been migrated).
pub(crate) fn schema_for_backup(backend: &Arc<dyn obop_storage::Backend>) -> Result<u64, String> {
    obop_migrate::Migrator::new(backend.clone(), obop_keys::KeyRing::default())
        .status()
        .map(|s| u64::from(s.stored.unwrap_or(0)))
        .map_err(|e| format!("the schema version cannot be read: {e}"))
}

/// How the instance takes the backup that comes before an update from inside (D63): the same
/// consistent, verified backup `obop backup` writes, into a directory.
pub fn backup_fn(url: String) -> obop_update::BackupFn {
    Arc::new(move |dir: &Path| {
        let backend = obop_storage::open(&url).map_err(|e| e.to_string())?;
        let ring = KeyRing::for_backend_read_only(&backend)?;
        let st = Migrator::new(backend.clone(), ring)
            .status()
            .map_err(|e| e.to_string())?;
        let schema = u64::from(st.stored.unwrap_or(0));
        let blobs = blob_catalog(&backend, &url).map_err(|_| "cannot open the uploaded files")?;
        let keys = backup_keys()?;
        let c = backup::create_sealed(&backend, Some(&blobs), schema, dir, &keys, &mut |_| {})?;
        if !c.manifest.sealed() {
            eprintln!(
                "warning: the backup before the update is NOT encrypted: set OBOP_BACKUP_KEY (or OBOP_SECRET_KEY)"
            );
        }
        backup::verify_with(&c.data_path, &keys)
            .map_err(|e| format!("the backup was written but does not verify: {e}"))?;
        Ok(c.data_path.display().to_string())
    })
}

fn describe(m: &backup::Manifest, authenticated: bool, keys: &Keys) -> String {
    let how = match (&m.encryption, authenticated) {
        (Some(e), true) => format!(
            ", sealed with key {} and AUTHENTICATED ({} chunks, manifest MAC checked)",
            e.key_id, e.chunks
        ),
        (Some(e), false) => format!(
            ", sealed with key {}; integrity only, NOT authenticated (no key given)",
            e.key_id
        ),
        (None, _) => format!(
            ", plain: checksums detect damage but do not authenticate or hide anything{}",
            if keys.can_seal() {
                " (accepted only because --allow-plain says you vouch for it; take a new backup to get a sealed one)"
            } else {
                ""
            }
        ),
    };
    format!(
        "ok: format {}{how}, written by obop {} at schema version {}, {} records, {}",
        m.format, m.core_version, m.schema_version, m.records, m.created_at
    )
}

/// `24h`, `90m`, `3600s`, `1d` as a duration of at least five minutes.
pub fn parse_every(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    let (num, unit) = s.split_at(s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len()));
    let n: u64 = num
        .parse()
        .map_err(|_| format!("{s:?} is not a duration (use 24h, 90m, 3600s or 1d)"))?;
    let secs = match unit {
        "s" => n,
        "m" | "min" => n * 60,
        "h" => n * 3600,
        "d" => n * 86_400,
        _ => {
            return Err(format!(
                "{s:?} is not a duration (use 24h, 90m, 3600s or 1d)"
            ));
        }
    };
    if secs < 300 {
        return Err(format!(
            "{s:?} is too often: a backup every five minutes at most"
        ));
    }
    Ok(Duration::from_secs(secs))
}

/// The directories to copy backups to: every `--copy-to`, else `OBOP_BACKUP_COPY_DIR` (comma
/// separated). `--copy-delete` (or `OBOP_BACKUP_COPY_DELETE=1`) lets retention delete there too.
fn sinks(args: &mut Vec<String>) -> Vec<Arc<dyn BackupSink>> {
    let mut dirs = Vec::new();
    while let Some(d) = take(args, "--copy-to") {
        dirs.push(d);
    }
    if dirs.is_empty() {
        dirs = std::env::var("OBOP_BACKUP_COPY_DIR")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    let del = has(args, "--copy-delete")
        || std::env::var("OBOP_BACKUP_COPY_DELETE")
            .is_ok_and(|v| matches!(v.trim(), "1" | "true" | "yes"));
    dirs.into_iter()
        .map(|d| Arc::new(FsSink::new(d).allow_delete(del)) as Arc<dyn BackupSink>)
        .collect()
}

/// The sinks this process is configured with (`OBOP_BACKUP_COPY_DIR`).
pub fn configured_sinks() -> Vec<Arc<dyn BackupSink>> {
    sinks(&mut Vec::new())
}

fn retention(args: &mut Vec<String>) -> Result<Retention, ExitCode> {
    let var = |n: &str| std::env::var(n).ok();
    Retention::from_vars(
        take(args, "--keep-daily")
            .or_else(|| var("OBOP_BACKUP_KEEP_DAILY"))
            .as_deref(),
        take(args, "--keep-weekly")
            .or_else(|| var("OBOP_BACKUP_KEEP_WEEKLY"))
            .as_deref(),
        take(args, "--keep-monthly")
            .or_else(|| var("OBOP_BACKUP_KEEP_MONTHLY"))
            .as_deref(),
    )
    .map_err(|e| fail(&format!("error: {e}")))
}

/// `--max-age-days N` (or `OBOP_BACKUP_MAX_AGE_DAYS`): the age no scheduled backup outlives; 0 or
/// `off` removes the bound.
fn max_age(args: &mut Vec<String>) -> Result<Option<u64>, ExitCode> {
    let v = take(args, "--max-age-days").or_else(|| std::env::var("OBOP_BACKUP_MAX_AGE_DAYS").ok());
    obop_ops::retention::max_age_from_var(v.as_deref()).map_err(|e| fail(&format!("error: {e}")))
}

/// What a scheduled backup is made of, from the environment (the server's timer).
pub fn settings_from_env(keys: Keys) -> Result<Option<(Settings, Duration)>, String> {
    let Some(every) = std::env::var("OBOP_BACKUP_SCHEDULE")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "off")
    else {
        return Ok(None);
    };
    let every = parse_every(&every).map_err(|e| format!("OBOP_BACKUP_SCHEDULE: {e}"))?;
    let dir = std::env::var("OBOP_BACKUP_DIR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .ok_or(
            "OBOP_BACKUP_SCHEDULE needs OBOP_BACKUP_DIR (a volume that outlives the container)",
        )?;
    let mut s = Settings::new(dir, keys);
    s.every = every;
    s.retention = Retention::from_env()?;
    s.max_age = obop_ops::retention::max_age_from_var(
        std::env::var("OBOP_BACKUP_MAX_AGE_DAYS").ok().as_deref(),
    )?;
    let mut none: Vec<String> = Vec::new();
    s.sinks = sinks(&mut none);
    Ok(Some((s, every)))
}

/// The scheduled-backup settings of this process, read and checked, to start the timer with
/// once the server is up. `serve` reads them before it opens (and so migrates) the database: a
/// typo in a duration, a retention number or a key stops the start before anything is written.
pub struct SchedulerPlan {
    settings: Settings,
    every: Duration,
}

/// Read and check the settings of the timer: `Ok(None)` when `OBOP_BACKUP_SCHEDULE` is unset or
/// `off`.
pub fn scheduler_plan() -> Result<Option<SchedulerPlan>, String> {
    if std::env::var_os("OBOP_BACKUP_SCHEDULE").is_none() {
        return Ok(None);
    }
    let keys = backup_keys()?;
    Ok(settings_from_env(keys)?.map(|(settings, every)| SchedulerPlan { settings, every }))
}

/// Start the server's timer for scheduled backups: one thread that asks every minute whether a
/// backup is due. The decision lives in the database, so any number of processes may run it and
/// one of them takes each backup.
pub fn start_scheduler(
    plan: Option<SchedulerPlan>,
    backend: Arc<dyn obop_storage::Backend>,
    blobs: Arc<dyn obop_storage::blob::Blobs>,
) {
    let Some(SchedulerPlan { settings, every }) = plan else {
        return;
    };
    eprintln!(
        "scheduled backups: every {} into {} ({}); keeping {}{}{}",
        obop_ops::doctor::human(every.as_secs()),
        settings.dir.display(),
        if settings.keys.can_seal() {
            "sealed"
        } else {
            "NOT encrypted: set OBOP_BACKUP_KEY"
        },
        settings.retention.describe(),
        settings.max_age.map_or(String::new(), |a| format!(
            ", none older than {} days",
            a / 86_400
        )),
        if settings.sinks.is_empty() {
            "; no off-host copy (set OBOP_BACKUP_COPY_DIR)".to_string()
        } else {
            format!(
                "; copied to {}",
                settings
                    .sinks
                    .iter()
                    .map(|s| s.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    );
    let schedule = Schedule::new(settings, Arc::new(SystemClock));
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(60));
            let schema = match schema_for_backup(&backend) {
                Ok(v) => v,
                // Not "version 0": the backup waits for the next minute.
                Err(e) => {
                    eprintln!("scheduled backup postponed: {e}");
                    continue;
                }
            };
            match schedule.tick(&backend, Some(&*blobs), schema) {
                Ok(Some(o)) => {
                    eprintln!(
                        "scheduled backup {} written and verified ({} records, {} bytes){}",
                        o.backup.id,
                        o.backup.records,
                        o.backup.bytes,
                        if o.copy_failures.is_empty() {
                            format!(
                                "; {} copy(ies) verified; {} old backup(s) removed",
                                o.backup.copies.len(),
                                o.pruned.len()
                            )
                        } else {
                            format!("; COPY FAILED: {:?}", o.copy_failures)
                        }
                    );
                    for n in &o.notes {
                        eprintln!("scheduled backup: {n}");
                    }
                }
                Ok(None) => {}
                Err(e) => eprintln!("scheduled backup failed: {e}"),
            }
        }
    });
}
pub fn backup_cmd(args: &mut Vec<String>) -> Exit {
    match args.first().map(String::as_str) {
        Some("run") => {
            args.remove(0);
            return run_job(args);
        }
        Some("list") => {
            args.remove(0);
            return list(args);
        }
        Some("prune") => {
            args.remove(0);
            return prune(args);
        }
        _ => {}
    }
    if args.first().map(String::as_str) == Some("verify") {
        args.remove(0);
        let structure_only = has(args, "--structure-only");
        let canary = take(args, "--canary");
        let keys = backup_keys_for(args).map_err(|e| fail(&e))?;
        if args.len() != 1 {
            return Err(usage());
        }
        let file = Path::new(&args[0]);
        let verified = if structure_only {
            backup::verify_structure(file)
        } else {
            backup::verify_with(file, &keys)
        };
        let v = verified.map_err(|e| failure_of("FAILED", &e))?;
        println!("{}", describe(&v.manifest, v.authenticated, &keys));
        for (model, n) in &v.manifest.models {
            println!("  {model}: {n}");
        }
        if let Some(c) = canary {
            let (hits, sealed) = backup::count_in_backup(file, c.as_bytes())
                .map_err(|e| fail(&format!("FAILED: {e}")))?;
            if sealed {
                println!(
                    "canary scan: {hits} occurrence(s) of the given text in the sealed file as stored"
                );
            } else {
                println!(
                    "canary scan: {hits} occurrence(s) of the given text in the file after decompression (this backup is plain: gunzip gives everyone its records)"
                );
            }
            if !sealed {
                return Err(fail(
                    "FAILED: this backup is not encrypted (format 1), so a canary scan proves nothing about it; seal backups with OBOP_BACKUP_KEY (or OBOP_SECRET_KEY)",
                ));
            }
            if hits > 0 {
                return Err(fail(
                    "FAILED: the canary text is in the backup file in the clear",
                ));
            }
        }
        return Ok(());
    }
    let url = db_url(take(args, "--db").as_deref());
    let out = take(args, "--out").ok_or_else(usage)?;
    let plain = has(args, "--plain");
    let require = has(args, "--encrypted");
    if !args.is_empty() || (plain && require) {
        return Err(usage());
    }
    let backend = open_existing(&url)?;
    let ring = KeyRing::for_backend_read_only(&backend).map_err(|e| fail(&e))?;
    let st = Migrator::new(backend.clone(), ring)
        .status()
        .map_err(|e| fail(&e))?;
    let schema = u64::from(st.stored.unwrap_or(0));
    let keys = if plain {
        Keys::none()
    } else {
        backup_keys().map_err(|e| fail(&e))?
    };
    if require && !keys.can_seal() {
        return Err(fail(
            "error: --encrypted needs a key: set OBOP_BACKUP_KEY (or OBOP_SECRET_KEY); see docs/DEPLOY.md",
        ));
    }
    let mut last = 0usize;
    let blobs = blob_catalog(&backend, &url)?;
    let c = backup::create_sealed(
        &backend,
        Some(&blobs),
        schema,
        Path::new(&out),
        &keys,
        &mut |n| {
            if n / 10_000 > last / 10_000 {
                eprintln!("  {n} records");
            }
            last = n;
        },
    )
    .map_err(|e| fail(&format!("backup failed: {e}")))?;
    // A backup is only a backup once it reads back (and, sealed, once it opens with its key).
    let v = backup::verify_with(&c.data_path, &keys)
        .map_err(|e| fail(&format!("the backup was written but does not verify: {e}")))?;
    println!(
        "backup of {} written: {} ({} records, {} uploaded files, {} bytes), manifest {}",
        redact(&url),
        c.data_path.display(),
        c.manifest.records,
        c.manifest.blobs,
        c.manifest.data_bytes,
        c.manifest_path.display()
    );
    println!("{}", describe(&c.manifest, v.authenticated, &keys));
    if c.manifest.sealed() {
        println!(
            "the records are encrypted with key {} (from {}): store that key apart from the backup; without it the backup cannot be read",
            c.manifest
                .encryption
                .as_ref()
                .map_or("?", |e| e.key_id.as_str()),
            keys.describe()
        );
    } else {
        println!(
            "warning: this backup is NOT encrypted: every record is readable by whoever holds the file. Set OBOP_BACKUP_KEY (or OBOP_SECRET_KEY) to get format 2, or keep it where only the operator can read it"
        );
    }
    if backend
        .get("connector_key", "master")
        .ok()
        .flatten()
        .is_some()
    {
        println!(
            "warning: this instance has no OBOP_SECRET_KEY, so its secret key is a record in the database and therefore in this backup. Set OBOP_SECRET_KEY (docs/DEPLOY.md) and keep it apart from backups."
        );
    } else {
        println!(
            "sealed secrets are inside as stored; keep OBOP_SECRET_KEY apart from backups: without it they cannot be opened"
        );
    }
    Ok(())
}

pub fn restore_cmd(args: &mut Vec<String>) -> Exit {
    let url = db_url(take(args, "--db").as_deref());
    let from = take(args, "--from").ok_or_else(usage)?;
    let force = has(args, "--force");
    let dry_run = has(args, "--dry-run");
    let keys = backup_keys_for(args).map_err(|e| fail(&e))?;
    if !args.is_empty() {
        return Err(usage());
    }
    // A backup that cannot be opened is refused before the target database is opened, so a
    // refused restore creates nothing (no file, no tables).
    backup::precheck(Path::new(&from), &keys)
        .map_err(|e| failure_of("restore refused or failed", &e))?;
    let backend =
        obop_storage::open(&url).map_err(|e| fail(&format!("cannot open the database: {e}")))?;
    let latest = obop_migrate::steps().len() as u64;
    let blobs = blob_catalog(&backend, &url)?;
    let r = backup::restore_sealed(
        &backend,
        Some(&blobs),
        latest,
        Path::new(&from),
        RestoreOptions { force, dry_run },
        &keys,
        &mut |n| {
            if n % 10_000 < 1000 {
                eprintln!("  {n} records");
            }
        },
    )
    .map_err(|e| failure_of("restore refused or failed", &e))?;
    let verb = if r.dry_run {
        "would restore"
    } else {
        "restored"
    };
    println!(
        "{verb} {} records into {} (backup of obop {}, schema version {}, taken {}){}",
        r.records,
        redact(&url),
        r.manifest.core_version,
        r.manifest.schema_version,
        r.manifest.created_at,
        if r.target_records > 0 {
            format!("; replacing {} existing records", r.target_records)
        } else {
            String::new()
        }
    );
    println!(
        "{}",
        if r.authenticated {
            "the backup was opened with its key and authenticated"
        } else {
            "the backup is NOT authenticated (plain, format 1): it was only checked against its checksums"
        }
    );
    if r.manifest.core_version != obop_types::CORE_VERSION {
        println!(
            "note: the backup is from obop {}, this is {}",
            r.manifest.core_version,
            obop_types::CORE_VERSION
        );
    }
    if r.needs_migration {
        println!(
            "the backup is from an older schema: run `obop migrate run --db {}` before serving",
            redact(&url)
        );
    }
    Ok(())
}

/// `obop backup run`: the whole job once, as cron or a platform's scheduled job runs it: a
/// backup, read back and authenticated, recorded, copied off the host with each copy read back,
/// and the retention policy applied to the scheduled ones.
fn run_job(args: &mut Vec<String>) -> Exit {
    let url = db_url(take(args, "--db").as_deref());
    let out = take(args, "--out")
        .or_else(|| std::env::var("OBOP_BACKUP_DIR").ok())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| fail("error: give --out DIR or set OBOP_BACKUP_DIR"))?;
    let reason = take(args, "--reason").unwrap_or_else(|| "scheduled".into());
    if !matches!(reason.as_str(), "scheduled" | "manual") {
        return Err(fail(
            "--reason takes scheduled (retention applies) or manual (kept until removed)",
        ));
    }
    let retention = retention(args)?;
    let max_age = max_age(args)?;
    let sinks = sinks(args);
    let plain = has(args, "--plain");
    if !args.is_empty() {
        return Err(usage());
    }
    let backend = open_existing(&url)?;
    let ring = KeyRing::for_backend_read_only(&backend).map_err(|e| fail(&e))?;
    let schema = u64::from(
        Migrator::new(backend.clone(), ring)
            .status()
            .map_err(|e| fail(&e))?
            .stored
            .unwrap_or(0),
    );
    let keys = if plain {
        Keys::none()
    } else {
        backup_keys().map_err(|e| fail(&e))?
    };
    let blobs = blob_catalog(&backend, &url)?;
    let mut settings = Settings::new(out, keys);
    settings.sinks = sinks;
    settings.retention = retention;
    settings.max_age = max_age;
    let o = schedule::take(
        &backend,
        Some(&blobs),
        schema,
        &settings,
        &reason,
        &SystemClock,
    )
    .map_err(|e| failure_of("backup failed", &e))?;
    println!(
        "backup {} written and verified{} ({} records, {} bytes, {} ms)",
        o.backup.id,
        if o.backup.authenticated {
            " and authenticated"
        } else {
            ""
        },
        o.backup.records,
        o.backup.bytes,
        o.backup.duration_ms
    );
    for c in &o.backup.copies {
        println!("copy in {} read back and hash-checked", c.sink);
    }
    for id in &o.pruned {
        println!(
            "removed {id} (past retention: {})",
            settings.retention.describe()
        );
    }
    for n in &o.notes {
        println!("note: {n}");
    }
    if o.copy_failures.is_empty() {
        Ok(())
    } else {
        for (sink, e) in &o.copy_failures {
            eprintln!("COPY FAILED to {sink}: {e}");
        }
        eprintln!("the backup is good on this host but not yet off it; fix the sink and run again");
        Err(ExitCode::FAILURE)
    }
}

/// `obop backup list`: what the instance remembers having taken.
fn list(args: &mut Vec<String>) -> Exit {
    let url = db_url(take(args, "--db").as_deref());
    if !args.is_empty() {
        return Err(usage());
    }
    let backend = open_existing(&url)?;
    let now = SystemClock.now();
    let all = Catalog::new(&*backend).backups().map_err(|e| fail(&e))?;
    if all.is_empty() {
        println!("no backup recorded (obop backup run records one)");
    }
    for b in &all {
        println!(
            "{}  {}  format {}{}  {} records, {} bytes  {}{}  copies: {}  [{}]",
            obop_ops::backup::rfc3339(b.created_at),
            b.id,
            b.format,
            b.key_id
                .as_deref()
                .map_or(String::new(), |k| format!(" key {k}")),
            b.records,
            b.bytes,
            if b.authenticated {
                "authenticated"
            } else if b.verified {
                "verified"
            } else {
                "NOT verified"
            },
            format_args!(
                ", {} ago",
                obop_ops::doctor::human(now.saturating_sub(b.created_at))
            ),
            if b.copies.is_empty() {
                "none".to_string()
            } else {
                b.copies
                    .iter()
                    .map(|c| c.sink.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            },
            b.reason,
        );
    }
    for (sink, parts) in schedule::sink_strays(&configured_sinks()) {
        println!(
            "{sink} holds {} unfinished upload(s): {} (a copy was killed; the next copy removes parts idle for hours)",
            parts.len(),
            parts.join(", ")
        );
    }
    let st = schedule::stats(&*backend, now).map_err(|e| fail(&e))?;
    if let Some(f) = st.last_failure {
        println!("last scheduled run reported: {f}");
    }
    Ok(())
}

/// `obop backup prune [--dry-run] [--no-sinks]`: apply the retention policy to the scheduled
/// backups now, by the same rules as the scheduled job (the dry run is the plan of the real run).
fn prune(args: &mut Vec<String>) -> Exit {
    let url = db_url(take(args, "--db").as_deref());
    let dry = has(args, "--dry-run");
    let no_sinks = has(args, "--no-sinks");
    let retention = retention(args)?;
    let max_age = max_age(args)?;
    let sinks = sinks(args);
    if !args.is_empty() {
        return Err(usage());
    }
    let backend = open_existing(&url)?;
    // The files this prune may remove are the ones under the directory the instance backs up
    // into, when this shell knows it.
    let dir = std::env::var("OBOP_BACKUP_DIR")
        .ok()
        .filter(|d| !d.trim().is_empty())
        .unwrap_or_default();
    let mut settings = Settings::new(dir, Keys::none());
    settings.retention = retention;
    settings.max_age = max_age;
    settings.sinks = sinks;
    settings.ignore_recorded_sinks = no_sinks;
    if dry {
        let plan = schedule::prune_plan(&backend, &settings, &SystemClock).map_err(|e| fail(&e))?;
        println!(
            "policy: {}{}",
            settings.retention.describe(),
            settings.max_age.map_or(String::new(), |a| format!(
                ", none older than {} days",
                a / 86_400
            ))
        );
        println!("would remove {}:", plan.remove.len());
        for b in &plan.remove {
            println!("  remove {}", b.id);
        }
        for n in &plan.notes {
            println!("note: {n}");
        }
        return Ok(());
    }
    let (pruned, notes) =
        schedule::prune(&backend, &settings, &SystemClock).map_err(|e| fail(&e))?;
    for id in &pruned {
        println!("removed {id}");
    }
    for n in notes {
        println!("note: {n}");
    }
    println!(
        "{} backup(s) removed; policy: {}",
        pruned.len(),
        settings.retention.describe()
    );
    Ok(())
}
