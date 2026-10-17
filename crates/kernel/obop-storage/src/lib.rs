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

mod atomic;
pub mod blob;

pub use atomic::{ApplyError, Op};
use atomic::{TxView, run_ops};

pub trait Backend: Send + Sync {
    fn insert(&self, model: &str, id: &str, fields: &Fields) -> Result<()>;
    fn get(&self, model: &str, id: &str) -> Result<Option<Fields>>;
    /// Records of a model in insertion order.
    fn list(&self, model: &str) -> Result<Vec<(String, Fields)>>;
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
    /// Apply several steps all-or-nothing and isolated from every other writer: the primitive
    /// for checks that must not race (the last unit of stock, a voucher's last use, a state that
    /// changes once). On `Err` nothing was changed. A backend that cannot do this refuses.
    fn apply(&self, _ops: &[Op]) -> std::result::Result<(), ApplyError> {
        Err(ApplyError::Storage(
            "this storage backend has no atomic batches".into(),
        ))
    }
    /// Short name for logs: `memory`, `sqlite` or `postgres`.
    fn kind(&self) -> &'static str;
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
    fn apply(&self, ops: &[Op]) -> std::result::Result<(), ApplyError> {
        struct M<'a>(&'a mut BTreeMap<String, Vec<(String, Fields)>>);
        impl TxView for M<'_> {
            fn tx_get(&mut self, model: &str, id: &str) -> Result<Option<Fields>> {
                Ok(self
                    .0
                    .get(model)
                    .and_then(|r| r.iter().find(|(i, _)| i == id))
                    .map(|(_, f)| f.clone()))
            }
            fn tx_insert(&mut self, model: &str, id: &str, fields: &Fields) -> Result<()> {
                self.0
                    .entry(model.into())
                    .or_default()
                    .push((id.into(), fields.clone()));
                Ok(())
            }
            fn tx_put(&mut self, model: &str, id: &str, fields: &Fields) -> Result<()> {
                if let Some((_, f)) = self
                    .0
                    .get_mut(model)
                    .and_then(|r| r.iter_mut().find(|(i, _)| i == id))
                {
                    *f = fields.clone();
                }
                Ok(())
            }
            fn tx_delete(&mut self, model: &str, id: &str) -> Result<()> {
                if let Some(r) = self.0.get_mut(model) {
                    r.retain(|(i, _)| i != id);
                }
                Ok(())
            }
        }
        let mut g = self.rows.lock().unwrap();
        let backup = g.clone();
        let r = run_ops(&mut M(&mut g), ops);
        if r.is_err() {
            *g = backup;
        }
        r
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
    fn apply(&self, ops: &[Op]) -> std::result::Result<(), ApplyError> {
        struct S<'a>(&'a rusqlite::Transaction<'a>);
        impl TxView for S<'_> {
            fn tx_get(&mut self, model: &str, id: &str) -> Result<Option<Fields>> {
                let text: Option<String> = self
                    .0
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
                text.map(|t| parse(&t)).transpose()
            }
            fn tx_insert(&mut self, model: &str, id: &str, fields: &Fields) -> Result<()> {
                self.0
                    .execute(
                        "INSERT INTO obop_record (model, id, data) VALUES (?1, ?2, ?3)",
                        (model, id, J::Object(fields.clone()).to_string()),
                    )
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
            fn tx_put(&mut self, model: &str, id: &str, fields: &Fields) -> Result<()> {
                self.0
                    .execute(
                        "UPDATE obop_record SET data = ?3 WHERE model = ?1 AND id = ?2",
                        (model, id, J::Object(fields.clone()).to_string()),
                    )
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
            fn tx_delete(&mut self, model: &str, id: &str) -> Result<()> {
                self.0
                    .execute(
                        "DELETE FROM obop_record WHERE model = ?1 AND id = ?2",
                        (model, id),
                    )
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
        }
        let mut g = self.conn.lock().unwrap();
        let tx = g
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| ApplyError::Storage(e.to_string()))?;
        let r = run_ops(&mut S(&tx), ops);
        match r {
            Ok(()) => tx.commit().map_err(|e| ApplyError::Storage(e.to_string())),
            Err(e) => {
                let _ = tx.rollback();
                Err(e)
            }
        }
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
    fn apply(&self, ops: &[Op]) -> std::result::Result<(), ApplyError> {
        struct P<'a>(postgres::Transaction<'a>);
        impl TxView for P<'_> {
            fn tx_get(&mut self, model: &str, id: &str) -> Result<Option<Fields>> {
                // Row lock: another process applying a batch to the same record waits here.
                let row = self
                    .0
                    .query_opt(
                        "SELECT data FROM obop_record WHERE model = $1 AND id = $2 FOR UPDATE",
                        &[&model, &id],
                    )
                    .map_err(|e| e.to_string())?;
                row.map(|r| pg_fields(r.get::<_, J>(0))).transpose()
            }
            fn tx_insert(&mut self, model: &str, id: &str, fields: &Fields) -> Result<()> {
                let data = J::Object(fields.clone());
                self.0
                    .execute(
                        "INSERT INTO obop_record (model, id, data) VALUES ($1, $2, $3)",
                        &[&model, &id, &data],
                    )
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
            fn tx_put(&mut self, model: &str, id: &str, fields: &Fields) -> Result<()> {
                let data = J::Object(fields.clone());
                self.0
                    .execute(
                        "UPDATE obop_record SET data = $3 WHERE model = $1 AND id = $2",
                        &[&model, &id, &data],
                    )
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
            fn tx_delete(&mut self, model: &str, id: &str) -> Result<()> {
                self.0
                    .execute(
                        "DELETE FROM obop_record WHERE model = $1 AND id = $2",
                        &[&model, &id],
                    )
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
        }
        let mut g = self.client.lock().unwrap();
        let tx = g
            .transaction()
            .map_err(|e| ApplyError::Storage(e.to_string()))?;
        let mut view = P(tx);
        let r = run_ops(&mut view, ops);
        match r {
            Ok(()) => view
                .0
                .commit()
                .map_err(|e| ApplyError::Storage(e.to_string())),
            Err(e) => {
                let _ = view.0.rollback();
                Err(e)
            }
        }
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
        atomic_contract(b, &model);
    }

    /// Atomic batches: bounds, upserts, compare-and-set and all-or-nothing.
    fn atomic_contract(b: &dyn Backend, model: &str) {
        let stock = |b: &dyn Backend| b.get(model, "s").unwrap().unwrap()["n"].as_i64().unwrap();
        b.insert(model, "s", &fields("n", J::from(3))).unwrap();
        let take = |n: i64| Op::add(model, "s", "n", -n).min(0);
        b.apply(&[take(2)]).unwrap();
        assert_eq!(stock(b), 1);
        // The second step crosses the bound: the first must not stick.
        let r = b.apply(&[Op::add(model, "s", "n", 5), take(9)]);
        assert!(
            matches!(r, Err(ApplyError::Conflict { index: 1, .. })),
            "{r:?}"
        );
        assert_eq!(stock(b), 1, "rolled back");
        // Upsert counters with an upper bound.
        let hit = || {
            Op::add(model, "u", "n", 1)
                .max(2)
                .or_create(fields("code", J::from("X")))
        };
        b.apply(&[hit()]).unwrap();
        b.apply(&[hit()]).unwrap();
        assert!(b.apply(&[hit()]).is_err());
        let u = b.get(model, "u").unwrap().unwrap();
        assert_eq!((u["n"].as_i64(), u["code"].as_str()), (Some(2), Some("X")));
        assert!(
            b.apply(&[Op::add(model, "nope", "n", 1)]).is_err(),
            "no record, no create"
        );
        // Insert is a conflict on an existing id; compare-and-set changes state once.
        let ins = Op::Insert {
            model: model.into(),
            id: "s".into(),
            fields: Fields::new(),
        };
        assert!(b.apply(&[ins]).is_err());
        let flip = |from: J| Op::Patch {
            model: model.into(),
            id: "s".into(),
            patch: fields("st", J::from("done")),
            expect: vec![("st".into(), from)],
        };
        b.apply(&[flip(J::Null)]).unwrap();
        assert!(b.apply(&[flip(J::Null)]).is_err(), "already changed");
        assert_eq!(b.get(model, "s").unwrap().unwrap()["st"], J::from("done"));
        assert_eq!(stock(b), 1, "other fields kept");
        // The last unit goes to exactly one of many racing writers.
        b.insert(model, "last", &fields("n", J::from(1))).unwrap();
        let ok = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| {
                    if b.apply(&[Op::add(model, "last", "n", -1).min(0)]).is_ok() {
                        ok.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    }
                });
            }
        });
        assert_eq!(ok.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(b.get(model, "last").unwrap().unwrap()["n"], J::from(0));
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
