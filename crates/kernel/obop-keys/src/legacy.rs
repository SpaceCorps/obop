//! The two envelope formats that came before the key ring's own (`e2.`), still opened so nothing
//! sealed earlier is lost, and re-sealed by `obop secrets rotate` and `obop migrate`:
//!
//! - `v1.<nonce>.<ciphertext>`: second-factor secrets (D53). Key: HKDF-SHA-256 of the 32-byte
//!   server key, bound to the user name.
//! - `obs1:<nonce><ciphertext>`: connector credentials and other connector secrets (D54). Key:
//!   two HKDF-SHA-256 stages from the key text, bound to the purpose.
//!
//! Neither names the key that sealed it, so opening tries every key of the ring.

use crate::{KeyError, KeyRing, hex, unhex};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;

const TOTP_VERSION: &str = "v1";
const BOX_PREFIX: &str = "obs1:";
const BOX_SALT: &[u8] = b"obop-secretbox-v1";

/// Whether `s` is in one of the older formats.
pub fn is_legacy(s: &str) -> bool {
    s.starts_with("v1.") || s.starts_with(BOX_PREFIX)
}

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

// ---- second-factor secrets (v1.) ---------------------------------------------------------

fn totp_cipher(master: &[u8; 32]) -> ChaCha20Poly1305 {
    let mut key = [0u8; 32];
    Hkdf::<Sha256>::new(None, master)
        .expand(b"obop/totp-secret-envelope/v1", &mut key)
        .expect("32 bytes is a valid HKDF output length");
    ChaCha20Poly1305::new((&key).into())
}

/// The 32-byte master a key text stood for in the first format: the key as hex or base64, or,
/// for the public development key, its 32 raw bytes.
fn totp_master(raw: &str) -> Option<[u8; 32]> {
    parse_key(raw)
        .ok()
        .or_else(|| <[u8; 32]>::try_from(raw.as_bytes()).ok())
}

/// Seal in the first second-factor format. Kept for tests of migrations.
pub fn seal_totp_v1(master: &[u8; 32], aad: &str, plain: &[u8]) -> String {
    let nonce = crate::random::<12>();
    let ct = totp_cipher(master)
        .encrypt(
            &Nonce::from(nonce),
            Payload {
                msg: plain,
                aad: aad.as_bytes(),
            },
        )
        .expect("encryption of in-memory input cannot fail");
    format!("{TOTP_VERSION}.{}.{}", hex(&nonce), hex(&ct))
}

/// Open a `v1.` second-factor secret of `user` with any key of the ring.
pub fn open_totp_v1(ring: &KeyRing, user: &str, sealed: &str) -> Result<Vec<u8>, KeyError> {
    let mut parts = sealed.split('.');
    let (Some(TOTP_VERSION), Some(n), Some(c), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(KeyError::NotSealed);
    };
    let nonce = unhex(n)
        .and_then(|n| Nonce::try_from(n.as_slice()).ok())
        .ok_or(KeyError::Malformed)?;
    let ct = unhex(c).ok_or(KeyError::Malformed)?;
    for k in ring.keys() {
        if let Some(master) = totp_master(k.expose())
            && let Ok(plain) = totp_cipher(&master).decrypt(
                &nonce,
                Payload {
                    msg: &ct,
                    aad: user.as_bytes(),
                },
            )
        {
            return Ok(plain);
        }
    }
    Err(KeyError::CannotOpen)
}

// ---- connector secrets (obs1:) -----------------------------------------------------------

/// The key the first connector format derived from a key text for `label` (`seal:<purpose>` for
/// sealing, `pseudonym:<connection>` for the pseudonym keys of that era).
pub fn box_derive(raw: &str, label: &str) -> [u8; 32] {
    let mut master = [0u8; 32];
    Hkdf::<Sha256>::new(Some(BOX_SALT), raw.as_bytes())
        .expand(b"master", &mut master)
        .expect("32 bytes");
    let mut out = [0u8; 32];
    Hkdf::<Sha256>::new(Some(BOX_SALT), &master)
        .expand(label.as_bytes(), &mut out)
        .expect("32 bytes");
    out
}

fn box_subkey(raw: &str, purpose: &str) -> [u8; 32] {
    box_derive(raw, &format!("seal:{purpose}"))
}

/// Seal in the first connector format. Kept for tests of migrations.
pub fn seal_box_v1(raw_key: &str, purpose: &str, plain: &[u8]) -> String {
    let nonce = crate::random::<12>();
    let ct = ChaCha20Poly1305::new_from_slice(&box_subkey(raw_key, purpose))
        .expect("32-byte key")
        .encrypt(
            &Nonce::from(nonce),
            Payload {
                msg: plain,
                aad: purpose.as_bytes(),
            },
        )
        .expect("encryption of in-memory input cannot fail");
    format!("{BOX_PREFIX}{}{}", hex(&nonce), hex(&ct))
}

/// Open an `obs1:` connector secret for `purpose` with any key of the ring.
pub fn open_box_v1(ring: &KeyRing, purpose: &str, sealed: &str) -> Result<Vec<u8>, KeyError> {
    let body = sealed.strip_prefix(BOX_PREFIX).ok_or(KeyError::NotSealed)?;
    let raw = unhex(body).ok_or(KeyError::Malformed)?;
    if raw.len() < 12 + 16 {
        return Err(KeyError::Malformed);
    }
    let (nonce, ct) = raw.split_at(12);
    let nonce = Nonce::try_from(nonce).map_err(|_| KeyError::Malformed)?;
    for k in ring.keys() {
        let Ok(c) = ChaCha20Poly1305::new_from_slice(&box_subkey(k.expose(), purpose)) else {
            continue;
        };
        if let Ok(plain) = c.decrypt(
            &nonce,
            Payload {
                msg: ct,
                aad: purpose.as_bytes(),
            },
        ) {
            return Ok(plain);
        }
    }
    Err(KeyError::CannotOpen)
}

/// Open a value in any format the server has used: the ring's own, or either older one. The
/// `legacy_user` is the associated data of the `v1.` format (the owner of a second-factor
/// secret); `purpose` is the purpose of the `e2.` and `obs1:` formats.
pub fn open_any(
    ring: &KeyRing,
    purpose: &str,
    legacy_user: Option<&str>,
    sealed: &str,
) -> Result<Vec<u8>, KeyError> {
    if sealed.starts_with("v1.") {
        return open_totp_v1(ring, legacy_user.unwrap_or(""), sealed);
    }
    if sealed.starts_with(BOX_PREFIX) {
        return open_box_v1(ring, purpose, sealed);
    }
    ring.open(purpose, sealed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn keys_parse_from_hex_and_base64() {
        let hexkey = "00".repeat(31) + "ff";
        let k = parse_key(&hexkey).unwrap();
        assert_eq!((k[0], k[31]), (0, 255));
        assert_eq!(
            parse_key("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=").unwrap(),
            [0; 32]
        );
        assert!(parse_key("short").is_err());
        assert!(parse_key(&"ab".repeat(16)).is_err());
    }

    #[test]
    fn second_factor_values_of_the_first_format_open_with_the_right_key_only() {
        let master = parse_key(A).unwrap();
        let s = seal_totp_v1(&master, "ada", b"seed");
        let ring = KeyRing::from_secrets(B, &[A]).unwrap();
        assert_eq!(open_totp_v1(&ring, "ada", &s).unwrap(), b"seed");
        assert_eq!(open_totp_v1(&ring, "grace", &s), Err(KeyError::CannotOpen));
        let only_b = KeyRing::from_secrets(B, &[]).unwrap();
        assert_eq!(open_totp_v1(&only_b, "ada", &s), Err(KeyError::CannotOpen));
        // The public development key was used as raw bytes.
        let dev = KeyRing::from_vars(None, None, true).unwrap();
        let s = seal_totp_v1(&crate::INSECURE_DEV_KEY, "ada", b"seed");
        assert_eq!(open_totp_v1(&dev, "ada", &s).unwrap(), b"seed");
    }

    #[test]
    fn connector_values_of_the_first_format_open_and_are_bound_to_the_purpose() {
        let s = seal_box_v1(A, "connection-credentials", b"sk_live");
        let ring = KeyRing::from_secrets(B, &[A]).unwrap();
        assert_eq!(
            open_box_v1(&ring, "connection-credentials", &s).unwrap(),
            b"sk_live"
        );
        assert_eq!(open_box_v1(&ring, "other", &s), Err(KeyError::CannotOpen));
        assert_eq!(
            open_any(&ring, "connection-credentials", None, &s).unwrap(),
            b"sk_live"
        );
        let fresh = ring.seal("connection-credentials", b"x").unwrap();
        assert_eq!(
            open_any(&ring, "connection-credentials", None, &fresh).unwrap(),
            b"x"
        );
        assert!(is_legacy(&s) && !is_legacy(&fresh));
    }
}
