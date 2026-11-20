//! What an update does to existing records (D63).
//!
//! A new version of a package may add models, fields, options and actions on its own: records
//! stay as they are and the new parts start empty. A change that would leave a stored record
//! unable to satisfy the new shape is *breaking*: a removed or retyped field, a removed select
//! option, a removed model, a new required field with records already stored. A breaking change
//! is refused unless the new version declares a `[[migration]]` step that resolves it:
//!
//! - `rename_field`: copies the value to the new name and removes the old one;
//! - `set_default`: writes a value into every record where the field is empty;
//! - `drop_field`: deletes the values of a field that the new version no longer has
//!   (needs `confirm = true`);
//! - `reformat_field`: rewrites the stored values of a `decimal` or `datetime` field in the form
//!   of its current shape, after its `scale` or `precision` changed (D72). Digits that would be
//!   lost are rounded with the field's `round` mode, and without one the update stops.
//!
//! A value moved into a `decimal` or `datetime` field by `rename_field` is converted to that
//! field's canonical text on the way (and refused, naming the record, when it is not one).
//!
//! The steps run as one atomic batch together with the new package record, each guarded by a
//! compare-and-set on the value it read, so a record changed by a user in the meantime makes
//! the whole batch fail cleanly (and it is retried against the fresh data) instead of losing the
//! edit. Everything here is pure except `build_ops`, which reads records.

use crate::manifest::{FieldM, Manifest, MigrationM, probe};
use crate::semver::{Req, Version};
use obop_registry::{Bound, Pattern};
use obop_storage::{Backend, Fields, Op};
use serde_json::{Value as J, json};
use std::cmp::Ordering;
use std::collections::BTreeMap;

/// One thing that differs between two versions of a package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// `model_added`, `model_removed`, `field_added`, `field_removed`, `field_retyped`,
    /// `field_reshaped`, `field_limits`, `field_readonly`, `field_required`, `option_added`,
    /// `option_removed`, `action_added`, `action_removed`.
    pub kind: &'static str,
    pub model: String,
    pub field: Option<String>,
    pub detail: String,
    /// Whether the change can leave a stored record invalid on its own.
    pub breaking: bool,
    /// The migration step that resolves it, in words, when there is one.
    pub resolved_by: Option<String>,
}

/// The limits a field has (D90), declared and implied, for the plan: a text or longtext field that
/// declares no `max_length` still has the default of its kind, so "none" would read as unlimited.
#[derive(Debug, Clone)]
struct Held {
    min_length: Option<u32>,
    /// The effective maximum (the declared one, else the default of the kind) and whether it is
    /// the default.
    max_length: Option<(u32, bool)>,
    min: Option<Bound>,
    max: Option<Bound>,
    /// The named format (U3): the declared `pattern`, or a currency code for a text field named
    /// `currency`.
    pattern: Option<Pattern>,
}

impl Held {
    fn of(f: &FieldM) -> Self {
        let Ok(def) = probe(f) else {
            // Not parseable: the install gate refuses it anyway; show what is declared.
            return Self {
                min_length: f.min_length,
                max_length: f.max_length.map(|n| (n, false)),
                min: None,
                max: None,
                pattern: f.pattern.as_deref().and_then(Pattern::parse),
            };
        };
        // A computed or numbered field is never written: it has no default length.
        let written = f.computed.is_none() && f.kind != "sequence";
        let l = def.declared_limits();
        Self {
            min_length: l.min_length,
            max_length: if written {
                def.effective_max_length()
                    .map(|n| (n, l.max_length.is_none()))
            } else {
                l.max_length.map(|n| (n, false))
            },
            min: l.min,
            max: l.max,
            pattern: def.effective_pattern(),
        }
    }

    /// Whether both hold a value to the same limits. A declared `max_length` that equals the default
    /// holds it to the same one.
    fn same(&self, other: &Self) -> bool {
        (
            self.min_length,
            self.max_length.map(|m| m.0),
            self.min,
            self.max,
            self.pattern,
        ) == (
            other.min_length,
            other.max_length.map(|m| m.0),
            other.min,
            other.max,
            other.pattern,
        )
    }

    /// Nothing is held: no length (a number, a date, ...), no bound.
    fn is_none(&self) -> bool {
        self.min_length.is_none()
            && self.max_length.is_none()
            && self.min.is_none()
            && self.max.is_none()
            && self.pattern.is_none()
    }

    fn text(&self) -> String {
        if self.is_none() {
            return "none".into();
        }
        let mut parts = Vec::new();
        if let Some(n) = self.min_length {
            parts.push(format!("min_length {n}"));
        }
        match self.max_length {
            Some((n, true)) => parts.push(format!("max_length {n} (default)")),
            Some((n, false)) => parts.push(format!("max_length {n}")),
            None => {}
        }
        if let Some(b) = self.min {
            parts.push(format!("min {b}"));
        }
        if let Some(b) = self.max {
            parts.push(format!("max {b}"));
        }
        if let Some(p) = self.pattern {
            parts.push(format!("pattern {}", p.name()));
        }
        parts.join(", ")
    }

    /// Whether `new` holds a value to more than `self` does, to less, or both (one limit tighter
    /// and another looser), comparing what is held and not what is spelled.
    fn change_to(&self, new: &Self) -> &'static str {
        let (mut tighter, mut looser) = (false, false);
        let mut note = |o: Ordering| match o {
            Ordering::Greater => tighter = true,
            Ordering::Less => looser = true,
            Ordering::Equal => {}
        };
        note(tighter_upper(
            self.max_length.map(|m| m.0),
            new.max_length.map(|m| m.0),
            |a, b| Some(a.cmp(&b)),
        ));
        note(tighter_lower(self.min_length, new.min_length, |a, b| {
            Some(a.cmp(&b))
        }));
        note(tighter_upper(self.max, new.max, cmp_bound));
        note(tighter_lower(self.min, new.min, cmp_bound));
        // A format where there was none holds a value to more; another format is both.
        match (self.pattern, new.pattern) {
            (None, Some(_)) => tighter = true,
            (Some(_), None) => looser = true,
            (Some(a), Some(b)) if a != b => (tighter, looser) = (true, true),
            _ => {}
        }
        match (tighter, looser) {
            (true, false) => "tighten",
            (false, true) => "loosen",
            (true, true) => "tighten and loosen",
            (false, false) => "change",
        }
    }
}

/// `Greater` when `new` is the tighter of two upper limits (`None` is no limit), `Less` when it
/// is the looser.
fn tighter_upper<T: Copy>(
    old: Option<T>,
    new: Option<T>,
    cmp: impl Fn(T, T) -> Option<Ordering>,
) -> Ordering {
    match (old, new) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(o), Some(n)) => cmp(o, n).unwrap_or(Ordering::Equal),
    }
}

/// As [`tighter_upper`] for lower limits: the higher one is the tighter.
fn tighter_lower<T: Copy>(
    old: Option<T>,
    new: Option<T>,
    cmp: impl Fn(T, T) -> Option<Ordering>,
) -> Ordering {
    match (old, new) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(o), Some(n)) => cmp(o, n).map_or(Ordering::Equal, Ordering::reverse),
    }
}

fn cmp_bound(a: Bound, b: Bound) -> Option<Ordering> {
    match (a, b) {
        (Bound::Int(x), Bound::Int(y)) => Some(x.cmp(&y)),
        (Bound::Dec(x), Bound::Dec(y)) => Some(x.compare(y)),
        _ => None,
    }
}

/// The structural difference between the installed manifest and the new one.
pub fn diff(old: &Manifest, new: &Manifest) -> Vec<Change> {
    let mut out = Vec::new();
    let ch = |kind, model: &str, field: Option<&str>, detail: String, breaking| Change {
        kind,
        model: model.to_string(),
        field: field.map(String::from),
        detail,
        breaking,
        resolved_by: None,
    };
    for nm in &new.model {
        let Some(om) = old.model.iter().find(|m| m.name == nm.name) else {
            out.push(ch(
                "model_added",
                &nm.name,
                None,
                format!("new model {} with {} field(s)", nm.name, nm.field.len()),
                false,
            ));
            continue;
        };
        for nf in &nm.field {
            match om.field.iter().find(|f| f.name == nf.name) {
                None => out.push(ch(
                    "field_added",
                    &nm.name,
                    Some(&nf.name),
                    format!(
                        "new {} field {}{}",
                        nf.kind,
                        nf.name,
                        if nf.required { " (required)" } else { "" }
                    ),
                    nf.required && nf.computed.is_none(),
                )),
                Some(of) => {
                    if of.kind != nf.kind || of.to != nf.to {
                        out.push(ch(
                            "field_retyped",
                            &nm.name,
                            Some(&nf.name),
                            format!(
                                "{} changes from {}{} to {}{}",
                                nf.name,
                                of.kind,
                                of.to.as_ref().map_or(String::new(), |t| format!(" -> {t}")),
                                nf.kind,
                                nf.to.as_ref().map_or(String::new(), |t| format!(" -> {t}")),
                            ),
                            true,
                        ));
                    } else {
                        // A decimal keeps its kind when its digits change, but the stored text has
                        // the old number of them (D72): one spelling per value needs a rewrite.
                        // A computed decimal stores nothing, so there is nothing to rewrite.
                        if nf.kind == "decimal"
                            && of.computed.is_none()
                            && nf.computed.is_none()
                            && (of.precision, of.scale) != (nf.precision, nf.scale)
                        {
                            let shape = |f: &FieldM| {
                                format!(
                                    "decimal({}, {})",
                                    f.precision.unwrap_or(0),
                                    f.scale.unwrap_or(0)
                                )
                            };
                            out.push(ch(
                                "field_reshaped",
                                &nm.name,
                                Some(&nf.name),
                                format!("{} changes from {} to {}", nf.name, shape(of), shape(nf)),
                                // Widening the precision alone leaves every stored value valid.
                                of.scale != nf.scale || nf.precision < of.precision,
                            ));
                        }
                        // Limits (D90) only tighten or loosen what is written from now on; stored
                        // values are not re-checked, so this is shown and never blocks.
                        // What is held counts, not what is spelled: a text field that drops its
                        // `max_length` is held to the default of its kind from then on.
                        let (held_old, held_new) = (Held::of(of), Held::of(nf));
                        if !held_old.same(&held_new) {
                            out.push(ch(
                                "field_limits",
                                &nm.name,
                                Some(&nf.name),
                                format!(
                                    "{}: limits {} from {} to {}; values already stored are not re-checked",
                                    nf.name,
                                    held_old.change_to(&held_new),
                                    held_old.text(),
                                    held_new.text()
                                ),
                                false,
                            ));
                        }
                        // U23: a field callers could write becomes one only the server sets.
                        // Stored values stay valid, so it never blocks, but a create or update
                        // that names it is refused from now on (`readonly_field`).
                        let fixed = |f: &FieldM| f.readonly || f.computed.is_some();
                        if fixed(nf) && !fixed(of) {
                            out.push(ch(
                                "field_readonly",
                                &nm.name,
                                Some(&nf.name),
                                format!(
                                    "{} becomes {}: create and update may no longer set it",
                                    nf.name,
                                    if nf.computed.is_some() { "computed" } else { "readonly" }
                                ),
                                false,
                            ));
                        }
                        if nf.required && !of.required && nf.computed.is_none() {
                            out.push(ch(
                                "field_required",
                                &nm.name,
                                Some(&nf.name),
                                format!("{} becomes required", nf.name),
                                true,
                            ));
                        }
                        for o in &nf.options {
                            if !of.options.contains(o) {
                                out.push(ch(
                                    "option_added",
                                    &nm.name,
                                    Some(&nf.name),
                                    format!("{}: new option {o}", nf.name),
                                    false,
                                ));
                            }
                        }
                        for o in &of.options {
                            if !nf.options.contains(o) {
                                out.push(ch(
                                    "option_removed",
                                    &nm.name,
                                    Some(&nf.name),
                                    format!(
                                        "{}: option {o} is removed; records may still hold it",
                                        nf.name
                                    ),
                                    true,
                                ));
                            }
                        }
                    }
                }
            }
        }
        for of in &om.field {
            if !nm.field.iter().any(|f| f.name == of.name) {
                out.push(ch(
                    "field_removed",
                    &nm.name,
                    Some(&of.name),
                    format!("field {} is removed", of.name),
                    true,
                ));
            }
        }
        for a in &nm.action {
            if !om.action.iter().any(|x| x.name == a.name) {
                out.push(ch(
                    "action_added",
                    &nm.name,
                    None,
                    format!("new action {}", a.name),
                    false,
                ));
            }
        }
        for a in &om.action {
            if !nm.action.iter().any(|x| x.name == a.name) {
                out.push(ch(
                    "action_removed",
                    &nm.name,
                    None,
                    format!("action {} is removed", a.name),
                    false,
                ));
            }
        }
    }
    for om in &old.model {
        if !new.model.iter().any(|m| m.name == om.name) {
            out.push(ch(
                "model_removed",
                &om.name,
                None,
                format!(
                    "model {} is removed; its records would be orphaned",
                    om.name
                ),
                true,
            ));
        }
    }
    out
}

/// A migration step that applies to an upgrade from `from`.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub op: String,
    pub model: String,
    pub field: String,
    pub to: Option<String>,
    pub value: Option<J>,
    pub description: String,
}

impl Step {
    /// The step in words, for the plan and the audit entry.
    pub fn summary(&self) -> String {
        match self.op.as_str() {
            "rename_field" => format!(
                "rename {}.{} to {}",
                self.model,
                self.field,
                self.to.as_deref().unwrap_or("?")
            ),
            "set_default" => format!(
                "fill empty {}.{} with {}",
                self.model,
                self.field,
                self.value.as_ref().map_or("?".into(), J::to_string)
            ),
            "drop_field" => format!("delete the values of {}.{}", self.model, self.field),
            "reformat_field" => format!(
                "rewrite the values of {}.{} in the form of its current shape",
                self.model, self.field
            ),
            "fill_from" => format!(
                "fill empty {}.{} from {}.{}",
                self.model,
                self.field,
                self.value
                    .as_ref()
                    .and_then(|v| v["via"].as_str())
                    .unwrap_or("?"),
                self.value
                    .as_ref()
                    .and_then(|v| v["field"].as_str())
                    .unwrap_or("?"),
            ),
            "link_by_name" => format!(
                "set empty {}.{} to the {} whose name is {}.{} (left empty when no name or several match)",
                self.model,
                self.field,
                self.value
                    .as_ref()
                    .and_then(|v| v["model"].as_str())
                    .unwrap_or("?"),
                self.model,
                self.value
                    .as_ref()
                    .and_then(|v| v["source"].as_str())
                    .unwrap_or("?"),
            ),
            other => other.to_string(),
        }
    }

    pub fn from_json(v: &J) -> Option<Step> {
        let t = |k: &str| v.get(k).and_then(J::as_str).map(String::from);
        Some(Step {
            op: t("op")?,
            model: t("model")?,
            field: t("field")?,
            to: t("to"),
            value: v.get("value").filter(|x| !x.is_null()).cloned(),
            description: t("description").unwrap_or_default(),
        })
    }

    pub fn to_json(&self) -> J {
        json!({"op": self.op, "model": self.model, "field": self.field, "to": self.to,
               "value": self.value, "description": self.description, "summary": self.summary()})
    }
}

fn to_json(v: &toml::Value) -> J {
    match v {
        toml::Value::String(s) => J::from(s.as_str()),
        toml::Value::Integer(i) => J::from(*i),
        toml::Value::Float(f) => J::from(*f),
        toml::Value::Boolean(b) => J::from(*b),
        _ => J::Null,
    }
}

fn step_of(m: &MigrationM) -> Step {
    Step {
        op: m.op.clone(),
        model: m.model.clone(),
        field: m.field.clone(),
        to: m.to.clone(),
        value: m.value.as_ref().map(to_json),
        description: m.description.clone(),
    }
}

/// `fill_from` and `link_by_name`: what `build_ops` needs besides the step, carried in `value`
/// (the model the ref points at, and the field read there or the text field matched).
fn carry_link(new: &Manifest, m: &MigrationM, s: &mut Step) {
    let source = m.source.clone().unwrap_or_default();
    match s.op.as_str() {
        "fill_from" => {
            let (via, field) = source.split_once('.').unwrap_or((&source, ""));
            let model = field_of(new, &s.model, via).and_then(|f| f.to.clone());
            s.value = Some(json!({"via": via, "model": model, "field": field}));
        }
        "link_by_name" => {
            let model = field_of(new, &s.model, &s.field).and_then(|f| f.to.clone());
            s.value = Some(json!({"source": source, "model": model, "by": "name"}));
        }
        _ => {}
    }
}

/// The steps of `new` that apply when upgrading from `from`, in declaration order.
pub fn steps_for(new: &Manifest, from: &Version) -> Vec<Step> {
    new.migration
        .iter()
        .filter(|m| Req::parse(&m.from).is_ok_and(|r| r.matches(from)))
        .map(|m| {
            let mut s = step_of(m);
            canonical_default(new, &mut s);
            carry_shape(new, &mut s);
            carry_link(new, m, &mut s);
            s
        })
        .collect()
}

/// The shape of a decimal or datetime field as the steps carry it: the target of a
/// `rename_field`, or the field a `reformat_field` rewrites. `build_ops` has only the steps, so
/// the digits and the rounding mode travel in `value`.
fn shape_of(f: &FieldM) -> Option<J> {
    match f.kind.as_str() {
        "decimal" => Some(json!({
            "kind": "decimal",
            "precision": f.precision.unwrap_or(0),
            "scale": f.scale.unwrap_or(0),
            "round": f.round,
            "unique": f.unique,
            "unique_with": f.unique_with,
        })),
        "datetime" => Some(json!({
            "kind": "datetime",
            "unique": f.unique,
            "unique_with": f.unique_with,
        })),
        // Numbers typed by hand that a rename moves into a numbered field: the counter goes on
        // after the highest of them (`counter_after_rename`). A numbered field is always unique
        // (`obop_commands` `seq::unique_specs`), so two equal old texts are named before the
        // batch runs (`check_unique`) instead of failing on the index.
        "sequence" => Some(json!({
            "kind": "sequence",
            "format": f.format,
            "reset": f.reset,
            "unique": true,
        })),
        _ => None,
    }
}

/// The counter record a numbered field takes its numbers from (`obop_commands` `engine/seq.rs`:
/// the model `sequence_counter`, one record per model and field when the series never resets).
const SEQUENCE_COUNTER: &str = "sequence_counter";

/// The number `v` holds in the series `format` (`MO-{seq:05}`), when `v` is written in it: the
/// text before and after `{seq}` (or `{seq:N}`) as written, and digits in between. A format with
/// `{year}` or `{month}` gives none: such a series is counted per period.
fn number_in(format: &str, v: &str) -> Option<u64> {
    let start = format.find("{seq")?;
    let end = start + format[start..].find('}')? + 1;
    let (before, after) = (&format[..start], &format[end..]);
    if before.contains('{') || after.contains('{') {
        return None;
    }
    let digits = v.strip_prefix(before)?.strip_suffix(after)?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// After a rename into a numbered field that never resets, the counter of that field is set to
/// the highest number the moved values hold in its format (`MO-0018` in `MO-{seq:05}` is 18), so
/// the next record is numbered after them instead of starting at 1 beside them. A counter that
/// is already higher is left as it is.
fn counter_after_rename(
    backend: &dyn Backend,
    s: &Step,
    highest: u64,
) -> Result<Option<Op>, String> {
    let to = s.to.as_deref().unwrap_or_default();
    let key = format!("{}.{to}", s.model);
    let had = backend
        .get(SEQUENCE_COUNTER, &key)
        .map_err(|e| format!("reading the counter of {key}: {e}"))?;
    let now = had
        .as_ref()
        .and_then(|f| f.get("value"))
        .and_then(J::as_u64)
        .unwrap_or(0);
    if highest <= now {
        return Ok(None);
    }
    let mut fields = had.unwrap_or_default();
    fields.insert("value".into(), J::from(highest));
    fields.insert("model".into(), J::from(s.model.as_str()));
    fields.insert("field".into(), J::from(to));
    Ok(Some(Op::Put {
        model: SEQUENCE_COUNTER.into(),
        id: key,
        fields,
    }))
}

fn carry_shape(new: &Manifest, s: &mut Step) {
    let target = match s.op.as_str() {
        "rename_field" => s.to.as_deref(),
        "reformat_field" => Some(s.field.as_str()),
        _ => return,
    };
    if let Some(f) = target.and_then(|t| field_of(new, &s.model, t)) {
        s.value = shape_of(f);
    }
}

/// `v` written in the canonical form of `shape`: `Ok(None)` for an empty value, an error that
/// says why for one that is not a decimal or an instant of that shape. A field that declares
/// `round` rounds extra fractional digits, as an import does; any other refuses them.
fn in_shape(shape: &J, v: &J) -> Result<Option<J>, String> {
    let text = match v {
        J::String(s) if s.is_empty() => return Ok(None),
        J::String(s) => s.clone(),
        J::Number(n) if n.is_i64() => n.to_string(),
        other => return Err(format!("{other} is not a value of that kind")),
    };
    match shape["kind"].as_str() {
        Some("decimal") => {
            let (p, sc) = (
                shape["precision"].as_u64().unwrap_or(0) as u8,
                shape["scale"].as_u64().unwrap_or(0) as u8,
            );
            let round = shape["round"].as_str().and_then(obop_values::Round::parse);
            let shown = |e: obop_values::DecimalError| {
                format!("{text:?} does not fit decimal({p}, {sc}): {e}")
            };
            let d = obop_values::Decimal::parse(&text).map_err(shown)?;
            // Dropping digits that are all zero loses nothing and needs no rounding mode.
            let exact = d
                .rescale(sc, obop_values::Round::Down)
                .ok()
                .filter(|x| *x == d);
            match exact {
                Some(x) => x.fit(p, sc, obop_values::Round::Down),
                None => obop_values::Decimal::parse_for(&text, p, sc, round),
            }
            .map(|d| Some(J::from(d.to_string())))
            .map_err(shown)
        }
        Some("datetime") => obop_values::Instant::parse(&text)
            .map(|i| Some(J::from(i.canonical())))
            .map_err(|e| format!("{text:?} is not an instant: {e}")),
        _ => Ok(Some(v.clone())),
    }
}

/// A `set_default` into a decimal or datetime field writes the canonical text (D72): the
/// default is written straight into the records, past the checks of the commands, so
/// `"12.5"` into two decimals must already be `"12.50"` or the record would later be refused.
fn canonical_default(new: &Manifest, s: &mut Step) {
    if s.op != "set_default" {
        return;
    }
    let Some(f) = new
        .model
        .iter()
        .find(|m| m.name == s.model)
        .and_then(|m| m.field.iter().find(|f| f.name == s.field))
    else {
        return;
    };
    let text = match &s.value {
        Some(J::String(t)) => t.clone(),
        Some(J::Number(n)) if f.kind == "decimal" => n.to_string(),
        _ => return,
    };
    let canonical: Option<String> = match (f.kind.as_str(), f.precision, f.scale) {
        ("decimal", Some(p), Some(sc)) => {
            obop_values::Decimal::parse_for(&text, p as u8, sc as u8, None)
                .ok()
                .map(|d| d.to_string())
        }
        ("datetime", _, _) => obop_values::Instant::parse(&text)
            .ok()
            .map(obop_values::Instant::canonical),
        _ => return,
    };
    if let Some(c) = canonical {
        s.value = Some(J::from(c));
    }
}

/// The data side of an update: what differs, which steps run, what stops it.
#[derive(Debug, Clone, Default)]
pub struct DataPlan {
    pub changes: Vec<Change>,
    pub steps: Vec<Step>,
    /// Breaking changes no step resolves, in words.
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
}

fn field_of<'a>(m: &'a Manifest, model: &str, field: &str) -> Option<&'a FieldM> {
    m.model
        .iter()
        .find(|x| x.name == model)?
        .field
        .iter()
        .find(|f| f.name == field)
}

/// Judge the update from `old` (installed) to `new`. `count` says how many records a model has:
/// a new required field is only a problem where there are records to fill.
pub fn plan_data(old: &Manifest, new: &Manifest, count: &dyn Fn(&str) -> usize) -> DataPlan {
    let from = old.semver();
    let steps = steps_for(new, &from);
    let mut changes = diff(old, new);
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();
    for c in &mut changes {
        if !c.breaking {
            continue;
        }
        let field = c.field.clone().unwrap_or_default();
        // A model with no records has nothing a change could leave invalid.
        if count(&c.model) == 0 {
            c.resolved_by = Some(format!("no {} records exist yet", c.model));
            continue;
        }
        let resolver: Option<&Step> = match c.kind {
            "field_removed" => steps.iter().find(|s| {
                s.model == c.model
                    && s.field == field
                    && matches!(s.op.as_str(), "drop_field" | "rename_field")
            }),
            "field_reshaped" => steps
                .iter()
                .find(|s| s.model == c.model && s.field == field && s.op == "reformat_field"),
            "field_added" | "field_required" => steps
                .iter()
                .find(|s| s.model == c.model && s.field == field && s.op == "set_default")
                .or_else(|| {
                    // A field filled by a rename is not new data.
                    steps.iter().find(|s| {
                        s.model == c.model
                            && s.op == "rename_field"
                            && s.to.as_deref() == Some(field.as_str())
                    })
                }),
            _ => None,
        };
        if let Some(s) = resolver {
            c.resolved_by = Some(s.summary());
            continue;
        }
        let records = count(&c.model);
        match c.kind {
            "field_added" | "field_required" if records == 0 => {
                c.resolved_by = Some(format!("no {} records exist yet", c.model));
            }
            "field_added" | "field_required" => blockers.push(format!(
                "{}: {} has {records} record(s) with no value for {field}, which is required. Declare [[migration]] from = \"{}\", op = \"set_default\", model = \"{}\", field = \"{field}\", value = ... to fill them, or make the field optional",
                c.model, c.model, old.module.version, c.model
            )),
            "field_removed" => blockers.push(format!(
                "{}: field {field} is removed. Declare a [[migration]] with op = \"drop_field\" and confirm = true to delete its values, or op = \"rename_field\" if it moved",
                c.model
            )),
            "field_reshaped" => blockers.push(format!(
                "{}: {} — the stored values keep the old number of digits. Declare [[migration]] from = \"{}\", op = \"reformat_field\", model = \"{}\", field = \"{field}\" to rewrite them (digits that would be lost need `round` on the field; a value that still does not fit stops the update)",
                c.model, c.detail, old.module.version, c.model
            )),
            "field_retyped" => blockers.push(format!(
                "{}: {} — a stored value cannot be converted automatically. Keep the old field, add the new one under another name, and move the data with your own import",
                c.model, c.detail
            )),
            "option_removed" => blockers.push(format!(
                "{}: {} Keep the option, or move the records to another option first",
                c.model, c.detail
            )),
            "model_removed" => blockers.push(format!(
                "{}: {}. An update cannot remove a model; uninstall the package instead (its data stays in the database)",
                c.model, c.detail
            )),
            _ => {}
        }
    }
    // Steps that do not match anything in the old version are harmless but worth a word.
    for s in &steps {
        let old_has = field_of(old, &s.model, &s.field).is_some();
        match s.op.as_str() {
            "rename_field" | "drop_field" if !old_has => warnings.push(format!(
                "migration: {} has no field {} in version {}; the step does nothing",
                s.model, s.field, old.module.version
            )),
            _ => {}
        }
        if s.op == "drop_field" {
            warnings.push(format!(
                "this update deletes the values of {}.{}; it cannot be rolled back (a backup can)",
                s.model, s.field
            ));
        }
        if s.op == "reformat_field" {
            warnings.push(format!(
                "this update rewrites the values of {}.{} in the form of its new shape; it cannot be rolled back (a backup can)",
                s.model, s.field
            ));
        }
    }
    for c in &changes {
        if c.kind == "field_added"
            && !c.breaking
            && field_of(new, &c.model, c.field.as_deref().unwrap_or("")).is_some_and(|f| f.unique)
            && count(&c.model) > 1
        {
            warnings.push(format!(
                "{}.{} is unique: empty values of existing records count as equal",
                c.model,
                c.field.clone().unwrap_or_default()
            ));
        }
    }
    DataPlan {
        changes,
        steps,
        blockers,
        warnings,
    }
}

/// The inverse of an update's steps, kept with the package so it can be rolled back: renames
/// reversed, defaults left (a filled value does no harm), and a list of what cannot be undone.
pub fn inverse(steps: &[Step]) -> (Vec<Step>, Vec<String>) {
    let mut inv = Vec::new();
    let mut irreversible = Vec::new();
    for s in steps.iter().rev() {
        match s.op.as_str() {
            "rename_field" => inv.push(Step {
                op: "rename_field".into(),
                model: s.model.clone(),
                field: s.to.clone().unwrap_or_default(),
                to: Some(s.field.clone()),
                value: None,
                description: format!("undo: {}", s.summary()),
            }),
            "drop_field" | "reformat_field" => irreversible.push(s.summary()),
            _ => {}
        }
    }
    (inv, irreversible)
}

fn is_empty(v: Option<&J>) -> bool {
    match v {
        None | Some(J::Null) => true,
        Some(J::String(s)) => s.is_empty(),
        _ => false,
    }
}

/// A record as read, as it will be, and what must change to get there.
struct Edit {
    orig: Fields,
    cur: Fields,
    patch: Fields,
}

/// A rewrite that rounds, or a rename that writes canonical text, can turn two different stored
/// texts into one. Where the target field is `unique`, that would break the promise the update
/// must keep, so it is refused, naming both records.
fn check_unique(
    steps: &[Step],
    order: &[(String, String)],
    edits: &BTreeMap<(String, String), Edit>,
) -> Result<(), String> {
    for s in steps {
        let Some(shape) = &s.value else { continue };
        if shape["unique"].as_bool() != Some(true) {
            continue;
        }
        let target = if s.op == "rename_field" {
            s.to.as_deref().unwrap_or("")
        } else {
            s.field.as_str()
        };
        let scope: Vec<&str> = shape["unique_with"]
            .as_array()
            .map(|a| a.iter().filter_map(J::as_str).collect())
            .unwrap_or_default();
        // value (with the values of the fields that scope it) -> first record, and whether a
        // step changed it
        let mut seen: BTreeMap<String, (&str, bool)> = BTreeMap::new();
        for key in order.iter().filter(|k| k.0 == s.model) {
            let Some(e) = edits.get(key) else { continue };
            let Some(v) = e.cur.get(target).filter(|v| !is_empty(Some(v))) else {
                continue;
            };
            let mut id = v.to_string();
            for w in &scope {
                id.push('\u{1}');
                id.push_str(&e.cur.get(*w).map(J::to_string).unwrap_or_default());
            }
            let changed = e.patch.contains_key(target);
            match seen.get(&id) {
                Some((first, first_changed)) if changed || *first_changed => {
                    return Err(format!(
                        "{} {first} and {} would both hold {v} in {target} after this update, and {target} is unique; fix one of them first",
                        s.model, key.1
                    ));
                }
                Some(_) => {}
                None => {
                    seen.insert(id, (key.1.as_str(), changed));
                }
            }
        }
    }
    Ok(())
}

/// A fresh install over records that an earlier install left behind (an uninstall keeps the
/// data, and the package record with its old shape is gone): the stored decimals and instants
/// must already be in the form of this version's fields, or the install is refused.
pub fn check_stored_shapes(backend: &dyn Backend, new: &Manifest) -> Result<(), String> {
    for nm in &new.model {
        let shapes: Vec<(&FieldM, J)> = nm
            .field
            .iter()
            .filter(|f| f.computed.is_none())
            .filter_map(|f| shape_of(f).map(|s| (f, s)))
            .collect();
        if shapes.is_empty() {
            continue;
        }
        // A refused count is not "no records": the check of what is stored must not be skipped.
        let held = backend
            .count(&nm.name)
            .map_err(|e| format!("counting {}: {e}", nm.name))?;
        if held == 0 {
            continue;
        }
        let rows = backend
            .list(&nm.name)
            .map_err(|e| format!("reading {}: {e}", nm.name))?;
        for (id, rec) in rows {
            for (f, shape) in &shapes {
                let Some(v) = rec.get(&f.name).filter(|v| !v.is_null()) else {
                    continue;
                };
                if !matches!(in_shape(shape, v), Ok(Some(ref n)) if n == v) {
                    let want = match f.kind.as_str() {
                        "decimal" => format!(
                            "decimal({}, {})",
                            f.precision.unwrap_or(0),
                            f.scale.unwrap_or(0)
                        ),
                        _ => "an instant in canonical UTC form".to_string(),
                    };
                    return Err(format!(
                        "{} {id} holds {v} in {}, which is not in the form of {want}: the records of an earlier install are still there, so install the version that wrote them first and update it to this one with a [[migration]] op = \"reformat_field\" for {}.{}",
                        nm.name, f.name, nm.name, f.name
                    ));
                }
            }
        }
    }
    Ok(())
}

/// The storage operations for `steps`: one compare-and-set per record that needs a change,
/// guarded by the values the record held when it was read. The whole list is applied in one
/// transaction by the caller.
pub fn build_ops(backend: &dyn Backend, steps: &[Step]) -> Result<Vec<Op>, String> {
    let mut edits: BTreeMap<(String, String), Edit> = BTreeMap::new();
    let mut order: Vec<(String, String)> = Vec::new();
    let mut counters: Vec<Op> = Vec::new();
    for s in steps {
        let rows = backend
            .list(&s.model)
            .map_err(|e| format!("reading {}: {e}", s.model))?;
        // A rename into a numbered field that never resets: the format its numbers are read in.
        let series = s
            .value
            .as_ref()
            .filter(|v| s.op == "rename_field" && v["kind"] == "sequence")
            .filter(|v| v["reset"].as_str().is_none_or(|r| r == "never"))
            .and_then(|v| v["format"].as_str());
        let mut highest = 0u64;
        // `fill_from` reads the record each ref points at; `link_by_name` looks a name up among
        // the records of the target, and a name that is no record's, or several records', links
        // nothing (it is never guessed).
        let link_arg = |k: &str| {
            s.value
                .as_ref()
                .and_then(|v| v[k].as_str())
                .unwrap_or_default()
                .to_string()
        };
        let mut targets: BTreeMap<String, Fields> = BTreeMap::new();
        let mut names: BTreeMap<String, Vec<String>> = BTreeMap::new();
        if matches!(s.op.as_str(), "fill_from" | "link_by_name") {
            let model = link_arg("model");
            if model.is_empty() {
                return Err(format!("{}: the ref {} names no model", s.op, s.field));
            }
            for (tid, f) in backend
                .list(&model)
                .map_err(|e| format!("reading {model}: {e}"))?
            {
                if let Some(n) = f.get("name").and_then(J::as_str) {
                    names
                        .entry(n.trim().to_string())
                        .or_default()
                        .push(tid.clone());
                }
                targets.insert(tid, f);
            }
        }
        for (id, orig) in rows {
            let rec_id = id.clone();
            let key = (s.model.clone(), id);
            if !edits.contains_key(&key) {
                order.push(key.clone());
            }
            let e = edits.entry(key).or_insert_with(|| Edit {
                orig: orig.clone(),
                cur: orig,
                patch: Fields::new(),
            });
            let set = |e: &mut Edit, k: &str, v: J| {
                e.cur.insert(k.to_string(), v.clone());
                e.patch.insert(k.to_string(), v);
            };
            match s.op.as_str() {
                "rename_field" => {
                    let to = s.to.as_deref().ok_or("rename_field without a target")?;
                    if let Some(v) = e.cur.get(&s.field).cloned().filter(|v| !v.is_null()) {
                        // Into a decimal or datetime field a value goes in its canonical text.
                        if let Some(n) = series.zip(v.as_str()).and_then(|(f, t)| number_in(f, t)) {
                            highest = highest.max(n);
                        }
                        let moved = match &s.value {
                            Some(shape) if shape["kind"] == "sequence" => Some(v),
                            Some(shape) => in_shape(shape, &v).map_err(|why| {
                                format!("{} {}: {} -> {to}: {why}", s.model, rec_id, s.field)
                            })?,
                            None => Some(v),
                        };
                        // Never overwrite a value already under the new name.
                        if is_empty(e.cur.get(to))
                            && let Some(v) = moved
                        {
                            set(e, to, v);
                        }
                        set(e, &s.field, J::Null);
                    }
                }
                "reformat_field" => {
                    let shape = s.value.as_ref().ok_or("reformat_field without a shape")?;
                    if let Some(v) = e.cur.get(&s.field).cloned().filter(|v| !v.is_null())
                        && let Some(nv) = in_shape(shape, &v)
                            .map_err(|why| format!("{} {}: {}: {why}", s.model, rec_id, s.field))?
                        && nv != v
                    {
                        set(e, &s.field, nv);
                    }
                }
                "set_default" => {
                    if is_empty(e.cur.get(&s.field)) {
                        set(e, &s.field, s.value.clone().unwrap_or(J::Null));
                    }
                }
                "drop_field" => {
                    if e.cur.get(&s.field).is_some_and(|v| !v.is_null()) {
                        set(e, &s.field, J::Null);
                    }
                }
                "fill_from" => {
                    if is_empty(e.cur.get(&s.field))
                        && let Some(J::String(via)) = e.cur.get(&link_arg("via")).cloned()
                        && let Some(v) = targets.get(&via).and_then(|t| t.get(&link_arg("field")))
                        && !is_empty(Some(v))
                    {
                        set(e, &s.field, v.clone());
                    }
                }
                "link_by_name" => {
                    if is_empty(e.cur.get(&s.field))
                        && let Some(J::String(name)) = e.cur.get(&link_arg("source")).cloned()
                        && let Some([one]) = names.get(name.trim()).map(Vec::as_slice)
                    {
                        set(e, &s.field, J::from(one.as_str()));
                    }
                }
                other => return Err(format!("unknown migration op {other}")),
            }
        }
        if series.is_some()
            && highest > 0
            && let Some(op) = counter_after_rename(backend, s, highest)?
        {
            counters.push(op);
        }
    }
    check_unique(steps, &order, &edits)?;
    let records: Vec<Op> = order
        .into_iter()
        .filter_map(|key| {
            let e = edits.remove(&key)?;
            if e.patch.is_empty() {
                return None;
            }
            let expect = e
                .patch
                .keys()
                .map(|k| (k.clone(), e.orig.get(k).cloned().unwrap_or(J::Null)))
                .collect();
            Some(Op::Patch {
                model: key.0,
                id: key.1,
                patch: e.patch,
                expect,
            })
        })
        .collect();
    Ok(records.into_iter().chain(counters).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_is_read_in_its_series_only_when_written_in_it() {
        assert_eq!(number_in("MO-{seq:05}", "MO-0018"), Some(18));
        assert_eq!(number_in("MO-{seq:05}", "MO-00042"), Some(42));
        assert_eq!(number_in("{seq}/A", "7/A"), Some(7));
        assert_eq!(number_in("MO-{seq:05}", "MO-12a"), None);
        assert_eq!(number_in("MO-{seq:05}", "WO-0001"), None);
        assert_eq!(number_in("MO-{seq:05}", "MO-"), None);
        assert_eq!(number_in("MO-{year}-{seq}", "MO-2026-0001"), None);
    }

    #[test]
    fn a_rename_into_a_numbered_field_moves_the_counter_past_the_moved_numbers() {
        let new = m(
            "0.3.0",
            "[[model.field]]\nname = \"number\"\ndescription = \"d\"\nclass = \"none\"\nkind = \"sequence\"\nformat = \"MO-{seq:05}\"",
            "[[migration]]\nfrom = \"<0.3.0\"\nop = \"rename_field\"\nmodel = \"mig_item\"\nfield = \"reference\"\nto = \"number\"\ndescription = \"d\"",
        );
        let steps = steps_for(&new, &Version::new(0, 2, 0));
        let b = obop_storage::open("memory:").unwrap();
        for (id, r) in [
            ("1", "MO-0003"),
            ("2", "MO-0018"),
            ("3", "by hand"),
            ("4", ""),
        ] {
            let mut f = Fields::new();
            f.insert("reference".into(), J::from(r));
            b.apply_checked(&[Op::Insert {
                model: "mig_item".into(),
                id: id.into(),
                fields: f,
            }])
            .unwrap();
        }
        b.apply_checked(&build_ops(&*b, &steps).unwrap()).unwrap();
        let c = b.get(SEQUENCE_COUNTER, "mig_item.number").unwrap().unwrap();
        assert_eq!(c["value"], 18);
        assert_eq!(
            b.get("mig_item", "2").unwrap().unwrap()["number"],
            "MO-0018"
        );
        // Run again (a retried update): a counter already past them stays where it is.
        let mut f = c.clone();
        f.insert("value".into(), J::from(25));
        b.apply_checked(&[Op::Put {
            model: SEQUENCE_COUNTER.into(),
            id: "mig_item.number".into(),
            fields: f,
        }])
        .unwrap();
        let again = build_ops(&*b, &steps).unwrap();
        assert!(again.iter().all(|o| !matches!(o, Op::Put { .. })));
    }

    #[test]
    fn a_rename_into_a_numbered_field_names_two_records_that_would_share_a_number() {
        // A numbered field is unique (`obop_commands` `seq::unique_specs`), so two old
        // references typed alike would break its index; the update says which two.
        let new = m(
            "0.3.0",
            "[[model.field]]\nname = \"number\"\ndescription = \"d\"\nclass = \"none\"\nkind = \"sequence\"\nformat = \"MO-{seq:05}\"",
            "[[migration]]\nfrom = \"<0.3.0\"\nop = \"rename_field\"\nmodel = \"mig_item\"\nfield = \"reference\"\nto = \"number\"\ndescription = \"d\"",
        );
        let steps = steps_for(&new, &Version::new(0, 2, 0));
        let b = obop_storage::open("memory:").unwrap();
        for (id, r) in [("11", "MO-0011"), ("12", "MO-0011"), ("13", "MO-0013")] {
            b.insert("mig_item", id, &rec(&[("reference", json!(r))]))
                .unwrap();
        }
        let e = build_ops(&*b, &steps).unwrap_err();
        assert!(
            e.contains("mig_item 11 and 12") && e.contains("MO-0011") && e.contains("unique"),
            "{e}"
        );
    }

    fn m(version: &str, fields: &str, migrations: &str) -> Manifest {
        Manifest::parse(&format!(
            "[module]\nid = \"mig\"\nversion = \"{version}\"\nname = \"M\"\ndescription = \"d\"\nlicense = \"MIT\"\nrequires_core = \"^0.2\"\n[[model]]\nname = \"mig_item\"\ndescription = \"d\"\nread_group = \"mig.user\"\nwrite_group = \"mig.user\"\n{fields}\n{migrations}"
        ))
        .unwrap()
    }

    fn field(name: &str, extra: &str) -> String {
        format!(
            "[[model.field]]\nname = \"{name}\"\ndescription = \"d\"\nclass = \"none\"\n{extra}\n"
        )
    }

    fn rec(pairs: &[(&str, J)]) -> Fields {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn additive_changes_need_nothing() {
        let old = m("0.1.0", &field("title", ""), "");
        let new = m(
            "0.2.0",
            &format!(
                "{}{}{}",
                field("title", ""),
                field("note", ""),
                field("kind", "kind = \"select\"\noptions = [\"a\"]")
            ),
            "",
        );
        let p = plan_data(&old, &new, &|_| 100);
        assert!(p.blockers.is_empty(), "{:?}", p.blockers);
        assert_eq!(
            p.changes.iter().filter(|c| c.kind == "field_added").count(),
            2
        );
        assert!(p.changes.iter().all(|c| !c.breaking));
    }

    #[test]
    fn breaking_changes_are_refused_without_a_step() {
        let old = m(
            "0.1.0",
            &format!("{}{}", field("title", ""), field("legacy", "")),
            "",
        );
        let new = m(
            "0.2.0",
            &format!(
                "{}{}",
                field("title", "kind = \"number\""),
                field("code", "required = true")
            ),
            "",
        );
        let p = plan_data(&old, &new, &|_| 3);
        let all = p.blockers.join("\n");
        assert!(all.contains("legacy is removed"), "{all}");
        assert!(all.contains("cannot be converted automatically"), "{all}");
        assert!(all.contains("no value for code"), "{all}");
        // With no records nothing can be left invalid, whatever changed.
        let p = plan_data(&old, &new, &|_| 0);
        assert!(p.blockers.is_empty(), "{:?}", p.blockers);
    }

    #[test]
    fn steps_resolve_the_breaking_changes_they_name() {
        let old = m(
            "0.1.0",
            &format!("{}{}", field("title", ""), field("legacy", "")),
            "",
        );
        let new = m(
            "0.2.0",
            &format!("{}{}", field("title", ""), field("code", "required = true")),
            "[[migration]]\nfrom = \"<0.2.0\"\nop = \"drop_field\"\nmodel = \"mig_item\"\nfield = \"legacy\"\nconfirm = true\n[[migration]]\nfrom = \"<0.2.0\"\nop = \"set_default\"\nmodel = \"mig_item\"\nfield = \"code\"\nvalue = \"n/a\"\n",
        );
        let p = plan_data(&old, &new, &|_| 5);
        assert!(p.blockers.is_empty(), "{:?}", p.blockers);
        assert!(
            p.warnings
                .iter()
                .any(|w| w.contains("cannot be rolled back"))
        );
        assert_eq!(p.steps.len(), 2);
        // A step for another starting version does not apply.
        let p = plan_data(&m("0.3.0", &field("title", ""), ""), &new, &|_| 5);
        assert!(p.steps.is_empty());
    }

    #[test]
    fn a_field_that_becomes_readonly_or_computed_is_shown_and_never_blocks() {
        // U23: hr 0.4.1 to 0.5.0 made `hr_leave_request.requested_by` readonly.
        let old = m("0.1.0", &field("by", ""), "");
        let ro = m("0.2.0", &field("by", "readonly = true"), "");
        let p = plan_data(&old, &ro, &|_| 5);
        let c: Vec<&Change> = p.changes.iter().filter(|c| c.kind == "field_readonly").collect();
        assert_eq!(c.len(), 1, "{:?}", p.changes);
        assert_eq!(c[0].field.as_deref(), Some("by"));
        assert!(!c[0].breaking && p.blockers.is_empty(), "{:?}", p.blockers);
        assert_eq!(
            c[0].detail,
            "by becomes readonly: create and update may no longer set it"
        );
        // Unchanged, or the other way round: nothing to show.
        assert!(diff(&ro, &ro).is_empty());
        assert!(!diff(&ro, &old).iter().any(|c| c.kind == "field_readonly"));
    }

    fn dec(p: u8, s: u8, extra: &str) -> String {
        field(
            "price",
            &format!("kind = \"decimal\"\nprecision = {p}\nscale = {s}\n{extra}"),
        )
    }

    fn step_to(from: &str, op: &str, field: &str, to: Option<&str>) -> String {
        format!(
            "[[migration]]\nfrom = \"{from}\"\nop = \"{op}\"\nmodel = \"mig_item\"\nfield = \"{field}\"\n{}",
            to.map_or(String::new(), |t| format!("to = \"{t}\"\n"))
        )
    }

    #[test]
    fn a_changed_decimal_shape_is_found_and_needs_a_rewrite_step() {
        let old = m("0.1.0", &dec(8, 2, ""), "");
        let has = |new: &Manifest, breaking: bool| {
            diff(&old, new)
                .iter()
                .any(|c| c.kind == "field_reshaped" && c.breaking == breaking)
        };
        // The scale changes either way, the precision narrows: stored text no longer has the
        // shape of the field.
        assert!(has(&m("0.2.0", &dec(8, 4, ""), ""), true), "scale up");
        assert!(has(&m("0.2.0", &dec(8, 1, ""), ""), true), "scale down");
        assert!(has(&m("0.2.0", &dec(6, 2, ""), ""), true), "precision down");
        // Wider precision alone leaves every stored value valid and canonical; the rounding
        // mode only concerns imports.
        assert!(has(&m("0.2.0", &dec(12, 2, ""), ""), false), "precision up");
        assert!(diff(&old, &m("0.2.0", &dec(8, 2, "round = \"half_up\""), "")).is_empty());
        assert!(diff(&old, &m("0.2.0", &dec(8, 2, ""), "")).is_empty());
        // Without a step the update is refused, saying which step; with records only.
        let wider = m("0.2.0", &dec(8, 4, ""), "");
        let p = plan_data(&old, &wider, &|_| 3);
        assert_eq!(p.blockers.len(), 1, "{:?}", p.blockers);
        assert!(
            p.blockers[0].contains("reformat_field") && p.blockers[0].contains("decimal(8, 2)"),
            "{}",
            p.blockers[0]
        );
        assert!(plan_data(&old, &wider, &|_| 0).blockers.is_empty());
        // The step resolves it, and the plan warns that it cannot be rolled back.
        let fixed = m(
            "0.2.0",
            &dec(8, 4, ""),
            &step_to("<0.2.0", "reformat_field", "price", None),
        );
        let p = plan_data(&old, &fixed, &|_| 3);
        assert!(p.blockers.is_empty(), "{:?}", p.blockers);
        assert!(
            p.warnings
                .iter()
                .any(|w| w.contains("cannot be rolled back"))
        );
        assert_eq!(p.steps.len(), 1);
        assert_eq!(p.steps[0].value.as_ref().unwrap()["scale"], 4);
    }

    #[test]
    fn a_computed_decimal_stores_nothing_so_its_digits_may_change_freely() {
        let calc = |p: u8, s: u8| m("0.2.0", &dec(p, s, "computed = \"1.5\""), "");
        let old = m("0.1.0", &dec(8, 2, "computed = \"1.5\""), "");
        for new in [calc(8, 4), calc(8, 1), calc(4, 2)] {
            assert!(diff(&old, &new).is_empty(), "{:?}", diff(&old, &new));
            let p = plan_data(&old, &new, &|_| 9);
            assert!(
                p.blockers.is_empty() && p.steps.is_empty(),
                "{:?}",
                p.blockers
            );
        }
        // Changing a stored decimal into a computed one, or back, leaves nothing to rewrite
        // either: the computed side has no stored value.
        let stored = m("0.1.0", &dec(8, 2, ""), "");
        assert!(
            diff(&stored, &calc(8, 4))
                .iter()
                .all(|c| c.kind != "field_reshaped")
        );
        assert!(
            diff(&old, &m("0.2.0", &dec(8, 4, ""), ""))
                .iter()
                .all(|c| c.kind != "field_reshaped")
        );
        // And a rewrite step for a computed field is refused when the package is read.
        let e = Manifest::parse(&format!(
            "[module]\nid = \"mig\"\nversion = \"0.2.0\"\nname = \"M\"\ndescription = \"d\"\nlicense = \"MIT\"\nrequires_core = \"^0.2\"\n[[model]]\nname = \"mig_item\"\ndescription = \"d\"\nread_group = \"mig.user\"\nwrite_group = \"mig.user\"\n{}\n{}",
            dec(8, 4, "computed = \"1.5\""),
            step_to("<0.2.0", "reformat_field", "price", None)
        ))
        .err()
        .unwrap_or_default();
        assert!(e.contains("computed and stores nothing"), "{e}");
    }

    #[test]
    fn a_rewrite_that_makes_two_values_one_is_refused_where_the_field_is_unique() {
        let run =
            |b: &dyn Backend, new: &Manifest| build_ops(b, &steps_for(new, &Version::new(0, 1, 0)));
        let mem = || {
            let b = obop_storage::open("memory:").unwrap();
            // Two different four-digit values that round to the same two-digit one.
            b.insert("mig_item", "1", &rec(&[("price", json!("12.5049"))]))
                .unwrap();
            b.insert("mig_item", "2", &rec(&[("price", json!("12.5001"))]))
                .unwrap();
            b.insert("mig_item", "3", &rec(&[("price", json!("7.0000"))]))
                .unwrap();
            b
        };
        let step = step_to("<0.2.0", "reformat_field", "price", None);
        let unique = m(
            "0.2.0",
            &dec(10, 2, "round = \"half_up\"\nunique = true"),
            &step,
        );
        let e = run(&*mem(), &unique).unwrap_err();
        assert!(
            e.contains("mig_item 1 and 2") && e.contains("12.50") && e.contains("unique"),
            "{e}"
        );
        // Without `unique` the same rewrite is fine, and so is a unique field that stays distinct.
        let plain = m("0.2.0", &dec(10, 2, "round = \"half_up\""), &step);
        assert_eq!(run(&*mem(), &plain).unwrap().len(), 3);
        let b = mem();
        b.update("mig_item", "2", &rec(&[("price", json!("12.6001"))]))
            .unwrap();
        assert_eq!(run(&*b, &unique).unwrap().len(), 3);
        // A rename into a unique decimal writes canonical text, which can also make two values
        // one ("1" and "1.0" are two texts and one number).
        let b = obop_storage::open("memory:").unwrap();
        b.insert("mig_item", "1", &rec(&[("p", json!("1"))]))
            .unwrap();
        b.insert("mig_item", "2", &rec(&[("p", json!("1.0"))]))
            .unwrap();
        let renamed = m(
            "0.2.0",
            &dec(8, 2, "unique = true"),
            &step_to("<0.2.0", "rename_field", "p", Some("price")),
        );
        let e = run(&*b, &renamed).unwrap_err();
        assert!(e.contains("mig_item 1 and 2") && e.contains("1.00"), "{e}");
    }

    #[test]
    fn reformat_field_must_name_a_decimal_or_datetime_field_of_the_package() {
        let bad = |fields: &str, mig: &str| {
            Manifest::parse(&format!(
                "[module]\nid = \"mig\"\nversion = \"0.2.0\"\nname = \"M\"\ndescription = \"d\"\nlicense = \"MIT\"\nrequires_core = \"^0.2\"\n[[model]]\nname = \"mig_item\"\ndescription = \"d\"\nread_group = \"mig.user\"\nwrite_group = \"mig.user\"\n{fields}\n{mig}"
            ))
            .err()
            .unwrap_or_default()
        };
        let step = step_to("<0.2.0", "reformat_field", "price", None);
        assert!(bad(&dec(8, 2, ""), &step).is_empty());
        assert!(bad(&field("price", ""), &step).contains("for decimal and datetime fields"));
        assert!(bad(&field("other", ""), &step).contains("is not a field of mig_item"));
        assert!(
            bad(&dec(8, 2, ""), &format!("{step}value = \"1\"\n")).contains("takes no other key")
        );
    }

    #[test]
    fn reformat_rewrites_stored_values_and_rounds_only_where_the_field_says() {
        let b = obop_storage::open("memory:").unwrap();
        for (id, v) in [("1", "12.50"), ("2", "0.05"), ("3", "-3.00")] {
            b.insert("mig_item", id, &rec(&[("price", json!(v))]))
                .unwrap();
        }
        b.insert("mig_item", "4", &rec(&[("title", json!("no price"))]))
            .unwrap();
        let run = |new: &Manifest| {
            let steps = steps_for(new, &Version::new(0, 1, 0));
            build_ops(&*b, &steps)
                .and_then(|ops| b.apply_checked(&ops).map_err(|e| format!("{e:?}")))
        };
        // Wider scale: padded.
        let wide = m(
            "0.2.0",
            &dec(10, 4, ""),
            &step_to("<0.2.0", "reformat_field", "price", None),
        );
        run(&wide).unwrap();
        let price = |id: &str| {
            b.get("mig_item", id)
                .unwrap()
                .unwrap()
                .get("price")
                .cloned()
        };
        assert_eq!(price("1"), Some(json!("12.5000")));
        assert_eq!(price("3"), Some(json!("-3.0000")));
        assert_eq!(price("4"), None, "empty stays empty");
        // Narrower scale needs `round`; without it the update stops and names the record.
        b.update("mig_item", "2", &rec(&[("price", json!("0.0549"))]))
            .unwrap();
        let narrow = m(
            "0.3.0",
            &dec(10, 2, ""),
            &step_to("<0.3.0", "reformat_field", "price", None),
        );
        let steps = steps_for(&narrow, &Version::new(0, 2, 0));
        let e = build_ops(&*b, &steps).unwrap_err();
        assert!(
            e.contains("mig_item 2") && e.contains("price") && e.contains("fractional"),
            "{e}"
        );
        let rounding = m(
            "0.3.0",
            &dec(10, 2, "round = \"half_up\""),
            &step_to("<0.3.0", "reformat_field", "price", None),
        );
        b.update("mig_item", "1", &rec(&[("price", json!("12.5050"))]))
            .unwrap();
        let steps = steps_for(&rounding, &Version::new(0, 2, 0));
        b.apply_checked(&build_ops(&*b, &steps).unwrap()).unwrap();
        assert_eq!(price("1"), Some(json!("12.51")));
        assert_eq!(price("2"), Some(json!("0.05")));
        assert_eq!(price("3"), Some(json!("-3.00")));
        // A narrower precision that a value exceeds is refused as well.
        b.update("mig_item", "3", &rec(&[("price", json!("12345678.00"))]))
            .unwrap();
        let small = m(
            "0.4.0",
            &dec(4, 2, ""),
            &step_to("<0.4.0", "reformat_field", "price", None),
        );
        let steps = steps_for(&small, &Version::new(0, 3, 0));
        assert!(build_ops(&*b, &steps).unwrap_err().contains("mig_item 3"));
    }

    #[test]
    fn a_renamed_value_goes_into_a_decimal_or_datetime_field_in_canonical_text() {
        let b = obop_storage::open("memory:").unwrap();
        b.insert(
            "mig_item",
            "1",
            &rec(&[
                ("p", json!("12.5")),
                ("at", json!("2026-03-29T03:30:00+02:00")),
            ]),
        )
        .unwrap();
        b.insert("mig_item", "2", &rec(&[("p", json!(7)), ("at", json!(""))]))
            .unwrap();
        let new = m(
            "0.2.0",
            &format!("{}{}", dec(8, 2, ""), field("seen", "kind = \"datetime\"")),
            &format!(
                "{}{}",
                step_to("<0.2.0", "rename_field", "p", Some("price")),
                step_to("<0.2.0", "rename_field", "at", Some("seen"))
            ),
        );
        let steps = steps_for(&new, &Version::new(0, 1, 0));
        b.apply_checked(&build_ops(&*b, &steps).unwrap()).unwrap();
        let one = b.get("mig_item", "1").unwrap().unwrap();
        assert_eq!(one["price"], "12.50");
        assert_eq!(one["seen"], "2026-03-29T01:30:00.000000Z");
        assert!(one["p"].is_null() && one["at"].is_null());
        let two = b.get("mig_item", "2").unwrap().unwrap();
        assert_eq!(two["price"], "7.00", "a whole number is a decimal");
        assert!(
            two.get("seen").is_none_or(J::is_null),
            "an empty text moves nothing"
        );
        // A value that is not of the kind stops the update and names the record.
        b.insert("mig_item", "3", &rec(&[("p", json!("twelve"))]))
            .unwrap();
        let e = build_ops(&*b, &steps).unwrap_err();
        assert!(
            e.contains("mig_item 3") && e.contains("p -> price") && e.contains("twelve"),
            "{e}"
        );
        b.delete("mig_item", "3").unwrap();
        b.insert("mig_item", "4", &rec(&[("at", json!("2026-03-29"))]))
            .unwrap();
        let e = build_ops(&*b, &steps).unwrap_err();
        assert!(e.contains("mig_item 4") && e.contains("at -> seen"), "{e}");
        // Into a text field the value still moves untouched.
        b.delete("mig_item", "4").unwrap();
        let plain = m(
            "0.2.0",
            &field("label", ""),
            &step_to("<0.2.0", "rename_field", "p", Some("label")),
        );
        let steps = steps_for(&plain, &Version::new(0, 1, 0));
        assert!(steps[0].value.is_none());
    }

    #[test]
    fn a_default_for_a_decimal_or_an_instant_is_written_canonically() {
        let new = m(
            "0.2.0",
            &format!(
                "{}{}{}",
                field("title", ""),
                field("price", "kind = \"decimal\"\n  precision = 8\n  scale = 2"),
                field("seen", "kind = \"datetime\"")
            ),
            "[[migration]]\nfrom = \"<0.2.0\"\nop = \"set_default\"\nmodel = \"mig_item\"\nfield = \"price\"\nvalue = \"12.5\"\n[[migration]]\nfrom = \"<0.2.0\"\nop = \"set_default\"\nmodel = \"mig_item\"\nfield = \"seen\"\nvalue = \"2026-03-29T03:30:00+02:00\"\n[[migration]]\nfrom = \"<0.2.0\"\nop = \"set_default\"\nmodel = \"mig_item\"\nfield = \"title\"\nvalue = \"12.5\"\n",
        );
        let steps = steps_for(&new, &Version::new(0, 1, 0));
        let values: Vec<_> = steps.iter().map(|s| s.value.clone().unwrap()).collect();
        assert_eq!(
            values,
            [
                json!("12.50"),
                json!("2026-03-29T01:30:00.000000Z"),
                json!("12.5")
            ],
            "only decimal and datetime defaults are rewritten; text stays as written"
        );
    }

    #[test]
    fn rename_moves_values_and_set_default_fills_only_the_empty() {
        let b = obop_storage::open("memory:").unwrap();
        b.insert(
            "mig_item",
            "1",
            &rec(&[("old", json!("x")), ("code", json!(""))]),
        )
        .unwrap();
        b.insert(
            "mig_item",
            "2",
            &rec(&[("old", json!("y")), ("code", json!("keep"))]),
        )
        .unwrap();
        b.insert("mig_item", "3", &rec(&[("other", json!(1))]))
            .unwrap();
        let steps = vec![
            Step {
                op: "rename_field".into(),
                model: "mig_item".into(),
                field: "old".into(),
                to: Some("new".into()),
                value: None,
                description: String::new(),
            },
            Step {
                op: "set_default".into(),
                model: "mig_item".into(),
                field: "code".into(),
                to: None,
                value: Some(json!("n/a")),
                description: String::new(),
            },
        ];
        let ops = build_ops(&*b, &steps).unwrap();
        b.apply_checked(&ops).unwrap();
        let r1 = b.get("mig_item", "1").unwrap().unwrap();
        assert_eq!(r1["new"], "x");
        assert!(r1["old"].is_null());
        assert_eq!(r1["code"], "n/a");
        let r2 = b.get("mig_item", "2").unwrap().unwrap();
        assert_eq!(r2["new"], "y");
        assert_eq!(r2["code"], "keep");
        // A record with neither field gets only the default.
        let r3 = b.get("mig_item", "3").unwrap().unwrap();
        assert_eq!(r3["code"], "n/a");
        assert!(!r3.contains_key("new"));
        // Running it again changes nothing.
        assert!(build_ops(&*b, &steps[1..]).unwrap().is_empty());
    }

    /// A field that became computed keeps no stored value of its own, so its old values may
    /// move to a new stored field; a field still stored may not be renamed away.
    #[test]
    fn the_stored_values_of_a_field_that_became_computed_can_be_renamed() {
        let num = "kind = \"number\"";
        let parse = |fields: String| {
            Manifest::parse(&format!(
                "[module]\nid = \"mig\"\nversion = \"0.2.0\"\nname = \"M\"\ndescription = \"d\"\nlicense = \"MIT\"\nrequires_core = \"^0.2\"\n[[model]]\nname = \"mig_item\"\ndescription = \"d\"\nread_group = \"mig.user\"\nwrite_group = \"mig.user\"\n{fields}\n{}",
                step_to("<0.2.0", "rename_field", "total", Some("kept"))
            ))
        };
        let computed = format!("{num}\ncomputed = \"if(is_set(kept), kept, 1)\"");
        assert!(parse(field("total", &computed) + &field("kept", num)).is_ok());
        let e = parse(field("total", num) + &field("kept", num)).unwrap_err();
        assert!(e.contains("still a stored field"), "{e}");
    }

    #[test]
    fn a_record_edited_meanwhile_fails_the_batch_and_changes_nothing() {
        let b = obop_storage::open("memory:").unwrap();
        b.insert("mig_item", "1", &rec(&[("old", json!("x"))]))
            .unwrap();
        b.insert("mig_item", "2", &rec(&[("old", json!("y"))]))
            .unwrap();
        let steps = vec![Step {
            op: "rename_field".into(),
            model: "mig_item".into(),
            field: "old".into(),
            to: Some("new".into()),
            value: None,
            description: String::new(),
        }];
        let ops = build_ops(&*b, &steps).unwrap();
        // A user edits record 2 after the plan read it.
        b.update("mig_item", "2", &rec(&[("old", json!("edited"))]))
            .unwrap();
        assert!(b.apply_checked(&ops).is_err());
        let r1 = b.get("mig_item", "1").unwrap().unwrap();
        assert_eq!(r1["old"], "x", "the first record was rolled back too");
        assert!(!r1.contains_key("new"));
    }

    #[test]
    fn renames_invert_and_drops_do_not() {
        let steps = vec![
            Step {
                op: "rename_field".into(),
                model: "a".into(),
                field: "x".into(),
                to: Some("y".into()),
                value: None,
                description: String::new(),
            },
            Step {
                op: "drop_field".into(),
                model: "a".into(),
                field: "z".into(),
                to: None,
                value: None,
                description: String::new(),
            },
        ];
        let (inv, irr) = inverse(&steps);
        assert_eq!(inv.len(), 1);
        assert_eq!(
            (inv[0].field.as_str(), inv[0].to.as_deref()),
            ("y", Some("x"))
        );
        assert_eq!(irr.len(), 1);
    }

    #[test]
    fn migration_steps_are_checked_when_the_package_is_read() {
        let bad = |migration: &str| {
            Manifest::parse(&format!(
                "[module]\nid = \"mig\"\nversion = \"0.2.0\"\nname = \"M\"\ndescription = \"d\"\nrequires_core = \"^0.2\"\n[[model]]\nname = \"mig_item\"\ndescription = \"d\"\nread_group = \"mig.user\"\nwrite_group = \"mig.user\"\n{}{}",
                field("title", ""),
                migration
            ))
            .unwrap_err()
        };
        let head = "[[migration]]\nfrom = \"<0.2.0\"\nmodel = \"mig_item\"\n";
        assert!(
            bad(&format!("{head}op = \"drop_field\"\nfield = \"gone\"\n"))
                .contains("confirm = true")
        );
        assert!(
            bad(&format!(
                "{head}op = \"drop_field\"\nfield = \"title\"\nconfirm = true\n"
            ))
            .contains("nothing to drop")
        );
        assert!(
            bad(&format!("{head}op = \"rename_field\"\nfield = \"gone\"\n")).contains("needs `to`")
        );
        assert!(
            bad(&format!(
                "{head}op = \"rename_field\"\nfield = \"gone\"\nto = \"nope\"\n"
            ))
            .contains("not a field")
        );
        assert!(
            bad(&format!(
                "{head}op = \"set_default\"\nfield = \"title\"\nvalue = 3\n"
            ))
            .contains("does not fit")
        );
        assert!(
            bad(&format!("{head}op = \"explode\"\nfield = \"title\"\n")).contains("unknown op")
        );
        assert!(bad("[[migration]]\nfrom = \"nope\"\nop = \"drop_field\"\nmodel = \"mig_item\"\nfield = \"q\"\n").contains("from"));
        assert!(bad("[[migration]]\nfrom = \"<1\"\nop = \"drop_field\"\nmodel = \"other\"\nfield = \"q\"\n").contains("declares no model"));
    }

    /// Two models: `mig_kind` (a name and a product) and `mig_item` (a ref to a kind, a product
    /// copied from it, a free-text head and its ref), with `migrations` added.
    fn linked(version: &str, migrations: &str) -> Manifest {
        let f = |name: &str, extra: &str| field(name, extra);
        Manifest::parse(&format!(
            "[module]\nid = \"mig\"\nversion = \"{version}\"\nname = \"M\"\ndescription = \"d\"\nlicense = \"MIT\"\nrequires_core = \"^0.2\"\n\
             [[model]]\nname = \"mig_kind\"\ndescription = \"d\"\nread_group = \"mig.user\"\nwrite_group = \"mig.user\"\n{}{}\
             [[model]]\nname = \"mig_item\"\ndescription = \"d\"\nread_group = \"mig.user\"\nwrite_group = \"mig.user\"\n{}{}{}{}\n{migrations}",
            f("name", ""),
            f("product", "kind = \"ref\"\nto = \"product\""),
            f("kind_id", "kind = \"ref\"\nto = \"mig_kind\""),
            f("product", "kind = \"ref\"\nto = \"product\"\nfill_from = \"kind_id.product\""),
            f("head", ""),
            f("head_id", "kind = \"ref\"\nto = \"mig_kind\""),
        ))
        .unwrap()
    }

    const FILL: &str = "[[migration]]\nfrom = \"<0.2.0\"\nop = \"fill_from\"\nmodel = \"mig_item\"\nfield = \"product\"\nsource = \"kind_id.product\"\n";
    const LINK: &str = "[[migration]]\nfrom = \"<0.2.0\"\nop = \"link_by_name\"\nmodel = \"mig_item\"\nfield = \"head_id\"\nsource = \"head\"\n";

    /// Q8: records made before a field had its `fill_from` get the value through the ref, and a
    /// value already there is kept.
    #[test]
    fn fill_from_copies_a_value_through_a_ref_into_empty_fields_only() {
        let b = obop_storage::open("memory:").unwrap();
        b.insert(
            "mig_kind",
            "k1",
            &rec(&[("name", json!("Drill")), ("product", json!("p1"))]),
        )
        .unwrap();
        b.insert("mig_kind", "k2", &rec(&[("name", json!("Saw"))]))
            .unwrap();
        b.insert("mig_item", "a", &rec(&[("kind_id", json!("k1"))]))
            .unwrap();
        b.insert(
            "mig_item",
            "b",
            &rec(&[("kind_id", json!("k1")), ("product", json!("p9"))]),
        )
        .unwrap();
        b.insert("mig_item", "c", &rec(&[("kind_id", json!("k2"))]))
            .unwrap();
        b.insert("mig_item", "d", &rec(&[])).unwrap();
        let new = linked("0.2.0", FILL);
        let steps = steps_for(&new, &Version::parse("0.1.0").unwrap());
        assert_eq!(steps.len(), 1);
        assert_eq!(
            steps[0].summary(),
            "fill empty mig_item.product from kind_id.product"
        );
        b.apply(&build_ops(&*b, &steps).unwrap()).unwrap();
        let product = |id: &str| {
            b.get("mig_item", id)
                .unwrap()
                .unwrap()
                .get("product")
                .cloned()
        };
        assert_eq!(product("a"), Some(json!("p1")));
        assert_eq!(
            product("b"),
            Some(json!("p9")),
            "a value is never overwritten"
        );
        assert_eq!(product("c"), None, "the kind has no product");
        assert_eq!(product("d"), None, "no kind");
        // Run again: nothing left to do.
        assert!(build_ops(&*b, &steps).unwrap().is_empty());
        // Nothing to undo: a filled value stays.
        assert!(inverse(&steps).0.is_empty() && inverse(&steps).1.is_empty());
    }

    /// R8: a ref set from the text that names its record; a name that matches no record or
    /// several is left alone.
    #[test]
    fn link_by_name_sets_a_ref_from_a_unique_name_and_never_guesses() {
        let b = obop_storage::open("memory:").unwrap();
        for (id, name) in [("k1", "Anna Berg"), ("k2", "Ola Lind"), ("k3", "Ola Lind")] {
            b.insert("mig_kind", id, &rec(&[("name", json!(name))]))
                .unwrap();
        }
        b.insert("mig_item", "a", &rec(&[("head", json!(" Anna Berg "))]))
            .unwrap();
        b.insert("mig_item", "b", &rec(&[("head", json!("Ola Lind"))]))
            .unwrap();
        b.insert("mig_item", "c", &rec(&[("head", json!("Nobody"))]))
            .unwrap();
        b.insert(
            "mig_item",
            "d",
            &rec(&[("head", json!("Anna Berg")), ("head_id", json!("k2"))]),
        )
        .unwrap();
        let new = linked("0.2.0", LINK);
        let steps = steps_for(&new, &Version::parse("0.1.0").unwrap());
        assert!(steps[0].summary().contains("whose name is mig_item.head"));
        b.apply(&build_ops(&*b, &steps).unwrap()).unwrap();
        let head = |id: &str| {
            b.get("mig_item", id)
                .unwrap()
                .unwrap()
                .get("head_id")
                .cloned()
        };
        assert_eq!(head("a"), Some(json!("k1")), "trimmed, exact");
        assert_eq!(head("b"), None, "two records have that name");
        assert_eq!(head("c"), None, "no record has that name");
        assert_eq!(head("d"), Some(json!("k2")), "a ref already set is kept");
        assert_eq!(
            b.get("mig_item", "a").unwrap().unwrap().get("head"),
            Some(&json!(" Anna Berg ")),
            "the text stays"
        );
    }

    #[test]
    fn fill_from_and_link_by_name_are_checked_when_the_package_is_read() {
        let bad = |steps: &str| {
            let f = |name: &str, extra: &str| field(name, extra);
            Manifest::parse(&format!(
                "[module]\nid = \"mig\"\nversion = \"0.2.0\"\nname = \"M\"\ndescription = \"d\"\nlicense = \"MIT\"\nrequires_core = \"^0.2\"\n\
                 [[model]]\nname = \"mig_kind\"\ndescription = \"d\"\nread_group = \"mig.user\"\nwrite_group = \"mig.user\"\n{}{}\
                 [[model]]\nname = \"mig_item\"\ndescription = \"d\"\nread_group = \"mig.user\"\nwrite_group = \"mig.user\"\n{}{}{}{}\n{steps}",
                f("name", ""),
                f("product", "kind = \"ref\"\nto = \"product\""),
                f("kind_id", "kind = \"ref\"\nto = \"mig_kind\""),
                f("product", "kind = \"ref\"\nto = \"product\""),
                f("head", ""),
                f("head_id", "kind = \"ref\"\nto = \"mig_kind\""),
            ))
            .unwrap_err()
        };
        let step = |op: &str, field: &str, source: Option<&str>| {
            format!(
                "[[migration]]\nfrom = \"<0.2.0\"\nop = \"{op}\"\nmodel = \"mig_item\"\nfield = \"{field}\"\n{}",
                source.map_or(String::new(), |s| format!("source = \"{s}\"\n"))
            )
        };
        assert!(bad(&step("fill_from", "product", None)).contains("needs `source"));
        assert!(bad(&step("fill_from", "product", Some("head.product"))).contains("not a ref"));
        assert!(bad(&step("fill_from", "product", Some("kind_id.nope"))).contains("not a field"));
        assert!(bad(&step("fill_from", "head", Some("kind_id.product"))).contains("kinds must"));
        assert!(bad(&step("link_by_name", "head", Some("head"))).contains("not a ref field"));
        assert!(bad(&step("link_by_name", "head_id", Some("nope"))).contains("needs `source`"));
        assert!(
            bad(&format!(
                "{}value = 1\n",
                step("link_by_name", "head_id", Some("head"))
            ))
            .contains("takes only `source`")
        );
        assert!(
            bad("[[migration]]\nfrom = \"<0.2.0\"\nop = \"set_default\"\nmodel = \"mig_item\"\nfield = \"head\"\nvalue = \"x\"\nsource = \"head\"\n")
                .contains("`source` belongs to")
        );
    }
}
