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
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use tower::ServiceExt;

/// The commands a module refuses for everyone (`Dispatcher::refuse_always`, D95: `can_delete` /
/// `can_update` false in the metadata): registered on REST, absent from the document. Every other
/// command has an operation, those of a model without a `list` included (K26).
const KNOWN_OMITTED: &[&str] = &[
    "mileage_point.delete",
    "mileage_point.update",
    "mileage_trip.delete",
];

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

/// Every run of the pattern `dddd-dd-dd` (a date) or `dd:dd:dd` (a time of day) in a text.
fn dates_and_times(text: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let b = text.as_bytes();
    let digits = |from: usize, n: usize| {
        b.len() >= from + n && b[from..from + n].iter().all(u8::is_ascii_digit)
    };
    let sep = |at: usize, c: u8| b.get(at) == Some(&c);
    let (mut dates, mut times) = (BTreeSet::new(), BTreeSet::new());
    for i in 0..b.len() {
        if digits(i, 4)
            && sep(i + 4, b'-')
            && digits(i + 5, 2)
            && sep(i + 7, b'-')
            && digits(i + 8, 2)
        {
            dates.insert(text[i..i + 10].to_string());
        }
        if digits(i, 2)
            && sep(i + 2, b':')
            && digits(i + 3, 2)
            && sep(i + 5, b':')
            && digits(i + 6, 2)
        {
            times.insert(text[i..i + 8].to_string());
        }
    }
    (dates, times)
}

#[test]
fn the_document_names_only_the_placeholder_address_and_fixed_dates_and_times() {
    let text = document();
    // Addresses: every http(s) URL in the document starts with the placeholder.
    for scheme in ["http://", "https://"] {
        for (at, _) in text.match_indices(scheme) {
            assert!(
                text[at..].starts_with(BASE_URL),
                "an address other than {BASE_URL}: {}",
                &text[at..(at + 40).min(text.len())]
            );
        }
    }
    // Dates and times: the MCP protocol versions (dates by name) and the one fixed example of a
    // date-time field are all there is; a timestamp of the run, a build or a file would be a
    // new date or a new time of day and fail here.
    let (dates, times) = dates_and_times(&text);
    let mut known: BTreeSet<String> = obop_mcp::SUPPORTED_VERSIONS
        .iter()
        .map(|v| v.to_string())
        .collect();
    known.insert("2026-03-29".into()); // the example of every date-time field (openapi.rs)
    let unknown: Vec<&String> = dates.difference(&known).collect();
    assert!(unknown.is_empty(), "unknown dates {unknown:?}");
    assert!(
        times.iter().all(|t| t == "01:30:00"),
        "unknown times of day: {times:?}"
    );
    let doc: J = serde_json::from_str(&text).unwrap();
    assert_eq!(doc["x-mcp"]["url"], format!("{BASE_URL}/mcp"));
    assert_eq!(doc["x-mcp"]["authorization"], "oauth2 or bearer");
}

#[test]
fn the_date_and_time_scan_finds_what_it_is_meant_to() {
    let (dates, times) =
        dates_and_times("a 2026-10-28T12:34:56Z b 2025-03-26 c 1999-1-1 d 12:34 e 7:00:00");
    assert_eq!(
        dates.into_iter().collect::<Vec<_>>(),
        ["2025-03-26", "2026-10-28"]
    );
    assert_eq!(times.into_iter().collect::<Vec<_>>(), ["12:34:56"]);
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
        let (model, command) =
            c.id.split_once('.')
                .unwrap_or_else(|| panic!("a command id without a model: {}", c.id));
        if !paths.contains_key(&format!("/api/{model}/{command}")) {
            missing.push(c.id.clone());
        }
    }
    missing.sort();
    // A command of a model without a `list` is described from `Dispatcher::commands_outside_meta`
    // (K26), so only the commands refused for everyone have no operation. A new omission fails
    // here.
    assert_eq!(
        missing, KNOWN_OMITTED,
        "commands without an operation in the document"
    );
    for want in [
        "/api/party/list",
        "/api/party/bulk_create",
        "/api/sale_order/confirm",
        "/api/auth/login",
        "/api/meta",
        "/api/setting/get",
        "/api/instance_update/check",
        "/api/import_map/get",
        "/api/module_plan/preview",
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
    // `HttpState::new` reads OBOP_PUBLIC_URL; an explicit default configuration reads nothing.
    let state = obop_http::HttpState::with_mcp(
        app.dispatcher.clone(),
        auth.clone(),
        obop_http::mcp::McpConfig::default(),
    );
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
    // A directory of this run's own, removed when the test ends (after the stores are closed).
    let _guard = obop_storage::testing::TempPath::dir("obop-openapi-cmd");
    let dir = _guard.path().to_path_buf();
    let file = dir.join("openapi.json");
    assert!(run(Some(file.to_str().unwrap())).is_ok());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), document());
    // A directory that does not exist is a failure with a message, not a panic.
    let nowhere = dir.join("missing").join("openapi.json");
    assert!(run(Some(nowhere.to_str().unwrap())).is_err());
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

/// `bulk_create` refuses an image or file field in a row (a blob is uploaded and claimed one record
/// at a time, D60, D70), so the document does not offer one there, though `create` takes it.
#[test]
fn a_bulk_row_does_not_offer_the_image_and_file_fields_that_bulk_create_refuses() {
    let doc = value();
    let blob = |p: &J| p["pattern"] == "^[0-9a-f]{64}$";
    let props = |path: &str, rows: bool| -> J {
        let body =
            &doc["paths"][path]["post"]["requestBody"]["content"]["application/json"]["schema"];
        if rows {
            body["properties"]["rows"]["items"]["properties"].clone()
        } else {
            body["properties"].clone()
        }
    };
    // The party's photo is an image: create takes it, bulk_create rows do not.
    assert!(blob(&props("/api/party/create", false)["photo"]));
    let rows = props("/api/party/bulk_create", true);
    assert!(
        rows.get("photo").is_none() && rows.get("name").is_some(),
        "{rows}"
    );
    // And for every model of the document, no row property is a blob id.
    let mut checked = 0;
    for path in doc["paths"].as_object().unwrap().keys() {
        if path.ends_with("/bulk_create") {
            checked += 1;
            for (field, p) in props(path, true).as_object().unwrap() {
                assert!(!blob(p), "{path} offers the blob field {field}");
            }
        }
    }
    assert!(checked > 5, "{checked}");
}

// ---- Golden pins of the anonymous OpenAPI document (S1a-0 of the branding work) ------------------

/// `GET /api/openapi.json` with no token: the document an instance with no brand row sends to
/// anyone (sign-in and the discovery endpoints), on an instance without an OAuth server (`dev`)
/// and on one with it (`auth`). The host is `inst.test`, the protocol https, and the version of
/// this build is written `<CORE_VERSION>` before hashing. (The document of a signed-in account is
/// the committed `openapi.json`, held by `the_committed_openapi_document_is_current`.)
#[tokio::test]
async fn pins_the_anonymous_openapi_document_as_an_instance_with_no_brand_sends_it() {
    use crate::mfa_tests::pin;
    let mut now = Vec::new();
    for inst in pin::instances().await {
        now.push((
            format!("{}/openapi", inst.name),
            pin::sha(&pin::document(&inst.router, "/api/openapi.json").await),
        ));
    }
    pin::check("OPENAPI_ANONYMOUS_PINS", OPENAPI_ANONYMOUS_PINS, &now);
}

// Printed by `OBOP_PRINT_PINS=1` and pasted here. First on origin/main 12a59a79; again on
// `feat/agent-token-admin` (the D69 amendment of 2026-11-14), which moved the document on purpose:
// the two paths `/api/auth/agent-tokens` and `/api/auth/agent-tokens/{id}/revoke`, the description
// of the scopes of `issueAgentToken`, and two more kinds in the description of `security_event.kind`.
// Again on `feat/ui-L9` (D95, 2026-11-15), on purpose: `info.version` 0.2.2 and the `message` key of
// the error schema. Again on `fix/w2b-h2` (N18, 2026-11-17), on purpose: the `expires_at` of a row of
// `listAgentTokens`. Again on `fix/issues-w2c` (K55, second review), on purpose: the query parameters
// `model`, `record` and `field` of `downloadBlob` and its description; repinned when main (PR 151)
// was merged in, as the document now carries both changes. Again on `feat/first-admin-setup` (D96,
// 2026-11-16, merged with main on 2026-11-18 twice), on purpose: the path `/api/auth/setup` and
// three more kinds in the description of `security_event.kind`.
#[rustfmt::skip]
const OPENAPI_ANONYMOUS_PINS: &[(&str, &str)] = &[
    ("dev/openapi", "fbb98141e88fd6092eda9293ba57145e8ffd9bf8c9f3767ea7a6a91af70e37fc"),
    ("auth/openapi", "9e06cc4541c985c2fd3ecc9e51e13038ee839b9990fa4e05a7213e2db410c873"),
];

/// With a brand row in the store the anonymous document is the one an instance with no row sends,
/// byte for byte, headers included, but for its title (slice S3: `{name} API`, in place of
/// "obop instance API"; `brand_docs_tests.rs` holds the difference member by member). With the
/// title put back, the digest is the pinned one: nothing else of the document moved.
#[tokio::test]
async fn with_a_brand_row_the_anonymous_openapi_document_is_what_it_was_but_for_its_title() {
    use crate::mfa_tests::pin;
    let mut now = Vec::new();
    for inst in pin::instances().await {
        crate::brand_row_tests::set_brand(&inst.app);
        let text = pin::document(&inst.router, "/api/openapi.json").await;
        let put_back = text.replacen(
            "\"title\":\"MOHV API\"",
            "\"title\":\"obop instance API\"",
            1,
        );
        assert_ne!(text, put_back, "{}: the title is the name's", inst.name);
        now.push((format!("{}/openapi", inst.name), pin::sha(&put_back)));
    }
    pin::check("OPENAPI_ANONYMOUS_PINS", OPENAPI_ANONYMOUS_PINS, &now);
}

/// L6: `device.update` takes the name and nothing else (its guard refuses every other field), and
/// the document says so, in its summary and in its body; the generic sentence of the model engine
/// ("a status field only moves along a declared action") described another command.
#[test]
fn the_device_update_operation_says_it_renames_and_takes_only_the_name() {
    let doc: J = serde_json::from_str(&document()).unwrap();
    let op = &doc["paths"]["/api/device/update"]["post"];
    assert_eq!(
        op["summary"],
        obop_commands::devices::UPDATE_SUMMARY,
        "{op}"
    );
    let body = &op["requestBody"]["content"]["application/json"]["schema"];
    let keys: BTreeSet<&str> = body["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, BTreeSet::from(["id", "name"]), "{body}");
    assert_eq!(body["required"], serde_json::json!(["id", "name"]));
}
