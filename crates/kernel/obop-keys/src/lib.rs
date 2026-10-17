//! The server key ring (B3, D59) and one envelope for secrets at rest.
//!
//! `OBOP_SECRET_KEY` is the current key; `OBOP_SECRET_KEY_PREVIOUS` is a comma-separated list of
//! earlier keys that are still needed to open values sealed before a rotation. Every key has an
//! id (the first eight hex digits of a SHA-256 over the key with a fixed prefix, so the id says
//! nothing usable about the key). A sealed value carries the id of the key that sealed it, so
//! opening picks the right key at once and a missing key is reported by id, never by content.
//! New values are always sealed with the current key; `obop secrets rotate` re-seals old ones.
//!
//! Nothing here ever prints, logs or returns a key, a plaintext or a sealed value in an error.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use hkdf::Hkdf;
use obop_storage::{Backend, Fields};
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// Backend model that holds the key generated for an instance that has no `OBOP_SECRET_KEY`
/// (not a registry model: no command reads it). The name is the one the connectors used first.
pub const GENERATED_MODEL: &str = "connector_key";
const SALT: &[u8] = b"obop-envelope-v2";
const TAG: &str = "e2";

/// The fixed key dev and test setups may opt into. It is public knowledge: never for real data.
pub const INSECURE_DEV_KEY: [u8; 32] = *b"obop-insecure-dev-key-0123456789";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    /// `OBOP_SECRET_KEY` or `OBOP_SECRET_KEY_PREVIOUS`.
    Environment,
    /// Generated and stored in the database next to the data.
    Generated,
    /// The public development key (`OBOP_DEV_INSECURE_KEY`).
    Development,
}

/// One key. The raw text is kept only to derive material; it is redacted from `Debug`.
#[derive(Clone)]
pub struct Key {
    id: String,
    raw: String,
    source: KeySource,
}

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Key({}, {:?}, redacted)", self.id, self.source)
    }
}

impl Key {
    /// A key from its text (any string of 32 or more characters; 64 hex digits recommended).
    pub fn new(raw: &str, source: KeySource) -> Result<Key, String> {
        let raw = raw.trim();
        if raw.len() < 32 {
            return Err(
                "a secret key must be at least 32 characters (use `openssl rand -hex 32`)".into(),
            );
        }
        Ok(Key {
            id: key_id(raw),
            raw: raw.to_string(),
            source,
        })
    }

    /// The short, non-secret id that sealed values carry.
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn source(&self) -> KeySource {
        self.source
    }

    /// The key text, for consumers that derive their own material (the authentication crate).
    pub fn expose(&self) -> &str {
        &self.raw
    }

    fn cipher(&self, purpose: &str) -> ChaCha20Poly1305 {
        let mut k = [0u8; 32];
        Hkdf::<Sha256>::new(Some(SALT), self.raw.as_bytes())
            .expand(format!("seal:{purpose}").as_bytes(), &mut k)
            .expect("32 bytes is a valid HKDF output length");
        ChaCha20Poly1305::new((&k).into())
    }

    /// A 32-byte subkey for one purpose.
    pub fn subkey(&self, purpose: &str) -> [u8; 32] {
        let mut k = [0u8; 32];
        Hkdf::<Sha256>::new(Some(SALT), self.raw.as_bytes())
            .expand(format!("sub:{purpose}").as_bytes(), &mut k)
            .expect("32 bytes is a valid HKDF output length");
        k
    }
}

/// The id of a key given as text.
pub fn key_id(raw: &str) -> String {
    let d = Sha256::digest(format!("obop-key-id/v1:{}", raw.trim()).as_bytes());
    hex(&d[..4])
}

/// Why a sealed value did not open. Messages carry key ids only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyError {
    /// No key at all is configured.
    NoKey,
    /// The value was sealed with a key that is not in the ring.
    UnknownKey(String),
    /// Not an envelope of this format.
    NotSealed,
    Malformed,
    /// The key (or purpose) is wrong, or the value was changed.
    CannotOpen,
}

impl std::fmt::Display for KeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyError::NoKey => write!(f, "no secret key is configured (set OBOP_SECRET_KEY)"),
            KeyError::UnknownKey(id) => write!(
                f,
                "sealed with key {id}, which is not configured: put it in OBOP_SECRET_KEY_PREVIOUS"
            ),
            KeyError::NotSealed => write!(f, "value is not sealed"),
            KeyError::Malformed => write!(f, "sealed value is malformed"),
            KeyError::CannotOpen => write!(
                f,
                "the secret does not open: wrong secret key, wrong purpose or a tampered record"
            ),
        }
    }
}

impl std::error::Error for KeyError {}

impl From<KeyError> for String {
    fn from(e: KeyError) -> String {
        e.to_string()
    }
}

/// The current key first, then the keys that may still open old values.
#[derive(Clone, Default)]
pub struct KeyRing {
    keys: Vec<Key>,
}

impl std::fmt::Debug for KeyRing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(&self.keys).finish()
    }
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn dev_flag() -> bool {
    env("OBOP_DEV_INSECURE_KEY").is_some_and(|v| matches!(v.trim(), "1" | "true" | "yes" | "on"))
}

impl KeyRing {
    /// A ring from explicit key texts (tests, tooling): `current`, then `previous`.
    pub fn from_secrets(current: &str, previous: &[&str]) -> Result<KeyRing, String> {
        let mut r = KeyRing::default();
        r.push(Key::new(current, KeySource::Environment)?);
        for p in previous {
            r.push(Key::new(p, KeySource::Environment)?);
        }
        Ok(r)
    }

    /// A fixed ring for tests.
    pub fn for_tests() -> KeyRing {
        KeyRing::from_secrets("test-only-master-key-not-secret!!", &[]).expect("valid test key")
    }

    fn push(&mut self, k: Key) {
        if !self.keys.iter().any(|x| x.id == k.id) {
            self.keys.push(k);
        }
    }

    /// The ring named by the environment alone: `OBOP_SECRET_KEY`, `OBOP_SECRET_KEY_PREVIOUS`,
    /// and the public key when `OBOP_DEV_INSECURE_KEY` is on. Empty when nothing is set.
    pub fn from_env() -> Result<KeyRing, String> {
        Self::from_vars(
            env("OBOP_SECRET_KEY").as_deref(),
            env("OBOP_SECRET_KEY_PREVIOUS").as_deref(),
            dev_flag(),
        )
    }

    /// As `from_env`, with the variables given explicitly.
    pub fn from_vars(
        current: Option<&str>,
        previous: Option<&str>,
        dev: bool,
    ) -> Result<KeyRing, String> {
        let mut r = KeyRing::default();
        if let Some(c) = current.map(str::trim).filter(|c| !c.is_empty()) {
            r.push(
                Key::new(c, KeySource::Environment).map_err(|e| format!("OBOP_SECRET_KEY: {e}"))?,
            );
        } else if dev {
            let raw = String::from_utf8_lossy(&INSECURE_DEV_KEY).to_string();
            r.push(Key::new(&raw, KeySource::Development)?);
        }
        for (i, p) in previous
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .enumerate()
        {
            r.push(
                Key::new(p, KeySource::Environment)
                    .map_err(|e| format!("OBOP_SECRET_KEY_PREVIOUS entry {}: {e}", i + 1))?,
            );
        }
        Ok(r)
    }

    /// The ring a server process uses: the environment, and when there is no current key, the
    /// key kept in the database (created on first use). A key stored in the database stays in
    /// the ring as a previous key when `OBOP_SECRET_KEY` is later set, so what it sealed still
    /// opens until `obop secrets rotate` has re-sealed it.
    pub fn for_backend(backend: &Arc<dyn Backend>) -> Result<KeyRing, String> {
        let mut r = Self::from_vars(
            env("OBOP_SECRET_KEY").as_deref(),
            env("OBOP_SECRET_KEY_PREVIOUS").as_deref(),
            false,
        )?;
        let stored = Self::stored_key(backend)?;
        if r.is_empty() {
            let k = match stored {
                Some(k) => k,
                None => {
                    let raw = hex(&random::<32>());
                    let mut f = Fields::new();
                    f.insert("key".into(), raw.clone().into());
                    match backend.insert(GENERATED_MODEL, "master", &f) {
                        Ok(()) => Key::new(&raw, KeySource::Generated)?,
                        // Another process created it first: use theirs.
                        Err(_) => {
                            Self::stored_key(backend)?.ok_or("could not create the secret key")?
                        }
                    }
                }
            };
            r.push(k);
        } else if let Some(k) = stored {
            r.push(k);
        }
        Ok(r)
    }

    fn stored_key(backend: &Arc<dyn Backend>) -> Result<Option<Key>, String> {
        let Some(rec) = backend.get(GENERATED_MODEL, "master")? else {
            return Ok(None);
        };
        let k = rec
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or("stored secret key is malformed")?;
        Ok(Some(Key::new(k, KeySource::Generated)?))
    }

    pub fn current(&self) -> Option<&Key> {
        self.keys.first()
    }

    pub fn find(&self, id: &str) -> Option<&Key> {
        self.keys.iter().find(|k| k.id == id)
    }

    pub fn keys(&self) -> &[Key] {
        &self.keys
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn previous(&self) -> &[Key] {
        self.keys.get(1..).unwrap_or(&[])
    }

    /// Seal `plain` for `purpose` with the current key. The purpose is bound in as associated
    /// data, so a value cannot be moved to another record or use.
    pub fn seal(&self, purpose: &str, plain: &[u8]) -> Result<String, KeyError> {
        let key = self.current().ok_or(KeyError::NoKey)?;
        Ok(seal_with(key, purpose, plain))
    }

    /// Open a value sealed by this ring (any of its keys).
    pub fn open(&self, purpose: &str, sealed: &str) -> Result<Vec<u8>, KeyError> {
        let (kid, nonce, ct) = parse(sealed)?;
        let key = self.find(&kid).ok_or(KeyError::UnknownKey(kid))?;
        open_with(key, purpose, &nonce, &ct)
    }

    /// Sealing text, opening text.
    pub fn seal_str(&self, purpose: &str, plain: &str) -> Result<String, KeyError> {
        self.seal(purpose, plain.as_bytes())
    }

    pub fn open_str(&self, purpose: &str, sealed: &str) -> Result<String, KeyError> {
        String::from_utf8(self.open(purpose, sealed)?).map_err(|_| KeyError::Malformed)
    }
}

/// Whether `s` looks like an envelope of this crate.
pub fn is_sealed(s: &str) -> bool {
    s.starts_with("e2.")
}

/// The id of the key that sealed `s`, when it is an envelope of this crate.
pub fn sealed_key_id(s: &str) -> Option<String> {
    parse(s).ok().map(|(kid, _, _)| kid)
}

pub fn seal_with(key: &Key, purpose: &str, plain: &[u8]) -> String {
    let nonce = random::<12>();
    let ct = key
        .cipher(purpose)
        .encrypt(
            &Nonce::from(nonce),
            Payload {
                msg: plain,
                aad: purpose.as_bytes(),
            },
        )
        .expect("encryption of in-memory input cannot fail");
    format!("{TAG}.{}.{}.{}", key.id, hex(&nonce), hex(&ct))
}

fn parse(sealed: &str) -> Result<(String, Vec<u8>, Vec<u8>), KeyError> {
    let mut parts = sealed.split('.');
    let (Some(TAG), Some(kid), Some(n), Some(c), None) = (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) else {
        return Err(KeyError::NotSealed);
    };
    let nonce = unhex(n).ok_or(KeyError::Malformed)?;
    let ct = unhex(c).ok_or(KeyError::Malformed)?;
    if nonce.len() != 12 || ct.len() < 16 {
        return Err(KeyError::Malformed);
    }
    Ok((kid.to_string(), nonce, ct))
}

fn open_with(key: &Key, purpose: &str, nonce: &[u8], ct: &[u8]) -> Result<Vec<u8>, KeyError> {
    key.cipher(purpose)
        .decrypt(
            &Nonce::try_from(nonce).map_err(|_| KeyError::Malformed)?,
            Payload {
                msg: ct,
                aad: purpose.as_bytes(),
            },
        )
        .map_err(|_| KeyError::CannotOpen)
}

pub fn random<const N: usize>() -> [u8; N] {
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

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn round_trip_with_key_id_and_purpose_binding() {
        let r = KeyRing::from_secrets(A, &[]).unwrap();
        let s = r.seal("conn:1", b"sk_live_abc").unwrap();
        assert!(is_sealed(&s) && !s.contains("sk_live"));
        assert_eq!(sealed_key_id(&s).unwrap(), key_id(A));
        assert_eq!(r.open("conn:1", &s).unwrap(), b"sk_live_abc");
        assert_eq!(r.open("conn:2", &s), Err(KeyError::CannotOpen));
        assert_ne!(s, r.seal("conn:1", b"sk_live_abc").unwrap(), "fresh nonce");
    }

    #[test]
    fn rotation_opens_old_values_with_the_previous_key() {
        let old = KeyRing::from_secrets(A, &[]).unwrap();
        let s = old.seal("p", b"x").unwrap();
        let new = KeyRing::from_secrets(B, &[A]).unwrap();
        assert_eq!(new.open("p", &s).unwrap(), b"x");
        assert_eq!(
            sealed_key_id(&new.seal("p", b"x").unwrap()).unwrap(),
            key_id(B)
        );
        // Without the previous key the error names the key id and nothing else.
        let only_new = KeyRing::from_secrets(B, &[]).unwrap();
        let e = only_new.open("p", &s).unwrap_err();
        assert_eq!(e, KeyError::UnknownKey(key_id(A)));
        assert!(!e.to_string().contains(A));
    }

    #[test]
    fn tampering_and_garbage_fail_without_echo() {
        let r = KeyRing::from_secrets(A, &[]).unwrap();
        let s = r.seal("p", b"x").unwrap();
        let mut bad = s.clone().into_bytes();
        let last = bad.len() - 1;
        bad[last] = if bad[last] == b'0' { b'1' } else { b'0' };
        assert_eq!(
            r.open("p", &String::from_utf8(bad).unwrap()),
            Err(KeyError::CannotOpen)
        );
        assert_eq!(r.open("p", "garbage"), Err(KeyError::NotSealed));
        assert_eq!(r.open("p", "e2.zz.00.00"), Err(KeyError::Malformed));
        assert_eq!(KeyRing::default().seal("p", b"x"), Err(KeyError::NoKey));
    }

    #[test]
    fn rings_parse_from_variables() {
        let r = KeyRing::from_vars(Some(A), Some(&format!(" {B} , ,{A}")), false).unwrap();
        assert_eq!(r.keys().len(), 2, "duplicates collapse");
        assert_eq!(r.current().unwrap().id(), key_id(A));
        assert!(KeyRing::from_vars(Some("short"), None, false).is_err());
        assert!(KeyRing::from_vars(Some(A), Some("short"), false).is_err());
        assert!(KeyRing::from_vars(None, None, false).unwrap().is_empty());
        assert_eq!(
            KeyRing::from_vars(None, None, true)
                .unwrap()
                .current()
                .unwrap()
                .source(),
            KeySource::Development
        );
        let dbg = format!("{r:?}");
        assert!(!dbg.contains(A) && !dbg.contains(B));
    }

    #[test]
    fn a_generated_key_is_created_once_and_stays_as_a_previous_key() {
        let backend = obop_storage::open("memory:").unwrap();
        let a = KeyRing::for_backend(&backend).unwrap();
        let b = KeyRing::for_backend(&backend).unwrap();
        assert_eq!(a.current().unwrap().source(), KeySource::Generated);
        let s = a.seal("p", b"x").unwrap();
        assert_eq!(b.open("p", &s).unwrap(), b"x");
    }
}
