//! Locale-aware text ordering for server-side sorts (D3).
//!
//! The back office sorts text in the browser with
//! `new Intl.Collator(locale, { numeric: true, sensitivity: "base" })` and falls back to comparing
//! the raw text. The server does the same with the Unicode Collation Algorithm and the CLDR
//! tailorings from `icu_collator` (Unicode-3.0 licence, data compiled in, no network and no system
//! locale): primary strength (letters only, so `a`, `A` and `á` tie), numeric ordering of digit
//! runs (`item 2` before `item 10`), then the raw text, then the record id, so the order is total and
//! stable. German, Swedish, Finnish and Polish therefore order `ä`, `å`, `ö` and `ł` as their
//! dictionaries do, and the same page sorts the same way on the server and in the browser. The
//! `lang` hint in the `search` input picks the tailoring (a BCP-47 tag such as `sv` or `de-AT`);
//! without it the root (language-neutral) order applies.
//!
//! Cost: comparing two strings is linear in their length; a sort is `O(n log n)` comparisons over
//! the matching records, and the collator is built once per query.

use icu_collator::options::{CollatorOptions, Strength};
use icu_collator::preferences::CollationNumericOrdering;
use icu_collator::{Collator, CollatorBorrowed, CollatorPreferences};
use icu_locale::Locale;
use std::cmp::Ordering;

/// Longest accepted `lang` hint.
const MAX_LANG: usize = 35;

pub struct TextOrder {
    collator: CollatorBorrowed<'static>,
}

impl std::fmt::Debug for TextOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TextOrder")
    }
}

/// Check a `lang` hint; the error names the problem.
pub fn parse_lang(lang: &str) -> Result<Locale, String> {
    let bad = || "lang must be a BCP-47 language tag such as \"sv\" or \"de-AT\"".to_string();
    if lang.is_empty() || lang.len() > MAX_LANG {
        return Err(bad());
    }
    lang.parse::<Locale>().map_err(|_| bad())
}

impl TextOrder {
    /// The order of `lang`, or the root order when `None`.
    pub fn new(lang: Option<&Locale>) -> Self {
        let mut prefs = lang.map_or_else(CollatorPreferences::default, CollatorPreferences::from);
        prefs.numeric_ordering = Some(CollationNumericOrdering::True);
        let mut options = CollatorOptions::default();
        options.strength = Some(Strength::Primary);
        let collator = Collator::try_new(prefs, options)
            .or_else(|_| Collator::try_new(CollatorPreferences::default(), options))
            .expect("the root collation data is compiled in");
        TextOrder { collator }
    }

    /// Collation order, then the exact text. `Equal` only for identical strings.
    pub fn compare(&self, a: &str, b: &str) -> Ordering {
        self.collator.compare(a, b).then_with(|| a.cmp(b))
    }
}
