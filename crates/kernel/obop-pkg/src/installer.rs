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
use crate::migrate;
use crate::resolve::{self, Plan, StepAction, World};
use crate::semver::Version;
use crate::views::{self, Ctx};
use obop_commands::{ADMIN_GROUP, ANY_GROUP, Dispatcher, Store, Value};
use obop_registry::{CommandDef, FieldDef, FieldKind, ModelDef, Module, ModuleMeta, intern};
use obop_storage::{ApplyError, Backend, Fields, Op};
use obop_types::DataClass;
use serde_json::Value as J;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, Weak};
use std::time::{SystemTime, UNIX_EPOCH};

const TABLE: &str = "obop_package";

/// A check that runs before an update or rollback that rewrites stored records (OBR-DAT-053,
/// D73): the binary plugs in the automatic verified backup, so the data steps of a module
/// upgrade never run without a way back. It runs with the module lock held and before any write;
/// an `Err` stops the operation with nothing changed.
/// Whether a model holds records, for deciding that a data step rewrites something: a count that
/// could not be read is taken to mean it does (the gate fails closed, never open).
/// `(module, from, to, steps)`: the data steps a plan runs for one package.
type DataSteps = (String, String, String, Vec<migrate::Step>);

pub(crate) fn holds_records(count: Result<usize, String>) -> bool {
    count.map_or(true, |n| n > 0)
}

pub trait MigrationGate: Send + Sync {
    fn before(&self, what: &GateRequest) -> Result<GateReceipt, String>;
}

/// What is about to run.
#[derive(Debug, Clone)]
pub struct GateRequest {
    /// `module.upgrade` or `module.rollback`.
    pub command: &'static str,
    /// The packages whose data steps run, with their versions: `crm 0.1.0->0.2.0`.
    pub modules: Vec<String>,
    /// The steps in words, as in the audit record.
    pub steps: Vec<String>,
    pub by: String,
}

/// What the gate did, to be written into the ledger rows of the operation.
#[derive(Debug, Clone, Default)]
pub struct GateReceipt {
    /// The verified backup taken first (its file name), if the gate took or found one.
    pub backup_id: Option<String>,
}

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
    /// Release notes of the installed version.
    pub changelog: String,
    /// The version an update replaced, kept so the update can be undone.
    pub rollback: Option<RollbackInfo>,
    /// Unix seconds of the last update, if there was one.
    pub updated_at: Option<u64>,
}

/// What a package remembers of the version before its last update (D63).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackInfo {
    pub version: String,
    /// Data steps of the update that cannot be undone (a dropped field). Not empty: the update
    /// can only be reverted from a backup.
    pub irreversible: Vec<String>,
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
    pub(crate) backend: Arc<dyn Backend>,
    store: Store,
    dispatcher: Weak<Dispatcher>,
    builtin: Vec<Module>,
    market: Arc<dyn Marketplace>,
    /// Plans are applied one at a time.
    pub(crate) apply: Mutex<()>,
    /// What this process has registered, by package id (a fingerprint of the stored record), so
    /// a change made through another process can be told from this process's own (sync.rs).
    pub(crate) registered: Mutex<std::collections::BTreeMap<String, String>>,
    pub(crate) seen_version: std::sync::atomic::AtomicU64,
    pub(crate) last_check_ms: std::sync::atomic::AtomicU64,
    /// The backup gate of updates that rewrite records (see [`MigrationGate`]).
    gate: Mutex<Option<Arc<dyn MigrationGate>>>,
}

/// What it takes to put a step back in this process's registry. Nothing is written to the
/// database before the whole plan commits, so there is nothing to undo there.
struct Undo {
    id: String,
    previous: Option<Fields>,
}

/// The locks held while the installed set changes.
struct Exclusive<'a> {
    _process: std::sync::MutexGuard<'a, ()>,
    _database: obop_storage::LockGuard,
}

/// Why a plan was not applied: `Retry` when a record changed under the batch (nothing was
/// written, the plan can be made again), `Refused` for anything final.
enum Failure {
    Retry(String),
    Refused(String),
}

/// Whether one installed package runs on a given core version.
#[derive(Debug, Clone)]
pub struct CoreCompat {
    pub id: String,
    pub name: String,
    pub version: String,
    pub requires_core: String,
    pub compatible: bool,
    pub reasons: Vec<String>,
}

/// An installed package with a newer version on offer.
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub id: String,
    pub installed: String,
    pub available: String,
    /// Whether the update can be applied here now.
    pub ok: bool,
    pub blockers: Vec<String>,
    pub changelog: String,
}

/// What `update_all` did.
#[derive(Debug, Clone)]
pub struct UpdateAll {
    pub applied: Vec<(PackageInfo, StepAction)>,
    /// Packages with an update that was not applied, with why.
    pub skipped: Vec<(String, String)>,
}

/// The cross-process lock that serialises changes to the installed set.
const LOCK: &str = "modules.apply";

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
            registered: Mutex::default(),
            seen_version: Default::default(),
            last_check_ms: Default::default(),
            gate: Mutex::new(None),
        })
    }

    /// Install the gate that runs before an update with data steps.
    pub fn set_gate(&self, gate: Arc<dyn MigrationGate>) {
        *self.gate.lock().unwrap_or_else(|e| e.into_inner()) = Some(gate);
    }

    /// The data steps a plan would run, per package: `(module, from, to, steps)`.
    ///
    /// A refused count is an error, not "no records": a step that rewrites stored records must
    /// not lose its backup because the store was busy when they were counted.
    fn data_steps_of(&self, plan: &Plan) -> Result<Vec<DataSteps>, String> {
        let mut out = Vec::new();
        let refused = std::cell::RefCell::new(None::<String>);
        for step in plan.work() {
            if step.action != StepAction::Upgrade {
                continue;
            }
            let Ok(Some(old)) = self.backend.get(TABLE, &step.id) else {
                continue;
            };
            let Some(om) = old
                .get("manifest")
                .and_then(J::as_str)
                .and_then(|t| Manifest::parse(t).ok())
            else {
                continue;
            };
            let count = |m: &str| match self.backend.count(m) {
                Ok(n) => n,
                Err(e) => {
                    refused.borrow_mut().get_or_insert(e);
                    0
                }
            };
            // A step on a model that holds no records rewrites nothing: it needs no backup.
            let steps: Vec<migrate::Step> = migrate::plan_data(&om, &step.manifest, &count)
                .steps
                .into_iter()
                .filter(|s| holds_records(self.backend.count(&s.model)))
                .collect();
            if !steps.is_empty() {
                out.push((
                    step.id.clone(),
                    om.module.version.clone(),
                    step.manifest.module.version.clone(),
                    steps,
                ));
            }
        }
        if let Some(e) = refused.into_inner() {
            return Err(format!("the records of a module could not be counted: {e}"));
        }
        Ok(out)
    }

    /// Run the gate for `command` over `steps` (the data steps about to run); `None` when there
    /// is nothing to guard or no gate is installed.
    fn run_gate(
        &self,
        command: &'static str,
        modules: Vec<String>,
        steps: Vec<String>,
        by: &str,
    ) -> Result<Option<GateReceipt>, String> {
        if steps.is_empty() {
            return Ok(None);
        }
        let gate = self.gate.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let Some(gate) = gate else {
            return Ok(None);
        };
        gate.before(&GateRequest {
            command,
            modules,
            steps,
            by: by.to_string(),
        })
        .map(Some)
        .map_err(|e| format!("not changed: {e}"))
    }

    pub(crate) fn d(&self) -> Result<Arc<Dispatcher>, String> {
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
            changelog: m.module.changelog.clone(),
            rollback: None,
            updated_at: None,
        }
    }

    /// As `info`, with what the stored record adds: the last update and the way back.
    fn info_stored(m: &Manifest, f: &Fields) -> PackageInfo {
        let mut p = Self::info(
            m,
            f.get("source").and_then(J::as_str).unwrap_or("upload"),
            f.get("installed_at").and_then(J::as_u64),
            f.get("installed_by").and_then(J::as_str).map(String::from),
        );
        p.updated_at = f.get("updated_at").and_then(J::as_u64);
        p.rollback = f
            .get("previous_version")
            .and_then(J::as_str)
            .filter(|_| f.get("previous_manifest").is_some())
            .map(|v| RollbackInfo {
                version: v.to_string(),
                irreversible: f
                    .get("irreversible")
                    .and_then(J::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default(),
            });
        p
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
            changelog: String::new(),
            rollback: None,
            updated_at: None,
        }
    }

    pub(crate) fn stored(&self) -> Vec<(String, Fields)> {
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
                v.push(Self::info_stored(&m, &f));
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
        // What an upgrade does to the records already stored (D63).
        let installed = self.installed_manifests();
        let refused = std::cell::RefCell::new(None::<String>);
        let count = |model: &str| match self.backend.count(model) {
            Ok(n) => n,
            Err(e) => {
                refused.borrow_mut().get_or_insert(e);
                0
            }
        };
        let mut data = Vec::new();
        for s in &plan.steps {
            if s.action != StepAction::Upgrade {
                continue;
            }
            if let Some(old) = installed.get(&s.id) {
                data.push((s.id.clone(), migrate::plan_data(old, &s.manifest, &count)));
            }
        }
        // A plan made with a count the store refused would miss the data that blocks it.
        if let Some(e) = refused.into_inner() {
            return Err(format!("the records of a module could not be counted: {e}"));
        }
        for (id, dp) in data {
            for b in &dp.blockers {
                plan.blockers.push(resolve::Blocker {
                    module: id.clone(),
                    kind: "data",
                    message: b.clone(),
                });
            }
            plan.warnings
                .extend(dp.warnings.iter().map(|w| format!("{id}: {w}")));
            plan.data.insert(id, dp);
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
        // Plan and apply under the lock: the plan is made against the installed set that the
        // apply then changes, with no other process in between.
        let _x = self.exclusive()?;
        let plan = self.plan(Request::Manifest(text))?.0;
        self.apply_plan_locked(&plan, &manifest.module.id, source, by)
    }

    pub fn install_from_marketplace(&self, id: &str, by: &str) -> Result<PackageInfo, String> {
        self.install_from_marketplace_outcome(id, by)
            .map(|o| o.root)
    }

    pub fn install_from_marketplace_outcome(&self, id: &str, by: &str) -> Result<Outcome, String> {
        let _x = self.exclusive()?;
        let (plan, root) = self.plan(Request::Market(id))?;
        self.apply_plan_locked(&plan, &root, "marketplace", by)
    }

    /// Update an installed package to the newer version the marketplace (the bundled catalogue
    /// or the configured one) offers, with whatever dependencies the plan needs (D63).
    pub fn update_outcome(&self, id: &str, by: &str) -> Result<Outcome, String> {
        let _x = self.exclusive()?;
        let have = self
            .installed_manifests()
            .remove(id)
            .ok_or_else(|| self.not_installed(id))?;
        let (plan, root) = self.plan(Request::Market(id))?;
        let offered = plan
            .steps
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.manifest.semver());
        if offered.as_ref().is_none_or(|o| *o <= have.semver()) {
            return Err(format!(
                "{id} {} is up to date: the marketplace offers nothing newer",
                have.module.version
            ));
        }
        self.apply_plan_locked(&plan, &root, "marketplace", by)
    }

    fn not_installed(&self, id: &str) -> String {
        if self.builtin.iter().any(|b| b.id == id) {
            format!("{id} is a built-in module; it is updated with the core (Settings, Updates)")
        } else {
            format!("{id} is not installed")
        }
    }

    /// Update every package that has a newer version the instance can take, one plan after the
    /// other (a dependency first when a dependent needs it). A package whose update is blocked
    /// stays where it is and is reported with the reason; the others still update.
    pub fn update_all(&self, by: &str) -> Result<UpdateAll, String> {
        let _x = self.exclusive()?;
        let mut applied: Vec<(PackageInfo, StepAction)> = Vec::new();
        let mut skipped: BTreeMap<String, String> = BTreeMap::new();
        for _round in 0..=self.installed_manifests().len() {
            let offered = self.offered();
            let candidates: Vec<String> = self
                .installed_manifests()
                .iter()
                .filter(|(id, m)| {
                    offered
                        .get(*id)
                        .and_then(|l| Version::parse(&l.version).ok())
                        .is_some_and(|v| v > m.semver())
                })
                .map(|(id, _)| id.clone())
                .collect();
            let mut progress = false;
            for id in candidates {
                let plan = match self.plan(Request::Market(&id)) {
                    Ok((p, _)) => p,
                    Err(e) => {
                        skipped.insert(id, e);
                        continue;
                    }
                };
                if !plan.ok() {
                    skipped.insert(
                        id,
                        plan.blockers
                            .iter()
                            .map(|b| b.message.clone())
                            .collect::<Vec<_>>()
                            .join("; "),
                    );
                    continue;
                }
                match self.apply_plan_locked(&plan, &id, "marketplace", by) {
                    Ok(o) => {
                        skipped.remove(&id);
                        applied.extend(o.applied);
                        progress = true;
                    }
                    Err(e) => {
                        skipped.insert(id, e);
                    }
                }
            }
            if !progress {
                break;
            }
        }
        Ok(UpdateAll {
            applied,
            skipped: skipped.into_iter().collect(),
        })
    }

    /// Apply a plan that has no blockers. The root of an upload keeps `source` `upload`; every
    /// other step comes from the marketplace. The whole plan is one operation: the packages'
    /// records and every data migration they declare are written in one atomic batch, so a
    /// failure at any step, in this process or at the database, leaves the instance exactly as
    /// it was (D63).
    pub fn apply_plan(
        &self,
        plan: &Plan,
        root: &str,
        root_source: &str,
        by: &str,
    ) -> Result<Outcome, String> {
        let _x = self.exclusive()?;
        self.apply_plan_locked(plan, root, root_source, by)
    }

    /// The locks every change to the installed set holds: this process's, then the database's,
    /// so two server processes never change the installed set at once (D59).
    fn exclusive(&self) -> Result<Exclusive<'_>, String> {
        let process = self.apply.lock().unwrap_or_else(|e| e.into_inner());
        let database = self.backend.lock(LOCK).map_err(|e| {
            format!("another change to the modules is still running ({e}); try again in a moment")
        })?;
        Ok(Exclusive {
            _process: process,
            _database: database,
        })
    }

    fn apply_plan_locked(
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
        let guarded = self.data_steps_of(plan)?;
        let receipt = self.run_gate(
            "module.upgrade",
            guarded
                .iter()
                .map(|(m, f, t, _)| format!("{m} {f}->{t}"))
                .collect(),
            guarded
                .iter()
                .flat_map(|(m, _, _, s)| s.iter().map(move |s| format!("{m}: {}", s.summary())))
                .collect(),
            by,
        )?;
        let backup = receipt.and_then(|r| r.backup_id);
        let mut last = String::new();
        for _ in 0..4 {
            match self.try_apply(plan, root, root_source, by, backup.as_deref()) {
                Ok(o) => return Ok(o),
                Err(Failure::Retry(why)) => last = why,
                Err(Failure::Refused(e)) => return Err(e),
            }
        }
        Err(format!(
            "records kept changing while the update ran ({last}); nothing was changed, try again"
        ))
    }

    fn try_apply(
        &self,
        plan: &Plan,
        root: &str,
        root_source: &str,
        by: &str,
        backup: Option<&str>,
    ) -> Result<Outcome, Failure> {
        let mut done: Vec<Undo> = Vec::new();
        let mut applied = Vec::new();
        let mut ops: Vec<Op> = Vec::new();
        let mut notes: Vec<(String, &'static str, String)> = Vec::new();
        for step in plan.work() {
            let source = if step.id == root {
                root_source
            } else {
                "marketplace"
            };
            match self.prepare_step(step, source, by, backup, &mut ops) {
                Ok((undo, info, note)) => {
                    done.push(undo);
                    notes.push((
                        step.id.clone(),
                        if step.action == StepAction::Upgrade {
                            "module_update"
                        } else {
                            "module_install"
                        },
                        note,
                    ));
                    applied.push((info, step.action));
                }
                Err(e) => {
                    let n = done.len();
                    self.unwind(done);
                    return Err(Failure::Refused(if n == 0 {
                        format!("{}: {e}", step.id)
                    } else {
                        format!(
                            "{}: {e}; the {n} step(s) before it were rolled back and nothing was installed",
                            step.id
                        )
                    }));
                }
            }
        }
        if let Err(e) = self.backend.apply_checked(&ops) {
            self.unwind(done);
            return Err(match e {
                ApplyError::Conflict { reason, .. } => Failure::Retry(reason),
                ApplyError::Storage(e) => {
                    Failure::Refused(format!("nothing was changed, the database refused it: {e}"))
                }
            });
        }
        // Tell the other server processes on this database that the installed set changed.
        self.note_local_change(
            &applied
                .iter()
                .map(|(i, _)| i.id.clone())
                .collect::<Vec<_>>(),
        );
        for (id, kind, note) in &notes {
            self.audit(by, kind, id, note);
        }
        let root_info = applied
            .iter()
            .find(|(i, _)| i.id == root)
            .map(|(i, _)| i.clone())
            .or_else(|| self.list().into_iter().find(|p| p.id == root))
            .ok_or_else(|| Failure::Refused(format!("{root} was not installed")))?;
        Ok(Outcome {
            root: root_info,
            applied,
            warnings: plan.warnings.clone(),
        })
    }

    /// One line in the audit log (`security_event`) about a change to the modules or the core.
    pub fn audit(&self, by: &str, kind: &str, subject: &str, detail: &str) {
        obop_commands::audit::record_event(&*self.backend, now(), subject, kind, by, detail);
    }

    /// Check one step of a plan, register the package in this process and add what the batch
    /// must write to `ops`: the package record and, for an upgrade, the data steps.
    fn prepare_step(
        &self,
        step: &resolve::Step,
        source: &str,
        by: &str,
        backup: Option<&str>,
        ops: &mut Vec<Op>,
    ) -> Result<(Undo, PackageInfo, String), String> {
        let manifest = &step.manifest;
        let module = manifest.to_module()?;
        let id = manifest.module.id.clone();
        if self.builtin.iter().any(|b| b.id == id) {
            return Err(format!("{id} is a built-in module"));
        }
        let d = self.d()?;
        let existing = self.backend.get(TABLE, &id).map_err(|e| e.to_string())?;
        let mut data_steps: Vec<migrate::Step> = Vec::new();
        let mut note = format!("{} {}", id, manifest.module.version);
        let mut old_manifest: Option<Manifest> = None;
        if let Some(old) = &existing {
            let old_text = old.get("manifest").and_then(J::as_str).unwrap_or("");
            let old_v = old.get("version").and_then(J::as_str).unwrap_or("0.0.0");
            if step.action == StepAction::Upgrade
                && step.from_version.as_deref().is_some_and(|f| f != old_v)
            {
                return Err(format!(
                    "{id} is now {old_v}, but this plan was made for {}; another change got in first, plan again",
                    step.from_version.as_deref().unwrap_or("?")
                ));
            }
            let old_v = Version::parse(old_v).unwrap_or_else(|_| Version::new(0, 0, 0));
            if manifest.semver() <= old_v {
                return Err(format!(
                    "{id} {old_v} is installed; installing {} is not an upgrade",
                    manifest.module.version
                ));
            }
            let om = Manifest::parse(old_text).map_err(|e| format!("the installed copy: {e}"))?;
            let refused = std::cell::RefCell::new(None::<String>);
            let count = |m: &str| match self.backend.count(m) {
                Ok(n) => n,
                Err(e) => {
                    refused.borrow_mut().get_or_insert(e);
                    0
                }
            };
            let dp = migrate::plan_data(&om, manifest, &count);
            if let Some(e) = refused.into_inner() {
                return Err(format!("the records of a module could not be counted: {e}"));
            }
            if !dp.blockers.is_empty() {
                return Err(dp.blockers.join("; "));
            }
            data_steps = dp.steps;
            note = format!("{id} {old_v} -> {}", manifest.module.version);
            if !data_steps.is_empty() {
                note.push_str(&format!(
                    "; data: {}",
                    data_steps
                        .iter()
                        .map(migrate::Step::summary)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            old_manifest = Some(om);
        } else if step.action == StepAction::Upgrade {
            return Err(format!(
                "{id} is no longer installed; this plan was made for an upgrade, plan again"
            ));
        }
        for m in &module.models {
            if d.has_model(m.name) && existing.is_none() {
                return Err(format!("model {} already exists", m.name));
            }
        }
        // An uninstall keeps the data: a reinstall must find it in the shape of this version.
        if existing.is_none() {
            migrate::check_stored_shapes(&*self.backend, manifest)?;
        }
        for (at, to) in manifest.external_refs() {
            if !d.has_model(&to) {
                return Err(format!("{at} refers to model {to}, which is not installed"));
            }
        }
        // The rules of the models (effects, computed fields, access) are checked against the
        // models they use, now that the dependencies are registered.
        let look = |n: &str| d.model_def(n).map(Arc::new);
        let problems = obop_registry::check_module_in(&module, &look);
        if !problems.is_empty() {
            return Err(problems
                .iter()
                .map(|v| format!("{}: {}", v.path, v.problem))
                .collect::<Vec<_>>()
                .join("; "));
        }
        let at = now();
        let mut f = Fields::new();
        f.insert("manifest".into(), J::from(manifest.text.as_str()));
        f.insert("version".into(), J::from(manifest.module.version.clone()));
        f.insert("source".into(), J::from(source));
        f.insert("installed_by".into(), J::from(by));
        f.insert("installed_at".into(), J::from(at));
        if let (Some(old), Some(om)) = (&existing, &old_manifest) {
            // Keep the way back: the version this one replaces, and what undoes the data steps.
            let (inverse, irreversible) = migrate::inverse(&data_steps);
            f.insert("updated_at".into(), J::from(at));
            f.insert(
                "previous_manifest".into(),
                old.get("manifest").cloned().unwrap_or(J::Null),
            );
            f.insert(
                "previous_version".into(),
                J::from(om.module.version.clone()),
            );
            f.insert(
                "previous_source".into(),
                old.get("source").cloned().unwrap_or(J::from("upload")),
            );
            f.insert(
                "rollback_steps".into(),
                J::Array(inverse.iter().map(migrate::Step::to_json).collect()),
            );
            f.insert(
                "irreversible".into(),
                J::Array(irreversible.into_iter().map(J::from).collect()),
            );
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
        let undo = Undo {
            id: id.clone(),
            previous: existing.clone(),
        };
        // The data steps read the records now; the batch checks that none changed before it
        // commits.
        match migrate::build_ops(&*self.backend, &data_steps) {
            Ok(mut data_ops) => ops.append(&mut data_ops),
            Err(e) => {
                self.unwind(vec![undo]);
                return Err(e);
            }
        }
        // One ledger row per data step, in the same batch (OBR-DAT-051).
        if let Some(om) = &old_manifest {
            ops.extend(Self::ledger_ops(
                &id,
                &om.module.version,
                &manifest.module.version,
                false,
                &data_steps,
                by,
                backup,
            ));
            if let (Some(b), false) = (backup, data_steps.is_empty()) {
                note.push_str(&format!("; backup {b}"));
            }
        }
        ops.push(if existing.is_some() {
            Op::Put {
                model: TABLE.into(),
                id: id.clone(),
                fields: f,
            }
        } else {
            Op::Insert {
                model: TABLE.into(),
                id: id.clone(),
                fields: f,
            }
        });
        Ok((
            undo,
            Self::info(manifest, source, Some(at), Some(by.to_string())),
            note,
        ))
    }

    /// The ledger rows for the data steps of one package upgrade or rollback.
    fn ledger_ops(
        module: &str,
        from: &str,
        to: &str,
        rollback: bool,
        steps: &[migrate::Step],
        by: &str,
        backup: Option<&str>,
    ) -> Vec<Op> {
        let summaries: Vec<String> = steps.iter().map(migrate::Step::summary).collect();
        let view: Vec<obop_migrate::ledger::PackageStep> = steps
            .iter()
            .zip(&summaries)
            .map(|(s, summary)| obop_migrate::ledger::PackageStep {
                op: &s.op,
                model: &s.model,
                field: &s.field,
                to: s.to.as_deref(),
                value: s.value.as_ref(),
                summary,
            })
            .collect();
        obop_migrate::ledger::package_entries(module, from, to, rollback, &view, by, backup)
            .iter()
            .map(obop_migrate::Entry::insert)
            .collect()
    }

    /// Put the registration of a stored package back (after a failed upgrade or a rollback).
    pub(crate) fn reregister(&self, id: &str, stored: &Fields) {
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

    /// Take back what this process registered for steps that were never committed.
    fn unwind(&self, done: Vec<Undo>) {
        for u in done.into_iter().rev() {
            if let Ok(d) = self.d() {
                d.unregister_module(&u.id);
            }
            if let Some(f) = u.previous {
                self.reregister(&u.id, &f);
            }
        }
    }

    /// Go back to the version before the last update (D63). Possible while the update left the
    /// data in a shape the older version reads: renames are reversed, fields the update added are
    /// kept (the older version ignores them), and an update that deleted a field's values is
    /// refused (restore the backup instead). The older version must still fit the core, its own
    /// dependencies, and every installed package that depends on this one.
    pub fn rollback(&self, id: &str, by: &str) -> Result<PackageInfo, String> {
        let _x = self.exclusive()?;
        let mut last = String::new();
        for _ in 0..4 {
            match self.try_rollback(id, by) {
                Ok(p) => return Ok(p),
                Err(Failure::Retry(why)) => last = why,
                Err(Failure::Refused(e)) => return Err(e),
            }
        }
        Err(format!(
            "records kept changing while the rollback ran ({last}); nothing was changed, try again"
        ))
    }

    /// Why `id` cannot go back. `None` when it can.
    pub fn rollback_blocker(&self, id: &str) -> Option<String> {
        let stored = self.backend.get(TABLE, id).ok().flatten()?;
        self.rollback_check(id, &stored).err()
    }

    fn rollback_check(&self, id: &str, stored: &Fields) -> Result<Manifest, String> {
        let Some(prev_text) = stored.get("previous_manifest").and_then(J::as_str) else {
            return Err(format!(
                "{id} has no earlier version to go back to: it was installed at this version, or it was already rolled back"
            ));
        };
        let irreversible: Vec<&str> = stored
            .get("irreversible")
            .and_then(J::as_array)
            .map(|a| a.iter().filter_map(J::as_str).collect())
            .unwrap_or_default();
        if !irreversible.is_empty() {
            return Err(format!(
                "the update of {id} deleted data ({}), so the older version cannot be restored by rolling back; restore the backup taken before the update",
                irreversible.join("; ")
            ));
        }
        let prev = Manifest::parse(prev_text).map_err(|e| format!("the earlier copy: {e}"))?;
        let core = self.core_version();
        let pv = prev.semver();
        if !prev.core_req().matches(&core) {
            return Err(format!(
                "{id} {pv} needs core {}, but this instance runs {core}",
                prev.core_req()
            ));
        }
        let installed = self.installed_manifests();
        for r in prev.requirements() {
            let have = if self.builtin.iter().any(|b| b.id == r.id) {
                Some(core.clone())
            } else {
                installed.get(&r.id).map(Manifest::semver)
            };
            match have {
                None => {
                    return Err(format!(
                        "{id} {pv} needs {} {}, which is not installed",
                        r.id, r.req
                    ));
                }
                Some(v) if !r.req.matches(&v) => {
                    return Err(format!(
                        "{id} {pv} needs {} {}, but {v} is installed",
                        r.id, r.req
                    ));
                }
                _ => {}
            }
        }
        for (other, m) in &installed {
            for r in m.requirements() {
                if r.id == id && !r.req.matches(&pv) {
                    return Err(format!(
                        "rolling {id} back to {pv} would break {other}, which needs {id} {}",
                        r.req
                    ));
                }
            }
        }
        Ok(prev)
    }

    fn try_rollback(&self, id: &str, by: &str) -> Result<PackageInfo, Failure> {
        let stored = self
            .backend
            .get(TABLE, id)
            .map_err(|e| Failure::Refused(e.to_string()))?
            .ok_or_else(|| Failure::Refused(self.not_installed(id)))?;
        self.rollback_check(id, &stored).map_err(Failure::Refused)?;
        let prev_text = stored
            .get("previous_manifest")
            .and_then(J::as_str)
            .unwrap_or("");
        let source = stored
            .get("previous_source")
            .and_then(J::as_str)
            .unwrap_or("upload")
            .to_string();
        let current_v = stored.get("version").and_then(J::as_str).unwrap_or("?");
        let inverse: Vec<migrate::Step> = stored
            .get("rollback_steps")
            .and_then(J::as_array)
            .map(|a| a.iter().filter_map(migrate::Step::from_json).collect())
            .unwrap_or_default();
        let prev = Self::with_bundled_labels(prev_text, &source).map_err(Failure::Refused)?;
        let prev_module = prev.to_module().map_err(Failure::Refused)?;
        let d = self.d().map_err(Failure::Refused)?;
        let receipt = self
            .run_gate(
                "module.rollback",
                if inverse
                    .iter()
                    .any(|s| holds_records(self.backend.count(&s.model)))
                {
                    vec![format!("{id} {current_v}->{}", prev.module.version)]
                } else {
                    Vec::new()
                },
                inverse
                    .iter()
                    .filter(|s| holds_records(self.backend.count(&s.model)))
                    .map(|s| format!("{id}: {}", s.summary()))
                    .collect(),
                by,
            )
            .map_err(Failure::Refused)?;
        let backup = receipt.and_then(|r| r.backup_id);
        let at = now();
        let mut f = Fields::new();
        f.insert("manifest".into(), J::from(prev_text));
        f.insert("version".into(), J::from(prev.module.version.clone()));
        f.insert("source".into(), J::from(source.as_str()));
        f.insert("installed_by".into(), J::from(by));
        f.insert("installed_at".into(), J::from(at));
        f.insert("updated_at".into(), J::from(at));
        f.insert("rolled_back_from".into(), J::from(current_v));
        d.unregister_module(id);
        if let Err(e) = self.register(&prev, &prev_module) {
            self.reregister(id, &stored);
            return Err(Failure::Refused(e));
        }
        let undo = Undo {
            id: id.to_string(),
            previous: Some(stored.clone()),
        };
        let mut ops = match migrate::build_ops(&*self.backend, &inverse) {
            Ok(o) => o,
            Err(e) => {
                self.unwind(vec![undo]);
                return Err(Failure::Refused(e));
            }
        };
        ops.extend(Self::ledger_ops(
            id,
            current_v,
            &prev.module.version,
            true,
            &inverse,
            by,
            backup.as_deref(),
        ));
        ops.push(Op::Put {
            model: TABLE.into(),
            id: id.to_string(),
            fields: f,
        });
        if let Err(e) = self.backend.apply_checked(&ops) {
            self.unwind(vec![undo]);
            return Err(match e {
                ApplyError::Conflict { reason, .. } => Failure::Retry(reason),
                ApplyError::Storage(e) => {
                    Failure::Refused(format!("nothing was changed, the database refused it: {e}"))
                }
            });
        }
        self.note_local_change(&[id.to_string()]);
        self.audit(
            by,
            "module_rollback",
            id,
            &format!("{id} {current_v} -> {}", prev.module.version),
        );
        Ok(Self::info(&prev, &source, Some(at), Some(by.to_string())))
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
        self.uninstall_as(id, "operator")
    }

    pub fn uninstall_as(&self, id: &str, by: &str) -> Result<(), String> {
        let _x = self.exclusive()?;
        if let Some(b) = self.uninstall_blocker(id) {
            return Err(b.message);
        }
        if !self.backend.delete(TABLE, id)? {
            return Err(format!("{id} is not installed"));
        }
        self.d()?.unregister_module(id);
        self.note_local_change(&[id.to_string()]);
        self.audit(
            by,
            "module_uninstall",
            id,
            &format!("{id} removed; its data stays"),
        );
        Ok(())
    }

    /// The instance-wide UI settings (theme, app order): one JSON object kept as text. For a
    /// display: settings the store would not give show as none.
    pub fn settings(&self) -> String {
        self.try_settings().unwrap_or_else(|_| "{}".into())
    }

    /// As [`Installer::settings`], and a refused read is an error: what a change is compared with
    /// (the audit line of a security setting says old and new) must not be "nothing".
    pub fn try_settings(&self) -> Result<String, String> {
        Ok(self
            .backend
            .get("obop_setting", "instance")?
            .and_then(|f| f.get("value").and_then(J::as_str).map(String::from))
            .unwrap_or_else(|| "{}".into()))
    }

    /// As `set_settings_as`, for the operator at the command line.
    pub fn set_settings(&self, text: &str) -> Result<(), String> {
        self.set_settings_as(text, "operator")
    }

    /// Store the settings. A change to anything under `security` (the two-factor policy and the
    /// like) is written to the security event log with who made it and the old and new value of
    /// each key that changed. Values that look like secrets are not written.
    pub fn set_settings_as(&self, text: &str, actor: &str) -> Result<(), String> {
        if text.len() > 64 * 1024 {
            return Err("settings are larger than 64 KiB".into());
        }
        let new: J = match serde_json::from_str::<J>(text) {
            Ok(v @ J::Object(_)) => v,
            _ => return Err("settings must be a JSON object".into()),
        };
        let old: J = serde_json::from_str(&self.try_settings()?).unwrap_or(J::Null);
        let mut f = Fields::new();
        f.insert("value".into(), J::from(text));
        if !self.backend.update("obop_setting", "instance", &f)? {
            self.backend.insert("obop_setting", "instance", &f)?;
        }
        for line in security_changes(&old["security"], &new["security"]) {
            obop_commands::audit::record_event(
                &*self.backend,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs()),
                "(instance)",
                "settings_change",
                actor,
                &line,
            );
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
        self.note_loaded();
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

    /// Whether each installed package would still run on core `core`: its own core range, and
    /// the ranges it declares on built-in modules (which version with the core). A core update
    /// uses this to refuse, or warn about, packages that were not written for the new core
    /// (D63).
    pub fn core_compat(&self, core: &Version) -> Vec<CoreCompat> {
        self.installed_manifests()
            .into_iter()
            .map(|(id, m)| {
                let mut reasons = Vec::new();
                if !m.core_req().matches(core) {
                    reasons.push(format!(
                        "it supports core {}, not {core}",
                        m.module.requires_core
                    ));
                }
                for r in m.requirements() {
                    if self.builtin.iter().any(|b| b.id == r.id) && !r.req.matches(core) {
                        reasons.push(format!(
                            "it needs {} {}, which versions with the core ({core})",
                            r.id, r.req
                        ));
                    }
                }
                CoreCompat {
                    id,
                    name: m.module.name.clone(),
                    version: m.module.version.clone(),
                    requires_core: m.module.requires_core.clone(),
                    compatible: reasons.is_empty(),
                    reasons,
                }
            })
            .collect()
    }

    /// The installed packages for which the marketplace offers a newer version, with whether the
    /// update can be applied here now and why not.
    pub fn updates(&self) -> Vec<UpdateInfo> {
        let offered = self.offered();
        let mut out = Vec::new();
        for (id, m) in self.installed_manifests() {
            let Some(l) = offered.get(&id) else { continue };
            let Ok(v) = Version::parse(&l.version) else {
                continue;
            };
            if v <= m.semver() {
                continue;
            }
            let (ok, blockers, changelog) = match self.plan(Request::Market(&id)) {
                Ok((plan, _)) => (
                    plan.ok(),
                    plan.blockers.iter().map(|b| b.message.clone()).collect(),
                    plan.steps
                        .iter()
                        .find(|s| s.id == id)
                        .map(|s| s.manifest.module.changelog.clone())
                        .unwrap_or_default(),
                ),
                Err(e) => (false, vec![e], String::new()),
            };
            out.push(UpdateInfo {
                id,
                installed: m.module.version.clone(),
                available: l.version.clone(),
                ok,
                blockers,
                changelog,
            });
        }
        out
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
        for st in plan.work().filter(|s| s.action == StepAction::Upgrade) {
            if !st.manifest.module.changelog.trim().is_empty() {
                out.push_str(&format!("  notes for {} {}:\n", st.id, st.version));
                for line in st.manifest.module.changelog.trim().lines() {
                    out.push_str(&format!("    {line}\n"));
                }
            }
            if let Some(d) = plan.data.get(&st.id) {
                for c in &d.changes {
                    out.push_str(&format!(
                        "  change {}: {}{}{}\n",
                        st.id,
                        c.detail,
                        if c.breaking { " [breaking]" } else { "" },
                        c.resolved_by
                            .as_ref()
                            .map_or(String::new(), |r| format!(" -> {r}")),
                    ));
                }
            }
            let installed = self.installed_manifests();
            for (pid, m) in &installed {
                if let Some(r) = m.requirements().into_iter().find(|r| r.id == st.id) {
                    out.push_str(&format!(
                        "  dependent {pid} {} supports {} {}: {}\n",
                        m.module.version,
                        st.id,
                        r.req,
                        if r.req.matches(&st.manifest.semver()) {
                            "ok"
                        } else {
                            "NOT SATISFIED"
                        }
                    ));
                }
            }
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
        m.insert("changelog".into(), views::text(&p.changelog));
        m.insert(
            "updated_at".into(),
            p.updated_at.map_or(Value::Null, |t| Value::Int(t as i64)),
        );
        let rb = p.rollback.as_ref();
        let rb_blocker = rb.and_then(|_| self.rollback_blocker(&p.id));
        m.insert(
            "rollback_version".into(),
            rb.map_or(Value::Null, |r| views::text(&r.version)),
        );
        m.insert(
            "rollback_available".into(),
            Value::Bool(rb.is_some() && rb_blocker.is_none()),
        );
        m.insert(
            "rollback_blocker".into(),
            rb_blocker.as_deref().map_or(Value::Null, views::text),
        );
        // What the update to the version on offer would do here (D63).
        let newer = matches!(m.get("update_available"), Some(Value::Text(_)));
        let (mut upd_notes, mut upd_ok, mut upd_blockers, mut upd_changes, mut upd_steps) = (
            String::new(),
            false,
            Value::List(vec![]),
            Value::List(vec![]),
            Value::List(vec![]),
        );
        if newer && p.source != "builtin" {
            match self.plan(Request::Market(&p.id)) {
                Ok((plan, _)) => {
                    upd_ok = plan.ok();
                    upd_blockers = views::blockers_value(&plan.blockers);
                    if let Some(step) = plan.steps.iter().find(|s| s.id == p.id) {
                        upd_notes = step.manifest.module.changelog.clone();
                    }
                    if let Some(d) = plan.data.get(&p.id) {
                        upd_changes =
                            Value::List(d.changes.iter().map(views::change_value).collect());
                        upd_steps = Value::List(
                            d.steps
                                .iter()
                                .map(|st| views::text(&st.summary()))
                                .collect(),
                        );
                    }
                }
                Err(e) => {
                    upd_blockers = Value::List(vec![Value::map([
                        ("module", views::text(&p.id)),
                        ("kind", views::text("unavailable")),
                        ("message", views::text(&e)),
                    ])]);
                }
            }
        }
        m.insert("update_changelog".into(), views::text(&upd_notes));
        m.insert("update_compatible".into(), Value::Bool(upd_ok));
        m.insert("update_blockers".into(), upd_blockers);
        m.insert("update_changes".into(), upd_changes);
        m.insert("update_migrations".into(), upd_steps);
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
                m.insert("changelog".into(), views::text(&l.changelog));
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

/// What changed between two `security` objects, one line per key: `security.key: old -> new`.
/// Booleans, numbers and short plain strings are shown; anything under a key that looks like a
/// secret, and anything longer or structured, is only said to have changed.
pub(crate) fn security_changes(old: &J, new: &J) -> Vec<String> {
    let empty = serde_json::Map::new();
    let (o, n) = (
        old.as_object().unwrap_or(&empty),
        new.as_object().unwrap_or(&empty),
    );
    let mut keys: Vec<&String> = o.keys().chain(n.keys()).collect();
    keys.sort();
    keys.dedup();
    let show = |key: &str, v: Option<&J>| -> String {
        let lower = key.to_ascii_lowercase();
        let secretish = ["secret", "token", "password", "key", "credential"]
            .iter()
            .any(|w| lower.contains(w));
        match v {
            None | Some(J::Null) => "(unset)".into(),
            _ if secretish => "(hidden)".into(),
            Some(J::Bool(b)) => b.to_string(),
            Some(J::Number(x)) => x.to_string(),
            Some(J::String(s)) if s.chars().count() <= 60 && !s.chars().any(char::is_control) => {
                format!("{s:?}")
            }
            Some(_) => "(changed)".into(),
        }
    };
    keys.into_iter()
        .filter(|k| o.get(*k) != n.get(*k))
        .map(|k| {
            format!(
                "security.{k}: {} -> {}",
                show(k, o.get(k)),
                show(k, n.get(k))
            )
        })
        .collect()
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
            list("changelog", "Release notes of this version"),
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
            "updated_at",
            "When it was last updated (Unix seconds); empty if never",
        ),
        list(
            "rollback_version",
            "The version before the last update, if it is kept",
        ),
        f(
            "rollback_available",
            "Whether the last update can be undone now",
        )
        .kind(FieldKind::Bool),
        list(
            "rollback_blocker",
            "Why the last update cannot be undone, if it cannot",
        ),
        list(
            "update_changelog",
            "Release notes of the version on offer",
        ),
        f(
            "update_compatible",
            "Whether the version on offer can be installed here now",
        )
        .kind(FieldKind::Bool),
        list(
            "update_blockers",
            "What stops the update to the version on offer (a list)",
        ),
        list(
            "update_changes",
            "What the version on offer changes in the models: added, removed, retyped fields (a list)",
        ),
        list(
            "update_migrations",
            "The data steps the update would run, in words (a list)",
        ),
        list(
            "updated",
            "Result of upgrade_all: the modules that were updated (a list)",
        ),
        list(
            "skipped",
            "Result of upgrade_all: the modules whose update was blocked, with why (a list)",
        ),
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
                access: None,
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
                    CommandDef::new(
                        "upgrade",
                        "Update an installed module to the newer version the marketplace offers, with the dependencies the plan needs, in one atomic operation. Give id",
                        ADMIN_GROUP,
                    )
                    .approval(),
                    CommandDef::new(
                        "upgrade_all",
                        "Update every module that has a newer version this instance can take; a module whose update is blocked stays and is reported",
                        ADMIN_GROUP,
                    )
                    .approval(),
                    CommandDef::new(
                        "rollback",
                        "Put a module back to the version before its last update, when the data allows it. Give id",
                        ADMIN_GROUP,
                    )
                    .approval(),
                ],
                ext: Default::default(),
            },
            crate::accounts::user_account_model(),
            ModelDef {
                name: "marketplace_listing",
                description: "A module offered by the marketplace",
                read_group: ADMIN_GROUP,
                write_group: ADMIN_GROUP,
                access: None,
                fields: listing_fields,
                commands: vec![
                    CommandDef::new(
                        "install",
                        "Install a module from the marketplace by id, with the dependencies it needs, in one operation",
                        ADMIN_GROUP,
                    )
                    .approval(),
                ],
                ext: Default::default(),
            },
            ModelDef {
                name: "module_plan",
                description: "What installing a module would do here: the steps in order, blockers and warnings",
                read_group: ADMIN_GROUP,
                write_group: ADMIN_GROUP,
                access: None,
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
                ext: Default::default(),
            },
            ModelDef {
                name: "instance_update",
                description: "Updating the instance itself: the running and the newest core version, whether the installed modules fit it, and the request to the control plane that runs the upgrade",
                read_group: ADMIN_GROUP,
                write_group: ADMIN_GROUP,
                access: None,
                fields: vec![
                    list("version", "Input for start: the published version to update to"),
                    f(
                        "skip_backup",
                        "Input for start: set when you have your own backup and none can be taken here",
                    )
                    .kind(FieldKind::Bool),
                    list("mode", "managed (a control plane runs the upgrade) or self_hosted (commands to run yourself)"),
                    list("current_version", "The core version this instance runs"),
                    list("latest_version", "The newest published core version"),
                    f("update_available", "Whether a newer core version is published").kind(FieldKind::Bool),
                    list("feed", "Where the releases were read from"),
                    list("feed_error", "Why the feed could not be read, if it could not"),
                    list("checked_at", "When the feed was read (Unix seconds)"),
                    list("releases", "Published releases, newest first, each with its notes, whether the installed modules fit it, and what blocks it (a list)"),
                    list("dropped", "Feed entries that were left out because they failed their integrity check (a list)"),
                    list("instructions", "The commands to update by hand (a list)"),
                    list("backup_dir", "Where the backup before an update is written, if configured"),
                    list("state", "idle, requested, running, succeeded, failed or rolled_back"),
                    list("kind", "update or rollback"),
                    list("target_version", "The version the request moves to"),
                    list("from_version", "The version the request moves from"),
                    list("phase", "What the upgrade is doing right now"),
                    list("error", "Why the request failed or was rolled back"),
                    list("backup", "Path of the backup taken before the update"),
                    list("started_at", "When the request was made (Unix seconds)"),
                    list("started_by", "Who made the request"),
                    list("finished_at", "When it finished (Unix seconds)"),
                    f("can_rollback", "Whether the last update can be reversed through the control plane").kind(FieldKind::Bool),
                ],
                commands: vec![
                    CommandDef::new(
                        "check",
                        "Read the release feed and judge each newer release against the installed modules; includes the state of the last request",
                        ADMIN_GROUP,
                    ),
                    CommandDef::new(
                        "status",
                        "The state of the last update request, refreshed from the control plane while it runs",
                        ADMIN_GROUP,
                    ),
                    CommandDef::new(
                        "start",
                        "Update the instance to a published core version: takes a backup, then asks the control plane to run the upgrade. Give version",
                        ADMIN_GROUP,
                    )
                    .approval(),
                    CommandDef::new(
                        "rollback",
                        "Ask the control plane to put the instance back on the version before the last update",
                        ADMIN_GROUP,
                    )
                    .approval(),
                ],
                ext: Default::default(),
            },
            ModelDef {
                name: "setting",
                description: "Instance-wide interface settings: theme and the app launcher layout",
                read_group: ANY_GROUP,
                write_group: ADMIN_GROUP,
                access: None,
                fields: vec![FieldDef::new(
                    "value",
                    "The settings as a JSON object",
                    DataClass::None,
                    false,
                    ANY_GROUP,
                )],
                commands: vec![],
                ext: Default::default(),
            },
            // Written by obop-auth (D53), append-only: only get, list and search exist.
            ModelDef {
                name: "security_event",
                description: "A sign-in security event: two-factor enrolment, passkeys, use of a recovery code, reset or lockout. Append-only",
                read_group: ADMIN_GROUP,
                write_group: ADMIN_GROUP,
                access: None,
                fields: vec![
                    f("user", "The account the event is about"),
                    f(
                        "kind",
                        "What happened: enrol_begin, enrol_confirm, disable, reset, recovery_regenerate, recovery_used, code_failed, lockout, limited_session, login_refused, password_change, password_change_failed, password_reset, sessions_revoked, user_disable, user_enable, settings_change, module_install, module_update, module_rollback, module_uninstall, instance_update_requested, instance_update_finished, instance_update_failed, instance_rollback_requested, oauth_authorize, oauth_denied, oauth_revoked, oauth_refresh_reuse, oauth_code_reuse, passkey_register, passkey_rename, passkey_remove, passkey_used, passkey_login, passkey_failed, passkey_clone, passkey_passwordless",
                    ),
                    f(
                        "actor",
                        "Who did it: the user, an administrator or the operator",
                    ),
                    f("detail", "A short note without any code or secret"),
                    f("at", "When it happened, in UTC (RFC 3339)"),
                ],
                commands: vec![],
                ext: Default::default(),
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
            i.uninstall_as(&id, &caller(x))
                .map(|()| Value::map([("id", s(&id)), ("uninstalled", Value::Bool(true))]))
        }),
    );
    let i = inst.clone();
    d.register(
        &m,
        "module",
        "upgrade",
        Box::new(move |input| {
            let Value::Map(x) = input else {
                return Err("expected an object".into());
            };
            refuse_skipped_dependencies(x)?;
            i.update_outcome(&text(x, "id")?, &caller(x))
                .map(|o| outcome_row(&o))
        }),
    );
    let i = inst.clone();
    d.register(
        &m,
        "module",
        "upgrade_all",
        Box::new(move |input| {
            let Value::Map(x) = input else {
                return Err("expected an object".into());
            };
            let r = i.update_all(&caller(x))?;
            Ok(Value::map([
                (
                    "updated",
                    Value::List(
                        r.applied
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
                ),
                (
                    "skipped",
                    Value::List(
                        r.skipped
                            .iter()
                            .map(|(id, why)| Value::map([("id", s(id)), ("reason", s(why))]))
                            .collect(),
                    ),
                ),
            ]))
        }),
    );
    let i = inst.clone();
    d.register(
        &m,
        "module",
        "rollback",
        Box::new(move |input| {
            let Value::Map(x) = input else {
                return Err("expected an object".into());
            };
            i.rollback(&text(x, "id")?, &caller(x)).map(|p| pkg_row(&p))
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
            i.set_settings_as(&v, &caller(x))?;
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

    /// B7: a change to the `security` settings is logged with who made it and what changed.
    #[test]
    fn changes_to_the_security_settings_are_written_to_the_security_log() {
        let store = Store::new();
        let (d, _) = setup(&store);
        let upd = |v: &str| {
            call(
                &d,
                &admin(),
                "setting.update",
                Value::map([("value", Value::text(v))]),
            )
            .unwrap_or_else(|e| panic!("{e:?}"))
        };
        let log = || -> Vec<String> {
            store
                .backend()
                .list("security_event")
                .unwrap()
                .into_iter()
                .filter(|(_, f)| f["kind"] == "settings_change")
                .map(|(_, f)| {
                    format!(
                        "{} | {} | {}",
                        f["actor"].as_str().unwrap(),
                        f["user"].as_str().unwrap(),
                        f["detail"].as_str().unwrap()
                    )
                })
                .collect()
        };
        // Interface settings are not security settings: no event.
        upd(r##"{"theme":{"accent":"#123456"}}"##);
        assert!(log().is_empty());
        upd(r##"{"theme":{"accent":"#123456"},"security":{"require_mfa_for_admins":true}}"##);
        assert_eq!(
            log(),
            ["root | (instance) | security.require_mfa_for_admins: (unset) -> true"]
        );
        upd(r##"{"security":{"require_mfa_for_admins":false,"require_mfa_for_all":true}}"##);
        let l = log();
        assert_eq!(l.len(), 3, "{l:?}");
        assert!(l.contains(
            &"root | (instance) | security.require_mfa_for_admins: true -> false".to_string()
        ));
        assert!(l.contains(
            &"root | (instance) | security.require_mfa_for_all: (unset) -> true".to_string()
        ));
        // Saving the same security values again writes nothing.
        upd(
            r##"{"theme":{"accent":"#abcdef"},"security":{"require_mfa_for_admins":false,"require_mfa_for_all":true}}"##,
        );
        assert_eq!(log().len(), 3);
        // Nothing that looks like a secret is written, nor long or structured values.
        upd(
            r##"{"security":{"webhook_secret":"hunter2-hunter2","note":{"a":[1,2]},"session_hours":12}}"##,
        );
        let all = log().join("\n");
        assert!(!all.contains("hunter2"), "{all}");
        assert!(all.contains("security.webhook_secret: (unset) -> (hidden)"));
        assert!(all.contains("security.note: (unset) -> (changed)"));
        assert!(all.contains("security.session_hours: (unset) -> 12"));
        // The actor is the caller, and the log is readable by admins only.
        assert!(log().iter().all(|l| l.starts_with("root |")));
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
            .replacen(
                "[[model]]",
                "[module.dependencies]\nghost = \"^0.1\"\n\n[[model]]",
                1,
            );
        assert!(
            inst.install(&needs, "upload", "r")
                .unwrap_err()
                .contains("ghost ^0.1")
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
requires_core = "^0.2"

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
                access: None,
                fields: vec![],
                commands: vec![],
                ext: Default::default(),
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

    /// B1: a module installed, upgraded or removed through one server process shows on the
    /// others at their next look, without a restart.
    #[test]
    fn modules_changed_through_one_process_follow_on_the_others() {
        let store = Store::new();
        let ((da, a), (db, b)) = (setup(&store), setup(&store));
        let manager = Principal::user("m", &["fleet.user", "fleet.manager"]);
        let visible = |d: &Dispatcher| {
            d.meta(Surface::Rest, &manager)
                .iter()
                .any(|m| m.id == "fleet")
        };
        let sync = |i: &Installer| {
            i.last_check_ms
                .store(0, std::sync::atomic::Ordering::Release);
            i.sync_modules();
        };
        a.install(&fleet_with_labels("0.3.0", FLEET_LABELS), "upload", "root")
            .unwrap();
        assert!(visible(&da));
        assert!(!visible(&db), "the other process has not looked yet");
        sync(&b);
        assert!(visible(&db), "installed through A, registered on B");
        assert_eq!(fleet_meta(&db).labels["de"], "Fuhrpark");
        // Looking again with nothing changed does nothing (and does not re-register).
        sync(&b);
        // An upgrade through A replaces the registration on B.
        let newer = FLEET_LABELS.replace("Fuhrpark", "Fahrzeugpark");
        a.install(&fleet_with_labels("0.4.0", &newer), "upload", "root")
            .unwrap();
        sync(&b);
        assert_eq!(fleet_meta(&db).labels["de"], "Fahrzeugpark");
        // An install through B is not undone by B's own next look, and A follows.
        b.install(&fleet_with_labels("0.5.0", &newer), "upload", "root")
            .unwrap();
        sync(&b);
        assert!(visible(&db));
        sync(&a);
        assert!(visible(&da));
        // A removal through A removes it on B.
        a.uninstall("fleet").unwrap();
        assert!(!visible(&da));
        assert!(visible(&db));
        sync(&b);
        assert!(!visible(&db), "uninstalled through A, gone on B");
        // The dispatcher's own entry points do the looking when the hook is set.
        a.install(&fleet_with_labels("0.6.0", &newer), "upload", "root")
            .unwrap();
        let hook = b.clone();
        db.set_sync_hook(Box::new(move || {
            hook.last_check_ms
                .store(0, std::sync::atomic::Ordering::Release);
            hook.sync_modules()
        }));
        assert!(visible(&db), "meta() syncs first");
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
