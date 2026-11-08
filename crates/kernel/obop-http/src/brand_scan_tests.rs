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
//! call `brand()` as many times as the row says, name the paths through the module `brand` that
//! the row lists, call `.current(`, `.fresh(` and `.strict(` and the helper `rp_name(` of
//! `obop-http/src/passkeys.rs` (which does one display read for a ceremony site) as many times in
//! all as the row says, and name the types the row lists as types only, and nothing else. A call
//! of `brand()` on the answer of a `.current()` is the `brand()` of a `Shown`, not the accessor of
//! the dispatcher, and is not counted as one (the `.current(` before it is counted as a read). A type "as a type only" is
//! never reached by `::` (`Reader::new`, `Reader::with_bound`, `Reader::inert`, `<Reader>::inert`
//! or any other function, which would build a second reader or reach round the one the file was
//! handed), never given a second name (`use ...::Reader as Rd;`, `type Rd = Reader;`) and never
//! implemented for (`impl Tr for Reader`, `impl Reader`). So the reason of such a row ("it reads
//! nothing", "the route asks the reader once and builds none") is held by the scan, and a second
//! reader added to a large file such as `obop-http/src/lib.rs` is found.
//!
//! **What it does not see.** It is a textual scan, not a parser. Text built at run time
//! (`format!("{}_brand", "instance")`), the model reached through a constant of another crate that
//! is not spelled in the file, a reader passed in as a trait object by a file that is on the list,
//! a reader built through a trait without the name of its type (`Default::default()`; `Reader` has
//! no `Default`, and a release that gives it one must extend the rules above), and Rust outside
//! `crates/` are not seen. A surface that shows the brand through a helper of another file has none
//! of the words above in its own text: the one helper that exists, `rp_name(`, is looked for by
//! name (`HELPER`; the same name written as a path with no call, as in `.map(passkeys::rp_name)`,
//! or imported, or renamed on import, is `HELPER_VALUE` and no row lets a file do it), so that the
//! OAuth pages' passkey step needs a row of its own; the bare name imported by a glob or a group
//! and then handed on is not seen, and a second helper
//! of that kind is not seen until it is added to `readers`. It makes a second reader visible; it does not stop one who works
//! around it. The behaviour is held by `crates/obop/src/brand_row_tests.rs`: with a
//! row, every output that is pinned is what it was without one (the logo path is the exception: it
//! answers the artwork with a row, `crates/obop/src/brand_http_tests.rs`).
//! Since S1e there are two more exceptions: the enrolment texts. With a row the label and `issuer=`
//! of an authenticator entry and the relying party name of a passkey are the name of the record, and
//! the enrolment asks the reader for it (`crates/obop/src/brand_enrol_tests.rs`, and the
//! `with_a_brand_row_*` tests of `mfa_tests` and `passkey_tests`).

use crate::src_scan::{Lexed, line_of, non_test, read_tree, tree, words};
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
    /// be neither renamed (`as`, `type X =`) nor implemented for (`impl`, `for`). The model,
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
        "kernel/obop-auth/src/mfa.rs",
        "the authenticator enrolment (S1e): `AuthConfig::from_env` reads `OBOP_MFA_ISSUER` through `brand::env_issuer`, and writes a warning for a value with a link character (`brand::issuer_warning`), and `begin` reads the stored name with `brand::strict` (the row alone, every time) and turns a failure into the error of an enrolment with `brand::enrolment_error`; it holds no reader and calls no method of one (`from_env` calls `KeyRing::current` in the form of a function, so that the scan, which cannot tell that method from the reader's, finds no `.current(` here)",
        Limit::Reads {
            calls: 0,
            reads: 0,
            paths: &["env_issuer", "issuer_warning", "strict", "enrolment_error"],
            types: &[],
        },
    ),
    (
        "kernel/obop-http/src/mfa.rs",
        "`mfa_error` answers the 503 `brand_unavailable` for a message that starts with `brand::UNREADABLE` (S1e); it reads nothing",
        Limit::Reads {
            calls: 0,
            reads: 0,
            paths: &["UNREADABLE"],
            types: &[],
        },
    ),
    (
        "kernel/obop-http/src/passkeys.rs",
        "the passkey ceremony site (S1e): `rp_name` gets the reader of the dispatcher and asks it for display once (`current`), for every route but the registration options and for the OAuth pages' site; `registration_name`, the name of the registration options, gets it once more and asks for the strict name (`strict`) with no display read, turns a failure into the error of an enrolment (`enrolment_error`) and refuses a reader that has no store (`no_reader_error`); it names no type and builds no reader (the two `brand()` calls the row lets it make are the dispatcher's, one in `rp_name` and one in `registration_name`; the `brand()` of a `Shown` after `.current()` is not counted; the three reads are `.current(`, `.strict(` and the one call of `rp_name(` in `display_name`)",
        Limit::Reads {
            calls: 2,
            reads: 3,
            paths: &["enrolment_error", "no_reader_error"],
            types: &[],
        },
    ),
    (
        "kernel/obop-http/src/oauth.rs",
        "the OAuth pages' passkey step (S1e): `webauthn_ctx` takes its relying party name from `passkeys::rp_name`, the display read of the passkey routes, and calls it once; the file holds no reader, names no path of the module `brand` and asks nothing else, so a second call of the helper is a second read that this row does not allow",
        Limit::Reads {
            calls: 0,
            reads: 1,
            paths: &[],
            types: &[],
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
const HELPER: &str =
    "a call of `rp_name(`, the helper of the passkey routes that makes one display read";
const HELPER_VALUE: &str = "the helper `rp_name` named by a path without a call, or renamed (passed on, bound to a name, imported)";
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

/// What a file with a row of [`Limit::Reads`] does with a reader once it has one: a call of
/// `.current(`, `.fresh(` or `.strict(` (a method call: the word follows a `.` and a `(` follows it),
/// a function reached through one of `types` (`Reader::new`, `Reader::with_bound`,
/// `Reader::inert`, `<Reader>::inert`: the word is followed by `::`, past the `>` that closes a
/// generic), and a type given a second name or an implementation, which is a way round the first
/// rule (`use ...::Reader as Rd;`, `type Rd = Reader;`, `impl Tr for Reader`: the word is followed
/// by `as`, follows an `=`, or follows `impl` or `for`). They are looked for only in the files of
/// such a row, where they are the whole of what the file may do with the reader: elsewhere a method
/// called `current` is no business of this scan.
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
            let mut tail = after;
            while code.get(tail) == Some(&b'>') {
                tail = skip_ws(code, tail + 1);
            }
            let what = if code[tail..].starts_with(b"::") {
                THROUGH
            } else if word_at(code, after) == "as"
                || code[..at]
                    .iter()
                    .rposition(|c| !c.is_ascii_whitespace())
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
            // The brand of a `Shown` (`reader.current().brand()`) is not the accessor of the
            // dispatcher; its `.current(` is counted as a read.
            if !on_a_current_answer(code, at) {
                hit(at, CALL, vec![]);
            }
        } else if colons {
            // `Dispatcher::brand` handed on (`.map(Dispatcher::brand)`), or `use x::brand;`.
            hit(at, VALUE, vec![]);
        }
    }
    for at in words(code, "rp_name") {
        let after = crate::src_scan::skip_ws(code, at + "rp_name".len());
        // A call, not the definition (`fn rp_name(`) and not the field of a struct (`rp_name:`).
        if code.get(after) == Some(&b'(') && word_before(code, at) != "fn" {
            hit(at, HELPER, vec![]);
        } else if word_before(code, at) != "fn"
            && (path_before(code, at) || crate::src_scan::word_at(code, after) == "as")
        {
            // The helper as a value, which is a read that nothing counts: `let f =
            // crate::passkeys::rp_name;`, `.map(crate::passkeys::rp_name)`, `use ...::rp_name;`
            // and `use ...::rp_name as other;` (a name that is only a path, or has another name
            // after it). A bare `rp_name` is a local or a field and is not looked at.
            hit(at, HELPER_VALUE, vec![]);
        }
    }
    out.sort_by_key(|h| h.at);
    out
}

/// Whether the word at `at` is the last segment of a path: `::` just before it, white space aside.
fn path_before(code: &[u8], at: usize) -> bool {
    code[..at]
        .iter()
        .rposition(|c| !c.is_ascii_whitespace())
        .is_some_and(|p| code[p] == b':' && p > 0 && code[p - 1] == b':')
}

/// Whether the `brand(` at `at` is called on the answer of a call of `current()`: `).` before it,
/// then `(`, then the word `current` (`s.dispatcher.brand().current().brand()`).
fn on_a_current_answer(code: &[u8], at: usize) -> bool {
    let last = |end: usize| code[..end].iter().rposition(|c| !c.is_ascii_whitespace());
    let Some(dot) = last(at).filter(|p| code[*p] == b'.') else {
        return false;
    };
    let Some(close) = last(dot).filter(|p| code[*p] == b')') else {
        return false;
    };
    let Some(open) = last(close).filter(|p| code[*p] == b'(') else {
        return false;
    };
    word_before(code, open) == "current"
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
            READ | HELPER => {
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
            "use obop_commands::brand::{Reader, Shown}; fn f(s: &S) -> Arc<Reader> { s.dispatcher.brand() } fn g(r: Arc<Reader>) -> Arc<Shown> { r.current() }",
        ),
        (
            "kernel/obop-http/src/lib.rs",
            "fn f() { let _ = brand::routes(g); }",
        ),
        (
            "kernel/obop-auth/src/mfa.rs",
            "fn f(b: &dyn Backend) { let _ = obop_commands::brand::strict(b); }",
        ),
        (
            "kernel/obop-http/src/mfa.rs",
            "fn f(m: &str) -> bool { m.starts_with(obop_commands::brand::UNREADABLE) }",
        ),
        (
            "kernel/obop-http/src/passkeys.rs",
            "use obop_commands::brand::enrolment_error; fn f(s: &S) { let _ = s.dispatcher.brand().current(); let _ = s.dispatcher.brand().strict().map_err(enrolment_error); }",
        ),
        (
            "kernel/obop-http/src/oauth.rs",
            "fn f(s: &S) { let _ = super::passkeys::rp_name(s); }",
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

/// The rows with limits are the files that only ask the reader: the two files of the logo route and
/// the four of the enrolment texts (the authenticator's configuration, the error mapper of the
/// routes, the passkey routes and the OAuth pages' passkey step). Loosening one of them to `Limit::Any` is a change of this test,
/// and so a decision that shows in the diff.
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
            "kernel/obop-auth/src/mfa.rs",
            "kernel/obop-http/src/mfa.rs",
            "kernel/obop-http/src/passkeys.rs",
            "kernel/obop-http/src/oauth.rs",
        ]
    );
}

/// The rows of the enrolment texts (S1e): each file may do what its row says and nothing else. The
/// authenticator's configuration reads a path and holds no reader; the error mapper of the routes
/// reads nothing; the passkey routes get the reader twice, ask it twice, and name no type.
#[test]
fn the_enrolment_files_may_name_what_their_rows_list_and_nothing_else() {
    let auth = "kernel/obop-auth/src/mfa.rs";
    let http_mfa = "kernel/obop-http/src/mfa.rs";
    let passkeys = "kernel/obop-http/src/passkeys.rs";
    let oauth = "kernel/obop-http/src/oauth.rs";
    for (path, src) in [
        (
            auth,
            "fn f() { let _ = obop_commands::brand::env_issuer(); let _ = obop_commands::brand::strict(b).map_err(obop_commands::brand::enrolment_error); let _ = KeyRing::current(&k); }",
        ),
        (
            http_mfa,
            "fn f(m: &str) -> bool { m.starts_with(obop_commands::brand::UNREADABLE) }",
        ),
        (
            passkeys,
            "use obop_commands::brand::enrolment_error; fn a(s: &S) -> String { s.dispatcher.brand().current().brand().name().to_string() } fn b(s: &S) { let _ = s.dispatcher.brand().strict().map_err(enrolment_error); }",
        ),
        // The helper is defined, called once, and named as a field, in the file that owns it.
        (
            passkeys,
            "fn rp_name(s: &S) -> String { s.dispatcher.brand().current().brand().name().to_string() } fn d(s: &S) -> Result<String, R> { Ok(rp_name(s)) } fn c(s: &S) -> Ctx { Ctx { rp_name: String::new() } }",
        ),
        // The OAuth pages' site calls the helper once, as the value of a field.
        (
            oauth,
            "fn f(s: &S) -> Ctx { Ctx { rp_name: super::passkeys::rp_name(s) } }",
        ),
    ] {
        let (found, _) = scan(&tree(&[(path, src)]));
        assert!(found.is_empty(), "{path}: {src}: {found:?}");
    }
    for (path, src, what) in [
        // A reader asked for by the authenticator's configuration, or one built there.
        (auth, "fn f(d: &Dispatcher) { let _ = d.brand(); }", CALL),
        // The row lets the file ask the reader nothing: `KeyRing::current` is called as a function,
        // and a `.current(` or `.strict(` method call is found, whichever reader it is on.
        (auth, "fn f(k: &KeyRing) { let _ = k.current(); }", READ),
        (auth, "fn f(r: &Reader) { let _ = r.strict(); }", READ),
        (
            auth,
            "fn f() { let _ = obop_commands::brand::Reader::inert(); }",
            PATH,
        ),
        (
            auth,
            "fn f() { let _ = obop_commands::brand::apply(); }",
            PATH,
        ),
        (
            auth,
            "fn f() { let _ = \"instance_brand\"; }",
            MODEL_LITERAL,
        ),
        // The error mapper of the routes reads nothing.
        (
            http_mfa,
            "fn f(d: &Dispatcher) { let _ = d.brand(); }",
            CALL,
        ),
        (
            http_mfa,
            "fn f(m: &str) { let _ = obop_commands::brand::strict(b); }",
            PATH,
        ),
        // The passkey routes: a third call of the accessor (the `brand()` of a `Shown` after a
        // `.current()` is not one), a fourth read, a second call of the helper, a built reader, a
        // path to anything else.
        (
            passkeys,
            "fn a(s: &S) { s.dispatcher.brand(); s.dispatcher.brand(); s.dispatcher.brand(); }",
            CALL,
        ),
        (
            passkeys,
            "fn a(r: &Reader) { r.current(); r.strict(); r.fresh(); r.current(); }",
            READ,
        ),
        (
            passkeys,
            "fn a(r: &Reader, s: &S) { r.current(); r.strict(); let _ = rp_name(s); let _ = rp_name(s); }",
            HELPER,
        ),
        // The OAuth pages' site: a second call of the helper is a second read; a reader asked for
        // in the file is found; so is any path through the module.
        (
            oauth,
            "fn f(s: &S) { let _ = super::passkeys::rp_name(s); let _ = super::passkeys::rp_name(s); }",
            HELPER,
        ),
        // The helper handed on, bound to a name or imported under another is a read that no count
        // sees: found in a file that has the row, whatever the way.
        (
            oauth,
            "fn f(s: &S) { let g = super::passkeys::rp_name; let _ = g(s); }",
            HELPER_VALUE,
        ),
        (
            oauth,
            "fn f(v: Vec<&S>) { let _: Vec<String> = v.into_iter().map(super::passkeys::rp_name).collect(); }",
            HELPER_VALUE,
        ),
        (
            oauth,
            "use super::passkeys::rp_name as other; fn f(s: &S) { let _ = other(s); }",
            HELPER_VALUE,
        ),
        (
            oauth,
            "use super::passkeys::{rp_name as other}; fn f(s: &S) { let _ = other(s); }",
            HELPER_VALUE,
        ),
        (oauth, "fn f(d: &Dispatcher) { let _ = d.brand(); }", CALL),
        (
            oauth,
            "fn f(r: &Reader) { let _ = r.current(); let _ = r.current(); }",
            READ,
        ),
        (
            oauth,
            "fn f() { let _ = obop_commands::brand::strict(b); }",
            PATH,
        ),
        (
            passkeys,
            "fn f() { let _ = obop_commands::brand::Reader::inert(); }",
            PATH,
        ),
        (
            passkeys,
            "fn f() { let _ = obop_commands::brand::strict(b); }",
            PATH,
        ),
    ] {
        let (found, _) = scan(&tree(&[(path, src)]));
        assert_eq!(found.len(), 1, "{path}: {src}: {found:?}");
        assert!(found[0].contains(what), "{path}: {src}: {found:?}");
    }
}

/// The names the module `brand` makes public in its own file (`pub use` items, `pub const` and
/// `pub fn`), by a plain reading of the text: the last name of a `pub use` path, the names inside
/// its braces, and the name after an `as`.
fn public_names_of_the_brand_module() -> Vec<String> {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../obop-commands/src/brand/mod.rs");
    let text = std::fs::read_to_string(&file).unwrap();
    let (mut names, mut statement) = (BTreeSet::new(), None::<String>);
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with("//") {
            continue;
        }
        // Only at the start of a line: an indented `pub fn` is a method of a type, such as
        // `Dispatcher::brand`, and is no name in the module.
        if let Some(rest) = raw.strip_prefix("pub const ") {
            names.insert(rest.split([':', ' ']).next().unwrap().to_string());
        } else if let Some(rest) = raw.strip_prefix("pub fn ") {
            names.insert(rest.split('(').next().unwrap().to_string());
        }
        if line.starts_with("pub use ") {
            statement = Some(String::new());
        }
        if let Some(text) = statement.as_mut() {
            text.push_str(line);
            text.push(' ');
            if line.ends_with(';') {
                let full = statement.take().unwrap();
                let list = full
                    .trim_start_matches("pub use ")
                    .trim_end_matches(|c: char| c == ';' || c.is_whitespace());
                let items: Vec<&str> = match list.split_once('{') {
                    Some((_, inside)) => inside.trim_end_matches('}').split(',').collect(),
                    None => vec![list.rsplit("::").next().unwrap()],
                };
                for item in items {
                    let item = item.trim();
                    // `MAX_BYTES as ARTWORK_MAX_BYTES` is known as the second name.
                    let name = item.rsplit(" as ").next().unwrap().trim();
                    if !name.is_empty() {
                        names.insert(name.to_string());
                    }
                }
            }
        }
    }
    names.into_iter().collect()
}

/// The limits of the rows of the enrolment texts, exactly as their reasons say: the calls of the
/// accessor, the reads, the paths and the types. The probes above show that the scan enforces a
/// limit; this one shows that no limit was loosened. A path added to a row (to let a file call
/// `brand::apply`, say) is a change of this table, and so a decision that shows in the diff. And for
/// each of these rows every public name of the module that the row does not list is found as a
/// path (the names are read from `brand/mod.rs`, so a name added to the module later is probed
/// without a list to keep).
#[test]
fn the_limits_of_the_enrolment_rows_are_exactly_what_their_reasons_say() {
    type Want = (&'static str, usize, usize, &'static [&'static str]);
    let want: [Want; 4] = [
        (
            "kernel/obop-auth/src/mfa.rs",
            0,
            0,
            &["env_issuer", "issuer_warning", "strict", "enrolment_error"],
        ),
        ("kernel/obop-http/src/mfa.rs", 0, 0, &["UNREADABLE"]),
        (
            "kernel/obop-http/src/passkeys.rs",
            2,
            3,
            &["enrolment_error", "no_reader_error"],
        ),
        ("kernel/obop-http/src/oauth.rs", 0, 1, &[]),
    ];
    // Every public name of the module, read from its own file, so that a name added to `brand` is
    // probed here without anyone remembering to add it to a list.
    let every_name = public_names_of_the_brand_module();
    // One of each way the file spells a public name: a function and a type of a `pub use` list, a
    // constant of a list, a constant of its own, a function of its own, and a name given with
    // `as`. And the method `brand` of `Dispatcher`, which is indented in the file, is no name.
    for needed in [
        "apply",
        "Reader",
        "UNREADABLE",
        "MODEL",
        "writable_fields",
        "ARTWORK_MAX_BYTES",
    ] {
        assert!(
            every_name.iter().any(|n| n == needed),
            "the names read from brand/mod.rs miss {needed}: {every_name:?}"
        );
    }
    assert!(
        !every_name.iter().any(|n| n == "brand" || n == "MAX_BYTES"),
        "{every_name:?}"
    );
    for (path, calls, reads, paths) in want {
        let i = allowed(path).unwrap_or_else(|| panic!("{path} has no row"));
        let Limit::Reads {
            calls: c,
            reads: r,
            paths: p,
            types: t,
        } = ALLOWED[i].2
        else {
            panic!("{path} has a row that lets it do anything")
        };
        assert_eq!((c, r, p), (calls, reads, paths), "{path}");
        assert!(t.is_empty(), "{path} may name a type: {t:?}");
        // Every other path through the module is found, once.
        for name in every_name.iter().filter(|n| !paths.contains(&n.as_str())) {
            let src = format!("fn f() {{ let _ = obop_commands::brand::{name}; }}");
            let (found, _) = scan(&tree(&[(path, &src)]));
            assert_eq!(found.len(), 1, "{path}: {name}: {found:?}");
            assert!(found[0].contains(PATH), "{path}: {name}: {found:?}");
        }
        // And every listed one is not.
        for name in paths {
            let src = format!("fn f() {{ let _ = obop_commands::brand::{name}; }}");
            let (found, _) = scan(&tree(&[(path, &src)]));
            assert!(found.is_empty(), "{path}: {name}: {found:?}");
        }
    }
}

/// The helper `rp_name(` is a read of the brand in whatever file calls it, and a file with no row
/// that calls it is found (the OAuth pages' passkey step had none, and a second or third call there
/// would have passed). The brand of a `Shown` after a `.current()` is not the accessor, and the
/// accessor before it is still counted.
#[test]
fn a_call_of_the_helper_is_found_in_a_file_with_no_row_and_the_brand_of_a_shown_is_not_the_accessor()
 {
    let site = "kernel/obop-site/src/lib.rs";
    let (found, _) = scan(&tree(&[(
        site,
        "fn f(s: &S) { let _ = passkeys::rp_name(s); }",
    )]));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains(HELPER), "{found:?}");
    // Not calls: the definition, a field, a local, and the name inside a longer one.
    for src in [
        "fn rp_name(s: &S) -> String { String::new() }",
        "fn f() -> Ctx { Ctx { rp_name: String::new() } }",
        "fn f(c: &Ctx) -> &str { &c.rp_name }",
        "fn f() { let rp_name = 1; let _ = rp_name + 1; let _ = rp_name_over(); }",
    ] {
        let (found, _) = scan(&tree(&[(site, src)]));
        assert!(found.is_empty(), "{src}: {found:?}");
    }
    // The helper as a value is found as well, in a file with no row: handed on, bound to a name,
    // imported, and imported under another name (a call of the other name has no `rp_name(`).
    for src in [
        "fn f(s: &S) { let g = crate::passkeys::rp_name; let _ = g(s); }",
        "fn f(v: Vec<&S>) { let _ = v.iter().map(|s| s).map(crate::passkeys::rp_name); }",
        "use crate::passkeys::rp_name as other; fn f(s: &S) { let _ = other(s); }",
        "use crate::passkeys::rp_name; fn f(s: &S) { let g = rp_name; let _ = g(s); }",
    ] {
        let (found, _) = scan(&tree(&[(site, src)]));
        assert_eq!(found.len(), 1, "{src}: {found:?}");
        assert!(found[0].contains(HELPER_VALUE), "{src}: {found:?}");
    }
    // The accessor is found, and the `brand()` after `.current()` is not a second one.
    let (found, _) = scan(&tree(&[(
        site,
        "fn f(d: &Dispatcher) { let _ = d.brand().current().brand().name(); }",
    )]));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains(CALL), "{found:?}");
}
