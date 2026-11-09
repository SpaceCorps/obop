//! The texts of the model `instance_brand` in six languages (docs/spec/branding.md, section 7: the
//! admin labels are machine-drafted, "56 cells per language, 336 in six languages"). The D52 tests
//! in the binary only ask that a cell exists and differs from the English one, so a wrong number, a
//! wrong language name or an identifier that was translated would pass them. These tests tie each
//! cell to the English text and the English text to the code:
//!
//! - the numbers in the English texts are the limits the code enforces, and every translation has
//!   the same numbers;
//! - the identifiers (`dry_run`, `title_<lang>`, `tile_accent`, ...) and `platform.admin` of every
//!   translation are those of the English text, with the same placeholder `<lang>`;
//! - the label and the description of a title or a message name the language of their key and no
//!   other;
//! - the text of `effective` lists the keys and the sources of the answer of `get`;
//! - the cells stay marked machine-drafted until the confirmation that the spec asks for is made.

use super::*;
use obop_commands::brand::{ARTWORK_MAX_BYTES, MESSAGE_MAX, NAME_MAX, SLUG_MAX, TITLE_MAX};
use obop_registry::Module;
use obop_registry::template::{Slot, SlotKind, slots};
use std::collections::BTreeSet;

const TRANSLATED: [&str; 6] = ["de", "fr", "pl", "fi", "sv", "es"];

/// The commands of the model, as they are typed.
const COMMANDS: [&str; 3] = ["get", "configure", "clear"];

/// What each of the six languages calls the seven languages of the record (the order of
/// `LANGUAGES`: en, de, fr, pl, fi, sv, es): the part of the word that stays the same in the label
/// ("Titel (Französisch)") and in the sentence ("Der Titel auf Französisch"), in lower case.
const NAMES: [(&str, [&str; 7]); 6] = [
    (
        "de",
        [
            "englisch",
            "deutsch",
            "französisch",
            "polnisch",
            "finnisch",
            "schwedisch",
            "spanisch",
        ],
    ),
    (
        "fr",
        [
            "anglais",
            "allemand",
            "français",
            "polonais",
            "finnois",
            "suédois",
            "espagnol",
        ],
    ),
    (
        "pl",
        [
            "angielsk",
            "niemieck",
            "francusk",
            "polsk",
            "fińsk",
            "szwedzk",
            "hiszpańsk",
        ],
    ),
    (
        "fi",
        [
            "englan", "saksa", "ranska", "puola", "suom", "ruots", "espanja",
        ],
    ),
    (
        "sv",
        [
            "engelska", "tyska", "franska", "polska", "finska", "svenska", "spanska",
        ],
    ),
    (
        "es",
        [
            "inglés", "alemán", "francés", "polaco", "finés", "sueco", "español",
        ],
    ),
];

/// The status files of the label directories, as the build reads them.
const STATUS: [(&str, &str); 6] = [
    (
        "de",
        include_str!("../../../../../../packages/labels/de/status.json"),
    ),
    (
        "fr",
        include_str!("../../../../../../packages/labels/fr/status.json"),
    ),
    (
        "pl",
        include_str!("../../../../../../packages/labels/pl/status.json"),
    ),
    (
        "fi",
        include_str!("../../../../../../packages/labels/fi/status.json"),
    ),
    (
        "sv",
        include_str!("../../../../../../packages/labels/sv/status.json"),
    ),
    (
        "es",
        include_str!("../../../../../../packages/labels/es/status.json"),
    ),
];

fn brand_slots(module: &Module) -> Vec<Slot> {
    slots(module)
        .into_iter()
        .filter(|s| s.model == MODEL)
        .collect()
}

fn text<'a>(module: &'a Module, slot: &Slot, lang: &str) -> &'a str {
    slot.get(&module.translations, lang)
        .unwrap_or_else(|| panic!("{} ({lang}) is missing", slot.path()))
}

/// The numbers in a text, in order of size.
fn numbers(text: &str) -> Vec<u64> {
    let mut out = Vec::new();
    let mut run = String::new();
    for c in text.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_digit() {
            run.push(c);
        } else if !run.is_empty() {
            out.push(run.parse().unwrap());
            run.clear();
        }
    }
    out.sort_unstable();
    out
}

/// The identifiers in a text: words of letters, digits, `_`, `<` and `>` that have an underscore
/// and a letter (`dry_run`, `title_<lang>`, `tile_accent`), whatever follows them in the language.
fn identifiers(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '<' | '>')))
        .filter(|w| w.contains('_') && w.chars().any(|c| c.is_ascii_alphabetic()))
        .map(String::from)
        .collect()
}

/// The placeholder of a language in an English text. A label value is plain text and may not hold
/// `<` or `>` (the loader refuses them), so a translated cell writes it as `XX` and says what it is.
const PLACEHOLDER: &str = "<lang>";

fn in_translation(identifier: &str) -> String {
    identifier.replace(PLACEHOLDER, "XX")
}

/// The width of `v` in hex digits, as the code produces it.
fn width_of_v() -> u64 {
    let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 8 8\"><rect width=\"8\" height=\"8\" fill=\"#112233\"/></svg>";
    let out = rig()
        .configure(&[("name", "MOHV"), ("logo_svg", svg)])
        .unwrap();
    out["v"].as_str().unwrap().len() as u64
}

/// The numbers the English text of a field description must hold, built from the limits of the
/// code (`obop-brand`'s policy and its SVG reader).
fn limits_in(item: &str) -> Vec<u64> {
    let mut v = match item {
        "name" => vec![1, NAME_MAX as u64],
        // "a-z, 0-9": the two digits of the range.
        "slug" => vec![0, 9, SLUG_MAX as u64],
        i if i.starts_with("title_") => vec![TITLE_MAX as u64],
        i if i.starts_with("message_") => vec![MESSAGE_MAX as u64],
        "logo_svg" => vec![(ARTWORK_MAX_BYTES / 1024) as u64],
        "v" => vec![width_of_v()],
        _ => vec![],
    };
    v.sort_unstable();
    v
}

#[test]
fn the_numbers_in_the_english_texts_are_the_limits_of_the_code_and_every_translation_has_them() {
    let module = admin_module();
    let all = brand_slots(&module);
    assert_eq!(all.len(), 56);
    let mut checked = 0;
    for slot in &all {
        let want = if slot.kind == SlotKind::FieldDescription {
            limits_in(&slot.item)
        } else {
            vec![]
        };
        assert_eq!(
            numbers(&slot.english),
            want,
            "{}: the English text",
            slot.path()
        );
        for lang in TRANSLATED {
            assert_eq!(
                numbers(text(&module, slot, lang)),
                want,
                "{} ({lang})",
                slot.path()
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 336, "56 cells in each of six languages");
}

#[test]
fn the_identifiers_and_the_placeholder_of_every_translation_are_those_of_the_english_text() {
    let module = admin_module();
    let mut named = BTreeSet::new();
    for slot in brand_slots(&module) {
        let english = identifiers(&slot.english);
        named.extend(english.iter().cloned());
        let want: BTreeSet<String> = english.iter().map(|i| in_translation(i)).collect();
        let admin = slot.english.contains(ADMIN_GROUP);
        let placeholder = english.iter().any(|i| i.contains(PLACEHOLDER));
        for lang in TRANSLATED {
            let cell = text(&module, &slot, lang);
            assert_eq!(
                identifiers(cell),
                want,
                "{} ({lang}): {cell:?}",
                slot.path()
            );
            // The placeholder is explained: every language code is in the cell, as a word. The
            // `configure` cells are at the limit of a description (400 characters) and have no room
            // for it; the fields they list are the model's own.
            if placeholder && slot.item == "effective" {
                let words: Vec<&str> = cell.split(|c: char| !c.is_alphanumeric()).collect();
                for (code, _) in LANGUAGES {
                    assert!(
                        words.contains(&code),
                        "{} ({lang}): {cell:?} does not list {code}",
                        slot.path()
                    );
                }
            }
            assert_eq!(
                cell.contains(ADMIN_GROUP),
                admin,
                "{} ({lang}): the group {ADMIN_GROUP}",
                slot.path()
            );
            // The names of the commands stay as they are, lower case and with no ending: a
            // command is a word the reader types.
            let words = |t: &str| -> BTreeSet<String> {
                t.split(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .filter(|w| COMMANDS.contains(w))
                    .map(String::from)
                    .collect()
            };
            assert_eq!(
                words(cell),
                words(&slot.english),
                "{} ({lang}): {cell:?}",
                slot.path()
            );
        }
    }
    // The English texts do name these, so the loop above is not about nothing.
    for id in [
        "dry_run",
        "logo_svg",
        "tile_accent",
        "title_<lang>",
        "message_<lang>",
    ] {
        assert!(named.contains(id), "{id}: {named:?}");
    }
}

#[test]
fn a_title_or_message_cell_names_its_own_language_and_no_other() {
    let module = admin_module();
    let all = brand_slots(&module);
    for (i, (code, language)) in LANGUAGES.iter().enumerate() {
        for (prefix, noun_en) in [("title_", "title"), ("message_", "message")] {
            let item = format!("{prefix}{code}");
            // English: "The title in German: ..."
            let description = all
                .iter()
                .find(|s| s.kind == SlotKind::FieldDescription && s.item == item)
                .unwrap();
            assert!(
                description
                    .english
                    .starts_with(&format!("The {noun_en} in {language}: ")),
                "{item}: {}",
                description.english
            );
            for (lang, names) in NAMES {
                for slot in all.iter().filter(|s| s.item == item) {
                    let cell = text(&module, slot, lang);
                    // The label is the head; the description opens with its own head, up to `:`.
                    let head = match slot.kind {
                        SlotKind::FieldLabel => cell,
                        SlotKind::FieldDescription => cell.split(':').next().unwrap(),
                        other => panic!("{other:?}"),
                    }
                    .to_lowercase();
                    assert!(
                        head.contains(names[i]),
                        "{} ({lang}): {cell:?} does not name {language}",
                        slot.path()
                    );
                    for (j, other) in names.iter().enumerate() {
                        assert!(
                            j == i || !head.contains(other),
                            "{} ({lang}): {cell:?} names {other:?} as well",
                            slot.path()
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn the_text_of_effective_lists_the_keys_and_the_sources_of_the_answer_of_get() {
    let r = rig();
    // The keys: `title_en` and the other six become `title_<lang>`.
    r.configure(&[("name", "MOHV"), ("title_sv", "Logga in")])
        .unwrap();
    let keys_of = |answer: &J| -> BTreeSet<String> {
        parsed(answer, "effective")
            .as_object()
            .unwrap()
            .keys()
            .map(|k| {
                LANGUAGES
                    .iter()
                    .find_map(|(c, _)| {
                        k.strip_suffix(&format!("_{c}"))
                            .map(|stem| format!("{stem}_<lang>"))
                    })
                    .unwrap_or_else(|| k.clone())
            })
            .collect()
    };
    let with_row = r.get();
    let keys = keys_of(&with_row);
    // The sources: a stored value, an explicit `site.*` key, a stored theme accent, and the defaults.
    let mut sources: BTreeSet<String> = BTreeSet::new();
    let mut collect = |answer: &J| {
        for v in parsed(answer, "effective").as_object().unwrap().values() {
            sources.insert(v["source"].as_str().unwrap().to_string());
        }
    };
    collect(&with_row);
    let themed = json!({"theme": {"accent": "#0a8f7a"}, "site": {"title": "Shop"}}).to_string();
    let mut f = obop_storage::Fields::new();
    f.insert("value".into(), json!(themed));
    r.store
        .backend()
        .apply_checked(&[obop_storage::Op::Put {
            model: "obop_setting".into(),
            id: "instance".into(),
            fields: f,
        }])
        .unwrap();
    collect(&r.get());
    collect(&rig().get());
    // The source `env` is the variable `OBOP_MFA_ISSUER`, which `get` reads from the environment of
    // the process; the answer with a value for it is built from the same read, so that no test
    // changes the environment.
    let with_variable: J = serde_json::from_str(
        &obop_commands::brand::Reader::new(r.store.backend())
            .fresh()
            .unwrap()
            .effective_json_with(Some("Acme".into())),
    )
    .unwrap();
    for v in with_variable.as_object().unwrap().values() {
        sources.insert(v["source"].as_str().unwrap().to_string());
    }
    // The source `unreadable` is the issuer of a stored name that failed the read check (no issuer
    // is in force: an enrolment is refused).
    let unreadable = rig();
    let mut row = obop_storage::Fields::new();
    row.insert("name".into(), json!("a<b"));
    unreadable
        .store
        .backend()
        .apply_checked(&[obop_storage::Op::Put {
            model: "instance_brand".into(),
            id: "instance".into(),
            fields: row,
        }])
        .unwrap();
    let without_name: J = serde_json::from_str(
        &obop_commands::brand::Reader::new(unreadable.store.backend())
            .fresh()
            .unwrap()
            .effective_json_with(None),
    )
    .unwrap();
    for v in without_name.as_object().unwrap().values() {
        sources.insert(v["source"].as_str().unwrap().to_string());
    }
    assert_eq!(
        sources,
        ["default", "env", "record", "site", "theme", "unreadable"]
            .map(String::from)
            .into_iter()
            .collect::<BTreeSet<_>>(),
        "the sources the answer can name at this head"
    );

    let module = admin_module();
    let slot = brand_slots(&module)
        .into_iter()
        .find(|s| s.kind == SlotKind::FieldDescription && s.item == "effective")
        .unwrap();
    // English: "... for each value in force (name, slug, ... and message_<lang>), ..."
    let inside = slot
        .english
        .split_once("value in force (")
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(list, _)| list)
        .unwrap_or_else(|| panic!("{}", slot.english));
    let listed: BTreeSet<String> = inside
        .split(',')
        .flat_map(|part| part.split(" and "))
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .collect();
    assert_eq!(listed, keys, "the keys the English text lists");
    // The sources are words of the text, not pieces of another word: the source `site` is not found
    // in the key `site.title`. A word is a run of letters, digits, `_` and `.`, so `site.title,` is
    // the word `site.title` and `record, site, theme` has the word `site`.
    let words = |t: &str| -> BTreeSet<String> {
        t.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
            .filter(|w| !w.is_empty())
            .map(str::to_string)
            .collect()
    };
    let english = words(&slot.english);
    for source in &sources {
        assert!(
            english.contains(source),
            "the source {source} as a word of the English text: {}",
            slot.english
        );
    }
    for lang in TRANSLATED {
        let cell = text(&module, &slot, lang);
        for key in &listed {
            let key = in_translation(key);
            assert!(cell.contains(key.as_str()), "({lang}) {key}: {cell:?}");
        }
        let in_cell = words(cell);
        for source in &sources {
            assert!(
                in_cell.contains(source),
                "({lang}) the source {source} as a word: {cell:?}"
            );
        }
    }
}

#[test]
fn the_platform_labels_are_marked_machine_drafted_until_they_are_confirmed() {
    // The spec asks the lead, with counsel, to confirm the machine-drafted cells (Q71 item 2, section
    // 7: "the confirmation itself is still to do"), the 56 of the model among them. Marking
    // `platform` as reviewed says that was done: change this test with that confirmation, not before.
    for (lang, status) in STATUS {
        let status: std::collections::BTreeMap<String, String> =
            serde_json::from_str(status).unwrap();
        assert_eq!(
            status.get("platform").map(String::as_str),
            Some("machine-drafted"),
            "packages/labels/{lang}/status.json"
        );
    }
}
