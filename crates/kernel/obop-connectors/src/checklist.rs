//! The compliance checklist: what is configured and what is missing, from the stored records.
//!
//! Every item is a statement about configuration that obop can see. None of them says that the
//! instance is compliant: the purposes, the legal bases, the agreements and the assessments are
//! the controller's to decide and to be able to show.

use serde_json::{Value as J, json};

use crate::model::*;
use crate::policy::Treatment;
use crate::runtime::Runtime;
use crate::secretbox::KeySource;
use crate::util::{now, parse_iso, str_of};

pub const DISCLAIMER: &str = "This checklist lists technical controls and settings that support GDPR compliance. It is not legal advice, not a certification and not a statement that your processing is lawful: purposes, legal bases, agreements with recipients, assessments and information duties remain yours to decide and to document.";

fn item(id: &str, status: &str, title: &str, detail: String, subjects: Vec<String>) -> J {
    json!({"id": id, "status": status, "title": title, "detail": detail, "subjects": subjects})
}

impl Runtime {
    pub fn checklist(&self) -> J {
        let inst = InstancePolicy::load(&*self.backend);
        let conns: Vec<(Connection, Option<crate::catalog::Provider>)> = self
            .backend
            .list(CONNECTION)
            .unwrap_or_default()
            .iter()
            .filter_map(|(id, f)| Connection::from_fields(id, f).ok())
            .filter(|c| c.state != "revoked")
            .map(|c| {
                let p = self.provider(&c.provider);
                (c, p)
            })
            .collect();
        let live: Vec<_> = conns.iter().filter(|(c, _)| c.is_connected()).collect();
        let names = |v: Vec<&(Connection, Option<crate::catalog::Provider>)>| -> Vec<String> {
            v.iter().map(|(c, _)| c.name.clone()).collect()
        };
        let mut items = Vec::new();

        items.push(match self.secrets.source() {
            KeySource::Environment => item("secret_key", "ok", "Credentials are encrypted with OBOP_SECRET_KEY", "The master key comes from the environment, so a copy of the database alone does not open the credentials.".into(), vec![]),
            KeySource::Generated => item("secret_key", "warn", "Credentials are encrypted with a key stored in the database", "OBOP_SECRET_KEY is not set, so a key was generated and kept next to the data. That is fine for evaluation. For production set OBOP_SECRET_KEY to a secret of at least 32 characters kept outside the database.".into(), vec![]),
        });

        items.push(if inst.flows_enabled {
            item(
                "master_switch",
                "info",
                "Data flows are switched on",
                "The master switch can stop every flow at once (Egress policy).".into(),
                vec![],
            )
        } else {
            item(
                "master_switch",
                "info",
                "Data flows are switched off",
                "Nothing leaves and nothing is accepted until the master switch is turned on."
                    .into(),
                vec![],
            )
        });

        // Agreements: a provider in use whose agreement is not recorded.
        let mut missing: Vec<String> = conns
            .iter()
            .filter(|(c, _)| c.state != "draft")
            .filter(|(c, _)| {
                !provider_policy(&*self.backend, &c.provider).is_some_and(|p| p.dpa_recorded)
            })
            .map(|(c, p)| {
                p.as_ref()
                    .map_or(c.provider.clone(), |p| p.name.to_string())
            })
            .collect();
        missing.sort();
        missing.dedup();
        items.push(if missing.is_empty() {
            item("agreements", "ok", "An agreement is recorded for every provider in use", "Recording it states that you hold a data processing agreement or equivalent with the recipient (Article 28). It does not check the agreement.".into(), vec![])
        } else {
            item("agreements", "warn", "Agreement not recorded for some providers in use", "Record the data processing agreement under the provider policy. While it is missing, personal data (even pseudonymised) is not sent when the instance requires it.".into(), missing)
        });

        let no_retention: Vec<_> = conns
            .iter()
            .filter(|(c, _)| c.retention_days <= 0)
            .collect();
        items.push(if no_retention.is_empty() {
            item("retention", "ok", "Retention is set on every connection", format!("Flow logs and runs are removed after the retention of their connection ({} days by default). Run the purge regularly or let the scheduler do it.", inst.default_retention_days), vec![])
        } else {
            item("retention", "warn", "Retention is not set on some connections", "Set the retention days.".into(), names(no_retention))
        });

        let overdue = self
            .backend
            .list(FLOW_LOG)
            .unwrap_or_default()
            .iter()
            .filter(|(_, f)| {
                parse_iso(str_of(f, "at")).is_some_and(|t| {
                    t + (inst.default_retention_days.max(1) as u64) * 86_400 * 4 < now()
                })
            })
            .count();
        if overdue > 0 {
            items.push(item(
                "purge",
                "todo",
                "Old log entries are waiting for the purge",
                format!(
                    "{overdue} flow log rows are far past the default retention. Run the purge."
                ),
                vec![],
            ));
        }

        let no_basis: Vec<_> = live
            .iter()
            .filter(|(c, _)| {
                !consents_of(&*self.backend, &c.id)
                    .iter()
                    .any(|k| k.is_active())
            })
            .collect();
        items.push(if no_basis.is_empty() {
            item("legal_basis", "ok", "Every connected connection has a recorded legal basis", "Each one has an active record of the purpose, the basis and the data classes. Withdrawing it revokes the connection.".into(), vec![])
        } else {
            item("legal_basis", "warn", "Connected without an active legal basis", "These cannot send or receive until a basis is recorded.".into(), live.iter().filter(|(c, _)| !consents_of(&*self.backend, &c.id).iter().any(|k| k.is_active())).map(|(c, _)| c.name.clone()).collect())
        });

        let high: Vec<_> = conns
            .iter()
            .filter(|(c, _)| !c.policy.acks.is_empty())
            .collect();
        items.push(if high.is_empty() {
            item("high_risk", "ok", "No high-risk class is released", "Confidential HR, special category and financial data are withheld everywhere by default.".into(), vec![])
        } else {
            item("high_risk", "warn", "High-risk classes are released with an acknowledgement", "Each has a recorded impact-assessment acknowledgement (who, when, text). Make sure the assessment itself exists and is current.".into(), names(high))
        });

        let clear: Vec<_> = conns
            .iter()
            .filter(|(c, _)| {
                c.policy
                    .classes
                    .values()
                    .chain(c.policy.fields.values())
                    .any(|t| *t == Treatment::Allow)
            })
            .collect();
        if !clear.is_empty() {
            items.push(item("clear_text", "warn", "Some fields leave in clear by policy", "The policy of these connections allows a field to leave as it is. Check that this is necessary for the purpose.".into(), names(clear)));
        }

        let mut third: Vec<String> = conns
            .iter()
            .filter(|(c, _)| c.state != "draft")
            .filter_map(|(_, p)| p.as_ref())
            .filter(|p| p.eu_adequate != "yes")
            .map(|p| {
                format!(
                    "{} ({})",
                    p.name,
                    if p.eu_adequate == "no" {
                        "third country"
                    } else {
                        "verify with the provider"
                    }
                )
            })
            .collect();
        third.sort();
        third.dedup();
        items.push(if third.is_empty() {
            item("transfers", "ok", "Providers in use rest on an EU or adequacy basis", "As stated in the catalogue; confirm with the provider.".into(), vec![])
        } else {
            item("transfers", "warn", "Transfer basis not established for some providers in use", "The catalogue could not establish an EU or adequacy basis. Confirm where the recipient processes data and which transfer mechanism applies (Articles 44 to 46).".into(), third)
        });

        if self.cfg.allow_loopback {
            items.push(item("loopback", "warn", "Loopback addresses are allowed for connections", "OBOP_CONNECTORS_ALLOW_LOOPBACK is set. Use it for tests and local development only.".into(), vec![]));
        }
        if self.cfg.public_url.is_none() {
            items.push(item("public_url", "info", "OBOP_PUBLIC_URL is not set", "Webhook addresses are shown relative to this server and OAuth connections cannot be authorised until it is set.".into(), vec![]));
        }
        if !inst.require_dpa {
            items.push(item(
                "require_dpa",
                "warn",
                "The agreement requirement is switched off",
                "Personal data may go to providers without a recorded agreement.".into(),
                vec![],
            ));
        }

        json!({"disclaimer": DISCLAIMER, "items": items, "generated_at": crate::util::iso(now())})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::ClientConfig;
    use crate::secretbox::SecretBox;
    use obop_commands::Dispatcher;
    use serde_json::json;
    use std::sync::Arc;

    #[test]
    fn checklist_flags_what_is_missing_and_always_carries_the_disclaimer() {
        let backend = obop_storage::open("memory:").unwrap();
        let rt = Runtime::with_providers(
            backend.clone(),
            Arc::new(Dispatcher::new()),
            SecretBox::for_tests(),
            crate::runtime::Config::default(),
            crate::catalog::providers(),
            Some(crate::net::Client::new(ClientConfig::default())),
        );
        let put = |m: &str, id: &str, v: J| backend.insert(m, id, v.as_object().unwrap()).unwrap();
        put(
            CONNECTION,
            "c1",
            json!({"name": "Hook", "provider": "webhook_out", "state": "connected", "retention_days": 30,
            "policy": "{\"classes\":{\"internal\":\"allow\"}}"}),
        );
        let v = rt.checklist();
        assert!(
            v["disclaimer"]
                .as_str()
                .unwrap()
                .contains("not legal advice")
        );
        let status = |id: &str| {
            v["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|i| i["id"] == id)
                .map(|i| i["status"].as_str().unwrap().to_string())
        };
        assert_eq!(status("agreements").as_deref(), Some("warn"));
        assert_eq!(status("legal_basis").as_deref(), Some("warn"));
        assert_eq!(status("clear_text").as_deref(), Some("warn"));
        assert_eq!(status("transfers").as_deref(), Some("warn"));
        assert_eq!(status("secret_key").as_deref(), Some("ok"));
        // Recording the agreement and a basis clears those items.
        put(
            PROVIDER_POLICY,
            "pp",
            json!({"provider": "webhook_out", "dpa_recorded": true}),
        );
        put(
            CONSENT,
            "k1",
            json!({"connection": "c1", "state": "active"}),
        );
        let v = rt.checklist();
        let status = |id: &str| {
            v["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|i| i["id"] == id)
                .map(|i| i["status"].as_str().unwrap().to_string())
        };
        assert_eq!(status("agreements").as_deref(), Some("ok"));
        assert_eq!(status("legal_basis").as_deref(), Some("ok"));
    }
}
