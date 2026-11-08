//! What the public site and shop take from the brand (docs/spec/branding.md, section 5 and the row
//! OT-04 of section 6). The site reads its settings through one door, `PublicApi::site_settings`,
//! and the brand reaches it there and nowhere else: [`fill_site`] puts the fallbacks of the record
//! into the `site` object that door returns, for the keys that object leaves unset, and hands the
//! artwork over through one reserved key.
//!
//! **The order.** An explicit `site.*` key always wins; the record is the default; today's default
//! comes last:
//!
//! | Key | Explicit | With a brand | Without |
//! |---|---|---|---|
//! | `title` | `site.title` | the name | `My Shop` |
//! | `logo_url` | `site.logo_url` | the logo route with its version, `/api/brand/logo.svg?v=<v>` | none |
//! | `accent` | `site.accent` | the tile accent (the theme's accent, else the slate) | teal |
//! | the tab icon | none | the artwork, when the record has one | the shop's own tile |
//!
//! `logo_text` is not filled: the site makes it follow the title, so the glyph of the shop's own
//! tile follows the name. A key counts as unset exactly when the site would not use it, that is
//! when `explicit_title`, `explicit_logo_url` or `explicit_accent` (the rules of
//! `obop_site::settings::Settings::from_map`, held to it by a test in `obop-site`) find nothing in
//! it: absent, null, not text, blank, or text the site would refuse (a colour that is no colour, a
//! logo address that is not an `https://` URL or a path). Without a brand nothing is filled and the
//! map is what it was.
//!
//! **The artwork.** The shop answers `/favicon.svg` and `/favicon.ico` with the artwork when the
//! record has one (the generated tile is not an artwork: the shop keeps its own tile then). It
//! reaches the shop through the reserved key `FAVICON_KEY`, an object `{"svg": ..., "v": ...}`
//! that the reader computed, so the shop hashes and re-derives nothing and its `ETag` is the logo
//! route's `v`. Because `setting.update` writes any key of the `site` object, [`fill_site`] first
//! removes a stored key of that name: nobody can put an artwork on the shop's tab icon by writing
//! to the settings.

use super::reader::Reader;
use super::row::{self, Shown};
use super::{SETTING_ID, SETTING_MODEL};
use obop_brand::{logo, theme, tile};
use obop_storage::Backend;
use serde_json::{Map, Value as J, json};

/// The reserved key of the `site` object through which the artwork reaches the shop's tab icon. Its
/// value is `{"svg": <the artwork as the reader re-serialised it>, "v": <its version>}`. The shop
/// (`obop-site`) imports this constant; a stored key with this name is removed before anything is
/// filled in.
pub const FAVICON_KEY: &str = "_brand_favicon";

/// The title of a shop that has neither a `site.title` nor a brand.
pub const DEFAULT_TITLE: &str = "My Shop";

/// Where a value of the shop comes from: the order of section 5.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    /// An explicit `site.*` key.
    Site,
    /// The record.
    Record,
    /// The accent of the instance theme.
    Theme,
    /// Today's default (or, for the accent of a brand without a theme accent, the slate).
    Default,
}

impl Source {
    pub(crate) fn word(self) -> &'static str {
        match self {
            Source::Site => "site",
            Source::Record => "record",
            Source::Theme => "theme",
            Source::Default => "default",
        }
    }
}

/// A value in force and where it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Chosen {
    pub value: String,
    pub source: Source,
}

impl Chosen {
    fn new(value: impl Into<String>, source: Source) -> Chosen {
        Chosen {
            value: value.into(),
            source,
        }
    }
}

/// What the shop shows, with the source of each: the one decision that `fill_site` carries out
/// and the `effective` answer of `get` reports.
pub(crate) struct View {
    pub title: Chosen,
    pub logo_url: Chosen,
    pub accent: Chosen,
    /// `artwork` (the record's, served on the shop's tab icon address) or `tile` (the shop's own).
    pub favicon: Chosen,
    /// The artwork and its version, when the tab icon is the record's.
    pub artwork: Option<(String, String)>,
}

/// The text of a `site` key as the site reads it: control characters other than the line feed
/// dropped, trimmed, at most `max` characters, and nothing for an empty text or a value that is not
/// text.
fn site_text(site: &Map<String, J>, key: &str, max: usize) -> Option<String> {
    let s: String = site
        .get(key)?
        .as_str()?
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .collect();
    let s = s.trim();
    (!s.is_empty()).then(|| s.chars().take(max).collect())
}

/// A URL that is safe to put into an attribute: no whitespace, quotes or angle brackets.
fn clean_url(u: &str) -> bool {
    !u.is_empty()
        && u.len() <= 500
        && !u
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || "\"'<>`\\".contains(c))
}

/// The `site.title` the site would use: its text, at most 80 characters. `None` when the key is
/// unset in the sense of the module documentation.
pub(crate) fn explicit_title(site: &Map<String, J>) -> Option<String> {
    site_text(site, "title", 80)
}

/// The `site.logo_url` the site would use: an `https://` URL without credentials or a path on the
/// site.
pub(crate) fn explicit_logo_url(site: &Map<String, J>) -> Option<String> {
    site_text(site, "logo_url", 500).filter(|u| {
        clean_url(u)
            && ((u.starts_with("https://") && !u.contains('@'))
                || (u.starts_with('/') && !u.starts_with("//")))
    })
}

/// The `site.accent` the site would use, as the site normalises it (`#rrggbb`, lower case).
pub(crate) fn explicit_accent(site: &Map<String, J>) -> Option<String> {
    site_text(site, "accent", 9).and_then(|a| theme::parse_hex(&a).map(theme::to_hex))
}

/// The decision for one `site` object and one brand. Without a brand (no row) it is today's:
/// the explicit keys, else the defaults; nothing comes from the record.
pub(crate) fn view(site: &Map<String, J>, shown: &Shown) -> View {
    let branded = shown.is_set();
    let title = match explicit_title(site) {
        Some(t) => Chosen::new(t, Source::Site),
        None if branded => Chosen::new(shown.brand().name(), Source::Record),
        None => Chosen::new(DEFAULT_TITLE, Source::Default),
    };
    let logo_url = match explicit_logo_url(site) {
        Some(u) => Chosen::new(u, Source::Site),
        None if branded => Chosen::new(logo::logo_url(shown.v()), Source::Record),
        None => Chosen::new("", Source::Default),
    };
    let accent = match explicit_accent(site) {
        Some(a) => Chosen::new(a, Source::Site),
        None if branded => {
            let t = tile::tile_accent(shown.theme_accent());
            let source = if t.from_theme {
                Source::Theme
            } else {
                Source::Default
            };
            Chosen::new(t.hex, source)
        }
        None => Chosen::new(theme::DEFAULT_ACCENT, Source::Default),
    };
    // The generated tile is no artwork: the shop keeps its own tile (drawn from its logo text and
    // its accent) when the record has none.
    let artwork = shown
        .logo()
        .filter(|l| !l.is_tile())
        .map(|l| (l.served().to_string(), l.v().to_string()));
    let favicon = match artwork {
        Some(_) => Chosen::new("artwork", Source::Record),
        None => Chosen::new("tile", Source::Default),
    };
    View {
        title,
        logo_url,
        accent,
        favicon,
        artwork,
    }
}

/// Puts the fallbacks of the brand into the `site` object of the settings, as the module
/// documentation says. First it removes a stored key with the reserved name `FAVICON_KEY`, whatever it holds,
/// because `setting.update` writes any key and the artwork of the shop's tab icon is the reader's
/// alone. Without a brand that is all it does.
pub fn fill_site(site: &mut Map<String, J>, shown: &Shown) {
    site.remove(FAVICON_KEY);
    if !shown.is_set() {
        return;
    }
    let v = view(site, shown);
    for (key, chosen) in [
        ("title", &v.title),
        ("logo_url", &v.logo_url),
        ("accent", &v.accent),
    ] {
        if chosen.source != Source::Site {
            site.insert(key.to_string(), J::String(chosen.value.clone()));
        }
    }
    if let Some((svg, version)) = v.artwork {
        site.insert(FAVICON_KEY.to_string(), json!({"svg": svg, "v": version}));
    }
}

/// The `site` object of the instance settings with the brand filled in: what `PublicApi::site_settings`
/// answers, the one door the public site reads its settings by. The settings are read here, the
/// reader is asked once (`current()`: the last good brand when its refill fails, the default in a
/// process that never had a good read), and the object is filled by [`fill_site`]. A settings row
/// that cannot be read gives an empty object, as the door always did; it is the request that met the
/// fault, and not this call, that is answered 503 (D74).
pub fn site_settings(backend: &dyn Backend, reader: &Reader) -> Map<String, J> {
    let setting = backend.get(SETTING_MODEL, SETTING_ID).ok().flatten();
    let mut site = row::site_of(setting.as_ref());
    fill_site(&mut site, &reader.current());
    site
}
