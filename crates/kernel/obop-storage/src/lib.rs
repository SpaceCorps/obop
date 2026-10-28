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

/// The model of the security log (written by `obop-commands::audit`): an entry in it is not
/// counted as a change of the request that wrote it (see `fault`).
pub(crate) const LOG_MODEL: &str = "security_event";
pub type Result<T> = std::result::Result<T, String>;

mod atomic;
pub mod blob;

pub use atomic::ApplyError;
use atomic::{TxView, run_ops};

/// One step of an atomic batch (see `Backend::apply`).
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// Add a new record. Fails when the id exists.
    Insert {
        model: String,
        id: String,
        fields: Fields,
    },
    /// Create the record, or replace it entirely when it exists (migrations, version stamps).
    Put {
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
    /// Add `delta` to the integer `field` (absent or null counts as 0) and refuse the whole batch
    /// if the result leaves `[min, max]` or overflows. With `create`, a missing record is
    /// inserted with those fields first (an upsert counter); without it a missing record is a
    /// conflict. The check and the change happen under one lock or row lock, so two callers
    /// cannot both take the last unit.
    Add {
        model: String,
        id: String,
        field: String,
        delta: i64,
        min: Option<i64>,
        max: Option<i64>,
        create: Option<Fields>,
    },
    /// Merge `patch` into a record only if every `expect` field currently equals the given value
    /// (compare-and-set, for state changes that must happen once). A missing field equals null.
    Patch {
        model: String,
        id: String,
        patch: Fields,
        expect: Vec<(String, J)>,
    },
    /// Erase the values from the change history of a record, in the same transaction and after
    /// every other step has written its entries (D75): the entries stay, with `{"t":"erased"}`
    /// where a value was, except for the fields the model's tracking keeps. Needs `apply_ctx` or
    /// `apply_checked`. Not for a pseudo-model.
    EraseHistory { model: String, id: String },
}

impl Op {
    /// The model of the record this step works on.
    pub fn model(&self) -> &str {
        match self {
            Op::Insert { model, .. }
            | Op::Put { model, .. }
            | Op::Merge { model, .. }
            | Op::Delete { model, .. }
            | Op::Add { model, .. }
            | Op::Patch { model, .. }
            | Op::EraseHistory { model, .. } => model,
        }
    }

    pub fn add(model: &str, id: &str, field: &str, delta: i64) -> Op {
        Op::Add {
            model: model.into(),
            id: id.into(),
            field: field.into(),
            delta,
            min: None,
            max: None,
            create: None,
        }
    }

    pub fn min(mut self, v: i64) -> Op {
        if let Op::Add { min, .. } = &mut self {
            *min = Some(v);
        }
        self
    }

    pub fn max(mut self, v: i64) -> Op {
        if let Op::Add { max, .. } = &mut self {
            *max = Some(v);
        }
        self
    }

    pub fn or_create(mut self, fields: Fields) -> Op {
        if let Op::Add { create, .. } = &mut self {
            *create = Some(fields);
        }
        self
    }
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
                Op::Add { .. } | Op::Patch { .. } | Op::Put { .. } => {
                    Err("counters, puts and compare-and-set need `apply_checked`".to_string())
                }
                Op::EraseHistory { .. } => {
                    Err("erasing history needs `apply_checked` or `apply_ctx`".to_string())
                }
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
                        Op::Add { .. }
                        | Op::Patch { .. }
                        | Op::Put { .. }
                        | Op::EraseHistory { .. } => Ok(()),
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
    /// Like [`Backend::apply`] but says whether a step was refused (`Conflict`, nothing changed:
    /// a bound crossed, a missing record, an expected value that differs) or storage failed. The
    /// shipped backends implement `apply` through it, isolated from every other writer (a lock in
    /// memory, `BEGIN IMMEDIATE` in SQLite, row locks in PostgreSQL). A backend that cannot do it
    /// refuses.
    fn apply_checked(&self, _ops: &[Op]) -> std::result::Result<(), ApplyError> {
        Err(ApplyError::Storage(
            "this storage backend has no atomic batches".into(),
        ))
    }
    /// Like [`Backend::apply_checked`], for a write that knows who makes it and with which tickets
    /// (D75, D77). Only [`Tracked`] implements it, by handing the context and its own list of
    /// tracked models to the backend underneath (`apply_tracked`); every other backend leaves
    /// this default, which drops the context, and a wrapper that sits above a `Tracked` must
    /// forward it (the lint `tracked_forwards_every_backend_method` and its siblings check that).
    fn apply_ctx(&self, ctx: &WriteCtx, ops: &[Op]) -> std::result::Result<(), ApplyError> {
        let _ = ctx;
        self.apply_checked(ops)
    }
    /// What the three shipped backends implement instead of `apply_checked`, which they define as
    /// this call with the system's context and no tracked models. Not for callers: they use
    /// `apply_ctx` or `apply_checked`. The default refuses, so a backend that forgets it fails
    /// loudly instead of calling itself.
    #[doc(hidden)]
    fn apply_tracked(
        &self,
        ctx: &WriteCtx,
        ops: &[Op],
        tracking: &Tracking,
    ) -> std::result::Result<(), ApplyError> {
        let _ = (ctx, ops, tracking);
        Err(ApplyError::Storage(
            "this storage backend has no atomic batches".into(),
        ))
    }
    /// Keep system fields and history for `model` from now on (D75). Only [`Tracked`] has a list.
    fn track(&self, _model: &str, _how: Track) {}
    /// Stop tracking `model`.
    fn untrack(&self, _model: &str) {}
    /// The models that are tracked, sorted.
    fn tracked_models(&self) -> Vec<String> {
        Vec::new()
    }
    /// The key ring that seals the values of sensitive fields in history entries (D75). Only
    /// [`Tracked`] keeps one; without it such a field is recorded as `redacted`.
    fn set_sealer(&self, _sealer: Arc<dyn Sealer>) {}
    /// The sealer set with [`Backend::set_sealer`], if any.
    fn sealer(&self) -> Option<Arc<dyn Sealer>> {
        None
    }
    /// The change history of one record, newest first (D75). A backend that keeps no history
    /// refuses, which is not the same as a record with no entries.
    fn history(&self, _q: &HistoryQuery) -> Result<Vec<ChangeRow>> {
        Err("this storage backend keeps no change history".into())
    }
    /// The entries of the change history of every record, in the order a consumer must read them
    /// (D75): one page, from `q.after` on. On PostgreSQL the order is `(txid, seq)` and an entry is
    /// shown only once no transaction that holds a lower transaction number is still open, so a
    /// consumer that follows the cursor never misses an entry that committed late; memory and
    /// SQLite order by `seq`. A cursor that cannot be followed (the history was restored, the
    /// database went back in time, was moved to another cluster or changed time line, which every
    /// promotion does) is refused with an error that starts `cursor_expired:`. A backend that
    /// keeps no history refuses.
    fn changes(&self, _q: &ChangeQuery) -> Result<ChangePage> {
        Err("this storage backend keeps no change history".into())
    }
    /// Check that the store can keep the change history, and under `Ddl::IfMissing` make what is
    /// missing of it (a start does not; the first write of a model that keeps history does the
    /// same). Under `Ddl::Never` it creates nothing and answers with an error that says what an
    /// owner must install. The stores that need no setup say `Ok`.
    fn ensure_history_schema(&self) -> Result<()> {
        Ok(())
    }
    /// What the store can say about its history without reading it.
    fn history_stats(&self) -> Result<HistoryStats> {
        Ok(HistoryStats::default())
    }
    /// Make the partition of the month `YYYY-MM` (UTC) of the history, and move into it the
    /// entries of that month that landed in the default partition. Idempotent. A store without
    /// partitions does nothing. PostgreSQL refuses a month before 2000 or more than a year ahead.
    fn history_ensure_partition(&self, _month: &str) -> Result<PartitionEnsured> {
        Ok(PartitionEnsured::default())
    }
    /// Drop the partition of the month `YYYY-MM` and return how many entries it held. The
    /// database refuses a month that ended less than `min_age_days` days ago, which the caller
    /// sets to the **longest** retention of the models that have entries in the month (the
    /// database cannot see the models). A month that is not over is never dropped; the SQL
    /// function also refuses a missing or negative age, also one that mixes months and days of
    /// opposite signs (the floor is tested as an instant): it is the check that outlives a
    /// mistake of the caller.
    fn history_drop_partition(&self, _month: &str, _min_age_days: u32) -> Result<u64> {
        Ok(0)
    }
    /// Stamp up to `batch` records of `models` that predate the system fields: `created_at` from
    /// the UUIDv7 id, else `fallback_at` (canonical text); `row_version` 1; `sys_backfilled`;
    /// no actor; no history entry. Call it until `remaining` is 0.
    fn backfill_sysfields(
        &self,
        _models: &[String],
        _fallback_at: &str,
        _batch: usize,
    ) -> Result<Backfilled> {
        Err("this storage backend cannot stamp the system fields of old records".into())
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
    /// The records a uniqueness rule (D57) compares with a new value: those of `model` whose
    /// `fields[0]` equals `values[0]` and whose later `fields[i]` equal `values[i]`, a missing or
    /// null stored value counting as the empty text `""` (so the caller passes `""` for a scope
    /// field it has no value for), in insertion order. [`unique_matches`] is the one definition of
    /// "equal": a JSON string `"5"` and the number `5` are different values.
    ///
    /// The answer is the same on every backend; only the cost differs. The default reads the model
    /// ([`Backend::find`] on `fields[0]`, then the other fields). SQLite and PostgreSQL ask the
    /// database with exactly the expressions of the index [`Backend::ensure_unique`] makes for the
    /// same `fields` in the same order, so with that index the cost does not grow with the model.
    /// Without it they scan, as `find` does. The caller passes a non-empty `values[0]`: the rule
    /// does not constrain an empty value.
    fn find_unique(
        &self,
        model: &str,
        fields: &[&str],
        values: &[J],
    ) -> Result<Vec<(String, Fields)>> {
        let (field, value) = unique_head(fields, values)?;
        Ok(self
            .find(model, field, value)?
            .into_iter()
            .filter(|(_, rec)| unique_matches(rec, fields, values))
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

    // ---- shared state for several server processes on one database (B1, D59) ---------------

    /// Take the cross-process lock `key` and hold it until the guard is dropped. Blocks up to
    /// `LOCK_WAIT`. PostgreSQL uses a session advisory lock (released by the server when a
    /// process dies), SQLite an operating-system file lock next to the database file, memory a
    /// process-local lock. Not re-entrant: do not take the same key twice on one thread.
    fn lock(&self, key: &str) -> Result<LockGuard>;

    /// Add one to the counter `key` and return the new value. A counter lives `ttl_ms` from its
    /// first hit (a fixed window; `0` means it never expires); when `extend` is set every hit
    /// pushes the expiry out to `ttl_ms` from now. An expired counter starts again at 1.
    fn counter_add(&self, key: &str, ttl_ms: u64, extend: bool) -> Result<u64>;
    /// The live value of a counter (0 when absent or expired).
    fn counter_get(&self, key: &str) -> Result<u64>;
    /// Milliseconds until a live counter expires (`None` when absent, expired or never).
    fn counter_ttl_ms(&self, key: &str) -> Result<Option<u64>>;
    fn counter_reset(&self, key: &str) -> Result<()>;

    /// Short-lived text values with an expiry, shared by every process: sessions, idempotency
    /// keys, vaults. `ttl_ms` of `None` means no expiry. Overwrites.
    fn kv_put(&self, key: &str, value: &str, ttl_ms: Option<u64>) -> Result<()>;
    /// Store only if the key is absent or expired; `true` when stored.
    fn kv_put_new(&self, key: &str, value: &str, ttl_ms: Option<u64>) -> Result<bool>;
    fn kv_get(&self, key: &str) -> Result<Option<String>>;
    fn kv_delete(&self, key: &str) -> Result<bool>;
    /// Live entries whose key starts with `prefix`, oldest key first.
    fn kv_scan(&self, prefix: &str) -> Result<Vec<(String, String)>>;
    /// Remove expired entries and counters; returns how many.
    fn purge_expired(&self) -> Result<usize>;

    /// Every record, in insertion order, in batches of at most `batch` rows, from one consistent
    /// snapshot (a repeatable-read transaction on PostgreSQL, a read transaction on SQLite).
    /// Memory is bounded by the batch. For `obop backup`. A store that keeps the change history
    /// of records (D75) delivers it after the records, as rows of the model `record_change` whose
    /// id is the number of the entry and whose fields carry the key `$kind` (`ChangeRow::to_fields`;
    /// a record of a model of that name, from a package installed before the name was reserved,
    /// has no such key), and `import` takes them back.
    fn export(&self, batch: usize, sink: &mut RowSink<'_>) -> Result<()>;
    /// Number of records of every model.
    fn total(&self) -> Result<usize>;

    /// Compare-and-set text fields of records, all in one transaction: each swap replaces the
    /// field only if it still holds `old`. Returns how many applied; a swap whose record changed
    /// meanwhile (or is gone) is skipped, not an error. For re-sealing secrets while serving.
    fn swap_fields(&self, swaps: &[Swap]) -> Result<usize>;
    /// Compare-and-set a short-lived value (see `kv_put`) if it still holds `old`.
    fn kv_swap(&self, key: &str, old: &str, new: &str) -> Result<bool>;

    /// Fill the database from a stream of record batches, in ONE transaction: `next` is called
    /// until it returns `None`. With `wipe`, every record and every short-lived value is removed
    /// first, in the same transaction, so a failure leaves the database as it was. Returns the
    /// number of records written. For `obop restore`.
    fn import(
        &self,
        wipe: bool,
        next: &mut dyn FnMut() -> Result<Option<Vec<Row>>>,
    ) -> Result<usize>;
}

/// The first field and value of a keyed lookup, after checking that every field has a value.
pub(crate) fn unique_head<'a>(fields: &[&'a str], values: &'a [J]) -> Result<(&'a str, &'a J)> {
    match (fields.first(), values.first()) {
        (Some(f), Some(v)) if fields.len() == values.len() => Ok((*f, v)),
        _ => Err("a keyed lookup needs a value for each of one or more fields".to_string()),
    }
}

/// Whether `rec` is a record that [`Backend::find_unique`] returns for `fields` and `values`: the
/// first field holds the first value, and every other field holds its value or, where the stored
/// field is missing or null, the empty text. This is the rule of `unique_with` (D57), written
/// once: the command layer applies it to the records of a unit in progress and the backends to the
/// candidates the database returns.
pub fn unique_matches(rec: &Fields, fields: &[&str], values: &[J]) -> bool {
    let (Some((first, rest)), Some((want, wanted))) = (fields.split_first(), values.split_first())
    else {
        return false;
    };
    rec.get(*first) == Some(want)
        && rest.len() == wanted.len()
        && rest.iter().zip(wanted).all(|(f, w)| match rec.get(*f) {
            None | Some(J::Null) => matches!(w, J::String(s) if s.is_empty()),
            Some(v) => v == w,
        })
}

/// One compare-and-set of a text field of a record.
#[derive(Debug, Clone)]
pub struct Swap {
    pub model: String,
    pub id: String,
    pub field: String,
    pub old: String,
    pub new: String,
}

/// One stored record: model, id, fields.
pub type Row = (String, String, Fields);
/// Receives the batches of `Backend::export`.
pub type RowSink<'a> = dyn FnMut(&[Row]) -> Result<()> + 'a;

/// How long `Backend::lock` waits before giving up.
pub const LOCK_WAIT: std::time::Duration = std::time::Duration::from_secs(60);

/// Held while a `Backend::lock` is taken; dropping it releases the lock.
pub type LockGuard = Box<dyn std::any::Any + Send>;

impl dyn Backend {
    /// Run `f` while holding the cross-process lock `key`.
    pub fn with_lock<T>(&self, key: &str, f: impl FnOnce() -> T) -> Result<T> {
        let _g = self.lock(key)?;
        Ok(f())
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// A key as a safe file-name part.
fn lock_file_part(key: &str) -> String {
    let ok = !key.is_empty()
        && key.len() <= 48
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
    if ok {
        key.to_string()
    } else {
        // A stable, collision-resistant name for any other key (FNV-1a, 64 bit, twice).
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in key.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x100_0000_01b3);
        }
        format!("k{h:016x}")
    }
}

/// Process-local locks by key, for memory databases and as the first step everywhere.
#[derive(Default)]
struct LocalLocks {
    held: Mutex<std::collections::HashSet<String>>,
    cv: std::sync::Condvar,
}

struct LocalGuard {
    locks: Arc<LocalLocks>,
    key: String,
}

impl Drop for LocalGuard {
    fn drop(&mut self) {
        self.locks
            .held
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.key);
        self.locks.cv.notify_all();
    }
}

impl LocalLocks {
    fn take(self: &Arc<Self>, key: &str) -> Result<LocalGuard> {
        let deadline = std::time::Instant::now() + LOCK_WAIT;
        let mut g = self.held.lock().unwrap_or_else(|e| e.into_inner());
        while g.contains(key) {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return Err(format!("timed out waiting for lock {key}"));
            }
            g = self
                .cv
                .wait_timeout(g, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        g.insert(key.to_string());
        Ok(LocalGuard {
            locks: self.clone(),
            key: key.to_string(),
        })
    }
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

/// Whether the process may issue DDL (create tables) when it opens a PostgreSQL database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ddl {
    /// Create what is missing, under the lock that serialises processes starting together.
    IfMissing,
    /// Create nothing: a database that lacks the schema is an error that says so. For a
    /// deployment whose serving role may not create tables. It covers the record store
    /// (`schema::parts`) and the byte store's table (`blob::DbBytes`). The change history of D75
    /// is not made at start under either mode: under `Never` it is only checked, when a model
    /// that keeps history is written or `Backend::ensure_history_schema` is asked, and the
    /// write is refused where it is missing. No shipped process selects `Never` yet.
    Never,
}

impl Ddl {
    /// What [`open`] uses: `Never` in a production process (see [`production`], which no process
    /// sets yet, so every shipped process runs `IfMissing`), else `IfMissing`.
    pub fn for_process() -> Ddl {
        if production() {
            Ddl::Never
        } else {
            Ddl::IfMissing
        }
    }
}

/// How [`open_options`] opens a database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenOptions {
    /// The connection pool settings; `None` reads them from the environment (`OBOP_DB_*`, see
    /// docs/DEPLOY.md). Only PostgreSQL has a pool.
    pub pool: Option<PgPoolConfig>,
    /// Whether opening may create the schema. Only PostgreSQL has one to create.
    pub ddl: Ddl,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            pool: None,
            ddl: Ddl::for_process(),
        }
    }
}

static PRODUCTION: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Declare this process a production process: from then on [`open`] does not create the schema.
/// Nothing calls it yet. The start-up of the roles work (D77) will, for a server started as a
/// production server, once there is a command that installs the schema as the owner (until
/// then a production process could not be given a database to open).
pub fn set_production(on: bool) {
    PRODUCTION.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Is this a production process, that is, was [`set_production`] called? Read by [`open`] and by
/// nothing else.
///
/// The environment variable `OBOP_PRODUCTION=1` is deliberately not read yet ([`production_from`]
/// is the rule it will follow). Reading it now would make every command that opens a database
/// with [`open`] refuse an empty PostgreSQL database (`obop restore` into a fresh one, the
/// restore drill, `obop migrate rehearse --scratch`, the control plane's first start), and no
/// command installs the schema as the owner yet (`schema::owner_install` is a Rust function).
pub fn production() -> bool {
    PRODUCTION.load(std::sync::atomic::Ordering::Relaxed)
}

/// The rule by which the variable will be read once it is: the flag, or `OBOP_PRODUCTION` set to
/// `1`. Tested; not used by [`production`] yet.
#[cfg(test)]
fn production_from(flag: bool, variable: Option<&str>) -> bool {
    flag || variable.is_some_and(|v| v.trim() == "1")
}

/// Open the database `url` names: `memory:`, `sqlite://PATH` or `postgres://...`. A production
/// process (see [`production`]; none is yet) does not create the PostgreSQL schema; any other
/// does.
pub fn open(url: &str) -> Result<Arc<dyn Backend>> {
    open_options(url, &OpenOptions::default())
}

/// As [`open`], with the connection pool settings given instead of read from the environment
/// (`OBOP_DB_*`, see docs/DEPLOY.md), and the schema created when it is missing whatever
/// `set_production` says: for the commands that set a database up (the control plane seeding a
/// new instance, tests). Only PostgreSQL has a pool; the setting is ignored for `memory:` and
/// SQLite.
pub fn open_with(url: &str, pool: Option<PgPoolConfig>) -> Result<Arc<dyn Backend>> {
    open_options(
        url,
        &OpenOptions {
            pool,
            ddl: Ddl::IfMissing,
        },
    )
}

/// Open a database with every setting given. The backend it returns is the [`Tracked`] wrapper
/// around the store (and the guard of `fault` around PostgreSQL), so a caller that holds only
/// `Arc<dyn Backend>` reaches `apply_ctx` and the list of tracked models.
pub fn open_options(url: &str, options: &OpenOptions) -> Result<Arc<dyn Backend>> {
    let backend = open_backend(url, options)?;
    // In a build with `test-support` (the tests of the workspace, never a release) the process
    // tests can make the binary's store refuse calls: `OBOP_TEST_FAULTS`, see `testing`.
    #[cfg(feature = "test-support")]
    let backend = testing::faults_from_env(backend);
    Ok(Tracked::wrap(backend))
}

fn open_backend(url: &str, options: &OpenOptions) -> Result<Arc<dyn Backend>> {
    if url == "memory:" {
        Ok(Arc::new(Memory::default()))
    } else if let Some(path) = url
        .strip_prefix("sqlite://")
        .or_else(|| url.strip_prefix("sqlite:"))
    {
        Ok(Arc::new(Sqlite::open(path)?))
    } else if url.starts_with("postgres://") || url.starts_with("postgresql://") {
        let pg = Pg::connect_options(url, options.pool.clone(), options.ddl)?;
        // The guard keeps the rule of `fault` (no change after a refused call, commits noted).
        Ok(guarded(Arc::new(pg)))
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

pub mod ctx;
pub mod fault;
#[cfg(test)]
mod golden_tests;
mod guard;
pub mod history;
pub mod keyexpr;
pub mod limit;
mod memory;
mod pg;
mod pg_hist;
#[cfg(test)]
mod pg_hist_tests;
#[cfg(test)]
mod pg_pool_tests;
mod pgpool;
pub mod pool;
#[cfg(test)]
mod pool_tests;
pub mod refusal;
pub mod schema;
#[cfg(test)]
mod seams_tests;
#[cfg(test)]
mod shared_tests;
mod sqlite;
#[cfg(test)]
mod start_tests;
#[cfg(test)]
mod sys_tests;
pub mod sysfields;
#[cfg(any(test, feature = "test-support"))]
pub mod testing;
pub mod tls;
#[cfg(test)]
mod tls_tests;
pub mod tracked;

pub use ctx::{ACTOR_PUBLIC, ACTOR_SYSTEM, ActionRun, Via, WriteCtx};
pub use guard::guarded;
pub use history::{
    Backfilled, ChangeCursor, ChangePage, ChangeQuery, HistoryQuery, HistoryStats, PartitionEnsured,
};
pub use pgpool::{PgPoolConfig, Pooler, statement_timeouts};
pub use sysfields::ChangeRow;
pub use tracked::{Sealer, Track, Tracked, Tracking, Treat};

use memory::Memory;
use pg::Pg;
use sqlite::Sqlite;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn fields(k: &str, v: J) -> Fields {
        let mut m = Fields::new();
        m.insert(k.into(), v);
        m
    }

    /// The contract every backend must meet. Also run against PostgreSQL by
    /// `OBOP_TEST_POSTGRES_URL=postgres://... cargo test -p obop-storage`.
    pub(crate) fn contract(b: &dyn Backend) {
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
