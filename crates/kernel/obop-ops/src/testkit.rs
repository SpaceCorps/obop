//! Test support: a backend that can be made to fail, to test what the code around it leaves
//! behind when storage fails half way.

use obop_storage::{
    ApplyError, Backend, Fields, LockGuard, Op, Result, Row, RowSink, Swap, Track, Tracking,
    WriteCtx,
};
use serde_json::Value as J;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Wraps a backend; each switch makes one kind of call fail.
pub struct Flaky {
    pub inner: Arc<dyn Backend>,
    /// `export` fails after delivering its first batch.
    pub fail_export: AtomicBool,
    /// `apply` and `apply_checked` fail (writes of the catalogue, the ledger and so on).
    pub fail_apply: AtomicBool,
    /// `kv_put_new` fails (the lease of the scheduler).
    pub fail_lease: AtomicBool,
    /// Models whose reads (`get`, `list`, `find`) are refused as a full pool refuses them.
    pub refuse_reads: std::sync::Mutex<Vec<String>>,
    /// Reads of those models to let through first (a refusal that starts at the Nth read).
    pub allow_reads: std::sync::atomic::AtomicUsize,
    /// `(model, id)` pairs whose `get` is refused, the same way (a record of a model that others
    /// read, such as the schema version beside the id of the database).
    pub refuse_gets: std::sync::Mutex<Vec<(String, String)>>,
}

impl Flaky {
    pub fn new(inner: Arc<dyn Backend>) -> Arc<Flaky> {
        Arc::new(Flaky {
            inner,
            fail_export: AtomicBool::new(false),
            fail_apply: AtomicBool::new(false),
            fail_lease: AtomicBool::new(false),
            refuse_reads: std::sync::Mutex::default(),
            allow_reads: std::sync::atomic::AtomicUsize::new(0),
            refuse_gets: std::sync::Mutex::default(),
        })
    }

    fn refused(&self, model: &str) -> Result<()> {
        if self.refuse_reads.lock().unwrap().iter().any(|m| m == model)
            && self
                .allow_reads
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                .is_err()
        {
            return Err(obop_storage::fault::Fault::Busy
                .message("all database connections are in use, try again"));
        }
        Ok(())
    }

    pub fn set(flag: &AtomicBool, on: bool) {
        flag.store(on, Ordering::SeqCst);
    }

    fn on(flag: &AtomicBool) -> bool {
        flag.load(Ordering::SeqCst)
    }
}

impl Backend for Flaky {
    fn insert(&self, model: &str, id: &str, fields: &Fields) -> Result<()> {
        self.inner.insert(model, id, fields)
    }
    fn get(&self, model: &str, id: &str) -> Result<Option<Fields>> {
        self.refused(model)?;
        if self
            .refuse_gets
            .lock()
            .unwrap()
            .iter()
            .any(|(m, i)| m == model && i == id)
            && self
                .allow_reads
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                .is_err()
        {
            return Err(obop_storage::fault::Fault::Busy
                .message("all database connections are in use, try again"));
        }
        self.inner.get(model, id)
    }
    fn list(&self, model: &str) -> Result<Vec<(String, Fields)>> {
        self.refused(model)?;
        self.inner.list(model)
    }
    fn update(&self, model: &str, id: &str, patch: &Fields) -> Result<bool> {
        self.inner.update(model, id, patch)
    }
    fn count(&self, model: &str) -> Result<usize> {
        self.inner.count(model)
    }
    fn delete(&self, model: &str, id: &str) -> Result<bool> {
        self.inner.delete(model, id)
    }
    fn insert_many(&self, rows: &[Row]) -> Result<()> {
        self.inner.insert_many(rows)
    }
    fn apply(&self, ops: &[Op]) -> Result<()> {
        if Self::on(&self.fail_apply) {
            return Err("disk full (simulated catalogue write failure)".into());
        }
        self.inner.apply(ops)
    }
    fn apply_checked(&self, ops: &[Op]) -> std::result::Result<(), ApplyError> {
        if Self::on(&self.fail_apply) {
            return Err(ApplyError::Storage("disk full (simulated)".into()));
        }
        self.inner.apply_checked(ops)
    }
    fn apply_ctx(&self, ctx: &WriteCtx, ops: &[Op]) -> std::result::Result<(), ApplyError> {
        if Self::on(&self.fail_apply) {
            return Err(ApplyError::Storage("disk full (simulated)".into()));
        }
        self.inner.apply_ctx(ctx, ops)
    }
    fn apply_tracked(
        &self,
        ctx: &WriteCtx,
        ops: &[Op],
        tracking: &Tracking,
    ) -> std::result::Result<(), ApplyError> {
        if Self::on(&self.fail_apply) {
            return Err(ApplyError::Storage("disk full (simulated)".into()));
        }
        self.inner.apply_tracked(ctx, ops, tracking)
    }
    fn track(&self, model: &str, how: Track) {
        self.inner.track(model, how)
    }
    fn untrack(&self, model: &str) {
        self.inner.untrack(model)
    }
    fn tracked_models(&self) -> Vec<String> {
        self.inner.tracked_models()
    }
    fn scan(
        &self,
        model: &str,
        visit: &mut dyn FnMut(&str, &Fields) -> Result<bool>,
    ) -> Result<()> {
        self.refused(model)?;
        self.inner.scan(model, visit)
    }
    fn ensure_unique(&self, name: &str, model: &str, fields: &[&str]) -> Result<()> {
        self.inner.ensure_unique(name, model, fields)
    }
    fn find(&self, model: &str, field: &str, value: &J) -> Result<Vec<(String, Fields)>> {
        self.refused(model)?;
        self.inner.find(model, field, value)
    }
    fn kind(&self) -> &'static str {
        self.inner.kind()
    }
    fn lock(&self, key: &str) -> Result<LockGuard> {
        self.inner.lock(key)
    }
    fn counter_add(&self, key: &str, ttl_ms: u64, extend: bool) -> Result<u64> {
        self.inner.counter_add(key, ttl_ms, extend)
    }
    fn counter_get(&self, key: &str) -> Result<u64> {
        self.inner.counter_get(key)
    }
    fn counter_ttl_ms(&self, key: &str) -> Result<Option<u64>> {
        self.inner.counter_ttl_ms(key)
    }
    fn counter_reset(&self, key: &str) -> Result<()> {
        self.inner.counter_reset(key)
    }
    fn kv_put(&self, key: &str, value: &str, ttl_ms: Option<u64>) -> Result<()> {
        self.inner.kv_put(key, value, ttl_ms)
    }
    fn kv_put_new(&self, key: &str, value: &str, ttl_ms: Option<u64>) -> Result<bool> {
        if Self::on(&self.fail_lease) {
            return Err("the lease cannot be written (simulated)".into());
        }
        self.inner.kv_put_new(key, value, ttl_ms)
    }
    fn kv_get(&self, key: &str) -> Result<Option<String>> {
        self.inner.kv_get(key)
    }
    fn kv_delete(&self, key: &str) -> Result<bool> {
        self.inner.kv_delete(key)
    }
    fn kv_scan(&self, prefix: &str) -> Result<Vec<(String, String)>> {
        self.inner.kv_scan(prefix)
    }
    fn purge_expired(&self) -> Result<usize> {
        self.inner.purge_expired()
    }
    fn export(&self, batch: usize, sink: &mut RowSink<'_>) -> Result<()> {
        if Self::on(&self.fail_export) {
            // Deliver something first, so the writer has begun a file.
            let mut first = true;
            self.inner.export(batch, &mut |rows| {
                if first {
                    first = false;
                    sink(rows)
                } else {
                    Err("the database went away (simulated)".into())
                }
            })?;
            return Err("the database went away (simulated)".into());
        }
        self.inner.export(batch, sink)
    }
    fn total(&self) -> Result<usize> {
        self.inner.total()
    }
    fn swap_fields(&self, swaps: &[Swap]) -> Result<usize> {
        self.inner.swap_fields(swaps)
    }
    fn kv_swap(&self, key: &str, old: &str, new: &str) -> Result<bool> {
        self.inner.kv_swap(key, old, new)
    }
    fn import(
        &self,
        wipe: bool,
        next: &mut dyn FnMut() -> Result<Option<Vec<Row>>>,
    ) -> Result<usize> {
        self.inner.import(wipe, next)
    }
}
