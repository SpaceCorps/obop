//! The generated get, list, search, create, update and delete and the generic action, built on
//! the unit of work, computed fields, row-level access and sequences.

use super::access::{Gate, creator_blocked};
use super::computed::Computer;
use super::effects::{lookup_fn, run_action};
use super::seq;
use super::unit::{Plain, R, Reads, Unit};
use super::{Actor, Engine};
use crate::Value;
use crate::guards;
use crate::query::Query;
use crate::store::{declared_fields, id_of};
use obop_registry::{FieldKind, ModelDef, is_valid_date};
use obop_storage::Fields;
use serde_json::Value as J;
use std::collections::BTreeMap;
use std::sync::Arc;

pub(crate) fn to_row(id: &str, f: &Fields) -> Value {
    let mut m: BTreeMap<String, Value> = f
        .iter()
        .map(|(k, v)| (k.clone(), Value::from_json(v)))
        .collect();
    m.insert("id".into(), Value::text(id));
    Value::Map(m)
}

fn to_fields(m: &BTreeMap<String, Value>) -> Fields {
    m.iter().map(|(k, v)| (k.clone(), v.to_json())).collect()
}

/// Governed fields start at their first option.
pub(crate) fn default_governed(def: &ModelDef, f: &mut Fields) {
    for g in def.governed_fields() {
        if f.get(g).is_none_or(J::is_null)
            && let Some(first) = def
                .fields
                .iter()
                .find(|d| d.name == g)
                .and_then(|d| d.kind.options().first())
        {
            f.insert(g.to_string(), J::from(*first));
        }
    }
}

/// The `own` field of a model with row-level access defaults to the person creating the record.
pub(crate) fn own_default(def: &ModelDef, f: &mut Fields, actor: &Actor) {
    if let Some(own) = def.access.as_ref().and_then(|a| a.own)
        && !actor.id.is_empty()
        && f.get(own)
            .is_none_or(|v| v.is_null() || v.as_str() == Some(""))
    {
        f.insert(own.to_string(), J::from(actor.id.clone()));
    }
}

fn check_json(
    reads: &dyn Reads,
    gate: &Gate<'_>,
    lookup: &dyn Fn(&str) -> Option<Arc<ModelDef>>,
    model: &str,
    name: &str,
    kind: &FieldKind,
    v: &J,
) -> R<()> {
    let wrong = || Err(format!("{model}.{name} must be {}", kind.describe()));
    match (kind, v) {
        (FieldKind::Text | FieldKind::LongText, J::String(_)) => Ok(()),
        (FieldKind::Number | FieldKind::Money, J::Number(n)) if n.is_i64() => Ok(()),
        (FieldKind::Bool, J::Bool(_)) => Ok(()),
        (FieldKind::Date, J::String(t)) if is_valid_date(t) => Ok(()),
        (FieldKind::Select(o), J::String(t)) if o.contains(&t.as_str()) => Ok(()),
        (FieldKind::Ref(to), J::String(id)) => {
            // A record the caller cannot see is reported exactly like one that is not there.
            let seen = match (reads.get(to, id)?, lookup(to)) {
                (Some(rec), Some(tdef)) => gate.visible(&tdef, &rec),
                (Some(_), None) => true,
                (None, _) => false,
            };
            if seen {
                Ok(())
            } else {
                Err(format!(
                    "{model}.{name} must be {}; there is no {to} {id}",
                    kind.describe()
                ))
            }
        }
        _ => wrong(),
    }
}

/// Validate the fields of a create (`creating`: every required field must be present) or of an
/// update against their kinds (D47), required flags and refs. A ref must name a record the caller
/// can see.
pub(crate) fn check_fields(
    reads: &dyn Reads,
    lookup: &dyn Fn(&str) -> Option<Arc<ModelDef>>,
    actor: &Actor,
    def: &ModelDef,
    fields: &Fields,
    creating: bool,
) -> R<()> {
    let gate = Gate::new(reads, lookup, actor);
    for f in &def.fields {
        if f.computed.is_some() {
            continue;
        }
        match fields.get(f.name) {
            None if creating && f.required => {
                return Err(format!("{}.{} is required", def.name, f.name));
            }
            None => {}
            Some(J::Null) if f.required => {
                return Err(format!(
                    "{}.{} is required and cannot be null",
                    def.name, f.name
                ));
            }
            Some(J::Null) => {}
            Some(v) => check_json(reads, &gate, lookup, def.name, f.name, &f.kind, v)?,
        }
    }
    Ok(())
}

/// A stored record as a row, computed fields filled in.
pub(crate) fn row(eng: &Engine, def: &ModelDef, id: &str) -> R<Value> {
    let reads = Plain(eng.backend());
    let look = lookup_fn(eng);
    let mut f = reads
        .get(def.name, id)?
        .ok_or_else(|| "record vanished".to_string())?;
    Computer::new(&reads, &look, false).fill(def, id, &mut f);
    Ok(to_row(id, &f))
}

fn commit(eng: &Engine, unit: Unit<'_>) -> R<()> {
    let touched = unit.touched();
    let look = lookup_fn(eng);
    unit.commit()
        .map_err(|e| seq::explain_db_error(e, &touched, &look))
}

pub(crate) fn get(eng: &Engine, def: &Arc<ModelDef>, input: &Value) -> R<Value> {
    let id = id_of(input)?;
    let actor = Actor::from_input(input);
    let reads = Plain(eng.backend());
    let look = lookup_fn(eng);
    let mut f = Gate::new(&reads, &look, &actor).load(def, &id)?;
    Computer::new(&reads, &look, false).fill(def, &id, &mut f);
    Ok(to_row(&id, &f))
}

/// `list` and `search`: the rows of the page and the number of matches before paging, over the
/// records the caller may see.
pub(crate) fn query(eng: &Engine, def: &Arc<ModelDef>, input: &Value) -> R<(Vec<Value>, usize)> {
    let actor = Actor::from_input(input);
    // The dispatcher's guard has already refused fields the caller cannot read; the handler
    // only needs the structure.
    let q = Query::parse(def, input, &|_| true)?;
    let reads = Plain(eng.backend());
    let look = lookup_fn(eng);
    let mut rows = reads.all(def.name)?;
    Gate::new(&reads, &look, &actor).retain(def, &mut rows);
    let has_computed = def.computed_fields().next().is_some();
    let needs_early = has_computed && q.touches(&|n| def.computed_fields().any(|f| f.name == n));
    let (page, total) = if needs_early {
        let c = Computer::new(&reads, &look, rows.len() > 16);
        c.fill_all(def, &mut rows);
        q.apply(rows)
    } else {
        let (mut page, total) = q.apply(rows);
        if has_computed {
            Computer::new(&reads, &look, page.len() > 16).fill_all(def, &mut page);
        }
        (page, total)
    };
    Ok((page.iter().map(|(id, f)| to_row(id, f)).collect(), total))
}

fn system_error(def: &ModelDef, name: &str) -> String {
    let f = def.fields.iter().find(|f| f.name == name);
    if f.is_some_and(|f| f.computed.is_some()) {
        format!("{}.{name} is computed and read-only", def.name)
    } else {
        format!(
            "{}.{name} is a number the system assigns and cannot be set",
            def.name
        )
    }
}

pub(crate) fn create(eng: &Engine, def: &Arc<ModelDef>, input: &Value) -> R<Value> {
    let actor = Actor::from_input(input);
    let given = declared_fields(def, input)?;
    if let Some(k) = given.keys().find(|k| def.is_system_field(k)) {
        return Err(system_error(def, k));
    }
    let mut f = to_fields(&given);
    default_governed(def, &mut f);
    own_default(def, &mut f, &actor);
    let look = lookup_fn(eng);
    let mut unit = Unit::new(eng.backend());
    check_fields(&unit, &look, &actor, def, &f, true)?;
    seq::assign(&mut unit, def, &mut f, None, eng.now())?;
    if let Some(why) = creator_blocked(&Gate::new(&unit, &look, &actor), def, &f) {
        return Err(why);
    }
    let id = uuid::Uuid::now_v7().to_string();
    unit.insert(def.name, &id, f)?;
    seq::check_unique(&unit, &look)?;
    commit(eng, unit)?;
    row(eng, def, &id)
}

pub(crate) fn update(eng: &Engine, def: &Arc<ModelDef>, input: &Value) -> R<Value> {
    let id = id_of(input)?;
    let actor = Actor::from_input(input);
    let given = declared_fields(def, input)?;
    let look = lookup_fn(eng);
    let mut unit = Unit::new(eng.backend());
    let cur = Gate::new(&unit, &look, &actor).load(def, &id)?;
    let mut patch = to_fields(&given);
    // A computed or numbered field may be sent back unchanged (a form saves the whole record);
    // any other value is refused.
    let system: Vec<String> = patch
        .keys()
        .filter(|k| def.is_system_field(k))
        .cloned()
        .collect();
    if !system.is_empty() {
        let mut shown = cur.clone();
        Computer::new(&unit, &look, false).fill(def, &id, &mut shown);
        for k in system {
            if shown.get(&k) == patch.get(&k)
                || (shown.get(&k).is_none_or(J::is_null) && patch[&k].is_null())
            {
                patch.remove(&k);
            } else {
                return Err(system_error(def, &k));
            }
        }
    }
    check_fields(&unit, &look, &actor, def, &patch, false)?;
    let changed: Fields = patch
        .iter()
        .filter(|(k, v)| cur.get(*k) != Some(*v))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if !changed.is_empty() {
        let mut after = cur.clone();
        after.extend(changed.clone());
        // The caller may not move a record out of their own sight.
        if let Some(why) = creator_blocked(&Gate::new(&unit, &look, &actor), def, &after) {
            return Err(why);
        }
        unit.merge(def.name, &id, changed)?;
        seq::check_unique(&unit, &look)?;
        commit(eng, unit)?;
    }
    row(eng, def, &id)
}

pub(crate) fn delete(eng: &Engine, def: &Arc<ModelDef>, input: &Value) -> R<Value> {
    let id = id_of(input)?;
    let actor = Actor::from_input(input);
    let look = lookup_fn(eng);
    let mut unit = Unit::new(eng.backend());
    let rec = Gate::new(&unit, &look, &actor).load(def, &id)?;
    // Checked here, not only in the guard, so the rule holds for any caller of the handler.
    if let Some(field) = seq::holds_gapless_number(def, &rec) {
        return Err(format!(
            "cannot delete {} {id}: it holds the gapless number in {field}; cancel it instead, so the series keeps no hole",
            def.name
        ));
    }
    if let Some(why) = guards::referrers(eng, &actor, def.name, &id)? {
        return Err(why);
    }
    unit.delete(def.name, &id)?;
    commit(eng, unit)?;
    Ok(Value::map([("id", Value::text(&id))]))
}

/// The generic handler of a declared action: assertions, the transition, sequences and effects
/// as one unit of work.
pub(crate) fn transition(
    eng: &Engine,
    def: &Arc<ModelDef>,
    command: &str,
    input: &Value,
) -> R<Value> {
    let id = id_of(input)?;
    let actor = Actor::from_input(input);
    let cmd = def
        .commands
        .iter()
        .find(|c| c.name == command)
        .ok_or_else(|| format!("{}.{command} is not declared", def.name))?;
    let look = lookup_fn(eng);
    let mut unit = Unit::new(eng.backend());
    let patch = run_action(eng, &actor, &mut unit, def, cmd, &id, 0)?;
    seq::check_unique(&unit, &look)?;
    commit(eng, unit)?;
    let mut out: BTreeMap<String, Value> = patch
        .iter()
        .map(|(k, v)| (k.clone(), Value::from_json(v)))
        .collect();
    out.insert("id".into(), Value::text(&id));
    Ok(Value::Map(out))
}
