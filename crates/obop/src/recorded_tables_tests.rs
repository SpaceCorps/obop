//! The tables the real program writes while it runs, against the names the installer keeps from a
//! package. `internal_tables_scan_tests` reads the source and cannot see a name that is built at run
//! time or handed on through a variable; this test runs the real `App` over a memory store through
//! a few flows and asks the store which models hold a row afterwards (`Backend::export`: every
//! record of every model, history included). Each of those names must be reserved by name
//! (`INTERNAL_TABLES`, `RESERVED_MODELS`) or be a model of a module of the `App` (the installer
//! refuses those because their module owns them).
//!
//! **What it sees.** The models that hold a row after: the start of the `App` (the key ring, the
//! connectors' sealed keys), the demo data and the numbers of its documents (`sequence_counter`), an
//! account created and signed in (the session is an `auth_token` row), a connection with its
//! provider policy, and a bundled package installed (its models are the package's while it is
//! installed).
//! **What it does not see.** A table that is only read, or written by a flow this test does not
//! run: the backup and restore catalog, the migration ledger, OAuth, single sign-on, passkeys, the
//! forum and the status page (they run in other processes, over other crates' stores), a webhook, a
//! payment. Those are the scan's job, and the scan's blind spots are in docs/ISSUES.md N17.

use crate::app::{App, dev_principal};
use obop_auth::Auth;
use obop_commands::{ADMIN_GROUP, Authenticator, CallError, Response, Surface, Value, Vault};
use obop_pkg::{Bundled, Marketplace};
use obop_registry::{INTERNAL_TABLES, RESERVED_MODELS};
use serde_json::json;
use std::collections::BTreeSet;

/// The models that hold at least one row.
fn written(app: &App) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    app.store
        .backend()
        .export(500, &mut |rows| {
            out.extend(rows.iter().map(|(model, _, _)| model.clone()));
            Ok(())
        })
        .expect("a memory store exports");
    out
}

/// A command as the development administrator, the way the HTTP surface would call it.
fn admin_call(app: &App, command: &str, input: serde_json::Value) -> Result<Response, CallError> {
    let admin = dev_principal("dev-admin").expect("the development administrator");
    let input = Value::from_json(&input);
    app.dispatcher
        .call(Surface::Rest, &admin, &mut Vault::new(), command, &input)
}

/// The flows, over one `App`. Each step must work: a flow that quietly did nothing would leave the
/// test green for the wrong reason (the names it must reach are asserted below).
fn run_flows(app: &App) {
    // The demo data, and the numbers of the documents (SO-00001): the counters of the sequences.
    app.seed_demo();
    // An account and a sign-in.
    let auth = Auth::new(app.store.backend());
    auth.create_user(
        "recorder-admin",
        "a password of some length",
        &[ADMIN_GROUP.to_string()],
    )
    .expect("create the account");
    auth.login("recorder-admin", "a password of some length")
        .expect("sign in");
    // A connection with its provider policy.
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
    ] {
        admin_call(app, command, input).unwrap_or_else(|e| panic!("{command}: {e:?}"));
    }
    // A bundled package, with the models it declares.
    let text = Bundled.fetch("website").expect("the bundled package");
    app.installer
        .install(&text, "marketplace", "root")
        .expect("install a bundled package");
}

#[test]
fn every_model_the_running_program_writes_is_reserved_or_belongs_to_a_module() {
    let app = App::new();
    let at_start = written(&app);
    run_flows(&app);
    let all = written(&app);
    let known: BTreeSet<String> = INTERNAL_TABLES
        .iter()
        .chain(RESERVED_MODELS.iter())
        .map(|n| n.to_string())
        .chain(app.dispatcher.model_names())
        .collect();
    let unknown: Vec<&String> = all.iter().filter(|n| !known.contains(*n)).collect();
    assert!(
        unknown.is_empty(),
        "the running program writes a model that no package is kept from declaring. Add it to \
         obop_registry::INTERNAL_TABLES:\n{unknown:?}"
    );
    // The flows reached what they are there for, so a green test means something: the document
    // counters and the accounts (`written` is what the store holds, not what the code names).
    for name in [
        "sequence_counter",
        "auth_user",
        "auth_token",
        "party",
        "connection",
    ] {
        assert!(
            all.contains(name),
            "the flows wrote no row of {name}: {all:?}"
        );
    }
    assert!(
        !at_start.contains("sequence_counter") && all.len() > at_start.len() + 10,
        "the flows add to what the start of the App wrote: {at_start:?} -> {all:?}"
    );
}
