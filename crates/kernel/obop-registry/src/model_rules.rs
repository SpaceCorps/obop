//! Model rules (D88, the first slice) and the no-overlap rule (D88 amendment of 2026-11-17).
//!
//! A model rule is a true/false expression over the record's own fields that every create,
//! update, `bulk_create` row and action effect must keep true (`[[model.rule]]`): an end that is
//! not before its start, a total that is not negative. The no-overlap rule (`[model.no_overlap]`)
//! refuses a record whose interval `[start, end)` overlaps that of another record with the same
//! key (one resource on one day). Both refuse with `assertion_failed` and the rule's key as
//! `assert_key`, and their messages are translated under `labels.<lang>.model.<m>.rule.<key>`.
//!
//! Built in this slice: own fields only. Not built (D88): paths through a ref to a parent
//! (`head.field`), `now()`, `applies = "frozen"`, `assert_scope` in the refusal body and the rules
//! in `/api/meta`.

use crate::expr::{self, Expr, Scope, Ty};
use crate::labels::{MAX_ASSERT_MESSAGE_CHARS, text_ok};
use crate::rules::{MAX_ASSERT_KEY, assert_key_ok};
use crate::{FieldKind, ModelDef, Translations, intern};
use std::collections::BTreeMap;
use std::sync::Arc;

/// The most rules a model may declare (the no-overlap rule counts as one).
pub const MAX_RULES: usize = 8;

/// When an update evaluates a rule. A create evaluates every rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Applies {
    /// When the update changes a field the rule reads (the default): a record that broke the
    /// rule before it was declared can still be changed in its other fields.
    #[default]
    Touched,
    /// On every update that changes anything.
    Always,
}

impl Applies {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "touched" => Some(Self::Touched),
            "always" => Some(Self::Always),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Touched => "touched",
            Self::Always => "always",
        }
    }
}

/// One `[[model.rule]]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRule {
    /// `[a-z_]{1,40}`, unique in the model: the `assert_key` of a refusal and the label slot.
    pub key: String,
    pub source: String,
    pub expr: Arc<Expr>,
    /// English, plain text: what the person is told.
    pub message: String,
    /// The message per language, from the module's label tables (`attach_messages`).
    pub messages: BTreeMap<String, String>,
    pub applies: Applies,
}

impl ModelRule {
    pub fn parse(key: &str, source: &str, message: &str, applies: Applies) -> Result<Self, String> {
        Ok(Self {
            key: key.to_string(),
            source: source.trim().to_string(),
            expr: Arc::new(Expr::parse(source)?),
            message: message.to_string(),
            messages: BTreeMap::new(),
            applies,
        })
    }

    /// The own fields the rule reads.
    pub fn reads(&self) -> Vec<&str> {
        let mut v = self.expr.fields();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// The text of the refusal: `<message> (rule <key> failed)`, the message in `lang` when the
    /// labels have it (the suffix stays English). The expression is not in the text: it names
    /// fields.
    pub fn failure_in(&self, lang: Option<&str>) -> String {
        failure(pick(&self.messages, lang, &self.message), &self.key)
    }
}

fn pick<'a>(
    messages: &'a BTreeMap<String, String>,
    lang: Option<&str>,
    english: &'a str,
) -> &'a str {
    lang.and_then(|l| messages.get(l))
        .map(String::as_str)
        .unwrap_or(english)
}

fn failure(message: &str, key: &str) -> String {
    format!("{message} (rule {key} failed)")
}

/// `[model.no_overlap]`: two records with the same `key` fields may not have overlapping
/// intervals `[start, end)`. A record with an empty key field, an empty start or end, or a state
/// in `ignore_states` takes part in no comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoOverlap {
    /// The rule key (`assert_key`, label slot), as for a `[[model.rule]]`.
    pub rule: String,
    /// The fields two records must share to clash (one resource, one day).
    pub key: Vec<&'static str>,
    pub start: &'static str,
    pub end: &'static str,
    /// The select field `ignore_states` names options of (`state` unless declared).
    pub state_field: &'static str,
    pub ignore_states: Vec<&'static str>,
    /// `capacity = "<ref field of key>.<number field>"`: how many records of one key may overlap
    /// at any moment, read from the record the key field points to (a room that seats eight).
    /// Unset, or a value that is empty or below 1, means 1. Kept as (ref field, field).
    pub capacity: Option<(&'static str, &'static str)>,
    pub message: String,
    /// The message per language (`attach_messages`).
    pub messages: BTreeMap<String, String>,
}

impl NoOverlap {
    /// This rule with a capacity path `<ref field>.<number field>` (checked by the install gate).
    pub fn with_capacity(mut self, path: &str) -> Self {
        let (r, f) = path.split_once('.').unwrap_or((path, ""));
        self.capacity = Some((intern(r), intern(f)));
        self
    }

    /// The own fields the rule reads. An update checks it again when it moves the key, the start
    /// or the end, or leaves an ignored state (`engine::model_rules::rechecks`).
    pub fn reads(&self) -> Vec<&str> {
        let mut v: Vec<&str> = self.key.clone();
        v.extend([self.start, self.end]);
        if !self.ignore_states.is_empty() {
            v.push(self.state_field);
        }
        v
    }

    pub fn failure_in(&self, lang: Option<&str>) -> String {
        failure(pick(&self.messages, lang, &self.message), &self.rule)
    }
}

impl ModelDef {
    /// A rule of a built-in module; panics on a bad expression (a programming error, as for
    /// `FieldDef::computed`). Packages go through `ModelRule::parse` and the install gate.
    pub fn rule(mut self, key: &str, source: &str, message: &str) -> Self {
        let r = ModelRule::parse(key, source, message, Applies::Touched)
            .unwrap_or_else(|e| panic!("{}: bad rule {source:?}: {e}", self.name));
        self.ext.rules.push(r);
        self
    }

    /// This model with a no-overlap rule.
    pub fn with_no_overlap(mut self, n: NoOverlap) -> Self {
        self.ext.no_overlap = Some(n);
        self
    }

    pub fn model_rules(&self) -> &[ModelRule] {
        &self.ext.rules
    }

    pub fn no_overlap(&self) -> Option<&NoOverlap> {
        self.ext.no_overlap.as_ref()
    }

    /// The message of the rule `key` (a `[[model.rule]]` or the no-overlap rule), if there is one.
    pub fn rule_message(&self, key: &str) -> Option<&str> {
        self.ext
            .rules
            .iter()
            .find(|r| r.key == key)
            .map(|r| r.message.as_str())
            .or_else(|| {
                self.no_overlap()
                    .filter(|n| n.rule == key)
                    .map(|n| n.message.as_str())
            })
    }

    /// The keys of every rule, in declaration order, the no-overlap rule last.
    pub fn rule_keys(&self) -> Vec<&str> {
        let mut v: Vec<&str> = self.ext.rules.iter().map(|r| r.key.as_str()).collect();
        if let Some(n) = self.no_overlap() {
            v.push(n.rule.as_str());
        }
        v
    }
}

/// A `NoOverlap` from manifest names (interned).
pub fn no_overlap(
    rule: &str,
    key: &[String],
    start: &str,
    end: &str,
    state_field: Option<&str>,
    ignore_states: &[String],
    message: &str,
) -> NoOverlap {
    NoOverlap {
        rule: rule.to_string(),
        key: key.iter().map(|k| intern(k)).collect(),
        start: intern(start),
        end: intern(end),
        state_field: intern(state_field.unwrap_or("state")),
        ignore_states: ignore_states.iter().map(|s| intern(s)).collect(),
        capacity: None,
        message: message.to_string(),
        messages: BTreeMap::new(),
    }
}

/// Copy the translated rule messages of `t` onto the rules of `models`, so that the command layer
/// answers a refusal in the caller's language with the model definition alone.
pub fn attach_messages(models: &mut [ModelDef], t: &Translations) {
    for m in models {
        let Some(mt) = t.models.get(m.name) else {
            continue;
        };
        for r in &mut m.ext.rules {
            if let Some(texts) = mt.rules.get(&r.key) {
                r.messages = texts.clone();
            }
        }
        if let Some(n) = &mut m.ext.no_overlap
            && let Some(texts) = mt.rules.get(&n.rule)
        {
            n.messages = texts.clone();
        }
    }
}

/// The key and the message, the parts every rule has.
fn check_key_and_message(key: &str, message: &str, seen: &mut Vec<String>, out: &mut Vec<String>) {
    if !assert_key_ok(key) {
        out.push(format!(
            "the key {key:?} must be 1 to {MAX_ASSERT_KEY} lowercase letters or underscores"
        ));
    } else if seen.iter().any(|k| k == key) {
        out.push(format!("the key {key:?} is used twice in the model"));
    }
    seen.push(key.to_string());
    if let Err(e) = text_ok(message, MAX_ASSERT_MESSAGE_CHARS) {
        out.push(format!("the message {e}"));
    }
    // A text that starts like `limit_violated: ...` would be read as that code.
    if let Some((head, _)) = message.split_once(": ")
        && !head.is_empty()
        && head.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
    {
        out.push(format!(
            "the message must not start with {head:?} and a colon: that reads as an error code"
        ));
    }
}

/// Why `name` cannot be read by a rule of `m`, if it cannot: a rule reads stored, settable,
/// non-personal fields of its own model (D88 Decision 11, item 5). `personal_ok`: the key of a
/// no-overlap rule may be a person (the employee of a shift); its refusal states no value, only
/// that the time is taken, which the caller who names the person can read in the list anyway.
fn unreadable(m: &ModelDef, name: &str, personal_ok: bool) -> Option<String> {
    let Some(f) = m.fields.iter().find(|f| f.name == name) else {
        return Some(format!("{} has no field `{name}`", m.name));
    };
    if f.computed.is_some() || f.sequence.is_some() || m.is_system_field(name) {
        return Some(format!(
            "`{name}` is computed or assigned by the system; a rule reads stored fields only"
        ));
    }
    if matches!(f.kind, FieldKind::Image(_) | FieldKind::File(_)) {
        return Some(format!("`{name}` is an image or a file"));
    }
    if m.governed_fields().contains(&f.name) {
        return Some(format!(
            "`{name}` moves only through actions; a rule does not read the state (an action's assertion does)"
        ));
    }
    let class_ok = f
        .class
        .is_some_and(|c| c != obop_types::DataClass::Secret && !c.is_sensitive());
    if !personal_ok && (!class_ok || f.personal_data) {
        return Some(format!(
            "`{name}` is personal, secret, sensitive or unclassified data, which a rule may not read"
        ));
    }
    None
}

/// What the install gate (D41) says about the rules of `m`: one sentence per problem.
pub fn problems(m: &ModelDef, lookup: &dyn Fn(&str) -> Option<Arc<ModelDef>>) -> Vec<String> {
    let mut out = Vec::new();
    let count = m.ext.rules.len() + usize::from(m.ext.no_overlap.is_some());
    if count > MAX_RULES {
        out.push(format!(
            "a model has at most {MAX_RULES} rules, this has {count}"
        ));
    }
    let mut seen = Vec::new();
    for r in &m.ext.rules {
        let mut p = Vec::new();
        check_key_and_message(&r.key, &r.message, &mut seen, &mut p);
        let scope = Scope {
            model: m,
            lookup,
            allow_computed: false,
        };
        match expr::check(&r.expr, &scope) {
            Ok(Ty::Bool) => {}
            Ok(t) => p.push(format!(
                "`{}` is {}; it must be a comparison or other true/false expression",
                r.source,
                t.name()
            )),
            Err(e) => p.push(format!("`{}`: {e}", r.source)),
        }
        if !r.expr.aggregates().is_empty() {
            p.push(format!(
                "`{}` reads child records; a rule reads the record's own fields only",
                r.source
            ));
        }
        for name in r.reads() {
            if let Some(why) = unreadable(m, name, false) {
                p.push(why);
            }
        }
        out.extend(p.into_iter().map(|x| format!("rule {:?}: {x}", r.key)));
    }
    if let Some(n) = &m.ext.no_overlap {
        let mut p = Vec::new();
        check_key_and_message(&n.rule, &n.message, &mut seen, &mut p);
        if n.key.is_empty() {
            p.push("`key` names at least one field".into());
        }
        let kind_of = |name: &str| m.fields.iter().find(|f| f.name == name).map(|f| &f.kind);
        for k in &n.key {
            if let Some(why) = unreadable(m, k, true) {
                p.push(why);
            }
        }
        for (what, name) in [("start", n.start), ("end", n.end)] {
            match kind_of(name) {
                None => p.push(format!("`{what}`: {} has no field `{name}`", m.name)),
                Some(FieldKind::Number | FieldKind::Date | FieldKind::Datetime) => {
                    if let Some(why) = unreadable(m, name, false) {
                        p.push(why);
                    }
                }
                Some(k) => p.push(format!(
                    "`{what}` must be a number, date or datetime field, not {:?}",
                    k.name()
                )),
            }
        }
        if let (Some(a), Some(b)) = (kind_of(n.start), kind_of(n.end))
            && a != b
        {
            p.push("`start` and `end` must be fields of the same kind".into());
        }
        if !n.ignore_states.is_empty() {
            match kind_of(n.state_field) {
                Some(FieldKind::Select(opts)) => {
                    for s in &n.ignore_states {
                        if !opts.contains(s) {
                            p.push(format!(
                                "`ignore_states`: {} has no option {s:?}",
                                n.state_field
                            ));
                        }
                    }
                }
                _ => p.push(format!(
                    "`ignore_states` needs a select field `{}` (set `state_field`)",
                    n.state_field
                )),
            }
        }
        if let Some((via, field)) = n.capacity {
            let target = match kind_of(via) {
                Some(FieldKind::Ref(t)) if n.key.contains(&via) => lookup(t),
                _ => {
                    p.push(format!(
                        "`capacity` must be `<field>.<number field>` where the first is a reference field of `key`, not {via:?}"
                    ));
                    None
                }
            };
            if let Some(t) = target {
                match t.fields.iter().find(|f| f.name == field) {
                    Some(f) if matches!(f.kind, FieldKind::Number) && f.computed.is_none() => {}
                    _ => p.push(format!(
                        "`capacity`: {} has no stored number field {field:?}",
                        t.name
                    )),
                }
            }
        }
        out.extend(
            p.into_iter()
                .map(|x| format!("no_overlap {:?}: {x}", n.rule)),
        );
    }
    out
}

#[cfg(test)]
#[path = "model_rules_tests.rs"]
mod tests;
