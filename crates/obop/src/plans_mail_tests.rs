//! The approvals inbox (D97) and the built-in module `mail`: an agent only proposes a change of a
//! mail server and a message, so both arrive in the inbox. The password of a mail server is
//! sealed and write-only, the text of a message is sealed and read by nobody, and an address is
//! read only by who holds `mail.admin`. These tests go through every door of the inbox on the real
//! module, with real sign-in: the answer the toast is made of, the list, the detail and its change
//! view, the agent's `plan.get`, the receipt (what a decided plan answers) and the security log.

use crate::app::App;
use crate::mfa_tests::{PW, post, token};
use obop_auth::{Auth, AuthConfig};
use obop_commands::Authenticator;
use serde_json::{Value as J, json};
use std::sync::Arc;

/// The password the agent proposes; it must never come back.
const NEW_PASSWORD: &str = "pw-new-5d1c0e7a";
/// The password the person stored first.
const OLD_PASSWORD: &str = "pw-old-93b2f4c8";
/// The text of the proposed message; read by nobody.
const BODY: &str = "Body-text-8e4a1f: the invoice is attached";
/// An address and a subject: read only by a holder of `mail.admin`.
const TO: &str = "erika@example.test";
const SUBJECT: &str = "Invoice 1042 for Erika";

struct World {
    r: axum::Router,
    app: App,
}

fn world() -> World {
    let app = App::new();
    let auth = Arc::new(Auth::with_test_config(
        app.store.backend(),
        AuthConfig::insecure_dev(),
    ));
    // ada: the administrator. mo: the mail administrator. sam: may queue mail, reads none.
    auth.create_user("ada", PW, &app.all_groups()).unwrap();
    auth.create_user(
        "mo",
        PW,
        &["mail.admin".to_string(), "mail.sender".to_string()],
    )
    .unwrap();
    auth.create_user("sam", PW, &["mail.sender".to_string()])
        .unwrap();
    app.register_accounts(auth.clone() as Arc<dyn Authenticator>);
    let r = obop_http::router(
        obop_http::HttpState::new(app.dispatcher.clone(), auth.clone()),
        None,
    );
    World { r, app }
}

/// Every value that no door may show to anyone.
fn assert_no_sealed_value(door: &str, text: &str) {
    for v in [NEW_PASSWORD, OLD_PASSWORD, BODY] {
        assert!(!text.contains(v), "{door} shows a sealed value {v}: {text}");
    }
}

fn assert_no_address(door: &str, text: &str) {
    for v in [TO, SUBJECT] {
        assert!(!text.contains(v), "{door} shows {v}: {text}");
    }
}

struct Proposed {
    server: String,
    change: String,
    message: String,
    agent: String,
    answers: Vec<J>,
}

/// ada stores a mail server; her agent proposes a new password for it and a message.
async fn propose(w: &World) -> Proposed {
    let r = &w.r;
    let ada = token(r, "ada").await;
    let (s, b) = post(
        r,
        Some(&ada),
        "/api/mail_server/create",
        json!({
            "name": "Relay", "kind": "smtp", "enabled": true, "is_default": true,
            "host": "relay.example.test", "port": 587, "security": "starttls",
            "username": "relay-user", "password": OLD_PASSWORD,
            "from_address": "billing@example.test",
        }),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    let server = b["value"]["id"].as_str().unwrap().to_string();
    let (s, b) = post(
        r,
        Some(&ada),
        "/api/auth/agent-token",
        json!({"scopes": ["mail.admin", "mail.sender"], "label": "bot"}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    let agent = b["token"].as_str().unwrap().to_string();
    let (s, change_answer) = post(
        r,
        Some(&agent),
        "/api/mail_server/update",
        json!({"id": server, "password": NEW_PASSWORD}),
    )
    .await;
    assert_eq!(s, 202, "{change_answer}");
    let (s, send_answer) = post(
        r,
        Some(&agent),
        "/api/outbound_mail/send",
        json!({"to": TO, "subject": SUBJECT, "body": BODY}),
    )
    .await;
    assert_eq!(s, 202, "{send_answer}");
    Proposed {
        server,
        change: change_answer["plan_id"].as_str().unwrap().to_string(),
        message: send_answer["plan_id"].as_str().unwrap().to_string(),
        agent,
        answers: vec![change_answer, send_answer],
    }
}

/// The change of `field` in a detail, if the change view lists it.
fn change_of<'a>(detail: &'a J, field: &str) -> Option<&'a J> {
    detail["changes"]
        .as_array()?
        .iter()
        .find(|c| c["field"] == field)
}

#[tokio::test]
async fn mail_the_answer_a_toast_is_made_of_names_the_plan_only() {
    let w = world();
    let p = propose(&w).await;
    for a in &p.answers {
        let text = a.to_string();
        assert_no_sealed_value("the 202 answer", &text);
        assert_no_address("the 202 answer", &text);
    }
}

#[tokio::test]
async fn mail_the_list_and_the_count_show_no_value_and_each_person_only_what_they_could_decide() {
    let w = world();
    let p = propose(&w).await;
    for (who, want) in [
        ("ada", vec![&p.change, &p.message]),
        ("mo", vec![&p.change, &p.message]),
        // sam may queue mail and not change a server: the message only.
        ("sam", vec![&p.message]),
    ] {
        let t = token(&w.r, who).await;
        let (s, b) = post(&w.r, Some(&t), "/api/plan/list", json!({})).await;
        assert_eq!(s, 200, "{b}");
        let mut ids: Vec<&str> = b["value"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect();
        ids.sort_unstable();
        let mut want: Vec<&str> = want.iter().map(|s| s.as_str()).collect();
        want.sort_unstable();
        assert_eq!(ids, want, "{who}");
        let text = b.to_string();
        assert_no_sealed_value("the list", &text);
        assert_no_address("the list", &text);
        let (s, b) = post(&w.r, Some(&t), "/api/plan/count", json!({})).await;
        assert_eq!(s, 200, "{b}");
        assert_eq!(b["value"]["waiting"], want.len(), "{who}");
    }
}

#[tokio::test]
async fn mail_the_detail_and_its_change_view_hide_the_password_and_what_a_person_may_not_read() {
    let w = world();
    let p = propose(&w).await;
    for who in ["ada", "mo"] {
        let t = token(&w.r, who).await;
        let (s, d) = post(&w.r, Some(&t), "/api/plan/get", json!({"id": p.change})).await;
        assert_eq!(s, 200, "{d}");
        let pw = change_of(&d["value"], "password").expect("the password is listed as changed");
        assert_eq!(pw["hidden"], true, "{who}: {pw}");
        assert!(pw["to"].is_null() && pw["from"].is_null(), "{who}: {pw}");
        // The record as the person reads it: the stored password is not in it, sealed or not.
        let stored = &d["value"]["record"]["password"];
        assert!(
            stored.is_null() || stored == "" || stored.as_str().is_some_and(|s| s.contains('*')),
            "{who}: {stored}"
        );
        assert_no_sealed_value(&format!("the server change for {who}"), &d.to_string());
    }
    // The text: hidden from a person without `mail.system` (the group of the sealed field).
    for who in ["mo", "sam"] {
        let t = token(&w.r, who).await;
        let (s, d) = post(&w.r, Some(&t), "/api/plan/get", json!({"id": p.message})).await;
        assert_eq!(s, 200, "{d}");
        let body = change_of(&d["value"], "body").expect("the text is listed as set");
        assert_eq!(body["hidden"], true, "{who}: {body}");
        assert!(body["to"].is_null(), "{who}: {body}");
        assert_no_sealed_value(&format!("the message for {who}"), &d.to_string());
    }
    // The address and the subject: a holder of mail.admin reads them, sam does not.
    let mo = token(&w.r, "mo").await;
    let (_, d) = post(&w.r, Some(&mo), "/api/plan/get", json!({"id": p.message})).await;
    assert_eq!(change_of(&d["value"], "to").unwrap()["to"], TO, "{d}");
    assert_eq!(change_of(&d["value"], "subject").unwrap()["to"], SUBJECT);
    let sam = token(&w.r, "sam").await;
    let (_, d) = post(&w.r, Some(&sam), "/api/plan/get", json!({"id": p.message})).await;
    for f in ["to", "subject"] {
        assert_eq!(
            change_of(&d["value"], f).unwrap()["hidden"],
            true,
            "{f}: {d}"
        );
    }
    assert_no_address("the message for sam", &d.to_string());
    // sam may not change a server: that plan does not exist for him.
    let (s, _) = post(&w.r, Some(&sam), "/api/plan/get", json!({"id": p.change})).await;
    assert_eq!(s, 404);
}

#[tokio::test]
async fn mail_the_agent_reads_back_no_value_through_plan_get() {
    let w = world();
    let p = propose(&w).await;
    for plan in [&p.change, &p.message] {
        let (s, b) = post(&w.r, Some(&p.agent), "/api/plan/get", json!({"id": plan})).await;
        assert_eq!(s, 200, "{b}");
        assert_eq!(b["value"]["status"], "waiting");
        let text = b.to_string();
        assert_no_sealed_value("the agent's plan.get", &text);
        assert_no_address("the agent's plan.get", &text);
    }
}

#[tokio::test]
async fn mail_the_receipt_and_the_security_log_hold_no_value_after_the_decisions() {
    let w = world();
    let p = propose(&w).await;
    let mo = token(&w.r, "mo").await;
    let (s, b) = post(
        &w.r,
        Some(&mo),
        "/api/plan/approve",
        json!({"id": p.change}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    assert_no_sealed_value("the approval's answer", &b.to_string());
    let (s, b) = post(
        &w.r,
        Some(&mo),
        "/api/plan/reject",
        json!({"id": p.message, "reason": "Not to Erika"}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    // The approved change ran: the server says a password is stored, and returns none.
    let (_, srv) = post(
        &w.r,
        Some(&mo),
        "/api/mail_server/get",
        json!({"id": p.server}),
    )
    .await;
    assert_eq!(srv["value"]["password_set"], true, "{srv}");
    assert_no_sealed_value("the server after the approval", &srv.to_string());
    // What a decided plan answers is made of its receipt.
    for t in [&mo, &p.agent] {
        for plan in [&p.change, &p.message] {
            let (s, b) = post(&w.r, Some(t), "/api/plan/get", json!({"id": plan})).await;
            assert_eq!(s, 200, "{b}");
            let text = b.to_string();
            assert_no_sealed_value("a decided plan", &text);
            assert_no_address("a decided plan", &text);
        }
        let (_, b) = post(&w.r, Some(t), "/api/plan/list", json!({"state": "decided"})).await;
        assert_no_sealed_value("the Decided list", &b.to_string());
    }
    let lines: Vec<J> = w
        .app
        .store
        .backend()
        .list("security_event")
        .unwrap()
        .into_iter()
        .map(|(_, f)| J::Object(f))
        .collect();
    let kinds: Vec<&str> = lines.iter().filter_map(|l| l["kind"].as_str()).collect();
    for k in ["plan_proposed", "plan_approved", "plan_rejected"] {
        assert!(kinds.contains(&k), "{k}: {kinds:?}");
    }
    let text = J::Array(lines.clone()).to_string();
    assert_no_sealed_value("the security log", &text);
    assert_no_address("the security log", &text);
    assert!(!text.contains("Not to Erika"), "the reason: {text}");
}
