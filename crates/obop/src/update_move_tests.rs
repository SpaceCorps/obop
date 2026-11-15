//! One `update` that moves a governed state and changes other fields (D48), over HTTP against a
//! real `App` with the built-in invoicing module, whose posted invoice is frozen by its guard
//! (D77) and numbered from a gapless series: the call stores the edit and the move together or
//! nothing at all, an agent's call waits for a person, and the approved plan does all of it.

use crate::app::App;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use obop_commands::{Authenticator, Principal};
use serde_json::{Value as J, json};
use std::sync::Arc;
use tower::ServiceExt;

/// Three callers by token: a manager who may post, a clerk who may not, and the manager's agent.
struct Staff;

const MANAGER: &str = "manager-token";
const CLERK: &str = "clerk-token";
const AGENT: &str = "agent-token";

const GROUPS: [&str; 3] = ["base.user", "invoicing.user", "invoicing.manager"];

impl Authenticator for Staff {
    fn resolve(&self, token: &str) -> Option<Principal> {
        match token {
            MANAGER => Some(Principal::user("mia", &GROUPS)),
            CLERK => Some(Principal::user("sam", &GROUPS[..2])),
            AGENT => Some(Principal::agent("mia/agent", &GROUPS, &GROUPS)),
            _ => None,
        }
    }
    fn login(&self, _: &str, _: &str) -> Result<String, String> {
        Err("no login in this test".into())
    }
    fn issue_agent_token(&self, _: &Principal, _: &[String], _: &str) -> Result<String, String> {
        Err("no agent tokens in this test".into())
    }
}

struct Rig {
    router: axum::Router,
}

impl Rig {
    fn new() -> Self {
        let app = App::new();
        let router = obop_http::router(
            obop_http::HttpState::new(app.dispatcher.clone(), Arc::new(Staff)),
            None,
        );
        Self { router }
    }

    async fn post(&self, token: &str, path: &str, body: J) -> (u16, J) {
        let req = Request::post(path)
            .header("host", "inst.test")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let resp = self.router.clone().oneshot(req).await.unwrap();
        let status = resp.status().as_u16();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or(J::Null))
    }

    /// A draft invoice, with one line of 50.00 when `with_line`; the id.
    async fn invoice(&self, with_line: bool) -> String {
        let (s, b) = self
            .post(CLERK, "/api/invoice/create", json!({"currency": "EUR"}))
            .await;
        assert_eq!(s, 200, "{b}");
        let id = b["value"]["id"].as_str().unwrap().to_string();
        if with_line {
            let (s, b) = self
                .post(
                    CLERK,
                    "/api/invoice_line/create",
                    json!({"invoice_id": id, "description": "Work", "quantity": 1,
                           "unit_price_minor": 5000}),
                )
                .await;
            assert_eq!(s, 200, "{b}");
        }
        id
    }

    async fn stored(&self, id: &str) -> J {
        let (s, b) = self
            .post(CLERK, "/api/invoice/get", json!({ "id": id }))
            .await;
        assert_eq!(s, 200, "{b}");
        b["value"].clone()
    }

    /// The invoice is the draft it was made as: not posted, no number, the currency unchanged.
    async fn untouched(&self, id: &str) {
        let v = self.stored(id).await;
        assert_eq!(v["state"], "draft", "{v}");
        assert!(v["number"].is_null(), "{v}");
        assert_eq!(v["currency"], "EUR", "{v}");
    }
}

fn is_first_number(v: &J) -> bool {
    v["number"]
        .as_str()
        .is_some_and(|n| n.starts_with("INV-") && n.ends_with("-00001"))
}

#[tokio::test]
async fn posting_with_an_edit_in_one_update_stores_both() {
    let r = Rig::new();
    let id = r.invoice(true).await;
    let (s, b) = r
        .post(
            MANAGER,
            "/api/invoice/update",
            json!({"id": id, "state": "posted", "currency": "SEK"}),
        )
        .await;
    assert_eq!(s, 200, "{b}");
    // The answer is the invoice after all of it, and so is what is stored.
    for v in [b["value"].clone(), r.stored(&id).await] {
        assert_eq!(v["state"], "posted", "{v}");
        assert_eq!(v["currency"], "SEK", "{v}");
        assert!(is_first_number(&v), "{v}");
    }
    // From now on it is frozen.
    let (s, b) = r
        .post(
            MANAGER,
            "/api/invoice/update",
            json!({"id": id, "currency": "EUR"}),
        )
        .await;
    assert_eq!(
        (s, b["code"].as_str()),
        (409, Some("immutable_record")),
        "{b}"
    );
}

#[tokio::test]
async fn an_edit_the_invoice_refuses_leaves_it_a_draft_without_a_number() {
    let r = Rig::new();
    let id = r.invoice(true).await;
    // The number is the system's: the update refuses it, and the invoice is not posted.
    let (s, b) = r
        .post(
            MANAGER,
            "/api/invoice/update",
            json!({"id": id, "state": "posted", "number": "INV-2026-99999"}),
        )
        .await;
    assert_eq!(s, 400, "{b}");
    assert!(
        b["error"]
            .as_str()
            .is_some_and(|e| e.contains("is a number the system assigns")),
        "{b}"
    );
    r.untouched(&id).await;
    // A value of the wrong kind, refused before anything runs, as it always was.
    let (s, b) = r
        .post(
            MANAGER,
            "/api/invoice/update",
            json!({"id": id, "state": "posted", "currency": 7}),
        )
        .await;
    assert_eq!(s, 400, "{b}");
    r.untouched(&id).await;
    // The refused calls took no number of the gapless series.
    let (s, b) = r
        .post(
            MANAGER,
            "/api/invoice/update",
            json!({"id": id, "state": "posted"}),
        )
        .await;
    assert_eq!(s, 200, "{b}");
    assert!(is_first_number(&b["value"]), "{b}");
}

#[tokio::test]
async fn an_assertion_that_fails_stores_nothing_the_edit_included() {
    let r = Rig::new();
    // No line, so no total: posting is refused.
    let id = r.invoice(false).await;
    let (s, b) = r
        .post(
            MANAGER,
            "/api/invoice/update",
            json!({"id": id, "state": "posted", "currency": "SEK"}),
        )
        .await;
    assert_eq!(
        (s, b["code"].as_str()),
        (400, Some("assertion_failed")),
        "{b}"
    );
    r.untouched(&id).await;
    // A caller without the group of the action is refused the same way: nothing is stored.
    let id = r.invoice(true).await;
    let (s, b) = r
        .post(
            CLERK,
            "/api/invoice/update",
            json!({"id": id, "state": "posted", "currency": "SEK"}),
        )
        .await;
    assert_eq!(s, 400, "{b}");
    assert!(
        b["error"]
            .as_str()
            .is_some_and(|e| e.contains("invoicing.manager")),
        "{b}"
    );
    r.untouched(&id).await;
}

#[tokio::test]
async fn an_agents_update_waits_for_a_person_and_the_approved_plan_does_all_of_it() {
    let r = Rig::new();
    let id = r.invoice(true).await;
    let (s, b) = r
        .post(
            AGENT,
            "/api/invoice/update",
            json!({"id": id, "state": "posted", "currency": "SEK"}),
        )
        .await;
    assert_eq!(s, 202, "{b}");
    let plan = b["plan_id"].as_str().unwrap().to_string();
    // Nothing has changed, not even the ordinary field.
    r.untouched(&id).await;
    let (s, b) = r
        .post(MANAGER, &format!("/api/approve/{plan}"), json!({}))
        .await;
    assert_eq!(s, 200, "{b}");
    let v = r.stored(&id).await;
    assert_eq!(v["state"], "posted", "{v}");
    assert_eq!(v["currency"], "SEK", "{v}");
    assert!(is_first_number(&v), "{v}");
}

#[tokio::test]
async fn posting_alone_by_update_works_as_before() {
    let r = Rig::new();
    let id = r.invoice(true).await;
    let (s, b) = r
        .post(
            MANAGER,
            "/api/invoice/update",
            json!({"id": id, "state": "posted"}),
        )
        .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["value"]["state"], "posted", "{b}");
    assert!(is_first_number(&b["value"]), "{b}");
    assert_eq!(b["value"]["currency"], "EUR", "{b}");
    // The record sent back whole with a new state (a form): what is as stored is not an edit.
    let id = r.invoice(true).await;
    let mut whole = r.stored(&id).await;
    whole["state"] = json!("posted");
    let (s, b) = r.post(MANAGER, "/api/invoice/update", whole).await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["value"]["state"], "posted", "{b}");
}
