//! Running an action (D48, D56): the checks of the transition, its assertions, the new state,
//! the sequence numbers it assigns and its effects, all in one unit of work.
//!
//! Effects run with the caller's permissions and never more: creating or changing a record of
//! another model needs that model's write group, a `set_state` effect needs the group of the
//! action it starts and goes through that action's own checks, assertions and effects, and every
//! record involved must be visible to the caller under the row-level rules. A `set_state` effect
//! therefore cannot do what the caller could not have done by pressing that button.

use super::access::{Gate, creator_blocked};
use super::computed::Computer;
use super::crud::{check_fields, default_governed, own_default};
use super::seq;
use super::unit::{R, Reads, Unit};
use super::{Actor, Engine, civil};
use crate::kinds::{self, Path};
use obop_registry::rules::{MAX_EFFECT_DEPTH, MAX_EFFECT_RECORDS};
use obop_registry::{
    CommandDef, Effect, EffectKind, FieldKind, Lit, Locator, ModelDef, Src, Target, expr,
};
use obop_storage::Fields;
use obop_values::Instant;
use serde_json::Value as J;
use std::collections::HashMap;
use std::sync::Arc;

/// The first required field of `def` missing in `rec`.
pub(crate) fn required_missing(def: &ModelDef, rec: &Fields) -> Option<&'static str> {
    def.fields
        .iter()
        .find(|f| {
            f.required
                && f.computed.is_none()
                && rec.get(f.name).is_none_or(serde_json::Value::is_null)
        })
        .map(|f| f.name)
}

/// The checks every action makes before it runs: the record is in the `from` state and every
/// required field is set.
pub(crate) fn precheck(def: &ModelDef, rec: &Fields, cmd: &CommandDef) -> R<()> {
    let Some(t) = cmd.transition else {
        return Ok(());
    };
    if let Some(from) = t.from {
        let cur = rec.get(t.field).and_then(J::as_str);
        if cur != Some(from) {
            return Err(format!(
                "{} must be {from} to do this, it is {}",
                t.field,
                match cur {
                    Some(s) => format!("{s:?}"),
                    None => "empty".into(),
                }
            ));
        }
    }
    if let Some(name) = required_missing(def, rec) {
        return Err(format!(
            "{}.{name} is required and must be set before {} can change to {:?}",
            def.name, t.field, t.to
        ));
    }
    Ok(())
}

pub(crate) fn lookup_fn(eng: &Engine) -> impl Fn(&str) -> Option<Arc<ModelDef>> + '_ {
    move |n| eng.def(n)
}

fn lit(l: &Lit) -> J {
    match l {
        Lit::Null => J::Null,
        Lit::Bool(b) => J::from(*b),
        Lit::Int(n) => J::from(*n),
        Lit::Text(t) => J::from(t.clone()),
    }
}

struct Vars<'a> {
    me: &'a Fields,
    me_id: &'a str,
    line: Option<(&'a str, &'a Fields)>,
    created: &'a HashMap<String, String>,
    now: i64,
    /// The same instant in microseconds, for a datetime field.
    now_us: i64,
    /// Today's date in the platform zone (D72).
    today: &'a str,
    actor: &'a Actor,
}

impl Vars<'_> {
    /// The value for field `field` of `def`: `$now` is the instant to the microsecond for a
    /// datetime field, and a decimal or instant copied from another field is written in the
    /// canonical form of the field it goes into (a decimal of another scale is padded).
    fn value_for(&self, s: &Src, def: &ModelDef, field: &str) -> R<J> {
        let kind = def.fields.iter().find(|f| f.name == field).map(|f| &f.kind);
        let v = match (s, kind) {
            (Src::Now, Some(FieldKind::Datetime)) => {
                Instant::from_micros(self.now_us).map_or(J::Null, |i| J::from(i.canonical()))
            }
            _ => self.value(s)?,
        };
        // A whole-number constant (`rate = 0`) into a decimal is that decimal's text: the gate
        // accepts it, and a JSON number is never a decimal.
        let v = match (&v, kind) {
            (J::Number(n), Some(FieldKind::Decimal { .. })) if n.is_i64() => J::from(n.to_string()),
            _ => v,
        };
        if matches!(kind, Some(FieldKind::Decimal { .. } | FieldKind::Datetime)) && v.is_string() {
            let mut one = Fields::new();
            one.insert(field.to_string(), v);
            kinds::canonicalise(def, &mut one, Path::Write)?;
            return Ok(one.remove(field).unwrap_or(J::Null));
        }
        Ok(v)
    }

    fn value(&self, s: &Src) -> R<J> {
        Ok(match s {
            Src::Const(l) => lit(l),
            Src::SelfField(f) => self.me.get(f).cloned().unwrap_or(J::Null),
            Src::SelfId => J::from(self.me_id),
            Src::Line(f) => self
                .line
                .map_or(J::Null, |(_, l)| l.get(f).cloned().unwrap_or(J::Null)),
            Src::LineId => self.line.map_or(J::Null, |(id, _)| J::from(id)),
            Src::Created(n) => match self.created.get(n) {
                Some(id) => J::from(id.clone()),
                None => return Err(format!("no record named {n} was created yet")),
            },
            Src::Now => J::from(civil(self.now).rfc3339()),
            Src::Today => J::from(self.today.to_string()),
            Src::Caller => J::from(self.actor.caller.clone()),
        })
    }

    fn int(&self, s: &Src) -> R<i64> {
        match self.value(s)? {
            J::Number(n) => n
                .as_i64()
                .ok_or_else(|| "the amount is not a whole number".into()),
            J::Null => Ok(0),
            _ => Err("the amount is not a number".into()),
        }
    }
}

fn is_empty(v: &J) -> bool {
    matches!(v, J::Null) || matches!(v, J::String(s) if s.is_empty())
}

struct Runner<'e> {
    eng: &'e Engine,
    actor: &'e Actor,
    now: i64,
    now_us: i64,
    /// Today's date on the wall clock of the platform zone.
    today: String,
}

/// Run the action `cmd` of `def` on record `id` inside `unit`. `depth` is 0 for the action the
/// caller named and grows with every `set_state` effect.
pub(crate) fn run_action(
    eng: &Engine,
    actor: &Actor,
    unit: &mut Unit<'_>,
    def: &Arc<ModelDef>,
    cmd: &CommandDef,
    id: &str,
    depth: usize,
) -> R<Fields> {
    let now = eng.now();
    let now_us = eng.now_micros();
    let today =
        Instant::from_micros(now_us).map_or_else(|| civil(now).date(), |at| eng.zones().today(at));
    let r = Runner {
        eng,
        actor,
        now,
        now_us,
        today,
    };
    r.action(unit, def, cmd, id, depth)
}

impl Runner<'_> {
    fn look(&self, n: &str) -> Option<Arc<ModelDef>> {
        self.eng.def(n)
    }

    fn action(
        &self,
        unit: &mut Unit<'_>,
        def: &Arc<ModelDef>,
        cmd: &CommandDef,
        id: &str,
        depth: usize,
    ) -> R<Fields> {
        let Some(t) = cmd.transition else {
            return Err(format!(
                "{}.{} is not a state transition",
                def.name, cmd.name
            ));
        };
        if depth > MAX_EFFECT_DEPTH {
            return Err("actions started by effects are nested too deeply".into());
        }
        if !self.actor.holds(cmd.permission_group) {
            return Err(format!(
                "{}.{} needs the {} group",
                def.name, cmd.name, cmd.permission_group
            ));
        }
        let look = |n: &str| self.look(n);
        let rec = Gate::new(&*unit, &look, self.actor).load(def, id)?;
        // The dispatcher's guard has made these checks for the action the caller named (or, for
        // an implicit transition, for the update as a whole); actions started by effects make
        // them here.
        if depth > 0 {
            precheck(def, &rec, cmd)?;
        }
        // Assertions look at the record as it is now, children included.
        if !cmd.asserts.is_empty() {
            let c = Computer::new(&*unit, &look, false);
            for a in &cmd.asserts {
                match c.evaluate(def, id, &rec, &a.expr) {
                    Ok(expr::V::Bool(true)) => {}
                    Ok(_) => return Err(a.failure()),
                    Err(e) => return Err(format!("{} ({e})", a.failure())),
                }
            }
        }
        // The new state and the numbers this action assigns.
        let mut after = rec.clone();
        after.insert(t.field.to_string(), J::from(t.to));
        seq::assign(unit, def, &mut after, Some(cmd.name), self.now)?;
        let patch: Fields = after
            .iter()
            .filter(|(k, v)| rec.get(*k) != Some(*v))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        unit.merge(def.name, id, patch.clone())?;
        // Effects, in order.
        let mut created: HashMap<String, String> = HashMap::new();
        for (i, e) in cmd.effects.iter().enumerate() {
            self.effect(unit, def, id, e, &mut created, depth)
                .map_err(|m| {
                    format!(
                        "{}.{}: effect {} ({}) failed, nothing was changed: {m}",
                        def.name,
                        cmd.name,
                        i + 1,
                        e.kind_name()
                    )
                })?;
        }
        Ok(patch)
    }

    /// The record with its computed fields, as `$self` sees it.
    fn me(&self, unit: &Unit<'_>, def: &ModelDef, id: &str) -> R<Fields> {
        let look = |n: &str| self.look(n);
        let mut rec =
            Reads::get(unit, def.name, id)?.ok_or_else(|| format!("no {} {id}", def.name))?;
        Computer::new(unit, &look, false).fill(def, id, &mut rec);
        Ok(rec)
    }

    fn effect(
        &self,
        unit: &mut Unit<'_>,
        def: &Arc<ModelDef>,
        id: &str,
        e: &Effect,
        created: &mut HashMap<String, String>,
        depth: usize,
    ) -> R<()> {
        let me = self.me(unit, def, id)?;
        let look = |n: &str| self.look(n);
        if let Some(w) = &e.when {
            let v = Computer::new(&*unit, &look, false).evaluate(def, id, &me, w)?;
            if v != expr::V::Bool(true) {
                return Ok(());
            }
        }
        let Some(target) = self.look(e.model()) else {
            return Err(format!("model {} is not installed", e.model()));
        };
        // The rows of the loop: none for an effect without `for_each`.
        let rows: Vec<Option<(String, Fields)>> = match &e.for_each {
            None => vec![None],
            Some(child) => {
                let (cm, via) = expr::resolve_child(def, child, e.for_each_via.as_deref(), &look)?;
                let mut rows = Reads::find(&*unit, cm.name, via, &J::from(id))?;
                if rows.len() > MAX_EFFECT_RECORDS {
                    return Err(format!(
                        "there are {} {} rows; at most {MAX_EFFECT_RECORDS} can be handled by one action",
                        rows.len(),
                        cm.name
                    ));
                }
                // Lines are copied as they are read: computed fields included.
                let c = Computer::new(&*unit, &look, true);
                c.fill_all(&cm, &mut rows);
                rows.into_iter().map(Some).collect()
            }
        };
        for row in rows {
            let line = row.as_ref().map(|(i, f)| (i.as_str(), f));
            // A created name refers to the record of this effect; inside a loop there is none.
            let vars = Vars {
                me: &me,
                me_id: id,
                line,
                created,
                now: self.now,
                now_us: self.now_us,
                today: &self.today,
                actor: self.actor,
            };
            match &e.kind {
                EffectKind::Create { map, name, .. } => {
                    let mut f = Fields::new();
                    for (k, s) in map {
                        let v = vars.value_for(s, &target, k)?;
                        // An empty ref is an unset ref.
                        let unset_ref = v.as_str() == Some("")
                            && target
                                .fields
                                .iter()
                                .any(|d| d.name == *k && d.kind.target().is_some());
                        if !v.is_null() && !unset_ref {
                            f.insert((*k).into(), v);
                        }
                    }
                    let new_id = self.create(unit, &target, f)?;
                    if let Some(n) = name {
                        created.insert(n.clone(), new_id);
                    }
                }
                EffectKind::Update { target: t, set, .. } => {
                    let ids = self.targets(unit, def, &me, id, &target, t)?;
                    let mut patch = Fields::new();
                    for (k, s) in set {
                        patch.insert((*k).into(), vars.value_for(s, &target, k)?);
                    }
                    for tid in ids {
                        self.change(unit, &target, &tid, patch.clone())?;
                    }
                }
                EffectKind::Adjust {
                    locator,
                    field,
                    subtract,
                    by,
                    create_missing,
                    min,
                    ..
                } => {
                    let amount = vars.int(by)?;
                    if amount == 0 {
                        continue;
                    }
                    let delta = if *subtract { -amount } else { amount };
                    let tid = match locator {
                        Locator::Ref(f) => match me.get(*f) {
                            Some(J::String(s)) if !s.is_empty() => Some(s.clone()),
                            _ => {
                                return Err(format!(
                                    "{}.{f} is empty, so there is nothing to adjust",
                                    def.name
                                ));
                            }
                        },
                        Locator::Match(m) => {
                            let mut want = Vec::new();
                            for (k, s) in m {
                                let v = vars.value_for(s, &target, k)?;
                                if is_empty(&v) {
                                    return Err(format!(
                                        "the value to match {}.{k} is empty",
                                        target.name
                                    ));
                                }
                                want.push((*k, v));
                            }
                            let (k0, v0) = &want[0];
                            let hits: Vec<_> = Reads::find(&*unit, target.name, k0, v0)?
                                .into_iter()
                                .filter(|(_, f)| want.iter().all(|(k, v)| f.get(*k) == Some(v)))
                                .collect();
                            match hits.len() {
                                1 => Some(hits[0].0.clone()),
                                0 if *create_missing => {
                                    let mut f: Fields = want
                                        .iter()
                                        .map(|(k, v)| ((*k).to_string(), v.clone()))
                                        .collect();
                                    if let Some(floor) = min
                                        && delta < *floor
                                    {
                                        return Err(
                                            self.floor_error(&target, field, *floor, 0, delta)
                                        );
                                    }
                                    f.insert((*field).into(), J::from(delta));
                                    self.create(unit, &target, f)?;
                                    None
                                }
                                0 => {
                                    return Err(format!(
                                        "there is no {} with {}",
                                        target.name,
                                        want.iter()
                                            .map(|(k, v)| format!("{k} = {v}"))
                                            .collect::<Vec<_>>()
                                            .join(" and ")
                                    ));
                                }
                                n => {
                                    return Err(format!(
                                        "{n} records of {} match; it must be exactly one",
                                        target.name
                                    ));
                                }
                            }
                        }
                    };
                    if let Some(tid) = tid {
                        let cur = {
                            let g = Gate::new(&*unit, &look, self.actor);
                            let rec = g
                                .load(&target, &tid)
                                .map_err(|_| self.unavailable(&target))?;
                            match rec.get(*field) {
                                Some(J::Number(n)) => n.as_i64().unwrap_or(0),
                                _ => 0,
                            }
                        };
                        let new = cur
                            .checked_add(delta)
                            .ok_or_else(|| format!("{}.{field} would overflow", target.name))?;
                        if let Some(floor) = min
                            && new < *floor
                        {
                            return Err(self.floor_error(&target, field, *floor, cur, delta));
                        }
                        let mut patch = Fields::new();
                        patch.insert((*field).into(), J::from(new));
                        self.change(unit, &target, &tid, patch)?;
                    }
                }
                EffectKind::SetState {
                    target: t, action, ..
                } => {
                    let ids = self.targets(unit, def, &me, id, &target, t)?;
                    let Some(next) = target.commands.iter().find(|c| c.name == *action) else {
                        return Err(format!("{} has no action {action}", target.name));
                    };
                    for tid in ids {
                        self.action(unit, &target, next, &tid, depth + 1)
                            .map_err(|m| {
                                format!("{}.{action} on {}: {m}", target.name, "the related record")
                            })?;
                    }
                }
            }
        }
        Ok(())
    }

    fn floor_error(
        &self,
        target: &ModelDef,
        field: &str,
        floor: i64,
        cur: i64,
        delta: i64,
    ) -> String {
        format!(
            "{}.{field} would go to {}, below {floor} (it is {cur}, the change is {delta})",
            target.name,
            cur + delta
        )
    }

    fn unavailable(&self, target: &ModelDef) -> String {
        format!("the related {} record is not available to you", target.name)
    }

    /// The ids an `update` or `set_state` effect affects.
    fn targets(
        &self,
        unit: &Unit<'_>,
        def: &ModelDef,
        me: &Fields,
        id: &str,
        target: &Arc<ModelDef>,
        t: &Target,
    ) -> R<Vec<String>> {
        match t {
            Target::Ref(f) => match me.get(*f) {
                Some(J::String(s)) if !s.is_empty() => Ok(vec![s.clone()]),
                _ => Ok(vec![]),
            },
            Target::Children(via) => {
                let via = match via {
                    Some(v) => (*v).to_string(),
                    None => target
                        .fields
                        .iter()
                        .find(|f| f.kind.target() == Some(def.name))
                        .map(|f| f.name.to_string())
                        .ok_or_else(|| format!("{} has no ref to {}", target.name, def.name))?,
                };
                let rows = Reads::find(unit, target.name, &via, &J::from(id))?;
                if rows.len() > MAX_EFFECT_RECORDS {
                    return Err(format!(
                        "there are {} related {} records; at most {MAX_EFFECT_RECORDS} can be handled by one action",
                        rows.len(),
                        target.name
                    ));
                }
                Ok(rows.into_iter().map(|(i, _)| i).collect())
            }
        }
    }

    /// Create a record the way the generated `create` does, for the caller.
    fn create(&self, unit: &mut Unit<'_>, def: &Arc<ModelDef>, mut f: Fields) -> R<String> {
        if !self.actor.holds(def.write_group) {
            return Err(format!(
                "creating a {} needs the {} group",
                def.name, def.write_group
            ));
        }
        kinds::canonicalise(def, &mut f, Path::Write)?;
        default_governed(def, &mut f);
        own_default(def, &mut f, self.actor);
        let look = |n: &str| self.look(n);
        check_fields(&*unit, &look, self.actor, def, &f, true)?;
        seq::assign(unit, def, &mut f, None, self.now)?;
        if let Some(why) = creator_blocked(&Gate::new(&*unit, &look, self.actor), def, &f) {
            return Err(why);
        }
        let id = uuid::Uuid::now_v7().to_string();
        unit.insert(def.name, &id, f)?;
        Ok(id)
    }

    /// Change fields of a record the way the generated `update` does, for the caller.
    fn change(
        &self,
        unit: &mut Unit<'_>,
        def: &Arc<ModelDef>,
        id: &str,
        mut patch: Fields,
    ) -> R<()> {
        if !self.actor.holds(def.write_group) {
            return Err(format!(
                "changing a {} needs the {} group",
                def.name, def.write_group
            ));
        }
        let look = |n: &str| self.look(n);
        let cur = Gate::new(&*unit, &look, self.actor)
            .load(def, id)
            .map_err(|_| self.unavailable(def))?;
        kinds::canonicalise(def, &mut patch, Path::Write)?;
        check_fields(&*unit, &look, self.actor, def, &patch, false)?;
        let mut after = cur;
        after.extend(patch.clone());
        if let Some(why) = creator_blocked(&Gate::new(&*unit, &look, self.actor), def, &after) {
            return Err(why);
        }
        unit.merge(def.name, id, patch)
    }
}

/// Everything an action may start, directly or through `set_state` effects: the model, the
/// command, and what the caller needs. Used to decide, before anything runs, whether an agent
/// must propose the whole operation and which groups the caller and the approver must hold.
pub(crate) fn reachable(
    eng: &Engine,
    def: &Arc<ModelDef>,
    cmd: &CommandDef,
) -> Vec<(Arc<ModelDef>, CommandDef)> {
    let mut out: Vec<(Arc<ModelDef>, CommandDef)> = Vec::new();
    let mut todo = vec![(def.clone(), cmd.clone(), 0usize)];
    while let Some((d, c, depth)) = todo.pop() {
        if depth > MAX_EFFECT_DEPTH {
            continue;
        }
        for e in &c.effects {
            if let EffectKind::SetState { model, action, .. } = &e.kind
                && let Some(t) = eng.def(model)
                && let Some(next) = t.commands.iter().find(|x| x.name == *action)
                && !out
                    .iter()
                    .any(|(m, x)| m.name == t.name && x.name == next.name)
            {
                out.push((t.clone(), next.clone()));
                todo.push((t.clone(), next.clone(), depth + 1));
            }
        }
        let _ = d;
    }
    out
}
