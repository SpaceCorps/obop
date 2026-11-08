//! The authenticator label follows the instance brand and fails closed (docs/spec/branding.md,
//! slice S1e: sections 4 and 5, row `OBOP_MFA_ISSUER`, and section 10, item 7, with the "Fail
//! closed" and "Issuer" bullets of section 13). The label and `issuer=` of a new entry are written
//! into the person's own device and cannot be corrected afterwards, so `begin` reads the name from
//! the row alone, every time, before it stores anything, and a name it could not read refuses the
//! enrolment: it never writes the default after a read error.
//!
//! The store is a memory database behind the `Flaky` double, which refuses reads as a full pool
//! does (a transient fault, noted on the thread) and, for these tests, can fail the reads of one
//! model with a plain error text (a store that is up and cannot answer).

use super::*;
use crate::mfa::EVENT_TABLE;
use crate::mfa_tests::{
    PW, Rig, T0, code_at, enforcing, enrol, mfa_token, rig, rig_with, session, user_with,
};
use obop_commands::ADMIN_GROUP;
use obop_commands::brand::{self, Action, UNREADABLE};
use obop_storage::fault::Fault;
use obop_storage::testing::{Flaky, Mode, time_budget};
use obop_storage::{Fields, Op};

/// The URI of an enrolment with the random secret blanked, as `otpauth_uri` builds it.
fn uri(issuer: &str, account: &str) -> String {
    format!(
        "otpauth://totp/{issuer}:{account}?secret=SECRET&issuer={issuer}&algorithm=SHA1&digits=6&period=30"
    )
}

/// The answer of `begin` for `user`: the URI with the secret blanked, and the issuer field.
fn begun(r: &Rig, user: &str) -> Result<(String, String), MfaError> {
    let (_, p) = session(&r.a, user);
    let b = r.a.begin(&p)?;
    let secret = b["secret"].as_str().unwrap();
    Ok((
        b["uri"].as_str().unwrap().replace(secret, "SECRET"),
        b["issuer"].as_str().unwrap().to_string(),
    ))
}

/// `begin` for a session that was opened before, so that a store which refuses reads from now on
/// is not met by the sign-in.
fn begin_with(r: &Rig, p: &Principal) -> Result<J, MfaError> {
    r.a.begin(p)
}

fn set_name(b: &dyn obop_storage::Backend, name: &str) {
    brand::apply(
        b,
        None,
        "operator",
        &Action::Configure(vec![("name".into(), name.into())]),
        false,
    )
    .unwrap();
}

/// A row as it is, whatever it holds: written by a restore, an import or a direct edit.
fn put_row(b: &dyn obop_storage::Backend, pairs: &[(&str, J)]) {
    let fields: Fields = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    b.apply_checked(&[Op::Put {
        model: brand::MODEL.into(),
        id: brand::ROW_ID.into(),
        fields,
    }])
    .unwrap();
}

fn flaky_rig(config: AuthConfig) -> (Arc<Flaky>, Rig) {
    let (flaky, b) = Flaky::memory();
    let a = Auth::with_test_config(b.clone(), config);
    a.set_clock_for_test(T0);
    (flaky, Rig { a, b })
}

/// What `begin` left behind for `user`: whether a secret is pending, and the events of the kind
/// `enrol_begin` of the log.
fn left_behind(r: &Rig, user: &str) -> (bool, usize) {
    let u = r.a.user_record(user).unwrap();
    let events =
        r.b.list(EVENT_TABLE)
            .unwrap()
            .into_iter()
            .filter(|(_, f)| f.get("kind").and_then(J::as_str) == Some("enrol_begin"))
            .count();
    (u["totp_pending"].is_string(), events)
}

/// The precedence of the label and of `issuer=`: the variable if it is set, else the name of the
/// record, else `obop`. The variable is `AuthConfig.issuer`, read at start by `env_issuer`; the
/// tests give it as the config so that none of them changes the environment of the process.
#[test]
fn the_label_and_the_issuer_follow_the_variable_then_the_record_then_obop() {
    for (variable, record, want) in [
        // Neither: today's output.
        (None, None, "obop"),
        // The record only.
        (None, Some("MOHV"), "MOHV"),
        // The variable only, and the variable over a record.
        (Some("Acme"), None, "Acme"),
        (Some("Acme"), Some("MOHV"), "Acme"),
        // The variable that says `obop` beats a record that says MOHV: it is a value, not the
        // absence of one.
        (Some("obop"), Some("MOHV"), "obop"),
    ] {
        let r = rig_with(AuthConfig {
            issuer: variable.map(String::from),
            ..AuthConfig::insecure_dev()
        });
        user_with(&r.a, "ada", &["base.user"]);
        if let Some(name) = record {
            set_name(&*r.b, name);
        }
        let (got_uri, got_issuer) = begun(&r, "ada").unwrap();
        assert_eq!(got_uri, uri(want, "ada"), "{variable:?} {record:?}");
        assert_eq!(got_issuer, want, "{variable:?} {record:?}");
    }
}

/// A name with a space, an ampersand and non-ASCII letters is percent-encoded in the label and in
/// `issuer=`, once each.
#[test]
fn a_name_that_needs_escaping_is_escaped_in_both_places() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    set_name(&*r.b, "Exämple & Söns");
    let (got_uri, got_issuer) = begun(&r, "ada").unwrap();
    assert_eq!(
        got_issuer, "Exämple & Söns",
        "the field is the name as it is"
    );
    assert_eq!(
        got_uri,
        "otpauth://totp/Ex%C3%A4mple%20%26%20S%C3%B6ns:ada?secret=SECRET&issuer=Ex%C3%A4mple%20%26%20S%C3%B6ns&algorithm=SHA1&digits=6&period=30"
    );
    crate::totp::uri_check::assert_sound(&got_uri, "Exämple & Söns", "ada");
}

/// `AuthConfig` has the variable and nothing else: with it unset the config holds none, and the
/// enrolment of an instance with no record is the output it always was.
#[test]
fn with_the_variable_unset_the_config_holds_no_issuer_and_an_empty_store_says_obop() {
    assert!(
        std::env::var_os(brand::ISSUER_VAR).is_none(),
        "{} is set in the environment of this run, so `AuthConfig::from_env` does not give the \
         default and this test cannot check it. Unset the variable and run again.",
        brand::ISSUER_VAR
    );
    assert_eq!(AuthConfig::from_env().issuer, None);
    assert_eq!(AuthConfig::insecure_dev().issuer, None);
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    assert_eq!(
        begun(&r, "ada").unwrap(),
        (uri("obop", "ada"), "obop".into())
    );
}

/// With the variable set `begin` needs no read of the brand: a store that refuses it, and a row
/// that could not be shown, change nothing, and the model is never even asked for.
#[test]
fn with_the_variable_set_the_brand_is_not_read() {
    let (flaky, r) = flaky_rig(AuthConfig {
        issuer: Some("Acme".into()),
        ..AuthConfig::insecure_dev()
    });
    user_with(&r.a, "ada", &["base.user"]);
    put_row(&*r.b, &[("name", json!("a<b"))]);
    let (_, p) = session(&r.a, "ada");
    flaky.refuse_model(brand::MODEL, Mode::Reads);
    let b = begin_with(&r, &p).unwrap();
    assert_eq!(b["issuer"], json!("Acme"));
    assert_eq!(flaky.refused(), 0, "nothing was refused: nothing asked");
    flaky.allow_models();
    // Stronger than a refusal that was not met: the model was never named in a call after the
    // row was put. (`models_asked` is cumulative, so the first start of the double is a new one.)
    let (flaky, r) = flaky_rig(AuthConfig {
        issuer: Some("Acme".into()),
        ..AuthConfig::insecure_dev()
    });
    user_with(&r.a, "ada", &["base.user"]);
    begun(&r, "ada").unwrap();
    assert!(
        !flaky.models_asked().contains(brand::MODEL),
        "{:?}",
        flaky.models_asked()
    );
}

/// A transient fault of the store: `begin` answers with the store's own error, which the route
/// turns into 503 with `Retry-After`, and writes no label (no pending secret, no event).
#[test]
fn a_transient_fault_refuses_the_enrolment_with_the_stores_own_error_and_writes_nothing() {
    let (flaky, r) = flaky_rig(AuthConfig::insecure_dev());
    user_with(&r.a, "ada", &["base.user"]);
    set_name(&*r.b, "MOHV");
    let (_, p) = session(&r.a, "ada");
    flaky.refuse_model(brand::MODEL, Mode::Reads);
    let Err(MfaError::Unavailable(m)) = begin_with(&r, &p) else {
        panic!("the enrolment went on without a name")
    };
    assert!(Fault::is_transient(&m), "{m}");
    assert!(!m.starts_with(UNREADABLE), "{m}");
    assert_eq!(left_behind(&r, "ada"), (false, 0));
    // Nothing is kept: the store comes back and the enrolment is made with the name.
    flaky.allow_models();
    assert_eq!(begin_with(&r, &p).unwrap()["issuer"], json!("MOHV"));
    assert_eq!(left_behind(&r, "ada"), (true, 1));
}

/// A read error that is not a transient fault is not taken for one, and not for a missing server
/// key either: a fixed message that starts with the words the route matches, and nothing of the
/// store's text.
#[test]
fn another_read_error_refuses_the_enrolment_with_the_fixed_message_and_writes_nothing() {
    let (flaky, r) = flaky_rig(AuthConfig::insecure_dev());
    user_with(&r.a, "ada", &["base.user"]);
    let (_, p) = session(&r.a, "ada");
    flaky.fail_gets_of(brand::MODEL, "relation \"record\" does not exist");
    let Err(MfaError::Unavailable(m)) = begin_with(&r, &p) else {
        panic!("the enrolment went on without a name")
    };
    assert!(m.starts_with(UNREADABLE), "{m}");
    assert!(!Fault::is_transient(&m), "{m}");
    assert!(
        !m.contains("relation") && !m.contains("OBOP_SECRET_KEY"),
        "no text of the store and no word about the key: {m}"
    );
    assert_eq!(left_behind(&r, "ada"), (false, 0));
    flaky.allow_models();
    assert_eq!(begin_with(&r, &p).unwrap()["issuer"], json!("obop"));
}

/// A stored name that cannot be shown refuses the enrolment, with the same fixed message, never the
/// default and never the stored text; another field that fails does not.
#[test]
fn a_stored_name_that_cannot_be_shown_refuses_the_enrolment_and_writes_nothing() {
    for bad in [
        json!("a<b"),
        json!(""),
        json!("a\u{200b}b"),
        json!("a\u{202e}b"),
        json!("x".repeat(41)),
        json!(5),
    ] {
        let r = rig();
        user_with(&r.a, "ada", &["base.user"]);
        put_row(&*r.b, &[("name", bad.clone())]);
        let Err(MfaError::Unavailable(m)) = begun(&r, "ada") else {
            panic!("the enrolment went on with a name that cannot be shown: {bad}")
        };
        assert!(m.starts_with(UNREADABLE), "{bad}: {m}");
        assert!(
            !m.contains('<') && !m.contains('\u{200b}') && !m.contains('\u{202e}'),
            "{m}"
        );
        assert_eq!(left_behind(&r, "ada"), (false, 0), "{bad}");
    }
    // A row without a name at all.
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    put_row(&*r.b, &[("title_en", json!("Hello"))]);
    assert!(matches!(begun(&r, "ada"), Err(MfaError::Unavailable(m)) if m.starts_with(UNREADABLE)));
    // Another field that fails the read check is no reason to refuse.
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    put_row(
        &*r.b,
        &[
            ("name", json!("MOHV")),
            ("title_en", json!("a<b")),
            ("slug", json!("Foo/Bar")),
            ("logo_svg", json!("<script>")),
        ],
    );
    assert_eq!(begun(&r, "ada").unwrap().1, "MOHV");
}

/// The variable cannot be used to repair a stored name that cannot be shown *after the fact*, but
/// it does not need the repair either: with it set, a bad name is not even looked at (above).
/// Without it, the repair is the command line, and the next enrolment is made with the new name.
#[test]
fn once_the_name_is_repaired_the_next_enrolment_is_made() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    put_row(&*r.b, &[("name", json!("a<b"))]);
    assert!(begun(&r, "ada").is_err());
    set_name(&*r.b, "MOHV");
    assert_eq!(begun(&r, "ada").unwrap().1, "MOHV");
}

/// A row that exists and has a name that cannot be shown does not stop a second-factor sign-in: the
/// sign-in writes no name. (The passkey routes: `crates/obop/src/brand_enrol_tests.rs`.)
#[test]
fn a_bad_stored_name_still_lets_a_person_who_is_enrolled_sign_in_with_the_code() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (secret, _codes) = enrol(&r.a, "ada");
    put_row(&*r.b, &[("name", json!("a<b"))]);
    r.a.set_clock_for_test(T0 + 60);
    let t = mfa_token(&r.a, "ada");
    let out =
        r.a.login_mfa(&t, Some(&code_at(&secret, T0 + 60)), None)
            .unwrap();
    assert!(matches!(out, LoginOutcome::Session { .. }), "{out:?}");
}

/// The name is read from the row every time: an enrolment that begins right after a `configure`
/// made through another process (here: `apply` with no reader, as the command line does) is made
/// with the new name, and never with a name kept from the last one.
#[test]
fn an_enrolment_right_after_a_change_made_by_another_process_has_the_new_name() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    set_name(&*r.b, "Old");
    assert_eq!(begun(&r, "ada").unwrap().1, "Old");
    set_name(&*r.b, "New");
    assert_eq!(begun(&r, "ada").unwrap().1, "New");
    brand::apply(&*r.b, None, "operator", &Action::Clear, false).unwrap();
    assert_eq!(begun(&r, "ada").unwrap().1, "obop");
}

/// A theme read that fails does not refuse an enrolment: the name is read from the row alone.
#[test]
fn a_failing_theme_read_does_not_refuse_the_enrolment() {
    let (flaky, r) = flaky_rig(AuthConfig::insecure_dev());
    user_with(&r.a, "ada", &["base.user"]);
    set_name(&*r.b, "MOHV");
    let (_, p) = session(&r.a, "ada");
    flaky.refuse_model("obop_setting", Mode::Reads);
    assert_eq!(begin_with(&r, &p).unwrap()["issuer"], json!("MOHV"));
    assert_eq!(flaky.refused(), 0, "the setting was not even asked for");
}

/// Waits for `done` to be true, up to the time budget of a test, and says whether it was. A condition
/// with a bound, not a pause: it returns as soon as the condition holds.
fn wait_for(secs: u64, mut done: impl FnMut() -> bool) -> bool {
    let limit = std::time::Instant::now() + time_budget(secs);
    while !done() {
        if std::time::Instant::now() >= limit {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    true
}

/// The read takes no lock and comes before the one `begin` takes: with the lock `auth.mfa` held by
/// someone else (here: by this thread, and the store's lock is not re-entrant), an enrolment that
/// cannot read the name answers at once, where one that read it under the lock would be parked at it
/// for the sixty seconds the store gives a lock. The answer is awaited as a condition (the thread of
/// `begin` has finished), with a bound, not as a measured time.
#[test]
fn the_read_of_the_name_needs_no_lock() {
    let (flaky, r) = flaky_rig(AuthConfig::insecure_dev());
    user_with(&r.a, "ada", &["base.user"]);
    let (_, p) = session(&r.a, "ada");
    flaky.fail_gets_of(brand::MODEL, "a read error that is not transient");
    let held = r.b.lock("auth.mfa").unwrap();
    let (answered, answer) = std::thread::scope(|s| {
        let parked = s.spawn(|| begin_with(&r, &p));
        let answered = wait_for(10, || parked.is_finished());
        // Let it go either way, so that a begin that waits for the lock ends too.
        drop(held);
        (answered, parked.join().unwrap())
    });
    assert!(
        answered,
        "the enrolment waited for the lock before it read the name"
    );
    assert!(
        matches!(&answer, Err(MfaError::Unavailable(m)) if m.starts_with(UNREADABLE)),
        "{answer:?}"
    );
}

/// A name that can be read is read once, and before the lock: the count is of the reads of the row
/// (`Flaky::gets_of`), so a second read (under the lock, after it, or a repeat before it) shows as a
/// two. The lock is held by this thread while `begin` runs on another, so the read that the answer
/// reports was made while the lock was out of its reach.
#[test]
fn a_readable_name_is_read_once_and_before_the_lock() {
    let (flaky, r) = flaky_rig(AuthConfig::insecure_dev());
    user_with(&r.a, "ada", &["base.user"]);
    set_name(&*r.b, "MOHV");
    let (_, p) = session(&r.a, "ada");
    let base = flaky.gets_of(brand::MODEL);
    let held = r.b.lock("auth.mfa").unwrap();
    let (read_while_held, answer) = std::thread::scope(|s| {
        let parked = s.spawn(|| begin_with(&r, &p));
        // The first read is made with the lock held by someone else; a begin that takes the lock
        // first never makes it, and the wait ends at the bound.
        let seen = wait_for(10, || flaky.gets_of(brand::MODEL) > base);
        drop(held);
        (seen, parked.join().unwrap())
    });
    assert!(read_while_held, "the name was not read before the lock");
    assert_eq!(answer.unwrap()["issuer"], json!("MOHV"));
    assert_eq!(
        flaky.gets_of(brand::MODEL) - base,
        1,
        "the row was read more than once for one enrolment"
    );
}

/// With the variable set the lock and the read are as before: nothing of the brand is asked for, so
/// the count of the reads of the row does not move (the same count as above, from the other side).
#[test]
fn with_the_variable_set_the_row_is_read_no_times() {
    let (flaky, r) = flaky_rig(AuthConfig {
        issuer: Some("Acme".into()),
        ..AuthConfig::insecure_dev()
    });
    user_with(&r.a, "ada", &["base.user"]);
    set_name(&*r.b, "MOHV");
    let (_, p) = session(&r.a, "ada");
    let base = flaky.gets_of(brand::MODEL);
    assert_eq!(begin_with(&r, &p).unwrap()["issuer"], json!("Acme"));
    assert_eq!(flaky.gets_of(brand::MODEL), base);
}

/// The server key is asked for before the name is read: with no key ring and a stored name that
/// cannot be shown, the answer is the missing key, not the branding, so that an administrator is not
/// sent to repair a name only to meet the key next. (With a key ring the same row is refused as the
/// branding: the order is of the two errors, not a rule about the name.)
#[test]
fn a_missing_server_key_is_reported_before_a_name_that_cannot_be_read() {
    let r = rig_with(AuthConfig {
        keys: obop_keys::KeyRing::default(),
        ..AuthConfig::insecure_dev()
    });
    user_with(&r.a, "ada", &["base.user"]);
    put_row(&*r.b, &[("name", json!("a<b"))]);
    let Err(MfaError::Unavailable(m)) = begun(&r, "ada") else {
        panic!("the enrolment went on without a key")
    };
    assert!(
        !m.starts_with(UNREADABLE) && m.contains("OBOP_SECRET_KEY"),
        "the missing key comes first: {m}"
    );
    // The control: with a key the same row is the branding's error.
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    put_row(&*r.b, &[("name", json!("a<b"))]);
    assert!(matches!(begun(&r, "ada"), Err(MfaError::Unavailable(m)) if m.starts_with(UNREADABLE)));
}

/// An administrator under the policy who must still enrol (D53) hits the same wall: the limited
/// session may call `begin`, and it is refused like anyone's; nothing is stored. The repair is at the
/// terminal, which needs no session (docs/DEPLOY.md).
#[test]
fn an_administrator_who_must_enrol_is_refused_as_anyone_is() {
    let r = rig_with(enforcing());
    user_with(&r.a, "root", &[ADMIN_GROUP]);
    put_row(&*r.b, &[("name", json!("a<b"))]);
    let LoginOutcome::Session {
        token,
        mfa_enrolment_required,
        ..
    } = r.a.login_step("root", PW).unwrap()
    else {
        panic!("a session")
    };
    assert!(mfa_enrolment_required);
    let p = r.a.resolve(&token).unwrap();
    assert!(p.mfa_limited);
    assert!(matches!(
        r.a.begin(&p),
        Err(MfaError::Unavailable(m)) if m.starts_with(UNREADABLE)
    ));
    assert_eq!(left_behind(&r, "root"), (false, 0));
}

/// Names with the characters that matter for a URI, stored around the command (a restore, an import
/// or a direct edit: `configure` refuses these as punctuation, the read check of the display does
/// not): `%` that starts an escape, `?` and `#` that start a query and a fragment, `&` and `=` of the
/// query, a double quote. In the label and in `issuer=` each is written as one escape, the label has
/// the one colon between its two parts, and the query has exactly its five keys, whatever the name
/// says; percent-decoding either place gives the name back.
#[test]
fn a_stored_name_with_uri_characters_is_escaped_in_both_places() {
    for name in [
        "100%",
        "a%3Ab",
        "q?x=1",
        "h#frag",
        "say \"hi\"",
        "a&secret=EVIL",
        "&issuer=evil&",
        "a=b",
        "a;b",
        "[x]{y}|z^`",
        "it's",
        "a+b",
    ] {
        let r = rig();
        user_with(&r.a, "ada", &["base.user"]);
        put_row(&*r.b, &[("name", json!(name))]);
        let (got_uri, got_issuer) = begun(&r, "ada").unwrap();
        assert_eq!(got_issuer, name, "the field is the name as it is");
        crate::totp::uri_check::assert_sound(&got_uri, name, "ada");
    }
}

/// The variable is operator-controlled text that is used as it is, with no rule of the brand
/// record: it may hold a colon, a slash or a backslash, and the URI stays one URI all the same.
#[test]
fn a_variable_with_uri_characters_is_escaped_in_both_places() {
    for name in [
        "Acme: Corp",
        "a:b",
        "a/b",
        "a\\b",
        "a%b",
        "a?b#c",
        "Acme & Sons",
        "a=b&c",
    ] {
        let r = rig_with(AuthConfig {
            issuer: Some(name.into()),
            ..AuthConfig::insecure_dev()
        });
        user_with(&r.a, "ada", &["base.user"]);
        let (got_uri, got_issuer) = begun(&r, "ada").unwrap();
        assert_eq!(got_issuer, name);
        crate::totp::uri_check::assert_sound(&got_uri, name, "ada");
    }
}

/// A name that passes the read check of the display and that `configure` refuses (a link character,
/// or only white space) can only be in the row if it was written around the command. In a label the
/// colon is where an app splits the decoded text, so it would show another issuer than `issuer=`
/// says (`javascript:alert(1)` would read as the issuer `javascript`), and a name of spaces is a
/// blank issuer. The enrolment refuses them as it refuses a name that cannot be shown, with the
/// same fixed message, and writes nothing; with the variable set the row is not looked at.
#[test]
fn a_stored_name_that_configure_refuses_for_a_link_or_for_being_blank_refuses_the_enrolment() {
    for bad in [
        "a:b",
        "a/b",
        "a\\b",
        "a@b",
        "http://evil.test/x",
        "javascript:alert(1)",
        " ",
        "  ",
    ] {
        // It really is a name `configure` would not write.
        let r = rig();
        user_with(&r.a, "ada", &["base.user"]);
        assert!(
            brand::apply(
                &*r.b,
                None,
                "operator",
                &Action::Configure(vec![("name".into(), bad.into())]),
                false,
            )
            .is_err(),
            "{bad:?} is writable by configure, so the case proves nothing"
        );
        put_row(&*r.b, &[("name", json!(bad))]);
        let Err(MfaError::Unavailable(m)) = begun(&r, "ada") else {
            panic!("the enrolment went on with a name that cannot go into a label: {bad:?}")
        };
        assert!(m.starts_with(UNREADABLE), "{bad:?}: {m}");
        assert!(
            bad.trim().is_empty() || !m.contains(bad.trim()),
            "nothing of the stored text: {m}"
        );
        assert_eq!(left_behind(&r, "ada"), (false, 0), "{bad:?}");
        // The variable does not read the row.
        let r = rig_with(AuthConfig {
            issuer: Some("Acme".into()),
            ..AuthConfig::insecure_dev()
        });
        user_with(&r.a, "ada", &["base.user"]);
        put_row(&*r.b, &[("name", json!(bad))]);
        assert_eq!(begun(&r, "ada").unwrap().1, "Acme");
    }
}

/// A person whose second factor is already on is told so, whatever the name holds: that answer needs
/// no name, and it is the one `begin` gave before it read the name at all. The order is held in both
/// directions of the fault: a stored name that cannot be shown, and a store that cannot be read.
#[test]
fn a_person_whose_two_factor_is_on_is_told_so_before_the_name_is_read() {
    let (flaky, r) = flaky_rig(AuthConfig::insecure_dev());
    user_with(&r.a, "ada", &["base.user"]);
    // A session opened before the second factor is on: once it is, a sign-in asks for the code.
    let (_, p) = session(&r.a, "ada");
    enrol(&r.a, "ada");
    let on = |answer: Result<J, MfaError>, what: &str| match answer {
        Err(MfaError::Invalid(m)) => assert!(m.contains("already on"), "{what}: {m}"),
        other => panic!("{what}: not the answer of a person whose two-factor is on: {other:?}"),
    };
    put_row(&*r.b, &[("name", json!("a<b"))]);
    on(begin_with(&r, &p), "a name that cannot be shown");
    flaky.fail_gets_of(brand::MODEL, "a read error that is not transient");
    on(begin_with(&r, &p), "a read error");
    flaky.refuse_model(brand::MODEL, Mode::Reads);
    on(begin_with(&r, &p), "a transient fault");
    // A person who has not enrolled meets the wall, as before.
    flaky.allow_models();
    user_with(&r.a, "bob", &["base.user"]);
    assert!(matches!(
        begun(&r, "bob"),
        Err(MfaError::Unavailable(m)) if m.starts_with(UNREADABLE)
    ));
}

/// The look under the guard still counts: a second factor that another request turned on after
/// `begin` made its first look and before it holds the guard is refused, and nothing is stored. The
/// other request runs right after the read of the name, which is the step between the two looks
/// (`after_the_get_of`, a hook of the double: no thread, no pause, so it holds on a loaded machine
/// as on an idle one).
#[test]
fn a_second_factor_turned_on_between_the_first_look_and_the_guard_is_still_refused() {
    let (flaky, r) = flaky_rig(AuthConfig::insecure_dev());
    user_with(&r.a, "ada", &["base.user"]);
    let (_, p) = session(&r.a, "ada");
    flaky.after_the_get_of(
        brand::MODEL,
        0,
        Box::new(|inner| {
            let mut turned_on = Fields::new();
            turned_on.insert("mfa_enabled".into(), json!(true));
            assert!(inner.update("auth_user", "ada", &turned_on).unwrap());
        }),
    );
    let answer = begin_with(&r, &p);
    assert!(
        matches!(&answer, Err(MfaError::Invalid(m)) if m.contains("already on")),
        "{answer:?}"
    );
    assert!(
        !r.a.user_record("ada").unwrap()["totp_pending"].is_string(),
        "no secret was stored"
    );
}
