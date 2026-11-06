//! `brand::apply`: merge, validation, idempotence, `dry_run` (docs/spec/branding.md, section 3 and
//! section 13, "Merge" and "Commands").

use super::*;
use crate::brand::{Action, Applied, Failure, apply, changes_json, refusals_json};
use obop_brand::{logo, tile};

fn cfg(pairs: &[(&str, &str)]) -> Action {
    Action::Configure(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    )
}

fn run(spy: &Spy, action: &Action) -> Result<Applied, Failure> {
    apply(spy, None, "ada", action, false)
}

fn ok(spy: &Spy, pairs: &[(&str, &str)]) -> Applied {
    run(spy, &cfg(pairs)).unwrap_or_else(|e| panic!("refused: {e}"))
}

fn refusals(r: Result<Applied, Failure>) -> Vec<(String, String)> {
    match r {
        Err(Failure::Refused(list)) => list.into_iter().map(|r| (r.field, r.rule)).collect(),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn stored(spy: &Spy) -> Option<Fields> {
    spy.get(MODEL, ROW_ID).unwrap()
}

fn text(f: &Fields, k: &str) -> String {
    f.get(k).and_then(J::as_str).unwrap_or("").to_string()
}

fn events(spy: &Spy) -> usize {
    spy.list("security_event").unwrap().len()
}

#[test]
fn the_first_call_needs_a_name() {
    let spy = Spy::memory();
    let r = refusals(run(&spy, &cfg(&[("title_en", "Hello")])));
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].0, "name");
    assert!(r[0].1.contains("required"), "{}", r[0].1);
    assert!(
        stored(&spy).is_none() && events(&spy) == 0,
        "nothing is stored"
    );
    // An empty call has no name either.
    assert_eq!(refusals(run(&spy, &cfg(&[])))[0].0, "name");
}

#[test]
fn the_first_call_with_a_name_stores_the_row_and_answers_the_version_of_the_logo() {
    let spy = Spy::memory();
    put_theme(&*spy, "#c2410c");
    let a = ok(&spy, &[("name", "MOHV")]);
    assert_eq!(a.changes.len(), 1);
    assert_eq!(
        (a.changes[0].field.as_str(), a.changes[0].old.as_str()),
        ("name", "")
    );
    assert_eq!(a.changes[0].new, "MOHV");
    assert!(!a.dry_run && a.refusals.is_empty() && !a.cleared);
    // The tile follows the theme accent, and v is the hash of what the route will serve.
    let tile = tile::svg("MOHV", Some("#c2410c"), None);
    assert_eq!(a.v, logo::version(tile.as_bytes()));
    let row = stored(&spy).unwrap();
    assert_eq!(text(&row, "name"), "MOHV");
    assert_eq!(
        row.len(),
        1,
        "only the fields that hold a value are stored: {row:?}"
    );
}

#[test]
fn the_one_flag_logo_swap_keeps_the_name_titles_and_messages() {
    let spy = Spy::memory();
    ok(
        &spy,
        &[
            ("name", "MOHV"),
            ("title_sv", "Logga in"),
            ("message_en", "Welcome"),
            ("slug", "mohv"),
        ],
    );
    let a = ok(&spy, &[("logo_svg", M_ARTWORK)]);
    assert_eq!(a.changes.len(), 1);
    assert_eq!(a.changes[0].field, "logo_svg");
    assert_eq!(a.changes[0].old, "");
    assert_eq!(a.changes[0].new, logo::version(M_ARTWORK.as_bytes()));
    assert_eq!(a.v, logo::version(M_ARTWORK.as_bytes()));
    let row = stored(&spy).unwrap();
    assert_eq!(text(&row, "name"), "MOHV");
    assert_eq!(text(&row, "title_sv"), "Logga in");
    assert_eq!(text(&row, "message_en"), "Welcome");
    assert_eq!(text(&row, "slug"), "mohv");
    assert_eq!(text(&row, "logo_svg"), M_ARTWORK);
    // The clear flag returns to the tile and touches nothing else.
    let a = ok(&spy, &[("logo_svg", "")]);
    assert_eq!(a.changes.len(), 1);
    assert_eq!(a.changes[0].old, logo::version(M_ARTWORK.as_bytes()));
    assert_eq!(a.changes[0].new, "");
    let row = stored(&spy).unwrap();
    assert!(!row.contains_key("logo_svg"));
    assert_eq!(text(&row, "name"), "MOHV");
    assert_eq!(text(&row, "title_sv"), "Logga in");
}

#[test]
fn an_empty_text_clears_only_that_field() {
    let spy = Spy::memory();
    ok(
        &spy,
        &[
            ("name", "MOHV"),
            ("title_sv", "Logga in"),
            ("title_de", "Anmelden"),
        ],
    );
    let a = ok(&spy, &[("title_sv", "")]);
    assert_eq!(a.changes.len(), 1);
    let row = stored(&spy).unwrap();
    assert!(!row.contains_key("title_sv"));
    assert_eq!(text(&row, "title_de"), "Anmelden");
    // A slug is cleared the same way.
    ok(&spy, &[("slug", "mohv")]);
    ok(&spy, &[("slug", "")]);
    assert!(!stored(&spy).unwrap().contains_key("slug"));
}

#[test]
fn a_name_cannot_be_cleared_only_replaced() {
    let spy = Spy::memory();
    ok(&spy, &[("name", "MOHV")]);
    let r = refusals(run(&spy, &cfg(&[("name", "")])));
    assert_eq!(r[0].0, "name");
    assert_eq!(text(&stored(&spy).unwrap(), "name"), "MOHV");
    ok(&spy, &[("name", "MOHV Two")]);
    assert_eq!(text(&stored(&spy).unwrap(), "name"), "MOHV Two");
}

#[test]
fn a_second_identical_call_changes_nothing_and_writes_no_event() {
    let spy = Spy::memory();
    let call = [
        ("name", "MOHV"),
        ("title_sv", "Logga in"),
        ("logo_svg", M_ARTWORK),
    ];
    ok(&spy, &call);
    let (batches, logged, row) = (spy.batches().len(), events(&spy), stored(&spy));
    let a = ok(&spy, &call);
    assert!(a.changes.is_empty(), "{:?}", a.changes);
    assert_eq!(
        a.v,
        logo::version(M_ARTWORK.as_bytes()),
        "v is still answered"
    );
    assert_eq!(spy.batches().len(), batches, "no batch at all");
    assert_eq!(events(&spy), logged);
    assert_eq!(stored(&spy), row);
}

#[test]
fn a_refused_field_stores_nothing_of_the_call() {
    let spy = Spy::memory();
    ok(&spy, &[("name", "MOHV")]);
    let (row, logged) = (stored(&spy), events(&spy));
    let r = refusals(run(
        &spy,
        &cfg(&[
            ("name", "Zed"),
            ("title_en", "Fine"),
            ("message_en", "visit shop.example"),
        ]),
    ));
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].0, "message_en");
    assert_eq!(stored(&spy), row, "not even the valid fields");
    assert_eq!(events(&spy), logged);
}

#[test]
fn every_refusal_is_in_the_message_one_per_line_as_field_and_rule() {
    let spy = Spy::memory();
    let e = run(
        &spy,
        &cfg(&[
            ("name", "a:b"),
            ("title_sv", "<b>"),
            ("message_fi", "call 1234567"),
        ]),
    )
    .unwrap_err();
    let msg = e.to_string();
    let lines: Vec<&str> = msg.lines().collect();
    assert_eq!(lines.len(), 3, "{msg}");
    assert!(lines[0].starts_with("name: "), "{msg}");
    assert!(lines[1].starts_with("title_sv: "), "{msg}");
    assert!(lines[2].starts_with("message_fi: "), "{msg}");
    // The refusals repeat no raw text of the input, and say the rule that was broken.
    assert!(!msg.contains("1234567"), "{msg}");
    assert_eq!(
        lines[2],
        "message_fi: must not hold 7 or more digits in a row: it looks like a phone number"
    );
}

const LONGER_THAN_40: &str =
    "is longer than 40 characters (Unicode code points, after normalisation)";
const LONGER_THAN_60: &str =
    "is longer than 60 characters (Unicode code points, after normalisation)";
const LONGER_THAN_240: &str =
    "is longer than 240 characters (Unicode code points, after normalisation)";

#[test]
fn each_field_is_checked_by_its_own_policy_class_and_its_own_limit() {
    // name 40, title 60, message 240, counted in code points: the limit of a class is not the
    // limit of another, so a field checked as the wrong class fails one of these.
    let spy = Spy::memory();
    let cases: [(&str, usize, &str); 3] = [
        ("name", 40, LONGER_THAN_40),
        ("title_sv", 60, LONGER_THAN_60),
        ("message_fi", 240, LONGER_THAN_240),
    ];
    ok(&spy, &[("name", "MOHV")]);
    for (field, max, rule) in cases {
        for letter in ["a", "\u{e9}"] {
            let at_the_limit = letter.repeat(max);
            ok(&spy, &[(field, at_the_limit.as_str())]);
            assert_eq!(text(&stored(&spy).unwrap(), field), at_the_limit, "{field}");
            let over = letter.repeat(max + 1);
            let r = refusals(run(&spy, &cfg(&[(field, over.as_str())])));
            assert_eq!(r, [(field.to_string(), rule.to_string())], "{field}");
            assert_eq!(
                text(&stored(&spy).unwrap(), field),
                at_the_limit,
                "{field}: nothing stored"
            );
        }
    }
}

#[test]
fn a_name_and_a_title_may_not_hold_the_punctuation_that_a_message_may() {
    let spy = Spy::memory();
    ok(&spy, &[("name", "MOHV")]);
    for text_ in ["Foo?", "Foo; bar", "A $ B"] {
        for field in ["name", "title_en"] {
            let r = refusals(run(&spy, &cfg(&[(field, text_)])));
            assert_eq!(r[0].0, field, "{text_:?} as {field}");
        }
    }
    ok(
        &spy,
        &[("message_en", "Is this you? Yes; sign in for 5 $ a month")],
    );
    assert_eq!(
        text(&stored(&spy).unwrap(), "message_en"),
        "Is this you? Yes; sign in for 5 $ a month"
    );
}

#[test]
fn a_slug_is_checked_against_its_pattern_when_it_is_written() {
    let spy = Spy::memory();
    ok(&spy, &[("name", "MOHV")]);
    let thirty_one = "a".repeat(31);
    for bad in [
        "Foo/Bar",
        "9x",
        "a-",
        "UPPER",
        "-a",
        "a_b",
        "a b",
        &thirty_one,
    ] {
        let r = refusals(run(&spy, &cfg(&[("slug", bad)])));
        assert_eq!(r.len(), 1, "{bad:?}");
        assert_eq!(r[0].0, "slug", "{bad:?}");
        assert!(!r[0].1.is_empty());
        assert!(!stored(&spy).unwrap().contains_key("slug"), "{bad:?}");
    }
    let thirty = "a".repeat(30);
    for good in ["mohv", "a-b-9", "x", &thirty] {
        ok(&spy, &[("slug", good)]);
        assert_eq!(text(&stored(&spy).unwrap(), "slug"), good);
    }
}

#[test]
fn a_later_pair_with_the_same_name_wins() {
    let spy = Spy::memory();
    ok(&spy, &[("name", "First"), ("name", "Second")]);
    assert_eq!(text(&stored(&spy).unwrap(), "name"), "Second");
    // The earlier value is not even checked: only what wins is stored and policed.
    ok(&spy, &[("title_en", "a<b"), ("title_en", "Fine")]);
    assert_eq!(text(&stored(&spy).unwrap(), "title_en"), "Fine");
}

#[test]
fn a_stored_row_without_a_name_still_needs_one_in_the_call() {
    let spy = Spy::memory();
    put_row(&*spy, &[("title_en", s("Hello"))]);
    let r = refusals(run(&spy, &cfg(&[("title_sv", "Hej")])));
    assert_eq!(
        r,
        [(
            "name".to_string(),
            "is required: a brand needs a name".to_string()
        )]
    );
    // An empty name is refused for being empty, and is not refused twice.
    let r = refusals(run(&spy, &cfg(&[("name", "")])));
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].1, "must not be empty");
    // With a name the stored title is carried along unchecked.
    ok(&spy, &[("name", "MOHV")]);
    assert_eq!(text(&stored(&spy).unwrap(), "title_en"), "Hello");
}

#[test]
fn a_valid_artwork_stored_in_another_form_is_the_same_logo_and_sending_it_again_writes_nothing() {
    // The row holds a valid file that is not in canonical form (an older writer, a restore). It is
    // served re-serialised, so it is the same logo as its canonical form: nothing changes.
    let spy = Spy::memory();
    let given = "<svg  xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0  64 64\" ><path d=\"M .5 .5 L 10.000000 10 Z\" fill=\"#FFFFFF\" /></svg>";
    let canonical = obop_brand::svg::canonical(given.as_bytes()).unwrap();
    assert_ne!(canonical, given);
    put_row(&*spy, &[("name", s("MOHV")), ("logo_svg", s(given))]);
    let (batches, logged) = (spy.batches().len(), events(&spy));
    let a = ok(&spy, &[("logo_svg", given)]);
    assert!(a.changes.is_empty(), "{:?}", a.changes);
    assert_eq!(a.v, logo::version(canonical.as_bytes()));
    assert_eq!(spy.batches().len(), batches, "no batch");
    assert_eq!(events(&spy), logged, "no event");
    assert_eq!(
        text(&stored(&spy).unwrap(), "logo_svg"),
        given,
        "the row is left as it was"
    );
    // Another field changing in the same call: one event, for that field; the artwork is stored in
    // canonical form with it.
    let a = ok(&spy, &[("name", "MOHV Two"), ("logo_svg", given)]);
    assert_eq!(a.changes.len(), 1);
    assert_eq!(a.changes[0].field, "name");
    assert_eq!(events(&spy), logged + 1);
    assert_eq!(text(&stored(&spy).unwrap(), "logo_svg"), canonical);
}

#[test]
fn dry_run_writes_nothing_logs_nothing_and_reports_the_same_refusals() {
    let spy = Spy::memory();
    let bad = cfg(&[("name", "a:b"), ("message_en", "see shop.example")]);
    let real = refusals(run(&spy, &bad));
    let dry = apply(&*spy, None, "ada", &bad, true).unwrap();
    assert!(dry.dry_run);
    assert!(dry.changes.is_empty());
    let dry_list: Vec<(String, String)> = dry
        .refusals
        .iter()
        .map(|r| (r.field.clone(), r.rule.clone()))
        .collect();
    assert_eq!(dry_list, real, "the same refusals, typed");
    // A dry run with a good call says what would change and writes nothing.
    let good = cfg(&[("name", "MOHV"), ("logo_svg", M_ARTWORK)]);
    let dry = apply(&*spy, None, "ada", &good, true).unwrap();
    assert!(dry.refusals.is_empty());
    assert_eq!(dry.changes.len(), 2);
    assert_eq!(dry.v, logo::version(M_ARTWORK.as_bytes()));
    assert!(stored(&spy).is_none());
    assert_eq!(events(&spy), 0);
    assert!(spy.batches().is_empty());
    // A dry run on a missing name says so too.
    let dry = apply(&*spy, None, "ada", &cfg(&[("title_en", "x")]), true).unwrap();
    assert_eq!(dry.refusals[0].field, "name");
    // And the JSON of the answer carries the fields of the spec.
    let j: J = serde_json::from_str(&refusals_json(&dry.refusals)).unwrap();
    assert_eq!(j[0]["field"], "name");
    assert!(j[0]["rule"].is_string());
}

#[test]
fn unknown_keys_are_refused_by_name_and_the_name_is_made_safe_to_repeat() {
    let spy = Spy::memory();
    for key in [
        "v",
        "id",
        "set",
        "changes",
        "dry_run",
        "hide_product_docs",
        "frob",
    ] {
        let r = refusals(run(&spy, &cfg(&[("name", "MOHV"), (key, "x")])));
        assert_eq!(r.len(), 1, "{key}");
        assert_eq!(r[0].0, key);
        assert!(r[0].1.contains("not a field"), "{}", r[0].1);
        assert!(stored(&spy).is_none());
    }
    let r = refusals(run(&spy, &cfg(&[("name", "MOHV"), ("a\nb\u{202e}c", "x")])));
    assert_eq!(
        r[0].0, "a?b?c",
        "no control or bidirectional character is repeated"
    );
    let long = "k".repeat(100);
    let r = refusals(run(&spy, &cfg(&[("name", "MOHV"), (long.as_str(), "x")])));
    assert!(r[0].0.len() <= 43, "{}", r[0].0);
}

/// `show_key` decides what of a caller's key a refusal may repeat, on every surface (a REST body,
/// MCP tool text, a terminal, the log). It is an allow-list: the only characters repeated are the
/// ASCII letters, digits, `_` and `-`. These are the cases a deny-list would miss, one by one, and
/// then every Unicode scalar value.
#[test]
fn show_key_repeats_only_ascii_letters_digits_underscore_and_hyphen() {
    use crate::brand::show_key;
    // The classes by name: bidirectional controls and isolates, zero-width and joiner characters,
    // line and paragraph separators, the byte order mark, tag characters, variation selectors,
    // fillers, no-break and soft-hyphen characters, other spaces, C0 and C1 controls, and the
    // ASCII characters that mean something in markup, a path, a shell or a log line.
    let mut hostile: Vec<char> = Vec::new();
    hostile.extend('\u{202a}'..='\u{202e}');
    hostile.extend('\u{2066}'..='\u{2069}');
    hostile.extend('\u{200b}'..='\u{200f}');
    hostile.extend(['\u{2028}', '\u{2029}', '\u{0085}', '\u{feff}']);
    hostile.extend('\u{2060}'..='\u{2064}');
    hostile.extend('\u{206a}'..='\u{206f}');
    hostile.extend(['\u{e0001}', '\u{e0020}', '\u{e0041}', '\u{e007f}']);
    hostile.extend('\u{fe00}'..='\u{fe0f}');
    hostile.extend(['\u{e0100}', '\u{e01ef}']);
    hostile.extend(['\u{3164}', '\u{ffa0}', '\u{115f}', '\u{1160}', '\u{034f}']);
    hostile.extend(['\u{00a0}', '\u{00ad}', '\u{180e}', '\u{2000}', '\u{3000}']);
    hostile.extend('\u{0000}'..='\u{001f}');
    hostile.extend('\u{007f}'..='\u{009f}');
    hostile.extend("<>\"'&/\\;:. ,()%`$|*?[]{}!@#=+~^".chars());
    // Letters and digits of other scripts, and look-alikes of ASCII ones.
    hostile.extend([
        '\u{e9}', '\u{df}', '\u{436}', '\u{5b57}', '\u{663}', '\u{ff21}', '\u{ff5a}', '\u{ff10}',
    ]);
    for c in hostile {
        assert_eq!(
            show_key(&format!("a{c}b")),
            "a?b",
            "U+{:04X} is repeated",
            c as u32
        );
    }
    // The characters that are repeated.
    assert_eq!(show_key("Aa_Zz-09"), "Aa_Zz-09");
    // Every scalar value, alone: it is repeated only when it is one of the allowed ASCII ones.
    for n in 0..=0x10_ffff_u32 {
        let Some(c) = char::from_u32(n) else {
            continue;
        };
        let allowed = c.is_ascii_alphanumeric() || c == '_' || c == '-';
        assert_eq!(
            show_key(&c.to_string()),
            if allowed { c.to_string() } else { "?".into() },
            "U+{n:04X}"
        );
    }
    // The cut: 40 characters, then three dots, and nothing outside the allowed set but the dots.
    let mixed: String = (0..300u32)
        .map(|n| char::from_u32(0x20 + n * 7).unwrap_or('x'))
        .collect();
    for key in [mixed.as_str(), &"k".repeat(41), &"\u{e9}".repeat(500)] {
        let shown = show_key(key);
        let (head, tail) = shown.split_at(40);
        assert_eq!(tail, "...", "{shown:?}");
        assert!(
            head.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '?'),
            "{shown:?}"
        );
    }
    // Exactly 40 characters is not cut, and nothing is nothing.
    assert_eq!(show_key(&"k".repeat(40)), "k".repeat(40));
    assert_eq!(show_key(""), "");
}

#[test]
fn a_field_this_binary_does_not_know_survives_a_merge() {
    // A newer binary stored a field (S2 adds hide_product_docs). An older binary that changes a
    // title must not drop it by replacing the whole row.
    let spy = Spy::memory();
    put_row(
        &*spy,
        &[
            ("name", s("MOHV")),
            ("hide_product_docs", J::Bool(true)),
            ("future_field", s("kept")),
        ],
    );
    ok(&spy, &[("title_sv", "Logga in")]);
    let row = stored(&spy).unwrap();
    assert_eq!(row.get("hide_product_docs"), Some(&J::Bool(true)));
    assert_eq!(text(&row, "future_field"), "kept");
    assert_eq!(text(&row, "title_sv"), "Logga in");
}

#[test]
fn a_stored_field_that_is_not_supplied_is_not_checked_again_but_a_supplied_one_is() {
    // A message with a link was written around the command (or by a release with a looser policy).
    // Changing the name does not refuse it; sending it again does.
    let spy = Spy::memory();
    put_row(
        &*spy,
        &[("name", s("MOHV")), ("message_en", s("visit shop.example"))],
    );
    ok(&spy, &[("name", "MOHV Two")]);
    assert_eq!(
        text(&stored(&spy).unwrap(), "message_en"),
        "visit shop.example"
    );
    let r = refusals(run(&spy, &cfg(&[("message_en", "visit shop.example")])));
    assert_eq!(r[0].0, "message_en");
}

#[test]
fn text_is_stored_in_nfc() {
    let spy = Spy::memory();
    ok(&spy, &[("name", "Cafe\u{301}")]);
    assert_eq!(text(&stored(&spy).unwrap(), "name"), "Caf\u{e9}");
    // The same value in the other form is no change.
    let a = ok(&spy, &[("name", "Caf\u{e9}")]);
    assert!(a.changes.is_empty());
}

#[test]
fn an_artwork_is_stored_in_canonical_form_never_as_given() {
    let spy = Spy::memory();
    let given = "<?xml version=\"1.0\"?>\n<svg  xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\">\n  <path d=\"M .5 .5 L 10.000000 10 L 5. 20 Z\" fill=\"#FFFFFF\"/>\n</svg>\n";
    let canonical = obop_brand::svg::canonical(given.as_bytes()).unwrap();
    assert_ne!(canonical, given);
    ok(&spy, &[("name", "MOHV"), ("logo_svg", given)]);
    assert_eq!(text(&stored(&spy).unwrap(), "logo_svg"), canonical);
    // The same file again is no change.
    assert!(ok(&spy, &[("logo_svg", given)]).changes.is_empty());
}

#[test]
fn an_artwork_the_reader_refuses_names_the_reason_and_stores_nothing() {
    let spy = Spy::memory();
    ok(&spy, &[("name", "MOHV")]);
    for (what, svg, named) in [
        (
            "a script",
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\"><script>x</script></svg>",
            "script",
        ),
        (
            "a link",
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\"><a href=\"https://x.example\"/></svg>",
            "<a>",
        ),
        ("not svg", "hello", ""),
    ] {
        let r = refusals(run(&spy, &cfg(&[("logo_svg", svg)])));
        assert!(!r.is_empty(), "{what}");
        assert!(
            r.iter()
                .all(|(f, rule)| f == "logo_svg" && !rule.is_empty()),
            "{what}"
        );
        assert!(
            r.iter().any(|(_, rule)| rule.contains(named)),
            "{what}: {r:?}"
        );
        assert!(!stored(&spy).unwrap().contains_key("logo_svg"));
    }
    // A file over 16 KiB is refused.
    let big = format!("{M_ARTWORK}{}", " ".repeat(17 * 1024));
    assert!(!refusals(run(&spy, &cfg(&[("logo_svg", big.as_str())]))).is_empty());
}

#[test]
fn clear_removes_the_row_and_answers_cleared() {
    let spy = Spy::memory();
    ok(&spy, &[("name", "MOHV"), ("title_en", "Hi")]);
    let a = run(&spy, &Action::Clear).unwrap();
    assert!(a.cleared && !a.dry_run);
    assert_eq!(a.v, "");
    assert!(stored(&spy).is_none());
    let (r, _) = reader(&spy, LONG);
    assert!(!r.current().is_set(), "today's look is back");
}

#[test]
fn clear_with_no_row_answers_cleared_false_and_writes_nothing() {
    let spy = Spy::memory();
    let a = run(&spy, &Action::Clear).unwrap();
    assert!(!a.cleared);
    assert!(a.changes.is_empty());
    assert!(spy.batches().is_empty());
    assert_eq!(events(&spy), 0);
}

#[test]
fn clear_with_dry_run_removes_nothing_and_says_what_it_would_do() {
    let spy = Spy::memory();
    ok(&spy, &[("name", "MOHV")]);
    let (row, logged) = (stored(&spy), events(&spy));
    let a = apply(&*spy, None, "ada", &Action::Clear, true).unwrap();
    assert!(a.cleared && a.dry_run, "would remove");
    assert_eq!(stored(&spy), row);
    assert_eq!(events(&spy), logged);
    let none = Spy::memory();
    let a = apply(&*none, None, "ada", &Action::Clear, true).unwrap();
    assert!(!a.cleared && a.dry_run);
}

#[test]
fn a_call_with_a_reader_updates_it_and_a_call_without_one_does_not() {
    let spy = Spy::memory();
    ok(&spy, &[("name", "One")]);
    let (r, _) = reader(&spy, LONG);
    assert_eq!(r.current().brand().name(), "One");
    // The command line has no dispatcher: its change reaches a running server within the bound.
    ok(&spy, &[("name", "Two")]);
    assert_eq!(r.current().brand().name(), "One");
    r.expire();
    assert_eq!(r.current().brand().name(), "Two");
    // The dispatcher passes its reader: at once.
    apply(&*spy, Some(&r), "ada", &cfg(&[("name", "Three")]), false).unwrap();
    assert_eq!(r.current().brand().name(), "Three");
}

#[test]
fn a_store_that_cannot_be_read_fails_the_call_with_its_text_unchanged() {
    let spy = Spy::memory();
    spy.fail(MODEL, "storage_busy: the pool is full");
    let e = run(&spy, &cfg(&[("name", "MOHV")])).unwrap_err();
    assert_eq!(e, Failure::Storage("storage_busy: the pool is full".into()));
    assert_eq!(e.to_string(), "storage_busy: the pool is full");
    // The theme is read for the answer's v, before anything is written: a failure there changes
    // nothing either.
    spy.heal();
    spy.fail(SETTING_MODEL, "storage_timeout: waited 5s");
    let e = run(&spy, &cfg(&[("name", "MOHV")])).unwrap_err();
    assert!(matches!(e, Failure::Storage(ref t) if t.starts_with("storage_timeout")));
    assert!(stored(&spy).is_none() && events(&spy) == 0);
}

#[test]
fn the_answers_carry_the_changes_as_json() {
    let spy = Spy::memory();
    let a = ok(&spy, &[("name", "MOHV"), ("title_sv", "Logga in")]);
    let j: J = serde_json::from_str(&changes_json(&a.changes)).unwrap();
    assert_eq!(j.as_array().unwrap().len(), 2);
    assert_eq!(
        j[0],
        serde_json::json!({"field": "name", "old": "", "new": "MOHV"})
    );
    assert_eq!(
        j[1],
        serde_json::json!({"field": "title_sv", "old": "", "new": "Logga in"})
    );
}

/// The details of the log in the order they were written. The list of the store is in insertion
/// order; the ids are not a usable order here (an id is the second and a random suffix, so the
/// events of one call, which share a second, sort at random).
fn details(spy: &Spy) -> Vec<String> {
    spy.list("security_event")
        .unwrap()
        .into_iter()
        .map(|(_, f)| text(&f, "detail"))
        .collect()
}

#[test]
fn an_empty_text_repairs_a_stored_value_that_is_not_text() {
    use serde_json::json;
    let spy = Spy::memory();
    put_row(
        &*spy,
        &[
            ("name", s("Acme Co")),
            ("slug", json!(true)),
            ("title_en", json!(5)),
            ("message_sv", json!(["a"])),
            ("logo_svg", json!(12)),
        ],
    );
    // The empty text is how a field is cleared, so it clears this one too: the value is gone, the
    // change is written to the log (old value as the word the fallback list uses), and the
    // fallback is gone.
    let a = ok(
        &spy,
        &[
            ("slug", ""),
            ("title_en", ""),
            ("message_sv", ""),
            ("logo_svg", ""),
        ],
    );
    let changed: Vec<&str> = a.changes.iter().map(|c| c.field.as_str()).collect();
    assert_eq!(changed, ["slug", "title_en", "message_sv", "logo_svg"]);
    let row = stored(&spy).unwrap();
    assert_eq!(row.keys().collect::<Vec<_>>(), ["name"], "{row:?}");
    assert_eq!(
        details(&spy),
        [
            "slug: \"not_text\" -> \"\"",
            "title_en: \"not_text\" -> \"\"",
            "message_sv: \"not_text\" -> \"\"",
            "logo_svg: unreadable -> none",
        ]
    );
    // The same call again changes nothing and writes nothing.
    let again = ok(
        &spy,
        &[
            ("slug", ""),
            ("title_en", ""),
            ("message_sv", ""),
            ("logo_svg", ""),
        ],
    );
    assert!(again.changes.is_empty());
    assert_eq!(events(&spy), 4);
}

#[test]
fn a_value_replaces_a_stored_value_that_is_not_text_and_the_log_says_what_it_replaced() {
    use serde_json::json;
    let spy = Spy::memory();
    put_row(
        &*spy,
        &[("name", s("Acme Co")), ("title_de", json!({"x": 1}))],
    );
    ok(&spy, &[("title_de", "Hallo")]);
    assert_eq!(text(&stored(&spy).unwrap(), "title_de"), "Hallo");
    assert_eq!(details(&spy), ["title_de: \"not_text\" -> \"Hallo\""]);
}
