//! Outbound mail (docs/spec/mail.md).
//!
//! - [`meta`]: the `mail` module: `mail_server`, `outbound_mail`, `mail_template` and their
//!   commands, so REST, MCP, the CLI and the back office are generated.
//! - [`address`] and [`mime`]: what may be sent and how it is written.
//! - [`smtp`]: one message to an SMTP relay; [`api`]: one message through a provider of the
//!   Connectors catalogue (engine `mail`), after the Connectors' egress gate.
//! - [`Mail`]: queueing (`outbound_mail.send`), the worker with retries, the failed state, the test
//!   message, retention.

pub mod address;
pub mod api;
mod guards;
mod handlers;
pub mod meta;
pub mod mime;
pub mod smtp;

#[cfg(test)]
mod tests;

pub use meta::{ADMIN, SENDER, SYSTEM, module};

use meta::{OUTBOUND, SERVER, TEMPLATE};
use obop_commands::{Dispatcher, Store};
use obop_connectors::Runtime;
use obop_storage::{Backend, Fields, Op};
use serde_json::{Value as J, json};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

/// How a delivery attempt ended when it did not succeed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// Worth another attempt later (a network error, a busy server).
    Temporary(String),
    /// Will not get better by waiting (refused recipient, wrong password, missing setting).
    Permanent(String),
}

impl Failure {
    pub fn text(&self) -> &str {
        match self {
            Self::Temporary(t) | Self::Permanent(t) => t,
        }
    }
}

/// Seconds after failed attempt 1, 2, ... before the next; after the last the message fails.
pub const BACKOFF_SECS: [u64; 5] = [60, 300, 1800, 7200, 21_600];
/// A claim lasts this long; a worker that died leaves its messages to another after it.
pub const LEASE_SECS: u64 = 300;
/// Most messages one run of the worker sends.
const BATCH: usize = 20;
/// The longest text accepted (characters).
pub const MAX_BODY: usize = 1_000_000;

const PURPOSE_PASSWORD: &str = "mail-server-password";

fn body_purpose(id: &str) -> String {
    format!("outbound-mail:{id}")
}

#[derive(Debug, Clone)]
pub struct Config {
    /// Days sent, cancelled and failed messages are kept (`OBOP_MAIL_RETENTION_DAYS`, default 90).
    pub retention_days: u64,
    /// The name given in `EHLO`: the host of `OBOP_PUBLIC_URL`, else `localhost`.
    pub helo: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            retention_days: 90,
            helo: "localhost".into(),
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        let mut c = Self::default();
        if let Some(d) = std::env::var("OBOP_MAIL_RETENTION_DAYS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|d| *d > 0)
        {
            c.retention_days = d;
        }
        if let Some(h) = std::env::var("OBOP_PUBLIC_URL")
            .ok()
            .and_then(|u| url::Url::parse(u.trim()).ok())
            .and_then(|u| u.host_str().map(str::to_string))
        {
            c.helo = h;
        }
        c
    }
}

pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

pub struct Mail {
    pub backend: Arc<dyn Backend>,
    pub rt: Arc<Runtime>,
    cfg: Config,
    trust: RwLock<smtp::Trust>,
    /// Provider API addresses replaced by a loopback stand-in (tests).
    bases: RwLock<BTreeMap<String, String>>,
    nudge: Mutex<Option<std::sync::mpsc::Sender<()>>>,
    clock: Clock,
}

pub(crate) fn text<'a>(f: &'a Fields, k: &str) -> &'a str {
    f.get(k).and_then(J::as_str).unwrap_or("")
}

fn num(f: &Fields, k: &str) -> u64 {
    match f.get(k) {
        Some(J::Number(n)) => n.as_u64().unwrap_or(0),
        Some(J::String(s)) => s.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

pub(crate) fn instant(secs: u64) -> String {
    obop_values::Instant::from_micros(secs as i64 * 1_000_000)
        .map(|i| i.canonical())
        .unwrap_or_default()
}

/// The comma-separated kinds of a server.
fn kinds_of(f: &Fields) -> Vec<String> {
    text(f, "kinds")
        .split(',')
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .collect()
}

fn on(f: &Fields, k: &str) -> bool {
    f.get(k) == Some(&J::Bool(true))
}

/// A message kind or template key: lowercase letters, digits and underscores.
pub fn check_kind(k: &str) -> Result<(), String> {
    if k.is_empty()
        || k.len() > 64
        || !k
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(format!(
            "{k:?} is not a message kind: use lowercase letters, digits and _ (such as invoice)"
        ));
    }
    Ok(())
}

/// Replace `{{name}}` with the values; a placeholder without a value is an error.
pub fn render(template: &str, vars: &BTreeMap<String, String>) -> Result<String, String> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after.find("}}").ok_or("the template has {{ without }}")?;
        let name = after[..end].trim();
        let v = vars
            .get(name)
            .ok_or_else(|| format!("the template needs a value for {name}"))?;
        out.push_str(v);
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

/// What `outbound_mail.send` was given.
#[derive(Debug, Clone, Default)]
pub struct Outgoing {
    pub kind: String,
    pub to: String,
    pub cc: String,
    pub bcc: String,
    pub reply_to: String,
    pub subject: String,
    pub body: String,
    pub template: String,
    pub language: String,
    /// A JSON object of texts and numbers.
    pub vars: String,
    pub server: String,
    pub related_model: String,
    pub related_id: String,
}

/// What one run of the worker did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    pub sent: usize,
    pub retrying: usize,
    pub failed: usize,
}

impl Mail {
    pub fn new(rt: Arc<Runtime>, cfg: Config) -> Arc<Mail> {
        Self::with_clock(rt, cfg, Arc::new(obop_connectors::util::now))
    }

    pub fn with_clock(rt: Arc<Runtime>, cfg: Config, clock: Clock) -> Arc<Mail> {
        Arc::new(Mail {
            backend: rt.backend.clone(),
            rt,
            cfg,
            trust: RwLock::new(smtp::Trust::default()),
            bases: RwLock::new(BTreeMap::new()),
            nudge: Mutex::new(None),
            clock,
        })
    }

    fn now(&self) -> u64 {
        (self.clock)()
    }

    /// The time the module goes by, in seconds since 1970.
    pub fn now_secs(&self) -> u64 {
        self.now()
    }

    /// Trust one more certificate for SMTP over TLS (tests with a stand-in that has its own).
    pub fn trust_certificate(&self, der: Vec<u8>) {
        if let Ok(mut t) = self.trust.write() {
            t.extra.push(der.into());
        }
    }

    /// Send a provider's API calls to a stand-in instead: plain http on a literal loopback
    /// address only, so no setting can send a credential elsewhere.
    pub fn use_api_base(&self, provider: &str, base: &str) -> Result<(), String> {
        let ok = url::Url::parse(base).is_ok_and(|u| {
            u.scheme() == "http" && matches!(u.host_str(), Some("127.0.0.1" | "[::1]"))
        });
        if !ok {
            return Err(format!("{base} is not a loopback address"));
        }
        self.bases
            .write()
            .map_err(|_| "lock poisoned")?
            .insert(provider.into(), base.trim_end_matches('/').into());
        Ok(())
    }

    pub(crate) fn api_base(&self, provider: &str) -> Option<String> {
        self.bases.read().ok()?.get(provider).cloned()
    }

    /// Bind the module to the dispatcher: the generated commands, the guards and the handlers.
    pub fn register(self: &Arc<Self>, d: &Arc<Dispatcher>, store: &Store) {
        let m = module();
        d.register_crud(&m, store);
        guards::install(d, self);
        handlers::install(d, &m, self);
    }

    /// Start the worker thread: it runs the due queue when a message is queued and every five
    /// seconds, and the retention once a day.
    pub fn start(self: &Arc<Self>) {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        if let Ok(mut n) = self.nudge.lock() {
            *n = Some(tx);
        }
        let this = self.clone();
        std::thread::spawn(move || {
            // Woken by a queued message or by the timeout; ends when the process does.
            while let Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Timeout) =
                rx.recv_timeout(Duration::from_secs(5))
            {
                let now = this.now();
                if let Err(e) = this.process(now) {
                    eprintln!("mail: the queue did not run: {e}");
                }
                this.purge_if_due(now);
            }
        });
    }

    fn wake(&self) {
        if let Ok(n) = self.nudge.lock()
            && let Some(tx) = n.as_ref()
        {
            let _ = tx.send(());
        }
    }

    pub(crate) fn seal_password(&self, pw: &str) -> String {
        self.rt.secrets.seal(PURPOSE_PASSWORD, pw)
    }

    // ---- queueing ---------------------------------------------------------------------------

    /// The server for a message of `kind`: the one asked for, else one that lists the kind, else
    /// the default, else the only enabled one.
    pub fn choose_server(&self, kind: &str, asked: &str) -> Result<(String, Fields), String> {
        if !asked.is_empty() {
            let f = self
                .backend
                .get(SERVER, asked)?
                .ok_or_else(|| format!("no mail server {asked}"))?;
            if !on(&f, "enabled") {
                return Err(format!(
                    "the mail server {} is not enabled",
                    text(&f, "name")
                ));
            }
            return Ok((asked.to_string(), f));
        }
        let mut enabled: Vec<(String, Fields)> = self
            .backend
            .list(SERVER)?
            .into_iter()
            .filter(|(_, f)| on(f, "enabled"))
            .collect();
        enabled.sort_by(|a, b| a.0.cmp(&b.0));
        if let Some(s) = enabled
            .iter()
            .find(|(_, f)| kinds_of(f).iter().any(|k| k == kind))
        {
            return Ok(s.clone());
        }
        if let Some(s) = enabled.iter().find(|(_, f)| on(f, "is_default")) {
            return Ok(s.clone());
        }
        let general: Vec<&(String, Fields)> = enabled
            .iter()
            .filter(|(_, f)| kinds_of(f).is_empty())
            .collect();
        if general.len() == 1 {
            return Ok(general[0].clone());
        }
        Err(if enabled.is_empty() {
            "no mail server is set up: add one under Customize, Mail".to_string()
        } else {
            format!(
                "no mail server sends messages of the kind {kind}: list it on a server or mark one as the default"
            )
        })
    }

    fn template(&self, key: &str, language: &str) -> Result<Fields, String> {
        let all = self.backend.find(TEMPLATE, "key", &J::from(key))?;
        let pick = |lang: &str| {
            all.iter()
                .find(|(_, f)| text(f, "language") == lang)
                .map(|(_, f)| f.clone())
        };
        pick(language)
            .or_else(|| pick("en"))
            .ok_or_else(|| format!("no template {key} in {language} or en"))
    }

    /// Check and queue one message; returns its id and the server.
    pub fn queue(&self, o: &Outgoing, by: &str) -> Result<(String, String), String> {
        let kind = if o.kind.trim().is_empty() {
            "general".to_string()
        } else {
            o.kind.trim().to_string()
        };
        check_kind(&kind)?;
        let to = address::parse_list(&o.to)?;
        let cc = address::parse_list(&o.cc)?;
        let bcc = address::parse_list(&o.bcc)?;
        if to.is_empty() {
            return Err("to: name at least one recipient".into());
        }
        if to.len() + cc.len() + bcc.len() > address::MAX_RECIPIENTS {
            return Err(format!(
                "a message may have at most {} recipients",
                address::MAX_RECIPIENTS
            ));
        }
        let reply_to = o.reply_to.trim();
        if !reply_to.is_empty() {
            address::check(reply_to)?;
        }
        let (subject, body) = if o.template.trim().is_empty() {
            (o.subject.clone(), o.body.clone())
        } else {
            let key = o.template.trim();
            check_kind(key)?;
            let lang = if o.language.is_empty() {
                "en"
            } else {
                &o.language
            };
            if !meta::LANGUAGES.contains(&lang) {
                return Err(format!("{lang} is not a template language"));
            }
            let t = self.template(key, lang)?;
            let vars = parse_vars(&o.vars)?;
            (
                render(text(&t, "subject"), &vars)?,
                render(text(&t, "body"), &vars)?,
            )
        };
        address::check_subject(&subject)?;
        if body.trim().is_empty() {
            return Err("the message has no text".into());
        }
        if body.chars().count() > MAX_BODY {
            return Err(format!("the text is longer than {MAX_BODY} characters"));
        }
        if !o.related_model.is_empty()
            && !o
                .related_model
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err("related_model is not a model name".into());
        }
        let (server, sf) = self.choose_server(&kind, o.server.trim())?;
        let id = uuid::Uuid::now_v7().to_string();
        let domain = text(&sf, "from_address")
            .rsplit_once('@')
            .map_or("localhost", |(_, d)| d)
            .to_string();
        let now = self.now();
        let f = json!({
            "kind": kind,
            "to": to.join(", "),
            "cc": cc.join(", "),
            "bcc": bcc.join(", "),
            "reply_to": reply_to,
            "subject": subject,
            "body": self.rt.secrets.seal(&body_purpose(&id), &body),
            "template": o.template.trim(),
            "language": if o.template.trim().is_empty() { "" } else if o.language.is_empty() { "en" } else { o.language.as_str() },
            "server": server,
            "status": "queued",
            "attempts": 0,
            "queued_at": instant(now),
            "next_attempt_at": instant(now),
            "due": now,
            "lease_until": 0,
            "last_error": "",
            "message_id": format!("{id}@{domain}"),
            "related_model": o.related_model,
            "related_id": o.related_id,
            "requested_by": by,
        });
        let J::Object(f) = f else { unreachable!() };
        self.backend.insert(OUTBOUND, &id, &f)?;
        self.wake();
        Ok((id, server))
    }

    // ---- the worker -------------------------------------------------------------------------

    /// Claim the due messages (under the cross-process lock) and send them.
    pub fn process(&self, now: u64) -> Result<Run, String> {
        let claimed = {
            let _g = self.backend.lock("mail.queue")?;
            let mut due: Vec<(String, Fields)> = Vec::new();
            for status in ["queued", "sending"] {
                for (id, f) in self.backend.find(OUTBOUND, "status", &J::from(status))? {
                    let ready = if status == "queued" {
                        num(&f, "due") <= now
                    } else {
                        num(&f, "lease_until") < now
                    };
                    if ready {
                        due.push((id, f));
                    }
                }
            }
            due.sort_by(|a, b| a.0.cmp(&b.0));
            let mut claimed = Vec::new();
            for (id, f) in due.into_iter().take(BATCH) {
                let mut patch = Fields::new();
                patch.insert("status".into(), "sending".into());
                patch.insert("lease_until".into(), (now + LEASE_SECS).into());
                let op = Op::Patch {
                    model: OUTBOUND.into(),
                    id: id.clone(),
                    patch,
                    expect: vec![
                        ("status".into(), f.get("status").cloned().unwrap_or(J::Null)),
                        (
                            "lease_until".into(),
                            f.get("lease_until").cloned().unwrap_or(J::Null),
                        ),
                    ],
                };
                if self.backend.apply_checked(&[op]).is_ok() {
                    claimed.push((id, f));
                }
            }
            claimed
        };
        let mut run = Run::default();
        for (id, f) in claimed {
            let outcome = self.attempt(&id, &f);
            match self.record(&id, &f, outcome, now)? {
                "sent" => run.sent += 1,
                "queued" => run.retrying += 1,
                _ => run.failed += 1,
            }
        }
        Ok(run)
    }

    /// One delivery attempt of a stored message.
    fn attempt(&self, id: &str, f: &Fields) -> Result<String, Failure> {
        let body = self
            .rt
            .secrets
            .open(&body_purpose(id), text(f, "body"))
            .map_err(|_| Failure::Permanent("the stored text could not be opened".into()))?;
        let server_id = text(f, "server");
        let sf = self
            .backend
            .get(SERVER, server_id)
            .map_err(Failure::Temporary)?
            .ok_or_else(|| Failure::Permanent("its mail server was deleted".into()))?;
        if !on(&sf, "enabled") {
            return Err(Failure::Temporary(format!(
                "the mail server {} is not enabled",
                text(&sf, "name")
            )));
        }
        let addrs = |k: &str| address::parse_list(text(f, k)).unwrap_or_default();
        let reply_to = Some(text(f, "reply_to"))
            .filter(|r| !r.is_empty())
            .or_else(|| Some(text(&sf, "reply_to")).filter(|r| !r.is_empty()))
            .map(str::to_string);
        let footer = text(&sf, "footer").trim();
        let text_body = if footer.is_empty() {
            body
        } else {
            format!("{body}\n\n-- \n{footer}")
        };
        let msg = mime::Message {
            from_name: text(&sf, "from_name").into(),
            from: text(&sf, "from_address").into(),
            to: addrs("to"),
            cc: addrs("cc"),
            bcc: addrs("bcc"),
            reply_to,
            subject: text(f, "subject").into(),
            text: text_body,
            message_id: text(f, "message_id").into(),
            date: self.now(),
        };
        self.deliver(&sf, &msg)
    }

    /// Send `msg` through the server `sf`.
    pub fn deliver(&self, sf: &Fields, msg: &mime::Message) -> Result<String, Failure> {
        match text(sf, "kind") {
            "smtp" => {
                let security = smtp::Security::parse(text(sf, "security"))
                    .ok_or_else(|| Failure::Permanent("unknown security".into()))?;
                let password = match text(sf, "password") {
                    "" => String::new(),
                    sealed => self
                        .rt
                        .secrets
                        .open(PURPOSE_PASSWORD, sealed)
                        .map_err(|_| {
                            Failure::Permanent(
                                "the stored password could not be opened; enter it again".into(),
                            )
                        })?,
                };
                let port = match num(sf, "port") {
                    0 if security == smtp::Security::Tls => 465,
                    0 => 587,
                    p => u16::try_from(p)
                        .map_err(|_| Failure::Permanent("the port is not valid".into()))?,
                };
                let relay = smtp::Relay {
                    host: text(sf, "host").into(),
                    port,
                    security,
                    username: text(sf, "username").into(),
                    password,
                    helo: self.cfg.helo.clone(),
                };
                let trust = self.trust.read().map(|t| t.clone()).unwrap_or_default();
                smtp::send(&self.rt.client, &trust, &relay, msg)
            }
            "provider" => api::send(self, text(sf, "connection"), msg),
            other => Err(Failure::Permanent(format!("unknown server kind {other:?}"))),
        }
    }

    /// Write the outcome of an attempt; returns the new status.
    fn record(
        &self,
        id: &str,
        f: &Fields,
        outcome: Result<String, Failure>,
        now: u64,
    ) -> Result<&'static str, String> {
        let attempts = num(f, "attempts") + 1;
        let mut p = Fields::new();
        p.insert("attempts".into(), attempts.into());
        p.insert("lease_until".into(), 0.into());
        let status = match outcome {
            Ok(reply) => {
                p.insert("sent_at".into(), instant(now).into());
                p.insert("server_reply".into(), clip(&reply).into());
                p.insert("last_error".into(), "".into());
                p.insert("body".into(), "".into());
                p.insert("next_attempt_at".into(), J::Null);
                "sent"
            }
            Err(Failure::Temporary(why)) if (attempts as usize) <= BACKOFF_SECS.len() => {
                let wait = BACKOFF_SECS[attempts as usize - 1];
                p.insert("last_error".into(), clip(&why).into());
                p.insert("due".into(), (now + wait).into());
                p.insert("next_attempt_at".into(), instant(now + wait).into());
                "queued"
            }
            Err(e) => {
                p.insert("last_error".into(), clip(e.text()).into());
                p.insert("next_attempt_at".into(), J::Null);
                "failed"
            }
        };
        p.insert("status".into(), status.into());
        obop_storage::fault::needed(|| self.backend.update(OUTBOUND, id, &p).map(|_| ()))?;
        Ok(status)
    }

    // ---- commands -----------------------------------------------------------------------------

    pub fn retry(&self, id: &str) -> Result<(), String> {
        let now = self.now();
        let mut patch = Fields::new();
        patch.insert("status".into(), "queued".into());
        patch.insert("attempts".into(), 0.into());
        patch.insert("due".into(), now.into());
        patch.insert("next_attempt_at".into(), instant(now).into());
        let f = self
            .backend
            .get(OUTBOUND, id)?
            .ok_or_else(|| format!("no message {id}"))?;
        if text(&f, "status") != "failed" {
            return Err(format!(
                "only a failed message is sent again; this one is {}",
                text(&f, "status")
            ));
        }
        if text(&f, "body").is_empty() {
            return Err("the text of this message is no longer stored".into());
        }
        self.backend
            .apply_checked(&[Op::Patch {
                model: OUTBOUND.into(),
                id: id.into(),
                patch,
                expect: vec![("status".into(), "failed".into())],
            }])
            .map_err(|_| "the message changed meanwhile; look again".to_string())?;
        self.wake();
        Ok(())
    }

    pub fn cancel(&self, id: &str) -> Result<(), String> {
        let mut patch = Fields::new();
        patch.insert("status".into(), "cancelled".into());
        patch.insert("body".into(), "".into());
        patch.insert("next_attempt_at".into(), J::Null);
        self.backend
            .apply_checked(&[Op::Patch {
                model: OUTBOUND.into(),
                id: id.into(),
                patch,
                expect: vec![("status".into(), "queued".into())],
            }])
            .map_err(|_| "only a queued message can be cancelled".to_string())
    }

    /// Send a test message through `server` now; the answer is recorded on the server and the
    /// message in the log.
    pub fn test_send(&self, server: &str, to: &str, by: &str) -> Result<String, String> {
        address::check(to.trim())?;
        let sf = self
            .backend
            .get(SERVER, server)?
            .ok_or_else(|| format!("no mail server {server}"))?;
        let id = uuid::Uuid::now_v7().to_string();
        let now = self.now();
        let domain = text(&sf, "from_address")
            .rsplit_once('@')
            .map_or("localhost", |(_, d)| d)
            .to_string();
        let subject = "obop test message".to_string();
        let body = format!(
            "This is a test message from obop, sent through the mail server \"{}\".\nIf you can read it, the server works.",
            text(&sf, "name")
        );
        let msg = mime::Message {
            from_name: text(&sf, "from_name").into(),
            from: text(&sf, "from_address").into(),
            to: vec![to.trim().into()],
            cc: vec![],
            bcc: vec![],
            reply_to: Some(text(&sf, "reply_to"))
                .filter(|r| !r.is_empty())
                .map(str::to_string),
            subject: subject.clone(),
            text: body,
            message_id: format!("{id}@{domain}"),
            date: now,
        };
        let outcome = self.deliver(&sf, &msg);
        let (status, result) = match &outcome {
            Ok(reply) => ("sent", format!("sent: {}", clip(reply))),
            Err(e) => ("failed", format!("failed: {}", clip(e.text()))),
        };
        let row = json!({
            "kind": "test",
            "to": to.trim(),
            "cc": "", "bcc": "", "reply_to": "",
            "subject": subject,
            "body": "",
            "server": server,
            "status": status,
            "attempts": 1,
            "queued_at": instant(now),
            "sent_at": if status == "sent" { J::from(instant(now)) } else { J::Null },
            "last_error": match &outcome { Err(e) => clip(e.text()), Ok(_) => String::new() },
            "server_reply": match &outcome { Ok(r) => clip(r), Err(_) => String::new() },
            "message_id": msg.message_id,
            "requested_by": by,
            "due": 0, "lease_until": 0,
        });
        let J::Object(row) = row else { unreachable!() };
        self.backend.insert(OUTBOUND, &id, &row)?;
        let mut p = Fields::new();
        p.insert("last_test_at".into(), instant(now).into());
        p.insert("last_test_result".into(), result.clone().into());
        self.backend.update(SERVER, server, &p)?;
        match outcome {
            Ok(_) => Ok(result),
            Err(e) => Err(format!("the test message was not sent: {}", e.text())),
        }
    }

    // ---- retention ----------------------------------------------------------------------------

    fn purge_if_due(&self, now: u64) {
        let Ok(_g) = self.backend.lock("mail.purge") else {
            return;
        };
        let last = match self.backend.kv_get("mail.purge") {
            Ok(v) => v.and_then(|v| v.parse::<u64>().ok()).unwrap_or(0),
            Err(_) => return,
        };
        if last + 86_400 > now
            || self
                .backend
                .kv_put("mail.purge", &now.to_string(), None)
                .is_err()
        {
            return;
        }
        if let Err(e) = self.purge(now) {
            eprintln!("mail: the retention run stopped: {e}");
        }
    }

    /// Remove sent, cancelled and failed messages older than the retention. Returns how many.
    pub fn purge(&self, now: u64) -> Result<usize, String> {
        let cutoff = now.saturating_sub(self.cfg.retention_days * 86_400);
        let cut = instant(cutoff);
        let mut n = 0;
        for (id, f) in self.backend.list(OUTBOUND)? {
            if !matches!(text(&f, "status"), "sent" | "cancelled" | "failed") {
                continue;
            }
            // Canonical instants compare as text.
            let at = Some(text(&f, "sent_at"))
                .filter(|s| !s.is_empty())
                .unwrap_or(text(&f, "queued_at"));
            if !at.is_empty() && at < cut.as_str() && self.backend.delete(OUTBOUND, &id)? {
                n += 1;
            }
        }
        Ok(n)
    }
}

fn clip(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(500).collect()
}

/// The `vars` of a template call: a JSON object of texts, numbers and booleans.
pub fn parse_vars(s: &str) -> Result<BTreeMap<String, String>, String> {
    if s.trim().is_empty() {
        return Ok(BTreeMap::new());
    }
    let J::Object(o) = serde_json::from_str::<J>(s).map_err(|_| "vars must be a JSON object")?
    else {
        return Err("vars must be a JSON object".into());
    };
    o.into_iter()
        .map(|(k, v)| {
            let t = match v {
                J::String(s) => s,
                J::Number(n) => n.to_string(),
                J::Bool(b) => b.to_string(),
                _ => return Err(format!("vars.{k} must be a text or a number")),
            };
            Ok((k, t))
        })
        .collect()
}
