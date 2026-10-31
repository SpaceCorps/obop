//! `obop admin users|groups|offboard|disable|enable|import` (D69): account administration for the
//! operator. Database access is the authority here, as for `admin create`; every change is
//! written to the security log with the actor `operator`.

use super::{Exit, Opts, clone_opts, csv, fail, open_app};
use crate::app::App;
use obop_auth::Auth;
use obop_commands::{ADMIN_GROUP, MfaApi, Principal};
use std::process::ExitCode;

fn accounts_app(o: &Opts) -> Result<(App, Auth), ExitCode> {
    let app = open_app(&Opts {
        demo: false,
        ..clone_opts(o)
    })?;
    let auth = Auth::new(app.store.backend());
    Ok((app, auth))
}

fn operator() -> Principal {
    Principal::user("operator", &[ADMIN_GROUP])
}

/// `obop admin users`
pub fn users(o: &Opts) -> Exit {
    let (_, auth) = accounts_app(o)?;
    let list = auth
        .users(&operator())
        .map_err(|e| fail(&format!("{e:?}")))?;
    for u in list["users"].as_array().into_iter().flatten() {
        let status = if u["offboarded"] == true {
            "offboarded"
        } else if u["disabled"] == true {
            "disabled"
        } else if u["must_change_password"] == true {
            "must-change-password"
        } else {
            "active"
        };
        let groups: Vec<&str> = u["groups"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|g| g.as_str())
            .collect();
        println!(
            "{:<24} {:<20} 2fa={:<3} sessions={}  {}",
            u["name"].as_str().unwrap_or(""),
            status,
            if u["mfa_enabled"] == true {
                "on"
            } else {
                "off"
            },
            u["sessions"],
            groups.join(",")
        );
    }
    Ok(())
}

/// `obop admin groups USER [--groups a,b]`
pub fn groups(o: &Opts, user: &str, groups: Option<&str>) -> Exit {
    let (app, auth) = accounts_app(o)?;
    let Some(groups) = groups else {
        let list = auth
            .users(&operator())
            .map_err(|e| fail(&format!("{e:?}")))?;
        let name = user.trim().to_lowercase();
        let u = list["users"]
            .as_array()
            .and_then(|a| a.iter().find(|u| u["name"].as_str() == Some(&name)))
            .ok_or_else(|| fail(&format!("no user {name}")))?;
        println!("{name}: {}", u["groups"]);
        return Ok(());
    };
    let done = auth
        .operator_set_groups(user, &csv(Some(groups)), &app.dispatcher.known_groups())
        .map_err(|e| fail(&e))?;
    println!(
        "{}: {} -> {}{}",
        done["user"].as_str().unwrap_or(user),
        done["before"],
        done["after"],
        if done["changed"] == true {
            "  (takes effect on the next request; recorded in the security log)"
        } else {
            "  (no change)"
        }
    );
    Ok(())
}

/// `obop admin offboard USER [--note TEXT]`
pub fn offboard(o: &Opts, user: &str, note: Option<&str>) -> Exit {
    let (_, auth) = accounts_app(o)?;
    let done = auth
        .operator_offboard(user, note.unwrap_or(""))
        .map_err(|e| fail(&e))?;
    println!(
        "{} is disabled and offboarded: {} session(s), {} agent token(s), {} connected app(s) and {} device(s) of mobile apps ended for good; recorded in the security log",
        done["user"].as_str().unwrap_or(user),
        done["revoked_sessions"],
        done["revoked_agent_tokens"],
        done["revoked_apps"],
        done["revoked_devices"]
    );
    Ok(())
}

/// `obop admin disable|enable USER`
pub fn disable(o: &Opts, user: &str, disabled: bool) -> Exit {
    let (_, auth) = accounts_app(o)?;
    auth.operator_set_disabled(user, disabled)
        .map_err(|e| fail(&e))?;
    println!(
        "{user} is {}",
        if disabled {
            "disabled; its sessions ended"
        } else {
            "enabled (credentials revoked by an offboarding stay revoked)"
        }
    );
    Ok(())
}

/// `obop admin sso status`: the provider, who is bound, who signs in with Microsoft only, and the
/// administrators who are the way in when Microsoft cannot be reached (D91).
pub fn sso_status(o: &Opts) -> Exit {
    let (_, auth) = accounts_app(o)?;
    let s = auth.operator_sso_status().map_err(|e| fail(&e))?;
    match s["provider"].as_object() {
        Some(p) => println!(
            "provider: Microsoft Entra, tenant {}, client id {}, {}, client secret {}",
            p["tenant"].as_str().unwrap_or(""),
            p["client_id"].as_str().unwrap_or(""),
            if p["enabled"] == true {
                "enabled"
            } else {
                "not enabled"
            },
            if p["secret_set"] == true {
                "stored"
            } else {
                "not stored"
            }
        ),
        None => println!("provider: none configured"),
    }
    for a in s["accounts"].as_array().into_iter().flatten() {
        if a["bound"] == true || a["sso_only"] == true {
            println!(
                "{:<24} oid={}  {}",
                a["name"].as_str().unwrap_or(""),
                a["oid"].as_str().unwrap_or("-"),
                if a["sso_only"] == true {
                    "signs in with Microsoft only"
                } else {
                    "bound"
                }
            );
        }
    }
    let bg: Vec<&str> = s["break_glass"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|n| n.as_str())
        .collect();
    println!(
        "break-glass administrators (password and a second factor): {}{}",
        if bg.is_empty() {
            "none".to_string()
        } else {
            bg.join(", ")
        },
        if s["break_glass_enough"] == true {
            ""
        } else {
            "  (fewer than two: no account can be made SSO-only yet)"
        }
    );
    Ok(())
}

/// `obop admin sso configure --tenant GUID --client-id GUID [--secret-stdin] [--disabled]`. The
/// client secret is read from stdin, never from an argument, and sealed under `OBOP_SECRET_KEY`.
pub fn sso_configure(
    o: &Opts,
    tenant: Option<&str>,
    client_id: Option<&str>,
    secret_stdin: bool,
    disabled: bool,
) -> Exit {
    let tenant = tenant.ok_or_else(|| fail("give --tenant (the tenant id, a GUID)"))?;
    let client_id = client_id
        .ok_or_else(|| fail("give --client-id (the application id of the app registration)"))?;
    let secret = if secret_stdin {
        eprintln!("client secret of the app registration, then Enter:");
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .map_err(|e| fail(&e.to_string()))?;
        Some(line.trim_end_matches(['\r', '\n']).to_string())
    } else {
        None
    };
    let (_, auth) = accounts_app(o)?;
    let p = auth
        .operator_sso_configure(tenant, client_id, secret.as_deref(), !disabled)
        .map_err(|e| fail(&e))?;
    println!(
        "Microsoft sign-in: tenant {}, client id {}, {}; recorded in the security log",
        p.tenant,
        p.client_id,
        if p.enabled { "enabled" } else { "not enabled" }
    );
    Ok(())
}

/// `obop admin sso clear`
pub fn sso_clear(o: &Opts) -> Exit {
    let (_, auth) = accounts_app(o)?;
    auth.operator_sso_clear().map_err(|e| fail(&e))?;
    println!(
        "Microsoft sign-in configuration removed (bindings stay); recorded in the security log"
    );
    Ok(())
}

/// `obop admin sso bind USER OID | unbind USER | only USER on|off`
pub fn sso_account(o: &Opts, verb: &str, user: &str, arg: Option<&str>) -> Exit {
    let (_, auth) = accounts_app(o)?;
    let done = match (verb, arg) {
        ("bind", Some(oid)) => auth.operator_sso_bind(user, oid),
        ("unbind", None) => auth.operator_sso_unbind(user),
        ("only", Some("on")) => auth.operator_sso_only(user, true),
        ("only", Some("off")) => auth.operator_sso_only(user, false),
        _ => {
            return Err(fail(
                "usage: obop admin sso bind USER OID | unbind USER | only USER on|off",
            ));
        }
    }
    .map_err(|e| fail(&e))?;
    println!("{done}");
    Ok(())
}

/// Groups in a cell of an import file: separated by commas, semicolons, bars or spaces.
pub fn split_cell_groups(cell: &str) -> Vec<String> {
    cell.split([',', ';', '|', ' '])
        .map(str::trim)
        .filter(|g| !g.is_empty())
        .map(String::from)
        .collect()
}

/// Write `text` to a new file that only its owner can read. Refuses to overwrite.
pub fn write_private(path: &str, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)?.write_all(text.as_bytes())
}

/// `obop admin import FILE.csv [--dry-run] [--out FILE]`
pub fn import(o: &Opts, file: &str, dry_run: bool, out: Option<&str>) -> Exit {
    let bytes = std::fs::read(file).map_err(|e| fail(&format!("cannot read {file}: {e}")))?;
    let table = obop_commands::csv::parse(&bytes, obop_commands::csv::MAX_ROWS)
        .map_err(|e| fail(&format!("{file}: {e}")))?;
    let col = |names: &[&str]| {
        table
            .headers
            .iter()
            .position(|h| names.contains(&h.as_str()))
    };
    let (Some(name_col), groups_col) = (col(&["name", "username"]), col(&["groups"])) else {
        return Err(fail(&format!(
            "{file}: the first line must name the columns: name (required) and groups (optional)"
        )));
    };
    if let Some((_, bad)) = table
        .headers
        .iter()
        .enumerate()
        .find(|(i, _)| *i != name_col && Some(*i) != groups_col)
    {
        return Err(fail(&format!(
            "{file}: unknown column {bad:?}; the columns are name and groups"
        )));
    }
    if let Some(path) = out
        && !dry_run
        && std::path::Path::new(path).exists()
    {
        return Err(fail(&format!(
            "{path} exists already; choose a new file (it will hold passwords)"
        )));
    }
    let rows: Vec<obop_auth::ImportRow> = table
        .rows
        .iter()
        .zip(&table.lines)
        .map(|(r, line)| obop_auth::ImportRow {
            line: *line,
            name: r[name_col].clone(),
            groups: groups_col
                .map(|c| split_cell_groups(&r[c]))
                .unwrap_or_default(),
        })
        .collect();
    let (app, auth) = accounts_app(o)?;
    let outcomes = auth.import_users(&rows, &app.dispatcher.known_groups(), dry_run);
    let bad: Vec<_> = outcomes.iter().filter(|r| r.error.is_some()).collect();
    for r in &bad {
        eprintln!(
            "line {}: {}: {}",
            r.line,
            r.name,
            r.error.as_deref().unwrap_or("")
        );
    }
    if !bad.is_empty() {
        return Err(fail(&format!(
            "{} of {} row(s) refused; nothing was created",
            bad.len(),
            outcomes.len()
        )));
    }
    if dry_run {
        for r in &outcomes {
            println!(
                "would create {} with groups: {}",
                r.name,
                r.groups.join(", ")
            );
        }
        println!(
            "{} account(s) would be created (dry run: nothing was written)",
            outcomes.len()
        );
        return Ok(());
    }
    let text: String = outcomes
        .iter()
        .map(|r| {
            format!(
                "{},{}\n",
                r.name,
                r.temporary_password.as_deref().unwrap_or("")
            )
        })
        .collect();
    match out {
        Some(path) => {
            write_private(path, &format!("name,temporary_password\n{text}")).map_err(|e| {
                fail(&format!(
                    "accounts were created but {path} could not be written: {e}"
                ))
            })?;
            eprintln!(
                "temporary passwords written to {path} (readable by you only); each person changes theirs at the first sign-in"
            );
        }
        None => {
            println!("name,temporary_password");
            print!("{text}");
            eprintln!("shown once; each person changes their password at the first sign-in");
        }
    }
    eprintln!("created {} account(s)", outcomes.len());
    Ok(())
}
