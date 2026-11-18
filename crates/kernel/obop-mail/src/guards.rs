//! The rules the generated commands cannot state: a mail server's password is sealed on the way
//! in and its settings are checked (on create, and on update against the stored record), the
//! fields the server keeps are not taken from a client, a new host forgets the stored password,
//! an agent only proposes a change to a mail server, and `outbound_mail` is written only by
//! `send` (no generated `create`, `update` or `bulk_create`).

use crate::meta::{OUTBOUND, SERVER};
use crate::{Mail, address, check_kind};
use obop_commands::{Dispatcher, Guard, Value, Verdict};
use obop_connectors::Engine;
use serde_json::Value as J;
use std::collections::BTreeMap;
use std::sync::Arc;

type M = BTreeMap<String, Value>;

fn map(v: &Value) -> Result<M, String> {
    match v {
        Value::Map(m) => Ok(m.clone()),
        _ => Err("expected an object".into()),
    }
}

fn txt(m: &M, k: &str) -> Option<String> {
    match m.get(k) {
        Some(Value::Text(t)) => Some(t.clone()),
        Some(Value::Null) => Some(String::new()),
        Some(Value::Int(n)) => Some(n.to_string()),
        _ => None,
    }
}

fn refuse(msg: &'static str) -> Guard {
    Box::new(move |_, _| Err(msg.into()))
}

/// The value a setting has after this call: the input's, else the stored one.
fn after(m: &M, stored: &serde_json::Map<String, J>, k: &str) -> String {
    txt(m, k).unwrap_or_else(|| match stored.get(k) {
        Some(J::String(s)) => s.clone(),
        Some(J::Number(n)) => n.to_string(),
        _ => String::new(),
    })
}

/// An agent only proposes a change to a mail server: a server names where every later message
/// and the relay's stored password go, so a person approves it (as a Connectors connection
/// change that exposes more, crates/modules/connectors/src/guards.rs).
fn agent_proposes() -> Guard {
    Box::new(|p, _| {
        Ok(Verdict {
            approval: !p.is_person(),
            ..Verdict::default()
        })
    })
}

fn server_input(mail: Arc<Mail>, create: bool) -> Guard {
    Box::new(move |p, input| {
        let mut m = map(input)?;
        // Kept by the server, never taken from a client.
        for k in [
            "password_set",
            "last_test_at",
            "last_test_result",
            "test_recipient",
        ] {
            m.remove(k);
        }
        let stored = if create {
            serde_json::Map::new()
        } else {
            let id = txt(&m, "id").unwrap_or_default();
            mail.backend
                .get(SERVER, &id)?
                .ok_or_else(|| format!("no mail server {id}"))?
        };
        // The switch that lets a relay on the business's own network in (docs/spec/mail.md 6):
        // only a person turns it on; an agent cannot, not even as a proposal (an approved
        // proposal runs as the agent, so this refuses it then too). Turning it off is always
        // allowed.
        let private_on = matches!(m.get("allow_private_network"), Some(Value::Bool(true)));
        if private_on && !p.is_person() {
            return Err(
                "allow_private_network: only a person turns this on, in the Mail servers form"
                    .into(),
            );
        }
        let private_network = match m.get("allow_private_network") {
            Some(Value::Bool(b)) => *b,
            Some(_) => false,
            None => stored.get("allow_private_network") == Some(&J::Bool(true)),
        };
        let get = |k: &str| after(&m, &stored, k);
        let kind = get("kind");
        match kind.as_str() {
            "smtp" => {
                if get("host").trim().is_empty() {
                    return Err("host: name the SMTP relay".into());
                }
                let host = get("host");
                if host
                    .chars()
                    .any(|c| c.is_whitespace() || c == '/' || c == '@')
                {
                    return Err(
                        "host: a host name or address only, without a scheme or port".into(),
                    );
                }
                let port = get("port");
                if !port.is_empty() && !port.parse::<u16>().is_ok_and(|p| p > 0) {
                    return Err("port: a number from 1 to 65535".into());
                }
                if crate::smtp::Security::parse(&get("security")).is_none() {
                    return Err("security: starttls, tls or none".into());
                }
                if get("security") == "none" && !get("username").is_empty() {
                    let loopback = host == "localhost"
                        || host
                            .parse::<std::net::IpAddr>()
                            .is_ok_and(|ip| ip.is_loopback());
                    // With the switch on, the send step still sends a password without TLS
                    // only when every address of the relay is on the business's own network.
                    if !loopback && !private_network {
                        return Err("a password is never sent without TLS: choose starttls or tls, leave the user name empty, or allow the business's own network".into());
                    }
                }
            }
            "provider" => {
                let c = get("connection");
                if c.is_empty() {
                    return Err("connection: choose the Connectors connection that sends".into());
                }
                let (_, p) = mail
                    .rt
                    .connection(&c)
                    .map_err(|_| format!("connection: no connection {c}"))?;
                if p.engine != Engine::Mail {
                    return Err(format!("connection: {} does not send mail", p.name));
                }
            }
            "" if !create => {}
            _ => return Err("kind: smtp or provider".into()),
        }
        let from = get("from_address");
        address::check(from.trim()).map_err(|e| format!("from_address: {e}"))?;
        let reply = get("reply_to");
        if !reply.trim().is_empty() {
            address::check(reply.trim()).map_err(|e| format!("reply_to: {e}"))?;
        }
        address::check_name(&get("from_name")).map_err(|e| format!("from_name: {e}"))?;
        for k in get("kinds")
            .split(',')
            .map(str::trim)
            .filter(|k| !k.is_empty())
        {
            check_kind(k).map_err(|e| format!("kinds: {e}"))?;
        }
        // A new host without a new password: the stored password belongs to the old relay and is
        // never sent to another one.
        let new_host = txt(&m, "host").map(|h| h.trim().to_ascii_lowercase());
        let old_host = after(&M::new(), &stored, "host")
            .trim()
            .to_ascii_lowercase();
        let new_password = matches!(m.get("password"), Some(Value::Text(pw)) if !pw.is_empty());
        if !create && new_host.is_some_and(|h| h != old_host) && !new_password {
            m.insert("password".into(), Value::Null);
        }
        if let Some(Value::Text(pw)) = m.get("password").cloned() {
            if pw.is_empty() {
                // An empty field keeps the stored password.
                m.remove("password");
            } else {
                m.insert("password".into(), Value::Text(mail.seal_password(&pw)));
                m.insert("password_set".into(), Value::Bool(true));
            }
        } else if m.get("password") == Some(&Value::Null) {
            m.insert("password".into(), Value::Text(String::new()));
            m.insert("password_set".into(), Value::Bool(false));
        }
        if create && !m.contains_key("password_set") {
            m.insert("password_set".into(), Value::Bool(false));
        }
        Ok(Verdict {
            input: Some(Value::Map(m)),
            approval: !p.is_person(),
            ..Verdict::default()
        })
    })
}

/// A template decides the text of every later message of its kind, so an agent only proposes a
/// change of one, as it does for a mail server.
fn template_input() -> Guard {
    Box::new(move |p, input| {
        let m = map(input)?;
        if let Some(k) = txt(&m, "key") {
            check_kind(&k).map_err(|e| format!("key: {e}"))?;
        }
        if let Some(s) = txt(&m, "subject")
            && (s.contains('\n') || s.contains('\r'))
        {
            return Err("subject: one line".into());
        }
        for k in ["subject", "body"] {
            if let Some(t) = txt(&m, k) {
                let mut rest = t.as_str();
                while let Some(i) = rest.find("{{") {
                    let after = &rest[i + 2..];
                    let Some(end) = after.find("}}") else {
                        return Err(format!("{k}: {{{{ without }}}}"));
                    };
                    let name = after[..end].trim();
                    if name.is_empty()
                        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    {
                        return Err(format!(
                            "{k}: a placeholder is {{{{name}}}} with letters, digits and _"
                        ));
                    }
                    rest = &after[end + 2..];
                }
            }
        }
        Ok(Verdict {
            approval: !p.is_person(),
            ..Verdict::default()
        })
    })
}

/// The audit line of the switch `allow_private_network`: every create or update that names it
/// writes a `security_event` of kind `mail_private_network` once the change has committed (an
/// observer runs only after a command succeeded), with the server and the new value.
fn audit_private_network(mail: Arc<Mail>) -> Box<obop_commands::Observer> {
    Box::new(move |cmd, p, input, out| {
        if cmd != format!("{SERVER}.create") && cmd != format!("{SERVER}.update") {
            return;
        }
        let Value::Map(m) = input else { return };
        let Some(Value::Bool(on)) = m.get("allow_private_network") else {
            return;
        };
        let id = match (m.get("id"), out) {
            (Some(Value::Text(id)), _) => id.clone(),
            (_, Value::Map(o)) => match o.get("id") {
                Some(Value::Text(id)) => id.clone(),
                _ => String::new(),
            },
            _ => String::new(),
        };
        let detail = format!(
            "mail server {id}: a relay on the business's own network is {}",
            if *on { "allowed" } else { "refused" }
        );
        obop_commands::audit::record_event(
            &*mail.backend,
            mail.now_secs(),
            "",
            "mail_private_network",
            &p.id,
            &detail,
        );
    })
}

pub fn install(d: &Arc<Dispatcher>, mail: &Arc<Mail>) {
    d.observe("mail", audit_private_network(mail.clone()));
    d.add_guard(
        "mail",
        &format!("{SERVER}.create"),
        true,
        server_input(mail.clone(), true),
    );
    d.add_guard(
        "mail",
        &format!("{SERVER}.update"),
        true,
        server_input(mail.clone(), false),
    );
    d.add_guard("mail", &format!("{SERVER}.delete"), false, agent_proposes());
    d.add_guard(
        "mail",
        &format!("{SERVER}.bulk_create"),
        false,
        refuse("mail servers are added one at a time"),
    );
    // Refused for everyone, and said so in the metadata (`can_create`, `can_update` false), so no
    // screen offers an edit of a delivery that the server refuses.
    for c in ["create", "update", "bulk_create"] {
        d.refuse_always(
            "mail",
            &format!("{OUTBOUND}.{c}"),
            "messages are queued with outbound_mail.send",
        );
    }
    d.add_guard("mail", "mail_template.create", false, template_input());
    d.add_guard("mail", "mail_template.update", false, template_input());
    d.add_guard("mail", "mail_template.delete", false, agent_proposes());
}
