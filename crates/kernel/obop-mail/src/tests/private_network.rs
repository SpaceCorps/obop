//! The owner's answer of 2026-11-18 to docs/spec/mail.md 6 (U56): a relay on the business's own
//! network and security `none` there are allowed through the per-server switch
//! `allow_private_network`, off by default, that only a person turns on; link-local and
//! cloud-metadata addresses stay refused whatever the switch says.

use super::*;
use crate::Failure;
use crate::smtp::{Relay, Security, vet};

fn agent() -> Principal {
    Principal::agent("ada/agent", &[ADMIN, SENDER], &[ADMIN, SENDER])
}

fn relay(host: &str, private_network: bool) -> Relay {
    Relay {
        host: host.into(),
        port: 25,
        security: Security::None,
        username: String::new(),
        password: String::new(),
        helo: "obop.test".into(),
        private_network,
    }
}

fn base(host: &str) -> Vec<(&'static str, J)> {
    vec![
        ("name", "Office relay".into()),
        ("kind", "smtp".into()),
        ("host", host.into()),
        ("port", 25.into()),
        ("security", "none".into()),
        ("from_address", "rechnung@nordlicht.example".into()),
    ]
}

fn with(mut f: Vec<(&'static str, J)>, extra: &[(&'static str, J)]) -> Value {
    for (k, val) in extra {
        f.retain(|(x, _)| x != k);
        f.push((k, val.clone()));
    }
    v(&f)
}

#[test]
fn a_relay_on_a_private_address_is_refused_unless_its_server_allows_the_private_network() {
    let r = rig_with(false);
    let client = &r.mail.rt.client;
    for host in ["10.0.0.5", "172.16.4.2", "192.168.1.20", "fd12:3456::25"] {
        match vet(client, &relay(host, false)) {
            Err(Failure::Permanent(_)) => {}
            other => panic!("{host} with the switch off: {other:?}"),
        }
        let addrs = vet(client, &relay(host, true)).unwrap_or_else(|e| panic!("{host}: {e:?}"));
        assert_eq!(addrs.len(), 1, "{host}");
    }
}

#[test]
fn link_local_metadata_and_loopback_addresses_stay_refused_with_the_switch_on() {
    let r = rig_with(false);
    let client = &r.mail.rt.client;
    for host in [
        "169.254.169.254",
        "169.254.10.1",
        "fe80::1",
        "fd00:ec2::254",
        "100.100.100.200",
        "127.0.0.1",
        "::ffff:169.254.169.254",
    ] {
        match vet(client, &relay(host, true)) {
            Err(Failure::Permanent(_)) => {}
            other => panic!("{host} must stay refused: {other:?}"),
        }
    }
}

#[test]
fn security_none_with_a_password_is_accepted_only_where_the_switch_is_on() {
    let r = rig();
    let pw = [
        ("username", J::from("office")),
        ("password", J::from("relay-secret-1")),
    ];
    let off = r.call(&admin(), "mail_server.create", with(base("10.0.0.5"), &pw));
    assert!(
        off.as_ref()
            .is_err_and(|e| e.contains("never sent without TLS")),
        "{off:?}"
    );
    let mut on = pw.to_vec();
    on.push(("allow_private_network", J::Bool(true)));
    let out = r
        .call(&admin(), "mail_server.create", with(base("10.0.0.5"), &on))
        .unwrap();
    assert_eq!(out["allow_private_network"], J::Bool(true));
}

#[test]
fn an_agent_cannot_turn_the_switch_on_and_a_person_s_change_is_in_the_audit_log() {
    let r = rig();
    // An agent's create or update that turns it on is refused outright, not proposed.
    let e = r
        .call(
            &agent(),
            "mail_server.create",
            with(
                base("10.0.0.5"),
                &[("allow_private_network", J::Bool(true))],
            ),
        )
        .unwrap_err();
    assert!(e.contains("only a person"), "{e}");
    let id = r
        .call(&admin(), "mail_server.create", v(&base("10.0.0.5")))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let e = r
        .call(
            &agent(),
            "mail_server.update",
            v(&[
                ("id", id.clone().into()),
                ("allow_private_network", J::Bool(true)),
            ]),
        )
        .unwrap_err();
    assert!(e.contains("only a person"), "{e}");
    assert_ne!(
        r.mail
            .backend
            .get(SERVER, &id)
            .unwrap()
            .unwrap()
            .get("allow_private_network"),
        Some(&J::Bool(true)),
        "still off"
    );

    // A person turns it on, then off: two lines in the security log, naming the person.
    for on in [true, false] {
        r.call(
            &admin(),
            "mail_server.update",
            v(&[
                ("id", id.clone().into()),
                ("allow_private_network", J::Bool(on)),
            ]),
        )
        .unwrap();
    }
    let events: Vec<_> = r
        .mail
        .backend
        .list(obop_commands::audit::EVENT_TABLE)
        .unwrap()
        .into_iter()
        .filter(|(_, f)| f["kind"] == "mail_private_network")
        .map(|(_, f)| {
            (
                f["actor"].clone(),
                f["detail"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(events.len(), 2, "{events:?}");
    assert!(events.iter().all(|(a, d)| a == "ada" && d.contains(&id)));
    assert!(events.iter().any(|(_, d)| d.ends_with("allowed")));
    assert!(events.iter().any(|(_, d)| d.ends_with("refused")));
}

#[test]
fn turning_the_switch_off_on_a_server_with_a_password_and_no_tls_says_what_goes_with_it() {
    let r = rig();
    let fields = with(
        base("10.0.0.5"),
        &[
            ("username", J::from("office")),
            ("password", J::from("relay-secret-1")),
            ("allow_private_network", J::Bool(true)),
        ],
    );
    let id = r.call(&admin(), "mail_server.create", fields).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    // Off alone would leave a password for a send without TLS: refused, and the refusal does
    // not tell the person to turn on the switch they are turning off.
    let e = r
        .call(
            &admin(),
            "mail_server.update",
            v(&[
                ("id", id.clone().into()),
                ("allow_private_network", J::Bool(false)),
            ]),
        )
        .unwrap_err();
    assert!(e.contains("to turn off"), "{e}");
    assert!(!e.contains("or allow the business"), "{e}");
    // Off together with TLS is accepted.
    let out = r
        .call(
            &admin(),
            "mail_server.update",
            v(&[
                ("id", id.clone().into()),
                ("allow_private_network", J::Bool(false)),
                ("security", "starttls".into()),
                ("port", 587.into()),
            ]),
        )
        .unwrap();
    assert_eq!(out["allow_private_network"], J::Bool(false));
    assert_eq!(out["security"], "starttls");
}
