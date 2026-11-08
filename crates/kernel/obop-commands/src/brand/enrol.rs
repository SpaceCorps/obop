//! What an enrolment writes into a person's own device: the label and issuer of an authenticator
//! entry and the relying party name of a passkey (docs/spec/branding.md, sections 4 and 5, row
//! `OBOP_MFA_ISSUER`, and section 10, item 7). Both are written once and cannot be corrected
//! afterwards, so an enrolment reads the name with [`strict`](super::strict) and **fails closed**: a
//! name that could not be read is never replaced by the default.
//!
//! The precedence of the authenticator label is one rule for every caller: the variable
//! `OBOP_MFA_ISSUER` if it is set, else the `name` of the record (read with `strict()`), else
//! `obop`. [`env_issuer`] is the one place the variable is read: `AuthConfig::from_env` calls it at
//! start and `get` calls it for the entry `authenticator_issuer` of `effective`, so the two cannot
//! differ.

use super::UNREADABLE;
use crate::MfaError;
use obop_storage::fault::Fault;

/// The environment variable that names the issuer of an authenticator entry.
pub const ISSUER_VAR: &str = "OBOP_MFA_ISSUER";

/// The value of `OBOP_MFA_ISSUER`, as it is set: a variable that is unset or holds only white space
/// is no value. The text is used as it is, not trimmed.
pub fn env_issuer() -> Option<String> {
    issuer_from(std::env::var(ISSUER_VAR).ok())
}

/// The rule of [`env_issuer`] on a value that was read: a missing value and one that is only white
/// space are no value, and any other is kept as it is. A function of its own, so a test can hold the
/// rule without changing the environment of the process.
pub(super) fn issuer_from(value: Option<String>) -> Option<String> {
    value.filter(|s| !s.trim().is_empty())
}

/// What an enrolment answers when [`strict`](super::strict) failed, as the one error type of the
/// enrolment routes. A transient fault of the store (its text starts with a code word of D74) and
/// the fixed message about a stored name that cannot be shown are kept as they are, so that the
/// transient one reaches the existing 503 with `Retry-After` and the other the 503
/// `brand_unavailable` (docs/spec/branding.md, section 4). Any other store error becomes a fixed
/// message that starts with [`UNREADABLE`], so that it is never taken for a missing server key,
/// and no text of the store reaches the answer.
pub fn enrolment_error(e: String) -> MfaError {
    if Fault::is_transient(&e) || e.starts_with(UNREADABLE) {
        MfaError::Unavailable(e)
    } else {
        MfaError::Unavailable(format!("{UNREADABLE}: the store could not be read"))
    }
}

/// What an enrolment answers when the dispatcher it runs under has no reader of the brand (one that
/// never ran `register_admin`; the shipped composition always does). Such a reader answers the
/// default brand, which is the answer of a reader with nothing to read and not of a row that was
/// read, and a default is the one thing an enrolment never writes into a person's device. The same
/// fixed message, so the route answers it as it answers any name that cannot be read.
pub fn no_reader_error() -> MfaError {
    MfaError::Unavailable(format!("{UNREADABLE}: this server has no branding reader"))
}

/// Whether a stored name that passed the read check still cannot go into the label of an
/// authenticator entry or the name of a passkey: it holds a character of a link or an address
/// (`:` `/` `\` `@`, which `configure` refuses as `link_character`) or it is only white space.
/// The read check of the display is the safety subset and accepts these, so a row written around the
/// command (a restore, an import, a direct edit) can hold one. In a label the colon is the one
/// character an app splits the decoded text at, so a name with a colon in it would show another
/// issuer than `issuer=` says; and a name of spaces is a blank issuer. The enrolment fails closed on
/// them as it does on a name that cannot be shown, so that a row `configure` would not write is not
/// written into a person's device either. Only [`strict`](super::strict) and `get` ask this.
pub(super) fn unfit_for_a_label(name: &str) -> bool {
    name.trim().is_empty() || name.chars().any(|c| matches!(c, ':' | '/' | '\\' | '@'))
}
