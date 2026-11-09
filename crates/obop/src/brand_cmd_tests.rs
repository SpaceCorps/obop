//! The argument handling and the `status` text of `obop admin brand` (docs/spec/branding.md,
//! section 13, "Merge": "`status incident create --title T` still works, because the repeatable
//! flags of `admin brand` are taken before the global ones"). The commands through the real binary,
//! the `--from` cases and the local smoke are in `tests/brand_cli.rs`.

use crate::brand_cmd::{
    Flags, Said, USAGE, VALUE_OPTIONS, clear_over, configure_over, render_status,
    render_status_with, status_over, storage_said, take_flags,
};
use obop_commands::Store;
use obop_commands::brand::{Action, Reader, apply};
use obop_storage::testing::{Flaky, Mode};
use obop_storage::{Backend, Fields, Op};
use serde_json::json;

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

/// The languages the usage text lists are the languages of the record, all seven and in their order.
#[test]
fn the_usage_lists_every_language_of_the_record() {
    let line = USAGE
        .lines()
        .find(|l| l.contains("LANG is one of"))
        .unwrap();
    assert!(
        line.ends_with(&format!(
            "LANG is one of {})",
            obop_commands::brand::LANGS.join(", ")
        )),
        "{line}"
    );
}

/// A correctly spelled call finds its area behind any global option that takes a value, not only
/// `--db` and `--token` (a wrong message and status 2 for `obop --label t admin brand configure ...`
/// was the failure), and behind a flag without a value.
#[test]
fn the_area_is_found_behind_any_global_option() {
    for option in VALUE_OPTIONS {
        let args = vec![
            option,
            "x",
            "admin",
            "brand",
            "configure",
            "--name",
            "N",
            "--title",
            "sv=Hej",
        ];
        let (flags, rest) = take_flags(args.clone()).unwrap();
        let flags = flags.unwrap_or_else(|| panic!("{option}: the area was not found"));
        assert_eq!(flags.name.as_deref(), Some("N"), "{option}");
        assert_eq!(flags.titles, pairs(&[("sv", "Hej")]), "{option}");
        assert_eq!(
            rest,
            vec![option, "x", "admin", "brand", "configure"],
            "{option}"
        );
    }
    // Several of them, a flag with no value among them, and the words after.
    let (flags, rest) = take_flags(vec![
        "--label",
        "t",
        "--dry-run",
        "--db",
        "sqlite://x",
        "admin",
        "brand",
        "clear",
    ])
    .unwrap();
    assert_eq!(flags, Some(Flags::default()));
    assert_eq!(rest.len(), 8);
    // `--year N` (the SIE options of `main`) is a global option with a value like the others: the
    // `2026` after it is not the command word, and a `--title` after the words is the area's.
    let (flags, rest) = take_flags(vec![
        "--year",
        "2026",
        "admin",
        "brand",
        "configure",
        "--title",
        "sv=X",
        "--dry-run",
    ])
    .unwrap();
    let flags = flags.expect("the area was not found behind --year 2026");
    assert_eq!(flags.titles, pairs(&[("sv", "X")]));
    assert_eq!(
        rest,
        vec!["--year", "2026", "admin", "brand", "configure", "--dry-run"]
    );
    // Another command with the same words later on is not the area.
    let (flags, _) = take_flags(vec!["--label", "t", "call", "admin", "brand"]).unwrap();
    assert_eq!(flags, None);
}

/// The list the area uses is the list `main` takes: every `take_opt(&args, "--x")` of `main.rs`
/// is a value option here and every value option here is taken there.
#[test]
fn the_value_options_are_the_ones_main_takes() {
    let main = include_str!("main.rs");
    let marker = "take_opt(&args, \"";
    let mut taken = Vec::new();
    let mut rest = main;
    while let Some(i) = rest.find(marker) {
        rest = &rest[i + marker.len()..];
        taken.push(&rest[..rest.find('"').unwrap()]);
    }
    taken.sort_unstable();
    let mut listed = VALUE_OPTIONS.to_vec();
    listed.sort_unstable();
    assert_eq!(taken, listed);
}

#[test]
fn every_other_command_line_goes_on_untouched() {
    for args in [
        vec![
            "status",
            "incident",
            "create",
            "--title",
            "T",
            "--message",
            "M",
        ],
        vec!["call", "party", "list"],
        vec!["admin", "sso", "status"],
        vec!["admin", "users", "--title", "x"],
        vec!["serve", "--db", "sqlite://x"],
        vec![],
    ] {
        let (flags, rest) = take_flags(args.clone()).unwrap();
        assert_eq!(flags, None, "{args:?}");
        assert_eq!(rest, args);
    }
}

#[test]
fn the_flags_of_the_area_are_pulled_out_and_the_rest_keeps_its_order() {
    let args = vec![
        "admin",
        "brand",
        "configure",
        "--db",
        "sqlite://x",
        "--title",
        "sv=Logga in",
        "--name",
        "MOHV",
        "--dry-run",
        "--title",
        "en=Log in",
        "--message",
        "fi=Hei",
        "--slug",
        "mohv",
        "--logo-file",
        "logo.svg",
        "--hide-product-docs",
        "on",
        "--from",
        "values.toml",
    ];
    let (flags, rest) = take_flags(args).unwrap();
    assert_eq!(
        flags,
        Some(Flags {
            name: Some("MOHV".into()),
            slug: Some("mohv".into()),
            titles: pairs(&[("sv", "Logga in"), ("en", "Log in")]),
            messages: pairs(&[("fi", "Hei")]),
            logo_file: Some("logo.svg".into()),
            clear_logo: false,
            hide_product_docs: Some(true),
        })
    );
    // The global options stay for the global handling, `--from` included.
    assert_eq!(
        rest,
        [
            "admin",
            "brand",
            "configure",
            "--db",
            "sqlite://x",
            "--dry-run",
            "--from",
            "values.toml"
        ]
    );
    let (flags, rest) = take_flags(vec!["admin", "brand", "configure", "--clear-logo"]).unwrap();
    assert!(flags.unwrap().clear_logo);
    assert_eq!(rest, ["admin", "brand", "configure"]);
}

#[test]
fn the_area_is_found_past_leading_global_options_and_only_there() {
    for args in [
        vec![
            "--db",
            "sqlite://x",
            "admin",
            "brand",
            "configure",
            "--title",
            "sv=A",
        ],
        vec![
            "--dev-auth",
            "admin",
            "brand",
            "configure",
            "--title",
            "sv=A",
        ],
        vec![
            "--token",
            "t",
            "--dev-auth",
            "admin",
            "brand",
            "configure",
            "--title",
            "sv=A",
        ],
    ] {
        let (flags, rest) = take_flags(args.clone()).unwrap();
        assert_eq!(flags.unwrap().titles, pairs(&[("sv", "A")]), "{args:?}");
        assert!(!rest.contains(&"--title"), "{rest:?}");
    }
    // A command that only mentions the words, or has them after another word, is not the area.
    for args in [
        vec!["call", "admin", "brand", "--title", "x"],
        vec!["serve", "--title", "x", "admin", "brand", "status"],
        vec!["x", "admin", "brand", "status"],
        vec!["admin", "--db", "x", "brand", "status"],
    ] {
        let (flags, rest) = take_flags(args.clone()).unwrap();
        assert_eq!(flags, None, "{args:?}");
        assert_eq!(rest, args);
    }
}

#[test]
fn a_text_may_be_empty_hold_a_sign_or_start_with_dashes() {
    let (flags, _) = take_flags(vec![
        "admin",
        "brand",
        "configure",
        "--title",
        "sv=",
        "--message",
        "en=a=b",
        "--title",
        "de=--x",
        "--name",
        "--clear-logo",
    ])
    .unwrap();
    let flags = flags.unwrap();
    // `--title sv=` clears the Swedish title; the text is everything after the first sign.
    assert_eq!(flags.titles, pairs(&[("sv", ""), ("de", "--x")]));
    assert_eq!(flags.messages, pairs(&[("en", "a=b")]));
    // The value of a flag is the next argument, whatever it looks like.
    assert_eq!(flags.name.as_deref(), Some("--clear-logo"));
    assert!(!flags.clear_logo);
}

#[test]
fn a_flag_that_cannot_be_read_is_refused_without_repeating_the_argument() {
    let area = ["admin", "brand", "configure"];
    let with = |more: &[&'static str]| {
        let mut v = area.to_vec();
        v.extend_from_slice(more);
        take_flags(v).unwrap_err()
    };
    assert_eq!(with(&["--name"]), "--name needs a value");
    assert_eq!(with(&["--slug"]), "--slug needs a value");
    assert_eq!(with(&["--logo-file"]), "--logo-file needs a value");
    // `--from` stays a global option, but a `--from` with no file is refused here, or the call would
    // go on as if no file had been meant.
    assert_eq!(with(&["--from"]), "--from needs a file");
    assert_eq!(with(&["--name", "A", "--from"]), "--from needs a file");
    assert!(with(&["--title"]).starts_with("--title needs LANG=TEXT"));
    assert!(with(&["--message"]).starts_with("--message needs LANG=TEXT"));
    assert!(with(&["--title", "NOSIGNSECRET"]).starts_with("--title needs LANG=TEXT"));
    assert!(!with(&["--title", "NOSIGNSECRET"]).contains("NOSIGNSECRET"));
    let unknown = with(&["--message", "xx=LANGSECRET"]);
    assert_eq!(
        unknown,
        "--message: the language must be one of en, de, fr, pl, fi, sv, es"
    );
    assert!(!unknown.contains("LANGSECRET"));
    assert_eq!(
        with(&["--name", "A", "--name", "B"]),
        "--name is given twice"
    );
    assert_eq!(
        with(&["--slug", "a", "--slug", "b"]),
        "--slug is given twice"
    );
    assert_eq!(
        with(&["--logo-file", "a", "--logo-file", "b"]),
        "--logo-file is given twice"
    );
    assert_eq!(
        with(&["--title", "sv=a", "--title", "sv=b"]),
        "--title sv is given twice"
    );
    assert_eq!(
        with(&["--message", "fi=a", "--message", "fi=b"]),
        "--message fi is given twice"
    );
    // A title and a message of one language are two fields.
    assert!(
        take_flags(vec![
            "admin",
            "brand",
            "configure",
            "--title",
            "sv=a",
            "--message",
            "sv=b"
        ])
        .is_ok()
    );
}

fn memory() -> (Store, std::sync::Arc<dyn Backend>) {
    let store = Store::new();
    let backend = store.backend();
    (store, backend)
}

/// The text of `status`, with no value for `OBOP_MFA_ISSUER`: these tests do not depend on the
/// environment of the process (the cases with a value are `status_names_the_variable_*`).
fn status_of(backend: &std::sync::Arc<dyn Backend>) -> String {
    render_status_with(&Reader::new(backend.clone()).fresh().unwrap(), None)
}

fn configure(backend: &std::sync::Arc<dyn Backend>, fields: &[(&str, &str)]) {
    apply(
        &**backend,
        None,
        "operator",
        &Action::Configure(pairs(fields)),
        false,
    )
    .unwrap();
}

#[test]
fn status_of_an_instance_with_no_brand_says_so_and_shows_the_defaults() {
    let (_store, backend) = memory();
    assert_eq!(
        status_of(&backend),
        "brand: not set (no record; every value is the default)\n\
         name: \"obop\" (default)\n\
         slug: \"obop\" (default)\n\
         logo: none (default)\n\
         tile_accent: \"#495670\" (default)\n\
         accent: \"#6d3fd0\" (default)\n\
         authenticator_issuer: \"obop\" (default)\n\
         favicon: \"tile\" (default)\n\
         site.accent: \"#0f766e\" (default)\n\
         site.logo_url: \"\" (default)\n\
         site.title: \"My Shop\" (default)\n\
         fallbacks: none\n"
    );
}

/// Write the instance settings (the theme and the `site` keys together) around the command.
fn put_settings(backend: &std::sync::Arc<dyn Backend>, settings: serde_json::Value) {
    let mut fields = Fields::new();
    fields.insert("value".into(), json!(settings.to_string()));
    backend
        .apply_checked(&[Op::Put {
            model: "obop_setting".into(),
            id: "instance".into(),
            fields,
        }])
        .unwrap();
}

/// With no record, the header says that every value is the default, and when the owner has set a
/// `site.*` key of the shop (its entry then has the source `site`) it says "except what a site.* key
/// sets". A theme alone, or a `site.*` key the shop would not use, leaves the header as it was: the
/// theme's accent is not named by the header (its own line says `(theme)`), because the outputs of
/// an instance with no record are held as they have been since S1a.
#[test]
fn status_with_no_record_and_an_explicit_site_key_says_which_values_are_not_the_default() {
    let (_store, backend) = memory();
    put_settings(
        &backend,
        json!({"site": {"title": "Fernhill Goods", "accent": "#2d6a4f"}}),
    );
    assert_eq!(
        status_of(&backend),
        "brand: not set (no record; every value is the default, except what a site.* key sets)\n\
         name: \"obop\" (default)\n\
         slug: \"obop\" (default)\n\
         logo: none (default)\n\
         tile_accent: \"#495670\" (default)\n\
         authenticator_issuer: \"obop\" (default)\n\
         favicon: \"tile\" (default)\n\
         site.accent: \"#2d6a4f\" (site)\n\
         site.logo_url: \"\" (default)\n\
         site.title: \"Fernhill Goods\" (site)\n\
         fallbacks: none\n"
    );
    // A theme is no `site.*` key, and a key the shop does not use is not one in force.
    for settings in [
        json!({"theme": {"accent": "#c2410c"}}),
        json!({"site": {"title": "   ", "accent": "red", "logo_url": "http://x.test/l.png"}}),
        json!({"site": {}}),
    ] {
        let (_store, backend) = memory();
        put_settings(&backend, settings.clone());
        let text = status_of(&backend);
        assert!(
            text.starts_with("brand: not set (no record; every value is the default)\n"),
            "{settings}: {text}"
        );
        assert!(!text.contains("(site)"), "{settings}: {text}");
    }
}

/// The same for `OBOP_MFA_ISSUER` (slice S1e): with no record a value of the variable is a value that
/// is not the default (its entry has the source `env`), so the header may not say that every value
/// is, and it names the variable, alone or beside a `site.*` key when both are in force. With a
/// record the header is `brand: set` whatever the variable says.
#[test]
fn status_with_no_record_and_the_issuer_variable_says_which_values_are_not_the_default() {
    let (_store, backend) = memory();
    let fresh = Reader::new(backend.clone()).fresh().unwrap();
    let text = render_status_with(&fresh, Some("Acme".into()));
    assert_eq!(
        text,
        "brand: not set (no record; every value is the default, except what OBOP_MFA_ISSUER sets)\n\
         name: \"obop\" (default)\n\
         slug: \"obop\" (default)\n\
         logo: none (default)\n\
         tile_accent: \"#495670\" (default)\n\
         authenticator_issuer: \"Acme\" (env)\n\
         favicon: \"tile\" (default)\n\
         site.accent: \"#0f766e\" (default)\n\
         site.logo_url: \"\" (default)\n\
         site.title: \"My Shop\" (default)\n\
         fallbacks: none\n"
    );
    // With no variable the header is the plain one, as the test above holds.
    assert!(
        render_status_with(&fresh, None)
            .starts_with("brand: not set (no record; every value is the default)\n")
    );
    // Both in force: both are named.
    put_settings(&backend, json!({"site": {"title": "Fernhill Goods"}}));
    let fresh = Reader::new(backend.clone()).fresh().unwrap();
    let text = render_status_with(&fresh, Some("Acme".into()));
    assert!(
        text.starts_with(
            "brand: not set (no record; every value is the default, except what a site.* key or OBOP_MFA_ISSUER sets)\n"
        ),
        "{text}"
    );
    assert!(
        text.contains("authenticator_issuer: \"Acme\" (env)\n"),
        "{text}"
    );
    assert!(
        text.contains("site.title: \"Fernhill Goods\" (site)\n"),
        "{text}"
    );
    // A record: the header is the short one, the variable's entry is as before.
    configure(&backend, &[("name", "MOHV")]);
    let fresh = Reader::new(backend.clone()).fresh().unwrap();
    let text = render_status_with(&fresh, Some("Acme".into()));
    assert!(text.starts_with("brand: set\n"), "{text}");
    assert!(
        text.contains("authenticator_issuer: \"Acme\" (env)\n"),
        "{text}"
    );
}

#[test]
fn status_shows_the_values_in_force_the_titles_and_messages_that_are_set_and_the_logo_version() {
    let (_store, backend) = memory();
    configure(
        &backend,
        &[
            ("name", "MOHV"),
            ("title_sv", "Logga in"),
            ("message_en", "Welcome"),
        ],
    );
    let text = status_of(&backend);
    let v = Reader::new(backend.clone())
        .fresh()
        .unwrap()
        .shown()
        .v()
        .to_string();
    assert_eq!(v.len(), 16);
    assert_eq!(
        text,
        format!(
            "brand: set\n\
             name: \"MOHV\" (record)\n\
             slug: \"mohv\" (record)\n\
             logo: tile (record)\n\
             tile_accent: \"#495670\" (default)\n\
             accent: \"#6d3fd0\" (default)\n\
             authenticator_issuer: \"MOHV\" (record)\n\
             favicon: \"tile\" (default)\n\
             site.accent: \"#495670\" (default)\n\
             site.logo_url: \"/api/brand/logo.svg?v={v}\" (record)\n\
             site.title: \"MOHV\" (record)\n\
             title_sv: \"Logga in\" (record)\n\
             message_en: \"Welcome\" (record)\n\
             logo version: {v}\n\
             fallbacks: none\n"
        )
    );
}

/// `status` names the authenticator issuer and the source that won; with the variable
/// `OBOP_MFA_ISSUER` set (given here as a value, `render_status_with`) it is the variable, also when
/// the variable says `obop` and the record says another name.
#[test]
fn status_names_the_variable_as_the_source_of_the_authenticator_issuer_when_it_wins() {
    let (_store, backend) = memory();
    configure(&backend, &[("name", "MOHV")]);
    let fresh = Reader::new(backend.clone()).fresh().unwrap();
    for (variable, line) in [
        (None, "authenticator_issuer: \"MOHV\" (record)\n"),
        (Some("Acme"), "authenticator_issuer: \"Acme\" (env)\n"),
        (Some("obop"), "authenticator_issuer: \"obop\" (env)\n"),
    ] {
        let text = render_status_with(&fresh, variable.map(String::from));
        assert!(text.contains(line), "{variable:?}: {text}");
        assert!(
            text.contains("name: \"MOHV\" (record)\n"),
            "the name is the record's whatever the variable says: {text}"
        );
    }
}

#[test]
fn status_names_a_stored_field_that_fell_back_and_prints_nothing_of_its_value() {
    let (_store, backend) = memory();
    let mut row = Fields::new();
    for (k, v) in [
        ("name", "Acme"),
        // A control character, a bidirectional override and markup: none may reach the terminal.
        ("title_de", "\u{1b}[31mFORGED\u{1b}[0m"),
        ("title_fr", "a\u{202e}b"),
        ("message_pl", "<script>x</script>"),
        ("slug", "Foo/Bar"),
    ] {
        row.insert(k.into(), json!(v));
    }
    backend
        .apply_checked(&[Op::Put {
            model: "instance_brand".into(),
            id: "instance".into(),
            fields: row,
        }])
        .unwrap();
    let text = status_of(&backend);
    assert!(text.contains("brand: set\n"), "{text}");
    assert!(text.contains("name: \"Acme\" (record)\n"), "{text}");
    for field in ["slug", "title_de", "title_fr", "message_pl"] {
        assert!(
            text.contains(&format!(
                "fallback: {field} is not used, the default is shown ("
            )),
            "{field}: {text}"
        );
    }
    assert!(!text.contains("fallbacks: none"), "{text}");
    for forbidden in ["FORGED", "script", "Foo/Bar", "\u{1b}", "\u{202e}"] {
        assert!(!text.contains(forbidden), "{forbidden:?} in {text:?}");
    }
    // Only the stored fields that passed are listed under their own name.
    assert!(!text.contains("title_de: "), "{text}");
    assert!(!text.contains("message_pl: "), "{text}");
}

/// A title or a message of nothing but spaces, written around the command (`configure` refuses it),
/// is as empty as a blank name: `status` lists it as a fallback and not as a value in force.
#[test]
fn status_lists_a_blank_title_and_message_as_fallbacks_and_not_as_values_in_force() {
    let (_store, backend) = memory();
    let mut row = Fields::new();
    for (k, v) in [("name", "MOHV"), ("title_en", "   "), ("message_en", "  ")] {
        row.insert(k.into(), json!(v));
    }
    backend
        .apply_checked(&[Op::Put {
            model: "instance_brand".into(),
            id: "instance".into(),
            fields: row,
        }])
        .unwrap();
    let text = status_of(&backend);
    assert!(text.contains("name: \"MOHV\" (record)\n"), "{text}");
    for field in ["title_en", "message_en"] {
        assert!(
            text.contains(&format!(
                "fallback: {field} is not used, the default is shown (empty)\n"
            )),
            "{field}: {text}"
        );
        assert!(!text.contains(&format!("{field}: \"")), "{field}: {text}");
    }
    assert!(!text.contains("fallbacks: none"), "{text}");
}

// ---- a store that fails ---------------------------------------------------------------------------------
//
// The branches of the command line that a healthy store never takes: a store that cannot be read is
// an error and never "no brand" (spec, section 3, "What `get` shows"), and a write that the store
// refuses is the line and the status `obop call` gives, with nothing stored.

const BUSY: &str = "storage_busy: all database connections are in use, try again";

fn flaky_with(fields: &[(&str, &str)]) -> (std::sync::Arc<Flaky>, std::sync::Arc<dyn Backend>) {
    let (flaky, backend) = Flaky::memory();
    if !fields.is_empty() {
        configure(&backend, fields);
    }
    (flaky, backend)
}

fn stored_name(backend: &std::sync::Arc<dyn Backend>) -> Option<String> {
    backend
        .get("instance_brand", "instance")
        .unwrap()
        .and_then(|row| row.get("name").and_then(|v| v.as_str()).map(String::from))
}

#[test]
fn a_refusal_of_the_store_is_the_line_and_status_75_of_obop_call_and_any_other_text_is_status_1() {
    assert_eq!(
        storage_said(BUSY),
        Said {
            out: String::new(),
            err: format!("error: {BUSY} (nothing was changed; try again in 1 s)\n"),
            code: 75,
        }
    );
    assert_eq!(
        storage_said("disk full"),
        Said {
            out: String::new(),
            err: "error: disk full\n".into(),
            code: 1,
        }
    );
    // The code word counts at the start only: text that merely holds one is not a fault, and no
    // caller is made to retry its own bad input.
    let said = storage_said(&format!("could not save: {BUSY}"));
    assert_eq!(said.code, 1, "{said:?}");
    assert_eq!(said.err, format!("error: could not save: {BUSY}\n"));
}

#[test]
fn status_of_a_store_that_cannot_be_read_is_an_error_and_never_no_brand() {
    for fields in [&[][..], &[("name", "MOHV")][..]] {
        let (flaky, backend) = flaky_with(fields);
        for mode in [Mode::Reads, Mode::All] {
            flaky.set(mode);
            let said = status_over(backend.clone());
            assert_eq!(said.code, 75, "{mode:?}: {said:?}");
            assert_eq!(said.out, "", "no text at all: {said:?}");
            assert!(said.err.starts_with("error: storage_busy: "), "{said:?}");
            assert!(
                !said.err.contains("not set") && !said.err.contains("brand:"),
                "{said:?}"
            );
        }
        // Either of the two reads (the row, then the theme setting) failing fails the whole answer.
        flaky.set(Mode::Healthy);
        flaky.refuse_call_after(1);
        let said = status_over(backend.clone());
        assert_eq!(said.code, 75, "the second read refused: {said:?}");
        assert_eq!(said.out, "", "{said:?}");
        // And once the store answers again, so does the command.
        let said = status_over(backend.clone());
        assert_eq!(said.code, 0, "{said:?}");
        assert!(said.out.starts_with("brand: "), "{said:?}");
        assert_eq!(said.err, "");
    }
}

#[test]
fn a_write_the_store_refuses_is_status_75_and_stores_nothing() {
    let (flaky, backend) = flaky_with(&[]);
    flaky.refuse_applies(1);
    // A dry run writes no batch, so it passes by the refusal that is waiting for the real call.
    let dry = configure_over(&*backend, pairs(&[("name", "MOHV")]), true);
    assert_eq!(dry.code, 0, "{dry:?}");
    assert!(dry.out.contains("dry run"), "{dry:?}");
    let said = configure_over(&*backend, pairs(&[("name", "MOHV")]), false);
    assert_eq!(said.code, 75, "{said:?}");
    assert_eq!(said.out, "", "{said:?}");
    assert!(
        said.err.starts_with("error: storage_busy: ")
            && said
                .err
                .ends_with("(nothing was changed; try again in 1 s)\n"),
        "{said:?}"
    );
    assert_eq!(stored_name(&backend), None);
    assert!(backend.list("security_event").unwrap().is_empty());
    // The refusal is spent: the same call now writes, and says so.
    let said = configure_over(&*backend, pairs(&[("name", "MOHV")]), false);
    assert_eq!(said.code, 0, "{said:?}");
    assert!(
        said.out.contains("recorded in the security log"),
        "{said:?}"
    );
    assert_eq!(stored_name(&backend).as_deref(), Some("MOHV"));

    // `clear` the same way: the row stays, with its events.
    let events = backend.list("security_event").unwrap().len();
    flaky.refuse_applies(1);
    let said = clear_over(&*backend, false);
    assert_eq!(said.code, 75, "{said:?}");
    assert_eq!(said.out, "", "{said:?}");
    assert!(said.err.starts_with("error: storage_busy: "), "{said:?}");
    assert_eq!(stored_name(&backend).as_deref(), Some("MOHV"));
    assert_eq!(backend.list("security_event").unwrap().len(), events);
    let said = clear_over(&*backend, false);
    assert_eq!(said.code, 0, "{said:?}");
    assert_eq!(stored_name(&backend), None);
}

#[test]
fn a_store_that_cannot_be_read_while_a_call_is_planned_is_status_75_too() {
    let (flaky, backend) = flaky_with(&[("name", "MOHV")]);
    flaky.set(Mode::Reads);
    for said in [
        configure_over(&*backend, pairs(&[("name", "Other")]), false),
        configure_over(&*backend, pairs(&[("name", "Other")]), true),
        clear_over(&*backend, false),
        clear_over(&*backend, true),
    ] {
        assert_eq!(said.code, 75, "{said:?}");
        assert_eq!(said.out, "", "{said:?}");
        assert!(said.err.starts_with("error: storage_busy: "), "{said:?}");
    }
    flaky.set(Mode::Healthy);
    assert_eq!(stored_name(&backend).as_deref(), Some("MOHV"));
}

#[test]
fn what_a_refused_call_and_the_dry_run_of_one_say_and_the_status_they_end_with() {
    let (_flaky, backend) = flaky_with(&[]);
    // No name on the first call: the refusal, one line per rule, and nothing stored.
    let said = configure_over(&*backend, pairs(&[("slug", "mohv")]), false);
    assert_eq!(
        said,
        Said {
            out: String::new(),
            err: "error: name: is required: a brand needs a name\nnothing was stored\n".into(),
            code: 1,
        }
    );
    // The dry run of the same call: the same reason, and what a real call would be.
    let dry = configure_over(&*backend, pairs(&[("slug", "mohv")]), true);
    assert_eq!(
        dry,
        Said {
            out: String::new(),
            err: "error: name: is required: a brand needs a name\n\
                  dry run: a real call would be refused; nothing was written\n"
                .into(),
            code: 1,
        }
    );
    assert_eq!(stored_name(&backend), None);
}

#[test]
fn a_dry_run_of_clear_says_what_a_real_one_would_and_not_that_it_was_logged() {
    let (_flaky, backend) = flaky_with(&[("name", "MOHV")]);
    let dry = clear_over(&*backend, true);
    assert_eq!(dry.code, 0, "{dry:?}");
    assert!(
        dry.out
            .starts_with("dry run, nothing was written; a real call would remove the brand:\n"),
        "{dry:?}"
    );
    assert!(!dry.out.contains("recorded in the security log"), "{dry:?}");
    assert_eq!(stored_name(&backend).as_deref(), Some("MOHV"));
    let real = clear_over(&*backend, false);
    assert!(real.out.starts_with("brand removed:\n"), "{real:?}");
    assert!(
        real.out.ends_with("recorded in the security log\n"),
        "{real:?}"
    );
    assert_eq!(
        clear_over(&*backend, false).out,
        "no brand is set; nothing to remove\n"
    );
}

// ---- status and the answer of get -------------------------------------------------------------------------

#[test]
fn every_entry_of_the_effective_answer_has_a_line_in_status_or_is_a_language_with_no_text_of_its_own()
 {
    let hostile = {
        let (_store, backend) = memory();
        let mut row = Fields::new();
        for (k, v) in [
            ("name", "Acme"),
            ("title_de", "a<b"),
            ("title_sv", "Logga in"),
        ] {
            row.insert(k.into(), json!(v));
        }
        backend
            .apply_checked(&[Op::Put {
                model: "instance_brand".into(),
                id: "instance".into(),
                fields: row,
            }])
            .unwrap();
        backend
    };
    let states: Vec<std::sync::Arc<dyn Backend>> = vec![
        memory().1,
        {
            let (_store, backend) = memory();
            configure(&backend, &[("name", "MOHV")]);
            backend
        },
        {
            let (_store, backend) = memory();
            configure(
                &backend,
                &[
                    ("name", "MOHV"),
                    ("title_en", "Sign in"),
                    ("title_sv", "Logga in"),
                    ("message_fi", "Tervetuloa"),
                ],
            );
            backend
        },
        hostile,
    ];
    for backend in states {
        let fresh = Reader::new(backend).fresh().unwrap();
        let text = render_status(&fresh);
        let effective: serde_json::Value = serde_json::from_str(&fresh.effective_json()).unwrap();
        for key in effective.as_object().unwrap().keys() {
            let printed = text.lines().any(|l| l.starts_with(&format!("{key}: ")));
            if key.starts_with("title_") || key.starts_with("message_") {
                // A language shows a line when its own text is stored and passed the read check.
                let fell_back = fresh.shown().fallbacks().iter().any(|f| f.field == *key);
                assert_eq!(
                    printed,
                    !fresh.stored_text(key).is_empty() && !fell_back,
                    "{key}: {text}"
                );
            } else {
                assert!(printed, "{key} has no line in\n{text}");
            }
        }
    }
}

/// The command line acts as the operator: every event it writes names `operator` as the actor and
/// `(instance)` as the user, for a change and for a removal alike. (The integration test of the
/// binary reads the same, but a run of this file alone must hold it too.)
#[test]
fn the_events_of_the_command_line_are_written_by_the_operator() {
    let backend = Store::new().backend();
    let said = configure_over(
        &*backend,
        pairs(&[("name", "MOHV"), ("slug", "mohv")]),
        false,
    );
    assert_eq!(said.code, 0, "{said:?}");
    let said = clear_over(&*backend, false);
    assert_eq!(said.code, 0, "{said:?}");
    let mut events: Vec<(String, String, String, String)> = backend
        .list("security_event")
        .unwrap()
        .into_iter()
        .map(|(_, f)| {
            let g = |k: &str| f[k].as_str().unwrap().to_string();
            (g("kind"), g("actor"), g("user"), g("detail"))
        })
        .collect();
    events.sort();
    assert_eq!(events.len(), 4, "{events:?}");
    for (kind, actor, user, detail) in &events {
        assert!(
            kind == "brand_configured" || kind == "brand_cleared",
            "{kind}"
        );
        assert_eq!(actor, "operator", "{detail}");
        assert_eq!(user, "(instance)", "{detail}");
    }
}

#[test]
fn the_switch_flag_takes_on_or_off_once_and_nothing_else() {
    for (word, want) in [("on", true), ("off", false)] {
        let (flags, rest) = take_flags(vec![
            "admin",
            "brand",
            "configure",
            "--hide-product-docs",
            word,
        ])
        .unwrap();
        assert_eq!(flags.unwrap().hide_product_docs, Some(want), "{word}");
        assert_eq!(rest, ["admin", "brand", "configure"]);
    }
    for (args, why) in [
        (
            vec!["admin", "brand", "configure", "--hide-product-docs"],
            "no value",
        ),
        (
            vec!["admin", "brand", "configure", "--hide-product-docs", "yes"],
            "yes",
        ),
        (
            vec!["admin", "brand", "configure", "--hide-product-docs", "true"],
            "true",
        ),
        (
            vec!["admin", "brand", "configure", "--hide-product-docs", "ON"],
            "ON",
        ),
        (
            vec![
                "admin",
                "brand",
                "configure",
                "--hide-product-docs",
                "on",
                "--hide-product-docs",
                "off",
            ],
            "given twice",
        ),
    ] {
        let e = take_flags(args).unwrap_err();
        assert!(e.contains("--hide-product-docs"), "{why}: {e}");
    }
    // Another command line is not touched by the flag.
    let (flags, rest) =
        take_flags(vec!["status", "incident", "--hide-product-docs", "on"]).unwrap();
    assert!(flags.is_none());
    assert_eq!(rest, ["status", "incident", "--hide-product-docs", "on"]);
}

#[test]
fn the_switch_is_set_and_shown_through_the_command_line() {
    let backend = Store::new().backend();
    let said = configure_over(
        &*backend,
        pairs(&[("name", "MOHV"), ("hide_product_docs", "true")]),
        false,
    );
    assert_eq!(said.code, 0, "{said:?}");
    assert!(
        said.out.contains("hide_product_docs: false -> true"),
        "{}",
        said.out
    );
    let status = status_over(backend.clone());
    assert!(
        status.out.contains("hide_product_docs: on\n"),
        "{}",
        status.out
    );
    let said = configure_over(&*backend, pairs(&[("hide_product_docs", "false")]), false);
    assert!(
        said.out.contains("hide_product_docs: true -> false"),
        "{}",
        said.out
    );
    let status = status_over(backend.clone());
    assert!(
        !status.out.contains("hide_product_docs"),
        "off is the default and prints nothing: {}",
        status.out
    );
    // A bad value is refused by the field's name and nothing is stored.
    let said = configure_over(&*backend, pairs(&[("hide_product_docs", "maybe")]), false);
    assert_eq!(said.code, 1);
    assert!(
        said.err
            .contains("hide_product_docs: must be true or false"),
        "{}",
        said.err
    );
    // With no brand, status says nothing of the switch.
    let empty = Store::new().backend();
    assert!(!status_over(empty).out.contains("hide_product_docs"));
}
