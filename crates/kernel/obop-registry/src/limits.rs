//! Length and range limits of fields (D90): `max_length` and `min_length` on `text` and
//! `longtext`, `min` and `max` on `number`, `money`, `decimal` and `date`, and a named `pattern`
//! on `text` (a currency code, an IBAN, a VAT number; K45).
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
/// scale of the field) for `decimal`, a calendar day for `date`. Never a float.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bound {
    Int(i64),
    Dec(Decimal),
    /// A day as `yyyymmdd` (2026-03-01 is 20260301), so days compare as numbers.
    Date(u32),
}

impl Bound {
    /// The bound of a `YYYY-MM-DD` text (a real calendar day from 0001-01-01 to 9999-12-31).
    pub fn date(s: &str) -> Option<Self> {
        day_number(s).map(Self::Date)
    }
}

/// `YYYY-MM-DD` as `yyyymmdd`, when it is a real calendar day.
pub fn day_number(s: &str) -> Option<u32> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let num = |r: std::ops::Range<usize>| -> Option<u32> {
        let t = &s[r];
        if t.bytes().all(|c| c.is_ascii_digit()) {
            t.parse().ok()
        } else {
            None
        }
    };
    let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    (y >= 1 && (1..=days).contains(&d)).then_some(y * 10_000 + m * 100 + d)
}

impl fmt::Display for Bound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(n) => write!(f, "{n}"),
            Self::Dec(d) => write!(f, "{d}"),
            Self::Date(n) => write!(f, "{:04}-{:02}-{:02}", n / 10_000, n / 100 % 100, n % 100),
        }
    }
}

/// A named format a `text` value must have (`pattern` in a manifest, K45). Named rather than a
/// regular expression: each is checked exactly (an IBAN by its check digits), and none can be
/// written so that matching it takes long.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pattern {
    /// A code of the ISO 4217 list (`ISO_4217`), such as EUR: the rule of the instance currency.
    CurrencyCode,
    /// An IBAN: country, check digits and account, the length of its country and the mod-97 check
    /// (ISO 13616). Spaces between the groups are allowed, letters of either case.
    Iban,
    /// A VAT identification number with its two-letter country prefix, in the shape of its EU
    /// member state (VIES); another prefix takes 2 to 13 letters and digits. Spaces are allowed.
    /// The number is not looked up.
    VatId,
}

impl Pattern {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "currency_code" => Some(Self::CurrencyCode),
            "iban" => Some(Self::Iban),
            "vat_id" => Some(Self::VatId),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::CurrencyCode => "currency_code",
            Self::Iban => "iban",
            Self::VatId => "vat_id",
        }
    }

    /// What a value must be, for a refusal and a description.
    pub fn requirement(self) -> &'static str {
        match self {
            Self::CurrencyCode => "a currency code of ISO 4217, such as EUR",
            Self::Iban => "a valid IBAN, such as DE89 3704 0044 0532 0130 00",
            Self::VatId => "a VAT number with its country prefix, such as DE123456789",
        }
    }

    /// Whether `s` has this format. The empty text is left to `required`.
    pub fn matches(self, s: &str) -> bool {
        if s.is_empty() {
            return true;
        }
        match self {
            Self::CurrencyCode => is_currency_code(s),
            Self::Iban => iban_ok(s),
            Self::VatId => vat_ok(s),
        }
    }
}

/// The currency codes of ISO 4217 (list one, as published by SIX), sorted: the current ones, the
/// fund and precious-metal codes, and the few withdrawn since 2020 that stored records may still
/// hold (ANG, CUC, HRK, SLL, ZWL). Left out: XTS (for testing) and XXX (no currency). U5, #103.
#[rustfmt::skip]
pub const ISO_4217: &[&str] = &[
    "AED", "AFN", "ALL", "AMD", "ANG", "AOA", "ARS", "AUD", "AWG", "AZN", "BAM", "BBD", "BDT",
    "BGN", "BHD", "BIF", "BMD", "BND", "BOB", "BOV", "BRL", "BSD", "BTN", "BWP", "BYN", "BZD",
    "CAD", "CDF", "CHE", "CHF", "CHW", "CLF", "CLP", "CNY", "COP", "COU", "CRC", "CUC", "CUP",
    "CVE", "CZK", "DJF", "DKK", "DOP", "DZD", "EGP", "ERN", "ETB", "EUR", "FJD", "FKP", "GBP",
    "GEL", "GHS", "GIP", "GMD", "GNF", "GTQ", "GYD", "HKD", "HNL", "HRK", "HTG", "HUF", "IDR",
    "ILS", "INR", "IQD", "IRR", "ISK", "JMD", "JOD", "JPY", "KES", "KGS", "KHR", "KMF", "KPW",
    "KRW", "KWD", "KYD", "KZT", "LAK", "LBP", "LKR", "LRD", "LSL", "LYD", "MAD", "MDL", "MGA",
    "MKD", "MMK", "MNT", "MOP", "MRU", "MUR", "MVR", "MWK", "MXN", "MXV", "MYR", "MZN", "NAD",
    "NGN", "NIO", "NOK", "NPR", "NZD", "OMR", "PAB", "PEN", "PGK", "PHP", "PKR", "PLN", "PYG",
    "QAR", "RON", "RSD", "RUB", "RWF", "SAR", "SBD", "SCR", "SDG", "SEK", "SGD", "SHP", "SLE",
    "SLL", "SOS", "SRD", "SSP", "STN", "SVC", "SYP", "SZL", "THB", "TJS", "TMT", "TND", "TOP",
    "TRY", "TTD", "TWD", "TZS", "UAH", "UGX", "USD", "USN", "UYI", "UYU", "UYW", "UZS", "VED",
    "VES", "VND", "VUV", "WST", "XAF", "XAG", "XAU", "XBA", "XBB", "XBC", "XBD", "XCD", "XCG",
    "XDR", "XOF", "XPD", "XPF", "XPT", "XSU", "XUA", "YER", "ZAR", "ZMW", "ZWG", "ZWL",
];

/// Whether `s` is a code of `ISO_4217` (three capital letters on the list), for a field that holds
/// a currency and for the instance currency alike.
pub fn is_currency_code(s: &str) -> bool {
    ISO_4217.binary_search(&s).is_ok()
}

/// The length of the IBAN of each country of the IBAN registry (SWIFT).
#[rustfmt::skip]
pub const IBAN_LENGTHS: &[(&str, usize)] = &[
    ("AD", 24), ("AE", 23), ("AL", 28), ("AT", 20), ("AZ", 28), ("BA", 20), ("BE", 16),
    ("BG", 22), ("BH", 22), ("BI", 27), ("BR", 29), ("BY", 28), ("CH", 21), ("CR", 22),
    ("CY", 28), ("CZ", 24), ("DE", 22), ("DJ", 27), ("DK", 18), ("DO", 28), ("EE", 20),
    ("EG", 29), ("ES", 24), ("FI", 18), ("FK", 18), ("FO", 18), ("FR", 27), ("GB", 22),
    ("GE", 22), ("GI", 23), ("GL", 18), ("GR", 27), ("GT", 28), ("HR", 21), ("HU", 28),
    ("IE", 22), ("IL", 23), ("IQ", 23), ("IS", 26), ("IT", 27), ("JO", 30), ("KW", 30),
    ("KZ", 20), ("LB", 28), ("LC", 32), ("LI", 21), ("LT", 20), ("LU", 20), ("LV", 21),
    ("LY", 25), ("MC", 27), ("MD", 24), ("ME", 22), ("MK", 19), ("MN", 20), ("MR", 27),
    ("MT", 31), ("MU", 30), ("NI", 28), ("NL", 18), ("NO", 15), ("OM", 23), ("PK", 24),
    ("PL", 28), ("PS", 29), ("PT", 25), ("QA", 29), ("RO", 24), ("RS", 22), ("RU", 33),
    ("SA", 24), ("SC", 31), ("SD", 18), ("SE", 24), ("SI", 19), ("SK", 24), ("SM", 27),
    ("SO", 23), ("ST", 25), ("SV", 28), ("TL", 23), ("TN", 24), ("TR", 26), ("UA", 29),
    ("VA", 22), ("VG", 24), ("XK", 20), ("YE", 30),
];

fn iban_ok(s: &str) -> bool {
    let c: Vec<u8> = s
        .bytes()
        .filter(|b| *b != b' ')
        .map(|b| b.to_ascii_uppercase())
        .collect();
    if !(15..=34).contains(&c.len())
        || !c[..2].iter().all(u8::is_ascii_uppercase)
        || !c[2..4].iter().all(u8::is_ascii_digit)
        || !c.iter().all(u8::is_ascii_alphanumeric)
    {
        return false;
    }
    let country = std::str::from_utf8(&c[..2]).unwrap_or("");
    if let Some((_, n)) = IBAN_LENGTHS.iter().find(|(k, _)| *k == country)
        && c.len() != *n
    {
        return false;
    }
    // ISO 7064 mod 97-10 over the account, then the country and check digits, letters as 10..35.
    let mut rem = 0u32;
    for b in c[4..].iter().chain(&c[..4]) {
        rem = if b.is_ascii_digit() {
            (rem * 10 + u32::from(b - b'0')) % 97
        } else {
            (rem * 100 + u32::from(b - b'A') + 10) % 97
        };
    }
    rem == 1
}

/// The shape of the VAT number of each EU member state (and XI, Northern Ireland) after its
/// prefix: `9` a digit, `A` a letter, `X` a letter or digit, anything else itself; alternatives
/// are separated by `|`.
#[rustfmt::skip]
pub const VAT_SHAPES: &[(&str, &str)] = &[
    ("AT", "U99999999"), ("BE", "9999999999"), ("BG", "999999999|9999999999"),
    ("CY", "99999999A"), ("CZ", "99999999|999999999|9999999999"), ("DE", "999999999"),
    ("DK", "99999999"), ("EE", "999999999"), ("EL", "999999999"), ("ES", "X9999999X"),
    ("FI", "99999999"), ("FR", "XX999999999"), ("HR", "99999999999"), ("HU", "99999999"),
    ("IE", "9X99999A|9X99999AA"), ("IT", "99999999999"), ("LT", "999999999|999999999999"),
    ("LU", "99999999"), ("LV", "99999999999"), ("MT", "99999999"), ("NL", "999999999B99"),
    ("PL", "9999999999"), ("PT", "999999999"),
    ("RO", "99|999|9999|99999|999999|9999999|99999999|999999999|9999999999"),
    ("SE", "999999999999"), ("SI", "99999999"), ("SK", "9999999999"),
    ("XI", "999999999|999999999999|GD999|HA999"),
];

fn vat_ok(s: &str) -> bool {
    let c: Vec<u8> = s.bytes().filter(|b| *b != b' ').collect();
    if c.len() < 4 || !c[..2].iter().all(u8::is_ascii_uppercase) {
        return false;
    }
    let (prefix, rest) = c.split_at(2);
    let prefix = std::str::from_utf8(prefix).unwrap_or("");
    let Some((_, shapes)) = VAT_SHAPES.iter().find(|(k, _)| *k == prefix) else {
        return (2..=13).contains(&rest.len())
            && rest
                .iter()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
    };
    shapes.split('|').any(|shape| {
        shape.len() == rest.len()
            && shape.bytes().zip(rest).all(|(p, b)| match p {
                b'9' => b.is_ascii_digit(),
                b'A' => b.is_ascii_uppercase(),
                b'X' => b.is_ascii_uppercase() || b.is_ascii_digit(),
                literal => literal == *b,
            })
    })
}

/// The limits a field declares (D90). Every part is optional; a field without any has none
/// (apart from the default length of a text field).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Limits {
    /// `text` and `longtext`: the fewest characters a written value has.
    pub min_length: Option<u32>,
    /// `text` and `longtext`: the most characters a written value has.
    pub max_length: Option<u32>,
    /// `number`, `money`, `decimal` and `date`: the smallest value, inclusive.
    pub min: Option<Bound>,
    /// `number`, `money`, `decimal` and `date`: the largest value, inclusive.
    pub max: Option<Bound>,
    /// `text`: the format a written value must have.
    pub pattern: Option<Pattern>,
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
    /// Not in the format of the field's `pattern`.
    Format(Pattern),
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
            Self::Format(p) => p.requirement().to_string(),
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
            // The value is not repeated: the person sees what they typed.
            Self::Format(_) => req,
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
        self.check_length(s)?;
        self.check_pattern(s)
    }

    /// Whether `s` has the format of this field's `pattern` (a `text` field), if it declares one.
    pub fn check_pattern(&self, s: &str) -> Result<(), Breach> {
        match self.effective_pattern() {
            Some(p) if !p.matches(s) => Err(Breach::Format(p)),
            _ => Ok(()),
        }
    }

    /// The format a written value of this field must have: the declared `pattern`, else, for a
    /// `text` field named `currency`, a currency code (#103). Every module names its currency field
    /// `currency` and the back office already shows a currency picker for exactly that field, so
    /// the server holds the same convention for packages and built-in modules alike.
    pub fn effective_pattern(&self) -> Option<Pattern> {
        if self.kind != FieldKind::Text || self.computed.is_some() || self.sequence.is_some() {
            return None;
        }
        self.declared_limits()
            .pattern
            .or((self.name == "currency").then_some(Pattern::CurrencyCode))
    }

    /// Whether the day `s` (`YYYY-MM-DD`) is within `min` and `max` of a `date` field.
    pub fn check_date(&self, s: &str) -> Result<(), Breach> {
        let l = self.declared_limits();
        let Some(day) = day_number(s) else {
            return Ok(());
        };
        if let Some(Bound::Date(m)) = l.min
            && day < m
        {
            return Err(Breach::Below(Bound::Date(m)));
        }
        if let Some(Bound::Date(m)) = l.max
            && day > m
        {
            return Err(Breach::Above(Bound::Date(m)));
        }
        Ok(())
    }

    /// The length part of [`FieldDef::check_text`].
    fn check_length(&self, s: &str) -> Result<(), Breach> {
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
        let note = limits_note(l.min_length, l.max_length, l.min, l.max);
        // The declared pattern only: the implied one of a `currency` field (effective_pattern)
        // adds nothing here, so the descriptions of the generated documents stay as they were.
        match l.pattern {
            Some(p) if note.is_empty() => format!(" ({})", p.requirement()),
            Some(p) => format!("{note} ({})", p.requirement()),
            None => note,
        }
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
        FieldKind::Number | FieldKind::Money | FieldKind::Decimal { .. } | FieldKind::Date
    );
    if f.computed.is_some() || f.sequence.is_some() {
        out.push(
            "a computed or numbered field is never written, so it takes no limits (max_length, min_length, min, max, pattern)"
                .into(),
        );
        return out;
    }
    if l.pattern.is_some() && f.kind != FieldKind::Text {
        out.push(format!(
            "`pattern` is only valid for kind \"text\", not {:?}",
            f.kind.name()
        ));
    }
    if !is_text && (l.max_length.is_some() || l.min_length.is_some()) {
        out.push(format!(
            "`max_length` and `min_length` are only valid for kind \"text\" or \"longtext\", not {:?}",
            f.kind.name()
        ));
    }
    if !is_num && (l.min.is_some() || l.max.is_some()) {
        out.push(format!(
            "`min` and `max` are only valid for kind \"number\", \"money\", \"decimal\" or \"date\", not {:?}",
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
        let is_date = f.kind == FieldKind::Date;
        let mut bound = |key: &str, b: Option<Bound>| match (b, shape) {
            (None, _) => {}
            (Some(Bound::Date(_)), _) if is_date => {}
            (Some(_), _) if is_date => out.push(format!(
                "`{key}` of a date field must be a day written as \"YYYY-MM-DD\""
            )),
            (Some(Bound::Date(_)), _) => out.push(format!(
                "`{key}` must be a number for kind {:?}, not a day",
                f.kind.name()
            )),
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
                (Bound::Date(a), Bound::Date(b)) => a > b,
                _ => false,
            };
            if above {
                out.push(format!("`min` {lo} is above `max` {hi}, so no value fits"));
            }
        }
    }
    out
}
