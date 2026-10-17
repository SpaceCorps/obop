mod app;
#[cfg(test)]
mod connectors_tests;
#[cfg(test)]
mod mcp_tests;
#[cfg(test)]
mod mfa_tests;
#[cfg(test)]
mod modules_tests;
mod ops;
#[cfg(test)]
mod public_site_tests;

use app::{App, DevAuth};
use obop_auth::Auth;
use obop_commands::{Authenticator, Principal, Surface, Value, Vault, names};
use std::io::BufRead;
use std::process::ExitCode;
use std::sync::Arc;

const USAGE: &str = "usage: obop <command>
  --version
  modules labels TARGET LANG                    a label template (every translatable text, English values) for a module, bundled package or FILE
  modules check [--complete] [--catalog] [FILE...]
                                                the install gate over linked modules, or over package files (no database):
                                                parser, gate, dependencies and ranges against the built-ins and the bundled
                                                catalogue, core requirement. --catalog also demands what the bundled catalogue and
                                                the marketplace require (a licence, requires_core); --complete demands every text
                                                translated in each language present
  modules list                                  every module with version, licence, core requirement and source
  modules tree [ID]                             the dependency tree of a module (or of every installed package)
  modules plan FILE|ID                          what installing a file or marketplace package would do, without doing it
  modules install FILE | uninstall ID           manage packages in the database (operator tool); an install brings its dependencies
  modules market                                list the marketplace; install with `modules install-from ID`
  admin create USER [--groups a,b | --all-groups]
                                                create a user; the password is read from stdin
  admin reset-mfa USER --db URL                 break-glass: remove USER's two-factor and sessions (needs database
                                                access; writes an audit record); see docs/DEPLOY.md
  agent-token USER --scopes a,b [--label TEXT]  issue a scoped agent token (shown once)
  platform serve [--addr A] [--static DIR]      the control plane: sign up and get your own instance
      --provisioner local|sliplane|fake         (default local: child processes on this machine)
      --instance-static DIR                     backoffice build served by local instances
      sliplane needs SLIPLANE_ACCOUNT, OBOP_SLIPLANE_PROJECT, OBOP_SLIPLANE_SERVER, OBOP_CELL_ADMIN_URL
      and OBOP_IMAGE or OBOP_REPO (+ OBOP_BRANCH); optional OBOP_MARKETPLACE_URL and
      OBOP_CELL_INSTANCE_HOST (host:port instances use for the cell, e.g. obop-postgres.internal:5432),
      OBOP_CELL_INSTANCE_HOST (host:port instances use for the cell, e.g. obop-postgres.internal:5432)
      the control plane also serves /community (forum) and /status (status page); settings:
      OBOP_PLATFORM_ADMINS (staff emails), OBOP_PUBLIC_URL, OBOP_STATUS_INTERVAL_SECS (default 60),
      OBOP_CELL_ADMIN_URL (adds the cell database check)
  status incident create --title T [--impact none|minor|major|critical] [--status S]
      [--components website,api,community,provisioning,celldb,instances] [--message M]
  status incident update ID [--status investigating|identified|monitoring|resolved]
      [--impact I] --message M
  status incident resolve ID [--message M] | status incident list
                                                manage status-page incidents (--db as for platform serve)
  serve [--addr 127.0.0.1:8080] [--static DIR]  REST API and the backoffice
      --public-site                             also serve the public website and shop at / (or OBOP_PUBLIC_SITE=1);
                                                the backoffice then moves to --admin-prefix (default /admin)
      --trust-proxy                             take the client address from X-Forwarded-For (OBOP_TRUST_PROXY=1)
  migrate status | plan | run [--dry-run] [--db URL]
                                                schema version of a database and the steps that upgrade it; `run` applies them
                                                (take a backup first). `serve` refuses a database that needs steps unless started
                                                with --auto-migrate (default: on for SQLite, off for PostgreSQL; OBOP_AUTO_MIGRATE=0|1)
  backup --db URL --out DIR|FILE                consistent logical backup (gzip JSON lines + checksum manifest), streamed
  backup verify FILE                            check a backup's checksums, format and counts
  restore --db URL --from FILE [--force] [--dry-run]
                                                restore into an empty database (--force replaces what is there)
  secrets status [--db URL]                     which key seals how many stored secrets
  secrets rotate [--db URL] [--dry-run] [--batch N] [--max-batches N] [--skip-unopenable]
                                                re-seal every secret with the current OBOP_SECRET_KEY (earlier keys in
                                                OBOP_SECRET_KEY_PREVIOUS); safe while serving, resumable
  connectors purge                              remove flow logs and runs older than their retention (also done daily by serve)
                                                OBOP_SECRET_KEY encrypts connection credentials; OBOP_PUBLIC_URL is the address webhooks and
                                                OAuth redirects use; OBOP_CONNECTORS_ALLOW_LOOPBACK=1 allows loopback targets (tests only)
  site seed                                     fill an empty instance with an invented demo shop and blog
  site set KEY VALUE | site show                set or list the site settings (title, accent, default_lang ...)
  mcp [--token TOKEN]                           MCP server on stdio (token or OBOP_TOKEN)
  commands [--token TOKEN]                      list the commands visible to a token
  call [--token TOKEN] <model> <command> [k=v...]
global options
  --db URL              sqlite://obop.db (default, local), postgres://user:pass@host/db, memory:
                        (or OBOP_DATABASE_URL)
  --demo                add fake demo data (done automatically for an empty SQLite database)
  --dev-auth            use the fixed dev tokens dev-admin, dev-sales, dev-agent (localhost only)";

type Exit = Result<(), ExitCode>;

fn fail(msg: &str) -> ExitCode {
    eprintln!("{msg}");
    ExitCode::FAILURE
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Operator commands that work on a whole database: migrate, backup, restore, secrets.
    if let Some(code) = ops::dispatch(&args) {
        return code;
    }
    let args = ops::take_migrate_flags(args);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let (db, args) = take_opt(&args, "--db");
    let (token, args) = take_opt(&args, "--token");
    let (scopes, args) = take_opt(&args, "--scopes");
    let (groups, args) = take_opt(&args, "--groups");
    let (label, args) = take_opt(&args, "--label");
    let (addr, args) = take_opt(&args, "--addr");
    let (static_dir, args) = take_opt(&args, "--static");
    let (provisioner, args) = take_opt(&args, "--provisioner");
    let (instance_static, args) = take_opt(&args, "--instance-static");
    let (title, args) = take_opt(&args, "--title");
    let (impact, args) = take_opt(&args, "--impact");
    let (inc_status, args) = take_opt(&args, "--status");
    let (components, args) = take_opt(&args, "--components");
    let (message, args) = take_opt(&args, "--message");
    let (admin_prefix, args) = take_opt(&args, "--admin-prefix");
    let (flags, args): (Vec<&str>, Vec<&str>) = args.into_iter().partition(|a| {
        matches!(
            *a,
            "--demo"
                | "--dev-auth"
                | "--all-groups"
                | "--public-site"
                | "--trust-proxy"
                | "--complete"
                | "--catalog"
        )
    });
    let has = |f: &str| flags.contains(&f);
    let opts = Opts {
        db: db
            .map(String::from)
            .or_else(|| std::env::var("OBOP_DATABASE_URL").ok()),
        token: token
            .map(String::from)
            .or_else(|| std::env::var("OBOP_TOKEN").ok()),
        demo: has("--demo"),
        dev_auth: has("--dev-auth"),
        public_site: has("--public-site") || env_flag("OBOP_PUBLIC_SITE"),
        trust_proxy: has("--trust-proxy") || env_flag("OBOP_TRUST_PROXY"),
        admin_prefix: admin_prefix
            .map(String::from)
            .or_else(|| std::env::var("OBOP_ADMIN_PREFIX").ok())
            .unwrap_or_else(|| "/admin".into()),
    };
    let result = match args.as_slice() {
        [] | ["--version"] => {
            println!("{} {}", obop_types::NAME, obop_types::CORE_VERSION);
            Ok(())
        }
        ["modules", "check"] => modules_check(),
        ["modules", "check", files @ ..] => {
            modules_check_files_with(files, has("--complete"), has("--catalog"))
        }
        ["modules", "labels", target, lang] => {
            obop_pkg::files::template_for(target, lang, &App::new().modules)
                .map(|t| print!("{t}"))
                .map_err(|e| fail(&e))
        }
        ["modules", "list"] => modules_list(&opts),
        ["modules", "tree"] => modules_tree(&opts, None),
        ["modules", "tree", id] => modules_tree(&opts, Some(id)),
        ["modules", "plan", target] => modules_plan(&opts, target),
        ["modules", "market"] => modules_market(&opts),
        ["modules", "install", file] => modules_install(&opts, Some(file), None),
        ["modules", "install-from", id] => modules_install(&opts, None, Some(id)),
        ["modules", "uninstall", id] => modules_uninstall(&opts, id),
        ["admin", "reset-mfa", user] => admin_reset_mfa(&opts, user),
        ["admin", "create", user] => admin_create(&opts, user, groups, has("--all-groups")),
        ["agent-token", user] => agent_token(&opts, user, scopes, label),
        ["serve"] => serve(&opts, addr, static_dir),
        ["platform", "serve"] => {
            platform_serve(&opts, addr, static_dir, provisioner, instance_static)
        }
        [
            "status",
            "incident",
            verb @ ("create" | "update" | "resolve" | "list"),
            rest @ ..,
        ] if rest.len() <= 1 => status_incident(
            &opts,
            verb,
            &IncidentArgs {
                id: rest.first().copied(),
                title,
                impact,
                status: inc_status,
                components,
                message,
            },
        ),
        ["connectors", "purge"] => connectors_purge(&opts),
        ["site", "seed"] => site_seed(&opts),
        ["site", "show"] => site_show(&opts),
        ["site", "set", key, value] => site_set(&opts, key, value),
        ["mcp"] => mcp(&opts),
        ["commands"] => commands(&opts),
        ["call", model, command, kv @ ..] => call(&opts, model, command, kv),
        _ => {
            eprintln!("{USAGE}");
            Err(ExitCode::from(2))
        }
    };
    result.map_or_else(|c| c, |()| ExitCode::SUCCESS)
}

struct Opts {
    db: Option<String>,
    token: Option<String>,
    demo: bool,
    dev_auth: bool,
    /// Serve the public website and shop at `/` and move the backoffice to `admin_prefix`.
    public_site: bool,
    trust_proxy: bool,
    admin_prefix: String,
}

fn env_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| matches!(v.trim(), "1" | "true" | "yes" | "on"))
}

/// Pull `--flag value` out of the argument list.
fn take_opt<'a>(args: &[&'a str], flag: &str) -> (Option<&'a str>, Vec<&'a str>) {
    let mut rest = Vec::new();
    let mut found = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if *a == flag {
            found = it.next().copied();
        } else {
            rest.push(*a);
        }
    }
    (found, rest)
}

fn csv(s: Option<&str>) -> Vec<String> {
    s.map(|s| {
        s.split(',')
            .map(str::trim)
            .filter(|x| !x.is_empty())
            .map(String::from)
            .collect()
    })
    .unwrap_or_default()
}

/// Open the configured database; default is a local SQLite file. Fake demo data goes into an
/// empty SQLite database, or into any database when `--demo` is given.
fn open_app(o: &Opts) -> Result<App, ExitCode> {
    let url = o.db.clone().unwrap_or_else(|| "sqlite://obop.db".into());
    let store = obop_commands::Store::open(&url)
        .map_err(|e| fail(&format!("cannot open database: {e}")))?;
    // An empty database is stamped with the schema version; one from the future is refused; one
    // that needs migration steps gets them (--auto-migrate) or the command to run them.
    ops::guard_database(&store.backend(), &url)?;
    eprintln!("database: {}", store.backend_kind());
    let seed = o.demo || store.backend_kind() == "sqlite";
    let app = App::with_store(store);
    if seed {
        app.seed_demo();
    }
    Ok(app)
}

fn authenticator(app: &App, o: &Opts) -> Arc<dyn Authenticator> {
    if o.dev_auth {
        Arc::new(DevAuth)
    } else {
        Arc::new(Auth::new(app.store.backend()))
    }
}

/// The principal for CLI and MCP use: a real token, or a dev token with `--dev-auth`.
fn principal(app: &App, o: &Opts) -> Result<Principal, ExitCode> {
    let token = o
        .token
        .as_deref()
        .unwrap_or(if o.dev_auth { "dev-admin" } else { "" });
    authenticator(app, o).resolve(token).ok_or_else(|| fail("no valid token: pass --token (or OBOP_TOKEN); see `obop admin create` and `obop agent-token`"))
}

/// The D41 install gate over every linked first-party module.
fn modules_check() -> Exit {
    let mut failed = false;
    for m in &App::new().modules {
        let violations = obop_registry::check_module(m);
        if violations.is_empty() {
            println!("ok    {}", m.id);
        } else {
            failed = true;
            for v in violations {
                println!("FAIL  {}: {}", v.path, v.problem);
            }
        }
    }
    if failed {
        Err(ExitCode::FAILURE)
    } else {
        Ok(())
    }
}

/// `obop modules check FILE...`: the parser, the D41 gate and the install-time rules that need no
/// database, for package files. Prints `ok` or the exact errors per file.
#[cfg(test)]
fn modules_check_files(files: &[&str], complete: bool) -> Exit {
    modules_check_files_with(files, complete, false)
}

fn modules_check_files_with(files: &[&str], complete: bool, catalog: bool) -> Exit {
    let builtin = App::new().modules;
    let mut failed = false;
    for f in files {
        let errors = match obop_pkg::files::read_source(std::path::Path::new(f)) {
            Ok(obop_pkg::files::Source::Package(text)) => {
                let r = obop_pkg::check_package(&text, &builtin, catalog);
                for w in &r.warnings {
                    println!("warn  {f}: {w}");
                }
                r.errors
            }
            Ok(obop_pkg::files::Source::Labels {
                stem,
                text,
                manifest,
                allow,
            }) => obop_pkg::files::check_labels_source(
                &stem,
                &text,
                manifest.as_deref(),
                &allow,
                complete,
                &builtin,
            ),
            Err(e) => vec![format!("cannot read: {e}")],
        };
        if errors.is_empty() {
            println!("ok    {f}");
        } else {
            failed = true;
            for e in errors {
                println!("FAIL  {f}: {e}");
            }
        }
    }
    if failed {
        Err(ExitCode::FAILURE)
    } else {
        Ok(())
    }
}

fn modules_list(o: &Opts) -> Exit {
    let app = open_app(&Opts {
        demo: false,
        ..clone_opts(o)
    })?;
    let core = obop_types::CORE_VERSION;
    println!(
        "{:<16} {:<10} {:<20} {:<12} {:<12} NAME",
        "ID", "VERSION", "LICENSE", "NEEDS CORE", "SOURCE"
    );
    for p in app.installer.list() {
        let req = if p.meta.requires_core.is_empty() {
            "*"
        } else {
            &p.meta.requires_core
        };
        let ok = obop_pkg::semver::Req::parse(req)
            .ok()
            .zip(obop_pkg::semver::Version::parse(core).ok())
            .is_none_or(|(r, v)| r.matches(&v));
        println!(
            "{:<16} {:<10} {:<20} {:<12} {:<12} {}{}",
            p.id,
            p.version,
            if p.meta.license.is_empty() {
                "-"
            } else {
                &p.meta.license
            },
            req,
            p.source,
            p.name,
            if ok { "" } else { "  (needs another core)" }
        );
    }
    println!("core {core}");
    Ok(())
}

fn modules_tree(o: &Opts, id: Option<&str>) -> Exit {
    let app = open_app(&Opts {
        demo: false,
        ..clone_opts(o)
    })?;
    print!("{}", app.installer.tree(id).map_err(|e| fail(&e))?);
    Ok(())
}

/// `obop modules plan FILE|ID`: the install plan, nothing installed. A target that is a file on
/// disk is an upload; anything else is a marketplace id.
fn modules_plan(o: &Opts, target: &str) -> Exit {
    let app = open_app(&Opts {
        demo: false,
        ..clone_opts(o)
    })?;
    let (text, ok) = if std::path::Path::new(target).is_file() {
        let manifest =
            obop_pkg::files::read_package(std::path::Path::new(target)).map_err(|e| fail(&e))?;
        app.installer
            .plan_text(obop_pkg::installer::Request::Manifest(&manifest))
    } else {
        app.installer
            .plan_text(obop_pkg::installer::Request::Market(target))
    }
    .map_err(|e| fail(&e))?;
    print!("{text}");
    if ok { Ok(()) } else { Err(ExitCode::FAILURE) }
}

fn modules_market(o: &Opts) -> Exit {
    let app = open_app(&Opts {
        demo: false,
        ..clone_opts(o)
    })?;
    let items = app.market.list().map_err(|e| fail(&e))?;
    for l in items {
        println!("{:<14} {:<10} {}", l.id, l.version, l.description);
    }
    Ok(())
}

fn modules_install(o: &Opts, file: Option<&str>, market_id: Option<&str>) -> Exit {
    let app = open_app(&Opts {
        demo: false,
        ..clone_opts(o)
    })?;
    let by = "operator";
    let r = match (file, market_id) {
        (Some(f), _) => app.installer.install_outcome(
            &obop_pkg::files::read_package(std::path::Path::new(f)).map_err(|e| fail(&e))?,
            "upload",
            by,
        ),
        (_, Some(id)) => app.installer.install_from_marketplace_outcome(id, by),
        _ => unreachable!(),
    };
    let o = r.map_err(|e| fail(&e))?;
    for (p, action) in o.applied.iter().filter(|(p, _)| p.id != o.root.id) {
        println!("{} {} {} (dependency)", action.as_str(), p.id, p.version);
    }
    let p = &o.root;
    println!(
        "installed {} {} (models: {}; groups: {})",
        p.id,
        p.version,
        p.models.join(", "),
        p.groups.join(", ")
    );
    for w in &o.warnings {
        println!("warning: {w}");
    }
    Ok(())
}

fn modules_uninstall(o: &Opts, id: &str) -> Exit {
    let app = open_app(&Opts {
        demo: false,
        ..clone_opts(o)
    })?;
    app.installer.uninstall(id).map_err(|e| fail(&e))?;
    println!("uninstalled {id}; its data stays in the database");
    Ok(())
}

fn admin_create(o: &Opts, user: &str, groups: Option<&str>, all: bool) -> Exit {
    let app = open_app(&Opts {
        demo: false,
        ..clone_opts(o)
    })?;
    let groups = if all { app.all_groups() } else { csv(groups) };
    eprintln!("password for {user} (min 12 characters), then Enter:");
    let mut pw = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut pw)
        .map_err(|e| fail(&e.to_string()))?;
    Auth::new(app.store.backend())
        .create_user(user, pw.trim_end_matches(['\r', '\n']), &groups)
        .map_err(|e| fail(&e))?;
    println!(
        "created {user} with groups: {}",
        if groups.is_empty() {
            "(none)".into()
        } else {
            groups.join(", ")
        }
    );
    Ok(())
}

/// `obop admin reset-mfa USER`: the operator's way back in when a user lost both the device and
/// the recovery codes. Needs direct database access, which is the proof of authority.
fn admin_reset_mfa(o: &Opts, user: &str) -> Exit {
    let app = open_app(&Opts {
        demo: false,
        ..clone_opts(o)
    })?;
    Auth::new(app.store.backend())
        .operator_reset_mfa(user)
        .map_err(|e| fail(&e))?;
    println!(
        "two-factor removed for {user}; their sessions are revoked; the reset is in the security log"
    );
    Ok(())
}

fn clone_opts(o: &Opts) -> Opts {
    Opts {
        db: o.db.clone(),
        token: o.token.clone(),
        demo: o.demo,
        dev_auth: o.dev_auth,
        public_site: o.public_site,
        trust_proxy: o.trust_proxy,
        admin_prefix: o.admin_prefix.clone(),
    }
}

fn agent_token(o: &Opts, user: &str, scopes: Option<&str>, label: Option<&str>) -> Exit {
    let app = open_app(&Opts {
        demo: false,
        ..clone_opts(o)
    })?;
    let auth = Auth::new(app.store.backend());
    let owner = auth
        .principal(user)
        .ok_or_else(|| fail(&format!("no user {user}")))?;
    let t = auth
        .issue_agent_token(&owner, &csv(scopes), label.unwrap_or("agent"))
        .map_err(|e| fail(&e))?;
    println!("{t}");
    eprintln!("shown once; store it like a password");
    Ok(())
}

fn commands(o: &Opts) -> Exit {
    let app = open_app(o)?;
    let p = principal(&app, o)?;
    for c in app.dispatcher.catalog(Surface::Cli, &p) {
        let note = if c.needs_approval {
            "  [needs approval for agents]"
        } else {
            ""
        };
        println!("{:<24} {}{note}", c.id, c.description);
    }
    Ok(())
}

fn call(o: &Opts, model: &str, command: &str, kv: &[&str]) -> Exit {
    let app = open_app(o)?;
    let p = principal(&app, o)?;
    let id = names::from_cli(&[model, command]).ok_or(ExitCode::from(2))?;
    // Command-line values are all text; give number, money and bool fields their JSON types so
    // typed fields (D47) validate the same way as over REST. Anything else stays a string.
    let meta = app.dispatcher.meta(Surface::Cli, &p);
    let kind_of = |field: &str| {
        meta.iter()
            .flat_map(|m| &m.models)
            .find(|m| m.name == model)
            .and_then(|m| m.fields.iter().find(|f| f.name == field))
            .map_or("text", |f| f.kind)
    };
    let input = Value::Map(
        kv.iter()
            .filter_map(|p| p.split_once('='))
            .map(|(k, v)| {
                // `list` and `search` options (D49): limit=5 offset=10 filter='[{...}]' order='[...]'.
                if matches!(command, "list" | "search") {
                    match k {
                        "limit" | "offset" => {
                            if let Ok(n) = v.parse() {
                                return (k.to_string(), Value::Int(n));
                            }
                        }
                        "filter" | "order" => {
                            if let Ok(j) = serde_json::from_str(v) {
                                return (k.to_string(), Value::from_json(&j));
                            }
                        }
                        _ => {}
                    }
                }
                let typed = match kind_of(k) {
                    "number" | "money" => v.parse().map(Value::Int).ok(),
                    "bool" => v.parse().map(Value::Bool).ok(),
                    _ => None,
                };
                (k.to_string(), typed.unwrap_or_else(|| Value::text(v)))
            })
            .collect(),
    );
    match app
        .dispatcher
        .call(Surface::Cli, &p, &mut Vault::new(), &id, &input)
    {
        Ok(r) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&r.value.to_json()).unwrap()
            );
            if !r.denied.is_empty() {
                println!("withheld: {}", r.denied.join(", "));
            }
            Ok(())
        }
        Err(e) => Err(fail(&format!("error: {e:?}"))),
    }
}

fn is_loopback(addr: &str) -> bool {
    addr.rsplit_once(':')
        .is_some_and(|(host, _)| matches!(host, "127.0.0.1" | "localhost" | "[::1]"))
}

fn serve(o: &Opts, addr: Option<&str>, static_dir: Option<&str>) -> Exit {
    let addr = addr.unwrap_or("127.0.0.1:8080");
    // The fixed dev tokens are public knowledge: they never leave this machine.
    if o.dev_auth && !is_loopback(addr) {
        return Err(fail(&format!(
            "refusing to bind {addr} with --dev-auth: dev tokens are localhost-only"
        )));
    }
    let app = open_app(o)?;
    let auth = authenticator(&app, o);
    if !o.dev_auth && Auth::new(app.store.backend()).user_count() == 0 {
        eprintln!(
            "no users yet: create one with `obop admin create NAME --all-groups` (or try --dev-auth on localhost)"
        );
    }
    let state = obop_http::HttpState::new(app.dispatcher.clone(), auth);
    state.set_connectors(app.connectors.clone());
    app.connectors.start();
    let router = if o.public_site {
        if !obop_http::public::admin_prefix_ok(&o.admin_prefix) {
            return Err(fail(&format!(
                "--admin-prefix {:?} must be one path segment such as /admin (lowercase letters, digits, - and _)",
                o.admin_prefix
            )));
        }
        let site = Arc::new(obop_site::Site::new(
            obop_commands::PublicApi::new(app.dispatcher.clone(), app.store.clone()),
            obop_site::Config {
                admin_prefix: Some(o.admin_prefix.clone()),
                ..obop_site::Config::from_env()
            },
        ));
        if !site.has_public_models() {
            eprintln!(
                "the public site is on but no public content is installed yet: obop modules install-from website, install-from ecommerce, then obop site seed"
            );
        }
        eprintln!(
            "public site at http://{addr}/ ; backoffice at http://{addr}{}/",
            o.admin_prefix
        );
        obop_http::public::router(
            state,
            static_dir,
            site,
            obop_http::public::PublicConfig {
                admin_prefix: o.admin_prefix.clone(),
                trust_proxy: o.trust_proxy,
            },
        )
    } else {
        obop_http::router(state, static_dir)
    };
    eprintln!(
        "obop listening on http://{addr}{}",
        if o.dev_auth {
            " (dev tokens: dev-admin, dev-agent, dev-sales)"
        } else {
            ""
        }
    );
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(obop_http::serve(addr, router))
        .map_err(|e| fail(&format!("error: {e}")))
}

/// `obop connectors purge`: retention for flow logs, runs and expired webhook and OAuth entries.
fn connectors_purge(o: &Opts) -> Exit {
    let app = open_app(o)?;
    let (logs, runs, other) = app.connectors.purge(obop_connectors::util::now());
    println!("removed {logs} flow log rows, {runs} runs and {other} expired entries");
    Ok(())
}

/// `obop site seed`: an invented demo shop and blog in an empty instance.
fn site_seed(o: &Opts) -> Exit {
    let app = open_app(o)?;
    let summary = obop_site::seed::seed(&app.store, &app.dispatcher).map_err(|e| fail(&e))?;
    for line in summary.0 {
        println!("created {line}");
    }
    println!("serve it with: obop serve --public-site --static web/apps/backoffice/dist");
    Ok(())
}

/// The `site` object of the instance settings, as stored.
fn site_settings(app: &App) -> serde_json::Map<String, serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(&app.installer.settings())
        .ok()
        .and_then(|v| v.get("site").and_then(|s| s.as_object().cloned()))
        .unwrap_or_default()
}

fn site_show(o: &Opts) -> Exit {
    let app = open_app(o)?;
    let set = site_settings(&app);
    for (key, help) in obop_site::settings::KEYS {
        match set.get(*key) {
            Some(v) => println!("{key} = {v}"),
            None => println!("{key} (not set)  {help}"),
        }
    }
    Ok(())
}

/// `obop site set KEY VALUE`: validate and store one site setting.
fn site_set(o: &Opts, key: &str, value: &str) -> Exit {
    let value = obop_site::settings::parse_value(key, value).map_err(|e| fail(&e))?;
    let app = open_app(o)?;
    let mut all: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&app.installer.settings()).unwrap_or_default();
    let mut site = site_settings(&app);
    site.insert(key.to_string(), value);
    all.insert("site".into(), serde_json::Value::Object(site));
    app.installer
        .set_settings(&serde_json::Value::Object(all).to_string())
        .map_err(|e| fail(&e))?;
    println!("site.{key} set");
    Ok(())
}

/// Build the provisioner named on the command line. Sliplane settings come from the
/// environment so no secret appears in the process list.
fn make_provisioner(
    kind: &str,
    instance_static: Option<&str>,
) -> Result<Arc<dyn obop_platform::provision::Provisioner>, String> {
    use obop_platform::{provision, sliplane};
    let env = |k: &str| std::env::var(k).map_err(|_| format!("{k} is not set"));
    match kind {
        "fake" => Ok(Arc::new(provision::Fake::default())),
        "local" => {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let dir = std::env::var("OBOP_DATA_DIR").unwrap_or_else(|_| "obop-instances".into());
            Ok(Arc::new(provision::Local::new(
                exe,
                dir.into(),
                instance_static.map(String::from),
            )))
        }
        "sliplane" => {
            let source = match (std::env::var("OBOP_IMAGE"), std::env::var("OBOP_REPO")) {
                (Ok(i), _) => sliplane::Source::Image(i),
                (_, Ok(url)) => sliplane::Source::Repo {
                    url,
                    branch: std::env::var("OBOP_BRANCH").unwrap_or_else(|_| "main".into()),
                },
                _ => return Err("set OBOP_IMAGE or OBOP_REPO".into()),
            };
            let cfg = sliplane::Config {
                account: env("SLIPLANE_ACCOUNT")?,
                project_id: env("OBOP_SLIPLANE_PROJECT")?,
                server_id: env("OBOP_SLIPLANE_SERVER")?,
                source,
                marketplace_url: std::env::var("OBOP_MARKETPLACE_URL").ok(),
                poll: sliplane::Config::DEFAULT_POLL,
                timeout: sliplane::Config::DEFAULT_TIMEOUT,
                check_reachable: true,
            };
            let cell = sliplane::PgCell {
                admin_url: env("OBOP_CELL_ADMIN_URL")?,
                instance_host: std::env::var("OBOP_CELL_INSTANCE_HOST").ok(),
            };
            Ok(Arc::new(sliplane::Sliplane::new(
                cfg,
                Arc::new(sliplane::Cli),
                Arc::new(cell),
            )))
        }
        other => Err(format!("unknown provisioner {other}")),
    }
}

fn platform_serve(
    o: &Opts,
    addr: Option<&str>,
    static_dir: Option<&str>,
    provisioner: Option<&str>,
    instance_static: Option<&str>,
) -> Exit {
    let addr = addr.unwrap_or("127.0.0.1:8090");
    let url =
        o.db.clone()
            .unwrap_or_else(|| "sqlite://obop-platform.db".into());
    let backend =
        obop_storage::open(&url).map_err(|e| fail(&format!("cannot open database: {e}")))?;
    ops::guard_database(&backend, &url)?;
    let prov =
        make_provisioner(provisioner.unwrap_or("local"), instance_static).map_err(|e| fail(&e))?;
    let auth = Arc::new(Auth::new(backend.clone()));
    let platform = obop_platform::Platform::new(auth.clone(), backend.clone(), prov);
    // The public forum (/community) and status page (/status), with their background checker.
    let community =
        obop_community::Community::open(obop_community::Config::from_env(), auth, backend.clone())
            .map_err(|e| fail(&format!("cannot open the community data: {e}")))?;
    let interval = std::env::var("OBOP_STATUS_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(60)
        .max(5);
    community.spawn_checker(
        status_probes(
            backend,
            addr,
            static_dir.is_some(),
            provisioner.unwrap_or("local"),
        ),
        std::time::Duration::from_secs(interval),
    );
    let router = obop_http::platform_router(Arc::new(obop_platform::Api(platform)), static_dir)
        .merge(obop_http::community::router(community));
    eprintln!("obop platform listening on http://{addr}");
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(obop_http::serve(addr, router))
        .map_err(|e| fail(&format!("error: {e}")))
}

/// The status page's checks for `platform serve`: website (only when a static site is served),
/// control plane API, community, provisioning, the cell database (only when
/// `OBOP_CELL_ADMIN_URL` is set) and the customer instances.
fn status_probes(
    backend: Arc<dyn obop_storage::Backend>,
    addr: &str,
    has_static: bool,
    provisioner: &str,
) -> Vec<obop_community::status::Probe> {
    use obop_community::status::{self as st, UreqHttp};
    let mut probes = vec![
        st::probe_storage("api", backend.clone(), "status_probe_api"),
        st::probe_storage("community", backend.clone(), "community_probe"),
        st::probe_provisioning(provisioner.into(), Arc::new(st::sliplane_version)),
        st::probe_instances(backend, Arc::new(UreqHttp { hardened: true })),
    ];
    if has_static && let Some(port) = addr.rsplit(':').next().filter(|p| p.parse::<u16>().is_ok()) {
        probes.push(st::probe_website(
            format!("http://127.0.0.1:{port}/"),
            Arc::new(UreqHttp { hardened: false }),
        ));
    }
    if let Ok(url) = std::env::var("OBOP_CELL_ADMIN_URL") {
        probes.push(st::probe_celldb(url));
    }
    probes
}

/// Arguments of `obop status incident ...`.
struct IncidentArgs<'a> {
    id: Option<&'a str>,
    title: Option<&'a str>,
    impact: Option<&'a str>,
    status: Option<&'a str>,
    components: Option<&'a str>,
    message: Option<&'a str>,
}

/// `obop status incident create|update|resolve|list`: manage status-page incidents from the
/// command line, directly in the control plane's database.
fn status_incident(o: &Opts, verb: &str, a: &IncidentArgs) -> Exit {
    use obop_community::status::{Impact, IncStatus, Status};
    let url =
        o.db.clone()
            .unwrap_or_else(|| "sqlite://obop-platform.db".into());
    let backend =
        obop_storage::open(&url).map_err(|e| fail(&format!("cannot open database: {e}")))?;
    let st = Status::open(backend).map_err(|e| fail(&format!("cannot read status data: {e}")))?;
    let now = obop_community::util::now();
    let bad = |what: &str, v: &str| fail(&format!("invalid {what}: {v}"));
    let impact = match a.impact {
        Some(i) => {
            Some(Impact::parse(i).ok_or_else(|| bad("impact (none|minor|major|critical)", i))?)
        }
        None => None,
    };
    let status = match a.status {
        Some(s) => Some(
            IncStatus::parse(s)
                .ok_or_else(|| bad("status (investigating|identified|monitoring|resolved)", s))?,
        ),
        None => None,
    };
    let need_id = || -> Result<u64, ExitCode> {
        a.id.and_then(|i| i.parse().ok())
            .ok_or_else(|| fail("an incident id is required"))
    };
    let msg = a.message.unwrap_or("");
    match verb {
        "create" => {
            let title = a.title.ok_or_else(|| fail("--title is required"))?;
            let id = st
                .create_incident(
                    title,
                    impact.unwrap_or(Impact::Minor),
                    status.unwrap_or(IncStatus::Investigating),
                    &csv(a.components),
                    msg,
                    now,
                )
                .map_err(|e| fail(&format!("error: {e}")))?;
            println!("incident {id} created");
        }
        "update" => {
            let id = need_id()?;
            st.update_incident(id, status, impact, msg, now)
                .map_err(|e| fail(&format!("error: {e}")))?;
            println!("incident {id} updated");
        }
        "resolve" => {
            let id = need_id()?;
            st.resolve_incident(id, if msg.is_empty() { "Resolved." } else { msg }, now)
                .map_err(|e| fail(&format!("error: {e}")))?;
            println!("incident {id} resolved");
        }
        _ => {
            for i in st.incidents() {
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    i.id,
                    i.status.code(),
                    i.impact.code(),
                    i.components.join(","),
                    i.title
                );
            }
        }
    }
    Ok(())
}

fn mcp(o: &Opts) -> Exit {
    let app = open_app(o)?;
    let p = principal(&app, o)?;
    obop_mcp::serve_stdio(&app.dispatcher, &p).map_err(|e| fail(&format!("error: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use serde_json::{Value as J, json};
    use tower::ServiceExt;

    /// An app on a fresh in-memory database with real authentication (cheap hashing).
    fn real() -> (axum::Router, Arc<Auth>) {
        let app = App::new();
        app.seed_demo();
        let auth = Arc::new(Auth::with_test_params(app.store.backend()));
        auth.create_user("ada", "correct horse battery", &app.all_groups())
            .unwrap();
        auth.create_user(
            "sam",
            "another long password",
            &["base.user".to_string(), "crm.user".to_string()],
        )
        .unwrap();
        (
            obop_http::router(
                obop_http::HttpState::new(app.dispatcher.clone(), auth.clone()),
                None,
            ),
            auth,
        )
    }

    fn dev() -> axum::Router {
        let app = App::new();
        app.seed_demo();
        obop_http::router(
            obop_http::HttpState::new(app.dispatcher.clone(), Arc::new(DevAuth)),
            None,
        )
    }

    async fn post(r: &axum::Router, token: Option<&str>, path: &str, body: J) -> (u16, J) {
        let mut req = Request::post(path).header("content-type", "application/json");
        if let Some(t) = token {
            req = req.header("authorization", format!("Bearer {t}"));
        }
        let resp = r
            .clone()
            .oneshot(req.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        let status = resp.status().as_u16();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or(J::Null))
    }

    async fn login(r: &axum::Router, user: &str, pw: &str) -> String {
        let (s, b) = post(
            r,
            None,
            "/api/auth/login",
            json!({"username": user, "password": pw}),
        )
        .await;
        assert_eq!(s, 200, "{b}");
        b["token"].as_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn no_token_or_bad_login_is_rejected() {
        let (r, _) = real();
        assert_eq!(post(&r, None, "/api/party/list", json!({})).await.0, 401);
        assert_eq!(
            post(&r, Some("obop_forged"), "/api/party/list", json!({}))
                .await
                .0,
            401
        );
        assert_eq!(
            post(
                &r,
                None,
                "/api/auth/login",
                json!({"username": "ada", "password": "wrong"})
            )
            .await
            .0,
            401
        );
        // Dev tokens are not accepted by the real authenticator.
        assert_eq!(
            post(&r, Some("dev-admin"), "/api/party/list", json!({}))
                .await
                .0,
            401
        );
    }

    #[tokio::test]
    async fn human_signs_in_issues_agent_token_and_agent_is_masked() {
        let (r, _) = real();
        let human = login(&r, "ada", "correct horse battery").await;
        let (s, b) = post(&r, Some(&human), "/api/party/list", json!({})).await;
        assert_eq!(s, 200);
        assert_eq!(b["value"][0]["name"], "Ada Example");

        // Scopes must be groups the human holds.
        let (s, _) = post(
            &r,
            Some(&human),
            "/api/auth/agent-token",
            json!({"scopes": ["nonexistent.group"]}),
        )
        .await;
        assert_eq!(s, 400);
        let (s, b) = post(
            &r,
            Some(&human),
            "/api/auth/agent-token",
            json!({"scopes": ["base.user"], "label": "bot"}),
        )
        .await;
        assert_eq!(s, 200);
        let agent = b["token"].as_str().unwrap().to_string();

        let (_, b) = post(&r, Some(&agent), "/api/party/list", json!({})).await;
        assert!(b["value"][0]["name"].as_str().unwrap().starts_with("NAME_"));
        assert_eq!(b["denied"], json!(["party.tax_id"]));
        // The agent's scope is base.user only: sales is out of reach even though ada holds it.
        assert_eq!(
            post(&r, Some(&agent), "/api/sale_order/list", json!({}))
                .await
                .0,
            404
        );
        // An agent cannot mint tokens.
        assert_eq!(
            post(
                &r,
                Some(&agent),
                "/api/auth/agent-token",
                json!({"scopes": ["base.user"]})
            )
            .await
            .0,
            400
        );
    }

    #[tokio::test]
    async fn groups_limit_what_a_user_can_do() {
        let (r, _) = real();
        let sam = login(&r, "sam", "another long password").await;
        assert_eq!(
            post(&r, Some(&sam), "/api/lead/list", json!({})).await.0,
            200
        );
        assert_eq!(
            post(&r, Some(&sam), "/api/invoice/list", json!({})).await.0,
            404
        );
    }

    #[tokio::test]
    async fn dev_auth_agent_is_masked_and_approval_gated() {
        let r = dev();
        let (_, body) = post(&r, Some("dev-agent"), "/api/party/list", json!({})).await;
        let id = body["value"][0]["id"].as_str().unwrap().to_string();
        let (s, body) = post(&r, Some("dev-agent"), "/api/party/erase", json!({"id": id})).await;
        assert_eq!(s, 202);
        let plan = body["plan_id"].as_str().unwrap().to_string();
        assert_eq!(
            post(
                &r,
                Some("dev-agent"),
                &format!("/api/approve/{plan}"),
                json!({})
            )
            .await
            .0,
            404
        );
        assert_eq!(
            post(
                &r,
                Some("dev-admin"),
                &format!("/api/approve/{plan}"),
                json!({})
            )
            .await
            .0,
            200
        );
    }

    #[tokio::test]
    async fn hidden_commands_look_unknown() {
        let r = dev();
        let (a, _) = post(&r, Some("dev-sales"), "/api/invoice/list", json!({})).await;
        let (b, _) = post(&r, Some("dev-sales"), "/api/nothing/list", json!({})).await;
        assert_eq!((a, b), (404, 404));
    }

    #[test]
    fn mcp_lists_and_calls_tools() {
        let app = App::new();
        app.seed_demo();
        let p = app::dev_principal("dev-agent").unwrap();
        let mut v = obop_mcp::Session::new();
        let list = obop_mcp::handle(
            &app.dispatcher,
            &p,
            &mut v,
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
        )
        .unwrap();
        let list: J = serde_json::from_str(&list).unwrap();
        let names: Vec<_> = list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"party__get") && names.contains(&"invoice__post"));
        let call = obop_mcp::handle(&app.dispatcher, &p, &mut v, r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"party__list","arguments":{}}}"#).unwrap();
        let call: J = serde_json::from_str(&call).unwrap();
        let text = call["result"]["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("NAME_")
                && !text.contains("Ada Example")
                && text.contains("party.tax_id")
        );
        assert!(
            obop_mcp::handle(
                &app.dispatcher,
                &p,
                &mut v,
                r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#
            )
            .is_none()
        );
    }

    #[tokio::test]
    async fn admin_installs_a_module_over_rest_and_it_appears_everywhere() {
        let (r, auth) = real();
        auth.create_user(
            "root",
            "an admin password!",
            &["platform.admin".to_string()],
        )
        .unwrap();
        let admin = login(&r, "root", "an admin password!").await;
        let sam = login(&r, "sam", "another long password").await;

        // A non-admin cannot even see the command.
        assert_eq!(
            post(&r, Some(&sam), "/api/marketplace_listing/list", json!({}))
                .await
                .0,
            404
        );
        let (s, b) = post(&r, Some(&admin), "/api/marketplace_listing/list", json!({})).await;
        assert_eq!(s, 200);
        assert!(
            b["value"]
                .as_array()
                .unwrap()
                .iter()
                .any(|l| l["id"] == "fleet")
        );

        assert_eq!(
            post(&r, Some(&admin), "/api/fleet_vehicle/list", json!({}))
                .await
                .0,
            404
        );
        assert_eq!(
            post(
                &r,
                Some(&admin),
                "/api/marketplace_listing/install",
                json!({"id": "fleet"})
            )
            .await
            .0,
            200
        );
        let (s, b) = post(
            &r,
            Some(&admin),
            "/api/fleet_vehicle/create",
            json!({"plate": "KLM-42", "driver": "Dana Example", "state": "available"}),
        )
        .await;
        assert_eq!(s, 200, "{b}");
        let id = b["value"]["id"].as_str().unwrap().to_string();
        assert_eq!(
            post(
                &r,
                Some(&admin),
                "/api/fleet_vehicle/dispatch",
                json!({"id": id})
            )
            .await
            .0,
            200
        );
        // sam holds no fleet group, so the new models are invisible to them.
        assert_eq!(
            post(&r, Some(&sam), "/api/fleet_vehicle/list", json!({}))
                .await
                .0,
            404
        );

        // An admin's agent token (scoped to platform.admin) can only propose an uninstall.
        let (_, b) = post(
            &r,
            Some(&admin),
            "/api/auth/agent-token",
            json!({"scopes": ["platform.admin"]}),
        )
        .await;
        let agent = b["token"].as_str().unwrap().to_string();
        assert_eq!(
            post(
                &r,
                Some(&agent),
                "/api/module/uninstall",
                json!({"id": "fleet"})
            )
            .await
            .0,
            202
        );
        assert_eq!(
            post(&r, Some(&admin), "/api/fleet_vehicle/list", json!({}))
                .await
                .0,
            200
        );
        assert_eq!(
            post(
                &r,
                Some(&admin),
                "/api/module/uninstall",
                json!({"id": "fleet"})
            )
            .await
            .0,
            200
        );
        assert_eq!(
            post(&r, Some(&admin), "/api/fleet_vehicle/list", json!({}))
                .await
                .0,
            404
        );
    }

    /// Regression: the PostgreSQL client panics when used on an async worker thread, which took
    /// every deployed instance down on its first login. Needs `OBOP_TEST_POSTGRES_URL`.
    #[tokio::test(flavor = "multi_thread")]
    async fn postgres_backed_instance_serves_requests_inside_the_runtime() {
        let Ok(url) = std::env::var("OBOP_TEST_POSTGRES_URL") else {
            eprintln!("skipped: set OBOP_TEST_POSTGRES_URL");
            return;
        };
        let user = format!("pg{}@example.test", std::process::id());
        let router = tokio::task::spawn_blocking({
            let user = user.clone();
            move || {
                let app = App::with_store(obop_commands::Store::open(&url).unwrap());
                app.seed_demo();
                let auth = Arc::new(Auth::with_test_params(app.store.backend()));
                auth.create_user(&user, "correct horse battery", &app.all_groups())
                    .unwrap();
                obop_http::router(
                    obop_http::HttpState::new(app.dispatcher.clone(), auth),
                    None,
                )
            }
        })
        .await
        .unwrap();
        let token = login(&router, &user, "correct horse battery").await;
        let (s, b) = post(&router, Some(&token), "/api/party/list", json!({})).await;
        assert_eq!(s, 200, "{b}");
        assert!(b["value"].as_array().is_some_and(|v| !v.is_empty()));
        // Dropping the PostgreSQL client also blocks, so drop it off the async threads too.
        tokio::task::spawn_blocking(move || drop(router))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn meta_endpoint_describes_what_the_caller_can_see() {
        let (r, auth) = real();
        auth.create_user(
            "root",
            "an admin password!",
            &["platform.admin".to_string()],
        )
        .unwrap();
        let get = |token: String| {
            let r = r.clone();
            async move {
                let resp = r
                    .oneshot(
                        Request::get("/api/meta")
                            .header("authorization", format!("Bearer {token}"))
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                let status = resp.status().as_u16();
                (
                    status,
                    serde_json::from_slice::<J>(
                        &resp.into_body().collect().await.unwrap().to_bytes(),
                    )
                    .unwrap_or(J::Null),
                )
            }
        };
        assert_eq!(get("obop_forged".into()).await.0, 401);
        let sam = login(&r, "sam", "another long password").await;
        let (s, b) = get(sam).await;
        assert_eq!(s, 200);
        let ids: Vec<_> = b["modules"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["id"].as_str().unwrap())
            .collect();
        assert!(ids.contains(&"crm") && ids.contains(&"base"));
        assert!(!ids.contains(&"invoicing"), "sam holds no invoicing group");
        let party = b["modules"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|m| m["models"].as_array().unwrap())
            .find(|m| m["name"] == "party")
            .unwrap();
        assert!(
            party["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["name"] == "tax_id" && f["class"] == "financial")
        );
        let root = login(&r, "root", "an admin password!").await;
        let (_, b) = get(root).await;
        assert!(
            b["modules"]
                .as_array()
                .unwrap()
                .iter()
                .any(|m| m["id"] == "platform"),
            "admins also get the customize models"
        );
    }

    async fn get_meta(r: &axum::Router, token: &str) -> J {
        let resp = r
            .clone()
            .oneshot(
                Request::get("/api/meta")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap()
    }

    /// The contract the UI builds on for D48 and D49: `can_delete`, the transition of each
    /// action, the shape of `search`, implicit transitions over REST, delete and its errors.
    #[tokio::test]
    async fn workflow_governance_delete_and_search_over_rest() {
        let r = dev();
        let meta = get_meta(&r, "dev-sales").await;
        let models: Vec<&J> = meta["modules"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|m| m["models"].as_array().unwrap())
            .collect();
        let lead = models.iter().find(|m| m["name"] == "lead").unwrap();
        assert_eq!(lead["can_delete"], true);
        let action = |id: &str| {
            lead["actions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["id"] == id)
                .unwrap_or_else(|| panic!("no action {id}"))
                .clone()
        };
        let won = action("lead.mark_won");
        assert_eq!(
            (&won["field"], &won["from"], &won["to"]),
            (&json!("stage"), &J::Null, &json!("won"))
        );
        let q = action("lead.qualify");
        assert_eq!(
            (&q["field"], &q["from"], &q["to"], &q["needs_approval"]),
            (
                &json!("stage"),
                &json!("new"),
                &json!("qualified"),
                &json!(false)
            )
        );
        // The generated commands are not listed as actions.
        assert!(
            lead["actions"].as_array().unwrap().iter().all(|a| !a["id"]
                .as_str()
                .unwrap()
                .ends_with(".delete")
                && !a["id"].as_str().unwrap().ends_with(".search"))
        );
        // A principal that may not write cannot delete: the invoice is not even visible.
        assert!(models.iter().all(|m| m["name"] != "invoice"));

        // A kanban drag is `update` with the new stage; it follows the transitions.
        let (_, rows) = post(&r, Some("dev-sales"), "/api/lead/list", json!({})).await;
        let id = rows["value"][0]["id"].as_str().unwrap().to_string();
        let stage = rows["value"][0]["stage"].as_str().unwrap().to_string();
        assert_eq!(stage, "qualified");
        let (s, b) = post(
            &r,
            Some("dev-sales"),
            "/api/lead/update",
            json!({"id": id, "stage": "qualified", "title": "kept"}),
        )
        .await;
        assert_eq!(s, 200, "unchanged stage is not a transition: {b}");
        let (s, b) = post(
            &r,
            Some("dev-sales"),
            "/api/lead/update",
            json!({"id": id, "stage": "won"}),
        )
        .await;
        assert_eq!((s, &b["value"]["stage"]), (200, &json!("won")), "{b}");
        let (s, b) = post(
            &r,
            Some("dev-sales"),
            "/api/lead/create",
            json!({"title": "x", "stage": "won"}),
        )
        .await;
        assert_eq!(s, 400);
        assert!(
            b["error"].as_str().unwrap().contains("starts as \"new\""),
            "{b}"
        );
        let (s, b) = post(
            &r,
            Some("dev-sales"),
            "/api/lead/update",
            json!({"id": id, "stage": "zzz"}),
        )
        .await;
        assert_eq!(s, 400);
        assert!(
            b["error"]
                .as_str()
                .unwrap()
                .contains("one of: new, qualified, won, lost"),
            "{b}"
        );

        // An agent proposing an implicit post gets the usual 202 plan; an admin approves it.
        let (_, inv) = post(&r, Some("dev-admin"), "/api/invoice/list", json!({})).await;
        let inv_id = inv["value"][0]["id"].as_str().unwrap().to_string();
        let (s, b) = post(
            &r,
            Some("dev-agent"),
            "/api/invoice/update",
            json!({"id": inv_id, "state": "posted"}),
        )
        .await;
        assert_eq!(s, 202, "{b}");
        assert_eq!(b["approval_required"], true);
        let plan = b["plan_id"].as_str().unwrap().to_string();
        let (_, now) = post(
            &r,
            Some("dev-admin"),
            "/api/invoice/get",
            json!({"id": inv_id}),
        )
        .await;
        assert_eq!(now["value"]["state"], "draft");
        let (s, b) = post(
            &r,
            Some("dev-admin"),
            &format!("/api/approve/{plan}"),
            json!({}),
        )
        .await;
        assert_eq!(s, 200, "{b}");
        assert_eq!(b["value"]["state"], "posted");
        assert_eq!(
            b["value"]["number"], "INV-00001",
            "the post action ran: {b}"
        );

        // search: {rows, total}; list keeps returning a bare array.
        let (s, b) = post(
            &r,
            Some("dev-sales"),
            "/api/lead/search",
            json!({"filter": [{"field": "title", "op": "contains", "value": "KEP"}], "limit": 1, "order": [{"field": "title", "dir": "desc"}]}),
        )
        .await;
        assert_eq!(s, 200, "{b}");
        assert_eq!(b["value"]["total"], 1);
        assert_eq!(b["value"]["rows"].as_array().unwrap().len(), 1);
        let (_, l) = post(&r, Some("dev-sales"), "/api/lead/list", json!({"limit": 1})).await;
        assert!(l["value"].is_array());
        // The agent cannot filter on a masked field; the error is the unknown-field one.
        let (s, b) = post(
            &r,
            Some("dev-agent"),
            "/api/party/search",
            json!({"filter": [{"field": "email", "op": "eq", "value": "ada@example.test"}]}),
        )
        .await;
        assert_eq!(s, 400);
        assert_eq!(b["error"], "unknown field party.email");
        let (_, b2) = post(
            &r,
            Some("dev-agent"),
            "/api/party/search",
            json!({"filter": [{"field": "nonexistent", "op": "eq", "value": "x"}]}),
        )
        .await;
        assert_eq!(b2["error"], "unknown field party.nonexistent");

        // delete: referenced records are refused with the referrers named; an agent proposes.
        let (_, ps) = post(&r, Some("dev-admin"), "/api/party/list", json!({})).await;
        let pid = ps["value"][0]["id"].as_str().unwrap().to_string();
        let (s, b) = post(
            &r,
            Some("dev-admin"),
            "/api/party/delete",
            json!({"id": pid}),
        )
        .await;
        assert_eq!(s, 400);
        assert!(
            b["error"]
                .as_str()
                .unwrap()
                .contains("still referenced by sale_order (1)"),
            "{b}"
        );
        let (_, leads) = post(&r, Some("dev-admin"), "/api/lead/list", json!({})).await;
        let lead_id = leads["value"][0]["id"].as_str().unwrap().to_string();
        let (s, b) = post(
            &r,
            Some("dev-agent"),
            "/api/lead/delete",
            json!({"id": lead_id}),
        )
        .await;
        assert_eq!((s, &b["approval_required"]), (202, &json!(true)));
        let plan = b["plan_id"].as_str().unwrap().to_string();
        let (s, _) = post(
            &r,
            Some("dev-admin"),
            &format!("/api/approve/{plan}"),
            json!({}),
        )
        .await;
        assert_eq!(s, 200);
        let (s, _) = post(
            &r,
            Some("dev-admin"),
            "/api/lead/get",
            json!({"id": lead_id}),
        )
        .await;
        assert_eq!(s, 400, "the lead is gone");
        // sam-like users who cannot write cannot delete: dev-sales has no invoicing groups.
        let (s, _) = post(
            &r,
            Some("dev-sales"),
            "/api/invoice/delete",
            json!({"id": inv_id}),
        )
        .await;
        assert_eq!(s, 404);
    }

    #[tokio::test]
    async fn meta_exposes_field_kinds_and_bad_types_are_http_400() {
        let (r, _) = real();
        let ada = login(&r, "ada", "correct horse battery").await;
        let resp = r
            .clone()
            .oneshot(
                Request::get("/api/meta")
                    .header("authorization", format!("Bearer {ada}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status().as_u16(), 200);
        let b: J =
            serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
        let field = |model: &str, name: &str| -> J {
            b["modules"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|m| m["models"].as_array().unwrap())
                .find(|m| m["name"] == model)
                .unwrap()["fields"]
                .as_array()
                .unwrap()
                .iter()
                .find(|f| f["name"] == name)
                .unwrap_or_else(|| panic!("{model}.{name}"))
                .clone()
        };
        let stage = field("lead", "stage");
        assert_eq!(stage["kind"], "select");
        assert_eq!(stage["options"], json!(["new", "qualified", "won", "lost"]));
        assert_eq!(stage["required"], false);
        assert!(stage["class"].is_string() && stage["personal_data"].is_boolean());
        let party = field("sale_order", "party_id");
        assert_eq!(
            (party["kind"].as_str(), party["to"].as_str()),
            (Some("ref"), Some("party"))
        );
        assert!(party.get("options").is_none());
        let total = field("sale_order", "total_minor");
        assert_eq!(total["kind"], "money");
        assert!(total.get("to").is_none() && total.get("options").is_none());
        assert_eq!(field("party", "name")["required"], true);
        assert_eq!(field("party", "notes")["kind"], "longtext");
        assert_eq!(field("party", "email")["kind"], "text");

        let (s, e) = post(
            &r,
            Some(&ada),
            "/api/sale_order/create",
            json!({"total_minor": 12.5}),
        )
        .await;
        assert_eq!(s, 400, "{e}");
        assert!(
            e.to_string()
                .contains("sale_order.total_minor must be an integer in minor currency units"),
            "{e}"
        );
        let (s, e) = post(
            &r,
            Some(&ada),
            "/api/lead/create",
            json!({"title": "x", "stage": "maybe"}),
        )
        .await;
        assert_eq!(s, 400);
        assert!(
            e.to_string()
                .contains("lead.stage must be one of: new, qualified, won, lost"),
            "{e}"
        );
        let (s, _) = post(
            &r,
            Some(&ada),
            "/api/sale_order/create",
            json!({"party_id": "ghost"}),
        )
        .await;
        assert_eq!(s, 400);
        let (s, b) = post(
            &r,
            Some(&ada),
            "/api/lead/create",
            json!({"title": "ok", "stage": "new", "expected_revenue_minor": 1000}),
        )
        .await;
        assert_eq!(s, 200, "{b}");
    }

    #[tokio::test]
    async fn openapi_documents_are_served_and_unknown_api_paths_answer_json() {
        let (r, _) = real();
        let get = |r: axum::Router, path: &'static str, token: Option<String>| async move {
            let mut b = Request::get(path).header("host", "inst.test");
            if let Some(t) = token {
                b = b.header("authorization", format!("Bearer {t}"));
            }
            let resp = r.oneshot(b.body(Body::empty()).unwrap()).await.unwrap();
            let status = resp.status().as_u16();
            let ct = resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            let body: J =
                serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes())
                    .unwrap_or(J::Null);
            (status, ct, body)
        };
        // Anonymous: only sign-in and discovery; signed in: the commands this token may use.
        let (s, _, anon) = get(r.clone(), "/api/openapi.json", None).await;
        assert_eq!(s, 200);
        assert_eq!(anon["openapi"], "3.1.0");
        assert!(
            anon["paths"]["/api/auth/login"].is_object()
                && anon["paths"]["/api/party/list"].is_null()
        );
        let sam = login(&r, "sam", "another long password").await;
        let (_, _, doc) = get(r.clone(), "/api/openapi.json", Some(sam)).await;
        assert!(doc["paths"]["/api/party/list"]["post"]["operationId"].is_string());
        assert!(
            doc["paths"]["/api/invoice/list"].is_null(),
            "sam holds no invoicing group"
        );
        assert_eq!(
            doc["components"]["schemas"]["party"]["properties"]["name"]["type"],
            "string"
        );
        // Unknown API paths are JSON with a code and a hint, never the HTML shell.
        let (s, ct, body) = get(r.clone(), "/api/definitely/not/there", None).await;
        assert_eq!((s, ct.as_str()), (404, "application/json"));
        assert_eq!(body["code"], "not_found");
        assert!(body["hint"].is_string());
    }

    #[tokio::test]
    async fn the_control_plane_publishes_openapi_and_structured_errors() {
        let backend = obop_storage::open("memory:").unwrap();
        let platform = obop_platform::Platform::new(
            Arc::new(Auth::with_test_params(backend.clone())),
            backend,
            Arc::new(obop_platform::provision::Fake::default()),
        );
        let r = obop_http::platform_router(Arc::new(obop_platform::Api(platform)), None);
        let resp = r
            .clone()
            .oneshot(
                Request::get("/openapi.json")
                    .header("host", "x.test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let doc: J =
            serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert_eq!(
            doc["paths"]["/api/platform/instances"]["post"]["operationId"],
            "createInstance"
        );
        let (s, b) = post(&r, None, "/api/platform/instances", json!({"name": "acme"})).await;
        assert_eq!((s, b["code"].as_str()), (401, Some("unauthorized")));
        assert!(b["hint"].is_string());
    }

    #[tokio::test]
    async fn platform_signup_to_instance_over_http() {
        let backend = obop_storage::open("memory:").unwrap();
        let platform = obop_platform::Platform::new(
            Arc::new(Auth::with_test_params(backend.clone())),
            backend,
            Arc::new(obop_platform::provision::Fake::default()),
        );
        let r = obop_http::platform_router(Arc::new(obop_platform::Api(platform)), None);
        let (s, b) = post(
            &r,
            None,
            "/api/platform/signup",
            json!({"email": "ada@example.test", "password": "correct horse battery"}),
        )
        .await;
        assert_eq!(s, 200, "{b}");
        let tok = b["token"].as_str().unwrap().to_string();
        assert_eq!(
            post(&r, None, "/api/platform/instances", json!({"name": "acme"}))
                .await
                .0,
            401
        );
        let (s, b) = post(
            &r,
            Some(&tok),
            "/api/platform/instances",
            json!({"name": "Acme Trading"}),
        )
        .await;
        assert_eq!((s, b["slug"].as_str()), (202, Some("acme-trading")));
        let id = b["id"].as_str().unwrap().to_string();
        let mut status = String::new();
        for _ in 0..100 {
            let resp = r
                .clone()
                .oneshot(
                    Request::get("/api/platform/instances")
                        .header("authorization", format!("Bearer {tok}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let v: J =
                serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            status = v["instances"][0]["status"].as_str().unwrap().to_string();
            if status == "running" {
                assert_eq!(v["instances"][0]["url"], "https://acme-trading.fake.test");
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(status, "running");
        let path = format!("/api/platform/instances/{id}/delete");
        let (s, b) = post(&r, Some(&tok), &path, json!({})).await;
        assert_eq!((s, b["status"].as_str()), (202, Some("deleting")));
        assert!(
            b.get("phase").is_some(),
            "the live phase is part of the record"
        );
        // Once the background teardown finishes the instance is gone.
        let mut last = 0;
        for _ in 0..100 {
            last = post(&r, Some(&tok), &path, json!({})).await.0;
            if last == 404 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(last, 404);
    }

    /// The control-plane router with the forum and status page merged in, as `platform serve`
    /// builds it: the new routes work and the platform API and static fallback are untouched.
    #[tokio::test]
    async fn community_and_status_do_not_shadow_the_platform_routes() {
        let dir = std::env::temp_dir().join(format!("obop-site-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.html"), "<h1>static home</h1>").unwrap();
        let backend = obop_storage::open("memory:").unwrap();
        let auth = Arc::new(Auth::with_test_params(backend.clone()));
        let platform = obop_platform::Platform::new(
            auth.clone(),
            backend.clone(),
            Arc::new(obop_platform::provision::Fake::default()),
        );
        let community =
            obop_community::Community::open(obop_community::Config::default(), auth, backend)
                .unwrap();
        let r = obop_http::platform_router(Arc::new(obop_platform::Api(platform)), dir.to_str())
            .merge(obop_http::community::router(community));
        let text = |resp: axum::response::Response| async move {
            let s = resp.status().as_u16();
            let b = resp.into_body().collect().await.unwrap().to_bytes();
            (s, String::from_utf8_lossy(&b).into_owned())
        };
        let (s, b) = text(
            r.clone()
                .oneshot(Request::get("/").body(Body::empty()).unwrap())
                .await
                .unwrap(),
        )
        .await;
        assert_eq!((s, b.as_str()), (200, "<h1>static home</h1>"));
        let (s, b) = text(
            r.clone()
                .oneshot(Request::get("/community").body(Body::empty()).unwrap())
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(s, 200);
        assert!(b.contains("obop community") && b.contains("Announcements"));
        let (s, b) = text(
            r.clone()
                .oneshot(
                    Request::get("/community/t/1")
                        .header("accept", "text/markdown")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(s, 404, "{b}");
        let (s, b) = text(
            r.clone()
                .oneshot(
                    Request::get("/status/api/v2/status.json")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(s, 200);
        assert!(b.contains("\"indicator\":\"none\""), "{b}");
        // A POST body larger than the limit is refused before it reaches the application.
        let big = "x=".to_string() + &"a".repeat(70_000);
        let resp = r
            .clone()
            .oneshot(
                Request::post("/community/login")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from(big))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status().as_u16(), 413);
        // The JSON platform API still works next to it.
        let (s, b) = post(
            &r,
            None,
            "/api/platform/signup",
            json!({"email": "ada@example.test", "password": "correct horse battery"}),
        )
        .await;
        assert_eq!(s, 200, "{b}");
        assert!(b["token"].as_str().is_some());
        let (s, _) = post(
            &r,
            None,
            "/api/platform/login",
            json!({"email": "ada@example.test", "password": "wrong wrong wrong"}),
        )
        .await;
        assert_eq!(s, 401);
        // The same account signs in to the forum.
        let resp = r
            .clone()
            .oneshot(
                Request::get("/community/login")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let csrf = resp
            .headers()
            .get_all("set-cookie")
            .iter()
            .find_map(|c| {
                c.to_str()
                    .ok()?
                    .strip_prefix("obop_csrf=")
                    .map(|v| v.split(';').next().unwrap_or("").to_string())
            })
            .expect("csrf cookie");
        let resp = r
            .clone()
            .oneshot(
                Request::post("/community/login")
                    .header("cookie", format!("obop_csrf={csrf}"))
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from(format!(
                        "csrf={csrf}&email=ada%40example.test&password=correct+horse+battery"
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status().as_u16(), 303);
        assert!(
            resp.headers()
                .get_all("set-cookie")
                .iter()
                .any(|c| c.to_str().unwrap().starts_with("obop_session="))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn modules_check_accepts_every_shipped_package_and_fails_on_a_bad_one() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut files = Vec::new();
        for dir in ["packages", "docs/modules"] {
            for e in std::fs::read_dir(root.join(dir)).unwrap() {
                let p = e.unwrap().path();
                if p.extension().is_some_and(|x| x == "toml") {
                    files.push(p.to_string_lossy().to_string());
                }
            }
        }
        assert!(files.len() >= 23, "{files:?}");
        let refs: Vec<&str> = files.iter().map(String::as_str).collect();
        assert!(modules_check_files(&refs, false).is_ok());

        let builtin = App::new().modules;
        let fleet = std::fs::read_to_string(root.join("packages/fleet.toml")).unwrap();
        for (broken, want) in [
            (
                fleet.replace("class = \"none\"", "class = \"bogus\""),
                "unknown data class",
            ),
            (
                std::fs::read_to_string(root.join("packages/accounting.toml"))
                    .unwrap()
                    .replace("base = \"^0.2\"", "ghost = \"^0.2\""),
                "needs module ghost",
            ),
            (format!("{fleet}\nbogus = 1"), "invalid obop.toml"),
        ] {
            let errors = obop_pkg::check_manifest(&broken, &builtin);
            assert!(
                errors.iter().any(|e| e.contains(want)),
                "{want}: {errors:?}"
            );
        }
        let dir = std::env::temp_dir().join(format!("obop-check-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bad = dir.join("bad.toml");
        std::fs::write(&bad, fleet.replace("0.3.0", "2")).unwrap();
        let good = root.join("packages/fleet.toml");
        let (bad_s, good_s) = (
            bad.to_string_lossy().to_string(),
            good.to_string_lossy().to_string(),
        );
        assert!(
            modules_check_files(&[&bad_s], false).is_err(),
            "non-zero exit"
        );
        assert!(
            modules_check_files(&[&good_s, &bad_s], false).is_err(),
            "one bad file fails the run"
        );
        assert!(modules_check_files(&["/no/such/file.toml"], false).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn dev_auth_binds_only_loopback() {
        assert!(
            is_loopback("127.0.0.1:8080") && is_loopback("localhost:1") && is_loopback("[::1]:9")
        );
        assert!(!is_loopback("0.0.0.0:8080") && !is_loopback("10.0.0.5:80"));
    }

    #[test]
    fn all_groups_cover_every_module() {
        let g = App::new().all_groups();
        for want in [
            "base.user",
            "privacy.officer",
            "product.cost_viewer",
            "invoicing.manager",
        ] {
            assert!(g.iter().any(|x| x == want), "{want}");
        }
    }

    #[test]
    fn every_linked_module_passes_the_gate() {
        assert!(
            App::new()
                .modules
                .iter()
                .all(|m| obop_registry::check_module(m).is_empty())
        );
    }
    /// D52 over HTTP: `/api/meta` carries `labels`, `descriptions` and `option_labels` exactly
    /// where a translation exists, for built-in modules and for installed packages, and nothing
    /// (not even an empty map) where there is none.
    #[tokio::test]
    async fn meta_endpoint_carries_server_labels_only_where_translated() {
        let (r, auth) = real();
        auth.create_user(
            "root",
            "an admin password!",
            &["platform.admin".to_string()],
        )
        .unwrap();
        let admin = &login(&r, "root", "an admin password!").await;
        let partial = format!(
            "{}\n\n{}",
            include_str!("../../../packages/fleet.toml").replace("0.2.0", "0.3.0"),
            r#"
[labels.de.module]
name = "Fuhrpark"
description = "Fahrzeuge und Fahrer"

[labels.de.model.fleet_vehicle]
name = "Fahrzeug"

[labels.de.model.fleet_vehicle.field.plate]
label = "Kennzeichen"
description = "Amtliches Kennzeichen"

[labels.de.model.fleet_vehicle.field.fuel.options]
diesel = "Diesel"
electric = "Elektro"

[labels.de.model.fleet_vehicle.action.dispatch]
label = "In Betrieb nehmen"
"#
        );
        let (s, b) = post(
            &r,
            Some(admin.as_str()),
            "/api/module/install",
            json!({"manifest": partial}),
        )
        .await;
        assert_eq!(s, 200, "{b}");

        let meta = get_meta(&r, admin.as_str()).await;
        let modules = meta["modules"].as_array().unwrap();
        let fleet = modules.iter().find(|m| m["id"] == "fleet").unwrap();
        assert_eq!(fleet["name"], "Fleet");
        assert_eq!(fleet["labels"], json!({"de": "Fuhrpark"}));
        assert_eq!(fleet["descriptions"], json!({"de": "Fahrzeuge und Fahrer"}));
        assert!(
            fleet["description"]
                .as_str()
                .unwrap()
                .starts_with("Vehicles")
        );
        let vehicle = fleet["models"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["name"] == "fleet_vehicle")
            .unwrap();
        assert_eq!(vehicle["labels"], json!({"de": "Fahrzeug"}));
        assert!(
            vehicle.get("descriptions").is_none(),
            "no description translated, so no key"
        );
        let field = |n: &str| {
            vehicle["fields"]
                .as_array()
                .unwrap()
                .iter()
                .find(|f| f["name"] == n)
                .unwrap()
                .clone()
        };
        let plate = field("plate");
        assert_eq!(plate["labels"], json!({"de": "Kennzeichen"}));
        assert_eq!(
            plate["descriptions"],
            json!({"de": "Amtliches Kennzeichen"})
        );
        assert!(plate.get("option_labels").is_none());
        let fuel = field("fuel");
        assert!(fuel.get("labels").is_none() && fuel.get("descriptions").is_none());
        assert_eq!(
            fuel["option_labels"],
            json!({"diesel": {"de": "Diesel"}, "electric": {"de": "Elektro"}})
        );
        assert_eq!(
            fuel["options"][0], "petrol",
            "the options themselves stay English"
        );
        let vin = field("vin");
        assert!(vin.get("labels").is_none() && vin.get("option_labels").is_none());
        let action = |id: &str| {
            vehicle["actions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["id"] == id)
                .unwrap()
                .clone()
        };
        assert_eq!(
            action("fleet_vehicle.dispatch")["labels"],
            json!({"de": "In Betrieb nehmen"})
        );
        assert!(
            action("fleet_vehicle.return_to_pool")
                .get("labels")
                .is_none()
        );

        // Built-in modules ship all six languages, declared in their registry metadata.
        let crm = modules.iter().find(|m| m["id"] == "crm").unwrap();
        for lang in ["de", "fr", "pl", "fi", "sv", "es"] {
            assert!(crm["labels"][lang].is_string(), "crm module {lang}");
            let lead = &crm["models"][0];
            assert!(lead["labels"][lang].is_string(), "lead {lang}");
            let stage = lead["fields"]
                .as_array()
                .unwrap()
                .iter()
                .find(|f| f["name"] == "stage")
                .unwrap();
            assert!(stage["labels"][lang].is_string(), "stage {lang}");
            for o in ["new", "qualified", "won", "lost"] {
                assert!(stage["option_labels"][o][lang].is_string(), "{o} {lang}");
            }
            let qualify = lead["actions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["id"] == "lead.qualify")
                .unwrap();
            assert!(qualify["labels"][lang].is_string(), "qualify {lang}");
        }
        let stage = crm["models"][0]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == "stage")
            .unwrap();
        assert_ne!(
            stage["option_labels"]["won"]["de"], "Won",
            "translated, not copied"
        );

        // The admin app itself is translated too.
        let platform = modules.iter().find(|m| m["id"] == "platform").unwrap();
        assert!(platform["labels"]["fi"].is_string());

        // Uninstalling removes the labels with the module.
        let (s, _) = post(
            &r,
            Some(admin.as_str()),
            "/api/module/uninstall",
            json!({"id": "fleet"}),
        )
        .await;
        assert_eq!(s, 200);
        let meta = get_meta(&r, admin.as_str()).await;
        assert!(
            meta["modules"]
                .as_array()
                .unwrap()
                .iter()
                .all(|m| m["id"] != "fleet")
        );
    }
}

/// Every bundled package, installed through the real installer and exercised through the one
/// dispatcher: sample records for every model, every declared action, the metadata gates, and
/// admin uninstall and reinstall.
#[cfg(test)]
mod bundled_packages {
    use super::*;
    use obop_commands::{ADMIN_GROUP, CallError, FieldInfo, ModelInfo};
    use obop_pkg::{Bundled, Manifest, Marketplace};
    use std::collections::{BTreeMap, BTreeSet, HashMap};

    /// Where the greedy walk over each model's actions (first action leading to a state not
    /// seen yet) must end, starting from the first option of `state`.
    const FINAL_STATE: &[(&str, &str)] = &[
        ("accounting_bank_statement_line", "reconciled"),
        ("accounting_fiscal_year", "closed"),
        ("accounting_move", "reversed"),
        ("accounting_period", "closed"),
        ("ecommerce_order", "refunded"),
        ("email_marketing_campaign", "sent"),
        ("field_service_job", "done"),
        ("knowledge_article", "archived"),
        ("planning_shift", "confirmed"),
        ("pos_order", "refunded"),
        ("pos_session", "closed"),
        ("sign_request", "signed"),
        ("website_blog_post", "archived"),
        ("website_form_submission", "handled"),
        ("website_page", "archived"),
        ("appointments_appointment", "done"),
        ("documents_document", "archived"),
        ("expenses_expense", "paid"),
        ("fleet_vehicle", "in_service"),
        ("fleet_vehicle_driver", "suspended"),
        ("fleet_vehicle_service", "done"),
        ("helpdesk_ticket", "closed"),
        ("hr_employee", "offboarded"),
        ("hr_leave_request", "approved"),
        ("inventory_move", "done"),
        ("manufacturing_order", "done"),
        ("projects_milestone", "reached"),
        ("projects_project", "done"),
        ("projects_task", "done"),
        ("purchase_order", "received"),
        ("subscriptions_subscription", "cancelled"),
        ("timesheets_entry", "approved"),
    ];

    fn call(
        app: &App,
        p: &Principal,
        id: &str,
        input: Value,
    ) -> Result<obop_commands::Response, CallError> {
        app.dispatcher
            .call(Surface::Rest, p, &mut Vault::new(), id, &input)
    }

    fn by_id(id: &str) -> Value {
        Value::map([("id", Value::text(id))])
    }

    fn id_of(r: &obop_commands::Response) -> String {
        match &r.value {
            Value::Map(m) => match &m["id"] {
                Value::Text(t) => t.clone(),
                other => panic!("id is not text: {other:?}"),
            },
            other => panic!("no record in {other:?}"),
        }
    }

    fn all_manifests() -> Vec<Manifest> {
        Bundled
            .list()
            .unwrap()
            .iter()
            .map(|l| Manifest::parse(&Bundled.fetch(&l.id).unwrap()).unwrap())
            .collect()
    }

    /// Dependency order computed from `depends`; built-in module ids count as installed.
    fn install_order(app: &App, mut left: Vec<Manifest>) -> Vec<Manifest> {
        let mut have: BTreeSet<String> = app.modules.iter().map(|m| m.id.clone()).collect();
        let mut out = Vec::new();
        while !left.is_empty() {
            let at = left
                .iter()
                .position(|m| m.dependency_ids().iter().all(|d| have.contains(d)))
                .expect("a dependency cycle or a dependency that no package provides");
            let m = left.remove(at);
            have.insert(m.module.id.clone());
            out.push(m);
        }
        out
    }

    fn models_of(app: &App, p: &Principal) -> HashMap<String, ModelInfo> {
        app.dispatcher
            .meta(Surface::Rest, p)
            .into_iter()
            .flat_map(|m| m.models)
            .map(|m| (m.name.clone(), m))
            .collect()
    }

    /// Builds sample records from field metadata, creating referenced records first.
    struct Maker<'a> {
        app: &'a App,
        meta: HashMap<String, ModelInfo>,
        pkg_models: BTreeSet<String>,
        pkg_user: Principal,
        admin: Principal,
        made: HashMap<String, String>,
    }

    impl Maker<'_> {
        fn input(&mut self, model: &str, state: Option<&str>, open: &mut Vec<String>) -> Value {
            let fields: Vec<FieldInfo> = self.meta[model].fields.clone();
            let mut m = BTreeMap::new();
            for f in fields {
                let v = match f.kind {
                    "text" => Value::text("sample"),
                    "longtext" => Value::text("a longer sample text"),
                    "number" => Value::Int(1),
                    "money" => Value::Int(12_345),
                    "date" => Value::text("2026-01-31"),
                    "bool" => Value::Bool(true),
                    "select" => match (f.name, state) {
                        ("state", Some(s)) => Value::text(s),
                        _ => Value::text(f.options[0]),
                    },
                    "ref" => {
                        let to = f.to.expect("ref has a target");
                        if open.iter().any(|o| o == to) {
                            assert!(!f.required, "{model}.{} refers to itself", f.name);
                            continue;
                        }
                        Value::Text(self.record(to, open))
                    }
                    other => panic!("{model}.{}: unknown kind {other}", f.name),
                };
                m.insert(f.name.to_string(), v);
            }
            Value::Map(m)
        }

        /// A record of `model` in its first state, reused when already made.
        fn record(&mut self, model: &str, open: &mut Vec<String>) -> String {
            if let Some(id) = self.made.get(model) {
                return id.clone();
            }
            open.push(model.to_string());
            let input = self.input(model, None, open);
            open.pop();
            let id = self.create(model, input);
            self.made.insert(model.to_string(), id.clone());
            id
        }

        fn create(&self, model: &str, input: Value) -> String {
            let p = if self.pkg_models.contains(model) {
                &self.pkg_user
            } else {
                &self.admin
            };
            let r = call(self.app, p, &format!("{model}.create"), input)
                .unwrap_or_else(|e| panic!("{model}.create failed: {e:?}"));
            id_of(&r)
        }

        /// A fresh record of `model` whose state starts as `state`. Written to the store
        /// directly: `create` only accepts the first state (D48).
        fn fresh(&mut self, model: &str, state: &str) -> String {
            let mut open = vec![model.to_string()];
            let Value::Map(fields) = self.input(model, Some(state), &mut open) else {
                unreachable!()
            };
            self.app.store.insert(model, fields).unwrap()
        }
    }

    fn state_of(app: &App, model: &str, id: &str) -> String {
        match app.store.field(model, id, "state") {
            Some(Value::Text(t)) => t,
            other => panic!("{model} {id} has no state: {other:?}"),
        }
    }

    #[test]
    fn every_bundled_package_installs_and_every_model_and_action_works() {
        let app = App::new();
        let manifests = install_order(&app, all_manifests());
        assert!(manifests.len() >= 22);
        let admin = Principal::user("root", &[ADMIN_GROUP]);
        let mut seen_special = false;
        let mut seen_financial = false;

        for m in &manifests {
            let id = &m.module.id;
            app.installer
                .install(&Bundled.fetch(id).unwrap(), "marketplace", "test")
                .unwrap_or_else(|e| panic!("{id} did not install: {e}"));

            let groups = m.groups();
            let group_refs: Vec<&str> = groups.iter().map(String::as_str).collect();
            let user = Principal::user("tester", &group_refs);
            let visible = models_of(&app, &user);
            let admin_meta = models_of(&app, &admin);
            let pkg_models: BTreeSet<String> = m.model.iter().map(|x| x.name.clone()).collect();

            // The metadata of every field: described, classified, never personal data as `none`.
            for model in &m.model {
                let info = visible
                    .get(&model.name)
                    .unwrap_or_else(|| panic!("{} is not visible to its own groups", model.name));
                assert_eq!(info.fields.len(), model.field.len(), "{}", model.name);
                assert_eq!(info.actions.len(), model.action.len(), "{}", model.name);
                assert!(info.can_create && info.can_update, "{}", model.name);
                // D52: the metadata a client renders carries all six languages.
                for lang in ["de", "fr", "pl", "fi", "sv", "es"] {
                    assert!(info.labels.contains_key(lang), "{} {lang}", model.name);
                    assert!(
                        info.descriptions.contains_key(lang),
                        "{} {lang}",
                        model.name
                    );
                    for f in &info.fields {
                        let at = format!("{}.{} {lang}", model.name, f.name);
                        assert!(f.labels.contains_key(lang), "{at}");
                        assert!(f.descriptions.contains_key(lang), "{at}");
                        for o in &f.options {
                            assert!(f.option_labels[*o].contains_key(lang), "{at} {o}");
                        }
                    }
                    for a in &info.actions {
                        assert!(a.labels.contains_key(lang), "{} {lang}", a.id);
                        assert!(a.descriptions.contains_key(lang), "{} {lang}", a.id);
                    }
                }
                for f in &info.fields {
                    assert!(
                        !f.description.trim().is_empty(),
                        "{}.{}",
                        model.name,
                        f.name
                    );
                    assert!(
                        !(f.personal_data && f.class == "none"),
                        "{}.{} is personal data classed none",
                        model.name,
                        f.name
                    );
                    seen_special |= id == "hr" && f.class == "special_category";
                    seen_financial |= id == "hr" && f.class == "financial";
                }
                for a in &model.action {
                    let c = info
                        .actions
                        .iter()
                        .find(|c| c.id == format!("{}.{}", model.name, a.name))
                        .unwrap_or_else(|| panic!("{}.{} not exposed", model.name, a.name));
                    assert_eq!(c.needs_approval, a.approval, "{}.{}", model.name, a.name);
                }
            }

            let mut mk = Maker {
                app: &app,
                meta: admin_meta,
                pkg_models,
                pkg_user: user.clone(),
                admin: admin.clone(),
                made: HashMap::new(),
            };

            for model in &m.model {
                let name = model.name.as_str();
                // A record with every field filled in is accepted and can be read back.
                let rec = mk.record(name, &mut Vec::new());
                call(&app, &user, &format!("{name}.get"), by_id(&rec)).unwrap();
                let list = call(
                    &app,
                    &user,
                    &format!("{name}.list"),
                    Value::Map(Default::default()),
                )
                .unwrap();
                assert!(
                    matches!(list.value, Value::List(l) if !l.is_empty()),
                    "{name}"
                );
                // A required field cannot be left out.
                if let Some(req) = model.field.iter().find(|f| f.required) {
                    let Value::Map(mut input) = mk.input(name, None, &mut vec![name.to_string()])
                    else {
                        unreachable!()
                    };
                    input.remove(&req.name);
                    let e = call(&app, &user, &format!("{name}.create"), Value::Map(input))
                        .unwrap_err();
                    assert!(
                        matches!(e, CallError::Handler(ref s) if s.contains("required")),
                        "{name}: {e:?}"
                    );
                }
                if model.action.is_empty() {
                    continue;
                }

                // Each action works from its `from` state and is refused out of order.
                for a in &model.action {
                    let from = a.from.as_deref().unwrap();
                    let rec = mk.fresh(name, from);
                    let cmd = format!("{name}.{}", a.name);
                    call(&app, &user, &cmd, by_id(&rec)).unwrap();
                    assert_eq!(state_of(&app, name, &rec), a.to, "{cmd}");
                    assert!(call(&app, &user, &cmd, by_id(&rec)).is_err(), "{cmd} twice");
                    // Agents may only propose an action that needs approval.
                    let bot = Principal::agent("bot", &group_refs, &group_refs);
                    let again = mk.fresh(name, from);
                    let r = call(&app, &bot, &cmd, by_id(&again));
                    if a.approval {
                        assert!(
                            matches!(r, Err(CallError::ApprovalRequired { .. })),
                            "{cmd}"
                        );
                        assert_eq!(state_of(&app, name, &again), from, "{cmd} proposed only");
                    } else {
                        r.unwrap_or_else(|e| panic!("{cmd} for an agent: {e:?}"));
                    }
                }

                // The workflow, walked from its first state, ends where it should.
                let states = model.field.iter().find(|f| f.name == "state").unwrap();
                let mut now = states.options[0].clone();
                let rec = mk.fresh(name, &now);
                let mut visited = BTreeSet::from([now.clone()]);
                while let Some(a) = model
                    .action
                    .iter()
                    .find(|a| a.from.as_deref() == Some(now.as_str()) && !visited.contains(&a.to))
                {
                    call(&app, &user, &format!("{name}.{}", a.name), by_id(&rec)).unwrap();
                    now = a.to.clone();
                    visited.insert(now.clone());
                    assert_eq!(state_of(&app, name, &rec), now);
                }
                let want = FINAL_STATE
                    .iter()
                    .find(|(n, _)| *n == name)
                    .unwrap_or_else(|| panic!("no expected final state for {name}"))
                    .1;
                assert_eq!(now, want, "{name} workflow ends in {want}");

                // Governed state (D48): the same walk as a kanban drag, one `update` per step,
                // lands in the same place.
                let first = states.options[0].clone();
                let by_update = mk.fresh(name, &first);
                let mut now = first.clone();
                let mut visited = BTreeSet::from([now.clone()]);
                while let Some(a) = model
                    .action
                    .iter()
                    .find(|a| a.from.as_deref() == Some(now.as_str()) && !visited.contains(&a.to))
                {
                    call(
                        &app,
                        &user,
                        &format!("{name}.update"),
                        Value::map([
                            ("id", Value::text(&by_update)),
                            ("state", Value::text(&a.to)),
                        ]),
                    )
                    .unwrap_or_else(|e| panic!("{name}: update to {} failed: {e:?}", a.to));
                    now = a.to.clone();
                    visited.insert(now.clone());
                    assert_eq!(state_of(&app, name, &by_update), now);
                }
                assert_eq!(now, want, "{name}: the walk by update ends in {want}");

                // Bypass attempts: a late state on create, and a jump no action allows.
                let late = states.options.last().unwrap().clone();
                if late != first {
                    let Value::Map(mut input) =
                        mk.input(name, Some(&late), &mut vec![name.to_string()])
                    else {
                        unreachable!()
                    };
                    input.insert("state".into(), Value::text(&late));
                    let e = call(&app, &user, &format!("{name}.create"), Value::Map(input))
                        .unwrap_err();
                    assert!(
                        matches!(e, CallError::Handler(ref s) if s.contains("starts as")),
                        "{name}: {e:?}"
                    );
                }
                let reachable: BTreeSet<&str> = model
                    .action
                    .iter()
                    .filter(|a| a.from.as_deref().is_none_or(|f| f == first))
                    .map(|a| a.to.as_str())
                    .collect();
                if let Some(jump) = states
                    .options
                    .iter()
                    .find(|o| **o != first && !reachable.contains(o.as_str()))
                {
                    let rec = mk.fresh(name, &first);
                    let e = call(
                        &app,
                        &user,
                        &format!("{name}.update"),
                        Value::map([("id", Value::text(&rec)), ("state", Value::text(jump))]),
                    )
                    .unwrap_err();
                    assert!(
                        matches!(e, CallError::Handler(ref s) if s.contains("cannot change from")),
                        "{name}: {e:?}"
                    );
                    assert_eq!(state_of(&app, name, &rec), first);
                }
            }
        }

        // hr is classified strictly, and pay is hidden from those who only read the directory.
        assert!(seen_special && seen_financial);
        let reader = Principal::user("r", &["hr.user"]);
        let employee = &models_of(&app, &reader)["hr_employee"];
        let names: Vec<_> = employee.fields.iter().map(|f| f.name).collect();
        assert!(names.contains(&"name") && names.contains(&"work_email"));
        for hidden in [
            "salary",
            "national_id",
            "bank_iban",
            "health_note",
            "union_member",
        ] {
            assert!(
                !names.contains(&hidden),
                "{hidden} must need a stricter group"
            );
        }
        let manager = Principal::user("m", &["hr.user", "hr.manager"]);
        let names: Vec<_> = models_of(&app, &manager)["hr_employee"]
            .fields
            .iter()
            .map(|f| f.name)
            .collect();
        assert!(names.contains(&"salary") && !names.contains(&"health_note"));
    }

    #[test]
    fn platform_admin_uninstalls_and_reinstalls_every_package_without_losing_data() {
        let app = App::new();
        let admin = Principal::user("root", &[ADMIN_GROUP]);
        let manifests = install_order(&app, all_manifests());
        for m in &manifests {
            call(
                &app,
                &admin,
                "marketplace_listing.install",
                by_id(&m.module.id),
            )
            .unwrap();
        }
        // One record per model, made as admin (who holds every group).
        let mut mk = Maker {
            app: &app,
            meta: models_of(&app, &admin),
            pkg_models: BTreeSet::new(),
            pkg_user: admin.clone(),
            admin: admin.clone(),
            made: HashMap::new(),
        };
        let mut counts = Vec::new();
        for m in &manifests {
            for model in &m.model {
                mk.record(&model.name, &mut Vec::new());
                counts.push((model.name.clone(), app.store.count(&model.name)));
            }
        }
        assert!(counts.iter().all(|(_, n)| *n >= 1));

        // A package other packages depend on cannot be removed first.
        assert!(
            manifests
                .iter()
                .any(|m| m.dependency_ids().contains(&"projects".to_string()))
        );
        assert!(call(&app, &admin, "module.uninstall", by_id("projects")).is_err());
        assert!(app.dispatcher.has_model("projects_task"));
        // The same holds for the packages that refer to people and to other apps.
        for (dep, model) in [("hr", "hr_employee"), ("accounting", "accounting_move")] {
            if manifests
                .iter()
                .any(|m| m.dependency_ids().contains(&dep.to_string()))
            {
                assert!(
                    call(&app, &admin, "module.uninstall", by_id(dep)).is_err(),
                    "{dep}"
                );
                assert!(app.dispatcher.has_model(model), "{dep} still installed");
            }
        }

        for m in manifests.iter().rev() {
            call(&app, &admin, "module.uninstall", by_id(&m.module.id))
                .unwrap_or_else(|e| panic!("uninstall {}: {e:?}", m.module.id));
            for model in &m.model {
                assert!(!app.dispatcher.has_model(&model.name), "{}", model.name);
            }
        }
        for m in &manifests {
            call(
                &app,
                &admin,
                "marketplace_listing.install",
                by_id(&m.module.id),
            )
            .unwrap();
            for model in &m.model {
                let want = counts.iter().find(|(n, _)| *n == model.name).unwrap().1;
                assert_eq!(
                    app.store.count(&model.name),
                    want,
                    "{} data kept",
                    model.name
                );
                let list = call(
                    &app,
                    &admin,
                    &format!("{}.list", model.name),
                    Value::Map(Default::default()),
                )
                .unwrap();
                assert!(matches!(list.value, Value::List(l) if l.len() == want));
            }
        }
    }

    /// The apps the landing page promises are all in the catalogue.
    #[test]
    fn the_promised_apps_are_all_bundled() {
        let ids: BTreeSet<String> = all_manifests().into_iter().map(|m| m.module.id).collect();
        for id in [
            "accounting",
            "planning",
            "field_service",
            "pos",
            "ecommerce",
            "website",
            "email_marketing",
            "sign",
            "dashboards",
            "knowledge",
        ] {
            assert!(ids.contains(id), "{id} is missing from packages/");
        }
    }

    /// Money-moving or irreversible steps of the new apps need an approval and the stricter
    /// group, so a plain user of the app cannot take them.
    #[test]
    fn new_apps_separate_duties_on_the_steps_that_matter() {
        let app = App::new();
        let admin = Principal::user("root", &[ADMIN_GROUP]);
        for m in install_order(&app, all_manifests()) {
            app.installer
                .install(&Bundled.fetch(&m.module.id).unwrap(), "marketplace", "test")
                .unwrap();
        }
        let mut mk = Maker {
            app: &app,
            meta: models_of(&app, &admin),
            pkg_models: BTreeSet::new(),
            pkg_user: admin.clone(),
            admin: admin.clone(),
            made: HashMap::new(),
        };
        for (model, action, from, group) in [
            ("accounting_move", "post", "draft", "accounting.approver"),
            ("accounting_period", "close", "open", "accounting.approver"),
            (
                "accounting_fiscal_year",
                "close",
                "open",
                "accounting.approver",
            ),
            ("pos_order", "refund", "paid", "pos.manager"),
            ("pos_session", "close", "open", "pos.manager"),
            ("ecommerce_order", "refund", "paid", "ecommerce.manager"),
            (
                "email_marketing_campaign",
                "send",
                "scheduled",
                "email_marketing.manager",
            ),
        ] {
            let prefix = group.split_once('.').unwrap().0;
            let user_group = format!("{prefix}.user");
            let plain = Principal::user("u", &[user_group.as_str()]);
            let rec = mk.fresh(model, from);
            let cmd = format!("{model}.{action}");
            let denied = call(&app, &plain, &cmd, by_id(&rec));
            assert!(
                matches!(denied, Err(CallError::Forbidden(_))),
                "{cmd} must need {group}: {denied:?}"
            );
            assert_eq!(
                state_of(&app, model, &rec),
                from,
                "{cmd} refused, state unchanged"
            );
            let allowed = Principal::user("a", &[group]);
            call(&app, &allowed, &cmd, by_id(&rec)).unwrap_or_else(|e| panic!("{cmd}: {e:?}"));
        }
    }

    /// A dashboard and its widgets round-trip with the documented schema, and a widget kind or
    /// aggregate outside the documented lists is refused.
    #[test]
    fn dashboard_widgets_follow_the_documented_schema() {
        let app = App::new();
        let admin = Principal::user("root", &[ADMIN_GROUP]);
        app.installer
            .install(&Bundled.fetch("dashboards").unwrap(), "marketplace", "test")
            .unwrap();
        let board = call(
            &app,
            &admin,
            "dashboards_dashboard.create",
            Value::map([
                ("title", Value::text("Sales overview")),
                ("owner", Value::text("Ada")),
            ]),
        )
        .unwrap();
        let widget = |kind: &str, aggregate: &str| {
            call(
                &app,
                &admin,
                "dashboards_widget.create",
                Value::map([
                    ("dashboard_id", Value::Text(id_of(&board))),
                    ("title", Value::text("Paid orders")),
                    ("kind", Value::text(kind)),
                    ("model_name", Value::text("sale_order")),
                    ("measure_field", Value::text("total")),
                    ("aggregate", Value::text(aggregate)),
                    ("group_by", Value::text("state")),
                    ("filter", Value::text("{\"state\":\"paid\"}")),
                    ("position", Value::Int(1)),
                ]),
            )
        };
        for kind in ["kpi", "bar", "line", "pie", "table"] {
            widget(kind, "sum").unwrap_or_else(|e| panic!("{kind}: {e:?}"));
        }
        assert!(widget("kpi", "count").is_ok());
        assert!(widget("gauge", "sum").is_err());
        assert!(widget("kpi", "average").is_err());
    }

    /// The tutorial module of docs/MODULES.md installs, upgrades and fails exactly as the
    /// document says. Keep the error strings here in step with the table in that document.
    #[test]
    fn the_tutorial_module_and_its_documented_errors_are_real() {
        const ASSETS: &str = include_str!("../../../docs/modules/assets.toml");
        let app = App::new();
        let info = app.installer.install(ASSETS, "upload", "test").unwrap();
        assert_eq!(info.models, ["assets_category", "assets_asset"]);
        assert_eq!(info.groups, ["assets.manager", "assets.user"]);
        let again = app.installer.install(ASSETS, "upload", "test").unwrap_err();
        assert!(again.contains("is already installed"), "{again}");
        let v2 = ASSETS.replace("version = \"0.1.0\"", "version = \"0.2.0\"");
        assert_eq!(
            app.installer
                .install(&v2, "upload", "test")
                .unwrap()
                .version,
            "0.2.0"
        );
        let older = ASSETS.replace("version = \"0.1.0\"", "version = \"0.1.5\"");
        assert!(app.installer.install(&older, "upload", "test").is_err());

        // The purchase price needs assets.manager; the holder is personal data.
        let staff = Principal::user("s", &["assets.user"]);
        let names: Vec<_> = models_of(&app, &staff)["assets_asset"]
            .fields
            .iter()
            .map(|f| f.name)
            .collect();
        assert!(names.contains(&"tag") && !names.contains(&"purchase_price"));

        // Parse and gate errors quoted in the manifest reference.
        let parse = |t: &str| Manifest::parse(t).and_then(|m| m.to_module().map(|_| ()));
        for (from, to, want) in [
            ("id = \"assets\"", "id = \"Assets\"", "module id"),
            (
                "version = \"0.1.0\"",
                "version = \"1.0\"",
                "version must look like 1.2.3",
            ),
            (
                "name = \"assets_category\"",
                "name = \"category\"",
                "must be \"assets\" or start with \"assets_\"",
            ),
            ("assets.manager", "managers", "must look like module.role"),
            (
                "class = \"internal\"",
                "class = \"bogus\"",
                "unknown data class",
            ),
            (
                "class = \"internal\"\n  personal_data = true",
                "class = \"none\"\n  personal_data = true",
                "personal data cannot be classed `none`",
            ),
            (
                "to = \"assets_category\"",
                "to = \"assets_ghost\"",
                "which this package does not declare",
            ),
            ("kind = \"money\"", "kind = \"float\"", "unknown kind"),
            (
                "kind = \"money\"",
                "kind = \"money\"\n  options = [\"a\"]",
                "options are only valid",
            ),
            (
                "to = \"assigned\"",
                "to = \"lost\"",
                "is not one of the options of state",
            ),
            (
                "field = \"state\"\n  from = \"in_stock\"\n  to = \"assigned\"",
                "field = \"status\"\n  from = \"in_stock\"\n  to = \"assigned\"",
                "acts on undeclared field",
            ),
            ("name = \"assign\"", "name = \"create\"", "bad action name"),
            (
                "description = \"Category name\"",
                "description = \"\"",
                "field has no description",
            ),
            (
                "author = \"Example Ltd\"",
                "author = \"x\"\nnetwork_destinations = [\"a.example\"]",
                "may not declare network destinations",
            ),
            (
                "author = \"Example Ltd\"",
                "author = \"x\"\nbogus = 1",
                "invalid obop.toml",
            ),
            (
                "name = \"tag\"",
                "name = \"id\"",
                "bad or repeated field name",
            ),
        ] {
            assert!(ASSETS.contains(from), "fixture lost {from:?}");
            let e = parse(&ASSETS.replacen(from, to, 1)).unwrap_err();
            assert!(e.contains(want), "{from:?} -> {to:?}: {e}");
        }

        // Install-time errors.
        let app = App::new();
        let e = app
            .installer
            .install(
                &ASSETS.replace("base = \"^0.2\"", "ghost = \"*\""),
                "upload",
                "t",
            )
            .unwrap_err();
        assert!(
            e.contains("needs ghost *, which is not installed and not offered by the marketplace"),
            "{e}"
        );
        let e = app
            .installer
            .install(
                &ASSETS.replace("to = \"party\"", "to = \"ghost_model\""),
                "upload",
                "t",
            )
            .unwrap_err();
        assert!(
            e.contains("refers to model ghost_model, which no dependency of assets provides"),
            "{e}"
        );
        let e = app
            .installer
            .install(
                &ASSETS.replace("id = \"assets\"", "id = \"base\""),
                "upload",
                "t",
            )
            .unwrap_err();
        assert!(!e.is_empty());
    }

    const LANGS: [&str; 6] = ["de", "fr", "pl", "fi", "sv", "es"];

    fn labels_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/labels")
    }

    /// D52 completeness: every bundled package and every built-in module has every name,
    /// description, field label, option and action translated in all six languages. A value
    /// identical to the English source fails unless allow-listed (`packages/labels/identical.json`
    /// for acronyms and international words, `packages/labels/<lang>/identical.json` per language).
    #[test]
    fn every_bundled_package_and_builtin_module_is_translated_in_all_six_languages() {
        let app = App::new();
        let mut modules: Vec<obop_registry::Module> = app.modules.clone();
        for m in all_manifests() {
            modules.push(m.to_module().unwrap());
        }
        assert!(modules.len() >= 29, "{}", modules.len());
        let mut problems = Vec::new();
        for module in &modules {
            for lang in LANGS {
                let allow = obop_pkg::files::allow_list(&labels_dir(), lang);
                for p in
                    obop_registry::template::incomplete(module, &module.translations, lang, &allow)
                {
                    problems.push(format!("{}: {p}", module.id));
                }
            }
        }
        assert!(
            problems.is_empty(),
            "{} untranslated texts, first: {:#?}",
            problems.len(),
            &problems[..problems.len().min(25)]
        );
    }

    /// The label files, status files and allow-lists form one consistent set: a file per module
    /// and language, a status entry for each, no file for a module that does not exist, and a
    /// bundled package that stays under the manifest size limit with all its labels.
    #[test]
    fn label_files_and_status_files_cover_exactly_the_shipped_modules() {
        let app = App::new();
        let mut ids: Vec<String> = app.modules.iter().map(|m| m.id.clone()).collect();
        ids.extend(all_manifests().into_iter().map(|m| m.module.id));
        ids.sort();
        for lang in LANGS {
            let dir = labels_dir().join(lang);
            let mut files: Vec<String> = std::fs::read_dir(&dir)
                .unwrap()
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let n = e.file_name().to_string_lossy().to_string();
                    n.strip_suffix(".toml").map(String::from)
                })
                .collect();
            files.sort();
            assert_eq!(files, ids, "{lang}: one labels file per module");
            let status: BTreeMap<String, String> =
                serde_json::from_str(&std::fs::read_to_string(dir.join("status.json")).unwrap())
                    .unwrap();
            assert_eq!(status.keys().cloned().collect::<Vec<_>>(), ids, "{lang}");
            assert!(
                status
                    .values()
                    .all(|v| v == "machine-drafted" || v == "reviewed"),
                "{lang}: {status:?}"
            );
            let allow: Vec<String> =
                serde_json::from_str(&std::fs::read_to_string(dir.join("identical.json")).unwrap())
                    .unwrap();
            let mut sorted = allow.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(allow, sorted, "{lang}: identical.json is sorted and unique");
        }
        for l in Bundled.list().unwrap() {
            let text = Bundled.fetch(&l.id).unwrap();
            assert!(
                text.len() <= obop_pkg::manifest::MAX_BYTES,
                "{} with all its labels is {} bytes",
                l.id,
                text.len()
            );
        }
    }

    /// Every language file validates on its own against its package, the way a translator runs
    /// `obop modules check --complete packages/labels/<lang>/<id>.toml`.
    #[test]
    fn every_label_file_passes_the_check_command() {
        let root = labels_dir();
        let mut files = Vec::new();
        for lang in LANGS {
            for e in std::fs::read_dir(root.join(lang)).unwrap() {
                let p = e.unwrap().path();
                if p.extension().is_some_and(|x| x == "toml") {
                    files.push(p.to_string_lossy().to_string());
                }
            }
        }
        assert_eq!(files.len(), 6 * 29);
        let refs: Vec<&str> = files.iter().map(String::as_str).collect();
        assert!(modules_check_files(&refs, true).is_ok());
    }

    #[test]
    fn label_checks_report_missing_wrong_and_english_entries() {
        let dir = std::env::temp_dir().join(format!("obop-labels-{}", std::process::id()));
        let ldir = dir.join("labels").join("de");
        std::fs::create_dir_all(&ldir).unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        std::fs::copy(root.join("packages/fleet.toml"), dir.join("fleet.toml")).unwrap();
        let file = ldir.join("fleet.toml");
        let path = file.to_string_lossy().to_string();
        let builtin = App::new().modules;
        let check = |text: &str, complete: bool| {
            std::fs::write(&file, text).unwrap();
            match obop_pkg::files::read_source(&file).unwrap() {
                obop_pkg::files::Source::Labels {
                    stem,
                    text,
                    manifest,
                    allow,
                } => obop_pkg::files::check_labels_source(
                    &stem,
                    &text,
                    manifest.as_deref(),
                    &allow,
                    complete,
                    &builtin,
                ),
                _ => panic!("not a labels file"),
            }
        };
        // A partial file is fine until completeness is asked for.
        let partial = "[labels.de.module]\nname = \"Fuhrpark\"\n";
        assert!(check(partial, false).is_empty());
        let incomplete = check(partial, true);
        assert!(
            incomplete
                .iter()
                .any(|e| e.contains("fleet_vehicle.plate.label (de): missing"))
        );
        // A typo in a key is an error with the path.
        let typo = "[labels.de.model.fleet_vehicle.field.plat]\nlabel = \"Kennzeichen\"\n";
        assert!(check(typo, false)[0].contains("no field \"plat\""));
        // A value left in English is reported once complete.
        let english = obop_pkg::files::template_for("fleet", "de", &builtin).unwrap();
        let e = check(&english, true);
        assert!(
            e.iter().any(|x| x.contains("identical to English")),
            "{e:?}"
        );
        // The shared allow-list is read from labels/identical.json next to the language folders.
        std::fs::write(dir.join("labels").join("identical.json"), "[\"Fleet\"]").unwrap();
        let e = check(&english, true);
        assert!(!e.iter().any(|x| x.starts_with("module.name")), "{e:?}");
        // Whole-file check through the CLI entry point reports a bad file as failure.
        std::fs::write(&file, typo).unwrap();
        assert!(modules_check_files(&[&path], false).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
