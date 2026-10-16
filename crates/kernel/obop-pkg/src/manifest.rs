//! `obop.toml` for declarative packages (code tier X1, D11): models, fields with data classes,
//! permission groups and state-transition actions. A package contains no code, so installing
//! one cannot run anything the registry does not already control.

use obop_registry::{
    CommandDef, FieldDef, FieldKind, GENERATED_COMMANDS, ModelDef, Module, check_module, intern,
};
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
    /// Field kind (D47): text, longtext, number, money, date, bool, select or ref.
    #[serde(default = "default_kind")]
    pub kind: String,
    /// Allowed values; only for `select`. For a `state` field they are the status order.
    #[serde(default)]
    pub options: Vec<String>,
    /// Target model; only for `ref`.
    pub to: Option<String>,
    /// Must be present and non-null on create. Defaults to false.
    #[serde(default)]
    pub required: bool,
}

fn default_kind() -> String {
    "text".into()
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
                self.check_kind(m, f)?;
            }
            let mut actions = std::collections::HashSet::new();
            for a in &m.action {
                if !ident_ok(&a.name) || GENERATED_COMMANDS.contains(&a.name.as_str()) {
                    return Err(format!("{}: bad action name {:?}", m.name, a.name));
                }
                if !actions.insert(&a.name) {
                    return Err(format!("{}: action {:?} is declared twice", m.name, a.name));
                }
                let Some(f) = m.field.iter().find(|f| f.name == a.field) else {
                    return Err(format!(
                        "{}.{}: acts on undeclared field {:?}",
                        m.name, a.name, a.field
                    ));
                };
                if f.kind != "select" {
                    return Err(format!(
                        "{}.{}: acts on {}, which is a {} field; an action moves a select field between its options",
                        m.name, a.name, f.name, f.kind
                    ));
                }
                for v in a.from.iter().chain(std::iter::once(&a.to)) {
                    if !f.options.contains(v) {
                        return Err(format!(
                            "{}.{}: {v:?} is not one of the options of {} ({})",
                            m.name,
                            a.name,
                            f.name,
                            f.options.join(", ")
                        ));
                    }
                }
                if a.from.as_ref() == Some(&a.to) {
                    return Err(format!(
                        "{}.{}: `from` and `to` are both {:?}; an action must change the state",
                        m.name, a.name, a.to
                    ));
                }
            }
        }
        Ok(())
    }

    /// Consistency of `kind`, `options`, `to` and `required` on one field.
    fn check_kind(&self, m: &ModelM, f: &FieldM) -> Result<(), String> {
        let at = format!("{}.{}", m.name, f.name);
        if !FieldKind::is_known(&f.kind) {
            return Err(format!(
                "{at}: unknown kind {:?} (use text, longtext, number, money, date, bool, select or ref)",
                f.kind
            ));
        }
        if f.kind != "select" && !f.options.is_empty() {
            return Err(format!("{at}: options are only valid for kind \"select\""));
        }
        if f.kind != "ref" && f.to.is_some() {
            return Err(format!("{at}: `to` is only valid for kind \"ref\""));
        }
        if f.kind == "select" {
            if f.options.is_empty() {
                return Err(format!("{at}: a select needs options"));
            }
            let mut seen = std::collections::HashSet::new();
            for o in &f.options {
                if o.trim().is_empty() || !seen.insert(o) {
                    return Err(format!(
                        "{at}: select options must be non-empty and unique (got {o:?})"
                    ));
                }
            }
        }
        if f.kind == "ref" {
            let Some(to) = f.to.as_deref() else {
                return Err(format!("{at}: a ref needs `to`, the target model"));
            };
            if !ident_ok(to) {
                return Err(format!("{at}: `to` {to:?} is not a model name"));
            }
            // A target outside this package is checked at install time, against what is
            // registered then; a target inside it must really be one of its models.
            let own = to == self.module.id || to.starts_with(&format!("{}_", self.module.id));
            if own && !self.model.iter().any(|x| x.name == to) {
                return Err(format!(
                    "{at}: `to` names {to:?}, which this package does not declare"
                ));
            }
        }
        Ok(())
    }

    /// Models that `ref` fields point at and that this package does not declare: they must be
    /// registered already when the package is installed.
    pub fn external_refs(&self) -> Vec<(String, String)> {
        let mut v = Vec::new();
        for m in &self.model {
            for f in &m.field {
                if let Some(to) = &f.to
                    && f.kind == "ref"
                    && !self.model.iter().any(|x| &x.name == to)
                {
                    v.push((format!("{}.{}", m.name, f.name), to.clone()));
                }
            }
        }
        v
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
                    .map(|f| {
                        let kind = match f.kind.as_str() {
                            "longtext" => FieldKind::LongText,
                            "number" => FieldKind::Number,
                            "money" => FieldKind::Money,
                            "date" => FieldKind::Date,
                            "bool" => FieldKind::Bool,
                            "select" => FieldKind::Select(
                                f.options
                                    .iter()
                                    .map(|o| intern(o))
                                    .collect::<Vec<_>>()
                                    .into(),
                            ),
                            "ref" => FieldKind::Ref(intern(f.to.as_deref().unwrap_or(""))),
                            _ => FieldKind::Text,
                        };
                        FieldDef {
                            name: intern(&f.name),
                            description: intern(&f.description),
                            class: DataClass::parse(&f.class),
                            permission_group: intern(f.group.as_deref().unwrap_or(&m.read_group)),
                            personal_data: f.personal_data,
                            kind,
                            required: f.required,
                        }
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
                        let c = c.transition(&a.field, a.from.as_deref(), &a.to);
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
  kind = "select"
  options = ["available", "in_service"]

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
        assert_eq!(module.models[0].all_commands().len(), 7);
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

    /// FLEET with a ref added.
    fn typed() -> String {
        FLEET
            .replace(
                "[[model.action]]",
                "[[model.field]]\n  name = \"parent_id\"\n  description = \"Parent vehicle\"\n  class = \"none\"\n  kind = \"ref\"\n  to = \"fleet_vehicle\"\n  required = true\n\n  [[model.action]]",
            )
    }

    #[test]
    fn kinds_options_to_and_required_reach_the_registry() {
        let module = Manifest::parse(&typed()).unwrap().to_module().unwrap();
        let f = &module.models[0].fields;
        assert_eq!(f[0].kind, FieldKind::Text, "kind defaults to text");
        assert!(!f[0].required);
        assert_eq!(f[2].kind.name(), "select");
        assert_eq!(f[2].kind.options(), ["available", "in_service"]);
        assert_eq!(f[3].kind.target(), Some("fleet_vehicle"));
        assert!(f[3].required);
    }

    #[test]
    fn every_kind_name_parses() {
        for k in ["text", "longtext", "number", "money", "date", "bool"] {
            let t = FLEET.replace(
                "name = \"plate\"",
                &format!("name = \"plate\"\n  kind = \"{k}\""),
            );
            let module = Manifest::parse(&t).unwrap().to_module().unwrap();
            assert_eq!(module.models[0].fields[0].kind.name(), k);
        }
    }

    #[test]
    fn rejects_inconsistent_field_kinds() {
        let at = |extra: &str| {
            FLEET.replace("name = \"plate\"", &format!("name = \"plate\"\n  {extra}"))
        };
        assert!(err(&at("kind = \"float\"")).contains("unknown kind"));
        assert!(err(&at("options = [\"a\"]")).contains("options are only valid"));
        assert!(
            err(&at("kind = \"money\"\n  options = [\"a\"]"))
                .contains("only valid for kind \"select\"")
        );
        assert!(err(&at("to = \"fleet_vehicle\"")).contains("`to` is only valid"));
        assert!(err(&at("kind = \"select\"")).contains("needs options"));
        assert!(err(&at("kind = \"select\"\n  options = []")).contains("needs options"));
        assert!(err(&at("kind = \"select\"\n  options = [\"a\", \"a\"]")).contains("unique"));
        assert!(err(&at("kind = \"ref\"")).contains("needs `to`"));
        assert!(err(&at("kind = \"ref\"\n  to = \"Bad Name\"")).contains("not a model name"));
        assert!(err(&at("kind = \"ref\"\n  to = \"fleet_ghost\"")).contains("does not declare"));
        assert!(err(&at("required = \"yes\"")).contains("invalid obop.toml"));
    }

    #[test]
    fn refs_to_other_packages_wait_for_install_time() {
        let t = FLEET.replace(
            "name = \"plate\"",
            "name = \"plate\"\n  kind = \"ref\"\n  to = \"party\"",
        );
        let m = Manifest::parse(&t).unwrap();
        assert_eq!(
            m.external_refs(),
            [("fleet_vehicle.plate".to_string(), "party".to_string())]
        );
        assert!(
            Manifest::parse(&typed())
                .unwrap()
                .external_refs()
                .is_empty()
        );
    }

    #[test]
    fn a_field_named_id_is_refused() {
        assert!(err(&FLEET.replace("name = \"plate\"", "name = \"id\"")).contains("field name"));
    }

    #[test]
    fn action_transitions_must_use_the_options_of_a_select() {
        let bad_to = typed().replace("to = \"in_service\"", "to = \"scrapped\"");
        assert!(err(&bad_to).contains("not one of the options"));
        let bad_from = typed().replace("from = \"available\"", "from = \"missing\"");
        assert!(err(&bad_from).contains("not one of the options"));
        assert!(Manifest::parse(&typed()).is_ok());
    }

    #[test]
    fn duplicate_action_names_are_refused() {
        let twice = format!(
            "{FLEET}\n  [[model.action]]\n  name = \"dispatch\"\n  description = \"again\"\n  field = \"state\"\n  from = \"in_service\"\n  to = \"available\"\n"
        );
        assert!(err(&twice).contains("action \"dispatch\" is declared twice"));
    }

    #[test]
    fn generated_command_names_cannot_be_actions() {
        for name in ["get", "list", "search", "create", "update", "delete"] {
            let bad = FLEET.replace("name = \"dispatch\"", &format!("name = \"{name}\""));
            assert!(err(&bad).contains("bad action name"), "{name}");
        }
    }

    #[test]
    fn an_action_acts_on_any_select_field_not_only_state() {
        let t = FLEET
            .replace(
                "name = \"plate\"",
                "name = \"priority\"\n  kind = \"select\"\n  options = [\"low\", \"high\"]",
            )
            .replace("field = \"state\"", "field = \"priority\"")
            .replace("from = \"available\"", "from = \"low\"")
            .replace("to = \"in_service\"", "to = \"high\"");
        let module = Manifest::parse(&t).unwrap().to_module().unwrap();
        let model = &module.models[0];
        assert_eq!(model.governed_fields(), ["priority"]);
        let tr = model.commands[0].transition.unwrap();
        assert_eq!(
            (tr.field, tr.from, tr.to),
            ("priority", Some("low"), "high")
        );
        // The governed field is not `state` any more, so `state` stays free.
        assert!(!model.governed_fields().contains(&"state"));
    }

    #[test]
    fn an_action_on_a_field_that_is_not_a_select_is_refused() {
        let bad = FLEET.replace(
            "kind = \"select\"\n  options = [\"available\", \"in_service\"]",
            "",
        );
        let e = err(&bad);
        assert!(
            e.contains("acts on state, which is a text field") && e.contains("select"),
            "{e}"
        );
    }

    #[test]
    fn an_action_must_change_the_state() {
        let bad = FLEET.replace("to = \"in_service\"", "to = \"available\"");
        assert!(err(&bad).contains("`from` and `to` are both"));
    }

    #[test]
    fn transitions_reach_the_registry() {
        let module = Manifest::parse(FLEET).unwrap().to_module().unwrap();
        let (cmd, t) = module.models[0].transitions().next().unwrap();
        assert_eq!(cmd.name, "dispatch");
        assert_eq!(
            (t.field, t.from, t.to),
            ("state", Some("available"), "in_service")
        );
    }
}
