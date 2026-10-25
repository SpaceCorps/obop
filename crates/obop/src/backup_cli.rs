//! `obop backup`, `obop backup verify` and `obop restore` (B4, D59, D73): the command line of
//! `obop_ops::backup`, including the sealed (format 2) backups of OBR-DAT-046.

use crate::ops::{Exit, blob_catalog, db_url, fail, has, open_existing, redact, take, usage};
use obop_keys::KeyRing;
use obop_migrate::Migrator;
use obop_ops::backup::{self, Keys, RestoreOptions};
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

/// The process exit status of a restore or verify that needs a key that is not configured. Stable:
/// scripts test for it (OBR-DAT-046).
pub const EXIT_KEY_REQUIRED: u8 = 3;

/// The keys of this process for backups: `OBOP_BACKUP_KEY`, else the server key from the
/// environment (never a key generated into the database; see `Keys::from_env`).
pub fn backup_keys() -> Result<Keys, String> {
    let server = KeyRing::from_env()?;
    Keys::from_env(&server)
}

/// A failed backup or restore as an exit status: code 3 for a missing key, 1 otherwise.
fn failure(what: &str, e: &str) -> ExitCode {
    eprintln!("{what}: {e}");
    if backup::error_code(e).is_some() {
        ExitCode::from(EXIT_KEY_REQUIRED)
    } else {
        ExitCode::FAILURE
    }
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
                " (restores fine; take a new backup to get a sealed one)"
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

pub fn backup_cmd(args: &mut Vec<String>) -> Exit {
    if args.first().map(String::as_str) == Some("verify") {
        args.remove(0);
        let structure_only = has(args, "--structure-only");
        let canary = take(args, "--canary");
        if args.len() != 1 {
            return Err(usage());
        }
        let file = Path::new(&args[0]);
        let keys = backup_keys().map_err(|e| fail(&e))?;
        let verified = if structure_only {
            backup::verify_structure(file)
        } else {
            backup::verify_with(file, &keys)
        };
        let v = verified.map_err(|e| failure("FAILED", &e))?;
        println!("{}", describe(&v.manifest, v.authenticated, &keys));
        for (model, n) in &v.manifest.models {
            println!("  {model}: {n}");
        }
        if let Some(c) = canary {
            let data = file
                .to_string_lossy()
                .trim_end_matches(".manifest.json")
                .to_string();
            let hits = backup::count_in_file(Path::new(&data), c.as_bytes())
                .map_err(|e| fail(&format!("FAILED: {e}")))?;
            println!("canary scan: {hits} occurrence(s) of the given text in the file as stored");
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
    if !args.is_empty() {
        return Err(usage());
    }
    let backend =
        obop_storage::open(&url).map_err(|e| fail(&format!("cannot open the database: {e}")))?;
    let latest = obop_migrate::steps().len() as u64;
    let blobs = blob_catalog(&backend, &url)?;
    let keys = backup_keys().map_err(|e| fail(&e))?;
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
    .map_err(|e| failure("restore refused or failed", &e))?;
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
