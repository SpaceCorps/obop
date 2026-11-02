//! Layer 0 kernel types. No I/O.

use std::fmt;

/// Product name, as reported by the binary.
pub const NAME: &str = "obop";

/// The one authoritative core version (D55): the workspace package version. Built-in modules
/// version with it, packages declare which core versions they run on, and `GET /api/meta`,
/// the OpenAPI document, the MCP server info and `obop --version` all report this string.
pub const CORE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Data classes of CONVENTIONS.md section 5, plus `None` for an explicit non-personal marker (D41).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DataClass {
    /// Explicitly not personal data.
    None,
    PublicDirectory,
    Internal,
    ConfidentialHr,
    SpecialCategory,
    Financial,
    Secret,
}

impl DataClass {
    /// The three sensitive classes (CONVENTIONS section 5).
    pub fn is_sensitive(self) -> bool {
        matches!(
            self,
            Self::ConfidentialHr | Self::SpecialCategory | Self::Financial
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::PublicDirectory => "public_directory",
            Self::Internal => "internal",
            Self::ConfidentialHr => "confidential_hr",
            Self::SpecialCategory => "special_category",
            Self::Financial => "financial",
            Self::Secret => "secret",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "none" => Self::None,
            "public_directory" => Self::PublicDirectory,
            "internal" => Self::Internal,
            "confidential_hr" => Self::ConfidentialHr,
            "special_category" => Self::SpecialCategory,
            "financial" => Self::Financial,
            "secret" => Self::Secret,
            _ => return Option::None,
        })
    }
}

impl fmt::Display for DataClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The name of the setting that lets the outbound client reach a literal loopback address over
/// plain http (tests and local development), and that the test authority of the Microsoft sign-in
/// (D91) needs as well.
pub const ALLOW_LOOPBACK_ENV: &str = "OBOP_CONNECTORS_ALLOW_LOOPBACK";

/// Whether the value of [`ALLOW_LOOPBACK_ENV`] turns loopback on: exactly `1` or `true`, nothing
/// else (not `TRUE`, `yes`, `on`, `0`, an empty value or one with a space). The one place that
/// reads the spelling, so that the outbound client and the sign-in cannot disagree about it.
pub fn loopback_allowed(value: Option<&str>) -> bool {
    matches!(value, Some("1" | "true"))
}

/// How a read surface presents a value to a principal (D40).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskMode {
    Allow,
    /// Reversible token.
    Mask,
    /// Irreversible.
    Redact,
    Deny,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_round_trips() {
        for c in [
            DataClass::None,
            DataClass::PublicDirectory,
            DataClass::Internal,
            DataClass::ConfidentialHr,
            DataClass::SpecialCategory,
            DataClass::Financial,
            DataClass::Secret,
        ] {
            assert_eq!(DataClass::parse(c.as_str()), Some(c));
        }
        assert_eq!(DataClass::parse("nope"), None);
    }

    #[test]
    fn loopback_is_on_for_one_and_true_only() {
        assert!(loopback_allowed(Some("1")));
        assert!(loopback_allowed(Some("true")));
        assert!(!loopback_allowed(None));
        for v in [
            "", "0", "yes", "on", "TRUE", "True", " 1", "1 ", "false", "2",
        ] {
            assert!(!loopback_allowed(Some(v)), "{v:?}");
        }
        assert_eq!(ALLOW_LOOPBACK_ENV, "OBOP_CONNECTORS_ALLOW_LOOPBACK");
    }

    #[test]
    fn sensitive_classes() {
        assert!(DataClass::Financial.is_sensitive());
        assert!(!DataClass::Internal.is_sensitive());
        assert!(!DataClass::Secret.is_sensitive());
    }
}
