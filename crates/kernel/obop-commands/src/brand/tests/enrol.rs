//! The two small rules the enrolments share (docs/spec/branding.md, sections 4 and 5, and section 13,
//! "Fail closed" and "Issuer"): the reading of `OBOP_MFA_ISSUER`, and the error an enrolment answers
//! when it could not read the name. `Auth::begin` and the passkey route use them; the rest of what
//! they are for is tested where they are used (`obop-auth`, and `crates/obop` over HTTP).

use super::super::enrol::issuer_from;
use crate::MfaError;
use crate::brand::{ISSUER_VAR, UNREADABLE, enrolment_error, env_issuer};
use obop_storage::fault::Fault;

#[test]
fn the_variable_is_the_one_the_authenticator_documents() {
    assert_eq!(ISSUER_VAR, "OBOP_MFA_ISSUER");
}

/// A missing value and one that is only white space are no value; any other is kept as it is, not
/// trimmed: the rule `AuthConfig::from_env` had before the record, so an instance that set the
/// variable keeps its output byte for byte.
#[test]
fn a_value_is_kept_as_it_is_and_nothing_or_white_space_is_no_value() {
    assert_eq!(issuer_from(None), None);
    for blank in ["", " ", "   ", "\t", "\n", " \t\n "] {
        assert_eq!(issuer_from(Some(blank.to_string())), None, "{blank:?}");
    }
    for kept in ["MOHV", "obop", " MOHV ", "Acme Corp", "Exämple & Söns"] {
        assert_eq!(
            issuer_from(Some(kept.to_string())),
            Some(kept.to_string()),
            "{kept:?}"
        );
    }
}

/// `env_issuer` is that rule on the environment of the process, and nothing else.
#[test]
fn env_issuer_reads_the_variable_of_the_process() {
    assert_eq!(
        env_issuer(),
        issuer_from(std::env::var(ISSUER_VAR).ok()),
        "the variable is read once, by this function, for the config and for `get`"
    );
}

fn unavailable(e: MfaError) -> String {
    match e {
        MfaError::Unavailable(m) => m,
        other => panic!("not an Unavailable: {other:?}"),
    }
}

/// A transient fault of the store (its text starts with a code word of D74) is kept as it is, so it
/// reaches the arm of the route that answers 503 with `Retry-After`; so is the fixed message about a
/// name that cannot be shown, so it reaches the arm of `brand_unavailable`.
#[test]
fn a_transient_fault_and_the_unreadable_name_are_kept_as_they_are() {
    for text in [
        "storage_busy: all database connections are in use, try again",
        "storage_unavailable: the connection broke",
        "storage_timeout: waited 5s",
    ] {
        assert!(Fault::is_transient(text), "{text}");
        assert_eq!(unavailable(enrolment_error(text.to_string())), text);
    }
    let unreadable = format!("{UNREADABLE}: the stored name cannot be shown");
    assert_eq!(unavailable(enrolment_error(unreadable.clone())), unreadable);
}

/// Any other store error becomes the fixed message, which starts with the words the route matches,
/// is not a transient fault, and holds no text of the store.
#[test]
fn any_other_store_error_becomes_the_fixed_message_and_nothing_of_the_store_is_repeated() {
    for text in [
        "a read error that is not transient",
        "relation \"record\" does not exist",
        "could not decode the row of instance_brand/instance",
        "",
        "OBOP_SECRET_KEY is missing",
    ] {
        let m = unavailable(enrolment_error(text.to_string()));
        assert!(m.starts_with(UNREADABLE), "{text}: {m}");
        assert!(!Fault::is_transient(&m), "{m}");
        assert!(
            text.is_empty() || !m.contains(text),
            "the store's text is not repeated: {m}"
        );
        assert!(
            !m.contains("OBOP_SECRET_KEY"),
            "never taken for a missing key: {m}"
        );
    }
    // The same message whatever the store said: it is fixed.
    let a = unavailable(enrolment_error("one".into()));
    let b = unavailable(enrolment_error("two".into()));
    assert_eq!(a, b);
}
