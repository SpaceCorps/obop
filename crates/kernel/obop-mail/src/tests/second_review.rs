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
