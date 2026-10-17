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

use crate::query::Query;
use crate::store::{Store, declared_fields, id_of, validate};
use crate::{Dispatcher, Guard, ModelMeta, Pre, Value, Verdict};
use obop_masking::mode_for;
use obop_registry::{ModelDef, Module, Transition};
use obop_storage::{Backend, Fields};
use obop_types::{DataClass, MaskMode};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, RwLock};

/// The first required field that is missing or null in `rec`.
pub(crate) fn required_missing(def: &ModelDef, rec: &Fields) -> Option<&'static str> {
    def.fields
        .iter()
        .find(|f| f.required && rec.get(f.name).is_none_or(serde_json::Value::is_null))
        .map(|f| f.name)
}

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

fn update_guard(def: &ModelDef, store: &Store) -> Option<Guard> {
    let governed = def.governed_fields();
    if governed.is_empty() {
        return None;
    }
    let (def, store) = (def.clone(), store.clone());
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
        let current = store
            .backend
            .get(def.name, &id)?
            .ok_or_else(|| format!("no {} {id}", def.name))?;
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
            validate(&def, &store, &declared_fields(&def, input)?, false)?;
        }
        Ok(verdict)
    }))
}

/// Checks before a transition action runs: the record exists, it is in the `from` state, and
/// every required field is set.
fn action_guard(def: &ModelDef, store: &Store, t: Transition) -> Guard {
    let (def, store) = (def.clone(), store.clone());
    Box::new(move |_, input| {
        let id = id_of(input)?;
        let rec = store
            .backend
            .get(def.name, &id)?
            .ok_or_else(|| format!("no {} {id}", def.name))?;
        if let Some(from) = t.from {
            let cur = rec
                .get(t.field)
                .map(Value::from_json)
                .unwrap_or(Value::Null);
            if cur != Value::text(from) {
                return Err(format!(
                    "{} must be {from} to do this, it is {}",
                    t.field,
                    match &cur {
                        Value::Text(s) => format!("{s:?}"),
                        _ => "empty".into(),
                    }
                ));
            }
        }
        if let Some(name) = required_missing(&def, &rec) {
            return Err(format!(
                "{}.{name} is required and must be set before {} can change to {:?}",
                def.name, t.field, t.to
            ));
        }
        Ok(Verdict::default())
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
pub(crate) fn referrers(
    models: &RwLock<HashMap<String, ModelMeta>>,
    backend: &dyn Backend,
    model: &str,
    id: &str,
) -> Result<Option<String>, String> {
    let models = models.read().unwrap();
    let mut by_model: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for (name, meta) in models.iter() {
        let fields: Vec<&str> = meta
            .fields
            .iter()
            .filter(|f| f.kind.target() == Some(model))
            .map(|f| f.name)
            .collect();
        if fields.is_empty() {
            continue;
        }
        for (rid, rec) in backend.list(name)? {
            if name == model && rid == id {
                continue;
            }
            if fields
                .iter()
                .any(|f| rec.get(*f).and_then(serde_json::Value::as_str) == Some(id))
            {
                by_model.entry(name).or_default().push(rid);
            }
        }
    }
    if by_model.is_empty() {
        return Ok(None);
    }
    let parts: Vec<String> = by_model
        .iter()
        .map(|(name, ids)| {
            let shown: Vec<&str> = ids.iter().take(5).map(String::as_str).collect();
            let more = if ids.len() > 5 { ", ..." } else { "" };
            format!("{name} ({}): {}{more}", ids.len(), shown.join(", "))
        })
        .collect();
    Ok(Some(format!(
        "cannot delete {model} {id}: still referenced by {}",
        parts.join("; ")
    )))
}

fn delete_guard(
    def: &ModelDef,
    store: &Store,
    models: Arc<RwLock<HashMap<String, ModelMeta>>>,
) -> Guard {
    let (name, store) = (def.name, store.clone());
    Box::new(move |_, input| {
        let id = id_of(input)?;
        store
            .backend
            .get(name, &id)?
            .ok_or_else(|| format!("no {name} {id}"))?;
        match referrers(&models, &*store.backend, name, &id)? {
            Some(why) => Err(why),
            None => Ok(Verdict::default()),
        }
    })
}

impl Dispatcher {
    /// Register the guards the registry metadata of `module` implies (see the module docs).
    pub(crate) fn register_guards(&self, module: &Module, store: &Store) {
        for def in &module.models {
            let id = |c: &str| format!("{}.{c}", def.name);
            if let Some(g) = create_guard(def) {
                self.register_guard(&module.id, &id("create"), true, g);
            }
            if let Some(g) = update_guard(def, store) {
                self.register_guard(&module.id, &id("update"), true, g);
            }
            for (c, t) in def.transitions() {
                self.register_guard(&module.id, &id(c.name), true, action_guard(def, store, t));
            }
            self.register_guard(
                &module.id,
                &id("delete"),
                true,
                delete_guard(def, store, self.models.clone()),
            );
            for c in ["list", "search"] {
                self.register_guard(&module.id, &id(c), false, query_guard(def));
            }
        }
    }
}
