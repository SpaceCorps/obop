//! The tables the real program asks the storage for while it runs, against the names the installer
//! keeps from a package. `internal_tables_scan_tests` reads the source and cannot see a name that is
//! built at run time or handed on through a variable; this test runs the real `App` over a memory
//! store behind a recording backend (`obop_storage::testing::Flaky::models_asked`: every model that
//! any call named, a read or a refused call included, and every model of a batch) through a few
//! flows. Each of those names must be reserved by name (`INTERNAL_TABLES`, `RESERVED_MODELS`) or be
//! a model of a module of the `App` (the installer refuses those because their module owns them).
//!
//! **What it sees.** The models named by the start of the `App` (the key ring, the connectors'
//! sealed keys, the package records) and by these flows: the demo data and the numbers of its
//! documents (`sequence_counter`), an account created and signed in (the session is an `auth_token`
//! row), a connection with its provider policy, a bundled package installed (its models are the
//! package's while it is installed), a bulk create with an idempotency key, a change of the instance
//! settings, and the registration of an OAuth client over the HTTP router. That reaches the tables
//! of [`MUST_REACH`] and some others; `reached` says how many of the names of `INTERNAL_TABLES`
//! the flows asked for, and the figure is in docs/ISSUES.md N17.
//! **What it does not see.** A table that none of these flows asks for: the backup and restore
//! catalog, the migration ledger, single sign-on, the forum and the status page (they run in other
//! processes, over other crates' stores), a webhook, a payment, most of the connectors' tables. For
//! those the protection is the scan of the source, and the scan's blind spots are in docs/ISSUES.md
//! N17: a name that this test does not reach and the scan cannot read is not seen by either.

use crate::app::{App, dev_principal};
use axum::{body::Body, http::Request};
use obop_auth::Auth;
use obop_commands::{
    ADMIN_GROUP, Authenticator, CallError, Response, Store, Surface, Value, Vault,
};
use obop_pkg::{Bundled, Marketplace};
use obop_registry::{INTERNAL_TABLES, RESERVED_MODELS};
use obop_storage::testing::Flaky;
use serde_json::json;
use std::collections::BTreeSet;
use std::sync::Arc;
use tower::ServiceExt;

/// Tables of the program that the flows must ask the storage for, so that a green test means
/// something: the document counters, the accounts and their sessions, the package records, the
/// security log, the idempotency keys of the bulk create, the instance settings and the OAuth
/// clients.
const MUST_REACH: [&str; 9] = [
    "sequence_counter",
    "auth_user",
    "auth_token",
    "obop_package",
    "security_event",
    "connector_key",
    "bulk_idempotency",
    "obop_setting",
    "oauth_client",
];

/// A command as the development administrator, the way the HTTP surface would call it.
fn admin_call(app: &App, command: &str, input: serde_json::Value) -> Result<Response, CallError> {
    let admin = dev_principal("dev-admin").expect("the development administrator");
    let input = Value::from_json(&input);
    app.dispatcher
        .call(Surface::Rest, &admin, &mut Vault::new(), command, &input)
}

/// The flows, over one `App`. Each step must work: a flow that quietly did nothing would leave the
/// test green for the wrong reason (the names it must reach are asserted by the caller).
async fn run_flows(app: &App) {
    // The demo data, and the numbers of the documents (SO-00001): the counters of the sequences.
    app.seed_demo();
    // An account and a sign-in.
    let auth = Arc::new(Auth::new(app.store.backend()));
    auth.create_user(
        "recorder-admin",
        "a password of some length",
        &[ADMIN_GROUP.to_string()],
    )
    .expect("create the account");
    auth.login("recorder-admin", "a password of some length")
        .expect("sign in");
    // A connection with its provider policy, and several rows at once with a key that makes a
    // repeat harmless (the idempotency table).
    for (command, input) in [
        (
            "provider_policy.create",
            json!({"provider": "webhook_in", "enabled": true, "users_may_use": true,
                   "dpa_recorded": true}),
        ),
        (
            "connection.create",
            json!({"name": "Form", "provider": "webhook_in",
                   "config": json!({"target_model": "lead",
                                    "mapping": json!({"title": "title"}).to_string()}).to_string(),
                   "credentials": json!({"signing_secret": "recorder-signing-secret-0001"})
                       .to_string(),
                   "field_allowlist": json!({"lead": ["title"]}).to_string()}),
        ),
        (
            "party.bulk_create",
            json!({"rows": [{"name": "Recorded one"}, {"name": "Recorded two"}],
                   "idempotency_key": "recorder-key-0001"}),
        ),
    ] {
        admin_call(app, command, input).unwrap_or_else(|e| panic!("{command}: {e:?}"));
    }
    // A bundled package, with the models it declares.
    let text = Bundled.fetch("website").expect("the bundled package");
    app.installer
        .install(&text, "marketplace", "root")
        .expect("install a bundled package");
    // The instance settings.
    app.installer
        .set_settings(r#"{"theme":{"name":"recorder"}}"#)
        .expect("store the settings");
    // An OAuth client registers itself over the HTTP router.
    let router = obop_http::router(
        obop_http::HttpState::new(app.dispatcher.clone(), auth.clone()),
        None,
    );
    let request = Request::builder()
        .method("POST")
        .uri("/oauth/register")
        .header("host", "inst.test")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"client_name": "Recorder", "token_endpoint_auth_method": "none",
                   "redirect_uris": ["https://claude.ai/api/mcp/auth_callback"]})
            .to_string(),
        ))
        .unwrap();
    let answer = router.oneshot(request).await.unwrap();
    assert_eq!(answer.status().as_u16(), 201, "the client registers");
}

/// A fresh `App` over a recording backend, the recorder, and what it had seen once the `App` had
/// started.
fn recorded_app() -> (Arc<Flaky>, App, BTreeSet<String>) {
    let (recorder, backend) = Flaky::memory();
    let app = App::with_store(Store::over(backend));
    let at_start = recorder.models_asked();
    (recorder, app, at_start)
}

/// The names of `INTERNAL_TABLES` that `asked` holds.
fn reached(asked: &BTreeSet<String>) -> Vec<&'static str> {
    INTERNAL_TABLES
        .iter()
        .copied()
        .filter(|n| asked.contains(*n))
        .collect()
}

/// The names in `asked` that nothing keeps a package from declaring: not in the two lists of the
/// registry and not a model of a module of `app`.
fn unknown(app: &App, asked: &BTreeSet<String>) -> Vec<String> {
    let known: BTreeSet<String> = INTERNAL_TABLES
        .iter()
        .chain(RESERVED_MODELS.iter())
        .map(|n| n.to_string())
        .chain(app.dispatcher.model_names())
        .collect();
    asked
        .iter()
        .filter(|n| !known.contains(*n))
        .cloned()
        .collect()
}

#[tokio::test]
async fn every_model_the_running_program_asks_for_is_reserved_or_belongs_to_a_module() {
    let (recorder, app, at_start) = recorded_app();
    assert!(
        at_start.contains("obop_package") && at_start.contains("connector_key"),
        "the recorder sees the start of the App: {at_start:?}"
    );
    run_flows(&app).await;
    let asked = recorder.models_asked();
    let unknown = unknown(&app, &asked);
    assert!(
        unknown.is_empty(),
        "the running program asks the storage for a model that no package is kept from declaring. \
         Add it to obop_registry::INTERNAL_TABLES:\n{unknown:?}"
    );
    // The flows reached what they are there for.
    for name in MUST_REACH {
        assert!(
            asked.contains(name),
            "the flows asked for no row of {name}: {asked:?}"
        );
    }
    let internal = reached(&asked);
    assert!(
        internal.len() >= MUST_REACH.len(),
        "{} of the {} internal tables: {internal:?}",
        internal.len(),
        INTERNAL_TABLES.len()
    );
    assert!(
        asked.len() > at_start.len() + 10,
        "the flows add to what the start of the App asked for: {at_start:?} -> {asked:?}"
    );
}

/// The comparison is worth something only if it reports a table that is on no list, and leaves a
/// model of a module and a reserved name alone.
#[tokio::test]
async fn a_table_the_program_asks_for_and_no_list_has_is_reported() {
    let (recorder, app, _) = recorded_app();
    run_flows(&app).await;
    let mut asked = recorder.models_asked();
    assert!(unknown(&app, &asked).is_empty());
    // A name the program does not ask for, standing in for a table built at run time.
    asked.insert("zz_table_of_the_program".to_string());
    assert_eq!(unknown(&app, &asked), ["zz_table_of_the_program"]);
    // A name that is a model of a module, or reserved, is not reported.
    asked.remove("zz_table_of_the_program");
    asked.insert("party".to_string());
    asked.insert(RESERVED_MODELS[0].to_string());
    assert!(unknown(&app, &asked).is_empty());
}
