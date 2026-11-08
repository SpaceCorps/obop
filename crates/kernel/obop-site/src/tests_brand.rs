//! The shop falls back to the instance brand (docs/spec/branding.md, section 5 and the row OT-04 of
//! section 6; section 13, "Public site" and "Cache"). A brand fills the keys of the `site` settings
//! that the owner left unset, an explicit `site.*` key always wins, and the record's artwork is the
//! shop's tab icon. With no brand every answer is what it always was: the pins of the shop files
//! (`SHOP_PINS` in `tests.rs`) hold that, and they are not touched by this file.

use super::*;
use crate::tests::*;
use obop_brand::logo::{LOGO_CACHE_CONTROL, LOGO_CONTENT_TYPE, LOGO_CSP, LOGO_NOSNIFF};
use obop_commands::Store;
use obop_commands::brand::{self, Action};
use serde_json::json;

/// The artwork of the placeholder logo (a plain "M"), in the canonical form the writer prints.
const ARTWORK: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\"><rect width=\"64\" height=\"64\" rx=\"14\" fill=\"#495670\"/><path d=\"M 16 48 L 16 16 L 24 16 L 32 30 L 40 16 L 48 16 L 48 48 L 41 48 L 41 29 L 32 44 L 23 29 L 23 48 Z\" fill=\"#ffffff\"/></svg>";

/// A shop with nothing set: the packages installed, nothing seeded, so no `site.*` key is stored
/// and the home page is the friendly page of a site that is not published yet.
fn bare() -> Shop {
    build(Store::new(), false)
}

/// Write the whole instance settings (the theme and the `site` keys together), as `setting.update`
/// stores them.
fn set_settings(s: &Shop, settings: serde_json::Value) {
    let mut f = obop_storage::Fields::new();
    f.insert("value".into(), json!(settings.to_string()));
    let b = s.store.backend();
    if !b.update("obop_setting", "instance", &f).unwrap() {
        b.insert("obop_setting", "instance", &f).unwrap();
    }
}

/// Set the brand through `brand::apply`, as the command does, on the reader of the shop's
/// dispatcher (made on the first call, as `register_admin` makes it).
fn set_brand(s: &Shop, fields: &[(&str, &str)]) {
    let reader = s.d.install_brand(s.store.backend());
    let action = Action::Configure(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    );
    brand::apply(
        &*s.store.backend(),
        Some(&reader),
        "operator",
        &action,
        false,
    )
    .unwrap_or_else(|e| panic!("the brand was refused: {e}"));
}

/// The version `v` and the bytes the logo route would serve for the brand that is set.
fn logo_of(s: &Shop) -> (String, String) {
    let shown = s.d.brand().current();
    let logo = shown.logo().expect("a brand is set");
    (logo.v().to_string(), logo.served().to_string())
}

fn css_of(s: &Shop) -> String {
    text(&get(s, "/assets/site.css"))
}

/// The whole shop answer that depends on the settings, as one text: the home page, the stylesheet
/// and the tab icon, headers included.
fn shop_files(s: &Shop) -> String {
    ["/", "/assets/site.css", "/favicon.svg", "/favicon.ico"]
        .iter()
        .map(|p| {
            let r = get(s, p);
            let mut headers: Vec<String> = r
                .headers
                .iter()
                .map(|(k, v)| format!("{}: {v}", k.to_ascii_lowercase()))
                .collect();
            headers.sort();
            format!(
                "{p}\n{}\n{}\n\n{}",
                r.status,
                headers.join("\n"),
                r.body_str()
            )
        })
        .collect::<Vec<_>>()
        .join("\n=====\n")
}

/// The shop's own tile for an accent and a letter, as `seo::favicon` draws it.
fn own_tile(accent: &str, letter: &str) -> String {
    let a = theme::resolve(accent);
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\"><rect width=\"64\" height=\"64\" rx=\"14\" fill=\"{}\"/><text x=\"32\" y=\"44\" font-family=\"Georgia,serif\" font-size=\"36\" font-weight=\"700\" text-anchor=\"middle\" fill=\"{}\">{letter}</text></svg>",
        a.light.fill, a.light.on_fill
    )
}

const SLATE: &str = "#495670";

// ---- Each fallback, and each explicit key winning ------------------------------------------------

#[test]
fn with_a_brand_the_shop_takes_its_title_its_logo_and_its_accent_from_the_record() {
    let s = bare();
    let before = text(&get(&s, "/"));
    assert!(before.contains("My Shop"), "the shop as it was: {before}");
    set_brand(&s, &[("name", "MOHV"), ("logo_svg", ARTWORK)]);
    let (v, _) = logo_of(&s);
    let home = text(&get(&s, "/"));
    // The title, the text logo that follows it, and the structured data.
    assert!(home.contains("<span>MOHV</span>"), "{home}");
    assert!(home.contains("<title>MOHV</title>"), "{home}");
    assert!(!home.contains("My Shop"), "{home}");
    // The logo is the logo route, with its version.
    let img = format!("<img src=\"/api/brand/logo.svg?v={v}\" alt=\"\" height=\"34\">");
    assert!(home.contains(&img), "{home}");
    // The accent is the tile accent of the brand: the slate, the theme storing none.
    assert_eq!(css_of(&s), assets::css(SLATE));
    assert_ne!(css_of(&s), assets::css(theme::DEFAULT_ACCENT));
}

#[test]
fn an_explicit_site_key_always_wins_over_the_record() {
    // Every key at once.
    let s = bare();
    set_brand(&s, &[("name", "MOHV"), ("logo_svg", ARTWORK)]);
    set_settings(
        &s,
        json!({"site": {"title": "Café Nord", "logo_url": "/brand/logo.svg", "accent": "#e11d48"}}),
    );
    let home = text(&get(&s, "/"));
    assert!(home.contains("<span>Café Nord</span>"), "{home}");
    assert!(home.contains("<img src=\"/brand/logo.svg\""), "{home}");
    assert!(
        !home.contains("MOHV") && !home.contains("/api/brand/logo.svg"),
        "{home}"
    );
    assert_eq!(css_of(&s), assets::css("#e11d48"));

    // Each key alone: the others still fall back.
    let (v, _) = logo_of(&s);
    set_settings(&s, json!({"site": {"title": "Café Nord"}}));
    let home = text(&get(&s, "/"));
    assert!(home.contains("<span>Café Nord</span>"));
    assert!(home.contains(&format!("src=\"/api/brand/logo.svg?v={v}\"")));
    assert_eq!(css_of(&s), assets::css(SLATE));

    set_settings(
        &s,
        json!({"site": {"logo_url": "https://cdn.example/l.png"}}),
    );
    let home = text(&get(&s, "/"));
    assert!(home.contains("<span>MOHV</span>"));
    assert!(home.contains("<img src=\"https://cdn.example/l.png\""));
    assert!(!home.contains("/api/brand/logo.svg"));

    set_settings(&s, json!({"site": {"accent": "#e11d48"}}));
    let home = text(&get(&s, "/"));
    assert!(home.contains("<span>MOHV</span>") && home.contains("logo.svg?v="));
    assert_eq!(css_of(&s), assets::css("#e11d48"));

    // The text logo follows the title unless it is set itself.
    set_settings(&s, json!({"site": {"logo_text": "M!"}}));
    let home = text(&get(&s, "/"));
    assert!(home.contains("<span>M!</span>"), "{home}");
    assert!(home.contains("<title>MOHV</title>"), "{home}");
}

#[test]
fn a_key_the_shop_would_not_use_does_not_keep_the_record_out() {
    let s = bare();
    set_brand(&s, &[("name", "MOHV")]);
    set_settings(
        &s,
        json!({"site": {"title": "   ", "accent": "red", "logo_url": "javascript:alert(1)"}}),
    );
    let home = text(&get(&s, "/"));
    assert!(home.contains("<span>MOHV</span>"), "{home}");
    assert!(home.contains("logo.svg?v="), "{home}");
    assert!(!home.contains("javascript"), "{home}");
    assert_eq!(css_of(&s), assets::css(SLATE));
}

/// A shop whose instance settings are these, with the brand MOHV set afterwards: the reader of the
/// shop's dispatcher reads the theme with the row, and keeps that for its two seconds, so a theme
/// that changes later is another case.
fn branded_with(settings: serde_json::Value) -> Shop {
    let s = bare();
    set_settings(&s, settings);
    set_brand(&s, &[("name", "MOHV")]);
    s
}

#[test]
fn the_accent_order_is_the_site_key_then_the_tile_accent_then_teal() {
    let theme_of = |a: &str| json!({"theme": {"accent": a}});
    // No brand: teal, whatever the theme says (the shop's accent was never the theme's).
    let s = bare();
    assert_eq!(css_of(&s), assets::css(theme::DEFAULT_ACCENT));
    set_settings(&s, theme_of("#c2410c"));
    assert_eq!(css_of(&s), assets::css(theme::DEFAULT_ACCENT));
    assert_eq!(
        text(&get(&s, "/favicon.svg")),
        own_tile(theme::DEFAULT_ACCENT, "M")
    );

    // A brand: the tile accent. The theme's accent when it stores one ...
    let s = branded_with(theme_of("#c2410c"));
    assert_eq!(css_of(&s), assets::css("#c2410c"));
    assert_eq!(text(&get(&s, "/favicon.svg")), own_tile("#c2410c", "M"));

    // ... the slate when it stores none, only the default violet, or something that is no colour.
    for slate in [
        json!({}),
        theme_of("#6d3fd0"),
        theme_of("#6D3FD0"),
        theme_of("red"),
    ] {
        let s = branded_with(slate.clone());
        assert_eq!(css_of(&s), assets::css(SLATE), "{slate}");
        assert_eq!(
            text(&get(&s, "/favicon.svg")),
            own_tile(SLATE, "M"),
            "{slate}"
        );
    }

    // An explicit `site.accent` wins over both, for the pages and for the tile.
    let s = branded_with(json!({"theme": {"accent": "#c2410c"}, "site": {"accent": "#e11d48"}}));
    assert_eq!(css_of(&s), assets::css("#e11d48"));
    assert_eq!(text(&get(&s, "/favicon.svg")), own_tile("#e11d48", "M"));
}

// ---- The tab icon ----------------------------------------------------------------------------------

fn header<'a>(r: &'a Resp, name: &str) -> &'a str {
    r.get_header(name).unwrap_or("")
}

#[test]
fn the_tab_icon_is_the_artwork_with_the_headers_of_the_logo_route_when_the_brand_has_one() {
    let s = bare();
    set_brand(&s, &[("name", "MOHV"), ("logo_svg", ARTWORK)]);
    let (v, served) = logo_of(&s);
    assert_eq!(served, ARTWORK, "the placeholder is already canonical");
    let svg = get(&s, "/favicon.svg");
    let ico = get(&s, "/favicon.ico");
    for r in [&svg, &ico] {
        assert_eq!(r.status, 200);
        assert_eq!(
            r.body_str(),
            served,
            "the artwork as the reader re-serialised it"
        );
        assert_eq!(header(r, "content-type"), LOGO_CONTENT_TYPE);
        assert_eq!(header(r, "x-content-type-options"), LOGO_NOSNIFF);
        assert_eq!(header(r, "content-security-policy"), LOGO_CSP);
        assert_eq!(header(r, "cache-control"), LOGO_CACHE_CONTROL);
        assert_eq!(
            header(r, "etag"),
            format!("\"{v}\""),
            "the version the reader computed"
        );
        // Nothing of a page: no cookie, no frame header, no site policy.
        assert!(r.get_header("set-cookie").is_none());
        assert!(r.get_header("x-frame-options").is_none());
    }
    assert_eq!(svg.body, ico.body, "the two addresses are the same bytes");
    // The three constants are what the spec says, so a change of the logo route's headers is a
    // change of this answer too.
    assert_eq!(LOGO_CONTENT_TYPE, "image/svg+xml");
    assert_eq!(LOGO_NOSNIFF, "nosniff");
    assert_eq!(LOGO_CSP, "default-src 'none'; style-src 'none'; sandbox");
    assert_eq!(LOGO_CACHE_CONTROL, "no-cache");
}

#[test]
fn a_matching_validator_on_the_tab_icon_is_a_304_without_a_body_or_a_content_type() {
    let s = bare();
    set_brand(&s, &[("name", "MOHV"), ("logo_svg", ARTWORK)]);
    let (v, _) = logo_of(&s);
    let tag = format!("\"{v}\"");
    for matching in [
        tag.clone(),
        format!("W/{tag}"),
        format!("\"other\", {tag}"),
        "*".to_string(),
    ] {
        let r = get_h(&s, "/favicon.svg", &[("if-none-match", &matching)]);
        assert_eq!(r.status, 304, "{matching}");
        assert!(r.body.is_empty());
        assert_eq!(header(&r, "etag"), tag);
        assert_eq!(header(&r, "cache-control"), LOGO_CACHE_CONTROL);
        assert_eq!(header(&r, "x-content-type-options"), LOGO_NOSNIFF);
        assert_eq!(header(&r, "content-security-policy"), LOGO_CSP);
        assert!(r.get_header("content-type").is_none(), "{matching}");
    }
    for other in ["\"0000000000000000\"", "", "x"] {
        let r = get_h(&s, "/favicon.ico", &[("if-none-match", other)]);
        assert_eq!(r.status, 200, "{other:?}");
        assert_eq!(r.body_str(), ARTWORK);
    }
}

#[test]
fn with_no_artwork_the_shops_own_tile_stays() {
    let s = bare();
    let was = get(&s, "/favicon.svg");
    assert_eq!(was.body_str(), own_tile(theme::DEFAULT_ACCENT, "M"));
    assert_eq!(
        header(&was, "cache-control"),
        "public, max-age=86400, stale-while-revalidate=3600"
    );

    // A brand without an artwork: the tile is still the shop's own (its glyph from the logo text,
    // which follows the name; its accent the tile accent), with the shop's own headers.
    set_brand(&s, &[("name", "Zed Goods")]);
    let r = get(&s, "/favicon.svg");
    assert_eq!(r.body_str(), own_tile(SLATE, "Z"));
    assert_eq!(
        header(&r, "cache-control"),
        "public, max-age=86400, stale-while-revalidate=3600"
    );
    assert_ne!(header(&r, "content-security-policy"), LOGO_CSP);
    assert_eq!(get(&s, "/favicon.ico").body, r.body);

    // A stored artwork that fails the read check is the generated tile, which is no artwork: the
    // shop keeps its own tile. (The row is written around the command, into a store whose reader
    // has not read yet.)
    let fresh = bare();
    let backend = fresh.store.backend();
    let mut row = obop_storage::Fields::new();
    row.insert("name".into(), json!("Zed Goods"));
    row.insert("logo_svg".into(), json!("<svg><script>x</script></svg>"));
    backend
        .apply_checked(&[obop_storage::Op::Put {
            model: "instance_brand".into(),
            id: "instance".into(),
            fields: row,
        }])
        .unwrap();
    fresh.d.install_brand(backend);
    assert_eq!(get(&fresh, "/favicon.svg").body_str(), own_tile(SLATE, "Z"));
    assert_ne!(
        get(&fresh, "/favicon.svg").body_str(),
        "<svg><script>x</script></svg>"
    );
}

// ---- The reserved key -----------------------------------------------------------------------------

#[test]
fn a_reserved_key_stored_in_the_settings_reaches_no_page_and_not_the_tab_icon() {
    let hostile = json!({"svg": "<svg onload=\"alert(1)\"></svg>", "v": "0123456789abcdef"});
    let with_key =
        json!({"site": {obop_commands::SITE_FAVICON_KEY: hostile, "title": "Café Nord"}});
    let without_key = json!({"site": {"title": "Café Nord"}});

    // No brand: every shop file is what it is without the key.
    let s = bare();
    set_settings(&s, without_key.clone());
    let clean = shop_files(&s);
    set_settings(&s, with_key.clone());
    assert_eq!(shop_files(&s), clean);
    assert!(!clean.contains("onload"));

    // A brand with no artwork: the same, the shop's own tile.
    set_brand(&s, &[("name", "MOHV")]);
    set_settings(&s, without_key.clone());
    let clean = shop_files(&s);
    set_settings(&s, with_key.clone());
    assert_eq!(shop_files(&s), clean);
    assert!(!clean.contains("onload"));

    // A brand with an artwork: the tab icon is the reader's, never the stored object.
    set_brand(&s, &[("logo_svg", ARTWORK)]);
    let r = get(&s, "/favicon.svg");
    assert_eq!(r.body_str(), ARTWORK);
    assert!(!shop_files(&s).contains("onload"));
    assert_ne!(header(&r, "etag"), "\"0123456789abcdef\"");
}

#[test]
fn a_value_that_is_not_an_artwork_under_the_reserved_name_is_not_one() {
    let key = obop_commands::SITE_FAVICON_KEY;
    let svg = "x".repeat(obop_brand::svg::MAX_BYTES);
    for bad in [
        json!(null),
        json!("<svg/>"),
        json!({}),
        json!({"svg": "<svg/>"}),
        json!({"v": "0123456789abcdef"}),
        json!({"svg": "", "v": "0123456789abcdef"}),
        json!({"svg": "<svg/>", "v": ""}),
        json!({"svg": "<svg/>", "v": "not hex!"}),
        json!({"svg": "<svg/>", "v": "0123456789ABCDEF"}),
        json!({"svg": "<svg/>", "v": "a\"b"}),
        json!({"svg": "<svg/>", "v": "0".repeat(65)}),
        json!({"svg": 5, "v": "0123456789abcdef"}),
        json!({"svg": "<svg/>", "v": 5}),
        json!({"svg": format!("{svg}x"), "v": "0123456789abcdef"}),
    ] {
        let s = settings::Settings::from_map(json!({ key: bad.clone() }).as_object().unwrap());
        assert_eq!(s.favicon, None, "{bad}");
    }
    let ok = settings::Settings::from_map(
        json!({ key: {"svg": svg.clone(), "v": "0123456789abcdef"} })
            .as_object()
            .unwrap(),
    );
    assert_eq!(
        ok.favicon,
        Some(settings::Favicon {
            svg,
            v: "0123456789abcdef".into()
        })
    );
    // It is not a key an owner can set from the command line.
    assert!(settings::parse_value(key, "x").is_err());
}

// ---- The site's own cache ---------------------------------------------------------------------------

/// A shop whose site keeps its settings for an hour, on a store behind a counting double.
fn cached() -> (Shop, std::sync::Arc<obop_storage::testing::Flaky>) {
    let (flaky, backend) = obop_storage::testing::Flaky::memory();
    let s = build(Store::over(backend), false);
    let site = Site::new(
        PublicApi::new(s.d.clone(), s.store.clone()),
        Config {
            settings_ttl: Duration::from_secs(3600),
            ..Config::default()
        },
    );
    (
        Shop {
            site,
            store: s.store,
            d: s.d,
        },
        flaky,
    )
}

/// The site's cache has run out, as the five seconds going by would make it.
fn expire_site_cache(s: &Shop) {
    *s.site.cached.lock().unwrap() = None;
}

#[test]
fn a_changed_name_reaches_a_shop_page_only_after_the_sites_cache_has_run_out() {
    let (s, _) = cached();
    set_brand(&s, &[("name", "MOHV")]);
    assert!(text(&get(&s, "/")).contains("<span>MOHV</span>"));
    // The record changes, in this process: the reader shows it at once ...
    set_brand(&s, &[("name", "Other")]);
    assert_eq!(s.d.brand().current().brand().name(), "Other");
    // ... and the shop does not, until its own cache runs out.
    for _ in 0..3 {
        let home = text(&get(&s, "/"));
        assert!(
            home.contains("<span>MOHV</span>") && !home.contains("Other"),
            "{home}"
        );
        assert!(text(&get(&s, "/favicon.svg")).contains("<text"));
    }
    expire_site_cache(&s);
    let home = text(&get(&s, "/"));
    assert!(
        home.contains("<span>Other</span>") && !home.contains("MOHV"),
        "{home}"
    );
    // A brand removed shows the same way: only after the cache.
    brand::apply(
        &*s.store.backend(),
        Some(&s.d.brand()),
        "operator",
        &Action::Clear,
        false,
    )
    .unwrap();
    assert!(text(&get(&s, "/")).contains("<span>Other</span>"));
    expire_site_cache(&s);
    assert!(text(&get(&s, "/")).contains("<span>My Shop</span>"));
}

#[test]
fn within_the_sites_cache_nothing_is_read_and_a_refresh_asks_the_door_once() {
    let (s, flaky) = cached();
    set_brand(&s, &[("name", "MOHV"), ("logo_svg", ARTWORK)]);
    // The first request fills the cache.
    assert_eq!(get(&s, "/favicon.svg").status, 200);
    flaky.reset_reads();
    for _ in 0..25 {
        assert_eq!(get(&s, "/favicon.svg").status, 200);
        assert_eq!(get(&s, "/favicon.ico").status, 200);
        assert_eq!(get(&s, "/assets/site.css").status, 200);
    }
    assert_eq!(
        flaky.reads().gets,
        0,
        "the cache answers: no read of the settings, no read of the brand"
    );

    // One refresh reads the settings once through the door; the reader is asked once, and it reads
    // the row and the theme (two reads) only if its own cell has run out, so a refresh is one read
    // or three, never two refreshes' worth (2) or more.
    for _ in 0..4 {
        flaky.reset_reads();
        expire_site_cache(&s);
        assert_eq!(get(&s, "/favicon.svg").status, 200);
        let gets = flaky.reads().gets;
        assert!(gets == 1 || gets == 3, "one refresh made {gets} reads");
        assert_eq!(get(&s, "/favicon.ico").status, 200);
        assert_eq!(
            flaky.reads().gets,
            gets,
            "the second request of the refresh read nothing"
        );
    }
}

// ---- The lengths ------------------------------------------------------------------------------------

#[test]
fn the_longest_name_arrives_whole_as_the_title_and_the_text_logo() {
    // A name is at most 40 characters, a title 80 and a text logo 40: nothing is cut.
    let name: String = "Ä".repeat(40);
    let s = bare();
    set_brand(&s, &[("name", &name)]);
    let settings = s.site.api().site_settings();
    let parsed = settings::Settings::from_map(&settings);
    assert_eq!(parsed.title, name);
    assert_eq!(parsed.logo_text, name);
    assert!(text(&get(&s, "/")).contains(&format!("<span>{name}</span>")));
    // The glyph of the tile follows the name.
    assert_eq!(get(&s, "/favicon.svg").body_str(), own_tile(SLATE, "Ä"));
}

// ---- The door and the settings agree on what is set -----------------------------------------------------

/// `site_settings` decides which keys are unset with its own copy of the rules of
/// `Settings::from_map`, because `obop-commands` cannot call this crate. This holds the two to each
/// other over a table: a value the shop uses is kept and shows as itself, one it would refuse is
/// replaced by the record's value.
#[test]
fn the_door_and_the_settings_agree_on_which_keys_are_set() {
    let s = bare();
    set_brand(&s, &[("name", "MOHV")]);
    let (v, _) = logo_of(&s);
    let route = format!("/api/brand/logo.svg?v={v}");

    // (key, stored value, what the shop shows for it when it uses it: None when it does not).
    type Case = (&'static str, serde_json::Value, Option<&'static str>);
    let cases: Vec<Case> = vec![
        ("title", json!("Shop X"), Some("Shop X")),
        ("title", json!("  Shop X\n"), Some("Shop X")),
        ("title", json!("My Shop"), Some("My Shop")),
        ("title", json!("a\u{7}b"), Some("ab")),
        ("title", json!(""), None),
        ("title", json!("  \t "), None),
        ("title", json!("\u{7}"), None),
        ("title", json!(7), None),
        ("title", json!(null), None),
        ("title", json!(["x"]), None),
        ("accent", json!("#e11d48"), Some("#e11d48")),
        ("accent", json!("#E11D48"), Some("#e11d48")),
        ("accent", json!("e11d48"), Some("#e11d48")),
        ("accent", json!("#abc"), Some("#aabbcc")),
        ("accent", json!(" #e11d48 "), Some("#e11d48")),
        ("accent", json!("#0f766e"), Some("#0f766e")),
        ("accent", json!("red"), None),
        ("accent", json!("#abcd"), None),
        ("accent", json!("#12345g"), None),
        ("accent", json!("#e11d48e11d48"), None),
        ("accent", json!("javascript:alert(1)"), None),
        ("accent", json!(5), None),
        ("accent", json!(""), None),
        ("logo_url", json!("/brand/l.svg"), Some("/brand/l.svg")),
        (
            "logo_url",
            json!("https://cdn.example/l.png"),
            Some("https://cdn.example/l.png"),
        ),
        ("logo_url", json!("javascript:x"), None),
        ("logo_url", json!("//evil.test/l.png"), None),
        ("logo_url", json!("http://cdn.example/l.png"), None),
        ("logo_url", json!("https://user@cdn.example/l.png"), None),
        ("logo_url", json!("/a b"), None),
        ("logo_url", json!("/a\"b"), None),
        ("logo_url", json!("/a'b"), None),
        ("logo_url", json!("/a<b"), None),
        ("logo_url", json!("/a\\b"), None),
        ("logo_url", json!("x".repeat(600)), None),
        ("logo_url", json!(1), None),
        ("logo_url", json!(""), None),
    ];
    let brand_value = |key: &str| match key {
        "title" => "MOHV".to_string(),
        "accent" => SLATE.to_string(),
        _ => route.clone(),
    };
    for (key, stored, shown) in cases {
        let one = |m: serde_json::Value| settings::Settings::from_map(m.as_object().unwrap());
        let pick = |st: &settings::Settings| match key {
            "title" => st.title.clone(),
            "accent" => st.accent.clone(),
            _ => st.logo_url.clone().unwrap_or_default(),
        };
        // What the shop does with the key on its own.
        let alone = pick(&one(json!({ key: stored.clone() })));
        let default = pick(&one(json!({})));
        match shown {
            Some(want) => assert_eq!(alone, want, "{key}={stored}: the shop uses it"),
            None => assert_eq!(alone, default, "{key}={stored}: the shop ignores it"),
        }
        // What the door makes of the same settings, read by the shop.
        set_settings(&s, json!({"site": { key: stored.clone() }}));
        let through_the_door = pick(&settings::Settings::from_map(&s.site.api().site_settings()));
        let want = shown.map_or_else(|| brand_value(key), str::to_string);
        assert_eq!(through_the_door, want, "{key}={stored}");
    }
}
