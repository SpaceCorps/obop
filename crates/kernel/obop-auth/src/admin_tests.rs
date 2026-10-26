//! D69: editing groups, creating users, offboarding and importing. Named acceptance tests:
//! `groups_edit_*`, `create_user_*`, `offboard_*`, `import_*`.

use super::*;
use crate::EVENT_TABLE;
use crate::mfa_tests::{PW, Rig, T0, enforcing, enrol, rig, rig_with, session, user_with};
use obop_commands::{ADMIN_GROUP, AccountsApi, Authenticator, LoginOutcome, MfaApi, NewUser};

fn known() -> Vec<String> {
    ["base.user", "sales.user", "sales.manager", ADMIN_GROUP]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn names(g: &[&str]) -> Vec<String> {
    g.iter().map(|s| s.to_string()).collect()
}

fn events(r: &Rig, kind: &str) -> Vec<(String, String, String)> {
    let mut v: Vec<(String, String, String)> =
        r.b.list(EVENT_TABLE)
            .unwrap()
            .into_iter()
            .filter(|(_, f)| f["kind"] == kind)
            .map(|(_, f)| {
                (
                    f["user"].as_str().unwrap().to_string(),
                    f["actor"].as_str().unwrap().to_string(),
                    f["detail"].as_str().unwrap().to_string(),
                )
            })
            .collect();
    v.sort();
    v
}

fn admin(r: &Rig, name: &str) -> Principal {
    user_with(&r.a, name, &[ADMIN_GROUP]);
    session(&r.a, name).1
}

#[test]
fn groups_edit_changes_the_groups_of_live_sessions_at_the_next_request() {
    let r = rig();
    let root = admin(&r, "root");
    user_with(&r.a, "ada", &["base.user"]);
    let (t, before) = session(&r.a, "ada");
    assert!(before.allows("base.user") && !before.allows("sales.user"));
    let done =
        r.a.set_user_groups(&root, "Ada", &names(&["base.user", "sales.user"]), &known())
            .unwrap();
    assert_eq!(done["before"], json!(["base.user"]));
    assert_eq!(done["after"], json!(["base.user", "sales.user"]));
    // The same token, resolved again, carries the new groups: nothing was revoked.
    let now = r.a.resolve(&t).unwrap();
    assert!(now.allows("sales.user"));
    // And the other way, for the agent token that was narrowed to a group the person then loses.
    let agent =
        r.a.issue_agent_token(&now, &names(&["sales.user"]), "bot")
            .unwrap();
    assert!(r.a.resolve(&agent).unwrap().allows("sales.user"));
    r.a.set_user_groups(&root, "ada", &names(&["base.user"]), &known())
        .unwrap();
    assert!(!r.a.resolve(&t).unwrap().allows("sales.user"));
    assert!(
        !r.a.resolve(&agent).unwrap().allows("sales.user"),
        "an agent cannot keep a group its owner lost"
    );
}

#[test]
fn groups_edit_is_audited_with_before_and_after_and_nothing_else() {
    let r = rig();
    let root = admin(&r, "root");
    user_with(&r.a, "ada", &["base.user", "sales.user"]);
    r.a.set_user_groups(
        &root,
        "ada",
        &names(&["base.user", "sales.manager"]),
        &known(),
    )
    .unwrap();
    let log = events(&r, "user_groups");
    assert_eq!(log.len(), 1);
    assert_eq!((log[0].0.as_str(), log[0].1.as_str()), ("ada", "root"));
    for part in [
        "added sales.manager",
        "removed sales.user",
        "now base.user, sales.manager",
    ] {
        assert!(log[0].2.contains(part), "{}", log[0].2);
    }
    // No change: no event, and the answer says so.
    let same =
        r.a.set_user_groups(
            &root,
            "ada",
            &names(&["sales.manager", "base.user"]),
            &known(),
        )
        .unwrap();
    assert_eq!(same["changed"], false);
    assert_eq!(events(&r, "user_groups").len(), 1);
}

#[test]
fn groups_edit_validates_names_against_the_registered_groups() {
    let r = rig();
    let root = admin(&r, "root");
    user_with(&r.a, "ada", &["retired.module"]);
    let bad = |g: &[&str]| {
        r.a.set_user_groups(&root, "ada", &names(g), &known())
            .unwrap_err()
    };
    assert!(matches!(bad(&["no.such"]), MfaError::Invalid(m) if m.contains("not a registered")));
    assert!(matches!(bad(&["has space"]), MfaError::Invalid(_)));
    assert!(matches!(
        bad(&["x".repeat(65).as_str()]),
        MfaError::Invalid(_)
    ));
    // A group the person already holds may stay even if no module declares it any more; it
    // cannot be given to anybody else.
    r.a.set_user_groups(
        &root,
        "ada",
        &names(&["retired.module", "base.user"]),
        &known(),
    )
    .unwrap();
    user_with(&r.a, "bob", &[]);
    assert!(
        r.a.set_user_groups(&root, "bob", &names(&["retired.module"]), &known())
            .is_err()
    );
    // Duplicates and blanks are tidied.
    let d =
        r.a.set_user_groups(
            &root,
            "bob",
            &names(&[" base.user ", "", "base.user"]),
            &known(),
        )
        .unwrap();
    assert_eq!(d["after"], json!(["base.user"]));
    assert!(matches!(
        r.a.set_user_groups(&root, "nobody", &names(&[]), &known()),
        Err(MfaError::Invalid(m)) if m.contains("no user")
    ));
}

#[test]
fn groups_edit_cannot_remove_the_last_administrator_or_lock_oneself_out() {
    let r = rig();
    let root = admin(&r, "root");
    // The only administrator keeps the group (and cannot remove it from themselves anyway).
    assert!(matches!(
        r.a.set_user_groups(&root, "root", &names(&["base.user"]), &known()),
        Err(MfaError::Invalid(m)) if m.contains("yourself")
    ));
    let second = admin(&r, "second");
    // Not from oneself even when another administrator exists.
    assert!(matches!(
        r.a.set_user_groups(&root, "root", &names(&["base.user"]), &known()),
        Err(MfaError::Invalid(m)) if m.contains("yourself")
    ));
    // Another administrator may take it, while two are enabled.
    r.a.set_user_groups(&second, "root", &names(&["base.user"]), &known())
        .unwrap();
    // Now `second` is the only one left: nobody else can demote them, and they cannot.
    admin(&r, "third");
    r.a.set_user_disabled(&second, "third", true).unwrap();
    assert!(matches!(
        r.a.set_user_groups(&second, "second", &names(&[]), &known()),
        Err(MfaError::Invalid(_))
    ));
    // A disabled administrator may be demoted: it is not one of the enabled ones.
    r.a.set_user_groups(&second, "third", &names(&["base.user"]), &known())
        .unwrap();
    // Editing one's own other groups is fine.
    r.a.set_user_groups(
        &second,
        "second",
        &names(&[ADMIN_GROUP, "base.user"]),
        &known(),
    )
    .unwrap();
}

#[test]
fn groups_edit_needs_an_enabled_administrator_with_a_full_session() {
    let r = rig_with(enforcing());
    user_with(&r.a, "root", &[ADMIN_GROUP]);
    user_with(&r.a, "ada", &["base.user"]);
    let (_t, ada) = session(&r.a, "ada");
    assert!(matches!(
        r.a.set_user_groups(&ada, "ada", &names(&[ADMIN_GROUP]), &known()),
        Err(MfaError::NotFound)
    ));
    // An administrator whose session is limited to enrolment is not an administrator yet.
    let (_t, limited) = session(&r.a, "root");
    assert!(limited.mfa_limited);
    assert!(matches!(
        r.a.set_user_groups(&limited, "ada", &names(&["base.user"]), &known()),
        Err(MfaError::NotFound)
    ));
    assert!(matches!(
        AccountsApi::create_user(
            &r.a,
            &limited,
            &NewUser {
                name: "x",
                password: None,
                groups: &[],
                must_change_password: true
            },
            &known()
        ),
        Err(MfaError::NotFound)
    ));
    assert!(matches!(
        r.a.offboard_user(&limited, "ada", ""),
        Err(MfaError::NotFound)
    ));
    // An agent has no such tools.
    let agent = Principal::agent("root/agent", &[ADMIN_GROUP], &[ADMIN_GROUP]);
    assert!(matches!(
        r.a.set_user_groups(&agent, "ada", &names(&[]), &known()),
        Err(MfaError::NotFound)
    ));
}

#[test]
fn groups_edit_applies_the_two_factor_policy_to_live_sessions() {
    let r = rig_with(enforcing());
    user_with(&r.a, "root", &[ADMIN_GROUP]);
    let (secret, _) = {
        // root enrols so that root's session is a full one.
        let (_t, p) = session(&r.a, "root");
        let begun = r.a.begin(&p).unwrap();
        let secret = totp::base32_decode(begun["secret"].as_str().unwrap()).unwrap();
        let code = totp::code_for_step(&secret, totp::step_at(T0));
        r.a.confirm(&p, &_t, &code).unwrap();
        (secret, ())
    };
    let root_session = match r.a.login_step("root", PW).unwrap() {
        LoginOutcome::MfaRequired { mfa_token } => match r
            .a
            .login_mfa(
                &mfa_token,
                Some(&totp::code_for_step(&secret, totp::step_at(T0) + 1)),
                None,
            )
            .unwrap()
        {
            LoginOutcome::Session { token, .. } => token,
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    };
    let root = r.a.resolve(&root_session).unwrap();
    assert!(!root.mfa_limited);
    user_with(&r.a, "ada", &["base.user"]);
    let (t, p) = session(&r.a, "ada");
    assert!(!p.mfa_limited);
    // Made an administrator without a second factor: the session she holds is limited at once.
    r.a.set_user_groups(&root, "ada", &names(&[ADMIN_GROUP]), &known())
        .unwrap();
    assert!(r.a.resolve(&t).unwrap().mfa_limited);
    assert!(
        events(&r, "user_groups")[0].2.contains("two-factor policy"),
        "{:?}",
        events(&r, "user_groups")
    );
    // Back to a member: the limit is lifted.
    r.a.set_user_groups(&root, "ada", &names(&["base.user"]), &known())
        .unwrap();
    assert!(!r.a.resolve(&t).unwrap().mfa_limited);
}

#[test]
fn create_user_with_a_password_or_a_generated_temporary_one() {
    let r = rig();
    let root = admin(&r, "root");
    let make = |name: &str, pw: Option<&str>, change: bool| {
        AccountsApi::create_user(
            &r.a,
            &root,
            &NewUser {
                name,
                password: pw,
                groups: &names(&["base.user"]),
                must_change_password: change,
            },
            &known(),
        )
    };
    // A chosen password, to be changed at the first sign-in.
    let a = make("Ada", Some("a password of mine"), true).unwrap();
    assert_eq!(
        (a["user"].clone(), a["must_change_password"].clone()),
        (json!("ada"), json!(true))
    );
    assert!(a.get("temporary_password").is_none());
    match r.a.login_step("ada", "a password of mine").unwrap() {
        LoginOutcome::Session {
            password_change_required,
            ..
        } => assert!(password_change_required),
        other => panic!("{other:?}"),
    }
    // A chosen password that need not change.
    make("bob", Some("a password of mine"), false).unwrap();
    assert!(matches!(
        r.a.login_step("bob", "a password of mine").unwrap(),
        LoginOutcome::Session {
            password_change_required: false,
            ..
        }
    ));
    // Generated: returned once, forced to change, never in the log.
    let c = make("cleo", None, false).unwrap();
    let temp = c["temporary_password"].as_str().unwrap().to_string();
    assert!(temp.len() >= MIN_PASSWORD_CHARS && c["must_change_password"] == true);
    assert!(matches!(
        r.a.login_step("cleo", &temp).unwrap(),
        LoginOutcome::Session {
            password_change_required: true,
            ..
        }
    ));
    let log = r.b.list(EVENT_TABLE).unwrap();
    assert!(
        !format!("{log:?}").contains(&temp) && !format!("{log:?}").contains("a password of mine")
    );
    assert_eq!(events(&r, "user_create").len(), 3);
    // Refusals.
    assert!(
        matches!(make("ada", Some("a password of mine"), true), Err(MfaError::Invalid(m)) if m.contains("exists"))
    );
    assert!(
        matches!(make("dan", Some("short"), true), Err(MfaError::Invalid(m)) if m.contains("12"))
    );
    assert!(matches!(make("a b", None, true), Err(MfaError::Invalid(_))));
    assert!(matches!(make("", None, true), Err(MfaError::Invalid(_))));
    let unknown = AccountsApi::create_user(
        &r.a,
        &root,
        &NewUser {
            name: "eve",
            password: None,
            groups: &names(&["no.such"]),
            must_change_password: true,
        },
        &known(),
    );
    assert!(matches!(unknown, Err(MfaError::Invalid(m)) if m.contains("not a registered")));
    assert!(r.a.user_record("eve").is_err(), "nothing was created");
}

/// An access token and a refresh token of a connected application, as `obop-oauth` stores them.
fn connect_app(r: &Rig, user: &str) -> (String, String) {
    let grant = format!("grant-{user}");
    let mut g = Fields::new();
    g.insert("owner".into(), J::from(user));
    g.insert("client_id".into(), J::from("c1"));
    g.insert("revoked".into(), J::Bool(false));
    g.insert("expires_at".into(), J::from(T0 + 86_400));
    r.b.insert("oauth_grant", &grant, &g).unwrap();
    let mut rf = Fields::new();
    rf.insert("grant".into(), J::from(grant.as_str()));
    r.b.insert("oauth_refresh", &format!("refresh-{user}"), &rf)
        .unwrap();
    let (token, _) = obop_commands::OAuthTokens::issue_oauth_access(
        &r.a,
        user,
        &names(&["base.user"]),
        "app",
        3600,
        "https://x.test/mcp",
        &grant,
    )
    .unwrap();
    (token, grant)
}

#[test]
fn offboard_ends_everything_and_enabling_does_not_bring_it_back() {
    let r = rig();
    let root = admin(&r, "root");
    user_with(&r.a, "ada", &["base.user"]);
    user_with(&r.a, "bob", &["base.user"]);
    let (s1, ada) = session(&r.a, "ada");
    let (s2, _) = session(&r.a, "ada");
    let agent =
        r.a.issue_agent_token(&ada, &names(&["base.user"]), "bot")
            .unwrap();
    let (app_token, grant) = connect_app(&r, "ada");
    let (bob_session, _) = session(&r.a, "bob");
    // A half-finished second step and a passkey proof.
    r.b.kv_put(&crate::passkey::proof_key("ada"), "{}", Some(60_000))
        .unwrap();
    let mut pending = Fields::new();
    pending.insert("user".into(), J::from("ada"));
    pending.insert("expires_at".into(), J::from(T0 + 300));
    r.b.insert("auth_mfa_token", "pending-1", &pending).unwrap();

    let done =
        r.a.offboard_user(&root, "Ada", "left the company\n; token=abc")
            .unwrap();
    assert_eq!(done["revoked_sessions"], 2);
    assert_eq!(done["revoked_agent_tokens"], 1);
    assert_eq!(done["revoked_apps"], 1);
    for t in [&s1, &s2, &agent, &app_token] {
        assert!(r.a.resolve(t).is_none());
    }
    assert!(r.a.resolve(&bob_session).is_some(), "others are untouched");
    assert!(
        r.b.kv_get(&crate::passkey::proof_key("ada"))
            .unwrap()
            .is_none()
    );
    assert!(r.b.get("auth_mfa_token", "pending-1").unwrap().is_none());
    assert!(r.b.get("oauth_refresh", "refresh-ada").unwrap().is_none());
    assert_eq!(
        r.b.get("oauth_grant", &grant).unwrap().unwrap()["revoked"],
        true
    );
    // Cannot sign in.
    assert!(r.a.login_step("ada", PW).is_err());
    // The list says offboarded.
    let list = r.a.users(&root).unwrap();
    let row = list["users"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["name"] == "ada")
        .unwrap()
        .clone();
    assert_eq!(
        (row["disabled"].clone(), row["offboarded"].clone()),
        (json!(true), json!(true))
    );
    // Enabled again: the person signs in with a new session, but nothing revoked comes back.
    r.a.set_user_disabled(&root, "ada", false).unwrap();
    assert!(r.a.login_step("ada", PW).is_ok());
    for t in [&s1, &s2, &agent, &app_token] {
        assert!(r.a.resolve(t).is_none(), "revoked stays revoked");
    }
    let list = r.a.users(&root).unwrap();
    let row = list["users"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["name"] == "ada")
        .unwrap()
        .clone();
    assert_eq!(row["offboarded"], false);
    // Audited: who, counts and the (cleaned) note; the enable says what stays revoked.
    let off = events(&r, "user_offboard");
    assert_eq!((off[0].0.as_str(), off[0].1.as_str()), ("ada", "root"));
    for part in [
        "2 session(s)",
        "1 agent token(s)",
        "1 connected app(s)",
        "left the company; token=abc",
    ] {
        assert!(off[0].2.contains(part), "{}", off[0].2);
    }
    assert!(!off[0].2.contains('\n'));
    assert!(events(&r, "user_enable")[0].2.contains("stay revoked"));
}

#[test]
fn offboard_refuses_oneself_strangers_and_the_last_enabled_administrator() {
    let r = rig();
    let root = admin(&r, "root");
    user_with(&r.a, "ada", &["base.user"]);
    let (_t, ada) = session(&r.a, "ada");
    assert!(
        matches!(r.a.offboard_user(&root, "root", ""), Err(MfaError::Invalid(m)) if m.contains("own"))
    );
    assert!(
        matches!(r.a.offboard_user(&root, "ghost", ""), Err(MfaError::Invalid(m)) if m.contains("no user"))
    );
    assert!(matches!(
        r.a.offboard_user(&ada, "root", ""),
        Err(MfaError::NotFound)
    ));
    // The operator cannot offboard the only enabled administrator either.
    assert!(r.a.operator_offboard("root", "").is_err());
    // With two administrators one can offboard the other; then the remaining one is protected.
    let second = admin(&r, "second");
    r.a.offboard_user(&second, "root", "").unwrap();
    assert!(r.a.operator_offboard("second", "").is_err());
}

#[test]
fn offboard_by_the_operator_has_no_self_check_and_logs_the_operator() {
    let r = rig();
    admin(&r, "root");
    user_with(&r.a, "ada", &["base.user"]);
    let (t, _) = session(&r.a, "ada");
    r.a.operator_offboard("ada", "").unwrap();
    assert!(r.a.resolve(&t).is_none());
    assert_eq!(events(&r, "user_offboard")[0].1, "operator");
    r.a.operator_set_disabled("ada", false).unwrap();
    r.a.operator_set_groups("ada", &names(&["sales.user"]), &known())
        .unwrap();
    assert_eq!(events(&r, "user_groups")[0].1, "operator");
}

// ---- a refused store is never "nothing to do" (D74) ---------------------------------------

/// A rig whose storage can be made to refuse calls, behind the request guard as in a server.
pub(crate) fn flaky_rig() -> (Rig, std::sync::Arc<obop_storage::testing::Flaky>) {
    flaky_rig_with(AuthConfig::insecure_dev())
}

pub(crate) fn flaky_rig_with(
    config: AuthConfig,
) -> (Rig, std::sync::Arc<obop_storage::testing::Flaky>) {
    let (flaky, b) = obop_storage::testing::Flaky::memory();
    let a = Auth::with_test_config(b.clone(), config);
    a.set_clock_for_test(T0);
    (Rig { a, b }, flaky)
}

pub(crate) fn request<T>(f: impl FnOnce() -> T) -> (T, obop_storage::fault::Outcome) {
    obop_storage::fault::begin_request(Default::default());
    let v = f();
    (v, obop_storage::fault::end_request())
}

#[test]
fn offboarding_with_a_token_list_that_is_refused_fails_as_partial_and_can_be_repeated() {
    use obop_storage::testing::Mode;
    let (r, flaky) = flaky_rig();
    let root = admin(&r, "root");
    user_with(&r.a, "ada", &["base.user"]);
    let (t1, _) = session(&r.a, "ada");
    let (agent_owner_session, ada) = session(&r.a, "ada");
    let agent =
        r.a.issue_agent_token(&ada, &names(&["base.user"]), "bot")
            .unwrap();
    // The reviewer's probe: the account is disabled, then the list of its tokens is refused.
    flaky.refuse_model("auth_token", Mode::Reads);
    let (done, out) = request(|| r.a.offboard_user(&root, "ada", "left"));
    assert!(
        matches!(&done, Err(MfaError::Unavailable(m)) if m.starts_with("storage_busy")),
        "never Ok with 0 revoked: {done:?}"
    );
    assert!(
        out.wrote && out.partial,
        "the account was disabled: {out:?}"
    );
    assert_eq!(out.fault, Some(obop_storage::fault::Fault::Busy));
    flaky.allow_models();
    // The disable did commit, so no token of the account resolves, and the log says what is missing.
    assert!(r.a.resolve(&t1).is_none() && r.a.resolve(&agent).is_none());
    let logged = events(&r, "user_offboard");
    assert!(
        logged.iter().any(|e| e.2.contains("INCOMPLETE")),
        "{logged:?}"
    );
    // Repeating the request finishes what is left and reports it.
    let (done, out) = request(|| r.a.offboard_user(&root, "ada", "left"));
    let done = done.unwrap();
    assert_eq!(done["revoked_sessions"], 2, "{done}");
    assert_eq!(done["revoked_agent_tokens"], 1);
    assert!(!out.partial);
    let _ = agent_owner_session;
}

#[test]
fn offboarding_goes_on_when_the_store_refuses_for_a_moment() {
    // One refused call at every position of the offboarding in turn. Whatever the position, the
    // answer is true: done (and the account is gone), or refused before anything changed (and the
    // account is untouched), or partial (the account is disabled and the log says what is left).
    let (mut done_count, mut none_count, mut partial_count) = (0, 0, 0);
    for skip in 0..60 {
        let (r, flaky) = flaky_rig();
        let root = admin(&r, "root");
        user_with(&r.a, "ada", &["base.user"]);
        let (t1, _) = session(&r.a, "ada");
        flaky.refuse_call_after(skip);
        let (done, out) = request(|| r.a.offboard_user(&root, "ada", ""));
        flaky.allow_models();
        let disabled =
            r.b.get("auth_user", "ada")
                .unwrap()
                .unwrap()
                .get("disabled")
                == Some(&J::Bool(true));
        match done {
            Ok(v) => {
                done_count += 1;
                assert_eq!(v["revoked_sessions"], 1, "skip {skip}: {v}");
                assert!(r.a.resolve(&t1).is_none(), "skip {skip}");
                assert!(!out.partial, "skip {skip}: {out:?}");
            }
            Err(_) if !out.wrote => {
                none_count += 1;
                assert!(!disabled, "skip {skip}: said nothing was changed");
                assert!(r.a.resolve(&t1).is_some(), "skip {skip}");
            }
            Err(_) => {
                partial_count += 1;
                assert!(out.partial && disabled, "skip {skip}: {out:?}");
            }
        }
    }
    assert!(
        done_count >= 3,
        "refusals that were waited out: {done_count}"
    );
    assert!(
        none_count >= 1 && partial_count + done_count >= 4,
        "{none_count} {partial_count}"
    );
}

#[test]
fn a_disable_or_an_offboarding_that_could_not_end_the_tokens_does_not_bring_them_back_on_enable() {
    use obop_storage::testing::Mode;
    // The revocation is refused and the administrator does not repeat the request. Enabling the
    // account later finishes it first, or the session (and for an offboarding the agent token)
    // the person was cut off from would resolve again.
    for offboard in [false, true] {
        let (r, flaky) = flaky_rig();
        let root = admin(&r, "root");
        user_with(&r.a, "ada", &["base.user"]);
        let (stolen, ada) = session(&r.a, "ada");
        let agent =
            r.a.issue_agent_token(&ada, &names(&["base.user"]), "bot")
                .unwrap();
        flaky.refuse_model("auth_token", Mode::Reads);
        let (done, out) = request(|| {
            if offboard {
                r.a.offboard_user(&root, "ada", "").map(|_| ())
            } else {
                r.a.set_user_disabled(&root, "ada", true)
            }
        });
        flaky.allow_models();
        assert!(
            done.is_err() && out.partial,
            "offboard={offboard}: {done:?} {out:?}"
        );
        // The enabling is refused while the tokens cannot be ended, and goes through when they can.
        flaky.refuse_model("auth_token", Mode::Reads);
        let (refused, _) = request(|| r.a.set_user_disabled(&root, "ada", false));
        flaky.allow_models();
        assert!(
            refused.is_err(),
            "offboard={offboard}: enabled with the tokens still live"
        );
        r.a.set_user_disabled(&root, "ada", false).unwrap();
        assert!(
            r.a.resolve(&stolen).is_none(),
            "offboard={offboard}: the session came back"
        );
        if offboard {
            assert!(r.a.resolve(&agent).is_none(), "the agent token came back");
        }
    }
}

#[test]
fn a_password_change_that_could_not_end_the_other_sessions_says_what_the_person_can_do() {
    use obop_storage::testing::Mode;
    let (r, flaky) = flaky_rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (mine, p) = session(&r.a, "ada");
    let (stolen, _) = session(&r.a, "ada");
    flaky.refuse_model("auth_token", Mode::Reads);
    let (done, out) = request(|| {
        r.a.change_password(&p, &mine, PW, "a brand new password", None, None)
    });
    flaky.allow_models();
    assert!(done.is_err() && out.partial, "{done:?} {out:?}");
    let (_, text) = incomplete_entry(&r, "password_change").expect("an INCOMPLETE entry");
    assert!(
        text.contains("sign out everywhere under Security") && !text.contains("repeat the request"),
        "a repeat cannot work (the old password is gone): {text}"
    );
    assert!(r.a.resolve(&stolen).is_some());
}

#[test]
fn a_note_for_the_repeat_is_not_a_change_so_a_refused_change_is_never_answered_partial() {
    // The note that lets a repeat finish the work is written before the change. If the change is
    // then refused, nothing was changed: the answer is "none", and the note is taken back. At
    // every position of turning two-factor off in turn: partial only when the factor really is off.
    for skip in 0..40 {
        let (r, flaky) = flaky_rig();
        user_with(&r.a, "ada", &["base.user"]);
        let (mine, p) = session(&r.a, "ada");
        let (_, codes) = enrol(&r.a, "ada");
        flaky.refuse_call_after(skip);
        let (done, out) = request(|| r.a.disable_in_session(&p, &mine, PW, None, Some(&codes[0])));
        flaky.allow_models();
        if done.is_ok() {
            continue;
        }
        let off = !r.a.mfa_enabled("ada");
        if out.partial {
            assert!(
                off,
                "skip {skip}: answered partial with the factor still on: {out:?}"
            );
        }
        if !off {
            let u = r.b.get("auth_user", "ada").unwrap().unwrap();
            assert!(
                u.get("sessions_owed").is_none_or(J::is_null),
                "skip {skip}: a stale note was left behind"
            );
        }
    }
}

#[test]
fn a_password_reset_whose_sessions_cannot_be_listed_is_a_partial_failure() {
    use obop_storage::testing::Mode;
    let (r, flaky) = flaky_rig();
    let root = admin(&r, "root");
    user_with(&r.a, "ada", &["base.user"]);
    let (t1, _) = session(&r.a, "ada");
    flaky.refuse_model("auth_token", Mode::Reads);
    let (done, out) = request(|| r.a.reset_user_password(&root, "ada", "a-new-temporary-pw"));
    assert!(matches!(done, Err(MfaError::Unavailable(_))), "{done:?}");
    assert!(out.partial, "{out:?}");
    flaky.allow_models();
    // The password did change, and the session that was not ended is still there for the retry.
    assert!(r.a.login_step("ada", PW).is_err());
    assert!(r.a.resolve(&t1).is_some());
    let (done, _) = request(|| {
        r.a.reset_user_password(&root, "ada", "another-temporary-pw")
    });
    done.unwrap();
    assert!(r.a.resolve(&t1).is_none());
}

/// The log entry of `kind` that says INCOMPLETE: its actor and its text.
fn incomplete_entry(r: &Rig, kind: &str) -> Option<(String, String)> {
    events(r, kind)
        .into_iter()
        .find(|e| e.2.contains("INCOMPLETE"))
        .map(|e| (e.1, e.2))
}

#[test]
fn a_group_change_that_could_not_relimit_the_sessions_is_finished_by_repeating_it() {
    use obop_storage::testing::Mode;
    let (r, flaky) = flaky_rig_with(enforcing());
    // root is an administrator with two-factor on, signed in with it: a full session.
    user_with(&r.a, "root", &[ADMIN_GROUP]);
    let (secret, _) = enrol(&r.a, "root");
    let LoginOutcome::MfaRequired { mfa_token } = r.a.login_step("root", PW).unwrap() else {
        panic!("root has two-factor on");
    };
    let code = totp::code_for_step(&secret, totp::step_at(T0) + 1);
    let LoginOutcome::Session {
        token: root_token, ..
    } = r.a.login_mfa(&mfa_token, Some(&code), None).unwrap()
    else {
        panic!("a session");
    };
    let root = r.a.resolve(&root_token).unwrap();
    assert!(!root.mfa_limited);
    user_with(&r.a, "ada", &["base.user"]);
    let (t, before) = session(&r.a, "ada");
    assert!(!before.mfa_limited);
    let groups = names(&["base.user", ADMIN_GROUP]);
    // Ada becomes an administrator on an instance that demands two-factor of administrators; the
    // live sessions cannot be relimited (the token list is refused).
    flaky.refuse_model("auth_token", Mode::Reads);
    let (done, out) = request(|| r.a.set_user_groups(&root, "ada", &groups, &known()));
    flaky.allow_models();
    assert!(matches!(done, Err(MfaError::Unavailable(_))), "{done:?}");
    assert!(out.wrote && out.partial, "{out:?}");
    assert!(!r.a.resolve(&t).unwrap().mfa_limited, "still not limited");
    // The entry is the administrator's own, with what is missing added; not a stranger's.
    let (actor, text) = incomplete_entry(&r, "user_groups").expect("an INCOMPLETE entry");
    assert_eq!(actor, "root");
    assert!(text.contains("groups changed: added"), "{text}");
    // The repeat finishes it, although the groups already are as asked.
    let (done, _) = request(|| r.a.set_user_groups(&root, "ada", &groups, &known()));
    assert_eq!(done.unwrap()["changed"], false);
    assert!(r.a.resolve(&t).unwrap().mfa_limited, "now limited");
}

#[test]
fn turning_two_factor_off_that_could_not_end_the_other_sessions_is_finished_by_repeating_it() {
    use obop_storage::testing::Mode;
    let (r, flaky) = flaky_rig();
    user_with(&r.a, "ada", &["base.user"]);
    // Two sessions exist before the second factor does (one is the person's, one is stolen).
    let (mine, p) = session(&r.a, "ada");
    let (stolen, _) = session(&r.a, "ada");
    let (_, codes) = enrol(&r.a, "ada");
    flaky.refuse_model("auth_token", Mode::Reads);
    let (done, out) = request(|| r.a.disable_in_session(&p, &mine, PW, None, Some(&codes[0])));
    flaky.allow_models();
    assert!(matches!(done, Err(MfaError::Unavailable(_))), "{done:?}");
    assert!(out.wrote && out.partial, "{out:?}");
    assert!(!r.a.mfa_enabled("ada"), "the factor is off");
    assert!(
        r.a.resolve(&stolen).is_some(),
        "the other session is still alive"
    );
    let (actor, text) = incomplete_entry(&r, "disable").expect("an INCOMPLETE entry");
    assert_eq!(actor, "ada");
    assert!(text.contains("turned off by the user"), "{text}");
    // The repeat does not say "two-factor is not on": it ends the session of the thief.
    let (done, _) = request(|| r.a.disable_in_session(&p, &mine, PW, None, Some(&codes[1])));
    done.unwrap();
    assert!(r.a.resolve(&stolen).is_none(), "the other session ended");
    assert!(r.a.resolve(&mine).is_some(), "this one stays");
    // Once finished, the repeat is the usual answer again.
    assert!(matches!(
        r.a.disable_in_session(&p, &mine, PW, None, Some(&codes[2])),
        Err(MfaError::Invalid(m)) if m.contains("not on")
    ));
}

#[test]
fn an_administrators_two_factor_reset_that_could_not_end_the_sessions_is_finished_by_repeating_it()
{
    use obop_storage::testing::Mode;
    let (r, flaky) = flaky_rig();
    let root = admin(&r, "root");
    user_with(&r.a, "ada", &["base.user"]);
    let (stolen, _) = session(&r.a, "ada");
    enrol(&r.a, "ada");
    flaky.refuse_model("auth_token", Mode::Reads);
    let (done, out) = request(|| r.a.admin_reset(&root, "ada"));
    flaky.allow_models();
    assert!(matches!(done, Err(MfaError::Unavailable(_))), "{done:?}");
    assert!(out.wrote && out.partial, "{out:?}");
    assert!(!r.a.mfa_enabled("ada"));
    assert!(r.a.resolve(&stolen).is_some());
    let (actor, _) = incomplete_entry(&r, "reset").expect("an INCOMPLETE entry");
    assert_eq!(actor, "root");
    let (done, _) = request(|| r.a.admin_reset(&root, "ada"));
    done.unwrap();
    assert!(
        r.a.resolve(&stolen).is_none(),
        "the session ended on the repeat"
    );
    assert!(
        matches!(r.a.admin_reset(&root, "ada"), Err(MfaError::Invalid(m)) if m.contains("no two-factor")),
        "once finished, the usual answer"
    );
}

#[test]
fn a_lookup_that_is_refused_is_the_stores_trouble_and_not_a_400_or_a_404() {
    use obop_storage::testing::Mode;
    let (r, flaky) = flaky_rig();
    let root = admin(&r, "root");
    user_with(&r.a, "ada", &["base.user"]);
    flaky.refuse_model("auth_user", Mode::Reads);
    for done in [
        request(|| r.a.offboard_user(&root, "ada", ""))
            .0
            .map(|_| ()),
        request(|| r.a.set_user_disabled(&root, "ada", true)).0,
        request(|| {
            r.a.set_user_groups(&root, "ada", &names(&["base.user"]), &known())
        })
        .0
        .map(|_| ()),
    ] {
        assert!(
            matches!(&done, Err(MfaError::Unavailable(m)) if m.starts_with("storage_busy")),
            "{done:?}"
        );
    }
    flaky.allow_models();
    assert!(
        matches!(request(|| r.a.offboard_user(&root, "ghost", "")).0, Err(MfaError::Invalid(m)) if m.contains("no user"))
    );
}

#[test]
fn import_checks_every_row_first_and_creates_all_or_nothing() {
    let r = rig();
    user_with(&r.a, "taken", &[]);
    let row = |line, name: &str, g: &[&str]| ImportRow {
        line,
        name: name.into(),
        groups: names(g),
    };
    let good = vec![
        row(2, "Ann", &["base.user"]),
        row(3, "ben", &["sales.user", "base.user"]),
    ];
    // Dry run: nothing is written.
    let dry = r.a.import_users(&good, &known(), true);
    assert!(
        dry.iter()
            .all(|o| o.error.is_none() && o.temporary_password.is_none())
    );
    assert!(r.a.user_record("ann").is_err());
    // One bad row refuses all, and says which.
    let mixed = vec![
        row(2, "cy", &["base.user"]),
        row(3, "taken", &[]),
        row(4, "dee", &["no.such"]),
        row(5, "cy", &[]),
        row(6, "bad name", &[]),
    ];
    let out = r.a.import_users(&mixed, &known(), false);
    let errs: Vec<_> = out
        .iter()
        .map(|o| o.error.as_deref().unwrap_or(""))
        .collect();
    assert_eq!(errs[0], "");
    assert!(errs[1].contains("exists") && errs[2].contains("not a registered"));
    assert!(errs[3].contains("twice") && errs[4].contains("username"));
    assert!(out.iter().all(|o| o.temporary_password.is_none()));
    assert!(r.a.user_record("cy").is_err(), "nothing created");
    // A clean file.
    let out = r.a.import_users(&good, &known(), false);
    assert!(out.iter().all(|o| o.error.is_none()));
    let pw = out[0].temporary_password.clone().unwrap();
    assert_ne!(Some(&pw), out[1].temporary_password.as_ref());
    match r.a.login_step("ann", &pw).unwrap() {
        LoginOutcome::Session {
            password_change_required,
            ..
        } => assert!(password_change_required),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        str_list(&r.a.user_record("ben").unwrap()["groups"]),
        ["sales.user", "base.user"]
    );
    assert_eq!(
        events(&r, "user_create")
            .iter()
            .filter(|e| e.1 == "operator")
            .count(),
        2
    );
    assert!(!format!("{:?}", r.b.list(EVENT_TABLE).unwrap()).contains(&pw));
}

#[test]
fn admin_commands_work_with_a_two_factor_admin_session() {
    // The same checks hold for an administrator who signed in with a second factor.
    let r = rig_with(enforcing());
    user_with(&r.a, "root", &[ADMIN_GROUP]);
    let (secret, _) = enrol(&r.a, "root");
    let mfa = crate::mfa_tests::mfa_token(&r.a, "root");
    r.a.set_clock_for_test(T0 + 30);
    let LoginOutcome::Session { token, .. } =
        r.a.login_mfa(
            &mfa,
            Some(&crate::mfa_tests::code_at(&secret, T0 + 30)),
            None,
        )
        .unwrap()
    else {
        panic!()
    };
    let root = r.a.resolve(&token).unwrap();
    assert!(!root.mfa_limited);
    user_with(&r.a, "ada", &["base.user"]);
    r.a.set_user_groups(&root, "ada", &names(&["sales.user"]), &known())
        .unwrap();
}

#[test]
fn groups_edit_an_approved_agent_proposal_runs_as_the_agent_but_cannot_touch_its_owner() {
    let r = rig();
    admin(&r, "root");
    let second = admin(&r, "second");
    // How the command layer runs an approved proposal: the owner's current rights, named
    // `root/agent` in the log.
    let mut agent = r.a.administrator("root").unwrap();
    agent.id = "root/agent".into();
    assert!(matches!(
        r.a.set_user_groups(&agent, "root", &names(&["base.user"]), &known()),
        Err(MfaError::Invalid(m)) if m.contains("yourself")
    ));
    assert!(
        matches!(r.a.offboard_user(&agent, "root", ""), Err(MfaError::Invalid(m)) if m.contains("own"))
    );
    assert!(
        matches!(r.a.set_user_disabled(&agent, "root", true), Err(MfaError::Invalid(m)) if m.contains("own"))
    );
    user_with(&r.a, "ada", &["base.user"]);
    r.a.set_user_groups(&agent, "ada", &names(&["sales.user"]), &known())
        .unwrap();
    assert_eq!(events(&r, "user_groups")[0].1, "root/agent");
    // A disabled owner has no administrator to run as.
    r.a.set_user_disabled(&second, "root", true).unwrap();
    assert!(r.a.administrator("root").is_none());
    assert!(
        r.a.administrator("ada").is_none(),
        "a member is not an administrator"
    );
}
