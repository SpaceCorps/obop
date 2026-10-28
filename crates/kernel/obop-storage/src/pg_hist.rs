//! The PostgreSQL side of system fields, row versions and the change history (D75, work package
//! 3). Memory and SQLite already keep them; this file gives PostgreSQL the same behaviour so the
//! command layer can switch tracking on for every backend.
//!
//! What lives here, and what `pg.rs` keeps of it (one-line calls into this file):
//!
//! - **The schema part `history`** ([`PART`]): the table `obop_record_change`, partitioned by
//!   month (UTC) with a default partition that takes whatever no month takes, the sequence that
//!   numbers the entries, the guard that makes the table append-only, and four functions that do
//!   the maintenance the guard otherwise forbids (`obop_history_ensure_partition`,
//!   `obop_history_drop_partition`, `obop_history_erase`, and the guard itself). Unlike the
//!   record store it is **not made at start**: a process that starts touches nothing for a
//!   feature that no model uses, so a database installed before it opens as it always did, with
//!   any role that could open it. It is made by the first write of a model that keeps history
//!   (`State::install_on_demand`, under `Ddl::IfMissing`), by `Backend::ensure_history_schema`
//!   (which the command layer will call when it registers a model that keeps history, D75 WP4
//!   and WP7: nothing calls it yet; under `Ddl::Never` it only checks), by
//!   `schema::install_all`, and by a restore whose backup carries entries (under `IfMissing`).
//! - **The transaction** ([`Tx`]): the lock of a record and the database's clock in one statement
//!   (the clock is read after the lock is granted, so the stamps of two writers of one record
//!   follow the order of the locks), the flush of the entries of a batch in one statement, and the
//!   erasure of the values of a record's entries.
//! - **Reads**: the history of a record, the change feed (commit-safe: an entry is shown only
//!   once no transaction that holds a lower transaction number is still open), statistics.
//! - **Maintenance primitives**: make and drop the partition of a month. The tick that calls
//!   them is not here.
//! - **The stamp of old records** (`backfill`), **backup** (`export` of the entries as the rows
//!   of the model `record_change`) and **restore** ([`Import`]: the entries, the partitions they
//!   need, the sequence).
//!
//! The append-only guard is an accident guard, not a defence against the role that owns the table
//! (it can set the maintenance setting itself): real separation of roles is D77.

use crate::fault::Fault;
use crate::history::{
    self, Backfilled, ChangePage, ChangeQuery, HistoryQuery, HistoryStats, PartitionEnsured,
};
use crate::keyexpr::{self, Slot};
use crate::pg::{Pg, pg_fields};
use crate::pgpool::{Class, PR, PgPool, describe};
use crate::schema::{self, Part};
use crate::sysfields::{self, ChangeRow, Clock, fmt_micros, parse_micros};
use crate::{Ddl, Fields, J, Result, RowSink};
use postgres::Client;
use postgres::error::SqlState;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::{Duration, Instant};

// ---- the schema part -------------------------------------------------------------------------------

/// The SQL of the part, filled in by [`render`]: `{schema}` is the quoted name of the schema
/// (an identifier), `{schema_lit}` the same name for the inside of a string literal (a quote
/// doubled), and `{do}` the quote of the body of the `DO` block (a tag the name does not
/// contain). Every statement is idempotent. The functions are `SECURITY DEFINER` and pin their
/// `search_path`; every function of the server that they call is named with its schema
/// (`pg_catalog.`), because a function of the same name in the history's schema, that takes
/// exactly the types of the call, would be chosen over one that needs a cast. Their names carry
/// no version on purpose: a changed signature gets a new name, so that an older binary running
/// `CREATE OR REPLACE` can never clobber a newer function.
const SQL: &str = r##"
CREATE SEQUENCE IF NOT EXISTS {schema}.obop_record_change_seq;
CREATE TABLE IF NOT EXISTS {schema}.obop_record_change (
  seq BIGINT NOT NULL DEFAULT nextval('{schema_lit}.obop_record_change_seq'::regclass),
  at TIMESTAMPTZ NOT NULL,
  txid XID8 NOT NULL DEFAULT pg_current_xact_id(),
  model TEXT NOT NULL,
  record_id TEXT NOT NULL,
  op TEXT NOT NULL CHECK (op IN ('create','update','delete','transition','erase','import')),
  actor TEXT NOT NULL,
  via JSONB NOT NULL DEFAULT '{}' CHECK (jsonb_typeof(via) = 'object'),
  row_version BIGINT NOT NULL,
  changed JSONB NOT NULL CHECK (jsonb_typeof(changed) = 'object'),
  sealed BOOLEAN NOT NULL DEFAULT false,
  batch_id TEXT,
  run_id TEXT,
  PRIMARY KEY (at, seq)
) PARTITION BY RANGE (at);
CREATE TABLE IF NOT EXISTS {schema}.obop_record_change_default
  PARTITION OF {schema}.obop_record_change DEFAULT;
CREATE INDEX IF NOT EXISTS obop_record_change_rec
  ON {schema}.obop_record_change (model, record_id, seq DESC);
CREATE INDEX IF NOT EXISTS obop_record_change_feed
  ON {schema}.obop_record_change (txid, seq);
CREATE INDEX IF NOT EXISTS obop_record_change_sealed
  ON {schema}.obop_record_change (seq) WHERE sealed;

CREATE OR REPLACE FUNCTION {schema}.obop_history_guard() RETURNS trigger
LANGUAGE plpgsql SET search_path = {schema}, pg_temp AS $$
BEGIN
  IF pg_catalog.current_setting('obop.history_maint', true) IS DISTINCT FROM 'on'
     OR current_user <> (SELECT pg_catalog.pg_get_userbyid(relowner) FROM pg_catalog.pg_class
                          WHERE oid = tg_relid) THEN
    RAISE EXCEPTION 'obop_record_change is append-only' USING errcode = '42501';
  END IF;
  RETURN coalesce(new, old);
END $$;

DO {do} BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'obop_record_change_ro'
                  AND tgrelid = '{schema_lit}.obop_record_change'::regclass) THEN
    CREATE TRIGGER obop_record_change_ro BEFORE UPDATE OR DELETE ON {schema}.obop_record_change
      FOR EACH ROW EXECUTE FUNCTION {schema}.obop_history_guard();
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'obop_record_change_ro_t'
                  AND tgrelid = '{schema_lit}.obop_record_change'::regclass) THEN
    CREATE TRIGGER obop_record_change_ro_t BEFORE TRUNCATE ON {schema}.obop_record_change
      FOR EACH STATEMENT EXECUTE FUNCTION {schema}.obop_history_guard();
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'obop_record_change_ro_t'
                  AND tgrelid = '{schema_lit}.obop_record_change_default'::regclass) THEN
    CREATE TRIGGER obop_record_change_ro_t BEFORE TRUNCATE ON {schema}.obop_record_change_default
      FOR EACH STATEMENT EXECUTE FUNCTION {schema}.obop_history_guard();
  END IF;
END {do};

CREATE OR REPLACE FUNCTION {schema}.obop_history_ensure_partition(p_month date) RETURNS text
LANGUAGE plpgsql SECURITY DEFINER SET search_path = {schema}, pg_temp AS $$
DECLARE
  lo timestamptz := pg_catalog.date_trunc('month', p_month::timestamp) AT TIME ZONE 'UTC';
  hi timestamptz := (pg_catalog.date_trunc('month', p_month::timestamp) + interval '1 month') AT TIME ZONE 'UTC';
  part text := 'obop_record_change_p' || pg_catalog.to_char(p_month::timestamp, 'YYYYMM');
  moved bigint := 0;
BEGIN
  IF p_month IS NULL OR p_month < date '2000-01-01' OR p_month > (current_date + interval '1 year')::date THEN
    RAISE EXCEPTION 'month % is outside the range history partitions are made for', p_month;
  END IF;
  PERFORM pg_catalog.pg_advisory_xact_lock(pg_catalog.hashtextextended('obop-history-partition', 0));
  IF pg_catalog.to_regclass(part) IS NOT NULL THEN RETURN 'exists'; END IF;
  PERFORM pg_catalog.set_config('obop.history_maint', 'on', true);
  EXECUTE pg_catalog.format('CREATE TABLE %I (LIKE obop_record_change INCLUDING DEFAULTS INCLUDING CONSTRAINTS)', part);
  EXECUTE pg_catalog.format('ALTER TABLE %I ADD CONSTRAINT %I CHECK (at >= %L AND at < %L)', part, part || '_bounds', lo, hi);
  EXECUTE pg_catalog.format('WITH m AS (DELETE FROM obop_record_change_default WHERE at >= %L AND at < %L RETURNING *) INSERT INTO %I SELECT * FROM m', lo, hi, part);
  GET DIAGNOSTICS moved = ROW_COUNT;
  EXECUTE pg_catalog.format('ALTER TABLE obop_record_change ATTACH PARTITION %I FOR VALUES FROM (%L) TO (%L)', part, lo, hi);
  EXECUTE pg_catalog.format('ALTER TABLE %I DROP CONSTRAINT %I', part, part || '_bounds');
  EXECUTE pg_catalog.format('CREATE TRIGGER %I BEFORE TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION obop_history_guard()', part || '_ro_t', part);
  PERFORM pg_catalog.set_config('obop.history_maint', 'off', true);
  RETURN 'created, moved ' || moved;
END $$;

CREATE OR REPLACE FUNCTION {schema}.obop_history_erase(
  p_model text, p_id text, p_keep text[], p_actor text, p_via jsonb, p_row_version bigint, p_at timestamptz)
RETURNS bigint LANGUAGE plpgsql SECURITY DEFINER SET search_path = {schema}, pg_temp AS $$
DECLARE n bigint; names text[];
BEGIN
  PERFORM pg_catalog.set_config('obop.history_maint', 'on', true);
  SELECT coalesce(pg_catalog.array_agg(DISTINCT e.k COLLATE "C" ORDER BY e.k COLLATE "C"), '{}') INTO names
    FROM obop_record_change c, LATERAL pg_catalog.jsonb_each(c.changed) e(k, v)
   WHERE c.model = p_model AND c.record_id = p_id AND c.op <> 'erase'
     AND e.k <> ALL (p_keep) AND NOT (e.v @> '{"t":"erased"}' OR e.v @> '{"t":"redacted"}');
  UPDATE obop_record_change c SET changed = (
      SELECT pg_catalog.jsonb_object_agg(e.k, CASE WHEN e.k = ANY (p_keep) OR e.v @> '{"t":"erased"}' OR e.v @> '{"t":"redacted"}'
                                        THEN e.v ELSE '{"t":"erased"}'::jsonb END)
        FROM pg_catalog.jsonb_each(c.changed) e(k, v)), sealed = false
   WHERE c.model = p_model AND c.record_id = p_id AND c.op <> 'erase'
     AND EXISTS (SELECT 1 FROM pg_catalog.jsonb_each(c.changed) e(k, v)
                  WHERE e.k <> ALL (p_keep) AND NOT (e.v @> '{"t":"erased"}' OR e.v @> '{"t":"redacted"}'));
  GET DIAGNOSTICS n = ROW_COUNT;
  INSERT INTO obop_record_change (at, model, record_id, op, actor, via, row_version, changed)
  VALUES (p_at, p_model, p_id, 'erase', p_actor, p_via, p_row_version,
          pg_catalog.jsonb_build_object('fields', pg_catalog.to_jsonb(names), 'entries', n));
  PERFORM pg_catalog.set_config('obop.history_maint', 'off', true);
  RETURN n;
END $$;

CREATE OR REPLACE FUNCTION {schema}.obop_history_drop_partition(p_month date, p_min_age interval)
RETURNS bigint LANGUAGE plpgsql SECURITY DEFINER SET search_path = {schema}, pg_temp AS $$
DECLARE hi timestamptz; part text; n bigint;
BEGIN
  -- The floor below only holds for an age that is zero or more: a missing one makes every
  -- comparison unknown, and a negative one moves the floor into the future. Two intervals are
  -- compared by counting a month as 30 days, but subtracted from an instant with calendar
  -- months, so an age that mixes signs ('-120 months 3600 days') can pass the first test and
  -- still move the floor into the future: the floor itself is tested as well.
  IF p_month IS NULL OR p_min_age IS NULL OR p_min_age < interval '0' THEN
    RAISE EXCEPTION 'obop_history_drop_partition needs a month and a minimum age that is not negative'
      USING errcode = '22023';
  END IF;
  IF pg_catalog.now() - p_min_age > pg_catalog.now() THEN
    RAISE EXCEPTION 'obop_history_drop_partition: the minimum age % puts the floor in the future', p_min_age
      USING errcode = '22023';
  END IF;
  hi := (pg_catalog.date_trunc('month', p_month::timestamp) + interval '1 month') AT TIME ZONE 'UTC';
  part := 'obop_record_change_p' || pg_catalog.to_char(p_month::timestamp, 'YYYYMM');
  IF pg_catalog.to_regclass(part) IS NULL THEN RETURN 0; END IF;
  IF NOT (hi <= pg_catalog.now() - p_min_age) THEN
    RAISE EXCEPTION 'partition % is younger than the retention of the models in it', part;
  END IF;
  PERFORM pg_catalog.set_config('lock_timeout', '3s', true);
  EXECUTE pg_catalog.format('SELECT count(*) FROM %I', part) INTO n;
  EXECUTE pg_catalog.format('DROP TABLE %I', part);
  RETURN n;
END $$;

REVOKE ALL ON FUNCTION {schema}.obop_history_ensure_partition(date) FROM PUBLIC;
REVOKE ALL ON FUNCTION {schema}.obop_history_erase(text, text, text[], text, jsonb, bigint, timestamptz) FROM PUBLIC;
REVOKE ALL ON FUNCTION {schema}.obop_history_drop_partition(date, interval) FROM PUBLIC;
"##;

/// The relations the part leaves behind (tables, sequence and indexes), as the server resolves the
/// names (search path).
pub(crate) const RELATIONS: [&str; 8] = [
    "obop_record_change_seq",
    "obop_record_change",
    "obop_record_change_default",
    "obop_record_change_pkey",
    "obop_record_change_default_pkey",
    "obop_record_change_rec",
    "obop_record_change_feed",
    "obop_record_change_sealed",
];

/// The functions of the part, by signature.
pub(crate) const FUNCTIONS: [&str; 4] = [
    "obop_history_guard()",
    "obop_history_ensure_partition(date)",
    "obop_history_erase(text, text, text[], text, jsonb, bigint, timestamptz)",
    "obop_history_drop_partition(date, interval)",
];

/// The SQL of the part for `schema` (the schema as `quote_ident` writes it). The three places
/// are filled in one pass, so that nothing a name contains is taken for a place.
fn render(schema: &str) -> String {
    let mut tag = String::from("obop_do");
    while schema.contains(&format!("${tag}$")) {
        tag.push('x');
    }
    let places = [
        ("{schema_lit}", schema.replace('\'', "''")),
        ("{schema}", schema.to_string()),
        ("{do}", format!("${tag}$")),
    ];
    let mut out = String::with_capacity(SQL.len() + 256);
    let mut rest = SQL;
    'scan: while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        for (name, value) in &places {
            if let Some(after) = rest.strip_prefix(name) {
                out.push_str(value);
                rest = after;
                continue 'scan;
            }
        }
        out.push('{');
        rest = &rest[1..];
    }
    out.push_str(rest);
    out
}

fn check(c: &mut Client) -> Result<bool> {
    if !schema::present(c, &RELATIONS)? {
        return Ok(false);
    }
    let functions = c
        .query_one(
            "SELECT bool_and(to_regprocedure(f) IS NOT NULL) FROM unnest($1::text[]) AS t(f)",
            &[&FUNCTIONS.as_slice()],
        )
        .map_err(|e| describe(&e))?;
    if !functions.get::<_, Option<bool>>(0).unwrap_or(false) {
        return Ok(false);
    }
    // The guard on the table, on the statement that empties it, and on the default partition.
    let triggers: i64 = c
        .query_one(
            "SELECT count(*) FROM pg_trigger t
              WHERE (t.tgname = 'obop_record_change_ro'
                       AND t.tgrelid = to_regclass('obop_record_change'))
                 OR (t.tgname = 'obop_record_change_ro_t'
                       AND t.tgrelid IN (to_regclass('obop_record_change'),
                                         to_regclass('obop_record_change_default')))",
            &[],
        )
        .map_err(|e| describe(&e))?
        .get(0);
    Ok(triggers == 3)
}

/// The part. Not required at start: nothing needs the history until a model is tracked.
pub(crate) const PART: Part = Part {
    name: "history",
    render,
    check,
    audit: Some(audit),
    required: false,
};

/// Does the table exist (as the server resolves the name)? `Err` when the question failed.
fn table_exists(c: &mut Client) -> PR<bool> {
    Ok(
        c.query_one("SELECT to_regclass('obop_record_change') IS NOT NULL", &[])?
            .get(0),
    )
}

const NOT_INSTALLED: &str = "the change history is not installed in this database (the table \
     obop_record_change is missing): install it with a role that may create tables \
     (obop_storage::schema::owner_install, or the migration step of the history), or start the \
     process with permission to create the schema";

/// `NOT_INSTALLED`, and why this process could not make the part itself, if it tried. When the
/// reason is a fault of the store (the connection broke, a time limit ran out, every connection
/// was busy), the text starts with that fault's code and says to try again: installing it with
/// another role is not the remedy for those.
fn not_installed(why: Option<String>) -> String {
    match why {
        Some(why) if Fault::of_message(&why).is_some() => format!(
            "{why}; the change history is not installed yet and this process could not make it \
             just now (the table obop_record_change is missing): try again"
        ),
        Some(why) => {
            format!("{NOT_INSTALLED} (this process tried to install it and could not: {why})")
        }
        None => NOT_INSTALLED.to_string(),
    }
}

/// How long after a failed install this process leaves the part alone: a refusal by the server
/// (the role may not create) does not change by itself, a fault of the store is over in moments.
fn install_backoff(why: &str) -> Duration {
    if Fault::of_message(why).is_some() {
        Duration::from_secs(1)
    } else {
        Duration::from_secs(30)
    }
}

// ---- what a process knows of the history ------------------------------------------------------------

/// The state of one `Pg` backend about the history. Everything here can be wrong without harm:
/// the worst a stale value does is one more catalogue lookup or one more `ensure` call.
pub(crate) struct State {
    ddl: Ddl,
    /// The part is there. Set at start-up, or when a later lookup finds it.
    ready: AtomicBool,
    /// When a missing part was last looked for (at most once a second).
    probed: Mutex<Option<Instant>>,
    /// The instant, in microseconds, from which the partitions of this process's last `ensure` no
    /// longer cover the current month.
    covered_until: AtomicI64,
    /// When `ensure` may be tried again after it failed.
    retry_after: Mutex<Option<Instant>>,
    /// Why the last `ensure` failed, if it did.
    problem: Mutex<Option<String>>,
    /// When the part may be made again after this process failed to make it (its role may not
    /// create tables), and why it failed.
    install_retry: Mutex<Option<(Instant, String)>>,
    /// Tests: the clock the stamps read instead of the server's.
    clock: Option<Clock>,
}

impl State {
    pub(crate) fn new(ddl: Ddl, ready: bool) -> Self {
        Self {
            ddl,
            ready: AtomicBool::new(ready),
            probed: Mutex::new(None),
            covered_until: AtomicI64::new(i64::MIN),
            retry_after: Mutex::new(None),
            problem: Mutex::new(None),
            install_retry: Mutex::new(None),
            clock: None,
        }
    }

    /// Stamp with `clock` instead of the database's (tests; the statements still run).
    #[cfg(test)]
    pub(crate) fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = Some(clock);
        self
    }

    /// Does the part exist? Looks again, at most once a second, while it did not.
    fn require(&self, c: &mut Client) -> Result<()> {
        if self.ready.load(Ordering::Acquire) {
            return Ok(());
        }
        {
            let mut last = self.probed.lock().unwrap_or_else(|e| e.into_inner());
            if last.is_some_and(|t| t.elapsed() < Duration::from_secs(1)) {
                return Err(self.refusal());
            }
            *last = Some(Instant::now());
        }
        if check(c)? {
            self.ready.store(true, Ordering::Release);
            Ok(())
        } else {
            Err(self.refusal())
        }
    }

    /// The refusal of a write that needs the part when it is not there. When the reason this
    /// process could not make it is a fault of the store, that fault is the request's own now
    /// (it is refused for it): noted, so that the answer is the 503 that can be tried again.
    fn refusal(&self) -> String {
        let why = self.install_problem();
        if let Some(f) = why.as_deref().and_then(Fault::of_message) {
            crate::fault::note(f);
        }
        not_installed(why)
    }

    /// The `Ddl` this process was opened with.
    pub(crate) fn ddl(&self) -> Ddl {
        self.ddl
    }

    /// Why this process could not make the part, if it tried and failed.
    pub(crate) fn install_problem(&self) -> Option<String> {
        self.install_retry
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|(_, why)| why.clone())
    }

    /// The first write that keeps history on a database that does not have the part yet: under
    /// `Ddl::IfMissing` this process makes it, outside the transaction of the write and at most
    /// once in 30 seconds after a failure. A role that may not create tables fails here and the
    /// write is refused with the reason by [`State::require`]; nothing else is affected, and a
    /// process that never writes history never gets here. Under `Never` nothing is made.
    pub(crate) fn install_on_demand(&self, main: &PgPool) {
        if self.ddl != Ddl::IfMissing {
            return;
        }
        if self
            .install_retry
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .is_some_and(|(t, _)| Instant::now() < *t)
        {
            return;
        }
        // Housekeeping on nobody's behalf: a refusal must not mark the request that happened to
        // write first (see `fault::detached`).
        match crate::fault::detached(|| self.ensure_schema(main)) {
            Ok(()) => {
                *self.install_retry.lock().unwrap_or_else(|e| e.into_inner()) = None;
            }
            Err(why) => {
                let wait = install_backoff(&why);
                *self.install_retry.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some((Instant::now() + wait, why));
            }
        }
    }

    fn now_micros(&self) -> i64 {
        self.clock
            .as_ref()
            .map_or_else(sysfields::host_micros, Clock::peek)
    }

    /// Before a batch that writes history: make the partitions of the current month and the next,
    /// once a month. Outside the transaction of the batch, so that the lock `ATTACH PARTITION`
    /// takes on the default partition is never held by a transaction that waits for another. A
    /// failure is remembered, not raised: the entries land in the default partition, where
    /// `history_ensure_partition` finds them.
    pub(crate) fn before_batch(&self, main: &PgPool) {
        if !self.ready.load(Ordering::Acquire) {
            self.install_on_demand(main);
            if !self.ready.load(Ordering::Acquire) {
                return;
            }
        }
        let now = self.now_micros();
        if now < self.covered_until.load(Ordering::Relaxed) {
            return;
        }
        if self
            .retry_after
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some_and(|t| Instant::now() < t)
        {
            return;
        }
        let (y, m) = year_month(now);
        let base = format!("{y:04}-{m:02}-01");
        // Housekeeping whose failure is remembered, not raised: it must not mark the request that
        // happened to write first as having met a storage fault (a later write of the same
        // request would be refused, its answer made a 503), see `fault::detached`.
        let done = crate::fault::detached(|| {
            main.run(Class::Batch, |c| {
                c.query(
                    "SELECT obop_history_ensure_partition(($1::text::date + g * interval '1 month')::date)
                       FROM generate_series(0, 1) g",
                    &[&base],
                )?;
                Ok(())
            })
        });
        match done {
            Ok(()) => {
                let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
                self.covered_until
                    .store(month_start(ny, nm), Ordering::Relaxed);
                *self.problem.lock().unwrap_or_else(|e| e.into_inner()) = None;
            }
            Err(why) => {
                *self.problem.lock().unwrap_or_else(|e| e.into_inner()) = Some(why);
                *self.retry_after.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some(Instant::now() + Duration::from_secs(30));
            }
        }
    }

    /// Whether a batch of `ops` writes history: some op is on a model that keeps it, or erases.
    pub(crate) fn wants(ops: &[crate::Op], tracking: &crate::Tracking) -> bool {
        !tracking.is_empty()
            && ops.iter().any(|o| {
                matches!(o, crate::Op::EraseHistory { .. })
                    || tracking.get(o.model()).is_some_and(|t| t.history)
            })
    }

    /// `ensure_history_schema`: create the part under `IfMissing`, or require it under `Never`.
    pub(crate) fn ensure_schema(&self, main: &PgPool) -> Result<()> {
        let ddl = self.ddl;
        main.run(Class::Batch, |c| {
            schema::with_lock(c, |c| schema::ensure_strict(c, &[PART], ddl))?;
            Ok(())
        })?;
        self.ready.store(true, Ordering::Release);
        Ok(())
    }
}

/// `(year, month)` of an instant given in microseconds.
fn year_month(us: i64) -> (i64, i64) {
    let t = fmt_micros(us);
    (
        t[0..4].parse().unwrap_or(1970),
        t[5..7].parse().unwrap_or(1),
    )
}

/// The first instant of a month, in microseconds (the end of time for a year past 9999).
fn month_start(y: i64, m: i64) -> i64 {
    parse_micros(&format!("{y:04}-{m:02}-01T00:00:00.000000Z")).unwrap_or(i64::MAX)
}

/// The month `YYYY-MM` of a canonical instant, as the first day of it for SQL.
fn first_of_month(at: &str) -> String {
    format!("{}-01", &at[0..7])
}

// ---- inside a transaction ----------------------------------------------------------------------------

/// The record, locked, and the database's clock, in one statement. The clock is evaluated after
/// the lock is granted, so a writer that waited for the lock stamps later than the one it waited
/// for. A record that is not there gives no data and the clock.
const LOCK_AND_CLOCK: &str =
    "SELECT t.data, (extract(epoch FROM clock_timestamp()) * 1000000)::bigint
     FROM (VALUES (1)) v(x)
     LEFT JOIN LATERAL (SELECT data FROM obop_record WHERE model = $1 AND id = $2 FOR UPDATE) t
       ON true";

const FLUSH: &str = "INSERT INTO obop_record_change
       (at, model, record_id, op, actor, via, row_version, changed, sealed, batch_id, run_id)
     SELECT timestamptz 'epoch' + x.at * interval '1 microsecond', x.m, x.r, x.op, x.actor, x.via,
            x.v, x.ch, x.s, x.b, x.run
       FROM unnest($1::bigint[], $2::text[], $3::text[], $4::text[], $5::text[], $6::jsonb[],
                   $7::bigint[], $8::jsonb[], $9::boolean[], $10::text[], $11::text[])
            WITH ORDINALITY AS x(at, m, r, op, actor, via, v, ch, s, b, run, ord)
      ORDER BY x.ord";

/// As [`FLUSH`], for entries that come with their number and no transaction (a restore).
const FLUSH_IMPORTED: &str = "INSERT INTO obop_record_change
       (seq, at, txid, model, record_id, op, actor, via, row_version, changed, sealed, batch_id,
        run_id)
     SELECT x.seq, timestamptz 'epoch' + x.at * interval '1 microsecond', '0'::xid8, x.m, x.r,
            x.op, x.actor, x.via, x.v, x.ch, x.s, x.b, x.run
       FROM unnest($1::bigint[], $2::bigint[], $3::text[], $4::text[], $5::text[], $6::text[],
                   $7::jsonb[], $8::bigint[], $9::jsonb[], $10::boolean[], $11::text[],
                   $12::text[])
            WITH ORDINALITY AS x(seq, at, m, r, op, actor, via, v, ch, s, b, run, ord)
      ORDER BY x.ord";

/// The entries as the columns of the statements above.
#[derive(Default)]
struct Columns {
    seq: Vec<i64>,
    at: Vec<i64>,
    model: Vec<String>,
    record_id: Vec<String>,
    op: Vec<String>,
    actor: Vec<String>,
    via: Vec<J>,
    version: Vec<i64>,
    changed: Vec<J>,
    sealed: Vec<bool>,
    batch: Vec<Option<String>>,
    run: Vec<Option<String>>,
}

impl Columns {
    fn of(rows: &[ChangeRow]) -> Result<Columns> {
        let mut c = Columns::default();
        for r in rows {
            c.seq.push(r.seq);
            c.at.push(parse_micros(&r.at).ok_or_else(|| {
                format!(
                    "the entry of {} {} has no canonical instant",
                    r.model, r.record_id
                )
            })?);
            c.model.push(r.model.clone());
            c.record_id.push(r.record_id.clone());
            c.op.push(r.op.clone());
            c.actor.push(r.actor.clone());
            c.via.push(J::Object(r.via.clone()));
            c.version.push(r.row_version);
            c.changed.push(J::Object(r.changed.clone()));
            c.sealed.push(r.sealed);
            c.batch.push(r.batch_id.clone());
            c.run.push(r.run_id.clone());
        }
        Ok(c)
    }
}

/// What a transaction of `apply_tracked` keeps of the history: the clock it learnt.
pub(crate) struct Tx<'a> {
    st: &'a State,
    /// The latest clock reading of this transaction (taken right after a row lock).
    clock: Option<i64>,
    /// The last instant handed out: stamps of one transaction never repeat.
    last: i64,
}

impl<'a> Tx<'a> {
    pub(crate) fn new(st: &'a State) -> Self {
        Self {
            st,
            clock: None,
            last: i64::MIN,
        }
    }

    /// `TxView::tx_get_clock`: the record, locked, and the clock after the lock.
    pub(crate) fn get_clock(
        &mut self,
        c: &mut Client,
        model: &str,
        id: &str,
    ) -> Result<Option<Fields>> {
        let row = c
            .query_one(LOCK_AND_CLOCK, &[&model, &id])
            .map_err(|e| describe(&e))?;
        let at: i64 = row.try_get(1).map_err(|e| describe(&e))?;
        self.clock = Some(self.clock.map_or(at, |c| c.max(at)));
        let data: Option<J> = row.try_get(0).map_err(|e| describe(&e))?;
        data.map(pg_fields).transpose()
    }

    /// `TxView::tx_now`: the clock of the last lock, or the database's if nothing was locked; at
    /// least a microsecond after the last answer.
    pub(crate) fn now(&mut self, c: &mut Client) -> Result<i64> {
        if let Some(clock) = &self.st.clock {
            return Ok(clock.tick());
        }
        let base = match self.clock {
            Some(t) => t,
            None => {
                let t: i64 = c
                    .query_one(
                        "SELECT (extract(epoch FROM clock_timestamp()) * 1000000)::bigint",
                        &[],
                    )
                    .map_err(|e| describe(&e))?
                    .get(0);
                self.clock = Some(t);
                t
            }
        };
        let t = base.max(self.last.saturating_add(1));
        self.last = t;
        Ok(t)
    }

    /// `TxView::tx_history`: the entries of the batch, in order, in one statement.
    pub(crate) fn history(&mut self, c: &mut Client, rows: &[ChangeRow]) -> Result<()> {
        self.st.require(c)?;
        let k = Columns::of(rows)?;
        c.execute(
            FLUSH,
            &[
                &k.at,
                &k.model,
                &k.record_id,
                &k.op,
                &k.actor,
                &k.via,
                &k.version,
                &k.changed,
                &k.sealed,
                &k.batch,
                &k.run,
            ],
        )
        .map(|_| ())
        .map_err(|e| describe(&e))
    }

    /// `TxView::tx_scrub`: erase the values of the entries of a record (the database function
    /// does it under the maintenance setting) and append the entry that says so.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn scrub(
        &mut self,
        c: &mut Client,
        model: &str,
        id: &str,
        keep: &[String],
        actor: &str,
        via: &Fields,
        row_version: i64,
        at: i64,
    ) -> Result<u64> {
        self.st.require(c)?;
        let via = J::Object(via.clone());
        let n: i64 = c
            .query_one(
                "SELECT obop_history_erase($1, $2, $3::text[], $4, $5::jsonb, $6::bigint,
                        timestamptz 'epoch' + $7::bigint * interval '1 microsecond')",
                &[&model, &id, &keep, &actor, &via, &row_version, &at],
            )
            .map_err(|e| describe(&e))?
            .get(0);
        Ok(n as u64)
    }
}

// ---- reads ---------------------------------------------------------------------------------------------

/// The columns of an entry, in the order [`read_change`] reads them.
///
/// The transaction number is read as text under another name. A bare `ORDER BY txid` binds to the
/// output column of that name before the table's column, and the text of a number sorts as text
/// ('10' < '9'); the feed's cursor compares the numbers, so the two disagreed whenever the numbers
/// of a page had different lengths. The sort keys of the feed name the table as well
/// ([`FEED_ORDER`]), and `no_order_by_names_an_output_column` keeps both true.
const COLUMNS: &str = "seq, (extract(epoch FROM at) * 1000000)::bigint, model, record_id, op, \
                       actor, via, row_version, changed, sealed, batch_id, run_id, \
                       txid::text AS txid_text";

/// The order of the change feed, naming the table so that it is the column that is sorted.
const FEED_ORDER: &str = "obop_record_change.txid, obop_record_change.seq";

/// The change feed's statement (`$1` the cursor's transaction number as text, `$2` its number,
/// `$3` models, `$4` operations, `$5` the earliest instant in microseconds, `$6` the limit).
pub(crate) fn feed_sql() -> String {
    format!(
        "SELECT {COLUMNS} FROM obop_record_change
          WHERE (txid, seq) > ($1::text::xid8, $2::bigint)
            AND txid < pg_snapshot_xmin(pg_current_snapshot())
            AND ($3::text[] IS NULL OR model = ANY ($3))
            AND ($4::text[] IS NULL OR op = ANY ($4))
            AND ($5::bigint IS NULL
                 OR at >= timestamptz 'epoch' + $5 * interval '1 microsecond')
          ORDER BY {FEED_ORDER} LIMIT $6"
    )
}

fn read_change(r: &postgres::Row) -> PR<ChangeRow> {
    let object = |v: J| pg_fields(v);
    let txid: String = r.try_get(12)?;
    Ok(ChangeRow {
        seq: r.try_get(0)?,
        at: fmt_micros(r.try_get(1)?),
        model: r.try_get(2)?,
        record_id: r.try_get(3)?,
        op: r.try_get(4)?,
        actor: r.try_get(5)?,
        via: object(r.try_get(6)?)?,
        row_version: r.try_get(7)?,
        changed: object(r.try_get(8)?)?,
        sealed: r.try_get(9)?,
        batch_id: r.try_get(10)?,
        run_id: r.try_get(11)?,
        txid: txid
            .parse()
            .map_err(|_| format!("the transaction number {txid} is out of range"))?,
    })
}

fn micros_of(at: &Option<String>) -> Result<Option<i64>> {
    at.as_deref()
        .map(|a| parse_micros(a).ok_or_else(|| format!("{a:?} is not a canonical instant")))
        .transpose()
}

/// The history of one record, newest first.
pub(crate) fn history(main: &PgPool, q: &HistoryQuery) -> Result<Vec<ChangeRow>> {
    let since = micros_of(&q.since_at)?;
    let limit = i64::try_from(q.limit).unwrap_or(i64::MAX);
    main.read(Class::List, |c| {
        // A database without the table has never recorded a change.
        if !table_exists(c)? {
            return Ok(Vec::new());
        }
        let rows = c.query(
            &format!(
                "SELECT {COLUMNS} FROM obop_record_change
                  WHERE model = $1 AND record_id = $2
                    AND ($3::bigint IS NULL OR seq < $3)
                    AND ($4::text IS NULL OR op = $4)
                    AND ($5::bigint IS NULL OR at >= timestamptz 'epoch' + $5 * interval '1 microsecond')
                  ORDER BY seq DESC LIMIT $6"
            ),
            &[&q.model, &q.record_id, &q.before_seq, &q.op, &since, &limit],
        )?;
        rows.iter().map(read_change).collect()
    })
}

/// What the feed binds its cursors to: the next transaction number the cluster will give out
/// (`$xmax`), and whether this role may read the control data that identifies the cluster. The
/// control data is for any role unless the server's owner has withdrawn it; then only the check
/// of the numbers remains.
///
/// Whether the role may is asked first and apart ([`IDENTITY`] is sent only when it may): the
/// server checks the right to call a function when a statement starts, not when the branch that
/// calls it is reached, so one statement with a `CASE` around the calls would fail for the role
/// that must not make them instead of answering NULL.
const XMAX: &str = "SELECT pg_snapshot_xmax(pg_current_snapshot())::text,
       has_function_privilege('pg_control_system()', 'EXECUTE')
         AND has_function_privilege('pg_control_checkpoint()', 'EXECUTE')";

/// The identity of the cluster with the time line it is on: its system identifier and the time
/// line of its last checkpoint, which follows a promotion within moments.
const IDENTITY: &str = "SELECT (SELECT system_identifier::text FROM pg_control_system())
       || '/' || (SELECT timeline_id::text FROM pg_control_checkpoint())";

/// The epoch the cursors of the feed carry: the stored one (renewed by a restore), followed by a
/// digest of the cluster and time line they were made on, where that is known. Letters and
/// digits only, as the text of a cursor allows.
pub(crate) fn epoch_of(stored: &str, cluster: Option<&str>) -> String {
    use sha2::{Digest, Sha256};
    match cluster {
        None => stored.to_string(),
        Some(identity) => {
            let digest = Sha256::digest(identity.as_bytes());
            let short: String = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();
            format!("{stored}{short}")
        }
    }
}

/// How far the clock of the oldest entry the feed holds back is behind: entries numbered at or
/// above the oldest transaction that is still open on the server and below the next number it
/// will give out. Numbers beyond that belong to no transaction of this cluster ([`FOREIGN`]).
const HELD_BACK: &str = "SELECT (extract(epoch FROM clock_timestamp() - min(at)) * 1000)::bigint
       FROM obop_record_change
      WHERE txid >= pg_snapshot_xmin(pg_current_snapshot())
        AND txid < pg_snapshot_xmax(pg_current_snapshot())";

/// How many entries carry a transaction number that this cluster has not given out yet: a dump
/// loaded into a new cluster, or replication from another, keeps the numbers as plain data, and
/// the counter of the new one may be lower. The feed shows an entry only below the oldest open
/// transaction, so it would not show these until the counter passed them, and then after newer
/// entries of the same record.
pub(crate) const FOREIGN: &str = "SELECT count(*) FROM obop_record_change
      WHERE txid >= pg_snapshot_xmax(pg_current_snapshot())";

/// The warning of [`schema::inspect`] about entries counted by [`FOREIGN`], or none.
fn audit(c: &mut Client) -> Result<Vec<schema::Finding>> {
    let n: i64 = c.query_one(FOREIGN, &[]).map_err(|e| describe(&e))?.get(0);
    if n == 0 {
        return Ok(Vec::new());
    }
    Ok(vec![schema::Finding {
        code: "history_foreign_numbers",
        object: "history".to_string(),
        detail: format!(
            "{n} entries of the change history carry a transaction number that this cluster has \
             not given out yet (the database was loaded from a dump or replicated from another \
             cluster), so the change feed cannot show them; renumber them as docs/DEPLOY.md \
             \"Moving a database to another cluster\" says"
        ),
        error: false,
    }])
}

/// One page of the feed: in `(txid, seq)` order, only what lies below the oldest transaction that
/// holds a transaction number and is still open anywhere in the cluster (its entries, and the
/// entries of everything numbered after it, are held back; none is lost). Entries numbered beyond
/// the cluster ([`FOREIGN`]) are not shown and are counted apart.
pub(crate) fn changes(pg: &Pg, q: &ChangeQuery) -> Result<ChangePage> {
    let limit = history::feed_limit(q)?;
    let since = micros_of(&q.since_at)?;
    let stored = history::feed_epoch(pg)?;
    let models = (!q.models.is_empty()).then(|| q.models.clone());
    let ops = (!q.ops.is_empty()).then(|| q.ops.clone());
    let (rows, lag, foreign, epoch) = pg.main.read_tx(Class::List, |tx| {
        // The cursors are made for this cluster and this time line of it: the order of the feed
        // is that of the transaction numbers of the server, which another cluster (a dump loaded
        // into a new one) or an earlier time line (a point in time recovery, a failover to a
        // standby that lagged) counts again from other numbers.
        let cluster = tx.query_one(XMAX, &[])?;
        let xmax: String = cluster.try_get(0)?;
        let xmax: u64 = xmax.parse().unwrap_or(u64::MAX);
        let identity: Option<String> = if cluster.try_get::<_, bool>(1)? {
            Some(tx.query_one(IDENTITY, &[])?.try_get(0)?)
        } else {
            None
        };
        let epoch = epoch_of(&stored, identity.as_deref());
        let exists: bool = tx
            .query_one("SELECT to_regclass('obop_record_change') IS NOT NULL", &[])?
            .get(0);
        // The newest number given out: a cursor beyond it comes from another time line. The
        // function needs USAGE on the sequence, which the writers have; reading the sequence
        // relation itself would need SELECT.
        let newest: i64 = if exists {
            tx.query_one(
                "SELECT coalesce(pg_catalog.pg_sequence_last_value('obop_record_change_seq'::regclass), 0)",
                &[],
            )?
            .get(0)
        } else {
            0
        };
        let (txid, seq) = history::resume(&epoch, q.after.as_ref(), newest)?;
        // An entry the cursor stands for has committed, so its number is below the next one this
        // cluster will give out. At or above it, the cursor was made by another cluster or on a
        // time line that was left (the numbers there ran further than they do here).
        if txid > 0 && u64::try_from(txid).is_ok_and(|t| t >= xmax) {
            return Err(history::cursor_expired(
                "this cursor was made on another time line of the database (it was moved to \
                 another server, recovered to an earlier point, or failed over to a standby \
                 that lagged); read the feed again from the start",
            )
            .into());
        }
        if !exists || limit == 0 {
            return Ok((Vec::new(), None, None, epoch));
        }
        let rows = tx.query(
            &feed_sql(),
            &[
                &txid.to_string(),
                &seq,
                &models,
                &ops,
                &since,
                &(limit as i64),
            ],
        )?;
        let rows: Vec<ChangeRow> = rows.iter().map(read_change).collect::<PR<_>>()?;
        // Only when the page reached the horizon can something lie beyond it.
        let (lag, foreign): (Option<i64>, Option<i64>) = if rows.len() < limit {
            (
                tx.query_one(HELD_BACK, &[])?.get(0),
                Some(tx.query_one(FOREIGN, &[])?.get(0)),
            )
        } else {
            (None, None)
        };
        Ok((rows, lag, foreign, epoch))
    })?;
    let mut page = history::page(&epoch, q, rows);
    page.horizon_lag_ms = lag.map(|ms| ms.max(0));
    page.foreign_entries = foreign;
    Ok(page)
}

/// Counts and ages, without reading the entries.
pub(crate) fn stats(pg: &Pg) -> Result<HistoryStats> {
    let mut s = pg.main.read(Class::Aggregate, |c| {
        if !table_exists(c)? {
            return Ok(HistoryStats::default());
        }
        let parts = c.query(
            "SELECT c.relname::text, c.reltuples::bigint
               FROM pg_inherits i JOIN pg_class c ON c.oid = i.inhrelid
              WHERE i.inhparent = to_regclass('obop_record_change')",
            &[],
        )?;
        let mut out = HistoryStats::default();
        for p in parts {
            let name: String = p.try_get(0)?;
            let estimate: i64 = p.try_get(1)?;
            let is_default = name == "obop_record_change_default";
            // The default partition is counted exactly (it should be empty); so is a partition
            // the server has never analysed.
            let n: i64 = if is_default || estimate < 0 {
                let quoted = name.replace('"', "\"\"");
                // A partition is its own table for privileges: the grants of the parent do not
                // reach it (a query through the parent is checked against the parent only).
                match c.query_one(&format!("SELECT count(*) FROM \"{quoted}\""), &[]) {
                    Ok(r) => r.get(0),
                    Err(e) if e.code() == Some(&SqlState::INSUFFICIENT_PRIVILEGE) => {
                        return Err(format!(
                            "the statistics count the rows of the partition {name}, and this role \
                             may not read it: grant SELECT on the partitions of \
                             obop_record_change as well as on the table itself \
                             (docs/DEPLOY.md, change history): {}",
                            describe(&e)
                        )
                        .into());
                    }
                    Err(e) => return Err(e.into()),
                }
            } else {
                estimate
            };
            if is_default {
                out.default_rows = n;
            }
            out.entries_estimate += n;
            out.partitions.push((name, n));
        }
        out.partitions.sort_by(|a, b| {
            let default = |n: &str| n == "obop_record_change_default";
            (default(&a.0), &a.0).cmp(&(default(&b.0), &b.0))
        });
        let oldest: Option<i64> = c
            .query_one(
                "SELECT (extract(epoch FROM min(at)) * 1000000)::bigint FROM obop_record_change",
                &[],
            )?
            .get(0);
        out.oldest_at = oldest.map(fmt_micros);
        out.horizon_lag_ms = c
            .query_one(HELD_BACK, &[])?
            .get::<_, Option<i64>>(0)
            .map_or(0, |ms| ms.max(0));
        out.foreign_rows = c.query_one(FOREIGN, &[])?.get(0);
        Ok(out)
    })?;
    s.maintenance_error = pg
        .hist
        .problem
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    Ok(s)
}

// ---- maintenance primitives ---------------------------------------------------------------------------

/// `YYYY-MM` as the first day of that month (`YYYY-MM-01`), or the reason it is not one.
fn month_day(month: &str) -> Result<String> {
    let ok = month.len() == 7
        && month.as_bytes()[4] == b'-'
        && month[0..4].bytes().all(|b| b.is_ascii_digit())
        && month[5..7].bytes().all(|b| b.is_ascii_digit())
        && (1..=12).contains(&month[5..7].parse::<u32>().unwrap_or(0));
    if ok {
        Ok(format!("{month}-01"))
    } else {
        Err(format!("{month:?} is not a month (YYYY-MM)"))
    }
}

pub(crate) fn ensure_partition(pg: &Pg, month: &str) -> Result<PartitionEnsured> {
    let day = month_day(month)?;
    let said: String = pg.main.run(Class::Batch, |c| {
        pg.hist.require(c)?;
        Ok(c.query_one(
            "SELECT obop_history_ensure_partition($1::text::date)",
            &[&day],
        )?
        .get(0))
    })?;
    Ok(PartitionEnsured {
        created: said.starts_with("created"),
        moved: said
            .rsplit(' ')
            .next()
            .and_then(|n| n.parse().ok())
            .unwrap_or(0),
    })
}

pub(crate) fn drop_partition(pg: &Pg, month: &str, min_age_days: u32) -> Result<u64> {
    let day = month_day(month)?;
    let days = i32::try_from(min_age_days).unwrap_or(i32::MAX);
    let n: i64 = pg.main.run(Class::Batch, |c| {
        pg.hist.require(c)?;
        Ok(c.query_one(
            "SELECT obop_history_drop_partition($1::text::date, $2::int * interval '1 day')",
            &[&day, &days],
        )?
        .get(0))
    })?;
    Ok(n as u64)
}

// ---- the stamp of old records ---------------------------------------------------------------------------

/// A record that has no usable `row_version`: absent, null, or not a whole number from 1 to the
/// largest `i64` (what `sysfields::row_version` calls "never stamped"). The read of the field is
/// the integer slot of `keyexpr`, the one writer of such expressions.
fn unstamped() -> String {
    let v = keyexpr::field_expr(Slot::Int, "row_version", None)
        .expect("row_version is a plain field name");
    format!("NOT coalesce({v} >= 1, false)")
}

/// Stamp up to `$3` unversioned records of the models `$1`: the creation time from a UUIDv7 id
/// (else `$2`), version 1, no actor. Rows another transaction holds are skipped, never waited
/// for, so the stamp cannot deadlock with a writer; the caller repeats until nothing is left.
fn backfill_sql() -> String {
    let unstamped = unstamped();
    format!(
        "WITH b AS (SELECT seq, id FROM obop_record
                     WHERE model = ANY ($1) AND {unstamped}
                     ORDER BY seq LIMIT $3 FOR UPDATE SKIP LOCKED),
         s AS (SELECT b.seq,
                      CASE WHEN b.id ~ '^[0-9a-f]{{8}}-[0-9a-f]{{4}}-7[0-9a-f]{{3}}-[89ab][0-9a-f]{{3}}-[0-9a-f]{{12}}$'
                           THEN to_char((timestamptz 'epoch' + least(('x' || substr(replace(b.id, '-', ''), 1, 12))::bit(48)::bigint, 253402300799999) * interval '1 millisecond') AT TIME ZONE 'UTC',
                                        'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"000Z\"')
                           ELSE $2::text END AS ts
                 FROM b)
         UPDATE obop_record r
            SET data = (r.data - 'created_by' - 'updated_by')
                       || jsonb_build_object('created_at', s.ts, 'updated_at', s.ts,
                                             'row_version', 1, 'sys_backfilled', true)
           FROM s WHERE r.seq = s.seq RETURNING r.seq"
    )
}

pub(crate) fn backfill(
    main: &PgPool,
    models: &[String],
    fallback_at: &str,
    batch: usize,
) -> Result<Backfilled> {
    if parse_micros(fallback_at).is_none() {
        return Err(format!("{fallback_at:?} is not a canonical instant"));
    }
    let limit = i64::try_from(batch.max(1)).unwrap_or(i64::MAX);
    let stamp = backfill_sql();
    let count = format!(
        "SELECT count(*) FROM obop_record WHERE model = ANY ($1) AND {}",
        unstamped()
    );
    main.tx_retry(Class::Batch, |c| {
        let stamped = c.query(&stamp, &[&models, &fallback_at, &limit])?.len();
        let remaining: i64 = c.query_one(&count, &[&models])?.get(0);
        Ok(Backfilled {
            stamped,
            remaining: remaining as usize,
        })
    })
}

// ---- backup and restore ----------------------------------------------------------------------------------

/// Call from inside the repeatable-read transaction of `export`, after the records: the entries,
/// as the rows `("record_change", "<seq>", ChangeRow::to_fields())`, paged by the primary key
/// `(at, seq)` (paging by `seq` alone would visit every partition for each page).
pub(crate) fn export(
    tx: &mut postgres::Transaction<'_>,
    batch: i64,
    sink: &mut RowSink<'_>,
) -> PR<()> {
    if !tx
        .query_one("SELECT to_regclass('obop_record_change') IS NOT NULL", &[])?
        .get::<_, bool>(0)
    {
        return Ok(());
    }
    let mut after: Option<(i64, i64)> = None;
    loop {
        let rows = match after {
            None => tx.query(
                &format!(
                    "SELECT {COLUMNS} FROM obop_record_change ORDER BY at, seq LIMIT $1"
                ),
                &[&batch],
            )?,
            Some((at, seq)) => tx.query(
                &format!(
                    "SELECT {COLUMNS} FROM obop_record_change
                      WHERE (at, seq) > (timestamptz 'epoch' + $1::bigint * interval '1 microsecond', $2::bigint)
                      ORDER BY at, seq LIMIT $3"
                ),
                &[&at, &seq, &batch],
            )?,
        };
        let Some(last) = rows.last() else {
            return Ok(());
        };
        after = Some((last.try_get(1)?, last.try_get(0)?));
        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let e = read_change(r)?;
            out.push((
                sysfields::ENTRY_MODEL.to_string(),
                e.seq.to_string(),
                e.to_fields(),
            ));
        }
        sink(&out)?;
    }
}

/// The number of entries (0 where there is no table), for `total`.
pub(crate) fn count(c: &mut Client) -> PR<usize> {
    if !table_exists(c)? {
        return Ok(0);
    }
    let n: i64 = c
        .query_one("SELECT count(*) FROM obop_record_change", &[])?
        .get(0);
    Ok(n as usize)
}

/// A restore, as seen by the history: entries come in among the records as rows of the model
/// `record_change` that carry the marker `$kind`; they go to the table with the number they had,
/// their partitions are made first, and the sequence is moved past the highest number so that
/// new entries sort after the restored ones. Run inside the transaction of `Pg::import`.
pub(crate) struct Import {
    exists: bool,
    /// The process may make the part (`Ddl::IfMissing`): a backup that carries entries makes it.
    may_create: bool,
    /// An entry came in and the table is not there: the part is made before the first flush.
    needs_install: bool,
    /// The table held entries when the restore began (and is to be emptied).
    had_rows: bool,
    pending: Vec<ChangeRow>,
    entries: u64,
    newest: i64,
}

impl Import {
    /// Look at what the restore meets, and refuse early, with the reason, what cannot work.
    pub(crate) fn preflight(c: &mut Client, wipe: bool, ddl: Ddl) -> PR<Import> {
        let exists = table_exists(c)?;
        let mut had_rows = false;
        if wipe && exists {
            had_rows = c
                .query_one("SELECT EXISTS (SELECT 1 FROM obop_record_change)", &[])?
                .get(0);
            if had_rows {
                // The guard lets only the owner of the table empty it.
                let (me, owner): (String, String) = {
                    let r = c.query_one(
                        "SELECT current_user::text,
                                (SELECT pg_get_userbyid(relowner)::text FROM pg_class
                                  WHERE oid = to_regclass('obop_record_change'))",
                        &[],
                    )?;
                    (r.try_get(0)?, r.try_get(1)?)
                };
                if me != owner {
                    return Err(format!(
                        "restoring over a database that holds change history needs the role that \
                         owns the table obop_record_change ({owner}); this process connects as {me}"
                    )
                    .into());
                }
            }
        }
        Ok(Import {
            exists,
            may_create: ddl == Ddl::IfMissing,
            needs_install: false,
            had_rows,
            pending: Vec::new(),
            entries: 0,
            newest: 0,
        })
    }

    /// Empty the history (the records are wiped by the caller). `TRUNCATE`, not `DELETE`: a
    /// history can be large, and the guard allows either under the maintenance setting.
    pub(crate) fn wipe(&self, c: &mut Client) -> PR<()> {
        if self.had_rows {
            c.batch_execute(
                "SELECT set_config('obop.history_maint', 'on', true);
                 TRUNCATE obop_record_change;
                 SELECT set_config('obop.history_maint', 'off', true)",
            )?;
        }
        Ok(())
    }

    /// Take the row if it is an entry (queued until [`Import::flush`]); `false`: an ordinary
    /// record, for the caller to insert.
    pub(crate) fn take(&mut self, model: &str, id: &str, fields: &Fields) -> Result<bool> {
        if !sysfields::is_entry_row(model, fields) {
            return Ok(false);
        }
        let seq = id
            .parse::<i64>()
            .ok()
            .filter(|s| *s >= 1)
            .ok_or_else(|| format!("record_change {id}: the id is not a number"))?;
        if !self.exists {
            if !self.may_create {
                return Err(format!(
                    "record_change {id}: the backup carries change history and this database has \
                     no table for it. {NOT_INSTALLED}"
                ));
            }
            self.needs_install = true;
        }
        self.pending.push(ChangeRow::from_fields(seq, fields)?);
        self.newest = self.newest.max(seq);
        self.entries += 1;
        Ok(true)
    }

    /// Write the queued entries: make the partitions their months need (best effort: an entry
    /// that no partition takes lands in the default one), then insert them.
    pub(crate) fn flush(&mut self, c: &mut Client) -> PR<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        if self.needs_install {
            // The backup carries history and the database has none yet: make the part, as the
            // start of a process would, under the schema lock and in this transaction.
            c.batch_execute(schema::LOCK_SQL)?;
            schema::ensure_parts(c, &[PART], Ddl::IfMissing)?;
            self.exists = true;
            self.needs_install = false;
        }
        let rows = std::mem::take(&mut self.pending);
        let mut months: Vec<String> = rows.iter().map(|r| first_of_month(&r.at)).collect();
        months.sort();
        months.dedup();
        c.batch_execute("SAVEPOINT obop_history_partitions")?;
        let made = c.query(
            "SELECT obop_history_ensure_partition(m)
               FROM unnest($1::text[]::date[]) AS t(m)
              WHERE m >= date '2000-01-01' AND m <= (current_date + interval '1 year')::date",
            &[&months],
        );
        match made {
            Ok(_) => c.batch_execute("RELEASE SAVEPOINT obop_history_partitions")?,
            Err(e) if crate::pgpool::connection_failed(&e) => return Err(e.into()),
            Err(_) => c.batch_execute(
                "ROLLBACK TO SAVEPOINT obop_history_partitions;
                 RELEASE SAVEPOINT obop_history_partitions",
            )?,
        }
        let k = Columns::of(&rows)?;
        let done = c.execute(
            FLUSH_IMPORTED,
            &[
                &k.seq,
                &k.at,
                &k.model,
                &k.record_id,
                &k.op,
                &k.actor,
                &k.via,
                &k.version,
                &k.changed,
                &k.sealed,
                &k.batch,
                &k.run,
            ],
        );
        match done {
            Ok(_) => Ok(()),
            Err(e) if e.code() == Some(&SqlState::UNIQUE_VIOLATION) => Err(
                "duplicate record_change: the backup repeats an entry (same instant and number)"
                    .to_string()
                    .into(),
            ),
            Err(e) => Err(e.into()),
        }
    }

    /// After the last row: no number twice, the sequence past the highest, the feed's epoch over.
    pub(crate) fn finish(self, c: &mut Client) -> PR<()> {
        if self.entries == 0 {
            return Ok(());
        }
        // The primary key is per partition: a number must not occur in two of them.
        if let Some(dup) = c.query_opt(
            "SELECT seq FROM obop_record_change GROUP BY seq HAVING count(*) > 1 LIMIT 1",
            &[],
        )? {
            let seq: i64 = dup.try_get(0)?;
            return Err(format!("duplicate record_change {seq}").into());
        }
        // Never backwards: numbers already given out stay given out.
        c.query_one(
            "SELECT setval('obop_record_change_seq',
                           greatest($1::bigint, 1,
                                    coalesce(pg_catalog.pg_sequence_last_value(
                                        'obop_record_change_seq'::regclass), 0)),
                           true)",
            &[&self.newest],
        )?;
        // Entries came in below every cursor there is: the readers of the feed start again.
        c.execute("DELETE FROM obop_kv WHERE key = $1", &[&history::EPOCH_KEY])?;
        Ok(())
    }
}
