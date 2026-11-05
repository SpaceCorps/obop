//! The brand as a page reads it: the defaults, the fallback chain, the neutral switch and the
//! comparison with a client's name (docs/spec/branding.md, sections 2, 4 and 10; section 13: the
//! fallback chain, and the consent bullet's fold).

use crate::testutil::{Rng, text};
use crate::{Brand, DEFAULT_NAME, LANGS, fold, lang_index};

#[test]
fn the_default_brand_is_obop_with_nothing_else_and_the_neutral_wording_off() {
    let b = Brand::default();
    assert_eq!(DEFAULT_NAME, "obop");
    assert!(!b.is_set() && !b.neutral());
    assert_eq!(b.name(), "obop");
    assert_eq!(b.slug(), "obop");
    assert_eq!(b.stored_slug(), "");
    for lang in LANGS {
        assert_eq!(b.title(lang), "obop");
        assert_eq!(b.message(lang), None);
        assert_eq!(b.stored_title(lang), "");
    }
    assert_eq!(b.titles().count() + b.messages().count(), 0);
}

#[test]
fn a_brand_that_is_set_switches_the_neutral_wording_on_in_every_language() {
    let b = Brand::new("MOHV");
    assert!(b.is_set() && b.neutral());
    // Even a row that names obop is a set brand: nothing it holds is dropped.
    assert!(Brand::new("obop").neutral());
    assert_eq!(b.name(), "MOHV");
    assert_eq!(b.slug(), "mohv", "derived from the name");
    assert_eq!(
        b.clone().with_slug("moh").slug(),
        "moh",
        "an explicit slug wins"
    );
    assert_eq!(b.clone().with_slug("moh").stored_slug(), "moh");
}

#[test]
fn the_langs_are_the_seven_of_the_pages_in_their_order() {
    assert_eq!(LANGS, ["en", "de", "fr", "pl", "fi", "sv", "es"]);
    assert_eq!(lang_index("en"), Some(0));
    assert_eq!(lang_index("es"), Some(6));
    assert_eq!(lang_index("nl"), None);
    assert_eq!(lang_index("EN"), None);
    assert_eq!(lang_index(""), None);
}

#[test]
fn a_swedish_page_with_only_message_en_shows_the_english_message() {
    let mut b = Brand::new("MOHV");
    assert!(b.set_message("en", "Welcome to MOHV."));
    assert_eq!(b.message("sv"), Some("Welcome to MOHV."));
    assert_eq!(b.message("en"), Some("Welcome to MOHV."));
    // The message of the page's own language wins over the English one.
    assert!(b.set_message("sv", "Välkommen."));
    assert_eq!(b.message("sv"), Some("Välkommen."));
    assert_eq!(b.message("de"), Some("Welcome to MOHV."));
    // No message at all is none; a language that is not one of the seven takes the English chain.
    assert_eq!(Brand::new("MOHV").message("sv"), None);
    assert_eq!(b.message("nl"), Some("Welcome to MOHV."));
    assert_eq!(b.message(""), Some("Welcome to MOHV."));
}

#[test]
fn a_swedish_page_with_no_title_sv_shows_title_en_then_the_name() {
    let mut b = Brand::new("MOHV");
    assert_eq!(b.title("sv"), "MOHV", "no title: the name");
    assert!(b.set_title("en", "MOHV Mileage"));
    assert_eq!(b.title("sv"), "MOHV Mileage", "title_sv empty: title_en");
    assert!(b.set_title("sv", "MOHV Körjournal"));
    assert_eq!(b.title("sv"), "MOHV Körjournal");
    assert_eq!(b.title("de"), "MOHV Mileage");
    assert_eq!(b.title("xx"), "MOHV Mileage");
    // Clearing a field is an empty string, and the chain moves on.
    assert!(b.set_title("sv", ""));
    assert_eq!(b.title("sv"), "MOHV Mileage");
    assert!(b.set_title("en", ""));
    assert_eq!(b.title("sv"), "MOHV");
}

#[test]
fn a_language_outside_the_seven_sets_nothing() {
    let mut b = Brand::new("MOHV");
    assert!(!b.set_title("nl", "x"));
    assert!(!b.set_message("nl", "x"));
    assert_eq!(b, Brand::new("MOHV"));
}

#[test]
fn the_titles_and_messages_that_are_set_come_in_the_order_of_langs() {
    let mut b = Brand::new("MOHV");
    b.set_title("sv", "Sv");
    b.set_title("de", "De");
    b.set_message("es", "Es");
    b.set_message("en", "En");
    assert_eq!(b.titles().collect::<Vec<_>>(), [("de", "De"), ("sv", "Sv")]);
    assert_eq!(
        b.messages().collect::<Vec<_>>(),
        [("en", "En"), ("es", "Es")]
    );
}

// ---- The comparison with a client's name ----------------------------------------------------------------

fn zed() -> Brand {
    Brand::new("Zed")
}

#[test]
fn a_client_named_like_the_brand_matches() {
    let b = zed();
    for client in [
        "Zed",
        "zed",
        "ZED",
        "zEd",
        "ＺＥＤ",
        "ｚｅｄ",
        "Z e d",
        " Zed ",
        "Zed\n",
    ] {
        assert!(b.matches_client(client), "{client:?}");
    }
}

#[test]
fn invisible_and_format_characters_and_white_space_do_not_defeat_the_comparison() {
    let b = zed();
    for (label, c) in [
        ("a zero-width joiner", '\u{200D}'),
        ("a zero-width non-joiner", '\u{200C}'),
        ("a soft hyphen", '\u{00AD}'),
        ("a variation selector 16", '\u{FE0F}'),
        ("a no-break space", '\u{00A0}'),
        ("a zero-width space", '\u{200B}'),
        ("a word joiner", '\u{2060}'),
        ("a bidirectional override", '\u{202E}'),
        ("a byte order mark", '\u{FEFF}'),
        ("an ideographic space", '\u{3000}'),
        // The characters that `clean_text` of the registration lets through (it stops at U+FE0E).
        ("U+FFF0", '\u{FFF0}'),
        ("U+E0080", '\u{E0080}'),
        ("a tag character", '\u{E0041}'),
        ("the Hangul filler", '\u{3164}'),
    ] {
        let between = format!("Z{c}e{c}d");
        assert!(b.matches_client(&between), "{label} between the letters");
        let around = format!("{c}Zed{c}");
        assert!(b.matches_client(&around), "{label} around the name");
    }
}

#[test]
fn the_fold_is_applied_to_both_sides() {
    // The brand's own name is folded too: a name stored with a fullwidth letter or compatibility
    // form still matches the plain client name.
    assert!(Brand::new("ＭＯＨＶ").matches_client("mohv"));
    assert!(Brand::new("MOHV").matches_client("ＭＯＨＶ"));
    assert!(Brand::new("e\u{FB01}x").matches_client("efix"));
}

#[test]
fn a_client_named_like_the_slug_or_a_set_title_matches() {
    let mut b = Brand::new("Zed Mileage Company");
    b = b.with_slug("zed");
    assert!(b.matches_client("zed"), "the slug");
    assert!(b.matches_client("Zed Mileage Company"), "the name");
    // The derived slug counts too.
    assert!(Brand::new("Zed Co").matches_client("zed-co"));
    // A title in any language, whatever language the page is in.
    let mut b = Brand::new("Zed");
    b.set_title("sv", "Zed Mileage");
    assert!(b.matches_client("Zed Mileage"));
    assert!(b.matches_client("ZED\u{200D} MILEAGE"));
    assert!(!b.matches_client("Zed Körjournal"));
}

#[test]
fn a_client_that_merely_contains_the_brand_does_not_match() {
    let b = zed();
    for client in [
        "Zed Support",
        "Acme",
        "Zedd",
        "Zed.",
        "Z-e-d",
        "",
        " ",
        "\u{200B}",
        "Ze",
        "ZeD2",
    ] {
        assert!(!b.matches_client(client), "{client:?}");
    }
    // Look-alikes in another script are not equal either (a stated limit): the Cyrillic Е.
    assert!(!Brand::new("Zed").matches_client("Z\u{0435}d"));
}

#[test]
fn with_no_brand_set_no_client_matches() {
    let b = Brand::default();
    assert!(!b.matches_client("obop"));
    assert!(!b.matches_client("OBOP"));
    assert!(!b.matches_client(""));
}

#[test]
fn the_fold_removes_ignorable_and_white_space_then_nfkc_then_lowercase() {
    assert_eq!(fold("Zed"), "zed");
    assert_eq!(fold("Ｚ\u{200B}e d"), "zed");
    assert_eq!(fold("  Mo\u{AD}HV\u{A0}"), "mohv");
    // The characters clean_text keeps: U+FE0F (a variation selector), U+FFF0 and U+E0080 (unassigned,
    // default ignorable).
    assert_eq!(fold("A\u{FE0F}B\u{FFF0}C\u{E0080}"), "abc");
    // No punctuation is ignored.
    assert_eq!(fold("M-O-H-V"), "m-o-h-v");
    assert_eq!(fold("MOHV."), "mohv.");
    // A stated approximation of case folding: ß stays, and the final sigma is not folded.
    assert_eq!(fold("Straße"), "straße");
    assert_eq!(
        fold("\u{039F}\u{0394}\u{039F}\u{03A3}"),
        "\u{03BF}\u{03B4}\u{03BF}\u{03C2}"
    );
    assert_eq!(fold(""), "");
}

#[test]
fn the_brand_functions_never_panic_on_generated_text() {
    let mut rng = Rng::new(0xB4A9_D002);
    let outcome = std::panic::catch_unwind(move || {
        for _ in 0..10_000 {
            let name = text(&mut rng, 30);
            let mut b = Brand::new(name.clone());
            b.set_title("sv", text(&mut rng, 30));
            b.set_message("en", text(&mut rng, 30));
            let client = text(&mut rng, 30);
            let _ = (
                b.slug(),
                b.title("sv"),
                b.message("de"),
                b.matches_client(&client),
            );
            let _ = fold(&client);
            // The brand compared with its own name never fails to match (when the fold leaves a
            // name).
            if !fold(&name).is_empty() {
                assert!(b.matches_client(&name), "{name:?}");
            }
        }
    });
    assert!(outcome.is_ok());
}
