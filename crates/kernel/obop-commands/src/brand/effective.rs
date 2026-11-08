//! The `effective` answer of `get` (docs/spec/branding.md, section 3, "What `get` shows"): each public
//! value in force with the source that won, so that an operator can see why a surface still says
//! `obop`.
//!
//! The object is staged by the slice that wires the surface an entry describes, because an entry
//! whose precedence involves a surface that is not built yet would be untrue at the merge. It holds:
//! `name`, `slug`, `logo`, `tile_accent`, `title_<lang>` and `message_<lang>` (seven languages each)
//! and, since the slice that reads the record for the enrolment texts, `authenticator_issuer`. The
//! `site.*` entries and the favicon arrive with the slice that reads the record for the public site
//! (S1 of the spec, section 12, and not in this part of it), and `accent` with the one that reads it
//! for the back office (S2). Each of those adds its entries here and its cases to the tests.
//!
//! **The source.** `record` when the value is the stored value of a field that passed the read check,
//! or is derived from one (the effective slug from a stored name, a title that falls back to the
//! English title or to the name); `env` for the issuer when `OBOP_MFA_ISSUER` is set, which wins over
//! the record (read by [`env_issuer`], the function the authenticator's configuration reads it with,
//! so the two cannot differ); `theme` for the accent the theme stores; `default` when it is the
//! built-in default (the name `obop`, no message, no logo). A stored name that failed the read check
//! shows the default name here as it does for `name`, and `fallbacks` lists it; an enrolment is
//! refused for as long as the name is in that list (docs/DEPLOY.md), which this entry does not say.
//! The artwork is never part of the answer:
//! it is served by the logo route, and `get` says only whether the logo in force is the artwork
//! (`artwork`), the generated tile (`tile`) or none (`none`, with no brand set).

use super::enrol::env_issuer;
use super::reader::Fresh;
use obop_brand::{LANGS, tile};
use serde_json::{Map, Value as J, json};

const RECORD: &str = "record";
const ENV: &str = "env";
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
        // The label and `issuer=` of a new authenticator entry: the variable, else the stored name,
        // else `obop` (spec, section 5). `Auth` makes the same choice at each enrolment.
        out.insert(
            "authenticator_issuer".into(),
            match issuer_variable {
                Some(value) => entry(value, ENV),
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
