//! A refused read of the two-factor policy is the store's trouble, never "not required" (UB3):
//! the sign-in, the status of the enrolment and the passkey list answer the fault instead of
//! a policy relaxed for one request.

use super::*;
use crate::admin_tests::{flaky_rig_with, request};
use crate::mfa_tests::{PW, enforcing, user_with};
use obop_commands::{Authenticator, MfaApi};
use obop_storage::testing::Mode;

/// Everybody must enrol: a stored setting, so the policy is read on every sign-in.
fn require_all(r: &crate::mfa_tests::Rig) {
    let mut f = Fields::new();
    f.insert(
        "value".into(),
        J::from(json!({"security": {"require_mfa_for_all": true}}).to_string()),
    );
    r.b.insert("obop_setting", "instance", &f).unwrap();
}

#[test]
fn a_sign_in_whose_policy_read_is_refused_gets_no_session_that_skips_enrolment() {
    let (r, flaky) = flaky_rig_with(enforcing());
    require_all(&r);
    user_with(&r.a, "bob", &["base.user"]);
    // The policy holds: the session is limited to enrolment.
    let Ok(LoginOutcome::Session {
        mfa_enrolment_required,
        ..
    }) = r.a.login_step("bob", PW)
    else {
        panic!("a session")
    };
    assert!(mfa_enrolment_required);
    // The policy cannot be read: it was read as "not required" and the person had a full
    // session without the second factor the administrator demands.
    flaky.refuse_model("obop_setting", Mode::Reads);
    let (done, _) = request(|| r.a.login_step("bob", PW));
    flaky.allow_models();
    assert!(
        matches!(&done, Err(m) if obop_storage::fault::Fault::is_transient(m)),
        "the store's trouble, never a session: {done:?}"
    );
    // Once the store answers, the sign-in works again and is limited as before.
    assert!(matches!(
        r.a.login_step("bob", PW),
        Ok(LoginOutcome::Session {
            mfa_enrolment_required: true,
            ..
        })
    ));
}

#[test]
fn the_status_of_the_enrolment_does_not_report_a_policy_it_could_not_read() {
    let (r, flaky) = flaky_rig_with(enforcing());
    require_all(&r);
    user_with(&r.a, "bob", &["base.user"]);
    let (_, p) = crate::mfa_tests::session(&r.a, "bob");
    assert_eq!(r.a.status(&p).unwrap()["require_all"], json!(true));
    flaky.refuse_model("obop_setting", Mode::Reads);
    let (done, _) = request(|| r.a.status(&p));
    flaky.allow_models();
    assert!(
        matches!(&done, Err(MfaError::Unavailable(m)) if obop_storage::fault::Fault::is_transient(m)),
        "a 503, never require_all false: {done:?}"
    );
}

#[test]
fn the_passkey_list_does_not_report_a_policy_it_could_not_read() {
    use obop_commands::PasskeyApi;
    let (r, flaky) = flaky_rig_with(enforcing());
    require_all(&r);
    user_with(&r.a, "bob", &["base.user"]);
    let (_, p) = crate::mfa_tests::session(&r.a, "bob");
    let ctx = crate::passkey_tests::ctx();
    assert_eq!(
        PasskeyApi::list(&r.a, &p, &ctx).unwrap()["policy_requires_second_factor"],
        json!(true)
    );
    flaky.refuse_model("obop_setting", Mode::Reads);
    let (done, _) = request(|| PasskeyApi::list(&r.a, &p, &ctx));
    flaky.allow_models();
    assert!(
        matches!(&done, Err(MfaError::Unavailable(m)) if obop_storage::fault::Fault::is_transient(m)),
        "{done:?}"
    );
}
