//! The Unicode spike. The spec (docs/spec/branding.md, section 2) states some facts "as
//! remembered and not re-fetched"; these are the first tests of the crate, and they hold the real
//! answers of the tables compiled into this binary (ICU4X 2.3.0, from `Cargo.lock`). Where a fact is
//! different from the spec's sentence, the test says what it is and the policy follows the real
//! property.

use crate::unicode::{category, default_ignorable};
use icu_properties::props::{GeneralCategory as Gc, Script};
use icu_properties::script::ScriptWithExtensions;

fn scripts_of(c: char) -> Vec<Script> {
    ScriptWithExtensions::new()
        .get_script_extensions_val(c)
        .iter()
        .collect()
}

/// "the Japanese prolonged sound mark U+30FC (category `Lm`, extensions Hiragana and Katakana)".
#[test]
fn spike_u30fc_is_a_modifier_letter_whose_extensions_are_hiragana_and_katakana() {
    assert_eq!(category('\u{30FC}'), Gc::ModifierLetter);
    assert!(!default_ignorable('\u{30FC}'));
    let swe = ScriptWithExtensions::new();
    // Its own script is Common, which is why the policy reads Script_Extensions.
    assert_eq!(swe.get_script_val('\u{30FC}'), Script::Common);
    let ext = scripts_of('\u{30FC}');
    assert_eq!(ext.len(), 2, "{ext:?}");
    assert!(ext.contains(&Script::Hiragana) && ext.contains(&Script::Katakana));
    assert!(swe.has_script('\u{30FC}', Script::Katakana));
}

/// "the tag characters U+E0000 to U+E007F and the Hangul filler ... and the zero-width joiners are
/// Default_Ignorable_Code_Point".
#[test]
fn spike_the_tag_characters_the_hangul_filler_and_the_joiners_are_default_ignorable() {
    // U+E0001 LANGUAGE TAG is a format character.
    assert!(default_ignorable('\u{E0001}'));
    assert_eq!(category('\u{E0001}'), Gc::Format);
    for tag in 0xE0000..=0xE007F {
        // Not every code point of the block is assigned, and the property covers the whole block.
        assert!(default_ignorable(char::from_u32(tag).unwrap()), "{tag:X}");
    }
    // U+3164 HANGUL FILLER is a LETTER (Lo) and still default ignorable: the policy must test the
    // property before the category, as the spec says ("whatever the category").
    assert!(default_ignorable('\u{3164}'));
    assert_eq!(category('\u{3164}'), Gc::OtherLetter);
    assert!(
        default_ignorable('\u{FFA0}'),
        "the halfwidth Hangul filler too"
    );
    // The joiners are format characters (Cf) of script Inherited.
    for joiner in ['\u{200C}', '\u{200D}'] {
        assert!(default_ignorable(joiner), "{joiner:?}");
        assert_eq!(category(joiner), Gc::Format);
    }
    // The soft hyphen, the word joiner and the variation selectors; U+FE0F is a nonspacing MARK,
    // so the property, not the category, is what stops it.
    for c in ['\u{00AD}', '\u{2060}', '\u{FE0F}', '\u{FE0E}', '\u{034F}'] {
        assert!(default_ignorable(c), "{c:?}");
    }
    assert_eq!(category('\u{FE0F}'), Gc::NonspacingMark);
    // The property also holds some UNASSIGNED code points; the ones `clean_text` of the OAuth
    // registration lets through (U+FFF0, U+E0080) are among them.
    for c in ['\u{FFF0}', '\u{E0080}'] {
        assert!(default_ignorable(c), "{c:?}");
        assert_eq!(category(c), Gc::Unassigned);
    }
    // And ordinary letters and the ASCII space are not.
    assert!(!default_ignorable('a') && !default_ignorable(' '));
}

/// "a modifier letter of script Common such as U+02BB (the Hawaiian okina)": no script of its own.
#[test]
fn spike_u02bb_is_a_modifier_letter_of_script_common_with_no_extensions() {
    assert_eq!(category('\u{02BB}'), Gc::ModifierLetter);
    let swe = ScriptWithExtensions::new();
    assert_eq!(swe.get_script_val('\u{02BB}'), Script::Common);
    assert_eq!(scripts_of('\u{02BB}'), vec![Script::Common]);
    // The neighbour U+02BC does have extensions (several scripts use it as the apostrophe), so a
    // name with U+02BC is a different case from U+02BB; only U+02BB is refused as having no script.
    assert!(scripts_of('\u{02BC}').len() > 1);
    // The mathematical alphanumerics have script Common and no extensions: out.
    assert_eq!(category('\u{1D400}'), Gc::UppercaseLetter);
    assert_eq!(scripts_of('\u{1D400}'), vec![Script::Common]);
}
