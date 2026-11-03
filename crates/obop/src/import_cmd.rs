//! `obop import MODEL FILE.csv [--mode partial] [--dry-run] [--key KEY]` (D70): the CSV file goes
//! through the same `bulk_create` command as REST and MCP, as the caller of the token, so every
//! permission, row rule and limit applies. Without a token a person with database access can
//! use `--dev-auth` on a local database, as with `obop call`.

use super::{Exit, Opts, call_failure, fail, open_app, principal, refused_answer};
use obop_commands::{CallError, Surface, Value, Vault, errors};

/// Where the rows came from: the system, the table, and a file with one source id per data row.
pub struct Source<'a> {
    pub system: &'a str,
    pub table: &'a str,
    pub ids_file: &'a str,
}

pub struct Args<'a> {
    pub model: &'a str,
    pub file: &'a str,
    pub mode: Option<&'a str>,
    pub dry_run: bool,
    pub key: Option<&'a str>,
    pub source: Option<Source<'a>>,
    pub on_existing: Option<&'a str>,
}

fn int(v: Option<&Value>) -> i64 {
    match v {
        Some(Value::Int(n)) => *n,
        _ => 0,
    }
}

pub fn run(o: &Opts, a: &Args) -> Exit {
    let bytes = std::fs::read(a.file).map_err(|e| fail(&format!("cannot read {}: {e}", a.file)))?;
    let text =
        String::from_utf8(bytes).map_err(|_| fail(&format!("{} is not UTF-8 text", a.file)))?;
    // One source id per line, one line per data row; a blank line is a row without a key.
    let ids = match &a.source {
        Some(s) => {
            let raw = std::fs::read_to_string(s.ids_file)
                .map_err(|e| fail(&format!("cannot read {}: {e}", s.ids_file)))?;
            Some(
                raw.lines()
                    .map(str::trim_end)
                    .map(|l| {
                        if l.is_empty() {
                            Value::Null
                        } else {
                            Value::text(l)
                        }
                    })
                    .collect::<Vec<_>>(),
            )
        }
        None => None,
    };
    if a.on_existing.is_some() && a.source.is_none() {
        return Err(fail(
            "--on-existing needs --source-system, --source-table and --source-ids",
        ));
    }
    let app = open_app(o)?;
    let p = principal(&app, o)?;
    let mut input: Vec<(&str, Value)> = vec![
        ("csv", Value::Text(text)),
        ("mode", Value::text(a.mode.unwrap_or("all_or_nothing"))),
        ("dry_run", Value::Bool(a.dry_run)),
    ];
    if let Some(k) = a.key {
        input.push(("idempotency_key", Value::text(k)));
    }
    if let (Some(s), Some(ids)) = (&a.source, ids) {
        input.push((
            "source",
            Value::map([
                ("system", Value::text(s.system)),
                ("table", Value::text(s.table)),
                ("ids", Value::List(ids)),
            ]),
        ));
    }
    if let Some(e) = a.on_existing {
        input.push(("on_existing", Value::text(e)));
    }
    let input = Value::Map(input.into_iter().map(|(k, v)| (k.to_string(), v)).collect());
    let id = format!("{}.bulk_create", a.model);
    // The call is one request (see `call_json`).
    let (result, seen) = errors::requested(|| {
        app.dispatcher
            .call(Surface::Cli, &p, &mut Vault::new(), &id, &input)
    });
    let out = match result {
        Ok(r) => {
            if let Some(code) = refused_answer(&seen) {
                return Err(code);
            }
            r.value
        }
        // The store refused: the answer is the store's, whatever the rows or the text say (a row
        // whose lookup was refused is not an invalid row).
        Err(e) if seen.fault.is_some() => return Err(call_failure(&e, &seen)),
        // The refusal of a command is printed as it is.
        Err(CallError::Handler(m)) => return Err(fail(&format!("error: {m}"))),
        Err(CallError::ApprovalRequired { plan_id }) => {
            println!("proposed, not run: a person with the right must approve plan {plan_id}");
            return Ok(());
        }
        Err(
            CallError::Forbidden(_) | CallError::UnknownCommand(_) | CallError::NotOnSurface(_),
        ) => {
            return Err(fail(&format!(
                "error: this token cannot create {} records (or there is no such model)",
                a.model
            )));
        }
        Err(e) => return Err(call_failure(&e, &seen)),
    };
    let Value::Map(m) = &out else {
        return Err(fail("error: unexpected answer"));
    };
    if let Some(Value::List(errs)) = m.get("errors") {
        for e in errs {
            let Value::Map(e) = e else { continue };
            let line = match e.get("line") {
                Some(Value::Int(l)) => format!(" (line {l})"),
                _ => String::new(),
            };
            let msg = match e.get("error") {
                Some(Value::Text(t)) => t.as_str(),
                _ => "",
            };
            let code = match e.get("code") {
                Some(Value::Text(c)) => format!("{c}: "),
                _ => String::new(),
            };
            eprintln!("row {}{line}: {code}{msg}", int(e.get("row")));
        }
        if m.get("errors_truncated") == Some(&Value::Bool(true)) {
            eprintln!("... and more; only the first 100 problems are listed");
        }
    }
    let (total, failed) = (int(m.get("total")), int(m.get("failed")));
    if a.dry_run {
        println!(
            "{} of {total} row(s) are valid (dry run: nothing was written)",
            int(m.get("valid"))
        );
    } else {
        println!(
            "created {} of {total} row(s){}",
            int(m.get("created")),
            if m.get("replayed") == Some(&Value::Bool(true)) {
                " (this key was used before: nothing new was created)"
            } else {
                ""
            }
        );
    }
    let already = int(m.get("already_imported"));
    if already > 0 {
        let rows = match m.get("already_imported_rows") {
            Some(Value::List(l)) => l.as_slice(),
            _ => &[],
        };
        for r in rows.iter().take(20) {
            let Value::Map(r) = r else { continue };
            let text = |k: &str| match r.get(k) {
                Some(Value::Text(t)) => t.clone(),
                _ => String::new(),
            };
            println!(
                "already imported: row {} (source id {}) is record {}",
                int(r.get("row")),
                text("source_id"),
                text("id")
            );
        }
        if rows.len() > 20 {
            println!("... and {} more already imported", rows.len() - 20);
        }
        println!("{already} row(s) were already imported and were not created again");
    }
    if failed > 0 {
        return Err(fail(&format!("{failed} row(s) have problems")));
    }
    Ok(())
}
