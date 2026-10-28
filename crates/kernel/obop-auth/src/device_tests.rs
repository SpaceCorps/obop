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
