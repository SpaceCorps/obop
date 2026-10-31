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
use crate::{AuthConfig, SESSION_SECS, bump_epoch, token_hash};
use obop_commands::{
    ADMIN_GROUP, Authenticator, LoginOutcome, MfaApi, MfaError, PasskeyApi, Principal, SsoError,
    SsoIdentity,
};
use obop_storage::Fields;
use serde_json::{Value as J, json};

const TENANT: &str = "11111111-2222-3333-4444-555555555555";
const OTHER_TENANT: &str = "99999999-2222-3333-4444-555555555555";
const CLIENT: &str = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
const ADA_OID: &str = "0f0f0f0f-1111-2222-3333-444444444444";
const BOB_OID: &str = "0b0b0b0b-1111-2222-3333-444444444444";
const NONCE: &str = "nonce-of-this-sign-in";
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
    r.a.operator_sso_configure(TENANT, CLIENT, Some(SECRET), true)
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

fn sso_only(r: &Rig, user: &str, on: bool) -> Result<J, SsoError> {
    sso(r).set_sso_only(&admin("root1"), user, on)
}

#[test]
fn the_configuration_is_checked_sealed_and_never_shown() {
    let r = rig();
    let root = admin("root");
    let cfg = |tenant: &str, client: &str, secret: Option<&str>| {
        sso(&r).configure(&root, tenant, client, secret, true)
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
        .configure(&root, TENANT, CLIENT, None, false)
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
            s.configure(&p, TENANT, CLIENT, None, true).unwrap_err(),
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
        sso(&r).configure(&admin("root"), TENANT, CLIENT, Some(SECRET), true),
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
    r.a.operator_sso_configure(TENANT, CLIENT, Some(SECRET), true)
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
        sso(&r).begin(&id, "203.0.113.9"),
        Err(SsoError::Refused(_))
    ));
}

#[test]
fn the_tenant_cannot_change_under_bound_accounts() {
    let r = world();
    let root = admin("root1");
    assert!(invalid(sso(&r).configure(&root, OTHER_TENANT, CLIENT, None, true)).contains("unbind"));
    // The same tenant, a new client id and a new secret are fine.
    let other_client = "bbbbbbbb-bbbb-cccc-dddd-eeeeeeeeeeee";
    sso(&r)
        .configure(&root, TENANT, other_client, Some("another-secret"), true)
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
    let code = sso(&r).begin(&id, "203.0.113.9").unwrap();
    assert!(code.starts_with("obopsso_") && code.len() > 40);
    // Stored hashed, with an expiry; the code itself is not a key.
    assert!(r.b.kv_get(&format!("auth.sso:{code}")).unwrap().is_none());
    assert!(r.b.kv_get(&code_key(&code)).unwrap().is_some());
    let (token, limited) = session_of(sso(&r).exchange(&code).unwrap());
    assert!(!limited);
    assert_eq!(r.a.resolve(&token).unwrap().id, "ada");
    // The session is the one a password gives: bearer, the same lifetime.
    let stored = r.b.get("auth_token", &token_hash(&token)).unwrap().unwrap();
    assert_eq!(stored["expires_at"], json!(T0 + SESSION_SECS));
    assert_eq!(stored["kind"], "user");
    // Spent.
    assert!(matches!(sso(&r).exchange(&code), Err(MfaError::Expired)));
    assert!(r.b.kv_get(&code_key(&code)).unwrap().is_none());
    // A code nobody issued, and one that waited too long.
    assert!(matches!(
        sso(&r).exchange("obopsso_made_up"),
        Err(MfaError::Expired)
    ));
    let late = sso(&r).begin(&id, "203.0.113.9").unwrap();
    r.a.set_clock_for_test(T0 + CODE_SECS + 1);
    assert!(matches!(sso(&r).exchange(&late), Err(MfaError::Expired)));
    // Two sign-ins get two codes.
    r.a.set_clock_for_test(T0);
    assert_ne!(
        sso(&r).begin(&id, "x").unwrap(),
        sso(&r).begin(&id, "x").unwrap()
    );
    assert_eq!(events(&r, "oidc_login").len(), 1);
    // Neither the code nor the secret is in the log.
    for (_, f) in r.b.list(EVENT_TABLE).unwrap() {
        let line = J::Object(f).to_string();
        assert!(!line.contains(&code) && !line.contains(SECRET) && !line.contains("obopsso_"));
    }
}

#[test]
fn an_identity_nobody_is_bound_to_signs_in_as_nobody() {
    let r = world();
    // Another object id in the right tenant, however well signed.
    let stranger = identity(&r, BOB_OID);
    assert!(matches!(
        sso(&r).begin(&stranger, "203.0.113.9"),
        Err(SsoError::Refused(_))
    ));
    assert_eq!(events(&r, "oidc_refused").len(), 1);
    assert!(events(&r, "oidc_refused")[0].contains(BOB_OID));
    // The same object id in another tenant or under another issuer is another identity.
    let mut forged = identity(&r, ADA_OID);
    forged.tenant = OTHER_TENANT.into();
    assert!(sso(&r).begin(&forged, "x").is_err());
    let mut forged = identity(&r, ADA_OID);
    forged.issuer = format!("{AUTHORITY}/{OTHER_TENANT}/v2.0");
    assert!(sso(&r).begin(&forged, "x").is_err());
}

#[test]
fn the_account_and_the_provider_are_checked_again_when_the_code_is_spent() {
    let r = world();
    let id = identity(&r, ADA_OID);
    // Disabled before the code is issued.
    r.a.operator_set_disabled("ada", true).unwrap();
    assert!(matches!(sso(&r).begin(&id, "x"), Err(SsoError::Refused(_))));
    r.a.operator_set_disabled("ada", false).unwrap();
    // Disabled between the callback and the exchange.
    let code = sso(&r).begin(&id, "x").unwrap();
    r.a.operator_set_disabled("ada", true).unwrap();
    assert!(matches!(sso(&r).exchange(&code), Err(MfaError::Expired)));
    r.a.operator_set_disabled("ada", false).unwrap();
    // Offboarded in between.
    let code = sso(&r).begin(&id, "x").unwrap();
    r.a.operator_offboard("ada", "left").unwrap();
    assert!(matches!(sso(&r).exchange(&code), Err(MfaError::Expired)));
    r.a.operator_set_disabled("ada", false).unwrap();
    // The credentials changed in between (a reset, new groups): the half-finished sign-in ends.
    let code = sso(&r).begin(&id, "x").unwrap();
    let mut patch = Fields::new();
    bump_epoch(&mut patch);
    r.b.update("auth_user", "ada", &patch).unwrap();
    assert!(matches!(sso(&r).exchange(&code), Err(MfaError::Expired)));
    // Unbound in between.
    let code = sso(&r).begin(&id, "x").unwrap();
    sso(&r).unbind(&admin("root1"), "ada").unwrap();
    assert!(matches!(sso(&r).exchange(&code), Err(MfaError::Expired)));
    // The provider switched off between the verification and the code.
    r.a.operator_sso_bind("ada", ADA_OID).unwrap();
    r.a.operator_sso_configure(TENANT, CLIENT, None, false)
        .unwrap();
    assert!(matches!(sso(&r).begin(&id, "x"), Err(SsoError::Refused(_))));
    assert!(!events(&r, "oidc_refused").is_empty());
}

#[test]
fn an_account_with_its_own_second_factor_still_gets_asked_for_it() {
    let r = world();
    let (secret, _) = enrol(&r.a, "ada");
    // Microsoft says mfa, but ada is not SSO-only: her obop second factor stays.
    let code = sso(&r).begin(&identity(&r, ADA_OID), "x").unwrap();
    let LoginOutcome::MfaRequired { mfa_token: t } = sso(&r).exchange(&code).unwrap() else {
        panic!("an obop second factor was expected")
    };
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
    let code = sso(&r).begin(&id, "x").unwrap();
    assert!(matches!(
        sso(&r).exchange(&code).unwrap(),
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
    sso_only(&r, "boss", true).unwrap();
    // Microsoft reports a second factor: a whole session, no obop code.
    let code = sso(&r).begin(&identity(&r, BOB_OID), "x").unwrap();
    let (token, limited) = session_of(sso(&r).exchange(&code).unwrap());
    assert!(!limited);
    let p = r.a.resolve(&token).unwrap();
    assert!(p.allows(ADMIN_GROUP) && !p.mfa_limited);
    // Microsoft does not report one: nothing stands for the second factor, so the session is
    // limited to enrolment, exactly as for a password.
    let code = sso(&r)
        .begin(&identity_with(&r, BOB_OID, without_mfa), "x")
        .unwrap();
    let (token, limited) = session_of(sso(&r).exchange(&code).unwrap());
    assert!(limited);
    assert!(r.a.resolve(&token).unwrap().mfa_limited);
    let line = events(&r, "oidc_login");
    assert_eq!(line.len(), 2);
    assert!(line[0].contains("reports a second factor") && line[1].contains("does not report"));
}

#[test]
fn an_sso_only_account_refuses_its_password_and_ends_its_sessions() {
    let r = world();
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
    r.a.operator_sso_configure(TENANT, CLIENT, Some(SECRET), true)
        .unwrap();
    // Not bound.
    assert!(invalid(sso_only(&r, "ada", true)).contains("not bound"));
    r.a.operator_sso_bind("ada", ADA_OID).unwrap();
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
    assert_eq!(sso_only(&r, "root1", true).unwrap()["changed"], true);
    r.a.operator_sso_bind("root2", "0c0c0c0c-1111-2222-3333-444444444444")
        .unwrap();
    let m = invalid(sso_only(&r, "root2", true));
    assert!(m.contains("there are 1 now") && m.contains("root3"), "{m}");
    // Provider off: nothing can be made SSO-only.
    r.a.operator_sso_configure(TENANT, CLIENT, None, false)
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
    sso_only(&r, "ada", true).unwrap();
    let id = identity(&r, ADA_OID);
    sso(&r).clear(&admin("root1")).unwrap();
    assert!(!sso(&r).offered() && sso(&r).provider().unwrap().is_none());
    assert!(sso(&r).leg().unwrap().is_none());
    assert!(matches!(sso(&r).begin(&id, "x"), Err(SsoError::Refused(_))));
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
    r.a.operator_sso_configure(TENANT, CLIENT, Some(SECRET), true)
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
