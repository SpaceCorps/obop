//! `obop drill` and `obop storage check|attest|show` (OBR-DAT-048, 049, 056, D73): the restore
//! drill, the checks of the host's configuration and evidence, and the attestation that storage is
//! encrypted at rest.

use crate::backup_cli::{backup_keys_for, exit_for};
use crate::ops::{
    Exit, db_url, fail, has, open_existing, operator_name, record_event, ring_for, server_info,
    take, usage,
};
use obop_ops::catalog::{Attestation, Catalog, DrillCheck, DrillRecord};
use obop_ops::doctor::{self, Level};
use obop_ops::drill::{self, Scratch};
use obop_ops::schedule::{Clock, SystemClock};
use std::path::PathBuf;
use std::process::ExitCode;

/// `obop drill BACKUP [--scratch URL] [--keep] [--migrate] [--record-in URL] [--plane NAME]
/// [--allow-plain] [--record-later FILE]`, and `obop drill --record-from FILE --server
/// ok|skipped|failed --record-in URL`.
///
/// Restores BACKUP into a scratch database, checks the copy and exits non-zero unless every check
/// passed (status 3 when a sealed backup's key is not configured, 4 for a plain backup offered to
/// an instance that seals its own). `--record-in` writes the result (with the measured recovery
/// point and time) into that database, where `obop storage check` and the metrics read it, and
/// is also the database the drill refuses to restore into.
///
/// `--record-later FILE` keeps the result in a file instead of recording it, for a script that
/// has one more check to make (a server on the copy): `--record-from FILE --server ...` then
/// adds that check to the result and records it, so a drill whose last check failed is never in
/// the evidence as a pass.
pub fn drill_cmd(args: &mut Vec<String>) -> Exit {
    if let Some(file) = take(args, "--record-from") {
        return record_from(&file, args);
    }
    let scratch = take(args, "--scratch");
    let keep = has(args, "--keep");
    let migrate = has(args, "--migrate");
    let record_in = take(args, "--record-in");
    let record_later = take(args, "--record-later");
    let plane = take(args, "--plane").unwrap_or_else(|| "main".into());
    let db = take(args, "--db");
    let keys = backup_keys_for(args).map_err(|e| fail(&e))?;
    if args.len() != 1 || (record_later.is_some() && record_in.is_some()) {
        return Err(usage());
    }
    let backup = PathBuf::from(&args[0]);
    let production = record_in.clone().or(db);
    let ring = ring_for(production.as_deref());
    let now = SystemClock.now();
    let scratch = match scratch.as_deref() {
        None | Some("temp") => Scratch::TempSqlite,
        Some(u) => Scratch::Url {
            url: u.to_string(),
            keep,
        },
    };
    let report = drill::run(&drill::Options {
        backup,
        keys,
        ring,
        scratch,
        avoid: production.clone(),
        plane,
        now,
        migrate,
    })
    .map_err(|e| fail(&format!("error: {e}")))?;
    print!("{}", report.render());
    if let Some(file) = record_later {
        let j = serde_json::json!({"id": report.record.id, "fields": report.record.to_fields()});
        std::fs::write(&file, format!("{j:#}\n"))
            .map_err(|e| fail(&format!("cannot write {file}: {e}")))?;
        println!(
            "the result is kept in {file}, not recorded yet: `obop drill --record-from {file} --server ok|skipped|failed --record-in URL` records it"
        );
    }
    if let Some(url) = record_in {
        record(&url, &report.record)?;
    }
    if report.passed() {
        Ok(())
    } else {
        Err(exit_for(report.error_code()))
    }
}

fn record(url: &str, rec: &DrillRecord) -> Exit {
    let b = open_existing(url)?;
    Catalog::new(&*b)
        .record_drill(rec)
        .map_err(|e| fail(&format!("could not record the drill: {e}")))?;
    record_event(
        &b,
        "drill_recorded",
        &format!(
            "restore drill of {} recorded as {}: {}",
            rec.backup_id,
            rec.id,
            if rec.passed { "passed" } else { "FAILED" }
        ),
    );
    println!("recorded as {} in {}", rec.id, crate::ops::redact(url));
    Ok(())
}

/// `obop drill --record-from FILE --server ok|skipped|failed --record-in URL`.
fn record_from(file: &str, args: &mut Vec<String>) -> Exit {
    let server = take(args, "--server").ok_or_else(usage)?;
    let url = take(args, "--record-in").ok_or_else(usage)?;
    let detail = take(args, "--detail").unwrap_or_default();
    if !args.is_empty() || !matches!(server.as_str(), "ok" | "skipped" | "failed") {
        return Err(usage());
    }
    let text =
        std::fs::read_to_string(file).map_err(|e| fail(&format!("cannot read {file}: {e}")))?;
    let j: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| fail(&format!("{file} is not a drill result: {e}")))?;
    let id = j["id"].as_str().unwrap_or_default();
    let fields = j["fields"].as_object().cloned().unwrap_or_default();
    let mut rec = DrillRecord::from_fields(id, &fields)
        .ok_or_else(|| fail(&format!("{file} is not a drill result")))?;
    rec.checks.push(DrillCheck {
        name: "server".into(),
        ok: server != "failed",
        detail: match (server.as_str(), detail.is_empty()) {
            ("ok", _) => "a server started on the copy and answered".to_string(),
            ("skipped", _) => "not checked: the server check was skipped".to_string(),
            (_, true) => "a server on the copy did not start or did not answer".to_string(),
            (_, false) => detail,
        },
    });
    rec.passed = rec.checks.iter().all(|c| c.ok);
    record(&url, &rec)?;
    if rec.passed {
        Ok(())
    } else {
        Err(ExitCode::FAILURE)
    }
}

fn query_param<'a>(url: &'a str, name: &str) -> Option<&'a str> {
    let q = url.split_once('?')?.1;
    q.split('&')
        .find_map(|kv| kv.strip_prefix(name)?.strip_prefix('='))
}

/// Whether the host of a database URL is this machine or a name that does not leave a private
/// network.
fn host_is_local(url: &str) -> bool {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let rest = rest.rsplit_once('@').map_or(rest, |(_, r)| r);
    let host = rest.split(['/', '?']).next().unwrap_or("");
    let host = if let Some(h) = host.strip_prefix('[') {
        h.split(']').next().unwrap_or("")
    } else {
        host.rsplit_once(':').map_or(host, |(h, _)| h)
    };
    // A file or memory database has no network path.
    url.starts_with("sqlite:")
        || url.starts_with("memory:")
        || host.is_empty()
        || matches!(host, "localhost" | "127.0.0.1" | "::1")
        || host.ends_with(".localhost")
        || host.ends_with(".internal")
        || host.ends_with(".local")
        || host.ends_with(".svc")
        || !host.contains('.')
}

pub fn storage_cmd(args: &mut Vec<String>) -> Exit {
    let verb = args.first().cloned();
    if verb.is_none() {
        return Err(usage());
    }
    args.remove(0);
    let url = db_url(take(args, "--db").as_deref());
    match verb.as_deref() {
        Some("check") => {
            let strict = has(args, "--strict");
            let plane = take(args, "--plane").unwrap_or_else(|| "main".into());
            if !args.is_empty() {
                return Err(usage());
            }
            let backend = open_existing(&url)?;
            let server = server_info(&url, backend.kind());
            let strays = obop_ops::schedule::sink_strays(&crate::backup_cli::configured_sinks());
            let report = doctor::check(&doctor::Inputs {
                backend: &*backend,
                production: backend.kind() == "postgres",
                server: server.as_deref(),
                sslmode: query_param(&url, "sslmode"),
                host_is_local: host_is_local(&url),
                plane: &plane,
                now: SystemClock.now(),
                sink_strays: &strays,
            })
            .map_err(|e| fail(&format!("error: {e}")))?;
            print!("{}", report.render());
            let worst = report.worst();
            println!(
                "{}",
                match worst {
                    Level::Pass => "every check passed",
                    Level::Warn => "passed, with warnings",
                    Level::Fail =>
                        "FAILED: a promise of docs/DEPLOY.md cannot be kept as configured",
                }
            );
            if worst == Level::Fail || (strict && worst == Level::Warn) {
                Err(ExitCode::FAILURE)
            } else {
                Ok(())
            }
        }
        Some("attest") => {
            fn need(args: &mut Vec<String>, flag: &str) -> Result<String, ExitCode> {
                take(args, flag).ok_or_else(|| {
                    fail(&format!(
                        "error: {flag} is required; see `obop storage attest` in docs/DEPLOY.md"
                    ))
                })
            }
            let plane = take(args, "--plane").unwrap_or_else(|| "main".into());
            let attested_by = take(args, "--by").unwrap_or_else(operator_name);
            let a = Attestation {
                plane,
                host: need(args, "--host")?,
                provider: need(args, "--provider")?,
                mechanism: need(args, "--mechanism")?,
                key_custody: need(args, "--custody")?,
                key_readers: need(args, "--readers")?,
                evidence: need(args, "--evidence")?,
                attested_by,
                at: SystemClock.now(),
            };
            if !args.is_empty() {
                return Err(usage());
            }
            for (name, v) in [
                ("--host", &a.host),
                ("--provider", &a.provider),
                ("--mechanism", &a.mechanism),
                ("--custody", &a.key_custody),
                ("--readers", &a.key_readers),
                ("--evidence", &a.evidence),
                ("--by", &a.attested_by),
            ] {
                if v.trim().is_empty() || v.len() > 300 || v.chars().any(char::is_control) {
                    return Err(fail(&format!(
                        "error: {name} must be 1 to 300 characters on one line"
                    )));
                }
            }
            if !a.mechanism.contains("256") && !a.mechanism.to_ascii_lowercase().contains("chacha")
            {
                return Err(fail(
                    "error: --mechanism must name the cipher and key size (AES-256 or stronger), for example \"provider-managed volume encryption, AES-256\"",
                ));
            }
            let backend = open_existing(&url)?;
            Catalog::new(&*backend)
                .attest(&a)
                .map_err(|e| fail(&format!("error: {e}")))?;
            record_event(
                &backend,
                "storage_attested",
                &format!(
                    "plane {} on {}: {}; keys held by {}; attested by {}",
                    a.plane, a.host, a.mechanism, a.key_custody, a.attested_by
                ),
            );
            println!(
                "attested by {}: plane {} on {} ({}) is encrypted at rest with {}; keys held by {}, readable by {}; evidence: {}. This is an attestation, not a verification: obop does not look. Renew within 12 months and after any host change.",
                a.attested_by,
                a.plane,
                a.host,
                a.provider,
                a.mechanism,
                a.key_custody,
                a.key_readers,
                a.evidence
            );
            Ok(())
        }
        Some("show") => {
            if !args.is_empty() {
                return Err(usage());
            }
            let backend = open_existing(&url)?;
            let c = Catalog::new(&*backend);
            let atts = c.attestations().map_err(|e| fail(&e))?;
            if atts.is_empty() {
                println!("attestations: none (obop storage attest ...)");
            }
            for a in atts {
                println!(
                    "attestation {}: {} on {} ({}), keys {}, readable by {}, {}, by {}, evidence {}",
                    a.plane,
                    a.mechanism,
                    a.host,
                    a.provider,
                    a.key_custody,
                    a.key_readers,
                    obop_ops::backup::rfc3339(a.at),
                    a.attested_by,
                    a.evidence
                );
            }
            let drills = c.drills().map_err(|e| fail(&e))?;
            if drills.is_empty() {
                println!("restore drills: none (obop drill BACKUP --record-in URL)");
            }
            for d in drills {
                println!(
                    "drill {} {}: backup {}, measured recovery point {}, recovery time {}: {}",
                    obop_ops::backup::rfc3339(d.at),
                    d.plane,
                    d.backup_id,
                    doctor::human(d.rpo_seconds),
                    doctor::human(d.rto_seconds),
                    if d.passed { "passed" } else { "FAILED" }
                );
            }
            Ok(())
        }
        _ => Err(usage()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_read_for_their_host_and_sslmode() {
        for (u, local) in [
            ("postgres://u:p@localhost:5432/db", true),
            ("postgres://u@127.0.0.1/db", true),
            ("postgres://u@[::1]:5432/db", true),
            ("postgres://u@obop-postgres.internal:5432/db", true),
            ("postgres://u@db/obop", true),
            (
                "postgres://u@db.example.com/obop?sslmode=verify-full",
                false,
            ),
            ("postgres://u:p@10.0.0.5/obop", false),
            ("sqlite://obop.db", true),
        ] {
            assert_eq!(host_is_local(u), local, "{u}");
        }
        assert_eq!(
            query_param("postgres://h/db?sslmode=require&x=1", "sslmode"),
            Some("require")
        );
        assert_eq!(
            query_param("postgres://h/db?a=1&sslmode=verify-ca", "sslmode"),
            Some("verify-ca")
        );
        assert_eq!(query_param("postgres://h/db", "sslmode"), None);
    }
}
