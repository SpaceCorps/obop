//! One dispatch path for every surface (D29, D31, D41): REST, MCP and CLI calls all end in
//! `Dispatcher::call`, which checks the surface, the permission (D18: user groups intersected
//! with token scopes for agents), restores masked input tokens (D40), runs the handler and
//! shapes the output through the masking policy.

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

struct Pending {
    principal: Principal,
    id: String,
    input: Value,
}

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
    handler: Handler,
}

/// Commands can be registered and removed while the server runs (module install and uninstall),
/// so the tables sit behind locks and handlers run on `Arc` clones taken out of the lock.
#[derive(Default)]
pub struct Dispatcher {
    pending: std::sync::Mutex<HashMap<String, Pending>>,
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
}

/// What an observer sees of a command that succeeded: its id, who ran it, the input the handler
/// received and the raw output, before it was shaped for the caller.
pub type Observer = dyn Fn(&str, &Principal, &Value, &Value) + Send + Sync;

pub(crate) struct ModelMeta {
    pub(crate) def: obop_registry::ModelDef,
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
    /// Lowercase field kind (D47): text, longtext, number, money, date, bool, select, ref, image or file.
    pub kind: &'static str,
    pub required: bool,
    /// Options of a `select`; empty for other kinds.
    pub options: Vec<&'static str>,
    /// Target model of a `ref`.
    pub to: Option<&'static str>,
    /// Largest upload in bytes of an `image` or `file` field.
    pub max_bytes: Option<usize>,
    /// Label and description per language (D52); empty maps when untranslated.
    pub labels: Texts,
    pub descriptions: Texts,
    /// Select option -> language -> label.
    pub option_labels: BTreeMap<String, Texts>,
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
    pub can_update: bool,
    pub can_delete: bool,
    pub labels: Texts,
    pub descriptions: Texts,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleInfo {
    pub id: String,
    pub name: String,
    /// English description; empty when the module declares none.
    pub description: String,
    pub labels: Texts,
    pub descriptions: Texts,
    pub models: Vec<ModelInfo>,
}

impl Dispatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// What `tools/list`, `GET /api/commands` and CLI help show for this principal.
    pub fn catalog(&self, surface: Surface, p: &Principal) -> Vec<CommandInfo> {
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
                                labels: ftr.map(|t| t.entry.labels.clone()).unwrap_or_default(),
                                descriptions: ftr
                                    .map(|t| t.entry.descriptions.clone())
                                    .unwrap_or_default(),
                                option_labels: ftr.map(|t| t.options.clone()).unwrap_or_default(),
                            }
                        })
                        .collect(),
                    actions,
                    can_create: can("create"),
                    can_update: can("update"),
                    can_delete: can("delete"),
                });
            }
            if !infos.is_empty() {
                let first = models.values().find(|m| &m.module == module);
                let name = first
                    .map(|m| m.module_name.clone())
                    .unwrap_or_else(|| module.clone());
                let (description, labels, descriptions) =
                    first.map_or_else(Default::default, |m| {
                        (
                            m.module_description.clone(),
                            m.translations.module.labels.clone(),
                            m.translations.module.descriptions.clone(),
                        )
                    });
                out.push(ModuleInfo {
                    id: module.clone(),
                    name,
                    description,
                    labels,
                    descriptions,
                    models: infos,
                });
            }
        }
        out
    }

    /// Bind a handler to a command declared in `module`'s registry metadata. A handler for an
    /// undeclared command is a programming error: the surface is generated from the registry.
    pub fn register(&self, module: &Module, model: &str, command: &str, handler: Handler) {
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
                def: m.clone(),
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
                envelope: command == "search",
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
            .map(|m| m.def.clone())
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
        let mut f = self.models.write().unwrap();
        for m in models {
            f.remove(&m);
        }
        self.order.write().unwrap().retain(|x| x != module);
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
            m.remove("_caller");
        }
        let guard = self.guards.read().unwrap().get(id).cloned();
        let _txn = guard
            .as_ref()
            .filter(|g| g.serialize)
            .map(|_| self.txn.lock().unwrap_or_else(|e| e.into_inner()));
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
            self.pending.lock().unwrap().insert(
                plan_id.clone(),
                Pending {
                    principal: p.clone(),
                    id: id.into(),
                    input,
                },
            );
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
        if approver.kind != PrincipalKind::User {
            return Err(CallError::Forbidden("approve".into()));
        }
        let _txn = self.txn.lock().unwrap_or_else(|e| e.into_inner());
        let mut pending = self.pending.lock().unwrap();
        let plan = pending
            .get(plan_id)
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
        let plan = pending.remove(plan_id).expect("checked above");
        drop(pending);
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
        }
        let out = (c.handler)(&input).map_err(CallError::Handler)?;
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
    /// A human issues a scoped token for an agent acting on their behalf (D18). Scopes must be
    /// groups the human holds.
    fn issue_agent_token(
        &self,
        owner: &Principal,
        scopes: &[String],
        label: &str,
    ) -> Result<String, String>;
}

/// What the first (or second) sign-in step produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginOutcome {
    Session {
        token: String,
        /// The session is limited to two-factor enrolment because the instance requires it.
        mfa_enrolment_required: bool,
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
    /// Needs the password and a current code or a recovery code.
    fn disable(
        &self,
        p: &Principal,
        password: &str,
        code: Option<&str>,
        recovery_code: Option<&str>,
    ) -> Result<(), MfaError>;
    fn regenerate_recovery(
        &self,
        p: &Principal,
        password: &str,
        code: Option<&str>,
        recovery_code: Option<&str>,
    ) -> Result<serde_json::Value, MfaError>;
    /// `platform.admin` removes another user's second factor; never one's own.
    fn admin_reset(&self, admin: &Principal, target: &str) -> Result<(), MfaError>;
}

pub mod blobs;
mod guards;
pub mod json;
pub mod pay;
pub mod public;
pub mod query;
pub mod store;

pub use blobs::{BlobError, BlobService, Served, clean_filename, disposition};
pub use pay::{FakeView, PaymentPort, PaymentView, StartPayment, Started};
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
mod tests;
