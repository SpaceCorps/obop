//! The questions one can ask the change history of a store (D75), and the answer to the one
//! about stamping old records. The entries themselves are [`crate::sysfields::ChangeRow`].
//!
//! All three stores keep the history (PostgreSQL in `pg_hist`); a backend that does not refuses,
//! it does not answer "no entries" (the defaults of [`crate::Backend`]).

use crate::{Backend, Result};

/// One record's entries, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryQuery {
    pub model: String,
    pub record_id: String,
    /// Only entries older than this one (`seq` below it): the page after the one that ended here.
    pub before_seq: Option<i64>,
    /// Only entries of this operation (`create`, `update`, `delete`, `transition`, `erase`,
    /// `import`).
    pub op: Option<String>,
    /// Only entries at or after this instant (`YYYY-MM-DDTHH:MM:SS.ffffffZ`): the record cannot
    /// have older ones, so a reader that knows when the live record was created passes it.
    pub since_at: Option<String>,
    pub limit: usize,
}

impl HistoryQuery {
    /// Every entry of `record_id` of `model`, newest first, at most `limit`.
    pub fn of(model: &str, record_id: &str, limit: usize) -> Self {
        Self {
            model: model.to_string(),
            record_id: record_id.to_string(),
            before_seq: None,
            op: None,
            since_at: None,
            limit,
        }
    }
}

/// What one pass of `Backend::backfill_sysfields` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Backfilled {
    /// Records stamped by this pass.
    pub stamped: usize,
    /// Records of the models that still have no `row_version` (a pass stamps at most `batch`).
    pub remaining: usize,
}

/// One page of the change feed (see [`crate::Backend::changes`]): the entries of every record, in
/// the order a consumer must read them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChangeQuery {
    /// Only entries of these models; empty means every model.
    pub models: Vec<String>,
    /// Only entries of these operations; empty means every operation.
    pub ops: Vec<String>,
    /// Where the previous page ended; `None` starts at the beginning.
    pub after: Option<ChangeCursor>,
    /// Only entries at or after this instant (`YYYY-MM-DDTHH:MM:SS.ffffffZ`).
    pub since_at: Option<String>,
    /// At most this many entries (at most [`FEED_PAGE_MAX`]; 0 gives an empty page).
    pub limit: usize,
}

/// The most entries one page of the feed holds, whatever the query asks.
pub const FEED_PAGE_MAX: usize = 10_000;

/// A place in the change feed. The text form is `c1.<epoch>.<txid>.<seq>`; an application treats
/// it as opaque. The `epoch` names the life of the history: a restore ends it, and a cursor from
/// before is refused (`cursor_expired`) instead of silently skipping what the restore brought back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeCursor {
    pub epoch: String,
    /// The transaction number of the entry on a store that has one (PostgreSQL), else 0.
    pub txid: i64,
    pub seq: i64,
}

impl ChangeCursor {
    pub fn to_text(&self) -> String {
        format!("c1.{}.{}.{}", self.epoch, self.txid, self.seq)
    }

    /// The cursor a text stands for. The error starts with `cursor_invalid:`.
    pub fn parse(text: &str) -> Result<ChangeCursor> {
        let bad = || format!("cursor_invalid: {text:?} is not a cursor of the change feed");
        let mut it = text.split('.');
        let (Some("c1"), Some(epoch), Some(txid), Some(seq), None) =
            (it.next(), it.next(), it.next(), it.next(), it.next())
        else {
            return Err(bad());
        };
        if epoch.is_empty() || !epoch.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err(bad());
        }
        let num = |s: &str| -> Result<i64> {
            if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
                return Err(bad());
            }
            s.parse::<i64>().map_err(|_| bad())
        };
        Ok(ChangeCursor {
            epoch: epoch.to_string(),
            txid: num(txid)?,
            seq: num(seq)?,
        })
    }
}

impl std::fmt::Display for ChangeCursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_text())
    }
}

/// A page of the change feed.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChangePage {
    pub rows: Vec<crate::sysfields::ChangeRow>,
    /// Where to ask for the next page: after the last row of this one, or, when there is none,
    /// where the question began (`None` only when it began at the start of an empty feed).
    pub next: Option<ChangeCursor>,
    /// On PostgreSQL, how old (in milliseconds) the oldest entry is that the feed holds back
    /// because a transaction with a lower transaction number is still open. Only worked out for a
    /// page that is not full (a full page has more to say first, and what lies beyond it is not
    /// looked at): `None` means nothing is held back or that nobody looked.
    pub horizon_lag_ms: Option<i64>,
    /// On PostgreSQL, how many entries carry a transaction number that the cluster has not given
    /// out yet, which the feed cannot show: the history was loaded from another cluster by a
    /// dump or by replication, whose counter ran further (docs/DEPLOY.md, "Moving a database to
    /// another cluster"). They are not held back by an open transaction and are not counted in
    /// `horizon_lag_ms`. Worked out with `horizon_lag_ms` (so `None` for a full page); `Some(0)`
    /// is the normal answer.
    pub foreign_entries: Option<i64>,
}

/// What the store can say about its history without reading it (`obop storage check`, the
/// metrics of the maintenance step).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HistoryStats {
    /// The number of entries: exact on memory and SQLite, the planner's estimate (exact for a
    /// partition it has never analysed) on PostgreSQL.
    pub entries_estimate: i64,
    /// Entries in the default partition, where an entry lands that no monthly partition takes
    /// (the month was not made yet, or the instant is far outside the years partitions are made
    /// for). Not zero is a warning: `history_ensure_partition` moves them.
    pub default_rows: i64,
    /// The `at` of the oldest entry.
    pub oldest_at: Option<String>,
    /// `(partition, entries)`, oldest month first; the default partition is last.
    pub partitions: Vec<(String, i64)>,
    /// As [`ChangePage::horizon_lag_ms`], 0 when nothing is held back.
    pub horizon_lag_ms: i64,
    /// Entries numbered beyond the cluster (see [`ChangePage::foreign_entries`]). Not zero is a
    /// warning: the feed cannot show them until the owner renumbers them.
    pub foreign_rows: i64,
    /// Why the last attempt of this process to make the partitions of the current month failed,
    /// if it did. The entries went to the default partition; nothing was lost.
    pub maintenance_error: Option<String>,
}

/// What `history_ensure_partition` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PartitionEnsured {
    /// The partition was made by this call (it was not there).
    pub created: bool,
    /// Entries that had landed in the default partition and now sit in the month's partition.
    pub moved: u64,
}

/// The key of the epoch of the feed in the short-lived values of the store (never expires). A wipe
/// removes every such value, so the epoch ends with the data it counted.
pub const EPOCH_KEY: &str = "obop.history.epoch";

/// The error of a cursor that cannot be followed any more. The words before the colon are the
/// stable code (HTTP 410).
pub(crate) fn cursor_expired(why: &str) -> String {
    format!("cursor_expired: {why}")
}

/// The epoch of the feed of `b`, made at the first use after the data came to be (a new database,
/// a restore).
pub(crate) fn feed_epoch(b: &dyn Backend) -> Result<String> {
    use sha2::{Digest, Sha256};
    if let Some(e) = b.kv_get(EPOCH_KEY)? {
        return Ok(e);
    }
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut h = Sha256::new();
    h.update(std::process::id().to_le_bytes());
    h.update(
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .to_le_bytes(),
    );
    h.update(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
            .to_le_bytes(),
    );
    let fresh: String = h
        .finalize()
        .iter()
        .take(8)
        .map(|b| format!("{b:02x}"))
        .collect();
    // Two processes may race to make it: the one that stored first wins and both use it.
    b.kv_put_new(EPOCH_KEY, &fresh, None)?;
    b.kv_get(EPOCH_KEY)?
        .ok_or_else(|| "the epoch of the change feed could not be stored".to_string())
}

/// Where a feed question starts: `(txid, seq)` after which entries are wanted. A cursor of another
/// epoch, or one past the newest entry (`newest` is the highest number given out), is expired.
pub(crate) fn resume(epoch: &str, after: Option<&ChangeCursor>, newest: i64) -> Result<(i64, i64)> {
    let Some(c) = after else {
        return Ok((0, 0));
    };
    if c.epoch != epoch {
        return Err(cursor_expired(
            "the history was restored, or the database moved to another cluster or time line (a \
             promotion ends every cursor), since this cursor was made; read the feed again from \
             the start",
        ));
    }
    if c.seq > newest {
        return Err(cursor_expired(
            "this cursor is ahead of the history (the database went back in time); read the feed \
             again from the start",
        ));
    }
    Ok((c.txid, c.seq))
}

/// The limit of a feed question, after the checks every store makes of it: the instant, if any,
/// must be canonical.
pub(crate) fn feed_limit(q: &ChangeQuery) -> Result<usize> {
    if let Some(at) = &q.since_at
        && crate::sysfields::parse_micros(at).is_none()
    {
        return Err(format!("{at:?} is not a canonical instant"));
    }
    Ok(q.limit.min(FEED_PAGE_MAX))
}

/// Does the feed question ask for this entry (everything but the place in the feed)?
pub(crate) fn wanted(q: &ChangeQuery, r: &crate::sysfields::ChangeRow) -> bool {
    (q.models.is_empty() || q.models.contains(&r.model))
        && (q.ops.is_empty() || q.ops.contains(&r.op))
        && q.since_at.as_ref().is_none_or(|a| r.at >= *a)
}

/// The page that answers a question with `rows`: its cursor is after the last row, or where the
/// question began when there is none.
pub(crate) fn page(
    epoch: &str,
    q: &ChangeQuery,
    rows: Vec<crate::sysfields::ChangeRow>,
) -> ChangePage {
    let next = rows
        .last()
        .map(|r| ChangeCursor {
            epoch: epoch.to_string(),
            txid: r.txid,
            seq: r.seq,
        })
        .or_else(|| q.after.clone());
    ChangePage {
        rows,
        next,
        horizon_lag_ms: None,
        foreign_entries: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cursor_is_text_and_back_and_refuses_anything_else() {
        let c = ChangeCursor {
            epoch: "00ff00ff00ff00ff".into(),
            txid: 123_456,
            seq: 9,
        };
        assert_eq!(c.to_text(), "c1.00ff00ff00ff00ff.123456.9");
        assert_eq!(ChangeCursor::parse(&c.to_text()).unwrap(), c);
        for bad in [
            "",
            "c1",
            "c2.a.1.2",
            "c1..1.2",
            "c1.a.1",
            "c1.a.1.2.3",
            "c1.a.-1.2",
            "c1.a.1.x",
            "c1.a b.1.2",
            "c1.a.99999999999999999999.2",
        ] {
            let e = ChangeCursor::parse(bad).unwrap_err();
            assert!(e.starts_with("cursor_invalid:"), "{bad}: {e}");
        }
    }

    #[test]
    fn a_cursor_of_another_life_or_from_the_future_is_expired() {
        let c = |epoch: &str, seq| ChangeCursor {
            epoch: epoch.into(),
            txid: 0,
            seq,
        };
        assert_eq!(resume("e1", None, 5), Ok((0, 0)));
        assert_eq!(resume("e1", Some(&c("e1", 5)), 5), Ok((0, 5)));
        for expired in [
            resume("e1", Some(&c("e2", 1)), 5),
            resume("e1", Some(&c("e1", 6)), 5),
        ] {
            assert!(
                expired.unwrap_err().starts_with("cursor_expired:"),
                "stable code"
            );
        }
    }
}
