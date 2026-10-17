//! `obop.toml` for declarative packages (code tier X1, D11): models, fields with data classes,
//! permission groups and state-transition actions. A package contains no code, so installing
//! one cannot run anything the registry does not already control.

use obop_commands::{PublicRead, PublicReceipt, PublicSpec, PublicSubmit};
use obop_registry::labels::{LangLabels, ModelShape, Shape, Translations, check_labels};
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
    /// Translations (D52): `[labels.<lang>.module]`, `.model.<m>`, `.model.<m>.field.<f>`
    /// (with `.options`) and `.model.<m>.action.<a>`. Optional; English is the manifest itself.
    #[serde(default)]
    pub labels: std::collections::BTreeMap<String, LangLabels>,
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
    /// What anonymous visitors may read and submit (D50). Absent: nothing is public.
    pub public: Option<PublicM>,
}

/// `[model.public]`: declarative public exposure (D50).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicM {
    /// Anonymous visitors may read published records. Needs `published_field` and `fields`.
    #[serde(default)]
    pub read: bool,
    /// A `bool` field (public when true) or a `select` field (public when it equals
    /// `published_value`).
    pub published_field: Option<String>,
    pub published_value: Option<String>,
    /// A select option meaning "was public, retired for good": answered with 410.
    pub gone_value: Option<String>,
    /// The only fields a public read returns.
    #[serde(default)]
    pub fields: Vec<String>,
    pub order_by: Option<String>,
    /// `asc` (default) or `desc`.
    pub order_dir: Option<String>,
    /// Anonymous visitors may create records.
    #[serde(default)]
    pub submit: bool,
    #[serde(default)]
    pub submit_fields: Vec<String>,
    pub rate_limit_per_hour: Option<u32>,
    /// A `secret` text field holding an unguessable token: whoever holds the token may read the
    /// record back through `receipt_fields` (an order confirmation).
    pub receipt_token_field: Option<String>,
    /// A ref to the model that has a token: this record is shown with its parent (order lines).
    pub receipt_parent: Option<String>,
    #[serde(default)]
    pub receipt_fields: Vec<String>,
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

    /// The manifest `text` joined with a sibling labels document (`<id>.labels.toml`, only
    /// `[labels.<lang>...]` tables). Both are plain TOML, so the combined document is their
    /// concatenation; that text is what is stored, installed and served.
    pub fn combine(text: &str, labels: &str) -> String {
        if labels.trim().is_empty() {
            return text.to_string();
        }
        format!("{}\n\n{labels}", text.trim_end())
    }

    /// The structure labels may refer to.
    pub fn shape(&self) -> Shape {
        Shape {
            models: self
                .model
                .iter()
                .map(|m| {
                    (
                        m.name.clone(),
                        ModelShape {
                            fields: m
                                .field
                                .iter()
                                .map(|f| {
                                    (
                                        f.name.clone(),
                                        (f.kind == "select")
                                            .then(|| f.options.iter().cloned().collect()),
                                    )
                                })
                                .collect(),
                            actions: m.action.iter().map(|a| a.name.clone()).collect(),
                        },
                    )
                })
                .collect(),
        }
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
        self.public_specs()?;
        check_labels(&self.labels, &self.shape())?;
        Ok(())
    }

    /// The languages the package ships labels for.
    pub fn label_languages(&self) -> Vec<String> {
        self.labels.keys().cloned().collect()
    }

    /// The public declarations of the package (D50), checked for shape. The field-level gate
    /// (data classes, personal data) runs in `to_module` and again at registration.
    pub fn public_specs(&self) -> Result<Vec<PublicSpec>, String> {
        let mut out = Vec::new();
        for m in &self.model {
            let Some(p) = &m.public else { continue };
            let at = format!("{}.public", m.name);
            for n in p
                .fields
                .iter()
                .chain(&p.submit_fields)
                .chain(p.published_field.iter())
                .chain(p.order_by.iter())
                .chain(&p.receipt_fields)
                .chain(p.receipt_token_field.iter())
                .chain(p.receipt_parent.iter())
            {
                if !m.field.iter().any(|f| &f.name == n) {
                    return Err(format!("{at}: no field {n:?}"));
                }
            }
            if !p.read
                && (p.published_field.is_some()
                    || p.published_value.is_some()
                    || p.gone_value.is_some()
                    || !p.fields.is_empty()
                    || p.order_by.is_some()
                    || p.order_dir.is_some())
            {
                return Err(format!(
                    "{at}: published_field, fields, order_by and friends need read = true"
                ));
            }
            if !p.submit && (!p.submit_fields.is_empty() || p.rate_limit_per_hour.is_some()) {
                return Err(format!(
                    "{at}: submit_fields and rate_limit_per_hour need submit = true"
                ));
            }
            if p.submit && p.rate_limit_per_hour.is_none() {
                return Err(format!("{at}: submit = true needs rate_limit_per_hour"));
            }
            if p.read && p.published_field.is_none() {
                return Err(format!(
                    "{at}: read = true needs published_field, so only published records are public"
                ));
            }
            let has_receipt = p.receipt_token_field.is_some() || p.receipt_parent.is_some();
            if !has_receipt && !p.receipt_fields.is_empty() {
                return Err(format!(
                    "{at}: receipt_fields need receipt_token_field or receipt_parent"
                ));
            }
            if !p.read && !p.submit && !has_receipt {
                return Err(format!("{at}: declares nothing public"));
            }
            let desc = match p.order_dir.as_deref() {
                None | Some("asc") => false,
                Some("desc") => true,
                Some(o) => return Err(format!("{at}: order_dir must be asc or desc, not {o:?}")),
            };
            out.push(PublicSpec {
                model: m.name.clone(),
                read: p.read.then(|| PublicRead {
                    published_field: p.published_field.clone().unwrap_or_default(),
                    published_value: p.published_value.clone(),
                    gone_value: p.gone_value.clone(),
                    fields: p.fields.clone(),
                    order_by: p.order_by.clone(),
                    order_desc: desc,
                }),
                submit: p.submit.then(|| PublicSubmit {
                    fields: p.submit_fields.clone(),
                    rate_limit_per_hour: p.rate_limit_per_hour.unwrap_or(1),
                }),
                receipt: has_receipt.then(|| PublicReceipt {
                    token_field: p.receipt_token_field.clone(),
                    parent_field: p.receipt_parent.clone(),
                    fields: p.receipt_fields.clone(),
                }),
            });
        }
        Ok(out)
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
            description: self.module.description.clone(),
            models,
            network_destinations: vec![],
            translations: std::sync::Arc::new(Translations::from_labels(&self.labels)),
        };
        let mut public_problems = Vec::new();
        for spec in self.public_specs()? {
            if let Some(def) = module.models.iter().find(|d| d.name == spec.model)
                && let Err(e) = spec.check(def)
            {
                public_problems.push(e);
            }
        }
        if !public_problems.is_empty() {
            return Err(public_problems.join("; "));
        }
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

    const LABELS: &str = r#"
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
label = "Status"

[labels.de.model.fleet_vehicle.field.state.options]
available = "Verfügbar"
in_service = "Im Einsatz"

[labels.de.model.fleet_vehicle.action.dispatch]
label = "In Betrieb nehmen"
description = "Ein verfügbares Fahrzeug in Betrieb nehmen"

[labels.fi.module]
name = "Autokanta"

[labels.pt-BR.model.fleet_vehicle]
name = "Veículo"
"#;

    fn with_labels(extra: &str) -> String {
        format!("{FLEET}\n{LABELS}\n{extra}")
    }

    #[test]
    fn labels_reach_the_registry_per_language() {
        let m = Manifest::parse(&with_labels("")).unwrap();
        assert_eq!(m.label_languages(), ["de", "fi", "pt-BR"]);
        let module = m.to_module().unwrap();
        let t = &module.translations;
        assert_eq!(t.module.labels["de"], "Fuhrpark");
        assert_eq!(t.module.labels["fi"], "Autokanta");
        assert_eq!(t.module.descriptions["de"], "Fahrzeuge und Fahrer");
        assert_eq!(module.description, "Vehicles and drivers");
        let v = &t.models["fleet_vehicle"];
        assert_eq!(v.entry.labels["de"], "Fahrzeug");
        assert_eq!(v.entry.labels["pt-BR"], "Veículo");
        assert_eq!(v.fields["plate"].entry.labels["de"], "Kennzeichen");
        assert_eq!(
            v.fields["plate"].entry.descriptions["de"],
            "Amtliches Kennzeichen"
        );
        assert_eq!(v.fields["state"].options["in_service"]["de"], "Im Einsatz");
        assert_eq!(v.actions["dispatch"].labels["de"], "In Betrieb nehmen");
        // A package without labels has none.
        assert!(
            Manifest::parse(FLEET)
                .unwrap()
                .to_module()
                .unwrap()
                .translations
                .is_empty()
        );
    }

    #[test]
    fn combine_joins_a_sibling_labels_document() {
        let text = Manifest::combine(FLEET, LABELS);
        let m = Manifest::parse(&text).unwrap();
        assert_eq!(m.label_languages(), ["de", "fi", "pt-BR"]);
        assert_eq!(Manifest::combine(FLEET, "  \n"), FLEET);
    }

    #[test]
    fn bad_labels_are_refused_with_the_path_of_the_mistake() {
        let bad = |extra: &str| err(&format!("{FLEET}\n{extra}"));
        // language code
        assert!(bad("[labels.DE.module]\nname = \"x\"").contains("not a language code"));
        assert!(bad("[labels.deu1.module]\nname = \"x\"").contains("not a language code"));
        assert!(bad("[labels.de_DE.module]\nname = \"x\"").contains("not a language code"));
        // keys must exist (a typo is an error)
        assert!(
            bad("[labels.de.model.fleet_vehikel]\nname = \"x\"")
                .contains("no model \"fleet_vehikel\"")
        );
        assert!(
            bad("[labels.de.model.fleet_vehicle.field.platte]\nlabel = \"x\"")
                .contains("no field \"platte\"")
        );
        assert!(
            bad("[labels.de.model.fleet_vehicle.action.dispach]\nlabel = \"x\"")
                .contains("no action \"dispach\"")
        );
        assert!(
            bad("[labels.de.model.fleet_vehicle.field.state.options]\nretired = \"x\"")
                .contains("no option \"retired\"")
        );
        assert!(
            bad("[labels.de.model.fleet_vehicle.field.plate.options]\na = \"x\"")
                .contains("not a select field")
        );
        // unknown table or key names are refused by the schema, not ignored
        assert!(
            bad("[labels.de.models.fleet_vehicle]\nname = \"x\"").contains("invalid obop.toml")
        );
        assert!(
            bad("[labels.de.model.fleet_vehicle.field.plate]\ntitle = \"x\"")
                .contains("invalid obop.toml")
        );
        // values
        let v = |val: &str| bad(&format!("[labels.de.module]\nname = {val}"));
        assert!(v("\"\"").contains("is empty"));
        assert!(v("\"   \"").contains("is empty"));
        assert!(v("\" padded \"").contains("whitespace"));
        assert!(v("\"two\\nlines\"").contains("control character"));
        assert!(v("\"bell\\u0007\"").contains("control character"));
        assert!(v("\"Hallo {name}\"").contains("placeholder"));
        assert!(v("\"<b>fett</b>\"").contains("placeholder or markup"));
        assert!(v(&format!("\"{}\"", "x".repeat(81))).contains("too long"));
        assert!(
            bad(&format!(
                "[labels.de.module]\ndescription = \"{}\"",
                "x".repeat(401)
            ))
            .contains("too long")
        );
        // a value must be a string
        assert!(v("3").contains("invalid obop.toml"));
    }

    #[test]
    fn labels_count_against_the_size_limit() {
        let big = format!(
            "{FLEET}\n[labels.de.module]\nname = \"x\"\n# {}",
            "y".repeat(MAX_BYTES)
        );
        assert!(err(&big).contains("larger"));
    }
}
