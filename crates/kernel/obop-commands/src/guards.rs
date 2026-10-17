//! Guards generated from the registry (D48, D49): the rules that need the principal or the
//! current record and so cannot live in a plain handler.
//!
//! - `create` and `update` of a model with governed fields: a governed field (the target of a
//!   declared transition) is set to its first option on create, and changed on update only as an
//!   implicit run of a declared transition, with that transition's group and approval.
//! - A transition action: its `from` state and the model's required fields are checked on the
//!   record before it runs.
//! - `list` and `search`: a filter or order on a field the caller cannot read in clear is refused.
//! - `delete`: refused while another record refers to the one being deleted.

use crate::engine::access::Gate;
use crate::engine::crud::check_fields;
use crate::engine::effects::{lookup_fn, precheck, reachable, required_missing};
use crate::engine::unit::{Plain, Reads};
use crate::engine::{Actor, Engine};
use crate::query::Query;
use crate::store::{Store, declared_fields, id_of};
use crate::{Dispatcher, Guard, Pre, Value, Verdict};
use obop_masking::mode_for;
use obop_registry::{EffectKind, ModelDef, Module, Transition};
use obop_types::{DataClass, MaskMode};
use std::collections::BTreeMap;
use std::sync::Arc;

fn show(v: Option<&str>) -> String {
    v.map_or_else(|| "(empty)".into(), |s| format!("{s:?}"))
}

fn text_of(v: &Value) -> Option<&str> {
    match v {
        Value::Text(t) => Some(t),
        _ => None,
    }
}

fn create_guard(def: &ModelDef) -> Option<Guard> {
    let firsts: Vec<(&'static str, &'static str, obop_registry::FieldKind)> = def
        .governed_fields()
        .into_iter()
        .filter_map(|g| {
            let f = def.fields.iter().find(|f| f.name == g)?;
            Some((g, *f.kind.options().first()?, f.kind.clone()))
        })
        .collect();
    if firsts.is_empty() {
        return None;
    }
    let model = def.name;
    Some(Box::new(move |_, input| {
        let Value::Map(m) = input else {
            return Ok(Verdict::default());
        };
        let mut out = m.clone();
        let mut changed = false;
        for (field, first, kind) in &firsts {
            match m.get(*field) {
                None | Some(Value::Null) => {
                    out.insert((*field).into(), Value::text(first));
                    changed = true;
                }
                Some(Value::Text(t)) if t == first => {}
                Some(Value::Text(t)) if kind.options().contains(&t.as_str()) => {
                    return Err(format!(
                        "{model}.{field} starts as {first:?} when a record is created; it moves on through the model's actions"
                    ));
                }
                // Not an option at all: say what validation says everywhere else.
                Some(_) => return Err(format!("{model}.{field} must be {}", kind.describe())),
            }
        }
        Ok(Verdict {
            input: changed.then_some(Value::Map(out)),
            ..Verdict::default()
        })
    }))
}

struct T {
    command: &'static str,
    group: &'static str,
    approval: bool,
    t: Transition,
}

fn update_guard(def: &ModelDef, eng: &Engine) -> Option<Guard> {
    let governed = def.governed_fields();
    if governed.is_empty() {
        return None;
    }
    let (def, eng) = (def.clone(), eng.clone());
    let trans: Vec<T> = def
        .transitions()
        .map(|(c, t)| T {
            command: c.name,
            group: c.permission_group,
            approval: c.needs_approval,
            t,
        })
        .collect();
    Some(Box::new(move |p, input| {
        let Value::Map(m) = input else {
            return Ok(Verdict::default());
        };
        let wanted: Vec<(&'static str, &Value)> = governed
            .iter()
            .filter_map(|g| m.get(*g).map(|v| (*g, v)))
            .collect();
        if wanted.is_empty() {
            return Ok(Verdict::default());
        }
        let id = id_of(input)?;
        let actor = Actor::of(p);
        let look = lookup_fn(&eng);
        let reads = Plain(eng.backend());
        let current = Gate::new(&reads, &look, &actor).load(&def, &id)?;
        let mut verdict = Verdict::default();
        let mut moved = Vec::new();
        for (field, new) in wanted {
            let cur = current
                .get(field)
                .map(Value::from_json)
                .unwrap_or(Value::Null);
            if &cur == new {
                continue; // not a change, so not a transition
            }
            let kind = &def
                .fields
                .iter()
                .find(|f| f.name == field)
                .expect("governed fields are declared")
                .kind;
            let to = match new {
                Value::Text(t) if kind.options().contains(&t.as_str()) => t.as_str(),
                _ => return Err(format!("{}.{field} must be {}", def.name, kind.describe())),
            };
            let cur_s = text_of(&cur);
            let leaves_here =
                |x: &&T| x.t.field == field && x.t.from.is_none_or(|f| Some(f) == cur_s);
            let mut cands: Vec<&T> = trans
                .iter()
                .filter(leaves_here)
                .filter(|x| x.t.to == to)
                .collect();
            // A transition that names the current state beats one that works from any state.
            cands.sort_by_key(|x| x.t.from.is_none());
            if cands.is_empty() {
                let mut targets: Vec<&str> = trans
                    .iter()
                    .filter(leaves_here)
                    .map(|x| x.t.to)
                    .filter(|t| Some(*t) != cur_s)
                    .collect();
                targets.dedup();
                let here = show(cur_s);
                return Err(if targets.is_empty() {
                    format!(
                        "{}.{field} cannot change from {here} to {to:?}: no transition leaves {here}",
                        def.name
                    )
                } else {
                    format!(
                        "{}.{field} cannot change from {here} to {to:?}; from {here} it can only go to: {}",
                        def.name,
                        targets.join(", ")
                    )
                });
            }
            let Some(chosen) = cands.iter().find(|x| p.allows(x.group)) else {
                let x = cands[0];
                return Err(format!(
                    "{}.{field}: moving from {} to {to:?} is the {} action, which needs the {} group",
                    def.name,
                    show(cur_s),
                    x.command,
                    x.group
                ));
            };
            verdict.requires.push(chosen.group);
            verdict.approval |= chosen.approval;
            verdict.pre.push(Pre {
                command: format!("{}.{}", def.name, chosen.command),
                input: Value::map([("id", Value::text(&id))]),
                field,
            });
            moved.push((field, to.to_string()));
        }
        if let Some((field, to)) = moved.first() {
            let mut merged = current.clone();
            for (k, v) in m {
                if k != "id" {
                    merged.insert(k.clone(), v.to_json());
                }
            }
            if let Some(name) = required_missing(&def, &merged) {
                return Err(format!(
                    "{}.{name} is required and must be set before {field} can change to {to:?}",
                    def.name
                ));
            }
            // The actions run before the update does, so everything the update would refuse
            // is refused now, before any of them has written.
            let given: serde_json::Map<String, serde_json::Value> = declared_fields(&def, input)?
                .into_iter()
                .map(|(k, v)| (k, v.to_json()))
                .collect();
            check_fields(&reads, &look, &actor, &def, &given, false)?;
        }
        Ok(verdict)
    }))
}

/// Checks before a transition action runs: the record exists (and is visible to the caller), it
/// is in the `from` state, every required field is set, and the caller may do everything the
/// action's effects do (D56): the groups of the records it writes and of the actions it starts.
/// If any of those actions needs approval, so does this one for an agent, and the approver must
/// hold their groups too.
fn action_guard(def: &ModelDef, eng: &Engine, command: &'static str) -> Guard {
    let (def, eng) = (def.clone(), eng.clone());
    Box::new(move |p, input| {
        let id = id_of(input)?;
        let actor = Actor::of(p);
        let look = lookup_fn(&eng);
        let reads = Plain(eng.backend());
        let rec = Gate::new(&reads, &look, &actor).load(&def, &id)?;
        let cmd = def
            .commands
            .iter()
            .find(|c| c.name == command)
            .ok_or_else(|| format!("{}.{command} is not declared", def.name))?;
        precheck(&def, &rec, cmd)?;
        let mut verdict = Verdict::default();
        let def = eng
            .def(def.name)
            .ok_or_else(|| format!("no model {}", def.name))?;
        let mut all = vec![(def.clone(), cmd.clone())];
        all.extend(reachable(&eng, &def, cmd));
        for (d, c) in &all {
            for e in &c.effects {
                let Some(t) = eng.def(e.model()) else {
                    continue;
                };
                if matches!(
                    e.kind,
                    EffectKind::Create { .. }
                        | EffectKind::Update { .. }
                        | EffectKind::Adjust { .. }
                ) {
                    if !p.allows(t.write_group) {
                        return Err(format!(
                            "{}.{} also changes {} records, which needs the {} group",
                            d.name, c.name, t.name, t.write_group
                        ));
                    }
                    verdict.requires.push(t.write_group);
                }
            }
        }
        for (d, c) in all.iter().skip(1) {
            if !p.allows(c.permission_group) {
                return Err(format!(
                    "{}.{} also runs {}.{}, which needs the {} group",
                    def.name, command, d.name, c.name, c.permission_group
                ));
            }
            verdict.requires.push(c.permission_group);
            verdict.approval |= c.needs_approval;
        }
        Ok(verdict)
    })
}

/// `list` and `search`: refuse a filter or order on a field the caller cannot read in clear
/// (masked, withheld or not permitted). The error is the one for a field that does not exist, so
/// the refusal is no oracle for the values behind it (D10).
pub(crate) fn query_guard(def: &ModelDef) -> Guard {
    let def = def.clone();
    Box::new(move |p, input| {
        let usable = |name: &str| {
            name == "id"
                || def.fields.iter().any(|f| {
                    f.name == name
                        && p.allows(f.permission_group)
                        && mode_for(
                            p.kind,
                            f.class.unwrap_or(DataClass::Secret),
                            f.personal_data,
                        ) == MaskMode::Allow
                })
        };
        Query::parse(&def, input, &usable).map(|_| Verdict::default())
    })
}

/// A message naming the records that still refer to `model` `id`, or `None` if nothing does.
/// Only records the caller can see are named; the others are mentioned without ids or counts.
pub(crate) fn referrers(
    eng: &Engine,
    actor: &Actor,
    model: &str,
    id: &str,
) -> Result<Option<String>, String> {
    // A snapshot, so the registry is not locked while the store is read.
    let defs: Vec<Arc<ModelDef>> = eng
        .models
        .read()
        .unwrap()
        .values()
        .map(|m| m.def.clone())
        .collect();
    let look = lookup_fn(eng);
    let reads = Plain(eng.backend());
    let gate = Gate::new(&reads, &look, actor);
    let mut by_model: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut hidden = false;
    for def in &defs {
        let fields: Vec<&str> = def
            .fields
            .iter()
            .filter(|f| f.kind.target() == Some(model))
            .map(|f| f.name)
            .collect();
        if fields.is_empty() {
            continue;
        }
        for (rid, rec) in reads.all(def.name)? {
            if def.name == model && rid == id {
                continue;
            }
            if fields
                .iter()
                .any(|f| rec.get(*f).and_then(serde_json::Value::as_str) == Some(id))
            {
                if gate.visible(def, &rec) {
                    by_model.entry(def.name).or_default().push(rid);
                } else {
                    hidden = true;
                }
            }
        }
    }
    if by_model.is_empty() && !hidden {
        return Ok(None);
    }
    let mut parts: Vec<String> = by_model
        .iter()
        .map(|(name, ids)| {
            let shown: Vec<&str> = ids.iter().take(5).map(String::as_str).collect();
            let more = if ids.len() > 5 { ", ..." } else { "" };
            format!("{name} ({}): {}{more}", ids.len(), shown.join(", "))
        })
        .collect();
    if hidden {
        parts.push("records you cannot see".into());
    }
    Ok(Some(format!(
        "cannot delete {model} {id}: still referenced by {}",
        parts.join("; ")
    )))
}

fn delete_guard(def: &ModelDef, eng: &Engine) -> Guard {
    let (name, eng) = (def.name, eng.clone());
    Box::new(move |p, input| {
        let id = id_of(input)?;
        let actor = Actor::of(p);
        let look = lookup_fn(&eng);
        let reads = Plain(eng.backend());
        let def = eng.def(name).ok_or_else(|| format!("no model {name}"))?;
        Gate::new(&reads, &look, &actor).load(&def, &id)?;
        match referrers(&eng, &actor, name, &id)? {
            Some(why) => Err(why),
            None => Ok(Verdict::default()),
        }
    })
}

impl Dispatcher {
    /// Register the guards the registry metadata of `module` implies (see the module docs).
    pub(crate) fn register_guards(&self, module: &Module, store: &Store) {
        let eng = self.engine(store);
        for def in &module.models {
            let id = |c: &str| format!("{}.{c}", def.name);
            if let Some(g) = create_guard(def) {
                self.register_guard(&module.id, &id("create"), true, g);
            }
            if let Some(g) = update_guard(def, &eng) {
                self.register_guard(&module.id, &id("update"), true, g);
            }
            for (c, _) in def.transitions() {
                self.register_guard(
                    &module.id,
                    &id(c.name),
                    true,
                    action_guard(def, &eng, c.name),
                );
            }
            self.register_guard(&module.id, &id("delete"), true, delete_guard(def, &eng));
            for c in ["list", "search"] {
                self.register_guard(&module.id, &id(c), false, query_guard(def));
            }
        }
    }
}
