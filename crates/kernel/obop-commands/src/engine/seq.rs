//! Sequences and uniqueness (D57).
//!
//! A sequence number is taken from a counter record (`sequence_counter`, one per model, field
//! and period) inside the same unit of work as the record that carries it. The counter step is
//! committed or rolled back together with the record, so a posting that fails halfway never
//! consumes a number, and two postings can never get the same one (the unit is built under the
//! dispatcher's write lock). Uniqueness is checked against the records in the unit's view just
//! before commit, and a unique index in the database backs it up.

use super::unit::{R, Reads, Unit};
use super::{Civil, civil};
use crate::Dispatcher;
use crate::Store;
use obop_registry::{FieldDef, ModelDef, Reset, SequenceSpec};
use obop_storage::Fields;
use serde_json::Value as J;
use std::sync::Arc;

/// The model of the counter records. Not registered with the dispatcher: no command reaches it.
pub const COUNTER: &str = "sequence_counter";

pub(crate) fn counter_key(model: &str, field: &str, spec: &SequenceSpec, c: &Civil) -> String {
    match spec.reset {
        Reset::Never => format!("{model}.{field}"),
        Reset::Year => format!("{model}.{field}:{:04}", c.year),
        Reset::Month => format!("{model}.{field}:{:04}-{:02}", c.year, c.month),
    }
}

fn counter_value(f: &Fields) -> i64 {
    f.get("value").and_then(J::as_i64).unwrap_or(0)
}

/// Take the next number of `field` and return it rendered.
pub(crate) fn next(
    unit: &mut Unit<'_>,
    model: &str,
    field: &FieldDef,
    spec: &SequenceSpec,
    now: i64,
) -> R<String> {
    let c = civil(now);
    let key = counter_key(model, field.name, spec, &c);
    let (n, exists) = match unit.get(COUNTER, &key)? {
        Some(f) => (counter_value(&f) + 1, true),
        None => (1, false),
    };
    let mut row = Fields::new();
    row.insert("value".into(), J::from(n));
    if exists {
        unit.merge(COUNTER, &key, row)?;
    } else {
        row.insert("model".into(), J::from(model));
        row.insert("field".into(), J::from(field.name));
        unit.insert(COUNTER, &key, row)?;
    }
    Ok(spec.format.render(c.year, c.month, n))
}

/// Number the sequence fields of `rec` that are numbered at `moment`: `None` when the record is
/// created, `Some(action)` when that action runs. A field that already has a number keeps it.
pub(crate) fn assign(
    unit: &mut Unit<'_>,
    def: &ModelDef,
    rec: &mut Fields,
    moment: Option<&str>,
    now: i64,
) -> R<()> {
    for f in &def.fields {
        let Some(spec) = &f.sequence else { continue };
        let wanted = match (spec.assign_on, moment) {
            (None, None) => true,
            (Some(a), Some(m)) => a == m,
            _ => false,
        };
        if !wanted {
            continue;
        }
        let has = matches!(rec.get(f.name), Some(J::String(s)) if !s.is_empty());
        if has {
            continue;
        }
        let number = next(unit, def.name, f, spec, now)?;
        rec.insert(f.name.to_string(), J::from(number));
    }
    Ok(())
}

/// Whether the record carries a number from a gapless sequence (it cannot be deleted).
pub(crate) fn holds_gapless_number(def: &ModelDef, rec: &Fields) -> Option<&'static str> {
    def.fields
        .iter()
        .find(|f| {
            f.sequence.as_ref().is_some_and(|s| s.gapless)
                && matches!(rec.get(f.name), Some(J::String(s)) if !s.is_empty())
        })
        .map(|f| f.name)
}

// ---- backfill -----------------------------------------------------------------------------

/// States a record is in once `to` has been reached: `to` itself and every state the declared
/// transitions lead on to.
fn states_from(def: &ModelDef, field: &str, to: &str) -> Vec<String> {
    let mut seen = vec![to.to_string()];
    let mut i = 0;
    while i < seen.len() {
        let cur = seen[i].clone();
        for (_, t) in def.transitions() {
            if t.field == field
                && t.from.is_none_or(|f| f == cur)
                && !seen.iter().any(|s| s == t.to)
                && t.to != "cancelled"
            {
                seen.push(t.to.to_string());
            }
        }
        i += 1;
    }
    seen
}

/// What `obop sequences backfill` did: the model, the field and how many records were numbered.
pub type Backfilled = Vec<(String, String, usize)>;

impl Dispatcher {
    /// Number the records that were created before a sequence field existed (or were written
    /// straight into the store): in the order they were created, from the current counter. A
    /// sequence that numbers at creation numbers every record without a number; one that numbers
    /// when an action runs numbers the records that have passed that action (are in its target
    /// state or a state that follows it). Each field is one transaction: it all happens or none
    /// of it does. `model` and `field` narrow the work.
    pub fn backfill_sequences(
        &self,
        store: &Store,
        model: Option<&str>,
        field: Option<&str>,
    ) -> Result<Backfilled, String> {
        let _txn = crate::shared::txn_lock(self.shared.get(), &self.txn)?;
        let eng = self.engine(store);
        let defs: Vec<Arc<ModelDef>> = {
            let g = self.models.read().unwrap();
            let mut v: Vec<_> = g.values().map(|m| m.def.clone()).collect();
            v.sort_by_key(|d| d.name);
            v
        };
        let mut out = Vec::new();
        for def in defs {
            if model.is_some_and(|m| m != def.name) {
                continue;
            }
            for f in def.fields.iter().filter(|f| f.sequence.is_some()) {
                if field.is_some_and(|x| x != f.name) {
                    continue;
                }
                let Some(spec) = f.sequence.clone() else {
                    continue;
                };
                let passed: Option<Vec<String>> = spec.assign_on.and_then(|a| {
                    def.commands
                        .iter()
                        .find(|c| c.name == a)
                        .and_then(|c| c.transition)
                        .map(|t| states_from(&def, t.field, t.to))
                });
                let state_field = spec
                    .assign_on
                    .and_then(|a| def.commands.iter().find(|c| c.name == a))
                    .and_then(|c| c.transition)
                    .map(|t| t.field);
                let mut unit = Unit::unbounded(eng.backend());
                let mut n = 0;
                for (id, rec) in eng.backend().list(def.name)? {
                    if matches!(rec.get(f.name), Some(J::String(s)) if !s.is_empty()) {
                        continue;
                    }
                    if let (Some(ok), Some(sf)) = (&passed, state_field) {
                        let cur = rec.get(sf).and_then(J::as_str).unwrap_or_default();
                        if !ok.iter().any(|s| s == cur) {
                            continue;
                        }
                    }
                    let number = next(&mut unit, def.name, f, &spec, eng.now())?;
                    let mut patch = Fields::new();
                    patch.insert(f.name.to_string(), J::from(number));
                    unit.merge(def.name, &id, patch)?;
                    n += 1;
                }
                if n > 0 {
                    let touched = unit.touched();
                    let look = |m: &str| eng.def(m);
                    unit.commit()
                        .map_err(|e| explain_db_error(e, &touched, &look))?;
                }
                out.push((def.name.to_string(), f.name.to_string(), n));
            }
        }
        Ok(out)
    }
}

// ---- uniqueness ---------------------------------------------------------------------------

/// Each uniqueness constraint of a model: the field and the fields that scope it. A sequence is
/// unique by construction and is also indexed, so a restore or an import cannot duplicate one.
pub(crate) fn unique_specs(def: &ModelDef) -> Vec<(&'static str, Vec<&'static str>)> {
    def.fields
        .iter()
        .filter_map(|f| match (&f.unique, &f.sequence) {
            (Some(u), _) => Some((f.name, u.with.clone())),
            (None, Some(_)) => Some((f.name, Vec::new())),
            _ => None,
        })
        .collect()
}

/// The name of the database constraint for a model and its fields (at most 63 characters, which
/// is the PostgreSQL limit, with a hash when the readable name would be longer).
pub(crate) fn index_name(model: &str, fields: &[&str]) -> String {
    let readable = format!("obop_uq_{model}_{}", fields.join("_"));
    if readable.len() <= 63 {
        return readable;
    }
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in readable.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{}_{:016x}", &readable[..46], h)
}

/// Create the database constraints of `def`. A database that already holds duplicates keeps
/// working (the write-time check still applies); the failure is reported.
pub(crate) fn ensure_indexes(backend: &dyn obop_storage::Backend, def: &ModelDef) {
    for (field, with) in unique_specs(def) {
        let mut cols = vec![field];
        cols.extend(with.iter().copied());
        if let Err(e) = backend.ensure_unique(&index_name(def.name, &cols), def.name, &cols) {
            eprintln!(
                "warning: no database constraint for the uniqueness of {}.{field}: {e}. It is still checked on every write",
                def.name
            );
        }
    }
}

fn norm(rec: &Fields, f: &str) -> J {
    match rec.get(f) {
        None | Some(J::Null) => J::from(""),
        Some(v) => v.clone(),
    }
}

fn shown(v: &J) -> String {
    match v {
        J::String(s) => format!("{s:?}"),
        other => other.to_string(),
    }
}

/// Refuse the unit if any record it inserted or changed would share a unique value with another
/// record, as the unit would leave things.
pub(crate) fn check_unique(
    unit: &Unit<'_>,
    lookup: &dyn Fn(&str) -> Option<Arc<ModelDef>>,
) -> R<()> {
    for (model, id) in unit.touched() {
        let Some(def) = lookup(&model) else { continue };
        let specs = unique_specs(&def);
        if specs.is_empty() {
            continue;
        }
        let Some(rec) = Reads::get(unit, &model, &id)? else {
            continue;
        };
        for (field, with) in specs {
            let value = match rec.get(field) {
                None | Some(J::Null) => continue,
                Some(J::String(s)) if s.is_empty() => continue,
                Some(v) => v.clone(),
            };
            for (other, orec) in Reads::find(unit, &model, field, &value)? {
                if other != id && with.iter().all(|w| norm(&rec, w) == norm(&orec, w)) {
                    let scope = if with.is_empty() {
                        String::new()
                    } else {
                        format!(" with the same {}", with.join(" and "))
                    };
                    return Err(format!(
                        "{model}.{field} must be unique: {} is already used by another record{scope}",
                        shown(&value)
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Turn a database constraint violation into the message of the rule it enforces. Anything else
/// is returned as it is.
pub(crate) fn explain_db_error(
    e: String,
    touched: &[(String, String)],
    lookup: &dyn Fn(&str) -> Option<Arc<ModelDef>>,
) -> String {
    if !e.contains("obop_uq_") {
        return e;
    }
    for (model, _) in touched {
        let Some(def) = lookup(model) else { continue };
        for (field, with) in unique_specs(&def) {
            let mut cols = vec![field];
            cols.extend(with.iter().copied());
            if e.contains(&index_name(def.name, &cols)) {
                return format!(
                    "{}.{field} must be unique: another record with that value was saved at the same moment",
                    def.name
                );
            }
        }
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    use obop_registry::SeqFormat;

    #[test]
    fn formats_render_and_validate() {
        let f = SeqFormat::parse("INV-{year}-{seq:05}").unwrap();
        assert_eq!(f.render(2026, 3, 42), "INV-2026-00042");
        let f = SeqFormat::parse("{seq}/{month}.{year}").unwrap();
        assert_eq!(f.render(2026, 3, 123456), "123456/03.2026");
        for bad in [
            "",
            "INV",
            "{seq}{seq}",
            "{seq:0}",
            "{seq:13}",
            "{seqx}",
            "{year",
            "X{seq}!",
            "{seq:ab}",
        ] {
            assert!(SeqFormat::parse(bad).is_err(), "{bad:?}");
        }
        assert!(SeqFormat::parse(&"a".repeat(41)).is_err());
    }

    #[test]
    fn counters_are_per_period_and_roll_back_with_the_unit() {
        let b = obop_storage::open("memory:").unwrap();
        let field = FieldDef::new("number", "n", obop_types::DataClass::None, false, "g").sequence(
            "INV-{year}-{seq:03}",
            true,
            Reset::Year,
        );
        let spec = field.sequence.clone().unwrap();
        let y2026 = 1_790_000_000; // 2026-09
        let y2027 = 1_810_000_000; // 2027-04
        let mut u = Unit::new(&*b);
        assert_eq!(
            next(&mut u, "inv", &field, &spec, y2026).unwrap(),
            "INV-2026-001"
        );
        assert_eq!(
            next(&mut u, "inv", &field, &spec, y2026).unwrap(),
            "INV-2026-002"
        );
        assert_eq!(
            next(&mut u, "inv", &field, &spec, y2027).unwrap(),
            "INV-2027-001"
        );
        // Nothing was committed: a dropped unit consumed nothing.
        drop(u);
        assert_eq!(b.count(COUNTER).unwrap(), 0);
        let mut u = Unit::new(&*b);
        assert_eq!(
            next(&mut u, "inv", &field, &spec, y2026).unwrap(),
            "INV-2026-001"
        );
        u.commit().unwrap();
        let mut u = Unit::new(&*b);
        assert_eq!(
            next(&mut u, "inv", &field, &spec, y2026).unwrap(),
            "INV-2026-002"
        );
    }

    #[test]
    fn index_names_fit_postgres() {
        assert_eq!(index_name("a", &["b"]), "obop_uq_a_b");
        let long = index_name(&"m".repeat(40), &["field_one", "field_two"]);
        assert!(long.len() <= 63, "{long}");
        assert_ne!(
            long,
            index_name(&"m".repeat(40), &["field_one", "field_three"])
        );
    }
}
