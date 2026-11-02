//! Passkeys (D65): full ceremonies against a software authenticator, and the ways they must fail.

use super::*;
use crate::mfa_tests::{
    PW, Rig, T0, code_at, enforcing, enrol, mfa_token, rig, rig_with, session, user_with,
};
use crate::webauthn::soft::SoftAuthenticator;
use crate::webauthn::{ALG_EDDSA, ALG_ES256, ALG_RS256, b64url, cbor};
use obop_commands::{ADMIN_GROUP, PasskeyApi, Reauth, WebauthnCtx};

pub(crate) const ORIGIN: &str = "https://obop.test";

pub(crate) fn ctx() -> WebauthnCtx {
    WebauthnCtx {
        rp_id: "obop.test".into(),
        origin: ORIGIN.into(),
        rp_name: "obop".into(),
        ip: "203.0.113.9".into(),
    }
}

pub(crate) fn pw() -> Reauth<'static> {
    Reauth {
        password: PW,
        ..Default::default()
    }
}

/// Add `auth` as a passkey of the signed-in person.
pub(crate) fn add(
    r: &Rig,
    token: &str,
    p: &Principal,
    auth: &mut SoftAuthenticator,
    name: &str,
) -> Result<J, MfaError> {
    let opts = r.a.register_options(p, &ctx(), &pw())?;
    let cred = auth.create(&opts, ORIGIN);
    r.a.register_finish(p, token, &ctx(), name, &cred)
}

fn events_of(r: &Rig, kind: &str) -> Vec<String> {
    r.b.list(EVENT_TABLE)
        .unwrap()
        .into_iter()
        .filter(|(_, f)| f["kind"] == kind)
        .map(|(_, f)| f["detail"].as_str().unwrap().to_string())
        .collect()
}

fn second_step(
    r: &Rig,
    user: &str,
    auth: &mut SoftAuthenticator,
) -> Result<LoginOutcome, MfaError> {
    let tok = mfa_token(&r.a, user);
    let opts = r.a.passkey_login_options(&ctx(), Some(&tok))?;
    r.a.passkey_login(&ctx(), Some(&tok), &auth.get(&opts, ORIGIN))
}

fn is_session(o: &LoginOutcome) -> bool {
    matches!(
        o,
        LoginOutcome::Session {
            mfa_enrolment_required: false,
            ..
        }
    )
}

fn denied<T: std::fmt::Debug>(r: Result<T, MfaError>) -> String {
    match r {
        Err(MfaError::Denied(m)) => m,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_passkey_is_registered_and_signs_in_as_a_second_factor_for_every_algorithm() {
    for alg in [ALG_ES256, ALG_EDDSA, ALG_RS256] {
        let r = rig();
        user_with(&r.a, "ada", &["base.user"]);
        let (token, p) = session(&r.a, "ada");
        let mut auth = SoftAuthenticator::new(alg);
        let done = add(&r, &token, &p, &mut auth, "Laptop").unwrap();
        assert_eq!(done["name"], "Laptop");
        assert_eq!(
            done["recovery_codes"].as_array().unwrap().len(),
            crate::mfa::RECOVERY_CODES,
            "a first second factor comes with recovery codes (alg {alg})"
        );
        // Password alone no longer signs in; the passkey completes it.
        let tok = mfa_token(&r.a, "ada");
        assert_eq!(r.a.methods(&tok), vec!["passkey"]);
        let out = second_step(&r, "ada", &mut auth).unwrap();
        assert!(is_session(&out), "alg {alg}");
        assert_eq!(events_of(&r, "passkey_used").len(), 1);
        let list = r.a.list(&p, &ctx()).unwrap();
        let row = &list["passkeys"][0];
        assert_eq!(row["name"], "Laptop");
        assert!(row["last_used_at"].is_string() && row["created_at"].is_string());
        assert_eq!(row["user_verified"], true);
        assert_eq!(row["discoverable"], true);
    }
}

#[test]
fn the_pending_sign_in_is_single_use_and_a_recovery_code_still_works() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (token, p) = session(&r.a, "ada");
    let mut auth = SoftAuthenticator::es256();
    let done = add(&r, &token, &p, &mut auth, "Key").unwrap();
    let tok = mfa_token(&r.a, "ada");
    let opts = r.a.passkey_login_options(&ctx(), Some(&tok)).unwrap();
    let answer = auth.get(&opts, ORIGIN);
    assert!(is_session(
        &r.a.passkey_login(&ctx(), Some(&tok), &answer).unwrap()
    ));
    // The same token with a second, genuine answer: the sign-in is spent.
    let opts = r.a.passkey_login_options(&ctx(), Some(&tok));
    assert!(matches!(opts, Err(MfaError::Expired)));
    // The recovery code returned at enrolment signs in through the ordinary second step.
    let code = done["recovery_codes"][0].as_str().unwrap();
    let tok = mfa_token(&r.a, "ada");
    assert!(matches!(
        r.a.login_mfa(&tok, None, Some(code)).unwrap(),
        LoginOutcome::Session { .. }
    ));
}

#[test]
fn a_replayed_answer_finds_its_challenge_used_up() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (token, p) = session(&r.a, "ada");
    let mut auth = SoftAuthenticator::es256();
    // Registration replay.
    let opts = r.a.register_options(&p, &ctx(), &pw()).unwrap();
    let cred = auth.create(&opts, ORIGIN);
    r.a.register_finish(&p, &token, &ctx(), "k", &cred).unwrap();
    assert!(denied(r.a.register_finish(&p, &token, &ctx(), "k", &cred)).contains("already used"));
    // Authentication replay (second step).
    let tok = mfa_token(&r.a, "ada");
    let opts = r.a.passkey_login_options(&ctx(), Some(&tok)).unwrap();
    let answer = auth.get(&opts, ORIGIN);
    r.a.passkey_login(&ctx(), Some(&tok), &answer).unwrap();
    let tok2 = mfa_token(&r.a, "ada");
    assert!(denied(r.a.passkey_login(&ctx(), Some(&tok2), &answer)).contains("already used"));
}

#[test]
fn a_challenge_expires_after_two_minutes() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (token, p) = session(&r.a, "ada");
    let mut auth = SoftAuthenticator::es256();
    let opts = r.a.register_options(&p, &ctx(), &pw()).unwrap();
    let cred = auth.create(&opts, ORIGIN);
    r.a.set_clock_for_test(T0 + crate::CHALLENGE_SECS + 1);
    assert!(denied(r.a.register_finish(&p, &token, &ctx(), "k", &cred)).contains("expired"));
    assert_eq!(r.a.passkey_count("ada"), 0);
}

#[test]
fn a_challenge_made_for_one_purpose_does_not_serve_another() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (token, p) = session(&r.a, "ada");
    let mut auth = SoftAuthenticator::es256();
    add(&r, &token, &p, &mut auth, "k").unwrap();
    // A passwordless challenge answered as a proof, and the other way round.
    let login_opts = r.a.passkey_login_options(&ctx(), None).unwrap();
    let answer = auth.get(&login_opts, ORIGIN);
    assert!(denied(r.a.proof(&p, &ctx(), &answer)).contains("something else"));
    let proof_opts = r.a.proof_options(&p, &ctx()).unwrap();
    let answer = auth.get(&proof_opts, ORIGIN);
    assert!(denied(r.a.passkey_login(&ctx(), None, &answer)).contains("something else"));
}

#[test]
fn a_wrong_origin_and_a_wrong_relying_party_are_refused() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (token, p) = session(&r.a, "ada");
    // Registration from a look-alike origin.
    let mut evil = SoftAuthenticator::es256();
    evil.origin_override = Some("https://obop.test.evil.example".into());
    assert!(denied(add(&r, &token, &p, &mut evil, "k")).contains("different origin"));
    // The right origin on another port is another origin.
    let mut port = SoftAuthenticator::es256();
    port.origin_override = Some("https://obop.test:8443".into());
    assert!(denied(add(&r, &token, &p, &mut port, "k")).contains("different origin"));
    // A relying party id hash for another site.
    let mut rp = SoftAuthenticator::es256();
    rp.rp_id_override = Some("evil.example".into());
    assert!(denied(add(&r, &token, &p, &mut rp, "k")).contains("relying party"));
    assert_eq!(r.a.passkey_count("ada"), 0);
    // And at authentication.
    let mut good = SoftAuthenticator::es256();
    add(&r, &token, &p, &mut good, "k").unwrap();
    good.origin_override = Some("https://evil.example".into());
    let tok = mfa_token(&r.a, "ada");
    let opts = r.a.passkey_login_options(&ctx(), Some(&tok)).unwrap();
    assert!(
        denied(r.a.passkey_login(&ctx(), Some(&tok), &good.get(&opts, ORIGIN))).contains("origin")
    );
    good.origin_override = None;
    good.rp_id_override = Some("evil.example".into());
    let tok = mfa_token(&r.a, "ada");
    let opts = r.a.passkey_login_options(&ctx(), Some(&tok)).unwrap();
    assert!(
        denied(r.a.passkey_login(&ctx(), Some(&tok), &good.get(&opts, ORIGIN)))
            .contains("relying party")
    );
}

#[test]
fn a_credential_registered_for_one_address_is_not_accepted_at_another() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (token, p) = session(&r.a, "ada");
    let mut auth = SoftAuthenticator::es256();
    add(&r, &token, &p, &mut auth, "k").unwrap();
    let other = WebauthnCtx {
        rp_id: "other.test".into(),
        origin: "https://other.test".into(),
        ..ctx()
    };
    let tok = mfa_token(&r.a, "ada");
    let opts = r.a.passkey_login_options(&other, Some(&tok)).unwrap();
    let a = auth.get(&opts, "https://other.test");
    assert!(denied(r.a.passkey_login(&other, Some(&tok), &a)).contains("different address"));
    // A challenge issued for one origin is not valid at another even with a good answer.
    let tok = mfa_token(&r.a, "ada");
    let opts = r.a.passkey_login_options(&ctx(), Some(&tok)).unwrap();
    let a = auth.get(&opts, "https://other.test");
    assert!(r.a.passkey_login(&other, Some(&tok), &a).is_err());
}

#[test]
fn a_counter_that_does_not_go_up_is_a_clone_and_is_refused_and_logged() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (token, p) = session(&r.a, "ada");
    let mut auth = SoftAuthenticator::es256();
    add(&r, &token, &p, &mut auth, "Phone").unwrap();
    auth.counter = 4;
    assert!(is_session(&second_step(&r, "ada", &mut auth).unwrap())); // counter 5
    // The same value again, and a lower one, are both refused.
    for stuck in [5, 2] {
        auth.counter = stuck;
        let tok = mfa_token(&r.a, "ada");
        let opts = r.a.passkey_login_options(&ctx(), Some(&tok)).unwrap();
        let answer = auth.get_with_counter(&opts, ORIGIN);
        assert!(denied(r.a.passkey_login(&ctx(), Some(&tok), &answer)).contains("cloned"));
    }
    let clone = events_of(&r, "passkey_clone");
    assert_eq!(clone.len(), 2);
    assert!(clone[0].contains("Phone"));
    // The stored counter did not move: the genuine device still works after its own count.
    auth.counter = 5;
    assert!(is_session(&second_step(&r, "ada", &mut auth).unwrap()));
}

#[test]
fn an_authenticator_without_a_counter_is_let_through() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (token, p) = session(&r.a, "ada");
    let mut auth = SoftAuthenticator::es256();
    add(&r, &token, &p, &mut auth, "Synced").unwrap();
    for _ in 0..3 {
        let tok = mfa_token(&r.a, "ada");
        let opts = r.a.passkey_login_options(&ctx(), Some(&tok)).unwrap();
        let a = auth.get_with_counter(&opts, ORIGIN); // always zero
        assert!(is_session(
            &r.a.passkey_login(&ctx(), Some(&tok), &a).unwrap()
        ));
    }
}

#[test]
fn another_persons_passkey_does_not_complete_my_sign_in() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    user_with(&r.a, "bob", &["base.user"]);
    let (ta, pa) = session(&r.a, "ada");
    let (tb, pb) = session(&r.a, "bob");
    let (mut ada_key, mut bob_key) = (SoftAuthenticator::es256(), SoftAuthenticator::es256());
    add(&r, &ta, &pa, &mut ada_key, "a").unwrap();
    add(&r, &tb, &pb, &mut bob_key, "b").unwrap();
    // Bob's key answers a challenge made for Ada's sign-in.
    let tok = mfa_token(&r.a, "ada");
    let opts = r.a.passkey_login_options(&ctx(), Some(&tok)).unwrap();
    let m = denied(r.a.passkey_login(&ctx(), Some(&tok), &bob_key.get(&opts, ORIGIN)));
    assert!(m.contains("different account"), "{m}");
    // And a proof inside Ada's session.
    let opts = r.a.proof_options(&pa, &ctx()).unwrap();
    assert!(
        denied(r.a.proof(&pa, &ctx(), &bob_key.get(&opts, ORIGIN))).contains("different account")
    );
    // Registering with a challenge issued to Ada while signed in as Bob.
    let opts = r.a.proof_options(&pa, &ctx()).unwrap();
    r.a.proof(&pa, &ctx(), &ada_key.get(&opts, ORIGIN)).unwrap();
    let opts = r.a.register_options(&pa, &ctx(), &pw()).unwrap();
    let mut third = SoftAuthenticator::es256();
    let cred = third.create(&opts, ORIGIN);
    assert!(
        denied(r.a.register_finish(&pb, &tb, &ctx(), "x", &cred)).contains("different account")
    );
}

#[test]
fn a_user_handle_for_somebody_else_is_refused() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (token, p) = session(&r.a, "ada");
    let mut auth = SoftAuthenticator::es256();
    add(&r, &token, &p, &mut auth, "k").unwrap();
    auth.user_handle = vec![9; 32];
    let tok = mfa_token(&r.a, "ada");
    let opts = r.a.passkey_login_options(&ctx(), Some(&tok)).unwrap();
    assert!(
        denied(r.a.passkey_login(&ctx(), Some(&tok), &auth.get(&opts, ORIGIN)))
            .contains("different account")
    );
}

#[test]
fn a_removed_passkey_no_longer_signs_in() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (token, p) = session(&r.a, "ada");
    // Another session of the person, from before the second factor.
    let (other, _) = session(&r.a, "ada");
    let (mut a1, mut a2) = (
        SoftAuthenticator::es256(),
        SoftAuthenticator::new(ALG_EDDSA),
    );
    add(&r, &token, &p, &mut a1, "one").unwrap();
    // A second key needs the first as proof.
    let opts = r.a.proof_options(&p, &ctx()).unwrap();
    r.a.proof(&p, &ctx(), &a1.get(&opts, ORIGIN)).unwrap();
    add(&r, &token, &p, &mut a2, "two").unwrap();
    let list = r.a.list(&p, &ctx()).unwrap();
    let id = list["passkeys"][0]["id"].as_str().unwrap().to_string();
    let opts = r.a.proof_options(&p, &ctx()).unwrap();
    r.a.proof(&p, &ctx(), &a2.get(&opts, ORIGIN)).unwrap();
    r.a.remove(&p, &token, &id, &pw()).unwrap();
    assert!(r.a.resolve(&other).is_none(), "other sessions end");
    assert!(r.a.resolve(&token).is_some(), "this one stays");
    assert_eq!(r.a.passkey_count("ada"), 1);
    // The removed one is refused: it is not registered, and the challenge is spent anyway.
    let tok = mfa_token(&r.a, "ada");
    let opts = r.a.passkey_login_options(&ctx(), Some(&tok)).unwrap();
    assert!(
        denied(r.a.passkey_login(&ctx(), Some(&tok), &a1.get(&opts, ORIGIN)))
            .contains("not registered")
    );
    assert!(is_session(&second_step(&r, "ada", &mut a2).unwrap()));
    assert_eq!(events_of(&r, "passkey_remove").len(), 1);
}

#[test]
fn a_tampered_signature_or_unknown_credential_is_refused() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (token, p) = session(&r.a, "ada");
    let mut auth = SoftAuthenticator::es256();
    add(&r, &token, &p, &mut auth, "k").unwrap();
    let tok = mfa_token(&r.a, "ada");
    let opts = r.a.passkey_login_options(&ctx(), Some(&tok)).unwrap();
    let mut a = auth.get(&opts, ORIGIN);
    let sig = crate::webauthn::b64url_decode(a["response"]["signature"].as_str().unwrap()).unwrap();
    let mut bad = sig.clone();
    *bad.last_mut().unwrap() ^= 1;
    a["response"]["signature"] = J::from(b64url(&bad));
    assert!(denied(r.a.passkey_login(&ctx(), Some(&tok), &a)).contains("signature"));
    // An authenticator the server never saw.
    let tok = mfa_token(&r.a, "ada");
    let opts = r.a.passkey_login_options(&ctx(), Some(&tok)).unwrap();
    let mut stranger = SoftAuthenticator::es256();
    assert!(
        denied(r.a.passkey_login(&ctx(), Some(&tok), &stranger.get(&opts, ORIGIN)))
            .contains("not registered")
    );
    // Garbage in every position is a refusal, never a panic.
    for junk in [
        json!({}),
        json!({"type": "public-key"}),
        json!(null),
        json!({"type":"public-key","rawId":"AAAA","id":"AAAA","response":{}}),
    ] {
        r.a.clear_failures_for_test("ada");
        let tok = mfa_token(&r.a, "ada");
        let _ = r.a.passkey_login_options(&ctx(), Some(&tok)).unwrap();
        assert!(matches!(
            r.a.passkey_login(&ctx(), Some(&tok), &junk),
            Err(MfaError::Denied(_))
        ));
    }
}

#[test]
fn only_attestation_none_with_a_supported_key_is_accepted() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (token, p) = session(&r.a, "ada");
    let mut auth = SoftAuthenticator::es256();
    let opts = r.a.register_options(&p, &ctx(), &pw()).unwrap();
    let mut cred = auth.create(&opts, ORIGIN);
    // Swap the format for "packed".
    let ao =
        crate::webauthn::b64url_decode(cred["response"]["attestationObject"].as_str().unwrap())
            .unwrap();
    let parsed = cbor::decode(&ao).unwrap();
    let auth_data = parsed.get("authData").unwrap().as_bytes().unwrap().to_vec();
    let mut forged = Vec::new();
    cbor::enc::map(&mut forged, 3);
    cbor::enc::text(&mut forged, "fmt");
    cbor::enc::text(&mut forged, "packed");
    cbor::enc::text(&mut forged, "attStmt");
    cbor::enc::map(&mut forged, 0);
    cbor::enc::text(&mut forged, "authData");
    cbor::enc::bytes(&mut forged, &auth_data);
    cred["response"]["attestationObject"] = J::from(b64url(&forged));
    assert!(denied(r.a.register_finish(&p, &token, &ctx(), "k", &cred)).contains("format none"));
    // Truncated authenticator data.
    let opts = r.a.register_options(&p, &ctx(), &pw()).unwrap();
    let mut cred = auth.create(&opts, ORIGIN);
    let mut short = Vec::new();
    cbor::enc::map(&mut short, 3);
    cbor::enc::text(&mut short, "fmt");
    cbor::enc::text(&mut short, "none");
    cbor::enc::text(&mut short, "attStmt");
    cbor::enc::map(&mut short, 0);
    cbor::enc::text(&mut short, "authData");
    cbor::enc::bytes(&mut short, &auth_data[..40]);
    cred["response"]["attestationObject"] = J::from(b64url(&short));
    assert!(matches!(
        r.a.register_finish(&p, &token, &ctx(), "k", &cred),
        Err(MfaError::Denied(_))
    ));
    assert_eq!(r.a.passkey_count("ada"), 0);
}

#[test]
fn the_same_authenticator_cannot_be_registered_twice_or_by_two_people() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    user_with(&r.a, "bob", &["base.user"]);
    let (ta, pa) = session(&r.a, "ada");
    let (tb, pb) = session(&r.a, "bob");
    let mut auth = SoftAuthenticator::es256();
    add(&r, &ta, &pa, &mut auth, "k").unwrap();
    assert!(matches!(
        add(&r, &tb, &pb, &mut auth, "k"),
        Err(MfaError::Invalid(_))
    ));
    // The first key vouches for Ada; adding the same one again is refused.
    let opts = r.a.proof_options(&pa, &ctx()).unwrap();
    r.a.proof(&pa, &ctx(), &auth.get(&opts, ORIGIN)).unwrap();
    assert!(matches!(
        add(&r, &ta, &pa, &mut auth, "k"),
        Err(MfaError::Invalid(_))
    ));
    // The options for a second key exclude the first.
    let opts = r.a.proof_options(&pa, &ctx()).unwrap();
    r.a.proof(&pa, &ctx(), &auth.get(&opts, ORIGIN)).unwrap();
    let opts = r.a.register_options(&pa, &ctx(), &pw()).unwrap();
    assert_eq!(
        opts["publicKey"]["excludeCredentials"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn managing_passkeys_needs_the_password_and_any_second_factor() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (token, p) = session(&r.a, "ada");
    let wrong = Reauth {
        password: "nope nope nope nope",
        ..Default::default()
    };
    assert!(matches!(
        r.a.register_options(&p, &ctx(), &wrong),
        Err(MfaError::BadPassword)
    ));
    let mut auth = SoftAuthenticator::es256();
    add(&r, &token, &p, &mut auth, "k").unwrap();
    let id = r.a.list(&p, &ctx()).unwrap()["passkeys"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    // Now a second factor is on: the password alone is not enough.
    assert!(matches!(
        r.a.rename(&p, &id, "x", &pw()),
        Err(MfaError::Invalid(_))
    ));
    assert!(matches!(
        r.a.remove(&p, &token, &id, &pw()),
        Err(MfaError::Invalid(_))
    ));
    assert_eq!(r.a.passkey_count("ada"), 1);
    // A passkey proof stands in for the code, once.
    let opts = r.a.proof_options(&p, &ctx()).unwrap();
    r.a.proof(&p, &ctx(), &auth.get(&opts, ORIGIN)).unwrap();
    r.a.rename(&p, &id, "  My   laptop\n", &pw()).unwrap();
    assert_eq!(
        r.a.list(&p, &ctx()).unwrap()["passkeys"][0]["name"],
        "My laptop"
    );
    assert!(
        matches!(r.a.rename(&p, &id, "x", &pw()), Err(MfaError::Invalid(_))),
        "the proof was spent"
    );
    // A recovery code works too.
    let rc = {
        let u = r.b.get("auth_user", "ada").unwrap().unwrap();
        assert_eq!(
            u["recovery"].as_array().unwrap().len(),
            crate::mfa::RECOVERY_CODES
        );
        // The codes were returned once at enrolment; make fresh ones through the passkey proof.
        let opts = r.a.proof_options(&p, &ctx()).unwrap();
        r.a.proof(&p, &ctx(), &auth.get(&opts, ORIGIN)).unwrap();
        r.a.regenerate_recovery(&p, PW, None, None).unwrap()["recovery_codes"][0]
            .as_str()
            .unwrap()
            .to_string()
    };
    let with_recovery = Reauth {
        password: PW,
        recovery_code: Some(&rc),
        ..Default::default()
    };
    r.a.rename(&p, &id, "Renamed", &with_recovery).unwrap();
    assert_eq!(events_of(&r, "passkey_rename").len(), 2);
    // Nobody else's passkey can be named by id.
    user_with(&r.a, "bob", &["base.user"]);
    let (_, pb) = session(&r.a, "bob");
    assert!(matches!(
        r.a.rename(&pb, &id, "mine", &pw()),
        Err(MfaError::Invalid(_))
    ));
}

#[test]
fn the_last_second_factor_of_an_administrator_cannot_be_removed() {
    let r = rig_with(enforcing());
    user_with(&r.a, "root", &[ADMIN_GROUP]);
    // Without a second factor an administrator gets a limited session.
    let (token, p) = session(&r.a, "root");
    assert!(p.mfa_limited);
    let mut auth = SoftAuthenticator::es256();
    add(&r, &token, &p, &mut auth, "Key").unwrap();
    assert!(
        !r.a.resolve(&token).unwrap().mfa_limited,
        "a passkey satisfies the policy"
    );
    // Signing in afterwards is a two-step with the passkey, and the session is not limited.
    let out = second_step(&r, "root", &mut auth).unwrap();
    let LoginOutcome::Session { token: t2, .. } = out else {
        panic!()
    };
    let p2 = r.a.resolve(&t2).unwrap();
    assert!(!p2.mfa_limited);
    let id = r.a.list(&p2, &ctx()).unwrap()["passkeys"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let opts = r.a.proof_options(&p2, &ctx()).unwrap();
    r.a.proof(&p2, &ctx(), &auth.get(&opts, ORIGIN)).unwrap();
    let e = r.a.remove(&p2, &t2, &id, &pw());
    assert!(
        matches!(&e, Err(MfaError::Invalid(m)) if m.contains("requires a second factor")),
        "{e:?}"
    );
    assert_eq!(r.a.passkey_count("root"), 1);
    // With an authenticator app as well, the passkey may go.
    let begun = r.a.begin(&p2).unwrap();
    let secret = crate::totp::base32_decode(begun["secret"].as_str().unwrap()).unwrap();
    r.a.confirm(&p2, &t2, &code_at(&secret, T0)).unwrap();
    let opts = r.a.proof_options(&p2, &ctx()).unwrap();
    r.a.proof(&p2, &ctx(), &auth.get(&opts, ORIGIN)).unwrap();
    r.a.remove(&p2, &t2, &id, &pw()).unwrap();
    assert_eq!(r.a.passkey_count("root"), 0);
    assert!(r.a.mfa_enabled("root"));
}

#[test]
fn removing_a_passkey_that_could_not_end_the_other_sessions_is_finished_by_repeating_it() {
    use crate::admin_tests::{flaky_rig, request};
    use obop_storage::testing::Mode;
    let (r, flaky) = flaky_rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (mine, p) = session(&r.a, "ada");
    let (stolen, _) = session(&r.a, "ada");
    let mut auth = SoftAuthenticator::es256();
    add(&r, &mine, &p, &mut auth, "k").unwrap();
    let id = r.a.list(&p, &ctx()).unwrap()["passkeys"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let opts = r.a.proof_options(&p, &ctx()).unwrap();
    r.a.proof(&p, &ctx(), &auth.get(&opts, ORIGIN)).unwrap();
    // The passkey goes; ending the other sessions is refused.
    flaky.refuse_model("auth_token", Mode::Reads);
    let (done, out) = request(|| r.a.remove(&p, &mine, &id, &pw()));
    flaky.allow_models();
    assert!(matches!(done, Err(MfaError::Unavailable(_))), "{done:?}");
    assert!(out.wrote && out.partial, "{out:?}");
    assert_eq!(r.a.passkey_count("ada"), 0);
    assert!(
        r.a.resolve(&stolen).is_some(),
        "the other session still lives"
    );
    assert!(
        events_of(&r, "passkey_remove")
            .iter()
            .any(|d| d.contains("removed \"k\"") && d.contains("INCOMPLETE")),
        "the entry of the removal itself, with what is missing"
    );
    // The repeat is not "no such passkey": it ends the other session.
    let (done, _) = request(|| r.a.remove(&p, &mine, &id, &pw()));
    done.unwrap();
    assert!(r.a.resolve(&stolen).is_none());
    assert!(r.a.resolve(&mine).is_some());
    // Finished: the usual answer from now on.
    assert!(matches!(
        r.a.remove(&p, &mine, &id, &pw()),
        Err(MfaError::Invalid(m)) if m.contains("no such passkey")
    ));
}

#[test]
fn removing_a_passkey_with_a_refusal_at_any_call_never_panics_and_the_repeat_leaves_nothing_behind()
{
    use crate::admin_tests::{flaky_rig, request};
    // A refused list of the passkeys made `len() - 1` underflow (a panic: the client got a dropped
    // connection), and a refusal between the removal and the user's record left the recovery codes
    // of a factor that is gone. Each call of the removal is refused in turn.
    let mut reached = 0;
    for skip in 0..60 {
        let (r, flaky) = flaky_rig();
        user_with(&r.a, "ada", &["base.user"]);
        let (mine, p) = session(&r.a, "ada");
        let (stolen, _) = session(&r.a, "ada");
        let mut auth = SoftAuthenticator::es256();
        add(&r, &mine, &p, &mut auth, "k").unwrap();
        let id = r.a.list(&p, &ctx()).unwrap()["passkeys"][0]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let prove = |r: &Rig, auth: &mut SoftAuthenticator| {
            let opts = r.a.proof_options(&p, &ctx()).unwrap();
            r.a.proof(&p, &ctx(), &auth.get(&opts, ORIGIN)).unwrap();
        };
        prove(&r, &mut auth);
        flaky.refuse_call_after(skip);
        let (first, out) = request(|| r.a.remove(&p, &mine, &id, &pw()));
        flaky.allow_models();
        if first.is_ok() {
            continue;
        }
        reached += 1;
        // Before anything changed a refused lookup may surface as another error (the layer says
        // 503 for it, a fault was noted); once the removal committed it is the store's.
        assert!(
            matches!(first, Err(MfaError::Unavailable(_))) || (!out.wrote && first.is_err()),
            "skip {skip}: {first:?} {out:?}"
        );
        // The repeat (a proof first, if the passkey is still there) finishes whatever is left.
        if r.a.passkey_count("ada") > 0 {
            prove(&r, &mut auth);
        }
        let (second, _) = request(|| r.a.remove(&p, &mine, &id, &pw()));
        assert!(
            second.is_ok() || r.a.passkey_count("ada") == 0 && !out.wrote,
            "skip {skip}: {second:?}"
        );
        assert_eq!(r.a.passkey_count("ada"), 0, "skip {skip}");
        assert!(
            r.a.resolve(&stolen).is_none(),
            "skip {skip}: the other session is still live"
        );
        assert!(r.a.resolve(&mine).is_some());
        let u = r.b.get("auth_user", "ada").unwrap().unwrap();
        assert!(
            u["recovery"].is_null(),
            "skip {skip}: recovery codes of a factor that is gone"
        );
        assert_eq!(u["passkey_login"], false, "skip {skip}");
    }
    assert!(reached >= 4, "the sweep reached the removal: {reached}");
}

#[test]
fn an_administrators_reset_never_answers_done_while_the_passkey_list_cannot_be_read() {
    use crate::admin_tests::{flaky_rig, request};
    use obop_storage::testing::Mode;
    // A refused lookup of the passkeys was read as "none", and the repeat of an earlier partial
    // reset took the finishing branch on its note: the sessions went, the passkey stayed, and the
    // administrator was told it was done.
    let (r, flaky) = flaky_rig();
    user_with(&r.a, "ada", &["base.user"]);
    user_with(&r.a, "root", &[ADMIN_GROUP]);
    let (_, root) = session(&r.a, "root");
    let (mine, p) = session(&r.a, "ada");
    let (stolen, _) = session(&r.a, "ada");
    add(&r, &mine, &p, &mut SoftAuthenticator::es256(), "k").unwrap();
    // An earlier reset left its note (it could not go on).
    r.a.owe_revocation("ada", "mfa_reset", None).unwrap();
    flaky.refuse_model(crate::passkey::PASSKEY_TABLE, Mode::Reads);
    let (done, _) = request(|| r.a.admin_reset(&root, "ada"));
    flaky.allow_models();
    assert!(matches!(done, Err(MfaError::Unavailable(_))), "{done:?}");
    assert_eq!(r.a.passkey_count("ada"), 1, "the passkey is still there");
    assert!(
        r.a.resolve(&stolen).is_some(),
        "and nothing was ended behind a refusal"
    );
    // With the list readable the reset does its work.
    let (done, _) = request(|| r.a.admin_reset(&root, "ada"));
    done.unwrap();
    assert_eq!(r.a.passkey_count("ada"), 0);
    assert!(r.a.resolve(&stolen).is_none());
}

#[test]
fn a_reset_that_stopped_after_the_passkeys_went_is_finished_by_repeating_it_recovery_codes_too() {
    use crate::admin_tests::{flaky_rig, request};
    // A person whose only second factor is a passkey has no `mfa_enabled` to show that the reset
    // got no further than deleting the passkeys: the repeat took the finishing branch, ended the
    // sessions and answered "done" with the recovery codes, the passwordless flag and the proof
    // still on the account. At every call of the reset in turn, a refusal that lasts.
    let mut reached = 0;
    for skip in 0..30 {
        let (r, flaky) = flaky_rig();
        user_with(&r.a, "ada", &["base.user"]);
        user_with(&r.a, "root", &[ADMIN_GROUP]);
        let (_, root) = session(&r.a, "root");
        let (mine, p) = session(&r.a, "ada");
        let (stolen, _) = session(&r.a, "ada");
        let mut auth = SoftAuthenticator::es256();
        add(&r, &mine, &p, &mut auth, "k").unwrap();
        let prove = |r: &Rig, auth: &mut SoftAuthenticator| {
            let opts = r.a.proof_options(&p, &ctx()).unwrap();
            r.a.proof(&p, &ctx(), &auth.get(&opts, ORIGIN)).unwrap();
        };
        prove(&r, &mut auth);
        r.a.set_passwordless(&p, true, &pw()).unwrap();
        prove(&r, &mut auth);
        flaky.refuse_calls_after(skip, 50);
        let (first, _) = request(|| r.a.admin_reset(&root, "ada"));
        flaky.allow_models();
        if first.is_ok() {
            continue;
        }
        if r.a.passkey_count("ada") == 0 {
            reached += 1;
        }
        let (second, _) = request(|| r.a.admin_reset(&root, "ada"));
        second.unwrap_or_else(|e| panic!("skip {skip}: the repeat must finish it: {e:?}"));
        let u = r.b.get("auth_user", "ada").unwrap().unwrap();
        assert!(
            u["recovery"].is_null(),
            "skip {skip}: recovery codes of a factor that is gone"
        );
        assert_eq!(u["passkey_login"], false, "skip {skip}");
        assert_eq!(r.a.passkey_count("ada"), 0, "skip {skip}");
        assert!(r.a.resolve(&stolen).is_none(), "skip {skip}");
        assert!(
            r.b.kv_get(&crate::passkey::proof_key("ada"))
                .unwrap()
                .is_none(),
            "skip {skip}: the proof"
        );
    }
    assert!(reached >= 2, "the sweep got past the deletion: {reached}");
}

#[test]
fn a_passkey_second_step_gives_no_session_to_an_account_disabled_in_between() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    user_with(&r.a, "root", &[ADMIN_GROUP]);
    let (_, root) = session(&r.a, "root");
    let (mine, p) = session(&r.a, "ada");
    let mut auth = SoftAuthenticator::es256();
    add(&r, &mine, &p, &mut auth, "k").unwrap();
    let tok = mfa_token(&r.a, "ada");
    let opts = r.a.passkey_login_options(&ctx(), Some(&tok)).unwrap();
    let answer = auth.get(&opts, ORIGIN);
    r.a.set_user_disabled(&root, "ada", true).unwrap();
    let before = r.b.list("auth_token").unwrap().len();
    let done = r.a.passkey_login(&ctx(), Some(&tok), &answer);
    assert!(matches!(done, Err(MfaError::Expired)), "{done:?}");
    assert_eq!(r.b.list("auth_token").unwrap().len(), before);
}

#[test]
fn a_passkey_enrolment_whose_limit_could_not_be_lifted_is_never_answered_as_done() {
    use crate::admin_tests::{flaky_rig_with, request};
    // The passkey is saved, and then the step that frees the session from "enrolment only" is
    // refused. It was thrown away: 200 with the recovery codes, and a session that stayed limited
    // with no word of it. Now it is a needed step: a partial 503 that says so.
    let (mut limited_ok, mut partial) = (0, 0);
    for skip in 0..40 {
        let (r, flaky) = flaky_rig_with(enforcing());
        user_with(&r.a, "root", &[ADMIN_GROUP]);
        let (token, p) = session(&r.a, "root");
        assert!(p.mfa_limited, "an administrator must enrol first");
        let mut auth = SoftAuthenticator::es256();
        let opts = r.a.register_options(&p, &ctx(), &pw()).unwrap();
        let cred = auth.create(&opts, ORIGIN);
        flaky.refuse_calls_after(skip, 50);
        let (done, out) = request(|| r.a.register_finish(&p, &token, &ctx(), "k", &cred));
        flaky.allow_models();
        match done {
            Ok(_) => {
                limited_ok += 1;
                assert!(
                    !r.a.resolve(&token).unwrap().mfa_limited,
                    "skip {skip}: answered done, and the session is still limited"
                );
            }
            Err(_) if r.a.passkey_count("root") == 1 => {
                // The passkey is there and the request failed: a change was committed and a fault
                // noted, which the layer answers as partial; the lifting itself says so on its own.
                assert!(out.wrote && out.fault.is_some(), "skip {skip}: {out:?}");
                partial += usize::from(out.partial);
            }
            Err(_) => {}
        }
    }
    assert!(limited_ok >= 1 && partial >= 1, "{limited_ok} {partial}");
}

#[test]
fn removing_the_last_passkey_of_an_ordinary_user_ends_their_two_factor_state() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (token, p) = session(&r.a, "ada");
    let mut auth = SoftAuthenticator::es256();
    add(&r, &token, &p, &mut auth, "k").unwrap();
    let id = r.a.list(&p, &ctx()).unwrap()["passkeys"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let opts = r.a.proof_options(&p, &ctx()).unwrap();
    r.a.proof(&p, &ctx(), &auth.get(&opts, ORIGIN)).unwrap();
    r.a.remove(&p, &token, &id, &pw()).unwrap();
    assert!(!r.a.has_second_factor("ada"));
    assert!(r.b.get("auth_user", "ada").unwrap().unwrap()["recovery"].is_null());
    // Password sign-in works again.
    assert!(matches!(
        r.a.login_step("ada", PW).unwrap(),
        LoginOutcome::Session { .. }
    ));
}

#[test]
fn passwordless_sign_in_needs_the_person_to_allow_it_and_a_verified_discoverable_passkey() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (token, p) = session(&r.a, "ada");
    let mut auth = SoftAuthenticator::es256();
    add(&r, &token, &p, &mut auth, "Phone").unwrap();
    let usernameless = |auth: &mut SoftAuthenticator| {
        let opts = r.a.passkey_login_options(&ctx(), None).unwrap();
        assert_eq!(
            opts["publicKey"]["allowCredentials"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        assert_eq!(opts["publicKey"]["userVerification"], "required");
        r.a.passkey_login(&ctx(), None, &auth.get(&opts, ORIGIN))
    };
    // Off by default.
    assert!(denied(usernameless(&mut auth)).contains("without a password"));
    r.a.set_passwordless(&p, true, &pw()).unwrap_err(); // second factor on: the password alone is not enough
    let opts = r.a.proof_options(&p, &ctx()).unwrap();
    r.a.proof(&p, &ctx(), &auth.get(&opts, ORIGIN)).unwrap();
    r.a.set_passwordless(&p, true, &pw()).unwrap();
    let out = usernameless(&mut auth).unwrap();
    let LoginOutcome::Session {
        token: t,
        mfa_enrolment_required,
        ..
    } = out
    else {
        panic!()
    };
    assert!(!mfa_enrolment_required);
    assert_eq!(r.a.resolve(&t).unwrap().id, "ada");
    assert_eq!(events_of(&r, "passkey_login").len(), 1);
    // A passkey that did not verify the person is refused even when allowed.
    auth.verified = false;
    assert!(denied(usernameless(&mut auth)).contains("did not verify"));
    // A person without a user-verified passkey cannot turn it on.
    user_with(&r.a, "bob", &["base.user"]);
    let (tb, pb) = session(&r.a, "bob");
    let mut weak = SoftAuthenticator::es256();
    weak.verified = false;
    add(&r, &tb, &pb, &mut weak, "key").unwrap();
    let opts = r.a.proof_options(&pb, &ctx()).unwrap();
    r.a.proof(&pb, &ctx(), &weak.get(&opts, ORIGIN)).unwrap();
    assert!(matches!(
        r.a.set_passwordless(&pb, true, &pw()),
        Err(MfaError::Invalid(_))
    ));
    // Turning it off is immediate.
    let opts = r.a.proof_options(&p, &ctx()).unwrap();
    auth.verified = true;
    r.a.proof(&p, &ctx(), &auth.get(&opts, ORIGIN)).unwrap();
    r.a.set_passwordless(&p, false, &pw()).unwrap();
    assert!(denied(usernameless(&mut auth)).contains("without a password"));
}

#[test]
fn passwordless_sign_in_needs_the_user_handle_and_an_enabled_account() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (token, p) = session(&r.a, "ada");
    let mut auth = SoftAuthenticator::es256();
    add(&r, &token, &p, &mut auth, "k").unwrap();
    let opts = r.a.proof_options(&p, &ctx()).unwrap();
    r.a.proof(&p, &ctx(), &auth.get(&opts, ORIGIN)).unwrap();
    r.a.set_passwordless(&p, true, &pw()).unwrap();
    // A usernameless answer without userHandle cannot say who it is for.
    auth.send_user_handle = false;
    let opts = r.a.passkey_login_options(&ctx(), None).unwrap();
    assert!(
        denied(r.a.passkey_login(&ctx(), None, &auth.get(&opts, ORIGIN)))
            .contains("different account")
    );
    auth.send_user_handle = true;
    // A disabled account is refused like a wrong credential.
    let mut f = Fields::new();
    f.insert("disabled".into(), J::Bool(true));
    r.b.update("auth_user", "ada", &f).unwrap();
    let opts = r.a.passkey_login_options(&ctx(), None).unwrap();
    assert!(
        denied(r.a.passkey_login(&ctx(), None, &auth.get(&opts, ORIGIN)))
            .contains("cannot be used")
    );
    assert_eq!(events_of(&r, "login_refused").len(), 1);
}

#[test]
fn a_passkey_proof_replaces_a_code_for_a_password_change() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (token, p) = session(&r.a, "ada");
    let mut auth = SoftAuthenticator::es256();
    add(&r, &token, &p, &mut auth, "k").unwrap();
    let e =
        r.a.change_password(&p, &token, PW, "a brand new passphrase", None, None);
    assert!(
        matches!(e, Err(MfaError::Invalid(_))),
        "no code, no proof: {e:?}"
    );
    let opts = r.a.proof_options(&p, &ctx()).unwrap();
    r.a.proof(&p, &ctx(), &auth.get(&opts, ORIGIN)).unwrap();
    r.a.change_password(&p, &token, PW, "a brand new passphrase", None, None)
        .unwrap();
}

#[test]
fn the_break_glass_reset_and_the_administrators_reset_clear_passkeys_too() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    user_with(&r.a, "bob", &["base.user"]);
    user_with(&r.a, "root", &[ADMIN_GROUP]);
    for u in ["ada", "bob"] {
        let (t, p) = session(&r.a, u);
        add(&r, &t, &p, &mut SoftAuthenticator::es256(), "k").unwrap();
    }
    // Both turn on passwordless sign-in state so the reset has something to clear.
    let mut f = Fields::new();
    f.insert("passkey_login".into(), J::Bool(true));
    r.b.update("auth_user", "ada", &f).unwrap();
    r.a.operator_reset_mfa("ada").unwrap();
    assert_eq!(r.a.passkey_count("ada"), 0);
    assert!(!r.a.has_second_factor("ada"));
    assert!(r.b.get("auth_user", "ada").unwrap().unwrap()["passkey_login"] == false);
    assert!(matches!(
        r.a.login_step("ada", PW).unwrap(),
        LoginOutcome::Session { .. }
    ));
    assert!(
        events_of(&r, "reset")
            .iter()
            .any(|d| d.contains("1 passkey(s) removed"))
    );
    // The administrator's reset removes Bob's passkeys, which count as a second factor to reset.
    let (_, root) = session(&r.a, "root");
    r.a.admin_reset(&root, "bob").unwrap();
    assert_eq!(r.a.passkey_count("bob"), 0);
    assert!(
        matches!(r.a.admin_reset(&root, "bob"), Err(MfaError::Invalid(_))),
        "nothing left to reset"
    );
}

#[test]
fn the_administrators_user_list_counts_passkeys_as_two_factor() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    user_with(&r.a, "root", &[ADMIN_GROUP]);
    let (t, p) = session(&r.a, "ada");
    add(&r, &t, &p, &mut SoftAuthenticator::es256(), "k").unwrap();
    let (_, root) = session(&r.a, "root");
    let users = r.a.users(&root).unwrap();
    let ada = users["users"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["name"] == "ada")
        .unwrap();
    assert_eq!(
        (ada["mfa_enabled"].clone(), ada["passkeys"].clone()),
        (json!(true), json!(1))
    );
}

#[test]
fn credentials_hold_public_keys_and_nothing_secret_and_the_log_holds_no_credential() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (token, p) = session(&r.a, "ada");
    let mut auth = SoftAuthenticator::es256();
    add(&r, &token, &p, &mut auth, "Laptop").unwrap();
    second_step(&r, "ada", &mut auth).unwrap();
    let rows = r.b.list(PASSKEY_TABLE).unwrap();
    assert_eq!(rows.len(), 1);
    let (id, rec) = &rows[0];
    assert_eq!(id.len(), 64, "keyed by the SHA-256 of the credential id");
    let keys: Vec<&String> = rec.keys().collect();
    for k in [
        "user",
        "name",
        "cred_id",
        "alg",
        "key",
        "sign_count",
        "rp_id",
    ] {
        assert!(keys.iter().any(|x| *x == k), "{k}");
    }
    assert_eq!(
        rec["key"].as_str().unwrap().len(),
        87,
        "an uncompressed P-256 point, base64url"
    );
    let log = format!("{:?}", r.b.list(EVENT_TABLE).unwrap());
    assert!(
        !log.contains(rec["cred_id"].as_str().unwrap())
            && !log.contains(rec["key"].as_str().unwrap())
    );
    // The key is the user handle's neighbour only by random id, never the account name.
    let u = r.b.get("auth_user", "ada").unwrap().unwrap();
    assert!(!u["webauthn_handle"].as_str().unwrap().contains("ada"));
}

#[test]
fn passkeys_are_unavailable_on_an_ip_address_or_without_https() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (_, p) = session(&r.a, "ada");
    let ip = WebauthnCtx {
        rp_id: "192.0.2.7".into(),
        origin: "https://192.0.2.7".into(),
        ..ctx()
    };
    let plain = WebauthnCtx {
        rp_id: "obop.test".into(),
        origin: "http://obop.test".into(),
        ..ctx()
    };
    for c in [&ip, &plain] {
        assert!(matches!(
            r.a.register_options(&p, c, &pw()),
            Err(MfaError::Unavailable(_))
        ));
        assert_eq!(r.a.list(&p, c).unwrap()["available"], false);
    }
    // localhost over http is the development exception.
    let local = WebauthnCtx {
        rp_id: "localhost".into(),
        origin: "http://localhost:3000".into(),
        ..ctx()
    };
    r.a.register_options(&p, &local, &pw()).unwrap();
}

#[test]
fn ceremonies_are_rate_limited_per_client_address() {
    let r = rig();
    for _ in 0..60 {
        r.a.passkey_login_options(&ctx(), None).unwrap();
    }
    assert!(matches!(
        r.a.passkey_login_options(&ctx(), None),
        Err(MfaError::Locked(_))
    ));
    let other = WebauthnCtx {
        ip: "198.51.100.1".into(),
        ..ctx()
    };
    r.a.passkey_login_options(&other, None).unwrap();
}

#[test]
fn repeated_wrong_answers_lock_the_second_step_like_wrong_codes() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (token, p) = session(&r.a, "ada");
    let mut auth = SoftAuthenticator::es256();
    add(&r, &token, &p, &mut auth, "k").unwrap();
    let mut stranger = SoftAuthenticator::es256();
    let tok = mfa_token(&r.a, "ada");
    for _ in 0..crate::mfa::MFA_TOKEN_ATTEMPTS {
        let opts = r.a.passkey_login_options(&ctx(), Some(&tok)).unwrap();
        assert!(
            r.a.passkey_login(&ctx(), Some(&tok), &stranger.get(&opts, ORIGIN))
                .is_err()
        );
    }
    // The pending sign-in is spent or the person is locked; either way the real key is refused now.
    let opts = r.a.passkey_login_options(&ctx(), Some(&tok));
    if let Ok(opts) = opts {
        assert!(matches!(
            r.a.passkey_login(&ctx(), Some(&tok), &auth.get(&opts, ORIGIN)),
            Err(MfaError::Locked(_))
        ));
    }
    assert!(!events_of(&r, "passkey_failed").is_empty());
}

#[test]
fn at_most_ten_passkeys() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (token, p) = session(&r.a, "ada");
    let first = add(&r, &token, &p, &mut SoftAuthenticator::es256(), "k0").unwrap();
    let rc = first["recovery_codes"][0].as_str().unwrap().to_string();
    // Nine more, stored directly.
    for i in 1..crate::MAX_PASSKEYS {
        let mut f = Fields::new();
        f.insert("user".into(), J::from("ada"));
        f.insert("name".into(), J::from(format!("k{i}")));
        r.b.insert(PASSKEY_TABLE, &format!("{i:064}"), &f).unwrap();
    }
    assert_eq!(r.a.passkey_count("ada"), crate::MAX_PASSKEYS);
    let re = Reauth {
        password: PW,
        recovery_code: Some(&rc),
        ..Default::default()
    };
    assert!(matches!(
        r.a.register_options(&p, &ctx(), &re),
        Err(MfaError::Invalid(m)) if m.contains("at most 10")
    ));
}

#[test]
fn agents_have_no_passkeys() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (_, p) = session(&r.a, "ada");
    let t =
        r.a.issue_agent_token(&p, &["base.user".into()], "bot")
            .unwrap();
    let agent = r.a.resolve(&t).unwrap();
    assert!(matches!(r.a.list(&agent, &ctx()), Err(MfaError::NotFound)));
    assert!(matches!(
        r.a.register_options(&agent, &ctx(), &pw()),
        Err(MfaError::NotFound)
    ));
}

#[test]
fn totp_and_passkeys_coexist_and_either_completes_the_second_step() {
    let r = rig();
    user_with(&r.a, "ada", &["base.user"]);
    let (token, p) = session(&r.a, "ada");
    let (secret, _) = enrol(&r.a, "ada");
    // Adding a passkey needs the code now.
    let code = code_at(&secret, T0 + 30);
    r.a.set_clock_for_test(T0 + 30);
    let re = Reauth {
        password: PW,
        code: Some(&code),
        recovery_code: None,
    };
    let mut auth = SoftAuthenticator::es256();
    let opts = r.a.register_options(&p, &ctx(), &re).unwrap();
    let cred = auth.create(&opts, ORIGIN);
    let done =
        r.a.register_finish(&p, &token, &ctx(), "Key", &cred)
            .unwrap();
    assert!(
        done.get("recovery_codes").is_none(),
        "recovery codes already exist"
    );
    let tok = mfa_token(&r.a, "ada");
    assert_eq!(r.a.methods(&tok), vec!["totp", "passkey"]);
    // The passkey completes it.
    let opts = r.a.passkey_login_options(&ctx(), Some(&tok)).unwrap();
    assert!(is_session(
        &r.a.passkey_login(&ctx(), Some(&tok), &auth.get(&opts, ORIGIN))
            .unwrap()
    ));
    // The code completes another.
    r.a.set_clock_for_test(T0 + 90);
    let tok = mfa_token(&r.a, "ada");
    let c = code_at(&secret, T0 + 90);
    assert!(matches!(
        r.a.login_mfa(&tok, Some(&c), None).unwrap(),
        LoginOutcome::Session { .. }
    ));
    // Turning the authenticator app off keeps the passkey and the recovery codes.
    r.a.set_clock_for_test(T0 + 150);
    let opts = r.a.proof_options(&p, &ctx()).unwrap();
    r.a.proof(&p, &ctx(), &auth.get(&opts, ORIGIN)).unwrap();
    r.a.disable(&p, PW, None, None).unwrap();
    assert!(!r.a.mfa_enabled("ada") && r.a.has_second_factor("ada"));
    assert!(!r.b.get("auth_user", "ada").unwrap().unwrap()["recovery"].is_null());
}

#[test]
fn a_ceremony_started_by_one_server_process_finishes_on_another() {
    // Two `Auth` values on one backend stand for two processes on one database (D59).
    let b = obop_storage::open("memory:").unwrap();
    let (a1, a2) = (
        Auth::with_test_params(b.clone()),
        Auth::with_test_params(b.clone()),
    );
    for a in [&a1, &a2] {
        a.set_clock_for_test(T0);
    }
    a1.create_user("ada", PW, &["base.user".to_string()])
        .unwrap();
    let LoginOutcome::Session { token, .. } = a1.login_step("ada", PW).unwrap() else {
        panic!()
    };
    let p = a1.resolve(&token).unwrap();
    let mut auth = SoftAuthenticator::es256();
    let opts = a1.register_options(&p, &ctx(), &pw()).unwrap();
    let cred = auth.create(&opts, ORIGIN);
    a2.register_finish(&p, &token, &ctx(), "k", &cred).unwrap();
    // Sign-in the other way round, and the spent challenge is spent for both.
    let LoginOutcome::MfaRequired { mfa_token: tok } = a2.login_step("ada", PW).unwrap() else {
        panic!()
    };
    let opts = a2.passkey_login_options(&ctx(), Some(&tok)).unwrap();
    let answer = auth.get(&opts, ORIGIN);
    assert!(matches!(
        a1.passkey_login(&ctx(), Some(&tok), &answer).unwrap(),
        LoginOutcome::Session { .. }
    ));
    assert!(a2.passkey_login(&ctx(), Some(&tok), &answer).is_err());
}

/// The same ceremonies on a real PostgreSQL: `OBOP_TEST_POSTGRES_URL=postgres://... cargo test -p obop-auth`.
#[test]
fn passkeys_work_on_postgres_when_available() {
    let Ok(url) = std::env::var("OBOP_TEST_POSTGRES_URL") else {
        eprintln!("skipped: set OBOP_TEST_POSTGRES_URL to run against PostgreSQL");
        return;
    };
    let b = obop_storage::open(&url).unwrap();
    let a = Auth::with_test_config(b.clone(), AuthConfig::insecure_dev());
    a.set_clock_for_test(T0);
    let user = format!("pgpk{}", std::process::id());
    a.create_user(&user, PW, &["base.user".to_string()])
        .unwrap();
    let c = WebauthnCtx {
        ip: format!("pg-{}", std::process::id()),
        ..ctx()
    };
    let LoginOutcome::Session { token, .. } = a.login_step(&user, PW).unwrap() else {
        panic!()
    };
    let p = a.resolve(&token).unwrap();
    let mut auth = SoftAuthenticator::new(ALG_ES256);
    let opts = a.register_options(&p, &c, &pw()).unwrap();
    let done = a
        .register_finish(&p, &token, &c, "pg key", &auth.create(&opts, ORIGIN))
        .unwrap();
    assert!(done["recovery_codes"].is_array());
    let LoginOutcome::MfaRequired { mfa_token: tok } = a.login_step(&user, PW).unwrap() else {
        panic!()
    };
    let opts = a.passkey_login_options(&c, Some(&tok)).unwrap();
    let answer = auth.get(&opts, ORIGIN);
    assert!(matches!(
        a.passkey_login(&c, Some(&tok), &answer).unwrap(),
        LoginOutcome::Session { .. }
    ));
    // A replay finds the challenge used up, a counter regression is refused.
    assert!(a.passkey_login(&c, Some(&tok), &answer).is_err());
    auth.counter = 0;
    let LoginOutcome::MfaRequired { mfa_token: tok } = a.login_step(&user, PW).unwrap() else {
        panic!()
    };
    let opts = a.passkey_login_options(&c, Some(&tok)).unwrap();
    let e = a.passkey_login(&c, Some(&tok), &auth.get_with_counter(&opts, ORIGIN));
    assert!(matches!(e, Err(MfaError::Denied(m)) if m.contains("cloned")));
    a.operator_reset_mfa(&user).unwrap();
    assert_eq!(a.passkey_count(&user), 0);
}
