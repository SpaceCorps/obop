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
use std::io::Write;

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

/// The warning for an `OBOP_MFA_ISSUER` that holds a character a stored name may not (a colon, a
/// slash, a backslash or an at sign), or `None`. The value is operator-controlled text and is used
/// as it is, as it always was (it is percent-escaped in the label and in `issuer=`, so the address
/// stays one address), so this is a warning and not a refusal: but an app splits the decoded label at
/// its colon, so it may show another issuer than `issuer=` says, which is why `configure` refuses
/// the same characters in a name. `AuthConfig::from_env` writes it once for the process (behind a
/// `Once`): the first time the configuration is read, which for `obop serve` is at start, however
/// many places in the process build their authentication.
pub fn issuer_warning(issuer: &str) -> Option<&'static str> {
    unfit_for_a_label(issuer).then_some(
        "OBOP_MFA_ISSUER holds a colon, a slash, a backslash or an at sign. It is used as it is, but an authenticator app splits the label at its colon and may show another issuer than the one in issuer=; a brand name may not hold these characters for that reason.",
    )
}

/// What an enrolment answers when [`strict`](super::strict) failed, as the one error type of the
/// enrolment routes. A transient fault of the store (its text starts with a code word of D74) and
/// the fixed message about a stored name that cannot be shown are kept as they are, so that the
/// transient one reaches the existing 503 with `Retry-After` and the other the 503
/// `brand_unavailable` (docs/spec/branding.md, section 4). Any other store error becomes a fixed
/// message that starts with [`UNREADABLE`], so that it is never taken for a missing server key,
/// and no text of the store reaches the answer. The text it leaves out of the answer is not lost to
/// the operator: it is written to the log of the server at each refusal, as a line with the cause
/// named ([`store_read_line`]); the route adds a second line when it answers, with the fixed
/// message and none of the store's text. The other two cases carry their cause in the message
/// itself, which the route logs when it answers.
pub fn enrolment_error(e: String) -> MfaError {
    if Fault::is_transient(&e) || e.starts_with(UNREADABLE) {
        MfaError::Unavailable(e)
    } else {
        // Not `eprintln!`, which panics when stderr cannot be written: a refusal does not depend on it.
        let _ = writeln!(std::io::stderr(), "{}", store_read_line(&e));
        MfaError::Unavailable(format!("{UNREADABLE}: the store could not be read"))
    }
}

/// The line of the log for a store that could not be read for an enrolment: the cause (`store read`,
/// as against `stored name`, which the answer states for itself), the command that shows the state
/// of the branding, and the store's own text with its control characters blanked and cut at 300
/// characters, so that a text with a line break in it cannot forge a line of the log.
pub(super) fn store_read_line(text: &str) -> String {
    let text: String = text
        .chars()
        .map(|c| if blanked_in_a_log(c) { ' ' } else { c })
        .take(300)
        .collect();
    format!(
        "obop: an enrolment was refused because the instance branding could not be read (cause: store read; obop admin brand status shows the state of the branding): {text}"
    )
}

/// Whether a character is blanked in a line of the log: a control character, and the line and
/// paragraph separators U+2028 and U+2029, which are not control characters (their categories are
/// `Zl` and `Zp`) and still start a line in a viewer that follows the Unicode rule. The same rule
/// as `log_safe` in `obop-http`.
fn blanked_in_a_log(c: char) -> bool {
    c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')
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
/// (`:` `/` `\` `@`, the four ASCII characters of those `configure` refuses as `link_character`) or
/// it is only white space. The check is on the ASCII characters alone, on purpose: `configure` folds
/// the text first (NFKC) and so refuses their fullwidth and small-form variants too, but an
/// authenticator app splits the decoded label at the ASCII colon and at nothing else, and the label
/// is percent-encoded, so a row written around the command that holds `U+FF1A` is enrolled with and
/// cannot change the issuer an app shows.
/// The read check of the display is the safety subset and accepts these, so a row written around the
/// command (a restore, an import, a direct edit) can hold one. In a label the colon is the one
/// character an app splits the decoded text at, so a name with a colon in it would show another
/// issuer than `issuer=` says; and a name of spaces is a blank issuer. The enrolment fails closed on
/// them as it does on a name that cannot be shown, so that a row that could make an app show another
/// issuer is not written into a person's device. Only [`strict`](super::strict) and `get` ask this.
pub(super) fn unfit_for_a_label(name: &str) -> bool {
    name.trim().is_empty() || name.chars().any(|c| matches!(c, ':' | '/' | '\\' | '@'))
}
