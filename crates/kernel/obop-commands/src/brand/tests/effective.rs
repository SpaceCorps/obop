//! The `effective` answer of `get` (docs/spec/branding.md, section 3, "What `get` shows", and section
//! 13, "Commands"): each value in force with the source that won, one case per value and source. The
//! object is staged by slice, so these tests also pin what it does not hold yet: the entries of the
//! surfaces that are not built (`authenticator_issuer` and `accent`) are not there, because an entry
//! whose precedence involves a surface that is not wired would be untrue. The entries of the public
//! site (`site.title`, `site.logo_url`, `site.accent` and `favicon`) arrived with the slice that
//! wires the shop to the record.

use super::*;
use obop_brand::LANGS;
use serde_json::json;

/// The `effective` object of a store with this row (none for `None`) and this theme accent.
fn effective(row: Option<&[(&str, J)]>, theme: Option<&str>) -> J {
    let spy = Spy::memory();
    if let Some(row) = row {
        put_row(&*spy, row);
    }
    if let Some(accent) = theme {
        put_theme(&*spy, accent);
    }
    let (r, _) = reader(&spy, LONG);
    let text = r.fresh().unwrap().effective_json();
    serde_json::from_str(&text).unwrap()
}

/// As [`effective`], with the instance settings given whole (the theme and the `site` keys).
fn effective_with_settings(row: Option<&[(&str, J)]>, settings: J) -> J {
    let spy = Spy::memory();
    if let Some(row) = row {
        put_row(&*spy, row);
    }
    put_settings(&*spy, settings);
    let (r, _) = reader(&spy, LONG);
    let text = r.fresh().unwrap().effective_json();
    serde_json::from_str(&text).unwrap()
}

fn entry(e: &J, key: &str) -> (String, String) {
    let one = &e[key];
    assert!(one.is_object(), "{key} is not in {e}");
    (
        one["value"].as_str().unwrap().to_string(),
        one["source"].as_str().unwrap().to_string(),
    )
}

fn is(e: &J, key: &str, value: &str, source: &str) {
    assert_eq!(
        entry(e, key),
        (value.to_string(), source.to_string()),
        "{key}"
    );
}

#[test]
fn with_no_row_every_value_is_the_default() {
    let e = effective(None, None);
    is(&e, "name", "obop", "default");
    is(&e, "slug", "obop", "default");
    is(&e, "logo", "none", "default");
    is(&e, "tile_accent", "#495670", "default");
    for lang in LANGS {
        is(&e, &format!("title_{lang}"), "obop", "default");
        is(&e, &format!("message_{lang}"), "", "default");
    }
    // The public site with nothing set and no brand: today's defaults.
    is(&e, "site.title", "My Shop", "default");
    is(&e, "site.logo_url", "", "default");
    is(&e, "site.accent", "#0f766e", "default");
    is(&e, "favicon", "tile", "default");
}

#[test]
fn the_object_holds_exactly_the_entries_of_the_surfaces_that_exist() {
    let e = effective(Some(&[("name", s("MOHV"))]), None);
    let mut keys: Vec<&str> = e.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    let mut want: Vec<String> = [
        "name",
        "slug",
        "logo",
        "tile_accent",
        "site.title",
        "site.logo_url",
        "site.accent",
        "favicon",
    ]
    .iter()
    .map(|k| k.to_string())
    .collect();
    for lang in LANGS {
        want.push(format!("title_{lang}"));
        want.push(format!("message_{lang}"));
    }
    want.sort();
    assert_eq!(keys, want.iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(keys.len(), 8 + 14);
    // Every entry is exactly a value and a source, and the source is one of the four.
    for (k, v) in e.as_object().unwrap() {
        assert_eq!(v.as_object().unwrap().len(), 2, "{k}");
        assert!(
            ["record", "site", "theme", "default"].contains(&v["source"].as_str().unwrap()),
            "{k}"
        );
    }
    for later in ["authenticator_issuer", "accent"] {
        assert!(e.get(later).is_none(), "{later} arrives with its surface");
    }
}

#[test]
fn a_stored_name_is_the_record_and_everything_derived_from_it_follows() {
    let e = effective(Some(&[("name", s("MOHV"))]), None);
    is(&e, "name", "MOHV", "record");
    is(&e, "slug", "mohv", "record");
    is(&e, "logo", "tile", "record");
    for lang in LANGS {
        // No title of its own and no English one: the name.
        is(&e, &format!("title_{lang}"), "MOHV", "record");
        is(&e, &format!("message_{lang}"), "", "default");
    }
}

#[test]
fn a_stored_slug_is_the_record_and_a_derived_one_follows_the_name() {
    let e = effective(Some(&[("name", s("MOHV")), ("slug", s("mo-hv"))]), None);
    is(&e, "slug", "mo-hv", "record");
    let e = effective(Some(&[("name", s("Exämple & Söns"))]), None);
    is(&e, "slug", "example-sons", "record");
    // A stored slug that fails its check falls back to the one derived from the effective name.
    let e = effective(Some(&[("name", s("MOHV")), ("slug", s("Foo/Bar"))]), None);
    is(&e, "slug", "mohv", "record");
    // A name that fell back too: the slug is derived from the default name, which is the default.
    let e = effective(
        Some(&[("name", s("<script>")), ("slug", s("Foo/Bar"))]),
        None,
    );
    is(&e, "name", "obop", "default");
    is(&e, "slug", "obop", "default");
}

#[test]
fn a_title_follows_its_own_language_then_english_then_the_name() {
    let e = effective(
        Some(&[
            ("name", s("MOHV")),
            ("title_en", s("Sign in to MOHV")),
            ("title_sv", s("Logga in")),
        ]),
        None,
    );
    is(&e, "title_sv", "Logga in", "record");
    is(&e, "title_en", "Sign in to MOHV", "record");
    // No German title: the English one.
    is(&e, "title_de", "Sign in to MOHV", "record");
    // No English title: the name.
    let e = effective(
        Some(&[("name", s("MOHV")), ("title_sv", s("Logga in"))]),
        None,
    );
    is(&e, "title_en", "MOHV", "record");
    is(&e, "title_sv", "Logga in", "record");
    is(&e, "title_fi", "MOHV", "record");
    // A row whose name fell back and that holds no title: the default name is the default.
    let e = effective(Some(&[("name", s("<b>"))]), None);
    is(&e, "title_en", "obop", "default");
    // A title that is stored and passes stays the record when the name fell back.
    let e = effective(Some(&[("name", s("<b>")), ("title_en", s("Hello"))]), None);
    is(&e, "title_de", "Hello", "record");
}

#[test]
fn a_message_follows_its_own_language_then_english_then_none() {
    let e = effective(
        Some(&[
            ("name", s("MOHV")),
            ("message_en", s("Use your work account")),
            ("message_fi", s("Käytä työtiliäsi")),
        ]),
        None,
    );
    is(&e, "message_fi", "Käytä työtiliäsi", "record");
    is(&e, "message_en", "Use your work account", "record");
    is(&e, "message_de", "Use your work account", "record");
    // No English message and none of its own: none, which is the default.
    let e = effective(Some(&[("name", s("MOHV")), ("message_fi", s("Hei"))]), None);
    is(&e, "message_fi", "Hei", "record");
    is(&e, "message_sv", "", "default");
    is(&e, "message_en", "", "default");
}

#[test]
fn the_logo_is_the_artwork_the_tile_or_none() {
    // A usable artwork.
    let e = effective(
        Some(&[("name", s("MOHV")), ("logo_svg", s(M_ARTWORK))]),
        None,
    );
    is(&e, "logo", "artwork", "record");
    // No artwork: the tile.
    let e = effective(Some(&[("name", s("MOHV"))]), None);
    is(&e, "logo", "tile", "record");
    // An artwork that fails the read check is the tile, and the name is still the record's.
    let e = effective(
        Some(&[
            ("name", s("MOHV")),
            ("logo_svg", s("<svg><script>x</script></svg>")),
        ]),
        None,
    );
    is(&e, "logo", "tile", "record");
    // A name that fell back: the tile is of the default name, which is the default.
    let e = effective(Some(&[("name", s("<b>"))]), None);
    is(&e, "logo", "tile", "default");
    // No row: no logo at all.
    is(&effective(None, None), "logo", "none", "default");
}

#[test]
fn the_tile_accent_is_the_themes_unless_it_is_missing_invalid_or_the_default_violet() {
    let row: &[(&str, J)] = &[("name", s("MOHV"))];
    let e = effective(Some(row), Some("#c2410c"));
    is(&e, "tile_accent", "#c2410c", "theme");
    // Upper case is read as lower case.
    let e = effective(Some(row), Some("#C2410C"));
    is(&e, "tile_accent", "#c2410c", "theme");
    // The default violet is no accent: the slate.
    for violet in ["#6d3fd0", "#6D3FD0"] {
        let e = effective(Some(row), Some(violet));
        is(&e, "tile_accent", "#495670", "default");
    }
    // No theme, or one that is not a #rrggbb.
    is(
        &effective(Some(row), None),
        "tile_accent",
        "#495670",
        "default",
    );
    for bad in ["red", "#abc", "#12345g", "c2410c"] {
        let e = effective(Some(row), Some(bad));
        is(&e, "tile_accent", "#495670", "default");
    }
    // With no row the entry is still true: it is what a tile would be drawn with.
    is(
        &effective(None, Some("#c2410c")),
        "tile_accent",
        "#c2410c",
        "theme",
    );
}

#[test]
fn a_value_written_around_the_command_never_breaks_the_object() {
    // Not text at all, or too long: the entries fall back and the object is still valid JSON.
    let e = effective(
        Some(&[
            ("name", json!(7)),
            ("title_en", json!({"a": 1})),
            ("message_en", s(&"x".repeat(5000))),
            ("logo_svg", json!(false)),
        ]),
        None,
    );
    is(&e, "name", "obop", "default");
    is(&e, "title_en", "obop", "default");
    is(&e, "message_en", "", "default");
    is(&e, "logo", "tile", "default");
}

/// The entries of the public site, one case per value and source (docs/spec/branding.md, section 3
/// and section 5): an explicit `site.*` key is `site` and wins, the record is `record`, the accent of
/// the theme is `theme`, and the defaults are `default`.
#[test]
fn the_shop_entries_name_the_source_that_won() {
    let row: &[(&str, J)] = &[("name", s("MOHV"))];
    // A brand and no `site.*` key: the shop is told the name, the logo route and the tile accent.
    let e = effective_with_settings(Some(row), json!({}));
    is(&e, "site.title", "MOHV", "record");
    let (logo_url, source) = entry(&e, "site.logo_url");
    assert_eq!(source, "record");
    let version = logo_url
        .strip_prefix("/api/brand/logo.svg?v=")
        .unwrap_or("");
    assert_eq!(version.len(), 16, "{logo_url}");
    assert!(version.bytes().all(|b| b.is_ascii_hexdigit()), "{logo_url}");
    is(&e, "site.accent", "#495670", "default");
    // No artwork: the shop keeps its own tile.
    is(&e, "favicon", "tile", "default");

    // An artwork on the record is the shop's tab icon.
    let e = effective_with_settings(
        Some(&[("name", s("MOHV")), ("logo_svg", s(M_ARTWORK))]),
        json!({}),
    );
    is(&e, "favicon", "artwork", "record");
    // An artwork that fails the read check is the tile, which the shop does not use.
    let e = effective_with_settings(
        Some(&[("name", s("MOHV")), ("logo_svg", s("<svg><script/></svg>"))]),
        json!({}),
    );
    is(&e, "favicon", "tile", "default");

    // The theme's accent is the shop's accent, with a source of its own.
    let e = effective_with_settings(Some(row), json!({"theme": {"accent": "#c2410c"}}));
    is(&e, "site.accent", "#c2410c", "theme");
    // The default violet is no accent, as for the tile.
    let e = effective_with_settings(Some(row), json!({"theme": {"accent": "#6d3fd0"}}));
    is(&e, "site.accent", "#495670", "default");

    // Every explicit key wins over the record and over the theme, and says so.
    let e = effective_with_settings(
        Some(row),
        json!({
            "theme": {"accent": "#c2410c"},
            "site": {"title": "Café Nord", "logo_url": "/brand/logo.svg", "accent": "#E11D48"},
        }),
    );
    is(&e, "site.title", "Café Nord", "site");
    is(&e, "site.logo_url", "/brand/logo.svg", "site");
    is(&e, "site.accent", "#e11d48", "site");
    // An explicit key does not change the tab icon: the shop has no key for it.
    is(&e, "favicon", "tile", "default");
    // One explicit key does not take the others with it.
    let e = effective_with_settings(Some(row), json!({"site": {"title": "Café Nord"}}));
    is(&e, "site.title", "Café Nord", "site");
    is(&e, "site.accent", "#495670", "default");
    assert_eq!(entry(&e, "site.logo_url").1, "record");

    // With no brand the explicit keys are still reported, as `site`, and the rest is today's.
    let e = effective_with_settings(
        None,
        json!({"site": {"title": "Café Nord", "accent": "#e11d48"}}),
    );
    is(&e, "site.title", "Café Nord", "site");
    is(&e, "site.accent", "#e11d48", "site");
    is(&e, "site.logo_url", "", "default");
    is(&e, "favicon", "tile", "default");
}

/// A `site.*` key that the shop would not use is no explicit key: the entry says what the shop
/// really shows, so the answer of `get` cannot say `site` for a value the shop ignores.
#[test]
fn a_site_key_the_shop_would_not_use_is_not_reported_as_the_one_in_force() {
    let row: &[(&str, J)] = &[("name", s("MOHV"))];
    for bad in [json!(""), json!("   "), json!(7), json!(null), json!(["x"])] {
        let e = effective_with_settings(Some(row), json!({"site": {"title": bad.clone()}}));
        is(&e, "site.title", "MOHV", "record");
        let e = effective_with_settings(None, json!({"site": {"title": bad}}));
        is(&e, "site.title", "My Shop", "default");
    }
    for bad in ["red", "#abcd", "#12345g", "javascript:alert(1)", ""] {
        let e = effective_with_settings(Some(row), json!({"site": {"accent": bad}}));
        is(&e, "site.accent", "#495670", "default");
        let e = effective_with_settings(None, json!({"site": {"accent": bad}}));
        is(&e, "site.accent", "#0f766e", "default");
    }
    // What the shop does read: the short form and a colour without the hash, normalised.
    for (written, shown) in [
        ("#abc", "#aabbcc"),
        ("e11d48", "#e11d48"),
        (" #E11D48 ", "#e11d48"),
    ] {
        let e = effective_with_settings(Some(row), json!({"site": {"accent": written}}));
        is(&e, "site.accent", shown, "site");
    }
    for bad in [
        "javascript:x",
        "//evil.test/l.png",
        "http://x.test/l.png",
        "/a b",
    ] {
        let e = effective_with_settings(Some(row), json!({"site": {"logo_url": bad}}));
        assert_eq!(entry(&e, "site.logo_url").1, "record", "{bad}");
        let e = effective_with_settings(None, json!({"site": {"logo_url": bad}}));
        is(&e, "site.logo_url", "", "default");
    }
}

/// The settings the entries are decided from are the ones the same read gave the theme: a settings
/// text that does not parse, or has no `site` object, gives the defaults and no failure.
#[test]
fn settings_that_cannot_be_read_give_the_defaults_for_the_shop_entries() {
    let spy = Spy::memory();
    put_row(&*spy, &[("name", s("MOHV"))]);
    spy.apply_checked(&[Op::Put {
        model: SETTING_MODEL.into(),
        id: SETTING_ID.into(),
        fields: fields(&[("value", s("{not json"))]),
    }])
    .unwrap();
    let (r, _) = reader(&spy, LONG);
    let e: J = serde_json::from_str(&r.fresh().unwrap().effective_json()).unwrap();
    is(&e, "site.title", "MOHV", "record");
    is(&e, "site.accent", "#495670", "default");
    // A `site` that is not an object, and a settings text that is not text.
    for settings in [json!({"site": "x"}), json!({"site": [1]}), json!(5)] {
        let e = effective_with_settings(Some(&[("name", s("MOHV"))]), settings);
        is(&e, "site.title", "MOHV", "record");
    }
}
