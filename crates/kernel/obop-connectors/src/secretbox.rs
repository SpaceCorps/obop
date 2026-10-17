//! Envelope for secrets at rest, on the server key ring (B3).
//!
//! The ring comes from `OBOP_SECRET_KEY` (current) and `OBOP_SECRET_KEY_PREVIOUS` (earlier keys,
//! comma separated); without `OBOP_SECRET_KEY` a random key is generated once and kept in the
//! database next to the data (see `obop_keys::KeyRing::for_backend`). Each value is sealed with
//! ChaCha20-Poly1305 under a key derived by HKDF for its purpose, with the purpose as associated
//! data, so a sealed credential cannot be moved to another purpose or another connection. A
//! sealed value names the key that sealed it (`e2.<key id>...`), so a rotation re-seals exactly
//! what an old key still protects. The first format (`obs1:`) is still opened by trying each key
//! of the ring.
//!
//! Pseudonym keys (`pseudonym_key`) are not derived from the ring on every use: the first use
//! derives one, seals it and stores it, so pseudonyms already handed to receivers stay valid
//! when the ring is rotated.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use hkdf::Hkdf;
use obop_keys::{Key, KeyRing};
use obop_storage::{Backend, Fields};
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const PREFIX: &str = "obs1:";
const SALT: &[u8] = b"obop-secretbox-v1";
/// Where sealed pseudonym keys live (not a registry model: no command reads it).
pub const PSEUDONYM_KEY_MODEL: &str = "connector_pseudonym_key";
const KEY_CACHE: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    /// `OBOP_SECRET_KEY`.
    Environment,
    /// Generated and stored in the database next to the data.
    Generated,
}

#[derive(Clone)]
pub struct SecretBox {
    ring: KeyRing,
    store: Option<Arc<dyn Backend>>,
    cache: Arc<Mutex<HashMap<String, ([u8; 32], Instant)>>>,
}

impl std::fmt::Debug for SecretBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecretBox({:?}, key redacted)", self.source())
    }
}

pub use obop_keys::{hex, unhex};


/// The master the first format derived from a key's text.
fn legacy_master(raw: &str) -> [u8; 32] {
    let mut master = [0u8; 32];
    Hkdf::<Sha256>::new(Some(SALT), raw.as_bytes())
        .expand(b"master", &mut master)
        .expect("32 bytes");
    master
}

fn legacy_subkey(raw: &str, purpose: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    Hkdf::<Sha256>::new(Some(SALT), &legacy_master(raw))
        .expand(purpose.as_bytes(), &mut out)
        .expect("32 bytes");
    out
}

fn legacy_open(raw: &str, purpose: &str, raw_bytes: &[u8]) -> Option<Vec<u8>> {
    if raw_bytes.len() < 12 + 16 {
        return None;
    }
    let (nonce, ct) = raw_bytes.split_at(12);
    let key = legacy_subkey(raw, &format!("seal:{purpose}"));
    ChaCha20Poly1305::new_from_slice(&key)
        .ok()?
        .decrypt(
            &Nonce::try_from(nonce).ok()?,
            Payload {
                msg: ct,
                aad: purpose.as_bytes(),
            },
        )
        .ok()
}

impl SecretBox {
    /// A box over one master secret (any string of 32 or more characters).
    pub fn from_secret(secret: &str) -> Result<Self, String> {
        let key = Key::new(secret, obop_keys::KeySource::Environment)
            .map_err(|_| "OBOP_SECRET_KEY must be at least 32 characters".to_string())?;
        Ok(Self::from_ring_inner(KeyRing::from_secrets(key.expose(), &[])?, None))
    }

    /// A box over an explicit ring (tooling, tests).
    pub fn from_ring(ring: KeyRing) -> Result<Self, String> {
        if ring.is_empty() {
            return Err("no secret key is configured".into());
        }
        Ok(Self::from_ring_inner(ring, None))
    }

    fn from_ring_inner(ring: KeyRing, store: Option<Arc<dyn Backend>>) -> Self {
        Self {
            ring,
            store,
            cache: Arc::default(),
        }
    }

    /// The box for this process: the ring named by the environment, or the key kept in the
    /// database (created on first use).
    pub fn open_for(backend: &Arc<dyn Backend>) -> Result<Self, String> {
        Ok(Self::from_ring_inner(
            KeyRing::for_backend(backend)?,
            Some(backend.clone()),
        ))
    }

    /// A fixed key for tests.
    pub fn for_tests() -> Self {
        Self::from_ring_inner(KeyRing::for_tests(), None)
    }

    pub fn source(&self) -> KeySource {
        match self.ring.current().map(Key::source) {
            Some(obop_keys::KeySource::Generated) => KeySource::Generated,
            _ => KeySource::Environment,
        }
    }

    pub fn ring(&self) -> &KeyRing {
        &self.ring
    }

    fn current(&self) -> &Key {
        self.ring.current().expect("a SecretBox always has a key")
    }

    /// A 32-byte subkey of the current key for one purpose (not stored; changes on rotation).
    pub fn subkey(&self, purpose: &str) -> [u8; 32] {
        legacy_subkey(self.current().expose(), purpose)
    }

    /// The HMAC key behind the pseudonyms of `name` (for example `pseudonym:<connection id>`).
    /// Stored sealed on first use, so it survives a key rotation unchanged.
    pub fn pseudonym_key(&self, name: &str) -> [u8; 32] {
        let Some(store) = &self.store else {
            return self.subkey(name);
        };
        if let Some((k, at)) = self.cache.lock().unwrap().get(name)
            && at.elapsed() < KEY_CACHE
        {
            return *k;
        }
        let key = self.load_or_create_pseudonym_key(store, name, None);
        self.cache
            .lock()
            .unwrap()
            .insert(name.to_string(), (key, Instant::now()));
        key
    }

    /// Make sure the pseudonym key of `name` exists, derived from `anchor` (the key that sealed
    /// the connection's credentials) when it has to be created. For rotation.
    pub fn materialise_pseudonym_key(&self, name: &str, anchor: Option<&Key>) -> Result<(), String> {
        let Some(store) = &self.store else {
            return Ok(());
        };
        self.load_or_create_pseudonym_key(store, name, anchor);
        Ok(())
    }

    fn load_or_create_pseudonym_key(
        &self,
        store: &Arc<dyn Backend>,
        name: &str,
        anchor: Option<&Key>,
    ) -> [u8; 32] {
        let purpose = format!("pseudonym-key:{name}");
        let read = || -> Option<[u8; 32]> {
            let f = store.get(PSEUDONYM_KEY_MODEL, name).ok()??;
            let sealed = f.get("key")?.as_str()?;
            let bytes = self.open_bytes(&purpose, sealed).ok()?;
            <[u8; 32]>::try_from(bytes.as_slice()).ok()
        };
        if let Some(k) = read() {
            return k;
        }
        let derived = legacy_subkey(anchor.unwrap_or_else(|| self.current()).expose(), name);
        let mut f = Fields::new();
        f.insert("key".into(), self.seal_bytes(&purpose, &derived).into());
        // Another process may have created it first: then theirs is the one.
        match store.insert(PSEUDONYM_KEY_MODEL, name, &f) {
            Ok(()) => derived,
            Err(_) => read().unwrap_or(derived),
        }
    }

    /// Forget the stored pseudonym keys of a connection (revocation).
    pub fn forget_pseudonym_keys(&self, connection: &str) {
        if let Some(store) = &self.store {
            for name in [
                format!("pseudonym:{connection}"),
                format!("pseudonym:{connection}:vault"),
            ] {
                let _ = store.delete(PSEUDONYM_KEY_MODEL, &name);
                self.cache.lock().unwrap().remove(&name);
            }
        }
    }

    pub fn is_sealed(s: &str) -> bool {
        s.starts_with(PREFIX) || obop_keys::is_sealed(s)
    }

    /// The id of the key that sealed `s`, when its format says so (`None` for `obs1:`).
    pub fn key_id_of(s: &str) -> Option<String> {
        obop_keys::sealed_key_id(s)
    }

    fn seal_bytes(&self, purpose: &str, plain: &[u8]) -> String {
        self.ring
            .seal(purpose, plain)
            .expect("a SecretBox always has a key")
    }

    /// Seal `plaintext` for `purpose` with the current key. The result is text safe for a JSON
    /// document.
    pub fn seal(&self, purpose: &str, plaintext: &str) -> String {
        self.seal_bytes(purpose, plaintext.as_bytes())
    }

    fn open_bytes(&self, purpose: &str, sealed: &str) -> Result<Vec<u8>, String> {
        if let Some(body) = sealed.strip_prefix(PREFIX) {
            let raw = unhex(body).ok_or("sealed value is malformed")?;
            return self
                .ring
                .keys()
                .iter()
                .find_map(|k| legacy_open(k.expose(), purpose, &raw))
                .ok_or_else(|| "sealed value cannot be opened (wrong key or purpose)".to_string());
        }
        self.ring.open(purpose, sealed).map_err(|e| e.to_string())
    }

    /// Open a sealed value. The error never contains the input.
    pub fn open(&self, purpose: &str, sealed: &str) -> Result<String, String> {
        let pt = self.open_bytes(purpose, sealed)?;
        String::from_utf8(pt).map_err(|_| "sealed value is not text".to_string())
    }

    /// Re-seal a value with the current key if another key sealed it. `Ok(None)` when it is
    /// already sealed with the current key.
    pub fn reseal(&self, purpose: &str, sealed: &str) -> Result<Option<String>, String> {
        if Self::key_id_of(sealed).as_deref() == Some(self.current().id()) {
            // Still prove it opens, so a damaged value is reported rather than skipped.
            self.open_bytes(purpose, sealed)?;
            return Ok(None);
        }
        let plain = self.open_bytes(purpose, sealed)?;
        Ok(Some(self.seal_bytes(purpose, &plain)))
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

    /// A value in the first format, for the migration tests.
    pub(crate) fn seal_v1(raw: &str, purpose: &str, plaintext: &str) -> String {
        let key = legacy_subkey(raw, &format!("seal:{purpose}"));
        let nonce = obop_keys::random::<12>();
        let ct = ChaCha20Poly1305::new_from_slice(&key)
            .unwrap()
            .encrypt(
                &Nonce::try_from(&nonce[..]).unwrap(),
                Payload {
                    msg: plaintext.as_bytes(),
                    aad: purpose.as_bytes(),
                },
            )
            .unwrap();
        format!("{PREFIX}{}{}", hex(&nonce), hex(&ct))
    }

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

    const OLD: &str = "old-master-key-with-more-than-32-characters";
    const NEW: &str = "new-master-key-with-more-than-32-characters";

    #[test]
    fn values_from_before_the_key_ring_still_open_and_reseal() {
        let legacy = seal_v1(OLD, "connection-credentials", "sk_live_abc");
        assert!(legacy.starts_with("obs1:"));
        let b = SecretBox::from_ring(KeyRing::from_secrets(NEW, &[OLD]).unwrap()).unwrap();
        assert_eq!(b.open("connection-credentials", &legacy).unwrap(), "sk_live_abc");
        let fresh = b.reseal("connection-credentials", &legacy).unwrap().unwrap();
        assert!(fresh.starts_with("e2."));
        assert_eq!(b.open("connection-credentials", &fresh).unwrap(), "sk_live_abc");
        assert_eq!(b.reseal("connection-credentials", &fresh).unwrap(), None);
        // Without the old key the first-format value cannot be opened, and the error says so
        // without echoing anything.
        let only_new = SecretBox::from_secret(NEW).unwrap();
        assert!(only_new.open("connection-credentials", &legacy).is_err());
    }

    #[test]
    fn rotation_keeps_pseudonym_keys_stable() {
        let backend = obop_storage::open("memory:").unwrap();
        let mk = |ring| {
            let mut b = SecretBox::from_ring(ring).unwrap();
            b.store = Some(backend.clone());
            b
        };
        let before = mk(KeyRing::from_secrets(OLD, &[]).unwrap());
        let k1 = before.pseudonym_key("pseudonym:c1");
        assert_eq!(k1, before.subkey("pseudonym:c1"), "first use matches the derivation");
        // A different ring with the same stored key record: unchanged while the old key is kept.
        let after = mk(KeyRing::from_secrets(NEW, &[OLD]).unwrap());
        assert_eq!(after.pseudonym_key("pseudonym:c1"), k1);
        assert_ne!(after.subkey("pseudonym:c1"), k1, "the derivation itself would have changed");
        // The stored key is sealed.
        let rec = backend.get(PSEUDONYM_KEY_MODEL, "pseudonym:c1").unwrap().unwrap();
        assert!(rec["key"].as_str().unwrap().starts_with("e2."));
        after.forget_pseudonym_keys("c1");
        assert!(backend.get(PSEUDONYM_KEY_MODEL, "pseudonym:c1").unwrap().is_none());
    }
}
