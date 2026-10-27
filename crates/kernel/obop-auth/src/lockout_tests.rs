//! The lockout counts wrong guesses and only wrong guesses (D74): a read of the account that the
//! store refused is not a wrong password, a guess that could not be counted is not answered as
//! one, and the log says "lockout" only for a count that was read.

use super::*;
use crate::admin_tests::{flaky_rig, request};
use crate::mfa_tests::{PW, Rig, enrol, mfa_token, user_with};
use obop_commands::{Authenticator, MfaApi};
use obop_storage::testing::Mode;

fn failures(r: &Rig, user: &str) -> u64 {
    r.b.counter_get(&Auth::failure_key(user)).unwrap()
}

fn lockouts(r: &Rig) -> usize {
    r.b.list(EVENT_TABLE)
        .unwrap()
        .iter()
        .filter(|(_, f)| f["kind"] == "lockout")
        .count()
}

#[test]
fn a_refused_read_of_the_account_is_not_a_wrong_password() {
    let (r, flaky) = flaky_rig();
    user_with(&r.a, "ada", &["base.user"]);
    // The right password, and the account cannot be read: the store's trouble. It was counted
    // as a wrong password, and five of these locked the person out of an account they had typed
    // correctly.
    flaky.refuse_model("auth_user", Mode::Reads);
    let (done, _) = request(|| r.a.login_step("ada", PW));
    flaky.allow_models();
    assert!(
        matches!(&done, Err(m) if m.starts_with("storage_busy")),
        "{done:?}"
    );
    assert_eq!(failures(&r, "ada"), 0);
    assert!(r.a.login_step("ada", PW).is_ok());
}

#[test]
fn the_right_password_is_never_counted_whichever_call_the_store_refuses() {
    // The password change, the turn-off of two-factor and the passkey re-authentication read
    // the account a second time to check the password: one refused call at each position.
    let mut counted = 0;
    for skip in 0..14 {
        let (r, flaky) = flaky_rig();
        user_with(&r.a, "ada", &["base.user"]);
        let (token, p) = crate::mfa_tests::session(&r.a, "ada");
        flaky.refuse_call_after(skip);
        let (done, _) = request(|| {
            r.a.change_password(&p, &token, PW, "another horse battery staple", None, None)
        });
        flaky.allow_models();
        let n = failures(&r, "ada");
        counted += n;
        assert_eq!(
            n, 0,
            "skip {skip}: the right password was counted ({done:?})"
        );
    }
    assert_eq!(counted, 0);
}

#[test]
fn a_wrong_guess_that_could_not_be_counted_is_the_stores_trouble_and_not_a_plain_no() {
    let (r, flaky) = flaky_rig();
    user_with(&r.a, "ada", &["base.user"]);
    // The count is read and the account too, and then the counter is refused for every try: the
    // answer was "invalid username or password" with the guess uncounted, which is what a
    // guesser wants while the pool is full.
    flaky.refuse_calls_after(2, 6);
    let (done, _) = request(|| r.a.login_step("ada", "not the password at all"));
    flaky.allow_models();
    assert!(
        matches!(&done, Err(m) if m.starts_with("storage_busy")),
        "{done:?}"
    );
    assert_eq!(failures(&r, "ada"), 0);
    // One refusal is waited out: counted, and the answer is the usual one.
    flaky.refuse_call_after(2);
    let done = r.a.login_step("ada", "not the password at all");
    flaky.allow_models();
    assert_eq!(done, Err("invalid username or password".to_string()));
    assert_eq!(failures(&r, "ada"), 1);
}

#[test]
fn the_log_says_lockout_only_for_a_count_that_was_read() {
    let (r, flaky) = flaky_rig();
    user_with(&r.a, "ada", &["base.user"]);
    let _ = enrol(&r.a, "ada");
    let pending = mfa_token(&r.a, "ada");
    // A wrong code counts one failure; the count is then unreadable. Read as "locked" for the
    // sign-in (fail closed), it wrote "lockout: too many failed attempts" for an account with a
    // single failure.
    flaky.refuse_reads_after_the_next_write();
    let done = r.a.login_mfa(&pending, Some("000000"), None);
    flaky.allow_models();
    assert!(done.is_err());
    assert_eq!(failures(&r, "ada"), 1);
    assert_eq!(lockouts(&r), 0, "one failure is no lockout");
    // Five failures are one, once the count can be read.
    for _ in 0..4 {
        let t = mfa_token(&r.a, "ada");
        let _ = r.a.login_mfa(&t, Some("000000"), None);
    }
    assert!(lockouts(&r) >= 1);
}
