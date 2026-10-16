//! Record store and the generated CRUD handlers. A thin layer over an `obop-storage` backend
//! (memory, SQLite or PostgreSQL); it implements the command contract, so modules do not change
//! when the backend does.

use crate::{Dispatcher, Value};
use obop_registry::{ModelDef, Module};
use obop_storage::{Backend, Fields};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct Store {
    backend: Arc<dyn Backend>,
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

fn to_fields(m: &BTreeMap<String, Value>) -> Fields {
    m.iter().map(|(k, v)| (k.clone(), v.to_json())).collect()
}

fn to_row(id: &str, f: &Fields) -> Value {
    let mut m: BTreeMap<String, Value> = f
        .iter()
        .map(|(k, v)| (k.clone(), Value::from_json(v)))
        .collect();
    m.insert("id".into(), Value::text(id));
    Value::Map(m)
}

impl Store {
    /// A fresh in-memory store.
    pub fn new() -> Self {
        Self::open("memory:").expect("memory backend")
    }

    /// Open `memory:`, `sqlite://PATH` or `postgres://...`.
    pub fn open(url: &str) -> Result<Self, String> {
        Ok(Self {
            backend: obop_storage::open(url)?,
        })
    }

    pub fn backend_kind(&self) -> &'static str {
        self.backend.kind()
    }

    /// Insert a record under a new UUIDv7 id (D9). Returns the id.
    pub fn insert(&self, model: &str, fields: BTreeMap<String, Value>) -> Result<String, String> {
        let id = uuid::Uuid::now_v7().to_string();
        self.backend.insert(model, &id, &to_fields(&fields))?;
        Ok(id)
    }

    pub fn count(&self, model: &str) -> usize {
        self.backend.count(model).unwrap_or(0)
    }

    /// Read one field of a record.
    pub fn field(&self, model: &str, id: &str, field: &str) -> Option<Value> {
        self.backend
            .get(model, id)
            .ok()??
            .get(field)
            .map(Value::from_json)
    }

    /// Merge fields into an existing record.
    pub fn update(
        &self,
        model: &str,
        id: &str,
        fields: BTreeMap<String, Value>,
    ) -> Result<(), String> {
        if self.backend.update(model, id, &to_fields(&fields))? {
            Ok(())
        } else {
            Err(format!("no {model} {id}"))
        }
    }
}

fn id_of(input: &Value) -> Result<String, String> {
    match input {
        Value::Map(m) => match m.get("id") {
            Some(Value::Text(id)) => Ok(id.clone()),
            _ => Err("missing id".into()),
        },
        _ => Err("expected an object".into()),
    }
}

/// Reject fields the model does not declare, so nothing unclassified enters the store.
fn declared_fields(def: &ModelDef, input: &Value) -> Result<BTreeMap<String, Value>, String> {
    let Value::Map(m) = input else {
        return Err("expected an object".into());
    };
    let mut out = BTreeMap::new();
    for (k, v) in m {
        if k == "id" {
            continue;
        }
        if !def.fields.iter().any(|f| f.name == k) {
            return Err(format!("unknown field {}.{k}", def.name));
        }
        out.insert(k.clone(), v.clone());
    }
    Ok(out)
}

impl Dispatcher {
    /// Register `get`, `list`, `create` and `update` for every model of `module` against `store`.
    pub fn register_crud(&mut self, module: &Module, store: &Store) {
        for def in &module.models {
            let name = def.name;
            let s = store.clone();
            self.register(
                module,
                name,
                "get",
                Box::new(move |input| {
                    let id = id_of(input)?;
                    let f = s
                        .backend
                        .get(name, &id)?
                        .ok_or_else(|| format!("no {name} {id}"))?;
                    Ok(to_row(&id, &f))
                }),
            );
            let s = store.clone();
            self.register(
                module,
                name,
                "list",
                Box::new(move |_| {
                    Ok(Value::List(
                        s.backend
                            .list(name)?
                            .iter()
                            .map(|(id, f)| to_row(id, f))
                            .collect(),
                    ))
                }),
            );
            let (s, d) = (store.clone(), def.clone());
            self.register(
                module,
                name,
                "create",
                Box::new(move |input| {
                    let fields = declared_fields(&d, input)?;
                    let id = s.insert(name, fields)?;
                    let f = s.backend.get(name, &id)?.ok_or("record vanished")?;
                    Ok(to_row(&id, &f))
                }),
            );
            let (s, d) = (store.clone(), def.clone());
            self.register(
                module,
                name,
                "update",
                Box::new(move |input| {
                    let id = id_of(input)?;
                    s.update(name, &id, declared_fields(&d, input)?)?;
                    let f = s.backend.get(name, &id)?.ok_or("record vanished")?;
                    Ok(to_row(&id, &f))
                }),
            );
        }
    }
}
