//! The id token checks of D91 as a table: every row is a way a token can be wrong, and every one
//! must be refused; the first row is the token that must pass. A check that is left out of
//! `verify` makes exactly its row fail (see the mutation list in the pull request).

use super::soft::{Entra, StandIn};
use super::verify::*;
use crate::webauthn::{b64url, b64url_decode};
use serde_json::{Value as J, json};

const TENANT: &str = "11111111-2222-3333-4444-555555555555";
const OTHER_TENANT: &str = "99999999-2222-3333-4444-555555555555";
const CLIENT: &str = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
const OID: &str = "0f0f0f0f-1111-2222-3333-444444444444";
const NONCE: &str = "nonce-of-this-sign-in";
const NOW: u64 = 1_800_000_000;

fn expect() -> Expect<'static> {
    Expect {
        authority: AUTHORITY,
        tenant: TENANT,
        client_id: CLIENT,
        nonce: NONCE,
        now: NOW,
    }
}

fn claims() -> J {
    StandIn::claims(AUTHORITY, TENANT, CLIENT, NONCE, OID, NOW)
}

fn keys(s: &StandIn) -> Jwks {
    Jwks::parse(s.jwks().as_bytes()).unwrap()
}

fn remove(c: &mut J, k: &str) {
    c.as_object_mut().unwrap().remove(k);
}

fn refused(r: Result<obop_commands::SsoIdentity, VerifyError>, why: &str) {
    match r {
        Err(VerifyError::Refused(w)) => {
            assert!(
                w.contains(why),
                "refused for {w:?}, wanted a reason with {why:?}"
            )
        }
        other => panic!("wanted a refusal about {why:?}, got {other:?}"),
    }
}

/// Change the claims and sign them again with the stand-in key.
fn with(f: impl FnOnce(&mut J)) -> (StandIn, String) {
    let s = StandIn::new();
    let mut c = claims();
    f(&mut c);
    let t = s.token(&c);
    (s, t)
}

fn check(f: impl FnOnce(&mut J), why: &str) {
    let (s, t) = with(f);
    refused(verify_id_token(&t, &keys(&s), &expect()), why);
}

/// A GUID is the same GUID in any case: the pinned tenant and the binding key are made from the
/// lower-case form, so a token that spells the tenant or the object id in capitals is the same
/// sign-in (and an identity is never two bindings). The tenant here has letters in it, so that
/// capitals are not the same text.
#[test]
fn the_tenant_and_the_object_id_are_compared_as_guids_not_as_text() {
    const LETTERS: &str = "abcdef01-2222-3333-4444-555555555555";
    let s = StandIn::new();
    let mut c = StandIn::claims(AUTHORITY, LETTERS, CLIENT, NONCE, OID, NOW);
    c["tid"] = J::from(LETTERS.to_uppercase());
    c["oid"] = J::from(OID.to_uppercase());
    assert_ne!(c["tid"], LETTERS);
    assert_ne!(c["oid"], OID);
    let exp = Expect {
        tenant: LETTERS,
        ..expect()
    };
    let id = verify_id_token(&s.token(&c), &keys(&s), &exp).unwrap();
    assert_eq!(id.tenant, LETTERS);
    assert_eq!(id.oid, OID);
}

#[test]
fn a_good_token_passes_and_says_who_and_how() {
    let s = StandIn::new();
    let id = verify_id_token(&s.token(&claims()), &keys(&s), &expect()).unwrap();
    assert_eq!(id.oid, OID);
    assert_eq!(id.tenant, TENANT);
    assert_eq!(id.issuer, format!("{AUTHORITY}/{TENANT}/v2.0"));
    assert!(id.mfa());
    assert_eq!(id.auth_time, Some(NOW));
}

#[test]
fn the_tenant_is_pinned_the_issuer_and_the_tid() {
    // A token of another tenant, correctly signed, with its own issuer.
    check(
        |c| {
            c["tid"] = J::from(OTHER_TENANT);
            c["iss"] = J::from(format!("{AUTHORITY}/{OTHER_TENANT}/v2.0"));
        },
        "issuer",
    );
    // The right issuer but a tid of another tenant.
    check(|c| c["tid"] = J::from(OTHER_TENANT), "pinned tenant");
    // The multi-tenant authorities as the issuer.
    for a in ["common", "organizations", "consumers"] {
        check(
            |c| c["iss"] = J::from(format!("{AUTHORITY}/{a}/v2.0")),
            "issuer",
        );
    }
    // The v1 issuer format, a trailing slash, another host, another version, no issuer.
    check(
        |c| c["iss"] = J::from(format!("https://sts.windows.net/{TENANT}/")),
        "issuer",
    );
    check(
        |c| c["iss"] = J::from(format!("{AUTHORITY}/{TENANT}/v2.0/")),
        "issuer",
    );
    check(
        |c| {
            c["iss"] = J::from(format!(
                "https://login.microsoftonline.com.evil.test/{TENANT}/v2.0"
            ))
        },
        "issuer",
    );
    check(
        |c| c["iss"] = J::from(format!("{AUTHORITY}/{TENANT}/v1.0")),
        "issuer",
    );
    check(|c| remove(c, "iss"), "issuer");
    check(|c| remove(c, "tid"), "tenant id");
    check(|c| c["tid"] = J::from("not-a-guid"), "tenant id");
}

#[test]
fn the_tenant_in_the_configuration_is_a_guid_and_never_a_multi_tenant_authority() {
    assert_eq!(tenant_id(TENANT).unwrap(), TENANT);
    assert_eq!(
        tenant_id("AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE").unwrap(),
        "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
    );
    assert_eq!(tenant_id(&format!("  {TENANT}\n")).unwrap(), TENANT);
    for bad in [
        "common",
        "Common",
        "organizations",
        "consumers",
        // The GUID form of `consumers`: every personal Microsoft account.
        "9188040d-6c67-4c5b-b112-36a304b66dad",
        "9188040D-6C67-4C5B-B112-36A304B66DAD",
        "contoso.onmicrosoft.com",
        "",
        "11111111-2222-3333-4444-55555555555",
        "11111111-2222-3333-4444-5555555555555",
        "1111111122223333444455555555555555ff",
        "{11111111-2222-3333-4444-555555555555}",
        "11111111-2222-3333-4444-55555555555g",
    ] {
        assert!(tenant_id(bad).is_err(), "{bad:?} must be refused");
    }
    assert!(tenant_id("common").unwrap_err().contains("multi-tenant"));
}

#[test]
fn the_authority_is_microsofts_or_in_a_test_a_loopback_address() {
    assert!(authority_ok(AUTHORITY));
    assert!(authority_ok("http://127.0.0.1:4455"));
    assert!(authority_ok("http://[::1]:4455"));
    for bad in [
        "http://login.microsoftonline.com",
        "https://login.microsoftonline.com/",
        "https://login.microsoftonline.com/common",
        "https://login.microsoftonline.us",
        "https://127.0.0.1:4455",
        "http://127.0.0.1",
        "http://localhost:4455",
        "http://127.0.0.1:4455/path",
        "http://user@127.0.0.1:4455",
        "http://10.0.0.1:4455",
        "http://127.0.0.1:44555555",
    ] {
        assert!(!authority_ok(bad), "{bad:?} must be refused");
    }
}

#[test]
fn audience_and_authorized_party_are_this_client() {
    check(|c| c["aud"] = J::from("someone-else"), "audience");
    check(|c| c["aud"] = J::from(OID), "audience");
    check(|c| remove(c, "aud"), "audience");
    check(|c| c["aud"] = json!([CLIENT, "someone-else"]), "audience");
    check(|c| c["aud"] = json!([]), "audience");
    check(|c| c["aud"] = json!(7), "audience");
    check(|c| c["azp"] = J::from("someone-else"), "authorized party");
    // One audience in a list, and an azp that agrees, are fine.
    let (s, t) = with(|c| {
        c["aud"] = json!([CLIENT]);
        c["azp"] = J::from(CLIENT);
    });
    assert!(verify_id_token(&t, &keys(&s), &expect()).is_ok());
}

#[test]
fn the_nonce_is_the_one_of_this_sign_in() {
    check(|c| c["nonce"] = J::from("another"), "nonce");
    check(|c| c["nonce"] = J::from(format!("{NONCE}x")), "nonce");
    check(|c| remove(c, "nonce"), "nonce");
    check(|c| c["nonce"] = J::from(7), "nonce");
    // A flow that has no nonce (it must always have one) accepts nothing.
    let s = StandIn::new();
    let mut c = claims();
    c["nonce"] = J::from("");
    let e = Expect {
        nonce: "",
        ..expect()
    };
    refused(verify_id_token(&s.token(&c), &keys(&s), &e), "nonce");
}

#[test]
fn the_times_are_checked_with_a_small_skew() {
    // Expired, just inside the skew (still accepted), just outside, missing, not a number.
    check(|c| c["exp"] = J::from(NOW - SKEW_SECS), "expired");
    check(|c| c["exp"] = J::from(NOW - 3600), "expired");
    let (s, t) = with(|c| c["exp"] = J::from(NOW - SKEW_SECS + 1));
    assert!(verify_id_token(&t, &keys(&s), &expect()).is_ok());
    check(|c| remove(c, "exp"), "no expiry");
    check(|c| c["exp"] = J::from("soon"), "number");
    // Not yet valid.
    check(|c| c["nbf"] = J::from(NOW + SKEW_SECS + 1), "not valid yet");
    let (s, t) = with(|c| c["nbf"] = J::from(NOW + SKEW_SECS));
    assert!(verify_id_token(&t, &keys(&s), &expect()).is_ok());
    // Issued in the future.
    check(|c| c["iat"] = J::from(NOW + SKEW_SECS + 1), "future");
    // Time claims as floats are read as the whole numbers they are.
    let (s, t) = with(|c| c["exp"] = J::from((NOW + 3600) as f64));
    assert!(verify_id_token(&t, &keys(&s), &expect()).is_ok());
}

#[test]
fn the_object_id_is_a_guid_and_is_required() {
    check(|c| remove(c, "oid"), "object id");
    check(|c| c["oid"] = J::from("ada@example.test"), "object id");
    check(|c| c["oid"] = J::from(""), "object id");
    check(|c| c["oid"] = J::from(7), "object id");
    // The subject is not what binds: a token with only `sub` is refused.
    check(
        |c| {
            remove(c, "oid");
            c["sub"] = J::from(OID);
        },
        "object id",
    );
    // Upper case is the same identity.
    let (s, t) = with(|c| c["oid"] = J::from(OID.to_uppercase()));
    assert_eq!(verify_id_token(&t, &keys(&s), &expect()).unwrap().oid, OID);
}

#[test]
fn amr_is_a_list_and_absence_is_not_mfa() {
    let id = |f: fn(&mut J)| {
        let (s, t) = with(f);
        verify_id_token(&t, &keys(&s), &expect()).unwrap()
    };
    assert!(id(|_| {}).mfa());
    assert!(!id(|c| remove(c, "amr")).mfa());
    assert!(!id(|c| c["amr"] = json!(["pwd"])).mfa());
    assert!(!id(|c| c["amr"] = json!([])).mfa());
    // Not a list: says nothing about a second factor, however it is spelled.
    assert!(!id(|c| c["amr"] = J::from("mfa")).mfa());
    assert!(!id(|c| c["amr"] = json!({"mfa": true})).mfa());
    assert!(!id(|c| c["amr"] = json!([true, 1, null])).mfa());
    assert!(
        !id(|c| c["amr"] = json!(["MFA"])).mfa(),
        "the value is exactly mfa"
    );
    // A malformed auth_time is dropped, not fatal and not trusted.
    assert_eq!(
        id(|c| c["auth_time"] = J::from("yesterday")).auth_time,
        None
    );
}

#[test]
fn the_algorithm_is_fixed_by_the_server() {
    let s = StandIn::new();
    let k = keys(&s);
    let signing_input = |header: J| {
        format!(
            "{}.{}",
            b64url(header.to_string().as_bytes()),
            b64url(claims().to_string().as_bytes())
        )
    };
    // none, with an empty signature and with a made-up one.
    let none = signing_input(json!({"alg": "none", "kid": s.kid}));
    refused(verify_id_token(&format!("{none}."), &k, &expect()), "empty");
    refused(
        verify_id_token(&format!("{none}.AAAA"), &k, &expect()),
        "algorithm",
    );
    // HS256 with the public key as the secret: the attack on servers that believe the header.
    let hs = signing_input(json!({"alg": "HS256", "kid": s.kid}));
    let mac = ring::hmac::sign(
        &ring::hmac::Key::new(ring::hmac::HMAC_SHA256, &k.keys[0].der),
        hs.as_bytes(),
    );
    let tag = b64url(mac.as_ref());
    refused(
        verify_id_token(&format!("{hs}.{tag}"), &k, &expect()),
        "algorithm",
    );
    // Other algorithms and spellings, even with a signature that would verify under RS256.
    for alg in [
        "RS384", "RS512", "PS256", "ES256", "EdDSA", "rs256", "RS256 ", "", "HS256",
    ] {
        let t = s.sign(&json!({"alg": alg, "kid": s.kid}), &claims());
        refused(verify_id_token(&t, &k, &expect()), "algorithm");
    }
    let t = s.sign(&json!({"kid": s.kid}), &claims());
    refused(verify_id_token(&t, &k, &expect()), "algorithm");
    let t = s.sign(&json!({"alg": ["RS256"], "kid": s.kid}), &claims());
    refused(verify_id_token(&t, &k, &expect()), "algorithm");
    // Critical extensions are not understood, so the token is not accepted.
    let t = s.sign(
        &json!({"alg": "RS256", "kid": s.kid, "crit": ["exp"]}),
        &claims(),
    );
    refused(verify_id_token(&t, &k, &expect()), "critical");
}

#[test]
fn keys_come_from_the_pinned_key_set_by_kid_and_nowhere_else() {
    let s = StandIn::new();
    let k = keys(&s);
    // No kid, an empty kid, a kid that is not a string.
    for kid in [J::Null, J::from(""), J::from(7)] {
        let mut h = json!({"alg": "RS256"});
        if !kid.is_null() {
            h["kid"] = kid;
        }
        refused(
            verify_id_token(&s.sign(&h, &claims()), &k, &expect()),
            "names no key",
        );
    }
    // An unknown kid asks for the key set to be fetched again.
    let t = s.sign(&json!({"alg": "RS256", "kid": "rotated-key"}), &claims());
    assert_eq!(
        verify_id_token(&t, &k, &expect()),
        Err(VerifyError::UnknownKey)
    );
    // A key carried in the header is never used, whatever it says.
    let jwk = s.jwk_for("rotated-key");
    let t = s.sign(
        &json!({"alg": "RS256", "kid": "rotated-key", "jwk": jwk, "jku": "https://evil.test/keys", "x5u": "https://evil.test/x"}),
        &claims(),
    );
    assert_eq!(
        verify_id_token(&t, &k, &expect()),
        Err(VerifyError::UnknownKey)
    );
    // Two keys of the same name: which one signed is not told by the name, so nothing verifies.
    let dup = StandIn::jwks_of(&[s.jwk_for(&s.kid), s.jwk_for(&s.kid)]);
    refused(
        verify_id_token(
            &s.token(&claims()),
            &Jwks::parse(dup.as_bytes()).unwrap(),
            &expect(),
        ),
        "same kid",
    );
    // A key marked for encryption, or for another algorithm, is not a key for this.
    let mut enc = s.jwk_for(&s.kid);
    enc["use"] = J::from("enc");
    let set = Jwks::parse(StandIn::jwks_of(&[enc.clone(), s.jwk_for("other")]).as_bytes()).unwrap();
    assert_eq!(
        verify_id_token(&s.token(&claims()), &set, &expect()),
        Err(VerifyError::UnknownKey)
    );
    enc["use"] = J::from("sig");
    enc["alg"] = J::from("RS512");
    let set = Jwks::parse(StandIn::jwks_of(&[enc, s.jwk_for("other")]).as_bytes()).unwrap();
    assert_eq!(
        verify_id_token(&s.token(&claims()), &set, &expect()),
        Err(VerifyError::UnknownKey)
    );
    // Another key under the right name: the signature does not verify.
    let wrong = Jwks::parse(s.jwks_with_wrong_modulus().as_bytes()).unwrap();
    refused(
        verify_id_token(&s.token(&claims()), &wrong, &expect()),
        "signature",
    );
}

#[test]
fn a_changed_or_cut_token_does_not_verify() {
    let s = StandIn::new();
    let k = keys(&s);
    let good = s.token(&claims());
    let mut parts = good.split('.').map(String::from).collect::<Vec<_>>();
    // Truncated signature, one byte flipped, a signature of the wrong length, an empty one.
    let sig = b64url_decode(&parts[2]).unwrap();
    let cut = format!(
        "{}.{}.{}",
        parts[0],
        parts[1],
        b64url(&sig[..sig.len() - 4])
    );
    refused(verify_id_token(&cut, &k, &expect()), "signature");
    let mut flipped = sig.clone();
    flipped[10] ^= 1;
    let t = format!("{}.{}.{}", parts[0], parts[1], b64url(&flipped));
    refused(verify_id_token(&t, &k, &expect()), "signature");
    let t = format!("{}.{}.{}", parts[0], parts[1], b64url(&[0u8; 256]));
    refused(verify_id_token(&t, &k, &expect()), "signature");
    let t = format!("{}.{}.", parts[0], parts[1]);
    refused(verify_id_token(&t, &k, &expect()), "empty");
    let t = format!("{}.{}.not base64!", parts[0], parts[1]);
    refused(verify_id_token(&t, &k, &expect()), "base64url");
    // Claims changed after signing: the signature covers the text as it was.
    let mut other = claims();
    other["oid"] = J::from("77777777-1111-2222-3333-444444444444");
    parts[1] = b64url(other.to_string().as_bytes());
    refused(
        verify_id_token(&parts.join("."), &k, &expect()),
        "signature",
    );
    // The header changed (the key name) after signing.
    let mut parts = good.split('.').map(String::from).collect::<Vec<_>>();
    parts[0] = b64url(
        json!({"alg": "RS256", "kid": "rotated-key"})
            .to_string()
            .as_bytes(),
    );
    assert_eq!(
        verify_id_token(&parts.join("."), &k, &expect()),
        Err(VerifyError::UnknownKey)
    );
}

#[test]
fn only_a_compact_jws_of_three_parts_is_read() {
    let s = StandIn::new();
    let k = keys(&s);
    let good = s.token(&claims());
    refused(verify_id_token("", &k, &expect()), "three parts");
    refused(verify_id_token("a.b", &k, &expect()), "three parts");
    refused(
        verify_id_token(&format!("{good}.x"), &k, &expect()),
        "three parts",
    );
    refused(verify_id_token("a.b.c.d.e", &k, &expect()), "three parts");
    refused(verify_id_token("..", &k, &expect()), "empty");
    // The header is not JSON, or JSON that is not an object.
    refused(verify_id_token("e30.e30.AAAA", &k, &expect()), "algorithm");
    let arr = b64url(b"[1]");
    refused(
        verify_id_token(&format!("{arr}.e30.AAAA"), &k, &expect()),
        "object",
    );
    refused(verify_id_token("!!!.e30.AAAA", &k, &expect()), "base64url");
    // Size: refused before anything is decoded.
    let big = format!("{}.b.c", "a".repeat(MAX_TOKEN_BYTES));
    refused(verify_id_token(&big, &k, &expect()), "too large");
}

#[test]
fn the_key_set_keeps_only_keys_that_can_verify_rs256() {
    let s = StandIn::new();
    let good = s.jwk_for("good");
    let mut short = s.jwk_for("short");
    short["n"] = J::from(b64url(&[0xffu8; 128])); // 1024 bits
    let mut ec = s.jwk_for("ec");
    ec["kty"] = J::from("EC");
    let mut noverify = s.jwk_for("noverify");
    noverify["key_ops"] = json!(["encrypt"]);
    let mut bigexp = s.jwk_for("bigexp");
    bigexp["e"] = J::from(b64url(&[1u8, 0, 0, 0, 1]));
    let mut noid = s.jwk_for("x");
    noid["kid"] = J::Null;
    let mut other_alg = s.jwk_for("rs384");
    other_alg["alg"] = J::from("RS384");
    let mut encryption = s.jwk_for("enc");
    encryption["use"] = J::from("enc");
    // The key as Microsoft publishes it carries no `alg` and a certificate chain: it stays.
    let good = {
        assert!(good.get("alg").is_none() && good.get("x5c").is_some());
        good
    };
    let set = Jwks::parse(
        StandIn::jwks_of(&[
            good, short, ec, noverify, bigexp, noid, other_alg, encryption,
        ])
        .as_bytes(),
    )
    .unwrap();
    assert_eq!(
        set.keys.iter().map(|k| k.kid.as_str()).collect::<Vec<_>>(),
        ["good"]
    );
    let bad: [&[u8]; 6] = [
        b"",
        b"not json",
        b"{}",
        b"{\"keys\": 5}",
        b"{\"keys\": []}",
        br#"{"keys": [{"kty": "oct", "kid": "x", "k": "AAAA"}]}"#,
    ];
    for doc in bad {
        assert!(
            Jwks::parse(doc).is_err(),
            "{:?}",
            String::from_utf8_lossy(doc)
        );
    }
    let many = (0..=100)
        .map(|i| s.jwk_for(&format!("k{i}")))
        .collect::<Vec<_>>();
    assert!(Jwks::parse(StandIn::jwks_of(&many).as_bytes()).is_err());
    assert!(Jwks::parse(&vec![b' '; MAX_JWKS_BYTES + 1]).is_err());
}

#[test]
fn the_discovery_document_is_the_pinned_tenants_and_points_only_under_it() {
    let s = StandIn::new();
    let ok = parse_discovery(s.discovery(AUTHORITY, TENANT).as_bytes(), AUTHORITY, TENANT).unwrap();
    assert_eq!(
        ok.authorization,
        format!("{AUTHORITY}/{TENANT}/oauth2/v2.0/authorize")
    );
    assert_eq!(ok.token, format!("{AUTHORITY}/{TENANT}/oauth2/v2.0/token"));
    assert_eq!(ok.jwks, format!("{AUTHORITY}/{TENANT}/discovery/v2.0/keys"));
    let doc = |f: &dyn Fn(&mut J)| {
        let mut d: J = serde_json::from_str(&s.discovery(AUTHORITY, TENANT)).unwrap();
        f(&mut d);
        parse_discovery(d.to_string().as_bytes(), AUTHORITY, TENANT)
    };
    assert!(doc(&|_| {}).is_ok());
    // Another issuer: the document of another tenant, or the template with {tenantid} in it.
    assert!(doc(&|d| d["issuer"] = J::from(format!("{AUTHORITY}/{OTHER_TENANT}/v2.0"))).is_err());
    assert!(doc(&|d| d["issuer"] = J::from(format!("{AUTHORITY}/{{tenantid}}/v2.0"))).is_err());
    // The right issuer with something after it is another issuer (a prefix is not a match).
    assert!(doc(&|d| d["issuer"] = J::from(format!("{AUTHORITY}/{TENANT}/v2.0/"))).is_err());
    assert!(doc(&|d| d["issuer"] = J::from(format!("{AUTHORITY}/{TENANT}/v2.0x"))).is_err());
    assert!(
        doc(&|d| {
            d.as_object_mut().unwrap().remove("issuer");
        })
        .is_err()
    );
    // An endpoint anywhere else, however close it looks.
    for (key, url) in [
        ("token_endpoint", "https://evil.test/token".to_string()),
        (
            "token_endpoint",
            format!("https://login.microsoftonline.com.evil.test/{TENANT}/oauth2/v2.0/token"),
        ),
        (
            "token_endpoint",
            format!("{AUTHORITY}/{OTHER_TENANT}/oauth2/v2.0/token"),
        ),
        (
            "token_endpoint",
            format!("{AUTHORITY}/common/oauth2/v2.0/token"),
        ),
        (
            "token_endpoint",
            format!("{AUTHORITY}/{TENANT}/../{OTHER_TENANT}/token"),
        ),
        ("token_endpoint", format!("{AUTHORITY}/{TENANT}/token?x=1")),
        ("token_endpoint", format!("{AUTHORITY}/{TENANT}/token#x")),
        ("token_endpoint", format!("{AUTHORITY}/{TENANT}/")),
        (
            "token_endpoint",
            format!("http://login.microsoftonline.com/{TENANT}/oauth2/v2.0/token"),
        ),
        ("authorization_endpoint", "javascript:alert(1)".to_string()),
        ("authorization_endpoint", String::new()),
        ("jwks_uri", format!("{AUTHORITY}@evil.test/{TENANT}/keys")),
    ] {
        assert!(
            doc(&|d| d[key] = J::from(url.clone())).is_err(),
            "{key} {url:?} must be refused"
        );
    }
    assert!(
        doc(&|d| {
            d.as_object_mut().unwrap().remove("jwks_uri");
        })
        .is_err()
    );
    assert!(parse_discovery(b"not json", AUTHORITY, TENANT).is_err());
    assert!(parse_discovery(&vec![b' '; MAX_JWKS_BYTES + 1], AUTHORITY, TENANT).is_err());
}

/// What an app registration that asks for `openid` only gets from Entra, by Microsoft's
/// documentation: no `oid`. The refusal says what to look at.
#[test]
fn a_token_without_the_profile_scope_has_no_object_id_and_the_refusal_says_what_to_check() {
    let s = StandIn::new();
    let c = StandIn::claims_for(
        AUTHORITY,
        TENANT,
        CLIENT,
        NONCE,
        OID,
        NOW,
        &Entra::configured("openid"),
    );
    assert!(
        c.get("oid").is_none() && c.get("preferred_username").is_none() && c.get("name").is_none()
    );
    refused(
        verify_id_token(&s.token(&c), &keys(&s), &expect()),
        "profile scope",
    );
    // With the scope it is a sign-in, and none of the profile claims is part of the identity.
    let c = StandIn::claims_for(
        AUTHORITY,
        TENANT,
        CLIENT,
        NONCE,
        OID,
        NOW,
        &Entra::configured("openid profile"),
    );
    assert!(c["preferred_username"].is_string() && c["name"].is_string());
    let id = verify_id_token(&s.token(&c), &keys(&s), &expect()).unwrap();
    assert_eq!(id.oid, OID);
}

/// An app registration with no optional claims gets no `amr` and no `auth_time`: no second factor
/// is reported, and the sign-in is still a sign-in.
#[test]
fn without_the_optional_claims_there_is_no_amr_and_no_auth_time() {
    let s = StandIn::new();
    let c = StandIn::claims_for(
        AUTHORITY,
        TENANT,
        CLIENT,
        NONCE,
        OID,
        NOW,
        &Entra::plain("openid profile"),
    );
    assert!(c.get("amr").is_none() && c.get("auth_time").is_none());
    let id = verify_id_token(&s.token(&c), &keys(&s), &expect()).unwrap();
    assert!(!id.mfa());
    assert_eq!(id.auth_time, None);
    assert_eq!(id.oid, OID);
}

/// Microsoft documents more values than `mfa` (`ngcmfa`, `wiaormfa`, `otp`, ...). D91 accepts the
/// exact string `mfa` and nothing that merely contains it.
#[test]
fn only_the_exact_value_mfa_counts_as_a_second_factor() {
    let id = |amr: J| {
        let (s, t) = with(|c| c["amr"] = amr);
        verify_id_token(&t, &keys(&s), &expect()).unwrap()
    };
    assert!(id(json!(["mfa"])).mfa());
    assert!(id(json!(["pwd", "mfa"])).mfa());
    assert!(id(json!(["pwd", "otp", "mfa", "fed"])).mfa());
    for other in [
        "wiaormfa",
        "ngcmfa",
        "nomfa",
        "mfa ",
        " mfa",
        "mfa
",
        "multifactor",
        "otp",
        "pwd",
        "fido",
    ] {
        assert!(
            !id(json!(["pwd", other])).mfa(),
            "{other:?} is not the value mfa"
        );
    }
}

/// The e-mail address, the user principal name, the display name and `sub` are not the identity:
/// the same `oid` is the same person whatever they say, and an address with another `oid` is
/// somebody else.
#[test]
fn the_identity_is_the_object_id_and_no_address_or_name_changes_it() {
    let who = |f: fn(&mut J)| {
        let (s, t) = with(f);
        verify_id_token(&t, &keys(&s), &expect()).unwrap()
    };
    let base = who(|_| {});
    let changed = who(|c| {
        c["email"] = J::from("someone.else@example.test");
        c["upn"] = J::from("someone.else@example.test");
        c["preferred_username"] = J::from("someone.else@example.test");
        c["unique_name"] = J::from("someone.else@example.test");
        c["name"] = J::from("Somebody Else");
        c["sub"] = J::from("another-pairwise-subject");
    });
    assert_eq!(base, changed);
    // The bound person's address, and another object id: another identity (the binding decides).
    let other = who(|c| {
        c["preferred_username"] = J::from("ada@example.test");
        c["email"] = J::from("ada@example.test");
        c["oid"] = J::from("0e0e0e0e-1111-2222-3333-444444444444");
    });
    assert_ne!(base.oid, other.oid);
    // An address and a name and no object id at all are no identity.
    check(
        |c| {
            remove(c, "oid");
            c["email"] = J::from("ada@example.test");
            c["upn"] = J::from("ada@example.test");
            c["preferred_username"] = J::from("ada@example.test");
        },
        "object id",
    );
}

/// A token another key signed under the right key name: the signature is checked with a real
/// second RSA key, so the refusal is a failed verification and not a rejected key.
#[test]
fn a_token_signed_by_another_key_under_the_right_name_is_refused() {
    let published = StandIn::new();
    let forger = StandIn::with_other_key();
    assert_eq!(forger.kid, published.kid);
    refused(
        verify_id_token(&forger.token(&claims()), &keys(&published), &expect()),
        "signature",
    );
    // The forger's own key set accepts it, so the refusal above is about the key and nothing else.
    assert!(verify_id_token(&forger.token(&claims()), &keys(&forger), &expect()).is_ok());
}

/// The size of a key is the size of the number, not the length of its encoding: a 1024-bit
/// modulus padded with zeros up to 2048 bits is not a 2048-bit key.
#[test]
fn a_short_key_padded_with_zeros_is_not_a_2048_bit_key() {
    let s = StandIn::new();
    let mut n = vec![0u8; 128];
    let mut short = vec![0xB5u8; 128];
    short[127] |= 1;
    n.extend_from_slice(&short);
    assert_eq!(n.len(), 256);
    let jwk = json!({"kty": "RSA", "use": "sig", "alg": "RS256", "kid": "padded", "n": b64url(&n), "e": b64url(&[1, 0, 1])});
    assert!(Jwks::parse(StandIn::jwks_of(std::slice::from_ref(&jwk)).as_bytes()).is_err());
    // Next to a real key it is left out, not trusted.
    let set = Jwks::parse(StandIn::jwks_of(&[jwk, s.jwk_for("real")]).as_bytes()).unwrap();
    let t = s.sign(&json!({"alg": "RS256", "kid": "padded"}), &claims());
    assert_eq!(
        verify_id_token(&t, &set, &expect()),
        Err(VerifyError::UnknownKey)
    );
}

#[test]
fn rows_the_review_of_the_mutations_found_missing() {
    // aud as a list that does not hold the client, with an azp that is the client (M6a).
    check(
        |c| {
            c["aud"] = json!(["someone-else"]);
            c["azp"] = J::from(CLIENT);
        },
        "audience",
    );
    // The nonce is compared as it is, not case-insensitively (M9a).
    check(|c| c["nonce"] = J::from(NONCE.to_uppercase()), "nonce");
    // The tenant id of the token is read in lower case, so an upper-case one is the same tenant (M18b).
    let (s, t) = with(|c| c["tid"] = J::from(TENANT.to_uppercase()));
    assert_eq!(
        verify_id_token(&t, &keys(&s), &expect()).unwrap().tenant,
        TENANT
    );
}
