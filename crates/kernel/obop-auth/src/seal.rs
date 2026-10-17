//! Envelope for secrets at rest (D53): ChaCha20-Poly1305 under a key derived with HKDF-SHA-256
//! from the server secret `OBOP_SECRET_KEY`. The user name is bound in as associated data, so a
//! sealed secret copied onto another user's record does not open.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;

const VERSION: &str = "v1";

/// The fixed key dev and test setups may opt into. It is public knowledge: never for real data.
pub const INSECURE_DEV_KEY: [u8; 32] = *b"obop-insecure-dev-key-0123456789";

/// Parse a 32-byte key given as 64 hex digits or as base64 (standard or URL-safe alphabet).
pub fn parse_key(text: &str) -> Result<[u8; 32], String> {
    let t = text.trim();
    let bytes = if t.len() == 64 && t.bytes().all(|b| b.is_ascii_hexdigit()) {
        (0..32)
            .map(|i| u8::from_str_radix(&t[2 * i..2 * i + 2], 16).map_err(|e| e.to_string()))
            .collect::<Result<Vec<u8>, String>>()?
    } else {
        base64_decode(t).ok_or("OBOP_SECRET_KEY must be 32 bytes as 64 hex digits or base64")?
    };
    <[u8; 32]>::try_from(bytes.as_slice())
        .map_err(|_| "OBOP_SECRET_KEY must be exactly 32 bytes".to_string())
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let (mut buf, mut bits) = (0u32, 0u32);
    let mut out = Vec::new();
    for c in s.trim_end_matches('=').chars() {
        let v = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '+' | '-' => 62,
            '/' | '_' => 63,
            _ => return None,
        };
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            out.push((buf >> (bits - 8)) as u8);
            bits -= 8;
        }
        buf &= (1 << bits) - 1;
    }
    Some(out)
}

fn cipher(master: &[u8; 32]) -> ChaCha20Poly1305 {
    let mut key = [0u8; 32];
    Hkdf::<Sha256>::new(None, master)
        .expand(b"obop/totp-secret-envelope/v1", &mut key)
        .expect("32 bytes is a valid HKDF output length");
    ChaCha20Poly1305::new((&key).into())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return None;
    }
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok())
        .collect()
}

pub fn seal(master: &[u8; 32], aad: &str, plain: &[u8]) -> Result<String, String> {
    let mut nonce = [0u8; 12];
    getrandom::fill(&mut nonce).map_err(|e| e.to_string())?;
    let ct = cipher(master)
        .encrypt(
            &Nonce::from(nonce),
            Payload {
                msg: plain,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| "encryption failed".to_string())?;
    Ok(format!("{VERSION}.{}.{}", hex(&nonce), hex(&ct)))
}

pub fn open(master: &[u8; 32], aad: &str, sealed: &str) -> Result<Vec<u8>, String> {
    let mut parts = sealed.split('.');
    let (Some(VERSION), Some(n), Some(c), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err("unknown envelope format".into());
    };
    let nonce = unhex(n)
        .and_then(|n| Nonce::try_from(n.as_slice()).ok())
        .ok_or("corrupt envelope")?;
    let ct = unhex(c).ok_or("corrupt envelope")?;
    cipher(master)
        .decrypt(
            &nonce,
            Payload {
                msg: &ct,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| "the secret does not open: wrong OBOP_SECRET_KEY or tampered record".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const K: [u8; 32] = [7; 32];

    #[test]
    fn round_trip_and_fresh_nonces() {
        let a = seal(&K, "ada", b"secret bytes").unwrap();
        let b = seal(&K, "ada", b"secret bytes").unwrap();
        assert_ne!(a, b, "a new nonce every time");
        assert!(!a.contains("secret"));
        assert_eq!(open(&K, "ada", &a).unwrap(), b"secret bytes");
    }

    #[test]
    fn tampering_the_wrong_key_and_another_user_are_all_refused() {
        let s = seal(&K, "ada", b"x").unwrap();
        let mut flipped = s.clone().into_bytes();
        let last = flipped.len() - 1;
        flipped[last] = if flipped[last] == b'0' { b'1' } else { b'0' };
        assert!(open(&K, "ada", &String::from_utf8(flipped).unwrap()).is_err());
        assert!(open(&[8; 32], "ada", &s).is_err());
        assert!(open(&K, "grace", &s).is_err(), "bound to the user");
        assert!(open(&K, "ada", "v2.00.00").is_err());
        assert!(open(&K, "ada", "garbage").is_err());
    }

    #[test]
    fn keys_parse_from_hex_and_base64() {
        let hexkey = "00".repeat(31) + "ff";
        let k = parse_key(&hexkey).unwrap();
        assert_eq!((k[0], k[31]), (0, 255));
        let b64 = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
        assert_eq!(parse_key(b64).unwrap(), [0; 32]);
        assert!(parse_key("short").is_err());
        assert!(parse_key(&"ab".repeat(16)).is_err());
    }
}
