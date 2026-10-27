//! `obop.toml` for declarative packages (code tier X1, D11): models, fields with data classes,
//! permission groups and state-transition actions. A package contains no code, so installing
//! one cannot run anything the registry does not already control.

use crate::semver::{Req, Version};
use crate::spdx::{self, LicenseKind};
use obop_commands::{PublicRead, PublicReceipt, PublicSpec, PublicSubmit};
use obop_registry::labels::{LangLabels, ModelShape, Shape, Translations, check_labels};
use obop_registry::rules::{MAX_EFFECTS, SeqFormat};
use obop_registry::{
    Access, Assertion, CommandDef, Computed, Effect, EffectKind, FieldDef, FieldKind,
    GENERATED_COMMANDS, History, Lit, Locator, MATURITIES, MODULE_CATEGORIES, ModelDef, ModelExt,
    Module, ModuleMeta, Reset, SequenceSpec, Src, Target, Unique, check_installed_module,
    check_module, intern,
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
    /// `[[migration]]` (D63): declarative steps that move existing records to the shape of this
    /// version when an older version is upgraded.
    #[serde(default)]
    pub migration: Vec<MigrationM>,
    /// The text this manifest was parsed from (what is stored and installed).
    #[serde(skip)]
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    pub id: String,
    /// Semantic version of the module (`1.2.3`, `1.2.3-beta.1`).
    pub version: String,
    pub name: String,
    pub description: String,
    /// One line for the card in the module catalogue, at most 140 characters (D55).
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub authors: Vec<String>,
    /// SPDX expression, `LicenseRef-...`, `proprietary` or `UNLICENSED`. Required for bundled and
    /// marketplace packages.
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub repository: String,
    #[serde(default)]
    pub documentation: String,
    #[serde(default)]
    pub support: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    /// A built-in icon name; unknown names fall back to a generic icon in the interface.
    #[serde(default)]
    pub icon: String,
    /// Release notes of this version, shown before an update (D63). Plain text, at most 8000
    /// characters.
    #[serde(default)]
    pub changelog: String,
    /// The core versions the module runs on, a semver requirement such as `^0.2` or
    /// `>=0.2, <0.4`. Required (D63).
    #[serde(default)]
    pub requires_core: String,
    /// `stable`, `beta` or `experimental`.
    #[serde(default)]
    pub maturity: String,
    /// Retired (D63): the any-version shorthand. Kept only so the error can say what to write.
    #[serde(default)]
    pub depends: Vec<String>,
    /// `[module.dependencies]`: module id to the versions of it this module supports, a semver
    /// range that is required for every dependency, for example `hr = ">=0.1, <1"`. A
    /// dependency on a built-in module is checked against the core version.
    #[serde(default)]
    pub dependencies: std::collections::BTreeMap<String, String>,
    /// Network destinations the package may reach (D41). Declarative packages run no code, so
    /// this must stay empty; the field exists so the contract is the same for later tiers.
    #[serde(default)]
    pub network_destinations: Vec<String>,
}

/// `[[migration]]` (D63): one declarative data step of an upgrade.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MigrationM {
    /// The installed versions the step applies to, a requirement such as `<0.4`.
    pub from: String,
    /// `rename_field`, `set_default`, `drop_field` or `reformat_field`.
    pub op: String,
    pub model: String,
    /// The field: the old name for `rename_field`, the field filled for `set_default`, the field
    /// removed for `drop_field`.
    pub field: String,
    /// `rename_field`: the new name (a field of this version).
    pub to: Option<String>,
    /// `set_default`: the value written to every record where the field is empty.
    pub value: Option<toml::Value>,
    /// `drop_field`: must be `true`, because the values are deleted.
    #[serde(default)]
    pub confirm: bool,
    /// A sentence for the plan the administrator confirms.
    #[serde(default)]
    pub description: String,
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
    /// Row-level access rules (D58). Absent: the groups decide alone.
    pub access: Option<AccessM>,
    /// `"full"` (the default) or `"none"`: whether the platform keeps the change history of the
    /// records (D75). `"none"` needs `history_reason` and is refused by the gate for a model with
    /// a sensitive field or a state field that actions move.
    pub history: Option<String>,
    /// Why the model keeps no history; only with `history = "none"`.
    pub history_reason: Option<String>,
    /// Days to keep history entries; absent keeps them for ever. Not with `history = "none"`.
    pub history_retention_days: Option<u32>,
}

/// `[model.access]`: which records a caller may see and change besides the groups (D58).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessM {
    /// A text field holding the user id (username) of the person the record belongs to.
    pub own: Option<String>,
    /// A ref field: the record is visible when the record it points at is.
    pub via: Option<String>,
    /// A bool field: the record is visible to everyone who may read the model when true.
    pub public_field: Option<String>,
    /// A text field of user ids separated by commas, semicolons, spaces or line breaks (the eight
    /// ASCII separators of `obop_storage::keyexpr::MEMBER_SEPARATORS`; nothing is trimmed): those
    /// people see the record.
    pub shared_with: Option<String>,
    /// The holders of this group (or of any of these groups) see every record.
    pub see_all_group: Option<OneOrMany>,
}

/// A group name or a list of them.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    pub fn all(&self) -> Vec<&str> {
        match self {
            Self::One(s) => vec![s.as_str()],
            Self::Many(v) => v.iter().map(String::as_str).collect(),
        }
    }
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
    /// The server may read every field of the records for its own checks (never public).
    #[serde(default)]
    pub server_read: bool,
    /// Number fields the server changes atomically when it creates an order (stock, a code's
    /// use counter).
    #[serde(default)]
    pub server_adjust: Vec<String>,
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
    /// Field kind (D47, D60): text, longtext, number, money, date, bool, select, ref, image or
    /// file.
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
    /// Largest upload in bytes; only for `image` (default 5 MiB) and `file` (default 25 MiB),
    /// at most 64 MiB.
    pub max_bytes: Option<u32>,
    /// `kind = "decimal"`: digits in all, 1 to 38 (D72).
    pub precision: Option<u32>,
    /// `kind = "decimal"`: digits after the point, 0 to 12 and not above the precision.
    pub scale: Option<u32>,
    /// `kind = "decimal"`: `half_even`, `half_up` or `down`. On a stored field it is how the
    /// import path rounds input with too many fractional digits (without it such input is an
    /// error); on a computed field it is how the result and every division are rounded.
    pub round: Option<String>,
    /// Computed when the record is read, from an expression over its fields and child records
    /// (D57): `sum(lines.amount)`, `quantity * unit_price`. Read-only.
    pub computed: Option<String>,
    /// No two records may have the same value (D57).
    #[serde(default)]
    pub unique: bool,
    /// Scope of `unique`: the value need only be unique among records equal in these fields.
    #[serde(default)]
    pub unique_with: Vec<String>,
    /// `kind = "sequence"`: the number template, for example `INV-{year}-{seq:05}`.
    pub format: Option<String>,
    /// `kind = "sequence"`: the action that numbers the record (default: when it is created).
    pub assign_on: Option<String>,
    /// `kind = "sequence"`: numbered records cannot be deleted, so the series has no hole.
    #[serde(default)]
    pub gapless: bool,
    /// `kind = "sequence"`: `never` (default), `year` or `month`.
    pub reset: Option<String>,
}

/// Whether `v` is a value the field can hold, for `set_default`.
fn default_matches(f: &FieldM, v: &toml::Value) -> Result<(), String> {
    use toml::Value as T;
    let ok = match (f.kind.as_str(), v) {
        ("text" | "longtext" | "date" | "ref", T::String(_)) => true,
        ("number" | "money", T::Integer(_) | T::Float(_)) => true,
        ("datetime", T::String(s)) => obop_values::Instant::parse(s).is_ok(),
        // A decimal default is a string ("12.50") or a whole number; a TOML float is not exact.
        ("decimal", T::String(s)) => match (f.precision, f.scale) {
            (Some(p), Some(sc)) => {
                obop_values::Decimal::parse_for(s, p as u8, sc as u8, None).is_ok()
            }
            _ => false,
        },
        ("decimal", T::Integer(n)) => match (f.precision, f.scale) {
            (Some(p), Some(sc)) => {
                obop_values::Decimal::parse_for(&n.to_string(), p as u8, sc as u8, None).is_ok()
            }
            _ => false,
        },
        ("bool", T::Boolean(_)) => true,
        ("select", T::String(s)) => f.options.iter().any(|o| o == s),
        ("select", _) => false,
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err(format!(
            "the value does not fit the {} field {}{}",
            f.kind,
            f.name,
            if f.kind == "select" {
                format!(" (options: {})", f.options.join(", "))
            } else if matches!(f.kind.as_str(), "image" | "file" | "sequence") {
                " (it cannot be given a default)".to_string()
            } else {
                String::new()
            }
        ))
    }
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
    /// What the action does besides moving the state, in the same operation (D56).
    #[serde(default)]
    pub effect: Vec<EffectM>,
    /// Conditions that must hold before the action runs (D56).
    #[serde(default)]
    pub assert: Vec<AssertM>,
}

/// An assertion: an expression, or an expression with the message to show when it fails.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum AssertM {
    Plain(String),
    Full(AssertFull),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssertFull {
    pub expr: String,
    pub message: Option<String>,
}

/// `[[model.action.effect]]` (D56): one thing an action does in the same operation as its
/// transition.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectM {
    /// `create`, `update`, `adjust` or `set_state`.
    pub kind: String,
    /// The model of the records created or changed.
    pub model: String,
    /// `create`: a name that later effects use as `$created.name`.
    #[serde(rename = "as")]
    pub name: Option<String>,
    /// Repeat once per child record of this record (a model with a ref to this one).
    pub for_each: Option<String>,
    pub for_each_via: Option<String>,
    /// Run only when this expression about the record is true.
    pub when: Option<String>,
    /// `create`: field = value.
    pub map: Option<std::collections::BTreeMap<String, toml::Value>>,
    /// `update`: field = value.
    pub set: Option<std::collections::BTreeMap<String, toml::Value>>,
    /// `update`, `adjust`, `set_state`: a ref field of this record naming the record to act on.
    #[serde(rename = "ref")]
    pub ref_field: Option<String>,
    /// `update`, `set_state`: act on every record of `model` that refers to this one.
    #[serde(default)]
    pub children: bool,
    /// With `children`: which ref field, if the model has several to this one.
    pub via: Option<String>,
    /// `adjust`: find the one record whose fields equal these values.
    #[serde(rename = "match")]
    pub match_fields: Option<std::collections::BTreeMap<String, toml::Value>>,
    /// `adjust`: the number or money field to change.
    pub field: Option<String>,
    /// `adjust`: `add` or `subtract`.
    pub op: Option<String>,
    /// `adjust`: the amount, `$self.quantity` or a whole number.
    pub by: Option<toml::Value>,
    /// `adjust` with `match`: create the record when none matches.
    #[serde(default)]
    pub create_missing: bool,
    /// `adjust`: the field may not end below this.
    pub min: Option<i64>,
    /// `set_state`: the action of `model` to run.
    pub action: Option<String>,
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

/// An `https://` URL without whitespace or control characters, at most 200 long.
fn https_url_ok(u: &str) -> bool {
    u.len() <= 200
        && u.strip_prefix("https://").is_some_and(|rest| {
            let host = rest.split(['/', '?', '#']).next().unwrap_or("");
            !host.is_empty()
                && host.contains('.')
                && !host.starts_with(['.', '-'])
                && host
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == ':')
        })
        && !u.chars().any(|c| c.is_whitespace() || c.is_control())
}

fn keyword_ok(k: &str) -> bool {
    (2..=24).contains(&k.len())
        && k.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && k.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn icon_ok(i: &str) -> bool {
    i.len() <= 30
        && i.starts_with(|c: char| c.is_ascii_lowercase())
        && i.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A dependency declared by a module: the id and what version it must have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    pub id: String,
    pub req: Req,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, String> {
        Self::parse_as(text, false)
    }

    /// As [`Manifest::parse`], for the text of a package that is already installed: the one name
    /// that was free when it was installed and is taken now, an action called `history`, is let
    /// through, so a server still starts with the module its data belongs to (see
    /// [`Manifest::to_module_installed`]). Everything else is checked as for an upload.
    pub fn parse_installed(text: &str) -> Result<Self, String> {
        Self::parse_as(text, true)
    }

    fn parse_as(text: &str, installed: bool) -> Result<Self, String> {
        if text.len() > MAX_BYTES {
            return Err(format!("manifest is larger than {} KiB", MAX_BYTES / 1024));
        }
        let mut m: Manifest =
            toml::from_str(text).map_err(|e| format!("invalid obop.toml: {e}"))?;
        m.text = text.to_string();
        m.validate(installed)?;
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
    fn validate(&self, installed: bool) -> Result<(), String> {
        let id = &self.module.id;
        if !ident_ok(id) {
            return Err(format!(
                "module id {id:?} must be lowercase letters, digits and _, starting with a letter"
            ));
        }
        if Version::parse(&self.module.version).is_err() {
            return Err("version must look like 1.2.3".into());
        }
        if self.module.name.trim().is_empty() || self.module.description.trim().is_empty() {
            return Err("module needs a name and a description".into());
        }
        self.validate_meta()?;
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
            Self::validate_history(m)?;
            for g in [&m.read_group, &m.write_group]
                .into_iter()
                .chain(m.field.iter().filter_map(|f| f.group.as_ref()))
                .chain(m.action.iter().filter_map(|a| a.group.as_ref()))
            {
                if !group_ok(g) {
                    return Err(format!("permission group {g:?} must look like module.role"));
                }
            }
            if let Some(a) = &m.access {
                for g in a.see_all_group.iter().flat_map(OneOrMany::all) {
                    if !group_ok(g) {
                        return Err(format!(
                            "{}.access: see_all_group {g:?} must look like module.role",
                            m.name
                        ));
                    }
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
                let generated = GENERATED_COMMANDS.contains(&a.name.as_str())
                    && !(installed && a.name == "history");
                if !ident_ok(&a.name) || generated {
                    let hint = if a.name == "history" {
                        format!(
                            " (it is the generated command that reads the change history of a record; call the action `{}` instead)",
                            obop_registry::rename_hint("history")
                        )
                    } else {
                        String::new()
                    };
                    return Err(format!("{}: bad action name {:?}{hint}", m.name, a.name));
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
        // Expressions, formats and effects must at least read correctly; their meaning is
        // checked against the models by the gate (`to_module`).
        self.build_models()?;
        self.public_specs()?;
        check_labels(&self.labels, &self.shape())?;
        self.validate_migrations()?;
        Ok(())
    }

    /// The three history keys of a model (D75) say one thing between them.
    fn validate_history(m: &ModelM) -> Result<(), String> {
        let (name, reason) = (&m.name, m.history_reason.as_deref().map(str::trim));
        match m.history.as_deref() {
            None | Some("full") => {
                if reason.is_some() {
                    return Err(format!(
                        "{name}: history_reason only goes with history = \"none\""
                    ));
                }
            }
            Some("none") => {
                if reason.is_none_or(str::is_empty) {
                    return Err(format!(
                        "{name}: history = \"none\" needs a history_reason that says why the changes of its records need not be kept"
                    ));
                }
                if m.history_retention_days.is_some() {
                    return Err(format!(
                        "{name}: history_retention_days means nothing with history = \"none\""
                    ));
                }
            }
            Some(other) => {
                return Err(format!(
                    "{name}: history must be \"full\" or \"none\", not {other:?}"
                ));
            }
        }
        if m.history_retention_days == Some(0) {
            return Err(format!("{name}: history_retention_days must be at least 1"));
        }
        Ok(())
    }

    /// `[[migration]]` (D63): each step names a model and field of this package and says what to
    /// do to the records of an older version. The steps are checked against the shape of this
    /// version, so a step that could never run is refused when the package is read.
    fn validate_migrations(&self) -> Result<(), String> {
        if self.migration.len() > 100 {
            return Err("a package may declare at most 100 migration steps".into());
        }
        for (i, st) in self.migration.iter().enumerate() {
            let at = format!("migration {} ({} {}.{})", i + 1, st.op, st.model, st.field);
            let from = Req::parse(&st.from).map_err(|e| format!("{at}: from: {e}"))?;
            if !from.is_satisfiable() {
                return Err(format!("{at}: from = {:?} matches no version", st.from));
            }
            let Some(model) = self.model.iter().find(|m| m.name == st.model) else {
                return Err(format!(
                    "{at}: the package declares no model {}; a step can only change this package's own models",
                    st.model
                ));
            };
            if !ident_ok(&st.field) {
                return Err(format!("{at}: {:?} is not a field name", st.field));
            }
            let exists = |name: &str| model.field.iter().find(|f| f.name == name);
            match st.op.as_str() {
                "rename_field" => {
                    let Some(to) = &st.to else {
                        return Err(format!("{at}: rename_field needs `to`, the new name"));
                    };
                    if exists(to).is_none() {
                        return Err(format!(
                            "{at}: the new name {to} is not a field of {} in this version",
                            st.model
                        ));
                    }
                    if exists(&st.field).is_some() {
                        return Err(format!(
                            "{at}: {} is still a field of {} in this version, so it is not renamed away",
                            st.field, st.model
                        ));
                    }
                    if st.value.is_some() || st.confirm {
                        return Err(format!("{at}: rename_field takes only `to`"));
                    }
                }
                "set_default" => {
                    let Some(f) = exists(&st.field) else {
                        return Err(format!(
                            "{at}: {} is not a field of {} in this version",
                            st.field, st.model
                        ));
                    };
                    let Some(v) = &st.value else {
                        return Err(format!("{at}: set_default needs `value`"));
                    };
                    default_matches(f, v).map_err(|e| format!("{at}: {e}"))?;
                    if st.to.is_some() || st.confirm {
                        return Err(format!("{at}: set_default takes only `value`"));
                    }
                }
                "drop_field" => {
                    if exists(&st.field).is_some() {
                        return Err(format!(
                            "{at}: {} is still a field of {} in this version, so there is nothing to drop",
                            st.field, st.model
                        ));
                    }
                    if !st.confirm {
                        return Err(format!(
                            "{at}: dropping a field deletes its values from every record; add confirm = true to say so"
                        ));
                    }
                    if st.to.is_some() || st.value.is_some() {
                        return Err(format!("{at}: drop_field takes only `confirm`"));
                    }
                }
                "reformat_field" => {
                    // D72: rewrite the stored values of a decimal or datetime field in the form of
                    // its current shape (after a change of `scale` or `precision`).
                    let Some(f) = exists(&st.field) else {
                        return Err(format!(
                            "{at}: {} is not a field of {} in this version",
                            st.field, st.model
                        ));
                    };
                    if !matches!(f.kind.as_str(), "decimal" | "datetime") {
                        return Err(format!(
                            "{at}: reformat_field is for decimal and datetime fields, and {} is {}",
                            st.field, f.kind
                        ));
                    }
                    if f.computed.is_some() {
                        return Err(format!(
                            "{at}: {} is computed and stores nothing, so there is nothing to reformat",
                            st.field
                        ));
                    }
                    if st.to.is_some() || st.value.is_some() || st.confirm {
                        return Err(format!("{at}: reformat_field takes no other key"));
                    }
                }
                other => {
                    return Err(format!(
                        "{at}: unknown op {other:?} (use rename_field, set_default, drop_field or reformat_field)"
                    ));
                }
            }
        }
        Ok(())
    }

    /// The `[module]` keys of D55: licence, links, keywords, categories, core requirement,
    /// maturity, summary and the dependency tables. Every one is optional here; the catalogue
    /// rules that make some of them required are in `catalog_problems`.
    fn validate_meta(&self) -> Result<(), String> {
        let m = &self.module;
        let s = &m.summary;
        if s.chars().count() > 140 || s.contains('\n') {
            return Err("summary must be one line of at most 140 characters".into());
        }
        if m.author.chars().count() > 100 {
            return Err("author is longer than 100 characters".into());
        }
        if m.authors.len() > 10
            || m.authors
                .iter()
                .any(|a| a.trim().is_empty() || a.chars().count() > 100)
        {
            return Err("authors takes at most 10 names of 1 to 100 characters".into());
        }
        if !m.license.is_empty() {
            spdx::check(&m.license).map_err(|e| format!("license: {e}"))?;
        }
        for (key, url) in [
            ("homepage", &m.homepage),
            ("repository", &m.repository),
            ("documentation", &m.documentation),
            ("support", &m.support),
        ] {
            if !url.is_empty() && !https_url_ok(url) {
                return Err(format!(
                    "{key} must be an https:// URL (got {url:?}); plain http and other schemes are refused"
                ));
            }
        }
        if m.keywords.len() > 10 {
            return Err("keywords takes at most 10 entries".into());
        }
        let mut seen = std::collections::HashSet::new();
        for k in &m.keywords {
            if !keyword_ok(k) || !seen.insert(k) {
                return Err(format!(
                    "keyword {k:?} must be unique, 2 to 24 lowercase letters, digits or -"
                ));
            }
        }
        if m.categories.len() > 3 {
            return Err("categories takes at most 3 entries".into());
        }
        for (i, c) in m.categories.iter().enumerate() {
            if !MODULE_CATEGORIES.contains(&c.as_str()) {
                return Err(format!(
                    "unknown category {c:?} (use {})",
                    MODULE_CATEGORIES.join(", ")
                ));
            }
            if m.categories[..i].contains(c) {
                return Err(format!("category {c:?} is listed twice"));
            }
        }
        if !m.icon.is_empty() && !icon_ok(&m.icon) {
            return Err(format!(
                "icon {:?} must be a lowercase icon name such as \"project\"",
                m.icon
            ));
        }
        if !m.maturity.is_empty() && !MATURITIES.contains(&m.maturity.as_str()) {
            return Err(format!(
                "maturity must be one of {} (got {:?})",
                MATURITIES.join(", "),
                m.maturity
            ));
        }
        if m.changelog.chars().count() > 8000 {
            return Err("changelog is longer than 8000 characters".into());
        }
        if m.requires_core.trim().is_empty() {
            return Err(
                "requires_core is required: declare the core versions this module runs on, for example requires_core = \"^0.2\" or \">=0.2, <0.4\""
                    .into(),
            );
        }
        let core = Req::parse(&m.requires_core).map_err(|e| format!("requires_core: {e}"))?;
        if !core.is_satisfiable() {
            return Err(format!(
                "requires_core = {:?} matches no version: the range is empty",
                m.requires_core
            ));
        }
        if !m.depends.is_empty() {
            return Err(format!(
                "depends = [...] is no longer accepted: every dependency must name the versions it supports. Write [module.dependencies] with a range for each, for example {} = \">=0.1, <1\"",
                m.depends[0]
            ));
        }
        if m.dependencies.len() > 20 {
            return Err("a module may declare at most 20 dependencies".into());
        }
        for (d, req) in &m.dependencies {
            if !ident_ok(d) {
                return Err(format!("[module.dependencies]: {d:?} is not a module id"));
            }
            if req.trim().is_empty() {
                return Err(format!(
                    "[module.dependencies] {d}: the version range is empty; write the versions of {d} this module supports, for example {d} = \">=0.1, <1\""
                ));
            }
            let r = Req::parse(req).map_err(|e| format!("[module.dependencies] {d}: {e}"))?;
            if !r.is_satisfiable() {
                return Err(format!(
                    "[module.dependencies] {d} = {req:?} matches no version: the range is empty"
                ));
            }
        }
        if m.dependencies.contains_key(&m.id) {
            return Err(format!("{} depends on itself", m.id));
        }
        Ok(())
    }

    /// The requirement on the core version (`*` when the manifest declares none).
    pub fn core_req(&self) -> Req {
        Req::parse(&self.module.requires_core).unwrap_or_else(|_| Req::any())
    }

    /// Every dependency with the versions it supports, `[module.dependencies]` in id order.
    pub fn requirements(&self) -> Vec<Requirement> {
        self.module
            .dependencies
            .iter()
            .map(|(id, r)| Requirement {
                id: id.clone(),
                req: Req::parse(r).unwrap_or_else(|_| Req::any()),
            })
            .collect()
    }

    /// The ids of the modules this one depends on.
    pub fn dependency_ids(&self) -> Vec<String> {
        self.requirements().into_iter().map(|r| r.id).collect()
    }

    pub fn semver(&self) -> Version {
        Version::parse(&self.module.version).unwrap_or_else(|_| Version::new(0, 0, 0))
    }

    /// What a package must declare to be listed in a catalogue (bundled or marketplace).
    pub fn catalog_problems(&self) -> Vec<String> {
        let mut v = Vec::new();
        if self.module.license.is_empty() {
            v.push(
                "license is required for marketplace and bundled packages (an SPDX expression such as Apache-2.0, or LicenseRef-<name>)"
                    .to_string(),
            );
        }
        v.extend(self.unbounded().into_iter().map(|u| {
            format!(
                "{u} has no upper bound: a catalogue package names the versions it was written for, for example \"^0.2\" or \">=0.2, <0.4\""
            )
        }));
        v
    }

    /// The ranges that promise to work on every future version: the core requirement and the
    /// dependencies, named as the manifest writes them.
    pub fn unbounded(&self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.core_req().is_bounded() {
            out.push(format!("requires_core = {:?}", self.module.requires_core));
        }
        for r in self.requirements() {
            if !r.req.is_bounded() {
                out.push(format!("dependency {} = {:?}", r.id, r.req.as_str()));
            }
        }
        out
    }

    /// Things worth knowing that do not stop an install.
    pub fn warnings(&self) -> Vec<String> {
        let mut v = Vec::new();
        for u in self.unbounded() {
            v.push(format!(
                "{u} has no upper bound, so the module promises to run on every future version; name the versions it was written for"
            ));
        }
        if self.module.license.is_empty() {
            v.push("no license is declared".to_string());
        } else if let Ok(kind) = spdx::check(&self.module.license) {
            match kind {
                LicenseKind::Proprietary => {
                    v.push("the module is proprietary: its terms are not open source".to_string())
                }
                LicenseKind::Custom => v.push(
                    "the licence uses LicenseRef-...: read the terms that come with the module"
                        .to_string(),
                ),
                LicenseKind::Spdx => {}
            }
        }
        v
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
                .chain(&p.server_adjust)
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
            if !p.read && !p.submit && !has_receipt && !p.server_read && p.server_adjust.is_empty()
            {
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
                server_read: p.server_read,
                server_adjust: p.server_adjust.clone(),
            });
        }
        Ok(out)
    }

    /// Consistency of `kind`, `options`, `to` and `required` on one field.
    fn check_kind(&self, m: &ModelM, f: &FieldM) -> Result<(), String> {
        let at = format!("{}.{}", m.name, f.name);
        if !FieldKind::is_known(&f.kind) && f.kind != "sequence" {
            return Err(format!(
                "{at}: unknown kind {:?} (use text, longtext, number, money, date, datetime, decimal, bool, select, ref, sequence, image or file)",
                f.kind
            ));
        }
        let is_seq = f.kind == "sequence";
        if !is_seq
            && (f.format.is_some() || f.assign_on.is_some() || f.gapless || f.reset.is_some())
        {
            return Err(format!(
                "{at}: format, assign_on, gapless and reset are only valid for kind \"sequence\""
            ));
        }
        if is_seq {
            if f.format.is_none() {
                return Err(format!(
                    "{at}: a sequence needs a format such as \"INV-{{year}}-{{seq:05}}\""
                ));
            }
            if f.computed.is_some() || f.unique || !f.unique_with.is_empty() || f.required {
                return Err(format!(
                    "{at}: a sequence is numbered by the system; it cannot be computed, required or have unique (it is unique already)"
                ));
            }
        }
        if f.computed.is_some() && (f.required || f.unique || !f.unique_with.is_empty()) {
            return Err(format!(
                "{at}: a computed field cannot be required or unique"
            ));
        }
        if !f.unique && !f.unique_with.is_empty() {
            return Err(format!("{at}: unique_with needs unique = true"));
        }
        if f.kind != "select" && !f.options.is_empty() {
            return Err(format!("{at}: options are only valid for kind \"select\""));
        }
        if !matches!(f.kind.as_str(), "image" | "file") && f.max_bytes.is_some() {
            return Err(format!(
                "{at}: `max_bytes` is only valid for kind \"image\" or \"file\""
            ));
        }
        if let Some(n) = f.max_bytes
            && !(1024..=64 * 1024 * 1024).contains(&n)
        {
            return Err(format!(
                "{at}: `max_bytes` must be between 1024 and 67108864 (64 MiB)"
            ));
        }
        if f.kind != "ref" && f.to.is_some() {
            return Err(format!("{at}: `to` is only valid for kind \"ref\""));
        }
        if f.kind == "decimal" {
            let (Some(p), Some(sc)) = (f.precision, f.scale) else {
                return Err(format!(
                    "{at}: a decimal needs `precision` (digits in all, 1 to 38) and `scale` (digits after the point, 0 to 12), for example precision = 12, scale = 2"
                ));
            };
            if !(1..=u32::from(obop_values::MAX_PRECISION)).contains(&p) {
                return Err(format!("{at}: `precision` must be from 1 to 38, not {p}"));
            }
            if sc > u32::from(obop_values::MAX_SCALE) || sc > p {
                return Err(format!(
                    "{at}: `scale` must be from 0 to 12 and not above the precision ({p}), not {sc}"
                ));
            }
            if let Some(r) = &f.round
                && obop_values::Round::parse(r).is_none()
            {
                return Err(format!(
                    "{at}: `round` must be half_even, half_up or down, not {r:?}"
                ));
            }
        } else if f.precision.is_some() || f.scale.is_some() || f.round.is_some() {
            return Err(format!(
                "{at}: `precision`, `scale` and `round` are only valid for kind \"decimal\""
            ));
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
            for a in &m.action {
                for (i, e) in a.effect.iter().enumerate() {
                    if !self.model.iter().any(|x| x.name == e.model) {
                        v.push((
                            format!("{}.{} effect {}", m.name, a.name, i + 1),
                            e.model.clone(),
                        ));
                    }
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
                    .chain(
                        m.access
                            .iter()
                            .flat_map(|a| a.see_all_group.iter().flat_map(OneOrMany::all))
                            .map(String::from),
                    )
            })
            .collect();
        g.sort();
        g.dedup();
        g
    }

    /// The catalogue metadata as the registry keeps it.
    pub fn module_meta(&self) -> ModuleMeta {
        let m = &self.module;
        ModuleMeta {
            version: m.version.clone(),
            summary: m.summary.clone(),
            author: m.author.clone(),
            authors: m.authors.clone(),
            license: m.license.clone(),
            homepage: m.homepage.clone(),
            repository: m.repository.clone(),
            documentation: m.documentation.clone(),
            support: m.support.clone(),
            keywords: m.keywords.clone(),
            categories: m.categories.clone(),
            icon: m.icon.clone(),
            requires_core: m.requires_core.clone(),
            maturity: m.maturity.clone(),
            dependencies: self
                .requirements()
                .into_iter()
                .map(|r| (r.id, r.req.to_string()))
                .collect(),
        }
    }

    fn lit_or_src(v: &toml::Value, at: &str) -> Result<Src, String> {
        match v {
            toml::Value::String(t) => Src::parse(t).map_err(|e| format!("{at}: {e}")),
            toml::Value::Integer(n) => Ok(Src::Const(Lit::Int(*n))),
            toml::Value::Boolean(b) => Ok(Src::Const(Lit::Bool(*b))),
            _ => Err(format!(
                "{at}: a value is text, a whole number, true/false or a $variable"
            )),
        }
    }

    fn mapping(
        what: &str,
        m: &std::collections::BTreeMap<String, toml::Value>,
    ) -> Result<Vec<(&'static str, Src)>, String> {
        m.iter()
            .map(|(k, v)| {
                if !ident_ok(k) {
                    return Err(format!("{what}: {k:?} is not a field name"));
                }
                Ok((intern(k), Self::lit_or_src(v, &format!("{what}: {k}"))?))
            })
            .collect()
    }

    /// An effect as registry data. Everything that can be said without knowing the other models
    /// is checked here; the gate checks the rest.
    fn effect_of(m: &ModelM, a: &ActionM, i: usize, e: &EffectM) -> Result<Effect, String> {
        let what = format!("{}.{} effect {}", m.name, a.name, i + 1);
        if !ident_ok(&e.model) {
            return Err(format!("{what}: {:?} is not a model name", e.model));
        }
        let stray = |name: &str, present: bool| {
            if present {
                Err(format!("{what}: {name} does not apply to {}", e.kind))
            } else {
                Ok(())
            }
        };
        let need = |name: &str, present: bool| {
            if present {
                Ok(())
            } else {
                Err(format!("{what}: {} needs {name}", e.kind))
            }
        };
        if e.name.is_some() && e.kind != "create" {
            return Err(format!("{what}: `as` is only for create"));
        }
        if let Some(n) = &e.name
            && !ident_ok(n)
        {
            return Err(format!("{what}: {n:?} is not a name"));
        }
        let target = || -> Result<Target, String> {
            match (&e.ref_field, e.children) {
                (Some(r), false) => {
                    stray("via", e.via.is_some())?;
                    if !ident_ok(r) {
                        return Err(format!("{what}: {r:?} is not a field name"));
                    }
                    Ok(Target::Ref(intern(r)))
                }
                (None, true) => {
                    if let Some(v) = &e.via
                        && !ident_ok(v)
                    {
                        return Err(format!("{what}: {v:?} is not a field name"));
                    }
                    Ok(Target::Children(e.via.as_deref().map(intern)))
                }
                (Some(_), true) => Err(format!("{what}: use ref or children, not both")),
                (None, false) => Err(format!(
                    "{what}: {} needs ref (a ref field of this record) or children = true",
                    e.kind
                )),
            }
        };
        let model = intern(&e.model);
        let kind = match e.kind.as_str() {
            "create" => {
                stray("set", e.set.is_some())?;
                stray("ref", e.ref_field.is_some())?;
                stray("children", e.children)?;
                stray("via", e.via.is_some())?;
                stray("match", e.match_fields.is_some())?;
                stray("field", e.field.is_some())?;
                stray("op", e.op.is_some())?;
                stray("by", e.by.is_some())?;
                stray("create_missing", e.create_missing)?;
                stray("min", e.min.is_some())?;
                stray("action", e.action.is_some())?;
                let Some(map) = &e.map else {
                    return Err(format!("{what}: create needs map"));
                };
                EffectKind::Create {
                    model,
                    map: Self::mapping(&what, map)?,
                    name: e.name.clone(),
                }
            }
            "update" => {
                stray("map", e.map.is_some())?;
                stray("match", e.match_fields.is_some())?;
                stray("field", e.field.is_some())?;
                stray("op", e.op.is_some())?;
                stray("by", e.by.is_some())?;
                stray("create_missing", e.create_missing)?;
                stray("min", e.min.is_some())?;
                stray("action", e.action.is_some())?;
                let Some(set) = &e.set else {
                    return Err(format!("{what}: update needs set"));
                };
                EffectKind::Update {
                    model,
                    target: target()?,
                    set: Self::mapping(&what, set)?,
                }
            }
            "adjust" => {
                stray("map", e.map.is_some())?;
                stray("set", e.set.is_some())?;
                stray("children", e.children)?;
                stray("via", e.via.is_some())?;
                stray("action", e.action.is_some())?;
                need("field", e.field.is_some())?;
                need("op", e.op.is_some())?;
                need("by", e.by.is_some())?;
                let field = e.field.as_deref().unwrap_or_default();
                if !ident_ok(field) {
                    return Err(format!("{what}: {field:?} is not a field name"));
                }
                let subtract = match e.op.as_deref() {
                    Some("add") => false,
                    Some("subtract") => true,
                    other => {
                        return Err(format!(
                            "{what}: op must be \"add\" or \"subtract\", not {other:?}"
                        ));
                    }
                };
                let locator = match (&e.ref_field, &e.match_fields) {
                    (Some(r), None) => {
                        if !ident_ok(r) {
                            return Err(format!("{what}: {r:?} is not a field name"));
                        }
                        Locator::Ref(intern(r))
                    }
                    (None, Some(mf)) => Locator::Match(Self::mapping(&what, mf)?),
                    (Some(_), Some(_)) => {
                        return Err(format!("{what}: use ref or match, not both"));
                    }
                    (None, None) => {
                        return Err(format!(
                            "{what}: adjust needs ref (a ref field of this record) or match"
                        ));
                    }
                };
                let by = Self::lit_or_src(
                    e.by.as_ref().unwrap_or(&toml::Value::Integer(0)),
                    &format!("{what}: by"),
                )?;
                EffectKind::Adjust {
                    model,
                    locator,
                    field: intern(field),
                    subtract,
                    by,
                    create_missing: e.create_missing,
                    min: e.min,
                }
            }
            "set_state" => {
                stray("map", e.map.is_some())?;
                stray("set", e.set.is_some())?;
                stray("match", e.match_fields.is_some())?;
                stray("field", e.field.is_some())?;
                stray("op", e.op.is_some())?;
                stray("by", e.by.is_some())?;
                stray("create_missing", e.create_missing)?;
                stray("min", e.min.is_some())?;
                need("action", e.action.is_some())?;
                let action = e.action.as_deref().unwrap_or_default();
                if !ident_ok(action) {
                    return Err(format!("{what}: {action:?} is not an action name"));
                }
                EffectKind::SetState {
                    model,
                    target: target()?,
                    action: intern(action),
                }
            }
            other => {
                return Err(format!(
                    "{what}: unknown kind {other:?} (use create, update, adjust or set_state)"
                ));
            }
        };
        stray(
            "for_each_via",
            e.for_each.is_none() && e.for_each_via.is_some(),
        )
        .map_err(|_| format!("{what}: for_each_via needs for_each"))?;
        let mut eff = Effect::new(kind);
        eff.for_each = e.for_each.clone();
        eff.for_each_via = e.for_each_via.clone();
        if let Some(w) = &e.when {
            eff.when = Some(std::sync::Arc::new(
                obop_registry::expr::Expr::parse(w).map_err(|er| format!("{what}: when: {er}"))?,
            ));
            eff.when_source = Some(w.clone());
        }
        Ok(eff)
    }

    fn field_def(&self, m: &ModelM, f: &FieldM) -> Result<FieldDef, String> {
        let at = format!("{}.{}", m.name, f.name);
        let kind = match f.kind.as_str() {
            "longtext" => FieldKind::LongText,
            "number" => FieldKind::Number,
            "money" => FieldKind::Money,
            "date" => FieldKind::Date,
            "datetime" => FieldKind::Datetime,
            "decimal" => FieldKind::Decimal {
                precision: f.precision.unwrap_or(0) as u8,
                scale: f.scale.unwrap_or(0) as u8,
                round: f.round.as_deref().and_then(obop_values::Round::parse),
            },
            "bool" => FieldKind::Bool,
            "select" => FieldKind::Select(
                f.options
                    .iter()
                    .map(|o| intern(o))
                    .collect::<Vec<_>>()
                    .into(),
            ),
            "ref" => FieldKind::Ref(intern(f.to.as_deref().unwrap_or(""))),
            "image" => FieldKind::Image(f.max_bytes.unwrap_or(0)),
            "file" => FieldKind::File(f.max_bytes.unwrap_or(0)),
            _ => FieldKind::Text,
        };
        let sequence = if f.kind == "sequence" {
            let format = SeqFormat::parse(f.format.as_deref().unwrap_or_default())
                .map_err(|e| format!("{at}: format: {e}"))?;
            let reset = match f.reset.as_deref() {
                None => Reset::Never,
                Some(r) => Reset::parse(r).ok_or_else(|| {
                    format!("{at}: reset must be never, year or month, not {r:?}")
                })?,
            };
            if let Some(a) = &f.assign_on
                && !ident_ok(a)
            {
                return Err(format!("{at}: assign_on {a:?} is not an action name"));
            }
            Some(std::sync::Arc::new(SequenceSpec {
                format,
                assign_on: f.assign_on.as_deref().map(intern),
                gapless: f.gapless,
                reset,
            }))
        } else {
            None
        };
        let computed = match &f.computed {
            Some(src) => Some(std::sync::Arc::new(
                Computed::parse(src).map_err(|e| format!("{at}: computed: {e}"))?,
            )),
            None => None,
        };
        for w in &f.unique_with {
            if !ident_ok(w) {
                return Err(format!("{at}: unique_with: {w:?} is not a field name"));
            }
        }
        Ok(FieldDef {
            name: intern(&f.name),
            description: intern(&f.description),
            class: DataClass::parse(&f.class),
            permission_group: intern(f.group.as_deref().unwrap_or(&m.read_group)),
            personal_data: f.personal_data,
            kind,
            required: f.required,
            computed,
            unique: f.unique.then(|| Unique {
                with: f.unique_with.iter().map(|w| intern(w)).collect(),
            }),
            sequence,
        })
    }

    /// The models as registry data, rules included. Syntax errors in expressions, formats and
    /// effects are reported here, with the model, field or action they are in.
    fn build_models(&self) -> Result<Vec<ModelDef>, String> {
        let mut out = Vec::new();
        for m in &self.model {
            let fields = m
                .field
                .iter()
                .map(|f| self.field_def(m, f))
                .collect::<Result<Vec<_>, _>>()?;
            let mut commands = Vec::new();
            for a in &m.action {
                let c = CommandDef::new(
                    intern(&a.name),
                    intern(&a.description),
                    intern(a.group.as_deref().unwrap_or(&m.write_group)),
                );
                let mut c = c.transition(&a.field, a.from.as_deref(), &a.to);
                if a.approval {
                    c = c.approval();
                }
                if a.effect.len() > MAX_EFFECTS {
                    return Err(format!(
                        "{}.{}: an action has at most {MAX_EFFECTS} effects",
                        m.name, a.name
                    ));
                }
                if a.assert.len() > 8 {
                    return Err(format!("{}.{}: at most 8 assertions", m.name, a.name));
                }
                for (i, e) in a.effect.iter().enumerate() {
                    c.effects.push(Self::effect_of(m, a, i, e)?);
                }
                for x in &a.assert {
                    let (src, msg) = match x {
                        AssertM::Plain(s) => (s.as_str(), None),
                        AssertM::Full(f) => (f.expr.as_str(), f.message.as_deref()),
                    };
                    c.asserts.push(
                        Assertion::parse(src, msg)
                            .map_err(|e| format!("{}.{}: assert {src:?}: {e}", m.name, a.name))?,
                    );
                }
                commands.push(c);
            }
            let access = m.access.as_ref().map(|a| Access {
                own: a.own.as_deref().map(intern),
                via: a.via.as_deref().map(intern),
                public_field: a.public_field.as_deref().map(intern),
                shared_with: a.shared_with.as_deref().map(intern),
                see_all: a
                    .see_all_group
                    .iter()
                    .flat_map(OneOrMany::all)
                    .map(intern)
                    .collect(),
            });
            out.push(ModelDef {
                name: intern(&m.name),
                description: intern(&m.description),
                read_group: intern(&m.read_group),
                write_group: intern(&m.write_group),
                access,
                fields,
                commands,
                ext: ModelExt {
                    history: History {
                        // The command layer enables the history of the models it records.
                        enabled: false,
                        off_reason: (m.history.as_deref() == Some("none"))
                            .then(|| intern(m.history_reason.as_deref().unwrap_or("").trim())),
                        retention_days: m.history_retention_days,
                    },
                    ..ModelExt::default()
                },
            });
        }
        Ok(out)
    }

    /// Build registry metadata and run the D41 install gate over it.
    pub fn to_module(&self) -> Result<Module, String> {
        self.build_module(true)
    }

    /// As [`Manifest::to_module`], for a package that is already installed and is registered
    /// again (server start, a failed upgrade put back, a rollback): a field or model whose name
    /// was free when the package was installed and is reserved now (D75) is let through, so a
    /// server never starts without a module its data belongs to. Installing or upgrading uses
    /// `to_module`, which refuses the name.
    pub fn to_module_installed(&self) -> Result<Module, String> {
        self.build_module(false)
    }

    fn build_module(&self, reserved: bool) -> Result<Module, String> {
        let models = self.build_models()?;
        let module = Module {
            id: self.module.id.clone(),
            name: self.module.name.clone(),
            description: self.module.description.clone(),
            models,
            network_destinations: vec![],
            translations: std::sync::Arc::new(Translations::from_labels(&self.labels)),
            meta: self.module_meta(),
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
        let violations = if reserved {
            check_module(&module)
        } else {
            check_installed_module(&module)
        };
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
    use crate::semver::Version;

    pub const FLEET: &str = r#"
[module]
id = "fleet"
version = "0.1.0"
name = "Fleet"
description = "Vehicles and drivers"
requires_core = "^0.2"

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
        assert_eq!(module.models[0].all_commands().len(), 8);
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
    fn decimal_and_datetime_fields_parse_with_their_shape() {
        let at = |extra: &str| {
            FLEET.replace("name = \"plate\"", &format!("name = \"plate\"\n  {extra}"))
        };
        let kind_of = |t: &str| {
            Manifest::parse(t).unwrap().to_module().unwrap().models[0].fields[0]
                .kind
                .clone()
        };
        assert_eq!(
            kind_of(&at("kind = \"decimal\"\n  precision = 12\n  scale = 2")),
            FieldKind::Decimal {
                precision: 12,
                scale: 2,
                round: None
            }
        );
        assert_eq!(
            kind_of(&at(
                "kind = \"decimal\"\n  precision = 38\n  scale = 12\n  round = \"half_even\""
            )),
            FieldKind::Decimal {
                precision: 38,
                scale: 12,
                round: Some(obop_registry::Round::HalfEven)
            }
        );
        assert_eq!(kind_of(&at("kind = \"datetime\"")), FieldKind::Datetime);
        for (extra, want) in [
            ("kind = \"decimal\"", "needs `precision`"),
            ("kind = \"decimal\"\n  precision = 12", "needs `precision`"),
            (
                "kind = \"decimal\"\n  precision = 0\n  scale = 0",
                "from 1 to 38",
            ),
            (
                "kind = \"decimal\"\n  precision = 39\n  scale = 2",
                "from 1 to 38",
            ),
            (
                "kind = \"decimal\"\n  precision = 12\n  scale = 13",
                "0 to 12",
            ),
            (
                "kind = \"decimal\"\n  precision = 2\n  scale = 3",
                "not above the precision",
            ),
            (
                "kind = \"decimal\"\n  precision = 12\n  scale = 2\n  round = \"up\"",
                "half_even, half_up or down",
            ),
            (
                "kind = \"money\"\n  precision = 12",
                "only valid for kind \"decimal\"",
            ),
            (
                "kind = \"datetime\"\n  scale = 2",
                "only valid for kind \"decimal\"",
            ),
            ("round = \"down\"", "only valid for kind \"decimal\""),
            (
                "kind = \"decimal\"\n  precision = 12.5\n  scale = 2",
                "invalid obop.toml",
            ),
        ] {
            let e = err(&at(extra));
            assert!(e.contains(want), "{extra:?} -> {e}");
        }
        // A migration default for a decimal is a string or a whole number, never a float.
        let t = at("kind = \"decimal\"\n  precision = 6\n  scale = 2");
        let f = Manifest::parse(&t).unwrap().model[0].field[0].clone();
        let v = |s: &str| toml::from_str::<toml::Table>(s).unwrap()["v"].clone();
        let ok = |x: &str| default_matches(&f, &v(&format!("v = {x}"))).is_ok();
        assert!(ok("\"12.5\"") && ok("12"));
        assert!(!ok("12.5") && !ok("\"1.234\"") && !ok("\"1e3\"") && !ok("true") && !ok("1234567"));
        let t = at("kind = \"datetime\"");
        let f = Manifest::parse(&t).unwrap().model[0].field[0].clone();
        let ok = |x: &str| default_matches(&f, &v(&format!("v = {x}"))).is_ok();
        assert!(ok("\"2026-03-29T01:30:00Z\"") && !ok("\"2026-03-29\"") && !ok("5"));
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
        for name in [
            "get",
            "list",
            "search",
            "create",
            "bulk_create",
            "update",
            "delete",
            "history",
        ] {
            let bad = FLEET.replace("name = \"dispatch\"", &format!("name = \"{name}\""));
            assert!(err(&bad).contains("bad action name"), "{name}");
        }
    }

    #[test]
    fn a_reserved_field_name_is_refused_when_installing_and_says_what_to_use() {
        for (name, hint) in [
            ("created_at", "recorded_at"),
            ("updated_at", "modified_at"),
            ("created_by", "author"),
            ("updated_by", "last_editor"),
            ("row_version", "revision"),
            ("sys_backfilled", "estimated"),
            ("if_version", "expected_version"),
        ] {
            let text = FLEET.replace("name = \"plate\"", &format!("name = \"{name}\""));
            // The text is a good manifest: only the gate objects, and names the field and the hint.
            let m = Manifest::parse(&text).unwrap();
            let e = m.to_module().unwrap_err();
            assert!(e.contains(&format!("fleet.fleet_vehicle.{name}")), "{e}");
            assert!(e.contains(&format!("`{name}`")), "{e}");
            assert!(e.contains(&format!("`{hint}`")), "{e}");
            // An installed package with that name is registered all the same.
            let module = m.to_module_installed().unwrap();
            assert_eq!(module.models[0].fields[0].name, name);
        }
    }

    #[test]
    fn an_action_called_history_is_refused_when_installing_and_kept_when_installed() {
        let text = FLEET.replace("name = \"dispatch\"", "name = \"history\"");
        let e = err(&text);
        assert!(e.contains("bad action name \"history\""), "{e}");
        assert!(e.contains("`show_history`"), "{e}");
        // The text of a package that is installed already is read, and its module is built.
        let m = Manifest::parse_installed(&text).unwrap();
        let module = m.to_module_installed().unwrap();
        assert!(
            module.models[0]
                .commands
                .iter()
                .any(|c| c.name == "history")
        );
        assert_eq!(
            module.models[0].all_commands().len(),
            8,
            "its own, not a ninth"
        );
        // Only that name is let through, and only for an installed package; `to_module` still
        // refuses the module built from it.
        let e = m.to_module().unwrap_err();
        assert!(e.contains("`show_history`"), "{e}");
        for name in [
            "get",
            "list",
            "search",
            "create",
            "bulk_create",
            "update",
            "delete",
        ] {
            let bad = FLEET.replace("name = \"dispatch\"", &format!("name = \"{name}\""));
            assert!(Manifest::parse_installed(&bad).is_err(), "{name}");
        }
    }

    #[test]
    fn a_reserved_model_name_is_refused_when_installing() {
        for (id, model) in [
            ("record", "record_change"),
            ("storage", "storage_event"),
            ("storage", "storage_index"),
            ("data", "data_guard"),
        ] {
            let text = FLEET
                .replace("id = \"fleet\"", &format!("id = \"{id}\""))
                .replace("name = \"fleet_vehicle\"", &format!("name = \"{model}\""));
            let m = Manifest::parse(&text).unwrap();
            let e = m.to_module().unwrap_err();
            assert!(e.contains(&format!("`{model}`")), "{e}");
            assert!(m.to_module_installed().is_ok());
        }
    }

    fn with_history(keys: &str) -> String {
        FLEET.replace(
            "write_group = \"fleet.manager\"
",
            &format!(
                "write_group = \"fleet.manager\"
{keys}
"
            ),
        )
    }

    #[test]
    fn the_history_keys_reach_the_registry_and_change_nothing_yet() {
        let plain = Manifest::parse(FLEET).unwrap().to_module().unwrap();
        assert_eq!(plain.models[0].ext, ModelExt::default());
        // `full` is the default and sets nothing.
        let full = Manifest::parse(&with_history(
            "history = \"full\"
history_retention_days = 90",
        ))
        .unwrap()
        .to_module()
        .unwrap();
        let h = &full.models[0].ext.history;
        assert_eq!((h.off_reason, h.retention_days), (None, Some(90)));
        // Nothing enables a model's history from a manifest: the model has no `history` command.
        assert!(!h.enabled && !full.models[0].history_on());
        assert_eq!(full.models[0].all_commands().len(), 8);
        // `none` with a reason, on a model that has no sensitive field and no state field moved by
        // an action (FLEET has both an action and a state, so take the action away).
        let simple = Manifest::parse(&with_history(
            "history = \"none\"
history_reason = \" a counter nobody audits \"",
        ))
        .unwrap();
        let e = simple.to_module().unwrap_err();
        assert!(e.contains("state field state"), "{e}");
        let no_action = |keys: &str| {
            let t = with_history(keys);
            let cut = t.find("  [[model.action]]").unwrap();
            Manifest::parse(&t[..cut]).unwrap()
        };
        let none = no_action(
            "history = \"none\"
history_reason = \" a counter nobody audits \"",
        )
        .to_module()
        .unwrap();
        assert_eq!(
            none.models[0].ext.history.off_reason,
            Some("a counter nobody audits")
        );
        // A sensitive field forbids it.
        let sensitive = no_action(
            "history = \"none\"
history_reason = \"x\"",
        )
        .text
        .replace("class = \"internal\"", "class = \"special_category\"");
        let e = Manifest::parse(&sensitive)
            .unwrap()
            .to_module()
            .unwrap_err();
        assert!(e.contains("sensitive field (driver"), "{e}");
    }

    #[test]
    fn each_history_key_has_its_own_error() {
        for (keys, wants) in [
            ("history = \"maybe\"", "must be \"full\" or \"none\""),
            ("history = \"none\"", "needs a history_reason"),
            (
                "history = \"none\"
history_reason = \"  \"",
                "needs a history_reason",
            ),
            (
                "history_reason = \"why\"",
                "only goes with history = \"none\"",
            ),
            (
                "history = \"none\"
history_reason = \"why\"
history_retention_days = 5",
                "means nothing with history = \"none\"",
            ),
            ("history_retention_days = 0", "at least 1"),
            ("history_retention_days = -1", "invalid obop.toml"),
            ("history = true", "invalid obop.toml"),
        ] {
            let e = Manifest::parse(&with_history(keys)).unwrap_err();
            assert!(e.contains(wants), "{keys}: {e}");
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

    /// FLEET with extra `[module]` keys inserted after the description.
    fn with_meta(extra: &str) -> String {
        if extra.contains("requires_core") {
            FLEET.replace("requires_core = \"^0.2\"\n", "").replace(
                "description = \"Vehicles and drivers\"\n",
                &format!("description = \"Vehicles and drivers\"\n{extra}\n"),
            )
        } else {
            FLEET.replace(
                "requires_core = \"^0.2\"\n",
                &format!("requires_core = \"^0.2\"\n{extra}\n"),
            )
        }
    }

    fn ok_meta(extra: &str) -> Manifest {
        Manifest::parse(&with_meta(extra)).unwrap_or_else(|e| panic!("{extra}: {e}"))
    }

    #[test]
    fn every_new_key_parses_and_reaches_the_registry() {
        let m = ok_meta(
            r#"summary = "Vehicles in one line"
authors = ["Ada Example", "Sam Example"]
license = "AGPL-3.0-or-later"
homepage = "https://example.com"
repository = "https://github.com/SpaceCorps/obop"
documentation = "https://example.com/docs?x=1#top"
support = "https://example.com/support"
keywords = ["fleet", "vehicles", "e-bike", "v2"]
categories = ["supply-chain", "services"]
icon = "fleet"
requires_core = ">=0.2, <0.4"
maturity = "beta"
"#,
        );
        let meta = m.to_module().unwrap().meta;
        assert_eq!(meta.version, "0.1.0");
        assert_eq!(meta.summary, "Vehicles in one line");
        assert_eq!(meta.authors, ["Ada Example", "Sam Example"]);
        assert_eq!(meta.license, "AGPL-3.0-or-later");
        assert_eq!(meta.documentation, "https://example.com/docs?x=1#top");
        assert_eq!(meta.keywords.len(), 4);
        assert_eq!(meta.categories, ["supply-chain", "services"]);
        assert_eq!(meta.icon, "fleet");
        assert_eq!(meta.requires_core, ">=0.2, <0.4");
        assert_eq!(meta.maturity, "beta");
        assert!(m.core_req().matches(&Version::new(0, 3, 9)));
        assert!(!m.core_req().matches(&Version::new(0, 4, 0)));
        assert!(m.catalog_problems().is_empty());
    }

    #[test]
    fn only_the_licence_is_optional_for_an_upload_and_the_catalogue_wants_bounds() {
        let m = Manifest::parse(FLEET).unwrap();
        assert_eq!(m.module.license, "");
        assert!(m.core_req().is_bounded());
        let w = m.warnings();
        assert!(w.iter().any(|x| x.contains("no license")), "{w:?}");
        assert_eq!(m.catalog_problems().len(), 1);
        // The core range is required, and a range with no upper bound is a warning on an upload
        // and an error in the catalogue (D63).
        let e = err(&FLEET.replace("requires_core = \"^0.2\"\n", ""));
        assert!(e.contains("requires_core is required"), "{e}");
        let e = err(&FLEET.replace("\"^0.2\"", "\"\""));
        assert!(e.contains("requires_core is required"), "{e}");
        let open = Manifest::parse(&FLEET.replace("\"^0.2\"", "\">=0.2\"")).unwrap();
        assert!(
            open.warnings()
                .iter()
                .any(|x| x.contains("has no upper bound"))
        );
        assert!(
            open.catalog_problems()
                .iter()
                .any(|x| x.contains("has no upper bound"))
        );
        let any = Manifest::parse(&FLEET.replace("\"^0.2\"", "\"*\"")).unwrap();
        assert!(!any.unbounded().is_empty());
        let e = err(&FLEET.replace("\"^0.2\"", "\">=2, <1\""));
        assert!(e.contains("matches no version"), "{e}");
        // A proprietary or custom licence is allowed and flagged.
        let w = ok_meta("license = \"proprietary\"").warnings();
        assert!(w.iter().any(|x| x.contains("proprietary")), "{w:?}");
        let w = ok_meta("license = \"LicenseRef-Acme\"").warnings();
        assert!(w.iter().any(|x| x.contains("LicenseRef")), "{w:?}");
        assert!(ok_meta("license = \"UNLICENSED\"").module.license == "UNLICENSED");
    }

    #[test]
    fn each_new_key_has_its_own_error() {
        let long = "x".repeat(141);
        let cases: Vec<(String, &str)> = vec![
            (format!("summary = \"{long}\""), "summary must be one line of at most 140"),
            ("summary = \"a\\nb\"".into(), "summary must be one line"),
            ("license = \"Apache\"".into(), "license: \"Apache\" is not a known SPDX"),
            ("license = \"MIT OR\"".into(), "license:"),
            ("homepage = \"http://example.com\"".into(), "homepage must be an https:// URL"),
            ("repository = \"ftp://example.com\"".into(), "repository must be an https:// URL"),
            ("documentation = \"https://\"".into(), "documentation must be an https:// URL"),
            ("support = \"https://exa mple.com\"".into(), "support must be an https:// URL"),
            ("support = \"https://nodot\"".into(), "support must be an https:// URL"),
            ("keywords = [\"a\"]".into(), "keyword \"a\""),
            ("keywords = [\"Fleet\"]".into(), "keyword \"Fleet\""),
            ("keywords = [\"fleet\", \"fleet\"]".into(), "keyword \"fleet\""),
            ("keywords = [\"aa\",\"bb\",\"cc\",\"dd\",\"ee\",\"ff\",\"gg\",\"hh\",\"ii\",\"jj\",\"kk\"]".into(), "at most 10"),
            ("categories = [\"misc\"]".into(), "unknown category \"misc\""),
            ("categories = [\"sales\", \"sales\"]".into(), "listed twice"),
            ("categories = [\"sales\",\"finance\",\"people\",\"other\"]".into(), "at most 3"),
            ("icon = \"Big Icon\"".into(), "icon \"Big Icon\""),
            ("maturity = \"alpha\"".into(), "maturity must be one of stable, beta, experimental"),
            ("requires_core = \"soon\"".into(), "requires_core:"),
            ("requires_core = \">=2, <1\"".into(), "matches no version"),
            ("authors = [\"\"]".into(), "authors takes at most 10"),
            ("depends = [\"hr\"]".into(), "depends = [...] is no longer accepted"),
            ("[module.dependencies]\nhr = \"nonsense\"".into(), "[module.dependencies] hr:"),
            ("[module.dependencies]\nhr = \"\"".into(), "the version range is empty"),
            ("[module.dependencies]\nhr = \"   \"".into(), "the version range is empty"),
            ("[module.dependencies]\nhr = \">=2, <1\"".into(), "matches no version"),
            ("[module.dependencies]\nfleet = \"^0.1\"".into(), "fleet depends on itself"),
            ("[module.dependencies]\nBad = \"^1\"".into(), "is not a module id"),
            ("changelog = \"\"\n[[migration]]\nfrom = \"<1\"\nop = \"drop_field\"\nmodel = \"fleet_vehicle\"\nfield = \"gone\"".into(), "confirm = true"),
        ];
        for (extra, want) in cases {
            // `depends` keys must come before the table header, which with_meta keeps in order.
            let e = Manifest::parse(&with_meta(&extra)).unwrap_err();
            assert!(e.contains(want), "{extra:?}: wanted {want:?}, got {e}");
        }
        // A dependency value that is not a string is a parse error, not silently accepted.
        let e = Manifest::parse(&with_meta(
            "[module.dependencies]\nhr = { version = \"*\", optional = true }",
        ))
        .unwrap_err();
        assert!(e.contains("invalid obop.toml"), "{e}");
        // Unknown keys stay refused.
        let e = Manifest::parse(&with_meta("optional = true")).unwrap_err();
        assert!(e.contains("unknown field"), "{e}");
        // Prerelease versions are accepted; other shapes are not.
        assert!(Manifest::parse(&FLEET.replace("0.1.0", "1.0.0-rc.1")).is_ok());
        assert!(err(&FLEET.replace("0.1.0", "1.0")).contains("version"));
    }

    #[test]
    fn every_dependency_carries_its_range() {
        let m = ok_meta("[module.dependencies]\nbase = \"^0.2\"\nhr = \">=0.1, <1\"");
        let r = m.requirements();
        assert_eq!(r.len(), 2);
        assert_eq!(
            (r[0].id.as_str(), r[0].req.to_string()),
            ("base", "^0.2".to_string())
        );
        assert_eq!(
            (r[1].id.as_str(), r[1].req.to_string()),
            ("hr", ">=0.1, <1".to_string())
        );
        assert_eq!(m.dependency_ids(), ["base", "hr"]);
        let meta = m.module_meta();
        assert_eq!(
            meta.dependencies[1],
            ("hr".to_string(), ">=0.1, <1".to_string())
        );
        // A wildcard parses (an upload may use it) but is flagged as unbounded.
        let any = ok_meta("[module.dependencies]\nhr = \"*\"");
        assert!(any.unbounded().iter().any(|u| u.contains("dependency hr")));
        assert!(
            any.catalog_problems()
                .iter()
                .any(|u| u.contains("dependency hr"))
        );
    }
}
