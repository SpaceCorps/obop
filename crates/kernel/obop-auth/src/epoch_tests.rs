//! A change that ends sessions must also end the session of a sign-in that began before it (D74,
//! "Left open after the sixth round" closed): every test here runs the change in the gap between
//! the read of the account that starts the sign-in and the write of its session, at the exact
//! place, with a hook in the test backend and no thread or sleep. Each one fails when the credential
//! epoch (`crate::Epoch`) is not compared.

use super::*;
use crate::mfa_tests::{PW, T0, enforcing, enrol, mfa_token, user_with};
use crate::webauthn::soft::SoftAuthenticator;
use obop_commands::{ADMIN_GROUP, AccountsApi, Authenticator, LoginOutcome, MfaApi, PasskeyApi};
use obop_commands::{Reauth, WebauthnCtx};
use obop_storage::testing::Flaky;
use std::sync::Arc;

const NEW_PW: &str = "another horse battery staple";
const ORIGIN: &str = "https://obop.test";

/// An `Auth` that a hook can call: shared, over a memory database behind the request guard.
fn shared(config: AuthConfig) -> (Arc<Auth>, Arc<Flaky>, Arc<dyn Backend>) {
    let (flaky, b) = Flaky::memory();
    let a = Arc::new(Auth::with_test_config(b.clone(), config));
    a.set_clock_for_test(T0);
    (a, flaky, b)
}

fn session_of(a: &Auth, user: &str) -> (String, Principal) {
    let LoginOutcome::Session { token, .. } = a.login_step(user, PW).unwrap() else {
        panic!("expected a session")
    };
    let p = a.resolve(&token).unwrap();
    (token, p)
}

fn known() -> Vec<String> {
    ["base.user", "sales.user", ADMIN_GROUP]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn live_sessions(b: &Arc<dyn Backend>, user: &str) -> usize {
    b.list("auth_token")
        .unwrap()
        .iter()
        .filter(|(_, t)| t["owner"] == user && t["kind"] == "user" && t["revoked"] != true)
        .count()
}

fn logged(b: &Arc<dyn Backend>, kind: &str) -> Vec<String> {
    b.list(EVENT_TABLE)
        .unwrap()
        .into_iter()
        .filter(|(_, f)| f["kind"] == kind)
        .map(|(_, f)| f["detail"].as_str().unwrap().to_string())
        .collect()
}

/// The reads of the account that one password sign-in makes before and after its session is
/// written: the password check, the disabled check, the second-factor check, the groups, the
/// owed password change, and the look at the account once the session is written. A change that
/// lands after any of them (`0..READS`) must not leave a live session behind.
const READS: i64 = 6;

#[test]
fn a_password_change_ends_the_session_of_a_sign_in_whose_password_check_began_before_it() {
    for after in 0..READS {
        let (a, flaky, b) = shared(AuthConfig::insecure_dev());
        user_with(&a, "ada", &["base.user"]);
        let (mine, p) = session_of(&a, "ada");
        // The attacker's sign-in is overtaken by the person's password change, which lists the
        // sessions before the attacker's is written.
        let (a2, mine2) = (a.clone(), mine.clone());
        flaky.after_the_get_of(
            "auth_user",
            after,
            Box::new(move |_| {
                a2.change_password(&p, &mine2, PW, NEW_PW, None, None)
                    .unwrap();
            }),
        );
        let done = a.login_step("ada", PW);
        if after < READS - 1 {
            assert_eq!(
                done,
                Err("invalid username or password".to_string()),
                "after read {after}: no session on a password that was changed while it was                  being checked"
            );
        }
        if let Ok(LoginOutcome::Session { token, .. }) = &done {
            assert!(
                a.resolve(token).is_none(),
                "after read {after}: revoked by the change"
            );
        }
        assert_eq!(
            live_sessions(&b, "ada"),
            1,
            "after read {after}: only the person's own session"
        );
        assert!(a.resolve(&mine).is_some());
        if after < READS - 1 {
            assert!(
                logged(&b, "login_refused")
                    .iter()
                    .any(|d| d.contains("credentials of the account changed")),
                "after read {after}: {:?}",
                logged(&b, "login_refused")
            );
        }
        // The new password signs in, and the old one does not.
        assert!(a.login_step("ada", NEW_PW).is_ok());
        assert!(a.login_step("ada", PW).is_err());
    }
}

#[test]
fn an_administrators_reset_ends_the_session_of_a_sign_in_that_began_before_it() {
    for after in 0..READS {
        let (a, flaky, b) = shared(AuthConfig::insecure_dev());
        user_with(&a, "root", &[ADMIN_GROUP]);
        user_with(&a, "ada", &["base.user"]);
        let (_, root) = session_of(&a, "root");
        let (a2, root2) = (a.clone(), root.clone());
        flaky.after_the_get_of(
            "auth_user",
            after,
            Box::new(move |_| {
                a2.reset_user_password(&root2, "ada", "a temporary password")
                    .unwrap();
            }),
        );
        let done = a.login_step("ada", PW);
        if after < READS - 1 {
            assert!(
                done.is_err(),
                "after read {after}: the old password gets no session, not even a limited one:                  {done:?}"
            );
        }
        assert_eq!(live_sessions(&b, "ada"), 0, "after read {after}");
    }
}

#[test]
fn new_groups_end_the_session_of_a_sign_in_that_read_the_old_ones() {
    // Two-factor is required of administrators. A person who becomes one while their sign-in is
    // being finished must not be left with a session that never proved a second factor (the
    // relimit after the change lists the sessions before this one is written).
    for after in 0..READS {
        let (a, flaky, b) = shared(enforcing());
        user_with(&a, "root", &[ADMIN_GROUP]);
        user_with(&a, "ada", &["base.user"]);
        // An administrator of an instance that demands two-factor has one.
        let (_, codes) = enrol(&a, "root");
        let (_, root) = session_of_mfa(&a, "root", &codes[0]);
        let (a2, root2) = (a.clone(), root.clone());
        flaky.after_the_get_of(
            "auth_user",
            after,
            Box::new(move |_| {
                a2.set_user_groups(
                    &root2,
                    "ada",
                    &["base.user".to_string(), ADMIN_GROUP.to_string()],
                    &known(),
                )
                .unwrap();
            }),
        );
        let done = a.login_step("ada", PW);
        if after < READS - 1 {
            assert!(done.is_err(), "after read {after}: {done:?}");
            assert_eq!(live_sessions(&b, "ada"), 0, "after read {after}");
        }
        // Whatever the position, no session of an administrator is free of the policy.
        for (_, t) in b.list("auth_token").unwrap() {
            if t["owner"] == "ada" && t["revoked"] != true {
                assert_eq!(t["limited"], true, "after read {after}: {t:?}");
            }
        }
    }
}

#[test]
fn a_password_change_ends_a_half_finished_sign_in_before_its_second_step() {
    let (a, _flaky, b) = shared(AuthConfig::insecure_dev());
    user_with(&a, "ada", &["base.user"]);
    let (secret, codes) = enrol(&a, "ada");
    let _ = secret;
    let (mine, p) = session_of_mfa(&a, "ada", &codes[0]);
    // The password step is passed with the old password; the person then changes it.
    let pending = mfa_token(&a, "ada");
    a.change_password(&p, &mine, PW, NEW_PW, None, Some(&codes[1]))
        .unwrap();
    let done = a.login_mfa(&pending, None, Some(&codes[2]));
    assert!(
        matches!(done, Err(MfaError::Expired)),
        "the old password's sign-in is over: {done:?}"
    );
    assert_eq!(
        live_sessions(&b, "ada"),
        1,
        "only the session that changed it"
    );
    assert!(
        logged(&b, "login_refused")
            .iter()
            .any(|d| d.contains("before the second step")),
        "{:?}",
        logged(&b, "login_refused")
    );
}

fn session_of_mfa(a: &Auth, user: &str, recovery: &str) -> (String, Principal) {
    let tok = mfa_token(a, user);
    let LoginOutcome::Session { token, .. } = a.login_mfa(&tok, None, Some(recovery)).unwrap()
    else {
        panic!("expected a session")
    };
    let p = a.resolve(&token).unwrap();
    (token, p)
}

fn ctx() -> WebauthnCtx {
    WebauthnCtx {
        rp_id: "obop.test".into(),
        origin: ORIGIN.into(),
        rp_name: "obop".into(),
        ip: "203.0.113.9".into(),
    }
}

fn pw() -> Reauth<'static> {
    Reauth {
        password: PW,
        ..Default::default()
    }
}

/// Ada with two passkeys (the first one used for passwordless sign-in), signed in; a proof ready
/// for the removal of the first, so the removal can run inside a hook.
struct PasswordlessRig {
    a: Arc<Auth>,
    flaky: Arc<Flaky>,
    b: Arc<dyn Backend>,
    /// The answer of the first key to a passwordless challenge, made before anything races.
    answer: serde_json::Value,
    /// The id of the first key.
    id: String,
    p: Principal,
    token: String,
}

fn passwordless_rig() -> PasswordlessRig {
    let (a, flaky, b) = shared(AuthConfig::insecure_dev());
    user_with(&a, "ada", &["base.user"]);
    let (token, p) = session_of(&a, "ada");
    let (mut a1, mut a2) = (SoftAuthenticator::es256(), SoftAuthenticator::es256());
    let opts = a.register_options(&p, &ctx(), &pw()).unwrap();
    a.register_finish(&p, &token, &ctx(), "one", &a1.create(&opts, ORIGIN))
        .unwrap();
    let opts = a.proof_options(&p, &ctx()).unwrap();
    a.proof(&p, &ctx(), &a1.get(&opts, ORIGIN)).unwrap();
    let opts = a.register_options(&p, &ctx(), &pw()).unwrap();
    a.register_finish(&p, &token, &ctx(), "two", &a2.create(&opts, ORIGIN))
        .unwrap();
    let opts = a.proof_options(&p, &ctx()).unwrap();
    a.proof(&p, &ctx(), &a1.get(&opts, ORIGIN)).unwrap();
    a.set_passwordless(&p, true, &pw()).unwrap();
    let list = a.list(&p, &ctx()).unwrap();
    let id = list["passkeys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["name"] == "one")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    // The answer of the first key to a passwordless challenge, made before anything races.
    let opts = a.passkey_login_options(&ctx(), None).unwrap();
    let answer = a1.get(&opts, ORIGIN);
    // The proof the removal needs, from the second key.
    let opts = a.proof_options(&p, &ctx()).unwrap();
    a.proof(&p, &ctx(), &a2.get(&opts, ORIGIN)).unwrap();
    PasswordlessRig {
        a,
        flaky,
        b,
        answer,
        id,
        p,
        token,
    }
}

#[test]
fn removing_a_passkey_ends_the_passwordless_sign_in_that_used_it_at_either_point() {
    // Once with the removal landing after the passkey was read and before the account was (the
    // passkey is checked again and is gone), once after both reads and before the session is
    // written (the epoch has changed).
    for after_gets in [0, 1] {
        let PasswordlessRig {
            a,
            flaky,
            b,
            answer,
            id,
            p,
            token,
        } = passwordless_rig();
        let (a2, p2, t2) = (a.clone(), p.clone(), token.clone());
        flaky.after_the_get_of(
            crate::passkey::PASSKEY_TABLE,
            after_gets,
            Box::new(move |_| {
                a2.remove(&p2, &t2, &id, &pw()).unwrap();
            }),
        );
        let before = live_sessions(&b, "ada");
        let done = a.passkey_login(&ctx(), None, &answer);
        assert!(
            done.is_err(),
            "after {after_gets} reads: a removed passkey gives no session: {done:?}"
        );
        assert_eq!(
            live_sessions(&b, "ada"),
            before,
            "after {after_gets} reads: no session was left behind"
        );
        assert_eq!(a.passkey_count("ada"), 1);
    }
}

#[test]
fn a_removed_second_factor_ends_the_sign_in_that_checked_it() {
    // An administrator's reset of the second factor between the password step and the second.
    let (a, _flaky, b) = shared(AuthConfig::insecure_dev());
    user_with(&a, "root", &[ADMIN_GROUP]);
    user_with(&a, "ada", &["base.user"]);
    let (_, root) = session_of(&a, "root");
    let (_, codes) = enrol(&a, "ada");
    let pending = mfa_token(&a, "ada");
    a.admin_reset(&root, "ada").unwrap();
    let done = a.login_mfa(&pending, None, Some(&codes[0]));
    assert!(matches!(done, Err(MfaError::Expired)), "{done:?}");
    assert_eq!(live_sessions(&b, "ada"), 0);
}

#[test]
fn a_second_step_finished_for_an_account_disabled_meanwhile_is_a_refused_sign_in() {
    // It was answered as a fault of the store ("ask the operator to set a server key").
    let (a, flaky, b) = shared(AuthConfig::insecure_dev());
    user_with(&a, "ada", &["base.user"]);
    let (_, codes) = enrol(&a, "ada");
    let pending = mfa_token(&a, "ada");
    let before = live_sessions(&b, "ada");
    flaky.after_the_next_insert_into(
        "auth_token",
        Box::new(|inner| {
            let mut f = Fields::new();
            f.insert("disabled".into(), J::Bool(true));
            inner.update("auth_user", "ada", &f).unwrap();
        }),
    );
    let done = a.login_mfa(&pending, None, Some(&codes[0]));
    assert!(matches!(done, Err(MfaError::Expired)), "{done:?}");
    assert_eq!(live_sessions(&b, "ada"), before);
}
