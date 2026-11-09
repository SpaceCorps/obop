//! The `effective` answer of `get` (docs/spec/branding.md, section 3, "What `get` shows", and section
//! 13, "Commands"): each value in force with the source that won, one case per value and source. The
//! object is staged by slice, and these tests pin what it holds: an entry whose precedence involves a
//! surface that is not wired would be untrue, so a surface's entry arrives with its slice.
//! `authenticator_issuer` is there since the slice that reads the record for the enrolment texts
//! (S1e); its cases, with the variable `OBOP_MFA_ISSUER` given as a value, are the last in this file.
//! The entries of the public site (`site.title`, `site.logo_url`, `site.accent` and `favicon`)
//! arrived with the slice that wires the shop to the record (S1f); their cases come before those of
//! the issuer. `accent`, the accent of the sign-in screens, arrived with the back office page (S2).

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
    // The variable `OBOP_MFA_ISSUER` of this process is not an input of these tests.
    let text = r.fresh().unwrap().effective_json_with(None);
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
    is(&e, "authenticator_issuer", "obop", "default");
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
        "accent",
        "site.title",
        "site.logo_url",
        "site.accent",
        "favicon",
        "authenticator_issuer",
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
    assert_eq!(keys.len(), 10 + 14);
    // Every entry is exactly a value and a source, and the source is one of the five a good row
    // gives (a sixth, `unreadable`, is the issuer of a name an enrolment would refuse, below).
    for (k, v) in e.as_object().unwrap() {
        assert_eq!(v.as_object().unwrap().len(), 2, "{k}");
        assert!(
            ["record", "site", "env", "theme", "default"].contains(&v["source"].as_str().unwrap()),
            "{k}"
        );
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

/// The accent of the sign-in screens (S2): the theme's stored accent, else the stock violet.
#[test]
fn the_accent_of_the_sign_in_screens_is_the_themes_else_the_stock_violet() {
    let row: &[(&str, J)] = &[("name", s("MOHV"))];
    is(&effective(Some(row), None), "accent", "#6d3fd0", "default");
    is(&effective(None, None), "accent", "#6d3fd0", "default");
    is(
        &effective(Some(row), Some("#c2410c")),
        "accent",
        "#c2410c",
        "theme",
    );
    is(
        &effective(Some(row), Some("#C2410C")),
        "accent",
        "#c2410c",
        "theme",
    );
    // A theme that stores exactly the stock violet stores it: the screens show that violet, and the
    // tile stays slate (spec section 2).
    is(
        &effective(Some(row), Some("#6d3fd0")),
        "accent",
        "#6d3fd0",
        "theme",
    );
    is(
        &effective(Some(row), Some("#6d3fd0")),
        "tile_accent",
        "#495670",
        "default",
    );
    for bad in ["red", "#abc", "#12345g", "c2410c"] {
        is(
            &effective(Some(row), Some(bad)),
            "accent",
            "#6d3fd0",
            "default",
        );
    }
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

/// A row whose name fell back is a brand with the default name (spec section 2): the shop is the
/// brand's, and its title is `obop`, but that is the default name and not a value the record holds,
/// so `get` says `default` for it, as it does for `name`, and for the logo route when it serves the
/// tile that is drawn from that name. An artwork that passed is the record's all the same, and an
/// explicit key still wins.
#[test]
fn a_name_that_fell_back_makes_the_shop_title_the_default_and_not_the_record() {
    let long = "x".repeat(80);
    for bad in ["<b>x</b>", long.as_str(), "a\u{202e}b"] {
        let e = effective_with_settings(Some(&[("name", s(bad))]), json!({}));
        is(&e, "name", "obop", "default");
        is(&e, "site.title", "obop", "default");
        is(&e, "logo", "tile", "default");
        let (url, source) = entry(&e, "site.logo_url");
        assert!(url.starts_with("/api/brand/logo.svg?v="), "{url}");
        assert_eq!(source, "default", "the tile of the default name: {bad:?}");
        is(&e, "favicon", "tile", "default");

        // An artwork that passed the read check is the record's, whatever the name did.
        let e = effective_with_settings(
            Some(&[("name", s(bad)), ("logo_svg", s(M_ARTWORK))]),
            json!({}),
        );
        is(&e, "site.title", "obop", "default");
        is(&e, "logo", "artwork", "record");
        assert_eq!(entry(&e, "site.logo_url").1, "record", "{bad:?}");
        is(&e, "favicon", "artwork", "record");

        // An explicit key wins over the fallen-back name and says so.
        let e = effective_with_settings(
            Some(&[("name", s(bad))]),
            json!({"site": {"title": "Café Nord", "logo_url": "/brand/logo.svg"}}),
        );
        is(&e, "site.title", "Café Nord", "site");
        is(&e, "site.logo_url", "/brand/logo.svg", "site");
    }
}

/// A name stored with a space at its start or end passes the read check and is the shop's title
/// without it (the shop trims its title), and a name of nothing but spaces fails the read check as
/// empty, so it is the default name: `get` reports the title the page shows in both cases. (The
/// test in `obop-site` holds `get` to the page itself.)
#[test]
fn a_padded_name_is_reported_as_the_title_the_shop_shows_and_a_blank_one_as_the_default_name() {
    for padded in ["  MOHV  ", "MOHV ", " MOHV"] {
        let e = effective_with_settings(Some(&[("name", s(padded))]), json!({}));
        is(&e, "name", padded, "record");
        is(&e, "site.title", "MOHV", "record");
    }
    for blank in [" ", "    ", &" ".repeat(obop_brand::policy::NAME_MAX)] {
        let e = effective_with_settings(Some(&[("name", s(blank))]), json!({}));
        is(&e, "name", "obop", "default");
        is(&e, "site.title", "obop", "default");
        is(&e, "slug", "obop", "default");
    }
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

// ---- authenticator_issuer (S1e) -----------------------------------------------------------------------
//
// The label and `issuer=` of a new authenticator entry: the variable `OBOP_MFA_ISSUER` if it is set,
// else the stored name, else `obop`. `Auth::begin` makes the same choice at each enrolment (its
// tests are in `obop-auth`); this is what `get` says about it. The variable is a parameter of
// `effective_json_with`, so no test changes the environment of the process.

/// As `effective`, with a value for the variable.
fn effective_with_variable(row: Option<&[(&str, J)]>, variable: Option<&str>) -> J {
    let spy = Spy::memory();
    if let Some(row) = row {
        put_row(&*spy, row);
    }
    let (r, _) = reader(&spy, LONG);
    let text = r
        .fresh()
        .unwrap()
        .effective_json_with(variable.map(String::from));
    serde_json::from_str(&text).unwrap()
}

#[test]
fn the_authenticator_issuer_names_the_source_that_won_in_each_case() {
    // Neither: the default.
    let e = effective_with_variable(None, None);
    is(&e, "authenticator_issuer", "obop", "default");
    // The record only.
    let e = effective_with_variable(Some(&[("name", s("MOHV"))]), None);
    is(&e, "authenticator_issuer", "MOHV", "record");
    // The variable only.
    let e = effective_with_variable(None, Some("Acme"));
    is(&e, "authenticator_issuer", "Acme", "env");
    // The variable over the record.
    let e = effective_with_variable(Some(&[("name", s("MOHV"))]), Some("Acme"));
    is(&e, "authenticator_issuer", "Acme", "env");
    // The variable that says `obop` beats a record that says MOHV, and `get` says it is the
    // variable that won: a value, not the absence of one.
    let e = effective_with_variable(Some(&[("name", s("MOHV"))]), Some("obop"));
    is(&e, "authenticator_issuer", "obop", "env");
    is(&e, "name", "MOHV", "record");
}

/// The entry is `null` from the source `unreadable`, and says no value.
fn is_unreadable(e: &J, what: &str) {
    assert_eq!(
        e["authenticator_issuer"],
        json!({"value": null, "source": "unreadable"}),
        "{what}"
    );
}

/// Where an enrolment is refused there is no issuer in force, and `get` says so instead of the
/// default that the enrolment would not write: `null` from the source `unreadable`, while `name`
/// still shows the default as it does for any stored name that failed the read check.
#[test]
fn a_stored_name_that_failed_the_read_check_has_no_issuer_in_force() {
    for bad in [
        s("<script>"),
        s(""),
        // Nothing but U+0020 is empty for the read check since S1c: the display shows `obop` for it
        // and the enrolment is refused as for any name that cannot be shown.
        s(" "),
        s("  "),
        s(&" ".repeat(40)),
        s("a\u{200b}b"),
        s(&"x".repeat(41)),
        J::from(5),
    ] {
        let e = effective_with_variable(Some(&[("name", bad.clone())]), None);
        is(&e, "name", "obop", "default");
        is_unreadable(&e, &bad.to_string());
        // The variable does not care about the row at all.
        let e = effective_with_variable(Some(&[("name", bad.clone())]), Some("Acme"));
        is(&e, "authenticator_issuer", "Acme", "env");
    }
    // A row with no name at all is the same case.
    let e = effective_with_variable(Some(&[("title_en", s("Hello"))]), None);
    is_unreadable(&e, "no name");
}

/// A name that passes the read check and still cannot go into a label (a link character) is
/// refused by `strict` and so has no issuer in force either; `name` shows it as it is, because the
/// display accepts it. (A name of only spaces is not of this kind since S1c: the read check calls
/// it empty, see the test above.)
#[test]
fn a_stored_name_that_cannot_go_into_a_label_has_no_issuer_in_force() {
    for bad in ["a:b", "a/b", "a\\b", "a@b", "http://evil.test/x"] {
        let e = effective_with_variable(Some(&[("name", s(bad))]), None);
        is_unreadable(&e, bad);
        assert_eq!(
            e["name"]["source"],
            json!("record"),
            "the display shows the name that passed the read check: {bad:?}"
        );
        let e = effective_with_variable(Some(&[("name", s(bad))]), Some("Acme"));
        is(&e, "authenticator_issuer", "Acme", "env");
    }
    // The ordinary punctuation of a name is no reason.
    for good in ["Acme, Inc.", "O'Neil & Sons", "Müller (DE)", "a-b_c+d!"] {
        let e = effective_with_variable(Some(&[("name", s(good))]), None);
        is(&e, "authenticator_issuer", good, "record");
    }
}

#[test]
fn the_issuer_follows_the_name_and_not_the_title_or_the_slug() {
    let e = effective_with_variable(
        Some(&[
            ("name", s("MOHV")),
            ("slug", s("mo-hv")),
            ("title_en", s("MOHV Mileage")),
        ]),
        None,
    );
    is(&e, "authenticator_issuer", "MOHV", "record");
}
