//! A store double for the reader tests: a memory store that counts reads per model and can be told
//! to fail, to park a read until the test lets it go, to answer slowly, or to panic. A read that is
//! parked has already read the store, so what it returns is the state from before the test changed
//! something while it was parked.

use obop_storage::{
    ApplyError, Backend, Fields, LockGuard, Op, Result, Row, RowSink, Swap, WriteCtx,
};
use serde_json::Value as J;
use std::collections::BTreeMap;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Plan {
    /// Every `get` of these models fails with the text after it has been counted (and parked).
    fail: BTreeMap<String, String>,
    /// The next `get` of this model parks after it has read the store.
    park_next: Option<String>,
    /// Callers parked right now.
    parked: usize,
    /// Set when the parked callers may go.
    released: u64,
    /// The next `n` gets of the model answer after `delay` and then fail with the text.
    slow: Option<(String, Duration, String)>,
    /// The next `get` of this model panics.
    panic_next: Option<String>,
    /// The parked read panics when it is let go.
    panic_after_park: bool,
    /// The next atomic batch is refused with this text.
    refuse_apply: Option<String>,
}

pub struct Spy {
    inner: Arc<dyn Backend>,
    gets: Mutex<BTreeMap<String, usize>>,
    batches: Mutex<Vec<Vec<Op>>>,
    plan: Mutex<Plan>,
    cv: Condvar,
}

impl Spy {
    pub fn memory() -> Arc<Spy> {
        Spy::over(obop_storage::open("memory:").unwrap())
    }

    pub fn over(inner: Arc<dyn Backend>) -> Arc<Spy> {
        Arc::new(Spy {
            inner,
            gets: Mutex::new(BTreeMap::new()),
            batches: Mutex::new(Vec::new()),
            plan: Mutex::new(Plan::default()),
            cv: Condvar::new(),
        })
    }

    /// How often `get` was called for the model.
    pub fn gets(&self, model: &str) -> usize {
        self.gets.lock().unwrap().get(model).copied().unwrap_or(0)
    }

    /// The atomic batches that were sent, oldest first.
    pub fn batches(&self) -> Vec<Vec<Op>> {
        self.batches.lock().unwrap().clone()
    }

    /// Every read of `model` fails with `text` until [`Spy::heal`].
    pub fn fail(&self, model: &str, text: &str) {
        self.plan
            .lock()
            .unwrap()
            .fail
            .insert(model.to_string(), text.to_string());
    }

    pub fn heal(&self) {
        self.plan.lock().unwrap().fail.clear();
    }

    /// The next read of `model` parks, after it has read the store, until [`Spy::release`].
    pub fn park_next(&self, model: &str) {
        self.plan.lock().unwrap().park_next = Some(model.to_string());
    }

    /// Lets every parked read go.
    pub fn release(&self) {
        let mut p = self.plan.lock().unwrap();
        p.released += 1;
        self.cv.notify_all();
    }

    /// Waits until `n` reads are parked; panics after ten seconds.
    pub fn wait_parked(&self, n: usize) {
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            if self.plan.lock().unwrap().parked >= n {
                return;
            }
            assert!(Instant::now() < until, "no read parked");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// The next read of `model` takes `delay` and then fails with `text`: the pool's own timeout.
    pub fn slow_failure(&self, model: &str, delay: Duration, text: &str) {
        self.plan.lock().unwrap().slow = Some((model.to_string(), delay, text.to_string()));
    }

    /// The next read of `model` panics.
    pub fn panic_next(&self, model: &str) {
        self.plan.lock().unwrap().panic_next = Some(model.to_string());
    }

    /// The read that is parked (see [`Spy::park_next`]) panics when it is let go.
    pub fn panic_after_park(&self) {
        self.plan.lock().unwrap().panic_after_park = true;
    }

    /// The next atomic batch is refused with `text` and changes nothing.
    pub fn refuse_next_apply(&self, text: &str) {
        self.plan.lock().unwrap().refuse_apply = Some(text.to_string());
    }
}

impl Backend for Spy {
    fn insert(&self, model: &str, id: &str, fields: &Fields) -> Result<()> {
        self.inner.insert(model, id, fields)
    }
    fn get(&self, model: &str, id: &str) -> Result<Option<Fields>> {
        *self
            .gets
            .lock()
            .unwrap()
            .entry(model.to_string())
            .or_insert(0) += 1;
        {
            let mut p = self.plan.lock().unwrap();
            if p.panic_next.as_deref() == Some(model) {
                p.panic_next = None;
                drop(p);
                panic!("the store double panics on a read of {model}");
            }
        }
        let found = self.inner.get(model, id);
        let slow = {
            let mut p = self.plan.lock().unwrap();
            match &p.slow {
                Some((m, _, _)) if m == model => p.slow.take(),
                _ => None,
            }
        };
        if let Some((_, delay, text)) = slow {
            std::thread::sleep(delay);
            return Err(text);
        }
        {
            let mut p = self.plan.lock().unwrap();
            if p.park_next.as_deref() == Some(model) {
                p.park_next = None;
                p.parked += 1;
                let ticket = p.released;
                while p.released == ticket {
                    p = self.cv.wait(p).unwrap();
                }
                p.parked -= 1;
                if p.panic_after_park {
                    p.panic_after_park = false;
                    drop(p);
                    panic!("the store double panics after it was parked");
                }
            }
            if let Some(text) = p.fail.get(model) {
                return Err(text.clone());
            }
        }
        found
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
        self.inner.update(model, id, patch)
    }
    fn count(&self, model: &str) -> Result<usize> {
        self.inner.count(model)
    }
    fn delete(&self, model: &str, id: &str) -> Result<bool> {
        self.inner.delete(model, id)
    }
    fn apply_checked(&self, ops: &[Op]) -> std::result::Result<(), ApplyError> {
        if let Some(text) = self.plan.lock().unwrap().refuse_apply.take() {
            return Err(ApplyError::Storage(text));
        }
        self.batches.lock().unwrap().push(ops.to_vec());
        self.inner.apply_checked(ops)
    }
    fn apply_ctx(&self, _ctx: &WriteCtx, ops: &[Op]) -> std::result::Result<(), ApplyError> {
        self.apply_checked(ops)
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

/// A JSON string value.
pub fn s(text: &str) -> J {
    J::String(text.to_string())
}
