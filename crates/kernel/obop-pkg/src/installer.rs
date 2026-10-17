//! Installing, upgrading and removing packages at runtime, and the admin commands that expose
//! it through the same dispatcher as everything else (REST, MCP and CLI, with agents limited
//! to proposing).
//!
//! An install is a plan (D55): the module asked for, plus the dependencies it needs from the
//! marketplace, in dependency order, each checked against the running core and against the
//! installed modules that depend on it. The plan is applied as one operation: if a step fails,
//! the steps already applied are undone.

use crate::manifest::Manifest;
use crate::marketplace::{Bundled, Listing, Marketplace};
use crate::resolve::{self, Plan, StepAction, World};
use crate::semver::Version;
use crate::views::{self, Ctx};
use obop_commands::{ADMIN_GROUP, ANY_GROUP, Dispatcher, Store, Value};
use obop_registry::{CommandDef, FieldDef, FieldKind, ModelDef, Module, ModuleMeta, intern};
use obop_storage::{Backend, Fields};
use obop_types::DataClass;
use serde_json::Value as J;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, Weak};
use std::time::{SystemTime, UNIX_EPOCH};

const TABLE: &str = "obop_package";

#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    /// `builtin`, `upload` or `marketplace`.
    pub source: String,
    pub models: Vec<String>,
    pub groups: Vec<String>,
    /// Catalogue metadata (D55); for a built-in module declared in code.
    pub meta: ModuleMeta,
    /// Unix seconds; none for a built-in module.
    pub installed_at: Option<u64>,
    pub installed_by: Option<String>,
    /// The registry view of the module, for the contents summary.
    pub module: Module,
}

/// What an install did, step by step.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// The module that was asked for.
    pub root: PackageInfo,
    /// Every module the plan installed or upgraded, dependencies first.
    pub applied: Vec<(PackageInfo, StepAction)>,
    pub warnings: Vec<String>,
}

pub struct Installer {
    backend: Arc<dyn Backend>,
    store: Store,
    dispatcher: Weak<Dispatcher>,
    builtin: Vec<Module>,
    market: Arc<dyn Marketplace>,
    /// Plans are applied one at a time.
    apply: Mutex<()>,
}

/// What it takes to put a step back.
struct Undo {
    id: String,
    previous: Option<Fields>,
}

/// Where a plan starts.
pub enum Request<'a> {
    /// The text of an `obop.toml` (an upload).
    Manifest(&'a str),
    /// A package of the marketplace by id.
    Market(&'a str),
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |t| t.as_secs())
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
            apply: Mutex::new(()),
        })
    }

    fn d(&self) -> Result<Arc<Dispatcher>, String> {
        self.dispatcher
            .upgrade()
            .ok_or_else(|| "server is shutting down".into())
    }

    /// The running core version.
    pub fn core_version(&self) -> Version {
        Version::parse(obop_types::CORE_VERSION).unwrap_or_else(|_| Version::new(0, 0, 0))
    }

    fn info(m: &Manifest, source: &str, at: Option<u64>, by: Option<String>) -> PackageInfo {
        let module = m.to_module().unwrap_or_default();
        PackageInfo {
            id: m.module.id.clone(),
            name: m.module.name.clone(),
            version: m.module.version.clone(),
            description: m.module.description.clone(),
            source: source.into(),
            models: m.model.iter().map(|x| x.name.clone()).collect(),
            groups: m.groups(),
            meta: m.module_meta(),
            installed_at: at,
            installed_by: by,
            module,
        }
    }

    fn builtin_info(m: &Module) -> PackageInfo {
        let mut meta = m.meta.clone();
        if meta.version.is_empty() {
            meta.version = obop_types::CORE_VERSION.to_string();
        }
        PackageInfo {
            id: m.id.clone(),
            name: m.name.clone(),
            version: meta.version.clone(),
            description: if m.description.is_empty() {
                meta.summary.clone()
            } else {
                m.description.clone()
            },
            source: "builtin".into(),
            models: m.models.iter().map(|x| x.name.to_string()).collect(),
            groups: m
                .permission_groups()
                .into_iter()
                .map(String::from)
                .collect(),
            meta,
            installed_at: None,
            installed_by: None,
            module: m.clone(),
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
                    f.get("installed_at").and_then(J::as_u64),
                    f.get("installed_by").and_then(J::as_str).map(String::from),
                ));
            }
        }
        v
    }

    /// The manifests of the installed packages, by id.
    pub fn installed_manifests(&self) -> BTreeMap<String, Manifest> {
        self.stored()
            .into_iter()
            .filter_map(|(id, f)| {
                f.get("manifest")
                    .and_then(J::as_str)
                    .and_then(|t| Manifest::parse(t).ok())
                    .map(|m| (id, m))
            })
            .collect()
    }

    /// A package installed from the marketplace before it carried translations has none stored.
    /// When the bundled catalog offers the same version with labels, they are added in memory (the
    /// stored text is left alone); a mismatch that fails validation keeps the stored manifest.
    fn with_bundled_labels(text: &str, source: &str) -> Result<Manifest, String> {
        let m = Manifest::parse(text)?;
        if source == "marketplace" && m.labels.is_empty() {
            let labels = crate::marketplace::Bundled::labels_text(&m.module.id);
            let bundled_same = Bundled
                .fetch(&m.module.id)
                .ok()
                .and_then(|t| Manifest::parse(&t).ok())
                .is_some_and(|b| b.module.version == m.module.version);
            if bundled_same
                && !labels.is_empty()
                && let Ok(with) = Manifest::parse(&Manifest::combine(text, &labels))
            {
                return Ok(with);
            }
        }
        Ok(m)
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

    /// What the marketplace offers, by id. An unreachable marketplace offers nothing.
    pub fn offered(&self) -> BTreeMap<String, Listing> {
        self.market
            .list()
            .unwrap_or_default()
            .into_iter()
            .map(|l| (l.id.clone(), l))
            .collect()
    }

    /// The context the views and the resolver need.
    fn ctx_parts(
        &self,
    ) -> (
        Version,
        BTreeMap<String, Manifest>,
        BTreeMap<String, Listing>,
    ) {
        (
            self.core_version(),
            self.installed_manifests(),
            self.offered(),
        )
    }

    fn with_ctx<R>(&self, f: impl FnOnce(&Ctx<'_>) -> R) -> R {
        let (core, installed, offered) = self.ctx_parts();
        f(&Ctx {
            core: &core,
            builtin: &self.builtin,
            installed: &installed,
            offered: &offered,
        })
    }

    /// Fetch and parse a marketplace package; it must satisfy the catalogue rules.
    fn fetch_market(&self, id: &str) -> Result<Manifest, String> {
        let text = self.market.fetch(id)?;
        let m = Manifest::parse(&text)?;
        if m.module.id != id {
            return Err(format!(
                "the marketplace answered {id} with the package {}",
                m.module.id
            ));
        }
        let problems = m.catalog_problems();
        if !problems.is_empty() {
            return Err(format!("marketplace package {id}: {}", problems.join("; ")));
        }
        Ok(m)
    }

    /// The plan for a request, without changing anything. `Err` when the request itself cannot
    /// be read (a manifest that does not parse, a package the marketplace does not have).
    pub fn plan(&self, req: Request<'_>) -> Result<(Plan, String), String> {
        let (root, source) = match req {
            Request::Manifest(t) => (Manifest::parse(t)?, "upload"),
            Request::Market(id) => (self.fetch_market(id)?, "marketplace"),
        };
        root.to_module()?;
        let offered: BTreeSet<String> = self.offered().into_keys().collect();
        let fetch = |id: &str| -> Result<Option<Manifest>, String> {
            if !offered.contains(id) {
                return Ok(None);
            }
            self.fetch_market(id).map(Some)
        };
        let world = World {
            core: self.core_version(),
            builtin: self
                .builtin
                .iter()
                .map(|m| {
                    (
                        m.id.clone(),
                        m.models.iter().map(|x| x.name.to_string()).collect(),
                    )
                })
                .collect(),
            installed: self.installed_manifests(),
            market: &fetch,
            registered: &|m| self.d().is_ok_and(|d| d.has_model(m)),
        };
        let mut plan = resolve::plan_install(&world, &root, source);
        // Models of the built-in modules and of other packages may not be taken.
        let taken: BTreeSet<&str> = self
            .builtin
            .iter()
            .flat_map(|m| m.models.iter().map(|x| x.name))
            .collect();
        for s in &plan.steps {
            if s.action == StepAction::Skip {
                continue;
            }
            for m in &s.manifest.model {
                if taken.contains(m.name.as_str()) {
                    plan.blockers.push(resolve::Blocker {
                        module: s.id.clone(),
                        kind: "conflict",
                        message: format!("model {} already exists", m.name),
                    });
                }
            }
        }
        Ok((plan, root.module.id))
    }

    /// `module_plan.preview` as data: the plan with how each dependency stands.
    pub fn preview(&self, req: Request<'_>) -> Result<Value, String> {
        let (plan, root) = self.plan(req)?;
        Ok(self.with_ctx(|ctx| views::plan_value(&plan, &root, ctx, &[])))
    }

    /// Validate and install (or upgrade to a newer version of) a package, together with the
    /// dependencies it needs from the marketplace. `source` is `upload` or `marketplace`; `by`
    /// is the caller recorded for audit.
    pub fn install(&self, text: &str, source: &str, by: &str) -> Result<PackageInfo, String> {
        self.install_outcome(text, source, by).map(|o| o.root)
    }

    pub fn install_outcome(&self, text: &str, source: &str, by: &str) -> Result<Outcome, String> {
        let manifest = Manifest::parse(text)?;
        if source == "marketplace" {
            let problems = manifest.catalog_problems();
            if !problems.is_empty() {
                return Err(format!(
                    "marketplace package {}: {}",
                    manifest.module.id,
                    problems.join("; ")
                ));
            }
        }
        let plan = self.plan(Request::Manifest(text))?.0;
        self.apply_plan(&plan, &manifest.module.id, source, by)
    }

    pub fn install_from_marketplace(&self, id: &str, by: &str) -> Result<PackageInfo, String> {
        self.install_from_marketplace_outcome(id, by)
            .map(|o| o.root)
    }

    pub fn install_from_marketplace_outcome(&self, id: &str, by: &str) -> Result<Outcome, String> {
        let (plan, root) = self.plan(Request::Market(id))?;
        self.apply_plan(&plan, &root, "marketplace", by)
    }

    /// Apply a plan that has no blockers. The root of an upload keeps `source` `upload`; every
    /// other step comes from the marketplace. If a step fails, the ones already applied are
    /// undone and the error says so.
    pub fn apply_plan(
        &self,
        plan: &Plan,
        root: &str,
        root_source: &str,
        by: &str,
    ) -> Result<Outcome, String> {
        if !plan.ok() {
            return Err(plan
                .blockers
                .iter()
                .map(|b| b.message.clone())
                .collect::<Vec<_>>()
                .join("; "));
        }
        let _one_at_a_time = self.apply.lock().unwrap_or_else(|e| e.into_inner());
        let mut done: Vec<Undo> = Vec::new();
        let mut applied = Vec::new();
        for step in plan.work() {
            let source = if step.id == root {
                root_source
            } else {
                "marketplace"
            };
            match self.apply_step(&step.manifest, source, by) {
                Ok((undo, info)) => {
                    done.push(undo);
                    applied.push((info, step.action));
                }
                Err(e) => {
                    let n = done.len();
                    for u in done.into_iter().rev() {
                        self.undo(u);
                    }
                    return Err(if n == 0 {
                        format!("{}: {e}", step.id)
                    } else {
                        format!(
                            "{}: {e}; the {n} step(s) before it were rolled back and nothing was installed",
                            step.id
                        )
                    });
                }
            }
        }
        let root_info = applied
            .iter()
            .find(|(i, _)| i.id == root)
            .map(|(i, _)| i.clone())
            .or_else(|| self.list().into_iter().find(|p| p.id == root))
            .ok_or_else(|| format!("{root} was not installed"))?;
        Ok(Outcome {
            root: root_info,
            applied,
            warnings: plan.warnings.clone(),
        })
    }

    /// Install or upgrade one package whose checks the plan has done.
    fn apply_step(
        &self,
        manifest: &Manifest,
        source: &str,
        by: &str,
    ) -> Result<(Undo, PackageInfo), String> {
        let module = manifest.to_module()?;
        let id = manifest.module.id.clone();
        if self.builtin.iter().any(|b| b.id == id) {
            return Err(format!("{id} is a built-in module"));
        }
        let d = self.d()?;
        let existing = self.backend.get(TABLE, &id).map_err(|e| e.to_string())?;
        if let Some(old) = &existing {
            let old_v = old.get("version").and_then(J::as_str).unwrap_or("0.0.0");
            let old_v = Version::parse(old_v).unwrap_or_else(|_| Version::new(0, 0, 0));
            if manifest.semver() <= old_v {
                return Err(format!(
                    "{id} {old_v} is installed; installing {} is not an upgrade",
                    manifest.module.version
                ));
            }
        }
        for m in &module.models {
            if d.has_model(m.name) && existing.is_none() {
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
        if let Err(e) = self.register(manifest, &module) {
            if let Some(old) = &existing {
                self.reregister(&id, old);
            }
            return Err(e);
        }
        let at = now();
        let mut f = Fields::new();
        f.insert("manifest".into(), J::from(manifest.text.as_str()));
        f.insert("version".into(), J::from(manifest.module.version.clone()));
        f.insert("source".into(), J::from(source));
        f.insert("installed_by".into(), J::from(by));
        f.insert("installed_at".into(), J::from(at));
        let stored = if existing.is_some() {
            self.backend.update(TABLE, &id, &f).map(|_| ())
        } else {
            self.backend.insert(TABLE, &id, &f).map(|_| ())
        };
        if let Err(e) = stored {
            d.unregister_module(&id);
            if let Some(old) = &existing {
                self.reregister(&id, old);
            }
            return Err(e.to_string());
        }
        Ok((
            Undo {
                id,
                previous: existing,
            },
            Self::info(manifest, source, Some(at), Some(by.to_string())),
        ))
    }

    /// Put the registration of a stored package back (after a failed upgrade or a rollback).
    fn reregister(&self, id: &str, stored: &Fields) {
        let source = stored.get("source").and_then(J::as_str).unwrap_or("upload");
        let r = stored
            .get("manifest")
            .and_then(J::as_str)
            .ok_or_else(|| "no manifest".to_string())
            .and_then(|t| Self::with_bundled_labels(t, source))
            .and_then(|m| m.to_module().and_then(|module| self.register(&m, &module)));
        if let Err(e) = r {
            eprintln!("could not restore module {id}: {e}");
        }
    }

    fn undo(&self, u: Undo) {
        if let Ok(d) = self.d() {
            d.unregister_module(&u.id);
        }
        match u.previous {
            Some(f) => {
                let _ = self.backend.update(TABLE, &u.id, &f);
                self.reregister(&u.id, &f);
            }
            None => {
                let _ = self.backend.delete(TABLE, &u.id);
            }
        }
    }

    /// What stops `id` from being removed: the installed packages that need it.
    pub fn uninstall_blocker(&self, id: &str) -> Option<resolve::Blocker> {
        if self.builtin.iter().any(|b| b.id == id) {
            return Some(resolve::Blocker {
                module: id.into(),
                kind: "builtin",
                message: format!("{id} is a built-in module"),
            });
        }
        resolve::uninstall_blocker(&self.installed_manifests(), id)
    }

    /// Remove a package. Its commands disappear at once; its records stay in the database, so a
    /// reinstall brings them back. Refused while another installed package depends on it.
    pub fn uninstall(&self, id: &str) -> Result<(), String> {
        let _one_at_a_time = self.apply.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(b) = self.uninstall_blocker(id) {
            return Err(b.message);
        }
        if !self.backend.delete(TABLE, id)? {
            return Err(format!("{id} is not installed"));
        }
        self.d()?.unregister_module(id);
        Ok(())
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

    /// Register every stored package again after a restart. Returns what failed. A package that
    /// no longer fits the running core still loads (its data is there); the module page marks
    /// it incompatible.
    pub fn load_installed(&self) -> Vec<(String, String)> {
        let mut failed = Vec::new();
        for (id, f) in self.stored() {
            let source = f.get("source").and_then(J::as_str).unwrap_or("upload");
            let r = f
                .get("manifest")
                .and_then(J::as_str)
                .ok_or_else(|| "no manifest".to_string())
                .and_then(|t| Self::with_bundled_labels(t, source))
                .and_then(|m| m.to_module().and_then(|module| self.register(&m, &module)));
            if let Err(e) = r {
                failed.push((id, e));
            }
        }
        failed
    }

    /// The dependency tree of one module (`obop modules tree ID`), or of every installed package.
    /// A package that is only in the marketplace can be asked for by id too.
    pub fn tree(&self, id: Option<&str>) -> Result<String, String> {
        let (core, installed, offered) = self.ctx_parts();
        let mut out = String::new();
        let roots: Vec<String> = match id {
            Some(i) => vec![i.to_string()],
            None => installed.keys().cloned().collect(),
        };
        if roots.is_empty() {
            return Ok(format!(
                "no packages are installed; core {core} provides {}\n",
                self.builtin
                    .iter()
                    .map(|m| m.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        for r in roots {
            let (version, license, core_req, deps, state) = if let Some(m) = installed.get(&r) {
                (
                    m.module.version.clone(),
                    m.module.license.clone(),
                    m.module.requires_core.clone(),
                    m.requirements(),
                    "installed",
                )
            } else if let Some(b) = self.builtin.iter().find(|b| b.id == r) {
                (
                    core.to_string(),
                    b.meta.license.clone(),
                    b.meta.requires_core.clone(),
                    Vec::new(),
                    "built in",
                )
            } else if let Some(l) = offered.get(&r) {
                (
                    l.version.clone(),
                    l.license.clone(),
                    l.requires_core.clone(),
                    l.stub()?.requirements(),
                    "marketplace, not installed",
                )
            } else {
                return Err(format!("no module {r} is installed or offered"));
            };
            let req = if core_req.is_empty() { "*" } else { &core_req };
            let ok = crate::semver::Req::parse(req).is_ok_and(|q| q.matches(&core));
            out.push_str(&format!(
                "{r} {version}  {}  [{state}]  core {req}: {} (running {core})\n",
                if license.is_empty() {
                    "no licence"
                } else {
                    &license
                },
                if ok { "ok" } else { "NOT COMPATIBLE" }
            ));
            let mut path = vec![r.clone()];
            self.tree_children(&deps, "", &mut path, &installed, &offered, &core, &mut out);
        }
        Ok(out)
    }

    #[allow(clippy::too_many_arguments)]
    fn tree_children(
        &self,
        deps: &[crate::manifest::Requirement],
        prefix: &str,
        path: &mut Vec<String>,
        installed: &BTreeMap<String, Manifest>,
        offered: &BTreeMap<String, Listing>,
        core: &Version,
        out: &mut String,
    ) {
        for (i, d) in deps.iter().enumerate() {
            let last = i + 1 == deps.len();
            let (branch, next) = if last {
                ("└── ", "    ")
            } else {
                ("├── ", "│   ")
            };
            let (status, children) = if self.builtin.iter().any(|b| b.id == d.id) {
                (
                    format!(
                        "built in, core {core}{}",
                        if d.req.matches(core) {
                            ""
                        } else {
                            " - NOT SATISFIED"
                        }
                    ),
                    Vec::new(),
                )
            } else if let Some(m) = installed.get(&d.id) {
                let v = m.semver();
                (
                    format!(
                        "installed {v}{}",
                        if d.req.matches(&v) {
                            ""
                        } else {
                            " - NOT SATISFIED"
                        }
                    ),
                    m.requirements(),
                )
            } else if let Some(l) = offered.get(&d.id) {
                (
                    format!("not installed, marketplace offers {}", l.version),
                    l.stub().map(|s| s.requirements()).unwrap_or_default(),
                )
            } else {
                ("MISSING".to_string(), Vec::new())
            };
            let cycle = path.contains(&d.id);
            out.push_str(&format!(
                "{prefix}{branch}{} {}  [{status}]{}\n",
                d.id,
                d.req,
                if cycle { "  (cycle)" } else { "" }
            ));
            if !cycle && !children.is_empty() {
                path.push(d.id.clone());
                self.tree_children(
                    &children,
                    &format!("{prefix}{next}"),
                    path,
                    installed,
                    offered,
                    core,
                    out,
                );
                path.pop();
            }
        }
    }

    /// A plan as text, for the command line, and whether it can be applied.
    pub fn plan_text(&self, req: Request<'_>) -> Result<(String, bool), String> {
        let (plan, root) = self.plan(req)?;
        let mut out = format!("plan for {root} (core {})\n", self.core_version());
        for (n, s) in plan.steps.iter().enumerate() {
            let from = s
                .from_version
                .as_ref()
                .filter(|_| s.action == StepAction::Upgrade)
                .map(|f| format!(" from {f}"))
                .unwrap_or_default();
            out.push_str(&format!(
                "  {}. {:<8} {} {}{}  {}  ({})\n",
                n + 1,
                s.action.as_str(),
                s.id,
                s.version,
                from,
                if s.license.is_empty() {
                    "no licence"
                } else {
                    &s.license
                },
                s.reason
            ));
        }
        for w in &plan.warnings {
            out.push_str(&format!("  warning: {w}\n"));
        }
        for b in &plan.blockers {
            out.push_str(&format!("  BLOCKED [{}]: {}\n", b.kind, b.message));
        }
        Ok((out, plan.ok()))
    }

    /// The row of one module as `module.list` returns it.
    pub fn module_row(&self, p: &PackageInfo) -> Value {
        self.with_ctx(|ctx| self.module_row_in(p, ctx))
    }

    fn module_row_in(&self, p: &PackageInfo, ctx: &Ctx<'_>) -> Value {
        let mut m = BTreeMap::new();
        let mut put = |k: &str, v: Value| {
            m.insert(k.to_string(), v);
        };
        put("id", views::text(&p.id));
        put("name", views::text(&p.name));
        put("version", views::text(&p.version));
        put("description", views::text(&p.description));
        put("source", views::text(&p.source));
        put(
            "models",
            Value::List(p.models.iter().map(|x| views::text(x)).collect()),
        );
        put(
            "groups",
            Value::List(p.groups.iter().map(|x| views::text(x)).collect()),
        );
        put(
            "installed_at",
            p.installed_at.map_or(Value::Null, |t| Value::Int(t as i64)),
        );
        put(
            "installed_by",
            p.installed_by.as_deref().map_or(Value::Null, views::text),
        );
        put("contents", views::contents(&p.module));
        let blocker = if p.source == "builtin" {
            None
        } else {
            resolve::uninstall_blocker(ctx.installed, &p.id)
        };
        put(
            "uninstallable",
            Value::Bool(p.source != "builtin" && blocker.is_none()),
        );
        put(
            "uninstall_blockers",
            views::blockers_value(&blocker.into_iter().collect::<Vec<_>>()),
        );
        let warnings: Vec<String> = ctx
            .installed
            .get(&p.id)
            .map(Manifest::warnings)
            .unwrap_or_default();
        put(
            "warnings",
            Value::List(warnings.iter().map(|x| views::text(x)).collect()),
        );
        views::put_meta(&mut m, &p.meta, &p.id, &p.version, ctx);
        Value::Map(m)
    }

    /// Every module, built-in ones first, as `module.list` returns them.
    pub fn module_rows(&self) -> Value {
        self.with_ctx(|ctx| {
            Value::List(
                self.list()
                    .iter()
                    .map(|p| self.module_row_in(p, ctx))
                    .collect(),
            )
        })
    }

    /// The marketplace as `marketplace_listing.list` returns it: each listing with what it would
    /// take to install it here.
    pub fn listing_rows(&self) -> Result<Value, String> {
        let listings = self.market.list()?;
        let (core, installed, offered) = self.ctx_parts();
        let ctx = Ctx {
            core: &core,
            builtin: &self.builtin,
            installed: &installed,
            offered: &offered,
        };
        // Resolve against the listings alone (no fetch per row): a stub carries the metadata
        // the resolver needs.
        let stubs: BTreeMap<String, Manifest> = listings
            .iter()
            .filter_map(|l| l.stub().ok().map(|m| (l.id.clone(), m)))
            .collect();
        let fetch = |id: &str| -> Result<Option<Manifest>, String> { Ok(stubs.get(id).cloned()) };
        let world = World {
            core: core.clone(),
            builtin: self
                .builtin
                .iter()
                .map(|m| (m.id.clone(), Vec::new()))
                .collect(),
            installed: installed.clone(),
            market: &fetch,
            registered: &|_| false,
        };
        let rows = listings
            .iter()
            .map(|l| {
                let mut m = BTreeMap::new();
                let mut put = |k: &str, v: Value| {
                    m.insert(k.to_string(), v);
                };
                put("id", views::text(&l.id));
                put("name", views::text(&l.name));
                put("version", views::text(&l.version));
                put("description", views::text(&l.description));
                put("source", views::text("marketplace"));
                let have = installed.get(&l.id);
                put("installed", Value::Bool(have.is_some()));
                put(
                    "installed_version",
                    have.map_or(Value::Null, |h| views::text(&h.module.version)),
                );
                let (installable, blockers) = match stubs.get(&l.id) {
                    Some(stub) => {
                        let plan = resolve::plan_install(&world, stub, "marketplace");
                        (plan.ok(), plan.blockers)
                    }
                    None => (
                        false,
                        vec![resolve::Blocker {
                            module: l.id.clone(),
                            kind: "unavailable",
                            message: format!("the listing for {} is malformed", l.id),
                        }],
                    ),
                };
                put("installable", Value::Bool(installable));
                put("blockers", views::blockers_value(&blockers));
                let meta = ModuleMeta {
                    version: l.version.clone(),
                    summary: l.summary.clone(),
                    author: l.author.clone(),
                    authors: l.authors.clone(),
                    license: l.license.clone(),
                    homepage: l.homepage.clone(),
                    repository: l.repository.clone(),
                    documentation: l.documentation.clone(),
                    support: l.support.clone(),
                    keywords: l.keywords.clone(),
                    categories: l.categories.clone(),
                    icon: l.icon.clone(),
                    requires_core: l.requires_core.clone(),
                    maturity: l.maturity.clone(),
                    dependencies: l
                        .dependencies
                        .iter()
                        .map(|(a, b)| (a.clone(), b.clone()))
                        .collect(),
                };
                // For a listing, "update available" compares the installed version to the
                // offered one, which is this listing.
                views::put_meta(&mut m, &meta, &l.id, &l.version, &ctx);
                let update = have
                    .filter(|h| h.semver() < Version::parse(&l.version).unwrap_or(h.semver()))
                    .map(|_| views::text(&l.version));
                m.insert("update_available".into(), update.unwrap_or(Value::Null));
                Value::Map(m)
            })
            .collect();
        Ok(Value::List(rows))
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

/// What an install command returns: the module and every step of the plan that was applied.
fn outcome_row(o: &Outcome) -> Value {
    let Value::Map(mut m) = pkg_row(&o.root) else {
        unreachable!()
    };
    m.insert(
        "steps".into(),
        Value::List(
            o.applied
                .iter()
                .map(|(p, a)| {
                    Value::map([
                        ("id", s(&p.id)),
                        ("name", s(&p.name)),
                        ("version", s(&p.version)),
                        ("action", s(a.as_str())),
                    ])
                })
                .collect(),
        ),
    );
    m.insert(
        "warnings".into(),
        Value::List(o.warnings.iter().map(|w| s(w)).collect()),
    );
    Value::Map(m)
}

/// `without_dependencies` may only be false: a module cannot be installed without what it needs.
fn refuse_skipped_dependencies(i: &BTreeMap<String, Value>) -> Result<(), String> {
    if matches!(i.get("without_dependencies"), Some(Value::Bool(true))) {
        return Err(
            "required dependencies cannot be skipped: install them together with the module (they are listed by module_plan.preview)"
                .into(),
        );
    }
    Ok(())
}

/// Registry metadata of the admin commands. The models are virtual: they have no stored
/// records of their own, only handlers.
pub fn admin_module() -> Module {
    let f =
        |name, description| FieldDef::new(name, description, DataClass::None, false, ADMIN_GROUP);
    let list = |name, description| f(name, description).kind(FieldKind::Text);
    // Catalogue fields shared by an installed module and a marketplace listing (D55).
    let catalogue = || {
        vec![
            list("summary", "One line about the module, shown on its card"),
            list("author", "Who publishes it"),
            list("authors", "All authors (a list)"),
            list(
                "license",
                "SPDX licence expression, LicenseRef-... or proprietary",
            ),
            list("license_kind", "spdx, custom, proprietary or none"),
            list(
                "license_url",
                "Link to the SPDX page of a single licence identifier",
            ),
            list("homepage", "Link to the module's home page"),
            list("repository", "Link to the source repository"),
            list("documentation", "Link to the documentation"),
            list("support", "Link to where support is given"),
            list("keywords", "Search keywords (a list)"),
            list(
                "categories",
                "Categories: finance, sales, supply-chain, people, services, marketing, productivity, platform, other",
            ),
            list("icon", "Name of a built-in icon"),
            list("maturity", "stable, beta or experimental"),
            list(
                "requires_core",
                "The core versions the module runs on, a semver requirement",
            ),
            f(
                "compatible",
                "Whether the running core satisfies requires_core",
            )
            .kind(FieldKind::Bool),
            list(
                "dependencies",
                "Dependencies with their requirement and how each stands (a list)",
            ),
            list(
                "dependents",
                "Ids of the installed modules that need this one (a list)",
            ),
            list(
                "update_available",
                "The newer version the marketplace offers, if any",
            ),
        ]
    };
    let mut module_fields = vec![
        f("name", "Display name"),
        f("version", "Installed version"),
        f("description", "What the module does"),
        f("source", "builtin, upload or marketplace"),
        f("models", "Models the module adds"),
        f("groups", "Permission groups the module uses"),
    ];
    module_fields.extend(catalogue());
    module_fields.extend([
        list(
            "installed_at",
            "When it was installed (Unix seconds); empty for built-in modules",
        ),
        list("installed_by", "Who installed it"),
        list(
            "contents",
            "Models, fields, actions, approval steps and data classes the module declares",
        ),
        f("uninstallable", "Whether it can be removed now").kind(FieldKind::Bool),
        list(
            "uninstall_blockers",
            "What stops it from being removed (a list)",
        ),
        list(
            "warnings",
            "Notes that do not stop anything, such as a proprietary licence (a list)",
        ),
        list(
            "steps",
            "The modules to install or upgrade, dependencies first (a list)",
        ),
    ]);
    let mut listing_fields = vec![
        f("name", "Display name"),
        f("version", "Latest version"),
        f("description", "What the module does"),
        f("source", "Where the install came from"),
        f("models", "Models the module adds"),
        f("groups", "Permission groups the module uses"),
        f(
            "without_dependencies",
            "Must be false or absent: required dependencies cannot be skipped",
        )
        .kind(FieldKind::Bool),
    ];
    listing_fields.extend(catalogue());
    listing_fields.extend([
        f("installed", "Whether a version is installed").kind(FieldKind::Bool),
        list("installed_version", "The installed version, if any"),
        f("installable", "Whether it can be installed here now").kind(FieldKind::Bool),
        list("blockers", "What stops it from being installed (a list)"),
        list(
            "warnings",
            "Notes that do not stop anything, such as a proprietary licence (a list)",
        ),
        list(
            "steps",
            "The modules to install or upgrade, dependencies first (a list)",
        ),
    ]);
    let m = Module {
        id: "platform".into(),
        name: "Customize".into(),
        models: vec![
            ModelDef {
                name: "module",
                description: "An installed module: built in, uploaded or from the marketplace",
                read_group: ADMIN_GROUP,
                write_group: ADMIN_GROUP,
                fields: module_fields,
                commands: vec![
                    CommandDef::new(
                        "install",
                        "Install or upgrade a module from the text of its obop.toml, with the dependencies it needs",
                        ADMIN_GROUP,
                    )
                    .approval(),
                    CommandDef::new(
                        "uninstall",
                        "Remove a module; its data stays in the database. Refused while another module needs it",
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
                fields: listing_fields,
                commands: vec![
                    CommandDef::new(
                        "install",
                        "Install a module from the marketplace by id, with the dependencies it needs, in one operation",
                        ADMIN_GROUP,
                    )
                    .approval(),
                ],
            },
            ModelDef {
                name: "module_plan",
                description: "What installing a module would do here: the steps in order, blockers and warnings",
                read_group: ADMIN_GROUP,
                write_group: ADMIN_GROUP,
                fields: vec![
                    list("manifest", "Input: the text of an obop.toml to plan (or give id for a marketplace package)"),
                    f("ok", "Whether the plan has no blockers").kind(FieldKind::Bool),
                    list("root", "The module the plan is for"),
                    list("core_version", "The running core version"),
                    list("steps", "The modules to install or upgrade, dependencies first (a list)"),
                    list("blockers", "What stops it from being installed (a list)"),
                    list("warnings", "Notes that do not stop anything, such as a proprietary licence (a list)"),
                ],
                commands: vec![CommandDef::new(
                    "preview",
                    "Plan an install without doing it: the steps in order with versions and licences, blockers and warnings. Give id (marketplace) or manifest (obop.toml text)",
                    ADMIN_GROUP,
                )],
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
            // Written by obop-auth (D53), append-only: only get, list and search exist.
            ModelDef {
                name: "security_event",
                description: "A sign-in security event: two-factor enrolment, use of a recovery code, reset or lockout. Append-only",
                read_group: ADMIN_GROUP,
                write_group: ADMIN_GROUP,
                fields: vec![
                    f("user", "The account the event is about"),
                    f(
                        "kind",
                        "What happened: enrol_begin, enrol_confirm, disable, reset, recovery_regenerate, recovery_used, code_failed, lockout, limited_session",
                    ),
                    f(
                        "actor",
                        "Who did it: the user, an administrator or the operator",
                    ),
                    f("detail", "A short note without any code or secret"),
                    f("at", "When it happened, in UTC (RFC 3339)"),
                ],
                commands: vec![],
            },
        ],
        network_destinations: vec![],
        description: String::new(),
        translations: Default::default(),
        meta: ModuleMeta::first_party(
            "Customize the instance: theme, apps, modules and the marketplace.",
            &["platform"],
            &["modules", "marketplace", "settings"],
            "customize",
            &[],
        ),
    };
    m.translated(&[
        include_str!("../../../../packages/labels/de/platform.toml"),
        include_str!("../../../../packages/labels/fr/platform.toml"),
        include_str!("../../../../packages/labels/pl/platform.toml"),
        include_str!("../../../../packages/labels/fi/platform.toml"),
        include_str!("../../../../packages/labels/sv/platform.toml"),
        include_str!("../../../../packages/labels/es/platform.toml"),
    ])
}

/// Bind handlers for the admin commands. Called once at start-up.
pub fn register_admin(d: &Dispatcher, inst: &Arc<Installer>) {
    let m = admin_module();
    let i = inst.clone();
    d.register(&m, "module", "list", Box::new(move |_| Ok(i.module_rows())));
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
                .map(|p| i.module_row(p))
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
            refuse_skipped_dependencies(x)?;
            i.install_outcome(&text(x, "manifest")?, "upload", &caller(x))
                .map(|o| outcome_row(&o))
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
    d.register_read_only(&m, "security_event", &inst.store);
    let i = inst.clone();
    d.register(
        &m,
        "marketplace_listing",
        "list",
        Box::new(move |_| i.listing_rows()),
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
            refuse_skipped_dependencies(x)?;
            i.install_from_marketplace_outcome(&text(x, "id")?, &caller(x))
                .map(|o| outcome_row(&o))
        }),
    );
    // Read-only: what an install would do. Admin only; agents may read it freely.
    let i = inst.clone();
    d.register(
        &m,
        "module_plan",
        "preview",
        Box::new(move |input| {
            let Value::Map(x) = input else {
                return Err("expected an object".into());
            };
            match (x.get("manifest"), x.get("id")) {
                (Some(Value::Text(t)), _) => i.preview(Request::Manifest(t)),
                (_, Some(Value::Text(id))) => i.preview(Request::Market(id)),
                _ => Err(
                    "give either id (a marketplace package) or manifest (the text of an obop.toml)"
                        .into(),
                ),
            }
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

    /// D53: the security event log is a model like any other for reading, and read-only.
    #[test]
    fn the_security_event_log_is_readable_by_admins_and_never_writable() {
        let store = Store::new();
        let (d, _) = setup(&store);
        let mut f = Fields::new();
        f.insert("user".into(), J::from("ada"));
        f.insert("kind".into(), J::from("enrol_confirm"));
        store.backend().insert("security_event", "001", &f).unwrap();
        let rows =
            |p: &Principal| call(&d, p, "security_event.list", Value::Map(Default::default()));
        let Ok(r) = rows(&admin()) else { panic!() };
        let Value::List(l) = r.value else { panic!() };
        assert_eq!(l.len(), 1);
        assert!(matches!(
            rows(&Principal::user("ada", &["base.user"])),
            Err(CallError::Forbidden(_))
        ));
        assert!(
            call(&d, &admin(), "security_event.get", by_id("001")).is_ok(),
            "get works"
        );
        for c in ["create", "update", "delete"] {
            assert!(
                matches!(
                    call(&d, &admin(), &format!("security_event.{c}"), by_id("001")),
                    Err(CallError::UnknownCommand(_))
                ),
                "{c} must not exist"
            );
        }
        let agent = Principal::agent("ada/agent", &[ADMIN_GROUP], &[ADMIN_GROUP]);
        assert!(
            rows(&agent).is_ok(),
            "an agent scoped to the admin group may read it"
        );
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
                .contains("already installed")
        );
        let newer = fleet.replace("version = \"0.3.0\"", "version = \"0.4.0\"");
        assert_eq!(
            inst.install(&newer, "upload", "r").unwrap().version,
            "0.4.0"
        );
        assert!(
            inst.install(fleet, "upload", "r")
                .unwrap_err()
                .contains("would be a downgrade")
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
                .contains("needs ghost *")
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
            e.contains("refers to model party, which no dependency of notes provides"),
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
            description: String::new(),
            translations: Default::default(),
            meta: Default::default(),
        };
        d.register_crud(&party, &store);
        inst.install(pkg, "upload", "root").unwrap();
    }

    /// Fleet with a few German and Finnish labels (a partial translation on purpose).
    const FLEET_LABELS: &str = r#"
[labels.de.module]
name = "Fuhrpark"
description = "Fahrzeuge und Fahrer"

[labels.de.model.fleet_vehicle]
name = "Fahrzeug"
description = "Ein Fahrzeug des Unternehmens"

[labels.de.model.fleet_vehicle.field.plate]
label = "Kennzeichen"
description = "Amtliches Kennzeichen"

[labels.de.model.fleet_vehicle.field.state]
label = "Verfügbarkeit"

[labels.de.model.fleet_vehicle.field.state.options]
available = "Verfügbar"
in_service = "Im Einsatz"

[labels.de.model.fleet_vehicle.action.dispatch]
label = "In Betrieb nehmen"
description = "Ein verfügbares Fahrzeug in Betrieb nehmen"

[labels.fi.module]
name = "Autokanta"

[labels.fi.model.fleet_vehicle.action.dispatch]
label = "Ota käyttöön"
"#;

    fn fleet_with_labels(version: &str, labels: &str) -> String {
        Manifest::combine(
            &include_str!("../../../../packages/fleet.toml").replace("0.3.0", version),
            labels,
        )
    }

    fn fleet_meta(d: &Dispatcher) -> obop_commands::ModuleInfo {
        let manager = Principal::user("m", &["fleet.user", "fleet.manager"]);
        d.meta(Surface::Rest, &manager)
            .into_iter()
            .find(|m| m.id == "fleet")
            .unwrap()
    }

    #[test]
    fn meta_carries_labels_only_where_translated() {
        let (d, inst) = setup(&Store::new());
        inst.install(
            include_str!("../../../../packages/fleet.toml"),
            "upload",
            "root",
        )
        .unwrap();
        let plain = fleet_meta(&d);
        assert!(plain.labels.is_empty() && plain.descriptions.is_empty());
        assert!(plain.models.iter().all(|m| m.labels.is_empty()));
        assert!(
            plain
                .models
                .iter()
                .flat_map(|m| &m.fields)
                .all(|f| f.labels.is_empty() && f.option_labels.is_empty())
        );

        inst.install(&fleet_with_labels("0.4.0", FLEET_LABELS), "upload", "root")
            .unwrap();
        let m = fleet_meta(&d);
        assert_eq!(m.name, "Fleet", "the English name stays the source");
        assert_eq!(m.labels["de"], "Fuhrpark");
        assert_eq!(m.labels["fi"], "Autokanta");
        assert_eq!(m.descriptions["de"], "Fahrzeuge und Fahrer");
        assert!(m.description.starts_with("Vehicles, drivers"));
        let v = m.models.iter().find(|x| x.name == "fleet_vehicle").unwrap();
        assert_eq!(v.labels["de"], "Fahrzeug");
        let plate = v.fields.iter().find(|f| f.name == "plate").unwrap();
        assert_eq!(plate.labels["de"], "Kennzeichen");
        assert_eq!(plate.descriptions["de"], "Amtliches Kennzeichen");
        let state = v.fields.iter().find(|f| f.name == "state").unwrap();
        assert_eq!(state.option_labels["in_service"]["de"], "Im Einsatz");
        assert!(!state.option_labels.contains_key("maintenance"));
        let vin = v.fields.iter().find(|f| f.name == "vin").unwrap();
        assert!(vin.labels.is_empty() && vin.descriptions.is_empty());
        let dispatch = v
            .actions
            .iter()
            .find(|a| a.id == "fleet_vehicle.dispatch")
            .unwrap();
        assert_eq!(dispatch.labels["de"], "In Betrieb nehmen");
        assert_eq!(dispatch.labels["fi"], "Ota käyttöön");
        assert_eq!(
            dispatch.descriptions["de"],
            "Ein verfügbares Fahrzeug in Betrieb nehmen"
        );
        let back = v
            .actions
            .iter()
            .find(|a| a.id == "fleet_vehicle.return_to_pool")
            .unwrap();
        assert!(back.labels.is_empty());
        let driver = m
            .models
            .iter()
            .find(|x| x.name == "fleet_vehicle_driver")
            .unwrap();
        assert!(driver.labels.is_empty(), "untranslated models stay bare");
    }

    #[test]
    fn upgrades_replace_labels_and_a_restart_keeps_them() {
        let store = Store::new();
        let (d, inst) = setup(&store);
        inst.install(&fleet_with_labels("0.3.0", FLEET_LABELS), "upload", "root")
            .unwrap();
        let newer = FLEET_LABELS.replace("Fuhrpark", "Fahrzeugpark");
        inst.install(&fleet_with_labels("0.4.0", &newer), "upload", "root")
            .unwrap();
        assert_eq!(fleet_meta(&d).labels["de"], "Fahrzeugpark");
        // A new upload without labels is a package without translations.
        inst.install(&fleet_with_labels("0.5.0", ""), "upload", "root")
            .unwrap();
        assert!(fleet_meta(&d).labels.is_empty());
        inst.install(&fleet_with_labels("0.6.0", &newer), "upload", "root")
            .unwrap();

        // Restart: a new dispatcher over the same store loads the stored document.
        let (d2, inst2) = setup(&store);
        assert!(inst2.load_installed().is_empty());
        let m = fleet_meta(&d2);
        assert_eq!(m.labels["de"], "Fahrzeugpark");
        let plate = m.models[0]
            .fields
            .iter()
            .find(|f| f.name == "plate")
            .unwrap();
        assert_eq!(plate.labels["de"], "Kennzeichen");
    }

    #[test]
    fn uninstall_removes_the_labels_and_reinstall_restores_them() {
        let (d, inst) = setup(&Store::new());
        inst.install(&fleet_with_labels("0.3.0", FLEET_LABELS), "upload", "root")
            .unwrap();
        let manager = Principal::user("m", &["fleet.user", "fleet.manager"]);
        inst.uninstall("fleet").unwrap();
        assert!(
            d.meta(Surface::Rest, &manager)
                .iter()
                .all(|m| m.id != "fleet")
        );
        inst.install(&fleet_with_labels("0.3.0", FLEET_LABELS), "upload", "root")
            .unwrap();
        assert_eq!(fleet_meta(&d).labels["de"], "Fuhrpark");
    }

    #[test]
    fn an_old_marketplace_install_gains_the_bundled_labels_on_load() {
        let store = Store::new();
        let (_d, inst) = setup(&store);
        // As installed before packages carried translations: the English text, source marketplace.
        inst.install(
            include_str!("../../../../packages/fleet.toml"),
            "marketplace",
            "root",
        )
        .unwrap();
        let (d2, inst2) = setup(&store);
        assert!(inst2.load_installed().is_empty());
        let m = fleet_meta(&d2);
        for lang in ["de", "fr", "pl", "fi", "sv", "es"] {
            assert!(m.labels.contains_key(lang), "{lang}");
        }
        // An upload of the same text is the author's own and is left alone.
        let store = Store::new();
        let (_d, inst) = setup(&store);
        inst.install(
            include_str!("../../../../packages/fleet.toml"),
            "upload",
            "root",
        )
        .unwrap();
        let (d3, inst3) = setup(&store);
        assert!(inst3.load_installed().is_empty());
        assert!(fleet_meta(&d3).labels.is_empty());
    }

    #[test]
    fn bad_labels_in_an_upload_refuse_the_install() {
        let (d, inst) = setup(&Store::new());
        let bad = fleet_with_labels(
            "0.3.0",
            "[labels.de.model.fleet_vehicle.field.kennzeichen]\nlabel = \"x\"",
        );
        let e = inst.install(&bad, "upload", "root").unwrap_err();
        assert!(e.contains("no field \"kennzeichen\""), "{e}");
        assert!(!d.has_model("fleet_vehicle"), "nothing was installed");
    }
}
