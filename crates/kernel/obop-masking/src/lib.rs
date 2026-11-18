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
    label: String,
    value: String,
    record: String,
    /// `value` is the JSON text of a value that is not text (a number, a bool, a list), so that
    /// the token restores to the same typed value (O39).
    json: bool,
}

/// What a token stands for: a text, or a value of another JSON type (O39).
#[derive(Debug, PartialEq)]
pub enum Restored<'a> {
    Text(&'a str),
    Json(serde_json::Value),
}

/// The key under which a value that is not text is remembered, apart from a text of the same
/// letters (`12` the number is not `"12"` the text).
fn json_key(text: &str) -> String {
    format!("\u{0}json:{text}")
}

/// Reversible tokens, stable inside one vault (one business and principal session).
#[derive(Default)]
pub struct Vault {
    by_token: HashMap<String, Entry>,
    by_value: HashMap<(String, String), String>,
    next: u64,
    /// Changed since it was loaded or last marked clean: a shared store saves only then.
    dirty: bool,
}

impl Vault {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace `value` with a stable token such as `PERSON_1`. `record` names the underlying
    /// record so a later restore can be permission-checked.
    pub fn mask(&mut self, label: &str, record: &str, value: &str) -> String {
        self.mask_entry(label, record, value.to_string(), false)
    }

    /// As `mask`, for a value that is not text: the token restores to the same JSON value (O39).
    /// A text passed here is masked as a text.
    pub fn mask_json(&mut self, label: &str, record: &str, value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::String(t) => self.mask(label, record, t),
            other => self.mask_entry(label, record, other.to_string(), true),
        }
    }

    fn mask_entry(&mut self, label: &str, record: &str, value: String, json: bool) -> String {
        let key = (
            label.to_string(),
            if json {
                json_key(&value)
            } else {
                value.clone()
            },
        );
        if let Some(t) = self.by_value.get(&key) {
            return t.clone();
        }
        self.next += 1;
        self.dirty = true;
        let token = format!("{label}_{}", self.next);
        self.by_value.insert(key, token.clone());
        self.by_token.insert(
            token.clone(),
            Entry {
                label: label.into(),
                value,
                record: record.into(),
                json,
            },
        );
        token
    }

    /// As `restore`, with the type of the value: a token of a value that is not text gives that
    /// value back as JSON (O39).
    pub fn restore_typed(
        &self,
        token: &str,
        can_read: impl Fn(&str) -> bool,
    ) -> Result<Restored<'_>, RestoreError> {
        let text = self.restore(token, can_read)?;
        let json = self.by_token.get(token).is_some_and(|e| e.json);
        Ok(match json {
            true => match serde_json::from_str(text) {
                Ok(v) => Restored::Json(v),
                Err(_) => Restored::Text(text),
            },
            false => Restored::Text(text),
        })
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

    /// Whether tokens were added or erased since the vault was loaded or marked clean.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// The vault as JSON, for a shared store (it holds personal data: seal it before it is kept).
    pub fn to_json(&self) -> serde_json::Value {
        let mut entries: Vec<_> = self.by_token.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        serde_json::json!({
            "next": self.next,
            "entries": entries.into_iter().map(|(t, e)| {
                let mut j = serde_json::json!({
                    "token": t, "label": e.label, "value": e.value, "record": e.record,
                });
                if e.json {
                    j["json"] = serde_json::Value::Bool(true);
                }
                j
            }).collect::<Vec<_>>(),
        })
    }

    /// A vault from `to_json` output. A malformed document yields an empty vault.
    pub fn from_json(j: &serde_json::Value) -> Vault {
        let mut v = Vault::new();
        v.next = j["next"].as_u64().unwrap_or(0);
        for e in j["entries"].as_array().into_iter().flatten() {
            let (Some(token), Some(label), Some(value), Some(record)) = (
                e["token"].as_str(),
                e["label"].as_str(),
                e["value"].as_str(),
                e["record"].as_str(),
            ) else {
                continue;
            };
            let json = e["json"].as_bool().unwrap_or(false);
            let key = if json {
                json_key(value)
            } else {
                value.to_string()
            };
            v.by_value
                .insert((label.to_string(), key), token.to_string());
            v.by_token.insert(
                token.to_string(),
                Entry {
                    label: label.into(),
                    value: value.into(),
                    record: record.into(),
                    json,
                },
            );
        }
        v
    }

    /// Erase every token of a record (data-subject erasure).
    pub fn erase_record(&mut self, record: &str) {
        self.dirty = true;
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
    fn a_vault_survives_a_round_trip_through_json_and_tracks_changes() {
        let mut v = Vault::new();
        assert!(!v.is_dirty());
        let a = v.mask("PERSON", "party:1", "Ada Lovelace");
        assert!(v.is_dirty());
        let mut w = Vault::from_json(&v.to_json());
        assert!(!w.is_dirty());
        assert_eq!(w.restore(&a, |_| true), Ok("Ada Lovelace"));
        assert_eq!(
            w.mask("PERSON", "party:1", "Ada Lovelace"),
            a,
            "stays stable"
        );
        assert!(!w.is_dirty(), "a known value changes nothing");
        let b = w.mask("PERSON", "party:2", "Alan Turing");
        assert_ne!(a, b, "the counter continues");
        assert!(w.is_dirty());
        assert_eq!(
            Vault::from_json(&serde_json::json!("junk")).to_json()["next"],
            0
        );
    }

    /// O39: a value that is not text gets a token that restores to the same typed value, apart
    /// from a text of the same letters, and survives the shared store.
    #[test]
    fn a_value_that_is_not_text_restores_to_the_same_type() {
        use serde_json::json;
        let mut v = Vault::new();
        let n = v.mask_json("AGE", "party:1", &json!(36));
        let s = v.mask("AGE", "party:1", "36");
        let b = v.mask_json("VIP", "party:1", &json!(true));
        assert_ne!(n, s, "the number and the text are two values");
        assert_eq!(v.mask_json("AGE", "party:1", &json!(36)), n, "stable");
        assert_eq!(v.restore_typed(&n, |_| true), Ok(Restored::Json(json!(36))));
        assert_eq!(v.restore_typed(&s, |_| true), Ok(Restored::Text("36")));
        assert_eq!(
            v.restore_typed(&b, |_| true),
            Ok(Restored::Json(json!(true)))
        );
        let w = Vault::from_json(&v.to_json());
        assert_eq!(w.restore_typed(&n, |_| true), Ok(Restored::Json(json!(36))));
        assert_eq!(w.restore_typed(&s, |_| true), Ok(Restored::Text("36")));
        assert_eq!(
            v.mask_json("NAME", "party:1", &json!("Ada")),
            v.mask("NAME", "party:1", "Ada"),
            "a text is masked as a text"
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
