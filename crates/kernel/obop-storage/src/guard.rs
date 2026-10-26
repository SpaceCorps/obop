//! The request guard around a backend (D74): it makes the rule of [`crate::fault`] true for every
//! caller without the callers knowing.
//!
//! - Every call that changes state first asks [`fault::before_write`]: in a request scope that has
//!   already hit a storage fault the change is refused with the same fault. Callers that swallow a
//!   refused read (`.ok()`, `unwrap_or_default()`) and carry on to write what they derived from
//!   it can therefore not overwrite anything; the request ends as a 503 and changed nothing.
//! - Every change that commits is reported with [`fault::wrote`] (counters and the security log
//!   excepted: they are bookkeeping, and a repeat of them does no harm), so the HTTP layer knows
//!   when a fault came too late to say "nothing was changed".
//!
//! Outside a request scope (the connector worker, provisioning threads, the CLI) the guard does
//! nothing but delegate.

use crate::{ApplyError, Backend, Fields, J, LockGuard, Op, Result, Row, RowSink, Swap, fault};
use std::sync::Arc;

/// The model the security log is written to: its entries do not count as a change.
const LOG_MODEL: &str = crate::LOG_MODEL;

pub(crate) struct Guarded {
    inner: Arc<dyn Backend>,
}

impl Guarded {
    pub(crate) fn new(inner: Arc<dyn Backend>) -> Self {
        Self { inner }
    }
}

/// Wrap `inner` so that it keeps the rule of [`crate::fault`] in request scopes.
pub fn guarded(inner: Arc<dyn Backend>) -> Arc<dyn Backend> {
    Arc::new(Guarded::new(inner))
}

fn done<T>(r: Result<T>, changed: impl FnOnce(&T) -> bool, model: &str) -> Result<T> {
    if let Ok(v) = &r
        && model != LOG_MODEL
        && changed(v)
    {
        fault::wrote();
    }
    r
}

impl Backend for Guarded {
    fn insert(&self, model: &str, id: &str, fields: &Fields) -> Result<()> {
        fault::before_write()?;
        done(self.inner.insert(model, id, fields), |_| true, model)
    }
    fn get(&self, model: &str, id: &str) -> Result<Option<Fields>> {
        self.inner.get(model, id)
    }
    fn list(&self, model: &str) -> Result<Vec<(String, Fields)>> {
        self.inner.list(model)
    }
    fn scan(
        &self,
        model: &str,
        visit: &mut dyn FnMut(&str, &Fields) -> Result<bool>,
    ) -> Result<()> {
        self.inner.scan(model, visit)
    }
    fn update(&self, model: &str, id: &str, patch: &Fields) -> Result<bool> {
        fault::before_write()?;
        done(self.inner.update(model, id, patch), |n| *n, model)
    }
    fn count(&self, model: &str) -> Result<usize> {
        self.inner.count(model)
    }
    fn delete(&self, model: &str, id: &str) -> Result<bool> {
        fault::before_write()?;
        done(self.inner.delete(model, id), |n| *n, model)
    }
    fn insert_many(&self, rows: &[(String, String, Fields)]) -> Result<()> {
        fault::before_write()?;
        let r = self.inner.insert_many(rows);
        if r.is_ok() && rows.iter().any(|(m, _, _)| m != LOG_MODEL) {
            fault::wrote();
        }
        r
    }
    fn apply(&self, ops: &[Op]) -> Result<()> {
        fault::before_write()?;
        let r = self.inner.apply(ops);
        if r.is_ok() && !ops.is_empty() {
            fault::wrote();
        }
        r
    }
    fn apply_checked(&self, ops: &[Op]) -> std::result::Result<(), ApplyError> {
        fault::before_write().map_err(ApplyError::Storage)?;
        let r = self.inner.apply_checked(ops);
        if r.is_ok() && !ops.is_empty() {
            fault::wrote();
        }
        r
    }
    fn find(&self, model: &str, field: &str, value: &J) -> Result<Vec<(String, Fields)>> {
        self.inner.find(model, field, value)
    }
    fn ensure_unique(&self, name: &str, model: &str, fields: &[&str]) -> Result<()> {
        fault::before_write()?;
        let r = self.inner.ensure_unique(name, model, fields);
        if r.is_ok() {
            fault::wrote();
        }
        r
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
        fault::before_write()?;
        done(self.inner.kv_put(key, value, ttl_ms), |_| true, "")
    }
    fn kv_put_new(&self, key: &str, value: &str, ttl_ms: Option<u64>) -> Result<bool> {
        fault::before_write()?;
        done(self.inner.kv_put_new(key, value, ttl_ms), |n| *n, "")
    }
    fn kv_get(&self, key: &str) -> Result<Option<String>> {
        self.inner.kv_get(key)
    }
    fn kv_delete(&self, key: &str) -> Result<bool> {
        fault::before_write()?;
        done(self.inner.kv_delete(key), |n| *n, "")
    }
    fn kv_scan(&self, prefix: &str) -> Result<Vec<(String, String)>> {
        self.inner.kv_scan(prefix)
    }
    fn purge_expired(&self) -> Result<usize> {
        self.inner.purge_expired()
    }

    fn export(&self, batch: usize, sink: &mut RowSink<'_>) -> Result<()> {
        self.inner.export(batch, sink)
    }
    fn total(&self) -> Result<usize> {
        self.inner.total()
    }

    fn swap_fields(&self, swaps: &[Swap]) -> Result<usize> {
        fault::before_write()?;
        done(self.inner.swap_fields(swaps), |n| *n > 0, "")
    }
    fn kv_swap(&self, key: &str, old: &str, new: &str) -> Result<bool> {
        fault::before_write()?;
        done(self.inner.kv_swap(key, old, new), |n| *n, "")
    }

    fn import(
        &self,
        wipe: bool,
        next: &mut dyn FnMut() -> Result<Option<Vec<Row>>>,
    ) -> Result<usize> {
        fault::before_write()?;
        let r = self.inner.import(wipe, next);
        if r.is_ok() {
            fault::wrote();
        }
        r
    }
}
