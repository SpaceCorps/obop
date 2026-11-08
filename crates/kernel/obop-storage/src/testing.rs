//! A backend that can be told to refuse calls the way a full pool does (D74), for tests of the
//! layers above the store: does a caller that sees `storage_busy` change nothing, does a loop
//! that gets it try again later. Not for production use.

use crate::fault::{self, Fault};
use crate::{
    ApplyError, Backend, Backfilled, ChangePage, ChangeQuery, ChangeRow, Fields, HistoryQuery,
    HistoryStats, J, LockGuard, Op, PartitionEnsured, Result, Row, RowSink, Sealer, Swap, Track,
    Tracking, WriteCtx, guarded,
};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU8, Ordering::SeqCst};

/// A time to wait for something in a test, in seconds, scaled by `OBOP_TEST_TIME_FACTOR` (a
/// number, default 1) so that a slow or shared runner can be given more without editing tests.
/// Tests wait for a condition up to this long; they do not sleep for it.
pub fn time_budget(secs: u64) -> std::time::Duration {
    let factor = std::env::var("OBOP_TEST_TIME_FACTOR")
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|f| f.is_finite() && *f > 0.0)
        .unwrap_or(1.0);
    std::time::Duration::from_secs_f64(secs as f64 * factor)
}

// ---- a path in the temporary directory that belongs to one test ----------------------------

/// A file or a directory in the temporary directory that belongs to one test: a SQLite database,
/// or a directory that holds one. Tests used to build such a path from the process id alone and
/// remove it at the end of the test, which went wrong in two ways: a recycled process id reused
/// the database an earlier run had left, so the test saw somebody else's records, and a run that
/// failed or was interrupted left the files behind for good (about 1,700 of them in a shared
/// temporary directory).
///
/// - [`TempPath::new`] and [`TempPath::dir`] make a name that no other test, process or earlier
///   run has used (the tag, the process id, a counter and the time).
/// - Whatever is at the path is removed when the value is made ([`TempPath::named`] does the
///   same for a fixed name) and again when it is dropped, a failing test included: the path, the
///   `-wal` and `-shm` files of SQLite, the blob directory (`.blobs`) and the lock files
///   (`.lock-*`) that the store keeps beside the database. A file that cannot be removed (a handle
///   still open) is a leftover and nothing more.
/// - On Windows a file that something still holds open cannot be removed, and an `App` of the
///   binary crate is never dropped (the dispatcher and the connectors hold each other), so the
///   database of a test that builds one stays until the process ends. [`TempPath::new`] therefore
///   also removes what earlier runs of the same tag left behind more than an hour ago (only names
///   in the form it makes itself, `<tag>-<pid>-<n>-<millis>` and its side files), so the leftovers
///   of the last hour are all that can pile up. On Linux the files go at once.
///
/// Declare it before the store: locals are dropped in reverse order, so the store is closed first.
pub struct TempPath {
    path: std::path::PathBuf,
}

impl TempPath {
    /// A path that does not exist yet, with a name of its own: `<tag>-<pid>-<n>-<millis>`.
    pub fn new(tag: &str) -> TempPath {
        use std::sync::atomic::AtomicUsize;
        static N: AtomicUsize = AtomicUsize::new(0);
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        sweep_old(tag);
        Self::named(&format!(
            "{tag}-{}-{}-{t}",
            std::process::id(),
            N.fetch_add(1, SeqCst)
        ))
    }

    /// A directory with a name of its own, made.
    pub fn dir(tag: &str) -> TempPath {
        let p = Self::new(tag);
        std::fs::create_dir_all(&p.path).expect("a temporary directory");
        p
    }

    /// The path for a fixed file name in the temporary directory, cleared of whatever an earlier
    /// run left there. Prefer [`TempPath::new`]: a fixed name is for a test of this type.
    pub fn named(name: &str) -> TempPath {
        let p = TempPath {
            path: std::env::temp_dir().join(name),
        };
        p.clear();
        p
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// The address of a SQLite database at this path.
    pub fn url(&self) -> String {
        format!("sqlite://{}", self.path.display())
    }

    /// Remove the path and what the store keeps beside it.
    fn clear(&self) {
        let remove = remove_any;
        remove(&self.path);
        let (Some(parent), Some(name)) = (self.path.parent(), self.path.file_name()) else {
            return;
        };
        let name = name.to_string_lossy().to_string();
        let Ok(entries) = std::fs::read_dir(parent) else {
            return;
        };
        for entry in entries.flatten() {
            let other = entry.file_name().to_string_lossy().to_string();
            let beside = other
                .strip_prefix(&name)
                .is_some_and(|rest| rest == "-wal" || rest == "-shm" || rest.starts_with('.'));
            if beside {
                remove(&entry.path());
            }
        }
    }
}

impl Drop for TempPath {
    fn drop(&mut self) {
        self.clear();
    }
}

/// Remove a file or a directory, if it can be removed.
fn remove_any(p: &std::path::Path) {
    if p.is_dir() {
        let _ = std::fs::remove_dir_all(p);
    } else {
        let _ = std::fs::remove_file(p);
    }
}

/// How old a leftover of an earlier run must be before a new path of the same tag removes it.
const LEFTOVER_AGE: std::time::Duration = std::time::Duration::from_secs(3600);

/// Is `name` a path (or a side file of a path) that [`TempPath::new`] makes for `tag`:
/// `<tag>-<pid>-<n>-<millis>`, then nothing, `-wal`, `-shm`, or a `.` suffix? Nothing else is ever
/// swept.
fn is_made_by(tag: &str, name: &str) -> bool {
    let Some(mut rest) = name.strip_prefix(tag).and_then(|r| r.strip_prefix('-')) else {
        return false;
    };
    for i in 0..3 {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return false;
        }
        rest = &rest[digits..];
        if i < 2 {
            match rest.strip_prefix('-') {
                Some(r) => rest = r,
                None => return false,
            }
        }
    }
    rest.is_empty() || rest == "-wal" || rest == "-shm" || rest.starts_with('.')
}

/// Remove what earlier runs of `tag` left in the temporary directory more than [`LEFTOVER_AGE`]
/// ago (a file that something held open when its test ended, or a run that was killed).
fn sweep_old(tag: &str) {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        if !is_made_by(tag, &entry.file_name().to_string_lossy()) {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age >= LEFTOVER_AGE);
        if old {
            remove_any(&entry.path());
        }
    }
}

// ---- which way the lookup of a unique rule went --------------------------------------------

/// What [`Backend::find_unique`] did on this thread: how often it ran its keyed statement, how
/// often it went the way of the default (`find` and the exact rule), the work SQLite did for the
/// keyed statements, and the text of the last keyed statement.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UniqueRuns {
    /// Lookups answered by the statement built from the index text.
    pub keyed: u64,
    /// Lookups answered by `find` and the exact rule (a value without exact text, or a name the
    /// index cannot carry).
    pub fallback: u64,
    /// SQLite only: the virtual machine steps (`SQLITE_STMTSTATUS_VM_STEP`) the keyed statements
    /// took, summed. Work that does not depend on the clock: a statement that reads the whole
    /// model takes a number of steps in proportion to the model, one that uses the index does not.
    pub vm_steps: u64,
    /// The text of the keyed statement that ran last.
    pub last_sql: Option<String>,
}

thread_local! {
    static UNIQUE_RUNS: std::cell::RefCell<UniqueRuns> = std::cell::RefCell::new(UniqueRuns::default());
}

/// The runs of `find_unique` on this thread since it started or since [`reset_unique_runs`].
pub fn unique_runs() -> UniqueRuns {
    UNIQUE_RUNS.with(|r| r.borrow().clone())
}

/// Set the count of [`unique_runs`] for this thread to zero.
pub fn reset_unique_runs() {
    UNIQUE_RUNS.with(|r| *r.borrow_mut() = UniqueRuns::default());
}

pub(crate) fn note_keyed(sql: &str, vm_steps: i32) {
    UNIQUE_RUNS.with(|r| {
        let mut r = r.borrow_mut();
        r.keyed += 1;
        r.vm_steps += u64::try_from(vm_steps).unwrap_or_default();
        r.last_sql = Some(sql.to_string());
    });
}

pub(crate) fn note_fallback() {
    UNIQUE_RUNS.with(|r| r.borrow_mut().fallback += 1);
}

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
    /// The batches that reached `apply_tracked`: see [`Flaky::seen_writes`].
    seen: std::sync::Mutex<Vec<SeenWrite>>,
    /// See [`Flaky::reads`].
    reads: std::sync::Mutex<Reads>,
    /// See [`Flaky::models_asked`].
    asked: std::sync::Mutex<BTreeSet<String>>,
    /// Run once, right before the next atomic batch is applied.
    before_apply: std::sync::Mutex<Option<AfterInsert>>,
    /// Run once, right before the next compare-and-set of a short-lived value is tried.
    before_kv_swap: std::sync::Mutex<Option<AfterInsert>>,
    /// Run once, right before the next read of a short-lived value whose key starts with the
    /// prefix.
    before_kv_get: std::sync::Mutex<Option<(String, AfterInsert)>>,
}

/// A batch as `apply_tracked` received it: the context, the steps and the tracked models.
pub type SeenWrite = (WriteCtx, Vec<Op>, Vec<String>);

/// What the layers above read through a [`Flaky`]: how often each way of reading a model was
/// used and how many records it handed back. A scan of the model by `list`, `scan` or `find`
/// shows as rows; a keyed lookup shows as the few rows it returns. See [`Flaky::reads`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reads {
    /// Calls of `get`.
    pub gets: u64,
    /// Calls of `list` and the records they returned.
    pub lists: u64,
    pub list_rows: u64,
    /// Calls of `scan` and the records visited.
    pub scans: u64,
    pub scan_rows: u64,
    /// Calls of `find` (one field, which the databases answer by reading the model unless an
    /// index happens to serve it) and the records returned.
    pub finds: u64,
    pub find_rows: u64,
    /// Calls of `find_unique` (the lookup that uses the unique index of a rule) and the records
    /// returned.
    pub keyed: u64,
    pub keyed_rows: u64,
    /// Calls of `count`.
    pub counts: u64,
    /// Calls that read a short-lived value or a counter (`kv_get`, `kv_scan`, `counter_get`,
    /// `counter_ttl_ms`).
    pub kv_reads: u64,
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
            seen: std::sync::Mutex::new(Vec::new()),
            reads: std::sync::Mutex::new(Reads::default()),
            asked: std::sync::Mutex::new(BTreeSet::new()),
            before_apply: std::sync::Mutex::new(None),
            before_kv_swap: std::sync::Mutex::new(None),
            before_kv_get: std::sync::Mutex::new(None),
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

    /// The batches that reached `apply_tracked` (the end of `Tracked::apply_ctx`, whoever sent
    /// them, refused ones included once the gate let them through), oldest first: for a test of
    /// the layers above that asks "did the write carry the context I expect?".
    pub fn seen_writes(&self) -> Vec<SeenWrite> {
        self.seen.lock().unwrap().clone()
    }

    /// What has been read through this backend since it was made or since [`Flaky::reset_reads`].
    pub fn reads(&self) -> Reads {
        *self.reads.lock().unwrap()
    }

    /// Start the count of [`Flaky::reads`] again from zero.
    pub fn reset_reads(&self) {
        *self.reads.lock().unwrap() = Reads::default();
    }

    /// Every model a call through this backend has named, read or written, refused calls
    /// included, since it was made: a record of the tables the program asks the storage for, for
    /// the test that holds that list to what the installer reserves. Counters and short-lived
    /// values (`counter_*`, `kv_*`) name no model and are not in it; the records of `export` and
    /// `total` are not asked for by model.
    pub fn models_asked(&self) -> BTreeSet<String> {
        self.asked.lock().unwrap().clone()
    }

    fn ask(&self, model: &str) {
        if !model.is_empty() {
            self.asked.lock().unwrap().insert(model.to_string());
        }
    }

    fn ask_ops(&self, ops: &[Op]) {
        for op in ops {
            self.ask(op.model());
        }
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

    /// Run `then` once, with the wrapped backend, right before the next atomic batch (`apply`,
    /// `apply_checked`) is applied: another process committing between the check of a command and
    /// its commit, with no thread and no sleep.
    pub fn before_the_next_apply(&self, then: AfterInsert) {
        *self.before_apply.lock().unwrap() = Some(then);
    }

    /// Run `then` once, with the wrapped backend, right before the next `kv_swap` is tried: another
    /// request changing the value (or the account it rests on) between a read and its
    /// compare-and-set, with no thread and no sleep.
    pub fn before_the_next_kv_swap(&self, then: AfterInsert) {
        *self.before_kv_swap.lock().unwrap() = Some(then);
    }

    /// Run `then` once, with the wrapped backend, right before the next `kv_get` of a key that
    /// starts with `prefix`: another request changing something between two steps of this one.
    pub fn before_the_next_kv_get_of(&self, prefix: &str, then: AfterInsert) {
        *self.before_kv_get.lock().unwrap() = Some((prefix.to_string(), then));
    }

    fn run_before_apply(&self) {
        let hook = self.before_apply.lock().unwrap().take();
        if let Some(f) = hook {
            f(&*self.inner);
        }
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
        self.ask(model);
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
        self.reads.lock().unwrap().gets += 1;
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
        let rows = self.inner.list(model)?;
        let mut r = self.reads.lock().unwrap();
        r.lists += 1;
        r.list_rows += rows.len() as u64;
        Ok(rows)
    }
    fn scan(
        &self,
        model: &str,
        visit: &mut dyn FnMut(&str, &Fields) -> Result<bool>,
    ) -> Result<()> {
        self.gate_m(false, model)?;
        self.reads.lock().unwrap().scans += 1;
        self.inner.scan(model, &mut |id, f| {
            self.reads.lock().unwrap().scan_rows += 1;
            visit(id, f)
        })
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
        self.reads.lock().unwrap().counts += 1;
        self.inner.count(model)
    }
    fn delete(&self, model: &str, id: &str) -> Result<bool> {
        self.gate_u(model)?;
        self.inner.delete(model, id)
    }
    fn insert_many(&self, rows: &[(String, String, Fields)]) -> Result<()> {
        rows.iter().for_each(|(model, _, _)| self.ask(model));
        self.gate(true)?;
        self.inner.insert_many(rows)
    }
    fn apply(&self, ops: &[Op]) -> Result<()> {
        self.ask_ops(ops);
        self.gate_apply()?;
        self.run_before_apply();
        self.inner.apply(ops)
    }
    fn apply_checked(&self, ops: &[Op]) -> std::result::Result<(), ApplyError> {
        self.ask_ops(ops);
        self.gate_apply().map_err(ApplyError::Storage)?;
        self.run_before_apply();
        self.inner.apply_checked(ops)
    }
    fn apply_ctx(&self, ctx: &WriteCtx, ops: &[Op]) -> std::result::Result<(), ApplyError> {
        self.ask_ops(ops);
        self.gate_apply().map_err(ApplyError::Storage)?;
        self.inner.apply_ctx(ctx, ops)
    }
    fn apply_tracked(
        &self,
        ctx: &WriteCtx,
        ops: &[Op],
        tracking: &Tracking,
    ) -> std::result::Result<(), ApplyError> {
        self.ask_ops(ops);
        self.gate_apply().map_err(ApplyError::Storage)?;
        self.seen
            .lock()
            .unwrap()
            .push((ctx.clone(), ops.to_vec(), tracking.models()));
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
    fn set_sealer(&self, sealer: Arc<dyn Sealer>) {
        self.inner.set_sealer(sealer)
    }
    fn sealer(&self) -> Option<Arc<dyn Sealer>> {
        self.inner.sealer()
    }
    fn history(&self, q: &HistoryQuery) -> Result<Vec<ChangeRow>> {
        self.gate_m(false, &q.model)?;
        self.inner.history(q)
    }
    fn changes(&self, q: &ChangeQuery) -> Result<ChangePage> {
        self.gate(false)?;
        self.inner.changes(q)
    }
    fn ensure_history_schema(&self) -> Result<()> {
        self.gate(true)?;
        self.inner.ensure_history_schema()
    }
    fn history_stats(&self) -> Result<HistoryStats> {
        self.gate(false)?;
        self.inner.history_stats()
    }
    fn history_ensure_partition(&self, month: &str) -> Result<PartitionEnsured> {
        self.gate(true)?;
        self.inner.history_ensure_partition(month)
    }
    fn history_drop_partition(&self, month: &str, min_age_days: u32) -> Result<u64> {
        self.gate(true)?;
        self.inner.history_drop_partition(month, min_age_days)
    }
    fn backfill_sysfields(
        &self,
        models: &[String],
        fallback_at: &str,
        batch: usize,
    ) -> Result<Backfilled> {
        models.iter().for_each(|m| self.ask(m));
        self.gate(true)?;
        self.inner.backfill_sysfields(models, fallback_at, batch)
    }
    fn find(&self, model: &str, field: &str, value: &J) -> Result<Vec<(String, Fields)>> {
        self.gate_m(false, model)?;
        let rows = self.inner.find(model, field, value)?;
        let mut r = self.reads.lock().unwrap();
        r.finds += 1;
        r.find_rows += rows.len() as u64;
        Ok(rows)
    }
    fn find_unique(
        &self,
        model: &str,
        fields: &[&str],
        values: &[J],
    ) -> Result<Vec<(String, Fields)>> {
        self.gate_m(false, model)?;
        let rows = self.inner.find_unique(model, fields, values)?;
        let mut r = self.reads.lock().unwrap();
        r.keyed += 1;
        r.keyed_rows += rows.len() as u64;
        Ok(rows)
    }
    fn ensure_unique(&self, name: &str, model: &str, fields: &[&str]) -> Result<()> {
        self.ask(model);
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
        self.reads.lock().unwrap().kv_reads += 1;
        self.inner.counter_get(key)
    }
    fn counter_ttl_ms(&self, key: &str) -> Result<Option<u64>> {
        self.gate(false)?;
        self.reads.lock().unwrap().kv_reads += 1;
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
        let hook = {
            let mut slot = self.before_kv_get.lock().unwrap();
            if slot
                .as_ref()
                .is_some_and(|(p, _)| key.starts_with(p.as_str()))
            {
                slot.take().map(|(_, f)| f)
            } else {
                None
            }
        };
        if let Some(f) = hook {
            f(&*self.inner);
        }
        if self.vanish.swap(false, SeqCst) {
            let _ = self.inner.kv_delete(key);
        }
        self.gate(false)?;
        self.reads.lock().unwrap().kv_reads += 1;
        self.inner.kv_get(key)
    }
    fn kv_delete(&self, key: &str) -> Result<bool> {
        self.gate(true)?;
        self.inner.kv_delete(key)
    }
    fn kv_scan(&self, prefix: &str) -> Result<Vec<(String, String)>> {
        self.gate(false)?;
        self.reads.lock().unwrap().kv_reads += 1;
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
        swaps.iter().for_each(|s| self.ask(&s.model));
        self.gate(true)?;
        self.inner.swap_fields(swaps)
    }
    fn kv_swap(&self, key: &str, old: &str, new: &str) -> Result<bool> {
        let hook = self.before_kv_swap.lock().unwrap().take();
        if let Some(f) = hook {
            f(&*self.inner);
        }
        self.gate(true)?;
        self.inner.kv_swap(key, old, new)
    }
    fn import(
        &self,
        wipe: bool,
        next: &mut dyn FnMut() -> Result<Option<Vec<Row>>>,
    ) -> Result<usize> {
        self.gate(true)?;
        self.inner.import(wipe, &mut || {
            let rows = next()?;
            if let Some(rows) = &rows {
                rows.iter().for_each(|(model, _, _)| self.ask(model));
            }
            Ok(rows)
        })
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

    /// The models a call names are noted for every way of naming one: a keyed call, a batch, a
    /// refused call, a backfill, a unique rule and an import. Counters name none.
    #[test]
    fn the_models_a_call_names_are_noted_whatever_the_way_it_names_them() {
        let (flaky, db) = Flaky::memory();
        assert!(flaky.models_asked().is_empty());
        db.insert("m_insert", "a", &Fields::new()).unwrap();
        db.get("m_get", "a").unwrap();
        db.list("m_list").unwrap();
        db.scan("m_scan", &mut |_, _| Ok(true)).unwrap();
        db.update("m_update", "a", &Fields::new()).unwrap();
        db.count("m_count").unwrap();
        db.delete("m_delete", "a").unwrap();
        db.find("m_find", "f", &json!(1)).unwrap();
        db.find_unique("m_unique", &["f"], &[json!(1)]).unwrap();
        db.insert_many(&[("m_many".into(), "a".into(), Fields::new())])
            .unwrap();
        db.apply(&[Op::Insert {
            model: "m_apply".into(),
            id: "a".into(),
            fields: Fields::new(),
        }])
        .unwrap();
        db.apply_checked(&[Op::add("m_checked", "a", "n", 1).or_create(Fields::new())])
            .unwrap();
        db.apply_ctx(
            &WriteCtx::system("test"),
            &[Op::add("m_ctx", "a", "n", 1).or_create(Fields::new())],
        )
        .unwrap();
        db.ensure_unique("u", "m_rule", &["f"]).unwrap();
        db.backfill_sysfields(
            &["m_backfill".to_string()],
            "2026-01-01T00:00:00.000000Z",
            10,
        )
        .unwrap();
        db.swap_fields(&[Swap {
            model: "m_swap".into(),
            id: "a".into(),
            field: "f".into(),
            old: "x".into(),
            new: "y".into(),
        }])
        .unwrap();
        db.counter_add("not_a_model", 1000, false).unwrap();
        db.kv_put("not_a_model_either", "v", None).unwrap();
        // A refused call is asked for all the same.
        flaky.refuse_next(1);
        assert!(db.get("m_refused", "a").is_err());
        let mut rows = Some(vec![(
            "m_import".to_string(),
            "a".to_string(),
            Fields::new(),
        )]);
        db.import(false, &mut || Ok(rows.take())).unwrap();
        let want: BTreeSet<String> = [
            "m_insert",
            "m_get",
            "m_list",
            "m_scan",
            "m_update",
            "m_count",
            "m_delete",
            "m_find",
            "m_unique",
            "m_many",
            "m_apply",
            "m_checked",
            "m_ctx",
            "m_rule",
            "m_backfill",
            "m_refused",
            "m_swap",
            "m_import",
        ]
        .map(String::from)
        .into();
        assert_eq!(flaky.models_asked(), want);

        // A `Flaky` under a `Tracked` is reached by `apply_tracked`: the models of that batch count.
        let below = Flaky::new(Arc::new(crate::memory::Memory::default()));
        let above = crate::tracked::Tracked::new(below.clone());
        above
            .apply_ctx(
                &WriteCtx::system("test"),
                &[Op::add("m_tracked", "a", "n", 1).or_create(Fields::new())],
            )
            .unwrap();
        assert!(below.models_asked().contains("m_tracked"));
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

#[cfg(test)]
mod temp_path_tests {
    use super::*;

    fn exists(p: &str) -> bool {
        std::path::Path::new(p).exists()
    }

    #[test]
    fn two_paths_with_one_tag_differ_and_neither_exists_yet() {
        let (a, b) = (TempPath::new("obop-tp"), TempPath::new("obop-tp"));
        assert_ne!(a.path(), b.path());
        assert!(!a.path().exists() && !b.path().exists());
        assert!(
            a.url().starts_with("sqlite://") && a.url().ends_with(&*a.path().to_string_lossy())
        );
    }

    #[test]
    fn a_fixed_name_is_cleared_of_what_an_earlier_run_left_and_so_is_everything_beside_it() {
        // The state an interrupted run leaves: the database, its side files, the blob directory
        // and a lock file, all under one fixed name (a recycled process id gave the old tests
        // exactly this).
        let name = format!("obop-tp-stale-{}", std::process::id());
        let base = std::env::temp_dir().join(&name).display().to_string();
        std::fs::write(&base, "stale").unwrap();
        for ext in ["-wal", "-shm", ".lock-obop.migrate"] {
            std::fs::write(format!("{base}{ext}"), "stale").unwrap();
        }
        std::fs::create_dir_all(format!("{base}.blobs")).unwrap();
        std::fs::write(format!("{base}.blobs/x"), "stale").unwrap();
        // A neighbour that merely starts with the same letters is not ours.
        let neighbour = format!("{base}2");
        std::fs::write(&neighbour, "keep").unwrap();

        let p = TempPath::named(&name);
        for ext in ["", "-wal", "-shm", ".lock-obop.migrate", ".blobs"] {
            assert!(!exists(&format!("{base}{ext}")), "{ext} was left");
        }
        assert!(exists(&neighbour), "a different name is untouched");
        drop(p);
        std::fs::remove_file(&neighbour).unwrap();
    }

    #[test]
    fn dropping_removes_the_database_its_side_files_and_the_directory_content() {
        let db = TempPath::new("obop-tp-db");
        let base = db.path().display().to_string();
        // A real SQLite file with its side files, as the store makes them.
        let b = crate::open(&db.url()).unwrap();
        b.insert("m", "1", &Fields::new()).unwrap();
        drop(b.lock("some-key").unwrap());
        assert!(exists(&base), "the database is there");
        assert!(
            exists(&format!("{base}.lock-some-key")),
            "and its lock file"
        );
        drop(b);
        drop(db);
        assert!(!exists(&base) && !exists(&format!("{base}.lock-some-key")));
        assert!(!exists(&format!("{base}-wal")) && !exists(&format!("{base}-shm")));

        let dir = TempPath::dir("obop-tp-dir");
        let inner = dir.path().join("deep/er");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(inner.join("f"), "x").unwrap();
        let shown = dir.path().to_path_buf();
        drop(dir);
        assert!(!shown.exists());
    }

    #[test]
    fn only_the_names_a_path_makes_for_its_tag_are_swept_and_only_when_they_are_old() {
        assert!(is_made_by("obop-tpj", "obop-tpj-12-3-1793696487850"));
        for ext in ["-wal", "-shm", ".blobs", ".lock-obop.migrate"] {
            assert!(
                is_made_by("obop-tpj", &format!("obop-tpj-12-3-9{ext}")),
                "{ext}"
            );
        }
        for no in [
            "obop-tpj",
            "obop-tpj-12-3",
            "obop-tpj-a-3-4",
            "obop-tpj-12-3-4x",
            "obop-tpj-12-3-4-wal2",
            "obop-tpjx-12-3-4",
            "obop-tpj-x-12-3-4",
            "xobop-tpj-12-3-4",
        ] {
            assert!(!is_made_by("obop-tpj", no), "{no}");
        }

        let tmp = std::env::temp_dir();
        let old_time = std::time::SystemTime::now() - 2 * LEFTOVER_AGE;
        let make = |name: &str, old: bool| {
            let p = tmp.join(name);
            std::fs::write(&p, "leftover").unwrap();
            if old {
                std::fs::File::options()
                    .write(true)
                    .open(&p)
                    .unwrap()
                    .set_modified(old_time)
                    .unwrap();
            }
            p
        };
        let pid = std::process::id();
        let stale = make(&format!("obop-tpj-{pid}-7-1"), true);
        let stale_side = make(&format!("obop-tpj-{pid}-7-1-wal"), true);
        let young = make(&format!("obop-tpj-{pid}-8-2"), false);
        let other_tag = make(&format!("obop-tpjx-{pid}-7-1"), true);
        let odd_name = make(&format!("obop-tpj-{pid}-seven"), true);

        let fresh = TempPath::new("obop-tpj");
        assert!(!stale.exists() && !stale_side.exists(), "old leftovers go");
        assert!(young.exists(), "a leftover of the last hour stays");
        assert!(other_tag.exists() && odd_name.exists(), "other names stay");
        drop(fresh);
        for p in [young, other_tag, odd_name] {
            std::fs::remove_file(p).unwrap();
        }
    }

    #[test]
    fn a_failing_test_still_cleans_up() {
        let path = std::sync::Mutex::new(None);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let d = TempPath::dir("obop-tp-panic");
            *path.lock().unwrap() = Some(d.path().to_path_buf());
            std::fs::write(d.path().join("f"), "x").unwrap();
            panic!("the test failed");
        }));
        assert!(r.is_err());
        let p = path.lock().unwrap().clone().unwrap();
        assert!(!p.exists(), "{} was left behind", p.display());
    }
}
