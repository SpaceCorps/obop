//! A package cannot take a name that is not its own, by any route that registers a package: not a
//! table the program reads and writes itself (the accounts, the sessions, the OAuth grants, the
//! package records ...), not a model that another module has registered, not the id of a built-in
//! module. `routes.rs` is about the models that were registered before any package; these are the
//! rest of the same family.
//!
//! The manifest rule lets a package declare a model named like its id or starting with `<id>_`, so
//! a package called `auth` may declare `auth_user`, and one called `fleet` may declare
//! `fleet_tools_item`, which belongs to the package `fleet_tools`. The generated commands of a model
//! reach the table of that name: a package called `auth` would list the password hashes and set
//! them. The install gate (the plan) refuses such a name; the routes that do not make a plan (the
//! load at start of a stored record, the sync with another process, a rollback) go through
//! `Installer::register`, which refuses it as well. A stored record is written around the installer
//! here, as an older binary would have written it.

use super::routes::{
    Boot, SQUAT, TABLE, boot, configured, drop_field, package, package_in, put, record,
    rollback_with, squatter, stored_version,
};
use super::*;
use crate::installer::Request;
use crate::marketplace::{Listing, Marketplace};
use obop_registry::{INTERNAL_TABLES, RESERVED_MODELS};
use obop_storage::Fields;
use std::sync::atomic::Ordering;

const TOOLS: &str = "tools.user";
const FLEET: &str = "fleet.user";

/// Another process changed the installed set: the counter moves, and this one looks.
fn sync(b: &Boot, store: &Store) {
    store
        .backend()
        .counter_add("modules.version", 0, false)
        .unwrap();
    b.inst.last_check_ms.store(0, Ordering::Release);
    b.inst.sync_modules();
}

/// A package for a marketplace, which wants a summary, an author and a licence besides.
fn offered(text: String) -> String {
    text.replace(
        "requires_core = \"^0.2\"\n",
        "requires_core = \"^0.2\"\nsummary = \"A package\"\nauthor = \"Tester\"\nlicense = \"MIT\"\n",
    )
}

/// A marketplace that offers the manifests it is given.
struct Shelf(Vec<String>);

impl Marketplace for Shelf {
    fn list(&self) -> Result<Vec<Listing>, String> {
        self.0
            .iter()
            .map(|t| crate::manifest::Manifest::parse(t).map(|m| Listing::from_manifest(&m)))
            .collect()
    }
    fn fetch(&self, id: &str) -> Result<String, String> {
        self.0
            .iter()
            .find(|t| crate::manifest::Manifest::parse(t).is_ok_and(|m| m.module.id == id))
            .cloned()
            .ok_or_else(|| format!("no package {id}"))
    }
}

fn boot_with(store: &Store, shelf: Vec<String>) -> Boot {
    let d = Arc::new(Dispatcher::new());
    let inst = Installer::new(
        store.clone(),
        &d,
        vec![admin_module()],
        Arc::new(Shelf(shelf)),
    );
    register_admin(&d, &inst);
    Boot { d, inst }
}

fn row(text: &str) -> Fields {
    [("name".to_string(), J::from(text))].into_iter().collect()
}

// ---- the tables of the program ------------------------------------------------------------

/// The attack of the review: the package `auth` declares `auth_user`, and whoever holds its group
/// then reads the password hashes and writes them. The same for the tokens, the grants and the other
/// tables the program reads by name. The plan refuses the install; a record stored around the gate
/// is not registered; the table is as it was.
#[test]
fn a_package_cannot_declare_a_table_the_program_reads_and_writes_itself() {
    for (id, table) in [
        ("auth", "auth_user"),
        ("auth", "auth_token"),
        ("auth", "auth_mfa_token"),
        ("auth", "auth_passkey"),
        ("oauth", "oauth_grant"),
        ("oauth", "oauth_refresh"),
        ("oauth", "oauth_client"),
        ("obop", "obop_package"),
        ("obop", "obop_migration"),
        ("obop", "obop_meta"),
        ("bulk", "bulk_idempotency"),
        ("connector", "connector_vault"),
    ] {
        let store = Store::new();
        store.backend().insert(table, "root", &row("real")).unwrap();
        let b = boot(&store);
        let e = b
            .inst
            .install(&package(id, "0.1.0", &[table]), "upload", "root")
            .unwrap_err();
        assert!(
            e.contains(&format!("model {table} already exists")),
            "{table}: {e}"
        );
        assert!(!b.d.has_model(table), "{table}: nothing was registered");
        // An older binary stored it, or another process did.
        put(
            &store,
            id,
            &record(&package(id, "0.1.0", &[table]), "0.1.0", None, J::Null),
        );
        // (The package table also holds the sentinel row, which is no package: it fails by itself.)
        let failed = b.inst.load_installed();
        let ours: Vec<_> = failed.iter().filter(|(i, _)| i == id).collect();
        assert_eq!(ours.len(), 1, "{table}: {failed:?}");
        assert!(
            ours[0].1.contains(&format!("`{table}`")) && ours[0].1.contains("not registered"),
            "{table}: {}",
            ours[0].1
        );
        assert!(!b.d.has_model(table), "{table}: not registered by the load");
        // The holder of the package's group has no command over the table, and it is as it was.
        for command in [
            "create",
            "get",
            "list",
            "search",
            "update",
            "delete",
            "bulk_create",
        ] {
            let e = b
                .call(
                    &squatter(),
                    &format!("{table}.{command}"),
                    &[("id", "root"), ("name", "taken")],
                )
                .unwrap_err();
            assert!(
                matches!(e, CallError::UnknownCommand(_)),
                "{table}.{command}: {e:?}"
            );
        }
        assert_eq!(
            store.backend().get(table, "root").unwrap(),
            Some(row("real")),
            "{table}"
        );
    }
}

/// Every name the registry reserves, the tables of the program and the names of the platform, is
/// refused when a stored package declares it. (The package takes the name as its id, which the
/// manifest rule allows.) A name that is also a model of the program is still that model's.
#[test]
fn every_name_the_registry_reserves_is_refused_at_the_load() {
    assert!(INTERNAL_TABLES.len() > 30 && RESERVED_MODELS.len() == 5);
    for name in INTERNAL_TABLES.iter().chain(RESERVED_MODELS.iter()) {
        let store = Store::new();
        put(
            &store,
            name,
            &record(&package(name, "0.1.0", &[name]), "0.1.0", None, J::Null),
        );
        let b = boot(&store);
        let before = b.d.model_owner(name);
        let failed = b.inst.load_installed();
        assert_eq!(failed.len(), 1, "{name}: {failed:?}");
        assert_eq!(failed[0].0, *name);
        assert!(
            failed[0].1.contains(&format!("`{name}`")) && failed[0].1.contains("not registered"),
            "{name}: {}",
            failed[0].1
        );
        assert_eq!(
            b.d.model_owner(name),
            before,
            "{name}: the package did not become the owner"
        );
        assert_ne!(b.d.model_owner(name).as_deref(), Some(*name), "{name}");
    }
}

// ---- a model of another module -------------------------------------------------------------

/// A package id that is a prefix of another's takes the other's models: `fleet` may declare
/// `fleet_tools_item`, which the package `fleet_tools` has. The plan of an upgrade that adds it is
/// blocked, and so is the install; nothing is changed.
#[test]
fn an_upgrade_that_adds_a_model_another_package_has_is_refused_by_the_plan_and_by_the_install() {
    let store = Store::new();
    let b = boot(&store);
    b.inst
        .install(
            &package_in("fleet_tools", "0.1.0", &["fleet_tools_item"], TOOLS, &[]),
            "upload",
            "root",
        )
        .unwrap();
    b.inst
        .install(
            &package_in("fleet", "0.1.0", &["fleet_a"], FLEET, &[]),
            "upload",
            "root",
        )
        .unwrap();
    let upgrade = package_in(
        "fleet",
        "0.2.0",
        &["fleet_a", "fleet_tools_item"],
        FLEET,
        &[],
    );
    let (plan, _) = b.inst.plan(Request::Manifest(&upgrade)).unwrap();
    assert!(
        plan.blockers.iter().any(|x| x.module == "fleet"
            && x.kind == "conflict"
            && x.message == "model fleet_tools_item already exists"),
        "{:?}",
        plan.blockers
            .iter()
            .map(|x| (&x.kind, &x.message))
            .collect::<Vec<_>>()
    );
    let e = b.inst.install(&upgrade, "upload", "root").unwrap_err();
    assert!(e.contains("model fleet_tools_item already exists"), "{e}");
    assert_eq!(stored_version(&store, "fleet"), "0.1.0");
    assert_eq!(
        b.d.model_def("fleet_tools_item").unwrap().write_group,
        TOOLS
    );
    assert!(b.d.has_model("fleet_a"), "the version in force stays");
}

/// The same name in two stored records (an older binary, a restore, two processes): whichever is
/// registered first has it, the other is refused whole and says who has the model.
#[test]
fn of_two_stored_packages_that_declare_one_model_the_first_registered_keeps_it() {
    // The model that is taken is the last of the package's, the first, and one in the middle.
    for (models, (first, second)) in [
        ["fleet_a", "fleet_tools_item", "fleet_z"],
        ["fleet_tools_item", "fleet_a", "fleet_z"],
        ["fleet_a", "fleet_z", "fleet_tools_item"],
    ]
    .into_iter()
    .flat_map(|m| [(m, ("fleet_tools", "fleet")), (m, ("fleet", "fleet_tools"))])
    {
        let manifest = |id: &str| match id {
            "fleet" => package_in("fleet", "0.1.0", &models, FLEET, &[]),
            _ => package_in("fleet_tools", "0.1.0", &["fleet_tools_item"], TOOLS, &[]),
        };
        let store = Store::new();
        for id in [first, second] {
            put(&store, id, &record(&manifest(id), "0.1.0", None, J::Null));
        }
        let b = boot(&store);
        let failed = b.inst.load_installed();
        assert_eq!(failed.len(), 1, "{first}: {failed:?}");
        assert_eq!(failed[0].0, second);
        assert!(
            failed[0].1.contains("`fleet_tools_item`")
                && failed[0].1.contains(&format!("`{first}`"))
                && failed[0].1.contains("already has")
                && failed[0].1.contains("not registered"),
            "{}",
            failed[0].1
        );
        assert_eq!(b.d.model_owner("fleet_tools_item").as_deref(), Some(first));
        let want = if first == "fleet" { FLEET } else { TOOLS };
        assert_eq!(b.d.model_def("fleet_tools_item").unwrap().write_group, want);
        // The refused package is skipped whole: not even its other models are registered.
        for other in ["fleet_a", "fleet_z"] {
            assert_eq!(b.d.has_model(other), first == "fleet", "{other}");
        }
    }
}

/// The sync: another process upgrades `fleet` to a version that takes the model. This process
/// does not register it, and the owner keeps the model.
#[test]
fn a_sync_does_not_let_an_upgraded_package_take_a_model_another_package_has() {
    let store = Store::new();
    let b = boot(&store);
    for text in [
        package_in("fleet_tools", "0.1.0", &["fleet_tools_item"], TOOLS, &[]),
        package_in("fleet", "0.1.0", &["fleet_a"], FLEET, &[]),
    ] {
        b.inst.install(&text, "upload", "root").unwrap();
    }
    let mut newer = record(
        &package_in(
            "fleet",
            "0.2.0",
            &["fleet_a", "fleet_tools_item"],
            FLEET,
            &[],
        ),
        "0.2.0",
        None,
        J::Null,
    );
    newer.insert("installed_at".into(), J::from(1_800_000_000_u64));
    store.backend().update(TABLE, "fleet", &newer).unwrap();
    sync(&b, &store);
    assert_eq!(
        b.d.model_owner("fleet_tools_item").as_deref(),
        Some("fleet_tools")
    );
    assert_eq!(
        b.d.model_def("fleet_tools_item").unwrap().write_group,
        TOOLS
    );
}

/// A rollback to a manifest that declares the model: refused, and the version in force stays.
#[test]
fn a_rollback_to_a_manifest_with_a_model_another_package_has_is_refused() {
    let store = Store::new();
    put(
        &store,
        "fleet_tools",
        &record(
            &package_in("fleet_tools", "0.1.0", &["fleet_tools_item"], TOOLS, &[]),
            "0.1.0",
            None,
            J::Null,
        ),
    );
    put(
        &store,
        "fleet",
        &record(
            &package_in("fleet", "0.2.0", &["fleet_a"], FLEET, &[]),
            "0.2.0",
            Some((
                &package_in(
                    "fleet",
                    "0.1.0",
                    &["fleet_a", "fleet_tools_item"],
                    FLEET,
                    &[],
                ),
                "0.1.0",
            )),
            json!([]),
        ),
    );
    let b = boot(&store);
    assert!(b.inst.load_installed().is_empty());
    let e = b.inst.rollback("fleet", "root").unwrap_err();
    assert!(
        e.contains("`fleet_tools_item`")
            && e.contains("`fleet_tools`")
            && e.contains("already has"),
        "{e}"
    );
    assert_eq!(stored_version(&store, "fleet"), "0.2.0");
    assert!(b.d.has_model("fleet_a"), "the version in force stays");
    assert_eq!(
        b.d.model_def("fleet_tools_item").unwrap().write_group,
        TOOLS
    );
}

/// The names the installer learns when it is made are a snapshot. A model that a module registers
/// after that, and that no list names, is still its module's: the dispatcher says who has it.
#[test]
fn a_model_registered_after_the_installer_was_made_is_still_its_modules() {
    const OWNER: &str = "late_owner.user";
    let late = Module {
        id: "late_core".into(),
        name: "Late".into(),
        models: vec![ModelDef {
            name: "late_thing",
            description: "A model of a module that was bound late",
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
    };
    let store = Store::new();
    let d = Arc::new(Dispatcher::new());
    let inst = Installer::new(store.clone(), &d, vec![admin_module()], Arc::new(Bundled));
    register_admin(&d, &inst);
    d.register_crud(&late, &store);
    put(
        &store,
        "late",
        &record(
            &package("late", "0.1.0", &["late_thing"]),
            "0.1.0",
            None,
            J::Null,
        ),
    );
    let failed = inst.load_installed();
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert!(
        failed[0].1.contains("`late_thing`") && failed[0].1.contains("`late_core`"),
        "{}",
        failed[0].1
    );
    assert_eq!(d.model_def("late_thing").unwrap().write_group, OWNER);
    assert_eq!(d.model_owner("late_thing").as_deref(), Some("late_core"));
    // A new install is refused in the words of the plan.
    let e = inst
        .install(&package("late", "0.1.0", &["late_thing"]), "upload", "root")
        .unwrap_err();
    assert!(e.contains("model late_thing already exists"), "{e}");
}

// ---- all the models of a package are looked at ---------------------------------------------

/// Where in the list of a package's models the reserved one stands does not matter: first, in the
/// middle, last. The package is skipped whole, and the harmless models are not registered either.
#[test]
fn a_reserved_model_among_harmless_ones_refuses_the_package_wherever_it_stands() {
    for position in 0..3 {
        for (id, reserved) in [("instance", MODEL), ("auth", "auth_user")] {
            let (first, last) = (format!("{id}_a"), format!("{id}_z"));
            let mut list = vec![first.as_str(), last.as_str()];
            list.insert(position, reserved);
            let what = format!("{reserved} at {position}");
            let other = format!("{id}_other");
            let nothing_else = |b: &Boot, store: &Store, when: &str| {
                for harmless in [&first, &last] {
                    assert!(
                        !b.d.has_model(harmless),
                        "{what}: {harmless} is not registered {when}"
                    );
                }
                if reserved == MODEL {
                    b.assert_untouched(store, &format!("{what} {when}"));
                }
            };

            // The load at start.
            let store = Store::new();
            let b = configured(&store);
            put(
                &store,
                id,
                &record(&package(id, "0.1.0", &list), "0.1.0", None, J::Null),
            );
            let failed = b.inst.load_installed();
            assert_eq!(failed.len(), 1, "{what}: {failed:?}");
            assert!(failed[0].1.contains(&format!("`{reserved}`")), "{what}");
            nothing_else(&b, &store, "after the load");

            // The sync.
            let store = Store::new();
            let b = configured(&store);
            assert!(b.inst.load_installed().is_empty());
            put(
                &store,
                id,
                &record(&package(id, "0.1.0", &list), "0.1.0", None, J::Null),
            );
            sync(&b, &store);
            nothing_else(&b, &store, "after the sync");

            // The rollback to a manifest like it.
            let store = Store::new();
            let b = configured(&store);
            put(
                &store,
                id,
                &record(
                    &package(id, "0.2.0", &[other.as_str()]),
                    "0.2.0",
                    Some((&package(id, "0.1.0", &list), "0.1.0")),
                    json!([]),
                ),
            );
            assert!(b.inst.load_installed().is_empty());
            let e = b.inst.rollback(id, "root").unwrap_err();
            assert!(e.contains(&format!("`{reserved}`")), "{what}: {e}");
            assert_eq!(stored_version(&store, id), "0.2.0", "{what}");
            nothing_else(&b, &store, "after the rollback");
            assert!(b.d.has_model(&other), "{what}: the version in force");
        }
    }
}

// ---- the plan looks at every step ----------------------------------------------------------

/// A package that depends on another is planned with it, dependency first. The reserved model is
/// refused wherever it is in the plan: in the dependency, or in the package that was asked for.
#[test]
fn the_plan_refuses_a_reserved_model_in_a_dependency_and_in_the_package_asked_for() {
    let blockers = |plan: &crate::resolve::Plan| -> Vec<(String, String, String)> {
        plan.blockers
            .iter()
            .map(|x| (x.module.clone(), x.kind.to_string(), x.message.clone()))
            .collect()
    };
    // The dependency declares it.
    let store = Store::new();
    let b = boot_with(
        &store,
        vec![offered(package_in(
            "instance",
            "0.1.0",
            &[MODEL, "instance_a"],
            SQUAT,
            &[],
        ))],
    );
    let root = package_in(
        "asker",
        "0.1.0",
        &["asker_item"],
        SQUAT,
        &[("instance", "^0.1")],
    );
    let (plan, _) = b.inst.plan(Request::Manifest(&root)).unwrap();
    assert_eq!(
        blockers(&plan),
        [(
            "instance".to_string(),
            "conflict".to_string(),
            "model instance_brand already exists".to_string()
        )]
    );
    assert!(plan.steps.len() >= 2, "the dependency is a step beside it");
    let e = b.inst.install(&root, "upload", "root").unwrap_err();
    assert!(e.contains("model instance_brand already exists"), "{e}");
    assert!(!b.d.has_model("asker_item") && !b.d.has_model("instance_a"));

    // The package asked for declares it; the dependency in the plan with it is harmless.
    let store = Store::new();
    let b = boot_with(
        &store,
        vec![offered(package_in(
            "depended",
            "0.1.0",
            &["depended"],
            SQUAT,
            &[],
        ))],
    );
    let root = package_in(
        "auth",
        "0.1.0",
        &["auth_item", "auth_user"],
        SQUAT,
        &[("depended", "^0.1")],
    );
    let (plan, _) = b.inst.plan(Request::Manifest(&root)).unwrap();
    assert_eq!(
        blockers(&plan),
        [(
            "auth".to_string(),
            "conflict".to_string(),
            "model auth_user already exists".to_string()
        )]
    );
    assert!(plan.steps.len() >= 2, "the dependency is a step before it");
    let e = b.inst.install(&root, "upload", "root").unwrap_err();
    assert!(e.contains("model auth_user already exists"), "{e}");
    assert!(
        !b.d.has_model("depended"),
        "the dependency was not installed either"
    );
}

// ---- the stored way back is held to the models of the package ------------------------------

/// The guard compares model names whole: a model whose name is the start of a declared one is not
/// a declared one. (`probe` against `probe_note`.)
#[test]
fn a_stored_way_back_on_a_model_named_like_the_start_of_a_models_of_the_package_is_refused() {
    for model in ["probe", "probe_not", "probe_notes", "probe_no"] {
        let (store, b, answer) = rollback_with(drop_field(model, "name"), &["probe_note"]);
        let e = answer.unwrap_err();
        assert!(e.contains(&format!("`{model}`")), "{model}: {e}");
        assert_eq!(stored_version(&store, "probe"), "0.2.0", "{model}");
        assert_eq!(b.state(&store).0.unwrap()["name"], "MOHV");
    }
}

/// What is shown of a model name that came from a stored record is cut at 40 characters and holds
/// ASCII letters, digits and `_` only: not the letters of another script that look like them.
#[test]
fn what_a_refusal_shows_of_a_stored_model_name_is_short_and_ascii() {
    let shown = |name: &str| -> String {
        let (_, _, answer) = rollback_with(drop_field(name, "name"), &["probe_note"]);
        let e = answer.unwrap_err();
        assert!(e.len() < 400, "{} bytes", e.len());
        let from = e.find("on the model `").unwrap() + "on the model `".len();
        let to = from + e[from..].find('`').unwrap();
        e[from..to].to_string()
    };
    assert_eq!(shown(&"a".repeat(5000)), "a".repeat(40));
    assert_eq!(
        shown(&format!("{}_{}", "x".repeat(39), "y".repeat(100))),
        format!("{}_", "x".repeat(39))
    );
    // Cyrillic letters that look like Latin ones, other digits, accented letters: none are shown.
    assert_eq!(shown("p\u{430}\u{440}ty"), "pty");
    assert_eq!(shown("\u{430}\u{440}\u{441}"), "");
    assert_eq!(shown("n\u{e9}w_\u{661}\u{662}9"), "nw_9");
    assert_eq!(shown("ok_name_1"), "ok_name_1");
}

// ---- the id of a built-in module ----------------------------------------------------------

/// A stored record can carry any id. One with the id of a built-in module is not registered by the
/// load, the sync or a rollback, and none of them unregisters the commands of the built-in module.
#[test]
fn a_record_with_the_id_of_a_built_in_module_takes_nothing_from_it() {
    let store = Store::new();
    let b = boot(&store);
    let works = |b: &Boot, what: &str| {
        b.call(&super::person(), "module.list", &[])
            .unwrap_or_else(|e| panic!("{what}: module.list: {e:?}"));
        b.call(&super::person(), "instance_brand.get", &[])
            .unwrap_or_else(|e| panic!("{what}: instance_brand.get: {e:?}"));
    };
    works(&b, "at the start");
    // Stored by an older binary, with a model of its own.
    put(
        &store,
        "platform",
        &record(
            &package("platform", "0.1.0", &["platform_x"]),
            "0.1.0",
            None,
            J::Null,
        ),
    );
    let failed = b.inst.load_installed();
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert_eq!(failed[0].0, "platform");
    assert!(
        failed[0].1.contains("built-in module") && failed[0].1.contains("not registered"),
        "{}",
        failed[0].1
    );
    assert!(!b.d.has_model("platform_x"));
    works(&b, "after the load");
    // It is stored again by another process: the fingerprint moves, and the sync looks.
    let mut newer = record(
        &package("platform", "0.2.0", &["platform_x"]),
        "0.2.0",
        None,
        J::Null,
    );
    newer.insert("installed_at".into(), J::from(1_800_000_000_u64));
    store.backend().update(TABLE, "platform", &newer).unwrap();
    sync(&b, &store);
    assert!(!b.d.has_model("platform_x"));
    works(&b, "after the sync of an upgrade");
    // A rollback is refused before anything is unregistered.
    let mut with_way_back = record(
        &package("platform", "0.2.0", &["platform_x"]),
        "0.2.0",
        Some((&package("platform", "0.1.0", &["platform_x"]), "0.1.0")),
        json!([]),
    );
    with_way_back.insert("installed_at".into(), J::from(1_800_000_100_u64));
    store
        .backend()
        .update(TABLE, "platform", &with_way_back)
        .unwrap();
    let e = b.inst.rollback("platform", "root").unwrap_err();
    assert!(e.contains("built-in module"), "{e}");
    works(&b, "after the rollback");
    // And when the record is removed elsewhere, the sync does not unregister the built-in module.
    store.backend().delete(TABLE, "platform").unwrap();
    sync(&b, &store);
    works(&b, "after the record was removed");
}

// ---- what the refusal says to do ----------------------------------------------------------

/// The refusal names `uninstall` first, because renaming works only while the model holds no
/// records, and the table of a built-in model has some. Both halves are true: the rename is refused
/// while the table holds records, and goes through while it holds none; the uninstall works and
/// keeps the data.
#[test]
fn the_remedy_the_refusal_names_works() {
    // A model with records: the branding row.
    let store = Store::new();
    let b = configured(&store);
    put(
        &store,
        "instance",
        &record(
            &package("instance", "0.1.0", &[MODEL]),
            "0.1.0",
            None,
            J::Null,
        ),
    );
    let failed = b.inst.load_installed();
    assert_eq!(failed.len(), 1);
    let said = &failed[0].1;
    assert!(
        said.contains("Uninstall the package; its records stay where they are")
            && said.find("Uninstall").unwrap() < said.find("renames").unwrap(),
        "{said}"
    );
    let e = b
        .inst
        .install(
            &package("instance", "0.2.0", &["instance_other"]),
            "upload",
            "root",
        )
        .unwrap_err();
    assert!(
        e.contains("orphaned"),
        "a rename is refused while there are records: {e}"
    );
    b.inst.uninstall("instance").unwrap();
    assert!(store.backend().get(TABLE, "instance").unwrap().is_none());
    assert_eq!(b.state(&store).0.unwrap()["name"], "MOHV", "the data stays");
    b.call(&super::person(), "instance_brand.get", &[]).unwrap();

    // A table of the program that holds no records: the rename goes through.
    let store = Store::new();
    let b = boot(&store);
    put(
        &store,
        "auth",
        &record(
            &package("auth", "0.1.0", &["auth_passkey"]),
            "0.1.0",
            None,
            J::Null,
        ),
    );
    assert_eq!(b.inst.load_installed().len(), 1);
    let renamed = b
        .inst
        .install(&package("auth", "0.2.0", &["auth_other"]), "upload", "root")
        .unwrap();
    assert_eq!(renamed.version, "0.2.0");
    assert!(b.d.has_model("auth_other"));
}
