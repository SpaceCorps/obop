//! The history of a file on the signer's page (an amendment of D92 dated 2026-11-04, PROPOSED).
//!
//! A request can ask (`sign_request.show_history`, set by a manager) that the page of its link
//! lists who has already signed **the same file**: the requests that name the same Document, hold
//! the same SHA-256, are `signed`, ask for the history themselves, and were signed on a page that
//! told the signer so (the consent id stored with the signature carries [`HISTORY_CONSENT_MARK`]).
//! Each entry is the name the signer typed and the time, in UTC to the second, and nothing else;
//! the signer's own entry is marked on the page that follows signing. A request that does not ask
//! is shown the page it has always been shown, and no query is made for it. The page that follows
//! signing lists them only while its link is within its end and not revoked
//! ([`SignService::history_for_end_page`]).
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

/// The fourth and last part of the consent id of a signature that was made on a page that told the
/// signer that their name and the time they sign are shown to the others who sign the file, for
/// example `v1-draft/en/e2880db0/history`. The first three parts are as in every consent id (the
/// id of the wording, the language, the hash of the words). The mark is **stored with the
/// signature**: whether a signature is listed is decided by it and by nothing that is worked out
/// again, so an edit of the sentence moves the hash of the signatures that follow and leaves the
/// ones already made listed. An id without the mark never is, whatever its other parts say. The
/// mark is put only by the page texts of `obop-http` (`consent_version_with_history_for`) and must
/// never be put on words that do not say that.
pub const HISTORY_CONSENT_MARK: &str = "history";

/// The consent id says that the signer was told that their name and the time are shown to the
/// others: four parts, the last one [`HISTORY_CONSENT_MARK`], the first three not empty. Nothing
/// else counts, so an id of the wording without the sentence is not one however it was written.
pub fn consent_names_the_history(consent_version: &str) -> bool {
    let parts: Vec<&str> = consent_version.split('/').collect();
    parts.len() == 4 && parts[3] == HISTORY_CONSENT_MARK && parts[..3].iter().all(|p| !p.is_empty())
}

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

    /// The history for the page that follows signing (`View::Signed`), or `None`. `status()` gives
    /// that page to two kinds of link, and the order in it matters (see there): a link that was
    /// used, whatever its `revoked_at` and its end, and a link that was not used, is not revoked
    /// and belongs to a signed request, whatever its end. A link that was not used and is revoked
    /// is answered 410 before the request is looked at, so it never gets here. The list is not
    /// served with the page as a matter of course: only while the link is within its end, the end
    /// of the link and of the request's `expires_on` day, by the rule of the page and of the PDF
    /// (`ended`), and only while `revoked_at` is empty, which can matter for the link that
    /// signed alone (an administrator can write it on a link that was used; the page stays). A
    /// link that is over, or a used link that was revoked, is shown the page it was always shown,
    /// with nobody's name on it, not even its own signer's, and no query is made for it.
    pub(super) fn history_for_end_page(&self, f: &Found) -> Result<Option<History>, DoorError> {
        if !self.history_on(&f.req) {
            return Ok(None);
        }
        if !text(&f.link, "revoked_at").is_empty() || self.ended(f) {
            return Ok(None);
        }
        self.history_for(f)
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
                // The signer read the sentence: the id stored with the signature says so. A
                // request signed before the box was ticked, while the package was rolled back, or
                // marked by hand is not listed to the others, whatever its box says now.
                || !consent_names_the_history(text(&rec, "consent_version"))
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
