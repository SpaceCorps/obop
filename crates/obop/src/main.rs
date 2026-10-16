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
    let input = Value::Map(
        kv.iter()
            .filter_map(|p| p.split_once('='))
            .map(|(k, v)| (k.to_string(), Value::text(v)))
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
