//! The scan that keeps the instance brand to one reader (docs/spec/branding.md, sections 4 and 13:
//! "no code outside the module `obop_commands::brand` reads the model `instance_brand`"). The
//! record is public by design and every surface that shows a name reads it through the one
//! reader, `Dispatcher::brand()`, with its cache, its last good value and its fail-closed
//! `strict()`. A second reader (a page, a handler or the shop that asks the store for the row
//! itself) would show a value the reader would have refused, and none of the pins of an instance
//! with no brand row would notice, because they render without a row.
//!
//! **What it catches.** In the non-test Rust code of every crate under `crates/` (walked
//! recursively; test code, the files of test-only modules and `tests/` and `benches/` directories
//! are left out, comments are blanked; see `src_scan.rs`), outside the files named in [`ALLOWED`]:
//! - the model name `instance_brand`, in code (it is part of `instance_brand_model`) or in a
//!   string literal;
//! - a path that starts at the module `brand` (`brand::strict`, `crate::brand::Reader`,
//!   `obop_commands::brand::apply`; not `obop_brand::`, the leaf crate of the rules, which has no
//!   store);
//! - a call `.brand()` or `::brand(` (the reader of the dispatcher), the same name used as a value
//!   (`.map(Dispatcher::brand)`, `use obop_commands::brand;`) and the identifier `install_brand`
//!   (what creates it).
//!
//! **A row of [`ALLOWED`] is the decision that a file may read or write the brand.** It names the
//! file and the reason, and it must match something: a row that no longer matches is a failure, so
//! the list cannot go stale. The slice that makes a surface show the brand adds the file of that
//! surface here, with its reason, in the commit that wires it.
//!
//! **A row carries a [`Limit`].** The files of the module, the model and the command line may do
//! anything ([`Limit::Any`]). A file of a surface that only asks the reader ([`Limit::Reads`]) may
//! call `brand()` as many times as the row says and name the paths through the module `brand` that
//! the row lists, and nothing else: so the reason of such a row ("it reads nothing", "the route
//! asks the reader once") is held by the scan, and a second reader added to a large file such as
//! `obop-http/src/lib.rs` is found.
//!
//! **What it does not see.** It is a textual scan, not a parser. Text built at run time
//! (`format!("{}_brand", "instance")`), the model reached through a constant of another crate that
//! is not spelled in the file, a reader passed in as a trait object by a file that is on the list,
//! and Rust outside `crates/` are not seen. It makes a second reader visible; it does not stop one
//! who works around it. The behaviour is held by `crates/obop/src/brand_row_tests.rs`: with a
//! row, every output that is pinned is what it was without one (the logo path is the exception: it
//! answers the artwork with a row, `crates/obop/src/brand_http_tests.rs`).

use crate::src_scan::{Lexed, line_of, non_test, read_tree, tree, words};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// What a row of [`ALLOWED`] lets its file do with the brand.
#[derive(Clone, Copy)]
enum Limit {
    /// Anything: the file belongs to the module, or acts on the row as its owner.
    Any,
    /// Ask the reader and nothing else: at most `calls` calls of `brand()`, and paths through the
    /// module `brand` only to the names in `paths`. The model, `install_brand`, any other path and
    /// the accessor named without a call are not allowed.
    Reads {
        calls: usize,
        paths: &'static [&'static str],
    },
}

/// Where the brand is read or written: the path from `crates/` (a prefix when it ends with `/`),
/// why, and what the file may do.
const ALLOWED: [(&str, &str, Limit); 7] = [
    (
        "kernel/obop-commands/src/brand/",
        "the module itself: the reader, `strict()`, `apply` and the read of `get`",
        Limit::Any,
    ),
    (
        "kernel/obop-commands/src/lib.rs",
        "the dispatcher holds the reader (one cell per dispatcher) and takes the text of `get` from the module",
        Limit::Any,
    ),
    (
        "kernel/obop-pkg/src/brand.rs",
        "the model `instance_brand` and the handlers of `get`, `configure` and `clear`, with the guard",
        Limit::Any,
    ),
    (
        "kernel/obop-pkg/src/installer.rs",
        "the built-in module `platform` lists the model, and `register_admin` creates the reader",
        Limit::Any,
    ),
    (
        "kernel/obop-http/src/brand.rs",
        "the logo route `GET /api/brand/logo.svg` and `current()`: the route asks the reader once, through the blocking pool",
        Limit::Reads {
            calls: 1,
            paths: &["Reader", "Shown"],
        },
    ),
    (
        "kernel/obop-http/src/lib.rs",
        "`api_routes` registers the logo route (`brand::routes`) and passes it the answer of the layout to an unknown path; it reads nothing",
        Limit::Reads {
            calls: 0,
            paths: &["routes", "NotFound"],
        },
    ),
    (
        "obop/src/brand_cmd.rs",
        "`obop admin brand`, which acts on the database as the operator (spec, section 3)",
        Limit::Any,
    ),
];

fn allowed(path: &str) -> Option<usize> {
    ALLOWED.iter().position(|(p, _, _)| {
        if p.ends_with('/') {
            path.starts_with(p)
        } else {
            path == *p
        }
    })
}

/// The leaf crate of the rules is not scanned: it has no store and no dependency on the crates that
/// have one (a test below reads its manifest), and it has a module of its own called `brand`.
const LEAF: &str = "kernel/obop-brand/";

const MODEL: &str = "the model `instance_brand`";
const MODEL_LITERAL: &str = "the model `instance_brand` in a literal";
const INSTALL: &str = "`install_brand`";
const PATH: &str = "a path through the module `brand`";
const CALL: &str = "a call of the reader accessor `brand()`";
const VALUE: &str = "the module `brand` or the reader accessor `brand` named without a call";

/// What in a file names the row or its reader: the offset, what was found and, for a path through
/// the module `brand`, the names it goes to (`brand::Reader` goes to `Reader`, `brand::{A, b::C}`
/// to `A` and `b`; a `*` is the empty name, which no row lists).
struct Hit {
    at: usize,
    what: &'static str,
    names: Vec<String>,
}

/// The names that a path through `brand::` goes to, from the offset just after the `::`.
fn targets(code: &[u8], from: usize) -> Vec<String> {
    use crate::src_scan::{is_ident, skip_ws, word_at};
    let at = skip_ws(code, from);
    if code.get(at) != Some(&b'{') {
        return vec![word_at(code, at).to_string()];
    }
    let (mut names, mut depth, mut entry) = (Vec::new(), 0usize, true);
    for (i, c) in code.iter().enumerate().skip(at) {
        match c {
            b'{' => {
                depth += 1;
                entry = true;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            b',' if depth == 1 => entry = true,
            c if c.is_ascii_whitespace() => {}
            c => {
                if entry && depth == 1 {
                    names.push(if is_ident(*c) {
                        word_at(code, i).to_string()
                    } else {
                        String::new()
                    });
                }
                entry = false;
            }
        }
    }
    names
}

fn readers(l: &Lexed) -> Vec<Hit> {
    let code = &l.code;
    let mut out = Vec::new();
    let find = |needle: &str| {
        let n = needle.as_bytes();
        let mut at = Vec::new();
        let mut from = 0;
        while let Some(rel) = code[from..].windows(n.len()).position(|w| w == n) {
            at.push(from + rel);
            from += rel + n.len();
        }
        at
    };
    let mut hit = |at: usize, what: &'static str, names: Vec<String>| {
        out.push(Hit { at, what, names });
    };
    for at in find("instance_brand") {
        hit(at, MODEL, vec![]);
    }
    for (start, _, text) in &l.strings {
        if text.contains("instance_brand") {
            hit(*start, MODEL_LITERAL, vec![]);
        }
    }
    for at in words(code, "install_brand") {
        hit(at, INSTALL, vec![]);
    }
    for at in words(code, "brand") {
        let after = crate::src_scan::skip_ws(code, at + "brand".len());
        let before = code[..at]
            .iter()
            .rposition(|c| !c.is_ascii_whitespace())
            .map(|p| (p, code[p]));
        let dot = before.is_some_and(|(_, c)| c == b'.');
        let colons = before.is_some_and(|(p, c)| c == b':' && p > 0 && code[p - 1] == b':');
        if code[after..].starts_with(b"::") {
            hit(at, PATH, targets(code, after + 2));
        } else if code.get(after) == Some(&b'(') && (dot || colons) {
            hit(at, CALL, vec![]);
        } else if colons {
            // `Dispatcher::brand` handed on (`.map(Dispatcher::brand)`), or `use x::brand;`.
            hit(at, VALUE, vec![]);
        }
    }
    out.sort_by_key(|h| h.at);
    out
}

/// The hits that the row `limit` does not let its file make, with what the row allows in words.
fn beyond(limit: Limit, hits: &[Hit]) -> Vec<(&Hit, String)> {
    let Limit::Reads { calls, paths } = limit else {
        return Vec::new();
    };
    let mut asked = 0;
    let mut out = Vec::new();
    for h in hits {
        let said = match h.what {
            PATH => {
                let off: Vec<&str> = h
                    .names
                    .iter()
                    .map(String::as_str)
                    .filter(|n| !paths.contains(n))
                    .collect();
                (!off.is_empty()).then(|| format!("to {off:?}, the row lists only {paths:?}"))
            }
            CALL => {
                asked += 1;
                (asked > calls).then(|| format!("call number {asked}, the row lets it ask {calls}"))
            }
            _ => Some("the row lets the file ask the reader and nothing else".to_string()),
        };
        if let Some(said) = said {
            out.push((h, said));
        }
    }
    out
}

/// Every place outside the allowed files, and every place beyond the limit of an allowed one, as
/// `path:line: what`, and the allowed rows that match nothing.
fn scan(files: &BTreeMap<String, String>) -> (Vec<String>, Vec<String>) {
    let lexed = non_test(files);
    let (mut found, mut used) = (Vec::new(), BTreeSet::new());
    for (path, l) in &lexed {
        if path.starts_with(LEAF) {
            continue;
        }
        let hits = readers(l);
        if hits.is_empty() {
            continue;
        }
        match allowed(path) {
            Some(i) => {
                used.insert(i);
                for (h, said) in beyond(ALLOWED[i].2, &hits) {
                    found.push(format!(
                        "{path}:{}: {} ({said})",
                        line_of(&l.code, h.at),
                        h.what
                    ));
                }
            }
            None => {
                for h in hits {
                    found.push(format!("{path}:{}: {}", line_of(&l.code, h.at), h.what));
                }
            }
        }
    }
    let stale = ALLOWED
        .iter()
        .enumerate()
        .filter(|(i, _)| !used.contains(i))
        .map(|(_, (p, why, _))| format!("{p} ({why})"))
        .collect();
    (found, stale)
}

#[test]
fn nothing_outside_the_brand_module_reads_the_row_or_the_reader() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = BTreeMap::new();
    read_tree(&root, &root, &mut files);
    let (found, stale) = scan(&files);
    assert!(
        found.is_empty(),
        "the instance brand is read or written outside the module that owns it (add the file to \
         ALLOWED with its reason only if it is meant to):\n{}",
        found.join("\n")
    );
    assert!(
        stale.is_empty(),
        "rows of ALLOWED that match nothing (the scan reads the wrong thing, or the row is stale):\n{}",
        stale.join("\n")
    );
}

#[test]
fn the_leaf_crate_that_is_not_scanned_has_no_way_to_the_store() {
    let manifest = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../obop-brand/Cargo.toml"),
    )
    .unwrap();
    let dependencies = manifest
        .split("[dependencies]")
        .nth(1)
        .and_then(|rest| rest.split("\n[").next())
        .unwrap();
    let names: Vec<&str> = dependencies
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| l.split('=').next())
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .collect();
    assert_eq!(names, ["icu_normalizer", "icu_properties", "sha2"]);
}

#[test]
fn the_scan_sees_the_ways_a_file_can_read_the_row_and_leaves_the_allowed_files_alone() {
    // Mutation of the review: the public site reads the row itself.
    for (src, what) in [
        (
            r#"fn site_settings(b: &dyn Backend) { let _ = b.get("instance_brand", "instance"); }"#,
            "the model `instance_brand` in a literal",
        ),
        (
            "fn f() { let _ = instance_brand_model(); }",
            "the model `instance_brand`",
        ),
        (
            "use obop_commands::brand::Reader; fn f(_: &Reader) {}",
            "a path through the module `brand`",
        ),
        (
            "use obop_commands::{brand, Dispatcher}; fn f() { brand::strict(b); }",
            "a path through the module `brand`",
        ),
        (
            "fn f(d: &Dispatcher) { let _ = d.brand().current(); }",
            "a call of the reader accessor `brand()`",
        ),
        (
            "fn f(d: &Dispatcher) { let _ = d\n    .brand()\n    .current(); }",
            "a call of the reader accessor `brand()`",
        ),
        (
            "fn f(d: &Dispatcher) { let _ = Dispatcher::brand(d); }",
            "a call of the reader accessor `brand()`",
        ),
        (
            "fn f(d: &Dispatcher) { d.install_brand(b); }",
            "`install_brand`",
        ),
        // The accessor named without a call, and the module imported by name.
        (
            "fn f(r: Option<&Dispatcher>) { let _ = r.map(Dispatcher::brand); }",
            VALUE,
        ),
        (
            "fn f() { let _f = obop_commands::Dispatcher::brand; }",
            VALUE,
        ),
        ("use obop_commands::brand;", VALUE),
        ("use obop_commands::brand as b;", VALUE),
    ] {
        let (found, _) = scan(&tree(&[("kernel/obop-site/src/lib.rs", src)]));
        assert_eq!(found.len(), 1, "{src}: {found:?}");
        assert!(found[0].contains(what), "{src}: {found:?}");
    }
    // What is not a reader of the row: the leaf crate of the rules, a local called `brand`, a
    // comment, a doc comment, a name that only ends in the word, and test code.
    for src in [
        "use obop_brand::policy::clean; fn f() { let _ = obop_brand::theme::resolve(\"#fff\"); }",
        "fn f(brand: &Brand) -> &str { brand.name() }",
        "fn f() { let brand = 1; let _ = brand + 1; }",
        "// the instance_brand model, d.brand() and brand::strict are named here\nfn f() {}",
        "/// reads instance_brand through brand::strict\nfn f() {}",
        "fn f() { let _ = other_brand(); let _ = x.my_brand(); }",
        "struct S { brand: Brand } fn f(s: &S) -> &Brand { &s.brand }",
        "#[cfg(test)]\nmod tests { fn t(d: &Dispatcher) { d.brand(); b.get(\"instance_brand\", \"instance\"); } }",
    ] {
        let (found, _) = scan(&tree(&[("kernel/obop-site/src/lib.rs", src)]));
        assert!(found.is_empty(), "{src}: {found:?}");
    }
    // The files on the list may, and a row that matches nothing is reported.
    let files = tree(&[
        (
            "kernel/obop-commands/src/brand/apply.rs",
            "fn f() { let _ = \"instance_brand\"; }",
        ),
        (
            "kernel/obop-pkg/src/brand.rs",
            "fn f() { instance_brand_model(); }",
        ),
        (
            "kernel/obop-pkg/src/installer.rs",
            "fn f(d: &Dispatcher) { d.install_brand(b); }",
        ),
        (
            "kernel/obop-commands/src/lib.rs",
            "fn f() { crate::brand::describe(); }",
        ),
        (
            "kernel/obop-http/src/brand.rs",
            "async fn f(s: &S) { let _ = s.dispatcher.brand().current(); let _ = obop_commands::brand::Reader::inert(); }",
        ),
        (
            "kernel/obop-http/src/lib.rs",
            "fn f() { let _ = brand::routes(g); }",
        ),
    ]);
    let (found, stale) = scan(&files);
    assert!(found.is_empty(), "{found:?}");
    assert_eq!(stale.len(), 1, "{stale:?}");
    assert!(stale[0].starts_with("obop/src/brand_cmd.rs"), "{stale:?}");
}

#[test]
fn a_file_of_a_test_module_may_name_the_row() {
    // The files of a module declared `#[cfg(test)]` and the directories named `tests` are test
    // code: the tests of the brand read the row to arrange their cases.
    let files = tree(&[
        (
            "kernel/obop-site/src/lib.rs",
            "#[cfg(test)]\nmod brand_tests;\nfn f() {}",
        ),
        (
            "kernel/obop-site/src/brand_tests.rs",
            "fn t(b: &dyn Backend) { b.get(\"instance_brand\", \"instance\"); }",
        ),
        (
            "kernel/obop-site/tests/brand.rs",
            "fn t(d: &Dispatcher) { d.brand(); }",
        ),
    ]);
    let (found, _) = scan(&files);
    assert!(found.is_empty(), "{found:?}");
}

/// The paths of the rows with limits: a file that was allowed to register a route is not thereby
/// allowed to read the brand (spec, section 13, "The scans").
#[test]
fn a_row_with_limits_lets_its_file_ask_the_reader_as_the_row_says_and_nothing_else() {
    let lib = "kernel/obop-http/src/lib.rs";
    let route = "kernel/obop-http/src/brand.rs";
    // What the row says is allowed.
    for (path, src) in [
        (
            lib,
            "fn f() { let _ = brand::routes(g); } type T = brand::NotFound;",
        ),
        (lib, "use brand::{routes, NotFound}; fn f() {}"),
        (
            lib,
            "mod brand; fn f() { let brand = 1; let _ = brand + 1; }",
        ),
        (
            route,
            "use obop_commands::brand::{Reader, Shown}; async fn f(s: &S) { let _ = s.dispatcher.brand(); }",
        ),
        (route, "fn f(r: &obop_commands::brand::Reader) {}"),
    ] {
        let (found, _) = scan(&tree(&[(path, src)]));
        assert!(found.is_empty(), "{path}: {src}: {found:?}");
    }
    // What it does not say: every one of these is found, once, with the reason.
    for (path, src, what) in [
        // A second reader in the file that registers the route (mutation S4 of the review).
        (
            lib,
            "fn probe(s: &S) { let _ = s.dispatcher.brand().current(); }",
            CALL,
        ),
        (lib, "fn f() { brand::strict(b); }", PATH),
        (
            lib,
            "fn f() { let _ = obop_commands::brand::Reader::inert(); }",
            PATH,
        ),
        (lib, "use obop_commands::brand::{routes, Reader};", PATH),
        (lib, "use obop_commands::brand::*;", PATH),
        (lib, "fn f(d: &Dispatcher) { d.install_brand(b); }", INSTALL),
        (lib, "fn f() { let _ = instance_brand_model(); }", MODEL),
        (
            lib,
            "fn f(b: &dyn Backend) { let _ = b.get(\"instance_brand\", \"instance\"); }",
            MODEL_LITERAL,
        ),
        (
            lib,
            "fn f(r: Option<&Dispatcher>) { let _ = r.map(Dispatcher::brand); }",
            VALUE,
        ),
        (lib, "use obop_commands::brand;", VALUE),
        // The route file asks once, through `current()`, and reads the paths of its types only.
        (
            route,
            "async fn f(s: &S) { let _ = s.dispatcher.brand(); let _ = s.dispatcher.brand(); }",
            CALL,
        ),
        (
            route,
            "fn f() { let _ = obop_commands::brand::strict(b); }",
            PATH,
        ),
        (
            route,
            "fn f(b: &dyn Backend) { b.get(\"instance_brand\", \"instance\"); }",
            MODEL_LITERAL,
        ),
        (
            route,
            "fn f(d: &Dispatcher) { d.install_brand(b); }",
            INSTALL,
        ),
        (
            route,
            "fn f(r: Option<&Dispatcher>) { let _ = r.map(Dispatcher::brand); }",
            VALUE,
        ),
    ] {
        let (found, _) = scan(&tree(&[(path, src)]));
        assert_eq!(found.len(), 1, "{path}: {src}: {found:?}");
        assert!(found[0].contains(what), "{path}: {src}: {found:?}");
        assert!(
            found[0].contains("the row"),
            "the reason is the row's: {found:?}"
        );
    }
    // The count is of the file: the second call is the one that is found, with its line.
    let (found, _) = scan(&tree(&[(
        route,
        "fn a(s: &S) { s.dispatcher.brand(); }\nfn b(s: &S) { s.dispatcher.brand(); }",
    )]));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].starts_with(&format!("{route}:2:")), "{found:?}");
    // Test code of those files is left alone, as everywhere.
    let (found, _) = scan(&tree(&[(
        lib,
        "#[cfg(test)]\nmod tests { fn t(s: &S) { s.dispatcher.brand(); brand::strict(b); } }",
    )]));
    assert!(found.is_empty(), "{found:?}");
}

/// The rows with limits are the two files of the logo route and no others: loosening one of them to
/// `Limit::Any` is a change of this test, and so a decision that shows in the diff.
#[test]
fn the_rows_with_limits_are_the_two_files_of_the_logo_route() {
    let limited: Vec<&str> = ALLOWED
        .iter()
        .filter(|(_, _, l)| matches!(l, Limit::Reads { .. }))
        .map(|(p, _, _)| *p)
        .collect();
    assert_eq!(
        limited,
        [
            "kernel/obop-http/src/brand.rs",
            "kernel/obop-http/src/lib.rs"
        ]
    );
}
