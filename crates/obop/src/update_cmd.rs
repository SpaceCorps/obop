//! `obop update ...` (the instance side of D63) and the control plane's release and token
//! commands (`obop platform release|register-instance|update-token`).

use crate::{App, Opts};
use std::io::Read;
use std::process::ExitCode;
use std::sync::Arc;

type Exit = Result<(), ExitCode>;

fn fail(msg: &str) -> ExitCode {
    eprintln!("{msg}");
    ExitCode::FAILURE
}

fn usage() -> ExitCode {
    eprintln!("{}", crate::USAGE);
    ExitCode::from(2)
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

fn url_of(db: Option<String>, default: &str) -> String {
    db.or_else(|| std::env::var("OBOP_DATABASE_URL").ok())
        .unwrap_or_else(|| default.into())
}

/// Handle `update` and the platform release commands; `None` for anything else.
pub fn dispatch(args: &[String]) -> Option<ExitCode> {
    let first = args.first()?.as_str();
    let second = args.get(1).map(String::as_str);
    let mine = first == "update"
        || (first == "platform"
            && matches!(
                second,
                Some("release" | "register-instance" | "update-token")
            ));
    if !mine {
        return None;
    }
    let mut rest: Vec<String> = args[1..].to_vec();
    let r = if first == "update" {
        update(&mut rest)
    } else {
        rest.remove(0);
        platform(second.unwrap_or(""), &mut rest)
    };
    Some(r.map_or_else(|c| c, |()| ExitCode::SUCCESS))
}

fn app_for(url: &str) -> Result<App, ExitCode> {
    let o = Opts {
        db: Some(url.to_string()),
        token: None,
        demo: false,
        dev_auth: false,
        public_site: false,
        trust_proxy: false,
        admin_prefix: "/admin".into(),
    };
    crate::open_app(&o)
}

fn update(args: &mut Vec<String>) -> Exit {
    let url = url_of(crate::ops::take_flag(args, "--db"), "sqlite://obop.db");
    let sub = if args.is_empty() {
        return Err(usage());
    } else {
        args.remove(0)
    };
    let to = take(args, "--to");
    let skip = has(args, "--skip-backup");
    let yes = has(args, "--yes");
    if !args.is_empty() {
        return Err(usage());
    }
    let app = app_for(&url)?;
    let up = &app.updater;
    match sub.as_str() {
        "check" => {
            let c = up.check();
            println!(
                "running obop {} ({})",
                c.current,
                match c.mode {
                    obop_update::Mode::Managed => "managed: a control plane can run the update",
                    obop_update::Mode::SelfHosted => "self-hosted: you run the update",
                }
            );
            println!("feed: {}", if c.feed.is_empty() { "off" } else { &c.feed });
            if let Some(e) = &c.feed_error {
                println!("could not read the feed: {e}");
            }
            for d in &c.dropped {
                println!("left out of the feed: {d}");
            }
            if c.releases.is_empty() && c.feed_error.is_none() {
                println!("the feed lists no releases");
            }
            for r in &c.releases {
                println!(
                    "  {}  {}",
                    r.release.version,
                    if !r.newer {
                        "(running or older)".to_string()
                    } else if r.installable {
                        "newer, can be installed here".to_string()
                    } else {
                        "newer, BLOCKED".to_string()
                    }
                );
                for b in &r.blockers {
                    println!("      blocked: {b}");
                }
                for w in &r.warnings {
                    println!("      note: {w}");
                }
                if r.newer && !r.release.notes.trim().is_empty() {
                    for l in r.release.notes.trim().lines() {
                        println!("      | {l}");
                    }
                }
            }
            if c.update_available {
                println!("an update is available");
            } else if c.feed_error.is_none() {
                println!("up to date");
            }
            if c.mode == obop_update::Mode::SelfHosted && c.update_available {
                println!("To update by hand:");
                for l in &c.instructions {
                    println!("  {l}");
                }
            }
            if c.releases.iter().any(|r| r.newer && !r.installable) {
                return Err(ExitCode::FAILURE);
            }
            Ok(())
        }
        "status" => {
            let s = up.status();
            println!(
                "{}: {} {} -> {}{}{}",
                s.state,
                if s.kind.is_empty() { "-" } else { &s.kind },
                if s.from.is_empty() { "-" } else { &s.from },
                if s.target.is_empty() { "-" } else { &s.target },
                if s.phase.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", s.phase)
                },
                if s.error.is_empty() {
                    String::new()
                } else {
                    format!(" error: {}", s.error)
                }
            );
            if !s.backup.is_empty() {
                println!("backup: {}", s.backup);
            }
            Ok(())
        }
        "start" => {
            let to = to.ok_or_else(|| fail("give the version: obop update start --to 0.2.1"))?;
            if !yes {
                return Err(fail(&format!(
                    "this takes a backup and moves the instance to {to}; run it again with --yes to confirm"
                )));
            }
            let s = up.start(&to, "operator", skip).map_err(|e| fail(&e))?;
            println!("requested: {} -> {} ({})", s.from, s.target, s.state);
            if !s.backup.is_empty() {
                println!("backup: {}", s.backup);
            }
            println!("follow it with: obop update status");
            Ok(())
        }
        "rollback" => {
            if !yes {
                return Err(fail(
                    "this asks the control plane to put the previous version back; run it again with --yes to confirm",
                ));
            }
            let s = up.rollback("operator").map_err(|e| fail(&e))?;
            println!("requested: {} -> {} ({})", s.from, s.target, s.state);
            Ok(())
        }
        _ => Err(usage()),
    }
}

fn platform(sub: &str, args: &mut Vec<String>) -> Exit {
    let url = url_of(
        crate::ops::take_flag(args, "--db"),
        "sqlite://obop-platform.db",
    );
    let backend =
        obop_storage::open(&url).map_err(|e| fail(&format!("cannot open the database: {e}")))?;
    crate::ops::guard_database(&backend, &url)?;
    let prov = crate::make_provisioner("fake", None).map_err(|e| fail(&e))?;
    let auth = Arc::new(obop_auth::Auth::new(backend.clone()));
    let platform = obop_platform::Platform::new(auth, backend, prov);
    match sub {
        "release" => {
            let verb = if args.is_empty() {
                return Err(usage());
            } else {
                args.remove(0)
            };
            match verb.as_str() {
                "publish" => {
                    let version = take(args, "--version").ok_or_else(usage)?;
                    let image = take(args, "--image").ok_or_else(usage)?;
                    let notes = match (take(args, "--notes"), take(args, "--notes-file")) {
                        (Some(n), _) => n,
                        (_, Some(f)) => std::fs::read_to_string(&f)
                            .map_err(|e| fail(&format!("cannot read {f}: {e}")))?,
                        _ => String::new(),
                    };
                    let min_from = take(args, "--min-from").unwrap_or_default();
                    // The release key (H7): a PKCS#8 file that `release keygen` wrote.
                    let key = match take(args, "--signing-key") {
                        Some(f) => Some(
                            std::fs::read(&f)
                                .map_err(|e| fail(&format!("cannot read {f}: {e}")))?,
                        ),
                        None => None,
                    };
                    if !args.is_empty() {
                        return Err(usage());
                    }
                    let r = platform
                        .publish_release_signed(&version, &image, &notes, &min_from, key.as_deref())
                        .map_err(|e| fail(&e))?;
                    let signed = if r.signature.is_empty() {
                        "not signed"
                    } else {
                        "signed"
                    };
                    println!("published {} ({}), {signed}", r.version, r.image);
                    Ok(())
                }
                "keygen" => {
                    // A new release key, written once to a file of the owner's choosing; the
                    // public half is printed to pin in obop-update's RELEASE_PUBLIC_KEYS.
                    let out = take(args, "--out").ok_or_else(usage)?;
                    if !args.is_empty() {
                        return Err(usage());
                    }
                    let (pkcs8, public) =
                        obop_update::release::generate_release_key().map_err(|e| fail(&e))?;
                    let mut f = create_private_file(std::path::Path::new(&out))
                        .map_err(|e| fail(&format!("cannot create {out}: {e}")))?;
                    std::io::Write::write_all(&mut f, &pkcs8)
                        .and_then(|()| f.sync_all())
                        .map_err(|e| fail(&format!("cannot write {out}: {e}")))?;
                    println!("wrote the release key to {out}; keep it secret and off this server");
                    println!("public key: {public}");
                    Ok(())
                }
                "withdraw" => {
                    let v = args.first().cloned().ok_or_else(usage)?;
                    platform.withdraw_release(&v).map_err(|e| fail(&e))?;
                    println!("withdrew {v}; instances already on it are not touched");
                    Ok(())
                }
                "list" => {
                    for r in platform.releases() {
                        println!(
                            "{}  {}  {}",
                            r.version,
                            r.image,
                            r.notes.lines().next().unwrap_or("")
                        );
                    }
                    Ok(())
                }
                _ => Err(usage()),
            }
        }
        "register-instance" => {
            let slug = if args.is_empty() {
                return Err(usage());
            } else {
                args.remove(0)
            };
            let url_of_instance = take(args, "--url").ok_or_else(usage)?;
            let external =
                take(args, "--external-id").unwrap_or_else(|| format!("registered-{slug}"));
            let image = take(args, "--image").unwrap_or_default();
            let token_stdin = has(args, "--token-stdin");
            if !args.is_empty() {
                return Err(usage());
            }
            let supplied = if token_stdin {
                let mut t = String::new();
                std::io::stdin()
                    .read_to_string(&mut t)
                    .map_err(|e| fail(&e.to_string()))?;
                Some(t.trim().to_string())
            } else {
                None
            };
            let token = platform
                .register_instance(
                    &slug,
                    &url_of_instance,
                    &external,
                    &image,
                    supplied.as_deref(),
                )
                .map_err(|e| fail(&e))?;
            if token_stdin {
                println!("registered {slug}; it keeps the update token you supplied");
            } else {
                println!("registered {slug}. Give the instance these (shown once):");
                println!("  OBOP_UPDATE_URL=<this control plane's public address>");
                println!("  OBOP_UPDATE_TOKEN={token}");
            }
            Ok(())
        }
        "update-token" => {
            let slug = if args.is_empty() {
                return Err(usage());
            } else {
                args.remove(0)
            };
            if has(args, "--revoke") {
                platform.revoke_update_token(&slug).map_err(|e| fail(&e))?;
                println!("the update token of {slug} no longer works");
            } else {
                let token = platform.mint_update_token(&slug).map_err(|e| fail(&e))?;
                println!("new update token for {slug} (shown once; the old one stopped working):");
                println!("  OBOP_UPDATE_TOKEN={token}");
            }
            Ok(())
        }
        _ => Err(usage()),
    }
}

/// A new file that only its owner can read, created that way (mode 0600 on Unix, never wider for
/// a moment); an existing file is never overwritten. The release key of `release keygen` (H7).
fn create_private_file(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(path)
}

#[cfg(test)]
mod private_file_tests {
    #[test]
    fn the_release_key_file_is_created_private_and_never_overwritten() {
        let dir = std::env::temp_dir().join(format!("obop-keyfile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("release.pk8");
        drop(super::create_private_file(&p).unwrap());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "{mode:o}");
        }
        assert!(super::create_private_file(&p).is_err(), "never overwritten");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
