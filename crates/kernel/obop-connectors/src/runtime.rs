//! The engines wired together over the dispatcher and the store.
//!
//! `Runtime` is what the `connectors` module, the HTTP routes and the CLI call. Every method
//! returns messages that are safe to show: no credential, token, URL query or payload is ever put
//! in an error.

use obop_commands::{Dispatcher, Principal, PrincipalKind, Surface, Value, Vault};
use obop_storage::{Backend, Fields};
use serde_json::{Map, Value as J, json};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::catalog::{Engine, Provider, providers};
use crate::egress::{Egress, Refusal};
use crate::ingress::{Ingress, Item, Outcome};
use crate::model::*;
use crate::net::{Client, ClientConfig, NetError, Request};
use crate::oauth::{self, OAuthConfig, Tokens};
use crate::queue::{Job, Queue, Requeued};
use crate::secretbox::SecretBox;
use crate::util::{iso, now, obj_of, parse_iso, path, random_hex, str_of};
use crate::webhook::{self, Scheme, Seen, VerifyError};

#[derive(Debug, Clone, Default)]
pub struct Config {
    /// The public origin of the instance (`OBOP_PUBLIC_URL`), for webhook addresses and the
    /// OAuth redirect.
    pub public_url: Option<String>,
    /// Allow plain http to a literal loopback host (`OBOP_CONNECTORS_ALLOW_LOOPBACK=1`): tests
    /// and local development only.
    pub allow_loopback: bool,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            public_url: std::env::var("OBOP_PUBLIC_URL")
                .ok()
                .map(|s| s.trim_end_matches('/').to_string())
                .filter(|s| !s.is_empty()),
            allow_loopback: matches!(
                std::env::var("OBOP_CONNECTORS_ALLOW_LOOPBACK").as_deref(),
                Ok("1" | "true")
            ),
        }
    }
}

/// Why an inbound webhook was not accepted; the HTTP layer maps each to a status.
#[derive(Debug, PartialEq, Eq)]
pub enum HookError {
    NotFound,
    TooLarge,
    /// Signature, timestamp or replay: all look the same to the sender.
    Unauthorised,
    Refused(String),
    Rejected(String),
}

pub struct Runtime {
    pub backend: Arc<dyn Backend>,
    pub dispatcher: Arc<Dispatcher>,
    pub secrets: SecretBox,
    pub egress: Arc<Egress>,
    pub ingress: Ingress,
    pub client: Client,
    pub queue: Queue,
    pub cfg: Config,
    providers: Vec<Provider>,
}

/// Purposes the sealed values are bound to (see `secretbox`).
pub const PURPOSE_CREDENTIALS: &str = "connection-credentials";
pub const PURPOSE_OAUTH: &str = "connection-oauth-tokens";
pub const PURPOSE_CLIENT: &str = "provider-oauth-client";

fn j(v: &Value) -> J {
    v.to_json()
}

impl Runtime {
    pub fn new(
        backend: Arc<dyn Backend>,
        dispatcher: Arc<Dispatcher>,
        secrets: SecretBox,
        cfg: Config,
    ) -> Arc<Self> {
        Self::with_providers(backend, dispatcher, secrets, cfg, providers(), None)
    }

    /// As `new`, with a different catalogue and name resolution (tests).
    pub fn with_providers(
        backend: Arc<dyn Backend>,
        dispatcher: Arc<Dispatcher>,
        secrets: SecretBox,
        cfg: Config,
        providers: Vec<Provider>,
        client: Option<Client>,
    ) -> Arc<Self> {
        let egress = Arc::new(Egress::new(
            backend.clone(),
            secrets.clone(),
            dispatcher.clone(),
        ));
        let inst = InstancePolicy::load(&*backend);
        let client = client.unwrap_or_else(|| {
            Client::new(ClientConfig {
                allow_loopback: cfg.allow_loopback,
                timeout: Duration::from_secs(20),
                instance_hosts: inst.hosts,
                ..ClientConfig::default()
            })
        });
        let queue = Queue::new(backend.clone(), secrets.clone());
        Arc::new(Self {
            ingress: Ingress::new(backend.clone(), egress.clone(), dispatcher.clone()),
            backend,
            dispatcher,
            secrets,
            egress,
            client,
            queue,
            cfg,
            providers,
        })
    }

    // ---- lookups --------------------------------------------------------------------------

    pub fn providers(&self) -> &[Provider] {
        &self.providers
    }

    pub fn provider(&self, id: &str) -> Option<Provider> {
        self.providers.iter().find(|p| p.id == id).cloned()
    }

    pub fn connection(&self, id: &str) -> Result<(Connection, Provider), String> {
        let c = Connection::load(&*self.backend, id)?;
        let p = self
            .provider(&c.provider)
            .ok_or_else(|| format!("unknown provider {}", c.provider))?;
        Ok((c, p))
    }

    /// The credentials of a connection, opened. Never log or return the result.
    pub fn credentials(&self, c: &Connection) -> Result<Map<String, J>, String> {
        if c.credentials.is_empty() {
            return Ok(Map::new());
        }
        let text = self.secrets.open(PURPOSE_CREDENTIALS, &c.credentials)?;
        match serde_json::from_str::<J>(&text) {
            Ok(J::Object(o)) => Ok(o),
            _ => Err("stored credentials are malformed".into()),
        }
    }

    /// The groups the connection may use: what its owner held when they last changed it, narrowed
    /// to what the owner holds now when the owner is a known user.
    pub fn owner_groups(&self, c: &Connection) -> Vec<String> {
        let now_groups: Option<Vec<String>> = self
            .backend
            .get("auth_user", &c.owner_user)
            .ok()
            .flatten()
            .map(|u| {
                u["groups"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default()
            });
        match now_groups {
            // A deleted or demoted owner narrows the connection; platform.admin keeps its meaning.
            Some(now_groups) => c
                .owner_groups
                .iter()
                .filter(|g| now_groups.contains(g))
                .cloned()
                .collect(),
            None => c.owner_groups.clone(),
        }
    }

    fn groups_for(&self, c: &Connection, model: &str) -> (Vec<String>, Vec<String>) {
        let held = self.owner_groups(c);
        // What a connector may use: the groups of this one model (read, write and its actions).
        let scopes = self
            .dispatcher
            .model_def(model)
            .map(|d| {
                let mut g = vec![d.read_group.to_string(), d.write_group.to_string()];
                g.extend(d.commands.iter().map(|c| c.permission_group.to_string()));
                g.sort();
                g.dedup();
                g
            })
            .unwrap_or_default();
        (held, scopes)
    }

    /// The principal that reads for an outbound flow: the owner's groups, scoped to the model's
    /// read group, so a connection can read nothing its owner could not.
    pub fn reader(&self, c: &Connection, model: &str) -> Principal {
        let (held, _) = self.groups_for(c, model);
        self.reader_for(&c.id, &held, model)
    }

    /// A reading principal for `groups`, scoped to the read group of `model`.
    pub fn reader_for(&self, id: &str, groups: &[String], model: &str) -> Principal {
        let scopes: Vec<&str> = self
            .dispatcher
            .model_def(model)
            .map(|d| vec![d.read_group])
            .unwrap_or_default();
        let refs: Vec<&str> = groups.iter().map(String::as_str).collect();
        let mut p = Principal::agent(&format!("connector:{id}"), &refs, &scopes);
        p.kind = PrincipalKind::User;
        p
    }

    /// The connector principal of inbound flows: agent kind, owner's groups, scoped to the
    /// model's groups. An action that needs approval stays a proposal.
    pub fn connector_principal(&self, c: &Connection, model: &str) -> Principal {
        let (held, scopes) = self.groups_for(c, model);
        let refs: Vec<&str> = held.iter().map(String::as_str).collect();
        let scopes: Vec<&str> = scopes.iter().map(String::as_str).collect();
        Principal::agent(&format!("connector:{}", c.id), &refs, &scopes)
    }

    /// Rows of `model` as the connection's owner may read them (secrets withheld), each with its
    /// `id`. `ids` limits the read to those records.
    pub fn read_rows(
        &self,
        c: &Connection,
        model: &str,
        ids: Option<&[String]>,
    ) -> Result<Vec<Map<String, J>>, String> {
        let p = self.reader(c, model);
        let mut out = Vec::new();
        let mut v = Vault::new();
        match ids {
            Some(ids) => {
                for id in ids.iter().take(MAX_ROWS_PER_FLOW) {
                    let r = self
                        .dispatcher
                        .call(
                            Surface::Rest,
                            &p,
                            &mut v,
                            &format!("{model}.get"),
                            &Value::map([("id", Value::text(id))]),
                        )
                        .map_err(|_| {
                            format!("the owner of this connection may not read {model}")
                        })?;
                    if let J::Object(o) = j(&r.value) {
                        out.push(o);
                    }
                }
            }
            None => {
                let input = Value::map([("limit", Value::Int(MAX_ROWS_PER_FLOW as i64))]);
                let r = self
                    .dispatcher
                    .call(Surface::Rest, &p, &mut v, &format!("{model}.list"), &input)
                    .map_err(|_| format!("the owner of this connection may not read {model}"))?;
                if let J::Array(a) = j(&r.value) {
                    out.extend(a.into_iter().filter_map(|x| x.as_object().cloned()));
                }
            }
        }
        Ok(out)
    }

    /// Seal a credentials object for storage.
    pub fn seal_credentials(&self, creds: &Map<String, J>) -> String {
        self.secrets
            .seal(PURPOSE_CREDENTIALS, &J::Object(creds.clone()).to_string())
    }

    pub fn seal_client_secret(&self, secret: &str) -> String {
        self.secrets.seal(PURPOSE_CLIENT, secret)
    }

    /// The processing register rows (Article 30 support).
    pub fn register_rows(&self) -> Vec<Map<String, J>> {
        crate::register::rows(&*self.backend, &self.egress, &|id| self.provider(id))
    }

    // ---- runs -----------------------------------------------------------------------------

    fn start_run(&self, c: &Connection, direction: &str, kind: &str) -> String {
        let id = uuid::Uuid::now_v7().to_string();
        let f = json!({
            "connection": c.id, "owner_user": c.owner_user, "direction": direction, "kind": kind,
            "started_at": iso(now()), "finished_at": "", "status": "running",
            "records": 0, "rejected": 0, "proposed": 0, "error_summary": "",
        });
        let _ = self
            .backend
            .insert(SYNC_RUN, &id, f.as_object().expect("object"));
        id
    }

    fn finish_run(
        &self,
        id: &str,
        status: &str,
        records: usize,
        rejected: usize,
        proposed: usize,
        error: &str,
    ) {
        let f = json!({
            "finished_at": iso(now()), "status": status, "records": records,
            "rejected": rejected, "proposed": proposed,
            "error_summary": error.chars().take(300).collect::<String>(),
        });
        let _ = self
            .backend
            .update(SYNC_RUN, id, f.as_object().expect("object"));
        // The connection shows its last run and its last error.
        if let Ok(Some(run)) = self.backend.get(SYNC_RUN, id) {
            let conn = str_of(&run, "connection").to_string();
            let mut p = Fields::new();
            p.insert("last_sync_at".into(), iso(now()).into());
            p.insert(
                "last_error".into(),
                if status == "failed" || status == "dead_letter" {
                    error.chars().take(200).collect::<String>()
                } else {
                    String::new()
                }
                .into(),
            );
            let _ = self.backend.update(CONNECTION, &conn, &p);
            self.maybe_error(&conn);
        }
    }

    /// Five failed runs in a row put a connected connection into `error`, so it stops sending
    /// until someone has looked (`retry`).
    fn maybe_error(&self, connection: &str) {
        let mut runs: Vec<(String, Fields)> = self
            .backend
            .list(SYNC_RUN)
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, f)| {
                str_of(f, "connection") == connection
                    && str_of(f, "status") != "running"
                    && str_of(f, "kind") != "test"
            })
            .collect();
        runs.sort_by(|a, b| b.0.cmp(&a.0));
        let failed = |f: &Fields| matches!(str_of(f, "status"), "failed" | "dead_letter");
        if runs.len() >= 5 && runs.iter().take(5).all(|(_, f)| failed(f)) {
            let still = self.backend.get(CONNECTION, connection).ok().flatten();
            if still.is_some_and(|f| str_of(&f, "state") == "connected") {
                let mut p = Fields::new();
                p.insert("state".into(), "error".into());
                let _ = self.backend.update(CONNECTION, connection, &p);
            }
        }
    }

    // ---- outbound -------------------------------------------------------------------------

    fn hosts_for(&self, c: &Connection, p: &Provider, url: &str) -> Vec<String> {
        if !p.allowed_hosts.is_empty() {
            return p.allowed_hosts.iter().map(|s| s.to_string()).collect();
        }
        let own = c.own_hosts();
        if !own.is_empty() {
            return own;
        }
        url::Url::parse(url)
            .ok()
            .and_then(|u| {
                u.host_str()
                    .map(|h| vec![h.trim_matches(['[', ']']).to_string()])
            })
            .unwrap_or_default()
    }

    fn target_url(&self, c: &Connection, creds: &Map<String, J>) -> Result<String, String> {
        let u = str_of(creds, "url");
        let u = if u.is_empty() { c.cfg("url") } else { u };
        if u.is_empty() {
            Err("the connection has no URL".into())
        } else {
            Ok(u.to_string())
        }
    }

    /// Encode prepared rows for the provider: one body per request.
    fn encode(
        &self,
        p: &Provider,
        event: &str,
        id: &str,
        prepared: &[Map<String, J>],
        model: &str,
    ) -> Vec<(String, Vec<u8>)> {
        match p.body_format {
            "form" => prepared
                .iter()
                .map(|r| {
                    let mut ser = url::form_urlencoded::Serializer::new(String::new());
                    for (k, v) in r {
                        let text = match v {
                            J::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        if k == "ref" {
                            ser.append_pair("metadata[obop_ref]", &text);
                        } else {
                            ser.append_pair(k, &text);
                        }
                    }
                    (
                        "application/x-www-form-urlencoded".to_string(),
                        ser.finish().into_bytes(),
                    )
                })
                .collect(),
            "slack" => {
                let mut text = format!("obop: {event} ({model})");
                for r in prepared.iter().take(10) {
                    let line = r
                        .iter()
                        .map(|(k, v)| {
                            format!(
                                "{k}: {}",
                                v.as_str().map_or_else(|| v.to_string(), String::from)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    text.push_str(&format!("\n• {line}"));
                }
                vec![(
                    "application/json".into(),
                    json!({"text": text}).to_string().into_bytes(),
                )]
            }
            _ => {
                let doc = json!({
                    "id": id, "type": event, "created": iso(now()), "model": model,
                    "records": prepared,
                });
                vec![("application/json".into(), doc.to_string().into_bytes())]
            }
        }
    }

    /// One delivery attempt. `Ok(status)` for any HTTP answer, `Err` for a failure to deliver.
    fn deliver(&self, c: &Connection, p: &Provider, job: &Job) -> Result<u16, NetError> {
        let creds = self
            .credentials(c)
            .map_err(|_| NetError::BadUrl("credentials cannot be opened"))?;
        let url = self
            .target_url(c, &creds)
            .map_err(|_| NetError::BadUrl("no url"))?;
        let hosts = self.hosts_for(c, p, &url);
        let mut headers = vec![("content-type".to_string(), job.content_type.clone())];
        let secret = str_of(&creds, "signing_secret");
        if !secret.is_empty() {
            headers.extend(webhook::outgoing_headers(secret, &job.id, now(), &job.body));
        }
        let key = str_of(&creds, "api_key");
        if !key.is_empty() {
            let name = c.cfg("header_name");
            if name.is_empty() {
                headers.push(("authorization".into(), format!("Bearer {key}")));
            } else {
                headers.push((name.to_string(), key.to_string()));
            }
        }
        let r = self.client.send(Request {
            method: "POST",
            url: &url,
            headers,
            body: Some(job.body.clone()),
            allowed_hosts: &hosts,
        })?;
        Ok(r.status)
    }

    fn retryable(status: u16) -> bool {
        status == 408 || status == 425 || status == 429 || status >= 500
    }

    /// Send prepared rows through the provider's engine. Records the outcome in `run`.
    fn send_prepared(
        &self,
        c: &Connection,
        p: &Provider,
        run: &str,
        event: &str,
        model: &str,
        prepared: &[Map<String, J>],
    ) -> (usize, String) {
        let id = format!("msg_{}", random_hex(12));
        let bodies = self.encode(p, event, &id, prepared, model);
        let mut failed = String::new();
        let mut sent = 0;
        let n = bodies.len();
        for (i, (content_type, body)) in bodies.into_iter().enumerate() {
            let job = Job {
                id: if n > 1 {
                    format!("{id}_{i}")
                } else {
                    id.clone()
                },
                connection: c.id.clone(),
                run: run.into(),
                content_type,
                body,
                attempt: 1,
                next_at: 0,
            };
            match self.deliver(c, p, &job) {
                Ok(s) if (200..300).contains(&s) => sent += 1,
                Ok(s) if Self::retryable(s) => {
                    failed = format!("the receiver answered {s}; will retry");
                    self.queue.push(Job {
                        next_at: now() + crate::queue::backoff(1, 100).unwrap_or(5),
                        ..job
                    });
                }
                Ok(s) => failed = format!("the receiver refused the delivery with status {s}"),
                Err(NetError::Timeout | NetError::Transport(_)) => {
                    failed = "the receiver could not be reached; will retry".into();
                    self.queue.push(Job {
                        next_at: now() + crate::queue::backoff(1, 100).unwrap_or(5),
                        ..job
                    });
                }
                Err(e) => failed = e.to_string(),
            }
        }
        (sent, failed)
    }

    /// Send the records of `model` (all, or `ids`) now. Used by the manual send and by events.
    pub fn send(
        &self,
        connection: &str,
        model: &str,
        ids: Option<&[String]>,
        event: &str,
        kind: &str,
    ) -> Result<J, String> {
        let (c, p) = self.connection(connection)?;
        if !matches!(p.engine, Engine::WebhookOut | Engine::CsvExport) {
            return Err(format!("{} does not send records", p.name));
        }
        let rows = self.read_rows(&c, model, ids)?;
        let prepared = self
            .egress
            .prepare(&c, &p, model, rows)
            .map_err(|r| r.to_string())?;
        if p.engine == Engine::CsvExport {
            let run = self.start_run(&c, "outbound", kind);
            let csv = rows_to_csv(&prepared.rows, &prepared.report.fields_sent);
            self.finish_run(&run, "succeeded", prepared.report.record_count, 0, 0, "");
            let name = if c.cfg("filename").is_empty() {
                model
            } else {
                c.cfg("filename")
            };
            return Ok(
                json!({"filename": format!("{}.csv", sanitize_name(name)), "csv": csv, "records": prepared.report.record_count}),
            );
        }
        let run = self.start_run(&c, "outbound", kind);
        let (sent, failed) = self.send_prepared(&c, &p, &run, event, model, &prepared.rows);
        let status = if failed.is_empty() {
            "succeeded"
        } else if failed.contains("will retry") {
            "retrying"
        } else {
            "failed"
        };
        self.finish_run(
            &run,
            status,
            if failed.is_empty() {
                prepared.report.record_count
            } else {
                0
            },
            0,
            0,
            &failed,
        );
        if status == "failed" {
            return Err(failed);
        }
        Ok(
            json!({"run": run, "status": status, "records": prepared.report.record_count, "deliveries": sent}),
        )
    }

    /// Manual send: every model on the allowlist.
    pub fn send_all(&self, connection: &str) -> Result<J, String> {
        let (c, p) = self.connection(connection)?;
        match p.engine {
            Engine::RestPull => return self.pull(connection),
            Engine::WebhookIn | Engine::None => {
                return Err(format!("{} has nothing to send", p.name));
            }
            _ => {}
        }
        let mut results = Vec::new();
        // The model that receives events is on the allowlist for what may be written; it is not
        // something this connection sends.
        let receives = if p.can_receive() {
            c.cfg("target_model")
        } else {
            ""
        };
        for model in c.allowlist.keys().filter(|m| m.as_str() != receives) {
            results.push(self.send(connection, model, None, "obop.manual", "manual")?);
        }
        if results.is_empty() {
            return Err("there is nothing on the allowlist to send".into());
        }
        if results.len() == 1 {
            return Ok(results.remove(0));
        }
        Ok(json!({"results": results}))
    }

    /// A command succeeded: deliver the record to the connections that subscribe to it.
    pub fn handle_event(&self, command: &str, record: &str) {
        let Some((model, _)) = command.split_once('.') else {
            return;
        };
        let conns: Vec<String> = self
            .backend
            .list(CONNECTION)
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, f)| str_of(f, "state") == "connected")
            .filter(|(_, f)| {
                obj_of(f, "config")
                    .get("events")
                    .and_then(J::as_str)
                    .is_some_and(|e| e.split(',').any(|x| x.trim() == command))
            })
            .map(|(id, _)| id)
            .collect();
        for id in conns {
            // A refusal is already in the flow log; an event has nobody to tell.
            let _ = self.send(&id, model, Some(&[record.to_string()]), command, "event");
        }
    }

    /// Try the deliveries that are due. Returns how many were attempted.
    pub fn process_queue(&self, at: u64) -> usize {
        // The queue lives in the database: this claims the jobs that are due and leased to no
        // other process, so several server processes share the work (B2).
        let jobs = self.queue.take_due(at);
        let n = jobs.len();
        for job in jobs {
            let Ok((c, p)) = self.connection(&job.connection) else {
                self.finish_run(
                    &job.run,
                    "dead_letter",
                    0,
                    0,
                    0,
                    "the connection no longer exists",
                );
                self.queue.complete(&job);
                continue;
            };
            let allowed = c.is_connected()
                && consents_of(&*self.backend, &c.id)
                    .iter()
                    .any(|k| k.is_active())
                && InstancePolicy::load(&*self.backend).flows_enabled;
            if !allowed {
                self.finish_run(&job.run, "dead_letter", 0, 0, 0, "delivery stopped: the connection is no longer connected or its legal basis was withdrawn");
                self.queue.complete(&job);
                continue;
            }
            let err = match self.deliver(&c, &p, &job) {
                Ok(s) if (200..300).contains(&s) => {
                    self.finish_run(&job.run, "succeeded", 1, 0, 0, "");
                    self.queue.complete(&job);
                    continue;
                }
                Ok(s) if !Self::retryable(s) => {
                    self.finish_run(
                        &job.run,
                        "failed",
                        0,
                        0,
                        0,
                        &format!("the receiver refused the delivery with status {s}"),
                    );
                    self.queue.complete(&job);
                    continue;
                }
                Ok(s) => format!("the receiver answered {s}"),
                Err(e) => e.to_string(),
            };
            match self.queue.requeue_because(job.clone(), at, 100, &err) {
                Requeued::At(_) => {}
                Requeued::Dead => self.finish_run(
                    &job.run,
                    "dead_letter",
                    0,
                    0,
                    0,
                    &format!("gave up after {} attempts: {err}", job.attempt + 1),
                ),
            }
        }
        n
    }

    // ---- inbound --------------------------------------------------------------------------

    fn map_record(&self, c: &Connection, payload: &J) -> Map<String, J> {
        let mapping = obj_of(&c.config, "mapping");
        if mapping.is_empty() {
            return payload.as_object().cloned().unwrap_or_default();
        }
        let mut out = Map::new();
        for (from, to) in &mapping {
            if let (Some(to), Some(v)) = (to.as_str(), path(payload, from)) {
                out.insert(to.to_string(), v.clone());
            }
        }
        out
    }

    fn items_of(&self, c: &Connection, doc: &J) -> Vec<Item> {
        let list: Vec<&J> = match path(doc, c.cfg("records_path")) {
            Some(J::Array(a)) => a.iter().collect(),
            Some(other) => vec![other],
            None => vec![],
        };
        let ext = c.cfg("external_id_key");
        list.into_iter()
            .map(|r| Item {
                values: self.map_record(c, r),
                external_id: (!ext.is_empty())
                    .then(|| path(r, ext))
                    .flatten()
                    .map(|v| v.as_str().map_or_else(|| v.to_string(), String::from)),
            })
            .collect()
    }

    fn accept(
        &self,
        c: &Connection,
        p: &Provider,
        kind: &str,
        items: Vec<Item>,
    ) -> Result<(Outcome, String), String> {
        let model = c.cfg("target_model").to_string();
        if model.is_empty() {
            return Err("the connection has no target model".into());
        }
        let run = self.start_run(c, "inbound", kind);
        let principal = self.connector_principal(c, &model);
        match self.ingress.accept(c, p, &model, items, &principal, &run) {
            Ok(o) => {
                let status = if o.rejected.is_empty() {
                    "succeeded"
                } else if o.accepted() + o.proposed.len() > 0 {
                    "partial"
                } else {
                    "failed"
                };
                let why = o
                    .rejected
                    .first()
                    .map(|(_, w)| w.clone())
                    .unwrap_or_default();
                self.finish_run(
                    &run,
                    status,
                    o.accepted(),
                    o.rejected.len(),
                    o.proposed.len(),
                    &why,
                );
                Ok((o, run))
            }
            Err(r) => {
                self.finish_run(&run, "failed", 0, 0, 0, &r.to_string());
                Err(r.to_string())
            }
        }
    }

    /// An inbound webhook for `connection`.
    pub fn receive_webhook(
        &self,
        connection: &str,
        header: &dyn Fn(&str) -> Option<String>,
        body: &[u8],
    ) -> Result<Outcome, HookError> {
        let (c, p) = self
            .connection(connection)
            .map_err(|_| HookError::NotFound)?;
        // A webhook receiver, or a service that both sends records and posts events back (Stripe).
        let receives = p.engine == Engine::WebhookIn
            || (p.engine == Engine::WebhookOut
                && p.can_receive()
                && !c.cfg("target_model").is_empty());
        if !receives {
            return Err(HookError::NotFound);
        }
        if body.len() > webhook::MAX_BODY {
            return Err(HookError::TooLarge);
        }
        let creds = self.credentials(&c).map_err(|_| HookError::Unauthorised)?;
        let secret = str_of(&creds, "signing_secret");
        if secret.is_empty() {
            return Err(HookError::Unauthorised);
        }
        let scheme = Scheme::parse(p.webhook_scheme).unwrap_or(Scheme::Standard);
        let v = webhook::verify(scheme, secret, header, body, now())
            .map_err(|_| HookError::Unauthorised)?;
        // A request that cannot be accepted now (paused, no basis) does not use up its id, so the
        // sender's retry after the connection is back is not taken for a replay.
        self.ingress
            .authorise(&c, &p, c.cfg("target_model"))
            .map_err(|r| {
                let why = r.to_string();
                self.egress.log(
                    &c,
                    "inbound",
                    c.cfg("target_model"),
                    0,
                    &[],
                    &[],
                    "",
                    "",
                    "refused",
                    &why,
                );
                HookError::Refused(why)
            })?;
        Seen::claim(&*self.backend, &c.id, &v.event_id, now())
            .map_err(|_: VerifyError| HookError::Unauthorised)?;
        let filter = c.cfg("event_types");
        if !filter.is_empty() {
            let ev = header("x-github-event").unwrap_or_default();
            if !filter.split(',').any(|x| x.trim() == ev) {
                return Ok(Outcome::default());
            }
        }
        let doc: J = serde_json::from_slice(body)
            .map_err(|_| HookError::Rejected("the body is not JSON".into()))?;
        let items = self.items_of(&c, &doc);
        let (o, _) = self
            .accept(&c, &p, "webhook", items)
            .map_err(HookError::Refused)?;
        if o.accepted() + o.proposed.len() == 0 && !o.rejected.is_empty() {
            return Err(HookError::Rejected(o.rejected[0].1.clone()));
        }
        Ok(o)
    }

    /// Pull from a REST API now.
    pub fn pull(&self, connection: &str) -> Result<J, String> {
        let (c, p) = self.connection(connection)?;
        if p.engine != Engine::RestPull {
            return Err(format!("{} does not pull", p.name));
        }
        let creds = self.credentials(&c)?;
        let url = self.target_url(&c, &creds)?;
        let hosts = self.hosts_for(&c, &p, &url);
        let mut headers = vec![("accept".to_string(), "application/json".to_string())];
        let key = str_of(&creds, "api_key");
        if !key.is_empty() {
            match c.cfg("header_name") {
                "" => headers.push(("authorization".into(), format!("Bearer {key}"))),
                name => headers.push((name.to_string(), key.to_string())),
            }
        } else if !c.oauth_tokens.is_empty() {
            headers.push((
                "authorization".into(),
                format!("Bearer {}", self.access_token(&c)?),
            ));
        }
        // Authorise before touching the network: no basis, no pull.
        if let Err(r) = self.ingress.authorise(&c, &p, c.cfg("target_model")) {
            let run = self.start_run(&c, "inbound", "pull");
            self.finish_run(&run, "failed", 0, 0, 0, &r.to_string());
            return Err(r.to_string());
        }
        let resp = self
            .client
            .send(Request {
                method: "GET",
                url: &url,
                headers,
                body: None,
                allowed_hosts: &hosts,
            })
            .map_err(|e| {
                let run = self.start_run(&c, "inbound", "pull");
                self.finish_run(&run, "failed", 0, 0, 0, &e.to_string());
                e.to_string()
            })?;
        if !(200..300).contains(&resp.status) {
            let run = self.start_run(&c, "inbound", "pull");
            let why = format!("the API answered {}", resp.status);
            self.finish_run(&run, "failed", 0, 0, 0, &why);
            return Err(why);
        }
        let doc: J = serde_json::from_slice(&resp.body)
            .map_err(|_| "the API did not answer JSON".to_string())?;
        let items = self.items_of(&c, &doc);
        let (o, run) = self.accept(&c, &p, "pull", items)?;
        Ok(
            json!({"run": run, "created": o.created, "updated": o.updated, "proposed": o.proposed.len(), "rejected": o.rejected.len()}),
        )
    }

    // ---- oauth ----------------------------------------------------------------------------

    fn oauth_config(&self, c: &Connection, p: &Provider) -> Result<OAuthConfig, String> {
        let def = p.oauth.as_ref().ok_or("this provider does not use OAuth")?;
        let pp = provider_policy(&*self.backend, p.id)
            .filter(|x| !x.oauth_client_id.is_empty())
            .ok_or("an administrator must first enter the OAuth client id and secret for this provider (obop ships none)")?;
        let secret = self
            .secrets
            .open(PURPOSE_CLIENT, &pp.oauth_client_secret)
            .map_err(|_| "the OAuth client secret is not set")?;
        let base =
            self.cfg.public_url.clone().ok_or(
                "OBOP_PUBLIC_URL is not set, so there is no address for the OAuth redirect",
            )?;
        let _ = c;
        Ok(OAuthConfig {
            auth_url: def.auth_url.into(),
            token_url: def.token_url.into(),
            client_id: pp.oauth_client_id,
            client_secret: secret,
            scopes: def.scopes.into(),
            redirect_uri: format!("{base}/oauth/callback"),
        })
    }

    pub fn oauth_begin(&self, connection: &str, actor: &str) -> Result<String, String> {
        let (c, p) = self.connection(connection)?;
        let cfg = self.oauth_config(&c, &p)?;
        oauth::begin(&*self.backend, &self.secrets, &cfg, &c.id, actor)
    }

    /// The redirect back from the provider. Returns the connection that is now authorised.
    pub fn oauth_callback(&self, state: &str, code: &str) -> Result<String, String> {
        let (pending, verifier) = oauth::take_state(&*self.backend, &self.secrets, state)?;
        let (c, p) = self.connection(&pending.connection)?;
        let cfg = self.oauth_config(&c, &p)?;
        let hosts: Vec<String> = p.allowed_hosts.iter().map(|s| s.to_string()).collect();
        let tokens = oauth::exchange_code(&self.client, &cfg, code, &verifier, &hosts)?;
        self.store_tokens(&c, &tokens)?;
        Ok(c.id)
    }

    fn store_tokens(&self, c: &Connection, t: &Tokens) -> Result<(), String> {
        let sealed = self.secrets.seal(PURPOSE_OAUTH, &t.to_json().to_string());
        let mut f = Fields::new();
        f.insert("oauth_tokens".into(), sealed.into());
        self.backend.update(CONNECTION, &c.id, &f)?;
        Ok(())
    }

    /// A usable access token, refreshed when it is about to expire.
    pub fn access_token(&self, c: &Connection) -> Result<String, String> {
        if c.oauth_tokens.is_empty() {
            return Err("the connection is not authorised yet".into());
        }
        let text = self.secrets.open(PURPOSE_OAUTH, &c.oauth_tokens)?;
        let t = Tokens::from_json(
            &serde_json::from_str(&text).map_err(|_| "stored tokens are malformed")?,
        )
        .ok_or("stored tokens are malformed")?;
        if !oauth::needs_refresh(&t, now()) {
            return Ok(t.access_token);
        }
        let p = self.provider(&c.provider).ok_or("unknown provider")?;
        let cfg = self.oauth_config(c, &p)?;
        let rt = t
            .refresh_token
            .as_deref()
            .ok_or("the access token expired and there is no refresh token; authorise again")?;
        let hosts: Vec<String> = p.allowed_hosts.iter().map(|s| s.to_string()).collect();
        let fresh = oauth::refresh(&self.client, &cfg, rt, &hosts)?;
        self.store_tokens(c, &fresh)?;
        Ok(fresh.access_token)
    }

    // ---- lifecycle ------------------------------------------------------------------------

    /// The first required setting or credential a connection lacks, by its label.
    pub fn missing_setting(
        &self,
        c: &Connection,
        p: &Provider,
    ) -> Result<Option<&'static str>, String> {
        let creds = self.credentials(c)?;
        for f in p.config.iter().filter(|f| f.required) {
            let in_creds = !str_of(&creds, f.key).is_empty();
            let have = if f.secret {
                in_creds
            } else {
                !c.cfg(f.key).is_empty() || in_creds
            };
            if !have {
                return Ok(Some(f.label));
            }
        }
        Ok(None)
    }

    /// Check a connection without sending personal data. Returns what was checked.
    pub fn test(&self, connection: &str) -> Result<J, String> {
        let (c, p) = self.connection(connection)?;
        let mut notes = Vec::new();
        let creds = self.credentials(&c)?;
        if let Some(label) = self.missing_setting(&c, &p)? {
            return Err(format!("{label} is missing"));
        }
        notes.push("required settings are present".to_string());
        match p.engine {
            Engine::WebhookOut | Engine::RestPull => {
                let url = self.target_url(&c, &creds)?;
                let hosts = self.hosts_for(&c, &p, &url);
                self.client.vet(&url, &hosts).map_err(|e| e.to_string())?;
                notes.push(
                    "the address passed the egress checks (https, allowed host, public address)"
                        .into(),
                );
                if p.engine == Engine::WebhookOut && p.body_format == "form" {
                    notes.push("no test request is sent: this service takes records only".into());
                } else if p.engine == Engine::WebhookOut {
                    // A test event carries no record and no personal data.
                    let run = self.start_run(&c, "outbound", "test");
                    let (sent, failed) = self.send_prepared(&c, &p, &run, "obop.test", "", &[]);
                    let (status, why) = if failed.is_empty() && sent > 0 {
                        ("succeeded", String::new())
                    } else {
                        ("failed", failed.clone())
                    };
                    self.finish_run(&run, status, 0, 0, 0, &why);
                    if status == "failed" {
                        return Err(if failed.is_empty() {
                            "nothing was delivered".into()
                        } else {
                            failed
                        });
                    }
                    notes.push("a test event without data was delivered".into());
                }
            }
            Engine::WebhookIn => {
                let sc = str_of(&creds, "signing_secret");
                if sc.chars().count() < 16 {
                    return Err("the signing secret must be at least 16 characters".into());
                }
                notes.push("the signing secret is set".into());
            }
            Engine::CsvExport => notes.push("nothing to reach: the file is produced here".into()),
            Engine::None => return Err("this provider is a definition only; nothing runs".into()),
        }
        Ok(json!({"ok": true, "checks": notes}))
    }

    /// The address a sender posts to, relative if no public URL is configured.
    pub fn webhook_path(&self, connection: &str) -> String {
        format!(
            "{}/hooks/in/{connection}",
            self.cfg.public_url.clone().unwrap_or_default()
        )
    }

    /// Revocation: forget credentials, tokens, pseudonym maps and queued deliveries.
    pub fn revoke(&self, connection: &str) -> usize {
        let mut f = Fields::new();
        f.insert("credentials".into(), "".into());
        f.insert("credentials_hint".into(), "revoked".into());
        f.insert("oauth_tokens".into(), "".into());
        let _ = self.backend.update(CONNECTION, connection, &f);
        self.queue.drop_connection(connection);
        self.egress.forget_connection(connection)
    }

    /// Retention: remove flow-log and sync-run rows older than their connection's retention
    /// (the instance default for a connection that is gone), and expired webhook ids and OAuth
    /// states. Returns `(flow_logs, sync_runs, other)`.
    pub fn purge(&self, at: u64) -> (usize, usize, usize) {
        let inst = InstancePolicy::load(&*self.backend);
        let days: std::collections::HashMap<String, i64> = self
            .backend
            .list(CONNECTION)
            .unwrap_or_default()
            .into_iter()
            .map(|(id, f)| {
                (
                    id,
                    crate::util::int_of(&f, "retention_days", DEFAULT_RETENTION_DAYS),
                )
            })
            .collect();
        let mut counts = [0usize; 2];
        for (i, table) in [FLOW_LOG, SYNC_RUN].into_iter().enumerate() {
            for (id, f) in self.backend.list(table).unwrap_or_default() {
                let keep = days
                    .get(str_of(&f, "connection"))
                    .copied()
                    .unwrap_or(inst.default_retention_days);
                let stamp = if table == FLOW_LOG {
                    "at"
                } else {
                    "started_at"
                };
                let Some(t) = parse_iso(str_of(&f, stamp)) else {
                    continue;
                };
                if t + (keep.max(1) as u64) * 86_400 < at
                    && self.backend.delete(table, &id).unwrap_or(false)
                {
                    counts[i] += 1;
                }
            }
        }
        let other = Seen::sweep(&*self.backend, at) + oauth::sweep(&*self.backend);
        (counts[0], counts[1], other)
    }

    /// Periodic work: due pulls, due retries, the daily purge. Returns a short description.
    pub fn tick(&self, at: u64) -> String {
        let mut did = Vec::new();
        let retried = self.process_queue(at);
        if retried > 0 {
            did.push(format!("{retried} retried"));
        }
        // Which pulls and the daily purge are due is decided under a lock and recorded in the
        // database, so with several server processes each pull and purge happens once.
        let Ok(_tick) = self.backend.lock("connectors.tick") else {
            return did.join("; ");
        };
        let last_of = |key: &str| -> u64 {
            self.backend
                .kv_get(key)
                .ok()
                .flatten()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0)
        };
        let due: Vec<String> = self
            .backend
            .list(CONNECTION)
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, f)| str_of(f, "state") == "connected")
            .filter_map(|(id, f)| {
                let cfg = obj_of(&f, "config");
                let mins = match cfg.get("interval_minutes") {
                    Some(J::Number(n)) => n.as_u64().unwrap_or(0),
                    Some(J::String(s)) => s.trim().parse().unwrap_or(0),
                    _ => 0,
                };
                let engine = self.provider(str_of(&f, "provider")).map(|p| p.engine);
                (mins > 0 && engine == Some(Engine::RestPull)).then_some((id, mins))
            })
            .filter(|(id, mins)| last_of(&format!("connectors.pull:{id}")) + mins * 60 <= at)
            .map(|(id, _)| id)
            .collect();
        for id in &due {
            let _ = self
                .backend
                .kv_put(&format!("connectors.pull:{id}"), &at.to_string(), None);
        }
        let purge_due = last_of("connectors.purge") + 86_400 <= at;
        if purge_due {
            let _ = self
                .backend
                .kv_put("connectors.purge", &at.to_string(), None);
        }
        drop(_tick);
        for id in due {
            let _ = self.pull(&id);
            did.push(format!("pulled {id}"));
        }
        if purge_due {
            let (a, b, c) = self.purge(at);
            did.push(format!(
                "purged {a} flow logs, {b} runs, {c} expired entries"
            ));
        }
        did.join("; ")
    }

    /// Wire the runtime to the dispatcher (events for subscribed connections, the erasure hook)
    /// and start the worker thread that delivers events and runs `tick`.
    pub fn start(self: &Arc<Self>) {
        let (tx, rx) = std::sync::mpsc::channel::<(String, String)>();
        let tx = Mutex::new(tx);
        self.dispatcher.observe(
            "connectors",
            Box::new(move |command, _p, input, out| {
                // A bulk create (D70) is one `create` event per record it made.
                if let Some(model) = command.strip_suffix(".bulk_create") {
                    if let Value::Map(m) = out
                        && let Some(Value::List(ids)) = m.get("ids")
                        && let Ok(t) = tx.lock()
                    {
                        for id in ids {
                            if let Value::Text(id) = id {
                                let _ = t.send((format!("{model}.create"), id.clone()));
                            }
                        }
                    }
                    return;
                }
                let id = match out {
                    Value::Map(m) => m.get("id"),
                    _ => None,
                }
                .or(match input {
                    Value::Map(m) => m.get("id"),
                    _ => None,
                });
                if let Some(Value::Text(id)) = id {
                    let _ = tx.lock().map(|t| t.send((command.to_string(), id.clone())));
                }
            }),
        );
        let this = self.clone();
        std::thread::spawn(move || {
            loop {
                match rx.recv_timeout(Duration::from_secs(5)) {
                    Ok((command, id)) => {
                        // Erasure first: a party that was erased must not be sent anywhere.
                        if command == "party.erase" {
                            this.egress.erase_subject(&format!("party:{id}"));
                        } else {
                            this.handle_event(&command, &id);
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        this.tick(now());
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        });
    }
}

fn sanitize_name(s: &str) -> String {
    let n: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if n.is_empty() { "export".into() } else { n }
}

/// CSV of treated rows: `ref` first, then the sent fields in allowlist order.
pub fn rows_to_csv(rows: &[Map<String, J>], fields: &[String]) -> String {
    let mut cols: Vec<String> = vec!["ref".into()];
    let wanted: BTreeSet<&String> = fields.iter().collect();
    cols.extend(fields.iter().filter(|f| wanted.contains(f)).cloned());
    let mut lines = vec![crate::util::csv_row(&cols)];
    for r in rows {
        let cells: Vec<String> = cols
            .iter()
            .map(|c| match r.get(c) {
                Some(J::String(s)) => s.clone(),
                Some(J::Null) | None => String::new(),
                Some(other) => other.to_string(),
            })
            .collect();
        lines.push(crate::util::csv_row(&cells));
    }
    lines.join("\r\n") + "\r\n"
}

impl From<Refusal> for String {
    fn from(r: Refusal) -> String {
        r.to_string()
    }
}
