//! The `effective` answer of `get` (docs/spec/branding.md, section 3, "What `get` shows"): each public
//! value in force with the source that won, so that an operator can see why a surface still says
//! `obop`.
//!
//! The object is staged by the slice that wires the surface an entry describes, because an entry
//! whose precedence involves a surface that is not built yet would be untrue at the merge. It holds,
//! from the first slice: `name`, `slug`, `logo`, `tile_accent`, `title_<lang>` and `message_<lang>`
//! (seven languages each); since the slice that reads the record for the enrolment texts,
//! `authenticator_issuer`; from the slice that reads the record for the public site (S1f), the
//! entries `site.title`, `site.logo_url`, `site.accent` and `favicon`, decided by the same function
//! that fills the site's settings (`site::view`), so that what `get` says is what the shop shows;
//! and, since the back office page reads the record (S2), `accent`: the accent the sign-in screens
//! use, the theme's stored one (source `theme`) else the stock violet (source `default`). Each of
//! those adds its entries here and its cases to the tests.
//!
//! **The source.** `record` when the value is the stored value of a field that passed the read check,
//! or is derived from one (the effective slug from a stored name, a title that falls back to the
//! English title or to the name, the shop's title that falls back to the name); `env` for the issuer
//! when `OBOP_MFA_ISSUER` is set, which wins over the record (read by [`env_issuer`], the function the
//! authenticator's configuration reads it with, so the two cannot differ); `site` for an explicit
//! `site.*` key of the public site, which wins over the record; `theme` for the accent the theme
//! stores; `default` when it is the built-in default (the name `obop`, no message, no logo, the
//! shop's own title and teal); `unreadable` for the authenticator issuer alone, when an enrolment is
//! refused because the stored name failed the read check or cannot go into a label (a link
//! character), with the value `null`: there is no issuer in force. A row whose name failed the read
//! check is a brand with the default name: what is derived from the name alone (the slug, a title,
//! the tile of the logo, the shop's title and the logo route that serves the tile) is `default` then,
//! in `get` and in `obop admin brand status` alike (`site::name_stored` decides it). `name` is not
//! changed by the issuer being `unreadable`: for a stored name that failed the read check it shows
//! the default, as it always does, with `fallbacks` listing the field (a name of nothing but spaces
//! is one of these since S1c: the read check calls it empty); for one that passed the read check and
//! only cannot go into a label it shows the stored name, because the display accepts it, with
//! nothing under `fallbacks` (docs/DEPLOY.md). The artwork is never part of the answer: it is served
//! by the logo route, and `get` says only whether the logo in force is the artwork (`artwork`), the
//! generated tile (`tile`) or none (`none`, with no brand set); `favicon` says whether the shop's tab
//! icon is the record's artwork (`artwork`) or the shop's own tile (`tile`).

use super::enrol::{env_issuer, unfit_for_a_label};
use super::reader::Fresh;
use super::site;
use obop_brand::{LANGS, tile};
use serde_json::{Map, Value as J, json};

const RECORD: &str = "record";
const ENV: &str = "env";
const THEME: &str = "theme";
const DEFAULT: &str = "default";
const UNREADABLE_SOURCE: &str = "unreadable";

fn entry(value: impl Into<J>, source: &str) -> J {
    json!({"value": value.into(), "source": source})
}

fn source(from_record: bool) -> &'static str {
    if from_record { RECORD } else { DEFAULT }
}

impl Fresh {
    /// The `effective` object of the answer of `get`, as compact JSON text: for each public value
    /// of the slice, `{"value": ..., "source": ...}`. See the module documentation. The issuer
    /// variable is read from the environment of this process by [`env_issuer`].
    pub fn effective_json(&self) -> String {
        self.effective_json_with(env_issuer())
    }

    /// As [`Fresh::effective_json`] with the value of `OBOP_MFA_ISSUER` given, so that a test (or a
    /// caller that holds the variable already) does not have to change the environment of the
    /// process.
    pub fn effective_json_with(&self, issuer_variable: Option<String>) -> String {
        let shown = self.shown();
        let brand = shown.brand();
        // A brand whose name passed the read check: the name, and everything derived from it, comes
        // from the record. A row whose name failed it shows the default name.
        let name_stored = site::name_stored(shown);
        // What an enrolment does with it: refused, for the two reasons `strict` refuses a row.
        let name_failed = self.set() && shown.fallbacks().iter().any(|f| f.field == "name");
        let enrolment_refused = name_failed || (name_stored && unfit_for_a_label(brand.name()));
        let mut out = Map::new();
        out.insert("name".into(), entry(brand.name(), source(name_stored)));
        out.insert(
            "slug".into(),
            entry(
                brand.slug(),
                source(self.set() && (!brand.stored_slug().is_empty() || name_stored)),
            ),
        );
        // The label and `issuer=` of a new authenticator entry: the variable, else the stored name,
        // else `obop` (spec, section 5). `Auth` makes the same choice at each enrolment, with one
        // difference that this entry states instead of hiding: where the enrolment is refused (the
        // stored name failed the read check or cannot go into a label) there is no value in force,
        // so the entry is `null` from the source `unreadable`. `name` is as it always was: the default
        // for a name that failed the read check, the stored name for one that only cannot go into a
        // label (the display accepts it).
        out.insert(
            "authenticator_issuer".into(),
            match issuer_variable {
                Some(value) => entry(value, ENV),
                None if enrolment_refused => entry(J::Null, UNREADABLE_SOURCE),
                None => entry(brand.name(), source(name_stored)),
            },
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
        // The accent of the sign-in screens: the theme's, else the stock violet of the back office.
        out.insert(
            "accent".into(),
            match shown.theme_accent() {
                Some(a) => entry(a, THEME),
                None => entry(tile::DEFAULT_VIOLET, DEFAULT),
            },
        );
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
