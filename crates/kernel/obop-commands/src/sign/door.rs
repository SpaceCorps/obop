//! The signer's door: what the page at `/sign/{request}/{token}` calls.
//!
//! The signer is not a principal. The link is a capability that reaches one held PDF and moves
//! one request. When that request asks for it (`show_history`, see `history.rs`) the page of the
//! link also lists the typed name and the time of the other signed requests of the same file:
//! the door reads those requests to find them, puts those two fields on the page, and changes
//! nothing of them. Every outcome that is not a success is one of [`DoorError`]; an unknown
//! link, a wrong token, a token for another request and a malformed address are all `NotFound`
//! and cannot be told apart.
//!
//! Rate limits (shared by the processes on one database, failing open when the store is down:
//! the 256-bit token is the real defence): the primary-key lookup comes first, so a valid link is
//! never refused because of the number of failures from its source; only a **failed** lookup is
//! counted (20 in ten minutes per source), then views (120) and posts (10 per source, 10 per
//! hour per link). There is no global bucket: it would let anyone lock every signer out.

use super::*;
use crate::audit;
use crate::blobs::Served;
use crate::devices::strip_hidden;
use crate::inline_pdf::inline_pdf;
use obop_storage::{ApplyError, Op, WriteCtx};
use std::collections::BTreeMap;
use std::net::IpAddr;
use std::str::FromStr;

const WINDOW_10_MIN: u64 = 600_000;

/// Who is asking, as the HTTP layer knows it.
#[derive(Debug, Clone, Default)]
pub struct Client {
    /// The address limits and evidence use, from the one rule every limiter of the server shares
    /// (`obop-http` `addr.rs`): the peer, or behind a trusted proxy the last entry of the last
    /// `X-Forwarded-For` line (the one the proxy wrote; the entries before it are what the client
    /// sent) when that entry is an address, and the peer when it is not. Empty for a call that
    /// did not come over a socket.
    pub addr: String,
    /// The peer of the socket, for the log.
    pub peer: String,
    /// Every `X-Forwarded-For` line, joined with ` | `, for the log.
    pub forwarded: String,
    pub user_agent: String,
}

impl Client {
    /// The canonical form of `addr` when it is an IP address (or an `ip:port` pair, which some
    /// proxies write), else `unknown`. Nothing else is ever stored as an address.
    pub(crate) fn ip(&self) -> String {
        let a = self.addr.trim();
        IpAddr::from_str(a)
            .or_else(|_| std::net::SocketAddr::from_str(a).map(|s| s.ip()))
            .map(|a| a.to_string())
            .unwrap_or_else(|_| "unknown".to_string())
    }

    /// A stand-in for the address in a counter key. An address that is not an IP address (a proxy
    /// that appends `ip:port`, junk in the header) is counted under the peer's address instead,
    /// so that all such clients do not share one bucket.
    fn source(&self) -> String {
        let ip = match self.ip().as_str() {
            "unknown" => IpAddr::from_str(self.peer.trim())
                .map(|a| a.to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
            known => known.to_string(),
        };
        token::short_hash("obop-sign-src:", &ip)
    }
}

/// Why the door did not do what was asked. Each variant is one HTTP status in `obop-http`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoorError {
    /// 404: nothing here, whatever the reason.
    NotFound,
    /// 410: the link or the request is over.
    Gone(Gone),
    /// 409: the document is not what was sent. Nothing was written.
    Changed,
    /// 429, with the seconds to wait.
    Busy(u64),
    /// 503: the store refused; nothing was written.
    Unavailable,
    /// 400: the form is not acceptable. Nothing was written.
    Invalid(Invalid),
    /// 409: another submit won or the request moved on.
    Refused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gone {
    Expired,
    Revoked,
    /// The request is no longer waiting for a signature (declined, expired, signed another way).
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invalid {
    Name,
    Agree,
    /// The request shows the signatures so far (`show_history`) and the form was made from a page
    /// that did not say so: the staff switch was turned on after the signer opened the page. Nothing
    /// was written; the page is shown again with the notice.
    History,
}

/// What the page shows for a link that works.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewData {
    pub title: String,
    pub message: String,
    pub signer_name: String,
    pub document_title: String,
    /// The SHA-256 of the document (hex); the form sends it back as `doc`.
    pub document_hash: String,
    pub expires_at: String,
    /// The signatures so far on this file, when the request asks for them (`show_history`); `None`
    /// for every other request, which is shown the page it has always been shown.
    pub history: Option<History>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    Form(ViewData),
    /// The link was used: the request is signed at this time.
    Signed {
        at: String,
        /// As [`ViewData::history`], with the signer's own entry marked.
        history: Option<History>,
    },
}

/// What the signer sent.
#[derive(Debug, Clone, Default)]
pub struct SignForm {
    pub name: String,
    pub agree: bool,
    /// The hash of the document the form was made for (the hidden field `doc`).
    pub doc: String,
    /// The id of the consent wording, the language and a hash of the words, for example
    /// `v1-draft/de/9ebff173` (derived by the HTTP layer from the page texts, `sign_text.rs`); an
    /// empty id is stored as `v1-draft`.
    pub consent_version: String,
    /// The same for a request that shows the signatures so far (`sign_text.rs`,
    /// `consent_version_with_history_for`): it also covers the sentence that tells the signer their
    /// name and time are shown to the others, and ends in [`HISTORY_CONSENT_MARK`], for example
    /// `v1-draft/de/d08d0039/history`. The door stores this one, and not `consent_version`, for
    /// such a request. Empty is stored as `v1-draft`, as above; a signature stored without the
    /// mark is never listed to the others.
    pub consent_version_history: String,
    /// The form says it came from a page that carried that sentence (the hidden field `hist`).
    /// This is the client's word: the door cannot tell what the browser was shown, only that a
    /// form for a request that asks must say so (see `submit`).
    pub saw_history: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Done {
    /// The link had been used before: nothing was written.
    pub already: bool,
}

pub(super) struct Found {
    pub(super) id: String,
    pub(super) rid: String,
    pub(super) link: Fields,
    pub(super) req: Fields,
}

/// A signed request shows the time it was signed. One that carries no time was not signed at the
/// door (an administrator moved it): the link is over and says nothing about a signature.
fn signed(f: &Found) -> Status {
    match text(&f.req, "signed_at") {
        "" => Status::Gone(Gone::Closed),
        at => Status::Signed(at.to_string()),
    }
}

enum Status {
    Usable,
    Signed(String),
    Gone(Gone),
}

pub(super) fn unavailable(e: String) -> DoorError {
    eprintln!("sign: the store refused: {e}");
    DoorError::Unavailable
}

/// Text from the client for a line of the security log: cleaned, and with the semicolons that
/// separate the fields of the line replaced, so that a forged header cannot start a field.
fn logged(raw: &str, max: usize) -> String {
    clean(raw, max).replace(';', ",")
}

/// A text from outside made safe for a log line or a record: hidden characters and line breaks
/// gone, cut to `max` characters.
pub(super) fn clean(raw: &str, max: usize) -> String {
    strip_hidden(&raw.chars().take(max * 2).collect::<String>())
        .chars()
        .take(max)
        .collect()
}

/// A text of staff with the characters that reorder or hide text taken out (bidirectional
/// controls and marks, zero-width space, word joiner, byte order mark). Line breaks stay, and so
/// do the two joiners that scripts need (zero-width joiner and non-joiner).
pub(crate) fn without_bidi(raw: &str) -> String {
    raw.chars().filter(|c| !is_bidi_or_hidden(*c)).collect()
}

fn is_bidi_or_hidden(c: char) -> bool {
    matches!(
        c,
        '\u{061c}'
            | '\u{200b}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{feff}'
    )
}

/// The name as typed, cleaned: at least two letters, 2 to 120 characters, no markup and no
/// character that reorders or hides text. A name over 400 characters is refused, not cut.
fn clean_name(raw: &str) -> Option<String> {
    if raw.chars().count() > 400 || raw.chars().any(is_bidi_or_hidden) {
        return None;
    }
    let n = strip_hidden(raw);
    let len = n.chars().count();
    (2..=120).contains(&len).then_some(())?;
    // No markup in a name: it is shown in pages and in the evidence, and no real name has one.
    (!n.contains(['<', '>'])).then_some(())?;
    (n.chars().filter(|c| c.is_alphabetic()).count() >= 2).then_some(n)
}

impl SignService {
    fn limit(&self, key: &str, max: u64, window_ms: u64) -> Result<(), DoorError> {
        obop_storage::limit::hit(&*self.store.backend, key, max, window_ms).map_err(DoorError::Busy)
    }

    /// A failed lookup: counted per source, and past the limit the answer is 429.
    fn fail(&self, client: &Client) -> DoorError {
        match self.limit(
            &format!("rl:sign:fail:{}", client.source()),
            20,
            WINDOW_10_MIN,
        ) {
            Ok(()) => DoorError::NotFound,
            Err(e) => e,
        }
    }

    /// The link and its request, or the reason there is none.
    fn find(&self, rid: &str, tok: &str, client: &Client) -> Result<Found, DoorError> {
        if !self.registered() {
            return Err(DoorError::NotFound);
        }
        if !token::token_shape(tok) || !token::rid_shape(rid) {
            return Err(self.fail(client));
        }
        let id = token::link_id(rid, tok);
        let Some(link) = self.get("sign_link", &id).map_err(unavailable)? else {
            return Err(self.fail(client));
        };
        // The id is a hash of the request id too, so this holds for every row the door made; it
        // fails for a row somebody wrote by hand.
        if !token::ct_eq(text(&link, "request_id"), rid) {
            return Err(self.fail(client));
        }
        let Some(req) = self.get("sign_request", rid).map_err(unavailable)? else {
            return Err(self.fail(client));
        };
        Ok(Found {
            id,
            rid: rid.to_string(),
            link,
            req,
        })
    }

    /// The link's own end and the end of the request's `expires_on` day, whichever is first, have
    /// passed (the clock of the service). A link without a readable end has ended: it does not
    /// work. The one rule of the page, of the PDF and of the history of the file. Where it is
    /// looked at is the order of [`Self::status`]: for a link that was used, or that was not used
    /// and belongs to a signed request, `status` does not look at it at all (the page after
    /// signing is given whatever the end) and only the list on that page stops with it.
    pub(super) fn ended(&self, f: &Found) -> bool {
        let mut end = instant_micros(text(&f.link, "expires_at")).unwrap_or(i64::MIN);
        if let Some(day_end) = end_of_day_micros(text(&f.req, "expires_on")) {
            end = end.min(day_end);
        }
        self.now_micros() > end
    }

    /// What a link shows, by the first of these that holds (docs/SIGN.md, "When the list is
    /// served"; `the_door_looks_at_a_link_in_the_order_the_documents_state` pins it):
    ///
    /// 1. the link was used (`used_at`): the page after signing when the request is signed and
    ///    carries a time, else "closed"; neither `revoked_at` nor the end is looked at;
    /// 2. the link was not used and is revoked: "revoked" (410), whatever the request is;
    /// 3. the link was not used and the request is signed: the page after signing, or "closed"
    ///    when it carries no time; the end of the link is not looked at;
    /// 4. the link has ended ([`Self::ended`]): "expired";
    /// 5. the request is not `sent`: "closed";
    /// 6. the form.
    fn status(&self, f: &Found) -> Status {
        if !text(&f.link, "used_at").is_empty() {
            return if text(&f.req, "state") == "signed" {
                signed(f)
            } else {
                Status::Gone(Gone::Closed)
            };
        }
        if !text(&f.link, "revoked_at").is_empty() {
            return Status::Gone(Gone::Revoked);
        }
        // The request is signed and this link was not the one that signed it: either another
        // link won a race (the link and the request are read one after the other, and the batch
        // that signs commits between the two reads) or a second link was made. Both see the end.
        if text(&f.req, "state") == "signed" {
            return signed(f);
        }
        if self.ended(f) {
            return Status::Gone(Gone::Expired);
        }
        if text(&f.req, "state") != "sent" {
            return Status::Gone(Gone::Closed);
        }
        Status::Usable
    }

    /// One `sign_refused` entry per link and reason in ten minutes. Unknown links are never
    /// logged, only counted.
    fn refused(&self, f: &Found, reason: &str, client: &Client) {
        let id12: String = f.id.chars().take(12).collect();
        let key = format!("sign:refused:{id12}:{reason}");
        if let Ok(true) = self
            .store
            .backend
            .kv_put_new(&key, "1", Some(WINDOW_10_MIN))
        {
            audit::record_event(
                &*self.store.backend,
                u64::try_from(self.now_micros() / MICROS_PER_SECOND).unwrap_or(0),
                &format!("sign_request:{}", f.rid),
                "sign_refused",
                &format!("signer:link:{id12}"),
                &format!(
                    "link {id12}; reason {reason}; address {}; peer {}; forwarded {}",
                    client.ip(),
                    logged(&client.peer, 64),
                    logged(&client.forwarded, 200)
                ),
            );
        }
    }

    /// The Document must still hold the file that was sent, and the held copy must be a PDF of
    /// the recorded id: the cheap check of a page view (ids and sizes, no hashing).
    fn cheap_integrity(&self, f: &Found, client: &Client) -> Result<Fields, DoorError> {
        let (hash, file, did) = (
            text(&f.req, "document_hash"),
            text(&f.req, "document_file"),
            text(&f.req, "document_id"),
        );
        let changed = |why: &str| {
            self.refused(f, why, client);
            DoorError::Changed
        };
        if hash.is_empty() || file != hash || did.is_empty() {
            return Err(changed("no_copy"));
        }
        let held = self.store.blobs.info(file).map_err(unavailable)?;
        match held {
            Some(i) if i.content_type == "application/pdf" && i.size <= MAX_PDF => {}
            _ => return Err(changed("copy_missing")),
        }
        let doc = self
            .get("documents_document", did)
            .map_err(unavailable)?
            .ok_or_else(|| changed("document_gone"))?;
        if text(&doc, "attachment") != hash {
            return Err(changed("changed"));
        }
        Ok(doc)
    }

    /// The page of a link.
    pub fn view(&self, rid: &str, tok: &str, client: &Client) -> Result<View, DoorError> {
        let f = self.find(rid, tok, client)?;
        self.limit(
            &format!("rl:sign:view:{}", client.source()),
            120,
            WINDOW_10_MIN,
        )?;
        match self.status(&f) {
            Status::Signed(at) => Ok(View::Signed {
                // The end page of a link that is over, or of a used link that was revoked, is the
                // page it always was: no list (`history_for_end_page`).
                history: self.history_for_end_page(&f)?,
                at,
            }),
            Status::Gone(g) => Err(DoorError::Gone(g)),
            Status::Usable => {
                let doc = self.cheap_integrity(&f, client)?;
                // What staff typed, as shown to the signer: cut, and without characters that
                // reorder or hide text (the consent sentence holds the requested name).
                let cut = |s: &str, n: usize| without_bidi(&s.chars().take(n).collect::<String>());
                Ok(View::Form(ViewData {
                    title: cut(text(&f.req, "title"), 200),
                    message: cut(text(&f.req, "message"), 2000),
                    signer_name: cut(text(&f.req, "signer_name"), 200),
                    document_title: cut(text(&doc, "title"), 200),
                    document_hash: text(&f.req, "document_hash").to_string(),
                    expires_at: text(&f.link, "expires_at").to_string(),
                    history: self.history_for(&f)?,
                }))
            }
        }
    }

    /// The held copy of the request of a link, for the page's viewer. Only while the link works:
    /// after signing the answer is 410. The bytes are hashed again before they are served (up to
    /// 25 MiB, a fraction of a second, at most 120 views in ten minutes from one source), so that
    /// a copy that was changed in the store is never shown as the document; the page itself does
    /// not read the bytes.
    pub fn pdf(&self, rid: &str, tok: &str, client: &Client) -> Result<Served, DoorError> {
        let f = self.find(rid, tok, client)?;
        self.limit(
            &format!("rl:sign:view:{}", client.source()),
            120,
            WINDOW_10_MIN,
        )?;
        match self.status(&f) {
            Status::Signed(_) => return Err(DoorError::Gone(Gone::Closed)),
            Status::Gone(g) => return Err(DoorError::Gone(g)),
            Status::Usable => {}
        }
        self.cheap_integrity(&f, client)?;
        let file = text(&f.req, "document_file");
        let bytes = self
            .store
            .blobs
            .read_all(file, MAX_PDF)
            .map_err(unavailable)?
            .ok_or_else(|| {
                self.refused(&f, "copy_missing", client);
                DoorError::Changed
            })?;
        if token::sha256_hex(&bytes) != file {
            self.refused(&f, "tampered", client);
            return Err(DoorError::Changed);
        }
        inline_pdf(Served {
            content_type: "application/pdf".to_string(),
            bytes,
            etag: format!("\"{file}\""),
            filename: String::new(),
            attachment: false,
            width: None,
            height: None,
        })
        .map_err(|_| DoorError::Changed)
    }

    /// The signer signs. One batch, or nothing.
    pub fn submit(
        &self,
        rid: &str,
        tok: &str,
        form: &SignForm,
        client: &Client,
    ) -> Result<Done, DoorError> {
        let f = self.find(rid, tok, client)?;
        self.limit(
            &format!("rl:sign:post:{}", client.source()),
            10,
            WINDOW_10_MIN,
        )?;
        self.limit(&format!("rl:sign:postlink:{}", &f.id[..24]), 10, 3_600_000)?;
        match self.status(&f) {
            // A second submit changes nothing, whatever name it carries.
            Status::Signed(_) => return Ok(Done { already: true }),
            Status::Gone(g) => {
                self.refused(&f, "gone_post", client);
                return Err(DoorError::Gone(g));
            }
            Status::Usable => {}
        }
        // A request that shows the signatures so far tells the signer so on its page. A form that
        // does not say it came from such a page (the switch was turned on after the page was made)
        // is not signed. What this keeps true is narrow: a request that asks is never signed under
        // the id of the wording without the sentence, and one that does not ask is never signed
        // under the id with it (the batch below compares the switch with what is read here). The
        // field `hist` is the client's word, so it does not prove that the browser showed the
        // sentence: a script that posts it without opening the page is stored under that id too.
        let history = self.history_on(&f.req);
        if history && !form.saw_history {
            return Err(DoorError::Invalid(Invalid::History));
        }
        let name = clean_name(&form.name).ok_or(DoorError::Invalid(Invalid::Name))?;
        if !form.agree {
            return Err(DoorError::Invalid(Invalid::Agree));
        }
        let hash = text(&f.req, "document_hash").to_string();
        if !token::ct_eq(&form.doc, &hash) {
            self.refused(&f, "stale_form", client);
            return Err(DoorError::Changed);
        }
        let doc_id = text(&f.req, "document_id").to_string();
        self.cheap_integrity(&f, client)?;
        // The held copy is read and hashed here and when the PDF is served, at no page view.
        let held = self
            .store
            .blobs
            .read_all(&hash, MAX_PDF)
            .map_err(unavailable)?;
        match held {
            Some(b) if token::sha256_hex(&b) == hash => {}
            _ => {
                self.refused(&f, "tampered", client);
                return Err(DoorError::Changed);
            }
        }

        let at = self.now_micros();
        let signed_at = micros_text(at);
        let signed_on: String = signed_at.chars().take(10).collect();
        let ip = client.ip();
        let agent = clean(&client.user_agent, 300);
        let reference = format!("typed:{name}");
        let wording = if history {
            &form.consent_version_history
        } else {
            &form.consent_version
        };
        let consent = match clean(wording, 40) {
            c if c.is_empty() => "v1-draft".to_string(),
            c => c,
        };
        let id12: String = f.id.chars().take(12).collect();
        let digest = evidence_digest(&[
            ("consent_version", consent.as_str()),
            ("document_hash", hash.as_str()),
            ("link_id", f.id.as_str()),
            ("request_id", f.rid.as_str()),
            ("signed_at", signed_at.as_str()),
            ("signed_name", name.as_str()),
            ("signer_ip", ip.as_str()),
            ("signer_user_agent", agent.as_str()),
            ("signature_reference", reference.as_str()),
        ]);

        let mut link_patch = Fields::new();
        link_patch.insert("used_at".into(), J::from(signed_at.clone()));
        let mut req_patch = Fields::new();
        for (k, v) in [
            ("state", "signed"),
            ("signed_at", signed_at.as_str()),
            ("signed_on", signed_on.as_str()),
            ("signed_name", name.as_str()),
            ("signer_ip", ip.as_str()),
            ("signer_user_agent", agent.as_str()),
            ("signature_reference", reference.as_str()),
            ("consent_version", consent.as_str()),
        ] {
            req_patch.insert(k.into(), J::from(v));
        }
        let ops = [
            // Lock order is always link, request, document.
            Op::Patch {
                model: "sign_link".into(),
                id: f.id.clone(),
                patch: link_patch,
                expect: vec![("used_at".into(), J::Null), ("revoked_at".into(), J::Null)],
            },
            Op::Patch {
                model: "sign_request".into(),
                id: f.rid.clone(),
                patch: req_patch,
                expect: vec![
                    ("state".into(), J::from("sent")),
                    ("document_hash".into(), J::from(hash.clone())),
                    // The switch is what the door read: one turned on or off after the read would
                    // leave a signature under the wording of the other (the id stored above).
                    (
                        HISTORY_FLAG.into(),
                        f.req.get(HISTORY_FLAG).cloned().unwrap_or(J::Null),
                    ),
                ],
            },
            // An empty patch that only checks, in the same transaction, that the Document still
            // holds the file that was shown.
            Op::Patch {
                model: "documents_document".into(),
                id: doc_id,
                patch: Fields::new(),
                expect: vec![("attachment".into(), J::from(hash.clone()))],
            },
            audit::event_op(
                u64::try_from(at / MICROS_PER_SECOND).unwrap_or(0),
                &format!("sign_request:{}", f.rid),
                "sign_signed",
                &format!("signer:link:{id12}"),
                &format!(
                    "request {}; link {id12}; document {hash}; consent {consent}; digest {digest}; address {ip}; peer {}; forwarded {}",
                    f.rid,
                    logged(&client.peer, 64),
                    logged(&client.forwarded, 200)
                ),
            ),
        ];
        let ctx = WriteCtx::new(
            &format!("signer:link:{id12}"),
            "public",
            "sign_request.sign_by_link",
        )
        .primary("sign_request", &f.rid)
        .ticket("sign_request", &f.rid, "sign");
        match self.store.backend.apply_ctx(&ctx, &ops) {
            Ok(()) => Ok(Done { already: false }),
            Err(ApplyError::Conflict { .. }) => self.after_conflict(&f, client),
            Err(ApplyError::Storage(e)) => Err(unavailable(e)),
        }
    }

    /// A batch was refused because something changed since it was read. If the link was used and
    /// the request is signed, another submit won: say so and write nothing.
    fn after_conflict(&self, f: &Found, client: &Client) -> Result<Done, DoorError> {
        let link = self.get("sign_link", &f.id).map_err(unavailable)?;
        let req = self.get("sign_request", &f.rid).map_err(unavailable)?;
        if let (Some(l), Some(r)) = (&link, &req)
            && !text(l, "used_at").is_empty()
            && text(r, "state") == "signed"
        {
            return Ok(Done { already: true });
        }
        if let Some(r) = req {
            let again = Found {
                id: f.id.clone(),
                rid: f.rid.clone(),
                link: link.unwrap_or_default(),
                req: r,
            };
            if self.cheap_integrity(&again, client).is_err() {
                return Err(DoorError::Changed);
            }
        }
        Err(DoorError::Refused)
    }
}

/// SHA-256 of the sorted `key=value` lines: what the log entry vouches for.
fn evidence_digest(pairs: &[(&str, &str)]) -> String {
    let sorted: BTreeMap<&str, &str> = pairs.iter().copied().collect();
    let mut s = String::new();
    for (k, v) in sorted {
        s.push_str(k);
        s.push('=');
        s.push_str(v);
        s.push('\n');
    }
    token::sha256_hex(s.as_bytes())
}

/// The digest of the evidence stored on a signed request, in the form the door wrote it, for a
/// reader that wants to compare it with the `sign_signed` entry of the security log.
pub fn digest_of_request(link_id: &str, request_id: &str, req: &Fields) -> String {
    evidence_digest(&[
        ("consent_version", text(req, "consent_version")),
        ("document_hash", text(req, "document_hash")),
        ("link_id", link_id),
        ("request_id", request_id),
        ("signed_at", text(req, "signed_at")),
        ("signed_name", text(req, "signed_name")),
        ("signer_ip", text(req, "signer_ip")),
        ("signer_user_agent", text(req, "signer_user_agent")),
        ("signature_reference", text(req, "signature_reference")),
    ])
}
