//! The SQL key expressions of `obop_storage::keyexpr` against the in-memory evaluator
//! (`query::stored_key`, `Gate::grants`, `Query::matches`), over a real PostgreSQL server (K3,
//! D76, D78).
//!
//! The in-memory evaluator is the oracle: a stored value that is not valid for its kind counts as
//! empty. Every SQL slot must return `NULL` exactly there and the same key everywhere else, and
//! no expression may raise on any JSON value. The test loads tens of thousands of stored
//! documents, the hostile ones on purpose (floats where integers belong, `5.0`, `1e3`, huge
//! numbers, wrong JSON types, null, nested values, impossible and non-canonical dates and
//! instants, over-long decimals, Unicode digits, odd white space), reads every document back the
//! way the backend does (the `jsonb` text, parsed by `serde_json`) and compares, row by row and
//! slot by slot, the expression with the oracle. It also compares the order of the collated text
//! slots with Rust's byte order, the `shared_with` tokens and membership, and `contains` through
//! `like_pattern`, then repeats everything in a session with unusual settings.
//!
//! It runs when `OBOP_TEST_POSTGRES_URL` is set, in a temporary table of its own that vanishes
//! with the connection. `OBOP_PARITY_SEED` and `OBOP_PARITY_PER_KIND` (default 2,500 generated
//! values per kind) change the corpus.

use crate::query::{Key, slot_of, stored_key};
use obop_registry::FieldKind;
use obop_storage::keyexpr::{self, Slot};
use obop_values::Decimal;
use postgres::Client;
use serde_json::Value as J;
use std::sync::Arc;

/// The options of the select field under test (one of them empty, one with a comma, one in
/// accents).
const OPTIONS: [&str; 6] = ["open", "in progress", "done", "ünï", "", "a,b"];

fn options() -> Arc<[&'static str]> {
    Arc::from(OPTIONS.to_vec())
}

fn decimal_kind() -> FieldKind {
    FieldKind::Decimal {
        precision: 38,
        scale: 12,
        round: None,
    }
}

// ---- generators -------------------------------------------------------------------------------

/// SplitMix64: small, deterministic, no dependency.
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
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len())]
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

/// Characters that have bitten text handling somewhere: the eight member separators, Unicode
/// spaces, quotes and the LIKE specials, accents, final sigma, dotted capital I, ligature-like
/// capitals, combining marks, CJK, emoji, right to left, fullwidth digits, control characters
/// (but never NUL) and the last scalar value.
const ALPHABET: &[char] = &[
    'a',
    'b',
    'z',
    'A',
    'Z',
    '0',
    '1',
    '9',
    ' ',
    ',',
    ';',
    '.',
    '-',
    '_',
    '%',
    '\\',
    '\'',
    '"',
    '/',
    '|',
    '\t',
    '\n',
    '\r',
    '\u{c}',
    '\u{b}',
    '\u{a0}',
    '\u{2003}',
    '\u{85}',
    '\u{2028}',
    '\u{2029}',
    '\u{3000}',
    '\u{200b}',
    '\u{feff}',
    'é',
    'ñ',
    'ü',
    'ß',
    'Å',
    'ø',
    'Ł',
    'ç',
    'Σ',
    'σ',
    'ς',
    'Α',
    'Ω',
    'Д',
    'д',
    'Я',
    'İ',
    'ı',
    'I',
    'i',
    'ǅ',
    'ẞ',
    '漢',
    '字',
    '😀',
    '\u{301}',
    '\u{308}',
    'ש',
    '０',
    '９',
    '\u{1}',
    '\u{1f}',
    '\u{7f}',
    '\u{10ffff}',
];

fn json_string(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}

fn text_of(r: &mut Rng, max: usize) -> String {
    let n = r.below(max + 1);
    (0..n)
        .map(|_| {
            if r.chance(55) {
                *r.pick(ALPHABET)
            } else {
                (b'a' + r.below(26) as u8) as char
            }
        })
        .collect()
}

/// A small edit that turns a valid text into a near miss.
fn mutate(s: &str, r: &mut Rng) -> String {
    let mut cs: Vec<char> = s.chars().collect();
    match r.below(9) {
        0 if !cs.is_empty() => {
            let i = r.below(cs.len());
            cs.remove(i);
        }
        1 => {
            let i = r.below(cs.len() + 1);
            cs.insert(i, *r.pick(ALPHABET));
        }
        2 if !cs.is_empty() => {
            let i = r.below(cs.len());
            cs[i] = *r.pick(ALPHABET);
        }
        3 => return s.to_uppercase(),
        4 => cs.push(*r.pick(&[' ', '\n', '\t', '\u{a0}', '\r', '\u{85}'])),
        5 => cs.insert(0, *r.pick(&[' ', '\n', '\t', '\u{a0}', '+', '-'])),
        6 if !cs.is_empty() => {
            let i = r.below(cs.len());
            cs.insert(i, cs[i]);
        }
        7 => cs.truncate(r.below(cs.len() + 1)),
        _ => {}
    }
    cs.into_iter().collect()
}

fn days_in_month(y: u32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400) => 29,
        _ => 28,
    }
}

fn ymd(r: &mut Rng) -> (u32, u32, u32) {
    let y = 1 + r.below(9999) as u32;
    let m = 1 + r.below(12) as u32;
    let d = 1 + r.below(days_in_month(y, m) as usize) as u32;
    (y, m, d)
}

fn gen_date(r: &mut Rng) -> String {
    let (y, m, d) = ymd(r);
    let base = format!("{y:04}-{m:02}-{d:02}");
    match r.below(12) {
        0..=3 => base,
        4 => format!("{y:04}-{:02}-{d:02}", *r.pick(&[0u32, 13, 14, 20, 99])),
        5 => format!(
            "{y:04}-{m:02}-{:02}",
            *r.pick(&[0, days_in_month(y, m) + 1, 32, 33, 99])
        ),
        6 => format!(
            "{:04}-02-29",
            *r.pick(&[1900, 1901, 2000, 2024, 2100, 2023, y])
        ),
        7 => format!("0000-{m:02}-{d:02}"),
        8 | 9 => mutate(&base, r),
        10 => (0..10)
            .map(|_| *r.pick(&['0', '1', '2', '9', '-', '-']))
            .collect(),
        _ => base
            .chars()
            .map(|c| match c.to_digit(10) {
                Some(n) if r.chance(30) => char::from_u32('０' as u32 + n).unwrap(),
                _ => c,
            })
            .collect(),
    }
}

fn gen_instant(r: &mut Rng) -> String {
    let (y, m, d) = ymd(r);
    let (h, mi, s, us) = (r.below(24), r.below(60), r.below(60), r.below(1_000_000));
    let at = |y: u32, m: u32, d: u32, h: usize, mi: usize, s: usize| {
        format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}")
    };
    let base = format!("{}.{us:06}Z", at(y, m, d, h, mi, s));
    match r.below(15) {
        0..=4 => base,
        5 => format!("{}.{us:06}Z", at(y, m, d, 24 + r.below(76), mi, s)),
        6 => format!("{}.{us:06}Z", at(y, m, d, h, 60 + r.below(40), s)),
        7 => format!("{}.{us:06}Z", at(y, m, d, h, mi, 60 + r.below(40))),
        8 => format!("{}.{us:06}Z", at(y, 13, d, h, mi, s)),
        9 => format!("{}.{us:06}Z", at(y, 2, 30, h, mi, s)),
        10 => format!("{}Z", at(y, m, d, h, mi, s)),
        11 => {
            let n = *r.pick(&[1usize, 2, 3, 4, 5, 7, 9]);
            let frac: String = (0..n).map(|_| (b'0' + r.below(10) as u8) as char).collect();
            format!("{}.{frac}Z", at(y, m, d, h, mi, s))
        }
        12 => format!(
            "{}.{us:06}{}",
            at(y, m, d, h, mi, s),
            r.pick(&["+00:00", "+02:00", "-03:30", "z", ""])
        ),
        _ => mutate(&base, r),
    }
}

fn gen_decimal(r: &mut Rng) -> String {
    let int_len = *r.pick(&[0usize, 1, 1, 1, 2, 3, 5, 10, 20, 37, 38, 39, 45, 70]);
    let frac_len = if r.chance(35) {
        None
    } else {
        Some(*r.pick(&[1usize, 1, 2, 2, 3, 6, 12, 20, 37, 38, 39, 45]))
    };
    let leading_zeros = r.chance(30);
    let mut s = String::new();
    if r.chance(30) {
        s.push('-');
    }
    if int_len == 0 {
        s.push('0');
    } else {
        s.push_str(&digits(r, int_len, leading_zeros));
    }
    if let Some(n) = frac_len {
        s.push('.');
        let zeros = n > 20 && int_len < 3;
        s.push_str(&digits(r, n, zeros));
    }
    if r.chance(22) { mutate(&s, r) } else { s }
}

/// `n` random digits, with many zeros when `zeros` says so.
fn digits(r: &mut Rng, n: usize, zeros: bool) -> String {
    (0..n)
        .map(|_| {
            if zeros && r.chance(70) {
                '0'
            } else {
                (b'0' + r.below(10) as u8) as char
            }
        })
        .collect()
}

fn gen_number(r: &mut Rng) -> String {
    let small = |r: &mut Rng| r.below(2001) as i64 - 1000;
    match r.below(15) {
        0 => small(r).to_string(),
        1 => (r.next() as i64).to_string(),
        2 => {
            let b = *r.pick(&[i64::MAX, i64::MIN, 1 << 53, -(1 << 53)]);
            b.saturating_add(r.below(5) as i64 - 2).to_string()
        }
        3 => r.next().to_string(),
        4 => {
            let n = 18 + r.below(22);
            let first = (b'1' + r.below(9) as u8) as char;
            let sign = if r.chance(30) { "-" } else { "" };
            format!("{sign}{first}{}", digits(r, n, false))
        }
        5 => format!("{}.{}", small(r), "0".repeat(1 + r.below(4))),
        6 => format!("{}.{}", small(r), r.below(1000)),
        7 => format!("{}e{}", small(r), r.below(30) as i64 - 3),
        8 => format!("{}E+{}", small(r), r.below(25)),
        9 => format!("{}.{}e{}", small(r), r.below(100), r.below(20) as i64 - 5),
        10 => format!("{}e-{}", small(r).abs() * 1000, r.below(4)),
        11 => r.pick(&["-0.0", "0.0", "-0", "0e0", "0.000"]).to_string(),
        12 => format!("1e{}", r.pick(&[400, 399, -400, 19, 18, 308, 309])),
        13 => format!("{}", i64::MAX - r.below(3) as i64),
        _ => format!("{}.0", r.next() as i64),
    }
}

fn gen_members(r: &mut Rng) -> String {
    let ids = [
        "ann",
        "bob",
        "Åsa",
        "o'brien",
        "a.b*c",
        "100%",
        "x_y",
        "joann",
        "anna",
        "é1",
        "_x",
        "A1",
        "a1",
        "ann\u{a0}",
        "Ann",
    ];
    let seps: [&str; 14] = [
        ",", ";", " ", "\t", "\n", "\r\n", "\u{c}", "\u{b}", ", ", " ; ", "\u{a0}", "\u{2003}",
        "|", "",
    ];
    let mut s = String::new();
    if r.chance(25) {
        s.push_str(r.pick(&seps));
    }
    for i in 0..r.below(7) {
        if i > 0 {
            s.push_str(r.pick(&seps));
        }
        if r.chance(70) {
            s.push_str(r.pick(&ids));
        } else {
            s.push_str(&text_of(r, 6));
        }
    }
    if r.chance(25) {
        s.push_str(r.pick(&seps));
    }
    s
}

fn gen_option(r: &mut Rng) -> String {
    let o = *r.pick(&OPTIONS);
    match r.below(5) {
        0 => mutate(o, r),
        1 => text_of(r, 8),
        _ => o.to_string(),
    }
}

/// A value of the wrong type or kind, as raw JSON text.
fn gen_other(r: &mut Rng) -> String {
    match r.below(12) {
        0 => "null".into(),
        1 => "true".into(),
        2 => "false".into(),
        3 => "[]".into(),
        4 => "{}".into(),
        5 => format!("[{}]", json_string(&text_of(r, 5))),
        6 => format!("{{\"f\":{}}}", gen_number(r)),
        7 => json_string(&r.below(100).to_string()),
        8 => json_string(r.pick(&["true", "false", "null", "t", "yes", "1", "0"])),
        9 => format!("[[{}]]", gen_number(r)),
        10 => format!("[true, {}]", json_string(&gen_date(r))),
        _ => gen_number(r),
    }
}

/// Hand-written values, as raw JSON text (`None`: the field is missing from the document).
fn hand() -> Vec<Option<String>> {
    let mut raw: Vec<String> = Vec::new();
    for n in [
        "0",
        "-0",
        "1",
        "-1",
        "5",
        "5.0",
        "5.00",
        "-5.0",
        "0.0",
        "-0.0",
        "1.5",
        "-1.5",
        "0.5",
        "1e3",
        "1E3",
        "1e+3",
        "12e-1",
        "10e-1",
        "1000e-3",
        "0.5e1",
        "5e0",
        "1e18",
        "9.223372036854775807e18",
        "9223372036854775807",
        "9223372036854775807.0",
        "9223372036854775806",
        "9223372036854775808",
        "-9223372036854775808",
        "-9223372036854775809",
        "-9223372036854775808.0",
        "18446744073709551615",
        "18446744073709551616",
        "1e19",
        "1e400",
        "-1e400",
        "1e-400",
        "1e1000",
        "123456789012345678901234567890",
        "9007199254740992",
        "9007199254740993",
        "-9007199254740993",
        "0.1",
        "0.30000000000000004",
        "1e0",
        "100e-2",
        "1.0e1",
        "92233720368547758070e-1",
        "922337203685477580.7e1",
        "true",
        "false",
        "null",
        "[]",
        "{}",
        "[1]",
        "[\"a\"]",
        "[[]]",
        "{\"a\":1}",
        "[true]",
        "{\"f\":1}",
        "[\"2026-01-01\"]",
        "{\"f\":\"x\"}",
    ] {
        raw.push(n.to_string());
    }
    let nines38 = "9".repeat(38);
    let long = |n: usize, c: &str| c.repeat(n);
    let strings: Vec<String> = [
        // dates
        "2026-01-01",
        "2024-02-29",
        "2023-02-29",
        "1900-02-29",
        "2000-02-29",
        "2100-02-29",
        "0001-01-01",
        "0000-01-01",
        "9999-12-31",
        "9999-12-32",
        "2026-00-10",
        "2026-13-10",
        "2026-04-31",
        "2026-04-30",
        "2026-02-30",
        "2026-1-1",
        "2026-01-1",
        "20260101",
        "2026-01-01 ",
        " 2026-01-01",
        "2026-01-01\n",
        "2026-01-01\t",
        "2026-01-01T00:00:00Z",
        "2026/01/01",
        "２０２６-０１-０１",
        "٢٠٢٦-٠١-٠١",
        "2026-01-0１",
        "202６-01-01",
        "-026-01-01",
        "+026-01-01",
        "é026-01-01",
        // instants
        "2026-03-29T01:30:00.000000Z",
        "0001-01-01T00:00:00.000000Z",
        "9999-12-31T23:59:59.999999Z",
        "1970-01-01T00:00:00.000000Z",
        "2024-02-29T23:59:59.999999Z",
        "2026-02-30T00:00:00.000000Z",
        "2026-13-01T00:00:00.000000Z",
        "2026-00-01T00:00:00.000000Z",
        "2026-01-00T00:00:00.000000Z",
        "2026-01-32T00:00:00.000000Z",
        "2026-03-29T24:00:00.000000Z",
        "2026-03-29T23:60:00.000000Z",
        "2026-03-29T23:59:60.000000Z",
        "0000-01-01T00:00:00.000000Z",
        "2023-02-29T00:00:00.000000Z",
        "2026-03-29T01:30:00Z",
        "2026-03-29T01:30:00.5Z",
        "2026-03-29T01:30:00.123Z",
        "2026-03-29T01:30:00.1234567Z",
        "2026-03-29T01:30:00.000000+00:00",
        "2026-03-29T03:30:00.000000+02:00",
        "2026-03-29t01:30:00.000000Z",
        "2026-03-29T01:30:00.000000z",
        "2026-03-29 01:30:00.000000Z",
        " 2026-03-29T01:30:00.000000Z",
        "2026-03-29T01:30:00.000000Z ",
        "2026-03-29T01:30:00.000000Z\n",
        "２０２６-03-29T01:30:00.000000Z",
        "2026-03-29T01:30:00,000000Z",
        "2026-03-29T01:30:00.000000",
        // decimals
        "0",
        "-0",
        "7",
        "-7",
        "0.5",
        "-0.05",
        "12.3400",
        "007.5",
        "00.00",
        "1.",
        "-.5",
        ".5",
        "5.",
        "+5",
        "1e3",
        "1E3",
        " 1",
        "1 ",
        "1\n",
        "1,5",
        "1_000",
        "NaN",
        "Infinity",
        "-Infinity",
        "0x10",
        "--1",
        "1.2.3",
        "-",
        "",
        ".",
        "١٢",
        "１２",
        "1.５",
        // selects, text, members
        "open",
        "Open",
        "open ",
        "in progress",
        "in  progress",
        "done",
        "ünï",
        "u\u{308}ni\u{308}",
        "a,b",
        "a",
        "ann",
        "ann,bob",
        "ann, bob",
        "ann; bob\tcy\nd",
        "\r\n",
        "a b",
        "\u{a0}",
        "ann\u{a0}",
        "\u{2003}ann",
        "a\u{85}b",
        "a\u{2028}b",
        "é",
        "Åsa",
        "Σ",
        "ΑΣ",
        "ΑΣ Β",
        "İ",
        "ß",
        "ǅ",
        "ẞ",
        "😀",
        "a\\b",
        "a'b",
        "a\"b",
        "%",
        "_",
        "100%",
        "\\",
        "\u{1}",
        "\u{7f}",
        "a\u{301}",
        "true",
        "false",
        "null",
    ]
    .iter()
    .map(|s| s.to_string())
    .chain([
        nines38.clone(),
        "9".repeat(39),
        format!("0.{}1", long(37, "0")),
        format!("0.{}1", long(38, "0")),
        format!("0.{}", long(38, "0")),
        format!("0.{}", long(39, "0")),
        format!("1{}", long(79, "0")),
        format!("1{}", long(80, "0")),
        long(80, "0"),
        long(81, "0"),
        format!("-{}", long(79, "0")),
        format!("-{}", long(80, "0")),
        format!("0.{}", long(38, "1")),
        format!("{}.{}", long(38, "1"), long(38, "1")),
        format!("0.{}{}", long(30, "0"), long(38, "1")),
        format!("{}{}", long(40, "0"), long(38, "1")),
        format!("{}{}", long(40, "0"), long(39, "1")),
        format!("{}0", long(38, "1")),
        format!("-{nines38}.{nines38}"),
        format!("{}7", long(100, "0")),
        "x".repeat(10_000),
    ])
    .collect();
    for s in &strings {
        raw.push(json_string(s));
    }
    let mut out: Vec<Option<String>> = raw.into_iter().map(Some).collect();
    out.push(None);
    out
}

fn corpus(seed: u64, per_kind: usize) -> Vec<Option<String>> {
    let mut r = Rng(seed);
    let mut out = hand();
    for _ in 0..per_kind {
        out.push(Some(gen_number(&mut r)));
        out.push(Some(json_string(&gen_date(&mut r))));
        out.push(Some(json_string(&gen_instant(&mut r))));
        out.push(Some(json_string(&gen_decimal(&mut r))));
        out.push(Some(json_string(&text_of(&mut r, 14))));
        out.push(Some(json_string(&gen_members(&mut r))));
        out.push(Some(json_string(&gen_option(&mut r))));
        out.push(Some(gen_other(&mut r)));
    }
    out
}

// ---- the oracle ---------------------------------------------------------------------------

fn tokens_json(list: &str) -> Key {
    Key::Str(serde_json::to_string(&keyexpr::member_tokens(list)).unwrap())
}

/// What the in-memory evaluator says about a stored value for a slot. `None` is "empty".
fn expected(slot: Slot, v: Option<&J>) -> Option<Key> {
    if slot == Slot::Members {
        // `Gate::grants`: a non-empty string is split; anything else lists nobody.
        return Some(match v {
            Some(J::String(s)) if !s.is_empty() => tokens_json(s),
            _ => Key::Str("[]".into()),
        });
    }
    let v = v?;
    match slot {
        Slot::Str | Slot::Raw => stored_key(&FieldKind::Text, v),
        Slot::Int => stored_key(&FieldKind::Number, v),
        Slot::Bool => stored_key(&FieldKind::Bool, v),
        Slot::Date => stored_key(&FieldKind::Date, v),
        Slot::Instant => stored_key(&FieldKind::Datetime, v),
        Slot::Dec => stored_key(&decimal_kind(), v),
        // The key of a select is the 0-based position; the SQL slot says 1-based.
        Slot::SelPos => stored_key(&FieldKind::Select(options()), v),
        // `Query::matches` lower-cases a text before it looks for the needle.
        Slot::Lower => match v {
            J::String(s) => Some(Key::Str(s.to_lowercase())),
            _ => None,
        },
        Slot::Members | Slot::Id => None,
    }
}

/// What the SQL expression returned, as a key, or an error when it returned something the slot
/// can never return (a text that is not an integer, a date that changed on the way).
fn actual(slot: Slot, sql: Option<&str>, v: Option<&J>) -> Result<Option<Key>, String> {
    let Some(t) = sql else {
        return Ok((slot == Slot::Members).then(|| Key::Str("[]".into())));
    };
    let same_text = |t: &str| -> Result<(), String> {
        match v {
            Some(J::String(s)) if s == t => Ok(()),
            _ => Err(format!(
                "the slot returned {t:?}, which is not the stored text"
            )),
        }
    };
    Ok(match slot {
        Slot::Str | Slot::Raw | Slot::Lower => Some(Key::Str(t.to_string())),
        Slot::Int => Some(Key::Int(
            t.parse::<i64>()
                .map_err(|e| format!("{t:?} is not an i64: {e}"))?,
        )),
        Slot::Bool => Some(Key::Int(match t {
            "true" => 1,
            "false" => 0,
            o => return Err(format!("{o:?} is not a boolean")),
        })),
        // A slot that returns a text the evaluator calls empty is wrong, not merely equal to it.
        Slot::Date => {
            same_text(t)?;
            Some(stored_key(&FieldKind::Date, &J::from(t)).ok_or("a date the evaluator refuses")?)
        }
        Slot::Instant => {
            same_text(t)?;
            Some(
                stored_key(&FieldKind::Datetime, &J::from(t))
                    .ok_or("an instant the evaluator refuses")?,
            )
        }
        Slot::Dec => Some(Key::Dec(
            Decimal::parse(t).map_err(|e| format!("{t:?} is not a decimal: {e}"))?,
        )),
        Slot::SelPos => Some(Key::Int(
            t.parse::<i64>().map_err(|e| format!("{t:?}: {e}"))? - 1,
        )),
        Slot::Members => {
            let all: Vec<String> =
                serde_json::from_str(t).map_err(|e| format!("{t:?} is not a list: {e}"))?;
            let tokens: Vec<&str> = all
                .iter()
                .map(String::as_str)
                .filter(|x| !x.is_empty())
                .collect();
            Some(Key::Str(serde_json::to_string(&tokens).unwrap()))
        }
        Slot::Id => None,
    })
}

// ---- the database side --------------------------------------------------------------------

struct Row {
    n: i32,
    id: String,
    /// The document as the backend reads it back; `None` for a field the document lacks or a
    /// value `serde_json` cannot read (a number beyond a double), which counts as empty.
    v: Option<J>,
}

#[derive(Default)]
struct Tally {
    comparisons: usize,
    orderings: usize,
    probes: usize,
    diffs: Vec<String>,
}

impl Tally {
    fn diff(&mut self, what: String) {
        if self.diffs.len() < 200 {
            self.diffs.push(what);
        } else if self.diffs.len() == 200 {
            self.diffs.push("... more".into());
        }
    }
}

fn short(s: &str) -> String {
    let t: String = s.chars().take(90).collect();
    if t.len() < s.len() {
        format!("{t}...")
    } else {
        t
    }
}

/// Load the corpus into a temporary table and read every document back as the backend does.
fn load(c: &mut Client, corpus: &[Option<String>], seed: u64) -> Vec<Row> {
    c.batch_execute(
        "CREATE TEMP TABLE obop_parity (n int PRIMARY KEY, id text NOT NULL, data jsonb NOT NULL)",
    )
    .unwrap();
    let mut r = Rng(seed ^ 0xA5A5);
    let ids: Vec<String> = (0..corpus.len())
        .map(|i| {
            if r.chance(50) {
                text_of(&mut r, 6)
            } else {
                format!("{}{i}", text_of(&mut r, 3))
            }
        })
        .collect();
    let insert = "INSERT INTO obop_parity SELECT n, id, \
        CASE WHEN raw IS NULL THEN '{}'::jsonb ELSE ('{\"f\":' || raw || '}')::jsonb END \
        FROM unnest($1::int4[], $2::text[], $3::text[]) AS u(n, id, raw)";
    let mut rejected = 0;
    for (chunk_no, chunk) in corpus.chunks(1000).enumerate() {
        let base = (chunk_no * 1000) as i32;
        let ns: Vec<i32> = (0..chunk.len() as i32).map(|i| base + i).collect();
        let idv: Vec<String> = ns.iter().map(|n| ids[*n as usize].clone()).collect();
        if c.execute(insert, &[&ns, &idv, &chunk.to_vec()]).is_err() {
            // One of them is not storable as jsonb at all (a number beyond numeric): find it.
            for (i, raw) in chunk.iter().enumerate() {
                let (n, id) = ([base + i as i32], [ids[(base as usize) + i].clone()]);
                if c.execute(insert, &[&n.to_vec(), &id.to_vec(), &vec![raw.clone()]])
                    .is_err()
                {
                    rejected += 1;
                }
            }
        }
    }
    eprintln!(
        "keyexpr_parity: {} documents offered, {rejected} refused by jsonb itself",
        corpus.len()
    );
    let mut rows = Vec::new();
    for row in c
        .query("SELECT n, id, data::text FROM obop_parity ORDER BY n", &[])
        .unwrap()
    {
        let text: String = row.get(2);
        let v = match serde_json::from_str::<J>(&text) {
            Ok(J::Object(m)) => m.get("f").cloned(),
            Ok(other) => panic!("not an object: {other}"),
            Err(_) => {
                // A document `serde_json` cannot read (a number beyond a double) cannot be a
                // record of this system either; as far as the oracle goes the field is empty.
                None
            }
        };
        rows.push(Row {
            n: row.get(0),
            id: row.get(1),
            v,
        });
    }
    rows
}

fn run_column(c: &mut Client, slot: Slot, expr: &str) -> Result<Vec<Option<String>>, String> {
    let wrapped = if slot == Slot::Members {
        format!("to_jsonb({expr})")
    } else {
        expr.to_string()
    };
    let sql = format!("SELECT ({wrapped})::text FROM obop_parity ORDER BY n");
    let rows = if slot == Slot::SelPos {
        let opts: Vec<String> = OPTIONS.iter().map(|s| s.to_string()).collect();
        c.query(&sql, &[&opts])
    } else {
        c.query(&sql, &[])
    }
    .map_err(|e| e.to_string())?;
    Ok(rows.iter().map(|r| r.get::<_, Option<String>>(0)).collect())
}

/// Every row, one expression, against the oracle.
fn compare_slot(c: &mut Client, rows: &[Row], slot: Slot, expr: &str, t: &mut Tally) {
    let sql = match run_column(c, slot, expr) {
        Ok(s) => s,
        Err(e) => {
            t.diff(format!("{slot:?}: the expression raised: {e}"));
            return;
        }
    };
    assert_eq!(sql.len(), rows.len());
    for (row, got) in rows.iter().zip(&sql) {
        let want = expected(slot, row.v.as_ref());
        let have = match actual(slot, got.as_deref(), row.v.as_ref()) {
            Ok(k) => k,
            Err(e) => {
                t.diff(format!(
                    "{slot:?} n={} {:?}: {e}",
                    row.n,
                    short(&format!("{:?}", row.v))
                ));
                continue;
            }
        };
        t.comparisons += 1;
        if want != have {
            t.diff(format!(
                "{slot:?} n={} value {}: the oracle says {want:?}, SQL says {have:?}",
                row.n,
                short(&format!("{:?}", row.v))
            ));
        }
    }
}

fn expr_of(slot: Slot) -> String {
    match slot {
        Slot::SelPos => keyexpr::select_pos("f", None, "$1::text[]").unwrap(),
        Slot::Id => keyexpr::field_expr(Slot::Id, "id", None).unwrap(),
        s => keyexpr::field_expr(s, "f", None).unwrap(),
    }
}

/// The order of a collated slot is the order of the keys.
fn check_order(c: &mut Client, rows: &[Row], slot: Slot, t: &mut Tally) {
    let e = expr_of(slot);
    let filter = if slot == Slot::Id {
        String::new()
    } else {
        format!("WHERE ({e}) IS NOT NULL")
    };
    let sql = format!("SELECT n FROM obop_parity {filter} ORDER BY ({e}), n");
    let got: Vec<i32> = match c.query(&sql, &[]) {
        Ok(r) => r.iter().map(|r| r.get(0)).collect(),
        Err(e) => {
            t.diff(format!("{slot:?} order: {e}"));
            return;
        }
    };
    let mut want: Vec<(Key, i32)> = rows
        .iter()
        .filter_map(|r| {
            let k = if slot == Slot::Id {
                Some(Key::Str(r.id.clone()))
            } else {
                expected(slot, r.v.as_ref())
            };
            k.map(|k| (k, r.n))
        })
        .collect();
    want.sort();
    let want: Vec<i32> = want.into_iter().map(|(_, n)| n).collect();
    t.orderings += 1;
    if got != want {
        let at = got.iter().zip(&want).position(|(a, b)| a != b).unwrap_or(0);
        t.diff(format!(
            "{slot:?}: the order differs from the byte order of the keys at position {at} (of {})",
            want.len()
        ));
    }
}

fn ids_matching(c: &mut Client, sql: &str, param: &str) -> Result<Vec<i32>, String> {
    c.query(sql, &[&param])
        .map(|r| r.iter().map(|r| r.get(0)).collect())
        .map_err(|e| e.to_string())
}

/// `shared_with` membership through `@>` and `contains` through `LIKE`, probed with values taken
/// from the corpus and with near misses.
fn check_probes(c: &mut Client, rows: &[Row], seed: u64, with_lower: bool, t: &mut Tally) {
    let mut r = Rng(seed ^ 0x5EED);
    let strings: Vec<&str> = rows
        .iter()
        .filter_map(|x| match &x.v {
            Some(J::String(s)) => Some(s.as_str()),
            _ => None,
        })
        .collect();
    // Members.
    let members = keyexpr::field_expr(Slot::Members, "f", None).unwrap();
    let sql =
        format!("SELECT n FROM obop_parity WHERE ({members}) @> ARRAY[$1]::text[] ORDER BY n");
    let mut ids: Vec<String> = [
        "ann", "bob", "Åsa", "o'brien", "a.b*c", "100%", "x_y", "joann",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for _ in 0..140 {
        let s = *r.pick(&strings);
        let toks = keyexpr::member_tokens(s);
        if toks.is_empty() {
            continue;
        }
        let tok = *r.pick(&toks);
        ids.push(tok.to_string());
        if r.chance(40) {
            ids.push(format!("{tok}\u{a0}"));
        }
        if r.chance(40) {
            ids.push(format!("{tok}x"));
        }
        if r.chance(30) {
            ids.push(tok.to_uppercase());
        }
    }
    for id in &ids {
        let got = match ids_matching(c, &sql, id) {
            Ok(g) => g,
            Err(e) => {
                t.diff(format!("members probe {id:?}: {e}"));
                continue;
            }
        };
        let want: Vec<i32> = rows
            .iter()
            .filter(|x| match &x.v {
                Some(J::String(s)) if !s.is_empty() => {
                    keyexpr::member_tokens(s).contains(&id.as_str())
                }
                _ => false,
            })
            .map(|x| x.n)
            .collect();
        t.probes += 1;
        if got != want {
            t.diff(format!(
                "members probe {id:?}: SQL {} rows, oracle {} rows",
                got.len(),
                want.len()
            ));
        }
    }
    // Contains.
    if !with_lower {
        return;
    }
    let lower = keyexpr::field_expr(Slot::Lower, "f", None).unwrap();
    let sql = format!("SELECT n FROM obop_parity WHERE ({lower}) LIKE $1::text ORDER BY n");
    let mut needles: Vec<String> = [
        "", "%", "_", "\\", "ß", "σ", "ς", "i\u{307}", "100%", "a_b", "é", "ǆ",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for _ in 0..140 {
        let s: Vec<char> = r.pick(&strings).chars().collect();
        if s.is_empty() {
            continue;
        }
        let a = r.below(s.len());
        let b = a + 1 + r.below((s.len() - a).min(5));
        let mut piece: String = s[a..b].iter().collect();
        if r.chance(40) {
            piece = piece.to_uppercase();
        }
        needles.push(piece);
    }
    for needle in &needles {
        // The parse step lower-cases the needle, and the evaluator then lower-cases the text.
        let n = needle.to_lowercase();
        let got = match ids_matching(c, &sql, &keyexpr::like_pattern(&n)) {
            Ok(g) => g,
            Err(e) => {
                t.diff(format!("contains probe {needle:?}: {e}"));
                continue;
            }
        };
        let want: Vec<i32> = rows
            .iter()
            .filter(|x| matches!(&x.v, Some(J::String(s)) if s.to_lowercase().contains(&n)))
            .map(|x| x.n)
            .collect();
        t.probes += 1;
        if got != want {
            t.diff(format!(
                "contains probe {:?}: SQL {} rows, oracle {} rows",
                short(needle),
                got.len(),
                want.len()
            ));
        }
    }
}

fn run_suite(c: &mut Client, rows: &[Row], seed: u64, with_lower: bool, t: &mut Tally) {
    for slot in Slot::ALL {
        if slot == Slot::Id || (slot == Slot::Lower && !with_lower) {
            continue;
        }
        compare_slot(c, rows, slot, &expr_of(slot), t);
    }
    for slot in [Slot::Str, Slot::Date, Slot::Instant, Slot::Id] {
        check_order(c, rows, slot, t);
    }
    check_probes(c, rows, seed, with_lower, t);
}

fn connect() -> Option<Client> {
    let Ok(url) = std::env::var("OBOP_TEST_POSTGRES_URL") else {
        eprintln!("skipped: set OBOP_TEST_POSTGRES_URL to run against PostgreSQL");
        return None;
    };
    Some(obop_storage::tls::connect(&url).expect("connect to PostgreSQL"))
}

fn env_num(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn has_icu(c: &mut Client) -> bool {
    c.query_one(
        "SELECT count(*) FROM pg_collation WHERE collname = 'und-x-icu'",
        &[],
    )
    .map(|r| r.get::<_, i64>(0) > 0)
    .unwrap_or(false)
}

/// The slots of every kind, against the oracle, in two sessions: the defaults, and one with
/// settings that change how text, dates and numbers are read and printed.
#[test]
fn keyexpr_parity_on_postgresql() {
    let Some(mut c) = connect() else { return };
    let seed = env_num("OBOP_PARITY_SEED", 20_261_026);
    let per_kind = env_num("OBOP_PARITY_PER_KIND", 2_500) as usize;
    let corpus = corpus(seed, per_kind);
    let rows = load(&mut c, &corpus, seed);
    let lower = has_icu(&mut c);
    if !lower {
        eprintln!(
            "keyexpr_parity: no und-x-icu collation on this server; the Lower slot is not tried"
        );
    }
    let mut t = Tally::default();
    run_suite(&mut c, &rows, seed, lower, &mut t);
    let default_run = (t.comparisons, t.orderings, t.probes);
    c.batch_execute(
        "SET DateStyle = 'SQL, DMY'; SET TimeZone = 'Pacific/Kiritimati'; \
         SET standard_conforming_strings = off; SET extra_float_digits = -3; \
         SET search_path = pg_catalog, pg_temp; SET bytea_output = 'escape'",
    )
    .unwrap();
    run_suite(&mut c, &rows, seed ^ 1, lower, &mut t);
    eprintln!(
        "keyexpr_parity: {} stored documents, {} (document, slot) comparisons, {} order checks and {} \
         membership and contains probes in two sessions (first session: {default_run:?}); {} differences",
        rows.len(),
        t.comparisons,
        t.orderings,
        t.probes,
        t.diffs.len()
    );
    assert!(
        t.comparisons > 100_000,
        "the corpus is too small: {}",
        t.comparisons
    );
    assert!(
        t.diffs.is_empty(),
        "{} differences between the SQL slots and the in-memory evaluator:\n{}",
        t.diffs.len(),
        t.diffs.join("\n")
    );
}

/// The slot kinds line up with the field kinds (one function maps them).
#[test]
fn the_slots_cover_every_field_kind_the_parity_test_checks() {
    let kinds = [
        FieldKind::Text,
        FieldKind::Number,
        FieldKind::Bool,
        FieldKind::Date,
        FieldKind::Datetime,
        decimal_kind(),
        FieldKind::Select(options()),
    ];
    let slots: Vec<Option<Slot>> = kinds.iter().map(slot_of).collect();
    assert_eq!(
        slots,
        [
            Some(Slot::Str),
            Some(Slot::Int),
            Some(Slot::Bool),
            Some(Slot::Date),
            Some(Slot::Instant),
            Some(Slot::Dec),
            Some(Slot::SelPos)
        ]
    );
}

/// The harness is not blind: the three expressions that the designs first wrote (an integer
/// that is `trunc`-equal, so `5.0`; a date without a calendar; a decimal with twelve fractional
/// digits) and an instant that accepts any spelling each differ from the oracle somewhere.
#[test]
fn the_parity_check_finds_the_expressions_that_were_wrong_before() {
    let Some(mut c) = connect() else { return };
    let corpus = corpus(7, 400);
    let rows = load(&mut c, &corpus, 7);
    let v = "(data -> 'f')";
    let t = "(data ->> 'f')";
    let wrong: [(Slot, String); 4] = [
        (
            Slot::Int,
            format!(
                "(CASE WHEN jsonb_typeof({v}) = 'number' THEN (CASE WHEN {v}::numeric = \
                 trunc({v}::numeric) AND {v}::numeric BETWEEN -9223372036854775808 AND \
                 9223372036854775807 THEN ({v}::numeric)::bigint END) END)"
            ),
        ),
        (
            Slot::Date,
            format!(
                "(CASE WHEN jsonb_typeof({v}) = 'string' THEN (CASE WHEN {t} ~ \
                 '^[0-9]{{4}}-[0-9]{{2}}-[0-9]{{2}}$' THEN {t} END) END)"
            ),
        ),
        (
            Slot::Dec,
            format!(
                "(CASE WHEN jsonb_typeof({v}) = 'string' AND {t} ~ \
                 '^-?[0-9]{{1,38}}([.][0-9]{{1,12}})?$' THEN {t}::numeric END)"
            ),
        ),
        (
            Slot::Instant,
            format!("(CASE WHEN jsonb_typeof({v}) = 'string' THEN {t} END)"),
        ),
    ];
    for (slot, expr) in &wrong {
        let mut tally = Tally::default();
        compare_slot(&mut c, &rows, *slot, expr, &mut tally);
        assert!(
            !tally.diffs.is_empty(),
            "{slot:?}: the wrong expression was not caught in {} comparisons",
            tally.comparisons
        );
        eprintln!(
            "{slot:?}: the old expression differs from the oracle in {} places, e.g. {}",
            tally.diffs.len(),
            tally.diffs[0]
        );
    }
    // And the real ones are clean on the same rows.
    let mut tally = Tally::default();
    for (slot, _) in &wrong {
        compare_slot(&mut c, &rows, *slot, &expr_of(*slot), &mut tally);
    }
    assert!(tally.diffs.is_empty(), "{:?}", tally.diffs);
}
