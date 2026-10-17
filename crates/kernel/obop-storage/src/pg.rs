//! The PostgreSQL backend: every deployment (D44). Many server processes may share one
//! database; cross-process locks are session advisory locks on connections of their own.

use super::*;
use postgres::{Client, NoTls};

pub(crate) struct Pg {
    url: String,
    client: Mutex<Client>,
    /// Idle connections kept for advisory locks (a lock is held on its own session).
    lock_pool: Arc<Mutex<Vec<Client>>>,
    locks: Arc<LocalLocks>,
}

/// The database's clock in milliseconds, so processes with different clocks agree on expiry.
const NOW: &str = "(extract(epoch from clock_timestamp()) * 1000)::bigint";

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS obop_record (
         seq   BIGINT GENERATED ALWAYS AS IDENTITY,
         model TEXT NOT NULL,
         id    TEXT NOT NULL,
         data  JSONB NOT NULL CHECK (jsonb_typeof(data) = 'object'),
         PRIMARY KEY (model, id)
     );
     CREATE INDEX IF NOT EXISTS obop_record_seq ON obop_record (model, seq);
     CREATE INDEX IF NOT EXISTS obop_record_seq_all ON obop_record (seq);
     CREATE TABLE IF NOT EXISTS obop_counter (
         key        TEXT PRIMARY KEY,
         n          BIGINT NOT NULL,
         expires_ms BIGINT NOT NULL
     );
     CREATE TABLE IF NOT EXISTS obop_kv (
         key        TEXT PRIMARY KEY,
         value      TEXT NOT NULL,
         expires_ms BIGINT NOT NULL
     );
     CREATE INDEX IF NOT EXISTS obop_kv_expiry ON obop_kv (expires_ms);";

fn e(err: postgres::Error) -> String {
    err.to_string()
}

impl Pg {
    pub(crate) fn connect(url: &str) -> Result<Self> {
        let mut client = Client::connect(url, NoTls).map_err(e)?;
        // Several processes may start at once; schema creation is serialised so they do not
        // trip over each other creating the same objects.
        let mut tx = client.transaction().map_err(e)?;
        tx.batch_execute("SELECT pg_advisory_xact_lock(hashtextextended('obop-schema', 0))")
            .map_err(e)?;
        tx.batch_execute(SCHEMA).map_err(e)?;
        tx.commit().map_err(e)?;
        Ok(Self {
            url: url.to_string(),
            client: Mutex::new(client),
            lock_pool: Arc::default(),
            locks: Arc::default(),
        })
    }
}

fn pg_fields(v: J) -> Result<Fields> {
    match v {
        J::Object(m) => Ok(m),
        _ => Err("stored record is not an object".into()),
    }
}

struct PgGuard {
    client: Option<Client>,
    key: String,
    pool: Arc<Mutex<Vec<Client>>>,
    _local: LocalGuard,
}

impl Drop for PgGuard {
    fn drop(&mut self) {
        if let Some(mut c) = self.client.take() {
            let ok = c
                .query_one(
                    "SELECT pg_advisory_unlock(hashtextextended($1, 0))",
                    &[&self.key],
                )
                .is_ok();
            // A session that could not unlock is closed: the server then releases the lock.
            if ok {
                let mut pool = self.pool.lock().unwrap_or_else(|e| e.into_inner());
                if pool.len() < 8 {
                    pool.push(c);
                }
            }
        }
    }
}

fn ttl(ttl_ms: Option<u64>) -> i64 {
    ttl_ms.map_or(0, |t| t.max(1) as i64)
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
            .map_err(e)
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
            .map_err(e)?;
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
            .map_err(e)?;
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
            .map_err(e)?;
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
            .map_err(e)?;
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
            .map_err(e)?;
        Ok(n > 0)
    }
    fn insert_many(&self, rows: &[(String, String, Fields)]) -> Result<()> {
        let mut g = self.client.lock().unwrap();
        let mut tx = g.transaction().map_err(e)?;
        for (model, id, fields) in rows {
            let data = J::Object(fields.clone());
            tx.execute(
                "INSERT INTO obop_record (model, id, data) VALUES ($1, $2, $3)",
                &[model, id, &data],
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
    fn apply(&self, ops: &[Op]) -> Result<()> {
        self.apply_checked(ops).map_err(|e| e.to_string())
    }
    fn apply_checked(&self, ops: &[Op]) -> std::result::Result<(), ApplyError> {
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

    fn lock(&self, key: &str) -> Result<LockGuard> {
        let local = self.locks.take(key)?;
        let pooled = self
            .lock_pool
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop();
        let mut client = match pooled {
            Some(c) => c,
            None => Client::connect(&self.url, NoTls).map_err(e)?,
        };
        let deadline = std::time::Instant::now() + LOCK_WAIT;
        loop {
            let got: bool = client
                .query_one(
                    "SELECT pg_try_advisory_lock(hashtextextended($1, 0))",
                    &[&key],
                )
                .map_err(e)?
                .get(0);
            if got {
                return Ok(Box::new(PgGuard {
                    client: Some(client),
                    key: key.to_string(),
                    pool: self.lock_pool.clone(),
                    _local: local,
                }));
            }
            if std::time::Instant::now() >= deadline {
                return Err(format!("timed out waiting for lock {key}"));
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    fn counter_add(&self, key: &str, ttl_ms: u64, extend: bool) -> Result<u64> {
        let sql = format!(
            "INSERT INTO obop_counter (key, n, expires_ms)
             VALUES ($1, 1, CASE WHEN $2::bigint = 0 THEN 0 ELSE {NOW} + $2::bigint END)
             ON CONFLICT (key) DO UPDATE SET
                 n = CASE WHEN obop_counter.expires_ms <> 0 AND obop_counter.expires_ms <= {NOW}
                          THEN 1 ELSE obop_counter.n + 1 END,
                 expires_ms = CASE
                     WHEN obop_counter.expires_ms <> 0 AND obop_counter.expires_ms <= {NOW}
                          THEN EXCLUDED.expires_ms
                     WHEN $3::boolean THEN EXCLUDED.expires_ms
                     ELSE obop_counter.expires_ms END
             RETURNING n"
        );
        let ttl_ms = ttl_ms as i64;
        let r = self
            .client
            .lock()
            .unwrap()
            .query_one(&sql, &[&key, &ttl_ms, &extend])
            .map_err(e)?;
        Ok(r.get::<_, i64>(0) as u64)
    }
    fn counter_get(&self, key: &str) -> Result<u64> {
        let sql = format!(
            "SELECT n FROM obop_counter WHERE key = $1 AND (expires_ms = 0 OR expires_ms > {NOW})"
        );
        let r = self
            .client
            .lock()
            .unwrap()
            .query_opt(&sql, &[&key])
            .map_err(e)?;
        Ok(r.map_or(0, |r| r.get::<_, i64>(0) as u64))
    }
    fn counter_ttl_ms(&self, key: &str) -> Result<Option<u64>> {
        let sql = format!(
            "SELECT expires_ms - {NOW} FROM obop_counter WHERE key = $1 AND expires_ms > {NOW}"
        );
        let r = self
            .client
            .lock()
            .unwrap()
            .query_opt(&sql, &[&key])
            .map_err(e)?;
        Ok(r.map(|r| r.get::<_, i64>(0).max(0) as u64))
    }
    fn counter_reset(&self, key: &str) -> Result<()> {
        self.client
            .lock()
            .unwrap()
            .execute("DELETE FROM obop_counter WHERE key = $1", &[&key])
            .map(|_| ())
            .map_err(e)
    }

    fn kv_put(&self, key: &str, value: &str, ttl_ms: Option<u64>) -> Result<()> {
        let sql = format!(
            "INSERT INTO obop_kv (key, value, expires_ms)
             VALUES ($1, $2, CASE WHEN $3::bigint = 0 THEN 0 ELSE {NOW} + $3::bigint END)
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value,
                                             expires_ms = EXCLUDED.expires_ms"
        );
        self.client
            .lock()
            .unwrap()
            .execute(&sql, &[&key, &value, &ttl(ttl_ms)])
            .map(|_| ())
            .map_err(e)
    }
    fn kv_put_new(&self, key: &str, value: &str, ttl_ms: Option<u64>) -> Result<bool> {
        let sql = format!(
            "INSERT INTO obop_kv (key, value, expires_ms)
             VALUES ($1, $2, CASE WHEN $3::bigint = 0 THEN 0 ELSE {NOW} + $3::bigint END)
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value,
                                             expires_ms = EXCLUDED.expires_ms
             WHERE obop_kv.expires_ms <> 0 AND obop_kv.expires_ms <= {NOW}"
        );
        self.client
            .lock()
            .unwrap()
            .execute(&sql, &[&key, &value, &ttl(ttl_ms)])
            .map(|n| n > 0)
            .map_err(e)
    }
    fn kv_get(&self, key: &str) -> Result<Option<String>> {
        let sql = format!(
            "SELECT value FROM obop_kv WHERE key = $1 AND (expires_ms = 0 OR expires_ms > {NOW})"
        );
        let r = self
            .client
            .lock()
            .unwrap()
            .query_opt(&sql, &[&key])
            .map_err(e)?;
        Ok(r.map(|r| r.get::<_, String>(0)))
    }
    fn kv_delete(&self, key: &str) -> Result<bool> {
        self.client
            .lock()
            .unwrap()
            .execute("DELETE FROM obop_kv WHERE key = $1", &[&key])
            .map(|n| n > 0)
            .map_err(e)
    }
    fn kv_scan(&self, prefix: &str) -> Result<Vec<(String, String)>> {
        let sql = format!(
            "SELECT key, value FROM obop_kv
             WHERE left(key, length($1::text)) = $1::text AND (expires_ms = 0 OR expires_ms > {NOW})
             ORDER BY key"
        );
        let rows = self
            .client
            .lock()
            .unwrap()
            .query(&sql, &[&prefix])
            .map_err(e)?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get::<_, String>(0), r.get::<_, String>(1)))
            .collect())
    }
    fn purge_expired(&self) -> Result<usize> {
        let mut g = self.client.lock().unwrap();
        let a = g
            .execute(
                &format!("DELETE FROM obop_kv WHERE expires_ms <> 0 AND expires_ms <= {NOW}"),
                &[],
            )
            .map_err(e)?;
        let b = g
            .execute(
                &format!("DELETE FROM obop_counter WHERE expires_ms <> 0 AND expires_ms <= {NOW}"),
                &[],
            )
            .map_err(e)?;
        Ok((a + b) as usize)
    }

    fn export(&self, batch: usize, sink: &mut RowSink<'_>) -> Result<()> {
        // A connection of its own, one repeatable-read read-only transaction: a consistent
        // snapshot that does not block writers and is paged by `seq`, so memory stays bounded.
        let mut client = Client::connect(&self.url, NoTls).map_err(e)?;
        let mut tx = client
            .build_transaction()
            .isolation_level(postgres::IsolationLevel::RepeatableRead)
            .read_only(true)
            .start()
            .map_err(e)?;
        let mut after = 0i64;
        let limit = batch.max(1) as i64;
        loop {
            let rows = tx
                .query(
                    "SELECT seq, model, id, data FROM obop_record WHERE seq > $1
                     ORDER BY seq LIMIT $2",
                    &[&after, &limit],
                )
                .map_err(e)?;
            let Some(last) = rows.last() else { break };
            after = last.get::<_, i64>(0);
            let out: Vec<(String, String, Fields)> = rows
                .into_iter()
                .map(|r| {
                    Ok((
                        r.get::<_, String>(1),
                        r.get::<_, String>(2),
                        pg_fields(r.get::<_, J>(3))?,
                    ))
                })
                .collect::<Result<_>>()?;
            sink(&out)?;
        }
        tx.commit().map_err(e)
    }
    fn import(
        &self,
        wipe: bool,
        next: &mut dyn FnMut() -> Result<Option<Vec<Row>>>,
    ) -> Result<usize> {
        let mut g = self.client.lock().unwrap();
        let mut tx = g.transaction().map_err(e)?;
        if wipe {
            for t in ["obop_record", "obop_kv", "obop_counter"] {
                tx.batch_execute(&format!("DELETE FROM {t}")).map_err(e)?;
            }
        }
        let mut n = 0;
        while let Some(batch) = next()? {
            for (model, id, fields) in batch {
                let data = J::Object(fields);
                tx.execute(
                    "INSERT INTO obop_record (model, id, data) VALUES ($1, $2, $3)",
                    &[&model, &id, &data],
                )
                .map_err(e)?;
                n += 1;
            }
        }
        tx.commit().map_err(e)?;
        Ok(n)
    }

    fn swap_fields(&self, swaps: &[Swap]) -> Result<usize> {
        let mut g = self.client.lock().unwrap();
        let mut tx = g.transaction().map_err(e)?;
        let mut n = 0;
        for s in swaps {
            let new = J::from(s.new.clone());
            let hit = tx
                .execute(
                    "UPDATE obop_record SET data = jsonb_set(data, ARRAY[$4::text], $5::jsonb)
                     WHERE model = $1 AND id = $2 AND data->>$4 = $3",
                    &[&s.model, &s.id, &s.old, &s.field, &new],
                )
                .map_err(e)?;
            n += hit as usize;
        }
        tx.commit().map_err(e)?;
        Ok(n)
    }
    fn kv_swap(&self, key: &str, old: &str, new: &str) -> Result<bool> {
        let sql = format!(
            "UPDATE obop_kv SET value = $3
             WHERE key = $1 AND value = $2 AND (expires_ms = 0 OR expires_ms > {NOW})"
        );
        self.client
            .lock()
            .unwrap()
            .execute(&sql, &[&key, &old, &new])
            .map(|n| n > 0)
            .map_err(e)
    }
    fn total(&self) -> Result<usize> {
        let r = self
            .client
            .lock()
            .unwrap()
            .query_one("SELECT COUNT(*) FROM obop_record", &[])
            .map_err(e)?;
        Ok(r.get::<_, i64>(0) as usize)
    }
}
