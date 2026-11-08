//! The two small rules the enrolments share (docs/spec/branding.md, sections 4 and 5, and section 13,
//! "Fail closed" and "Issuer"): the reading of `OBOP_MFA_ISSUER`, and the error an enrolment answers
//! when it could not read the name. `Auth::begin` and the passkey route use them; the rest of what
//! they are for is tested where they are used (`obop-auth`, and `crates/obop` over HTTP).

use super::super::enrol::{issuer_from, store_read_line};
use crate::MfaError;
use crate::brand::{
    ISSUER_VAR, UNREADABLE, enrolment_error, env_issuer, issuer_warning, no_reader_error,
};
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
///
/// This restates the function, and says so: it holds the rule, not the wiring. That `AuthConfig::from_env`
/// and the `effective_json` of `get` both call `env_issuer` (so that the two cannot differ) needs a
/// process whose environment holds the variable, and the workspace forbids `unsafe`, which
/// `std::env::set_var` is. The wiring is therefore held by the two tests that start the real
/// program with the variable set: `crates/obop/tests/brand_enrol_process.rs` (the server's
/// `AuthConfig`, mutation: `from_env` giving `None`) and `crates/obop/tests/brand_cli.rs`,
/// `status_and_get_name_the_authenticator_issuer_and_the_source_that_won` (`effective_json`,
/// mutation: it passing `None`). Both are in the default `cargo test -p obop` and in CI.
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

/// A dispatcher with no reader is refused with the message of the unreadable name, which the route
/// answers as `brand_unavailable`: it starts with the words the route matches, is not a transient
/// fault and is kept as it is by `enrolment_error`.
#[test]
fn a_server_with_no_reader_gets_the_message_of_the_unreadable_name() {
    let m = unavailable(no_reader_error());
    assert!(m.starts_with(UNREADABLE), "{m}");
    assert!(!Fault::is_transient(&m), "{m}");
    assert_eq!(unavailable(enrolment_error(m.clone())), m);
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
        // The words of the fixed message in the middle of a text, or at its end, are not its start:
        // an unrelated error that merely contains them must not be passed through as the message.
        "relation x: the instance branding could not be read",
        "x the instance branding could not be read: with more after it",
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

/// The text the answer leaves out is written to the log of the server, with the cause named: an
/// operator who sees `brand_unavailable` finds out there whether the store failed or the stored name
/// was bad. One line, whatever the store said, and bounded.
#[test]
fn the_line_of_the_log_names_the_cause_and_is_one_bounded_line() {
    let line = store_read_line("relation \"record\" does not exist");
    assert!(line.contains("cause: store read"), "{line}");
    assert!(line.contains("obop admin brand status"), "{line}");
    assert!(
        line.ends_with("relation \"record\" does not exist"),
        "{line}"
    );
    // A line break, a carriage return or an escape in the store's text is blanked, so it cannot
    // start a line of its own or move a cursor.
    let forged = store_read_line("first\nobop: a forged line\r\u{1b}[31mred");
    assert_eq!(forged.lines().count(), 1, "{forged:?}");
    assert!(
        !forged.contains('\u{1b}') && !forged.contains('\r') && !forged.contains('\n'),
        "{forged:?}"
    );
    // And it is cut: a store that answers with a page of text fills no more than a line of the log.
    let long = store_read_line(&"x".repeat(5_000));
    assert!(long.len() < 600, "{}", long.len());
    assert!(
        long.ends_with(&"x".repeat(300)),
        "the text is cut at 300 characters"
    );
    assert!(!long.ends_with(&"x".repeat(301)));
}

/// A value of `OBOP_MFA_ISSUER` with a character a stored name may not hold is warned about, and
/// nothing else is: the value is still used (`env_issuer` and `issuer_from` above keep it as it is).
/// A plain name, one with spaces, an ampersand or non-ASCII letters has no warning.
#[test]
fn a_value_with_a_link_character_is_warned_about_and_a_plain_one_is_not() {
    for plain in [
        "MOHV",
        "obop",
        "Acme Corp",
        "Exämple & Söns",
        "a-b_c.d",
        "100%",
    ] {
        assert_eq!(issuer_warning(plain), None, "{plain:?}");
    }
    for linked in ["Acme: Corp", "a:b", "a/b", "a@b", "http://evil.test/x"] {
        let w = issuer_warning(linked).unwrap_or_else(|| panic!("{linked:?} was not warned about"));
        assert!(w.contains(ISSUER_VAR), "{w}");
        assert!(
            !w.contains(linked),
            "the warning does not repeat the value: {w}"
        );
    }
    // The backslash, spelled so that this file has no escape in a literal to misread.
    let backslash = format!("a{}b", char::from(92u8));
    assert!(issuer_warning(&backslash).is_some(), "{backslash:?}");
    // And the value is kept as it is.
    assert_eq!(
        issuer_from(Some("Acme: Corp".into())).as_deref(),
        Some("Acme: Corp")
    );
}
