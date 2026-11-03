//! Signing a Document through a link (D92, PROPOSED): the rules around `sign_request` and
//! `sign_link`, and the door a signer uses.
//!
//! - **Send** (`sign_request.send_document`, a wrapper): takes the SHA-256 of the PDF in
//!   `document_id` (its blob id), keeps an exact copy for the request (`document_file`), and only
//!   then lets the generated transition move the request to `sent`. The copy, not the Document,
//!   is what the signer sees and what is hashed, so replacing the Document's file later cannot
//!   change what was signed.
//! - **Link** (`sign_link.create`, a wrapper that replaces the generated create): a person with
//!   `sign.manager` makes a link for a sent request. 256 bits from the operating system, shown
//!   once; only the SHA-256 of (domain, request id, token) is kept, as the record id.
//! - **Door** ([`SignService::view`], [`SignService::pdf`], [`SignService::submit`]): what
//!   `obop-http` calls for `/sign/{request}/{token}`. The signer is not a principal and holds no
//!   group; the link is the capability. The door reads one request, its held copy and its link,
//!   and writes one batch (link used, request signed, a `sign_signed` security event).
//! - **Guards on the generic writes**: the evidence fields belong to the door; `sign_request`
//!   `create`, `update` and `bulk_create` refuse them from everyone but an administrator, a
//!   signed request is changed or deleted by an administrator only, and `sign_link` takes no
//!   write but its own `create` and the stamping of `revoked_at`.
//!
//! Honest limits (also in docs/SIGN.md and D92): this is a simple electronic signature. A typed
//! name does not prove who typed it, the time is the server clock, the address is what the proxy
//! chain reports, and an administrator (or anyone with the database) can still change a record.
//! The security log entry written at signing carries a digest of the evidence as it stood.

mod door;
mod guards;
mod link;
mod send;
mod token;

pub use door::{
    Client, Done, DoorError, Gone, Invalid, SignForm, View, ViewData, digest_of_request,
};

use crate::engine::Models as ModelsMap;
use crate::store::Store;
use crate::{Dispatcher, Principal};
use obop_storage::Fields;
use obop_values::datetime::Instant;
use serde_json::Value as J;
use std::sync::{Arc, RwLock};

/// SHA-256 of `bytes` as lower-case hex: the form of a blob id and of a document hash.
pub fn sha256_hex(bytes: &[u8]) -> String {
    token::sha256_hex(bytes)
}

/// The largest PDF that is held and shown: the cap of a file field.
pub(crate) const MAX_PDF: u64 = crate::inline_pdf::MAX_INLINE_PDF as u64;

/// Days a new link works, unless the caller asks for less or the request ends sooner.
pub(crate) const LINK_DAYS: i64 = 14;

const MICROS_PER_SECOND: i64 = 1_000_000;
const MICROS_PER_DAY: i64 = 86_400 * MICROS_PER_SECOND;

/// The keys of `sign_request` that are the evidence: the signer door (and, for the hash and the
/// copy, `send_document`) write them and nobody else sets them. A field group restricts reading
/// only, so the refusal is made by a wrapper. The two dates `sent_on` and `signed_on` are written
/// by the same code but are not evidence (the evidence is the time and the digest) and are not
/// guarded: a person with `sign.user` can set them (docs/ISSUES.md O34).
pub(crate) const DOOR_OWNED: [&str; 8] = [
    "document_hash",
    "document_file",
    "signed_at",
    "signed_name",
    "consent_version",
    "signer_ip",
    "signer_user_agent",
    "signature_reference",
];

type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

pub struct SignService {
    pub(crate) models: ModelsMap,
    pub(crate) store: Store,
    /// Unix time in microseconds. Replaceable so tests can cross an expiry.
    clock: RwLock<Clock>,
    /// `OBOP_PUBLIC_URL` without a trailing slash, when set: the link is returned with it.
    public_url: RwLock<Option<String>>,
}

impl SignService {
    /// Make the service and put its wrappers around the commands of `sign` (they act only when
    /// the package is installed: a wrapper for a command nobody registered is never called).
    pub(crate) fn install(d: &Dispatcher, store: &Store) -> Arc<Self> {
        let svc = Arc::new(Self {
            models: d.models.clone(),
            store: store.clone(),
            clock: RwLock::new(Arc::new(system_micros)),
            public_url: RwLock::new(
                std::env::var("OBOP_PUBLIC_URL")
                    .ok()
                    .map(|u| u.trim().trim_end_matches('/').to_string())
                    .filter(|u| !u.is_empty()),
            ),
        });
        guards::install(d, &svc);
        send::install(d, &svc);
        link::install(d, &svc);
        svc
    }

    /// Replace the clock (Unix microseconds). For tests.
    pub fn set_clock(&self, clock: Arc<dyn Fn() -> i64 + Send + Sync>) {
        *self.clock.write().unwrap() = clock;
    }

    /// Set the public origin the returned link starts with (tests; the server reads
    /// `OBOP_PUBLIC_URL` itself).
    pub fn set_public_url(&self, url: Option<&str>) {
        *self.public_url.write().unwrap() = url
            .map(|u| u.trim_end_matches('/').to_string())
            .filter(|u| !u.is_empty());
    }

    pub(crate) fn now_micros(&self) -> i64 {
        (self.clock.read().unwrap())()
    }

    pub(crate) fn now_text(&self) -> String {
        Instant::from_micros(self.now_micros())
            .map(Instant::canonical)
            .unwrap_or_default()
    }

    /// Today as `YYYY-MM-DD` (UTC).
    pub(crate) fn today(&self) -> String {
        self.now_text().chars().take(10).collect()
    }

    /// The package is installed with its link model: without it the door answers 404 for
    /// everything (a binary deployed without the package upgrade exposes nothing).
    pub(crate) fn registered(&self) -> bool {
        let m = self.models.read().unwrap();
        m.contains_key("sign_link") && m.contains_key("sign_request")
    }

    pub(crate) fn public_url(&self) -> Option<String> {
        self.public_url.read().unwrap().clone()
    }

    pub(crate) fn get(&self, model: &str, id: &str) -> Result<Option<Fields>, String> {
        self.store.backend.get(model, id)
    }
}

fn system_micros() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_micros()).ok())
        .unwrap_or(0)
}

impl Dispatcher {
    /// The service behind the signer door and the wrappers of `sign` (D92); available once a
    /// module has been registered against a store.
    pub fn sign_service(&self) -> Option<Arc<SignService>> {
        self.sign_service.get().cloned()
    }
}

// ---- small helpers shared by the parts ---------------------------------------------------

/// A text field of a record, or `""` when it is missing, null or not text.
pub(crate) fn text<'a>(rec: &'a Fields, key: &str) -> &'a str {
    match rec.get(key) {
        Some(J::String(s)) => s,
        _ => "",
    }
}

/// The id of the record a call is about.
pub(crate) fn id_of(input: &crate::Value) -> Result<String, String> {
    crate::store::id_of(input)
}

/// Whether `p` is an instance administrator (holds every group): a person. An agent token whose
/// scopes happen to include `platform.admin` is not one here, so that no agent is the exception to
/// the rules about the evidence and about a signed request.
pub(crate) fn is_admin(p: &Principal) -> bool {
    p.is_person() && p.allows(crate::ADMIN_GROUP)
}

/// The micros since the epoch of the end of the day `YYYY-MM-DD` (23:59:59.999999 UTC).
pub(crate) fn end_of_day_micros(date: &str) -> Option<i64> {
    let (y, m, d) = obop_values::datetime::parse_date(date)?;
    let days = obop_values::datetime::days_from_civil(y, m, d);
    Some(days * MICROS_PER_DAY + MICROS_PER_DAY - 1)
}

/// `expires_at` of a link record as micros; `None` when it is missing or not an instant.
pub(crate) fn instant_micros(s: &str) -> Option<i64> {
    Instant::parse(s).ok().map(Instant::micros)
}

pub(crate) fn micros_text(us: i64) -> String {
    Instant::from_micros(us)
        .map(Instant::canonical)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
