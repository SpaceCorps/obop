//! MCP server core over the same `Dispatcher` as REST and the CLI (D18, D29, D51).
//!
//! This crate is transport-free: it turns one JSON-RPC payload into a reply. `serve_stdio` is the
//! stdio transport; `obop-http` mounts the Streamable HTTP transport (`/mcp`) over the same
//! `handle_payload`, so tool listing and calling are one code path.
//!
//! Protocol: the handshake-based revisions `2025-11-25`, `2025-06-18` and `2025-03-26`. The
//! stateless `2026-07-28` revision is not implemented (docs/DECISIONS.md D51).

pub mod schema;

use obop_commands::{CallError, Dispatcher, Principal, Surface, Value, Vault, names};
use serde_json::{Value as J, json};
use std::io::{BufRead, Write};

/// Revisions this server speaks, newest first.
pub const SUPPORTED_VERSIONS: [&str; 3] = ["2025-11-25", "2025-06-18", "2025-03-26"];
/// Tools per `tools/list` page; longer lists are paged with an opaque cursor.
pub const PAGE_SIZE: usize = 200;

pub const INSTRUCTIONS: &str = "This is an obop instance: an open-source business suite (contacts, CRM, sales, invoicing and installed modules) driven by typed commands. \
Call obop__meta first to learn the models, fields and actions you may use. \
Every tool is named <model>__<command>: list, search, get, create, update, delete and model-specific actions. Prefer search with a small limit, filter and order over list. \
Money is an integer in minor units (cents); dates are YYYY-MM-DD; a ref field takes the id of the target record. \
Personal data comes back as mask tokens such as NAME_1; send the token back unchanged where you need that person or value, it is the same token for the whole session. \
A result may list denied fields: you are not allowed to read them. \
Irreversible or money-moving steps, and every delete, do not run for an agent: the result has status approval_required and a plan_id. Tell a human to approve that plan in the back office and do not retry the call. An agent can never approve its own plan.";

/// The newest revision whose name is `v`, if supported.
fn supported(v: &str) -> Option<&'static str> {
    SUPPORTED_VERSIONS.iter().copied().find(|s| *s == v)
}

/// Whether `v` is a revision this server speaks (HTTP checks the `MCP-Protocol-Version` header).
pub fn is_supported(v: &str) -> bool {
    supported(v).is_some()
}

/// State of one MCP session: the masking vault (mask tokens stay stable and restorable for the
/// whole session, D40) and what was negotiated.
pub struct Session {
    pub vault: Vault,
    pub protocol: Option<&'static str>,
    pub initialized: bool,
    pub client: Option<String>,
}

impl Session {
    pub fn new() -> Self {
        Self {
            vault: Vault::new(),
            protocol: None,
            initialized: false,
            client: None,
        }
    }
}

impl Session {
    /// The session as JSON, for a store shared between server processes. It carries the masking
    /// vault, so it holds personal data: seal it before it is kept.
    pub fn to_json(&self) -> J {
        json!({
            "protocol": self.protocol,
            "initialized": self.initialized,
            "client": self.client,
            "vault": self.vault.to_json(),
        })
    }

    /// A session from `to_json` output; an unknown protocol revision reads as not negotiated.
    pub fn from_json(j: &J) -> Session {
        Session {
            vault: Vault::from_json(&j["vault"]),
            protocol: j["protocol"].as_str().and_then(supported),
            initialized: j["initialized"].as_bool().unwrap_or(false),
            client: j["client"].as_str().map(String::from),
        }
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

/// What to send back for one payload.
#[derive(Debug, PartialEq)]
pub enum Reply {
    /// Notifications and client responses get no JSON-RPC answer (HTTP 202).
    Accepted,
    Json(J),
}

fn ok(id: &J, result: J) -> J {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

pub fn error(id: &J, code: i64, message: &str) -> J {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// The JSON-RPC error code of a reply, if it is a single error response.
pub fn error_code(reply: &J) -> Option<i64> {
    reply["error"]["code"].as_i64()
}

fn is_initialize(m: &J) -> bool {
    m.get("method").and_then(J::as_str) == Some("initialize")
}

/// `true` for a single `initialize` request (the HTTP transport creates the session for it).
pub fn is_initialize_request(payload: &J) -> bool {
    is_initialize(payload) && payload.get("id").is_some()
}

/// Handle one parsed payload: a message, or a batch where the negotiated revision allows it
/// (only `2025-03-26`; later revisions removed batching).
pub fn handle_payload(d: &Dispatcher, p: &Principal, s: &mut Session, payload: &J) -> Reply {
    match payload {
        J::Array(items) => {
            if s.protocol != Some("2025-03-26") {
                return Reply::Json(error(
                    &J::Null,
                    -32600,
                    "JSON-RPC batches are not supported by this protocol revision",
                ));
            }
            if items.is_empty() || items.iter().any(is_initialize) {
                return Reply::Json(error(
                    &J::Null,
                    -32600,
                    "invalid batch: it must be non-empty and must not contain initialize",
                ));
            }
            let out: Vec<J> = items
                .iter()
                .filter_map(|m| handle_message(d, p, s, m))
                .collect();
            if out.is_empty() {
                Reply::Accepted
            } else {
                Reply::Json(J::Array(out))
            }
        }
        m => handle_message(d, p, s, m).map_or(Reply::Accepted, Reply::Json),
    }
}

/// Handle one JSON-RPC line (stdio). Returns the line to write, or `None` for notifications.
pub fn handle(d: &Dispatcher, p: &Principal, s: &mut Session, line: &str) -> Option<String> {
    let payload: J = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return Some(error(&J::Null, -32700, "parse error").to_string()),
    };
    match handle_payload(d, p, s, &payload) {
        Reply::Accepted => None,
        Reply::Json(j) => Some(j.to_string()),
    }
}

fn handle_message(d: &Dispatcher, p: &Principal, s: &mut Session, msg: &J) -> Option<J> {
    let Some(obj) = msg.as_object() else {
        return Some(error(&J::Null, -32600, "invalid request"));
    };
    if obj.get("jsonrpc").and_then(J::as_str) != Some("2.0") {
        return Some(error(
            obj.get("id").unwrap_or(&J::Null),
            -32600,
            "invalid request: jsonrpc must be \"2.0\"",
        ));
    }
    let Some(method) = obj.get("method").and_then(J::as_str) else {
        // A response to a request the server made; this server makes none. Accept and drop.
        if obj.contains_key("id") && (obj.contains_key("result") || obj.contains_key("error")) {
            return None;
        }
        return Some(error(&J::Null, -32600, "invalid request: no method"));
    };
    let Some(id) = obj.get("id") else {
        notification(s, method);
        return None;
    };
    if !(id.is_string() || id.is_number()) {
        return Some(error(
            &J::Null,
            -32600,
            "invalid request: id must be a string or number",
        ));
    }
    let params = obj.get("params").cloned().unwrap_or(J::Null);
    Some(match method {
        "initialize" => initialize(id, s, &params),
        "ping" => ok(id, json!({})),
        "tools/list" => match list_tools(d, p, &params) {
            Ok(r) => ok(id, r),
            Err((c, m)) => error(id, c, &m),
        },
        "tools/call" => match call_tool(d, p, s, &params) {
            Ok(r) => ok(id, r),
            Err((c, m)) => error(id, c, &m),
        },
        _ => error(id, -32601, "method not found"),
    })
}

fn notification(s: &mut Session, method: &str) {
    if method == "notifications/initialized" {
        s.initialized = true;
    }
    // notifications/cancelled, notifications/roots/list_changed and the rest need no action:
    // every call finishes before the next one is read.
}

fn initialize(id: &J, s: &mut Session, params: &J) -> J {
    let Some(asked) = params.get("protocolVersion").and_then(J::as_str) else {
        return error(id, -32602, "initialize needs params.protocolVersion");
    };
    // Lifecycle: answer the requested revision when supported, otherwise our newest.
    let version = supported(asked).unwrap_or(SUPPORTED_VERSIONS[0]);
    s.protocol = Some(version);
    s.client = params
        .pointer("/clientInfo/name")
        .and_then(J::as_str)
        .map(|n| n.chars().take(100).collect());
    ok(
        id,
        json!({
            "protocolVersion": version,
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {
                "name": "obop",
                "title": "obop",
                "version": obop_types::CORE_VERSION,
            },
            "instructions": INSTRUCTIONS,
        }),
    )
}

type RpcError = (i64, String);

fn list_tools(d: &Dispatcher, p: &Principal, params: &J) -> Result<J, RpcError> {
    page(&schema::tools(d, p), params, PAGE_SIZE)
}

/// One `tools/list` page. The cursor is the opaque offset of the next page.
fn page(tools: &[J], params: &J, size: usize) -> Result<J, RpcError> {
    let start = match params.get("cursor") {
        None | Some(J::Null) => 0,
        Some(c) => c
            .as_str()
            .and_then(|c| c.strip_prefix('o'))
            .and_then(|n| n.parse::<usize>().ok())
            .filter(|n| *n <= tools.len())
            .ok_or((-32602, "invalid cursor".to_string()))?,
    };
    let end = (start + size).min(tools.len());
    let mut out = json!({"tools": tools[start..end]});
    if end < tools.len() {
        out["nextCursor"] = J::from(format!("o{end}"));
    }
    Ok(out)
}

fn unknown_tool(name: &str) -> RpcError {
    (-32602, format!("unknown tool: {name}"))
}

/// A tool result with structured content and its JSON text.
fn tool_result(structured: J, is_error: bool) -> J {
    json!({
        "content": [{"type": "text", "text": structured.to_string()}],
        "structuredContent": structured,
        "isError": is_error,
    })
}

fn call_tool(d: &Dispatcher, p: &Principal, s: &mut Session, params: &J) -> Result<J, RpcError> {
    let name = params
        .get("name")
        .and_then(J::as_str)
        .ok_or((-32602, "tools/call needs params.name".to_string()))?;
    let args = match params.get("arguments") {
        None | Some(J::Null) => json!({}),
        Some(a) if a.is_object() => a.clone(),
        Some(_) => return Err((-32602, "arguments must be an object".into())),
    };
    if name == schema::META_TOOL {
        return Ok(tool_result(meta_json(d, Surface::Mcp, p), false));
    }
    // Unknown, hidden, forbidden and not-on-this-surface all answer the same, so a caller cannot
    // probe for commands it may not see.
    let cmd = names::from_mcp(name).ok_or_else(|| unknown_tool(name))?;
    match d.call(
        Surface::Mcp,
        p,
        &mut s.vault,
        &cmd,
        &Value::from_json(&args),
    ) {
        Ok(r) => {
            let mut out = json!({"status": "ok", "value": r.value.to_json()});
            if !r.denied.is_empty() {
                out["denied"] = json!(r.denied);
            }
            Ok(tool_result(out, false))
        }
        Err(CallError::ApprovalRequired { plan_id }) => Ok(tool_result(
            json!({
                "status": "approval_required",
                "plan_id": plan_id,
                "instruction": "Nothing has run. This step needs a human: ask a person with the right permissions to approve this plan in the back office (or POST /api/approve/<plan_id>), then check the record. Do not call this tool again for the same step.",
            }),
            false,
        )),
        Err(
            CallError::Forbidden(_) | CallError::UnknownCommand(_) | CallError::NotOnSurface(_),
        ) => Err(unknown_tool(name)),
        Err(CallError::UnknownPlan(_)) => Err(unknown_tool(name)),
        Err(CallError::Restore(_)) => Ok(tool_result(json!({"error": "invalid token"}), true)),
        Err(CallError::Handler(m)) => Ok(tool_result(json!({"error": m}), true)),
    }
}

/// The metadata `GET /api/meta` and the `obop__meta` tool return: modules, models, readable
/// fields and actions for this caller.
pub fn meta_json(d: &Dispatcher, surface: Surface, p: &Principal) -> J {
    let modules: Vec<J> = d
        .meta(surface, p)
        .into_iter()
        .map(|m| {
            let mut mo = json!({
                "id": m.id,
                "name": m.name,
                "models": m.models.iter().map(|x| {
                    let mut o = json!({
                        "name": x.name,
                        "description": x.description,
                        "can_create": x.can_create,
                        "can_update": x.can_update,
                        "can_delete": x.can_delete,
                        "fields": x.fields.iter().map(|f| {
                            let mut o = json!({
                                "name": f.name, "description": f.description,
                                "class": f.class, "personal_data": f.personal_data,
                                "kind": f.kind, "required": f.required,
                            });
                            if f.kind == "select" {
                                o["options"] = json!(f.options);
                            }
                            if let Some(to) = f.to {
                                o["to"] = json!(to);
                            }
                            translations(&mut o, &f.labels, &f.descriptions);
                            if !f.option_labels.is_empty() {
                                o["option_labels"] = json!(f.option_labels);
                            }
                            o
                        }).collect::<Vec<_>>(),
                        "actions": x.actions.iter().map(|a| {
                            let mut o = json!({
                                "id": a.id, "description": a.description, "needs_approval": a.needs_approval,
                                "field": a.transition.map(|t| t.field),
                                "from": a.transition.and_then(|t| t.from),
                                "to": a.transition.map(|t| t.to),
                            });
                            translations(&mut o, &a.labels, &a.descriptions);
                            o
                        }).collect::<Vec<_>>(),
                    });
                    translations(&mut o, &x.labels, &x.descriptions);
                    o
                }).collect::<Vec<_>>(),
            });
            if !m.description.is_empty() {
                mo["description"] = json!(m.description);
            }
            translations(&mut mo, &m.labels, &m.descriptions);
            mo
        })
        .collect();
    json!({"core_version": obop_types::CORE_VERSION, "modules": modules})
}

/// `labels` and `descriptions` (language -> text, D52), only when there is something to say.
fn translations(
    o: &mut J,
    labels: &std::collections::BTreeMap<String, String>,
    descriptions: &std::collections::BTreeMap<String, String>,
) {
    if !labels.is_empty() {
        o["labels"] = json!(labels);
    }
    if !descriptions.is_empty() {
        o["descriptions"] = json!(descriptions);
    }
}

/// The stdio transport: newline-delimited JSON-RPC on standard input and output.
pub fn serve_stdio(d: &Dispatcher, p: &Principal) -> std::io::Result<()> {
    let mut session = Session::new();
    let stdout = std::io::stdout();
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(resp) = handle(d, p, &mut session, &line) {
            let mut out = stdout.lock();
            writeln!(out, "{resp}")?;
            out.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(n: usize) -> Vec<J> {
        (0..n).map(|i| json!({"name": format!("t{i}")})).collect()
    }

    #[test]
    fn tools_list_pages_with_opaque_cursors() {
        let all = items(7);
        let first = page(&all, &json!({}), 3).unwrap();
        assert_eq!(first["tools"].as_array().unwrap().len(), 3);
        let c = first["nextCursor"].as_str().unwrap().to_string();
        let second = page(&all, &json!({"cursor": c}), 3).unwrap();
        assert_eq!(second["tools"][0]["name"], "t3");
        let last = page(&all, &json!({"cursor": second["nextCursor"]}), 3).unwrap();
        assert_eq!(last["tools"].as_array().unwrap().len(), 1);
        assert!(last.get("nextCursor").is_none());
        // An exact multiple has no dangling cursor; bad cursors are invalid params.
        assert!(
            page(&items(3), &json!({}), 3)
                .unwrap()
                .get("nextCursor")
                .is_none()
        );
        for bad in [json!("x"), json!("o99"), json!(5), json!("o-1")] {
            assert_eq!(
                page(&all, &json!({"cursor": bad}), 3).unwrap_err().0,
                -32602
            );
        }
    }

    #[test]
    fn batches_need_the_old_revision_and_never_hold_initialize() {
        let d = Dispatcher::new();
        let p = Principal::user("u", &[]);
        let mut s = Session::new();
        let batch = json!([{"jsonrpc": "2.0", "id": 1, "method": "ping"}]);
        assert_eq!(
            error_code(&match handle_payload(&d, &p, &mut s, &batch) {
                Reply::Json(j) => j,
                Reply::Accepted => panic!(),
            }),
            Some(-32600)
        );
        s.protocol = Some("2025-03-26");
        match handle_payload(&d, &p, &mut s, &batch) {
            Reply::Json(j) => assert_eq!(j[0]["result"], json!({})),
            Reply::Accepted => panic!(),
        }
        let init = json!([{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}]);
        assert!(
            matches!(handle_payload(&d, &p, &mut s, &init), Reply::Json(j) if error_code(&j) == Some(-32600))
        );
    }

    #[test]
    fn stdio_lines_parse_and_notifications_get_no_answer() {
        let d = Dispatcher::new();
        let p = Principal::user("u", &[]);
        let mut s = Session::new();
        assert!(
            handle(
                &d,
                &p,
                &mut s,
                r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#
            )
            .is_none()
        );
        assert!(s.initialized);
        let bad = handle(&d, &p, &mut s, "nope").unwrap();
        assert!(bad.contains("-32700"));
        let init = handle(
            &d,
            &p,
            &mut s,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        )
        .unwrap();
        assert!(init.contains("\"protocolVersion\":\"2025-06-18\""));
        assert_eq!(s.protocol, Some("2025-06-18"));
    }
}
