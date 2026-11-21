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
//!   and writes one batch (link used, request signed, a `sign_signed` security event). When the
//!   request asks for it (`show_history`), the door also reads the other requests that hold the
//!   same file's hash and puts two fields of the signed ones, the typed name and the time, on the
//!   page; it writes nothing for them.
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
mod evidence;
mod guards;
mod history;
mod link;
mod send;
mod token;

pub use door::{
    Client, Done, DoorError, Gone, Invalid, SignForm, View, ViewData, digest_of_request,
};
pub use history::{
    HISTORY_CONSENT_MARK, HISTORY_FLAG, HISTORY_MAX, History, HistoryEntry,
    consent_names_the_history,
};

use crate::engine::Models as ModelsMap;
use crate::store::Store;
use crate::{Dispatcher, Principal};
use obop_storage::Fields;
use obop_values::datetime::Instant;
use serde_json::Value as J;
use std::sync::{Arc, RwLock};

/// The wording digest the signer's page carries as `words` for this request as stored (the
/// title, the message and the requested name). `pub` for the tests of the `obop` binary, which
/// post the form as a page would.
#[doc(hidden)]
pub fn page_wording(req: &Fields) -> String {
    evidence::wording_digest(req)
}

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
/// only, so the refusal is made by a wrapper. The two dates `sent_on` and `signed_on` are not
/// evidence (the evidence is the time and the digest), but they are written by the same code (and
/// by the effects of `send` and `sign`), so they are guarded too: a request that was not signed
/// cannot show a signing day (docs/ISSUES.md O34).
///
/// `pub` only so that the tests of the `obop` binary can compare their own list of the keys with
/// this one (`sign_tests/rules.rs`); nothing else reads it from outside.
#[doc(hidden)]
pub const DOOR_OWNED: [&str; 10] = [
    "sent_on",
    "signed_on",
    "document_hash",
    "document_file",
    "signed_at",
    "signed_name",
    "consent_version",
    "signer_ip",
    "signer_user_agent",
    "signature_reference",
];

/// The models whose guards act in `Dispatcher::invoke` (the wrappers of this module): an action
/// effect writes through the engine and skips them, so only an action of `sign` itself may write
/// them (docs/ISSUES.md O35). The install gate refuses an effect of another package on them, and
/// the engine refuses it again when it runs.
pub const OWN_EFFECTS_ONLY: [&str; 2] = ["sign_request", "sign_link"];

/// Whether an action effect of `module` may write `model` as far as this module is concerned.
pub fn effect_may_write_from(model: &str, module: &str) -> bool {
    !OWN_EFFECTS_ONLY.contains(&model) || module == "sign"
}

type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

pub struct SignService {
    pub(crate) models: ModelsMap,
    pub(crate) store: Store,
    /// Unix time in microseconds. Replaceable so tests can cross an expiry.
    clock: RwLock<Clock>,
    /// `OBOP_PUBLIC_URL` without a trailing slash, when set: the link is returned with it.
    public_url: RwLock<Option<String>>,
    /// The dispatcher's time zone settings (`OBOP_ZONE`, D72): the days of a request (`sent_on`,
    /// `signed_on`, the last day `expires_on`) are days of the instance's zone.
    zones: crate::engine::SharedZones,
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
            zones: d.zones.clone(),
        });
        guards::install(d, &svc);
        send::install(d, &svc);
        link::install(d, &svc);
        evidence::install(d, &svc);
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

    /// Today as `YYYY-MM-DD` in the instance's zone.
    pub(crate) fn today(&self) -> String {
        self.day_of(self.now_micros())
    }

    /// The day `YYYY-MM-DD` of the instant `us` in the instance's zone.
    pub(crate) fn day_of(&self, us: i64) -> String {
        Instant::from_micros(us)
            .map(|at| self.zones.read().unwrap().today(at))
            .unwrap_or_default()
    }

    /// The micros since the epoch of the end of the day `YYYY-MM-DD` in the instance's zone (the
    /// last microsecond before the next local midnight).
    pub(crate) fn end_of_day_micros(&self, date: &str) -> Option<i64> {
        let utc_end = end_of_day_micros(date)?;
        let zones = self.zones.read().unwrap().clone();
        let zone = zones.platform();
        // The local midnight that ends the day: UTC midnight moved by the offset in force then
        // (looked up twice, so that a change of offset in the last hours of the day is followed).
        let midnight = utc_end + 1;
        let off = |us: i64| {
            Instant::from_micros(us).map_or(0, |at| i64::from(zone.offset_secs(at)))
                * MICROS_PER_SECOND
        };
        let first = midnight - off(midnight);
        Some(midnight - off(first) - 1)
    }

    /// The package is installed with its link model: without it the door answers 404 for
    /// everything (a binary deployed without the package upgrade exposes nothing).
    pub(crate) fn registered(&self) -> bool {
        let m = self.models.read().unwrap();
        m.contains_key("sign_link") && m.contains_key("sign_request")
    }

    /// What the server says at start about the signer door (docs/ISSUES.md O29): with `sign`
    /// installed and no `OBOP_PUBLIC_URL`, a new link is a bare path, and the door checks the
    /// origin of a post against the `Host` header, over https only when the proxy sends
    /// `X-Forwarded-Proto: https`; a proxy that does not would have every signature refused.
    /// `None` when there is nothing to say.
    pub fn start_up_note(&self) -> Option<String> {
        if !self.registered() || self.public_url().is_some() {
            return None;
        }
        Some(
            "sign: OBOP_PUBLIC_URL is not set, so a new signing link is only a path (/sign/...) and the signer page checks where a post came from against the Host header, over https only when the proxy sends X-Forwarded-Proto: https; set OBOP_PUBLIC_URL to the public https address of this instance (docs/SIGN.md)"
                .to_string(),
        )
    }

    pub(crate) fn public_url(&self) -> Option<String> {
        self.public_url.read().unwrap().clone()
    }

    pub(crate) fn get(&self, model: &str, id: &str) -> Result<Option<Fields>, String> {
        self.store.backend.get(model, id)
    }

    /// The record `id` of `model` as `p` may see it by the model's row rules (D58), or `None`:
    /// a request about a Document the caller cannot see, and its links, do not exist for them
    /// (docs/ISSUES.md O4). The wrappers look a record up through this, so that their refusals
    /// never tell such a caller what the record is; the generated handler then answers "no ...".
    pub(crate) fn visible_record(
        &self,
        p: &Principal,
        model: &str,
        id: &str,
    ) -> Result<Option<Fields>, String> {
        let models = self.models.read().unwrap();
        let Some(def) = models.get(model).map(|m| m.def.clone()) else {
            return self.get(model, id);
        };
        let lookup = |n: &str| models.get(n).map(|m| m.def.clone());
        let reads = crate::engine::unit::Plain(&*self.store.backend);
        let actor = crate::engine::Actor::of(p);
        let gate = crate::engine::access::Gate::new(&reads, &lookup, &actor);
        match self.get(model, id)? {
            Some(rec) if gate.visible(&def, &rec) => Ok(Some(rec)),
            _ => Ok(None),
        }
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

/// The micros since the epoch of the end of the day `YYYY-MM-DD` (23:59:59.999999 UTC); the
/// rules use [`SignService::end_of_day_micros`], the end in the instance's zone.
fn end_of_day_micros(date: &str) -> Option<i64> {
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
