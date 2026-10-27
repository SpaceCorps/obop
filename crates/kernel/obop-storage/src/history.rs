//! The questions one can ask the change history of a store (D75), and the answer to the one
//! about stamping old records. The entries themselves are [`crate::sysfields::ChangeRow`].
//!
//! Memory and SQLite keep the history; PostgreSQL gets its history store in a later step, until
//! then the defaults of [`crate::Backend`] say so (they refuse, they do not answer "no entries").

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
