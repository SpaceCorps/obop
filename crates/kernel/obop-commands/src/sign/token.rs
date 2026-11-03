//! The secret of a signing link and the record id derived from it.
//!
//! A link is `/sign/<request id>/<token>`. The token is 32 bytes from the operating system's
//! random source, written as 64 lowercase hexadecimal characters; it is shown once, when the link
//! is created, and is never stored, logged or put into an error. What is stored is the record id of
//! the `sign_link` row, the SHA-256 of a domain string, the request id and the token. The request
//! id is part of the hash, so a token cannot be pointed at another request by editing a field: the
//! door recomputes the id from the address it was given.

use sha2::{Digest, Sha256};

const DOMAIN: &[u8] = b"obop-sign-link-v1\0";

/// A fresh token: 256 bits from the operating system, as 64 lowercase hexadecimal characters.
/// There is no fallback: if the system's randomness fails, no link is made.
pub(crate) fn mint() -> Result<String, String> {
    let mut b = [0u8; 32];
    getrandom::fill(&mut b)
        .map_err(|_| "the system's random source is not available".to_string())?;
    Ok(hex(&b))
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// The record id of the link made from `token` for request `rid`.
pub(crate) fn link_id(rid: &str, token: &str) -> String {
    let mut h = Sha256::new();
    h.update(DOMAIN);
    h.update(rid.as_bytes());
    h.update([0u8]);
    h.update(token.as_bytes());
    hex(&h.finalize())
}

/// The token has the shape `mint` makes: exactly 64 lowercase hexadecimal characters.
pub(crate) fn token_shape(t: &str) -> bool {
    t.len() == 64 && t.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A record id of a request as the address carries it: 1 to 64 of letters, digits, `-` and `_`.
pub(crate) fn rid_shape(r: &str) -> bool {
    (1..=64).contains(&r.len())
        && r.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Compare two texts without stopping at the first difference.
pub(crate) fn ct_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        diff |= usize::from(a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0));
    }
    diff == 0
}

/// The SHA-256 of `bytes` as lowercase hexadecimal (the id of a blob).
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// A short stand-in for a value in a counter key or a log line: the first 24 hexadecimal
/// characters of the SHA-256 of `domain` and `value`. The raw value (an address, a token) never
/// goes into a key.
pub(crate) fn short_hash(domain: &str, value: &str) -> String {
    let mut h = Sha256::new();
    h.update(domain.as_bytes());
    h.update(value.as_bytes());
    hex(&h.finalize())[..24].to_string()
}
