//! D40 mask-and-restore. Policy resolution and a token vault; storage is in-memory here and
//! moves to the business database when `obop-orm` exists.

use obop_types::{DataClass, MaskMode};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrincipalKind {
    User,
    Agent,
}

/// Resolve the mode for a field (D40 defaults). Users see data under normal permissions;
/// agents get `deny` for sensitive and secret classes and `mask` for other personal data.
pub fn mode_for(kind: PrincipalKind, class: DataClass, personal_data: bool) -> MaskMode {
    match kind {
        PrincipalKind::User => {
            if class == DataClass::Secret {
                MaskMode::Deny
            } else {
                MaskMode::Allow
            }
        }
        PrincipalKind::Agent => {
            if class.is_sensitive() || class == DataClass::Secret {
                MaskMode::Deny
            } else if personal_data {
                MaskMode::Mask
            } else {
                MaskMode::Allow
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum RestoreError {
    UnknownToken(String),
    NotPermitted(String),
}

struct Entry {
    value: String,
    record: String,
}

/// Reversible tokens, stable inside one vault (one business and principal session).
#[derive(Default)]
pub struct Vault {
    by_token: HashMap<String, Entry>,
    by_value: HashMap<(String, String), String>,
    next: u64,
}

impl Vault {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace `value` with a stable token such as `PERSON_1`. `record` names the underlying
    /// record so a later restore can be permission-checked.
    pub fn mask(&mut self, label: &str, record: &str, value: &str) -> String {
        let key = (label.to_string(), value.to_string());
        if let Some(t) = self.by_value.get(&key) {
            return t.clone();
        }
        self.next += 1;
        let token = format!("{label}_{}", self.next);
        self.by_value.insert(key, token.clone());
        self.by_token.insert(
            token.clone(),
            Entry {
                value: value.into(),
                record: record.into(),
            },
        );
        token
    }

    /// Resolve a token back to its value, only if `can_read(record)` allows it (D10).
    pub fn restore(
        &self,
        token: &str,
        can_read: impl Fn(&str) -> bool,
    ) -> Result<&str, RestoreError> {
        let e = self
            .by_token
            .get(token)
            .ok_or_else(|| RestoreError::UnknownToken(token.into()))?;
        if can_read(&e.record) {
            Ok(&e.value)
        } else {
            Err(RestoreError::NotPermitted(token.into()))
        }
    }

    /// Erase every token of a record (data-subject erasure).
    pub fn erase_record(&mut self, record: &str) {
        self.by_token.retain(|_, e| e.record != record);
        let live: std::collections::HashSet<_> = self.by_token.keys().cloned().collect();
        self.by_value.retain(|_, t| live.contains(t));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_defaults() {
        use DataClass::*;
        assert_eq!(
            mode_for(PrincipalKind::Agent, Financial, true),
            MaskMode::Deny
        );
        assert_eq!(
            mode_for(PrincipalKind::Agent, Secret, false),
            MaskMode::Deny
        );
        assert_eq!(
            mode_for(PrincipalKind::Agent, Internal, true),
            MaskMode::Mask
        );
        assert_eq!(mode_for(PrincipalKind::Agent, None, false), MaskMode::Allow);
        assert_eq!(
            mode_for(PrincipalKind::User, Financial, true),
            MaskMode::Allow
        );
    }

    #[test]
    fn tokens_are_stable_and_restorable() {
        let mut v = Vault::new();
        let a = v.mask("PERSON", "party:1", "Ada Lovelace");
        assert_eq!(a, v.mask("PERSON", "party:1", "Ada Lovelace"));
        assert_ne!(a, v.mask("PERSON", "party:2", "Alan Turing"));
        assert_eq!(v.restore(&a, |_| true), Ok("Ada Lovelace"));
    }

    #[test]
    fn restore_respects_permission() {
        let mut v = Vault::new();
        let t = v.mask("PERSON", "party:1", "Ada");
        assert_eq!(
            v.restore(&t, |_| false),
            Err(RestoreError::NotPermitted(t.clone()))
        );
        assert_eq!(
            v.restore("PERSON_99", |_| true),
            Err(RestoreError::UnknownToken("PERSON_99".into()))
        );
    }

    #[test]
    fn erasure_removes_tokens() {
        let mut v = Vault::new();
        let t = v.mask("PERSON", "party:1", "Ada");
        v.erase_record("party:1");
        assert!(matches!(
            v.restore(&t, |_| true),
            Err(RestoreError::UnknownToken(_))
        ));
        assert_ne!(v.mask("PERSON", "party:1", "Ada"), t);
    }
}
