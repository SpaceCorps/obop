//! The operator commands (B3 to B5, D59): `obop migrate`, `obop backup`, `obop restore` and
//! `obop secrets`, and the schema guard every command that opens a database goes through.
//! They work on a database given by `--db` (or `OBOP_DATABASE_URL`, default `sqlite://obop.db`)
//! and need no running server.

use obop_keys::KeyRing;
use obop_keys::reseal::Options as ResealOptions;
use obop_migrate::Migrator;
use obop_ops::backup::{self, RestoreOptions};
use obop_storage::Backend;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

type Exit = Result<(), ExitCode>;

fn fail(msg: &str) -> ExitCode {
    eprintln!("{msg}");
    ExitCode::FAILURE
}

fn usage() -> ExitCode {
    eprintln!("{}", crate::USAGE);
    ExitCode::from(2)
}

/// `--auto-migrate` (1), `--no-auto-migrate` (2), neither (0).
static AUTO_FLAG: AtomicU8 = AtomicU8::new(0);

/// Remove `--auto-migrate` / `--no-auto-migrate` from the arguments and remember them.
pub fn take_migrate_flags(args: Vec<String>) -> Vec<String> {
    args.into_iter()
        .filter(|a| match a.as_str() {
            "--auto-migrate" => {
                AUTO_FLAG.store(1, Ordering::Relaxed);
                false
            }
            "--no-auto-migrate" => {
                AUTO_FLAG.store(2, Ordering::Relaxed);
                false
            }
            _ => true,
        })
        .collect()
}

/// Whether pending migration steps run by themselves when a command opens the database.
fn auto_migrate(kind: &str) -> bool {
    match AUTO_FLAG.load(Ordering::Relaxed) {
        1 => return true,
        2 => return false,
        _ => {}
    }
    if let Ok(v) = std::env::var("OBOP_AUTO_MIGRATE") {
        match v.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => return true,
            "0" | "false" | "no" | "off" => return false,
            _ => {}
        }
    }
    // A local file is one process and cheap to back up; a PostgreSQL deployment may run several
    // processes, so an operator decides when the schema changes.
    kind != "postgres"
}

/// A URL for messages, without a password.
fn redact(url: &str) -> String {
    match (url.find("://"), url.rfind('@')) {
        (Some(i), Some(at)) if at > i => format!("{}://***@{}", &url[..i], &url[at + 1..]),
        _ => url.to_string(),
    }
}

/// Called when a command opens the database: stamp an empty one with the current schema
/// version, refuse one from the future, and run (or demand) pending migration steps.
pub fn guard_database(backend: &Arc<dyn Backend>, url: &str) -> Exit {
    let ring = KeyRing::for_backend_read_only(backend).map_err(|e| fail(&e))?;
    let m = Migrator::new(backend.clone(), ring);
    m.stamp_if_fresh().map_err(|e| fail(&e))?;
    let lines = m
        .check_for_serve(auto_migrate(backend.kind()), &redact(url))
        .map_err(|e| fail(&format!("error: {e}")))?;
    for l in lines {
        eprintln!("{l}");
    }
    Ok(())
}

fn db_url(db: Option<&str>) -> String {
    db.map(String::from)
        .or_else(|| std::env::var("OBOP_DATABASE_URL").ok())
        .unwrap_or_else(|| "sqlite://obop.db".into())
}

/// Open an existing database for an operator command. A SQLite path that does not exist is an
/// error (opening it would quietly create an empty database).
fn open_existing(url: &str) -> Result<Arc<dyn Backend>, ExitCode> {
    if let Some(path) = url
        .strip_prefix("sqlite://")
        .or_else(|| url.strip_prefix("sqlite:"))
        && !path.is_empty()
        && path != ":memory:"
        && !Path::new(path).exists()
    {
        return Err(fail(&format!("no database at {path}")));
    }
    obop_storage::open(url).map_err(|e| fail(&format!("cannot open the database: {e}")))
}

fn take(args: &mut Vec<String>, flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    args.remove(i);
    (i < args.len()).then(|| args.remove(i))
}

fn has(args: &mut Vec<String>, flag: &str) -> bool {
    let n = args.len();
    args.retain(|a| a != flag);
    args.len() != n
}

fn number(v: Option<String>, flag: &str) -> Result<Option<usize>, ExitCode> {
    v.map(|s| {
        s.parse::<usize>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| fail(&format!("{flag} takes a positive number")))
    })
    .transpose()
}

/// Handle `migrate`, `backup`, `restore` and `secrets`; `None` for any other command.
pub fn dispatch(args: &[String]) -> Option<ExitCode> {
    let first = args.first()?.as_str();
    if first == "platform" && args.get(1).map(String::as_str) == Some("upgrade-instances") {
        let mut rest: Vec<String> = args[2..].to_vec();
        return Some(upgrade_instances(&mut rest).map_or_else(|c| c, |()| ExitCode::SUCCESS));
    }
    if !matches!(first, "migrate" | "backup" | "restore" | "secrets") {
        return None;
    }
    let mut rest: Vec<String> = args[1..].to_vec();
    let r = match first {
        "migrate" => migrate(&mut rest),
        "backup" => backup_cmd(&mut rest),
        "restore" => restore_cmd(&mut rest),
        _ => secrets_cmd(&mut rest),
    };
    Some(r.map_or_else(|c| c, |()| ExitCode::SUCCESS))
}

/// `obop platform upgrade-instances`: the control plane's rolling upgrade (B6).
fn upgrade_instances(args: &mut Vec<String>) -> Exit {
    use obop_platform::{UpgradeOutcome, resolve_image};
    let url = take(args, "--db")
        .or_else(|| std::env::var("OBOP_DATABASE_URL").ok())
        .unwrap_or_else(|| "sqlite://obop-platform.db".into());
    let image_arg = take(args, "--image").ok_or_else(usage)?;
    let only = take(args, "--only");
    let provisioner = take(args, "--provisioner").unwrap_or_else(|| "sliplane".into());
    let dry_run = has(args, "--dry-run");
    if !args.is_empty() {
        return Err(usage());
    }
    let backend = open_existing(&url)?;
    guard_database(&backend, &url)?;
    let prov =
        crate::make_provisioner(&provisioner, None).map_err(|e| fail(&format!("error: {e}")))?;
    let image = resolve_image(&image_arg, std::env::var("OBOP_IMAGE").ok().as_deref());
    let auth = Arc::new(obop_auth::Auth::new(backend.clone()));
    let platform = obop_platform::Platform::new(auth, backend, prov);
    eprintln!(
        "{} to {image}",
        if dry_run {
            "planning the upgrade"
        } else {
            "upgrading"
        }
    );
    let report = platform
        .upgrade_instances(&image, only.as_deref(), dry_run, &|l| eprintln!("  {l}"))
        .map_err(|e| fail(&format!("error: {e}")))?;
    for s in &report.steps {
        let from = if s.from.is_empty() {
            "an unknown image".to_string()
        } else {
            s.from.clone()
        };
        let what = match &s.outcome {
            UpgradeOutcome::Upgraded => format!("upgraded (was {from})"),
            UpgradeOutcome::WouldUpgrade => format!("would be upgraded (is on {from})"),
            UpgradeOutcome::Skipped(w) => format!("skipped: {w}"),
            UpgradeOutcome::Failed(e) => format!("FAILED: {e}"),
            UpgradeOutcome::NotAttempted => "not attempted: an earlier instance failed".to_string(),
        };
        println!("{}: {what}", s.slug);
    }
    if report.steps.is_empty() {
        println!("no instances");
    }
    if let Some(f) = report
        .steps
        .iter()
        .find(|s| matches!(s.outcome, UpgradeOutcome::Failed(_)))
    {
        eprintln!(
            "stopped at {}. To go back: obop platform upgrade-instances --image {} --only {} (a schema migration that already ran is not undone: restore a backup if the older version refuses the database; docs/DEPLOY.md)",
            f.slug,
            if f.from.is_empty() {
                "<previous image>"
            } else {
                &f.from
            },
            f.slug
        );
        return Err(ExitCode::FAILURE);
    }
    Ok(())
}

fn migrate(args: &mut Vec<String>) -> Exit {
    let url = db_url(take(args, "--db").as_deref());
    let dry = has(args, "--dry-run");
    let verb = args.first().map(String::as_str);
    if args.len() > 1
        || !matches!(verb, Some("status" | "plan" | "run"))
        || (dry && verb == Some("status"))
    {
        return Err(usage());
    }
    let backend = open_existing(&url)?;
    let ring = KeyRing::for_backend_read_only(&backend).map_err(|e| fail(&e))?;
    let m = Migrator::new(backend, ring);
    let st = m.status().map_err(|e| fail(&e))?;
    match verb {
        Some("status") => {
            println!("{}", st.describe());
            for (v, name) in &st.pending {
                println!("  pending {v}: {name}");
            }
            Ok(())
        }
        Some(v) => {
            let dry = dry || v == "plan";
            let r = m.run(dry).map_err(|e| fail(&format!("error: {e}")))?;
            if r.applied.is_empty() {
                println!("{}", st.describe());
            }
            for (v, name, what) in &r.applied {
                println!(
                    "{} {v}: {name} ({what})",
                    if dry { "would run" } else { "ran" }
                );
            }
            if !dry && !r.applied.is_empty() {
                println!("done: {}", m.status().map_err(|e| fail(&e))?.describe());
            }
            Ok(())
        }
        None => Err(usage()),
    }
}

fn backup_cmd(args: &mut Vec<String>) -> Exit {
    if args.first().map(String::as_str) == Some("verify") {
        if args.len() != 2 {
            return Err(usage());
        }
        return match backup::verify(Path::new(&args[1])) {
            Ok(v) => {
                let m = v.manifest;
                println!(
                    "ok: format {}, written by obop {} at schema version {}, {} records, {}",
                    m.format, m.core_version, m.schema_version, m.records, m.created_at
                );
                for (model, n) in &m.models {
                    println!("  {model}: {n}");
                }
                Ok(())
            }
            Err(e) => Err(fail(&format!("FAILED: {e}"))),
        };
    }
    let url = db_url(take(args, "--db").as_deref());
    let out = take(args, "--out").ok_or_else(usage)?;
    if !args.is_empty() {
        return Err(usage());
    }
    let backend = open_existing(&url)?;
    let ring = KeyRing::for_backend_read_only(&backend).map_err(|e| fail(&e))?;
    let st = Migrator::new(backend.clone(), ring)
        .status()
        .map_err(|e| fail(&e))?;
    let schema = u64::from(st.stored.unwrap_or(0));
    let mut last = 0usize;
    let c = backup::create(&backend, schema, Path::new(&out), &mut |n| {
        if n / 10_000 > last / 10_000 {
            eprintln!("  {n} records");
        }
        last = n;
    })
    .map_err(|e| fail(&format!("backup failed: {e}")))?;
    // A backup is only a backup once it reads back.
    backup::verify(&c.data_path)
        .map_err(|e| fail(&format!("the backup was written but does not verify: {e}")))?;
    println!(
        "backup of {} written: {} ({} records, {} bytes), manifest {}",
        redact(&url),
        c.data_path.display(),
        c.manifest.records,
        c.manifest.data_bytes,
        c.manifest_path.display()
    );
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

fn restore_cmd(args: &mut Vec<String>) -> Exit {
    let url = db_url(take(args, "--db").as_deref());
    let from = take(args, "--from").ok_or_else(usage)?;
    let force = has(args, "--force");
    let dry_run = has(args, "--dry-run");
    if !args.is_empty() {
        return Err(usage());
    }
    let backend =
        obop_storage::open(&url).map_err(|e| fail(&format!("cannot open the database: {e}")))?;
    let latest = obop_migrate::steps().len() as u64;
    let r = backup::restore(
        &backend,
        latest,
        Path::new(&from),
        RestoreOptions { force, dry_run },
        &mut |n| {
            if n % 10_000 < 1000 {
                eprintln!("  {n} records");
            }
        },
    )
    .map_err(|e| fail(&format!("restore refused or failed: {e}")))?;
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

fn secrets_cmd(args: &mut Vec<String>) -> Exit {
    let url = db_url(take(args, "--db").as_deref());
    let dry_run = has(args, "--dry-run");
    let skip = has(args, "--skip-unopenable");
    let batch = number(take(args, "--batch"), "--batch")?;
    let max_batches = number(take(args, "--max-batches"), "--max-batches")?;
    let verb = args.first().cloned();
    if args.len() != 1 {
        return Err(usage());
    }
    let backend = open_existing(&url)?;
    match verb.as_deref() {
        Some("status") => {
            let ring = KeyRing::for_backend_read_only(&backend).map_err(|e| fail(&e))?;
            print!(
                "{}",
                obop_ops::secrets::status(&backend, &ring).map_err(|e| fail(&e))?
            );
            Ok(())
        }
        Some("rotate") => {
            // Rotation needs a current key from the environment: the generated key kept in the
            // database is not something to rotate to.
            let ring = KeyRing::for_backend_read_only(&backend).map_err(|e| fail(&e))?;
            if !std::env::var("OBOP_SECRET_KEY").is_ok_and(|k| !k.trim().is_empty()) {
                return Err(fail(
                    "error: set OBOP_SECRET_KEY to the new key (and OBOP_SECRET_KEY_PREVIOUS to the old one); see docs/DEPLOY.md",
                ));
            }
            let opts = ResealOptions {
                dry_run,
                skip_unopenable: skip,
                batch: batch.unwrap_or(200),
                max_batches,
                only_legacy: false,
            };
            let r = obop_ops::secrets::rotate(&backend, &ring, opts, &mut |l| eprintln!("  {l}"))
                .map_err(|e| fail(&format!("error: {e}")))?;
            println!(
                "{} sealed values: {} already under the current key, {} {}",
                r.scanned,
                r.current,
                if dry_run { r.to_reseal } else { r.resealed },
                if dry_run {
                    "to re-seal (dry run, nothing written)"
                } else {
                    "re-sealed"
                }
            );
            for (k, n) in &r.by_key_before {
                println!("  before: {n} under {k}");
            }
            if r.changed_meanwhile > 0 {
                println!(
                    "{} value(s) were changed by the running server meanwhile; run again to finish them",
                    r.changed_meanwhile
                );
            }
            if r.stopped_early {
                println!(
                    "stopped after {} batch(es) as asked; run again to continue",
                    r.batches
                );
            }
            for p in &r.unopenable {
                println!("skipped, cannot be opened: {} ({})", p.place, p.why);
            }
            if r.complete() && !dry_run {
                println!(
                    "done: everything is under the current key; OBOP_SECRET_KEY_PREVIOUS can be dropped at the next deploy"
                );
            }
            if r.complete() || dry_run {
                Ok(())
            } else {
                Err(ExitCode::FAILURE)
            }
        }
        _ => Err(usage()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    /// A platform database with one running instance, as `platform serve --provisioner fake`
    /// leaves it.
    fn platform_db() -> (std::path::PathBuf, String) {
        let path = std::env::temp_dir().join(format!("obop-ops-{}.db", std::process::id()));
        let url = format!("sqlite://{}", path.display());
        let backend = obop_storage::open(&url).unwrap();
        let auth = Arc::new(obop_auth::Auth::with_test_params(backend.clone()));
        let p = obop_platform::Platform::new(
            auth,
            backend,
            Arc::new(obop_platform::provision::Fake::default()),
        );
        let t = p
            .signup("ops@example.test", "correct horse battery")
            .unwrap();
        let i = p.create_instance(&t, "alpha-one").unwrap();
        for _ in 0..200 {
            if p.get(&t, &i.id).unwrap().status == "running" {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        (path, url)
    }

    #[test]
    fn upgrade_instances_runs_through_the_cli_and_checks_its_arguments() {
        let (path, url) = platform_db();
        let base = [
            "platform",
            "upgrade-instances",
            "--provisioner",
            "fake",
            "--db",
            url.as_str(),
        ];
        let run = |extra: &[&str]| {
            let mut v = args(&base);
            v.extend(extra.iter().map(|s| s.to_string()));
            dispatch(&v).unwrap()
        };
        assert_eq!(run(&["--image", "abc123", "--dry-run"]), ExitCode::SUCCESS);
        assert_eq!(run(&["--image", "abc123"]), ExitCode::SUCCESS);
        // Without an image, or for an instance that does not exist: no success.
        assert_ne!(run(&[]), ExitCode::SUCCESS);
        assert_ne!(
            run(&["--image", "x", "--only", "nobody"]),
            ExitCode::SUCCESS
        );
        assert!(
            dispatch(&args(&["platform", "serve"])).is_none(),
            "the rest of platform is main's"
        );
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }

    #[test]
    fn urls_in_messages_carry_no_password() {
        assert_eq!(
            redact("postgres://u:secret@host:5432/db"),
            "postgres://***@host:5432/db"
        );
        assert_eq!(redact("sqlite://obop.db"), "sqlite://obop.db");
    }
}
