//! System fields and change history of a record (D75), the part that does not depend on where
//! the record is stored: the timestamp format, the system keys, and the planner that memory,
//! SQLite and PostgreSQL share. A backend supplies a transaction ([`TxView`]) and a clock; the
//! planner decides what is written and what the history says.
//!
//! A *tracked* model (see [`Tracking`]) has six reserved keys inside every record:
//! `created_at`, `updated_at` (`YYYY-MM-DDTHH:MM:SS.ffffffZ`, UTC, the same bytes as the canonical
//! text of a `datetime`), `created_by`, `updated_by`, `row_version` (counts the changes, from 1) and
//! `sys_backfilled` (true on a record that predates the tracking, whose `created_at` is an
//! estimate). Nobody sets them: whatever a write supplies under those names is dropped, and the
//! planner stamps the record.
//!
//! For a tracked model a batch behaves like this:
//!
//! - A write that changes nothing (the new record equals the record in the canonical form, where
//!   system keys and nulls do not count) is not written: no new version, no entry.
//! - The first effective write of a record in a transaction stamps it (`row_version + 1`,
//!   `updated_at = max(clock, previous + 1 µs)`); later writes of the same transaction change the
//!   data only. A record that was stamped and ends the transaction as it began (A to B to A) is
//!   written back as it was and leaves no entry.
//! - With history on, the transaction appends one entry per changed record (a record created
//!   and deleted in the same transaction leaves none; one updated and then deleted leaves two).
//!   An entry says who, by which route, and for every field that changed `{b: before, a: after,
//!   t: marker}`; a field of class `secret` or an undeclared one is never stored (`redacted`),
//!   one of a sensitive class is sealed (`sealed`), a value over [`HISTORY_VALUE_MAX`] is a
//!   `large` marker with its length.
//! - `Op::Patch` can require `row_version`: an absent version reads as 1, and a mismatch is a
//!   `Conflict` whose reason is `stale_version:<current>`.

use crate::atomic::TxView;
use crate::ctx::{Via, WriteCtx};
use crate::tracked::{Sealer, Track, Tracking, Treat};
use crate::{ApplyError, Fields, J, Op, Result};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, OnceLock};

/// The prefix of the `Conflict` reason of a stale `row_version`: `stale_version:<current>`.
pub const STALE_PREFIX: &str = "stale_version:";

/// The longest value (as JSON text) a history entry holds; a longer one is a `large` marker.
pub const HISTORY_VALUE_MAX: usize = 32 * 1024;

/// The reserved keys of a tracked record, in the order of `obop_registry::SYSTEM_FIELDS` (the
/// storage layer cannot depend on the registry; a test in `obop-commands` compares the two).
pub const SYSTEM_KEYS: [&str; 6] = [
    "created_at",
    "updated_at",
    "created_by",
    "updated_by",
    "row_version",
    "sys_backfilled",
];

/// Whether `k` is one of the [`SYSTEM_KEYS`].
pub fn is_system_key(k: &str) -> bool {
    SYSTEM_KEYS.contains(&k)
}

/// Remove the system keys from `f`.
pub fn strip_system(f: &mut Fields) {
    f.retain(|k, _| !is_system_key(k));
}

/// `f` without system keys and without nulls (a null counts as an absent key), in key order: what
/// "the record did not change" is compared on.
pub fn canon(f: &Fields) -> BTreeMap<&str, &J> {
    f.iter()
        .filter(|(k, v)| !is_system_key(k) && !v.is_null())
        .map(|(k, v)| (k.as_str(), v))
        .collect()
}

/// The `row_version` of a stored record: absent, null, or not a whole number from 1 mean "this
/// record has never been stamped" (`None`).
pub fn row_version(f: &Fields) -> Option<i64> {
    f.get("row_version").and_then(J::as_i64).filter(|v| *v >= 1)
}

// ---- the timestamp format -------------------------------------------------------------------

const MIN_US: i64 = -62_135_596_800 * 1_000_000;
const MAX_US: i64 = 253_402_300_799 * 1_000_000 + 999_999;

/// Days since 1970-01-01 of a civil date (proleptic Gregorian).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        _ => 28,
    }
}

/// The canonical text of an instant given in microseconds since 1970-01-01T00:00:00Z:
/// `2026-10-25T15:40:00.123456Z`. Years 0001 to 9999; an instant outside is the nearest end.
pub fn fmt_micros(us: i64) -> String {
    let us = us.clamp(MIN_US, MAX_US);
    let (secs, frac) = (us.div_euclid(1_000_000), us.rem_euclid(1_000_000));
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{frac:06}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// The microseconds of a text of exactly the shape [`fmt_micros`] makes, or `None` for anything
/// else (other offsets, other numbers of fractional digits, a day that is not on the calendar).
pub fn parse_micros(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 27
        || [
            (4, b'-'),
            (7, b'-'),
            (10, b'T'),
            (13, b':'),
            (16, b':'),
            (19, b'.'),
            (26, b'Z'),
        ]
        .iter()
        .any(|(i, c)| b[*i] != *c)
    {
        return None;
    }
    let num = |from: usize, to: usize| -> Option<i64> {
        let digits = &b[from..to];
        digits.iter().all(u8::is_ascii_digit).then(|| {
            digits
                .iter()
                .fold(0i64, |n, d| n * 10 + i64::from(d - b'0'))
        })
    };
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, mi, sec, frac) = (num(11, 13)?, num(14, 16)?, num(17, 19)?, num(20, 26)?);
    if y < 1 || !(1..=12).contains(&mo) || d < 1 || d > days_in_month(y, mo) {
        return None;
    }
    if h > 23 || mi > 59 || sec > 59 {
        return None;
    }
    let secs = days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + sec;
    Some(secs * 1_000_000 + frac)
}

/// The milliseconds a UUIDv7 id carries (its first 48 bits), or `None` for any other text. The
/// version nibble must be 7 and the variant `8`, `9`, `a` or `b`, in lower case, like the SQL
/// pattern that backfills a PostgreSQL database. Clamped to the last millisecond of year 9999.
pub fn uuid7_millis(id: &str) -> Option<i64> {
    let b = id.as_bytes();
    if b.len() != 36 || [8, 13, 18, 23].iter().any(|i| b[*i] != b'-') {
        return None;
    }
    let hex = |i: usize| (b[i] as char).is_ascii_hexdigit() && !b[i].is_ascii_uppercase();
    if b[14] != b'7' || !matches!(b[19], b'8' | b'9' | b'a' | b'b') {
        return None;
    }
    if (0..36).any(|i| ![8, 13, 18, 23].contains(&i) && !hex(i)) {
        return None;
    }
    let digits: String = id[..8].chars().chain(id[9..13].chars()).collect();
    let ms = i64::from_str_radix(&digits, 16).ok()?;
    Some(ms.min(MAX_US / 1000))
}

// ---- the clock ------------------------------------------------------------------------------

/// A clock in microseconds that never answers the same or a smaller number twice, however the host
/// clock behaves: memory and SQLite stamp with it, so the stamps of one process follow the order
/// of the transactions. (Said plainly: it is the host clock, made monotonic. PostgreSQL stamps
/// with the server's clock.)
pub(crate) struct Clock {
    last: AtomicI64,
    source: Box<dyn Fn() -> i64 + Send + Sync>,
}

fn host_micros() -> i64 {
    // A host clock set before 1970 reads as the epoch; `tick` keeps the stamps increasing anyway.
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_micros() as i64)
}

impl Clock {
    pub(crate) fn system() -> Self {
        Self::from_source(host_micros)
    }

    /// A clock that reads `source` (tests step it forwards and backwards).
    pub(crate) fn from_source(source: impl Fn() -> i64 + Send + Sync + 'static) -> Self {
        Self {
            last: AtomicI64::new(i64::MIN),
            source: Box::new(source),
        }
    }

    pub(crate) fn tick(&self) -> i64 {
        let now = (self.source)();
        let mut last = self.last.load(Ordering::Relaxed);
        loop {
            let next = now.max(last.saturating_add(1));
            match self
                .last
                .compare_exchange_weak(last, next, Ordering::SeqCst, Ordering::Relaxed)
            {
                Ok(_) => return next,
                Err(seen) => last = seen,
            }
        }
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::system()
    }
}

/// The clock of a backend that has none of its own (the default of `TxView::tx_now`).
pub(crate) fn process_micros() -> i64 {
    static CLOCK: OnceLock<Clock> = OnceLock::new();
    CLOCK.get_or_init(Clock::system).tick()
}

// ---- history entries ------------------------------------------------------------------------

/// One entry of the change history of a record (the table `obop_record_change`).
#[derive(Debug, Clone, PartialEq)]
pub struct ChangeRow {
    /// The store's number of the entry, global and increasing; 0 until stored.
    pub seq: i64,
    /// The record's `updated_at` of this change (the clock, for a delete), canonical text.
    pub at: String,
    pub model: String,
    pub record_id: String,
    /// `create`, `update`, `delete`, `transition`, `erase` or `import`.
    pub op: String,
    /// The login, or a reserved actor id (`system:internal`, ...).
    pub actor: String,
    /// `{surface, path, command, request_id, token_ref, agent}` and `approver` when there is one.
    pub via: Fields,
    /// The record's `row_version` after the change (a delete: the version it would have had).
    pub row_version: i64,
    /// `{field: {"b": before, "a": after, "t": marker}}`; for `erase`, `{fields, entries}`.
    pub changed: Fields,
    /// Some value in `changed` is sealed.
    pub sealed: bool,
    pub batch_id: Option<String>,
    pub run_id: Option<String>,
    /// The transaction number on a store that has one, else 0.
    pub txid: i64,
}

impl ChangeRow {
    /// The entry as the record of a backup stream: model `record_change`, id the `seq`. The `seq`
    /// and `txid` are not in it.
    pub fn to_fields(&self) -> Fields {
        let text = |v: &Option<String>| v.as_ref().map_or(J::Null, |s| J::from(s.as_str()));
        let mut f = Fields::new();
        f.insert("at".into(), J::from(self.at.as_str()));
        f.insert("model".into(), J::from(self.model.as_str()));
        f.insert("record_id".into(), J::from(self.record_id.as_str()));
        f.insert("op".into(), J::from(self.op.as_str()));
        f.insert("actor".into(), J::from(self.actor.as_str()));
        f.insert("via".into(), J::Object(self.via.clone()));
        f.insert("row_version".into(), J::from(self.row_version));
        f.insert("changed".into(), J::Object(self.changed.clone()));
        f.insert("sealed".into(), J::from(self.sealed));
        f.insert("batch_id".into(), text(&self.batch_id));
        f.insert("run_id".into(), text(&self.run_id));
        f
    }

    /// The inverse of [`ChangeRow::to_fields`]; `seq` is the id of the backup row. Anything that
    /// is not an entry is refused, with the reason.
    pub fn from_fields(seq: i64, f: &Fields) -> Result<ChangeRow> {
        let bad = |what: &str| format!("record_change {seq}: {what}");
        let text = |k: &str| -> Result<String> {
            f.get(k)
                .and_then(J::as_str)
                .map(String::from)
                .ok_or_else(|| bad(&format!("`{k}` is not text")))
        };
        let opt = |k: &str| -> Result<Option<String>> {
            match f.get(k) {
                None | Some(J::Null) => Ok(None),
                Some(J::String(s)) => Ok(Some(s.clone())),
                Some(_) => Err(bad(&format!("`{k}` is not text"))),
            }
        };
        let object = |k: &str| -> Result<Fields> {
            f.get(k)
                .and_then(J::as_object)
                .cloned()
                .ok_or_else(|| bad(&format!("`{k}` is not an object")))
        };
        let at = text("at")?;
        if parse_micros(&at).is_none() {
            return Err(bad("`at` is not a canonical instant"));
        }
        Ok(ChangeRow {
            seq,
            at,
            model: text("model")?,
            record_id: text("record_id")?,
            op: text("op")?,
            actor: text("actor")?,
            via: object("via")?,
            row_version: f
                .get("row_version")
                .and_then(J::as_i64)
                .ok_or_else(|| bad("`row_version` is not a whole number"))?,
            changed: object("changed")?,
            sealed: f
                .get("sealed")
                .and_then(J::as_bool)
                .ok_or_else(|| bad("`sealed` is not true or false"))?,
            batch_id: opt("batch_id")?,
            run_id: opt("run_id")?,
            txid: 0,
        })
    }
}

/// How the entries of the history of a record are rewritten by an erasure: every value of a field
/// that is not in `keep` (and not already erased or redacted) becomes `{"t":"erased"}`. `None` when
/// nothing needs rewriting; else the new `changed` and the names of the fields that were erased.
pub(crate) fn scrub_changed(changed: &Fields, keep: &[String]) -> Option<(Fields, Vec<String>)> {
    let mut out = Fields::new();
    let mut erased = Vec::new();
    for (k, v) in changed {
        let marker = v.get("t").and_then(J::as_str).unwrap_or("");
        if keep.contains(k) || marker == "erased" || marker == "redacted" {
            out.insert(k.clone(), v.clone());
        } else {
            out.insert(k.clone(), json!({"t": "erased"}));
            erased.push(k.clone());
        }
    }
    (!erased.is_empty()).then_some((out, erased))
}

/// The `changed` of the entry an erasure appends: which fields were erased and in how many entries.
pub(crate) fn erase_changed(fields: BTreeSet<String>, entries: usize) -> Fields {
    let mut c = Fields::new();
    c.insert(
        "fields".into(),
        J::Array(fields.into_iter().map(J::from).collect()),
    );
    c.insert("entries".into(), J::from(entries as i64));
    c
}

/// The route of a change as stored in an entry.
pub fn via_json(v: &Via, path: Option<&str>) -> Fields {
    let text = |s: &Option<String>| s.as_ref().map_or(J::Null, |s| J::from(s.as_str()));
    let mut m = Fields::new();
    m.insert("surface".into(), J::from(v.surface.as_str()));
    m.insert("path".into(), J::from(path.unwrap_or(&v.path)));
    m.insert("command".into(), J::from(v.command.as_str()));
    m.insert("request_id".into(), text(&v.request_id));
    m.insert("token_ref".into(), text(&v.token_ref));
    m.insert("agent".into(), J::from(v.agent));
    if let Some(a) = &v.approver {
        m.insert("approver".into(), J::from(a.as_str()));
    }
    m
}

/// A record of a tracked model that has no `row_version`, stamped as the backfill does: the
/// creation time from the UUIDv7 id (else `fallback_at`), version 1, `sys_backfilled`, no actor.
/// `None` when the record already has a version.
pub(crate) fn backfilled(id: &str, f: &Fields, fallback_at: &str) -> Option<Fields> {
    if row_version(f).is_some() {
        return None;
    }
    let at = uuid7_millis(id).map_or_else(|| fallback_at.to_string(), |ms| fmt_micros(ms * 1000));
    let mut out = f.clone();
    out.remove("created_by");
    out.remove("updated_by");
    out.insert("created_at".into(), J::from(at.as_str()));
    out.insert("updated_at".into(), J::from(at));
    out.insert("row_version".into(), J::from(1));
    out.insert("sys_backfilled".into(), J::from(true));
    Some(out)
}

// ---- the planner ----------------------------------------------------------------------------

/// One record of a tracked model, from its first appearance in the transaction.
struct Rec {
    model: String,
    id: String,
    track: Arc<Track>,
    /// The record as the database held it when the transaction first looked (system keys
    /// included); `None`: it was not there.
    before: Option<Fields>,
    /// The record now; `None`: not there (never was, or deleted).
    live: Option<Fields>,
    /// A write has stamped `live` in this transaction.
    stamped: bool,
    /// `live` is in the database (written or inserted by this transaction, or found there).
    in_db: bool,
    /// Set when the record was deleted.
    gone: Option<Gone>,
}

struct Gone {
    /// The record at the moment it was deleted.
    last: Fields,
    /// The instant of the delete, at least a microsecond after the last stamp.
    at: i64,
}

/// The planner for one batch: it sees every op on a tracked model, writes through the transaction,
/// and at the end writes back what ended unchanged, appends the history and carries out the
/// erasures.
pub(crate) struct Stage<'a> {
    ctx: &'a WriteCtx,
    tracking: &'a Tracking,
    sealer: Option<Arc<dyn Sealer>>,
    /// The tracking of every model asked about, fixed at the first question so that a model
    /// tracked (or not) while the batch runs is treated alike by all its steps.
    seen: HashMap<String, Option<Arc<Track>>>,
    recs: Vec<Rec>,
    index: HashMap<(String, String), usize>,
    erase: Vec<(String, String)>,
}

fn stripped(f: &Fields) -> Fields {
    let mut c = f.clone();
    strip_system(&mut c);
    c
}

fn stamp_new(out: &mut Fields, ts: i64, by: &str) {
    let at = J::from(fmt_micros(ts));
    out.insert("created_at".into(), at.clone());
    out.insert("updated_at".into(), at);
    out.insert("created_by".into(), J::from(by));
    out.insert("updated_by".into(), J::from(by));
    out.insert("row_version".into(), J::from(1));
}

/// Stamp a record that exists: `updated_at` is the clock, but at least a microsecond after the
/// last stamp, so the instants of a record only grow.
fn stamp_existing(out: &mut Fields, live: &Fields, id: &str, ts: i64, by: &str) {
    let prev = live
        .get("updated_at")
        .and_then(J::as_str)
        .and_then(parse_micros);
    let at = prev.map_or(ts, |p| ts.max(p.saturating_add(1)));
    let version = row_version(live);
    let known = version
        .and(live.get("created_at"))
        .and_then(J::as_str)
        .filter(|c| parse_micros(c).is_some());
    let estimated = known.is_none() || live.get("sys_backfilled") == Some(&J::Bool(true));
    let created_at = known.map_or_else(
        || uuid7_millis(id).map_or_else(|| fmt_micros(ts), |ms| fmt_micros(ms * 1000)),
        String::from,
    );
    out.insert("created_at".into(), J::from(created_at));
    out.insert("updated_at".into(), J::from(fmt_micros(at)));
    if version.is_some()
        && let Some(by) = live.get("created_by")
    {
        out.insert("created_by".into(), by.clone());
    }
    out.insert("updated_by".into(), J::from(by));
    out.insert("row_version".into(), J::from(version.unwrap_or(1) + 1));
    if estimated {
        out.insert("sys_backfilled".into(), J::from(true));
    }
}

impl<'a> Stage<'a> {
    pub(crate) fn new(ctx: &'a WriteCtx, tracking: &'a Tracking) -> Self {
        Self {
            ctx,
            tracking,
            sealer: tracking.sealer(),
            seen: HashMap::new(),
            recs: Vec::new(),
            index: HashMap::new(),
            erase: Vec::new(),
        }
    }

    /// The tracking of `model`, as it was when the batch first asked.
    pub(crate) fn track_of(&mut self, model: &str) -> Option<Arc<Track>> {
        if let Some(t) = self.seen.get(model) {
            return t.clone();
        }
        let t = self.tracking.get(model);
        self.seen.insert(model.to_string(), t.clone());
        t
    }

    pub(crate) fn queue_erase(&mut self, model: &str, id: &str) {
        self.erase.push((model.to_string(), id.to_string()));
    }

    /// The record `(model, id)` of this batch, read from the transaction the first time (that read
    /// takes the row lock and learns the clock). `fresh` starts a new life for a record this batch
    /// has deleted, for an op that creates it again.
    fn rec(
        &mut self,
        tx: &mut dyn TxView,
        model: &str,
        id: &str,
        track: &Arc<Track>,
        fresh: bool,
    ) -> std::result::Result<usize, ApplyError> {
        let key = (model.to_string(), id.to_string());
        let new = |before: Option<Fields>| Rec {
            model: model.to_string(),
            id: id.to_string(),
            track: track.clone(),
            in_db: before.is_some(),
            live: before.clone(),
            before,
            stamped: false,
            gone: None,
        };
        if let Some(&i) = self.index.get(&key) {
            if fresh && self.recs[i].gone.is_some() {
                self.recs.push(new(None));
                let n = self.recs.len() - 1;
                self.index.insert(key, n);
                return Ok(n);
            }
            return Ok(i);
        }
        let before = tx.tx_get_clock(model, id).map_err(ApplyError::Storage)?;
        self.recs.push(new(before));
        let n = self.recs.len() - 1;
        self.index.insert(key, n);
        Ok(n)
    }

    /// Carry out one op on a record of a tracked model.
    pub(crate) fn op(
        &mut self,
        tx: &mut dyn TxView,
        index: usize,
        op: &Op,
        track: &Arc<Track>,
    ) -> std::result::Result<(), ApplyError> {
        let conflict = |reason: &str| ApplyError::Conflict {
            index,
            reason: reason.into(),
        };
        match op {
            Op::Insert { model, id, fields } => {
                let i = self.rec(tx, model, id, track, true)?;
                if self.recs[i].live.is_some() {
                    return Err(conflict("already exists"));
                }
                self.write(tx, i, stripped(fields))
            }
            Op::Put { model, id, fields } => {
                let i = self.rec(tx, model, id, track, true)?;
                self.write(tx, i, stripped(fields))
            }
            Op::Merge { model, id, patch } => {
                let i = self.rec(tx, model, id, track, false)?;
                let Some(live) = &self.recs[i].live else {
                    return Err(conflict("no such record"));
                };
                let mut data = stripped(live);
                data.extend(stripped(patch));
                self.write(tx, i, data)
            }
            Op::Patch {
                model,
                id,
                patch,
                expect,
            } => {
                let i = self.rec(tx, model, id, track, false)?;
                let Some(live) = &self.recs[i].live else {
                    return Err(conflict("no such record"));
                };
                // An absent version reads as 1, so a record from before the tracking can be
                // guarded by the version a client was shown for it.
                let current = row_version(live).unwrap_or(1);
                let mut changed = false;
                let mut stale = false;
                for (k, v) in expect {
                    let got = if k == "row_version" {
                        J::from(current)
                    } else {
                        live.get(k).cloned().unwrap_or(J::Null)
                    };
                    if got != *v {
                        if k == "row_version" {
                            stale = true;
                        } else {
                            changed = true;
                        }
                    }
                }
                if stale {
                    return Err(conflict(&format!("{STALE_PREFIX}{current}")));
                }
                if changed {
                    return Err(conflict("the record changed"));
                }
                let mut data = stripped(live);
                data.extend(stripped(patch));
                self.write(tx, i, data)
            }
            Op::Add {
                model,
                id,
                field,
                delta,
                min,
                max,
                create,
            } => {
                let i = self.rec(tx, model, id, track, create.is_some())?;
                if is_system_key(field) {
                    return Err(conflict("a system field cannot be counted"));
                }
                let mut data = match (&self.recs[i].live, create) {
                    (Some(l), _) => stripped(l),
                    (None, Some(c)) => stripped(c),
                    (None, None) => return Err(conflict("no such record")),
                };
                let cur = match data.get(field) {
                    None | Some(J::Null) => 0,
                    Some(v) => v
                        .as_i64()
                        .ok_or_else(|| conflict("the field is not an integer"))?,
                };
                let new = cur
                    .checked_add(*delta)
                    .ok_or_else(|| conflict("overflow"))?;
                if min.is_some_and(|m| new < m) {
                    return Err(conflict("below the minimum"));
                }
                if max.is_some_and(|m| new > m) {
                    return Err(conflict("above the maximum"));
                }
                data.insert(field.clone(), J::from(new));
                self.write(tx, i, data)
            }
            Op::Delete { model, id } => {
                let i = self.rec(tx, model, id, track, false)?;
                let Some(last) = self.recs[i].live.take() else {
                    return Err(conflict("no such record"));
                };
                tx.tx_delete(model, id).map_err(ApplyError::Storage)?;
                // The instant of the delete follows the record's last stamp.
                let ts = tx.tx_now().map_err(ApplyError::Storage)?;
                let prev = last
                    .get("updated_at")
                    .and_then(J::as_str)
                    .and_then(parse_micros);
                let at = prev.map_or(ts, |p| ts.max(p.saturating_add(1)));
                let rec = &mut self.recs[i];
                rec.in_db = false;
                rec.gone = Some(Gone { last, at });
                Ok(())
            }
            Op::EraseHistory { model, id } => {
                self.queue_erase(model, id);
                Ok(())
            }
        }
    }

    /// Make record `i` hold `data` (no system keys in it) if that is a change, stamping it the
    /// first time in this transaction.
    fn write(
        &mut self,
        tx: &mut dyn TxView,
        i: usize,
        data: Fields,
    ) -> std::result::Result<(), ApplyError> {
        let by = self.ctx.by.as_str();
        let rec = &mut self.recs[i];
        if rec.live.as_ref().is_some_and(|l| canon(l) == canon(&data)) {
            return Ok(());
        }
        let mut out = data;
        match (&rec.live, rec.stamped) {
            (Some(live), true) => {
                for k in SYSTEM_KEYS {
                    if let Some(v) = live.get(k) {
                        out.insert(k.into(), v.clone());
                    }
                }
            }
            (Some(live), false) => {
                let ts = tx.tx_now().map_err(ApplyError::Storage)?;
                stamp_existing(&mut out, live, &rec.id, ts, by);
            }
            (None, _) => {
                let ts = tx.tx_now().map_err(ApplyError::Storage)?;
                stamp_new(&mut out, ts, by);
            }
        }
        let written = if rec.in_db {
            tx.tx_put(&rec.model, &rec.id, &out)
        } else {
            tx.tx_insert(&rec.model, &rec.id, &out)
        };
        written.map_err(ApplyError::Storage)?;
        rec.stamped = true;
        rec.in_db = true;
        rec.live = Some(out);
        Ok(())
    }

    /// After the last op: put back what ended as it began, append the entries of the history,
    /// carry out the erasures.
    pub(crate) fn finish(self, tx: &mut dyn TxView) -> std::result::Result<(), ApplyError> {
        let st = ApplyError::Storage;
        let mut rows: Vec<ChangeRow> = Vec::new();
        for rec in &self.recs {
            match (&rec.before, &rec.live, &rec.gone) {
                // Created and, if at all, deleted again in this transaction.
                (None, Some(live), _) => {
                    if rec.track.history {
                        rows.push(self.entry(rec, "create", live, None, Some(live)));
                    }
                }
                (None, None, _) | (Some(_), None, None) => {}
                (Some(before), Some(live), _) => {
                    if !rec.stamped {
                        continue;
                    }
                    if canon(before) == canon(live) {
                        // A to B to A: the record goes back to what it was, stamps and all.
                        tx.tx_put(&rec.model, &rec.id, before).map_err(st)?;
                    } else if rec.track.history {
                        let primary = self
                            .ctx
                            .primary
                            .as_ref()
                            .is_some_and(|(m, i)| *m == rec.model && *i == rec.id);
                        let op = if primary { "transition" } else { "update" };
                        rows.push(self.entry(rec, op, live, Some(before), Some(live)));
                    }
                }
                (Some(before), None, Some(gone)) => {
                    let last = &gone.last;
                    let net = rec.stamped && canon(before) != canon(last);
                    if net && rec.track.history {
                        rows.push(self.entry(rec, "update", last, Some(before), Some(last)));
                    }
                    if rec.track.history {
                        let version = row_version(if net { last } else { before }).unwrap_or(1) + 1;
                        let mut row = self.entry(rec, "delete", last, Some(last), None);
                        row.at = fmt_micros(gone.at);
                        row.row_version = version;
                        rows.push(row);
                    }
                }
            }
        }
        if !rows.is_empty() {
            tx.tx_history(&rows).map_err(st)?;
        }
        for (model, id) in &self.erase {
            self.scrub(tx, model, id)?;
        }
        Ok(())
    }

    /// Erase the values from the history of a record and append the entry that says so.
    fn scrub(
        &self,
        tx: &mut dyn TxView,
        model: &str,
        id: &str,
    ) -> std::result::Result<(), ApplyError> {
        let st = ApplyError::Storage;
        let keep: Vec<String> = self
            .tracking
            .get(model)
            .map(|t| t.keep_on_erase.iter().cloned().collect())
            .unwrap_or_default();
        let version = match self.index.get(&(model.to_string(), id.to_string())) {
            Some(&i) => self.recs[i]
                .live
                .as_ref()
                .map_or(0, |l| row_version(l).unwrap_or(1)),
            None => tx
                .tx_get(model, id)
                .map_err(st)?
                .map_or(0, |l| row_version(&l).unwrap_or(1)),
        };
        let at = tx.tx_now().map_err(st)?;
        let via = via_json(&self.ctx.via, None);
        tx.tx_scrub(model, id, &keep, &self.ctx.by, &via, version, at)
            .map_err(st)?;
        Ok(())
    }

    /// One entry for `rec`. `before` and `after` are the record around the change (system keys
    /// are ignored); the entry says `after`'s stamp.
    fn entry(
        &self,
        rec: &Rec,
        op: &str,
        stamped: &Fields,
        before: Option<&Fields>,
        after: Option<&Fields>,
    ) -> ChangeRow {
        let effect = self
            .ctx
            .primary
            .as_ref()
            .is_some_and(|(m, i)| *m != rec.model || *i != rec.id);
        let (changed, sealed) = self.changed(rec, before, after);
        ChangeRow {
            seq: 0,
            at: stamped
                .get("updated_at")
                .and_then(J::as_str)
                .map_or_else(String::new, String::from),
            model: rec.model.clone(),
            record_id: rec.id.clone(),
            op: op.to_string(),
            actor: self.ctx.by.clone(),
            via: via_json(&self.ctx.via, effect.then_some("effect")),
            row_version: row_version(stamped).unwrap_or(1),
            changed,
            sealed,
            batch_id: self.ctx.batch_id.clone(),
            run_id: self.ctx.run_id.clone(),
            txid: 0,
        }
    }

    /// `{field: {b, a, t}}` for the fields that differ between `before` and `after`, and whether
    /// any value in it is sealed.
    fn changed(
        &self,
        rec: &Rec,
        before: Option<&Fields>,
        after: Option<&Fields>,
    ) -> (Fields, bool) {
        let side = |f: Option<&'_ Fields>| -> BTreeMap<String, J> {
            f.map(|f| {
                canon(f)
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v.clone()))
                    .collect()
            })
            .unwrap_or_default()
        };
        let (b, a) = (side(before), side(after));
        let keys: BTreeSet<&String> = b.keys().chain(a.keys()).collect();
        let mut changed = Fields::new();
        let mut sealed = false;
        for k in keys {
            let (bv, av) = (b.get(k), a.get(k));
            if bv == av {
                continue;
            }
            let (entry, was_sealed) = self.change_of(rec, k, bv, av);
            sealed |= was_sealed;
            changed.insert(k.clone(), entry);
        }
        (changed, sealed)
    }

    /// The entry for one field. Whatever cannot be recorded plainly is recorded as the fact that
    /// the field changed, with a marker.
    fn change_of(&self, rec: &Rec, field: &str, b: Option<&J>, a: Option<&J>) -> (J, bool) {
        let redacted = (json!({"t": "redacted"}), false);
        let treat = rec
            .track
            .fields
            .get(field)
            .copied()
            .unwrap_or(Treat::Redact);
        if treat == Treat::Redact {
            return redacted;
        }
        let texts: Vec<Option<String>> = [b, a].iter().map(|v| v.map(|v| v.to_string())).collect();
        let longest = texts.iter().flatten().map(String::len).max().unwrap_or(0);
        if longest > HISTORY_VALUE_MAX {
            return (json!({"t": "large", "len": longest}), false);
        }
        let mut e = Fields::new();
        if treat == Treat::Seal {
            let purpose = format!("history.v1:{}.{field}", rec.model);
            let Some(sealer) = &self.sealer else {
                return redacted;
            };
            for (key, text) in [("b", &texts[0]), ("a", &texts[1])] {
                if let Some(plain) = text {
                    let Some(envelope) = sealer.seal(&purpose, plain) else {
                        return redacted;
                    };
                    e.insert(key.into(), J::from(envelope));
                }
            }
            e.insert("t".into(), J::from("sealed"));
            return (J::Object(e), true);
        }
        if let Some(v) = b {
            e.insert("b".into(), v.clone());
        }
        if let Some(v) = a {
            e.insert("a".into(), v.clone());
        }
        (J::Object(e), false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_timestamp_has_the_shape_of_a_canonical_datetime() {
        assert_eq!(fmt_micros(0), "1970-01-01T00:00:00.000000Z");
        assert_eq!(
            fmt_micros(1_700_000_000_123_456),
            "2023-11-14T22:13:20.123456Z"
        );
        assert_eq!(fmt_micros(-1), "1969-12-31T23:59:59.999999Z");
        assert_eq!(
            fmt_micros(951_782_400_000_000),
            "2000-02-29T00:00:00.000000Z"
        );
        // The ends of the range, and what lies beyond them.
        assert_eq!(fmt_micros(MIN_US), "0001-01-01T00:00:00.000000Z");
        assert_eq!(fmt_micros(MAX_US), "9999-12-31T23:59:59.999999Z");
        assert_eq!(fmt_micros(i64::MIN), "0001-01-01T00:00:00.000000Z");
        assert_eq!(fmt_micros(i64::MAX), "9999-12-31T23:59:59.999999Z");
    }

    #[test]
    fn parse_is_the_inverse_of_format_and_takes_nothing_else() {
        let mut n: i64 = 0x2545_F491_4F6C_DD1D;
        for _ in 0..20_000 {
            // A cheap deterministic spread over the whole range, including the negative half.
            n = n
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let us = MIN_US + (n as u64 % (MAX_US - MIN_US + 1) as u64) as i64;
            let text = fmt_micros(us);
            assert_eq!(text.len(), 27, "{text}");
            assert_eq!(parse_micros(&text), Some(us), "{text}");
        }
        for us in [MIN_US, MAX_US, 0, -1, 1, 1_000_000, 86_399_999_999] {
            assert_eq!(parse_micros(&fmt_micros(us)), Some(us));
        }
        for bad in [
            "",
            "2026-10-25T15:40:00Z",
            "2026-10-25T15:40:00.123Z",
            "2026-10-25T15:40:00.123456+00:00",
            "2026-10-25 15:40:00.123456Z",
            "2026-10-25T15:40:00.123456z",
            "2026-02-30T00:00:00.000000Z",
            "2025-02-29T00:00:00.000000Z",
            "2026-13-01T00:00:00.000000Z",
            "2026-00-01T00:00:00.000000Z",
            "2026-10-00T00:00:00.000000Z",
            "2026-10-25T24:00:00.000000Z",
            "2026-10-25T15:60:00.000000Z",
            "2026-10-25T15:40:60.000000Z",
            "0000-01-01T00:00:00.000000Z",
            "+026-10-25T15:40:00.123456Z",
            "2026-10-25T15:40:00.12345é",
        ] {
            assert_eq!(parse_micros(bad), None, "{bad:?}");
        }
        assert_eq!(
            parse_micros("2024-02-29T00:00:00.000000Z"),
            Some(1_709_164_800_000_000)
        );
    }

    #[test]
    fn a_uuid7_id_carries_its_creation_time() {
        // 2026-10-25T15:40:00.123Z in the first 48 bits.
        let ms = 1_793_000_400_123_i64;
        let id = format!(
            "{:08x}-{:04x}-7abc-8def-0123456789ab",
            ms >> 16,
            ms & 0xffff
        );
        assert_eq!(uuid7_millis(&id), Some(ms));
        for variant in ['8', '9', 'a', 'b'] {
            let mut v = id.clone();
            v.replace_range(19..20, &variant.to_string());
            assert_eq!(uuid7_millis(&v), Some(ms), "{v}");
        }
        for bad in [
            "",
            "not-a-uuid",
            // version 4, variant c, upper case, missing dashes, wrong length
            "0194c9a8-b3d2-4abc-8def-0123456789ab",
            "0194c9a8-b3d2-7abc-cdef-0123456789ab",
            "0194C9A8-B3D2-7ABC-8DEF-0123456789AB",
            "0194c9a8b3d27abc8def0123456789ab",
            "0194c9a8-b3d2-7abc-8def-0123456789a",
            "0194c9a8-b3d2-7abc-8def-0123456789ag",
        ] {
            assert_eq!(uuid7_millis(bad), None, "{bad:?}");
        }
        // All ones is clamped to the last millisecond of year 9999.
        assert_eq!(
            uuid7_millis("ffffffff-ffff-7abc-8def-0123456789ab"),
            Some(MAX_US / 1000)
        );
    }

    #[test]
    fn the_clock_never_goes_back_or_stands_still() {
        use std::sync::atomic::AtomicI64;
        let now = Arc::new(AtomicI64::new(1_000));
        let source = now.clone();
        let c = Clock::from_source(move || source.load(Ordering::SeqCst));
        assert_eq!(c.tick(), 1_000);
        assert_eq!(
            c.tick(),
            1_001,
            "the same host reading gives a later instant"
        );
        now.store(500, Ordering::SeqCst);
        assert_eq!(c.tick(), 1_002, "the host clock stepped back");
        now.store(5_000, Ordering::SeqCst);
        assert_eq!(c.tick(), 5_000);
        let a = process_micros();
        assert!(process_micros() > a);
    }

    #[test]
    fn canon_ignores_system_keys_and_nulls() {
        let f = |v: J| v.as_object().cloned().unwrap();
        let a = f(json!({"x": 1, "y": null, "row_version": 3, "updated_by": "ann"}));
        let b = f(json!({"x": 1, "created_at": "t"}));
        assert_eq!(canon(&a), canon(&b));
        assert_ne!(canon(&a), canon(&f(json!({"x": 2}))));
        assert_ne!(
            canon(&a),
            canon(&f(json!({"x": 1, "z": 0}))),
            "zero is not null"
        );
        assert_ne!(canon(&a), canon(&f(json!({"x": 1, "z": false}))));
        let mut s = f(json!({"x": 1, "row_version": 2, "sys_backfilled": true, "created_by": "a"}));
        strip_system(&mut s);
        assert_eq!(s, f(json!({"x": 1})));
        assert_eq!(SYSTEM_KEYS.len(), 6);
        assert!(
            is_system_key("updated_at") && !is_system_key("id") && !is_system_key("if_version")
        );
        assert_eq!(row_version(&f(json!({"row_version": 4}))), Some(4));
        for none in [
            json!({}),
            json!({"row_version": null}),
            json!({"row_version": 0}),
            json!({"row_version": "3"}),
        ] {
            assert_eq!(row_version(&f(none)), None);
        }
    }

    #[test]
    fn an_entry_survives_the_trip_through_a_backup_row() {
        let row = ChangeRow {
            seq: 42,
            at: "2026-10-25T15:40:00.123456Z".into(),
            model: "deal".into(),
            record_id: "d1".into(),
            op: "update".into(),
            actor: "ann".into(),
            via: via_json(&WriteCtx::new("ann", "rest", "deal.update").via, None),
            row_version: 3,
            changed: json!({"stage": {"b": "new", "a": "won"}, "pin": {"t": "redacted"}})
                .as_object()
                .cloned()
                .unwrap(),
            sealed: false,
            batch_id: Some("b1".into()),
            run_id: None,
            txid: 0,
        };
        let f = row.to_fields();
        assert_eq!(ChangeRow::from_fields(42, &f).unwrap(), row);
        for (key, value) in [
            ("at", json!("yesterday")),
            ("model", json!(5)),
            ("via", json!([])),
            ("row_version", json!("3")),
            ("sealed", json!(null)),
            ("batch_id", json!(7)),
        ] {
            let mut bad = f.clone();
            bad.insert(key.into(), value);
            assert!(ChangeRow::from_fields(42, &bad).is_err(), "{key}");
        }
        let mut missing = f.clone();
        missing.remove("changed");
        assert!(
            ChangeRow::from_fields(42, &missing)
                .unwrap_err()
                .contains("record_change 42")
        );
    }

    #[test]
    fn the_route_of_an_entry_names_surface_path_command_and_the_approver() {
        let mut ctx = WriteCtx::new("ann", "mcp", "deal.update");
        ctx.via.agent = true;
        ctx.via.token_ref = Some("0123456789ab".into());
        let v = via_json(&ctx.via, None);
        assert_eq!(v["surface"], "mcp");
        assert_eq!(v["path"], "direct");
        assert_eq!(v["command"], "deal.update");
        assert_eq!(v["agent"], true);
        assert_eq!(v["token_ref"], "0123456789ab");
        assert_eq!(v["request_id"], J::Null);
        assert!(!v.contains_key("approver"));
        ctx.via.approver = Some("bob".into());
        let v = via_json(&ctx.via, Some("effect"));
        assert_eq!(
            (v["path"].as_str(), v["approver"].as_str()),
            (Some("effect"), Some("bob"))
        );
    }

    #[test]
    fn scrubbing_keeps_what_is_kept_and_what_is_already_gone() {
        let f = |v: J| v.as_object().cloned().unwrap();
        let changed = f(json!({
            "email": {"b": "a@x", "a": "b@x"},
            "stage": {"b": "new", "a": "won"},
            "pin": {"t": "redacted"},
            "old": {"t": "erased"},
            "note": {"t": "sealed", "a": "envelope"},
        }));
        let (out, erased) = scrub_changed(&changed, &["stage".to_string()]).unwrap();
        assert_eq!(erased, ["email", "note"]);
        assert_eq!(out["email"], json!({"t": "erased"}));
        assert_eq!(out["note"], json!({"t": "erased"}));
        assert_eq!(
            out["stage"], changed["stage"],
            "a declared, non-personal field stays"
        );
        assert_eq!(out["pin"], changed["pin"]);
        assert_eq!(out["old"], changed["old"]);
        // A second pass has nothing left to do.
        assert!(scrub_changed(&out, &["stage".to_string()]).is_none());
        let e = erase_changed(["b".to_string(), "a".to_string()].into(), 3);
        assert_eq!(J::Object(e), json!({"fields": ["a", "b"], "entries": 3}));
    }

    #[test]
    fn a_record_without_a_version_is_stamped_from_its_id_or_the_fallback() {
        let f = |v: J| v.as_object().cloned().unwrap();
        let ms = 1_793_000_400_123_i64;
        let id = format!(
            "{:08x}-{:04x}-7abc-8def-0123456789ab",
            ms >> 16,
            ms & 0xffff
        );
        let fallback = "2026-01-01T00:00:00.000000Z";
        let old = f(json!({"x": 1, "created_by": "stale", "updated_by": "stale"}));
        let from_id = backfilled(&id, &old, fallback).unwrap();
        assert_eq!(from_id["created_at"], fmt_micros(ms * 1000));
        assert_eq!(from_id["updated_at"], from_id["created_at"]);
        assert_eq!(
            (
                from_id["row_version"].as_i64(),
                from_id["sys_backfilled"].as_bool()
            ),
            (Some(1), Some(true))
        );
        assert!(!from_id.contains_key("created_by") && !from_id.contains_key("updated_by"));
        assert_eq!(from_id["x"], 1);
        let from_fallback = backfilled("plain-id", &old, fallback).unwrap();
        assert_eq!(from_fallback["created_at"], fallback);
        // A record that has a version is not touched.
        assert!(backfilled(&id, &f(json!({"row_version": 2})), fallback).is_none());
    }
}
