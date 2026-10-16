//! MCP server (JSON-RPC 2.0 over stdio) over the same `Dispatcher` as REST and the CLI (D18, D29).
//! A minimal hand-written subset: `initialize`, `tools/list` and `tools/call`. `rmcp` replaces
//! it when the HTTP transport and OAuth 2.1 arrive.

use obop_commands::{CallError, Dispatcher, Principal, Surface, Value, Vault, names};
use serde_json::{Value as J, json};
use std::io::{BufRead, Write};

const PROTOCOL: &str = "2025-06-18";

fn ok(id: &J, result: J) -> String {
    json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
}

fn err(id: &J, code: i64, message: &str) -> String {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}).to_string()
}

fn text_result(text: String, is_error: bool) -> J {
    json!({"content": [{"type": "text", "text": text}], "isError": is_error})
}

/// Handle one JSON-RPC line. Returns the response line, or `None` for notifications.
pub fn handle(d: &Dispatcher, p: &Principal, vault: &mut Vault, line: &str) -> Option<String> {
    let req: J = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(_) => return Some(err(&J::Null, -32700, "parse error")),
    };
    let id = req.get("id").cloned()?; // no id: a notification
    let method = req.get("method").and_then(J::as_str).unwrap_or("");
    Some(match method {
        "initialize" => ok(
            &id,
            json!({
                "protocolVersion": PROTOCOL,
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "obop", "version": env!("CARGO_PKG_VERSION")},
            }),
        ),
        "ping" => ok(&id, json!({})),
        "tools/list" => {
            let tools: Vec<J> = d
                .catalog(Surface::Mcp, p)
                .into_iter()
                .map(|c| {
                    let suffix = if c.needs_approval {
                        " (agents propose; a human approves)"
                    } else {
                        ""
                    };
                    json!({
                        "name": names::to_mcp(&c.id),
                        "description": format!("{}{suffix}", c.description),
                        "inputSchema": {"type": "object"},
                    })
                })
                .collect();
            ok(&id, json!({"tools": tools}))
        }
        "tools/call" => {
            let params = req.get("params").cloned().unwrap_or(J::Null);
            let Some(cmd) = params
                .get("name")
                .and_then(J::as_str)
                .and_then(names::from_mcp)
            else {
                return Some(err(&id, -32602, "unknown tool"));
            };
            let args = Value::from_json(params.get("arguments").unwrap_or(&json!({})));
            match d.call(Surface::Mcp, p, vault, &cmd, &args) {
                Ok(r) => {
                    let mut out = json!({"value": r.value.to_json()});
                    if !r.denied.is_empty() {
                        out["withheld_fields"] = json!(r.denied);
                    }
                    ok(&id, text_result(out.to_string(), false))
                }
                Err(CallError::ApprovalRequired { plan_id }) => ok(
                    &id,
                    text_result(
                        json!({"status": "approval_required", "plan_id": plan_id}).to_string(),
                        false,
                    ),
                ),
                Err(
                    CallError::Forbidden(_)
                    | CallError::UnknownCommand(_)
                    | CallError::NotOnSurface(_),
                ) => {
                    return Some(err(&id, -32602, "unknown tool"));
                }
                Err(e) => ok(&id, text_result(format!("{e:?}"), true)),
            }
        }
        _ => err(&id, -32601, "method not found"),
    })
}

pub fn serve_stdio(d: &Dispatcher, p: &Principal) -> std::io::Result<()> {
    let mut vault = Vault::new();
    let stdout = std::io::stdout();
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(resp) = handle(d, p, &mut vault, &line) {
            let mut out = stdout.lock();
            writeln!(out, "{resp}")?;
            out.flush()?;
        }
    }
    Ok(())
}
