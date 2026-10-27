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
//!   should use it match by text. A change to any expression text must bump [`EXPR_VERSION`]:
//!   the test `expression_texts_are_pinned` keeps a history of (version, digest) pairs and fails
//!   until the new digest is a new row with a higher version. An index built from the old text
//!   then gets a new name.
//! * No other source file of any crate under `crates/` may spell `jsonb_typeof(`, take a text out
//!   of a document with `->>`, `#>` or `#>>`, or call a `jsonb_extract_path` or `jsonb_path`
//!   function: the test `one_writer_lint` scans them all and lists, by path, the older code that
//!   still does.
//!
//! The functions are pure and know no connection. PostgreSQL is the only dialect (SQLite has no
//! pushdown and uses `json_extract` in its own file).
//!
//! Server requirements: PostgreSQL 14 or later for the casts (`jsonb` to `numeric` and
//! `boolean`), and, for the `Lower` slot only, a server built with ICU, which defines the
//! collation `und-x-icu` (the official images are). Where it is missing a query that uses
//! `Lower` fails as a whole with "collation does not exist"; the caller probes for it first.

use crate::Result;

/// The version of the expression texts. Index names hash it; bump it with any change of text
/// (and add the new digest to the history in the test `expression_texts_are_pinned`).
pub const EXPR_VERSION: u32 = 2;

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
    /// (the only form the stores write, D72), as `text` collated `"C"` (text order is time
    /// order).
    Instant,
    /// A JSON string that `obop_values::Decimal::parse` accepts, as `numeric` (80 characters,
    /// 38 significant digits, at most 38 fractional digits).
    Dec,
    /// The 1-based position of a JSON string in a bound option list, as `integer`; use
    /// [`select_pos`], which takes the list.
    SelPos,
    /// The tokens of a JSON string list as `text[]`, exactly the list [`member_tokens`] returns:
    /// the non-empty pieces between [`MEMBER_SEPARATORS`] (a leading or trailing separator leaves
    /// no empty element, so the id `''` is in no list). `NULL` for a value that is not a string;
    /// an empty string gives the empty array. For `@> ARRAY[$actor]::text[]` and a GIN index; the
    /// caller still refuses an empty actor id, as `Gate::grants` does.
    Members,
    /// The JSON string lower-cased by ICU (`und-x-icu`) as `text`, for `LIKE` and a trigram index.
    /// It needs a server built with ICU (see the module notes). It equals Rust's
    /// `str::to_lowercase`, which the in-memory evaluator uses, only while the Unicode version of
    /// the server's ICU and the one of the Rust standard library agree: a letter that gained a
    /// case mapping in a newer Unicode release is lower-cased by one side and left alone by the
    /// other. The parity test checks a corpus of long-stable characters, not the whole standard.
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
/// **Behaviour change (the member tokenizer of D78, written with the query pushdown D76):**
/// before this function `Gate::grants` split on four separators (`, ; space` and line feed) and
/// then trimmed every piece with Unicode `trim`, so `"ann\u{a0}"` listed `ann` and a tab, a
/// carriage return, a form feed or a vertical tab did not separate. Now eight ASCII separators
/// split and nothing is trimmed. User ids contain none of these characters; a list written with
/// tabs or CRLF now works, and one padded with Unicode spaces no longer names the person. Until
/// the decision record is written, this comment and `docs/MODULES.md` are where it is stated.
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
        // `regexp_split_to_array` leaves an empty element for a leading or trailing separator;
        // `member_tokens` has none, so neither may the array (an empty actor id must match
        // nothing). A NULL (not a string) stays NULL.
        Slot::Members => format!(
            "array_remove(regexp_split_to_array({raw}, {}), '')",
            member_class()
        ),
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
/// does. The pattern is a bound value, never part of the statement text. The text side is
/// lower-cased by the server's ICU ([`Slot::Lower`]), so the match equals the in-memory one only
/// while the Unicode versions of Rust and of that ICU agree.
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

    /// Every version of the expression texts that has been written, with the SHA-256 of all the
    /// texts of that version. Never edit a row. A change of any expression text gives a new
    /// digest, which fails this test until a new row with the next version is appended and
    /// `EXPR_VERSION` is raised to it: an index built from the old text then has another name
    /// (D78) and the old one shows as an orphan instead of being silently kept.
    const HISTORY: &[(u32, &str)] = &[
        // The first text, in which the member list kept empty tokens; no index was ever built.
        (
            1,
            "a1cb1e1eafa0cb4ab9760d538b70f9834e9d007d8499162951ebbbc9b266a3a1",
        ),
        // The member list drops empty tokens (`array_remove`).
        (
            2,
            "386aea455f613b66f53719348db876cb1c58821d8de9c4362b0ff9333d4a12c1",
        ),
    ];

    #[test]
    fn expression_texts_are_pinned() {
        let mut h = Sha256::new();
        for e in all_expressions() {
            h.update(e.as_bytes());
            h.update([0]);
        }
        h.update(like_pattern("a_b%c\\d").as_bytes());
        let digest: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
        for (i, (version, d)) in HISTORY.iter().enumerate() {
            assert_eq!(*version as usize, i + 1, "the versions are 1, 2, 3, ...");
            assert!(
                HISTORY[..i].iter().all(|(_, earlier)| earlier != d),
                "version {version} has the digest of an earlier version"
            );
        }
        let (latest, pinned) = *HISTORY.last().unwrap();
        assert_eq!(
            EXPR_VERSION, latest,
            "EXPR_VERSION must be the last version of HISTORY"
        );
        assert_eq!(
            digest,
            pinned,
            "an expression text changed: raise EXPR_VERSION to {} and append the row ({}, \"{digest}\") to HISTORY",
            EXPR_VERSION + 1,
            EXPR_VERSION + 1
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
        // Members: no empty element, so the id '' is in no list.
        let members = field_expr(Slot::Members, "f", None).unwrap();
        assert!(
            members.starts_with("array_remove(regexp_split_to_array(")
                && members.ends_with(", '')"),
            "{members}"
        );
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
        // before the tokenizer changed.
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

    /// Whole files that may spell the patterns: the writer itself, and the parity harness, which
    /// writes the expressions the designs first had, wrong on purpose. Paths are relative to
    /// `crates/`, with `/`.
    const ALLOWED_FILES: &[(&str, &str)] = &[
        (
            "kernel/obop-storage/src/keyexpr.rs",
            "the one writer (and the lint that spells the patterns)",
        ),
        (
            "kernel/obop-commands/src/keyexpr_parity.rs",
            "the parity harness writes the earlier wrong expressions on purpose",
        ),
    ];

    /// Older code that still takes a field out of a document itself, by path (relative to
    /// `crates/`) and a piece of one source line, compared with all white space removed and in
    /// lower case. The piece is cut out of the line and the rest is checked again, so a second
    /// read on the same line is still found. The test fails for any other line, and for an entry
    /// that matches nothing any more (delete the entry when its code is gone).
    const ALLOWED_LINES: &[(&str, &str, &str)] = &[
        (
            "kernel/obop-storage/src/schema.rs",
            "CHECK(jsonb_typeof(data)='object')",
            "the table constraint on the whole document, not a field value (the DDL of the base              part, and the text it was moved from, which a test compares it with)",
        ),
        (
            "kernel/obop-storage/src/pg.rs",
            "data->$2::text=$3",
            "Pg::find, converted to the slots by the index and keyset work (D78)",
        ),
        (
            "kernel/obop-storage/src/pg.rs",
            "NULLIF(data->>'{f}','')",
            "the D57 unique index; its text must stay as deployed databases hold it",
        ),
        (
            "kernel/obop-storage/src/pg.rs",
            "COALESCE(data->>'{f}','')",
            "the D57 unique index; its text must stay as deployed databases hold it",
        ),
        (
            "kernel/obop-storage/src/pg.rs",
            "data->>$4=$3",
            "swap_fields, a compare and swap of a text under a bound field name (key rotation)",
        ),
        (
            "kernel/obop-commands/src/tests.rs",
            "\"data->>'name'\"",
            "a hostile field name that the test proves is refused as an unknown field",
        ),
    ];

    /// Lower case, white space removed.
    const PATTERNS: [&str; 9] = [
        "jsonb_typeof(",
        "->>",
        "#>",
        "data->",
        // The arrow on a column or alias of another name, with a literal or a bound name after it
        // (a Rust return type is followed by a type, never by a quote or a placeholder).
        "->'",
        "->$",
        "jsonb_extract_path",
        "jsonb_path_",
        "json_extract_path",
    ];

    fn squeeze(line: &str) -> String {
        line.chars()
            .filter(|c| !c.is_whitespace())
            .flat_map(char::to_lowercase)
            .collect()
    }

    fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                if p.file_name().is_some_and(|n| n != "target") {
                    sources(&p, out);
                }
            } else if p.extension().is_some_and(|x| x == "rs" || x == "sql") {
                out.push(p);
            }
        }
    }

    /// The lines of `text` that spell a pattern, minus what an allowed line piece covers.
    fn offending(
        text: &str,
        allowed: &[(&str, &str, &str)],
        used: &mut [usize],
    ) -> Vec<(usize, String)> {
        let mut bad = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let mut rest = squeeze(line);
            if !PATTERNS.iter().any(|pat| rest.contains(pat)) {
                continue;
            }
            for (i, (_, piece, _)) in allowed.iter().enumerate() {
                let piece = squeeze(piece);
                if rest.contains(&piece) {
                    used[i] += 1;
                    rest = rest.replace(&piece, "");
                }
            }
            if PATTERNS.iter().any(|pat| rest.contains(pat)) {
                bad.push((n + 1, line.trim().to_string()));
            }
        }
        bad
    }

    #[test]
    fn one_writer_lint() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let crates = manifest.parent().and_then(Path::parent).unwrap();
        assert!(crates.ends_with("crates"), "{crates:?}");
        let mut files = Vec::new();
        sources(crates, &mut files);
        assert!(files.len() >= 100, "found {} sources", files.len());
        let rel = |p: &Path| {
            p.strip_prefix(crates)
                .unwrap()
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join("/")
        };
        for (file, why) in ALLOWED_FILES {
            assert!(
                files.iter().any(|p| rel(p) == *file),
                "the allowed file {file} ({why}) does not exist; fix the path"
            );
        }
        let mut used = vec![0usize; ALLOWED_LINES.len()];
        let mut bad = Vec::new();
        for p in &files {
            let path = rel(p);
            if ALLOWED_FILES.iter().any(|(f, _)| *f == path) {
                continue;
            }
            let mine: Vec<usize> = (0..ALLOWED_LINES.len())
                .filter(|i| ALLOWED_LINES[*i].0 == path)
                .collect();
            let here: Vec<(&str, &str, &str)> = mine.iter().map(|i| ALLOWED_LINES[*i]).collect();
            let mut counts = vec![0usize; here.len()];
            let text = std::fs::read_to_string(p).unwrap();
            for (n, line) in offending(&text, &here, &mut counts) {
                bad.push(format!("{path}:{n}: {line}"));
            }
            for (j, i) in mine.iter().enumerate() {
                used[*i] += counts[j];
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
                ALLOWED_LINES[i].1,
                ALLOWED_LINES[i].0
            );
        }
    }

    /// The lint cannot be satisfied by a file name, a letter case, another JSON operator or a
    /// second read on a line that already holds an allowed one.
    #[test]
    fn the_lint_finds_what_it_is_meant_to_find() {
        let allowed = [("a/pg.rs", "data->>$4=$3", "swap")];
        let check = |text: &str| {
            let mut used = [0usize];
            (offending(text, &allowed, &mut used), used[0])
        };
        let (bad, used) = check("WHERE id = $2 AND data->>$4 = $3");
        assert!(bad.is_empty() && used == 1);
        for sneaky in [
            "WHERE id = $2 AND data->>$4 = $3 AND data ->> 'x' = 'y'",
            "WHERE JSONB_TYPEOF(d.data) = 'string'",
            "WHERE data #>> '{f}' = $1",
            "WHERE data #> '{f}' IS NOT NULL",
            "WHERE jsonb_extract_path_text(data, 'f') = $1",
            "WHERE jsonb_path_exists(data, '$.f')",
            "WHERE (r.data)->'f' = $1",
            "SELECT 1 FROM t WHERE x ->> 'f' = $1",
        ] {
            let (bad, _) = check(sneaky);
            assert_eq!(bad.len(), 1, "{sneaky}");
        }
        let (bad, used) = check("let x = 1; // nothing to see");
        assert!(bad.is_empty() && used == 0);
    }
}
