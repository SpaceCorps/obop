//! Layer 3 registry: model, field and command metadata, and the D41 install gate.

use obop_types::DataClass;
pub use obop_values::Round;
use obop_values::{MAX_PRECISION, MAX_SCALE};
use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock};

pub mod expr;
pub mod ext;
pub mod labels;
pub mod rules;
pub mod template;
pub use ext::{
    CondOp, Exit, Guard, History, IndexCond, IndexDef, IndexKind, LockDate, ModelExt, StateSet,
};
pub use labels::Translations;
pub use rules::{
    Access, Assertion, Computed, Effect, EffectKind, Lit, Locator, Reset, SeqFormat, SequenceSpec,
    Src, Target, Unique,
};

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

/// The type of a field (D47). Serialised lowercase (`FieldKind::name`) in manifests and in
/// `/api/meta`. Cheap to clone: options are shared, the target model name is interned.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum FieldKind {
    /// JSON string.
    #[default]
    Text,
    /// JSON string, multi-line.
    LongText,
    /// JSON integer.
    Number,
    /// JSON integer in minor currency units, never a float.
    Money,
    /// JSON string `YYYY-MM-DD`, a real calendar date.
    Date,
    /// JSON boolean.
    Bool,
    /// JSON string, one of the options (in status order for a `state` field).
    Select(Arc<[&'static str]>),
    /// JSON string: the id of an existing record of the named model.
    Ref(&'static str),
    /// JSON string: the id (64 lowercase hex characters, the SHA-256) of a stored image blob
    /// (D60). The number is the largest accepted upload in bytes; 0 means the default.
    Image(u32),
    /// As `Image`, for a document kept byte for byte and served as a download.
    File(u32),
    /// JSON string: an exact decimal in its canonical form, always exactly `scale` fractional
    /// digits (`"12.3400"`), never a JSON number (D72). `precision` (at most 38) counts all
    /// digits, `scale` (0 to 12) those after the point. `round` is the import path's rounding
    /// mode for input with too many fractional digits; without it such input is an error. In a
    /// computed field it is the rounding of the result (default `half_even`).
    Decimal {
        precision: u8,
        scale: u8,
        round: Option<Round>,
    },
    /// JSON string: an instant with microsecond precision, always UTC, always
    /// `YYYY-MM-DDTHH:MM:SS.ffffffZ` (D72).
    Datetime,
}

impl FieldKind {
    /// The lowercase name used in manifests and `/api/meta`.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::LongText => "longtext",
            Self::Number => "number",
            Self::Money => "money",
            Self::Date => "date",
            Self::Bool => "bool",
            Self::Select(_) => "select",
            Self::Ref(_) => "ref",
            Self::Image(_) => "image",
            Self::File(_) => "file",
            Self::Decimal { .. } => "decimal",
            Self::Datetime => "datetime",
        }
    }

    /// A `decimal` of this shape; `None` when the precision or scale is out of range.
    pub fn decimal(precision: u8, scale: u8, round: Option<Round>) -> Option<Self> {
        ((1..=MAX_PRECISION).contains(&precision) && scale <= MAX_SCALE && scale <= precision)
            .then_some(Self::Decimal {
                precision,
                scale,
                round,
            })
    }

    /// `(precision, scale, round)` of a `decimal`.
    pub fn decimal_shape(&self) -> Option<(u8, u8, Option<Round>)> {
        match self {
            Self::Decimal {
                precision,
                scale,
                round,
            } => Some((*precision, *scale, *round)),
            _ => None,
        }
    }

    /// Whether `name` is one of the kind names.
    pub fn is_known(name: &str) -> bool {
        matches!(
            name,
            "text"
                | "longtext"
                | "number"
                | "money"
                | "date"
                | "bool"
                | "select"
                | "ref"
                | "image"
                | "file"
                | "decimal"
                | "datetime"
        )
    }

    /// The options of a `select`, empty for every other kind.
    pub fn options(&self) -> &[&'static str] {
        match self {
            Self::Select(o) => o,
            _ => &[],
        }
    }

    /// The target model of a `ref`.
    pub fn target(&self) -> Option<&'static str> {
        match self {
            Self::Ref(t) => Some(t),
            _ => None,
        }
    }

    /// What a valid value is, for "`model.field` must be ..." messages.
    pub fn describe(&self) -> String {
        match self {
            Self::Text | Self::LongText => "a string".into(),
            Self::Number => "an integer".into(),
            Self::Money => "an integer in minor currency units".into(),
            Self::Date => "a real calendar date as YYYY-MM-DD".into(),
            Self::Bool => "true or false".into(),
            Self::Select(o) => format!("one of: {}", o.join(", ")),
            Self::Ref(t) => format!("the id of an existing {t} record"),
            Self::Image(_) => "the id of an uploaded image (upload it with POST /api/blobs)".into(),
            Self::File(_) => "the id of an uploaded file (upload it with POST /api/blobs)".into(),
            Self::Decimal {
                precision, scale, ..
            } => {
                let example = obop_values::decimal::example(*precision, *scale);
                format!(
                    "a decimal number written as a string such as \"{example}\" (at most {precision} digits, {scale} after the point; not a JSON number)"
                )
            }
            Self::Datetime => {
                "an RFC 3339 instant with an offset or Z such as 2026-03-29T01:30:00Z".into()
            }
        }
    }

    /// The regular expression for the text of a decimal of this shape (OpenAPI `pattern`):
    /// digits that fit the precision, then up to `scale` fractional digits. Responses always
    /// carry exactly `scale` of them.
    pub fn decimal_pattern(&self) -> Option<String> {
        let (precision, scale, _) = self.decimal_shape()?;
        Some(obop_values::decimal::pattern(precision, scale))
    }

    /// The largest upload in bytes this field accepts (`image` and `file` only): the field's own
    /// limit, else 5 MiB for an image and 25 MiB for a file, never more than 64 MiB.
    pub fn max_bytes(&self) -> Option<usize> {
        let cap = |n: u32, default: usize| {
            if n == 0 {
                default
            } else {
                (n as usize).min(64 * 1024 * 1024)
            }
        };
        match self {
            Self::Image(n) => Some(cap(*n, 5 * 1024 * 1024)),
            Self::File(n) => Some(cap(*n, 25 * 1024 * 1024)),
            _ => None,
        }
    }

    /// Whether the field holds a blob id.
    pub fn is_blob(&self) -> bool {
        matches!(self, Self::Image(_) | Self::File(_))
    }
}

/// Whether `s` is a real calendar date written `YYYY-MM-DD` (leap years included).
pub fn is_valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let digits = |r: std::ops::Range<usize>| -> Option<u32> {
        let part = s.get(r)?;
        if part.bytes().all(|c| c.is_ascii_digit()) {
            part.parse().ok()
        } else {
            None
        }
    };
    let (Some(y), Some(m), Some(d)) = (digits(0..4), digits(5..7), digits(8..10)) else {
        return false;
    };
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    y >= 1 && (1..=days).contains(&d)
}

#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: &'static str,
    pub description: &'static str,
    pub class: Option<DataClass>,
    pub permission_group: &'static str,
    pub personal_data: bool,
    pub kind: FieldKind,
    pub required: bool,
    /// Computed on read from the record and its children; never stored or written (D57).
    pub computed: Option<Arc<Computed>>,
    /// No two records may share this value (D57).
    pub unique: Option<Unique>,
    /// Numbered by the system (D57).
    pub sequence: Option<Arc<SequenceSpec>>,
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
    /// Set when the command is a state transition (D48): it moves a `select` field to a new
    /// value. The field is then governed: generic `create` and `update` may not set it freely.
    pub transition: Option<Transition>,
    /// What the action does besides moving the state, in the same operation (D56).
    pub effects: Vec<Effect>,
    /// Conditions that must hold before the action runs (D56).
    pub asserts: Vec<Assertion>,
}

/// A declared state transition: set `field` to `to`, only when it currently equals `from` (if
/// given). Part of the registry metadata so every surface and the generic commands agree on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transition {
    pub field: &'static str,
    pub from: Option<&'static str>,
    pub to: &'static str,
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
            transition: None,
            effects: Vec::new(),
            asserts: Vec::new(),
        }
    }

    /// Declare the command a transition of `field` (strings are interned, so literals and
    /// manifest text both work).
    pub fn transition(mut self, field: &str, from: Option<&str>, to: &str) -> Self {
        self.transition = Some(Transition {
            field: intern(field),
            from: from.map(intern),
            to: intern(to),
        });
        self
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
            kind: FieldKind::Text,
            required: false,
            computed: None,
            unique: None,
            sequence: None,
        }
    }

    pub fn kind(mut self, kind: FieldKind) -> Self {
        self.kind = kind;
        self
    }

    /// A `select` with these options (interned, so literals and manifest strings both work).
    pub fn select(self, options: &[&str]) -> Self {
        let o: Vec<&'static str> = options.iter().map(|x| intern(x)).collect();
        self.kind(FieldKind::Select(o.into()))
    }

    /// A `ref` to records of `model`.
    pub fn reference(self, model: &str) -> Self {
        self.kind(FieldKind::Ref(intern(model)))
    }

    /// An exact `decimal` of `precision` digits, `scale` of them after the point (D72).
    pub fn decimal(self, precision: u8, scale: u8) -> Self {
        self.kind(FieldKind::Decimal {
            precision,
            scale,
            round: None,
        })
    }

    /// A `decimal` whose import path rounds extra fractional digits with `round`, and whose
    /// computed value (if any) is rounded the same way.
    pub fn decimal_rounded(self, precision: u8, scale: u8, round: Round) -> Self {
        self.kind(FieldKind::Decimal {
            precision,
            scale,
            round: Some(round),
        })
    }

    /// An instant with microsecond precision (D72).
    pub fn datetime(self) -> Self {
        self.kind(FieldKind::Datetime)
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
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
    /// Row-level access rules (D58); none: the groups decide alone.
    pub access: Option<Access>,
    /// Record history, declared indexes and the immutability guard; `Default::default()` for a
    /// model that declares none of them. New per-model settings of that kind go into `ModelExt`,
    /// not into this struct, so that no `ModelDef` literal changes again.
    pub ext: ModelExt,
}

impl ModelDef {
    /// Generated `get`, `list`, `search`, `create`, `bulk_create`, `update` and `delete`, followed
    /// by the custom commands. The REST, MCP and CLI surfaces are derived from this list (D29, D41, D49).
    pub fn all_commands(&self) -> Vec<CommandDef> {
        let crud = CommandDef::new;
        let create = crud("create", "Create a record", self.write_group);
        // Creating many at once needs exactly the approval that creating one needs (D70).
        let mut bulk = crud(
            "bulk_create",
            "Create many records in one call (at most 1000): rows as a list of objects, or csv text with field names as headers. Every row gets the checks of create. All or nothing by default; mode partial keeps the good rows and reports the bad ones; dry_run checks without writing; idempotency_key makes a retried call return the first answer",
            self.write_group,
        );
        bulk.needs_approval = create.needs_approval;
        let mut v = vec![
            crud("get", "Read one record by id", self.read_group),
            crud(
                "list",
                "List records; optional filter, order, limit and offset",
                self.read_group,
            ),
            crud(
                "search",
                "Like list, but returns {rows, total} with the count before limit and offset",
                self.read_group,
            ),
            create,
            bulk,
            crud("update", "Update fields of a record", self.write_group),
            crud(
                "delete",
                "Delete a record that no other record refers to",
                self.write_group,
            )
            .approval(),
        ];
        v.extend(self.commands.iter().cloned());
        v
    }

    /// The transitions declared by this model's commands, with the command that runs each.
    pub fn transitions(&self) -> impl Iterator<Item = (&CommandDef, Transition)> {
        self.commands
            .iter()
            .filter_map(|c| c.transition.map(|t| (c, t)))
    }

    /// Fields that are the target of at least one declared transition (D48).
    pub fn governed_fields(&self) -> Vec<&'static str> {
        let mut v: Vec<&'static str> = self.transitions().map(|(_, t)| t.field).collect();
        v.sort_unstable();
        v.dedup();
        v
    }
}

/// Command names the platform generates for every model; a custom command cannot use them.
pub const GENERATED_COMMANDS: [&str; 7] = [
    "get",
    "list",
    "search",
    "create",
    "bulk_create",
    "update",
    "delete",
];

/// Record keys the platform sets itself on every tracked record (D75), in storage order. Not yet
/// refused by the install gate; the record-history step does that.
pub const SYSTEM_FIELDS: [&str; 6] = [
    "created_at",
    "updated_at",
    "created_by",
    "updated_by",
    "row_version",
    "sys_backfilled",
];

/// Names a package field cannot take once the system fields exist: the system fields, the record
/// id (the gate refuses `id` today) and `if_version`, the optimistic-lock input of `update`.
pub const RESERVED_FIELDS: [&str; 8] = [
    "created_at",
    "updated_at",
    "created_by",
    "updated_by",
    "row_version",
    "sys_backfilled",
    "id",
    "if_version",
];

/// Model names the platform keeps for its own records (D75, D76, D77, D78); a package cannot
/// declare a model of this name once the install gate enforces the list.
pub const RESERVED_MODELS: [&str; 4] = [
    "record_change",
    "storage_event",
    "storage_index",
    "data_guard",
];

/// Most rows one `bulk_create` call takes (D70).
pub const MAX_BULK_ROWS: usize = 1000;

#[derive(Debug, Clone, Default)]
pub struct Module {
    pub id: String,
    /// Name shown in the app launcher.
    pub name: String,
    /// What the module does, in English (empty for modules that declare none).
    pub description: String,
    pub models: Vec<ModelDef>,
    /// Network destinations the module declares in `obop.toml` (D41).
    pub network_destinations: Vec<String>,
    /// Names and descriptions in other languages (D52). English stays the source.
    pub translations: Arc<Translations>,
    /// Version, author, licence, core requirement and the other catalogue data (D55).
    pub meta: ModuleMeta,
}

/// The categories a module may declare (D55).
pub const MODULE_CATEGORIES: [&str; 9] = [
    "finance",
    "sales",
    "supply-chain",
    "people",
    "services",
    "marketing",
    "productivity",
    "platform",
    "other",
];

/// How settled a module is (D55).
pub const MATURITIES: [&str; 3] = ["stable", "beta", "experimental"];

/// What a module says about itself besides its models: the `[module]` table of `obop.toml` for a
/// package, the same fields declared in code for a built-in module (D55). Empty strings mean
/// "not declared".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModuleMeta {
    /// Semantic version. Built-in modules version with the core.
    pub version: String,
    /// One line for the card, at most 140 characters.
    pub summary: String,
    pub author: String,
    pub authors: Vec<String>,
    /// SPDX expression, `LicenseRef-...`, `proprietary` or `UNLICENSED`.
    pub license: String,
    pub homepage: String,
    pub repository: String,
    pub documentation: String,
    pub support: String,
    pub keywords: Vec<String>,
    pub categories: Vec<String>,
    /// A built-in icon name; an unknown one falls back to a default.
    pub icon: String,
    /// Semver requirement on the core version, for example `^0.2`.
    pub requires_core: String,
    /// `stable`, `beta` or `experimental`.
    pub maturity: String,
    /// Module id and semver requirement, in declaration order.
    pub dependencies: Vec<(String, String)>,
}

impl ModuleMeta {
    /// The metadata of a first-party built-in module: versioned with the core, licensed as the
    /// core is (D1), requiring exactly the core series it ships with.
    pub fn first_party(
        summary: &str,
        categories: &[&str],
        keywords: &[&str],
        icon: &str,
        dependencies: &[&str],
    ) -> Self {
        let v = obop_types::CORE_VERSION;
        let mut parts = v.split('.');
        let (major, minor) = (parts.next().unwrap_or("0"), parts.next().unwrap_or("0"));
        Self {
            version: v.to_string(),
            summary: summary.to_string(),
            author: "SpaceCorps".into(),
            authors: Vec::new(),
            license: "AGPL-3.0-or-later".into(),
            homepage: "https://github.com/SpaceCorps/obop".into(),
            repository: "https://github.com/SpaceCorps/obop".into(),
            documentation: String::new(),
            support: String::new(),
            keywords: keywords.iter().map(|x| x.to_string()).collect(),
            categories: categories.iter().map(|x| x.to_string()).collect(),
            icon: icon.to_string(),
            requires_core: format!("^{major}.{minor}"),
            maturity: "stable".into(),
            // Built-in modules version with the core, so each supports the series it ships with.
            dependencies: dependencies
                .iter()
                .map(|d| (d.to_string(), format!("^{major}.{minor}")))
                .collect(),
        }
    }
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
                    .chain(m.access.iter().flat_map(|a| a.see_all.iter().copied()))
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
    let mut v = check_basics(m);
    // Rules (D56 to D58). Effects on models outside the module cannot be resolved here; the
    // install checks them against the dependencies (`check_module_in`).
    v.extend(rules::check_rules(m, &|_| None, false));
    v
}

/// Like `check_module`, with `lookup` finding the models of the dependencies, so an effect on a
/// model outside the module is checked too, and an unknown one is a violation.
pub fn check_module_in(
    m: &Module,
    lookup: &dyn Fn(&str) -> Option<Arc<ModelDef>>,
) -> Vec<Violation> {
    let mut v = check_basics(m);
    v.extend(rules::check_rules(m, lookup, true));
    v
}

fn check_basics(m: &Module) -> Vec<Violation> {
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
            if f.name == "id" {
                bad(
                    fp.clone(),
                    "a field cannot be named `id`; it is the record id",
                );
            }
            match &f.kind {
                FieldKind::Select(o) if o.is_empty() => {
                    bad(fp.clone(), "select field has no options")
                }
                FieldKind::Select(o) => {
                    let mut seen = HashSet::new();
                    if o.iter().any(|x| x.is_empty() || !seen.insert(*x)) {
                        bad(fp.clone(), "select options must be non-empty and unique");
                    }
                }
                FieldKind::Ref(t) if t.trim().is_empty() => {
                    bad(fp.clone(), "ref field has no target model")
                }
                FieldKind::Decimal {
                    precision, scale, ..
                } if !(1..=MAX_PRECISION).contains(precision)
                    || *scale > MAX_SCALE
                    || scale > precision =>
                {
                    bad(
                        fp.clone(),
                        "a decimal needs a precision of 1 to 38 and a scale of 0 to 12, not above the precision",
                    )
                }
                _ => {}
            }
        }
        let mut command_names = HashSet::new();
        for c in &model.commands {
            if GENERATED_COMMANDS.contains(&c.name) || !command_names.insert(c.name) {
                bad(
                    format!("{mp}::{}", c.name),
                    "command name is generated or declared twice",
                );
            }
            if let Some(t) = c.transition {
                match model.fields.iter().find(|f| f.name == t.field) {
                    Some(f) if matches!(f.kind, FieldKind::Select(_)) => {
                        let o = f.kind.options();
                        if t.from.iter().chain([&t.to]).any(|v| !o.contains(v)) {
                            bad(
                                format!("{mp}::{}", c.name),
                                "transition uses a value that is not an option of the field",
                            );
                        }
                    }
                    _ => bad(
                        format!("{mp}::{}", c.name),
                        "transition must act on a declared select field",
                    ),
                }
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
            kind: FieldKind::Text,
            required: false,
            computed: None,
            unique: None,
            sequence: None,
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
                access: None,
                fields: vec![f],
                commands: vec![],
                ext: Default::default(),
            }],
            network_destinations: vec![],
            description: String::new(),
            translations: Default::default(),
            meta: Default::default(),
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
            transition: None,
            effects: vec![],
            asserts: vec![],
        });
        assert_eq!(check_module(&m).len(), 1);
    }

    fn gate(f: FieldDef) -> Vec<Violation> {
        check_module(&module(f))
    }

    fn ok_field() -> FieldDef {
        field(Some(DataClass::Internal), true)
    }

    #[test]
    fn typed_fields_pass_when_well_formed() {
        assert!(gate(ok_field().select(&["a", "b"]).required()).is_empty());
        assert!(gate(ok_field().reference("party")).is_empty());
    }

    #[test]
    fn select_needs_unique_options() {
        assert!(
            gate(ok_field().select(&[]))[0]
                .problem
                .contains("no options")
        );
        assert!(
            gate(ok_field().select(&["a", "a"]))[0]
                .problem
                .contains("unique")
        );
    }

    #[test]
    fn ref_needs_a_target() {
        assert!(gate(ok_field().reference(""))[0].problem.contains("target"));
    }

    #[test]
    fn generated_commands_include_search_and_an_approval_gated_delete() {
        let m = module(ok_field());
        let cmds = m.models[0].all_commands();
        let names: Vec<_> = cmds.iter().map(|c| c.name).collect();
        assert_eq!(
            names,
            [
                "get",
                "list",
                "search",
                "create",
                "bulk_create",
                "update",
                "delete"
            ]
        );
        assert!(
            cmds.iter()
                .find(|c| c.name == "delete")
                .unwrap()
                .needs_approval
        );
        assert!(
            cmds.iter()
                .filter(|c| c.name != "delete")
                .all(|c| !c.needs_approval)
        );
    }

    #[test]
    fn transitions_make_governed_fields_and_are_gated() {
        let mut m = module(ok_field().select(&["a", "b", "c"]));
        m.models[0].fields[0].name = "state";
        m.models[0].commands = vec![
            CommandDef::new("go", "Go", "g").transition("state", Some("a"), "b"),
            CommandDef::new("stop", "Stop", "g").transition("state", None, "c"),
        ];
        assert!(check_module(&m).is_empty());
        assert_eq!(m.models[0].governed_fields(), ["state"]);
        assert_eq!(m.models[0].transitions().count(), 2);
        // Off-option values, a missing field and a non-select field are gate violations.
        m.models[0].commands[0] =
            CommandDef::new("go", "Go", "g").transition("state", Some("a"), "z");
        assert!(check_module(&m)[0].problem.contains("not an option"));
        m.models[0].commands[0] = CommandDef::new("go", "Go", "g").transition("ghost", None, "b");
        assert!(
            check_module(&m)[0]
                .problem
                .contains("declared select field")
        );
        let mut text = module(ok_field());
        text.models[0].commands =
            vec![CommandDef::new("go", "Go", "g").transition("email", None, "b")];
        assert!(
            check_module(&text)[0]
                .problem
                .contains("declared select field")
        );
    }

    #[test]
    fn custom_commands_cannot_reuse_generated_names_or_each_other() {
        let mut m = module(ok_field());
        m.models[0].commands = vec![CommandDef::new("delete", "x", "g")];
        assert!(
            check_module(&m)[0]
                .problem
                .contains("generated or declared twice")
        );
        m.models[0].commands = vec![
            CommandDef::new("a", "x", "g"),
            CommandDef::new("a", "x", "g"),
        ];
        assert!(
            check_module(&m)[0]
                .problem
                .contains("generated or declared twice")
        );
    }

    #[test]
    fn a_field_cannot_be_named_id() {
        let mut f = ok_field();
        f.name = "id";
        assert!(gate(f)[0].problem.contains("`id`"));
    }

    #[test]
    fn the_reserved_names_are_exactly_these() {
        assert_eq!(
            SYSTEM_FIELDS,
            [
                "created_at",
                "updated_at",
                "created_by",
                "updated_by",
                "row_version",
                "sys_backfilled"
            ]
        );
        // The system fields in the same order, then the record id and the optimistic-lock input.
        assert_eq!(RESERVED_FIELDS[..6], SYSTEM_FIELDS);
        assert_eq!(RESERVED_FIELDS[6..], ["id", "if_version"]);
        assert_eq!(
            RESERVED_MODELS,
            [
                "record_change",
                "storage_event",
                "storage_index",
                "data_guard"
            ]
        );
        // No name appears twice in a list.
        for list in [
            &SYSTEM_FIELDS[..],
            &RESERVED_FIELDS[..],
            &RESERVED_MODELS[..],
        ] {
            let set: HashSet<_> = list.iter().collect();
            assert_eq!(set.len(), list.len());
        }
        // The generated commands do not change until the history command lands (D75).
        assert_eq!(GENERATED_COMMANDS.len(), 7);
        // The gate refuses `id` already and does not yet refuse the other reserved names.
        assert!(RESERVED_FIELDS.contains(&"id"));
        let mut f = ok_field();
        f.name = "created_at";
        assert!(gate(f).is_empty());
    }

    #[test]
    fn a_model_has_no_extensions_unless_it_declares_them() {
        let m = ModelExt::default();
        assert_eq!(m, ModelExt::default());
        assert_eq!(m.history, History::default());
        assert_eq!(m.history.off_reason, None);
        assert_eq!(m.history.retention_days, None);
        assert!(m.indexes.is_empty());
        assert!(m.guard.is_none());
        // A guard compares by value, so a gate or a diff can tell two declarations apart.
        let guard = |v: &'static str| Guard {
            immutable_when: Some(StateSet {
                field: "state",
                values: vec![v],
            }),
            mutable_fields: vec![],
            exit_to: vec![Exit {
                to: "draft",
                action: "reopen",
                may_change: vec![],
            }],
            no_delete_when: None,
            lock_date: Some(LockDate {
                field: "date",
                setting: "accounting.lock_date",
            }),
            children_of: None,
            append_only: false,
        };
        assert_eq!(guard("posted"), guard("posted"));
        assert_ne!(guard("posted"), guard("paid"));
        let index = IndexDef {
            cols: vec![("driver", false), ("day", true)],
            kind: IndexKind::Btree,
            filter: vec![IndexCond {
                field: "state",
                op: CondOp::Ne,
                values: vec![Lit::Text("cancelled".into())],
            }],
        };
        assert_eq!(index.clone(), index);
    }

    #[test]
    fn kind_names_and_descriptions() {
        assert_eq!(FieldKind::Money.name(), "money");
        assert_eq!(FieldKind::default(), FieldKind::Text);
        let f = ok_field().select(&["x", "y"]);
        assert_eq!(f.kind.describe(), "one of: x, y");
        assert_eq!(f.kind.options(), ["x", "y"]);
        assert_eq!(ok_field().reference("party").kind.target(), Some("party"));
        assert!(FieldKind::is_known("longtext") && !FieldKind::is_known("float"));
    }

    #[test]
    fn decimal_and_datetime_kinds_describe_themselves() {
        assert!(FieldKind::is_known("decimal") && FieldKind::is_known("datetime"));
        assert_eq!(FieldKind::Datetime.name(), "datetime");
        let d = FieldKind::decimal(10, 2, None).unwrap();
        assert_eq!(d.name(), "decimal");
        assert_eq!(d.decimal_shape(), Some((10, 2, None)));
        assert!(d.describe().contains("\"12.33\"") && d.describe().contains("not a JSON number"));
        assert_eq!(
            d.decimal_pattern().unwrap(),
            "^-?(0|[1-9][0-9]{0,7})(\\.[0-9]{1,2})?$"
        );
        assert_eq!(
            FieldKind::decimal(2, 2, None)
                .unwrap()
                .decimal_pattern()
                .unwrap(),
            "^-?0(\\.[0-9]{1,2})?$"
        );
        assert_eq!(
            FieldKind::decimal(5, 0, None)
                .unwrap()
                .decimal_pattern()
                .unwrap(),
            "^-?(0|[1-9][0-9]{0,4})$"
        );
        // The example in the description is a value of the shape, in every shape (it used to
        // be `12.` for a whole number and `12.33` where only one digit is left of the point).
        for (p, s, example) in [
            (5, 0, "12"),
            (1, 0, "1"),
            (2, 2, "0.33"),
            (3, 2, "1.33"),
            (10, 2, "12.33"),
            (38, 12, "12.333333333333"),
        ] {
            let k = FieldKind::decimal(p, s, None).unwrap();
            assert!(
                k.describe().contains(&format!("\"{example}\"")),
                "decimal({p}, {s}): {}",
                k.describe()
            );
            let parsed = obop_values::Decimal::parse_for(example, p, s, None)
                .unwrap_or_else(|e| panic!("decimal({p}, {s}) example {example}: {e}"));
            assert_eq!(parsed.to_string(), example);
        }
        for (p, s) in [(0, 0), (39, 2), (10, 13), (2, 3)] {
            assert!(FieldKind::decimal(p, s, None).is_none(), "{p},{s}");
        }
        assert!(FieldKind::Datetime.describe().contains("RFC 3339"));
        // The gate refuses a decimal declared outside its limits and takes a good one.
        assert!(gate(ok_field().decimal(38, 12)).is_empty());
        assert!(gate(ok_field().datetime()).is_empty());
        assert!(
            gate(ok_field().decimal(39, 2))[0]
                .problem
                .contains("precision of 1 to 38")
        );
        assert!(
            gate(ok_field().decimal(10, 13))[0]
                .problem
                .contains("scale of 0 to 12")
        );
    }

    #[test]
    fn dates_are_checked_against_the_calendar() {
        for ok in ["2024-02-29", "2000-02-29", "2023-12-31", "0001-01-01"] {
            assert!(is_valid_date(ok), "{ok}");
        }
        for bad in [
            "2023-02-29",
            "1900-02-29",
            "2024-13-01",
            "2024-00-10",
            "2024-04-31",
            "2024-1-01",
            "2024-01-001",
            "20240101",
            "2024/01/01",
            "+024-01-01",
            "0000-01-01",
            "",
        ] {
            assert!(!is_valid_date(bad), "{bad}");
        }
    }
}
