//! `obop import MODEL FILE.csv [--mode partial] [--dry-run] [--key KEY]` (D70): the CSV file goes
//! through the same `bulk_create` command as REST and MCP, as the caller of the token, so every
//! permission, row rule and limit applies. Without a token a person with database access can
//! use `--dev-auth` on a local database, as with `obop call`.

use super::{Exit, Opts, call_failure, fail, open_app, principal, refused_answer};
use obop_commands::{CallError, Surface, Value, Vault, errors};

pub struct Args<'a> {
    pub model: &'a str,
    pub file: &'a str,
    pub mode: Option<&'a str>,
    pub dry_run: bool,
    pub key: Option<&'a str>,
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
            eprintln!("row {}{line}: {msg}", int(e.get("row")));
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
    if failed > 0 {
        return Err(fail(&format!("{failed} row(s) have problems")));
    }
    Ok(())
}
