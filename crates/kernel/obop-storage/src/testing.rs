//! A backend that can be told to refuse calls the way a full pool does (D74), for tests of the
//! layers above the store: does a caller that sees `storage_busy` change nothing, does a loop
//! that gets it try again later. Not for production use.

use crate::fault::{self, Fault};
use crate::{ApplyError, Backend, Fields, J, LockGuard, Op, Result, Row, RowSink, Swap, guarded};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU8, Ordering::SeqCst};

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

/// For the process tests of the binary: `OBOP_TEST_FAULTS=apply:SKIP[:N]` lets the first `SKIP`
/// atomic batches of the process through and refuses the next `N` (default 1) as `storage_busy`,
/// behind the request guard, as the PostgreSQL pool would. Without the variable the backend is
/// returned as it is. Only built with the `test-support` feature, which a release build never
/// enables; a text that is not of this form stops the process, so a test cannot run unarmed.
pub fn faults_from_env(inner: Arc<dyn Backend>) -> Arc<dyn Backend> {
    let Ok(spec) = std::env::var("OBOP_TEST_FAULTS") else {
        return inner;
    };
    let mut parts = spec.split(':');
    let number = |p: Option<&str>| p.and_then(|n| n.parse::<i64>().ok());
    let flaky = Flaky::new(inner);
    match (parts.next(), number(parts.next())) {
        (Some("apply"), Some(skip)) => {
            flaky.refuse_applies_after(skip, number(parts.next()).unwrap_or(1));
        }
        _ => panic!("OBOP_TEST_FAULTS must be apply:SKIP[:N], got {spec:?}"),
    }
    guarded(flaky)
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
    kv_put_budget: AtomicI64,
    /// -1: off; n: calls to let through before one is refused.
    once: AtomicI64,
    /// Calls still to refuse after the first one of a run (see refuse_calls_after).
    run_left: AtomicI64,
    run_len: AtomicI64,
    /// The key the next `kv_get` finds gone: another submit gave it back just before.
    vanish: AtomicBool,
    /// -1: off; n: updates to let through before the acknowledgement of one is lost.
    lose_ack: AtomicI64,
    /// Run once, right after the next successful insert into the model (another process doing
    /// its part between two steps of this one).
    after_insert: std::sync::Mutex<Option<(String, AfterInsert)>>,
    /// Run once, right after a successful `get` of the model, before its answer is returned: the
    /// state the reader holds is old by the time it uses it. `i64` is the gets of the model to let
    /// pass first.
    after_get: std::sync::Mutex<Option<(String, i64, AfterInsert)>>,
    /// Inserts into the model fail with this server error (no fault noted: it is not a refusal of
    /// the pool but a statement that failed, such as a full disk) until `allow_models`.
    insert_error: std::sync::Mutex<Option<(String, String)>>,
}

/// What [`Flaky::after_the_next_insert_into`] runs, with the wrapped backend.
pub type AfterInsert = Box<dyn FnOnce(&dyn Backend) + Send>;

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
            kv_put_budget: AtomicI64::new(0),
            once: AtomicI64::new(-1),
            run_left: AtomicI64::new(0),
            run_len: AtomicI64::new(0),
            vanish: AtomicBool::new(false),
            lose_ack: AtomicI64::new(-1),
            after_insert: std::sync::Mutex::new(None),
            after_get: std::sync::Mutex::new(None),
            insert_error: std::sync::Mutex::new(None),
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
        *self.after_get.lock().unwrap() = None;
        *self.insert_error.lock().unwrap() = None;
        self.after_write.store(0, SeqCst);
        self.after_budget.store(0, SeqCst);
        self.next.store(0, SeqCst);
        self.apply_budget.store(0, SeqCst);
        self.apply_skip.store(0, SeqCst);
        self.kv_new_budget.store(0, SeqCst);
        self.kv_put_budget.store(0, SeqCst);
        self.once.store(-1, SeqCst);
        self.run_left.store(0, SeqCst);
        self.run_len.store(0, SeqCst);
        self.vanish.store(false, SeqCst);
        self.lose_ack.store(-1, SeqCst);
    }

    /// Run `then` once, right after the next successful insert into `model`.
    pub fn after_the_next_insert_into(&self, model: &str, then: AfterInsert) {
        *self.after_insert.lock().unwrap() = Some((model.to_string(), then));
    }

    /// Fail every insert into `model` with `message`, a server error that is neither a duplicate
    /// nor a refusal of the pool (`could not extend file: No space left on device`).
    pub fn fail_inserts_into(&self, model: &str, message: &str) {
        *self.insert_error.lock().unwrap() = Some((model.to_string(), message.to_string()));
    }

    /// Let `skip` successful `get`s of `model` through, then run `then` right after the next one,
    /// before its answer is returned: another request doing its part between the read and the
    /// next step of this one, with no thread and no sleep (the answer the reader gets is the state
    /// from before `then` ran).
    pub fn after_the_get_of(&self, model: &str, skip: i64, then: AfterInsert) {
        *self.after_get.lock().unwrap() = Some((model.to_string(), skip, then));
    }

    /// Let `skip` updates through, then make the next one commit and answer `storage_unavailable`:
    /// the connection broke after the server committed and before the acknowledgement arrived, so
    /// the outcome of the change is unknown to the caller.
    pub fn lose_the_acknowledgement_of_an_update_after(&self, skip: i64) {
        self.lose_ack.store(skip, SeqCst);
    }

    /// Delete the key that the next `kv_get` asks for just before it is read: the submit that held
    /// it gave it back between a failed claim and the read of it.
    pub fn release_key_before_the_next_kv_get(&self) {
        self.vanish.store(true, SeqCst);
    }

    /// Refuse the next `n` `kv_put` calls (short-lived values: plans, sessions) and nothing else.
    pub fn refuse_kv_put(&self, n: i64) {
        self.kv_put_budget.store(n, SeqCst);
    }

    /// Let `skip` calls of any kind through, then refuse the one after them, once: a refusal at
    /// one chosen position of a request (sweep `skip` to put it at every position in turn).
    pub fn refuse_call_after(&self, skip: i64) {
        self.once.store(skip, SeqCst);
        self.run_len.store(0, SeqCst);
    }

    /// Let `skip` calls of any kind through, then refuse the next `n`: a store that stays refused,
    /// from one chosen position of a request on, for longer than a step is tried again.
    pub fn refuse_calls_after(&self, skip: i64, n: i64) {
        self.once.store(skip, SeqCst);
        self.run_len.store(n.max(1) - 1, SeqCst);
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
        self.refuse_applies_after(skip, 1);
    }

    /// Let `skip` atomic batches through, then refuse the next `n`: a store that stays refused
    /// from one chosen step of a request on, longer than the step is tried again.
    pub fn refuse_applies_after(&self, skip: i64, n: i64) {
        self.apply_skip.store(skip, SeqCst);
        self.apply_budget.store(n, SeqCst);
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
        // `refuse_call_after`: let `n` calls through, then refuse the next one, once.
        let once = self.once.try_update(SeqCst, SeqCst, |n| match n {
            n if n > 0 => Some(n - 1),
            0 => Some(-1),
            _ => None,
        });
        if once == Ok(0) {
            self.run_left.store(self.run_len.load(SeqCst), SeqCst);
            self.refused.fetch_add(1, SeqCst);
            fault::note(Fault::Busy);
            return Err(Fault::Busy.message("all database connections are in use, try again"));
        }
        if self
            .run_left
            .try_update(SeqCst, SeqCst, |n| (n > 0).then_some(n - 1))
            .is_ok()
        {
            self.refused.fetch_add(1, SeqCst);
            fault::note(Fault::Busy);
            return Err(Fault::Busy.message("all database connections are in use, try again"));
        }
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
        if let Some((m, message)) = &*self.insert_error.lock().unwrap()
            && m == model
        {
            return Err(message.clone());
        }
        self.inner.insert(model, id, fields)?;
        let hook = {
            let mut g = self.after_insert.lock().unwrap();
            match &*g {
                Some((m, _)) if m == model => g.take().map(|(_, f)| f),
                _ => None,
            }
        };
        if let Some(f) = hook {
            f(&*self.inner);
        }
        Ok(())
    }
    fn get(&self, model: &str, id: &str) -> Result<Option<Fields>> {
        self.gate_m(false, model)?;
        let found = self.inner.get(model, id)?;
        let hook = {
            let mut g = self.after_get.lock().unwrap();
            match g.as_mut() {
                Some((m, skip, _)) if m == model => {
                    if *skip > 0 {
                        *skip -= 1;
                        None
                    } else {
                        g.take().map(|(_, _, f)| f)
                    }
                }
                _ => None,
            }
        };
        if let Some(f) = hook {
            f(&*self.inner);
        }
        Ok(found)
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
        let lost = self.lose_ack.try_update(SeqCst, SeqCst, |n| match n {
            n if n > 0 => Some(n - 1),
            0 => Some(-1),
            _ => None,
        });
        let done = self.inner.update(model, id, patch)?;
        if lost == Ok(0) {
            self.refused.fetch_add(1, SeqCst);
            fault::note_in_flight();
            return Err(
                Fault::Unavailable.message("the connection broke while the change was in flight")
            );
        }
        Ok(done)
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
        let hit = self
            .kv_put_budget
            .try_update(SeqCst, SeqCst, |n| (n > 0).then_some(n - 1))
            .is_ok();
        if hit {
            self.refused.fetch_add(1, SeqCst);
            fault::note(Fault::Busy);
            return Err(Fault::Busy.message("all database connections are in use, try again"));
        }
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
        if self.vanish.swap(false, SeqCst) {
            let _ = self.inner.kv_delete(key);
        }
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

// ---- a scratch PostgreSQL database for one test (wave 2, G2) ---------------------------------

/// A database of one test's own on the server `OBOP_TEST_POSTGRES_URL` names, with optional login
/// roles, all dropped when it goes out of scope.
///
/// Why a database per test: the tests of this crate and of others run side by side against one
/// server, and several of them build indexes, fill tables, change roles and grants or count the
/// records of the whole database. Sharing one database makes a statement wait for another test's
/// lock or a count come out wrong; a database of its own cannot be disturbed by a neighbour and
/// is dropped (sessions included, `WITH (FORCE)`) when the value is dropped, also when the test
/// fails.
///
/// ```ignore
/// let Some(db) = obop_storage::testing::TestDb::new() else { return }; // no server: skipped
/// let backend = obop_storage::open(db.url())?;
/// ```
///
/// Rules for a test that ends sessions or processes. Ending the sessions of the scratch database
/// is fine ([`TestDb::terminate_sessions`] touches this database only). A test that stops or
/// kills the SERVER, or kills a process that holds connections to it, must not run against the
/// shared server: it starts a container of its own (see `pg_pool_tests.rs`, which does it for
/// the tests that stop and start PostgreSQL), because every other test running against the
/// shared server loses its connections too.
pub struct TestDb {
    admin_url: String,
    name: String,
    url: String,
    roles: Vec<String>,
}

fn unique_name() -> String {
    use std::sync::atomic::AtomicUsize;
    static N: AtomicUsize = AtomicUsize::new(0);
    // The time makes a name that an earlier, killed run of the same process id left behind
    // unlikely to clash.
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() % 1_000_000_000);
    format!(
        "obop_t{}_{}_{t}",
        std::process::id(),
        N.fetch_add(1, SeqCst)
    )
}

/// `base` with another database name, and with the login `user:password` instead of its own when
/// one is given. Anything after `?` (`sslmode` and the like) is kept.
fn address(base: &str, db: &str, login: Option<(&str, &str)>) -> Result<String> {
    let (scheme, rest) = base
        .split_once("://")
        .ok_or_else(|| "not a database address".to_string())?;
    let (head, query) = match rest.split_once('?') {
        Some((h, q)) => (h, format!("?{q}")),
        None => (rest, String::new()),
    };
    let authority = head.split_once('/').map_or(head, |(a, _)| a);
    let (userinfo, host) = match authority.rsplit_once('@') {
        Some((u, h)) => (Some(u.to_string()), h),
        None => (None, authority),
    };
    let userinfo = login
        .map(|(u, p)| format!("{u}:{p}"))
        .or(userinfo)
        .map(|u| format!("{u}@"))
        .unwrap_or_default();
    Ok(format!("{scheme}://{userinfo}{host}/{db}{query}"))
}

impl TestDb {
    /// A scratch database on the server named by `OBOP_TEST_POSTGRES_URL`, or `None` (saying so on
    /// stderr) when the variable is not set, so that the test returns early.
    pub fn new() -> Option<TestDb> {
        match std::env::var("OBOP_TEST_POSTGRES_URL") {
            Ok(base) => Some(Self::create(&base).expect("cannot create the scratch database")),
            Err(_) => {
                eprintln!("skipped: set OBOP_TEST_POSTGRES_URL to run against PostgreSQL");
                None
            }
        }
    }

    /// A scratch database on the server `admin_url` reaches (the login must be allowed to create
    /// databases).
    pub fn create(admin_url: &str) -> Result<TestDb> {
        let name = unique_name();
        let mut c = crate::tls::connect(admin_url)?;
        c.batch_execute(&format!("CREATE DATABASE {name}"))
            .map_err(|e| e.to_string())?;
        Ok(TestDb {
            admin_url: admin_url.to_string(),
            url: address(admin_url, &name, None)?,
            name,
            roles: Vec::new(),
        })
    }

    /// The address of the scratch database, as the login of `OBOP_TEST_POSTGRES_URL`.
    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The name [`TestDb::role`] gives the role it creates for `name`.
    pub fn role_name(&self, name: &str) -> String {
        format!("{}_{name}", self.name)
    }

    /// The address of the server as given (its maintenance database).
    pub fn admin_url(&self) -> &str {
        &self.admin_url
    }

    /// A connection to the scratch database as the login of `OBOP_TEST_POSTGRES_URL`.
    pub fn connect(&self) -> postgres::Client {
        crate::tls::connect(&self.url).expect("cannot connect to the scratch database")
    }

    /// Create a login role of this database's own and return the address that logs in as it. The
    /// role name is made unique per database (`name` is a suffix of it); `attributes` is what
    /// follows `CREATE ROLE ... LOGIN` (`NOSUPERUSER`, `CREATEDB` and so on). The role can connect
    /// and nothing more: grants are the test's business. It is dropped with the database.
    pub fn role(&mut self, name: &str, attributes: &str) -> Result<String> {
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
            return Err(format!(
                "{name:?} is not a role name for a test (a-z and _)"
            ));
        }
        let role = self.role_name(name);
        // A throwaway password for a role that lives as long as one test.
        let password = format!("pw{}", self.name.replace('_', ""));
        let mut c = crate::tls::connect(&self.admin_url)?;
        c.batch_execute(&format!(
            "CREATE ROLE {role} LOGIN PASSWORD '{password}' {attributes}"
        ))
        .map_err(|e| e.to_string())?;
        self.roles.push(role.clone());
        c.batch_execute(&format!(
            "GRANT CONNECT ON DATABASE {} TO {role}",
            self.name
        ))
        .map_err(|e| e.to_string())?;
        address(&self.admin_url, &self.name, Some((&role, &password)))
    }

    /// End every session on the scratch database, as a restart of the server would for it.
    /// Sessions of other databases are not touched.
    pub fn terminate_sessions(&self) {
        let mut c = crate::tls::connect(&self.admin_url).expect("cannot connect to the server");
        c.execute(
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity \
             WHERE datname = $1 AND pid <> pg_backend_pid()",
            &[&self.name],
        )
        .expect("cannot end the sessions");
        // The server ends a session a moment after the signal: wait until they are gone.
        for _ in 0..500 {
            let n: i64 = c
                .query_one(
                    "SELECT count(*) FROM pg_stat_activity WHERE datname = $1",
                    &[&self.name],
                )
                .expect("cannot count the sessions")
                .get(0);
            if n == 0 {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the sessions of {} did not end", self.name);
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        if let Ok(mut c) = crate::tls::connect(&self.admin_url) {
            let _ = c.batch_execute(&format!(
                "DROP DATABASE IF EXISTS {} WITH (FORCE)",
                self.name
            ));
            for r in &self.roles {
                let _ = c.batch_execute(&format!("DROP ROLE IF EXISTS {r}"));
            }
        }
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

#[cfg(test)]
mod testdb_tests {
    use super::*;

    #[test]
    fn the_address_gets_another_database_and_login_and_keeps_the_rest() {
        assert_eq!(
            address("postgres://u:p@h:5432/postgres", "scratch", None).unwrap(),
            "postgres://u:p@h:5432/scratch"
        );
        assert_eq!(
            address("postgresql://h/x?sslmode=disable", "scratch", None).unwrap(),
            "postgresql://h/scratch?sslmode=disable"
        );
        assert_eq!(
            address(
                "postgres://u:p@[::1]:5433/x?a=b&c=d",
                "s",
                Some(("rt", "pw"))
            )
            .unwrap(),
            "postgres://rt:pw@[::1]:5433/s?a=b&c=d"
        );
        assert_eq!(
            address("postgres://h:1", "s", Some(("rt", "pw"))).unwrap(),
            "postgres://rt:pw@h:1/s"
        );
        assert!(address("h:1/x", "s", None).is_err());
    }

    fn exists(admin: &mut postgres::Client, what: &str, name: &str) -> bool {
        let sql = match what {
            "database" => "SELECT count(*) FROM pg_database WHERE datname = $1",
            _ => "SELECT count(*) FROM pg_roles WHERE rolname = $1",
        };
        admin.query_one(sql, &[&name]).unwrap().get::<_, i64>(0) == 1
    }

    #[test]
    fn a_scratch_database_is_private_usable_and_gone_afterwards() {
        let Some(mut db) = TestDb::new() else {
            return;
        };
        let other = TestDb::new().unwrap();
        assert_ne!(
            db.name(),
            other.name(),
            "every test gets a database of its own"
        );
        let mut admin = crate::tls::connect(db.admin_url()).unwrap();
        assert!(exists(&mut admin, "database", db.name()));
        // It is a working database: the store opens on it and the neighbour does not see the row.
        let b = crate::open(db.url()).unwrap();
        b.insert("t", "a", &Fields::new()).unwrap();
        let c = crate::open(other.url()).unwrap();
        assert!(c.get("t", "a").unwrap().is_none());
        // A role of its own: it can log in, it owns nothing, and it is dropped with the database.
        let rt_url = db
            .role("rt", "NOSUPERUSER NOCREATEDB NOCREATEROLE")
            .unwrap();
        let mut rt = crate::tls::connect(&rt_url).unwrap();
        let who: String = rt
            .query_one("SELECT current_user::text", &[])
            .unwrap()
            .get(0);
        assert_eq!(who, format!("{}_rt", db.name()));
        assert!(db.role("Bad Name", "").is_err());
        drop(rt);
        let (name, role) = (db.name().to_string(), who);
        assert!(exists(&mut admin, "role", &role));
        drop(b);
        drop(db);
        assert!(
            !exists(&mut admin, "database", &name),
            "dropped with its sessions"
        );
        assert!(!exists(&mut admin, "role", &role));
        assert!(
            exists(&mut admin, "database", other.name()),
            "the neighbour is untouched"
        );
    }

    #[test]
    fn ending_the_sessions_of_one_scratch_database_leaves_the_neighbour_alone() {
        let Some(db) = TestDb::new() else {
            return;
        };
        let other = TestDb::new().unwrap();
        let mut mine = db.connect();
        let mut theirs = other.connect();
        let pid: i32 = mine
            .query_one("SELECT pg_backend_pid()", &[])
            .unwrap()
            .get(0);
        db.terminate_sessions();
        assert!(
            mine.query_one("SELECT 1", &[]).is_err(),
            "pid {pid} was ended"
        );
        assert_eq!(
            theirs
                .query_one("SELECT 1::int", &[])
                .unwrap()
                .get::<_, i32>(0),
            1
        );
    }
}
