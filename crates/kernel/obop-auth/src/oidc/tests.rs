//! Sign-in with Microsoft at the level of the identity provider (D91): configuration, bindings, the
//! break-glass rule, the one-time code and what the session is for each kind of account. The token
//! checks themselves are in `verify_tests`; the browser round trip is in `obop-http` and the
//! tests of `crates/obop`.

use super::soft::StandIn;
use super::verify::AUTHORITY;
use super::{BREAK_GLASS_MIN, CODE_SECS, LINK_TABLE, PROVIDER_TABLE, code_key, link_key};
use crate::mfa::EVENT_TABLE;
use crate::mfa_tests::{
    PW, Rig, T0, code_at, enforcing, enrol, mfa_token, rig, rig_with, session, user_with,
};
use crate::passkey_tests::{ORIGIN, add, ctx, pw};
use crate::webauthn::soft::SoftAuthenticator;
use crate::{Auth, AuthConfig, SESSION_SECS, bump_epoch, token_hash};
use obop_commands::{
    ADMIN_GROUP, AccountsApi, Authenticator, LoginOutcome, MfaApi, MfaError, PasskeyApi, Principal,
    SsoError, SsoIdentity,
};
use obop_storage::Fields;
use serde_json::{Value as J, json};

const TENANT: &str = "11111111-2222-3333-4444-555555555555";
const OTHER_TENANT: &str = "99999999-2222-3333-4444-555555555555";
const CLIENT: &str = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
const ADA_OID: &str = "0f0f0f0f-1111-2222-3333-444444444444";
const BOB_OID: &str = "0b0b0b0b-1111-2222-3333-444444444444";
const ROOT2_OID: &str = "0c0c0c0c-1111-2222-3333-444444444444";
const NONCE: &str = "nonce-of-this-sign-in";
const BINDER: &str = "browser-secret-of-this-sign-in-0123456789";
const SECRET: &str = "client-secret-value-9f3a";

fn sso(r: &Rig) -> &dyn obop_commands::SsoApi {
    r.a.sso().expect("the provider of the instance")
}

fn admin(name: &str) -> Principal {
    Principal::user(name, &[ADMIN_GROUP])
}

/// Three administrators with an authenticator app (the break-glass ones), `ada` bound to `ADA_OID`
/// and a provider that is configured and enabled.
fn world_with(config: AuthConfig) -> Rig {
    let r = rig_with(config);
    for n in ["root1", "root2", "root3"] {
        user_with(&r.a, n, &[ADMIN_GROUP]);
        enrol(&r.a, n);
    }
    user_with(&r.a, "ada", &["base.user"]);
    user_with(&r.a, "bob", &["base.user"]);
    r.a.operator_sso_configure(TENANT, CLIENT, Some(SECRET), Some(true))
        .unwrap();
    r.a.operator_sso_bind("ada", ADA_OID).unwrap();
    r
}

fn world() -> Rig {
    world_with(AuthConfig::insecure_dev())
}

/// What the verifier says about a token the stand-in signed for `oid`, with the claims edited.
fn identity_with(r: &Rig, oid: &str, edit: impl FnOnce(&mut J)) -> SsoIdentity {
    let s = StandIn::new();
    let mut c = StandIn::claims(AUTHORITY, TENANT, CLIENT, NONCE, oid, r.a.now());
    edit(&mut c);
    sso(r)
        .verify(&s.token(&c), s.jwks().as_bytes(), NONCE)
        .expect("a good token")
}

fn identity(r: &Rig, oid: &str) -> SsoIdentity {
    identity_with(r, oid, |_| {})
}

fn without_mfa(c: &mut J) {
    c["amr"] = json!(["pwd"]);
}

fn events(r: &Rig, kind: &str) -> Vec<String> {
    r.b.list(EVENT_TABLE)
        .unwrap()
        .into_iter()
        .filter(|(_, f)| f["kind"] == kind)
        .map(|(_, f)| f["detail"].as_str().unwrap_or("").to_string())
        .collect()
}

fn session_of(out: LoginOutcome) -> (String, bool) {
    match out {
        LoginOutcome::Session {
            token,
            mfa_enrolment_required,
            ..
        } => (token, mfa_enrolment_required),
        other => panic!("expected a session, got {other:?}"),
    }
}

fn invalid<T: std::fmt::Debug>(r: Result<T, SsoError>) -> String {
    match r {
        Err(SsoError::Invalid(m)) => m,
        other => panic!("expected a refusal of the input, got {other:?}"),
    }
}

/// The person signs in once with the button on their binding, as the administrator asks before
/// making the account Microsoft-only. (An account with an obop second factor is then asked for
/// it; nothing is finished, and the binding still records the sign-in.)
fn signed_in_once(r: &Rig, oid: &str) {
    let code = sso(r).begin(&identity(r, oid), BINDER).unwrap();
    sso(r).exchange(&code, BINDER).unwrap();
}

fn sso_only(r: &Rig, user: &str, on: bool) -> Result<J, SsoError> {
    sso(r).set_sso_only(&admin("root1"), user, on)
}

#[test]
fn the_configuration_is_checked_sealed_and_never_shown() {
    let r = rig();
    let root = admin("root");
    let cfg = |tenant: &str, client: &str, secret: Option<&str>| {
        sso(&r).configure(&root, tenant, client, secret, Some(true))
    };
    // The tenant is a GUID; the multi-tenant authorities are not a tenant.
    for t in [
        "common",
        "organizations",
        "consumers",
        "contoso.onmicrosoft.com",
        "",
    ] {
        invalid(cfg(t, CLIENT, Some(SECRET)));
    }
    assert!(invalid(cfg("common", CLIENT, Some(SECRET))).contains("multi-tenant"));
    invalid(cfg(TENANT, "not-a-guid", Some(SECRET)));
    // A secret is needed the first time, and it is a plain line of text.
    assert!(invalid(cfg(TENANT, CLIENT, None)).contains("client secret"));
    invalid(cfg(TENANT, CLIENT, Some("")));
    invalid(cfg(TENANT, CLIENT, Some("two\nlines")));
    invalid(cfg(TENANT, CLIENT, Some(&"x".repeat(513))));
    assert!(sso(&r).provider().unwrap().is_none() && !sso(&r).offered());

    let v = cfg(&TENANT.to_uppercase(), &CLIENT.to_uppercase(), Some(SECRET)).unwrap();
    assert_eq!(
        (v.tenant.as_str(), v.client_id.as_str(), v.enabled),
        (TENANT, CLIENT, true)
    );
    assert!(v.secret_set && sso(&r).offered());
    // Sealed at rest: the stored value is not the secret.
    let stored = r.b.get(PROVIDER_TABLE, "provider").unwrap().unwrap();
    let sealed = stored["client_secret"].as_str().unwrap();
    assert!(!sealed.contains(SECRET) && sealed != SECRET);
    // The leg opens it for the one call that needs it, and does not print it.
    let leg = sso(&r).leg().unwrap().unwrap();
    assert_eq!(leg.client_secret, SECRET);
    assert_eq!(
        (leg.authority.as_str(), leg.tenant.as_str()),
        (AUTHORITY, TENANT)
    );
    assert!(!format!("{leg:?}").contains(SECRET));
    // No secret in the answer or in the log.
    assert!(!format!("{:?}", sso(&r).provider().unwrap()).contains(SECRET));
    // A change without a new secret keeps the stored one; switched off it offers nothing.
    cfg(TENANT, CLIENT, None).unwrap();
    assert_eq!(sso(&r).leg().unwrap().unwrap().client_secret, SECRET);
    sso(&r)
        .configure(&root, TENANT, CLIENT, None, Some(false))
        .unwrap();
    assert!(!sso(&r).offered() && sso(&r).leg().unwrap().is_none());
    for (_, f) in r.b.list(EVENT_TABLE).unwrap() {
        assert!(
            !J::Object(f).to_string().contains(SECRET),
            "a secret in the log"
        );
    }
    assert_eq!(events(&r, "oidc_provider_configured").len(), 3);
}

#[test]
fn only_a_whole_administrator_session_configures_and_binds() {
    let r = world();
    let mut limited = admin("root1");
    limited.mfa_limited = true;
    let agent = Principal::agent("root1", &[ADMIN_GROUP], &[ADMIN_GROUP]);
    for p in [
        Principal::user("ada", &["base.user"]),
        limited,
        agent,
        Principal::user("sales", &["sales.manager"]),
    ] {
        let s = sso(&r);
        assert_eq!(
            s.configure(&p, TENANT, CLIENT, None, Some(true))
                .unwrap_err(),
            SsoError::NotFound
        );
        assert_eq!(s.clear(&p).unwrap_err(), SsoError::NotFound);
        assert_eq!(s.bind(&p, "bob", BOB_OID).unwrap_err(), SsoError::NotFound);
        assert_eq!(s.unbind(&p, "ada").unwrap_err(), SsoError::NotFound);
        assert_eq!(
            s.set_sso_only(&p, "ada", true).unwrap_err(),
            SsoError::NotFound
        );
        assert_eq!(s.bindings(&p).unwrap_err(), SsoError::NotFound);
    }
    // An approved proposal of an agent runs as `root1/agent`, a person's name with a suffix.
    assert!(sso(&r).bind(&admin("root1/agent"), "bob", BOB_OID).is_ok());
    assert_eq!(events(&r, "oidc_bind").len(), 2);
}

#[test]
fn a_secret_cannot_be_sealed_without_the_server_key() {
    let r = rig_with(AuthConfig {
        keys: obop_keys::KeyRing::default(),
        ..AuthConfig::insecure_dev()
    });
    assert!(matches!(
        sso(&r).configure(&admin("root"), TENANT, CLIENT, Some(SECRET), Some(true)),
        Err(SsoError::Unavailable(m)) if m.contains("OBOP_SECRET_KEY")
    ));
    assert!(!sso(&r).offered());
}

#[test]
fn one_identity_one_account_and_the_binding_is_the_object_id() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    user_with(&r.a, "bob", &["base.user"]);
    // Nothing binds before there is a provider.
    assert!(invalid(sso(&r).bind(&admin("root"), "ada", ADA_OID)).contains("configure"));
    r.a.operator_sso_configure(TENANT, CLIENT, Some(SECRET), Some(true))
        .unwrap();
    let a = admin("root");
    // Not an object id: a name, an address, nothing.
    for bad in [
        "ada@example.test",
        "Ada Lovelace",
        "",
        "0f0f0f0f-1111-2222-3333-44444444444",
    ] {
        assert!(invalid(sso(&r).bind(&a, "ada", bad)).contains("object id"));
    }
    invalid(sso(&r).bind(&a, "ghost", ADA_OID));
    let done = sso(&r).bind(&a, "Ada", &ADA_OID.to_uppercase()).unwrap();
    assert_eq!(done["oid"], ADA_OID);
    // The record is keyed by the hash of issuer, tenant and object id, and holds no name.
    let issuer = format!("{AUTHORITY}/{TENANT}/v2.0");
    let rec =
        r.b.get(LINK_TABLE, &link_key(&issuer, TENANT, ADA_OID))
            .unwrap()
            .unwrap();
    assert_eq!(
        (
            rec["user"].as_str(),
            rec["oid"].as_str(),
            rec["tid"].as_str()
        ),
        (Some("ada"), Some(ADA_OID), Some(TENANT))
    );
    assert!(
        !J::Object(rec)
            .to_string()
            .to_lowercase()
            .contains("example.test")
    );
    // One account, one identity; one identity, one account.
    assert!(invalid(sso(&r).bind(&a, "ada", BOB_OID)).contains("bound"));
    assert!(invalid(sso(&r).bind(&a, "bob", ADA_OID)).contains("another account"));
    assert!(invalid(sso(&r).bind(&a, "bob", &ADA_OID.to_uppercase())).contains("another account"));
    // A disabled account is not bound.
    r.a.operator_set_disabled("bob", true).unwrap();
    assert!(invalid(sso(&r).bind(&a, "bob", BOB_OID)).contains("disabled"));
    // The key depends on every part, and is stable.
    let k = link_key(&issuer, TENANT, ADA_OID);
    assert_eq!(k, link_key(&issuer, TENANT, ADA_OID));
    assert_ne!(k, link_key(&issuer, TENANT, BOB_OID));
    assert_ne!(k, link_key(&issuer, OTHER_TENANT, ADA_OID));
    assert_ne!(k, link_key("https://other.example", TENANT, ADA_OID));
    // Unbinding ends the way in; the repeat has nothing to do.
    let id = identity(&r, ADA_OID);
    sso(&r).unbind(&a, "ada").unwrap();
    assert!(invalid(sso(&r).unbind(&a, "ada")).contains("not bound"));
    assert!(matches!(
        sso(&r).begin(&id, BINDER),
        Err(SsoError::Refused(_))
    ));
}

#[test]
fn the_tenant_cannot_change_under_bound_accounts() {
    let r = world();
    let root = admin("root1");
    assert!(
        invalid(sso(&r).configure(&root, OTHER_TENANT, CLIENT, None, Some(true)))
            .contains("unbind")
    );
    // The same tenant, a new client id and a new secret are fine.
    let other_client = "bbbbbbbb-bbbb-cccc-dddd-eeeeeeeeeeee";
    sso(&r)
        .configure(
            &root,
            TENANT,
            other_client,
            Some("another-secret"),
            Some(true),
        )
        .unwrap();
    assert_eq!(
        sso(&r).leg().unwrap().unwrap().client_secret,
        "another-secret"
    );
}

#[test]
fn a_bound_identity_gets_a_one_time_code_that_is_spent_once_and_expires() {
    let r = world();
    let id = identity(&r, ADA_OID);
    let code = sso(&r).begin(&id, BINDER).unwrap();
    assert!(code.starts_with("obopsso_") && code.len() > 40);
    // Stored hashed, with an expiry; the code itself is not a key.
    assert!(r.b.kv_get(&format!("auth.sso:{code}")).unwrap().is_none());
    assert!(r.b.kv_get(&code_key(&code)).unwrap().is_some());
    let (token, limited) = session_of(sso(&r).exchange(&code, BINDER).unwrap().1);
    assert!(!limited);
    assert_eq!(r.a.resolve(&token).unwrap().id, "ada");
    // The session is the one a password gives: bearer, the same lifetime.
    let stored = r.b.get("auth_token", &token_hash(&token)).unwrap().unwrap();
    assert_eq!(stored["expires_at"], json!(T0 + SESSION_SECS));
    assert_eq!(stored["kind"], "user");
    // Spent.
    assert!(matches!(
        sso(&r).exchange(&code, BINDER),
        Err(MfaError::Expired)
    ));
    assert!(r.b.kv_get(&code_key(&code)).unwrap().is_none());
    // A code nobody issued, and one that waited too long.
    assert!(matches!(
        sso(&r).exchange("obopsso_made_up", BINDER),
        Err(MfaError::Expired)
    ));
    let late = sso(&r).begin(&id, BINDER).unwrap();
    r.a.set_clock_for_test(T0 + CODE_SECS + 1);
    assert!(matches!(
        sso(&r).exchange(&late, BINDER),
        Err(MfaError::Expired)
    ));
    // Two sign-ins get two codes.
    r.a.set_clock_for_test(T0);
    assert_ne!(
        sso(&r).begin(&id, BINDER).unwrap(),
        sso(&r).begin(&id, BINDER).unwrap()
    );
    assert_eq!(events(&r, "oidc_login").len(), 1);
    // Neither the code nor the secret is in the log.
    for (_, f) in r.b.list(EVENT_TABLE).unwrap() {
        let line = J::Object(f).to_string();
        assert!(!line.contains(&code) && !line.contains(SECRET) && !line.contains("obopsso_"));
    }
}

#[test]
fn a_code_is_worth_nothing_to_another_browser_than_the_one_that_signed_in() {
    // Login CSRF: someone signs in at Microsoft in their own browser, gets the one-time code and
    // puts it in a link for a victim. The victim's browser does not hold the browser secret.
    let r = world();
    let id = identity(&r, ADA_OID);
    let code = sso(&r).begin(&id, BINDER).unwrap();
    assert!(matches!(
        sso(&r).exchange(&code, "another-browsers-secret-0123456789"),
        Err(MfaError::Expired)
    ));
    assert!(matches!(
        sso(&r).exchange(&code, ""),
        Err(MfaError::Expired)
    ));
    // The code was spent by the attempt: the right browser cannot use it afterwards either.
    assert!(matches!(
        sso(&r).exchange(&code, BINDER),
        Err(MfaError::Expired)
    ));
    assert!(
        events(&r, "oidc_refused")
            .iter()
            .any(|d| d.contains("another browser"))
    );
    // Only its hash is kept with the code.
    let code = sso(&r).begin(&id, BINDER).unwrap();
    let stored = r.b.kv_get(&code_key(&code)).unwrap().unwrap();
    assert!(!stored.contains(BINDER));
    // No browser secret, no code.
    assert!(matches!(sso(&r).begin(&id, ""), Err(SsoError::Invalid(_))));
    assert!(matches!(
        sso(&r).begin(&id, "short"),
        Err(SsoError::Invalid(_))
    ));
}

#[test]
fn an_identity_nobody_is_bound_to_signs_in_as_nobody() {
    let r = world();
    // Another object id in the right tenant, however well signed.
    let stranger = identity(&r, BOB_OID);
    assert!(matches!(
        sso(&r).begin(&stranger, BINDER),
        Err(SsoError::Refused(_))
    ));
    assert_eq!(events(&r, "oidc_refused").len(), 1);
    assert!(events(&r, "oidc_refused")[0].contains(BOB_OID));
    // The same object id in another tenant or under another issuer is another identity.
    let mut forged = identity(&r, ADA_OID);
    forged.tenant = OTHER_TENANT.into();
    assert!(sso(&r).begin(&forged, BINDER).is_err());
    let mut forged = identity(&r, ADA_OID);
    forged.issuer = format!("{AUTHORITY}/{OTHER_TENANT}/v2.0");
    assert!(sso(&r).begin(&forged, BINDER).is_err());
}

#[test]
fn the_account_and_the_provider_are_checked_again_when_the_code_is_spent() {
    let r = world();
    let id = identity(&r, ADA_OID);
    // Disabled before the code is issued.
    r.a.operator_set_disabled("ada", true).unwrap();
    assert!(matches!(
        sso(&r).begin(&id, BINDER),
        Err(SsoError::Refused(_))
    ));
    r.a.operator_set_disabled("ada", false).unwrap();
    // Disabled between the callback and the exchange.
    let code = sso(&r).begin(&id, BINDER).unwrap();
    r.a.operator_set_disabled("ada", true).unwrap();
    assert!(matches!(
        sso(&r).exchange(&code, BINDER),
        Err(MfaError::Expired)
    ));
    r.a.operator_set_disabled("ada", false).unwrap();
    // Offboarded in between.
    let code = sso(&r).begin(&id, BINDER).unwrap();
    r.a.operator_offboard("ada", "left").unwrap();
    assert!(matches!(
        sso(&r).exchange(&code, BINDER),
        Err(MfaError::Expired)
    ));
    r.a.operator_set_disabled("ada", false).unwrap();
    // The credentials changed in between (a reset, new groups): the half-finished sign-in ends.
    let code = sso(&r).begin(&id, BINDER).unwrap();
    let mut patch = Fields::new();
    bump_epoch(&mut patch);
    r.b.update("auth_user", "ada", &patch).unwrap();
    assert!(matches!(
        sso(&r).exchange(&code, BINDER),
        Err(MfaError::Expired)
    ));
    // Unbound in between.
    let code = sso(&r).begin(&id, BINDER).unwrap();
    sso(&r).unbind(&admin("root1"), "ada").unwrap();
    assert!(matches!(
        sso(&r).exchange(&code, BINDER),
        Err(MfaError::Expired)
    ));
    // The provider switched off between the verification and the code.
    r.a.operator_sso_bind("ada", ADA_OID).unwrap();
    r.a.operator_sso_configure(TENANT, CLIENT, None, Some(false))
        .unwrap();
    assert!(matches!(
        sso(&r).begin(&id, BINDER),
        Err(SsoError::Refused(_))
    ));
    assert!(!events(&r, "oidc_refused").is_empty());
}

#[test]
fn the_disabled_flag_alone_ends_a_half_finished_sign_in() {
    // Every path that disables an account also changes its credential epoch today; the exchange
    // reads the flag itself so that a path that does not still ends the sign-in.
    let r = world();
    let code = sso(&r).begin(&identity(&r, ADA_OID), BINDER).unwrap();
    let mut patch = Fields::new();
    patch.insert("disabled".into(), J::Bool(true));
    r.b.update("auth_user", "ada", &patch).unwrap();
    assert!(matches!(
        sso(&r).exchange(&code, BINDER),
        Err(MfaError::Expired)
    ));
    assert!(
        events(&r, "oidc_refused")
            .iter()
            .any(|d| d.contains("disabled")),
        "{:?}",
        events(&r, "oidc_refused")
    );
}

#[test]
fn the_one_time_code_lives_sixty_seconds_and_not_a_second_more() {
    // The lifetime is stated in D91 and in docs/DEPLOY.md: pinned here by its number, not by the
    // constant that sets it.
    let r = world();
    let id = identity(&r, ADA_OID);
    let early = sso(&r).begin(&id, BINDER).unwrap();
    let late = sso(&r).begin(&id, BINDER).unwrap();
    r.a.set_clock_for_test(T0 + 59);
    assert!(sso(&r).exchange(&early, BINDER).is_ok());
    r.a.set_clock_for_test(T0 + 60);
    assert!(matches!(
        sso(&r).exchange(&late, BINDER),
        Err(MfaError::Expired)
    ));
    assert_eq!(CODE_SECS, 60);
}

#[test]
fn an_account_with_its_own_second_factor_still_gets_asked_for_it() {
    let r = world();
    let (secret, _) = enrol(&r.a, "ada");
    // Microsoft says mfa, but ada is not SSO-only: her obop second factor stays.
    let code = sso(&r).begin(&identity(&r, ADA_OID), BINDER).unwrap();
    let (who, LoginOutcome::MfaRequired { mfa_token: t }) =
        sso(&r).exchange(&code, BINDER).unwrap()
    else {
        panic!("an obop second factor was expected")
    };
    assert_eq!(who, "ada", "the page is told who is signing in");
    assert!(r.a.resolve(&t).is_none(), "an mfa token is not a session");
    r.a.set_clock_for_test(T0 + 60);
    let (token, limited) = session_of(
        r.a.login_mfa(&t, Some(&code_at(&secret, T0 + 60)), None)
            .unwrap(),
    );
    assert!(!limited && r.a.resolve(&token).unwrap().id == "ada");
    // And without `mfa` in amr nothing changes.
    let id = identity_with(&r, ADA_OID, without_mfa);
    assert!(!id.mfa());
    let code = sso(&r).begin(&id, BINDER).unwrap();
    assert!(matches!(
        sso(&r).exchange(&code, BINDER).unwrap().1,
        LoginOutcome::MfaRequired { .. }
    ));
}

#[test]
fn an_sso_only_account_takes_microsofts_second_factor_as_its_own() {
    let r = world_with(enforcing());
    // An administrator who has no obop second factor: with the policy on, a password session
    // would be limited to enrolment.
    user_with(&r.a, "boss", &[ADMIN_GROUP]);
    r.a.operator_sso_bind("boss", BOB_OID).unwrap();
    let (_, limited) = session_of(r.a.login_step("boss", PW).unwrap());
    assert!(limited, "the policy limits a password session of an admin");
    signed_in_once(&r, BOB_OID);
    sso_only(&r, "boss", true).unwrap();
    // Microsoft reports a second factor: a whole session, no obop code.
    let code = sso(&r).begin(&identity(&r, BOB_OID), BINDER).unwrap();
    let (token, limited) = session_of(sso(&r).exchange(&code, BINDER).unwrap().1);
    assert!(!limited);
    let p = r.a.resolve(&token).unwrap();
    assert!(p.allows(ADMIN_GROUP) && !p.mfa_limited);
    // Microsoft does not report one: nothing stands for the second factor, so the session is
    // limited to enrolment, exactly as for a password.
    let code = sso(&r)
        .begin(&identity_with(&r, BOB_OID, without_mfa), BINDER)
        .unwrap();
    let (token, limited) = session_of(sso(&r).exchange(&code, BINDER).unwrap().1);
    assert!(limited);
    assert!(r.a.resolve(&token).unwrap().mfa_limited);
    let line = events(&r, "oidc_login");
    // The first one is the sign-in made before the account was Microsoft-only.
    assert_eq!(line.len(), 3);
    assert!(line[1].contains("reports a second factor") && line[2].contains("does not report"));
}

#[test]
fn an_sso_only_account_refuses_its_password_and_ends_its_sessions() {
    let r = world();
    signed_in_once(&r, ADA_OID);
    let (old, _) = session(&r.a, "ada");
    assert!(r.a.resolve(&old).is_some());
    let on = sso_only(&r, "ada", true).unwrap();
    assert_eq!(on["changed"], true);
    assert_eq!(on["break_glass"], json!(["root1", "root2", "root3"]));
    // Signed out at once.
    assert!(r.a.resolve(&old).is_none());
    // The password, right or wrong, gets the answer of a wrong password.
    let right = r.a.login_step("ada", PW).unwrap_err();
    let wrong = r.a.login_step("ada", "not the password").unwrap_err();
    let ghost = r.a.login_step("ghost", PW).unwrap_err();
    assert_eq!(right, wrong);
    assert_eq!(right, ghost);
    assert!(r.a.login("ada", PW).is_err());
    assert_eq!(events(&r, "login_refused").len(), 2);
    assert!(events(&r, "login_refused")[0].contains("Microsoft only"));
    // The repeat changes nothing; an SSO-only account is not unbound; turning it off gives the
    // password back.
    assert_eq!(sso_only(&r, "ada", true).unwrap()["changed"], false);
    assert!(invalid(sso(&r).unbind(&admin("root1"), "ada")).contains("turn that off"));
    let off = sso_only(&r, "ada", false).unwrap();
    assert_eq!(off["changed"], true);
    assert!(r.a.login_step("ada", PW).is_ok());
    // No password can be set for it while it is SSO-only.
    sso_only(&r, "ada", true).unwrap();
    assert!(matches!(
        r.a.reset_user_password(&admin("root1"), "ada", "a temporary password"),
        Err(MfaError::Invalid(m)) if m.contains("Microsoft only")
    ));
    assert!(r.a.login_step("ada", "a temporary password").is_err());
}

#[test]
fn an_sso_only_account_does_not_sign_in_with_a_passkey_alone() {
    let r = world();
    let (token, p) = session(&r.a, "ada");
    let mut key = SoftAuthenticator::es256();
    add(&r, &token, &p, &mut key, "Phone").unwrap();
    let opts = r.a.proof_options(&p, &ctx()).unwrap();
    r.a.proof(&p, &ctx(), &key.get(&opts, ORIGIN)).unwrap();
    r.a.set_passwordless(&p, true, &pw()).unwrap();
    let alone = |key: &mut SoftAuthenticator| {
        let opts = r.a.passkey_login_options(&ctx(), None).unwrap();
        r.a.passkey_login(&ctx(), None, &key.get(&opts, ORIGIN))
    };
    // Before: passwordless works.
    assert!(matches!(alone(&mut key), Ok(LoginOutcome::Session { .. })));
    signed_in_once(&r, ADA_OID);
    sso_only(&r, "ada", true).unwrap();
    match alone(&mut key) {
        Err(MfaError::Denied(m)) => assert!(m.contains("could not be used")),
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert!(
        events(&r, "login_refused")
            .iter()
            .any(|d| d.contains("passkey alone"))
    );
    // Turned off, it works again.
    sso_only(&r, "ada", false).unwrap();
    assert!(matches!(alone(&mut key), Ok(LoginOutcome::Session { .. })));
}

#[test]
fn an_account_is_made_sso_only_only_with_a_binding_a_provider_and_break_glass_administrators() {
    // Two administrators, one with a second factor and one without.
    let r = rig();
    for n in ["root1", "root2"] {
        user_with(&r.a, n, &[ADMIN_GROUP]);
    }
    enrol(&r.a, "root1");
    user_with(&r.a, "ada", &["base.user"]);
    r.a.operator_sso_configure(TENANT, CLIENT, Some(SECRET), Some(true))
        .unwrap();
    // Not bound.
    assert!(invalid(sso_only(&r, "ada", true)).contains("not bound"));
    r.a.operator_sso_bind("ada", ADA_OID).unwrap();
    // Bound, but the person never signed in with the button: the object id is untried.
    assert!(invalid(sso_only(&r, "ada", true)).contains("never signed in"));
    signed_in_once(&r, ADA_OID);
    // One administrator with a second factor is not enough; the one without does not count.
    let m = invalid(sso_only(&r, "ada", true));
    assert!(m.contains(&format!("at least {BREAK_GLASS_MIN}")) && m.contains("there are 1 now"));
    assert!(m.contains("root1") && !m.contains("root2"));
    // A disabled administrator does not count, whatever second factor it holds.
    user_with(&r.a, "root3", &[ADMIN_GROUP]);
    enrol(&r.a, "root3");
    r.a.operator_set_disabled("root3", true).unwrap();
    assert!(invalid(sso_only(&r, "ada", true)).contains("there are 1 now"));
    r.a.operator_set_disabled("root3", false).unwrap();
    // An administrator with a passkey and no app counts.
    let (token, p) = session(&r.a, "root2");
    let mut key = SoftAuthenticator::es256();
    add(&r, &token, &p, &mut key, "Key").unwrap();
    assert_eq!(sso_only(&r, "ada", true).unwrap()["changed"], true);
    let status = sso(&r).bindings(&admin("root1")).unwrap();
    assert_eq!(status["break_glass"], json!(["root1", "root2", "root3"]));
    assert_eq!(status["break_glass_enough"], true);
    // An administrator who is SSO-only does not count as one that signs in locally: after root1
    // only root2 and root3 remain, which is enough; after root2 only root3 would.
    r.a.operator_sso_bind("root1", BOB_OID).unwrap();
    signed_in_once(&r, BOB_OID);
    assert_eq!(sso_only(&r, "root1", true).unwrap()["changed"], true);
    r.a.operator_sso_bind("root2", ROOT2_OID).unwrap();
    signed_in_once(&r, ROOT2_OID);
    let m = invalid(sso_only(&r, "root2", true));
    assert!(m.contains("there are 1 now") && m.contains("root3"), "{m}");
    // Provider off: nothing can be made SSO-only.
    r.a.operator_sso_configure(TENANT, CLIENT, None, Some(false))
        .unwrap();
    user_with(&r.a, "bob", &["base.user"]);
    assert!(invalid(sso_only(&r, "bob", true)).contains("not enabled"));
    // The status shows who is bound and who signs in with Microsoft only.
    let status = sso(&r).bindings(&admin("root1")).unwrap();
    let row = |n: &str| {
        status["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["name"] == n)
            .cloned()
            .unwrap()
    };
    assert_eq!(row("ada")["sso_only"], true);
    assert_eq!(row("ada")["oid"], ADA_OID);
    assert_eq!(row("bob")["bound"], false);
    assert_eq!(status["break_glass"], json!(["root2", "root3"]));
    assert_eq!(status["break_glass_enough"], true);
}

#[test]
fn a_disabled_account_is_not_made_sso_only() {
    let r = world();
    r.a.operator_set_disabled("ada", true).unwrap();
    assert!(invalid(sso_only(&r, "ada", true)).contains("disabled"));
    invalid(sso_only(&r, "ghost", true));
}

#[test]
fn clearing_the_provider_keeps_the_bindings_and_says_who_depends_on_it() {
    let r = world();
    signed_in_once(&r, ADA_OID);
    sso_only(&r, "ada", true).unwrap();
    let id = identity(&r, ADA_OID);
    sso(&r).clear(&admin("root1")).unwrap();
    assert!(!sso(&r).offered() && sso(&r).provider().unwrap().is_none());
    assert!(sso(&r).leg().unwrap().is_none());
    assert!(matches!(
        sso(&r).begin(&id, BINDER),
        Err(SsoError::Refused(_))
    ));
    let line = events(&r, "oidc_provider_cleared");
    assert_eq!(line.len(), 1);
    assert!(line[0].contains("1 SSO-only"), "{}", line[0]);
    // The binding stays for the day the provider is configured again.
    let status = sso(&r).bindings(&admin("root1")).unwrap();
    assert!(status["provider"].is_null());
    assert!(
        status["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["name"] == "ada" && a["bound"] == true)
    );
    assert!(invalid(sso(&r).clear(&admin("root1"))).contains("no provider"));
}

#[test]
fn the_provider_port_checks_the_token_against_this_instance() {
    let r = world();
    let s = StandIn::new();
    let now = r.a.now();
    let good = StandIn::claims(AUTHORITY, TENANT, CLIENT, NONCE, ADA_OID, now);
    let jwks = s.jwks();
    let verify = |c: &J, nonce: &str| sso(&r).verify(&s.token(c), jwks.as_bytes(), nonce);
    assert!(verify(&good, NONCE).is_ok());
    assert!(matches!(
        verify(&good, "another nonce"),
        Err(SsoError::Refused(_))
    ));
    let mut c = good.clone();
    c["aud"] = J::from("11111111-0000-0000-0000-000000000000");
    assert!(matches!(verify(&c, NONCE), Err(SsoError::Refused(_))));
    let mut c = good.clone();
    c["tid"] = J::from(OTHER_TENANT);
    c["iss"] = J::from(format!("{AUTHORITY}/{OTHER_TENANT}/v2.0"));
    assert!(matches!(verify(&c, NONCE), Err(SsoError::Refused(_))));
    // A key the cached set does not hold asks for the set to be fetched again.
    let unknown = s.sign(&json!({"alg": "RS256", "kid": "rotated"}), &good);
    assert_eq!(
        sso(&r).verify(&unknown, jwks.as_bytes(), NONCE),
        Err(SsoError::UnknownKey)
    );
    // A key set that is not one is the provider's trouble, not a refusal of the person.
    assert!(matches!(
        sso(&r).verify(&s.token(&good), b"<html>", NONCE),
        Err(SsoError::Unavailable(_))
    ));
    // The discovery document of the pinned tenant, and nobody else's.
    let ok = sso(&r)
        .endpoints(s.discovery(AUTHORITY, TENANT).as_bytes())
        .unwrap();
    assert!(ok.token.starts_with(&format!("{AUTHORITY}/{TENANT}/")));
    assert!(matches!(
        sso(&r).endpoints(s.discovery(AUTHORITY, OTHER_TENANT).as_bytes()),
        Err(SsoError::Refused(_))
    ));
}

#[test]
fn the_stand_in_authority_is_loopback_over_http_or_nothing() {
    use super::authority_from;
    assert_eq!(authority_from(None), None);
    assert_eq!(authority_from(Some("")), None);
    assert_eq!(authority_from(Some(AUTHORITY)), None);
    assert_eq!(
        authority_from(Some(" http://127.0.0.1:4455/ ")).as_deref(),
        Some("http://127.0.0.1:4455")
    );
    for bad in [
        "https://login.microsoftonline.com.evil.test",
        "http://evil.test:80",
        "http://localhost:4455",
        "https://127.0.0.1:4455",
        "http://127.0.0.1:4455/tenant",
    ] {
        assert_eq!(authority_from(Some(bad)), None, "{bad}");
    }
    // The authority a test sets is the one the tokens are checked under.
    let r = world();
    r.a.set_oidc_authority_for_test("http://127.0.0.1:4455");
    assert_eq!(r.a.oidc_authority(), "http://127.0.0.1:4455");
    let s = StandIn::new();
    let c = StandIn::claims(
        "http://127.0.0.1:4455",
        TENANT,
        CLIENT,
        NONCE,
        ADA_OID,
        r.a.now(),
    );
    assert!(
        sso(&r)
            .verify(&s.token(&c), s.jwks().as_bytes(), NONCE)
            .is_ok()
    );
    // A token of the real authority is then not this instance's.
    let c = StandIn::claims(AUTHORITY, TENANT, CLIENT, NONCE, ADA_OID, r.a.now());
    assert!(
        sso(&r)
            .verify(&s.token(&c), s.jwks().as_bytes(), NONCE)
            .is_err()
    );
}

#[test]
fn only_the_sign_in_kinds_can_be_written_to_the_log_through_the_port() {
    let r = world();
    let binds = events(&r, "oidc_bind").len(); // the one of the setup
    sso(&r).audit("ada", "oidc_start", "begun");
    sso(&r).audit(
        "ada",
        "oidc_cancelled",
        "the person said no at the provider",
    );
    sso(&r).audit("ada", "login_ok", "pretending to be a password sign-in");
    sso(&r).audit("ada", "oidc_bind", "pretending to be an administrator");
    assert_eq!(events(&r, "oidc_start").len(), 1);
    assert_eq!(events(&r, "oidc_cancelled").len(), 1);
    assert!(events(&r, "login_ok").is_empty());
    assert_eq!(events(&r, "oidc_bind").len(), binds);
    assert_eq!(events(&r, "oidc_error").len(), 2);
}

#[test]
fn the_password_sign_in_of_an_ordinary_account_is_unchanged() {
    // A guard for the change to the password step: an account with a second factor still gets
    // the two-step sign-in, and one without still gets a session.
    let r = world();
    let (secret, _) = enrol(&r.a, "bob");
    let t = mfa_token(&r.a, "bob");
    r.a.set_clock_for_test(T0 + 60);
    let (token, limited) = session_of(
        r.a.login_mfa(&t, Some(&code_at(&secret, T0 + 60)), None)
            .unwrap(),
    );
    assert!(!limited && r.a.resolve(&token).is_some());
    let (token, limited) = session_of(r.a.login_step("ada", PW).unwrap());
    assert!(!limited && r.a.resolve(&token).is_some());
}

#[test]
fn the_client_secret_survives_a_key_rotation() {
    // The secret is listed in the rotation inventory under the purpose it is sealed with, and
    // `secrets rotate` re-seals it so that the new key alone opens it.
    use obop_keys::reseal::{FIELDS, Options, Purpose, run};
    assert!(FIELDS.iter().any(|f| f.model == PROVIDER_TABLE
        && f.field == "client_secret"
        && matches!(f.purpose, Purpose::Fixed(p) if p == super::SECRET_PURPOSE)));
    let old = "old-master-key-for-the-test-only!!";
    let new = "new-master-key-for-the-test-only!!";
    let r = rig_with(AuthConfig {
        keys: obop_keys::KeyRing::from_secrets(old, &[]).unwrap(),
        ..AuthConfig::insecure_dev()
    });
    r.a.operator_sso_configure(TENANT, CLIENT, Some(SECRET), Some(true))
        .unwrap();
    let ring = obop_keys::KeyRing::from_secrets(new, &[old]).unwrap();
    let report = run(&r.b, &ring, Options::default(), &mut |_| {}).unwrap();
    assert!(report.to_reseal >= 1, "{report:?}");
    // Only the new key is left.
    let only_new = obop_keys::KeyRing::from_secrets(new, &[]).unwrap();
    let a = crate::Auth::with_test_config(
        r.b.clone(),
        AuthConfig {
            keys: only_new,
            ..AuthConfig::insecure_dev()
        },
    );
    assert_eq!(
        a.sso().unwrap().leg().unwrap().unwrap().client_secret,
        SECRET
    );
}

// ---- the break-glass rule on every way of taking a local sign-in away (D91 Decision 6) --------

/// root1, root2 and root3 (administrators with an authenticator app), `ada` bound, signed in once
/// and Microsoft-only, and root3 disabled: exactly [`BREAK_GLASS_MIN`] local administrators are
/// left, root1 and root2. Returns the rig and the secret of root2's app.
fn depending_world() -> (Rig, Vec<u8>) {
    let r = rig();
    let mut secret2 = Vec::new();
    for n in ["root1", "root2", "root3"] {
        user_with(&r.a, n, &[ADMIN_GROUP]);
        let (secret, _) = enrol(&r.a, n);
        if n == "root2" {
            secret2 = secret;
        }
    }
    user_with(&r.a, "ada", &["base.user"]);
    r.a.operator_sso_configure(TENANT, CLIENT, Some(SECRET), Some(true))
        .unwrap();
    r.a.operator_sso_bind("ada", ADA_OID).unwrap();
    signed_in_once(&r, ADA_OID);
    sso_only(&r, "ada", true).unwrap();
    // Three local administrators, one may go.
    r.a.set_user_disabled(&admin("root1"), "root3", true)
        .unwrap();
    assert_eq!(
        sso(&r).bindings(&admin("root1")).unwrap()["break_glass"],
        json!(["root1", "root2"])
    );
    (r, secret2)
}

fn shortfall<T: std::fmt::Debug>(r: Result<T, MfaError>) -> String {
    match r {
        Err(MfaError::Invalid(m)) => {
            assert!(
                m.contains("sign in with Microsoft only") && m.contains("fewer than 2"),
                "{m}"
            );
            m
        }
        other => panic!("expected the break-glass refusal, got {other:?}"),
    }
}

fn known() -> Vec<String> {
    vec!["base.user".into(), ADMIN_GROUP.into(), "sales.user".into()]
}

#[test]
fn an_administrator_is_not_disabled_offboarded_or_demoted_below_two_local_ones() {
    let (r, _) = depending_world();
    let root = admin("root1");
    shortfall(r.a.set_user_disabled(&root, "root2", true));
    shortfall(r.a.offboard_user(&root, "root2", "left"));
    shortfall(r.a.set_user_groups(&root, "root2", &["base.user".to_string()], &known()));
    // Nothing was done by any of them.
    let u = r.b.get("auth_user", "root2").unwrap().unwrap();
    assert_ne!(u.get("disabled"), Some(&json!(true)));
    assert!(
        u["groups"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g == ADMIN_GROUP)
    );
    // Changes that take no local sign-in away pass: another group for the same administrator,
    // and an account that is not a break-glass administrator.
    let with_sales = vec![
        "base.user".to_string(),
        ADMIN_GROUP.to_string(),
        "sales.user".to_string(),
    ];
    r.a.set_user_groups(&root, "root2", &with_sales, &known())
        .unwrap();
    user_with(&r.a, "carol", &["base.user"]);
    r.a.set_user_disabled(&root, "carol", true).unwrap();
    r.a.offboard_user(&root, "carol", "left").unwrap();
    // A third local administrator makes room for one of them again.
    user_with(&r.a, "root4", &[ADMIN_GROUP]);
    enrol(&r.a, "root4");
    r.a.set_user_disabled(&root, "root2", true).unwrap();
}

#[test]
fn the_last_administrator_rule_does_not_count_microsoft_only_administrators() {
    let r = rig();
    user_with(&r.a, "solo", &[ADMIN_GROUP]);
    user_with(&r.a, "boss", &[ADMIN_GROUP]);
    // `boss` signs in with Microsoft only (the state, as the flag leaves it).
    let mut f = Fields::new();
    f.insert("sso_only".into(), json!(true));
    r.b.update("auth_user", "boss", &f).unwrap();
    let boss = admin("boss");
    for refusal in [
        r.a.set_user_disabled(&boss, "solo", true),
        r.a.offboard_user(&boss, "solo", "left").map(|_| ()),
        r.a.set_user_groups(&boss, "solo", &["base.user".to_string()], &known())
            .map(|_| ()),
    ] {
        match refusal {
            Err(MfaError::Invalid(m)) => assert!(m.contains("last enabled administrator"), "{m}"),
            other => panic!("expected the last-administrator refusal, got {other:?}"),
        }
    }
    // A second local administrator and it is allowed.
    user_with(&r.a, "root2", &[ADMIN_GROUP]);
    r.a.set_user_disabled(&boss, "solo", true).unwrap();
}

#[test]
fn turning_the_authenticator_app_off_or_an_administrators_reset_keeps_two_local_ones() {
    let (r, secret2) = depending_world();
    // root2 turns the app off for itself: it is the only second factor it has.
    r.a.set_clock_for_test(T0 + 60);
    let t = mfa_token(&r.a, "root2");
    let (token, _) = session_of(
        r.a.login_mfa(&t, Some(&code_at(&secret2, T0 + 60)), None)
            .unwrap(),
    );
    let p = r.a.resolve(&token).unwrap();
    r.a.set_clock_for_test(T0 + 120);
    shortfall(r.a.disable(&p, PW, Some(&code_at(&secret2, T0 + 120)), None));
    // Another administrator resets it for them.
    shortfall(r.a.admin_reset(&admin("root1"), "root2"));
    // root2 still has the app and still counts.
    assert_eq!(
        sso(&r).bindings(&admin("root1")).unwrap()["break_glass"],
        json!(["root1", "root2"])
    );
    // With a third local administrator the same request goes through.
    user_with(&r.a, "root4", &[ADMIN_GROUP]);
    enrol(&r.a, "root4");
    r.a.admin_reset(&admin("root1"), "root2").unwrap();
}

#[test]
fn removing_the_last_passkey_of_a_local_administrator_keeps_two_local_ones() {
    let r = rig();
    for n in ["root1", "root3"] {
        user_with(&r.a, n, &[ADMIN_GROUP]);
        enrol(&r.a, n);
    }
    // An administrator whose only second factor is a passkey.
    user_with(&r.a, "keys", &[ADMIN_GROUP]);
    let (token, p) = session(&r.a, "keys");
    let mut key = SoftAuthenticator::es256();
    add(&r, &token, &p, &mut key, "Key").unwrap();
    let id = r.a.list(&p, &ctx()).unwrap()["passkeys"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    user_with(&r.a, "ada", &["base.user"]);
    r.a.operator_sso_configure(TENANT, CLIENT, Some(SECRET), Some(true))
        .unwrap();
    r.a.operator_sso_bind("ada", ADA_OID).unwrap();
    signed_in_once(&r, ADA_OID);
    sso_only(&r, "ada", true).unwrap();
    r.a.set_user_disabled(&admin("root1"), "root3", true)
        .unwrap();
    let prove = |key: &mut SoftAuthenticator| {
        let opts = r.a.proof_options(&p, &ctx()).unwrap();
        r.a.proof(&p, &ctx(), &key.get(&opts, ORIGIN)).unwrap();
    };
    prove(&mut key);
    shortfall(r.a.remove(&p, &token, &id, &pw()));
    assert_eq!(
        r.a.list(&p, &ctx()).unwrap()["passkeys"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // With Microsoft no longer needed by anyone, it is allowed.
    sso_only(&r, "ada", false).unwrap();
    prove(&mut key);
    r.a.remove(&p, &token, &id, &pw()).unwrap();
}

#[test]
fn the_operator_is_warned_not_stopped_and_the_line_says_what_was_left() {
    let (r, _) = depending_world();
    r.a.operator_reset_mfa("root2").unwrap();
    let reset = events(&r, "reset");
    assert!(
        reset
            .iter()
            .any(|d| d.contains("WARNING") && d.contains("fewer than 2")),
        "{reset:?}"
    );
    r.a.operator_set_disabled("root1", true).unwrap();
    assert!(
        events(&r, "user_disable")
            .iter()
            .any(|d| d.contains("WARNING"))
    );
    // The operator's groups command and offboarding warn the same way.
    user_with(&r.a, "root5", &[ADMIN_GROUP]);
    enrol(&r.a, "root5");
    user_with(&r.a, "root6", &[ADMIN_GROUP]);
    enrol(&r.a, "root6");
    r.a.operator_set_groups("root5", &["base.user".to_string()], &known())
        .unwrap();
    assert!(
        events(&r, "user_groups")
            .iter()
            .any(|d| d.contains("WARNING")),
        "{:?}",
        events(&r, "user_groups")
    );
    r.a.operator_offboard("root6", "left").unwrap();
    assert!(
        events(&r, "user_offboard")
            .iter()
            .any(|d| d.contains("WARNING"))
    );
}

#[test]
fn a_replaced_server_key_leaves_an_authenticator_app_that_does_not_count() {
    // The flag is on, the secret is sealed under a key this server no longer holds: that
    // administrator cannot sign in with the app and is not a break-glass administrator.
    let (r, _) = depending_world();
    let mut f = Fields::new();
    f.insert("totp_secret".into(), json!("not-a-sealed-secret"));
    r.b.update("auth_user", "root2", &f).unwrap();
    assert_eq!(
        sso(&r).bindings(&admin("root1")).unwrap()["break_glass"],
        json!(["root1"]),
        "an app whose secret cannot be opened is no way in"
    );
    assert_eq!(
        sso(&r).bindings(&admin("root1")).unwrap()["break_glass_enough"],
        false
    );
}

#[test]
fn an_account_made_microsoft_only_needs_a_sign_in_on_its_binding_and_the_current_tenant() {
    let r = world();
    // Bound but never used.
    assert!(invalid(sso_only(&r, "ada", true)).contains("never signed in"));
    // A sign-in of someone else's binding does not count.
    r.a.operator_sso_bind("bob", BOB_OID).unwrap();
    signed_in_once(&r, BOB_OID);
    assert!(invalid(sso_only(&r, "ada", true)).contains("never signed in"));
    signed_in_once(&r, ADA_OID);
    sso_only(&r, "ada", true).unwrap();
    let status = sso(&r).bindings(&admin("root1")).unwrap();
    let row = status["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "ada")
        .cloned()
        .unwrap();
    assert!(row["last_login_at"].is_string() && row["binding_current"] == true);
    // Turning it off hands the password back, but the old one is not: it must be changed.
    sso_only(&r, "ada", false).unwrap();
    let u = r.b.get("auth_user", "ada").unwrap().unwrap();
    assert_eq!(u["must_change_password"], true);
}

#[test]
fn the_tenant_pin_holds_after_the_provider_was_cleared() {
    let r = world();
    let root = admin("root1");
    signed_in_once(&r, ADA_OID);
    sso(&r).clear(&root).unwrap();
    // No provider record, a binding of the old tenant: another tenant is refused all the same.
    assert!(
        invalid(sso(&r).configure(&root, OTHER_TENANT, CLIENT, Some(SECRET), Some(true)))
            .contains("unbind")
    );
    // The same tenant again is fine, and the binding still works.
    sso(&r)
        .configure(&root, TENANT, CLIENT, Some(SECRET), Some(true))
        .unwrap();
    sso_only(&r, "ada", true).unwrap();
    // Unbound, the tenant can change.
    sso_only(&r, "ada", false).unwrap();
    sso(&r).unbind(&root, "ada").unwrap();
    sso(&r).clear(&root).unwrap();
    sso(&r)
        .configure(&root, OTHER_TENANT, CLIENT, Some(SECRET), Some(true))
        .unwrap();
}

#[test]
fn a_binding_of_another_tenant_than_the_providers_is_not_made_microsoft_only() {
    let r = world();
    signed_in_once(&r, ADA_OID);
    // The binding is rewritten as if it was made under another tenant (a provider that was
    // changed behind the administrators' back): it can never match a sign-in.
    let id = link_key(&format!("{AUTHORITY}/{TENANT}/v2.0"), TENANT, ADA_OID);
    let mut f = Fields::new();
    f.insert("tid".into(), json!(OTHER_TENANT));
    r.b.update(LINK_TABLE, &id, &f).unwrap();
    assert!(invalid(sso_only(&r, "ada", true)).contains("not to the tenant"));
    let status = sso(&r).bindings(&admin("root1")).unwrap();
    let row = status["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "ada")
        .cloned()
        .unwrap();
    assert_eq!(row["binding_current"], false);
}

#[test]
fn unbinding_ends_the_sessions_and_a_code_in_the_air_and_says_how_many() {
    let r = world();
    signed_in_once(&r, ADA_OID);
    let (one, _) = session(&r.a, "ada");
    let (two, _) = session(&r.a, "ada");
    let code = sso(&r).begin(&identity(&r, ADA_OID), BINDER).unwrap();
    let done = sso(&r).unbind(&admin("root1"), "ada").unwrap();
    assert_eq!(done["changed"], true);
    assert!(
        done["sessions_ended"].as_u64().unwrap() >= 2,
        "the two sessions made here at least: {done}"
    );
    assert!(r.a.resolve(&one).is_none() && r.a.resolve(&two).is_none());
    assert!(matches!(
        sso(&r).exchange(&code, BINDER),
        Err(MfaError::Expired)
    ));
    let line = events(&r, "oidc_unbind");
    assert!(line[0].contains("session(s) signed out") && line[0].contains("agent tokens"));
    // The account is still there with its password, and the binding is gone.
    assert!(r.a.login_step("ada", PW).is_ok());
    assert!(invalid(sso(&r).unbind(&admin("root1"), "ada")).contains("not bound"));
}

#[test]
fn an_unbinding_that_could_not_end_the_sessions_is_finished_by_repeating_it() {
    use obop_storage::testing::Mode;
    let (r, flaky) = crate::admin_tests::flaky_rig();
    for n in ["root1", "root2", "root3"] {
        user_with(&r.a, n, &[ADMIN_GROUP]);
    }
    user_with(&r.a, "ada", &["base.user"]);
    r.a.operator_sso_configure(TENANT, CLIENT, Some(SECRET), Some(true))
        .unwrap();
    r.a.operator_sso_bind("ada", ADA_OID).unwrap();
    signed_in_once(&r, ADA_OID);
    let (stolen, _) = session(&r.a, "ada");
    flaky.refuse_model("auth_token", Mode::Reads);
    let (done, out) = crate::admin_tests::request(|| sso(&r).unbind(&admin("root1"), "ada"));
    flaky.allow_models();
    assert!(done.is_err() && out.partial, "{done:?} {out:?}");
    // The repeat ends what is left and reports it; the binding is gone either way.
    let again = sso(&r).unbind(&admin("root1"), "ada").unwrap();
    assert_eq!(again["changed"], false);
    assert!(again["sessions_ended"].as_u64().unwrap() >= 1, "{again}");
    assert!(r.a.resolve(&stolen).is_none());
}

#[test]
fn an_unbinding_refused_at_any_call_never_leaves_a_session_alive_when_it_says_done() {
    let mut met = 0;
    for skip in 0..80 {
        let (r, flaky) = crate::admin_tests::flaky_rig();
        for n in ["root1", "root2"] {
            user_with(&r.a, n, &[ADMIN_GROUP]);
        }
        user_with(&r.a, "ada", &["base.user"]);
        r.a.operator_sso_configure(TENANT, CLIENT, Some(SECRET), Some(true))
            .unwrap();
        r.a.operator_sso_bind("ada", ADA_OID).unwrap();
        let (old, _) = session(&r.a, "ada");
        flaky.refuse_call_after(skip);
        let (done, _) = crate::admin_tests::request(|| sso(&r).unbind(&admin("root1"), "ada"));
        met += i32::from(flaky.refused() > 0);
        flaky.allow_models();
        if done.is_ok() {
            assert!(
                r.a.resolve(&old).is_none(),
                "skip {skip}: said done, session alive"
            );
        }
        // Repeating finishes it, whatever stopped it.
        let _ = sso(&r).unbind(&admin("root1"), "ada");
        assert!(r.a.resolve(&old).is_none(), "skip {skip}: {done:?}");
        let id = link_key(&format!("{AUTHORITY}/{TENANT}/v2.0"), TENANT, ADA_OID);
        assert!(r.b.get(LINK_TABLE, &id).unwrap().is_none(), "skip {skip}");
    }
    assert!(met >= 8, "the sweep met a refusal at {met} positions");
}

// ---- more than one process: a code made on one server is spent on another, once --------------

fn second_auth(r: &Rig) -> Auth {
    let a = Auth::with_test_config(r.b.clone(), AuthConfig::insecure_dev());
    a.set_clock_for_test(T0);
    a
}

#[test]
fn a_code_begun_on_one_server_is_spent_on_another_and_by_only_one_of_two_racers() {
    let r = world();
    let other = second_auth(&r);
    let id = identity(&r, ADA_OID);
    // Begun here, spent there (two server processes on one database).
    let code = sso(&r).begin(&id, BINDER).unwrap();
    let (who, _) = other.sso().unwrap().exchange(&code, BINDER).unwrap();
    assert_eq!(who, "ada");
    assert!(matches!(
        sso(&r).exchange(&code, BINDER),
        Err(MfaError::Expired)
    ));
    // Two racers for one code, one on each server: exactly one wins, every time.
    for round in 0..25 {
        let code = sso(&r).begin(&id, BINDER).unwrap();
        let (x, y) = std::thread::scope(|s| {
            let a = s.spawn(|| sso(&r).exchange(&code, BINDER).is_ok());
            let b = s.spawn(|| other.sso().unwrap().exchange(&code, BINDER).is_ok());
            (a.join().unwrap(), b.join().unwrap())
        });
        assert_eq!(
            usize::from(x) + usize::from(y),
            1,
            "round {round}: one winner"
        );
    }
}

/// The same flow on a real PostgreSQL: `OBOP_TEST_POSTGRES_URL=postgres://... cargo test -p obop-auth`.
/// The provider and its bindings are one set per database, so the test clears what it made.
#[test]
fn sign_in_with_microsoft_works_on_postgres_when_available() {
    let Ok(url) = std::env::var("OBOP_TEST_POSTGRES_URL") else {
        eprintln!("skipped: set OBOP_TEST_POSTGRES_URL to run against PostgreSQL");
        return;
    };
    let b = obop_storage::open(&url).unwrap();
    let a = Auth::with_test_config(b.clone(), AuthConfig::insecure_dev());
    a.set_clock_for_test(T0);
    let pid = std::process::id();
    let (user, oid) = (
        format!("pgsso{pid}"),
        format!("{pid:08x}-1111-2222-3333-444444444444"),
    );
    let tenant = format!("{pid:08x}-2222-3333-4444-555555555555");
    // Whatever an earlier, interrupted run left.
    for (id, f) in b.list(LINK_TABLE).unwrap() {
        if f["user"].as_str().is_some_and(|u| u.starts_with("pgsso")) {
            b.delete(LINK_TABLE, &id).unwrap();
        }
    }
    let _ = a.operator_sso_clear();
    a.create_user(&user, PW, &["base.user".to_string()])
        .unwrap();
    a.operator_sso_configure(&tenant, CLIENT, Some(SECRET), Some(true))
        .unwrap();
    a.operator_sso_bind(&user, &oid).unwrap();
    let s = StandIn::new();
    let claims = StandIn::claims(AUTHORITY, &tenant, CLIENT, NONCE, &oid, a.now());
    let api = a.sso().unwrap();
    let id = api
        .verify(&s.token(&claims), s.jwks().as_bytes(), NONCE)
        .unwrap();
    // Begun on one connection pool, spent on another, once.
    let other = Auth::with_test_config(
        obop_storage::open(&url).unwrap(),
        AuthConfig::insecure_dev(),
    );
    other.set_clock_for_test(T0);
    let code = api.begin(&id, BINDER).unwrap();
    let (x, y) = std::thread::scope(|sc| {
        let h1 = sc.spawn(|| api.exchange(&code, BINDER).is_ok());
        let h2 = sc.spawn(|| other.sso().unwrap().exchange(&code, BINDER).is_ok());
        (h1.join().unwrap(), h2.join().unwrap())
    });
    assert_eq!(usize::from(x) + usize::from(y), 1);
    // Unbinding ends the session this made.
    let admin = Principal::user("root", &[ADMIN_GROUP]);
    let done = api.unbind(&admin, &user).unwrap();
    assert!(done["sessions_ended"].as_u64().unwrap() >= 1);
    api.clear(&admin).unwrap();
}

#[test]
fn a_right_password_on_a_microsoft_only_account_locks_it_like_a_wrong_one() {
    // Counted like a wrong password, a right one cannot be told from a wrong one by the lockout.
    let r = world();
    signed_in_once(&r, ADA_OID);
    sso_only(&r, "ada", true).unwrap();
    let said = |r: &Rig, pw: &str| r.a.login_step("ada", pw).unwrap_err();
    let refused = said(&r, PW);
    assert_eq!(refused, said(&r, "not the password"));
    for _ in 0..4 {
        said(&r, PW);
    }
    // Five right passwords in a row, and the sixth attempt is the lockout's answer, as after five
    // wrong ones.
    assert!(said(&r, PW).contains("too many attempts"), "locked");
    assert!(said(&r, "not the password").contains("too many attempts"));
}

#[test]
fn the_provider_is_checked_again_when_the_code_is_spent_not_only_when_it_is_made() {
    let r = world();
    let id = identity(&r, ADA_OID);
    // Switched off between the callback and the exchange.
    let code = sso(&r).begin(&id, BINDER).unwrap();
    r.a.operator_sso_configure(TENANT, CLIENT, None, Some(false))
        .unwrap();
    assert!(matches!(
        sso(&r).exchange(&code, BINDER),
        Err(MfaError::Expired)
    ));
    assert!(
        events(&r, "oidc_refused")
            .iter()
            .any(|d| d.contains("switched off before the sign-in finished"))
    );
    r.a.operator_sso_configure(TENANT, CLIENT, None, Some(true))
        .unwrap();
    // Cleared between the callback and the exchange.
    let code = sso(&r).begin(&id, BINDER).unwrap();
    sso(&r).clear(&admin("root1")).unwrap();
    assert!(matches!(
        sso(&r).exchange(&code, BINDER),
        Err(MfaError::Expired)
    ));
    assert!(
        events(&r, "oidc_refused")
            .iter()
            .any(|d| d.contains("removed before the sign-in finished"))
    );
    // Moved to another tenant (the binding's rewritten to model it) between the two.
    sso(&r)
        .configure(&admin("root1"), TENANT, CLIENT, Some(SECRET), Some(true))
        .unwrap();
    let code = sso(&r).begin(&id, BINDER).unwrap();
    let link = link_key(&format!("{AUTHORITY}/{TENANT}/v2.0"), TENANT, ADA_OID);
    let mut f = Fields::new();
    f.insert("tid".into(), json!(OTHER_TENANT));
    r.b.update(LINK_TABLE, &link, &f).unwrap();
    assert!(matches!(
        sso(&r).exchange(&code, BINDER),
        Err(MfaError::Expired)
    ));
    assert!(
        events(&r, "oidc_refused")
            .iter()
            .any(|d| d.contains("another tenant than the binding"))
    );
}

/// Seconds later: onto a new step of the authenticator app, with the lockout counter of `ada`
/// emptied (it runs on the real clock, not on the test clock).
fn later(r: &Rig) -> u64 {
    r.a.set_clock_for_test(r.a.now() + 61);
    r.a.clear_failures_for_test("ada");
    r.a.now()
}

/// How many wrong passwords the lockout has counted for `ada`.
fn counted(r: &Rig) -> u64 {
    r.b.counter_get(&Auth::failure_key("ada")).unwrap()
}

#[test]
fn a_leftover_password_proves_nothing_for_a_microsoft_only_account_on_any_path() {
    // The hash is kept while the flag is on (turning it off gives the password back), so no step
    // that asks for the password of a signed-in person may accept it: changing the password,
    // turning the app off, new recovery codes, and the re-authentication that adding, renaming
    // and removing a passkey and the passwordless switch are built on (and any later one).
    let r = world();
    let (token0, p0) = session(&r.a, "ada");
    let mut key = SoftAuthenticator::es256();
    add(&r, &token0, &p0, &mut key, "Phone").unwrap();
    let begun = r.a.begin(&p0).unwrap();
    let secret = crate::totp::base32_decode(begun["secret"].as_str().unwrap()).unwrap();
    r.a.confirm(&p0, &token0, &code_at(&secret, T0)).unwrap();
    let id = r.a.list(&p0, &ctx()).unwrap()["passkeys"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    signed_in_once(&r, ADA_OID);
    sso_only(&r, "ada", true).unwrap();
    // The whole session Microsoft's second factor gives.
    let code = sso(&r).begin(&identity(&r, ADA_OID), BINDER).unwrap();
    let (token, limited) = session_of(sso(&r).exchange(&code, BINDER).unwrap().1);
    assert!(!limited);
    let p = r.a.resolve(&token).unwrap();
    let prove = |r: &Rig, key: &mut SoftAuthenticator| {
        let opts = r.a.proof_options(&p, &ctx()).unwrap();
        r.a.proof(&p, &ctx(), &key.get(&opts, ORIGIN)).unwrap();
    };
    // Each refusal is a wrong password for the lockout, so that the leftover cannot be guessed at.
    let bad = |what: &str, got: Result<(), MfaError>| {
        assert!(matches!(got, Err(MfaError::BadPassword)), "{what}: {got:?}");
        assert_eq!(counted(&r), 1, "{what}: counted like a wrong password");
    };

    // The right password, a right code, a right passkey proof: refused on every path.
    let now = later(&r);
    match r.a.change_password(
        &p,
        &token,
        PW,
        "a brand new password",
        Some(&code_at(&secret, now)),
        None,
    ) {
        Err(MfaError::Invalid(m)) => {
            assert!(m.contains("Microsoft only") && m.contains("no password"))
        }
        other => panic!("expected a refusal in words, got {other:?}"),
    }
    assert_eq!(counted(&r), 0, "the answer does not depend on the password");
    let now = later(&r);
    bad(
        "disable",
        r.a.disable(&p, PW, Some(&code_at(&secret, now)), None),
    );
    let now = later(&r);
    bad(
        "regenerate_recovery",
        r.a.regenerate_recovery(&p, PW, Some(&code_at(&secret, now)), None)
            .map(|_| ()),
    );
    later(&r);
    prove(&r, &mut key);
    bad(
        "register_options",
        r.a.register_options(&p, &ctx(), &pw()).map(|_| ()),
    );
    later(&r);
    prove(&r, &mut key);
    bad("rename", r.a.rename(&p, &id, "Renamed", &pw()));
    later(&r);
    prove(&r, &mut key);
    bad("remove", r.a.remove(&p, &token, &id, &pw()));
    later(&r);
    prove(&r, &mut key);
    bad("set_passwordless", r.a.set_passwordless(&p, true, &pw()));
    // Nothing changed, and each refusal of the password was counted and written down.
    assert!(r.a.mfa_enabled("ada") && r.a.passkey_count("ada") == 1);
    assert!(events(&r, "recovery_regenerate").is_empty());
    assert!(events(&r, "disable").is_empty());
    assert_eq!(events(&r, "passkey_failed").len(), 4);
    later(&r);
    assert!(
        r.a.resolve(&token).is_some(),
        "the session itself is untouched"
    );

    // Turned off, the password is the person's again, and every path works with it.
    sso_only(&r, "ada", false).unwrap();
    let now = later(&r);
    let t = mfa_token(&r.a, "ada");
    let (token, _) = session_of(
        r.a.login_mfa(&t, Some(&code_at(&secret, now)), None)
            .unwrap(),
    );
    let p = r.a.resolve(&token).unwrap();
    let now = later(&r);
    r.a.regenerate_recovery(&p, PW, Some(&code_at(&secret, now)), None)
        .unwrap();
    later(&r);
    let opts = r.a.proof_options(&p, &ctx()).unwrap();
    r.a.proof(&p, &ctx(), &key.get(&opts, ORIGIN)).unwrap();
    r.a.set_passwordless(&p, true, &pw()).unwrap();
    let now = later(&r);
    r.a.change_password(
        &p,
        &token,
        PW,
        "a brand new password",
        Some(&code_at(&secret, now)),
        None,
    )
    .unwrap();
    // The password is the new one now, and turning the app off works with it.
    let now = later(&r);
    r.a.disable(
        &p,
        "a brand new password",
        Some(&code_at(&secret, now)),
        None,
    )
    .unwrap();
    assert!(!r.a.mfa_enabled("ada"));
}

#[test]
fn a_store_that_cannot_be_read_is_never_a_yes_for_the_password_of_a_microsoft_only_account() {
    // The answer is "no" for the account and "unavailable" for a refused read; at no position of
    // the call does a refused store make the leftover hash count.
    let (r, flaky) = crate::admin_tests::flaky_rig();
    for n in ["root1", "root2", "root3"] {
        user_with(&r.a, n, &[ADMIN_GROUP]);
        enrol(&r.a, n);
    }
    user_with(&r.a, "ada", &["base.user"]);
    r.a.operator_sso_configure(TENANT, CLIENT, Some(SECRET), Some(true))
        .unwrap();
    r.a.operator_sso_bind("ada", ADA_OID).unwrap();
    signed_in_once(&r, ADA_OID);
    assert!(r.a.password_ok("ada", PW).unwrap(), "an ordinary account");
    sso_only(&r, "ada", true).unwrap();
    assert!(!r.a.password_ok("ada", PW).unwrap());
    let mut unavailable = 0;
    for skip in 0..6 {
        flaky.refuse_call_after(skip);
        match r.a.password_ok("ada", PW) {
            Ok(true) => panic!("a refused read at position {skip} made the password count"),
            Ok(false) => {}
            Err(MfaError::Unavailable(_)) => unavailable += 1,
            Err(other) => panic!("position {skip}: {other:?}"),
        }
    }
    flaky.refuse_call_after(1_000);
    assert!(
        unavailable >= 2,
        "both reads of the account are checked: {unavailable}"
    );
}
