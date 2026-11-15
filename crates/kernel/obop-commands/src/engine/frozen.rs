//! The command-layer immutability guard of D77 (`guard_level = command_only`, on every backend).
//!
//! `Unit` asks this module before every insert, merge and delete of a model that declares
//! `[model.guard]`, so the rule holds for the generated update and delete, `bulk_create`, the
//! fields an action `needs`, every effect writer and an action started by an effect, on REST, MCP
//! and the CLI alike. A refusal is 409 `immutable_record` with a sentence that names the model,
//! the record, the rule and field names, never a value.
//!
//! - Whether a record is frozen is decided by the record **as it was when the unit began** (the
//!   stored row; a record the unit inserted is never frozen). So one action may post a record and
//!   write what posting writes (a number, an effect's `set`), and later writes of the unit may not
//!   unfreeze a record by moving it first.
//! - A frozen record changes only in `mutable_fields`, or along an `exit_to` edge by the action
//!   that the unit runs on it (the ticket `Unit::ticket` records), with that edge's `may_change`;
//!   it is never deleted, nor is a record in a `no_delete_when` state.
//! - A new or changed lock-date field may not fall on or before the lock date (or inside a closed
//!   period); a new or changed parent may not be frozen; a child of a frozen parent is not added,
//!   changed or deleted.
//!
//! The pure decisions are `frozen_by`, `merge_refusal` and `delete_refusal`; the reads (the stored
//! row, the parent, the lock date) are done by the small functions around them.

use super::unit::{R, Reads, Unit};
use crate::errors::{code, coded};
use obop_registry::guard::{CLOSED_OPTION, MAX_GUARD_DEPTH, closed_field};
use obop_registry::{Guard, LockDate, ModelDef, is_valid_date};
use obop_storage::Fields;
use serde_json::Value as J;
use std::sync::Arc;

/// Why a record is frozen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Frozen {
    /// Its state field holds one of the immutable states (the state, when it is short text).
    State(Option<String>),
    /// Its lock-date field is on or before the lock date, or inside a closed period.
    Locked,
    /// Its parent (model, id) is frozen.
    Parent(&'static str, String),
}

/// The definitions a check needs.
pub(crate) type Look<'a> = &'a dyn Fn(&str) -> Option<Arc<ModelDef>>;

fn text<'a>(f: &'a Fields, k: &str) -> Option<&'a str> {
    f.get(k).and_then(J::as_str).filter(|s| !s.is_empty())
}

/// A state as a refusal may name it: a string of at most 40 characters (D77 Decision 2).
fn shown_state(v: Option<&J>) -> Option<String> {
    v.and_then(J::as_str)
        .filter(|s| s.chars().count() <= 40)
        .map(str::to_string)
}

fn refuse(message: String) -> String {
    coded(code::IMMUTABLE_RECORD, &message)
}

// ---- the lock date --------------------------------------------------------------------------

/// What locks dates for one guard, read once per question.
enum Lock {
    /// Nothing is locked (the setting is not set).
    None,
    /// Dates on or before this one.
    Through(String),
    /// Dates inside one of these closed periods, as (start, end).
    Periods(Vec<(String, String)>),
}

impl Lock {
    fn holds(&self, date: &str) -> bool {
        match self {
            Lock::None => false,
            Lock::Through(d) => date <= d.as_str(),
            Lock::Periods(p) => p
                .iter()
                .any(|(s, e)| s.as_str() <= date && date <= e.as_str()),
        }
    }
}

fn read_lock(reads: &dyn Reads, look: Look<'_>, l: &LockDate) -> R<Lock> {
    if let Some(key) = l.setting {
        let Some(row) = reads.get("obop_setting", "instance")? else {
            return Ok(Lock::None);
        };
        let Some(text) = row.get("value").and_then(J::as_str) else {
            return Ok(Lock::None);
        };
        let doc: J = serde_json::from_str(text).unwrap_or(J::Null);
        let mut at = &doc;
        for part in key.split('.') {
            at = &at[part];
        }
        return match at {
            J::Null => Ok(Lock::None),
            J::String(s) if s.is_empty() => Ok(Lock::None),
            J::String(s) if is_valid_date(s) => Ok(Lock::Through(s.clone())),
            _ => Err(format!(
                "the setting {key} is the lock date of the records and must be a date as YYYY-MM-DD"
            )),
        };
    }
    let Some(model) = l.closed_by else {
        return Ok(Lock::None);
    };
    let Some(pdef) = look(model) else {
        return Ok(Lock::None);
    };
    let Some(state) = closed_field(&pdef) else {
        return Ok(Lock::None);
    };
    let closed = reads.find(model, state, &J::from(CLOSED_OPTION))?;
    Ok(Lock::Periods(
        closed
            .iter()
            .filter_map(|(_, p)| {
                Some((
                    text(p, "start_on")?.to_string(),
                    text(p, "end_on")?.to_string(),
                ))
            })
            .collect(),
    ))
}

fn locked(reads: &dyn Reads, look: Look<'_>, g: &Guard, rec: &Fields) -> R<bool> {
    let Some(l) = &g.lock_date else {
        return Ok(false);
    };
    let Some(date) = text(rec, l.field) else {
        return Ok(false);
    };
    Ok(read_lock(reads, look, l)?.holds(date))
}

// ---- frozen ---------------------------------------------------------------------------------

/// Whether the record `rec` of `def` (as stored when the unit began) is frozen, and why. A chain
/// of parents deeper than `MAX_GUARD_DEPTH` counts as frozen.
pub(crate) fn frozen_by(
    u: &Unit<'_>,
    look: Look<'_>,
    def: &ModelDef,
    rec: &Fields,
    depth: usize,
) -> R<Option<Frozen>> {
    let Some(g) = def.guard() else {
        return Ok(None);
    };
    if let Some(s) = &g.immutable_when
        && rec
            .get(s.field)
            .and_then(J::as_str)
            .is_some_and(|v| s.values.contains(&v))
    {
        return Ok(Some(Frozen::State(shown_state(rec.get(s.field)))));
    }
    if locked(u, look, g, rec)? {
        return Ok(Some(Frozen::Locked));
    }
    if let Some((pm, pid)) = parent_of(def, g, rec)
        && parent_frozen(u, look, pm, &pid, depth + 1)?
    {
        return Ok(Some(Frozen::Parent(pm, pid)));
    }
    Ok(None)
}

/// The model and id of the parent `rec` names in `children_of`.
fn parent_of(def: &ModelDef, g: &Guard, rec: &Fields) -> Option<(&'static str, String)> {
    let field = g.children_of?;
    let to = def.fields.iter().find(|f| f.name == field)?.kind.target()?;
    Some((to, text(rec, field)?.to_string()))
}

/// Whether the parent `model` `id`, as stored when the unit began, is frozen.
fn parent_frozen(u: &Unit<'_>, look: Look<'_>, model: &str, id: &str, depth: usize) -> R<bool> {
    if depth > MAX_GUARD_DEPTH {
        return Ok(true);
    }
    let Some(pdef) = look(model) else {
        return Ok(false);
    };
    let Some(prec) = u.stored(model, id)? else {
        return Ok(false);
    };
    Ok(frozen_by(u, look, &pdef, &prec, depth)?.is_some())
}

fn frozen_sentence(def: &ModelDef, id: &str, why: &Frozen) -> String {
    match why {
        Frozen::State(Some(s)) => format!("{} {id} is {s}", def.name),
        Frozen::State(None) => format!("{} {id} is in a state that is final", def.name),
        Frozen::Locked => format!("{} {id} is dated in a locked period", def.name),
        Frozen::Parent(pm, pid) => {
            format!("{} {id} belongs to {pm} {pid}, which is locked", def.name)
        }
    }
}

// ---- the decisions --------------------------------------------------------------------------

/// The fields a merge changes: the keys of `patch` whose value differs from `cur`.
fn changed_keys<'a>(cur: &Fields, patch: &'a Fields) -> Vec<&'a str> {
    patch
        .iter()
        .filter(|(k, v)| match cur.get(*k) {
            Some(c) => c != *v,
            None => !v.is_null(),
        })
        .map(|(k, _)| k.as_str())
        .collect()
}

/// Why a merge of `changed` into a record frozen for `why` is refused, if it is: only the
/// mutable fields, and the state and `may_change` fields of an exit edge whose action the unit
/// runs on the record (`actions`), may change.
pub(crate) fn merge_refusal(
    def: &ModelDef,
    g: &Guard,
    id: &str,
    why: &Frozen,
    changed: &[&str],
    patch: &Fields,
    actions: &[&str],
) -> Option<String> {
    let edges: Vec<_> = g
        .exit_to
        .iter()
        .filter(|e| actions.contains(&e.action))
        .collect();
    let state = g.immutable_when.as_ref().map(|s| s.field);
    for k in changed {
        if g.mutable_fields.contains(k) {
            continue;
        }
        if Some(*k) == state {
            let to = patch.get(*k).and_then(J::as_str);
            if edges.iter().any(|e| Some(e.to) == to) {
                continue;
            }
            let exits: Vec<&str> = g.exit_to.iter().map(|e| e.action).collect();
            let how = if exits.is_empty() {
                "no action takes it back".to_string()
            } else {
                format!("only {} can move it on", exits.join(" or "))
            };
            return Some(format!("{}: {how}", frozen_sentence(def, id, why)));
        }
        if edges.iter().any(|e| e.may_change.contains(k)) {
            continue;
        }
        let open: Vec<&str> = g.mutable_fields.to_vec();
        let still = if open.is_empty() {
            String::new()
        } else {
            format!(" (only {} can still change)", open.join(", "))
        };
        return Some(format!(
            "{} and can no longer be changed: {}.{k} is fixed{still}",
            frozen_sentence(def, id, why),
            def.name
        ));
    }
    None
}

/// Why deleting a record is refused, if it is.
pub(crate) fn delete_refusal(
    def: &ModelDef,
    g: &Guard,
    id: &str,
    frozen: Option<&Frozen>,
    current: &Fields,
) -> Option<String> {
    if g.append_only {
        return Some(format!(
            "{} {id} cannot be deleted: {} records are kept as they were written",
            def.name, def.name
        ));
    }
    if let Some(why) = frozen {
        return Some(format!(
            "{} and cannot be deleted",
            frozen_sentence(def, id, why)
        ));
    }
    if let Some(s) = &g.no_delete_when
        && let Some(v) = current.get(s.field).and_then(J::as_str)
        && s.values.contains(&v)
    {
        let st = shown_state(current.get(s.field)).unwrap_or_else(|| "in this state".into());
        return Some(format!("{} {id} is {st} and cannot be deleted", def.name));
    }
    None
}

// ---- what `Unit` calls ----------------------------------------------------------------------

/// Before `fields` is inserted as a new record of `def`.
pub(crate) fn on_insert(u: &Unit<'_>, look: Look<'_>, def: &ModelDef, fields: &Fields) -> R<()> {
    let Some(g) = def.guard() else {
        return Ok(());
    };
    if let Some((pm, pid)) = parent_of(def, g, fields)
        && parent_frozen(u, look, pm, &pid, 1)?
    {
        return Err(refuse(format!(
            "{pm} {pid} can no longer be changed, so no {} can be added to it",
            def.name
        )));
    }
    if locked(u, look, g, fields)? {
        let field = g.lock_date.as_ref().map_or("", |l| l.field);
        return Err(refuse(format!(
            "{}.{field} falls in a locked period; choose a later date",
            def.name
        )));
    }
    Ok(())
}

/// Before `patch` is merged into the record `id` of `def`, which the unit holds as `cur` now.
pub(crate) fn on_merge(
    u: &Unit<'_>,
    look: Look<'_>,
    def: &ModelDef,
    id: &str,
    cur: &Fields,
    patch: &Fields,
) -> R<()> {
    let Some(g) = def.guard() else {
        return Ok(());
    };
    let changed = changed_keys(cur, patch);
    if changed.is_empty() {
        return Ok(());
    }
    if g.append_only {
        return Err(refuse(format!(
            "{} {id} cannot be changed: {} records are kept as they were written",
            def.name, def.name
        )));
    }
    if let Some(stored) = u.stored(def.name, id)?
        && let Some(why) = frozen_by(u, look, def, &stored, 0)?
    {
        let actions = u.tickets_of(def.name, id);
        if let Some(m) = merge_refusal(def, g, id, &why, &changed, patch, &actions) {
            return Err(refuse(m));
        }
    }
    let mut after = cur.clone();
    after.extend(patch.clone());
    if let Some(l) = &g.lock_date
        && changed.contains(&l.field)
        && locked(u, look, g, &after)?
    {
        return Err(refuse(format!(
            "{}.{} falls in a locked period; choose a later date",
            def.name, l.field
        )));
    }
    if let Some(field) = g.children_of
        && changed.contains(&field)
        && let Some((pm, pid)) = parent_of(def, g, &after)
        && parent_frozen(u, look, pm, &pid, 1)?
    {
        return Err(refuse(format!(
            "{pm} {pid} can no longer be changed, so no {} can be moved to it",
            def.name
        )));
    }
    Ok(())
}

/// Before the record `id` of `def`, which the unit holds as `cur` now, is deleted.
pub(crate) fn on_delete(
    u: &Unit<'_>,
    look: Look<'_>,
    def: &ModelDef,
    id: &str,
    cur: &Fields,
) -> R<()> {
    let Some(g) = def.guard() else {
        return Ok(());
    };
    let frozen = match u.stored(def.name, id)? {
        Some(stored) => frozen_by(u, look, def, &stored, 0)?,
        None => None,
    };
    match delete_refusal(def, g, id, frozen.as_ref(), cur) {
        Some(m) => Err(refuse(m)),
        None => Ok(()),
    }
}

/// Whether deleting the stored record `id` of `def` is refused by its guard: for the delete guard
/// of the dispatcher, so the refusal is the guard's and comes before "other records refer to it".
pub(crate) fn delete_refused(eng: &super::Engine, def: &ModelDef, id: &str) -> R<()> {
    if def.guard().is_none() {
        return Ok(());
    }
    let u = eng.unit();
    let Some(cur) = u.stored(def.name, id)? else {
        return Ok(());
    };
    let look = |n: &str| eng.def(n);
    on_delete(&u, &look, def, id, &cur)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(pairs: &[(&str, &str)]) -> Fields {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), J::from(*v)))
            .collect()
    }

    fn def() -> ModelDef {
        use obop_registry::{CommandDef, FieldDef as F};
        let f = |n: &'static str| F::new(n, "d", obop_types::DataClass::None, false, "g");
        ModelDef {
            name: "fz_order",
            description: "d",
            read_group: "g",
            write_group: "g",
            access: None,
            fields: vec![
                f("note"),
                f("name"),
                f("state").select(&["draft", "done", "void"]),
            ],
            commands: vec![
                CommandDef::new("finish", "d", "g").transition("state", Some("draft"), "done"),
                CommandDef::new("void", "d", "g").transition("state", Some("done"), "void"),
            ],
            ext: Default::default(),
        }
    }

    #[test]
    fn a_frozen_record_changes_only_where_the_guard_allows() {
        let g = Guard::frozen_in("state", &["done"])
            .mutable(&["note"])
            .exit("void", "void", &["name"]);
        let d = def().with_guard(g.clone());
        let why = Frozen::State(Some("done".into()));
        let p = rec(&[("note", "x")]);
        assert_eq!(merge_refusal(&d, &g, "1", &why, &["note"], &p, &[]), None);
        let p = rec(&[("name", "x")]);
        let m = merge_refusal(&d, &g, "1", &why, &["name"], &p, &[]).unwrap();
        assert!(m.contains("fz_order.name is fixed"), "{m}");
        assert!(m.contains("only note can still change"), "{m}");
        // The exit, by its action, with what it may change.
        let p = rec(&[("state", "void"), ("name", "x")]);
        assert_eq!(
            merge_refusal(&d, &g, "1", &why, &["state", "name"], &p, &["void"]),
            None
        );
        // The same move without the action's ticket.
        let m = merge_refusal(&d, &g, "1", &why, &["state"], &p, &[]).unwrap();
        assert!(m.contains("only void can move it on"), "{m}");
        assert!(m.starts_with("fz_order 1 is done"), "{m}");
    }

    #[test]
    fn delete_is_refused_for_frozen_append_only_and_no_delete_states() {
        let g = Guard::frozen_in("state", &["done"]).no_delete_in("state", &["void"]);
        let d = def().with_guard(g.clone());
        let why = Frozen::State(Some("done".into()));
        assert!(
            delete_refusal(&d, &g, "1", Some(&why), &rec(&[]))
                .unwrap()
                .contains("cannot be deleted")
        );
        assert!(
            delete_refusal(&d, &g, "1", None, &rec(&[("state", "void")]))
                .unwrap()
                .contains("is void and cannot be deleted")
        );
        assert_eq!(
            delete_refusal(&d, &g, "1", None, &rec(&[("state", "draft")])),
            None
        );
        let a = Guard::append_only();
        assert!(delete_refusal(&d, &a, "1", None, &rec(&[])).is_some());
    }

    #[test]
    fn a_lock_holds_dates_through_it_or_inside_closed_periods() {
        assert!(Lock::Through("2026-03-31".into()).holds("2026-03-31"));
        assert!(!Lock::Through("2026-03-31".into()).holds("2026-04-01"));
        let p = Lock::Periods(vec![("2026-03-01".into(), "2026-03-31".into())]);
        assert!(p.holds("2026-03-15") && !p.holds("2026-02-28") && !p.holds("2026-04-01"));
        assert!(!Lock::None.holds("2000-01-01"));
    }
}
