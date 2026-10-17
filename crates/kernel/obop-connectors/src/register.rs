//! The processing register: a read model over connections, providers, consents and policies that
//! supports the record of processing activities of GDPR Article 30. It is generated, never
//! edited, so it cannot drift from what is actually configured. It is a starting point for the
//! controller's own record, not a substitute for it.

use obop_storage::Backend;
use serde_json::{Map, Value as J};

use crate::catalog::Provider;
use crate::egress::Egress;
use crate::model::*;
use crate::util::{csv_row, str_of};

/// Column names, in the order of the CSV export; they are also the fields of the model.
pub const COLUMNS: [&str; 17] = [
    "connection",
    "name",
    "state",
    "owner_scope",
    "owner_user",
    "provider",
    "recipient",
    "recipient_country",
    "purpose",
    "legal_basis",
    "consent_recorded_at",
    "data_subjects",
    "personal_data_categories",
    "transfer_basis",
    "agreement_recorded",
    "retention_days",
    "safeguards",
];

fn safeguards(c: &Connection, personal_in_clear: bool) -> String {
    let mut s = vec![
        "credentials encrypted at rest",
        "access limited to the owner's groups",
        "every flow logged without payload",
        "withdrawal revokes access and purges credentials",
    ];
    s.push(if personal_in_clear {
        "WARNING: some personal fields are sent in clear by policy"
    } else {
        "personal fields pseudonymised or withheld by default"
    });
    if c.policy.acks.is_empty() {
        s.push("no high-risk class released");
    } else {
        s.push("high-risk class released with a recorded impact-assessment acknowledgement");
    }
    s.join("; ")
}

/// One row per connection, newest first by id.
pub fn rows(
    backend: &dyn Backend,
    egress: &Egress,
    provider: &dyn Fn(&str) -> Option<Provider>,
) -> Vec<Map<String, J>> {
    let mut out = Vec::new();
    let mut conns = backend.list(CONNECTION).unwrap_or_default();
    conns.sort_by(|a, b| b.0.cmp(&a.0));
    for (id, f) in conns {
        let Ok(c) = Connection::from_fields(&id, &f) else {
            continue;
        };
        let prov = provider(&c.provider);
        let consents = consents_of(backend, &id);
        let consent = consents.iter().find(|k| k.is_active()).or(consents.first());
        let consent_rec = backend
            .get(CONSENT, consent.map_or("", |k| k.id.as_str()))
            .ok()
            .flatten();
        let pp = provider_policy(backend, &c.provider);
        let mut categories = Vec::new();
        let mut clear = false;
        if let Some(p) = &prov {
            for model in c.allowlist.keys() {
                if let Ok(plan) = egress.plan(&c, p, model) {
                    let fields: Vec<String> = plan
                        .sent()
                        .map(|f| {
                            if f.personal && f.decision.treatment == crate::policy::Treatment::Allow
                            {
                                clear = true;
                            }
                            format!(
                                "{} ({}, {})",
                                f.name,
                                f.class,
                                f.decision.treatment.as_str()
                            )
                        })
                        .collect();
                    if !fields.is_empty() {
                        categories.push(format!("{model}: {}", fields.join(", ")));
                    }
                }
            }
        }
        let subjects = match str_of(&f, "owner_scope") {
            "user" => {
                "the user who connected their own account, and people in the records they send"
            }
            _ => {
                "people in the records of the selected models (customers, contacts, staff as modelled)"
            }
        };
        let transfer = match prov.as_ref().map(|p| p.eu_adequate) {
            Some("yes") => "EU or adequate country".to_string(),
            Some("no") => "third country: a transfer mechanism is required".to_string(),
            _ => "not established: verify with the recipient".to_string(),
        };
        let mut row = Map::new();
        let mut put = |k: &str, v: String| {
            row.insert(k.into(), J::from(v));
        };
        put("id", id.clone());
        put("connection", id.clone());
        put("name", c.name.clone());
        put("state", c.state.clone());
        put("owner_scope", c.owner_scope.clone());
        put("owner_user", c.owner_user.clone());
        put("provider", c.provider.clone());
        put(
            "recipient",
            prov.as_ref()
                .map_or("unknown provider".into(), |p| p.legal_name.to_string()),
        );
        put(
            "recipient_country",
            prov.as_ref()
                .map_or(String::new(), |p| p.country.to_string()),
        );
        put(
            "purpose",
            consent_rec
                .as_ref()
                .map(|k| str_of(k, "purpose").to_string())
                .filter(|s| !s.is_empty())
                .or_else(|| prov.as_ref().map(|p| p.purpose.to_string()))
                .unwrap_or_default(),
        );
        put(
            "legal_basis",
            consent.map_or("not recorded".into(), |k| k.legal_basis.clone()),
        );
        put(
            "consent_recorded_at",
            consent.map_or(String::new(), |k| k.recorded_at.clone()),
        );
        put("data_subjects", subjects.into());
        put("personal_data_categories", categories.join(" | "));
        put("transfer_basis", transfer);
        put(
            "agreement_recorded",
            if pp.as_ref().is_some_and(|p| p.dpa_recorded) {
                "yes"
            } else {
                "no"
            }
            .into(),
        );
        put("retention_days", c.retention_days.to_string());
        put("safeguards", safeguards(&c, clear));
        out.push(row);
    }
    out
}

pub fn to_csv(rows: &[Map<String, J>]) -> String {
    let mut lines = vec![csv_row(&COLUMNS.map(String::from))];
    for r in rows {
        lines.push(csv_row(&COLUMNS.map(|c| str_of(r, c).to_string())));
    }
    lines.join("\r\n") + "\r\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::egress::tests::setup;
    use serde_json::json;

    #[test]
    fn one_row_per_connection_with_the_basis_and_no_credentials() {
        let (b, e, conn) = setup();
        let mut f = json!({
            "name": "Hook", "provider": "webhook_out", "state": "connected",
            "owner_scope": "instance", "owner_user": "root",
            "credentials": "obs1:secretsecret",
            "field_allowlist": "{\"party\":[\"name\",\"email\"]}",
        });
        f["field_allowlist"] = J::from(serde_json::to_string(&conn.allowlist).unwrap());
        b.update(CONNECTION, "c1", f.as_object().unwrap()).unwrap();
        b.update(CONSENT, "k1", json!({"legal_basis": "contract", "purpose": "Sync customers", "recorded_at": "2026-10-17T00:00:00Z"}).as_object().unwrap()).unwrap();
        let r = rows(&*b, &e, &|id| crate::catalog::provider(id));
        assert_eq!(r.len(), 1);
        assert_eq!(r[0]["legal_basis"], "contract");
        assert_eq!(r[0]["purpose"], "Sync customers");
        assert_eq!(r[0]["agreement_recorded"], "yes");
        assert!(
            r[0]["personal_data_categories"]
                .as_str()
                .unwrap()
                .contains("name (public_directory, pseudonymise)")
        );
        let csv = to_csv(&r);
        assert!(csv.starts_with("connection,name,state"));
        assert!(!csv.contains("obs1:") && !csv.contains("secretsecret"));
        assert_eq!(csv.lines().count(), 2);
    }

    #[test]
    fn csv_cannot_carry_a_formula() {
        let mut row = Map::new();
        row.insert("name".into(), J::from("=HYPERLINK(\"http://evil\")"));
        let csv = to_csv(&[row]);
        assert!(csv.contains("'=HYPERLINK"), "{csv}");
    }
}
