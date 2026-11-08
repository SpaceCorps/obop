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
//!   (`.map(Dispatcher::brand)`, `use obop_commands::brand;`), the module imported inside a group
//!   or under another name (`use obop_commands::{brand as bb, Dispatcher};`: a `brand` in a `use`
//!   that no `::` follows, so a later `bb::strict(b)`, which names no `brand`, is found at the
//!   import) and the identifier `install_brand` (what creates it).
//!
//! **A row of [`ALLOWED`] is the decision that a file may read or write the brand.** It names the
//! file and the reason, and it must match something: a row that no longer matches is a failure, so
//! the list cannot go stale. The slice that makes a surface show the brand adds the file of that
//! surface here, with its reason, in the commit that wires it.
//!
//! **A row carries a [`Limit`].** The files of the module, the model and the command line may do
//! anything ([`Limit::Any`]). A file of a surface that only asks the reader ([`Limit::Reads`]) may
//! call `brand()` as many times as the row says, name the paths through the module `brand` that
//! the row lists, call `.current(`, `.fresh(` and `.strict(` as many times in all as the row says,
//! and name the types the row lists as types only, and nothing else. A type "as a type only" is
//! never reached by `::` (`Reader::new`, `Reader::with_bound`, `Reader::inert`, `<Reader>::inert`
//! or any other function, which would build a second reader or reach round the one the file was
//! handed; the closers of the type are looked past, so `<(Reader)>::inert` and `<Reader<>>::inert`
//! are the same as `<Reader>::inert`), never given a second name (`use ...::Reader as Rd;`,
//! `type Rd = Reader;`, `type Rd = (Reader);`) and never implemented for (`impl Tr for Reader`,
//! `impl Reader`, on one line or across several). So the reason of such a row ("it reads
//! nothing", "the route asks the reader once and builds none") is held by the scan, and a second
//! reader added to a large file such as `obop-http/src/lib.rs` is found.
//!
//! **What it does not see.** It is a textual scan, not a parser. Text built at run time
//! (`format!("{}_brand", "instance")`), the model reached through a constant of another crate that
//! is not spelled in the file, a reader passed in as a trait object by a file that is on the list,
//! a reader built through a trait without the name of its type (`Default::default()`; `Reader` has
//! no `Default`, and a release that gives it one must extend the rules above), a type named inside
//! another (`type Rd = Arc<Reader>;` names the type and builds nothing; what `Rd::new` reaches
//! is `Arc`'s), and Rust outside `crates/` are not seen. It makes a second reader visible; it
//! does not stop one who works around it. The behaviour is held by
//! `crates/obop/src/brand_row_tests.rs`: with a row, every output that is pinned is what it was
//! without one (the logo path is the exception: it answers the artwork with a row,
//! `crates/obop/src/brand_http_tests.rs`).

use crate::src_scan::{Lexed, after_brace, line_of, non_test, read_tree, tree, words};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// What a row of [`ALLOWED`] lets its file do with the brand.
#[derive(Clone, Copy)]
enum Limit {
    /// Anything: the file belongs to the module, or acts on the row as its owner.
    Any,
    /// Ask the reader and nothing else: at most `calls` calls of `brand()`, at most `reads` calls
    /// in all of `.current(`, `.fresh(` and `.strict(` (the three ways of asking a reader), and
    /// paths through the module `brand` only to the names in `paths`; a name in `types` may be
    /// written (it is in `paths` too) but nothing may be reached through it with `::`, and it may
    /// be neither renamed (`as`, `type X =`) nor implemented for (`impl`, `for`); this holds for
    /// every name in `types`, not for the first. The model,
    /// `install_brand`, any other path and the accessor named without a call are not allowed.
    Reads {
        calls: usize,
        reads: usize,
        paths: &'static [&'static str],
        types: &'static [&'static str],
    },
}

/// Where the brand is read or written: the path from `crates/` (a prefix when it ends with `/`),
/// why, and what the file may do.
const ALLOWED: [(&str, &str, Limit); 11] = [
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
        "the logo route `GET /api/brand/logo.svg` and `current()`: the route gets the reader of the dispatcher once and asks it once, through the blocking pool; it names the types `Reader` and `Shown` as types only (no function through them, no second name, no impl), so it builds no reader",
        Limit::Reads {
            calls: 1,
            reads: 1,
            paths: &["Reader", "Shown"],
            types: &["Reader", "Shown"],
        },
    ),
    (
        "kernel/obop-http/src/lib.rs",
        "`api_routes` registers the logo route (`brand::routes`) and passes it the answer of the layout to an unknown path; it reads nothing",
        Limit::Reads {
            calls: 0,
            reads: 0,
            paths: &["routes", "NotFound"],
            types: &[],
        },
    ),
    (
        "obop/src/brand_cmd.rs",
        "`obop admin brand`, which acts on the database as the operator (spec, section 3)",
        Limit::Any,
    ),
    (
        "kernel/obop-http/src/oauth.rs",
        "the pages of the authorization flow (S1c): `read_brand` gets the reader of the dispatcher once and asks it once, on a blocking thread, and the four starts of a request (`authorize_blocking`, `step`, `microsoft_blocking`, `microsoft_return`) call it and hand that one value to every page they render; it names the type `Shown` as a type only (no function through it, no second name, no impl), so it builds no reader",
        Limit::Reads {
            calls: 1,
            reads: 1,
            paths: &["Shown"],
            types: &["Shown"],
        },
    ),
    (
        "kernel/obop-oauth/src/page.rs",
        "the pages of the authorization flow (S1c) show the `Shown` that the handler hands them through `FlowCtx.brand`, and read nothing: no reader here, only the four calls of the accessor `Shown::brand()` (the brand line, the neutral wording, the message and the comparison with the client's name)",
        Limit::Reads {
            calls: 4,
            reads: 0,
            paths: &["Shown"],
            types: &[],
        },
    ),
    (
        "kernel/obop-oauth/src/flow.rs",
        "`FlowCtx` carries the `Shown` that the handler read; `view` takes it as an argument and puts it in the page context, so a context cannot be made without one, and this file reads nothing",
        Limit::Reads {
            calls: 0,
            reads: 0,
            paths: &["Shown"],
            types: &[],
        },
    ),
    (
        "kernel/obop-http/src/shell.rs",
        "the back office shell (S2): `Shell::for_state` gets the reader of the dispatcher once, `shown()` asks it once, on a blocking thread, and only when the answer is the HTML document itself (never for a script, a style sheet or a picture); `Public::of` takes the public subset from the `Shown` with one call of the accessor `Shown::brand()`; it names the types `Reader` and `Shown` as types only (no function through them, no second name, no impl), so it builds no reader",
        Limit::Reads {
            calls: 2,
            reads: 1,
            paths: &["Reader", "Shown"],
            types: &["Reader", "Shown"],
        },
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
const READ: &str = "a call of `.current(`, `.fresh(` or `.strict(`";
const THROUGH: &str = "a function reached through a type the row lets the file name as a type only";
const RENAMED: &str = "a type the row lets the file name as a type only, renamed or implemented";
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

/// The word that ends just before `at`, ignoring white space (`impl` in `impl Reader`), or "" when
/// what precedes is not a word (an `=`, a `<`, a `:`).
fn word_before(code: &[u8], at: usize) -> &str {
    use crate::src_scan::is_ident;
    let end = code[..at]
        .iter()
        .rposition(|c| !c.is_ascii_whitespace())
        .map_or(0, |p| p + 1);
    let start = code[..end]
        .iter()
        .rposition(|c| !is_ident(*c))
        .map_or(0, |p| p + 1);
    std::str::from_utf8(&code[start..end]).unwrap_or("")
}

/// The offset after what closes a type name that ends at `from` (white space already skipped): an
/// empty `<>` (`Reader<>`), the `>` that closes a generic and the `)` that closes a parenthesised
/// type, each with the white space after it. `<(Reader)>::inert` and `<Reader<>>::inert` reach the
/// same function as `<Reader>::inert`, so the test for a `::` starts past all of them.
fn past_closers(code: &[u8], from: usize) -> usize {
    use crate::src_scan::skip_ws;
    let mut tail = from;
    loop {
        match code.get(tail) {
            Some(b'>' | b')') => tail = skip_ws(code, tail + 1),
            Some(b'<') if code.get(skip_ws(code, tail + 1)) == Some(&b'>') => {
                tail = skip_ws(code, skip_ws(code, tail + 1) + 1);
            }
            _ => return tail,
        }
    }
}

/// The spans of the `use` declarations of a file, from the keyword to the `;` that ends it (the end
/// of the file when none does). `use<'a>`, the precise-capturing bound, is not a declaration.
fn use_spans(code: &[u8]) -> Vec<(usize, usize)> {
    use crate::src_scan::skip_ws;
    words(code, "use")
        .into_iter()
        .filter(|at| code.get(skip_ws(code, at + "use".len())) != Some(&b'<'))
        .map(|at| {
            let end = code[at..]
                .iter()
                .position(|c| *c == b';')
                .map_or(code.len(), |p| at + p);
            (at, end)
        })
        .collect()
}

/// What a file with a row of [`Limit::Reads`] does with a reader once it has one: a call of
/// `.current(`, `.fresh(` or `.strict(` (a method call: the word follows a `.` and a `(` follows it),
/// a function reached through one of `types` (`Reader::new`, `Reader::with_bound`,
/// `Reader::inert`, `<Reader>::inert`: the word is followed by `::`, past what closes the type, see
/// [`past_closers`]), and a type given a second name or an implementation, which is a way round the
/// first rule (`use ...::Reader as Rd;`, `type Rd = Reader;`, `type Rd = (Reader);`,
/// `impl Tr for Reader`: the word is followed by `as`, follows an `=` (past any `(`), or follows
/// `impl` or `for`). They are looked for only in the files of such a row, where they are the whole
/// of what the file may do with the reader: elsewhere a method called `current` is no business of
/// this scan.
fn uses(l: &Lexed, types: &[&str]) -> Vec<Hit> {
    use crate::src_scan::{skip_ws, word_at};
    let code = &l.code;
    let mut out = Vec::new();
    for name in ["current", "fresh", "strict"] {
        for at in words(code, name) {
            let after = skip_ws(code, at + name.len());
            let dot = code[..at]
                .iter()
                .rposition(|c| !c.is_ascii_whitespace())
                .is_some_and(|p| code[p] == b'.');
            if dot && code.get(after) == Some(&b'(') {
                out.push(Hit {
                    at,
                    what: READ,
                    names: vec![],
                });
            }
        }
    }
    for name in types {
        for at in words(code, name) {
            let after = skip_ws(code, at + name.len());
            let tail = past_closers(code, after);
            let what = if code[tail..].starts_with(b"::") {
                THROUGH
            } else if word_at(code, after) == "as"
                || code[..at]
                    .iter()
                    .rposition(|c| !c.is_ascii_whitespace() && *c != b'(')
                    .is_some_and(|p| code[p] == b'=')
                || matches!(word_before(code, at), "impl" | "for")
            {
                RENAMED
            } else {
                continue;
            };
            out.push(Hit {
                at,
                what,
                names: vec![name.to_string()],
            });
        }
    }
    out
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
    let imports = use_spans(code);
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
        } else if colons || imports.iter().any(|(from, to)| (*from..*to).contains(&at)) {
            // `Dispatcher::brand` handed on (`.map(Dispatcher::brand)`), or `use x::brand;`, or
            // the module in a group or under another name (`use x::{brand as bb, Y};`, where the
            // later `bb::strict` names no `brand`): in a `use`, a `brand` that no `::` follows is
            // the module imported.
            hit(at, VALUE, vec![]);
        }
    }
    out.sort_by_key(|h| h.at);
    out
}

/// The hits that the row `limit` does not let its file make, with what the row allows in words.
fn beyond(limit: Limit, hits: &[Hit]) -> Vec<(&Hit, String)> {
    let Limit::Reads {
        calls,
        reads,
        paths,
        ..
    } = limit
    else {
        return Vec::new();
    };
    let (mut asked, mut read) = (0, 0);
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
            READ => {
                read += 1;
                (read > reads).then(|| format!("call number {read}, the row lets it ask {reads}"))
            }
            THROUGH => Some(format!(
                "`{}::` is a second way to a reader; the row lets the file name the type only",
                h.names[0]
            )),
            RENAMED => Some(format!(
                "`{}` under another name or with an impl is a second way to a reader; the row \
                 lets the file name the type only",
                h.names[0]
            )),
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
        let mut hits = readers(l);
        if let Some(i) = allowed(path)
            && let Limit::Reads { types, .. } = ALLOWED[i].2
        {
            hits.extend(uses(l, types));
            hits.sort_by_key(|h| h.at);
        }
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
    // Each of these is a file that reads the row or the reader around the module (the public site
    // asking the store for the row itself is the first).
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
            "use obop_commands::Dispatcher; fn f() { obop_commands::brand::strict(b); }",
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
    // The module imported inside a group, or under another name. The name after `as` is not
    // `brand`, so the uses of the alias (`bb::strict`) are not seen; the import is the one place
    // where the file names the module, and it is found there: one hit for the import (the module
    // named without a call), and the path that follows it, where there is one, is found as well.
    for (src, whats) in [
        (
            "use obop_commands::{brand, Dispatcher}; fn f() { brand::strict(b); }",
            vec![VALUE, PATH],
        ),
        (
            "use obop_commands::{brand as bb, Dispatcher}; fn f() { bb::strict(b); }",
            vec![VALUE],
        ),
        (
            "use obop_commands::{Dispatcher, brand as bb}; fn f() { bb::strict(b); }",
            vec![VALUE],
        ),
        ("use obop_commands::{Dispatcher, brand};", vec![VALUE]),
        (
            "use obop_commands::{\n    Dispatcher,\n    brand\n        as bb,\n};",
            vec![VALUE],
        ),
        (
            "use obop_commands::{self, nested::{brand as bb}};",
            vec![VALUE],
        ),
        ("pub use obop_commands::{brand as bb};", vec![VALUE]),
        (
            "fn f() { use obop_commands::{brand as bb, Dispatcher}; }",
            vec![VALUE],
        ),
        // The module of the crate itself.
        ("use brand as bb; fn f() { bb::strict(b); }", vec![VALUE]),
        // Two imports, and a use that comes after one that is not about the module.
        (
            "use std::fmt; use obop_commands::{brand as bb, Dispatcher}; use std::io;",
            vec![VALUE],
        ),
    ] {
        let (found, _) = scan(&tree(&[("kernel/obop-site/src/lib.rs", src)]));
        assert_eq!(found.len(), whats.len(), "{src}: {found:?}");
        for (f, what) in found.iter().zip(&whats) {
            assert!(f.contains(what), "{src}: {found:?}");
        }
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
        // A local called `brand` next to a comma, a brace, a cast or an `as`, outside any `use`: the
        // import rule is for the declarations only, and it ends at their `;`.
        "fn f(brand: u8) -> u8 { brand as u8 }",
        "fn f(a: u8, brand: u8) -> S { S { a, brand } }",
        "fn f(a: u8, brand: u8) -> (u8, u8) { (a, brand) }",
        "use std::fmt; use std::io::{Read, Write}; fn f(brand: u8) -> u8 { brand as u8 }",
        "use std::fmt::{Brand, brand_name, my_brand}; fn f() {}",
        "fn f<'a>() -> impl Tr + use<'a> { let brand = 1; (brand, 2) }",
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
            "use obop_commands::brand::{Reader, Shown}; fn f(s: &S) -> Arc<Reader> { s.dispatcher.brand() } fn g(r: Arc<Reader>) -> Arc<Shown> { r.current() }",
        ),
        (
            "kernel/obop-http/src/lib.rs",
            "fn f() { let _ = brand::routes(g); }",
        ),
        (
            "kernel/obop-http/src/oauth.rs",
            "use obop_commands::brand::Shown; fn read_brand(s: &S) -> Arc<Shown> { s.dispatcher.brand().current() }",
        ),
        (
            "kernel/obop-oauth/src/page.rs",
            "use obop_commands::brand::Shown; fn f(b: &Shown) -> &str { b.brand().name() } fn g() -> Shown { Shown::not_set(None) }",
        ),
        (
            "kernel/obop-oauth/src/flow.rs",
            "use obop_commands::brand::Shown; fn f() -> Arc<Shown> { Arc::new(Shown::not_set(None)) }",
        ),
        (
            "kernel/obop-http/src/shell.rs",
            "use obop_commands::brand::{Reader, Shown}; fn f(s: &S) -> Arc<Reader> { s.dispatcher.brand() } fn g(r: Arc<Reader>) -> Arc<Shown> { r.current() } fn h(s: &Shown) -> &str { s.brand().name() }",
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
        // The one reader, handed to the file, asked once, and the types named as types.
        (
            route,
            "use obop_commands::brand::{Reader, Shown}; fn current(r: Arc<Reader>) -> Arc<Shown> { r.current() }",
        ),
        // A local or a function called `current` is no call of a method of a reader.
        (
            lib,
            "fn current() -> u8 { 1 } fn f() { let current = current(); let _ = current; }",
        ),
        // A type named inside generics, in a bound, in a `let` annotation and in a return type is
        // named as a type: the `>` that closes the generic is not a way through it.
        (
            route,
            "use obop_commands::brand::{Reader, Shown}; fn f<T: Into<Arc<Reader>>>(x: T) -> Option<Arc<Shown>> where T: Send { let r: Arc<Reader> = x.into(); Some(r.current()) }",
        ),
        // The `)` and `>` that close a type are looked past only to find a `::`: a type in a tuple
        // or in a parenthesised generic, followed by anything else, is a type.
        (
            route,
            "use obop_commands::brand::{Reader, Shown}; fn f(r: (Arc<Reader>, Arc<Shown>)) -> Option<(Arc<Reader>)> { None } fn g(r: &Reader) -> Foo { Foo::new() }",
        ),
        (
            route,
            "use obop_commands::brand::{Reader, Shown}; fn f(r: &Reader, s: (&Shown)) { let _t: (Arc<Reader>,) = (todo(),); match x { A(Reader) => 1, B(Shown) => 2 } }",
        ),
    ] {
        let (found, _) = scan(&tree(&[(path, src)]));
        assert!(found.is_empty(), "{path}: {src}: {found:?}");
    }
    // What it does not say: every one of these is found, once, with the reason.
    for (path, src, what) in [
        // A second reader in the file that registers the route: a handler there that asks the
        // dispatcher for the reader, and one that asks a reader it was handed.
        (
            lib,
            "fn probe(s: &S) { let _ = s.dispatcher.brand(); }",
            CALL,
        ),
        (lib, "fn probe(r: &Reader) { let _ = r.current(); }", READ),
        (lib, "fn probe(r: &Reader) { let _ = r.fresh(); }", READ),
        (lib, "fn probe(r: &Reader) { let _ = r.strict(); }", READ),
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
        // The route file gets the reader once, asks it once, builds none, and reads the paths of
        // its types only.
        (
            route,
            "async fn f(s: &S) { let _ = s.dispatcher.brand(); let _ = s.dispatcher.brand(); }",
            CALL,
        ),
        // A second reader built in the file: by the path of the type, by its imported name, by a
        // qualified call with the type in angle brackets, through a second name given to the
        // type (`as`, `type X =`) and through an implementation. Each is found once, where the
        // type is named in the way that gets round the first rule.
        (
            route,
            "fn f() { let _other = obop_commands::brand::Reader::inert(); }",
            THROUGH,
        ),
        (
            route,
            "use obop_commands::brand::Reader; fn f() { let _other = <Reader>::inert(); }",
            THROUGH,
        ),
        (
            route,
            "use obop_commands::brand::Reader; fn f() { let _other = < Reader > :: inert(); }",
            THROUGH,
        ),
        (
            route,
            "use obop_commands::brand::Reader as Rd; fn f() { let _other = Rd::inert(); }",
            RENAMED,
        ),
        (
            route,
            "use obop_commands::brand::{Reader as Rd, Shown}; fn f() { let _other = Rd::inert(); }",
            RENAMED,
        ),
        (
            route,
            "use obop_commands::brand::Reader; fn f() { let _other = <Reader as Make>::make(); }",
            RENAMED,
        ),
        (
            route,
            "use obop_commands::brand::Reader; type Rd = Reader; fn f() { let _other = Rd::inert(); }",
            RENAMED,
        ),
        (
            route,
            "use obop_commands::brand::Reader; type Rd =\n    Reader;",
            RENAMED,
        ),
        (
            route,
            "use obop_commands::brand::Reader; impl Make for Reader { fn make() -> Self { Self::inert() } }",
            RENAMED,
        ),
        (
            route,
            "use obop_commands::brand::Reader; impl Reader { fn make() {} }",
            RENAMED,
        ),
        // `rustfmt` may put the type on the next line, and a source may use more than one space:
        // the word before the type is the keyword whatever white space lies between.
        (
            route,
            "use obop_commands::brand::Reader; impl Make for\n    Reader { fn make() {} }",
            RENAMED,
        ),
        (
            route,
            "use obop_commands::brand::Reader; impl  Reader { fn make() {} }",
            RENAMED,
        ),
        (
            route,
            "use obop_commands::brand::Reader; impl<T> Make<T> for\n\t\n  Reader { fn make() {} }",
            RENAMED,
        ),
        // The closers of a type are looked past before the `::`: a parenthesised type, an empty
        // generic list and the two together reach the same function as `<Reader>::inert`, and a
        // parenthesised type given a second name is a second name.
        (
            route,
            "use obop_commands::brand::Reader; fn f() { let _other = <(Reader)>::inert(); }",
            THROUGH,
        ),
        (
            route,
            "use obop_commands::brand::Reader; fn f() { let _other = <Reader<>>::inert(); }",
            THROUGH,
        ),
        (
            route,
            "use obop_commands::brand::Reader; fn f() { let _other = < ( Reader ) < > > :: inert(); }",
            THROUGH,
        ),
        (
            route,
            "use obop_commands::brand::Reader; fn f() { let _other = <((Reader))>::inert(); }",
            THROUGH,
        ),
        (
            route,
            "use obop_commands::brand::Reader; type Rd = (Reader); fn f() { let _other = <Rd>::inert(); }",
            RENAMED,
        ),
        (
            route,
            "use obop_commands::brand::Reader; type Rd = ( ( Reader ) );",
            RENAMED,
        ),
        // The other type of the row, `Shown`: the rules hold for every name in `types`, not for the
        // first (a scan that looks at `Reader` alone passes every case above).
        (
            route,
            "use obop_commands::brand::Shown; fn f() { let _ = Shown::not_set(None); }",
            THROUGH,
        ),
        (
            route,
            "use obop_commands::brand::Shown; fn f() { let _ = <Shown>::not_set(None); }",
            THROUGH,
        ),
        (
            route,
            "use obop_commands::brand::Shown; fn f() { let _ = <(Shown)>::not_set(None); }",
            THROUGH,
        ),
        (
            route,
            "use obop_commands::brand::Shown as S; fn f() { let _ = S::not_set(None); }",
            RENAMED,
        ),
        (
            route,
            "use obop_commands::brand::{Reader, Shown as S};",
            RENAMED,
        ),
        (
            route,
            "use obop_commands::brand::Shown; type S = Shown;",
            RENAMED,
        ),
        (
            route,
            "use obop_commands::brand::Shown; type S =\n    (Shown);",
            RENAMED,
        ),
        (
            route,
            "use obop_commands::brand::Shown; impl Make for Shown { fn make() {} }",
            RENAMED,
        ),
        (
            route,
            "use obop_commands::brand::Shown; impl Make for\n    Shown { fn make() {} }",
            RENAMED,
        ),
        (
            route,
            "use obop_commands::brand::Shown; impl Shown { fn make() {} }",
            RENAMED,
        ),
        // The module imported under another name, in the file that registers the route and in the
        // route file: the import is found, because the alias names no `brand` and its uses are not.
        (
            lib,
            "use obop_commands::{brand as bb, Dispatcher}; fn f() { bb::strict(b); }",
            VALUE,
        ),
        (
            route,
            "use obop_commands::{brand as bb, Dispatcher}; fn f() { bb::strict(b); }",
            VALUE,
        ),
        (lib, "use obop_commands::{Dispatcher, brand};", VALUE),
        (route, "use obop_commands::{Dispatcher, brand};", VALUE),
        (
            route,
            "use obop_commands::brand::Reader; fn f(b: Arc<dyn Backend>) { let _ = Reader::new(b); }",
            THROUGH,
        ),
        (
            route,
            "use obop_commands::brand::Reader; fn f(b: Arc<dyn Backend>) { let _ = Reader :: with_bound(b, d); }",
            THROUGH,
        ),
        (
            route,
            "use obop_commands::brand::Reader; fn f(r: &Reader) { let _ = Reader::fresh(r); }",
            THROUGH,
        ),
        // The one reader asked a second way, or twice: `current()` is the one place that asks it.
        (
            route,
            "fn current(r: Arc<Reader>) { let _ = r.fresh(); let _ = r.current(); }",
            READ,
        ),
        (
            route,
            "fn current(r: Arc<Reader>) { let _ = r.current(); let _ = r\n    .strict(); }",
            READ,
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
    // The same for the calls of the reader's methods: the second, in the order of the file, is the
    // one that is found, whichever of the three names it is. The two orders matter: the scan finds
    // the calls name by name (`current`, `fresh`, `strict`), so the excess is the later one in the
    // file only if the hits are put in the order of the file.
    for (src, line_of_the_second) in [
        (
            "fn a(r: &Reader) { r.current(); }\nfn b(r: &Reader) { r.fresh(); }",
            2,
        ),
        (
            "fn a(r: &Reader) { r.fresh(); }\nfn b(r: &Reader) { r.current(); }",
            2,
        ),
        (
            "fn a(r: &Reader) { r.strict(); }\nfn b(r: &Reader) { r.current(); }\nfn c() {}",
            2,
        ),
    ] {
        let (found, _) = scan(&tree(&[(route, src)]));
        assert_eq!(found.len(), 1, "{src}: {found:?}");
        assert!(
            found[0].starts_with(&format!("{route}:{line_of_the_second}:")),
            "{src}: {found:?}"
        );
    }
    // And a second way to a reader that comes first in the file is not reported after a later call
    // of a method: each hit is at its own line.
    let (found, _) = scan(&tree(&[(
        route,
        "use obop_commands::brand::Reader;\nfn a(r: &Reader) { r.current(); }\nfn b() { let _ = Reader::inert(); }\nfn c(r: &Reader) { r.fresh(); }",
    )]));
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found[0].starts_with(&format!("{route}:3:")), "{found:?}");
    assert!(found[1].starts_with(&format!("{route}:4:")), "{found:?}");
    // Test code of those files is left alone, as everywhere.
    let (found, _) = scan(&tree(&[(
        lib,
        "#[cfg(test)]\nmod tests { fn t(s: &S) { s.dispatcher.brand(); brand::strict(b); } }",
    )]));
    assert!(found.is_empty(), "{found:?}");
}

/// The rows with limits are the two files of the logo route and the three files of the pages of the
/// authorization flow, and no others: loosening one of them to `Limit::Any` is a change of this
/// test, and so a decision that shows in the diff.
#[test]
fn the_rows_with_limits_are_the_files_that_only_ask_the_reader() {
    let limited: Vec<&str> = ALLOWED
        .iter()
        .filter(|(_, _, l)| matches!(l, Limit::Reads { .. }))
        .map(|(p, _, _)| *p)
        .collect();
    assert_eq!(
        limited,
        [
            "kernel/obop-http/src/brand.rs",
            "kernel/obop-http/src/lib.rs",
            "kernel/obop-http/src/oauth.rs",
            "kernel/obop-oauth/src/page.rs",
            "kernel/obop-oauth/src/flow.rs",
            "kernel/obop-http/src/shell.rs"
        ]
    );
}

// ---- The scan for literals (slice S1c) -------------------------------------------------------------
//
// docs/spec/branding.md, section 13: a test that fails when a literal `obop` is added to a page that
// should use the record. It reads the non-test code of a closed list of files ([`LITERAL_FILES`],
// the files of the pages that a slice has made show the brand), on the same reader as the scan
// above (comments blanked, test-only code removed, literals kept apart from code), and fails on any
// string literal that contains `obop`, case-insensitively, that is not a row of [`LITERAL_ALLOWED`].
// A row names the file, the item that holds the literal (the function, or the constant when no
// function encloses it), the literal (exactly; or every literal of the item that names obop), how
// many there must be, a class and the reason. A row that matches fewer or more literals than it says
// fails too, so the list cannot go stale and a sixth text that names the product cannot slip in
// under the row of the five.
//
// A row is the decision that a text may name obop on every instance. It is reviewed against the
// spec. The class says why:
//
// - `default`: the one place that spells the default name (`obop_brand::DEFAULT_NAME`);
// - `wire`: a name on the wire (a header, a field, a scheme);
// - `env`: the name of an environment variable;
// - `identifier`: a name inside the program that a person does not read (a cookie, a key, a table);
// - `admin`: a text only an administrator reads, which names obop on purpose;
// - `today`: a text that must stay byte for byte for an instance with no brand; the reason names the
//   branded counterpart, the text that a brand puts in its place.
//
// What it does not see is what the scan above does not see: text built at run time, text in a
// dependency, a file that is not on the list, and a name inside a longer literal that is on the list
// for another reason (a row with no literal lets every literal of its item through, up to its count).
// It does read through the escapes of a literal (`\u{6f}`, `\x6f`, a line continuation), because the
// text a person sees is the text after them, and it fails on `concat!` in a listed file, which would
// put a name together out of parts that no literal holds. It does not see a name built by another
// macro (`format!`, `stringify!`, a macro of the crate) or by `include_str!`.

/// The closed list of files whose string literals are scanned. The slice that makes another page
/// show the brand adds its file here in the commit that wires it, with a row for each literal that
/// has to stay.
const LITERAL_FILES: [&str; 2] = [
    "kernel/obop-oauth/src/page.rs",
    "kernel/obop-http/src/oauth.rs",
];

/// The classes of a row, in the words of the spec.
const LITERAL_CLASSES: [&str; 6] = ["default", "wire", "env", "identifier", "admin", "today"];

/// A row of [`LITERAL_ALLOWED`]: the file, the item, the literal (`None`: every literal of the item
/// that names obop), the count, the class and the reason.
type LiteralRow = (
    &'static str,
    &'static str,
    Option<&'static str>,
    usize,
    &'static str,
    &'static str,
);

const LITERAL_ALLOWED: [LiteralRow; 4] = [
    (
        "kernel/obop-oauth/src/page.rs",
        "CATALOGUE",
        None,
        35,
        "today",
        "the 35 cells (five texts in seven languages) that name the product on an instance with no \
         brand: login.lead, consent.who_body, consent.f2, consent.who_body_mobile and \
         consent.who_body_mobile_loopback. They stay byte for byte (the golden pins of obop-oauth hold \
         them). The branded counterpart of each is its `.neutral` twin in the same catalogue, shown \
         whenever a brand is set; the second assertion below requires the twin to exist and to name \
         no product",
    ),
    (
        "kernel/obop-http/src/oauth.rs",
        "cors",
        Some("Retry-After, X-Obop-Applied"),
        1,
        "wire",
        "the name of a response header that a browser client may read: a name on the wire, never shown \
         to a person",
    ),
    (
        "kernel/obop-http/src/oauth.rs",
        "cookie_name",
        Some("obop_oauth_{}"),
        1,
        "identifier",
        "the name of the cookie that binds a browser to its flow: an identifier of the program that a \
         person does not read; renaming it would end the flows in progress",
    ),
    (
        "kernel/obop-http/src/oauth.rs",
        "webauthn_ctx",
        Some("obop"),
        1,
        "today",
        "the relying-party name of the passkey ceremony site of the sign-in pages. It reaches an \
         authenticator only through registration options, which the REST routes make, not these \
         pages. Its branded counterpart does not exist in this tree",
    ),
];

/// A string literal of a scanned file that names obop.
struct Named {
    path: String,
    item: String,
    text: String,
    line: usize,
}

/// The name of the item that holds the offset: the innermost function whose body contains it, else
/// the nearest `const` or `static` that starts before it (the literals of a table).
fn item_of(code: &[u8], at: usize) -> String {
    use crate::src_scan::{enclosing_fn, skip_ws, word_at};
    if let Some(f) = enclosing_fn(code, at) {
        return f;
    }
    ["const", "static"]
        .iter()
        .flat_map(|k| words(code, k).into_iter().map(move |p| (p, k.len())))
        .filter(|(p, _)| *p < at)
        .max_by_key(|(p, _)| *p)
        .map(|(p, len)| word_at(code, skip_ws(code, p + len)).to_string())
        .unwrap_or_default()
}

/// The text a (non-raw) literal stands for: `\u{..}` and `\x..` become the character, the one-letter
/// escapes the character they name, and a backslash at the end of a line takes the line break and
/// the white space after it with it (the continuation of a string). What is not an escape is left
/// as it is, so a malformed one is read as written and is no way round the scan.
fn unescaped(text: &str) -> String {
    let (mut out, mut it) = (String::new(), text.chars().peekable());
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('u') if it.peek() == Some(&'{') => {
                it.next();
                let hex: String = it.by_ref().take_while(|c| *c != '}').collect();
                let digits: String = hex.chars().filter(|c| *c != '_').collect();
                match u32::from_str_radix(&digits, 16)
                    .ok()
                    .and_then(char::from_u32)
                {
                    Some(c) => out.push(c),
                    None => out.push_str(&format!("\\u{{{hex}}}")),
                }
            }
            Some('x') => {
                let hex: String = (0..2).filter_map(|_| it.next()).collect();
                match u8::from_str_radix(&hex, 16) {
                    Ok(b) => out.push(char::from(b)),
                    Err(_) => out.push_str(&format!("\\x{hex}")),
                }
            }
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('0') => out.push('\0'),
            Some(c @ ('\\' | '"' | '\'')) => out.push(c),
            Some('\n') => {
                while it.peek().is_some_and(|c| c.is_whitespace()) {
                    it.next();
                }
            }
            Some('\r') if it.peek() == Some(&'\n') => {
                it.next();
                while it.peek().is_some_and(|c| c.is_whitespace()) {
                    it.next();
                }
            }
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Every literal of the listed files that names obop, in the order of the files and of the source.
/// A listed file that is not in the tree (or is all test code) is reported by [`literal_scan`].
fn named_literals(files: &BTreeMap<String, String>) -> Vec<Named> {
    let lexed = non_test(files);
    let mut out = Vec::new();
    for path in LITERAL_FILES {
        let Some(l) = lexed.get(path) else { continue };
        for (start, _, text) in &l.strings {
            // A raw literal starts at its `r`; any other starts at its quote and has escapes.
            let raw = files
                .get(path)
                .is_some_and(|src| src.as_bytes().get(*start) == Some(&b'r'));
            let seen = if raw { text.clone() } else { unescaped(text) };
            if seen.to_lowercase().contains("obop") {
                out.push(Named {
                    path: path.to_string(),
                    item: item_of(&l.code, *start),
                    text: text.clone(),
                    line: line_of(&l.code, *start),
                });
            }
        }
    }
    out
}

/// The row that lets this literal stand, if any.
fn literal_row(n: &Named) -> Option<usize> {
    LITERAL_ALLOWED
        .iter()
        .position(|(file, item, lit, _, _, _)| {
            *file == n.path && *item == n.item && lit.is_none_or(|l| l == n.text)
        })
}

fn cut(s: &str) -> String {
    let t: String = s.chars().take(60).collect();
    if t.chars().count() < s.chars().count() {
        format!("{t}...")
    } else {
        t
    }
}

/// The literals that no row lets stand and the listed files that are missing (`found`), and the
/// rows that match fewer or more literals than they say (`drift`).
fn literal_scan(files: &BTreeMap<String, String>) -> (Vec<String>, Vec<String>) {
    let named = named_literals(files);
    let lexed = non_test(files);
    let mut found = Vec::new();
    let mut seen = [0usize; LITERAL_ALLOWED.len()];
    for path in LITERAL_FILES {
        if !lexed.contains_key(path) {
            found.push(format!(
                "{path}: a file of LITERAL_FILES that is not in the tree (or is all test code)"
            ));
        }
    }
    for path in LITERAL_FILES {
        let Some(l) = lexed.get(path) else { continue };
        for at in words(&l.code, "concat") {
            let after = crate::src_scan::skip_ws(&l.code, at + "concat".len());
            if l.code.get(after) == Some(&b'!') {
                found.push(format!(
                    "{path}:{}: `concat!` puts a text together out of parts that no scan of literals \
                     can read; write the literal whole",
                    line_of(&l.code, at)
                ));
            }
        }
    }
    for n in &named {
        match literal_row(n) {
            Some(i) => seen[i] += 1,
            None => found.push(format!(
                "{}:{}: a literal that names obop is not a row of LITERAL_ALLOWED (in `{}`): {:?}",
                n.path,
                n.line,
                n.item,
                cut(&n.text)
            )),
        }
    }
    let drift = LITERAL_ALLOWED
        .iter()
        .enumerate()
        .filter(|(i, (_, _, _, count, _, _))| seen[*i] != *count)
        .map(|(i, (file, item, lit, count, _, _))| {
            format!(
                "{file} `{item}` {}: the row says {count}, the file has {}",
                lit.map_or("(every literal)".to_string(), |l| format!("{l:?}")),
                seen[i]
            )
        })
        .collect();
    (found, drift)
}

#[test]
fn no_literal_that_names_obop_is_added_to_a_page_that_should_use_the_record() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = BTreeMap::new();
    read_tree(&root, &root, &mut files);
    let (found, drift) = literal_scan(&files);
    assert!(
        found.is_empty(),
        "a page that shows the instance name has a literal that names obop (use the brand, or add a \
         row to LITERAL_ALLOWED with its class and reason if the text may name obop on every \
         instance):\n{}",
        found.join("\n")
    );
    assert!(
        drift.is_empty(),
        "rows of LITERAL_ALLOWED that match fewer or more literals than they say (the row is stale, \
         or a text that names obop was added under it):\n{}",
        drift.join("\n")
    );
}

#[test]
fn every_row_of_the_literal_list_has_a_known_class_a_reason_and_a_file_of_the_list() {
    for (file, item, lit, count, class, why) in LITERAL_ALLOWED {
        assert!(
            LITERAL_FILES.contains(&file),
            "{file} is not in LITERAL_FILES"
        );
        assert!(
            LITERAL_CLASSES.contains(&class),
            "{file} {item}: class {class}"
        );
        assert!(count > 0, "{file} {item}: a row of no literals");
        assert!(!item.is_empty() && lit.is_none_or(|l| !l.is_empty()));
        assert!(why.len() > 40, "{file} {item}: say why in words");
        // A text that stays for an instance with no brand must say what a brand puts in its place.
        if class == "today" {
            assert!(
                why.contains("counterpart"),
                "{file} {item}: a `today` row names the branded counterpart"
            );
        }
    }
    for file in LITERAL_FILES {
        assert!(
            LITERAL_ALLOWED.iter().any(|r| r.0 == file),
            "{file} is on the list and has no row, so it could never hold a name"
        );
    }
}

/// A `page.rs` with a catalogue of `cells` cells that name obop.
fn page_with(cells: usize) -> String {
    let mut s = String::from("pub const CATALOGUE: &[(&str, [&str; 7])] = &[\n");
    for i in 0..cells {
        s.push_str(&format!("(\"k{i}\", [\"obop is here\"]),\n"));
    }
    s.push_str("];\n");
    s
}

/// An `oauth.rs` with the three literals of the rows.
const OAUTH_RS: &str = "fn cors() { let _ = \"Retry-After, X-Obop-Applied\"; }\n\
     fn cookie_name() { let _ = format!(\"obop_oauth_{}\", 1); }\n\
     fn webauthn_ctx() { let _ = \"obop\"; }\n";

const PAGE_PATH: &str = "kernel/obop-oauth/src/page.rs";
const OAUTH_PATH: &str = "kernel/obop-http/src/oauth.rs";

#[test]
fn the_scan_for_literals_finds_a_new_literal() {
    let ok = tree(&[(PAGE_PATH, &page_with(35)), (OAUTH_PATH, OAUTH_RS)]);
    let (found, drift) = literal_scan(&ok);
    assert!(found.is_empty() && drift.is_empty(), "{found:?} {drift:?}");

    // A literal `obop` added to a listed file is found, with the file, the line and the item.
    let added = format!(
        "{}\nfn brand_line() -> &'static str {{\n    \"obop\"\n}}\n",
        page_with(35)
    );
    let (found, drift) = literal_scan(&tree(&[(PAGE_PATH, &added), (OAUTH_PATH, OAUTH_RS)]));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].starts_with(&format!("{PAGE_PATH}:40:")) && found[0].contains("`brand_line`"),
        "{found:?}"
    );
    assert!(drift.is_empty(), "{drift:?}");

    // The case of the name does not matter.
    let loud = format!(
        "{}\nfn f() {{ let _ = \"Welcome to OBOP\"; }}\n",
        page_with(35)
    );
    let (found, _) = literal_scan(&tree(&[(PAGE_PATH, &loud), (OAUTH_PATH, OAUTH_RS)]));
    assert_eq!(found.len(), 1, "{found:?}");

    // The same text in another function is not the row's: the row names the item.
    let moved = OAUTH_RS.replace("fn webauthn_ctx()", "fn other()");
    let (found, drift) = literal_scan(&tree(&[(PAGE_PATH, &page_with(35)), (OAUTH_PATH, &moved)]));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("`other`"), "{found:?}");
    assert!(
        drift.iter().any(|d| d.contains("webauthn_ctx")),
        "{drift:?}"
    );

    // A different literal in the item of a row with an exact literal is not let through.
    let widened = OAUTH_RS.replace("\"obop\"", "\"obop shop\"");
    let (found, _) = literal_scan(&tree(&[
        (PAGE_PATH, &page_with(35)),
        (OAUTH_PATH, &widened),
    ]));
    assert_eq!(found.len(), 1, "{found:?}");

    // A file of the list that is missing from the tree is a failure, not a pass.
    let (found, _) = literal_scan(&tree(&[(OAUTH_PATH, OAUTH_RS)]));
    assert!(found.iter().any(|f| f.starts_with(PAGE_PATH)), "{found:?}");
}

#[test]
fn the_scan_for_literals_finds_a_row_that_matches_more_or_less_than_it_says() {
    // A sixth catalogue key that names obop sits under the row of the five and over its count.
    let (found, drift) = literal_scan(&tree(&[
        (PAGE_PATH, &page_with(36)),
        (OAUTH_PATH, OAUTH_RS),
    ]));
    assert!(found.is_empty(), "{found:?}");
    assert_eq!(drift.len(), 1, "{drift:?}");
    assert!(
        drift[0].contains("says 35") && drift[0].contains("has 36"),
        "{drift:?}"
    );

    // A row that matches less than it says: a cell lost its name, or the literal was renamed.
    let (found, drift) = literal_scan(&tree(&[
        (PAGE_PATH, &page_with(34)),
        (OAUTH_PATH, OAUTH_RS),
    ]));
    assert!(found.is_empty(), "{found:?}");
    assert!(drift[0].contains("has 34"), "{drift:?}");
    let renamed = OAUTH_RS.replace("obop_oauth_{}", "oauth_{}");
    let (found, drift) = literal_scan(&tree(&[
        (PAGE_PATH, &page_with(35)),
        (OAUTH_PATH, &renamed),
    ]));
    assert!(found.is_empty(), "{found:?}");
    assert!(
        drift.len() == 1 && drift[0].contains("cookie_name") && drift[0].contains("has 0"),
        "{drift:?}"
    );
}

/// A name written with an escape or put together with `concat!` is the same text for a person: the
/// scan reads the escapes and refuses `concat!` in a listed file, and leaves a raw literal (which
/// has no escapes) and the escapes that make no name alone.
#[test]
fn the_scan_for_literals_reads_escapes_and_finds_concat() {
    let listed = |extra: &str| {
        let page = format!("{}\n{extra}\n", page_with(35));
        literal_scan(&tree(&[(PAGE_PATH, &page), (OAUTH_PATH, OAUTH_RS)]))
    };
    // The name written with escapes, and over a line break.
    for (what, src) in [
        (r"\u{..}", r#"fn f() { let _ = "ob\u{6f}p"; }"#),
        (r"\u{..} in capitals", r#"fn f() { let _ = "\u{4F}BOP"; }"#),
        (r"\x..", r#"fn f() { let _ = "ob\x6fp"; }"#),
        ("a continuation", "fn f() { let _ = \"ob\\\n        op\"; }"),
        ("a byte string", r#"fn f() { let _ = b"ob\x6fp"; }"#),
    ] {
        let (found, _) = listed(src);
        assert_eq!(found.len(), 1, "{what}: {found:?}");
        assert!(found[0].contains("names obop"), "{what}: {found:?}");
    }
    // `concat!`, with or without a space before the bang.
    for src in [
        r#"fn f() { let _ = concat!("ob", "op"); }"#,
        r#"fn f() { let _ = concat ! ("ob", "op"); }"#,
    ] {
        let (found, _) = listed(src);
        assert!(
            found.iter().any(|f| f.contains("`concat!`")),
            "{src}: {found:?}"
        );
    }
    // What is not a name: a raw literal holds the characters it shows, an escape that makes no name,
    // and a word that only starts like the macro.
    for src in [
        r##"fn f() { let _ = r#"ob\u{6f}p"#; }"##,
        r#"fn f() { let _ = "line\nbreak \u{2026} \\ \u{zz}"; }"#,
        r#"fn f() { let _ = concat_all("a"); }"#,
    ] {
        let (found, _) = listed(src);
        assert!(found.is_empty(), "{src}: {found:?}");
    }
    // `unescaped` itself.
    assert_eq!(unescaped(r"ob\u{6f}p"), "obop");
    assert_eq!(unescaped(r"\x41\u{1F600}"), "A\u{1F600}");
    assert_eq!(unescaped("a\\\n   b"), "ab");
    assert_eq!(unescaped(r"a\qb\"), r"a\qb\");
}

#[test]
fn the_scan_for_literals_leaves_comments_test_code_and_files_off_the_list_alone() {
    let noisy = format!(
        "{}// a comment about obop\n/// a doc about obop\n\
         #[cfg(test)]\nmod tests {{ fn t() {{ let _ = \"obop in a test\"; }} }}\n",
        page_with(35)
    );
    let elsewhere = "fn f() { let _ = \"obop\"; }";
    let (found, drift) = literal_scan(&tree(&[
        (PAGE_PATH, &noisy),
        (OAUTH_PATH, OAUTH_RS),
        ("kernel/obop-http/src/sign_page.rs", elsewhere),
        ("kernel/obop-oauth/src/tests/mobile.rs", elsewhere),
    ]));
    assert!(found.is_empty() && drift.is_empty(), "{found:?} {drift:?}");
}

/// The second assertion of the scan: the neutral twins and the fixed sentence name no product, in
/// any language, and every text of the catalogue that names the product has a twin. The second part
/// is what makes "a sixth key that names obop fails until it has a neutral twin" true.
#[test]
fn the_neutral_twins_and_the_same_name_sentence_name_no_product() {
    use obop_oauth::page::{CATALOGUE, LANGS};
    let twins = [
        "login.lead.neutral",
        "consent.who_body.neutral",
        "consent.f2.neutral",
        "consent.who_body_mobile.neutral",
        "consent.who_body_mobile_loopback.neutral",
        "consent.same_name",
    ];
    for key in twins {
        let (_, cells) = CATALOGUE
            .iter()
            .find(|(k, _)| *k == key)
            .unwrap_or_else(|| panic!("{key} is not in the catalogue"));
        for (cell, lang) in cells.iter().zip(LANGS) {
            assert!(
                !cell.to_lowercase().contains("obop"),
                "{key} names the product in {lang}: {cell}"
            );
        }
    }
    let mut naming = 0;
    for (key, cells) in CATALOGUE {
        if cells.iter().any(|c| c.to_lowercase().contains("obop")) {
            naming += 1;
            assert!(
                CATALOGUE
                    .iter()
                    .any(|(k, _)| *k == format!("{key}.neutral")),
                "{key} names the product and has no neutral twin, `{key}.neutral`"
            );
        }
        if let Some(base) = key.strip_suffix(".neutral") {
            assert!(
                CATALOGUE.iter().any(|(k, _)| *k == base),
                "{key} is a twin of a text that does not exist"
            );
        }
    }
    assert_eq!(naming, 5, "the five texts that name the product");
}

/// The three rows of the authorization flow's pages (S1c) let their files do what the row says and
/// nothing else: the handlers get the reader of the dispatcher once and ask it once and build none;
/// the page functions and the flow context only use the value they were handed.
#[test]
fn the_rows_of_the_pages_of_the_authorization_flow_hold_their_files_to_what_they_say() {
    let route = "kernel/obop-http/src/oauth.rs";
    let page = "kernel/obop-oauth/src/page.rs";
    let flow = "kernel/obop-oauth/src/flow.rs";
    // What the rows say is allowed.
    for (path, src) in [
        (
            route,
            "use obop_commands::brand::Shown; fn read_brand(s: &S) -> Arc<Shown> { s.dispatcher.brand().current() } fn f(b: &Shown) {}",
        ),
        (
            page,
            "use obop_commands::brand::Shown; fn f(b: &Shown) { b.brand(); b.brand(); b.brand(); b.brand(); } fn g() -> Shown { Shown::not_set(None) }",
        ),
        (
            flow,
            "use obop_commands::brand::Shown; fn f() -> Arc<Shown> { Arc::new(Shown::not_set(None)) }",
        ),
    ] {
        let (found, _) = scan(&tree(&[(path, src)]));
        assert!(found.is_empty(), "{path}: {src}: {found:?}");
    }
    // What they do not say is found, once, with the reason.
    for (path, src, what) in [
        // A second question to the dispatcher, and a second read, in the handlers.
        (
            route,
            "fn f(s: &S) { let _ = s.dispatcher.brand(); let _ = s.dispatcher.brand(); }",
            CALL,
        ),
        (
            route,
            "fn f(r: &Reader) { let _ = r.current(); let _ = r.fresh(); }",
            READ,
        ),
        (
            route,
            "fn f(r: &Reader) { let _ = r.current(); let _ = r.strict(); }",
            READ,
        ),
        // A function through the type the handlers may name as a type only.
        (route, "fn f() { let _ = Shown::not_set(None); }", THROUGH),
        (route, "use obop_commands::brand::Shown as S;", RENAMED),
        (route, "use obop_commands::brand::Reader;", PATH),
        (
            route,
            "fn f(d: &Dispatcher) { d.install_brand(b); }",
            INSTALL,
        ),
        // A fifth accessor call in the page functions, or any read of a reader there.
        (
            page,
            "fn f(b: &Shown) { b.brand(); b.brand(); b.brand(); b.brand(); b.brand(); }",
            CALL,
        ),
        (page, "fn f(r: &Reader) { let _ = r.current(); }", READ),
        (page, "use obop_commands::brand::Reader;", PATH),
        (
            page,
            "fn f(d: &Dispatcher) { d.install_brand(b); }",
            INSTALL,
        ),
        // The flow context holds the value and asks nothing.
        (flow, "fn f(b: &Shown) { b.brand(); }", CALL),
        (flow, "fn f(r: &Reader) { let _ = r.current(); }", READ),
        (flow, "use obop_commands::brand::Reader;", PATH),
    ] {
        let (found, _) = scan(&tree(&[(path, src)]));
        assert_eq!(found.len(), 1, "{path}: {src}: {found:?}");
        assert!(found[0].contains(what), "{path}: {src}: {found:?}");
    }
}

// ---- One read for each request that makes a page (slice S1c) ----------------------------------------
//
// The scan for the one reader counts what a file asks of the reader, and the handlers ask it in one
// helper, `read_brand`, so a second call of the helper in a handler is a second question that the
// counts above cannot see. The pages of a request must come from one value (the reader answers from
// its cell within two seconds, but a refill in between may change the answer, and a request that
// shows the name twice should not show two names). So the calls of `read_brand` are pinned too:
// exactly one in each of the four functions where a request of the flow begins, and none anywhere
// else in the file.

const ROUTE_FILE: &str = "kernel/obop-http/src/oauth.rs";

/// The four functions where a request of the authorization flow begins: the page of the flow, the
/// step of a form post, the link of the Microsoft step and its return address.
const REQUEST_STARTS: [&str; 4] = [
    "authorize_blocking",
    "microsoft_blocking",
    "microsoft_return",
    "step",
];

/// The function that holds each call of `read_brand` in the non-test code of the route file, in
/// source order. The definition of the helper is not a call.
fn brand_read_sites(files: &BTreeMap<String, String>) -> Vec<String> {
    use crate::src_scan::{enclosing_fn, skip_ws};
    let lexed = non_test(files);
    let Some(l) = lexed.get(ROUTE_FILE) else {
        return vec!["(the route file is not in the tree)".to_string()];
    };
    let name = "read_brand";
    words(&l.code, name)
        .into_iter()
        .filter(|&p| {
            let before = String::from_utf8_lossy(&l.code[..p]);
            let defines = before.trim_end().ends_with("fn");
            let calls = l.code.get(skip_ws(&l.code, p + name.len())) == Some(&b'(');
            calls && !defines
        })
        .map(|p| enclosing_fn(&l.code, p).unwrap_or_else(|| "(no function)".to_string()))
        .collect()
}

#[test]
fn each_request_that_makes_a_page_reads_the_brand_once_and_nothing_else_does() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = BTreeMap::new();
    read_tree(&root, &root, &mut files);
    let mut sites = brand_read_sites(&files);
    sites.sort();
    assert_eq!(
        sites,
        REQUEST_STARTS.to_vec(),
        "`read_brand` is called once in each function where a request of the flow begins and \
         nowhere else; a page handler takes the brand it is handed"
    );
}

#[test]
fn the_scan_of_the_calls_of_the_brand_helper_sees_a_second_call_a_missing_one_and_a_stray_one() {
    let at = |src: &str| {
        let mut sites = brand_read_sites(&tree(&[(ROUTE_FILE, src)]));
        sites.sort();
        sites
    };
    let helper = "fn read_brand(s: &S) -> Arc<Shown> { s.dispatcher.brand().current() }";
    // The helper alone is no call; a call in each start is what the list says.
    assert!(at(helper).is_empty());
    assert_eq!(
        at(&format!(
            "{helper} fn authorize_blocking() {{ let b = read_brand(s); }} \
             fn step() {{ let b = read_brand(s); }} \
             fn microsoft_blocking() {{ let b = read_brand(s); }} \
             async fn microsoft_return() {{ blocking(move || {{ let b = read_brand(&s); }}) }}"
        )),
        REQUEST_STARTS.to_vec()
    );
    // A second call in one of the starts, a call in another function, a start with none.
    assert_eq!(
        at("fn step() { let a = read_brand(s); let b = read_brand(s); }"),
        ["step", "step"]
    );
    assert_eq!(
        at("fn consent_or_gone() { read_brand(s); }"),
        ["consent_or_gone"]
    );
    assert!(
        !at("fn authorize_blocking() { let b = 1; }").contains(&"authorize_blocking".to_string())
    );
    // Comments and test code are not code of the route.
    assert!(at("fn f() { /* read_brand(s) */ } // read_brand(s)\n").is_empty());
    assert!(at("#[cfg(test)] mod t { fn x() { read_brand(s); } }").is_empty());
}

// ---- The two functions that make a page with the default brand are for tests only (slice S1c) ----------
//
// `error_page` (page.rs) and `OAuth::view_plain` (flow.rs) make a page, or a page context, for an
// instance with no brand. They exist for the golden pins and the tests of the flow, and they are
// compiled for the tests of `obop-oauth` only (`#[cfg(test)]`), so that a handler cannot call them:
// the call would not compile. Nothing else held that gate: take the attribute off and every test,
// scan and lint stays green, and the hole is open again. So the non-test code of the two files is
// scanned for a function of either name. (`error_page_with` is another word and is the function the
// handlers use.)

/// The files that hold a function which must not exist outside a test, and its name.
const TEST_ONLY: [(&str, &str); 2] = [
    ("kernel/obop-oauth/src/page.rs", "error_page"),
    ("kernel/obop-oauth/src/flow.rs", "view_plain"),
];

/// Where a function of `TEST_ONLY` is defined in code that a release build compiles.
fn defined_outside_tests(files: &BTreeMap<String, String>) -> Vec<String> {
    let lexed = non_test(files);
    let mut found = Vec::new();
    for (path, name) in TEST_ONLY {
        let Some(l) = lexed.get(path) else {
            found.push(format!("{path}: not in the tree"));
            continue;
        };
        for at in words(&l.code, name) {
            let before = String::from_utf8_lossy(&l.code[..at]);
            if before.trim_end().ends_with("fn") {
                found.push(format!(
                    "{path}:{}: `fn {name}` is compiled outside the tests",
                    line_of(&l.code, at)
                ));
            }
        }
    }
    found
}

#[test]
fn the_functions_that_make_a_page_with_the_default_brand_are_compiled_for_tests_only() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = BTreeMap::new();
    read_tree(&root, &root, &mut files);
    let found = defined_outside_tests(&files);
    assert!(
        found.is_empty(),
        "a handler could call a page of an instance with no brand:\n{}",
        found.join("\n")
    );
    // The scan reads the real files: both functions are in them, behind the attribute.
    for (path, name) in TEST_ONLY {
        let src = &files[path];
        assert!(
            src.contains(&format!("#[cfg(test)]\npub fn {name}"))
                || src.contains(&format!("#[cfg(test)]\n    pub(crate) fn {name}")),
            "{path}: `fn {name}` is not behind `#[cfg(test)]` where the test expects it"
        );
    }
}

#[test]
fn the_scan_of_the_test_only_functions_sees_one_without_the_attribute_and_leaves_the_rest_alone() {
    let (page, flow) = (TEST_ONLY[0].0, TEST_ONLY[1].0);
    let at = |page_src: &str, flow_src: &str| {
        defined_outside_tests(&tree(&[(page, page_src), (flow, flow_src)]))
    };
    // Behind the attribute: nothing. `error_page_with` is another function, and a call is no
    // definition.
    assert!(
        at(
            "#[cfg(test)]\npub fn error_page(l: &str) {}\npub fn error_page_with(b: &S) { error_page(l); }",
            "impl O { #[cfg(test)] pub(crate) fn view_plain(&self) {} }"
        )
        .is_empty()
    );
    // Without it, in either file, public or private.
    let found = at("pub fn error_page(l: &str) {}", "");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("page.rs:1") && found[0].contains("error_page"));
    let found = at("", "impl O { pub(crate) fn view_plain(&self) {} }");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("flow.rs") && found[0].contains("view_plain"));
    let found = at("fn error_page() {}", "fn view_plain() {}");
    assert_eq!(found.len(), 2, "{found:?}");
    // A comment or a string that says the name is no definition; a file that is gone is reported.
    assert!(
        at(
            "// fn error_page() {}\nconst S: &str = \"fn error_page\";",
            ""
        )
        .is_empty()
    );
    assert_eq!(
        defined_outside_tests(&tree(&[(page, "")])),
        [format!("{flow}: not in the tree")]
    );
}

// ---- The brand of a page context is private to its crate (slice S1c, review) --------------------------
//
// `FlowCtx.brand` is the one way a page gets its brand: `OAuth::view` takes the brand as an argument
// and puts it there, so a handler cannot get a page context without handing one over. That holds only
// while no other crate can build a `FlowCtx` by hand with a default brand in it, and the field is
// `pub(crate)` for that: make it `pub` and every test, scan and lint stays green while the hole is
// open. So the field's declaration is read.

/// The file of the page context and its name.
const FLOW_CTX_FILE: &str = "kernel/obop-oauth/src/flow.rs";

/// What is wrong with the declaration of the field `brand` of `struct FlowCtx` in the non-test code
/// of `FLOW_CTX_FILE`: it does not exist, it exists twice, or it is `pub` (visible to every crate).
fn flow_ctx_brand_visibility(files: &BTreeMap<String, String>) -> Vec<String> {
    let lexed = non_test(files);
    let Some(l) = lexed.get(FLOW_CTX_FILE) else {
        return vec![format!("{FLOW_CTX_FILE}: not in the tree")];
    };
    let code = &l.code;
    let defs: Vec<usize> = words(code, "FlowCtx")
        .into_iter()
        .filter(|at| {
            String::from_utf8_lossy(&code[..*at])
                .trim_end()
                .ends_with("struct")
        })
        .collect();
    let [at] = defs[..] else {
        return vec![format!(
            "{FLOW_CTX_FILE}: `struct FlowCtx` is defined {} times",
            defs.len()
        )];
    };
    let Some(open) = code[at..].iter().position(|c| *c == b'{').map(|p| at + p) else {
        return vec![format!("{FLOW_CTX_FILE}: `struct FlowCtx` has no body")];
    };
    let body = String::from_utf8_lossy(&code[open..after_brace(code, open)]).into_owned();
    // The declarations of a field named `brand`: whatever stands before the name in its declaration
    // (the text between two commas or braces) is its visibility.
    let mut decls = Vec::new();
    for line in body.split(['\n', ',', '{', '}']).map(str::trim) {
        if let Some((vis, rest)) = line.split_once("brand")
            && rest.trim_start().starts_with(':')
            && (vis.is_empty() || vis.trim() == "pub" || vis.trim().starts_with("pub("))
        {
            decls.push(vis.trim().to_string());
        }
    }
    match decls.as_slice() {
        [vis] if vis == "pub" => vec![format!(
            "{FLOW_CTX_FILE}: the field `FlowCtx.brand` is `pub`; it must be `pub(crate)`, so that no other crate can build a page context with a default brand"
        )],
        [_] => Vec::new(),
        other => vec![format!(
            "{FLOW_CTX_FILE}: `struct FlowCtx` has {} fields named `brand`, the scan expects one",
            other.len()
        )],
    }
}

#[test]
fn the_brand_of_a_page_context_is_not_visible_to_other_crates() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = BTreeMap::new();
    read_tree(&root, &root, &mut files);
    let found = flow_ctx_brand_visibility(&files);
    assert!(found.is_empty(), "{}", found.join("\n"));
    // The scan reads the real declaration: it is there, and it is the crate-private one.
    assert!(
        files[FLOW_CTX_FILE].contains("    pub(crate) brand: Arc<Shown>,"),
        "{FLOW_CTX_FILE}: the declaration the scan reads is not where the test expects it"
    );
}

#[test]
fn the_scan_of_the_page_context_sees_a_public_field_a_missing_one_and_two() {
    let at = |src: &str| flow_ctx_brand_visibility(&tree(&[(FLOW_CTX_FILE, src)]));
    // The field as it is: nothing. A private field or one limited to the parent is no wider.
    assert!(
        at("pub struct FlowCtx { pub lang: String, pub(crate) brand: Arc<Shown>, }").is_empty()
    );
    assert!(at("pub struct FlowCtx { brand: Arc<Shown> }").is_empty());
    assert!(at("pub struct FlowCtx { pub(super) brand: Arc<Shown> }").is_empty());
    // Public: named. The same on one line or many, and a doc comment or a string that says the
    // words is no declaration.
    let found = at("pub struct FlowCtx {\n    pub lang: String,\n    pub brand: Arc<Shown>,\n}");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("`pub`"), "{found:?}");
    assert!(
        at("pub struct FlowCtx {\n    /// pub brand: the brand\n    pub(crate) brand: Arc<Shown>,\n    pub note: &'static str,\n}")
            .is_empty()
    );
    // Missing, doubled, no struct, a second struct, a file that is gone.
    assert_eq!(at("pub struct FlowCtx { pub lang: String }").len(), 1);
    assert_eq!(
        at("pub struct FlowCtx { pub(crate) brand: A,\n pub(crate) brand: B }").len(),
        1
    );
    assert_eq!(at("fn f() {}").len(), 1);
    assert_eq!(
        at("pub struct FlowCtx { pub(crate) brand: A }\npub struct FlowCtx { }").len(),
        1
    );
    assert_eq!(
        flow_ctx_brand_visibility(&tree(&[("kernel/obop-oauth/src/page.rs", "")])),
        [format!("{FLOW_CTX_FILE}: not in the tree")]
    );
}
