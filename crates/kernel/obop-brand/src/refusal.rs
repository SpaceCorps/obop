//! What a rule says when it refuses a value: a stable code and a sentence for a person. Neither
//! ever holds raw text from the input, only what [`show_char`] and `show_text` make safe, so a
//! refusal can be written to a log line or shown on a page as it is.

use std::fmt;

/// A refused value. `code` is a fixed word a test or a script can match; `message` is the rule in
/// words, naming the character, number, element or attribute that broke it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub code: &'static str,
    pub message: String,
}

impl Refusal {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Refusal {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// A code point as a person reads it in a refusal: `U+200B`, and for a printable ASCII character
/// that character too (`U+003A ':'`). Never the raw character otherwise (it may be invisible or
/// reorder the text around it).
pub(crate) fn show_char(c: char) -> String {
    if c.is_ascii_graphic() {
        format!("U+{:04X} '{c}'", c as u32)
    } else {
        format!("U+{:04X}", c as u32)
    }
}

/// Text from the input made safe to repeat: only `A-Z a-z 0-9` and `: _ . - # %` kept, every
/// other character shown as `?`, cut at `max` characters with `...`.
pub(crate) fn show_text(s: &str, max: usize) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i == max {
            out.push_str("...");
            break;
        }
        if c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '.' | '-' | '#' | '%') {
            out.push(c);
        } else {
            out.push('?');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_never_repeats_raw_text() {
        assert_eq!(show_char('\u{202E}'), "U+202E");
        assert_eq!(show_char(':'), "U+003A ':'");
        assert_eq!(show_text("a\u{202E}b<c>\n", 40), "a?b?c??");
        assert_eq!(show_text("abcdefghij", 4), "abcd...");
    }
}
