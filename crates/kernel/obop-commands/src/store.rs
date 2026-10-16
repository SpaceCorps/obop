//! In-memory record store and the generated CRUD handlers. Stands in for `obop-orm` until a
//! database exists; it implements the same command contract, so modules do not change.

use crate::{Dispatcher, Value};
use obop_registry::{ModelDef, Module};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

type Rows = BTreeMap<String, BTreeMap<String, Value>>;

#[derive(Clone, Default)]
pub struct Store {
    models: Arc<Mutex<BTreeMap<&'static str, Rows>>>,
}

impl Store {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a record, generating a UUIDv7 id (D9). Returns the id.
    pub fn insert(&self, model: &'static str, fields: BTreeMap<String, Value>) -> String {
        let id = uuid::Uuid::now_v7().to_string();
        self.models
            .lock()
            .unwrap()
            .entry(model)
            .or_default()
            .insert(id.clone(), fields);
        id
    }

    /// Number of records of a model.
    pub fn count(&self, model: &'static str) -> usize {
        self.models
            .lock()
            .unwrap()
            .get(model)
            .map_or(0, |r| r.len())
    }

    /// Read one field of a record.
    pub fn field(&self, model: &'static str, id: &str, field: &str) -> Option<Value> {
        self.models
            .lock()
            .unwrap()
            .get(model)?
            .get(id)?
            .get(field)
            .cloned()
    }

    /// Overwrite fields of an existing record.
    pub fn update(
        &self,
        model: &'static str,
        id: &str,
        fields: BTreeMap<String, Value>,
    ) -> Result<(), String> {
        let mut g = self.models.lock().unwrap();
        let f = g
            .get_mut(model)
            .and_then(|r| r.get_mut(id))
            .ok_or_else(|| format!("no {model} {id}"))?;
        f.extend(fields);
        Ok(())
    }

    fn row(id: &str, f: &BTreeMap<String, Value>) -> Value {
        let mut m = f.clone();
        m.insert("id".into(), Value::text(id));
        Value::Map(m)
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
fn declared_fields(
    def: &ModelDef,
    input: &Value,
    skip_id: bool,
) -> Result<BTreeMap<String, Value>, String> {
    let Value::Map(m) = input else {
        return Err("expected an object".into());
    };
    let mut out = BTreeMap::new();
    for (k, v) in m {
        if skip_id && k == "id" {
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
                    let g = s.models.lock().unwrap();
                    let f = g
                        .get(name)
                        .and_then(|r| r.get(&id))
                        .ok_or_else(|| format!("no {name} {id}"))?;
                    Ok(Store::row(&id, f))
                }),
            );
            let s = store.clone();
            self.register(
                module,
                name,
                "list",
                Box::new(move |_| {
                    let g = s.models.lock().unwrap();
                    Ok(Value::List(
                        g.get(name)
                            .map(|r| r.iter().map(|(id, f)| Store::row(id, f)).collect())
                            .unwrap_or_default(),
                    ))
                }),
            );
            let (s, d) = (store.clone(), def.clone());
            self.register(
                module,
                name,
                "create",
                Box::new(move |input| {
                    let fields = declared_fields(&d, input, true)?;
                    let id = s.insert(name, fields.clone());
                    Ok(Store::row(&id, &fields))
                }),
            );
            let (s, d) = (store.clone(), def.clone());
            self.register(
                module,
                name,
                "update",
                Box::new(move |input| {
                    let id = id_of(input)?;
                    let fields = declared_fields(&d, input, true)?;
                    let mut g = s.models.lock().unwrap();
                    let f = g
                        .get_mut(name)
                        .and_then(|r| r.get_mut(&id))
                        .ok_or_else(|| format!("no {name} {id}"))?;
                    f.extend(fields);
                    Ok(Store::row(&id, f))
                }),
            );
        }
    }
}
