//! The import map (docs/spec/cli.md "Import map"): where an imported record came from.
//!
//! A record that `bulk_create` makes with a `source` {system, table, ids} gets one entry in the
//! `import_map` table: the model and id of the record, and the key of the row it was made from
//! (the source system, the table in it and the id of the row). The key is what makes a re-run
//! of an import safe: a key that is already mapped to a record that still exists is reported as
//! `already_imported` with that record's id, and nothing is created again.
//!
//! - *Storage.* One record per (model, source system, source table, source id), under the id
//!   [`entry_id`] (a SHA-256 of the four values), written in the same transaction as the record
//!   it describes. It is a table like `bulk_idempotency`: it is not a model anybody creates
//!   records of, so there is no `create`, `update` or `delete`, and the backup (which exports
//!   every table) holds it. An update of the record changes nothing here. A record that is
//!   deleted leaves its entry behind; the entry then counts as no longer mapped (an import of
//!   the same key creates the record again and replaces the entry) and is not shown.
//! - *Reads.* `import_map.search` (a source key to the record) and `import_map.get` (a record to
//!   its keys), both in the group [`READ_GROUP`]. An entry is shown only to a caller who may read
//!   the record it points at: the group of that model and its row rules (D58) apply, and an
//!   entry whose record the caller cannot see answers exactly like one that does not exist.

use crate::devices::instant;
use crate::engine::access::Gate;
use crate::engine::unit::{Plain, R, Reads};
use crate::engine::{Actor, Engine};
use crate::{Dispatcher, Value};
use obop_registry::{CommandDef, FieldDef, ModelDef, Module};
use obop_storage::Fields;
use obop_types::DataClass;
use serde_json::Value as J;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// The model and the storage table of the entries.
pub const MODEL: &str = "import_map";
/// The group that may ask where a record came from and which record a source key became. The
/// instance administrator holds it implicitly.
pub const READ_GROUP: &str = "api.import_map_read";

/// The longest source system, table or id, in characters.
pub const MAX_PART_CHARS: usize = 256;

/// The row code of a source key that is already mapped to a record.
pub const ALREADY_IMPORTED: &str = "already_imported";
/// The row code of a source key that two rows of one call carry.
pub const DUPLICATE_SOURCE_KEY: &str = "duplicate_source_key";

/// Registry metadata of the model. Only `get` and `search` are bound (see
/// [`Dispatcher::register_import_map`]); the entries are written by `bulk_create`.
pub fn model_def() -> ModelDef {
    let f = |name, description, class| FieldDef::new(name, description, class, false, READ_GROUP);
    ModelDef {
        name: MODEL,
        description: "Where an imported record came from: the key (source system, source table, source id) of the row of a foreign system that a record was made from, written by bulk_create when a call gives a source. `search` finds the record of a source key (give source_system, source_table, source_id and optionally model); `get` finds the keys of a record (give model and id). Only entries whose record the caller may read are shown",
        read_group: READ_GROUP,
        write_group: crate::ADMIN_GROUP,
        access: None,
        fields: vec![
            f(
                "model",
                "The model of the record that was imported",
                DataClass::Internal,
            ),
            f(
                "record",
                "The id of the record that was imported",
                DataClass::Internal,
            ),
            f(
                "source_system",
                "The foreign system the row came from, as the importer named it",
                DataClass::Internal,
            ),
            f(
                "source_table",
                "The table, sheet or file of that system the row came from",
                DataClass::Internal,
            ),
            f(
                "source_id",
                "The id of the row in that table",
                DataClass::Internal,
            ),
            f(
                "imported_at",
                "When the record was imported",
                DataClass::None,
            )
            .datetime(),
            f("imported_by", "Who imported it", DataClass::Internal),
        ],
        commands: Vec::<CommandDef>::new(),
        ext: Default::default(),
    }
}

/// What `get` and `search` of this model do, for the catalogue, the MCP tools and the API
/// document (the generated texts describe the commands of an ordinary model). `None` for any
/// other command or model.
pub(crate) fn describe(model: &str, command: &str) -> Option<&'static str> {
    if model != MODEL {
        return None;
    }
    match command {
        "get" => Some(
            "The source keys of a record that an import made: give model and id (the record). Returns a list of entries (source_system, source_table, source_id, imported_at, imported_by); empty when the record has no key or you may not read it",
        ),
        "search" => Some(
            "The record that a source key became: give source_system, source_table and source_id, and optionally model. Returns {rows, total}, each row with model and record (the id); only records you may read are shown. Check total before importing again",
        ),
        _ => None,
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The id of the entry of one source key for one model.
pub fn entry_id(model: &str, system: &str, table: &str, source_id: &str) -> String {
    let mut h = Sha256::new();
    h.update(format!("{model}\0{system}\0{table}\0{source_id}").as_bytes());
    hex(&h.finalize())
}

/// The fields of an entry.
pub fn entry_fields(
    (model, record): (&str, &str),
    (system, table, source_id): (&str, &str, &str),
    now: u64,
    by: &str,
) -> Fields {
    let mut f = Fields::new();
    f.insert("model".into(), J::from(model));
    f.insert("record".into(), J::from(record));
    f.insert("source_system".into(), J::from(system));
    f.insert("source_table".into(), J::from(table));
    f.insert("source_id".into(), J::from(source_id));
    f.insert("imported_at".into(), J::from(instant(now)));
    f.insert("imported_by".into(), J::from(by));
    f
}

fn part(what: &str, raw: &str) -> R<String> {
    let n = raw.chars().count();
    if raw.trim().is_empty() || n > MAX_PART_CHARS || raw.chars().any(char::is_control) {
        return Err(format!(
            "{what} must be 1 to {MAX_PART_CHARS} characters without control characters"
        ));
    }
    Ok(raw.to_string())
}

/// The `source` of a `bulk_create` call: the system, the table, and one source id per row.
pub(crate) struct Source {
    pub system: String,
    table: String,
    ids: Vec<Option<String>>,
    tables: Vec<Option<String>>,
}

impl Source {
    /// The (table, id) of row `i`, or none when the row has no source id.
    pub fn key_of(&self, i: usize) -> Option<(&str, &str)> {
        let id = self.ids.get(i)?.as_deref()?;
        let table = self
            .tables
            .get(i)
            .and_then(|t| t.as_deref())
            .unwrap_or(&self.table);
        Some((table, id))
    }
}

/// Read `source` of a call with `rows` rows. `None` when it is absent.
///
/// `source` is an object: `system` and `table` (text), `ids` (a list with one entry per row, in
/// the order of the rows; text or a whole number; null leaves a row without a key) and, when the
/// rows come from several tables, `tables` (one entry per row, null meaning `table`).
pub(crate) fn parse_source(v: Option<&Value>, rows: usize) -> R<Option<Source>> {
    let m = match v {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Map(m)) => m,
        Some(_) => return Err("source must be an object: system, table and ids".into()),
    };
    for k in m.keys() {
        if !matches!(k.as_str(), "system" | "table" | "ids" | "tables") {
            return Err(format!(
                "unknown source key {k:?}: source takes system, table, ids and tables"
            ));
        }
    }
    let text = |k: &str| match m.get(k) {
        Some(Value::Text(t)) => part(&format!("source.{k}"), t),
        _ => Err(format!("source.{k} is required and must be text")),
    };
    let system = text("system")?;
    let table = text("table")?;
    let list = |k: &str, numbers: bool| -> R<Vec<Option<String>>> {
        let Some(Value::List(l)) = m.get(k) else {
            return Err(format!("source.{k} must be a list with one entry per row"));
        };
        if l.len() != rows {
            return Err(format!(
                "source.{k} has {} entries but the call has {rows} rows: give one per row (null for none)",
                l.len()
            ));
        }
        l.iter()
            .enumerate()
            .map(|(i, x)| match x {
                Value::Null => Ok(None),
                Value::Text(t) => part(&format!("source.{k}[{i}]"), t).map(Some),
                Value::Int(n) if numbers => Ok(Some(n.to_string())),
                _ => Err(format!(
                    "source.{k}[{i}] must be text{} or null",
                    if numbers { " or a whole number" } else { "" }
                )),
            })
            .collect()
    };
    let ids = list("ids", true)?;
    let tables = match m.get("tables") {
        None | Some(Value::Null) => Vec::new(),
        Some(_) => list("tables", false)?,
    };
    Ok(Some(Source {
        system,
        table,
        ids,
        tables,
    }))
}

/// Whether `actor` may read the record `rec` of `model`: the group of the model and its row
/// rules. A model that is not registered, or a record that is gone, is not readable.
fn may_read(eng: &Engine, reads: &dyn Reads, actor: &Actor, model: &str, record: &str) -> bool {
    let Some(def) = eng.def(model) else {
        return false;
    };
    if !actor.holds(def.read_group) {
        return false;
    }
    let look = crate::engine::effects::lookup_fn(eng);
    let gate = Gate::new(reads, &look, actor);
    matches!(reads.get(model, record), Ok(Some(rec)) if gate.visible(&def, &rec))
}

/// An existing mapping of a source key for `model`: the record, if the record is still there.
pub(crate) enum Mapped {
    /// No entry, or an entry whose record was deleted since.
    No { stale: bool },
    /// The record the key is mapped to, and whether the caller may read it.
    Record { id: String, readable: bool },
}

pub(crate) fn lookup(
    eng: &Engine,
    actor: &Actor,
    model: &str,
    key: (&str, &str, &str),
) -> R<Mapped> {
    let reads = Plain(eng.backend());
    let Some(e) = reads.get(MODEL, &entry_id(model, key.0, key.1, key.2))? else {
        return Ok(Mapped::No { stale: false });
    };
    let Some(record) = e.get("record").and_then(J::as_str) else {
        return Ok(Mapped::No { stale: true });
    };
    if reads.get(model, record)?.is_none() {
        return Ok(Mapped::No { stale: true });
    }
    Ok(Mapped::Record {
        id: record.to_string(),
        readable: may_read(eng, &reads, actor, model, record),
    })
}

fn text_in(m: &BTreeMap<String, Value>, k: &str) -> R<String> {
    match m.get(k) {
        Some(Value::Text(t)) if !t.trim().is_empty() => Ok(t.clone()),
        _ => Err(format!("{k} is required and must be text")),
    }
}

fn row(id: &str, rec: &Fields) -> Value {
    let mut m: BTreeMap<String, Value> = rec
        .iter()
        .map(|(k, v)| (k.clone(), Value::from_json(v)))
        .collect();
    m.insert("id".into(), Value::text(id));
    Value::Map(m)
}

fn only_keys(m: &BTreeMap<String, Value>, allowed: &[&str], what: &str) -> R<()> {
    match m
        .keys()
        .find(|k| !k.starts_with('_') && !allowed.contains(&k.as_str()))
    {
        Some(k) => Err(format!("unknown input {k:?}: {what}")),
        None => Ok(()),
    }
}

/// `import_map.search`: the entries of a source key whose record the caller may read.
fn search(eng: &Engine, input: &Value) -> R<Value> {
    let Value::Map(m) = input else {
        return Err("expected an object".into());
    };
    only_keys(
        m,
        &["source_system", "source_table", "source_id", "model"],
        "search takes source_system, source_table, source_id and model",
    )?;
    let system = text_in(m, "source_system")?;
    let table = text_in(m, "source_table")?;
    let sid = match m.get("source_id") {
        Some(Value::Int(n)) => n.to_string(),
        _ => text_in(m, "source_id")?,
    };
    let actor = Actor::from_input(input);
    let reads = Plain(eng.backend());
    let found: Vec<(String, Fields)> = match m.get("model") {
        None | Some(Value::Null) => reads
            .find(MODEL, "source_id", &J::from(sid.as_str()))?
            .into_iter()
            .filter(|(_, e)| {
                e.get("source_system").and_then(J::as_str) == Some(system.as_str())
                    && e.get("source_table").and_then(J::as_str) == Some(table.as_str())
            })
            .collect(),
        Some(Value::Text(model)) => reads
            .get(MODEL, &entry_id(model, &system, &table, &sid))?
            .map(|e| (entry_id(model, &system, &table, &sid), e))
            .into_iter()
            .collect(),
        Some(_) => return Err("model must be text".into()),
    };
    let rows: Vec<Value> = found
        .iter()
        .filter(|(_, e)| readable(eng, &reads, &actor, e))
        .map(|(id, e)| row(id, e))
        .collect();
    Ok(Value::map([
        ("total", Value::Int(rows.len() as i64)),
        ("rows", Value::List(rows)),
    ]))
}

fn readable(eng: &Engine, reads: &dyn Reads, actor: &Actor, e: &Fields) -> bool {
    match (
        e.get("model").and_then(J::as_str),
        e.get("record").and_then(J::as_str),
    ) {
        (Some(model), Some(record)) => may_read(eng, reads, actor, model, record),
        _ => false,
    }
}

/// `import_map.get`: the keys of one record, when the caller may read the record. A record with
/// no key, one that does not exist and one the caller cannot see all answer with an empty list.
fn get(eng: &Engine, input: &Value) -> R<Value> {
    let Value::Map(m) = input else {
        return Err("expected an object".into());
    };
    only_keys(m, &["model", "id"], "get takes model and id (the record)")?;
    let model = text_in(m, "model")?;
    let record = text_in(m, "id")?;
    let actor = Actor::from_input(input);
    let reads = Plain(eng.backend());
    if !may_read(eng, &reads, &actor, &model, &record) {
        return Ok(Value::List(Vec::new()));
    }
    let mut found: Vec<(String, Fields)> = reads
        .find(MODEL, "record", &J::from(record.as_str()))?
        .into_iter()
        .filter(|(_, e)| e.get("model").and_then(J::as_str) == Some(model.as_str()))
        .collect();
    found.sort_by(|a, b| {
        let at = |e: &Fields| e.get("imported_at").and_then(J::as_str).map(String::from);
        (at(&a.1), &a.0).cmp(&(at(&b.1), &b.0))
    });
    Ok(Value::List(
        found.iter().map(|(id, e)| row(id, e)).collect(),
    ))
}

impl Dispatcher {
    /// Bind `import_map.get` and `import_map.search` of the model of [`model_def`] (declared in
    /// `module`). Nothing else is bound: the entries are made by `bulk_create`.
    pub fn register_import_map(&self, module: &Module, store: &crate::Store) {
        let eng = self.engine(store);
        let e = eng.clone();
        self.register_with_actor(module, MODEL, "get", Box::new(move |input| get(&e, input)));
        self.register_with_actor(
            module,
            MODEL,
            "search",
            Box::new(move |input| search(&eng, input)),
        );
    }
}
