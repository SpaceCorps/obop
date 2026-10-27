//! The error seam end to end (K16, D74): the store's refusal answers as one thing on every surface
//! of a real `App` whose store refuses on demand (`Flaky`, behind the request guard, as
//! PostgreSQL is). REST: the status, `Retry-After`, `X-Obop-Applied` and the body. MCP: the tool
//! error of the stdio server (the JSON-RPC 503 of `/mcp` is in `mcp_tests`). The command line:
//! the line and the exit status.
//!
//! `unknown` needs a connection that breaks while a change is in flight, which the store of these
//! tests can only simulate on a route of its own: its REST, JSON-RPC and OAuth shapes are in
//! `obop-http` (`ops::tests`); here are `none` and `partial`, both end to end.

use crate::app::{App, DevAuth};
use axum::{
    body::Body,
    http::{HeaderMap, Request},
};
use http_body_util::BodyExt;
use obop_commands::{CallError, Value, errors};
use obop_http::mcp::McpConfig;
use obop_storage::testing::Flaky;
use serde_json::{Value as J, json};
use std::sync::Arc;
use tower::ServiceExt;

fn flaky_app() -> (Arc<Flaky>, App) {
    let (flaky, backend) = Flaky::memory();
    let app = App::with_store(obop_commands::Store::over(backend));
    app.seed_demo();
    (flaky, app)
}

fn router(app: &App) -> axum::Router {
    router_with(app, Arc::new(DevAuth))
}

fn router_with(app: &App, auth: Arc<dyn obop_commands::Authenticator>) -> axum::Router {
    obop_http::router(
        obop_http::HttpState::with_mcp(app.dispatcher.clone(), auth, McpConfig::default()),
        None,
    )
}

/// Every token is the administrator of the instance, who can also use the module commands.
struct Root(obop_commands::Principal);

impl obop_commands::Authenticator for Root {
    fn resolve(&self, _: &str) -> Option<obop_commands::Principal> {
        Some(self.0.clone())
    }
    fn login(&self, _: &str, _: &str) -> Result<String, String> {
        Err("no sign-in here".into())
    }
    fn issue_agent_token(
        &self,
        _: &obop_commands::Principal,
        _: &[String],
        _: &str,
    ) -> Result<String, String> {
        Err("no agent tokens here".into())
    }
}

fn root() -> obop_commands::Principal {
    let mut p = admin();
    p.groups.insert(obop_commands::ADMIN_GROUP.to_string());
    p
}

async fn post(r: &axum::Router, path: &str, body: J) -> (u16, HeaderMap, J) {
    let req = Request::post(path)
        .header("host", "inst.test")
        .header("authorization", "Bearer dev-admin")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = r.clone().oneshot(req).await.unwrap();
    let (status, headers) = (resp.status().as_u16(), resp.headers().clone());
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(J::Null);
    (status, headers, json)
}

fn header<'a>(h: &'a HeaderMap, name: &str) -> &'a str {
    h.get(name).and_then(|v| v.to_str().ok()).unwrap_or("")
}

fn admin() -> obop_commands::Principal {
    crate::app::dev_principal("dev-admin").unwrap()
}

#[tokio::test]
async fn a_refused_rest_call_is_a_503_that_says_nothing_was_changed() {
    let (flaky, app) = flaky_app();
    let r = router(&app);
    let (s, _, _) = post(&r, "/api/lead/list", json!({})).await;
    assert_eq!(s, 200, "healthy: untouched");
    let before = post(&r, "/api/lead/list", json!({})).await.2["value"]
        .as_array()
        .map(Vec::len);

    // A read and a write that meet a full pool answer the same way, in the words of the seam.
    for (path, body) in [
        ("/api/lead/list", json!({})),
        ("/api/lead/create", json!({"title": "Refused lead"})),
    ] {
        flaky.refuse_next(1);
        let (s, h, b) = post(&r, path, body).await;
        assert_eq!(s, 503, "{path}: {b}");
        assert_eq!(header(&h, "retry-after"), "1", "{path}");
        assert_eq!(header(&h, "x-obop-applied"), "none", "{path}");
        assert_eq!(
            b,
            errors::storage_fault(
                obop_storage::fault::Fault::Busy,
                errors::Applied::None,
                None
            )
            .body,
            "{path}"
        );
    }
    flaky.allow_models();
    let after = post(&r, "/api/lead/list", json!({})).await.2["value"]
        .as_array()
        .map(Vec::len);
    assert!(before.is_some_and(|n| n > 0), "the demo data is listed");
    assert_eq!(before, after, "nothing was changed");
}

/// A refusal at every position of a change that takes two atomic batches: whichever the request
/// met, the word of the 503 is true. At least one position leaves nothing changed (`none`) and at
/// least one leaves the first batch committed (`partial`); never a 4xx, never a bare error.
#[tokio::test]
async fn a_refusal_after_a_commit_is_a_partial_503_and_the_word_is_true() {
    let (flaky, app) = flaky_app();
    let r = router(&app);
    let (mut none, mut partial) = (0, 0);
    for k in 1..=10 {
        let (s, _, lead) = post(
            &r,
            "/api/lead/create",
            json!({"title": format!("Lead {k}")}),
        )
        .await;
        assert_eq!(s, 200, "{lead}");
        let id = lead["value"]["id"].as_str().unwrap().to_string();
        // The implicit action commits, the update that follows meets a refusal.
        flaky.refuse_nth_apply(k - 1);
        let (s, h, b) = post(
            &r,
            "/api/lead/update",
            json!({"id": id, "stage": "won", "contact_name": "Zed Changed"}),
        )
        .await;
        flaky.allow_models();
        let now = post(&r, "/api/lead/get", json!({"id": id})).await.2;
        let stage = now["value"]["stage"].as_str().unwrap_or("").to_string();
        if s == 200 {
            assert_eq!(stage, "won", "k={k}: the change is there once");
            continue;
        }
        assert_eq!(s, 503, "k={k}: {b}");
        assert!(!header(&h, "retry-after").is_empty(), "k={k}");
        let word = header(&h, "x-obop-applied").to_string();
        assert_eq!(b["applied"], word.as_str(), "k={k}: header and body agree");
        assert_eq!(b["code"], "storage_busy", "k={k}: {b}");
        assert_eq!(b["error"], "storage_busy", "k={k}: {b}");
        match word.as_str() {
            "none" => {
                none += 1;
                assert_ne!(stage, "won", "k={k}: said nothing was changed: {b}");
            }
            "partial" => {
                partial += 1;
                assert_eq!(stage, "won", "k={k}: said part was applied: {b}");
                assert!(
                    b["hint"]
                        .as_str()
                        .unwrap()
                        .contains("Do not repeat it blindly"),
                    "{b}"
                );
            }
            other => panic!("k={k}: applied {other}: {b}"),
        }
    }
    assert!(
        none >= 1 && partial >= 1,
        "the sweep saw {none} none and {partial} partial"
    );
}

/// One tool call on the stdio server (`obop mcp`), the way a client sends it.
fn stdio_call(app: &App, p: &obop_commands::Principal, name: &str, args: J) -> J {
    let mut session = obop_mcp::Session::new();
    let line = json!({"jsonrpc":"2.0","id":7,"method":"tools/call",
        "params":{"name": name, "arguments": args}})
    .to_string();
    let out = obop_mcp::handle(&app.dispatcher, p, &mut session, &line).unwrap();
    serde_json::from_str::<J>(&out).unwrap()
}

fn input(pairs: &[(&str, &str)]) -> Value {
    Value::Map(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), Value::text(v)))
            .collect(),
    )
}

fn created_id(app: &App, p: &obop_commands::Principal, title: &str) -> String {
    let (v, _) = crate::dispatch_call(app, p, "lead.create", &input(&[("title", title)]))
        .unwrap_or_else(|(line, _)| panic!("{line}"));
    v["id"].as_str().unwrap().to_string()
}

fn stage_of(app: &App, p: &obop_commands::Principal, id: &str) -> String {
    let (v, _) = crate::dispatch_call(app, p, "lead.get", &input(&[("id", id)]))
        .unwrap_or_else(|(line, _)| panic!("{line}"));
    v["stage"].as_str().unwrap_or("").to_string()
}

#[test]
fn a_refused_tool_call_of_the_stdio_server_is_a_tool_error_with_the_wait_and_applied() {
    let (flaky, app) = flaky_app();
    let p = admin();
    let healthy = stdio_call(&app, &p, "lead__list", json!({}));
    assert_eq!(healthy["result"]["isError"], false, "{healthy}");

    flaky.refuse_next(1);
    let refused = stdio_call(&app, &p, "lead__list", json!({}));
    flaky.allow_models();
    // A tool error the model can read, not a JSON-RPC error and not a success.
    assert!(refused.get("error").is_none(), "{refused}");
    assert_eq!(refused["result"]["isError"], true, "{refused}");
    let b = &refused["result"]["structuredContent"];
    assert_eq!(b["code"], "storage_busy", "{b}");
    assert_eq!(b["error"], "storage_busy");
    assert_eq!(b["applied"], "none");
    assert_eq!(b["retry_after"], 1);
    assert!(
        b["hint"]
            .as_str()
            .is_some_and(|h| h.contains("Nothing was changed"))
    );
    // The next line is a request of its own: nothing of the refusal is left behind.
    let again = stdio_call(&app, &p, "lead__list", json!({}));
    assert_eq!(again["result"]["isError"], false, "{again}");
}

#[test]
fn a_refused_command_line_call_prints_the_word_and_exits_with_75() {
    let (flaky, app) = flaky_app();
    let p = admin();
    let by_id = input(&[("id", "nope")]);

    // An error of the command itself keeps the documented line and status 1.
    let e = crate::dispatch_call(&app, &p, "lead.get", &by_id).unwrap_err();
    assert_eq!(
        e,
        ("error: Handler(\"no lead nope\")".to_string(), 1),
        "an error of the call itself"
    );

    // The store's refusal is the same word as everywhere else, with what became of the changes,
    // and a status a script can tell from a failed command.
    flaky.refuse_next(1);
    let (line, status) = crate::dispatch_call(&app, &p, "lead.get", &by_id).unwrap_err();
    flaky.allow_models();
    assert_eq!(status, 75);
    assert!(
        line.starts_with("error: storage_busy: ")
            && line.ends_with("(nothing was changed; try again in 1 s)"),
        "{line}"
    );
    // The next call is a request of its own: the fault does not stay on the thread.
    let e = crate::dispatch_call(&app, &p, "lead.get", &by_id).unwrap_err();
    assert_eq!(e.1, 1, "{e:?}");
}

/// A refusal after the first of two batches of one call (the implicit action of a governed field
/// commits, then the update meets the refusal): on the command line and on the stdio server the
/// answer says `partial` exactly when the stage was changed, as the REST sweep does.
#[test]
fn a_refusal_after_a_commit_says_partial_on_the_command_line_and_the_stdio_server() {
    let (flaky, app) = flaky_app();
    let p = admin();
    let (mut cli, mut stdio) = ([0, 0], [0, 0]);
    for k in 1..=10 {
        // The command line.
        let id = created_id(&app, &p, &format!("CLI {k}"));
        flaky.refuse_nth_apply(k - 1);
        let r = crate::dispatch_call(
            &app,
            &p,
            "lead.update",
            &input(&[("id", &id), ("stage", "won"), ("contact_name", "Zed")]),
        );
        flaky.allow_models();
        let stage = stage_of(&app, &p, &id);
        match r {
            Ok(_) => assert_eq!(stage, "won", "k={k}"),
            Err((line, status)) => {
                assert_eq!(status, 75, "k={k}: {line}");
                if line.contains("nothing was changed") {
                    cli[0] += 1;
                    assert_ne!(stage, "won", "k={k}: said nothing was changed: {line}");
                } else {
                    cli[1] += 1;
                    assert!(
                        line.contains("part of it was applied: do not repeat it blindly"),
                        "k={k}: {line}"
                    );
                    assert_eq!(stage, "won", "k={k}: said part was applied: {line}");
                }
            }
        }

        // The stdio server.
        let id = created_id(&app, &p, &format!("MCP {k}"));
        flaky.refuse_nth_apply(k - 1);
        let reply = stdio_call(
            &app,
            &p,
            "lead__update",
            json!({"id": id, "stage": "won", "contact_name": "Zed"}),
        );
        flaky.allow_models();
        let stage = stage_of(&app, &p, &id);
        let result = &reply["result"];
        if result["isError"] == false {
            assert_eq!(stage, "won", "k={k}");
            continue;
        }
        let b = &result["structuredContent"];
        assert_eq!(b["code"], "storage_busy", "k={k}: {reply}");
        assert_eq!(b["retry_after"], 1, "k={k}");
        match b["applied"].as_str().unwrap() {
            "none" => {
                stdio[0] += 1;
                assert_ne!(stage, "won", "k={k}: said nothing was changed: {reply}");
            }
            "partial" => {
                stdio[1] += 1;
                assert_eq!(stage, "won", "k={k}: said part was applied: {reply}");
                assert!(
                    b["hint"]
                        .as_str()
                        .unwrap()
                        .contains("Do not repeat it blindly")
                );
            }
            other => panic!("k={k}: applied {other}: {reply}"),
        }
    }
    assert!(
        cli[0] >= 1 && cli[1] >= 1,
        "command line: {} none and {} partial",
        cli[0],
        cli[1]
    );
    assert!(
        stdio[0] >= 1 && stdio[1] >= 1,
        "stdio: {} none and {} partial",
        stdio[0],
        stdio[1]
    );
}

/// The engine wraps the text of a refusal (a row whose reference could not be looked up is
/// "invalid", and the command says which rows): the answer is the store's all the same, because the
/// request says so and not the words. A refusal at every position of a bulk create.
#[test]
fn a_wrapped_refusal_is_the_stores_on_the_command_line_and_the_stdio_server() {
    let (flaky, app) = flaky_app();
    let p = admin();
    let party = crate::dispatch_call(&app, &p, "party.create", &input(&[("name", "Buyer")]))
        .unwrap_or_else(|(line, _)| panic!("{line}"))
        .0["id"]
        .as_str()
        .unwrap()
        .to_string();
    let rows = json!([
        {"party_id": party, "currency": "EUR"},
        {"party_id": party, "currency": "EUR"}
    ]);
    let bulk = Value::from_json(&json!({"rows": rows}));
    // (A bulk create is rate limited, twelve a minute: the command line walks every position, the
    // stdio server meets the first refused one and the first wrapped one.)
    let (mut refused, mut wrapped) = (0, 0);
    let mut stdio_at = Vec::new();
    for k in 0..8 {
        let before = flaky.refused();
        flaky.refuse_call_after(k);
        let r = crate::dispatch_call(&app, &p, "sale_order.bulk_create", &bulk);
        let seen_refusal = flaky.refused() - before;
        flaky.allow_models();
        if seen_refusal == 0 {
            // Past the last call of the command: nothing was refused any more.
            assert!(r.is_ok(), "k={k}: {r:?}");
            break;
        }
        refused += 1;
        match r {
            // The command carried on after a refusal that cost it only something optional.
            Ok(_) => {}
            Err((line, status)) => {
                assert_eq!(
                    status, 75,
                    "k={k}: a refusal of the store is not an input error"
                );
                assert!(line.starts_with("error: storage_"), "k={k}: {line}");
                let is_wrapped = line.contains("row(s) are invalid");
                if is_wrapped {
                    wrapped += 1;
                }
                if refused == 1 || (is_wrapped && wrapped == 1) {
                    stdio_at.push(k);
                }
            }
        }
    }
    assert!(refused >= 2, "the sweep refused {refused} positions");
    assert!(
        wrapped >= 1,
        "no refusal came wrapped in the text of the rows"
    );
    for k in stdio_at {
        flaky.refuse_call_after(k);
        let reply = stdio_call(&app, &p, "sale_order__bulk_create", json!({"rows": rows}));
        flaky.allow_models();
        let b = &reply["result"]["structuredContent"];
        assert_eq!(reply["result"]["isError"], true, "k={k}: {reply}");
        assert_eq!(b["code"], "storage_busy", "k={k}: {reply}");
        assert!(b.get("retry_after").is_some(), "k={k}");
    }
}

/// Text that begins with a code word is the caller's own text when no fault was noted: a record
/// whose name or field value does, an id that does, a module id that does, is answered as before
/// on REST, the command line and the stdio server. Never a 503, never a row of the table.
#[tokio::test]
async fn input_that_begins_with_a_code_word_cannot_forge_a_fault_or_a_row() {
    let (_flaky, app) = flaky_app();
    let r = router_with(&app, Arc::new(Root(root())));
    let p = root();
    for row in errors::TABLE {
        let w = row.code;
        let value = format!("{w}: x");

        // A record whose name and a field value begin with the word is an ordinary record.
        let (s, _, b) = post(&r, "/api/lead/create", json!({"title": value})).await;
        assert_eq!(s, 200, "{w}: {b}");
        let id = b["value"]["id"].as_str().unwrap().to_string();
        let (s, _, b) = post(&r, "/api/lead/get", json!({"id": id})).await;
        assert_eq!(
            (s, b["value"]["title"].as_str()),
            (200, Some(value.as_str()))
        );
        // An id that begins with it is an ordinary 400: no record of that name.
        let (s, h, b) = post(&r, "/api/lead/get", json!({"id": value})).await;
        assert_eq!(s, 400, "{w}: {b}");
        assert_eq!(b, json!({"error": format!("no lead {value}")}), "{w}");
        assert!(h.get("retry-after").is_none() && h.get("x-obop-applied").is_none());
        // A refusal that echoes the input first is set apart, and is a 400 too. The text of a
        // storage row is not read from text at all (a fault needs a fault noted), so it is left
        // as it is: the readers off a request scope still find the word of a genuine refusal.
        let (s, h, b) = post(&r, "/api/module/uninstall", json!({"id": value})).await;
        assert_eq!(s, 400, "{w}: {b}");
        assert!(b.get("code").is_none(), "{w}: {b}");
        let wanted = if row.origin == "D74" {
            format!("{value} is not installed")
        } else {
            format!("input refused: {value} is not installed")
        };
        assert_eq!(b["error"], wanted, "{w}");
        assert!(h.get("retry-after").is_none() && h.get("x-obop-applied").is_none());

        // The command line.
        let (line, status) =
            crate::dispatch_call(&app, &p, "lead.get", &input(&[("id", &value)])).unwrap_err();
        assert_eq!(
            (line.as_str(), status),
            (format!("error: Handler(\"no lead {value}\")").as_str(), 1),
            "{w}"
        );
        let (line, status) =
            crate::dispatch_call(&app, &p, "module.uninstall", &input(&[("id", &value)]))
                .unwrap_err();
        assert_eq!(status, 1, "{w}: {line}");
        if row.origin == "D74" {
            assert_eq!(
                line,
                format!("error: Handler(\"{value} is not installed\")"),
                "{w}"
            );
        } else {
            assert!(
                line.starts_with("error: Handler(\"input refused: "),
                "{w}: {line}"
            );
        }

        // The stdio server.
        let reply = stdio_call(&app, &p, "lead__get", json!({"id": value}));
        assert_eq!(reply["result"]["isError"], true, "{w}: {reply}");
        assert_eq!(
            reply["result"]["structuredContent"],
            json!({"error": format!("no lead {value}")}),
            "{w}"
        );
    }
}

#[test]
fn the_exit_status_and_line_of_every_failed_call_come_from_the_table() {
    let none = errors::Outcome::default();
    let line = |e: CallError| crate::call_failure_line(&e, &none);
    assert_eq!(
        line(CallError::Handler("name is required".into())),
        ("error: Handler(\"name is required\")".to_string(), 1)
    );
    assert_eq!(
        line(CallError::ApprovalRequired {
            plan_id: "plan-1".into()
        }),
        (
            "error: ApprovalRequired { plan_id: \"plan-1\" }".to_string(),
            1
        )
    );
    assert_eq!(
        line(CallError::PreconditionFailed { current: 8 }),
        (
            "error: precondition_failed: the record was changed since you read it (it is at version 8 now)"
                .to_string(),
            1
        )
    );
    // The store's words are text when the request met no fault.
    assert_eq!(
        line(CallError::Handler("storage_busy: a name".into())),
        ("error: Handler(\"storage_busy: a name\")".to_string(), 1)
    );
    for (fault, word, text, applied) in [
        (
            errors::Fault::Busy,
            "storage_busy",
            "every connection is in use",
            "nothing was changed",
        ),
        (
            errors::Fault::Timeout,
            "storage_timeout",
            "a statement ran into its time limit",
            "nothing was changed",
        ),
        (
            errors::Fault::Unavailable,
            "storage_unavailable",
            "the database cannot be reached",
            "a change in flight may or may not have been applied: read before you repeat a write",
        ),
    ] {
        let seen = errors::Outcome {
            fault: Some(fault),
            ..none
        };
        let (l, status) =
            crate::call_failure_line(&CallError::Handler(format!("{word}: {text}")), &seen);
        assert_eq!(status, 75, "{word}");
        assert!(
            l.starts_with(&format!("error: {word}: {text} ({applied}; try again in ")),
            "{l}"
        );
    }
}

/// The stdio server answers a batch line (revision 2025-03-26) call by call: the refusal met by
/// one call is not the answer of the independent calls that follow it in the same line.
#[test]
fn the_calls_of_a_batch_line_of_the_stdio_server_do_not_answer_for_one_another() {
    let (flaky, app) = flaky_app();
    let p = admin();
    let mut session = obop_mcp::Session::new();
    let init = json!({"jsonrpc":"2.0","id":1,"method":"initialize",
        "params":{"protocolVersion":"2025-03-26"}})
    .to_string();
    obop_mcp::handle(&app.dispatcher, &p, &mut session, &init).unwrap();
    let call = |id: i64, name: &str, args: J| {
        json!({"jsonrpc":"2.0","id":id,"method":"tools/call",
            "params":{"name": name, "arguments": args}})
    };
    // The first call meets a refusal; the second is a plain read, the third a valid write.
    flaky.refuse_next(1);
    let line = J::Array(vec![
        call(10, "lead__list", json!({})),
        call(11, "lead__list", json!({})),
        call(12, "lead__create", json!({"title": "Created in a batch"})),
    ])
    .to_string();
    let out = obop_mcp::handle(&app.dispatcher, &p, &mut session, &line).unwrap();
    flaky.allow_models();
    let replies: Vec<J> = serde_json::from_str(&out).unwrap();
    let by_id = |id: i64| replies.iter().find(|r| r["id"] == id).unwrap();
    assert_eq!(by_id(10)["result"]["isError"], true, "{out}");
    assert_eq!(
        by_id(10)["result"]["structuredContent"]["code"],
        "storage_busy"
    );
    assert_eq!(
        by_id(11)["result"]["isError"],
        false,
        "a read that succeeded is not the refusal of its neighbour: {out}"
    );
    assert_eq!(
        by_id(12)["result"]["isError"],
        false,
        "a valid write is attempted: {out}"
    );
}

/// The lookup of the token of `obop call`, `obop import` and `obop mcp` is part of the call: the
/// store's refusal is told from a token that is not valid (exit 75, the word), and a token that
/// is not valid stays exit 1.
#[test]
fn a_refused_token_lookup_is_not_a_token_that_is_not_valid() {
    use obop_commands::Authenticator;
    let (flaky, backend) = Flaky::memory();
    let app = App::with_store(obop_commands::Store::over(backend));
    let auth = obop_auth::Auth::with_test_params(app.store.backend());
    auth.create_user("ada", "correct horse battery", &app.all_groups())
        .unwrap();
    let token = auth.login("ada", "correct horse battery").unwrap();
    let opts = |token: &str| crate::Opts {
        db: Some("memory:".into()),
        token: Some(token.to_string()),
        demo: false,
        dev_auth: false,
        public_site: false,
        trust_proxy: false,
        admin_prefix: "/admin/".into(),
    };
    assert!(crate::principal(&app, &opts(&token)).is_ok());
    // The store refuses the read of the token: not "no valid token" (exit 1), the word (exit 75).
    flaky.refuse_next(1);
    let refused = crate::principal(&app, &opts(&token));
    flaky.allow_models();
    assert_eq!(
        refused.err().map(|c| format!("{c:?}")),
        Some(format!("{:?}", std::process::ExitCode::from(75)))
    );
    // A token that is not one stays exit 1, with the store healthy.
    let bad = crate::principal(&app, &opts("obop_made_up"));
    assert_eq!(
        bad.err().map(|c| format!("{c:?}")),
        Some(format!("{:?}", std::process::ExitCode::FAILURE))
    );
}
