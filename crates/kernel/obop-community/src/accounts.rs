//! Cookie sessions for the server-rendered pages, and CSRF tokens bound to them. Accounts
//! themselves are the control-plane accounts of `obop-auth`; this module never stores or sees a
//! password beyond handing it to `Auth`.

use crate::util::{ct_eq, random_token, sha256_hex};
use obop_storage::{Backend, Fields};
use serde_json::{Value as J, json};
use std::sync::Arc;

pub const SESSION_COOKIE: &str = "obop_session";
pub const CSRF_COOKIE: &str = "obop_csrf";
pub const SESSION_SECS: u64 = 30 * 24 * 3600;
const TABLE: &str = "community_session";

pub struct Sessions {
    backend: Arc<dyn Backend>,
}

impl Sessions {
    pub fn new(backend: Arc<dyn Backend>) -> Sessions {
        Sessions { backend }
    }

    /// Start a session for `email`. The raw token goes to the browser; only its SHA-256 is kept.
    pub fn create(&self, email: &str, now: u64) -> Result<String, String> {
        let raw = random_token();
        if raw.is_empty() {
            return Err("no randomness available".into());
        }
        let mut f = Fields::new();
        f.insert("email".into(), J::from(email));
        f.insert("created".into(), J::from(now));
        f.insert("expires".into(), J::from(now + SESSION_SECS));
        f.insert("revoked".into(), J::Bool(false));
        self.backend.insert(TABLE, &sha256_hex(&raw), &f)?;
        Ok(raw)
    }

    /// The account of a live session token, or `None` if unknown, expired or revoked.
    pub fn lookup(&self, raw: &str, now: u64) -> Option<String> {
        if raw.len() != 64 || !raw.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let f = self.backend.get(TABLE, &sha256_hex(raw)).ok()??;
        if f.get("revoked").and_then(J::as_bool) == Some(true)
            || f.get("expires")
                .and_then(J::as_u64)
                .is_none_or(|e| e <= now)
        {
            return None;
        }
        f.get("email").and_then(J::as_str).map(String::from)
    }

    pub fn revoke(&self, raw: &str) -> bool {
        let mut p = Fields::new();
        p.insert("revoked".into(), json!(true));
        self.backend
            .update(TABLE, &sha256_hex(raw), &p)
            .unwrap_or(false)
    }

    /// Delete sessions that expired or were revoked long ago.
    pub fn purge(&self, now: u64) {
        if let Ok(all) = self.backend.list(TABLE) {
            for (id, f) in all {
                let expired = f
                    .get("expires")
                    .and_then(J::as_u64)
                    .is_none_or(|e| e + 86_400 < now);
                if expired {
                    let _ = self.backend.delete(TABLE, &id);
                }
            }
        }
    }
}

/// CSRF token for a logged-in session: a hash of the session token that the browser cannot
/// read (HttpOnly) and an attacker's page cannot guess.
pub fn csrf_for_session(raw: &str) -> String {
    sha256_hex(&format!("obop-csrf-v1:{raw}"))
}

/// Compare a submitted token against the expected one in constant time. Empty never matches.
pub fn csrf_ok(expected: &str, submitted: &str) -> bool {
    !expected.is_empty() && ct_eq(expected, submitted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sessions() -> Sessions {
        Sessions::new(obop_storage::open("memory:").unwrap())
    }

    #[test]
    fn sessions_expire_and_can_be_revoked() {
        let s = sessions();
        let t = s.create("ada@example.test", 1000).unwrap();
        assert_eq!(t.len(), 64);
        assert_eq!(s.lookup(&t, 1001).as_deref(), Some("ada@example.test"));
        assert_eq!(
            s.lookup(&t, 1000 + SESSION_SECS - 1).as_deref(),
            Some("ada@example.test")
        );
        assert_eq!(
            s.lookup(&t, 1000 + SESSION_SECS),
            None,
            "30 days later it is gone"
        );
        assert!(s.revoke(&t));
        assert_eq!(s.lookup(&t, 1001), None, "revoked");
        assert!(!s.revoke("deadbeef"));
    }

    #[test]
    fn only_the_hash_is_stored_and_forgeries_fail() {
        let backend = obop_storage::open("memory:").unwrap();
        let s = Sessions::new(backend.clone());
        let t = s.create("ada@example.test", 1000).unwrap();
        assert!(backend.get(TABLE, &t).unwrap().is_none());
        assert!(backend.get(TABLE, &sha256_hex(&t)).unwrap().is_some());
        assert_eq!(s.lookup(&"0".repeat(64), 1001), None);
        assert_eq!(s.lookup("short", 1001), None);
        assert_eq!(s.lookup(&format!("{}%", &t[..63]), 1001), None);
    }

    #[test]
    fn purge_removes_old_sessions_only() {
        let s = sessions();
        let old = s.create("a@example.test", 0).unwrap();
        let new = s.create("b@example.test", 10 * SESSION_SECS).unwrap();
        s.purge(10 * SESSION_SECS);
        assert_eq!(s.lookup(&old, 5), None);
        assert!(s.lookup(&new, 10 * SESSION_SECS + 1).is_some());
        assert_eq!(s.backend.count(TABLE).unwrap(), 1);
    }

    #[test]
    fn csrf_tokens_are_bound_to_the_session() {
        let a = csrf_for_session("session-a");
        let b = csrf_for_session("session-b");
        assert_ne!(a, b);
        assert!(csrf_ok(&a, &a));
        assert!(!csrf_ok(&a, &b));
        assert!(!csrf_ok("", ""));
        assert!(!csrf_ok(&a, ""));
    }
}
