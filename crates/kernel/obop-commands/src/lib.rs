//! One dispatch path for every surface (D29, D31, D41): REST, MCP and CLI calls all end in
//! `Dispatcher::call`, which checks the surface, the permission (D18: user groups intersected
//! with token scopes for agents), restores masked input tokens (D40), runs the handler and
//! shapes the output through the masking policy.

use obop_masking::mode_for;
pub use obop_masking::{PrincipalKind, RestoreError, Vault};
use obop_registry::{FieldDef, Module};
use obop_types::MaskMode;
use std::collections::{BTreeMap, HashMap, HashSet};

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

#[derive(Debug, Clone)]
pub struct Principal {
    pub id: String,
    pub kind: PrincipalKind,
    /// Permission groups of the principal (for an agent: of the user it acts for).
    pub groups: HashSet<String>,
    /// Token scopes. `None` means unrestricted; agents must carry `Some`.
    pub scopes: Option<HashSet<String>>,
}

impl Principal {
    pub fn user(id: &str, groups: &[&str]) -> Self {
        Self {
            id: id.into(),
            kind: PrincipalKind::User,
            groups: set(groups),
            scopes: None,
        }
    }

    pub fn agent(id: &str, on_behalf_groups: &[&str], scopes: &[&str]) -> Self {
        Self {
            id: id.into(),
            kind: PrincipalKind::Agent,
            groups: set(on_behalf_groups),
            scopes: Some(set(scopes)),
        }
    }

    /// Effective permission: the group is held and, if scoped, the scope allows it.
    pub fn allows(&self, group: &str) -> bool {
        self.groups.contains(group) && self.scopes.as_ref().is_none_or(|s| s.contains(group))
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
    description: &'static str,
    needs_approval: bool,
    model: &'static str,
    permission_group: &'static str,
    surfaces: obop_registry::Surfaces,
    handler: Handler,
}

#[derive(Default)]
pub struct Dispatcher {
    pending: std::sync::Mutex<HashMap<String, Pending>>,
    commands: HashMap<String, Registered>,
    fields: HashMap<&'static str, Vec<FieldDef>>,
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
                let c = &self.commands[&id];
                CommandInfo {
                    id,
                    description: c.description,
                    permission_group: c.permission_group,
                    needs_approval: c.needs_approval,
                }
            })
            .collect()
    }

    /// Bind a handler to a command declared in `module`'s registry metadata. A handler for an
    /// undeclared command is a programming error: the surface is generated from the registry.
    pub fn register(&mut self, module: &Module, model: &str, command: &str, handler: Handler) {
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
        self.fields.insert(m.name, m.fields.clone());
        self.commands.insert(
            format!("{model}.{command}"),
            Registered {
                description: c.description,
                needs_approval: c.needs_approval,
                model: m.name,
                permission_group: c.permission_group,
                surfaces: c.surfaces,
                handler,
            },
        );
    }

    /// Command ids the principal may call on `surface`: what MCP `tools/list` and CLI help show.
    pub fn visible(&self, surface: Surface, p: &Principal) -> Vec<String> {
        let mut v: Vec<_> = self
            .commands
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
            .get(id)
            .ok_or_else(|| CallError::UnknownCommand(id.into()))?;
        if !on_surface(c.surfaces, surface) {
            return Err(CallError::NotOnSurface(id.into()));
        }
        // Same answer for "no such group" and "scope too narrow": do not leak which.
        if !p.allows(c.permission_group) {
            return Err(CallError::Forbidden(id.into()));
        }
        let input = self.restore(p, vault, input)?;
        if c.needs_approval && p.kind == PrincipalKind::Agent {
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
        self.run(c, p, vault, input)
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
        let mut pending = self.pending.lock().unwrap();
        let plan = pending
            .get(plan_id)
            .ok_or_else(|| CallError::UnknownPlan(plan_id.into()))?;
        let c = &self.commands[&plan.id];
        if !approver.allows(c.permission_group) || !plan.principal.allows(c.permission_group) {
            return Err(CallError::Forbidden(plan.id.clone()));
        }
        let plan = pending.remove(plan_id).expect("checked above");
        drop(pending);
        self.run(&self.commands[&plan.id], &plan.principal, vault, plan.input)
    }

    fn run(
        &self,
        c: &Registered,
        p: &Principal,
        vault: &mut Vault,
        input: Value,
    ) -> Result<Response, CallError> {
        let out = (c.handler)(&input).map_err(CallError::Handler)?;
        let mut denied = Vec::new();
        let value = self.shape(c.model, p, vault, out, &mut denied);
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
        model: &'static str,
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
                for (k, v) in m {
                    let Some(def) = self
                        .fields
                        .get(model)
                        .and_then(|f| f.iter().find(|f| f.name == k))
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
pub mod json;
pub mod store;

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
