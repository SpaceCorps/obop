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
    /// Memory is bounded by the batch. For `obop backup`.
    fn export(&self, batch: usize, sink: &mut RowSink<'_>) -> Result<()>;
    /// Number of records of every model.
    fn total(&self) -> Result<usize>;

    /// Compare-and-set text fields of records, all in one transaction: each swap replaces the
    /// field only if it still holds `old`. Returns how many applied; a swap whose record changed
    /// meanwhile (or is gone) is skipped, not an error. For re-sealing secrets while serving.
    fn swap_fields(&self, swaps: &[Swap]) -> Result<usize>;
    /// Compare-and-set a short-lived value (see `kv_put`) if it still holds `old`.
    fn kv_swap(&self, key: &str, old: &str, new: &str) -> Result<bool>;

    /// Apply several writes as one transaction: all of them or none. A `Merge` of a record that
    /// does not exist fails the whole batch. For migrations.
    fn apply(&self, ops: &[Op]) -> Result<()>;

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

/// One write of `Backend::apply`.
#[derive(Debug, Clone)]
pub enum Op {
    /// Create the record, or replace it entirely when it exists.
    Put {
        model: String,
        id: String,
        fields: Fields,
    },
    /// Merge fields into an existing record (like `update`).
    Merge {
        model: String,
        id: String,
        patch: Fields,
    },
    /// Remove a record; a missing one is not an error.
    Delete { model: String, id: String },
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

pub mod limit;
mod memory;
mod pg;
#[cfg(test)]
mod shared_tests;
mod sqlite;

use memory::Memory;
use pg::Pg;
use sqlite::Sqlite;

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
