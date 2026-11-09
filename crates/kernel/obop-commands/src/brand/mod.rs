//! The instance brand as the server reads and writes it (docs/spec/branding.md, PROPOSED; decision
//! D94 is reserved, no ruling number is assigned). The rules of the brand itself (the character
//! policy, the slug, the strict SVG reader, the tile) are in the leaf crate `obop-brand`; this
//! module is everything that touches the store:
//!
//! - [`Reader`]: what a page reads. [`Reader::current`] answers from a cell that lives two seconds
//!   (one unit of the row and the theme setting, single flight, the last good value of any age, the
//!   default only in a process that never had a good read); [`Reader::fresh`] is the uncached read
//!   of the row and the theme that `get` makes, and returns a store failure as an error, never as a
//!   brand that is not set; [`strict`] reads the row alone, every time, for the text that is written
//!   into a person's own device.
//! - [`apply`]: the one function that writes the row (`configure` and `clear`, on every surface),
//!   under the lock `brand.write`, with the audit events in the same atomic batch.
//! - [`site_settings`] and [`fill_site`]: what the public site and shop take from the brand. The
//!   shop reads the `site` object of the instance settings through one door,
//!   `PublicApi::site_settings_read`, and the brand fills the keys that object leaves unset and
//!   hands the artwork of the tab icon over through one reserved key; the same decision
//!   (`site::view`) gives the `site.*` and `favicon` entries of `get`. A read that failed is
//!   reported as `SiteRead::degraded`, so that the shop does not keep it.
//! - [`Dispatcher::brand`]: the reader the dispatcher holds, created by `register_admin`.
//! - [`env_issuer`] and [`enrolment_error`]: what the enrolment of an authenticator or a passkey
//!   needs besides [`strict`] (the one reading of `OBOP_MFA_ISSUER`, and the error of an enrolment
//!   that could not read the name).
//!
//! No code outside this module reads the model `instance_brand`: `brand_scan_tests` in `obop-http`
//! lists the few files that name it or the reader, with the reason of each, and fails on any other.
//! There is no global: the reader belongs to the dispatcher.

mod apply;
mod effective;
mod enrol;
mod reader;
mod row;
mod site;

pub use apply::{
    Action, Applied, Changed, Failure, Refused, apply, change_line, changes_json, printable,
    refusals_json, show_key,
};
pub use enrol::{ISSUER_VAR, enrolment_error, env_issuer, issuer_warning, no_reader_error};
pub use reader::{DEFAULT_BOUND, Fresh, Reader, Sink, strict};
pub use row::{Fallback, SWITCH, Shown, fallbacks_json};
pub use site::{DEFAULT_TITLE, FAVICON_KEY, SiteRead, fill_site, site_settings};

use crate::Dispatcher;
pub use obop_brand::LANGS;
/// The limits `configure` enforces, for the texts that state them (the descriptions of the model
/// and their translations are held to these numbers by a test): the longest name, title, message
/// and slug in characters, and the largest artwork in bytes.
pub use obop_brand::policy::{MESSAGE_MAX, NAME_MAX, SLUG_MAX, TITLE_MAX};
pub use obop_brand::svg::MAX_BYTES as ARTWORK_MAX_BYTES;
use obop_storage::Backend;
use std::sync::Arc;

/// The model of the record.
pub const MODEL: &str = "instance_brand";
/// The id of its one row: no row means no brand.
pub const ROW_ID: &str = "instance";
/// The cross-process lock `apply` takes around the read, the merge and the write.
pub const LOCK_KEY: &str = "brand.write";
/// The `user` of every event of the log that this record writes.
pub const AUDIT_USER: &str = "(instance)";
/// The model and id of the instance settings, which hold the theme the brand defers to.
pub(crate) const SETTING_MODEL: &str = "obop_setting";
pub(crate) const SETTING_ID: &str = "instance";

/// What [`strict`] answers when the stored name cannot be shown: a fixed message that starts with
/// this text, so that a caller can tell it from a storage error (which is returned unchanged).
pub const UNREADABLE: &str = "the instance branding could not be read";

/// The field names a `configure` may supply, in the order they are checked and reported: `name`,
/// `slug`, `title_<lang>` and `message_<lang>` for each language of
/// [`obop_brand::LANGS`], `logo_svg` and the switch [`SWITCH`] (`hide_product_docs`, a boolean: the
/// others are text).
pub fn writable_fields() -> Vec<String> {
    let mut v = vec!["name".to_string(), "slug".to_string()];
    v.extend(LANGS.iter().map(|l| format!("title_{l}")));
    v.extend(LANGS.iter().map(|l| format!("message_{l}")));
    v.push("logo_svg".to_string());
    v.push(SWITCH.to_string());
    v
}

/// The text of the generated `get` of this model for the catalogue, the MCP tool text and the API
/// document: the generated text describes the `get` of an ordinary model ("read one record by
/// id"), and this one takes no id. `None` for any other command or model. The text says what the
/// record holds, not which pages show it.
pub(crate) fn describe(model: &str, command: &str) -> Option<&'static str> {
    if model != MODEL {
        return None;
    }
    match command {
        "get" => Some(
            "Read the instance branding. Takes no input. Returns the stored fields as stored (not the artwork; a stored text that failed the read check comes back in ASCII with each other character written as \\u{XXXX}), hide_product_docs (the switch: false unless a boolean true is stored), set (whether a brand exists), v (the version of the logo), effective (the values in force, each with the source that won) and fallbacks (the stored fields that failed the read check and fell back to a default, each with the reason). With no brand set the answer is the defaults and set false, which is not an error; a store that cannot be read is an error",
        ),
        _ => None,
    }
}

impl Dispatcher {
    /// The reader of the instance brand. Every composition that runs `register_admin` has one; a
    /// dispatcher that never did (some test rigs) gives an inert reader: the default brand, no
    /// store, and no cost.
    pub fn brand(&self) -> Arc<Reader> {
        self.brand
            .get()
            .cloned()
            .unwrap_or_else(|| Arc::new(Reader::inert()))
    }

    /// Create the reader against the store. Called once by `register_admin`; a second call keeps
    /// the first reader and returns it.
    pub fn install_brand(&self, backend: Arc<dyn Backend>) -> Arc<Reader> {
        self.brand
            .get_or_init(|| Arc::new(Reader::new(backend)))
            .clone()
    }
}

#[cfg(test)]
pub(crate) mod tests;
