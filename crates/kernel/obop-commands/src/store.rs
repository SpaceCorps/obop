//! Record store and the generated CRUD handlers. A thin layer over an `obop-storage` backend
//! (memory, SQLite or PostgreSQL); it implements the command contract, so modules do not change
//! when the backend does.

use crate::query::Query;
use crate::{Dispatcher, Value, guards};
use obop_registry::{FieldKind, ModelDef, Module, is_valid_date};
use obop_storage::{Backend, Fields};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct Store {
    pub(crate) backend: Arc<dyn Backend>,
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

    /// The backend, for components that keep their own records (identity).
    pub fn backend(&self) -> Arc<dyn Backend> {
        self.backend.clone()
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

pub(crate) fn id_of(input: &Value) -> Result<String, String> {
    match input {
        Value::Map(m) => match m.get("id") {
            Some(Value::Text(id)) => Ok(id.clone()),
            _ => Err("missing id".into()),
        },
        _ => Err("expected an object".into()),
    }
}

/// Reject fields the model does not declare, so nothing unclassified enters the store.
pub(crate) fn declared_fields(
    def: &ModelDef,
    input: &Value,
) -> Result<BTreeMap<String, Value>, String> {
    let Value::Map(m) = input else {
        return Err("expected an object".into());
    };
    let mut out = BTreeMap::new();
    for (k, v) in m {
        if k == "id" || k == "_caller" || k.starts_with(crate::query::ACTOR_PREFIX) {
            continue;
        }
        if !def.fields.iter().any(|f| f.name == k) {
            return Err(format!("unknown field {}.{k}", def.name));
        }
        out.insert(k.clone(), v.clone());
    }
    Ok(out)
}

/// Check one non-null value against its field kind (D47). Tokens are already restored by the
/// dispatcher, so this always sees real values.
fn check_value(
    store: &Store,
    pending: &[(String, String)],
    model: &str,
    name: &str,
    kind: &FieldKind,
    v: &Value,
) -> Result<(), String> {
    match (kind, v) {
        (FieldKind::Text | FieldKind::LongText, Value::Text(_)) => Ok(()),
        (FieldKind::Number | FieldKind::Money, Value::Int(_)) => Ok(()),
        (FieldKind::Bool, Value::Bool(_)) => Ok(()),
        (FieldKind::Date, Value::Text(t)) if is_valid_date(t) => Ok(()),
        (FieldKind::Select(o), Value::Text(t)) if o.contains(&t.as_str()) => Ok(()),
        (FieldKind::Ref(to), Value::Text(id))
            if pending.iter().any(|(m, i)| m == to && i == id) =>
        {
            Ok(())
        }
        (FieldKind::Ref(to), Value::Text(id)) => match store.backend.get(to, id) {
            Ok(Some(_)) => Ok(()),
            Ok(None) => Err(format!(
                "{model}.{name} must be {}; there is no {to} {id}",
                kind.describe()
            )),
            Err(e) => Err(e),
        },
        _ => Err(format!("{model}.{name} must be {}", kind.describe())),
    }
}

/// Validate the fields of a create (`creating`) or an update against the model's field kinds.
pub fn validate(
    def: &ModelDef,
    store: &Store,
    fields: &BTreeMap<String, Value>,
    creating: bool,
) -> Result<(), String> {
    validate_with(def, store, fields, creating, &[])
}

/// Like `validate`, but a ref may also point at a record of `pending` (model, id) pairs that a
/// batch is about to insert together with this one.
pub fn validate_with(
    def: &ModelDef,
    store: &Store,
    fields: &BTreeMap<String, Value>,
    creating: bool,
    pending: &[(String, String)],
) -> Result<(), String> {
    for f in &def.fields {
        match fields.get(f.name) {
            None if creating && f.required => {
                return Err(format!("{}.{} is required", def.name, f.name));
            }
            None => {}
            Some(Value::Null) if f.required => {
                return Err(format!(
                    "{}.{} is required and cannot be null",
                    def.name, f.name
                ));
            }
            Some(Value::Null) => {}
            Some(v) => check_value(store, pending, def.name, f.name, &f.kind, v)?,
        }
    }
    Ok(())
}

impl Dispatcher {
    /// Register `get`, `list`, `create` and `update` for every model of `module` against `store`.
    pub fn register_crud(&self, module: &Module, store: &Store) {
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
            let (s, d) = (store.clone(), def.clone());
            self.register(
                module,
                name,
                "list",
                Box::new(move |input| {
                    let (rows, _) = run_query(&s, &d, input)?;
                    Ok(Value::List(rows))
                }),
            );
            let (s, d) = (store.clone(), def.clone());
            self.register(
                module,
                name,
                "search",
                Box::new(move |input| {
                    let (rows, total) = run_query(&s, &d, input)?;
                    Ok(Value::map([
                        ("rows", Value::List(rows)),
                        ("total", Value::Int(total as i64)),
                    ]))
                }),
            );
            let (s, models) = (store.clone(), self.models.clone());
            self.register(
                module,
                name,
                "delete",
                Box::new(move |input| {
                    let id = id_of(input)?;
                    // Checked again here, not only in the guard, so the rule holds for any caller
                    // of the handler.
                    if let Some(why) = guards::referrers(&models, &*s.backend, name, &id)? {
                        return Err(why);
                    }
                    if !s.backend.delete(name, &id)? {
                        return Err(format!("no {name} {id}"));
                    }
                    Ok(Value::map([("id", Value::text(&id))]))
                }),
            );
            let (s, d) = (store.clone(), def.clone());
            self.register(
                module,
                name,
                "create",
                Box::new(move |input| {
                    let fields = declared_fields(&d, input)?;
                    validate(&d, &s, &fields, true)?;
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
                    let fields = declared_fields(&d, input)?;
                    validate(&d, &s, &fields, false)?;
                    s.update(name, &id, fields)?;
                    let f = s.backend.get(name, &id)?.ok_or("record vanished")?;
                    Ok(to_row(&id, &f))
                }),
            );
        }
        self.register_guards(module, store);
    }

    /// Register only `get`, `list` and `search` for the named models of `module` (with the query
    /// guard of `list` and `search`). For records that are written by the platform itself and
    /// must not be created, changed or deleted through a command (logs, runs, computed views).
    pub fn register_read_only(&self, module: &Module, store: &Store, models: &[&str]) {
        for def in module.models.iter().filter(|d| models.contains(&d.name)) {
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
            let (s, d) = (store.clone(), def.clone());
            self.register(
                module,
                name,
                "list",
                Box::new(move |input| {
                    let (rows, _) = run_query(&s, &d, input)?;
                    Ok(Value::List(rows))
                }),
            );
            let (s, d) = (store.clone(), def.clone());
            self.register(
                module,
                name,
                "search",
                Box::new(move |input| {
                    let (rows, total) = run_query(&s, &d, input)?;
                    Ok(Value::map([
                        ("rows", Value::List(rows)),
                        ("total", Value::Int(total as i64)),
                    ]))
                }),
            );
            for c in ["list", "search"] {
                self.register_guard(
                    &module.id,
                    &format!("{name}.{c}"),
                    false,
                    guards::query_guard(def),
                );
            }
        }
    }

    /// Bind the generic handler of a command declared as a transition (D48): it sets the field
    /// to the declared value. Its checks (current state, required fields) are the command's
    /// guard, registered by `register_crud`.
    pub fn register_transition(&self, module: &Module, model: &str, command: &str, store: &Store) {
        let t = module
            .models
            .iter()
            .find(|m| m.name == model)
            .and_then(|m| m.commands.iter().find(|c| c.name == command))
            .and_then(|c| c.transition)
            .unwrap_or_else(|| panic!("{model}.{command} is not declared as a transition"));
        let (s, name) = (store.clone(), obop_registry::intern(model));
        self.register(
            module,
            model,
            command,
            Box::new(move |input| {
                let id = id_of(input)?;
                s.update(
                    name,
                    &id,
                    BTreeMap::from([(t.field.to_string(), Value::text(t.to))]),
                )?;
                Ok(Value::map([
                    ("id", Value::text(&id)),
                    (t.field, Value::text(t.to)),
                ]))
            }),
        );
    }
}

/// Run the optional filter, order, limit and offset of `list` and `search` over the records of a
/// model. Returns the rows of the page and the number of matches before paging.
fn run_query(s: &Store, d: &ModelDef, input: &Value) -> Result<(Vec<Value>, usize), String> {
    // The dispatcher's guard has already refused fields the caller cannot read; the handler
    // only needs the structure.
    let q = Query::parse(d, input, &|_| true)?;
    let all = s.backend.list(d.name)?;
    let (page, total) = q.apply(all);
    Ok((page.iter().map(|(id, f)| to_row(id, f)).collect(), total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Principal, Surface, Vault};
    use obop_registry::{FieldDef, ModelDef};
    use obop_types::DataClass;

    fn f(name: &'static str) -> FieldDef {
        FieldDef::new(name, "d", DataClass::None, false, "t.user")
    }

    fn setup() -> (Dispatcher, Store, Principal) {
        let model = |name, fields| ModelDef {
            name,
            description: "d",
            read_group: "t.user",
            write_group: "t.user",
            fields,
            commands: vec![],
        };
        let module = Module {
            id: "t".into(),
            name: "T".into(),
            models: vec![
                model("customer", vec![f("name").required()]),
                model(
                    "deal",
                    vec![
                        f("title").required(),
                        f("notes").kind(FieldKind::LongText),
                        f("qty").kind(FieldKind::Number),
                        f("price").kind(FieldKind::Money),
                        f("due").kind(FieldKind::Date),
                        f("won").kind(FieldKind::Bool),
                        f("stage").select(&["new", "won"]),
                        f("customer_id").reference("customer"),
                    ],
                ),
            ],
            network_destinations: vec![],
            description: String::new(),
            translations: Default::default(),
        };
        let (d, s) = (Dispatcher::new(), Store::new());
        d.register_crud(&module, &s);
        (d, s, Principal::user("u", &["t.user"]))
    }

    fn call(
        d: &Dispatcher,
        u: &Principal,
        cmd: &str,
        pairs: Vec<(&str, Value)>,
    ) -> Result<Value, String> {
        d.call(
            Surface::Rest,
            u,
            &mut Vault::new(),
            cmd,
            &Value::Map(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()),
        )
        .map(|r| r.value)
        .map_err(|e| match e {
            crate::CallError::Handler(m) => m,
            other => format!("{other:?}"),
        })
    }

    fn deal(d: &Dispatcher, u: &Principal, extra: Vec<(&str, Value)>) -> Result<Value, String> {
        let mut v = vec![("title", Value::text("t"))];
        v.extend(extra);
        call(d, u, "deal.create", v)
    }

    fn rejects(r: Result<Value, String>, want: &str) {
        let e = r.expect_err("should be rejected");
        assert!(e.contains(want), "{e:?} should contain {want:?}");
    }

    #[test]
    fn valid_values_of_every_kind_are_accepted() {
        let (d, _, u) = setup();
        let c = call(
            &d,
            &u,
            "customer.create",
            vec![("name", Value::text("Acme"))],
        )
        .unwrap();
        let Value::Map(c) = c else { panic!() };
        deal(
            &d,
            &u,
            vec![
                ("notes", Value::text("a\nb")),
                ("qty", Value::Int(-3)),
                ("price", Value::Int(1999)),
                ("due", Value::text("2024-02-29")),
                ("won", Value::Bool(false)),
                ("stage", Value::text("won")),
                ("customer_id", c["id"].clone()),
            ],
        )
        .unwrap();
    }

    #[test]
    fn wrong_json_types_are_rejected_with_the_field_named() {
        let (d, _, u) = setup();
        rejects(
            deal(&d, &u, vec![("qty", Value::text("3"))]),
            "deal.qty must be an integer",
        );
        rejects(
            deal(&d, &u, vec![("won", Value::text("true"))]),
            "deal.won must be true or false",
        );
        rejects(
            deal(&d, &u, vec![("notes", Value::Int(1))]),
            "deal.notes must be a string",
        );
        rejects(
            deal(&d, &u, vec![("title", Value::Bool(true))]),
            "deal.title must be a string",
        );
    }

    #[test]
    fn money_must_be_an_integer() {
        let (d, _, u) = setup();
        // A JSON float reaches the store as non-integer text, so it is not an integer either.
        let float: serde_json::Value = serde_json::json!(12.5);
        rejects(
            deal(&d, &u, vec![("price", Value::from_json(&float))]),
            "deal.price must be an integer in minor currency units",
        );
        rejects(
            deal(&d, &u, vec![("price", Value::text("1999"))]),
            "deal.price must be an integer",
        );
        assert!(deal(&d, &u, vec![("price", Value::Int(0))]).is_ok());
    }

    #[test]
    fn dates_must_be_real_calendar_dates() {
        let (d, _, u) = setup();
        for ok in ["2024-02-29", "2000-02-29", "1999-12-31"] {
            deal(&d, &u, vec![("due", Value::text(ok))]).unwrap();
        }
        for bad in [
            "2023-02-29",
            "1900-02-29",
            "2024-13-01",
            "2024-04-31",
            "01/02/2024",
        ] {
            rejects(
                deal(&d, &u, vec![("due", Value::text(bad))]),
                "deal.due must be a real calendar date",
            );
        }
        rejects(
            deal(&d, &u, vec![("due", Value::Int(20240101))]),
            "deal.due must be",
        );
    }

    #[test]
    fn select_must_be_one_of_the_options() {
        let (d, _, u) = setup();
        rejects(
            deal(&d, &u, vec![("stage", Value::text("lost"))]),
            "deal.stage must be one of: new, won",
        );
        rejects(
            deal(&d, &u, vec![("stage", Value::Int(1))]),
            "one of: new, won",
        );
    }

    #[test]
    fn ref_must_point_at_an_existing_record_of_the_target_model() {
        let (d, s, u) = setup();
        rejects(
            deal(&d, &u, vec![("customer_id", Value::text("nope"))]),
            "there is no customer nope",
        );
        // A real id of the wrong model is dangling too.
        let other = deal(&d, &u, vec![]).unwrap();
        let Value::Map(other) = other else { panic!() };
        rejects(
            deal(&d, &u, vec![("customer_id", other["id"].clone())]),
            "deal.customer_id must be the id of an existing customer",
        );
        rejects(
            deal(&d, &u, vec![("customer_id", Value::Int(1))]),
            "must be the id of",
        );
        let c = s
            .insert("customer", [("name".to_string(), Value::text("A"))].into())
            .unwrap();
        deal(&d, &u, vec![("customer_id", Value::text(&c))]).unwrap();
    }

    #[test]
    fn required_is_enforced_on_create_and_null_on_update() {
        let (d, _, u) = setup();
        rejects(
            call(&d, &u, "deal.create", vec![]),
            "deal.title is required",
        );
        rejects(
            call(&d, &u, "deal.create", vec![("title", Value::Null)]),
            "deal.title is required",
        );
        let made = deal(&d, &u, vec![]).unwrap();
        let Value::Map(made) = made else { panic!() };
        let id = made["id"].clone();
        rejects(
            call(
                &d,
                &u,
                "deal.update",
                vec![("id", id.clone()), ("title", Value::Null)],
            ),
            "cannot be null",
        );
        // Absent on update is fine, and null clears an optional field.
        call(
            &d,
            &u,
            "deal.update",
            vec![("id", id.clone()), ("qty", Value::Int(2))],
        )
        .unwrap();
        call(
            &d,
            &u,
            "deal.update",
            vec![("id", id), ("qty", Value::Null)],
        )
        .unwrap();
    }

    #[test]
    fn update_validates_like_create() {
        let (d, _, u) = setup();
        let made = deal(&d, &u, vec![]).unwrap();
        let Value::Map(made) = made else { panic!() };
        rejects(
            call(
                &d,
                &u,
                "deal.update",
                vec![("id", made["id"].clone()), ("stage", Value::text("zzz"))],
            ),
            "deal.stage must be one of",
        );
    }
}
