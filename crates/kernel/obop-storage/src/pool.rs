//! A small blocking connection pool (D74), written for this crate instead of taken from one.
//!
//! What it promises, in the order the requirements name them:
//!
//! - **Bounded in size.** At most `max_size` connections exist (open, being opened or checked
//!   out). They are made on demand by a short-lived helper thread, so a slow or hanging connect
//!   never keeps a caller past its acquire timeout.
//! - **Bounded in waiting.** A caller that finds nothing free waits at most `acquire_timeout`
//!   and only if fewer than `max_waiters` callers already wait; otherwise it is refused at once.
//!   Both refusals are [`PoolError::Busy`], which the HTTP layer answers with 503 and
//!   `Retry-After` (see [`crate::fault`]). The queue is therefore bounded in length and in time,
//!   and so is the memory it holds (one parked thread per waiter).
//! - **Lanes.** A *long read* (a streaming scan, an export) takes a permit first; at most
//!   `max_long` of them are out at once, so a handful of heavy reads can never take every
//!   connection from interactive callers.
//! - **Recycling.** Idle connections are closed after `idle_timeout`, every connection after
//!   `max_lifetime`; a background thread sweeps, and a checkout also refuses an expired one.
//! - **Health.** A connection that sat idle for `health_check_idle` or longer is pinged when it is
//!   checked out, and a dead one is dropped and replaced. A connection that failed, or whose
//!   holder panicked, is never put back.
//! - **Honest accounting.** In use, idle, waiting, acquire latency, timeouts and refusals are
//!   counted (see [`stats`]) for `/metrics`.
//!
//! The pool does not know what a connection is: a [`Manager`] makes and tests them. The
//! PostgreSQL side is in `pgpool.rs`; the tests here use a fake manager and need no database.

use crate::fault::{self, Fault};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

/// Makes and checks the connections of one pool.
pub trait Manager: Send + Sync + 'static {
    type Conn: Send + 'static;
    /// Open a connection. Called on a helper thread, never under the pool's lock.
    fn connect(&self) -> Result<Self::Conn, String>;
    /// Is the connection still usable? A cheap round trip that gives up after `timeout`.
    fn ping(&self, conn: &mut Self::Conn, timeout: Duration) -> bool;
}

/// How long a health check may take before the connection is declared dead.
const PING_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone)]
pub struct Settings {
    /// `main`, `lock` or `blob`: the label of the metrics.
    pub name: &'static str,
    pub max_size: usize,
    pub acquire_timeout: Duration,
    pub idle_timeout: Duration,
    pub max_lifetime: Duration,
    /// Connections idle at least this long are pinged on checkout; zero pings on every checkout.
    pub health_check_idle: Duration,
    /// Callers allowed to wait at once; more are refused immediately.
    pub max_waiters: usize,
    /// Long reads allowed out at once (clamped to `1..=max_size`).
    pub max_long: usize,
    /// After a failed connect, no new attempt for this long.
    pub connect_backoff: Duration,
}

impl Settings {
    pub fn new(name: &'static str, max_size: usize) -> Self {
        Self {
            name,
            max_size,
            acquire_timeout: Duration::from_secs(2),
            idle_timeout: Duration::from_secs(300),
            max_lifetime: Duration::from_secs(1800),
            health_check_idle: Duration::from_secs(1),
            max_waiters: 256,
            max_long: (max_size / 2).max(1),
            connect_backoff: Duration::from_millis(200),
        }
    }
}

/// Why a connection could not be handed out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PoolError {
    /// Nothing became free within the acquire timeout.
    Timeout,
    /// Too many callers were already waiting.
    QueueFull,
    /// A connection could not be made (the text is the driver's, without the URL).
    Unavailable(String),
    /// The pool was shut down.
    Closed,
}

impl PoolError {
    pub fn fault(&self) -> Fault {
        match self {
            PoolError::Timeout | PoolError::QueueFull => Fault::Busy,
            PoolError::Unavailable(_) | PoolError::Closed => Fault::Unavailable,
        }
    }

    /// The error text of the storage contract (see [`crate::fault`]).
    pub fn message(&self) -> String {
        match self {
            PoolError::Timeout => {
                Fault::Busy.message("all database connections are in use, try again")
            }
            PoolError::QueueFull => Fault::Busy
                .message("too many requests are waiting for a database connection, try again"),
            PoolError::Unavailable(e) => {
                Fault::Unavailable.message(&format!("cannot reach the database: {e}"))
            }
            PoolError::Closed => Fault::Unavailable.message("the database pool is closed"),
        }
    }
}

impl std::fmt::Display for PoolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

/// Upper bounds of the acquire-latency histogram, in seconds.
pub const ACQUIRE_BUCKETS: [f64; 12] = [
    0.0005, 0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5,
];

/// A reading of one pool, for `/metrics`.
#[derive(Debug, Clone, Default)]
pub struct PoolStats {
    pub name: &'static str,
    pub max_size: usize,
    pub in_use: usize,
    pub idle: usize,
    pub waiting: usize,
    /// Per bucket of [`ACQUIRE_BUCKETS`], not cumulative; slower acquires are in `acquire_count` only.
    pub acquire_buckets: [u64; ACQUIRE_BUCKETS.len()],
    pub acquire_count: u64,
    pub acquire_sum_secs: f64,
    /// Acquires that waited the whole timeout and gave up.
    pub timeouts: u64,
    /// Acquires refused at once because too many callers were waiting.
    pub rejected: u64,
    pub connect_failures: u64,
    pub connections_opened: u64,
    pub connections_closed: u64,
}

impl PoolStats {
    fn merge(&mut self, o: &PoolStats) {
        self.max_size += o.max_size;
        self.in_use += o.in_use;
        self.idle += o.idle;
        self.waiting += o.waiting;
        for (a, b) in self.acquire_buckets.iter_mut().zip(&o.acquire_buckets) {
            *a += b;
        }
        self.acquire_count += o.acquire_count;
        self.acquire_sum_secs += o.acquire_sum_secs;
        self.timeouts += o.timeouts;
        self.rejected += o.rejected;
        self.connect_failures += o.connect_failures;
        self.connections_opened += o.connections_opened;
        self.connections_closed += o.connections_closed;
    }
}

trait Reporter: Send + Sync {
    fn report(&self) -> PoolStats;
}

static REGISTRY: Mutex<Vec<Weak<dyn Reporter>>> = Mutex::new(Vec::new());

/// A reading of every live pool in this process, one entry per name (pools of the same name,
/// which only happens in tests, are added together).
pub fn stats() -> Vec<PoolStats> {
    let mut reg = REGISTRY.lock().unwrap_or_else(|e| e.into_inner());
    reg.retain(|w| w.strong_count() > 0);
    let mut out: Vec<PoolStats> = Vec::new();
    for r in reg.iter().filter_map(Weak::upgrade) {
        let s = r.report();
        match out.iter_mut().find(|o| o.name == s.name) {
            Some(o) => o.merge(&s),
            None => out.push(s),
        }
    }
    out
}

struct Idle<C> {
    conn: C,
    born: Instant,
    since: Instant,
    /// Has it served a checkout yet? (A fresh connection has not.)
    used: bool,
}

struct State<C> {
    /// Free connections, newest last (a checkout takes the newest, so the others age out).
    idle: Vec<Idle<C>>,
    /// Idle + checked out + being opened.
    total: usize,
    out: usize,
    connecting: usize,
    long_out: usize,
    waiting: usize,
    /// Counts failed connects, so a waiter can tell that one happened while it waited.
    connect_epoch: u64,
    last_connect_error: Option<String>,
    backoff_until: Instant,
    closed: bool,
}

#[derive(Default)]
struct Counters {
    acquire_buckets: [AtomicU64; ACQUIRE_BUCKETS.len()],
    acquire_count: AtomicU64,
    acquire_micros: AtomicU64,
    timeouts: AtomicU64,
    rejected: AtomicU64,
    connect_failures: AtomicU64,
    opened: AtomicU64,
    closed: AtomicU64,
}

struct Inner<M: Manager> {
    manager: M,
    cfg: Settings,
    state: Mutex<State<M::Conn>>,
    /// Callers waiting for a connection.
    cv: Condvar,
    /// Callers waiting for a long-read permit.
    cv_long: Condvar,
    counters: Counters,
    /// Wakes the sweeper when the pool goes away.
    shutdown: Arc<(Mutex<bool>, Condvar)>,
}

/// The pool. Cloning shares it.
pub struct Pool<M: Manager> {
    inner: Arc<Inner<M>>,
}

impl<M: Manager> Clone for Pool<M> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<M: Manager> Reporter for Inner<M> {
    fn report(&self) -> PoolStats {
        let st = self.lock();
        let c = &self.counters;
        let mut s = PoolStats {
            name: self.cfg.name,
            max_size: self.cfg.max_size,
            in_use: st.out,
            idle: st.idle.len(),
            waiting: st.waiting,
            acquire_count: c.acquire_count.load(Relaxed),
            acquire_sum_secs: c.acquire_micros.load(Relaxed) as f64 / 1e6,
            timeouts: c.timeouts.load(Relaxed),
            rejected: c.rejected.load(Relaxed),
            connect_failures: c.connect_failures.load(Relaxed),
            connections_opened: c.opened.load(Relaxed),
            connections_closed: c.closed.load(Relaxed),
            ..PoolStats::default()
        };
        for (d, b) in s.acquire_buckets.iter_mut().zip(&c.acquire_buckets) {
            *d = b.load(Relaxed);
        }
        s
    }
}

impl<M: Manager> Inner<M> {
    fn lock(&self) -> MutexGuard<'_, State<M::Conn>> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn observe(&self, took: Duration) {
        let secs = took.as_secs_f64();
        if let Some(i) = ACQUIRE_BUCKETS.iter().position(|b| secs <= *b) {
            self.counters.acquire_buckets[i].fetch_add(1, Relaxed);
        }
        self.counters.acquire_count.fetch_add(1, Relaxed);
        self.counters
            .acquire_micros
            .fetch_add(took.as_micros() as u64, Relaxed);
    }

    fn expired(&self, born: Instant, since: Instant) -> bool {
        born.elapsed() >= self.cfg.max_lifetime || since.elapsed() >= self.cfg.idle_timeout
    }

    /// The oldest idle connections beyond their time, closed. Called by the sweeper.
    fn sweep(&self) {
        let doomed: Vec<Idle<M::Conn>> = {
            let mut st = self.lock();
            let mut keep = Vec::with_capacity(st.idle.len());
            let mut gone = Vec::new();
            for i in st.idle.drain(..) {
                if self.expired(i.born, i.since) {
                    gone.push(i);
                } else {
                    keep.push(i);
                }
            }
            st.idle = keep;
            st.total -= gone.len();
            gone
        };
        if !doomed.is_empty() {
            self.counters.closed.fetch_add(doomed.len() as u64, Relaxed);
            self.cv.notify_one();
        }
        drop(doomed);
    }
}

impl<M: Manager> Pool<M> {
    pub fn new(manager: M, cfg: Settings) -> Self {
        let mut cfg = cfg;
        cfg.max_size = cfg.max_size.max(1);
        cfg.max_long = cfg.max_long.clamp(1, cfg.max_size);
        let shutdown = Arc::new((Mutex::new(false), Condvar::new()));
        let inner = Arc::new(Inner {
            manager,
            cfg,
            state: Mutex::new(State {
                idle: Vec::new(),
                total: 0,
                out: 0,
                connecting: 0,
                long_out: 0,
                waiting: 0,
                connect_epoch: 0,
                last_connect_error: None,
                backoff_until: Instant::now(),
                closed: false,
            }),
            cv: Condvar::new(),
            cv_long: Condvar::new(),
            counters: Counters::default(),
            shutdown: shutdown.clone(),
        });
        let typed: Weak<Inner<M>> = Arc::downgrade(&inner);
        let weak: Weak<dyn Reporter> = typed;
        REGISTRY
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(weak);
        spawn_sweeper(Arc::downgrade(&inner), shutdown);
        Pool { inner }
    }

    pub fn settings(&self) -> &Settings {
        &self.inner.cfg
    }

    /// Put a connection made by the caller into the pool (the first one, opened at start-up to
    /// prepare the schema). Ignored when the pool is already full.
    pub fn seed(&self, conn: M::Conn) {
        let now = Instant::now();
        let mut st = self.inner.lock();
        if st.total < self.inner.cfg.max_size && !st.closed {
            st.total += 1;
            st.idle.push(Idle {
                conn,
                born: now,
                since: now,
                used: false,
            });
            self.inner.counters.opened.fetch_add(1, Relaxed);
        }
    }

    /// Close every idle connection. Called when a connection turned out to be dead: the others
    /// most likely belong to the same server restart or failover.
    pub fn purge_idle(&self) {
        let doomed: Vec<Idle<M::Conn>> = {
            let mut st = self.inner.lock();
            let n = st.idle.len();
            st.total -= n;
            std::mem::take(&mut st.idle)
        };
        self.inner
            .counters
            .closed
            .fetch_add(doomed.len() as u64, Relaxed);
        if !doomed.is_empty() {
            // Capacity was freed: let a waiter open a fresh connection.
            self.inner.cv.notify_all();
        }
        drop(doomed);
    }

    /// Check a connection out. `long` takes a long-read permit first (see the module comment).
    /// A failure is also noted on the calling thread (see [`crate::fault`]).
    pub fn get(&self, long: bool) -> Result<Pooled<M>, PoolError> {
        let started = Instant::now();
        let r = self.get_inner(long, started + self.inner.cfg.acquire_timeout);
        self.inner.observe(started.elapsed());
        if let Err(e) = &r {
            fault::note(e.fault());
        }
        r
    }

    /// Wait on `cv` until woken or `deadline`; counts the caller as waiting (once) and refuses
    /// when too many already wait. `Err` hands back the guard with the reason.
    fn park<'a>(
        &'a self,
        mut st: MutexGuard<'a, State<M::Conn>>,
        cv: &Condvar,
        deadline: Instant,
        waiting: &mut bool,
    ) -> Result<MutexGuard<'a, State<M::Conn>>, (MutexGuard<'a, State<M::Conn>>, PoolError)> {
        if !*waiting {
            if st.waiting >= self.inner.cfg.max_waiters {
                self.inner.counters.rejected.fetch_add(1, Relaxed);
                return Err((st, PoolError::QueueFull));
            }
            st.waiting += 1;
            *waiting = true;
        }
        let now = Instant::now();
        if now >= deadline {
            let e = self.timeout_error(&st);
            return Err((st, e));
        }
        let (st, _) = cv
            .wait_timeout(st, deadline - now)
            .unwrap_or_else(|e| e.into_inner());
        Ok(st)
    }

    fn timeout_error(&self, st: &State<M::Conn>) -> PoolError {
        if st.last_connect_error.is_some() && st.total == st.connecting {
            // Nothing is open and the last attempt to open one failed: the database is the problem.
            return PoolError::Unavailable(st.last_connect_error.clone().unwrap_or_default());
        }
        self.inner.counters.timeouts.fetch_add(1, Relaxed);
        PoolError::Timeout
    }

    fn get_inner(&self, long: bool, deadline: Instant) -> Result<Pooled<M>, PoolError> {
        let inner = &self.inner;
        let mut st = inner.lock();
        let epoch0 = st.connect_epoch;
        let mut waiting = false;
        let mut permit = false;
        let mut doomed: Vec<M::Conn> = Vec::new();
        let outcome: Result<(M::Conn, Instant, bool), PoolError> = 'acquire: loop {
            if st.closed {
                break Err(PoolError::Closed);
            }
            if long && !permit {
                if st.long_out < inner.cfg.max_long {
                    st.long_out += 1;
                    permit = true;
                    continue;
                }
                match self.park(st, &inner.cv_long, deadline, &mut waiting) {
                    Ok(g) => st = g,
                    Err((g, e)) => {
                        st = g;
                        break Err(e);
                    }
                }
                continue;
            }
            // A free connection, newest first. One that is too old is closed instead.
            while let Some(idle) = st.idle.pop() {
                if inner.expired(idle.born, idle.since) {
                    st.total -= 1;
                    inner.counters.closed.fetch_add(1, Relaxed);
                    doomed.push(idle.conn);
                    continue;
                }
                st.out += 1;
                // A connection that was just opened for this caller is not pinged.
                if !idle.used || idle.since.elapsed() < inner.cfg.health_check_idle {
                    break 'acquire Ok((idle.conn, idle.born, idle.used));
                }
                drop(st);
                let mut conn = idle.conn;
                let alive = inner.manager.ping(&mut conn, PING_TIMEOUT);
                st = inner.lock();
                if alive {
                    break 'acquire Ok((conn, idle.born, idle.used));
                }
                st.out -= 1;
                st.total -= 1;
                inner.counters.closed.fetch_add(1, Relaxed);
                doomed.push(conn);
                // The others are probably as dead as this one (a restart): do not ping them one
                // by one while the caller waits.
                let rest = std::mem::take(&mut st.idle);
                st.total -= rest.len();
                inner.counters.closed.fetch_add(rest.len() as u64, Relaxed);
                doomed.extend(rest.into_iter().map(|i| i.conn));
            }
            // Room for another connection: open one on a helper thread, and wait for whichever
            // connection comes free first.
            if st.total < inner.cfg.max_size {
                let outage = st.last_connect_error.is_some();
                if !outage || (st.connecting == 0 && Instant::now() >= st.backoff_until) {
                    self.spawn_connect(&mut st);
                } else if st.total == 0 && st.connecting == 0 {
                    // The last attempt failed a moment ago and nothing is open: say so at once.
                    break Err(PoolError::Unavailable(
                        st.last_connect_error.clone().unwrap_or_default(),
                    ));
                }
            }
            match self.park(st, &inner.cv, deadline, &mut waiting) {
                Ok(g) => st = g,
                Err((g, e)) => {
                    st = g;
                    break Err(e);
                }
            }
            if st.connect_epoch != epoch0 && st.idle.is_empty() && st.connecting == 0 && st.out == 0
            {
                break Err(PoolError::Unavailable(
                    st.last_connect_error.clone().unwrap_or_default(),
                ));
            }
        };
        if waiting {
            st.waiting -= 1;
        }
        let result = match outcome {
            Ok((conn, born, reused)) => Ok(Pooled {
                pool: self.inner.clone(),
                conn: Some(conn),
                born,
                reused,
                long: permit,
                broken: false,
            }),
            Err(e) => {
                if permit {
                    st.long_out -= 1;
                    inner.cv_long.notify_one();
                }
                Err(e)
            }
        };
        let freed = !doomed.is_empty();
        drop(st);
        if freed {
            // Capacity was freed by closing old connections; another waiter may open a new one.
            inner.cv.notify_one();
        }
        drop(doomed);
        result
    }

    /// Reserve a slot and open the connection on a helper thread.
    fn spawn_connect(&self, st: &mut State<M::Conn>) {
        st.total += 1;
        st.connecting += 1;
        let inner = self.inner.clone();
        let spawned = std::thread::Builder::new()
            .name(format!("obop-db-{}-connect", inner.cfg.name))
            .spawn({
                let inner = inner.clone();
                move || {
                    let r = inner.manager.connect();
                    finish_connect(&inner, r);
                }
            });
        if let Err(e) = spawned {
            st.connecting -= 1;
            st.total -= 1;
            st.connect_epoch += 1;
            st.last_connect_error = Some(format!("cannot start a connection thread: {e}"));
            st.backoff_until = Instant::now() + inner.cfg.connect_backoff;
            inner.counters.connect_failures.fetch_add(1, Relaxed);
        }
    }
}

fn finish_connect<M: Manager>(inner: &Arc<Inner<M>>, r: Result<M::Conn, String>) {
    let mut st = inner.lock();
    st.connecting -= 1;
    match r {
        Ok(conn) if !st.closed => {
            let now = Instant::now();
            st.last_connect_error = None;
            st.idle.push(Idle {
                conn,
                born: now,
                since: now,
                used: false,
            });
            inner.counters.opened.fetch_add(1, Relaxed);
            drop(st);
            inner.cv.notify_one();
        }
        Ok(_) => {
            st.total -= 1;
        }
        Err(e) => {
            st.total -= 1;
            st.connect_epoch += 1;
            st.last_connect_error = Some(e);
            st.backoff_until = Instant::now() + inner.cfg.connect_backoff;
            inner.counters.connect_failures.fetch_add(1, Relaxed);
            drop(st);
            // Every waiter should look: some can now say "unavailable" instead of waiting on.
            inner.cv.notify_all();
        }
    }
}

fn spawn_sweeper<M: Manager>(pool: Weak<Inner<M>>, shutdown: Arc<(Mutex<bool>, Condvar)>) {
    let Some(first) = pool.upgrade() else { return };
    let every = (first.cfg.idle_timeout.min(first.cfg.max_lifetime) / 4)
        .clamp(Duration::from_millis(20), Duration::from_secs(5));
    drop(first);
    let _ = std::thread::Builder::new()
        .name("obop-db-sweeper".into())
        .spawn(move || {
            let (flag, cv) = &*shutdown;
            loop {
                {
                    let mut stop = flag.lock().unwrap_or_else(|e| e.into_inner());
                    if !*stop {
                        stop = cv
                            .wait_timeout(stop, every)
                            .unwrap_or_else(|e| e.into_inner())
                            .0;
                    }
                    if *stop {
                        return;
                    }
                }
                // The guard is gone: dropping the last reference to the pool locks the flag.
                match pool.upgrade() {
                    Some(inner) => inner.sweep(),
                    None => return,
                }
            }
        });
}

impl<M: Manager> Drop for Inner<M> {
    fn drop(&mut self) {
        let (flag, cv) = &*self.shutdown;
        *flag.lock().unwrap_or_else(|e| e.into_inner()) = true;
        cv.notify_all();
    }
}

/// A checked-out connection. Dropping it puts the connection back, unless it was marked broken,
/// the holder is panicking, or it is past its lifetime.
pub struct Pooled<M: Manager> {
    pool: Arc<Inner<M>>,
    conn: Option<M::Conn>,
    born: Instant,
    reused: bool,
    long: bool,
    broken: bool,
}

impl<M: Manager> Pooled<M> {
    /// Do not put this connection back: its state is unknown or it is dead.
    pub fn discard(&mut self) {
        self.broken = true;
    }

    /// `true` when the connection had served before (it was not opened for this checkout).
    pub fn reused(&self) -> bool {
        self.reused
    }
}

impl<M: Manager> std::ops::Deref for Pooled<M> {
    type Target = M::Conn;
    fn deref(&self) -> &M::Conn {
        self.conn.as_ref().expect("connection present until drop")
    }
}

impl<M: Manager> std::ops::DerefMut for Pooled<M> {
    fn deref_mut(&mut self) -> &mut M::Conn {
        self.conn.as_mut().expect("connection present until drop")
    }
}

impl<M: Manager> Drop for Pooled<M> {
    fn drop(&mut self) {
        let Some(conn) = self.conn.take() else { return };
        let inner = &self.pool;
        let bad = self.broken
            || std::thread::panicking()
            || self.born.elapsed() >= inner.cfg.max_lifetime;
        let mut doomed = None;
        {
            let mut st = inner.lock();
            st.out -= 1;
            if self.long {
                st.long_out -= 1;
                inner.cv_long.notify_one();
            }
            if bad || st.closed {
                st.total -= 1;
                doomed = Some(conn);
                inner.counters.closed.fetch_add(1, Relaxed);
            } else {
                st.idle.push(Idle {
                    conn,
                    born: self.born,
                    since: Instant::now(),
                    used: true,
                });
            }
            // Either a connection came back or a slot is free to open one.
            inner.cv.notify_one();
        }
        drop(doomed);
    }
}
