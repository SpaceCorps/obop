//! A sign-in over a store that refuses one call, at every position of the request in turn (N22):
//! a refused read of the account is an error of the sign-in, never "not disabled", "not
//! Microsoft-only", "no second factor" or "no password change owed". Each sweep fails when one of
//! those helpers turns a refused read into `false` again.

use super::*;
use crate::mfa_tests::{PW, T0, enrol, user_with};
use obop_commands::{Authenticator, LoginOutcome};
use obop_storage::testing::Flaky;

/// More calls than one password step makes, so the last runs of a sweep see no refusal at all.
const CALLS: i64 = 40;

fn rig() -> (Auth, Arc<Flaky>, Arc<dyn Backend>) {
    let (flaky, b) = Flaky::memory();
    let a = Auth::with_test_config(b.clone(), AuthConfig::insecure_dev());
    a.set_clock_for_test(T0);
    (a, flaky, b)
}

fn set_flag(b: &Arc<dyn Backend>, user: &str, key: &str) {
    let mut p = Fields::new();
    p.insert(key.into(), J::Bool(true));
    b.update("auth_user", user, &p).unwrap();
}

/// Run the password step of `user` once per position of the refusal, each on a fresh rig made by
/// `setup`, and hand every result to `check`. Returns how many runs met a refusal.
fn sweep(
    setup: impl Fn(&Auth, &Arc<dyn Backend>),
    check: impl Fn(i64, &Result<LoginOutcome, String>),
) -> i64 {
    let mut refused_runs = 0;
    for skip in 0..CALLS {
        let (a, flaky, b) = rig();
        setup(&a, &b);
        flaky.refuse_call_after(skip);
        let before = flaky.refused();
        let r = a.login_step("ada", PW);
        if flaky.refused() > before {
            refused_runs += 1;
        }
        check(skip, &r);
    }
    refused_runs
}

#[test]
fn a_refused_read_never_lets_a_microsoft_only_account_in_with_its_password() {
    let n = sweep(
        |a, b| {
            user_with(a, "ada", &["base.user"]);
            set_flag(b, "ada", "sso_only");
        },
        |skip, r| assert!(r.is_err(), "refusal after {skip} calls: {r:?}"),
    );
    assert!(n > 3, "the sweep met a refusal in {n} runs only");
}

#[test]
fn a_refused_read_never_lets_a_disabled_account_in() {
    let n = sweep(
        |a, b| {
            user_with(a, "ada", &["base.user"]);
            set_flag(b, "ada", "disabled");
        },
        |skip, r| assert!(r.is_err(), "refusal after {skip} calls: {r:?}"),
    );
    assert!(n > 3, "the sweep met a refusal in {n} runs only");
}

#[test]
fn a_refused_read_never_skips_the_second_factor() {
    let n = sweep(
        |a, _| {
            user_with(a, "ada", &["base.user"]);
            enrol(a, "ada");
        },
        |skip, r| {
            assert!(
                !matches!(r, Ok(LoginOutcome::Session { .. })),
                "refusal after {skip} calls gave a session without the second factor: {r:?}"
            )
        },
    );
    assert!(n > 3, "the sweep met a refusal in {n} runs only");
}

#[test]
fn a_refused_read_never_frees_a_session_that_owes_a_password_change() {
    let n = sweep(
        |a, b| {
            user_with(a, "ada", &["base.user"]);
            set_flag(b, "ada", "must_change_password");
        },
        |skip, r| {
            if let Ok(LoginOutcome::Session {
                password_change_required,
                ..
            }) = r
            {
                assert!(
                    password_change_required,
                    "refusal after {skip} calls gave a session free of the owed password change"
                );
            }
        },
    );
    assert!(n > 3, "the sweep met a refusal in {n} runs only");
}

/// A refused call of the store while an agent token is made is answered with the store's own text
/// (the route makes it the 503 of D74), never as a bad request: before, the read of the Microsoft
/// binding came back as `Unavailable("storage_...")` and the re-read of the account as "the
/// account changed", both answered 400.
#[test]
fn a_refused_call_while_an_agent_token_is_made_is_a_store_fault_and_never_a_bad_request() {
    let mut refused_runs = 0;
    for skip in 0..CALLS {
        let (a, flaky, _) = rig();
        user_with(&a, "ada", &["base.user"]);
        let p = Principal::user("ada", &["base.user"]);
        flaky.refuse_call_after(skip);
        let before = flaky.refused();
        let r = a.issue_agent_token(&p, &["base.user".to_string()], "bot");
        if flaky.refused() > before {
            refused_runs += 1;
            if let Err(e) = &r {
                assert!(
                    obop_storage::fault::Fault::is_transient(e),
                    "refusal after {skip} calls: {e}"
                );
            }
        }
    }
    assert!(
        refused_runs > 1,
        "the sweep met a refusal in {refused_runs} runs only"
    );
}
