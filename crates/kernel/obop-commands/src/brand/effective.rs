//! The `effective` answer of `get` (docs/spec/branding.md, section 3, "What `get` shows"): each public
//! value in force with the source that won, so that an operator can see why a surface still says
//! `obop`.
//!
//! The object is staged by the slice that wires the surface an entry describes, because an entry
//! whose precedence involves a surface that is not built yet would be untrue at the merge. It holds,
//! from the first slice: `name`, `slug`, `logo`, `tile_accent`, `title_<lang>` and `message_<lang>`
//! (seven languages each); from the slice that reads the record for the public site (S1f), the
//! entries `site.title`, `site.logo_url`, `site.accent` and `favicon`, decided by the same function
//! that fills the site's settings (`site::view`), so that what `get` says is what the shop shows.
//! `authenticator_issuer` arrives with the slice that reads the record for the authenticator entry,
//! and `accent` with the one that reads it for the back office (S2). Each of those adds its entries
//! here and its cases to the tests.
//!
//! **The source.** `record` when the value is the stored value of a field that passed the read check,
//! or is derived from one (the effective slug from a stored name, a title that falls back to the
//! English title or to the name, the shop's title that falls back to the name); `site` for an
//! explicit `site.*` key of the public site, which wins over the record; `theme` for the accent the
//! theme stores; `default` when it is the built-in default (the name `obop`, no message, no logo,
//! the shop's own title and teal). The artwork is never part of the answer: it is served by the logo
//! route, and `get` says only whether the logo in force is the artwork (`artwork`), the generated
//! tile (`tile`) or none (`none`, with no brand set); `favicon` says whether the shop's tab icon is
//! the record's artwork (`artwork`) or the shop's own tile (`tile`).

use super::reader::Fresh;
use super::site;
use obop_brand::{LANGS, tile};
use serde_json::{Map, Value as J, json};

const RECORD: &str = "record";
const THEME: &str = "theme";
const DEFAULT: &str = "default";

fn entry(value: impl Into<J>, source: &str) -> J {
    json!({"value": value.into(), "source": source})
}

fn source(from_record: bool) -> &'static str {
    if from_record { RECORD } else { DEFAULT }
}

impl Fresh {
    /// The `effective` object of the answer of `get`, as compact JSON text: for each public value
    /// of the slice, `{"value": ..., "source": ...}`. See the module documentation.
    pub fn effective_json(&self) -> String {
        let shown = self.shown();
        let brand = shown.brand();
        // A brand whose name passed the read check: the name, and everything derived from it, comes
        // from the record. A row whose name failed it shows the default name.
        let name_stored = self.set() && !shown.fallbacks().iter().any(|f| f.field == "name");
        let mut out = Map::new();
        out.insert("name".into(), entry(brand.name(), source(name_stored)));
        out.insert(
            "slug".into(),
            entry(
                brand.slug(),
                source(self.set() && (!brand.stored_slug().is_empty() || name_stored)),
            ),
        );
        let logo = match shown.logo() {
            None => entry("none", DEFAULT),
            Some(l) if l.is_tile() => entry("tile", source(name_stored)),
            Some(_) => entry("artwork", RECORD),
        };
        out.insert("logo".into(), logo);
        let accent = tile::tile_accent(shown.theme_accent());
        out.insert(
            "tile_accent".into(),
            entry(accent.hex, if accent.from_theme { THEME } else { DEFAULT }),
        );
        // The public site: the same decision that fills its settings, so the answer cannot drift
        // from what the shop shows.
        let site = site::view(self.site(), shown);
        for (key, chosen) in [
            ("site.title", &site.title),
            ("site.logo_url", &site.logo_url),
            ("site.accent", &site.accent),
            ("favicon", &site.favicon),
        ] {
            out.insert(
                key.into(),
                entry(chosen.value.as_str(), chosen.source.word()),
            );
        }
        for lang in LANGS {
            let from_record = self.set()
                && (!brand.stored_title(lang).is_empty()
                    || !brand.stored_title("en").is_empty()
                    || name_stored);
            out.insert(
                format!("title_{lang}"),
                entry(brand.title(lang), source(from_record)),
            );
        }
        for lang in LANGS {
            let message = brand.message(lang);
            out.insert(
                format!("message_{lang}"),
                entry(message.unwrap_or(""), source(message.is_some())),
            );
        }
        J::Object(out).to_string()
    }
}
