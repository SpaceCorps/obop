//! A package cannot take the id of a module the program has, and the order in which `App` builds
//! itself is what makes that true: the installer reserves the ids of the modules the dispatcher
//! holds when the installer is made, so every module the binary registers itself (the one for the
//! SIE exchange is not on the list of built-in modules the installer is given) must be registered
//! before `Installer::new`, and the stored packages must be loaded after it. `obop-pkg` tests the
//! rule with modules registered by hand; these tests run it on the real `App`, so that a move of
//! `crate::sie::command::register` in `app.rs` is noticed.

use crate::app::App;
use obop_commands::Store;
use obop_pkg::installer::Request;
use obop_storage::Fields;
use serde_json::{Value as J, json};
use std::collections::BTreeSet;

const TABLE: &str = "obop_package";

/// A package `id` with one model of its own, which no module has.
fn package(id: &str, version: &str) -> String {
    format!(
        r#"
[module]
id = "{id}"
version = "{version}"
name = "Squatter"
description = "A package"
requires_core = "^0.2"

[[model]]
name = "{id}_zz_item"
description = "A model"
read_group = "squat.user"
write_group = "squat.user"

  [[model.field]]
  name = "name"
  description = "A name"
  class = "none"
"#
    )
}

/// The record of an installed package as an older binary, another process or a restore would have
/// written it; with `previous` it can be rolled back.
fn record(manifest: &str, version: &str, previous: Option<(&str, &str)>) -> Fields {
    let mut f: Fields = [
        ("manifest".to_string(), J::from(manifest)),
        ("version".to_string(), J::from(version)),
        ("source".to_string(), J::from("upload")),
        ("installed_by".to_string(), J::from("root")),
        ("installed_at".to_string(), J::from(1_700_000_000_u64)),
    ]
    .into_iter()
    .collect();
    if let Some((text, v)) = previous {
        f.insert("previous_manifest".into(), J::from(text));
        f.insert("previous_version".into(), J::from(v));
        f.insert("previous_source".into(), J::from("upload"));
        f.insert("rollback_steps".into(), json!([]));
    }
    f
}

/// The SIE exchange still works: its module is registered, with the model it owns.
fn sie_intact(app: &App, what: &str) {
    assert!(
        app.dispatcher.module_ids().iter().any(|m| m == "sie"),
        "{what}: the module `sie` is gone"
    );
    assert_eq!(
        app.dispatcher.model_owner("sie_exchange").as_deref(),
        Some("sie"),
        "{what}"
    );
    assert!(
        app.dispatcher.has_model("sie_exchange"),
        "{what}: the commands of the SIE exchange are gone"
    );
}

/// Every id of a module the real `App` has, by the dispatcher or by the list of built-in modules,
/// is blocked at the plan and refused at the install. The ids that this checks include `sie`.
#[test]
fn a_package_cannot_take_the_id_of_any_module_the_real_app_has() {
    let app = App::new();
    let mut ids: BTreeSet<String> = app.dispatcher.module_ids().into_iter().collect();
    ids.extend(app.modules.iter().map(|m| m.id.clone()));
    for expected in [
        "base",
        "connectors",
        "crm",
        "invoicing",
        "mileage",
        "platform",
        "product",
        "sales",
        "sie",
    ] {
        assert!(ids.contains(expected), "{expected} is not among {ids:?}");
    }
    for id in &ids {
        let text = package(id, "0.1.0");
        let (plan, _) = app
            .installer
            .plan(Request::Manifest(&text))
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        assert!(
            plan.blockers.iter().any(|b| b.module == *id),
            "{id}: the plan does not block the id"
        );
        let e = app.installer.install(&text, "upload", "root").unwrap_err();
        assert!(e.contains(id.as_str()), "{id}: {e}");
        assert!(
            !app.dispatcher.has_model(&format!("{id}_zz_item")),
            "{id}: the package was registered"
        );
    }
    sie_intact(&app, "after the installs");
}

/// A record kept under the id `sie`, which was in the package table before this binary started:
/// the load at start refuses it, so the module of the program keeps its commands and no model of
/// the record is registered. If the program registered the SIE exchange after the load, the record
/// would be loaded as a package first.
#[test]
fn a_stored_package_called_sie_is_not_loaded_and_takes_nothing_from_the_program_s_module() {
    let store = Store::new();
    store
        .backend()
        .insert(
            TABLE,
            "sie",
            &record(&package("sie", "0.1.0"), "0.1.0", None),
        )
        .unwrap();
    let app = App::with_store(store.clone());
    sie_intact(&app, "after the start");
    assert!(
        !app.dispatcher.has_model("sie_zz_item"),
        "the record was loaded as a package"
    );

    // The load again (the start does it once), and the sync with another process.
    let failed = app.installer.load_installed();
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert_eq!(failed[0].0, "sie");
    assert!(
        failed[0].1.contains("built-in module") && failed[0].1.contains("not registered"),
        "{}",
        failed[0].1
    );
    sie_intact(&app, "after the load");
    store
        .backend()
        .counter_add("modules.version", 0, false)
        .unwrap();
    app.installer.sync_modules();
    sie_intact(&app, "after the sync");

    // A way back of the record is refused before anything is unregistered.
    store
        .backend()
        .update(
            TABLE,
            "sie",
            &record(
                &package("sie", "0.2.0"),
                "0.2.0",
                Some((&package("sie", "0.1.0"), "0.1.0")),
            ),
        )
        .unwrap();
    let e = app.installer.rollback("sie", "root").unwrap_err();
    assert_eq!(e, "sie is a built-in module");
    sie_intact(&app, "after the rollback");

    // The record can be removed; the module stays.
    app.installer.uninstall("sie").unwrap();
    assert!(store.backend().get(TABLE, "sie").unwrap().is_none());
    sie_intact(&app, "after the uninstall");
    let e = app.installer.uninstall("sie").unwrap_err();
    assert_eq!(e, "sie is a built-in module");
    sie_intact(&app, "after the second uninstall");
}
