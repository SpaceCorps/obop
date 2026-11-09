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
//! the row lists, call `.current(`, `.fresh(` and `.strict(` and the helper `rp_name(` of
//! `obop-http/src/passkeys.rs` (which does one display read for a ceremony site) as many times in
//! all as the row says, and name the types the row lists as types only, and nothing else. A call
//! of `brand()` on the answer of a `.current()` is the `brand()` of a `Shown`, not the accessor of
//! the dispatcher, and is not counted as one (the `.current(` before it is counted as a read). A type "as a type only" is
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
//! is `Arc`'s), and Rust outside `crates/` are not seen. A surface that shows the brand through a
//! helper of another file has none of the words above in its own text: the one helper that exists,
//! `rp_name(`, is looked for by name (`HELPER`; the same name written as a path with no call, as in
//! `.map(passkeys::rp_name)`, or imported, or renamed on import, is `HELPER_VALUE` and no row lets a
//! file do it), so that the OAuth pages' passkey step needs a row of its own; the bare name imported
//! by a glob or a group and then handed on is not seen, and a second helper of that kind is not seen
//! until it is added to `readers`. It makes a second reader visible; it does not stop one who works
//! around it. The behaviour is held by `crates/obop/src/brand_row_tests.rs`: with a
//! row, every output that is pinned is what it was without one (the logo path is the exception: it
//! answers the artwork with a row, `crates/obop/src/brand_http_tests.rs`).
//! Since S1c the pages of the OAuth authorization flow are the next exception (`oauth_tests/brand.rs`),
//! and since S1e there are two more: the enrolment texts. With a row the label and `issuer=` of an
//! authenticator entry and the relying party name of a passkey are the name of the record, and the
//! enrolment asks the reader for it (`crates/obop/src/brand_enrol_tests.rs`, and the
//! `with_a_brand_row_*` tests of `mfa_tests` and `passkey_tests`).

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
const ALLOWED: [(&str, &str, Limit); 17] = [
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
        "kernel/obop-commands/src/public.rs",
        "`PublicApi::site_settings_read` (`site_settings` is the same call answering the object alone), the one door the public site and shop read their settings by: it hands the reader of the dispatcher to `brand::site_settings`, which does the read and the fill (spec, sections 4 and 5, row OT-04); it asks nothing of the reader itself",
        Limit::Reads {
            calls: 1,
            reads: 0,
            paths: &["site_settings"],
            types: &[],
        },
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
        "the pages of the authorization flow (S1c) and the ceremony site of the passkey step (S1e): `read_brand` gets the reader of the dispatcher once and asks it once, on a blocking thread, and the four starts of a request (`authorize_blocking`, `step`, `microsoft_blocking`, `microsoft_return`) call it and hand that one value to every page they render; it names the type `Shown` as a type only (no function through it, no second name, no impl), so it builds no reader; `webauthn_ctx` takes the relying party name of the passkey step from `passkeys::rp_name`, the display read of the passkey routes, and calls it once, so a passkey route of the flow makes a second display read after the one of its start (a hit of the cache within two seconds) for a name that no sign-in emits, and a second call of the helper is a third read that this row does not allow (the two reads the row lets the file make are the `.current(` of `read_brand` and the one call of `rp_name(`)",
        Limit::Reads {
            calls: 1,
            reads: 2,
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
        "kernel/obop-http/src/mcp.rs",
        "the anonymous documents (S3): `document_brand` gets the reader of the dispatcher once and asks it through the blocking pool (`brand::current`), and hands the brand out by value; the handlers of the protected-resource documents and of the server card, the metadata of the authorization server and the OpenAPI document each ask it once, first, and build the whole document from that one value; it names no type of the module and builds no reader. The two calls are `dispatcher.brand()` and the accessor `Shown::brand()` of `document_brand`; that each handler asks once and first is held by `each_anonymous_document_asks_the_reader_once_and_first`",
        Limit::Reads {
            calls: 2,
            reads: 0,
            paths: &["current"],
            types: &[],
        },
    ),
    (
        "kernel/obop-mcp/src/lib.rs",
        "`initialize` (S3) gets the reader of the dispatcher it was handed, asks it once for the brand and builds the `serverInfo` and the instructions from that one value; it reads nothing else and builds no reader. The two calls are `dispatcher.brand()` and the accessor `Shown::brand()`; the one `.current(` is the one ask",
        Limit::Reads {
            calls: 2,
            reads: 1,
            paths: &[],
            types: &[],
        },
    ),
    (
        "kernel/obop-connectors/src/runtime.rs",
        "the Slack message (S3): `signer` gets the reader of the dispatcher the runtime holds and reads the name for itself, uncached (`fresh`, so that a refill another request runs cannot hand it the default), once for a send, for the name that signs the message, before the flow is logged, the run is recorded or anything leaves (a read that met a fault of the store refuses the send and leaves nothing behind); `encode` writes that name in the body, so a retry sends the text it was made with; it builds no reader. The two calls are `dispatcher.brand()` and the accessor `Shown::brand()`; the one `.fresh(` is the one ask",
        Limit::Reads {
            calls: 2,
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
            // The brand of a `Shown` (`reader.current().brand()`) is not the accessor of the
            // dispatcher; its `.current(` is counted as a read.
            if !on_a_current_answer(code, at) {
                hit(at, CALL, vec![]);
            }
        } else if colons || imports.iter().any(|(from, to)| (*from..*to).contains(&at)) {
            // `Dispatcher::brand` handed on (`.map(Dispatcher::brand)`), or `use x::brand;`, or
            // the module in a group or under another name (`use x::{brand as bb, Y};`, where the
            // later `bb::strict` names no `brand`): in a `use`, a `brand` that no `::` follows is
            // the module imported.
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
            "kernel/obop-commands/src/public.rs",
            "fn f(&self) { crate::brand::site_settings(&*self.store.backend, &self.d.brand()) }",
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
            "use obop_commands::brand::Shown; fn read_brand(s: &S) -> Arc<Shown> { s.dispatcher.brand().current() } fn site(s: &S) -> String { super::passkeys::rp_name(s) }",
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
            "kernel/obop-http/src/mcp.rs",
            "async fn f(s: &S) -> Brand { super::brand::current(s.dispatcher.brand()).await.brand().clone() }",
        ),
        (
            "kernel/obop-mcp/src/lib.rs",
            "fn f(d: &D) { let shown = d.brand().current(); let b = shown.brand(); }",
        ),
        (
            "kernel/obop-connectors/src/runtime.rs",
            "fn f(&self) { let read = self.dispatcher.brand().fresh(); let n = read.unwrap().shown().brand().name(); }",
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

/// The rows with limits are the two files of the logo route, the settings door of the public site
/// (`PublicApi::site_settings_read`, since slice S1f), the four files of the enrolment texts (since
/// slice S1e: the authenticator's configuration, the error mapper of the routes, the passkey routes
/// and the OAuth pages' route file, which has one row for both) and the three files of the pages of
/// the authorization flow (since slice S1c), and no others: loosening one of them to `Limit::Any` is a
/// change of this test, and so a decision that shows in the diff.
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
            "kernel/obop-commands/src/public.rs",
            "kernel/obop-auth/src/mfa.rs",
            "kernel/obop-http/src/mfa.rs",
            "kernel/obop-http/src/passkeys.rs",
            "kernel/obop-http/src/oauth.rs",
            "kernel/obop-oauth/src/page.rs",
            "kernel/obop-oauth/src/flow.rs",
            // S3: the anonymous documents, `initialize` and the Slack message only ask the reader.
            "kernel/obop-http/src/mcp.rs",
            "kernel/obop-mcp/src/lib.rs",
            "kernel/obop-connectors/src/runtime.rs"
        ]
    );
}

/// What each of those rows lets its file do, literally: how many `brand()` calls, how many reads of
/// the reader, which paths through the module `brand` and which types as types only. The test above
/// holds which files have limits; this holds the limits, so that a row loosened by a number or a name
/// (a second call, a read, `apply` among the paths) is a change of this test, and so a decision that
/// shows in the diff. Every row with limits has its own line here.
#[test]
fn the_limits_of_the_rows_with_limits_are_the_ones_decided() {
    type Want = (
        &'static str,
        usize,
        usize,
        &'static [&'static str],
        &'static [&'static str],
    );
    let decided: [Want; 9] = [
        (
            "kernel/obop-http/src/brand.rs",
            1,
            1,
            &["Reader", "Shown"],
            &["Reader", "Shown"],
        ),
        (
            "kernel/obop-http/src/lib.rs",
            0,
            0,
            &["routes", "NotFound"],
            &[],
        ),
        (
            "kernel/obop-commands/src/public.rs",
            1,
            0,
            &["site_settings"],
            &[],
        ),
        (
            "kernel/obop-auth/src/mfa.rs",
            0,
            0,
            &["env_issuer", "issuer_warning", "strict", "enrolment_error"],
            &[],
        ),
        ("kernel/obop-http/src/mfa.rs", 0, 0, &["UNREADABLE"], &[]),
        (
            "kernel/obop-http/src/passkeys.rs",
            2,
            3,
            &["enrolment_error", "no_reader_error"],
            &[],
        ),
        (
            "kernel/obop-http/src/oauth.rs",
            1,
            2,
            &["Shown"],
            &["Shown"],
        ),
        ("kernel/obop-oauth/src/page.rs", 4, 0, &["Shown"], &[]),
        ("kernel/obop-oauth/src/flow.rs", 0, 0, &["Shown"], &[]),
    ];
    for (file, calls, reads, paths, types) in decided {
        let Some((
            _,
            _,
            Limit::Reads {
                calls: c,
                reads: r,
                paths: p,
                types: t,
            },
        )) = ALLOWED.iter().find(|(f, _, _)| *f == file)
        else {
            panic!("{file} has no row with limits");
        };
        assert_eq!((*c, *r, *p, *t), (calls, reads, paths, types), "{file}");
    }
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
        // The route file of the OAuth pages asks the reader once in `read_brand` (S1c) and its
        // passkey step calls the helper once, as the value of a field (S1e): two reads, and the row
        // lets the file make two.
        (
            oauth,
            "fn read_brand(s: &S) -> Arc<Shown> { s.dispatcher.brand().current() } fn f(s: &S) -> Ctx { Ctx { rp_name: super::passkeys::rp_name(s) } }",
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
        // The OAuth pages' site: the file's two reads are the `.current(` of `read_brand` and one call
        // of the helper, so a second call of the helper is a third read; a reader asked for in the
        // file beyond them is found; so is any path through the module.
        (
            oauth,
            "fn read_brand(s: &S) -> Arc<Shown> { s.dispatcher.brand().current() } fn f(s: &S) { let _ = super::passkeys::rp_name(s); let _ = super::passkeys::rp_name(s); }",
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
        (
            oauth,
            "fn f(d: &Dispatcher) { let _ = d.brand(); let _ = d.brand(); }",
            CALL,
        ),
        (
            oauth,
            "fn f(r: &Reader) { let _ = r.current(); let _ = r.current(); let _ = r.current(); }",
            READ,
        ),
        // The helper called once and the reader asked twice: the third read is the second `.current(`.
        (
            oauth,
            "fn f(r: &Reader, s: &S) { let _ = r.current(); let _ = super::passkeys::rp_name(s); let _ = r.current(); }",
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

const LITERAL_ALLOWED: [LiteralRow; 3] = [
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

/// An `oauth.rs` with the two literals of the rows. (The literal `"obop"` of `webauthn_ctx`, the
/// relying-party name of the passkey step, had a fourth row, `today`, that the enrolment slice
/// removes: since S1e the function takes the name from the reader.)
const OAUTH_RS: &str = "fn cors() { let _ = \"Retry-After, X-Obop-Applied\"; }\n\
     fn cookie_name() { let _ = format!(\"obop_oauth_{}\", 1); }\n";

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
    let moved = OAUTH_RS.replace("fn cookie_name()", "fn other()");
    let (found, drift) = literal_scan(&tree(&[(PAGE_PATH, &page_with(35)), (OAUTH_PATH, &moved)]));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("`other`"), "{found:?}");
    assert!(drift.iter().any(|d| d.contains("cookie_name")), "{drift:?}");

    // The relying-party name of the passkey step is no longer a row (S1e takes it from the reader):
    // the literal `"obop"` back in `webauthn_ctx` is found, with the item, and no row is left over.
    let back = format!("{OAUTH_RS}fn webauthn_ctx() {{ let _ = \"obop\"; }}\n");
    let (found, drift) = literal_scan(&tree(&[(PAGE_PATH, &page_with(35)), (OAUTH_PATH, &back)]));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("`webauthn_ctx`"), "{found:?}");
    assert!(drift.is_empty(), "{drift:?}");

    // A different literal in the item of a row with an exact literal is not let through.
    let widened = OAUTH_RS.replace("obop_oauth_{}", "obop_oauth_{} shop");
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
/// nothing else: the handlers get the reader of the dispatcher once and ask it once (the passkey
/// step of the same file makes one more display read, through the helper of the passkey routes,
/// S1e) and build none; the page functions and the flow context only use the value they were handed.
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
        // A second question to the dispatcher, and a third read (the row lets the file make two: the
        // `.current(` of `read_brand` and the helper of the passkey step, S1e), in the handlers.
        (
            route,
            "fn f(s: &S) { let _ = s.dispatcher.brand(); let _ = s.dispatcher.brand(); }",
            CALL,
        ),
        (
            route,
            "fn f(r: &Reader) { let _ = r.current(); let _ = r.current(); let _ = r.fresh(); }",
            READ,
        ),
        (
            route,
            "fn f(r: &Reader) { let _ = r.current(); let _ = r.current(); let _ = r.strict(); }",
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
    // The file, the calls of the accessor, the reads, the paths and the types the row lists.
    type Want = (
        &'static str,
        usize,
        usize,
        &'static [&'static str],
        &'static [&'static str],
    );
    let want: [Want; 4] = [
        (
            "kernel/obop-auth/src/mfa.rs",
            0,
            0,
            &["env_issuer", "issuer_warning", "strict", "enrolment_error"],
            &[],
        ),
        ("kernel/obop-http/src/mfa.rs", 0, 0, &["UNREADABLE"], &[]),
        (
            "kernel/obop-http/src/passkeys.rs",
            2,
            3,
            &["enrolment_error", "no_reader_error"],
            &[],
        ),
        // One row for the pages of the authorization flow (S1c: one `read_brand`, one value of the
        // type `Shown`) and the ceremony site of its passkey step (S1e: one call of the helper): two
        // reads.
        (
            "kernel/obop-http/src/oauth.rs",
            1,
            2,
            &["Shown"],
            &["Shown"],
        ),
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
    for (path, calls, reads, paths, types) in want {
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
        assert_eq!((c, r, p, t), (calls, reads, paths, types), "{path}");
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

// ---- One read for each request that makes a page (slice S1c) ----------------------------------------
//
// The scan for the one reader counts what a file asks of the reader, and the handlers ask it in one
// helper, `read_brand`, so a second call of the helper in a handler is a second question that the
// counts above cannot see. The pages of a request must come from one value (the reader answers from
// its cell within two seconds, but a refill in between may change the answer, and a request that
// shows the name twice should not show two names). So the calls of `read_brand` are pinned too:
// exactly one in each of the four functions where a request of the flow begins, and none anywhere
// else in the file. (The two passkey routes of the flow also ask the reader for the display name of
// their ceremony site, S1e, through the helper `rp_name(`: that is the second of the two reads the
// row of the file lets it make, a hit of the reader's cell, and no page shows what it returns.)

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

// ---- The scan for literals, extended (slice S3) ------------------------------------------------------
//
// From S3 `initialize`, the discovery documents and the title of the OpenAPI document read the row (their
// tests name the words that change: `crates/obop/src/brand_docs_tests.rs`), and so does the Slack message
// (`crates/modules/connectors/src/tests.rs`), besides the OAuth pages of S1c; the module doc above is
// the first slice's and is left as it was. The row of `kernel/obop-http/src/lib.rs` in `ALLOWED` says
// that `api_routes` reads nothing, and that stays true of the scan of that file; the OpenAPI handler
// there asks the reader through `mcp::document_brand`, a call that scan does not see, and
// `each_anonymous_document_asks_the_reader_once_and_first` below holds it.
//
// docs/spec/branding.md, section 13: "the list gains the files S3 changes, so no pending literal ever
// sits on the list", and the word scan of the four crates that the phone's server text comes from. The
// S3 files keep their own list and their own rows (`LITERAL_FILES_S3`, `LITERAL_ALLOWED_S3`) and are
// read by the same scan as the files of the pages above; the test of the whole tree reads both lists.
//
// What the rows say, in the classes above: the six files hold the texts of the MCP server, the
// discovery documents, the OpenAPI title, the Slack message and the Drive browser. The texts that must
// stay byte for byte for an instance with no brand are `today` rows and name what a brand puts in their
// place (the instructions of `initialize`, the three texts of the discovery documents, the two links
// to the documentation, the title of the API); the Slack prefix and the Drive sentence have no row:
// the first is the name of the brand, which is `obop` when none is set, and the second names no product
// on any instance.

/// The files S3 changes, whose string literals are scanned.
const LITERAL_FILES_S3: [&str; 6] = [
    "kernel/obop-http/src/mcp.rs",
    "kernel/obop-http/src/openapi.rs",
    "kernel/obop-mcp/src/lib.rs",
    "kernel/obop-oauth/src/lib.rs",
    "kernel/obop-connectors/src/runtime.rs",
    "kernel/obop-blob-gdrive/src/browse.rs",
];

const MCP_HTTP: &str = "kernel/obop-http/src/mcp.rs";
const OPENAPI: &str = "kernel/obop-http/src/openapi.rs";
const MCP_CORE: &str = "kernel/obop-mcp/src/lib.rs";
const OAUTH_CORE: &str = "kernel/obop-oauth/src/lib.rs";
const RUNTIME: &str = "kernel/obop-connectors/src/runtime.rs";
const BROWSE: &str = "kernel/obop-blob-gdrive/src/browse.rs";

const LITERAL_ALLOWED_S3: [LiteralRow; 31] = [
    (
        MCP_HTTP,
        "from_env",
        None,
        2,
        "env",
        "the names of the two environment variables that configure the endpoint (OBOP_PUBLIC_URL and \
         OBOP_MCP_RATE_PER_MINUTE), read at start: an operator's words, never shown to a person who \
         connects",
    ),
    (
        MCP_HTTP,
        "with_cors",
        Some("Mcp-Session-Id, WWW-Authenticate, Retry-After, X-Obop-Applied"),
        1,
        "wire",
        "the response headers a browser client may read from /mcp: header names on the wire, one of \
         them the X-Obop-Applied of D74, never shown to a person",
    ),
    (
        MCP_HTTP,
        "words",
        Some("obop instance MCP server"),
        1,
        "today",
        "the name of the MCP server in the protected-resource documents (RFC 9728 resource_name), as an \
         instance with no brand has always sent it. The branded counterpart is `{name} MCP server`, \
         built in the same function and sent whenever a brand is set",
    ),
    (
        MCP_HTTP,
        "words",
        Some("obop instance"),
        1,
        "today",
        "the title of the MCP server card, as an instance with no brand has always sent it. The branded \
         counterpart is the name of the brand, built in the same function and sent whenever a brand is set",
    ),
    (
        MCP_HTTP,
        "words",
        Some(
            "Business data and actions of this obop instance as MCP tools, with masking and approvals.",
        ),
        1,
        "today",
        "the description of the MCP server card, as an instance with no brand has always sent it. The \
         branded counterpart is `Business data and actions of {name}, with masking and approvals.`, \
         shorter so that it fits the 100 characters the card allows for a name of 40",
    ),
    (
        MCP_HTTP,
        "DOCUMENTATION",
        Some("https://obop.dev/docs/agents/mcp/"),
        1,
        "today",
        "the link to the documentation of the software in the protected-resource documents \
         (resource_documentation, optional in RFC 9728). An instance with no brand sends it as it always \
         has; the branded counterpart is that the member is left out whenever a brand is set",
    ),
    (
        MCP_HTTP,
        "protected_resource",
        Some("x-obop-token-endpoints"),
        1,
        "wire",
        "an extension member of the protected-resource document that names the two token endpoints: a \
         name on the wire that clients read, never shown to a person",
    ),
    (
        MCP_HTTP,
        "server_card",
        Some("{}/obop"),
        1,
        "identifier",
        "the identifier of the server card, `<host>/obop` (the schema's namespace/name): a name on the \
         wire that registries key on, which a brand does not change (spec, section 11)",
    ),
    (
        MCP_HTTP,
        "server_card",
        Some("dev.obop/auth"),
        1,
        "wire",
        "the key of the extension block of the card that names the authorization endpoints: a name on \
         the wire, never shown to a person (spec, section 11)",
    ),
    (
        MCP_HTTP,
        "with_oauth",
        Some("dev.obop/auth"),
        2,
        "wire",
        "the same extension key, read twice to name the authorization server in the card: a name on the \
         wire, never shown to a person",
    ),
    (
        MCP_HTTP,
        "ai_catalog",
        Some("urn:air:obop:{}:mcp"),
        1,
        "identifier",
        "the identifier of the catalogue entry (an AI Catalog urn): a name on the wire that scanners key \
         on, which a brand does not change (spec, section 11)",
    ),
    (
        OPENAPI,
        "limit_keywords",
        None,
        2,
        "wire",
        "the extension keywords x-obop-minimum and x-obop-maximum of a decimal's bounds in the document: \
         names on the wire that tools read, never shown to a person",
    ),
    (
        OPENAPI,
        "error_schema",
        None,
        2,
        "wire",
        "two descriptions of members of the error body that name the cookie obop_lang and the header \
         X-Obop-Applied: wire names that a developer reads in the API description",
    ),
    (
        OPENAPI,
        "STORAGE_503",
        None,
        1,
        "wire",
        "the description of the 503 answer, which names the header X-Obop-Applied: a wire name that a \
         developer reads in the API description",
    ),
    (
        OPENAPI,
        "error_responses",
        Some("X-Obop-Applied"),
        1,
        "wire",
        "the response header of the 503 answer, as the document describes it: a name on the wire",
    ),
    (
        OPENAPI,
        "platform",
        Some("obop control plane API"),
        1,
        "admin",
        "the title of the control plane's API, which is SpaceCorps' own service and not an instance's \
         (spec, section 6, OT-12): it names the operator's product on purpose and no brand reaches it",
    ),
    (
        OPENAPI,
        "platform",
        None,
        1,
        "admin",
        "the description of the control plane's API (the other literal of the function, the title being \
         the row above): it tells a developer how to create an instance and names the product the \
         operator sells",
    ),
    (
        OPENAPI,
        "instance",
        Some("The server has no OBOP_SECRET_KEY."),
        1,
        "env",
        "the description of the 503 answer of the authenticator enrolment, which names the environment \
         variable an operator must set: a developer's text, not an end user's",
    ),
    (
        OPENAPI,
        "instance",
        Some(", except in bulk_create and obop import, which round with {r}"),
        1,
        "admin",
        "the description of a decimal field, which names the command line `obop import`: a developer's \
         text that names the software they run",
    ),
    (
        OPENAPI,
        "instance",
        Some("obop instance API"),
        1,
        "today",
        "the title of the instance's API, as an instance with no brand has always sent it and as \
         `obop openapi` writes it (the committed document is the software's, never given a brand). The \
         branded counterpart is `{name} API`, set by `describe_brand` on the document of a running \
         instance whenever a brand is set",
    ),
    (
        MCP_CORE,
        "INSTRUCTIONS_LEAD",
        None,
        1,
        "today",
        "the first sentence of the instructions of `initialize`, as an instance with no brand has always \
         sent it. The branded counterpart is INSTRUCTIONS_LEAD_BRANDED, `This is the {name} system: \
         business data (...)`, which makes no claim about the software and is sent whenever a brand is set",
    ),
    (
        MCP_CORE,
        "INSTRUCTIONS_REST",
        None,
        1,
        "wire",
        "the rest of the instructions, the same on every instance: they name the tool obop__meta, which \
         keeps its name (a wire name every client and prompt knows; Q71 item 3, answered)",
    ),
    (
        OAUTH_CORE,
        "authorization_server_metadata",
        Some("obop_client_classes_supported"),
        1,
        "wire",
        "the member of the metadata that names the client classes (mcp, mobile) a client may register \
         as: a name on the wire, never shown to a person (spec, section 11)",
    ),
    (
        OAUTH_CORE,
        "authorization_server_metadata",
        Some("https://obop.dev/docs/agents/mcp/"),
        1,
        "today",
        "the link to the documentation of the software in the metadata (service_documentation, optional \
         in RFC 8414). An instance with no brand sends it as it always has; the branded counterpart is \
         that the member is left out whenever a brand is set",
    ),
    (
        RUNTIME,
        "from_vars",
        Some("OBOP_PUBLIC_URL"),
        1,
        "env",
        "the name of the environment variable that holds the public address of the instance, for the \
         addresses of webhooks and of the OAuth redirect: an operator's words",
    ),
    (
        RUNTIME,
        "encode",
        Some("metadata[obop_ref]"),
        1,
        "wire",
        "the form field that carries the reference of a record to a receiver that encodes forms: a name \
         on the wire, which the event that comes back is matched on",
    ),
    (
        RUNTIME,
        "send_all",
        Some("obop.manual"),
        1,
        "wire",
        "the event name of a manual send, as the receiver sees it in the type of the message: a name on \
         the wire",
    ),
    (
        RUNTIME,
        "oauth_config",
        Some(
            "an administrator must first enter the OAuth client id and secret for this provider (obop ships none)",
        ),
        1,
        "admin",
        "the refusal an administrator reads in the connector wizard before the OAuth client of a \
         provider is entered: it names the software they run, and is admin wording (spec, section 11)",
    ),
    (
        RUNTIME,
        "oauth_config",
        Some("OBOP_PUBLIC_URL is not set, so there is no address for the OAuth redirect"),
        1,
        "env",
        "the refusal an administrator reads when the public address is not set: it names the \
         environment variable an operator must set",
    ),
    (
        RUNTIME,
        "test",
        Some("obop.test"),
        1,
        "wire",
        "the event name of the test message of a connection, as the receiver sees it in the type of the \
         message: a name on the wire",
    ),
    (
        BROWSE,
        "from_env",
        None,
        6,
        "env",
        "the names of the environment variables that configure the Drive browser (the client, the \
         refresh token, the API and token addresses of the tests): an operator's words",
    ),
];

// The scan of the pages above (`named_literals`, `literal_row`, `literal_scan` and their two tests)
// reads its own list and is left as it was; the functions below are the same scan over any list of
// files and any rows, which S3 reads with its own (`LITERAL_FILES_S3`, `LITERAL_ALLOWED_S3`). Once
// the slices that edit this file have merged, the two can be folded into one.

/// As `named_literals`, for any list of files: every literal of the listed files that names obop, in
/// the order of the files and of the source.
fn named_literals_of(files: &BTreeMap<String, String>, list: &[&str]) -> Vec<Named> {
    let lexed = non_test(files);
    let mut out = Vec::new();
    for path in list.iter().copied() {
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

/// As `literal_row`, for any rows: the row that lets this literal stand, if any.
fn literal_row_of(n: &Named, rows: &[LiteralRow]) -> Option<usize> {
    rows.iter().position(|(file, item, lit, _, _, _)| {
        *file == n.path && *item == n.item && lit.is_none_or(|l| l == n.text)
    })
}

/// As `literal_scan`, for any list of files and any rows: the literals that no row lets stand and the
/// listed files that are missing (`found`), and the rows that match fewer or more literals than they
/// say (`drift`).
fn literal_scan_of(
    files: &BTreeMap<String, String>,
    list: &[&str],
    rows: &[LiteralRow],
) -> (Vec<String>, Vec<String>) {
    let named = named_literals_of(files, list);
    let lexed = non_test(files);
    let mut found = Vec::new();
    for path in list.iter().copied() {
        if !lexed.contains_key(path) {
            found.push(format!(
                "{path}: a file of the list that is not in the tree (or is all test code)"
            ));
        }
    }
    for path in list.iter().copied() {
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
    let (unlisted, drift) = judge(&named, rows, "LITERAL_ALLOWED_S3");
    found.extend(unlisted);
    (found, drift)
}

/// What the rows say of the literals found: those that no row lets stand, and the rows that match
/// fewer or more literals than they say.
fn judge(named: &[Named], rows: &[LiteralRow], table: &str) -> (Vec<String>, Vec<String>) {
    let mut found = Vec::new();
    let mut seen = vec![0usize; rows.len()];
    for n in named {
        match literal_row_of(n, rows) {
            Some(i) => seen[i] += 1,
            None => found.push(format!(
                "{}:{}: a literal that names obop is not a row of {table} (in `{}`): {:?}",
                n.path,
                n.line,
                n.item,
                cut(&n.text)
            )),
        }
    }
    let drift = rows
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

/// The test of the whole tree for the files of S3: every literal that names obop in one of them is a
/// row, and every row is met as often as it says (the first slice's test reads its own list).
#[test]
fn no_literal_that_names_obop_is_added_to_a_file_of_slice_s3() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = BTreeMap::new();
    read_tree(&root, &root, &mut files);
    let (found, drift) = scan_s3(&files);
    assert!(
        found.is_empty(),
        "a file of slice S3 has a literal that names obop (use the brand, or add a row to \
         LITERAL_ALLOWED_S3 with its class and its reason):\n{}",
        found.join("\n")
    );
    assert!(
        drift.is_empty(),
        "rows of LITERAL_ALLOWED_S3 that match fewer or more literals than they say:\n{}",
        drift.join("\n")
    );
}

#[test]
fn every_row_of_the_s3_literal_list_has_a_known_class_a_reason_and_a_file_of_the_list() {
    for (file, item, lit, count, class, why) in LITERAL_ALLOWED_S3 {
        assert!(
            LITERAL_FILES_S3.contains(&file),
            "{file} is not in LITERAL_FILES_S3"
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
    for file in LITERAL_FILES_S3 {
        assert!(
            LITERAL_ALLOWED_S3.iter().any(|r| r.0 == file),
            "{file} is on the list and has no row, so it could never hold a name"
        );
    }
}

/// The files of the S3 list as they are in the tree, and only those: the base of the self-tests
/// below, which change one of them in memory and read what the scan says.
fn s3_files_of_the_tree() -> BTreeMap<String, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut all = BTreeMap::new();
    read_tree(&root, &root, &mut all);
    LITERAL_FILES_S3
        .iter()
        .map(|f| {
            let src = all
                .get(*f)
                .unwrap_or_else(|| panic!("{f} is not in the tree"));
            (f.to_string(), src.clone())
        })
        .collect()
}

fn scan_s3(files: &BTreeMap<String, String>) -> (Vec<String>, Vec<String>) {
    literal_scan_of(files, &LITERAL_FILES_S3, &LITERAL_ALLOWED_S3)
}

/// The scan reads the files of S3 as they are: every literal that names obop is a row, and every row
/// is met as often as it says. (The test of the whole tree says the same; this one is the base of
/// the changes that follow.)
#[test]
fn the_files_of_slice_s3_hold_the_literals_the_rows_say_and_no_other() {
    let (found, drift) = scan_s3(&s3_files_of_the_tree());
    assert!(found.is_empty() && drift.is_empty(), "{found:?} {drift:?}");
}

/// A literal `obop` added to any file of S3 is found, with the file, the line and the item; a
/// different text in the item of a row with an exact literal is not let through; and a row that no
/// longer matches what it says is a failure.
#[test]
fn the_scan_finds_a_new_literal_in_each_file_of_slice_s3_and_a_row_that_no_longer_matches() {
    let base = s3_files_of_the_tree();
    for file in LITERAL_FILES_S3 {
        let mut files = base.clone();
        files
            .get_mut(file)
            .unwrap()
            .push_str("\nfn added_by_a_test() -> &'static str {\n    \"Welcome to Obop\"\n}\n");
        let (found, drift) = scan_s3(&files);
        assert_eq!(found.len(), 1, "{file}: {found:?}");
        assert!(
            found[0].starts_with(&format!("{file}:")) && found[0].contains("`added_by_a_test`"),
            "{found:?}"
        );
        assert!(drift.is_empty(), "{file}: {drift:?}");
    }
    // The title of the API changed to another text that names the product: not the row's literal.
    let mut files = base.clone();
    let src = files.get_mut(OPENAPI).unwrap();
    assert!(src.contains("\"obop instance API\""));
    *src = src.replace("\"obop instance API\"", "\"an obop API\"");
    let (found, drift) = scan_s3(&files);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("`instance`"), "{found:?}");
    assert!(
        drift
            .iter()
            .any(|d| d.contains("obop instance API") && d.contains("has 0")),
        "{drift:?}"
    );
    // A row whose literal is gone from its file: the wire name of the card's extension block.
    let mut files = base.clone();
    let src = files.get_mut(MCP_HTTP).unwrap();
    let before = src.matches("\"dev.obop/auth\"").count();
    assert_eq!(before, 3, "the card names the block three times");
    *src = src.replacen("\"dev.obop/auth\"", "\"dev.example/auth\"", 1);
    let (found, drift) = scan_s3(&files);
    assert!(found.is_empty(), "{found:?}");
    assert_eq!(drift.len(), 1, "{drift:?}");
    assert!(
        drift[0].contains("server_card") && drift[0].contains("has 0"),
        "{drift:?}"
    );
    // A file of the list that is gone from the tree is a failure, not a pass.
    let mut files = base;
    files.remove(MCP_CORE);
    let (found, _) = scan_s3(&files);
    assert!(found.iter().any(|f| f.starts_with(MCP_CORE)), "{found:?}");
}

/// The rows that say a text must stay for an instance with no brand name what a brand puts in its
/// place; this holds that the branded text exists in the file of the row, so that a row cannot
/// outlive the counterpart it names. (The documentation links' counterpart is an absence, held by
/// the tests of the documents.)
#[test]
fn the_branded_counterpart_each_today_row_of_slice_s3_names_is_in_its_file() {
    let files = s3_files_of_the_tree();
    let lexed = non_test(&files);
    for (file, text) in [
        (MCP_HTTP, "{name} MCP server"),
        (
            MCP_HTTP,
            "Business data and actions of {name}, with masking and approvals.",
        ),
        (OPENAPI, "{} API"),
        (MCP_CORE, "This is the {name} system: business data"),
    ] {
        assert!(
            lexed[file].strings.iter().any(|(_, _, t)| t.contains(text)),
            "{file}: the branded counterpart {text:?} is not in the file"
        );
    }
    // And each `today` row of the list says "counterpart" (the test of the rows requires it); this
    // counts them, so a row added without its entry above makes the author look at this list.
    let today = LITERAL_ALLOWED_S3.iter().filter(|r| r.4 == "today").count();
    assert_eq!(
        today, 7,
        "the `today` rows of S3 and their counterparts above"
    );
}

// ---- The word scan of the server text (slice S3) -----------------------------------------------------
//
// Spec section 6, HP-11, and section 13: the server texts that reach a phone come from `obop-commands`,
// `obop-registry`, `obop-masking` and the mileage module, and none of them says "obop" as a word. The
// scan reads the non-test code of the four and fails on any string literal in which `obop` stands as a
// word ("the obop CLI", "obop.dev", "obop: failed", "obop's limit", "(obop)", "Ask obop!",
// "github.com/SpaceCorps/obop"), unless a row of [`WORD_ALLOWED`] lets it stand. An identifier is not a
// word and is not matched: `obop` with a letter, a digit, `_` or `-` on either side (`obop_setting`,
// `x-obop-minimum`, `OBOP_PUBLIC_URL`, `myobop`), and the dotted name that goes on as a path
// (`dev.obop/auth`). The hyphen is the price of that: a compound in prose (`an obop-style message`)
// is not found, since a text cannot be told from the name of a header by its look (accepted, and
// held as it is by `the_word_is_told_from_an_identifier`). The spec's own sentence (section 6, HP-11: "a space, a point or the end of the
// literal") is narrower than its purpose, that no prose `obop` reaches a phone; the scan keeps the
// purpose, since a name followed by a comma, a colon, an apostrophe or a bracket is as much prose.

/// The four crates, as path prefixes under `crates/`.
const WORD_CRATES: [&str; 4] = [
    "kernel/obop-commands/",
    "kernel/obop-registry/",
    "kernel/obop-masking/",
    "modules/mileage/",
];

const WORD_ALLOWED: [LiteralRow; 3] = [
    (
        "kernel/obop-commands/src/brand/reader.rs",
        "note",
        None,
        1,
        "admin",
        "the line the process writes to its own stderr, once for each stored field of the branding that fell back to its default; it names the command an operator runs (`obop admin brand status`). It is an operator's text, never sent to a person who uses the instance",
    ),
    (
        "kernel/obop-registry/src/lib.rs",
        "first_party",
        Some("https://github.com/SpaceCorps/obop"),
        2,
        "admin",
        "the homepage and the repository that a module declares when it names none: the address of the project on GitHub, metadata that an administrator reads in the module list (spec, section 11: module author, homepage and repository are admin wording)",
    ),
    (
        "kernel/obop-commands/src/brand/enrol.rs",
        "store_read_line",
        None,
        1,
        "admin",
        "the line the process writes to its own stderr (S1e) when an enrolment was refused because the store could not be read for the branding; it names the command an operator runs (`obop admin brand status`) and carries the store's text with its control characters blanked. It is an operator's text: the person who enrols is answered with the fixed message of `UNREADABLE`, which names no product",
    ),
];

/// Whether a character belongs to an identifier: a letter, a digit, `_` or `-`.
fn identifier_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

/// Whether `obop` stands in the text as a word: at least once with no identifier character on either
/// side of it, and not as the middle of a dotted name that goes on as a path (`dev.obop/auth`).
fn names_obop_as_a_word(text: &str) -> bool {
    let lower = text.to_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("obop") {
        let at = from + rel;
        let end = at + "obop".len();
        let before = lower[..at].chars().next_back();
        let after = lower[end..].chars().next();
        let identifier = before.is_some_and(identifier_char)
            || after.is_some_and(identifier_char)
            || (before == Some('.') && after == Some('/'));
        if !identifier {
            return true;
        }
        from = end;
    }
    false
}

/// Every string literal of the non-test code of the four crates in which `obop` is a word.
fn word_literals(files: &BTreeMap<String, String>) -> Vec<Named> {
    let lexed = non_test(files);
    let mut out = Vec::new();
    for (path, l) in &lexed {
        if !WORD_CRATES.iter().any(|c| path.starts_with(c)) {
            continue;
        }
        for (start, _, text) in &l.strings {
            let raw = files
                .get(path)
                .is_some_and(|src| src.as_bytes().get(*start) == Some(&b'r'));
            let seen = if raw { text.clone() } else { unescaped(text) };
            if names_obop_as_a_word(&seen) {
                out.push(Named {
                    path: path.clone(),
                    item: item_of(&l.code, *start),
                    text: text.clone(),
                    line: line_of(&l.code, *start),
                });
            }
        }
    }
    out
}

/// The literals in which `obop` is a word that no row lets stand (`found`), and the rows that match
/// fewer or more than they say (`drift`); a crate of the four with no file in the tree is a failure.
fn word_scan(files: &BTreeMap<String, String>) -> (Vec<String>, Vec<String>) {
    let lexed = non_test(files);
    let mut found = Vec::new();
    for c in WORD_CRATES {
        if !lexed.keys().any(|p| p.starts_with(c)) {
            found.push(format!(
                "{c}: a crate of the word scan with no file in the tree (it was renamed or moved)"
            ));
        }
    }
    // A text put together out of parts is read by no scan of literals, as in the scan of the S3 files.
    for (path, l) in &lexed {
        if !WORD_CRATES.iter().any(|c| path.starts_with(c)) {
            continue;
        }
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
    let (unlisted, drift) = judge(&word_literals(files), &WORD_ALLOWED, "WORD_ALLOWED");
    found.extend(unlisted);
    (found, drift)
}

#[test]
fn no_text_of_the_server_that_reaches_a_phone_says_obop_as_a_word() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = BTreeMap::new();
    read_tree(&root, &root, &mut files);
    let (found, drift) = word_scan(&files);
    assert!(
        found.is_empty(),
        "a text of obop-commands, obop-registry, obop-masking or the mileage module says `obop` as a \
         word (a person who uses the instance, on a phone or not, would read it; leave the product out, \
         or add a row to WORD_ALLOWED if only an administrator reads it):\n{}",
        found.join("\n")
    );
    assert!(
        drift.is_empty(),
        "rows of WORD_ALLOWED that match fewer or more literals than they say:\n{}",
        drift.join("\n")
    );
}

#[test]
fn the_word_is_told_from_an_identifier() {
    for (text, yes) in [
        ("obop", true),
        ("OBOP", true),
        ("the obop CLI", true),
        ("see obop.dev for more", true),
        ("https://github.com/SpaceCorps/obop", true),
        ("(obop admin brand status lists it)", true),
        ("obop: the stored name", true),
        // The punctuation that follows a name in a sentence (the cases a narrower rule let through).
        ("ask obop, then retry", true),
        ("obop's data", true),
        ("the obop: option", true),
        ("(obop)", true),
        ("is it obop?", true),
        ("see OBOP\n", true),
        ("Welcome to obop!", true),
        ("\"obop\"", true),
        ("obop; and obop", true),
        // An identifier has a letter, a digit, `_` or `-` beside it: the underscore on either side.
        ("x_obop and then", false),
        ("a obop_ b", false),
        ("obop_setting", false),
        ("OBOP_PUBLIC_URL", false),
        ("x-obop-minimum", false),
        ("dev.obop/auth", false),
        ("metadata[obop_ref]", false),
        ("myobop and notobop", false),
        // A digit beside the name, or a letter of any script, makes an identifier too.
        ("obop2 and then", false),
        ("v2obop and then", false),
        ("\u{e9}obop and then", false),
        ("obop\u{e9} and then", false),
        ("\u{682a}obop and then", false),
        // The hyphen is the price of telling `x-obop-minimum` from prose (see the header of this
        // section): a compound in prose is not found. Held as it is.
        ("an obop-style message", false),
        ("", false),
        ("obo p", false),
        ("an obopx and then obop.", true),
    ] {
        assert_eq!(names_obop_as_a_word(text), yes, "{text:?}");
    }
}

#[test]
fn the_word_scan_finds_a_new_text_in_each_of_the_four_crates_and_leaves_the_rest_alone() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut base = BTreeMap::new();
    read_tree(&root, &root, &mut base);
    let (found, drift) = word_scan(&base);
    assert!(found.is_empty() && drift.is_empty(), "{found:?} {drift:?}");
    let with = |path: &str, extra: &str| {
        let mut f = base.clone();
        f.entry(path.to_string())
            .and_modify(|s| s.push_str(extra))
            .or_insert_with(|| extra.to_string());
        word_scan(&f)
    };
    for (path, extra, yes) in [
        // A message a person could read, in each of the four crates (a file of each, new or old).
        (
            "kernel/obop-commands/src/engine/crud.rs",
            "\nfn a() -> &'static str { \"obop cannot do that\" }\n",
            true,
        ),
        (
            "kernel/obop-registry/src/kinds_added_by_a_test.rs",
            "\nfn a() -> &'static str { \"ask obop.\" }\n",
            true,
        ),
        (
            "kernel/obop-masking/src/lib.rs",
            "\nfn a() -> &'static str { \"powered by obop\" }\n",
            true,
        ),
        (
            "modules/mileage/src/model.rs",
            "\nfn a() -> &'static str { \"obop\" }\n",
            true,
        ),
        // A name written with an escape is read as what it stands for (`\u{6f}` is an `o`), but a raw
        // literal has no escapes, and a text put together by `concat!` is refused whole.
        (
            "kernel/obop-commands/src/engine/crud.rs",
            "\nfn a() -> &'static str { \"\\u{6f}bop cannot do that\" }\n",
            true,
        ),
        (
            "kernel/obop-commands/src/engine/crud.rs",
            "\nfn a() -> &'static str { \"\\x6fbop cannot do that\" }\n",
            true,
        ),
        (
            "kernel/obop-commands/src/engine/crud.rs",
            "\nfn a() -> &'static str { r\"\\u{6f}bop is a raw text\" }\n",
            false,
        ),
        (
            "kernel/obop-commands/src/engine/crud.rs",
            "\nfn a() -> &'static str { concat!(\"ob\", \"op cannot do that\") }\n",
            true,
        ),
        // Identifiers are not words, and test code and other crates are not read.
        (
            "kernel/obop-commands/src/engine/crud.rs",
            "\nfn a() -> &'static str { \"obop_record\" }\n",
            false,
        ),
        (
            "kernel/obop-commands/src/engine/crud.rs",
            "\n#[cfg(test)]\nmod t { fn a() -> &'static str { \"obop cannot do that\" } }\n",
            false,
        ),
        (
            "kernel/obop-http/src/lib.rs",
            "\nfn a() -> &'static str { \"obop cannot do that\" }\n",
            false,
        ),
    ] {
        let (found, drift) = with(path, extra);
        assert_eq!(!found.is_empty(), yes, "{path}: {extra}: {found:?}");
        assert!(drift.is_empty(), "{path}: {drift:?}");
    }
    // A row that matches less than it says is a failure too (the registry's address renamed).
    let mut f = base.clone();
    let src = f.get_mut("kernel/obop-registry/src/lib.rs").unwrap();
    assert!(src.contains("https://github.com/SpaceCorps/obop"));
    *src = src.replace(
        "https://github.com/SpaceCorps/obop",
        "https://github.com/SpaceCorps/other",
    );
    let (found, drift) = word_scan(&f);
    assert!(found.is_empty(), "{found:?}");
    assert!(
        drift.iter().any(|d| d.contains("the row says 2")),
        "{drift:?}"
    );
    // A crate that is not in the tree is a failure, not a pass.
    let mut f = base;
    f.retain(|p, _| !p.starts_with("modules/mileage/"));
    let (found, _) = word_scan(&f);
    assert!(
        found.iter().any(|m| m.starts_with("modules/mileage/")),
        "{found:?}"
    );
}

// ---- The anonymous documents ask the reader once, and first (slice S3) ------------------------------
//
// Spec section 4: "a handler asks the Reader once, first, and uses that one `Brand` for every page it
// renders", so that a request that cannot read the store is refused before it has done anything, and
// a document is built from one value and never from two reads that could differ. The rows of
// [`ALLOWED`] count the calls of `.brand()` in a whole file, which cannot tell one ask in each of six
// handlers from two asks in one. This scan can: it finds every use of `document_brand` in the
// non-test code of the crates and requires, for each of the six handlers of the anonymous documents,
// exactly one, with no `.await`, no `blocking(` and no read of the caller's token before it.

/// The handlers that ask for the brand of an anonymous document: the file and the function. The
/// catalogue asks nothing and is not here.
const DOCUMENT_ASKERS: [(&str, &str); 6] = [
    ("kernel/obop-http/src/mcp.rs", "oauth_protected_resource"),
    (
        "kernel/obop-http/src/mcp.rs",
        "oauth_protected_resource_mcp",
    ),
    ("kernel/obop-http/src/mcp.rs", "server_card_json"),
    ("kernel/obop-http/src/mcp.rs", "server_card_canonical"),
    ("kernel/obop-http/src/oauth.rs", "authorization_server"),
    ("kernel/obop-http/src/lib.rs", "instance_openapi"),
];

/// Where a function's signature starts (the `fn` of `fn name`) in the blanked code.
fn fn_start(code: &[u8], name: &str) -> Option<usize> {
    use crate::src_scan::{skip_ws, word_at};
    words(code, "fn")
        .into_iter()
        .find(|p| word_at(code, skip_ws(code, p + 2)) == name)
}

/// What is wrong with the asks for `document_brand` in these files: a use in a function that is not
/// in [`DOCUMENT_ASKERS`], a handler of the table with no ask or with two, and an ask that is not
/// first (an `.await`, a `blocking(` or a read of the caller's token before it).
fn document_asks(files: &BTreeMap<String, String>) -> Vec<String> {
    let lexed = non_test(files);
    let mut problems = Vec::new();
    let mut asked: BTreeMap<(String, String), usize> = BTreeMap::new();
    for (path, l) in &lexed {
        for at in words(&l.code, "document_brand") {
            let before = &l.code[..at];
            let defines = before
                .trim_ascii_end()
                .strip_suffix(b"fn")
                .is_some_and(|rest| rest.last().is_none_or(|c| !crate::src_scan::is_ident(*c)));
            if defines {
                continue;
            }
            let item = crate::src_scan::enclosing_fn(&l.code, at).unwrap_or_default();
            *asked.entry((path.clone(), item.clone())).or_default() += 1;
            let Some(start) = fn_start(&l.code, &item) else {
                continue;
            };
            let head = String::from_utf8_lossy(&l.code[start..at]);
            for bad in [".await", "blocking(", "auth_api("] {
                if head.contains(bad) {
                    problems.push(format!(
                        "{path}:{}: `{item}` does `{bad}` before it asks for the brand (a handler asks first)",
                        line_of(&l.code, at)
                    ));
                }
            }
        }
    }
    for (file, item) in DOCUMENT_ASKERS {
        let n = asked
            .remove(&(file.to_string(), item.to_string()))
            .unwrap_or(0);
        if n != 1 {
            problems.push(format!(
                "{file}: `{item}` asks for the brand {n} times (the rule is once)"
            ));
        }
    }
    for ((file, item), n) in asked {
        problems.push(format!(
            "{file}: `{item}` asks for the brand {n} time(s) and is not a handler of DOCUMENT_ASKERS \
             (add it with its test, or ask through a handler of the list)"
        ));
    }
    problems
}

#[test]
fn each_anonymous_document_asks_the_reader_once_and_first() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = BTreeMap::new();
    read_tree(&root, &root, &mut files);
    let problems = document_asks(&files);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The scan sees a second ask in a handler, a handler that stopped asking, an ask in a function that
/// is not on the list, and an ask that is not first, and leaves the tree as it is alone.
#[test]
fn the_scan_of_the_asks_sees_a_second_one_a_missing_one_a_stray_one_and_a_late_one() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut base = BTreeMap::new();
    read_tree(&root, &root, &mut base);
    assert!(document_asks(&base).is_empty());
    let changed = |path: &str, from: &str, to: &str| {
        let mut f = base.clone();
        let src = f.get_mut(path).unwrap();
        assert_eq!(src.matches(from).count(), 1, "{path}: {from}");
        *src = src.replace(from, to);
        document_asks(&f)
    };
    let mcp = "kernel/obop-http/src/mcp.rs";
    // A second ask in the card's handler.
    let twice = changed(
        mcp,
        "pub(crate) async fn server_card_json(State(s): State<Arc<HttpState>>, h: HeaderMap) -> Response {\n    let brand = document_brand(&s).await;",
        "pub(crate) async fn server_card_json(State(s): State<Arc<HttpState>>, h: HeaderMap) -> Response {\n    let brand = document_brand(&s).await;\n    let _again = document_brand(&s).await;",
    );
    // (The second ask is also a late one, behind the first `.await`: both are found.)
    assert!(
        twice
            .iter()
            .any(|p| p.contains("`server_card_json` asks for the brand 2 times")),
        "{twice:?}"
    );
    // The OpenAPI handler asks twice.
    let twice = changed(
        "kernel/obop-http/src/lib.rs",
        "let brand = mcp::document_brand(&s).await;",
        "let brand = mcp::document_brand(&s).await;\n    let _again = mcp::document_brand(&s).await;",
    );
    assert!(
        twice
            .iter()
            .any(|p| p.contains("`instance_openapi` asks for the brand 2 times")),
        "{twice:?}"
    );
    // A handler that stopped asking.
    let none = changed(
        "kernel/obop-http/src/oauth.rs",
        "let brand = super::mcp::document_brand(&s).await;",
        "let brand = obop_brand::Brand::default();",
    );
    assert_eq!(none.len(), 1, "{none:?}");
    assert!(none[0].contains("`authorization_server` asks for the brand 0 times"));
    // An ask in a function the table does not list.
    let stray = changed(
        mcp,
        "const DOCUMENTATION: &str",
        "async fn another(s: &HttpState) { let _ = document_brand(s).await; }\n\nconst DOCUMENTATION: &str",
    );
    assert_eq!(stray.len(), 1, "{stray:?}");
    assert!(stray[0].contains("`another` asks") && stray[0].contains("not a handler"));
    // An ask that comes after another await.
    let late = changed(
        "kernel/obop-http/src/lib.rs",
        "let brand = mcp::document_brand(&s).await;",
        "let _first = something().await;\n    let brand = mcp::document_brand(&s).await;",
    );
    assert_eq!(late.len(), 1, "{late:?}");
    assert!(late[0].contains("`.await` before it asks"), "{late:?}");
    // An ask that comes after a read of the caller's token, and after a call of the blocking pool
    // with no `.await` before it: each is found by its own word, so a rule that listed fewer words
    // would let one of them through.
    let token = changed(
        "kernel/obop-http/src/lib.rs",
        "let brand = mcp::document_brand(&s).await;",
        "let _t = s.auth_api(&h);\n    let brand = mcp::document_brand(&s).await;",
    );
    assert_eq!(token.len(), 1, "{token:?}");
    assert!(token[0].contains("`auth_api(` before it asks"), "{token:?}");
    let pooled = changed(
        "kernel/obop-http/src/lib.rs",
        "let brand = mcp::document_brand(&s).await;",
        "let _b = blocking(|| ());\n    let brand = mcp::document_brand(&s).await;",
    );
    assert_eq!(pooled.len(), 1, "{pooled:?}");
    assert!(
        pooled[0].contains("`blocking(` before it asks"),
        "{pooled:?}"
    );
}

/// What each of the three rows with limits that slice S3 adds lets its file do, literally: how many
/// `brand()` calls, how many reads of the reader, which paths through the module `brand` and which
/// types as types only. `the_limits_of_the_rows_with_limits_are_the_ones_decided` holds the rows of
/// the slices before this one; the rows of S3 are held here, in its own block, so that a row loosened
/// by a number or a name (a second call, a read, a path) is a change of this test, and so a decision
/// that shows in the diff.
#[test]
fn the_limits_of_the_rows_of_slice_s3_are_the_ones_decided() {
    type Want = (
        &'static str,
        usize,
        usize,
        &'static [&'static str],
        &'static [&'static str],
    );
    let decided: [Want; 3] = [
        ("kernel/obop-http/src/mcp.rs", 2, 0, &["current"], &[]),
        ("kernel/obop-mcp/src/lib.rs", 2, 1, &[], &[]),
        ("kernel/obop-connectors/src/runtime.rs", 2, 1, &[], &[]),
    ];
    for (file, calls, reads, paths, types) in decided {
        let Some((
            _,
            _,
            Limit::Reads {
                calls: c,
                reads: r,
                paths: p,
                types: t,
            },
        )) = ALLOWED.iter().find(|(f, _, _)| *f == file)
        else {
            panic!("{file} has no row with limits");
        };
        assert_eq!((*c, *r, *p, *t), (calls, reads, paths, types), "{file}");
    }
}
