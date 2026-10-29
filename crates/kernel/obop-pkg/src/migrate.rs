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

use crate::manifest::{FieldM, Manifest, MigrationM};
use crate::semver::{Req, Version};
use obop_storage::{Backend, Fields, Op};
use serde_json::{Value as J, json};
use std::collections::BTreeMap;

/// One thing that differs between two versions of a package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// `model_added`, `model_removed`, `field_added`, `field_removed`, `field_retyped`,
    /// `field_reshaped`, `field_limits`, `field_required`, `option_added`, `option_removed`, `action_added`,
    /// `action_removed`.
    pub kind: &'static str,
    pub model: String,
    pub field: Option<String>,
    pub detail: String,
    /// Whether the change can leave a stored record invalid on its own.
    pub breaking: bool,
    /// The migration step that resolves it, in words, when there is one.
    pub resolved_by: Option<String>,
}

/// The limits a field declares (D90) as one line, `none` when it declares none.
fn limits_text(f: &FieldM) -> String {
    let mut parts = Vec::new();
    let show = |v: &toml::Value| match v {
        toml::Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    if let Some(n) = f.min_length {
        parts.push(format!("min_length {n}"));
    }
    if let Some(n) = f.max_length {
        parts.push(format!("max_length {n}"));
    }
    if let Some(v) = &f.min {
        parts.push(format!("min {}", show(v)));
    }
    if let Some(v) = &f.max {
        parts.push(format!("max {}", show(v)));
    }
    if parts.is_empty() {
        "none".into()
    } else {
        parts.join(", ")
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
                        if limits_text(of) != limits_text(nf) {
                            out.push(ch(
                                "field_limits",
                                &nm.name,
                                Some(&nf.name),
                                format!(
                                    "{}: limits change from {} to {}; values already stored are not re-checked",
                                    nf.name,
                                    limits_text(of),
                                    limits_text(nf)
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

/// The steps of `new` that apply when upgrading from `from`, in declaration order.
pub fn steps_for(new: &Manifest, from: &Version) -> Vec<Step> {
    new.migration
        .iter()
        .filter(|m| Req::parse(&m.from).is_ok_and(|r| r.matches(from)))
        .map(|m| {
            let mut s = step_of(m);
            canonical_default(new, &mut s);
            carry_shape(new, &mut s);
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
        _ => None,
    }
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
    for s in steps {
        let rows = backend
            .list(&s.model)
            .map_err(|e| format!("reading {}: {e}", s.model))?;
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
                        let moved = match &s.value {
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
                other => return Err(format!("unknown migration op {other}")),
            }
        }
    }
    check_unique(steps, &order, &edits)?;
    Ok(order
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
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
