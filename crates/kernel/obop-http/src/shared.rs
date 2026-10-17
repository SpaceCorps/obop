//! Session state shared between server processes (B1, D59): the masking vaults of REST
//! sessions and of MCP sessions, and the rate limits.
//!
//! A vault holds personal data (the values behind mask tokens), so it is kept sealed under the
//! server key ring in the storage backend, with an expiry, and keyed by a hash of the bearer
//! token (never the token itself). Mask tokens are sequence numbers inside a vault (`PERSON_3`),
//! so two processes must not both add to one vault at once: an agent's call holds a
//! cross-process lock on its vault for the duration of the call, exactly as the in-process mutex
//! did. A person's call takes the lock only when the call actually added tokens, which for the
//! default policy (people see their data in clear) it never does.

use obop_commands::Vault;
use obop_keys::KeyRing;
use obop_storage::Backend;
use serde_json::Value as J;
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// A REST vault is forgotten this long after its last change.
const VAULT_TTL_MS: u64 = 24 * 3600 * 1000;

pub(crate) struct SharedState {
    pub backend: Arc<dyn Backend>,
    pub ring: KeyRing,
}

/// A stable, non-secret id for a bearer token.
pub(crate) fn token_id(token: &str) -> String {
    Sha256::digest(format!("obop-session:{token}").as_bytes())[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

impl SharedState {
    /// The backend and ring of the dispatcher when it has them (a server process), else a
    /// private in-memory pair (tests, embedding): then nothing is shared, as before.
    pub fn for_dispatcher(d: &obop_commands::Dispatcher) -> Arc<SharedState> {
        if let Some((backend, ring)) = d.shared_parts() {
            return Arc::new(SharedState { backend, ring });
        }
        let key = obop_keys::hex(&obop_keys::random::<32>());
        Arc::new(SharedState {
            backend: obop_storage::open("memory:").expect("memory backend"),
            ring: KeyRing::from_secrets(&key, &[]).expect("random key"),
        })
    }

    pub fn seal_json(&self, purpose: &str, j: &J) -> Result<String, String> {
        self.ring
            .seal_str(purpose, &j.to_string())
            .map_err(String::from)
    }

    pub fn open_json(&self, purpose: &str, sealed: &str) -> Option<J> {
        let text = self.ring.open_str(purpose, sealed).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Housekeeping at most once a minute across all processes: drop expired entries.
    pub fn maybe_purge(&self) {
        if self
            .backend
            .counter_add("housekeeping.purge", 60_000, false)
            .is_ok_and(|n| n == 1)
        {
            let _ = self.backend.purge_expired();
        }
    }

    /// The vault of one bearer token.
    pub fn vault(self: &Arc<Self>, token: &str, agent: bool) -> VaultRef {
        VaultRef {
            state: self.clone(),
            id: token_id(token),
            agent,
        }
    }
}

/// One REST session's vault in the shared store.
pub(crate) struct VaultRef {
    state: Arc<SharedState>,
    id: String,
    agent: bool,
}

impl VaultRef {
    fn load(&self) -> Vault {
        self.state
            .backend
            .kv_get(&format!("vault:{}", self.id))
            .ok()
            .flatten()
            .and_then(|sealed| self.state.open_json(&format!("vault:{}", self.id), &sealed))
            .map(|j| Vault::from_json(&j))
            .unwrap_or_default()
    }

    fn save(&self, v: &Vault) -> Result<(), String> {
        let purpose = format!("vault:{}", self.id);
        let sealed = self.state.seal_json(&purpose, &v.to_json())?;
        self.state
            .backend
            .kv_put(&purpose, &sealed, Some(VAULT_TTL_MS))
    }

    /// Run `f` with the vault. The vault is saved if `f` changed it.
    pub fn with<T>(&self, f: impl FnOnce(&mut Vault) -> T) -> Result<T, String> {
        let lock_key = format!("vault-lock:{}", self.id);
        let mut guard = if self.agent {
            Some(self.state.backend.lock(&lock_key)?)
        } else {
            None
        };
        let mut vault = self.load();
        let out = f(&mut vault);
        if vault.is_dirty() {
            if guard.is_none() {
                guard = Some(self.state.backend.lock(&lock_key)?);
            }
            self.save(&vault)?;
        }
        drop(guard);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> Arc<SharedState> {
        Arc::new(SharedState {
            backend: obop_storage::open("memory:").unwrap(),
            ring: KeyRing::for_tests(),
        })
    }

    #[test]
    fn a_vault_is_shared_sealed_and_keyed_by_a_hash() {
        let s = state();
        let a = s.vault("obop_secrettoken", true);
        let t = a
            .with(|v| v.mask("PERSON", "party:1", "Ada Lovelace"))
            .unwrap();
        // A second handle (another process) sees the token and continues the numbering.
        let b = s.vault("obop_secrettoken", true);
        assert_eq!(
            b.with(|v| v.mask("PERSON", "party:1", "Ada Lovelace"))
                .unwrap(),
            t
        );
        let t2 = b.with(|v| v.mask("PERSON", "party:2", "Alan")).unwrap();
        assert_ne!(t, t2);
        assert_eq!(
            a.with(|v| v.restore(&t2, |_| true).map(String::from))
                .unwrap(),
            Ok("Alan".to_string())
        );
        // Only the hash of the bearer token is a key, and the stored value is sealed.
        let key = format!("vault:{}", token_id("obop_secrettoken"));
        let raw = s.backend.kv_get(&key).unwrap().unwrap();
        assert!(raw.starts_with("e2.") && !raw.contains("Ada") && !key.contains("secrettoken"));
        // Another token has its own, empty vault.
        let other = s.vault("obop_other", false);
        assert!(other.with(|v| v.restore(&t, |_| true).is_err()).unwrap());
    }

    #[test]
    fn a_person_who_masks_nothing_writes_nothing() {
        let s = state();
        let p = s.vault("obop_person", false);
        p.with(|_| ()).unwrap();
        assert!(
            s.backend
                .kv_get(&format!("vault:{}", token_id("obop_person")))
                .unwrap()
                .is_none()
        );
    }
}
