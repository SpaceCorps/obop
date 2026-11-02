//! Field limits (D90) over HTTP and MCP against a real `App`: the 400 with its code, hint, field
//! and numbers on create, update and `bulk_create`, the rule for a record stored above a lowered
//! limit, the default on the built-in models, and what `GET /api/meta`, the OpenAPI document and
//! the MCP tool schemas say. The package under test is installed through `module.install` like
//! any other.

use crate::app::App;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use obop_auth::{Auth, AuthConfig};
use serde_json::{Value as J, json};
use std::sync::Arc;
use tower::ServiceExt;

const PW: &str = "correct horse battery";

/// A package with a field of every kind that takes a limit, and one of each without.
const LIM: &str = r#"
[module]
id = "lim"
version = "0.1.0"
name = "Lim"
description = "Limits"
license = "MIT"
requires_core = "^0.2.1"

[[model]]
name = "lim_note"
description = "A note"
read_group = "base.user"
write_group = "base.user"

  [[model.field]]
  name = "title"
  description = "Title"
  class = "none"
  max_length = 2000
  min_length = 1

  [[model.field]]
  name = "body"
  description = "Body"
  class = "none"
  kind = "longtext"

  [[model.field]]
  name = "plain"
  description = "Plain text with no limit declared"
  class = "none"

  [[model.field]]
  name = "qty"
  description = "Quantity"
  class = "none"
  kind = "number"
  min = 0
  max = 100

  [[model.field]]
  name = "price"
  description = "Price in minor units"
  class = "none"
  kind = "money"
  min = 0
  max = 100000

  [[model.field]]
  name = "rate"
  description = "Rate"
  class = "none"
  kind = "decimal"
  precision = 12
  scale = 7
  min = "0.1000000"
  max = "0.9000000"
"#;

struct Rig {
    r: axum::Router,
    app: App,
    token: String,
}

async fn send(r: &axum::Router, method: &str, token: &str, path: &str, body: J) -> (u16, J) {
    let req = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = r.clone().oneshot(req).await.unwrap();
    let status = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(J::Null))
}

impl Rig {
    /// A server with a user who may do everything and no package installed.
    async fn bare() -> Rig {
        let app = App::new();
        app.seed_demo();
        let auth = Arc::new(Auth::with_test_config(
            app.store.backend(),
            AuthConfig {
                default_require_admins: false,
                ..AuthConfig::insecure_dev()
            },
        ));
        auth.create_user("root", PW, &app.all_groups()).unwrap();
        let r = obop_http::router(
            obop_http::HttpState::new(app.dispatcher.clone(), auth.clone()),
            None,
        );
        let (s, b) = send(
            &r,
            "POST",
            "",
            "/api/auth/login",
            json!({"username": "root", "password": PW}),
        )
        .await;
        assert_eq!(s, 200, "{b}");
        let token = b["token"].as_str().unwrap().to_string();
        Rig { r, app, token }
    }
    /// As `bare`, with the package `lim` installed.
    async fn new() -> Rig {
        let rig = Rig::bare().await;
        let (s, b) = rig
            .post("/api/module/install", json!({"manifest": LIM}))
            .await;
        assert_eq!(s, 200, "{b}");
        rig
    }
    async fn post(&self, path: &str, body: J) -> (u16, J) {
        send(&self.r, "POST", &self.token, path, body).await
    }
    async fn get(&self, path: &str) -> J {
        let (s, b) = send(&self.r, "GET", &self.token, path, J::Null).await;
        assert_eq!(s, 200, "{path}: {b}");
        b
    }
    async fn create(&self, model: &str, body: J) -> (u16, J) {
        self.post(&format!("/api/{model}/create"), body).await
    }
    async fn count(&self, model: &str) -> u64 {
        let (s, b) = self
            .post(&format!("/api/{model}/search"), json!({"limit": 1}))
            .await;
        assert_eq!(s, 200, "{b}");
        b["value"]["total"].as_u64().unwrap()
    }
}

fn error(b: &J) -> String {
    b["error"].as_str().unwrap_or_default().to_string()
}

#[tokio::test]
async fn rest_refuses_a_value_outside_a_limit_with_a_400_that_names_the_field_and_the_numbers() {
    let rig = Rig::new().await;
    let long = "x".repeat(2001);
    let (s, b) = rig.create("lim_note", json!({"title": long})).await;
    assert_eq!(s, 400, "{b}");
    assert_eq!(
        error(&b),
        "lim_note.title must be at most 2000 characters; this has 2001"
    );
    assert_eq!(b["code"], "limit_violated");
    assert!(b["hint"].as_str().unwrap().contains("GET /api/meta"), "{b}");
    // 2,000 characters are accepted (the limit is inclusive), counted as characters.
    let (s, b) = rig
        .create("lim_note", json!({"title": "å".repeat(2000)}))
        .await;
    assert_eq!(s, 200, "{b}");
    let (s, b) = rig
        .create("lim_note", json!({"title": "\u{1F600}".repeat(2001)}))
        .await;
    assert_eq!(s, 400, "{b}");
    assert!(error(&b).ends_with("at most 2000 characters; this has 2001"));

    for (field, value, want) in [
        (
            "title",
            json!(""),
            "must be at least 1 character; this has 0",
        ),
        ("qty", json!(-1), "must be at least 0; this is -1"),
        ("qty", json!(101), "must be at most 100; this is 101"),
        (
            "price",
            json!(100_001),
            "must be at most 100000; this is 100001",
        ),
        (
            "rate",
            json!("0.0999999"),
            "must be at least 0.1000000; this is 0.0999999",
        ),
        (
            "rate",
            json!("0.9000001"),
            "must be at most 0.9000000; this is 0.9000001",
        ),
    ] {
        let (s, b) = rig
            .create("lim_note", json!({"title": "t", field: value}))
            .await;
        assert_eq!(s, 400, "{field}: {b}");
        assert!(
            error(&b).starts_with(&format!("lim_note.{field} ")) && error(&b).contains(want),
            "{field}: {b}"
        );
        assert_eq!(b["code"], "limit_violated", "{field}");
    }
    // Nothing was stored by any of the refused calls.
    assert_eq!(rig.count("lim_note").await, 1);

    // The inclusive ends are fine.
    for body in [
        json!({"title": "t", "qty": 0, "price": 0, "rate": "0.1000000"}),
        json!({"title": "t", "qty": 100, "price": 100_000, "rate": "0.9000000"}),
    ] {
        let (s, b) = rig.create("lim_note", body).await;
        assert_eq!(s, 200, "{b}");
    }
}

#[tokio::test]
async fn rest_update_holds_a_changed_value_to_the_limit_and_spares_an_unchanged_one() {
    let rig = Rig::new().await;
    let (_, b) = rig
        .create("lim_note", json!({"title": "ok", "qty": 5}))
        .await;
    let id = b["value"]["id"].as_str().unwrap().to_string();
    let (s, b) = rig
        .post("/api/lim_note/update", json!({"id": id, "qty": 101}))
        .await;
    assert_eq!(
        (s, b["code"].as_str()),
        (400, Some("limit_violated")),
        "{b}"
    );
    assert_eq!(error(&b), "lim_note.qty must be at most 100; this is 101");

    // Behind the commands' back: a record from before the limit was lowered.
    let patch = [
        ("title".to_string(), json!("t".repeat(3000))),
        ("qty".to_string(), json!(5000)),
    ]
    .into_iter()
    .collect();
    assert!(
        rig.app
            .store
            .backend()
            .update("lim_note", &id, &patch)
            .unwrap()
    );
    // Another field can be changed, with or without the stored values sent back with it.
    let (s, b) = rig
        .post("/api/lim_note/update", json!({"id": id, "plain": "edited"}))
        .await;
    assert_eq!(s, 200, "{b}");
    let (s, b) = rig
        .post(
            "/api/lim_note/update",
            json!({"id": id, "title": "t".repeat(3000), "qty": 5000, "plain": "again"}),
        )
        .await;
    assert_eq!(s, 200, "{b}");
    // A changed value is held to the limit, however far above it the stored one was.
    let (s, b) = rig
        .post(
            "/api/lim_note/update",
            json!({"id": id, "title": "u".repeat(3000)}),
        )
        .await;
    assert_eq!(s, 400, "{b}");
    assert_eq!(
        error(&b),
        "lim_note.title must be at most 2000 characters; this has 3000"
    );
}

#[tokio::test]
async fn rest_bulk_create_names_the_row_and_csv_goes_through_the_same_check() {
    let rig = Rig::new().await;
    let (s, b) = rig
        .post(
            "/api/lim_note/bulk_create",
            json!({"rows": [
                {"title": "fine"},
                {"title": "x".repeat(2001)},
                {"title": "fine", "qty": 101},
            ]}),
        )
        .await;
    assert_eq!(s, 400, "{b}");
    let e = error(&b);
    assert!(
        e.contains("2 of 3 row(s) are invalid")
            && e.contains("row 2")
            && e.contains("lim_note.title must be at most 2000 characters; this has 2001")
            && e.contains("row 3")
            && e.contains("lim_note.qty must be at most 100; this is 101"),
        "{e}"
    );
    assert_eq!(rig.count("lim_note").await, 0);

    // Partial mode: the errors come as a list with the row number and no code in the text.
    let (s, b) = rig
        .post(
            "/api/lim_note/bulk_create",
            json!({"mode": "partial", "rows": [
                {"title": "fine"},
                {"title": "x".repeat(2001)},
            ]}),
        )
        .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["value"]["created"], 1, "{b}");
    assert_eq!(b["value"]["errors"][0]["row"], 2);
    assert_eq!(
        b["value"]["errors"][0]["error"],
        "lim_note.title must be at most 2000 characters; this has 2001"
    );

    let csv = format!("title,qty\nok,1\n{},1\nok,500\n", "y".repeat(2001));
    let (s, b) = rig
        .post("/api/lim_note/bulk_create", json!({"csv": csv}))
        .await;
    assert_eq!(s, 400, "{b}");
    assert!(
        error(&b).contains("row 2") && error(&b).contains("row 3"),
        "{b}"
    );
    assert_eq!(rig.count("lim_note").await, 1);
}

#[tokio::test]
async fn the_built_in_models_have_the_default_length() {
    let rig = Rig::new().await;
    let (s, b) = rig.create("party", json!({"name": "a".repeat(2048)})).await;
    assert_eq!(s, 200, "{b}");
    let (s, b) = rig.create("party", json!({"name": "a".repeat(2049)})).await;
    assert_eq!(s, 400, "{b}");
    assert_eq!(
        error(&b),
        "party.name must be at most 2048 characters; this has 2049"
    );
    // A longtext field of a built-in model has the larger default.
    let (s, b) = rig
        .create("party", json!({"name": "n", "notes": "z".repeat(100_000)}))
        .await;
    assert_eq!(s, 200, "{b}");
    let (s, b) = rig
        .create("party", json!({"name": "n", "notes": "z".repeat(100_001)}))
        .await;
    assert_eq!(s, 400, "{b}");
    assert_eq!(
        error(&b),
        "party.notes must be at most 100000 characters; this has 100001"
    );
}

#[tokio::test]
async fn meta_says_the_limit_of_every_field_that_has_one() {
    let rig = Rig::new().await;
    let meta = rig.get("/api/meta").await;
    let fields = |model: &str| -> Vec<J> {
        for m in meta["modules"].as_array().unwrap() {
            for d in m["models"].as_array().unwrap() {
                if d["name"] == model {
                    return d["fields"].as_array().unwrap().clone();
                }
            }
        }
        panic!("no model {model}");
    };
    let field = |model: &str, name: &str| -> J {
        fields(model)
            .into_iter()
            .find(|f| f["name"] == name)
            .unwrap_or_else(|| panic!("no field {model}.{name}"))
    };
    let t = field("lim_note", "title");
    assert_eq!(
        (t["max_length"].clone(), t["min_length"].clone()),
        (json!(2000), json!(1))
    );
    // The default is stated, not left for the client to guess; no minimum is invented.
    let p = field("lim_note", "plain");
    assert_eq!(p["max_length"], 2048);
    assert!(p.get("min_length").is_none(), "{p}");
    assert_eq!(field("lim_note", "body")["max_length"], 100_000);
    let q = field("lim_note", "qty");
    assert_eq!((q["min"].clone(), q["max"].clone()), (json!(0), json!(100)));
    assert!(q.get("max_length").is_none(), "{q}");
    let m = field("lim_note", "price");
    assert_eq!(
        (m["min"].clone(), m["max"].clone()),
        (json!(0), json!(100_000))
    );
    // A decimal bound is an exact string at the scale of the field, never a number.
    let d = field("lim_note", "rate");
    assert_eq!(
        (d["min"].clone(), d["max"].clone()),
        (json!("0.1000000"), json!("0.9000000"))
    );
    // Every writable text field of every model says its maximum; no other kind does.
    let mut texts = 0;
    for m in meta["modules"].as_array().unwrap() {
        for d in m["models"].as_array().unwrap() {
            for f in d["fields"].as_array().unwrap() {
                let kind = f["kind"].as_str().unwrap();
                let system = f["computed"] == true || f["sequence"] == true;
                if matches!(kind, "text" | "longtext") && !system {
                    texts += 1;
                    let max = f["max_length"].as_u64();
                    assert!(
                        max.is_some_and(|n| (1..=1_000_000).contains(&n)),
                        "{}.{} has no maximum: {f}",
                        d["name"],
                        f["name"]
                    );
                } else {
                    assert!(
                        f.get("max_length").is_none() && f.get("min_length").is_none(),
                        "{}.{} should have no length: {f}",
                        d["name"],
                        f["name"]
                    );
                }
                if !matches!(kind, "number" | "money" | "decimal") {
                    assert!(f.get("min").is_none() && f.get("max").is_none(), "{f}");
                }
            }
        }
    }
    assert!(texts > 50, "{texts}");
}

#[tokio::test]
async fn the_openapi_document_has_the_keywords_on_inputs_and_not_on_records() {
    let rig = Rig::new().await;
    let doc = rig.get("/api/openapi.json").await;
    assert_eq!(doc["info"]["version"], "0.2.1");
    let input = |op: &str| -> J {
        doc["paths"][format!("/api/lim_note/{op}")]["post"]["requestBody"]["content"]
            ["application/json"]["schema"]
            .clone()
    };
    let c = input("create");
    let p = &c["properties"];
    assert_eq!(
        (
            p["title"]["maxLength"].clone(),
            p["title"]["minLength"].clone()
        ),
        (json!(2000), json!(1))
    );
    assert_eq!(p["body"]["maxLength"], 100_000);
    assert_eq!(p["plain"]["maxLength"], 2048);
    assert_eq!(
        (p["qty"]["minimum"].clone(), p["qty"]["maximum"].clone()),
        (json!(0), json!(100))
    );
    assert_eq!(
        (p["price"]["minimum"].clone(), p["price"]["maximum"].clone()),
        (json!(0), json!(100000))
    );
    // A decimal is a string: JSON Schema's `minimum` would be a number, so the bounds go in
    // extension keywords, as exact strings.
    assert!(p["rate"].get("minimum").is_none(), "{}", p["rate"]);
    assert_eq!(p["rate"]["x-obop-minimum"], "0.1000000");
    assert_eq!(p["rate"]["x-obop-maximum"], "0.9000000");
    // A declared limit is also said in words; the default is not repeated on every field.
    assert!(
        p["title"]["description"]
            .as_str()
            .unwrap()
            .ends_with("(from 1 character to 2000 characters)"),
        "{}",
        p["title"]
    );
    assert_eq!(
        p["plain"]["description"],
        "Plain text with no limit declared"
    );
    // The update input and the bulk rows carry them too.
    assert_eq!(input("update")["properties"]["title"]["maxLength"], 2000);
    let rows = &input("bulk_create")["properties"]["rows"]["items"]["properties"];
    assert_eq!(rows["title"]["maxLength"], 2000);
    // The record schema is what is stored, which may be above a limit set later: no keywords.
    let rec = &doc["components"]["schemas"]["lim_note"]["properties"];
    for f in ["title", "plain", "qty", "rate"] {
        for k in [
            "maxLength",
            "minLength",
            "minimum",
            "maximum",
            "x-obop-minimum",
        ] {
            assert!(rec[f].get(k).is_none(), "{f} {k}: {}", rec[f]);
        }
    }
    // Every writable text property of every create input has a maximum: the kinds come from
    // /api/meta, because a reference or an image is a string in the schema too.
    let meta = rig.get("/api/meta").await;
    let mut seen = 0;
    for m in meta["modules"].as_array().unwrap() {
        for d in m["models"].as_array().unwrap() {
            let name = d["name"].as_str().unwrap();
            let Some(item) = doc["paths"].get(format!("/api/{name}/create")) else {
                continue;
            };
            let props =
                &item["post"]["requestBody"]["content"]["application/json"]["schema"]["properties"];
            for f in d["fields"].as_array().unwrap() {
                let kind = f["kind"].as_str().unwrap();
                if !matches!(kind, "text" | "longtext")
                    || f["computed"] == true
                    || f["sequence"] == true
                {
                    continue;
                }
                let fname = f["name"].as_str().unwrap();
                assert_eq!(
                    props[fname]["maxLength"], f["max_length"],
                    "{name}.{fname}: {}",
                    props[fname]
                );
                seen += 1;
            }
        }
    }
    assert!(seen > 50, "{seen}");
}

// ---- MCP --------------------------------------------------------------------------------

const HOST: &str = "inst.test";

async fn mcp(r: &axum::Router, token: &str, session: Option<&str>, body: J) -> (u16, J, String) {
    let mut req = Request::post("/mcp")
        .header("host", HOST)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("authorization", format!("Bearer {token}"));
    if let Some(s) = session {
        req = req.header("mcp-session-id", s);
    }
    let resp = r
        .clone()
        .oneshot(req.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = resp.status().as_u16();
    let sid = resp
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(J::Null),
        sid,
    )
}

async fn mcp_session(rig: &Rig) -> String {
    let (s, b, sid) = mcp(
        &rig.r,
        &rig.token,
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion": "2025-11-25", "capabilities": {},
            "clientInfo": {"name": "test", "version": "0"}}}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["result"]["serverInfo"]["version"], "0.2.1", "{b}");
    let (s, _, _) = mcp(
        &rig.r,
        &rig.token,
        Some(&sid),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    )
    .await;
    assert_eq!(s, 202);
    sid
}

#[tokio::test]
async fn mcp_tool_schemas_carry_the_limits_and_a_call_over_the_limit_is_an_error_result() {
    let rig = Rig::new().await;
    let sid = mcp_session(&rig).await;
    // The list comes in pages.
    let mut tools: Vec<J> = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let params = cursor.as_ref().map_or(json!({}), |c| json!({"cursor": c}));
        let (s, list, _) = mcp(
            &rig.r,
            &rig.token,
            Some(&sid),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":params}),
        )
        .await;
        assert_eq!(s, 200, "{list}");
        tools.extend(list["result"]["tools"].as_array().unwrap().clone());
        match list["result"]["nextCursor"].as_str() {
            Some(c) => cursor = Some(c.to_string()),
            None => break,
        }
    }
    let tool = |name: &str| -> J {
        tools
            .iter()
            .find(|t| t["name"] == name)
            .unwrap_or_else(|| panic!("no tool {name}"))
            .clone()
    };
    let create = tool("lim_note__create");
    let p = &create["inputSchema"]["properties"];
    assert_eq!(
        (
            p["title"]["maxLength"].clone(),
            p["title"]["minLength"].clone()
        ),
        (json!(2000), json!(1))
    );
    assert_eq!(p["plain"]["maxLength"], 2048);
    assert_eq!(
        (p["qty"]["minimum"].clone(), p["qty"]["maximum"].clone()),
        (json!(0), json!(100))
    );
    assert_eq!(p["rate"]["x-obop-minimum"], "0.1000000");
    assert!(
        p["title"]["description"]
            .as_str()
            .unwrap()
            .contains("(from 1 character to 2000 characters)"),
        "{}",
        p["title"]
    );
    // An agent sees personal data as tokens such as PERSON_1, so a bound on the length of the
    // real value would refuse the token: such a field gets no keyword in the tool schema.
    let party = tool("party__create");
    assert!(
        party["inputSchema"]["properties"]["name"]
            .get("maxLength")
            .is_none(),
        "{}",
        party["inputSchema"]["properties"]["name"]
    );
    // A field of the same model that is not personal data does get it.
    assert_eq!(p["body"]["maxLength"], 100_000);

    // A call that breaks a limit comes back as an error result with the sentence.
    let (s, out, _) = mcp(
        &rig.r,
        &rig.token,
        Some(&sid),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{
            "name": "lim_note__create", "arguments": {"title": "x".repeat(2001)}}}),
    )
    .await;
    assert_eq!(s, 200, "{out}");
    assert_eq!(out["result"]["isError"], true, "{out}");
    let text = out["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("lim_note.title must be at most 2000 characters; this has 2001"),
        "{text}"
    );
    assert_eq!(rig.count("lim_note").await, 0);
    // And a call inside the limits works.
    let (_, ok, _) = mcp(
        &rig.r,
        &rig.token,
        Some(&sid),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{
            "name": "lim_note__create", "arguments": {"title": "x".repeat(2000)}}}),
    )
    .await;
    assert_ne!(ok["result"]["isError"], true, "{ok}");
    assert_eq!(rig.count("lim_note").await, 1);
}

#[tokio::test]
async fn installing_a_package_with_a_limit_that_cannot_hold_is_refused() {
    let rig = Rig::bare().await;
    for (from, to, want) in [
        (
            "max_length = 2000",
            "max_length = 0",
            "`max_length` must be at least 1",
        ),
        (
            "max_length = 2000",
            "max_length = 10001",
            "above the largest a text field may declare",
        ),
        (
            "min_length = 1",
            "min_length = 3000",
            "`min_length` 3000 is above `max_length` 2000",
        ),
        (
            "min = 0\n  max = 100\n",
            "min = 5\n  max = 4\n",
            "`min` 5 is above `max` 4",
        ),
        (
            "min = \"0.1000000\"",
            "min = 0.1",
            "a TOML float is not exact",
        ),
    ] {
        let bad = LIM.replace(from, to);
        assert_ne!(bad, LIM, "{from}");
        let (s, b) = rig
            .post("/api/module/install", json!({"manifest": bad}))
            .await;
        assert_eq!(s, 400, "{to}: {b}");
        assert!(error(&b).contains(want), "{to}: {b}");
    }
    // Nothing was installed by the refused ones; the good package still installs.
    assert!(!rig.app.dispatcher.has_model("lim_note"));
    let (s, b) = rig
        .post("/api/module/install", json!({"manifest": LIM}))
        .await;
    assert_eq!(s, 200, "{b}");
    assert!(rig.app.dispatcher.has_model("lim_note"));
}
