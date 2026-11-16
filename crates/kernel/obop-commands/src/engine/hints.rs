//! The rules of the D95 hints on write, the same on every surface (REST, MCP, the CLI, an import
//! and an action's effects): what the server fills in on create (`default`, `fill_from`), the
//! fields only the server writes (`readonly`), what a picked record must satisfy (`ref_filter`),
//! and the two action rules (`needs`, `not_by`).
//!
//! - A default or a `fill_from` applies only to a field the write leaves out (a value the caller
//!   sends, `null` included, is the caller's).
//! - `fill_from` copies a value only when the caller could read it in clear: the field's group,
//!   the record's visibility and the masking of its data class (D40). Otherwise the field stays
//!   empty, exactly as if the source were empty, so the answer is no oracle for what it holds.
//! - A `fill_from` into the model's `access.own` field is applied again by an update that moves
//!   the record to another source and leaves the owner out (`refill_owner`): the record follows.
//! - `readonly` refuses a value on create and a changed value on update (code `readonly_field`);
//!   the record sent back as it is passes. Defaults, `fill_from` and effects still write it.

use super::access::Gate;
use super::computed::Computer;
use super::unit::{R, Reads};
use super::{Actor, Engine, civil};
use crate::errors::{code, coded};
use obop_masking::{PrincipalKind, mode_for};
use obop_registry::ui::{CmpOp, DefaultLit, DefaultSpec, FilterValue, open_state_field};
use obop_registry::{CommandDef, FieldDef, ModelDef};
use obop_storage::Fields;
use obop_types::{DataClass, MaskMode};
use serde_json::Value as J;
use std::sync::Arc;

fn is_unset(v: Option<&J>) -> bool {
    match v {
        None | Some(J::Null) => true,
        Some(J::String(s)) => s.is_empty(),
        _ => false,
    }
}

/// Whether the caller reads `f` in clear: holds its group and sees it unmasked.
pub(crate) fn reads_clear(actor: &Actor, f: &FieldDef) -> bool {
    let kind = if actor.agent {
        PrincipalKind::Agent
    } else {
        PrincipalKind::User
    };
    actor.holds(f.permission_group)
        && mode_for(kind, f.class.unwrap_or(DataClass::Secret), f.personal_data) == MaskMode::Allow
}

/// Today in the platform zone (D72), as `YYYY-MM-DD`.
pub(crate) fn today(eng: &Engine) -> String {
    obop_values::Instant::from_micros(eng.now_micros())
        .map_or_else(|| civil(eng.now()).date(), |at| eng.zones().today(at))
}

/// `date` plus `days`.
pub(crate) fn add_days(date: &str, days: u32) -> String {
    let n = |r: std::ops::Range<usize>| date.get(r).and_then(|s| s.parse::<i64>().ok());
    let (Some(y), Some(m), Some(d)) = (n(0..4), n(5..7), n(8..10)) else {
        return date.to_string();
    };
    // Days from the civil date (Howard Hinnant's algorithm), the inverse of `civil`.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days_since = era * 146_097 + doe - 719_468 + i64::from(days);
    civil(days_since * 86_400).date()
}

/// What a fill needs to know besides the record.
pub(crate) struct FillCtx<'a> {
    pub eng: &'a Engine,
    pub reads: &'a dyn Reads,
    pub look: &'a dyn Fn(&str) -> Option<Arc<ModelDef>>,
    pub actor: &'a Actor,
    pub today: &'a str,
}

/// Fill the defaults and then the `fill_from` fields of a record being created, for the fields
/// it leaves out.
pub(crate) fn fill_in(cx: &FillCtx<'_>, def: &ModelDef, f: &mut Fields) -> R<()> {
    let mut defaults = Vec::new();
    for fd in &def.fields {
        let Some(h) = &fd.hints else { continue };
        let Some(d) = &h.default else { continue };
        if f.contains_key(fd.name) {
            continue;
        }
        if let Some(v) = default_value(cx, fd, d)? {
            defaults.push((fd.name, v));
        }
    }
    for (k, v) in defaults {
        f.insert(k.to_string(), v);
    }
    for fd in &def.fields {
        let Some(h) = &fd.hints else { continue };
        let Some(p) = &h.fill_from else { continue };
        let Some(via) = &p.via else { continue };
        if f.contains_key(fd.name) || is_unset(f.get(via.as_str())) {
            continue;
        }
        if let Some(v) = copied(cx, def, via, &p.field, f)? {
            f.insert(fd.name.to_string(), v);
        }
    }
    Ok(())
}

/// The owner (the model's `access.own` field) that its `fill_from` gives again when an update
/// moves the record to another source (the `via` ref changes) and does not name the owner itself:
/// leave moved to another employee becomes that employee's. When nothing can be copied the
/// caller owns it, as on create (`own_default`).
pub(crate) fn refill_owner(
    cx: &FillCtx<'_>,
    def: &ModelDef,
    patch: &Fields,
    changed: &Fields,
    after: &Fields,
) -> R<Option<(String, J)>> {
    let Some(own) = def.access.as_ref().and_then(|a| a.own) else {
        return Ok(None);
    };
    let Some(p) = def
        .fields
        .iter()
        .find(|x| x.name == own)
        .and_then(|x| x.hints.as_ref())
        .and_then(|h| h.fill_from.as_ref())
    else {
        return Ok(None);
    };
    let Some(via) = &p.via else {
        return Ok(None);
    };
    if patch.contains_key(own) || !changed.contains_key(via.as_str()) {
        return Ok(None);
    }
    let copied = if is_unset(after.get(via.as_str())) {
        None
    } else {
        copied(cx, def, via, &p.field, after)?
    };
    let v = copied.or_else(|| (!cx.actor.id.is_empty()).then(|| J::from(cx.actor.id.clone())));
    Ok(v.map(|v| (own.to_string(), v)))
}

fn default_value(cx: &FillCtx<'_>, fd: &FieldDef, d: &DefaultSpec) -> R<Option<J>> {
    Ok(match d {
        DefaultSpec::Literal(DefaultLit::Text(t)) => Some(J::from(t.clone())),
        DefaultSpec::Literal(DefaultLit::Int(n)) => Some(match fd.kind {
            // A whole number into a decimal is that decimal's text (canonicalised after).
            obop_registry::FieldKind::Decimal { .. } => J::from(n.to_string()),
            _ => J::from(*n),
        }),
        DefaultSpec::Literal(DefaultLit::Bool(b)) => Some(J::from(*b)),
        DefaultSpec::Today(n) => Some(J::from(add_days(cx.today, *n))),
        DefaultSpec::User => (!cx.actor.id.is_empty()).then(|| J::from(cx.actor.id.clone())),
        DefaultSpec::InstanceCurrency => Some(J::from(cx.eng.currency())),
        DefaultSpec::Open(m) => {
            let Some(target) = (cx.look)(m) else {
                return Ok(None);
            };
            let Some(state) = open_state_field(&target) else {
                return Ok(None);
            };
            let gate = Gate::new(cx.reads, cx.look, cx.actor);
            let open: Vec<(String, Fields)> = cx
                .reads
                .find(target.name, state, &J::from("open"))?
                .into_iter()
                .filter(|(_, r)| gate.visible(&target, r))
                .collect();
            // Exactly one open record the caller sees; none or several leave the field empty.
            match open.as_slice() {
                [(id, _)] if cx.actor.holds(target.read_group) => Some(J::from(id.clone())),
                _ => None,
            }
        }
    })
}

/// The value of `field` of the record `via` points at, when the caller may read it in clear.
fn copied(cx: &FillCtx<'_>, def: &ModelDef, via: &str, field: &str, f: &Fields) -> R<Option<J>> {
    let Some(to) = def
        .fields
        .iter()
        .find(|x| x.name == via)
        .and_then(|x| x.kind.target())
    else {
        return Ok(None);
    };
    let Some(target) = (cx.look)(to) else {
        return Ok(None);
    };
    let Some(src) = target.fields.iter().find(|x| x.name == field) else {
        return Ok(None);
    };
    if !cx.actor.holds(target.read_group) || !reads_clear(cx.actor, src) {
        return Ok(None);
    }
    let Some(J::String(id)) = f.get(via) else {
        return Ok(None);
    };
    let gate = Gate::new(cx.reads, cx.look, cx.actor);
    let mut rec = match cx.reads.get(target.name, id)? {
        Some(rec) if gate.visible(&target, &rec) => rec,
        // Not there or not visible: the ref check of the write says so in its own words.
        _ => return Ok(None),
    };
    if src.computed.is_some() {
        Computer::new(cx.reads, cx.look, false).fill(&target, id, &mut rec);
    }
    Ok(rec.get(field).filter(|v| !v.is_null()).cloned())
}

/// Refuse a value of a `readonly` field: any non-null value on create (`stored` is `None`), a
/// changed one on update.
pub(crate) fn refuse_readonly(def: &ModelDef, given: &Fields, stored: Option<&Fields>) -> R<()> {
    for fd in def.fields.iter().filter(|f| f.is_readonly()) {
        let Some(v) = given.get(fd.name) else {
            continue;
        };
        let same = match stored {
            None => v.is_null(),
            Some(s) => s.get(fd.name) == Some(v) || (v.is_null() && is_unset(s.get(fd.name))),
        };
        if !same {
            return Err(coded(
                code::READONLY_FIELD,
                &format!(
                    "{}.{} is set by the server and cannot be written",
                    def.name, fd.name
                ),
            ));
        }
    }
    Ok(())
}

fn num(v: &J) -> Option<f64> {
    match v {
        J::Number(n) => n.as_f64(),
        // Decimals are canonical text (D72): they compare as numbers.
        J::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn ordering(a: &J, b: &J) -> Option<std::cmp::Ordering> {
    if let (Some(x), Some(y)) = (num(a), num(b)) {
        return x.partial_cmp(&y);
    }
    match (a, b) {
        (J::String(x), J::String(y)) => Some(x.cmp(y)),
        _ => None,
    }
}

fn equal(a: Option<&J>, b: &J) -> bool {
    match (a, b) {
        (None | Some(J::Null), J::Null | J::Bool(false)) => true,
        (Some(x), y) => {
            x == y || (num(x).is_some() && ordering(x, y) == Some(std::cmp::Ordering::Equal))
        }
        _ => false,
    }
}

/// Check the `ref_filter` of every ref field the write sets: on create every set ref, on update
/// the refs that change or whose filter reads a field that changes. `after` is the record as it
/// will be stored.
pub(crate) fn check_ref_filters(
    cx: &FillCtx<'_>,
    def: &ModelDef,
    after: &Fields,
    changed: Option<&Fields>,
) -> R<()> {
    for fd in &def.fields {
        let Some(rf) = fd.hints.as_ref().and_then(|h| h.ref_filter.as_ref()) else {
            continue;
        };
        let Some(J::String(id)) = after.get(fd.name) else {
            continue;
        };
        if id.is_empty() {
            continue;
        }
        if let Some(ch) = changed
            && !ch.contains_key(fd.name)
            && !rf.self_fields().iter().any(|s| ch.contains_key(*s))
        {
            continue;
        }
        let Some(to) = fd.kind.target() else { continue };
        let Some(target) = (cx.look)(to) else {
            continue;
        };
        let Some(mut rec) = cx.reads.get(to, id)? else {
            // The ref check of the write reports a record that is not there.
            continue;
        };
        if rf.clauses.iter().any(|c| {
            target
                .fields
                .iter()
                .any(|f| f.name == c.field && f.computed.is_some())
        }) {
            Computer::new(cx.reads, cx.look, false).fill(&target, id, &mut rec);
        }
        let holds = rf.clauses.iter().all(|c| {
            let want = match &c.value {
                FilterValue::Null => J::Null,
                FilterValue::Bool(b) => J::from(*b),
                FilterValue::Int(n) => J::from(*n),
                FilterValue::Text(t) => J::from(t.clone()),
                FilterValue::Field(f) => after.get(f.as_str()).cloned().unwrap_or(J::Null),
                FilterValue::User => J::from(cx.actor.id.clone()),
                FilterValue::Today(n) => J::from(add_days(cx.today, *n)),
            };
            let have = rec.get(c.field.as_str());
            match c.op {
                CmpOp::Eq => equal(have, &want),
                CmpOp::Ne => !equal(have, &want),
                op => {
                    let Some(o) = have.and_then(|h| ordering(h, &want)) else {
                        return false;
                    };
                    match op {
                        CmpOp::Lt => o.is_lt(),
                        CmpOp::Le => o.is_le(),
                        CmpOp::Gt => o.is_gt(),
                        _ => o.is_ge(),
                    }
                }
            }
        });
        if !holds {
            return Err(coded(
                code::REF_NOT_ALLOWED,
                &format!(
                    "{}.{}: the {to} {id} cannot be picked here; it must satisfy {}",
                    def.name, fd.name, rf.source
                ),
            ));
        }
    }
    Ok(())
}

/// The `needs` fields an action's input carries (D95): the values to write in the same unit
/// before the assertions. A key of the input that the action does not need is ignored, as before.
pub(crate) fn needs_of(cmd: &CommandDef, input: &crate::Value) -> Fields {
    let mut out = Fields::new();
    let (Some(ui), crate::Value::Map(m)) = (&cmd.ui, input) else {
        return out;
    };
    for n in &ui.needs {
        if let Some(v) = m.get(n.as_str()) {
            out.insert(n.clone(), v.to_json());
        }
    }
    out
}

/// The first `needs` field that is still empty on `rec` once the input is applied.
pub(crate) fn need_missing<'a>(cmd: &'a CommandDef, rec: &Fields) -> Option<&'a str> {
    cmd.ui
        .as_ref()?
        .needs
        .iter()
        .find(|n| is_unset(rec.get(n.as_str())))
        .map(String::as_str)
}

/// The refusal of `need_missing`.
pub(crate) fn need_message(def: &ModelDef, cmd: &CommandDef, field: &str) -> String {
    format!(
        "{}.{} needs {field}: give it in the input of the action (or set it on the record first)",
        def.name, cmd.name
    )
}

/// `not_by`: the person named in the field may not run the action (D95).
pub(crate) fn not_by(def: &ModelDef, cmd: &CommandDef, rec: &Fields, actor_id: &str) -> R<()> {
    let Some(f) = cmd.ui.as_ref().and_then(|u| u.not_by.as_deref()) else {
        return Ok(());
    };
    if !actor_id.is_empty() && rec.get(f).and_then(J::as_str) == Some(actor_id) {
        return Err(coded(
            code::DENIED,
            &format!(
                "{}.{} cannot be done by the person in {f}; someone else must do it",
                def.name, cmd.name
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_are_added_across_months_and_leap_years() {
        assert_eq!(add_days("2026-11-15", 0), "2026-11-15");
        assert_eq!(add_days("2026-11-15", 30), "2026-12-15");
        assert_eq!(add_days("2028-02-28", 1), "2028-02-29");
        assert_eq!(add_days("2027-12-31", 1), "2028-01-01");
        assert_eq!(add_days("2026-01-31", 365), "2027-01-31");
    }

    #[test]
    fn numbers_and_decimals_compare_as_numbers() {
        assert_eq!(
            ordering(&J::from("10.50"), &J::from("9.00")),
            Some(std::cmp::Ordering::Greater)
        );
        assert_eq!(
            ordering(&J::from("2026-01-02"), &J::from("2026-01-10")),
            Some(std::cmp::Ordering::Less)
        );
        assert!(equal(Some(&J::from(true)), &J::from(true)));
        assert!(equal(None, &J::from(false)));
        assert!(!equal(Some(&J::from("a")), &J::from("b")));
    }
}
