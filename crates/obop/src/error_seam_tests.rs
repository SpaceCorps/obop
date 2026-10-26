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
use obop_commands::{CallError, Surface, Value, Vault, errors};
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
    obop_http::router(
        obop_http::HttpState::with_mcp(
            app.dispatcher.clone(),
            Arc::new(DevAuth),
            McpConfig::default(),
        ),
        None,
    )
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

#[test]
fn a_refused_tool_call_of_the_stdio_server_is_a_tool_error_with_the_wait_and_applied() {
    let (flaky, app) = flaky_app();
    let p = admin();
    let mut session = obop_mcp::Session::new();
    let mut call = |name: &str, args: J| {
        let line = json!({"jsonrpc":"2.0","id":7,"method":"tools/call",
            "params":{"name": name, "arguments": args}})
        .to_string();
        let out = obop_mcp::handle(&app.dispatcher, &p, &mut session, &line).unwrap();
        serde_json::from_str::<J>(&out).unwrap()
    };
    let healthy = call("lead__list", json!({}));
    assert_eq!(healthy["result"]["isError"], false, "{healthy}");

    flaky.refuse_next(1);
    let refused = call("lead__list", json!({}));
    flaky.allow_models();
    // This thread has no request scope: forget the fault the store noted on it.
    obop_storage::fault::clear();
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
}

#[test]
fn a_refused_command_line_call_prints_the_word_and_exits_with_75() {
    let (flaky, app) = flaky_app();
    let p = admin();
    let by_id = Value::map([("id", Value::text("nope"))]);
    let call = |id: &str, input: &Value| {
        app.dispatcher
            .call(Surface::Cli, &p, &mut Vault::new(), id, input)
    };

    // An error of the command itself keeps the documented line and status 1.
    let e = call("lead.get", &by_id).unwrap_err();
    assert_eq!(
        crate::call_failure_line(&e),
        ("error: Handler(\"no lead nope\")".to_string(), 1)
    );

    // The store's refusal is the same word as everywhere else, with what became of the changes,
    // and a status a script can tell from a failed command.
    flaky.refuse_next(1);
    let e = call("lead.get", &by_id).unwrap_err();
    flaky.allow_models();
    obop_storage::fault::clear();
    assert!(
        matches!(&e, CallError::Handler(m) if m.starts_with("storage_busy:")),
        "{e:?}"
    );
    let (line, status) = crate::call_failure_line(&e);
    assert_eq!(status, 75);
    assert!(
        line.starts_with("error: storage_busy: ")
            && line.ends_with("(nothing was changed; try again in 1 s)"),
        "{line}"
    );
}

#[test]
fn the_exit_status_and_line_of_every_failed_call_come_from_the_table() {
    let line = |e: CallError| crate::call_failure_line(&e);
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
    for (word, text, applied) in [
        (
            "storage_busy",
            "every connection is in use",
            "nothing was changed",
        ),
        (
            "storage_timeout",
            "a statement ran into its time limit",
            "nothing was changed",
        ),
        (
            "storage_unavailable",
            "the database cannot be reached",
            "a change in flight may or may not have been applied: read before you repeat a write",
        ),
    ] {
        let (l, status) = line(CallError::Handler(format!("{word}: {text}")));
        assert_eq!(status, 75, "{word}");
        assert!(
            l.starts_with(&format!("error: {word}: {text} ({applied}; try again in ")),
            "{l}"
        );
    }
}
