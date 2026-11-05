//! Golden names: texts that `configure` must keep accepting in every release
//! (docs/spec/branding.md, section 13, "Golden names"). A release that tightens the character policy
//! fails here, instead of refusing a customer's own name on its next save. The lists are read by a
//! human: add a line when a real customer needs one, and never remove or edit a line to make a
//! change pass. Every name here is made up.

use crate::policy::{Kind, clean, safe};
use crate::slug;

/// Names, one per line. Latin with accents, legal forms with dots, digits, ampersands, other
/// scripts, and the three sets of the Highly Restrictive level.
const GOLDEN_NAMES: &[&str] = &[
    // The first customer's own name, as the owner gave it.
    "MOHV",
    // Accents and an ampersand.
    "Exämple & Söns",
    "Café 1",
    "C+ Care",
    "Yay! Tea",
    "Beispiel (Nord) GmbH",
    "Musterstadt-Bau GmbH & Co. KG",
    "my_instance",
    "Zeta 7 Studio",
    "24 Stunden Dienst",
    // Legal forms and abbreviations with dots.
    "Sportverein Beispiel e.V.",
    "Beispiel sp. z o.o.",
    "Przykładowa Firma sp. z o.o.",
    "Ejemplo Norte S.L.",
    "Exemple Nord S.A.",
    "Société d'Exemple SARL",
    "Example Inc.",
    // Finnish and Swedish.
    "Esimerkki Kahvila Oy",
    "Kalatukku Länsi Oy",
    "Päiväkoti Auringonkukka ry",
    "Exempelbolaget Östra AB",
    "Bageri Äpplet & Söner AB",
    // One script that is not Latin.
    "Пример Бюро",
    "Παράδειγμα Α.Ε.",
    "דוגמה בע'מ",
    "مثال",
    "उदाहरण",
    "ตัวอย่าง",
    "예시 주식회사",
    "示例公司",
    // Japanese: Han, Hiragana, Katakana and the prolonged sound mark U+30FC.
    "株式会社ムーヴ",
    "ラーメン店",
    // Latin with Han, Hiragana, Katakana, Bopomofo or Hangul (UTS #39, Highly Restrictive).
    "Example 示例",
    "Example 株式会社",
    "Example ムーヴ",
    "Example ㄅㄆ",
    "Example 예시",
];

const GOLDEN_TITLES: &[&str] = &[
    "MOHV Mileage",
    "MOHV Körjournal",
    "MOHV Kilometrikirjanpito",
    "Exämple & Söns - Ajopäiväkirja",
    "MOHV (intern)",
];

/// Messages: plain text, any punctuation, dates and times written with dots, no links.
const GOLDEN_MESSAGES: &[&str] = &[
    "Welcome to MOHV. Use your work account to sign in.",
    "Välkommen till MOHV. Logga in med ditt arbetskonto.",
    "Tervetuloa! Kirjaudu työtililläsi.",
    "Questions? Ask your team lead \u{2014} they know.",
    "Open Mon-Fri 8.30-17.00 (closed 24.12.2026).",
    "Klo 10.30 alkaen, hinta 5 € + alv.",
    "Bienvenue ! Connectez-vous avec votre compte professionnel.",
    "¡Bienvenido! Inicie sesión con su cuenta de trabajo.",
    "Witamy w MOHV, zaloguj się swoim kontem.",
    "Sign in to continue (VPN not needed).",
    "50% off for members, #1 in service *",
];

#[test]
fn every_golden_name_is_accepted_unchanged() {
    for name in GOLDEN_NAMES {
        let stored = clean(Kind::Name, name).unwrap_or_else(|r| panic!("{name:?}: {r}"));
        assert_eq!(
            &stored, name,
            "{name:?} is stored in NFC, which it already is"
        );
        assert_eq!(
            safe(Kind::Name, &stored),
            Ok(()),
            "{name:?} on the read side"
        );
        let slug = slug::derive(name);
        assert!(slug::is_valid(&slug), "{name:?} gives the slug {slug:?}");
    }
}

#[test]
fn every_golden_name_is_a_valid_title_too() {
    for name in GOLDEN_NAMES {
        assert_eq!(clean(Kind::Title, name).as_deref(), Ok(*name), "{name:?}");
        assert_eq!(clean(Kind::Message, name).as_deref(), Ok(*name), "{name:?}");
    }
}

#[test]
fn every_golden_title_and_message_is_accepted_unchanged() {
    for title in GOLDEN_TITLES {
        let stored = clean(Kind::Title, title).unwrap_or_else(|r| panic!("{title:?}: {r}"));
        assert_eq!(&stored, title);
        assert_eq!(safe(Kind::Title, &stored), Ok(()));
    }
    for message in GOLDEN_MESSAGES {
        let stored = clean(Kind::Message, message).unwrap_or_else(|r| panic!("{message:?}: {r}"));
        assert_eq!(&stored, message);
        assert_eq!(safe(Kind::Message, &stored), Ok(()));
    }
}

#[test]
fn the_golden_lists_have_no_duplicates_and_every_name_fits_the_limits() {
    let mut seen = std::collections::BTreeSet::new();
    for name in GOLDEN_NAMES {
        assert!(seen.insert(*name), "{name:?} is listed twice");
        assert!(name.chars().count() <= crate::policy::NAME_MAX);
    }
    for title in GOLDEN_TITLES {
        assert!(title.chars().count() <= crate::policy::TITLE_MAX);
    }
    for message in GOLDEN_MESSAGES {
        assert!(message.chars().count() <= crate::policy::MESSAGE_MAX);
    }
}
