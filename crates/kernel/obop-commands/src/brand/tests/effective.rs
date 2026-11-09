//! The `effective` answer of `get` (docs/spec/branding.md, section 3, "What `get` shows", and section
//! 13, "Commands"): each value in force with the source that won, one case per value and source. The
//! object is staged by slice, so these tests also pin what it does not hold yet: the entries of the
//! surfaces that are not built (`accent`, the `site.*` keys and the favicon) are not there, because
//! an entry whose precedence involves a surface that is not wired would be untrue.
//! `authenticator_issuer` is there since the slice that reads the record for the enrolment texts
//! (S1e); its cases, with the variable `OBOP_MFA_ISSUER` given as a value, are the last in this file.

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
    assert_eq!(keys.len(), 5 + 14);
    // Every entry is exactly a value and a source, and the source is one of the four a good row
    // gives (the fifth, `unreadable`, is the issuer of a name an enrolment would refuse, below).
    for (k, v) in e.as_object().unwrap() {
        assert_eq!(v.as_object().unwrap().len(), 2, "{k}");
        assert!(
            ["record", "env", "theme", "default"].contains(&v["source"].as_str().unwrap()),
            "{k}"
        );
    }
    for later in [
        "accent",
        "site.title",
        "site.logo_url",
        "site.accent",
        "favicon",
    ] {
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
