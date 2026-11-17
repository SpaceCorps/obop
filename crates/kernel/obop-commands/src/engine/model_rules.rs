//! Model rules (D88, first slice) and the no-overlap rule, checked on every write that the
//! uniqueness check sees: a create, an update, each row of `bulk_create` and the records an
//! action and its effects write. `seq::check_unique_keys` calls [`check`] first, so a rule refuses
//! before a duplicate does, and a refusal leaves nothing written (the unit is not committed).
//!
//! A create evaluates every rule. An update evaluates a rule with `applies = "touched"` only when
//! it changed a field the rule reads, so a record stored before the rule existed can still be
//! changed in its other fields; `applies = "always"` runs on every effective update. The
//! no-overlap rule is checked on a create, on an update that changes its key, its start or its
//! end, and on a move out of a state it ignores (never on a move between two other states).
//!
//! A refusal is `<message> (rule <key> failed)` in the caller's language when the labels have
//! one, noted for `errors::classify`, which answers `assertion_failed` with the key as
//! `assert_key`. It never names another record or a value.

use super::computed::{Computer, Lookup};
use super::unit::{R, Reads, Unit};
use crate::locale;
use obop_registry::expr::V;
use obop_registry::{Applies, ModelDef, NoOverlap};
use obop_storage::Fields;
use serde_json::Value as J;
use std::cmp::Ordering;

/// Refuse when a record of `keys`, as the unit leaves it, breaks a rule of its model.
pub(crate) fn check(unit: &Unit<'_>, lookup: Lookup<'_>, keys: &[(String, String)]) -> R<()> {
    for (model, id) in keys {
        let Some(def) = lookup(model) else { continue };
        if def.ext.rules.is_empty() && def.ext.no_overlap.is_none() {
            continue;
        }
        // Deleted in this unit: nothing to hold.
        let Some(rec) = Reads::get(unit, model, id)? else {
            continue;
        };
        let stored = unit.stored(model, id)?;
        if stored.as_ref() == Some(&rec) {
            continue;
        }
        let changed = |f: &str| stored.as_ref().is_none_or(|s| s.get(f) != rec.get(f));
        for r in &def.ext.rules {
            let runs = stored.is_none()
                || r.applies == Applies::Always
                || r.reads().into_iter().any(changed);
            if !runs {
                continue;
            }
            match Computer::new(unit, lookup, false).evaluate(&def, id, &rec, &r.expr) {
                Ok(V::Bool(true)) => {}
                Ok(_) => return Err(refusal(r.failure_in(locale::language()), &r.key)),
                Err(e) if obop_storage::fault::Fault::is_transient(&e) => return Err(e),
                // The cause can quote a stored value: it is not shown (D88 Decision 6).
                Err(_) => {
                    return Err(format!(
                        "{model}: rule {} could not be evaluated, so nothing was changed",
                        r.key
                    ));
                }
            }
        }
        if let Some(n) = &def.ext.no_overlap
            && rechecks(n, stored.as_ref(), &rec)
            && clashes(unit, &def, n, id, &rec)?
        {
            return Err(refusal(n.failure_in(locale::language()), &n.rule));
        }
    }
    Ok(())
}

fn refusal(text: String, key: &str) -> String {
    locale::note_assertion(&text, Some(key));
    text
}

/// A value that takes part in a comparison: present, not null, not the empty text.
fn present(v: Option<&J>) -> Option<&J> {
    v.filter(|v| !v.is_null() && v.as_str() != Some(""))
}

/// Numbers by value, texts (dates, canonical instants) as text.
fn order(a: &J, b: &J) -> Option<Ordering> {
    match (a, b) {
        (J::Number(x), J::Number(y)) => x.as_f64()?.partial_cmp(&y.as_f64()?),
        (J::String(x), J::String(y)) => Some(x.cmp(y)),
        _ => None,
    }
}

/// Whether `rec` (record `id`) overlaps another record of the same key: `[start, end)` against
/// `[other.start, other.end)`, so one booking may end when the next begins.
/// Whether a write can add a clash: a create; an update that moves the key, the start or the
/// end; or a state move out of an ignored state (a cancelled booking taken up again). A move
/// between two states that are not ignored (confirmed to done) adds none, so a record that
/// already overlapped before the rule existed can still be confirmed or completed.
fn rechecks(n: &NoOverlap, stored: Option<&Fields>, rec: &Fields) -> bool {
    let Some(s) = stored else { return true };
    let moved = |f: &str| s.get(f) != rec.get(f);
    n.key.iter().any(|k| moved(k))
        || moved(n.start)
        || moved(n.end)
        || (ignored(n, s) && !ignored(n, rec))
}

fn ignored(n: &NoOverlap, r: &Fields) -> bool {
    !n.ignore_states.is_empty()
        && r.get(n.state_field)
            .and_then(J::as_str)
            .is_some_and(|s| n.ignore_states.contains(&s))
}

fn clashes(unit: &Unit<'_>, def: &ModelDef, n: &NoOverlap, id: &str, rec: &Fields) -> R<bool> {
    let ignored = |r: &Fields| ignored(n, r);
    if ignored(rec) {
        return Ok(false);
    }
    let mut key = Vec::with_capacity(n.key.len());
    for k in &n.key {
        match present(rec.get(*k)) {
            Some(v) => key.push(v),
            None => return Ok(false),
        }
    }
    let (Some(start), Some(end)) = (present(rec.get(n.start)), present(rec.get(n.end))) else {
        return Ok(false);
    };
    for (other, o) in Reads::find(unit, def.name, n.key[0], key[0])? {
        if other == id || ignored(&o) {
            continue;
        }
        if n.key
            .iter()
            .zip(&key)
            .skip(1)
            .any(|(k, v)| o.get(*k) != Some(*v))
        {
            continue;
        }
        let (Some(os), Some(oe)) = (present(o.get(n.start)), present(o.get(n.end))) else {
            continue;
        };
        if order(start, oe) == Some(Ordering::Less) && order(os, end) == Some(Ordering::Less) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
#[path = "model_rules_tests.rs"]
mod tests;
