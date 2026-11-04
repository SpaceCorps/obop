//! The text of the signer page (`/sign/{request}/{token}`, D92) in the seven languages of the
//! site and of the OAuth pages: `en` `de` `fr` `pl` `fi` `sv` `es` ([`obop_oauth::page::LANGS`]).
//!
//! **This file is pure data.** It reads nothing of a request, it writes nothing, and it knows no
//! other part of the signer door. The page renderer (`sign_page.rs`) calls [`text`], [`fill`] and
//! [`html`]; the route picks the language with [`choose_language`].
//!
//! **Status of the words.** The English text is the original. The six other languages are
//! **MACHINE-DRAFTED**: nobody who speaks those languages has reviewed them (docs/ISSUES.md, the
//! signer-page row). The consent sentence (`consent`) is a **draft for counsel**, not legal advice,
//! and its id is [`consent_version_for`]: [`CONSENT_VERSION`] (`v1-draft`), the language and the
//! first 8 hex characters of a SHA-256 over the eight texts a signer relies on when they agree
//! ([`WORDING_KEYS`]: the consent sentence, the notice of what is recorded, the hint, the button,
//! the viewer fallback, the missing-tick message, the "request is for" line and the fingerprint
//! line), computed from the catalogue at run time. The id is recorded with every signature, so
//! editing any of those texts changes the stored id **by construction**: an id that a real
//! signature carries cannot be reused for other words. The test
//! `the_wording_the_signer_agrees_to_is_pinned_to_its_id` is a second net: it holds the ids of
//! today, so that an edit shows up as a failing test that names the language. When counsel
//! confirms the English text the constant becomes `v1`. The first live use must be the English
//! page (the only original); any other language, German included, is machine-drafted and must be
//! read by a native speaker first.
//!
//! **Placeholders.** `{title}`, `{name}`, `{hash}` and `{time}` are the only ones. Every column of
//! a key carries the same set (a test). The caller passes plain values; [`html`] escapes both the
//! catalogue text and the values, [`fill`] does not escape anything (for a `<title>` or a header
//! that the caller escapes itself).
//!
//! **The keys are fixed** (the design of D92, section 4.1): `title`, `document`, `open_full`,
//! `no_viewer`, `fingerprint{hash}`, `name_label`, `name_hint`, `consent{name}`, `submit`,
//! `records`, `err_name`, `err_agree`, `done_title`, `done_body{time}`, `gone_expired`,
//! `gone_revoked`, `changed`, `unknown`, `busy`, `unavailable`, `request{title}`, `for{name}`, and,
//! added when the door's page needed them, `forbidden` (a post that did not come from the page) and
//! `closed` (a request that was declined, has expired or no longer needs a signature).
//! `consent` takes `{name}` (the requested person), because the sentence names the person who is
//! asked to sign (lead decision 9; attacker finding S7).

use obop_oauth::page::{LANGS, esc};

/// The version of the consent wording, the first part of the id recorded with the evidence of a
/// signature. A draft until counsel has seen it (lead decision 9, attacker finding F18); the rest
/// of the id is derived from the texts themselves ([`consent_version_for`]), so this constant is
/// the label of a review, not what makes the id differ.
pub const CONSENT_VERSION: &str = "v1-draft";

/// The languages whose columns are machine-drafted and not reviewed by a native speaker.
pub const MACHINE_DRAFTED: [&str; 6] = ["de", "fr", "pl", "fi", "sv", "es"];

/// The texts a signer relies on when they tick the box and press the button, in the order they
/// are hashed into the id: the consent sentence (which names the button), the notice of what is
/// recorded, the hint under the name, the button, the fallback when the browser cannot show the
/// PDF (which tells the signer to read all of it), the message about the missing tick, the
/// "this request is for" line and the line about the file's fingerprint.
pub const WORDING_KEYS: [&str; 8] = [
    "consent",
    "records",
    "name_hint",
    "submit",
    "no_viewer",
    "err_agree",
    "for",
    "fingerprint",
];

/// `consent_version` as stored on the request: `<CONSENT_VERSION>/<language>/<wording>`, for
/// example `v1-draft/de/823c236d`, where `<wording>` is the first 8 hex characters of the SHA-256
/// of the texts of [`WORDING_KEYS`] in that language (`key`, a line feed, the text and a line
/// feed, for each key in turn). The texts are the exact catalogue texts the page renders, with
/// their `{placeholders}`. An unknown language is recorded as `en`. The language is the one the
/// caller names (the POST's), not a record of the page that was shown.
pub fn consent_version_for(lang: &str) -> String {
    let i = lang_idx(lang);
    format!(
        "{CONSENT_VERSION}/{}/{}",
        LANGS[i],
        wording_id(CATALOGUE, i)
    )
}

/// The first 8 hex characters of the SHA-256 over the [`WORDING_KEYS`] texts in column `i` of
/// `catalogue`. A text that is empty in a language is read from English, as [`text`] does.
fn wording_id(catalogue: &[(&str, [&str; 7])], i: usize) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    for key in WORDING_KEYS {
        let cols = catalogue
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, c)| c)
            .unwrap_or(&["", "", "", "", "", "", ""]);
        let text = if cols[i].is_empty() { cols[0] } else { cols[i] };
        h.update(format!("{key}\n{text}\n").as_bytes());
    }
    h.finalize()
        .iter()
        .take(4)
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn lang_idx(lang: &str) -> usize {
    LANGS.iter().position(|l| *l == lang).unwrap_or(0)
}

/// The language of a tag such as `de`, `de-AT` or `pt_BR` when the catalogue has it.
fn known(tag: &str) -> Option<&'static str> {
    let primary = tag
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    LANGS.into_iter().find(|l| *l == primary)
}

/// The page language: the `lang` parameter, then the best match of `Accept-Language` (q-values
/// honoured, an earlier tag wins a tie), then the instance default language, then English. Any of
/// the three inputs may be missing or nonsense; the answer is always one of [`LANGS`].
pub fn choose_language(
    param: Option<&str>,
    accept: Option<&str>,
    instance_default: Option<&str>,
) -> &'static str {
    if let Some(l) = param.and_then(known) {
        return l;
    }
    let mut best: Option<(f32, usize, &'static str)> = None;
    for (i, part) in accept.unwrap_or("").split(',').enumerate() {
        let mut it = part.split(';');
        let tag = it.next().unwrap_or("").trim();
        let q = it
            .find_map(|p| {
                p.trim()
                    .strip_prefix("q=")
                    .and_then(|v| v.parse::<f32>().ok())
            })
            .unwrap_or(1.0);
        if q <= 0.0 || q.is_nan() {
            continue;
        }
        if let Some(l) = known(tag)
            && best.is_none_or(|(bq, bi, _)| q > bq || (q == bq && i < bi))
        {
            best = Some((q, i, l));
        }
    }
    if let Some((_, _, l)) = best {
        return l;
    }
    instance_default.and_then(known).unwrap_or("en")
}

/// The raw text for `key` in `lang`, with its `{placeholders}` still in it. An unknown language
/// gives English; an unknown key gives the empty string (a test makes sure the renderer never asks
/// for one).
pub fn text(lang: &str, key: &str) -> &'static str {
    match CATALOGUE.iter().find(|(k, _)| *k == key) {
        Some((_, cols)) if !cols[lang_idx(lang)].is_empty() => cols[lang_idx(lang)],
        Some((_, cols)) => cols[0],
        None => "",
    }
}

/// The text with each `{name}` replaced by its plain value. Nothing is escaped.
pub fn fill(lang: &str, key: &str, args: &[(&str, &str)]) -> String {
    let mut s = text(lang, key).to_string();
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

/// The text for HTML: the catalogue text and every value are escaped, and each `{name}` is
/// replaced by its escaped value. The result is safe inside an element or a quoted attribute.
pub fn html(lang: &str, key: &str, args: &[(&str, &str)]) -> String {
    let mut s = esc(text(lang, key));
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), &esc(v));
    }
    s
}

/// Text of the signer page. One row per key, columns in the order of [`LANGS`]:
/// en, de, fr, pl, fi, sv, es. The six translations are machine-drafted (see the header).
#[rustfmt::skip]
pub const CATALOGUE: &[(&str, [&str; 7])] = &[
    ("title", ["Sign a document", "Dokument unterschreiben", "Signer un document", "Podpisz dokument", "Allekirjoita asiakirja", "Signera ett dokument", "Firmar un documento"]),
    ("request", ["Signature request: {title}", "Unterschriftsanfrage: {title}", "Demande de signature : {title}", "Prośba o podpis: {title}", "Allekirjoituspyyntö: {title}", "Begäran om signatur: {title}", "Solicitud de firma: {title}"]),
    ("for", ["This request is for {name}.", "Diese Anfrage ist für {name} bestimmt.", "Cette demande est destinée à {name}.", "Ta prośba jest przeznaczona dla: {name}.", "Tämä pyyntö on osoitettu henkilölle {name}.", "Den här begäran gäller {name}.", "Esta solicitud es para {name}."]),
    ("document", ["The document", "Das Dokument", "Le document", "Dokument", "Asiakirja", "Dokumentet", "El documento"]),
    ("open_full", ["Open the document in full size (new tab)", "Dokument in voller Größe öffnen (neuer Tab)", "Ouvrir le document en grand (nouvel onglet)", "Otwórz dokument w pełnym rozmiarze (nowa karta)", "Avaa asiakirja koko näytölle (uusi välilehti)", "Öppna dokumentet i full storlek (ny flik)", "Abrir el documento a tamaño completo (pestaña nueva)"]),
    ("no_viewer", ["Your browser cannot show the document on this page. Open it in full size and read all of it before you sign.", "Ihr Browser kann das Dokument auf dieser Seite nicht anzeigen. Öffnen Sie es in voller Größe und lesen Sie es ganz, bevor Sie unterschreiben.", "Votre navigateur ne peut pas afficher le document sur cette page. Ouvrez-le en grand et lisez-le en entier avant de signer.", "Twoja przeglądarka nie może wyświetlić dokumentu na tej stronie. Otwórz go w pełnym rozmiarze i przeczytaj całość przed podpisaniem.", "Selaimesi ei voi näyttää asiakirjaa tällä sivulla. Avaa se koko näytölle ja lue se kokonaan ennen allekirjoittamista.", "Din webbläsare kan inte visa dokumentet på den här sidan. Öppna det i full storlek och läs allt innan du signerar.", "Su navegador no puede mostrar el documento en esta página. Ábralo a tamaño completo y léalo entero antes de firmar."]),
    ("fingerprint", ["File fingerprint (first 16 characters of its SHA-256): {hash}", "Fingerabdruck der Datei (erste 16 Zeichen des SHA-256): {hash}", "Empreinte du fichier (16 premiers caractères du SHA-256) : {hash}", "Odcisk pliku (pierwsze 16 znaków SHA-256): {hash}", "Tiedoston sormenjälki (SHA-256:n 16 ensimmäistä merkkiä): {hash}", "Filens fingeravtryck (de första 16 tecknen i SHA-256): {hash}", "Huella del archivo (primeros 16 caracteres de su SHA-256): {hash}"]),
    ("name_label", ["Your full name", "Ihr vollständiger Name", "Votre nom complet", "Imię i nazwisko", "Koko nimesi", "Ditt fullständiga namn", "Su nombre completo"]),
    ("name_hint", ["Type your name as you would sign it. It is recorded as your signature.", "Geben Sie Ihren Namen so ein, wie Sie unterschreiben würden. Er wird als Ihre Unterschrift gespeichert.", "Saisissez votre nom comme vous le signeriez. Il est enregistré comme votre signature.", "Wpisz swoje imię i nazwisko tak, jak byś się podpisał(a). Zostanie zapisane jako Twój podpis.", "Kirjoita nimesi niin kuin allekirjoittaisit sen. Se tallennetaan allekirjoituksenasi.", "Skriv ditt namn som du skulle skriva under det. Det sparas som din signatur.", "Escriba su nombre como lo firmaría. Se guarda como su firma."]),
    ("consent", ["I am {name}. I have read the document and I agree to it. By typing my name and clicking Sign, I sign it electronically.", "Ich bin {name}. Ich habe das Dokument gelesen und stimme ihm zu. Indem ich meinen Namen eingebe und auf Unterschreiben klicke, unterschreibe ich es elektronisch.", "Je suis {name}. J'ai lu le document et je l'approuve. En saisissant mon nom et en cliquant sur Signer, je le signe électroniquement.", "Jestem {name}. Przeczytałem(-am) dokument i wyrażam na niego zgodę. Wpisując swoje imię i nazwisko i klikając Podpisz, podpisuję go elektronicznie.", "Olen {name}. Olen lukenut asiakirjan ja hyväksyn sen. Kirjoittamalla nimeni ja painamalla Allekirjoita allekirjoitan sen sähköisesti.", "Jag är {name}. Jag har läst dokumentet och godkänner det. Genom att skriva mitt namn och klicka på Signera signerar jag det elektroniskt.", "Soy {name}. He leído el documento y estoy de acuerdo con él. Al escribir mi nombre y hacer clic en Firmar, lo firmo electrónicamente."]),
    ("submit", ["Sign", "Unterschreiben", "Signer", "Podpisz", "Allekirjoita", "Signera", "Firmar"]),
    ("records", ["When you sign, this system records the name you typed, the date and time, the network address and the browser your request came from, and the fingerprint of this document. This is a simple electronic signature. The link is the only check that it is you, so do not pass it on.", "Wenn Sie unterschreiben, speichert dieses System den eingegebenen Namen, Datum und Uhrzeit, die Netzwerkadresse und den Browser Ihrer Anfrage sowie den Fingerabdruck dieses Dokuments. Das ist eine einfache elektronische Signatur. Der Link ist die einzige Prüfung, dass Sie es sind. Geben Sie ihn deshalb nicht weiter.", "Lorsque vous signez, ce système enregistre le nom saisi, la date et l'heure, l'adresse réseau et le navigateur d'où provient votre requête, ainsi que l'empreinte de ce document. Il s'agit d'une signature électronique simple. Le lien est la seule vérification qu'il s'agit bien de vous : ne le transmettez donc pas.", "Gdy podpisujesz, system zapisuje wpisane imię i nazwisko, datę i godzinę, adres sieciowy i przeglądarkę, z których wysłano żądanie, oraz odcisk tego dokumentu. To jest zwykły podpis elektroniczny. Link jest jedynym sprawdzeniem, że to Ty, więc nie przekazuj go dalej.", "Kun allekirjoitat, järjestelmä tallentaa kirjoittamasi nimen, päivämäärän ja kellonajan, pyyntösi verkko-osoitteen ja selaimen sekä tämän asiakirjan sormenjäljen. Tämä on yksinkertainen sähköinen allekirjoitus. Linkki on ainoa tarkistus siitä, että se olet sinä, joten älä välitä sitä eteenpäin.", "När du signerar sparar systemet namnet du skrev, datum och tid, nätverksadressen och webbläsaren som din begäran kom från samt dokumentets fingeravtryck. Det här är en enkel elektronisk signatur. Länken är den enda kontrollen av att det är du, så skicka den inte vidare.", "Al firmar, este sistema registra el nombre que escribió, la fecha y la hora, la dirección de red y el navegador desde los que llegó su solicitud, y la huella de este documento. Es una firma electrónica simple. El enlace es la única comprobación de que es usted, así que no lo comparta."]),
    ("err_name", ["Type your full name: at least two letters and at most 120 characters.", "Geben Sie Ihren vollständigen Namen ein: mindestens zwei Buchstaben und höchstens 120 Zeichen.", "Saisissez votre nom complet : au moins deux lettres et 120 caractères au plus.", "Wpisz swoje imię i nazwisko: co najmniej dwie litery i najwyżej 120 znaków.", "Kirjoita koko nimesi: vähintään kaksi kirjainta ja enintään 120 merkkiä.", "Skriv ditt fullständiga namn: minst två bokstäver och högst 120 tecken.", "Escriba su nombre completo: al menos dos letras y como máximo 120 caracteres."]),
    ("err_agree", ["Tick the box to confirm that you have read the document and agree to it.", "Setzen Sie den Haken, um zu bestätigen, dass Sie das Dokument gelesen haben und ihm zustimmen.", "Cochez la case pour confirmer que vous avez lu le document et que vous l'approuvez.", "Zaznacz pole, aby potwierdzić, że przeczytałeś(-aś) dokument i się z nim zgadzasz.", "Valitse ruutu vahvistaaksesi, että olet lukenut asiakirjan ja hyväksyt sen.", "Kryssa i rutan för att bekräfta att du har läst dokumentet och godkänner det.", "Marque la casilla para confirmar que ha leído el documento y está de acuerdo con él."]),
    ("done_title", ["Signed", "Unterschrieben", "Signé", "Podpisano", "Allekirjoitettu", "Signerat", "Firmado"]),
    ("done_body", ["This document was signed on {time} (UTC). The signature is recorded. You can close this page.", "Dieses Dokument wurde am {time} (UTC) unterschrieben. Die Unterschrift ist gespeichert. Sie können diese Seite schließen.", "Ce document a été signé le {time} (UTC). La signature est enregistrée. Vous pouvez fermer cette page.", "Ten dokument został podpisany {time} (UTC). Podpis został zapisany. Możesz zamknąć tę stronę.", "Tämä asiakirja allekirjoitettiin {time} (UTC). Allekirjoitus on tallennettu. Voit sulkea tämän sivun.", "Dokumentet signerades {time} (UTC). Signaturen är sparad. Du kan stänga den här sidan.", "Este documento se firmó el {time} (UTC). La firma queda registrada. Puede cerrar esta página."]),
    ("gone_expired", ["This link has expired. Ask the sender for a new one.", "Dieser Link ist abgelaufen. Bitten Sie den Absender um einen neuen.", "Ce lien a expiré. Demandez-en un nouveau à l'expéditeur.", "Ten link wygasł. Poproś nadawcę o nowy.", "Tämä linkki on vanhentunut. Pyydä lähettäjältä uusi.", "Länken har gått ut. Be avsändaren om en ny.", "Este enlace ha caducado. Pida uno nuevo al remitente."]),
    ("gone_revoked", ["This link can no longer be used. Ask the sender for a new one.", "Dieser Link kann nicht mehr verwendet werden. Bitten Sie den Absender um einen neuen.", "Ce lien ne peut plus être utilisé. Demandez-en un nouveau à l'expéditeur.", "Tego linku nie można już użyć. Poproś nadawcę o nowy.", "Tätä linkkiä ei voi enää käyttää. Pyydä lähettäjältä uusi.", "Länken kan inte längre användas. Be avsändaren om en ny.", "Este enlace ya no se puede usar. Pida uno nuevo al remitente."]),
    ("changed", ["The document changed after this request was sent, so it cannot be signed here. Do not sign. Tell the sender.", "Das Dokument wurde nach dem Versand dieser Anfrage geändert und kann hier nicht unterschrieben werden. Bitte nicht unterschreiben. Informieren Sie den Absender.", "Le document a été modifié après l'envoi de cette demande ; il ne peut pas être signé ici. Ne signez pas. Prévenez l'expéditeur.", "Dokument zmieniono po wysłaniu tej prośby, więc nie można go tu podpisać. Nie podpisuj. Powiadom nadawcę.", "Asiakirjaa on muutettu pyynnön lähettämisen jälkeen, joten sitä ei voi allekirjoittaa täällä. Älä allekirjoita. Kerro asiasta lähettäjälle.", "Dokumentet har ändrats efter att begäran skickades och kan inte signeras här. Signera inte. Berätta för avsändaren.", "El documento cambió después de enviarse esta solicitud, así que no se puede firmar aquí. No firme. Avise al remitente."]),
    ("unknown", ["This page was not found. Check that you copied the whole link.", "Diese Seite wurde nicht gefunden. Prüfen Sie, ob Sie den ganzen Link kopiert haben.", "Cette page est introuvable. Vérifiez que vous avez copié le lien en entier.", "Nie znaleziono tej strony. Sprawdź, czy skopiowano cały link.", "Sivua ei löytynyt. Tarkista, että kopioit koko linkin.", "Sidan hittades inte. Kontrollera att du kopierade hela länken.", "No se encontró esta página. Compruebe que copió el enlace completo."]),
    ("busy", ["Too many attempts. Wait a minute and try again.", "Zu viele Versuche. Warten Sie eine Minute und versuchen Sie es erneut.", "Trop de tentatives. Attendez une minute et réessayez.", "Zbyt wiele prób. Poczekaj minutę i spróbuj ponownie.", "Liian monta yritystä. Odota minuutti ja yritä uudelleen.", "För många försök. Vänta en minut och försök igen.", "Demasiados intentos. Espere un minuto e inténtelo de nuevo."]),
    ("unavailable", ["This page is temporarily unavailable. Try again in a few minutes.", "Diese Seite ist vorübergehend nicht verfügbar. Versuchen Sie es in einigen Minuten erneut.", "Cette page est temporairement indisponible. Réessayez dans quelques minutes.", "Ta strona jest tymczasowo niedostępna. Spróbuj ponownie za kilka minut.", "Sivu ei ole tilapäisesti käytettävissä. Yritä uudelleen muutaman minuutin kuluttua.", "Sidan är tillfälligt otillgänglig. Försök igen om några minuter.", "Esta página no está disponible temporalmente. Inténtelo de nuevo en unos minutos."]),
    ("forbidden", ["This request could not be checked. Open the link again and sign from that page.", "Diese Anfrage konnte nicht überprüft werden. Öffnen Sie den Link erneut und unterschreiben Sie auf dieser Seite.", "Cette demande n'a pas pu être vérifiée. Rouvrez le lien et signez depuis cette page.", "Nie udało się zweryfikować tego żądania. Otwórz link ponownie i podpisz z tej strony.", "Tätä pyyntöä ei voitu tarkistaa. Avaa linkki uudelleen ja allekirjoita siltä sivulta.", "Den här begäran kunde inte kontrolleras. Öppna länken igen och signera från den sidan.", "No se pudo comprobar esta solicitud. Abra el enlace de nuevo y firme desde esa página."]),
    ("closed", ["This request is closed. It was declined or has expired, or it no longer needs your signature. Ask the sender if you think this is a mistake.", "Diese Anfrage ist geschlossen. Sie wurde abgelehnt oder ist abgelaufen oder braucht Ihre Unterschrift nicht mehr. Fragen Sie den Absender, wenn Sie glauben, dass es sich um einen Fehler handelt.", "Cette demande est clôturée. Elle a été refusée ou a expiré, ou elle n'a plus besoin de votre signature. Demandez à l'expéditeur si vous pensez qu'il s'agit d'une erreur.", "Ta prośba jest zamknięta. Została odrzucona lub wygasła albo nie wymaga już Twojego podpisu. Zapytaj nadawcę, jeśli uważasz, że to pomyłka.", "Tämä pyyntö on suljettu. Se on hylätty tai vanhentunut, tai se ei enää tarvitse allekirjoitustasi. Kysy lähettäjältä, jos luulet, että tämä on virhe.", "Den här begäran är stängd. Den har avslagits eller gått ut, eller behöver inte längre din signatur. Fråga avsändaren om du tror att det är ett misstag.", "Esta solicitud está cerrada. Fue rechazada o ha caducado, o ya no necesita su firma. Pregunte al remitente si cree que es un error."]),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The keys the signer door and its page may ask for (D92, section 4.1), in the design's
    /// order. A key outside the list, or one missing from it, fails here.
    const KEYS: [&str; 24] = [
        "title",
        "document",
        "open_full",
        "no_viewer",
        "fingerprint",
        "name_label",
        "name_hint",
        "consent",
        "submit",
        "records",
        "err_name",
        "err_agree",
        "done_title",
        "done_body",
        "gone_expired",
        "gone_revoked",
        "changed",
        "unknown",
        "busy",
        "unavailable",
        "request",
        "for",
        "forbidden",
        "closed",
    ];

    /// The `{placeholders}` of a text, in order of appearance.
    fn placeholders(s: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = s;
        while let Some(i) = rest.find('{') {
            let tail = &rest[i + 1..];
            let j = tail.find('}').expect("unclosed {");
            out.push(tail[..j].to_string());
            rest = &tail[j + 1..];
        }
        out
    }

    #[test]
    fn the_key_list_is_the_fixed_one() {
        let mut have: Vec<&str> = CATALOGUE.iter().map(|(k, _)| *k).collect();
        let mut want: Vec<&str> = KEYS.to_vec();
        have.sort_unstable();
        want.sort_unstable();
        assert_eq!(have, want);
        let mut seen = std::collections::BTreeSet::new();
        for (k, _) in CATALOGUE {
            assert!(seen.insert(*k), "key {k} appears twice");
        }
    }

    #[test]
    fn every_text_exists_in_every_language_with_the_same_placeholders() {
        for (key, cols) in CATALOGUE {
            let want = placeholders(cols[0]);
            for (i, c) in cols.iter().enumerate() {
                assert!(!c.trim().is_empty(), "{key} is empty in {}", LANGS[i]);
                assert_eq!(
                    placeholders(c),
                    want,
                    "{key}: the placeholders of {} differ from English",
                    LANGS[i]
                );
                for ph in placeholders(c) {
                    assert!(
                        ["title", "name", "hash", "time"].contains(&ph.as_str()),
                        "{key}: unknown placeholder {{{ph}}} in {}",
                        LANGS[i]
                    );
                }
            }
        }
    }

    #[test]
    fn a_translation_is_not_a_copy_of_the_english_text() {
        for (key, cols) in CATALOGUE {
            for (i, c) in cols.iter().enumerate().skip(1) {
                assert_ne!(*c, cols[0], "{key} in {} is the English text", LANGS[i]);
            }
        }
    }

    #[test]
    fn a_text_is_one_plain_line_with_no_markup() {
        for (key, cols) in CATALOGUE {
            for (i, c) in cols.iter().enumerate() {
                assert!(
                    !c.contains(['<', '>', '\n', '\r', '\t', '"']),
                    "{key} in {} holds markup, a quote or a line break",
                    LANGS[i]
                );
                assert!(
                    c.len() <= 600,
                    "{key} in {} is longer than 600 bytes",
                    LANGS[i]
                );
                assert_eq!(c.trim(), *c, "{key} in {} has outer whitespace", LANGS[i]);
            }
        }
    }

    #[test]
    fn the_consent_sentence_names_the_requested_person_in_every_language() {
        // Attacker finding S7: the signer asserts who they are; the sentence carries the name of
        // the person the request is for.
        let (_, cols) = CATALOGUE.iter().find(|(k, _)| *k == "consent").unwrap();
        for (i, c) in cols.iter().enumerate() {
            assert!(
                c.contains("{name}"),
                "consent in {} lacks {{name}}",
                LANGS[i]
            );
        }
        let (_, cols) = CATALOGUE.iter().find(|(k, _)| *k == "for").unwrap();
        for (i, c) in cols.iter().enumerate() {
            assert!(c.contains("{name}"), "for in {} lacks {{name}}", LANGS[i]);
        }
    }

    #[test]
    fn the_notice_of_what_is_recorded_says_what_it_is_not() {
        // The record is evidence, not a proof of identity; the wording must keep the words that
        // say so ("simple" and the link as the only check) in every language.
        let (_, cols) = CATALOGUE.iter().find(|(k, _)| *k == "records").unwrap();
        let needles = [
            [
                "simple",
                "einfache",
                "simple",
                "zwykły",
                "yksinkertainen",
                "enkel",
                "simple",
            ],
            ["link", "link", "lien", "link", "linkki", "länken", "enlace"],
        ];
        for (i, c) in cols.iter().enumerate() {
            for set in &needles {
                assert!(
                    c.to_lowercase().contains(set[i]),
                    "records in {} lost the word {:?}",
                    LANGS[i],
                    set[i]
                );
            }
        }
    }

    #[test]
    fn text_fill_and_html() {
        assert_eq!(text("de", "submit"), "Unterschreiben");
        assert_eq!(
            text("xx", "submit"),
            "Sign",
            "an unknown language gives English"
        );
        assert_eq!(text("en", "no_such_key"), "");
        assert_eq!(
            fill("en", "for", &[("name", "Ada & Bob")]),
            "This request is for Ada & Bob."
        );
        let h = html(
            "en",
            "request",
            &[("title", "<script>alert(1)</script>\"'")],
        );
        assert!(!h.contains(['<', '"', '\'']), "{h}");
        assert!(h.contains("&lt;script&gt;"), "{h}");
        // A value that looks like a placeholder is not expanded a second time.
        let h = html("en", "for", &[("name", "{title}")]);
        assert_eq!(h, "This request is for {title}.");
    }

    #[test]
    fn the_language_is_the_parameter_then_the_header_then_the_default_then_english() {
        assert_eq!(choose_language(Some("de"), Some("fr"), Some("sv")), "de");
        assert_eq!(choose_language(Some("DE-at"), None, None), "de");
        assert_eq!(
            choose_language(Some("xx"), Some("fi-FI,en;q=0.5"), Some("sv")),
            "fi"
        );
        assert_eq!(
            choose_language(None, Some("pl;q=0.3, sv;q=0.8"), None),
            "sv"
        );
        assert_eq!(
            choose_language(None, Some("en;q=0.2, es;q=0.2"), None),
            "en"
        );
        assert_eq!(
            choose_language(None, Some("es;q=0.2, en;q=0.2"), None),
            "es"
        );
        // Neither the parameter nor the header names a language we have: the instance default.
        assert_eq!(
            choose_language(None, Some("ja, zh;q=0.9"), Some("fr")),
            "fr"
        );
        assert_eq!(choose_language(Some("??"), None, Some("pl-PL")), "pl");
        // The header's own English beats the instance default.
        assert_eq!(choose_language(None, Some("en"), Some("fr")), "en");
        // Nothing usable anywhere.
        assert_eq!(choose_language(None, None, None), "en");
        assert_eq!(
            choose_language(None, Some("*;q=nan,,;q=0"), Some("klingon")),
            "en"
        );
        assert_eq!(choose_language(Some(""), Some("de;q=0"), None), "en");
    }

    #[test]
    fn the_consent_version_names_the_draft_the_language_and_the_wording() {
        assert_eq!(CONSENT_VERSION, "v1-draft");
        for l in LANGS {
            let id = consent_version_for(l);
            let parts: Vec<&str> = id.split('/').collect();
            assert_eq!(parts.len(), 3, "{id}");
            assert_eq!(parts[0], "v1-draft", "{id}");
            assert_eq!(parts[1], l, "{id}");
            assert!(
                parts[2].len() == 8 && parts[2].bytes().all(|b| b.is_ascii_hexdigit()),
                "{id}"
            );
        }
        // An unknown language is recorded as English, words included.
        assert_eq!(consent_version_for("zz"), consent_version_for("en"));
        assert!(consent_version_for("zz").starts_with("v1-draft/en/"));
        // Seven languages, seven different ids.
        let mut all: Vec<String> = LANGS.iter().map(|l| consent_version_for(l)).collect();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), LANGS.len());
    }

    /// The ids of today's words, per language in the order of [`LANGS`]. The id is derived from
    /// the texts, so an edit of any text of [`WORDING_KEYS`] changes it by itself; this table is
    /// the second net: the edit also fails this test, which names the language, so that no one
    /// changes the words a signer agrees to without seeing it in the diff. Put the new ids
    /// here when the change is meant. (Nothing real has been signed under these ids yet.)
    const PINNED: [&str; 7] = [
        "v1-draft/en/a05c68c3",
        "v1-draft/de/9ebff173",
        "v1-draft/fr/31aa87b0",
        "v1-draft/pl/c3127377",
        "v1-draft/fi/a670b631",
        "v1-draft/sv/9e91bef0",
        "v1-draft/es/08f87264",
    ];

    #[test]
    fn the_wording_the_signer_agrees_to_is_pinned_to_its_id() {
        let have: Vec<String> = LANGS.iter().map(|l| consent_version_for(l)).collect();
        for (i, lang) in LANGS.iter().enumerate() {
            assert_eq!(
                have[i], PINNED[i],
                "a text of WORDING_KEYS changed in `{lang}`: the id is now different (that is what                  it is for). If the change is meant, put the new ids in PINNED:
{have:#?}"
            );
        }
    }

    #[test]
    fn every_text_that_the_id_covers_exists_and_none_is_left_out_by_accident() {
        for key in WORDING_KEYS {
            assert!(
                CATALOGUE.iter().any(|(k, _)| *k == key),
                "{key} is not a key"
            );
        }
        let mut seen = WORDING_KEYS.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), WORDING_KEYS.len(), "a key twice");
        // The consent sentence names the button: the two must be one word in every language.
        let col = |key: &str, i: usize| CATALOGUE.iter().find(|(k, _)| *k == key).unwrap().1[i];
        for (i, lang) in LANGS.iter().enumerate() {
            assert!(
                col("consent", i).contains(col("submit", i)),
                "the consent sentence in `{lang}` does not name the button {:?}",
                col("submit", i)
            );
        }
    }

    #[test]
    fn the_id_changes_with_every_text_it_covers_and_with_nothing_else() {
        let base: Vec<String> = (0..LANGS.len()).map(|i| wording_id(CATALOGUE, i)).collect();
        let edit = |key: &str, i: usize| -> Vec<(&'static str, [&'static str; 7])> {
            let mut cat: Vec<(&'static str, [&'static str; 7])> = CATALOGUE.to_vec();
            let row = cat.iter_mut().find(|(k, _)| *k == key).unwrap();
            row.1[i] = Box::leak(format!("{} (edited)", row.1[i]).into_boxed_str());
            cat
        };
        for key in WORDING_KEYS {
            for (i, edited) in LANGS.iter().enumerate() {
                let cat = edit(key, i);
                for (j, other) in LANGS.iter().enumerate() {
                    let id = wording_id(&cat, j);
                    if j == i {
                        assert_ne!(id, base[j], "editing {key} in {edited} left the id");
                    } else {
                        assert_eq!(id, base[j], "editing {key} in {edited} moved {other}");
                    }
                }
            }
        }
        // A text the id does not cover (the title, a refusal) does not move it.
        for key in ["title", "done_body", "gone_expired", "busy"] {
            let cat = edit(key, 0);
            assert_eq!(wording_id(&cat, 0), base[0], "{key} moved the id");
        }
    }

    #[test]
    fn the_machine_drafted_list_is_every_language_but_english() {
        let mut want: Vec<&str> = LANGS.iter().copied().filter(|l| *l != "en").collect();
        let mut have: Vec<&str> = MACHINE_DRAFTED.to_vec();
        want.sort_unstable();
        have.sort_unstable();
        assert_eq!(have, want);
    }

    #[test]
    fn the_refusal_of_a_foreign_post_says_what_the_door_tests_look_for_and_what_to_do() {
        let en = text("en", "forbidden");
        assert_eq!(
            en,
            "This request could not be checked. Open the link again and sign from that page."
        );
        for (i, lang) in LANGS.iter().enumerate() {
            let t = text(lang, "forbidden");
            assert!(!t.contains('{'), "forbidden in {lang} takes no value");
            // Two sentences: what happened, then what to do (the link, the page).
            assert!(
                t.matches('.').count() >= 2,
                "forbidden in {lang} is not two sentences: {t}"
            );
            assert_ne!(
                t,
                text(lang, "gone_revoked"),
                "forbidden in {lang} reuses another message"
            );
            if i > 0 {
                assert_ne!(t, en, "forbidden in {lang} is the English text");
            }
        }
    }

    #[test]
    fn the_closed_request_has_its_own_text_and_does_not_borrow_the_dead_link_message() {
        let en = text("en", "closed");
        assert!(en.starts_with("This request is closed."));
        assert!(en.contains("Ask the sender"));
        for (i, lang) in LANGS.iter().enumerate() {
            let t = text(lang, "closed");
            assert!(!t.contains('{'), "closed in {lang} takes no value");
            assert_ne!(
                t,
                text(lang, "gone_revoked"),
                "closed in {lang} reuses the message of a dead link"
            );
            assert_ne!(
                t,
                text(lang, "gone_expired"),
                "closed in {lang} reuses the message of an expired link"
            );
            // It names a reason and sends the person to the sender: more than one sentence.
            assert!(
                t.matches('.').count() >= 3,
                "closed in {lang} is too short to say what happened and what to do: {t}"
            );
            if i > 0 {
                assert_ne!(t, en, "closed in {lang} is the English text");
            }
        }
    }
}
