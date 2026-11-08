//! Slice S3 of the instance branding (docs/spec/branding.md, sections 6 and 13): the MCP
//! `initialize` result, the discovery documents (the protected-resource documents, the server
//! card and the authorization server's metadata) and the title of the OpenAPI document name the
//! instance when a brand is set, and name nothing else.
//!
//! The documents are anonymous: anybody who can reach the instance reads them. What they publish is
//! exactly the public fields of the record (`name`, and `slug` as the `serverInfo` name), read through
//! the one reader once for the request (section 4), and nothing else of it. With no row they are the
//! documents an instance has always sent: the golden pins of `mcp_tests.rs` (`DOCUMENT_PINS`) and
//! `openapi_tests.rs` (`OPENAPI_ANONYMOUS_PINS`) hold that, byte for byte, and are not edited here.
//!
//! Each test that sets a brand compares what an instance sends after with what it sent before on
//! the same instance, and writes the difference out by hand: a document is the document it was with
//! the listed members changed and nothing else. So a member that moves without being listed fails,
//! and so does a listed member that does not move.

use crate::app::{App, DevAuth};
use crate::brand_row_tests::set_brand;
use crate::mfa_tests::pin;
use axum::{
    body::Body,
    http::{HeaderMap, Request},
};
use http_body_util::BodyExt;
use obop_commands::Store;
use obop_http::mcp::McpConfig;
use obop_storage::Op;
use obop_storage::testing::{Flaky, Mode};
use serde_json::{Value as J, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use tower::ServiceExt;

const HOST: &str = "inst.test";

/// The anonymous documents of the discovery set, by the name the pins give them.
const DOCS: [(&str, &str); 6] = [
    (
        "protected_resource",
        "/.well-known/oauth-protected-resource",
    ),
    (
        "protected_resource_mcp",
        "/.well-known/oauth-protected-resource/mcp",
    ),
    (
        "server_card_well_known",
        "/.well-known/mcp/server-card.json",
    ),
    ("server_card", "/mcp/server-card"),
    ("ai_catalog", "/.well-known/ai-catalog.json"),
    (
        "authorization_server",
        "/.well-known/oauth-authorization-server",
    ),
];

/// A document as the pin hashes it, split into its status and headers and its JSON body.
struct Doc {
    head: String,
    body: J,
}

fn split(answer: &str) -> Doc {
    let (head, body) = answer
        .split_once("\n\n")
        .unwrap_or_else(|| panic!("no body in {answer}"));
    Doc {
        head: head.to_string(),
        body: serde_json::from_str(body).unwrap_or_else(|_| panic!("not JSON: {body}")),
    }
}

async fn documents(router: &axum::Router) -> BTreeMap<&'static str, Doc> {
    let mut out = BTreeMap::new();
    for (name, path) in DOCS {
        out.insert(name, split(&pin::document(router, path).await));
    }
    out
}

/// The document with the member at `pointer` set to `value`.
fn with(mut doc: J, pointer: &str, value: J) -> J {
    match doc.pointer_mut(pointer) {
        Some(slot) => *slot = value,
        None => panic!("{pointer} is not in the document"),
    }
    doc
}

/// The document without the member `key` of its root.
fn without(mut doc: J, key: &str) -> J {
    assert!(
        doc.as_object_mut().unwrap().remove(key).is_some(),
        "{key} is not in {doc}"
    );
    doc
}

/// The version of this build, as the pins write it.
const V: &str = "<CORE_VERSION>";

// ---- The discovery documents ---------------------------------------------------------------------

/// With a brand set the discovery documents carry the name where they said "obop instance", leave
/// out the two links to the documentation of the software, and are the same documents otherwise:
/// the identifiers (`name`, `dev.obop/auth`, `urn:air:obop:...`, `obop_client_classes_supported`,
/// `x-obop-token-endpoints`) are wire names and stay (section 11), the headers are the same, and the
/// catalogue, which has no human field, does not move. The brand is `MOHV`, slug `mohv`.
#[tokio::test]
async fn with_a_brand_the_discovery_documents_carry_the_name_and_no_link_and_nothing_else_moves() {
    for inst in pin::instances().await {
        let n = inst.name;
        let before = documents(&inst.router).await;
        set_brand(&inst.app);
        let after = documents(&inst.router).await;
        for (name, _) in DOCS {
            assert_eq!(before[name].head, after[name].head, "{n}/{name}: headers");
        }

        // The window is not vacuous: before the row, the documents are the ones that say obop.
        let b = |name: &str| before[name].body.clone();
        let a = |name: &str| after[name].body.clone();
        assert_eq!(
            b("protected_resource")["resource_name"],
            "obop instance MCP server"
        );
        assert_eq!(
            b("protected_resource")["resource_documentation"],
            "https://obop.dev/docs/agents/mcp/"
        );
        assert_eq!(b("server_card")["title"], "obop instance");

        for name in ["protected_resource", "protected_resource_mcp"] {
            let want = without(
                with(b(name), "/resource_name", json!("MOHV MCP server")),
                "resource_documentation",
            );
            assert_eq!(a(name), want, "{n}/{name}");
        }
        for name in ["server_card_well_known", "server_card"] {
            let want = with(
                with(
                    with(b(name), "/title", json!("MOHV")),
                    "/description",
                    json!("Business data and actions of MOHV, with masking and approvals."),
                ),
                "/serverInfo",
                json!({"name": "mohv", "version": V}),
            );
            assert_eq!(a(name), want, "{n}/{name}");
            // The card's identifier is the host and a wire name: it does not follow the brand.
            assert_eq!(a(name)["name"], "inst.test/obop", "{n}/{name}");
        }
        assert_eq!(a("ai_catalog"), b("ai_catalog"), "{n}: the catalogue");
        if n == "auth" {
            assert_eq!(
                b("authorization_server")["service_documentation"],
                "https://obop.dev/docs/agents/mcp/"
            );
            assert_eq!(
                a("authorization_server"),
                without(b("authorization_server"), "service_documentation"),
                "{n}: the metadata of the authorization server loses its link and nothing else"
            );
        } else {
            // No OAuth server on this instance: the answer is the refusal it always was.
            assert_eq!(a("authorization_server"), b("authorization_server"), "{n}");
        }

        // By the field, as the spec's test asks (section 13): the human fields hold the name and
        // no `obop`, `serverInfo.name` is the slug, and no link to the software's page is left.
        let human = [
            a("protected_resource")["resource_name"].clone(),
            a("server_card")["title"].clone(),
            a("server_card")["description"].clone(),
            a("server_card")["serverInfo"]["name"].clone(),
        ];
        for text in human {
            let text = text.as_str().unwrap();
            assert!(!text.to_lowercase().contains("obop"), "{n}: {text}");
        }
        for (name, _) in DOCS {
            let text = after[name].body.to_string();
            assert!(!text.contains("obop.dev"), "{n}/{name}: {text}");
        }
    }
}

/// Only the name of the record is published: a title, a message, the artwork and its version are
/// not in any anonymous document (docs/spec/branding.md, 15.1: the fields behind the doors).
#[tokio::test]
async fn the_documents_publish_the_name_and_the_slug_and_no_other_field_of_the_record() {
    for inst in pin::instances().await {
        set_brand(&inst.app);
        let mut seen = documents(&inst.router).await;
        seen.insert(
            "openapi",
            split(&pin::document(&inst.router, "/api/openapi.json").await),
        );
        let (_, init) = crate::mcp_tests::initialize(&inst.router, &inst.token, "2025-11-25").await;
        let mut all: Vec<String> = seen.values().map(|d| d.body.to_string()).collect();
        all.push(init.to_string());
        for (field, value) in crate::brand_row_tests::FIELDS {
            if matches!(field, "name" | "slug") {
                continue;
            }
            for text in &all {
                assert!(
                    !text.contains(value),
                    "{}: the value of `{field}` is in a document: {text}",
                    inst.name
                );
            }
        }
        // The slug is there, as the name of the server, in the card and in `initialize`.
        assert_eq!(seen["server_card"].body["serverInfo"]["name"], "mohv");
        assert_eq!(init["result"]["serverInfo"]["name"], "mohv");
    }
}

// ---- initialize -----------------------------------------------------------------------------------

/// `initialize` with no brand names the server `obop` and describes it as an open-source business
/// suite (the golden pin holds the whole result); with a brand the name is the slug, the title is
/// the name, and the instructions name the instance and say nothing of the software. The rest of
/// the result is the same, and the instructions after their first sentence too.
#[tokio::test]
async fn initialize_with_a_brand_names_the_instance_and_without_one_names_obop() {
    for inst in pin::instances().await {
        let n = inst.name;
        let (_, before) =
            crate::mcp_tests::initialize(&inst.router, &inst.token, "2025-11-25").await;
        let v = obop_types::CORE_VERSION;
        assert_eq!(
            before["result"]["serverInfo"],
            json!({"name": "obop", "title": "obop", "version": v}),
            "{n}"
        );
        let plain = before["result"]["instructions"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            plain.starts_with("This is an obop instance: an open-source business suite ("),
            "{n}: {plain}"
        );

        set_brand(&inst.app);
        let (_, after) =
            crate::mcp_tests::initialize(&inst.router, &inst.token, "2025-11-25").await;
        assert_eq!(
            after["result"]["serverInfo"],
            json!({"name": "mohv", "title": "MOHV", "version": v}),
            "{n}"
        );
        let text = after["result"]["instructions"].as_str().unwrap();
        assert!(
            text.starts_with(
                "This is the MOHV system: business data (contacts, CRM, sales, invoicing and installed modules) driven by typed commands. Call obop__meta first"
            ),
            "{n}: {text}"
        );
        assert!(!text.to_lowercase().contains("open-source"), "{n}: {text}");
        // The one place the word is left is the tool, which keeps its name on every instance.
        assert!(
            !text
                .replace("obop__meta", "")
                .to_lowercase()
                .contains("obop"),
            "{n}: {text}"
        );
        let rest = |t: &str| t.split_once("Call obop__meta").unwrap().1.to_string();
        assert_eq!(rest(text), rest(&plain), "{n}: the rest is the same text");
        // Nothing else of the result moves.
        assert_eq!(
            without(
                with(after["result"].clone(), "/serverInfo", json!(null)),
                "instructions"
            ),
            without(
                with(before["result"].clone(), "/serverInfo", json!(null)),
                "instructions"
            ),
            "{n}"
        );
        assert_eq!(after["result"]["protocolVersion"], "2025-11-25");
    }
}

// ---- The OpenAPI document -------------------------------------------------------------------------

/// The title of the document is the instance's own, `{name} API`, for a caller with no token and for
/// one with a token alike; the control plane keeps its title (its own, the operator's) and the
/// document of `obop openapi` is the software's, never given a brand. Nothing else of the document
/// moves.
#[tokio::test]
async fn the_openapi_title_is_the_name_of_the_instance_and_nothing_else_of_the_document_moves() {
    for inst in pin::instances().await {
        let n = inst.name;
        let anon = || async { split(&pin::document(&inst.router, "/api/openapi.json").await) };
        let signed = || async {
            split(
                &pin::exchange_as(&inst.router, "GET", "/api/openapi.json", Some(&inst.token))
                    .await,
            )
        };
        let (a0, s0) = (anon().await, signed().await);
        assert_eq!(a0.body["info"]["title"], "obop instance API", "{n}");
        assert_eq!(s0.body["info"]["title"], "obop instance API", "{n}");

        set_brand(&inst.app);
        let (a1, s1) = (anon().await, signed().await);
        assert_eq!(a1.head, a0.head, "{n}: the headers");
        assert_eq!(s1.head, s0.head, "{n}: the headers");
        assert_eq!(
            a1.body,
            with(a0.body.clone(), "/info/title", json!("MOHV API")),
            "{n}: anonymous"
        );
        assert_eq!(
            s1.body,
            with(s0.body.clone(), "/info/title", json!("MOHV API")),
            "{n}: signed in"
        );
        // The title holds the name and the description, which names no product, is untouched.
        assert!(!a1.body["info"].to_string().to_lowercase().contains("obop"));
    }
}

/// The control plane's document is SpaceCorps' own and carries no brand: an instance's brand does
/// not reach `platform()`. (Its title is a literal of the program; a row cannot change it.)
#[test]
fn the_control_plane_document_keeps_its_title_whatever_the_brand() {
    let doc = obop_http::openapi::platform("https://x.test");
    assert_eq!(doc["info"]["title"], "obop control plane API");
}

// ---- A name written around the command -----------------------------------------------------------

/// A row written straight to the store, as a restore or an import would.
fn write_row(app: &App, row: J) {
    app.store
        .backend()
        .apply(&[Op::Put {
            model: "instance_brand".into(),
            id: "instance".into(),
            fields: row.as_object().unwrap().clone(),
        }])
        .unwrap();
}

/// The documents of one instance as JSON, by name: `initialize` (from a token), the six discovery
/// documents and the anonymous OpenAPI document.
async fn json_documents(inst: &pin::Instance) -> BTreeMap<String, J> {
    let mut out = BTreeMap::new();
    for (name, doc) in documents(&inst.router).await {
        out.insert(name.to_string(), doc.body);
    }
    out.insert(
        "openapi".into(),
        split(&pin::document(&inst.router, "/api/openapi.json").await).body,
    );
    let (_, init) = crate::mcp_tests::initialize(&inst.router, &inst.token, "2025-11-25").await;
    out.insert("initialize".into(), init["result"].clone());
    out
}

/// Every string of a document with `from` replaced by `to`.
fn replace_in(doc: &J, from: &str, to: &str) -> J {
    match doc {
        J::String(s) => J::String(s.replace(from, to)),
        J::Array(a) => J::Array(a.iter().map(|v| replace_in(v, from, to)).collect()),
        J::Object(o) => J::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), replace_in(v, from, to)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// A name that a row written around the command can hold and the read check lets through (it only
/// refuses controls, format and invisible characters, `<` and `>`, and a length over the limit) can
/// hold a quote, a backslash, braces and a comma. Every document is built from a JSON value, never
/// from text, so each one stays valid JSON with the same members and the same shape as the
/// documents of a harmless name, and the name stands in the same places, as the string it is. The
/// stored slug is a fixed pattern, so a hostile slug falls back to the one derived from the name.
#[tokio::test]
async fn a_hostile_name_stays_a_string_in_every_json_document() {
    const BENIGN: &str = "Zed Co";
    // The harmless instances: the same shape of row, with a name that needs no escaping.
    let mut plain = Vec::new();
    for inst in pin::instances().await {
        write_row(
            &inst.app,
            json!({"name": BENIGN, "slug": "zed-slug", "logo_svg": ""}),
        );
        plain.push((inst.name, json_documents(&inst).await));
    }
    for hostile in [
        r#"A"}],"x":["y"#,
        r#"C:\path\"quoted" \u0022"#,
        "Q&A 'n' {\"a\": 1}",
    ] {
        for (inst, (name, plain)) in pin::instances().await.into_iter().zip(&plain) {
            // A slug that fails its pattern: the one derived from the name is read instead.
            write_row(
                &inst.app,
                json!({"name": hostile, "slug": "Foo/Bar", "logo_svg": ""}),
            );
            let docs = json_documents(&inst).await;
            let slug = docs["initialize"]["serverInfo"]["name"]
                .as_str()
                .unwrap()
                .to_string();
            assert!(
                !slug.is_empty()
                    && slug
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
                "{hostile:?}: the slug is a fixed pattern, not the name: {slug:?}"
            );
            for (doc, value) in &docs {
                // The name stands where the harmless name stands, as the string it is ...
                let mapped = replace_in(&replace_in(value, hostile, BENIGN), &slug, "zed-slug");
                assert_eq!(
                    mapped, plain[doc],
                    "{name}/{doc} for {hostile:?}: the document differs in more than the name"
                );
                // ... and its text reached the document, so the comparison was not empty.
                if matches!(
                    doc.as_str(),
                    "protected_resource"
                        | "protected_resource_mcp"
                        | "server_card"
                        | "server_card_well_known"
                        | "openapi"
                        | "initialize"
                ) {
                    let escaped = serde_json::to_string(&json!(hostile)).unwrap();
                    let text = value.to_string();
                    assert!(
                        text.contains(escaped.trim_matches('"')),
                        "{name}/{doc}: the name is not in the document ({hostile:?}): {text}"
                    );
                }
            }
            // The metadata of the authorization server has no name to carry: nothing of it is there.
            if let Some(meta) = docs.get("authorization_server") {
                assert!(!meta.to_string().contains("quoted"), "{meta}");
            }
        }
    }
}

/// A name that fails the read check (markup) is never in a document: the default is read in its
/// place, as for every surface (section 4), and the slug of the default.
#[tokio::test]
async fn a_name_that_fails_the_read_check_is_never_in_a_document() {
    for inst in pin::instances().await {
        write_row(
            &inst.app,
            json!({"name": "<script>alert(1)</script>", "slug": "ok-slug", "logo_svg": ""}),
        );
        let docs = json_documents(&inst).await;
        for (doc, value) in &docs {
            let text = value.to_string();
            assert!(
                !text.contains("<script") && !text.contains("alert"),
                "{}/{doc}: {text}",
                inst.name
            );
        }
        // The row exists, so it is a brand: the name fell back to `obop`, the slug is the stored one.
        assert_eq!(docs["initialize"]["serverInfo"]["name"], "ok-slug");
        assert_eq!(docs["initialize"]["serverInfo"]["title"], "obop");
        assert_eq!(docs["openapi"]["info"]["title"], "obop API");
    }
}

// ---- The store that fails (D74) -------------------------------------------------------------------

/// The server over a store that can be made to fail, with the development authenticator (the token
/// `dev-agent` opens an MCP session).
fn flaky_server() -> (Arc<Flaky>, App, axum::Router) {
    let (flaky, backend) = Flaky::memory();
    let app = App::with_store(Store::over(backend));
    let router = obop_http::router(
        obop_http::HttpState::with_mcp(
            app.dispatcher.clone(),
            Arc::new(DevAuth),
            McpConfig::default(),
        ),
        None,
    );
    (flaky, app, router)
}

struct Answer {
    status: u16,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl Answer {
    fn header(&self, name: &str) -> &str {
        self.headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    }
    fn json(&self) -> J {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|_| panic!("not JSON: {}", String::from_utf8_lossy(&self.body)))
    }
}

async fn ask(r: &axum::Router, req: Request<Body>) -> Answer {
    let resp = r.clone().oneshot(req).await.unwrap();
    let (status, headers) = (resp.status().as_u16(), resp.headers().clone());
    let body = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    Answer {
        status,
        headers,
        body,
    }
}

async fn get(r: &axum::Router, path: &str) -> Answer {
    ask(
        r,
        Request::get(path)
            .header("host", HOST)
            .body(Body::empty())
            .unwrap(),
    )
    .await
}

/// The code word of a 503 of D74: in the member `code` of the REST error, and in the message of the
/// JSON-RPC error that the layer sends for a path under `/mcp` (the card is one).
fn fault_code(body: &J) -> String {
    body["code"]
        .as_str()
        .or_else(|| body["error"]["message"].as_str())
        .unwrap_or_default()
        .to_string()
}

/// A POST of `initialize` as the development agent.
async fn post_initialize(r: &axum::Router) -> Answer {
    let body = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}
    }});
    ask(
        r,
        Request::post("/mcp")
            .header("host", HOST)
            .header("authorization", "Bearer dev-agent")
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
}

/// The bound of the reader's cell (`brand/reader.rs`, `DEFAULT_BOUND`; pinned at two seconds by
/// `brand/tests/cache.rs`); the tests that wait for the cell to go stale wait this long.
const READER_BOUND: std::time::Duration = std::time::Duration::from_secs(2);

/// Returns when a cell filled by a read that began before `filled` is stale. The loop is on the
/// clock, so an early wake-up does not end it early.
fn wait_until_stale(filled: std::time::Instant) {
    let stale = READER_BOUND + std::time::Duration::from_millis(50);
    while filled.elapsed() <= stale {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// The documents that ask the reader, in the order the test below visits them.
const ASKING: [(&str, &str); 4] = [
    (
        "protected_resource",
        "/.well-known/oauth-protected-resource",
    ),
    ("server_card", "/mcp/server-card"),
    (
        "server_card_well_known",
        "/.well-known/mcp/server-card.json",
    ),
    ("openapi", "/api/openapi.json"),
];

/// What the spec says of an anonymous document when the store cannot be read (section 4, the
/// display read and D74; the documents are the same case as the logo route): once the reader's cell
/// has expired, the request that runs the refill meets the fault and is answered 503 with
/// `Retry-After` by D74 and not with a document that says `obop`; nothing is kept, so the next
/// request reads again and is answered the same way, whichever document it asks for; when the store
/// recovers the documents are back with the name. The document that carries no human field, the
/// catalogue, asks the reader nothing and is served throughout.
#[tokio::test]
async fn after_a_good_read_a_failing_store_answers_an_anonymous_document_with_a_503_and_keeps_nothing()
 {
    let (flaky, app, router) = flaky_server();
    set_brand(&app);
    let good: Vec<(String, Vec<u8>)> = {
        let mut v = Vec::new();
        for (name, path) in ASKING {
            let a = get(&router, path).await;
            assert_eq!(a.status, 200, "{name}");
            v.push((name.to_string(), a.body));
        }
        v
    };
    assert!(
        String::from_utf8_lossy(&good[1].1).contains("MOHV"),
        "the cell holds the brand"
    );
    // Taken after the read that filled the cell: the cell is at least as old as this instant.
    let filled = std::time::Instant::now();
    flaky.refuse_model("instance_brand", Mode::Reads);
    tokio::task::spawn_blocking(move || wait_until_stale(filled))
        .await
        .unwrap();

    // The catalogue has no human field and asks the reader nothing.
    let catalogue = get(&router, "/.well-known/ai-catalog.json").await;
    assert_eq!(catalogue.status, 200);
    assert_eq!(flaky.refused(), 0, "the catalogue read nothing");

    // Each document in turn is the request that runs a refill that fails, and nothing was kept.
    for (name, path) in ASKING {
        let before = flaky.refused();
        let a = get(&router, path).await;
        assert_eq!(
            a.status,
            503,
            "{name}: {}",
            String::from_utf8_lossy(&a.body)
        );
        assert_eq!(a.header("retry-after"), "1", "{name}");
        assert_eq!(a.header("x-obop-applied"), "none", "{name}");
        assert_eq!(fault_code(&a.json()), "storage_busy", "{name}");
        let body = a.json();
        for member in ["resource_name", "serverInfo", "info", "title"] {
            assert!(body.get(member).is_none(), "{name}: not a document: {body}");
        }
        assert!(
            flaky.refused() > before,
            "{name}: the refill read the store"
        );
    }
    // `initialize` is the same: the refill meets the fault, the request is answered 503 and no
    // session is left open for a client that was never told who the server is.
    let init = post_initialize(&router).await;
    assert_eq!(init.status, 503, "{}", String::from_utf8_lossy(&init.body));
    assert_eq!(init.header("retry-after"), "1");
    assert_eq!(fault_code(&init.json()), "storage_busy");
    assert!(
        app.store.backend().kv_scan("mcp:i:").unwrap().is_empty(),
        "no session was opened"
    );

    // The store recovers: every document is back, with the name.
    flaky.allow_models();
    for (name, path) in ASKING {
        let a = get(&router, path).await;
        assert_eq!(a.status, 200, "{name}");
        assert_eq!(
            a.body,
            good.iter().find(|(n, _)| n == name).unwrap().1,
            "{name}"
        );
    }
    let init = post_initialize(&router).await;
    assert_eq!(init.status, 200);
    assert_eq!(init.json()["result"]["serverInfo"]["title"], "MOHV");
}

/// A process that never had a good read: the request that runs the first refill meets the fault and
/// is answered by D74, and nothing is pinned, so when the store answers the next request is the
/// answer of the store (here: no row, and the document of an instance with no brand).
#[tokio::test]
async fn a_process_that_never_had_a_good_read_serves_the_default_documents_once_the_store_answers()
{
    for model in ["instance_brand", "obop_setting"] {
        let (flaky, _app, router) = flaky_server();
        flaky.refuse_model(model, Mode::Reads);
        for (name, path) in ASKING {
            let a = get(&router, path).await;
            assert_eq!(a.status, 503, "{model}/{name}");
            assert_eq!(a.header("retry-after"), "1", "{model}/{name}");
        }
        flaky.allow_models();
        let card = get(&router, "/mcp/server-card").await.json();
        assert_eq!(card["title"], "obop instance", "{model}");
        let api = get(&router, "/api/openapi.json").await.json();
        assert_eq!(api["info"]["title"], "obop instance API", "{model}");
    }
}

/// A request that arrives while a refill is parked in the store does not wait for it and is given
/// the last good name; the runtime has one worker, so a read on the thread that polls the requests
/// would stop everything below. The owner's refill is let go at the end and is answered with the
/// card. The counts are the reader's: a hundred requests read nothing, and the owner read the row
/// and the theme.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn while_a_refill_is_parked_the_documents_are_served_with_the_last_good_name() {
    let (flaky, app, router) = flaky_server();
    set_brand(&app);
    let good = get(&router, "/mcp/server-card").await;
    assert_eq!(good.status, 200);
    let filled = std::time::Instant::now();
    tokio::task::spawn_blocking(move || wait_until_stale(filled))
        .await
        .unwrap();

    let (parked_tx, parked_rx) = std::sync::mpsc::channel::<()>();
    let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
    flaky.after_the_get_of(
        "instance_brand",
        0,
        Box::new(move |_| {
            parked_tx.send(()).unwrap();
            let _ = go_rx.recv_timeout(std::time::Duration::from_secs(30));
        }),
    );
    flaky.reset_reads();
    let owner = {
        let r = router.clone();
        tokio::spawn(async move { get(&r, "/mcp/server-card").await })
    };
    let parked = tokio::task::spawn_blocking(move || {
        parked_rx.recv_timeout(std::time::Duration::from_secs(20))
    })
    .await
    .unwrap();
    assert!(parked.is_ok(), "the request never reached the store");

    let started = std::time::Instant::now();
    let mut set = tokio::task::JoinSet::new();
    for i in 0..100 {
        let r = router.clone();
        let path = [
            "/mcp/server-card",
            "/.well-known/oauth-protected-resource",
            "/api/openapi.json",
        ][i % 3];
        set.spawn(async move { (path, get(&r, path).await) });
    }
    let mut answers = Vec::new();
    while let Some(a) = set.join_next().await {
        answers.push(a.unwrap());
    }
    assert!(
        started.elapsed() < std::time::Duration::from_secs(20),
        "a request waited for the parked refill: {:?}",
        started.elapsed()
    );
    for (path, a) in &answers {
        assert_eq!(a.status, 200, "{path}");
        let text = String::from_utf8_lossy(&a.body);
        assert!(text.contains("MOHV"), "{path}: the last good name: {text}");
    }
    assert_eq!(
        flaky.reads().gets,
        1,
        "the hundred read nothing: the owner's flight"
    );
    assert!(!owner.is_finished(), "the owner is still in the store");

    go_tx.send(()).unwrap();
    let owner = owner.await.unwrap();
    assert_eq!((owner.status, owner.body), (200, good.body));
}

/// The cost, while the store answers (section 9): the first request for a document costs the
/// reader's one flight, the row and the theme setting; the documents that follow within the bound
/// read nothing, whichever they are; the catalogue, which has no human field, never asks.
#[tokio::test]
async fn a_document_costs_the_reader_one_flight_and_the_catalogue_none() {
    let (flaky, app, router) = flaky_server();
    set_brand(&app);
    flaky.reset_reads();
    // `set_brand` filled the cell moments ago: the documents are served from it.
    let started = std::time::Instant::now();
    for (_, path) in ASKING {
        assert_eq!(get(&router, path).await.status, 200);
    }
    assert_eq!(
        get(&router, "/.well-known/ai-catalog.json").await.status,
        200
    );
    if started.elapsed() < std::time::Duration::from_millis(1500) {
        assert_eq!(
            flaky.reads().gets,
            0,
            "the cell was fresh: no document read the store"
        );
    }
    // A cold reader: the first document costs one flight of two reads, the rest nothing.
    let (flaky, app, router) = flaky_server();
    write_row(&app, json!({"name": "Zed", "logo_svg": ""}));
    flaky.reset_reads();
    assert_eq!(
        get(&router, "/.well-known/ai-catalog.json").await.status,
        200
    );
    assert_eq!(
        flaky.reads().gets,
        0,
        "the catalogue asks the reader nothing"
    );
    let started = std::time::Instant::now();
    assert_eq!(get(&router, "/mcp/server-card").await.status, 200);
    assert_eq!(flaky.reads().gets, 2, "the row and the theme: one flight");
    for (_, path) in ASKING {
        assert_eq!(get(&router, path).await.status, 200);
    }
    if started.elapsed() < std::time::Duration::from_millis(1500) {
        assert_eq!(
            flaky.reads().gets,
            2,
            "the documents that follow read nothing"
        );
    }
}
