//! Installing, upgrading and removing packages at runtime, and the admin commands that expose
//! it through the same dispatcher as everything else (REST, MCP and CLI, with agents limited
//! to proposing).

use crate::manifest::Manifest;
use crate::marketplace::Marketplace;
use obop_commands::{ADMIN_GROUP, ANY_GROUP, Dispatcher, Store, Value};
use obop_registry::{CommandDef, FieldDef, ModelDef, Module, intern};
use obop_storage::{Backend, Fields};
use obop_types::DataClass;
use serde_json::Value as J;
use std::collections::BTreeMap;
use std::sync::{Arc, Weak};
use std::time::{SystemTime, UNIX_EPOCH};

const TABLE: &str = "obop_package";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    /// `builtin`, `upload` or `marketplace`.
    pub source: String,
    pub models: Vec<String>,
    pub groups: Vec<String>,
}

pub struct Installer {
    backend: Arc<dyn Backend>,
    store: Store,
    dispatcher: Weak<Dispatcher>,
    builtin: Vec<Module>,
    market: Arc<dyn Marketplace>,
}

fn version_tuple(v: &str) -> (u32, u32, u32) {
    let mut p = v.split('.').map(|x| x.parse().unwrap_or(0));
    (
        p.next().unwrap_or(0),
        p.next().unwrap_or(0),
        p.next().unwrap_or(0),
    )
}

impl Installer {
    pub fn new(
        store: Store,
        dispatcher: &Arc<Dispatcher>,
        builtin: Vec<Module>,
        market: Arc<dyn Marketplace>,
    ) -> Arc<Self> {
        Arc::new(Self {
            backend: store.backend(),
            store,
            dispatcher: Arc::downgrade(dispatcher),
            builtin,
            market,
        })
    }

    fn d(&self) -> Result<Arc<Dispatcher>, String> {
        self.dispatcher
            .upgrade()
            .ok_or_else(|| "server is shutting down".into())
    }

    fn info(m: &Manifest, source: &str) -> PackageInfo {
        PackageInfo {
            id: m.module.id.clone(),
            name: m.module.name.clone(),
            version: m.module.version.clone(),
            description: m.module.description.clone(),
            source: source.into(),
            models: m.model.iter().map(|x| x.name.clone()).collect(),
            groups: m.groups(),
        }
    }

    fn builtin_info(m: &Module) -> PackageInfo {
        PackageInfo {
            id: m.id.clone(),
            name: m.name.clone(),
            version: "built in".into(),
            description: format!("First-party module with {} model(s)", m.models.len()),
            source: "builtin".into(),
            models: m.models.iter().map(|x| x.name.to_string()).collect(),
            groups: m
                .permission_groups()
                .into_iter()
                .map(String::from)
                .collect(),
        }
    }

    fn stored(&self) -> Vec<(String, Fields)> {
        self.backend.list(TABLE).unwrap_or_default()
    }

    pub fn list(&self) -> Vec<PackageInfo> {
        let mut v: Vec<_> = self.builtin.iter().map(Self::builtin_info).collect();
        for (_, f) in self.stored() {
            if let Some(m) = f
                .get("manifest")
                .and_then(J::as_str)
                .and_then(|t| Manifest::parse(t).ok())
            {
                v.push(Self::info(
                    &m,
                    f.get("source").and_then(J::as_str).unwrap_or("upload"),
                ));
            }
        }
        v
    }

    fn register(&self, m: &Manifest, module: &Module) -> Result<(), String> {
        let d = self.d()?;
        // Registers the generated commands and, from the transitions declared in the registry
        // metadata, the guards that govern state fields (D48).
        d.register_crud(module, &self.store);
        for model in &m.model {
            for a in &model.action {
                d.register_transition(module, intern(&model.name), &a.name, &self.store);
            }
        }
        // Public exposure (D50) takes effect only once the models exist; the gate runs again.
        if let Err(e) = d.register_public(&m.module.id, m.public_specs()?) {
            d.unregister_module(&m.module.id);
            return Err(e);
        }
        Ok(())
    }

    /// Validate and install (or upgrade to a newer version of) a package. `source` is `upload`
    /// or `marketplace`; `by` is the caller recorded for audit.
    pub fn install(&self, text: &str, source: &str, by: &str) -> Result<PackageInfo, String> {
        let manifest = Manifest::parse(text)?;
        let module = manifest.to_module()?;
        let id = manifest.module.id.clone();
        if self.builtin.iter().any(|b| b.id == id) {
            return Err(format!("{id} is a built-in module"));
        }
        for dep in &manifest.module.depends {
            if !self.list().iter().any(|p| &p.id == dep) {
                return Err(format!("{id} needs module {dep}, which is not installed"));
            }
        }
        let d = self.d()?;
        let existing = self.backend.get(TABLE, &id).map_err(|e| e.to_string())?;
        if let Some(old) = &existing {
            let old_v = old.get("version").and_then(J::as_str).unwrap_or("0.0.0");
            if version_tuple(&manifest.module.version) <= version_tuple(old_v) {
                return Err(format!(
                    "{id} {old_v} is installed; installing {} is not an upgrade",
                    manifest.module.version
                ));
            }
        }
        for m in &module.models {
            let owned_by_self = existing.is_some();
            if d.has_model(m.name) && !owned_by_self {
                return Err(format!("model {} already exists", m.name));
            }
        }
        for (at, to) in manifest.external_refs() {
            if !d.has_model(&to) {
                return Err(format!("{at} refers to model {to}, which is not installed"));
            }
        }
        if existing.is_some() {
            d.unregister_module(&id);
        }
        self.register(&manifest, &module)?;
        let mut f = Fields::new();
        f.insert("manifest".into(), J::from(text));
        f.insert("version".into(), J::from(manifest.module.version.clone()));
        f.insert("source".into(), J::from(source));
        f.insert("installed_by".into(), J::from(by));
        f.insert(
            "installed_at".into(),
            J::from(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |t| t.as_secs()),
            ),
        );
        if existing.is_some() {
            self.backend.update(TABLE, &id, &f)?;
        } else {
            self.backend.insert(TABLE, &id, &f)?;
        }
        Ok(Self::info(&manifest, source))
    }

    pub fn install_from_marketplace(&self, id: &str, by: &str) -> Result<PackageInfo, String> {
        let text = self.market.fetch(id)?;
        self.install(&text, "marketplace", by)
    }

    /// Remove a package. Its commands disappear at once; its records stay in the database, so a
    /// reinstall brings them back.
    pub fn uninstall(&self, id: &str) -> Result<(), String> {
        if self.builtin.iter().any(|b| b.id == id) {
            return Err(format!("{id} is a built-in module"));
        }
        if self
            .list()
            .iter()
            .any(|p| p.id != id && self.depends_on(&p.id, id))
        {
            return Err(format!("another installed module depends on {id}"));
        }
        if !self.backend.delete(TABLE, id)? {
            return Err(format!("{id} is not installed"));
        }
        self.d()?.unregister_module(id);
        Ok(())
    }

    fn depends_on(&self, pkg: &str, dep: &str) -> bool {
        self.backend
            .get(TABLE, pkg)
            .ok()
            .flatten()
            .and_then(|f| {
                f.get("manifest")
                    .and_then(J::as_str)
                    .and_then(|t| Manifest::parse(t).ok())
            })
            .is_some_and(|m| m.module.depends.iter().any(|d| d == dep))
    }

    /// The instance-wide UI settings (theme, app order): one JSON object kept as text.
    pub fn settings(&self) -> String {
        self.backend
            .get("obop_setting", "instance")
            .ok()
            .flatten()
            .and_then(|f| f.get("value").and_then(J::as_str).map(String::from))
            .unwrap_or_else(|| "{}".into())
    }

    pub fn set_settings(&self, text: &str) -> Result<(), String> {
        if text.len() > 64 * 1024 {
            return Err("settings are larger than 64 KiB".into());
        }
        match serde_json::from_str::<J>(text) {
            Ok(J::Object(_)) => {}
            _ => return Err("settings must be a JSON object".into()),
        }
        let mut f = Fields::new();
        f.insert("value".into(), J::from(text));
        if !self.backend.update("obop_setting", "instance", &f)? {
            self.backend.insert("obop_setting", "instance", &f)?;
        }
        Ok(())
    }

    /// Register every stored package again after a restart. Returns what failed.
    pub fn load_installed(&self) -> Vec<(String, String)> {
        let mut failed = Vec::new();
        for (id, f) in self.stored() {
            let r = f
                .get("manifest")
                .and_then(J::as_str)
                .ok_or_else(|| "no manifest".to_string())
                .and_then(Manifest::parse)
                .and_then(|m| m.to_module().and_then(|module| self.register(&m, &module)));
            if let Err(e) = r {
                failed.push((id, e));
            }
        }
        failed
    }
}

fn s(v: &str) -> Value {
    Value::text(v)
}

fn caller(i: &BTreeMap<String, Value>) -> String {
    match i.get("_caller") {
        Some(Value::Text(t)) => t.clone(),
        _ => "unknown".into(),
    }
}

fn text(i: &BTreeMap<String, Value>, k: &str) -> Result<String, String> {
    match i.get(k) {
        Some(Value::Text(t)) => Ok(t.clone()),
        _ => Err(format!("missing {k}")),
    }
}

fn pkg_row(p: &PackageInfo) -> Value {
    Value::map([
        ("id", s(&p.id)),
        ("name", s(&p.name)),
        ("version", s(&p.version)),
        ("description", s(&p.description)),
        ("source", s(&p.source)),
        (
            "models",
            Value::List(p.models.iter().map(|x| s(x)).collect()),
        ),
        (
            "groups",
            Value::List(p.groups.iter().map(|x| s(x)).collect()),
        ),
    ])
}

/// Registry metadata of the admin commands. Both models are virtual: they have no stored
/// records of their own, only handlers.
pub fn admin_module() -> Module {
    let f =
        |name, description| FieldDef::new(name, description, DataClass::None, false, ADMIN_GROUP);
    Module {
        id: "platform".into(),
        name: "Customize".into(),
        models: vec![
            ModelDef {
                name: "module",
                description: "An installed module: built in, uploaded or from the marketplace",
                read_group: ADMIN_GROUP,
                write_group: ADMIN_GROUP,
                fields: vec![
                    f("name", "Display name"),
                    f("version", "Installed version"),
                    f("description", "What the module does"),
                    f("source", "builtin, upload or marketplace"),
                    f("models", "Models the module adds"),
                    f("groups", "Permission groups the module uses"),
                ],
                commands: vec![
                    CommandDef::new(
                        "install",
                        "Install or upgrade a module from the text of its obop.toml",
                        ADMIN_GROUP,
                    )
                    .approval(),
                    CommandDef::new(
                        "uninstall",
                        "Remove a module; its data stays in the database",
                        ADMIN_GROUP,
                    )
                    .approval(),
                ],
            },
            ModelDef {
                name: "marketplace_listing",
                description: "A module offered by the marketplace",
                read_group: ADMIN_GROUP,
                write_group: ADMIN_GROUP,
                fields: vec![
                    f("name", "Display name"),
                    f("version", "Latest version"),
                    f("description", "What the module does"),
                    f("author", "Who publishes it"),
                    f("source", "Where the install came from"),
                    f("models", "Models the module adds"),
                    f("groups", "Permission groups the module uses"),
                ],
                commands: vec![
                    CommandDef::new(
                        "install",
                        "Install a module from the marketplace by id",
                        ADMIN_GROUP,
                    )
                    .approval(),
                ],
            },
            ModelDef {
                name: "setting",
                description: "Instance-wide interface settings: theme and the app launcher layout",
                read_group: ANY_GROUP,
                write_group: ADMIN_GROUP,
                fields: vec![FieldDef::new(
                    "value",
                    "The settings as a JSON object",
                    DataClass::None,
                    false,
                    ANY_GROUP,
                )],
                commands: vec![],
            },
        ],
        network_destinations: vec![],
    }
}

/// Bind handlers for the admin commands. Called once at start-up.
pub fn register_admin(d: &Dispatcher, inst: &Arc<Installer>) {
    let m = admin_module();
    let i = inst.clone();
    d.register(
        &m,
        "module",
        "list",
        Box::new(move |_| Ok(Value::List(i.list().iter().map(pkg_row).collect()))),
    );
    let i = inst.clone();
    d.register(
        &m,
        "module",
        "get",
        Box::new(move |input| {
            let Value::Map(x) = input else {
                return Err("expected an object".into());
            };
            let id = text(x, "id")?;
            i.list()
                .iter()
                .find(|p| p.id == id)
                .map(pkg_row)
                .ok_or_else(|| format!("no module {id}"))
        }),
    );
    let i = inst.clone();
    d.register(
        &m,
        "module",
        "install",
        Box::new(move |input| {
            let Value::Map(x) = input else {
                return Err("expected an object".into());
            };
            i.install(&text(x, "manifest")?, "upload", &caller(x))
                .map(|p| pkg_row(&p))
        }),
    );
    let i = inst.clone();
    d.register(
        &m,
        "module",
        "uninstall",
        Box::new(move |input| {
            let Value::Map(x) = input else {
                return Err("expected an object".into());
            };
            let id = text(x, "id")?;
            i.uninstall(&id)
                .map(|()| Value::map([("id", s(&id)), ("uninstalled", Value::Bool(true))]))
        }),
    );
    let i = inst.clone();
    d.register(
        &m,
        "setting",
        "get",
        Box::new(move |_| {
            Ok(Value::map([
                ("id", s("instance")),
                ("value", s(&i.settings())),
            ]))
        }),
    );
    let i = inst.clone();
    d.register(
        &m,
        "setting",
        "update",
        Box::new(move |input| {
            let Value::Map(x) = input else {
                return Err("expected an object".into());
            };
            let v = text(x, "value")?;
            i.set_settings(&v)?;
            Ok(Value::map([("id", s("instance")), ("value", s(&v))]))
        }),
    );
    let i = inst.clone();
    d.register(
        &m,
        "marketplace_listing",
        "list",
        Box::new(move |_| {
            let listings = i.market.list()?;
            Ok(Value::List(
                listings
                    .iter()
                    .map(|l| {
                        Value::map([
                            ("id", s(&l.id)),
                            ("name", s(&l.name)),
                            ("version", s(&l.version)),
                            ("description", s(&l.description)),
                            ("author", s(&l.author)),
                        ])
                    })
                    .collect(),
            ))
        }),
    );
    let i = inst.clone();
    d.register(
        &m,
        "marketplace_listing",
        "install",
        Box::new(move |input| {
            let Value::Map(x) = input else {
                return Err("expected an object".into());
            };
            i.install_from_marketplace(&text(x, "id")?, &caller(x))
                .map(|p| pkg_row(&p))
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::marketplace::Bundled;
    use obop_commands::{CallError, Principal, Surface, Vault};

    fn setup(store: &Store) -> (Arc<Dispatcher>, Arc<Installer>) {
        let d = Arc::new(Dispatcher::new());
        let inst = Installer::new(store.clone(), &d, vec![admin_module()], Arc::new(Bundled));
        register_admin(&d, &inst);
        (d, inst)
    }

    fn admin() -> Principal {
        Principal::user("root", &[ADMIN_GROUP])
    }

    fn call(
        d: &Dispatcher,
        p: &Principal,
        id: &str,
        input: Value,
    ) -> Result<obop_commands::Response, CallError> {
        d.call(Surface::Rest, p, &mut Vault::new(), id, &input)
    }

    fn by_id(id: &str) -> Value {
        Value::map([("id", Value::text(id))])
    }

    fn id_of(r: &obop_commands::Response) -> String {
        match &r.value {
            Value::Map(m) => match &m["id"] {
                Value::Text(t) => t.clone(),
                _ => panic!(),
            },
            _ => panic!(),
        }
    }

    #[test]
    fn admin_installs_from_marketplace_and_the_surface_appears() {
        let store = Store::new();
        let (d, _) = setup(&store);
        let user = Principal::user("u", &["fleet.user", "fleet.manager"]);
        assert!(matches!(
            call(
                &d,
                &user,
                "fleet_vehicle.list",
                Value::Map(Default::default())
            ),
            Err(CallError::UnknownCommand(_))
        ));

        call(&d, &admin(), "marketplace_listing.install", by_id("fleet")).unwrap();
        let v = call(
            &d,
            &user,
            "fleet_vehicle.create",
            Value::map([
                ("plate", Value::text("ABC-123")),
                ("state", Value::text("available")),
                ("driver", Value::text("Dana Example")),
            ]),
        )
        .unwrap();
        let id = id_of(&v);
        call(&d, &user, "fleet_vehicle.dispatch", by_id(&id)).unwrap();
        // The state machine refuses a second dispatch.
        assert!(matches!(
            call(&d, &user, "fleet_vehicle.dispatch", by_id(&id)),
            Err(CallError::Handler(_))
        ));
        call(&d, &user, "fleet_vehicle.return_to_pool", by_id(&id)).unwrap();
    }

    #[test]
    fn installed_models_get_masking_and_field_rules_for_free() {
        let store = Store::new();
        let (d, _) = setup(&store);
        call(&d, &admin(), "marketplace_listing.install", by_id("fleet")).unwrap();
        let agent = Principal::agent("bot", &["fleet.user"], &["fleet.user"]);
        let owner = Principal::user("u", &["fleet.user", "fleet.manager"]);
        call(
            &d,
            &owner,
            "fleet_vehicle.create",
            Value::map([
                ("plate", Value::text("ABC-123")),
                ("driver", Value::text("Dana Example")),
            ]),
        )
        .unwrap();
        let r = call(
            &d,
            &agent,
            "fleet_vehicle.list",
            Value::Map(Default::default()),
        )
        .unwrap();
        let Value::List(rows) = r.value else { panic!() };
        let Value::Map(row) = &rows[0] else { panic!() };
        assert_eq!(row["plate"], Value::text("ABC-123"));
        assert!(matches!(&row["driver"], Value::Text(t) if t.starts_with("DRIVER_")));
        // Agents cannot write: fleet.manager is not in their scope.
        assert!(matches!(
            call(
                &d,
                &agent,
                "fleet_vehicle.create",
                Value::map([("plate", Value::text("X"))])
            ),
            Err(CallError::Forbidden(_))
        ));
    }

    #[test]
    fn agents_only_propose_installs() {
        let store = Store::new();
        let (d, _) = setup(&store);
        let agent = Principal::agent("bot", &[ADMIN_GROUP], &[ADMIN_GROUP]);
        let Err(CallError::ApprovalRequired { plan_id }) =
            call(&d, &agent, "marketplace_listing.install", by_id("fleet"))
        else {
            panic!()
        };
        assert!(!d.has_model("fleet_vehicle"));
        d.approve(&admin(), &mut Vault::new(), &plan_id).unwrap();
        assert!(d.has_model("fleet_vehicle"));
    }

    #[test]
    fn everyone_reads_settings_but_only_admins_write_them() {
        let store = Store::new();
        let (d, inst) = setup(&store);
        let user = Principal::user("u", &["base.user"]);
        let read = |p: &Principal| call(&d, p, "setting.get", by_id("instance"));
        let v = read(&user).unwrap().value;
        assert!(matches!(v, Value::Map(m) if m["value"] == Value::text("{}")));
        let set = |p: &Principal, v: &str| {
            call(
                &d,
                p,
                "setting.update",
                Value::map([("id", Value::text("instance")), ("value", Value::text(v))]),
            )
        };
        assert!(matches!(set(&user, "{}"), Err(CallError::Forbidden(_))));
        set(&admin(), r##"{"theme":{"accent":"#6d3fd0"}}"##).unwrap();
        assert!(inst.settings().contains("#6d3fd0"));
        assert!(
            matches!(read(&user).unwrap().value, Value::Map(m) if matches!(&m["value"], Value::Text(t) if t.contains("accent")))
        );
        assert!(matches!(set(&admin(), "[1]"), Err(CallError::Handler(_))));
        assert!(matches!(
            set(&admin(), "not json"),
            Err(CallError::Handler(_))
        ));
        // An agent scoped to base.user can still read them, and cannot write.
        let bot = Principal::agent("bot", &["base.user"], &["base.user"]);
        assert!(read(&bot).is_ok());
        assert!(matches!(set(&bot, "{}"), Err(CallError::Forbidden(_))));
    }

    #[test]
    fn meta_lists_only_what_the_principal_can_see() {
        let store = Store::new();
        let (d, inst) = setup(&store);
        inst.install(
            include_str!("../../../../packages/fleet.toml"),
            "marketplace",
            "root",
        )
        .unwrap();
        let none = Principal::user("n", &["base.user"]);
        assert!(
            d.meta(Surface::Rest, &none).iter().all(|m| m.id != "fleet"),
            "no fleet group, no fleet app"
        );
        let viewer = Principal::user("v", &["fleet.user"]);
        let fleet = d
            .meta(Surface::Rest, &viewer)
            .into_iter()
            .find(|m| m.id == "fleet")
            .unwrap();
        assert_eq!(fleet.name, "Fleet");
        let model = &fleet.models[0];
        assert_eq!(model.name, "fleet_vehicle");
        assert!(!model.can_create, "fleet.manager writes");
        assert!(model.actions.is_empty(), "dispatch needs fleet.manager too");
        let manager = Principal::user("m", &["fleet.user", "fleet.manager"]);
        let fleet = d
            .meta(Surface::Rest, &manager)
            .into_iter()
            .find(|m| m.id == "fleet")
            .unwrap();
        let model = &fleet.models[0];
        assert!(model.can_create && model.can_update);
        assert_eq!(
            model
                .actions
                .iter()
                .map(|a| a.id.as_str())
                .collect::<Vec<_>>(),
            ["fleet_vehicle.dispatch", "fleet_vehicle.return_to_pool"]
        );
        let driver = model.fields.iter().find(|f| f.name == "driver").unwrap();
        assert_eq!((driver.class, driver.personal_data), ("internal", true));
    }

    #[test]
    fn non_admins_cannot_see_or_run_module_commands() {
        let (d, _) = setup(&Store::new());
        let user = Principal::user("u", &["base.user"]);
        assert!(matches!(
            call(
                &d,
                &user,
                "module.install",
                Value::map([("manifest", Value::text("x"))])
            ),
            Err(CallError::Forbidden(_))
        ));
        assert_eq!(
            d.visible(Surface::Rest, &user),
            ["setting.get"],
            "only the settings everyone may read"
        );
    }

    #[test]
    fn upload_is_validated_and_clients_cannot_spoof_the_caller() {
        let store = Store::new();
        let (d, inst) = setup(&store);
        let bad = Value::map([("manifest", Value::text("[module]\nid = \"x\""))]);
        assert!(matches!(
            call(&d, &admin(), "module.install", bad),
            Err(CallError::Handler(_))
        ));
        let fleet = include_str!("../../../../packages/fleet.toml");
        let input = Value::map([
            ("manifest", Value::text(fleet)),
            ("_caller", Value::text("someone-else")),
        ]);
        call(&d, &admin(), "module.install", input).unwrap();
        let rec = store.backend().get(TABLE, "fleet").unwrap().unwrap();
        assert_eq!(rec["installed_by"], "root");
        assert_eq!(
            inst.list().iter().filter(|p| p.source == "upload").count(),
            1
        );
    }

    #[test]
    fn uninstall_removes_commands_but_keeps_data_and_reinstall_restores_it() {
        let store = Store::new();
        let (d, _) = setup(&store);
        let owner = Principal::user("u", &["fleet.user", "fleet.manager"]);
        call(&d, &admin(), "marketplace_listing.install", by_id("fleet")).unwrap();
        call(
            &d,
            &owner,
            "fleet_vehicle.create",
            Value::map([("plate", Value::text("KEEP-1"))]),
        )
        .unwrap();
        call(&d, &admin(), "module.uninstall", by_id("fleet")).unwrap();
        assert!(!d.has_model("fleet_vehicle"));
        assert!(matches!(
            call(
                &d,
                &owner,
                "fleet_vehicle.list",
                Value::Map(Default::default())
            ),
            Err(CallError::UnknownCommand(_))
        ));
        call(&d, &admin(), "marketplace_listing.install", by_id("fleet")).unwrap();
        let r = call(
            &d,
            &owner,
            "fleet_vehicle.list",
            Value::Map(Default::default()),
        )
        .unwrap();
        let Value::List(rows) = r.value else { panic!() };
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn collisions_builtins_and_downgrades_are_refused() {
        let store = Store::new();
        let (d, inst) = setup(&store);
        let fleet = include_str!("../../../../packages/fleet.toml");
        assert!(
            inst.install(
                &fleet
                    .replace("id = \"fleet\"", "id = \"platform\"")
                    .replace("fleet_vehicle", "platform_vehicle"),
                "upload",
                "r"
            )
            .unwrap_err()
            .contains("built-in")
        );
        inst.install(fleet, "upload", "r").unwrap();
        assert!(
            inst.install(fleet, "upload", "r")
                .unwrap_err()
                .contains("not an upgrade")
        );
        let newer = fleet.replace("version = \"0.2.0\"", "version = \"0.3.0\"");
        assert_eq!(
            inst.install(&newer, "upload", "r").unwrap().version,
            "0.3.0"
        );
        assert!(
            inst.install(fleet, "upload", "r")
                .unwrap_err()
                .contains("not an upgrade")
        );
        assert!(d.has_model("fleet_vehicle"));
        let needs = fleet
            .replace("id = \"fleet\"", "id = \"other\"")
            .replace("fleet_vehicle", "other_vehicle")
            .replace(
                "description = \"Vehicles",
                "depends = [\"ghost\"]\ndescription = \"Vehicles",
            );
        assert!(
            inst.install(&needs, "upload", "r")
                .unwrap_err()
                .contains("needs module ghost")
        );
    }

    #[test]
    fn packages_survive_a_restart() {
        let store = Store::new();
        let (_first, inst) = setup(&store);
        inst.install(
            include_str!("../../../../packages/helpdesk.toml"),
            "marketplace",
            "root",
        )
        .unwrap();
        let (d2, inst2) = setup(&store);
        assert!(!d2.has_model("helpdesk_ticket"));
        assert!(inst2.load_installed().is_empty());
        assert!(d2.has_model("helpdesk_ticket"));
    }

    #[test]
    fn approval_flagged_actions_make_agents_propose() {
        let store = Store::new();
        let (d, inst) = setup(&store);
        inst.install(
            include_str!("../../../../packages/helpdesk.toml"),
            "marketplace",
            "root",
        )
        .unwrap();
        // `create` only starts a ticket in its first state (D48), so put one in `resolved`
        // the way an import would: straight into the store.
        let id = store
            .insert(
                "helpdesk_ticket",
                [
                    ("subject".to_string(), Value::text("printer")),
                    ("state".to_string(), Value::text("resolved")),
                ]
                .into(),
            )
            .unwrap();
        let bot = Principal::agent("bot", &["helpdesk.manager"], &["helpdesk.manager"]);
        assert!(matches!(
            call(&d, &bot, "helpdesk_ticket.close", by_id(&id)),
            Err(CallError::ApprovalRequired { .. })
        ));
    }

    #[test]
    fn installed_models_validate_field_kinds_and_actions_keep_working() {
        let store = Store::new();
        let (d, _) = setup(&store);
        call(
            &d,
            &admin(),
            "marketplace_listing.install",
            by_id("projects"),
        )
        .unwrap();
        let u = Principal::user("u", &["projects.user", "projects.manager"]);
        let msg = |r: Result<obop_commands::Response, CallError>| match r {
            Err(CallError::Handler(m)) => m,
            other => panic!("expected a handler error, got {other:?}"),
        };
        // A required field, a select outside its options and a dangling ref are all refused.
        let task = |extra: Vec<(&str, Value)>| {
            let mut v = vec![("title", Value::text("Write docs"))];
            v.extend(extra);
            Value::Map(v.into_iter().map(|(k, x)| (k.to_string(), x)).collect())
        };
        assert!(
            msg(call(
                &d,
                &u,
                "projects_task.create",
                Value::map([("assignee", Value::text("x"))])
            ))
            .contains("projects_task.title is required")
        );
        assert!(
            msg(call(
                &d,
                &u,
                "projects_task.create",
                task(vec![("state", Value::text("blocked"))])
            ))
            .contains("one of: todo, doing, done")
        );
        assert!(
            msg(call(
                &d,
                &u,
                "projects_task.create",
                task(vec![("project_id", Value::text("ghost"))])
            ))
            .contains("there is no projects_project ghost")
        );
        let p = call(
            &d,
            &u,
            "projects_project.create",
            Value::map([
                ("name", Value::text("Site")),
                ("state", Value::text("planned")),
            ]),
        )
        .unwrap();
        let t = call(
            &d,
            &u,
            "projects_task.create",
            task(vec![
                ("project_id", Value::text(&id_of(&p))),
                ("state", Value::text("todo")),
            ]),
        )
        .unwrap();
        // The transition action still works and lands on an allowed option.
        call(&d, &u, "projects_task.begin", by_id(&id_of(&t))).unwrap();
        assert_eq!(
            store.field("projects_task", &id_of(&t), "state"),
            Some(Value::text("doing"))
        );
    }

    fn handler_error(r: Result<obop_commands::Response, CallError>) -> String {
        match r {
            Err(CallError::Handler(m)) => m,
            other => panic!("expected a handler error, got {other:?}"),
        }
    }

    #[test]
    fn installed_packages_govern_their_state_fields() {
        let store = Store::new();
        let (d, inst) = setup(&store);
        inst.install(
            include_str!("../../../../packages/helpdesk.toml"),
            "marketplace",
            "root",
        )
        .unwrap();
        let agent = Principal::user("a", &["helpdesk.agent"]);
        let manager = Principal::user("m", &["helpdesk.agent", "helpdesk.manager"]);
        let bot = Principal::agent(
            "bot",
            &["helpdesk.agent", "helpdesk.manager"],
            &["helpdesk.agent", "helpdesk.manager"],
        );
        // The state is required, yet may be left out: it defaults to the first option.
        let id = id_of(
            &call(
                &d,
                &agent,
                "helpdesk_ticket.create",
                Value::map([("subject", Value::text("printer"))]),
            )
            .unwrap(),
        );
        assert_eq!(
            store.field("helpdesk_ticket", &id, "state"),
            Some(Value::text("open"))
        );
        let e = handler_error(call(
            &d,
            &agent,
            "helpdesk_ticket.create",
            Value::map([
                ("subject", Value::text("x")),
                ("state", Value::text("closed")),
            ]),
        ));
        assert!(e.contains("starts as \"open\""), "{e}");
        let set = |p: &Principal, st: &str| {
            call(
                &d,
                p,
                "helpdesk_ticket.update",
                Value::Map(
                    [
                        ("id".to_string(), Value::text(&id)),
                        ("state".to_string(), Value::text(st)),
                    ]
                    .into(),
                ),
            )
        };
        // open -> closed skips two steps; the message names where it can go.
        let e = handler_error(set(&agent, "closed"));
        assert!(e.contains("it can only go to: in_progress"), "{e}");
        set(&agent, "in_progress").unwrap();
        set(&agent, "resolved").unwrap();
        // Closing needs helpdesk.manager and, for an agent, an approval.
        let e = handler_error(set(&agent, "closed"));
        assert!(
            e.contains("close action") && e.contains("helpdesk.manager"),
            "{e}"
        );
        let Err(CallError::ApprovalRequired { plan_id }) = set(&bot, "closed") else {
            panic!("an agent only proposes the implicit close")
        };
        assert_eq!(
            store.field("helpdesk_ticket", &id, "state"),
            Some(Value::text("resolved"))
        );
        d.approve(&manager, &mut Vault::new(), &plan_id).unwrap();
        assert_eq!(
            store.field("helpdesk_ticket", &id, "state"),
            Some(Value::text("closed"))
        );
        // No update leaves a final state either.
        let e = handler_error(set(&manager, "open"));
        assert!(e.contains("no transition leaves \"closed\""), "{e}");
    }

    #[test]
    fn an_action_refuses_a_record_with_an_empty_required_field() {
        let store = Store::new();
        let (d, inst) = setup(&store);
        inst.install(
            include_str!("../../../../packages/helpdesk.toml"),
            "marketplace",
            "root",
        )
        .unwrap();
        let agent = Principal::user("a", &["helpdesk.agent"]);
        // A record imported without its required subject.
        let id = store
            .insert(
                "helpdesk_ticket",
                [("state".to_string(), Value::text("open"))].into(),
            )
            .unwrap();
        let e = handler_error(call(&d, &agent, "helpdesk_ticket.take", by_id(&id)));
        assert!(e.contains("helpdesk_ticket.subject is required"), "{e}");
        assert_eq!(
            store.field("helpdesk_ticket", &id, "state"),
            Some(Value::text("open"))
        );
        call(
            &d,
            &agent,
            "helpdesk_ticket.update",
            Value::map([("id", Value::text(&id)), ("subject", Value::text("fixed"))]),
        )
        .unwrap();
        call(&d, &agent, "helpdesk_ticket.take", by_id(&id)).unwrap();
    }

    #[test]
    fn installed_models_get_delete_and_search_and_the_ref_rule() {
        let store = Store::new();
        let (d, inst) = setup(&store);
        inst.install(
            include_str!("../../../../packages/helpdesk.toml"),
            "marketplace",
            "root",
        )
        .unwrap();
        let manager = Principal::user("m", &["helpdesk.agent", "helpdesk.manager"]);
        let sla = id_of(
            &call(
                &d,
                &manager,
                "helpdesk_sla.create",
                Value::map([
                    ("name", Value::text("Gold")),
                    ("priority", Value::text("high")),
                    ("first_response_minutes", Value::Int(10)),
                    ("resolution_minutes", Value::Int(60)),
                ]),
            )
            .unwrap(),
        );
        let ticket = id_of(
            &call(
                &d,
                &manager,
                "helpdesk_ticket.create",
                Value::map([
                    ("subject", Value::text("t1")),
                    ("sla_id", Value::text(&sla)),
                ]),
            )
            .unwrap(),
        );
        let e = handler_error(call(&d, &manager, "helpdesk_sla.delete", by_id(&sla)));
        assert!(
            e.contains("helpdesk_ticket (1)") && e.contains(&ticket),
            "{e}"
        );
        // An agent only proposes the delete; it still needs a human.
        let bot = Principal::agent("b", &["helpdesk.agent"], &["helpdesk.agent"]);
        assert!(matches!(
            call(&d, &bot, "helpdesk_ticket.delete", by_id(&ticket)),
            Err(CallError::ApprovalRequired { .. })
        ));
        call(&d, &manager, "helpdesk_ticket.delete", by_id(&ticket)).unwrap();
        call(&d, &manager, "helpdesk_sla.delete", by_id(&sla)).unwrap();
        let r = call(
            &d,
            &manager,
            "helpdesk_ticket.search",
            Value::map([("limit", Value::Int(10))]),
        )
        .unwrap();
        assert_eq!(
            r.value,
            Value::map([("rows", Value::List(vec![])), ("total", Value::Int(0))])
        );
    }

    #[test]
    fn refs_to_models_outside_the_package_need_them_installed() {
        let store = Store::new();
        let (d, inst) = setup(&store);
        let pkg = r#"
[module]
id = "notes"
version = "0.1.0"
name = "Notes"
description = "Notes about customers"

[[model]]
name = "notes_note"
description = "A note"
read_group = "notes.user"
write_group = "notes.user"

  [[model.field]]
  name = "party_id"
  description = "Who the note is about"
  class = "none"
  kind = "ref"
  to = "party"
"#;
        let e = inst.install(pkg, "upload", "root").unwrap_err();
        assert!(
            e.contains("refers to model party, which is not installed"),
            "{e}"
        );
        // Once a model of that name is registered, the same package installs.
        let party = Module {
            id: "base".into(),
            name: "Contacts".into(),
            models: vec![ModelDef {
                name: "party",
                description: "p",
                read_group: "base.user",
                write_group: "base.user",
                fields: vec![],
                commands: vec![],
            }],
            network_destinations: vec![],
        };
        d.register_crud(&party, &store);
        inst.install(pkg, "upload", "root").unwrap();
    }
}
