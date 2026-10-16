//! `obop.toml` for declarative packages (code tier X1, D11): models, fields with data classes,
//! permission groups and state-transition actions. A package contains no code, so installing
//! one cannot run anything the registry does not already control.

use obop_registry::{CommandDef, FieldDef, ModelDef, Module, check_module, intern};
use obop_types::DataClass;
use serde::Deserialize;

pub const MAX_BYTES: usize = 256 * 1024;
const MAX_MODELS: usize = 50;
const MAX_FIELDS: usize = 200;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub module: Meta,
    #[serde(default)]
    pub model: Vec<ModelM>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    pub id: String,
    pub version: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub depends: Vec<String>,
    /// Network destinations the package may reach (D41). Declarative packages run no code, so
    /// this must stay empty; the field exists so the contract is the same for later tiers.
    #[serde(default)]
    pub network_destinations: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelM {
    pub name: String,
    pub description: String,
    pub read_group: String,
    pub write_group: String,
    #[serde(default)]
    pub field: Vec<FieldM>,
    #[serde(default)]
    pub action: Vec<ActionM>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldM {
    pub name: String,
    pub description: String,
    /// One of the data classes of CONVENTIONS section 5, or `none` for explicitly non-personal.
    pub class: String,
    #[serde(default)]
    pub personal_data: bool,
    /// Defaults to the model's read group.
    pub group: Option<String>,
}

/// A state transition: set `field` to `to`, only when it currently equals `from` (if given).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionM {
    pub name: String,
    pub description: String,
    pub field: String,
    pub from: Option<String>,
    pub to: String,
    /// Defaults to the model's write group.
    pub group: Option<String>,
    /// Agents may only propose it (D18). Defaults to false.
    #[serde(default)]
    pub approval: bool,
}

fn ident_ok(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|f| f.is_ascii_lowercase())
        && s.len() <= 40
        && c.all(|x| x.is_ascii_lowercase() || x.is_ascii_digit() || x == '_')
}

fn group_ok(g: &str) -> bool {
    g.split_once('.')
        .is_some_and(|(a, b)| ident_ok(a) && ident_ok(b))
}

fn semver_ok(v: &str) -> bool {
    let p: Vec<_> = v.split('.').collect();
    p.len() == 3
        && p.iter()
            .all(|x| !x.is_empty() && x.len() <= 6 && x.chars().all(|c| c.is_ascii_digit()))
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > MAX_BYTES {
            return Err(format!("manifest is larger than {} KiB", MAX_BYTES / 1024));
        }
        let m: Manifest = toml::from_str(text).map_err(|e| format!("invalid obop.toml: {e}"))?;
        m.validate()?;
        Ok(m)
    }

    /// Structural rules that need no knowledge of what is already installed.
    fn validate(&self) -> Result<(), String> {
        let id = &self.module.id;
        if !ident_ok(id) {
            return Err(format!(
                "module id {id:?} must be lowercase letters, digits and _, starting with a letter"
            ));
        }
        if !semver_ok(&self.module.version) {
            return Err("version must look like 1.2.3".into());
        }
        if self.module.name.trim().is_empty() || self.module.description.trim().is_empty() {
            return Err("module needs a name and a description".into());
        }
        if !self.module.network_destinations.is_empty() {
            return Err(
                "declarative packages run no code and may not declare network destinations".into(),
            );
        }
        if self.model.len() > MAX_MODELS
            || self.model.iter().map(|m| m.field.len()).sum::<usize>() > MAX_FIELDS
        {
            return Err("package is too large".into());
        }
        let mut seen = std::collections::HashSet::new();
        for m in &self.model {
            if !ident_ok(&m.name) || !(m.name == *id || m.name.starts_with(&format!("{id}_"))) {
                return Err(format!(
                    "model {:?} must be {id:?} or start with \"{id}_\"",
                    m.name
                ));
            }
            if !seen.insert(&m.name) {
                return Err(format!("model {} is declared twice", m.name));
            }
            for g in [&m.read_group, &m.write_group]
                .into_iter()
                .chain(m.field.iter().filter_map(|f| f.group.as_ref()))
                .chain(m.action.iter().filter_map(|a| a.group.as_ref()))
            {
                if !group_ok(g) {
                    return Err(format!("permission group {g:?} must look like module.role"));
                }
            }
            let mut names = std::collections::HashSet::new();
            for f in &m.field {
                if !ident_ok(&f.name) || f.name == "id" || !names.insert(&f.name) {
                    return Err(format!("{}.{}: bad or repeated field name", m.name, f.name));
                }
                if DataClass::parse(&f.class).is_none() {
                    return Err(format!(
                        "{}.{}: unknown data class {:?}",
                        m.name, f.name, f.class
                    ));
                }
            }
            for a in &m.action {
                if !ident_ok(&a.name)
                    || ["get", "list", "create", "update"].contains(&a.name.as_str())
                {
                    return Err(format!("{}: bad action name {:?}", m.name, a.name));
                }
                if !m.field.iter().any(|f| f.name == a.field) {
                    return Err(format!(
                        "{}.{}: acts on undeclared field {:?}",
                        m.name, a.name, a.field
                    ));
                }
            }
        }
        Ok(())
    }

    /// Permission groups the package refers to.
    pub fn groups(&self) -> Vec<String> {
        let mut g: Vec<String> = self
            .model
            .iter()
            .flat_map(|m| {
                [m.read_group.clone(), m.write_group.clone()]
                    .into_iter()
                    .chain(m.field.iter().filter_map(|f| f.group.clone()))
                    .chain(m.action.iter().filter_map(|a| a.group.clone()))
            })
            .collect();
        g.sort();
        g.dedup();
        g
    }

    /// Build registry metadata and run the D41 install gate over it.
    pub fn to_module(&self) -> Result<Module, String> {
        let models = self
            .model
            .iter()
            .map(|m| ModelDef {
                name: intern(&m.name),
                description: intern(&m.description),
                read_group: intern(&m.read_group),
                write_group: intern(&m.write_group),
                fields: m
                    .field
                    .iter()
                    .map(|f| FieldDef {
                        name: intern(&f.name),
                        description: intern(&f.description),
                        class: DataClass::parse(&f.class),
                        permission_group: intern(f.group.as_deref().unwrap_or(&m.read_group)),
                        personal_data: f.personal_data,
                    })
                    .collect(),
                commands: m
                    .action
                    .iter()
                    .map(|a| {
                        let c = CommandDef::new(
                            intern(&a.name),
                            intern(&a.description),
                            intern(a.group.as_deref().unwrap_or(&m.write_group)),
                        );
                        if a.approval { c.approval() } else { c }
                    })
                    .collect(),
            })
            .collect();
        let module = Module {
            id: self.module.id.clone(),
            name: self.module.name.clone(),
            models,
            network_destinations: vec![],
        };
        let violations = check_module(&module);
        if violations.is_empty() {
            Ok(module)
        } else {
            Err(violations
                .iter()
                .map(|v| format!("{}: {}", v.path, v.problem))
                .collect::<Vec<_>>()
                .join("; "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const FLEET: &str = r#"
[module]
id = "fleet"
version = "0.1.0"
name = "Fleet"
description = "Vehicles and drivers"

[[model]]
name = "fleet_vehicle"
description = "A vehicle"
read_group = "fleet.user"
write_group = "fleet.manager"

  [[model.field]]
  name = "plate"
  description = "Registration plate"
  class = "none"

  [[model.field]]
  name = "driver"
  description = "Name of the assigned driver"
  class = "internal"
  personal_data = true

  [[model.field]]
  name = "state"
  description = "available or in_service"
  class = "none"

  [[model.action]]
  name = "dispatch"
  description = "Put an available vehicle in service"
  field = "state"
  from = "available"
  to = "in_service"
"#;

    #[test]
    fn valid_manifest_becomes_a_module_that_passes_the_gate() {
        let m = Manifest::parse(FLEET).unwrap();
        let module = m.to_module().unwrap();
        assert_eq!(module.models[0].all_commands().len(), 5);
        assert_eq!(m.groups(), ["fleet.manager", "fleet.user"]);
    }

    fn err(text: &str) -> String {
        Manifest::parse(text)
            .and_then(|m| m.to_module().map(|_| ()))
            .unwrap_err()
    }

    #[test]
    fn rejects_bad_input() {
        assert!(
            err(&FLEET.replace("class = \"internal\"", "class = \"bogus\""))
                .contains("unknown data class")
        );
        assert!(
            err(&FLEET.replace("name = \"fleet_vehicle\"", "name = \"party\"")).contains("must be")
        );
        assert!(err(&FLEET.replace("id = \"fleet\"", "id = \"Fleet\"")).contains("module id"));
        assert!(err(&FLEET.replace("0.1.0", "1.0")).contains("version"));
        assert!(err(&FLEET.replace("fleet.user", "everyone")).contains("module.role"));
        assert!(
            err(&FLEET.replace("field = \"state\"", "field = \"ghost\""))
                .contains("undeclared field")
        );
        assert!(err(&format!("{FLEET}\nbogus = 1")).contains("invalid obop.toml"));
        assert!(
            err(&FLEET.replace(
                "version = \"0.1.0\"",
                "version = \"0.1.0\"\nnetwork_destinations = [\"evil.example\"]"
            ))
            .contains("network")
        );
        assert!(err(&"x".repeat(MAX_BYTES + 1)).contains("larger"));
    }

    #[test]
    fn personal_data_cannot_be_unclassified() {
        // `class = "none"` on a field marked personal_data trips the D41 gate.
        let bad = FLEET.replace(
            "class = \"internal\"\n  personal_data = true",
            "class = \"none\"\n  personal_data = true",
        );
        assert!(err(&bad).contains("personal data cannot be classed"));
    }
}
