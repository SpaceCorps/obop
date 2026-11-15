//! The Unicode tables the rules need, behind small functions: general category, the
//! Default_Ignorable_Code_Point property, Script_Extensions, and the normalisation forms. They come
//! from the ICU4X crates, compiled into the binary, so every host answers the same.

use icu_normalizer::{ComposingNormalizer, DecomposingNormalizer};
use icu_properties::props::{DefaultIgnorableCodePoint, GeneralCategory, Script};
use icu_properties::script::ScriptWithExtensions;
use icu_properties::{CodePointMapData, CodePointSetData};

/// The general category of a code point.
pub(crate) fn category(c: char) -> GeneralCategory {
    CodePointMapData::<GeneralCategory>::new().get(c)
}

/// Whether the code point has the property Default_Ignorable_Code_Point: the zero-width
/// characters, the soft hyphen, the variation selectors, the tag characters and the Hangul fillers,
/// whatever their category.
pub(crate) fn default_ignorable(c: char) -> bool {
    CodePointSetData::new::<DefaultIgnorableCodePoint>().contains(c)
}

pub(crate) fn is_letter(g: GeneralCategory) -> bool {
    matches!(
        g,
        GeneralCategory::UppercaseLetter
            | GeneralCategory::LowercaseLetter
            | GeneralCategory::TitlecaseLetter
            | GeneralCategory::ModifierLetter
            | GeneralCategory::OtherLetter
    )
}

pub(crate) fn is_mark(g: GeneralCategory) -> bool {
    matches!(
        g,
        GeneralCategory::NonspacingMark
            | GeneralCategory::SpacingMark
            | GeneralCategory::EnclosingMark
    )
}

pub(crate) fn is_punctuation(g: GeneralCategory) -> bool {
    matches!(
        g,
        GeneralCategory::ConnectorPunctuation
            | GeneralCategory::DashPunctuation
            | GeneralCategory::OpenPunctuation
            | GeneralCategory::ClosePunctuation
            | GeneralCategory::InitialPunctuation
            | GeneralCategory::FinalPunctuation
            | GeneralCategory::OtherPunctuation
    )
}

/// The scripts of a code point by Script_Extensions, leaving out Common, Inherited and Unknown: a
/// letter that has none of its own has no script of its own (the mathematical alphanumerics, the
/// modifier letter U+02BB).
pub(crate) fn real_scripts(c: char) -> Vec<Script> {
    ScriptWithExtensions::new()
        .get_script_extensions_val(c)
        .iter()
        .filter(|s| !matches!(*s, Script::Common | Script::Inherited | Script::Unknown))
        .collect()
}

pub(crate) fn nfc(s: &str) -> String {
    ComposingNormalizer::new_nfc().normalize(s).into_owned()
}

pub(crate) fn nfkc(s: &str) -> String {
    ComposingNormalizer::new_nfkc().normalize(s).into_owned()
}

pub(crate) fn nfkd(s: &str) -> String {
    DecomposingNormalizer::new_nfkd().normalize(s).into_owned()
}
