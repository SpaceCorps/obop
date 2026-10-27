//! `obop openapi`: the exported document is deterministic, is the document a running instance
//! serves to an account with every group, and the committed copy that the TypeScript client is
//! generated from (`web/packages/client/openapi.json`) is current.

use crate::app::App;
use crate::openapi_cmd::{BASE_URL, document, run, value};
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use obop_auth::Auth;
use obop_commands::{Authenticator, Principal, Surface};
use serde_json::Value as J;
use std::path::PathBuf;
use std::sync::Arc;
use tower::ServiceExt;

/// The committed copy: `web/packages/client/openapi.json`, from the root of the workspace.
fn committed_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../web/packages/client/openapi.json")
}

#[test]
fn two_runs_write_the_same_bytes_in_a_fixed_shape() {
    let (a, b) = (document(), document());
    assert_eq!(a, b);
    assert!(
        a.ends_with("}\n") && !a.ends_with("\n\n"),
        "one trailing newline"
    );
    assert!(!a.contains('\r'), "LF line ends only");
    assert!(
        a.starts_with("{\n  \"components\""),
        "sorted keys, two-space indent"
    );
    let doc: J = serde_json::from_str(&a).unwrap();
    assert_eq!(doc["openapi"], "3.1.0");
    assert_eq!(doc["servers"][0]["url"], BASE_URL);
}

#[test]
fn nothing_in_the_document_depends_on_the_host_or_the_time() {
    let text = document();
    // The placeholder is the only address; no temp dir, home directory or user name leaks in.
    for needle in [
        std::env::temp_dir().to_string_lossy().to_string(),
        std::env::var("USERPROFILE").unwrap_or_default(),
        std::env::var("HOME").unwrap_or_default(),
        std::env::var("USERNAME").unwrap_or_default(),
    ] {
        if needle.len() > 3 {
            assert!(!text.contains(&needle), "{needle}");
        }
    }
    let doc: J = serde_json::from_str(&text).unwrap();
    assert_eq!(doc["x-mcp"]["url"], format!("{BASE_URL}/mcp"));
    assert_eq!(doc["x-mcp"]["authorization"], "oauth2 or bearer");
}

#[test]
fn it_lists_every_command_of_an_account_with_every_group() {
    let app = App::new();
    let groups = app.all_groups();
    let groups: Vec<&str> = groups.iter().map(String::as_str).collect();
    let all = Principal::user("all", &groups);
    let doc = value();
    let paths = doc["paths"].as_object().unwrap();
    let commands = app.dispatcher.catalog(Surface::Rest, &all);
    assert!(commands.len() > 100, "{}", commands.len());
    let mut missing = Vec::new();
    for c in &commands {
        let Some((model, command)) = c.id.split_once('.') else {
            continue;
        };
        if !paths.contains_key(&format!("/api/{model}/{command}")) {
            missing.push(c.id.clone());
        }
    }
    // Commands of the built-in models only: the ones that belong to no model of the registry
    // (for example the instance-wide update commands) are not generated operations.
    assert!(
        missing.len() * 4 < commands.len(),
        "most commands are listed, missing {missing:?}"
    );
    for want in [
        "/api/party/list",
        "/api/party/bulk_create",
        "/api/sale_order/confirm",
        "/api/auth/login",
        "/api/meta",
    ] {
        assert!(paths.contains_key(want), "{want}");
    }
}

#[test]
fn keys_are_sorted_at_every_level() {
    fn check(v: &J, at: &str) {
        match v {
            J::Object(m) => {
                let keys: Vec<&String> = m.keys().collect();
                let mut sorted = keys.clone();
                sorted.sort();
                assert_eq!(keys, sorted, "unsorted keys at {at}");
                for (k, v) in m {
                    check(v, &format!("{at}/{k}"));
                }
            }
            J::Array(a) => a
                .iter()
                .enumerate()
                .for_each(|(i, v)| check(v, &format!("{at}/{i}"))),
            _ => {}
        }
    }
    check(&serde_json::from_str::<J>(&document()).unwrap(), "");
}

/// A running instance serves the very document that `obop openapi` writes, to an account that
/// holds every group and asks at the placeholder address.
#[tokio::test]
async fn the_export_is_what_a_running_instance_serves_to_an_administrator() {
    let app = App::new();
    let auth = Arc::new(Auth::with_test_params(app.store.backend()));
    auth.create_user("ada", "correct horse battery", &app.all_groups())
        .unwrap();
    let as_auth: Arc<dyn Authenticator> = auth.clone();
    app.register_accounts(as_auth);
    let token = auth.login("ada", "correct horse battery").unwrap();
    let state = obop_http::HttpState::new(app.dispatcher.clone(), auth.clone());
    let router = obop_http::router(state, None);
    let host = BASE_URL.trim_start_matches("http://");
    let req = Request::get("/api/openapi.json")
        .header("host", host)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let served: J = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        served,
        value(),
        "the exported document differs from the served one"
    );
}

#[test]
fn out_writes_the_document_to_a_file_and_refuses_a_path_it_cannot_write() {
    let dir = std::env::temp_dir().join(format!("obop-openapi-cmd-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("openapi.json");
    assert!(run(Some(file.to_str().unwrap())).is_ok());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), document());
    // A directory that does not exist is a failure with a message, not a panic.
    let nowhere = dir.join("missing").join("openapi.json");
    assert!(run(Some(nowhere.to_str().unwrap())).is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The drift gate of the TypeScript client: `web/packages/client/openapi.json` is the input of
/// `schema.d.ts`, and it must be what `obop openapi` writes now. When a model, a field or a
/// command changes, refresh it with either
///
/// ```text
/// cargo run -q -- openapi --out web/packages/client/openapi.json
/// OBOP_UPDATE_OPENAPI=1 cargo test -p obop the_committed_openapi_document
/// ```
///
/// and then regenerate the types (`cd web/packages/client && vp run generate:types`).
#[test]
fn the_committed_openapi_document_is_current() {
    let path = committed_path();
    let now = document();
    if std::env::var("OBOP_UPDATE_OPENAPI").is_ok_and(|v| v == "1") {
        std::fs::write(&path, now.as_bytes()).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    assert!(
        committed == now,
        "web/packages/client/openapi.json is out of date. Refresh it with\n  \
         cargo run -q -- openapi --out web/packages/client/openapi.json\n\
         (or OBOP_UPDATE_OPENAPI=1 cargo test -p obop the_committed_openapi_document), then run\n  \
         cd web/packages/client && vp run generate:types\n\
         and commit both files."
    );
}

/// `declare_503` adds the 503 to every operation; an operation that already had one keeps its own
/// sentence and gains the rule. A generated operation has no 503 of its own, so its description
/// is the rule once and not twice.
#[test]
fn a_generated_operation_describes_the_503_once() {
    let doc = value();
    let text = doc["paths"]["/api/party/list"]["post"]["responses"]["503"]["description"]
        .as_str()
        .unwrap();
    assert_eq!(text.matches("storage_busy").count(), 1, "{text}");
    assert!(!text.contains("Also:"));
}
