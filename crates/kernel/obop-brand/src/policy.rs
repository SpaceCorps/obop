//! The character policy of an instance's name, titles and messages (docs/spec/branding.md,
//! section 2). One allow-list, applied when a value is written ([`clean`]); on every read only the
//! safety subset applies ([`safe`]), so a policy tightened in a later release never reverts a
//! stored name.
//!
//! A character is allowed only if its Unicode general category is a letter (with a script of its
//! own), a mark after a letter, a digit or another mark, a decimal digit, the ASCII space, or listed
//! punctuation. Whatever the category, a Default_Ignorable_Code_Point is refused. Three forms are
//! refused in all three kinds of text, on an NFKC-folded copy: the characters `: / \ @`, a run that
//! looks like a host name, and a run of seven or more digits. A name and a title must also be one
//! script (the allowance of the Highly Restrictive level of UTS #39).

use crate::refusal::{Refusal, show_char};
use crate::unicode::{
    category, default_ignorable, is_letter, is_mark, is_punctuation, nfc, nfkc, real_scripts,
};
use icu_properties::props::{GeneralCategory as Gc, Script};

/// The longest name, in Unicode code points.
pub const NAME_MAX: usize = 40;
/// The longest title, in Unicode code points.
pub const TITLE_MAX: usize = 60;
/// The longest message, in Unicode code points.
pub const MESSAGE_MAX: usize = 240;
/// The longest slug.
pub const SLUG_MAX: usize = 30;
/// Marks in a row, at most.
const MARKS_IN_A_ROW: usize = 3;
/// A run of this many digits or more is refused: it looks like a phone number.
const DIGIT_RUN_REFUSED: usize = 7;
/// Text longer than this many times the limit is refused before any table is read, so a huge
/// input costs nothing. (Normalisation never makes a text three times longer.)
const BOUND_BEFORE_NORMALISING: usize = 8;

/// Which text a value is: the rules differ a little.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The product name: 1 to 40 characters.
    Name,
    /// The line beside the logo, in one language: up to 60 characters.
    Title,
    /// The message under the lead of a sign-in screen, in one language: up to 240 characters.
    Message,
}

impl Kind {
    pub const fn max_chars(self) -> usize {
        match self {
            Kind::Name => NAME_MAX,
            Kind::Title => TITLE_MAX,
            Kind::Message => MESSAGE_MAX,
        }
    }

    const fn min_chars(self) -> usize {
        match self {
            Kind::Name => 1,
            Kind::Title | Kind::Message => 0,
        }
    }
}

/// The reason words of the read check (the safety subset). Fixed, so that a line written to a log
/// names the field and one of these and never the value.
pub mod reason {
    pub const EMPTY: &str = "empty";
    pub const TOO_LONG: &str = "too_long";
    pub const UNSAFE_CHARACTER: &str = "unsafe_character";
    pub const INVALID_SLUG: &str = "invalid_slug";
    pub const INVALID_ARTWORK: &str = "invalid_artwork";
}

/// The punctuation a name and a title may hold: the ASCII `- . , & ' + _ ( ) !` and nothing else.
fn listed_ascii(c: char) -> bool {
    matches!(
        c,
        '-' | '.' | ',' | '&' | '\'' | '+' | '_' | '(' | ')' | '!'
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Letter,
    Digit,
    Mark,
    Space,
    Other,
}

/// The write-side check: the whole policy. Returns the text in NFC, which is what is stored. An
/// empty title or message is no refusal (an empty value is how a field is cleared); an empty name
/// is.
pub fn clean(kind: Kind, input: &str) -> Result<String, Refusal> {
    let max = kind.max_chars();
    if input.chars().nth(max * BOUND_BEFORE_NORMALISING).is_some() {
        return Err(too_long(max));
    }
    let text = nfc(input);
    let n = text.chars().count();
    if n < kind.min_chars() {
        return Err(Refusal::new("empty", "must not be empty"));
    }
    if n > max {
        return Err(too_long(max));
    }
    if n == 0 {
        return Ok(text);
    }
    check_characters(kind, &text)?;
    check_forms(&text)?;
    if kind != Kind::Message {
        check_one_script(&text)?;
    }
    Ok(text)
}

fn too_long(max: usize) -> Refusal {
    Refusal::new(
        "too_long",
        format!("is longer than {max} characters (Unicode code points, after normalisation)"),
    )
}

fn check_characters(kind: Kind, text: &str) -> Result<(), Refusal> {
    let mut previous: Option<Class> = None;
    let mut marks = 0usize;
    for c in text.chars() {
        if c == '<' || c == '>' {
            return Err(Refusal::new("markup", "must not hold < or >, in any field"));
        }
        if default_ignorable(c) {
            return Err(Refusal::new(
                "invisible_character",
                format!(
                    "holds an invisible or formatting character ({}): zero-width characters, \
                     the soft hyphen, variation selectors, tag characters and fillers are not \
                     allowed",
                    show_char(c)
                ),
            ));
        }
        let g = category(c);
        if previous.is_none() && kind != Kind::Message && !(is_letter(g) || g == Gc::DecimalNumber)
        {
            return Err(Refusal::new(
                "must_start_with_letter_or_digit",
                format!("must start with a letter or a digit, not {}", show_char(c)),
            ));
        }
        let class = if is_letter(g) {
            if real_scripts(c).is_empty() {
                return Err(Refusal::new(
                    "letter_without_script",
                    format!(
                        "holds a letter that belongs to no script of its own ({}), such as the \
                         mathematical alphanumerics",
                        show_char(c)
                    ),
                ));
            }
            Class::Letter
        } else if matches!(g, Gc::NonspacingMark | Gc::SpacingMark) {
            if !matches!(previous, Some(Class::Letter | Class::Digit | Class::Mark)) {
                return Err(Refusal::new(
                    "mark_position",
                    format!(
                        "holds a combining mark ({}) that does not follow a letter, a digit or \
                         another mark",
                        show_char(c)
                    ),
                ));
            }
            marks += 1;
            if marks > MARKS_IN_A_ROW {
                return Err(Refusal::new(
                    "too_many_marks",
                    format!("holds more than {MARKS_IN_A_ROW} combining marks in a row"),
                ));
            }
            Class::Mark
        } else if g == Gc::DecimalNumber {
            Class::Digit
        } else if c == ' ' {
            if matches!(previous, None | Some(Class::Space)) {
                return Err(Refusal::new(
                    "space_position",
                    "must not start with a space or hold two spaces in a row",
                ));
            }
            Class::Space
        } else {
            match g {
                Gc::Control => {
                    return Err(Refusal::new(
                        "control_character",
                        format!("holds a control character ({})", show_char(c)),
                    ));
                }
                Gc::Format => {
                    return Err(Refusal::new(
                        "invisible_character",
                        format!(
                            "holds a formatting character ({}), such as a bidirectional \
                             control",
                            show_char(c)
                        ),
                    ));
                }
                Gc::PrivateUse | Gc::Surrogate => {
                    return Err(Refusal::new(
                        "private_use",
                        format!("holds a private-use code point ({})", show_char(c)),
                    ));
                }
                Gc::Unassigned => {
                    return Err(Refusal::new(
                        "unassigned",
                        format!("holds an unassigned code point ({})", show_char(c)),
                    ));
                }
                Gc::SpaceSeparator | Gc::LineSeparator | Gc::ParagraphSeparator => {
                    return Err(Refusal::new(
                        "space_character",
                        format!(
                            "holds a space other than the ASCII space ({}): no-break and \
                             ideographic spaces are not allowed",
                            show_char(c)
                        ),
                    ));
                }
                g if is_punctuation(g) => {
                    if kind != Kind::Message && !listed_ascii(c) {
                        return Err(Refusal::new(
                            "punctuation",
                            format!(
                                "holds punctuation that a name or a title may not hold ({}); \
                                 allowed are - . , & ' + _ ( ) !",
                                show_char(c)
                            ),
                        ));
                    }
                }
                Gc::CurrencySymbol => {
                    if kind != Kind::Message {
                        return Err(Refusal::new(
                            "symbol",
                            format!(
                                "holds a currency symbol ({}), which only a message may hold",
                                show_char(c)
                            ),
                        ));
                    }
                }
                Gc::MathSymbol if c == '+' => {}
                _ => {
                    return Err(Refusal::new(
                        "character_not_allowed",
                        format!(
                            "holds a character that is not allowed ({}): only letters, \
                             combining marks, digits, the space and punctuation are",
                            show_char(c)
                        ),
                    ));
                }
            }
            Class::Other
        };
        if class != Class::Mark {
            marks = 0;
        }
        previous = Some(class);
    }
    if previous == Some(Class::Space) {
        return Err(Refusal::new("space_position", "must not end with a space"));
    }
    Ok(())
}

/// The forms of a link, an address and a phone number, on an NFKC-folded copy of the text (so the
/// fullwidth forms count as their ASCII originals, and a combining mark cannot break a run). A
/// lint, not a guarantee: a message can still say where to go in words.
fn check_forms(text: &str) -> Result<(), Refusal> {
    let folded: Vec<char> = nfkc(text)
        .chars()
        .filter(|c| !is_mark(category(*c)))
        .collect();
    if let Some(c) = folded
        .iter()
        .copied()
        .find(|c| matches!(c, ':' | '/' | '\\' | '@'))
    {
        return Err(Refusal::new(
            "link_character",
            format!(
                "must not hold {}: it would make a link or an address",
                show_char(c)
            ),
        ));
    }
    // A run of characters other than white space that holds a dot with at least two letters or
    // digits directly before it and at least two directly after it, when one of those two groups
    // holds a letter: shop.example, example.com, mohv.se, v2.beta. A dot between groups of digits
    // only is a date or a time (24.12.2026, 10.30); a group of one letter (e.V., z.B., a.io) and a
    // dot at the end of a run (Inc.) are not hosts.
    for run in folded.split(|c| c.is_whitespace()) {
        for (i, c) in run.iter().enumerate() {
            if *c != '.' {
                continue;
            }
            let before = run[..i]
                .iter()
                .rev()
                .take_while(|c| c.is_alphanumeric())
                .count();
            let after = run[i + 1..]
                .iter()
                .take_while(|c| c.is_alphanumeric())
                .count();
            if before < 2 || after < 2 {
                continue;
            }
            let letter = run[i - before..i]
                .iter()
                .chain(&run[i + 1..i + 1 + after])
                .any(|c| c.is_alphabetic());
            if letter {
                return Err(Refusal::new(
                    "host_like",
                    "must not hold a dot between two groups of letters or digits that looks like a \
                     host name (shop.example); dates and times written with dots are allowed",
                ));
            }
        }
    }
    let mut run = 0usize;
    for c in &folded {
        if category(*c) == Gc::DecimalNumber {
            run += 1;
            if run >= DIGIT_RUN_REFUSED {
                return Err(Refusal::new(
                    "digit_run",
                    format!(
                        "must not hold {DIGIT_RUN_REFUSED} or more digits in a row: it looks \
                         like a phone number"
                    ),
                ));
            }
        } else {
            run = 0;
        }
    }
    Ok(())
}

/// The scripts a name or a title may mix: one script, or one of the three sets of the Highly
/// Restrictive level of UTS #39. Only letters count; Common and Inherited are neutral, so digits,
/// marks and the listed punctuation are free. A letter counts for every script of its
/// Script_Extensions.
fn check_one_script(text: &str) -> Result<(), Refusal> {
    const ALLOWED_SETS: [&[Script]; 3] = [
        &[
            Script::Latin,
            Script::Han,
            Script::Hiragana,
            Script::Katakana,
        ],
        &[Script::Latin, Script::Han, Script::Bopomofo],
        &[Script::Latin, Script::Han, Script::Hangul],
    ];
    let letters: Vec<Vec<Script>> = text
        .chars()
        .filter(|c| is_letter(category(*c)))
        .map(real_scripts)
        .collect();
    if letters.is_empty() {
        return Ok(());
    }
    let covered_by = |set: &[Script]| {
        letters
            .iter()
            .all(|scripts| scripts.iter().any(|s| set.contains(s)))
    };
    if ALLOWED_SETS.iter().any(|set| covered_by(set)) {
        return Ok(());
    }
    // One script: any script that every letter has.
    if letters
        .iter()
        .flatten()
        .any(|one| covered_by(std::slice::from_ref(one)))
    {
        return Ok(());
    }
    Err(Refusal::new(
        "mixed_scripts",
        "mixes letters of different scripts (for example Latin and Cyrillic); a name and a \
         title are written in one script",
    ))
}

/// The read-side check: the safety subset. The length limit (and, for a name, not empty: a name of
/// nothing but U+0020 is empty, because no page could show it), and none of a control, format,
/// private-use or surrogate code point, a default-ignorable one, a space other than U+0020, or `<`
/// and `>`. Unassigned code points pass: whether one is assigned depends on the Unicode tables of
/// the binary, and a value stored by a newer binary must not fall back on an older one. A space at
/// the start or the end of a text passes (it is no danger on a page). On failure the answer is one
/// of the fixed words of [`reason`].
pub fn safe(kind: Kind, text: &str) -> Result<(), &'static str> {
    let max = kind.max_chars();
    let mut n = 0usize;
    for c in text.chars() {
        n += 1;
        if n > max {
            return Err(reason::TOO_LONG);
        }
        if c == '<' || c == '>' || default_ignorable(c) {
            return Err(reason::UNSAFE_CHARACTER);
        }
        match category(c) {
            Gc::Control
            | Gc::Format
            | Gc::PrivateUse
            | Gc::Surrogate
            | Gc::LineSeparator
            | Gc::ParagraphSeparator => return Err(reason::UNSAFE_CHARACTER),
            Gc::SpaceSeparator if c != ' ' => return Err(reason::UNSAFE_CHARACTER),
            _ => {}
        }
    }
    if n < kind.min_chars() || (kind == Kind::Name && text.chars().all(|c| c == ' ')) {
        return Err(reason::EMPTY);
    }
    Ok(())
}
