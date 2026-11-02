//! Length and range limits of fields (D90): `max_length` and `min_length` on `text` and
//! `longtext`, `min` and `max` on `number`, `money` and `decimal`.
//!
//! The limits are metadata of the field, so every surface (REST, MCP, CLI, the back office, the
//! OpenAPI document, `GET /api/meta`) reads them from here and the command layer checks them on
//! every write that checks field kinds. They are checked on the value that is written; a value
//! that is already stored is never refused for being outside a limit that was set later (D90
//! Decision 6).
//!
//! # Lengths count Unicode scalar values
//!
//! A length is the number of code points (`str::chars`), not bytes: `å`, `ä`, `ö` and an emoji
//! such as U+1F600 count 1 each. A letter written with a combining mark and an emoji built from
//! several code points (a flag, a family) count as many as they have; the limit is not about what
//! a person sees as one character. Nothing is trimmed or normalised before counting.
//!
//! # The default and the cap
//!
//! A `text` or `longtext` field that declares no `max_length` has the default of its kind
//! ([`DEFAULT_TEXT_MAX`], [`DEFAULT_LONGTEXT_MAX`]); a field may declare its own up to the hard
//! cap of its kind ([`TEXT_MAX_CAP`], [`LONGTEXT_MAX_CAP`]). All four are named here and nowhere
//! else.

use crate::{FieldDef, FieldKind};
use obop_values::Decimal;
use std::cmp::Ordering;
use std::fmt;
use std::sync::Arc;

/// The longest value, in characters, a `text` field accepts when it declares no `max_length`.
///
/// A round number with a margin over the longest text value measured in the bundled seed data
/// (140 characters); names, titles, addresses and codes are far shorter. 2,048 is also the
/// conservative ceiling for a URL, the longest ordinary single-line value: sitemaps.org asks for
/// fewer than 2,048 characters and old Internet Explorer stopped near it (modern browsers accept
/// far more), and it is this project's own limit for an OAuth redirect URI (`obop-oauth`,
/// `redirect.rs`). A field that holds a list in one line (a comma-separated list of people)
/// declares its own `max_length`.
pub const DEFAULT_TEXT_MAX: u32 = 2_048;

/// The longest value, in characters, a `longtext` field accepts when it declares no
/// `max_length`.
///
/// A round number with a margin over the longest longtext value measured in the bundled seed
/// data (2,040 characters), about fifty printed pages. Unofficial third-party analogies, for
/// scale only: GitHub refuses an issue body above 65,536 characters (its error message; it is
/// not documented), and mail clients are reported to clip HTML mail near 102,400 bytes.
pub const DEFAULT_LONGTEXT_MAX: u32 = 100_000;

/// The largest `max_length` a `text` field may declare.
pub const TEXT_MAX_CAP: u32 = 10_000;

/// The largest `max_length` a `longtext` field may declare. The request body limit of the HTTP
/// server (2 MiB) still applies first, so text of three or four bytes a character reaches the
/// body limit before it reaches this number.
pub const LONGTEXT_MAX_CAP: u32 = 1_000_000;

/// A bound of `min` or `max`: a whole number for `number` and `money`, an exact decimal (at the
/// scale of the field) for `decimal`. Never a float.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bound {
    Int(i64),
    Dec(Decimal),
}

impl fmt::Display for Bound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(n) => write!(f, "{n}"),
            Self::Dec(d) => write!(f, "{d}"),
        }
    }
}

/// The limits a field declares (D90). Every part is optional; a field without any has none
/// (apart from the default length of a text field).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Limits {
    /// `text` and `longtext`: the fewest characters a written value has.
    pub min_length: Option<u32>,
    /// `text` and `longtext`: the most characters a written value has.
    pub max_length: Option<u32>,
    /// `number`, `money` and `decimal`: the smallest value, inclusive.
    pub min: Option<Bound>,
    /// `number`, `money` and `decimal`: the largest value, inclusive.
    pub max: Option<Bound>,
}

impl Limits {
    /// Whether nothing is declared.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// How a written value breaks a limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Breach {
    /// Fewer characters than `min_length`; `have` is the number of characters written.
    TooShort { min: u32, have: usize },
    /// More characters than the effective `max_length`.
    TooLong { max: u32, have: usize },
    /// Below `min`.
    Below(Bound),
    /// Above `max`.
    Above(Bound),
}

impl Breach {
    /// What the value must be, with nothing about the value itself: `at most 2000 characters`,
    /// `at least 0`. This is what a refusal says when the caller may not be told how long or how
    /// large the value is (an agent holding a masked token; a value an action copied from a
    /// field the caller cannot read).
    pub fn requirement(&self) -> String {
        let chars = |n: u32| format!("{n} character{}", if n == 1 { "" } else { "s" });
        match self {
            Self::TooShort { min, .. } => format!("at least {}", chars(*min)),
            Self::TooLong { max, .. } => format!("at most {}", chars(*max)),
            Self::Below(b) => format!("at least {b}"),
            Self::Above(b) => format!("at most {b}"),
        }
    }

    /// What the value must be and what it was: `at most 2000 characters; this has 2001`. `value` is
    /// the written number, for the two bounds.
    pub fn detail(&self, value: &str) -> String {
        let req = self.requirement();
        match self {
            Self::TooShort { have, .. } | Self::TooLong { have, .. } => {
                format!("{req}; this has {have}")
            }
            Self::Below(_) | Self::Above(_) => format!("{req}; this is {value}"),
        }
    }

    /// `model.field must be ...`, naming the field and the limit and saying what was written.
    pub fn message(&self, model: &str, field: &str, value: &str) -> String {
        format!("{model}.{field} must be {}", self.detail(value))
    }

    /// As [`Breach::message`] without the length or the value that was written: `model.field must
    /// be at most 10 characters`.
    pub fn message_without_value(&self, model: &str, field: &str) -> String {
        format!("{model}.{field} must be {}", self.requirement())
    }
}

/// The number of characters (Unicode scalar values) of `s`.
pub fn char_count(s: &str) -> usize {
    s.chars().count()
}

impl FieldDef {
    /// This field with these limits (none when `limits` is empty).
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = (!limits.is_empty()).then(|| Arc::new(limits));
        self
    }

    /// The limits as declared, with no default filled in.
    pub fn declared_limits(&self) -> Limits {
        self.limits.as_deref().copied().unwrap_or_default()
    }

    /// The most characters a value of this field has: the declared `max_length`, else the default
    /// of the kind. `None` for every kind but `text` and `longtext`, and for a computed or numbered
    /// field, which is never written.
    pub fn effective_max_length(&self) -> Option<u32> {
        if self.computed.is_some() || self.sequence.is_some() {
            return None;
        }
        let default = match self.kind {
            FieldKind::Text => DEFAULT_TEXT_MAX,
            FieldKind::LongText => DEFAULT_LONGTEXT_MAX,
            _ => return None,
        };
        Some(self.declared_limits().max_length.unwrap_or(default))
    }

    /// The fewest characters a value of this field has, if it declares a `min_length`.
    pub fn effective_min_length(&self) -> Option<u32> {
        if self.computed.is_some() || self.sequence.is_some() {
            return None;
        }
        match self.kind {
            FieldKind::Text | FieldKind::LongText => self.declared_limits().min_length,
            _ => None,
        }
    }

    /// The declared `min` of a number, money or decimal field.
    pub fn lower_bound(&self) -> Option<Bound> {
        self.declared_limits().min
    }

    /// The declared `max` of a number, money or decimal field.
    pub fn upper_bound(&self) -> Option<Bound> {
        self.declared_limits().max
    }

    /// Whether `s` is a value this field accepts, as far as its length is concerned.
    pub fn check_text(&self, s: &str) -> Result<(), Breach> {
        let max = self.effective_max_length();
        let min = self.effective_min_length();
        if max.is_none() && min.is_none() {
            return Ok(());
        }
        // A string of `n` bytes has at most `n` and at least `n / 4` (rounded up) characters,
        // so most values are decided without counting them.
        let bytes = s.len();
        let sure_max = max.is_none_or(|m| bytes <= m as usize);
        let sure_min = min.is_none_or(|m| bytes.div_ceil(4) >= m as usize);
        if sure_max && sure_min {
            return Ok(());
        }
        let have = char_count(s);
        if let Some(m) = max
            && have > m as usize
        {
            return Err(Breach::TooLong { max: m, have });
        }
        if let Some(m) = min
            && have < m as usize
        {
            return Err(Breach::TooShort { min: m, have });
        }
        Ok(())
    }

    /// Whether `n` is a value this field accepts, as far as `min` and `max` are concerned (a
    /// `number` or `money` field).
    pub fn check_int(&self, n: i64) -> Result<(), Breach> {
        let l = self.declared_limits();
        if let Some(Bound::Int(m)) = l.min
            && n < m
        {
            return Err(Breach::Below(Bound::Int(m)));
        }
        if let Some(Bound::Int(m)) = l.max
            && n > m
        {
            return Err(Breach::Above(Bound::Int(m)));
        }
        Ok(())
    }

    /// As [`FieldDef::check_int`], for a `decimal` field. The comparison is exact (`Decimal`
    /// compares by value whatever the scales), never through a float.
    pub fn check_decimal(&self, d: Decimal) -> Result<(), Breach> {
        let l = self.declared_limits();
        if let Some(Bound::Dec(m)) = l.min
            && d.compare(m) == Ordering::Less
        {
            return Err(Breach::Below(Bound::Dec(m)));
        }
        if let Some(Bound::Dec(m)) = l.max
            && d.compare(m) == Ordering::Greater
        {
            return Err(Breach::Above(Bound::Dec(m)));
        }
        Ok(())
    }

    /// The limits this field declares, as a sentence for descriptions (OpenAPI, MCP): empty when it
    /// declares none, so the default of the kind (which the schema keywords and `/api/meta` state)
    /// adds no words to the description of every text field. `(at most 100 characters)`, `(from 1
    /// character to 2000 characters)`, `(from 0 to 100)`.
    pub fn limits_note(&self) -> String {
        let l = self.declared_limits();
        limits_note(l.min_length, l.max_length, l.min, l.max)
    }
}

/// The text behind [`FieldDef::limits_note`], for callers that hold the limits as plain values.
pub fn limits_note(
    min_length: Option<u32>,
    max_length: Option<u32>,
    min: Option<Bound>,
    max: Option<Bound>,
) -> String {
    let chars = |n: u32| format!("{n} character{}", if n == 1 { "" } else { "s" });
    match (min_length, max_length, min, max) {
        (None, None, None, None) => String::new(),
        (Some(lo), Some(hi), _, _) => format!(" (from {} to {})", chars(lo), chars(hi)),
        (Some(lo), None, _, _) => format!(" (at least {})", chars(lo)),
        (None, Some(hi), _, _) => format!(" (at most {})", chars(hi)),
        (_, _, Some(lo), Some(hi)) => format!(" (from {lo} to {hi})"),
        (_, _, Some(lo), None) => format!(" (at least {lo})"),
        (_, _, None, Some(hi)) => format!(" (at most {hi})"),
    }
}

/// What the install gate (D41) says about the limits of one field: one sentence per problem,
/// each naming the key. `kind` and the digits of a decimal come from the field itself.
pub fn problems(f: &FieldDef) -> Vec<String> {
    let l = f.declared_limits();
    let mut out = Vec::new();
    if l.is_empty() {
        return out;
    }
    let is_text = matches!(f.kind, FieldKind::Text | FieldKind::LongText);
    let is_num = matches!(
        f.kind,
        FieldKind::Number | FieldKind::Money | FieldKind::Decimal { .. }
    );
    if f.computed.is_some() || f.sequence.is_some() {
        out.push(
            "a computed or numbered field is never written, so it takes no limits (max_length, min_length, min, max)"
                .into(),
        );
        return out;
    }
    if !is_text && (l.max_length.is_some() || l.min_length.is_some()) {
        out.push(format!(
            "`max_length` and `min_length` are only valid for kind \"text\" or \"longtext\", not {:?}",
            f.kind.name()
        ));
    }
    if !is_num && (l.min.is_some() || l.max.is_some()) {
        out.push(format!(
            "`min` and `max` are only valid for kind \"number\", \"money\" or \"decimal\", not {:?}",
            f.kind.name()
        ));
    }
    if is_text {
        let cap = if f.kind == FieldKind::LongText {
            LONGTEXT_MAX_CAP
        } else {
            TEXT_MAX_CAP
        };
        let default = if f.kind == FieldKind::LongText {
            DEFAULT_LONGTEXT_MAX
        } else {
            DEFAULT_TEXT_MAX
        };
        if l.max_length == Some(0) {
            out.push("`max_length` must be at least 1".into());
        }
        if l.min_length == Some(0) {
            out.push("`min_length` must be at least 1 (a minimum of 0 limits nothing)".into());
        }
        if let Some(m) = l.max_length
            && m > cap
        {
            out.push(format!(
                "`max_length` {m} is above the largest a {} field may declare, {cap}",
                f.kind.name()
            ));
        }
        match (l.min_length, l.max_length) {
            (Some(lo), Some(hi)) if lo > hi => out.push(format!(
                "`min_length` {lo} is above `max_length` {hi}, so no value fits"
            )),
            // No `max_length`, so the default applies. Above the cap no `max_length` can help:
            // say so, and where more room is (a longtext field), rather than advise a number
            // the gate would refuse next.
            (Some(lo), None) if lo > cap => out.push(format!(
                "`min_length` {lo} is above {cap}, the most a {} field can hold{}",
                f.kind.name(),
                if f.kind == FieldKind::LongText {
                    String::new()
                } else {
                    format!("; use a longtext field for more (up to {LONGTEXT_MAX_CAP})")
                }
            )),
            (Some(lo), None) if lo > default => out.push(format!(
                "`min_length` {lo} is above the default maximum of a {} field ({default}); declare a `max_length` of at least {lo}, at most {cap}{}",
                f.kind.name(),
                if f.kind == FieldKind::LongText {
                    String::new()
                } else {
                    format!(" (use a longtext field for more than {cap})")
                }
            )),
            _ => {}
        }
    }
    if is_num {
        let shape = f.kind.decimal_shape();
        let mut bound = |key: &str, b: Option<Bound>| match (b, shape) {
            (None, _) => {}
            (Some(Bound::Int(_)), None) => {}
            (Some(Bound::Dec(_)), None) => out.push(format!(
                "`{key}` must be a whole number for kind {:?}, not a decimal",
                f.kind.name()
            )),
            (Some(Bound::Int(_)), Some(_)) => out.push(format!(
                "`{key}` of a decimal field must be a decimal at the scale of the field"
            )),
            (Some(Bound::Dec(d)), Some((precision, scale, _))) => {
                if d.scale() != scale || d.digits() > u32::from(precision) {
                    out.push(format!(
                        "`{key}` {d} does not fit the field: a decimal of {precision} digits, {scale} after the point"
                    ));
                }
            }
        };
        bound("min", l.min);
        bound("max", l.max);
        if let (Some(lo), Some(hi)) = (l.min, l.max) {
            let above = match (lo, hi) {
                (Bound::Int(a), Bound::Int(b)) => a > b,
                (Bound::Dec(a), Bound::Dec(b)) => a.compare(b) == Ordering::Greater,
                _ => false,
            };
            if above {
                out.push(format!("`min` {lo} is above `max` {hi}, so no value fits"));
            }
        }
    }
    out
}
