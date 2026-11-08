//! The read check: what a page shows of a stored row (docs/spec/branding.md, section 4). Each field
//! is checked on its own by the rule of its class, and a field that fails falls back to its default
//! while the rest of the brand stays:
//!
//! - the text fields (`name`, `title_<lang>`, `message_<lang>`) by the safety subset of the
//!   character policy (`obop_brand::policy::safe`), not the whole policy, so that a policy tightened
//!   in a later release never reverts a stored name;
//! - the `slug` by its fixed pattern;
//! - the artwork (`logo_svg`) by the strict SVG reader: the stored text is parsed, and what is
//!   served, inlined and hashed is the printer's output for that parse, never the stored text. A
//!   stored artwork that is valid but not in canonical form is served re-serialised and is no
//!   fallback; one that fails the parse is replaced by the generated tile.
//!
//! Everything here is total: whatever a row holds, the answer is a [`Shown`], never a panic.

use obop_brand::policy::{self, Kind, reason};
use obop_brand::{Brand, DEFAULT_NAME, LANGS, Logo, slug, tile};
use obop_storage::Fields;
use serde_json::{Value as J, json};

/// The reason word of a field whose stored value is not text at all.
pub(crate) const NOT_TEXT: &str = "not_text";

/// The largest instance setting text the theme is read from. The settings are refused above 64 KiB
/// when they are written; a bigger one was put there around the command and is not parsed on every
/// refill.
const MAX_SETTING_BYTES: usize = 256 * 1024;

/// A stored field that failed its read check and so fell back to its default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fallback {
    /// The name of the field (`name`, `title_sv`, `logo_svg`).
    pub field: String,
    /// A fixed reason word of the code (`unsafe_character`, `too_long`, `invalid_slug`, `empty`,
    /// `not_text`, or, for the artwork, the code of the first refusal of the SVG reader). Never a
    /// part of the stored value, so a line written from it cannot be shaped by the value.
    pub code: &'static str,
    /// The reason in words for the answer of `get`: the code, and for the artwork what the SVG
    /// reader said (its messages hold only what the reader made safe to repeat).
    pub reason: String,
}

impl Fallback {
    fn word(field: &str, code: &'static str) -> Fallback {
        Fallback {
            field: field.to_string(),
            code,
            reason: code.to_string(),
        }
    }

    fn artwork(refusals: &[obop_brand::Refusal]) -> Fallback {
        let code = refusals.first().map_or(reason::INVALID_ARTWORK, |r| r.code);
        let mut reason = match refusals.first() {
            Some(r) => format!("{}: {}", r.code, r.message),
            None => reason::INVALID_ARTWORK.to_string(),
        };
        if refusals.len() > 1 {
            reason.push_str(&format!(" (and {} more)", refusals.len() - 1));
        }
        Fallback {
            field: "logo_svg".to_string(),
            code,
            reason,
        }
    }
}

/// The `fallbacks` of an answer: a JSON list of `{"field": ..., "reason": ...}`.
pub fn fallbacks_json(list: &[Fallback]) -> String {
    J::Array(
        list.iter()
            .map(|f| json!({"field": f.field, "reason": f.reason}))
            .collect(),
    )
    .to_string()
}

/// A brand as a page shows it: the [`Brand`], the logo (when a brand is set), the accent the theme
/// stores and the fields that fell back. Immutable; the reader hands it out behind an `Arc`.
pub struct Shown {
    brand: Brand,
    logo: Option<Logo>,
    theme_accent: Option<String>,
    fallbacks: Vec<Fallback>,
    /// This is the default standing in for a brand that could not be read (see [`Shown::unread`]).
    unread: bool,
}

impl std::fmt::Debug for Shown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shown")
            .field("brand", &self.brand)
            .field("v", &self.v())
            .field("theme_accent", &self.theme_accent)
            .field("fallbacks", &self.fallbacks)
            .finish()
    }
}

impl Shown {
    /// No brand set: the default brand (`obop`, nothing else), no logo.
    pub fn not_set(theme_accent: Option<String>) -> Shown {
        Shown {
            brand: Brand::default(),
            logo: None,
            theme_accent,
            fallbacks: Vec::new(),
            unread: false,
        }
    }

    /// The default brand in the place of one that could not be read: what the reader hands out in a
    /// process that never had a good read while the store fails. It looks like [`Shown::not_set`]
    /// and is told apart from it by [`Shown::is_unread`], so that a cache above the reader does not
    /// keep it as if the store had said "no brand".
    pub(crate) fn unread() -> Shown {
        Shown {
            unread: true,
            ..Shown::not_set(None)
        }
    }

    /// Whether this is the stand-in of [`Shown::unread`]: the store could not be read and there was
    /// no earlier good read to serve. False for a brand that was read, for the answer "no brand is
    /// set" and for an inert reader.
    pub fn is_unread(&self) -> bool {
        self.unread
    }

    /// The brand: the default one when no row exists.
    pub fn brand(&self) -> &Brand {
        &self.brand
    }

    /// Whether a brand is set (a row exists): the one whole-brand test.
    pub fn is_set(&self) -> bool {
        self.brand.is_set()
    }

    /// The logo: the stored artwork re-serialised, or the generated tile. `None` when no brand is
    /// set (the logo route answers 404 then).
    pub fn logo(&self) -> Option<&Logo> {
        self.logo.as_ref()
    }

    /// The version of the logo, `v`: the first 16 hex digits of the SHA-256 of the bytes served.
    /// Empty when no brand is set.
    pub fn v(&self) -> &str {
        self.logo.as_ref().map_or("", Logo::v)
    }

    /// The accent the theme stores, when it is a valid `#rrggbb` (lower case); `None` when the
    /// theme stores none or something else.
    pub fn theme_accent(&self) -> Option<&str> {
        self.theme_accent.as_deref()
    }

    /// The stored fields that failed the read check, in the order of the fields.
    pub fn fallbacks(&self) -> &[Fallback] {
        &self.fallbacks
    }
}

/// The text a stored field holds: empty when absent or null, an error word when it is not text.
pub(crate) fn text_of<'a>(row: &'a Fields, key: &str) -> Result<&'a str, &'static str> {
    match row.get(key) {
        None | Some(J::Null) => Ok(""),
        Some(J::String(s)) => Ok(s),
        Some(_) => Err(NOT_TEXT),
    }
}

/// The accent the theme stores in the instance settings (`theme.accent`), when it is a valid
/// `#rrggbb`. A setting that does not parse, or does not hold one, gives none: a theme that cannot
/// be read is no reason for the brand to fail (a store that cannot be read is another matter and
/// is the caller's error).
pub(crate) fn accent_of(setting: Option<&Fields>) -> Option<String> {
    let text = setting?.get("value")?.as_str()?;
    if text.len() > MAX_SETTING_BYTES {
        return None;
    }
    let v: J = serde_json::from_str(text).ok()?;
    tile::valid_accent(v.get("theme")?.get("accent")?.as_str()?)
}

/// The `site` object of the instance settings (the keys of the public site), when the settings
/// parse and hold one; empty otherwise. Read with the theme, from the same row of settings, so that
/// `get` reports on the `site.*` keys the public site is told about, and by the one door the public
/// site reads its settings by (`site::site_settings`), where it is the read that door always made,
/// with the same result for the same text. Total.
pub(crate) fn site_of(setting: Option<&Fields>) -> serde_json::Map<String, J> {
    let parsed = setting
        .and_then(|s| s.get("value")?.as_str())
        .and_then(|t| serde_json::from_str::<J>(t).ok());
    match parsed {
        Some(v) => match v.get("site") {
            Some(J::Object(o)) => o.clone(),
            _ => serde_json::Map::new(),
        },
        None => serde_json::Map::new(),
    }
}

/// The text fields of a row as a [`Brand`], each checked by the safety subset, and the fields that
/// fell back. The brand is set: the row exists. Does not read the artwork.
pub(crate) fn read_text(row: &Fields) -> (Brand, Vec<Fallback>) {
    let mut fallbacks = Vec::new();
    let mut checked = |field: &str, kind: Kind| -> String {
        match text_of(row, field).and_then(|t| policy::safe(kind, t).map(|()| t)) {
            Ok(t) => t.to_string(),
            Err(code) => {
                fallbacks.push(Fallback::word(field, code));
                String::new()
            }
        }
    };
    let name = match checked("name", Kind::Name) {
        n if n.is_empty() => DEFAULT_NAME.to_string(),
        n => n,
    };
    let titles: Vec<String> = LANGS
        .iter()
        .map(|l| checked(&format!("title_{l}"), Kind::Title))
        .collect();
    let messages: Vec<String> = LANGS
        .iter()
        .map(|l| checked(&format!("message_{l}"), Kind::Message))
        .collect();
    let slug = match text_of(row, "slug") {
        Ok("") => String::new(),
        Ok(s) if slug::is_valid(s) => s.to_string(),
        Ok(_) => {
            fallbacks.push(Fallback::word("slug", reason::INVALID_SLUG));
            String::new()
        }
        Err(code) => {
            fallbacks.push(Fallback::word("slug", code));
            String::new()
        }
    };
    let mut brand = Brand::new(name).with_slug(slug);
    for (i, lang) in LANGS.iter().enumerate() {
        brand.set_title(lang, titles[i].as_str());
        brand.set_message(lang, messages[i].as_str());
    }
    // The fallbacks were pushed in the order the closure ran; put them in the order of the fields
    // (name, slug, titles, messages), which is the order a reader of `get` expects.
    fallbacks.sort_by_key(|f| field_rank(&f.field));
    (brand, fallbacks)
}

fn field_rank(field: &str) -> usize {
    if field == "name" {
        return 0;
    }
    if field == "slug" {
        return 1;
    }
    for (i, l) in LANGS.iter().enumerate() {
        if field == format!("title_{l}") {
            return 2 + i;
        }
        if field == format!("message_{l}") {
            return 2 + LANGS.len() + i;
        }
    }
    2 + 2 * LANGS.len()
}

/// The whole read check of a row (or of no row) with the accent the theme stores. Total.
pub(crate) fn read_row(row: Option<&Fields>, theme_accent: Option<String>) -> Shown {
    let Some(row) = row else {
        return Shown::not_set(theme_accent);
    };
    let (brand, mut fallbacks) = read_text(row);
    let stored = match text_of(row, "logo_svg") {
        Ok(t) => t,
        Err(code) => {
            fallbacks.push(Fallback::word("logo_svg", code));
            ""
        }
    };
    let logo = Logo::resolve(stored, brand.name(), theme_accent.as_deref());
    if let Some(refusals) = logo.fallback() {
        fallbacks.push(Fallback::artwork(refusals));
    }
    Shown {
        brand,
        logo: Some(logo),
        theme_accent,
        fallbacks,
        unread: false,
    }
}
