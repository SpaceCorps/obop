//! The remote MCP endpoint (`/mcp`, D51) end to end over HTTP: handshake, sessions, origin and
//! auth checks, limits, tools, masking and the approval loop, against a real `App`.

use crate::app::{self, App, DevAuth};
use axum::{
    body::Body,
    http::{HeaderMap, Request},
};
use http_body_util::BodyExt;
use obop_http::mcp::McpConfig;
use serde_json::{Value as J, json};
use std::sync::Arc;
use tower::ServiceExt;

const HOST: &str = "inst.test";

fn router_with(cfg: McpConfig) -> axum::Router {
    let app = App::new();
    app.seed_demo();
    obop_http::router(
        obop_http::HttpState::with_mcp(app.dispatcher.clone(), Arc::new(DevAuth), cfg),
        None,
    )
}

fn router() -> axum::Router {
    router_with(McpConfig::default())
}

struct Resp {
    status: u16,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl Resp {
    fn json(&self) -> J {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|_| panic!("not JSON: {}", String::from_utf8_lossy(&self.body)))
    }
    fn header(&self, name: &str) -> &str {
        self.headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    }
}

async fn send(r: &axum::Router, req: Request<Body>) -> Resp {
    let resp = r.clone().oneshot(req).await.unwrap();
    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    let body = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    Resp {
        status,
        headers,
        body,
    }
}

/// POST to /mcp with the usual client headers. `extra` adds or overrides headers.
async fn post_raw(
    r: &axum::Router,
    token: Option<&str>,
    session: Option<&str>,
    body: String,
    extra: &[(&str, &str)],
) -> Resp {
    let mut hs: Vec<(String, String)> = vec![
        ("host".into(), HOST.into()),
        ("content-type".into(), "application/json".into()),
        (
            "accept".into(),
            "application/json, text/event-stream".into(),
        ),
    ];
    if let Some(t) = token {
        hs.push(("authorization".into(), format!("Bearer {t}")));
    }
    if let Some(s) = session {
        hs.push(("mcp-session-id".into(), s.into()));
    }
    for (k, v) in extra {
        hs.retain(|(name, _)| name != k);
        hs.push((k.to_string(), v.to_string()));
    }
    let mut req = Request::post("/mcp");
    for (k, v) in hs {
        req = req.header(k, v);
    }
    send(r, req.body(Body::from(body)).unwrap()).await
}

async fn rpc(r: &axum::Router, token: &str, session: &str, msg: J) -> Resp {
    post_raw(r, Some(token), Some(session), msg.to_string(), &[]).await
}

async fn initialize(r: &axum::Router, token: &str, version: &str) -> (String, J) {
    let resp = post_raw(
        r,
        Some(token),
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion": version, "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}
        }})
        .to_string(),
        &[],
    )
    .await;
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    let id = resp.header("mcp-session-id").to_string();
    assert!(!id.is_empty());
    let note = post_raw(
        r,
        Some(token),
        Some(&id),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}).to_string(),
        &[],
    )
    .await;
    assert_eq!(note.status, 202);
    assert!(note.body.is_empty());
    (id, resp.json())
}

async fn session(r: &axum::Router, token: &str) -> String {
    initialize(r, token, "2025-11-25").await.0
}

async fn call_tool(r: &axum::Router, token: &str, sid: &str, name: &str, args: J) -> J {
    let resp = rpc(
        r,
        token,
        sid,
        json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":name,"arguments":args}}),
    )
    .await;
    assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
    resp.json()
}

/// REST call (for the human side of the approval loop and for checking the outcome).
async fn rest(r: &axum::Router, token: &str, path: &str, body: J) -> (u16, J) {
    let resp = send(
        r,
        Request::post(path)
            .header("host", HOST)
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await;
    (resp.status, resp.json())
}

#[tokio::test]
async fn handshake_negotiates_a_supported_revision_and_issues_a_session() {
    let r = router();
    for v in ["2025-11-25", "2025-06-18", "2025-03-26"] {
        let (sid, init) = initialize(&r, "dev-agent", v).await;
        assert_eq!(sid.len(), 64);
        assert_eq!(init["result"]["protocolVersion"], v);
        assert_eq!(init["result"]["serverInfo"]["name"], "obop");
        assert_eq!(
            init["result"]["capabilities"]["tools"]["listChanged"],
            false
        );
        assert!(
            init["result"]["instructions"]
                .as_str()
                .unwrap()
                .contains("approval_required")
        );
    }
    // The stateless 2026-07-28 revision is not spoken: answer with our newest.
    let (_, init) = initialize(&r, "dev-agent", "2026-07-28").await;
    assert_eq!(init["result"]["protocolVersion"], "2025-11-25");
    // initialize needs a version.
    let bad = post_raw(
        &r,
        Some("dev-agent"),
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}).to_string(),
        &[],
    )
    .await;
    assert_eq!(bad.json()["error"]["code"], -32602);
    assert!(bad.header("mcp-session-id").is_empty());
}

#[tokio::test]
async fn ping_and_unknown_methods() {
    let r = router();
    let sid = session(&r, "dev-agent").await;
    let p = rpc(
        &r,
        "dev-agent",
        &sid,
        json!({"jsonrpc":"2.0","id":2,"method":"ping"}),
    )
    .await;
    assert_eq!((p.status, p.json()["result"].clone()), (200, json!({})));
    let m = rpc(
        &r,
        "dev-agent",
        &sid,
        json!({"jsonrpc":"2.0","id":"abc","method":"resources/list"}),
    )
    .await;
    assert_eq!(m.status, 200);
    assert_eq!(m.json()["error"]["code"], -32601);
    assert_eq!(m.json()["id"], "abc");
    // A client response (we send no requests) is accepted and dropped.
    let c = rpc(
        &r,
        "dev-agent",
        &sid,
        json!({"jsonrpc":"2.0","id":9,"result":{}}),
    )
    .await;
    assert_eq!(c.status, 202);
}

#[tokio::test]
async fn missing_or_bad_tokens_get_401_with_resource_metadata() {
    let r = router();
    let body = json!({"jsonrpc":"2.0","id":1,"method":"ping"}).to_string();
    let none = post_raw(&r, None, None, body.clone(), &[]).await;
    assert_eq!(none.status, 401);
    assert_eq!(
        none.header("www-authenticate"),
        "Bearer resource_metadata=\"http://inst.test/.well-known/oauth-protected-resource\""
    );
    let bad = post_raw(&r, Some("obop_forged"), None, body.clone(), &[]).await;
    assert_eq!(bad.status, 401);
    let w = bad.header("www-authenticate");
    assert!(
        w.starts_with("Bearer error=\"invalid_token\"")
            && w.contains(
                "resource_metadata=\"http://inst.test/.well-known/oauth-protected-resource\""
            ),
        "{w}"
    );
    // Behind a TLS proxy the challenge names https.
    let https = post_raw(&r, None, None, body, &[("x-forwarded-proto", "https")]).await;
    assert!(
        https
            .header("www-authenticate")
            .contains("https://inst.test/")
    );
    // Every method needs the token, GET and DELETE too.
    for m in ["GET", "DELETE"] {
        let resp = send(
            &r,
            Request::builder()
                .method(m)
                .uri("/mcp")
                .header("host", HOST)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(resp.status, 401, "{m}");
    }
}

#[tokio::test]
async fn sessions_are_required_validated_ended_and_bound_to_the_token() {
    let r = router();
    let sid = session(&r, "dev-agent").await;
    let list = json!({"jsonrpc":"2.0","id":2,"method":"tools/list"});
    // No session header: 400. Unknown: 404.
    let none = post_raw(&r, Some("dev-agent"), None, list.to_string(), &[]).await;
    assert_eq!(none.status, 400);
    let unknown = rpc(&r, "dev-agent", "nope", list.clone()).await;
    assert_eq!(unknown.status, 404);
    // Another token cannot use (or learn about) the session.
    let other = rpc(&r, "dev-admin", &sid, list.clone()).await;
    assert_eq!(other.status, 404);
    assert_eq!(rpc(&r, "dev-agent", &sid, list.clone()).await.status, 200);
    // DELETE ends it; later requests are 404; ending twice is 404.
    let del = |token: &'static str, sid: String| {
        let r = r.clone();
        async move {
            send(
                &r,
                Request::delete("/mcp")
                    .header("host", HOST)
                    .header("authorization", format!("Bearer {token}"))
                    .header("mcp-session-id", sid)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
        }
    };
    assert_eq!(del("dev-admin", sid.clone()).await.status, 404);
    assert_eq!(del("dev-agent", sid.clone()).await.status, 204);
    assert_eq!(rpc(&r, "dev-agent", &sid, list).await.status, 404);
    assert_eq!(del("dev-agent", sid).await.status, 404);
}

#[tokio::test]
async fn expired_sessions_are_404() {
    let r = router_with(McpConfig {
        session_ttl: std::time::Duration::from_millis(50),
        ..McpConfig::default()
    });
    let sid = session(&r, "dev-agent").await;
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    let resp = rpc(
        &r,
        "dev-agent",
        &sid,
        json!({"jsonrpc":"2.0","id":2,"method":"ping"}),
    )
    .await;
    assert_eq!(resp.status, 404);
}

#[tokio::test]
async fn origins_that_are_not_allowed_get_403_before_anything_else() {
    let r = router_with(McpConfig {
        allowed_origins: vec!["https://client.example".into()],
        ..McpConfig::default()
    });
    let ping = json!({"jsonrpc":"2.0","id":1,"method":"ping"}).to_string();
    // A rebound page: Origin and Host are the attacker's name.
    let evil = post_raw(
        &r,
        Some("dev-agent"),
        None,
        ping.clone(),
        &[("origin", "http://evil.test"), ("host", "evil.test")],
    )
    .await;
    assert_eq!(evil.status, 403);
    // 403 comes before 401: nothing is learned without an allowed origin.
    let evil_no_token = post_raw(
        &r,
        None,
        None,
        ping.clone(),
        &[("origin", "http://evil.test")],
    )
    .await;
    assert_eq!(evil_no_token.status, 403);
    let null = post_raw(
        &r,
        Some("dev-agent"),
        None,
        ping.clone(),
        &[("origin", "null")],
    )
    .await;
    assert_eq!(null.status, 403);
    // Listed origin: passes and gets CORS headers; no Origin header: passes (not a browser).
    let ok = post_raw(
        &r,
        Some("dev-agent"),
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}).to_string(),
        &[("origin", "https://client.example")],
    )
    .await;
    assert_eq!(ok.status, 200);
    assert_eq!(
        ok.header("access-control-allow-origin"),
        "https://client.example"
    );
    assert!(
        ok.header("access-control-expose-headers")
            .contains("Mcp-Session-Id")
    );
    // Same-origin on a loopback host is fine.
    let local = post_raw(
        &r,
        Some("dev-agent"),
        None,
        ping,
        &[
            ("origin", "http://localhost:8080"),
            ("host", "localhost:8080"),
        ],
    )
    .await;
    assert_ne!(local.status, 403);
    // Preflight.
    let pre = send(
        &r,
        Request::builder()
            .method("OPTIONS")
            .uri("/mcp")
            .header("host", HOST)
            .header("origin", "https://client.example")
            .header("access-control-request-method", "POST")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(pre.status, 204);
    assert!(
        pre.header("access-control-allow-headers")
            .contains("Mcp-Session-Id")
    );
    let pre_evil = send(
        &r,
        Request::builder()
            .method("OPTIONS")
            .uri("/mcp")
            .header("host", HOST)
            .header("origin", "https://evil.example")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(pre_evil.status, 403);
}

#[tokio::test]
async fn content_negotiation_and_unsupported_methods() {
    let r = router();
    let ping = json!({"jsonrpc":"2.0","id":1,"method":"ping"}).to_string();
    let sse_only = post_raw(
        &r,
        Some("dev-agent"),
        None,
        ping.clone(),
        &[("accept", "text/event-stream")],
    )
    .await;
    assert_eq!(sse_only.status, 406);
    let text = post_raw(
        &r,
        Some("dev-agent"),
        None,
        ping.clone(),
        &[("content-type", "text/plain")],
    )
    .await;
    assert_eq!(text.status, 415);
    let get = send(
        &r,
        Request::get("/mcp")
            .header("host", HOST)
            .header("authorization", "Bearer dev-agent")
            .header("accept", "text/event-stream")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(get.status, 405);
    assert_eq!(get.header("allow"), "POST, DELETE, OPTIONS");
    // An unsupported protocol version header on a later request is a 400.
    let sid = session(&r, "dev-agent").await;
    let bad = post_raw(
        &r,
        Some("dev-agent"),
        Some(&sid),
        ping,
        &[("mcp-protocol-version", "1999-01-01")],
    )
    .await;
    assert_eq!(bad.status, 400);
    assert!(
        bad.json()["error"]["message"]
            .as_str()
            .unwrap()
            .contains("2025-11-25")
    );
}

#[tokio::test]
async fn malformed_json_and_invalid_requests_are_400_with_json_rpc_errors() {
    let r = router();
    let sid = session(&r, "dev-agent").await;
    let post = |body: &str| {
        let (r, sid, body) = (r.clone(), sid.clone(), body.to_string());
        async move { post_raw(&r, Some("dev-agent"), Some(&sid), body, &[]).await }
    };
    let broken = post("{not json").await;
    assert_eq!(broken.status, 400);
    assert_eq!(broken.json()["error"]["code"], -32700);
    assert_eq!(broken.json()["jsonrpc"], "2.0");
    for bad in [
        "42",
        "\"x\"",
        "{}",
        r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#,
        r#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#,
        r#"{"jsonrpc":"2.0","id":{"a":1},"method":"ping"}"#,
    ] {
        let resp = post(bad).await;
        assert_eq!(resp.status, 400, "{bad}");
        assert_eq!(resp.json()["error"]["code"], -32600, "{bad}");
    }
    // Missing arguments / wrong shapes are invalid params, a normal 200 JSON-RPC error.
    let no_name = post(r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{}}"#).await;
    assert_eq!(
        (no_name.status, no_name.json()["error"]["code"].clone()),
        (200, json!(-32602))
    );
    let bad_args = post(
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"party__list","arguments":[1]}}"#,
    )
    .await;
    assert_eq!(bad_args.json()["error"]["code"], -32602);
    let cursor =
        post(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"cursor":"zzz"}}"#).await;
    assert_eq!(cursor.json()["error"]["code"], -32602);
}

#[tokio::test]
async fn batches_follow_the_negotiated_revision() {
    let r = router();
    let batch = json!([
        {"jsonrpc":"2.0","id":1,"method":"ping"},
        {"jsonrpc":"2.0","method":"notifications/cancelled"},
        {"jsonrpc":"2.0","id":2,"method":"tools/list"}
    ]);
    // 2025-06-18 and later removed batching.
    let (new, _) = initialize(&r, "dev-agent", "2025-06-18").await;
    let rejected = rpc(&r, "dev-agent", &new, batch.clone()).await;
    assert_eq!(rejected.status, 400);
    assert_eq!(rejected.json()["error"]["code"], -32600);
    // 2025-03-26 has them: one answer per request, none for the notification.
    let (old, _) = initialize(&r, "dev-agent", "2025-03-26").await;
    let ok = rpc(&r, "dev-agent", &old, batch).await;
    assert_eq!(ok.status, 200);
    let arr = ok.json();
    let arr = arr.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["id"], 1);
    assert!(arr[1]["result"]["tools"].is_array());
    // A batch of notifications is accepted with no body; an empty one or one with initialize is not.
    let notes = rpc(
        &r,
        "dev-agent",
        &old,
        json!([{"jsonrpc":"2.0","method":"notifications/cancelled"}]),
    )
    .await;
    assert_eq!(notes.status, 202);
    assert_eq!(rpc(&r, "dev-agent", &old, json!([])).await.status, 400);
    let with_init = rpc(
        &r,
        "dev-agent",
        &old,
        json!([{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}]),
    )
    .await;
    assert_eq!(with_init.status, 400);
    // initialize itself inside a batch cannot open a session.
    let open = post_raw(
        &r,
        Some("dev-agent"),
        None,
        json!([{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}]).to_string(),
        &[],
    )
    .await;
    assert_eq!(open.status, 400);
}

#[tokio::test]
async fn request_size_and_rate_are_limited() {
    let r = router_with(McpConfig {
        max_body_bytes: 2048,
        rate_per_minute: 6,
        ..McpConfig::default()
    });
    let sid = session(&r, "dev-agent").await; // 2 requests
    let big = json!({"jsonrpc":"2.0","id":1,"method":"ping","params":{"pad":"x".repeat(4000)}});
    let too_big = rpc(&r, "dev-agent", &sid, big).await; // 3rd
    assert_eq!(too_big.status, 413);
    for _ in 0..3 {
        // 4th to 6th
        let ok = rpc(
            &r,
            "dev-agent",
            &sid,
            json!({"jsonrpc":"2.0","id":1,"method":"ping"}),
        )
        .await;
        assert_eq!(ok.status, 200);
    }
    let limited = rpc(
        &r,
        "dev-agent",
        &sid,
        json!({"jsonrpc":"2.0","id":1,"method":"ping"}),
    )
    .await;
    assert_eq!(limited.status, 429);
    assert!(limited.header("retry-after").parse::<u32>().unwrap() >= 1);
    // The limit is per token.
    let other = post_raw(
        &r,
        Some("dev-admin"),
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"ping"}).to_string(),
        &[],
    )
    .await;
    assert_ne!(other.status, 429);
}

#[tokio::test]
async fn session_count_is_capped_per_token() {
    let r = router_with(McpConfig {
        max_sessions_per_token: 2,
        ..McpConfig::default()
    });
    let a = session(&r, "dev-agent").await;
    let b = session(&r, "dev-agent").await;
    let c = session(&r, "dev-agent").await;
    let ping = json!({"jsonrpc":"2.0","id":1,"method":"ping"});
    // The oldest was dropped to make room.
    assert_eq!(rpc(&r, "dev-agent", &a, ping.clone()).await.status, 404);
    assert_eq!(rpc(&r, "dev-agent", &b, ping.clone()).await.status, 200);
    assert_eq!(rpc(&r, "dev-agent", &c, ping).await.status, 200);
}

async fn tools_of(r: &axum::Router, token: &str, sid: &str) -> Vec<J> {
    let mut out = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let params = cursor.as_ref().map_or(json!({}), |c| json!({"cursor": c}));
        let resp = rpc(
            r,
            token,
            sid,
            json!({"jsonrpc":"2.0","id":3,"method":"tools/list","params":params}),
        )
        .await
        .json();
        out.extend(resp["result"]["tools"].as_array().unwrap().clone());
        match resp["result"]["nextCursor"].as_str() {
            Some(c) => cursor = Some(c.to_string()),
            None => return out,
        }
    }
}

#[tokio::test]
async fn tools_mirror_the_rest_surface_for_the_caller() {
    let r = router();
    let sid = session(&r, "dev-agent").await;
    let tools = tools_of(&r, "dev-agent", &sid).await;
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names[0], "obop__meta");
    // One tool per command REST lists for the same token, named model__command.
    let (_, cmds) = {
        let resp = send(
            &r,
            Request::get("/api/commands")
                .header("host", HOST)
                .header("authorization", "Bearer dev-agent")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        (resp.status, resp.json())
    };
    let mut expected: Vec<String> = cmds["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().replacen('.', "__", 1))
        .collect();
    expected.push("obop__meta".into());
    expected.sort();
    let mut got: Vec<String> = names.iter().map(|s| s.to_string()).collect();
    got.sort();
    assert_eq!(got, expected);
    for t in &tools {
        assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
        assert!(t["annotations"]["title"].is_string(), "{}", t["name"]);
        assert!(t["description"].as_str().unwrap().len() > 3);
    }
    let tool = |n: &str| tools.iter().find(|t| t["name"] == n).unwrap().clone();
    // Reads, deletes and approval wording.
    assert_eq!(tool("party__list")["annotations"]["readOnlyHint"], true);
    assert_eq!(tool("party__search")["annotations"]["readOnlyHint"], true);
    assert_eq!(tool("party__get")["annotations"]["idempotentHint"], true);
    assert_eq!(
        tool("party__delete")["annotations"]["destructiveHint"],
        true
    );
    assert_eq!(tool("party__create")["annotations"]["readOnlyHint"], false);
    assert_eq!(
        tool("party__create")["annotations"]["destructiveHint"],
        false
    );
    assert!(
        tool("party__delete")["description"]
            .as_str()
            .unwrap()
            .contains("approval")
    );
    assert!(
        tool("invoice__post")["description"]
            .as_str()
            .unwrap()
            .contains("approval_required")
    );
    // Search takes filter, order, limit, offset; get/delete take a required id.
    let s = &tool("lead__search")["inputSchema"]["properties"];
    for k in ["filter", "order", "limit", "offset"] {
        assert!(s[k].is_object(), "{k}");
    }
    assert_eq!(s["filter"]["items"]["properties"]["op"]["enum"][0], "eq");
    assert_eq!(tool("lead__get")["inputSchema"]["required"], json!(["id"]));
    assert_eq!(
        tool("lead__delete")["inputSchema"]["required"],
        json!(["id"])
    );
    // Field kinds: money is an integer, select an enum, ref a string naming its target.
    let create = &tool("lead__create")["inputSchema"];
    assert_eq!(
        create["properties"]["expected_revenue_minor"]["type"],
        "integer"
    );
    assert_eq!(
        create["properties"]["stage"],
        J::Null,
        "the governed stage is not settable on create"
    );
    let upd = &tool("lead__update")["inputSchema"];
    assert_eq!(upd["required"], json!(["id"]));
    let order = &tool("sale_order__create")["inputSchema"];
    assert_eq!(order["properties"]["party_id"]["type"], "string");
    assert!(
        order["properties"]["party_id"]["description"]
            .as_str()
            .unwrap()
            .contains("party")
    );
    // A tool the token cannot use is absent, for a user with fewer groups too.
    let sales = session(&r, "dev-sales").await;
    let sales_names: Vec<String> = tools_of(&r, "dev-sales", &sales)
        .await
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert!(!sales_names.iter().any(|n| n.starts_with("invoice__")));
    assert!(sales_names.contains(&"lead__list".to_string()));
}

#[tokio::test]
async fn stdio_and_http_list_the_same_tools() {
    let app = App::new();
    app.seed_demo();
    let r = obop_http::router(
        obop_http::HttpState::with_mcp(
            app.dispatcher.clone(),
            Arc::new(DevAuth),
            McpConfig::default(),
        ),
        None,
    );
    for token in ["dev-agent", "dev-sales", "dev-admin"] {
        let sid = session(&r, token).await;
        let http = tools_of(&r, token, &sid).await;
        let p = app::dev_principal(token).unwrap();
        let mut s = obop_mcp::Session::new();
        let line = obop_mcp::handle(
            &app.dispatcher,
            &p,
            &mut s,
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
        )
        .unwrap();
        let stdio: J = serde_json::from_str(&line).unwrap();
        assert_eq!(
            stdio["result"]["tools"].as_array().unwrap(),
            &http,
            "{token}"
        );
    }
}

#[tokio::test]
async fn unknown_and_hidden_tools_are_the_same_protocol_error() {
    let r = router();
    let sid = session(&r, "dev-sales").await;
    let hidden = rpc(
        &r,
        "dev-sales",
        &sid,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"invoice__list","arguments":{}}}),
    )
    .await
    .json();
    let missing = rpc(
        &r,
        "dev-sales",
        &sid,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"nothing__list","arguments":{}}}),
    )
    .await
    .json();
    assert_eq!(hidden["error"]["code"], -32602);
    assert_eq!(
        hidden["error"]["message"]
            .as_str()
            .unwrap()
            .replace("invoice__list", "X"),
        missing["error"]["message"]
            .as_str()
            .unwrap()
            .replace("nothing__list", "X")
    );
    let no_sep = rpc(
        &r,
        "dev-sales",
        &sid,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"plain","arguments":{}}}),
    )
    .await
    .json();
    assert_eq!(no_sep["error"]["code"], -32602);
}

#[tokio::test]
async fn tool_results_carry_structured_content_and_errors_are_tool_errors() {
    let r = router();
    let sid = session(&r, "dev-admin").await;
    let ok = call_tool(&r, "dev-admin", &sid, "party__list", json!({})).await;
    let res = &ok["result"];
    assert_eq!(res["isError"], false);
    assert_eq!(res["structuredContent"]["status"], "ok");
    assert!(
        !res["structuredContent"]["value"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let text: J = serde_json::from_str(res["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(&text, &res["structuredContent"]);
    // A validation error is a tool error with the server's message, not a protocol error.
    let bad = call_tool(&r, "dev-admin", &sid, "lead__create", json!({"title": 5})).await;
    assert!(bad["error"].is_null(), "{bad}");
    assert_eq!(bad["result"]["isError"], true);
    assert!(
        !bad["result"]["structuredContent"]["error"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    // obop__meta returns what /api/meta returns.
    let meta = call_tool(&r, "dev-admin", &sid, "obop__meta", json!({})).await;
    let (s, rest_meta) = {
        let resp = send(
            &r,
            Request::get("/api/meta")
                .header("host", HOST)
                .header("authorization", "Bearer dev-admin")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        (resp.status, resp.json())
    };
    assert_eq!(s, 200);
    assert_eq!(meta["result"]["structuredContent"], rest_meta);
}

#[tokio::test]
async fn mask_tokens_are_stable_and_restorable_within_a_session_and_isolated_between_sessions() {
    let r = router();
    let (_, parties) = rest(&r, "dev-admin", "/api/party/list", json!({})).await;
    let real_name = parties["value"][0]["name"].as_str().unwrap().to_string();
    let first_id = parties["value"][0]["id"].as_str().unwrap().to_string();

    let s1 = session(&r, "dev-agent").await;
    let a = call_tool(&r, "dev-agent", &s1, "party__list", json!({})).await;
    let b = call_tool(&r, "dev-agent", &s1, "party__list", json!({})).await;
    let rows_a = a["result"]["structuredContent"]["value"].clone();
    assert_eq!(
        rows_a, b["result"]["structuredContent"]["value"],
        "tokens differ between calls"
    );
    let token = rows_a[0]["name"].as_str().unwrap().to_string();
    assert_ne!(token, real_name);
    assert!(token.starts_with("NAME_"), "{token}");
    // get returns the same token for the same record.
    let got = call_tool(&r, "dev-agent", &s1, "party__get", json!({"id": first_id})).await;
    assert_eq!(got["result"]["structuredContent"]["value"]["name"], token);
    // Restorable: send the token back in input and the real value is stored.
    let made = call_tool(
        &r,
        "dev-agent",
        &s1,
        "party__create",
        json!({"name": token}),
    )
    .await;
    let new_id = made["result"]["structuredContent"]["value"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (_, stored) = rest(&r, "dev-admin", "/api/party/get", json!({"id": new_id})).await;
    assert_eq!(stored["value"]["name"], real_name);

    // Another session of the same token does not know that token: it stays literal text.
    let s2 = session(&r, "dev-agent").await;
    let made2 = call_tool(
        &r,
        "dev-agent",
        &s2,
        "party__create",
        json!({"name": token}),
    )
    .await;
    let id2 = made2["result"]["structuredContent"]["value"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (_, stored2) = rest(&r, "dev-admin", "/api/party/get", json!({"id": id2})).await;
    assert_eq!(stored2["value"]["name"], token);
}

#[tokio::test]
async fn an_agent_proposes_a_delete_a_human_approves_once() {
    let r = router();
    let (s, lead) = rest(
        &r,
        "dev-admin",
        "/api/lead/create",
        json!({"title": "Throwaway lead"}),
    )
    .await;
    assert_eq!(s, 200, "{lead}");
    let lead_id = lead["value"]["id"].as_str().unwrap().to_string();

    let sid = session(&r, "dev-agent").await;
    let proposed = call_tool(
        &r,
        "dev-agent",
        &sid,
        "lead__delete",
        json!({"id": lead_id}),
    )
    .await;
    let res = &proposed["result"];
    assert_eq!(res["isError"], false);
    assert_eq!(res["structuredContent"]["status"], "approval_required");
    assert!(
        res["structuredContent"]["instruction"]
            .as_str()
            .unwrap()
            .contains("human")
    );
    let plan = res["structuredContent"]["plan_id"]
        .as_str()
        .unwrap()
        .to_string();
    // Nothing ran yet.
    let (s, _) = rest(&r, "dev-admin", "/api/lead/get", json!({"id": lead_id})).await;
    assert_eq!(s, 200);
    // The agent cannot approve its own plan (REST: 404, same as unknown).
    assert_eq!(
        rest(&r, "dev-agent", &format!("/api/approve/{plan}"), json!({}))
            .await
            .0,
        404
    );
    // A human approves through REST: it runs, once.
    let (s, b) = rest(&r, "dev-admin", &format!("/api/approve/{plan}"), json!({})).await;
    assert_eq!(s, 200, "{b}");
    let (s, _) = rest(&r, "dev-admin", "/api/lead/get", json!({"id": lead_id})).await;
    assert_eq!(s, 400, "the record is gone");
    assert_eq!(
        rest(&r, "dev-admin", &format!("/api/approve/{plan}"), json!({}))
            .await
            .0,
        404
    );
}

async fn get_public(r: &axum::Router, path: &str) -> Resp {
    send(
        r,
        Request::get(path)
            .header("host", HOST)
            .header("x-forwarded-proto", "https")
            .body(Body::empty())
            .unwrap(),
    )
    .await
}

#[tokio::test]
async fn discovery_documents_are_public_honest_and_consistent() {
    let r = router();
    // RFC 9728 metadata: no authorization server is claimed.
    let prm = get_public(&r, "/.well-known/oauth-protected-resource").await;
    assert_eq!(prm.status, 200);
    assert_eq!(prm.header("access-control-allow-origin"), "*");
    let prm = prm.json();
    assert_eq!(prm["resource"], "https://inst.test");
    assert!(prm.get("authorization_servers").is_none());
    assert_eq!(prm["bearer_methods_supported"], json!(["header"]));
    assert_eq!(
        prm["x-obop-token-endpoints"]["agent_token"],
        "https://inst.test/api/auth/agent-token"
    );
    let path = get_public(&r, "/.well-known/oauth-protected-resource/mcp")
        .await
        .json();
    assert_eq!(path["resource"], "https://inst.test/mcp");

    // Server card, at the well-known path and at the canonical one.
    let card = get_public(&r, "/.well-known/mcp/server-card.json").await;
    assert_eq!(card.status, 200);
    let card = card.json();
    assert_eq!(card["name"], "inst.test/obop");
    assert_eq!(card["remotes"][0]["type"], "streamable-http");
    assert_eq!(card["remotes"][0]["url"], "https://inst.test/mcp");
    assert_eq!(
        card["remotes"][0]["supportedProtocolVersions"],
        json!(["2025-11-25", "2025-06-18", "2025-03-26"])
    );
    assert!(card["$comment"].as_str().unwrap().contains("incubation"));
    assert_eq!(card["endpoint"], "https://inst.test/mcp");
    assert_eq!(card["capabilities"]["tools"]["listChanged"], false);
    let canonical = get_public(&r, "/mcp/server-card").await;
    assert_eq!(
        canonical.header("content-type"),
        "application/mcp-server-card+json"
    );
    assert_eq!(canonical.json(), card);
    let catalog = get_public(&r, "/.well-known/ai-catalog.json").await.json();
    assert_eq!(
        catalog["entries"][0]["url"],
        "https://inst.test/mcp/server-card"
    );

    // The OpenAPI document and the /api/meta Link header advertise the endpoint.
    let api = get_public(&r, "/api/openapi.json").await.json();
    assert_eq!(api["x-mcp"]["url"], "https://inst.test/mcp");
    assert_eq!(api["x-mcp"]["transport"], "streamable-http");
    let meta = send(
        &r,
        Request::get("/api/meta")
            .header("host", HOST)
            .header("authorization", "Bearer dev-agent")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let link = meta.header("link");
    assert!(
        link.contains("<http://inst.test/mcp>; rel=\"mcp\""),
        "{link}"
    );
    assert!(link.contains("server-card.json>; rel=\"mcp-server-card\""));
}

#[tokio::test]
async fn the_public_url_setting_wins_over_the_host_header() {
    let r = router_with(McpConfig {
        public_url: Some("https://acme.example.com".into()),
        ..McpConfig::default()
    });
    let card = get_public(&r, "/.well-known/mcp/server-card.json")
        .await
        .json();
    assert_eq!(card["remotes"][0]["url"], "https://acme.example.com/mcp");
    let none = post_raw(&r, None, None, "{}".into(), &[]).await;
    assert!(
        none.header("www-authenticate")
            .contains("https://acme.example.com/.well-known/oauth-protected-resource")
    );
}
