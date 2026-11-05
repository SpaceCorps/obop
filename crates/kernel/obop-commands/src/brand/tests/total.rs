//! The reader and the policy checks never panic (docs/spec/branding.md, section 13, "A blocked
//! store, a panic, hostile text"): whatever text any field of a stored row holds, `current`, `fresh`,
//! `strict` and `apply` return a value or a refusal. Asserted under `catch_unwind` with a fixed-seed
//! generator written here (no property-test crate is in the lock), and by cutting a valid file at
//! every length.

use super::*;
use crate::brand::{Action, apply, strict};
use obop_brand::policy::{self, Kind};
use obop_brand::slug;
use std::panic::{AssertUnwindSafe, catch_unwind};

/// A small deterministic generator (SplitMix64): the same sequence on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Pieces that make trouble: markup, quotes, control and format characters, bidirectional
/// controls, spaces that are not U+0020, private use, the end of the range, combining marks,
/// scripts, and the beginnings of a hostile file.
const PIECES: &[&str] = &[
    "a",
    "Z",
    "0",
    "7",
    " ",
    "-",
    ".",
    ",",
    "&",
    "'",
    "\"",
    "<",
    ">",
    "/",
    "\\",
    ":",
    "@",
    "=",
    "(",
    ")",
    "!",
    "+",
    "_",
    "\n",
    "\r",
    "\t",
    "\u{0}",
    "\u{1b}",
    "\u{7f}",
    "\u{80}",
    "\u{ad}",
    "\u{200b}",
    "\u{200d}",
    "\u{202e}",
    "\u{2066}",
    "\u{feff}",
    "\u{00a0}",
    "\u{2003}",
    "\u{3000}",
    "\u{fe0f}",
    "\u{e000}",
    "\u{e0001}",
    "\u{10ffff}",
    "\u{301}",
    "\u{0308}",
    "\u{3164}",
    "\u{0436}",
    "\u{043e}",
    "\u{4eba}",
    "\u{30fc}",
    "\u{1f600}",
    "\u{fb01}",
    "\u{ff53}",
    "\u{ff0e}",
    "ß",
    "İ",
    "ǅ",
    "<svg",
    "</svg>",
    " viewBox=\"0 0 64 64\"",
    "<path d=\"",
    "M 1 2 L 3 4 Z",
    "\"/>",
    "<script>",
    "onload=\"x\"",
    "xmlns=\"http://www.w3.org/2000/svg\"",
    "1e5",
    "100000.1",
    "-0.00001",
    ".5.5",
    "a1 1 0 011 1",
    "<?xml version=\"1.0\"?>",
    "<!--",
    "]]>",
    "&amp;",
    "url(",
    "javascript:",
    "shop.example",
    "1234567",
    "#fff",
    "#12345",
];

fn text(rng: &mut Rng, max_pieces: usize) -> String {
    let n = rng.below(max_pieces + 1);
    (0..n).map(|_| PIECES[rng.below(PIECES.len())]).collect()
}

/// A stored value: usually text, sometimes something that is not.
fn value(rng: &mut Rng, max_pieces: usize) -> J {
    match rng.below(20) {
        0 => J::Null,
        1 => J::from(rng.next() as i64),
        2 => J::Bool(true),
        3 => serde_json::json!([text(rng, 3)]),
        4 => serde_json::json!({"x": text(rng, 3)}),
        _ => s(&text(rng, max_pieces)),
    }
}

fn random_row(rng: &mut Rng) -> Vec<(String, J)> {
    let mut keys: Vec<String> = vec!["name".into(), "slug".into(), "logo_svg".into()];
    for l in obop_brand::LANGS {
        keys.push(format!("title_{l}"));
        keys.push(format!("message_{l}"));
    }
    keys.push("hide_product_docs".into());
    let mut row = Vec::new();
    for k in keys {
        if rng.below(3) == 0 {
            continue;
        }
        let max = if k == "logo_svg" { 40 } else { 30 };
        row.push((k, value(rng, max)));
    }
    row
}

fn put(b: &dyn Backend, row: &[(String, J)]) {
    let pairs: Vec<(&str, J)> = row.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    put_row(b, &pairs);
}

/// What every answer of the reader must satisfy whatever the row holds.
fn check_shown(shown: &crate::brand::Shown, what: &str) {
    let b = shown.brand();
    assert!(
        policy::safe(Kind::Name, b.name()).is_ok(),
        "{what}: name {:?}",
        b.name()
    );
    assert!(slug::is_valid(&b.slug()), "{what}: slug {:?}", b.slug());
    for l in obop_brand::LANGS {
        assert!(
            policy::safe(Kind::Title, b.stored_title(l)).is_ok(),
            "{what}: title_{l}"
        );
        assert!(
            policy::safe(Kind::Message, b.stored_message(l)).is_ok(),
            "{what}: message_{l}"
        );
    }
    if let Some(logo) = shown.logo() {
        let served = logo.served();
        assert!(
            served.starts_with("<svg ") && served.ends_with("</svg>"),
            "{what}: {served}"
        );
        assert!(
            !served.contains("<script") && !served.contains("onload"),
            "{what}"
        );
        if !logo.is_tile() {
            // An artwork is the printer's output: only the bytes of section 8.
            for byte in served.bytes() {
                assert!(
                    byte.is_ascii_alphanumeric() || b" ,.:/#()=<>\"-".contains(&byte),
                    "{what}: byte {byte:#04x} in {served}"
                );
            }
            assert!(served.len() <= 16 * 1024, "{what}");
        }
        assert_eq!(shown.v().len(), 16, "{what}");
    } else {
        assert!(!shown.is_set(), "{what}");
    }
}

#[test]
fn whatever_a_stored_row_holds_the_reader_returns_a_value_and_never_panics() {
    let mut rng = Rng(0x0b0b_2026_1105);
    for case in 0..500 {
        let row = random_row(&mut rng);
        let spy = Spy::memory();
        put(&*spy, &row);
        let (r, _) = reader(&spy, LONG);
        let what = format!("case {case}: {row:?}");
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            let shown = r.current();
            check_shown(&shown, &what);
            let fresh = r.fresh().expect("the store answers");
            check_shown(fresh.shown(), &what);
            assert_eq!(fresh.shown().is_set(), shown.is_set());
            let _ = strict(&*spy);
            let _ = crate::brand::fallbacks_json(shown.fallbacks());
            // The inline form and the served form are total too.
            if let Some(l) = shown.logo() {
                let _ = l.inline(28);
            }
        }));
        assert!(outcome.is_ok(), "{what}");
    }
}

#[test]
fn apply_never_panics_on_generated_input() {
    let mut rng = Rng(0x0a11_ce00_0001);
    for case in 0..300 {
        let spy = Spy::memory();
        let mut fields = Vec::new();
        let mut keys: Vec<String> = crate::brand::writable_fields();
        keys.push("v".into());
        keys.push(text(&mut rng, 3));
        for k in keys {
            if rng.below(3) == 0 {
                fields.push((k, text(&mut rng, 25)));
            }
        }
        let action = if rng.below(8) == 0 {
            Action::Clear
        } else {
            Action::Configure(fields)
        };
        for dry in [true, false] {
            let r = catch_unwind(AssertUnwindSafe(|| apply(&*spy, None, "ada", &action, dry)));
            assert!(r.is_ok(), "case {case}: {action:?}");
        }
    }
}

#[test]
fn a_valid_artwork_cut_at_every_length_is_a_tile_or_a_logo_and_never_a_panic() {
    let spy = Spy::memory();
    let (r, _) = reader(&spy, Duration::ZERO);
    let chars: Vec<usize> = M_ARTWORK
        .char_indices()
        .map(|(i, _)| i)
        .chain([M_ARTWORK.len()])
        .collect();
    let mut accepted = 0;
    for end in chars {
        let cut = &M_ARTWORK[..end];
        put_row(&*spy, &[("name", s("Acme")), ("logo_svg", s(cut))]);
        let shown = catch_unwind(AssertUnwindSafe(|| r.current())).expect("no panic");
        check_shown(&shown, &format!("cut at {end}"));
        if !shown.logo().unwrap().is_tile() {
            accepted += 1;
        }
    }
    assert_eq!(accepted, 1, "only the whole file is an artwork");
}

#[test]
fn a_huge_stored_value_is_refused_without_reading_it_all() {
    let spy = Spy::memory();
    let huge = "<".repeat(2_000_000);
    put_row(
        &*spy,
        &[
            ("name", s(&"n".repeat(1_000_000))),
            ("message_en", s(&huge)),
            ("logo_svg", s(&huge)),
        ],
    );
    let (r, _) = reader(&spy, LONG);
    // A watchdog against a regression to something quadratic, not a measurement of speed: it holds
    // on a machine that is loaded many times over.
    let started = std::time::Instant::now();
    let shown = r.current();
    assert!(started.elapsed() < Duration::from_secs(120));
    assert_eq!(shown.brand().name(), "obop");
    assert!(shown.logo().unwrap().is_tile());
    assert_eq!(shown.fallbacks().len(), 3);
}
