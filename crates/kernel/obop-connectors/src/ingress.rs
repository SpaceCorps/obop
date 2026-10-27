//! The gate for inbound records: `Ingress::accept`.
//!
//! Records come in only through the same governed `create` and `update` commands as any other
//! write, as the connection's *connector principal*: an agent-kind principal limited to the groups
//! its owner holds, so it can never do more than its owner, and an action that needs approval
//! stays a proposal. The gate also checks the connection (connected, a legal basis that has not
//! been withdrawn), accepts only the fields on the allowlist (unknown or not-allowed fields reject
//! the record, they are not dropped silently), applies the same class policy as outbound flows
//! (a denied class cannot be written by a connector), restores tokens this connection issued,
//! remembers where each record came from and logs one `data_flow_log` row.

use obop_commands::{CallError, Dispatcher, Principal, Surface, Value, Vault};
use obop_storage::{Backend, Fields};
use serde_json::{Map, Value as J};
use std::sync::Arc;

use crate::catalog::Provider;
use crate::egress::{Egress, Refusal};
use crate::model::*;
use crate::policy::Treatment;
use crate::util::{iso, now, sha256_hex, str_of};

pub struct Item {
    pub values: Map<String, J>,
    /// The sender's id for the record: the next delivery of the same id updates the record.
    pub external_id: Option<String>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub created: usize,
    pub updated: usize,
    /// Plan ids of changes that need a human to approve them.
    pub proposed: Vec<String>,
    pub rejected: Vec<(usize, String)>,
}

impl Outcome {
    pub fn accepted(&self) -> usize {
        self.created + self.updated
    }
}

pub struct Ingress {
    backend: Arc<dyn Backend>,
    egress: Arc<Egress>,
    dispatcher: Arc<Dispatcher>,
}

impl Ingress {
    pub fn new(
        backend: Arc<dyn Backend>,
        egress: Arc<Egress>,
        dispatcher: Arc<Dispatcher>,
    ) -> Self {
        Self {
            backend,
            egress,
            dispatcher,
        }
    }

    /// May this connection accept records into `model`? Same questions as outbound.
    pub fn authorise(
        &self,
        conn: &Connection,
        provider: &Provider,
        model: &str,
    ) -> Result<(), Refusal> {
        let inst = InstancePolicy::try_load(&*self.backend).map_err(|_| Refusal::StoreBusy)?;
        if !inst.flows_enabled {
            return Err(Refusal::FlowsDisabled);
        }
        if !provider.can_receive() {
            return Err(Refusal::DirectionNotAllowed);
        }
        if !conn.is_connected() {
            return Err(Refusal::NotConnected(conn.state.clone()));
        }
        let consents = try_consents_of(&*self.backend, &conn.id).map_err(|_| Refusal::StoreBusy)?;
        let Some(consent) = consents.iter().find(|c| c.is_active()) else {
            return Err(if consents.is_empty() {
                Refusal::NoConsent
            } else {
                Refusal::ConsentWithdrawn
            });
        };
        if conn.allowlist.get(model).is_none_or(Vec::is_empty) {
            return Err(Refusal::EmptyAllowlist);
        }
        let plan = self.egress.plan(conn, provider, model)?;
        let uncovered: Vec<String> = plan
            .classes()
            .into_iter()
            .filter(|c| *c != obop_types::DataClass::None && !consent.classes.contains(c))
            .map(|c| c.as_str().to_string())
            .collect();
        if !uncovered.is_empty() {
            return Err(Refusal::ConsentClasses(uncovered));
        }
        Ok(())
    }

    /// Accept `items` into `model`. `principal` is the connection's connector principal.
    pub fn accept(
        &self,
        conn: &Connection,
        provider: &Provider,
        model: &str,
        items: Vec<Item>,
        principal: &Principal,
        run_id: &str,
    ) -> Result<Outcome, Refusal> {
        if let Err(r) = self.authorise(conn, provider, model) {
            self.egress.log(
                conn,
                "inbound",
                model,
                0,
                &[],
                &[],
                "",
                "",
                "refused",
                &r.to_string(),
            );
            return Err(r);
        }
        if items.len() > MAX_ROWS_PER_FLOW {
            let r = Refusal::TooManyRows(MAX_ROWS_PER_FLOW);
            self.egress.log(
                conn,
                "inbound",
                model,
                0,
                &[],
                &[],
                "",
                "",
                "refused",
                &r.to_string(),
            );
            return Err(r);
        }
        let plan = self.egress.plan(conn, provider, model)?;
        let mut out = Outcome::default();
        let mut hasher_input = String::new();
        for (i, item) in items.into_iter().enumerate() {
            match self.accept_one(conn, &plan, model, item, principal, run_id) {
                Ok(Done::Created(id)) => {
                    out.created += 1;
                    hasher_input.push_str(&id);
                }
                Ok(Done::Updated(id)) => {
                    out.updated += 1;
                    hasher_input.push_str(&id);
                }
                Ok(Done::Proposed(plan_id)) => out.proposed.push(plan_id),
                Err(why) => out.rejected.push((i, why)),
            }
        }
        let allowed: Vec<String> = plan.sent().map(|f| f.name.clone()).collect();
        let denied: Vec<String> = plan.denied().map(|f| f.name.clone()).collect();
        let outcome = if out.rejected.is_empty() {
            "received"
        } else {
            "rejected"
        };
        let reason = if out.rejected.is_empty() {
            String::new()
        } else {
            format!("{} record(s) rejected", out.rejected.len())
        };
        self.egress.log(
            conn,
            "inbound",
            model,
            out.accepted() + out.proposed.len(),
            &allowed,
            &denied,
            &plan.mode_summary(),
            &sha256_hex(hasher_input.as_bytes()),
            outcome,
            &reason,
        );
        Ok(out)
    }

    fn accept_one(
        &self,
        conn: &Connection,
        plan: &crate::egress::Plan,
        model: &str,
        item: Item,
        principal: &Principal,
        run_id: &str,
    ) -> Result<Done, String> {
        let mut values = Map::new();
        for (k, v) in &item.values {
            let Some(pf) = plan.fields.iter().find(|f| &f.name == k) else {
                return Err(format!(
                    "field {model}.{k} is not on the allowlist or does not exist"
                ));
            };
            if pf.decision.treatment == Treatment::Deny {
                return Err(format!(
                    "field {model}.{k} may not be written by a connector ({})",
                    pf.decision.why.as_str()
                ));
            }
            // A value that is one of this connection's own tokens means what it stood for.
            let v = match v {
                J::String(s) => self
                    .egress
                    .restore(&conn.id, s)
                    .map_or_else(|| v.clone(), J::from),
                other => other.clone(),
            };
            values.insert(k.clone(), v);
        }
        let ext_key = item
            .external_id
            .as_ref()
            .map(|e| format!("{}:{}", conn.id, sha256_hex(e.as_bytes())));
        // Whether this record was imported before decides between an update and a new record: a
        // lookup the store refused is not "never imported", which would make a duplicate of it
        // (the pull runs on the worker, where nothing else would stop it). The item is refused
        // for now and the next run reads it again.
        let existing = match &ext_key {
            Some(k) => self
                .backend
                .get(EXTMAP, k)?
                .map(|f| str_of(&f, "record").to_string()),
            None => None,
        };
        let call = |cmd: &str, input: Value| {
            self.dispatcher.call(
                Surface::Rest,
                principal,
                &mut Vault::new(),
                &format!("{model}.{cmd}"),
                &input,
            )
        };
        let to_value = |extra: Option<&str>| {
            let mut m = values.clone();
            if let Some(id) = extra {
                m.insert("id".into(), J::from(id));
            }
            Value::from_json(&J::Object(m))
        };
        let (cmd, input, prior) = match &existing {
            Some(rec) => {
                let id = rec.split_once(':').map_or(rec.as_str(), |(_, id)| id);
                ("update", to_value(Some(id)), Some(id.to_string()))
            }
            None => ("create", to_value(None), None),
        };
        match call(cmd, input) {
            Ok(resp) => {
                let id = prior.unwrap_or_else(|| match &resp.value {
                    Value::Map(m) => match m.get("id") {
                        Some(Value::Text(t)) => t.clone(),
                        _ => String::new(),
                    },
                    _ => String::new(),
                });
                self.remember(conn, model, &id, ext_key.as_deref(), run_id)
                    .map_err(|e| {
                        format!(
                            "the record was saved, but its link to the external id could not be: {e}; the next run may add it again"
                        )
                    })?;
                Ok(if cmd == "create" {
                    Done::Created(id)
                } else {
                    Done::Updated(id)
                })
            }
            Err(CallError::ApprovalRequired { plan_id }) => Ok(Done::Proposed(plan_id)),
            Err(CallError::Handler(m)) => Err(m),
            Err(
                CallError::Forbidden(_) | CallError::UnknownCommand(_) | CallError::NotOnSurface(_),
            ) => Err(format!(
                "the connection's owner may not {cmd} {model} records"
            )),
            Err(CallError::Restore(_) | CallError::UnknownPlan(_)) => {
                Err("the record could not be written".into())
            }
            // An inbound record carries no version to compare, so this cannot happen today.
            Err(e @ CallError::PreconditionFailed { .. }) => {
                Err(obop_commands::errors::classify_with(&e, &Default::default()).cli)
            }
        }
    }

    fn remember(
        &self,
        conn: &Connection,
        model: &str,
        id: &str,
        ext_key: Option<&str>,
        run_id: &str,
    ) -> Result<(), String> {
        use obop_storage::fault::{Fault, patiently};
        if id.is_empty() {
            return Ok(());
        }
        let record = format!("{model}:{id}");
        let pause = std::time::Duration::from_millis(250);
        let mut f = Fields::new();
        f.insert("connection".into(), conn.id.clone().into());
        f.insert("run".into(), run_id.into());
        f.insert("at".into(), iso(now()).into());
        // Where the record came from (erasure and revocation find it by this): an entry that is
        // there already is the same record seen again.
        match patiently(4, pause, || self.backend.insert(ORIGIN, &record, &f)) {
            Ok(()) => {}
            Err(e) if Fault::is_transient(&e) => return Err(e),
            Err(_) => {}
        }
        if let Some(k) = ext_key {
            let mut m = Fields::new();
            m.insert("connection".into(), conn.id.clone().into());
            m.insert("record".into(), record.into());
            // The link that makes the next run update this record instead of adding it again.
            patiently(4, pause, || match self.backend.insert(EXTMAP, k, &m) {
                Ok(()) => Ok(()),
                Err(e) if Fault::is_transient(&e) => Err(e),
                // Already linked (an earlier run): the link is brought up to date.
                Err(_) => self.backend.update(EXTMAP, k, &m).map(|_| ()),
            })?;
        }
        Ok(())
    }
}

enum Done {
    Created(String),
    Updated(String),
    Proposed(String),
}
