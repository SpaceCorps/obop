//! Stream n6 (docs/ISSUES.md U58, U63, U66): the sealed body is never in an answer, a test
//! message's outcome is a code, and the commands of the whole model are marked as such.

use super::*;

/// A person who holds every group, as a platform administrator does.
fn platform_admin() -> Principal {
    Principal::user("root", &["platform.admin"])
}

#[test]
fn the_sealed_body_is_in_no_answer_also_for_a_platform_administrator() {
    let r = rig();
    r.server(&[("port", J::from(25_u16))]);
    let id = r.send(&[]).unwrap();
    // The store holds the sealed text; the worker reads it there.
    assert!(!text(&r.msg(&id), "body").is_empty());
    for p in [platform_admin(), admin()] {
        let got = r
            .call(&p, "outbound_mail.get", v(&[("id", id.clone().into())]))
            .unwrap();
        assert!(got.get("body").is_none(), "get: {got}");
        assert_eq!(got["status"], "queued", "the rest is there: {got}");
        let listed = r.call(&p, "outbound_mail.list", v(&[])).unwrap();
        let searched = r.call(&p, "outbound_mail.search", v(&[])).unwrap();
        for (what, out) in [("list", listed), ("search", searched)] {
            let rows = out
                .get("rows")
                .cloned()
                .unwrap_or(out.clone())
                .as_array()
                .cloned()
                .unwrap_or_default();
            assert!(!rows.is_empty(), "{what}: {out}");
            assert!(
                rows.iter().all(|m| m.get("body").is_none()),
                "{what}: {out}"
            );
        }
    }
}

#[test]
fn a_test_message_stores_its_outcome_as_a_code_beside_the_server_s_words() {
    let r = rig();
    let test = |srv: &str| {
        r.call(
            &admin(),
            "mail_server.test_send",
            v(&[
                ("id", srv.to_string().into()),
                ("test_recipient", "ada@example.org".into()),
            ]),
        )
    };
    let outcome = |srv: &str| {
        let f = r.mail.backend.get(SERVER, srv).unwrap().unwrap();
        (
            text(&f, "last_test_outcome").to_string(),
            text(&f, "last_test_result").to_string(),
        )
    };
    // Sent.
    let s = smtp(Script::default());
    let ok = r.server(&[("port", J::from(s.port)), ("name", "Sent".into())]);
    let out = test(&ok).unwrap();
    assert_eq!(out["last_test_outcome"], "sent", "{out}");
    assert_eq!(outcome(&ok).0, "sent");
    // Refused: a permanent answer of the relay.
    let s = smtp(Script {
        rcpt: 550,
        ..Script::default()
    });
    let refused = r.server(&[("port", J::from(s.port)), ("name", "Refused".into())]);
    let e = test(&refused).unwrap_err();
    assert!(e.contains("The test message was not sent: "), "{e}");
    let (code, words) = outcome(&refused);
    assert_eq!(code, "permanent");
    assert!(
        words.contains("550"),
        "the reason stays as the detail: {words}"
    );
    // Not now: a temporary answer of the relay.
    let s = smtp(Script {
        data_end: 451,
        ..Script::default()
    });
    let later = r.server(&[("port", J::from(s.port)), ("name", "Later".into())]);
    assert!(test(&later).is_err());
    assert_eq!(outcome(&later).0, "temporary");
    // Nobody writes the code by hand.
    r.call(
        &admin(),
        "mail_server.update",
        v(&[
            ("id", later.clone().into()),
            ("last_test_outcome", "sent".into()),
        ]),
    )
    .ok();
    assert_eq!(outcome(&later).0, "temporary");
}

#[test]
fn send_and_process_are_commands_of_the_whole_model_and_the_others_are_not() {
    let r = rig();
    let meta = r.d.meta(Surface::Rest, &admin());
    let model = meta
        .iter()
        .flat_map(|m| m.models.iter())
        .find(|m| m.name == OUTBOUND)
        .expect("outbound_mail");
    let flag = |name: &str| {
        model
            .actions
            .iter()
            .find(|a| a.id == format!("{OUTBOUND}.{name}"))
            .unwrap_or_else(|| panic!("{name}"))
            .whole_model
    };
    assert!(flag("send"));
    let send = model
        .actions
        .iter()
        .find(|a| a.id == format!("{OUTBOUND}.send"))
        .unwrap();
    assert_eq!(
        send.ui
            .as_ref()
            .map(|u| u.needs.clone())
            .unwrap_or_default(),
        ["to", "subject", "body"],
        "the list's Send asks for the message"
    );
    assert!(flag("process"));
    assert!(!flag("retry"));
    assert!(!flag("cancel"));
}

#[test]
fn the_log_only_sink_records_each_delivery_and_opens_no_connection() {
    // A relay that would take the message, and a provider connection that does not exist:
    // neither is reached, since the sink sends nothing (docs/ISSUES.md U55, first slice).
    let s = smtp(Script::default());
    let r = rig_cfg(
        true,
        Config {
            log_only: true,
            ..Config::default()
        },
    );
    let srv = r.server(&[("port", J::from(s.port))]);
    let id = r.send(&[]).unwrap();
    r.mail.process(T0).unwrap();
    let m = r.msg(&id);
    assert_eq!(text(&m, "status"), "sent", "{m:?}");
    assert_eq!(text(&m, "server_reply"), crate::LOGGED_ONLY);
    assert_eq!(text(&m, "body"), "", "the text is erased as after a send");
    // A test message is logged the same way.
    let out = r
        .call(
            &admin(),
            "mail_server.test_send",
            v(&[
                ("id", srv.clone().into()),
                ("test_recipient", "ada@example.org".into()),
            ]),
        )
        .unwrap();
    assert_eq!(out["last_test_outcome"], "sent", "{out}");
    let f = r.mail.backend.get(SERVER, &srv).unwrap().unwrap();
    assert!(
        text(&f, "last_test_result").contains(crate::LOGGED_ONLY),
        "{f:?}"
    );
    let seen = s.seen.lock().unwrap();
    assert!(
        seen.lines.is_empty(),
        "the relay was reached: {:?}",
        seen.lines
    );
}
