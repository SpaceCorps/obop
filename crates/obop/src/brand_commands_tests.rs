//! The commands of the instance brand on the real surfaces (docs/spec/branding.md, section 13:
//! "Commands", "Only these commands", "Parity" and "The model's language fields equal `LANGS`"):
//! the real `App` (so the real model, labels and wiring), the real REST router, the MCP handler of
//! `obop-mcp`, the dispatcher with the surface of the command line, and `obop import`. The
//! principals are made by a small authenticator that gives each token one of the kinds the spec
//! names: a person with `platform.admin`, an agent token, a scoped token, a mobile app's token and
//! a person without the group.
//!
//! The first CLI, REST and MCP parity test of the repository: the answers of `get`, `configure`
//! and `clear` carry the same keys and values on the three surfaces.

use crate::app::App;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use obop_commands::{
    ADMIN_GROUP, Authenticator, CallError, Dispatcher, Principal, Surface, Value, Vault,
};
use serde_json::{Value as J, json};
use std::sync::Arc;
use tower::ServiceExt;

/// The tokens of the tests. All of them are test values for this process only.
struct Tokens;

impl Authenticator for Tokens {
    fn resolve(&self, token: &str) -> Option<Principal> {
        match token {
            "admin" => Some(admin()),
            "agent" => Some(agent()),
            "scoped" => Some(scoped()),
            "mobile" => Some(mobile()),
            "bob" => Some(bob()),
            _ => None,
        }
    }
    fn login(&self, _: &str, _: &str) -> Result<String, String> {
        Err("no login in this test".into())
    }
    fn issue_agent_token(&self, _: &Principal, _: &[String], _: &str) -> Result<String, String> {
        Err("no agent tokens in this test".into())
    }
}

fn admin() -> Principal {
    Principal::user("ada", &[ADMIN_GROUP])
}

fn agent() -> Principal {
    Principal::agent("ada/agent", &[ADMIN_GROUP], &[ADMIN_GROUP])
}

fn scoped() -> Principal {
    let mut p = admin();
    p.scopes = Some([ADMIN_GROUP.to_string()].into_iter().collect());
    p
}

fn mobile() -> Principal {
    let mut p = scoped();
    p.device = Some("device-1".into());
    p
}

fn bob() -> Principal {
    Principal::user("bob", &["base.user"])
}

fn router(app: &App) -> axum::Router {
    obop_http::router(
        obop_http::HttpState::new(app.dispatcher.clone(), Arc::new(Tokens)),
        None,
    )
}

async fn rest(r: &axum::Router, method: &str, token: &str, path: &str, body: J) -> (u16, J) {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "inst.test")
        .header("authorization", format!("Bearer {token}"));
    let body = if method == "GET" {
        Body::empty()
    } else {
        req = req.header("content-type", "application/json");
        Body::from(body.to_string())
    };
    let resp = r.clone().oneshot(req.body(body).unwrap()).await.unwrap();
    let status = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).to_string();
    (
        status,
        serde_json::from_str(&text).unwrap_or(J::String(text)),
    )
}

async fn post(r: &axum::Router, token: &str, command: &str, body: J) -> (u16, J) {
    rest(
        r,
        "POST",
        token,
        &format!("/api/instance_brand/{command}"),
        body,
    )
    .await
}

/// One MCP request for a principal over the JSON-RPC handler, in a session that has been through
/// the handshake.
struct Mcp {
    d: Arc<Dispatcher>,
    session: obop_mcp::Session,
    next: i64,
}

impl Mcp {
    fn new(d: &Arc<Dispatcher>, p: &Principal) -> Mcp {
        let mut m = Mcp {
            d: d.clone(),
            session: obop_mcp::Session::new(),
            next: 1,
        };
        m.rpc(
            p,
            "initialize",
            json!({"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}),
        );
        let _ = obop_mcp::handle(
            d,
            p,
            &mut m.session,
            &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string(),
        );
        m
    }

    fn rpc(&mut self, p: &Principal, method: &str, params: J) -> J {
        self.next += 1;
        let line = json!({"jsonrpc": "2.0", "id": self.next, "method": method, "params": params})
            .to_string();
        let reply = obop_mcp::handle(&self.d, p, &mut self.session, &line).expect("a reply");
        serde_json::from_str(&reply).unwrap()
    }

    fn tools(&mut self, p: &Principal) -> Vec<J> {
        let mut all = Vec::new();
        let mut cursor = J::Null;
        loop {
            let params = if cursor.is_null() {
                json!({})
            } else {
                json!({"cursor": cursor})
            };
            let r = self.rpc(p, "tools/list", params);
            all.extend(r["result"]["tools"].as_array().unwrap().clone());
            match r["result"].get("nextCursor") {
                Some(c) => cursor = c.clone(),
                None => return all,
            }
        }
    }

    fn call(&mut self, p: &Principal, tool: &str, args: J) -> J {
        self.rpc(p, "tools/call", json!({"name": tool, "arguments": args}))
    }
}

fn names(tools: &[J]) -> Vec<String> {
    let mut v: Vec<String> = tools
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .filter(|n| n.starts_with("instance_brand__"))
        .collect();
    v.sort();
    v
}

fn keys(v: &J) -> Vec<String> {
    let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
    k.sort();
    k
}

// ---- the model's language fields -----------------------------------------------------------------

#[test]
fn the_models_language_fields_equal_the_languages_of_the_oauth_pages() {
    let model = obop_pkg::brand::instance_brand_model();
    let suffixes = |prefix: &str| -> Vec<String> {
        model
            .fields
            .iter()
            .filter_map(|f| f.name.strip_prefix(prefix).map(String::from))
            .collect()
    };
    let pages: Vec<String> = obop_oauth::page::LANGS
        .iter()
        .map(|l| l.to_string())
        .collect();
    assert_eq!(pages.len(), 7);
    assert_eq!(
        suffixes("title_"),
        pages,
        "title_<lang>, in the order of LANGS"
    );
    assert_eq!(
        suffixes("message_"),
        pages,
        "message_<lang>, in the order of LANGS"
    );
    // The reader of the record and the model's own list are the same list.
    assert_eq!(
        obop_commands::brand::LANGS.to_vec(),
        obop_oauth::page::LANGS.to_vec()
    );
    // The writable fields of `configure` hold one title and one message for each of them.
    let writable = obop_commands::brand::writable_fields();
    for l in obop_oauth::page::LANGS {
        assert!(writable.contains(&format!("title_{l}")), "{l}");
        assert!(writable.contains(&format!("message_{l}")), "{l}");
    }
}

// ---- REST ------------------------------------------------------------------------------------------

#[tokio::test]
async fn rest_get_configure_and_clear_for_a_person_with_platform_admin() {
    let app = App::new();
    let r = router(&app);
    let (s, body) = post(&r, "admin", "get", json!({})).await;
    assert_eq!(s, 200, "{body}");
    assert_eq!(body["value"]["set"], json!(false));
    assert_eq!(
        body["denied"],
        json!([]),
        "no key of the answer is withheld"
    );
    let (s, body) = post(&r, "admin", "configure", json!({"name": "MOHV"})).await;
    assert_eq!(s, 200, "{body}");
    assert_eq!(body["denied"], json!([]));
    assert_eq!(keys(&body["value"]), ["changes", "v"]);
    let (_, body) = post(&r, "admin", "get", json!({})).await;
    assert_eq!(body["value"]["set"], json!(true));
    assert_eq!(body["value"]["name"], json!("MOHV"));
    let (s, body) = post(&r, "admin", "clear", json!({})).await;
    assert_eq!(s, 200, "{body}");
    assert_eq!(body["value"], json!({"cleared": true}));
    let (_, body) = post(&r, "admin", "clear", json!({})).await;
    assert_eq!(body["value"], json!({"cleared": false}));
}

/// The reader that the logo route and the OAuth pages (slice S1c) ask is the dispatcher's own (the
/// signer page, the connector pages, the enrolment texts, the public site and the shop, slices S1d
/// to S1f, do not ask it yet), and `configure` and `clear` tell it of a change that committed: it
/// shows at once in the process that made it, whatever surface the call came by
/// (docs/spec/branding.md, section 13, "Cache"). The cell is filled first, because a value in it is
/// good for two seconds and a change that did not reach it would not show.
#[tokio::test]
async fn rest_configure_and_clear_show_in_the_reader_of_the_dispatcher_at_once() {
    let app = App::new();
    let r = router(&app);
    let reader = app.dispatcher.brand();
    let before = reader.current();
    assert_eq!(before.brand().name(), "obop");
    assert!(!before.is_set());

    let (s, out) = post(&r, "admin", "configure", json!({"name": "MOHV"})).await;
    assert_eq!(s, 200, "{out}");
    let shown = reader.current();
    assert_eq!(shown.brand().name(), "MOHV");
    assert!(shown.is_set());

    let (s, out) = post(
        &r,
        "admin",
        "configure",
        json!({"name": "Zed", "title_sv": "Logga in"}),
    )
    .await;
    assert_eq!(s, 200, "{out}");
    let shown = reader.current();
    assert_eq!(shown.brand().name(), "Zed");
    assert_eq!(shown.brand().title("sv"), "Logga in");

    // A dry run, and a refusal, move nothing.
    post(
        &r,
        "admin",
        "configure",
        json!({"name": "Other", "dry_run": true}),
    )
    .await;
    let (s, _) = post(&r, "admin", "configure", json!({"name": "a:b"})).await;
    assert_eq!(s, 400);
    assert_eq!(reader.current().brand().name(), "Zed");

    let (s, out) = post(&r, "admin", "clear", json!({})).await;
    assert_eq!(s, 200, "{out}");
    let shown = reader.current();
    assert_eq!(shown.brand().name(), "obop");
    assert!(!shown.is_set());
}

/// A client cannot send the keys the dispatcher adds: a `_caller` in the body is dropped, and the
/// event of the change names the person who signed in.
#[tokio::test]
async fn rest_a_caller_sent_by_the_client_does_not_change_the_actor_of_the_event() {
    let app = App::new();
    let r = router(&app);
    let (s, out) = post(
        &r,
        "admin",
        "configure",
        json!({"name": "MOHV", "_caller": "mallory", "_actor": "mallory", "_actor_groups": "x"}),
    )
    .await;
    assert_eq!(s, 200, "{out}");
    let events = app.store.backend().list("security_event").unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].1["kind"], json!("brand_configured"));
    assert_eq!(events[0].1["actor"], json!("ada"));
}

#[tokio::test]
async fn rest_refuses_an_agent_a_scoped_token_a_mobile_token_and_a_person_without_the_group() {
    let app = App::new();
    let r = router(&app);
    for token in ["agent", "scoped", "mobile"] {
        for (command, body) in [
            ("configure", json!({"name": "MOHV"})),
            ("configure", json!({"name": "MOHV", "dry_run": true})),
            ("clear", json!({})),
        ] {
            let (s, out) = post(&r, token, command, body).await;
            assert_eq!(s, 400, "{token} {command}: {out}");
            let error = out["error"].as_str().unwrap();
            assert!(error.contains("by a person"), "{token} {command}: {out}");
            assert!(
                error.contains("the branding is public") && !error.contains("pages"),
                "{token} {command}: {out}"
            );
            assert!(out.get("plan_id").is_none(), "no plan is made: {out}");
        }
    }
    // A person without the group gets the answer of a command that does not exist: the REST
    // routes do not tell a missing command from a forbidden one.
    let (_, nothing) = post(&r, "admin", "no_such_command", json!({})).await;
    for command in ["configure", "clear"] {
        let (s, out) = post(&r, "bob", command, json!({"name": "x"})).await;
        assert_eq!(s, 404, "bob {command}: {out}");
        assert_eq!(out, nothing, "bob {command}");
    }
    let (s, _) = post(&r, "nobody", "get", json!({})).await;
    assert_eq!(s, 401, "an unknown token is no one");
    // And nothing was stored.
    let (_, body) = post(&r, "admin", "get", json!({})).await;
    assert_eq!(body["value"]["set"], json!(false));
}

#[tokio::test]
async fn rest_get_is_open_to_every_signed_in_principal() {
    let app = App::new();
    let r = router(&app);
    post(&r, "admin", "configure", json!({"name": "MOHV"})).await;
    for token in ["admin", "agent", "scoped", "mobile", "bob"] {
        let (s, body) = post(&r, token, "get", json!({})).await;
        assert_eq!(s, 200, "{token}: {body}");
        assert_eq!(body["value"]["name"], json!("MOHV"), "{token}");
        assert_eq!(body["denied"], json!([]), "{token}");
    }
}

#[tokio::test]
async fn rest_refuses_unknown_keys_by_name_and_accepts_a_bool_as_text_or_json() {
    let app = App::new();
    let r = router(&app);
    // The keys the dispatcher adds itself pass; no other key that starts with `_` does.
    for key in [
        "v",
        "id",
        "set",
        "bogus",
        "hide_product_docs",
        "_foo",
        "_caller2",
    ] {
        let (s, out) = post(&r, "admin", "configure", json!({"name": "MOHV", key: "x"})).await;
        assert_eq!(s, 400, "{key}: {out}");
        assert!(
            out["error"]
                .as_str()
                .unwrap()
                .contains(&format!("the key {key} is not accepted")),
            "{key}: {out}"
        );
        assert!(out.get("code").is_none(), "{key}: {out}");
        let (s, out) = post(&r, "admin", "clear", json!({key: "x"})).await;
        assert_eq!(s, 400, "{key}: {out}");
        assert!(out["error"].as_str().unwrap().contains(key), "{key}: {out}");
    }
    // A key that spells an error code is only a key: the answer is the plain 400 of a refusal.
    let (s, out) = post(
        &r,
        "admin",
        "configure",
        json!({"name": "MOHV", "denied": "x"}),
    )
    .await;
    assert_eq!(s, 400, "{out}");
    assert!(out.get("code").is_none(), "{out}");
    let (_, body) = post(&r, "admin", "get", json!({})).await;
    assert_eq!(body["value"]["set"], json!(false), "nothing was stored");
    // dry_run as the text `obop call` sends, and as the JSON boolean.
    for dry in [json!("true"), json!(true)] {
        let (s, out) = post(
            &r,
            "admin",
            "configure",
            json!({"name": "MOHV", "dry_run": dry}),
        )
        .await;
        assert_eq!(s, 200, "{out}");
        assert_eq!(out["value"]["dry_run"], json!(true));
    }
    let (_, body) = post(&r, "admin", "get", json!({})).await;
    assert_eq!(
        body["value"]["set"],
        json!(false),
        "a dry run writes nothing"
    );
    let (s, out) = post(
        &r,
        "admin",
        "configure",
        json!({"name": "MOHV", "dry_run": "maybe"}),
    )
    .await;
    assert_eq!(s, 400, "{out}");
    assert!(
        out["error"]
            .as_str()
            .unwrap()
            .contains("dry_run must be true or false")
    );
}

#[tokio::test]
async fn rest_a_refusal_lists_every_refusal_and_dry_run_gives_the_typed_list() {
    let app = App::new();
    let r = router(&app);
    let body = json!({"name": "Foo: Bar", "title_de": "<b>"});
    let (s, out) = post(&r, "admin", "configure", body.clone()).await;
    assert_eq!(s, 400, "{out}");
    let text = out["error"].as_str().unwrap();
    assert_eq!(text.lines().count(), 2, "{text}");
    assert!(
        text.starts_with("name: ") && text.contains("\ntitle_de: "),
        "{text}"
    );
    let mut dry = body;
    dry["dry_run"] = json!(true);
    let (s, out) = post(&r, "admin", "configure", dry).await;
    assert_eq!(s, 200, "{out}");
    let refusals: J = serde_json::from_str(out["value"]["refusals"].as_str().unwrap()).unwrap();
    assert_eq!(refusals.as_array().unwrap().len(), 2);
    let lines: Vec<String> = refusals
        .as_array()
        .unwrap()
        .iter()
        .map(|x| {
            format!(
                "{}: {}",
                x["field"].as_str().unwrap(),
                x["rule"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(
        lines.join("\n"),
        text,
        "the same refusals, typed and as lines"
    );
}

#[tokio::test]
async fn rest_only_these_commands_exist_and_the_model_is_in_no_listing() {
    let app = App::new();
    let r = router(&app);
    for command in [
        "create",
        "update",
        "delete",
        "bulk_create",
        "list",
        "search",
    ] {
        let (s, out) = post(&r, "admin", command, json!({"name": "x"})).await;
        assert_eq!(s, 404, "{command}: {out}");
        assert_eq!(out, json!({"error": "not found"}), "{command}: {out}");
    }
    let (s, out) = rest(&r, "GET", "admin", "/api/commands", json!({})).await;
    assert_eq!(s, 200);
    let mut ids: Vec<String> = out
        .as_array()
        .or_else(|| out["commands"].as_array())
        .unwrap_or_else(|| panic!("{out}"))
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_string())
        .filter(|i| i.starts_with("instance_brand."))
        .collect();
    ids.sort();
    assert_eq!(
        ids,
        [
            "instance_brand.clear",
            "instance_brand.configure",
            "instance_brand.get"
        ]
    );
    let (_, out) = rest(&r, "GET", "bob", "/api/commands", json!({})).await;
    let bobs: Vec<String> = out
        .as_array()
        .or_else(|| out["commands"].as_array())
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_string())
        .filter(|i| i.starts_with("instance_brand."))
        .collect();
    assert_eq!(
        bobs,
        ["instance_brand.get"],
        "any other principal gains get only"
    );
    // The model is in neither the metadata nor the authenticated OpenAPI document.
    for token in ["admin", "bob"] {
        let (s, meta) = rest(&r, "GET", token, "/api/meta", json!({})).await;
        assert_eq!(s, 200);
        assert!(
            !meta.to_string().contains("instance_brand"),
            "{token}: meta"
        );
        let (s, doc) = rest(&r, "GET", token, "/api/openapi.json", json!({})).await;
        assert_eq!(s, 200);
        assert!(
            !doc.to_string().contains("instance_brand"),
            "{token}: openapi"
        );
    }
}

// ---- MCP -------------------------------------------------------------------------------------------

#[test]
fn mcp_lists_the_three_tools_to_an_administrator_and_get_to_everyone_else() {
    let app = App::new();
    let mut m = Mcp::new(&app.dispatcher, &admin());
    assert_eq!(
        names(&m.tools(&admin())),
        [
            "instance_brand__clear",
            "instance_brand__configure",
            "instance_brand__get"
        ]
    );
    // An agent token holds the group in its scope, so the tools are listed (a command is on every
    // surface or on none); the two that write refuse when they run.
    assert_eq!(names(&m.tools(&agent())).len(), 3);
    for p in [bob(), Principal::user("x", &[])] {
        assert_eq!(names(&m.tools(&p)), ["instance_brand__get"]);
    }
    let tools = m.tools(&admin());
    let get = tools
        .iter()
        .find(|t| t["name"] == "instance_brand__get")
        .unwrap();
    assert_eq!(get["annotations"]["readOnlyHint"], json!(true));
    assert!(
        !get["description"].as_str().unwrap().contains("by id"),
        "the generated text of an ordinary get is replaced"
    );
    let configure = tools
        .iter()
        .find(|t| t["name"] == "instance_brand__configure")
        .unwrap();
    let text = configure["description"].as_str().unwrap();
    for key in [
        "name",
        "slug",
        "title_<lang>",
        "message_<lang>",
        "logo_svg",
        "dry_run",
    ] {
        assert!(text.contains(key), "{key}: {text}");
    }
}

#[test]
fn mcp_only_these_tools_exist() {
    let app = App::new();
    let mut m = Mcp::new(&app.dispatcher, &admin());
    for c in [
        "create",
        "update",
        "delete",
        "bulk_create",
        "list",
        "search",
    ] {
        let r = m.call(&admin(), &format!("instance_brand__{c}"), json!({}));
        assert_eq!(r["error"]["code"], json!(-32602), "{c}: {r}");
        assert!(
            r["error"]["message"]
                .as_str()
                .unwrap()
                .contains("unknown tool")
        );
    }
}

#[test]
fn mcp_calls_answer_with_the_same_keys_and_refuse_by_name_and_by_principal() {
    let app = App::new();
    let mut m = Mcp::new(&app.dispatcher, &admin());
    let r = m.call(&admin(), "instance_brand__get", json!({}));
    assert_eq!(r["result"]["isError"], json!(false), "{r}");
    let value = &r["result"]["structuredContent"]["value"];
    assert_eq!(value["set"], json!(false));
    assert!(r["result"]["structuredContent"].get("denied").is_none());
    // Unknown keys are refused by name.
    let r = m.call(
        &admin(),
        "instance_brand__configure",
        json!({"name": "MOHV", "v": "x", "id": "instance"}),
    );
    assert_eq!(r["result"]["isError"], json!(true), "{r}");
    let text = r["result"]["structuredContent"]["error"].as_str().unwrap();
    assert!(text.contains("the key v is not accepted"), "{text}");
    assert!(text.contains("the key id is not accepted"), "{text}");
    // An agent, a scoped token and a mobile token are refused, and no plan is made.
    for p in [agent(), scoped(), mobile()] {
        let r = m.call(&p, "instance_brand__configure", json!({"name": "MOHV"}));
        assert_eq!(r["result"]["isError"], json!(true), "{r}");
        assert!(
            r["result"]["structuredContent"]["error"]
                .as_str()
                .unwrap()
                .contains("by a person"),
            "{r}"
        );
        assert!(r["result"]["structuredContent"].get("plan_id").is_none());
        let r = m.call(&p, "instance_brand__clear", json!({}));
        assert_eq!(r["result"]["isError"], json!(true), "{r}");
    }
    // A person with platform.admin may, and `get` is open to the agent.
    let r = m.call(
        &admin(),
        "instance_brand__configure",
        json!({"name": "MOHV"}),
    );
    assert_eq!(r["result"]["isError"], json!(false), "{r}");
    let r = m.call(&agent(), "instance_brand__get", json!({}));
    assert_eq!(
        r["result"]["structuredContent"]["value"]["name"],
        json!("MOHV")
    );
    assert!(
        r["result"]["structuredContent"].get("denied").is_none(),
        "{r}"
    );
    // A person without the group does not see the tool at all.
    let r = m.call(&bob(), "instance_brand__clear", json!({}));
    assert_eq!(r["error"]["code"], json!(-32602), "{r}");
}

// ---- parity: the CLI, REST and MCP --------------------------------------------------------------------

/// The `value` of a call through the dispatcher with the surface of the command line, as `obop
/// call` makes it: every input is text.
fn cli(d: &Dispatcher, command: &str, pairs: &[(&str, &str)]) -> Result<J, String> {
    let input = Value::Map(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), Value::text(v)))
            .collect(),
    );
    match d.call(
        Surface::Cli,
        &admin(),
        &mut Vault::new(),
        &format!("instance_brand.{command}"),
        &input,
    ) {
        Ok(r) => {
            assert!(r.denied.is_empty(), "{:?}", r.denied);
            Ok(r.value.to_json())
        }
        Err(CallError::Handler(m)) => Err(m),
        Err(e) => panic!("{e:?}"),
    }
}

fn mcp_value(m: &mut Mcp, command: &str, args: J) -> Result<J, String> {
    let r = m.call(&admin(), &format!("instance_brand__{command}"), args);
    let sc = &r["result"]["structuredContent"];
    if r["result"]["isError"] == json!(true) {
        return Err(sc["error"].as_str().unwrap().to_string());
    }
    Ok(sc["value"].clone())
}

async fn rest_value(r: &axum::Router, command: &str, args: J) -> Result<J, String> {
    let (s, out) = post(r, "admin", command, args).await;
    if s == 200 {
        assert_eq!(out["denied"], json!([]));
        Ok(out["value"].clone())
    } else {
        Err(out["error"].as_str().unwrap().to_string())
    }
}

#[tokio::test]
async fn the_answers_of_get_configure_and_clear_are_the_same_on_the_cli_rest_and_mcp() {
    let app = App::new();
    let r = router(&app);
    let mut m = Mcp::new(&app.dispatcher, &admin());
    let d = &app.dispatcher;
    let args = json!({"name": "MOHV", "title_sv": "Logga in", "dry_run": "true"});
    let pairs = [
        ("name", "MOHV"),
        ("title_sv", "Logga in"),
        ("dry_run", "true"),
    ];

    // configure with dry_run: identical answers, the typed `changes` and `refusals` included.
    let c = cli(d, "configure", &pairs).unwrap();
    let h = rest_value(&r, "configure", args.clone()).await.unwrap();
    let t = mcp_value(&mut m, "configure", args).unwrap();
    assert_eq!(c, h);
    assert_eq!(c, t);
    assert_eq!(keys(&c), ["changes", "dry_run", "refusals", "v"]);

    // A refusal without dry_run carries every refusal, the same lines on the three surfaces.
    let bad = [("name", "Foo: Bar"), ("title_de", "<b>")];
    let c = cli(d, "configure", &bad).unwrap_err();
    let h = rest_value(
        &r,
        "configure",
        json!({"name": "Foo: Bar", "title_de": "<b>"}),
    )
    .await
    .unwrap_err();
    let t = mcp_value(
        &mut m,
        "configure",
        json!({"name": "Foo: Bar", "title_de": "<b>"}),
    )
    .unwrap_err();
    assert_eq!(c, h);
    assert_eq!(c, t);
    assert_eq!(c.lines().count(), 2);

    // The same unknown key is refused by name, the same way.
    let c = cli(d, "configure", &[("name", "MOHV"), ("v", "x")]).unwrap_err();
    let h = rest_value(&r, "configure", json!({"name": "MOHV", "v": "x"}))
        .await
        .unwrap_err();
    let t = mcp_value(&mut m, "configure", json!({"name": "MOHV", "v": "x"})).unwrap_err();
    assert_eq!(c, h);
    assert_eq!(c, t);

    // get and clear with no row.
    let c = cli(d, "get", &[]).unwrap();
    let h = rest_value(&r, "get", json!({})).await.unwrap();
    let t = mcp_value(&mut m, "get", json!({})).unwrap();
    assert_eq!(c, h);
    assert_eq!(c, t);
    let c = cli(d, "clear", &[("dry_run", "true")]).unwrap();
    let h = rest_value(&r, "clear", json!({"dry_run": "true"}))
        .await
        .unwrap();
    let t = mcp_value(&mut m, "clear", json!({"dry_run": true})).unwrap();
    assert_eq!(c, json!({"cleared": false, "dry_run": true}));
    assert_eq!(c, h);
    assert_eq!(c, t);

    // A real configure through each surface in turn: the same keys, and a state that every
    // surface reads the same.
    let c = cli(d, "configure", &[("name", "MOHV")]).unwrap();
    assert_eq!(keys(&c), ["changes", "v"]);
    let h = rest_value(&r, "configure", json!({"title_sv": "Logga in"}))
        .await
        .unwrap();
    assert_eq!(keys(&h), ["changes", "v"]);
    let t = mcp_value(&mut m, "configure", json!({"title_de": "Anmelden"})).unwrap();
    assert_eq!(keys(&t), ["changes", "v"]);
    let c = cli(d, "get", &[]).unwrap();
    let h = rest_value(&r, "get", json!({})).await.unwrap();
    let t = mcp_value(&mut m, "get", json!({})).unwrap();
    assert_eq!(c, h);
    assert_eq!(c, t);
    assert_eq!(c["title_sv"], json!("Logga in"));
    assert_eq!(c["title_de"], json!("Anmelden"));
    // Every key of get is a declared field, so none is withheld on any surface.
    let declared: Vec<String> = obop_pkg::brand::instance_brand_model()
        .fields
        .iter()
        .map(|f| f.name.to_string())
        .collect();
    for k in keys(&c) {
        assert!(declared.contains(&k), "{k}");
    }
    let c = cli(d, "clear", &[]).unwrap();
    assert_eq!(c, json!({"cleared": true}));
    let h = rest_value(&r, "clear", json!({})).await.unwrap();
    assert_eq!(h, json!({"cleared": false}), "the row is gone");
}

#[test]
fn a_bool_given_as_the_text_true_or_false_is_accepted_on_the_command_line_surface() {
    let app = App::new();
    let d = &app.dispatcher;
    let on = cli(d, "configure", &[("name", "MOHV"), ("dry_run", "true")]).unwrap();
    assert_eq!(on["dry_run"], json!(true));
    let off = cli(d, "configure", &[("name", "MOHV"), ("dry_run", "false")]).unwrap();
    assert!(off.get("dry_run").is_none());
    assert_eq!(cli(d, "get", &[]).unwrap()["set"], json!(true));
    let m = cli(d, "clear", &[("dry_run", "perhaps")]).unwrap_err();
    assert!(m.contains("dry_run must be true or false"), "{m}");
}

#[test]
fn obop_call_get_works_on_the_command_line_and_obop_import_of_the_model_is_refused() {
    let dir = std::env::temp_dir().join(format!(
        "obop-brand-cli-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    sweep_scratch();
    std::fs::create_dir_all(&dir).unwrap();
    // Removes the directory when the test ends, by a failed assertion too. On Windows the removal
    // fails with a sharing violation while the process lives, even after every handle this test
    // holds is dropped (the applications it opens keep the database file), so a run cannot always
    // clean up after itself; `sweep_scratch` of the next run does.
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _scratch = Scratch(dir.clone());
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
    // `obop call instance_brand get`: any signed-in principal reads it, a dev principal included.
    let (value, denied) = crate::call_json(&o, "instance_brand", "get", &[]).unwrap();
    assert!(denied.is_empty(), "{denied:?}");
    assert_eq!(value["set"], json!(false));
    // The six generated commands do not exist at a terminal either. `update` and `delete` are given
    // the id of the one row, so a refusal for a missing id cannot be what is seen: each answers
    // exactly as a command that was never defined does, and `call_json` stops with that status.
    let hidden_status = {
        let app = crate::open_app(&o).expect("the database opens");
        let p = crate::principal(&app, &o).expect("the dev principal");
        let answer = |command: &str| {
            let input = Value::map([("id", Value::text("instance")), ("name", Value::text("x"))]);
            crate::dispatch_call(&app, &p, &format!("instance_brand.{command}"), &input)
        };
        let line = |c: &str| format!("error: UnknownCommand(\"instance_brand.{c}\")");
        let nothing = answer("no_such_command").expect_err("a command that does not exist");
        assert_eq!(nothing.0, line("no_such_command"));
        for c in [
            "create",
            "update",
            "delete",
            "bulk_create",
            "list",
            "search",
        ] {
            assert_eq!(answer(c).expect_err(c), (line(c), nothing.1), "{c}");
        }
        // And the commands that are bound do work for the same principal.
        let (value, _) = crate::dispatch_call(&app, &p, "instance_brand.get", &Value::map([]))
            .expect("get is bound");
        assert_eq!(value["set"], json!(false));
        nothing.1
    };
    for c in ["create", "update", "delete", "list", "search"] {
        assert!(
            crate::call_json(&o, "instance_brand", c, &["id=instance", "name=x"]).is_err(),
            "{c}"
        );
    }
    // `obop import instance_brand FILE.csv` is `instance_brand.bulk_create`, which is not bound.
    let file = path("brand.csv");
    std::fs::write(&file, "name\nMOHV\n").unwrap();
    let run = || {
        crate::import_cmd::run(
            &o,
            &crate::import_cmd::Args {
                model: "instance_brand",
                file: &file,
                mode: None,
                dry_run: false,
                key: None,
                source: None,
                on_existing: None,
            },
        )
    };
    // It stops with the status of a command that does not exist, and nothing was stored.
    let refused = run().expect_err("the import is refused");
    assert_eq!(
        format!("{refused:?}"),
        format!("{:?}", std::process::ExitCode::from(hidden_status)),
        "the status of an unknown command"
    );
    let after = obop_storage::open(&db).unwrap();
    assert_eq!(
        after.count("instance_brand").expect("the store counts"),
        0,
        "and nothing was stored"
    );
    // The directory goes when `_scratch` is dropped, after the connection of `after`.
}

/// Removes the scratch directories that earlier runs of the test above left behind: those of
/// another process that are more than ten minutes old, so a run in parallel keeps its own. Where
/// the directory is still in use, the removal fails and the directory stays for the next run.
fn sweep_scratch() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some((pid, nanos)) = name
            .strip_prefix("obop-brand-cli-")
            .and_then(|rest| rest.split_once('-'))
        else {
            continue;
        };
        let (Ok(pid), Ok(nanos)) = (pid.parse::<u32>(), nanos.parse::<u128>()) else {
            continue;
        };
        if pid != std::process::id() && now.saturating_sub(nanos) > 600_000_000_000 {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}
