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
    // Each is described once, by its own shapes, never by the untyped description of the
    // commands outside /api/meta (K26, `describe_commands`): no operation of the plan commands
    // sits under another path, and the one under its path is the typed one.
    let described_by_plans = |doc: &J| {
        let ops: Vec<(String, J)> = doc["paths"]
            .as_object()
            .unwrap()
            .iter()
            .filter(|(_, item)| {
                item["post"]["operationId"]
                    .as_str()
                    .is_some_and(|o| o.starts_with("plan."))
            })
            .map(|(k, item)| (k.clone(), item["post"]["tags"].clone()))
            .collect();
        for (path, tags) in &ops {
            assert!(path.starts_with("/api/plan/"), "{path}");
            assert_eq!(*tags, json!(["Approvals"]), "{path}");
        }
        ops.len()
    };
    assert_eq!(described_by_plans(&doc), 5);
    let (_, doc) = send(&w.r, "GET", Some(&agent), "/api/openapi.json", json!({})).await;
    assert_eq!(paths(&doc), ["/api/plan/get"]);
    assert_eq!(described_by_plans(&doc), 1);
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

#[tokio::test]
async fn rest_an_agents_plan_runs_as_its_owner_is_now_demoted_or_offboarded() {
    let w = world();
    let r = &w.r;
    // Another administrator, so that ada can lose hers.
    w.auth
        .create_user("root", PW, &["platform.admin".to_string()])
        .unwrap();
    let ada = token(r, "ada").await;
    let bob = token(r, "bob").await;
    let agent = agent_token(r, &ada).await;
    let plan = propose(r, &agent).await;
    let rights = |b: &J| {
        (
            b["value"]["can_approve"].clone(),
            b["value"]["proposer_can_run"].clone(),
            b["value"]["can_reject"].clone(),
        )
    };
    let (s, b) = post(r, Some(&bob), "/api/plan/get", json!({"id": plan})).await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(rights(&b), (json!(true), json!(true), json!(true)));
    let all: Vec<String> = w
        .app
        .all_groups()
        .into_iter()
        .filter(|g| g != obop_commands::ANY_GROUP)
        .collect();
    for step in ["demoted", "offboarded"] {
        if step == "demoted" {
            // ada keeps base.user and loses privacy.officer, the group of party.erase.
            w.auth
                .operator_set_groups("ada", &["base.user".to_string()], &all)
                .unwrap();
        } else {
            w.auth.operator_set_groups("ada", &all, &all).unwrap();
            let (_, b) = post(r, Some(&bob), "/api/plan/get", json!({"id": plan})).await;
            assert_eq!(rights(&b).0, json!(true), "given the group back");
            w.auth.operator_offboard("ada", "left").unwrap();
        }
        let (s, b) = post(r, Some(&bob), "/api/plan/get", json!({"id": plan})).await;
        assert_eq!(s, 200, "{step}: {b}");
        assert_eq!(
            rights(&b),
            (json!(false), json!(false), json!(true)),
            "{step}"
        );
        let (s, b) = post(r, Some(&bob), "/api/plan/approve", json!({"id": plan})).await;
        assert_eq!((s, b), (404, json!({"error": "not found"})), "{step}");
        let (s, _) = post(r, Some(&bob), &format!("/api/approve/{plan}"), json!({})).await;
        assert_eq!(s, 404, "{step}: the alias refuses it too");
        let (_, b) = post(r, Some(&bob), "/api/plan/list", json!({})).await;
        assert_eq!(b["value"]["total"], 1, "{step}: it still waits");
    }
    // Reject is still offered, and works.
    let (_, b) = post(r, Some(&bob), "/api/plan/reject", json!({"id": plan})).await;
    assert_eq!(b["value"]["status"], "rejected", "{b}");
}

/// A JSON-RPC message to `/mcp` in the session `sid`, as an MCP client sends it.
async fn mcp(r: &axum::Router, token: &str, sid: &str, msg: J) -> J {
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let req = Request::post("/mcp")
        .header("host", "inst.test")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("authorization", format!("Bearer {token}"))
        .header("mcp-session-id", sid)
        .body(Body::from(msg.to_string()))
        .unwrap();
    let resp = r.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn rule1_a_phone_token_over_rest_and_an_oauth_token_over_mcp_are_no_person() {
    use obop_commands::OAuthTokens;
    let w = world();
    let r = &w.r;
    let ada = token(r, "ada").await;
    let agent = agent_token(r, &ada).await;
    let plan = propose(r, &agent).await;

    // The access token of ada's phone: ada herself, narrowed to some groups, on /api.
    let device = "phone-1";
    let mut rec = obop_storage::Fields::new();
    rec.insert("state".into(), json!("active"));
    rec.insert("grant".into(), json!("grant-phone"));
    rec.insert("registered_at".into(), json!("2026-11-17T10:00:00Z"));
    w.app
        .store
        .backend()
        .insert("device", device, &rec)
        .unwrap();
    let (phone, _) = w
        .auth
        .issue_mobile_access(
            "ada",
            &["base.user".to_string()],
            "phone",
            3600,
            &obop_oauth::api_resource("http://localhost"),
            "grant-phone",
            device,
        )
        .unwrap();
    let (s, _) = post(r, Some(&phone), "/api/party/list", json!({})).await;
    assert_eq!(s, 200, "the phone token works on /api");
    let (_, nothing) = post(r, Some(&phone), "/api/nothing/list", json!({})).await;
    for c in ["count", "list", "approve", "reject", "get"] {
        let (s, b) = post(
            r,
            Some(&phone),
            &format!("/api/plan/{c}"),
            json!({"id": plan}),
        )
        .await;
        if c == "get" {
            // plan.get exists for it, as for an agent: not its plan.
            assert_eq!((s, b), (404, json!({"error": "not found"})), "{c}");
        } else {
            assert_eq!((s, &b), (404, &nothing), "{c}");
        }
    }

    // The access token of a connected MCP client (OAuth): an agent of ada, on /mcp.
    let (client, _) = w
        .auth
        .issue_oauth_access(
            "ada",
            &["base.user".to_string(), "privacy.officer".to_string()],
            "client",
            3600,
            &obop_oauth::mcp_resource("http://inst.test"),
            "grant-mcp",
        )
        .unwrap();
    let (sid, _) = crate::mcp_tests::initialize(r, &client, "2025-11-25").await;
    let list = mcp(
        r,
        &client,
        &sid,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    )
    .await;
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .filter(|n| n.starts_with("plan__"))
        .collect();
    assert_eq!(names, ["plan__get"]);
    for c in ["count", "list", "approve", "reject"] {
        let name = format!("plan__{c}");
        let reply = mcp(
            r,
            &client,
            &sid,
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call",
                   "params":{"name": name, "arguments": {"id": plan}}}),
        )
        .await;
        assert_eq!(reply["error"]["code"], -32602, "{reply}");
        assert_eq!(
            reply["error"]["message"],
            format!("unknown tool: {name}"),
            "{reply}"
        );
    }
    // Nothing ran: the plan still waits for ada.
    let (_, b) = post(r, Some(&ada), "/api/plan/list", json!({})).await;
    assert_eq!(b["value"]["total"], 1);
}

/// Main's L17 (the catalogues show a token only the device commands it can use) and L20 (a mobile
/// app's token is refused the setting record) filter the same lists as rule 1 of D97: after both,
/// each kind of principal still sees exactly the plan commands D97 gives it, in the catalogue of
/// `GET /api/commands` and in its OpenAPI document alike: a person all five, an agent's token and
/// a phone's token `plan.get` alone (the MCP side is `mcp_an_agent_is_shown_plan_get_only...` and
/// `rule1_a_phone_token_over_rest...`).
#[tokio::test]
async fn rule1_the_catalogue_and_the_openapi_document_of_each_principal_agree_after_l17_and_l20() {
    use obop_commands::OAuthTokens;
    let w = world();
    let r = &w.r;
    let ada = token(r, "ada").await;
    let agent = agent_token(r, &ada).await;
    let device = "phone-2";
    let mut rec = obop_storage::Fields::new();
    rec.insert("state".into(), json!("active"));
    rec.insert("grant".into(), json!("grant-phone-2"));
    rec.insert("registered_at".into(), json!("2026-11-18T10:00:00Z"));
    w.app
        .store
        .backend()
        .insert("device", device, &rec)
        .unwrap();
    let (phone, _) = w
        .auth
        .issue_mobile_access(
            "ada",
            &["base.user".to_string()],
            "phone",
            3600,
            &obop_oauth::api_resource("http://localhost"),
            "grant-phone-2",
            device,
        )
        .unwrap();
    let all = [
        "plan.approve",
        "plan.count",
        "plan.get",
        "plan.list",
        "plan.reject",
    ];
    for (who, tok, want) in [
        ("person", &ada, &all[..]),
        ("agent", &agent, &["plan.get"][..]),
        ("phone", &phone, &["plan.get"][..]),
    ] {
        let (s, b) = send(r, "GET", Some(tok), "/api/commands", json!({})).await;
        assert_eq!(s, 200, "{who}: {b}");
        let mut listed: Vec<&str> = b["commands"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|c| c["id"].as_str())
            .filter(|id| id.starts_with("plan."))
            .collect();
        listed.sort();
        assert_eq!(listed, want, "{who}: the catalogue");
        let (s, doc) = send(r, "GET", Some(tok), "/api/openapi.json", json!({})).await;
        assert_eq!(s, 200, "{who}");
        let mut ops: Vec<String> = doc["paths"]
            .as_object()
            .unwrap()
            .values()
            .filter_map(|item| item["post"]["operationId"].as_str())
            .filter(|o| o.starts_with("plan."))
            .map(String::from)
            .collect();
        ops.sort();
        assert_eq!(ops, want, "{who}: the OpenAPI document");
    }
    // L20 still holds beside it: the phone's token is refused the setting record.
    let (s, _) = post(r, Some(&phone), "/api/setting/get", json!({"id": "ui"})).await;
    assert_ne!(s, 200, "the phone reads no setting (L20)");
}

/// ISSUES V16, unchanged by main's N18, K19 and L16 (which end more agent tokens: on an unbinding,
/// on "Microsoft only", after 90 days, and those made during a revocation): a waiting plan names
/// its agent `{owner}/agent`, not the token, so ending the token that proposed it leaves the plan
/// to be decided by a person. The token itself can no longer read it; another agent token of the
/// same person still can (V7, V17). When V16 is closed this
/// test is the one to turn around.
#[tokio::test]
async fn v16_a_revoked_agent_token_leaves_its_waiting_plan_to_a_person() {
    let w = world();
    let r = &w.r;
    let ada = token(r, "ada").await;
    let agent = agent_token(r, &ada).await;
    let plan = propose(r, &agent).await;
    let (s, b) = send(r, "GET", Some(&ada), "/api/auth/agent-tokens", json!({})).await;
    assert_eq!(s, 200, "{b}");
    let id = b["tokens"][0]["id"].as_str().unwrap().to_string();
    let (s, b) = post(
        r,
        Some(&ada),
        &format!("/api/auth/agent-tokens/{id}/revoke"),
        json!({}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    let (s, _) = post(r, Some(&agent), "/api/plan/get", json!({"id": plan})).await;
    assert_eq!(s, 401, "the ended token reads nothing");
    // But every agent token of one person is the one principal `{owner}/agent` (V7, V17): another
    // token of ada, also one made after the revocation, still reads the plan by its id.
    let sibling = agent_token(r, &ada).await;
    let (s, b) = post(r, Some(&sibling), "/api/plan/get", json!({"id": plan})).await;
    assert_eq!(s, 200, "a sibling token reads it: {b}");
    assert_eq!(b["value"]["status"], "waiting", "{b}");
    let (_, b) = post(r, Some(&ada), "/api/plan/list", json!({})).await;
    assert_eq!(b["value"]["total"], 1, "the plan still waits: {b}");
    // A person approves it and the erase it proposed runs (the answer is the command's own).
    let (s, b) = post(r, Some(&ada), "/api/plan/approve", json!({"id": plan})).await;
    assert_eq!(s, 200, "{b}");
    let (_, b) = post(r, Some(&ada), "/api/plan/list", json!({})).await;
    assert_eq!(b["value"]["total"], 0, "nothing waits any more: {b}");
}

/// Main's N5: an account that signs in with Microsoft only can now connect an MCP client through
/// the OAuth sign-in pages (a grant of at most 30 days). Its access token is what the exchange
/// issues for any MCP client, an agent of the account, so on every door it is still no person: the
/// tools list holds `plan__get` alone, the four commands of a person are unknown tools, and a
/// plan waiting for that person is left as it was.
#[tokio::test]
async fn n5_the_mcp_client_of_a_microsoft_only_account_is_no_person() {
    use obop_commands::OAuthTokens;
    let w = world();
    let r = &w.r;
    let ada = token(r, "ada").await;
    let agent = agent_token(r, &ada).await;
    let plan = propose(r, &agent).await;
    // ada signs in with Microsoft only from now on (what `sso_only` stores).
    let mut only = obop_storage::Fields::new();
    only.insert("sso_only".into(), json!(true));
    w.app
        .store
        .backend()
        .update("auth_user", "ada", &only)
        .unwrap();
    assert_eq!(w.auth.is_microsoft_only("ada"), Ok(true));
    let (client, _) = w
        .auth
        .issue_oauth_access(
            "ada",
            &["base.user".to_string(), "privacy.officer".to_string()],
            "client",
            30 * 24 * 3600,
            &obop_oauth::mcp_resource("http://inst.test"),
            "grant-ms-only",
        )
        .unwrap();
    let principal = w.auth.resolve(&client).expect("the client's token works");
    assert!(!principal.is_person(), "{principal:?}");
    let (sid, _) = crate::mcp_tests::initialize(r, &client, "2025-11-25").await;
    let list = mcp(
        r,
        &client,
        &sid,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    )
    .await;
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .filter(|n| n.starts_with("plan__"))
        .collect();
    assert_eq!(names, ["plan__get"]);
    for c in ["count", "list", "approve", "reject"] {
        let name = format!("plan__{c}");
        let reply = mcp(
            r,
            &client,
            &sid,
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call",
                   "params":{"name": name, "arguments": {"id": plan}}}),
        )
        .await;
        assert_eq!(
            reply["error"]["message"],
            format!("unknown tool: {name}"),
            "{reply}"
        );
    }
    // plan__get: the client is an agent of ada, the principal `ada/agent` that every agent token
    // and connected client of hers shares (docs/ISSUES.md V7, V17), so it reads the plan as its
    // own, with the answer an agent gets: what became of it and what to do, no person's view.
    let reply = mcp(
        r,
        &client,
        &sid,
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call",
               "params":{"name": "plan__get", "arguments": {"id": plan}}}),
    )
    .await;
    let told = &reply["result"]["structuredContent"]["value"];
    assert_eq!(told["status"], "waiting", "{reply}");
    let mut keys: Vec<&str> = told
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "command",
            "created_at",
            "decided_at",
            "expires_at",
            "failure",
            "id",
            "instruction",
            "reason",
            "record_id",
            "status"
        ],
        "{reply}"
    );
    // Nothing was decided: the plan still waits for a person who may decide it.
    let person = obop_commands::Principal::user(
        "ada",
        &w.app
            .all_groups()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    );
    let rows = w
        .app
        .dispatcher
        .call(
            obop_commands::Surface::Rest,
            &person,
            &mut obop_commands::Vault::new(),
            "plan.list",
            &obop_commands::Value::Map(Default::default()),
        )
        .unwrap()
        .value
        .to_json();
    assert_eq!(rows["total"], 1, "{rows}");
}
