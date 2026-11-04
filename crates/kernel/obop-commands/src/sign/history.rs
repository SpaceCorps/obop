//! The history of a file on the signer's page (an amendment of D92 dated 2026-11-04, PROPOSED).
//!
//! A request can ask (`sign_request.show_history`, set by a manager) that the page of its link
//! lists who has already signed **the same file**: the requests that name the same Document, hold
//! the same SHA-256, are `signed` and ask for the history themselves. Each entry is the name the
//! signer typed and the time, in UTC to the second, and nothing else; the signer's own entry is
//! marked on the page that follows signing. A request that does not ask is shown the page it has
//! always been shown, and no query is made for it.
//!
//! The switch only counts while the installed package declares the field (`sign` 0.2.2): on an
//! instance that still has 0.2.1, or after `obop modules rollback sign`, the flag a stored record
//! may hold is inert, and the door neither fails nor logs.
//!
//! **Cost.** One `Backend::find` on `sign_request.document_hash`: the database filters the model's
//! rows and returns the ones that hold this file's hash (all states, flagged or not), which are then
//! filtered here. There is no index on that key, so the database reads every `sign_request` row to
//! compare it; what comes back, and what is parsed, is the handful of requests about one file. A
//! page view is limited to 120 per ten minutes per source, so a source cannot run it without bound.

use super::door::{Found, clean, unavailable};
use super::*;

/// The key of `sign_request` that switches the history on. Declared by `sign` 0.2.2.
pub const HISTORY_FLAG: &str = "show_history";

/// The most entries a page lists. The earliest are kept; the page says when the list is cut.
pub const HISTORY_MAX: usize = 50;

/// The longest name shown: the door refuses a longer one when it is typed.
const NAME_MAX: usize = 120;

/// One signature on the file: what the signer typed, and when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    /// The name as the signer typed it (not the name the request asked for), cleaned again.
    pub name: String,
    /// The signing time in UTC to the second, `2026-11-03T10:00:00Z`.
    pub at: String,
    /// This is the signature of the request whose link is open.
    pub own: bool,
}

/// The signatures so far on one file, oldest first.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct History {
    /// At most [`HISTORY_MAX`] entries.
    pub entries: Vec<HistoryEntry>,
    /// More signatures qualified than are listed.
    pub cut: bool,
}

impl SignService {
    /// The installed package declares [`HISTORY_FLAG`] on `sign_request`.
    fn history_declared(&self) -> bool {
        self.models
            .read()
            .unwrap()
            .get("sign_request")
            .is_some_and(|m| m.def.fields.iter().any(|f| f.name == HISTORY_FLAG))
    }

    /// The request asks for the history **and** the installed package knows the switch. The stored
    /// value counts only as the boolean `true`; the registry is looked at only for such a record,
    /// so a request without the switch costs nothing.
    pub(super) fn history_on(&self, req: &Fields) -> bool {
        matches!(req.get(HISTORY_FLAG), Some(J::Bool(true))) && self.history_declared()
    }

    /// The history for the page of `f`, or `None` when its request does not ask for one.
    pub(super) fn history_for(&self, f: &Found) -> Result<Option<History>, DoorError> {
        if !self.history_on(&f.req) {
            return Ok(None);
        }
        let (doc, hash) = (text(&f.req, "document_id"), text(&f.req, "document_hash"));
        if doc.is_empty() || hash.is_empty() {
            return Ok(None);
        }
        let rows = self
            .store
            .backend
            .find("sign_request", "document_hash", &J::from(hash))
            .map_err(unavailable)?;
        let mut signed: Vec<(i64, String, HistoryEntry)> = Vec::new();
        for (id, rec) in rows {
            // The same file twice over: the Document and the hash are both read again from the
            // record (the database compared the hash as a JSON value).
            if text(&rec, "document_id") != doc
                || text(&rec, "document_hash") != hash
                || text(&rec, "state") != "signed"
                || !matches!(rec.get(HISTORY_FLAG), Some(J::Bool(true)))
            {
                continue;
            }
            // A signature made at the door has a time and a name. A record without a readable
            // one (marked signed by an administrator) is left out rather than shown half.
            let Some(us) = instant_micros(text(&rec, "signed_at")) else {
                continue;
            };
            let name = clean(text(&rec, "signed_name"), NAME_MAX);
            if name.is_empty() {
                continue;
            }
            let at: String = micros_text(us).chars().take(19).collect();
            signed.push((
                us,
                id.clone(),
                HistoryEntry {
                    name,
                    at: format!("{at}Z"),
                    own: id == f.rid,
                },
            ));
        }
        signed.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        let cut = signed.len() > HISTORY_MAX;
        let entries = signed
            .into_iter()
            .take(HISTORY_MAX)
            .map(|(_, _, e)| e)
            .collect();
        Ok(Some(History { entries, cut }))
    }
}
