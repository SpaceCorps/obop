//! What the public site takes from the brand (docs/spec/branding.md, section 5 and the row OT-04 of
//! section 6): `fill_site` puts the fallbacks of the record into the `site` object that
//! `PublicApi::site_settings` returns, for the keys that object leaves unset, and hands the artwork
//! over through the one reserved key. Section 13: "each fallback and each explicit `site.*` key
//! winning; the accent order for the shop and for its favicon tile (`site.accent`, then the tile
//! accent of section 2, then teal)", and "a `site.*` key with the name of the reserved key ...
//! does not reach ... any page, because `site_settings` strips it".

use super::super::site::{Source, view};
use super::super::{DEFAULT_TITLE, FAVICON_KEY, Shown, fill_site};
use super::*;
use obop_brand::policy::NAME_MAX;
use serde_json::{Map, json};

fn site(v: J) -> Map<String, J> {
    v.as_object().expect("an object").clone()
}

/// What a page would be shown for a store with this row (none for `None`) and this theme accent.
fn shown(row: Option<&[(&str, J)]>, theme: Option<&str>) -> Arc<Shown> {
    let spy = Spy::memory();
    if let Some(row) = row {
        put_row(&*spy, row);
    }
    if let Some(accent) = theme {
        put_theme(&*spy, accent);
    }
    let (r, _) = reader(&spy, LONG);
    r.current()
}

fn filled(site_keys: J, brand: &Shown) -> Map<String, J> {
    let mut m = site(site_keys);
    fill_site(&mut m, brand);
    m
}

const HOSTILE: &str = "<svg onload=\"x\"></svg>";

#[test]
fn with_no_brand_nothing_is_filled() {
    let none = shown(None, None);
    assert!(!none.is_set());
    assert!(filled(json!({}), &none).is_empty());
    // The keys that are there are left exactly as they are, the unusable ones included: the site
    // decides what it does with them, as it always did.
    let keys =
        json!({"title": "Café Nord", "accent": "red", "logo_url": "", "tagline": "x", "n": 3});
    assert_eq!(filled(keys.clone(), &none), site(keys));
    // A theme alone is no brand.
    let themed = shown(None, Some("#c2410c"));
    assert!(filled(json!({}), &themed).is_empty());
}

#[test]
fn with_no_brand_a_stored_key_with_the_reserved_name_is_removed() {
    let none = shown(None, None);
    for stored in [
        json!({"svg": HOSTILE, "v": "0123456789abcdef"}),
        json!("x"),
        json!(null),
        json!(5),
        json!({}),
    ] {
        let m = filled(json!({FAVICON_KEY: stored, "title": "Café Nord"}), &none);
        assert!(!m.contains_key(FAVICON_KEY), "{m:?}");
        assert_eq!(m["title"], "Café Nord", "the other keys stay");
        assert_eq!(m.len(), 1);
    }
}

#[test]
fn a_stored_key_with_the_reserved_name_never_survives_a_brand_either() {
    // With a brand that has no artwork the key is gone (the shop keeps its own tile) ...
    let tile = shown(Some(&[("name", s("MOHV"))]), None);
    let m = filled(
        json!({FAVICON_KEY: {"svg": HOSTILE, "v": "0123456789abcdef"}}),
        &tile,
    );
    assert!(!m.contains_key(FAVICON_KEY), "{m:?}");
    // ... and with an artwork it is the reader's, not the stored object.
    let art = shown(
        Some(&[("name", s("MOHV")), ("logo_svg", s(M_ARTWORK))]),
        None,
    );
    let m = filled(
        json!({FAVICON_KEY: {"svg": HOSTILE, "v": "0123456789abcdef"}}),
        &art,
    );
    let hand_over = &m[FAVICON_KEY];
    assert_eq!(hand_over["svg"], art.logo().unwrap().served());
    assert_eq!(hand_over["v"], art.v());
    assert!(!format!("{m:?}").contains("onload"), "{m:?}");
}

#[test]
fn with_a_brand_the_unset_keys_are_the_records() {
    let b = shown(Some(&[("name", s("MOHV"))]), None);
    let m = filled(json!({}), &b);
    assert_eq!(m["title"], "MOHV");
    assert_eq!(m["logo_url"], format!("/api/brand/logo.svg?v={}", b.v()));
    assert_eq!(m["accent"], "#495670", "the tile accent: the slate");
    assert!(
        !m.contains_key(FAVICON_KEY),
        "the generated tile is no artwork"
    );
    // `logo_text` is the site's to derive from the title, so the glyph of its own tile follows the
    // name: it is not filled here.
    assert_eq!(m.len(), 3, "{m:?}");
}

#[test]
fn an_artwork_goes_through_the_reserved_key_with_the_version_the_reader_computed() {
    let b = shown(
        Some(&[("name", s("MOHV")), ("logo_svg", s(M_ARTWORK))]),
        None,
    );
    let m = filled(json!({}), &b);
    let hand_over = m[FAVICON_KEY].as_object().expect("an object");
    assert_eq!(hand_over.len(), 2);
    assert_eq!(hand_over["svg"], b.logo().unwrap().served());
    assert_eq!(hand_over["v"], b.v());
    assert_eq!(m["logo_url"], format!("/api/brand/logo.svg?v={}", b.v()));
}

#[test]
fn a_stored_artwork_that_fails_the_read_check_is_the_tile_and_reaches_the_shop_as_none() {
    let b = shown(
        Some(&[
            ("name", s("MOHV")),
            ("logo_svg", s("<svg><script>x</script></svg>")),
        ]),
        None,
    );
    let m = filled(json!({}), &b);
    assert!(!m.contains_key(FAVICON_KEY));
    assert_eq!(m["logo_url"], format!("/api/brand/logo.svg?v={}", b.v()));
}

#[test]
fn an_explicit_key_always_wins_and_is_left_as_written() {
    let art = shown(
        Some(&[("name", s("MOHV")), ("logo_svg", s(M_ARTWORK))]),
        Some("#c2410c"),
    );
    let all = json!({"title": " Café Nord ", "logo_url": "/brand/logo.svg", "accent": "#E11D48"});
    let m = filled(all.clone(), &art);
    // The very text the owner stored: the site normalises it as it always did.
    assert_eq!(m["title"], all["title"]);
    assert_eq!(m["logo_url"], all["logo_url"]);
    assert_eq!(m["accent"], all["accent"]);
    // The artwork is not a `site.*` key: it is handed over all the same.
    assert!(m.contains_key(FAVICON_KEY));
    // One explicit key does not keep the others from falling back.
    let m = filled(json!({"title": "Café Nord"}), &art);
    assert_eq!(m["title"], "Café Nord");
    assert_eq!(m["logo_url"], format!("/api/brand/logo.svg?v={}", art.v()));
    assert_eq!(m["accent"], "#c2410c", "the theme's accent, then");
    let m = filled(json!({"accent": "#E11D48"}), &art);
    assert_eq!(m["title"], "MOHV");
    assert_eq!(m["accent"], "#E11D48");
    let m = filled(json!({"logo_url": "https://cdn.example/l.png"}), &art);
    assert_eq!(m["logo_url"], "https://cdn.example/l.png");
    assert_eq!(m["title"], "MOHV");
}

#[test]
fn a_key_the_site_would_not_use_is_unset() {
    let b = shown(Some(&[("name", s("MOHV"))]), None);
    let v = b.v().to_string();
    let logo = format!("/api/brand/logo.svg?v={v}");
    for bad in [
        json!(""),
        json!("  \n "),
        json!("\u{7}"),
        json!(7),
        json!(null),
        json!({}),
    ] {
        let m = filled(json!({"title": bad}), &b);
        assert_eq!(m["title"], "MOHV");
    }
    for bad in [
        json!("red"),
        json!("#abcd"),
        json!("#12345g"),
        json!("javascript:alert(1)"),
        json!(5),
        json!(""),
    ] {
        let m = filled(json!({"accent": bad}), &b);
        assert_eq!(m["accent"], "#495670");
    }
    for bad in [
        json!("javascript:x"),
        json!("//evil.test/l.png"),
        json!("http://x.test/l.png"),
        json!("https://user@x.test/l.png"),
        json!("/a b"),
        json!("/a\"b"),
        json!("x".repeat(600)),
        json!(1),
        json!(""),
    ] {
        let m = filled(json!({"logo_url": bad}), &b);
        assert_eq!(m["logo_url"], logo);
    }
    // And what the site does use, in the shapes it takes, is explicit.
    for good in ["#abc", "e11d48", " #E11D48 "] {
        assert_eq!(filled(json!({"accent": good}), &b)["accent"], good);
    }
    for good in ["/brand/l.svg", "https://cdn.example/l.png"] {
        assert_eq!(filled(json!({"logo_url": good}), &b)["logo_url"], good);
    }
}

/// The accent of the shop and of its tile: `site.accent`, then with a brand the tile accent of
/// section 2 (the theme's accent unless it is missing, invalid or the default violet, else the
/// slate), then teal as today.
#[test]
fn the_accent_order_is_the_site_the_tile_accent_and_teal() {
    let row: &[(&str, J)] = &[("name", s("MOHV"))];
    let accent_of = |site_keys: J, brand: &Shown| view(&site(site_keys), brand).accent;
    let own = json!({"accent": "#e11d48"});
    let none = json!({});
    for theme in [None, Some("#c2410c"), Some("#6d3fd0")] {
        let b = shown(Some(row), theme);
        let a = accent_of(own.clone(), &b);
        assert_eq!(
            (a.value.as_str(), a.source),
            ("#e11d48", Source::Site),
            "{theme:?}"
        );
    }
    let themed = accent_of(none.clone(), &shown(Some(row), Some("#c2410c")));
    assert_eq!(
        (themed.value.as_str(), themed.source),
        ("#c2410c", Source::Theme)
    );
    for theme in [
        None,
        Some("#6d3fd0"),
        Some("#6D3FD0"),
        Some("red"),
        Some("#abc"),
    ] {
        let slate = accent_of(none.clone(), &shown(Some(row), theme));
        assert_eq!(
            (slate.value.as_str(), slate.source),
            ("#495670", Source::Default),
            "{theme:?}"
        );
    }
    // No brand: the theme is not the shop's concern, as it never was. Teal.
    let teal = accent_of(none.clone(), &shown(None, Some("#c2410c")));
    assert_eq!(
        (teal.value.as_str(), teal.source),
        ("#0f766e", Source::Default)
    );
    // An explicit key still wins with no brand.
    let own_no_brand = accent_of(own, &shown(None, None));
    assert_eq!(own_no_brand.source, Source::Site);
}

#[test]
fn the_longest_name_the_policy_allows_arrives_whole_as_the_title() {
    let name: String = "Ä".repeat(NAME_MAX);
    assert_eq!(name.chars().count(), 40);
    let b = shown(Some(&[("name", s(&name))]), None);
    assert_eq!(b.brand().name(), name);
    let m = filled(json!({}), &b);
    assert_eq!(
        m["title"], name,
        "the site takes 80 characters of a title, the name 40"
    );
}

#[test]
fn a_name_that_fell_back_is_the_default_name_and_still_a_brand() {
    // A row written around the command: the name fails the read check, so the brand is set and
    // its name is `obop` (spec section 2: a row that names obop is not special).
    let b = shown(
        Some(&[("name", s("<b>x</b>")), ("logo_svg", s(M_ARTWORK))]),
        None,
    );
    assert!(b.is_set());
    let m = filled(json!({}), &b);
    assert_eq!(m["title"], "obop");
    assert!(m.contains_key(FAVICON_KEY));
    // The title is the default name, and the source says so; the artwork that passed is the record's.
    let v = view(&site(json!({})), &b);
    assert_eq!(
        (v.title.value.as_str(), v.title.source),
        ("obop", Source::Default)
    );
    assert_eq!(v.logo_url.source, Source::Record);
}

/// The spelling of the reserved key is listed in CONVENTIONS 9.1 and in spec 15.2, and a stored
/// `site` object can hold a key of that name (it is removed first): a rename is a decision, and it
/// fails here first. The shop imports the key as `SITE_FAVICON_KEY`.
#[test]
fn the_reserved_key_is_spelled_as_the_documents_list_it() {
    assert_eq!(FAVICON_KEY, "_brand_favicon");
    assert_eq!(crate::SITE_FAVICON_KEY, "_brand_favicon");
    assert_eq!(crate::DEFAULT_SITE_TITLE, "My Shop");
}

#[test]
fn the_default_title_is_the_one_the_site_has_always_had() {
    assert_eq!(DEFAULT_TITLE, "My Shop");
    let t = view(&site(json!({})), &shown(None, None)).title;
    assert_eq!((t.value.as_str(), t.source), ("My Shop", Source::Default));
}

/// The shop shows its title as the settings of the site make it (control characters dropped, the
/// text trimmed, at most 80 characters), and the title that comes from the record is made the same
/// way, so what `get` reports is what the page shows. A name with a space at its start or end passes
/// the read check; a name that would leave nothing is no title (the reader never gives one, since
/// the read check calls a name of spaces empty, so this is the answer for a `Shown` that did).
#[test]
fn the_title_of_the_record_is_the_name_as_the_shop_shows_it() {
    use super::super::site::brand_title;
    let cases: [(&str, bool, &str, Source); 9] = [
        ("MOHV", true, "MOHV", Source::Record),
        ("  MOHV  ", true, "MOHV", Source::Record),
        (" MOHV", true, "MOHV", Source::Record),
        ("MOHV ", true, "MOHV", Source::Record),
        ("Zed  Goods", true, "Zed  Goods", Source::Record),
        ("obop", false, "obop", Source::Default),
        (" obop ", false, "obop", Source::Default),
        ("    ", true, "My Shop", Source::Default),
        ("", true, "My Shop", Source::Default),
    ];
    for (name, stored, want, source) in cases {
        let t = brand_title(name, stored);
        assert_eq!((t.value.as_str(), t.source), (want, source), "{name:?}");
    }
}

/// Through the reader, for a row written around the command: a padded name is the shop's title
/// without the padding, from the record, and a name of spaces is no name at all, so the brand has
/// the default name and the title is `obop`, derived from no stored field.
#[test]
fn a_padded_name_is_the_title_without_the_padding_and_a_blank_name_falls_back() {
    for padded in ["  MOHV  ", "MOHV ", " MOHV"] {
        let b = shown(Some(&[("name", s(padded))]), None);
        assert!(b.fallbacks().is_empty(), "{padded:?}: {:?}", b.fallbacks());
        assert_eq!(b.brand().name(), padded, "the name is as stored");
        let t = view(&site(json!({})), &b).title;
        assert_eq!((t.value.as_str(), t.source), ("MOHV", Source::Record));
        assert_eq!(filled(json!({}), &b)["title"], "MOHV", "{padded:?}");
    }
    for blank in [" ", "    ", &" ".repeat(NAME_MAX)] {
        let b = shown(Some(&[("name", s(blank))]), None);
        assert!(b.is_set(), "the row exists: still a brand");
        let fell_back: Vec<_> = b
            .fallbacks()
            .iter()
            .map(|f| (f.field.as_str(), f.code))
            .collect();
        assert_eq!(fell_back, [("name", "empty")], "{blank:?}");
        assert_eq!(b.brand().name(), "obop");
        let t = view(&site(json!({})), &b).title;
        assert_eq!((t.value.as_str(), t.source), ("obop", Source::Default));
        assert_eq!(filled(json!({}), &b)["title"], "obop", "{blank:?}");
    }
}
