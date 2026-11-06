//! The argument handling and the `status` text of `obop admin brand` (docs/spec/branding.md,
//! section 13, "Merge": "`status incident create --title T` still works, because the repeatable
//! flags of `admin brand` are taken before the global ones"). The commands through the real binary,
//! the `--from` cases and the local smoke are in `tests/brand_cli.rs`.

use crate::brand_cmd::{Flags, render_status, take_flags};
use obop_commands::Store;
use obop_commands::brand::{Action, Reader, apply};
use obop_storage::{Backend, Fields, Op};
use serde_json::json;

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
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
        vec!["--title", "x", "admin", "brand", "status"],
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

fn status_of(backend: &std::sync::Arc<dyn Backend>) -> String {
    render_status(&Reader::new(backend.clone()).fresh().unwrap())
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
         fallbacks: none\n"
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
             title_sv: \"Logga in\" (record)\n\
             message_en: \"Welcome\" (record)\n\
             logo version: {v}\n\
             fallbacks: none\n"
        )
    );
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
