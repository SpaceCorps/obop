//! One message through a provider of the Connectors catalogue whose engine is `mail`
//! (docs/spec/mail.md 4). The connection holds the sealed credentials or OAuth tokens; before
//! anything leaves, the Connectors' egress gate decides (flows on, connection connected, a legal
//! basis that covers the fields, the agreement and adequacy rules, the connection's hourly rate and
//! daily volume caps); the fields `to`, `subject` and `body` of `outbound_mail`, and `cc`, `bcc`
//! and `reply_to` when the message has them, must be on the connection's allowlist and released as
//! they are. As in `Egress::prepare`, the release is written to the data flow log before the
//! request leaves (a hash, never the payload; a row that cannot be written is a send that does not
//! happen), a refusal is logged `refused` and a provider that does not accept the message is
//! logged `rejected` with its answer.

use crate::meta::OUTBOUND;
use crate::mime::Message;
use crate::{Failure, Mail};
use base64::Engine as _;
use obop_connectors::Engine;
use obop_connectors::Refusal;
use obop_connectors::catalog::Provider;
use obop_connectors::model::Connection;
use obop_connectors::net::{NetError, Request};
use obop_connectors::policy::Treatment;
use serde_json::{Value as J, json};
use sha2::{Digest, Sha256};

/// The fields every mail flow releases.
pub const FIELDS: [&str; 3] = ["to", "subject", "body"];

/// The fields a message releases: [`FIELDS`] and the copy, blind copy and reply-to recipients it
/// has, in the order of the model.
pub fn fields_of(msg: &Message) -> Vec<&'static str> {
    let mut out = vec!["to"];
    if !msg.cc.is_empty() {
        out.push("cc");
    }
    if !msg.bcc.is_empty() {
        out.push("bcc");
    }
    if msg.reply_to.as_deref().is_some_and(|r| !r.is_empty()) {
        out.push("reply_to");
    }
    out.extend(["subject", "body"]);
    out
}

fn default_base(provider: &str, c: &Connection) -> Option<String> {
    Some(
        match provider {
            "postmark" => "https://api.postmarkapp.com",
            "sendgrid" => "https://api.sendgrid.com",
            "mailgun" if c.cfg("region").eq_ignore_ascii_case("eu") => "https://api.eu.mailgun.net",
            "mailgun" => "https://api.mailgun.net",
            "microsoft_graph_mail" => "https://graph.microsoft.com",
            "gmail_send" => "https://gmail.googleapis.com",
            _ => return None,
        }
        .to_string(),
    )
}

/// The provider's answer as a failure: 429 and 5xx are worth another attempt, other refusals not.
fn status_failure(provider: &str, status: u16, body: &[u8]) -> Failure {
    let detail: String = String::from_utf8_lossy(body)
        .chars()
        .filter(|c| !c.is_control())
        .take(200)
        .collect();
    let why = format!("{provider} answered HTTP {status}: {detail}");
    if status == 429 || status >= 500 {
        Failure::Temporary(why)
    } else {
        Failure::Permanent(why)
    }
}

fn net_failure(e: NetError) -> Failure {
    match e {
        NetError::Timeout | NetError::Transport(_) | NetError::Resolve(_) => {
            Failure::Temporary(e.to_string())
        }
        other => Failure::Permanent(other.to_string()),
    }
}

fn form(pairs: &[(&str, String)]) -> Vec<u8> {
    let mut s = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in pairs {
        s.append_pair(k, v);
    }
    s.finish().into_bytes()
}

fn address_list(list: &[String]) -> Vec<J> {
    list.iter()
        .map(|a| json!({"emailAddress": {"address": a}}))
        .collect()
}

/// A request as one provider expects it: url, headers and body.
type Prepared = (String, Vec<(String, String)>, Vec<u8>);

/// The request one provider expects.
fn request_for(
    p: &Provider,
    c: &Connection,
    base: &str,
    secret: &str,
    msg: &Message,
) -> Result<Prepared, Failure> {
    let from = crate::mime::mailbox(&msg.from_name, &msg.from);
    let json_type = ("content-type".to_string(), "application/json".to_string());
    Ok(match p.id {
        "postmark" => {
            let mut body = json!({
                "From": from,
                "To": msg.to.join(", "),
                "Subject": msg.subject,
                "TextBody": msg.text,
                "Headers": [{"Name": "Message-ID", "Value": format!("<{}>", msg.message_id)}],
            });
            if !msg.cc.is_empty() {
                body["Cc"] = msg.cc.join(", ").into();
            }
            if !msg.bcc.is_empty() {
                body["Bcc"] = msg.bcc.join(", ").into();
            }
            if let Some(r) = &msg.reply_to {
                body["ReplyTo"] = r.clone().into();
            }
            if !c.cfg("message_stream").is_empty() {
                body["MessageStream"] = c.cfg("message_stream").into();
            }
            (
                format!("{base}/email"),
                vec![
                    json_type,
                    ("accept".into(), "application/json".into()),
                    ("x-postmark-server-token".into(), secret.into()),
                ],
                body.to_string().into_bytes(),
            )
        }
        "sendgrid" => {
            let to = |l: &[String]| l.iter().map(|a| json!({"email": a})).collect::<Vec<_>>();
            let mut pers = json!({"to": to(&msg.to)});
            if !msg.cc.is_empty() {
                pers["cc"] = to(&msg.cc).into();
            }
            if !msg.bcc.is_empty() {
                pers["bcc"] = to(&msg.bcc).into();
            }
            let mut body = json!({
                "personalizations": [pers],
                "from": {"email": msg.from, "name": msg.from_name},
                "subject": msg.subject,
                "content": [{"type": "text/plain", "value": msg.text}],
                "headers": {"Message-ID": format!("<{}>", msg.message_id)},
            });
            if let Some(r) = &msg.reply_to {
                body["reply_to"] = json!({"email": r});
            }
            (
                format!("{base}/v3/mail/send"),
                vec![
                    json_type,
                    ("authorization".into(), format!("Bearer {secret}")),
                ],
                body.to_string().into_bytes(),
            )
        }
        "mailgun" => {
            let domain = c.cfg("domain").trim();
            if domain.is_empty()
                || !domain
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '-')
            {
                return Err(Failure::Permanent(
                    "the Mailgun connection has no valid sending domain".into(),
                ));
            }
            let mut pairs = vec![
                ("from", from),
                ("to", msg.to.join(", ")),
                ("subject", msg.subject.clone()),
                ("text", msg.text.clone()),
                ("h:Message-ID", format!("<{}>", msg.message_id)),
            ];
            if !msg.cc.is_empty() {
                pairs.push(("cc", msg.cc.join(", ")));
            }
            if !msg.bcc.is_empty() {
                pairs.push(("bcc", msg.bcc.join(", ")));
            }
            if let Some(r) = &msg.reply_to {
                pairs.push(("h:Reply-To", r.clone()));
            }
            let auth = base64::engine::general_purpose::STANDARD.encode(format!("api:{secret}"));
            (
                format!("{base}/v3/{domain}/messages"),
                vec![
                    (
                        "content-type".into(),
                        "application/x-www-form-urlencoded".into(),
                    ),
                    ("authorization".into(), format!("Basic {auth}")),
                ],
                form(&pairs),
            )
        }
        "microsoft_graph_mail" => {
            let mut message = json!({
                "subject": msg.subject,
                "body": {"contentType": "Text", "content": msg.text},
                "toRecipients": address_list(&msg.to),
                "from": {"emailAddress": {"address": msg.from, "name": msg.from_name}},
                "internetMessageId": format!("<{}>", msg.message_id),
            });
            if !msg.cc.is_empty() {
                message["ccRecipients"] = address_list(&msg.cc).into();
            }
            if !msg.bcc.is_empty() {
                message["bccRecipients"] = address_list(&msg.bcc).into();
            }
            if let Some(r) = &msg.reply_to {
                message["replyTo"] = address_list(std::slice::from_ref(r)).into();
            }
            (
                format!("{base}/v1.0/me/sendMail"),
                vec![
                    json_type,
                    ("authorization".into(), format!("Bearer {secret}")),
                ],
                json!({"message": message, "saveToSentItems": true})
                    .to_string()
                    .into_bytes(),
            )
        }
        "gmail_send" => {
            let mime = msg.to_bytes().map_err(Failure::Permanent)?;
            // Gmail takes the envelope from the headers; a blind copy must be one of them there.
            let mut raw = Vec::new();
            if !msg.bcc.is_empty() {
                raw.extend_from_slice(format!("Bcc: {}\r\n", msg.bcc.join(", ")).as_bytes());
            }
            raw.extend_from_slice(&mime);
            let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw);
            (
                format!("{base}/gmail/v1/users/me/messages/send"),
                vec![
                    json_type,
                    ("authorization".into(), format!("Bearer {secret}")),
                ],
                json!({"raw": raw}).to_string().into_bytes(),
            )
        }
        other => {
            return Err(Failure::Permanent(format!(
                "{other} is not a mail provider"
            )));
        }
    })
}

/// Send one message through the connection `connection`.
pub fn send(mail: &Mail, connection: &str, msg: &Message) -> Result<String, Failure> {
    let rt = &mail.rt;
    if connection.is_empty() {
        return Err(Failure::Permanent(
            "the mail server names no connection".into(),
        ));
    }
    let (c, p) = rt.connection(connection).map_err(Failure::Permanent)?;
    if p.engine != Engine::Mail {
        return Err(Failure::Permanent(format!("{} does not send mail", p.name)));
    }
    let egress = &rt.egress;
    // A refusal is logged as `Egress::prepare` logs one (not when the store was busy: then
    // nothing could be read or written, and the message is tried again).
    let refuse = |r: Refusal| -> Failure {
        if !matches!(r, Refusal::StoreBusy) {
            egress.log(
                &c,
                "outbound",
                OUTBOUND,
                0,
                &[],
                &[],
                "",
                "",
                "refused",
                &r.to_string(),
            );
        }
        match r {
            Refusal::StoreBusy | Refusal::RateLimited(_) | Refusal::VolumeCap(_) => {
                Failure::Temporary(r.to_string())
            }
            other => Failure::Permanent(format!("the connection may not send it: {other}")),
        }
    };
    // The Connectors' gate, as for every record that leaves.
    let plan = egress.authorise(&c, &p, OUTBOUND).map_err(refuse)?;
    let fields = fields_of(msg);
    for name in &fields {
        let released = plan
            .sent()
            .any(|f| f.name == *name && f.decision.treatment == Treatment::Allow);
        if !released {
            let why = format!(
                "the connection's allowlist must let {name} of outbound_mail leave as it is"
            );
            egress.log(
                &c,
                "outbound",
                OUTBOUND,
                0,
                &[],
                &[],
                "",
                "",
                "refused",
                &why,
            );
            return Err(Failure::Permanent(why));
        }
    }
    // The caps of the connection, counted from the log: a log that cannot be read is not "no
    // flows yet" (D74).
    let (flows, records) = egress
        .try_usage(&c.id, obop_connectors::util::now())
        .map_err(|_| refuse(Refusal::StoreBusy))?;
    if flows >= c.rate_limit_per_hour {
        return Err(refuse(Refusal::RateLimited(c.rate_limit_per_hour)));
    }
    if records + 1 > c.volume_cap_per_day {
        return Err(refuse(Refusal::VolumeCap(c.volume_cap_per_day)));
    }
    let secret = if p.auth_kind == "oauth2" {
        rt.access_token(&c).map_err(Failure::Temporary)?
    } else {
        let creds = rt.credentials(&c).map_err(Failure::Permanent)?;
        creds
            .get("api_key")
            .and_then(J::as_str)
            .filter(|k| !k.trim().is_empty())
            .map(str::to_string)
            .ok_or_else(|| Failure::Permanent("the connection has no API key".into()))?
    };
    let base = mail
        .api_base(p.id)
        .or_else(|| default_base(p.id, &c))
        .ok_or_else(|| Failure::Permanent(format!("{} has no address", p.name)))?;
    let (url, headers, body) = request_for(&p, &c, &base, &secret, msg)?;
    let host = url::Url::parse(&url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_default();
    let mut allowed: Vec<String> = p.allowed_hosts.iter().map(|h| h.to_string()).collect();
    if mail.api_base(p.id).is_some() {
        allowed.push(host);
    }
    let hash = {
        let mut h = Sha256::new();
        h.update(&body);
        h.finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    let sent: Vec<String> = fields.iter().map(|s| s.to_string()).collect();
    let mode = plan.mode_summary();
    // Recorded before it leaves: a release that cannot be logged does not happen.
    egress
        .try_log(
            &c,
            "outbound",
            OUTBOUND,
            1,
            &sent,
            &[],
            &mode,
            &hash,
            "released",
            "",
        )
        .map_err(|_| Failure::Temporary(Refusal::StoreBusy.to_string()))?;
    let failed = |f: Failure| -> Failure {
        egress.log(
            &c,
            "outbound",
            OUTBOUND,
            1,
            &sent,
            &[],
            &mode,
            &hash,
            "rejected",
            f.text(),
        );
        f
    };
    let r = rt
        .client
        .send(Request {
            method: "POST",
            url: &url,
            headers,
            body: Some(body),
            allowed_hosts: &allowed,
        })
        .map_err(|e| failed(net_failure(e)))?;
    if (200..300).contains(&r.status) {
        let id = serde_json::from_slice::<J>(&r.body)
            .ok()
            .and_then(|v| {
                ["MessageID", "id"]
                    .iter()
                    .find_map(|k| v.get(*k).and_then(J::as_str).map(str::to_string))
            })
            .or_else(|| r.header("x-message-id").map(str::to_string))
            .unwrap_or_default();
        Ok(format!("{} accepted it (HTTP {}) {id}", p.name, r.status)
            .trim()
            .to_string())
    } else {
        Err(failed(status_failure(p.name, r.status, &r.body)))
    }
}
