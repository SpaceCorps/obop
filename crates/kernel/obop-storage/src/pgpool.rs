//! The PostgreSQL side of the connection pool (D74): configuration from the environment, the
//! manager that opens connections through [`PgConnector`] (so TLS from D66 applies to every one
//! of them), per-statement time limits by command class, and the helpers the backend uses to run
//! work on a pooled connection.
//!
//! # Session state
//!
//! A pooled connection is shared by many callers one after another, so nothing a caller sets may
//! outlive it. The only state this module ever sets is `statement_timeout`:
//!
//! - **direct** (the default): single statements run with a session-level `SET statement_timeout`
//!   that is issued only when the wanted value differs from what the connection already has (it
//!   is tracked per connection, so a run of point reads costs no extra round trip); transactions
//!   start with `BEGIN; SET LOCAL statement_timeout = ...` in one round trip and never touch the
//!   session value;
//! - **transaction** (`OBOP_DB_POOLER=transaction`, PgBouncer in transaction mode and the like):
//!   the server connection behind a client connection changes between transactions, so a
//!   session-level `SET` would land on somebody else's session. Every statement therefore runs
//!   inside its own transaction and the limit is `SET LOCAL`, which ends with the transaction.
//!
//! A connection is discarded, not returned, when a statement failed in a way that leaves its state
//! unknown (the socket broke, the server closed it, a rollback failed, the holder panicked).

use crate::fault::{self, Fault};
use crate::pool::{Manager, Pool, Pooled, Settings};
use crate::tls::PgConnector;
use postgres::error::SqlState;
use postgres::{Client, Transaction};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::Duration;

/// What kind of work a statement is, which decides its time limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Class {
    /// One record by key, a counter, a short-lived value: default 5 s.
    Point,
    /// A list, a search, a lookup that returns many rows: default 10 s.
    List,
    /// A scan that folds a whole model, a count of everything: default 15 s.
    Aggregate,
    /// A multi-statement transaction, an import, an export, DDL: default 60 s.
    Batch,
}

/// How the application's connections reach PostgreSQL.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Pooler {
    /// Straight to PostgreSQL, or through a pooler in session mode.
    #[default]
    Direct,
    /// Through a pooler that hands the server connection back after every transaction.
    Transaction,
}

/// Everything about the pool that an operator can set (see docs/DEPLOY.md, "Connection pool").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PgPoolConfig {
    /// Connections in the main pool (`OBOP_DB_POOL_SIZE`, 1 to 64, default 8).
    pub size: usize,
    pub acquire_timeout: Duration,
    pub idle_timeout: Duration,
    pub max_lifetime: Duration,
    pub health_check_idle: Duration,
    pub connect_timeout: Duration,
    pub max_waiters: usize,
    /// Streaming scans, exports and imports allowed at once (default half the pool).
    pub max_long: usize,
    /// Time limits in milliseconds; `None` leaves the server's own `statement_timeout` alone.
    pub point_ms: Option<u64>,
    pub list_ms: Option<u64>,
    pub aggregate_ms: Option<u64>,
    pub batch_ms: Option<u64>,
    /// How long a scan or export transaction may sit idle between two fetches.
    pub idle_in_tx_ms: Option<u64>,
    pub pooler: Pooler,
    /// Connections for advisory locks (1 to 8, default 8); a pool of their own.
    pub lock_size: usize,
    /// A direct URL for the lock connections, when the main URL goes through a pooler.
    pub lock_url: Option<String>,
    /// Connections of the blob store's pool (default 4).
    pub blob_size: usize,
}

impl Default for PgPoolConfig {
    fn default() -> Self {
        let size = 8;
        Self {
            size,
            acquire_timeout: Duration::from_millis(2000),
            idle_timeout: Duration::from_secs(300),
            max_lifetime: Duration::from_secs(1800),
            health_check_idle: Duration::from_millis(1000),
            connect_timeout: Duration::from_millis(10_000),
            max_waiters: 256,
            max_long: (size / 2).max(1),
            point_ms: Some(5_000),
            list_ms: Some(10_000),
            aggregate_ms: Some(15_000),
            batch_ms: Some(60_000),
            idle_in_tx_ms: Some(120_000),
            pooler: Pooler::Direct,
            lock_size: 8,
            lock_url: None,
            blob_size: 4,
        }
    }
}

fn number(
    get: &dyn Fn(&str) -> Option<String>,
    name: &str,
    default: u64,
    min: u64,
    max: u64,
) -> Result<u64, String> {
    let Some(raw) = get(name)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    else {
        return Ok(default);
    };
    match raw.parse::<u64>() {
        Ok(n) if (min..=max).contains(&n) => Ok(n),
        _ => Err(format!(
            "{name} must be a whole number from {min} to {max} (got {})",
            shown(&raw)
        )),
    }
}

/// A rejected value as it may appear in a message: short and not shaped like an address (a
/// database URL put into the wrong variable carries a password).
fn shown(raw: &str) -> String {
    if raw.len() > 24 || raw.contains('@') || raw.contains("://") {
        "a value that is not shown".into()
    } else {
        format!("{raw:?}")
    }
}

impl PgPoolConfig {
    /// The settings for a database the process opens once, for a moment, that is not the one it
    /// serves from (the control plane seeding a new instance's database): one connection, and
    /// nothing from the `OBOP_DB_*` or `OBOP_LOCK_DATABASE_URL` variables, which describe the
    /// process's own database and would be applied to the wrong one.
    pub fn one_shot() -> Self {
        Self {
            size: 1,
            max_long: 1,
            lock_size: 1,
            blob_size: 1,
            ..Self::default()
        }
    }

    /// Say that the database this process opens is reached by an address of the operator's own
    /// choosing, not through the pooler the `OBOP_DB_POOLER` and `OBOP_LOCK_DATABASE_URL`
    /// variables describe: those two describe how the server reaches *its* database
    /// (`OBOP_DATABASE_URL`). A command line tool that was given another address with `--db`
    /// (a maintenance command that goes straight to the database) calls this, and from then on
    /// both variables are ignored in this process; it is a direct connection, so a session
    /// advisory lock taken on it excludes everybody, and there is no second address to keep
    /// apart. The server itself (`obop serve`) never calls this.
    pub fn address_is_direct() {
        DIRECT_ADDRESS.store(true, Relaxed);
    }

    /// Why a pooler in transaction mode is refused without a direct address for the locks.
    pub(crate) const NEEDS_LOCK_URL: &'static str = "OBOP_DB_POOLER=transaction needs OBOP_LOCK_DATABASE_URL: a session advisory lock belongs to one server session, which a transaction-mode pooler hands to somebody else between statements, so locks taken through it exclude nobody; give the lock connections a direct address of the same database";

    /// Read the settings from the process environment. A value that is not a number in range is
    /// an error that names the variable, so a typo stops the server at start-up.
    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(&|k| std::env::var(k).ok())
    }

    pub fn from_lookup(get: &dyn Fn(&str) -> Option<String>) -> Result<Self, String> {
        let d = Self::default();
        let size = number(get, "OBOP_DB_POOL_SIZE", d.size as u64, 1, 64)? as usize;
        let ms = |v: u64| (v > 0).then_some(v);
        let timeout = |name: &str, default: Option<u64>| -> Result<Option<u64>, String> {
            Ok(ms(number(get, name, default.unwrap_or(0), 0, 3_600_000)?))
        };
        let pooler = match get("OBOP_DB_POOLER").map(|v| v.trim().to_ascii_lowercase()) {
            None => Pooler::Direct,
            Some(v) if v.is_empty() || v == "direct" || v == "session" => Pooler::Direct,
            Some(v) if v == "transaction" => Pooler::Transaction,
            Some(v) => {
                return Err(format!(
                    "OBOP_DB_POOLER must be \"direct\" or \"transaction\" (got {})",
                    shown(&v)
                ));
            }
        };
        let lock_url = get("OBOP_LOCK_DATABASE_URL")
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        // See `address_is_direct`: a direct address has neither a pooler nor a second address.
        let (pooler, lock_url) = if DIRECT_ADDRESS.load(Relaxed) {
            (Pooler::Direct, None)
        } else {
            (pooler, lock_url)
        };
        if pooler == Pooler::Transaction && lock_url.is_none() {
            return Err(Self::NEEDS_LOCK_URL.into());
        }
        let max_long = number(get, "OBOP_DB_LONG_READS", (size as u64 / 2).max(1), 1, 64)? as usize;
        Ok(Self {
            size,
            acquire_timeout: Duration::from_millis(number(
                get,
                "OBOP_DB_ACQUIRE_TIMEOUT_MS",
                2_000,
                50,
                60_000,
            )?),
            idle_timeout: Duration::from_secs(number(
                get,
                "OBOP_DB_IDLE_TIMEOUT_SECS",
                300,
                1,
                86_400,
            )?),
            max_lifetime: Duration::from_secs(number(
                get,
                "OBOP_DB_MAX_LIFETIME_SECS",
                1_800,
                10,
                86_400,
            )?),
            health_check_idle: Duration::from_millis(number(
                get,
                "OBOP_DB_HEALTH_CHECK_IDLE_MS",
                1_000,
                0,
                3_600_000,
            )?),
            connect_timeout: Duration::from_millis(number(
                get,
                "OBOP_DB_CONNECT_TIMEOUT_MS",
                10_000,
                100,
                120_000,
            )?),
            max_waiters: number(get, "OBOP_DB_MAX_WAITERS", 256, 0, 100_000)? as usize,
            max_long: max_long.min(size),
            point_ms: timeout("OBOP_DB_STATEMENT_TIMEOUT_POINT_MS", d.point_ms)?,
            list_ms: timeout("OBOP_DB_STATEMENT_TIMEOUT_LIST_MS", d.list_ms)?,
            aggregate_ms: timeout("OBOP_DB_STATEMENT_TIMEOUT_AGGREGATE_MS", d.aggregate_ms)?,
            batch_ms: timeout("OBOP_DB_STATEMENT_TIMEOUT_BATCH_MS", d.batch_ms)?,
            idle_in_tx_ms: timeout("OBOP_DB_IDLE_IN_TRANSACTION_MS", d.idle_in_tx_ms)?,
            pooler,
            lock_size: number(get, "OBOP_DB_LOCK_POOL_SIZE", 8, 1, 8)? as usize,
            lock_url,
            blob_size: number(get, "OBOP_DB_BLOB_POOL_SIZE", 4, 1, 64)? as usize,
        })
    }

    fn limit(&self, class: Class) -> Option<u64> {
        match class {
            Class::Point => self.point_ms,
            Class::List => self.list_ms,
            Class::Aggregate => self.aggregate_ms,
            Class::Batch => self.batch_ms,
        }
    }

    pub(crate) fn settings(&self, name: &'static str, size: usize) -> Settings {
        let mut s = Settings::new(name, size);
        s.acquire_timeout = self.acquire_timeout;
        s.idle_timeout = self.idle_timeout;
        s.max_lifetime = self.max_lifetime;
        s.health_check_idle = self.health_check_idle;
        s.max_waiters = self.max_waiters;
        s.max_long = self.max_long.min(size);
        s
    }
}

/// `SET LOCAL statement_timeout` to the class's limit, or to the server's own value.
fn local_limit(want: Option<u64>) -> String {
    match want {
        Some(ms) => format!("SET LOCAL statement_timeout = {ms}"),
        None => "SET LOCAL statement_timeout TO DEFAULT".into(),
    }
}

/// A connection with what the pool remembers about its session.
pub(crate) struct Session {
    pub(crate) client: Client,
    /// The session-level `statement_timeout` in milliseconds; `None` is the server's own value.
    timeout: Option<u64>,
    /// A rollback failed: the transaction state is unknown, so the connection must go.
    dirty: bool,
}

impl Session {
    pub(crate) fn new(client: Client) -> Self {
        Self {
            client,
            timeout: None,
            dirty: false,
        }
    }

    fn set_timeout(&mut self, want: Option<u64>) -> Result<(), postgres::Error> {
        if self.timeout == want {
            return Ok(());
        }
        match want {
            Some(ms) => self
                .client
                .batch_execute(&format!("SET statement_timeout = {ms}"))?,
            None => self.client.batch_execute("RESET statement_timeout")?,
        }
        self.timeout = want;
        Ok(())
    }

    /// `BEGIN` with the limit for this transaction only, in one round trip. A class without a
    /// limit gets `DEFAULT`: the server's own value, whatever a single-statement call left on
    /// this session before.
    fn begin(&mut self, want: Option<u64>, extra: &str) -> Result<(), postgres::Error> {
        self.client
            .batch_execute(&format!("BEGIN{extra}; {}", local_limit(want)))
    }

    fn rollback(&mut self) {
        if self.client.batch_execute("ROLLBACK").is_err() {
            self.dirty = true;
        }
    }
}

pub(crate) struct PgManager {
    connector: PgConnector,
    /// The limit a new connection is given before anyone uses it, so the first call on it does not
    /// pay a round trip for it: the point-read limit in direct mode, none otherwise.
    initial: Option<u64>,
}

impl PgManager {
    pub(crate) fn new(connector: PgConnector, initial: Option<u64>) -> Self {
        Self { connector, initial }
    }
}

impl Manager for PgManager {
    type Conn = Session;
    fn connect(&self) -> Result<Session, String> {
        let mut s = Session::new(self.connector.connect()?);
        if self.initial.is_some() {
            s.set_timeout(self.initial)
                .map_err(|e| describe_unsent(&e))?;
        }
        Ok(s)
    }
    fn ping(&self, conn: &mut Session, timeout: Duration) -> bool {
        conn.client.is_valid(timeout).is_ok()
    }
}

/// The error of a pooled statement: the driver's, or a message made by the backend.
pub(crate) enum PErr {
    Pg(postgres::Error),
    /// The connection failed before the statement of the caller was sent (setting the session's
    /// limit, `BEGIN`): nothing of the caller's was on it, so no change is in doubt.
    Setup(postgres::Error),
    Msg(String),
}

impl From<postgres::Error> for PErr {
    fn from(e: postgres::Error) -> Self {
        PErr::Pg(e)
    }
}
impl From<String> for PErr {
    fn from(e: String) -> Self {
        PErr::Msg(e)
    }
}
impl From<&str> for PErr {
    fn from(e: &str) -> Self {
        PErr::Msg(e.to_string())
    }
}

pub(crate) type PR<T> = std::result::Result<T, PErr>;

static STATEMENT_TIMEOUTS: AtomicU64 = AtomicU64::new(0);

/// Set by [`PgPoolConfig::address_is_direct`].
static DIRECT_ADDRESS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Statements cancelled by their time limit since the process started.
pub fn statement_timeouts() -> u64 {
    STATEMENT_TIMEOUTS.load(Relaxed)
}

/// The server cancelled the statement (SQLSTATE 57014, `query_canceled`). The message text is
/// localised by the server's `lc_messages`, so only the code is trusted. obop sets no
/// `lock_timeout` (that is 55P03) and never cancels another backend, so within the application a
/// cancelled statement is a statement that ran into `statement_timeout`.
fn is_statement_timeout(e: &postgres::Error) -> bool {
    e.code() == Some(&SqlState::QUERY_CANCELED)
}

/// Errors the driver raises on the client side about one statement, with the connection still
/// sound: a parameter that does not serialise, a column of the wrong type, a wrong row count.
fn is_statement_level(e: &postgres::Error) -> bool {
    let text = e.to_string();
    [
        "error serializing parameter",
        "error deserializing column",
        "invalid column",
        "query returned an unexpected",
        "expected ",
    ]
    .iter()
    .any(|p| text.starts_with(p))
}

/// The connection itself failed (as opposed to a statement the server refused or the driver
/// refused to send): the socket broke, the server closed the session or is shutting down, the
/// stream is out of step. Such a connection is not used again.
pub(crate) fn connection_failed(e: &postgres::Error) -> bool {
    match e.as_db_error() {
        None => !is_statement_level(e),
        Some(d) => {
            let c = d.code();
            c == &SqlState::ADMIN_SHUTDOWN
                || c == &SqlState::CRASH_SHUTDOWN
                || c == &SqlState::CANNOT_CONNECT_NOW
                || c == &SqlState::IDLE_IN_TRANSACTION_SESSION_TIMEOUT
                || c.code().starts_with("08")
        }
    }
}

/// The error text of a driver error, noting a storage fault for the HTTP layer when it is one.
pub(crate) fn describe(e: &postgres::Error) -> String {
    describe_with(e, true)
}

/// As [`describe`], for an error raised before any statement of the caller was sent on the
/// connection (the limit of the session, `BEGIN`): a connection found dead there changed nothing,
/// which is "unavailable" and not "a change may or may not have been applied".
pub(crate) fn describe_unsent(e: &postgres::Error) -> String {
    describe_with(e, false)
}

fn describe_with(e: &postgres::Error, sent: bool) -> String {
    if is_statement_timeout(e) {
        STATEMENT_TIMEOUTS.fetch_add(1, Relaxed);
        fault::note(Fault::Timeout);
        return Fault::Timeout.message("a database statement ran into its time limit");
    }
    // A write the database's own guards refused (D77, SQLSTATE OBG01 / OBG02) is neither a fault
    // nor a plain server error: it carries the typed form of `refusal` through the `String`.
    if let Some(r) = e.as_db_error().and_then(crate::refusal::from_db) {
        return crate::refusal::to_error(&r);
    }
    if connection_failed(e) {
        if sent {
            // A statement was on this connection when it broke: what it did is unknown.
            fault::note_in_flight();
        } else {
            fault::note(Fault::Unavailable);
        }
        return Fault::Unavailable.message(&e.to_string());
    }
    match e.as_db_error() {
        // The driver's own text for a server error is only "db error"; the code and the message
        // of the server (no detail, which may carry values) say what happened.
        Some(d) => format!("db error (SQLSTATE {}): {}", d.code().code(), d.message()),
        None => e.to_string(),
    }
}

/// The server rolled the transaction back because it was part of a deadlock (40P01) or could not
/// be serialised (40001): nothing of it was applied, so the whole transaction may be run again.
pub(crate) fn rolled_back_for_retry(e: &PErr) -> bool {
    let code_is_retryable = |c: &str| c == "40P01" || c == "40001";
    match e {
        PErr::Pg(e) => e.code().is_some_and(|c| code_is_retryable(c.code())),
        PErr::Setup(_) => false,
        PErr::Msg(m) => is_retryable_text(m),
    }
}

/// Does this error text, made by [`describe`], say the server rolled the transaction back for a
/// deadlock or a serialisation failure? The words are recognised where `describe` puts them, at
/// the start of the text. Anywhere else they may be text somebody chose: the message of a
/// refusal names the record, and a record id that contains `SQLSTATE 40001` must not make a
/// batch the guards refused run again.
pub(crate) fn is_retryable_text(m: &str) -> bool {
    ["db error (SQLSTATE 40P01)", "db error (SQLSTATE 40001)"]
        .iter()
        .any(|p| m.starts_with(p))
}

/// A checked-out connection with the pool's rules attached.
pub(crate) struct Lease {
    pooled: Pooled<PgManager>,
    pool: Pool<PgManager>,
    cfg: Arc<PgPoolConfig>,
}

impl Lease {
    fn session(&mut self) -> &mut Session {
        &mut self.pooled
    }

    /// Turn a statement's outcome into the storage contract's, discarding the connection when it
    /// can no longer be trusted.
    pub(crate) fn finish<T>(&mut self, r: PR<T>) -> Result<T, String> {
        self.vet();
        match r {
            Ok(v) => Ok(v),
            Err(PErr::Msg(m)) => Err(m),
            Err(PErr::Pg(e)) => {
                let text = describe(&e);
                if connection_failed(&e) {
                    self.broken();
                }
                Err(text)
            }
            Err(PErr::Setup(e)) => {
                let text = describe_unsent(&e);
                if connection_failed(&e) {
                    self.broken();
                }
                Err(text)
            }
        }
    }

    /// A failed rollback, or a socket that is gone, spoils the connection whatever the statement
    /// said (a batch that reports its own failure as a value still ends up here as `Ok`). Every
    /// way a lease ends goes through here, also the one that runs a transaction again.
    fn vet(&mut self) {
        if self.pooled.dirty || self.pooled.client.is_closed() {
            self.broken();
        }
    }

    /// Close this connection and every idle one: they most likely share its fate.
    fn broken(&mut self) {
        self.pooled.discard();
        self.pool.purge_idle();
    }

    /// Run one statement (or a few that need not be atomic) with the limit of `class`.
    fn exec<T>(&mut self, class: Class, f: impl FnOnce(&mut Client) -> PR<T>) -> PR<T> {
        let want = self.cfg.limit(class);
        let pooler = self.cfg.pooler;
        let s = self.session();
        match pooler {
            Pooler::Direct => {
                s.set_timeout(want).map_err(PErr::Setup)?;
                f(&mut s.client)
            }
            Pooler::Transaction => {
                s.begin(want, "").map_err(PErr::Setup)?;
                match f(&mut s.client) {
                    Ok(v) => {
                        s.client.batch_execute("COMMIT")?;
                        Ok(v)
                    }
                    Err(e) => {
                        s.rollback();
                        Err(e)
                    }
                }
            }
        }
    }

    /// Run `f` in one transaction with the limit of `class`. `f` says whether to commit.
    fn transaction<T>(
        &mut self,
        class: Class,
        f: impl FnOnce(&mut Client) -> PR<(T, bool)>,
    ) -> PR<T> {
        let want = self.cfg.limit(class);
        let s = self.session();
        s.begin(want, "").map_err(PErr::Setup)?;
        match f(&mut s.client) {
            Ok((v, true)) => {
                s.client.batch_execute("COMMIT")?;
                Ok(v)
            }
            Ok((v, false)) => {
                s.rollback();
                Ok(v)
            }
            Err(e) => {
                s.rollback();
                Err(e)
            }
        }
    }

    /// Run `f` in one repeatable-read, read-only transaction: every statement in it sees the same
    /// snapshot. The limit applies to each statement, and a transaction left idle between two of
    /// them (a slow consumer) is ended by the server after `idle_in_tx_ms`.
    fn snapshot<T>(
        &mut self,
        class: Class,
        f: impl FnOnce(&mut Transaction<'_>) -> PR<T>,
    ) -> PR<T> {
        let want = self.cfg.limit(class);
        let idle = self.cfg.idle_in_tx_ms;
        let s = self.session();
        let mut tx = s
            .client
            .build_transaction()
            .isolation_level(postgres::IsolationLevel::RepeatableRead)
            .read_only(true)
            .start()?;
        let mut set = format!("{};", local_limit(want));
        if let Some(ms) = idle {
            set.push_str(&format!(
                "SET LOCAL idle_in_transaction_session_timeout = {ms};"
            ));
        }
        tx.batch_execute(&set)?;
        let v = f(&mut tx)?;
        tx.commit()?;
        Ok(v)
    }
}

/// How many more times a transaction the server rolled back for a deadlock is run.
pub(crate) const MAX_REPEATS: u32 = 3;

/// A short, slightly random pause before a rolled-back transaction is run again, so the two that
/// collided do not collide again in step.
pub(crate) fn repeat_pause(attempt: u32) {
    let jitter = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::from(d.subsec_nanos()) % 15);
    std::thread::sleep(Duration::from_millis(5 * u64::from(attempt) + jitter));
}

/// A pool of PostgreSQL connections for one purpose (the records, the blobs).
#[derive(Clone)]
pub(crate) struct PgPool {
    pool: Pool<PgManager>,
    cfg: Arc<PgPoolConfig>,
}

/// Options every pooled connection gets: a bounded connect, keep-alive so a vanished server is
/// noticed, and a name that shows in `pg_stat_activity`.
pub(crate) fn tune(mut c: PgConnector, application: &str, cfg: &PgPoolConfig) -> PgConnector {
    let conf = c.config_mut();
    // What the URL sets itself wins over the defaults of the pool.
    if conf.get_connect_timeout().is_none() {
        conf.connect_timeout(cfg.connect_timeout);
    }
    conf.keepalives(true);
    // The driver's own default idle time is two hours: a value other than that was chosen.
    if conf.get_keepalives_idle() == Duration::from_secs(2 * 60 * 60) {
        conf.keepalives_idle(Duration::from_secs(60));
    }
    if conf.get_tcp_user_timeout().is_none() {
        conf.tcp_user_timeout(Duration::from_secs(30));
    }
    if conf.get_application_name().is_none() {
        conf.application_name(application);
    }
    c
}

impl PgPool {
    pub(crate) fn new(
        name: &'static str,
        connector: PgConnector,
        size: usize,
        cfg: &Arc<PgPoolConfig>,
    ) -> Self {
        let connector = tune(connector, &format!("obop-{name}"), cfg);
        let initial = match cfg.pooler {
            Pooler::Direct => cfg.point_ms,
            Pooler::Transaction => None,
        };
        Self {
            pool: Pool::new(PgManager::new(connector, initial), cfg.settings(name, size)),
            cfg: cfg.clone(),
        }
    }

    /// A reading of this pool alone.
    #[cfg(test)]
    pub(crate) fn stats(&self) -> crate::pool::PoolStats {
        self.pool.report()
    }

    /// Hand over a connection that is already open (the one that prepared the schema).
    pub(crate) fn seed(&self, client: Client) {
        let mut s = Session::new(client);
        if self.cfg.pooler == Pooler::Direct {
            // Best effort: a failure here shows on the first call, which discards the connection.
            let _ = s.set_timeout(self.cfg.point_ms);
        }
        self.pool.seed(s);
    }

    /// Check a connection out; `long` takes a long-read permit as well (see `pool`). The error is
    /// already in the storage contract's form and has noted its fault.
    pub(crate) fn lease(&self, long: bool) -> Result<Lease, String> {
        match self.pool.get(long) {
            Ok(pooled) => Ok(Lease {
                pooled,
                pool: self.pool.clone(),
                cfg: self.cfg.clone(),
            }),
            Err(e) => Err(e.message()),
        }
    }

    /// One statement, not repeated if the connection turns out to be dead: for writes.
    pub(crate) fn run<T>(
        &self,
        class: Class,
        f: impl FnOnce(&mut Client) -> PR<T>,
    ) -> Result<T, String> {
        let mut lease = self.lease(false)?;
        let r = lease.exec(class, f);
        lease.finish(r)
    }

    /// A read. If the connection was an old one and turns out to be dead (the server restarted),
    /// the idle connections are dropped and the read is repeated once on a fresh one: a read can
    /// be repeated without changing anything.
    pub(crate) fn read<T>(
        &self,
        class: Class,
        mut f: impl FnMut(&mut Client) -> PR<T>,
    ) -> Result<T, String> {
        let mut attempt = 0;
        loop {
            let mut lease = self.lease(false)?;
            let reused = lease.pooled.reused();
            let r = lease.exec(class, &mut f);
            if attempt == 0
                && reused
                && matches!(&r, Err(PErr::Pg(e) | PErr::Setup(e)) if connection_failed(e))
            {
                lease.broken();
                attempt += 1;
                continue;
            }
            return lease.finish(r);
        }
    }

    /// Several statements in one transaction, committed when `f` returns `Ok`. (The callers that
    /// can be run again from the start use [`PgPool::tx_retry`]; this one is for the tests.)
    #[cfg(test)]
    pub(crate) fn tx<T>(
        &self,
        class: Class,
        f: impl FnOnce(&mut Client) -> PR<T>,
    ) -> Result<T, String> {
        self.tx_decide(class, false, |c| f(c).map(|v| (v, true)))
    }

    /// As [`PgPool::tx`], but `f` also says whether to commit (`false` rolls back and still
    /// returns the value). `long` runs it in the long-read lane (imports).
    pub(crate) fn tx_decide<T>(
        &self,
        class: Class,
        long: bool,
        f: impl FnOnce(&mut Client) -> PR<(T, bool)>,
    ) -> Result<T, String> {
        let mut lease = self.lease(long)?;
        let r = lease.transaction(class, f);
        lease.finish(r)
    }

    /// As [`PgPool::tx`] for a transaction that can be run again from its start: when the server
    /// rolled it back for a deadlock (40P01) or a serialisation failure (40001) nothing of it was
    /// applied, and it is run again, at most three more times after a short pause.
    pub(crate) fn tx_retry<T>(
        &self,
        class: Class,
        mut f: impl FnMut(&mut Client) -> PR<T>,
    ) -> Result<T, String> {
        let mut attempt = 0;
        loop {
            let mut lease = self.lease(false)?;
            let r = lease.transaction(class, |c| f(c).map(|v| (v, true)));
            if attempt < MAX_REPEATS && r.as_ref().is_err_and(rolled_back_for_retry) {
                // The rollback that ended this attempt may have failed (the socket died): such a
                // connection must not go back to the pool for the next attempt to find.
                lease.vet();
                drop(lease);
                attempt += 1;
                repeat_pause(attempt);
                continue;
            }
            return lease.finish(r);
        }
    }

    /// A short read that needs one snapshot over several statements (a page of rows and the
    /// count behind it, the rows of a keyset page): a repeatable-read, read-only transaction on a
    /// normal lease, not in the long-read lane that [`PgPool::snapshot`] takes, so that requests
    /// can run as many of them at once as the pool has connections. The limit of `class` applies
    /// to each statement.
    ///
    /// As [`PgPool::read`]: if the connection was an old one and turns out to be dead (the server
    /// restarted), the idle connections are dropped and `f` runs again, once, on a fresh one. It
    /// is therefore `FnMut`, and must not leave anything behind that a second run would repeat.
    // Used by the query and index layers of wave 2 (D76, D78); tested here until they land.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn read_tx<T>(
        &self,
        class: Class,
        mut f: impl FnMut(&mut Transaction<'_>) -> PR<T>,
    ) -> Result<T, String> {
        let mut attempt = 0;
        loop {
            let mut lease = self.lease(false)?;
            let reused = lease.pooled.reused();
            let r = lease.snapshot(class, &mut f);
            if attempt == 0
                && reused
                && matches!(&r, Err(PErr::Pg(e) | PErr::Setup(e)) if connection_failed(e))
            {
                lease.broken();
                attempt += 1;
                continue;
            }
            return lease.finish(r);
        }
    }

    /// A consistent view of the whole database for a long read, in the long-read lane.
    pub(crate) fn snapshot<T>(
        &self,
        class: Class,
        f: impl FnOnce(&mut Transaction<'_>) -> PR<T>,
    ) -> Result<T, String> {
        let mut lease = self.lease(true)?;
        let r = lease.snapshot(class, f);
        lease.finish(r)
    }
}
