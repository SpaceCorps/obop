//! Tests of the module over real dispatcher, store and engines, with the `base`, `crm` and
//! `invoicing` modules as the data that flows. Nothing here talks to the internet: receivers and
//! the fake OAuth provider are threads on loopback.

use super::*;
use obop_commands::{CallError, Principal, Surface, Value, Vault};
use obop_connectors::catalog::providers;
use obop_connectors::webhook::{self, Scheme};
use obop_connectors::{Config, SecretBox};
use serde_json::{Value as J, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Mutex;

const SECRET: &str = "0123456789abcdef-signing-secret";

struct Env {
    d: Arc<Dispatcher>,
    s: Store,
    rt: Arc<Runtime>,
}

fn env() -> Env {
    env_on(Store::new())
}

fn env_on(s: Store) -> Env {
    let d = Arc::new(Dispatcher::new());
    base::register(&d, &s);
    crm::register(&d, &s);
    invoicing::register(&d, &s);
    let rt = Runtime::with_providers(
        s.backend(),
        d.clone(),
        SecretBox::for_tests(),
        Config {
            allow_loopback: true,
            public_url: Some("http://127.0.0.1:8080".into()),
        },
        providers(),
        None,
    );
    register(&d, &s, &rt);
    Env { d, s, rt }
}

fn admin() -> Principal {
    Principal::user("root", &["platform.admin"])
}
fn sam() -> Principal {
    Principal::user("sam", &["connectors.user", "base.user", "crm.user"])
}
fn ann() -> Principal {
    Principal::user("ann", &["connectors.user", "base.user"])
}
fn sam_agent() -> Principal {
    Principal::agent(
        "sam/agent",
        &["connectors.user", "base.user", "crm.user"],
        &["connectors.user", "base.user", "crm.user"],
    )
}

fn raw(e: &Env, p: &Principal, id: &str, v: J) -> Result<J, CallError> {
    e.d.call(
        Surface::Rest,
        p,
        &mut Vault::new(),
        id,
        &Value::from_json(&v),
    )
    .map(|r| r.value.to_json())
}
fn ok(e: &Env, p: &Principal, id: &str, v: J) -> J {
    raw(e, p, id, v).unwrap_or_else(|x| panic!("{id}: {x:?}"))
}
fn err(e: &Env, p: &Principal, id: &str, v: J) -> String {
    match raw(e, p, id, v) {
        Err(CallError::Handler(m)) => m,
        Err(other) => format!("{other:?}"),
        Ok(v) => panic!("{id} should have failed, got {v}"),
    }
}

/// Everything a response may not contain.
fn assert_clean(v: &J) {
    let text = v.to_string();
    for needle in [SECRET, "obs1:", "sk_live"] {
        assert!(!text.contains(needle), "{needle} leaked in {text}");
    }
}

// ---- a receiver on loopback ----------------------------------------------------------------------

struct Receiver {
    port: u16,
    got: Arc<Mutex<Vec<String>>>,
}

impl Receiver {
    fn new(status: u16) -> Self {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let got = Arc::new(Mutex::new(Vec::new()));
        let seen = got.clone();
        std::thread::spawn(move || {
            for s in l.incoming() {
                let Ok(mut s) = s else { break };
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let n = s.read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let t = String::from_utf8_lossy(&buf).to_string();
                    if let Some(h) = t.find("\r\n\r\n") {
                        let len = t[..h]
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                            })
                            .unwrap_or(0);
                        if buf.len() >= h + 4 + len {
                            break;
                        }
                    }
                }
                seen.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&buf).to_string());
                let _ = s.write_all(
                    format!(
                        "HTTP/1.1 {status} X\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{{}}"
                    )
                    .as_bytes(),
                );
            }
        });
        Self { port, got }
    }
    fn url(&self) -> String {
        format!("http://127.0.0.1:{}/hook", self.port)
    }
    fn count(&self) -> usize {
        self.got.lock().unwrap().len()
    }
    fn last(&self) -> String {
        self.got
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("a request was received")
    }
    fn body(&self) -> J {
        let l = self.last();
        serde_json::from_str(l.split("\r\n\r\n").nth(1).unwrap()).unwrap()
    }
    fn wait(&self, n: usize) -> bool {
        for _ in 0..100 {
            if self.count() >= n {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        false
    }
}

fn enable(e: &Env, provider: &str, users: bool) {
    ok(
        e,
        &admin(),
        "provider_policy.create",
        json!({"provider": provider, "enabled": true, "users_may_use": users, "dpa_recorded": true,
               "dpa_reference": "DPA signed 2026-10-01, folder Legal/DPAs",
               "legal_basis_text": "We send your contact record to the tool you chose."}),
    );
}

fn make_connection(e: &Env, p: &Principal, r: &Receiver, allow: J) -> String {
    let c = ok(
        e,
        p,
        "connection.create",
        json!({"name": "Billing hook", "provider": "webhook_out",
               "config": json!({"url": r.url()}).to_string(),
               "credentials": json!({"signing_secret": SECRET}).to_string(),
               "field_allowlist": allow.to_string()}),
    );
    c["id"].as_str().unwrap().to_string()
}

fn give_basis(e: &Env, p: &Principal, conn: &str) -> J {
    ok(
        e,
        p,
        "consent.create",
        json!({"connection": conn, "purpose": "Keep the billing tool up to date with customers", "legal_basis": "contract"}),
    )
}

fn ready(e: &Env, p: &Principal, r: &Receiver) -> String {
    let id = make_connection(
        e,
        p,
        r,
        json!({"party": ["name", "email", "notes", "tax_id"]}),
    );
    give_basis(e, p, &id);
    ok(e, p, "connection.connect", json!({"id": id}));
    id
}

fn seed_party(e: &Env) -> String {
    base::seed(&e.s)
}

// ---- tests -----------------------------------------------------------------------------------------

#[test]
fn the_module_passes_the_install_gate() {
    assert!(obop_registry::check_module(&module()).is_empty());
    // Every field has a class and a group; nothing returns a secret.
    for m in module().models {
        for f in m.fields {
            assert!(
                f.class.is_some() && !f.permission_group.is_empty(),
                "{}.{}",
                m.name,
                f.name
            );
        }
    }
}

#[test]
fn an_outbound_flow_end_to_end_sends_pseudonyms_and_never_the_denied_class() {
    let e = env();
    seed_party(&e);
    let r = Receiver::new(200);
    enable(&e, "webhook_out", false);
    let id = make_connection(
        &e,
        &admin(),
        &r,
        json!({"party": ["name", "email", "notes", "tax_id"]}),
    );

    // No legal basis yet: it cannot connect, and nothing can be sent.
    assert!(err(&e, &admin(), "connection.connect", json!({"id": id})).contains("legal basis"));
    give_basis(&e, &admin(), &id);
    ok(&e, &admin(), "connection.connect", json!({"id": id}));

    let out = ok(&e, &admin(), "connection.sync", json!({"id": id}));
    assert_eq!(r.count(), 1);
    assert_clean(&out);
    let body = r.body();
    let rec = &body["records"][0];
    for k in ["name", "email", "notes", "ref"] {
        assert!(rec[k].as_str().unwrap().starts_with("psn_"), "{k}: {rec}");
    }
    assert!(
        rec.get("tax_id").is_none(),
        "financial data never leaves by default"
    );
    let wire = r.last();
    for plain in ["Ada", "ada@example", "DE000000000", "Prefers invoices"] {
        assert!(!wire.contains(plain), "{plain} reached the wire");
    }
    // Signed with the shared secret, verifiable by the receiver.
    let headers: Vec<(String, String)> = wire
        .lines()
        .take_while(|l| !l.is_empty())
        .filter_map(|l| l.split_once(": "))
        .map(|(k, v)| (k.to_ascii_lowercase(), v.to_string()))
        .collect();
    let get = |k: &str| headers.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    let b = wire.split("\r\n\r\n").nth(1).unwrap().as_bytes();
    assert!(
        webhook::verify(
            Scheme::Standard,
            SECRET,
            &get,
            b,
            obop_connectors::util::now()
        )
        .is_ok()
    );

    // The audit trail: a flow log row without the payload, and a successful run.
    let logs = ok(&e, &admin(), "data_flow_log.list", json!({}));
    let log = logs
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["outcome"] == "released")
        .unwrap();
    assert_eq!(log["fields_denied"], "tax_id");
    assert_eq!(log["record_count"], 1);
    assert_eq!(log["payload_hash"].as_str().unwrap().len(), 64);
    assert!(!log.to_string().contains("psn_") && !log.to_string().contains("Ada"));
    let runs = ok(&e, &admin(), "sync_run.list", json!({}));
    assert!(
        runs.as_array()
            .unwrap()
            .iter()
            .any(|x| x["status"] == "succeeded" && x["records"] == 1)
    );
}

#[test]
fn credentials_are_encrypted_at_rest_and_never_returned() {
    let e = env();
    let r = Receiver::new(200);
    enable(&e, "webhook_out", true);
    let id = ready(&e, &admin(), &r);
    // At rest: sealed, not the secret.
    let stored = e.s.backend().get("connection", &id).unwrap().unwrap();
    let c = stored["credentials"].as_str().unwrap();
    assert!(c.starts_with("obs1:") && !c.contains(SECRET));
    // Over every surface, for every kind of caller.
    for p in [admin(), sam_agent()] {
        for (cmd, input) in [
            ("connection.get", json!({"id": id})),
            ("connection.list", json!({})),
            ("connection.search", json!({})),
            ("processing_register.list", json!({})),
            ("consent.list", json!({})),
            ("sync_run.list", json!({})),
            ("data_flow_log.list", json!({})),
        ] {
            if let Ok(v) = raw(&e, &p, cmd, input) {
                assert_clean(&v);
            }
        }
    }
    let got = ok(&e, &admin(), "connection.get", json!({"id": id}));
    assert_eq!(got["credentials_hint"], "signing_secret: set, ends cret");
    assert!(got.get("credentials").is_none() && got.get("oauth_tokens").is_none());
    // Errors do not echo it either.
    let bad = err(
        &e,
        &admin(),
        "connection.update",
        json!({"id": id, "credentials": json!({"nonsense": SECRET}).to_string()}),
    );
    assert!(!bad.contains(SECRET), "{bad}");
    let short = err(
        &e,
        &admin(),
        "connection.update",
        json!({"id": id, "credentials": json!({"signing_secret": "short"}).to_string()}),
    );
    assert!(short.contains("16 characters") && !short.contains("short\""));
    // A partial update keeps the other keys: an empty box means unchanged.
    ok(
        &e,
        &admin(),
        "connection.update",
        json!({"id": id, "credentials": json!({"signing_secret": ""}).to_string()}),
    );
    assert!(
        e.rt.credentials(&obop_connectors::model::Connection::load(&*e.s.backend(), &id).unwrap())
            .unwrap()
            .contains_key("signing_secret")
    );
}

#[test]
fn state_moves_only_through_the_governed_actions() {
    let e = env();
    let r = Receiver::new(200);
    enable(&e, "webhook_out", true);
    let id = make_connection(&e, &admin(), &r, json!({"party": ["name"]}));
    // Created as a draft whatever the caller asks.
    assert!(
        err(
            &e,
            &admin(),
            "connection.create",
            json!({"name": "x", "provider": "webhook_out", "state": "connected"})
        )
        .contains("starts as")
    );
    // The generic update cannot jump to connected without the basis either.
    assert!(
        err(
            &e,
            &admin(),
            "connection.update",
            json!({"id": id, "state": "connected"})
        )
        .contains("legal basis")
    );
    give_basis(&e, &admin(), &id);
    ok(
        &e,
        &admin(),
        "connection.update",
        json!({"id": id, "state": "connected"}),
    );
    assert_eq!(
        e.s.field("connection", &id, "state"),
        Some(Value::text("connected"))
    );
    ok(&e, &admin(), "connection.pause", json!({"id": id}));
    assert!(err(&e, &admin(), "connection.sync", json!({"id": id})).contains("paused"));
    ok(&e, &admin(), "connection.resume", json!({"id": id}));
    // A draft cannot be paused; a revoked connection is final.
    ok(&e, &admin(), "connection.revoke", json!({"id": id}));
    assert!(err(&e, &admin(), "connection.resume", json!({"id": id})).contains("must be paused"));
    assert!(
        err(
            &e,
            &admin(),
            "connection.update",
            json!({"id": id, "name": "again"})
        )
        .contains("revoked")
    );
    // Revocation purged what it held.
    let stored = e.s.backend().get("connection", &id).unwrap().unwrap();
    assert_eq!(stored["credentials"], "");
    assert_eq!(stored["credentials_hint"], "revoked");
}

#[test]
fn people_see_only_their_own_connections() {
    let e = env();
    let r = Receiver::new(200);
    enable(&e, "webhook_out", true);
    let mine = make_connection(&e, &sam(), &r, json!({"party": ["name"]}));
    let theirs = make_connection(&e, &ann(), &r, json!({"party": ["name"]}));
    for (p, own, other) in [(sam(), &mine, &theirs), (ann(), &theirs, &mine)] {
        let list = ok(&e, &p, "connection.list", json!({}));
        assert_eq!(list.as_array().unwrap().len(), 1);
        assert_eq!(list[0]["id"], own.as_str());
        let search = ok(&e, &p, "connection.search", json!({}));
        assert_eq!(search["total"], 1);
        // A filter cannot reach past the scope.
        let wide = ok(
            &e,
            &p,
            "connection.list",
            json!({"filter": [{"field": "owner_scope", "op": "eq", "value": "user"}]}),
        );
        assert_eq!(wide.as_array().unwrap().len(), 1);
        for cmd in [
            "connection.get",
            "connection.update",
            "connection.delete",
            "connection.connect",
            "connection.test",
            "connection.sync",
            "connection.revoke",
        ] {
            let m = err(&e, &p, cmd, json!({"id": other, "name": "x"}));
            assert_eq!(
                m,
                format!("no connection {other}"),
                "{cmd} must look like a missing record"
            );
        }
        // Their consent, runs and logs are theirs too.
        assert_eq!(
            ok(&e, &p, "consent.list", json!({}))
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }
    // An administrator sees both, never a credential.
    let all = ok(&e, &admin(), "connection.list", json!({}));
    assert_eq!(all.as_array().unwrap().len(), 2);
    assert_clean(&all);
    // A user's connection belongs to the user scope whatever they ask for.
    let forced = ok(
        &e,
        &sam(),
        "connection.create",
        json!({"name": "x", "provider": "webhook_out", "owner_scope": "instance", "owner_user": "ann"}),
    );
    assert_eq!(
        (
            forced["owner_scope"].as_str(),
            forced["owner_user"].as_str()
        ),
        (Some("user"), Some("sam"))
    );
    // Someone else's consent record cannot be created against my connection either.
    let m = err(
        &e,
        &ann(),
        "consent.create",
        json!({"connection": mine, "purpose": "I am not the owner of this", "legal_basis": "consent"}),
    );
    assert!(m.contains("no connection"));
}

#[test]
fn providers_reach_people_only_when_an_administrator_enabled_them_for_users() {
    let e = env();
    let names = |p: &Principal| -> Vec<String> {
        ok(&e, p, "connector_provider.list", json!({}))
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["id"].as_str().unwrap().to_string())
            .collect()
    };
    assert!(names(&sam()).is_empty());
    assert!(
        names(&admin()).len() >= 12,
        "the admin sees the whole catalogue"
    );
    // Enabled for the instance only: still not for people.
    enable(&e, "webhook_out", false);
    assert!(names(&sam()).is_empty());
    assert!(
        err(
            &e,
            &sam(),
            "connection.create",
            json!({"name": "x", "provider": "webhook_out"})
        )
        .contains("not enabled for you")
    );
    let pid = ok(&e, &admin(), "provider_policy.list", json!({}))[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    ok(
        &e,
        &admin(),
        "provider_policy.update",
        json!({"id": pid, "users_may_use": true}),
    );
    assert_eq!(names(&sam()), ["webhook_out"]);
    let row = ok(
        &e,
        &sam(),
        "connector_provider.get",
        json!({"id": "webhook_out"}),
    );
    assert_eq!(row["maturity"], "working");
    assert_eq!(
        row["legal_basis_text"],
        "We send your contact record to the tool you chose."
    );
    // The policy itself, with its agreement reference, is for administrators.
    assert!(matches!(
        raw(&e, &sam(), "provider_policy.list", json!({})),
        Err(CallError::Forbidden(_))
    ));
    // A definition-only provider cannot be connected at all.
    enable(&e, "google_oauth", true);
    assert!(
        err(
            &e,
            &admin(),
            "connection.create",
            json!({"name": "g", "provider": "google_oauth"})
        )
        .contains("definition only")
    );
}

#[test]
fn an_agent_may_only_propose_connecting_pausing_revoking_and_raising_the_policy() {
    let e = env();
    let r = Receiver::new(200);
    enable(&e, "webhook_out", true);
    let agent = sam_agent();
    let id = make_connection(&e, &agent, &r, json!({"party": ["name"]}));
    assert_eq!(
        e.s.field("connection", &id, "owner_user"),
        Some(Value::text("sam")),
        "an agent's connection is its owner's"
    );
    give_basis(&e, &agent, &id);
    let plan = |res: Result<J, CallError>| match res {
        Err(CallError::ApprovalRequired { plan_id }) => plan_id,
        other => panic!("expected a proposal, got {other:?}"),
    };
    let p = plan(raw(&e, &agent, "connection.connect", json!({"id": id})));
    assert_eq!(
        e.s.field("connection", &id, "state"),
        Some(Value::text("draft")),
        "nothing happened yet"
    );
    e.d.approve(&sam(), &mut Vault::new(), &p).unwrap();
    assert_eq!(
        e.s.field("connection", &id, "state"),
        Some(Value::text("connected"))
    );
    plan(raw(&e, &agent, "connection.pause", json!({"id": id})));
    plan(raw(&e, &agent, "connection.revoke", json!({"id": id})));
    plan(raw(&e, &agent, "connection.sync", json!({"id": id})));
    // The same through the generic update: a state change is the action, so it is a proposal too.
    plan(raw(
        &e,
        &agent,
        "connection.update",
        json!({"id": id, "state": "paused"}),
    ));
    // Raising the policy beyond the defaults is a proposal; tightening it is not.
    plan(raw(
        &e,
        &agent,
        "connection.update",
        json!({"id": id, "policy": json!({"classes": {"internal": "allow"}}).to_string()}),
    ));
    ok(
        &e,
        &agent,
        "connection.update",
        json!({"id": id, "policy": json!({"classes": {"internal": "redact"}}).to_string()}),
    );
    // Its own credentials never come back to it.
    assert_clean(&ok(&e, &agent, "connection.get", json!({"id": id})));
    // And it cannot withdraw consent on its own.
    let k = ok(&e, &agent, "consent.list", json!({}))[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    plan(raw(&e, &agent, "consent.withdraw", json!({"id": k})));
}

#[test]
fn withdrawing_consent_revokes_the_connection_and_purges_credentials() {
    let e = env();
    seed_party(&e);
    let r = Receiver::new(200);
    enable(&e, "webhook_out", true);
    let id = ready(&e, &sam(), &r);
    ok(&e, &sam(), "connection.sync", json!({"id": id}));
    assert_eq!(r.count(), 1);
    let k = ok(&e, &sam(), "consent.list", json!({}))[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    // Consent records are evidence: no edit, no delete.
    assert!(
        err(
            &e,
            &sam(),
            "consent.update",
            json!({"id": k, "purpose": "something else entirely"})
        )
        .contains("not edited")
    );
    assert!(err(&e, &sam(), "consent.delete", json!({"id": k})).contains("evidence"));
    // One click: withdraw.
    ok(&e, &sam(), "consent.withdraw", json!({"id": k}));
    let rec = e.s.backend().get("consent", &k).unwrap().unwrap();
    assert_eq!(
        (rec["state"].as_str(), rec["withdrawn_by"].as_str()),
        (Some("withdrawn"), Some("sam"))
    );
    assert!(!rec["withdrawn_at"].as_str().unwrap().is_empty());
    let conn = e.s.backend().get("connection", &id).unwrap().unwrap();
    assert_eq!(conn["state"], "revoked");
    assert_eq!(conn["credentials"], "");
    assert!(
        e.s.backend()
            .list("connector_pseudonym")
            .unwrap()
            .is_empty(),
        "pseudonym maps are dropped"
    );
    assert!(err(&e, &sam(), "connection.sync", json!({"id": id})).contains("revoked"));
    assert_eq!(r.count(), 1, "nothing was sent after the withdrawal");
    // Withdrawing twice is refused.
    assert!(err(&e, &sam(), "consent.withdraw", json!({"id": k})).contains("must be active"));
}

#[test]
fn high_risk_classes_need_a_raised_policy_an_acknowledgement_and_a_consent_that_covers_them() {
    let e = env();
    seed_party(&e);
    let r = Receiver::new(200);
    enable(&e, "webhook_out", true);
    let id = make_connection(&e, &admin(), &r, json!({"party": ["name", "tax_id"]}));
    give_basis(&e, &admin(), &id);
    ok(&e, &admin(), "connection.connect", json!({"id": id}));
    let policy = |ack: &str| {
        json!({"classes": {"financial": "pseudonymise"}, "acks": [{"class": "financial", "text": ack, "by": "somebody else", "at": "1999-01-01T00:00:00Z"}]}).to_string()
    };
    // A short text is not an assessment.
    assert!(
        err(
            &e,
            &admin(),
            "connection.update",
            json!({"id": id, "policy": policy("ok")})
        )
        .contains("at least 30 characters")
    );
    ok(
        &e,
        &admin(),
        "connection.update",
        json!({"id": id, "policy": policy("DPIA 2026-10: tax ids go to the billing tool, retention 7 years")}),
    );
    // Who and when come from the server, not from the client.
    let stored = e.s.backend().get("connection", &id).unwrap().unwrap();
    let pol: J = serde_json::from_str(stored["policy"].as_str().unwrap()).unwrap();
    assert_eq!(pol["acks"][0]["by"], "root");
    assert!(pol["acks"][0]["at"].as_str().unwrap().starts_with("20"));
    // The consent recorded before does not cover financial data: the flow is refused until it is recorded again.
    let refused = err(&e, &admin(), "connection.sync", json!({"id": id}));
    assert!(refused.contains("financial"), "{refused}");
    assert_eq!(r.count(), 0);
    give_basis(&e, &admin(), &id);
    ok(&e, &admin(), "connection.sync", json!({"id": id}));
    let rec = &r.body()["records"][0];
    assert!(rec["tax_id"].as_str().unwrap().starts_with("psn_"));
    assert!(!r.last().contains("DE000000000"));
    // The register shows the release.
    let reg = ok(&e, &admin(), "processing_register.get", json!({"id": id}));
    assert!(
        reg["personal_data_categories"]
            .as_str()
            .unwrap()
            .contains("tax_id (financial, pseudonymise)")
    );
    assert!(
        reg["safeguards"]
            .as_str()
            .unwrap()
            .contains("acknowledgement")
    );
}

#[test]
fn the_privacy_preview_shows_the_real_payload_and_each_treatment() {
    let e = env();
    let party = seed_party(&e);
    enable(&e, "webhook_out", true);
    let out = ok(
        &e,
        &sam(),
        "privacy_preview.render",
        json!({"provider": "webhook_out", "target_model": "party", "record_id": party,
               "field_allowlist": json!({"party": ["name", "email", "tax_id", "notes"]}).to_string()}),
    );
    let payload: J = serde_json::from_str(out["payload"].as_str().unwrap()).unwrap();
    assert!(payload["name"].as_str().unwrap().starts_with("psn_"));
    assert!(payload.get("tax_id").is_none());
    let t: J = serde_json::from_str(out["treatments"].as_str().unwrap()).unwrap();
    let field = |n: &str| {
        t["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["field"] == n)
            .unwrap()
            .clone()
    };
    assert_eq!(
        (
            field("name")["treatment"].as_str(),
            field("name")["class"].as_str()
        ),
        (Some("pseudonymise"), Some("public_directory"))
    );
    assert_eq!(field("tax_id")["treatment"], "deny");
    assert_eq!(field("tax_id")["denied"], true);
    assert_eq!(t["sends_personal_data"], true);
    // Nothing was stored by looking.
    assert!(
        e.s.backend().list("data_flow_log").unwrap().is_empty()
            && e.s
                .backend()
                .list("connector_pseudonym")
                .unwrap()
                .is_empty()
    );
    // Someone who may not read the model gets no sample from it.
    let nobody = Principal::user("zed", &["connectors.user"]);
    assert!(err(&e, &nobody, "privacy_preview.render", json!({"provider": "webhook_out", "target_model": "party", "field_allowlist": json!({"party": ["name"]}).to_string()})).contains("may not read"));
}

#[test]
fn connectors_cannot_touch_their_own_records_or_the_platforms_and_secrets_cannot_be_allowlisted() {
    let e = env();
    let r = Receiver::new(200);
    enable(&e, "webhook_out", true);
    for model in [
        "connection",
        "consent",
        "provider_policy",
        "egress_policy",
        "module",
        "setting",
        "connector_vault",
    ] {
        let m = err(
            &e,
            &admin(),
            "connection.create",
            json!({"name": "x", "provider": "webhook_out", "field_allowlist": json!({model: ["name"]}).to_string()}),
        );
        assert!(
            m.contains("cannot be on an allowlist") || m.contains("unknown model"),
            "{model}: {m}"
        );
    }
    assert!(err(&e, &admin(), "connection.create", json!({"name": "x", "provider": "webhook_out", "field_allowlist": json!({"party": ["nope"]}).to_string()})).contains("does not exist"));
    let m = err(
        &e,
        &admin(),
        "connection.create",
        json!({"name": "x", "provider": "webhook_in", "config": json!({"target_model": "connection"}).to_string()}),
    );
    assert!(m.contains("not a model connectors may write"), "{m}");
    // A secret is never allowlistable: use the platform's own secret fields.
    let m = err(
        &e,
        &admin(),
        "connection.create",
        json!({"name": "x", "provider": "webhook_out", "field_allowlist": json!({"provider_policy": ["oauth_client_secret"]}).to_string()}),
    );
    assert!(m.contains("cannot be on an allowlist"));
    // Unknown settings and secrets in the readable config are refused.
    assert!(err(&e, &admin(), "connection.create", json!({"name": "x", "provider": "webhook_out", "config": json!({"signing_secret": "abc"}).to_string()})).contains("credentials"));
    assert!(err(&e, &admin(), "connection.create", json!({"name": "x", "provider": "webhook_out", "config": json!({"urll": "x"}).to_string()})).contains("not a setting"));
    // Addresses are checked when saved: https only, allowed host.
    for url in [
        "http://example.com/hook",
        "https://user:pw@example.com/",
        "ftp://example.com/",
        "http://169.254.169.254/latest/meta-data",
    ] {
        assert!(err(&e, &admin(), "connection.create", json!({"name": "x", "provider": "webhook_out", "config": json!({"url": url}).to_string()})).contains("url refused"), "{url}");
    }
    // Slack's webhook is a secret and must be on Slack's host.
    enable(&e, "slack_webhook", true);
    let m = err(
        &e,
        &admin(),
        "connection.create",
        json!({"name": "s", "provider": "slack_webhook", "credentials": json!({"url": "https://evil.example.com/sk_live_x"}).to_string()}),
    );
    assert!(
        m.contains("not on the allow-list") && !m.contains("sk_live"),
        "{m}"
    );
    let m = err(
        &e,
        &admin(),
        "connection.create",
        json!({"name": "s", "provider": "slack_webhook", "config": json!({"url": "https://hooks.slack.com/services/T/B/X"}).to_string()}),
    );
    assert!(m.contains("credentials"), "{m}");
    let ok_slack = ok(
        &e,
        &admin(),
        "connection.create",
        json!({"name": "s", "provider": "slack_webhook", "credentials": json!({"url": "https://hooks.slack.com/services/T000/B000/sk_live_abc"}).to_string()}),
    );
    assert_clean(&ok_slack);
    let _ = r;
}

#[test]
fn a_signed_webhook_creates_records_through_the_governed_commands() {
    let e = env();
    enable(&e, "webhook_in", true);
    let c = ok(
        &e,
        &sam(),
        "connection.create",
        json!({
        "name": "Leads from the form", "provider": "webhook_in",
        "config": json!({"target_model": "lead", "mapping": json!({"data.title": "title", "data.contact": "contact_name"}).to_string(), "external_id_key": "data.id"}).to_string(),
        "credentials": json!({"signing_secret": SECRET}).to_string(),
        "field_allowlist": json!({"lead": ["title", "contact_name"]}).to_string()}),
    );
    let id = c["id"].as_str().unwrap().to_string();
    ok(
        &e,
        &sam(),
        "consent.create",
        json!({"connection": id, "purpose": "Create leads from the website form", "legal_basis": "legitimate_interest", "note": "Balancing test: the visitor asked us to get in touch."}),
    );
    // The user's own basis follows the administrator's setting when there is one; here none.
    ok(&e, &sam(), "connection.connect", json!({"id": id}));

    let send = |body: &J, secret: &str, id_hdr: &str, ts: u64| {
        let b = body.to_string();
        let sig = webhook::sign_standard(secret, id_hdr, ts, b.as_bytes());
        let hdr = move |k: &str| match k {
            "webhook-id" => Some(id_hdr.to_string()),
            "webhook-timestamp" => Some(ts.to_string()),
            "webhook-signature" => Some(sig.clone()),
            _ => None,
        };
        e.rt.receive_webhook(&id, &hdr, b.as_bytes())
    };
    let now = obop_connectors::util::now();
    let payload =
        json!({"data": {"id": 7, "title": "Quote for 40 seats", "contact": "Grace Hopper"}});
    let o = send(&payload, SECRET, "evt_1", now).unwrap();
    assert_eq!((o.created, o.updated), (1, 0));
    let leads = e.s.backend().list("lead").unwrap();
    assert_eq!(leads.len(), 1);
    assert_eq!(leads[0].1["title"], "Quote for 40 seats");
    assert_eq!(
        leads[0].1["stage"], "new",
        "the governed state starts at its first option"
    );
    // Where it came from is remembered.
    let origin =
        e.s.backend()
            .get("connector_origin", &format!("lead:{}", leads[0].0))
            .unwrap()
            .unwrap();
    assert_eq!(origin["connection"], id.as_str());

    // The same event id again is a replay; a bad signature and a stale timestamp are refused alike.
    assert_eq!(
        send(&payload, SECRET, "evt_1", now).unwrap_err(),
        obop_connectors::runtime::HookError::Unauthorised
    );
    assert_eq!(
        send(&payload, "wrong-secret-wrong-secret", "evt_2", now).unwrap_err(),
        obop_connectors::runtime::HookError::Unauthorised
    );
    assert_eq!(
        send(&payload, SECRET, "evt_3", now - 4000).unwrap_err(),
        obop_connectors::runtime::HookError::Unauthorised
    );
    assert_eq!(e.s.backend().list("lead").unwrap().len(), 1);
    // The next delivery of the same external id updates, not duplicates.
    let again =
        json!({"data": {"id": 7, "title": "Quote for 45 seats", "contact": "Grace Hopper"}});
    let o = send(&again, SECRET, "evt_4", now).unwrap();
    assert_eq!((o.created, o.updated), (0, 1));
    assert_eq!(
        e.s.backend().list("lead").unwrap()[0].1["title"],
        "Quote for 45 seats"
    );

    // A payload mapped to a field that is not on the allowlist is rejected, not trimmed.
    ok(&e, &sam(), "connection.pause", json!({"id": id}));
    ok(
        &e,
        &sam(),
        "connection.update",
        json!({"id": id, "config": json!({"target_model": "lead", "mapping": json!({"data.title": "title", "data.mail": "contact_email"}).to_string()}).to_string()}),
    );
    ok(&e, &sam(), "connection.resume", json!({"id": id}));
    let m = send(
        &json!({"data": {"title": "x", "mail": "a@b.test"}}),
        SECRET,
        "evt_5",
        now,
    )
    .unwrap_err();
    assert!(
        matches!(m, obop_connectors::runtime::HookError::Rejected(ref s) if s.contains("allowlist")),
        "{m:?}"
    );
    // The run and the log are recorded for the owner; nothing carries the payload.
    let runs = ok(&e, &sam(), "sync_run.list", json!({}));
    assert!(
        runs.as_array()
            .unwrap()
            .iter()
            .any(|r| r["kind"] == "webhook" && r["status"] == "succeeded")
    );
    let logs = ok(&e, &sam(), "data_flow_log.list", json!({}));
    assert!(
        logs.as_array()
            .unwrap()
            .iter()
            .any(|l| l["direction"] == "inbound" && l["outcome"] == "received")
    );
    assert!(!logs.to_string().contains("Grace"));
    // Not connected, not accepting.
    ok(&e, &sam(), "connection.pause", json!({"id": id}));
    assert!(matches!(
        send(&payload, SECRET, "evt_6", now).unwrap_err(),
        obop_connectors::runtime::HookError::Refused(_)
    ));
}

#[test]
fn a_connector_can_never_do_more_than_its_owner_and_approval_actions_stay_proposals() {
    let e = env();
    enable(&e, "webhook_in", true);
    // Ann cannot write leads (no crm.user): her connector cannot either.
    let make = |p: &Principal, groups_for_inbound: &[&str]| {
        let _ = groups_for_inbound;
        let c = ok(
            &e,
            p,
            "connection.create",
            json!({
            "name": "inbound", "provider": "webhook_in",
            "config": json!({"target_model": "lead", "mapping": json!({"t": "title"}).to_string()}).to_string(),
            "credentials": json!({"signing_secret": SECRET}).to_string(),
            "field_allowlist": json!({"lead": ["title"]}).to_string()}),
        );
        let id = c["id"].as_str().unwrap().to_string();
        ok(
            &e,
            p,
            "consent.create",
            json!({"connection": id, "purpose": "Create leads from the form", "legal_basis": "consent"}),
        );
        ok(&e, p, "connection.connect", json!({"id": id}));
        id
    };
    let send = |id: &str, body: J, eid: &str| {
        let b = body.to_string();
        let now = obop_connectors::util::now();
        let sig = webhook::sign_standard(SECRET, eid, now, b.as_bytes());
        let eid = eid.to_string();
        let hdr = move |k: &str| match k {
            "webhook-id" => Some(eid.clone()),
            "webhook-timestamp" => Some(now.to_string()),
            "webhook-signature" => Some(sig.clone()),
            _ => None,
        };
        e.rt.receive_webhook(id, &hdr, b.as_bytes())
    };
    let ann_conn = make(&ann(), &[]);
    let m = send(&ann_conn, json!({"t": "from ann"}), "a1").unwrap_err();
    assert!(
        matches!(m, obop_connectors::runtime::HookError::Rejected(ref s) if s.contains("may not create")),
        "{m:?}"
    );
    assert!(e.s.backend().list("lead").unwrap().is_empty());
    // Sam can: his connector can.
    let sam_conn = make(&sam(), &[]);
    send(&sam_conn, json!({"t": "from sam"}), "s1").unwrap();
    assert_eq!(e.s.backend().list("lead").unwrap().len(), 1);
    // Narrowing: if sam loses crm.user the snapshot is intersected with what the user table says.
    let mut f = obop_storage::Fields::new();
    f.insert("groups".into(), json!(["connectors.user"]));
    e.s.backend().insert("auth_user", "sam", &f).unwrap();
    let m = send(&sam_conn, json!({"t": "again"}), "s2").unwrap_err();
    assert!(
        matches!(m, obop_connectors::runtime::HookError::Rejected(_)),
        "{m:?}"
    );
    assert_eq!(e.s.backend().list("lead").unwrap().len(), 1);
}

#[test]
fn an_inbound_change_that_needs_approval_becomes_a_proposal() {
    let e = env();
    enable(&e, "rest_api_pull", true);
    let owner = Principal::user(
        "fin",
        &["connectors.user", "invoicing.user", "invoicing.manager"],
    );
    let inv =
        e.s.insert(
            "invoice",
            [
                ("state".to_string(), Value::text("draft")),
                ("total_minor".to_string(), Value::Int(100)),
            ]
            .into(),
        )
        .unwrap();
    // The sender knows the invoice under external id "X-1": map it by a first create is not possible,
    // so register the link as a previous delivery would have.
    let r = Receiver::new(200);
    let c = ok(
        &e,
        &owner,
        "connection.create",
        json!({
        "name": "ERP pull", "provider": "rest_api_pull",
        "config": json!({"url": r.url(), "target_model": "invoice", "external_id_key": "ref", "mapping": json!({"state": "state"}).to_string()}).to_string(),
        "field_allowlist": json!({"invoice": ["state"]}).to_string()}),
    );
    let id = c["id"].as_str().unwrap().to_string();
    ok(
        &e,
        &owner,
        "consent.create",
        json!({"connection": id, "purpose": "Mirror invoice states from the ERP", "legal_basis": "contract"}),
    );
    ok(&e, &owner, "connection.connect", json!({"id": id}));
    let mut f = obop_storage::Fields::new();
    f.insert("connection".into(), id.clone().into());
    f.insert("record".into(), format!("invoice:{inv}").into());
    e.s.backend()
        .insert(
            "connector_extmap",
            &format!("{id}:{}", obop_connectors::util::sha256_hex(b"X-1")),
            &f,
        )
        .unwrap();
    let ingress_run = |state: &str| {
        let (c, p) = e.rt.connection(&id).unwrap();
        e.rt.ingress.accept(
            &c,
            &p,
            "invoice",
            vec![obop_connectors::ingress::Item {
                values: json!({"state": state}).as_object().unwrap().clone(),
                external_id: Some("X-1".into()),
            }],
            &e.rt.connector_principal(&c, "invoice"),
            "run-1",
        )
    };
    let o = ingress_run("posted").unwrap();
    assert_eq!((o.updated, o.proposed.len()), (0, 1), "{o:?}");
    assert_eq!(
        e.s.field("invoice", &inv, "state"),
        Some(Value::text("draft")),
        "the invoice was not posted"
    );
    // A human approves; the action runs with its own handler (the number is assigned).
    e.d.approve(&owner, &mut Vault::new(), &o.proposed[0])
        .unwrap();
    assert_eq!(
        e.s.field("invoice", &inv, "state"),
        Some(Value::text("posted"))
    );
    assert!(
        matches!(e.s.field("invoice", &inv, "number"), Some(Value::Text(n)) if n.starts_with("INV-"))
    );
}

#[test]
fn erasing_a_party_drops_the_pseudonym_maps_and_tokens_that_pointed_at_it() {
    let e = env();
    let party = seed_party(&e);
    let r = Receiver::new(200);
    enable(&e, "webhook_out", true);
    let id = ready(&e, &admin(), &r);
    ok(
        &e,
        &admin(),
        "connection.update",
        json!({"id": id, "policy": json!({"fields": {"party.name": "mask"}}).to_string()}),
    );
    ok(&e, &admin(), "connection.sync", json!({"id": id}));
    let token = r.body()["records"][0]["name"].as_str().unwrap().to_string();
    assert!(token.starts_with("TKN_"));
    let (c, _) = e.rt.connection(&id).unwrap();
    assert_eq!(
        e.rt.egress.restore(&id, &token).as_deref(),
        Some("Ada Example")
    );
    e.rt.start();
    let privacy = Principal::user("dpo", &["privacy.officer", "base.user"]);
    ok(&e, &privacy, "party.erase", json!({"id": party}));
    let mut gone = false;
    for _ in 0..100 {
        if e.rt.egress.restore(&c.id, &token).is_none() {
            gone = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    assert!(gone, "the token must stop resolving after the erasure");
    assert!(
        e.s.backend()
            .list("connector_pseudonym")
            .unwrap()
            .iter()
            .all(|(_, f)| f["record"] != format!("party:{party}"))
    );
    // The register of flows records the erasure.
    let logs = ok(&e, &admin(), "data_flow_log.list", json!({}));
    assert!(
        logs.as_array()
            .unwrap()
            .iter()
            .any(|l| l["outcome"] == "erased")
    );
}

#[test]
fn retention_removes_old_flow_logs_and_runs() {
    let e = env();
    let r = Receiver::new(200);
    enable(&e, "webhook_out", true);
    let id = ready(&e, &admin(), &r);
    ok(
        &e,
        &admin(),
        "connection.update",
        json!({"id": id, "retention_days": 30}),
    );
    let b = e.s.backend();
    let old = obop_connectors::util::iso(obop_connectors::util::now() - 40 * 86_400);
    let recent = obop_connectors::util::iso(obop_connectors::util::now() - 5 * 86_400);
    for (i, at) in [(1, &old), (2, &recent)] {
        b.insert(
            "data_flow_log",
            &format!("l{i}"),
            json!({"connection": id, "at": at, "outcome": "released"})
                .as_object()
                .unwrap(),
        )
        .unwrap();
        b.insert(
            "sync_run",
            &format!("r{i}"),
            json!({"connection": id, "started_at": at, "status": "succeeded"})
                .as_object()
                .unwrap(),
        )
        .unwrap();
    }
    let out = ok(&e, &admin(), "egress_policy.purge", json!({}));
    let res: J = serde_json::from_str(out["result"].as_str().unwrap()).unwrap();
    assert_eq!(
        (res["flow_logs"].as_u64(), res["sync_runs"].as_u64()),
        (Some(1), Some(1))
    );
    assert!(
        b.get("data_flow_log", "l1").unwrap().is_none()
            && b.get("data_flow_log", "l2").unwrap().is_some()
    );
    // An agent may only propose a purge.
    let agent = Principal::agent("root/agent", &["platform.admin"], &["connectors.admin"]);
    assert!(matches!(
        raw(&e, &agent, "egress_policy.purge", json!({})),
        Err(CallError::ApprovalRequired { .. })
    ));
}

#[test]
fn subscribed_events_deliver_the_record_minimised() {
    let e = env();
    let r = Receiver::new(200);
    enable(&e, "webhook_out", true);
    let c = ok(
        &e,
        &admin(),
        "connection.create",
        json!({
        "name": "Won deals", "provider": "webhook_out",
        "config": json!({"url": r.url(), "events": "lead.mark_won"}).to_string(),
        "credentials": json!({"signing_secret": SECRET}).to_string(),
        "field_allowlist": json!({"lead": ["title", "contact_email", "expected_revenue_minor"]}).to_string()}),
    );
    let id = c["id"].as_str().unwrap().to_string();
    give_basis(&e, &admin(), &id);
    ok(&e, &admin(), "connection.connect", json!({"id": id}));
    e.rt.start();
    let lead = ok(
        &e,
        &admin(),
        "lead.create",
        json!({"title": "Big deal", "contact_email": "grace@example.test", "expected_revenue_minor": 500}),
    );
    let lead_id = lead["id"].as_str().unwrap();
    ok(&e, &admin(), "lead.mark_won", json!({"id": lead_id}));
    assert!(r.wait(1), "the event must be delivered");
    let body = r.body();
    assert_eq!(body["type"], "lead.mark_won");
    let rec = &body["records"][0];
    assert_eq!(rec["title"], "Big deal");
    assert_eq!(rec["expected_revenue_minor"], 500);
    assert!(rec["contact_email"].as_str().unwrap().starts_with("psn_"));
    assert!(!r.last().contains("grace@example"));
}

#[test]
fn a_failing_receiver_is_retried_then_dead_lettered_and_a_withdrawal_stops_the_retries() {
    let e = env();
    seed_party(&e);
    let r = Receiver::new(503);
    enable(&e, "webhook_out", true);
    let id = ready(&e, &admin(), &r);
    ok(&e, &admin(), "connection.sync", json!({"id": id}));
    assert_eq!(e.rt.queue.len(), 1);
    let run = ok(&e, &admin(), "sync_run.list", json!({}))[0].clone();
    assert_eq!(run["status"], "retrying");
    // Each due pass tries again; after the last backoff the delivery is dead-lettered.
    let mut at = obop_connectors::util::now();
    for _ in 0..8 {
        at += 8000;
        e.rt.process_queue(at);
    }
    assert!(e.rt.queue.is_empty());
    assert!(r.count() >= 6, "{}", r.count());
    let run = ok(&e, &admin(), "sync_run.get", json!({"id": run["id"]}));
    assert_eq!(run["status"], "dead_letter");
    assert!(run["error_summary"].as_str().unwrap().contains("gave up"));
    // Queue again, then withdraw the basis: the retry does not send.
    ok(&e, &admin(), "connection.sync", json!({"id": id}));
    let before = r.count();
    let k = ok(&e, &admin(), "consent.list", json!({}))[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    ok(&e, &admin(), "consent.withdraw", json!({"id": k}));
    e.rt.process_queue(at + 100_000);
    assert_eq!(r.count(), before);
}

#[test]
fn the_instance_switches_stop_flows_and_restrict_hosts() {
    let e = env();
    seed_party(&e);
    let r = Receiver::new(200);
    enable(&e, "webhook_out", true);
    let id = ready(&e, &admin(), &r);
    // People cannot touch the instance policy.
    assert!(matches!(
        raw(
            &e,
            &sam(),
            "egress_policy.update",
            json!({"flows_enabled": false})
        ),
        Err(CallError::Forbidden(_))
    ));
    ok(
        &e,
        &admin(),
        "egress_policy.update",
        json!({"flows_enabled": false}),
    );
    assert!(err(&e, &admin(), "connection.sync", json!({"id": id})).contains("switched off"));
    ok(
        &e,
        &admin(),
        "egress_policy.update",
        json!({"flows_enabled": true, "host_allowlist": "hooks.example.org"}),
    );
    assert!(err(&e, &admin(), "connection.sync", json!({"id": id})).contains("allow-list"));
    ok(
        &e,
        &admin(),
        "egress_policy.update",
        json!({"host_allowlist": ""}),
    );
    ok(&e, &admin(), "connection.sync", json!({"id": id}));
    assert_eq!(r.count(), 1);
    assert!(
        err(
            &e,
            &admin(),
            "egress_policy.update",
            json!({"host_allowlist": "bad host!"})
        )
        .contains("host names")
    );
    assert!(
        err(
            &e,
            &admin(),
            "egress_policy.update",
            json!({"nonsense": true})
        )
        .contains("not a setting")
    );
    // The agreement requirement: personal data goes nowhere without it.
    let pid = ok(&e, &admin(), "provider_policy.list", json!({}))[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    ok(
        &e,
        &admin(),
        "provider_policy.update",
        json!({"id": pid, "dpa_recorded": false}),
    );
    assert!(err(&e, &admin(), "connection.sync", json!({"id": id})).contains("agreement"));
}

#[test]
fn the_register_exports_csv_without_secrets_or_formulas_and_the_checklist_carries_the_disclaimer() {
    let e = env();
    let r = Receiver::new(200);
    enable(&e, "webhook_out", true);
    let id = ready(&e, &admin(), &r);
    ok(
        &e,
        &admin(),
        "connection.update",
        json!({"id": id, "name": "=cmd|' /C calc'!A0"}),
    );
    let out = ok(&e, &admin(), "processing_register.export_csv", json!({}));
    let csv = out["csv"].as_str().unwrap();
    assert!(csv.starts_with("connection,name,state"));
    assert!(csv.contains("'=cmd"), "formulas are defused: {csv}");
    assert_clean(&out);
    assert!(matches!(
        raw(&e, &sam(), "processing_register.export_csv", json!({})),
        Err(CallError::Forbidden(_))
    ));
    let list = ok(&e, &admin(), "processing_register.list", json!({}));
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["legal_basis"], "contract");
    assert_eq!(list[0]["agreement_recorded"], "yes");
    let c = ok(&e, &admin(), "egress_policy.checklist", json!({}));
    let res: J = serde_json::from_str(c["result"].as_str().unwrap()).unwrap();
    assert!(
        res["disclaimer"]
            .as_str()
            .unwrap()
            .contains("not legal advice")
    );
    assert!(
        res["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["id"] == "agreements" && i["status"] == "ok")
    );
}

#[test]
fn a_pull_reads_a_json_api_into_a_model_with_a_mapping() {
    let e = env();
    enable(&e, "rest_api_pull", true);
    // A tiny API that answers a JSON list.
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let seen_auth = Arc::new(Mutex::new(String::new()));
    let sa = seen_auth.clone();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { break };
            let mut buf = [0u8; 4096];
            let n = s.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            *sa.lock().unwrap() = req
                .lines()
                .find(|l| l.to_ascii_lowercase().starts_with("authorization:"))
                .unwrap_or("")
                .to_string();
            let body = json!({"data": {"items": [{"id": "a", "name": "Grace Hopper", "deal": "Mainframe"}, {"id": "b", "name": "Alan Turing", "deal": "Enigma"}]}}).to_string();
            let _ = s.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes());
        }
    });
    let c = ok(
        &e,
        &sam(),
        "connection.create",
        json!({
        "name": "CRM export", "provider": "rest_api_pull",
        "config": json!({"url": format!("http://127.0.0.1:{port}/leads"), "target_model": "lead", "records_path": "data.items",
                         "external_id_key": "id", "mapping": json!({"deal": "title", "name": "contact_name"}).to_string()}).to_string(),
        "credentials": json!({"api_key": "sk_live_pullkey_1234"}).to_string(),
        "field_allowlist": json!({"lead": ["title", "contact_name"]}).to_string()}),
    );
    let id = c["id"].as_str().unwrap().to_string();
    assert_clean(&c);
    ok(
        &e,
        &sam(),
        "consent.create",
        json!({"connection": id, "purpose": "Read leads from the partner CRM", "legal_basis": "contract"}),
    );
    ok(&e, &sam(), "connection.connect", json!({"id": id}));
    let out = ok(&e, &sam(), "connection.sync", json!({"id": id}));
    let res: J = serde_json::from_str(out["result"].as_str().unwrap()).unwrap();
    assert_eq!(
        (res["created"].as_u64(), res["rejected"].as_u64()),
        (Some(2), Some(0))
    );
    assert_eq!(
        seen_auth.lock().unwrap().as_str(),
        "authorization: Bearer sk_live_pullkey_1234"
    );
    // A second run updates by external id instead of adding again.
    let out = ok(&e, &sam(), "connection.sync", json!({"id": id}));
    let res: J = serde_json::from_str(out["result"].as_str().unwrap()).unwrap();
    assert_eq!(
        (res["created"].as_u64(), res["updated"].as_u64()),
        (Some(0), Some(2))
    );
    assert_eq!(e.s.backend().list("lead").unwrap().len(), 2);
    assert_clean(&ok(&e, &sam(), "sync_run.list", json!({})));
}

#[test]
fn csv_export_goes_through_the_same_policy() {
    let e = env();
    seed_party(&e);
    enable(&e, "csv_export", true);
    let c = ok(
        &e,
        &sam(),
        "connection.create",
        json!({"name": "Customer file", "provider": "csv_export",
        "field_allowlist": json!({"party": ["name", "email", "tax_id", "notes"]}).to_string()}),
    );
    let id = c["id"].as_str().unwrap().to_string();
    ok(
        &e,
        &sam(),
        "consent.create",
        json!({"connection": id, "purpose": "A file for the accountant", "legal_basis": "legal_obligation"}),
    );
    ok(&e, &sam(), "connection.connect", json!({"id": id}));
    let out = ok(&e, &sam(), "connection.sync", json!({"id": id}));
    let res: J = serde_json::from_str(out["result"].as_str().unwrap()).unwrap();
    let csv = res["csv"].as_str().unwrap();
    assert!(csv.starts_with("ref,name,email,notes\r\n"), "{csv}");
    assert!(csv.contains("psn_") && !csv.contains("Ada") && !csv.contains("DE000000000"));
    assert_eq!(res["filename"], "party.csv");
}

#[test]
fn event_and_catalogue_metadata_is_honest() {
    let e = env();
    let _ = &e;
    let p = providers();
    assert!(
        p.iter()
            .filter(|x| x.maturity == obop_connectors::Maturity::Working)
            .count()
            == 4
    );
    assert!(p.iter().all(|x| !x.purpose.is_empty()));
    let v = ok(&e, &admin(), "connector_provider.list", json!({}));
    for row in v.as_array().unwrap() {
        assert!(
            ["working", "generic_adapter", "definition_only"]
                .contains(&row["maturity"].as_str().unwrap())
        );
    }
}

// ---- the snapshot the public website renders its app page from ------------------------------------

/// The module in the manifest format the website reads (`web/apps/site/data/builtin`). Nobody edits
/// the file by hand: this test regenerates it and fails when the file is stale.
fn site_snapshot() -> String {
    let m = module();
    let mut out = String::from("# Snapshot of crates/modules/connectors/src/meta.rs (built-in).\n");
    out.push_str(&format!(
        "[module]\nid = \"connectors\"\nversion = \"builtin\"\nname = \"Connectors\"\ndescription = {:?}\nauthor = \"obop\"\ndepends = []\n",
        "Connect external systems and let people connect their own accounts, with the controls that support GDPR compliance built in."
    ));
    for model in &m.models {
        out.push_str(&format!(
            "\n[[model]]\nname = {:?}\ndescription = {:?}\nread_group = {:?}\nwrite_group = {:?}\n",
            model.name, model.description, model.read_group, model.write_group
        ));
        for f in &model.fields {
            out.push_str(&format!(
                "\n[[model.field]]\nname = {:?}\ndescription = {:?}\nclass = {:?}\n",
                f.name,
                f.description,
                f.class.map_or("none", |c| c.as_str())
            ));
            if f.personal_data {
                out.push_str("personal_data = true\n");
            }
            if f.permission_group != model.read_group {
                out.push_str(&format!("group = {:?}\n", f.permission_group));
            }
            if f.kind.name() != "text" {
                out.push_str(&format!("kind = {:?}\n", f.kind.name()));
            }
            if !f.kind.options().is_empty() {
                out.push_str(&format!("options = {:?}\n", f.kind.options()));
            }
            if let Some(to) = f.kind.target() {
                out.push_str(&format!("to = {to:?}\n"));
            }
            if f.required {
                out.push_str("required = true\n");
            }
        }
        for c in &model.commands {
            out.push_str(&format!(
                "\n[[model.action]]\nname = {:?}\ndescription = {:?}\n",
                c.name, c.description
            ));
            if let Some(t) = c.transition {
                out.push_str(&format!("field = {:?}\n", t.field));
                if let Some(from) = t.from {
                    out.push_str(&format!("from = {from:?}\n"));
                }
                out.push_str(&format!("to = {:?}\n", t.to));
            }
            if c.permission_group != model.write_group {
                out.push_str(&format!("group = {:?}\n", c.permission_group));
            }
            if c.needs_approval {
                out.push_str("approval = true\n");
            }
        }
    }
    out
}

#[test]
fn the_website_snapshot_matches_the_module() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../web/apps/site/data/builtin/connectors.toml");
    let want = site_snapshot();
    if std::env::var("OBOP_UPDATE_SNAPSHOT").is_ok() {
        std::fs::write(&path, &want).unwrap();
    }
    let have = std::fs::read_to_string(&path).expect("the snapshot exists");
    assert_eq!(
        have, want,
        "web/apps/site/data/builtin/connectors.toml is stale: run the connectors tests with OBOP_UPDATE_SNAPSHOT=1"
    );
}

// ---- the same flow on every storage backend ----------------------------------------------------------

/// Outbound with pseudonyms, the log, withdrawal and the purge of what the connection held.
fn flow_on(e: &Env) {
    let b = e.s.backend();
    // A shared database may hold earlier runs: start from nothing.
    for table in [
        "connection",
        "consent",
        "provider_policy",
        "sync_run",
        "data_flow_log",
        "egress_policy",
        "connector_pseudonym",
        "connector_vault",
        "connector_extmap",
        "connector_origin",
        "connector_seen",
        "party",
    ] {
        for (id, _) in b.list(table).unwrap() {
            b.delete(table, &id).unwrap();
        }
    }
    let party = seed_party(e);
    let r = Receiver::new(200);
    enable(e, "webhook_out", true);
    let id = ready(e, &sam(), &r);
    ok(e, &sam(), "connection.sync", json!({"id": id}));
    let rec = &r.body()["records"][0];
    assert!(rec["name"].as_str().unwrap().starts_with("psn_") && rec.get("tax_id").is_none());
    assert!(!r.last().contains("Ada Example") && !r.last().contains(&party));
    assert!(!b.list("connector_pseudonym").unwrap().is_empty());
    let k = ok(e, &sam(), "consent.list", json!({}))[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    ok(e, &sam(), "consent.withdraw", json!({"id": k}));
    assert!(b.list("connector_pseudonym").unwrap().is_empty());
    assert_eq!(
        b.get("connection", &id).unwrap().unwrap()["credentials"],
        ""
    );
    // Replay protection and the purge use the same primitives on every backend.
    assert!(obop_connectors::webhook::Seen::claim(&*b, "c", "evt", 1).is_ok());
    assert!(obop_connectors::webhook::Seen::claim(&*b, "c", "evt", 1).is_err());
    assert_eq!(e.rt.purge(obop_connectors::util::now() + 400 * 86_400).0, 1);
}

#[test]
fn the_flow_works_on_sqlite() {
    let path = std::env::temp_dir().join(format!(
        "obop-connectors-flow-{}.sqlite",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    flow_on(&env_on(
        Store::open(&format!("sqlite://{}", path.display())).unwrap(),
    ));
    let _ = std::fs::remove_file(&path);
}

/// Runs only when `OBOP_TEST_POSTGRES_URL` is set (a throwaway database).
#[test]
fn the_flow_works_on_postgres() {
    let Ok(url) = std::env::var("OBOP_TEST_POSTGRES_URL") else {
        return;
    };
    flow_on(&env_on(Store::open(&url).unwrap()));
}
