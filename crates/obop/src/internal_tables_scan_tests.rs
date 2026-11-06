//! The scan that keeps `obop_registry::INTERNAL_TABLES` complete. A table is named by a model: a
//! package that declares a model `auth_user` (the manifest rule lets the package `auth` do so)
//! would get the generated create, update, delete, list and search over the table that holds the
//! accounts and their password hashes. So no package may declare the name of a table that the
//! program's own crates read or write by name, and the installer reserves `INTERNAL_TABLES`. A
//! table that is missing from that list is a way in, and nothing else would notice it, so this test
//! reads the non-test Rust code of every crate under `crates/` (see `src_scan.rs`: comments are
//! blanked, test code and the files of test modules are left out) and fails when it finds the name
//! of a table that is on no list. (It lives in the binary's crate because it needs the real `App`:
//! the models of its modules are what it compares the names with.)
//!
//! **What it catches.** A table named in one of these ways, whatever the crate and whatever the
//! identifier is called:
//! - the value of a constant with one of the words `TABLE`, `MODEL` or `COLLECTION` in its name
//!   (`const PASSKEY_TABLE: &str = "auth_passkey"`);
//! - the first argument of a call of a method of the storage `Backend` that takes a model
//!   (`.get("auth_user", id)`; the number of arguments is checked, so `value.get("key")` and
//!   `map.insert("key", v)` are not taken for it), or the value of a field called `model` or
//!   `table` (`Op::Insert { model: "x".into(), .. }`), when it is a string literal;
//! - the same two places when the argument is a **constant of any name**, anywhere in the
//!   workspace (`unit.get(COUNTER, &key)`, `model: Self::TABLE.into()`, `b.list(model::VAULT)`,
//!   `self.put_link(ORIGIN, ..)`): the identifier is looked up among the `const X: &str = "name"` of
//!   the crate it is used in, and of the whole workspace when that crate has none of the name. The
//!   "place" is a call of any function or method of the workspace whose first parameter, after
//!   `self`, is called `model` or `table` and takes a string (the scan reads the signatures itself:
//!   [`model_first`]), so a helper nobody listed is taken too. The counters of the document
//!   sequences (`sequence_counter`, whose constant is called `COUNTER`) and the tables of the
//!   connectors, the forum, the status page and the control plane were open for that reason.
//!
//! Each name found must be on `INTERNAL_TABLES`, on `RESERVED_MODELS`, a model of a module of the
//! running `App` (the installer reserves those at start), or on [`BUNDLED_MODELS`] below. A name of
//! `INTERNAL_TABLES` or [`BUNDLED_MODELS`] that no code spells any more is a failure too, so the
//! lists cannot go stale.
//!
//! **What it does not see.** It is a textual scan, not a parser. A table whose name is built at run
//! time (`format!("{}_user", "auth")`), one handed on through a variable or a parameter
//! (`fn read(b: &dyn Backend, model: &str)`), one listed in an array and used in a loop, one reached
//! through a method that is not in [`METHODS`] (a literal: a constant is followed to any function
//! that takes a model first), and a name in a macro are not seen. It makes a new
//! table visible; it does not stop one who works around it. The test in
//! `recorded_tables_tests.rs` sees what the scan cannot, for the flows it runs: it records every
//! model the storage is asked for while the real `App` runs.

use crate::app::App;
use crate::src_scan::{
    Lexed, is_ident, line_of, non_test, read_tree, skip_ws, tree, word_at, words,
};
use obop_pkg::{Bundled, Manifest, Marketplace, installer::Request};
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

/// The words that make a constant a table name, whatever it is used for.
const CONST_WORDS: [&str; 3] = ["TABLE", "MODEL", "COLLECTION"];

/// The models of bundled packages (`packages/*.toml`) that a crate of the program also spells (the
/// accounts import reads `accounting_move`, the shop reads `ecommerce_listing`, the signing page
/// reads `sign_request`). They are not tables of the program: the package owns each of them while
/// it is installed, and the installer refuses a second package that declares one then. They are
/// not reserved **by name**, though: before the bundled package is installed, a package of its id
/// can declare the name (it needs `platform.admin` to install, and `origin/main` has the same
/// gap: docs/ISSUES.md N17 says so). Adding a name here is the decision that it is a bundled
/// package's model and not a table of the program; the test below checks that a bundled package
/// does declare it and that no module of the `App` and no reserved list has it.
const BUNDLED_MODELS: [&str; 17] = [
    "accounting_account",
    "accounting_fiscal_year",
    "accounting_journal",
    "accounting_move",
    "accounting_move_line",
    "accounting_se_company",
    "documents_document",
    "ecommerce_discount_code",
    "ecommerce_discount_use",
    "ecommerce_listing",
    "ecommerce_order",
    "ecommerce_order_line",
    "ecommerce_payment",
    "sign_link",
    "sign_request",
    "website_form_submission",
    "website_page",
];

/// Names the scan finds in a place where a table goes that are no table. `show_history` is the field
/// of a signing request, read with `req.get(HISTORY_FLAG)`: a map lookup with one argument, which the
/// storage's `Tracking::get(model)` has too, so the scan cannot tell them apart. Adding a name here
/// says it is no table; a name that the scan no longer finds is a failure, like the other lists.
const NOT_TABLES: [&str; 1] = ["show_history"];

/// One place that names a table: the file, the line, the name and how it was named.
struct Hit {
    path: String,
    line: usize,
    name: String,
    how: String,
}

fn is_table_name(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_lowercase())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// A constant name: capitals, digits and `_`, starting with a capital.
fn is_screaming(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_uppercase())
        && s.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

/// The crate a path belongs to (`kernel/obop-x/src/lib.rs` is in `kernel/obop-x`).
fn crate_of(path: &str) -> &str {
    path.split_once("/src/").map_or(path, |(c, _)| c)
}

/// The module a file is: `kernel/obop-x/src/model.rs` is `model`, `.../model/mod.rs` too.
fn file_stem(path: &str) -> &str {
    let mut parts = path.rsplit('/');
    let file = parts.next().unwrap_or("");
    match file.trim_end_matches(".rs") {
        "mod" | "lib" | "main" => parts.next().unwrap_or(""),
        stem => stem,
    }
}

/// The byte before `at`, past whitespace, and where it is.
fn byte_before(code: &[u8], at: usize) -> Option<(usize, u8)> {
    code[..at]
        .iter()
        .rposition(|c| !c.is_ascii_whitespace())
        .map(|p| (p, code[p]))
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

/// The number of arguments of the call whose first argument ends at `after`. A literal is blanked
/// in the code, so `starts` (where each one begins) tells it from blank space. A trailing comma
/// does not make another argument, and the commas between the parameters of a closure
/// (`|a, b| ..`, `move |a, b| ..`, `&mut |a, b| ..`) do not make any.
fn arguments(code: &[u8], starts: &BTreeSet<usize>, after: usize) -> usize {
    let (mut depth, mut count, mut comma) = (0_usize, 1_usize, false);
    let mut in_params = false;
    for (at, &c) in code.iter().enumerate().skip(after) {
        let literal = starts.contains(&at);
        if in_params {
            in_params = c != b'|';
            continue;
        }
        match c {
            b')' | b']' | b'}' if depth == 0 => break,
            b',' if depth == 0 => {
                comma = true;
                continue;
            }
            c if c.is_ascii_whitespace() && !literal => continue,
            _ => {}
        }
        let begins_argument = comma;
        if comma {
            count += 1;
            comma = false;
        }
        match c {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            // The parameters of a closure that starts an argument (or follows `&`, `&mut`, `move`).
            b'|' if depth == 0
                && (begins_argument
                    || matches!(byte_before(code, at), Some((_, b'&')))
                    || matches!(ident_before(code, at), "mut" | "move")) =>
            {
                in_params = true;
            }
            _ => {}
        }
    }
    count
}

/// A constant that holds a string with the look of a table name.
struct Def {
    path: String,
    line: usize,
    value: String,
}

/// Every `const NAME: &str = "x";` and `static NAME: &str = "x";` of the non-test code whose value
/// looks like a table name, by the identifier. (The same identifier can be defined in several
/// crates: `TABLE` is.) Arrays and anything computed are not read.
fn consts(lexed: &BTreeMap<String, Lexed>) -> BTreeMap<String, Vec<Def>> {
    let mut out: BTreeMap<String, Vec<Def>> = BTreeMap::new();
    for (path, l) in lexed {
        let by_start: BTreeMap<usize, &String> =
            l.strings.iter().map(|(s, _, t)| (*s, t)).collect();
        for keyword in ["const", "static"] {
            for at in words(&l.code, keyword) {
                let name_at = skip_ws(&l.code, at + keyword.len());
                let name = word_at(&l.code, name_at);
                if !is_screaming(name) {
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
                if l.code[eq] != b'=' {
                    continue;
                }
                // The literal is blank in the code: the next thing after `=` is it, or something else.
                let value = (eq + 1..l.code.len())
                    .find(|i| by_start.contains_key(i) || !l.code[*i].is_ascii_whitespace());
                if let Some(text) = value.and_then(|i| by_start.get(&i))
                    && is_table_name(text)
                {
                    out.entry(name.to_string()).or_default().push(Def {
                        path: path.clone(),
                        line: line_of(&l.code, eq),
                        value: (*text).clone(),
                    });
                }
            }
        }
    }
    out
}

/// The constant that starts at `at` (past whitespace and any `&`): `NAME`, `Self::NAME`,
/// `module::NAME`. Its name, the segment in front of it (`header` in `header::ORIGIN`) and where
/// the path ends. A lower-case last segment is a variable, a function or a type, and no constant:
/// `None`.
fn const_path(code: &[u8], at: usize) -> Option<(&str, Option<&str>, usize)> {
    let mut j = skip_ws(code, at);
    while code.get(j) == Some(&b'&') {
        j = skip_ws(code, j + 1);
    }
    let mut qualifier = None;
    loop {
        let w = word_at(code, j);
        if w.is_empty() {
            return None;
        }
        let end = j + w.len();
        let k = skip_ws(code, end);
        if code[k..].starts_with(b"::") {
            qualifier = Some(w);
            j = skip_ws(code, k + 2);
            continue;
        }
        return is_screaming(w).then_some((w, qualifier, end));
    }
}

/// The parameters of the function whose `(` is at `open`, as text, split at the commas that are not
/// inside brackets (a closure type or a tuple has commas of its own).
fn parameters(code: &[u8], open: usize) -> Vec<String> {
    let (mut out, mut depth, mut from) = (Vec::new(), 0_i32, open + 1);
    for k in open + 1..code.len() {
        match code[k] {
            b'(' | b'[' | b'<' | b'{' => depth += 1,
            // The `>` of `->` closes nothing.
            b'>' if code[k - 1] != b'-' => depth -= 1,
            b')' | b']' | b'}' if depth == 0 => {
                out.push(String::from_utf8_lossy(&code[from..k]).trim().to_string());
                break;
            }
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                out.push(String::from_utf8_lossy(&code[from..k]).trim().to_string());
                from = k + 1;
            }
            _ => {}
        }
    }
    out.retain(|p| !p.is_empty());
    out
}

/// The functions and methods of the workspace whose first parameter, after `self`, is a model: a
/// parameter called `model` or `table` that takes a string. `fn put_link(&self, model: &str, id:
/// &str, f: &Fields, refresh: bool)` is `put_link` with four arguments; the methods of `Backend` are
/// in it, and so is every helper the authors wrote that takes a model first. By name and by the
/// number of arguments (`Map::get(key)` is not `Backend::get(model, id)`). The author's own word for
/// the parameter is the evidence: a constant handed to such a function is a model name.
fn model_first(lexed: &BTreeMap<String, Lexed>) -> BTreeMap<String, BTreeSet<usize>> {
    let mut out: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    for l in lexed.values() {
        let code = &l.code;
        for at in words(code, "fn") {
            let name_at = skip_ws(code, at + 2);
            let name = word_at(code, name_at);
            let mut open = skip_ws(code, name_at + name.len());
            if code.get(open) == Some(&b'<') {
                // Generic parameters: up to the `>` that closes them.
                let mut depth = 0_i32;
                for (k, c) in code.iter().enumerate().skip(open) {
                    match c {
                        b'<' => depth += 1,
                        b'>' if code[k - 1] != b'-' => {
                            depth -= 1;
                            if depth == 0 {
                                open = skip_ws(code, k + 1);
                                break;
                            }
                        }
                        _ => {}
                    }
                }
            }
            if name.is_empty() || code.get(open) != Some(&b'(') {
                continue;
            }
            let params: Vec<String> = parameters(code, open)
                .into_iter()
                .filter(|p| {
                    let head = p.split(':').next().unwrap_or("").trim();
                    head != "self" && !head.ends_with(" self") && !head.ends_with("&self")
                })
                .collect();
            let Some(first) = params.first() else {
                continue;
            };
            let Some((head, ty)) = first.split_once(':') else {
                continue;
            };
            if matches!(head.trim(), "model" | "table") && ty.contains("str") {
                out.entry(name.to_string())
                    .or_default()
                    .insert(params.len());
            }
        }
    }
    out
}

/// Where a constant is handed to such a function as the table, or as the value of a field `model` or
/// `table`: `(offset, name, how)`.
fn const_uses<'a>(
    code: &'a [u8],
    starts: &BTreeSet<usize>,
    model_first: &BTreeMap<String, BTreeSet<usize>>,
) -> Vec<(usize, &'a str, Option<&'a str>, &'static str)> {
    let mut out = Vec::new();
    // `.get(NAME, ..)`, `put_link(NAME, ..)`, `Op::add(NAME, ..)`: the first argument of a function
    // that takes a model first.
    let mut at = 0;
    while at < code.len() {
        if !is_ident(code[at]) || (at > 0 && is_ident(code[at - 1])) {
            at += 1;
            continue;
        }
        let w = word_at(code, at);
        let call = skip_ws(code, at + w.len());
        if let Some(arities) = model_first.get(w)
            && code.get(call) == Some(&b'(')
            && let Some((name, qualifier, end)) = const_path(code, call + 1)
            && matches!(code.get(skip_ws(code, end)), Some(b',' | b')'))
            && arities.contains(&arguments(code, starts, end))
        {
            out.push((
                at,
                name,
                qualifier,
                "a constant handed to a function that takes a model first",
            ));
        }
        at += w.len();
    }
    // `model: NAME.into()`: the field of an operation.
    for field in ["model", "table"] {
        for at in words(code, field) {
            let colon = skip_ws(code, at + field.len());
            if code.get(colon) != Some(&b':') {
                continue;
            }
            let Some((name, qualifier, end)) = const_path(code, colon + 1) else {
                continue;
            };
            let mut after = skip_ws(code, end);
            // `.into()`, `.to_string()`, `.to_owned()`: the same text, owned.
            while code.get(after) == Some(&b'.') {
                let call_at = skip_ws(code, after + 1);
                let w = word_at(code, call_at);
                if !matches!(w, "into" | "to_string" | "to_owned") {
                    break;
                }
                let paren = skip_ws(code, call_at + w.len());
                if code.get(paren) != Some(&b'(') {
                    break;
                }
                let close = skip_ws(code, paren + 1);
                if code.get(close) != Some(&b')') {
                    break;
                }
                after = skip_ws(code, close + 1);
            }
            if matches!(code.get(after), Some(b',' | b'}')) {
                out.push((
                    at,
                    name,
                    qualifier,
                    "a constant handed as the value of a field `model` or `table`",
                ));
            }
        }
    }
    out
}

fn hits(files: &BTreeMap<String, String>) -> Vec<Hit> {
    let lexed = non_test(files);
    let defs = consts(&lexed);
    let model_first = model_first(&lexed);
    let mut out = Vec::new();
    // A constant with a table word in its name is a table, used or not.
    for (name, list) in &defs {
        if name.split('_').any(|w| CONST_WORDS.contains(&w)) {
            for d in list {
                out.push(Hit {
                    path: d.path.clone(),
                    line: d.line,
                    name: d.value.clone(),
                    how: "the value of a constant that names a table or a model".to_string(),
                });
            }
        }
    }
    for (path, l) in &lexed {
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
                    how: how.to_string(),
                });
            }
        }
        // A constant of any name, handed on in the same two places: the crate's own constants of
        // that name first, then the workspace's.
        for (at, name, qualifier, how) in const_uses(&l.code, &starts, &model_first) {
            let Some(all) = defs.get(name) else { continue };
            // `header::ORIGIN` is no constant of ours unless a file called `header` has one.
            let named: Vec<&Def> = all
                .iter()
                .filter(|d| match qualifier {
                    Some(q) if !matches!(q, "Self" | "self" | "super" | "crate") => {
                        file_stem(&d.path) == q
                    }
                    _ => true,
                })
                .collect();
            let here: Vec<&Def> = named
                .iter()
                .copied()
                .filter(|d| crate_of(&d.path) == crate_of(path))
                .collect();
            let chosen = if here.is_empty() { named } else { here };
            for d in chosen {
                out.push(Hit {
                    path: path.clone(),
                    line: line_of(&l.code, at),
                    name: d.value.clone(),
                    how: format!("{how} (`{name}`, defined at {}:{})", d.path, d.line),
                });
            }
        }
    }
    out
}

/// The models of the bundled packages (`packages/*.toml`): the names the catalogue declares.
fn bundled_package_models() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for l in Bundled.list().expect("the bundled catalogue lists") {
        let text = Bundled.fetch(&l.id).expect("the bundled package");
        let m = Manifest::parse(&text).expect("the bundled package parses");
        out.extend(m.model.iter().map(|x| x.name.clone()));
    }
    out
}

/// What the scan may find without a complaint besides the two lists of the registry: the models of
/// the modules of a running `App` (the installer reserves them), and [`BUNDLED_MODELS`].
fn modules_models() -> BTreeSet<String> {
    let app = App::new();
    let mut out: BTreeSet<String> = app.dispatcher.model_names().into_iter().collect();
    out.extend(BUNDLED_MODELS.iter().map(|n| n.to_string()));
    out
}

/// The places that name a table nobody has accounted for (sorted), and the names of the lists that
/// no code spells. `modules` is what [`modules_models`] returns.
fn check(
    files: &BTreeMap<String, String>,
    modules: &BTreeSet<String>,
) -> (Vec<String>, Vec<String>) {
    let known: BTreeSet<&str> = INTERNAL_TABLES
        .iter()
        .chain(RESERVED_MODELS.iter())
        .copied()
        .chain(modules.iter().map(String::as_str))
        .collect();
    let found = hits(files);
    let used: BTreeSet<&str> = found.iter().map(|h| h.name.as_str()).collect();
    let mut unreserved: Vec<String> = found
        .iter()
        .filter(|h| !known.contains(h.name.as_str()) && !NOT_TABLES.contains(&h.name.as_str()))
        .map(|h| format!("{}:{}: `{}` as {}", h.path, h.line, h.name, h.how))
        .collect();
    unreserved.sort();
    unreserved.dedup();
    let stale = INTERNAL_TABLES
        .iter()
        .chain(BUNDLED_MODELS.iter())
        .chain(NOT_TABLES.iter())
        .filter(|n| !used.contains(**n))
        .map(|n| (*n).to_string())
        .collect();
    (unreserved, stale)
}

fn source_tree() -> BTreeMap<String, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = BTreeMap::new();
    read_tree(&root, &root, &mut files);
    files
}

#[test]
fn every_table_the_program_names_is_reserved_or_a_model_of_a_module() {
    let (unreserved, stale) = check(&source_tree(), &modules_models());
    assert!(
        unreserved.is_empty(),
        "the program names a table that no package is kept from declaring. Add it to \
         obop_registry::INTERNAL_TABLES (a table of the program), or to BUNDLED_MODELS here only if \
         it is a model of a bundled package (a model of a built-in module needs nothing):\n{}",
        unreserved.join("\n")
    );
    assert!(
        stale.is_empty(),
        "names of INTERNAL_TABLES, BUNDLED_MODELS or NOT_TABLES that no code spells any more (the \
         scan reads the wrong thing, or the row is stale):\n{}",
        stale.join("\n")
    );
}

/// The list of bundled models is held to what it says: each name is declared by a bundled package,
/// and none is a table of the program, a reserved name or a model of a built-in module (which the
/// installer refuses by name).
#[test]
fn the_bundled_models_are_the_models_of_bundled_packages_and_of_nothing_else() {
    let declared = bundled_package_models();
    let app = App::new();
    let builtin: BTreeSet<String> = app.dispatcher.model_names().into_iter().collect();
    for name in BUNDLED_MODELS {
        assert!(
            declared.contains(name),
            "{name} is declared by no bundled package"
        );
        assert!(
            !builtin.contains(name),
            "{name} is a model of a built-in module"
        );
        assert!(
            !INTERNAL_TABLES.contains(&name) && !RESERVED_MODELS.contains(&name),
            "{name} is reserved already"
        );
    }
    assert!(
        BUNDLED_MODELS.windows(2).all(|w| w[0] < w[1]),
        "sorted, once each"
    );
}

/// The point of it all: every name the code spells that is reserved by name, or is a model of a
/// built-in module, is refused for a package by the installer of the real `App`. The package takes
/// the name as its own id, which the manifest rule allows. A name that only a bundled package
/// declares is the one thing the installer does not refuse by name ([`BUNDLED_MODELS`]).
#[test]
fn the_installer_refuses_every_name_the_program_spells_except_the_models_of_bundled_packages() {
    let app = App::new();
    let builtin: BTreeSet<String> = app.dispatcher.model_names().into_iter().collect();
    let spelled: BTreeSet<String> = hits(&source_tree()).into_iter().map(|h| h.name).collect();
    assert!(spelled.len() > 50, "the scan found {} names", spelled.len());
    let mut checked = 0;
    for name in &spelled {
        if BUNDLED_MODELS.contains(&name.as_str()) || NOT_TABLES.contains(&name.as_str()) {
            continue;
        }
        let text = package_declaring(name);
        // Refused in the plan, either by the install gate (an error: the name is reserved by name,
        // or the platform keeps it for its own records) or as a blocker (a model of a module).
        match app.installer.plan(Request::Manifest(&text)) {
            Err(e) => assert!(e.contains(name.as_str()), "{name}: {e}"),
            Ok((plan, _)) => assert!(
                plan.blockers
                    .iter()
                    .any(|b| b.message == format!("model {name} already exists")),
                "{name}: the installer plans a package that declares it: {:?}",
                plan.blockers.iter().map(|b| &b.message).collect::<Vec<_>>()
            ),
        }
        let e = app.installer.install(&text, "upload", "root").unwrap_err();
        assert!(e.contains(name.as_str()), "{name}: {e}");
        assert!(
            !app.dispatcher.has_model(name) || builtin.contains(name),
            "{name}: nothing of the package was registered"
        );
        checked += 1;
    }
    assert!(
        checked > 40,
        "only {checked} names were put to the installer"
    );
}

/// A package whose id is `model` and that declares the model of that name (the manifest rule lets a
/// package declare a model named like its id), with a group of its own.
fn package_declaring(model: &str) -> String {
    format!(
        r#"
[module]
id = "{model}"
version = "0.1.0"
name = "Squatter"
description = "A package"
requires_core = "^0.2"

[[model]]
name = "{model}"
description = "A model"
read_group = "squat.user"
write_group = "squat.user"

  [[model.field]]
  name = "name"
  description = "A name"
  class = "none"
"#
    )
}

/// The ways a table is named, and the near misses that are not.
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
        // A constant with a table word in its name.
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

/// The functions of the workspace that take a model first, as the scan reads them from the
/// signatures: the methods of the backend, a helper with a `model` parameter, one with generics and
/// a closure, a free function. Not: a parameter of another name or another type, a `self` that is
/// not the first.
const SIGNATURES: &str = "trait Backend { fn insert(&self, model: &str, id: &str, f: &Fields); fn get(&self, model: &str, id: &str) -> R<Option<Fields>>; fn list(&self, model: &str) -> R<Vec<(String, Fields)>>; fn count(&self, model: &str) -> R<usize>; fn find(&self, model: &str, field: &str, value: &J) -> R<Vec<(String, Fields)>>; }\nimpl Unit { pub fn merge(&mut self, model: &str, id: &str, patch: Fields) -> R<()> { Ok(()) } fn put_link(&self, model: &str, id: &str, f: &Fields, refresh: bool) -> Result<(), String> { Ok(()) } fn scan<F: FnMut(&str, &Fields) -> bool>(&self, table: &str, visit: F) {} fn each(&self, table: &str, visit: impl Fn(&str) -> bool, last: bool) {} }\npub fn describe(model: &str, command: &str) -> Option<&'static str> { None }\nfn other(name: &str, model: &str) {}\nfn number(model: u32) {}\nfn skipped(&self, fields: &[&str], model: &str) {}\n";

/// A constant of any name is followed from where it is defined to where it is handed to a function
/// that takes a model first, in the same crate or in another: the way the counters of the sequences
/// (`unit.get(COUNTER, ..)`) and the origins of the connectors (`self.put_link(ORIGIN, ..)`) escaped
/// the scan of the constants with a table word in their name.
#[test]
fn the_scan_follows_a_constant_of_any_name_to_the_place_it_is_handed_to_the_storage() {
    let names = |files: &[(&str, &str)]| -> Vec<String> {
        let mut all = vec![("kernel/obop-s/src/lib.rs", SIGNATURES)];
        all.extend_from_slice(files);
        let mut v: Vec<String> = hits(&tree(&all)).into_iter().map(|h| h.name).collect();
        v.sort();
        v
    };
    let def = (
        "kernel/obop-a/src/seq.rs",
        "pub const COUNTER: &str = \"x_counter\";",
    );
    for user in [
        // The methods of the backend, by the bare name, a path, `&`, and a path through `Self`.
        "fn f(u: &Unit) { u.get(COUNTER, &key); }",
        "fn f(u: &Unit) { u.get(seq::COUNTER, &key); }",
        "fn f(u: &Unit) { u.get(&seq::COUNTER, &key); }",
        "fn f() { self.b.list(Self::COUNTER); }",
        "fn f() { b.count(crate::seq::COUNTER).unwrap(); }",
        "fn f() { b.insert(COUNTER, id, &f); }",
        "fn f() { b.find(COUNTER, \"f\", &v); }",
        "fn f() { b\n    .get(\n        COUNTER,\n        id,\n    ); }",
        // A helper nobody listed, one with generics and a closure, a free function, an associated
        // function: whatever takes a `model` or a `table` first.
        "fn f(u: &mut Unit) { u.merge(COUNTER, id, patch); }",
        "fn f() { needed(|| self.put_link(COUNTER, k, &m, true))?; }",
        "fn f() { b.scan(COUNTER, |a, b| true); }",
        "fn f() { b.scan(COUNTER, &mut |a, b| true); }",
        "fn f() { b.scan(COUNTER, move |a, b| true); }",
        "fn f() { b.scan(COUNTER, &|a, b| true); }",
        "fn f() { u.each(COUNTER, |s| true, false); }",
        "fn f() { describe(COUNTER, \"get\"); }",
        "fn f() { crate::describe(COUNTER, \"get\"); }",
        // The field of an operation, with the conversions that make the text owned.
        "fn f() { Op::Merge { model: COUNTER.into(), id, patch } }",
        "fn f() { Op::Merge { model: COUNTER.to_string(), id } }",
        "fn f() { let s = S { table: Self::COUNTER, f }; }",
        "fn f() { let s = S { table: seq::COUNTER.to_owned() }; }",
    ] {
        assert_eq!(
            names(&[def, ("kernel/obop-b/src/lib.rs", user)]),
            ["x_counter"],
            "{user}"
        );
    }
    // The constant of the crate wins over one of the same name elsewhere; with none in the crate,
    // every one of the name counts.
    let other = (
        "kernel/obop-c/src/lib.rs",
        "const COUNTER: &str = \"y_counter\";",
    );
    let user = "fn f(u: &Unit) { u.get(COUNTER, &key); }";
    assert_eq!(
        names(&[def, other, ("kernel/obop-a/src/use.rs", user)]),
        ["x_counter"],
        "the crate's own"
    );
    assert_eq!(
        names(&[def, other, ("kernel/obop-b/src/lib.rs", user)]),
        ["x_counter", "y_counter"],
        "every one of the name"
    );
    // The near misses: a variable, a field, a function that takes no model first (another word for
    // the parameter, another type, `self` not first, the definition itself), a constant that is
    // only copied, a constant in test code, one that is defined but never named, and one that is no
    // table.
    for user in [
        "fn f(u: &Unit) { u.get(counter, &key); }",
        "fn f(u: &Unit) { u.get(self.COUNTER, &key); }",
        // Only the constant itself is the table, not something made from it.
        "fn f(u: &Unit) { u.get(COUNTER.len(), &key); u.get(COUNTER + 1, &key); }",
        "fn f() { g(COUNTER, id); other(COUNTER, id); number(COUNTER); }",
        "fn f() { s.fetch(COUNTER, id); s.skipped(&[], COUNTER); }",
        "fn f() { let c = COUNTER; h(c); }",
        "#[cfg(test)]\nmod tests { fn t(u: &Unit) { u.get(COUNTER, &key); } }",
        "fn f() { b.get(OTHER, id); }",
        "fn get(&self, model: &str, id: &str) {}",
        // The number of arguments tells the call from a method of the same name on something else.
        "fn f(m: &Map) { m.get(COUNTER); m.get(COUNTER, a, b); m.list(COUNTER, 1); }",
        // A constant of another module of the same name is not ours.
        "fn f(u: &Unit) { u.get(header::COUNTER, &key); }",
        "fn f() { let s = S { model: COUNTER.len() }; }",
    ] {
        assert!(
            names(&[def, ("kernel/obop-b/src/lib.rs", user)]).is_empty(),
            "{user}"
        );
    }
    for not_a_table in [
        "pub const COUNTER: u64 = 3;",
        "pub const COUNTER: &[&str] = &[\"x_counter\"];",
        "pub const COUNTER: &str = \"Not A Table\";",
        "#[cfg(test)]\npub const COUNTER: &str = \"x_counter\";",
    ] {
        assert!(
            names(&[
                ("kernel/obop-a/src/seq.rs", not_a_table),
                (
                    "kernel/obop-b/src/lib.rs",
                    "fn f(u: &Unit) { u.get(COUNTER, &key); }"
                ),
            ])
            .is_empty(),
            "{not_a_table}"
        );
    }
    // `static` is read like `const`.
    assert_eq!(
        names(&[
            (
                "kernel/obop-a/src/seq.rs",
                "static COUNTER: &str = \"s_counter\";"
            ),
            (
                "kernel/obop-b/src/lib.rs",
                "fn f(u: &Unit) { u.get(COUNTER, &key); }"
            ),
        ]),
        ["s_counter"]
    );
}

/// A name that is on no list is reported with its place; one on a list is not; a name on a list
/// that nothing spells is reported as stale (a table of the program, and a bundled model alike).
#[test]
fn a_table_on_no_list_is_reported_and_a_listed_one_is_not() {
    let files = tree(&[
        ("kernel/obop-s/src/lib.rs", SIGNATURES),
        (
            "kernel/obop-x/src/lib.rs",
            "fn f(b: &dyn Backend) {\n    b.get(\"auth_user\", id);\n    b.get(\"brand_new_table\", id);\n    b.get(\"party\", id);\n    b.get(\"device\", id);\n    b.get(\"website_page\", id);\n    b.get(\"ecommerce_new\", id);\n}\n",
        ),
        (
            "kernel/obop-y/src/lib.rs",
            "const FRESH_MODEL: &str = \"fresh_table\";\npub const NUMBER: &str = \"fresh_counter\";\nfn g(u: &Unit) { u.get(NUMBER, &k); }\n",
        ),
    ]);
    let modules: BTreeSet<String> = ["party", "website_page"].map(String::from).into();
    let (unreserved, stale) = check(&files, &modules);
    assert_eq!(
        unreserved,
        [
            "kernel/obop-x/src/lib.rs:3: `brand_new_table` as the first argument of a call of the storage backend",
            "kernel/obop-x/src/lib.rs:7: `ecommerce_new` as the first argument of a call of the storage backend",
            "kernel/obop-y/src/lib.rs:1: `fresh_table` as the value of a constant that names a table or a model",
            "kernel/obop-y/src/lib.rs:3: `fresh_counter` as a constant handed to a function that takes a model first (`NUMBER`, defined at kernel/obop-y/src/lib.rs:2)",
        ]
    );
    // Almost everything on the lists is spelled by nothing in these two files.
    assert!(stale.contains(&"obop_package".to_string()), "{stale:?}");
    assert!(!stale.contains(&"auth_user".to_string()), "{stale:?}");
    // A bundled model that nothing spells is stale too (`ecommerce_order` is not spelled in these
    // files, `website_page` is), and so is a name of the list of non-tables (`show_history`).
    assert!(stale.contains(&"ecommerce_order".to_string()), "{stale:?}");
    assert!(!stale.contains(&"website_page".to_string()), "{stale:?}");
    assert!(stale.contains(&"show_history".to_string()), "{stale:?}");
}
