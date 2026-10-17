//! Per-field treatment by data class: the decision table of the egress policy.
//!
//! Treatments (D40 modes, plus `pseudonymise`): `allow` (as is), `mask` (a reversible token kept in
//! the instance), `pseudonymise` (a keyed HMAC, stable per connection, irreversible without the
//! key), `redact` (the value is replaced by a fixed marker), `deny` (the field is left out).
//!
//! Defaults: `secret` never leaves; the three sensitive classes are denied unless the owner raises
//! the policy for that class and records an impact-assessment acknowledgement; personal data of
//! the other classes is pseudonymised; non-personal fields are allowed. A provider's maximum
//! classes and the allowlist of the connection narrow this further; nothing here widens them.

use obop_types::DataClass;
use serde_json::{Value as J, json};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Treatment {
    Allow,
    Mask,
    Pseudonymise,
    Redact,
    Deny,
}

impl Treatment {
    pub const ALL: [Treatment; 5] = [
        Self::Allow,
        Self::Mask,
        Self::Pseudonymise,
        Self::Redact,
        Self::Deny,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Mask => "mask",
            Self::Pseudonymise => "pseudonymise",
            Self::Redact => "redact",
            Self::Deny => "deny",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.as_str() == s)
    }

    /// How much of the value reaches the recipient: 0 nothing, 1 a fixed marker, 2 an opaque
    /// stand-in, 3 the value. A policy "raises" a field when it exposes more than the default.
    pub fn exposure(self) -> u8 {
        match self {
            Self::Deny => 0,
            Self::Redact => 1,
            Self::Mask | Self::Pseudonymise => 2,
            Self::Allow => 3,
        }
    }
}

/// The three classes of CONVENTIONS section 5 that need an impact-assessment acknowledgement.
pub fn is_high_risk(class: DataClass) -> bool {
    class.is_sensitive()
}

/// The default treatment of a field that nothing overrides.
pub fn default_treatment(class: DataClass, personal_data: bool) -> Treatment {
    match class {
        DataClass::Secret => Treatment::Deny,
        c if is_high_risk(c) => Treatment::Deny,
        _ if personal_data => Treatment::Pseudonymise,
        _ => Treatment::Allow,
    }
}

/// A recorded acknowledgement that a high-risk class may leave (who, when, why).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ack {
    pub class: DataClass,
    pub by: String,
    pub at: String,
    pub text: String,
}

/// Minimum length of the impact-assessment text. It is a guard against an empty box, not a
/// judgement about the quality of the assessment.
pub const MIN_ACK_CHARS: usize = 30;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Policy {
    /// Treatment per data class name.
    pub classes: BTreeMap<String, Treatment>,
    /// Treatment per `model.field`.
    pub fields: BTreeMap<String, Treatment>,
    pub acks: Vec<Ack>,
}

/// Why a field was treated as it was; shown in the privacy preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    Default,
    ClassOverride,
    FieldOverride,
    SecretNeverLeaves,
    ProviderMaximum,
    HighRiskNotAcknowledged,
}

impl Why {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::ClassOverride => "class override",
            Self::FieldOverride => "field override",
            Self::SecretNeverLeaves => "secret never leaves",
            Self::ProviderMaximum => "class not allowed for this provider",
            Self::HighRiskNotAcknowledged => "high-risk class without acknowledgement",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decision {
    pub treatment: Treatment,
    pub why: Why,
}

/// What the policy needs to know about a field.
#[derive(Debug, Clone, Copy)]
pub struct FieldInfo<'a> {
    pub model: &'a str,
    pub field: &'a str,
    pub class: DataClass,
    pub personal_data: bool,
}

impl Policy {
    /// Parse the JSON stored in `connection.policy`. Empty text is the default policy.
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.trim().is_empty() {
            return Ok(Self::default());
        }
        let v: J = serde_json::from_str(text).map_err(|_| "policy must be a JSON object")?;
        let J::Object(o) = v else {
            return Err("policy must be a JSON object".into());
        };
        let mut p = Self::default();
        for (k, v) in &o {
            match k.as_str() {
                "classes" | "fields" => {
                    let J::Object(m) = v else {
                        return Err(format!("policy.{k} must be an object"));
                    };
                    for (name, t) in m {
                        let t = t
                            .as_str()
                            .and_then(Treatment::parse)
                            .ok_or_else(|| format!("policy.{k}.{name}: unknown treatment"))?;
                        if k == "classes" {
                            let c = DataClass::parse(name)
                                .ok_or_else(|| format!("policy.classes.{name}: unknown class"))?;
                            if c == DataClass::Secret && t != Treatment::Deny {
                                return Err(
                                    "secret fields never leave; the class cannot be raised".into(),
                                );
                            }
                            p.classes.insert(name.clone(), t);
                        } else {
                            if !name.contains('.') {
                                return Err(format!("policy.fields.{name}: use model.field"));
                            }
                            p.fields.insert(name.clone(), t);
                        }
                    }
                }
                "acks" => {
                    let J::Array(a) = v else {
                        return Err("policy.acks must be a list".into());
                    };
                    for x in a {
                        let class = x["class"]
                            .as_str()
                            .and_then(DataClass::parse)
                            .filter(|c| is_high_risk(*c))
                            .ok_or("policy.acks: class must be confidential_hr, special_category or financial")?;
                        let text = x["text"].as_str().unwrap_or("").trim().to_string();
                        if text.chars().count() < MIN_ACK_CHARS {
                            return Err(format!(
                                "policy.acks: the impact-assessment text for {class} needs at least {MIN_ACK_CHARS} characters"
                            ));
                        }
                        p.acks.push(Ack {
                            class,
                            by: x["by"].as_str().unwrap_or("").into(),
                            at: x["at"].as_str().unwrap_or("").into(),
                            text,
                        });
                    }
                }
                other => return Err(format!("policy: unknown key {other:?}")),
            }
        }
        Ok(p)
    }

    pub fn to_json(&self) -> J {
        json!({
            "classes": self.classes.iter().map(|(k, v)| (k.clone(), J::from(v.as_str()))).collect::<serde_json::Map<_, _>>(),
            "fields": self.fields.iter().map(|(k, v)| (k.clone(), J::from(v.as_str()))).collect::<serde_json::Map<_, _>>(),
            "acks": self.acks.iter().map(|a| json!({"class": a.class.as_str(), "by": a.by, "at": a.at, "text": a.text})).collect::<Vec<_>>(),
        })
    }

    pub fn acknowledged(&self, class: DataClass) -> bool {
        self.acks
            .iter()
            .any(|a| a.class == class && !a.by.is_empty() && !a.at.is_empty())
    }

    /// The treatment of one field. `provider_allows` is whether the provider may receive the
    /// field's class at all.
    pub fn decide(&self, f: &FieldInfo, provider_allows: bool) -> Decision {
        let d = |treatment, why| Decision { treatment, why };
        if f.class == DataClass::Secret {
            return d(Treatment::Deny, Why::SecretNeverLeaves);
        }
        if !provider_allows {
            return d(Treatment::Deny, Why::ProviderMaximum);
        }
        let key = format!("{}.{}", f.model, f.field);
        let (t, why) = if let Some(t) = self.fields.get(&key) {
            (*t, Why::FieldOverride)
        } else if let Some(t) = self.classes.get(f.class.as_str()) {
            (*t, Why::ClassOverride)
        } else {
            (default_treatment(f.class, f.personal_data), Why::Default)
        };
        if is_high_risk(f.class) && t != Treatment::Deny && !self.acknowledged(f.class) {
            return d(Treatment::Deny, Why::HighRiskNotAcknowledged);
        }
        d(t, why)
    }

    /// Overrides that expose more than the defaults do. A connection whose policy has any is
    /// raised: an agent may only propose it. `class_of` gives the class and personal-data flag
    /// of a `model.field`.
    pub fn raises(&self, class_of: impl Fn(&str) -> Option<(DataClass, bool)>) -> Vec<String> {
        let mut out = Vec::new();
        for (name, t) in &self.classes {
            let Some(c) = DataClass::parse(name) else {
                continue;
            };
            // The strictest default of the class: personal data of that class.
            if t.exposure() > default_treatment(c, true).exposure() {
                out.push(format!("class {name} -> {}", t.as_str()));
            }
        }
        for (key, t) in &self.fields {
            let base = class_of(key)
                .map(|(c, p)| default_treatment(c, p).exposure())
                .unwrap_or(0);
            if t.exposure() > base {
                out.push(format!("field {key} -> {}", t.as_str()));
            }
        }
        for a in &self.acks {
            out.push(format!("acknowledgement for {}", a.class));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use DataClass::*;

    fn info(class: DataClass, personal: bool) -> FieldInfo<'static> {
        FieldInfo {
            model: "party",
            field: "x",
            class,
            personal_data: personal,
        }
    }

    fn acked(class: DataClass) -> Policy {
        Policy {
            acks: vec![Ack {
                class,
                by: "root".into(),
                at: "2026-10-17T00:00:00Z".into(),
                text: "x".repeat(40),
            }],
            ..Policy::default()
        }
    }

    #[test]
    fn default_table_for_every_class() {
        let p = Policy::default();
        let t = |c, personal| p.decide(&info(c, personal), true).treatment;
        assert_eq!(t(None, false), Treatment::Allow);
        assert_eq!(t(PublicDirectory, true), Treatment::Pseudonymise);
        assert_eq!(t(Internal, true), Treatment::Pseudonymise);
        assert_eq!(t(Internal, false), Treatment::Allow);
        assert_eq!(t(ConfidentialHr, true), Treatment::Deny);
        assert_eq!(t(SpecialCategory, true), Treatment::Deny);
        assert_eq!(t(Financial, true), Treatment::Deny);
        assert_eq!(t(Secret, false), Treatment::Deny);
    }

    #[test]
    fn every_class_times_every_treatment() {
        for class in [
            None,
            PublicDirectory,
            Internal,
            ConfidentialHr,
            SpecialCategory,
            Financial,
            Secret,
        ] {
            for t in Treatment::ALL {
                let mut p = if is_high_risk(class) {
                    acked(class)
                } else {
                    Policy::default()
                };
                p.fields.insert("party.x".into(), t);
                let got = p.decide(&info(class, true), true).treatment;
                let want = if class == Secret { Treatment::Deny } else { t };
                assert_eq!(got, want, "{class} {t:?}");
                // Without an acknowledgement a high-risk class stays denied, whatever is set.
                if is_high_risk(class) {
                    let mut bare = Policy::default();
                    bare.fields.insert("party.x".into(), t);
                    assert_eq!(
                        bare.decide(&info(class, true), true).treatment,
                        Treatment::Deny
                    );
                }
            }
        }
    }

    #[test]
    fn provider_maximum_wins_over_everything() {
        let mut p = acked(Financial);
        p.classes.insert("financial".into(), Treatment::Allow);
        assert_eq!(
            p.decide(&info(Financial, true), true).treatment,
            Treatment::Allow
        );
        let d = p.decide(&info(Financial, true), false);
        assert_eq!(
            (d.treatment, d.why),
            (Treatment::Deny, Why::ProviderMaximum)
        );
    }

    #[test]
    fn field_override_beats_class_override() {
        let mut p = Policy::default();
        p.classes.insert("internal".into(), Treatment::Allow);
        p.fields.insert("party.x".into(), Treatment::Redact);
        assert_eq!(
            p.decide(&info(Internal, true), true).treatment,
            Treatment::Redact
        );
        assert_eq!(
            p.decide(
                &FieldInfo {
                    field: "y",
                    ..info(Internal, true)
                },
                true
            )
            .treatment,
            Treatment::Allow
        );
    }

    #[test]
    fn secret_cannot_be_raised() {
        assert!(Policy::parse(r#"{"classes":{"secret":"allow"}}"#).is_err());
        assert!(Policy::parse(r#"{"classes":{"secret":"deny"}}"#).is_ok());
        let mut p = Policy::default();
        p.fields.insert("party.x".into(), Treatment::Allow);
        assert_eq!(
            p.decide(&info(Secret, false), true).treatment,
            Treatment::Deny
        );
    }

    #[test]
    fn parse_rejects_unknown_and_short_acks() {
        assert!(Policy::parse("").unwrap() == Policy::default());
        assert!(Policy::parse(r#"{"bogus":1}"#).is_err());
        assert!(Policy::parse(r#"{"fields":{"nodot":"allow"}}"#).is_err());
        assert!(Policy::parse(r#"{"fields":{"a.b":"leak"}}"#).is_err());
        assert!(Policy::parse(r#"{"acks":[{"class":"financial","text":"short"}]}"#).is_err());
        assert!(
            Policy::parse(
                r#"{"acks":[{"class":"internal","text":"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"}]}"#
            )
            .is_err()
        );
        let ok = Policy::parse(&format!(
            r#"{{"acks":[{{"class":"financial","by":"a","at":"b","text":"{}"}}]}}"#,
            "y".repeat(40)
        ))
        .unwrap();
        assert!(ok.acknowledged(Financial) && !ok.acknowledged(SpecialCategory));
        // An acknowledgement that nobody stamped does not count.
        let unstamped = Policy::parse(&format!(
            r#"{{"acks":[{{"class":"financial","text":"{}"}}]}}"#,
            "y".repeat(40)
        ))
        .unwrap();
        assert!(!unstamped.acknowledged(Financial));
    }

    #[test]
    fn raising_is_detected() {
        let class_of = |k: &str| (k == "party.email").then_some((PublicDirectory, true));
        assert!(Policy::default().raises(class_of).is_empty());
        let mut p = Policy::default();
        p.classes.insert("internal".into(), Treatment::Redact);
        p.fields.insert("party.email".into(), Treatment::Mask);
        assert!(
            p.raises(class_of).is_empty(),
            "tightening or equal is not a raise"
        );
        p.fields.insert("party.email".into(), Treatment::Allow);
        assert_eq!(
            p.raises(class_of),
            vec!["field party.email -> allow".to_string()]
        );
        let mut q = Policy::default();
        q.classes
            .insert("financial".into(), Treatment::Pseudonymise);
        assert_eq!(q.raises(class_of).len(), 1);
    }

    #[test]
    fn json_round_trip() {
        let mut p = acked(Financial);
        p.classes.insert("internal".into(), Treatment::Mask);
        p.fields.insert("a.b".into(), Treatment::Redact);
        assert_eq!(Policy::parse(&p.to_json().to_string()).unwrap(), p);
    }
}
