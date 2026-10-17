use super::*;
use crate::mfa::EVENT_TABLE;
use obop_commands::{ADMIN_GROUP, Authenticator};
use obop_storage::Backend;

const PW: &str = "correct horse battery";
const T0: u64 = 1_700_000_010;

struct Rig {
    a: Auth,
    b: Arc<dyn Backend>,
}

fn rig_with(config: AuthConfig) -> Rig {
    let b = obop_storage::open("memory:").unwrap();
    let a = Auth::with_test_config(b.clone(), config);
    a.set_clock_for_test(T0);
    Rig { a, b }
}

fn rig() -> Rig {
    rig_with(AuthConfig::insecure_dev())
}

fn enforcing() -> AuthConfig {
    AuthConfig {
        default_require_admins: true,
        ..AuthConfig::insecure_dev()
    }
}

fn groups(g: &[&str]) -> Vec<String> {
    g.iter().map(|s| s.to_string()).collect()
}

fn session(a: &Auth, user: &str) -> (String, Principal) {
    let LoginOutcome::Session { token, .. } = a.login_step(user, PW).unwrap() else {
        panic!("expected a session")
    };
    let p = a.resolve(&token).unwrap();
    (token, p)
}

/// Enrol `user` and return (secret bytes, recovery codes). The clock is left at `T0`.
fn enrol(a: &Auth, user: &str) -> (Vec<u8>, Vec<String>) {
    let (token, p) = session(a, user);
    let begun = a.begin(&p).unwrap();
    let secret = totp::base32_decode(begun["secret"].as_str().unwrap()).unwrap();
    let code = totp::code_for_step(&secret, totp::step_at(a.now()));
    let done = a.confirm(&p, &token, &code).unwrap();
    let codes = done["recovery_codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_string())
        .collect();
    (secret, codes)
}

fn mfa_token(a: &Auth, user: &str) -> String {
    match a.login_step(user, PW).unwrap() {
        LoginOutcome::MfaRequired { mfa_token } => mfa_token,
        other => panic!("expected the second step, got {other:?}"),
    }
}

fn code_at(secret: &[u8], secs: u64) -> String {
    totp::code_for_step(secret, totp::step_at(secs))
}

fn user_with(a: &Auth, name: &str, g: &[&str]) {
    a.create_user(name, PW, &groups(g)).unwrap();
}

#[test]
fn enrolment_is_two_steps_and_nothing_is_active_until_confirmed() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (token, p) = session(&r.a, "ada");
    let begun = r.a.begin(&p).unwrap();
    let secret_b32 = begun["secret"].as_str().unwrap();
    assert!(
        begun["uri"]
            .as_str()
            .unwrap()
            .starts_with("otpauth://totp/obop:ada?secret=")
    );
    assert!(begun["uri"].as_str().unwrap().contains(secret_b32));
    assert_eq!(
        totp::base32_decode(secret_b32).unwrap().len(),
        totp::SECRET_LEN
    );
    // Still a plain password sign-in.
    assert!(matches!(
        r.a.login_step("ada", PW).unwrap(),
        LoginOutcome::Session { .. }
    ));
    let st = r.a.status(&p).unwrap();
    assert_eq!(
        (st["enabled"].clone(), st["pending"].clone()),
        (json!(false), json!(true))
    );
    // A wrong code does not activate it.
    assert!(matches!(
        r.a.confirm(&p, &token, "000000"),
        Err(MfaError::Denied(_))
    ));
    assert!(!r.a.mfa_enabled("ada"));
    // The right one does, and returns ten recovery codes once.
    let secret = totp::base32_decode(secret_b32).unwrap();
    let done = r.a.confirm(&p, &token, &code_at(&secret, T0)).unwrap();
    assert_eq!(
        done["recovery_codes"].as_array().unwrap().len(),
        RECOVERY_CODES_FOR_TEST
    );
    assert!(r.a.mfa_enabled("ada"));
    assert!(
        matches!(r.a.begin(&p), Err(MfaError::Invalid(_))),
        "already on"
    );
    assert_eq!(r.a.status(&p).unwrap()["recovery_remaining"], json!(10));
}

const RECOVERY_CODES_FOR_TEST: usize = 10;

#[test]
fn sign_in_needs_the_second_step_and_the_token_is_not_a_session() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (secret, _) = enrol(&r.a, "ada");
    r.a.set_clock_for_test(T0 + 60);
    // The password alone never yields a session, and the legacy one-step login refuses.
    assert!(r.a.login("ada", PW).is_err());
    let t = mfa_token(&r.a, "ada");
    assert!(
        r.a.resolve(&t).is_none(),
        "an mfa_token is not a bearer token"
    );
    let out =
        r.a.login_mfa(&t, Some(&code_at(&secret, T0 + 60)), None)
            .unwrap();
    let LoginOutcome::Session {
        token,
        mfa_enrolment_required,
    } = out
    else {
        panic!()
    };
    assert!(!mfa_enrolment_required);
    assert!(r.a.resolve(&token).unwrap().allows("base.user"));
    // Single purpose, single use.
    assert!(
        r.a.login_mfa(&t, Some(&code_at(&secret, T0 + 90)), None)
            .is_err()
    );
    // The token is stored hashed.
    assert!(r.b.get("auth_mfa_token", &t).unwrap().is_none());
}

#[test]
fn a_code_is_accepted_once_and_older_ones_with_it() {
    let r = rig();
    user_with(&r.a, "ada", &[]);
    let (secret, _) = enrol(&r.a, "ada");
    // Enrolment consumed the step of T0: the same code cannot sign in.
    let t = mfa_token(&r.a, "ada");
    assert!(matches!(
        r.a.login_mfa(&t, Some(&code_at(&secret, T0)), None),
        Err(MfaError::Denied(_))
    ));
    // The next step works once...
    let c = code_at(&secret, T0 + 30);
    r.a.set_clock_for_test(T0 + 30);
    let t = mfa_token(&r.a, "ada");
    assert!(r.a.login_mfa(&t, Some(&c), None).is_ok());
    // ...and never again, even though the window still covers it.
    let t = mfa_token(&r.a, "ada");
    assert!(matches!(
        r.a.login_mfa(&t, Some(&c), None),
        Err(MfaError::Denied(_))
    ));
}

#[test]
fn the_secret_is_sealed_at_rest_and_bound_to_the_user() {
    let r = rig();
    user_with(&r.a, "ada", &[]);
    user_with(&r.a, "grace", &[]);
    let (secret, _) = enrol(&r.a, "ada");
    let rec = r.b.get("auth_user", "ada").unwrap().unwrap();
    let sealed = rec["totp_secret"].as_str().unwrap().to_string();
    assert!(sealed.starts_with("v1."));
    assert!(!sealed.contains(&totp::base32_encode(&secret)));
    assert!(
        !serde_json::to_string(&rec)
            .unwrap()
            .contains(&totp::base32_encode(&secret))
    );
    // Copying the sealed secret to another user does not open.
    let mut p = Fields::new();
    p.insert("totp_secret".into(), J::from(sealed.clone()));
    p.insert("mfa_enabled".into(), J::Bool(true));
    r.b.update("auth_user", "grace", &p).unwrap();
    let t = mfa_token(&r.a, "grace");
    assert!(matches!(
        r.a.login_mfa(&t, Some("123456"), None),
        Err(MfaError::Unavailable(_))
    ));
    // A tampered record does not open either.
    let mut bad = Fields::new();
    let mut flipped = sealed.into_bytes();
    let n = flipped.len() - 1;
    flipped[n] = if flipped[n] == b'a' { b'b' } else { b'a' };
    bad.insert(
        "totp_secret".into(),
        J::from(String::from_utf8(flipped).unwrap()),
    );
    r.b.update("auth_user", "ada", &bad).unwrap();
    r.a.set_clock_for_test(T0 + 60);
    let t = mfa_token(&r.a, "ada");
    assert!(matches!(
        r.a.login_mfa(&t, Some(&code_at(&secret, T0 + 60)), None),
        Err(MfaError::Unavailable(_))
    ));
}

#[test]
fn without_a_server_key_two_factor_cannot_be_enabled() {
    let r = rig_with(AuthConfig {
        secret_key: None,
        ..enforcing()
    });
    user_with(&r.a, "root", &[ADMIN_GROUP]);
    let LoginOutcome::Session {
        token,
        mfa_enrolment_required,
    } = r.a.login_step("root", PW).unwrap()
    else {
        panic!()
    };
    // No key: the policy cannot lock the admin out of an instance that cannot enrol.
    assert!(!mfa_enrolment_required);
    let p = r.a.resolve(&token).unwrap();
    assert!(
        matches!(r.a.begin(&p), Err(MfaError::Unavailable(m)) if m.contains("OBOP_SECRET_KEY"))
    );
    assert_eq!(r.a.status(&p).unwrap()["available"], json!(false));
}

#[test]
fn recovery_codes_are_single_use_and_stored_hashed() {
    let r = rig();
    user_with(&r.a, "ada", &[]);
    let (_, codes) = enrol(&r.a, "ada");
    assert_eq!(codes.len(), 10);
    assert!(
        codes
            .iter()
            .all(|c| c.len() == 19 && c.matches('-').count() == 3)
    );
    let stored = serde_json::to_string(&r.b.get("auth_user", "ada").unwrap().unwrap()).unwrap();
    for c in &codes {
        assert!(
            !stored.contains(c.as_str()),
            "plain recovery code in the record"
        );
    }
    let t = mfa_token(&r.a, "ada");
    assert!(
        r.a.login_mfa(&t, None, Some(&codes[0].to_lowercase()))
            .is_ok(),
        "case and dashes are forgiving"
    );
    let t = mfa_token(&r.a, "ada");
    assert!(matches!(
        r.a.login_mfa(&t, None, Some(&codes[0])),
        Err(MfaError::Denied(_))
    ));
    let t = mfa_token(&r.a, "ada");
    assert!(r.a.login_mfa(&t, None, Some(&codes[1])).is_ok());
    let (_, p) = {
        let t = mfa_token(&r.a, "ada");
        let LoginOutcome::Session { token, .. } = r.a.login_mfa(&t, None, Some(&codes[2])).unwrap()
        else {
            panic!()
        };
        (token.clone(), r.a.resolve(&token).unwrap())
    };
    assert_eq!(r.a.status(&p).unwrap()["recovery_remaining"], json!(7));
}

#[test]
fn the_mfa_token_expires_and_survives_only_five_wrong_codes() {
    let r = rig();
    user_with(&r.a, "ada", &[]);
    let (secret, _) = enrol(&r.a, "ada");
    r.a.set_clock_for_test(T0 + 60);
    let t = mfa_token(&r.a, "ada");
    r.a.set_clock_for_test(T0 + 60 + crate::mfa::MFA_TOKEN_SECS + 1);
    assert!(matches!(
        r.a.login_mfa(&t, Some(&code_at(&secret, r.a.now())), None),
        Err(MfaError::Expired)
    ));
    // Four wrong codes leave the token usable; the fifth is the last it gets.
    let now = r.a.now();
    let t = mfa_token(&r.a, "ada");
    for _ in 0..4 {
        assert!(matches!(
            r.a.login_mfa(&t, Some("000000"), None),
            Err(MfaError::Denied(_))
        ));
    }
    // Lift the per-user lockout to see the per-token cap on its own.
    r.a.failures.lock().unwrap().clear();
    assert!(matches!(
        r.a.login_mfa(&t, Some("000000"), None),
        Err(MfaError::Denied(_))
    ));
    r.a.failures.lock().unwrap().clear();
    assert!(
        matches!(
            r.a.login_mfa(&t, Some(&code_at(&secret, now)), None),
            Err(MfaError::Locked(_))
        ),
        "five misses burn the token even for the right code"
    );
}

#[test]
fn wrong_codes_count_toward_the_same_lockout_as_wrong_passwords() {
    let r = rig();
    user_with(&r.a, "ada", &[]);
    let (secret, _) = enrol(&r.a, "ada");
    r.a.set_clock_for_test(T0 + 60);
    // Two wrong passwords and three wrong codes lock the account.
    for _ in 0..2 {
        assert!(r.a.login_step("ada", "wrong password!!").is_err());
    }
    let t = mfa_token(&r.a, "ada");
    for _ in 0..3 {
        assert!(r.a.login_mfa(&t, Some("000000"), None).is_err());
    }
    assert!(r.a.login_step("ada", PW).unwrap_err().contains("too many"));
    assert!(matches!(
        r.a.login_mfa(&t, Some(&code_at(&secret, T0 + 60)), None),
        Err(MfaError::Locked(_))
    ));
}

#[test]
fn admins_without_two_factor_get_a_limited_session_until_they_enrol() {
    let r = rig_with(enforcing());
    user_with(&r.a, "root", &[ADMIN_GROUP]);
    user_with(&r.a, "ada", &["base.user"]);
    // A non-admin is untouched.
    let LoginOutcome::Session {
        mfa_enrolment_required,
        ..
    } = r.a.login_step("ada", PW).unwrap()
    else {
        panic!()
    };
    assert!(!mfa_enrolment_required);
    // The admin is limited: the principal holds nothing, not even `*`.
    let LoginOutcome::Session {
        token,
        mfa_enrolment_required,
    } = r.a.login_step("root", PW).unwrap()
    else {
        panic!()
    };
    assert!(mfa_enrolment_required);
    let p = r.a.resolve(&token).unwrap();
    assert!(p.mfa_limited);
    assert!(!p.allows("*") && !p.allows("base.user") && !p.allows(ADMIN_GROUP));
    // ...but may enrol, and the same session is lifted by the confirmation.
    let begun = r.a.begin(&p).unwrap();
    let secret = totp::base32_decode(begun["secret"].as_str().unwrap()).unwrap();
    r.a.confirm(&p, &token, &code_at(&secret, T0)).unwrap();
    let after = r.a.resolve(&token).unwrap();
    assert!(!after.mfa_limited && after.allows("base.user"));
    // Next sign-in is two-step.
    r.a.set_clock_for_test(T0 + 60);
    assert!(matches!(
        r.a.login_step("root", PW).unwrap(),
        LoginOutcome::MfaRequired { .. }
    ));
}

#[test]
fn the_policy_follows_the_instance_setting_and_can_cover_everyone() {
    let r = rig_with(enforcing());
    user_with(&r.a, "root", &[ADMIN_GROUP]);
    user_with(&r.a, "ada", &["base.user"]);
    let set = |v: J| {
        let mut f = Fields::new();
        f.insert("value".into(), J::from(v.to_string()));
        if !r.b.update("obop_setting", "instance", &f).unwrap() {
            r.b.insert("obop_setting", "instance", &f).unwrap();
        }
    };
    let limited = |u: &str| match r.a.login_step(u, PW).unwrap() {
        LoginOutcome::Session {
            mfa_enrolment_required,
            ..
        } => mfa_enrolment_required,
        _ => false,
    };
    assert!(limited("root") && !limited("ada"));
    set(json!({"security": {"require_mfa_for_admins": false}}));
    assert!(!limited("root"), "a stored value beats the default");
    set(json!({"security": {"require_mfa_for_admins": true, "require_mfa_for_all": true}}));
    assert!(limited("root") && limited("ada"));
    set(json!({"theme": {}}));
    assert!(
        limited("root") && !limited("ada"),
        "no stored value: the default applies"
    );
}

#[test]
fn agents_cannot_enrol_disable_or_reset() {
    let r = rig();
    user_with(&r.a, "root", &[ADMIN_GROUP]);
    user_with(&r.a, "ada", &["base.user"]);
    enrol(&r.a, "ada");
    let (_, human) = session_after_enrol(&r, "root");
    let t =
        r.a.issue_agent_token(&human, &groups(&[ADMIN_GROUP]), "bot")
            .unwrap();
    let agent = r.a.resolve(&t).unwrap();
    assert_eq!(agent.kind, PrincipalKind::Agent);
    assert_eq!(r.a.begin(&agent), Err(MfaError::NotFound));
    assert_eq!(r.a.status(&agent), Err(MfaError::NotFound));
    assert_eq!(r.a.confirm(&agent, &t, "123456"), Err(MfaError::NotFound));
    assert_eq!(
        r.a.disable(&agent, PW, Some("123456"), None),
        Err(MfaError::NotFound)
    );
    assert_eq!(
        r.a.regenerate_recovery(&agent, PW, Some("1"), None),
        Err(MfaError::NotFound)
    );
    assert_eq!(r.a.admin_reset(&agent, "ada"), Err(MfaError::NotFound));
    assert!(r.a.mfa_enabled("ada"));
}

fn session_after_enrol(r: &Rig, user: &str) -> (String, Principal) {
    session(&r.a, user)
}

#[test]
fn disabling_needs_the_password_and_a_code_or_a_recovery_code() {
    let r = rig();
    user_with(&r.a, "ada", &[]);
    let (secret, codes) = enrol(&r.a, "ada");
    r.a.set_clock_for_test(T0 + 60);
    let t = mfa_token(&r.a, "ada");
    let LoginOutcome::Session { token, .. } =
        r.a.login_mfa(&t, Some(&code_at(&secret, T0 + 60)), None)
            .unwrap()
    else {
        panic!()
    };
    let p = r.a.resolve(&token).unwrap();
    let c = code_at(&secret, T0 + 90);
    r.a.set_clock_for_test(T0 + 90);
    assert!(matches!(
        r.a.disable(&p, "wrong password!!", Some(&c), None),
        Err(MfaError::BadPassword)
    ));
    assert!(matches!(
        r.a.disable(&p, PW, Some("000000"), None),
        Err(MfaError::Denied(_))
    ));
    assert!(matches!(
        r.a.disable(&p, PW, None, None),
        Err(MfaError::Invalid(_))
    ));
    assert!(r.a.mfa_enabled("ada"));
    // A recovery code stands in for the code.
    r.a.disable(&p, PW, None, Some(&codes[3])).unwrap();
    assert!(!r.a.mfa_enabled("ada"));
    assert!(matches!(
        r.a.login_step("ada", PW).unwrap(),
        LoginOutcome::Session { .. }
    ));
    let rec = r.b.get("auth_user", "ada").unwrap().unwrap();
    assert!(rec["totp_secret"].is_null() && rec["recovery"].is_null());
}

#[test]
fn regenerating_replaces_all_recovery_codes() {
    let r = rig();
    user_with(&r.a, "ada", &[]);
    let (secret, old) = enrol(&r.a, "ada");
    r.a.set_clock_for_test(T0 + 60);
    let t = mfa_token(&r.a, "ada");
    let LoginOutcome::Session { token, .. } =
        r.a.login_mfa(&t, Some(&code_at(&secret, T0 + 60)), None)
            .unwrap()
    else {
        panic!()
    };
    let p = r.a.resolve(&token).unwrap();
    r.a.set_clock_for_test(T0 + 90);
    let new =
        r.a.regenerate_recovery(&p, PW, Some(&code_at(&secret, T0 + 90)), None)
            .unwrap();
    let new: Vec<String> = new["recovery_codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().into())
        .collect();
    assert_eq!(new.len(), 10);
    let t = mfa_token(&r.a, "ada");
    assert!(
        r.a.login_mfa(&t, None, Some(&old[0])).is_err(),
        "old codes are void"
    );
    let t = mfa_token(&r.a, "ada");
    assert!(r.a.login_mfa(&t, None, Some(&new[0])).is_ok());
}

#[test]
fn an_admin_can_reset_another_user_but_not_themselves() {
    let r = rig();
    user_with(&r.a, "root", &[ADMIN_GROUP]);
    user_with(&r.a, "ada", &["base.user"]);
    user_with(&r.a, "bob", &["base.user"]);
    enrol(&r.a, "ada");
    let (_, admin) = session(&r.a, "root");
    let ada = Principal::user("bob", &["base.user"]);
    // Not an admin.
    assert_eq!(r.a.admin_reset(&ada, "ada"), Err(MfaError::NotFound));
    // Not oneself.
    assert!(matches!(
        r.a.admin_reset(&admin, "root"),
        Err(MfaError::Invalid(_))
    ));
    // Unknown user, user without a factor.
    assert!(matches!(
        r.a.admin_reset(&admin, "ghost"),
        Err(MfaError::Invalid(_))
    ));
    r.a.admin_reset(&admin, "ada").unwrap();
    assert!(!r.a.mfa_enabled("ada"));
    assert!(matches!(
        r.a.admin_reset(&admin, "ada"),
        Err(MfaError::Invalid(_))
    ));
    let ev = events(&r);
    let reset = ev.iter().find(|e| e["kind"] == "reset").unwrap();
    assert_eq!(
        (reset["user"].as_str(), reset["actor"].as_str()),
        (Some("ada"), Some("root"))
    );
}

#[test]
fn the_break_glass_reset_clears_the_factor_revokes_sessions_and_is_audited() {
    let r = rig();
    user_with(&r.a, "ada", &[]);
    let (secret, _) = enrol(&r.a, "ada");
    r.a.set_clock_for_test(T0 + 60);
    let t = mfa_token(&r.a, "ada");
    let LoginOutcome::Session { token, .. } =
        r.a.login_mfa(&t, Some(&code_at(&secret, T0 + 60)), None)
            .unwrap()
    else {
        panic!()
    };
    assert!(r.a.resolve(&token).is_some());
    r.a.operator_reset_mfa("ADA").unwrap();
    assert!(!r.a.mfa_enabled("ada"));
    assert!(r.a.resolve(&token).is_none(), "sessions are revoked");
    assert!(r.a.operator_reset_mfa("ghost").is_err());
    let ev = events(&r);
    let reset = ev.iter().find(|e| e["kind"] == "reset").unwrap();
    assert_eq!(reset["actor"], "operator");
    assert!(matches!(
        r.a.login_step("ada", PW).unwrap(),
        LoginOutcome::Session { .. }
    ));
}

fn events(r: &Rig) -> Vec<Fields> {
    r.b.list(EVENT_TABLE)
        .unwrap()
        .into_iter()
        .map(|(_, f)| f)
        .collect()
}

#[test]
fn the_event_log_records_what_happened_and_never_a_secret() {
    let r = rig();
    user_with(&r.a, "ada", &[]);
    let (_, p) = session(&r.a, "ada");
    let begun = r.a.begin(&p).unwrap();
    let b32 = begun["secret"].as_str().unwrap().to_string();
    let secret = totp::base32_decode(&b32).unwrap();
    // Failed codes: a flood writes a few records, not thousands.
    let (token, _) = session(&r.a, "ada");
    for _ in 0..4 {
        let _ = r.a.confirm(&p, &token, "999999");
    }
    r.a.failures.lock().unwrap().clear();
    let good = code_at(&secret, T0);
    let done = r.a.confirm(&p, &token, &good).unwrap();
    let codes: Vec<String> = done["recovery_codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().into())
        .collect();
    r.a.set_clock_for_test(T0 + 60);
    let t = mfa_token(&r.a, "ada");
    r.a.login_mfa(&t, None, Some(&codes[0])).unwrap();
    let ev = events(&r);
    let kinds: Vec<&str> = ev.iter().map(|e| e["kind"].as_str().unwrap()).collect();
    assert_eq!(
        kinds.iter().filter(|k| **k == "code_failed").count(),
        1,
        "rate limited: {kinds:?}"
    );
    for want in ["enrol_begin", "enrol_confirm", "recovery_used"] {
        assert!(kinds.contains(&want), "{want} missing in {kinds:?}");
    }
    let all = serde_json::to_string(&ev).unwrap();
    for secret in [
        b32.as_str(),
        good.as_str(),
        "999999",
        codes[0].as_str(),
        begun["uri"].as_str().unwrap(),
    ] {
        assert!(!all.contains(secret), "{secret} leaked into the event log");
    }
    assert!(ev.iter().all(|e| e["at"].as_str().unwrap().ends_with('Z')));
}

#[test]
fn timestamps_are_rfc3339() {
    assert_eq!(crate::mfa::rfc3339(0), "1970-01-01T00:00:00Z");
    assert_eq!(crate::mfa::rfc3339(1_700_000_010), "2023-11-14T22:13:30Z");
    assert_eq!(crate::mfa::rfc3339(951_782_400), "2000-02-29T00:00:00Z");
}
