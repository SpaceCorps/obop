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
//! - a string literal handed to a method of the storage `Backend` that takes a model
//!   (`.get("auth_user", id)`, the number of arguments checked, so `value.get("key")` and
//!   `map.insert("key", v)` are not taken for it), or to **any function or method of the workspace
//!   that takes a model at that place**: a parameter called `model` or `table`, or ending `_model` or
//!   `_table`, that takes a string (the scan reads the signatures itself: [`signatures`]). So
//!   `api.list("website_menu_item", &[], Some(50), 0)`, `Op::add("x", ..)`, `self.localize(ctx, r,
//!   "x", ..)` and a helper nobody listed are taken, by name and number of arguments, at the first
//!   place or a later one. A name that some other definition shares with another meaning
//!   (`new(name, ..)`), and the names of the standard collections (`insert`, `get`, `remove`,
//!   `set`, `contains`: [`COLLECTION_METHODS`]), are not told by their shape; a call written as a
//!   path is told by the type in front of the name where that type defines the function (`Op::add`
//!   takes a model, `Report::add` does not: [`impl_type_of`] reads the `impl` or `trait` block that
//!   holds each definition);
//! - the value of a field called `model`, `table` or `collection` (`Op::Insert { model:
//!   "x".into(), .. }`), as a literal (also `&"x"`, `String::from("x")`, `.to_string()`) or as a
//!   constant;
//! - a **constant of any name**, anywhere in the workspace, in those places (`unit.get(COUNTER,
//!   &key)`, `model: Self::TABLE.into()`, `b.list(model::VAULT)`, `self.put_link(ORIGIN, ..)`): the
//!   identifier is looked up among the `const X: &str = "name"` of the crate it is used in, and of
//!   the whole workspace when that crate has none of the name. Here the shape of the signature is
//!   enough, whatever else the name means.
//!
//! Each name found must be on `INTERNAL_TABLES`, on `RESERVED_MODELS`, a model of a module of the
//! running `App` (the installer reserves those at start), or on [`BUNDLED_MODELS`] below. A name of
//! `INTERNAL_TABLES`, [`BUNDLED_MODELS`] or [`NOT_TABLES`] that no code spells any more is a failure
//! too, so the lists cannot go stale. A test reads the crates that the scan reads, so that a crate
//! cannot drop out of the tree unnoticed.
//!
//! **What it does not see.** It is a textual scan, not a parser. Not seen:
//! - a name built at run time (`format!("{}_user", "auth")`, `concat!`), one handed on through a
//!   variable, a tuple or a parameter (`fn read(b: &dyn Backend, model: &str)`; the drill's probe
//!   was one until it got a constant), one listed in an array or a `vec!` of rows and used in a
//!   loop or by `insert_many`, one a function returns, one in a closure parameter, and a name in a
//!   macro;
//! - a constant reached by a path other than a file name: `Type::NAME` of an associated constant is
//!   looked up among the constants of a file called `Type`, so a `const` inside `impl Type` in
//!   another file is not followed; a constant that is another constant (`const B: &str = A;`) is
//!   not read, only a literal is;
//! - a literal handed to a function whose shape is shared (the ambiguous names above) by a method
//!   call, which has no type in front of the name (`report.add(..)` and `ops.add(..)` are not told
//!   apart), and one handed to a collection's method (`store.insert("x", fields)`), unless the same
//!   name is spelled elsewhere in a way the scan does see;
//! - what follows a bar that it takes for a closure's, in a shape it cannot tell: the parameters of
//!   a closure are read up to the next bar inside their own brackets and before any `{`, so an
//!   or-pattern whose first bar follows an opener and a literal (`matches!(c, 'a' | 'b')`) is told
//!   from a closure only by that rule (the real call that this once hid is in
//!   `a_table_added_next_to_a_real_call_of_the_code_is_reported`);
//! - code that the scan leaves out by design (see `src_scan.rs`): test code, `#[cfg(test)]` items,
//!   and any file in a directory called `tests` or `benches`, which `cargo` builds only for tests.
//!
//! It makes a new table visible; it does not stop one who works around it. The test in
//! `recorded_tables_tests.rs` sees some of what the scan cannot, for the flows it runs: it records
//! every model the storage is asked for while the real `App` runs those flows (docs/ISSUES.md N17
//! says how many of the tables of the program that reaches).

use crate::app::App;
use crate::src_scan::{
    Lexed, is_ident, line_of, non_test, read_tree, skip_ws, tree, word_at, words,
};
use obop_pkg::{Bundled, Manifest, Marketplace, installer::Request};
use obop_registry::{INTERNAL_TABLES, RESERVED_MODELS};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The methods of `Backend` whose first argument is a model, and how many arguments each takes (from
/// and to). The count tells the call from a method of the same name on something else
/// (`Value::get("key")`, `Map::insert("key", value)`, a closure `list("name", "description")`). These
/// are known by name where the signatures cannot say (the receiver is a trait object whose trait is
/// out of the tree, as in a test of this scan that has no `Backend`), and a literal is taken for a
/// table here whatever else the name means.
const METHODS: [(&str, usize, usize); 9] = [
    ("insert", 3, 3),
    ("get", 2, 2),
    ("list", 1, 1),
    ("scan", 2, 2),
    ("update", 3, 3),
    ("count", 1, 1),
    ("delete", 2, 2),
    ("find", 3, 3),
    ("find_unique", 3, 3),
];

/// Names of methods of the standard collections (and the like) that the signatures cannot tell from
/// a method of the workspace that takes a model: `Store::insert(model, fields)` and
/// `HashMap::insert(key, value)` are both `insert` with two arguments, and the literal of a map is a
/// key, in thousands of places. A literal handed to one of these is a table only by [`METHODS`].
const COLLECTION_METHODS: [&str; 5] = ["contains", "get", "insert", "remove", "set"];

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
const BUNDLED_MODELS: [&str; 26] = [
    "accounting_account",
    "accounting_fiscal_year",
    "accounting_journal",
    "accounting_move",
    "accounting_move_line",
    "accounting_se_company",
    "documents_document",
    "ecommerce_category",
    "ecommerce_discount_code",
    "ecommerce_discount_use",
    "ecommerce_listing",
    "ecommerce_listing_image",
    "ecommerce_listing_translation",
    "ecommerce_order",
    "ecommerce_order_line",
    "ecommerce_payment",
    "ecommerce_shipping_method",
    "ecommerce_tax_rate",
    "sign_link",
    "sign_request",
    "website_blog_post",
    "website_blog_post_translation",
    "website_form_submission",
    "website_menu_item",
    "website_page",
    "website_page_translation",
];

/// Names the scan finds in a place where a table goes that are no table. `show_history` is the field
/// of a signing request, read with `req.get(HISTORY_FLAG)`: a map lookup with one argument, which the
/// storage's `Tracking::get(model)` has too, so the scan cannot tell them apart. The other six are
/// the tables of an imported accounting file (the Open Accounting Schema import keeps a ledger of what
/// it has imported, by its own table and the id in the file: `cx.done("voucher", ..)`, with a
/// parameter called `table`); they are no table of the database. `swap` is the label that the
/// guard of the store gives to `swap_fields` (`done(.., "swap")`, whose parameter is called `model`):
/// that call spans no model. The scan did not see it while the signer's page had a function `done`
/// of the same arity (the name was ambiguous, so its literals were not read); it does now, since that
/// function takes the brand as a fourth argument (slice S1d of the branding work). `hide_product_docs`
/// is the switch of the instance's brand, a field of its row, read with `row.get(SWITCH)` (S2). Adding
/// a name here says it is no table; a name that the scan no longer finds is a failure, like the other
/// lists.
const NOT_TABLES: [&str; 9] = [
    "account",
    "company",
    "customer",
    "document",
    "hide_product_docs",
    "journal",
    "show_history",
    "swap",
    "voucher",
];

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

/// One call, as the scan reads it: the name in front of the parenthesis (empty for a tuple or a
/// grouping, or a call through something that is not a name), and where each of its arguments starts
/// (past whitespace). A trailing comma makes no argument, and neither do the commas between the
/// parameters of a closure (`|a, b| ..`, `move |a, b| ..`, `&mut |a, b| ..`) or those inside `[]`
/// and `{}`.
struct Call<'a> {
    callee: &'a str,
    /// The type in front of the name when it is a path (`Op` of `Op::add(..)`).
    qualifier: Option<&'a str>,
    args: Vec<usize>,
}

/// The identifier in front of the `::` that precedes the name that ends right before `at`.
fn qualifier_before(code: &[u8], at: usize) -> Option<&str> {
    let mut start = at;
    while start > 0 && code[start - 1].is_ascii_whitespace() {
        start -= 1;
    }
    while start > 0 && is_ident(code[start - 1]) {
        start -= 1;
    }
    while start > 0 && code[start - 1].is_ascii_whitespace() {
        start -= 1;
    }
    if start < 2 || &code[start - 2..start] != b"::" {
        return None;
    }
    start -= 2;
    while start > 0 && code[start - 1].is_ascii_whitespace() {
        start -= 1;
    }
    let end = start;
    while start > 0 && is_ident(code[start - 1]) {
        start -= 1;
    }
    (start < end).then(|| std::str::from_utf8(&code[start..end]).unwrap_or(""))
}

/// Where the bar that closes the parameters of the closure whose opening bar stands at `open` is, or
/// `None` when the bar is no closure's. A literal is blank in `code`, so in `matches!(c, 'a' | 'b')`,
/// `["x", "a" | "b"]` and the arm `'a' | 'b' => ..` the byte before the bar looks like the start of
/// an argument. The parameters stay inside their own brackets and hold no block: a bracket that
/// closes before the next bar, or a `{` in front of it, ends the search, and nothing is skipped (the
/// bar that was taken for a closure's skipped the calls up to the next closure in the file).
fn closure_params_end(code: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0_usize;
    for (k, c) in code.iter().enumerate().skip(open + 1) {
        match c {
            b'|' if depth == 0 => return Some(k),
            b'(' | b'[' => depth += 1,
            b')' | b']' | b'}' => depth = depth.checked_sub(1)?,
            b'{' => return None,
            _ => {}
        }
    }
    None
}

/// The calls of `code`, innermost first.
fn calls<'a>(code: &'a [u8], starts: &BTreeSet<usize>) -> Vec<Call<'a>> {
    // The open brackets: a parenthesis keeps the call it opens, `[` and `{` keep nothing.
    let mut stack: Vec<Option<Call<'_>>> = Vec::new();
    let mut out = Vec::new();
    let mut i = 0;
    while i < code.len() {
        match code[i] {
            b'(' => {
                let first = skip_blank(code, starts, i + 1);
                stack.push(Some(Call {
                    callee: ident_before(code, i),
                    qualifier: qualifier_before(code, i),
                    args: if code.get(first) == Some(&b')') {
                        Vec::new()
                    } else {
                        vec![first]
                    },
                }));
            }
            b'[' | b'{' => stack.push(None),
            b')' | b']' | b'}' => {
                if let Some(Some(call)) = stack.pop() {
                    out.push(call);
                }
            }
            b',' => {
                if let Some(Some(call)) = stack.last_mut() {
                    let next = skip_blank(code, starts, i + 1);
                    if code.get(next) != Some(&b')') {
                        call.args.push(next);
                    }
                }
            }
            // The parameters of a closure that starts an argument, follows `=`, `&`, `&mut` or
            // `move`: up to the bar that closes them.
            b'|' if matches!(byte_before(code, i), Some((_, b'(' | b',' | b'&' | b'=')))
                || matches!(ident_before(code, i), "mut" | "move") =>
            {
                if let Some(close) = closure_params_end(code, i) {
                    i = close;
                }
            }
            _ => {}
        }
        i += 1;
    }
    out
}

/// `at`, or the first place after it that is not whitespace. A string literal is blank in the code,
/// so `starts` (where each one begins) tells it from blank space: it is where the next thing starts.
fn skip_blank(code: &[u8], starts: &BTreeSet<usize>, mut at: usize) -> usize {
    while at < code.len() && code[at].is_ascii_whitespace() && !starts.contains(&at) {
        at += 1;
    }
    at
}

/// Where the value of the argument that starts at `at` starts: past a `&`.
fn value_start(code: &[u8], starts: &BTreeSet<usize>, at: usize) -> usize {
    let mut v = skip_blank(code, starts, at);
    while code.get(v) == Some(&b'&') {
        v = skip_blank(code, starts, v + 1);
    }
    v
}

/// The position of `Name` in `Name::from(` when the `(` at `open` is the one of such a call: the
/// conversion `String::from("x")` and `Cow::from("x")` wrap a literal without changing what it is.
fn from_call_start(code: &[u8], open: usize) -> Option<usize> {
    if ident_before(code, open) != "from" {
        return None;
    }
    let mut end = open;
    while end > 0 && code[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    let mut at = end - "from".len();
    while at > 0 && code[at - 1].is_ascii_whitespace() {
        at -= 1;
    }
    if at < 2 || &code[at - 2..at] != b"::" {
        return None;
    }
    let mut start = at - 2;
    while start > 0 && code[start - 1].is_ascii_whitespace() {
        start -= 1;
    }
    let mut name = start;
    while name > 0 && is_ident(code[name - 1]) {
        name -= 1;
    }
    (name < start).then_some(name)
}

/// Whether the string literal at `start` is the value of a field called `model`, `table` or
/// `collection` (`Op::Insert { model: "x".into(), .. }`), also as `&"x"` and `String::from("x")`.
fn is_field_value(code: &[u8], start: usize) -> bool {
    let mut at = start;
    loop {
        match byte_before(code, at) {
            Some((p, b'&')) => at = p,
            Some((p, b'(')) => match from_call_start(code, p) {
                Some(name) => at = name,
                None => return false,
            },
            Some((p, b':')) => {
                return matches!(ident_before(code, p), "model" | "table" | "collection")
                    && code.get(p.wrapping_sub(1)) != Some(&b':');
            }
            _ => return false,
        }
    }
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

/// What the signatures of the workspace say about the functions and methods, by name and by the
/// number of their parameters (after `self`): `at` holds, for the definitions that have a parameter
/// that takes a model, where it stands (0 is the first); `without` the name and number of parameters
/// of every definition that has none.
#[derive(Default)]
struct Signatures {
    at: BTreeMap<(String, usize), BTreeSet<usize>>,
    without: BTreeSet<(String, usize)>,
    /// The same, for the functions defined in an `impl` or a `trait` of a type, by that type: `Op`
    /// of `impl Op { pub fn add(model: &str, ..) }` tells `Op::add("x", ..)` from `Report::add(..)`.
    typed_at: BTreeMap<(String, String, usize), BTreeSet<usize>>,
    typed_without: BTreeSet<(String, String, usize)>,
}

impl Signatures {
    /// Whether some definition of `name` with `arity` parameters takes a model as its `index`th. A
    /// constant that is handed there is a model name; the author's own word for the parameter is the
    /// evidence.
    fn takes_model(&self, name: &str, arity: usize, index: usize) -> bool {
        self.at
            .get(&(name.to_string(), arity))
            .is_some_and(|i| i.contains(&index))
    }

    /// As [`Signatures::takes_model`], and every definition of `name` with `arity` parameters has it
    /// there, and the name is not one of the standard collections'. A string literal is an argument
    /// of many calls that name no table, so it is told by its shape only where the shape is not
    /// shared with something else: `new(name, ..)` is not told by a `new(table, ..)` elsewhere, and
    /// `Store::insert(model, fields)` is not told from `HashMap::insert(key, value)` ([`METHODS`]
    /// knows `insert` with three arguments).
    ///
    /// A call through a path (`Op::add(..)`) is told by the type when that type defines a function
    /// of the name, whatever the other types define.
    fn strictly_takes_model(
        &self,
        qualifier: Option<&str>,
        name: &str,
        arity: usize,
        index: usize,
    ) -> bool {
        if COLLECTION_METHODS.contains(&name) {
            return false;
        }
        if let Some(q) = qualifier {
            let key = (q.to_string(), name.to_string(), arity);
            if let Some(at) = self.typed_at.get(&key) {
                return at.contains(&index) && !self.typed_without.contains(&key);
            }
        }
        self.takes_model(name, arity, index) && !self.without.contains(&(name.to_string(), arity))
    }
}

/// The type whose `impl` or `trait` block holds the item at `at`: `Op` for a `fn` in `impl Op { .. }`
/// or `impl<T> Trait for Op<T> { .. }` or `trait Op { .. }`; `None` for a free function or an item
/// in a `mod`.
fn impl_type_of(code: &[u8], at: usize) -> Option<String> {
    // The `{` that opens the block that holds `at`.
    let mut depth = 0_usize;
    let mut open = None;
    for k in (0..at).rev() {
        match code[k] {
            b'}' => depth += 1,
            b'{' if depth == 0 => {
                open = Some(k);
                break;
            }
            b'{' => depth -= 1,
            _ => {}
        }
    }
    let open = open?;
    // Its header: back to the end of the previous item.
    let from = code[..open]
        .iter()
        .rposition(|c| matches!(c, b';' | b'}' | b'{'))
        .map_or(0, |p| p + 1);
    let header = String::from_utf8_lossy(&code[from..open]).to_string();
    let header = header.split_whitespace().collect::<Vec<_>>().join(" ");
    let header = header.split(" where ").next().unwrap_or("").trim();
    let rest = header
        .strip_prefix("impl")
        .or_else(|| header.strip_prefix("pub trait"))
        .or_else(|| header.strip_prefix("trait"))?;
    // `impl<T> Trait for Op<T>` is `Op`; `impl Op` too; a trait's own name for `trait Op: Send`.
    let rest = rest.split(" for ").last().unwrap_or(rest).trim_start();
    let rest = if rest.starts_with('<') {
        // The generics of the impl, up to their closing `>`.
        let mut d = 0_i32;
        let mut cut = rest.len();
        let mut before = ' ';
        for (k, c) in rest.char_indices() {
            let arrow = before == '-';
            before = c;
            match c {
                '<' => d += 1,
                '>' if arrow => {}
                '>' => {
                    d -= 1;
                    if d == 0 {
                        cut = k + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        rest[cut..].trim_start()
    } else {
        rest
    };
    let name: String = rest
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == ':'))
        .next()
        .unwrap_or("")
        .rsplit("::")
        .next()
        .unwrap_or("")
        .trim_end_matches(':')
        .to_string();
    (!name.is_empty()).then_some(name)
}

/// The signatures of the workspace: for every function and method, where its parameter that takes a
/// model stands. That is a parameter called `model` or `table`, or ending `_model` or `_table`
/// (`tr_model`), that takes a string. `fn put_link(&self, model: &str, id: &str, f: &Fields,
/// refresh: bool)` takes a model as its first of four; `fn localize(&self, ctx: &Ctx, rec: &Record,
/// tr_model: &str, ..)` as its third. The methods of `Backend` are in it, and so is every helper
/// the authors wrote that takes a model.
fn signatures(lexed: &BTreeMap<String, Lexed>) -> Signatures {
    let mut out = Signatures::default();
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
            if params.is_empty() {
                continue;
            }
            let key = (name.to_string(), params.len());
            let typed = impl_type_of(code, at).map(|ty| (ty, key.0.clone(), key.1));
            let models: BTreeSet<usize> = params
                .iter()
                .enumerate()
                .filter(|(_, p)| {
                    p.split_once(':').is_some_and(|(head, ty)| {
                        let head = head.trim().trim_start_matches("mut ").trim();
                        (matches!(head, "model" | "table")
                            || head.ends_with("_model")
                            || head.ends_with("_table"))
                            && ty.contains("str")
                    })
                })
                .map(|(i, _)| i)
                .collect();
            if models.is_empty() {
                out.without.insert(key);
                out.typed_without.extend(typed);
            } else {
                if let Some(typed) = typed {
                    out.typed_at
                        .entry(typed)
                        .or_default()
                        .extend(models.iter().copied());
                }
                out.at.entry(key).or_default().extend(models);
            }
        }
    }
    out
}

/// Where a constant is the value of a field `model`, `table` or `collection`
/// (`Op::Insert { model: NAME.into(), .. }`): `(offset, name, qualifier)`.
fn const_fields(code: &[u8]) -> Vec<(usize, &str, Option<&str>)> {
    let mut out = Vec::new();
    for field in ["model", "table", "collection"] {
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
                out.push((at, name, qualifier));
            }
        }
    }
    out
}

/// The constants of the name `name` that a use at `path` can mean: those defined in a file called
/// `qualifier` when the use is `qualifier::NAME`, and of those the crate's own, if it has any (the
/// workspace's otherwise). `header::ORIGIN` is no constant of ours unless a file called `header`
/// has one.
fn chosen<'a>(
    defs: &'a BTreeMap<String, Vec<Def>>,
    name: &str,
    qualifier: Option<&str>,
    path: &str,
) -> Vec<&'a Def> {
    let Some(all) = defs.get(name) else {
        return Vec::new();
    };
    let named: Vec<&Def> = all
        .iter()
        .filter(|d| match qualifier {
            Some(q) if !matches!(q, "Self" | "self" | "super" | "crate") => file_stem(&d.path) == q,
            _ => true,
        })
        .collect();
    let here: Vec<&Def> = named
        .iter()
        .copied()
        .filter(|d| crate_of(&d.path) == crate_of(path))
        .collect();
    if here.is_empty() { named } else { here }
}

fn hits(files: &BTreeMap<String, String>) -> Vec<Hit> {
    let lexed = non_test(files);
    let defs = consts(&lexed);
    let signatures = signatures(&lexed);
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
        let literals: BTreeMap<usize, &String> =
            l.strings.iter().map(|(s, _, text)| (*s, text)).collect();
        // A string literal as the value of a field called `model`, `table` or `collection`.
        for (start, _, text) in &l.strings {
            if is_table_name(text) && is_field_value(&l.code, *start) {
                out.push(Hit {
                    path: path.clone(),
                    line: line_of(&l.code, *start),
                    name: text.clone(),
                    how: "the value of a field `model`, `table` or `collection`".to_string(),
                });
            }
        }
        // A constant as the value of such a field: the crate's own constants of that name first,
        // then the workspace's.
        for (at, name, qualifier) in const_fields(&l.code) {
            for d in chosen(&defs, name, qualifier, path) {
                out.push(Hit {
                    path: path.clone(),
                    line: line_of(&l.code, at),
                    name: d.value.clone(),
                    how: format!(
                        "a constant handed as the value of a field `model`, `table` or `collection` (`{name}`, defined at {}:{})",
                        d.path, d.line
                    ),
                });
            }
        }
        // An argument of a call of a function that takes a model there: a method of the backend, or
        // any function of the workspace whose parameter at that place is a model.
        for call in calls(&l.code, &starts) {
            let arity = call.args.len();
            for (index, &arg) in call.args.iter().enumerate() {
                let value = value_start(&l.code, &starts, arg);
                if let Some(text) = literals.get(&value) {
                    let by_method = index == 0
                        && METHODS.iter().any(|(m, least, most)| {
                            *m == call.callee && (*least..=*most).contains(&arity)
                        });
                    if is_table_name(text)
                        && (by_method
                            || signatures.strictly_takes_model(
                                call.qualifier,
                                call.callee,
                                arity,
                                index,
                            ))
                    {
                        out.push(Hit {
                            path: path.clone(),
                            line: line_of(&l.code, value),
                            name: (*text).clone(),
                            how: format!(
                                "a string handed to `{}`, which takes a model there",
                                call.callee
                            ),
                        });
                    }
                } else if let Some((name, qualifier, end)) = const_path(&l.code, arg)
                    && matches!(l.code.get(skip_ws(&l.code, end)), Some(b',' | b')'))
                    && signatures.takes_model(call.callee, arity, index)
                {
                    for d in chosen(&defs, name, qualifier, path) {
                        out.push(Hit {
                            path: path.clone(),
                            line: line_of(&l.code, arg),
                            name: d.value.clone(),
                            how: format!(
                                "a constant handed to `{}`, which takes a model there (`{name}`, defined at {}:{})",
                                call.callee, d.path, d.line
                            ),
                        });
                    }
                }
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
    // The models of the built-in modules that are bound later (the accounts, the devices, single
    // sign-on) are in the installer's list of them and not yet in the dispatcher.
    out.extend(
        app.modules
            .iter()
            .flat_map(|m| m.models.iter().map(|x| x.name.to_string())),
    );
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
        // or the platform keeps it for its own records) or as a blocker: a model of a module, or the
        // id of one (the package takes the name as its id, and a name that is also the id of a
        // module of the program, such as `product`, is refused by the id).
        match app.installer.plan(Request::Manifest(&text)) {
            Err(e) => assert!(e.contains(name.as_str()), "{name}: {e}"),
            Ok((plan, _)) => assert!(
                plan.blockers
                    .iter()
                    .any(|b| b.message == format!("model {name} already exists")
                        || b.message
                            == format!("{name} is a built-in module and is part of the core")),
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

/// The functions of the workspace that take a model, as the scan reads them from the signatures: the
/// methods of the backend, a helper with a `model` parameter (the first, or a later one such as
/// `tr_model`), one with generics and a closure, a free function, an associated function, a method
/// with a query (`list`, four arguments, beside the backend's one). Not: a parameter of another name
/// or another type, a `self` that is not the first. `same` is defined twice, once with a model first
/// and once without.
/// `add` is defined twice with four parameters, once by `Op` with a model first and once by `Report`
/// without: the type in front of a path (`Op::add(..)`) tells them apart, a method call (`r.add(..)`)
/// cannot.
const SIGNATURES: &str = "trait Backend { fn insert(&self, model: &str, id: &str, f: &Fields); fn get(&self, model: &str, id: &str) -> R<Option<Fields>>; fn list(&self, model: &str) -> R<Vec<(String, Fields)>>; fn count(&self, model: &str) -> R<usize>; fn find(&self, model: &str, field: &str, value: &J) -> R<Vec<(String, Fields)>>; }\nimpl Unit { pub fn merge(&mut self, model: &str, id: &str, patch: Fields) -> R<()> { Ok(()) } fn put_link(&self, model: &str, id: &str, f: &Fields, refresh: bool) -> Result<(), String> { Ok(()) } fn scan<F: FnMut(&str, &Fields) -> bool>(&self, table: &str, visit: F) {} fn each(&self, table: &str, visit: impl Fn(&str) -> bool, last: bool) {} }\npub fn describe(model: &str, command: &str) -> Option<&'static str> { None }\nfn other(name: &str, model: &str) {}\nfn number(model: u32) {}\nfn skipped(&self, fields: &[&str], model: &str) {}\nimpl Api { fn list(&self, model: &str, filters: &[(String, F)], limit: Option<usize>, offset: usize) -> R<()> { Ok(()) } fn server_records(&self, model: &str) -> R<()> { Ok(()) } fn translations_into(&self, model: &str, ref_field: &str, lang: Lang) {} fn localize(&self, ctx: &Ctx, rec: &Rec, tr_model: &str, ref_field: &str, keys: &[&str]) {} }\nimpl Op { pub fn add(model: &str, id: &str, field: &str, delta: i64) -> Op { todo() } }\nimpl Store { fn insert(&self, model: &str, fields: Fields) -> R<String> { todo() } }\nimpl Report { fn add(&mut self, name: &'static str, about: &'static str, level: u8, detail: String) {} }\nimpl Site { fn translated(&self, site: &Site, link_table: &str) {} }\nimpl Mixed { fn put(&self, model: &str, a: u8) {} }\nimpl Mixed { fn put(&self, name: &str, a: u8) {} }\nfn same(model: &str, a: u8) {}\nfn same(name: &str, a: u8) {}\n";

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
        // A later place, the word `tr_model`, a name that a second definition shares, a query.
        "fn f() { s.skipped(&[], COUNTER); }",
        "fn f() { self.localize(ctx, r, COUNTER, \"page_id\", &[\"body\"]); }",
        "fn f() { same(COUNTER, 1); }",
        "fn f() { store.insert(COUNTER, fields); }",
        "fn f() { api.list(COUNTER, &[], Some(3), 0); }",
        "fn f() { u.each(COUNTER, |s| true, false); }",
        "fn f() { describe(COUNTER, \"get\"); }",
        "fn f() { crate::describe(COUNTER, \"get\"); }",
        // The field of an operation, with the conversions that make the text owned.
        "fn f() { Op::Merge { model: COUNTER.into(), id, patch } }",
        "fn f() { Op::Merge { model: COUNTER.to_string(), id } }",
        "fn f() { let s = S { table: Self::COUNTER, f }; }",
        "fn f() { let s = S { collection: COUNTER, f }; }",
        "fn f() { s.translated(site, COUNTER); }",
        "fn f() { Op::add(COUNTER, id, \"n\", -1); }",
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
        "fn f() { s.fetch(COUNTER, id); s.skipped(COUNTER, &[]); }",
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

/// A string literal is followed to the places a constant is, and one more: any function of the
/// workspace whose parameter at that place takes a model (the scan reads the signatures), by name and
/// number of arguments, as the first or any later one. `api.list("x", &[], Some(3), 0)`,
/// `Op::add("x", ..)`, `self.localize(ctx, r, "x", ..)` and a helper nobody listed all name a table;
/// the first three escaped the scan of the literals, which knew nine methods at one argument each.
#[test]
fn the_scan_follows_a_string_to_any_place_that_takes_a_model() {
    let names = |src: &str| -> Vec<String> {
        let mut v: Vec<String> = hits(&tree(&[
            ("kernel/obop-s/src/lib.rs", SIGNATURES),
            ("kernel/obop-b/src/lib.rs", src),
        ]))
        .into_iter()
        .map(|h| h.name)
        .collect();
        v.sort();
        v
    };
    for src in [
        // The first argument of a method that is not one of the nine, with the arguments of the
        // call counted past nested brackets, a closure, a trailing comma and line breaks.
        "fn f(u: &mut Unit) { u.merge(\"x_lit\", id, patch); }",
        "fn f() { self.put_link(\"x_lit\", k, &m, true); }",
        "fn f() { needed(|| self.put_link(\"x_lit\", k, &m, true))?; }",
        "fn f() { b.scan(\"x_lit\", |a, b| true); }",
        "fn f() { b.scan(\"x_lit\", &mut |a, b| true); }",
        "fn f() { u.each(\"x_lit\", |s| true, false); }",
        "fn f() { describe(\"x_lit\", \"get\"); }",
        "fn f() { crate::describe(\"x_lit\", \"get\"); }",
        "fn f() { Op::add(\"x_lit\", id, \"n\", -1).min(0); }",
        "fn f() { api.list(\"x_lit\", &[], Some(3), 0); }",
        "fn f() { api.list(\"x_lit\", &[(\"a\".to_string(), g(1, 2))], None, 0); }",
        "fn f() { api\n    .list(\n        \"x_lit\",\n        &[],\n        Some(3),\n        0,\n    )\n    .unwrap(); }",
        "fn f() { api.server_records(\"x_lit\"); }",
        // A parameter that ends `_table`, in a later place.
        "fn f() { s.translated(site, \"x_lit\"); }",
        "fn f() { self.translations_into(\"x_lit\", \"listing_id\", ctx.lang); }",
        "fn f() { b.get(&\"x_lit\", id); }",
        // After an or-pattern of literals, with a closure further on: the bar of the pattern is not
        // a closure's, and the call between it and the closure is read.
        "fn f() { if matches!(t(&r, \"k\"), \"a\" | \"b\") { ops.push(Op::add(\"x_lit\", id, \"n\", -1).min(0)); } let g = |z| z; }",
        "fn f() { if matches!(c, 'a' | 'b') { b.get(\"x_lit\", id); } let g = |z| z; }",
        // Where the parameter is not the first: its place counts, and the word `tr_model` does.
        "fn f() { self.localize(ctx, r, \"x_lit\", \"page_id\", &[\"body\"]); }",
        "fn f() { s.skipped(&[], \"x_lit\"); }",
        "fn f() { other(a, \"x_lit\"); }",
        // The value of a field called `model`, `table` or `collection`, made owned in the ways
        // there are, or borrowed.
        "fn f() { Op::Insert { model: String::from(\"x_lit\"), id, fields } }",
        "fn f() { Op::Insert { model: Cow::from(\"x_lit\"), id } }",
        "fn f() { Op::Insert { model: \"x_lit\".to_string(), id } }",
        "fn f() { S { table: \"x_lit\".to_owned() } }",
        "fn f() { S { collection: \"x_lit\", f } }",
        "fn f() { S { model: &\"x_lit\" } }",
    ] {
        assert_eq!(names(src), ["x_lit"], "{src}");
    }
    // Not a table: a function that no signature says takes a model there, a call with another number
    // of arguments, the place before or after the one that takes it, a string that is no table name,
    // and the shapes that have nothing to do with a call.
    for src in [
        "fn f() { g(\"x_lit\", id); number(\"x_lit\"); }",
        "fn f() { s.fetch(\"x_lit\", id); b.get(id, \"x_lit\"); }",
        // The parameter is the second of `other` and of `skipped`, the first of `describe` and `list`.
        "fn f() { other(\"x_lit\", id); s.skipped(\"x_lit\", &[]); }",
        "fn f() { describe(command, \"x_lit\"); api.list(&[], \"x_lit\", None, 0); }",
        // The number of arguments tells the call from a method of the same name on something else.
        "fn f() { api.list(\"x_lit\", &[], Some(3)); api.list(\"x_lit\", &[], Some(3), 0, 1); }",
        "fn f() { self.localize(ctx, r, \"x_lit\", \"p\"); }",
        "fn f() { m.get(\"x_lit\"); m.get(\"x_lit\", a, b); m.list(\"x_lit\", 1); }",
        // `Store::insert(model, fields)` and `HashMap::insert(key, value)` look alike.
        "fn f() { store.insert(\"x_lit\", fields); map.insert(\"x_lit\", 1); }",
        // A name that two definitions give a different first parameter is not told by its shape.
        "fn f() { same(\"x_lit\", 1); }",
        // `add` with four arguments is `Op::add(model, ..)` and `Report::add(name, ..)`: a method call
        // is not told, and a path to a type that does not define it is not told either.
        "fn f() { report.add(\"x_lit\", \"about\", 1, d); Other::add(\"x_lit\", \"a\", 1, d); }",
        "fn f() { s.translated(\"x_lit\", site); }",
        // Two definitions of the same type and name that differ: not told by the type either.
        "fn f() { Mixed::put(\"x_lit\", 1); }",
        // A parameter that is not a string.
        "fn f() { number(\"x_lit\"); }",
        "fn f() { api.list(\"Not A Table\", &[], None, 0); }",
        // No call: a tuple, an array, a variant, a macro, a field of another name, a comment.
        "fn f() { let t = (\"x_lit\", 1); let v = vec![\"x_lit\"]; let o = Some(\"x_lit\"); println!(\"x_lit\"); }",
        "fn f() { let s = S { name: \"x_lit\", model_name: \"x_lit\" }; }",
        "fn f() { api.list(/* \"x_lit\" */ &[], None, 0); }",
        "#[cfg(test)]\nmod tests { fn t() { api.list(\"x_lit\", &[], None, 0); } }",
    ] {
        assert!(names(src).is_empty(), "{src}: {:?}", names(src));
    }
}

/// The type that an `impl` or a `trait` block is for, as the scan reads it to tell `Op::add(..)` from
/// `Report::add(..)`.
#[test]
fn the_type_of_the_block_that_holds_a_function_is_read_from_its_header() {
    let of = |src: &str| -> Option<String> {
        let code = src.as_bytes();
        let at = src.find("fn x").unwrap();
        impl_type_of(code, at)
    };
    for (src, want) in [
        ("impl Op { fn x() {} }", Some("Op")),
        ("impl<T> Op<T> { fn x() {} }", Some("Op")),
        ("impl<T: Clone> Trait for Op<T> { fn x() {} }", Some("Op")),
        ("impl<F: Fn(&str) -> bool> Op<F> { fn x() {} }", Some("Op")),
        ("impl crate::ops::Op { fn x() {} }", Some("Op")),
        ("impl Op\nwhere\n    T: Send,\n{ fn x() {} }", Some("Op")),
        ("trait Op: Send { fn x(); }", Some("Op")),
        ("pub trait Backend { fn x(); }", Some("Backend")),
        // The block of an earlier item is closed, a nested block is the one that holds the function.
        (
            "impl A { fn y() { if a { b } } } impl B { fn x() {} }",
            Some("B"),
        ),
        ("impl A { fn y() { if a { fn x() {} } } }", None),
        // A free function, and one in a module.
        ("fn x() {}", None),
        ("mod m { fn x() {} }", None),
    ] {
        assert_eq!(of(src).as_deref(), want, "{src}");
    }
}

/// The scan reads the real definitions: the functions the workspace has that take a model, by the type
/// they are defined in. `Op::add` shares its name and its four parameters with other `add`s, and is
/// told from them by `Op`.
#[test]
fn the_scan_reads_where_the_real_functions_that_take_a_model_are_defined() {
    let lexed = non_test(&source_tree());
    let sigs = signatures(&lexed);
    let key = ("Op".to_string(), "add".to_string(), 4);
    let at = sigs.typed_at.get(&key);
    assert!(
        at.is_some_and(|at| at.contains(&0)),
        "the scan does not know that `Op::add` takes its model first: {at:?}"
    );
    assert!(
        !sigs.typed_without.contains(&key),
        "another `Op::add` with four parameters takes no model"
    );
    assert!(sigs.strictly_takes_model(Some("Op"), "add", 4, 0));
}

/// A table added to the code that is really there is reported: a name inserted next to a real
/// `Op::add`, which follows a `matches!` with an or-pattern of literals in the same function (the
/// bar of the pattern was once taken for a closure's, and the scan skipped the rest of the file up to
/// the next closure), and one next to a real `api.list`.
#[test]
fn a_table_added_next_to_a_real_call_of_the_code_is_reported() {
    for (path, anchor, added) in [
        (
            "kernel/obop-payments/src/lib.rs",
            "ops.push(Op::add(\"ecommerce_discount_use\", usage, \"count\", -1).min(0));",
            "ops.push(Op::add(\"zz_new_table\", usage, \"count\", -1).min(0));",
        ),
        (
            "kernel/obop-site/src/lib.rs",
            "let Ok((rows, _)) = self.api.list(\"website_menu_item\", &[], Some(50), 0) else {",
            "let _ = self.api.list(\"zz_new_table\", &[], Some(50), 0);",
        ),
    ] {
        let mut files = source_tree();
        let src = files.get_mut(path).expect("the file is in the tree");
        assert_eq!(src.matches(anchor).count(), 1, "{path}: the anchor moved");
        *src = src.replace(anchor, &format!("{added} {anchor}"));
        let (unreserved, _) = check(&files, &modules_models());
        assert!(
            unreserved.iter().any(|u| u.contains("`zz_new_table`")),
            "{path}: a table added next to {anchor} is not reported: {unreserved:?}"
        );
    }
}

/// The calls of the source are read with the arguments they have.
#[test]
fn the_calls_of_a_source_are_read_with_their_arguments() {
    let calls_of = |src: &str| -> Vec<(String, usize)> {
        let l = crate::src_scan::lex(src);
        let starts: BTreeSet<usize> = l.strings.iter().map(|(s, _, _)| *s).collect();
        let mut v: Vec<(String, usize)> = calls(&l.code, &starts)
            .into_iter()
            .filter(|c| !c.callee.is_empty())
            .map(|c| (c.callee.to_string(), c.args.len()))
            .collect();
        v.sort();
        v
    };
    for (src, want) in [
        ("f(a, b, c)", vec![("f", 3)]),
        ("f()", vec![("f", 0)]),
        // A trailing comma makes no argument; a literal is an argument though it is blank.
        ("f(a, b,)", vec![("f", 2)]),
        ("f(\"a\", \"b\")", vec![("f", 2)]),
        ("f(\"\")", vec![("f", 1)]),
        // The commas of a closure's parameters, of an array and of a block are no arguments.
        ("f(|a, b| a, c)", vec![("f", 2)]),
        ("f(c, move |a, b| a)", vec![("f", 2)]),
        ("f(c, &mut |a, b| a)", vec![("f", 2)]),
        ("f(&[a, b], c)", vec![("f", 2)]),
        ("f(|(a, b), c| a, d)", vec![("f", 2)]),
        ("f(|[a, b], c| a, d)", vec![("f", 2)]),
        ("f({ a, b }, c)", vec![("f", 2)]),
        // The bar of an or-pattern after a literal is no closure's: nothing up to the next closure
        // is skipped (a char, a string, an array, the arm of a `match`).
        (
            "f(matches!(c, 'a' | 'b'), g(1, 2), |z| z)",
            vec![("f", 3), ("g", 2)],
        ),
        (
            "f(matches!(c, \"a\" | \"b\"), g(1, 2), |z| z)",
            vec![("f", 3), ("g", 2)],
        ),
        (
            "f(&[\"x\", \"a\" | \"b\"], g(1, 2), |z| z)",
            vec![("f", 3), ("g", 2)],
        ),
        (
            "match c { 'x' => 1, 'a' | 'b' => { g(1, 2); } _ => 0 }; let h = |z| z;",
            vec![("g", 2)],
        ),
        (
            "match c { 'x' => 1, 'a' | 'b' => 2, _ => 3 } g(1, 2); let h = |z| z;",
            vec![("g", 2)],
        ),
        (
            "match c { 'x' => 1, 'a' | 'b' => { g(1, 2); let h = |z| z; } _ => 0 }",
            vec![("g", 2)],
        ),
        // Calls inside calls, each with its own.
        ("f(g(a, b), h(c))", vec![("f", 2), ("g", 2), ("h", 1)]),
        ("o.f(a).g(b, c)", vec![("f", 1), ("g", 2)]),
    ] {
        let want: Vec<(String, usize)> =
            want.into_iter().map(|(n, c)| (n.to_string(), c)).collect();
        assert_eq!(calls_of(src), want, "{src}");
    }
}

/// The scan reads every crate: a crate that spells no listed name could drop out of the tree and
/// nothing else would notice. (The tree is read from `crates/`; each crate is a directory with a
/// `Cargo.toml`, and the files of its tests and of test modules are left out.)
#[test]
fn the_scan_reads_at_least_one_file_of_every_crate() {
    let files = source_tree();
    let lexed = crate::src_scan::non_test(&files);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut crates = Vec::new();
    for group in ["kernel", "modules"] {
        for entry in std::fs::read_dir(root.join(group)).unwrap() {
            let dir = entry.unwrap().path();
            if dir.join("Cargo.toml").exists() {
                crates.push(format!(
                    "{group}/{}",
                    dir.file_name().unwrap().to_str().unwrap()
                ));
            }
        }
    }
    crates.push("obop".to_string());
    assert!(
        crates.len() > 30,
        "found {} crates: {crates:?}",
        crates.len()
    );
    let unread: Vec<&String> = crates
        .iter()
        .filter(|c| !lexed.keys().any(|path| crate_of(path) == c.as_str()))
        .collect();
    assert!(
        unread.is_empty(),
        "the scan reads no file of these crates: {unread:?}"
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
            "kernel/obop-x/src/lib.rs:3: `brand_new_table` as a string handed to `get`, which takes a model there",
            "kernel/obop-x/src/lib.rs:7: `ecommerce_new` as a string handed to `get`, which takes a model there",
            "kernel/obop-y/src/lib.rs:1: `fresh_table` as the value of a constant that names a table or a model",
            "kernel/obop-y/src/lib.rs:3: `fresh_counter` as a constant handed to `get`, which takes a model there (`NUMBER`, defined at kernel/obop-y/src/lib.rs:2)",
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
