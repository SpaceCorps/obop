//! D70 over HTTP, MCP and the CLI: `bulk_create` on a real model of the built-in modules.
//! Named acceptance tests: `rest_*`, `agents_*`, `mcp_*`, `cli_*`.

use crate::mfa_tests::{post, rig, send, token};
use serde_json::{Value as J, json};

async fn party_count(r: &axum::Router, t: &str) -> usize {
    let (s, b) = post(r, Some(t), "/api/party/search", json!({"limit": 1})).await;
    assert_eq!(s, 200, "{b}");
    b["value"]["total"].as_u64().unwrap() as usize
}

fn names(n: usize, prefix: &str) -> J {
    J::Array(
        (0..n)
            .map(|i| json!({"name": format!("{prefix} {i}")}))
            .collect(),
    )
}

#[tokio::test]
async fn rest_bulk_create_checks_creates_and_replays() {
    let rig = rig(false);
    let r = &rig.r;
    let ada = token(r, "ada").await;
    let before = party_count(r, &ada).await;

    // A dry run writes nothing and reports.
    let (s, b) = post(
        r,
        Some(&ada),
        "/api/party/bulk_create",
        json!({"dry_run": true, "mode": "partial", "rows": [{"name": "A"}, {"email": "no name"}]}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(
        (
            b["value"]["valid"].clone(),
            b["value"]["failed"].clone(),
            b["value"]["errors"][0]["row"].clone()
        ),
        (json!(1), json!(1), json!(2))
    );
    assert_eq!(party_count(r, &ada).await, before);

    // One bad row in the default mode: a 400 that names it, nothing written.
    let (s, b) = post(
        r,
        Some(&ada),
        "/api/party/bulk_create",
        json!({"rows": [{"name": "A"}, {"email": "no name"}]}),
    )
    .await;
    assert_eq!(s, 400, "{b}");
    assert!(b["error"].as_str().unwrap().contains("row 2"), "{b}");
    assert_eq!(party_count(r, &ada).await, before);

    // Rows, then CSV, then partial.
    let (s, b) = post(
        r,
        Some(&ada),
        "/api/party/bulk_create",
        json!({"rows": names(3, "Rows")}),
    )
    .await;
    assert_eq!((s, b["value"]["created"].clone()), (200, json!(3)), "{b}");
    let (s, b) = post(
        r,
        Some(&ada),
        "/api/party/bulk_create",
        json!({"csv": "\u{feff}name;email\r\nCsv One;one@example.test\r\n\"Csv, Two\";\r\n"}),
    )
    .await;
    assert_eq!((s, b["value"]["created"].clone()), (200, json!(2)), "{b}");
    let (s, b) = post(
        r,
        Some(&ada),
        "/api/party/bulk_create",
        json!({"mode": "partial", "csv": "name,email\nOk,\n\n,x@example.test\n"}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(party_count(r, &ada).await, before + 3 + 2 + 1);

    // The same key again: the first answer, no new rows.
    let call = json!({"idempotency_key": "import-2026-10", "rows": names(2, "Keyed")});
    let (_, first) = post(r, Some(&ada), "/api/party/bulk_create", call.clone()).await;
    let (_, again) = post(r, Some(&ada), "/api/party/bulk_create", call).await;
    assert_eq!(
        (
            first["value"]["replayed"].clone(),
            again["value"]["replayed"].clone()
        ),
        (json!(false), json!(true))
    );
    assert_eq!(first["value"]["ids"], again["value"]["ids"]);
    assert_eq!(party_count(r, &ada).await, before + 8);

    // Limits and who may.
    let (s, b) = post(
        r,
        Some(&ada),
        "/api/party/bulk_create",
        json!({"rows": names(1001, "Too many")}),
    )
    .await;
    assert_eq!(s, 400, "{b}");
    assert!(b["error"].as_str().unwrap().contains("at most 1000"));
    assert_eq!(
        post(r, None, "/api/party/bulk_create", json!({"rows": []}))
            .await
            .0,
        401
    );
    rig.auth
        .create_user("nogroups", crate::mfa_tests::PW, &[])
        .unwrap();
    let none = token(r, "nogroups").await;
    assert_eq!(
        post(
            r,
            Some(&none),
            "/api/party/bulk_create",
            json!({"rows": []})
        )
        .await
        .0,
        404
    );
    // A file or image cell is not importable.
    let (s, b) = post(
        r,
        Some(&ada),
        "/api/party/bulk_create",
        json!({"csv": "name,photo\nA,abc\n"}),
    )
    .await;
    assert_eq!(s, 400, "{b}");
    assert!(
        b["error"].as_str().unwrap().contains("image or file"),
        "{b}"
    );
}

#[tokio::test]
async fn agents_bulk_create_with_the_scope_of_create_and_stay_masked() {
    let rig = rig(false);
    let r = &rig.r;
    let ada = token(r, "ada").await;
    let (_, b) = post(
        r,
        Some(&ada),
        "/api/auth/agent-token",
        json!({"scopes": ["base.user"]}),
    )
    .await;
    let agent = b["token"].as_str().unwrap().to_string();
    let (s, b) = post(
        r,
        Some(&agent),
        "/api/party/bulk_create",
        json!({"rows": [{"name": "Agent Made", "email": "agent.made@example.test"}]}),
    )
    .await;
    assert_eq!((s, b["value"]["created"].clone()), (200, json!(1)), "{b}");
    // What it reads back is masked, exactly as for any other create.
    let id = b["value"]["ids"][0].as_str().unwrap();
    let (_, got) = post(r, Some(&agent), "/api/party/get", json!({"id": id})).await;
    assert!(
        !got.to_string().contains("agent.made@example.test"),
        "{got}"
    );
    let (_, human) = post(r, Some(&ada), "/api/party/get", json!({"id": id})).await;
    assert_eq!(human["value"]["email"], "agent.made@example.test");
    // An agent scoped to something else cannot.
    rig.auth
        .create_user("crmonly", crate::mfa_tests::PW, &["crm.user".into()])
        .unwrap();
    let crm = token(r, "crmonly").await;
    let (_, b) = post(
        r,
        Some(&crm),
        "/api/auth/agent-token",
        json!({"scopes": ["crm.user"]}),
    )
    .await;
    let narrow = b["token"].as_str().unwrap();
    assert_eq!(
        post(
            r,
            Some(narrow),
            "/api/party/bulk_create",
            json!({"rows": []})
        )
        .await
        .0,
        404
    );
}

#[tokio::test]
async fn mcp_and_openapi_describe_bulk_create_for_every_model_that_can_create() {
    let rig = rig(false);
    let r = &rig.r;
    let ada = token(r, "ada").await;
    let (s, doc) = send(r, "GET", Some(&ada), "/api/openapi.json", json!({})).await;
    assert_eq!(s, 200);
    let op = &doc["paths"]["/api/party/bulk_create"]["post"];
    assert!(
        op.is_object(),
        "{}",
        doc["paths"]
            .as_object()
            .unwrap()
            .keys()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(",")
    );
    assert_eq!(
        op["requestBody"]["content"]["application/json"]["schema"]["properties"]["rows"]["maxItems"],
        1000
    );
    let member = obop_commands::Principal::user("ada", &["base.user"]);
    let app = crate::app::App::new();
    let tools = obop_mcp::schema::tools(&app.dispatcher, &member);
    let tool = tools
        .iter()
        .find(|t| t["name"] == "party__bulk_create")
        .expect("party__bulk_create");
    let props = &tool["inputSchema"]["properties"];
    for k in ["rows", "csv", "mode", "dry_run", "idempotency_key"] {
        assert!(props[k].is_object(), "{k}");
    }
    assert_eq!(props["rows"]["items"]["required"], json!(["name"]));
    assert_eq!(tool["annotations"]["readOnlyHint"], false);
    assert_eq!(tool["annotations"]["destructiveHint"], false);
    // A person who cannot create sees no such tool.
    let reader = obop_commands::Principal::user("x", &[]);
    assert!(
        !obop_mcp::schema::tools(&app.dispatcher, &reader)
            .iter()
            .any(|t| t["name"].as_str().unwrap().ends_with("__bulk_create"))
    );
}

#[test]
fn cli_import_goes_through_bulk_create_with_every_mode() {
    let dir = std::env::temp_dir().join(format!(
        "obop-import-cli-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = |n: &str| dir.join(n).to_str().unwrap().to_string();
    let db = format!("sqlite://{}", path("cli.db"));
    let o = crate::Opts {
        db: Some(db.clone()),
        token: None,
        demo: false,
        dev_auth: true,
        public_site: false,
        trust_proxy: false,
        admin_prefix: "/admin".into(),
    };
    let parties = || obop_storage::open(&db).unwrap().count("party").unwrap_or(0);
    let run = |file: &str, mode: Option<&str>, dry_run: bool, key: Option<&str>| {
        crate::import_cmd::run(
            &o,
            &crate::import_cmd::Args {
                model: "party",
                file,
                mode,
                dry_run,
                key,
            },
        )
    };
    let good = path("good.csv");
    std::fs::write(&good, "name,email\nCli One,one@example.test\nCli Two,\n").unwrap();
    let bad = path("bad.csv");
    std::fs::write(&bad, "name,email\nFine,\n,x@example.test\n").unwrap();
    assert!(run(&good, None, true, None).is_ok());
    let start = parties();
    assert!(run(&good, None, true, None).is_ok());
    assert_eq!(parties(), start, "a dry run writes nothing");
    assert!(run(&good, None, false, Some("k-1")).is_ok());
    assert_eq!(parties(), start + 2);
    assert!(
        run(&good, None, false, Some("k-1")).is_ok(),
        "the same key again is the first answer"
    );
    assert_eq!(parties(), start + 2);
    // The default mode refuses a file with a bad row; partial keeps the good row and still fails.
    assert!(run(&bad, None, false, None).is_err());
    assert_eq!(parties(), start + 2);
    assert!(run(&bad, Some("partial"), false, None).is_err());
    assert_eq!(parties(), start + 3);
    assert!(run(&path("missing.csv"), None, false, None).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
