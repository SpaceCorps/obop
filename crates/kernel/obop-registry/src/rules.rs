//! Declarative rules of a model: computed fields, sequences, uniqueness, row-level access and
//! the effects and assertions of an action (D56, D57, D58). These are plain data; the command
//! layer runs them. This module also holds the install-time checks, so a rule that can never
//! work is refused when the module is parsed or installed, not when somebody first uses it.

use crate::expr::{self, Expr, Scope, Ty};
use crate::{CommandDef, FieldDef, FieldKind, ModelDef, Module, Violation, intern};
use obop_types::DataClass;
use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

/// At most this many records are created, changed or looked at by one action (effects included).
pub const MAX_EFFECT_RECORDS: usize = 500;
/// At most this many effects per action.
pub const MAX_EFFECTS: usize = 12;
/// An action may start at most this many nested actions through `set_state` effects.
pub const MAX_EFFECT_DEPTH: usize = 4;

// ---- computed fields ----------------------------------------------------------------------

/// A computed field (D57): an expression over the record and its child records, evaluated when
/// the record is read. Never stored, never written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Computed {
    pub source: String,
    pub expr: Expr,
}

impl Computed {
    pub fn parse(source: &str) -> Result<Self, String> {
        Ok(Self {
            source: source.trim().to_string(),
            expr: Expr::parse(source)?,
        })
    }
}

// ---- uniqueness ---------------------------------------------------------------------------

/// `unique = true` (plus `unique_with`): no two records of the model may have the same value in
/// this field and the same values in the scope fields. Empty values never collide.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Unique {
    pub with: Vec<&'static str>,
}

// ---- sequences ----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Reset {
    #[default]
    Never,
    Year,
    Month,
}

impl Reset {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "never" => Some(Self::Never),
            "year" => Some(Self::Year),
            "month" => Some(Self::Month),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::Year => "year",
            Self::Month => "month",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Lit(String),
    Year,
    Month,
    Seq(usize),
}

/// A sequence number template such as `INV-{year}-{seq:05}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeqFormat {
    source: String,
    parts: Vec<Part>,
}

impl SeqFormat {
    pub fn parse(src: &str) -> Result<Self, String> {
        if src.is_empty() || src.chars().count() > 40 {
            return Err("format must be 1 to 40 characters".into());
        }
        let mut parts = Vec::new();
        let mut lit = String::new();
        let mut rest = src;
        while !rest.is_empty() {
            let c = rest.chars().next().unwrap_or(' ');
            if c == '{' {
                let Some(end) = rest.find('}') else {
                    return Err("`{` is not closed".into());
                };
                let tok = &rest[1..end];
                if !lit.is_empty() {
                    parts.push(Part::Lit(std::mem::take(&mut lit)));
                }
                parts.push(match tok {
                    "year" => Part::Year,
                    "month" => Part::Month,
                    "seq" => Part::Seq(1),
                    t => match t.strip_prefix("seq:") {
                        Some(w) if !w.is_empty() && w.chars().all(|c| c.is_ascii_digit()) => {
                            let width: usize = w.parse().unwrap_or(0);
                            if !(1..=12).contains(&width) {
                                return Err("{seq:N} needs N from 1 to 12".into());
                            }
                            Part::Seq(width)
                        }
                        _ => {
                            return Err(format!(
                                "unknown placeholder {{{t}}} (use {{year}}, {{month}}, {{seq}} or {{seq:05}})"
                            ));
                        }
                    },
                });
                rest = &rest[end + 1..];
            } else if c.is_ascii_alphanumeric() || "-_/. #".contains(c) {
                lit.push(c);
                rest = &rest[c.len_utf8()..];
            } else {
                return Err(format!(
                    "{c:?} is not allowed in a format (letters, digits and - _ / . # space)"
                ));
            }
        }
        if !lit.is_empty() {
            parts.push(Part::Lit(lit));
        }
        if parts.iter().filter(|p| matches!(p, Part::Seq(_))).count() != 1 {
            return Err("format needs exactly one {seq} or {seq:05}".into());
        }
        Ok(Self {
            source: src.to_string(),
            parts,
        })
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn has_year(&self) -> bool {
        self.parts.contains(&Part::Year)
    }

    pub fn has_month(&self) -> bool {
        self.parts.contains(&Part::Month)
    }

    pub fn render(&self, year: i64, month: i64, n: i64) -> String {
        let mut s = String::new();
        for p in &self.parts {
            match p {
                Part::Lit(l) => s.push_str(l),
                Part::Year => s.push_str(&format!("{year:04}")),
                Part::Month => s.push_str(&format!("{month:02}")),
                Part::Seq(w) => s.push_str(&format!("{n:0w$}", w = *w)),
            }
        }
        s
    }
}

/// A `sequence` field (D57): read-only, numbered by the system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceSpec {
    pub format: SeqFormat,
    /// The action that numbers the record; none: numbered when it is created.
    pub assign_on: Option<&'static str>,
    /// A numbered record cannot be deleted, so the series never has a hole.
    pub gapless: bool,
    pub reset: Reset,
}

// ---- row-level access ---------------------------------------------------------------------

/// `[model.access]` (D58): who may see and change which records, in addition to the groups of
/// the model. A record is visible when any rule grants it; the holder of `see_all_group` and the
/// instance administrator see every record.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Access {
    pub own: Option<&'static str>,
    pub via: Option<&'static str>,
    pub public_field: Option<&'static str>,
    pub shared_with: Option<&'static str>,
    pub see_all_group: Option<&'static str>,
}

impl Access {
    /// Fields the rules read.
    pub fn fields(&self) -> Vec<&'static str> {
        [self.own, self.via, self.public_field, self.shared_with]
            .into_iter()
            .flatten()
            .collect()
    }

    /// A short description for the interface: which rule kinds are on.
    pub fn is_empty(&self) -> bool {
        self.own.is_none()
            && self.via.is_none()
            && self.public_field.is_none()
            && self.shared_with.is_none()
            && self.see_all_group.is_none()
    }
}

// ---- effects and assertions ---------------------------------------------------------------

/// A literal in a mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lit {
    Null,
    Bool(bool),
    Int(i64),
    Text(String),
}

/// Where a value of a mapping comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Src {
    Const(Lit),
    /// `$self.field`: a field of the record the action runs on (after the transition).
    SelfField(String),
    /// `$self.id`
    SelfId,
    /// `$line.field`: a field of the current child row (only with `for_each`).
    Line(String),
    /// `$line.id`
    LineId,
    /// `$created.name`: the id of the record an earlier effect named `name` created.
    Created(String),
    /// `$now`: the time as an RFC 3339 UTC string.
    Now,
    /// `$today`: the date, `YYYY-MM-DD`.
    Today,
    /// `$caller`: the id of the principal that ran the action.
    Caller,
}

impl Src {
    /// Parse a value of a mapping: a string beginning with `$` is a variable (`$$` starts a
    /// literal dollar sign), anything else is a constant.
    pub fn from_str(s: &str) -> Result<Self, String> {
        if let Some(lit) = s.strip_prefix("$$") {
            return Ok(Src::Const(Lit::Text(format!("${lit}"))));
        }
        let Some(v) = s.strip_prefix('$') else {
            return Ok(Src::Const(Lit::Text(s.to_string())));
        };
        let (head, tail) = v.split_once('.').map_or((v, None), |(h, t)| (h, Some(t)));
        let name_ok = |t: &str| {
            !t.is_empty()
                && t.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        };
        match (head, tail) {
            ("now", None) => Ok(Src::Now),
            ("today", None) => Ok(Src::Today),
            ("caller", None) => Ok(Src::Caller),
            ("self", Some("id")) => Ok(Src::SelfId),
            ("self", Some(f)) if name_ok(f) => Ok(Src::SelfField(f.to_string())),
            ("line", Some("id")) => Ok(Src::LineId),
            ("line", Some(f)) if name_ok(f) => Ok(Src::Line(f.to_string())),
            ("created", Some(n)) if name_ok(n) => Ok(Src::Created(n.to_string())),
            _ => Err(format!(
                "{s:?} is not a value (use $self.field, $self.id, $line.field, $created.name, $now, $today, $caller, or a constant)"
            )),
        }
    }
}

/// Where an `update` or `set_state` effect finds its records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// The record this record refers to through the ref field.
    Ref(&'static str),
    /// Every record of the target model that refers to this record (through `via`, or its only
    /// ref to this model).
    Children(Option<&'static str>),
}

/// How an `adjust` effect finds its record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Locator {
    Ref(&'static str),
    /// The one record whose fields equal these values.
    Match(Vec<(&'static str, Src)>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectKind {
    Create {
        model: &'static str,
        map: Vec<(&'static str, Src)>,
        /// Name for `$created.name` in later effects.
        name: Option<String>,
    },
    Update {
        model: &'static str,
        target: Target,
        set: Vec<(&'static str, Src)>,
    },
    Adjust {
        model: &'static str,
        locator: Locator,
        field: &'static str,
        subtract: bool,
        by: Src,
        /// A `match` that finds nothing creates the record with the match values and the delta.
        create_missing: bool,
        /// The adjusted field may not end below this.
        min: Option<i64>,
    },
    SetState {
        model: &'static str,
        target: Target,
        action: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effect {
    pub kind: EffectKind,
    /// Repeat once per child row of this record: a child model, resolved like an aggregate's.
    pub for_each: Option<String>,
    pub for_each_via: Option<String>,
    /// Run only when this expression, evaluated on the record after the transition, is true.
    pub when: Option<Arc<Expr>>,
    pub when_source: Option<String>,
}

impl Effect {
    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            EffectKind::Create { .. } => "create",
            EffectKind::Update { .. } => "update",
            EffectKind::Adjust { .. } => "adjust",
            EffectKind::SetState { .. } => "set_state",
        }
    }

    pub fn model(&self) -> &'static str {
        match &self.kind {
            EffectKind::Create { model, .. }
            | EffectKind::Update { model, .. }
            | EffectKind::Adjust { model, .. }
            | EffectKind::SetState { model, .. } => model,
        }
    }
}

/// A condition that must hold for an action to run: a boolean expression over the record and
/// its children, evaluated before the transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assertion {
    pub source: String,
    pub expr: Arc<Expr>,
    pub message: Option<String>,
}

impl Assertion {
    pub fn parse(source: &str, message: Option<&str>) -> Result<Self, String> {
        Ok(Self {
            source: source.trim().to_string(),
            expr: Arc::new(Expr::parse(source)?),
            message: message.map(String::from),
        })
    }

    /// The text of the error when the assertion does not hold.
    pub fn failure(&self) -> String {
        match &self.message {
            Some(m) => format!("{m} (assertion {} failed)", self.source),
            None => format!("assertion failed: {}", self.source),
        }
    }
}

// ---- builders -----------------------------------------------------------------------------

impl FieldDef {
    /// A computed field; panics on a bad expression (for modules written in Rust, where that is
    /// a programming error; packages go through `Computed::parse`).
    pub fn computed(mut self, source: &str) -> Self {
        self.computed = Some(Arc::new(Computed::parse(source).unwrap_or_else(|e| {
            panic!("{}: bad computed expression {source:?}: {e}", self.name)
        })));
        self
    }

    pub fn unique(mut self) -> Self {
        self.unique = Some(Unique::default());
        self
    }

    pub fn unique_with(mut self, with: &[&str]) -> Self {
        self.unique = Some(Unique {
            with: with.iter().map(|w| intern(w)).collect(),
        });
        self
    }

    /// A sequence field numbered when the record is created.
    pub fn sequence(mut self, format: &str, gapless: bool, reset: Reset) -> Self {
        self.sequence = Some(Arc::new(SequenceSpec {
            format: SeqFormat::parse(format)
                .unwrap_or_else(|e| panic!("{}: bad sequence format {format:?}: {e}", self.name)),
            assign_on: None,
            gapless,
            reset,
        }));
        self
    }

    /// Number the record when this action runs instead of when it is created.
    pub fn assign_on(mut self, action: &str) -> Self {
        if let Some(s) = &mut self.sequence {
            Arc::make_mut(s).assign_on = Some(intern(action));
        }
        self
    }
}

impl CommandDef {
    pub fn assertion(mut self, source: &str) -> Self {
        self.asserts.push(
            Assertion::parse(source, None)
                .unwrap_or_else(|e| panic!("{}: bad assertion {source:?}: {e}", self.name)),
        );
        self
    }

    pub fn assertion_msg(mut self, source: &str, message: &str) -> Self {
        self.asserts.push(
            Assertion::parse(source, Some(message))
                .unwrap_or_else(|e| panic!("{}: bad assertion {source:?}: {e}", self.name)),
        );
        self
    }

    pub fn effect(mut self, effect: Effect) -> Self {
        self.effects.push(effect);
        self
    }
}

impl Effect {
    pub fn new(kind: EffectKind) -> Self {
        Self {
            kind,
            for_each: None,
            for_each_via: None,
            when: None,
            when_source: None,
        }
    }

    pub fn for_each(mut self, child: &str) -> Self {
        self.for_each = Some(child.to_string());
        self
    }

    pub fn when(mut self, source: &str) -> Self {
        self.when = Some(Arc::new(
            Expr::parse(source).unwrap_or_else(|e| panic!("bad condition {source:?}: {e}")),
        ));
        self.when_source = Some(source.to_string());
        self
    }
}

impl ModelDef {
    /// Whether any rule of the model needs the one write lock: a unique field, a sequence, a
    /// computed field cannot race, but effects touch other records.
    pub fn has_rules(&self) -> bool {
        self.fields
            .iter()
            .any(|f| f.unique.is_some() || f.sequence.is_some())
            || self.commands.iter().any(|c| !c.effects.is_empty())
    }

    pub fn computed_fields(&self) -> impl Iterator<Item = &FieldDef> {
        self.fields.iter().filter(|f| f.computed.is_some())
    }

    /// Fields no create or update may set: computed values and sequences.
    pub fn is_system_field(&self, name: &str) -> bool {
        self.fields
            .iter()
            .any(|f| f.name == name && (f.computed.is_some() || f.sequence.is_some()))
    }
}

// ---- the checks ---------------------------------------------------------------------------

type Lookup<'a> = &'a dyn Fn(&str) -> Option<Arc<ModelDef>>;

fn kind_class_ok(target: &FieldKind, src: &FieldKind) -> bool {
    match target {
        FieldKind::Text | FieldKind::LongText => !matches!(src, FieldKind::Bool),
        FieldKind::Number | FieldKind::Money => matches!(src, FieldKind::Number | FieldKind::Money),
        FieldKind::Date => matches!(src, FieldKind::Date),
        FieldKind::Bool => matches!(src, FieldKind::Bool),
        FieldKind::Select(to) => match src {
            FieldKind::Select(from) => from.iter().all(|o| to.contains(o)),
            _ => false,
        },
        FieldKind::Ref(to) => matches!(src, FieldKind::Ref(from) if from == to),
    }
}

/// What a value of the mapping is, for type checking.
enum Shape {
    Kind(FieldKind),
    Lit(Lit),
    Id(String),
    Now,
    Today,
    Caller,
}

fn shape_ok(target: &FieldKind, s: &Shape) -> bool {
    match s {
        Shape::Kind(k) => kind_class_ok(target, k),
        Shape::Lit(Lit::Null) => true,
        Shape::Lit(Lit::Bool(_)) => matches!(target, FieldKind::Bool),
        Shape::Lit(Lit::Int(_)) => matches!(target, FieldKind::Number | FieldKind::Money),
        Shape::Lit(Lit::Text(t)) => match target {
            FieldKind::Text | FieldKind::LongText => true,
            FieldKind::Date => crate::is_valid_date(t),
            FieldKind::Select(o) => o.contains(&t.as_str()),
            _ => false,
        },
        Shape::Id(model) => match target {
            FieldKind::Ref(to) => to == model,
            FieldKind::Text | FieldKind::LongText => true,
            _ => false,
        },
        Shape::Now | Shape::Caller => matches!(target, FieldKind::Text | FieldKind::LongText),
        Shape::Today => matches!(
            target,
            FieldKind::Date | FieldKind::Text | FieldKind::LongText
        ),
    }
}

struct Ctx<'a> {
    this: &'a ModelDef,
    lookup: Lookup<'a>,
    /// The child model of `for_each`, when there is one.
    line: Option<Arc<ModelDef>>,
    /// Created names declared so far, with their model and whether they came from a loop.
    created: Vec<(String, &'static str, bool)>,
}

impl Ctx<'_> {
    fn shape(&self, s: &Src) -> Result<Shape, String> {
        Ok(match s {
            Src::Const(l) => Shape::Lit(l.clone()),
            Src::SelfField(f) => match self.this.fields.iter().find(|d| d.name == f) {
                Some(d) => Shape::Kind(d.kind.clone()),
                None => return Err(format!("$self.{f}: {} has no field {f}", self.this.name)),
            },
            Src::SelfId => Shape::Id(self.this.name.to_string()),
            Src::Line(f) => {
                let Some(line) = &self.line else {
                    return Err(format!("$line.{f} needs for_each"));
                };
                match line.fields.iter().find(|d| d.name == f) {
                    Some(d) => Shape::Kind(d.kind.clone()),
                    None => return Err(format!("$line.{f}: {} has no field {f}", line.name)),
                }
            }
            Src::LineId => match &self.line {
                Some(l) => Shape::Id(l.name.to_string()),
                None => return Err("$line.id needs for_each".into()),
            },
            Src::Created(n) => match self.created.iter().find(|(c, _, _)| c == n) {
                Some((_, _, true)) => {
                    return Err(format!(
                        "$created.{n}: that effect runs once per child row, so there is no single record"
                    ));
                }
                Some((_, m, false)) => Shape::Id(m.to_string()),
                None => {
                    return Err(format!(
                        "$created.{n}: no earlier create effect is named {n} (name it with `as`)"
                    ));
                }
            },
            Src::Now => Shape::Now,
            Src::Today => Shape::Today,
            Src::Caller => Shape::Caller,
        })
    }

    fn mapping(
        &self,
        target: &ModelDef,
        what: &str,
        map: &[(&'static str, Src)],
    ) -> Result<(), String> {
        for (k, src) in map {
            let Some(f) = target.fields.iter().find(|f| f.name == *k) else {
                return Err(format!("{what}: {} has no field {k}", target.name));
            };
            if f.computed.is_some() || f.sequence.is_some() {
                return Err(format!(
                    "{what}: {}.{k} is {} and cannot be set",
                    target.name,
                    if f.computed.is_some() {
                        "computed"
                    } else {
                        "a sequence number assigned by the system"
                    }
                ));
            }
            if target.governed_fields().contains(k) {
                return Err(format!(
                    "{what}: {}.{k} is a governed state field; use a set_state effect, not a value",
                    target.name
                ));
            }
            let shape = self.shape(src).map_err(|e| format!("{what}: {k}: {e}"))?;
            if !shape_ok(&f.kind, &shape) {
                return Err(format!(
                    "{what}: {}.{k} must be {}; that value does not fit",
                    target.name,
                    f.kind.describe()
                ));
            }
        }
        Ok(())
    }

    /// The target of an update or set_state, resolved against this record.
    fn target(&self, what: &str, model: &ModelDef, t: &Target) -> Result<(), String> {
        match t {
            Target::Ref(f) => match self.this.fields.iter().find(|d| d.name == *f) {
                Some(d) if d.kind.target() == Some(model.name) => Ok(()),
                Some(d) => Err(format!(
                    "{what}: {}.{f} is {}, not a ref to {}",
                    self.this.name,
                    d.kind.name(),
                    model.name
                )),
                None => Err(format!("{what}: {} has no field {f}", self.this.name)),
            },
            Target::Children(via) => {
                let refs: Vec<&str> = model
                    .fields
                    .iter()
                    .filter(|f| f.kind.target() == Some(self.this.name))
                    .map(|f| f.name)
                    .collect();
                match (via, refs.as_slice()) {
                    (Some(v), _) if refs.contains(v) => Ok(()),
                    (Some(v), _) => Err(format!(
                        "{what}: {}.{v} is not a ref to {}",
                        model.name, self.this.name
                    )),
                    (None, [_]) => Ok(()),
                    (None, []) => Err(format!(
                        "{what}: {} has no ref to {}, so it has no records \"of\" one {}",
                        model.name, self.this.name, self.this.name
                    )),
                    (None, _) => Err(format!(
                        "{what}: {} has several refs to {}; name one with `via`",
                        model.name, self.this.name
                    )),
                }
            }
        }
    }
}

fn check_effect(
    cmd: &CommandDef,
    idx: usize,
    e: &Effect,
    ctx: &mut Ctx<'_>,
    strict: bool,
) -> Result<(), String> {
    let what = format!("effect {} ({})", idx + 1, e.kind_name());
    ctx.line = None;
    if let Some(child) = &e.for_each {
        let (line, _) = expr::resolve_child(ctx.this, child, e.for_each_via.as_deref(), ctx.lookup)
            .map_err(|er| {
                format!(
                    "{what}: for_each: {er}; it must be a model of records that refer to {}",
                    ctx.this.name
                )
            })?;
        ctx.line = Some(line);
    }
    if let Some(w) = &e.when {
        let scope = Scope {
            model: ctx.this,
            lookup: ctx.lookup,
            allow_computed: true,
        };
        let ty = expr::check(w, &scope).map_err(|er| format!("{what}: when: {er}"))?;
        if ty != Ty::Bool {
            return Err(format!("{what}: when must be a true/false expression"));
        }
    }
    let Some(target) = (ctx.lookup)(e.model()) else {
        if strict {
            return Err(format!("{what}: model {} does not exist", e.model()));
        }
        return Ok(());
    };
    match &e.kind {
        EffectKind::Create { map, name, .. } => {
            ctx.mapping(&target, &what, map)?;
            for f in &target.fields {
                if f.required
                    && f.computed.is_none()
                    && f.sequence.is_none()
                    && !target.governed_fields().contains(&f.name)
                    && !map.iter().any(|(k, _)| *k == f.name)
                {
                    return Err(format!(
                        "{what}: {}.{} is required, so the effect must set it",
                        target.name, f.name
                    ));
                }
            }
            if let Some(n) = name {
                if ctx.created.iter().any(|(c, _, _)| c == n) {
                    return Err(format!("{what}: the name {n} is used twice"));
                }
                ctx.created
                    .push((n.clone(), target.name, e.for_each.is_some()));
            }
        }
        EffectKind::Update { target: t, set, .. } => {
            ctx.target(&what, &target, t)?;
            ctx.mapping(&target, &what, set)?;
            if set.is_empty() {
                return Err(format!("{what}: set is empty"));
            }
        }
        EffectKind::Adjust {
            locator,
            field,
            by,
            min,
            create_missing,
            ..
        } => {
            let Some(f) = target.fields.iter().find(|d| d.name == *field) else {
                return Err(format!("{what}: {} has no field {field}", target.name));
            };
            if !matches!(f.kind, FieldKind::Number | FieldKind::Money) || f.computed.is_some() {
                return Err(format!(
                    "{what}: {}.{field} must be a stored number or money field",
                    target.name
                ));
            }
            match ctx.shape(by).map_err(|er| format!("{what}: by: {er}"))? {
                Shape::Kind(FieldKind::Number | FieldKind::Money) | Shape::Lit(Lit::Int(_)) => {}
                _ => return Err(format!("{what}: by must be a number or money value")),
            }
            match locator {
                Locator::Ref(r) => {
                    ctx.target(&what, &target, &Target::Ref(r))?;
                    if *create_missing {
                        return Err(format!("{what}: create_missing needs match, not ref"));
                    }
                }
                Locator::Match(m) => {
                    if m.is_empty() {
                        return Err(format!("{what}: match is empty"));
                    }
                    ctx.mapping(&target, &what, m)?;
                    if *create_missing {
                        for f2 in &target.fields {
                            if f2.required
                                && f2.name != *field
                                && !m.iter().any(|(k, _)| *k == f2.name)
                            {
                                return Err(format!(
                                    "{what}: create_missing would create {} without its required field {}; match on it",
                                    target.name, f2.name
                                ));
                            }
                        }
                    }
                }
            }
            if min.is_some() && f.computed.is_some() {
                return Err(format!("{what}: min needs a stored field"));
            }
        }
        EffectKind::SetState {
            target: t, action, ..
        } => {
            ctx.target(&what, &target, t)?;
            let Some(a) = target.commands.iter().find(|c| c.name == *action) else {
                return Err(format!("{what}: {} has no action {action}", target.name));
            };
            if a.transition.is_none() {
                return Err(format!("{what}: {action} is not a state transition"));
            }
            if a.name == cmd.name && target.name == ctx.this.name {
                return Err(format!(
                    "{what}: {}.{action} would start itself",
                    target.name
                ));
            }
        }
    }
    Ok(())
}

/// Actions that `cmd` of `model` starts, directly, through set_state effects.
fn started<'a>(
    model: &'a ModelDef,
    cmd: &'a CommandDef,
    lookup: Lookup<'_>,
) -> Vec<(Arc<ModelDef>, &'static str)> {
    let _ = model;
    cmd.effects
        .iter()
        .filter_map(|e| match &e.kind {
            EffectKind::SetState { model, action, .. } => lookup(model).map(|m| (m, *action)),
            _ => None,
        })
        .collect()
}

/// A chain of actions, by set_state effects, that comes back to where it started, or is longer
/// than `MAX_EFFECT_DEPTH`.
fn effect_chain_problem(model: &ModelDef, cmd: &CommandDef, lookup: Lookup<'_>) -> Option<String> {
    fn go(
        model: &ModelDef,
        cmd: &CommandDef,
        lookup: Lookup<'_>,
        path: &mut Vec<String>,
    ) -> Option<String> {
        let me = format!("{}.{}", model.name, cmd.name);
        if path.contains(&me) {
            path.push(me);
            return Some(format!("effects form a cycle: {}", path.join(" -> ")));
        }
        if path.len() >= MAX_EFFECT_DEPTH {
            path.push(me);
            return Some(format!(
                "effects start actions more than {MAX_EFFECT_DEPTH} deep: {}",
                path.join(" -> ")
            ));
        }
        path.push(me);
        for (m, action) in started(model, cmd, lookup) {
            if let Some(next) = m.commands.iter().find(|c| c.name == action)
                && let Some(p) = go(&m, next, lookup, path)
            {
                return Some(p);
            }
        }
        path.pop();
        None
    }
    go(model, cmd, lookup, &mut Vec::new())
}

fn covers(out: DataClass, input: DataClass) -> bool {
    if input.is_sensitive() || input == DataClass::Secret {
        out == input
    } else {
        out >= input
    }
}

fn check_computed(
    m: &ModelDef,
    f: &FieldDef,
    c: &Computed,
    lookup: Lookup<'_>,
    bad: &mut dyn FnMut(String),
) {
    if f.required || f.unique.is_some() || f.sequence.is_some() {
        bad("a computed field cannot be required, unique or a sequence".into());
    }
    if !matches!(
        f.kind,
        FieldKind::Text | FieldKind::Number | FieldKind::Money | FieldKind::Bool
    ) {
        bad(format!(
            "a computed field is text, number, money or bool, not {}",
            f.kind.name()
        ));
    }
    if m.governed_fields().contains(&f.name) {
        bad("a computed field cannot be governed by an action".into());
    }
    let scope = Scope {
        model: m,
        lookup,
        allow_computed: true,
    };
    match expr::check(&c.expr, &scope) {
        Err(e) => bad(format!("computed `{}`: {e}", c.source)),
        Ok(ty) if ty != Ty::of_kind(&f.kind) => bad(format!(
            "computed `{}` is {}, but the field is {}",
            c.source,
            ty.name(),
            f.kind.name()
        )),
        Ok(_) => {}
    }
    // The inputs, for the class and group rule.
    let mut inputs: Vec<(String, &FieldDef)> = Vec::new();
    for n in c.expr.fields() {
        if let Some(i) = m.fields.iter().find(|d| d.name == n) {
            inputs.push((format!("{}.{n}", m.name), i));
        }
    }
    let mut agg_inputs: Vec<(String, Arc<ModelDef>, &'static str)> = Vec::new();
    for (_, child, via, field) in c.expr.aggregates() {
        if let Ok((cm, _)) = expr::resolve_child(m, child, via, lookup)
            && let Some(fname) = field
            && let Some(d) = cm.fields.iter().find(|d| d.name == fname)
        {
            agg_inputs.push((format!("{}.{fname}", cm.name), cm.clone(), d.name));
        }
    }
    let mut all: Vec<(String, FieldDef)> =
        inputs.into_iter().map(|(n, d)| (n, d.clone())).collect();
    for (n, cm, dn) in &agg_inputs {
        if let Some(d) = cm.fields.iter().find(|d| d.name == *dn) {
            all.push((n.clone(), d.clone()));
        }
    }
    for (name, input) in all {
        let fc = f.class.unwrap_or(DataClass::Secret);
        let ic = input.class.unwrap_or(DataClass::Secret);
        if !covers(fc, ic) {
            bad(format!(
                "computed `{}` reads {name} (class {ic}); the field must have class {} or stricter, because a value computed from protected data is protected too",
                c.source,
                if ic.is_sensitive() {
                    ic.as_str()
                } else {
                    "at least the same"
                }
            ));
        }
        if input.permission_group != f.permission_group {
            bad(format!(
                "computed `{}` reads {name}, which only {} may read; give the computed field `group = \"{}\"`",
                c.source, input.permission_group, input.permission_group
            ));
        }
        if input.personal_data && !f.personal_data {
            bad(format!(
                "computed `{}` reads personal data in {name}, so the field must be personal data too",
                c.source
            ));
        }
    }
}

/// Dependency cycle among computed fields (through aggregates too). Returns a description.
fn computed_cycle(models: &[Arc<ModelDef>], lookup: Lookup<'_>) -> Option<String> {
    fn deps(m: &Arc<ModelDef>, f: &str, lookup: Lookup<'_>) -> Vec<(Arc<ModelDef>, &'static str)> {
        let Some(def) = m.fields.iter().find(|d| d.name == f) else {
            return vec![];
        };
        let Some(c) = &def.computed else {
            return vec![];
        };
        let mut out = Vec::new();
        for n in c.expr.fields() {
            if let Some(d) = m.fields.iter().find(|d| d.name == n)
                && d.computed.is_some()
            {
                out.push((m.clone(), d.name));
            }
        }
        for (_, child, via, field) in c.expr.aggregates() {
            if let Ok((cm, _)) = expr::resolve_child(m, child, via, lookup)
                && let Some(fname) = field
                && let Some(d) = cm.fields.iter().find(|d| d.name == fname)
                && d.computed.is_some()
            {
                out.push((cm.clone(), d.name));
            }
        }
        out
    }
    fn go(
        m: &Arc<ModelDef>,
        f: &'static str,
        lookup: Lookup<'_>,
        path: &mut Vec<String>,
        done: &mut HashSet<String>,
    ) -> Option<String> {
        let me = format!("{}.{f}", m.name);
        if path.contains(&me) {
            path.push(me);
            return Some(path.join(" -> "));
        }
        if done.contains(&me) {
            return None;
        }
        path.push(me.clone());
        for (dm, df) in deps(m, f, lookup) {
            if let Some(p) = go(&dm, df, lookup, path, done) {
                return Some(p);
            }
        }
        path.pop();
        done.insert(me);
        None
    }
    let mut done = HashSet::new();
    for m in models {
        for f in m.computed_fields() {
            if let Some(p) = go(m, f.name, lookup, &mut Vec::new(), &mut done) {
                return Some(p);
            }
        }
    }
    None
}

fn check_access(m: &ModelDef, a: &Access, lookup: Lookup<'_>, bad: &mut dyn FnMut(String)) {
    if a.is_empty() {
        bad("[model.access] declares no rule".into());
        return;
    }
    let field = |name: &str| m.fields.iter().find(|f| f.name == name);
    if let Some(n) = a.own {
        match field(n) {
            Some(f)
                if matches!(f.kind, FieldKind::Text | FieldKind::Ref(_))
                    && f.computed.is_none() => {}
            Some(_) => bad(format!(
                "access.own: {n} must be a stored text or ref field"
            )),
            None => bad(format!("access.own: no field {n}")),
        }
    }
    if let Some(n) = a.via {
        match field(n).map(|f| &f.kind) {
            Some(FieldKind::Ref(to)) => {
                if to == &m.name {
                    bad(format!("access.via: {n} refers to the model itself"));
                }
                if let Some(parent) = lookup(to) {
                    // A chain of `via` rules that returns here never ends.
                    let mut seen = vec![m.name];
                    let mut cur = parent;
                    loop {
                        if seen.contains(&cur.name) {
                            bad(format!("access.via: {} -> {} is a cycle", m.name, cur.name));
                            break;
                        }
                        seen.push(cur.name);
                        let next = cur
                            .access
                            .as_ref()
                            .and_then(|a| a.via)
                            .and_then(|v| cur.fields.iter().find(|f| f.name == v))
                            .and_then(|f| f.kind.target())
                            .and_then(lookup);
                        match next {
                            Some(n) => cur = n,
                            None => break,
                        }
                    }
                }
            }
            Some(_) => bad(format!("access.via: {n} must be a ref field")),
            None => bad(format!("access.via: no field {n}")),
        }
    }
    if let Some(n) = a.public_field {
        match field(n).map(|f| &f.kind) {
            Some(FieldKind::Bool) => {}
            Some(_) => bad(format!("access.public_field: {n} must be a bool field")),
            None => bad(format!("access.public_field: no field {n}")),
        }
    }
    if let Some(n) = a.shared_with {
        match field(n) {
            Some(f) if matches!(f.kind, FieldKind::Text | FieldKind::LongText) => {}
            Some(_) => bad(format!(
                "access.shared_with: {n} must be a text field holding user ids separated by commas"
            )),
            None => bad(format!("access.shared_with: no field {n}")),
        }
    }
}

/// The rules of a model: computed fields, uniqueness, sequences, access and the effects and
/// assertions of its actions. `lookup` finds models outside the module (dependencies); with
/// `strict` an effect on an unknown model is a violation, otherwise it is left for the install
/// to check.
pub fn check_rules(module: &Module, lookup_ext: Lookup<'_>, strict: bool) -> Vec<Violation> {
    let own: Vec<Arc<ModelDef>> = module.models.iter().cloned().map(Arc::new).collect();
    let lookup = |n: &str| {
        own.iter()
            .find(|m| m.name == n)
            .cloned()
            .or_else(|| lookup_ext(n))
    };
    let mut out = Vec::new();
    for m in &own {
        let mp = format!("{}.{}", module.id, m.name);
        let mut bad = |path: String, problem: String| out.push(Violation { path, problem });
        let governed = m.governed_fields();
        for f in &m.fields {
            let fp = format!("{mp}.{}", f.name);
            if let Some(c) = &f.computed {
                check_computed(m, f, c, &lookup, &mut |p| bad(fp.clone(), p));
            }
            if let Some(u) = &f.unique {
                if f.computed.is_some() {
                    bad(fp.clone(), "a computed field cannot be unique".into());
                }
                if matches!(f.kind, FieldKind::Bool | FieldKind::LongText) {
                    bad(
                        fp.clone(),
                        "unique needs a text, number, money, date, select or ref field".into(),
                    );
                }
                let mut seen = BTreeSet::new();
                for w in &u.with {
                    match m.fields.iter().find(|d| d.name == *w) {
                        None => bad(fp.clone(), format!("unique_with: no field {w}")),
                        Some(d) if d.computed.is_some() => {
                            bad(fp.clone(), format!("unique_with: {w} is computed"))
                        }
                        Some(_) if *w == f.name => {
                            bad(fp.clone(), "unique_with lists the field itself".into())
                        }
                        Some(_) if !seen.insert(*w) => {
                            bad(fp.clone(), format!("unique_with lists {w} twice"))
                        }
                        Some(_) => {}
                    }
                }
                if u.with.len() > 4 {
                    bad(fp.clone(), "unique_with takes at most 4 fields".into());
                }
            }
            if let Some(s) = &f.sequence {
                if !matches!(f.kind, FieldKind::Text) {
                    bad(fp.clone(), "a sequence is a text field".into());
                }
                if f.required || f.computed.is_some() {
                    bad(
                        fp.clone(),
                        "a sequence cannot be required or computed".into(),
                    );
                }
                if governed.contains(&f.name) {
                    bad(fp.clone(), "a sequence cannot be governed".into());
                }
                match s.reset {
                    Reset::Year if !s.format.has_year() => bad(
                        fp.clone(),
                        "reset = \"year\" needs {year} in the format, or numbers would repeat"
                            .into(),
                    ),
                    Reset::Month if !(s.format.has_year() && s.format.has_month()) => bad(
                        fp.clone(),
                        "reset = \"month\" needs {year} and {month} in the format".into(),
                    ),
                    _ => {}
                }
                if let Some(a) = s.assign_on {
                    match m.commands.iter().find(|c| c.name == a) {
                        Some(c) if c.transition.is_some() => {}
                        _ => bad(
                            fp.clone(),
                            format!("assign_on: {a} is not an action of the model"),
                        ),
                    }
                }
            }
        }
        if let Some(a) = &m.access {
            check_access(m, a, &lookup, &mut |p| bad(mp.clone(), p));
        }
        for c in &m.commands {
            let cp = format!("{mp}::{}", c.name);
            if c.transition.is_none() && (!c.effects.is_empty() || !c.asserts.is_empty()) {
                bad(
                    cp.clone(),
                    "effects and assertions belong to an action that moves a state".into(),
                );
            }
            if c.effects.len() > MAX_EFFECTS {
                bad(
                    cp.clone(),
                    format!("an action has at most {MAX_EFFECTS} effects"),
                );
            }
            for a in &c.asserts {
                let scope = Scope {
                    model: m,
                    lookup: &lookup,
                    allow_computed: true,
                };
                match expr::check(&a.expr, &scope) {
                    Ok(Ty::Bool) => {}
                    Ok(t) => bad(
                        cp.clone(),
                        format!(
                            "assertion `{}` is {}; it must be a comparison or other true/false expression",
                            a.source,
                            t.name()
                        ),
                    ),
                    Err(e) => bad(cp.clone(), format!("assertion `{}`: {e}", a.source)),
                }
            }
            let mut ctx = Ctx {
                this: m,
                lookup: &lookup,
                line: None,
                created: Vec::new(),
            };
            for (i, e) in c.effects.iter().enumerate() {
                if let Err(p) = check_effect(c, i, e, &mut ctx, strict) {
                    bad(cp.clone(), p);
                }
            }
            if let Some(p) = effect_chain_problem(m, c, &lookup) {
                bad(cp.clone(), p);
            }
        }
    }
    if let Some(p) = computed_cycle(&own, &lookup) {
        out.push(Violation {
            path: module.id.clone(),
            problem: format!("computed fields depend on each other in a cycle: {p}"),
        });
    }
    out
}

/// Models and fields that effects, aggregates and access rules of `module` use and that the
/// module does not declare itself: the dependencies it needs for them. `(where, model)`.
pub fn external_models(module: &Module) -> Vec<(String, String)> {
    let own: HashSet<&str> = module.models.iter().map(|m| m.name).collect();
    let mut v = Vec::new();
    for m in &module.models {
        for c in &m.commands {
            for (i, e) in c.effects.iter().enumerate() {
                let t = e.model();
                if !own.contains(t) {
                    v.push((
                        format!("{}::{} effect {}", m.name, c.name, i + 1),
                        t.to_string(),
                    ));
                }
            }
        }
    }
    v
}
