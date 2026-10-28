//! The PostgreSQL backend: every deployment (D44). Many server processes may share one
//! database. Statements run on connections from a pool (D74, `pool.rs` and `pgpool.rs`): no
//! process-wide lock, a long read holds one connection and nothing else; cross-process locks are
//! session advisory locks on connections of a pool of their own.

use super::*;
use crate::pg_hist;
use crate::pgpool::{
    Class, MAX_REPEATS, PR, PgManager, PgPool, PgPoolConfig, Pooler, Session, describe,
    is_retryable_text, repeat_pause, tune,
};
use crate::pool::{Pool, Pooled};
use crate::tls::PgConnector;
use postgres::Client;
use std::cell::RefCell;
use std::sync::Weak;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub(crate) struct Pg {
    /// Tells this backend's lock connections from another backend's on the same thread (tests
    /// open two backends on one database to stand in for two server processes).
    id: u64,
    pub(crate) main: PgPool,
    /// Connections that hold advisory locks (a lock lives as long as its session).
    pub(crate) lock_pool: Pool<PgManager>,
    locks: Arc<LocalLocks>,
    /// Atomic batches of this process wait here before they lease a connection: a batch that
    /// names many records (a bulk write) holds almost every stripe, so the others would each park
    /// a pooled connection at the advisory locks and the pool, then reads too, would run dry
    /// (D74). Large batches take it exclusively, small ones share it.
    batches: std::sync::RwLock<()>,
    /// What this process knows of the change history (D75): whether it is installed, the months
    /// it has made partitions for, the clock tests set.
    pub(crate) hist: pg_hist::State,
}

/// A batch that locks more stripes than this is a bulk write.
const BULK_STRIPES: usize = 32;

/// The database's clock in milliseconds, so processes with different clocks agree on expiry.
const NOW: &str = "(extract(epoch from clock_timestamp()) * 1000)::bigint";

/// Rows fetched from the server at a time by a streaming scan.
const SCAN_CHUNK: i32 = 500;

fn e(err: postgres::Error) -> String {
    describe(&err)
}

/// What identifies the server a connection reached: the database and when its server started.
fn identity(c: &mut Client) -> Result<String> {
    // The simple protocol: one round trip, no named statement that a pooler in transaction mode
    // could send to another server connection.
    let rows = c
        .simple_query(
            "SELECT current_database()::text || ' @ ' || pg_postmaster_start_time()::text",
        )
        .map_err(e)?;
    rows.iter()
        .find_map(|m| match m {
            postgres::SimpleQueryMessage::Row(r) => r.get(0).map(str::to_string),
            _ => None,
        })
        .ok_or_else(|| "the database did not say which database it is".to_string())
}

impl Pg {
    /// As `connect_options` with the schema created when missing: what the tests of the pool
    /// connect with.
    #[cfg(test)]
    pub(crate) fn connect_with(url: &str, cfg: PgPoolConfig) -> Result<Self> {
        Self::connect_options(url, Some(cfg), Ddl::IfMissing)
    }

    /// As `connect_with`, with stamps that read `clock` instead of the database's (the statements
    /// that lock a record and read the clock still run).
    #[cfg(test)]
    pub(crate) fn with_clock(mut self, clock: crate::sysfields::Clock) -> Self {
        self.hist = self.hist.with_clock(clock);
        self
    }

    /// Connect with the pool settings given (`None`: read `OBOP_DB_*`) and say whether the schema
    /// may be created.
    pub(crate) fn connect_options(url: &str, cfg: Option<PgPoolConfig>, ddl: Ddl) -> Result<Self> {
        let cfg = match cfg {
            Some(c) => c,
            None => PgPoolConfig::from_env()?,
        };
        static IDS: AtomicU64 = AtomicU64::new(1);
        // Behind a pooler in transaction mode the locks need a direct address of their own
        // (D74); an address that is the pooler's own would take them through it.
        if cfg.pooler == Pooler::Transaction {
            match cfg.lock_url.as_deref() {
                None => return Err(PgPoolConfig::NEEDS_LOCK_URL.into()),
                Some(l) if l == url.trim() => {
                    return Err(
                        "OBOP_LOCK_DATABASE_URL is the same address as the database URL; \
                                with OBOP_DB_POOLER=transaction it must be a direct address of \
                                the database, not the pooler's"
                            .into(),
                    );
                }
                Some(_) => {}
            }
        }
        let cfg = Arc::new(cfg);
        let connector = PgConnector::new(url)?;
        let mut client = tune(connector.clone(), "obop-main", &cfg).connect()?;
        // Several processes may start at once; schema creation is serialised so they do not
        // trip over each other creating the same objects. The schema is the parts of `schema`:
        // what is missing is created under `Ddl::IfMissing`, and `Ddl::Never` only checks.
        //
        // The history is not made at start (`schema::parts` is the record store only): a
        // database installed before it opens as it did, whatever the role. It is made when a
        // model that keeps history first writes; where it cannot be made, that write is refused
        // (see `pg_hist`). Whether it is there is asked in the same transaction: behind a pooler
        // in transaction mode a statement that is prepared outside one may be run on another
        // server connection (`start_tests` watches the wire).
        let history_ready = schema::with_lock(&mut client, |c| {
            schema::ensure(c, ddl)?;
            (pg_hist::PART.check)(c)
        })?;

        // Advisory locks need a session that lasts: behind a pooler in transaction mode they go
        // to a direct address. That address must reach the same server and database.
        let lock_pool = match &cfg.lock_url {
            None => Pool::new(
                PgManager::new(tune(connector.clone(), "obop-lock", &cfg), None),
                cfg.settings("lock", cfg.lock_size),
            ),
            Some(lock_url) => {
                let lock_connector = tune(
                    PgConnector::new(lock_url)
                        .map_err(|e| format!("OBOP_LOCK_DATABASE_URL: {e}"))?,
                    "obop-lock",
                    &cfg,
                );
                let mut direct = lock_connector
                    .connect()
                    .map_err(|e| format!("OBOP_LOCK_DATABASE_URL: {e}"))?;
                if identity(&mut client)? != identity(&mut direct)? {
                    return Err(
                        "OBOP_LOCK_DATABASE_URL reaches another database or server than \
                                the database URL; advisory locks only exclude each other when \
                                both addresses reach the same database"
                            .into(),
                    );
                }
                let pool = Pool::new(
                    PgManager::new(lock_connector, None),
                    cfg.settings("lock", cfg.lock_size),
                );
                pool.seed(Session::new(direct));
                pool
            }
        };
        let main = PgPool::new("main", connector, cfg.size, &cfg);
        main.seed(client);
        Ok(Self {
            id: IDS.fetch_add(1, Ordering::Relaxed),
            main,
            lock_pool,
            locks: Arc::default(),
            batches: std::sync::RwLock::new(()),
            hist: pg_hist::State::new(ddl, history_ready),
        })
    }

    /// The session that holds this thread's locks: a lock taken while the thread already holds
    /// another (the agent's vault lock, then the write lock, then an MCP session lock) goes on the
    /// same connection, so a thread that holds a lock never waits for a second connection and
    /// the small lock pool cannot deadlock on callers that each hold one and want another.
    fn lock_session(&self) -> Result<Arc<LockSession>> {
        let mine = SESSIONS.with(|v| {
            let mut v = v.borrow_mut();
            v.retain(|(_, w)| w.strong_count() > 0);
            v.iter()
                .filter(|(id, _)| *id == self.id)
                .filter_map(|(_, w)| w.upgrade())
                .find(|s| !s.poisoned.load(Ordering::Relaxed))
        });
        if let Some(s) = mine {
            return Ok(s);
        }
        let pooled = self.lock_pool.get(false).map_err(|e| e.message())?;
        let s = Arc::new(LockSession {
            conn: Mutex::new(pooled),
            poisoned: AtomicBool::new(false),
        });
        SESSIONS.with(|v| v.borrow_mut().push((self.id, Arc::downgrade(&s))));
        Ok(s)
    }
}

thread_local! {
    static SESSIONS: RefCell<Vec<(u64, Weak<LockSession>)>> = const { RefCell::new(Vec::new()) };
}

/// A lock connection, shared by the locks one thread holds. It goes back to the pool when the
/// last of them is released, unless a lock call failed on it: then the session is closed, which
/// makes the server release whatever it still held.
struct LockSession {
    conn: Mutex<Pooled<PgManager>>,
    poisoned: AtomicBool,
}

impl LockSession {
    fn ask(&self, sql: &str, key: &str) -> Result<bool> {
        let mut g = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        match g.client.query_one(sql, &[&key]) {
            Ok(r) => Ok(r.get(0)),
            Err(err) => {
                self.poisoned.store(true, Ordering::Relaxed);
                Err(describe(&err))
            }
        }
    }
}

impl Drop for LockSession {
    fn drop(&mut self) {
        if self.poisoned.load(Ordering::Relaxed) {
            self.conn
                .get_mut()
                .unwrap_or_else(|e| e.into_inner())
                .discard();
        }
    }
}

/// The first integer of the two-integer advisory keys that order batches. The one-integer keys
/// of `Backend::lock` are a separate key space, so the two never meet.
const BATCH_LOCK_NAMESPACE: i32 = 0x6f62;

/// The record an op works on.
fn op_target(op: &Op) -> (&str, &str) {
    match op {
        Op::Insert { model, id, .. }
        | Op::Put { model, id, .. }
        | Op::Merge { model, id, .. }
        | Op::Delete { model, id }
        | Op::Add { model, id, .. }
        | Op::Patch { model, id, .. }
        | Op::EraseHistory { model, id } => (model, id),
    }
}

/// How many advisory locks one batch takes at most. The records a batch names are hashed onto this
/// many stripes, and the batch locks the distinct stripes in ascending order. A lock held by a
/// transaction takes a slot of the server's shared lock table (`max_locks_per_transaction` times
/// `max_connections`, 6,400 with the defaults); one lock per record would let a handful of
/// 1,000-row batches that run together (a bulk import) fill it up, and the server then refuses
/// every statement that needs a lock with "out of shared memory". With the stripes a batch holds
/// at most this many (and a few records, a few locks), whatever its size.
///
/// Two batches whose records fall on one stripe queue behind each other, which costs a moment and
/// is never wrong; the ordering that keeps batches from deadlocking is the same total order as
/// before (ascending stripe). A batch of a thousand records lands on nearly every stripe, so while
/// it runs the other batches of the database wait for it: a bulk write is one short transaction.
pub(crate) const BATCH_LOCK_STRIPES: u32 = 128;

/// The stripe of a record: a 32-bit FNV-1a hash of its identity, modulo the number of stripes.
fn batch_key((model, id): (&str, &str)) -> i32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in model.bytes().chain(std::iter::once(0xff)).chain(id.bytes()) {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    (h % BATCH_LOCK_STRIPES) as i32
}

/// The advisory lock keys of a batch: the distinct stripes of its records, ascending, at most
/// [`BATCH_LOCK_STRIPES`] of them.
pub(crate) fn batch_lock_keys(ops: &[Op]) -> Vec<i32> {
    let mut keys: Vec<i32> = ops.iter().map(|o| batch_key(op_target(o))).collect();
    keys.sort_unstable();
    keys.dedup();
    keys
}

#[cfg(test)]
thread_local! {
    /// The advisory locks the server held for the last batch this thread applied (tests only).
    pub(crate) static BATCH_LOCKS_HELD: std::cell::Cell<i64> = const { std::cell::Cell::new(-1) };
}

/// Take the transaction-level advisory locks of a batch (see [`batch_lock_keys`]) in one
/// statement, ascending. They are released when the transaction ends.
pub(crate) fn lock_batch(c: &mut Client, keys: &[i32]) -> PR<()> {
    if !keys.is_empty() {
        c.execute(
            "SELECT pg_advisory_xact_lock($1, k)
             FROM (SELECT k FROM unnest($2::int4[]) AS t(k) ORDER BY k) s",
            &[&BATCH_LOCK_NAMESPACE, &keys],
        )?;
    }
    Ok(())
}

pub(crate) fn pg_fields(v: J) -> Result<Fields> {
    match v {
        J::Object(m) => Ok(m),
        _ => Err("stored record is not an object".into()),
    }
}

struct PgGuard {
    session: Arc<LockSession>,
    key: String,
    _local: LocalGuard,
}

impl Drop for PgGuard {
    fn drop(&mut self) {
        // A session that could not unlock is closed (see `LockSession`).
        let _ = self.session.ask(
            "SELECT pg_advisory_unlock(hashtextextended($1, 0))",
            &self.key,
        );
    }
}

fn ttl(ttl_ms: Option<u64>) -> i64 {
    ttl_ms.map_or(0, |t| t.max(1) as i64)
}

fn rows_with_id(rows: Vec<postgres::Row>) -> PR<Vec<(String, Fields)>> {
    rows.into_iter()
        .map(|r| Ok((r.get::<_, String>(0), pg_fields(r.get::<_, J>(1))?)))
        .collect()
}

/// The columns of the unique index of a D57 rule, one per field, in the order of `fields`. The one
/// place both the index and the lookup that must use it take them from, so they cannot drift.
fn unique_columns(fields: &[&str]) -> Result<Vec<String>> {
    if fields.is_empty() {
        return Err("a unique rule needs at least one field".to_string());
    }
    fields
        .iter()
        .enumerate()
        .map(|(i, f)| crate::keyexpr::unique_column(i, f))
        .collect()
}

/// The statement that makes the unique index of `fields` on `model`. Its text is what deployed
/// databases were given by every version since D57 (the test `unique_index_text_is_what_deployed_databases_hold`).
pub(crate) fn unique_index_sql(name: &str, model: &str, fields: &[&str]) -> Result<String> {
    let model = ident(model)?;
    Ok(format!(
        "CREATE UNIQUE INDEX IF NOT EXISTS {name} ON obop_record ({}) WHERE model = '{model}'",
        unique_columns(fields)?.join(", ")
    ))
}

/// The statement of [`Backend::find_unique`]: the same columns, each compared with a bound text,
/// and the same `model = '...'` as the partial index, so the planner can use the index. The model
/// is a validated literal, never a parameter: a parameter would not prove the index's predicate.
pub(crate) fn unique_lookup_sql(model: &str, fields: &[&str]) -> Result<String> {
    let model = ident(model)?;
    let tests: Vec<String> = unique_columns(fields)?
        .iter()
        .enumerate()
        .map(|(i, column)| format!("{column} = ${}", i + 1))
        .collect();
    Ok(format!(
        "SELECT id, data FROM obop_record WHERE model = '{model}' AND {} ORDER BY seq",
        tests.join(" AND ")
    ))
}

/// The text the index column holds for each value, as the bound texts of the lookup; `None` when a
/// value has no exact text (a fraction, a list, an object, a NUL, or an empty first value, which
/// the index treats as no value at all), and the caller then reads with `find` instead.
fn unique_texts(values: &[J]) -> Option<Vec<String>> {
    if matches!(values.first(), Some(J::String(s)) if s.is_empty()) {
        return None;
    }
    values
        .iter()
        .map(|v| match v {
            J::String(s) if !s.contains('\0') => Some(s.clone()),
            J::Number(n) if n.is_i64() || n.is_u64() => Some(n.to_string()),
            J::Bool(b) => Some(b.to_string()),
            _ => None,
        })
        .collect()
}

impl Backend for Pg {
    fn insert(&self, model: &str, id: &str, fields: &Fields) -> Result<()> {
        let data = J::Object(fields.clone());
        self.main.run(Class::Point, |c| {
            c.execute(
                "INSERT INTO obop_record (model, id, data) VALUES ($1, $2, $3)",
                &[&model, &id, &data],
            )?;
            Ok(())
        })
    }
    fn get(&self, model: &str, id: &str) -> Result<Option<Fields>> {
        self.main.read(Class::Point, |c| {
            let row = c.query_opt(
                "SELECT data FROM obop_record WHERE model = $1 AND id = $2",
                &[&model, &id],
            )?;
            Ok(row.map(|r| pg_fields(r.get::<_, J>(0))).transpose()?)
        })
    }
    fn list(&self, model: &str) -> Result<Vec<(String, Fields)>> {
        self.main.read(Class::List, |c| {
            let rows = c.query(
                "SELECT id, data FROM obop_record WHERE model = $1 ORDER BY seq",
                &[&model],
            )?;
            rows_with_id(rows)
        })
    }
    fn update(&self, model: &str, id: &str, patch: &Fields) -> Result<bool> {
        let patch = J::Object(patch.clone());
        self.main.run(Class::Point, |c| {
            let n = c.execute(
                "UPDATE obop_record SET data = data || $3 WHERE model = $1 AND id = $2",
                &[&model, &id, &patch],
            )?;
            Ok(n > 0)
        })
    }
    fn count(&self, model: &str) -> Result<usize> {
        self.main.read(Class::List, |c| {
            let r = c.query_one(
                "SELECT COUNT(*) FROM obop_record WHERE model = $1",
                &[&model],
            )?;
            Ok(r.get::<_, i64>(0) as usize)
        })
    }
    fn delete(&self, model: &str, id: &str) -> Result<bool> {
        self.main.run(Class::Point, |c| {
            let n = c.execute(
                "DELETE FROM obop_record WHERE model = $1 AND id = $2",
                &[&model, &id],
            )?;
            Ok(n > 0)
        })
    }
    fn insert_many(&self, rows: &[(String, String, Fields)]) -> Result<()> {
        self.main.tx_retry(Class::Batch, |c| {
            for (model, id, fields) in rows {
                let data = J::Object(fields.clone());
                c.execute(
                    "INSERT INTO obop_record (model, id, data) VALUES ($1, $2, $3)",
                    &[model, id, &data],
                )?;
            }
            Ok(())
        })
    }
    fn scan(
        &self,
        model: &str,
        visit: &mut dyn FnMut(&str, &Fields) -> Result<bool>,
    ) -> Result<()> {
        // One repeatable-read, read-only transaction on one pooled connection: the whole scan
        // sees one snapshot, writers are not blocked, and other requests use other connections.
        // Rows come in chunks through a portal, so memory is bounded by a chunk and the time
        // limit applies to each fetch, not to however long `visit` takes.
        self.main.snapshot(Class::Aggregate, |tx| {
            let stmt =
                tx.prepare("SELECT id, data FROM obop_record WHERE model = $1 ORDER BY seq")?;
            let portal = tx.bind(&stmt, &[&model])?;
            loop {
                let rows = tx.query_portal(&portal, SCAN_CHUNK)?;
                let n = rows.len();
                for r in rows {
                    let id: String = r.get(0);
                    if !visit(&id, &pg_fields(r.get::<_, J>(1))?)? {
                        return Ok(());
                    }
                }
                if n < SCAN_CHUNK as usize {
                    return Ok(());
                }
            }
        })
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
        struct P<'a> {
            c: &'a mut Client,
            h: pg_hist::Tx<'a>,
        }
        impl TxView for P<'_> {
            fn tx_get(&mut self, model: &str, id: &str) -> Result<Option<Fields>> {
                // Row lock: another process applying a batch to the same record waits here.
                let row = self
                    .c
                    .query_opt(
                        "SELECT data FROM obop_record WHERE model = $1 AND id = $2 FOR UPDATE",
                        &[&model, &id],
                    )
                    .map_err(e)?;
                row.map(|r| pg_fields(r.get::<_, J>(0))).transpose()
            }
            fn tx_insert(&mut self, model: &str, id: &str, fields: &Fields) -> Result<()> {
                let data = J::Object(fields.clone());
                self.c
                    .execute(
                        "INSERT INTO obop_record (model, id, data) VALUES ($1, $2, $3)",
                        &[&model, &id, &data],
                    )
                    .map(|_| ())
                    .map_err(e)
            }
            fn tx_put(&mut self, model: &str, id: &str, fields: &Fields) -> Result<()> {
                let data = J::Object(fields.clone());
                self.c
                    .execute(
                        "UPDATE obop_record SET data = $3 WHERE model = $1 AND id = $2",
                        &[&model, &id, &data],
                    )
                    .map(|_| ())
                    .map_err(e)
            }
            fn tx_delete(&mut self, model: &str, id: &str) -> Result<()> {
                self.c
                    .execute(
                        "DELETE FROM obop_record WHERE model = $1 AND id = $2",
                        &[&model, &id],
                    )
                    .map(|_| ())
                    .map_err(e)
            }
            fn tx_context(&mut self, json: &str) -> Result<()> {
                // Local to this transaction: gone at COMMIT or ROLLBACK, so a pooled connection
                // never carries one write's tickets to the next.
                self.c
                    .query_one("SELECT set_config('obop.ctx', $1, true)", &[&json])
                    .map(|_| ())
                    .map_err(e)
            }
            fn tx_get_clock(&mut self, model: &str, id: &str) -> Result<Option<Fields>> {
                self.h.get_clock(self.c, model, id)
            }
            fn tx_now(&mut self) -> Result<i64> {
                self.h.now(self.c)
            }
            fn tx_history(&mut self, rows: &[ChangeRow]) -> Result<()> {
                self.h.history(self.c, rows)
            }
            fn tx_scrub(
                &mut self,
                model: &str,
                id: &str,
                keep: &[String],
                actor: &str,
                via: &Fields,
                row_version: i64,
                at: i64,
            ) -> Result<u64> {
                self.h
                    .scrub(self.c, model, id, keep, actor, via, row_version, at)
            }
        }
        if pg_hist::State::wants(ops, tracking) {
            self.hist.before_batch(&self.main);
        }
        // Two batches that touch the same records must not hold them in opposite order (they
        // would deadlock), and two that create the same record must not both think it is
        // missing: every batch first takes a transaction-level advisory lock for each stripe its
        // records fall on (at most `BATCH_LOCK_STRIPES`, whatever the size of the batch), in one
        // statement and in ascending order. Nothing else takes these locks, so overlapping
        // batches simply queue; should the server still find a deadlock (a plain write or a bulk
        // statement of another process), the batch was rolled back whole and is run again.
        let keys = batch_lock_keys(ops);
        // Wait for a turn without a connection (see `Pg::batches`). Neither guard is held across
        // anything that waits for another batch of this process.
        let _turn = if keys.len() > BULK_STRIPES {
            (
                None,
                Some(self.batches.write().unwrap_or_else(|e| e.into_inner())),
            )
        } else {
            (
                Some(self.batches.read().unwrap_or_else(|e| e.into_inner())),
                None,
            )
        };
        let mut attempt = 0;
        loop {
            // A refused batch is a value, not an error of the connection: roll back, report it.
            let outcome = self.main.tx_decide(Class::Batch, false, |c| {
                lock_batch(c, &keys)?;
                // For the tests: what the server really holds for this batch, asked inside the
                // real path, so a change that goes back to one lock per record cannot hide.
                #[cfg(test)]
                {
                    let n: i64 = c
                        .query_one(
                            "SELECT count(*) FROM pg_locks WHERE locktype = 'advisory' \
                             AND pid = pg_backend_pid()",
                            &[],
                        )?
                        .get(0);
                    BATCH_LOCKS_HELD.with(|h| h.set(n));
                }
                let r = run_ops(
                    &mut P {
                        c,
                        h: pg_hist::Tx::new(&self.hist),
                    },
                    ops,
                    ctx,
                    tracking,
                );
                let commit = r.is_ok();
                Ok((r, commit))
            });
            match outcome {
                Ok(Err(ApplyError::Storage(m)))
                    if attempt < MAX_REPEATS && is_retryable_text(&m) => {}
                Err(m) if attempt < MAX_REPEATS && is_retryable_text(&m) => {}
                Ok(r) => return r,
                Err(m) => return Err(ApplyError::Storage(m)),
            }
            attempt += 1;
            repeat_pause(attempt);
        }
    }
    fn history(&self, q: &HistoryQuery) -> Result<Vec<ChangeRow>> {
        pg_hist::history(&self.main, q)
    }
    fn changes(&self, q: &ChangeQuery) -> Result<ChangePage> {
        pg_hist::changes(self, q)
    }
    fn ensure_history_schema(&self) -> Result<()> {
        self.hist.ensure_schema(&self.main)
    }
    fn history_stats(&self) -> Result<HistoryStats> {
        pg_hist::stats(self)
    }
    fn history_ensure_partition(&self, month: &str) -> Result<PartitionEnsured> {
        pg_hist::ensure_partition(self, month)
    }
    fn history_drop_partition(&self, month: &str, min_age_days: u32) -> Result<u64> {
        pg_hist::drop_partition(self, month, min_age_days)
    }
    fn backfill_sysfields(
        &self,
        models: &[String],
        fallback_at: &str,
        batch: usize,
    ) -> Result<Backfilled> {
        pg_hist::backfill(&self.main, models, fallback_at, batch)
    }
    fn find(&self, model: &str, field: &str, value: &J) -> Result<Vec<(String, Fields)>> {
        self.main.read(Class::List, |c| {
            let rows = c.query(
                "SELECT id, data FROM obop_record WHERE model = $1 AND data -> $2::text = $3 ORDER BY seq",
                &[&model, &field, value],
            )?;
            rows_with_id(rows)
        })
    }
    fn find_unique(
        &self,
        model: &str,
        fields: &[&str],
        values: &[J],
    ) -> Result<Vec<(String, Fields)>> {
        unique_head(fields, values)?;
        // The lookup is the index text applied to the bound texts of the values. A value that has
        // no exact text (a float, a list), or a name the index cannot carry, goes the way of the
        // default: read by `find` and filtered, the same answer.
        let keyed = match (unique_lookup_sql(model, fields), unique_texts(values)) {
            (Ok(sql), Some(texts)) => Some((sql, texts)),
            _ => None,
        };
        let Some((sql, texts)) = keyed else {
            let (field, value) = unique_head(fields, values)?;
            return Ok(self
                .find(model, field, value)?
                .into_iter()
                .filter(|(_, rec)| unique_matches(rec, fields, values))
                .collect());
        };
        let rows = self.main.read(Class::List, |c| {
            let params: Vec<&(dyn postgres::types::ToSql + Sync)> =
                texts.iter().map(|t| t as _).collect();
            rows_with_id(c.query(&sql, &params)?)
        })?;
        Ok(rows
            .into_iter()
            .filter(|(_, rec)| unique_matches(rec, fields, values))
            .collect())
    }
    fn ensure_unique(&self, name: &str, model: &str, fields: &[&str]) -> Result<()> {
        let sql = unique_index_sql(ident(name)?, model, fields)?;
        self.main.run(Class::Batch, |c| {
            c.batch_execute(&sql)?;
            Ok(())
        })
    }
    fn kind(&self) -> &'static str {
        "postgres"
    }

    fn lock(&self, key: &str) -> Result<LockGuard> {
        let local = self.locks.take(key)?;
        let session = self.lock_session()?;
        let deadline = std::time::Instant::now() + LOCK_WAIT;
        loop {
            if session.ask("SELECT pg_try_advisory_lock(hashtextextended($1, 0))", key)? {
                return Ok(Box::new(PgGuard {
                    session,
                    key: key.to_string(),
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
        self.main.run(Class::Point, |c| {
            let r = c.query_one(&sql, &[&key, &ttl_ms, &extend])?;
            Ok(r.get::<_, i64>(0) as u64)
        })
    }
    fn counter_get(&self, key: &str) -> Result<u64> {
        let sql = format!(
            "SELECT n FROM obop_counter WHERE key = $1 AND (expires_ms = 0 OR expires_ms > {NOW})"
        );
        self.main.read(Class::Point, |c| {
            let r = c.query_opt(&sql, &[&key])?;
            Ok(r.map_or(0, |r| r.get::<_, i64>(0) as u64))
        })
    }
    fn counter_ttl_ms(&self, key: &str) -> Result<Option<u64>> {
        let sql = format!(
            "SELECT expires_ms - {NOW} FROM obop_counter WHERE key = $1 AND expires_ms > {NOW}"
        );
        self.main.read(Class::Point, |c| {
            let r = c.query_opt(&sql, &[&key])?;
            Ok(r.map(|r| r.get::<_, i64>(0).max(0) as u64))
        })
    }
    fn counter_reset(&self, key: &str) -> Result<()> {
        self.main.run(Class::Point, |c| {
            c.execute("DELETE FROM obop_counter WHERE key = $1", &[&key])?;
            Ok(())
        })
    }

    fn kv_put(&self, key: &str, value: &str, ttl_ms: Option<u64>) -> Result<()> {
        let sql = format!(
            "INSERT INTO obop_kv (key, value, expires_ms)
             VALUES ($1, $2, CASE WHEN $3::bigint = 0 THEN 0 ELSE {NOW} + $3::bigint END)
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value,
                                             expires_ms = EXCLUDED.expires_ms"
        );
        self.main.run(Class::Point, |c| {
            c.execute(&sql, &[&key, &value, &ttl(ttl_ms)])?;
            Ok(())
        })
    }
    fn kv_put_new(&self, key: &str, value: &str, ttl_ms: Option<u64>) -> Result<bool> {
        let sql = format!(
            "INSERT INTO obop_kv (key, value, expires_ms)
             VALUES ($1, $2, CASE WHEN $3::bigint = 0 THEN 0 ELSE {NOW} + $3::bigint END)
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value,
                                             expires_ms = EXCLUDED.expires_ms
             WHERE obop_kv.expires_ms <> 0 AND obop_kv.expires_ms <= {NOW}"
        );
        self.main.run(Class::Point, |c| {
            Ok(c.execute(&sql, &[&key, &value, &ttl(ttl_ms)])? > 0)
        })
    }
    fn kv_get(&self, key: &str) -> Result<Option<String>> {
        let sql = format!(
            "SELECT value FROM obop_kv WHERE key = $1 AND (expires_ms = 0 OR expires_ms > {NOW})"
        );
        self.main.read(Class::Point, |c| {
            let r = c.query_opt(&sql, &[&key])?;
            Ok(r.map(|r| r.get::<_, String>(0)))
        })
    }
    fn kv_delete(&self, key: &str) -> Result<bool> {
        self.main.run(Class::Point, |c| {
            Ok(c.execute("DELETE FROM obop_kv WHERE key = $1", &[&key])? > 0)
        })
    }
    fn kv_scan(&self, prefix: &str) -> Result<Vec<(String, String)>> {
        let sql = format!(
            "SELECT key, value FROM obop_kv
             WHERE left(key, length($1::text)) = $1::text AND (expires_ms = 0 OR expires_ms > {NOW})
             ORDER BY key"
        );
        self.main.read(Class::List, |c| {
            let rows = c.query(&sql, &[&prefix])?;
            Ok(rows
                .into_iter()
                .map(|r| (r.get::<_, String>(0), r.get::<_, String>(1)))
                .collect())
        })
    }
    fn purge_expired(&self) -> Result<usize> {
        let kv = format!("DELETE FROM obop_kv WHERE expires_ms <> 0 AND expires_ms <= {NOW}");
        let counters =
            format!("DELETE FROM obop_counter WHERE expires_ms <> 0 AND expires_ms <= {NOW}");
        self.main.run(Class::Batch, |c| {
            let a = c.execute(&kv, &[])?;
            let b = c.execute(&counters, &[])?;
            Ok((a + b) as usize)
        })
    }

    fn export(&self, batch: usize, sink: &mut RowSink<'_>) -> Result<()> {
        // One pooled connection, one repeatable-read read-only transaction: a consistent
        // snapshot that does not block writers and is paged by `seq`, so memory stays bounded.
        // The change history follows the records, from the same snapshot.
        let limit = batch.max(1) as i64;
        self.main.snapshot(Class::Batch, |tx| {
            let mut after = 0i64;
            loop {
                let rows = tx.query(
                    "SELECT seq, model, id, data FROM obop_record WHERE seq > $1
                     ORDER BY seq LIMIT $2",
                    &[&after, &limit],
                )?;
                let Some(last) = rows.last() else {
                    break;
                };
                after = last.try_get::<_, i64>(0)?;
                let mut out: Vec<(String, String, Fields)> = Vec::with_capacity(rows.len());
                for r in &rows {
                    out.push((
                        r.try_get::<_, String>(1)?,
                        r.try_get::<_, String>(2)?,
                        pg_fields(r.try_get::<_, J>(3)?)?,
                    ));
                }
                sink(&out)?;
            }
            pg_hist::export(tx, limit, sink)
        })
    }
    fn import(
        &self,
        wipe: bool,
        next: &mut dyn FnMut() -> Result<Option<Vec<Row>>>,
    ) -> Result<usize> {
        // A restore is a long write: it takes a long-read permit, so a few of them at once can
        // never take every connection.
        self.main.tx_decide(Class::Batch, true, |c| {
            // The entries of the change history come in among the records (D75): the pipeline
            // is preflight, wipe, rows (records and entries), then the partitions, the sequence
            // and the epoch of the feed (`pg_hist::Import`).
            let mut history = pg_hist::Import::preflight(c, wipe, self.hist.ddl())?;
            if wipe {
                for t in ["obop_record", "obop_kv", "obop_counter"] {
                    c.batch_execute(&format!("DELETE FROM {t}"))?;
                }
                history.wipe(c)?;
            }
            let mut n = 0;
            while let Some(batch) = next()? {
                for (model, id, fields) in batch {
                    if history.take(&model, &id, &fields)? {
                        n += 1;
                        continue;
                    }
                    let data = J::Object(fields);
                    c.execute(
                        "INSERT INTO obop_record (model, id, data) VALUES ($1, $2, $3)",
                        &[&model, &id, &data],
                    )?;
                    n += 1;
                }
                history.flush(c)?;
            }
            history.finish(c)?;
            Ok((n, true))
        })
    }

    fn swap_fields(&self, swaps: &[Swap]) -> Result<usize> {
        self.main.tx_retry(Class::Batch, |c| {
            let mut n = 0;
            for s in swaps {
                let new = J::from(s.new.clone());
                let hit = c.execute(
                    "UPDATE obop_record SET data = jsonb_set(data, ARRAY[$4::text], $5::jsonb)
                     WHERE model = $1 AND id = $2 AND data->>$4 = $3",
                    &[&s.model, &s.id, &s.old, &s.field, &new],
                )?;
                n += hit as usize;
            }
            Ok(n)
        })
    }
    fn kv_swap(&self, key: &str, old: &str, new: &str) -> Result<bool> {
        let sql = format!(
            "UPDATE obop_kv SET value = $3
             WHERE key = $1 AND value = $2 AND (expires_ms = 0 OR expires_ms > {NOW})"
        );
        self.main.run(Class::Point, |c| {
            Ok(c.execute(&sql, &[&key, &old, &new])? > 0)
        })
    }
    fn total(&self) -> Result<usize> {
        self.main.read(Class::Aggregate, |c| {
            let r = c.query_one("SELECT COUNT(*) FROM obop_record", &[])?;
            Ok(r.get::<_, i64>(0) as usize + pg_hist::count(c)?)
        })
    }
}
