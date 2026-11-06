//! The scan that keeps `obop_registry::INTERNAL_TABLES` complete. A table is named by a model: a
//! package that declares a model `auth_user` (the manifest rule lets the package `auth` do so)
//! would get the generated create, update, delete, list and search over the table that holds the
//! accounts and their password hashes. So no package may declare the name of a table that the
//! program's own crates read or write by name, and the installer reserves `INTERNAL_TABLES`. A
//! table that is missing from that list is a way in, and nothing else would notice it, so this test
//! reads the non-test Rust code of every crate under `crates/` (see `src_scan.rs`: comments are
//! blanked, test code and the files of test modules are left out) and fails when it finds the name
//! of a table that is on no list.
//!
//! **What it catches.** A string literal that names a table in one of three ways:
//! - the value of a constant with one of the words `TABLE`, `MODEL` or `COLLECTION` in its name
//!   (`const PASSKEY_TABLE: &str = "auth_passkey"`);
//! - the first argument of a call of a method of the storage `Backend` that takes a model
//!   (`.get("auth_user", id)`; the number of arguments is checked, so `value.get("key")` and
//!   `map.insert("key", v)` are not taken for it);
//! - the value of a field called `model` or `table` (`Op::Insert { model: "x".into(), .. }`).
//!
//! Each name found must be in `INTERNAL_TABLES`, in `RESERVED_MODELS`, or in [`MODULE_MODELS`]
//! below. A name of `INTERNAL_TABLES` or `MODULE_MODELS` that no code spells any more is a failure
//! too, so the lists cannot go stale.
//!
//! **What it does not see.** It is a textual scan, not a parser. A table named by text built at run
//! time (`format!("{}_user", "auth")`), one reached through a constant of another crate that is not
//! spelled in the file, and a method that is not in [`METHODS`] are not seen. It makes a new table
//! visible; it does not stop one who works around it.

use crate::src_scan::{
    Lexed, is_ident, line_of, non_test, read_tree, skip_ws, tree, word_at, words,
};
use obop_registry::{INTERNAL_TABLES, RESERVED_MODELS};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The methods of `Backend` whose first argument is a model, and how many arguments each takes (at
/// most: `scan` takes a closure, and the commas of its parameters count here). The count tells the
/// call from a method of the same name on something else (`Value::get("key")`,
/// `Map::insert("key", value)`, a closure `list("name", "description")`).
const METHODS: [(&str, usize, usize); 9] = [
    ("insert", 3, 3),
    ("get", 2, 2),
    ("list", 1, 1),
    ("scan", 2, 4),
    ("update", 3, 3),
    ("count", 1, 1),
    ("delete", 2, 2),
    ("find", 3, 3),
    ("find_unique", 3, 3),
];

/// The words that make a constant a table name.
const CONST_WORDS: [&str; 3] = ["TABLE", "MODEL", "COLLECTION"];

/// The models of the built-in modules and of the bundled packages that a crate of the program also
/// spells (the accounts import reads `accounting_move`, the shop reads `ecommerce_listing`, the
/// signing page reads `sign_request`). A module owns each of them, and the installer refuses a
/// package that declares one: while the module is registered the dispatcher knows the name and the
/// owner, so these are not in `INTERNAL_TABLES`. Adding a name here is the decision that it is a
/// module's model and not a table of the program. (`instance_brand` is the model of the module
/// `platform`, which the installer is given; it is here and not in `INTERNAL_TABLES` because the
/// scan of `brand_scan_tests` lets only the files it lists spell the name.)
const MODULE_MODELS: [&str; 20] = [
    "accounting_account",
    "accounting_fiscal_year",
    "accounting_journal",
    "accounting_move",
    "accounting_move_line",
    "accounting_se_company",
    "connection",
    "documents_document",
    "ecommerce_discount_code",
    "ecommerce_discount_use",
    "ecommerce_listing",
    "ecommerce_order",
    "ecommerce_order_line",
    "instance_brand",
    "party",
    "provider_policy",
    "sign_link",
    "sign_request",
    "website_form_submission",
    "website_page",
];

/// One place that names a table: the file, the line, the name and how it was named.
struct Hit {
    path: String,
    line: usize,
    name: String,
    how: &'static str,
}

fn is_table_name(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_lowercase())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// The identifier that ends right before `at` (after the whitespace in front of it).
fn ident_before(code: &[u8], at: usize) -> &str {
    let mut end = at;
    while end > 0 && code[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && is_ident(code[start - 1]) {
        start -= 1;
    }
    std::str::from_utf8(&code[start..end]).unwrap_or("")
}

/// The byte before `at`, past whitespace, and where it is.
fn byte_before(code: &[u8], at: usize) -> Option<(usize, u8)> {
    code[..at]
        .iter()
        .rposition(|c| !c.is_ascii_whitespace())
        .map(|p| (p, code[p]))
}

/// The number of arguments of the call whose first argument, a string literal, ends at `after`. A
/// literal is blanked in the code, so `starts` (where each one begins) tells it from blank space.
/// A trailing comma does not make another argument.
fn arguments(code: &[u8], starts: &BTreeSet<usize>, after: usize) -> usize {
    let (mut depth, mut count, mut comma) = (0_usize, 1_usize, false);
    for (at, &c) in code.iter().enumerate().skip(after) {
        let literal = starts.contains(&at);
        match c {
            b')' | b']' | b'}' if depth == 0 => break,
            b',' if depth == 0 => {
                comma = true;
                continue;
            }
            c if c.is_ascii_whitespace() && !literal => continue,
            _ => {}
        }
        if comma {
            count += 1;
            comma = false;
        }
        match c {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            _ => {}
        }
    }
    count
}

/// `const NAME: &str = "x";` with `TABLE`, `MODEL` or `COLLECTION` among the words of the name.
fn constants(path: &str, l: &Lexed, out: &mut Vec<Hit>) {
    let by_start: BTreeMap<usize, &String> = l.strings.iter().map(|(s, _, t)| (*s, t)).collect();
    for at in words(&l.code, "const") {
        let name_at = skip_ws(&l.code, at + "const".len());
        let name = word_at(&l.code, name_at);
        if !name.split('_').any(|w| CONST_WORDS.contains(&w)) {
            continue;
        }
        // `: &str =` or `: &'static str =`, then the literal.
        let Some(eq) = l.code[name_at..]
            .iter()
            .position(|c| matches!(c, b'=' | b';'))
            .map(|p| name_at + p)
        else {
            continue;
        };
        if l.code[eq] != b'=' || !String::from_utf8_lossy(&l.code[name_at..eq]).contains("str") {
            continue;
        }
        // The literal is blank in the code: the next thing after `=` is it, or something else.
        let value = (eq + 1..l.code.len())
            .find(|i| by_start.contains_key(i) || !l.code[*i].is_ascii_whitespace());
        if let Some(text) = value.and_then(|i| by_start.get(&i))
            && is_table_name(text)
        {
            out.push(Hit {
                path: path.to_string(),
                line: line_of(&l.code, eq),
                name: (*text).clone(),
                how: "the value of a constant that names a table or a model",
            });
        }
    }
}

fn hits(files: &BTreeMap<String, String>) -> Vec<Hit> {
    let lexed = non_test(files);
    let mut out = Vec::new();
    for (path, l) in &lexed {
        constants(path, l, &mut out);
        let starts: BTreeSet<usize> = l.strings.iter().map(|(s, _, _)| *s).collect();
        for (start, end, text) in &l.strings {
            if !is_table_name(text) {
                continue;
            }
            let how = match byte_before(&l.code, *start) {
                // `.get("x", ..)`: the first argument of a method of the backend.
                Some((p, b'('))
                    if METHODS.iter().any(|(m, least, most)| {
                        *m == ident_before(&l.code, p)
                            && (*least..=*most).contains(&arguments(&l.code, &starts, *end))
                    }) =>
                {
                    Some("the first argument of a call of the storage backend")
                }
                // `model: "x".into()`: the field of an operation.
                Some((p, b':'))
                    if matches!(ident_before(&l.code, p), "model" | "table")
                        && l.code.get(p.wrapping_sub(1)) != Some(&b':') =>
                {
                    Some("the value of a field `model` or `table`")
                }
                _ => None,
            };
            if let Some(how) = how {
                out.push(Hit {
                    path: path.clone(),
                    line: line_of(&l.code, *start),
                    name: text.clone(),
                    how,
                });
            }
        }
    }
    out
}

/// The places that name a table nobody has accounted for, and the names of the lists that no code
/// spells.
fn check(files: &BTreeMap<String, String>) -> (Vec<String>, Vec<String>) {
    let known: BTreeSet<&str> = INTERNAL_TABLES
        .iter()
        .chain(RESERVED_MODELS.iter())
        .chain(MODULE_MODELS.iter())
        .copied()
        .collect();
    let found = hits(files);
    let used: BTreeSet<&str> = found.iter().map(|h| h.name.as_str()).collect();
    let unreserved = found
        .iter()
        .filter(|h| !known.contains(h.name.as_str()))
        .map(|h| format!("{}:{}: `{}` as {}", h.path, h.line, h.name, h.how))
        .collect();
    let stale = INTERNAL_TABLES
        .iter()
        .chain(MODULE_MODELS.iter())
        .filter(|n| !used.contains(**n))
        .map(|n| (*n).to_string())
        .collect();
    (unreserved, stale)
}

#[test]
fn every_table_the_program_names_is_reserved_or_a_model_of_a_module() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = BTreeMap::new();
    read_tree(&root, &root, &mut files);
    let (unreserved, stale) = check(&files);
    assert!(
        unreserved.is_empty(),
        "the program names a table that no package is kept from declaring. Add it to \
         obop_registry::INTERNAL_TABLES (a table of the program), or to MODULE_MODELS here only if \
         it is a model of a built-in module or a bundled package:\n{}",
        unreserved.join("\n")
    );
    assert!(
        stale.is_empty(),
        "names of INTERNAL_TABLES or MODULE_MODELS that no code spells any more (the scan reads \
         the wrong thing, or the row is stale):\n{}",
        stale.join("\n")
    );
}

/// The three ways a table is named, and the near misses that are not.
#[test]
fn the_scan_sees_the_ways_the_code_can_name_a_table_and_leaves_the_rest_alone() {
    let names = |src: &str| -> Vec<String> {
        hits(&tree(&[("kernel/obop-x/src/lib.rs", src)]))
            .into_iter()
            .map(|h| h.name)
            .collect()
    };
    for (src, want) in [
        // A call of the backend, whatever the receiver, whichever method.
        (
            r#"fn f(b: &dyn Backend) { b.get("auth_user", id); }"#,
            "auth_user",
        ),
        (
            r#"fn f() { self.backend.list("x_log").unwrap(); }"#,
            "x_log",
        ),
        (r#"fn f() { s.count("x_count"); }"#, "x_count"),
        (r#"fn f() { s.update("x_up", id, &patch); }"#, "x_up"),
        (r#"fn f() { s.insert("x_in", id, &f); }"#, "x_in"),
        (r#"fn f() { s.delete("x_del", id); }"#, "x_del"),
        (r#"fn f() { s.find("x_find", "field", &v); }"#, "x_find"),
        (r#"fn f() { s.find_unique("x_uq", &["a"], &[v]); }"#, "x_uq"),
        (
            r#"fn f() { s.scan("x_scan", &mut |a, b| Ok(true)); }"#,
            "x_scan",
        ),
        (
            "fn f() { b\n    .get(\n        \"x_wrapped\",\n        id,\n    ); }",
            "x_wrapped",
        ),
        // The field of an operation.
        (
            r#"fn f() { Op::Insert { model: "x_op".into(), id, fields } }"#,
            "x_op",
        ),
        (
            r#"fn f() { let s = Sealed { table: "x_sealed", field: "f" }; }"#,
            "x_sealed",
        ),
        // A constant.
        (r#"const LOG_TABLE: &str = "x_const";"#, "x_const"),
        (
            r#"pub(crate) const STATE_MODEL: &str = "x_state";"#,
            "x_state",
        ),
        (r#"const COLLECTION: &'static str = "x_coll";"#, "x_coll"),
    ] {
        assert_eq!(names(src), [want], "{src}");
    }
    for src in [
        // A method of the same name on something else: one argument, or two.
        r#"fn f(v: &Value) { v.get("auth_user"); }"#,
        r#"fn f(m: &mut Map) { m.insert("auth_user", 1); }"#,
        r#"fn f() { let list = |a, b| g(a, b); list("auth_user", "A description"); }"#,
        r#"fn f() { s.update("auth_user", id); }"#,
        // Another method, a bare function, a macro.
        r#"fn f() { s.fetch("auth_user", id); println!("auth_user"); g("auth_user"); }"#,
        // Not a name of a table: a constant of another kind, text with capitals or spaces.
        r#"const KIND: &str = "auth_user";"#,
        r#"const IMMUTABLE_RECORD: &str = "auth_user";"#,
        r#"const LOG_TABLE: &str = "Not A Table";"#,
        r#"fn f(b: &dyn Backend) { b.get("Auth User", id); }"#,
        // A comment and test code.
        "// b.get(\"auth_user\", id)\nfn f() {}",
        "#[cfg(test)]\nmod tests { fn t(b: &dyn Backend) { b.get(\"auth_user\", id); } }",
        "#[cfg(test)]\nconst LOG_TABLE: &str = \"auth_user\";",
    ] {
        assert!(names(src).is_empty(), "{src}: {:?}", names(src));
    }
}

/// A name that is on no list is reported with its place; one on a list is not; a name on a list
/// that nothing spells is reported as stale.
#[test]
fn a_table_on_no_list_is_reported_and_a_listed_one_is_not() {
    let files = tree(&[
        (
            "kernel/obop-x/src/lib.rs",
            "fn f(b: &dyn Backend) {\n    b.get(\"auth_user\", id);\n    b.get(\"brand_new_table\", id);\n    b.get(\"party\", id);\n    b.get(\"device\", id);\n}\n",
        ),
        (
            "kernel/obop-y/src/lib.rs",
            "const FRESH_MODEL: &str = \"fresh_table\";\n",
        ),
    ]);
    let (unreserved, stale) = check(&files);
    assert_eq!(
        unreserved,
        [
            "kernel/obop-x/src/lib.rs:3: `brand_new_table` as the first argument of a call of the storage backend",
            "kernel/obop-y/src/lib.rs:1: `fresh_table` as the value of a constant that names a table or a model",
        ]
    );
    // Almost everything on the lists is spelled by nothing in these two files.
    assert!(stale.contains(&"obop_package".to_string()), "{stale:?}");
    assert!(!stale.contains(&"auth_user".to_string()), "{stale:?}");
    assert!(!stale.contains(&"party".to_string()), "{stale:?}");
}
