//! NEW-EXP (owner ruling 2026-11-17): the person who made an expense claim may not approve it
//! unless the company setting `expenses.allow_self_approval` is on.

use crate::app::App;
use obop_commands::{ADMIN_GROUP, CallError, Principal, Surface, Value, Vault};
use obop_pkg::{Bundled, Marketplace};
use serde_json::{Value as J, json};

fn call(a: &App, p: &Principal, id: &str, input: J) -> Result<J, CallError> {
    a.dispatcher
        .call(
            Surface::Rest,
            p,
            &mut Vault::new(),
            id,
            &Value::from_json(&input),
        )
        .map(|r| r.value.to_json())
}

fn manager(name: &str) -> Principal {
    Principal::user(name, &["expenses.user", "expenses.manager"])
}

fn app() -> App {
    let app = App::new();
    app.installer
        .install(&Bundled.fetch("expenses").unwrap(), "marketplace", "test")
        .unwrap();
    app
}

/// A submitted claim made by `who`.
fn claim(a: &App, who: &str) -> String {
    let p = manager(who);
    let r = call(
        a,
        &p,
        "expenses_expense.create",
        json!({"description": "Train", "amount": 4200, "category": "travel"}),
    )
    .unwrap();
    let id = r["id"].as_str().unwrap().to_string();
    call(a, &p, "expenses_expense.submit", json!({"id": id})).unwrap();
    id
}

fn state(a: &App, id: &str) -> String {
    a.store
        .backend()
        .get("expenses_expense", id)
        .unwrap()
        .unwrap()["state"]
        .as_str()
        .unwrap()
        .to_string()
}

fn refused(r: Result<J, CallError>) -> String {
    match r {
        Err(CallError::Handler(m)) => m,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn the_maker_of_a_claim_cannot_approve_it_by_default() {
    let a = app();
    let id = claim(&a, "alice");
    let e = refused(call(
        &a,
        &manager("alice"),
        "expenses_expense.approve",
        json!({"id": id}),
    ));
    assert!(e.contains("someone else has to approve it"), "{e}");
    assert_eq!(state(&a, &id), "submitted");
    // Through an update of the state (it runs the action) the same.
    let e = refused(call(
        &a,
        &manager("alice"),
        "expenses_expense.update",
        json!({"id": id, "state": "approved"}),
    ));
    assert!(e.contains("someone else has to approve it"), "{e}");
    assert_eq!(state(&a, &id), "submitted");
    // Another manager approves it.
    call(
        &a,
        &manager("bob"),
        "expenses_expense.approve",
        json!({"id": id}),
    )
    .unwrap();
    assert_eq!(state(&a, &id), "approved");
}

#[test]
fn an_approved_plan_is_refused_when_the_maker_approves_it() {
    let a = app();
    let id = claim(&a, "alice");
    let agent = Principal::agent(
        "carol/agent",
        &["expenses.user", "expenses.manager"],
        &["expenses.user", "expenses.manager"],
    );
    let Err(CallError::ApprovalRequired { plan_id }) =
        call(&a, &agent, "expenses_expense.approve", json!({"id": id}))
    else {
        panic!("an agent proposes")
    };
    let e = match a
        .dispatcher
        .approve(&manager("alice"), &mut Vault::new(), &plan_id)
    {
        Err(CallError::Handler(m)) => m,
        other => panic!("expected a refusal, got {other:?}"),
    };
    assert!(e.contains("someone else has to approve it"), "{e}");
    assert_eq!(state(&a, &id), "submitted");
}

#[test]
fn the_company_setting_allows_self_approval_and_only_an_administrator_changes_it() {
    let a = app();
    let id = claim(&a, "alice");
    let get = call(&a, &manager("alice"), "expenses_setting.get", json!({})).unwrap();
    assert_eq!(get["allow_self_approval"], false);
    // A manager may not switch it on for themselves.
    assert!(matches!(
        call(
            &a,
            &manager("alice"),
            "expenses_setting.update",
            json!({"allow_self_approval": true}),
        ),
        Err(CallError::Forbidden(_))
    ));
    let root = Principal::user("root", &[ADMIN_GROUP]);
    let set = call(
        &a,
        &root,
        "expenses_setting.update",
        json!({"allow_self_approval": true}),
    )
    .unwrap();
    assert_eq!(set["allow_self_approval"], true);
    // Stored with the other instance settings.
    let stored: J = serde_json::from_str(&a.installer.try_settings().unwrap()).unwrap();
    assert_eq!(stored["expenses"]["allow_self_approval"], true);
    call(
        &a,
        &manager("alice"),
        "expenses_expense.approve",
        json!({"id": id}),
    )
    .unwrap();
    assert_eq!(state(&a, &id), "approved");
}

#[test]
fn the_refusal_is_worded_in_the_language_of_the_request() {
    for lang in obop_commands::locale::LANGUAGES {
        let t = crate::expense_policy::refusal_text(Some(lang));
        assert!(!t.is_empty());
        if lang != "en" {
            assert_ne!(t, crate::expense_policy::refusal_text(None), "{lang}");
        }
    }
    let a = app();
    let id = claim(&a, "alice");
    let _s = obop_commands::locale::scope(Some("de"));
    let e = refused(call(
        &a,
        &manager("alice"),
        "expenses_expense.approve",
        json!({"id": id}),
    ));
    assert!(e.contains("jemand anderes"), "{e}");
}
