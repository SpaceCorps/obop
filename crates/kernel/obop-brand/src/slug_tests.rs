//! Slug derivation and the slug pattern (docs/spec/branding.md, section 13, the slug bullet).

use crate::slug::{FALLBACK_SLUG, check, derive, is_valid};
use crate::testutil::{Rng, text};

#[test]
fn a_name_with_no_latin_letter_or_digit_gives_instance() {
    assert_eq!(derive("Пример"), "instance");
    assert_eq!(derive("株式会社ムーヴ"), "instance");
    assert_eq!(derive("مثال"), "instance");
    assert_eq!(derive("!!! ---"), "instance");
    assert_eq!(derive(""), FALLBACK_SLUG);
    // It never falls back to obop.
    assert_ne!(derive("Пример"), "obop");
}

#[test]
fn the_slug_of_a_name_joins_the_runs_of_a_z_and_0_9() {
    assert_eq!(derive("MOHV"), "mohv");
    assert_eq!(derive("Exämple & Söns"), "example-sons");
    assert_eq!(derive("Café 1"), "cafe-1");
    assert_eq!(derive("C+ Care"), "c-care");
    assert_eq!(
        derive("Sportverein Beispiel e.V."),
        "sportverein-beispiel-e-v"
    );
    assert_eq!(derive("Beispiel sp. z o.o."), "beispiel-sp-z-o-o");
    assert_eq!(derive("  --Zed--  "), "zed");
    // Compatibility forms fold to ASCII: fullwidth letters, the ligature fi, the Kelvin sign.
    assert_eq!(derive("ＭＯＨＶ"), "mohv");
    assert_eq!(derive("ﬁnn"), "finn");
    assert_eq!(derive("\u{212A}ilo"), "kilo");
    // Marks are dropped: a base letter with three marks is one letter.
    assert_eq!(derive("x\u{0301}\u{0302}\u{0303}yz"), "xyz");
    // The effective name of the default.
    assert_eq!(derive("obop"), "obop");
}

#[test]
fn only_a_z_and_0_9_survive_so_some_letters_split_the_name() {
    // The quirk of the spec's section 14: a letter that does not decompose to ASCII separates.
    assert_eq!(derive("Straße"), "stra-e");
    assert_eq!(derive("Ærø"), "r");
    assert_eq!(derive("Łódź"), "odz");
    assert_eq!(derive("Øresund"), "resund");
    assert_eq!(derive("Straße 7"), "stra-e-7");
}

#[test]
fn a_name_that_starts_with_a_digit_gets_i_in_front() {
    assert_eq!(derive("24 Stunden"), "i-24-stunden");
    assert_eq!(derive("7"), "i-7");
    assert_eq!(derive("１２ Zed"), "i-12-zed");
    // The prefix counts to the limit of 30.
    let slug = derive(&format!("1{}", "a".repeat(60)));
    assert_eq!(slug.chars().count(), 30);
    assert!(slug.starts_with("i-1aaa"));
}

#[test]
fn a_long_name_is_cut_to_30_characters_with_no_trailing_hyphen() {
    let forty = "abcdefghij".repeat(4);
    assert_eq!(derive(&forty), "abcdefghijabcdefghijabcdefghij");
    // The cut lands right after a hyphen: it is trimmed.
    let name = format!("{} {}", "a".repeat(29), "b".repeat(10));
    let slug = derive(&name);
    assert_eq!(slug, "a".repeat(29));
    assert!(!slug.ends_with('-'));
    // A cut after two words.
    let slug = derive("alpha beta gamma delta epsilon zeta eta theta");
    assert_eq!(slug, "alpha-beta-gamma-delta-epsilon");
    assert!(slug.chars().count() <= 30);
}

#[test]
fn every_derived_slug_is_a_valid_slug() {
    for name in [
        "MOHV",
        "Exämple & Söns",
        "Straße",
        "Ærø",
        "Łódź",
        "Пример",
        "24 Stunden",
        "7",
        "",
        "---",
        "株式会社ムーヴ",
        "C+ Care",
        "a",
    ] {
        let slug = derive(name);
        assert!(is_valid(&slug), "{name:?} gave {slug:?}");
    }
    let mut rng = Rng::new(0x5106_0001);
    for _ in 0..5_000 {
        let name = text(&mut rng, 60);
        let slug = derive(&name);
        assert!(is_valid(&slug), "{name:?} gave {slug:?}");
    }
}

#[test]
fn a_given_slug_follows_the_pattern() {
    for ok in [
        "mohv",
        "a",
        "mohv-se",
        "a1",
        "a--b",
        "x9-9",
        &"a".repeat(30),
    ] {
        assert!(check(ok).is_ok(), "{ok}");
    }
    let code = |s: &str| check(s).unwrap_err().code;
    assert_eq!(code("9x"), "must_start_with_letter", "starts with a digit");
    assert_eq!(code("-x"), "must_start_with_letter");
    assert_eq!(code("foo-"), "trailing_hyphen", "ends in a hyphen");
    assert_eq!(code("Foo"), "character_not_allowed", "upper case");
    assert_eq!(code("foo/bar"), "character_not_allowed");
    assert_eq!(code("foo bar"), "character_not_allowed");
    assert_eq!(code("foo_bar"), "character_not_allowed");
    assert_eq!(code("fo\u{0301}o"), "character_not_allowed");
    assert_eq!(code("foö"), "character_not_allowed");
    assert_eq!(code(&"a".repeat(31)), "too_long");
    assert_eq!(code(""), "empty");
    assert!(!is_valid("9x") && !is_valid("foo-") && !is_valid("") && is_valid("mohv"));
}

#[test]
fn the_slug_functions_never_panic() {
    let mut rng = Rng::new(0x5106_0002);
    let outcome = std::panic::catch_unwind(move || {
        for _ in 0..20_000 {
            let t = text(&mut rng, 100);
            let _ = derive(&t);
            let _ = check(&t);
        }
    });
    assert!(outcome.is_ok());
}
