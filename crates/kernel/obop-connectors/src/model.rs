//! Typed views of the stored records (the registry models of the `connectors` module) and the
//! constants that name them. The records are JSON documents in the shared store; these structs
//! parse them once so the engines never poke at raw fields.

use obop_storage::{Backend, Fields};
use obop_types::DataClass;
use serde_json::Value as J;
use std::collections::BTreeMap;

use crate::policy::Policy;
use crate::util::{bool_of, int_of, obj_of, str_of};

pub const CONNECTION: &str = "connection";
pub const CONSENT: &str = "consent";
pub const PROVIDER_POLICY: &str = "provider_policy";
pub const EGRESS_POLICY: &str = "egress_policy";
pub const SYNC_RUN: &str = "sync_run";
pub const FLOW_LOG: &str = "data_flow_log";
/// Id of the single `egress_policy` record.
pub const INSTANCE: &str = "instance";

// Tables the engines keep for themselves (not registry models: no generated command reaches them).
pub const PSEUDONYMS: &str = "connector_pseudonym";
pub const VAULT: &str = "connector_vault";
pub const EXTMAP: &str = "connector_extmap";
pub const ORIGIN: &str = "connector_origin";

pub const DEFAULT_RATE_PER_HOUR: i64 = 120;
pub const DEFAULT_VOLUME_PER_DAY: i64 = 10_000;
pub const DEFAULT_RETENTION_DAYS: i64 = 90;
pub const MAX_ROWS_PER_FLOW: usize = 1000;

#[derive(Debug, Clone)]
pub struct Connection {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub owner_scope: String,
    pub owner_user: String,
    /// The groups the owner held when they last changed the connection.
    pub owner_groups: Vec<String>,
    pub state: String,
    pub config: serde_json::Map<String, J>,
    pub credentials: String,
    pub oauth_tokens: String,
    /// Model -> fields that may be sent or accepted.
    pub allowlist: BTreeMap<String, Vec<String>>,
    pub policy: Policy,
    pub rate_limit_per_hour: i64,
    pub volume_cap_per_day: i64,
    pub retention_days: i64,
}

fn csv(s: &str) -> Vec<String> {
    s.split(',')
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(String::from)
        .collect()
}

impl Connection {
    pub fn from_fields(id: &str, f: &Fields) -> Result<Self, String> {
        let mut allowlist = BTreeMap::new();
        for (model, v) in obj_of(f, "field_allowlist") {
            let fields = v
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            allowlist.insert(model, fields);
        }
        Ok(Self {
            id: id.into(),
            name: str_of(f, "name").into(),
            provider: str_of(f, "provider").into(),
            owner_scope: str_of(f, "owner_scope").into(),
            owner_user: str_of(f, "owner_user").into(),
            owner_groups: csv(str_of(f, "owner_groups")),
            state: str_of(f, "state").into(),
            config: obj_of(f, "config"),
            credentials: str_of(f, "credentials").into(),
            oauth_tokens: str_of(f, "oauth_tokens").into(),
            allowlist,
            policy: Policy::parse(str_of(f, "policy"))?,
            rate_limit_per_hour: int_of(f, "rate_limit_per_hour", DEFAULT_RATE_PER_HOUR),
            volume_cap_per_day: int_of(f, "volume_cap_per_day", DEFAULT_VOLUME_PER_DAY),
            retention_days: int_of(f, "retention_days", DEFAULT_RETENTION_DAYS),
        })
    }

    pub fn load(backend: &dyn Backend, id: &str) -> Result<Self, String> {
        let f = backend
            .get(CONNECTION, id)?
            .ok_or_else(|| format!("no connection {id}"))?;
        Self::from_fields(id, &f)
    }

    pub fn is_connected(&self) -> bool {
        self.state == "connected"
    }

    /// The text of a config entry.
    pub fn cfg(&self, key: &str) -> &str {
        self.config.get(key).and_then(J::as_str).unwrap_or("")
    }

    pub fn cfg_int(&self, key: &str) -> i64 {
        match self.config.get(key) {
            Some(J::Number(n)) => n.as_i64().unwrap_or(0),
            Some(J::String(s)) => s.trim().parse().unwrap_or(0),
            _ => 0,
        }
    }

    /// Hosts named in the connection's own config (`allowed_hosts`, comma separated).
    pub fn own_hosts(&self) -> Vec<String> {
        csv(self.cfg("allowed_hosts"))
    }
}

#[derive(Debug, Clone)]
pub struct Consent {
    pub id: String,
    pub connection: String,
    pub state: String,
    pub legal_basis: String,
    pub classes: Vec<DataClass>,
    pub recorded_at: String,
}

impl Consent {
    pub fn from_fields(id: &str, f: &Fields) -> Self {
        Self {
            id: id.into(),
            connection: str_of(f, "connection").into(),
            state: str_of(f, "state").into(),
            legal_basis: str_of(f, "legal_basis").into(),
            classes: csv(str_of(f, "classes"))
                .iter()
                .filter_map(|c| DataClass::parse(c))
                .collect(),
            recorded_at: str_of(f, "recorded_at").into(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.state == "active"
    }
}

/// The consent records of a connection, newest first by id (ids are time-ordered UUIDv7).
pub fn consents_of(backend: &dyn Backend, connection: &str) -> Vec<Consent> {
    try_consents_of(backend, connection).unwrap_or_default()
}

/// As [`consents_of`], but a storage error is an error, not "no consent": a caller that must tell
/// "withdrawn" from "could not look" (the delivery worker) uses this.
pub fn try_consents_of(backend: &dyn Backend, connection: &str) -> Result<Vec<Consent>, String> {
    let mut v: Vec<Consent> = backend
        .list(CONSENT)?
        .iter()
        .filter(|(_, f)| str_of(f, "connection") == connection)
        .map(|(id, f)| Consent::from_fields(id, f))
        .collect();
    v.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(v)
}

/// The instance-wide switches (`egress_policy`, a single record).
#[derive(Debug, Clone)]
pub struct InstancePolicy {
    /// Master switch: when off nothing leaves and nothing is accepted.
    pub flows_enabled: bool,
    /// Only providers that are adequate or EU-based may receive personal data.
    pub eu_only: bool,
    /// Personal data (even pseudonymised) only goes to providers whose agreement is recorded.
    pub require_dpa: bool,
    /// Operator allow-list of hosts; empty means no extra restriction.
    pub hosts: Vec<String>,
    pub default_retention_days: i64,
}

impl Default for InstancePolicy {
    fn default() -> Self {
        Self {
            flows_enabled: true,
            eu_only: false,
            require_dpa: true,
            hosts: vec![],
            default_retention_days: DEFAULT_RETENTION_DAYS,
        }
    }
}

impl InstancePolicy {
    /// The policy, or the reason it could not be read. A policy that was never saved is the
    /// default (flows allowed); a storage error is not that.
    pub fn try_load(backend: &dyn Backend) -> Result<Self, String> {
        Ok(match backend.get(EGRESS_POLICY, INSTANCE)? {
            Some(f) => Self {
                flows_enabled: f.get("flows_enabled").and_then(J::as_bool).unwrap_or(true),
                eu_only: bool_of(&f, "eu_only"),
                require_dpa: f.get("require_dpa").and_then(J::as_bool).unwrap_or(true),
                hosts: csv(str_of(&f, "host_allowlist")),
                default_retention_days: int_of(
                    &f,
                    "default_retention_days",
                    DEFAULT_RETENTION_DAYS,
                ),
            },
            None => Self::default(),
        })
    }

    /// The policy that holds when it could not be read: nothing leaves and nothing is accepted
    /// until it can be (D74: a busy database must not switch the operator's master switch off
    /// "off", nor widen who may receive data).
    pub fn closed() -> Self {
        Self {
            flows_enabled: false,
            eu_only: true,
            ..Self::default()
        }
    }

    /// The policy, failing closed ([`InstancePolicy::closed`]) when the store cannot be read.
    pub fn load(backend: &dyn Backend) -> Self {
        Self::try_load(backend).unwrap_or_else(|_| Self::closed())
    }
}

/// What the admin recorded about one provider.
#[derive(Debug, Clone, Default)]
pub struct ProviderPolicy {
    pub id: String,
    pub enabled: bool,
    pub users_may_use: bool,
    pub dpa_recorded: bool,
    pub legal_basis: String,
    pub legal_basis_text: String,
    pub allowed_group: String,
    pub default_policy: String,
    pub oauth_client_id: String,
    pub oauth_client_secret: String,
}

pub fn provider_policy(backend: &dyn Backend, provider: &str) -> Option<ProviderPolicy> {
    backend
        .list(PROVIDER_POLICY)
        .ok()?
        .into_iter()
        .find(|(_, f)| str_of(f, "provider") == provider)
        .map(|(id, f)| ProviderPolicy {
            id,
            enabled: bool_of(&f, "enabled"),
            users_may_use: bool_of(&f, "users_may_use"),
            dpa_recorded: bool_of(&f, "dpa_recorded"),
            legal_basis: str_of(&f, "legal_basis").into(),
            legal_basis_text: str_of(&f, "legal_basis_text").into(),
            allowed_group: str_of(&f, "allowed_group").into(),
            default_policy: str_of(&f, "default_policy").into(),
            oauth_client_id: str_of(&f, "oauth_client_id").into(),
            oauth_client_secret: str_of(&f, "oauth_client_secret").into(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fields(v: J) -> Fields {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn a_policy_that_cannot_be_read_closes_the_gates_instead_of_opening_them() {
        use obop_storage::testing::{Flaky, Mode};
        // D74: a busy store is not "the operator never saved a policy".
        let (flaky, db) = Flaky::memory();
        let open = InstancePolicy::load(&*db);
        assert!(
            open.flows_enabled && !open.eu_only,
            "never saved: the default"
        );
        flaky.set(Mode::Reads);
        let closed = InstancePolicy::load(&*db);
        assert!(
            !closed.flows_enabled,
            "nothing leaves while it cannot be read"
        );
        assert!(closed.eu_only);
        assert!(InstancePolicy::try_load(&*db).is_err());
        assert!(try_consents_of(&*db, "c").is_err(), "not 'no consent'");
        assert!(consents_of(&*db, "c").is_empty());
    }

    #[test]
    fn connection_parses_with_defaults() {
        let c = Connection::from_fields(
            "c1",
            &fields(json!({
                "name": "n", "provider": "webhook_out", "state": "connected",
                "owner_groups": "base.user, crm.user",
                "field_allowlist": "{\"party\":[\"name\",\"email\"]}",
                "config": "{\"url\":\"https://x.example\",\"interval_minutes\":\"15\"}",
            })),
        )
        .unwrap();
        assert!(c.is_connected());
        assert_eq!(c.owner_groups, ["base.user", "crm.user"]);
        assert_eq!(c.allowlist["party"], ["name", "email"]);
        assert_eq!(c.cfg("url"), "https://x.example");
        assert_eq!(c.cfg_int("interval_minutes"), 15);
        assert_eq!(c.rate_limit_per_hour, DEFAULT_RATE_PER_HOUR);
        assert_eq!(c.retention_days, DEFAULT_RETENTION_DAYS);
        // Broken policy text is an error, not silently the default.
        assert!(Connection::from_fields("c", &fields(json!({"policy": "{\"x\":1}"}))).is_err());
    }

    #[test]
    fn consents_come_newest_first_and_instance_policy_defaults_are_strict() {
        let b = obop_storage::open("memory:").unwrap();
        for (id, state) in [("a", "withdrawn"), ("b", "active"), ("c", "active")] {
            let f = fields(json!({"connection": "c1", "state": state, "classes": "none,internal"}));
            b.insert(CONSENT, id, &f).unwrap();
        }
        b.insert(CONSENT, "z", &fields(json!({"connection": "other"})))
            .unwrap();
        let v = consents_of(&*b, "c1");
        assert_eq!(
            v.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["c", "b", "a"]
        );
        assert!(v[0].is_active() && !v[2].is_active());
        assert_eq!(v[0].classes, [DataClass::None, DataClass::Internal]);
        let p = InstancePolicy::load(&*b);
        assert!(p.flows_enabled && p.require_dpa && !p.eu_only);
    }
}
