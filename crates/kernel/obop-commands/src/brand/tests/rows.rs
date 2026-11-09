//! Rows written around the command (a restore, an import, a direct edit, another binary): what a
//! page shows of them (docs/spec/branding.md, section 13, "Escaping" and "Fallback chain"; section
//! 4, "the read check").

use super::*;
use crate::brand::Shown;
use obop_brand::{logo, svg, tile};
use serde_json::json;

fn shown_of(pairs: &[(&str, J)]) -> (Arc<Shown>, Arc<Spy>) {
    let spy = Spy::memory();
    put_row(&*spy, pairs);
    let (r, _) = reader(&spy, LONG);
    (r.current(), spy)
}

fn fields_of(shown: &Shown) -> Vec<String> {
    shown.fallbacks().iter().map(|f| f.field.clone()).collect()
}

#[test]
fn a_script_in_a_name_and_a_title_falls_back_for_those_fields_only() {
    let (shown, _) = shown_of(&[
        ("name", s("<script>alert(1)</script>")),
        ("title_en", s("<b>Hi</b>")),
        ("title_de", s("Guten Tag")),
        ("message_en", s("Welcome")),
    ]);
    assert!(shown.is_set(), "the row exists, so a brand is set");
    assert_eq!(shown.brand().name(), "obop", "the name fell back");
    assert_eq!(shown.brand().stored_title("en"), "", "the title fell back");
    assert_eq!(
        shown.brand().stored_title("de"),
        "Guten Tag",
        "the rest stays"
    );
    assert_eq!(shown.brand().message("fr"), Some("Welcome"));
    assert_eq!(fields_of(&shown), ["name", "title_en"]);
    assert!(
        shown
            .fallbacks()
            .iter()
            .all(|f| f.code == "unsafe_character")
    );
    // Nothing raw reaches the tile either: the glyph is that of the name it fell back to.
    assert!(shown.logo().unwrap().is_tile());
    assert!(!shown.logo().unwrap().served().contains("script"));
    assert_eq!(
        shown.logo().unwrap().served(),
        tile::svg("obop", None, None),
        "the tile of the effective name (an O), not of the stored one (an S)"
    );
    assert_ne!(
        tile::svg("obop", None, None),
        tile::svg("<script>alert(1)</script>", None, None)
    );
}

#[test]
fn the_tile_of_a_name_that_fell_back_has_the_glyph_of_the_default_name_and_the_theme_accent() {
    let spy = Spy::memory();
    put_theme(&*spy, "#c2410c");
    put_row(&*spy, &[("name", s("<script>alert(1)</script>"))]);
    let (r, _) = reader(&spy, LONG);
    let shown = r.current();
    assert_eq!(shown.brand().name(), "obop");
    assert_eq!(
        shown.logo().unwrap().served(),
        tile::svg("obop", Some("#c2410c"), None)
    );
}

#[test]
fn a_name_that_only_the_full_policy_refuses_is_shown_as_stored() {
    // A Latin and Cyrillic mix (the policy refuses it on a write), a colon and a host-like run: the
    // read side applies only the safety subset, so a policy tightened later never reverts a name.
    for name in ["M\u{041e}HV", "Foo: Bar", "shop.example", "Zed\u{00a9}"] {
        let (shown, _) = shown_of(&[("name", s(name))]);
        assert_eq!(shown.brand().name(), name, "{name:?}");
        assert!(
            shown.fallbacks().is_empty(),
            "{name:?}: {:?}",
            shown.fallbacks()
        );
    }
}

#[test]
fn a_name_that_breaks_the_safety_subset_falls_back() {
    let long = "x".repeat(41);
    for (name, code) in [
        ("", "empty"),
        ("a\u{200b}b", "unsafe_character"),
        ("a\u{202e}b", "unsafe_character"),
        ("a\u{00a0}b", "unsafe_character"),
        ("a\u{00ad}b", "unsafe_character"),
        ("a\nb", "unsafe_character"),
        ("a<b", "unsafe_character"),
        ("a>b", "unsafe_character"),
        ("a\u{e0001}b", "unsafe_character"),
        ("a\u{e000}b", "unsafe_character"),
        (long.as_str(), "too_long"),
    ] {
        let (shown, _) = shown_of(&[("name", s(name))]);
        assert_eq!(shown.brand().name(), "obop", "{name:?}");
        assert_eq!(shown.fallbacks().len(), 1, "{name:?}");
        assert_eq!(shown.fallbacks()[0].code, code, "{name:?}");
    }
}

#[test]
fn the_read_side_limit_of_each_field_is_its_own_counted_in_code_points() {
    // name 40, title 60, message 240: the last length that is shown and the first that falls back,
    // with an ASCII letter and with one that is two bytes long.
    for letter in ["a", "\u{e9}"] {
        for (field, max) in [("name", 40), ("title_pl", 60), ("message_es", 240)] {
            let at_the_limit = letter.repeat(max);
            let (shown, _) = shown_of(&[("name", s("Acme")), (field, s(&at_the_limit))]);
            assert!(
                shown.fallbacks().is_empty(),
                "{field} at {max} {letter:?}: {:?}",
                shown.fallbacks()
            );
            let over = letter.repeat(max + 1);
            let (shown, _) = shown_of(&[("name", s("Acme")), (field, s(&over))]);
            assert_eq!(fields_of(&shown), [field], "{field} at {}", max + 1);
            assert_eq!(shown.fallbacks()[0].code, "too_long", "{field}");
            assert_eq!(shown.fallbacks()[0].reason, "too_long", "{field}");
        }
    }
    // The fallback of a name that is too long is the default name, not a cut of the stored one.
    let (shown, _) = shown_of(&[("name", s(&"a".repeat(41)))]);
    assert_eq!(shown.brand().name(), "obop");
    // A title of 41 to 60 characters and a message of 61 to 240 are not names or titles.
    let (shown, _) = shown_of(&[
        ("name", s("Acme")),
        ("title_en", s(&"t".repeat(50))),
        ("message_en", s(&"m".repeat(200))),
    ]);
    assert!(shown.fallbacks().is_empty(), "{:?}", shown.fallbacks());
}

#[test]
fn a_row_that_exists_without_a_name_is_set_and_says_obop() {
    let (shown, _) = shown_of(&[("title_en", s("Hello"))]);
    assert!(shown.is_set());
    assert_eq!(shown.brand().name(), "obop");
    assert_eq!(fields_of(&shown), ["name"]);
    assert_eq!(shown.fallbacks()[0].code, "empty");
}

#[test]
fn a_stored_slug_that_breaks_the_pattern_falls_back_to_the_derived_slug() {
    let long = "x".repeat(31);
    for slug in [
        "Foo/Bar",
        "9x",
        "foo-",
        "a b",
        "ÄÖ",
        long.as_str(),
        "-x",
        "x_y",
    ] {
        let (shown, _) = shown_of(&[("name", s("Acme Co")), ("slug", s(slug))]);
        assert_eq!(shown.brand().slug(), "acme-co", "{slug:?}");
        assert_eq!(fields_of(&shown), ["slug"], "{slug:?}");
        assert_eq!(shown.fallbacks()[0].code, "invalid_slug");
    }
    let (shown, _) = shown_of(&[("name", s("Acme Co")), ("slug", s("acme"))]);
    assert_eq!(shown.brand().slug(), "acme");
    assert!(shown.fallbacks().is_empty());
}

#[test]
fn a_stored_value_that_is_not_text_is_a_fallback_and_never_a_panic() {
    let (shown, _) = shown_of(&[
        ("name", json!(5)),
        ("slug", json!(["a"])),
        ("title_en", json!({"x": 1})),
        ("message_sv", json!(true)),
        ("logo_svg", json!(12)),
    ]);
    assert!(shown.is_set());
    assert_eq!(shown.brand().name(), "obop");
    assert_eq!(
        fields_of(&shown),
        ["name", "slug", "title_en", "message_sv", "logo_svg"]
    );
    assert!(shown.fallbacks().iter().all(|f| f.code == "not_text"));
    // A null is the same as absent.
    let (shown, _) = shown_of(&[("name", s("Acme")), ("title_en", J::Null)]);
    assert!(shown.fallbacks().is_empty());
}

fn hostile_artworks() -> Vec<(&'static str, String)> {
    let ns = "http://www.w3.org/2000/svg";
    vec![
        (
            "script",
            format!("<svg xmlns=\"{ns}\" viewBox=\"0 0 64 64\"><script>alert(1)</script></svg>"),
        ),
        (
            "a form after the root",
            format!(
                "<svg xmlns=\"{ns}\" viewBox=\"0 0 64 64\"><path d=\"M 0 0\"/></svg></svg><form action=\"https://evil.example/x\"><input name=\"p\"></form>"
            ),
        ),
        (
            "a form in a closed root",
            format!(
                "</svg><form action=\"https://evil.example/x\"></form><svg xmlns=\"{ns}\" viewBox=\"0 0 64 64\"></svg>"
            ),
        ),
        (
            "a link",
            format!(
                "<svg xmlns=\"{ns}\" viewBox=\"0 0 64 64\"><a href=\"https://evil.example\"><path d=\"M 0 0\"/></a></svg>"
            ),
        ),
        (
            "an onload attribute",
            format!(
                "<svg xmlns=\"{ns}\" viewBox=\"0 0 64 64\" onload=\"alert(1)\"><path d=\"M 0 0\"/></svg>"
            ),
        ),
        ("17 KiB", format!("{M_ARTWORK}{}", " ".repeat(17 * 1024))),
        (
            "a number with an exponent",
            format!(
                "<svg xmlns=\"{ns}\" viewBox=\"0 0 64 64\"><rect width=\"1e5\" height=\"4\"/></svg>"
            ),
        ),
        (
            "a number over the bound",
            format!(
                "<svg xmlns=\"{ns}\" viewBox=\"0 0 64 64\"><rect width=\"100000.1\" height=\"4\"/></svg>"
            ),
        ),
        (
            "a number of 25 characters",
            format!(
                "<svg xmlns=\"{ns}\" viewBox=\"0 0 64 64\"><rect width=\"1.23456789012345678901234\" height=\"4\"/></svg>"
            ),
        ),
        ("a byte order mark", format!("\u{feff}{M_ARTWORK}")),
        (
            "an external image",
            format!(
                "<svg xmlns=\"{ns}\" viewBox=\"0 0 64 64\"><image href=\"https://evil.example/p.png\"/></svg>"
            ),
        ),
        ("not svg at all", "just some text".to_string()),
    ]
}

#[test]
fn a_hostile_stored_artwork_is_replaced_by_the_tile_and_listed() {
    for (what, stored) in hostile_artworks() {
        let spy = Spy::memory();
        put_theme(&*spy, "#c2410c");
        put_row(&*spy, &[("name", s("Acme")), ("logo_svg", s(&stored))]);
        let (r, _) = reader(&spy, LONG);
        let shown = r.current();
        let logo_ = shown.logo().unwrap();
        let tile_bytes = tile::svg("Acme", Some("#c2410c"), None);
        assert!(logo_.is_tile(), "{what}");
        assert_eq!(logo_.served(), tile_bytes, "{what}");
        assert_eq!(
            shown.v(),
            logo::version(tile_bytes.as_bytes()),
            "{what}: v is the tile's"
        );
        assert_eq!(fields_of(&shown), ["logo_svg"], "{what}");
        assert!(!shown.fallbacks()[0].reason.is_empty());
        // The stored text appears nowhere: not in what is served, not inline on a page.
        for needle in ["evil.example", "alert", "<form", "<script", "onload"] {
            assert!(!logo_.served().contains(needle), "{what}: {needle}");
            assert!(!logo_.inline(28).contains(needle), "{what}: {needle}");
        }
        // The rest of the brand stays.
        assert_eq!(shown.brand().name(), "Acme", "{what}");
    }
}

#[test]
fn a_valid_artwork_that_is_not_in_canonical_form_is_served_re_serialised() {
    // Extra spaces, a leading point, a trailing point, trailing zeros, six decimals, upper case hex:
    // the output of an older writer, or a row written around the command.
    let stored = "<svg  xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0  64 64\" >\
                  <rect  width=\"64\" height=\"64.000000\" rx=\"14.0\" fill=\"#495670\" />\
                  <path d=\"M .5 .5 L 10.000000 10 L 5. 20.123456 Z\" fill=\"#FFFFFF\" /></svg>";
    let expected = svg::canonical(stored.as_bytes()).expect("the reader accepts this file");
    assert_ne!(
        expected, stored,
        "the case needs a stored text that is not canonical"
    );
    let (shown, _) = shown_of(&[("name", s("Acme")), ("logo_svg", s(stored))]);
    let l = shown.logo().unwrap();
    assert!(!l.is_tile());
    assert_eq!(l.served(), expected, "the printer's output is served");
    assert_eq!(
        shown.v(),
        logo::version(expected.as_bytes()),
        "v is of the bytes served"
    );
    assert!(shown.fallbacks().is_empty(), "a valid file is no fallback");
    assert!(
        !l.served().contains("  "),
        "the stored text appears nowhere"
    );
    assert!(!l.inline(28).contains("64.000000"));
    assert!(l.inline(28).contains("width=\"28\" height=\"28\""));
    assert!(!l.served().contains("width=\"28\""));
}

#[test]
fn a_canonical_artwork_is_served_as_stored() {
    let (shown, _) = shown_of(&[("name", s("Acme")), ("logo_svg", s(M_ARTWORK))]);
    assert_eq!(shown.logo().unwrap().served(), M_ARTWORK);
    assert!(shown.fallbacks().is_empty());
}

#[test]
fn the_tile_follows_the_theme_accent_and_v_follows_the_tile() {
    let spy = Spy::memory();
    put_row(&*spy, &[("name", s("Acme"))]);
    let v_of = |spy: &Arc<Spy>| {
        let (r, _) = reader(spy, LONG);
        r.current().v().to_string()
    };
    let none = v_of(&spy);
    put_theme(&*spy, "#c2410c");
    let orange = v_of(&spy);
    assert_ne!(
        none, orange,
        "a theme change alone changes the tile and so v"
    );
    // The default violet counts as no accent, in either case.
    put_theme(&*spy, "#6d3fd0");
    assert_eq!(v_of(&spy), none);
    put_theme(&*spy, "#6D3FD0");
    assert_eq!(v_of(&spy), none);
    // Something that is not #rrggbb is no accent.
    put_theme(&*spy, "red");
    assert_eq!(v_of(&spy), none);
    // An artwork does not move with the accent.
    put_row(&*spy, &[("name", s("Acme")), ("logo_svg", s(M_ARTWORK))]);
    let art = v_of(&spy);
    put_theme(&*spy, "#c2410c");
    assert_eq!(v_of(&spy), art);
    assert_eq!(art, logo::version(M_ARTWORK.as_bytes()));
}

#[test]
fn the_theme_accent_is_read_only_when_valid() {
    let spy = Spy::memory();
    put_row(&*spy, &[("name", s("Acme"))]);
    put_theme(&*spy, "#C2410C");
    let (r, _) = reader(&spy, LONG);
    assert_eq!(r.current().theme_accent(), Some("#c2410c"));
    put_theme(&*spy, "#12");
    let (r, _) = reader(&spy, LONG);
    assert_eq!(r.current().theme_accent(), None);
    // A setting that is not JSON is no theme.
    spy.apply_checked(&[Op::Put {
        model: SETTING_MODEL.into(),
        id: SETTING_ID.into(),
        fields: fields(&[("value", s("{not json"))]),
    }])
    .unwrap();
    let (r, _) = reader(&spy, LONG);
    assert_eq!(r.current().theme_accent(), None);
    assert!(r.current().is_set());
}

#[test]
fn no_row_is_the_default_brand_with_no_logo() {
    let spy = Spy::memory();
    let (r, _) = reader(&spy, LONG);
    let shown = r.current();
    assert!(!shown.is_set());
    assert_eq!(shown.brand().name(), "obop");
    assert!(
        shown.logo().is_none(),
        "the logo route answers 404 with no brand"
    );
    assert_eq!(shown.v(), "");
    assert!(shown.fallbacks().is_empty());
}

#[test]
fn the_fallback_chain_of_titles_and_messages() {
    // A Swedish page with only message_en set shows the English message; with title_sv empty it
    // shows title_en, then the name.
    let (shown, _) = shown_of(&[
        ("name", s("Acme")),
        ("message_en", s("Welcome back")),
        ("title_en", s("Acme Mileage")),
    ]);
    assert_eq!(shown.brand().message("sv"), Some("Welcome back"));
    assert_eq!(shown.brand().title("sv"), "Acme Mileage");
    let (shown, _) = shown_of(&[("name", s("Acme")), ("title_sv", s("Acme Logga"))]);
    assert_eq!(shown.brand().title("sv"), "Acme Logga");
    assert_eq!(
        shown.brand().title("de"),
        "Acme",
        "no title at all: the name"
    );
    assert_eq!(shown.brand().message("de"), None);
    // A title that fell back does not hide the English one.
    let (shown, _) = shown_of(&[
        ("name", s("Acme")),
        ("title_sv", s("a<b")),
        ("title_en", s("Acme English")),
    ]);
    assert_eq!(shown.brand().title("sv"), "Acme English");
    assert_eq!(fields_of(&shown), ["title_sv"]);
}

/// The back office sign-in picks the message in TypeScript (`brandMessage` in `@obop/ui`, from the
/// `messages` of the meta tag); the OAuth page picks it here. The two are tested against the same
/// cases, in `web/packages/ui/src/brand.fixtures.json` (spec section 13, "Fallback chain": the
/// chain is a second implementation in TypeScript, tested against the cases of the Rust test).
#[test]
fn the_page_script_and_the_server_choose_the_same_message() {
    const FIXTURES: &str =
        include_str!("../../../../../../web/packages/ui/src/brand.fixtures.json");
    let all: serde_json::Value = serde_json::from_str(FIXTURES).unwrap();
    let cases = all["message"].as_array().unwrap();
    assert!(cases.len() >= 15, "the fixture file lost its cases");
    for (i, case) in cases.iter().enumerate() {
        let mut pairs: Vec<(String, J)> = vec![("name".into(), s("Acme"))];
        for (lang, text) in case["messages"].as_object().unwrap() {
            pairs.push((format!("message_{lang}"), s(text.as_str().unwrap())));
        }
        let borrowed: Vec<(&str, J)> = pairs.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
        let (shown, _) = shown_of(&borrowed);
        let lang = case["lang"].as_str().unwrap();
        let want = case["want"].as_str().unwrap();
        assert_eq!(
            shown.brand().message(lang).unwrap_or(""),
            want,
            "case {}: {} shown in {lang}",
            i + 1,
            case["messages"]
        );
        // And what the shell writes into the tag is what the script reads: the languages that
        // are set, nothing else.
        let tag_langs: Vec<&str> = shown.brand().messages().map(|(l, _)| l).collect();
        let mut set: Vec<&str> = case["messages"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        set.sort_by_key(|l| obop_brand::LANGS.iter().position(|x| x == l));
        assert_eq!(tag_langs, set, "case {}", i + 1);
    }
}

/// The write check refuses a name or a title that starts with a space, so a text of nothing but
/// spaces is a row written around the command (a restore, an import, a direct edit, an older
/// binary). The read check calls it empty: the name falls back to `obop`, a title to the name, a
/// message to none, so no page shows a blank brand line next to the logo, and a blank title does not
/// hide the name.
#[test]
fn a_text_of_nothing_but_spaces_is_empty_and_falls_back_like_an_empty_one() {
    let (shown, _) = shown_of(&[
        ("name", s("   ")),
        ("title_en", s("   ")),
        ("title_sv", s(" ")),
        ("message_en", s("  ")),
    ]);
    assert!(shown.is_set());
    assert_eq!(shown.brand().name(), "obop", "the name fell back");
    for lang in ["en", "sv", "de"] {
        assert_eq!(shown.brand().title(lang), "obop", "{lang}");
    }
    assert_eq!(shown.brand().message("en"), None);
    assert_eq!(
        fields_of(&shown),
        ["name", "title_en", "title_sv", "message_en"]
    );
    assert!(shown.fallbacks().iter().all(|f| f.code == "empty"));
    // With a name that is good, a blank title is the name; nothing else falls back.
    let (shown, _) = shown_of(&[("name", s("Acme")), ("title_en", s("   "))]);
    assert_eq!(shown.brand().name(), "Acme");
    assert_eq!(shown.brand().title("en"), "Acme");
    assert_eq!(fields_of(&shown), ["title_en"]);
    // Spaces between words are the text.
    let (shown, _) = shown_of(&[("name", s("Acme Co")), ("title_en", s("Acme Co Mileage"))]);
    assert_eq!(shown.brand().title("en"), "Acme Co Mileage");
    assert!(shown.fallbacks().is_empty());
}

#[test]
fn the_stderr_line_is_written_once_per_field_names_the_field_and_holds_nothing_of_the_value() {
    let spy = Spy::memory();
    // A name with a newline, an escape sequence and a forged line prefix, a bad slug and a hostile
    // artwork: three fields.
    let forged = "x\n\u{1b}[31mobop: FORGED 2026-01-01 error: root password reset";
    put_row(
        &*spy,
        &[
            ("name", s(forged)),
            ("slug", s("Foo/Bar\nFORGED")),
            (
                "logo_svg",
                s(
                    "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\"><script>FORGED</script></svg>",
                ),
            ),
        ],
    );
    let (r, kept) = reader(&spy, Duration::ZERO);
    for _ in 0..5 {
        r.current();
    }
    let got = kept.lock().unwrap().clone();
    assert_eq!(got.len(), 3, "one line per field and process: {got:?}");
    for (line, (field, code)) in got.iter().zip([
        ("name", "unsafe_character"),
        ("slug", "invalid_slug"),
        ("logo_svg", "svg_text"),
    ]) {
        assert!(line.contains(&format!("stored {field} ")), "{line}");
        assert!(line.contains(code), "{line}");
        assert!(!line.contains('\n') && !line.contains('\u{1b}'), "{line:?}");
        assert!(
            !line.contains("FORGED") && !line.contains("password"),
            "{line}"
        );
    }
    // A field that starts to fail later is one more line; the ones already written are not
    // repeated.
    put_row(
        &*spy,
        &[
            ("name", s(forged)),
            ("slug", s("Foo/Bar\nFORGED")),
            ("title_fi", s("a<b")),
        ],
    );
    r.current();
    let got = kept.lock().unwrap().clone();
    assert_eq!(got.len(), 4, "{got:?}");
    assert!(got[3].contains("stored title_fi "));
}

#[test]
fn fresh_lists_the_fallbacks_and_gives_the_stored_fields_as_stored() {
    let spy = Spy::memory();
    put_row(
        &*spy,
        &[
            ("name", s("Acme")),
            ("title_en", s("a<b")),
            ("slug", s("Foo/Bar")),
        ],
    );
    let (r, kept) = reader(&spy, LONG);
    let fresh = r.fresh().unwrap();
    assert!(fresh.set());
    assert_eq!(fields_of(fresh.shown()), ["slug", "title_en"]);
    assert_eq!(
        fresh.stored_text("title_en"),
        "a<b",
        "as stored, not as shown"
    );
    assert_eq!(fresh.stored_text("slug"), "Foo/Bar");
    assert_eq!(fresh.stored_text("message_en"), "");
    assert!(
        kept.lock().unwrap().is_empty(),
        "get writes no line; it lists"
    );
    let json = crate::brand::fallbacks_json(fresh.shown().fallbacks());
    let v: J = serde_json::from_str(&json).unwrap();
    assert_eq!(v[0]["field"], "slug");
    assert!(v[0]["reason"].as_str().unwrap().contains("invalid_slug"));
}

#[test]
fn each_hostile_artwork_falls_back_with_the_code_and_the_reason_of_the_reader() {
    // The code is the one of the first refusal of the SVG reader, and the reason is that refusal in
    // words, with the number of the others after it when there are more.
    let expected: &[(&str, &str)] = &[
        ("script", "svg_text"),
        ("a form after the root", "svg_malformed"),
        ("a form in a closed root", "svg_malformed"),
        ("a link", "svg_element"),
        ("an onload attribute", "svg_attribute"),
        ("17 KiB", "svg_too_large"),
        ("a number with an exponent", "svg_value"),
        ("a number over the bound", "svg_value"),
        ("a number of 25 characters", "svg_value"),
        ("a byte order mark", "svg_byte_order_mark"),
        ("an external image", "svg_element"),
        ("not svg at all", "svg_text"),
    ];
    let corpus = hostile_artworks();
    assert_eq!(corpus.len(), expected.len());
    for ((what, stored), (named, code)) in corpus.iter().zip(expected) {
        assert_eq!(what, named);
        let (shown, _) = shown_of(&[("name", s("Acme")), ("logo_svg", s(stored))]);
        let f = &shown.fallbacks()[0];
        assert_eq!(f.field, "logo_svg", "{what}");
        assert_eq!(f.code, *code, "{what}");
        assert!(
            f.reason.starts_with(&format!("{code}: ")),
            "{what}: {}",
            f.reason
        );
    }
    // One refusal gives its reason alone; more give the count of the others (here exactly one).
    let (what, stored) = &corpus[0];
    let (shown, _) = shown_of(&[("name", s("Acme")), ("logo_svg", s(stored))]);
    assert_eq!(
        shown.fallbacks()[0].reason,
        "svg_text: text outside the elements is not allowed (<text> and <title> are not either) (and 1 more)",
        "{what}"
    );
    let ns = "http://www.w3.org/2000/svg";
    let one = format!(
        "<svg xmlns=\"{ns}\" viewBox=\"0 0 64 64\"><a href=\"https://evil.example\"><path d=\"M 0 0\"/></a></svg>"
    );
    let (shown, _) = shown_of(&[("name", s("Acme")), ("logo_svg", s(&one))]);
    assert_eq!(
        shown.fallbacks()[0].reason,
        "svg_element: the element <a> is not allowed (only g, path, rect, circle, ellipse, line and polygon are); delete it"
    );
    let two = format!(
        "<svg xmlns=\"{ns}\" viewBox=\"0 0 64 64\"><script>x</script><a href=\"x\"/></svg>"
    );
    let (shown, _) = shown_of(&[("name", s("Acme")), ("logo_svg", s(&two))]);
    assert_eq!(
        shown.fallbacks()[0].reason,
        "svg_text: text outside the elements is not allowed (<text> and <title> are not either) (and 2 more)"
    );
}

#[test]
fn the_stderr_lines_are_fixed_texts_that_name_the_field_and_the_code() {
    let ns = "http://www.w3.org/2000/svg";
    let spy = Spy::memory();
    // An element and an attribute with a name that imitates a log line: the reason in `get` may
    // repeat what the reader made safe, the line on stderr repeats nothing of the file.
    let forged = format!(
        "<svg xmlns=\"{ns}\" viewBox=\"0 0 64 64\" obop-FORGED-line=\"1\"><obop-FORGED-element/></svg>"
    );
    put_row(
        &*spy,
        &[
            ("name", s("a<b")),
            ("slug", s("Foo/Bar")),
            ("title_fi", s("x".repeat(61).as_str())),
            ("logo_svg", s(&forged)),
        ],
    );
    let (r, kept) = reader(&spy, LONG);
    r.current();
    let line = |field: &str, code: &str| {
        format!(
            "obop: the stored {field} of the instance branding is not used ({code}); the default is shown instead (obop admin brand status lists it)"
        )
    };
    assert_eq!(
        *kept.lock().unwrap(),
        [
            line("name", "unsafe_character"),
            line("slug", "invalid_slug"),
            line("title_fi", "too_long"),
            line("logo_svg", "svg_attribute"),
        ]
    );
}

#[test]
fn the_fallbacks_come_in_the_order_of_the_fields() {
    // name, slug, the titles in the order of the languages, the messages in the same order, the
    // artwork.
    let (shown, _) = shown_of(&[
        ("logo_svg", s("nope")),
        ("message_en", s("a<b")),
        ("title_sv", s("a<b")),
        ("message_es", s("a<b")),
        ("title_en", s("a<b")),
        ("slug", s("Foo/Bar")),
        ("name", s("a<b")),
    ]);
    assert_eq!(
        fields_of(&shown),
        [
            "name",
            "slug",
            "title_en",
            "title_sv",
            "message_en",
            "message_es",
            "logo_svg"
        ]
    );
}

#[test]
fn a_theme_setting_of_more_than_256_kib_is_not_parsed_and_gives_no_accent() {
    // The settings are refused above 64 KiB when written; a bigger text was put there around the
    // command, and the reader does not parse it on every refill.
    let accent_with = |pad: usize| {
        let spy = Spy::memory();
        put_row(&*spy, &[("name", s("Acme"))]);
        let text = format!(
            "{{\"theme\":{{\"accent\":\"#c2410c\"}},\"pad\":\"{}\"}}",
            "x".repeat(pad)
        );
        spy.apply_checked(&[Op::Put {
            model: SETTING_MODEL.into(),
            id: SETTING_ID.into(),
            fields: fields(&[("value", s(&text))]),
        }])
        .unwrap();
        let (r, _) = reader(&spy, LONG);
        r.current().theme_accent().map(str::to_string)
    };
    assert_eq!(accent_with(100 * 1024), Some("#c2410c".to_string()));
    assert_eq!(accent_with(300 * 1024), None);
}
