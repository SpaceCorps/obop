//! The approvals inbox (D97) over REST and MCP, with real sign-in: an agent token proposes, the
//! person sees and decides it on `/api/plan/*`, the agent reads the outcome with `plan.get`
//! (REST and the MCP tool `plan__get`); the alias `POST /api/approve/{plan_id}` answers as it
//! always has; the OpenAPI document lists what the caller may use.

use crate::app::App;
use crate::mfa_tests::{PW, post, send, token};
use obop_auth::{Auth, AuthConfig};
use obop_commands::Authenticator;
use serde_json::{Value as J, json};
use std::sync::Arc;

struct World {
    r: axum::Router,
    app: App,
    auth: Arc<Auth>,
}

fn world() -> World {
    let app = App::new();
    app.seed_demo();
    let auth = Arc::new(Auth::with_test_config(
        app.store.backend(),
        AuthConfig::insecure_dev(),
    ));
    auth.create_user("ada", PW, &app.all_groups()).unwrap();
    // Holds the groups of the command and not that of the contacts' tax number.
    auth.create_user(
        "bob",
        PW,
        &["base.user".to_string(), "privacy.officer".to_string()],
    )
    .unwrap();
    app.register_accounts(auth.clone() as Arc<dyn Authenticator>);
    let r = obop_http::router(
        obop_http::HttpState::new(app.dispatcher.clone(), auth.clone()),
        None,
    );
    World { r, app, auth }
}

async fn agent_token(r: &axum::Router, person: &str) -> String {
    let (s, b) = post(
        r,
        Some(person),
        "/api/auth/agent-token",
        json!({"scopes": ["base.user", "privacy.officer"], "label": "bot"}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    b["token"].as_str().unwrap().to_string()
}

/// The agent proposes to erase the first contact; the plan id.
async fn propose(r: &axum::Router, agent: &str) -> String {
    let (_, list) = post(r, Some(agent), "/api/party/list", json!({})).await;
    let id = list["value"][0]["id"].as_str().unwrap().to_string();
    let (s, b) = post(r, Some(agent), "/api/party/erase", json!({"id": id})).await;
    assert_eq!(s, 202, "{b}");
    b["plan_id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn rest_and_mcp_the_walk_from_proposal_to_what_the_agent_is_told() {
    let w = world();
    let r = &w.r;
    let ada = token(r, "ada").await;
    let agent = agent_token(r, &ada).await;
    let plan = propose(r, &agent).await;

    // The person: the count for the badge, the list, the detail.
    let (s, b) = post(r, Some(&ada), "/api/plan/count", json!({})).await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["value"]["waiting"], 1);
    assert_eq!(b["value"]["mine_waiting"], 1);
    let (s, b) = post(r, Some(&ada), "/api/plan/list", json!({})).await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["value"]["rows"][0]["id"], json!(plan));
    assert_eq!(b["value"]["rows"][0]["own_agent"], true);
    assert_eq!(b["value"]["rows"][0]["proposer"], "ada/agent");
    let (s, b) = post(r, Some(&ada), "/api/plan/get", json!({"id": plan})).await;
    assert_eq!(s, 200, "{b}");
    assert!(b["value"]["seen_version"].is_i64(), "{b}");

    // The agent: none of the person's commands exist for it, the same 404 as nothing at all.
    let (_, nothing) = post(r, Some(&agent), "/api/nothing/list", json!({})).await;
    for c in ["count", "list", "approve", "reject"] {
        let (s, b) = post(
            r,
            Some(&agent),
            &format!("/api/plan/{c}"),
            json!({"id": plan}),
        )
        .await;
        assert_eq!((s, &b), (404, &nothing), "{c}");
    }
    // It reads its own plan: waiting, and what to do.
    let (s, b) = post(r, Some(&agent), "/api/plan/get", json!({"id": plan})).await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["value"]["status"], "waiting");

    // A stale version is refused with 412 and the plan still waits.
    let (s, b) = post(
        r,
        Some(&ada),
        "/api/plan/approve",
        json!({"id": plan, "seen_version": 1}),
    )
    .await;
    assert_eq!(s, 412, "{b}");
    assert_eq!(b["code"], "precondition_failed");

    // Rejected with a reason; the agent hears it over REST and over MCP, the reason in its own
    // field.
    let (s, b) = post(
        r,
        Some(&ada),
        "/api/plan/reject",
        json!({"id": plan, "reason": "Not this one"}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["value"]["status"], "rejected");
    let (s, b) = post(r, Some(&agent), "/api/plan/get", json!({"id": plan})).await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["value"]["reason"], "Not this one");
    let principal = w.auth.resolve(&agent).unwrap();
    let mut session = obop_mcp::Session::new();
    let call = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"plan__get","arguments":{{"id":"{plan}"}}}}}}"#
    );
    let reply: J = serde_json::from_str(
        &obop_mcp::handle(&w.app.dispatcher, &principal, &mut session, &call).unwrap(),
    )
    .unwrap();
    let told = &reply["result"]["structuredContent"]["value"];
    assert_eq!(told["status"], "rejected", "{reply}");
    assert_eq!(told["reason"], "Not this one");
    assert!(
        !told["instruction"]
            .as_str()
            .unwrap()
            .contains("Not this one")
    );

    // A second decision: 409 with who and when.
    let (s, b) = post(r, Some(&ada), "/api/plan/approve", json!({"id": plan})).await;
    assert_eq!(s, 409, "{b}");
    assert_eq!(b["code"], "already_decided");
    assert_eq!(b["state"], "rejected");
    assert_eq!(b["decided_by"], "ada");
}

#[tokio::test]
async fn mcp_an_agent_is_shown_plan_get_only_and_told_where_a_person_decides() {
    let w = world();
    let ada = token(&w.r, "ada").await;
    let agent = agent_token(&w.r, &ada).await;
    let principal = w.auth.resolve(&agent).unwrap();
    let mut session = obop_mcp::Session::new();
    let list: J = serde_json::from_str(
        &obop_mcp::handle(
            &w.app.dispatcher,
            &principal,
            &mut session,
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
        )
        .unwrap(),
    )
    .unwrap();
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .filter(|n| n.starts_with("plan__"))
        .collect();
    assert_eq!(names, ["plan__get"]);
    let get = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "plan__get")
        .unwrap();
    assert_eq!(get["inputSchema"]["required"], json!(["id"]));
    // The proposal: the instruction names the Approvals page and plan__get, and no route an
    // agent cannot use.
    let (_, rows) = post(&w.r, Some(&agent), "/api/party/list", json!({})).await;
    let id = rows["value"][0]["id"].as_str().unwrap();
    let call = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"party__erase","arguments":{{"id":"{id}"}}}}}}"#
    );
    let reply: J = serde_json::from_str(
        &obop_mcp::handle(&w.app.dispatcher, &principal, &mut session, &call).unwrap(),
    )
    .unwrap();
    let out = &reply["result"]["structuredContent"];
    assert_eq!(out["status"], "approval_required", "{reply}");
    let instruction = out["instruction"].as_str().unwrap();
    assert!(instruction.contains("Approvals page") && instruction.contains("plan__get"));
    assert!(!instruction.contains("/api/approve"), "{instruction}");
    assert!(instruction.contains(out["plan_id"].as_str().unwrap()));
}

#[tokio::test]
async fn rest_the_approve_alias_keeps_its_answers() {
    let w = world();
    let r = &w.r;
    let ada = token(r, "ada").await;
    let agent = agent_token(r, &ada).await;
    let plan = propose(r, &agent).await;
    // The agent cannot approve its own plan; a person approves it; the second time is not found.
    assert_eq!(
        post(r, Some(&agent), &format!("/api/approve/{plan}"), json!({}))
            .await
            .0,
        404
    );
    let (s, b) = post(r, Some(&ada), &format!("/api/approve/{plan}"), json!({})).await;
    assert_eq!(s, 200, "{b}");
    let (s, b) = post(r, Some(&ada), &format!("/api/approve/{plan}"), json!({})).await;
    assert_eq!((s, b), (404, json!({"error": "not found"})));
    // The agent hears approved.
    let (_, b) = post(r, Some(&agent), "/api/plan/get", json!({"id": plan})).await;
    assert_eq!(b["value"]["status"], "approved");
}

#[tokio::test]
async fn openapi_lists_the_five_for_a_person_and_get_alone_for_an_agent() {
    let w = world();
    let ada = token(&w.r, "ada").await;
    let agent = agent_token(&w.r, &ada).await;
    let paths = |doc: &J| -> Vec<String> {
        let mut v: Vec<String> = doc["paths"]
            .as_object()
            .unwrap()
            .keys()
            .filter(|k| k.starts_with("/api/plan/"))
            .cloned()
            .collect();
        v.sort();
        v
    };
    let (s, doc) = send(&w.r, "GET", Some(&ada), "/api/openapi.json", json!({})).await;
    assert_eq!(s, 200);
    assert_eq!(
        paths(&doc),
        [
            "/api/plan/approve",
            "/api/plan/count",
            "/api/plan/get",
            "/api/plan/list",
            "/api/plan/reject"
        ]
    );
    assert!(doc["components"]["schemas"]["PlanRow"].is_object());
    let (_, doc) = send(&w.r, "GET", Some(&agent), "/api/openapi.json", json!({})).await;
    assert_eq!(paths(&doc), ["/api/plan/get"]);
    let (_, doc) = send(&w.r, "GET", None, "/api/openapi.json", json!({})).await;
    assert!(
        paths(&doc).is_empty(),
        "the anonymous document is unchanged"
    );
    // The model is no record: it is not in /api/meta.
    let (_, meta) = send(&w.r, "GET", Some(&ada), "/api/meta", json!({})).await;
    assert!(!meta.to_string().contains("\"name\":\"plan\""));
}

#[tokio::test]
async fn rest_another_person_who_may_decide_sees_the_plan_and_it_is_not_his_agents() {
    let w = world();
    let r = &w.r;
    let ada = token(r, "ada").await;
    let bob = token(r, "bob").await;
    let agent = agent_token(r, &ada).await;
    let plan = propose(r, &agent).await;
    // bob holds the group of party.erase and can read contacts: he may decide it, and it is not
    // his own agent's.
    let (s, b) = post(r, Some(&bob), "/api/plan/get", json!({"id": plan})).await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["value"]["can_approve"], true);
    let (s, b) = post(r, Some(&bob), "/api/plan/list", json!({"mine": true})).await;
    assert_eq!(s, 200);
    assert_eq!(b["value"]["total"], 0, "not his agent");
}
