//! Regressions of the second review of 2026-11-18: what an agent reads of a failure, what the
//! metadata offers on a delivery, and what an agent may change without a proposal.

use super::*;

fn agent() -> Principal {
    Principal::agent("ada/agent", &[ADMIN, SENDER], &[ADMIN, SENDER])
}

fn get(r: &Rig, p: &Principal, id: &str, model: &str) -> J {
    r.call(p, &format!("{model}.get"), v(&[("id", id.into())]))
        .unwrap()
}

#[test]
fn an_agent_never_reads_a_recipient_in_the_reason_of_a_failure() {
    let r = rig();
    let s = smtp(Script {
        rcpt: 550,
        ..Script::default()
    });
    let srv = r.server(&[("port", J::from(s.port))]);
    let id = r
        .send(&[("to", "erika.mustermann@example.com".into())])
        .unwrap();
    assert_eq!(r.mail.process(T0).unwrap().failed, 1);

    // The reason names the step, not the address (a relay's answer may still repeat it).
    let person = get(&r, &admin(), &id, OUTBOUND);
    let why = person["last_error"].as_str().unwrap();
    assert!(why.contains("RCPT TO") && why.contains("550"), "{why}");
    assert!(!why.contains("erika"), "{why}");
    // An agent gets the reason masked, as it gets the address.
    let bot = get(&r, &agent(), &id, OUTBOUND);
    assert_ne!(bot["last_error"], person["last_error"], "{bot}");
    assert!(!bot.to_string().contains("erika"), "{bot}");

    let _ = r.call(
        &admin(),
        "mail_server.test_send",
        v(&[
            ("id", srv.clone().into()),
            ("test_recipient", "hans.private@example.com".into()),
        ]),
    );
    let person = get(&r, &admin(), &srv, SERVER);
    let result = person["last_test_result"].as_str().unwrap();
    assert!(result.starts_with("failed"), "{result}");
    assert!(!result.contains("hans.private"), "{result}");
    let bot = get(&r, &agent(), &srv, SERVER);
    assert_ne!(bot["last_test_result"], person["last_test_result"], "{bot}");
    assert!(!bot.to_string().contains("hans.private"), "{bot}");
}

#[test]
fn the_metadata_offers_no_edit_of_a_delivery_and_each_action_only_where_it_starts() {
    let r = rig();
    let meta = r.d.meta(Surface::Rest, &admin());
    let model = meta
        .iter()
        .flat_map(|m| m.models.iter())
        .find(|m| m.name == OUTBOUND)
        .unwrap();
    // Only `send` queues a message: no screen offers New, Import, Save or Edit in list.
    assert!(!model.can_create && !model.can_bulk_create && !model.can_update);
    let from = |c: &str| {
        model
            .actions
            .iter()
            .find(|a| a.id == format!("{OUTBOUND}.{c}"))
            .and_then(|a| a.transition)
            .map(|t| (t.field, t.from))
    };
    assert_eq!(from("retry"), Some(("status", Some("failed"))));
    assert_eq!(from("cancel"), Some(("status", Some("queued"))));
    // The worker's numbers read as times where a holder of every group sees them.
    for f in ["lease_until", "due"] {
        let def = crate::meta::module()
            .models
            .iter()
            .find(|m| m.name == OUTBOUND)
            .unwrap()
            .fields
            .iter()
            .find(|x| x.name == f)
            .unwrap()
            .clone();
        assert_eq!(
            def.hints.as_ref().and_then(|h| h.ui.display.as_deref()),
            Some("epoch_seconds"),
            "{f}"
        );
    }
    // The server holds the same: a sent message is not queued again, nor an update taken.
    let s = smtp(Script::default());
    r.server(&[("port", J::from(s.port))]);
    let id = r.send(&[]).unwrap();
    assert_eq!(r.mail.process(T0).unwrap().sent, 1);
    assert!(
        r.call(
            &admin(),
            "outbound_mail.retry",
            v(&[("id", id.clone().into())])
        )
        .is_err()
    );
    assert!(
        r.call(
            &admin(),
            "outbound_mail.update",
            v(&[("id", id.clone().into()), ("subject", "x".into())]),
        )
        .is_err()
    );
    assert_eq!(text(&r.msg(&id), "status"), "sent");
}

#[test]
fn an_agent_only_proposes_a_change_to_a_template() {
    let r = rig();
    let made = r
        .call(
            &admin(),
            "mail_template.create",
            v(&[
                ("key", "password_reset".into()),
                ("language", "en".into()),
                ("subject", "Reset".into()),
                ("body", "Open {{link}}".into()),
            ]),
        )
        .unwrap();
    let tid = made["id"].as_str().unwrap().to_string();
    let before = r.mail.backend.get(TEMPLATE, &tid).unwrap().unwrap();
    let proposed = |id: &str, input: Value| {
        let out = r.call(&agent(), id, input);
        assert!(
            format!("{out:?}").contains("ApprovalRequired"),
            "{id}: {out:?}"
        );
    };
    proposed(
        "mail_template.create",
        v(&[
            ("key", "invoice".into()),
            ("language", "en".into()),
            ("subject", "Invoice".into()),
            ("body", "Click http://evil.example {{link}}".into()),
        ]),
    );
    proposed(
        "mail_template.update",
        v(&[
            ("id", tid.clone().into()),
            ("body", "Click http://evil.example {{link}}".into()),
        ]),
    );
    proposed("mail_template.delete", v(&[("id", tid.clone().into())]));
    assert_eq!(r.mail.backend.get(TEMPLATE, &tid).unwrap().unwrap(), before);
    assert!(
        r.mail
            .backend
            .find(TEMPLATE, "key", &J::from("invoice"))
            .unwrap()
            .is_empty()
    );
}
