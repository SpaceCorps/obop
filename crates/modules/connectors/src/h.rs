//! Small helpers shared by the guards and the handlers.

use obop_commands::{Principal, Value};
use obop_storage::Fields;
use serde_json::Value as J;
use std::collections::BTreeMap;

use crate::meta::ADMIN;

pub type M = BTreeMap<String, Value>;

pub fn map(v: &Value) -> Result<&M, String> {
    match v {
        Value::Map(m) => Ok(m),
        _ => Err("expected an object".into()),
    }
}

pub fn txt<'a>(m: &'a M, k: &str) -> &'a str {
    match m.get(k) {
        Some(Value::Text(t)) => t,
        _ => "",
    }
}

pub fn has(m: &M, k: &str) -> bool {
    !matches!(m.get(k), None | Some(Value::Null))
}

/// The person behind a principal: an agent acts for its owner and sees what its owner owns.
pub fn owner_id(p: &Principal) -> String {
    p.id.strip_suffix("/agent").unwrap_or(&p.id).to_string()
}

/// Who is calling, as the guards place it in the input (`_actor*` keys, set on every call and
/// never taken from a client).
#[derive(Debug, Clone)]
pub struct Actor {
    pub id: String,
    pub admin: bool,
    /// Groups the caller holds and may use.
    pub groups: Vec<String>,
}

pub fn put_actor(p: &Principal, m: &mut M) {
    m.insert("_actor".into(), Value::text(&owner_id(p)));
    m.insert("_actor_admin".into(), Value::Bool(p.allows(ADMIN)));
    let mut groups: Vec<String> = p.groups.iter().filter(|g| p.allows(g)).cloned().collect();
    groups.sort();
    m.insert(
        "_actor_groups".into(),
        Value::List(groups.iter().map(|g| Value::text(g)).collect()),
    );
}

pub fn strip_actor(m: &mut M) {
    m.retain(|k, _| !k.starts_with("_actor"));
}

pub fn actor(m: &M) -> Result<Actor, String> {
    let id = txt(m, "_actor");
    if id.is_empty() {
        return Err("the caller is unknown".into());
    }
    Ok(Actor {
        id: id.into(),
        admin: m.get("_actor_admin") == Some(&Value::Bool(true)),
        groups: match m.get("_actor_groups") {
            Some(Value::List(l)) => l
                .iter()
                .filter_map(|x| {
                    if let Value::Text(t) = x {
                        Some(t.clone())
                    } else {
                        None
                    }
                })
                .collect(),
            _ => vec![],
        },
    })
}

pub fn row(id: &str, f: &Fields) -> Value {
    let mut o = f.clone();
    o.insert("id".into(), J::from(id));
    Value::from_json(&J::Object(o))
}

pub fn fields_of(pairs: &[(&str, J)]) -> Fields {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}
