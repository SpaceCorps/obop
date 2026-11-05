//! The generated tile and the logo that is served (docs/spec/branding.md, section 2 "The generated
//! tile", section 4 "The artwork read check", section 13 "The tile" and the escaping bullet).

use crate::logo::{LOGO_HEADERS, Logo, logo_etag, logo_url, version};
use crate::testutil::{Rng, text};
use crate::theme;
use crate::tile::{
    DEFAULT_VIOLET, FALLBACK_GLYPH, SLATE, TileAccent, glyph, svg, tile_accent, valid_accent,
};

// ---- The glyph ------------------------------------------------------------------------------------------

#[test]
fn the_glyph_is_the_first_letter_or_digit_in_upper_case() {
    assert_eq!(glyph("MOHV"), 'M');
    assert_eq!(glyph("mohv"), 'M');
    assert_eq!(glyph("Exämple"), 'E');
    assert_eq!(glyph("éclair"), 'É');
    assert_eq!(glyph("ølen"), 'Ø');
    assert_eq!(glyph("Łódź"), 'Ł');
    assert_eq!(glyph("株式会社"), '株');
    assert_eq!(glyph("пример"), 'П');
    assert_eq!(glyph("ǆ"), 'Ǆ', "a one-to-one uppercase mapping is used");
}

#[test]
fn a_digit_is_its_own_glyph() {
    assert_eq!(glyph("24 Stunden"), '2');
    assert_eq!(glyph("7"), '7');
    assert_eq!(glyph("٣ stars"), '٣');
}

#[test]
fn a_letter_whose_uppercase_is_longer_stays_as_it_is() {
    // ß would become SS, ŉ would become ʼN, ǰ would become J and a combining caron.
    assert_eq!(glyph("ß"), 'ß');
    assert_eq!(glyph("Straße"), 'S');
    assert_eq!(glyph("ßtrasse"), 'ß');
    assert_eq!(glyph("ŉ"), 'ŉ');
    assert_eq!(glyph("ǰ"), 'ǰ');
    // The Greek small letter with a ypogegrammeni expands too; it stays.
    assert_eq!(glyph("\u{1FB3}"), '\u{1FB3}');
}

#[test]
fn the_first_letter_or_digit_skips_what_comes_before_it() {
    // A row written around the command may start with anything.
    assert_eq!(glyph("(zed)"), 'Z');
    assert_eq!(glyph("  - Zed"), 'Z');
    assert_eq!(glyph("<b>zed</b>"), 'B');
    assert_eq!(glyph("\u{200B}zed"), 'Z');
    // A combining mark or a symbol is no letter.
    assert_eq!(glyph("\u{0301}zed"), 'Z');
    assert_eq!(glyph("€zed"), 'Z');
}

#[test]
fn a_name_with_no_letter_or_digit_draws_o() {
    for name in [
        "", " ", "---", "(!)", "€€", "\u{200B}", "😀", "\u{0301}", "<>",
    ] {
        assert_eq!(glyph(name), FALLBACK_GLYPH, "{name:?}");
        assert_eq!(FALLBACK_GLYPH, 'O');
    }
}

// ---- The tile accent ---------------------------------------------------------------------------------

#[test]
fn the_tile_accent_is_the_stored_theme_accent_when_it_is_a_valid_hex_colour() {
    let a = tile_accent(Some("#e11d48"));
    assert_eq!(
        a,
        TileAccent {
            hex: "#e11d48".into(),
            from_theme: true
        }
    );
    // Upper case is accepted and written in lower case.
    assert_eq!(tile_accent(Some("#E11D48")).hex, "#e11d48");
    assert_eq!(tile_accent(Some("#0F766E")).hex, "#0f766e");
}

#[test]
fn the_tile_accent_is_slate_when_the_theme_stores_none_or_the_default_violet() {
    let slate = TileAccent {
        hex: SLATE.into(),
        from_theme: false,
    };
    assert_eq!(tile_accent(None), slate);
    // "Set as the default for everyone" stores the whole resolved theme, the default accent
    // included: the tile stays slate.
    assert_eq!(tile_accent(Some("#6d3fd0")), slate);
    assert_eq!(tile_accent(Some("#6D3FD0")), slate);
    assert_eq!(tile_accent(Some("#6d3Fd0")), slate);
    assert_eq!(DEFAULT_VIOLET, "#6d3fd0");
    assert_eq!(SLATE, "#495670");
    // A violet chosen on purpose that is not exactly the default is a colour like any other.
    assert!(tile_accent(Some("#6d3fd1")).from_theme);
}

#[test]
fn an_invalid_stored_accent_is_no_accent() {
    let slate = TileAccent {
        hex: SLATE.into(),
        from_theme: false,
    };
    for bad in [
        "",
        "#",
        "#abc",
        "#abcd",
        "#abcde",
        "#abcdefg",
        "#gggggg",
        "e11d48",
        "red",
        " #e11d48",
        "#e11d48 ",
        "#e11d48ff",
        "rgb(1,2,3)",
        "javascript:alert(1)",
        "#e11d4\u{0661}",
        "#é11d48",
    ] {
        assert_eq!(tile_accent(Some(bad)), slate, "{bad:?}");
        assert_eq!(valid_accent(bad), None, "{bad:?}");
    }
    assert_eq!(valid_accent("#AbCdEf"), Some("#abcdef".to_string()));
}

// ---- The tile ----------------------------------------------------------------------------------------

#[test]
fn the_fill_is_the_resolved_light_fill_and_the_glyph_its_on_fill() {
    let accent = "#e11d48";
    let resolved = theme::resolve(accent);
    let tile = svg("MOHV", Some(accent), None);
    assert!(
        tile.contains(&format!("fill=\"{}\"/>", resolved.light.fill)),
        "{tile}"
    );
    assert!(
        tile.contains(&format!(
            "text-anchor=\"middle\" fill=\"{}\">M</text>",
            resolved.light.on_fill
        )),
        "{tile}"
    );
    // The search can move the colour: the fill need not be the stored hex.
    let yellow = svg("MOHV", Some("#ffff00"), None);
    assert!(!yellow.contains("fill=\"#ffff00\""), "{yellow}");
}

/// For the slate the value the colour search returns is pinned here: the spec expected it to be the
/// slate itself and did not compute it.
#[test]
fn the_slate_tile_is_pinned() {
    let resolved = theme::resolve(SLATE);
    assert_eq!(
        resolved.light.fill, "#495670",
        "the search leaves the slate where it is"
    );
    assert_eq!(resolved.light.on_fill, "#ffffff");
    assert_eq!(
        svg("MOHV", None, None),
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\">\
         <rect width=\"64\" height=\"64\" rx=\"14\" fill=\"#495670\"/>\
         <text x=\"32\" y=\"44\" font-family=\"Georgia,serif\" font-size=\"36\" \
         font-weight=\"700\" text-anchor=\"middle\" fill=\"#ffffff\">M</text></svg>"
    );
    // With no accent, with the default violet in either case and with an invalid one: the same.
    for stored in [
        None,
        Some("#6d3fd0"),
        Some("#6D3FD0"),
        Some("nonsense"),
        Some("#abc"),
    ] {
        assert_eq!(
            svg("MOHV", stored, None),
            svg("MOHV", None, None),
            "{stored:?}"
        );
    }
}

#[test]
fn the_tile_is_a_template_with_one_escaped_glyph() {
    let tile = svg("Zed", None, None);
    assert_eq!(tile.matches("<text").count(), 1);
    assert_eq!(tile.matches("</text>").count(), 1);
    assert!(tile.starts_with("<svg ") && tile.ends_with("</svg>"));
    // A hostile name cannot put markup into the tile: the glyph is one letter or digit, and
    // whatever else the name holds is not drawn.
    for name in [
        "<script>alert(1)</script>",
        "\"><svg onload=x>",
        "&amp;",
        "'; drop",
        "a\"b",
    ] {
        let t = svg(name, None, None);
        assert_eq!(
            t.matches('<').count(),
            svg("Zed", None, None).matches('<').count(),
            "{name:?}"
        );
        assert!(!t.contains("script") && !t.contains("onload"), "{t}");
        assert_eq!(t.matches("<text").count(), 1);
    }
    // A symbol is no glyph: the tile of "&" draws the O.
    assert!(svg("&", None, None).contains(">O</text>"));
}

#[test]
fn the_name_without_a_letter_draws_o_in_the_tile() {
    let tile = svg("---", None, None);
    assert!(tile.contains(">O</text>"), "{tile}");
    let tile = svg("ß", None, None);
    assert!(tile.contains(">ß</text>"), "{tile}");
    let tile = svg("7 Sins", None, None);
    assert!(tile.contains(">7</text>"), "{tile}");
}

#[test]
fn the_inline_tile_carries_the_size_and_the_served_one_does_not() {
    let served = svg("MOHV", None, None);
    let inline = svg("MOHV", None, Some(28));
    assert!(!served.contains("width=\"28\""));
    assert!(inline.contains(" width=\"28\" height=\"28\" viewBox=\"0 0 64 64\""));
    assert_eq!(
        inline.replacen(" width=\"28\" height=\"28\"", "", 1),
        served
    );
}

// ---- The version v ----------------------------------------------------------------------------------

#[test]
fn v_is_the_first_16_hex_digits_of_the_sha256_of_the_bytes_served() {
    // SHA-256("abc") = ba7816bf 8f01cfea 414140de 5dae2223 b00361a3 96177a9c b410ff61 f20015ad
    assert_eq!(version(b"abc"), "ba7816bf8f01cfea");
    assert_eq!(version(b"abc").len(), 16);
    assert_eq!(
        version(b""),
        "e3b0c44298fc1c14",
        "SHA-256 of nothing is e3b0c442 98fc1c14 9afbf4c8 996fb924 ..."
    );
}

#[test]
fn v_and_the_etag_change_when_only_the_theme_accent_changes() {
    let a = Logo::resolve("", "MOHV", None);
    let b = Logo::resolve("", "MOHV", Some("#e11d48"));
    let c = Logo::resolve("", "MOHV", Some("#6d3fd0"));
    assert!(a.is_tile() && b.is_tile() && c.is_tile());
    assert_ne!(a.v(), b.v(), "a stored accent alters the tile");
    assert_eq!(
        a.v(),
        c.v(),
        "the default violet is no accent: the same tile"
    );
    assert_eq!(a.v(), version(a.served().as_bytes()));
    assert_ne!(logo_etag(a.v()), logo_etag(b.v()));
    // And with an artwork the theme changes nothing.
    let art = "<svg viewBox=\"0 0 1 1\"><rect width=\"1\" height=\"1\"/></svg>";
    assert_eq!(
        Logo::resolve(art, "MOHV", None).v(),
        Logo::resolve(art, "MOHV", Some("#e11d48")).v()
    );
}

#[test]
fn the_logo_route_constants() {
    assert_eq!(crate::logo::LOGO_PATH, "/api/brand/logo.svg");
    assert_eq!(
        logo_url("0123456789abcdef"),
        "/api/brand/logo.svg?v=0123456789abcdef"
    );
    assert_eq!(logo_etag("0123456789abcdef"), "\"0123456789abcdef\"");
    assert_eq!(
        LOGO_HEADERS,
        [
            ("Content-Type", "image/svg+xml"),
            ("X-Content-Type-Options", "nosniff"),
            (
                "Content-Security-Policy",
                "default-src 'none'; style-src 'none'; sandbox"
            ),
            ("Cache-Control", "no-cache"),
        ]
    );
}

// ---- The artwork read check -------------------------------------------------------------------------

const ART: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\"><rect width=\"64\" height=\"64\" rx=\"14\" fill=\"#123456\"/></svg>";

#[test]
fn no_artwork_is_the_tile_with_no_fallback() {
    let l = Logo::resolve("", "MOHV", None);
    assert!(l.is_tile() && l.fallback().is_none());
    assert_eq!(l.served(), svg("MOHV", None, None));
}

#[test]
fn a_stored_artwork_is_served_re_serialised_and_has_no_fallback() {
    let l = Logo::resolve(ART, "MOHV", None);
    assert!(!l.is_tile() && l.fallback().is_none());
    assert_eq!(l.served(), ART);
    assert_eq!(l.v(), version(ART.as_bytes()));
    // The inline form differs by the two size attributes only.
    assert_eq!(
        l.inline(28),
        ART.replacen(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"",
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"28\" height=\"28\"",
            1
        )
    );
}

#[test]
fn a_valid_file_that_is_not_in_canonical_form_is_no_fallback() {
    // Extra spaces, .5, trailing zeros and six decimals: valid, and served as the printer writes it.
    let stored = "<?xml version=\"1.0\"?>\n<svg   xmlns = \"http://www.w3.org/2000/svg\"\n viewBox=\"0  0 64.000000 64\"  >\n  <rect  width=\"64.0\"  height=\".5\" x=\"0.123456\" fill=\"#ABCDEF\" />\n</svg>\n";
    let l = Logo::resolve(stored, "MOHV", None);
    assert!(!l.is_tile(), "{:?}", l.fallback());
    assert!(l.fallback().is_none());
    assert_eq!(
        l.served(),
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\"><rect x=\"0.1235\" width=\"64\" height=\"0.5\" fill=\"#abcdef\"/></svg>"
    );
    assert!(!l.served().contains(".000000") && !l.served().contains("ABCDEF"));
    assert_eq!(l.v(), version(l.served().as_bytes()));
}

#[test]
fn a_stored_artwork_that_fails_the_parse_is_the_tile_and_a_fallback() {
    let big = format!("<svg viewBox=\"0 0 1 1\">{}</svg>", " ".repeat(17 * 1024));
    let cases: Vec<(&str, String)> = vec![
        (
            "a script element",
            "<svg viewBox=\"0 0 1 1\"><script>alert(1)</script></svg>".into(),
        ),
        (
            "a form after the root",
            "<svg viewBox=\"0 0 1 1\"></svg><form action=\"/x\"></form>".into(),
        ),
        (
            "an early close and a form",
            "<svg viewBox=\"0 0 1 1\"></svg><form action=\"x\"><svg>".into(),
        ),
        (
            "a link",
            "<svg viewBox=\"0 0 1 1\"><a href=\"http://x.test\"/></svg>".into(),
        ),
        (
            "an onload attribute",
            "<svg viewBox=\"0 0 1 1\" onload=\"alert(1)\"/>".into(),
        ),
        ("17 KiB", big),
        (
            "numbers that break the rule",
            "<svg viewBox=\"0 0 1 1\"><rect x=\"1e5\"/></svg>".into(),
        ),
        ("a byte order mark", format!("\u{FEFF}{ART}")),
        ("whitespace only", "   ".into()),
        ("not an svg", "hello".into()),
    ];
    for (label, stored) in cases {
        let l = Logo::resolve(&stored, "MOHV", None);
        assert!(l.is_tile(), "{label}");
        assert_eq!(
            l.served(),
            svg("MOHV", None, None),
            "{label}: the tile is served"
        );
        assert_eq!(
            l.v(),
            version(svg("MOHV", None, None).as_bytes()),
            "{label}"
        );
        let reasons = l
            .fallback()
            .unwrap_or_else(|| panic!("{label}: no fallback listed"));
        assert!(!reasons.is_empty(), "{label}");
        // The stored text appears nowhere: not in what is served, not inline, not in v.
        for needle in ["script", "form", "onload", "href", "alert"] {
            assert!(!l.served().contains(needle), "{label}: {needle}");
            assert!(!l.inline(28).contains(needle), "{label}: {needle}");
        }
    }
}

/// The artwork files that earlier releases wrote: every one still renders, re-serialised by the
/// current printer. The first entry is the output of the first release's printer; an entry is added
/// whenever the writer changes, and none is removed or edited.
const GOLDEN_ARTWORK: &[&str] = &[
    // The first release (S1a): the "M" placeholder.
    "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\"><rect width=\"64\" height=\"64\" rx=\"14\" fill=\"#495670\"/><path d=\"M 16 48 L 16 16 L 24 16 L 32 30 L 40 16 L 48 16 L 48 48 L 41 48 L 41 29 L 32 44 L 23 29 L 23 48 Z\" fill=\"#ffffff\"/></svg>",
    // Groups, a transform, strokes and every shape.
    "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\"><g fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.5\" transform=\"translate(2 3) scale(2)\"><circle cx=\"6\" cy=\"6\" r=\"3\"/><ellipse cx=\"12\" cy=\"12\" rx=\"4\" ry=\"2\"/><line x1=\"0\" y1=\"0\" x2=\"5\" y2=\"5\"/><polygon points=\"0 0 5 0 2.5 4\"/><path d=\"M 0 0 a 1 1 0 0 1 1 1 z\" fill-rule=\"evenodd\"/></g></svg>",
];

#[test]
fn the_artwork_files_of_earlier_releases_still_render_as_themselves() {
    for stored in GOLDEN_ARTWORK {
        let l = Logo::resolve(stored, "MOHV", None);
        assert!(!l.is_tile(), "{stored}: {:?}", l.fallback());
        assert!(l.fallback().is_none());
        assert_eq!(
            l.served(),
            *stored,
            "the current printer writes the golden file as it is"
        );
    }
}

#[test]
fn resolving_a_logo_never_panics() {
    let mut rng = Rng::new(0xB4A9_D004);
    let outcome = std::panic::catch_unwind(move || {
        for _ in 0..5_000 {
            let stored = text(&mut rng, 80);
            let name = text(&mut rng, 20);
            let accent = text(&mut rng, 8);
            let l = Logo::resolve(&stored, &name, Some(&accent));
            let _ = (l.served().len(), l.v().len(), l.inline(28).len());
            assert_eq!(l.v(), version(l.served().as_bytes()));
        }
    });
    assert!(outcome.is_ok());
}
