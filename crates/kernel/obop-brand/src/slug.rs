//! The slug of an instance: the short ASCII stem of the recovery-codes file and the name of the MCP
//! server (docs/spec/branding.md, section 2). It is given (`a-z`, `0-9` and `-`, starting with a
//! letter, at most 30, no trailing `-`) or derived from the name.

use crate::policy::SLUG_MAX;
use crate::refusal::{Refusal, show_char};
use crate::unicode::{category, is_mark, nfkd};

/// The slug of a name that has no Latin letter or digit (Cyrillic, Han): it never falls back to
/// the default name.
pub const FALLBACK_SLUG: &str = "instance";

/// Whether `s` is a slug: the fixed pattern, which is also the read check of a stored slug.
pub fn is_valid(s: &str) -> bool {
    check(s).is_ok()
}

/// Checks a given slug against the pattern and says which part it breaks. An empty slug is not a
/// slug (it means "derive one"; the caller handles that before it asks).
pub fn check(s: &str) -> Result<(), Refusal> {
    if s.is_empty() {
        return Err(Refusal::new("empty", "must not be empty"));
    }
    if s.chars().count() > SLUG_MAX {
        return Err(Refusal::new(
            "too_long",
            format!("is longer than {SLUG_MAX} characters"),
        ));
    }
    if let Some(c) = s
        .chars()
        .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-'))
    {
        return Err(Refusal::new(
            "character_not_allowed",
            format!(
                "holds a character that is not allowed ({}): only a-z, 0-9 and - are",
                show_char(c)
            ),
        ));
    }
    if !s.starts_with(|c: char| c.is_ascii_lowercase()) {
        return Err(Refusal::new(
            "must_start_with_letter",
            "must start with a letter",
        ));
    }
    if s.ends_with('-') {
        return Err(Refusal::new("trailing_hyphen", "must not end with -"));
    }
    Ok(())
}

/// The slug derived from a name: decompose (NFKD), drop the marks, lowercase, keep each run of
/// `a-z` and `0-9`, join the runs with `-`; a result that starts with a digit gets `i-` in front;
/// cut to 30 characters and trim a trailing `-`. Nothing left means [`FALLBACK_SLUG`].
///
/// Only `a-z` and `0-9` survive, so `Straße` gives `stra-e`, `Ærø` gives `r` and `Łódź` gives
/// `odz`: an operator who wants a readable slug sets one.
pub fn derive(name: &str) -> String {
    let mut runs: Vec<String> = Vec::new();
    let mut run = String::new();
    for c in nfkd(name).chars() {
        if is_mark(category(c)) {
            continue;
        }
        for l in c.to_lowercase() {
            if l.is_ascii_lowercase() || l.is_ascii_digit() {
                run.push(l);
            } else if !run.is_empty() {
                runs.push(std::mem::take(&mut run));
            }
        }
    }
    if !run.is_empty() {
        runs.push(run);
    }
    let mut slug = runs.join("-");
    if slug.is_empty() {
        return FALLBACK_SLUG.to_string();
    }
    if slug.starts_with(|c: char| c.is_ascii_digit()) {
        slug.insert_str(0, "i-");
    }
    slug.truncate(SLUG_MAX);
    while slug.ends_with('-') {
        slug.pop();
    }
    slug
}
