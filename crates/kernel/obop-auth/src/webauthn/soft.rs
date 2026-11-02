//! A software authenticator for tests (feature `test-support`): it makes the creation and request
//! responses a browser would return, for ES256, EdDSA and RS256, with knobs to misbehave (wrong
//! origin, wrong relying party, a counter that goes backwards, no user verification). It exists
//! to prove the server accepts what real authenticators send and refuses what they must not.

use super::cbor::enc;
use super::{ALG_EDDSA, ALG_ES256, ALG_RS256, FLAG_AT, FLAG_BE, FLAG_BS, FLAG_UP, FLAG_UV};
use super::{b64url, b64url_decode, sha256};
use ring::rand::SystemRandom;
use ring::signature::{
    ECDSA_P256_SHA256_ASN1_SIGNING, EcdsaKeyPair, Ed25519KeyPair, KeyPair, RSA_PKCS1_SHA256,
    RsaKeyPair,
};
use serde_json::{Value as J, json};

enum Key {
    Es256(EcdsaKeyPair),
    Ed25519(Ed25519KeyPair),
    Rs256(RsaKeyPair),
}

pub struct SoftAuthenticator {
    key: Key,
    pub credential_id: Vec<u8>,
    pub counter: u32,
    /// Set the user-verified flag.
    pub verified: bool,
    pub backup_eligible: bool,
    pub backed_up: bool,
    /// The origin written into the client data; `None` is the one the caller passes.
    pub origin_override: Option<String>,
    /// The relying party id hashed into the authenticator data; `None` is the one in the options.
    pub rp_id_override: Option<String>,
    /// Send `userHandle` in assertions.
    pub send_user_handle: bool,
    /// Stored when the credential was created with a user handle.
    pub user_handle: Vec<u8>,
}

const RSA_FIXTURE: &[u8] = include_bytes!("../../testdata/rsa2048-test-key.der");

impl SoftAuthenticator {
    pub fn new(alg: i64) -> Self {
        let rng = SystemRandom::new();
        let key = match alg {
            ALG_ES256 => {
                let pkcs8 =
                    EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng).unwrap();
                Key::Es256(
                    EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref(), &rng)
                        .unwrap(),
                )
            }
            ALG_EDDSA => {
                let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
                Key::Ed25519(Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap())
            }
            ALG_RS256 => Key::Rs256(RsaKeyPair::from_der(RSA_FIXTURE).unwrap()),
            other => panic!("the software authenticator has no algorithm {other}"),
        };
        let mut credential_id = vec![0u8; 32];
        getrandom::fill(&mut credential_id).unwrap();
        Self {
            key,
            credential_id,
            counter: 0,
            verified: true,
            backup_eligible: false,
            backed_up: false,
            origin_override: None,
            rp_id_override: None,
            send_user_handle: true,
            user_handle: Vec::new(),
        }
    }

    pub fn es256() -> Self {
        Self::new(ALG_ES256)
    }

    fn cose_key(&self) -> Vec<u8> {
        let mut k = Vec::new();
        match &self.key {
            Key::Es256(p) => {
                let point = p.public_key().as_ref();
                enc::map(&mut k, 5);
                enc::int(&mut k, 1);
                enc::int(&mut k, 2);
                enc::int(&mut k, 3);
                enc::int(&mut k, ALG_ES256);
                enc::int(&mut k, -1);
                enc::int(&mut k, 1);
                enc::int(&mut k, -2);
                enc::bytes(&mut k, &point[1..33]);
                enc::int(&mut k, -3);
                enc::bytes(&mut k, &point[33..65]);
            }
            Key::Ed25519(p) => {
                enc::map(&mut k, 4);
                enc::int(&mut k, 1);
                enc::int(&mut k, 1);
                enc::int(&mut k, 3);
                enc::int(&mut k, ALG_EDDSA);
                enc::int(&mut k, -1);
                enc::int(&mut k, 6);
                enc::int(&mut k, -2);
                enc::bytes(&mut k, p.public_key().as_ref());
            }
            Key::Rs256(p) => {
                let (n, e) = split_rsa_der(p.public().as_ref());
                enc::map(&mut k, 4);
                enc::int(&mut k, 1);
                enc::int(&mut k, 3);
                enc::int(&mut k, 3);
                enc::int(&mut k, ALG_RS256);
                enc::int(&mut k, -1);
                enc::bytes(&mut k, n);
                enc::int(&mut k, -2);
                enc::bytes(&mut k, e);
            }
        }
        k
    }

    fn sign(&self, msg: &[u8]) -> Vec<u8> {
        match &self.key {
            Key::Es256(p) => p.sign(&SystemRandom::new(), msg).unwrap().as_ref().to_vec(),
            Key::Ed25519(p) => p.sign(msg).as_ref().to_vec(),
            Key::Rs256(p) => {
                let mut sig = vec![0u8; p.public().modulus_len()];
                p.sign(&RSA_PKCS1_SHA256, &SystemRandom::new(), msg, &mut sig)
                    .unwrap();
                sig
            }
        }
    }

    fn flags(&self) -> u8 {
        let mut f = FLAG_UP;
        if self.verified {
            f |= FLAG_UV;
        }
        if self.backup_eligible {
            f |= FLAG_BE;
        }
        if self.backed_up {
            f |= FLAG_BS;
        }
        f
    }

    fn client_data(&self, kind: &str, challenge: &str, origin: &str) -> Vec<u8> {
        json!({
            "type": kind,
            "challenge": challenge,
            "origin": self.origin_override.as_deref().unwrap_or(origin),
            "crossOrigin": false,
        })
        .to_string()
        .into_bytes()
    }

    fn auth_data(&self, rp_id: &str, attested: bool) -> Vec<u8> {
        let mut d = sha256(self.rp_id_override.as_deref().unwrap_or(rp_id).as_bytes()).to_vec();
        d.push(self.flags() | if attested { FLAG_AT } else { 0 });
        d.extend(self.counter.to_be_bytes());
        if attested {
            d.extend([0u8; 16]);
            d.extend((self.credential_id.len() as u16).to_be_bytes());
            d.extend(&self.credential_id);
            d.extend(self.cose_key());
        }
        d
    }

    /// The response to `navigator.credentials.create({publicKey: options})` at `origin`.
    pub fn create(&mut self, options: &J, origin: &str) -> J {
        let pk = &options["publicKey"];
        let rp_id = pk["rp"]["id"].as_str().expect("rp.id");
        let challenge = pk["challenge"].as_str().expect("challenge");
        // The handle is stored with the credential the first time and kept.
        if self.user_handle.is_empty()
            && let Some(h) = pk["user"]["id"].as_str().and_then(b64url_decode)
        {
            self.user_handle = h;
        }
        let mut att = Vec::new();
        enc::map(&mut att, 3);
        enc::text(&mut att, "fmt");
        enc::text(&mut att, "none");
        enc::text(&mut att, "attStmt");
        enc::map(&mut att, 0);
        enc::text(&mut att, "authData");
        enc::bytes(&mut att, &self.auth_data(rp_id, true));
        let id = b64url(&self.credential_id);
        json!({
            "id": id,
            "rawId": id,
            "type": "public-key",
            "authenticatorAttachment": "platform",
            "clientExtensionResults": {"credProps": {"rk": true}},
            "response": {
                "clientDataJSON": b64url(&self.client_data("webauthn.create", challenge, origin)),
                "attestationObject": b64url(&att),
                "transports": ["internal"],
            }
        })
    }

    /// The response to `navigator.credentials.get({publicKey: options})` at `origin`. The counter
    /// goes up by one first unless it is zero and stays so (`counter == 0` before and after is how
    /// an authenticator without a counter behaves: use `fixed_counter`).
    pub fn get(&mut self, options: &J, origin: &str) -> J {
        self.counter += 1;
        self.get_with_counter(options, origin)
    }

    /// As `get`, with the counter exactly as it is now.
    pub fn get_with_counter(&mut self, options: &J, origin: &str) -> J {
        let pk = &options["publicKey"];
        let rp_id = pk["rpId"].as_str().expect("rpId");
        let challenge = pk["challenge"].as_str().expect("challenge");
        let client = self.client_data("webauthn.get", challenge, origin);
        let auth = self.auth_data(rp_id, false);
        let mut msg = auth.clone();
        msg.extend(sha256(&client));
        let id = b64url(&self.credential_id);
        let mut resp = json!({
            "clientDataJSON": b64url(&client),
            "authenticatorData": b64url(&auth),
            "signature": b64url(&self.sign(&msg)),
        });
        if self.send_user_handle && !self.user_handle.is_empty() {
            resp["userHandle"] = J::from(b64url(&self.user_handle));
        }
        json!({
            "id": id,
            "rawId": id,
            "type": "public-key",
            "clientExtensionResults": {},
            "response": resp,
        })
    }
}

/// `(n, e)` of a DER `RSAPublicKey` (SEQUENCE of two INTEGERs), without leading zeros.
pub(crate) fn split_rsa_der(der: &[u8]) -> (&[u8], &[u8]) {
    fn len(b: &[u8]) -> (usize, usize) {
        if b[0] < 0x80 {
            (usize::from(b[0]), 1)
        } else {
            let n = usize::from(b[0] & 0x7f);
            (
                b[1..=n].iter().fold(0, |a, x| (a << 8) | usize::from(*x)),
                1 + n,
            )
        }
    }
    let (_, h) = len(&der[1..]);
    let mut pos = 1 + h;
    let mut ints = Vec::new();
    for _ in 0..2 {
        let (l, h) = len(&der[pos + 1..]);
        let start = pos + 1 + h;
        let mut v = &der[start..start + l];
        while v.len() > 1 && v[0] == 0 {
            v = &v[1..];
        }
        ints.push(v);
        pos = start + l;
    }
    (ints[0], ints[1])
}
