//! A store double for the reader tests: a memory store that counts reads per model and can be told
//! to fail, to park a read (or the next atomic batch) until the test lets it go, or to panic. A read
//! that is parked has already read the store, so what it returns is the state from before the test
//! changed something while it was parked.
//!
//! It also keeps what a test of the lock needs: which cross-process locks each thread held when it
//! read a model or sent a batch, and how often each lock was asked for.

use obop_storage::fault::{self, Fault};
use obop_storage::{
    ApplyError, Backend, Fields, LockGuard, Op, Result, Row, RowSink, Swap, WriteCtx,
};
use serde_json::Value as J;
use std::collections::BTreeMap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, ThreadId};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Plan {
    /// Every `get` of these models fails with the text after it has been counted (and parked).
    fail: BTreeMap<String, String>,
    /// The next `get` of this model parks after it has read the store.
    park_next: Option<String>,
    /// The next atomic batch parks before it is written.
    park_apply: bool,
    /// Callers parked right now.
    parked: usize,
    /// Set when the parked callers may go.
    released: u64,
    /// The next `get` of this model panics.
    panic_next: Option<String>,
    /// The parked read panics when it is let go.
    panic_after_park: bool,
    /// The next atomic batch is refused with this text.
    refuse_apply: Option<String>,
    /// A failure the double reports is also noted on the thread, as the pool does.
    note_faults: bool,
}

/// The keys of the locks the threads hold: `(thread, key)`.
type Held = Arc<Mutex<Vec<(ThreadId, String)>>>;

/// What a lock of the double is while it is held: it forgets the key when it is dropped, and then
/// the lock of the store under it is let go.
struct HeldLock {
    held: Held,
    owner: ThreadId,
    key: String,
    _inner: LockGuard,
}

impl Drop for HeldLock {
    fn drop(&mut self) {
        let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(i) = held
            .iter()
            .position(|(t, k)| *t == self.owner && *k == self.key)
        {
            held.remove(i);
        }
    }
}

pub struct Spy {
    inner: Arc<dyn Backend>,
    gets: Mutex<BTreeMap<String, usize>>,
    batches: Mutex<Vec<Vec<Op>>>,
    plan: Mutex<Plan>,
    cv: Condvar,
    held: Held,
    attempts: Mutex<BTreeMap<String, usize>>,
    /// For every `get`: the model and the keys the calling thread held.
    reads_under: Mutex<Vec<(String, Vec<String>)>>,
    /// For every atomic batch: the keys the calling thread held.
    batches_under: Mutex<Vec<Vec<String>>>,
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
            held: Arc::new(Mutex::new(Vec::new())),
            attempts: Mutex::new(BTreeMap::new()),
            reads_under: Mutex::new(Vec::new()),
            batches_under: Mutex::new(Vec::new()),
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

    /// A read that fails also notes the fault on the thread that made it, as the pool of a real
    /// store does, when its text starts with a code word of D74.
    pub fn note_faults(&self) {
        self.plan.lock().unwrap().note_faults = true;
    }

    /// The next read of `model` parks, after it has read the store, until [`Spy::release`].
    pub fn park_next(&self, model: &str) {
        self.plan.lock().unwrap().park_next = Some(model.to_string());
    }

    /// The next atomic batch parks, before it is written, until [`Spy::release`]. The thread that
    /// sent it holds whatever locks it took.
    pub fn park_next_apply(&self) {
        self.plan.lock().unwrap().park_apply = true;
    }

    /// Lets every parked read and batch go.
    pub fn release(&self) {
        let mut p = self.plan.lock().unwrap();
        p.released += 1;
        self.cv.notify_all();
    }

    /// Waits until `n` reads or batches are parked; panics after ten seconds.
    pub fn wait_parked(&self, n: usize) {
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            if self.plan.lock().unwrap().parked >= n {
                return;
            }
            assert!(Instant::now() < until, "nothing parked");
            thread::sleep(Duration::from_millis(2));
        }
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

    /// How often the lock `key` was asked for, whether it was got at once or waited for.
    pub fn lock_attempts(&self, key: &str) -> usize {
        self.attempts.lock().unwrap().get(key).copied().unwrap_or(0)
    }

    /// Waits until the lock `key` has been asked for `n` times; panics after ten seconds.
    pub fn wait_lock_attempts(&self, key: &str, n: usize) {
        let until = Instant::now() + Duration::from_secs(10);
        while self.lock_attempts(key) < n {
            assert!(Instant::now() < until, "the lock {key} was not asked for");
            thread::sleep(Duration::from_millis(1));
        }
    }

    /// The keys the calling thread holds right now.
    pub fn held_by_this_thread(&self) -> Vec<String> {
        let me = thread::current().id();
        self.held
            .lock()
            .unwrap()
            .iter()
            .filter(|(t, _)| *t == me)
            .map(|(_, k)| k.clone())
            .collect()
    }

    /// For each read of `model`, in order: the keys the reading thread held.
    pub fn locks_at_reads(&self, model: &str) -> Vec<Vec<String>> {
        self.reads_under
            .lock()
            .unwrap()
            .iter()
            .filter(|(m, _)| m == model)
            .map(|(_, k)| k.clone())
            .collect()
    }

    /// For each atomic batch, in order: the keys the sending thread held.
    pub fn locks_at_batches(&self) -> Vec<Vec<String>> {
        self.batches_under.lock().unwrap().clone()
    }

    /// Blocks the calling thread, which has `p`, until [`Spy::release`].
    fn park<'a>(&'a self, mut p: MutexGuard<'a, Plan>) -> MutexGuard<'a, Plan> {
        p.parked += 1;
        let ticket = p.released;
        while p.released == ticket {
            p = self.cv.wait(p).unwrap();
        }
        p.parked -= 1;
        p
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
        self.reads_under
            .lock()
            .unwrap()
            .push((model.to_string(), self.held_by_this_thread()));
        {
            let mut p = self.plan.lock().unwrap();
            if p.panic_next.as_deref() == Some(model) {
                p.panic_next = None;
                drop(p);
                panic!("the store double panics on a read of {model}");
            }
        }
        let found = self.inner.get(model, id);
        {
            let mut p = self.plan.lock().unwrap();
            if p.park_next.as_deref() == Some(model) {
                p.park_next = None;
                p = self.park(p);
                if p.panic_after_park {
                    p.panic_after_park = false;
                    drop(p);
                    panic!("the store double panics after it was parked");
                }
            }
            if let Some(text) = p.fail.get(model) {
                if p.note_faults
                    && let Some(f) = Fault::of_message(text)
                {
                    fault::note(f);
                }
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
        self.batches_under
            .lock()
            .unwrap()
            .push(self.held_by_this_thread());
        {
            let mut p = self.plan.lock().unwrap();
            if p.park_apply {
                p.park_apply = false;
                drop(self.park(p));
            }
        }
        self.inner.apply_checked(ops)
    }
    fn apply_ctx(&self, _ctx: &WriteCtx, ops: &[Op]) -> std::result::Result<(), ApplyError> {
        self.apply_checked(ops)
    }
    fn kind(&self) -> &'static str {
        self.inner.kind()
    }
    fn lock(&self, key: &str) -> Result<LockGuard> {
        *self
            .attempts
            .lock()
            .unwrap()
            .entry(key.to_string())
            .or_insert(0) += 1;
        let inner = self.inner.lock(key)?;
        let owner = thread::current().id();
        self.held.lock().unwrap().push((owner, key.to_string()));
        Ok(Box::new(HeldLock {
            held: self.held.clone(),
            owner,
            key: key.to_string(),
            _inner: inner,
        }))
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
