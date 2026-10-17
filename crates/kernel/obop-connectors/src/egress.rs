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
use obop_storage::{Backend, Fields};
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
pub fn pseudonym(secrets: &SecretBox, connection: &str, value: &J) -> String {
    let text = match value {
        J::String(s) => s.clone(),
        other => other.to_string(),
    };
    let mut m = HmacSha256::new_from_slice(&secrets.pseudonym_key(&format!("pseudonym:{connection}")))
        .expect("HMAC takes any key length");
    m.update(b"v1|");
    m.update(text.as_bytes());
    format!("psn_{}", hex(&m.finalize().into_bytes()[..16]))
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
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FlowsDisabled => write!(f, "flows are switched off for this instance"),
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
        let inst = InstancePolicy::load(&*self.backend);
        if !inst.flows_enabled {
            return Err(Refusal::FlowsDisabled);
        }
        if !provider.can_send() {
            return Err(Refusal::DirectionNotAllowed);
        }
        if !conn.is_connected() {
            return Err(Refusal::NotConnected(conn.state.clone()));
        }
        let consents = consents_of(&*self.backend, &conn.id);
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
            if inst.require_dpa
                && !provider_policy(&*self.backend, provider.id).is_some_and(|p| p.dpa_recorded)
            {
                return Err(Refusal::NoDpa(provider.name.into()));
            }
            if inst.eu_only && provider.eu_adequate != "yes" {
                return Err(Refusal::NotEuAdequate(provider.name.into()));
            }
        }
        Ok(plan)
    }

    /// Flows in the last hour and records in the last day that were released for `connection`.
    pub fn usage(&self, connection: &str, at: u64) -> (i64, i64) {
        let (mut flows, mut records) = (0, 0);
        for (_, f) in self.backend.list(FLOW_LOG).unwrap_or_default() {
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
        (flows, records)
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
        if let Err(r) = &outcome {
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
        let (flows, records) = self.usage(&conn.id, at);
        if flows >= conn.rate_limit_per_hour {
            return Err(Refusal::RateLimited(conn.rate_limit_per_hour));
        }
        if records + rows.len() as i64 > conn.volume_cap_per_day {
            return Err(Refusal::VolumeCap(conn.volume_cap_per_day));
        }
        let out: Vec<Map<String, J>> = rows
            .iter()
            .map(|r| self.apply(conn, &plan, r, Mode::Live))
            .collect();
        let hash = sha256_hex(serde_json::to_string(&out).unwrap_or_default().as_bytes());
        let sent: Vec<String> = plan.sent().map(|f| f.name.clone()).collect();
        let denied: Vec<String> = plan
            .denied()
            .map(|f| f.name.clone())
            .chain(plan.unknown.iter().cloned())
            .collect();
        let mode = plan.mode_summary();
        let flow_id = self.log(
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
        );
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
            .collect();
        Ok((plan, out))
    }

    fn apply(
        &self,
        conn: &Connection,
        plan: &Plan,
        row: &Map<String, J>,
        mode: Mode,
    ) -> Map<String, J> {
        let id = str_of(row, "id");
        let record = format!("{}:{id}", plan.model);
        let reference = pseudonym(&self.secrets, &conn.id, &J::from(record.clone()));
        if mode == Mode::Live {
            let mut f = Fields::new();
            f.insert("connection".into(), conn.id.clone().into());
            f.insert("record".into(), record.clone().into());
            let _ = self
                .backend
                .insert(PSEUDONYMS, &format!("{}:{reference}", conn.id), &f);
        }
        let mut out = Map::new();
        out.insert("ref".into(), reference.into());
        for pf in plan.sent() {
            let Some(v) = row.get(&pf.name) else { continue };
            let treated = match (pf.decision.treatment, v) {
                (_, J::Null) => J::Null,
                (Treatment::Allow, v) => v.clone(),
                (Treatment::Redact, _) => J::from(REDACTED),
                (Treatment::Pseudonymise, v) => J::from(pseudonym(&self.secrets, &conn.id, v)),
                (Treatment::Mask, v) => J::from(self.mask(conn, &record, &pf.name, v, mode)),
                (Treatment::Deny, _) => continue,
            };
            out.insert(pf.name.clone(), treated);
        }
        out
    }

    /// A reversible stand-in for `v`, stable per connection and value. The value is kept sealed
    /// in the instance so an inbound response that cites the token can be restored.
    fn mask(&self, conn: &Connection, record: &str, field: &str, v: &J, mode: Mode) -> String {
        let text = match v {
            J::String(s) => s.clone(),
            other => other.to_string(),
        };
        let index = pseudonym(
            &self.secrets,
            &format!("{}:vault", conn.id),
            &J::from(text.clone()),
        );
        if mode == Mode::Preview {
            return format!("TKN_{}", &index[4..16]);
        }
        let key = format!("{}:{index}", conn.id);
        if let Ok(Some(f)) = self.backend.get(VAULT, &key) {
            return str_of(&f, "token").to_string();
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
        // Two keys, one entry: by value (to stay stable) and by token (to restore).
        if self.backend.insert(VAULT, &key, &f).is_ok() {
            let _ = self.backend.insert(VAULT, &format!("tok:{token}"), &f);
            token
        } else {
            self.backend
                .get(VAULT, &key)
                .ok()
                .flatten()
                .map(|f| str_of(&f, "token").to_string())
                .unwrap_or(token)
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

    /// Drop everything the instance holds that links a pseudonym or a token to `record`
    /// (`model:id`): the erasure hook. Returns how many entries were removed per connection.
    pub fn erase_subject(&self, record: &str) -> BTreeMap<String, usize> {
        let mut per: BTreeMap<String, usize> = BTreeMap::new();
        for table in [PSEUDONYMS, VAULT, EXTMAP] {
            for (id, f) in self.backend.list(table).unwrap_or_default() {
                if str_of(&f, "record") == record
                    && self.backend.delete(table, &id).unwrap_or(false)
                {
                    *per.entry(str_of(&f, "connection").to_string()).or_default() += 1;
                }
            }
        }
        for (conn, n) in &per {
            if let Ok(c) = Connection::load(&*self.backend, conn) {
                self.log(
                    &c,
                    "outbound",
                    record.split(':').next().unwrap_or(""),
                    0,
                    &[],
                    &[],
                    "",
                    "",
                    "erased",
                    &format!("subject erased: {n} pseudonym or token entries removed"),
                );
            }
        }
        per
    }

    /// Drop everything a connection holds that can be linked back (revocation): its tokens and
    /// pseudonym maps. The pseudonyms already sent stay meaningless without the key.
    pub fn forget_connection(&self, connection: &str) -> usize {
        let mut n = 0;
        for table in [PSEUDONYMS, VAULT, EXTMAP] {
            for (id, f) in self.backend.list(table).unwrap_or_default() {
                if str_of(&f, "connection") == connection
                    && self.backend.delete(table, &id).unwrap_or(false)
                {
                    n += 1;
                }
            }
        }
        self.secrets.forget_pseudonym_keys(connection);
        n
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
        let _ = self
            .backend
            .insert(FLOW_LOG, &id, f.as_object().expect("object"));
        id
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
        let a = pseudonym(&sb, "c1", &J::from("ada@example.test"));
        assert_eq!(a, pseudonym(&sb, "c1", &J::from("ada@example.test")));
        assert_ne!(a, pseudonym(&sb, "c2", &J::from("ada@example.test")));
        assert_ne!(a, pseudonym(&sb, "c1", &J::from("alan@example.test")));
        assert!(!a.contains("ada") && a.len() == 4 + 32);
        // A different master key gives different pseudonyms: they are not derivable from the value.
        let other = SecretBox::from_secret("another-master-key-with-32-chars!!!").unwrap();
        assert_ne!(a, pseudonym(&other, "c1", &J::from("ada@example.test")));
        // Numbers and booleans pseudonymise too.
        assert_ne!(
            pseudonym(&sb, "c1", &J::from(5)),
            pseudonym(&sb, "c1", &J::from("6"))
        );
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
        let removed = e.erase_subject("party:r1");
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
        let n = e.forget_connection("c1");
        assert!(n >= 1 && b.list(PSEUDONYMS).unwrap().is_empty());
    }
}
