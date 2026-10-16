mod app;

use app::{App, dev_principal};
use obop_commands::{Surface, Value, Vault, names};
use std::process::ExitCode;
use std::sync::Arc;

const USAGE: &str = "usage: obop <command>
  --version
  modules check                                  run the install gate over linked modules
  commands [--as TOKEN]                          list commands visible to a dev principal
  call [--as TOKEN] <model> <command> [k=v...]   run one command through the dispatcher
  approve [--as TOKEN] <plan_id>                 (only useful inside `serve`; plans are per process)
  serve [--addr 127.0.0.1:8080] [--static DIR]   REST API and the backoffice (localhost only for now)
  mcp [--as TOKEN]                               MCP server on stdio
database (serve, mcp, call): --db URL or OBOP_DATABASE_URL
  sqlite://obop.db (default, local)   postgres://user:pass@host/db (deployments)   memory:
  fake demo data is added to an empty SQLite database, or to any database with --demo
dev tokens: dev-admin (default), dev-agent, dev-sales";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let (token, args) = take_opt(&args, "--as");
    let token = token.unwrap_or("dev-admin");
    let (db, args) = take_opt(&args, "--db");
    let demo = args.contains(&"--demo");
    let args: Vec<&str> = args.into_iter().filter(|a| *a != "--demo").collect();
    let db = db
        .map(String::from)
        .or_else(|| std::env::var("OBOP_DATABASE_URL").ok());
    match args.as_slice() {
        [] | ["--version"] => {
            println!("{} {}", obop_types::NAME, env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        ["modules", "check"] => modules_check(),
        ["commands"] => commands(token),
        ["call", model, command, kv @ ..] => call(token, model, command, kv, db, demo),
        ["serve", rest @ ..] => serve(rest, db, demo),
        ["mcp"] => mcp(token, db, demo),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
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

/// The D41 install gate over every linked first-party module.
fn modules_check() -> ExitCode {
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
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn commands(token: &str) -> ExitCode {
    let Some(p) = dev_principal(token) else {
        return bad_token();
    };
    for c in App::new().dispatcher.catalog(Surface::Cli, &p) {
        let note = if c.needs_approval {
            "  [needs approval for agents]"
        } else {
            ""
        };
        println!("{:<24} {}{note}", c.id, c.description);
    }
    ExitCode::SUCCESS
}

/// Open the configured database; default is a local SQLite file. Fake demo data goes into an
/// empty SQLite database, or into any database when `--demo` is given.
fn open_app(db: Option<String>, demo: bool) -> Result<App, ExitCode> {
    let url = db.unwrap_or_else(|| "sqlite://obop.db".into());
    let store = obop_commands::Store::open(&url).map_err(|e| {
        eprintln!("cannot open database: {e}");
        ExitCode::FAILURE
    })?;
    eprintln!("database: {}", store.backend_kind());
    let seed = demo || store.backend_kind() == "sqlite";
    let app = App::with_store(store);
    if seed {
        app.seed_demo();
    }
    Ok(app)
}

fn bad_token() -> ExitCode {
    eprintln!("unknown dev token");
    ExitCode::from(2)
}

fn call(
    token: &str,
    model: &str,
    command: &str,
    kv: &[&str],
    db: Option<String>,
    demo: bool,
) -> ExitCode {
    let Some(p) = dev_principal(token) else {
        return bad_token();
    };
    let app = match open_app(db, demo) {
        Ok(a) => a,
        Err(c) => return c,
    };
    let Some(id) = names::from_cli(&[model, command]) else {
        return ExitCode::from(2);
    };
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
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e:?}");
            ExitCode::FAILURE
        }
    }
}

fn serve(args: &[&str], db: Option<String>, demo: bool) -> ExitCode {
    let (addr, rest) = take_opt(args, "--addr");
    let (static_dir, _) = take_opt(&rest, "--static");
    let addr = addr.unwrap_or("127.0.0.1:8080");
    let insecure = rest.contains(&"--insecure-dev-auth");
    // Authentication is three hard-coded dev tokens; never expose that beyond this machine
    // unless the operator says so explicitly (D33).
    if !is_loopback(addr) && !insecure {
        eprintln!(
            "refusing to bind {addr}: only dev tokens exist, so serve is localhost-only until real authentication lands (override: --insecure-dev-auth)"
        );
        return ExitCode::from(2);
    }
    let app = match open_app(db, demo) {
        Ok(a) => a,
        Err(c) => return c,
    };
    let state = obop_http::HttpState::new(Arc::new(app.dispatcher), Arc::new(dev_principal));
    let router = obop_http::router(state, static_dir);
    eprintln!("obop listening on http://{addr} (dev tokens: dev-admin, dev-agent, dev-sales)");
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    match rt.block_on(obop_http::serve(addr, router)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn is_loopback(addr: &str) -> bool {
    addr.rsplit_once(':')
        .is_some_and(|(host, _)| matches!(host, "127.0.0.1" | "localhost" | "[::1]"))
}

fn mcp(token: &str, db: Option<String>, demo: bool) -> ExitCode {
    let Some(p) = dev_principal(token) else {
        return bad_token();
    };
    let app = match open_app(db, demo) {
        Ok(a) => a,
        Err(c) => return c,
    };
    match obop_mcp::serve_stdio(&app.dispatcher, &p) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use serde_json::{Value as J, json};
    use tower::ServiceExt;

    fn router() -> axum::Router {
        let app = App::new();
        app.seed_demo();
        obop_http::router(
            obop_http::HttpState::new(Arc::new(app.dispatcher), Arc::new(dev_principal)),
            None,
        )
    }

    async fn post(r: &axum::Router, token: &str, path: &str, body: J) -> (u16, J) {
        let req = Request::post(path)
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let resp = r.clone().oneshot(req).await.unwrap();
        let status = resp.status().as_u16();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or(J::Null))
    }

    #[tokio::test]
    async fn rest_requires_a_token() {
        let (s, _) = post(&router(), "nope", "/api/party/list", json!({})).await;
        assert_eq!(s, 401);
    }

    #[tokio::test]
    async fn agent_over_rest_is_masked_and_approval_gated() {
        let r = router();
        let (s, body) = post(&r, "dev-agent", "/api/party/list", json!({})).await;
        assert_eq!(s, 200);
        let party = &body["value"][0];
        assert!(party["name"].as_str().unwrap().starts_with("NAME_"));
        assert!(party.get("tax_id").is_none());
        assert_eq!(body["denied"], json!(["party.tax_id"]));

        let id = party["id"].as_str().unwrap();
        let (s, body) = post(&r, "dev-agent", "/api/party/erase", json!({"id": id})).await;
        assert_eq!(s, 202);
        let plan = body["plan_id"].as_str().unwrap().to_string();
        // The agent cannot approve its own plan; the admin can.
        assert_eq!(
            post(&r, "dev-agent", &format!("/api/approve/{plan}"), json!({}))
                .await
                .0,
            404
        );
        assert_eq!(
            post(&r, "dev-admin", &format!("/api/approve/{plan}"), json!({}))
                .await
                .0,
            200
        );
    }

    #[tokio::test]
    async fn hidden_commands_look_unknown() {
        // dev-sales holds no invoicing group: same 404 as a command that does not exist.
        let (a, _) = post(&router(), "dev-sales", "/api/invoice/list", json!({})).await;
        let (b, _) = post(&router(), "dev-sales", "/api/nothing/list", json!({})).await;
        assert_eq!((a, b), (404, 404));
    }

    #[test]
    fn mcp_lists_and_calls_tools() {
        let app = App::new();
        app.seed_demo();
        let p = dev_principal("dev-agent").unwrap();
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

    #[test]
    fn serve_binds_only_loopback_by_default() {
        assert!(
            is_loopback("127.0.0.1:8080") && is_loopback("localhost:1") && is_loopback("[::1]:9")
        );
        assert!(!is_loopback("0.0.0.0:8080") && !is_loopback("10.0.0.5:80"));
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
