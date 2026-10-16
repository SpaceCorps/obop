//! Layer 3 registry: model, field and command metadata, and the D41 install gate.

use obop_types::DataClass;

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
    /// Exposed through generated REST, MCP and CLI (D41: all or none, so this is not optional).
    pub surfaces: Surfaces,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Surfaces {
    pub rest: bool,
    pub mcp: bool,
    pub cli: bool,
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
    pub fields: Vec<FieldDef>,
    pub commands: Vec<CommandDef>,
}

#[derive(Debug, Clone, Default)]
pub struct Module {
    pub id: String,
    pub models: Vec<ModelDef>,
    /// Network destinations the module declares in `obop.toml` (D41).
    pub network_destinations: Vec<String>,
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
        for c in &model.commands {
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
            models: vec![ModelDef {
                name: "party",
                description: "Party",
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
            surfaces: Surfaces {
                rest: true,
                mcp: false,
                cli: true,
            },
        });
        assert_eq!(check_module(&m).len(), 1);
    }
}
