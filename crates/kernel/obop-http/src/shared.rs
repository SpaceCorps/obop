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
    /// The stored vault, or an empty one when there is none. **A read that failed is an error,
    /// never an empty vault** (D74): a command run on an empty vault numbers its mask tokens from
    /// 1 again and the save then replaces the stored vault, so tokens the agent already holds would
    /// point at other people.
    fn load(&self) -> Result<Vault, String> {
        let key = format!("vault:{}", self.id);
        let Some(sealed) = self.state.backend.kv_get(&key)? else {
            return Ok(Vault::default());
        };
        Ok(self
            .state
            .open_json(&key, &sealed)
            .map(|j| Vault::from_json(&j))
            .unwrap_or_default())
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
        let mut vault = self.load()?;
        let out = f(&mut vault);
        if vault.is_dirty() {
            // The answer that `f` built names tokens that only the saved vault can restore. Save
            // it, trying again if the store is busy; the write is bookkeeping (it rests on what
            // `f` just did, not on a read that may have failed), so it is not refused for an
            // earlier refusal. If it still fails the request is answered 503, and when `f` had
            // already committed a change the 503 says so (`"applied":"partial"`).
            let saved = (|| {
                if guard.is_none() {
                    guard = Some(self.state.backend.lock(&lock_key)?);
                }
                obop_storage::fault::patiently(4, std::time::Duration::from_millis(150), || {
                    obop_storage::fault::bookkeeping(|| self.save(&vault))
                })
            })();
            if let Err(e) = saved {
                if obop_storage::fault::state().wrote {
                    obop_storage::fault::mark_partial();
                }
                return Err(e);
            }
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
    fn a_refused_read_of_the_vault_fails_the_call_and_leaves_the_stored_vault_alone() {
        // D74: a full pool refuses the read of the vault. The call must fail; it must not run on an
        // empty vault and save that over the stored one (the agent's tokens would then point at
        // other people).
        let (flaky, backend) = obop_storage::testing::Flaky::memory();
        let s = Arc::new(SharedState {
            backend,
            ring: KeyRing::for_tests(),
        });
        let a = s.vault("obop_agent", true);
        let t1 = a.with(|v| v.mask("PERSON", "party:1", "Ada")).unwrap();
        let t2 = a.with(|v| v.mask("PERSON", "party:2", "Alan")).unwrap();
        flaky.refuse_next(1);
        let refused = a.with(|v| v.mask("PERSON", "party:3", "Grace"));
        let Err(e) = refused else {
            panic!("the call must fail");
        };
        assert!(obop_storage::fault::Fault::is_transient(&e), "{e}");
        // The stored vault is as it was: numbering goes on, and the old tokens still resolve.
        let t3 = a.with(|v| v.mask("PERSON", "party:3", "Grace")).unwrap();
        assert!(t3 != t1 && t3 != t2, "{t1} {t2} {t3}");
        assert_eq!(
            a.with(|v| v.restore(&t1, |_| true).map(String::from))
                .unwrap(),
            Ok("Ada".to_string())
        );
        assert_eq!(
            a.with(|v| v.restore(&t2, |_| true).map(String::from))
                .unwrap(),
            Ok("Alan".to_string())
        );
    }

    /// A call that changed a record and masked a value, with the save of the vault refused `n`
    /// times right after the change. Returns the call's result and what the request did.
    fn masked_call_after_a_commit(
        n: i64,
    ) -> (
        Arc<SharedState>,
        Result<String, String>,
        obop_storage::fault::Outcome,
    ) {
        let (flaky, backend) = obop_storage::testing::Flaky::memory();
        let s = Arc::new(SharedState {
            backend: backend.clone(),
            ring: KeyRing::for_tests(),
        });
        let a = s.vault("obop_agent", true);
        obop_storage::fault::begin_request(Default::default());
        flaky.refuse_n_after_the_next_write(n);
        let r = a.with(|v| {
            backend
                .insert("note", "n1", &obop_storage::Fields::new())
                .unwrap();
            v.mask("PERSON", "party:1", "Ada")
        });
        (s, r, obop_storage::fault::end_request())
    }

    #[test]
    fn a_vault_save_refused_for_a_moment_is_tried_again() {
        // The call committed a change and its answer names a token only the saved vault restores.
        let (s, r, out) = masked_call_after_a_commit(2);
        let token = r.expect("the save went through on the third try");
        assert!(out.wrote && !out.partial, "{out:?}");
        let again = s.vault("obop_agent", true);
        assert_eq!(
            again
                .with(|v| v.restore(&token, |_| true).map(String::from))
                .unwrap(),
            Ok("Ada".to_string())
        );
    }

    #[test]
    fn a_vault_save_that_stays_refused_after_a_commit_is_a_partial_failure() {
        let (_s, r, out) = masked_call_after_a_commit(50);
        let e = r.expect_err("the answer would name a token nobody stored");
        assert!(obop_storage::fault::Fault::is_transient(&e), "{e}");
        assert!(out.wrote && out.partial, "{out:?}");
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
