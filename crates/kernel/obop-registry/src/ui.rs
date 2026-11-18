//! UI hints in the package format (D95, PROPOSED): what a module says about how its records are
//! shown and filled in, besides what they are.
//!
//! D95 is a deliberate lite subset of `docs/spec/ux.md` 4.3 Layer 1 ("package views, lite"). The
//! hints are served in `/api/meta` (and the MCP `obop__meta` tool) under `ui` keys, so a later view
//! resolver can replace them without changing that contract.
//!
//! Two kinds of hint live here:
//!
//! - **Display hints** (`ModelUi`, `FieldUi`, `ActionUi`, `ModuleUi`): the server only checks and
//!   serves them; the back office reads them (title templates, list columns, menus, tones).
//! - **Rules the server enforces** (`FieldHints::default`, `fill_from`, `readonly`, `ref_filter`,
//!   `ActionUi::needs`, `ActionUi::not_by`): the command layer applies them on every surface, so
//!   the CLI, REST and MCP get exactly what the interface gets (OR-4). They are declared with the
//!   display hints because the interface also shows them (a prefilled form, a read-only field, a
//!   narrowed picker, a field asked for before an action runs).
//!
//! This module holds the types, the parsers of the small value languages (defaults, paths,
//! filters), the builder methods for modules written in Rust and the install gate of every key.

use crate::labels::text_ok;
use crate::{CommandDef, FieldDef, FieldKind, ModelDef, Module, ModuleMeta, Violation};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// `[model.ui] default_view`.
pub const VIEW_HINTS: [&str; 9] = [
    "list", "kanban", "calendar", "pivot", "graph", "gallery", "grid", "timeline", "tree",
];
/// `[model.ui] menu`.
pub const MENU_PLACES: [&str; 4] = ["main", "configuration", "reporting", "hidden"];
/// `[model.field.ui] display`.
pub const DISPLAYS: [&str; 11] = [
    "time_of_day",
    "duration_minutes",
    "percent",
    "percent_bp",
    "km_from_m",
    "epoch_seconds",
    "markdown",
    "tel",
    "email",
    "url",
    "address",
];
/// `[model.field.ui] tones` values.
pub const TONES: [&str; 5] = ["ok", "bad", "warn", "info", "neutral"];
/// `[model.field.ui] aggregate`.
pub const AGGREGATES: [&str; 3] = ["sum", "avg", "none"];
/// `[[model.child]] shape`.
pub const CHILD_SHAPES: [&str; 4] = ["lines", "list", "images", "translations"];

/// The most days `today+N` may add.
pub const MAX_TODAY_OFFSET: u32 = 3660;
/// The longest template, path or filter text.
pub const MAX_HINT_TEXT: usize = 200;

// ---- the module -----------------------------------------------------------------------------

/// `[module] home` and `launcher`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModuleUi {
    /// Where the app opens: a model of the module, or `overview`.
    pub home: Option<String>,
    /// `Some(false)`: the app is not on the launcher (it lives under another app).
    pub launcher: Option<bool>,
}

// ---- the model ------------------------------------------------------------------------------

/// `[model.ui] calendar`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CalendarUi {
    pub start: String,
    pub end: Option<String>,
    pub color: Option<String>,
}

/// `[model.ui] report`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReportUi {
    pub rows: Option<String>,
    pub cols: Option<String>,
    pub measure: Option<String>,
}

/// `[[model.ui.section]]`: a titled group of fields in the form.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UiSection {
    pub label: String,
    pub fields: Vec<String>,
    pub collapsed: bool,
    /// Shown only while the record's state is one of these (empty: always).
    pub show_in: Vec<String>,
}

/// `[[model.ui.filter]]`: a saved filter the control panel offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiFilter {
    pub label: String,
    pub filter: Filter,
}

/// `[[model.child]]`: a model drawn inside this model's form.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChildDecl {
    pub model: String,
    /// The child's ref field that points at this model.
    pub via: String,
    /// `lines`, `list`, `images` or `translations`; `None`: the interface decides.
    pub shape: Option<String>,
}

/// `[model.ui]` and `[[model.child]]`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelUi {
    /// A template such as `{code} {name}`; `|` separates alternatives, the first whose
    /// placeholders are all set wins; `{a.b}` follows the ref `a` to the field `b`.
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub list: Vec<String>,
    pub card: Vec<String>,
    pub card_sum: Option<String>,
    pub quick_create: Vec<String>,
    /// `date desc, number desc`.
    pub order: Option<String>,
    pub default_view: Option<String>,
    pub menu: Option<String>,
    pub menu_section: Option<String>,
    pub menu_order: Option<i64>,
    pub calendar: Option<CalendarUi>,
    pub report: Option<ReportUi>,
    pub sections: Vec<UiSection>,
    pub filters: Vec<UiFilter>,
    /// The label of one of `filters` that a list applies when it is opened with no search of its
    /// own (R5); the person can remove it like any filter. A hint only.
    pub default_filter: Option<String>,
    /// Where a record is seen on the public site: `/blog/{slug}`.
    pub public_url: Option<String>,
    /// Declared children, added to those the `resolve_child` rule finds.
    pub children: Vec<ChildDecl>,
}

impl ModelUi {
    /// Whether nothing is declared (no `ui` key is served then).
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

// ---- the field ------------------------------------------------------------------------------

/// `[model.field.ui]`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FieldUi {
    pub display: Option<String>,
    pub aggregate: Option<String>,
    /// Option value -> tone.
    pub tones: BTreeMap<String, String>,
    pub warn_before_days: Option<u32>,
    pub hidden: bool,
    /// `journal_id.currency` or `currency`: where a money field's currency comes from.
    pub currency_from: Option<String>,
    /// A condition on the saved record (the part of the expression language the browser evaluates,
    /// `expr::check_display_subset`) while which the form shows the field read-only (R6). A hint
    /// only: the server answers as it always did, and the rule it holds is the module's own.
    pub readonly_when: Option<String>,
}

impl FieldUi {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

/// A literal default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefaultLit {
    Text(String),
    Int(i64),
    Bool(bool),
}

/// `default` of a field: what the server fills in on create when the field is missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefaultSpec {
    Literal(DefaultLit),
    /// `today` (0) or `today+N`: the date in the platform zone, plus N days.
    Today(u32),
    /// `$user`: the person creating the record (for an agent, the person it acts for).
    User,
    /// `$instance.currency`: the currency of the instance (`OBOP_CURRENCY`, default EUR).
    InstanceCurrency,
    /// `$open:<model>`: the one record of `<model>` in the state `open` the caller can see.
    Open(String),
}

impl DefaultSpec {
    /// Read a default as written: a string (with the special forms), a whole number or a bool.
    pub fn parse_text(s: &str) -> Result<Self, String> {
        if let Some(lit) = s.strip_prefix("$$") {
            return Ok(Self::Literal(DefaultLit::Text(format!("${lit}"))));
        }
        if s == "today" {
            return Ok(Self::Today(0));
        }
        if let Some(n) = s.strip_prefix("today+") {
            return match n.parse::<u32>() {
                Ok(n) if (1..=MAX_TODAY_OFFSET).contains(&n) && !n.to_string().is_empty() => {
                    Ok(Self::Today(n))
                }
                _ => Err(format!(
                    "{s:?}: today+N needs N from 1 to {MAX_TODAY_OFFSET} days"
                )),
            };
        }
        match s {
            "$user" => return Ok(Self::User),
            "$instance.currency" => return Ok(Self::InstanceCurrency),
            _ => {}
        }
        if let Some(m) = s.strip_prefix("$open:") {
            if !name_ok(m) {
                return Err(format!("{s:?}: $open: needs a model name"));
            }
            return Ok(Self::Open(m.to_string()));
        }
        if s.starts_with('$') {
            return Err(format!(
                "{s:?} is not a default (use a literal, today, today+N, $user, $instance.currency or $open:<model>; $$ starts a literal dollar sign)"
            ));
        }
        Ok(Self::Literal(DefaultLit::Text(s.to_string())))
    }

    /// The default as written, the way `/api/meta` serves it.
    pub fn written(&self) -> DefaultLit {
        match self {
            Self::Literal(DefaultLit::Text(t)) if t.starts_with('$') => {
                DefaultLit::Text(format!("${t}"))
            }
            Self::Literal(l) => l.clone(),
            Self::Today(0) => DefaultLit::Text("today".into()),
            Self::Today(n) => DefaultLit::Text(format!("today+{n}")),
            Self::User => DefaultLit::Text("$user".into()),
            Self::InstanceCurrency => DefaultLit::Text("$instance.currency".into()),
            Self::Open(m) => DefaultLit::Text(format!("$open:{m}")),
        }
    }
}

/// A path to a field: `field` of this record, or `ref.field` of the record a ref points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    /// The ref field of this record; `None` for a field of the record itself.
    pub via: Option<String>,
    pub field: String,
}

impl Path {
    pub fn parse(s: &str) -> Result<Self, String> {
        let (via, field) = match s.split_once('.') {
            Some((a, b)) => (Some(a), b),
            None => (None, s),
        };
        if via.is_some_and(|v| !name_ok(v)) || !name_ok(field) {
            return Err(format!(
                "{s:?} must be a field name or ref.field, for example \"product_id.list_price_minor\""
            ));
        }
        Ok(Self {
            via: via.map(String::from),
            field: field.to_string(),
        })
    }

    pub fn written(&self) -> String {
        match &self.via {
            Some(v) => format!("{v}.{}", self.field),
            None => self.field.clone(),
        }
    }
}

/// How a filter compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl CmpOp {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Eq => "=",
            Self::Ne => "!=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
        }
    }
}

/// The right-hand side of a filter clause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterValue {
    Null,
    Bool(bool),
    Int(i64),
    Text(String),
    /// `$field`: a field of the record being written (only in a `ref_filter`).
    Field(String),
    /// `$user`: the caller's user id.
    User,
    /// `today` or `today+N`.
    Today(u32),
}

/// One clause: `field op value`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clause {
    pub field: String,
    pub op: CmpOp,
    pub value: FilterValue,
}

/// A conjunction of clauses: `active = true && project_id = $project_id`. Used by `ref_filter` and
/// by `[[model.ui.filter]] where`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    pub source: String,
    pub clauses: Vec<Clause>,
}

impl Filter {
    pub fn parse(src: &str) -> Result<Self, String> {
        let source = src.trim().to_string();
        if source.is_empty() || source.chars().count() > MAX_HINT_TEXT {
            return Err(format!(
                "a filter is 1 to {MAX_HINT_TEXT} characters, such as \"active = true && project_id = $project_id\""
            ));
        }
        let mut clauses = Vec::new();
        for part in source.split("&&") {
            clauses.push(Self::clause(part.trim()).map_err(|e| format!("{source:?}: {e}"))?);
        }
        if clauses.len() > 8 {
            return Err(format!("{source:?}: at most 8 clauses"));
        }
        Ok(Self { source, clauses })
    }

    fn clause(c: &str) -> Result<Clause, String> {
        let at = c
            .find(['=', '!', '<', '>'])
            .ok_or_else(|| format!("{c:?} needs a comparison (=, !=, <, <=, >, >=)"))?;
        let field = c[..at].trim();
        let rest = &c[at..];
        let (op, len) = if rest.starts_with("!=") {
            (CmpOp::Ne, 2)
        } else if rest.starts_with("<=") {
            (CmpOp::Le, 2)
        } else if rest.starts_with(">=") {
            (CmpOp::Ge, 2)
        } else if rest.starts_with("==") {
            (CmpOp::Eq, 2)
        } else if rest.starts_with('=') {
            (CmpOp::Eq, 1)
        } else if rest.starts_with('<') {
            (CmpOp::Lt, 1)
        } else if rest.starts_with('>') {
            (CmpOp::Gt, 1)
        } else {
            return Err(format!("{c:?}: unknown comparison"));
        };
        if !name_ok(field) {
            return Err(format!("{field:?} is not a field name"));
        }
        let v = rest[len..].trim();
        let value = match v {
            "true" => FilterValue::Bool(true),
            "false" => FilterValue::Bool(false),
            "null" => FilterValue::Null,
            "$user" => FilterValue::User,
            "today" => FilterValue::Today(0),
            _ => {
                if let Some(n) = v.strip_prefix("today+") {
                    match n.parse::<u32>() {
                        Ok(n) if (1..=MAX_TODAY_OFFSET).contains(&n) => FilterValue::Today(n),
                        _ => return Err(format!("{v:?}: today+N needs N from 1 to 3660")),
                    }
                } else if let Some(f) = v.strip_prefix('$') {
                    if !name_ok(f) {
                        return Err(format!("{v:?} is not a $field"));
                    }
                    FilterValue::Field(f.to_string())
                } else if let Some(t) = v.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
                    if t.contains('"') {
                        return Err(format!("{v:?}: a quoted text cannot hold a quote"));
                    }
                    FilterValue::Text(t.to_string())
                } else if let Ok(n) = v.parse::<i64>() {
                    FilterValue::Int(n)
                } else if !v.is_empty()
                    && v.chars()
                        .all(|c| c.is_ascii_alphanumeric() || "_-.:".contains(c))
                {
                    FilterValue::Text(v.to_string())
                } else {
                    return Err(format!(
                        "{v:?} is not a value (true, false, null, a whole number, \"text\", a word, today, today+N, $user or $field)"
                    ));
                }
            }
        };
        if matches!(value, FilterValue::Null | FilterValue::Bool(_))
            && !matches!(op, CmpOp::Eq | CmpOp::Ne)
        {
            return Err(format!(
                "{c:?}: true, false and null compare with = or != only"
            ));
        }
        Ok(Clause {
            field: field.to_string(),
            op,
            value,
        })
    }

    /// The `$field` names the filter reads from the record being written.
    pub fn self_fields(&self) -> Vec<&str> {
        self.clauses
            .iter()
            .filter_map(|c| match &c.value {
                FilterValue::Field(f) => Some(f.as_str()),
                _ => None,
            })
            .collect()
    }
}

/// What a field declares besides its kind: the D95 hints.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FieldHints {
    pub default: Option<DefaultSpec>,
    /// `ref.field`: copied on create from the record the ref points at, when the field is missing.
    pub fill_from: Option<Path>,
    /// Set by the server only: refused on create and update (code `readonly_field`).
    pub readonly: bool,
    /// What a record named by this ref field must satisfy (checked on write).
    pub ref_filter: Option<Filter>,
    pub ui: FieldUi,
}

impl FieldHints {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

// ---- the action -----------------------------------------------------------------------------

/// `[model.action.ui]`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActionUi {
    /// The main button of the record in the states this action leaves.
    pub primary: bool,
    /// Goes to the overflow menu behind a confirm.
    pub danger: bool,
    /// The question asked before it runs.
    pub confirm: Option<String>,
    /// The note shown after it ran.
    pub done: Option<String>,
    /// Actions with the same group show as one entry.
    pub group: Option<String>,
    /// Fields the action input may carry: written in the same unit before the assertions, and
    /// required (in the input or already on the record) for the action to run.
    pub needs: Vec<String>,
    /// A text field holding a user id: the person named there may not run the action (a
    /// reviewer cannot confirm their own trip).
    pub not_by: Option<String>,
}

impl ActionUi {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

fn name_ok(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|f| f.is_ascii_lowercase())
        && s.len() <= 40
        && c.all(|x| x.is_ascii_lowercase() || x.is_ascii_digit() || x == '_')
}

/// The placeholders of a template (`{f}`, `{ref.f}`), or why it does not read.
pub fn template_paths(t: &str) -> Result<Vec<Path>, String> {
    if t.trim().is_empty() || t.chars().count() > MAX_HINT_TEXT {
        return Err(format!("a template is 1 to {MAX_HINT_TEXT} characters"));
    }
    if t.chars().any(|c| c.is_control() || c == '<' || c == '>') {
        return Err(format!("{t:?}: no control characters or markup"));
    }
    let mut out = Vec::new();
    let mut rest = t;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let close = after
            .find('}')
            .ok_or_else(|| format!("{t:?}: `{{` is not closed"))?;
        out.push(Path::parse(&after[..close]).map_err(|e| format!("{t:?}: {e}"))?);
        rest = &after[close + 1..];
    }
    if rest.contains('}') {
        return Err(format!("{t:?}: `}}` without `{{`"));
    }
    Ok(out)
}

// ---- builders for modules written in Rust ---------------------------------------------------

impl FieldDef {
    fn hints_mut(&mut self) -> &mut FieldHints {
        Arc::make_mut(self.hints.get_or_insert_with(Default::default))
    }

    /// `default`: a literal or one of `today`, `today+N`, `$user`, `$instance.currency`,
    /// `$open:<model>`. Panics on a malformed one (a programming error in a built-in module).
    pub fn default_value(mut self, spec: &str) -> Self {
        let d = DefaultSpec::parse_text(spec)
            .unwrap_or_else(|e| panic!("{}: bad default: {e}", self.name));
        self.hints_mut().default = Some(d);
        self
    }

    /// A whole-number default.
    pub fn default_int(mut self, n: i64) -> Self {
        self.hints_mut().default = Some(DefaultSpec::Literal(DefaultLit::Int(n)));
        self
    }

    /// A true/false default.
    pub fn default_bool(mut self, b: bool) -> Self {
        self.hints_mut().default = Some(DefaultSpec::Literal(DefaultLit::Bool(b)));
        self
    }

    /// `fill_from = "ref.field"`.
    pub fn fill_from(mut self, path: &str) -> Self {
        let p = Path::parse(path).unwrap_or_else(|e| panic!("{}: bad fill_from: {e}", self.name));
        self.hints_mut().fill_from = Some(p);
        self
    }

    /// `readonly = true`: set by the server only.
    pub fn readonly(mut self) -> Self {
        self.hints_mut().readonly = true;
        self
    }

    /// `ref_filter`: what a referenced record must satisfy.
    pub fn ref_filter(mut self, filter: &str) -> Self {
        let f =
            Filter::parse(filter).unwrap_or_else(|e| panic!("{}: bad ref_filter: {e}", self.name));
        self.hints_mut().ref_filter = Some(f);
        self
    }

    /// `[model.field.ui] display`.
    pub fn display(mut self, display: &str) -> Self {
        self.hints_mut().ui.display = Some(display.to_string());
        self
    }

    /// `[model.field.ui] aggregate`: `sum`, `avg` or `none`.
    pub fn aggregate(mut self, how: &str) -> Self {
        self.hints_mut().ui.aggregate = Some(how.to_string());
        self
    }

    /// `[model.field.ui] tones`: option value and tone pairs.
    pub fn tones(mut self, tones: &[(&str, &str)]) -> Self {
        self.hints_mut().ui.tones = tones
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        self
    }

    /// `[model.field.ui] warn_before_days`.
    pub fn warn_before_days(mut self, days: u32) -> Self {
        self.hints_mut().ui.warn_before_days = Some(days);
        self
    }

    /// `[model.field.ui] hidden = true`.
    pub fn hidden(mut self) -> Self {
        self.hints_mut().ui.hidden = true;
        self
    }

    /// `[model.field.ui] currency_from`.
    pub fn currency_from(mut self, path: &str) -> Self {
        self.hints_mut().ui.currency_from = Some(path.to_string());
        self
    }

    /// `[model.field.ui] readonly_when`: the form shows the field read-only while `source` holds
    /// on the saved record (a new record is never held by it).
    pub fn readonly_when(mut self, source: &str) -> Self {
        self.hints_mut().ui.readonly_when = Some(source.trim().to_string());
        self
    }

    /// Whether the server alone sets the field (`readonly`).
    pub fn is_readonly(&self) -> bool {
        self.hints.as_ref().is_some_and(|h| h.readonly)
    }
}

impl CommandDef {
    /// `[model.action.ui]` as a whole.
    pub fn ui(mut self, ui: ActionUi) -> Self {
        self.ui = (!ui.is_empty()).then(|| Arc::new(ui));
        self
    }

    fn ui_mut(&mut self) -> &mut ActionUi {
        Arc::make_mut(self.ui.get_or_insert_with(Default::default))
    }

    /// `primary = true`.
    pub fn primary(mut self) -> Self {
        self.ui_mut().primary = true;
        self
    }

    /// `danger = true`.
    pub fn danger(mut self) -> Self {
        self.ui_mut().danger = true;
        self
    }

    /// `confirm = "..."`.
    pub fn confirm(mut self, question: &str) -> Self {
        self.ui_mut().confirm = Some(question.to_string());
        self
    }

    /// `done = "..."`.
    pub fn done(mut self, note: &str) -> Self {
        self.ui_mut().done = Some(note.to_string());
        self
    }

    /// `group = "..."`.
    pub fn in_group(mut self, group: &str) -> Self {
        self.ui_mut().group = Some(group.to_string());
        self
    }

    /// `needs = [...]`.
    pub fn needs(mut self, fields: &[&str]) -> Self {
        self.ui_mut().needs = fields.iter().map(|f| f.to_string()).collect();
        self
    }

    /// `not_by = "field"`.
    pub fn not_by(mut self, field: &str) -> Self {
        self.ui_mut().not_by = Some(field.to_string());
        self
    }
}

impl ModelDef {
    /// `[model.ui]` (and `[[model.child]]`) as a whole.
    pub fn with_ui(mut self, ui: ModelUi) -> Self {
        self.ext.ui = (!ui.is_empty()).then(|| Arc::new(ui));
        self
    }

    /// The `[model.ui]` hints, if any.
    pub fn ui(&self) -> Option<&ModelUi> {
        self.ext.ui.as_deref()
    }
}

impl ModuleMeta {
    /// `[module] home`.
    pub fn home(mut self, home: &str) -> Self {
        self.ui.home = Some(home.to_string());
        self
    }

    /// `[module] launcher = false`.
    pub fn off_launcher(mut self) -> Self {
        self.ui.launcher = Some(false);
        self
    }
}

// ---- the gate -------------------------------------------------------------------------------

type Lookup<'a> = &'a dyn Fn(&str) -> Option<Arc<ModelDef>>;

fn field<'a>(m: &'a ModelDef, name: &str) -> Option<&'a FieldDef> {
    m.fields.iter().find(|f| f.name == name)
}

fn is_numeric(k: &FieldKind) -> bool {
    matches!(
        k,
        FieldKind::Number | FieldKind::Money | FieldKind::Decimal { .. }
    )
}

fn is_text(k: &FieldKind) -> bool {
    matches!(k, FieldKind::Text | FieldKind::LongText)
}

/// Whether a value of kind `src` may be copied into a field of kind `dst` (`fill_from`).
fn copy_ok(dst: &FieldKind, src: &FieldKind) -> bool {
    match (dst, src) {
        (FieldKind::Text | FieldKind::LongText, s) => {
            is_text(s) || matches!(s, FieldKind::Select(_))
        }
        (FieldKind::Number | FieldKind::Money, FieldKind::Number | FieldKind::Money) => true,
        (FieldKind::Decimal { scale: a, .. }, FieldKind::Decimal { scale: b, .. }) => b <= a,
        (FieldKind::Select(a), FieldKind::Select(b)) => b.iter().all(|o| a.contains(o)),
        (FieldKind::Ref(a), FieldKind::Ref(b)) => a == b,
        (a, b) => a.name() == b.name() && !a.is_blob(),
    }
}

/// Whether the literal fits the kind.
fn literal_ok(kind: &FieldKind, l: &DefaultLit) -> bool {
    match (kind, l) {
        (FieldKind::Text | FieldKind::LongText, DefaultLit::Text(_)) => true,
        (FieldKind::Number | FieldKind::Money, DefaultLit::Int(_)) => true,
        (FieldKind::Bool, DefaultLit::Bool(_)) => true,
        (FieldKind::Date, DefaultLit::Text(t)) => crate::is_valid_date(t),
        (FieldKind::Select(o), DefaultLit::Text(t)) => o.contains(&t.as_str()),
        (
            FieldKind::Decimal {
                precision, scale, ..
            },
            DefaultLit::Text(t),
        ) => obop_values::Decimal::parse_for(t, *precision, *scale, None).is_ok(),
        (
            FieldKind::Decimal {
                precision, scale, ..
            },
            DefaultLit::Int(n),
        ) => obop_values::Decimal::parse_for(&n.to_string(), *precision, *scale, None).is_ok(),
        (FieldKind::Datetime, DefaultLit::Text(t)) => obop_values::Instant::parse(t).is_ok(),
        _ => false,
    }
}

/// The governed select field of `m` that has the option `open`, if any.
pub fn open_state_field(m: &ModelDef) -> Option<&'static str> {
    m.governed_fields()
        .into_iter()
        .find(|g| field(m, g).is_some_and(|f| f.kind.options().contains(&"open")))
}

struct Gate<'a> {
    module: &'a Module,
    lookup: Lookup<'a>,
    strict: bool,
    out: Vec<Violation>,
}

impl Gate<'_> {
    fn bad(&mut self, path: &str, problem: String) {
        self.out.push(Violation {
            path: path.to_string(),
            problem,
        });
    }

    /// The model a path of `m` ends in, and the field there; an error names what is wrong.
    /// `Ok(None)`: the ref points outside the module and the model is not known (not strict).
    fn resolve(&self, m: &ModelDef, p: &Path) -> Result<Option<FieldDef>, String> {
        match &p.via {
            None => field(m, &p.field)
                .cloned()
                .map(Some)
                .ok_or_else(|| format!("{} has no field {}", m.name, p.field)),
            Some(via) => {
                let r = field(m, via).ok_or_else(|| format!("{} has no field {via}", m.name))?;
                let Some(to) = r.kind.target() else {
                    return Err(format!("{}.{via} is not a ref field", m.name));
                };
                match (self.lookup)(to) {
                    Some(t) => field(&t, &p.field)
                        .cloned()
                        .map(Some)
                        .ok_or_else(|| format!("{to} has no field {}", p.field)),
                    None if self.strict => Err(format!("model {to} does not exist")),
                    None => Ok(None),
                }
            }
        }
    }

    fn fields_exist(&mut self, at: &str, m: &ModelDef, key: &str, names: &[String], max: usize) {
        if names.len() > max {
            self.bad(at, format!("{key} takes at most {max} fields"));
        }
        let mut seen = BTreeSet::new();
        for n in names {
            if field(m, n).is_none() {
                self.bad(at, format!("{key}: {} has no field {n}", m.name));
            } else if !seen.insert(n) {
                self.bad(at, format!("{key} lists {n} twice"));
            }
        }
    }

    fn one_of(&mut self, at: &str, key: &str, v: &Option<String>, allowed: &[&str]) {
        if let Some(v) = v
            && !allowed.contains(&v.as_str())
        {
            self.bad(
                at,
                format!("{key} must be one of {}, not {v:?}", allowed.join(", ")),
            );
        }
    }

    fn text(&mut self, at: &str, key: &str, v: &Option<String>, max: usize) {
        if let Some(v) = v
            && let Err(e) = text_ok(v, max)
        {
            self.bad(at, format!("{key} {e}"));
        }
    }

    fn template(&mut self, at: &str, m: &ModelDef, key: &str, t: &Option<String>) {
        let Some(t) = t else { return };
        match template_paths(t) {
            Err(e) => self.bad(at, format!("{key}: {e}")),
            Ok(paths) => {
                for p in paths {
                    if let Err(e) = self.resolve(m, &p) {
                        self.bad(at, format!("{key}: {{{}}}: {e}", p.written()));
                    }
                }
            }
        }
    }

    fn filter(
        &mut self,
        at: &str,
        key: &str,
        target: &ModelDef,
        this: Option<&ModelDef>,
        f: &Filter,
    ) {
        for c in &f.clauses {
            let Some(tf) = field(target, &c.field) else {
                self.bad(
                    at,
                    format!("{key}: {} has no field {}", target.name, c.field),
                );
                continue;
            };
            let ordered = !matches!(c.op, CmpOp::Eq | CmpOp::Ne);
            if ordered
                && !(is_numeric(&tf.kind)
                    || matches!(
                        tf.kind,
                        FieldKind::Date | FieldKind::Datetime | FieldKind::Text
                    ))
            {
                self.bad(
                    at,
                    format!(
                        "{key}: {}.{} cannot be compared with {}",
                        target.name,
                        c.field,
                        c.op.symbol()
                    ),
                );
            }
            match &c.value {
                FilterValue::Text(_)
                | FilterValue::Int(_)
                | FilterValue::Today(_)
                | FilterValue::User
                    if matches!(tf.kind, FieldKind::Bool) =>
                {
                    self.bad(
                        at,
                        format!(
                            "{key}: {}.{} is a true/false field; compare it with true or false",
                            target.name, c.field
                        ),
                    )
                }
                FilterValue::Bool(_) if !matches!(tf.kind, FieldKind::Bool) => self.bad(
                    at,
                    format!(
                        "{key}: {}.{} is not a true/false field",
                        target.name, c.field
                    ),
                ),
                FilterValue::Today(_) if !matches!(tf.kind, FieldKind::Date) => self.bad(
                    at,
                    format!(
                        "{key}: today compares with a date field, and {} is not one",
                        c.field
                    ),
                ),
                FilterValue::Text(t) if matches!(&tf.kind, FieldKind::Select(o) if !o.contains(&t.as_str())) => {
                    self.bad(
                        at,
                        format!(
                            "{key}: {t:?} is not an option of {}.{}",
                            target.name, c.field
                        ),
                    )
                }
                FilterValue::Field(sf) => match this {
                    None => self.bad(at, format!("{key}: ${sf} is only for a ref_filter")),
                    Some(this) if field(this, sf).is_none() => {
                        self.bad(at, format!("{key}: {} has no field {sf}", this.name))
                    }
                    Some(_) => {}
                },
                _ => {}
            }
        }
    }
}

/// Every D95 hint of a module, checked against its models (and, through `lookup`, the models of
/// its dependencies; `strict`: an unknown model is a violation).
pub fn check(m: &Module, lookup_ext: Lookup<'_>, strict: bool) -> Vec<Violation> {
    let own: Vec<Arc<ModelDef>> = m.models.iter().cloned().map(Arc::new).collect();
    let lookup = |n: &str| {
        own.iter()
            .find(|x| x.name == n)
            .cloned()
            .or_else(|| lookup_ext(n))
    };
    let mut g = Gate {
        module: m,
        lookup: &lookup,
        strict,
        out: Vec::new(),
    };
    if let Some(h) = &m.meta.ui.home
        && h != "overview"
        && !m.models.iter().any(|x| x.name == h)
    {
        g.bad(
            &m.id,
            format!("home must be \"overview\" or a model of the module, not {h:?}"),
        );
    }
    for model in &own {
        check_model(&mut g, model);
    }
    let _ = g.module;
    g.out
}

fn check_model(g: &mut Gate<'_>, model: &ModelDef) {
    let mp = format!("{}.{}", g.module.id, model.name);
    if let Some(ui) = model.ui() {
        let at = format!("{mp}.ui");
        g.template(&at, model, "title", &ui.title);
        g.template(&at, model, "subtitle", &ui.subtitle);
        g.fields_exist(&at, model, "list", &ui.list, 20);
        g.fields_exist(&at, model, "card", &ui.card, 12);
        g.fields_exist(&at, model, "quick_create", &ui.quick_create, 12);
        if let Some(s) = &ui.card_sum {
            match field(model, s) {
                Some(f) if is_numeric(&f.kind) => {}
                _ => g.bad(
                    &at,
                    format!(
                        "card_sum must be a number, money or decimal field of {}",
                        model.name
                    ),
                ),
            }
        }
        if let Some(o) = &ui.order {
            let parts: Vec<&str> = o.split(',').map(str::trim).collect();
            if parts.len() > 4 {
                g.bad(&at, "order takes at most 4 fields".into());
            }
            for p in parts {
                let mut w = p.split_whitespace();
                let (f, dir, extra) = (w.next().unwrap_or(""), w.next(), w.next());
                if field(model, f).is_none()
                    || extra.is_some()
                    || dir.is_some_and(|d| d != "asc" && d != "desc")
                {
                    g.bad(
                        &at,
                        format!(
                            "order: {p:?} must be a field of {} with asc or desc",
                            model.name
                        ),
                    );
                }
            }
        }
        g.one_of(&at, "default_view", &ui.default_view, &VIEW_HINTS);
        if let Some(d) = &ui.default_filter
            && !ui.filters.iter().any(|f| &f.label == d)
        {
            g.bad(
                &at,
                format!("default_filter {d:?} must be the label of one of the model's [[model.ui.filter]]"),
            );
        }
        g.one_of(&at, "menu", &ui.menu, &MENU_PLACES);
        g.text(&at, "menu_section", &ui.menu_section, 40);
        if ui.menu_order.is_some_and(|n| !(-1000..=1000).contains(&n)) {
            g.bad(&at, "menu_order must be from -1000 to 1000".into());
        }
        if let Some(c) = &ui.calendar {
            for (k, v) in [("start", Some(&c.start)), ("end", c.end.as_ref())] {
                if let Some(v) = v
                    && !field(model, v)
                        .is_some_and(|f| matches!(f.kind, FieldKind::Date | FieldKind::Datetime))
                {
                    g.bad(
                        &at,
                        format!(
                            "calendar.{k} must be a date or datetime field of {}",
                            model.name
                        ),
                    );
                }
            }
            if let Some(v) = &c.color
                && !field(model, v)
                    .is_some_and(|f| matches!(f.kind, FieldKind::Select(_) | FieldKind::Ref(_)))
            {
                g.bad(
                    &at,
                    format!(
                        "calendar.color must be a select or ref field of {}",
                        model.name
                    ),
                );
            }
        }
        if let Some(r) = &ui.report {
            for (k, v) in [("rows", &r.rows), ("cols", &r.cols)] {
                if let Some(v) = v
                    && field(model, v).is_none()
                {
                    g.bad(&at, format!("report.{k}: {} has no field {v}", model.name));
                }
            }
            if let Some(v) = &r.measure
                && !field(model, v).is_some_and(|f| is_numeric(&f.kind))
            {
                g.bad(
                    &at,
                    format!(
                        "report.measure must be a number, money or decimal field of {}",
                        model.name
                    ),
                );
            }
        }
        if ui.sections.len() > 20 {
            g.bad(&at, "at most 20 sections".into());
        }
        let states: BTreeSet<&str> = model
            .governed_fields()
            .iter()
            .filter_map(|n| field(model, n))
            .flat_map(|f| f.kind.options().iter().copied())
            .collect();
        for (i, s) in ui.sections.iter().enumerate() {
            let sp = format!("{at}.section {}", i + 1);
            g.text(&sp, "label", &Some(s.label.clone()), 80);
            if s.fields.is_empty() {
                g.bad(&sp, "a section needs fields".into());
            }
            g.fields_exist(&sp, model, "fields", &s.fields, 40);
            for v in &s.show_in {
                if !states.contains(v.as_str()) {
                    g.bad(&sp, format!("show_in: {v:?} is not a state of {} (an option of a field its actions move)", model.name));
                }
            }
        }
        if ui.filters.len() > 20 {
            g.bad(&at, "at most 20 filters".into());
        }
        for (i, f) in ui.filters.iter().enumerate() {
            let fp = format!("{at}.filter {}", i + 1);
            g.text(&fp, "label", &Some(f.label.clone()), 80);
            g.filter(&fp, "where", model, None, &f.filter);
        }
        if let Some(u) = &ui.public_url {
            if !u.starts_with('/') || u.chars().any(char::is_whitespace) {
                g.bad(
                    &at,
                    format!("public_url must be a path such as \"/blog/{{slug}}\", not {u:?}"),
                );
            } else {
                match template_paths(u) {
                    Err(e) => g.bad(&at, format!("public_url: {e}")),
                    Ok(paths) => {
                        for p in paths {
                            if p.via.is_some() || field(model, &p.field).is_none() {
                                g.bad(
                                    &at,
                                    format!(
                                        "public_url: {{{}}} must be a field of {}",
                                        p.written(),
                                        model.name
                                    ),
                                );
                            }
                        }
                    }
                }
            }
        }
        let mut seen = BTreeSet::new();
        for c in &ui.children {
            let cp = format!("{mp}.child {}", c.model);
            g.one_of(&cp, "shape", &c.shape, &CHILD_SHAPES);
            if !seen.insert(c.model.clone()) {
                g.bad(&cp, "the child is declared twice".into());
            }
            match (g.lookup)(&c.model) {
                Some(cm) => {
                    if !field(&cm, &c.via).is_some_and(|f| f.kind.target() == Some(model.name)) {
                        g.bad(
                            &cp,
                            format!("via: {}.{} is not a ref to {}", c.model, c.via, model.name),
                        );
                    }
                }
                None if g.strict => g.bad(&cp, format!("model {} does not exist", c.model)),
                None => {}
            }
        }
    }
    let governed = model.governed_fields();
    for f in &model.fields {
        let Some(h) = &f.hints else { continue };
        let fp = format!("{mp}.{}", f.name);
        check_field(g, model, f, h, &fp, &governed);
    }
    for c in &model.commands {
        let Some(ui) = &c.ui else { continue };
        let cp = format!("{mp}::{}", c.name);
        if ui.primary && ui.danger {
            g.bad(&cp, "an action is primary or danger, not both".into());
        }
        g.text(&cp, "confirm", &ui.confirm, 200);
        g.text(&cp, "done", &ui.done, 200);
        if let Some(gr) = &ui.group
            && !name_ok(gr)
        {
            g.bad(&cp, format!("group {gr:?} must be a lowercase name"));
        }
        if ui.needs.len() > 8 {
            g.bad(&cp, "needs takes at most 8 fields".into());
        }
        let mut seen = BTreeSet::new();
        for n in &ui.needs {
            match field(model, n) {
                None => g.bad(&cp, format!("needs: {} has no field {n}", model.name)),
                Some(f) if f.computed.is_some() || f.sequence.is_some() || governed.contains(&f.name) || f.kind.is_blob() => g.bad(
                    &cp,
                    format!("needs: {n} cannot be written by an action input (computed, numbered, a state field or a file)"),
                ),
                Some(_) if !seen.insert(n) => g.bad(&cp, format!("needs lists {n} twice")),
                Some(_) => {}
            }
        }
        if let Some(n) = &ui.not_by
            && !field(model, n).is_some_and(|f| is_text(&f.kind))
        {
            g.bad(
                &cp,
                format!(
                    "not_by must be a text field of {} that holds a user id",
                    model.name
                ),
            );
        }
    }
}

fn check_field(
    g: &mut Gate<'_>,
    model: &ModelDef,
    f: &FieldDef,
    h: &FieldHints,
    fp: &str,
    governed: &[&str],
) {
    let set_by_system = f.computed.is_some() || f.sequence.is_some();
    if let Some(d) = &h.default {
        if set_by_system || governed.contains(&f.name) || f.kind.is_blob() {
            g.bad(
                fp,
                "default: a computed, numbered, state or file field cannot have a default".into(),
            );
        } else {
            let fits = match d {
                DefaultSpec::Literal(l) => literal_ok(&f.kind, l),
                DefaultSpec::Today(_) => matches!(f.kind, FieldKind::Date),
                DefaultSpec::User | DefaultSpec::InstanceCurrency => {
                    matches!(f.kind, FieldKind::Text)
                }
                DefaultSpec::Open(m) => f.kind.target() == Some(m.as_str()),
            };
            if !fits {
                g.bad(
                    fp,
                    format!(
                        "default {:?} does not fit the {} field {}",
                        d.written_text(),
                        f.kind.name(),
                        f.name
                    ),
                );
            }
            if let DefaultSpec::Open(m) = d {
                match (g.lookup)(m) {
                    Some(t) if open_state_field(&t).is_none() => g.bad(
                        fp,
                        format!(
                            "default $open:{m}: {m} has no state field with the option \"open\""
                        ),
                    ),
                    None if g.strict => {
                        g.bad(fp, format!("default $open:{m}: model {m} does not exist"))
                    }
                    _ => {}
                }
            }
        }
    }
    if let Some(p) = &h.fill_from {
        if set_by_system || governed.contains(&f.name) {
            g.bad(
                fp,
                "fill_from: a computed, numbered or state field cannot be filled".into(),
            );
        }
        if p.via.is_none() {
            g.bad(fp, "fill_from must be ref.field: a ref field of this model and a field of the record it points at".into());
        } else {
            match g.resolve(model, p) {
                Err(e) => g.bad(fp, format!("fill_from: {e}")),
                Ok(Some(src)) if !copy_ok(&f.kind, &src.kind) => g.bad(
                    fp,
                    format!(
                        "fill_from: {} is {}, which does not fit the {} field {}",
                        p.written(),
                        src.kind.name(),
                        f.kind.name(),
                        f.name
                    ),
                ),
                Ok(_) => {}
            }
        }
    }
    if h.readonly && f.required && h.default.is_none() && h.fill_from.is_none() {
        g.bad(fp, "a readonly field that is required needs a default or fill_from, or no create could set it".into());
    }
    if h.readonly && set_by_system {
        g.bad(
            fp,
            "readonly: a computed or numbered field is read-only already".into(),
        );
    }
    if let Some(rf) = &h.ref_filter {
        match f.kind.target() {
            None => g.bad(fp, "ref_filter is only for a ref field".into()),
            Some(to) => match (g.lookup)(to) {
                Some(t) => g.filter(fp, "ref_filter", &t, Some(model), rf),
                None if g.strict => g.bad(fp, format!("ref_filter: model {to} does not exist")),
                None => {}
            },
        }
    }
    let ui = &h.ui;
    g.one_of(fp, "display", &ui.display, &DISPLAYS);
    if let Some(d) = &ui.display {
        let fits = match d.as_str() {
            "time_of_day" | "duration_minutes" | "percent" | "percent_bp" | "km_from_m"
            | "epoch_seconds" => is_numeric(&f.kind),
            "markdown" | "tel" | "email" | "url" | "address" => is_text(&f.kind),
            _ => true,
        };
        if !fits {
            g.bad(
                fp,
                format!(
                    "display {d:?} does not fit the {} field {}",
                    f.kind.name(),
                    f.name
                ),
            );
        }
    }
    g.one_of(fp, "aggregate", &ui.aggregate, &AGGREGATES);
    if ui.aggregate.is_some() && !is_numeric(&f.kind) {
        g.bad(
            fp,
            "aggregate is for a number, money or decimal field".into(),
        );
    }
    for (o, t) in &ui.tones {
        if !f.kind.options().contains(&o.as_str()) {
            g.bad(fp, format!("tones: {o:?} is not an option of {}", f.name));
        }
        if !TONES.contains(&t.as_str()) {
            g.bad(
                fp,
                format!("tones: {t:?} must be one of {}", TONES.join(", ")),
            );
        }
    }
    if let Some(n) = ui.warn_before_days {
        if !matches!(f.kind, FieldKind::Date | FieldKind::Datetime) {
            g.bad(
                fp,
                "warn_before_days is for a date or datetime field".into(),
            );
        }
        if !(1..=365).contains(&n) {
            g.bad(fp, "warn_before_days must be from 1 to 365".into());
        }
    }
    if let Some(src) = &ui.readonly_when {
        check_readonly_when(g, model, f, h, fp, src);
    }
    if let Some(c) = &ui.currency_from {
        if !matches!(f.kind, FieldKind::Money | FieldKind::Decimal { .. }) {
            g.bad(fp, "currency_from is for a money or decimal field".into());
        }
        match Path::parse(c) {
            Err(e) => g.bad(fp, format!("currency_from: {e}")),
            Ok(p) => match g.resolve(model, &p) {
                Err(e) => g.bad(fp, format!("currency_from: {e}")),
                Ok(Some(src)) if !is_text(&src.kind) => {
                    g.bad(fp, format!("currency_from: {c} must be a text field"))
                }
                Ok(_) => {}
            },
        }
    }
}

impl DefaultSpec {
    /// The default as written, as text (for messages).
    pub fn written_text(&self) -> String {
        match self.written() {
            DefaultLit::Text(t) => t,
            DefaultLit::Int(n) => n.to_string(),
            DefaultLit::Bool(b) => b.to_string(),
        }
    }
}

/// `readonly_when`: a true/false condition over fields of the same record, inside the part of the
/// language the browser evaluates, on a field a person could edit (not one the server sets).
fn check_readonly_when(
    g: &mut Gate<'_>,
    model: &ModelDef,
    f: &FieldDef,
    h: &FieldHints,
    fp: &str,
    src: &str,
) {
    if f.computed.is_some() || f.sequence.is_some() || h.readonly {
        g.bad(
            fp,
            "readonly_when is for a field a person edits, not a computed, numbered or readonly one"
                .into(),
        );
        return;
    }
    if src.is_empty() || src.chars().count() > 400 {
        g.bad(fp, "readonly_when must be 1 to 400 characters".into());
        return;
    }
    let e = match crate::expr::Expr::parse_viewer(src) {
        Ok(e) => e,
        Err(e) => return g.bad(fp, format!("readonly_when `{src}`: {e}")),
    };
    if let Err(e) = crate::expr::check_display_subset(&e, model) {
        return g.bad(fp, format!("readonly_when `{src}`: {e}"));
    }
    let scope = crate::expr::Scope {
        model,
        lookup: g.lookup,
        allow_computed: true,
    };
    match crate::expr::check(&e, &scope) {
        Err(e) => g.bad(fp, format!("readonly_when `{src}`: {e}")),
        Ok(crate::expr::Ty::Bool) => {}
        Ok(t) => g.bad(
            fp,
            format!(
                "readonly_when `{src}` is {}; it must be a comparison or other true/false expression",
                t.name()
            ),
        ),
    }
}

/// Every child of `this` by the `resolve_child` rule over `models` (a model named `<this>_<x>`
/// with exactly one ref to `this`), followed by the declared ones not found that way. The shape of
/// a found child is `images` or `translations` by its name, `lines` otherwise.
pub fn children_of(this: &ModelDef, models: &[&ModelDef]) -> Vec<ChildDecl> {
    let prefix = format!("{}_", this.name);
    let mut out: Vec<ChildDecl> = Vec::new();
    for c in models {
        let Some(rest) = c.name.strip_prefix(&prefix) else {
            continue;
        };
        let refs: Vec<&str> = c
            .fields
            .iter()
            .filter(|f| f.kind.target() == Some(this.name))
            .map(|f| f.name)
            .collect();
        if let [via] = refs.as_slice() {
            let shape = match rest {
                "image" | "images" => "images",
                "translation" | "translations" => "translations",
                _ => "lines",
            };
            out.push(ChildDecl {
                model: c.name.to_string(),
                via: via.to_string(),
                shape: Some(shape.to_string()),
            });
        }
    }
    for d in this.ui().map(|u| u.children.as_slice()).unwrap_or_default() {
        match out.iter_mut().find(|c| c.model == d.model) {
            Some(found) => {
                let shape = d.shape.clone().or_else(|| found.shape.clone());
                *found = ChildDecl { shape, ..d.clone() };
            }
            None => out.push(d.clone()),
        }
    }
    out
}
