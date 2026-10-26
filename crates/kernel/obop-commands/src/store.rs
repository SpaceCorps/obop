//! Record store and the generated CRUD handlers. A thin layer over an `obop-storage` backend
//! (memory, SQLite or PostgreSQL); it implements the command contract, so modules do not change
//! when the backend does.

use crate::engine::{Engine, crud, seq};
use crate::{Dispatcher, Value, guards};
use obop_registry::{FieldKind, ModelDef, Module, is_valid_date};
use obop_storage::blob::{Blobs, Catalog};
use obop_storage::{Backend, Fields};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct Store {
    pub(crate) backend: Arc<dyn Backend>,
    /// Images and files held by records (D60); the metadata lives in `backend`.
    pub(crate) blobs: Arc<dyn Blobs>,
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

fn to_fields(m: &BTreeMap<String, Value>) -> Fields {
    m.iter().map(|(k, v)| (k.clone(), v.to_json())).collect()
}

impl Store {
    /// A fresh in-memory store.
    pub fn new() -> Self {
        Self::open("memory:").expect("memory backend")
    }

    /// Open `memory:`, `sqlite://PATH` or `postgres://...`. Blob bytes go to `OBOP_BLOB_DIR` when
    /// set, else beside a SQLite file (`<file>.blobs`), else into the database (PostgreSQL), or
    /// memory for an in-memory database (D60).
    pub fn open(url: &str) -> Result<Self, String> {
        let backend = obop_storage::open(url)?;
        let dir = std::env::var("OBOP_BLOB_DIR").ok();
        let bytes = obop_storage::blob::open_bytes(url, dir.as_deref())?;
        Ok(Self {
            blobs: Arc::new(Catalog::new(backend.clone(), bytes)),
            backend,
        })
    }

    /// The blob store (images and files).
    pub fn blobs(&self) -> Arc<dyn Blobs> {
        self.blobs.clone()
    }

    /// Use another blob store (a different byte backend, a scanner).
    pub fn with_blobs(mut self, blobs: Arc<dyn Blobs>) -> Self {
        self.blobs = blobs;
        self
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
        (FieldKind::Image(_) | FieldKind::File(_), Value::Text(id)) => {
            crate::blobs::check_field_value(store, model, name, kind, id)
        }
        (FieldKind::Date, Value::Text(t)) if is_valid_date(t) => Ok(()),
        (FieldKind::Decimal { .. } | FieldKind::Datetime, Value::Text(t)) => {
            crate::kinds::check_canonical(model, name, kind, t)
        }
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
    /// The upload and download service for images and files (D60); available once a module has
    /// been registered against a store.
    pub fn blob_service(&self) -> Option<Arc<crate::BlobService>> {
        self.blob_service.get().cloned()
    }

    pub(crate) fn engine(&self, store: &Store) -> Engine {
        Engine {
            store: store.clone(),
            models: self.models.clone(),
            clock: self.clock.0.clone(),
            zones: self.zones.clone(),
            clock_replaced: self.clock_replaced.clone(),
        }
    }

    /// Register `get`, `list` and `search` of one model (the reads of the model engine).
    fn register_reads(&self, module: &Module, def: &ModelDef, eng: &Engine) {
        let name = def.name;
        let e = eng.clone();
        self.register_generic(
            module,
            name,
            "get",
            Box::new(move |input| {
                let def = e.def(name).ok_or_else(|| format!("no model {name}"))?;
                crud::get(&e, &def, input)
            }),
        );
        let e = eng.clone();
        self.register_generic(
            module,
            name,
            "list",
            Box::new(move |input| {
                let def = e.def(name).ok_or_else(|| format!("no model {name}"))?;
                let (rows, _) = crud::query(&e, &def, input)?;
                Ok(Value::List(rows))
            }),
        );
        let e = eng.clone();
        self.register_generic(
            module,
            name,
            "search",
            Box::new(move |input| {
                let def = e.def(name).ok_or_else(|| format!("no model {name}"))?;
                crud::search(&e, &def, input)
            }),
        );
    }

    /// Register only `get`, `list` and `search` for one model, so records that other code writes
    /// (the security event log, D53) can be read on every surface and never changed through it.
    pub fn register_read_only(&self, module: &Module, model: &str, store: &Store) {
        let def = module
            .models
            .iter()
            .find(|m| m.name == model)
            .unwrap_or_else(|| panic!("{}: no model {model}", module.id));
        let eng = self.engine(store);
        self.register_reads(module, def, &eng);
    }

    /// Register `get`, `list`, `search`, `create`, `update` and `delete` for every model of
    /// `module` against `store`, and the guards the registry metadata implies.
    pub fn register_crud(&self, module: &Module, store: &Store) {
        let eng = self.engine(store);
        for def in &module.models {
            let name = def.name;
            self.register_reads(module, def, &eng);
            let e = eng.clone();
            self.register_generic(
                module,
                name,
                "delete",
                Box::new(move |input| {
                    let def = e.def(name).ok_or_else(|| format!("no model {name}"))?;
                    crud::delete(&e, &def, input)
                }),
            );
            let e = eng.clone();
            self.register_generic(
                module,
                name,
                "create",
                Box::new(move |input| {
                    let def = e.def(name).ok_or_else(|| format!("no model {name}"))?;
                    crud::create(&e, &def, input)
                }),
            );
            let e = eng.clone();
            self.register_generic(
                module,
                name,
                "bulk_create",
                Box::new(move |input| {
                    let def = e.def(name).ok_or_else(|| format!("no model {name}"))?;
                    crate::engine::bulk::bulk_create(&e, &def, input)
                }),
            );
            let e = eng.clone();
            self.register_generic(
                module,
                name,
                "update",
                Box::new(move |input| {
                    let def = e.def(name).ok_or_else(|| format!("no model {name}"))?;
                    crud::update(&e, &def, input)
                }),
            );
            // Database constraints for the unique fields (D57); a write is checked either way.
            seq::ensure_indexes(&*store.backend, def);
        }
        self.register_guards(module, store);
        self.blob_service
            .get_or_init(|| crate::BlobService::new(self, store.clone()));
        self.refresh_lock_models();
    }

    /// Register only `get`, `list` and `search` for the named models of `module` (with the query
    /// guard of `list` and `search`). For records that are written by the platform itself and
    /// must not be created, changed or deleted through a command (logs, runs, computed views).
    pub fn register_read_only_models(&self, module: &Module, store: &Store, models: &[&str]) {
        let eng = self.engine(store);
        for def in module.models.iter().filter(|d| models.contains(&d.name)) {
            self.register_reads(module, def, &eng);
            for c in ["list", "search"] {
                self.register_guard(
                    &module.id,
                    &format!("{}.{c}", def.name),
                    false,
                    guards::query_guard(def, &eng),
                );
            }
        }
    }

    /// Bind the generic handler of a command declared as a transition (D48): it checks the
    /// declared assertions, sets the field to the declared value, numbers the record where a
    /// sequence says so, and runs the declared effects, all as one unit of work (D56). Its
    /// checks (current state, required fields) are the command's guard, registered by
    /// `register_crud`.
    pub fn register_transition(&self, module: &Module, model: &str, command: &str, store: &Store) {
        module
            .models
            .iter()
            .find(|m| m.name == model)
            .and_then(|m| m.commands.iter().find(|c| c.name == command))
            .and_then(|c| c.transition)
            .unwrap_or_else(|| panic!("{model}.{command} is not declared as a transition"));
        let (e, name, cmd) = (
            self.engine(store),
            obop_registry::intern(model),
            obop_registry::intern(command),
        );
        self.register_generic(
            module,
            model,
            command,
            Box::new(move |input| {
                let def = e.def(name).ok_or_else(|| format!("no model {name}"))?;
                crud::transition(&e, &def, cmd, input)
            }),
        );
        self.refresh_lock_models();
    }
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
            access: None,
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
            meta: Default::default(),
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
        // A JSON float reaches the commands as `Value::Num`, which no kind accepts (D72).
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

    // ---- bulk create from CSV text (D70) ---------------------------------------------------

    fn bulk_csv(d: &Dispatcher, u: &Principal, csv: &str, mode: &str) -> Result<Value, String> {
        call(
            d,
            u,
            "deal.bulk_create",
            vec![("csv", Value::text(csv)), ("mode", Value::text(mode))],
        )
    }

    fn field(v: &Value, k: &str) -> Value {
        let Value::Map(m) = v else { panic!("{v:?}") };
        m.get(k).cloned().unwrap_or(Value::Null)
    }

    #[test]
    fn bulk_csv_converts_cells_by_kind_and_keeps_text_as_written() {
        let (d, s, u) = setup();
        let c = call(
            &d,
            &u,
            "customer.create",
            vec![("name", Value::text("Acme"))],
        )
        .unwrap();
        let cid = match field(&c, "id") {
            Value::Text(t) => t,
            other => panic!("{other:?}"),
        };
        // A byte order mark, semicolons, a quoted cell with a line break and a comma, a cell
        // that looks like a formula, an empty cell, true/false/yes, a date, a reference.
        let file = format!(
            "\u{feff}title;notes;qty;price;due;won;stage;customer_id\r\n\
             first;\"two\nlines, and a comma\";5;1250;2026-10-24;yes;new;{cid}\r\n\
             =1+1;;-3;0;;false;;\r\n"
        );
        let out = bulk_csv(&d, &u, &file, "all_or_nothing").unwrap();
        assert_eq!(field(&out, "created"), Value::Int(2), "{out:?}");
        let Value::List(ids) = field(&out, "ids") else {
            panic!()
        };
        let rec = |i: usize| {
            let Value::Text(id) = &ids[i] else { panic!() };
            call(&d, &u, "deal.get", vec![("id", Value::text(id))]).unwrap()
        };
        let first = rec(0);
        assert_eq!(
            field(&first, "notes"),
            Value::text("two\nlines, and a comma")
        );
        assert_eq!(field(&first, "qty"), Value::Int(5));
        assert_eq!(field(&first, "price"), Value::Int(1250));
        assert_eq!(field(&first, "won"), Value::Bool(true));
        assert_eq!(field(&first, "due"), Value::text("2026-10-24"));
        assert_eq!(field(&first, "customer_id"), Value::text(&cid));
        let second = rec(1);
        assert_eq!(
            field(&second, "title"),
            Value::text("=1+1"),
            "a cell that looks like a formula is data, kept exactly"
        );
        assert_eq!(field(&second, "qty"), Value::Int(-3));
        assert_eq!(
            field(&second, "notes"),
            Value::Null,
            "an empty cell leaves the field out"
        );
        assert_eq!(s.count("deal"), 2);
    }

    #[test]
    fn bulk_csv_row_problems_name_the_row_and_the_line() {
        let (d, s, u) = setup();
        let file = "title,qty,price,won,due\n\
                    ok,1,100,true,2026-01-31\n\
                    ,2,100,true,\n\
                    bad qty,2.5,100,true,\n\
                    bad money,1,12.50,true,\n\
                    bad bool,1,1,maybe,\n\
                    bad date,1,1,true,31/01/2026\n";
        let out = bulk_csv(&d, &u, file, "partial").unwrap();
        assert_eq!(field(&out, "created"), Value::Int(1));
        let Value::List(errs) = field(&out, "errors") else {
            panic!()
        };
        let text = format!("{errs:?}");
        for needle in [
            "title is required",
            "must be a whole number, not \\\"2.5\\\"",
            "without a decimal point",
            "must be true or false",
            "calendar date",
        ] {
            assert!(text.contains(needle), "{needle} not in {text}");
        }
        // The line of the file is given for a CSV source.
        assert_eq!(field(&errs[0], "line"), Value::Int(3));
        assert_eq!(field(&errs[0], "row"), Value::Int(2));
        assert_eq!(s.count("deal"), 1);
    }

    #[test]
    fn bulk_csv_header_and_file_problems_refuse_the_whole_file() {
        let (d, s, u) = setup();
        for (file, needle) in [
            ("title,nonsense\na,b\n", "unknown column"),
            ("id,title\n1,a\n", "\"id\" cannot be imported"),
            ("title,title\na,b\n", "appears twice"),
            ("title,qty\na\n", "1 column(s), but the header has 2"),
            ("title\n\"a\n", "never closed"),
            ("", "empty"),
        ] {
            let e = bulk_csv(&d, &u, file, "all_or_nothing").unwrap_err();
            assert!(e.contains(needle), "{e} / {needle}");
        }
        let big = format!("title\n{}\n", "x".repeat(1_048_577));
        assert!(
            bulk_csv(&d, &u, &big, "all_or_nothing")
                .unwrap_err()
                .contains("larger")
        );
        let many: String = (0..1001).map(|i| format!("t{i}\n")).collect();
        assert!(
            bulk_csv(&d, &u, &format!("title\n{many}"), "all_or_nothing")
                .unwrap_err()
                .contains("more than 1000")
        );
        assert_eq!(s.count("deal"), 0);
    }
}
