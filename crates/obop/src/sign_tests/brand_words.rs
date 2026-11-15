//! The help of `sign_request.signer_email` names no product (instance branding, S2): the signer's
//! name field is shown to people of an instance that has a brand of its own, and the sentence it
//! had said "obop sends no email". `sign` 0.2.3 words it without the product, in English and in the
//! six other languages. The package version moved so that `module.upgrade` brings the text to an
//! instance that has `sign` installed; this file proves that, with the previous words rebuilt from
//! the bundled text (the package of 0.2.2 is not kept as a fixture, only that of 0.2.1).

use super::*;
use obop_commands::{Principal, Surface};
use obop_pkg::installer::Request as PkgRequest;
use obop_pkg::resolve::StepAction;

const FIELD: &str = "signer_email";

/// (language, the words of 0.2.2, the words now). English is the manifest's own `description`.
const WORDS: [(&str, &str, &str); 7] = [
    (
        "en",
        "Email address of the signer; optional, obop sends no email for a request about a document",
        "Email address of the signer; optional, no email is sent for a request about a document",
    ),
    (
        "de",
        "E-Mail-Adresse der unterzeichnenden Person; optional, obop versendet bei einer Anfrage zu einem Dokument keine E-Mail",
        "E-Mail-Adresse der unterzeichnenden Person; optional, bei einer Anfrage zu einem Dokument wird keine E-Mail versendet",
    ),
    (
        "fr",
        "Adresse e-mail du signataire ; facultative, obop n'envoie pas d'e-mail pour une demande portant sur un document",
        "Adresse e-mail du signataire ; facultative, aucun e-mail n'est envoyé pour une demande portant sur un document",
    ),
    (
        "pl",
        "Adres e-mail osoby podpisującej; opcjonalny, obop nie wysyła e-maili przy prośbie o dokument",
        "Adres e-mail osoby podpisującej; opcjonalny, przy prośbie o dokument nie jest wysyłany żaden e-mail",
    ),
    (
        "fi",
        "Allekirjoittajan sähköpostiosoite; valinnainen, obop ei lähetä sähköpostia asiakirjaa koskevasta pyynnöstä",
        "Allekirjoittajan sähköpostiosoite; valinnainen, asiakirjaa koskevasta pyynnöstä ei lähetetä sähköpostia",
    ),
    (
        "sv",
        "Signerarens e-postadress; valfri, obop skickar inget e-postmeddelande för en begäran om ett dokument",
        "Signerarens e-postadress; valfri, inget e-postmeddelande skickas för en begäran om ett dokument",
    ),
    (
        "es",
        "Correo electrónico de quien firma; opcional, obop no envía correo en una solicitud sobre un documento",
        "Correo electrónico de quien firma; opcional, no se envía correo en una solicitud sobre un documento",
    ),
];

/// The bundled text of `sign` as 0.2.2 had it: the version and the seven sentences.
fn as_it_was_in_0_2_2(now: &str) -> String {
    let mut text = now.to_string();
    for (lang, was, new) in WORDS {
        assert_eq!(text.matches(new).count(), 1, "{lang}: {new}");
        text = text.replace(new, was);
    }
    assert_eq!(text.matches("version = \"0.2.3\"").count(), 1);
    text.replace("version = \"0.2.3\"", "version = \"0.2.2\"")
}

/// The English help and every translation of the field, as an administrator sees them in the
/// metadata of the REST surface.
fn help_of(app: &App) -> Vec<(String, String)> {
    let p = Principal::user("ann", &["platform.admin", "sign.user", "sign.manager"]);
    let metas = app.dispatcher.meta(Surface::Rest, &p);
    let model = metas
        .iter()
        .flat_map(|m| m.models.iter())
        .find(|m| m.name == "sign_request")
        .expect("sign_request is installed");
    let field = model
        .fields
        .iter()
        .find(|f| f.name == FIELD)
        .expect("signer_email is readable");
    let mut out = vec![("en".to_string(), field.description.to_string())];
    out.extend(
        field
            .descriptions
            .iter()
            .map(|(l, t)| (l.clone(), t.clone())),
    );
    out
}

#[test]
fn the_words_table_is_the_bundled_text() {
    // Guards the test itself: every "now" sentence is in the bundled text exactly once, and no
    // "was" sentence is, so the rebuilt old text really differs in all seven places.
    let now = Bundled.fetch("sign").unwrap();
    for (lang, was, new) in WORDS {
        assert_eq!(now.matches(new).count(), 1, "{lang}");
        assert_eq!(now.matches(was).count(), 0, "{lang}");
        assert!(!new.to_lowercase().contains("obop"), "{lang}: {new}");
        assert!(was.to_lowercase().contains("obop"), "{lang}: {was}");
    }
    let m = obop_pkg::Manifest::parse(&now).unwrap();
    assert_eq!(m.module.version, "0.2.3");
}

#[test]
fn the_help_names_no_product_in_any_language() {
    let app = App::new();
    app.installer
        .install(&Bundled.fetch("sign").unwrap(), "marketplace", "test")
        .unwrap();
    let help = help_of(&app);
    let langs: Vec<&str> = help.iter().map(|(l, _)| l.as_str()).collect();
    assert_eq!(langs, ["en", "de", "es", "fi", "fr", "pl", "sv"]);
    for (lang, text) in &help {
        assert!(
            !text.to_lowercase().contains("obop"),
            "{lang} names the product: {text}"
        );
    }
}

#[test]
fn module_upgrade_brings_the_new_words_to_an_instance_that_has_sign() {
    let now = Bundled.fetch("sign").unwrap();
    let before = as_it_was_in_0_2_2(&now);
    let app = App::new();
    app.installer
        .install(&Bundled.fetch("documents").unwrap(), "marketplace", "test")
        .unwrap();
    app.installer.install(&before, "upload", "test").unwrap();
    assert_eq!(
        app.installer.installed_manifests()["sign"].module.version,
        "0.2.2"
    );
    // The instance shows the old sentence, in all seven languages.
    let help = help_of(&app);
    assert_eq!(help.len(), 7);
    for (lang, text) in &help {
        let (_, was, _) = WORDS.iter().find(|w| w.0 == lang).unwrap();
        assert_eq!(text, was, "{lang} before the upgrade");
    }

    // The plan the Modules page shows: one upgrade of `sign`, nothing that blocks, no data step.
    let old = obop_pkg::Manifest::parse(&before).unwrap();
    let new = obop_pkg::Manifest::parse(&now).unwrap();
    assert!(
        obop_pkg::migrate::diff(&old, &new).is_empty(),
        "a reworded help is not a change of the model"
    );
    let (plan, _) = app.installer.plan(PkgRequest::Manifest(&now)).unwrap();
    assert!(plan.ok(), "{:?}", plan.blockers);
    let steps: Vec<_> = plan.work().map(|s| (s.id.clone(), s.action)).collect();
    assert_eq!(steps, vec![("sign".to_string(), StepAction::Upgrade)]);
    assert!(plan.data["sign"].steps.is_empty());

    let out = app
        .installer
        .install_outcome(&now, "marketplace", "test")
        .unwrap();
    assert_eq!(out.root.version, "0.2.3");

    // Now every language says it without the product, and says what the table says.
    let help = help_of(&app);
    assert_eq!(help.len(), 7);
    for (lang, text) in &help {
        let (_, _, new) = WORDS.iter().find(|w| w.0 == lang).unwrap();
        assert_eq!(text, new, "{lang} after the upgrade");
        assert!(!text.to_lowercase().contains("obop"), "{lang}: {text}");
    }
}
