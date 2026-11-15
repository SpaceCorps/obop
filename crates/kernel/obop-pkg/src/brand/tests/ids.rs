//! A package cannot take the id of a module it did not make, and a record is the package it is kept
//! as. The installer is given a list of built-in modules, but the binary registers modules of its
//! own that are not on it (the one for the SIE exchange is the real case), and a stored record can
//! carry any id: a package that took such an id would be registered under the module's id and
//! removed together with the module's commands by the next uninstall, upgrade, rollback or sync.
//! The installer therefore reserves the ids of every module the dispatcher had when it was made,
//! asks the dispatcher again for a module registered since, removes only what it registered itself,
//! and lets a package register again the models of a module of its own id only if it made that
//! module. A record whose key is not the id of its manifest is not registered.
//!
//! Every module here is registered by hand, as the binary registers its own: not through the
//! installer, and not on the list of built-in modules it is given.

use super::routes::{
    Boot, SQUAT, TABLE, drop_field, package, package_in, put, record, squatter, stored_version,
};
use super::*;
use crate::installer::{GateReceipt, GateRequest, MigrationGate, Request};
use crate::manifest::Manifest;
use crate::resolve::{Plan, Step, StepAction};
use std::sync::Mutex;
use std::sync::atomic::Ordering;

/// The group of the models of the modules that are registered by hand.
const OWNER: &str = "ledger_owner.user";

/// A module of the program: one model with a text field, the commands generated for it.
fn hand_module(id: &str, model: &'static str) -> Module {
    Module {
        id: id.into(),
        name: "By hand".into(),
        models: vec![ModelDef {
            name: model,
            description: "A model of a module that the binary registered itself",
            read_group: OWNER,
            write_group: OWNER,
            access: None,
            fields: vec![FieldDef::new(
                "name",
                "A name",
                DataClass::None,
                false,
                OWNER,
            )],
            commands: vec![],
            ext: Default::default(),
        }],
        network_destinations: vec![],
        description: String::new(),
        translations: Default::default(),
        meta: Default::default(),
    }
}

fn owner() -> Principal {
    Principal::user("owner", &[OWNER])
}

/// The security events of the log, oldest first: `(kind, user, detail)`.
fn events(store: &Store) -> Vec<(String, String, String)> {
    store
        .backend()
        .list("security_event")
        .unwrap()
        .into_iter()
        .map(|(_, f)| {
            let g = |k: &str| f[k].as_str().unwrap_or("").to_string();
            (g("kind"), g("user"), g("detail"))
        })
        .collect()
}

/// A process whose dispatcher holds `before` (registered before the installer is made, and not on
/// its list of built-in modules) and, after the installer, `after`.
fn process(store: &Store, before: &[(&str, &'static str)], after: &[(&str, &'static str)]) -> Boot {
    let d = Arc::new(Dispatcher::new());
    for (id, model) in before {
        d.register_crud(&hand_module(id, model), store);
    }
    let inst = Installer::new(store.clone(), &d, vec![admin_module()], Arc::new(Bundled));
    register_admin(&d, &inst);
    for (id, model) in after {
        d.register_crud(&hand_module(id, model), store);
    }
    Boot { d, inst }
}

/// Another process stored or changed a record: the counter moves, and this one looks.
fn sync(b: &Boot, store: &Store) {
    store
        .backend()
        .counter_add("modules.version", 0, false)
        .unwrap();
    b.inst.last_check_ms.store(0, Ordering::Release);
    b.inst.sync_modules();
}

/// What the owner of the module can do: its generated commands answer, and its model is its own.
fn module_intact(b: &Boot, id: &str, model: &str, what: &str) {
    assert_eq!(b.d.model_owner(model).as_deref(), Some(id), "{what}");
    assert_eq!(b.d.model_def(model).unwrap().write_group, OWNER, "{what}");
    b.call(&owner(), &format!("{model}.list"), &[])
        .unwrap_or_else(|e| panic!("{what}: {model}.list: {e:?}"));
    assert!(b.d.module_ids().iter().any(|m| m == id), "{what}");
}

// ---- the id of a module that was there before the installer ---------------------------------

/// The real case in the binary is `sie`; this is the same shape with a module that is on no list.
/// The plan blocks the install, a stored record is not registered, an uninstall, a sync that sees
/// the record removed and a rollback leave the module's commands alone and the record as it is.
#[test]
fn a_package_cannot_take_the_id_of_a_module_registered_before_the_installer() {
    for (id, own_model) in [("ledger", "ledger_entry"), ("ledger", "ledger_other")] {
        // The module has `ledger_entry`; the package declares `own_model`, which may or may not be
        // the module's: the id alone is enough to refuse it.
        let store = Store::new();
        let b = process(&store, &[(id, "ledger_entry")], &[]);
        let text = package(id, "0.1.0", &[own_model]);
        let (plan, _) = b.inst.plan(Request::Manifest(&text)).unwrap();
        assert!(
            plan.blockers
                .iter()
                .any(|x| x.module == id && x.kind == "builtin"),
            "{own_model}: {:?}",
            plan.blockers
                .iter()
                .map(|x| (&x.module, x.kind, &x.message))
                .collect::<Vec<_>>()
        );
        let e = b.inst.install(&text, "upload", "root").unwrap_err();
        assert!(
            e.contains(&format!(
                "{id} is a built-in module and is part of the core"
            )),
            "{own_model}: {e}"
        );
        assert!(store.backend().get(TABLE, id).unwrap().is_none());
        module_intact(&b, id, "ledger_entry", "after the install");

        // A record that an older binary stored: the load does not register it.
        put(
            &store,
            id,
            &record(&package(id, "0.1.0", &[own_model]), "0.1.0", None, J::Null),
        );
        let failed = b.inst.load_installed();
        assert_eq!(failed.len(), 1, "{own_model}: {failed:?}");
        assert!(
            failed[0].1.contains("built-in module") && failed[0].1.contains("not registered"),
            "{own_model}: {}",
            failed[0].1
        );
        assert!(!b.d.has_model(own_model) || own_model == "ledger_entry");
        module_intact(&b, id, "ledger_entry", "after the load");

        // The record is a leftover (a package stored before the id was taken, or written around
        // the installer), and the uninstall removes it: it is the only way to get rid of it. The
        // module is not the record's, so nothing of it is unregistered.
        let version = store.backend().counter_get("modules.version").unwrap_or(0);
        b.inst.uninstall(id).unwrap();
        assert!(store.backend().get(TABLE, id).unwrap().is_none());
        assert!(
            store.backend().counter_get("modules.version").unwrap_or(0) > version,
            "{own_model}: the other processes are told that a record went"
        );
        module_intact(&b, id, "ledger_entry", "after the uninstall");
        let line = events(&store)
            .into_iter()
            .find(|(kind, subject, _)| kind == "module_uninstall" && subject == id);
        assert!(line.is_some(), "{own_model}: the removal is in the log");
        // With no record there is only the module, and that is not for an uninstall.
        let e = b.inst.uninstall(id).unwrap_err();
        assert_eq!(e, format!("{id} is a built-in module"), "{own_model}");
        module_intact(&b, id, "ledger_entry", "after the second uninstall");

        // The record is removed by another process: the sync does not unregister the module.
        put(
            &store,
            id,
            &record(&package(id, "0.1.0", &[own_model]), "0.1.0", None, J::Null),
        );
        store.backend().delete(TABLE, id).unwrap();
        sync(&b, &store);
        module_intact(&b, id, "ledger_entry", "after the sync that saw it removed");

        // A rollback of a record like it is refused before anything is unregistered.
        put(
            &store,
            id,
            &record(
                &package(id, "0.2.0", &[own_model]),
                "0.2.0",
                Some((&package(id, "0.1.0", &[own_model]), "0.1.0")),
                json!([]),
            ),
        );
        let e = b.inst.rollback(id, "root").unwrap_err();
        assert_eq!(e, format!("{id} is a built-in module"), "{own_model}");
        assert_eq!(stored_version(&store, id), "0.2.0");
        module_intact(&b, id, "ledger_entry", "after the rollback");
    }
}

/// The ids and the model names are those the dispatcher had when the installer was made, not those
/// it still has: a module that is gone again is still the program's id, and its model is still a
/// name the program keeps. (The module has a model that a package of another id could declare.)
#[test]
fn what_the_program_had_stays_reserved_when_the_module_is_gone() {
    let store = Store::new();
    let b = process(&store, &[("ledger", "fleet_entry")], &[]);
    b.d.unregister_module("ledger");
    assert!(!b.d.module_ids().iter().any(|m| m == "ledger"));
    assert!(!b.d.has_model("fleet_entry"));
    let text = package("ledger", "0.1.0", &["ledger_item"]);
    let (plan, _) = b.inst.plan(Request::Manifest(&text)).unwrap();
    assert!(
        plan.blockers
            .iter()
            .any(|x| x.module == "ledger" && x.kind == "builtin"),
        "the plan blocks the id"
    );
    assert!(b.inst.install(&text, "upload", "root").is_err());
    let text = package("fleet", "0.1.0", &["fleet_entry"]);
    let e = b.inst.install(&text, "upload", "root").unwrap_err();
    assert!(e.contains("model fleet_entry already exists"), "{e}");
    assert!(!b.d.has_model("fleet_entry"));
}

// ---- a module registered after the installer ------------------------------------------------

/// A module that is registered after the installer was made is on no snapshot. The dispatcher
/// knows it, and the installer made no module of that id, so a package of the id is refused: the
/// plan blocks it, a stored record is not registered, and nothing the installer does removes the
/// module. The models are the module's, whether or not the package declares one of them.
#[test]
fn a_package_cannot_take_the_id_of_a_module_registered_after_the_installer() {
    for models in [&["late_thing"][..], &["late_other"][..]] {
        let store = Store::new();
        let b = process(&store, &[], &[("late", "late_thing")]);
        let text = package("late", "0.1.0", models);
        let (plan, _) = b.inst.plan(Request::Manifest(&text)).unwrap();
        assert!(
            plan.blockers
                .iter()
                .any(|x| x.module == "late" && x.message.contains("already registered")),
            "{models:?}: {:?}",
            plan.blockers
                .iter()
                .map(|x| (&x.module, x.kind, &x.message))
                .collect::<Vec<_>>()
        );
        assert!(b.inst.install(&text, "upload", "root").is_err());
        module_intact(&b, "late", "late_thing", "after the install");

        put(
            &store,
            "late",
            &record(&text_of(models), "0.1.0", None, J::Null),
        );
        let failed = b.inst.load_installed();
        assert_eq!(failed.len(), 1, "{models:?}: {failed:?}");
        assert!(
            failed[0].1.contains("`late`") && failed[0].1.contains("not registered"),
            "{models:?}: {}",
            failed[0].1
        );
        assert!(!b.d.has_model("late_other"));
        module_intact(&b, "late", "late_thing", "after the load");

        // The record is stored again by another process, and removed: the sync touches nothing.
        let mut newer = record(&text_of(models), "0.1.1", None, J::Null);
        newer.insert("installed_at".into(), J::from(1_800_000_000_u64));
        store.backend().update(TABLE, "late", &newer).unwrap();
        sync(&b, &store);
        module_intact(&b, "late", "late_thing", "after the sync of a change");
        store.backend().delete(TABLE, "late").unwrap();
        sync(&b, &store);
        module_intact(&b, "late", "late_thing", "after the sync of a removal");

        // The uninstall of a record like it removes the record and leaves the module.
        put(
            &store,
            "late",
            &record(&text_of(models), "0.1.0", None, J::Null),
        );
        let _ = b.inst.uninstall("late");
        module_intact(&b, "late", "late_thing", "after the uninstall");
    }
}

fn text_of(models: &[&str]) -> String {
    package("late", "0.1.0", models)
}

/// The same-id allowance of the model check is for a module the installer made, and only that: a
/// package registers its own models again (the load runs twice), but a module of its id that
/// something else registered is not its own, whatever the id says.
#[test]
fn a_package_registers_its_own_models_again_but_not_those_of_a_module_it_did_not_make() {
    let store = Store::new();
    let b = process(&store, &[], &[]);
    b.inst
        .install(&package("fleet", "0.1.0", &["fleet_a"]), "upload", "root")
        .unwrap();
    assert!(b.inst.load_installed().is_empty(), "again: its own models");
    assert_eq!(b.d.model_owner("fleet_a").as_deref(), Some("fleet"));

    // The installer unregisters the package it made, and only that.
    b.inst.uninstall("fleet").unwrap();
    assert!(!b.d.has_model("fleet_a"));
    let store = Store::new();
    let b = process(&store, &[], &[("late", "late_thing")]);
    let before = b.d.module_ids();
    b.inst.unregister(&b.d, "late");
    assert_eq!(b.d.module_ids(), before, "not the installer's to remove");
    module_intact(&b, "late", "late_thing", "after the installer's unregister");
}

// ---- the key of a record is the id of its manifest ------------------------------------------

/// Two records that carry the same manifest id: the one under its own key is the package, the other
/// is not registered. Before, the second replaced the models and groups of the first.
#[test]
fn a_record_kept_under_another_key_than_its_manifest_id_is_not_registered() {
    let store = Store::new();
    let b = process(&store, &[], &[]);
    b.inst
        .install(&package("fleet", "0.1.0", &["fleet_a"]), "upload", "root")
        .unwrap();
    assert_eq!(b.d.model_def("fleet_a").unwrap().write_group, SQUAT);
    let evil = package_in("fleet", "0.1.0", &["fleet_a"], "evil.user", &[]);
    put(&store, "zz", &record(&evil, "0.1.0", None, J::Null));

    // The load at start of another process.
    let failed = b.inst.load_installed();
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert_eq!(failed[0].0, "zz");
    assert!(
        failed[0].1.contains("kept under the id `zz`")
            && failed[0].1.contains("declares the id `fleet`")
            && failed[0].1.contains("not registered"),
        "{}",
        failed[0].1
    );
    assert_eq!(b.d.model_def("fleet_a").unwrap().write_group, SQUAT);

    // The sync, when another process stores such a record.
    let store = Store::new();
    let b = process(&store, &[], &[]);
    b.inst
        .install(&package("fleet", "0.1.0", &["fleet_a"]), "upload", "root")
        .unwrap();
    put(&store, "zz", &record(&evil, "0.1.0", None, J::Null));
    sync(&b, &store);
    assert_eq!(b.d.model_def("fleet_a").unwrap().write_group, SQUAT);
    assert_eq!(
        b.d.model_owner("fleet_a").as_deref(),
        Some("fleet"),
        "the package is the one under its key"
    );
    assert!(b.call(&squatter(), "fleet_a.list", &[]).is_ok());
}

/// A record kept under the id of a built-in module with a manifest of another id: it used to load
/// (the check looked at the manifest's id only) and could not be uninstalled.
#[test]
fn a_record_kept_under_the_id_of_a_built_in_module_with_another_manifest_id_is_not_registered() {
    let store = Store::new();
    let b = process(&store, &[], &[]);
    put(
        &store,
        "platform",
        &record(
            &package("fine", "0.1.0", &["fine_item"]),
            "0.1.0",
            None,
            J::Null,
        ),
    );
    let failed = b.inst.load_installed();
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert_eq!(failed[0].0, "platform");
    assert!(
        failed[0].1.contains("declares the id `fine`"),
        "{}",
        failed[0].1
    );
    assert!(!b.d.has_model("fine_item"));
    b.call(&super::person(), "module.list", &[]).unwrap();
}

/// The shown ids are the ASCII letters, digits and `_` of the stored text, at most 40.
#[test]
fn the_ids_a_mismatch_shows_are_cut_and_plain() {
    let store = Store::new();
    let b = process(&store, &[], &[]);
    let key = format!("{}\u{430}\n", "k".repeat(60));
    put(
        &store,
        &key,
        &record(
            &package("fine", "0.1.0", &["fine_item"]),
            "0.1.0",
            None,
            J::Null,
        ),
    );
    let failed = b.inst.load_installed();
    assert_eq!(failed.len(), 1, "{failed:?}");
    let said = &failed[0].1;
    assert!(said.contains(&format!("`{}`", "k".repeat(40))), "{said}");
    assert!(
        !said.contains('\n') && !said.contains('\u{430}'),
        "{said:?}"
    );
}

/// A way back whose earlier manifest declares another id than the record's key is refused, and the
/// version in force stays.
#[test]
fn a_rollback_to_a_manifest_of_another_id_is_refused() {
    let store = Store::new();
    let b = process(&store, &[], &[]);
    put(
        &store,
        "fleet",
        &record(
            &package("fleet", "0.2.0", &["fleet_a"]),
            "0.2.0",
            Some((&package("other", "0.1.0", &["other_a"]), "0.1.0")),
            json!([]),
        ),
    );
    assert!(b.inst.load_installed().is_empty());
    let e = b.inst.rollback("fleet", "root").unwrap_err();
    assert!(
        e.contains("kept under the id `fleet`") && e.contains("declares the id `other`"),
        "{e}"
    );
    assert_eq!(stored_version(&store, "fleet"), "0.2.0");
    assert!(b.d.has_model("fleet_a") && !b.d.has_model("other_a"));
}

// ---- a plan that was not made by `plan` -----------------------------------------------------

/// `apply_plan` is public, and a plan is data: one with a step for a module of the program is
/// refused by the step itself, in its own words, before the record is read. (`register` would
/// refuse it later, with another text.)
#[test]
fn a_forged_plan_with_the_id_of_a_module_of_the_program_is_refused_by_the_step() {
    let store = Store::new();
    let b = process(&store, &[("ledger", "ledger_entry")], &[]);
    for id in ["platform", "ledger"] {
        let extra = format!("{id}_extra");
        let manifest = Manifest::parse(&package(id, "0.1.0", &[extra.as_str()])).unwrap();
        let plan = Plan {
            steps: vec![Step {
                id: id.into(),
                name: "Forged".into(),
                version: "0.1.0".into(),
                action: StepAction::Install,
                from_version: None,
                license: String::new(),
                reason: "forged".into(),
                requested: true,
                source: "upload",
                manifest,
            }],
            blockers: Vec::new(),
            warnings: Vec::new(),
            data: Default::default(),
        };
        let e = b.inst.apply_plan(&plan, id, "upload", "root").unwrap_err();
        assert_eq!(e, format!("{id}: {id} is a built-in module"));
        assert!(store.backend().get(TABLE, id).unwrap().is_none());
        assert!(!b.d.has_model(&extra));
    }
    module_intact(&b, "ledger", "ledger_entry", "after the forged plans");
    b.call(&super::person(), "module.list", &[]).unwrap();
}

// ---- a record is listed and counted only as the package it is kept as -----------------------

/// A record kept under another key than its manifest's id is not registered (above), so it is not
/// an installed package either: the list shows the package once, the resolver does not count the
/// record, and the dependencies it declares do not hold back the uninstall of what it names.
#[test]
fn a_record_kept_under_another_key_is_not_listed_or_counted_as_an_installed_package() {
    let store = Store::new();
    let b = process(&store, &[], &[]);
    b.inst
        .install(&package("fleet", "0.1.0", &["fleet_a"]), "upload", "root")
        .unwrap();
    // The same id under another key, and a record under yet another that needs `fleet`.
    put(
        &store,
        "zz",
        &record(
            &package("fleet", "0.1.0", &["fleet_a"]),
            "0.1.0",
            None,
            J::Null,
        ),
    );
    let needs = package_in("other", "0.1.0", &["other_a"], SQUAT, &[("fleet", "^0.1")]);
    put(&store, "yy", &record(&needs, "0.1.0", None, J::Null));

    let listed: Vec<String> = b.inst.list().into_iter().map(|p| p.id).collect();
    assert_eq!(
        listed.iter().filter(|id| *id == "fleet").count(),
        1,
        "{listed:?}"
    );
    assert!(!listed.iter().any(|id| id == "other"), "{listed:?}");
    let keys: Vec<String> = b.inst.installed_manifests().into_keys().collect();
    assert_eq!(keys, ["fleet"], "only the package under its own key");
    assert!(
        b.inst.uninstall_blocker("fleet").is_none(),
        "{:?}: a record that is not registered needs nothing",
        b.inst.uninstall_blocker("fleet").map(|x| x.message)
    );
    // The records can still be removed by their keys.
    b.inst.uninstall("zz").unwrap();
    b.inst.uninstall("yy").unwrap();
    assert!(store.backend().get(TABLE, "fleet").unwrap().is_some());
    assert!(b.d.has_model("fleet_a"), "the package is untouched");
}

// ---- the refusal of the dispatcher at the last step of a registration -----------------------

/// A package that every check of the manifest accepts and that the dispatcher alone refuses when it
/// registers the public declarations: the model `{id}_line` names `head_id` as its receipt parent,
/// and the model it refers to has no receipt token.
fn refused_by_the_dispatcher(id: &str, version: &str) -> String {
    format!(
        r#"
[module]
id = "{id}"
version = "{version}"
name = "Squatter"
description = "A package"
requires_core = "^0.2"

[[model]]
name = "{id}_head"
description = "A model"
read_group = "{SQUAT}"
write_group = "{SQUAT}"

  [[model.field]]
  name = "name"
  description = "A name"
  class = "none"

[[model]]
name = "{id}_line"
description = "A model"
read_group = "{SQUAT}"
write_group = "{SQUAT}"

  [model.public]
  receipt_parent = "head_id"
  receipt_fields = ["name"]

  [[model.field]]
  name = "head_id"
  description = "The head"
  class = "none"
  kind = "ref"
  to = "{id}_head"

  [[model.field]]
  name = "name"
  description = "A name"
  class = "none"
"#
    )
}

const PUBLIC_REFUSAL: &str = "receipt_parent must refer to a model with a receipt_token_field";

fn nothing_of_it_is_registered(b: &Boot, id: &str, what: &str) {
    assert!(!b.d.has_model(&format!("{id}_head")), "{what}: head");
    assert!(!b.d.has_model(&format!("{id}_line")), "{what}: line");
    assert!(!b.d.module_ids().iter().any(|m| m == id), "{what}: module");
}

/// The registration fails at its last step, after the commands are registered: the commands are
/// taken out again, whichever route registered the package, so a package that is refused leaves
/// no model, no command and no module behind (and no record).
#[test]
fn a_package_the_dispatcher_refuses_at_the_last_step_leaves_nothing_registered() {
    let text = refused_by_the_dispatcher("fleet", "0.1.0");
    assert!(Manifest::parse(&text).is_ok(), "the manifest passes");

    // A new install.
    let store = Store::new();
    let b = process(&store, &[], &[]);
    let e = b.inst.install(&text, "upload", "root").unwrap_err();
    assert!(e.contains(PUBLIC_REFUSAL), "{e}");
    nothing_of_it_is_registered(&b, "fleet", "after the install");
    assert!(store.backend().get(TABLE, "fleet").unwrap().is_none());
    // And the id is still free for a package that is fine.
    b.inst
        .install(&package("fleet", "0.1.0", &["fleet_a"]), "upload", "root")
        .unwrap();
    assert!(b.d.has_model("fleet_a"));
}

/// The same package kept as a record (an older binary stored it, or it was restored) is not
/// registered by the load at start, and leaves nothing registered either.
#[test]
fn a_stored_package_the_dispatcher_refuses_at_the_last_step_leaves_nothing_registered() {
    let text = refused_by_the_dispatcher("fleet", "0.1.0");
    let store = Store::new();
    let b = process(&store, &[], &[]);
    put(&store, "fleet", &record(&text, "0.1.0", None, J::Null));
    let failed = b.inst.load_installed();
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert!(failed[0].1.contains(PUBLIC_REFUSAL), "{}", failed[0].1);
    nothing_of_it_is_registered(&b, "fleet", "after the load");
}

/// An upgrade whose new version the dispatcher refuses at the last step: the commands of the new
/// version are taken out and those of the version in force are back, with its definition.
#[test]
fn an_upgrade_the_dispatcher_refuses_at_the_last_step_puts_the_old_version_back() {
    let store = Store::new();
    let b = process(&store, &[], &[]);
    b.inst
        .install(&package("fleet", "0.1.0", &["fleet_a"]), "upload", "root")
        .unwrap();
    let e = b
        .inst
        .install(
            &refused_by_the_dispatcher("fleet", "0.2.0"),
            "upload",
            "root",
        )
        .unwrap_err();
    assert!(e.contains(PUBLIC_REFUSAL), "{e}");
    assert!(!b.d.has_model("fleet_head") && !b.d.has_model("fleet_line"));
    assert_eq!(stored_version(&store, "fleet"), "0.1.0");
    assert_eq!(b.d.model_owner("fleet_a").as_deref(), Some("fleet"));
    assert!(b.call(&squatter(), "fleet_a.list", &[]).is_ok());
    // Still the installer's own: it can remove the package.
    b.inst.uninstall("fleet").unwrap();
    assert!(!b.d.has_model("fleet_a"));
}

// ---- the registrations that an update, a way back and a sync replace ------------------------

/// The upgrade takes the commands of the old version out before it registers the new one: a model
/// the new version no longer declares (it holds no records, so the update may drop it) is gone.
#[test]
fn an_upgrade_that_drops_a_model_unregisters_it() {
    let store = Store::new();
    let b = process(&store, &[], &[]);
    b.inst
        .install(
            &package("fleet", "0.1.0", &["fleet_a", "fleet_b"]),
            "upload",
            "root",
        )
        .unwrap();
    b.inst
        .install(&package("fleet", "0.2.0", &["fleet_a"]), "upload", "root")
        .unwrap();
    assert_eq!(stored_version(&store, "fleet"), "0.2.0");
    assert!(b.d.has_model("fleet_a"));
    assert!(!b.d.has_model("fleet_b"), "the dropped model is gone");
}

/// The way back takes the commands of the newer version out: the model it added is gone.
#[test]
fn a_rollback_unregisters_the_model_the_newer_version_added() {
    let store = Store::new();
    let b = process(&store, &[], &[]);
    b.inst
        .install(&package("fleet", "0.1.0", &["fleet_a"]), "upload", "root")
        .unwrap();
    b.inst
        .install(
            &package("fleet", "0.2.0", &["fleet_a", "fleet_b"]),
            "upload",
            "root",
        )
        .unwrap();
    assert!(b.d.has_model("fleet_b"));
    b.inst.rollback("fleet", "root").unwrap();
    assert_eq!(stored_version(&store, "fleet"), "0.1.0");
    assert!(b.d.has_model("fleet_a"));
    assert!(!b.d.has_model("fleet_b"), "the model of the newer version");
}

/// The sync of an upgrade made by another process: the old registration is taken out before the new
/// one is made, so a model the new version dropped is gone from this process too.
#[test]
fn a_sync_of_an_upgrade_made_elsewhere_unregisters_the_model_it_dropped() {
    let store = Store::new();
    let b = process(&store, &[], &[]);
    b.inst
        .install(
            &package("fleet", "0.1.0", &["fleet_a", "fleet_b"]),
            "upload",
            "root",
        )
        .unwrap();
    let mut newer = record(
        &package("fleet", "0.2.0", &["fleet_a"]),
        "0.2.0",
        None,
        J::Null,
    );
    newer.insert("installed_at".into(), J::from(1_800_000_000_u64));
    store.backend().update(TABLE, "fleet", &newer).unwrap();
    sync(&b, &store);
    assert!(b.d.has_model("fleet_a"));
    assert!(!b.d.has_model("fleet_b"), "the model the upgrade dropped");
}

// ---- a way back that is refused by the id costs no backup -----------------------------------

/// A gate that counts the times it is asked.
#[derive(Default)]
struct CountingGate {
    asked: Mutex<usize>,
}

impl MigrationGate for CountingGate {
    fn before(&self, _: &GateRequest) -> Result<GateReceipt, String> {
        *self.asked.lock().unwrap() += 1;
        Ok(GateReceipt::default())
    }
}

/// The refusal of a way back by the id comes before the backup gate, which would otherwise take a
/// verified backup for a rollback that is then refused. The control is the same record under an id
/// that is free: the gate is asked, so the setup does reach it.
#[test]
fn a_rollback_refused_by_the_id_does_not_ask_the_backup_gate() {
    for (id, refused) in [("late", true), ("fleet", false)] {
        let store = Store::new();
        let b = process(&store, &[], &[("late", "late_thing")]);
        let gate = Arc::new(CountingGate::default());
        b.inst.set_gate(gate.clone());
        // A model of the package that holds a record, so that the step of the way back rewrites
        // something and the gate would be asked.
        let model = format!("{id}_note");
        let mut f = obop_storage::Fields::new();
        f.insert("name".into(), J::from("a"));
        store.backend().insert(&model, "1", &f).unwrap();
        put(
            &store,
            id,
            &record(
                &package(id, "0.2.0", &[model.as_str()]),
                "0.2.0",
                Some((&package(id, "0.1.0", &[model.as_str()]), "0.1.0")),
                drop_field(&model, "name"),
            ),
        );
        if refused {
            let e = b.inst.rollback(id, "root").unwrap_err();
            assert!(e.contains("registered and is not a package"), "{e}");
            assert_eq!(*gate.asked.lock().unwrap(), 0, "{id}");
        } else {
            // The record becomes a package of this process by the load, then it is rolled back.
            let loaded = b.inst.load_installed();
            assert!(loaded.is_empty(), "{loaded:?}");
            b.inst.rollback(id, "root").unwrap();
            assert_eq!(*gate.asked.lock().unwrap(), 1, "{id}");
        }
    }
}
