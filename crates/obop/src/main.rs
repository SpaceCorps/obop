mod app;

use app::{App, DevAuth};
use obop_auth::Auth;
use obop_commands::{Authenticator, Principal, Surface, Value, Vault, names};
use std::io::BufRead;
use std::process::ExitCode;
use std::sync::Arc;

const USAGE: &str = "usage: obop <command>
  --version
  modules check                                 run the install gate over linked modules
  modules list | install FILE | uninstall ID    manage packages in the database (operator tool)
  modules market                                list the marketplace; install with `modules install-from ID`
  admin create USER [--groups a,b | --all-groups]
                                                create a user; the password is read from stdin
  agent-token USER --scopes a,b [--label TEXT]  issue a scoped agent token (shown once)
  platform serve [--addr A] [--static DIR]      the control plane: sign up and get your own instance
      --provisioner local|sliplane|fake         (default local: child processes on this machine)
      --instance-static DIR                     backoffice build served by local instances
      sliplane needs SLIPLANE_ACCOUNT, OBOP_SLIPLANE_PROJECT, OBOP_SLIPLANE_SERVER, OBOP_CELL_ADMIN_URL
      and OBOP_IMAGE or OBOP_REPO (+ OBOP_BRANCH); optional OBOP_MARKETPLACE_URL and
      OBOP_CELL_INSTANCE_HOST (host:port instances use for the cell, e.g. obop-postgres.internal:5432),
      OBOP_CELL_INSTANCE_HOST (host:port instances use for the cell, e.g. obop-postgres.internal:5432)
  serve [--addr 127.0.0.1:8080] [--static DIR]  REST API and the backoffice
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
    let (flags, args): (Vec<&str>, Vec<&str>) = args
        .into_iter()
        .partition(|a| matches!(*a, "--demo" | "--dev-auth" | "--all-groups"));
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
    };
    let result = match args.as_slice() {
        [] | ["--version"] => {
            println!("{} {}", obop_types::NAME, env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        ["modules", "check"] => modules_check(),
        ["modules", "list"] => modules_list(&opts),
        ["modules", "market"] => modules_market(&opts),
        ["modules", "install", file] => modules_install(&opts, Some(file), None),
        ["modules", "install-from", id] => modules_install(&opts, None, Some(id)),
        ["modules", "uninstall", id] => modules_uninstall(&opts, id),
        ["admin", "create", user] => admin_create(&opts, user, groups, has("--all-groups")),
        ["agent-token", user] => agent_token(&opts, user, scopes, label),
        ["serve"] => serve(&opts, addr, static_dir),
        ["platform", "serve"] => {
            platform_serve(&opts, addr, static_dir, provisioner, instance_static)
        }
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

fn modules_list(o: &Opts) -> Exit {
    let app = open_app(&Opts {
        demo: false,
        ..clone_opts(o)
    })?;
    for p in app.installer.list() {
        println!(
            "{:<14} {:<10} {:<12} {}",
            p.id, p.version, p.source, p.description
        );
    }
    Ok(())
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
        (Some(f), _) => app.installer.install(
            &std::fs::read_to_string(f).map_err(|e| fail(&format!("{f}: {e}")))?,
            "upload",
            by,
        ),
        (_, Some(id)) => app.installer.install_from_marketplace(id, by),
        _ => unreachable!(),
    };
    let p = r.map_err(|e| fail(&e))?;
    println!(
        "installed {} {} (models: {}; groups: {})",
        p.id,
        p.version,
        p.models.join(", "),
        p.groups.join(", ")
    );
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

fn clone_opts(o: &Opts) -> Opts {
    Opts {
        db: o.db.clone(),
        token: o.token.clone(),
        demo: o.demo,
        dev_auth: o.dev_auth,
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
    let router = obop_http::router(state, static_dir);
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
    let prov =
        make_provisioner(provisioner.unwrap_or("local"), instance_static).map_err(|e| fail(&e))?;
    let platform =
        obop_platform::Platform::new(Arc::new(Auth::new(backend.clone())), backend, prov);
    let router = obop_http::platform_router(Arc::new(obop_platform::Api(platform)), static_dir);
    eprintln!("obop platform listening on http://{addr}");
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(obop_http::serve(addr, router))
        .map_err(|e| fail(&format!("error: {e}")))
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
        let mut v = Vault::new();
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
                .position(|m| m.module.depends.iter().all(|d| have.contains(d)))
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

        /// A fresh record of `model` whose state starts as `state`.
        fn fresh(&mut self, model: &str, state: &str) -> String {
            let mut open = vec![model.to_string()];
            let input = self.input(model, Some(state), &mut open);
            self.create(model, input)
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
                .any(|m| m.module.depends.contains(&"projects".to_string()))
        );
        assert!(call(&app, &admin, "module.uninstall", by_id("projects")).is_err());
        assert!(app.dispatcher.has_model("projects_task"));
        // The same holds for the packages that refer to people and to other apps.
        for (dep, model) in [("hr", "hr_employee"), ("accounting", "accounting_move")] {
            if manifests
                .iter()
                .any(|m| m.module.depends.contains(&dep.to_string()))
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
        assert!(again.contains("is not an upgrade"), "{again}");
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
                &ASSETS.replace("depends = [\"base\"]", "depends = [\"ghost\"]"),
                "upload",
                "t",
            )
            .unwrap_err();
        assert!(
            e.contains("needs module ghost, which is not installed"),
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
            e.contains("refers to model ghost_model, which is not installed"),
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
}
