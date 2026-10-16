//! Layer 3 registry: model, field and command metadata, and the D41 install gate.

use obop_types::DataClass;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

/// Registry metadata uses `&'static str`. Modules installed at runtime build their names from
/// manifests, so those strings are interned: each distinct string is leaked once, which bounds
/// the cost at the number of distinct names ever installed.
pub fn intern(s: &str) -> &'static str {
    static POOL: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut pool = POOL.get_or_init(Default::default).lock().unwrap();
    if let Some(found) = pool.get(s) {
        return found;
    }
    let leaked: &'static str = Box::leak(s.to_owned().into_boxed_str());
    pool.insert(leaked);
    leaked
}

#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: &'static str,
    pub description: &'static str,
    pub class: Option<DataClass>,
    pub permission_group: &'static str,
    pub personal_data: bool,
}

#[derive(Debug, Clone)]
pub struct CommandDef {
    pub name: &'static str,
    pub description: &'static str,
    pub permission_group: &'static str,
    /// Agents may only propose this command; a human approves it (D18, D42). Posting, paying
    /// and destructive commands set this.
    pub needs_approval: bool,
    /// Exposed through generated REST, MCP and CLI (D41: all or none, so this is not optional).
    pub surfaces: Surfaces,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Surfaces {
    pub rest: bool,
    pub mcp: bool,
    pub cli: bool,
}

impl CommandDef {
    pub fn new(
        name: &'static str,
        description: &'static str,
        permission_group: &'static str,
    ) -> Self {
        Self {
            name,
            description,
            permission_group,
            needs_approval: false,
            surfaces: Surfaces::ALL,
        }
    }

    pub fn approval(mut self) -> Self {
        self.needs_approval = true;
        self
    }
}

impl FieldDef {
    pub fn new(
        name: &'static str,
        description: &'static str,
        class: DataClass,
        personal_data: bool,
        permission_group: &'static str,
    ) -> Self {
        Self {
            name,
            description,
            class: Some(class),
            permission_group,
            personal_data,
        }
    }
}

impl Surfaces {
    pub const ALL: Self = Self {
        rest: true,
        mcp: true,
        cli: true,
    };

    fn is_all_or_none(self) -> bool {
        (self.rest && self.mcp && self.cli) || !(self.rest || self.mcp || self.cli)
    }
}

#[derive(Debug, Clone)]
pub struct ModelDef {
    pub name: &'static str,
    pub description: &'static str,
    /// Group that may call the generated `get` and `list` commands.
    pub read_group: &'static str,
    /// Group that may call the generated `create` and `update` commands.
    pub write_group: &'static str,
    pub fields: Vec<FieldDef>,
    /// Custom commands beyond the generated CRUD set.
    pub commands: Vec<CommandDef>,
}

impl ModelDef {
    /// Generated `get`, `list`, `create` and `update`, followed by the custom commands. The
    /// REST, MCP and CLI surfaces are derived from this list (D29, D41).
    pub fn all_commands(&self) -> Vec<CommandDef> {
        let crud = CommandDef::new;
        let mut v = vec![
            crud("get", "Read one record by id", self.read_group),
            crud("list", "List records", self.read_group),
            crud("create", "Create a record", self.write_group),
            crud("update", "Update fields of a record", self.write_group),
        ];
        v.extend(self.commands.iter().cloned());
        v
    }
}

#[derive(Debug, Clone, Default)]
pub struct Module {
    pub id: String,
    /// Name shown in the app launcher.
    pub name: String,
    pub models: Vec<ModelDef>,
    /// Network destinations the module declares in `obop.toml` (D41).
    pub network_destinations: Vec<String>,
}

impl Module {
    /// Every permission group the module refers to, sorted and de-duplicated.
    pub fn permission_groups(&self) -> Vec<&'static str> {
        let mut g: Vec<&'static str> = self
            .models
            .iter()
            .flat_map(|m| {
                [m.read_group, m.write_group]
                    .into_iter()
                    .chain(m.fields.iter().map(|f| f.permission_group))
                    .chain(m.commands.iter().map(|c| c.permission_group))
            })
            .collect();
        g.sort_unstable();
        g.dedup();
        g
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub path: String,
    pub problem: String,
}

/// The D41 install gate: every model, field and command carries a description, a permission
/// group and a data class; personal data cannot be classed `none`; surfaces are all or none.
pub fn check_module(m: &Module) -> Vec<Violation> {
    let mut v = Vec::new();
    let mut bad = |path: String, problem: &str| {
        v.push(Violation {
            path,
            problem: problem.into(),
        })
    };
    for model in &m.models {
        let mp = format!("{}.{}", m.id, model.name);
        if model.description.trim().is_empty() {
            bad(mp.clone(), "model has no description");
        }
        if model.read_group.trim().is_empty() || model.write_group.trim().is_empty() {
            bad(mp.clone(), "model has no read or write permission group");
        }
        for f in &model.fields {
            let fp = format!("{mp}.{}", f.name);
            if f.description.trim().is_empty() {
                bad(fp.clone(), "field has no description");
            }
            if f.permission_group.trim().is_empty() {
                bad(fp.clone(), "field has no permission group");
            }
            match f.class {
                None => bad(
                    fp.clone(),
                    "field has no data class (use `none` explicitly if not personal)",
                ),
                Some(DataClass::None) if f.personal_data => {
                    bad(fp.clone(), "personal data cannot be classed `none`")
                }
                _ => {}
            }
        }
        for c in &model.all_commands() {
            let cp = format!("{mp}::{}", c.name);
            if c.description.trim().is_empty() {
                bad(cp.clone(), "command has no description");
            }
            if c.permission_group.trim().is_empty() {
                bad(cp.clone(), "command has no permission group");
            }
            if !c.surfaces.is_all_or_none() {
                bad(cp, "command must be on REST, MCP and CLI or on none (D41)");
            }
        }
    }
    v
}

#[cfg(test)]
mod intern_tests {
    #[test]
    fn same_string_same_pointer() {
        assert!(std::ptr::eq(
            super::intern("abc"),
            super::intern(&String::from("abc"))
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(class: Option<DataClass>, personal: bool) -> FieldDef {
        FieldDef {
            name: "email",
            description: "Email",
            class,
            permission_group: "base.user",
            personal_data: personal,
        }
    }

    fn module(f: FieldDef) -> Module {
        Module {
            id: "t".into(),
            name: "T".into(),
            models: vec![ModelDef {
                name: "party",
                description: "Party",
                read_group: "g",
                write_group: "g",
                fields: vec![f],
                commands: vec![],
            }],
            network_destinations: vec![],
        }
    }

    #[test]
    fn complete_module_passes() {
        assert!(check_module(&module(field(Some(DataClass::Internal), true))).is_empty());
    }

    #[test]
    fn missing_class_fails() {
        let v = check_module(&module(field(None, false)));
        assert_eq!(v.len(), 1);
        assert!(v[0].problem.contains("no data class"));
    }

    #[test]
    fn personal_data_cannot_be_none() {
        assert_eq!(
            check_module(&module(field(Some(DataClass::None), true))).len(),
            1
        );
    }

    #[test]
    fn partial_surfaces_fail() {
        let mut m = module(field(Some(DataClass::None), false));
        m.models[0].commands.push(CommandDef {
            name: "merge",
            description: "Merge",
            permission_group: "base.admin",
            needs_approval: false,
            surfaces: Surfaces {
                rest: true,
                mcp: false,
                cli: true,
            },
        });
        assert_eq!(check_module(&m).len(), 1);
    }
}
