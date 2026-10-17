//! Layer 1 storage: one small record contract with three backends.
//!
//! - `memory:` for tests.
//! - `sqlite://PATH` (or `sqlite::memory:`) for local development and solo use.
//! - `postgres://...` for every deployment (D6, D8, D30, D44). The database-enforced integrity
//!   guarantees of D30 exist on PostgreSQL only; SQLite is a convenience backend.
//!
//! Records are JSON documents keyed by (model, id) in one table, so no DDL is built from model
//! names. The typed-column schema manager (`obop-schema`) replaces this on PostgreSQL later.

use serde_json::Value as J;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

pub type Fields = serde_json::Map<String, J>;
pub type Result<T> = std::result::Result<T, String>;

/// One step of an atomic batch (see `Backend::apply`).
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// Add a new record. Fails when the id exists.
    Insert {
        model: String,
        id: String,
        fields: Fields,
    },
    /// Merge fields into an existing record. Fails when it does not exist.
    Merge {
        model: String,
        id: String,
        patch: Fields,
    },
    /// Remove an existing record. Fails when it does not exist.
    Delete { model: String, id: String },
}

pub trait Backend: Send + Sync {
    fn insert(&self, model: &str, id: &str, fields: &Fields) -> Result<()>;
    fn get(&self, model: &str, id: &str) -> Result<Option<Fields>>;
    /// Records of a model in insertion order.
    fn list(&self, model: &str) -> Result<Vec<(String, Fields)>>;
    /// Visit the records of a model in insertion order without collecting them, so a caller that
    /// only folds them (an aggregate) holds one record at a time. `visit` returns `Ok(false)` to
    /// stop early. It runs while the backend holds its connection or lock, so it must not call
    /// the backend. The default collects with `list`; the shipped backends stream.
    fn scan(
        &self,
        model: &str,
        visit: &mut dyn FnMut(&str, &Fields) -> Result<bool>,
    ) -> Result<()> {
        for (id, f) in self.list(model)? {
            if !visit(&id, &f)? {
                break;
            }
        }
        Ok(())
    }
    /// Merge `patch` into an existing record atomically. `false` if it does not exist.
    fn update(&self, model: &str, id: &str, patch: &Fields) -> Result<bool>;
    fn count(&self, model: &str) -> Result<usize>;
    /// Remove a record. `false` if it did not exist.
    fn delete(&self, model: &str, id: &str) -> Result<bool>;
    /// Insert several records all-or-nothing. The default inserts one by one and removes what it
    /// inserted when one fails; the shipped backends override it with a transaction.
    fn insert_many(&self, rows: &[(String, String, Fields)]) -> Result<()> {
        for (i, (model, id, fields)) in rows.iter().enumerate() {
            if let Err(e) = self.insert(model, id, fields) {
                for (m, id, _) in &rows[..i] {
                    let _ = self.delete(m, id);
                }
                return Err(e);
            }
        }
        Ok(())
    }
    /// Apply several steps all-or-nothing: every step happens or none does, and readers never see
    /// a half-applied batch. The shipped backends do it in one transaction. The default applies
    /// the steps one by one and puts back what it changed when one fails (it cannot undo a
    /// removed key, and a restored record moves to the end of the insertion order), which is
    /// good enough for a backend written for tests.
    fn apply(&self, ops: &[Op]) -> Result<()> {
        let mut undo: Vec<Op> = Vec::new();
        for op in ops {
            let step = match op {
                Op::Insert { model, id, fields } => self.insert(model, id, fields).map(|()| {
                    undo.push(Op::Delete {
                        model: model.clone(),
                        id: id.clone(),
                    })
                }),
                Op::Merge { model, id, patch } => match self.get(model, id) {
                    Ok(Some(before)) => self.update(model, id, patch).map(|_| {
                        undo.push(Op::Merge {
                            model: model.clone(),
                            id: id.clone(),
                            patch: before,
                        })
                    }),
                    Ok(None) => Err(format!("no {model} {id}")),
                    Err(e) => Err(e),
                },
                Op::Delete { model, id } => match self.get(model, id) {
                    Ok(Some(before)) => self.delete(model, id).map(|_| {
                        undo.push(Op::Insert {
                            model: model.clone(),
                            id: id.clone(),
                            fields: before,
                        })
                    }),
                    Ok(None) => Err(format!("no {model} {id}")),
                    Err(e) => Err(e),
                },
            };
            if let Err(e) = step {
                for u in undo.into_iter().rev() {
                    let _ = match u {
                        Op::Insert { model, id, fields } => self.insert(&model, &id, &fields),
                        Op::Merge { model, id, patch } => {
                            self.update(&model, &id, &patch).map(|_| ())
                        }
                        Op::Delete { model, id } => self.delete(&model, &id).map(|_| ()),
                    };
                }
                return Err(e);
            }
        }
        Ok(())
    }
    /// Records of `model` whose `field` equals `value`, in insertion order. The shipped backends
    /// answer it in the database; the default reads the model.
    fn find(&self, model: &str, field: &str, value: &J) -> Result<Vec<(String, Fields)>> {
        Ok(self
            .list(model)?
            .into_iter()
            .filter(|(_, f)| f.get(field) == Some(value))
            .collect())
    }
    /// Make the database refuse two records of `model` that have the same non-empty `fields[0]`
    /// and the same values in the other `fields` (D57). `name` identifies the constraint in
    /// errors (`obop_uq_...`). Backends without indexes do nothing: the command layer checks on
    /// every write too. Fails if existing records already break the constraint.
    fn ensure_unique(&self, _name: &str, _model: &str, _fields: &[&str]) -> Result<()> {
        Ok(())
    }
    /// Short name for logs: `memory`, `sqlite` or `postgres`.
    fn kind(&self) -> &'static str;
}

/// Names that go into DDL: lowercase letters, digits and underscore.
fn ident(s: &str) -> Result<&str> {
    if !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        Ok(s)
    } else {
        Err(format!("{s:?} is not an identifier"))
    }
}

pub fn open(url: &str) -> Result<Arc<dyn Backend>> {
    if url == "memory:" {
        Ok(Arc::new(Memory::default()))
    } else if let Some(path) = url
        .strip_prefix("sqlite://")
        .or_else(|| url.strip_prefix("sqlite:"))
    {
        Ok(Arc::new(Sqlite::open(path)?))
    } else if url.starts_with("postgres://") || url.starts_with("postgresql://") {
        Ok(Arc::new(Pg::connect(url)?))
    } else {
        Err(format!(
            "unsupported database url (use memory:, sqlite://PATH or postgres://...): {url}"
        ))
    }
}

fn parse(text: &str) -> Result<Fields> {
    match serde_json::from_str(text).map_err(|e| e.to_string())? {
        J::Object(m) => Ok(m),
        _ => Err("stored record is not an object".into()),
    }
}

fn merged(mut base: Fields, patch: &Fields) -> Fields {
    base.extend(patch.clone());
    base
}

// ---- memory -------------------------------------------------------------------------------

#[derive(Default)]
struct Memory {
    rows: Mutex<BTreeMap<String, Vec<(String, Fields)>>>,
}

impl Backend for Memory {
    fn insert(&self, model: &str, id: &str, fields: &Fields) -> Result<()> {
        let mut g = self.rows.lock().unwrap();
        let rows = g.entry(model.into()).or_default();
        if rows.iter().any(|(i, _)| i == id) {
            return Err(format!("duplicate {model} {id}"));
        }
        rows.push((id.into(), fields.clone()));
        Ok(())
    }
    fn get(&self, model: &str, id: &str) -> Result<Option<Fields>> {
        let g = self.rows.lock().unwrap();
        Ok(g.get(model)
            .and_then(|r| r.iter().find(|(i, _)| i == id))
            .map(|(_, f)| f.clone()))
    }
    fn list(&self, model: &str) -> Result<Vec<(String, Fields)>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .get(model)
            .cloned()
            .unwrap_or_default())
    }
    fn scan(
        &self,
        model: &str,
        visit: &mut dyn FnMut(&str, &Fields) -> Result<bool>,
    ) -> Result<()> {
        let g = self.rows.lock().unwrap();
        for (id, f) in g.get(model).into_iter().flatten() {
            if !visit(id, f)? {
                break;
            }
        }
        Ok(())
    }
    fn update(&self, model: &str, id: &str, patch: &Fields) -> Result<bool> {
        let mut g = self.rows.lock().unwrap();
        match g
            .get_mut(model)
            .and_then(|r| r.iter_mut().find(|(i, _)| i == id))
        {
            Some((_, f)) => {
                f.extend(patch.clone());
                Ok(true)
            }
            None => Ok(false),
        }
    }
    fn count(&self, model: &str) -> Result<usize> {
        Ok(self.rows.lock().unwrap().get(model).map_or(0, Vec::len))
    }
    fn delete(&self, model: &str, id: &str) -> Result<bool> {
        let mut g = self.rows.lock().unwrap();
        let Some(rows) = g.get_mut(model) else {
            return Ok(false);
        };
        let before = rows.len();
        rows.retain(|(i, _)| i != id);
        Ok(rows.len() < before)
    }
    fn insert_many(&self, rows: &[(String, String, Fields)]) -> Result<()> {
        let mut g = self.rows.lock().unwrap();
        for (n, (model, id, _)) in rows.iter().enumerate() {
            let dup_in_batch = rows[..n].iter().any(|(m, i, _)| m == model && i == id);
            if dup_in_batch || g.get(model).is_some_and(|r| r.iter().any(|(i, _)| i == id)) {
                return Err(format!("duplicate {model} {id}"));
            }
        }
        for (model, id, fields) in rows {
            g.entry(model.clone())
                .or_default()
                .push((id.clone(), fields.clone()));
        }
        Ok(())
    }
    fn apply(&self, ops: &[Op]) -> Result<()> {
        let mut g = self.rows.lock().unwrap();
        let mut work = g.clone();
        for op in ops {
            match op {
                Op::Insert { model, id, fields } => {
                    let rows = work.entry(model.clone()).or_default();
                    if rows.iter().any(|(i, _)| i == id) {
                        return Err(format!("duplicate {model} {id}"));
                    }
                    rows.push((id.clone(), fields.clone()));
                }
                Op::Merge { model, id, patch } => {
                    match work
                        .get_mut(model)
                        .and_then(|r| r.iter_mut().find(|(i, _)| i == id))
                    {
                        Some((_, f)) => f.extend(patch.clone()),
                        None => return Err(format!("no {model} {id}")),
                    }
                }
                Op::Delete { model, id } => {
                    let rows = work.entry(model.clone()).or_default();
                    let before = rows.len();
                    rows.retain(|(i, _)| i != id);
                    if rows.len() == before {
                        return Err(format!("no {model} {id}"));
                    }
                }
            }
        }
        *g = work;
        Ok(())
    }
    fn kind(&self) -> &'static str {
        "memory"
    }
}

// ---- sqlite -------------------------------------------------------------------------------

struct Sqlite {
    conn: Mutex<rusqlite::Connection>,
}

impl Sqlite {
    fn open(path: &str) -> Result<Self> {
        let conn = if path == ":memory:" || path.is_empty() {
            rusqlite::Connection::open_in_memory()
        } else {
            rusqlite::Connection::open(path)
        }
        .map_err(|e| e.to_string())?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS obop_record (
                 seq   INTEGER PRIMARY KEY AUTOINCREMENT,
                 model TEXT NOT NULL,
                 id    TEXT NOT NULL,
                 data  TEXT NOT NULL CHECK (json_valid(data)),
                 UNIQUE (model, id)
             );",
        )
        .map_err(|e| e.to_string())?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

impl Backend for Sqlite {
    fn insert(&self, model: &str, id: &str, fields: &Fields) -> Result<()> {
        let data = J::Object(fields.clone()).to_string();
        self.conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO obop_record (model, id, data) VALUES (?1, ?2, ?3)",
                (model, id, data),
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    fn get(&self, model: &str, id: &str) -> Result<Option<Fields>> {
        let g = self.conn.lock().unwrap();
        let mut st = g
            .prepare("SELECT data FROM obop_record WHERE model = ?1 AND id = ?2")
            .map_err(|e| e.to_string())?;
        let mut rows = st.query((model, id)).map_err(|e| e.to_string())?;
        match rows.next().map_err(|e| e.to_string())? {
            Some(r) => Ok(Some(parse(
                &r.get::<_, String>(0).map_err(|e| e.to_string())?,
            )?)),
            None => Ok(None),
        }
    }
    fn list(&self, model: &str) -> Result<Vec<(String, Fields)>> {
        let g = self.conn.lock().unwrap();
        let mut st = g
            .prepare("SELECT id, data FROM obop_record WHERE model = ?1 ORDER BY seq")
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map((model,), |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        rows.map(|r| {
            let (id, data) = r.map_err(|e| e.to_string())?;
            Ok((id, parse(&data)?))
        })
        .collect()
    }
    fn scan(
        &self,
        model: &str,
        visit: &mut dyn FnMut(&str, &Fields) -> Result<bool>,
    ) -> Result<()> {
        let g = self.conn.lock().unwrap();
        let mut st = g
            .prepare("SELECT id, data FROM obop_record WHERE model = ?1 ORDER BY seq")
            .map_err(|e| e.to_string())?;
        let mut rows = st.query((model,)).map_err(|e| e.to_string())?;
        while let Some(r) = rows.next().map_err(|e| e.to_string())? {
            let id: String = r.get(0).map_err(|e| e.to_string())?;
            let data: String = r.get(1).map_err(|e| e.to_string())?;
            if !visit(&id, &parse(&data)?)? {
                break;
            }
        }
        Ok(())
    }
    fn update(&self, model: &str, id: &str, patch: &Fields) -> Result<bool> {
        let mut g = self.conn.lock().unwrap();
        let tx = g
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let current: Option<String> = tx
            .query_row(
                "SELECT data FROM obop_record WHERE model = ?1 AND id = ?2",
                (model, id),
                |r| r.get(0),
            )
            .map(Some)
            .or_else(|e| {
                if e == rusqlite::Error::QueryReturnedNoRows {
                    Ok(None)
                } else {
                    Err(e)
                }
            })
            .map_err(|e| e.to_string())?;
        let Some(current) = current else {
            return Ok(false);
        };
        let data = J::Object(merged(parse(&current)?, patch)).to_string();
        tx.execute(
            "UPDATE obop_record SET data = ?3 WHERE model = ?1 AND id = ?2",
            (model, id, data),
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(true)
    }
    fn count(&self, model: &str) -> Result<usize> {
        self.conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM obop_record WHERE model = ?1",
                (model,),
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n as usize)
            .map_err(|e| e.to_string())
    }
    fn delete(&self, model: &str, id: &str) -> Result<bool> {
        self.conn
            .lock()
            .unwrap()
            .execute(
                "DELETE FROM obop_record WHERE model = ?1 AND id = ?2",
                (model, id),
            )
            .map(|n| n > 0)
            .map_err(|e| e.to_string())
    }
    fn insert_many(&self, rows: &[(String, String, Fields)]) -> Result<()> {
        let mut g = self.conn.lock().unwrap();
        let tx = g.transaction().map_err(|e| e.to_string())?;
        for (model, id, fields) in rows {
            tx.execute(
                "INSERT INTO obop_record (model, id, data) VALUES (?1, ?2, ?3)",
                (model, id, J::Object(fields.clone()).to_string()),
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())
    }
    fn apply(&self, ops: &[Op]) -> Result<()> {
        let mut g = self.conn.lock().unwrap();
        let tx = g
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        for op in ops {
            match op {
                Op::Insert { model, id, fields } => {
                    tx.execute(
                        "INSERT INTO obop_record (model, id, data) VALUES (?1, ?2, ?3)",
                        (model, id, J::Object(fields.clone()).to_string()),
                    )
                    .map_err(|e| e.to_string())?;
                }
                Op::Merge { model, id, patch } => {
                    let current: Option<String> = tx
                        .query_row(
                            "SELECT data FROM obop_record WHERE model = ?1 AND id = ?2",
                            (model, id),
                            |r| r.get(0),
                        )
                        .map(Some)
                        .or_else(|e| {
                            if e == rusqlite::Error::QueryReturnedNoRows {
                                Ok(None)
                            } else {
                                Err(e)
                            }
                        })
                        .map_err(|e| e.to_string())?;
                    let Some(current) = current else {
                        return Err(format!("no {model} {id}"));
                    };
                    let data = J::Object(merged(parse(&current)?, patch)).to_string();
                    tx.execute(
                        "UPDATE obop_record SET data = ?3 WHERE model = ?1 AND id = ?2",
                        (model, id, data),
                    )
                    .map_err(|e| e.to_string())?;
                }
                Op::Delete { model, id } => {
                    let n = tx
                        .execute(
                            "DELETE FROM obop_record WHERE model = ?1 AND id = ?2",
                            (model, id),
                        )
                        .map_err(|e| e.to_string())?;
                    if n == 0 {
                        return Err(format!("no {model} {id}"));
                    }
                }
            }
        }
        tx.commit().map_err(|e| e.to_string())
    }
    fn find(&self, model: &str, field: &str, value: &J) -> Result<Vec<(String, Fields)>> {
        use rusqlite::types::Value as SqlValue;
        let bound = match value {
            J::String(s) => SqlValue::Text(s.clone()),
            J::Number(n) if n.is_i64() => SqlValue::Integer(n.as_i64().unwrap_or(0)),
            J::Bool(b) => SqlValue::Integer(i64::from(*b)),
            _ => {
                return Ok(self
                    .list(model)?
                    .into_iter()
                    .filter(|(_, f)| f.get(field) == Some(value))
                    .collect());
            }
        };
        let g = self.conn.lock().unwrap();
        let mut st = g
            .prepare(
                "SELECT id, data FROM obop_record WHERE model = ?1 \
                 AND json_extract(data, '$.' || ?2) = ?3 ORDER BY seq",
            )
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map(rusqlite::params![model, field, bound], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        // A JSON string "5" and a JSON number 5 are different values; SQLite compares by type
        // affinity here, so keep only exact matches.
        let mut out = Vec::new();
        for r in rows {
            let (id, data) = r.map_err(|e| e.to_string())?;
            let f = parse(&data)?;
            if f.get(field) == Some(value) {
                out.push((id, f));
            }
        }
        Ok(out)
    }
    fn ensure_unique(&self, name: &str, model: &str, fields: &[&str]) -> Result<()> {
        let (name, model) = (ident(name)?, ident(model)?);
        let mut cols = Vec::new();
        for (i, f) in fields.iter().enumerate() {
            let f = ident(f)?;
            cols.push(if i == 0 {
                format!("NULLIF(json_extract(data, '$.{f}'), '')")
            } else {
                format!("COALESCE(json_extract(data, '$.{f}'), '')")
            });
        }
        self.conn
            .lock()
            .unwrap()
            .execute_batch(&format!(
                "CREATE UNIQUE INDEX IF NOT EXISTS {name} ON obop_record ({}) WHERE model = '{model}'",
                cols.join(", ")
            ))
            .map_err(|e| e.to_string())
    }
    fn kind(&self) -> &'static str {
        "sqlite"
    }
}

// ---- postgres -----------------------------------------------------------------------------

struct Pg {
    client: Mutex<postgres::Client>,
}

impl Pg {
    fn connect(url: &str) -> Result<Self> {
        let mut client =
            postgres::Client::connect(url, postgres::NoTls).map_err(|e| e.to_string())?;
        client
            .batch_execute(
                "CREATE TABLE IF NOT EXISTS obop_record (
                     seq   BIGINT GENERATED ALWAYS AS IDENTITY,
                     model TEXT NOT NULL,
                     id    TEXT NOT NULL,
                     data  JSONB NOT NULL CHECK (jsonb_typeof(data) = 'object'),
                     PRIMARY KEY (model, id)
                 );
                 CREATE INDEX IF NOT EXISTS obop_record_seq ON obop_record (model, seq);",
            )
            .map_err(|e| e.to_string())?;
        Ok(Self {
            client: Mutex::new(client),
        })
    }
}

fn pg_fields(v: J) -> Result<Fields> {
    match v {
        J::Object(m) => Ok(m),
        _ => Err("stored record is not an object".into()),
    }
}

impl Backend for Pg {
    fn insert(&self, model: &str, id: &str, fields: &Fields) -> Result<()> {
        let data = J::Object(fields.clone());
        self.client
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO obop_record (model, id, data) VALUES ($1, $2, $3)",
                &[&model, &id, &data],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    fn get(&self, model: &str, id: &str) -> Result<Option<Fields>> {
        let row = self
            .client
            .lock()
            .unwrap()
            .query_opt(
                "SELECT data FROM obop_record WHERE model = $1 AND id = $2",
                &[&model, &id],
            )
            .map_err(|e| e.to_string())?;
        row.map(|r| pg_fields(r.get::<_, J>(0))).transpose()
    }
    fn list(&self, model: &str) -> Result<Vec<(String, Fields)>> {
        let rows = self
            .client
            .lock()
            .unwrap()
            .query(
                "SELECT id, data FROM obop_record WHERE model = $1 ORDER BY seq",
                &[&model],
            )
            .map_err(|e| e.to_string())?;
        rows.into_iter()
            .map(|r| Ok((r.get::<_, String>(0), pg_fields(r.get::<_, J>(1))?)))
            .collect()
    }
    fn scan(
        &self,
        model: &str,
        visit: &mut dyn FnMut(&str, &Fields) -> Result<bool>,
    ) -> Result<()> {
        use postgres::fallible_iterator::FallibleIterator;
        let mut client = self.client.lock().unwrap();
        let mut rows = client
            .query_raw(
                "SELECT id, data FROM obop_record WHERE model = $1 ORDER BY seq",
                [&model as &(dyn postgres::types::ToSql + Sync)],
            )
            .map_err(|e| e.to_string())?;
        while let Some(r) = rows.next().map_err(|e| e.to_string())? {
            let id: String = r.get(0);
            if !visit(&id, &pg_fields(r.get::<_, J>(1))?)? {
                // Dropping the iterator early leaves the portal unread; the client discards it.
                break;
            }
        }
        Ok(())
    }
    fn update(&self, model: &str, id: &str, patch: &Fields) -> Result<bool> {
        let patch = J::Object(patch.clone());
        let n = self
            .client
            .lock()
            .unwrap()
            .execute(
                "UPDATE obop_record SET data = data || $3 WHERE model = $1 AND id = $2",
                &[&model, &id, &patch],
            )
            .map_err(|e| e.to_string())?;
        Ok(n > 0)
    }
    fn count(&self, model: &str) -> Result<usize> {
        let r = self
            .client
            .lock()
            .unwrap()
            .query_one(
                "SELECT COUNT(*) FROM obop_record WHERE model = $1",
                &[&model],
            )
            .map_err(|e| e.to_string())?;
        Ok(r.get::<_, i64>(0) as usize)
    }
    fn delete(&self, model: &str, id: &str) -> Result<bool> {
        let n = self
            .client
            .lock()
            .unwrap()
            .execute(
                "DELETE FROM obop_record WHERE model = $1 AND id = $2",
                &[&model, &id],
            )
            .map_err(|e| e.to_string())?;
        Ok(n > 0)
    }
    fn insert_many(&self, rows: &[(String, String, Fields)]) -> Result<()> {
        let mut g = self.client.lock().unwrap();
        let mut tx = g.transaction().map_err(|e| e.to_string())?;
        for (model, id, fields) in rows {
            let data = J::Object(fields.clone());
            tx.execute(
                "INSERT INTO obop_record (model, id, data) VALUES ($1, $2, $3)",
                &[model, id, &data],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())
    }
    fn apply(&self, ops: &[Op]) -> Result<()> {
        let mut g = self.client.lock().unwrap();
        let mut tx = g.transaction().map_err(|e| e.to_string())?;
        for op in ops {
            match op {
                Op::Insert { model, id, fields } => {
                    let data = J::Object(fields.clone());
                    tx.execute(
                        "INSERT INTO obop_record (model, id, data) VALUES ($1, $2, $3)",
                        &[model, id, &data],
                    )
                    .map_err(|e| e.to_string())?;
                }
                Op::Merge { model, id, patch } => {
                    let patch = J::Object(patch.clone());
                    let n = tx
                        .execute(
                            "UPDATE obop_record SET data = data || $3 WHERE model = $1 AND id = $2",
                            &[model, id, &patch],
                        )
                        .map_err(|e| e.to_string())?;
                    if n == 0 {
                        return Err(format!("no {model} {id}"));
                    }
                }
                Op::Delete { model, id } => {
                    let n = tx
                        .execute(
                            "DELETE FROM obop_record WHERE model = $1 AND id = $2",
                            &[model, id],
                        )
                        .map_err(|e| e.to_string())?;
                    if n == 0 {
                        return Err(format!("no {model} {id}"));
                    }
                }
            }
        }
        tx.commit().map_err(|e| e.to_string())
    }
    fn find(&self, model: &str, field: &str, value: &J) -> Result<Vec<(String, Fields)>> {
        let rows = self
            .client
            .lock()
            .unwrap()
            .query(
                "SELECT id, data FROM obop_record WHERE model = $1 AND data -> $2::text = $3 ORDER BY seq",
                &[&model, &field, value],
            )
            .map_err(|e| e.to_string())?;
        rows.into_iter()
            .map(|r| Ok((r.get::<_, String>(0), pg_fields(r.get::<_, J>(1))?)))
            .collect()
    }
    fn ensure_unique(&self, name: &str, model: &str, fields: &[&str]) -> Result<()> {
        let (name, model) = (ident(name)?, ident(model)?);
        let mut cols = Vec::new();
        for (i, f) in fields.iter().enumerate() {
            let f = ident(f)?;
            cols.push(if i == 0 {
                format!("(NULLIF(data ->> '{f}', ''))")
            } else {
                format!("(COALESCE(data ->> '{f}', ''))")
            });
        }
        self.client
            .lock()
            .unwrap()
            .batch_execute(&format!(
                "CREATE UNIQUE INDEX IF NOT EXISTS {name} ON obop_record ({}) WHERE model = '{model}'",
                cols.join(", ")
            ))
            .map_err(|e| e.to_string())
    }
    fn kind(&self) -> &'static str {
        "postgres"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(k: &str, v: J) -> Fields {
        let mut m = Fields::new();
        m.insert(k.into(), v);
        m
    }

    /// The contract every backend must meet. Also run against PostgreSQL by
    /// `OBOP_TEST_POSTGRES_URL=postgres://... cargo test -p obop-storage`.
    fn contract(b: &dyn Backend) {
        let model = format!("t{}", std::process::id());
        b.insert(&model, "a", &fields("n", J::from(1))).unwrap();
        b.insert(&model, "b", &fields("n", J::Null)).unwrap();
        assert!(
            b.insert(&model, "a", &Fields::new()).is_err(),
            "duplicate id"
        );
        assert_eq!(b.count(&model).unwrap(), 2);
        assert_eq!(b.get(&model, "a").unwrap().unwrap()["n"], J::from(1));
        assert!(b.get(&model, "zzz").unwrap().is_none());
        let ids: Vec<_> = b
            .list(&model)
            .unwrap()
            .into_iter()
            .map(|(i, _)| i)
            .collect();
        assert_eq!(ids, ["a", "b"], "insertion order");
        // A merge keeps other fields and may set a field to null.
        assert!(b.update(&model, "a", &fields("m", J::Null)).unwrap());
        let a = b.get(&model, "a").unwrap().unwrap();
        assert_eq!((a["n"].clone(), a["m"].clone()), (J::from(1), J::Null));
        assert!(!b.update(&model, "zzz", &Fields::new()).unwrap());
        assert!(b.delete(&model, "b").unwrap());
        assert!(!b.delete(&model, "b").unwrap());
        assert_eq!(b.count(&model).unwrap(), 1);
        // A batch is all or nothing.
        let row = |id: &str| (model.clone(), id.to_string(), fields("n", J::from(2)));
        b.insert_many(&[row("m1"), row("m2")]).unwrap();
        assert_eq!(b.count(&model).unwrap(), 3);
        assert!(b.insert_many(&[row("m3"), row("a")]).is_err());
        assert!(b.get(&model, "m3").unwrap().is_none(), "rolled back");
        assert_eq!(b.count(&model).unwrap(), 3);
        batches_and_lookups(b, &model);
    }

    fn batches_and_lookups(b: &dyn Backend, model: &str) {
        let row = |k: &str, v: J| fields(k, v);
        let (m, s) = (model.to_string(), |x: &str| x.to_string());
        // A batch applies every step in order, later steps seeing earlier ones.
        b.apply(&[
            Op::Insert {
                model: m.clone(),
                id: s("b1"),
                fields: row("sku", J::from("X-1")),
            },
            Op::Merge {
                model: m.clone(),
                id: s("b1"),
                patch: row("qty", J::from(5)),
            },
            Op::Merge {
                model: m.clone(),
                id: s("a"),
                patch: row("qty", J::from(7)),
            },
            Op::Delete {
                model: m.clone(),
                id: s("m2"),
            },
        ])
        .unwrap();
        let b1 = b.get(model, "b1").unwrap().unwrap();
        assert_eq!(
            (b1["sku"].clone(), b1["qty"].clone()),
            (J::from("X-1"), J::from(5))
        );
        assert!(b.get(model, "m2").unwrap().is_none());
        let before = b.count(model).unwrap();
        // One failing step undoes all the others, whatever it is.
        for bad in [
            Op::Insert {
                model: m.clone(),
                id: s("a"),
                fields: Fields::new(),
            },
            Op::Merge {
                model: m.clone(),
                id: s("ghost"),
                patch: Fields::new(),
            },
            Op::Delete {
                model: m.clone(),
                id: s("ghost"),
            },
        ] {
            let r = b.apply(&[
                Op::Insert {
                    model: m.clone(),
                    id: s("b2"),
                    fields: Fields::new(),
                },
                Op::Merge {
                    model: m.clone(),
                    id: s("a"),
                    patch: row("qty", J::from(99)),
                },
                Op::Delete {
                    model: m.clone(),
                    id: s("b1"),
                },
                bad,
            ]);
            assert!(r.is_err());
            assert!(b.get(model, "b2").unwrap().is_none(), "insert rolled back");
            assert_eq!(b.get(model, "a").unwrap().unwrap()["qty"], J::from(7));
            assert!(b.get(model, "b1").unwrap().is_some(), "delete rolled back");
            assert_eq!(b.count(model).unwrap(), before);
        }
        // Lookup by field value, typed.
        let hit = b.find(model, "sku", &J::from("X-1")).unwrap();
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].0, "b1");
        assert_eq!(b.find(model, "qty", &J::from(7)).unwrap().len(), 1);
        assert!(b.find(model, "sku", &J::from("nope")).unwrap().is_empty());
        assert!(b.find(model, "sku", &J::from(5)).unwrap().is_empty());
        // A unique index refuses a second record with the same value, but not an empty one.
        if b.kind() != "memory" {
            let idx = format!("obop_uq_t{}_sku", std::process::id());
            b.ensure_unique(&idx, model, &["sku"]).unwrap();
            let dup = b.insert(model, "dup", &row("sku", J::from("X-1")));
            assert!(dup.is_err(), "duplicate value");
            b.insert(model, "e1", &row("sku", J::from(""))).unwrap();
            b.insert(model, "e2", &row("sku", J::from(""))).unwrap();
            b.insert(model, "e3", &Fields::new()).unwrap();
            assert!(
                b.apply(&[Op::Insert {
                    model: m.clone(),
                    id: s("dup2"),
                    fields: row("sku", J::from("X-1")),
                }])
                .is_err()
            );
        }
    }

    #[test]
    fn memory_meets_contract() {
        contract(&*open("memory:").unwrap());
    }

    #[test]
    fn sqlite_meets_contract_and_persists() {
        contract(&*open("sqlite::memory:").unwrap());
        let path = std::env::temp_dir().join(format!("obop-test-{}.db", std::process::id()));
        let url = format!("sqlite://{}", path.display());
        open(&url)
            .unwrap()
            .insert("m", "x", &fields("k", J::from("v")))
            .unwrap();
        // A second open sees the record: it is on disk.
        assert_eq!(
            open(&url).unwrap().get("m", "x").unwrap().unwrap()["k"],
            J::from("v")
        );
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }

    #[test]
    fn postgres_meets_contract_when_available() {
        let Ok(url) = std::env::var("OBOP_TEST_POSTGRES_URL") else {
            eprintln!("skipped: set OBOP_TEST_POSTGRES_URL to run against PostgreSQL");
            return;
        };
        contract(&*open(&url).unwrap());
    }

    #[test]
    fn rejects_unknown_urls() {
        assert!(open("mysql://x").is_err());
    }
}
