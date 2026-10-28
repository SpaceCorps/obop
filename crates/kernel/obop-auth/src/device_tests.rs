//! The devices of mobile apps (the D64 amendment) as the identity provider ends them: with the
//! sessions of the person on a credential change, and for good with an offboarding. The sign-in
//! of an app is played here by writing what `obop-oauth` writes (a device, its family and an
//! access token); the protocol itself is tested in `obop-oauth` and over HTTP in `crates/obop`.

use super::*;
use crate::EVENT_TABLE;
use crate::mfa_tests::{PW, Rig, enrol, rig, session, user_with};
use obop_commands::devices as dev;
use obop_commands::{ADMIN_GROUP, AccountsApi, Authenticator, MfaApi, OAuthTokens};

struct App {
    access: String,
    device: String,
    grant: String,
    refresh: String,
}

fn fields(v: J) -> Fields {
    v.as_object().cloned().unwrap_or_default()
}

/// A mobile app signed in as `user`: the records of a code exchange, and its access token.
fn sign_in(r: &Rig, user: &str, name: &str) -> App {
    let (grant, device, refresh) = (
        format!("og_{user}_{name}"),
        format!("dev_{user}_{name}"),
        format!("rf_{user}_{name}"),
    );
    let (access, access_id) =
        r.a.issue_mobile_access(
            user,
            &["base.user".to_string()],
            "App (mobile)",
            3600,
            "https://inst.test/api",
            &grant,
            &device,
        )
        .unwrap();
    let meta = dev::Metadata {
        name: Some(name.into()),
        platform: Some("android".into()),
        app_version: None,
    };
    // The records carry real time, as the OAuth server writes it; the test clock of `Auth` is old.
    let now = now_system();
    r.b.insert(
        dev::MODEL,
        &device,
        &dev::new_record(user, "App", &meta, &grant, now, now + 86_400),
    )
    .unwrap();
    r.b.insert(
        "oauth_grant",
        &grant,
        &fields(json!({
            "owner": user, "client_id": "oc_x", "client_name": "App", "revoked": false,
            "access_id": access_id, "device": device, "class": "mobile",
            "scopes": ["base.user"], "expires_at": now + 86_400,
        })),
    )
    .unwrap();
    r.b.insert(
        "oauth_refresh",
        &refresh,
        &fields(json!({"grant": grant, "used": false})),
    )
    .unwrap();
    App {
        access,
        device,
        grant,
        refresh,
    }
}

fn state(r: &Rig, a: &App) -> String {
    r.b.get(dev::MODEL, &a.device).unwrap().unwrap()["state"]
        .as_str()
        .unwrap()
        .to_string()
}

fn ended(r: &Rig, a: &App) -> bool {
    r.a.resolve(&a.access).is_none()
        && state(r, a) == "revoked"
        && r.b.get("oauth_grant", &a.grant).unwrap().unwrap()["revoked"] == true
        && r.b.get("oauth_refresh", &a.refresh).unwrap().is_none()
}

fn device_events(r: &Rig, user: &str) -> Vec<String> {
    r.b.list(EVENT_TABLE)
        .unwrap()
        .into_iter()
        .filter(|(_, f)| f["kind"] == "device_revoked" && f["user"] == user)
        .map(|(_, f)| f["detail"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_mobile_token_is_the_person_narrowed_and_never_a_person() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user", "sales.user"]);
    let app = sign_in(&r, "ada", "Pixel");
    let (p, audience) = r.a.resolve_with_audience(&app.access).unwrap();
    assert_eq!(audience.as_deref(), Some("https://inst.test/api"));
    assert_eq!((p.id.as_str(), p.kind), ("ada", PrincipalKind::User));
    assert!(p.allows("base.user") && !p.allows("sales.user"));
    assert!(!p.is_person(), "so it manages no account");
    // None of the things a person does to their own account.
    assert!(matches!(r.a.status(&p), Err(MfaError::NotFound)));
    assert!(matches!(r.a.begin(&p), Err(MfaError::NotFound)));
    assert!(r.a.sessions_impl(&p, "x").is_err());
    assert!(
        r.a.issue_agent_token(&p, &["base.user".to_string()], "x")
            .is_err()
    );
    // A session is a person.
    let (_, s) = session(&r.a, "ada");
    assert!(s.is_person());
    assert!(
        r.a.issue_agent_token(&s, &["base.user".to_string()], "x")
            .is_ok()
    );
    // The administrator group does not make a mobile token an administrator of accounts.
    user_with(&r.a, "root", &[ADMIN_GROUP, "base.user"]);
    let app = sign_in(&r, "root", "Admin phone");
    let p = r.a.resolve(&app.access).unwrap();
    assert!(
        matches!(r.a.users(&p), Err(MfaError::NotFound)),
        "a narrowed token administers nobody"
    );
}

#[test]
fn a_new_password_ends_the_devices_of_that_person_and_no_others() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    user_with(&r.a, "bob", &["base.user"]);
    let (mine, p) = session(&r.a, "ada");
    let (a1, a2) = (sign_in(&r, "ada", "Phone"), sign_in(&r, "ada", "Tablet"));
    let b1 = sign_in(&r, "bob", "Bob's phone");
    r.a.change_password(&p, &mine, PW, "a brand new password", None, None)
        .unwrap();
    assert!(ended(&r, &a1) && ended(&r, &a2));
    assert!(!ended(&r, &b1) && r.a.resolve(&b1.access).is_some());
    assert_eq!(device_events(&r, "ada").len(), 2);
    assert!(device_events(&r, "ada")[0].contains("ended by password_change"));
    assert!(device_events(&r, "bob").is_empty());
    // A device the person signs in afterwards is a new one and is not touched by the repeat.
    let a3 = sign_in(&r, "ada", "New phone");
    assert!(r.a.resolve(&a3.access).is_some());
}

#[test]
fn turning_two_factor_off_an_administrators_reset_and_a_disable_end_the_devices() {
    let r = rig();
    user_with(&r.a, "root", &[ADMIN_GROUP]);
    let (_, root) = session(&r.a, "root");
    // The person turns the second factor off.
    user_with(&r.a, "ada", &["base.user"]);
    let (token, p) = session(&r.a, "ada");
    let (_, codes) = enrol(&r.a, "ada");
    let app = sign_in(&r, "ada", "Phone");
    r.a.disable_in_session(&p, &token, PW, None, Some(&codes[0]))
        .unwrap();
    assert!(ended(&r, &app), "turned off by the person");
    assert!(device_events(&r, "ada")[0].contains("ended by disable"));
    // An administrator resets the second factor of somebody else.
    user_with(&r.a, "bob", &["base.user"]);
    enrol(&r.a, "bob");
    let app = sign_in(&r, "bob", "Phone");
    r.a.admin_reset(&root, "bob").unwrap();
    assert!(ended(&r, &app), "reset by an administrator");
    // An administrator disables an account: the devices go with its sessions.
    user_with(&r.a, "cy", &["base.user"]);
    let app = sign_in(&r, "cy", "Phone");
    r.a.set_user_disabled(&root, "cy", true).unwrap();
    assert!(ended(&r, &app), "disabled");
    r.a.set_user_disabled(&root, "cy", false).unwrap();
    assert!(
        r.a.resolve(&app.access).is_none(),
        "enabling brings nothing back"
    );
    assert_eq!(state(&r, &app), "revoked");
}

#[test]
fn an_offboarding_ends_the_devices_for_good_and_counts_them() {
    let r = rig();
    user_with(&r.a, "root", &[ADMIN_GROUP]);
    let (_, root) = session(&r.a, "root");
    user_with(&r.a, "ada", &["base.user"]);
    let (a1, a2) = (sign_in(&r, "ada", "Phone"), sign_in(&r, "ada", "Tablet"));
    let done = r.a.offboard_user(&root, "ada", "left").unwrap();
    assert_eq!(done["revoked_devices"], 2);
    assert_eq!(done["revoked_apps"], 2, "each is also a grant");
    assert_eq!(done["revoked_sessions"], 0);
    assert!(ended(&r, &a1) && ended(&r, &a2));
    assert_eq!(device_events(&r, "ada").len(), 2);
    assert!(device_events(&r, "ada")[0].contains("ended by user_offboard"));
    r.a.set_user_disabled(&root, "ada", false).unwrap();
    assert!(r.a.resolve(&a1.access).is_none() && state(&r, &a1) == "revoked");
    // Offboarding again changes nothing and finds nothing more to end.
    let again = r.a.offboard_user(&root, "ada", "").unwrap();
    assert_eq!(again["revoked_devices"], 0);
    assert_eq!(device_events(&r, "ada").len(), 2);
}

#[test]
fn a_store_that_refuses_the_devices_of_a_new_password_fails_the_change_as_partial_and_the_repeat_finishes()
 {
    use obop_storage::testing::Mode;
    let (r, flaky) = crate::admin_tests::flaky_rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (mine, p) = session(&r.a, "ada");
    let app = sign_in(&r, "ada", "Phone");
    flaky.refuse_model(dev::MODEL, Mode::Reads);
    let (done, out) = crate::admin_tests::request(|| {
        r.a.change_password(&p, &mine, PW, "a brand new password", None, None)
    });
    flaky.allow_models();
    assert!(matches!(done, Err(MfaError::Unavailable(_))), "{done:?}");
    assert!(out.wrote && out.partial, "{out:?}");
    // The device is still in force: it is told, never reported as ended. The token also reads
    // the device, so a store that refuses it would not let the token through either.
    assert!(state(&r, &app) == "active");
    // The repeat needs the old password, which no longer works: the person signs in with the new
    // one and signs everything out, and that ends the device.
    let (t2, p2) = session_with(&r, "ada", "a brand new password");
    r.a.revoke_sessions(&p2, &t2, true).unwrap();
    assert!(ended(&r, &app));
}

fn session_with(r: &Rig, user: &str, password: &str) -> (String, Principal) {
    let LoginOutcome::Session { token, .. } = r.a.login_step(user, password).unwrap() else {
        panic!("expected a session")
    };
    let p = r.a.resolve(&token).unwrap();
    (token, p)
}

// ---- who the log names, what is stored, and the walks of D74 -----------------------------------

#[test]
fn the_log_names_the_administrator_who_ended_the_devices_and_the_person_who_did_it_to_themselves() {
    let r = rig();
    user_with(&r.a, "root", &[ADMIN_GROUP]);
    let (_, root) = session(&r.a, "root");
    user_with(&r.a, "ada", &["base.user"]);
    user_with(&r.a, "bob", &["base.user"]);
    let actors = |r: &Rig, user: &str| -> Vec<String> {
        r.b.list(EVENT_TABLE)
            .unwrap()
            .into_iter()
            .filter(|(_, f)| f["kind"] == "device_revoked" && f["user"] == user)
            .map(|(_, f)| f["actor"].as_str().unwrap().to_string())
            .collect()
    };
    sign_in(&r, "ada", "Phone");
    r.a.offboard_user(&root, "ada", "left").unwrap();
    assert_eq!(actors(&r, "ada"), ["root"], "the administrator did it");
    sign_in(&r, "bob", "Phone");
    enrol(&r.a, "bob");
    r.a.admin_reset(&root, "bob").unwrap();
    assert_eq!(actors(&r, "bob"), ["root"]);
    user_with(&r.a, "cy", &["base.user"]);
    let (mine, p) = session(&r.a, "cy");
    sign_in(&r, "cy", "Phone");
    r.a.change_password(&p, &mine, PW, "a brand new password", None, None)
        .unwrap();
    assert_eq!(actors(&r, "cy"), ["cy"], "the person did it to themselves");
}

#[test]
fn a_mobile_token_never_carries_a_group_a_phone_may_not_have_whatever_the_record_says() {
    let r = rig();
    user_with(
        &r.a,
        "ada",
        &[
            "base.user",
            "platform.admin",
            "mileage.admin",
            "expenses.approver",
        ],
    );
    let app = sign_in(&r, "ada", "Phone");
    // A record that was written by hand, or by an older build, with more than a phone may hold.
    let (id, _) =
        r.b.list("auth_token")
            .unwrap()
            .into_iter()
            .find(|(_, t)| t.get("mobile") == Some(&json!(true)))
            .unwrap();
    let mut patch = Fields::new();
    patch.insert(
        "scopes".into(),
        json!([
            "base.user",
            "platform.admin",
            "mileage.admin",
            "expenses.approver"
        ]),
    );
    r.b.update("auth_token", &id, &patch).unwrap();
    let p = r.a.resolve(&app.access).unwrap();
    assert!(p.allows("base.user"));
    for never in ["platform.admin", "mileage.admin", "expenses.approver"] {
        assert!(!p.allows(never), "{never}");
    }
    // Without the device it is bound to, it is nobody.
    let mut gone = Fields::new();
    gone.insert("device".into(), J::Null);
    r.b.update("auth_token", &id, &gone).unwrap();
    assert!(r.a.resolve(&app.access).is_none());
}

#[derive(Clone, Copy, Debug)]
enum Change {
    NewPassword,
    SignOutEverywhere,
    Disable,
    Offboard,
}

fn make_change(
    r: &Rig,
    what: Change,
    p: &Principal,
    mine: &str,
    root: &Principal,
) -> Result<(), MfaError> {
    match what {
        Change::NewPassword => {
            r.a.change_password(p, mine, PW, "a brand new password", None, None)
        }
        Change::SignOutEverywhere => r.a.revoke_sessions(p, mine, true).map(|_| ()),
        Change::Disable => r.a.set_user_disabled(root, "ada", true),
        Change::Offboard => r.a.offboard_user(root, "ada", "left").map(|_| ()),
    }
}

/// How many entries of the log name this device as ended.
fn logged_as_ended(r: &Rig, a: &App) -> usize {
    device_events(r, "ada")
        .iter()
        .filter(|d| d.contains(&format!("[{}]", a.device)))
        .count()
}

/// A change of the credentials of a person who has two devices, with the store refusing from one
/// position on (for `stays` calls), at every position in turn up to the one after the last call the
/// change makes. Wherever the refusal falls: the answer is true (done, or refused with the work
/// that is left to repeat), a device that is marked ended is in the log exactly once, and the
/// repeat ends the rest without logging anything twice.
fn walk(what: Change, stays: i64) {
    let mut reached_the_end = false;
    let mut refused_at_least_once = 0;
    let mut met_the_refusal = 0;
    for skip in 0..400 {
        let (r, flaky) = crate::admin_tests::flaky_rig();
        user_with(&r.a, "root", &[ADMIN_GROUP]);
        user_with(&r.a, "ada", &["base.user"]);
        let (_, root) = session(&r.a, "root");
        let (mine, p) = session(&r.a, "ada");
        let apps = [sign_in(&r, "ada", "Phone"), sign_in(&r, "ada", "Tablet")];
        if stays == 1 {
            flaky.refuse_call_after(skip);
        } else {
            flaky.refuse_calls_after(skip, stays);
        }
        let (done, out) = crate::admin_tests::request(|| make_change(&r, what, &p, &mine, &root));
        let refused = flaky.refused();
        met_the_refusal += i32::from(refused > 0);
        flaky.allow_models();
        let at = format!("{what:?}, refused from call {skip} for {stays}: {done:?} {out:?}");
        // Whatever is ended is in the log, at once. An entry may be there for a device whose end
        // the store then refused (the repeat finishes it): the entry comes first, so that one
        // that is lost never leaves a device ended and unlogged. One request never writes an
        // entry twice.
        for a in &apps {
            let logged = logged_as_ended(&r, a);
            assert!(logged >= usize::from(state(&r, a) == "revoked"), "{at}");
            assert!(logged <= 1, "{at}");
        }
        match &done {
            Ok(()) => assert!(
                apps.iter().all(|a| ended(&r, a)),
                "said done, so it is: {at}"
            ),
            Err(e) => {
                refused_at_least_once += 1;
                // Never "done" and never a verdict about the change: a refusal of the store is
                // what the caller is told, and the request knows it met one.
                assert!(out.fault.is_some(), "{e:?}: {at}");
                // Finish it. After a new password the old one no longer works: the person signs in
                // with the new one and signs everything out, which ends what is left.
                match what {
                    Change::NewPassword if out.wrote => {
                        let (t2, p2) = session_with(&r, "ada", "a brand new password");
                        r.a.revoke_sessions(&p2, &t2, true).unwrap();
                    }
                    _ => make_change(&r, what, &p, &mine, &root).unwrap(),
                }
            }
        }
        for a in &apps {
            assert!(ended(&r, a), "after the repeat: {at}");
            // Once, however many tries of one request; twice at most when a request that could not
            // finish was repeated.
            assert!((1..=2).contains(&logged_as_ended(&r, a)), "{at}");
        }
        if refused == 0 {
            reached_the_end = true;
            break;
        }
    }
    assert!(
        reached_the_end,
        "{what:?}: the walk went past the last call of the change"
    );
    assert!(
        met_the_refusal >= 5,
        "{what:?}: the walk met the refusal in {met_the_refusal} places"
    );
    if stays > 1 {
        assert!(
            refused_at_least_once >= 1,
            "{what:?}: a store that stays refused made the change fail {refused_at_least_once} times"
        );
    }
}

#[test]
fn ending_the_devices_of_a_new_password_survives_a_refusal_at_every_call() {
    walk(Change::NewPassword, 1);
    walk(Change::NewPassword, 6);
}

#[test]
fn ending_the_devices_of_a_sign_out_everywhere_survives_a_refusal_at_every_call() {
    walk(Change::SignOutEverywhere, 1);
    walk(Change::SignOutEverywhere, 6);
}

#[test]
fn ending_the_devices_of_a_disable_survives_a_refusal_at_every_call() {
    walk(Change::Disable, 1);
    walk(Change::Disable, 6);
}

#[test]
fn ending_the_devices_of_an_offboarding_survives_a_refusal_at_every_call() {
    walk(Change::Offboard, 1);
    walk(Change::Offboard, 6);
}

#[test]
fn the_entry_of_a_device_that_was_ended_is_kept_when_the_next_step_is_refused_for_good() {
    use obop_storage::testing::Mode;
    // The sessions step is refused for longer than it is tried again, after the devices were
    // ended: the devices are in the log all the same (they were once written after that step and
    // lost with it), and the repeat does not write them a second time.
    let (r, flaky) = crate::admin_tests::flaky_rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (mine, p) = session(&r.a, "ada");
    let (a1, a2) = (sign_in(&r, "ada", "Phone"), sign_in(&r, "ada", "Tablet"));
    flaky.refuse_model("auth_token", Mode::Reads);
    let (done, _) = crate::admin_tests::request(|| r.a.revoke_sessions(&p, &mine, true));
    flaky.allow_models();
    assert!(done.is_err(), "{done:?}");
    assert_eq!(state(&r, &a1), "revoked");
    assert_eq!(state(&r, &a2), "revoked");
    assert_eq!(
        device_events(&r, "ada").len(),
        2,
        "{:?}",
        device_events(&r, "ada")
    );
    r.a.revoke_sessions(&p, &mine, true).unwrap();
    assert_eq!(device_events(&r, "ada").len(), 2, "not twice");
}

#[test]
fn a_row_of_another_table_with_the_same_name_is_not_ended_as_a_device() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    // A record of a package that was installed before the model was built in: no grant, no
    // registration time.
    r.b.insert(
        dev::MODEL,
        "theirs",
        &fields(json!({"user": "ada", "state": "active", "name": "Not a phone"})),
    )
    .unwrap();
    let (mine, p) = session(&r.a, "ada");
    let app = sign_in(&r, "ada", "Phone");
    r.a.change_password(&p, &mine, PW, "a brand new password", None, None)
        .unwrap();
    assert!(ended(&r, &app));
    let theirs = r.b.get(dev::MODEL, "theirs").unwrap().unwrap();
    assert_eq!(theirs["state"], "active", "{theirs:?}");
    assert_eq!(device_events(&r, "ada").len(), 1);
}
