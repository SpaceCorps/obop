//! Regressions of the review of 2026-11-18: long encoded headers, agents and mail servers, the
//! flow rules of a provider send, the answer of `process`, and test messages that are not sent
//! again.

use super::*;

/// The subject of a message as the stand-in saw it: unfolded and decoded.
fn decoded_subject(data: &str) -> String {
    let head = data.split_once("\n\n").unwrap().0.replace("\n ", " ");
    let line = head
        .lines()
        .find_map(|l| l.strip_prefix("Subject: "))
        .unwrap()
        .to_string();
    line.split(' ')
        .map(|w| {
            let b = w
                .strip_prefix("=?UTF-8?B?")
                .and_then(|w| w.strip_suffix("?="))
                .unwrap_or_else(|| panic!("not an encoded word: {w}"));
            String::from_utf8(base64::engine::general_purpose::STANDARD.decode(b).unwrap()).unwrap()
        })
        .collect()
}

#[test]
fn a_long_non_ascii_subject_and_sender_name_go_out_folded() {
    let r = rig();
    let s = smtp(Script::default());
    let name = "Nordlicht Werkzeuge GmbH Buchhaltung und Rechnungsstelle Süd";
    let subject = "Ihre Rechnung RE-2026-0042 von Nordlicht – fällig am 30.11.";
    r.server(&[("port", J::from(s.port)), ("from_name", name.into())]);
    let id = r.send(&[("subject", subject.into())]).unwrap();
    assert_eq!(r.mail.process(T0).unwrap().sent, 1, "{:?}", r.msg(&id));
    let data = s.seen.lock().unwrap().data[0].clone();
    assert_eq!(decoded_subject(&data), subject);
    let head = data.split_once("\n\n").unwrap().0;
    assert!(head.lines().all(|l| l.len() <= 78), "{head}");
    assert!(head.contains("\n =?UTF-8?B?"), "folded: {head}");
}

#[test]
fn a_message_with_a_header_that_cannot_be_built_never_reaches_the_relay() {
    // A line break that got past the checks is refused before the relay is asked for anything.
    let msg = mime::Message {
        from_name: String::new(),
        from: "a@example.com".into(),
        to: vec!["b@example.com".into()],
        cc: vec![],
        bcc: vec![],
        reply_to: None,
        subject: "Hallo\r\nBcc: x@example.com".into(),
        text: "x".into(),
        message_id: "1@example.com".into(),
        date: T0,
    };
    assert!(msg.to_bytes().is_err());
    let r = rig();
    let s = smtp(Script::default());
    let srv = r.server(&[("port", J::from(s.port))]);
    let sf = r.mail.backend.get(SERVER, &srv).unwrap().unwrap();
    assert!(matches!(
        r.mail.deliver(&sf, &msg),
        Err(Failure::Permanent(_))
    ));
    assert!(
        s.seen.lock().unwrap().lines.is_empty(),
        "the relay was contacted"
    );
}

fn agent() -> Principal {
    Principal::agent("ada/agent", &[ADMIN, SENDER], &[ADMIN, SENDER])
}

#[test]
fn an_agent_only_proposes_a_change_to_a_mail_server() {
    let r = rig();
    let srv = r.server(&[
        ("security", "tls".into()),
        ("username", "relay-user".into()),
        ("password", PASSWORD.into()),
    ]);
    let before = r.mail.backend.get(SERVER, &srv).unwrap().unwrap();
    let update = r.call(
        &agent(),
        "mail_server.update",
        v(&[
            ("id", srv.clone().into()),
            ("host", "evil.example.com".into()),
        ]),
    );
    assert!(
        format!("{update:?}").contains("ApprovalRequired"),
        "{update:?}"
    );
    assert_eq!(r.mail.backend.get(SERVER, &srv).unwrap().unwrap(), before);
    let create = r.call(
        &agent(),
        "mail_server.create",
        v(&[
            ("name", "Other".into()),
            ("kind", "smtp".into()),
            ("host", "smtp.example.com".into()),
            ("security", "tls".into()),
            ("from_address", "a@example.com".into()),
        ]),
    );
    assert!(
        format!("{create:?}").contains("ApprovalRequired"),
        "{create:?}"
    );
    let delete = r.call(
        &agent(),
        "mail_server.delete",
        v(&[("id", srv.clone().into())]),
    );
    assert!(
        format!("{delete:?}").contains("ApprovalRequired"),
        "{delete:?}"
    );
    assert!(r.mail.backend.get(SERVER, &srv).unwrap().is_some());
}

#[test]
fn a_new_host_without_a_new_password_forgets_the_stored_one() {
    let r = rig();
    let srv = r.server(&[
        ("host", "smtp.example.com".into()),
        ("security", "tls".into()),
        ("username", "relay-user".into()),
        ("password", PASSWORD.into()),
    ]);
    // The same host (as a form sends it) with another setting keeps it.
    r.call(
        &admin(),
        "mail_server.update",
        v(&[
            ("id", srv.clone().into()),
            ("host", "smtp.example.com".into()),
            ("from_name", "N".into()),
        ]),
    )
    .unwrap();
    let f = r.mail.backend.get(SERVER, &srv).unwrap().unwrap();
    assert_eq!(f.get("password_set"), Some(&J::Bool(true)));
    r.call(
        &admin(),
        "mail_server.update",
        v(&[
            ("id", srv.clone().into()),
            ("host", "smtp.other.example".into()),
        ]),
    )
    .unwrap();
    let f = r.mail.backend.get(SERVER, &srv).unwrap().unwrap();
    assert_eq!(text(&f, "password"), "");
    assert_eq!(f.get("password_set"), Some(&J::Bool(false)));
    // A new password given with the new host is kept.
    r.call(
        &admin(),
        "mail_server.update",
        v(&[
            ("id", srv.clone().into()),
            ("host", "smtp.third.example".into()),
            ("password", PASSWORD.into()),
        ]),
    )
    .unwrap();
    let f = r.mail.backend.get(SERVER, &srv).unwrap().unwrap();
    assert_eq!(f.get("password_set"), Some(&J::Bool(true)));
}

fn flows(r: &Rig, conn: &str) -> Vec<Fields> {
    let mut rows = r
        .mail
        .backend
        .find("data_flow_log", "connection", &J::from(conn))
        .unwrap();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows.into_iter().map(|(_, f)| f).collect()
}

#[test]
fn a_copy_recipient_off_the_allowlist_sends_nothing_and_the_refusal_is_logged() {
    let r = rig();
    let (port, seen) = http(200, "{}");
    r.mail
        .use_api_base("postmark", &format!("http://127.0.0.1:{port}"))
        .unwrap();
    let conn = connection(
        &r,
        "postmark",
        &["to", "subject", "body"],
        r#"{"api_key":"pm"}"#,
        "{}",
    );
    r.server(&[
        ("kind", "provider".into()),
        ("connection", conn.clone().into()),
    ]);
    for (k, a) in [
        ("cc", "chef@example.org"),
        ("bcc", "archiv@nordlicht.example"),
        ("reply_to", "fragen@nordlicht.example"),
    ] {
        let id = r.send(&[(k, a.into())]).unwrap();
        assert_eq!(r.mail.process(T0).unwrap().failed, 1, "{k}");
        assert!(
            text(&r.msg(&id), "last_error").contains(k),
            "{:?}",
            r.msg(&id)
        );
    }
    assert!(seen.lock().unwrap().is_empty());
    let log = flows(&r, &conn);
    assert_eq!(log.len(), 3);
    assert!(
        log.iter().all(|f| text(f, "outcome") == "refused"),
        "{log:?}"
    );
    // On the allowlist, it leaves and the log names it.
    let conn = connection(
        &r,
        "postmark",
        &["to", "cc", "subject", "body"],
        r#"{"api_key":"pm"}"#,
        "{}",
    );
    let srv = r.server(&[
        ("name", "B".into()),
        ("kind", "provider".into()),
        ("connection", conn.clone().into()),
    ]);
    r.send(&[("server", srv.into()), ("cc", "chef@example.org".into())])
        .unwrap();
    assert_eq!(r.mail.process(T0).unwrap().sent, 1);
    let log = flows(&r, &conn);
    assert_eq!(log.len(), 1);
    assert_eq!(text(&log[0], "outcome"), "released");
    assert_eq!(text(&log[0], "fields_sent"), "to,cc,subject,body");
}

#[test]
fn a_provider_send_is_logged_before_it_leaves_its_failure_is_logged_and_the_caps_hold() {
    let r = rig();
    let (port, seen) = http(400, r#"{"message":"bad"}"#);
    r.mail
        .use_api_base("sendgrid", &format!("http://127.0.0.1:{port}"))
        .unwrap();
    let conn = connection_with(
        &r,
        "sendgrid",
        &["to", "subject", "body"],
        r#"{"api_key":"sg"}"#,
        "{}",
        &[("rate_limit_per_hour", J::from(1))],
    );
    r.server(&[
        ("kind", "provider".into()),
        ("connection", conn.clone().into()),
    ]);
    let a = r.send(&[]).unwrap();
    assert_eq!(r.mail.process(T0).unwrap().failed, 1, "{:?}", r.msg(&a));
    let log = flows(&r, &conn);
    let outcomes: Vec<&str> = log.iter().map(|f| text(f, "outcome")).collect();
    assert_eq!(outcomes, ["released", "rejected"], "{log:?}");
    assert!(text(&log[1], "reason").contains("400"), "{log:?}");
    // One flow an hour: the next one waits, and nothing reaches the provider.
    let b = r.send(&[]).unwrap();
    assert_eq!(r.mail.process(T0).unwrap().retrying, 1, "{:?}", r.msg(&b));
    assert!(
        text(&r.msg(&b), "last_error").contains("rate"),
        "{:?}",
        r.msg(&b)
    );
    assert_eq!(seen.lock().unwrap().len(), 1);
    assert_eq!(text(flows(&r, &conn).last().unwrap(), "outcome"), "refused");
}

#[test]
fn process_answers_each_message_with_its_new_status() {
    let r = rig();
    let s = smtp(Script::default());
    r.server(&[("port", J::from(s.port))]);
    let id = r.send(&[]).unwrap();
    let mut vault = Vault::new();
    let resp =
        r.d.call(
            Surface::Rest,
            &admin(),
            &mut vault,
            "outbound_mail.process",
            &v(&[]),
        )
        .unwrap();
    assert!(resp.denied.is_empty(), "{:?}", resp.denied);
    assert_eq!(resp.value.to_json(), json!([{"id": id, "status": "sent"}]));
}

#[test]
fn a_test_message_is_not_offered_again() {
    let r = rig();
    let s = smtp(Script {
        rcpt: 550,
        ..Script::default()
    });
    let srv = r.server(&[("port", J::from(s.port))]);
    let _ = r.call(
        &admin(),
        "mail_server.test_send",
        v(&[
            ("id", srv.into()),
            ("test_recipient", "ada@example.org".into()),
        ]),
    );
    let log = r
        .mail
        .backend
        .find(OUTBOUND, "kind", &J::from("test"))
        .unwrap();
    assert_eq!(text(&log[0].1, "status"), "failed");
    let e = r
        .call(
            &admin(),
            "outbound_mail.retry",
            v(&[("id", log[0].0.clone().into())]),
        )
        .unwrap_err();
    assert!(e.contains("Send test"), "{e}");
}
