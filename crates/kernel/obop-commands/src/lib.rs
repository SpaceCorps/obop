//! One dispatch path for every surface (D29, D31, D41): REST, MCP and CLI calls all end in
//! `Dispatcher::call`, which checks the surface, the permission (D18: user groups intersected
//! with token scopes for agents), restores masked input tokens (D40), runs the handler and
//! shapes the output through the masking policy.

pub mod audit;
mod shared;
pub use shared::Shared;
use shared::{Pending, Plans};

use obop_masking::mode_for;
pub use obop_masking::{PrincipalKind, RestoreError, Vault};
use obop_registry::labels::Texts;
use obop_registry::{FieldDef, GENERATED_COMMANDS, Module, Transition, Translations};
use obop_types::MaskMode;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    /// A JSON number that is not a 64 bit integer (a float, or an integer too large), kept as
    /// written. No field kind accepts it, so a float never turns into a value by accident; a
    /// decimal is sent as a string (D72).
    Num(String),
    Text(String),
    List(Vec<Value>),
    Map(BTreeMap<String, Value>),
}

impl Value {
    pub fn text(s: &str) -> Self {
        Self::Text(s.into())
    }
    pub fn map<const N: usize>(items: [(&str, Value); N]) -> Self {
        Self::Map(items.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Rest,
    Mcp,
    Cli,
}

/// The instance administrator group.
pub const ADMIN_GROUP: &str = "platform.admin";

/// A command group that every authenticated principal satisfies.
pub const ANY_GROUP: &str = "*";

#[derive(Debug, Clone)]
pub struct Principal {
    pub id: String,
    pub kind: PrincipalKind,
    /// Permission groups of the principal (for an agent: of the user it acts for).
    pub groups: HashSet<String>,
    /// Token scopes. `None` means unrestricted; agents must carry `Some`.
    pub scopes: Option<HashSet<String>>,
    /// A session that signed in without the second factor the instance requires (D53). It may
    /// only reach the two-factor enrolment routes; every command refuses it.
    pub mfa_limited: bool,
}

impl Principal {
    pub fn user(id: &str, groups: &[&str]) -> Self {
        Self {
            id: id.into(),
            kind: PrincipalKind::User,
            groups: set(groups),
            scopes: None,
            mfa_limited: false,
        }
    }

    pub fn agent(id: &str, on_behalf_groups: &[&str], scopes: &[&str]) -> Self {
        Self {
            id: id.into(),
            kind: PrincipalKind::Agent,
            groups: set(on_behalf_groups),
            scopes: Some(set(scopes)),
            mfa_limited: false,
        }
    }

    /// Effective permission: the group is held (`platform.admin` holds every group, so modules
    /// installed later need no re-granting) and, if scoped, the scope allows it. An agent's scopes
    /// always list concrete groups, so an admin's agent still gets only what it was scoped to.
    pub fn allows(&self, group: &str) -> bool {
        // A session limited to two-factor enrolment holds nothing (D53), not even `*`.
        if self.mfa_limited {
            return false;
        }
        // `*` means any signed-in principal (settings every user may read).
        if group == ANY_GROUP {
            return true;
        }
        let held = self.groups.contains(group) || self.groups.contains(ADMIN_GROUP);
        held && self.scopes.as_ref().is_none_or(|s| s.contains(group))
    }
}

fn set(items: &[&str]) -> HashSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[derive(Debug, PartialEq, Eq)]
pub enum CallError {
    UnknownCommand(String),
    /// The command is not exposed on this surface.
    NotOnSurface(String),
    Forbidden(String),
    /// A masked token in the input could not be restored.
    Restore(RestoreError),
    /// An agent proposed a command that needs human approval; approve `plan_id` to run it.
    ApprovalRequired {
        plan_id: String,
    },
    UnknownPlan(String),
    /// The record is no longer at the version the caller read (D75). `current` is the version it
    /// has now. Nothing was written. No producer yet.
    PreconditionFailed {
        current: i64,
    },
    /// The call ran and was refused. The text may start with a code of `errors::TABLE`
    /// (`code: message`); `errors::classify` is the only reader of it.
    Handler(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandInfo {
    pub id: String,
    pub description: &'static str,
    pub permission_group: &'static str,
    pub needs_approval: bool,
    /// For a state transition (D48): the governed field and the values it moves between.
    pub transition: Option<Transition>,
    /// Display name per language (D52); empty in the command catalog, set in `meta`.
    pub labels: Texts,
    pub descriptions: Texts,
}

/// What a guard decides about one call (D48).
#[derive(Debug, Default)]
pub struct Verdict {
    /// The input to run instead of the one received, when the guard normalised it (a default
    /// filled in).
    pub input: Option<Value>,
    /// Groups of the transitions this call implicitly runs. The caller already holds them (the
    /// guard checked); a human approving a proposal must hold them too.
    pub requires: Vec<&'static str>,
    /// An agent may only propose this call, although the command itself needs no approval.
    pub approval: bool,
    /// Transition actions to run before the command, so an implicit transition does everything
    /// the action does (a number assigned on posting), not only set the field.
    pub pre: Vec<Pre>,
}

/// An action the dispatcher runs ahead of an `update` that moves a governed field.
#[derive(Debug)]
pub struct Pre {
    /// Command id, `model.action`.
    pub command: String,
    pub input: Value,
    /// The governed field the action sets. When the action has a registered handler the field
    /// is dropped from the update, which would only repeat the write.
    pub field: &'static str,
}

/// A pre-hook on one command, run by the dispatcher after the permission check and before the
/// call executes or is turned into a proposal. It sees the principal and the restored input and
/// may refuse with a message. It is re-run when a proposal is approved, because the record may
/// have changed since.
pub type Guard = Box<dyn Fn(&Principal, &Value) -> Result<Verdict, String> + Send + Sync>;

struct Guarded {
    module: String,
    /// Mutating guards run under the dispatcher's transaction lock, so the check and the write
    /// of one call cannot interleave with another guarded call.
    serialize: bool,
    guard: Guard,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Response {
    pub value: Value,
    /// Fields withheld from this principal (`model.field`), so agents learn what they cannot see.
    pub denied: Vec<String>,
}

pub type Handler = Box<dyn Fn(&Value) -> Result<Value, String> + Send + Sync>;

struct Registered {
    /// `model.command`.
    id: String,
    /// Id of the module that registered the command, for uninstall.
    module: String,
    description: &'static str,
    needs_approval: bool,
    model: &'static str,
    permission_group: &'static str,
    surfaces: obop_registry::Surfaces,
    transition: Option<Transition>,
    /// `search`: the handler returns `{rows, total}` and only the rows are shaped.
    envelope: bool,
    /// The handler is one of the generated ones: it needs to know who is calling (row-level
    /// access, the groups of effects), so the dispatcher puts the actor in its input.
    actor: bool,
    /// The command changes records.
    writes: bool,
    handler: Handler,
}

/// Commands can be registered and removed while the server runs (module install and uninstall),
/// so the tables sit behind locks and handlers run on `Arc` clones taken out of the lock.
#[derive(Default)]
pub struct Dispatcher {
    /// Proposed calls waiting for approval: in memory, or shared between processes (B1).
    plans: Plans,
    /// Run before commands, the catalogue and the metadata are looked up: lets the installer
    /// bring this process's modules in line with what another process installed (B1).
    sync_hook: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>>,
    shared: std::sync::OnceLock<Shared>,
    commands: std::sync::RwLock<HashMap<String, Arc<Registered>>>,
    /// Shared with the generated `delete` handler, which looks for records that refer to the
    /// one being deleted in every registered model.
    models: Arc<std::sync::RwLock<HashMap<String, ModelMeta>>>,
    /// Guards by command id (D48).
    guards: std::sync::RwLock<HashMap<String, Arc<Guarded>>>,
    /// Held across the guard and the handler of every mutating guarded call.
    txn: std::sync::Mutex<()>,
    /// Module ids in the order they were first registered, so the launcher is stable.
    order: std::sync::RwLock<Vec<String>>,
    /// Public declarations by model name, with the module that registered them (D50).
    public: std::sync::RwLock<HashMap<String, (String, PublicSpec)>>,
    /// Observers told about every command that succeeded (see `observe`).
    observers: std::sync::RwLock<Vec<(String, Arc<Observer>)>>,
    /// Uploads and downloads of images and files, created with the first module registered
    /// against a store (D60).
    blob_service: std::sync::OnceLock<Arc<BlobService>>,
    /// Wrappers around a command's handler, by command id (see `around`).
    around: std::sync::RwLock<HashMap<String, Arc<Around>>>,
    /// Models whose writes run under the transaction lock because a rule needs it: a unique
    /// field, a sequence, an effect, or being the target of one (D56, D57).
    lock_models: std::sync::RwLock<HashSet<String>>,
    /// The time sequences and `$now` use.
    clock: SharedClock,
    /// Set once a test replaced the clock.
    clock_replaced: Arc<std::sync::atomic::AtomicBool>,
    /// The time zone database and the platform zone (D72).
    zones: engine::SharedZones,
}

/// The clock of the engine; tests replace it to cross a year end.
#[derive(Clone)]
pub struct SharedClock(Arc<std::sync::RwLock<ClockFn>>);

impl Default for SharedClock {
    fn default() -> Self {
        Self(Arc::new(std::sync::RwLock::new(engine::system_clock())))
    }
}

/// A wrapper around a command's handler: it sees who calls and the input, and decides whether and
/// how to call the handler it wraps (the `&dyn Fn`). It runs only when the command really runs,
/// after the guards and after any approval, so a proposal an agent makes calls nothing outside.
pub type Around = dyn Fn(&Principal, &Value, &dyn Fn(&Value) -> Result<Value, String>) -> Result<Value, String>
    + Send
    + Sync;

/// What an observer sees of a command that succeeded: its id, who ran it, the input the handler
/// received and the raw output, before it was shaped for the caller.
pub type Observer = dyn Fn(&str, &Principal, &Value, &Value) + Send + Sync;

pub(crate) struct ModelMeta {
    pub(crate) def: Arc<obop_registry::ModelDef>,
    module: String,
    module_name: String,
    description: &'static str,
    pub(crate) fields: Vec<FieldDef>,
    module_description: String,
    translations: Arc<Translations>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldInfo {
    pub name: &'static str,
    pub description: &'static str,
    pub class: &'static str,
    pub personal_data: bool,
    /// Lowercase field kind (D47, D72): text, longtext, number, money, date, datetime, decimal,
    /// bool, select, ref, image or file.
    pub kind: &'static str,
    pub required: bool,
    /// Options of a `select`; empty for other kinds.
    pub options: Vec<&'static str>,
    /// Target model of a `ref`.
    pub to: Option<&'static str>,
    /// Largest upload in bytes of an `image` or `file` field.
    pub max_bytes: Option<usize>,
    /// Digits in all, digits after the point and the import rounding mode (`half_even`,
    /// `half_up`, `down`) of a `decimal` field (D72).
    pub precision: Option<u8>,
    pub scale: Option<u8>,
    pub round: Option<&'static str>,
    /// Label and description per language (D52); empty maps when untranslated.
    pub labels: Texts,
    pub descriptions: Texts,
    /// Select option -> language -> label.
    pub option_labels: BTreeMap<String, Texts>,
    /// The field is the target of an action: it moves only through the model's actions (D48).
    pub governed: bool,
    /// Computed from the record and its children when read; read-only (D57).
    pub computed: bool,
    /// No two records may share the value (D57).
    pub unique: bool,
    /// Numbered by the system (D57).
    pub sequence: bool,
}

impl FieldInfo {
    /// The regular expression for the text of a `decimal` field (OpenAPI and MCP `pattern`).
    pub fn decimal_pattern(&self) -> Option<String> {
        Some(obop_values::decimal::pattern(self.precision?, self.scale?))
    }

    /// The pattern of the rows of `bulk_create` and `obop import`: a decimal that rounds on
    /// import takes any number of fractional digits there.
    pub fn decimal_import_pattern(&self) -> Option<String> {
        let (p, s) = (self.precision?, self.scale?);
        Some(if self.round.is_some() {
            obop_values::decimal::import_pattern(p, s)
        } else {
            obop_values::decimal::pattern(p, s)
        })
    }

    /// A value that fits this decimal, for examples (`12.33`, `12` at scale 0, `0.33` for 2,2).
    pub fn decimal_example(&self) -> Option<String> {
        Some(obop_values::decimal::example(self.precision?, self.scale?))
    }
}

/// The row-level access rules of a model, for the interface (D58).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccessInfo {
    pub own: Option<&'static str>,
    pub via: Option<&'static str>,
    pub public_field: Option<&'static str>,
    pub shared_with: Option<&'static str>,
    /// The holders of any of these groups see every record.
    pub see_all: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfo {
    pub name: String,
    pub description: &'static str,
    /// Fields this principal may read.
    pub fields: Vec<FieldInfo>,
    /// Commands beyond get, list, create and update that this principal may call.
    pub actions: Vec<CommandInfo>,
    pub can_create: bool,
    /// The caller may create many records at once (D70).
    pub can_bulk_create: bool,
    pub can_update: bool,
    pub can_delete: bool,
    pub labels: Texts,
    pub descriptions: Texts,
    /// Records the caller sees are limited by these rules (D58); none: all of the model.
    pub access: Option<AccessInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleInfo {
    pub id: String,
    pub name: String,
    /// English description; empty when the module declares none.
    pub description: String,
    pub labels: Texts,
    pub descriptions: Texts,
    /// The card summary per language (D52); empty when untranslated.
    pub summaries: Texts,
    pub models: Vec<ModelInfo>,
}

impl Dispatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Share this dispatcher's write lock and proposed plans with other server processes on the
    /// same database (B1): the lock becomes the backend's cross-process lock and plans become
    /// sealed, expiring records. Call once, before serving.
    pub fn set_shared(&self, backend: Arc<dyn obop_storage::Backend>, ring: obop_keys::KeyRing) {
        let _ = self.shared.set(Shared::new(backend, ring));
    }

    /// The backend shared with other processes, when `set_shared` was called.
    /// Set the function run at the start of `call`, `approve`, `catalog` and `meta`. It must be
    /// cheap when there is nothing to do.
    pub fn set_sync_hook(&self, hook: Box<dyn Fn() + Send + Sync>) {
        let _ = self.sync_hook.set(hook);
    }

    fn sync(&self) {
        if let Some(h) = self.sync_hook.get() {
            h();
        }
    }

    pub fn shared_backend(&self) -> Option<Arc<dyn obop_storage::Backend>> {
        self.shared.get().map(|s| s.backend.clone())
    }

    /// The backend and key ring shared with other processes, when `set_shared` was called.
    pub fn shared_parts(&self) -> Option<(Arc<dyn obop_storage::Backend>, obop_keys::KeyRing)> {
        self.shared
            .get()
            .map(|s| (s.backend.clone(), s.ring.clone()))
    }

    fn txn_guard(&self) -> Result<shared::TxnGuard<'_>, CallError> {
        shared::txn_lock(self.shared.get(), &self.txn).map_err(CallError::Handler)
    }

    /// What `tools/list`, `GET /api/commands` and CLI help show for this principal.
    pub fn catalog(&self, surface: Surface, p: &Principal) -> Vec<CommandInfo> {
        self.sync();
        self.visible(surface, p)
            .into_iter()
            .map(|id| {
                let c = self.commands.read().unwrap()[&id].clone();
                CommandInfo {
                    id,
                    description: c.description,
                    permission_group: c.permission_group,
                    needs_approval: c.needs_approval,
                    transition: c.transition,
                    labels: Texts::new(),
                    descriptions: Texts::new(),
                }
            })
            .collect()
    }

    /// What the UI renders from: the modules, models, readable fields and actions this principal
    /// can see. A model appears only if its `list` command is visible, and a field only if the
    /// principal may read it, so metadata never reveals what data would not.
    pub fn meta(&self, surface: Surface, p: &Principal) -> Vec<ModuleInfo> {
        self.sync();
        let cmds = self.commands.read().unwrap();
        let models = self.models.read().unwrap();
        let order = self.order.read().unwrap();
        let visible =
            |c: &Registered| on_surface(c.surfaces, surface) && p.allows(c.permission_group);
        let mut out = Vec::new();
        for module in order.iter() {
            let mut infos = Vec::new();
            let mut names: Vec<&String> = models
                .iter()
                .filter(|(_, m)| &m.module == module)
                .map(|(n, _)| n)
                .collect();
            names.sort();
            for name in names {
                let m = &models[name];
                let Some(list) = cmds.get(&format!("{name}.list")) else {
                    continue;
                };
                if !visible(list) {
                    continue;
                }
                let can = |c: &str| cmds.get(&format!("{name}.{c}")).is_some_and(|r| visible(r));
                let mut actions: Vec<CommandInfo> = cmds
                    .iter()
                    .filter(|(id, c)| {
                        c.model == name.as_str()
                            && id.starts_with(&format!("{name}."))
                            && !GENERATED_COMMANDS.contains(&&id[name.len() + 1..])
                            && visible(c)
                    })
                    .map(|(id, c)| {
                        let tr = m
                            .translations
                            .models
                            .get(name.as_str())
                            .and_then(|t| t.actions.get(&id[name.len() + 1..]));
                        CommandInfo {
                            id: id.clone(),
                            description: c.description,
                            permission_group: c.permission_group,
                            needs_approval: c.needs_approval,
                            transition: c.transition,
                            labels: tr.map(|t| t.labels.clone()).unwrap_or_default(),
                            descriptions: tr.map(|t| t.descriptions.clone()).unwrap_or_default(),
                        }
                    })
                    .collect();
                actions.sort_by(|a, b| a.id.cmp(&b.id));
                let mtr = m.translations.models.get(name.as_str());
                infos.push(ModelInfo {
                    name: name.clone(),
                    description: m.description,
                    labels: mtr.map(|t| t.entry.labels.clone()).unwrap_or_default(),
                    descriptions: mtr
                        .map(|t| t.entry.descriptions.clone())
                        .unwrap_or_default(),
                    fields: m
                        .fields
                        .iter()
                        .filter(|f| p.allows(f.permission_group))
                        .map(|f| {
                            let ftr = mtr.and_then(|t| t.fields.get(f.name));
                            FieldInfo {
                                name: f.name,
                                description: f.description,
                                class: f.class.map_or("none", |c| c.as_str()),
                                personal_data: f.personal_data,
                                kind: f.kind.name(),
                                required: f.required,
                                options: f.kind.options().to_vec(),
                                to: f.kind.target(),
                                max_bytes: f.kind.max_bytes(),
                                precision: f.kind.decimal_shape().map(|(p, _, _)| p),
                                scale: f.kind.decimal_shape().map(|(_, s, _)| s),
                                round: f
                                    .kind
                                    .decimal_shape()
                                    .and_then(|(_, _, r)| r)
                                    .map(|r| r.name()),
                                labels: ftr.map(|t| t.entry.labels.clone()).unwrap_or_default(),
                                descriptions: ftr
                                    .map(|t| t.entry.descriptions.clone())
                                    .unwrap_or_default(),
                                option_labels: ftr.map(|t| t.options.clone()).unwrap_or_default(),
                                governed: m.def.governed_fields().contains(&f.name),
                                computed: f.computed.is_some(),
                                unique: f.unique.is_some() || f.sequence.is_some(),
                                sequence: f.sequence.is_some(),
                            }
                        })
                        .collect(),
                    actions,
                    can_create: can("create"),
                    can_bulk_create: can("bulk_create"),
                    can_update: can("update"),
                    can_delete: can("delete"),
                    access: m
                        .def
                        .access
                        .as_ref()
                        .filter(|a| !a.is_empty())
                        .map(|a| AccessInfo {
                            own: a.own,
                            via: a.via,
                            public_field: a.public_field,
                            shared_with: a.shared_with,
                            see_all: a.see_all.clone(),
                        }),
                });
            }
            if !infos.is_empty() {
                let first = models.values().find(|m| &m.module == module);
                let name = first
                    .map(|m| m.module_name.clone())
                    .unwrap_or_else(|| module.clone());
                let (description, labels, descriptions, summaries) =
                    first.map_or_else(Default::default, |m| {
                        (
                            m.module_description.clone(),
                            m.translations.module.labels.clone(),
                            m.translations.module.descriptions.clone(),
                            m.translations.summaries.clone(),
                        )
                    });
                out.push(ModuleInfo {
                    id: module.clone(),
                    name,
                    description,
                    labels,
                    descriptions,
                    summaries,
                    models: infos,
                });
            }
        }
        out
    }

    /// Bind a handler to a command declared in `module`'s registry metadata. A handler for an
    /// undeclared command is a programming error: the surface is generated from the registry.
    pub fn register(&self, module: &Module, model: &str, command: &str, handler: Handler) {
        self.register_inner(module, model, command, handler, false);
    }

    /// Like `register`, for the generated handlers, which read the caller from their input.
    pub(crate) fn register_generic(
        &self,
        module: &Module,
        model: &str,
        command: &str,
        handler: Handler,
    ) {
        self.register_inner(module, model, command, handler, true);
    }

    fn register_inner(
        &self,
        module: &Module,
        model: &str,
        command: &str,
        handler: Handler,
        actor: bool,
    ) {
        let m = module
            .models
            .iter()
            .find(|m| m.name == model)
            .unwrap_or_else(|| panic!("{}: no model {model}", module.id));
        let c = m
            .all_commands()
            .into_iter()
            .find(|c| c.name == command)
            .unwrap_or_else(|| panic!("{}.{model}: command {command} is not declared", module.id));
        self.models.write().unwrap().insert(
            m.name.to_string(),
            ModelMeta {
                def: Arc::new(m.clone()),
                module: module.id.clone(),
                module_name: module.name.clone(),
                module_description: module.description.clone(),
                translations: module.translations.clone(),
                description: m.description,
                fields: m.fields.clone(),
            },
        );
        {
            let mut order = self.order.write().unwrap();
            if !order.contains(&module.id) {
                order.push(module.id.clone());
            }
        }
        self.commands.write().unwrap().insert(
            format!("{model}.{command}"),
            Arc::new(Registered {
                id: format!("{model}.{command}"),
                module: module.id.clone(),
                description: c.description,
                needs_approval: c.needs_approval,
                model: m.name,
                permission_group: c.permission_group,
                surfaces: c.surfaces,
                transition: c.transition,
                // `search` returns {rows, total} and `bulk_create` a report: only rows are shaped.
                envelope: matches!(command, "search" | "bulk_create"),
                actor,
                writes: !matches!(command, "get" | "list" | "search"),
                handler,
            }),
        );
    }

    /// Attach a guard to the command `id` (`model.command`) of `module`. It is removed with the
    /// module. `serialize` makes the call hold the transaction lock from guard to handler.
    pub fn register_guard(&self, module: &str, id: &str, serialize: bool, guard: Guard) {
        self.guards.write().unwrap().insert(
            id.to_string(),
            Arc::new(Guarded {
                module: module.into(),
                serialize,
                guard,
            }),
        );
    }

    /// Like `register_guard`, but keeps the guard already on the command (the generated one):
    /// that one runs first, and `guard` then sees the input it produced. The verdicts combine.
    /// A module uses this to add rules that need the caller (ownership of a record) without
    /// replacing what the registry metadata implies.
    pub fn add_guard(&self, module: &str, id: &str, serialize: bool, guard: Guard) {
        let mut guards = self.guards.write().unwrap();
        let inner = guards.get(id).cloned();
        let combined: Guard = match inner {
            None => guard,
            Some(first) => Box::new(move |p, input| {
                let mut v = (first.guard)(p, input)?;
                let seen = v.input.take();
                let second = guard(p, seen.as_ref().unwrap_or(input))?;
                v.requires.extend(second.requires);
                v.approval |= second.approval;
                v.pre.extend(second.pre);
                v.input = second.input.or(seen);
                Ok(v)
            }),
        };
        let serialize = serialize || guards.get(id).is_some_and(|g| g.serialize);
        guards.insert(
            id.to_string(),
            Arc::new(Guarded {
                module: module.into(),
                serialize,
                guard: combined,
            }),
        );
    }

    /// Wrap the handler of command `id` (`model.command`). The wrapper survives the command being
    /// registered again, which happens when a package is reinstalled. Used where a command has
    /// to do something outside the store that must succeed or fail with it, such as returning
    /// money before an order is marked refunded (D62). A second wrapper for the same id replaces
    /// the first.
    pub fn around(&self, id: &str, wrapper: Box<Around>) {
        self.around
            .write()
            .unwrap()
            .insert(id.to_string(), Arc::from(wrapper));
    }

    /// Be told about every command that succeeds, with the module that owns the observer (removed
    /// with it). Observers run inside the call, under the dispatcher's locks: they must be quick
    /// and must not call the dispatcher; hand the work to another thread.
    pub fn observe(&self, module: &str, observer: Box<Observer>) {
        self.observers
            .write()
            .unwrap()
            .push((module.to_string(), Arc::from(observer)));
    }

    /// The field definitions of a registered model, including those a principal could not read.
    /// For components that apply their own policy by data class (connectors).
    pub fn model_fields(&self, model: &str) -> Option<Vec<FieldDef>> {
        self.models
            .read()
            .unwrap()
            .get(model)
            .map(|m| m.fields.clone())
    }

    /// The definition of a registered model (fields, groups, commands).
    pub fn model_def(&self, model: &str) -> Option<obop_registry::ModelDef> {
        self.models
            .read()
            .unwrap()
            .get(model)
            .map(|m| (*m.def).clone())
    }

    /// Remove every command a module registered. Its data stays in the store.
    pub fn unregister_module(&self, module: &str) {
        self.public.write().unwrap().retain(|_, (m, _)| m != module);
        self.observers.write().unwrap().retain(|(m, _)| m != module);
        self.guards
            .write()
            .unwrap()
            .retain(|_, g| g.module != module);
        let mut cmds = self.commands.write().unwrap();
        let models: HashSet<&str> = cmds
            .values()
            .filter(|c| c.module == module)
            .map(|c| c.model)
            .collect();
        let models: Vec<String> = models.into_iter().map(String::from).collect();
        cmds.retain(|_, c| c.module != module);
        drop(cmds);
        {
            let mut f = self.models.write().unwrap();
            for m in models {
                f.remove(&m);
            }
        }
        self.order.write().unwrap().retain(|x| x != module);
        self.refresh_lock_models();
    }

    /// Recompute which models need the write lock: those with a unique field, a sequence or
    /// an effect, and every model an effect may write.
    pub(crate) fn refresh_lock_models(&self) {
        let mut set = HashSet::new();
        for (name, meta) in self.models.read().unwrap().iter() {
            if meta.def.has_rules() {
                set.insert(name.clone());
            }
            for c in &meta.def.commands {
                for e in &c.effects {
                    set.insert(e.model().to_string());
                }
            }
        }
        *self.lock_models.write().unwrap() = set;
    }

    /// Replace the clock sequences and `$now` read (seconds since the epoch). For tests.
    pub fn set_clock(&self, clock: ClockFn) {
        *self.clock.0.write().unwrap() = clock;
        self.clock_replaced
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Replace the time zone settings (the database and the platform zone, D72). The server
    /// sets them from `OBOP_ZONE` at start; tests swap in a fixture database.
    pub fn set_zones(&self, zones: obop_values::Zones) {
        *self.zones.write().unwrap() = Arc::new(zones);
    }

    /// The time zone settings in force.
    pub fn zones(&self) -> Arc<obop_values::Zones> {
        self.zones.read().unwrap().clone()
    }

    /// Every permission group the registered models, fields and commands refer to (the
    /// administrator group included, `*` excluded), sorted. These are the groups an account can
    /// be given (D69); a module installed later adds its own.
    pub fn known_groups(&self) -> Vec<String> {
        self.sync();
        let mut g: std::collections::BTreeSet<String> = [ADMIN_GROUP.to_string()].into();
        for m in self.models.read().unwrap().values() {
            let d = &m.def;
            let from_model = [d.read_group, d.write_group]
                .into_iter()
                .chain(d.fields.iter().map(|f| f.permission_group))
                .chain(d.commands.iter().map(|c| c.permission_group))
                .chain(d.access.iter().flat_map(|a| a.see_all.iter().copied()));
            g.extend(from_model.filter(|x| *x != ANY_GROUP).map(String::from));
        }
        g.into_iter().collect()
    }

    /// Whether any command of `model` is registered.
    pub fn has_model(&self, model: &str) -> bool {
        self.commands
            .read()
            .unwrap()
            .values()
            .any(|c| c.model == model)
    }

    /// Command ids the principal may call on `surface`: what MCP `tools/list` and CLI help show.
    pub fn visible(&self, surface: Surface, p: &Principal) -> Vec<String> {
        let cmds = self.commands.read().unwrap();
        let mut v: Vec<_> = cmds
            .iter()
            .filter(|(_, c)| on_surface(c.surfaces, surface) && p.allows(c.permission_group))
            .map(|(id, _)| id.clone())
            .collect();
        v.sort();
        v
    }

    pub fn call(
        &self,
        surface: Surface,
        p: &Principal,
        vault: &mut Vault,
        id: &str,
        input: &Value,
    ) -> Result<Response, CallError> {
        self.sync();
        let c = self
            .commands
            .read()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| CallError::UnknownCommand(id.into()))?;
        if !on_surface(c.surfaces, surface) {
            return Err(CallError::NotOnSurface(id.into()));
        }
        // Same answer for "no such group" and "scope too narrow": do not leak which.
        if !p.allows(c.permission_group) {
            return Err(CallError::Forbidden(id.into()));
        }
        let mut input = self.restore(p, vault, input)?;
        if let Value::Map(m) = &mut input {
            // The caller is set by the dispatcher and by guards, never by a client.
            m.remove("_caller");
            m.retain(|k, _| !k.starts_with(query::ACTOR_PREFIX));
        }
        let guard = self.guards.read().unwrap().get(id).cloned();
        let needs_lock = guard.as_ref().is_some_and(|g| g.serialize)
            || (c.writes && self.lock_models.read().unwrap().contains(c.model));
        let _txn = match needs_lock {
            true => Some(self.txn_guard()?),
            false => None,
        };
        let mut guard_approval = false;
        let mut pre = Vec::new();
        if let Some(g) = &guard {
            let v = (g.guard)(p, &input).map_err(CallError::Handler)?;
            guard_approval = v.approval;
            pre = v.pre;
            if let Some(i) = v.input {
                input = i;
            }
        }
        if (c.needs_approval || guard_approval) && p.kind == PrincipalKind::Agent {
            // Dry-run: keep the restored input, hand back a plan id (D18, D42).
            let plan_id = uuid::Uuid::now_v7().to_string();
            self.plans
                .put(
                    self.shared.get(),
                    &plan_id,
                    &Pending {
                        principal: p.clone(),
                        id: id.into(),
                        input,
                    },
                )
                .map_err(CallError::Handler)?;
            return Err(CallError::ApprovalRequired { plan_id });
        }
        self.run(&c, p, vault, input, pre)
    }

    /// A human approves a proposed plan. The command runs with the proposing agent's own
    /// permissions (still checked) and the approver must hold the command's group too.
    pub fn approve(
        &self,
        approver: &Principal,
        vault: &mut Vault,
        plan_id: &str,
    ) -> Result<Response, CallError> {
        self.sync();
        if approver.kind != PrincipalKind::User {
            return Err(CallError::Forbidden("approve".into()));
        }
        let _txn = self.txn_guard()?;
        let plan = self
            .plans
            .get(self.shared.get(), plan_id)
            .ok_or_else(|| CallError::UnknownPlan(plan_id.into()))?;
        let c = self
            .commands
            .read()
            .unwrap()
            .get(&plan.id)
            .cloned()
            .ok_or_else(|| CallError::UnknownCommand(plan.id.clone()))?;
        if !approver.allows(c.permission_group) || !plan.principal.allows(c.permission_group) {
            return Err(CallError::Forbidden(plan.id.clone()));
        }
        // The record may have changed since the proposal, so the guard runs again, and the
        // approver must hold the groups of any transition the call implicitly runs (D48).
        let mut input = plan.input.clone();
        let mut pre = Vec::new();
        let guard = self.guards.read().unwrap().get(&plan.id).cloned();
        if let Some(g) = guard {
            let v = (g.guard)(&plan.principal, &input).map_err(CallError::Handler)?;
            if v.requires.iter().any(|r| !approver.allows(r)) {
                return Err(CallError::Forbidden(plan.id.clone()));
            }
            pre = v.pre;
            if let Some(i) = v.input {
                input = i;
            }
        }
        self.plans.remove(self.shared.get(), plan_id);
        self.run(&c, &plan.principal, vault, input, pre)
    }

    fn run(
        &self,
        c: &Registered,
        p: &Principal,
        vault: &mut Vault,
        input: Value,
        pre: Vec<Pre>,
    ) -> Result<Response, CallError> {
        let mut input = input;
        for step in pre {
            let action = self.commands.read().unwrap().get(&step.command).cloned();
            if let Some(action) = action {
                let mut i = step.input;
                if let Value::Map(m) = &mut i {
                    m.insert("_caller".into(), Value::text(&p.id));
                    if action.actor {
                        Actor::of(p).stamp(m);
                    }
                }
                (action.handler)(&i).map_err(CallError::Handler)?;
                if let Value::Map(m) = &mut input {
                    m.remove(step.field);
                }
            }
        }
        // Handlers that need to know who is calling (audit trails) read `_caller`. The dispatcher
        // sets it after discarding anything a client sent under that name.
        if let Value::Map(m) = &mut input {
            m.insert("_caller".into(), Value::text(&p.id));
            if c.actor {
                Actor::of(p).stamp(m);
            }
        }
        let wrapper = self.around.read().unwrap().get(&c.id).cloned();
        let out = match wrapper {
            Some(w) => w(p, &input, &|i| (c.handler)(i)),
            None => (c.handler)(&input),
        }
        .map_err(CallError::Handler)?;
        let observers: Vec<Arc<Observer>> = self
            .observers
            .read()
            .unwrap()
            .iter()
            .map(|(_, o)| o.clone())
            .collect();
        for o in observers {
            o(&c.id, p, &input, &out);
        }
        let mut denied = Vec::new();
        let value = match out {
            // `search`: shape the rows, keep the envelope.
            Value::Map(mut m) if c.envelope => {
                if let Some(rows) = m.remove("rows") {
                    m.insert(
                        "rows".into(),
                        self.shape(c.model, p, vault, rows, &mut denied),
                    );
                }
                Value::Map(m)
            }
            out => self.shape(c.model, p, vault, out, &mut denied),
        };
        // A list shapes every row; name each withheld field once.
        let mut seen = HashSet::new();
        denied.retain(|d| seen.insert(d.clone()));
        Ok(Response { value, denied })
    }

    /// Replace tokens in input text with the values they stand for, if the principal could read
    /// the record they came from (D10).
    fn restore(&self, p: &Principal, vault: &Vault, v: &Value) -> Result<Value, CallError> {
        Ok(match v {
            Value::Text(t) => match vault.restore(t, |_| p.allows_any_read()) {
                Ok(real) => Value::Text(real.into()),
                Err(RestoreError::UnknownToken(_)) => v.clone(), // ordinary text
                Err(e) => return Err(CallError::Restore(e)),
            },
            Value::List(l) => Value::List(
                l.iter()
                    .map(|x| self.restore(p, vault, x))
                    .collect::<Result<_, _>>()?,
            ),
            Value::Map(m) => Value::Map(
                m.iter()
                    .map(|(k, x)| Ok((k.clone(), self.restore(p, vault, x)?)))
                    .collect::<Result<_, CallError>>()?,
            ),
            other => other.clone(),
        })
    }

    fn shape(
        &self,
        model: &str,
        p: &Principal,
        vault: &mut Vault,
        out: Value,
        denied: &mut Vec<String>,
    ) -> Value {
        match out {
            Value::List(items) => Value::List(
                items
                    .into_iter()
                    .map(|i| self.shape(model, p, vault, i, denied))
                    .collect(),
            ),
            Value::Map(m) => {
                let record = match m.get("id") {
                    Some(Value::Text(id)) => format!("{model}:{id}"),
                    Some(Value::Int(id)) => format!("{model}:{id}"),
                    _ => model.to_string(),
                };
                let mut shaped = BTreeMap::new();
                let defs = self
                    .models
                    .read()
                    .unwrap()
                    .get(model)
                    .map(|m| m.fields.clone());
                for (k, v) in m {
                    let Some(def) = defs.as_ref().and_then(|f| f.iter().find(|f| f.name == k))
                    else {
                        // Unclassified output is withheld: the gate guarantees declared fields only.
                        if k != "id" {
                            denied.push(format!("{model}.{k}"));
                            continue;
                        }
                        shaped.insert(k, v);
                        continue;
                    };
                    if !p.allows(def.permission_group) {
                        denied.push(format!("{model}.{k}"));
                        continue;
                    }
                    let class = def.class.unwrap_or(obop_types::DataClass::Secret);
                    match mode_for(p.kind, class, def.personal_data) {
                        MaskMode::Allow => {
                            shaped.insert(k, v);
                        }
                        MaskMode::Mask => {
                            let masked = match &v {
                                Value::Text(t) => Value::Text(vault.mask(&label(&k), &record, t)),
                                _ => Value::text("[masked]"),
                            };
                            shaped.insert(k, masked);
                        }
                        MaskMode::Redact => {
                            shaped.insert(k, Value::text("[redacted]"));
                        }
                        MaskMode::Deny => denied.push(format!("{model}.{k}")),
                    }
                }
                Value::Map(shaped)
            }
            other => other,
        }
    }
}

impl Principal {
    /// Placeholder for record-level read checks until `obop-security` exists: any principal that
    /// holds at least one group may restore tokens it was shown.
    fn allows_any_read(&self) -> bool {
        !self.groups.is_empty()
    }
}

fn label(field: &str) -> String {
    field.to_uppercase()
}

fn on_surface(s: obop_registry::Surfaces, surface: Surface) -> bool {
    match surface {
        Surface::Rest => s.rest,
        Surface::Mcp => s.mcp,
        Surface::Cli => s.cli,
    }
}

/// Surface naming adapters. All three map to the same command id, so a call is the same call.
/// Identity provider for the network surfaces (D33). Implemented by `obop-auth`.
pub trait Authenticator: Send + Sync {
    /// Resolve a bearer token to a principal; `None` for unknown, expired or revoked tokens.
    fn resolve(&self, token: &str) -> Option<Principal>;
    /// Exchange credentials for a session token. Refuses a user who has two-factor sign-in on:
    /// use `login_step` and `login_mfa` for those.
    fn login(&self, username: &str, password: &str) -> Result<String, String>;
    /// First sign-in step: a session, or the demand for a second factor (D53).
    fn login_step(&self, username: &str, password: &str) -> Result<LoginOutcome, String> {
        self.login(username, password)
            .map(|token| LoginOutcome::Session {
                token,
                mfa_enrolment_required: false,
                password_change_required: false,
            })
    }
    /// Second sign-in step: a TOTP `code` or a `recovery_code` for the short-lived `mfa_token`.
    fn login_mfa(
        &self,
        _mfa_token: &str,
        _code: Option<&str>,
        _recovery_code: Option<&str>,
    ) -> Result<LoginOutcome, MfaError> {
        Err(MfaError::NotFound)
    }
    /// Two-factor management, for authenticators that support it.
    fn mfa(&self) -> Option<&dyn MfaApi> {
        None
    }
    /// Account administration (D69), for authenticators that support it.
    fn accounts(&self) -> Option<&dyn AccountsApi> {
        None
    }
    /// Passkeys (WebAuthn, D65), for authenticators that support them.
    fn passkeys(&self) -> Option<&dyn PasskeyApi> {
        None
    }
    /// Resolve a bearer token and say which audience (resource) it was issued for. Tokens from
    /// the OAuth flow (D64) carry one and work only there; every other token has none.
    fn resolve_with_audience(&self, token: &str) -> Option<(Principal, Option<String>)> {
        self.resolve(token).map(|p| (p, None))
    }
    /// What the OAuth authorization server needs from the identity provider (D64).
    fn oauth(&self) -> Option<&dyn OAuthTokens> {
        None
    }
    /// How many people's sign-in sessions are valid now (not revoked, not expired), for the
    /// metrics endpoint (D67); `None` when the authenticator does not keep them.
    fn active_session_count(&self) -> Option<u64> {
        None
    }
    /// A human issues a scoped token for an agent acting on their behalf (D18). Scopes must be
    /// groups the human holds.
    fn issue_agent_token(
        &self,
        owner: &Principal,
        scopes: &[String],
        label: &str,
    ) -> Result<String, String>;
}

/// The few identity operations the OAuth authorization server (D64) needs. Tokens it issues are
/// the scoped agent tokens of D18 with a short life, an audience and a link to their grant.
pub trait OAuthTokens: Send + Sync {
    /// The person's principal (current groups); `None` when unknown or disabled.
    fn principal_of(&self, user: &str) -> Option<Principal>;
    /// A scoped agent token for `owner` that lives `ttl_secs`, works only for `audience` and
    /// belongs to `grant`. Returns the token (shown once) and its id (for `revoke_token_id`).
    /// Scopes must be groups the owner holds.
    fn issue_oauth_access(
        &self,
        owner: &str,
        scopes: &[String],
        label: &str,
        ttl_secs: u64,
        audience: &str,
        grant: &str,
    ) -> Result<(String, String), String>;
    /// Revoke a token by the id `issue_oauth_access` returned.
    fn revoke_token_id(&self, id: &str) -> bool;
    /// Revoke a session token that was only needed to prove a sign-in.
    fn revoke_session(&self, token: &str) -> bool;
    /// The grant an access token belongs to.
    fn grant_of(&self, token: &str) -> Option<String>;
    /// Append to the security event log (never a token, a code or a secret).
    fn audit(&self, user: &str, kind: &str, actor: &str, detail: &str);
}

/// What the first (or second) sign-in step produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginOutcome {
    Session {
        token: String,
        /// The session is limited to two-factor enrolment because the instance requires it.
        mfa_enrolment_required: bool,
        /// The session is limited to changing the password because an administrator reset it.
        password_change_required: bool,
    },
    MfaRequired {
        mfa_token: String,
    },
}

/// Why a two-factor call failed; the HTTP layer maps each to a status code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MfaError {
    /// Bad input or wrong state (already enrolled, nothing pending): 400.
    Invalid(String),
    /// A wrong code: 401 `invalid_code`.
    Denied(String),
    /// A wrong password: 401 `invalid_password`.
    BadPassword,
    /// The `mfa_token` is unknown, spent or older than five minutes: 401 `mfa_expired`.
    Expired,
    /// Too many attempts: 429.
    Locked(String),
    /// The caller may not do this (not an admin): 404, like an unknown command.
    NotFound,
    /// The server cannot do it (no `OBOP_SECRET_KEY`): 503.
    Unavailable(String),
}

/// Two-factor management of the signed-in human (D53). `session` is the bearer token of the
/// caller's session, so a limited session can be lifted once enrolment is confirmed.
pub trait MfaApi: Send + Sync {
    fn status(&self, p: &Principal) -> Result<serde_json::Value, MfaError>;
    /// Start enrolment: a fresh secret (base32 and `otpauth://`), nothing active yet.
    fn begin(&self, p: &Principal) -> Result<serde_json::Value, MfaError>;
    /// A valid code activates the pending secret and returns the recovery codes, once.
    fn confirm(
        &self,
        p: &Principal,
        session: &str,
        code: &str,
    ) -> Result<serde_json::Value, MfaError>;
    /// Needs the password and a current code or a recovery code. Signs out every session of the
    /// person (a stolen session must not outlive the second factor).
    fn disable(
        &self,
        p: &Principal,
        password: &str,
        code: Option<&str>,
        recovery_code: Option<&str>,
    ) -> Result<(), MfaError>;
    /// As `disable`, keeping the session the call came from signed in.
    fn disable_in_session(
        &self,
        p: &Principal,
        _session: &str,
        password: &str,
        code: Option<&str>,
        recovery_code: Option<&str>,
    ) -> Result<(), MfaError> {
        self.disable(p, password, code, recovery_code)
    }
    fn regenerate_recovery(
        &self,
        p: &Principal,
        password: &str,
        code: Option<&str>,
        recovery_code: Option<&str>,
    ) -> Result<serde_json::Value, MfaError>;
    /// `platform.admin` removes another user's second factor; never one's own.
    fn admin_reset(&self, admin: &Principal, target: &str) -> Result<(), MfaError>;

    // ---- sessions, password and accounts (B7) -------------------------------------------

    /// The person's own sessions: when they started, when they end, which one this is. Nothing
    /// in the answer can be used as a credential.
    fn sessions(&self, _p: &Principal, _session: &str) -> Result<serde_json::Value, MfaError> {
        Err(MfaError::NotFound)
    }
    /// Sign the person out: every session, or every one but the calling session. Returns how
    /// many ended. Agent tokens issued by the person are separate credentials and stay.
    fn revoke_sessions(
        &self,
        _p: &Principal,
        _session: &str,
        _keep_current: bool,
    ) -> Result<usize, MfaError> {
        Err(MfaError::NotFound)
    }
    /// Change one's own password: needs the current password and, when two-factor is on, a
    /// code or recovery code. Ends the person's other sessions.
    fn change_password(
        &self,
        _p: &Principal,
        _session: &str,
        _current: &str,
        _new: &str,
        _code: Option<&str>,
        _recovery_code: Option<&str>,
    ) -> Result<(), MfaError> {
        Err(MfaError::NotFound)
    }
    /// `platform.admin` lists the accounts (never a hash or a secret).
    fn users(&self, _admin: &Principal) -> Result<serde_json::Value, MfaError> {
        Err(MfaError::NotFound)
    }
    /// `platform.admin` disables or enables another account. Disabling ends its sessions and
    /// blocks its tokens; it is refused for oneself and for the last enabled administrator.
    fn set_user_disabled(
        &self,
        _admin: &Principal,
        _target: &str,
        _disabled: bool,
    ) -> Result<(), MfaError> {
        Err(MfaError::NotFound)
    }
    /// `platform.admin` sets a temporary password for another account: its sessions end and the
    /// person must choose a new password at the next sign-in.
    fn reset_user_password(
        &self,
        _admin: &Principal,
        _target: &str,
        _temporary: &str,
    ) -> Result<(), MfaError> {
        Err(MfaError::NotFound)
    }
}

/// Where a WebAuthn ceremony takes place, as the server sees it (D65). The HTTP layer derives it
/// from the public URL (or, without one, the request) so the origin and the relying party id can
/// never come from the browser's own claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebauthnCtx {
    /// The relying party id: the host of the origin, no port.
    pub rp_id: String,
    /// The origin the browser must report in `clientDataJSON` (scheme, host, port).
    pub origin: String,
    /// Shown by the authenticator next to the account.
    pub rp_name: String,
    /// The client address, for rate limits.
    pub ip: String,
}

/// What a person proves again before a passkey is added, renamed or removed: the password, and
/// when a second factor is on, a code, a recovery code or a passkey proof given beforehand
/// (`PasskeyApi::proof`).
#[derive(Debug, Clone, Copy, Default)]
pub struct Reauth<'a> {
    pub password: &'a str,
    pub code: Option<&'a str>,
    pub recovery_code: Option<&'a str>,
}

/// Passkey management and sign-in (WebAuthn, D65). Credentials are JSON as the browser's
/// `PublicKeyCredential` gives them, with every binary field base64url encoded.
pub trait PasskeyApi: Send + Sync {
    /// The person's passkeys (names, dates, flags; never anything usable as a credential).
    fn list(&self, p: &Principal, ctx: &WebauthnCtx) -> Result<serde_json::Value, MfaError>;
    /// Start adding a passkey: needs the password again (and a second factor when one is on).
    /// Returns `publicKey` creation options and consumes nothing but the re-authentication.
    fn register_options(
        &self,
        p: &Principal,
        ctx: &WebauthnCtx,
        reauth: &Reauth<'_>,
    ) -> Result<serde_json::Value, MfaError>;
    /// Finish adding it. The first second factor of a person also returns recovery codes, once.
    fn register_finish(
        &self,
        p: &Principal,
        session: &str,
        ctx: &WebauthnCtx,
        name: &str,
        credential: &serde_json::Value,
    ) -> Result<serde_json::Value, MfaError>;
    fn rename(
        &self,
        p: &Principal,
        id: &str,
        name: &str,
        reauth: &Reauth<'_>,
    ) -> Result<(), MfaError>;
    /// Remove one; refused when it is the last second factor and the policy requires one.
    /// Every other session of the person ends.
    fn remove(
        &self,
        p: &Principal,
        session: &str,
        id: &str,
        reauth: &Reauth<'_>,
    ) -> Result<(), MfaError>;
    /// Allow or forbid signing in with a passkey alone (user verification required).
    fn set_passwordless(
        &self,
        p: &Principal,
        on: bool,
        reauth: &Reauth<'_>,
    ) -> Result<(), MfaError>;
    /// `publicKey` request options for proving possession of a passkey inside a session.
    fn proof_options(
        &self,
        p: &Principal,
        ctx: &WebauthnCtx,
    ) -> Result<serde_json::Value, MfaError>;
    /// Verify that assertion; for the next 60 seconds one re-authentication of that person
    /// accepts it in place of a code.
    fn proof(
        &self,
        p: &Principal,
        ctx: &WebauthnCtx,
        credential: &serde_json::Value,
    ) -> Result<(), MfaError>;
    /// Request options for signing in: for the second step when `mfa_token` is given, else
    /// usernameless (discoverable credentials).
    fn passkey_login_options(
        &self,
        ctx: &WebauthnCtx,
        mfa_token: Option<&str>,
    ) -> Result<serde_json::Value, MfaError>;
    /// Verify the assertion: the second step with `mfa_token`, else passwordless sign-in.
    fn passkey_login(
        &self,
        ctx: &WebauthnCtx,
        mfa_token: Option<&str>,
        credential: &serde_json::Value,
    ) -> Result<LoginOutcome, MfaError>;
    /// The second-step methods a pending sign-in offers: `totp` and `passkey`.
    fn methods(&self, mfa_token: &str) -> Vec<&'static str>;
}

pub mod accounts;
pub mod aggregate;
pub mod blobs;
pub mod collate;
pub mod csv;
mod engine;
pub mod errors;
mod guards;
pub mod json;
mod kinds;
pub mod pay;
pub mod public;
pub mod query;
pub mod store;

pub use accounts::{AccountsApi, NewUser};
pub use blobs::{BlobError, BlobService, Served, clean_filename, disposition};
pub use engine::{Actor, ClockFn};
pub use pay::{FakeView, HookReply, PaymentHooks, PaymentPort, PaymentView, StartPayment, Started};
pub use public::{
    Adjust, Lookup, PublicApi, PublicFilter, PublicRead, PublicReceipt, PublicRecord, PublicSpec,
    PublicSubmit, Receipt, Submission, SubmitError,
};
pub use store::Store;

pub mod names {
    /// `POST /api/party/get` -> `party.get`
    pub fn from_rest(method: &str, path: &str) -> Option<String> {
        let rest = path.strip_prefix("/api/")?;
        let (model, cmd) = rest.split_once('/')?;
        (method == "POST" && !cmd.contains('/')).then(|| format!("{model}.{cmd}"))
    }
    /// MCP tool `party__get` -> `party.get` (double underscore: ids may contain single ones)
    pub fn from_mcp(tool: &str) -> Option<String> {
        tool.split_once("__").map(|(m, c)| format!("{m}.{c}"))
    }
    /// CLI `party get` -> `party.get`
    pub fn from_cli(words: &[&str]) -> Option<String> {
        match words {
            [m, c] => Some(format!("{m}.{c}")),
            _ => None,
        }
    }
    pub fn to_mcp(id: &str) -> String {
        id.replacen('.', "__", 1)
    }
}

#[cfg(test)]
mod engine_tests;
#[cfg(test)]
mod kinds_tests;
#[cfg(test)]
mod tests;
