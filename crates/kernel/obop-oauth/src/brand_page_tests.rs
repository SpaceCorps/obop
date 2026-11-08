//! The pages of the authorization flow with a brand set (docs/spec/branding.md, slice S1c). The
//! pages are pure functions of a [`FlowCtx`], so these tests render them directly: the brand is
//! written through `brand::apply` (what `instance_brand.configure` does) or put into the store as it
//! is (a row written around the command), and read back through a [`Reader`], which is the only way
//! a [`Shown`] is made. What an instance with **no** row sends is held by the golden pins
//! (`golden_tests.rs`), which this file leaves alone; the tests here that have no brand compare a
//! page with the page of a brand only to say what changed.
//!
//! The handlers and the real router are tested in the `obop` crate (`oauth_tests/brand.rs`).

use crate::client::Class;
use crate::flow::FlowCtx;
use crate::page::{
    CATALOGUE, LANGS, Rendered, consent_page, error_page_with, esc, login_page, mfa_page, t,
};
use crate::redirect::RedirectForm;
use crate::util::clean_text;
use obop_brand::Brand;
use obop_commands::brand::{self, Action, Reader, Shown};
use obop_storage::{Backend, Fields, Op};
use serde_json::{Value as J, json};
use std::sync::Arc;
use std::time::Duration;

/// The artwork of the placeholder logo (a plain "M") in the canonical form the writer prints.
const ARTWORK: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\"><rect width=\"64\" height=\"64\" rx=\"14\" fill=\"#495670\"/><path d=\"M 16 48 L 16 16 L 24 16 L 32 30 L 40 16 L 48 16 L 48 48 L 41 48 L 41 29 L 32 44 L 23 29 L 23 48 Z\" fill=\"#ffffff\"/></svg>";

fn memory() -> Arc<dyn Backend> {
    obop_storage::open("memory:").unwrap()
}

/// A brand written through `brand::apply`, as the command writes it, and read back through a
/// reader.
fn shown(pairs: &[(&str, &str)]) -> Arc<Shown> {
    let b = memory();
    let reader = Reader::with_bound(b.clone(), Duration::ZERO);
    let supplied = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    brand::apply(
        &*b,
        Some(&reader),
        "operator",
        &Action::Configure(supplied),
        false,
    )
    .unwrap_or_else(|e| panic!("the brand was refused: {e}"));
    reader.current()
}

/// A row put into the store as it is, whatever it holds (a restore, an import, a direct edit, another
/// binary), read back through a reader.
fn row_around(pairs: &[(&str, J)]) -> Arc<Shown> {
    let b = memory();
    let fields: Fields = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    b.apply_checked(&[Op::Put {
        model: brand::MODEL.into(),
        id: brand::ROW_ID.into(),
        fields,
    }])
    .unwrap();
    Reader::with_bound(b, Duration::ZERO).current()
}

/// What an instance with no row gives a page: a reader over an empty store.
fn plain() -> Arc<Shown> {
    let shown = Reader::with_bound(memory(), Duration::ZERO).current();
    assert!(!shown.is_set());
    shown
}

fn mohv() -> Arc<Shown> {
    shown(&[("name", "MOHV"), ("logo_svg", ARTWORK)])
}

/// The kinds of page, in the order of the golden pins. A test below holds the list to theirs.
const KINDS: [&str; 13] = [
    "login_mcp",
    "login_mobile_ms",
    "mfa_code",
    "mfa_passkey",
    "consent_mcp",
    "consent_mobile",
    "consent_loopback",
    "consent_capped",
    "err_client",
    "err_redirect",
    "err_gone",
    "err_forbidden",
    "err_busy",
];

/// A flow in the state a page is rendered for. The redirect hosts name no product, so the visible
/// text of a page can be held to name none.
fn ctx(kind: &str, client: &str, lang: &str, brand: &Arc<Shown>) -> FlowCtx {
    let web = kind.ends_with("mcp");
    let loopback = kind == "consent_loopback";
    let mobile = !web;
    let (origin, host) = if web {
        ("https://claude.ai", "claude.ai")
    } else if loopback {
        ("http://127.0.0.1:53682", "127.0.0.1:53682")
    } else {
        ("se.mohv.app:", "se.mohv.app")
    };
    let signed_in = kind.starts_with("consent") || kind.starts_with("mfa");
    FlowCtx {
        id: "flow-0001".into(),
        csrf: "csrf-0001".into(),
        lang: lang.into(),
        client_name: client.into(),
        redirect_origin: origin.into(),
        redirect_host: host.into(),
        redirect_form: if web {
            RedirectForm::Web
        } else if loopback {
            RedirectForm::Loopback
        } else {
            RedirectForm::PrivateUse
        },
        user: signed_in.then(|| "ada".to_string()),
        class: if mobile { Class::Mobile } else { Class::Mcp },
        groups: vec!["base.user".into(), "sales.user".into()],
        preselected: vec!["base.user".into()],
        microsoft: mobile,
        max_days: if kind == "consent_capped" { 30 } else { 90 },
        brand: brand.clone(),
    }
}

fn render(kind: &str, client: &str, lang: &str, brand: &Arc<Shown>) -> Rendered {
    let c = ctx(kind, client, lang, brand);
    match kind {
        "login_mcp" | "login_mobile_ms" => login_page(&c, None),
        "mfa_code" => mfa_page(&c, "mfa-token-0001", None, false),
        "mfa_passkey" => mfa_page(&c, "mfa-token-0001", None, true),
        k if k.starts_with("consent") => consent_page(&c, None),
        k => error_page_with(brand, lang, &format!("err.{}", &k[4..])),
    }
}

/// The text of a page a person reads: scripts and style sheets taken out, then every tag (so the
/// hidden inputs and every attribute go with them).
fn visible(html: &str) -> String {
    let mut s = html.to_string();
    for (open, close) in [("<script", "</script>"), ("<style", "</style>")] {
        while let Some(a) = s.find(open) {
            let b = s[a..].find(close).map_or(s.len(), |i| a + i + close.len());
            s.replace_range(a..b, " ");
        }
    }
    let (mut out, mut in_tag) = (String::new(), false);
    for c in s.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' if in_tag => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// `html` without its first stretch from `from` to `to` (both included).
fn cut(html: &str, from: &str, to: &str) -> String {
    let a = html
        .find(from)
        .unwrap_or_else(|| panic!("no {from:?} in {html}"));
    let b = html[a..]
        .find(to)
        .unwrap_or_else(|| panic!("no {to:?} after {from:?}"))
        + a
        + to.len();
    format!("{}{}", &html[..a], &html[b..])
}

/// What a brand changes on a page, taken out: the brand line, the icon link, and the texts that
/// name the product (the lead of the sign-in page, the box and the second fact of the consent page).
/// What is left is what the brand must not touch.
fn without_what_a_brand_changes(html: &str, kind: &str) -> String {
    let mut s = html.to_string();
    s = cut(&s, "<p class=\"brand\">", "</p>");
    if s.contains("<link rel=\"icon\"") {
        s = cut(&s, "<link rel=\"icon\"", ">");
    }
    if kind.starts_with("login") {
        // The lead is the first paragraph after the heading.
        let at = s.find("</h1><p>").unwrap() + "</h1>".len();
        let end = s[at..].find("</p>").unwrap() + at + "</p>".len();
        s.replace_range(at..end, "");
    }
    if kind.starts_with("consent") {
        s = cut(&s, "<div class=\"notice\">", "</div>");
        let ul = s.find("<ul>").unwrap();
        let second = s[ul..].match_indices("<li>").nth(1).unwrap().0 + ul;
        let end = s[second..].find("</li>").unwrap() + second + "</li>".len();
        s.replace_range(second..end, "");
    }
    s
}

/// The text of `key` as the page writes it: escaped, then the placeholders filled with the escaped
/// values in `<strong>`.
fn filled(lang: &str, key: &str, app: &str, host: &str) -> String {
    esc(t(lang, key))
        .replace("{app}", &format!("<strong>{}</strong>", esc(app)))
        .replace("{host}", &format!("<strong>{}</strong>", esc(host)))
}

fn notice(html: &str) -> String {
    let a = html.find("<div class=\"notice\">").unwrap();
    let b = html[a..].find("</div>").unwrap() + a + "</div>".len();
    html[a..b].to_string()
}

// ---- The list of kinds -----------------------------------------------------------------------------

#[test]
fn the_kinds_of_page_here_are_the_kinds_of_the_golden_pins() {
    assert_eq!(KINDS.to_vec(), crate::golden_tests::kind_names());
}

// ---- With a brand, no obop --------------------------------------------------------------------------

/// Spec section 13, "With a brand, no obop": with the name MOHV and an artwork, every page of the
/// slice, in every language, shows no obop to the person, and shows the name and the logo.
#[test]
fn with_a_brand_the_visible_text_of_every_page_holds_no_obop_in_seven_languages() {
    let brand = mohv();
    for kind in KINDS {
        for lang in LANGS {
            let page = render(kind, "Claude", lang, &brand);
            let text = visible(&page.html);
            assert!(
                !text.to_lowercase().contains("obop"),
                "{kind}/{lang} names the product: {text}"
            );
            // Nor does anything else of the page: not an attribute, not the policy.
            assert!(
                !page.html.to_lowercase().contains("obop")
                    && !page.csp.to_lowercase().contains("obop"),
                "{kind}/{lang} holds obop outside the visible text"
            );
            // The name and the logo are there: in the line above the card, the logo first.
            let line = &page.html[page.html.find("<p class=\"brand\">").unwrap()..];
            let line = &line[..line.find("</p>").unwrap()];
            assert!(
                line.contains("<svg") && line.ends_with("MOHV"),
                "{kind}/{lang}: {line}"
            );
            assert!(
                line.find("<svg").unwrap() < line.find("MOHV").unwrap(),
                "{kind}/{lang}: the logo comes first"
            );
            assert!(text.contains("MOHV"));
        }
    }
}

#[test]
fn with_no_brand_the_line_is_the_default_name_and_no_twin_shows() {
    let brand = plain();
    let twins = [
        "login.lead.neutral",
        "consent.who_body.neutral",
        "consent.f2.neutral",
        "consent.who_body_mobile.neutral",
        "consent.who_body_mobile_loopback.neutral",
        "consent.same_name",
    ];
    for kind in KINDS {
        for lang in LANGS {
            let page = render(kind, "Zed", lang, &brand);
            assert!(
                page.html.contains("<p class=\"brand\">obop</p>"),
                "{kind}/{lang}"
            );
            assert!(!page.html.contains("<svg") && !page.html.contains("rel=\"icon\""));
            assert!(!page.csp.contains("img-src"), "{kind}/{lang}");
            for key in twins {
                let text = filled(lang, key, "Zed", "claude.ai");
                assert!(!page.html.contains(&text), "{kind}/{lang} shows {key}");
            }
            // The pages that always named the product still do: the line, and for the sign-in and
            // consent pages the sentences.
            let sentences = kind.starts_with("login") || kind.starts_with("consent");
            let named = page.html.to_lowercase().matches("obop").count();
            assert!(named >= 1 && (named > 1 || !sentences), "{kind}/{lang}");
        }
    }
}

/// The neutral wording is the wording of a brand that is set, in every language, also for a language
/// whose pages would otherwise name the product in a compound; the sentences it replaces are on no
/// page.
#[test]
fn with_a_brand_the_sentences_that_name_the_product_use_their_twins() {
    let brand = mohv();
    for lang in LANGS {
        for (kind, base, host) in [
            ("login_mcp", "login.lead", "claude.ai"),
            ("login_mobile_ms", "login.lead", "se.mohv.app"),
            ("consent_mcp", "consent.who_body", "claude.ai"),
            ("consent_mobile", "consent.who_body_mobile", "se.mohv.app"),
            (
                "consent_loopback",
                "consent.who_body_mobile_loopback",
                "127.0.0.1:53682",
            ),
            ("consent_mcp", "consent.f2", "claude.ai"),
        ] {
            let html = render(kind, "Claude", lang, &brand).html;
            let twin = filled(lang, &format!("{base}.neutral"), "Claude", host);
            assert!(html.contains(&twin), "{kind}/{lang}: the twin of {base}");
            let old = filled(lang, base, "Claude", host);
            assert!(!html.contains(&old), "{kind}/{lang}: the text of {base}");
        }
        // The other facts of a mobile app's page are not twins: f2_mobile stays.
        let phone = render("consent_mobile", "Claude", lang, &brand).html;
        assert!(phone.contains(&esc(t(lang, "consent.f2_mobile"))), "{lang}");
    }
}

// ---- The brand stays a label -------------------------------------------------------------------------

/// Spec section 13, "The brand stays a label": a consent render with the brand name `Zed` and a client
/// named `Acme`. `Zed` occurs only in the brand line; the sentence that names the client, the
/// headings and the buttons are byte for byte those of the page without a brand.
#[test]
fn the_brand_stays_a_label_and_the_rest_of_the_page_is_the_page_without_a_brand() {
    let zed = shown(&[("name", "Zed")]);
    let plain = plain();
    for kind in KINDS.iter().filter(|k| !k.starts_with("err")) {
        for lang in LANGS {
            let with = render(kind, "Acme", lang, &zed);
            let without = render(kind, "Acme", lang, &plain);
            let line = &with.html[with.html.find("<p class=\"brand\">").unwrap()..];
            assert!(line[..line.find("</p>").unwrap()].contains("Zed"));
            let rest = without_what_a_brand_changes(&with.html, kind);
            assert!(
                !rest.contains("Zed"),
                "{kind}/{lang}: Zed outside the brand line"
            );
            assert_eq!(
                rest,
                without_what_a_brand_changes(&without.html, kind),
                "{kind}/{lang}: the brand changed more than the line, the icon and the wording"
            );
            // The sentence that names the client is one of the twins, with the client's name and
            // never the brand's.
            if kind.starts_with("consent") {
                let b = notice(&with.html);
                assert!(
                    b.matches("<strong>Acme</strong>").count() >= 2,
                    "{kind}/{lang}"
                );
                assert!(!b.contains("Zed"));
            }
        }
    }
    // Error pages: the line, the icon and the policy are all that differ.
    for kind in KINDS.iter().filter(|k| k.starts_with("err")) {
        for lang in LANGS {
            let with = render(kind, "Acme", lang, &zed);
            let without = render(kind, "Acme", lang, &plain);
            assert_eq!(
                without_what_a_brand_changes(&with.html, kind),
                without_what_a_brand_changes(&without.html, kind),
                "{kind}/{lang}"
            );
        }
    }
}

// ---- Consent -----------------------------------------------------------------------------------------

/// The claim of the box is kept for each class of client, in every language: the box, the host line
/// and the second mention of the client's name are there, the host or scheme is the one the
/// application registered.
#[test]
fn consent_with_a_brand_keeps_the_box_the_host_line_and_the_claim_for_each_class() {
    let brand = mohv();
    for (kind, host) in [
        ("consent_mcp", "claude.ai"),
        ("consent_mobile", "se.mohv.app"),
        ("consent_loopback", "127.0.0.1:53682"),
    ] {
        for lang in LANGS {
            let page = render(kind, "Acme", lang, &brand);
            let b = notice(&page.html);
            assert!(
                b.starts_with("<div class=\"notice\"><strong>"),
                "{kind}/{lang}"
            );
            assert!(
                b.contains(&format!("<strong>{host}</strong>")),
                "{kind}/{lang}: the host line"
            );
            assert!(
                b.matches("<strong>Acme</strong>").count() >= 2,
                "{kind}/{lang}: the application is named twice, in the claim too"
            );
            // The heading of the box is the one of the page without a brand.
            assert!(b.contains(&esc(t(lang, "consent.who"))));
            // The buttons are the ones that were there.
            for key in ["consent.deny", "consent.allow"] {
                assert!(
                    page.html
                        .contains(&format!(">{}</button>", esc(t(lang, key))))
                );
            }
        }
    }
}

/// Each of the three "who" twins names the application at least twice in every language, in the
/// claim sentence too: a pronoun could be read as the instance. The catalogue keeps the claim and the
/// rest of the box in one string per key, so the test reads the cells.
#[test]
fn the_three_who_twins_name_the_application_at_least_twice_in_every_language() {
    for key in [
        "consent.who_body.neutral",
        "consent.who_body_mobile.neutral",
        "consent.who_body_mobile_loopback.neutral",
    ] {
        let (_, cells) = CATALOGUE.iter().find(|(k, _)| *k == key).unwrap();
        for (cell, lang) in cells.iter().zip(LANGS) {
            assert!(
                cell.matches("{app}").count() >= 2,
                "{key} in {lang}: {cell}"
            );
            assert_eq!(cell.matches("{host}").count(), 1, "{key} in {lang}");
        }
    }
}

/// A twin has the placeholders of the text it stands for, in every language, so the same arguments
/// fill both; and the one fixed sentence has none.
#[test]
fn a_twin_has_the_placeholders_of_its_base_and_the_same_name_sentence_has_none() {
    let cells = |key: &str| CATALOGUE.iter().find(|(k, _)| *k == key).unwrap().1;
    for base in [
        "login.lead",
        "consent.who_body",
        "consent.f2",
        "consent.who_body_mobile",
        "consent.who_body_mobile_loopback",
    ] {
        let (b, n) = (cells(base), cells(&format!("{base}.neutral")));
        for i in 0..LANGS.len() {
            for ph in ["{app}", "{user}", "{host}", "{days}", "{name}"] {
                assert_eq!(
                    b[i].contains(ph),
                    n[i].contains(ph),
                    "{base} and its twin in {}: placeholder {ph}",
                    LANGS[i]
                );
            }
            assert_ne!(
                b[i], n[i],
                "{base}: the twin is another text in {}",
                LANGS[i]
            );
        }
    }
    for text in cells("consent.same_name") {
        assert!(!text.contains('{'), "{text}");
    }
    // Every cell of the six new keys is another text than the English one in the six languages: they
    // are drafts, and an English cell left in a column is the mistake to catch.
    for key in [
        "login.lead.neutral",
        "consent.who_body.neutral",
        "consent.f2.neutral",
        "consent.who_body_mobile.neutral",
        "consent.who_body_mobile_loopback.neutral",
        "consent.same_name",
    ] {
        let c = cells(key);
        for i in 1..LANGS.len() {
            assert_ne!(c[i], c[0], "{key} is still English in {}", LANGS[i]);
        }
    }
}

// ---- A client named like the instance ----------------------------------------------------------------

#[test]
fn a_client_named_like_the_brand_gets_the_fixed_sentence_in_every_language() {
    let zed = shown(&[("name", "Zed Co"), ("title_sv", "Zed Mileage")]);
    for client in [
        "Zed Co",
        "zed co",
        "ZED CO",
        "ＺＥＤ ＣＯ",
        // The slug is the instance's other name.
        "zed-co",
        // A title in another language than the page's.
        "Zed Mileage",
        // The characters that registration lets through, and the ones it drops.
        "Zed\u{200D} Co",
        "Zed\u{200C}Co",
        "Z\u{00AD}ed Co",
        "Zed Co\u{FE0F}",
        "Zed\u{00A0}Co",
        "Zed\u{3164}Co",
        "Zed Co\u{E0080}",
    ] {
        for lang in LANGS {
            let sentence = format!("<p>{}</p>", esc(t(lang, "consent.same_name")));
            for kind in ["consent_mcp", "consent_mobile", "consent_loopback"] {
                let page = render(kind, client, lang, &zed);
                assert!(
                    notice(&page.html).ends_with(&format!("{sentence}</div>")),
                    "{client:?} {kind}/{lang}: no sentence"
                );
            }
        }
    }
}

#[test]
fn a_client_that_only_contains_the_name_or_has_another_gets_none() {
    let zed = shown(&[("name", "Zed Co"), ("title_sv", "Zed Mileage")]);
    for client in [
        "Zed Support",
        "Acme",
        "Zed Co.",
        "Zed",
        "Z-e-d Co",
        "Zed Mileage Co",
        "",
    ] {
        for lang in LANGS {
            let sentence = esc(t(lang, "consent.same_name"));
            let page = render("consent_mcp", client, lang, &zed);
            assert!(!page.html.contains(&sentence), "{client:?}/{lang}");
        }
    }
    // With no brand the sentence never appears, whatever the client is called.
    let none = plain();
    for client in ["obop", "OBOP", "Zed", "Claude"] {
        for lang in LANGS {
            let page = render("consent_mcp", client, lang, &none);
            assert!(!page.html.contains(&esc(t(lang, "consent.same_name"))));
        }
    }
}

/// With the sentence the box, the host line and the buttons are what they are for a client of another
/// name: the sentence is added to the box, nothing else moves.
#[test]
fn the_sentence_is_added_to_the_box_and_nothing_else_moves() {
    let zed = shown(&[("name", "Zed")]);
    let other = shown(&[("name", "Other")]);
    for lang in LANGS {
        for kind in ["consent_mcp", "consent_mobile", "consent_loopback"] {
            let named = render(kind, "Zed", lang, &zed).html;
            let not_named = render(kind, "Zed", lang, &other).html;
            let sentence = format!("<p>{}</p>", esc(t(lang, "consent.same_name")));
            assert_eq!(
                notice(&named),
                notice(&not_named).replace("</div>", &format!("{sentence}</div>")),
                "{kind}/{lang}"
            );
            // The rest of the page, once the brand line, the icon and the box are out, is the same.
            let strip = |h: &str| {
                let h = cut(h, "<p class=\"brand\">", "</p>");
                let h = cut(&h, "<link rel=\"icon\"", ">");
                cut(&h, "<div class=\"notice\">", "</div>")
            };
            assert_eq!(strip(&named), strip(&not_named), "{kind}/{lang}");
        }
    }
}

/// SDD2-11 (docs/spec/branding.md section 10, item 2): "the comparison does not rely on the cleaning
/// that registration does" holds for the classes the fold removes (default-ignorable and format
/// characters, white space). Controls and U+2800 are kept by the fold and dropped by the registration
/// (`devices::is_invisible`), and U+FE0F is kept by the registration and dropped by the fold. This
/// crosses the two over every scalar value: the name as registered (`clean_text`, the one function
/// `register` calls) is compared with the brand, and so is the raw name, and the sets of characters
/// that only one side removes are written down, so a change to either function shows here.
#[test]
fn the_name_fold_agrees_with_the_registration_cleaning_over_every_scalar_value() {
    use obop_commands::devices::is_invisible;
    let brand = Brand::new("Zed");
    let mut only_registration = Vec::new();
    let mut only_fold = Vec::new();
    for c in (0..=0x10FFFFu32).filter_map(char::from_u32) {
        let raw = format!("Z{c}ed");
        let stored = clean_text(&raw, 80);
        let (raw_match, stored_match) = (brand.matches_client(&raw), brand.matches_client(&stored));
        // The cleaning only removes characters (and turns white space into spaces), so a name the
        // fold takes for the brand's is still taken for it once registration has cleaned it.
        assert!(
            !raw_match || stored_match,
            "U+{:04X}: matches raw but not as registered",
            c as u32
        );
        if is_invisible(c) {
            assert!(
                stored_match,
                "U+{:04X}: dropped by registration, and the name does not match",
                c as u32
            );
        }
        if c.is_whitespace() {
            assert!(raw_match && stored_match, "U+{:04X}: white space", c as u32);
        }
        if stored_match && !raw_match {
            only_registration.push(c);
        }
        if raw_match && !is_invisible(c) && !c.is_whitespace() {
            only_fold.push(c);
        }
        // A character neither removes is a different name, and is not taken for the brand's.
        if !stored_match {
            assert!(
                !is_invisible(c) && !c.is_whitespace(),
                "U+{:04X} is dropped by registration and the name does not match",
                c as u32
            );
        }
    }
    // What registration drops and the fold keeps: the controls that are not white space, and the
    // Braille blank. A client name never holds them (it is stored as `clean_text` left it), and a
    // brand name cannot (the policy refuses them).
    assert!(
        only_registration
            .iter()
            .all(|c| c.is_control() || *c == '\u{2800}'),
        "{:?}",
        only_registration
            .iter()
            .map(|c| format!("U+{:04X}", *c as u32))
            .collect::<Vec<_>>()
    );
    for c in ['\u{0}', '\u{7}', '\u{1B}', '\u{7F}', '\u{9F}', '\u{2800}'] {
        assert!(only_registration.contains(&c), "U+{:04X}", c as u32);
    }
    // What the fold drops and registration keeps includes the variation selector 16 and the
    // unassigned default-ignorable code points that `clean_text` lets through.
    for c in ['\u{FE0F}', '\u{FFF0}', '\u{E0080}', '\u{E01F0}'] {
        assert!(only_fold.contains(&c), "U+{:04X}", c as u32);
    }
}

/// The same crossing with names from a generator with a fixed seed (no property-test crate): the
/// brand's name, its slug or a title, with characters of the hostile classes put in at random
/// places, and sometimes a letter that makes it another name. The name as registered matches the
/// brand exactly when no letter was put in.
#[test]
fn generated_hostile_names_are_taken_for_the_brand_exactly_when_no_letter_was_added() {
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            // xorshift64*
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }
    let mut brand = Brand::new("Zed Mileage Co").with_slug("zed-mileage");
    brand.set_title("sv", "Zed Körjournal");
    // Characters of the classes under test: white space, invisible and format characters, the
    // variation selectors, the tag characters, controls and the blank cells.
    let pool: Vec<char> = (0..=0x10FFFFu32)
        .filter_map(char::from_u32)
        .filter(|c| {
            c.is_whitespace()
                || obop_commands::devices::is_invisible(*c)
                || matches!(*c as u32, 0xFE0F | 0xFFF0..=0xFFF8 | 0xE0080..=0xE00FF | 0x180E | 0xFEFF)
        })
        .collect();
    assert!(pool.len() > 200, "{}", pool.len());
    let bases = [
        "Zed Mileage Co",
        "zed-mileage",
        "Zed Körjournal",
        "ZED KÖRJOURNAL",
    ];
    let mut rng = Rng(0x5EED_0011_C0DE_D00D);
    let mut matched = 0;
    for round in 0..4000 {
        let mut chars: Vec<char> = bases[rng.below(bases.len())].chars().collect();
        let mut letter_added = false;
        for _ in 0..1 + rng.below(6) {
            let at = rng.below(chars.len() + 1);
            if rng.below(8) == 0 {
                chars.insert(at, ['x', 'q', 'Z', '7'][rng.below(4)]);
                letter_added = true;
            } else {
                chars.insert(at, pool[rng.below(pool.len())]);
            }
        }
        let raw: String = chars.into_iter().collect();
        let stored = clean_text(&raw, 80);
        let is_match = brand.matches_client(&stored);
        assert_eq!(
            is_match, !letter_added,
            "round {round}: {raw:?} stored as {stored:?}"
        );
        if is_match {
            matched += 1;
        }
    }
    assert!(matched > 2500, "{matched}");
}

// ---- The policy, the icon and the logo ---------------------------------------------------------------

#[test]
fn with_a_brand_the_policy_differs_by_the_image_source_alone_and_the_style_hash_is_the_same() {
    let (brand, none) = (mohv(), plain());
    for kind in KINDS {
        for lang in LANGS {
            let with = render(kind, "Claude", lang, &brand);
            let without = render(kind, "Claude", lang, &none);
            assert_eq!(
                with.csp,
                format!("{}; img-src 'self'", without.csp),
                "{kind}/{lang}"
            );
            assert!(!without.csp.contains("img-src"));
            // The style sheet is the constant it was: the same hash, and no new rule or attribute.
            let hash = |csp: &str| {
                let a = csp.find("style-src 'sha256-").unwrap();
                csp[a..].split(';').next().unwrap().to_string()
            };
            assert_eq!(hash(&with.csp), hash(&without.csp));
            assert!(
                !with.html.contains(" style=\""),
                "{kind}/{lang}: a style attribute"
            );
            assert_eq!(with.html.matches("<style>").count(), 1);
        }
    }
}

#[test]
fn the_tab_icon_is_the_logo_route_with_the_version_and_only_with_a_brand() {
    let brand = mohv();
    let v = brand.v().to_string();
    assert_eq!(v.len(), 16);
    for kind in KINDS {
        let with = render(kind, "Claude", "en", &brand).html;
        let link = format!(
            "<link rel=\"icon\" href=\"/api/brand/logo.svg?v={v}\" type=\"image/svg+xml\">"
        );
        assert_eq!(with.matches(&link).count(), 1, "{kind}");
        assert!(
            with.find(&link).unwrap() < with.find("<style>").unwrap(),
            "{kind}: in the head"
        );
        let without = render(kind, "Claude", "en", &plain()).html;
        assert!(!without.contains("rel=\"icon\""));
    }
}

#[test]
fn the_inline_logo_has_the_size_of_the_template_and_the_route_bytes_have_none() {
    let brand = mohv();
    let page = render("login_mcp", "Claude", "en", &brand).html;
    let line = &page[page.find("<p class=\"brand\">").unwrap()..];
    let line = &line[..line.find("</p>").unwrap()];
    assert!(
        line.contains("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"28\" height=\"28\" viewBox=\"0 0 64 64\">"),
        "{line}"
    );
    // The logo is decoration; the name follows it.
    assert!(
        line.starts_with("<p class=\"brand\"><span aria-hidden=\"true\"><svg"),
        "{line}"
    );
    assert!(line.ends_with("</svg></span> MOHV"), "{line}");
    // The bytes of the logo route, and so `v`, carry no width and no height on the root.
    let served = brand.logo().unwrap().served();
    assert!(
        served.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\">"),
        "{served}"
    );
    assert_eq!(served, ARTWORK);
}

// ---- The fallback chain on a page ---------------------------------------------------------------------

#[test]
fn the_message_and_the_title_follow_the_fallback_chain_on_a_real_page() {
    let brand = shown(&[
        ("name", "MOHV"),
        ("message_en", "Welcome to the team"),
        ("title_en", "MOHV Mileage"),
        ("message_fi", "Tervetuloa joukkoon"),
        ("title_de", "MOHV Fahrtenbuch"),
    ]);
    let muted = |m: &str| format!("<p class=\"muted\">{m}</p>");
    let line = |html: &str| {
        let a = html.find("<p class=\"brand\">").unwrap();
        let end = html[a..].find("</p>").unwrap() + a;
        let text = &html[a..end];
        text.rsplit_once("</span> ")
            .map_or(text.to_string(), |(_, n)| n.to_string())
    };
    // Swedish: only an English message is set, and no Swedish title: the English ones.
    let sv = render("login_mcp", "Claude", "sv", &brand).html;
    assert!(sv.contains(&muted("Welcome to the team")));
    assert_eq!(line(&sv), "MOHV Mileage");
    // Finnish has a message of its own and falls back to English for the title.
    let fi = render("login_mcp", "Claude", "fi", &brand).html;
    assert!(fi.contains(&muted("Tervetuloa joukkoon")) && !fi.contains("Welcome to the team"));
    assert_eq!(line(&fi), "MOHV Mileage");
    // German has a title of its own and the English message.
    let de = render("login_mcp", "Claude", "de", &brand).html;
    assert_eq!(line(&de), "MOHV Fahrtenbuch");
    assert!(de.contains(&muted("Welcome to the team")));
    // With no title at all the name is shown; with no message none is, on any language.
    let bare = shown(&[("name", "MOHV")]);
    for lang in LANGS {
        let page = render("login_mcp", "Claude", lang, &bare).html;
        assert_eq!(line(&page), "MOHV", "{lang}");
        assert!(!page.contains("Welcome"), "{lang}");
        // The message sits under the lead of the sign-in page only: the page has the same
        // paragraphs without one as the page with, less the message.
        let with = render("login_mcp", "Claude", lang, &brand).html;
        assert_eq!(
            with.matches("<p class=\"muted\">").count(),
            page.matches("<p class=\"muted\">").count() + 1,
            "{lang}"
        );
    }
    // No other page shows it.
    for kind in KINDS.iter().filter(|k| !k.starts_with("login")) {
        assert!(
            !render(kind, "Claude", "en", &brand)
                .html
                .contains("Welcome to the team"),
            "{kind}"
        );
    }
}

// ---- Rows written around the command --------------------------------------------------------------------

fn brand_text(html: &str) -> String {
    let a = html.find("<p class=\"brand\">").unwrap();
    let end = html[a..].find("</p>").unwrap() + a;
    html[a + "<p class=\"brand\">".len()..end].to_string()
}

#[test]
fn a_script_in_a_stored_name_title_or_message_is_neither_raw_nor_escaped_on_a_page() {
    let brand = row_around(&[
        ("name", json!("<script>alert(1)</script>")),
        ("title_en", json!("<b>Hi</b>")),
        ("message_en", json!("<img src=x onerror=1>")),
    ]);
    assert!(brand.is_set());
    for kind in KINDS {
        for lang in LANGS {
            let html = render(kind, "Claude", lang, &brand).html;
            for needle in [
                "alert(1)",
                "<b>",
                "&lt;b&gt;",
                "onerror",
                "&lt;script",
                "<img",
            ] {
                assert!(!html.contains(needle), "{kind}/{lang}: {needle}");
            }
            // The fields fell back: the name to the default, with the tile of the effective name.
            assert!(
                brand_text(&html).ends_with(" obop"),
                "{kind}/{lang}: {}",
                brand_text(&html)
            );
        }
    }
}

#[test]
fn a_stored_name_that_only_the_full_policy_refuses_is_shown_as_stored_and_escaped() {
    // Latin and Cyrillic in one name: the write refuses it, the read check does not, and a page
    // shows it escaped. An ampersand and an apostrophe are escaped too.
    let brand = row_around(&[("name", json!("M\u{43E}hv & Sons' <ok"))]);
    // `<` is in the safety subset, so this one falls back; a name without it does not.
    assert_eq!(brand.brand().name(), "obop");
    let brand = row_around(&[("name", json!("M\u{43E}hv & Sons'"))]);
    assert_eq!(brand.brand().name(), "M\u{43E}hv & Sons'");
    let html = render("login_mcp", "Claude", "en", &brand).html;
    assert!(
        brand_text(&html).ends_with(" M\u{43E}hv &amp; Sons&#39;"),
        "{}",
        brand_text(&html)
    );
}

#[test]
fn a_stored_artwork_that_fails_the_read_check_is_the_tile_and_its_text_is_on_no_page() {
    for bad in [
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1 1\"><script>alert(1)</script></svg>",
        "</svg><form action=\"https://evil.test\"><input name=x></form>",
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1 1\" onload=\"x()\"/>",
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1 1\"><a href=\"https://evil.test\"/></svg>",
        "\u{FEFF}<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1 1\"/>",
    ] {
        let brand = row_around(&[("name", json!("MOHV")), ("logo_svg", json!(bad))]);
        assert!(brand.logo().unwrap().is_tile(), "{bad}");
        assert!(!brand.fallbacks().is_empty());
        let html = render("consent_mcp", "Claude", "en", &brand).html;
        assert!(html.contains("<svg"), "the tile is drawn");
        for needle in ["script", "evil.test", "onload", "<form action", "<a href"] {
            assert!(!html.contains(needle), "{bad}: {needle}");
        }
        assert!(html.contains(&format!("?v={}", brand.v())));
        assert!(html.contains(">M</text>"), "the glyph of the name");
    }
}

#[test]
fn a_stored_artwork_that_is_valid_but_not_canonical_is_shown_as_the_printer_writes_it() {
    // Extra spaces, a leading point and trailing zeros: valid, and not the canonical text.
    let loose = "<svg  xmlns=\"http://www.w3.org/2000/svg\"   viewBox=\"0 0 64.0 64\" ><rect width=\"64\" height=\"64.000\" rx=\"14\" fill=\"#495670\"/><path d=\"M 16 48 L 16 16 L 24 16 L 32 30 L 40 16 L 48 16 L 48 48 L 41 48 L 41 29 L 32 44 L 23 29 L 23 48 Z\" fill=\"#ffffff\"/></svg>";
    assert_ne!(loose, ARTWORK);
    let brand = row_around(&[("name", json!("MOHV")), ("logo_svg", json!(loose))]);
    assert!(brand.fallbacks().is_empty(), "{:?}", brand.fallbacks());
    let html = render("login_mcp", "Claude", "en", &brand).html;
    assert!(
        !html.contains("64.000") && !html.contains("<svg  "),
        "the stored text is on no page"
    );
    assert!(html.contains("<rect width=\"64\" height=\"64\" rx=\"14\" fill=\"#495670\"/>"));
    assert_eq!(brand.logo().unwrap().served(), ARTWORK);
}

/// A row that exists and holds nothing but the default name is a brand: the neutral wording, the
/// tile and the icon, and the name is shown as the name. Nothing it holds is dropped.
#[test]
fn a_row_that_names_the_default_is_a_brand_all_the_same() {
    let brand = shown(&[("name", "obop")]);
    let html = render("consent_mcp", "Claude", "en", &brand).html;
    assert!(html.contains("rel=\"icon\"") && html.contains("<svg"));
    assert!(html.contains(&esc(t("en", "consent.f2.neutral"))));
    assert!(!html.contains(&esc(t("en", "consent.f2"))));
}

// ---- Helpers of the pins ---------------------------------------------------------------------------

/// The pages of an instance with no row, rendered through a reader over an empty store, are the
/// pages of the golden pins: a reader that gave the pages anything else would move them.
#[test]
fn the_pages_of_an_empty_store_read_through_a_reader_are_the_default_pages() {
    let (none, default) = (plain(), Arc::new(Shown::not_set(None)));
    for kind in KINDS {
        for lang in LANGS {
            let (a, b) = (
                render(kind, "Claude", lang, &none),
                render(kind, "Claude", lang, &default),
            );
            assert_eq!((a.html, a.csp), (b.html, b.csp), "{kind}/{lang}");
        }
    }
}
