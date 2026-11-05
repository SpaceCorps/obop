//! SIE 4 export and import for the `accounting` models (docs/spec/accounting-se.md section 4).
//!
//! - `export`: a SIE file of the posted moves of one year, built from `accounting_account`,
//!   `accounting_move` and `accounting_move_line` (and the company name and organisation number
//!   of `accounting_se_company` when that package is installed).
//! - `import`: accounts, journals (one per voucher series), vouchers as draft moves with the
//!   series and number in `reference`, and optionally the real `accounting_move.post`.
//!
//! The same functions are behind the CLI (`obop sie export|import`) and the typed commands
//! `sie_exchange.export|import` (REST and MCP, [`command`]). The record layout of the file
//! itself is the `obop-sie` crate's, which has not been checked against the official SIE
//! specification.

pub(crate) mod command;
pub(crate) mod export;
pub(crate) mod import;

use obop_commands::{CallError, Dispatcher, Principal, Store, Surface, Value, Vault};
use serde_json::Value as J;
use std::collections::{HashMap, HashSet};

/// What the functions need of an instance: the dispatcher (every write goes through it, as the
/// caller) and the store (reads).
#[derive(Clone, Copy)]
pub struct Env<'a> {
    pub d: &'a Dispatcher,
    pub store: &'a Store,
}

/// The group that may export (read the books).
pub const EXPORT_GROUP: &str = "accounting.user";
/// The group that may import (create accounts and journals). Posting needs `accounting.approver`.
pub const IMPORT_GROUP: &str = "accounting.manager";
pub const APPROVER_GROUP: &str = "accounting.approver";

pub(crate) fn models_of(env: Env<'_>, p: &Principal) -> HashSet<String> {
    env.d
        .meta(Surface::Cli, p)
        .iter()
        .flat_map(|m| m.models.iter().map(|m| m.name.to_string()))
        .collect()
}

/// `(field value, record id)` of every record of a model.
pub(crate) fn existing(env: Env<'_>, model: &str, field: &str) -> HashMap<String, String> {
    env.store
        .backend()
        .list(model)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(id, f)| Some((f.get(field)?.as_str()?.to_string(), id)))
        .collect()
}

/// One command as the caller; the refusals in words.
pub(crate) fn call(env: Env<'_>, p: &Principal, cmd: &str, input: J) -> Result<J, String> {
    match env.d.call(
        Surface::Cli,
        p,
        &mut Vault::new(),
        cmd,
        &Value::from_json(&input),
    ) {
        Ok(r) => Ok(r.value.to_json()),
        Err(CallError::Handler(m)) => Err(format!("{cmd}: {m}")),
        Err(CallError::ApprovalRequired { plan_id }) => Err(format!(
            "{cmd} needs a person's approval (plan {plan_id}); run it as a signed-in person, not an agent"
        )),
        Err(CallError::Forbidden(_)) => Err(format!(
            "{cmd}: this token may not do that (the import runs as you; it needs the groups of the models it writes)"
        )),
        Err(e) => Err(format!("{cmd}: {e:?}")),
    }
}

const RATE_LIMIT_TEXT: &str = "too many bulk imports";

/// `bulk_create` with an optional source key (`table` and one id per row). With `wait` a rate
/// limit (OBOP_BULK_PER_MINUTE) is waited out; without it the refusal is the answer, and a re-run
/// resumes because every step is idempotent.
pub(crate) fn bulk(
    env: Env<'_>,
    p: &Principal,
    model: &str,
    rows: Vec<J>,
    source: Option<(&str, &str, Vec<String>)>,
    wait: bool,
) -> Result<(), String> {
    let mut input = serde_json::json!({"rows": rows, "mode": "all_or_nothing"});
    if let Some((system, table, ids)) = source {
        input["source"] = serde_json::json!({"system": system, "table": table, "ids": ids});
    }
    let cmd = format!("{model}.bulk_create");
    let mut tries = 0;
    let answer = loop {
        match call(env, p, &cmd, input.clone()) {
            Ok(a) => break a,
            Err(e) if wait && e.contains(RATE_LIMIT_TEXT) && tries < 30 => {
                tries += 1;
                eprintln!(
                    "bulk import rate limit reached; waiting 61 seconds (OBOP_BULK_PER_MINUTE)"
                );
                std::thread::sleep(std::time::Duration::from_secs(61));
            }
            Err(e) => return Err(e),
        }
    };
    let failed = answer["failed"].as_i64().unwrap_or(0);
    if failed > 0 || answer["ok"] == J::Bool(false) {
        let first: Vec<String> = answer["errors"]
            .as_array()
            .map(|l| {
                l.iter()
                    .take(3)
                    .map(|e| {
                        format!(
                            "row {}: {}",
                            e["row"].as_i64().unwrap_or(0),
                            e["error"].as_str().unwrap_or("")
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        return Err(format!(
            "{cmd}: {failed} row(s) refused, nothing of this batch was created; {}",
            first.join("; ")
        ));
    }
    Ok(())
}

/// `YYYY-MM-DD` from a SIE date.
pub(crate) fn iso(d: obop_sie::Date) -> String {
    format!("{:04}-{:02}-{:02}", d.year, d.month, d.day)
}

/// A SIE date from `YYYY-MM-DD`.
pub(crate) fn from_iso(s: &str) -> Option<obop_sie::Date> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    obop_sie::Date::new(
        s[0..4].parse().ok()?,
        s[5..7].parse().ok()?,
        s[8..10].parse().ok()?,
    )
}

/// Today (UTC) as a SIE date, from the system clock.
pub(crate) fn today() -> obop_sie::Date {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as u16;
    obop_sie::Date::new(year, month, day).unwrap_or(obop_sie::Date {
        year: 1970,
        month: 1,
        day: 1,
    })
}

// ---- the CLI --------------------------------------------------------------------------------

pub struct ExportArgs<'a> {
    pub year: &'a str,
    pub out: Option<&'a str>,
}

pub struct ImportArgs<'a> {
    pub file: &'a str,
    pub dry_run: bool,
    pub post: bool,
    pub default_account_types: bool,
    pub account_types: Option<&'a str>,
}

pub fn export_cli(o: &super::Opts, a: &ExportArgs) -> super::Exit {
    let year: u16 = a
        .year
        .parse()
        .ok()
        .filter(|y| (1000..=9999).contains(y))
        .ok_or_else(|| super::fail("usage: obop sie export --year YYYY [--out FILE] [--db URL]"))?;
    let app = super::open_app_with(o, false)?;
    let p = super::principal(&app, o)?;
    let env = Env {
        d: &app.dispatcher,
        store: &app.store,
    };
    let done = export::export(env, &p, year).map_err(|e| super::fail(&format!("error: {e}")))?;
    for n in &done.notes {
        eprintln!("note: {n}");
    }
    let target = a.out.map_or_else(|| done.filename.clone(), String::from);
    std::fs::write(&target, &done.bytes)
        .map_err(|e| super::fail(&format!("cannot write {target}: {e}")))?;
    println!(
        "wrote {target}: {} account(s), {} voucher(s), {} to {}, {} byte(s), sha256 {}",
        done.accounts,
        done.vouchers,
        done.from,
        done.to,
        done.bytes.len(),
        done.sha256
    );
    Ok(())
}

pub fn import_cli(o: &super::Opts, a: &ImportArgs) -> super::Exit {
    let bytes =
        std::fs::read(a.file).map_err(|e| super::fail(&format!("cannot read {}: {e}", a.file)))?;
    let account_types = match a.account_types {
        Some(f) => Some(
            std::fs::read_to_string(f)
                .map_err(|e| super::fail(&format!("cannot read {f}: {e}")))?,
        ),
        None => None,
    };
    // The demo seed would put fake contacts into the database that receives real books.
    let app = super::open_app_with(o, false)?;
    let p = super::principal(&app, o)?;
    let env = Env {
        d: &app.dispatcher,
        store: &app.store,
    };
    let opts = import::Options {
        dry_run: a.dry_run,
        post: a.post,
        default_account_types: a.default_account_types,
        account_types,
        wait_on_rate_limit: true,
    };
    match import::import(env, &p, &bytes, &opts) {
        Ok(rep) => {
            print!("{rep}");
            Ok(())
        }
        Err(rep) => {
            print!("{rep}");
            Err(super::fail(if a.dry_run {
                "the dry run found problems; nothing was written"
            } else {
                "the import did not complete (see PROBLEMS); a run again resumes where it stopped"
            }))
        }
    }
}
