//! The operator commands (B3 to B5, D59): `obop migrate`, `obop backup`, `obop restore` and
//! `obop secrets`, and the schema guard every command that opens a database goes through.
//! They work on a database given by `--db` (or `OBOP_DATABASE_URL`, default `sqlite://obop.db`)
//! and need no running server.

use obop_keys::KeyRing;
use obop_keys::reseal::Options as ResealOptions;
use obop_migrate::Migrator;
use obop_storage::Backend;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

pub(crate) type Exit = Result<(), ExitCode>;

pub(crate) fn fail(msg: &str) -> ExitCode {
    eprintln!("{msg}");
    ExitCode::FAILURE
}

pub(crate) fn usage() -> ExitCode {
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
pub(crate) fn redact(url: &str) -> String {
    match (url.find("://"), url.rfind('@')) {
        (Some(i), Some(at)) if at > i => format!("{}://***@{}", &url[..i], &url[at + 1..]),
        _ => url.to_string(),
    }
}

/// Called when a command opens the database: stamp an empty one with the current schema
/// version, refuse one from the future, and run (or demand) pending migration steps.
pub fn guard_database(backend: &Arc<dyn Backend>, url: &str) -> Exit {
    let ring = KeyRing::for_backend_read_only(backend).map_err(|e| fail(&e))?;
    let m = Migrator::new(backend.clone(), ring).by("server");
    m.stamp_if_fresh().map_err(|e| fail(&e))?;
    let auto = auto_migrate(backend.kind());
    let mut opts = obop_migrate::RunOptions::default();
    let st = m.status().map_err(|e| fail(&e))?;
    if auto && !st.pending.is_empty() && !st.ahead {
        // A server that migrates itself takes the verified backup first when it has a place to
        // put one, and a failed backup stops the migration (OBR-DAT-053). The backup is taken by
        // the migrator itself, under the migration lock, once the drift check and the plan say
        // that steps will run: a start that is refused takes none, two servers starting together
        // take one, and a crash loop reuses the backup of the attempt that failed.
        let dir = std::env::var("OBOP_BACKUP_DIR")
            .ok()
            .filter(|d| !d.trim().is_empty());
        let keys = crate::backup_cli::backup_keys().map_err(|e| fail(&e))?;
        let blobs = blob_catalog(backend, url)
            .ok()
            .map(|b| Arc::new(b) as Arc<dyn obop_storage::blob::Blobs>);
        let need_bytes = dir
            .as_ref()
            .and_then(|_| database_bytes(backend.kind(), url));
        opts.before = Some(obop_ops::gates::auto_backup_hook(
            backend.clone(),
            blobs,
            obop_ops::gates::AutoBackup {
                dir: dir.map(std::path::PathBuf::from),
                keys,
                need_bytes,
            },
            Arc::new(obop_ops::schedule::SystemClock),
            Arc::new(|l: &str| eprintln!("{l}")),
        ));
    }
    let lines = m
        .check_for_serve_with(auto, &redact(url), &opts)
        .map_err(|e| fail(&format!("error: {e}")))?;
    for l in lines {
        eprintln!("{l}");
    }
    Ok(())
}

/// About how big the database is, for a check that a backup volume has room: the size of the
/// SQLite file, or what PostgreSQL reports; `None` when it cannot be told (nothing is checked).
fn database_bytes(kind: &str, url: &str) -> Option<u64> {
    if kind == "postgres" {
        return obop_ops::pgadmin::Pg::connect(url)
            .ok()
            .and_then(|p| obop_ops::pgadmin::ServerInfo::database_bytes(&p).ok());
    }
    let path = url
        .strip_prefix("sqlite://")
        .or_else(|| url.strip_prefix("sqlite:"))?;
    std::fs::metadata(path).ok().map(|m| m.len())
}

/// The name an operator command puts in the ledger and the security log: `operator`, and the
/// operating system user when it can be told, since a command line has no sign-in of its own.
pub(crate) fn operator_name() -> String {
    ["USER", "USERNAME"]
        .iter()
        .find_map(|v| std::env::var(v).ok().filter(|s| !s.trim().is_empty()))
        .map_or_else(|| "operator".to_string(), |u| format!("operator:{u}"))
}

/// Write one line of the security log for an operator command that changes how the instance is
/// protected or what it holds (`migration_run`, `storage_attested`, `drill_recorded`). The actor
/// is the operating system user, as far as a command line can tell who ran it.
pub(crate) fn record_event(backend: &Arc<dyn Backend>, kind: &str, detail: &str) {
    obop_ops::audit_event(
        &**backend,
        obop_ops::schedule::Clock::now(&obop_ops::schedule::SystemClock),
        &operator_name(),
        kind,
        detail,
    );
}

pub(crate) fn db_url(db: Option<&str>) -> String {
    db.map(String::from)
        .or_else(|| std::env::var("OBOP_DATABASE_URL").ok())
        .unwrap_or_else(|| "sqlite://obop.db".into())
}

/// Open an existing database for an operator command. A SQLite path that does not exist is an
/// error (opening it would quietly create an empty database).
pub(crate) fn open_existing(url: &str) -> Result<Arc<dyn Backend>, ExitCode> {
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

pub(crate) fn take(args: &mut Vec<String>, flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    args.remove(i);
    (i < args.len()).then(|| args.remove(i))
}

pub(crate) fn has(args: &mut Vec<String>, flag: &str) -> bool {
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

/// `take`, for the other command modules.
pub fn take_flag(args: &mut Vec<String>, flag: &str) -> Option<String> {
    take(args, flag)
}

/// Handle `migrate`, `backup`, `restore` and `secrets`; `None` for any other command.
pub fn dispatch(args: &[String]) -> Option<ExitCode> {
    if let Some(code) = crate::update_cmd::dispatch(args) {
        return Some(code);
    }
    let first = args.first()?.as_str();
    if first == "platform" && args.get(1).map(String::as_str) == Some("upgrade-instances") {
        let mut rest: Vec<String> = args[2..].to_vec();
        return Some(upgrade_instances(&mut rest).map_or_else(|c| c, |()| ExitCode::SUCCESS));
    }
    if !matches!(
        first,
        "migrate" | "backup" | "restore" | "secrets" | "drill" | "storage"
    ) {
        return None;
    }
    let mut rest: Vec<String> = args[1..].to_vec();
    let r = match first {
        "migrate" => crate::migrate_cli::migrate(&mut rest),
        "backup" => crate::backup_cli::backup_cmd(&mut rest),
        "restore" => crate::backup_cli::restore_cmd(&mut rest),
        "drill" => crate::storage_cli::drill_cmd(&mut rest),
        "storage" => crate::storage_cli::storage_cmd(&mut rest),
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

/// The key ring of an operator command that may run away from the database: the environment,
/// else what the database holds (read only).
pub(crate) fn ring_for(url: Option<&str>) -> KeyRing {
    let env = KeyRing::from_env().unwrap_or_default();
    if !env.is_empty() {
        return env;
    }
    url.and_then(|u| obop_storage::open(u).ok())
        .and_then(|b| KeyRing::for_backend_read_only(&b).ok())
        .unwrap_or_default()
}

/// The PostgreSQL server behind `url` as the operator tools ask about it; `None` for any other
/// database, or when the server cannot be asked (a note says why).
pub(crate) fn server_info(url: &str, kind: &str) -> Option<Arc<dyn obop_ops::pgadmin::ServerInfo>> {
    if kind != "postgres" {
        return None;
    }
    match obop_ops::pgadmin::Pg::connect(url) {
        Ok(p) => Some(Arc::new(p)),
        Err(e) => {
            eprintln!("note: cannot ask the PostgreSQL server about itself: {e}");
            None
        }
    }
}

/// The store of uploaded pictures and files for the database at `url`: its records are in the
/// database, its bytes where `OBOP_BLOB_DIR` and the URL say (D60).
pub(crate) fn blob_catalog(
    backend: &std::sync::Arc<dyn obop_storage::Backend>,
    url: &str,
) -> Result<obop_storage::blob::Catalog, ExitCode> {
    let dir = std::env::var("OBOP_BLOB_DIR").ok();
    let bytes = obop_storage::blob::open_bytes(url, dir.as_deref())
        .map_err(|e| fail(&format!("cannot open the uploaded files: {e}")))?;
    Ok(obop_storage::blob::Catalog::new(backend.clone(), bytes))
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
