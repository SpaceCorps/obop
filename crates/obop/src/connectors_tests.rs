//! The connectors over HTTP (D54): the generated command API with real bearer tokens, the signed
//! webhook door, the OAuth redirect against a fake provider on loopback, and a scan of every
//! response and every stored record for the secrets that were put in.

use axum::{
    body::Body,
    http::{Method, Request},
};
use http_body_util::BodyExt;
use obop_commands::{Dispatcher, Store};
use obop_connectors::catalog::{OAuthDef, providers};
use obop_connectors::webhook;
use obop_connectors::{Config, Runtime, SecretBox};
use serde_json::{Value as J, json};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

use crate::app::DevAuth;

const HOOK_SECRET: &str = "http-test-signing-secret-0001";
const API_KEY: &str = "sk_live_http_test_key_9999";
const CLIENT_SECRET: &str = "oauth-client-secret-http-test";

struct Rig {
    router: axum::Router,
    store: Store,
    rt: Arc<Runtime>,
    seen: Mutex<Vec<String>>,
}

struct Resp {
    status: u16,
    body: String,
}

impl Resp {
    fn json(&self) -> J {
        serde_json::from_str(&self.body).unwrap_or_else(|_| panic!("not JSON: {}", self.body))
    }
}

fn rig_with(extra: Vec<obop_connectors::Provider>) -> Rig {
    let store = Store::new();
    let d = Arc::new(Dispatcher::new());
    base::register(&d, &store);
    crm::register(&d, &store);
    let mut all = providers();
    all.extend(extra);
    let rt = Runtime::with_providers(
        store.backend(),
        d.clone(),
        SecretBox::for_tests(),
        Config {
            allow_loopback: true,
            public_url: Some("http://127.0.0.1:8080".into()),
        },
        all,
        None,
    );
    connectors::register(&d, &store, &rt);
    base::seed(&store);
    let state = obop_http::HttpState::new(d, Arc::new(DevAuth));
    state.set_connectors(rt.clone());
    Rig {
        router: obop_http::router(state, None),
        store,
        rt,
        seen: Mutex::new(Vec::new()),
    }
}

fn rig() -> Rig {
    rig_with(vec![])
}

impl Rig {
    async fn send(&self, req: Request<Body>) -> Resp {
        let resp = self.router.clone().oneshot(req).await.unwrap();
        let status = resp.status().as_u16();
        let body = String::from_utf8_lossy(&resp.into_body().collect().await.unwrap().to_bytes())
            .to_string();
        self.seen.lock().unwrap().push(body.clone());
        Resp { status, body }
    }

    async fn api(&self, token: &str, cmd: &str, body: J) -> Resp {
        let (m, c) = cmd.split_once('.').unwrap();
        self.send(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/{m}/{c}"))
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
    }

    async fn ok(&self, token: &str, cmd: &str, body: J) -> J {
        let r = self.api(token, cmd, body).await;
        assert_eq!(r.status, 200, "{cmd}: {}", r.body);
        r.json()["value"].clone()
    }

    async fn hook(&self, id: &str, headers: &[(&str, String)], body: &[u8]) -> Resp {
        let mut b = Request::builder()
            .method(Method::POST)
            .uri(format!("/hooks/in/{id}"));
        for (k, v) in headers {
            b = b.header(*k, v);
        }
        self.send(b.body(Body::from(body.to_vec())).unwrap()).await
    }

    /// Every response so far and every stored record must be free of the secrets.
    fn assert_no_plaintext_secrets(&self, secrets: &[&str]) {
        for body in self.seen.lock().unwrap().iter() {
            for s in secrets {
                assert!(!body.contains(s), "{s} appeared in a response: {body}");
            }
        }
        let backend = self.store.backend();
        for table in [
            "connection",
            "consent",
            "provider_policy",
            "sync_run",
            "data_flow_log",
            "egress_policy",
            "connector_pseudonym",
            "connector_vault",
            "connector_extmap",
            "connector_origin",
            "connector_seen",
            "connector_oauth_state",
            "connector_key",
        ] {
            for (id, f) in backend.list(table).unwrap_or_default() {
                let text = serde_json::to_string(&f).unwrap();
                if table == "connector_key" {
                    continue; // the key of a database that has no OBOP_SECRET_KEY, by design
                }
                for s in secrets {
                    assert!(!text.contains(s), "{s} is stored in clear in {table} {id}");
                }
            }
        }
    }
}

fn signed(secret: &str, id: &str, ts: u64, body: &[u8]) -> Vec<(&'static str, String)> {
    vec![
        ("webhook-id", id.into()),
        ("webhook-timestamp", ts.to_string()),
        (
            "webhook-signature",
            webhook::sign_standard(secret, id, ts, body),
        ),
    ]
}

fn now() -> u64 {
    obop_connectors::util::now()
}

async fn enable(r: &Rig, provider: &str, extra: J) {
    let mut v =
        json!({"provider": provider, "enabled": true, "users_may_use": true, "dpa_recorded": true});
    v.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    r.ok("dev-admin", "provider_policy.create", v).await;
}

#[tokio::test]
async fn the_command_api_scopes_records_to_their_owner_and_holds_agents_to_proposals() {
    let r = rig();
    enable(&r, "webhook_out", json!({})).await;
    // Unauthenticated callers get nothing.
    let anon = r
        .send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/connection/list")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await;
    assert_eq!(anon.status, 401);
    let make = |name: &str| {
        json!({"name": name, "provider": "webhook_out",
        "config": json!({"url": "http://127.0.0.1:9/hook"}).to_string(),
        "credentials": json!({"signing_secret": HOOK_SECRET}).to_string(),
        "field_allowlist": json!({"party": ["name"]}).to_string()})
    };
    let mine = r.ok("dev-sales", "connection.create", make("mine")).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let theirs = r
        .ok("dev-admin", "connection.create", make("admin's"))
        .await["id"]
        .as_str()
        .unwrap()
        .to_string();
    // dev-sales sees its own; dev-admin sees both; neither sees a credential.
    let list = r.ok("dev-sales", "connection.list", json!({})).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], mine.as_str());
    assert_eq!(
        r.ok("dev-admin", "connection.list", json!({}))
            .await
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let other = r
        .api("dev-sales", "connection.get", json!({"id": theirs}))
        .await;
    assert_eq!(other.status, 400);
    assert_eq!(other.json()["error"], format!("no connection {theirs}"));
    // Credentials are denied fields in every reply, not values.
    let got = r
        .api("dev-sales", "connection.get", json!({"id": mine}))
        .await;
    assert!(
        got.json()["denied"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d == "connection.credentials")
    );
    assert!(got.json()["value"].get("credentials").is_none());
    // The agent token proposes; nothing happens until a human approves.
    let c = r
        .api("dev-agent", "connection.revoke", json!({"id": theirs}))
        .await;
    assert_eq!(c.status, 202);
    assert_eq!(c.json()["approval_required"], true);
    assert_eq!(
        r.store.field("connection", &theirs, "state"),
        Some(obop_commands::Value::text("draft"))
    );
    let plan = c.json()["plan_id"].as_str().unwrap().to_string();
    let ap = r
        .send(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/approve/{plan}"))
                .header("authorization", "Bearer dev-admin")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(ap.status, 200, "{}", ap.body);
    assert_eq!(
        r.store.field("connection", &theirs, "state"),
        Some(obop_commands::Value::text("revoked"))
    );
    // The generated metadata marks the admin-only pieces: a plain user does not see them.
    let meta = |t: &'static str| {
        let router = r.router.clone();
        async move {
            let resp = router
                .oneshot(
                    Request::builder()
                        .uri("/api/meta")
                        .header("authorization", format!("Bearer {t}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let v: J =
                serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            v["modules"]
                .as_array()
                .unwrap()
                .iter()
                .find(|m| m["id"] == "connectors")
                .unwrap()["models"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| m["name"].as_str().unwrap().to_string())
                .collect::<Vec<_>>()
        }
    };
    let admin_models = meta("dev-admin").await;
    let user_models = meta("dev-sales").await;
    assert!(
        admin_models.contains(&"processing_register".to_string())
            && admin_models.contains(&"egress_policy".to_string())
    );
    assert!(
        !user_models.contains(&"processing_register".to_string())
            && !user_models.contains(&"egress_policy".to_string())
            && !user_models.contains(&"provider_policy".to_string())
    );
    assert!(
        user_models.contains(&"connection".to_string())
            && user_models.contains(&"connector_provider".to_string())
    );
    r.assert_no_plaintext_secrets(&[HOOK_SECRET]);
}

#[tokio::test]
async fn the_webhook_door_checks_the_signature_the_age_and_the_size_before_anything_else() {
    let r = rig();
    enable(&r, "webhook_in", json!({})).await;
    let c = r
        .ok("dev-admin", "connection.create", json!({
            "name": "Form", "provider": "webhook_in",
            "config": json!({"target_model": "lead", "mapping": json!({"title": "title"}).to_string()}).to_string(),
            "credentials": json!({"signing_secret": HOOK_SECRET}).to_string(),
            "field_allowlist": json!({"lead": ["title"]}).to_string()}))
        .await;
    let id = c["id"].as_str().unwrap().to_string();
    r.ok("dev-admin", "consent.create", json!({"connection": id, "purpose": "Create leads from the form", "legal_basis": "contract"})).await;
    // Not connected yet: signed requests are refused with a reason, unsigned ones are 401.
    let body = br#"{"title":"Hello"}"#;
    let ts = now();
    let res = r
        .hook(&id, &signed(HOOK_SECRET, "e1", ts, body), body)
        .await;
    assert_eq!(res.status, 403, "{}", res.body);
    r.ok("dev-admin", "connection.connect", json!({"id": id}))
        .await;
    let res = r
        .hook(&id, &signed(HOOK_SECRET, "e1", ts, body), body)
        .await;
    assert_eq!(
        (res.status, res.json()["accepted"].as_u64()),
        (202, Some(1)),
        "{}",
        res.body
    );
    assert_eq!(r.store.count("lead"), 1);
    // A replay, a wrong secret, a missing header and a stale timestamp are all the same 401.
    let replay = r
        .hook(&id, &signed(HOOK_SECRET, "e1", ts, body), body)
        .await;
    let wrong = r
        .hook(
            &id,
            &signed("another-secret-another-secret", "e2", ts, body),
            body,
        )
        .await;
    let none = r.hook(&id, &[], body).await;
    let stale = r
        .hook(&id, &signed(HOOK_SECRET, "e3", ts - 3600, body), body)
        .await;
    for x in [&replay, &wrong, &none, &stale] {
        assert_eq!(
            (x.status, x.body.as_str()),
            (401, replay.body.as_str()),
            "{}",
            x.body
        );
    }
    assert_eq!(replay.json()["error"], "invalid signature");
    assert_eq!(r.store.count("lead"), 1);
    // An unknown connection and a wrong method.
    assert_eq!(
        r.hook("nope", &signed(HOOK_SECRET, "e4", ts, body), body)
            .await
            .status,
        404
    );
    let get = r
        .send(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/hooks/in/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(get.status, 405);
    // A body over the limit is refused before it is read as JSON.
    let big = vec![b'x'; 300 * 1024];
    assert_eq!(
        r.hook(&id, &signed(HOOK_SECRET, "e5", ts, &big), &big)
            .await
            .status,
        413
    );
    // A payload that does not map to the allowlist is a 422 with the reason, and creates nothing.
    ok_pause_and_remap(&r, &id).await;
    let body2 = br#"{"title":"x","mail":"a@b.test"}"#;
    let res = r
        .hook(&id, &signed(HOOK_SECRET, "e6", ts, body2), body2)
        .await;
    assert_eq!(res.status, 422, "{}", res.body);
    assert!(res.json()["error"].as_str().unwrap().contains("allowlist"));
    assert_eq!(r.store.count("lead"), 1);
    r.assert_no_plaintext_secrets(&[HOOK_SECRET]);
}

async fn ok_pause_and_remap(r: &Rig, id: &str) {
    r.ok("dev-admin", "connection.pause", json!({"id": id}))
        .await;
    r.ok("dev-admin", "connection.update", json!({"id": id, "config": json!({"target_model": "lead", "mapping": json!({"title": "title", "mail": "contact_email"}).to_string()}).to_string()})).await;
    r.ok("dev-admin", "connection.resume", json!({"id": id}))
        .await;
}

#[tokio::test]
async fn the_oauth_flow_runs_against_a_local_fake_provider_and_tokens_stay_sealed() {
    // The fake provider: it remembers the challenge of the code it issued and checks PKCE and the
    // client secret at the token endpoint, as a real one does.
    let codes = Arc::new(Mutex::new(HashMap::<String, String>::new()));
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let known = codes.clone();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { break };
            let mut buf = vec![0u8; 8192];
            let n = s.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let body = req.split("\r\n\r\n").nth(1).unwrap_or("");
            let f: HashMap<String, String> = url_pairs(body);
            let reply = match f.get("grant_type").map(String::as_str) {
                Some("authorization_code") => {
                    let ok = f["client_secret"] == CLIENT_SECRET
                        && f["client_id"] == "cid"
                        && known.lock().unwrap().get(&f["code"]).is_some_and(|c| {
                            *c == obop_connectors::oauth::challenge_of(&f["code_verifier"])
                        });
                    if ok {
                        r#"{"access_token":"at-1-secret","refresh_token":"rt-1-secret","expires_in":30}"#
                    } else {
                        "bad"
                    }
                }
                Some("refresh_token") if f["refresh_token"] == "rt-1-secret" => {
                    r#"{"access_token":"at-2-secret","expires_in":3600}"#
                }
                _ => "bad",
            };
            let (status, body) = if reply == "bad" {
                ("400 Bad Request", r#"{"error":"invalid_grant"}"#)
            } else {
                ("200 OK", reply)
            };
            let _ = s.write_all(format!("HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes());
        }
    });
    let mut fake = obop_connectors::catalog::provider("rest_api_pull").unwrap();
    fake.id = "fake_oauth";
    fake.name = "Fake OAuth provider";
    fake.auth_kind = "oauth2";
    fake.allowed_hosts = vec!["127.0.0.1"];
    fake.oauth = Some(OAuthDef {
        auth_url: Box::leak(format!("http://127.0.0.1:{port}/authorize").into_boxed_str()),
        token_url: Box::leak(format!("http://127.0.0.1:{port}/token").into_boxed_str()),
        scopes: "read",
    });
    let r = rig_with(vec![fake]);
    // Without the administrator's client, there is nothing to start.
    r.ok("dev-admin", "provider_policy.create", json!({"provider": "fake_oauth", "enabled": true, "users_may_use": true, "dpa_recorded": true})).await;
    let c = r
        .ok("dev-admin", "connection.create", json!({"name": "Calendar", "provider": "fake_oauth",
            "config": json!({"url": format!("http://127.0.0.1:{port}/events"), "target_model": "lead"}).to_string()}))
        .await;
    let id = c["id"].as_str().unwrap().to_string();
    let none = r
        .api("dev-admin", "connection.oauth_start", json!({"id": id}))
        .await;
    assert_eq!(none.status, 400);
    assert!(
        none.json()["error"]
            .as_str()
            .unwrap()
            .contains("client id and secret")
    );
    let pid = r.ok("dev-admin", "provider_policy.list", json!({})).await[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    r.ok(
        "dev-admin",
        "provider_policy.update",
        json!({"id": pid, "oauth_client_id": "cid", "oauth_client_secret": CLIENT_SECRET}),
    )
    .await;
    // The client secret is sealed at rest and never returned.
    let stored = r
        .store
        .backend()
        .get("provider_policy", &pid)
        .unwrap()
        .unwrap();
    assert!(
        stored["oauth_client_secret"]
            .as_str()
            .unwrap()
            .starts_with("obs1:")
    );
    let pol = r
        .api("dev-admin", "provider_policy.get", json!({"id": pid}))
        .await;
    assert!(!pol.body.contains(CLIENT_SECRET) && !pol.body.contains("obs1:"));

    let start = r
        .ok("dev-admin", "connection.oauth_start", json!({"id": id}))
        .await;
    let url = url::Url::parse(
        serde_json::from_str::<J>(start["result"].as_str().unwrap()).unwrap()["url"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let q: HashMap<String, String> = url.query_pairs().into_owned().collect();
    assert_eq!(
        (q["client_id"].as_str(), q["code_challenge_method"].as_str()),
        ("cid", "S256")
    );
    assert_eq!(q["redirect_uri"], "http://127.0.0.1:8080/oauth/callback");
    // The person agrees at the provider, which hands the browser a code bound to the challenge.
    codes
        .lock()
        .unwrap()
        .insert("code-1".into(), q["code_challenge"].clone());
    let cb = |state: &str, code: &str| {
        Request::builder()
            .uri(format!("/oauth/callback?state={state}&code={code}"))
            .body(Body::empty())
            .unwrap()
    };
    // A guessed state is refused; the real one works once.
    let bad = r.send(cb("guess", "code-1")).await;
    assert!(bad.body.contains("Not connected") && !bad.body.contains("guess"));
    let good = r.send(cb(&q["state"], "code-1")).await;
    assert!(
        good.body.contains("Connected") && !good.body.contains("at-1"),
        "{}",
        good.body
    );
    let again = r.send(cb(&q["state"], "code-1")).await;
    assert!(again.body.contains("Not connected"));
    let denied = r
        .send(
            Request::builder()
                .uri("/oauth/callback?error=access_denied&error_description=%3Cscript%3E")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert!(denied.body.contains("did not authorise") && !denied.body.contains("<script>"));
    // The tokens are sealed; the access token is refreshed when it is about to expire.
    let stored = r.store.backend().get("connection", &id).unwrap().unwrap();
    assert!(
        stored["oauth_tokens"]
            .as_str()
            .unwrap()
            .starts_with("obs1:")
    );
    let (conn, _) = r.rt.connection(&id).unwrap();
    assert_eq!(
        r.rt.access_token(&conn).unwrap(),
        "at-2-secret",
        "30 s left is inside the refresh margin"
    );
    let (conn, _) = r.rt.connection(&id).unwrap();
    assert_eq!(r.rt.access_token(&conn).unwrap(), "at-2-secret");
    r.assert_no_plaintext_secrets(&[CLIENT_SECRET, "at-1-secret", "rt-1-secret", "at-2-secret"]);
}

fn url_pairs(s: &str) -> HashMap<String, String> {
    url::form_urlencoded::parse(s.as_bytes())
        .into_owned()
        .collect()
}

#[tokio::test]
async fn an_api_key_never_appears_in_a_response_a_log_row_or_the_database() {
    let r = rig();
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { break };
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            let _ = s.write_all(
                b"HTTP/1.1 500 Oops\r\ncontent-length: 5\r\nconnection: close\r\n\r\nboom!",
            );
        }
    });
    enable(&r, "rest_api_pull", json!({})).await;
    let c = r
        .ok("dev-admin", "connection.create", json!({"name": "Pull", "provider": "rest_api_pull",
            "config": json!({"url": format!("http://127.0.0.1:{port}/x"), "target_model": "lead"}).to_string(),
            "credentials": json!({"api_key": API_KEY}).to_string(),
            "field_allowlist": json!({"lead": ["title"]}).to_string()}))
        .await;
    let id = c["id"].as_str().unwrap().to_string();
    r.ok("dev-admin", "consent.create", json!({"connection": id, "purpose": "Read leads from the partner", "legal_basis": "contract"})).await;
    r.ok("dev-admin", "connection.connect", json!({"id": id}))
        .await;
    let failed = r
        .api("dev-admin", "connection.sync", json!({"id": id}))
        .await;
    assert_eq!(failed.status, 400);
    assert!(failed.json()["error"].as_str().unwrap().contains("500"));
    for cmd in [
        "connection.get",
        "connection.list",
        "sync_run.list",
        "data_flow_log.list",
        "processing_register.list",
        "consent.list",
    ] {
        r.api("dev-admin", cmd, json!({"id": id})).await;
    }
    let _ = r
        .api("dev-admin", "connection.test", json!({"id": id}))
        .await;
    r.assert_no_plaintext_secrets(&[API_KEY]);
    // The hint shows at most the last four characters.
    let got = r.ok("dev-admin", "connection.get", json!({"id": id})).await;
    assert!(got["credentials_hint"].as_str().unwrap().ends_with("9999"));
}

#[tokio::test]
async fn a_flood_at_the_webhook_door_is_limited_before_any_signature_work() {
    let r = rig();
    let mut limited = 0;
    for _ in 0..260 {
        let res = r.hook("flood-target", &[], b"{}").await;
        assert!(matches!(res.status, 404 | 429), "{}", res.status);
        if res.status == 429 {
            limited += 1;
        }
    }
    assert!(limited > 0, "the door must start refusing a flood");
}

/// A minimal MCP client over the router: initialize, then call tools as the agent token.
async fn mcp(r: &Rig, body: J, session: Option<&str>) -> (u16, Option<String>, J) {
    let mut b = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("host", "inst.test")
        .header("authorization", "Bearer dev-agent")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream");
    if let Some(s) = session {
        b = b.header("mcp-session-id", s);
    }
    let resp = r
        .router
        .clone()
        .oneshot(b.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = resp.status().as_u16();
    let sid = resp
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).to_string();
    r.seen.lock().unwrap().push(text.clone());
    (status, sid, serde_json::from_str(&text).unwrap_or(J::Null))
}

#[tokio::test]
async fn agents_get_the_generated_tools_and_the_connect_family_needs_approval() {
    let r = rig();
    enable(&r, "webhook_out", json!({})).await;
    let (st, sid, _) = mcp(
        &r,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}),
        None,
    )
    .await;
    assert_eq!(st, 200);
    let sid = sid.unwrap();
    mcp(
        &r,
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        Some(&sid),
    )
    .await;
    let (_, _, list) = mcp(
        &r,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        Some(&sid),
    )
    .await;
    let tools: Vec<String> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    for want in [
        "connection__create",
        "connection__connect",
        "connection__pause",
        "connection__revoke",
        "connection__sync",
        "consent__create",
        "consent__withdraw",
        "privacy_preview__render",
        "processing_register__export_csv",
        "connector_provider__list",
        "data_flow_log__search",
    ] {
        assert!(tools.iter().any(|t| t == want), "{want} is generated");
    }
    // Generated, not hand-written: no tool exists for the engine internals or the secret tables.
    assert!(
        !tools.iter().any(|t| t.contains("vault")
            || t.contains("pseudonym")
            || t.contains("oauth_callback"))
    );
    // Create a connection through the tool: the credentials go in, never out.
    let (_, _, made) = mcp(
        &r,
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"connection__create","arguments":{
            "name":"Agent hook","provider":"webhook_out",
            "credentials": json!({"signing_secret": HOOK_SECRET}).to_string(),
            "config": json!({"url":"http://127.0.0.1:9/hook"}).to_string(),
            "field_allowlist": json!({"party":["name"]}).to_string()}}}),
        Some(&sid),
    )
    .await;
    assert_ne!(made["result"]["isError"], true, "{made}");
    let id = made["result"]["structuredContent"]["value"]["id"]
        .as_str()
        .or_else(|| made["result"]["structuredContent"]["id"].as_str())
        .unwrap_or_else(|| panic!("{made}"))
        .to_string();
    // Connecting is only a proposal for an agent.
    let (_, _, c) = mcp(
        &r,
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"connection__connect","arguments":{"id":id}}}),
        Some(&sid),
    )
    .await;
    assert_eq!(
        c["result"]["structuredContent"]["status"], "approval_required",
        "{c}"
    );
    assert_eq!(
        r.store.field("connection", &id, "state"),
        Some(obop_commands::Value::text("draft"))
    );
    // Reading it back shows the field as withheld, never its value.
    let (_, _, got) = mcp(
        &r,
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"connection__get","arguments":{"id":id}}}),
        Some(&sid),
    )
    .await;
    assert!(
        got.to_string().contains("connection.credentials"),
        "named as withheld: {got}"
    );
    r.assert_no_plaintext_secrets(&[HOOK_SECRET]);
}

#[tokio::test]
async fn the_purge_command_applies_retention_to_a_real_database() {
    let path = std::env::temp_dir().join(format!(
        "obop-connectors-purge-{}.sqlite",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let url = format!("sqlite://{}", path.display());
    {
        let store = Store::open(&url).unwrap();
        let b = store.backend();
        let old = obop_connectors::util::iso(now() - 400 * 86_400);
        let fresh = obop_connectors::util::iso(now());
        for (id, at) in [("old", &old), ("new", &fresh)] {
            b.insert(
                "data_flow_log",
                id,
                json!({"connection": "gone", "at": at, "outcome": "released"})
                    .as_object()
                    .unwrap(),
            )
            .unwrap();
            b.insert(
                "sync_run",
                id,
                json!({"connection": "gone", "started_at": at, "status": "succeeded"})
                    .as_object()
                    .unwrap(),
            )
            .unwrap();
        }
    }
    let opts = crate::Opts {
        db: Some(url.clone()),
        token: None,
        demo: false,
        dev_auth: false,
        public_site: false,
        trust_proxy: false,
        admin_prefix: "/admin".into(),
    };
    assert!(crate::connectors_purge(&opts).is_ok());
    let b = Store::open(&url).unwrap().backend();
    assert!(
        b.get("data_flow_log", "old").unwrap().is_none()
            && b.get("data_flow_log", "new").unwrap().is_some()
    );
    assert!(
        b.get("sync_run", "old").unwrap().is_none() && b.get("sync_run", "new").unwrap().is_some()
    );
    let _ = std::fs::remove_file(&path);
}
