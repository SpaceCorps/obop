//! Envelope for secrets at rest: HKDF-SHA256 derives one key per purpose from a master key, and
//! ChaCha20-Poly1305 seals each value with a fresh nonce and the purpose as associated data, so a
//! sealed credential cannot be moved to another purpose or another connection.
//!
//! The master key comes from `OBOP_SECRET_KEY` (any string of 32 or more characters; it is run
//! through HKDF, never used directly). Without it a random key is generated once and kept in the
//! database next to the data: that protects against a leaked export of one table, not against a
//! stolen database, and the compliance checklist says so. The module is deliberately small so a
//! KMS or the shared helper of the authentication crate can replace it behind the same two calls
//! (`seal`, `open`).

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use hkdf::Hkdf;
use obop_storage::{Backend, Fields};
use sha2::Sha256;
use std::sync::Arc;

const PREFIX: &str = "obs1:";
const SALT: &[u8] = b"obop-secretbox-v1";
/// Backend model that holds a generated master key (not a registry model: no command reads it).
const KEY_MODEL: &str = "connector_key";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    /// `OBOP_SECRET_KEY`.
    Environment,
    /// Generated and stored in the database next to the data.
    Generated,
}

#[derive(Clone)]
pub struct SecretBox {
    master: [u8; 32],
    source: KeySource,
}

impl std::fmt::Debug for SecretBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecretBox({:?}, key redacted)", self.source)
    }
}

fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).expect("operating system randomness");
    b
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

impl SecretBox {
    /// A box over a master secret (any length of 32 or more bytes of input keying material).
    pub fn from_secret(secret: &str) -> Result<Self, String> {
        if secret.len() < 32 {
            return Err("OBOP_SECRET_KEY must be at least 32 characters".into());
        }
        Ok(Self::derive_master(
            secret.as_bytes(),
            KeySource::Environment,
        ))
    }

    fn derive_master(ikm: &[u8], source: KeySource) -> Self {
        let hk = Hkdf::<Sha256>::new(Some(SALT), ikm);
        let mut master = [0u8; 32];
        hk.expand(b"master", &mut master).expect("32 bytes");
        Self { master, source }
    }

    /// The box for this process: `OBOP_SECRET_KEY` if set, else the key kept in the database
    /// (created on first use).
    pub fn open_for(backend: &Arc<dyn Backend>) -> Result<Self, String> {
        if let Ok(k) = std::env::var("OBOP_SECRET_KEY")
            && !k.is_empty()
        {
            return Self::from_secret(&k);
        }
        if let Some(rec) = backend.get(KEY_MODEL, "master")?
            && let Some(k) = rec.get("key").and_then(|v| v.as_str())
        {
            return Ok(Self::derive_master(k.as_bytes(), KeySource::Generated));
        }
        let k = hex(&random::<32>());
        let mut f = Fields::new();
        f.insert("key".into(), k.clone().into());
        match backend.insert(KEY_MODEL, "master", &f) {
            Ok(()) => Ok(Self::derive_master(k.as_bytes(), KeySource::Generated)),
            // Another process created it first: use theirs.
            Err(_) => {
                let rec = backend
                    .get(KEY_MODEL, "master")?
                    .ok_or("could not create the secret key")?;
                let k = rec
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or("stored secret key is malformed")?;
                Ok(Self::derive_master(k.as_bytes(), KeySource::Generated))
            }
        }
    }

    /// A fixed key for tests.
    pub fn for_tests() -> Self {
        Self::derive_master(b"test-only-master-key-not-secret!!", KeySource::Environment)
    }

    pub fn source(&self) -> KeySource {
        self.source
    }

    /// A 32-byte subkey for one purpose (for example `pseudonym:<connection id>`).
    pub fn subkey(&self, purpose: &str) -> [u8; 32] {
        let hk = Hkdf::<Sha256>::new(Some(SALT), &self.master);
        let mut out = [0u8; 32];
        hk.expand(purpose.as_bytes(), &mut out).expect("32 bytes");
        out
    }

    pub fn is_sealed(s: &str) -> bool {
        s.starts_with(PREFIX)
    }

    /// Seal `plaintext` for `purpose`. The result is text safe for a JSON document.
    pub fn seal(&self, purpose: &str, plaintext: &str) -> String {
        let key = self.subkey(&format!("seal:{purpose}"));
        let cipher = ChaCha20Poly1305::new_from_slice(&key).expect("32-byte key");
        let nonce_bytes = random::<12>();
        let ct = cipher
            .encrypt(
                &Nonce::try_from(&nonce_bytes[..]).expect("12-byte nonce"),
                Payload {
                    msg: plaintext.as_bytes(),
                    aad: purpose.as_bytes(),
                },
            )
            .expect("encryption cannot fail for in-memory input");
        format!("{PREFIX}{}{}", hex(&nonce_bytes), hex(&ct))
    }

    /// Open a sealed value. The error never contains the input.
    pub fn open(&self, purpose: &str, sealed: &str) -> Result<String, String> {
        let body = sealed.strip_prefix(PREFIX).ok_or("value is not sealed")?;
        let raw = unhex(body).ok_or("sealed value is malformed")?;
        if raw.len() < 12 + 16 {
            return Err("sealed value is malformed".into());
        }
        let (nonce, ct) = raw.split_at(12);
        let key = self.subkey(&format!("seal:{purpose}"));
        let cipher = ChaCha20Poly1305::new_from_slice(&key).expect("32-byte key");
        let pt = cipher
            .decrypt(
                &Nonce::try_from(nonce).map_err(|_| "sealed value is malformed".to_string())?,
                Payload {
                    msg: ct,
                    aad: purpose.as_bytes(),
                },
            )
            .map_err(|_| "sealed value cannot be opened (wrong key or purpose)".to_string())?;
        String::from_utf8(pt).map_err(|_| "sealed value is not text".to_string())
    }
}

/// Short, safe hint of a secret for the UI: never more than the last four characters, and
/// nothing at all for short values.
pub fn hint(secret: &str) -> String {
    let n = secret.chars().count();
    if n < 12 {
        "set".into()
    } else {
        let tail: String = secret.chars().skip(n - 4).collect();
        format!("set, ends {tail}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_purpose_binding() {
        let b = SecretBox::for_tests();
        let s = b.seal("connection:1", "sk_live_abcdef");
        assert!(SecretBox::is_sealed(&s) && !s.contains("sk_live"));
        assert_eq!(b.open("connection:1", &s).unwrap(), "sk_live_abcdef");
        // Another purpose, a tampered value and garbage all fail without echoing the input.
        assert!(b.open("connection:2", &s).is_err());
        let mut bad = s.clone();
        bad.push('0');
        assert!(b.open("connection:1", &bad).is_err());
        let e = b.open("connection:1", "obs1:zz").unwrap_err();
        assert!(!e.contains("zz"));
        // Sealing twice differs (fresh nonce).
        assert_ne!(s, b.seal("connection:1", "sk_live_abcdef"));
    }

    #[test]
    fn another_master_cannot_open() {
        let a = SecretBox::for_tests();
        let b = SecretBox::from_secret("another-master-key-with-32-chars!!!").unwrap();
        let s = a.seal("p", "x");
        assert!(b.open("p", &s).is_err());
        assert!(SecretBox::from_secret("short").is_err());
    }

    #[test]
    fn generated_key_is_created_once_and_reused() {
        let backend = obop_storage::open("memory:").unwrap();
        // SAFETY-free: tests of this module never set the variable.
        let a = SecretBox::open_for(&backend).unwrap();
        let b = SecretBox::open_for(&backend).unwrap();
        let s = a.seal("p", "x");
        assert_eq!(b.open("p", &s).unwrap(), "x");
        assert_eq!(a.source(), KeySource::Generated);
    }

    #[test]
    fn hints_reveal_at_most_four_characters() {
        assert_eq!(hint("short"), "set");
        assert_eq!(hint("sk_live_1234567890wxyz"), "set, ends wxyz");
        assert!(!hint("sk_live_1234567890wxyz").contains("1234567890"));
    }

    #[test]
    fn subkeys_differ_per_purpose() {
        let b = SecretBox::for_tests();
        assert_ne!(b.subkey("a"), b.subkey("b"));
        assert_eq!(b.subkey("a"), b.subkey("a"));
    }
}
