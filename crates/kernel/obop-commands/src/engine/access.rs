//! Row-level access (D58): which records of a model a caller may see and change, in addition to
//! the group checks of the model.
//!
//! A record is visible when any declared rule grants it: the caller is the `own` of the record,
//! the record's `via` parent is visible, the `public_field` is true, or the caller is listed in
//! `shared_with`. The holders of the `see_all` groups and the instance administrator see every record.
//! A model without `[model.access]` is not restricted. A record the caller cannot see is treated
//! exactly like a record that does not exist, in every command and in every error message.

use super::Actor;
use super::computed::Computer;
use super::unit::{R, Reads};
use obop_registry::expr::V;
use obop_registry::{Access, ModelDef};
use obop_storage::Fields;
use serde_json::Value as J;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

/// How far `via` chains are followed.
const MAX_HOPS: usize = 4;

pub(crate) struct Gate<'a> {
    reads: &'a dyn Reads,
    lookup: &'a dyn Fn(&str) -> Option<Arc<ModelDef>>,
    actor: &'a Actor,
    cache: RefCell<HashMap<(String, String), bool>>,
}

fn text<'f>(rec: &'f Fields, key: &str) -> Option<&'f str> {
    match rec.get(key) {
        Some(J::String(s)) if !s.is_empty() => Some(s),
        _ => None,
    }
}

impl<'a> Gate<'a> {
    pub fn new(
        reads: &'a dyn Reads,
        lookup: &'a dyn Fn(&str) -> Option<Arc<ModelDef>>,
        actor: &'a Actor,
    ) -> Self {
        Self {
            reads,
            lookup,
            actor,
            cache: RefCell::new(HashMap::new()),
        }
    }

    /// Whether the model restricts records at all for this caller.
    pub fn restricts(&self, def: &ModelDef) -> bool {
        def.access.as_ref().is_some_and(|a| !a.is_empty()) && !self.actor.admin
    }

    fn grants(&self, def: &ModelDef, a: &Access, rec: &Fields, hops: usize) -> bool {
        if a.see_all.iter().any(|g| self.actor.holds(g)) {
            return true;
        }
        if let Some(f) = a.own
            && !self.actor.id.is_empty()
            && text(rec, f) == Some(self.actor.id.as_str())
        {
            return true;
        }
        if let Some(f) = a.public_field {
            // The flag may be a computed bool (a confidentiality level that is not secret).
            let computed = def
                .fields
                .iter()
                .find(|d| d.name == f)
                .and_then(|d| d.computed.clone());
            let open = match computed {
                Some(c) => {
                    Computer::new(self.reads, self.lookup, false)
                        .evaluate(def, "", rec, &c.expr)
                        .ok()
                        == Some(V::Bool(true))
                }
                None => rec.get(f) == Some(&J::Bool(true)),
            };
            if open {
                return true;
            }
        }
        if let Some(f) = a.shared_with
            && !self.actor.id.is_empty()
            && text(rec, f).is_some_and(|list| {
                list.split([',', ' ', ';', '\n'])
                    .any(|u| u.trim() == self.actor.id)
            })
        {
            return true;
        }
        if let Some(f) = a.via
            && hops < MAX_HOPS
            && let Some(pid) = text(rec, f)
            && let Some(parent) = def
                .fields
                .iter()
                .find(|d| d.name == f)
                .and_then(|d| d.kind.target())
                .and_then(|t| (self.lookup)(t))
        {
            return self.visible_at(&parent, pid, hops + 1);
        }
        false
    }

    fn visible_at(&self, def: &ModelDef, id: &str, hops: usize) -> bool {
        let Some(a) = def.access.as_ref().filter(|a| !a.is_empty()) else {
            // The parent has no rules of its own: it must merely exist.
            return self.reads.get(def.name, id).ok().flatten().is_some();
        };
        if self.actor.admin {
            return self.reads.get(def.name, id).ok().flatten().is_some();
        }
        let key = (def.name.to_string(), id.to_string());
        if let Some(v) = self.cache.borrow().get(&key) {
            return *v;
        }
        let v = match self.reads.get(def.name, id) {
            Ok(Some(rec)) => self.grants(def, a, &rec, hops),
            _ => false,
        };
        self.cache.borrow_mut().insert(key, v);
        v
    }

    /// Whether the caller may see `rec`, a record of `def`.
    pub fn visible(&self, def: &ModelDef, rec: &Fields) -> bool {
        match &def.access {
            Some(a) if !a.is_empty() && !self.actor.admin => self.grants(def, a, rec, 0),
            _ => true,
        }
    }

    /// Keep the rows the caller may see.
    pub fn retain(&self, def: &ModelDef, rows: &mut Vec<(String, Fields)>) {
        if self.restricts(def) {
            rows.retain(|(_, f)| self.visible(def, f));
        }
    }

    /// A record the caller may see, or the error for a record that does not exist.
    pub fn load(&self, def: &ModelDef, id: &str) -> R<Fields> {
        match self.reads.get(def.name, id)? {
            Some(rec) if self.visible(def, &rec) => Ok(rec),
            _ => Err(format!("no {} {id}", def.name)),
        }
    }
}

/// What a create may set so that the creator still sees the record, as an error message for
/// the caller, or `None` when it is fine.
pub(crate) fn creator_blocked(gate: &Gate<'_>, def: &ModelDef, rec: &Fields) -> Option<String> {
    if !gate.restricts(def) || gate.visible(def, rec) {
        return None;
    }
    let a = def.access.as_ref()?;
    let how = match (a.own, a.via) {
        (Some(o), _) => format!("set {o} to yourself"),
        (None, Some(v)) => format!("set {v} to a record you can see"),
        _ => "you may not create records you could not see afterwards".to_string(),
    };
    Some(format!(
        "{} would not be visible to you ({how}), so it was not saved",
        def.name
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::unit::Plain;
    use obop_registry::{FieldDef, FieldKind};
    use obop_types::DataClass;

    fn f(name: &'static str) -> FieldDef {
        FieldDef::new(name, "d", DataClass::None, false, "t.user")
    }

    fn rec(pairs: &[(&str, J)]) -> Fields {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn rules_grant_and_the_rest_is_invisible() {
        let project = Arc::new(ModelDef {
            name: "t_project",
            description: "d",
            read_group: "t.user",
            write_group: "t.user",
            fields: vec![
                f("owner"),
                f("open").kind(FieldKind::Bool),
                f("members").kind(FieldKind::LongText),
            ],
            commands: vec![],
            access: Some(Access {
                own: Some("owner"),
                public_field: Some("open"),
                shared_with: Some("members"),
                see_all: vec!["t.manager"],
                via: None,
            }),
        });
        let task = Arc::new(ModelDef {
            name: "t_task",
            description: "d",
            read_group: "t.user",
            write_group: "t.user",
            fields: vec![f("project_id").reference("t_project")],
            commands: vec![],
            access: Some(Access {
                via: Some("project_id"),
                ..Access::default()
            }),
        });
        let (p, t) = (project.clone(), task.clone());
        let lookup = move |n: &str| match n {
            "t_project" => Some(p.clone()),
            "t_task" => Some(t.clone()),
            _ => None,
        };
        let b = obop_storage::open("memory:").unwrap();
        b.insert("t_project", "mine", &rec(&[("owner", "ann".into())]))
            .unwrap();
        b.insert("t_project", "theirs", &rec(&[("owner", "bob".into())]))
            .unwrap();
        b.insert(
            "t_project",
            "open",
            &rec(&[("owner", "bob".into()), ("open", true.into())]),
        )
        .unwrap();
        b.insert(
            "t_project",
            "shared",
            &rec(&[("owner", "bob".into()), ("members", "cy, ann".into())]),
        )
        .unwrap();
        for p in ["mine", "theirs", "open", "shared"] {
            b.insert(
                "t_task",
                &format!("task-{p}"),
                &rec(&[("project_id", p.into())]),
            )
            .unwrap();
        }
        let reads = Plain(&*b);
        let ids = |actor: &Actor, def: &Arc<ModelDef>| -> Vec<String> {
            let gate = Gate::new(&reads, &lookup, actor);
            let mut rows = reads.all(def.name).unwrap();
            gate.retain(def, &mut rows);
            rows.into_iter().map(|(i, _)| i).collect()
        };
        let ann = Actor {
            id: "ann".into(),
            ..Actor::default()
        };
        assert_eq!(ids(&ann, &project), ["mine", "open", "shared"]);
        // Tasks follow their project.
        assert_eq!(ids(&ann, &task), ["task-mine", "task-open", "task-shared"]);
        let nobody = Actor::default();
        assert_eq!(ids(&nobody, &project), ["open"], "no id owns nothing");
        let manager = Actor {
            id: "mia".into(),
            groups: ["t.manager".to_string()].into(),
            ..Actor::default()
        };
        assert_eq!(ids(&manager, &project).len(), 4);
        assert_eq!(
            ids(&manager, &task).len(),
            4,
            "the parent's see-all carries over"
        );
        let admin = Actor {
            admin: true,
            ..Actor::default()
        };
        assert_eq!(ids(&admin, &project).len(), 4);
        let gate = Gate::new(&reads, &lookup, &ann);
        assert!(gate.load(&project, "theirs").unwrap_err() == "no t_project theirs");
        assert!(gate.load(&project, "ghost").unwrap_err() == "no t_project ghost");
        assert!(creator_blocked(&gate, &project, &rec(&[("owner", "bob".into())])).is_some());
        assert!(creator_blocked(&gate, &project, &rec(&[("owner", "ann".into())])).is_none());
    }
}
