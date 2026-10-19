//! The WebAuthn (level 2/3) checks of D65, with no storage and no HTTP: base64url, the
//! authenticator data, COSE public keys, `clientDataJSON` and signature verification. Only the
//! parts a relying party that asks for attestation `none` needs: ES256 (-7), EdDSA (-8) and
//! RS256 (-257), signatures checked by `ring`. Every function takes untrusted input and returns a
//! plain reason on failure; none of them panics on bad input.

pub mod cbor;
#[cfg(any(test, feature = "test-support"))]
pub mod soft;

use cbor::Cbor;
use ring::signature::{
    ECDSA_P256_SHA256_ASN1, ED25519, RSA_PKCS1_2048_8192_SHA256, UnparsedPublicKey,
};
use serde_json::Value as J;
use sha2::{Digest, Sha256};

pub const ALG_ES256: i64 = -7;
pub const ALG_EDDSA: i64 = -8;
pub const ALG_RS256: i64 = -257;
/// What the creation options ask for, in order of preference.
pub const ALGS: [i64; 3] = [ALG_ES256, ALG_EDDSA, ALG_RS256];

pub const FLAG_UP: u8 = 0x01;
pub const FLAG_UV: u8 = 0x04;
pub const FLAG_BE: u8 = 0x08;
pub const FLAG_BS: u8 = 0x10;
pub const FLAG_AT: u8 = 0x40;
pub const FLAG_ED: u8 = 0x80;

// ---- base64url (RFC 4648 section 5, no padding) ----------------------------------------------

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub fn b64url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for i in 0..=c.len() {
            out.push(B64[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

/// Strict decoding: the URL-safe alphabet only, no padding, no stray bits.
pub fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for ch in s.bytes() {
        let v = match ch {
            b'A'..=b'Z' => ch - b'A',
            b'a'..=b'z' => ch - b'a' + 26,
            b'0'..=b'9' => ch - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    (acc == 0).then_some(out)
}

pub fn sha256(b: &[u8]) -> [u8; 32] {
    Sha256::digest(b).into()
}

// ---- authenticator data ------------------------------------------------------------------------

/// A public key ready for verification: the algorithm and the bytes `ring` wants (an uncompressed
/// P-256 point, 32 bytes of Ed25519, or a DER `RSAPublicKey`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKey {
    pub alg: i64,
    pub key: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Attested {
    pub aaguid: [u8; 16],
    pub credential_id: Vec<u8>,
    pub key: PublicKey,
}

#[derive(Debug, Clone)]
pub struct AuthData {
    pub rp_id_hash: [u8; 32],
    pub flags: u8,
    pub counter: u32,
    pub attested: Option<Attested>,
}

impl AuthData {
    pub fn user_present(&self) -> bool {
        self.flags & FLAG_UP != 0
    }
    pub fn user_verified(&self) -> bool {
        self.flags & FLAG_UV != 0
    }
    pub fn backup_eligible(&self) -> bool {
        self.flags & FLAG_BE != 0
    }
    pub fn backed_up(&self) -> bool {
        self.flags & FLAG_BS != 0
    }
}

/// Parse authenticator data. `expect_attested` is true when a registration is read. Anything left
/// over after the data the flags announce is an error.
pub fn parse_auth_data(b: &[u8], expect_attested: bool) -> Result<AuthData, String> {
    if b.len() < 37 {
        return Err("the authenticator data is too short".into());
    }
    let mut rp_id_hash = [0u8; 32];
    rp_id_hash.copy_from_slice(&b[..32]);
    let flags = b[32];
    let counter = u32::from_be_bytes(b[33..37].try_into().unwrap());
    let mut pos = 37;
    let mut attested = None;
    let has_at = flags & FLAG_AT != 0;
    if has_at != expect_attested {
        return Err(if expect_attested {
            "the registration carries no credential".into()
        } else {
            "an assertion must not carry a new credential".into()
        });
    }
    if has_at {
        if b.len() < pos + 18 {
            return Err("the credential data is too short".into());
        }
        let mut aaguid = [0u8; 16];
        aaguid.copy_from_slice(&b[pos..pos + 16]);
        let id_len = usize::from(u16::from_be_bytes(
            b[pos + 16..pos + 18].try_into().unwrap(),
        ));
        pos += 18;
        if !(16..=1023).contains(&id_len) || b.len() < pos + id_len {
            return Err("the credential id has an unacceptable length".into());
        }
        let credential_id = b[pos..pos + id_len].to_vec();
        pos += id_len;
        let (cose, used) = cbor::decode_prefix(&b[pos..])?;
        pos += used;
        attested = Some(Attested {
            aaguid,
            credential_id,
            key: cose_key(&cose)?,
        });
    }
    if flags & FLAG_ED != 0 {
        let (ext, used) = cbor::decode_prefix(&b[pos..])?;
        if !matches!(ext, Cbor::Map(_)) {
            return Err("the extension data is not a map".into());
        }
        pos += used;
    }
    if pos != b.len() {
        return Err("unexpected bytes after the authenticator data".into());
    }
    Ok(AuthData {
        rp_id_hash,
        flags,
        counter,
        attested,
    })
}

// ---- COSE keys -----------------------------------------------------------------------------------

fn der_len(out: &mut Vec<u8>, n: usize) {
    if n < 128 {
        out.push(n as u8);
    } else if n < 256 {
        out.extend([0x81, n as u8]);
    } else {
        out.extend([0x82, (n >> 8) as u8, n as u8]);
    }
}

fn der_uint(out: &mut Vec<u8>, v: &[u8]) {
    let v = match v.iter().position(|b| *b != 0) {
        Some(i) => &v[i..],
        None => &[0],
    };
    out.push(0x02);
    let pad = usize::from(v[0] & 0x80 != 0);
    der_len(out, v.len() + pad);
    if pad == 1 {
        out.push(0);
    }
    out.extend(v);
}

/// `RSAPublicKey ::= SEQUENCE { modulus INTEGER, publicExponent INTEGER }` (RFC 8017).
pub fn rsa_public_key_der(n: &[u8], e: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    der_uint(&mut body, n);
    der_uint(&mut body, e);
    let mut out = vec![0x30];
    der_len(&mut out, body.len());
    out.extend(body);
    out
}

/// Read a COSE_Key and keep it only when it is one of the three supported algorithms with the
/// key type, curve and sizes that algorithm needs.
pub fn cose_key(c: &Cbor) -> Result<PublicKey, String> {
    let int = |k: i128| c.get_int(k).and_then(Cbor::as_int);
    let bytes = |k: i128| c.get_int(k).and_then(Cbor::as_bytes);
    let (kty, alg) = (int(1), int(3).and_then(|a| i64::try_from(a).ok()));
    match (kty, alg) {
        (Some(2), Some(ALG_ES256)) => {
            let (x, y) = (bytes(-2), bytes(-3));
            match (int(-1), x, y) {
                (Some(1), Some(x), Some(y)) if x.len() == 32 && y.len() == 32 => {
                    let mut key = vec![0x04];
                    key.extend(x);
                    key.extend(y);
                    Ok(PublicKey {
                        alg: ALG_ES256,
                        key,
                    })
                }
                _ => Err("not a P-256 public key".into()),
            }
        }
        (Some(1), Some(ALG_EDDSA)) => match (int(-1), bytes(-2)) {
            (Some(6), Some(x)) if x.len() == 32 => Ok(PublicKey {
                alg: ALG_EDDSA,
                key: x.to_vec(),
            }),
            _ => Err("not an Ed25519 public key".into()),
        },
        (Some(3), Some(ALG_RS256)) => match (bytes(-1), bytes(-2)) {
            (Some(n), Some(e))
                if (256..=512).contains(&n.len()) && !e.is_empty() && e.len() <= 4 =>
            {
                Ok(PublicKey {
                    alg: ALG_RS256,
                    key: rsa_public_key_der(n, e),
                })
            }
            _ => Err("not an RSA public key of 2048 to 4096 bits".into()),
        },
        _ => Err("the credential uses an algorithm this server does not accept".into()),
    }
}

/// Check `signature` over `message` (authenticator data followed by the SHA-256 of the client
/// data, for an assertion).
pub fn verify_signature(key: &PublicKey, message: &[u8], signature: &[u8]) -> bool {
    let alg: &'static dyn ring::signature::VerificationAlgorithm = match key.alg {
        ALG_ES256 => &ECDSA_P256_SHA256_ASN1,
        ALG_EDDSA => &ED25519,
        ALG_RS256 => &RSA_PKCS1_2048_8192_SHA256,
        _ => return false,
    };
    UnparsedPublicKey::new(alg, &key.key)
        .verify(message, signature)
        .is_ok()
}

// ---- clientDataJSON -------------------------------------------------------------------------------

/// What the browser reported about the ceremony.
#[derive(Debug, Clone)]
pub struct ClientData {
    pub challenge: String,
    pub raw: Vec<u8>,
}

/// Parse `clientDataJSON` and check its type, origin and cross-origin flags. The challenge is
/// returned (still base64url) for the caller to look up and consume.
pub fn client_data(raw: &[u8], kind: &str, origin: &str) -> Result<ClientData, String> {
    let v: J = serde_json::from_slice(raw).map_err(|_| "the client data is not JSON")?;
    if v["type"].as_str() != Some(kind) {
        return Err("the client data is for a different ceremony".into());
    }
    if v["origin"].as_str() != Some(origin) {
        return Err("the browser reported a different origin".into());
    }
    if v["crossOrigin"].as_bool() == Some(true) || v.get("topOrigin").is_some() {
        return Err("a cross-origin ceremony is not accepted".into());
    }
    let challenge = v["challenge"]
        .as_str()
        .filter(|c| !c.is_empty() && c.len() <= 128)
        .ok_or("the client data carries no challenge")?;
    Ok(ClientData {
        challenge: challenge.to_string(),
        raw: raw.to_vec(),
    })
}

/// The fields of a browser `PublicKeyCredential` (as JSON, binary fields base64url) every
/// ceremony needs.
#[derive(Debug)]
pub struct Credential {
    pub raw_id: Vec<u8>,
    pub client_data_json: Vec<u8>,
    pub response: J,
    pub extensions: J,
}

pub fn credential(c: &J) -> Result<Credential, String> {
    if c["type"].as_str() != Some("public-key") {
        return Err("not a public-key credential".into());
    }
    let raw_id = c["rawId"]
        .as_str()
        .and_then(b64url_decode)
        .filter(|b| (16..=1023).contains(&b.len()))
        .ok_or("the credential id is missing or malformed")?;
    if c["id"].as_str() != c["rawId"].as_str() {
        return Err("the credential id does not match its raw form".into());
    }
    let resp = &c["response"];
    let client_data_json = resp["clientDataJSON"]
        .as_str()
        .and_then(b64url_decode)
        .filter(|b| b.len() <= 4096)
        .ok_or("the client data is missing or malformed")?;
    Ok(Credential {
        raw_id,
        client_data_json,
        response: resp.clone(),
        extensions: c["clientExtensionResults"].clone(),
    })
}

/// Fields of an assertion response after decoding.
pub struct Assertion {
    pub auth_data_bytes: Vec<u8>,
    pub signature: Vec<u8>,
    pub user_handle: Option<Vec<u8>>,
}

pub fn assertion(resp: &J) -> Result<Assertion, String> {
    let bytes = |k: &str| resp[k].as_str().and_then(b64url_decode);
    Ok(Assertion {
        auth_data_bytes: bytes("authenticatorData")
            .filter(|b| b.len() <= 2048)
            .ok_or("the authenticator data is missing or malformed")?,
        signature: bytes("signature")
            .filter(|b| !b.is_empty() && b.len() <= 1024)
            .ok_or("the signature is missing or malformed")?,
        user_handle: bytes("userHandle").filter(|b| !b.is_empty()),
    })
}

/// The bytes an assertion signature covers.
pub fn assertion_message(auth_data: &[u8], client_data_json: &[u8]) -> Vec<u8> {
    let mut m = auth_data.to_vec();
    m.extend(sha256(client_data_json));
    m
}

/// A registration response: the parsed attestation object, which must be format `none`.
pub fn registration(resp: &J) -> Result<AuthData, String> {
    let ao = resp["attestationObject"]
        .as_str()
        .and_then(b64url_decode)
        .filter(|b| b.len() <= 16 * 1024)
        .ok_or("the attestation object is missing or malformed")?;
    let obj = cbor::decode(&ao)?;
    if obj.get("fmt").and_then(Cbor::as_text) != Some("none") {
        return Err("this server accepts attestation format none only".into());
    }
    match obj.get("attStmt") {
        Some(Cbor::Map(m)) if m.is_empty() => {}
        _ => return Err("the attestation statement of format none must be empty".into()),
    }
    let auth = obj
        .get("authData")
        .and_then(Cbor::as_bytes)
        .ok_or("the attestation object has no authenticator data")?;
    parse_auth_data(auth, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_round_trips_and_is_strict() {
        for n in 0..40 {
            let data: Vec<u8> = (0..n).map(|i| (i * 7 + 3) as u8).collect();
            assert_eq!(b64url_decode(&b64url(&data)).unwrap(), data);
        }
        assert_eq!(b64url(&[0xfb, 0xff]), "-_8");
        assert!(b64url_decode("-_8=").is_none(), "no padding");
        assert!(b64url_decode("a+b/").is_none(), "standard alphabet");
        assert!(b64url_decode("a").is_none());
        assert!(b64url_decode("AB").is_none(), "stray trailing bits");
        assert!(b64url_decode("A B_").is_none());
    }

    #[test]
    fn authenticator_data_must_be_exactly_what_its_flags_announce() {
        let mut d = vec![7u8; 32];
        d.extend([FLAG_UP, 0, 0, 0, 5]);
        let a = parse_auth_data(&d, false).unwrap();
        assert_eq!(
            (a.counter, a.user_present(), a.user_verified()),
            (5, true, false)
        );
        d.push(0);
        assert!(parse_auth_data(&d, false).is_err(), "trailing byte");
        assert!(parse_auth_data(&d[..36], false).is_err(), "short");
        // A registration without the credential flag is refused.
        d.pop();
        assert!(parse_auth_data(&d, true).is_err());
        // An assertion that claims a new credential is refused.
        d[32] = FLAG_UP | FLAG_AT;
        assert!(parse_auth_data(&d, false).is_err());
    }

    #[test]
    fn client_data_checks_type_origin_and_cross_origin() {
        let ok = br#"{"type":"webauthn.get","challenge":"abc","origin":"https://a.test"}"#;
        assert_eq!(
            client_data(ok, "webauthn.get", "https://a.test")
                .unwrap()
                .challenge,
            "abc"
        );
        assert!(client_data(ok, "webauthn.create", "https://a.test").is_err());
        assert!(client_data(ok, "webauthn.get", "https://b.test").is_err());
        assert!(client_data(ok, "webauthn.get", "https://a.test:8443").is_err());
        let cross = br#"{"type":"webauthn.get","challenge":"abc","origin":"https://a.test","crossOrigin":true}"#;
        assert!(client_data(cross, "webauthn.get", "https://a.test").is_err());
        let top = br#"{"type":"webauthn.get","challenge":"abc","origin":"https://a.test","topOrigin":"https://x.test"}"#;
        assert!(client_data(top, "webauthn.get", "https://a.test").is_err());
        assert!(client_data(b"nope", "webauthn.get", "https://a.test").is_err());
        let none = br#"{"type":"webauthn.get","origin":"https://a.test"}"#;
        assert!(client_data(none, "webauthn.get", "https://a.test").is_err());
    }

    #[test]
    fn rsa_der_encoding_pads_the_high_bit_and_strips_leading_zeros() {
        let der = rsa_public_key_der(&[0x80, 1], &[1, 0, 1]);
        // SEQUENCE(len 4+... ) INTEGER 00 80 01, INTEGER 01 00 01
        assert_eq!(
            der,
            [
                0x30, 0x0a, 0x02, 0x03, 0x00, 0x80, 0x01, 0x02, 0x03, 1, 0, 1
            ]
        );
        let der = rsa_public_key_der(&[0, 0, 0x7f], &[3]);
        assert_eq!(der, [0x30, 0x06, 0x02, 0x01, 0x7f, 0x02, 0x01, 3]);
    }
}
