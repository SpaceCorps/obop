//! A backend that can be told to refuse calls the way a full pool does (D74), for tests of the
//! layers above the store: does a caller that sees `storage_busy` change nothing, does a loop
//! that gets it try again later. Not for production use.

use crate::fault::{self, Fault};
use crate::{ApplyError, Backend, Fields, J, LockGuard, Op, Result, Row, RowSink, Swap, guarded};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU8, Ordering::SeqCst};

/// Which calls are refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// None.
    Healthy,
    /// Every read; writes go through.
    Reads,
    /// Every read and every change.
    All,
    /// Only `update` and `delete` of the model named to [`Flaky::refuse_model`] (the best-effort
    /// follow-up of a change that has just committed).
    Updates,
}

/// Wraps a backend; refuses calls as `storage_busy` while a mode or a budget says so, noting the
/// fault on the calling thread exactly as the PostgreSQL pool does.
pub struct Flaky {
    inner: Arc<dyn Backend>,
    mode: AtomicU8,
    next: AtomicI64,
    refused: AtomicI64,
    target: std::sync::Mutex<Option<(String, Mode)>>,
    after_write: AtomicU8,
    after_budget: AtomicI64,
    apply_budget: AtomicI64,
    apply_skip: AtomicI64,
    kv_new_budget: AtomicI64,
}

impl Flaky {
    pub fn new(inner: Arc<dyn Backend>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            mode: AtomicU8::new(0),
            next: AtomicI64::new(0),
            refused: AtomicI64::new(0),
            target: std::sync::Mutex::new(None),
            after_write: AtomicU8::new(0),
            after_budget: AtomicI64::new(0),
            apply_budget: AtomicI64::new(0),
            apply_skip: AtomicI64::new(0),
            kv_new_budget: AtomicI64::new(0),
        })
    }

    /// A memory database behind a `Flaky` behind the request guard: what `open` returns for
    /// PostgreSQL, with refusals on demand.
    pub fn memory() -> (Arc<Flaky>, Arc<dyn Backend>) {
        let f = Flaky::new(crate::open("memory:").expect("memory backend"));
        let g = guarded(f.clone());
        (f, g)
    }

    pub fn set(&self, mode: Mode) {
        self.mode.store(
            match mode {
                Mode::Healthy | Mode::Updates => 0,
                Mode::Reads => 1,
                Mode::All => 2,
            },
            SeqCst,
        );
    }

    /// Refuse the next `n` calls of any kind (locks and `kind` excepted), then go on as before.
    pub fn refuse_next(&self, n: i64) {
        self.next.store(n, SeqCst);
    }

    /// How many calls were refused so far.
    pub fn refused(&self) -> i64 {
        self.refused.load(SeqCst)
    }

    /// Refuse the calls that touch `model` (its reads, or all of them), whatever the mode.
    pub fn refuse_model(&self, model: &str, mode: Mode) {
        *self.target.lock().unwrap() = Some((model.to_string(), mode));
    }

    /// Stop refusing a model (see [`Flaky::refuse_model`]) and the reads after a write.
    pub fn allow_models(&self) {
        *self.target.lock().unwrap() = None;
        self.after_write.store(0, SeqCst);
        self.after_budget.store(0, SeqCst);
        self.next.store(0, SeqCst);
        self.apply_budget.store(0, SeqCst);
        self.apply_skip.store(0, SeqCst);
        self.kv_new_budget.store(0, SeqCst);
    }

    /// From the next successful change on, refuse every read: the pool fills up at the very
    /// moment a request has committed, so its read-back is refused.
    pub fn refuse_reads_after_the_next_write(&self) {
        self.after_write.store(1, SeqCst);
    }

    /// After the next successful change, refuse the next `n` calls of any kind, then go on: the
    /// pool is full for a moment right after a request committed.
    pub fn refuse_n_after_the_next_write(&self, n: i64) {
        self.after_budget.store(n, SeqCst);
        self.after_write.store(3, SeqCst);
    }

    /// Refuse the next `n` atomic batches (`apply`, `apply_checked`) and nothing else.
    pub fn refuse_applies(&self, n: i64) {
        self.apply_budget.store(n, SeqCst);
    }

    /// Refuse the next `n` `kv_put_new` calls (the claim of a key) and nothing else.
    pub fn refuse_kv_new(&self, n: i64) {
        self.kv_new_budget.store(n, SeqCst);
    }

    /// Let `skip` atomic batches through, then refuse the one after them (and nothing else): the
    /// request that committed `skip` batches and met a refusal at the next.
    pub fn refuse_nth_apply(&self, skip: i64) {
        self.apply_skip.store(skip, SeqCst);
        self.apply_budget.store(1, SeqCst);
    }

    fn gate_apply(&self) -> Result<()> {
        // Batches to let through before the budget starts to refuse.
        let skipped = self
            .apply_skip
            .try_update(SeqCst, SeqCst, |n| (n > 0).then_some(n - 1))
            .is_ok();
        let hit = !skipped
            && self
                .apply_budget
                .try_update(SeqCst, SeqCst, |n| (n > 0).then_some(n - 1))
                .is_ok();
        if hit {
            self.refused.fetch_add(1, SeqCst);
            fault::note(Fault::Busy);
            return Err(Fault::Busy.message("all database connections are in use, try again"));
        }
        self.gate(true)
    }

    fn gate(&self, write: bool) -> Result<()> {
        self.gate_m(write, "")
    }

    fn gate_u(&self, model: &str) -> Result<()> {
        let hit = matches!(&*self.target.lock().unwrap(), Some((m, Mode::Updates)) if m == model);
        if hit {
            self.refused.fetch_add(1, SeqCst);
            fault::note(Fault::Busy);
            return Err(Fault::Busy.message("all database connections are in use, try again"));
        }
        self.gate_m(true, model)
    }

    fn counted_after(&self) -> bool {
        self.after_write.load(SeqCst) == 4
            && self
                .after_budget
                .try_update(SeqCst, SeqCst, |n| (n > 0).then_some(n - 1))
                .is_ok()
    }

    fn gate_m(&self, write: bool, model: &str) -> Result<()> {
        let by_budget = self
            .next
            .try_update(SeqCst, SeqCst, |n| (n > 0).then_some(n - 1))
            .is_ok();
        let by_mode = match self.mode.load(SeqCst) {
            1 => !write,
            2 => true,
            _ => false,
        };
        // 0: off, 1: armed (waiting for a change), 2: a change went through, reads are refused.
        let by_after_write = if write {
            let _ = self.after_write.compare_exchange(1, 2, SeqCst, SeqCst);
            // The change that arms a counted refusal passes; the calls after it are refused.
            if self
                .after_write
                .compare_exchange(3, 4, SeqCst, SeqCst)
                .is_ok()
            {
                false
            } else {
                self.counted_after()
            }
        } else {
            self.after_write.load(SeqCst) == 2 || self.counted_after()
        };
        let by_model = match &*self.target.lock().unwrap() {
            Some((m, Mode::Reads)) => m == model && !write,
            Some((m, Mode::All)) => m == model,
            _ => false,
        };
        if by_budget || by_mode || by_model || by_after_write {
            self.refused.fetch_add(1, SeqCst);
            fault::note(Fault::Busy);
            return Err(Fault::Busy.message("all database connections are in use, try again"));
        }
        Ok(())
    }
}

impl Backend for Flaky {
    fn insert(&self, model: &str, id: &str, fields: &Fields) -> Result<()> {
        self.gate_m(true, model)?;
        self.inner.insert(model, id, fields)
    }
    fn get(&self, model: &str, id: &str) -> Result<Option<Fields>> {
        self.gate_m(false, model)?;
        self.inner.get(model, id)
    }
    fn list(&self, model: &str) -> Result<Vec<(String, Fields)>> {
        self.gate_m(false, model)?;
        self.inner.list(model)
    }
    fn scan(
        &self,
        model: &str,
        visit: &mut dyn FnMut(&str, &Fields) -> Result<bool>,
    ) -> Result<()> {
        self.gate_m(false, model)?;
        self.inner.scan(model, visit)
    }
    fn update(&self, model: &str, id: &str, patch: &Fields) -> Result<bool> {
        self.gate_u(model)?;
        self.inner.update(model, id, patch)
    }
    fn count(&self, model: &str) -> Result<usize> {
        self.gate_m(false, model)?;
        self.inner.count(model)
    }
    fn delete(&self, model: &str, id: &str) -> Result<bool> {
        self.gate_u(model)?;
        self.inner.delete(model, id)
    }
    fn insert_many(&self, rows: &[(String, String, Fields)]) -> Result<()> {
        self.gate(true)?;
        self.inner.insert_many(rows)
    }
    fn apply(&self, ops: &[Op]) -> Result<()> {
        self.gate_apply()?;
        self.inner.apply(ops)
    }
    fn apply_checked(&self, ops: &[Op]) -> std::result::Result<(), ApplyError> {
        self.gate_apply().map_err(ApplyError::Storage)?;
        self.inner.apply_checked(ops)
    }
    fn find(&self, model: &str, field: &str, value: &J) -> Result<Vec<(String, Fields)>> {
        self.gate_m(false, model)?;
        self.inner.find(model, field, value)
    }
    fn ensure_unique(&self, name: &str, model: &str, fields: &[&str]) -> Result<()> {
        self.gate(true)?;
        self.inner.ensure_unique(name, model, fields)
    }
    fn kind(&self) -> &'static str {
        self.inner.kind()
    }
    fn lock(&self, key: &str) -> Result<LockGuard> {
        self.inner.lock(key)
    }
    fn counter_add(&self, key: &str, ttl_ms: u64, extend: bool) -> Result<u64> {
        self.gate(true)?;
        self.inner.counter_add(key, ttl_ms, extend)
    }
    fn counter_get(&self, key: &str) -> Result<u64> {
        self.gate(false)?;
        self.inner.counter_get(key)
    }
    fn counter_ttl_ms(&self, key: &str) -> Result<Option<u64>> {
        self.gate(false)?;
        self.inner.counter_ttl_ms(key)
    }
    fn counter_reset(&self, key: &str) -> Result<()> {
        self.gate(true)?;
        self.inner.counter_reset(key)
    }
    fn kv_put(&self, key: &str, value: &str, ttl_ms: Option<u64>) -> Result<()> {
        self.gate(true)?;
        self.inner.kv_put(key, value, ttl_ms)
    }
    fn kv_put_new(&self, key: &str, value: &str, ttl_ms: Option<u64>) -> Result<bool> {
        let hit = self
            .kv_new_budget
            .try_update(SeqCst, SeqCst, |n| (n > 0).then_some(n - 1))
            .is_ok();
        if hit {
            self.refused.fetch_add(1, SeqCst);
            fault::note(Fault::Busy);
            return Err(Fault::Busy.message("all database connections are in use, try again"));
        }
        self.gate(true)?;
        self.inner.kv_put_new(key, value, ttl_ms)
    }
    fn kv_get(&self, key: &str) -> Result<Option<String>> {
        self.gate(false)?;
        self.inner.kv_get(key)
    }
    fn kv_delete(&self, key: &str) -> Result<bool> {
        self.gate(true)?;
        self.inner.kv_delete(key)
    }
    fn kv_scan(&self, prefix: &str) -> Result<Vec<(String, String)>> {
        self.gate(false)?;
        self.inner.kv_scan(prefix)
    }
    fn purge_expired(&self) -> Result<usize> {
        self.gate(true)?;
        self.inner.purge_expired()
    }
    fn export(&self, batch: usize, sink: &mut RowSink<'_>) -> Result<()> {
        self.gate(false)?;
        self.inner.export(batch, sink)
    }
    fn total(&self) -> Result<usize> {
        self.gate(false)?;
        self.inner.total()
    }
    fn swap_fields(&self, swaps: &[Swap]) -> Result<usize> {
        self.gate(true)?;
        self.inner.swap_fields(swaps)
    }
    fn kv_swap(&self, key: &str, old: &str, new: &str) -> Result<bool> {
        self.gate(true)?;
        self.inner.kv_swap(key, old, new)
    }
    fn import(
        &self,
        wipe: bool,
        next: &mut dyn FnMut() -> Result<Option<Vec<Row>>>,
    ) -> Result<usize> {
        self.gate(true)?;
        self.inner.import(wipe, next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fields(v: serde_json::Value) -> Fields {
        v.as_object().cloned().unwrap()
    }

    #[test]
    fn after_a_refused_read_the_guard_refuses_changes_in_a_request_and_not_outside_one() {
        let (flaky, db) = Flaky::memory();
        db.insert("t", "a", &fields(json!({"n": 1}))).unwrap();
        // A request: the read is refused and swallowed, the caller goes on to write.
        fault::begin_request(Default::default());
        flaky.refuse_next(1);
        assert!(db.get("t", "a").is_err());
        let w = db.update("t", "a", &fields(json!({"n": 2})));
        assert!(
            Fault::is_transient(&w.unwrap_err()),
            "the change is refused"
        );
        let w = db.kv_put("k", "v", None);
        assert!(w.is_err());
        // Counters and locks are bookkeeping and still work.
        assert!(db.counter_add("c", 1000, false).is_ok());
        let out = fault::end_request();
        assert_eq!(out.fault, Some(Fault::Busy));
        assert!(!out.wrote);
        assert_eq!(
            db.get("t", "a").unwrap().unwrap()["n"],
            1,
            "nothing changed"
        );
        // Outside a request the same sequence just works (a background thread).
        flaky.refuse_next(1);
        assert!(db.get("t", "a").is_err());
        assert!(db.update("t", "a", &fields(json!({"n": 3}))).unwrap());
    }

    #[test]
    fn bookkeeping_is_allowed_after_a_refusal_and_the_log_and_dedup_markers_are_not_changes() {
        let (flaky, db) = Flaky::memory();
        db.insert("t", "a", &Fields::new()).unwrap();
        fault::begin_request(Default::default());
        // A request claims an idempotency key and issues a token, commits a record, and is then
        // refused a read.
        assert!(db.kv_put_new("idem", "1", None).unwrap());
        db.insert("tok", "t1", &fields(json!({"live": true})))
            .unwrap();
        assert!(fault::state().wrote);
        flaky.refuse_next(1);
        assert!(db.get("t", "a").is_err());
        // Ordinary writes are refused now ...
        assert!(db.insert("t", "b", &Fields::new()).is_err());
        assert!(db.kv_delete("idem").is_err());
        // ... the security log is not (the entry of a committed change must not be lost) ...
        db.insert("security_event", "e1", &Fields::new()).unwrap();
        // ... and compensations are: the key is released, the token revoked (which undoes the
        // change it counted), and the dedup marker of a webhook is released.
        fault::bookkeeping(|| {
            assert!(db.kv_delete("idem").unwrap());
            assert!(
                db.update("tok", "t1", &fields(json!({"live": false})))
                    .unwrap()
            );
        });
        assert!(
            db.kv_put_new("idem", "2", None).is_err(),
            "outside it is refused again"
        );
        assert!(!fault::state().wrote, "the only change was undone");
        let out = fault::end_request();
        assert_eq!(out.fault, Some(Fault::Busy));
        assert!(!out.wrote && !out.partial);
        assert_eq!(db.get("tok", "t1").unwrap().unwrap()["live"], false);
        assert_eq!(db.list("security_event").unwrap().len(), 1);
        // A webhook's dedup marker is not a change.
        fault::begin_request(Default::default());
        db.insert("connector_seen", "c:1", &Fields::new()).unwrap();
        assert!(!fault::end_request().wrote);
    }

    #[test]
    fn commits_are_noted_except_counters_values_dedup_markers_and_the_security_log() {
        let (_flaky, db) = Flaky::memory();
        fault::begin_request(Default::default());
        db.insert("security_event", "e1", &Fields::new()).unwrap();
        db.counter_add("c", 1000, false).unwrap();
        db.kv_put("k", "v", None).unwrap();
        db.kv_put_new("k2", "v", None).unwrap();
        assert!(!fault::end_request().wrote);

        fault::begin_request(Default::default());
        db.insert("t", "a", &Fields::new()).unwrap();
        assert!(fault::end_request().wrote);

        // A change that did nothing (no such record) is not a commit.
        fault::begin_request(Default::default());
        assert!(!db.update("t", "ghost", &Fields::new()).unwrap());
        assert!(!db.delete("t", "ghost").unwrap());
        assert!(!fault::end_request().wrote);
    }
}
