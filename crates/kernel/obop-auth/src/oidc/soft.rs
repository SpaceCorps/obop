//! A stand-in for Microsoft Entra's token side, for tests (feature `test-support`): it publishes a
//! discovery document and a key set and signs id tokens, with every claim open to change, so the
//! checks of `verify` and the whole sign-in can be tried against what a provider would send and
//! against what it must not. It is not Entra and proves nothing about Entra: the claims it makes
//! are those D91 reads, in the shape Microsoft's documentation describes (general knowledge).
//!
//! `ring` cannot make RSA keys, so the key is the test-only fixture the software authenticator of
//! the passkey tests already uses (`testdata/rsa2048-test-key.der`). It protects nothing.

use crate::webauthn::soft::split_rsa_der;
use crate::webauthn::{b64url, b64url_decode};
use ring::rand::SystemRandom;
use ring::signature::{RSA_PKCS1_SHA256, RsaKeyPair};
use serde_json::{Value as J, json};

const RSA_FIXTURE: &[u8] = include_bytes!("../../testdata/rsa2048-test-key.der");

/// The kid the stand-in publishes.
pub const KID: &str = "standin-key-1";

pub struct StandIn {
    key: RsaKeyPair,
    /// The `kid` the key set publishes and the tokens name.
    pub kid: String,
}

impl Default for StandIn {
    fn default() -> Self {
        Self::new()
    }
}

impl StandIn {
    pub fn new() -> Self {
        Self {
            key: RsaKeyPair::from_der(RSA_FIXTURE).expect("the fixture key"),
            kid: KID.into(),
        }
    }

    fn jwk(&self, kid: &str) -> J {
        let (n, e) = split_rsa_der(self.key.public().as_ref());
        json!({"kty": "RSA", "use": "sig", "alg": "RS256", "kid": kid, "n": b64url(n), "e": b64url(e)})
    }

    /// The key set the stand-in publishes.
    pub fn jwks(&self) -> String {
        json!({"keys": [self.jwk(&self.kid)]}).to_string()
    }

    /// A key set with the given keys: `(kid, jwk)`; use [`StandIn::jwk_for`] for the real key.
    pub fn jwks_of(keys: &[J]) -> String {
        json!({"keys": keys}).to_string()
    }

    /// The real key under another name.
    pub fn jwk_for(&self, kid: &str) -> J {
        self.jwk(kid)
    }

    /// A key set whose only key has this stand-in's name but another modulus (one bit flipped):
    /// a token this stand-in signed does not verify against it.
    pub fn jwks_with_wrong_modulus(&self) -> String {
        let mut k = self.jwk(&self.kid);
        let mut n = b64url_decode(k["n"].as_str().unwrap()).unwrap();
        let last = n.len() - 1;
        n[last] ^= 1;
        k["n"] = J::from(b64url(&n));
        json!({"keys": [k]}).to_string()
    }

    /// The discovery document of a tenant under `authority`.
    pub fn discovery(&self, authority: &str, tenant: &str) -> String {
        json!({
            "issuer": format!("{authority}/{tenant}/v2.0"),
            "authorization_endpoint": format!("{authority}/{tenant}/oauth2/v2.0/authorize"),
            "token_endpoint": format!("{authority}/{tenant}/oauth2/v2.0/token"),
            "jwks_uri": format!("{authority}/{tenant}/discovery/v2.0/keys"),
        })
        .to_string()
    }

    /// The claims of a good id token for `oid` at time `now`: v2.0 issuer, the tenant, the
    /// client, the nonce, valid for an hour, and `amr` of a password and a second factor.
    pub fn claims(
        authority: &str,
        tenant: &str,
        client_id: &str,
        nonce: &str,
        oid: &str,
        now: u64,
    ) -> J {
        json!({
            "iss": format!("{authority}/{tenant}/v2.0"),
            "tid": tenant,
            "aud": client_id,
            "sub": "standin-subject-per-application",
            "oid": oid,
            "nonce": nonce,
            "ver": "2.0",
            "iat": now,
            "nbf": now,
            "exp": now + 3600,
            "auth_time": now,
            "amr": ["pwd", "mfa"],
        })
    }

    /// A compact JWS over `header` and `claims`, signed with RS256 by the stand-in's key.
    pub fn sign(&self, header: &J, claims: &J) -> String {
        let signing = format!(
            "{}.{}",
            b64url(header.to_string().as_bytes()),
            b64url(claims.to_string().as_bytes())
        );
        let mut sig = vec![0u8; self.key.public().modulus_len()];
        self.key
            .sign(
                &RSA_PKCS1_SHA256,
                &SystemRandom::new(),
                signing.as_bytes(),
                &mut sig,
            )
            .expect("signing");
        format!("{signing}.{}", b64url(&sig))
    }

    /// A token with the usual header (`RS256`, this key's `kid`).
    pub fn token(&self, claims: &J) -> String {
        self.sign(
            &json!({"alg": "RS256", "typ": "JWT", "kid": self.kid}),
            claims,
        )
    }
}
