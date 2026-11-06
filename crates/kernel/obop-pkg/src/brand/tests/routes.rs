//! A package cannot take the name of a model that was there before any package, by any route that
//! registers a package (docs/spec/branding.md, section 3, "Only these commands": the branding row
//! and the log of security events are changed by their own commands alone). A package named
//! `instance` may declare a model `instance_brand`, and one named `security` a model
//! `security_event`; if such a package were registered, its generated create, update and delete
//! would write the branding row, and delete the log, for anyone who holds the package's group, with
//! no person-only check, no character policy, no lock and no audit event.
//!
//! The install gate (the plan) refuses the name for an install and for an upgrade. These tests are
//! about the routes that do not make a plan: the load at start of a package that was stored before
//! the name was taken (or by an older binary, or restored), the sync with another process, and a
//! rollback to an earlier manifest. A stored record is written around the installer, as an older
//! binary would have written it.

use super::*;
use obop_storage::Fields;

const TABLE: &str = "obop_package";
const SQUAT: &str = "squat.user";

/// The text of a package `id` at `version` with the models given, each with a text field `name`
/// (and, for `squat`, the group `squat.user` for reading and writing).
fn package(id: &str, version: &str, models: &[&str]) -> String {
    let mut text = format!(
        r#"
[module]
id = "{id}"
version = "{version}"
name = "Squatter"
description = "A package"
requires_core = "^0.2"
"#
    );
    for model in models {
        text.push_str(&format!(
            r#"
[[model]]
name = "{model}"
description = "A model"
read_group = "{SQUAT}"
write_group = "{SQUAT}"

  [[model.field]]
  name = "name"
  description = "A name"
  class = "none"
"#
        ));
    }
    text
}

/// The record of a package as the installer writes it (an older binary, another process, or a
/// restore wrote it around the install gate). With `previous` it can be rolled back.
fn record(manifest: &str, version: &str, previous: Option<(&str, &str)>, steps: J) -> Fields {
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
        f.insert("rollback_steps".into(), steps);
    }
    f
}

fn put(store: &Store, id: &str, f: &Fields) {
    store.backend().insert(TABLE, id, f).unwrap();
}

fn stored_version(store: &Store, id: &str) -> String {
    store.backend().get(TABLE, id).unwrap().unwrap()["version"]
        .as_str()
        .unwrap()
        .to_string()
}

/// A process: the dispatcher with the admin module and an installer over it, as `App::new` makes
/// them.
struct Boot {
    d: Arc<Dispatcher>,
    inst: Arc<Installer>,
}

fn boot(store: &Store) -> Boot {
    let d = Arc::new(Dispatcher::new());
    let inst = Installer::new(store.clone(), &d, vec![admin_module()], Arc::new(Bundled));
    register_admin(&d, &inst);
    Boot { d, inst }
}

fn squatter() -> Principal {
    Principal::user("squatter", &[SQUAT])
}

impl Boot {
    fn call(&self, p: &Principal, id: &str, pairs: &[(&str, &str)]) -> Result<Response, CallError> {
        self.d
            .call(Surface::Rest, p, &mut Vault::new(), id, &text_input(pairs))
    }

    /// The row of the branding, and the events of the log, as they are.
    fn state(&self, store: &Store) -> (Option<Fields>, Vec<(String, String)>) {
        let events = store
            .backend()
            .list("security_event")
            .unwrap()
            .into_iter()
            .map(|(id, f)| (id, f["detail"].as_str().unwrap().to_string()))
            .collect();
        (store.backend().get(MODEL, "instance").unwrap(), events)
    }

    /// What the holder of the package's group can do to the two models: nothing. Every generated
    /// command is unknown (the models have hand-registered commands only), and the row and the log
    /// are as they were.
    fn assert_untouched(&self, store: &Store, what: &str) {
        let before = self.state(store);
        for (model, calls) in [
            (
                MODEL,
                vec![
                    ("create", vec![("name", "Evil Name")]),
                    ("update", vec![("id", "instance"), ("name", "Evil Name")]),
                    ("delete", vec![("id", "instance")]),
                ],
            ),
            (
                "security_event",
                vec![
                    ("create", vec![("kind", "login_refused")]),
                    ("update", vec![("id", "x"), ("kind", "login_refused")]),
                    ("delete", vec![("id", "x")]),
                ],
            ),
        ] {
            for (command, pairs) in calls {
                let id = format!("{model}.{command}");
                for who in [squatter(), Principal::user("ada", &[ADMIN_GROUP, SQUAT])] {
                    let e = self.call(&who, &id, &pairs).unwrap_err();
                    assert!(
                        matches!(e, CallError::UnknownCommand(_)),
                        "{what}: {id} by {}: {e:?}",
                        who.id
                    );
                }
            }
        }
        assert!(!self.d.effects_may_write(MODEL), "{what}");
        assert!(!self.d.effects_may_write("security_event"), "{what}");
        assert_eq!(
            self.state(store),
            before,
            "{what}: row and log as they were"
        );
    }
}

/// Configures the brand as a person, so there is a row and an event to lose.
fn configured(store: &Store) -> Boot {
    let b = boot(store);
    b.call(&person(), "instance_brand.configure", &[("name", "MOHV")])
        .unwrap();
    assert_eq!(b.state(store).1.len(), 1);
    b
}

/// The packages that would get commands over a built-in model: the id is the model's name or the
/// word before its first underscore, as a package names its models.
const SQUATTERS: [(&str, &str); 6] = [
    ("instance", MODEL),
    ("security", "security_event"),
    ("setting", "setting"),
    ("user", "user_account"),
    ("sso", "sso_provider"),
    ("device", "device"),
];

#[test]
fn a_package_stored_with_a_built_in_model_name_is_not_registered_at_start() {
    for (id, model) in SQUATTERS {
        let store = Store::new();
        // An older binary stored it, before the name was taken; a harmless package sits beside it.
        put(
            &store,
            id,
            &record(&package(id, "0.1.0", &[model]), "0.1.0", None, J::Null),
        );
        put(
            &store,
            "fine",
            &record(
                &package("fine", "0.1.0", &["fine_item"]),
                "0.1.0",
                None,
                J::Null,
            ),
        );
        let b = boot(&store);
        let failed = b.inst.load_installed();
        assert_eq!(failed.len(), 1, "{id}: {failed:?}");
        assert_eq!(failed[0].0, id);
        assert!(
            failed[0].1.contains(&format!("`{model}`")) && failed[0].1.contains("not registered"),
            "{id}: {}",
            failed[0].1
        );
        assert!(
            failed[0].1.contains("records are untouched"),
            "{id}: {}",
            failed[0].1
        );
        // The start went on: the other package is there, and the refused one has no command.
        assert!(b.d.has_model("fine_item"), "{id}: the rest of start-up ran");
        assert!(
            !b.d.effects_may_write(model),
            "{id}: no generated create or update over {model}"
        );
        // And the package and its record are still stored.
        assert!(store.backend().get(TABLE, id).unwrap().is_some());
    }
}

#[test]
fn what_a_package_of_the_branding_model_or_the_log_could_do_is_unknown_after_the_load() {
    for (id, model) in [("instance", MODEL), ("security", "security_event")] {
        let store = Store::new();
        let b = configured(&store);
        // Events other than the brand's, so there is something in the log to delete.
        store
            .backend()
            .apply_checked(&[obop_commands::audit::event_op(
                1_700_000_000,
                "ada",
                "login_refused",
                "ada",
                "a refused sign-in",
            )])
            .unwrap();
        // The process that stored the package was an older one: the record is written around.
        put(
            &store,
            id,
            &record(&package(id, "0.1.0", &[model]), "0.1.0", None, J::Null),
        );
        let failed = b.inst.load_installed();
        assert_eq!(failed.len(), 1, "{id}: {failed:?}");
        b.assert_untouched(&store, &format!("{id} after the load"));
        // The brand is still changed by its own command, by a person.
        b.call(&person(), "instance_brand.configure", &[("name", "Acme")])
            .unwrap();
        assert_eq!(b.state(&store).0.unwrap()["name"], "Acme");
    }
}

#[test]
fn a_sync_does_not_register_a_package_another_process_stored_with_a_built_in_model_name() {
    let sync = |b: &Boot, store: &Store| {
        store
            .backend()
            .counter_add("modules.version", 0, false)
            .unwrap();
        b.inst
            .last_check_ms
            .store(0, std::sync::atomic::Ordering::Release);
        b.inst.sync_modules();
    };
    for (id, model) in [("instance", MODEL), ("security", "security_event")] {
        let store = Store::new();
        let b = configured(&store);
        assert!(b.inst.load_installed().is_empty());
        // Another process stores a harmless package and the squatter. The sync is told to look
        // (the control: the harmless one is registered by it).
        put(
            &store,
            "fine",
            &record(
                &package("fine", "0.1.0", &["fine_item"]),
                "0.1.0",
                None,
                J::Null,
            ),
        );
        put(
            &store,
            id,
            &record(&package(id, "0.1.0", &[model]), "0.1.0", None, J::Null),
        );
        sync(&b, &store);
        assert!(b.d.has_model("fine_item"), "{id}: the sync ran");
        b.assert_untouched(&store, &format!("{id} after the first sync"));
        // And when the squatter is upgraded elsewhere (the fingerprint of the record moves).
        let mut newer = record(&package(id, "0.2.0", &[model]), "0.2.0", None, J::Null);
        newer.insert("installed_at".into(), J::from(1_700_000_100_u64));
        store.backend().update(TABLE, id, &newer).unwrap();
        sync(&b, &store);
        b.assert_untouched(&store, &format!("{id} after the sync of an upgrade"));
    }
}

#[test]
fn a_rollback_to_a_manifest_with_a_built_in_model_name_is_refused_and_changes_nothing() {
    for (id, model, other) in [
        ("instance", MODEL, "instance_other"),
        ("security", "security_event", "security_other"),
    ] {
        let store = Store::new();
        let b = configured(&store);
        // Version 0.2.0 is harmless; the earlier 0.1.0, stored by an older binary, declared the
        // model.
        put(
            &store,
            id,
            &record(
                &package(id, "0.2.0", &[other]),
                "0.2.0",
                Some((&package(id, "0.1.0", &[model]), "0.1.0")),
                json!([]),
            ),
        );
        assert!(b.inst.load_installed().is_empty());
        assert!(b.d.has_model(other));
        let before = b.state(&store);
        let e = b.inst.rollback(id, "root").unwrap_err();
        assert!(
            e.contains(&format!("`{model}`")) && e.contains("not registered"),
            "{id}: {e}"
        );
        assert_eq!(stored_version(&store, id), "0.2.0", "{id}: the record");
        assert!(b.d.has_model(other), "{id}: the version in force stays");
        b.assert_untouched(&store, &format!("{id} after the rollback"));
        // Only the module_* events of the installer could be new; the brand's are what they were.
        assert_eq!(b.state(&store), before);
    }
}

#[test]
fn an_upgrade_that_adds_a_built_in_model_name_is_refused_by_the_plan_and_by_the_install() {
    for (id, model, other) in [
        ("instance", MODEL, "instance_other"),
        ("security", "security_event", "security_other"),
    ] {
        let store = Store::new();
        let b = configured(&store);
        b.inst
            .install(&package(id, "0.1.0", &[other]), "upload", "root")
            .unwrap();
        let before = b.state(&store);
        let upgrade = package(id, "0.2.0", &[other, model]);
        // The plan says so, as a conflict of that package.
        let (plan, _) = b
            .inst
            .plan(crate::installer::Request::Manifest(&upgrade))
            .unwrap();
        assert!(!plan.ok(), "{id}: the plan has a blocker");
        assert!(
            plan.blockers.iter().any(|x| x.module == id
                && x.kind == "conflict"
                && x.message == format!("model {model} already exists")),
            "{id}: {:?}",
            plan.blockers
                .iter()
                .map(|x| (&x.kind, &x.message))
                .collect::<Vec<_>>()
        );
        // And so does the install; nothing was changed.
        let e = b.inst.install(&upgrade, "upload", "root").unwrap_err();
        assert!(
            e.contains(&format!("model {model} already exists")),
            "{id}: {e}"
        );
        assert_eq!(stored_version(&store, id), "0.1.0");
        assert!(b.d.has_model(other));
        b.assert_untouched(&store, &format!("{id} after the refused upgrade"));
        assert_eq!(b.state(&store), before);
        // A new install of the same name is refused in the same words.
        let e = boot(&Store::new())
            .inst
            .install(&package(id, "0.1.0", &[model]), "upload", "root")
            .unwrap_err();
        assert!(
            e.contains(&format!("model {model} already exists")),
            "{id}: {e}"
        );
    }
}

/// The names are not a list kept in the installer: a model the dispatcher already had when the
/// installer was made is reserved too, whether or not a built-in module was handed to the installer
/// with it (the SIE exchange of the binary is registered that way).
#[test]
fn a_model_registered_before_the_installer_is_reserved_without_being_in_the_builtin_list() {
    const OWNER: &str = "ledger_owner.user";
    let ledger = Module {
        id: "ledger_core".into(),
        name: "Ledger".into(),
        models: vec![ModelDef {
            name: "ledger",
            description: "The ledger of the program",
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
    d.register_crud(&ledger, &store);
    // The module is not given to the installer as a built-in one.
    let inst = Installer::new(store.clone(), &d, vec![admin_module()], Arc::new(Bundled));
    register_admin(&d, &inst);
    put(
        &store,
        "ledger",
        &record(
            &package("ledger", "0.1.0", &["ledger"]),
            "0.1.0",
            None,
            J::Null,
        ),
    );
    let failed = inst.load_installed();
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert!(failed[0].1.contains("`ledger`"), "{}", failed[0].1);
    assert_eq!(
        d.model_def("ledger").unwrap().write_group,
        OWNER,
        "the model is still the program's, not the package's"
    );
    // A new install of that name is refused too.
    let e = inst
        .install(&package("ledger", "0.1.0", &["ledger"]), "upload", "root")
        .unwrap_err();
    assert!(e.contains("model ledger already exists"), "{e}");
}

/// `device` keeps being refused where the installer is made without the admin module among its
/// built-in ones (the tests of other crates make it so): its name is reserved by itself.
#[test]
fn the_device_name_is_reserved_also_where_the_admin_module_is_not_a_built_in_one() {
    let store = Store::new();
    put(
        &store,
        "device",
        &record(
            &package("device", "0.1.0", &["device"]),
            "0.1.0",
            None,
            J::Null,
        ),
    );
    let d = Arc::new(Dispatcher::new());
    let inst = Installer::new(store.clone(), &d, vec![], Arc::new(Bundled));
    let failed = inst.load_installed();
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert!(
        failed[0].1.contains("`device`") && failed[0].1.contains("devices of mobile apps"),
        "{}",
        failed[0].1
    );
    assert!(!d.has_model("device"));
}
