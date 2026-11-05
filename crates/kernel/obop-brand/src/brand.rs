//! The brand of an instance as a page reads it: the name, the slug, a title and a message per
//! language, and the one switch that says a brand is set (docs/spec/branding.md, sections 2, 7 and
//! 10). No I/O: the reader that `obop-commands` will hold builds a [`Brand`] from the stored row,
//! field by field, after the read check.

use crate::slug;
use crate::unicode::{category, default_ignorable, nfkc};
use icu_properties::props::GeneralCategory as Gc;

/// The name an instance shows when no brand is set.
pub const DEFAULT_NAME: &str = "obop";

/// The languages of the pages, in the order of the stored fields (`title_<lang>`,
/// `message_<lang>`). The same list as the OAuth pages' `LANGS`; a test added with the model will
/// tie the model's language fields to it.
pub const LANGS: [&str; 7] = ["en", "de", "fr", "pl", "fi", "sv", "es"];

/// The position of a language in [`LANGS`], `None` for any other.
pub fn lang_index(lang: &str) -> Option<usize> {
    LANGS.iter().position(|l| *l == lang)
}

/// The brand of an instance. `Brand::default()` is the name `obop`, no titles, no messages and the
/// neutral wording off, and so is any instance with no row: every consumer asks a `Brand` for the
/// name and so takes one code path with or without a row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Brand {
    set: bool,
    name: String,
    slug: String,
    titles: [String; 7],
    messages: [String; 7],
}

impl Default for Brand {
    fn default() -> Self {
        Brand {
            set: false,
            name: DEFAULT_NAME.to_string(),
            slug: String::new(),
            titles: Default::default(),
            messages: Default::default(),
        }
    }
}

impl Brand {
    /// A brand that is set, with this name and nothing else yet. The caller has checked the name
    /// (the policy on a write, the safety subset on a read).
    pub fn new(name: impl Into<String>) -> Brand {
        Brand {
            set: true,
            name: name.into(),
            ..Brand::default()
        }
    }

    /// Sets the slug as stored: empty means derived from the name.
    pub fn with_slug(mut self, slug: impl Into<String>) -> Brand {
        self.slug = slug.into();
        self
    }

    /// Sets the title of one language; `false` and nothing set for a language that is not in
    /// [`LANGS`].
    pub fn set_title(&mut self, lang: &str, text: impl Into<String>) -> bool {
        match lang_index(lang) {
            Some(i) => {
                self.titles[i] = text.into();
                true
            }
            None => false,
        }
    }

    /// Sets the message of one language; `false` and nothing set for a language that is not in
    /// [`LANGS`].
    pub fn set_message(&mut self, lang: &str, text: impl Into<String>) -> bool {
        match lang_index(lang) {
            Some(i) => {
                self.messages[i] = text.into();
                true
            }
            None => false,
        }
    }

    /// Whether a brand is set: the row exists. The only whole-brand test.
    pub fn is_set(&self) -> bool {
        self.set
    }

    /// The neutral wording of the sentences that name the product (spec section 7) is used
    /// whenever a brand is set, in every language, and never otherwise.
    pub fn neutral(&self) -> bool {
        self.set
    }

    /// The product name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The slug as stored; empty when it is derived.
    pub fn stored_slug(&self) -> &str {
        &self.slug
    }

    /// The effective slug: the stored one, or the one derived from the effective name.
    pub fn slug(&self) -> String {
        if self.slug.is_empty() {
            slug::derive(&self.name)
        } else {
            self.slug.clone()
        }
    }

    /// The title of one language as stored (empty when unset), with no fallback.
    pub fn stored_title(&self, lang: &str) -> &str {
        lang_index(lang).map_or("", |i| &self.titles[i])
    }

    /// The message of one language as stored (empty when unset), with no fallback.
    pub fn stored_message(&self, lang: &str) -> &str {
        lang_index(lang).map_or("", |i| &self.messages[i])
    }

    /// The line beside the logo on the OAuth and signer pages, by the fallback chain: the title
    /// of the language, else the English title, else the name.
    pub fn title(&self, lang: &str) -> &str {
        [self.stored_title(lang), self.stored_title("en")]
            .into_iter()
            .find(|t| !t.is_empty())
            .unwrap_or(&self.name)
    }

    /// The message under the lead of a sign-in screen, by the fallback chain: the message of the
    /// language, else the English message, else none.
    pub fn message(&self, lang: &str) -> Option<&str> {
        [self.stored_message(lang), self.stored_message("en")]
            .into_iter()
            .find(|m| !m.is_empty())
    }

    /// The messages that are set, as `(language, text)` in the order of [`LANGS`].
    pub fn messages(&self) -> impl Iterator<Item = (&'static str, &str)> {
        LANGS
            .iter()
            .zip(self.messages.iter())
            .filter(|(_, m)| !m.is_empty())
            .map(|(l, m)| (*l, m.as_str()))
    }

    /// The titles that are set, as `(language, text)` in the order of [`LANGS`].
    pub fn titles(&self) -> impl Iterator<Item = (&'static str, &str)> {
        LANGS
            .iter()
            .zip(self.titles.iter())
            .filter(|(_, t)| !t.is_empty())
            .map(|(l, t)| (*l, t.as_str()))
    }

    /// Whether a client's registered name is the instance's own name: with a brand set, the name
    /// equals the brand name, the effective slug or a title that is set, after both sides are
    /// folded alike by [`fold`]. The consent page then adds one fixed sentence (spec section 10).
    /// With no brand set it is always false. A name that only contains the brand is not equal.
    pub fn matches_client(&self, client_name: &str) -> bool {
        if !self.set {
            return false;
        }
        let client = fold(client_name);
        if client.is_empty() {
            return false;
        }
        std::iter::once(self.name.as_str())
            .chain(self.titles().map(|(_, t)| t))
            .map(str::to_string)
            .chain(std::iter::once(self.slug()))
            .any(|own| fold(&own) == client)
    }
}

/// The fold under which the client's name is compared with the brand's: remove every character
/// with the property Default_Ignorable_Code_Point or the general category `Cf` and all white space
/// (`char::is_whitespace`, the no-break space included), then NFKC, then `str::to_lowercase`. It
/// is a stated approximation of case folding: `ß`, a final sigma and the dotted capital `İ` are not
/// folded as a case folder would, and no punctuation is ignored.
pub fn fold(s: &str) -> String {
    let kept: String = s
        .chars()
        .filter(|c| !(default_ignorable(*c) || category(*c) == Gc::Format || c.is_whitespace()))
        .collect();
    nfkc(&kept).to_lowercase()
}
