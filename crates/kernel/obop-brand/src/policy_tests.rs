//! The policy table of docs/spec/branding.md, section 13 ("S1", the first bullet): valid and refused
//! names, titles and messages.

use crate::policy::{Kind, MESSAGE_MAX, NAME_MAX, TITLE_MAX, clean, reason, safe};
use crate::testutil::{Rng, text};

fn accepted(kind: Kind, input: &str) -> String {
    clean(kind, input).unwrap_or_else(|r| panic!("{kind:?} {input:?} was refused: {r:?}"))
}

fn refused(kind: Kind, input: &str) -> &'static str {
    match clean(kind, input) {
        Ok(t) => panic!("{kind:?} {input:?} was accepted as {t:?}"),
        Err(r) => r.code,
    }
}

const NAME: Kind = Kind::Name;
const TITLE: Kind = Kind::Title;
const MESSAGE: Kind = Kind::Message;

// ---- Characters that are refused whatever the category ----------------------------------------------

#[test]
fn invisible_and_format_characters_are_refused_in_all_three_kinds() {
    for (label, bad) in [
        ("a bidirectional override", "\u{202E}"),
        ("a zero-width space", "\u{200B}"),
        ("a zero-width joiner", "\u{200D}"),
        ("a zero-width non-joiner", "\u{200C}"),
        ("a tag character", "\u{E0041}"),
        ("the language tag", "\u{E0001}"),
        ("a variation selector", "\u{FE0F}"),
        ("the Hangul filler", "\u{3164}"),
        ("the halfwidth Hangul filler", "\u{FFA0}"),
        ("a soft hyphen", "\u{AD}"),
        ("a word joiner", "\u{2060}"),
        ("a byte order mark", "\u{FEFF}"),
    ] {
        for kind in [NAME, TITLE, MESSAGE] {
            let text = format!("Zed{bad}Co");
            assert_eq!(
                refused(kind, &text),
                "invisible_character",
                "{label} in {kind:?}"
            );
        }
    }
    // A variation selector is a MARK that follows a letter, and the Hangul filler is a LETTER; the
    // property is tested before the category, so both are stopped as invisible.
    assert_eq!(refused(NAME, "Zed\u{FE0F}"), "invisible_character");
    assert_eq!(refused(NAME, "Zed\u{3164}"), "invisible_character");
}

#[test]
fn controls_spaces_and_private_use_are_refused() {
    for kind in [NAME, TITLE, MESSAGE] {
        assert_eq!(refused(kind, "Zed\tCo"), "control_character");
        assert_eq!(refused(kind, "Zed\nCo"), "control_character");
        assert_eq!(refused(kind, "Zed\u{0}Co"), "control_character");
        assert_eq!(refused(kind, "Zed\u{7F}Co"), "control_character");
        assert_eq!(
            refused(kind, "Zed\u{A0}Co"),
            "space_character",
            "no-break space"
        );
        assert_eq!(
            refused(kind, "Zed\u{3000}Co"),
            "space_character",
            "ideographic space"
        );
        assert_eq!(
            refused(kind, "Zed\u{2003}Co"),
            "space_character",
            "em space"
        );
        assert_eq!(
            refused(kind, "Zed\u{2028}Co"),
            "space_character",
            "line separator"
        );
        assert_eq!(refused(kind, "Zed\u{E000}Co"), "private_use");
        assert_eq!(refused(kind, "Zed\u{0378}Co"), "unassigned");
    }
}

#[test]
fn only_a_single_ascii_space_between_words_is_allowed() {
    assert_eq!(accepted(NAME, "Zed Co"), "Zed Co");
    assert_eq!(refused(NAME, "Zed  Co"), "space_position", "doubled");
    assert_eq!(refused(NAME, "Zed Co "), "space_position", "trailing");
    assert_eq!(refused(MESSAGE, "Zed  Co"), "space_position");
    assert_eq!(
        refused(MESSAGE, " Zed"),
        "space_position",
        "leading in a message"
    );
    assert_eq!(refused(MESSAGE, "Zed "), "space_position");
    // A name and a title start with a letter or a digit.
    assert_eq!(refused(NAME, " Zed"), "must_start_with_letter_or_digit");
}

#[test]
fn markup_is_refused_everywhere() {
    for kind in [NAME, TITLE, MESSAGE] {
        assert_eq!(refused(kind, "<b>Zed</b>"), "markup");
        assert_eq!(refused(kind, "a>b"), "markup");
        assert_eq!(refused(kind, "a<b"), "markup");
    }
}

// ---- The forms of a link, an address and a phone number ---------------------------------------------

#[test]
fn the_forms_of_a_link_and_an_address_are_refused() {
    for bad in [
        "a@b.c",
        "https://x",
        "http:x",
        "Zed/Co",
        "Zed\\Co",
        "Zed:Co",
        "shop.example",
        "example.com",
        "mohv.se",
        "v2.beta",
        "Visit mohv.se today",
    ] {
        for kind in [NAME, TITLE, MESSAGE] {
            refused(kind, bad);
        }
    }
    assert_eq!(refused(MESSAGE, "visit shop.example"), "host_like");
    assert_eq!(refused(MESSAGE, "write to a@b.c"), "link_character");
    assert_eq!(refused(MESSAGE, "see https://x"), "link_character");
    assert_eq!(refused(NAME, "shop.example"), "host_like");
    assert_eq!(refused(NAME, "example.com"), "host_like");
    assert_eq!(refused(NAME, "mohv.se"), "host_like");
    assert_eq!(
        refused(NAME, "v2.beta"),
        "host_like",
        "a letter in one group is enough"
    );
    assert_eq!(
        refused(MESSAGE, "Zed: open 8-17"),
        "link_character",
        "a colon is a lead-in"
    );
    assert_eq!(refused(MESSAGE, "open 24/7"), "link_character");
    assert_eq!(refused(MESSAGE, "path\\to"), "link_character");
}

#[test]
fn the_fullwidth_forms_are_refused_like_their_ascii_originals() {
    assert_eq!(refused(MESSAGE, "ｓｈｏｐ．ｅｘａｍｐｌｅ"), "host_like");
    assert_eq!(
        refused(NAME, "ｓｈｏｐ．ｅｘａｍｐｌｅ"),
        "punctuation",
        "also in a name"
    );
    assert_eq!(refused(MESSAGE, "ｈｔｔｐ：／／ｘ"), "link_character");
    assert_eq!(refused(MESSAGE, "ａ＠ｂ．ｃ"), "link_character");
    // Fullwidth letters alone are letters, and normal.
    assert_eq!(accepted(MESSAGE, "ＭＯＨＶ"), "ＭＯＨＶ");
}

#[test]
fn dates_and_times_written_with_dots_pass() {
    for ok in [
        "24.12.2026",
        "03.11.2026",
        "10.30",
        "klo 10.30",
        "Mon-Fri 8.30-17.00",
        "Open 8.30",
    ] {
        assert_eq!(accepted(MESSAGE, ok), ok);
    }
    // Hosts with a one-letter part, and a dot that ends a run, are not hosts to the lint.
    assert_eq!(accepted(MESSAGE, "a.io"), "a.io");
    // What the lint also refuses and is not (honest limits of the spec): a number with a unit after
    // the dot.
    assert_eq!(refused(MESSAGE, "weighs 10.5kg"), "host_like");
}

#[test]
fn legal_forms_and_abbreviations_pass() {
    for ok in [
        "Inc.",
        "e.V.",
        "z.B.",
        "S.A.",
        "sp. z o.o.",
        "Zed Inc.",
        "Beispiel e.V.",
    ] {
        for kind in [NAME, TITLE, MESSAGE] {
            assert_eq!(accepted(kind, ok), ok, "{kind:?} {ok}");
        }
    }
}

#[test]
fn a_run_of_seven_digits_is_refused_and_six_pass() {
    assert_eq!(accepted(MESSAGE, "123456"), "123456");
    assert_eq!(refused(MESSAGE, "1234567"), "digit_run");
    assert_eq!(refused(MESSAGE, "0123456789"), "digit_run");
    assert_eq!(refused(NAME, "Zed 1234567"), "digit_run");
    assert_eq!(accepted(NAME, "Zed 123456"), "Zed 123456");
    // Two groups of digits with a space between are two runs.
    assert_eq!(accepted(MESSAGE, "123 4567"), "123 4567");
    // Digits of other scripts and the fullwidth digits are digits too.
    assert_eq!(refused(MESSAGE, "٠١٢٣٤٥٦"), "digit_run");
    assert_eq!(refused(MESSAGE, "１２３４５６７"), "digit_run");
    // A combining mark cannot break a run (the lint looks at the text without marks).
    assert_eq!(refused(MESSAGE, "123\u{0301}4567"), "digit_run");
}

#[test]
fn a_combining_mark_cannot_hide_a_host() {
    assert_eq!(refused(MESSAGE, "mo\u{0301}hv.se"), "host_like");
    assert_eq!(
        refused(MESSAGE, "x\u{0301}\u{0302}\u{0303}y.com"),
        "host_like"
    );
}

// ---- Letters, marks, digits, punctuation --------------------------------------------------------------

#[test]
fn punctuation_is_the_listed_ascii_in_a_name_and_a_title() {
    for kind in [NAME, TITLE] {
        assert_eq!(
            accepted(kind, "A-B, C&D + E_F (G) H! I'J."),
            "A-B, C&D + E_F (G) H! I'J."
        );
        for bad in [
            "A\"B", "A#B", "A%B", "A*B", "A;B", "A?B", "A[B]", "A’B", "A“B”", "A«B»", "A¡B",
        ] {
            assert_eq!(refused(kind, bad), "punctuation", "{kind:?} {bad}");
        }
        assert_eq!(refused(kind, "A=B"), "character_not_allowed");
        assert_eq!(refused(kind, "A~B"), "character_not_allowed");
        assert_eq!(refused(kind, "A|B"), "character_not_allowed");
        assert_eq!(
            refused(kind, "A$B"),
            "symbol",
            "a currency symbol is for a message"
        );
        assert_eq!(refused(kind, "A€B"), "symbol");
        assert_eq!(refused(kind, "A©B"), "character_not_allowed");
        assert_eq!(refused(kind, "A😀B"), "character_not_allowed");
    }
}

#[test]
fn a_message_may_hold_any_punctuation_currency_and_plus() {
    let ok = "Hello, world! (Mon-Fri) \"quoted\" 'single' «guillemets» ¡hola! 50% off, 20 € or $5 + tax; #1 *";
    assert_eq!(accepted(MESSAGE, ok), ok);
    for bad in [
        "A=B", "A~B", "A|B", "A^B", "A`B", "A©B", "A😀B", "A²B", "Aⅷ",
    ] {
        refused(MESSAGE, bad);
    }
    assert_eq!(refused(MESSAGE, "A=B"), "character_not_allowed");
    // Every mark a name may hold is allowed in a message too.
    for c in "-.,&'+_()!".chars() {
        assert!(clean(MESSAGE, &format!("A{c}B")).is_ok(), "{c}");
        assert!(clean(NAME, &format!("A{c}B")).is_ok(), "{c}");
    }
}

#[test]
fn c_plus_care_is_accepted_as_a_name_and_as_a_message() {
    assert_eq!(accepted(NAME, "C+ Care"), "C+ Care");
    assert_eq!(accepted(TITLE, "C+ Care"), "C+ Care");
    assert_eq!(accepted(MESSAGE, "C+ Care"), "C+ Care");
}

#[test]
fn a_name_and_a_title_start_with_a_letter_or_a_digit() {
    for kind in [NAME, TITLE] {
        for bad in [
            "-Zed",
            "(Zed)",
            "!Zed",
            "&Zed",
            "_Zed",
            "'Zed",
            "+Zed",
            ".Zed",
            "\u{0301}Zed",
        ] {
            assert_eq!(
                refused(kind, bad),
                "must_start_with_letter_or_digit",
                "{kind:?} {bad}"
            );
        }
        assert_eq!(accepted(kind, "1st Zed"), "1st Zed");
    }
    // A message may start with punctuation.
    assert_eq!(accepted(MESSAGE, "(Zed)"), "(Zed)");
    assert_eq!(accepted(MESSAGE, "!Zed"), "!Zed");
    // But a mark needs something to follow.
    assert_eq!(refused(MESSAGE, "\u{0301}Zed"), "mark_position");
    assert_eq!(refused(MESSAGE, "a (\u{0301}b)"), "mark_position");
}

#[test]
fn a_mathematical_alphanumeric_has_no_script_of_its_own() {
    for bad in ["𝐌𝐎𝐇𝐕", "Zed 𝐀", "𝟏", "ℍ"] {
        // The digit U+1D7CF is a decimal digit and passes; the letters do not.
        if bad == "𝟏" {
            assert_eq!(accepted(NAME, bad), bad);
        } else {
            assert_eq!(refused(NAME, bad), "letter_without_script", "{bad}");
            assert_eq!(refused(MESSAGE, bad), "letter_without_script", "{bad}");
        }
    }
    // The Hawaiian okina is a modifier letter of script Common (the spike).
    assert_eq!(refused(NAME, "Hawai\u{02BB}i"), "letter_without_script");
}

#[test]
fn the_prolonged_sound_mark_is_a_letter_of_hiragana_and_katakana() {
    assert_eq!(accepted(NAME, "株式会社ムーヴ"), "株式会社ムーヴ");
    assert_eq!(accepted(NAME, "ラーメン"), "ラーメン");
    assert_eq!(accepted(NAME, "らーめん"), "らーめん");
    // The mark is a letter of Hiragana and Katakana: with Latin it is inside the allowance of the
    // Highly Restrictive level (Latin, Han, Hiragana, Katakana), with Cyrillic it is not.
    assert_eq!(accepted(NAME, "Zedー"), "Zedー");
    assert_eq!(refused(NAME, "Жー"), "mixed_scripts");
}

#[test]
fn one_script_in_a_name_and_a_title_but_not_in_a_message() {
    // Latin and Cyrillic: the Cyrillic О (U+041E) looks like the Latin O.
    let mixed = "M\u{041E}HV";
    for kind in [NAME, TITLE] {
        assert_eq!(refused(kind, mixed), "mixed_scripts");
    }
    assert_eq!(
        accepted(MESSAGE, mixed),
        mixed,
        "the rule is not applied to messages"
    );
    // One script: Cyrillic, Greek, Hebrew, Arabic, Devanagari, Thai, Hangul, Han.
    for ok in [
        "Пример ООО",
        "Παράδειγμα",
        "דוגמה",
        "مثال",
        "उदाहरण",
        "ตัวอย่าง",
        "예시",
        "示例",
    ] {
        assert_eq!(accepted(NAME, ok), ok, "{ok}");
    }
    // The three sets of the Highly Restrictive level.
    assert_eq!(
        accepted(NAME, "Example 株式会社 ムーヴ ひらがな"),
        "Example 株式会社 ムーヴ ひらがな"
    );
    assert_eq!(accepted(NAME, "Example 示例 ㄅㄆ"), "Example 示例 ㄅㄆ");
    assert_eq!(accepted(NAME, "Example 示例 한국"), "Example 示例 한국");
    // Not across the sets: Hangul with Hiragana, Bopomofo with Katakana.
    assert_eq!(refused(NAME, "한국 ひらがな"), "mixed_scripts");
    assert_eq!(refused(NAME, "ㄅㄆ ムーヴ"), "mixed_scripts");
    assert_eq!(refused(NAME, "Zed Ж"), "mixed_scripts");
    // Digits and the listed punctuation are script-neutral.
    assert_eq!(accepted(NAME, "Пример 24 & Ко."), "Пример 24 & Ко.");
}

#[test]
fn combining_marks_three_in_a_row_pass_and_four_are_refused() {
    // x has no precomposed form with an acute accent, so the marks stay marks after NFC.
    let three = "x\u{0301}\u{0302}\u{0303}";
    let four = "x\u{0301}\u{0302}\u{0303}\u{0300}";
    for kind in [NAME, TITLE, MESSAGE] {
        assert_eq!(accepted(kind, three), three);
        assert_eq!(refused(kind, four), "too_many_marks");
    }
    // The count is of a run, so a letter between resets it.
    assert_eq!(
        accepted(NAME, "x\u{0301}\u{0302}\u{0303}x\u{0301}\u{0302}\u{0303}")
            .chars()
            .count(),
        8
    );
    // A mark may follow a digit and another mark, and an Indic vowel sign follows its consonant.
    assert_eq!(accepted(NAME, "1\u{0301}"), "1\u{0301}");
    assert_eq!(accepted(NAME, "कि"), "कि");
    // An enclosing mark is not allowed.
    assert_eq!(refused(NAME, "x\u{20DD}"), "character_not_allowed");
}

#[test]
fn text_is_normalised_to_nfc_before_the_checks_and_stored_that_way() {
    // e + combining acute is é after NFC, and counts as one character.
    assert_eq!(accepted(NAME, "Cafe\u{0301}"), "Caf\u{00E9}");
    // A name of 40 decomposed é is 80 code points before NFC and 40 after: accepted.
    let decomposed = "e\u{0301}".repeat(40);
    assert_eq!(accepted(NAME, &decomposed), "é".repeat(40));
    // Hangul jamo compose into a syllable.
    assert_eq!(accepted(NAME, "\u{1112}\u{1161}\u{11AB}"), "\u{D55C}");
    // Already NFC text is unchanged.
    assert_eq!(accepted(NAME, "Exämple & Söns"), "Exämple & Söns");
}

#[test]
fn non_ascii_letters_are_accepted() {
    for ok in [
        "Ärzte Öl Üben",
        "Łódź",
        "Ærø",
        "Straße",
        "Ñandú",
        "Çağrı",
        "Åland",
        "Žižek",
        "İstanbul",
    ] {
        assert_eq!(accepted(NAME, ok), ok, "{ok}");
    }
}

// ---- Limits -------------------------------------------------------------------------------------------

#[test]
fn the_limits_count_unicode_code_points() {
    assert_eq!(
        accepted(NAME, &"a".repeat(NAME_MAX)).chars().count(),
        NAME_MAX
    );
    assert_eq!(refused(NAME, &"a".repeat(NAME_MAX + 1)), "too_long");
    assert_eq!(
        accepted(TITLE, &"a".repeat(TITLE_MAX)).chars().count(),
        TITLE_MAX
    );
    assert_eq!(refused(TITLE, &"a".repeat(TITLE_MAX + 1)), "too_long");
    assert_eq!(
        accepted(MESSAGE, &"a".repeat(MESSAGE_MAX)).chars().count(),
        MESSAGE_MAX
    );
    assert_eq!(refused(MESSAGE, &"a".repeat(MESSAGE_MAX + 1)), "too_long");
    // Characters, not bytes: 40 characters of 3 bytes each pass.
    assert_eq!(
        accepted(NAME, &"株".repeat(NAME_MAX)).chars().count(),
        NAME_MAX
    );
    assert_eq!(refused(NAME, &"株".repeat(NAME_MAX + 1)), "too_long");
    // A huge input is refused without being read: no table is touched.
    assert_eq!(refused(NAME, &"a".repeat(1_000_000)), "too_long");
}

#[test]
fn an_empty_name_is_refused_and_an_empty_title_or_message_clears_the_field() {
    assert_eq!(refused(NAME, ""), "empty");
    assert_eq!(accepted(TITLE, ""), "");
    assert_eq!(accepted(MESSAGE, ""), "");
}

// ---- The read side: the safety subset -----------------------------------------------------------------

#[test]
fn the_safety_subset_is_smaller_than_the_policy() {
    // What only the full policy refuses is readable: a name stored by an older or a newer binary,
    // or restored from a backup, must not fall back because the policy changed.
    assert_eq!(safe(NAME, "M\u{041E}HV"), Ok(()), "mixed scripts");
    assert_eq!(safe(NAME, "shop.example"), Ok(()), "a host-like run");
    assert_eq!(
        safe(NAME, "Zed: 1234567"),
        Ok(()),
        "a colon and seven digits"
    );
    assert_eq!(safe(NAME, "𝐌𝐎𝐇𝐕"), Ok(()), "mathematical letters");
    assert_eq!(
        safe(NAME, "Zed\u{0378}"),
        Ok(()),
        "an unassigned code point"
    );
    assert_eq!(
        safe(NAME, "x\u{0301}\u{0302}\u{0303}\u{0300}"),
        Ok(()),
        "four marks"
    );
    assert_eq!(
        safe(NAME, "(Zed)  Co "),
        Ok(()),
        "spaces and a start with punctuation"
    );
    assert_eq!(safe(TITLE, ""), Ok(()), "an empty title is unset");
    assert_eq!(safe(MESSAGE, ""), Ok(()));
}

#[test]
fn the_safety_subset_refuses_what_could_do_harm_on_a_page() {
    for bad in [
        "<script>",
        "Zed>",
        "Zed\u{0}",
        "Zed\n",
        "Zed\t",
        "Zed\u{7F}",
        "Zed\u{202E}",
        "Zed\u{200B}",
        "Zed\u{AD}",
        "Zed\u{FE0F}",
        "Zed\u{3164}",
        "Zed\u{E0041}",
        "Zed\u{FFF0}",
        "Zed\u{E0080}",
        "Zed\u{E000}",
        "Zed\u{A0}",
        "Zed\u{3000}",
        "Zed\u{2028}",
        "Zed\u{2029}",
        "\u{FEFF}Zed",
    ] {
        for kind in [NAME, TITLE, MESSAGE] {
            assert_eq!(
                safe(kind, bad),
                Err(reason::UNSAFE_CHARACTER),
                "{kind:?} {bad:?}"
            );
        }
    }
    assert_eq!(safe(NAME, ""), Err(reason::EMPTY));
    // A text of nothing but U+0020 is empty, whichever the kind: a name has none, a title would
    // hide the name, a message would be a blank line. (The write check refuses them too.)
    for blank in [" ", "   ", &" ".repeat(NAME_MAX)] {
        for kind in [NAME, TITLE, MESSAGE] {
            assert_eq!(safe(kind, blank), Err(reason::EMPTY), "{kind:?} {blank:?}");
        }
    }
    // A space between or around letters is not blank, and a too long blank is too long.
    assert_eq!(safe(NAME, "Zed Co"), Ok(()));
    assert_eq!(safe(TITLE, " a"), Ok(()));
    assert_eq!(safe(NAME, &" ".repeat(NAME_MAX + 1)), Err(reason::TOO_LONG));
    assert_eq!(safe(NAME, &"a".repeat(NAME_MAX + 1)), Err(reason::TOO_LONG));
    assert_eq!(
        safe(TITLE, &"a".repeat(TITLE_MAX + 1)),
        Err(reason::TOO_LONG)
    );
    assert_eq!(
        safe(MESSAGE, &"a".repeat(MESSAGE_MAX + 1)),
        Err(reason::TOO_LONG)
    );
    assert_eq!(safe(NAME, &"a".repeat(NAME_MAX)), Ok(()));
    // A refused value never reaches a log through the reason: it is a fixed word.
    assert!(
        [reason::EMPTY, reason::TOO_LONG, reason::UNSAFE_CHARACTER]
            .iter()
            .all(|w| w.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
    );
}

#[test]
fn everything_clean_accepts_the_safety_subset_accepts() {
    let mut rng = Rng::new(0x5EED_0001);
    let mut accepted_any = 0;
    for _ in 0..20_000 {
        let t = text(&mut rng, 12);
        for kind in [NAME, TITLE, MESSAGE] {
            if let Ok(stored) = clean(kind, &t) {
                assert_eq!(safe(kind, &stored), Ok(()), "{kind:?} {t:?}");
                accepted_any += 1;
            }
        }
    }
    assert!(
        accepted_any > 500,
        "the generator should hit accepted texts: {accepted_any}"
    );
}

/// A text of nothing but the space has nothing to show, so on the read side it is as empty as no
/// text: a name falls back to the default name in every page, and a title or a message falls back
/// to unset (the chain goes on to the English title, the name, or no message). A space at the start
/// or the end of a text is no danger and passes. An absent title or message (the empty text) is
/// unset, not a failure.
#[test]
fn a_text_of_nothing_but_spaces_is_empty_and_a_text_with_spaces_around_it_is_not() {
    for blank in [" ", "  ", "    ", &" ".repeat(NAME_MAX)] {
        assert_eq!(safe(NAME, blank), Err(reason::EMPTY), "{blank:?}");
        // The write side never let such a name in.
        assert!(clean(NAME, blank).is_err(), "{blank:?}");
    }
    // The length is checked first, and it is the larger fault.
    assert_eq!(safe(NAME, &" ".repeat(NAME_MAX + 1)), Err(reason::TOO_LONG));
    for padded in [" a", "a ", "  a  ", " . ", " 1 "] {
        assert_eq!(safe(NAME, padded), Ok(()), "{padded:?}");
    }
    // A title and a message are the same when they are there: a blank one is empty, the empty one
    // is not a failure (it is unset), the length is still the larger fault, and padding passes.
    for (kind, max) in [(TITLE, TITLE_MAX), (MESSAGE, MESSAGE_MAX)] {
        assert_eq!(safe(kind, ""), Ok(()), "{kind:?}: unset is no failure");
        for blank in [" ", "   ", &" ".repeat(max)] {
            assert_eq!(safe(kind, blank), Err(reason::EMPTY), "{kind:?} {blank:?}");
            // The write side never let such a text in.
            assert!(clean(kind, blank).is_err(), "{kind:?} {blank:?}");
        }
        assert_eq!(safe(kind, &" ".repeat(max + 1)), Err(reason::TOO_LONG));
        for padded in [" a", "a ", "  a  ", " . ", " 1 "] {
            assert_eq!(safe(kind, padded), Ok(()), "{kind:?} {padded:?}");
        }
        // Only the space makes a text blank: any other white space is refused as unsafe.
        assert_eq!(
            safe(kind, "\u{a0}"),
            Err(reason::UNSAFE_CHARACTER),
            "{kind:?}"
        );
    }
}

// ---- Total: never panics --------------------------------------------------------------------------------

#[test]
fn the_policy_never_panics_on_generated_text() {
    let mut rng = Rng::new(0xB4A9_D001);
    let outcome = std::panic::catch_unwind(move || {
        for _ in 0..30_000 {
            let t = text(&mut rng, 80);
            for kind in [NAME, TITLE, MESSAGE] {
                let _ = clean(kind, &t);
                let _ = safe(kind, &t);
            }
        }
        // Truncated at every length: a prefix of a long text with every nasty character.
        let all: String = crate::testutil::NASTY.iter().collect();
        for end in 0..=all.chars().count() {
            let prefix: String = all.chars().take(end).collect();
            for kind in [NAME, TITLE, MESSAGE] {
                let _ = clean(kind, &prefix);
                let _ = safe(kind, &prefix);
            }
        }
    });
    assert!(outcome.is_ok(), "the policy panicked");
}

// ---- Pinned by value: the limits, the Cf clause, the boundaries of the lints ----------------------------

#[test]
fn the_limits_are_the_numbers_of_the_spec_not_only_their_constants() {
    assert_eq!(
        (NAME_MAX, TITLE_MAX, MESSAGE_MAX, crate::policy::SLUG_MAX),
        (40, 60, 240, 30)
    );
    for (kind, fits, over) in [(NAME, 40, 41), (TITLE, 60, 61), (MESSAGE, 240, 241)] {
        assert_eq!(accepted(kind, &"a".repeat(fits)), "a".repeat(fits));
        assert_eq!(refused(kind, &"a".repeat(over)), "too_long", "{kind:?}");
        assert_eq!(safe(kind, &"a".repeat(fits)), Ok(()), "{kind:?}");
        assert_eq!(
            safe(kind, &"a".repeat(over)),
            Err(reason::TOO_LONG),
            "{kind:?}"
        );
    }
}

#[test]
fn the_reason_words_of_the_read_check_are_fixed() {
    assert_eq!(reason::EMPTY, "empty");
    assert_eq!(reason::TOO_LONG, "too_long");
    assert_eq!(reason::UNSAFE_CHARACTER, "unsafe_character");
    assert_eq!(reason::INVALID_SLUG, "invalid_slug");
    assert_eq!(reason::INVALID_ARTWORK, "invalid_artwork");
}

#[test]
fn format_characters_that_are_not_default_ignorable_are_refused_in_both_directions() {
    use crate::testutil::format_but_not_ignorable;
    use crate::unicode::{category, default_ignorable};
    use icu_properties::props::GeneralCategory as Gc;
    let cf = format_but_not_ignorable();
    // The characters the review found: category Cf, and not default ignorable.
    for c in ['\u{0600}', '\u{06DD}', '\u{08E2}', '\u{FFF9}'] {
        assert_eq!(category(c), Gc::Format, "U+{:04X}", c as u32);
        assert!(!default_ignorable(c), "U+{:04X}", c as u32);
        assert!(cf.contains(&c));
    }
    assert!(cf.len() >= 20, "{} characters", cf.len());
    for c in cf {
        let text = format!("Zed{c}Co");
        for kind in [NAME, TITLE, MESSAGE] {
            assert_eq!(
                refused(kind, &text),
                "invisible_character",
                "{kind:?} U+{:04X}",
                c as u32
            );
            assert_eq!(
                safe(kind, &text),
                Err(reason::UNSAFE_CHARACTER),
                "{kind:?} U+{:04X}",
                c as u32
            );
        }
    }
}

#[test]
fn a_combining_mark_after_a_space_is_refused() {
    // x has no precomposed form with an acute accent, so the mark stays a mark after NFC.
    for kind in [NAME, TITLE, MESSAGE] {
        assert_eq!(accepted(kind, "x\u{0301}b"), "x\u{0301}b");
        assert_eq!(refused(kind, "x \u{0301}b"), "mark_position", "{kind:?}");
    }
}

#[test]
fn a_host_needs_two_letters_or_digits_after_the_dot_as_well_as_before() {
    for kind in [NAME, TITLE, MESSAGE] {
        // Two before, one after: not a host. One before, two after: not a host either.
        for ok in ["mohv.s", "ab.c", "x.yz", "visit mohv.s", "a.io"] {
            assert_eq!(accepted(kind, ok), ok, "{kind:?} {ok}");
        }
        // Two and two, with a letter in either group: a host.
        for bad in ["ab.cd", "mohv.se", "12.ab", "ab.12"] {
            assert_eq!(refused(kind, bad), "host_like", "{kind:?} {bad}");
        }
    }
    // Two and two, digits only: a date or a time.
    assert_eq!(accepted(MESSAGE, "12.30"), "12.30");
}

#[test]
fn a_name_of_40_decomposed_hangul_syllables_is_measured_after_normalising() {
    // Each syllable is three jamo before NFC: 120 code points, 40 after.
    let jamo = "\u{1112}\u{1161}\u{11AB}".repeat(40);
    assert_eq!(jamo.chars().count(), 120);
    assert_eq!(accepted(NAME, &jamo), "\u{D55C}".repeat(40));
    // 41 of them are 41 after NFC: too long.
    assert_eq!(
        refused(NAME, &"\u{1112}\u{1161}\u{11AB}".repeat(41)),
        "too_long"
    );
}
