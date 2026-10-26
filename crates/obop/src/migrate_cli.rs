//! `obop migrate status|verify|plan|run|rehearse|rollback` (B5, D59, D73): the schema version, the
//! migration ledger, and the gates around running a migration (OBR-DAT-051, OBR-DAT-053).

use crate::backup_cli::{backup_keys_for, exit_for};
use crate::ops::{
    Exit, db_url, fail, has, open_existing, operator_name, ring_for, server_info, take, usage,
};
use obop_keys::KeyRing;
use obop_migrate::Migrator;
use obop_ops::drill::Scratch;
use obop_ops::gates::{self, Gate, Mode, Options, RehearseOptions};
use obop_ops::schedule::{Clock, SystemClock};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

/// One ledger row on one line.
fn ledger_line(r: &obop_migrate::Entry) -> String {
    format!(
        "{} {} {} [{}, ddl {}] {} by {} {}ms sha256:{}{}{}",
        obop_ops::backup::rfc3339(r.at),
        r.id,
        r.status.as_str(),
        r.class.as_str(),
        r.ddl.as_str(),
        r.name,
        r.by,
        r.duration_ms,
        &r.checksum[..r.checksum.len().min(12)],
        r.backup_id
            .as_deref()
            .map_or(String::new(), |b| format!(" backup {b}")),
        if r.note.is_empty() {
            String::new()
        } else {
            format!(" ({})", r.note)
        }
    )
}

/// The gate mode: `--gates strict|relaxed`, else `OBOP_MIGRATION_GATES`, else strict on
/// PostgreSQL and relaxed elsewhere.
fn mode(flag: Option<String>, kind: &str) -> Result<Mode, ExitCode> {
    let chosen = flag.or_else(|| std::env::var("OBOP_MIGRATION_GATES").ok());
    match chosen.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(Mode::for_backend(kind)),
        Some(s) => Mode::parse(s).ok_or_else(|| {
            fail(&format!(
                "--gates and OBOP_MIGRATION_GATES take strict or relaxed, not {s:?}"
            ))
        }),
    }
}

fn backup_dir(flag: Option<String>) -> Option<PathBuf> {
    flag.or_else(|| std::env::var("OBOP_BACKUP_DIR").ok())
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
}

pub fn migrate(args: &mut Vec<String>) -> Exit {
    let db_flag = take(args, "--db");
    // Whether the database was named at all (here or in the environment): without it a command
    // that needs the server key reads it from the environment only.
    let db_given = db_flag.is_some() || std::env::var("OBOP_DATABASE_URL").is_ok();
    let url = db_url(db_flag.as_deref());
    let verb = args.first().cloned();
    match verb.as_deref() {
        Some("rehearse") => {
            args.remove(0);
            return rehearse(&url, db_given, args);
        }
        Some("rollback") => {
            args.remove(0);
            return rollback(&url, args);
        }
        _ => {}
    }
    let dry = has(args, "--dry-run");
    let gates_flag = take(args, "--gates");
    let backup_file = take(args, "--backup");
    let backup_dir_flag = take(args, "--backup-dir");
    let receipt = take(args, "--rehearsal");
    let window = has(args, "--window");
    let disk_ok = has(args, "--disk-ok");
    let keys_for_run = backup_keys_for(args);
    if args.len() != 1
        || !matches!(verb.as_deref(), Some("status" | "verify" | "plan" | "run"))
        || (dry && matches!(verb.as_deref(), Some("status" | "verify")))
    {
        return Err(usage());
    }
    let backend = open_existing(&url)?;
    let ring = KeyRing::for_backend_read_only(&backend).map_err(|e| fail(&e))?;
    let m = Migrator::new(backend.clone(), ring.clone());
    let st = m.status().map_err(|e| fail(&e))?;
    match verb.as_deref() {
        Some("status") => {
            println!("{}", st.describe());
            for (v, name) in &st.pending {
                println!("  pending {v}: {name}");
            }
            let rows = m.ledger().map_err(|e| fail(&e))?;
            if rows.is_empty() {
                println!(
                    "ledger: empty (a database from before the ledger is adopted by the next start or run)"
                );
            } else {
                println!("ledger ({} rows):", rows.len());
                for r in &rows {
                    println!("  {}", ledger_line(r));
                }
            }
            Ok(())
        }
        Some("verify") => {
            let v = m.verify().map_err(|e| fail(&e))?;
            for n in &v.notes {
                println!("note: {n}");
            }
            for p in &v.problems {
                println!("PROBLEM: {p}");
            }
            if v.ok() {
                println!("ledger, code and schema version agree");
                Ok(())
            } else {
                Err(ExitCode::FAILURE)
            }
        }
        Some("plan") => {
            let plan = m.plan().map_err(|e| fail(&format!("error: {e}")))?;
            print!("{}", plan.describe());
            if plan.is_empty() {
                // Nothing to store, and nothing is written to the database.
            } else if dry {
                println!(
                    "plan {} (not stored: --dry-run writes nothing; run without it to store the plan a production run needs)",
                    plan.id
                );
            } else {
                m.store_plan(&plan)
                    .map_err(|e| fail(&format!("error: {e}")))?;
                println!(
                    "plan {} stored in the database: a production run needs this exact plan (run `obop migrate rehearse` on a backup of this database, then `obop migrate run --backup FILE --rehearsal RECEIPT --disk-ok`)",
                    plan.id
                );
            }
            Ok(())
        }
        _ => {
            let keys = keys_for_run.map_err(|e| fail(&e))?;
            let gate = Gate {
                backend: backend.clone(),
                ring,
                blobs: crate::ops::blob_catalog(&backend, &url)
                    .ok()
                    .map(|c| Arc::new(c) as Arc<dyn obop_storage::blob::Blobs>),
                server: server_info(&url, backend.kind()),
                clock: Arc::new(SystemClock),
                options: Options {
                    mode: mode(gates_flag, backend.kind())?,
                    actor: operator_name(),
                    backup_dir: backup_dir(backup_dir_flag),
                    backup_file: backup_file.map(PathBuf::from),
                    keys,
                    receipt: receipt.map(PathBuf::from),
                    window,
                    disk_attested: disk_ok,
                    url: url.clone(),
                    lock_wait: obop_ops::gates::LOCK_WAIT,
                    window_rows: None,
                },
                steps: None,
            };
            let gate_mode = gate.options.mode;
            let ran = gate.run(dry, &mut |l| eprintln!("{l}")).map_err(|e| {
                // The same stable statuses as `backup verify` and `restore`: a gate refused for a
                // key that is not configured (3) or a plain backup where a key is (4).
                eprintln!("error: {e}");
                exit_for(obop_ops::backup::code_in(&e))
            })?;
            if ran.plan.is_empty() {
                println!("{}", st.describe());
                return Ok(());
            }
            for c in &ran.checks {
                if c.level != obop_ops::doctor::Level::Pass {
                    eprintln!("{:>5}  {}: {}", c.level.as_str(), c.name, c.detail);
                }
            }
            if dry {
                // A preview: the plan, what each step would do, and which checks would refuse a
                // real run. It needs no gate and writes nothing.
                println!("{}", ran.plan.describe().trim_end());
                for (v, name, what) in &ran.applied {
                    println!("would run {v}: {name} ({what})");
                }
                if ran.would_refuse.is_empty() {
                    println!("dry run: every gate passed or only warned; nothing was taken or run");
                } else {
                    println!(
                        "dry run: a real run would be REFUSED by these checks (nothing was taken or run):"
                    );
                    for l in &ran.would_refuse {
                        println!("{l}");
                    }
                }
                return Ok(());
            }
            crate::ops::record_event(
                &backend,
                "migration_run",
                &format!(
                    "ran {} step(s) under gates {}; backup {}; restore point {}",
                    ran.applied.len(),
                    if gate_mode == obop_ops::gates::Mode::Strict {
                        "strict"
                    } else {
                        "relaxed"
                    },
                    ran.backup.as_ref().map_or("none", |b| b.id.as_str()),
                    ran.restore_point.as_deref().unwrap_or("none")
                ),
            );
            for (v, name, what) in &ran.applied {
                println!("ran {v}: {name} ({what})");
            }
            if let Some(b) = &ran.backup {
                println!(
                    "backup before the run: {} (recorded in the ledger)",
                    b.path.display()
                );
            }
            if let Some(p) = &ran.restore_point {
                println!("restore point: {p}");
            }
            println!("done: {}", m.status().map_err(|e| fail(&e))?.describe());
            Ok(())
        }
    }
}

/// `obop migrate rehearse --from BACKUP [--scratch URL] [--db URL] [--out FILE] [--allow-plain]`.
fn rehearse(url: &str, db_given: bool, args: &mut Vec<String>) -> Exit {
    let from = take(args, "--from").ok_or_else(usage)?;
    let scratch = take(args, "--scratch");
    let out = take(args, "--out");
    let keys = backup_keys_for(args).map_err(|e| fail(&e))?;
    if !args.is_empty() {
        return Err(usage());
    }
    let ring = ring_for(db_given.then_some(url));
    if ring.current().is_none() {
        return Err(fail(
            "error: a rehearsal receipt is signed with the server key: set OBOP_SECRET_KEY (the one the database uses)",
        ));
    }
    let now = SystemClock.now();
    let r = gates::rehearse(&RehearseOptions {
        backup: PathBuf::from(&from),
        keys,
        ring: ring.clone(),
        scratch: scratch.map_or(Scratch::TempSqlite, |u| Scratch::Url {
            url: u,
            keep: false,
        }),
        avoid: Some(url.to_string()),
        now,
        steps: None,
    })
    .map_err(|e| crate::backup_cli::failure_of("error", &e))?;
    print!("{}", r.render());
    if let Some(out) = out {
        let j = r
            .receipt(now)
            .sign(&ring)
            .map_err(|e| fail(&format!("error: {e}")))?;
        std::fs::write(&out, format!("{j:#}\n"))
            .map_err(|e| fail(&format!("cannot write {out}: {e}")))?;
        println!("receipt written to {out}: pass it to `obop migrate run --rehearsal {out}`");
    }
    if r.passed {
        Ok(())
    } else {
        Err(ExitCode::FAILURE)
    }
}

/// `obop migrate rollback --from BACKUP --reason TEXT --yes [--other-database] [--allow-plain] [--db URL]`.
fn rollback(url: &str, args: &mut Vec<String>) -> Exit {
    let from = take(args, "--from").ok_or_else(usage)?;
    let reason = take(args, "--reason").ok_or_else(usage)?;
    let yes = has(args, "--yes");
    let other_database = has(args, "--other-database");
    let keys = backup_keys_for(args).map_err(|e| fail(&e))?;
    if !args.is_empty() {
        return Err(usage());
    }
    let backend = open_existing(url)?;
    let ring = KeyRing::for_backend_read_only(&backend).unwrap_or_default();
    let file = Path::new(&from);
    let v = obop_ops::backup::verify_with(file, &keys)
        .map_err(|e| crate::backup_cli::failure_of("the backup does not verify", &e))?;
    println!(
        "rollback: replace {} with the backup {} (schema version {}, taken {}, {} records)",
        crate::ops::redact(url),
        file.display(),
        v.manifest.schema_version,
        v.manifest.created_at,
        v.manifest.records
    );
    if !yes {
        println!(
            "this replaces everything in the database with the backup. Stop every process first, then pass --yes"
        );
        return Err(ExitCode::from(2));
    }
    let blobs = crate::ops::blob_catalog(&backend, url)?;
    let done = gates::rollback(
        &backend,
        Some(&blobs),
        &ring,
        file,
        &keys,
        &reason,
        &operator_name(),
        other_database,
        &SystemClock,
    )
    .map_err(|e| crate::backup_cli::failure_of("rollback refused or failed", &e))?;
    println!(
        "rolled back: {} records restored from {}; schema version {} became {}; the reason is in the ledger and the security log",
        done.records,
        done.backup_id,
        done.schema_before.map_or("?".into(), |v| v.to_string()),
        done.schema_after.map_or("none".into(), |v| v.to_string()),
    );
    println!(
        "{}",
        if done.authenticated {
            "the backup was opened with its key and authenticated"
        } else {
            "the backup is NOT authenticated (plain, format 1): it was only checked against its checksums"
        }
    );
    if done.unnamed {
        eprintln!(
            "note: the backup carries no database id (it was made before databases had one), so it could not be checked to belong to this database; this database keeps its own id"
        );
    }
    println!(
        "start the obop version that matches the backup's schema, then check `obop migrate status`"
    );
    Ok(())
}
