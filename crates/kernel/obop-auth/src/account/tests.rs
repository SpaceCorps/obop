use super::*;
use crate::EVENT_TABLE;
use crate::mfa_tests::{
    PW, T0, code_at, enforcing, enrol, mfa_token, rig, rig_with, session, user_with,
};
use obop_commands::{ADMIN_GROUP, Authenticator, LoginOutcome, MfaApi};

fn events(r: &crate::mfa_tests::Rig) -> Vec<(String, String, String)> {
    let mut v: Vec<(String, String, String)> =
        r.b.list(EVENT_TABLE)
            .unwrap()
            .into_iter()
            .map(|(_, f)| {
                (
                    f["kind"].as_str().unwrap().to_string(),
                    f["actor"].as_str().unwrap().to_string(),
                    f["detail"].as_str().unwrap().to_string(),
                )
            })
            .collect();
    v.sort();
    v
}

fn kinds(r: &crate::mfa_tests::Rig, kind: &str) -> Vec<(String, String)> {
    events(r)
        .into_iter()
        .filter(|(k, _, _)| k == kind)
        .map(|(_, a, d)| (a, d))
        .collect()
}

#[test]
fn sessions_are_listed_without_anything_that_could_be_used_as_a_credential() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (t1, p) = session(&r.a, "ada");
    let (t2, _) = session(&r.a, "ada");
    user_with(&r.a, "bob", &["base.user"]);
    let _ = session(&r.a, "bob");
    let list = r.a.sessions(&p, &t2).unwrap();
    let rows = list["sessions"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "only ada's");
    assert_eq!(rows.iter().filter(|s| s["current"] == true).count(), 1);
    let text = list.to_string();
    for token in [&t1, &t2] {
        assert!(!text.contains(token.as_str()) && !text.contains(&token_hash(token)));
    }
    assert!(rows[0]["created_at"].as_str().unwrap().ends_with('Z'));
    assert!(rows[0]["id"].as_str().unwrap().len() == 12);
    // Agents have no sessions to list.
    let agent =
        r.a.issue_agent_token(&p, &["base.user".into()], "bot")
            .unwrap();
    let ap = r.a.resolve(&agent).unwrap();
    assert!(matches!(r.a.sessions(&ap, &agent), Err(MfaError::NotFound)));
}

#[test]
fn sign_out_everywhere_ends_every_session_but_not_agent_tokens_or_other_people() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    user_with(&r.a, "bob", &["base.user"]);
    let (t1, p) = session(&r.a, "ada");
    let (t2, _) = session(&r.a, "ada");
    let (bob, _) = session(&r.a, "bob");
    let agent =
        r.a.issue_agent_token(&p, &["base.user".into()], "bot")
            .unwrap();
    // Keeping the current one.
    assert_eq!(r.a.revoke_sessions(&p, &t1, true).unwrap(), 1);
    assert!(r.a.resolve(&t1).is_some() && r.a.resolve(&t2).is_none());
    // Everywhere, this one too.
    assert_eq!(r.a.revoke_sessions(&p, &t1, false).unwrap(), 1);
    assert!(r.a.resolve(&t1).is_none());
    assert!(r.a.resolve(&bob).is_some(), "other people are untouched");
    assert!(
        r.a.resolve(&agent).is_some(),
        "an agent token is a separate credential"
    );
    assert_eq!(kinds(&r, "sessions_revoked").len(), 2);
}

#[test]
fn changing_the_password_needs_the_current_one_signs_out_the_others_and_is_logged() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (t1, p) = session(&r.a, "ada");
    let (t2, _) = session(&r.a, "ada");
    let new = "a brand new passphrase";
    // Wrong current password: refused, counted toward the lockout, nothing changed.
    assert!(matches!(
        r.a.change_password(&p, &t1, "not it at all!!", new, None, None),
        Err(MfaError::BadPassword)
    ));
    assert!(r.a.login("ada", PW).is_ok());
    // Too short, and the same as before.
    assert!(matches!(
        r.a.change_password(&p, &t1, PW, "short", None, None),
        Err(MfaError::Invalid(_))
    ));
    assert!(matches!(
        r.a.change_password(&p, &t1, PW, PW, None, None),
        Err(MfaError::Invalid(_))
    ));
    r.a.change_password(&p, &t1, PW, new, None, None).unwrap();
    assert!(r.a.resolve(&t1).is_some(), "this session stays");
    assert!(r.a.resolve(&t2).is_none(), "the others end");
    assert!(r.a.login("ada", PW).is_err());
    assert!(r.a.login("ada", new).is_ok());
    let log = kinds(&r, "password_change");
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].0, "ada");
    assert!(log[0].1.contains("other session"), "{}", log[0].1);
    // Nothing in the log carries a password.
    let all = format!("{:?}", events(&r));
    assert!(!all.contains(PW) && !all.contains(new));
}

#[test]
fn wrong_current_passwords_lock_the_account_like_wrong_sign_ins() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (t, p) = session(&r.a, "ada");
    for _ in 0..5 {
        assert!(matches!(
            r.a.change_password(
                &p,
                &t,
                "wrong password!!",
                "another long passphrase",
                None,
                None
            ),
            Err(MfaError::BadPassword)
        ));
    }
    assert!(matches!(
        r.a.change_password(&p, &t, PW, "another long passphrase", None, None),
        Err(MfaError::Locked(_))
    ));
    assert!(r.a.login("ada", PW).unwrap_err().contains("too many"));
}

#[test]
fn with_two_factor_on_a_password_change_also_needs_a_code() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (secret, _) = enrol(&r.a, "ada");
    r.a.set_clock_for_test(T0 + 60);
    let t = {
        let m = mfa_token(&r.a, "ada");
        let LoginOutcome::Session { token, .. } =
            r.a.login_mfa(&m, Some(&code_at(&secret, T0 + 60)), None)
                .unwrap_or_else(|_| panic!())
        else {
            panic!()
        };
        token
    };
    r.a.set_clock_for_test(T0 + 120);
    let p = r.a.resolve(&t).unwrap();
    let new = "a brand new passphrase";
    assert!(matches!(
        r.a.change_password(&p, &t, PW, new, None, None),
        Err(MfaError::Invalid(_))
    ));
    assert!(matches!(
        r.a.change_password(&p, &t, PW, new, Some("000000"), None),
        Err(MfaError::Denied(_))
    ));
    r.a.set_clock_for_test(T0 + 180);
    r.a.change_password(&p, &t, PW, new, Some(&code_at(&secret, T0 + 180)), None)
        .unwrap();
    assert!(r.a.mfa_enabled("ada"));
}

#[test]
fn turning_two_factor_off_signs_out_every_other_session() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (secret, _) = enrol(&r.a, "ada");
    let sign_in = |at: u64| {
        r.a.set_clock_for_test(at);
        let m = mfa_token(&r.a, "ada");
        let LoginOutcome::Session { token, .. } =
            r.a.login_mfa(&m, Some(&code_at(&secret, at)), None)
                .unwrap_or_else(|_| panic!())
        else {
            panic!()
        };
        token
    };
    let (mine, other) = (sign_in(T0 + 60), sign_in(T0 + 120));
    r.a.set_clock_for_test(T0 + 180);
    let p = r.a.resolve(&mine).unwrap();
    r.a.disable_in_session(&p, &mine, PW, Some(&code_at(&secret, T0 + 180)), None)
        .unwrap();
    assert!(!r.a.mfa_enabled("ada"));
    assert!(
        r.a.resolve(&mine).is_some(),
        "the session that did it stays"
    );
    assert!(r.a.resolve(&other).is_none(), "the other one is gone");
    assert!(kinds(&r, "disable")[0].1.contains("other session"));
}

#[test]
fn an_administrator_can_list_disable_and_enable_accounts() {
    let r = rig();
    user_with(&r.a, "root", &[ADMIN_GROUP]);
    user_with(&r.a, "ada", &["base.user"]);
    let (t_root, root) = session(&r.a, "root");
    let (t_ada, ada) = session(&r.a, "ada");
    let agent =
        r.a.issue_agent_token(&ada, &["base.user".into()], "bot")
            .unwrap();
    // The list holds no hash or secret, and shows the state.
    let list = r.a.users(&root).unwrap();
    let text = list.to_string();
    assert!(!text.contains("argon2") && !text.contains("password_hash"));
    let rows = list["users"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["name"], "ada");
    assert_eq!(
        (rows[0]["sessions"].clone(), rows[0]["disabled"].clone()),
        (json!(1), json!(false))
    );
    assert_eq!(rows[1]["admin"], true);
    // People who are not administrators see nothing of it.
    assert!(matches!(r.a.users(&ada), Err(MfaError::NotFound)));
    assert!(matches!(
        r.a.set_user_disabled(&ada, "root", true),
        Err(MfaError::NotFound)
    ));

    r.a.set_user_disabled(&root, "ada", true).unwrap();
    assert!(r.a.resolve(&t_ada).is_none(), "its sessions ended");
    assert!(
        r.a.resolve(&agent).is_none(),
        "and its agent tokens stop working"
    );
    // The sign-in fails the same way as a wrong password; the log says why.
    assert_eq!(
        r.a.login("ada", PW).unwrap_err(),
        "invalid username or password"
    );
    assert_eq!(kinds(&r, "login_refused").len(), 1);
    assert_eq!(
        kinds(&r, "user_disable"),
        vec![(
            "root".to_string(),
            "account disabled; 1 session(s) signed out; its tokens stop working".to_string()
        )]
    );
    assert!(r.a.users(&root).unwrap()["users"][0]["disabled"] == true);

    r.a.set_user_disabled(&root, "ada", false).unwrap();
    assert!(r.a.login("ada", PW).is_ok());
    assert!(r.a.resolve(&agent).is_some(), "the agent token works again");
    assert_eq!(kinds(&r, "user_enable").len(), 1);
    assert!(r.a.resolve(&t_root).is_some());
}

#[test]
fn an_administrator_cannot_lock_everyone_out() {
    let r = rig();
    user_with(&r.a, "root", &[ADMIN_GROUP]);
    user_with(&r.a, "other", &[ADMIN_GROUP]);
    let (_, root) = session(&r.a, "root");
    // Not oneself.
    assert!(
        matches!(r.a.set_user_disabled(&root, "root", true), Err(MfaError::Invalid(m)) if m.contains("own"))
    );
    // The last enabled administrator is protected from the other one.
    r.a.set_user_disabled(&root, "other", true).unwrap();
    let (_, other_root) = (0, r.a.principal("other"));
    assert!(other_root.is_none(), "disabled accounts hold no principal");
    user_with(&r.a, "third", &[ADMIN_GROUP]);
    let (_, third) = session(&r.a, "third");
    // root is now the only other enabled admin besides third: disabling one of the two is fine,
    // disabling the last is not.
    r.a.set_user_disabled(&third, "root", true).unwrap();
    assert!(matches!(
        r.a.set_user_disabled(&third, "third", true),
        Err(MfaError::Invalid(_))
    ));
    let _ = third;
    // Unknown accounts.
    assert!(matches!(
        r.a.set_user_disabled(&session(&r.a, "third").1, "nobody", true),
        Err(MfaError::Invalid(_))
    ));
}

#[test]
fn a_password_reset_by_an_administrator_forces_a_new_password_at_the_next_sign_in() {
    let r = rig();
    user_with(&r.a, "root", &[ADMIN_GROUP]);
    user_with(&r.a, "ada", &["base.user"]);
    let (_, root) = session(&r.a, "root");
    let (t_old, _) = session(&r.a, "ada");
    // Locked out by guesses, then reset by the administrator.
    for _ in 0..5 {
        let _ = r.a.login("ada", "wrong");
    }
    assert!(matches!(
        r.a.reset_user_password(&root, "ada", "short"),
        Err(MfaError::Invalid(_))
    ));
    assert!(matches!(
        r.a.reset_user_password(&root, "root", "a temporary passphrase"),
        Err(MfaError::Invalid(_))
    ));
    r.a.reset_user_password(&root, "ada", "a temporary passphrase")
        .unwrap();
    assert!(r.a.resolve(&t_old).is_none(), "sessions ended");
    assert!(r.a.login("ada", PW).is_err(), "the old password is gone");
    // The temporary password signs in, but only to change it.
    let LoginOutcome::Session {
        token,
        mfa_enrolment_required,
        password_change_required,
    } = r.a.login_step("ada", "a temporary passphrase").unwrap()
    else {
        panic!()
    };
    assert!(password_change_required && !mfa_enrolment_required);
    let p = r.a.resolve(&token).unwrap();
    assert!(p.mfa_limited, "a limited session holds no permission");
    assert!(!p.allows("base.user"));
    // Choosing a new one lifts the limit.
    r.a.change_password(
        &p,
        &token,
        "a temporary passphrase",
        "chosen by ada herself",
        None,
        None,
    )
    .unwrap();
    let p = r.a.resolve(&token).unwrap();
    assert!(!p.mfa_limited && p.allows("base.user"));
    let LoginOutcome::Session {
        password_change_required,
        ..
    } = r.a.login_step("ada", "chosen by ada herself").unwrap()
    else {
        panic!()
    };
    assert!(!password_change_required);
    let log = events(&r);
    assert!(
        log.iter()
            .any(|(k, a, _)| k == "password_reset" && a == "root")
    );
    assert!(!format!("{log:?}").contains("a temporary passphrase"));
}

#[test]
fn a_reset_for_someone_who_must_also_enrol_keeps_the_enrolment_limit_after_the_change() {
    let r = rig_with(enforcing());
    user_with(&r.a, "root", &[ADMIN_GROUP]);
    user_with(&r.a, "boss", &[ADMIN_GROUP]);
    // Enrol root so root can be the one resetting.
    let (secret, _) = enrol(&r.a, "root");
    r.a.set_clock_for_test(T0 + 60);
    let m = mfa_token(&r.a, "root");
    let LoginOutcome::Session { token, .. } =
        r.a.login_mfa(&m, Some(&code_at(&secret, T0 + 60)), None)
            .unwrap_or_else(|_| panic!())
    else {
        panic!()
    };
    let root = r.a.resolve(&token).unwrap();
    r.a.reset_user_password(&root, "boss", "a temporary passphrase")
        .unwrap();
    let LoginOutcome::Session {
        token: bt,
        password_change_required,
        ..
    } = r.a.login_step("boss", "a temporary passphrase").unwrap()
    else {
        panic!()
    };
    assert!(password_change_required);
    let p = r.a.resolve(&bt).unwrap();
    r.a.change_password(
        &p,
        &bt,
        "a temporary passphrase",
        "boss picks a long one",
        None,
        None,
    )
    .unwrap();
    assert!(
        r.a.resolve(&bt).unwrap().mfa_limited,
        "still owes two-factor enrolment"
    );
}

#[test]
fn old_and_revoked_tokens_are_purged_without_touching_live_ones() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (live, p) = session(&r.a, "ada");
    let (gone, _) = session(&r.a, "ada");
    r.a.revoke(&gone);
    r.a.set_clock_for_test(T0 + 3 * 86_400);
    r.b.counter_reset("auth.purge").unwrap();
    r.a.purge_tokens();
    assert!(
        r.b.get("auth_token", &token_hash(&gone)).unwrap().is_none(),
        "revoked and old: removed"
    );
    // The live one had expired too by then (12 hours), so it went as well; a fresh one stays.
    assert!(r.a.resolve(&live).is_none());
    let (fresh, _) = session(&r.a, "ada");
    r.b.counter_reset("auth.purge").unwrap();
    r.a.purge_tokens();
    assert!(r.a.resolve(&fresh).is_some());
    let _ = p;
}
