//! The SQLite backend: local development and solo use (D44). Several processes may share one
//! file (WAL mode, a busy timeout); cross-process locks are file locks next to the database.

use super::*;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

pub(crate) struct Sqlite {
    conn: Mutex<Connection>,
    /// The database file; `None` for an in-memory database.
    path: Option<String>,
    locks: Arc<LocalLocks>,
}

const SCHEMA: &str = "PRAGMA journal_mode = WAL;
     PRAGMA foreign_keys = ON;
     CREATE TABLE IF NOT EXISTS obop_record (
         seq   INTEGER PRIMARY KEY AUTOINCREMENT,
         model TEXT NOT NULL,
         id    TEXT NOT NULL,
         data  TEXT NOT NULL CHECK (json_valid(data)),
         UNIQUE (model, id)
     );
     CREATE TABLE IF NOT EXISTS obop_counter (
         key        TEXT PRIMARY KEY,
         n          INTEGER NOT NULL,
         expires_ms INTEGER NOT NULL
     );
     CREATE TABLE IF NOT EXISTS obop_kv (
         key        TEXT PRIMARY KEY,
         value      TEXT NOT NULL,
         expires_ms INTEGER NOT NULL
     );
     CREATE INDEX IF NOT EXISTS obop_kv_expiry ON obop_kv (expires_ms);";

fn e(err: rusqlite::Error) -> String {
    err.to_string()
}

impl Sqlite {
    pub(crate) fn open(path: &str) -> Result<Self> {
        let memory = path == ":memory:" || path.is_empty();
        let conn = if memory {
            Connection::open_in_memory()
        } else {
            Connection::open(path)
        }
        .map_err(e)?;
        conn.busy_timeout(std::time::Duration::from_secs(30))
            .map_err(e)?;
        // Two processes opening a fresh file at once may both try to switch the journal mode or
        // create tables; the busy timeout covers the wait, and a retry covers the rare race.
        let mut last = Ok(());
        for _ in 0..5 {
            last = conn.execute_batch(SCHEMA).map_err(e);
            if last.is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        last?;
        Ok(Self {
            conn: Mutex::new(conn),
            path: (!memory).then(|| path.to_string()),
            locks: Arc::default(),
        })
    }
}

struct FileGuard {
    file: std::fs::File,
    _local: LocalGuard,
}

impl Drop for FileGuard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

fn expiry(ttl_ms: Option<u64>) -> i64 {
    ttl_ms.map_or(0, |t| (now_ms() + t.max(1)) as i64)
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
            .map_err(e)
    }
    fn get(&self, model: &str, id: &str) -> Result<Option<Fields>> {
        let g = self.conn.lock().unwrap();
        let mut st = g
            .prepare("SELECT data FROM obop_record WHERE model = ?1 AND id = ?2")
            .map_err(e)?;
        let mut rows = st.query((model, id)).map_err(e)?;
        match rows.next().map_err(e)? {
            Some(r) => Ok(Some(parse(&r.get::<_, String>(0).map_err(e)?)?)),
            None => Ok(None),
        }
    }
    fn list(&self, model: &str) -> Result<Vec<(String, Fields)>> {
        let g = self.conn.lock().unwrap();
        let mut st = g
            .prepare("SELECT id, data FROM obop_record WHERE model = ?1 ORDER BY seq")
            .map_err(e)?;
        let rows = st
            .query_map((model,), |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(e)?;
        rows.map(|r| {
            let (id, data) = r.map_err(e)?;
            Ok((id, parse(&data)?))
        })
        .collect()
    }
    fn update(&self, model: &str, id: &str, patch: &Fields) -> Result<bool> {
        let mut g = self.conn.lock().unwrap();
        let tx = g
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(e)?;
        let current: Option<String> = tx
            .query_row(
                "SELECT data FROM obop_record WHERE model = ?1 AND id = ?2",
                (model, id),
                |r| r.get(0),
            )
            .optional()
            .map_err(e)?;
        let Some(current) = current else {
            return Ok(false);
        };
        let data = J::Object(merged(parse(&current)?, patch)).to_string();
        tx.execute(
            "UPDATE obop_record SET data = ?3 WHERE model = ?1 AND id = ?2",
            (model, id, data),
        )
        .map_err(e)?;
        tx.commit().map_err(e)?;
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
            .map_err(e)
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
            .map_err(e)
    }
    fn insert_many(&self, rows: &[(String, String, Fields)]) -> Result<()> {
        let mut g = self.conn.lock().unwrap();
        let tx = g
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(e)?;
        for (model, id, fields) in rows {
            tx.execute(
                "INSERT INTO obop_record (model, id, data) VALUES (?1, ?2, ?3)",
                (model, id, J::Object(fields.clone()).to_string()),
            )
            .map_err(e)?;
        }
        tx.commit().map_err(e)
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
    fn apply(&self, ops: &[Op]) -> Result<()> {
        self.apply_checked(ops).map_err(|e| e.to_string())
    }
    fn apply_checked(&self, ops: &[Op]) -> std::result::Result<(), ApplyError> {
        self.apply_tracked(ctx::system_ctx(), ops, Tracking::empty())
    }
    fn apply_tracked(
        &self,
        ctx: &WriteCtx,
        ops: &[Op],
        tracking: &Tracking,
    ) -> std::result::Result<(), ApplyError> {
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
        let r = run_ops(&mut S(&tx), ops, ctx, tracking);
        match r {
            Ok(()) => tx.commit().map_err(|e| ApplyError::Storage(e.to_string())),
            Err(e) => {
                let _ = tx.rollback();
                Err(e)
            }
        }
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

    fn lock(&self, key: &str) -> Result<LockGuard> {
        let local = self.locks.take(key)?;
        let Some(path) = &self.path else {
            return Ok(Box::new(local));
        };
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(format!("{path}.lock-{}", lock_file_part(key)))
            .map_err(|x| format!("cannot open the lock file: {x}"))?;
        let deadline = std::time::Instant::now() + LOCK_WAIT;
        loop {
            match file.try_lock() {
                Ok(()) => {
                    return Ok(Box::new(FileGuard {
                        file,
                        _local: local,
                    }));
                }
                Err(std::fs::TryLockError::WouldBlock) => {
                    if std::time::Instant::now() >= deadline {
                        return Err(format!("timed out waiting for lock {key}"));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(std::fs::TryLockError::Error(x)) => {
                    return Err(format!("cannot lock {key}: {x}"));
                }
            }
        }
    }

    fn counter_add(&self, key: &str, ttl_ms: u64, extend: bool) -> Result<u64> {
        let now = now_ms() as i64;
        let exp = if ttl_ms == 0 { 0 } else { now + ttl_ms as i64 };
        self.conn
            .lock()
            .unwrap()
            .query_row(
                "INSERT INTO obop_counter (key, n, expires_ms) VALUES (?1, 1, ?2)
                 ON CONFLICT (key) DO UPDATE SET
                     n = CASE WHEN obop_counter.expires_ms <> 0 AND obop_counter.expires_ms <= ?3
                              THEN 1 ELSE obop_counter.n + 1 END,
                     expires_ms = CASE
                         WHEN obop_counter.expires_ms <> 0 AND obop_counter.expires_ms <= ?3
                              THEN excluded.expires_ms
                         WHEN ?4 THEN excluded.expires_ms
                         ELSE obop_counter.expires_ms END
                 RETURNING n",
                (key, exp, now, extend),
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n as u64)
            .map_err(e)
    }
    fn counter_get(&self, key: &str) -> Result<u64> {
        let now = now_ms() as i64;
        self.conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT n FROM obop_counter WHERE key = ?1 AND (expires_ms = 0 OR expires_ms > ?2)",
                (key, now),
                |r| r.get::<_, i64>(0),
            )
            .optional()
            .map(|n| n.unwrap_or(0) as u64)
            .map_err(e)
    }
    fn counter_ttl_ms(&self, key: &str) -> Result<Option<u64>> {
        let now = now_ms() as i64;
        self.conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT expires_ms FROM obop_counter WHERE key = ?1 AND expires_ms > ?2",
                (key, now),
                |r| r.get::<_, i64>(0),
            )
            .optional()
            .map(|x| x.map(|x| (x - now) as u64))
            .map_err(e)
    }
    fn counter_reset(&self, key: &str) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM obop_counter WHERE key = ?1", (key,))
            .map(|_| ())
            .map_err(e)
    }

    fn kv_put(&self, key: &str, value: &str, ttl_ms: Option<u64>) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO obop_kv (key, value, expires_ms) VALUES (?1, ?2, ?3)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value,
                                                 expires_ms = excluded.expires_ms",
                (key, value, expiry(ttl_ms)),
            )
            .map(|_| ())
            .map_err(e)
    }
    fn kv_put_new(&self, key: &str, value: &str, ttl_ms: Option<u64>) -> Result<bool> {
        let now = now_ms() as i64;
        self.conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO obop_kv (key, value, expires_ms) VALUES (?1, ?2, ?3)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value,
                                                 expires_ms = excluded.expires_ms
                 WHERE obop_kv.expires_ms <> 0 AND obop_kv.expires_ms <= ?4",
                (key, value, expiry(ttl_ms), now),
            )
            .map(|n| n > 0)
            .map_err(e)
    }
    fn kv_get(&self, key: &str) -> Result<Option<String>> {
        let now = now_ms() as i64;
        self.conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT value FROM obop_kv WHERE key = ?1 AND (expires_ms = 0 OR expires_ms > ?2)",
                (key, now),
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(e)
    }
    fn kv_delete(&self, key: &str) -> Result<bool> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM obop_kv WHERE key = ?1", (key,))
            .map(|n| n > 0)
            .map_err(e)
    }
    fn kv_scan(&self, prefix: &str) -> Result<Vec<(String, String)>> {
        let now = now_ms() as i64;
        let g = self.conn.lock().unwrap();
        let mut st = g
            .prepare(
                "SELECT key, value FROM obop_kv
                 WHERE substr(key, 1, length(?1)) = ?1 AND (expires_ms = 0 OR expires_ms > ?2)
                 ORDER BY key",
            )
            .map_err(e)?;
        let rows = st
            .query_map((prefix, now), |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(e)?;
        rows.map(|r| r.map_err(e)).collect()
    }
    fn purge_expired(&self) -> Result<usize> {
        let now = now_ms() as i64;
        let g = self.conn.lock().unwrap();
        let a = g
            .execute(
                "DELETE FROM obop_kv WHERE expires_ms <> 0 AND expires_ms <= ?1",
                (now,),
            )
            .map_err(e)?;
        let b = g
            .execute(
                "DELETE FROM obop_counter WHERE expires_ms <> 0 AND expires_ms <= ?1",
                (now,),
            )
            .map_err(e)?;
        Ok(a + b)
    }

    fn export(&self, batch: usize, sink: &mut RowSink<'_>) -> Result<()> {
        // A file database is read on a connection of its own, inside one read transaction (WAL
        // gives it a stable snapshot) so writers are not held up; an in-memory one uses the
        // shared connection.
        match &self.path {
            Some(p) => {
                let mut own = Connection::open(p).map_err(e)?;
                own.busy_timeout(std::time::Duration::from_secs(30))
                    .map_err(e)?;
                export_from(&mut own, batch, sink)
            }
            None => export_from(&mut self.conn.lock().unwrap(), batch, sink),
        }
    }
    fn import(
        &self,
        wipe: bool,
        next: &mut dyn FnMut() -> Result<Option<Vec<Row>>>,
    ) -> Result<usize> {
        let mut g = self.conn.lock().unwrap();
        let tx = g
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(e)?;
        if wipe {
            for t in ["obop_record", "obop_kv", "obop_counter"] {
                tx.execute(&format!("DELETE FROM {t}"), ()).map_err(e)?;
            }
        }
        let mut n = 0;
        while let Some(batch) = next()? {
            for (model, id, fields) in batch {
                tx.execute(
                    "INSERT INTO obop_record (model, id, data) VALUES (?1, ?2, ?3)",
                    (&model, &id, J::Object(fields).to_string()),
                )
                .map_err(e)?;
                n += 1;
            }
        }
        tx.commit().map_err(e)?;
        Ok(n)
    }

    fn swap_fields(&self, swaps: &[Swap]) -> Result<usize> {
        let mut g = self.conn.lock().unwrap();
        let tx = g
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(e)?;
        let mut n = 0;
        for s in swaps {
            let current: Option<String> = tx
                .query_row(
                    "SELECT data FROM obop_record WHERE model = ?1 AND id = ?2",
                    (&s.model, &s.id),
                    |r| r.get(0),
                )
                .optional()
                .map_err(e)?;
            let Some(current) = current else { continue };
            let mut f = parse(&current)?;
            if f.get(&s.field).and_then(J::as_str) != Some(s.old.as_str()) {
                continue;
            }
            f.insert(s.field.clone(), J::from(s.new.clone()));
            tx.execute(
                "UPDATE obop_record SET data = ?3 WHERE model = ?1 AND id = ?2",
                (&s.model, &s.id, J::Object(f).to_string()),
            )
            .map_err(e)?;
            n += 1;
        }
        tx.commit().map_err(e)?;
        Ok(n)
    }
    fn kv_swap(&self, key: &str, old: &str, new: &str) -> Result<bool> {
        let now = now_ms() as i64;
        self.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE obop_kv SET value = ?3
                 WHERE key = ?1 AND value = ?2 AND (expires_ms = 0 OR expires_ms > ?4)",
                (key, old, new, now),
            )
            .map(|n| n > 0)
            .map_err(e)
    }
    fn total(&self) -> Result<usize> {
        self.conn
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM obop_record", (), |r| {
                r.get::<_, i64>(0)
            })
            .map(|n| n as usize)
            .map_err(e)
    }
}

fn export_from(conn: &mut Connection, batch: usize, sink: &mut RowSink<'_>) -> Result<()> {
    let tx = conn.transaction().map_err(e)?;
    let mut after = 0i64;
    loop {
        let rows: Vec<(i64, String, String, String)> = {
            let mut st = tx
                .prepare(
                    "SELECT seq, model, id, data FROM obop_record WHERE seq > ?1
                     ORDER BY seq LIMIT ?2",
                )
                .map_err(e)?;
            let it = st
                .query_map((after, batch.max(1) as i64), |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                })
                .map_err(e)?;
            it.collect::<std::result::Result<_, _>>().map_err(e)?
        };
        let Some(last) = rows.last() else { break };
        after = last.0;
        let out: Vec<(String, String, Fields)> = rows
            .into_iter()
            .map(|(_, m, i, d)| Ok((m, i, parse(&d)?)))
            .collect::<Result<_>>()?;
        sink(&out)?;
    }
    Ok(())
}
