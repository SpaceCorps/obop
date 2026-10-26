//! The one writer of the typed SQL expressions for a field of a record stored as a JSONB document
//! in `obop_record` (D76 query pushdown, D78 declared indexes).
//!
//! A record is `obop_record(seq, model, id, data jsonb)`, and `data` may hold any JSON for any
//! field: data that bypassed validation, an old value of a retyped field, a float where an
//! integer belongs. The in-memory evaluator (`obop-commands` `query.rs`, `stored_key`) treats a
//! value that is not valid for the field's kind as *empty*. Every expression here is `NULL`
//! exactly where `stored_key` returns `None`, and equals its key everywhere else; no cast can
//! raise on any JSON value. The test `keyexpr_parity` of `obop-commands` proves it against a
//! PostgreSQL server over thousands of hostile stored values.
//!
//! Rules that keep it true:
//!
//! * Guards are nested `CASE`s, never `AND`: PostgreSQL gives no evaluation order for `AND`, so a
//!   cast next to its guard in an `AND` may run first and raise.
//! * Names reach the text only as validated literals (`[a-z0-9_]{1,64}`, [`crate::ident`]); values
//!   never do (they are bound by the caller with an explicit cast).
//! * One expression per slot and field, byte for byte: an expression index and the query that
//!   should use it match by text. A change to any expression text must bump [`EXPR_VERSION`]
//!   (the test `expression_texts_are_pinned` fails until it does), which gives every index built
//!   from the old text a new name.
//! * No other file of this crate may spell `jsonb_typeof(` or take a text out of a document with
//!   `->>`: the test `one_writer_lint` scans the sources and lists the older code that still does.
//!
//! The functions are pure and know no connection. PostgreSQL is the only dialect (SQLite has no
//! pushdown and uses `json_extract` in its own file).

use crate::Result;

/// The version of the expression texts. Index names hash it; bump it with any change of text.
pub const EXPR_VERSION: u32 = 1;

/// What a field's value is read as. `NULL` means "empty" in every slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    /// A JSON string as `text`, collated `"C"` (byte order): text, long text, ref. Equality,
    /// ranges, order and the index all use this one collated expression.
    Str,
    /// The same string without a collation, for the caller that applies its own (an ICU order
    /// with `COLLATE obop_l1_sv`, or `lower`).
    Raw,
    /// A JSON integer that fits an `i64` as `bigint` (number, money). `5.0`, `1.5` and
    /// `9223372036854775808` are `NULL`, as `serde_json::Number::as_i64` says.
    Int,
    /// A JSON boolean as `boolean`.
    Bool,
    /// A JSON string `YYYY-MM-DD` that is a real calendar date, as `text` collated `"C"` (fixed
    /// width, so text order is date order).
    Date,
    /// A JSON string in the canonical instant form `YYYY-MM-DDTHH:MM:SS.ffffffZ` and nothing else
    /// (D72 amendment), as `text` collated `"C"` (text order is time order).
    Instant,
    /// A JSON string that `obop_values::Decimal::parse` accepts, as `numeric` (80 characters,
    /// 38 significant digits, at most 38 fractional digits).
    Dec,
    /// The 1-based position of a JSON string in a bound option list, as `integer`; use
    /// [`select_pos`], which takes the list.
    SelPos,
    /// The tokens of a JSON string list as `text[]` (see [`MEMBER_SEPARATORS`]), for
    /// `@> ARRAY[$actor]::text[]` and a GIN index.
    Members,
    /// The JSON string lower-cased by ICU (`und-x-icu`) as `text`, for `LIKE` and a trigram index.
    Lower,
    /// The record id, `text` collated `"C"`. The field must be named `id`.
    Id,
}

impl Slot {
    /// Every slot.
    pub const ALL: [Slot; 11] = [
        Slot::Str,
        Slot::Raw,
        Slot::Int,
        Slot::Bool,
        Slot::Date,
        Slot::Instant,
        Slot::Dec,
        Slot::SelPos,
        Slot::Members,
        Slot::Lower,
        Slot::Id,
    ];

    /// A stable lower-case name (used in index identities).
    pub fn name(self) -> &'static str {
        match self {
            Slot::Str => "str",
            Slot::Raw => "raw",
            Slot::Int => "int",
            Slot::Bool => "bool",
            Slot::Date => "date",
            Slot::Instant => "instant",
            Slot::Dec => "dec",
            Slot::SelPos => "selpos",
            Slot::Members => "members",
            Slot::Lower => "lower",
            Slot::Id => "id",
        }
    }
}

// ---- members ------------------------------------------------------------------------------

/// The characters that separate the user ids of a `shared_with` list: comma, semicolon, space,
/// tab, line feed, carriage return, form feed and vertical tab. One constant for Rust
/// ([`member_tokens`], used by the row rule `Gate::grants`) and for SQL ([`Slot::Members`]), so
/// the two cannot drift.
pub const MEMBER_SEPARATORS: [char; 8] = [',', ';', ' ', '\t', '\n', '\r', '\u{c}', '\u{b}'];

/// The user ids in a `shared_with` list: the non-empty pieces between separators, with nothing
/// trimmed (a non-breaking space or an em space is part of a token, not a separator).
///
/// **Behaviour change (K5, D76):** before this function `Gate::grants` split on four separators
/// (`, ; space` and line feed) and then trimmed every piece with Unicode `trim`, so
/// `"ann\u{a0}"` listed `ann` and a tab, a carriage return, a form feed or a vertical tab did
/// not separate. Now eight ASCII separators split and nothing is trimmed. User ids contain none
/// of these characters; a list written with tabs or CRLF now works, and one padded with Unicode
/// spaces no longer names the person.
pub fn member_tokens(list: &str) -> Vec<&str> {
    list.split(MEMBER_SEPARATORS)
        .filter(|t| !t.is_empty())
        .collect()
}

/// The separators as the body of an `E'[...]+'` regular expression: every control character as a
/// hex escape (the text then holds no control character and does not depend on
/// `standard_conforming_strings`).
fn member_class() -> String {
    let mut s = String::from("E'[");
    for c in MEMBER_SEPARATORS {
        if c.is_ascii_graphic() || c == ' ' {
            s.push(c);
        } else {
            s.push_str(&format!("\\x{:02x}", c as u32));
        }
    }
    s.push_str("]+'");
    s
}

// ---- expressions --------------------------------------------------------------------------

const RE_DATE: &str = "'^[0-9]{4}-[0-9]{2}-[0-9]{2}$'";
const RE_INSTANT: &str = "'^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}[.][0-9]{6}Z$'";
const RE_DEC: &str = "'^-?[0-9]+([.][0-9]+)?$'";
/// `Decimal::parse` takes at most this many characters.
const DEC_CHARS: u32 = 80;
/// `Decimal::parse` takes at most this many significant digits, and this many fractional digits.
const DEC_DIGITS: u32 = 38;

/// `p.` for an alias `p`, nothing for none.
fn prefix(alias: Option<&str>) -> Result<String> {
    match alias {
        None => Ok(String::new()),
        Some(a) => Ok(format!("{}.", crate::ident(a)?)),
    }
}

/// `y-m-d` of the text `t` (its first ten characters, already known to be `dddd-dd-dd`) is a
/// real calendar date, year 1 or later: the mirror of `is_valid_date`. Nothing in it can raise:
/// the digits are guaranteed by the caller's regular expression.
fn calendar(t: &str) -> String {
    let y = format!("substr({t}, 1, 4)::int");
    let m = format!("substr({t}, 6, 2)::int");
    let d = format!("substr({t}, 9, 2)::int");
    let leap = format!("(({y} % 4 = 0 AND {y} % 100 <> 0) OR {y} % 400 = 0)");
    let days = format!(
        "(CASE {m} WHEN 2 THEN (CASE WHEN {leap} THEN 29 ELSE 28 END) \
         WHEN 4 THEN 30 WHEN 6 THEN 30 WHEN 9 THEN 30 WHEN 11 THEN 30 ELSE 31 END)"
    );
    format!("({y} >= 1 AND {m} BETWEEN 1 AND 12 AND {d} BETWEEN 1 AND {days})")
}

/// The id column: `text` collated `"C"`, the order of `str::cmp`. An index on it is
/// `(model, (id COLLATE "C"))`.
pub fn id_expr(alias: Option<&str>) -> Result<String> {
    Ok(format!("({}id COLLATE \"C\")", prefix(alias)?))
}

/// The id column as it is, in the collation of the database: for a probe of the primary key
/// (`model = 'm' AND id = $1`), where equality does not depend on the collation but the index
/// does. Not for order or ranges.
pub fn id_pk(alias: Option<&str>) -> Result<String> {
    Ok(format!("{}id", prefix(alias)?))
}

/// The SQL expression of `slot` over `field` of the document `data` (of the table `alias`, when
/// given). `Slot::SelPos` has its own function ([`select_pos`]); `Slot::Id` takes the field
/// `id`. The result is parenthesised, so it can be put in any position, including an index.
pub fn field_expr(slot: Slot, field: &str, alias: Option<&str>) -> Result<String> {
    match slot {
        Slot::Id => {
            return if field == "id" {
                id_expr(alias)
            } else {
                Err("the id slot reads the field named id".into())
            };
        }
        Slot::SelPos => {
            return Err("a select position needs its option list: use select_pos".into());
        }
        _ => {}
    }
    let f = crate::ident(field)?;
    let a = prefix(alias)?;
    let v = format!("({a}data -> '{f}')");
    let t = format!("({a}data ->> '{f}')");
    let string = |then: &str| format!("CASE WHEN jsonb_typeof({v}) = 'string' THEN {then} END");
    let raw = format!("({})", string(&t));
    Ok(match slot {
        Slot::Raw => raw,
        Slot::Str => format!("({raw} COLLATE \"C\")"),
        Slot::Int => format!(
            "(CASE WHEN jsonb_typeof({v}) = 'number' THEN (CASE WHEN scale({v}::numeric) = 0 \
             AND {v}::numeric BETWEEN -9223372036854775808 AND 9223372036854775807 \
             THEN ({v}::numeric)::bigint END) END)"
        ),
        Slot::Bool => format!("(CASE WHEN jsonb_typeof({v}) = 'boolean' THEN {v}::boolean END)"),
        Slot::Date => {
            let cal = calendar(&t);
            let inner =
                format!("CASE WHEN {t} ~ {RE_DATE} THEN (CASE WHEN {cal} THEN {t} END) END");
            format!("(({}) COLLATE \"C\")", string(&format!("({inner})")))
        }
        Slot::Instant => {
            let cal = calendar(&t);
            let clock = format!(
                "substr({t}, 12, 2)::int <= 23 AND substr({t}, 15, 2)::int <= 59 \
                 AND substr({t}, 18, 2)::int <= 59"
            );
            let inner = format!(
                "CASE WHEN {t} ~ {RE_INSTANT} THEN (CASE WHEN {cal} AND {clock} THEN {t} END) END"
            );
            format!("(({}) COLLATE \"C\")", string(&format!("({inner})")))
        }
        Slot::Dec => {
            let significant = format!(
                "char_length(replace(regexp_replace({t}, '^-?[0.]*', ''), '.', '')) <= {DEC_DIGITS}"
            );
            let fractional = format!("char_length(split_part({t}, '.', 2)) <= {DEC_DIGITS}");
            let inner = format!(
                "CASE WHEN char_length({t}) <= {DEC_CHARS} AND {t} ~ {RE_DEC} THEN \
                 (CASE WHEN {significant} AND {fractional} THEN {t}::numeric END) END"
            );
            format!("({})", string(&format!("({inner})")))
        }
        Slot::Members => format!("regexp_split_to_array({raw}, {})", member_class()),
        Slot::Lower => format!("lower({raw} COLLATE \"und-x-icu\")"),
        Slot::Id | Slot::SelPos => unreachable!("handled above"),
    })
}

/// The 1-based position of the string in `field` among the options, `NULL` when it is not one
/// (or not a string). `options` is the SQL of the bound option list and must be exactly a
/// placeholder with its cast, `$3::text[]`.
pub fn select_pos(field: &str, alias: Option<&str>, options: &str) -> Result<String> {
    let ok = options
        .strip_prefix('$')
        .and_then(|r| r.strip_suffix("::text[]"))
        .is_some_and(|n| !n.is_empty() && n.len() <= 5 && n.bytes().all(|b| b.is_ascii_digit()));
    if !ok {
        return Err("the option list must be a bound parameter such as $3::text[]".into());
    }
    let raw = field_expr(Slot::Raw, field, alias)?;
    Ok(format!("array_position({options}, {raw})"))
}

/// The `LIKE` pattern for "contains `needle`" over a [`Slot::Lower`] expression: `%`, the
/// needle with `\`, `%` and `_` escaped by a backslash (the default `LIKE` escape), `%`. The
/// caller lower-cases the needle first with Rust `to_lowercase`, as the in-memory evaluator
/// does. The pattern is a bound value, never part of the statement text.
pub fn like_pattern(needle_lowercase: &str) -> String {
    let mut p = String::with_capacity(needle_lowercase.len() + 2);
    p.push('%');
    for c in needle_lowercase.chars() {
        if matches!(c, '\\' | '%' | '_') {
            p.push('\\');
        }
        p.push(c);
    }
    p.push('%');
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::path::{Path, PathBuf};

    fn all_expressions() -> Vec<String> {
        let mut out = Vec::new();
        for alias in [None, Some("r")] {
            for slot in Slot::ALL {
                let field = if slot == Slot::Id { "id" } else { "f" };
                if slot != Slot::SelPos {
                    out.push(field_expr(slot, field, alias).unwrap());
                }
            }
            out.push(select_pos("f", alias, "$3::text[]").unwrap());
            out.push(id_pk(alias).unwrap());
        }
        out
    }

    /// The SHA-256 of every expression text. Any change of text fails this test until
    /// `EXPR_VERSION` is bumped and the digest below is replaced: an index built from the old
    /// text then gets a new name (D78) and the old one shows as an orphan.
    #[test]
    fn expression_texts_are_pinned() {
        let mut h = Sha256::new();
        for e in all_expressions() {
            h.update(e.as_bytes());
            h.update([0]);
        }
        h.update(like_pattern("a_b%c\\d").as_bytes());
        let digest: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            (EXPR_VERSION, digest.as_str()),
            (
                1,
                "a1cb1e1eafa0cb4ab9760d538b70f9834e9d007d8499162951ebbbc9b266a3a1"
            ),
            "an expression text changed: bump EXPR_VERSION and replace the digest with {digest}"
        );
    }

    #[test]
    fn expressions_are_golden_for_the_simple_slots() {
        assert_eq!(
            field_expr(Slot::Raw, "name", Some("r")).unwrap(),
            "(CASE WHEN jsonb_typeof((r.data -> 'name')) = 'string' THEN (r.data ->> 'name') END)"
        );
        assert_eq!(
            field_expr(Slot::Str, "name", None).unwrap(),
            "((CASE WHEN jsonb_typeof((data -> 'name')) = 'string' THEN (data ->> 'name') END) \
             COLLATE \"C\")"
        );
        assert_eq!(
            field_expr(Slot::Bool, "open", None).unwrap(),
            "(CASE WHEN jsonb_typeof((data -> 'open')) = 'boolean' THEN (data -> 'open')::boolean END)"
        );
        assert_eq!(
            field_expr(Slot::Id, "id", Some("r")).unwrap(),
            "(r.id COLLATE \"C\")"
        );
        assert_eq!(id_pk(Some("p1")).unwrap(), "p1.id");
        assert_eq!(
            select_pos("state", Some("r"), "$4::text[]").unwrap(),
            "array_position($4::text[], (CASE WHEN jsonb_typeof((r.data -> 'state')) = 'string' \
             THEN (r.data ->> 'state') END))"
        );
        // Integers: 5.0 is not one, so the guard is the scale, not a truncation.
        let int = field_expr(Slot::Int, "n", None).unwrap();
        assert!(int.contains("scale((data -> 'n')::numeric) = 0"), "{int}");
        assert!(!int.contains("trunc"), "{int}");
    }

    #[test]
    fn names_and_aliases_are_validated_and_values_never_enter() {
        let long = "x".repeat(65);
        for bad in [
            "",
            "A",
            "a b",
            "a'b",
            "a\"b",
            "a;b",
            "a-b",
            "é",
            "a\0",
            "a\n",
            &long,
            "data -> 'x'",
            "a) OR (1=1",
        ] {
            for slot in [Slot::Str, Slot::Int, Slot::Dec, Slot::Members] {
                assert!(field_expr(slot, bad, None).is_err(), "{bad:?}");
            }
            assert!(
                field_expr(Slot::Str, "f", Some(bad)).is_err(),
                "alias {bad:?}"
            );
            assert!(select_pos(bad, None, "$1::text[]").is_err(), "{bad:?}");
        }
        assert!(field_expr(Slot::Id, "other", None).is_err());
        assert!(field_expr(Slot::SelPos, "f", None).is_err());
        for bad in [
            "",
            "$1",
            "$::text[]",
            "$1::text",
            "ARRAY['a']::text[]",
            "$1::text[]; --",
            "$1::text[] ",
            "$123456::text[]",
            "1::text[]",
            "$1::TEXT[]",
        ] {
            assert!(select_pos("f", None, bad).is_err(), "{bad:?}");
        }
        assert!(select_pos("f", None, "$12::text[]").is_ok());
        for e in all_expressions() {
            // The semicolon is a member separator inside one string literal, nowhere else.
            let e = e.replace(&member_class(), "");
            assert!(!e.contains(';') && !e.contains("--"), "{e}");
        }
    }

    #[test]
    fn the_member_class_is_the_constant() {
        // Decode the E-string body back and compare it with MEMBER_SEPARATORS.
        let class = member_class();
        let body = class
            .strip_prefix("E'[")
            .and_then(|b| b.strip_suffix("]+'"))
            .unwrap();
        let mut decoded = Vec::new();
        let mut it = body.chars().peekable();
        while let Some(c) = it.next() {
            if c == '\\' {
                assert_eq!(it.next(), Some('x'));
                let hex: String = [it.next().unwrap(), it.next().unwrap()].iter().collect();
                decoded.push(char::from(u8::from_str_radix(&hex, 16).unwrap()));
            } else {
                decoded.push(c);
            }
        }
        assert_eq!(decoded, MEMBER_SEPARATORS);
        assert!(class.is_ascii() && !class.chars().any(|c| c.is_control()));
    }

    #[test]
    fn member_tokens_split_on_eight_ascii_separators_and_trim_nothing() {
        let t = member_tokens;
        assert_eq!(t("ann"), ["ann"]);
        assert_eq!(t("ann,bob"), ["ann", "bob"]);
        assert_eq!(t("ann, bob;cy dee"), ["ann", "bob", "cy", "dee"]);
        // All eight separators, in a row and mixed.
        assert_eq!(
            t("a,b;c d\te\nf\rg\u{c}h\u{b}i"),
            ["a", "b", "c", "d", "e", "f", "g", "h", "i"]
        );
        assert_eq!(t("a,,;; \t\r\n b"), ["a", "b"]);
        assert_eq!(t("\r\nann\r\nbob\r\n"), ["ann", "bob"]);
        // No tokens.
        for empty in ["", ",", " ", ",;, \t\n\r\u{c}\u{b}"] {
            assert!(t(empty).is_empty(), "{empty:?}");
        }
        // Nothing is trimmed and no Unicode space separates: these were separators or trimmed
        // before K5.
        assert_eq!(t("ann\u{a0}"), ["ann\u{a0}"]);
        assert_eq!(t("\u{2003}ann"), ["\u{2003}ann"]);
        assert_eq!(t("a\u{a0}b"), ["a\u{a0}b"]);
        assert_eq!(t("a\u{85}b\u{2028}c"), ["a\u{85}b\u{2028}c"]);
        // Unicode ids, punctuation and case are kept as they are.
        assert_eq!(
            t("Åsa,o'brien;a.b*c 100%"),
            ["Åsa", "o'brien", "a.b*c", "100%"]
        );
        assert_eq!(t("Ann,ann"), ["Ann", "ann"]);
        // An id that holds a separator can never be a token.
        assert!(!t("a b").contains(&"a b"));
    }

    #[test]
    fn like_patterns_escape_the_three_special_characters() {
        assert_eq!(like_pattern(""), "%%");
        assert_eq!(like_pattern("abc"), "%abc%");
        assert_eq!(like_pattern("100%"), "%100\\%%");
        assert_eq!(like_pattern("a_b"), "%a\\_b%");
        assert_eq!(like_pattern("a\\b"), "%a\\\\b%");
        assert_eq!(like_pattern("ä'\""), "%ä'\"%");
    }

    // ---- the one-writer lint ----------------------------------------------------------------

    /// Older code that still takes a field out of a document itself. Each entry is a file and a
    /// piece of a source line with all white space removed, and says who converts it. The test
    /// fails when a line outside this list (or outside this file) spells one of the patterns,
    /// and when an entry no longer matches anything (delete the entry when its code is gone).
    const ALLOWED: &[(&str, &str, &str)] = &[
        (
            "pg.rs",
            "CHECK(jsonb_typeof(data)='object')",
            "the table constraint on the whole document, not a field value",
        ),
        (
            "pg.rs",
            "data->$2::text=$3",
            "Pg::find, converted to the slots by the index and keyset work (D78, K6)",
        ),
        (
            "pg.rs",
            "NULLIF(data->>'{f}','')",
            "the D57 unique index; its text must stay as deployed databases hold it",
        ),
        (
            "pg.rs",
            "COALESCE(data->>'{f}','')",
            "the D57 unique index; its text must stay as deployed databases hold it",
        ),
        (
            "pg.rs",
            "data->>$4=$3",
            "swap_fields, a compare and swap of a text under a bound field name (key rotation)",
        ),
    ];

    const PATTERNS: [&str; 3] = ["jsonb_typeof(", "->>", "data->"];

    fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                sources(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }

    #[test]
    fn one_writer_lint() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        sources(&src, &mut files);
        assert!(files.len() >= 10, "found {} sources", files.len());
        let mut used = vec![0usize; ALLOWED.len()];
        let mut bad = Vec::new();
        for p in files {
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            if name == "keyexpr.rs" {
                continue;
            }
            let text = std::fs::read_to_string(&p).unwrap();
            for (n, line) in text.lines().enumerate() {
                let squeezed: String = line.chars().filter(|c| !c.is_whitespace()).collect();
                if !PATTERNS.iter().any(|pat| squeezed.contains(pat)) {
                    continue;
                }
                let mut allowed = false;
                for (i, (file, piece, _)) in ALLOWED.iter().enumerate() {
                    if *file == name && squeezed.contains(piece) {
                        used[i] += 1;
                        allowed = true;
                    }
                }
                if !allowed {
                    bad.push(format!("{name}:{}: {}", n + 1, line.trim()));
                }
            }
        }
        assert!(
            bad.is_empty(),
            "SQL that reads a field out of a document must come from keyexpr.rs:\n{}",
            bad.join("\n")
        );
        for (i, n) in used.iter().enumerate() {
            assert!(
                *n > 0,
                "the allowed exception {:?} in {} matches nothing any more; delete it",
                ALLOWED[i].1,
                ALLOWED[i].0
            );
        }
    }
}
