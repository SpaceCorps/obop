//! The gate every outbound flow passes: `Egress::prepare`.
//!
//! It refuses unless the instance allows flows, the provider may send, the connection is
//! `connected`, a legal basis is recorded and not withdrawn and covers the classes that would
//! leave, and (for personal data) the agreement with the provider is recorded. It then keeps only
//! the fields on the connection's allowlist (minimisation: an empty allowlist sends nothing),
//! treats each by its data class (`policy`), checks the rate and volume caps and writes one
//! `data_flow_log` row: when, what model, how many records, which fields, the treatments and a
//! SHA-256 of the payload, never the payload. Record ids never leave: each row carries a keyed
//! pseudonym as `ref`, and the instance remembers which record it stands for.

use hmac::{Hmac, KeyInit, Mac};
use obop_commands::Dispatcher;
use obop_storage::{ApplyError, Backend, Fields, Op};
use obop_types::DataClass;
use serde_json::{Map, Value as J, json};
use sha2::Sha256;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::catalog::Provider;
use crate::model::*;
use crate::policy::{Decision, FieldInfo, Treatment};
use crate::secretbox::{SecretBox, hex};
use crate::util::{iso, now, parse_iso, sha256_hex, str_of};

type HmacSha256 = Hmac<Sha256>;

pub const REDACTED: &str = "[redacted]";

#[derive(Debug, Clone)]
pub struct FieldMeta {
    pub name: String,
    pub class: DataClass,
    pub personal: bool,
}

/// Where the engines learn a model's fields and their classes.
pub trait Catalog: Send + Sync {
    fn fields(&self, model: &str) -> Option<Vec<FieldMeta>>;
}

impl Catalog for Dispatcher {
    fn fields(&self, model: &str) -> Option<Vec<FieldMeta>> {
        self.model_fields(model).map(|fs| {
            fs.into_iter()
                .map(|f| FieldMeta {
                    name: f.name.to_string(),
                    // A field without a class is treated as a secret: it never leaves.
                    class: f.class.unwrap_or(DataClass::Secret),
                    personal: f.personal_data,
                })
                .collect()
        })
    }
}

/// A keyed pseudonym of `value`: stable for one connection, different for every other, and not
/// reversible without the master key (and, even with it, only by guessing the plain value).
pub fn pseudonym(secrets: &SecretBox, connection: &str, value: &J) -> Result<String, String> {
    let text = match value {
        J::String(s) => s.clone(),
        other => other.to_string(),
    };
    let mut m =
        HmacSha256::new_from_slice(&secrets.pseudonym_key(&format!("pseudonym:{connection}"))?)
            .expect("HMAC takes any key length");
    m.update(b"v1|");
    m.update(text.as_bytes());
    Ok(format!("psn_{}", hex(&m.finalize().into_bytes()[..16])))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    FlowsDisabled,
    DirectionNotAllowed,
    NotConnected(String),
    NoConsent,
    ConsentWithdrawn,
    ConsentClasses(Vec<String>),
    UnknownModel(String),
    EmptyAllowlist,
    NothingAllowed,
    NoDpa(String),
    NotEuAdequate(String),
    RateLimited(i64),
    VolumeCap(i64),
    TooManyRows(usize),
    /// The policy, the connection's consents or the provider's record could not be read because
    /// the store was busy (D74). Not a refusal of the data flow: try again.
    StoreBusy,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FlowsDisabled => write!(f, "flows are switched off for this instance"),
            Self::StoreBusy => f.write_str(
                &obop_storage::fault::Fault::Busy.message("the database is busy, try again"),
            ),
            Self::DirectionNotAllowed => {
                write!(f, "the provider does not send data in this direction")
            }
            Self::NotConnected(s) => write!(f, "the connection is {s}, not connected"),
            Self::NoConsent => write!(f, "no legal basis is recorded for this connection"),
            Self::ConsentWithdrawn => write!(f, "the consent was withdrawn"),
            Self::ConsentClasses(c) => write!(
                f,
                "the recorded consent does not cover the data classes {}; record it again",
                c.join(", ")
            ),
            Self::UnknownModel(m) => write!(f, "unknown model {m}"),
            Self::EmptyAllowlist => write!(
                f,
                "the field allowlist for this model is empty, so nothing is sent"
            ),
            Self::NothingAllowed => write!(f, "the policy leaves no allowed field for this model"),
            Self::NoDpa(p) => write!(
                f,
                "no agreement with {p} is recorded for personal data (provider policy)"
            ),
            Self::NotEuAdequate(p) => write!(
                f,
                "the instance only allows providers with an EU adequacy basis; {p} is not one"
            ),
            Self::RateLimited(n) => write!(f, "rate limit reached ({n} flows per hour)"),
            Self::VolumeCap(n) => write!(f, "daily volume cap reached ({n} records)"),
            Self::TooManyRows(n) => write!(f, "more than {n} records in one flow"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlannedField {
    pub name: String,
    pub class: DataClass,
    pub personal: bool,
    pub decision: Decision,
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub model: String,
    /// The allowlisted fields of the model with their treatment.
    pub fields: Vec<PlannedField>,
    /// Allowlisted names the model does not have.
    pub unknown: Vec<String>,
}

impl Plan {
    pub fn sent(&self) -> impl Iterator<Item = &PlannedField> {
        self.fields
            .iter()
            .filter(|f| f.decision.treatment != Treatment::Deny)
    }

    pub fn denied(&self) -> impl Iterator<Item = &PlannedField> {
        self.fields
            .iter()
            .filter(|f| f.decision.treatment == Treatment::Deny)
    }

    pub fn sends_personal_data(&self) -> bool {
        self.sent().any(|f| f.personal)
    }

    /// The classes of the fields that would leave, without duplicates, in class order.
    pub fn classes(&self) -> Vec<DataClass> {
        let mut c: Vec<DataClass> = self.sent().map(|f| f.class).collect();
        c.sort();
        c.dedup();
        c
    }

    pub fn mode_summary(&self) -> String {
        let mut n: BTreeMap<&str, usize> = BTreeMap::new();
        for f in &self.fields {
            *n.entry(f.decision.treatment.as_str()).or_default() += 1;
        }
        n.iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(";")
    }
}

#[derive(Debug, Clone)]
pub struct Report {
    pub flow_id: String,
    pub model: String,
    pub record_count: usize,
    pub fields_sent: Vec<String>,
    pub fields_denied: Vec<String>,
    pub payload_hash: String,
    pub mode: String,
}

#[derive(Debug)]
pub struct Prepared {
    pub rows: Vec<Map<String, J>>,
    pub report: Report,
}

pub struct Egress {
    backend: Arc<dyn Backend>,
    secrets: SecretBox,
    catalog: Arc<dyn Catalog>,
}

/// How a row is treated: for real (tokens are stored) or as a preview (nothing is stored).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Live,
    Preview,
}

impl Egress {
    pub fn new(backend: Arc<dyn Backend>, secrets: SecretBox, catalog: Arc<dyn Catalog>) -> Self {
        Self {
            backend,
            secrets,
            catalog,
        }
    }

    pub fn secrets(&self) -> &SecretBox {
        &self.secrets
    }

    /// The treatment of every allowlisted field of `model`. Changes nothing.
    pub fn plan(
        &self,
        conn: &Connection,
        provider: &Provider,
        model: &str,
    ) -> Result<Plan, Refusal> {
        let meta = self
            .catalog
            .fields(model)
            .ok_or_else(|| Refusal::UnknownModel(model.into()))?;
        let names = conn.allowlist.get(model).cloned().unwrap_or_default();
        let mut fields = Vec::new();
        let mut unknown = Vec::new();
        for n in names {
            match meta.iter().find(|m| m.name == n) {
                Some(m) => {
                    let d = conn.policy.decide(
                        &FieldInfo {
                            model,
                            field: &m.name,
                            class: m.class,
                            personal_data: m.personal,
                        },
                        provider.allows_class(m.class),
                    );
                    fields.push(PlannedField {
                        name: m.name.clone(),
                        class: m.class,
                        personal: m.personal,
                        decision: d,
                    });
                }
                None => unknown.push(n),
            }
        }
        Ok(Plan {
            model: model.into(),
            fields,
            unknown,
        })
    }

    /// Everything that must hold before data may leave. `Ok` carries the plan.
    pub fn authorise(
        &self,
        conn: &Connection,
        provider: &Provider,
        model: &str,
    ) -> Result<Plan, Refusal> {
        let inst = InstancePolicy::try_load(&*self.backend).map_err(|_| Refusal::StoreBusy)?;
        if !inst.flows_enabled {
            return Err(Refusal::FlowsDisabled);
        }
        if !provider.can_send() {
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
        let plan = self.plan(conn, provider, model)?;
        if plan.sent().next().is_none() {
            return Err(Refusal::NothingAllowed);
        }
        let uncovered: Vec<String> = plan
            .classes()
            .into_iter()
            .filter(|c| *c != DataClass::None && !consent.classes.contains(c))
            .map(|c| c.as_str().to_string())
            .collect();
        if !uncovered.is_empty() {
            return Err(Refusal::ConsentClasses(uncovered));
        }
        if plan.sends_personal_data() {
            // A refused read of the agreement is the store's trouble, not "no agreement recorded".
            let agreement =
                try_provider_policy(&*self.backend, provider.id).map_err(|_| Refusal::StoreBusy)?;
            if inst.require_dpa && !agreement.is_some_and(|p| p.dpa_recorded) {
                return Err(Refusal::NoDpa(provider.name.into()));
            }
            if inst.eu_only && provider.eu_adequate != "yes" {
                return Err(Refusal::NotEuAdequate(provider.name.into()));
            }
        }
        Ok(plan)
    }

    /// Flows in the last hour and records in the last day that were released for `connection`.
    /// A log that cannot be read counts as none (for a display); the gate uses [`Egress::try_usage`].
    pub fn usage(&self, connection: &str, at: u64) -> (i64, i64) {
        self.try_usage(connection, at).unwrap_or_default()
    }

    /// As [`Egress::usage`], and a refused read of the log is an error: read as "no flows yet" it
    /// let the rate and volume caps pass.
    pub fn try_usage(&self, connection: &str, at: u64) -> Result<(i64, i64), String> {
        let (mut flows, mut records) = (0, 0);
        for (_, f) in self.backend.list(FLOW_LOG)? {
            if str_of(&f, "connection") != connection
                || str_of(&f, "outcome") != "released"
                || str_of(&f, "direction") != "outbound"
            {
                continue;
            }
            let Some(t) = parse_iso(str_of(&f, "at")) else {
                continue;
            };
            if t + 3600 > at {
                flows += 1;
            }
            if t + 86_400 > at {
                records += f.get("record_count").and_then(J::as_i64).unwrap_or(0);
            }
        }
        Ok((flows, records))
    }

    /// The gate. `rows` are records read as the connection's owner (so permissions already
    /// applied), each with its `id`. Returns the rows to send, treated and minimised.
    pub fn prepare(
        &self,
        conn: &Connection,
        provider: &Provider,
        model: &str,
        rows: Vec<Map<String, J>>,
    ) -> Result<Prepared, Refusal> {
        let outcome = self.prepare_inner(conn, provider, model, rows);
        if let Err(r) = &outcome
            && !matches!(r, Refusal::StoreBusy)
        {
            self.log(
                conn,
                "outbound",
                model,
                0,
                &[],
                &[],
                "",
                "",
                "refused",
                &r.to_string(),
            );
        }
        outcome
    }

    fn prepare_inner(
        &self,
        conn: &Connection,
        provider: &Provider,
        model: &str,
        rows: Vec<Map<String, J>>,
    ) -> Result<Prepared, Refusal> {
        let plan = self.authorise(conn, provider, model)?;
        if rows.len() > MAX_ROWS_PER_FLOW {
            return Err(Refusal::TooManyRows(MAX_ROWS_PER_FLOW));
        }
        let at = now();
        // The caps are checked against what the log says: a log that cannot be read is not "no
        // flows yet", and nothing leaves on a guess (D74).
        let (flows, records) = self
            .try_usage(&conn.id, at)
            .map_err(|_| Refusal::StoreBusy)?;
        if flows >= conn.rate_limit_per_hour {
            return Err(Refusal::RateLimited(conn.rate_limit_per_hour));
        }
        if records + rows.len() as i64 > conn.volume_cap_per_day {
            return Err(Refusal::VolumeCap(conn.volume_cap_per_day));
        }
        let out: Vec<Map<String, J>> = rows
            .iter()
            .map(|r| self.apply(conn, &plan, r, Mode::Live))
            .collect::<Result<_, _>>()
            .map_err(|_| Refusal::StoreBusy)?;
        let hash = sha256_hex(serde_json::to_string(&out).unwrap_or_default().as_bytes());
        let sent: Vec<String> = plan.sent().map(|f| f.name.clone()).collect();
        let denied: Vec<String> = plan
            .denied()
            .map(|f| f.name.clone())
            .chain(plan.unknown.iter().cloned())
            .collect();
        let mode = plan.mode_summary();
        // Every flow is recorded before it leaves: a row that cannot be written is a flow that does
        // not happen (it was released with no trace, and the caller was told "applied none").
        let flow_id = self
            .try_log(
                conn,
                "outbound",
                model,
                out.len(),
                &sent,
                &denied,
                &mode,
                &hash,
                "released",
                "",
            )
            .map_err(|_| Refusal::StoreBusy)?;
        Ok(Prepared {
            rows: out,
            report: Report {
                flow_id,
                model: model.into(),
                record_count: rows.len(),
                fields_sent: sent,
                fields_denied: denied,
                payload_hash: hash,
                mode,
            },
        })
    }

    /// What a sample would look like after the policy, without any gate and without storing
    /// anything (no log row, no token, no pseudonym map). For the privacy preview.
    pub fn preview(
        &self,
        conn: &Connection,
        provider: &Provider,
        model: &str,
        rows: &[Map<String, J>],
    ) -> Result<(Plan, Vec<Map<String, J>>), Refusal> {
        let plan = self.plan(conn, provider, model)?;
        let out = rows
            .iter()
            .map(|r| self.apply(conn, &plan, r, Mode::Preview))
            .collect::<Result<_, _>>()
            .map_err(|_| Refusal::StoreBusy)?;
        Ok((plan, out))
    }

    fn apply(
        &self,
        conn: &Connection,
        plan: &Plan,
        row: &Map<String, J>,
        mode: Mode,
    ) -> Result<Map<String, J>, String> {
        let id = str_of(row, "id");
        let record = format!("{}:{id}", plan.model);
        let reference = pseudonym(&self.secrets, &conn.id, &J::from(record.clone()))?;
        if mode == Mode::Live {
            let mut f = Fields::new();
            f.insert("connection".into(), conn.id.clone().into());
            f.insert("record".into(), record.clone().into());
            // The map is what lets erasure and revocation find what was sent. An entry that is
            // there already (the same record sent before) is as it should be; a store that
            // refused is not, and nothing leaves without it.
            let key = format!("{}:{reference}", conn.id);
            if let Err(e) = self.backend.insert(PSEUDONYMS, &key, &f) {
                crate::util::row_exists_or(&*self.backend, PSEUDONYMS, &key, e)?;
            }
        }
        let mut out = Map::new();
        out.insert("ref".into(), reference.into());
        for pf in plan.sent() {
            let Some(v) = row.get(&pf.name) else { continue };
            let treated = match (pf.decision.treatment, v) {
                (_, J::Null) => J::Null,
                (Treatment::Allow, v) => v.clone(),
                (Treatment::Redact, _) => J::from(REDACTED),
                (Treatment::Pseudonymise, v) => J::from(pseudonym(&self.secrets, &conn.id, v)?),
                (Treatment::Mask, v) => J::from(self.mask(conn, &record, &pf.name, v, mode)?),
                (Treatment::Deny, _) => continue,
            };
            out.insert(pf.name.clone(), treated);
        }
        Ok(out)
    }

    /// A reversible stand-in for `v`, stable per connection and value. The value is kept sealed
    /// in the instance so an inbound response that cites the token can be restored.
    ///
    /// A token the vault could not store is not sent (a response that cites it could not be
    /// restored, and the value would be gone): a refusal of the store is an error (D74).
    fn mask(
        &self,
        conn: &Connection,
        record: &str,
        field: &str,
        v: &J,
        mode: Mode,
    ) -> Result<String, String> {
        let text = match v {
            J::String(s) => s.clone(),
            other => other.to_string(),
        };
        let index = pseudonym(
            &self.secrets,
            &format!("{}:vault", conn.id),
            &J::from(text.clone()),
        )?;
        if mode == Mode::Preview {
            return Ok(format!("TKN_{}", &index[4..16]));
        }
        let key = format!("{}:{index}", conn.id);
        if let Some(f) = self.backend.get(VAULT, &key)? {
            // The entry by value is there: make sure the entry by token is too (an earlier flow
            // that was refused between the two writes, or one of an older release, left it out),
            // or the token would be sent and could never be restored.
            let token = str_of(&f, "token").to_string();
            self.ensure_token_entry(&token, &f)?;
            return Ok(token);
        }
        let token = format!("TKN_{}", crate::util::random_hex(6));
        let mut f = Fields::new();
        f.insert("connection".into(), conn.id.clone().into());
        f.insert("token".into(), token.clone().into());
        f.insert("record".into(), record.into());
        f.insert("field".into(), field.into());
        f.insert(
            "value".into(),
            self.secrets
                .seal(&format!("vault:{}", conn.id), &text)
                .into(),
        );
        // Two keys, one entry: by token (to restore) and by value (to stay stable). The one that
        // restore reads goes first: if the second write is refused the token was never sent, and
        // an entry by token that nothing points to is harmless (erasure finds it by its record).
        let by_token = format!("tok:{token}");
        self.backend.insert(VAULT, &by_token, &f)?;
        match self.backend.insert(VAULT, &key, &f) {
            Ok(()) => Ok(token),
            Err(e) => {
                let _ = self.backend.delete(VAULT, &by_token);
                if obop_storage::fault::Fault::is_transient(&e) {
                    return Err(e);
                }
                // Another flow stored the same value first: its token stands.
                match self.backend.get(VAULT, &key)? {
                    Some(f) => {
                        let token = str_of(&f, "token").to_string();
                        self.ensure_token_entry(&token, &f)?;
                        Ok(token)
                    }
                    None => Err(e),
                }
            }
        }
    }

    /// The entry that `restore` reads for `token` is there, written from `f` if it is not.
    fn ensure_token_entry(&self, token: &str, f: &Fields) -> Result<(), String> {
        let k = format!("tok:{token}");
        if self.backend.get(VAULT, &k)?.is_some() {
            return Ok(());
        }
        match self.backend.insert(VAULT, &k, f) {
            Ok(()) => Ok(()),
            Err(e) if obop_storage::fault::Fault::is_transient(&e) => Err(e),
            // Written by another flow in between.
            Err(e) => match self.backend.get(VAULT, &k)? {
                Some(_) => Ok(()),
                None => Err(e),
            },
        }
    }

    /// Restore a token this connection issued, for an inbound response. Anything else, including
    /// pseudonyms (which have no way back), stays as it is: `None`.
    pub fn restore(&self, connection: &str, text: &str) -> Option<String> {
        if !text.starts_with("TKN_") {
            return None;
        }
        let f = self.backend.get(VAULT, &format!("tok:{text}")).ok()??;
        if str_of(&f, "connection") != connection {
            return None;
        }
        self.secrets
            .open(&format!("vault:{connection}"), str_of(&f, "value"))
            .ok()
    }

    /// The id of the record a reference pseudonym stood for, for a connection that asked for it
    /// (`resolve_refs`): the way an event from the receiver finds the record it is about. It reads
    /// the map the erasure hook already clears, so once a subject is erased the pseudonym stays
    /// unresolvable. `None` for anything that is not a reference this connection issued.
    pub fn resolve_ref(&self, connection: &str, text: &str) -> Option<String> {
        if !text.starts_with("psn_") {
            return None;
        }
        let f = self
            .backend
            .get(PSEUDONYMS, &format!("{connection}:{text}"))
            .ok()??;
        let (_, id) = str_of(&f, "record").split_once(':')?;
        Some(id.to_string())
    }

    /// Drop everything the instance holds that links a pseudonym or a token to `record`
    /// (`model:id`): the erasure hook. Returns how many entries were removed per connection.
    ///
    /// A refusal of the store is an error, never "nothing to erase" (D74): personal data that
    /// survives must not be answered as erased. Safe to run again.
    ///
    /// The entries of one connection and the `erased` row of the data flow log that says so go in
    /// one atomic batch: either both are done or neither is, so a row refused after the deletes
    /// can no longer be lost (the repeat found nothing to delete and wrote no row).
    pub fn erase_subject(&self, record: &str) -> Result<BTreeMap<String, usize>, String> {
        let mut per: BTreeMap<String, usize> = BTreeMap::new();
        // Another process may remove an entry between the list and the batch: the batch is then
        // refused whole (nothing changed) and the entries are listed again.
        for attempt in 0..3 {
            let mut found: BTreeMap<String, Vec<(&'static str, String)>> = BTreeMap::new();
            for table in [PSEUDONYMS, VAULT, EXTMAP] {
                for (id, f) in self.backend.list(table)? {
                    if str_of(&f, "record") == record {
                        found
                            .entry(str_of(&f, "connection").to_string())
                            .or_default()
                            .push((table, id));
                    }
                }
            }
            let mut raced = false;
            for (conn, entries) in found {
                let mut ops: Vec<Op> = entries
                    .iter()
                    .map(|(table, id)| Op::Delete {
                        model: (*table).into(),
                        id: id.clone(),
                    })
                    .collect();
                match Connection::load(&*self.backend, &conn) {
                    Ok(c) => {
                        let (id, fields) = Self::log_row(
                            &c,
                            "outbound",
                            record.split(':').next().unwrap_or(""),
                            0,
                            &[],
                            &[],
                            "",
                            "",
                            "erased",
                            &format!(
                                "subject erased: {} pseudonym or token entries removed",
                                entries.len()
                            ),
                        );
                        ops.push(Op::Insert {
                            model: FLOW_LOG.into(),
                            id,
                            fields,
                        });
                    }
                    Err(e) if obop_storage::fault::Fault::is_transient(&e) => return Err(e),
                    // The connection is gone: there is no row to say it on.
                    Err(_) => {}
                }
                match self.backend.apply_checked(&ops) {
                    Ok(()) => {
                        *per.entry(conn).or_default() += entries.len();
                    }
                    Err(ApplyError::Conflict { .. }) if attempt < 2 => {
                        raced = true;
                        break;
                    }
                    Err(ApplyError::Conflict { reason, .. }) => return Err(reason),
                    Err(ApplyError::Storage(e)) => return Err(e),
                }
            }
            if !raced {
                return Ok(per);
            }
        }
        Ok(per)
    }

    /// Drop everything a connection holds that can be linked back (revocation): its tokens and
    /// pseudonym maps. The pseudonyms already sent stay meaningless without the key.
    ///
    /// A refusal of the store is an error (D74); the keys are forgotten only when every entry is
    /// gone, so a repeat can still find what is left. Safe to run again.
    pub fn forget_connection(&self, connection: &str) -> Result<usize, String> {
        let mut n = 0;
        for table in [PSEUDONYMS, VAULT, EXTMAP] {
            for (id, f) in self.backend.list(table)? {
                if str_of(&f, "connection") == connection && self.backend.delete(table, &id)? {
                    n += 1;
                }
            }
        }
        self.secrets.forget_pseudonym_keys(connection)?;
        Ok(n)
    }

    /// Append one row to the data flow log. Returns its id.
    #[allow(clippy::too_many_arguments)]
    pub fn log(
        &self,
        conn: &Connection,
        direction: &str,
        model: &str,
        count: usize,
        sent: &[String],
        denied: &[String],
        mode: &str,
        hash: &str,
        outcome: &str,
        reason: &str,
    ) -> String {
        self.try_log(
            conn, direction, model, count, sent, denied, mode, hash, outcome, reason,
        )
        .unwrap_or_default()
    }

    /// As [`Egress::log`], and a row the store refused is an error: the gate does not let a flow
    /// go that it could not record.
    #[allow(clippy::too_many_arguments)]
    pub fn try_log(
        &self,
        conn: &Connection,
        direction: &str,
        model: &str,
        count: usize,
        sent: &[String],
        denied: &[String],
        mode: &str,
        hash: &str,
        outcome: &str,
        reason: &str,
    ) -> Result<String, String> {
        let (id, f) = Self::log_row(
            conn, direction, model, count, sent, denied, mode, hash, outcome, reason,
        );
        self.backend.insert(FLOW_LOG, &id, &f)?;
        Ok(id)
    }

    /// The id and the fields of a data flow log row.
    #[allow(clippy::too_many_arguments)]
    fn log_row(
        conn: &Connection,
        direction: &str,
        model: &str,
        count: usize,
        sent: &[String],
        denied: &[String],
        mode: &str,
        hash: &str,
        outcome: &str,
        reason: &str,
    ) -> (String, Fields) {
        let id = uuid::Uuid::now_v7().to_string();
        let f = json!({
            "connection": conn.id,
            "owner_user": conn.owner_user,
            "at": iso(now()),
            "direction": direction,
            "model": model,
            "record_count": count,
            "fields_sent": sent.join(","),
            "fields_denied": denied.join(","),
            "masking_mode": mode,
            "payload_hash": hash,
            "outcome": outcome,
            "reason": reason,
        });
        (id, f.as_object().expect("object").clone())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::catalog::provider;
    use crate::policy::{Ack, Policy};

    pub struct Fake;
    impl Catalog for Fake {
        fn fields(&self, model: &str) -> Option<Vec<FieldMeta>> {
            let f = |n: &str, class, personal| FieldMeta {
                name: n.into(),
                class,
                personal,
            };
            (model == "party").then(|| {
                vec![
                    f("name", DataClass::PublicDirectory, true),
                    f("email", DataClass::PublicDirectory, true),
                    f("notes", DataClass::Internal, true),
                    f("tax_id", DataClass::Financial, true),
                    f("salary", DataClass::ConfidentialHr, true),
                    f("health", DataClass::SpecialCategory, true),
                    f("api_key", DataClass::Secret, false),
                    f("kind", DataClass::None, false),
                ]
            })
        }
    }

    pub fn setup() -> (Arc<dyn Backend>, Egress, Connection) {
        let backend = obop_storage::open("memory:").unwrap();
        let egress = Egress::new(backend.clone(), SecretBox::for_tests(), Arc::new(Fake));
        let conn = Connection {
            id: "c1".into(),
            name: "Test".into(),
            provider: "webhook_out".into(),
            owner_scope: "instance".into(),
            owner_user: "root".into(),
            owner_groups: vec![],
            state: "connected".into(),
            config: Map::new(),
            credentials: String::new(),
            oauth_tokens: String::new(),
            allowlist: BTreeMap::from([(
                "party".to_string(),
                vec!["name".to_string(), "email".to_string(), "kind".to_string()],
            )]),
            policy: Policy::default(),
            rate_limit_per_hour: 5,
            volume_cap_per_day: 100,
            retention_days: 90,
        };
        let put = |model: &str, id: &str, v: J| {
            backend.insert(model, id, v.as_object().unwrap()).unwrap()
        };
        put(CONNECTION, "c1", json!({"state": "connected"}));
        put(
            CONSENT,
            "k1",
            json!({"connection": "c1", "state": "active", "classes": "public_directory,internal"}),
        );
        put(
            PROVIDER_POLICY,
            "pp",
            json!({"provider": "webhook_out", "dpa_recorded": true}),
        );
        (backend, egress, conn)
    }

    pub fn row(id: &str) -> Map<String, J> {
        json!({"id": id, "name": "Ada Lovelace", "email": "ada@example.test", "notes": "likes tea",
               "tax_id": "DE123", "salary": 1, "health": "x", "api_key": "sk_live_zzz", "kind": "customer"})
            .as_object().unwrap().clone()
    }

    fn webhook() -> Provider {
        provider("webhook_out").unwrap()
    }

    #[test]
    fn minimisation_pseudonyms_and_the_log_without_payload() {
        let (b, e, conn) = setup();
        let p = e
            .prepare(&conn, &webhook(), "party", vec![row("r1")])
            .unwrap();
        let r = &p.rows[0];
        // Only the allowlisted fields; personal ones as pseudonyms; the plain kind as is; no raw id.
        assert_eq!(
            r.keys().cloned().collect::<Vec<_>>(),
            ["email", "kind", "name", "ref"]
        );
        assert_eq!(r["kind"], "customer");
        for k in ["name", "email", "ref"] {
            assert!(r[k].as_str().unwrap().starts_with("psn_"), "{k}");
        }
        let text = serde_json::to_string(&p.rows).unwrap();
        for plain in ["Ada", "ada@example", "r1", "DE123", "sk_live"] {
            assert!(!text.contains(plain), "{plain} leaked: {text}");
        }
        // The log has the hash and the field names, never a value.
        let (_, log) = b.list(FLOW_LOG).unwrap().pop().unwrap();
        assert_eq!(log["outcome"], "released");
        assert_eq!(log["payload_hash"], p.report.payload_hash.as_str());
        assert_eq!(log["fields_sent"], "name,email,kind");
        assert_eq!(log["record_count"], 1);
        let dump = serde_json::to_string(&log).unwrap();
        assert!(!dump.contains("Ada") && !dump.contains("psn_"));
    }

    #[test]
    fn pseudonyms_are_stable_per_connection_and_differ_across_connections() {
        let sb = SecretBox::for_tests();
        let p = |sb: &SecretBox, c: &str, v: J| pseudonym(sb, c, &v).unwrap();
        let a = p(&sb, "c1", J::from("ada@example.test"));
        assert_eq!(a, p(&sb, "c1", J::from("ada@example.test")));
        assert_ne!(a, p(&sb, "c2", J::from("ada@example.test")));
        assert_ne!(a, p(&sb, "c1", J::from("alan@example.test")));
        assert!(!a.contains("ada") && a.len() == 4 + 32);
        // A different master key gives different pseudonyms: they are not derivable from the value.
        let other = SecretBox::from_secret("another-master-key-with-32-chars!!!").unwrap();
        assert_ne!(a, p(&other, "c1", J::from("ada@example.test")));
        // Numbers and booleans pseudonymise too.
        assert_ne!(p(&sb, "c1", J::from(5)), p(&sb, "c1", J::from("6")));
    }

    #[test]
    fn denied_classes_never_leave_even_when_allowlisted() {
        let (_, e, mut conn) = setup();
        conn.allowlist.insert(
            "party".into(),
            ["name", "tax_id", "salary", "health", "api_key"]
                .map(String::from)
                .to_vec(),
        );
        let p = e
            .prepare(&conn, &webhook(), "party", vec![row("r1")])
            .unwrap();
        assert_eq!(
            p.rows[0].keys().cloned().collect::<Vec<_>>(),
            ["name", "ref"]
        );
        assert_eq!(
            p.report.fields_denied,
            ["tax_id", "salary", "health", "api_key"]
        );
        // Only denied fields allowlisted: nothing to send at all.
        conn.allowlist.insert("party".into(), vec!["tax_id".into()]);
        assert_eq!(
            e.prepare(&conn, &webhook(), "party", vec![row("r1")])
                .unwrap_err(),
            Refusal::NothingAllowed
        );
    }

    #[test]
    fn high_risk_needs_an_override_an_acknowledgement_and_a_provider_that_allows_it() {
        let (b, e, mut conn) = setup();
        conn.allowlist.insert("party".into(), vec!["tax_id".into()]);
        conn.policy
            .classes
            .insert("financial".into(), Treatment::Pseudonymise);
        // Raised but not acknowledged: still denied.
        assert_eq!(
            e.prepare(&conn, &webhook(), "party", vec![row("r1")])
                .unwrap_err(),
            Refusal::NothingAllowed
        );
        conn.policy.acks.push(Ack {
            class: DataClass::Financial,
            by: "root".into(),
            at: "2026-10-17T00:00:00Z".into(),
            text: "Impact assessment 2026-10: invoicing export, retention 7 years".into(),
        });
        // The consent must cover the class as well.
        assert_eq!(
            e.prepare(&conn, &webhook(), "party", vec![row("r1")])
                .unwrap_err(),
            Refusal::ConsentClasses(vec!["financial".into()])
        );
        b.update(
            CONSENT,
            "k1",
            json!({"classes": "public_directory,internal,financial"})
                .as_object()
                .unwrap(),
        )
        .unwrap();
        let p = e
            .prepare(&conn, &webhook(), "party", vec![row("r1")])
            .unwrap();
        assert!(p.rows[0]["tax_id"].as_str().unwrap().starts_with("psn_"));
        // A provider whose maximum excludes the class never gets it.
        let slack = provider("slack_webhook").unwrap();
        let mut sconn = conn.clone();
        sconn.provider = "slack_webhook".into();
        b.insert(
            PROVIDER_POLICY,
            "pp2",
            json!({"provider": "slack_webhook", "dpa_recorded": true})
                .as_object()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            e.prepare(&sconn, &slack, "party", vec![row("r1")])
                .unwrap_err(),
            Refusal::NothingAllowed
        );
    }

    #[test]
    fn treatments_mask_redact_allow() {
        let (_, e, mut conn) = setup();
        conn.allowlist.insert(
            "party".into(),
            ["name", "email", "notes"].map(String::from).to_vec(),
        );
        conn.policy
            .fields
            .insert("party.name".into(), Treatment::Mask);
        conn.policy
            .fields
            .insert("party.email".into(), Treatment::Redact);
        conn.policy
            .fields
            .insert("party.notes".into(), Treatment::Allow);
        let p = e
            .prepare(&conn, &webhook(), "party", vec![row("r1"), row("r2")])
            .unwrap();
        let tok = p.rows[0]["name"].as_str().unwrap().to_string();
        assert!(tok.starts_with("TKN_") && !tok.contains("Ada"));
        assert_eq!(p.rows[0]["email"], REDACTED);
        assert_eq!(p.rows[0]["notes"], "likes tea");
        // The token is stable per value, and restorable only for this connection.
        assert_eq!(p.rows[1]["name"].as_str().unwrap(), tok);
        assert_eq!(e.restore("c1", &tok).as_deref(), Some("Ada Lovelace"));
        assert_eq!(e.restore("c2", &tok), None);
        assert_eq!(e.restore("c1", "TKN_nothere"), None);
        assert_eq!(
            e.restore("c1", p.rows[0]["ref"].as_str().unwrap()),
            None,
            "pseudonyms have no way back"
        );
    }

    #[test]
    fn a_token_the_vault_could_not_store_is_never_sent_and_the_flow_leaves_no_row() {
        use obop_storage::testing::{Flaky, Mode};
        // The vault write is refused: the token would be sent and could not be restored when a
        // response cites it (and the value is gone). The flow stops, busy, and writes no row.
        let (b, _, mut conn) = setup();
        conn.policy
            .fields
            .insert("party.name".into(), Treatment::Mask);
        let flaky = Flaky::new(b.clone());
        let e = Egress::new(flaky.clone(), SecretBox::for_tests(), Arc::new(Fake));
        flaky.refuse_model(VAULT, Mode::All);
        let r = e.prepare(&conn, &webhook(), "party", vec![row("r1")]);
        flaky.allow_models();
        assert_eq!(r.unwrap_err(), Refusal::StoreBusy);
        assert!(
            b.list(FLOW_LOG).unwrap().is_empty(),
            "no row for a flow that did not happen, and none that says it was refused by policy"
        );
        // With the vault back, the same flow goes and the token restores.
        let p = e
            .prepare(&conn, &webhook(), "party", vec![row("r1")])
            .unwrap();
        let tok = p.rows[0]["name"].as_str().unwrap();
        assert_eq!(e.restore("c1", tok).as_deref(), Some("Ada Lovelace"));
    }

    #[test]
    fn a_mask_refused_between_its_two_writes_never_sends_a_token_that_cannot_be_restored() {
        use obop_storage::testing::Flaky;
        // The vault holds an entry by value and one by token; `restore` reads only the second. A
        // refusal between the two writes left the first alone, and the flow that was repeated
        // found it, sent its token and never wrote the second. One refused call at every
        // position of the flow in turn, then the same flow again on a healthy store.
        let mut stopped = 0;
        for skip in 0..40 {
            let (b, _, mut conn) = setup();
            conn.policy
                .fields
                .insert("party.name".into(), Treatment::Mask);
            let flaky = Flaky::new(b.clone());
            let e = Egress::new(flaky.clone(), SecretBox::for_tests(), Arc::new(Fake));
            flaky.refuse_call_after(skip);
            let first = e.prepare(&conn, &webhook(), "party", vec![row("r1")]);
            flaky.allow_models();
            stopped += usize::from(first.is_err());
            let p = e
                .prepare(&conn, &webhook(), "party", vec![row("r1")])
                .unwrap();
            let tok = p.rows[0]["name"].as_str().unwrap();
            assert_eq!(
                e.restore("c1", tok).as_deref(),
                Some("Ada Lovelace"),
                "skip {skip}: a token was sent that cannot be restored"
            );
        }
        assert!(stopped >= 5, "the sweep reached the flow: {stopped}");
        // An entry by value that an earlier release left without its entry by token is mended.
        let (b, e, mut conn) = setup();
        conn.policy
            .fields
            .insert("party.name".into(), Treatment::Mask);
        let p = e
            .prepare(&conn, &webhook(), "party", vec![row("r1")])
            .unwrap();
        let tok = p.rows[0]["name"].as_str().unwrap().to_string();
        b.delete(VAULT, &format!("tok:{tok}")).unwrap();
        assert_eq!(e.restore("c1", &tok), None);
        let again = e
            .prepare(&conn, &webhook(), "party", vec![row("r2")])
            .unwrap();
        assert_eq!(again.rows[0]["name"].as_str().unwrap(), tok);
        assert_eq!(e.restore("c1", &tok).as_deref(), Some("Ada Lovelace"));
    }

    #[test]
    fn a_refused_read_of_the_agreement_is_busy_and_never_logged_as_no_agreement() {
        use obop_storage::testing::{Flaky, Mode};
        let (b, _, conn) = setup();
        let flaky = Flaky::new(b.clone());
        let e = Egress::new(flaky.clone(), SecretBox::for_tests(), Arc::new(Fake));
        flaky.refuse_model(PROVIDER_POLICY, Mode::Reads);
        let r = e.prepare(&conn, &webhook(), "party", vec![row("r1")]);
        flaky.allow_models();
        assert_eq!(r.unwrap_err(), Refusal::StoreBusy);
        assert!(
            b.list(FLOW_LOG).unwrap().is_empty(),
            "a busy store is not a refusal of the flow, so nothing is logged as one"
        );
        assert!(
            e.prepare(&conn, &webhook(), "party", vec![row("r1")])
                .is_ok()
        );
    }

    #[test]
    fn an_erasure_the_store_refused_is_an_error_and_a_repeat_finishes_it() {
        use obop_storage::testing::{Flaky, Mode};
        let (b, _, mut conn) = setup();
        conn.policy
            .fields
            .insert("party.name".into(), Treatment::Mask);
        let flaky = Flaky::new(b.clone());
        let e = Egress::new(flaky.clone(), SecretBox::for_tests(), Arc::new(Fake));
        let p = e
            .prepare(&conn, &webhook(), "party", vec![row("r1")])
            .unwrap();
        let tok = p.rows[0]["name"].as_str().unwrap().to_string();
        // The vault cannot be listed: the erasure is not "done, nothing found".
        flaky.refuse_model(VAULT, Mode::Reads);
        let first = e.erase_subject("party:r1");
        flaky.allow_models();
        assert!(
            matches!(&first, Err(m) if m.starts_with("storage_busy")),
            "{first:?}"
        );
        assert_eq!(e.restore("c1", &tok).as_deref(), Some("Ada Lovelace"));
        // The repeat finishes it.
        let done = e.erase_subject("party:r1").unwrap();
        assert!(done["c1"] >= 1, "{done:?}");
        assert_eq!(e.restore("c1", &tok), None);
        // A refused delete is an error too, and so is a refused forget of a revoked connection.
        let p = e
            .prepare(&conn, &webhook(), "party", vec![row("r9")])
            .unwrap();
        flaky.refuse_model(VAULT, Mode::All);
        assert!(e.forget_connection("c1").is_err());
        flaky.allow_models();
        assert!(
            e.restore("c1", p.rows[0]["name"].as_str().unwrap())
                .is_some()
        );
        assert!(e.forget_connection("c1").unwrap() >= 1);
    }

    #[test]
    fn a_pseudonym_map_the_server_failed_to_write_is_not_taken_for_one_that_was_there() {
        use obop_storage::testing::Flaky;
        let (b, _, conn) = setup();
        let flaky = Flaky::new(b.clone());
        let e = Egress::new(flaky.clone(), SecretBox::for_tests(), Arc::new(Fake));
        // Any insert error that was not a duplicate was read as "the same record sent before", and
        // the record left with a reference that erasure and revocation could not find.
        flaky.fail_inserts_into(PSEUDONYMS, "could not extend file: No space left on device");
        let sent = e.prepare(&conn, &webhook(), "party", vec![row("r1")]);
        assert!(sent.is_err(), "nothing leaves without its map");
        assert!(b.list(PSEUDONYMS).unwrap().is_empty());
        flaky.allow_models();
        // The same record sent twice is a duplicate, and that is as it should be.
        e.prepare(&conn, &webhook(), "party", vec![row("r1")])
            .unwrap();
        e.prepare(&conn, &webhook(), "party", vec![row("r1")])
            .unwrap();
        assert_eq!(b.list(PSEUDONYMS).unwrap().len(), 1);
    }

    #[test]
    fn refusals_each_have_a_reason_and_are_logged() {
        let (b, e, conn) = setup();
        let w = webhook();
        let go = |c: &Connection| e.prepare(c, &w, "party", vec![row("r1")]);
        let mut draft = conn.clone();
        draft.state = "draft".into();
        assert_eq!(
            go(&draft).unwrap_err(),
            Refusal::NotConnected("draft".into())
        );
        let mut empty = conn.clone();
        empty.allowlist.clear();
        assert_eq!(go(&empty).unwrap_err(), Refusal::EmptyAllowlist);
        assert_eq!(
            e.prepare(&conn, &w, "ghost", vec![]).unwrap_err(),
            Refusal::EmptyAllowlist
        );
        let mut other = conn.clone();
        other.allowlist.insert("ghost".into(), vec!["x".into()]);
        assert_eq!(
            e.prepare(&other, &w, "ghost", vec![]).unwrap_err(),
            Refusal::UnknownModel("ghost".into())
        );
        assert_eq!(
            e.prepare(&conn, &provider("webhook_in").unwrap(), "party", vec![])
                .unwrap_err(),
            Refusal::DirectionNotAllowed
        );
        // No agreement recorded for personal data.
        b.update(
            PROVIDER_POLICY,
            "pp",
            json!({"dpa_recorded": false}).as_object().unwrap(),
        )
        .unwrap();
        assert!(matches!(go(&conn).unwrap_err(), Refusal::NoDpa(_)));
        b.update(
            PROVIDER_POLICY,
            "pp",
            json!({"dpa_recorded": true}).as_object().unwrap(),
        )
        .unwrap();
        // The master switch and the EU-only switch.
        b.insert(
            EGRESS_POLICY,
            INSTANCE,
            json!({"flows_enabled": false}).as_object().unwrap(),
        )
        .unwrap();
        assert_eq!(go(&conn).unwrap_err(), Refusal::FlowsDisabled);
        b.update(
            EGRESS_POLICY,
            INSTANCE,
            json!({"flows_enabled": true, "eu_only": true})
                .as_object()
                .unwrap(),
        )
        .unwrap();
        assert!(matches!(go(&conn).unwrap_err(), Refusal::NotEuAdequate(_)));
        b.update(
            EGRESS_POLICY,
            INSTANCE,
            json!({"eu_only": false}).as_object().unwrap(),
        )
        .unwrap();
        assert!(go(&conn).is_ok());
        let log = b.list(FLOW_LOG).unwrap();
        assert!(
            log.iter()
                .filter(|(_, f)| f["outcome"] == "refused")
                .count()
                >= 6
        );
        assert!(
            log.iter()
                .any(|(_, f)| f["reason"].as_str().unwrap().contains("not connected"))
        );
    }

    #[test]
    fn consent_withdrawal_stops_flows() {
        let (b, e, conn) = setup();
        assert!(
            e.prepare(&conn, &webhook(), "party", vec![row("r1")])
                .is_ok()
        );
        b.update(
            CONSENT,
            "k1",
            json!({"state": "withdrawn"}).as_object().unwrap(),
        )
        .unwrap();
        assert_eq!(
            e.prepare(&conn, &webhook(), "party", vec![row("r1")])
                .unwrap_err(),
            Refusal::ConsentWithdrawn
        );
        b.delete(CONSENT, "k1").unwrap();
        assert_eq!(
            e.prepare(&conn, &webhook(), "party", vec![row("r1")])
                .unwrap_err(),
            Refusal::NoConsent
        );
    }

    #[test]
    fn rate_and_volume_caps() {
        let (_, e, mut conn) = setup();
        let w = webhook();
        conn.rate_limit_per_hour = 2;
        assert!(e.prepare(&conn, &w, "party", vec![row("a")]).is_ok());
        assert!(e.prepare(&conn, &w, "party", vec![row("b")]).is_ok());
        assert_eq!(
            e.prepare(&conn, &w, "party", vec![row("c")]).unwrap_err(),
            Refusal::RateLimited(2)
        );
        conn.rate_limit_per_hour = 100;
        conn.volume_cap_per_day = 4;
        assert_eq!(
            e.prepare(&conn, &w, "party", vec![row("d"), row("e"), row("f")])
                .unwrap_err(),
            Refusal::VolumeCap(4)
        );
        assert!(
            e.prepare(&conn, &w, "party", vec![row("d"), row("e")])
                .is_ok()
        );
        // Refusals do not count against the caps.
        let (flows, records) = e.usage("c1", now());
        assert_eq!((flows, records), (3, 4));
    }

    #[test]
    fn preview_stores_nothing_and_matches_the_real_treatment() {
        let (b, e, conn) = setup();
        let (plan, rows) = e.preview(&conn, &webhook(), "party", &[row("r1")]).unwrap();
        assert!(plan.sends_personal_data() && plan.denied().count() == 0);
        assert!(b.list(FLOW_LOG).unwrap().is_empty() && b.list(PSEUDONYMS).unwrap().is_empty());
        // Same pseudonyms as the real flow, because they use the same key.
        let real = e
            .prepare(&conn, &webhook(), "party", vec![row("r1")])
            .unwrap();
        assert_eq!(rows[0], real.rows[0]);
    }

    #[test]
    fn erasure_and_revocation_drop_pseudonym_maps_and_tokens() {
        let (b, e, mut conn) = setup();
        conn.policy
            .fields
            .insert("party.name".into(), Treatment::Mask);
        let p = e
            .prepare(&conn, &webhook(), "party", vec![row("r1"), row("r2")])
            .unwrap();
        let tok = p.rows[0]["name"].as_str().unwrap().to_string();
        assert!(!b.list(PSEUDONYMS).unwrap().is_empty() && !b.list(VAULT).unwrap().is_empty());
        let removed = e.erase_subject("party:r1").unwrap();
        assert!(removed["c1"] >= 3, "{removed:?}");
        assert_eq!(e.restore("c1", &tok), None, "the token no longer resolves");
        // r2's map is untouched, and the erasure itself is in the log.
        assert!(
            b.list(PSEUDONYMS)
                .unwrap()
                .iter()
                .any(|(_, f)| f["record"] == "party:r2")
        );
        assert!(
            b.list(FLOW_LOG)
                .unwrap()
                .iter()
                .any(|(_, f)| f["outcome"] == "erased")
        );
        let n = e.forget_connection("c1").unwrap();
        assert!(n >= 1 && b.list(PSEUDONYMS).unwrap().is_empty());
    }
}
