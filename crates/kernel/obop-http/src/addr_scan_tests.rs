//! The scan that makes a second reader of the client's address a deliberate act (D92,
//! docs/ISSUES.md O6): the one rule is `addr::client_address`, which the signer door, the OAuth
//! pages and endpoints, passkeys, Microsoft sign-in and the public site all go through. A second
//! reader of the forwarded header or of the peer could choose its own bucket or record another
//! address, and the tests of its door would not notice.
//!
//! **What it catches.** In the non-test Rust code of every crate under `crates/` (walked
//! recursively; what `#[cfg(test)]` is on is taken out, and only that: an item by its end, a
//! field, variant, arm or statement by its `,` or `;`; `cfg(any(test, ..))` stays, because it is
//! compiled into production builds too; the files of test-only modules and `tests/` and
//! `benches/` directories are left out; comments are blanked; see `src_scan.rs`):
//! - a string literal that is the name of a header that carries a client address, from a closed
//!   list ([`HEADER_NAMES`]: `x-forwarded-for`, `forwarded`, `x-real-ip`, `cf-connecting-ip`,
//!   `true-client-ip`, `x-client-ip` and the platform headers listed there, in any case), or that
//!   starts with `x-forwarded` or `x-original-forwarded` without being `x-forwarded-proto` or
//!   `x-forwarded-host` (so `starts_with("x-forwarded")` and a `concat!` whose first piece
//!   starts that way are found, and so is a name with an escape after that prefix,
//!   `"x-forwarded-\x66or"`: a literal is read as written, escapes are not decoded);
//! - the identifiers `ConnectInfo`, `peer_addr`, `remote_addr` and the constants of those headers
//!   (`X_FORWARDED_FOR`, ..., and `FORWARDED` as `header::FORWARDED`, in a `use` of the `header`
//!   module, or bare where the file glob-imports `header::*`).
//!
//! The places that may are named in [`ALLOWED`] by the whole path, the function **and the name or
//! identifier read, with how many times**: a second read in an exempt function, of another header
//! or of the same one again, is a problem, and a row that matches less than it says is a problem
//! too. **A row in ALLOWED is the decision that a new place may read the client's address on its
//! own.** It is reviewed against D92 (the address in the evidence and in every limiter is
//! `addr::client_address` and nothing else), not fixed to make this test pass.
//!
//! **What it does not see.** It is a textual scan, not a parser: it makes a second reader
//! visible, it does not stop one who works around it (docs/ISSUES.md O6 and D92 list the same).
//! - Names: a header name built at run time (`format!("x-{}", ..)`) or with `concat!` from pieces
//!   that do not start with `x-forwarded` (`concat!("x-real", "-ip")`); a platform header that is
//!   not on the list; a name inside longer text (`"x-real-ip,cf-connecting-ip"`); a name spelled
//!   with an escape (`"x\x2dreal\x2dip"`, `"x-real-\u{69}p"`) or split by a `\`-newline
//!   continuation: literals are not unescaped, so only a name whose escape comes after the
//!   `x-forwarded` prefix is found; a header read for the code by a dependency (an extractor
//!   crate leaves no name and no identifier here); `FORWARDED` through a glob other than
//!   `header::*` (`header::{self, *}`, an alias of the module).
//! - The stripping of test code: a production module in a directory named `tests` or `benches`
//!   is left out as test code, and so is a module declared `#[cfg(test)] mod x;` beside a
//!   `#[cfg(not(test))] mod x;`; `#[cfg(test)]` on a generic parameter with a default
//!   (`<#[cfg(test)] const N: usize = 0>`) is taken for an item and blanks the item after it, and
//!   the table of constructs does not count it.
//! - What is not a `.rs` file under `crates/`: generated code, `include!` of another file, a
//!   build script's output, a proc macro; Rust outside `crates/`; an address taken from a socket
//!   another way (a `TcpStream` the code accepted itself); one read through a constant in another
//!   crate.
//! - The burden: a new `#[cfg(test)]` field, arm or statement in a file that is not in the table
//!   of constructs, a block of more than 12 lines under one, and the rename of a function that
//!   the injection test is anchored on (seven of them) each fail a test that someone then has to
//!   read and change. That is the price of failing closed.
//!
//! `x-forwarded-proto` and `x-forwarded-host` are allowed (they decide the scheme and host of a
//! page, not the client).

use crate::src_scan::{enclosing_fn, line_of, non_test, read_tree, tree, words};
use std::collections::BTreeMap;
use std::path::Path;

/// Where the address may be read: the whole path from `crates/`, the function (`<use>` is a `use`
/// declaration), the header name (lower case) or identifier, and how many times.
const ALLOWED: [(&str, &str, &str, usize); 11] = [
    // The rule itself.
    (
        "kernel/obop-http/src/addr.rs",
        "client_address",
        "ConnectInfo",
        1,
    ),
    (
        "kernel/obop-http/src/addr.rs",
        "forwarded_entry",
        "x-forwarded-for",
        1,
    ),
    ("kernel/obop-http/src/addr.rs", "<use>", "ConnectInfo", 1),
    // The log entry of the signer door: the lines as they came and the peer, beside the address
    // the rule gave. The address itself is `client_ip`, three lines above the peer.
    (
        "kernel/obop-http/src/sign.rs",
        "forwarded_lines",
        "x-forwarded-for",
        1,
    ),
    ("kernel/obop-http/src/sign.rs", "client", "ConnectInfo", 1),
    ("kernel/obop-http/src/sign.rs", "<use>", "ConnectInfo", 1),
    // The access rule of /metrics: a request that crossed a proxy at all, or a peer that is not
    // loopback, is not let in.
    (
        "kernel/obop-http/src/ops.rs",
        "has_proxy_header",
        "x-forwarded-for",
        1,
    ),
    (
        "kernel/obop-http/src/ops.rs",
        "has_proxy_header",
        "x-real-ip",
        1,
    ),
    (
        "kernel/obop-http/src/ops.rs",
        "has_proxy_header",
        "forwarded",
        1,
    ),
    ("kernel/obop-http/src/ops.rs", "authorize", "ConnectInfo", 1),
    ("kernel/obop-http/src/ops.rs", "<use>", "ConnectInfo", 1),
];

/// The headers that carry the address of a client behind some proxy or platform. A closed list.
const HEADER_NAMES: [&str; 20] = [
    "x-forwarded-for",
    "forwarded",
    "x-real-ip",
    "cf-connecting-ip",
    "cf-connecting-ipv6",
    "true-client-ip",
    "x-client-ip",
    "x-cluster-client-ip",
    "fastly-client-ip",
    "fly-client-ip",
    "do-connecting-ip",
    "x-original-forwarded-for",
    "client-ip",
    "x-azure-clientip",
    "x-azure-socketip",
    "x-envoy-external-address",
    "x-appengine-user-ip",
    "x-proxyuser-ip",
    "x-forwarded-client-cert",
    "x-original-for",
];

/// The names that start like a forwarding header and are not one that carries an address.
const FORWARD_PREFIXES: [&str; 2] = ["x-forwarded", "x-original-forwarded"];
const FORWARD_OK: [&str; 2] = ["x-forwarded-proto", "x-forwarded-host"];

/// `FORWARDED` is also the name of a local constant (the headers a handler passes on); it counts
/// as the header only as `header::FORWARDED`, in a `use` of the `header` module, or bare where
/// the file glob-imports `header::*`.
const IDENTIFIERS: [&str; 10] = [
    "ConnectInfo",
    "peer_addr",
    "remote_addr",
    "X_FORWARDED_FOR",
    "X_REAL_IP",
    "CF_CONNECTING_IP",
    "TRUE_CLIENT_IP",
    "X_CLIENT_IP",
    "X_CLUSTER_CLIENT_IP",
    "FASTLY_CLIENT_IP",
];

/// What the scan found: the readers that are not allowed and how many code lines of each file
/// were read after the test items were taken out.
pub(crate) struct Report {
    pub problems: Vec<String>,
    pub scanned_lines: BTreeMap<String, usize>,
}

fn is_header(lower: &str) -> bool {
    HEADER_NAMES.contains(&lower)
        || (FORWARD_PREFIXES.iter().any(|p| lower.starts_with(p)) && !FORWARD_OK.contains(&lower))
}

/// Scan `files` (paths from `crates/`, with `/`) and report the readers of the client address
/// that `allowed` does not name, name often enough, or names but does not find.
pub(crate) fn scan(
    files: &BTreeMap<String, String>,
    allowed: &[(&str, &str, &str, usize)],
) -> Report {
    let lexed = non_test(files);

    let mut problems = Vec::new();
    let mut scanned_lines = BTreeMap::new();
    // (path, function, name) -> how many reads.
    let mut seen: BTreeMap<(String, String, String), usize> = BTreeMap::new();
    for (path, l) in &lexed {
        let text = String::from_utf8_lossy(&l.code).into_owned();
        scanned_lines.insert(
            (*path).to_string(),
            text.lines().filter(|x| !x.trim().is_empty()).count(),
        );
        // (offset, the name for the table, what to show)
        let mut hits: Vec<(usize, String, String)> = Vec::new();
        for id in IDENTIFIERS {
            hits.extend(
                words(&l.code, id)
                    .into_iter()
                    .map(|at| (at, id.to_string(), id.to_string())),
            );
        }
        let glob = text.contains("header::*");
        for at in words(&l.code, "FORWARDED") {
            let stmt = &text[text[..at].rfind([';', '}']).map_or(0, |p| p + 1)..at];
            let stmt = stmt.trim_start();
            let in_use = stmt.starts_with("use ") || stmt.starts_with("pub use ");
            if glob || text[..at].trim_end().ends_with("::") || (in_use && stmt.contains("header"))
            {
                hits.push((at, "FORWARDED".to_string(), "FORWARDED".to_string()));
            }
        }
        for (at, _, s) in &l.strings {
            let lower = s.trim().to_ascii_lowercase();
            if is_header(&lower) {
                hits.push((*at, lower, format!("{s:?}")));
            }
        }
        hits.sort();
        for (at, name, shown) in hits {
            let func = enclosing_fn(&l.code, at).unwrap_or_else(|| {
                let start = text[..at].rfind([';', '}']).map_or(0, |p| p + 1);
                if text[start..at].trim_start().starts_with("use ")
                    || text[start..at].trim_start().starts_with("pub use ")
                {
                    "<use>".to_string()
                } else {
                    "<item>".to_string()
                }
            });
            let known = allowed
                .iter()
                .any(|(p, f, n, _)| p == path && *f == func && *n == name);
            if !known {
                problems.push(format!(
                    "{path}:{} reads the client address itself ({shown}) in `{func}`; go through \
                     addr::client_address. A row in ALLOWED is the decision that a new place may \
                     do this, to be reviewed against D92",
                    line_of(&l.code, at)
                ));
            }
            *seen.entry((path.clone(), func, name)).or_default() += 1;
        }
    }
    for (path, func, name, times) in allowed {
        let n = seen
            .get(&(
                (*path).to_string(),
                (*func).to_string(),
                (*name).to_string(),
            ))
            .copied()
            .unwrap_or(0);
        if n > *times {
            problems.push(format!(
                "{path}: `{func}` reads {name} {n} times and ALLOWED allows {times}"
            ));
        } else if n < *times {
            problems.push(format!(
                "ALLOWED says `{func}` in {path} reads {name} {times} times and it reads it {n}: \
                 remove or lower the exemption"
            ));
        }
    }
    Report {
        problems,
        scanned_lines,
    }
}

fn real_files() -> BTreeMap<String, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = BTreeMap::new();
    read_tree(&root, &root, &mut files);
    assert!(
        files.len() > 300,
        "the scan found only {} files under crates/",
        files.len()
    );
    files
}

#[test]
fn no_place_reads_the_forwarded_header_or_the_peer_but_the_rule_and_the_log_of_the_door() {
    let report = scan(&real_files(), &ALLOWED);
    assert!(report.problems.is_empty(), "{}", report.problems.join("\n"));
}

#[test]
fn a_reader_put_into_the_real_files_is_found_whatever_comes_before_it() {
    // The failures of the two first versions of this scan, driven on the real files: it cut a
    // file at its first `#[cfg(test)]` (40 lines of a 1,100-line router were read), and a
    // `#[cfg(test)]` on a struct field blanked 194 lines of `engine/unit.rs` after it. A reader is
    // put in at the start of a function or an impl of each file, deep below the first test
    // attribute, and must be found. `client` of sign.rs is exempt for the peer; a header read in
    // it is not.
    let files = real_files();
    for (file, anchor) in [
        (
            "kernel/obop-commands/src/engine/unit.rs",
            "impl<'a> Unit<'a> {",
        ),
        (
            "kernel/obop-http/src/lib.rs",
            "fn body_limit() -> axum::extract::DefaultBodyLimit {",
        ),
        (
            "kernel/obop-http/src/oauth.rs",
            "pub(crate) fn client_ip(s: &HttpState, parts: &Parts) -> String {",
        ),
        (
            "kernel/obop-http/src/sign.rs",
            "fn client(s: &HttpState, parts: &Parts) -> Client {",
        ),
        (
            "kernel/obop-commands/src/sign/door.rs",
            "pub fn pdf(&self, rid: &str, tok: &str, client: &Client) -> Result<Served, DoorError> {",
        ),
        (
            "kernel/obop-storage/src/pg.rs",
            "pub(crate) fn lock_batch(c: &mut Client, keys: &[i32]) -> PR<()> {",
        ),
        ("obop/src/main.rs", "fn take_opt<'a>("),
    ] {
        let src = &files[file];
        assert_eq!(
            src.matches(anchor).count(),
            1,
            "{file}: the anchor {anchor:?} moved; point this test at another function"
        );
        let mut files = files.clone();
        let injected = src.replacen(
            anchor,
            &format!("{anchor}\n    let _probe = headers.get(\"x-real-ip\");\n"),
            1,
        );
        files.insert(file.to_string(), injected);
        let p = scan(&files, &ALLOWED).problems;
        assert_eq!(p.len(), 1, "{file}: {p:?}");
        assert!(p[0].contains(file) && p[0].contains("x-real-ip"), "{p:?}");
    }
}

#[test]
fn every_file_balances_and_the_things_taken_out_by_cfg_test_are_the_few_known() {
    use crate::src_scan::{Taken, lex, strip_test_items};
    // A lexer that loses its place (a char literal taken for a string, say) makes the braces of
    // the rest of a file not balance, and a test module is then taken out too far or too short.
    let files = real_files();
    for (path, src) in &files {
        let l = lex(src);
        for (open, close) in [(b'{', b'}'), (b'(', b')'), (b'[', b']')] {
            let (o, c) = (
                l.code.iter().filter(|b| **b == open).count(),
                l.code.iter().filter(|b| **b == close).count(),
            );
            assert_eq!(
                o, c,
                "{path}: {} {} and {} {}",
                o, open as char, c, close as char
            );
        }
    }
    // A `#[cfg(test)]` on something that is not an item takes out that thing only, but it is
    // the place where the stripping is least sure, so each one is listed: a new one is a decision
    // to read (the test prints where it is and how many lines were taken out).
    const KNOWN: [(&str, usize); 2] = [
        ("kernel/obop-commands/src/engine/unit.rs", 4),
        ("kernel/obop-storage/src/pg.rs", 2),
    ];
    let mut found: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (path, src) in &files {
        let mut l = lex(src);
        let _ = strip_test_items(&mut l);
        // Offsets are those of the source: the code keeps its length and its line breaks.
        for s in l.stripped.iter().filter(|s| s.what == Taken::Construct) {
            let lines = src[s.start..s.end].matches('\n').count() + 1;
            assert!(
                lines <= 12,
                "{path}:{}: a #[cfg(test)] on a construct took out {lines} lines; it should take \
                 out one field, arm or statement",
                line_of(src.as_bytes(), s.start)
            );
            found.entry(path.clone()).or_default().push(format!(
                "{path}:{} ({lines} lines)",
                line_of(src.as_bytes(), s.start)
            ));
        }
    }
    let got: Vec<(String, usize)> = found.iter().map(|(p, v)| (p.clone(), v.len())).collect();
    let want: Vec<(String, usize)> = KNOWN.iter().map(|(p, n)| (p.to_string(), *n)).collect();
    assert_eq!(got, want, "{found:#?}");
}

fn problems(files: &[(&str, &str)], allowed: &[(&str, &str, &str, usize)]) -> Vec<String> {
    scan(&tree(files), allowed).problems
}

#[test]
fn a_reader_after_the_first_test_attribute_of_a_file_is_found() {
    // The reviewer's first mutant: lib.rs has `#[cfg(test)] mod error_tests;` near the top.
    let lib = "\
#[cfg(test)]
mod error_tests;

fn route(h: &HeaderMap) {
    let _ = h.get(\"x-forwarded-for\");
}
";
    let p = problems(&[("kernel/obop-http/src/lib.rs", lib)], &[]);
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(
        p[0].contains("lib.rs:5") && p[0].contains("`route`"),
        "{p:?}"
    );
    // The same with the peer.
    let lib = "#[cfg(test)]\nmod error_tests;\nfn f(e: &Extensions) { e.get::<axum::extract::ConnectInfo<SocketAddr>>(); }\n";
    assert_eq!(
        problems(&[("kernel/obop-http/src/lib.rs", lib)], &[]).len(),
        1
    );
}

#[test]
fn a_cfg_test_on_a_field_a_variant_an_arm_or_a_statement_takes_out_that_thing_only() {
    // The shape of `engine/unit.rs`: a test-only field, then an impl with a reader in it.
    let unit = "\
struct Unit {
    a: u8,
    #[cfg(test)]
    examined: std::cell::Cell<usize>,
}
impl Unit {
    fn leak(h: &HeaderMap) { h.get(\"x-real-ip\"); }
}
";
    let p = problems(&[("kernel/x/src/unit.rs", unit)], &[]);
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(p[0].contains("`leak`"), "{p:?}");
    // A variant, an arm with and without a block, a statement and an argument.
    for (what, src) in [
        (
            "a variant",
            "enum E { A, #[cfg(test)] T, B }\nfn f(h: &HeaderMap) { h.get(\"x-real-ip\"); }\n",
        ),
        (
            "an arm with a block",
            "fn g(x: u8, h: &HeaderMap) { match x { #[cfg(test)] 1 => { y(); } 2 => { h.get(\"x-real-ip\"); } _ => {} } }\n",
        ),
        (
            "an arm without a block",
            "fn g(x: u8, h: &HeaderMap) { match x { #[cfg(test)] 1 => y(), 2 => { h.get(\"x-real-ip\"); } _ => {} } }\n",
        ),
        (
            "a statement",
            "fn g(h: &HeaderMap) { #[cfg(test)] let t = 1; h.get(\"x-real-ip\"); }\n",
        ),
        (
            "an expression statement with calls",
            "fn g(h: &HeaderMap) { #[cfg(test)] self.examined.set(self.examined.get() + 1); h.get(\"x-real-ip\"); }\n",
        ),
        (
            "a block statement",
            "fn g(h: &HeaderMap) { #[cfg(test)] { let n = q(1, 2); } h.get(\"x-real-ip\"); }\n",
        ),
        (
            "an if with an else",
            "fn g(h: &HeaderMap) { #[cfg(test)] if a { b(); } else { c(); } h.get(\"x-real-ip\"); }\n",
        ),
        (
            "an argument",
            "fn g(h: &HeaderMap) { f(a, #[cfg(test)] b, c); h.get(\"x-real-ip\"); }\n",
        ),
        (
            "a field named like an item head",
            "struct S { #[cfg(test)] union: u8, b: u8 }\nfn g(h: &HeaderMap) { h.get(\"x-real-ip\"); }\n",
        ),
        (
            "a macro item",
            "#[cfg(test)]\nthread_local! { static X: u8 = 0; }\nfn g(h: &HeaderMap) { h.get(\"x-real-ip\"); }\n",
        ),
    ] {
        let p = problems(&[("kernel/x/src/a.rs", src)], &[]);
        assert_eq!(p.len(), 1, "{what}: {p:?}");
    }
    // And what the attribute is on is gone, reader or not.
    let src = "fn g(x: u8, h: &HeaderMap) { match x { #[cfg(test)] 1 => { h.get(\"x-real-ip\"); } _ => {} } }\n";
    assert!(problems(&[("kernel/x/src/a.rs", src)], &[]).is_empty());
    let src = "struct S { #[cfg(test)] r: Reader<ConnectInfo<A>>, b: u8 }\n";
    assert!(problems(&[("kernel/x/src/a.rs", src)], &[]).is_empty());
}

#[test]
fn a_cfg_that_is_also_true_in_a_production_build_is_code() {
    // Only `test`, `all(test, ..)` and `any(..)` of tests are test-only. The rest is compiled into
    // some production build.
    for (cfg, test_only) in [
        ("test", true),
        ("all(test, feature = \"x\")", true),
        ("all(feature = \"x\", test)", true),
        ("all(unix, any(test, windows))", false),
        ("any(test, feature = \"x\")", false),
        ("any(test, debug_assertions)", false),
        ("any(unix, test)", false),
        ("any(test, test)", true),
        ("not(test)", false),
        ("feature = \"test\"", false),
        ("unix", false),
    ] {
        let src = format!("#[cfg({cfg})]\nfn f(h: &HeaderMap) {{ h.get(\"x-real-ip\"); }}\n");
        let p = problems(&[("kernel/x/src/a.rs", &src)], &[]);
        assert_eq!(p.len(), usize::from(!test_only), "{cfg}: {p:?}");
    }
}

#[test]
fn the_closed_lists_are_the_lists_written_here() {
    // The test below walks the lists itself, so it cannot notice a name leave them; this one is
    // the lists written out again (ISSUES O6 names several of the headers).
    assert_eq!(
        HEADER_NAMES.as_slice(),
        [
            "x-forwarded-for",
            "forwarded",
            "x-real-ip",
            "cf-connecting-ip",
            "cf-connecting-ipv6",
            "true-client-ip",
            "x-client-ip",
            "x-cluster-client-ip",
            "fastly-client-ip",
            "fly-client-ip",
            "do-connecting-ip",
            "x-original-forwarded-for",
            "client-ip",
            "x-azure-clientip",
            "x-azure-socketip",
            "x-envoy-external-address",
            "x-appengine-user-ip",
            "x-proxyuser-ip",
            "x-forwarded-client-cert",
            "x-original-for",
        ]
        .as_slice()
    );
    assert_eq!(
        IDENTIFIERS.as_slice(),
        [
            "ConnectInfo",
            "peer_addr",
            "remote_addr",
            "X_FORWARDED_FOR",
            "X_REAL_IP",
            "CF_CONNECTING_IP",
            "TRUE_CLIENT_IP",
            "X_CLIENT_IP",
            "X_CLUSTER_CLIENT_IP",
            "FASTLY_CLIENT_IP",
        ]
        .as_slice()
    );
}

#[test]
fn the_whole_header_list_the_forwarded_prefix_and_the_peer_are_found() {
    let mut reads: Vec<String> = HEADER_NAMES
        .iter()
        .map(|n| format!("h.get(\"{n}\")"))
        .collect();
    reads.extend(
        [
            "h.get(\"X-Real-IP\")",
            "h.get(\"Forwarded\")",
            "h.get(r#\"x-real-ip\"#)",
            "h.get(HeaderName::from_static(\"x-cluster-client-ip\"))",
            "h.get(header::FORWARDED)",
            "h.get(X_FORWARDED_FOR)",
            "s.peer_addr()",
            "s.remote_addr()",
            "e.get::<ConnectInfo<SocketAddr>>()",
            // The compile-time and prefix spellings that start like a forwarding header.
            "h.get(concat!(\"x-forwarded-\", \"for\"))",
            "h.get(concat!(\"x-forwarded\", \"-for\"))",
            // An escape after the prefix (literals are not unescaped: the prefix is what is found).
            "h.get(\"x-forwarded-\\x66or\")",
            "k.as_str().starts_with(\"x-forwarded\")",
            "h.get(\"x-forwarded-port\")",
            "h.get(\"x-original-forwarded-host\")",
        ]
        .map(String::from),
    );
    for read in &reads {
        let src = format!("fn limiter(h: &HeaderMap) {{ let _ = {read}; }}\n");
        let p = problems(&[("kernel/obop-http/src/oauth.rs", &src)], &[]);
        assert_eq!(p.len(), 1, "{read}: {p:?}");
    }
    // An import under another name is a reader too (the `use` is where the name enters).
    let src = "use axum::extract::ConnectInfo as Peer;\n";
    assert_eq!(problems(&[("kernel/x/src/a.rs", src)], &[]).len(), 1);
    let src = "use axum::http::header::{ACCEPT, FORWARDED};\n";
    assert_eq!(problems(&[("kernel/x/src/a.rs", src)], &[]).len(), 1);
    // A glob import of the header module brings the constant in by its bare name.
    let src = "use axum::http::header::*;\nfn f(h: &HeaderMap) { h.get(FORWARDED); }\n";
    assert_eq!(
        problems(&[("kernel/x/src/a.rs", src)], &[]).len(),
        1,
        "a glob"
    );
    // A local constant of that name (the headers a handler hands on) is not the header.
    let src = "const FORWARDED: [&str; 2] = [\"accept\", \"cookie\"];\nfn f() { for n in FORWARDED { let _ = n; } }\n";
    assert!(problems(&[("kernel/x/src/a.rs", src)], &[]).is_empty());
    // The headers that decide the page's scheme and host are not the client's address.
    let src =
        "fn f(h: &HeaderMap) { h.get(\"x-forwarded-proto\"); h.get(\"x-forwarded-host\"); }\n";
    assert!(problems(&[("kernel/x/src/a.rs", src)], &[]).is_empty());
}

#[test]
fn a_reader_in_a_subdirectory_or_another_crate_is_found() {
    let src = "fn f(h: &HeaderMap) { h.get(\"x-real-ip\"); }\n";
    for path in [
        "kernel/obop-http/src/sub/deeper/mod.rs",
        "kernel/obop-commands/src/sign/link.rs",
        "obop/src/main.rs",
        "modules/crm/src/lib.rs",
    ] {
        let p = problems(&[(path, src)], &[]);
        assert_eq!(p.len(), 1, "{path}: {p:?}");
    }
}

#[test]
fn test_code_is_left_out_and_what_follows_it_is_still_read() {
    let src = "\
fn fine() {}

#[cfg(test)]
mod tests {
    fn braces_in_strings_and_chars() {
        let _ = \"} {\"; let _ = r#\"}}} {{{\"#; let _ = '}'; let _ = '{'; let _ = '\"';
        let _ = h.get(\"x-forwarded-for\");
        let _: Option<&'static str> = None;
    }
}

#[cfg(test)]
fn helper() { let _ = e.get::<ConnectInfo<SocketAddr>>(); }

#[cfg(all(test, feature = \"x\"))]
use axum::extract::ConnectInfo;

#[cfg(test)]
const T: usize = 3;

#[cfg_attr(test, derive(Debug))]
struct Plain;

#[cfg(test)]
#[allow(dead_code)]
pub(crate) mod more { fn f() { let _ = h.get(\"x-real-ip\"); } }

fn late(h: &HeaderMap) { h.get(\"x-real-ip\"); }
";
    let p = problems(&[("kernel/x/src/a.rs", src)], &[]);
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(p[0].contains("`late`"), "{p:?}");
    // A char literal of a quote or a brace does not move the reader out of sight.
    let src =
        "fn q() { let _ = '\"'; let _ = '{'; }\nfn late(h: &HeaderMap) { h.get(\"x-real-ip\"); }\n";
    assert_eq!(problems(&[("kernel/x/src/a.rs", src)], &[]).len(), 1);
    // A cfg that is not a test build is code.
    let src = "#[cfg(not(test))]\nfn f(h: &HeaderMap) { h.get(\"x-real-ip\"); }\n#[cfg(feature = \"test\")]\nfn g(h: &HeaderMap) { h.get(\"x-real-ip\"); }\n";
    assert_eq!(problems(&[("kernel/x/src/a.rs", src)], &[]).len(), 2);
}

#[test]
fn a_module_declared_as_test_only_takes_its_files_and_the_modules_under_them_with_it() {
    let reader = "fn t(h: &HeaderMap) { h.get(\"x-real-ip\"); }\n";
    let files = [
        (
            "kernel/x/src/lib.rs",
            "#[cfg(test)]\nmod widget_tests;\nmod widget;\n",
        ),
        ("kernel/x/src/widget_tests.rs", "mod inner;\n"),
        ("kernel/x/src/widget_tests/inner.rs", reader),
        ("kernel/x/src/widget.rs", reader),
        ("kernel/x/tests/it.rs", reader),
        ("kernel/x/benches/b.rs", reader),
        // A directory module: `sign_tests/mod.rs`.
        ("kernel/y/src/main.rs", "#[cfg(test)]\nmod sign_tests;\n"),
        ("kernel/y/src/sign_tests/mod.rs", reader),
        ("kernel/y/src/sign_tests/door.rs", reader),
        // A file that is test code as a whole.
        (
            "kernel/z/src/helper.rs",
            "#![cfg(test)]\nfn t(h: &HeaderMap) { h.get(\"x-real-ip\"); }\n",
        ),
    ];
    let p = problems(&files, &[]);
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(p[0].contains("kernel/x/src/widget.rs"), "{p:?}");
    // A `;`-terminated test item directly before a production function.
    let src = "#[cfg(test)]\nuse a::b;\nfn late(h: &HeaderMap) { h.get(\"x-real-ip\"); }\n";
    assert_eq!(problems(&[("kernel/x/src/a.rs", src)], &[]).len(), 1);
}

#[test]
fn an_inner_cfg_test_below_the_top_of_a_file_does_not_take_the_file_out() {
    // `#![cfg(test)]` belongs to the module, function or macro it is in; only at the top of a
    // file is it the file's. A reader in the rest of the file must still be found, and so must
    // one in the files of the module directory under it.
    let reader = "fn leak(h: &HeaderMap) { h.get(\"x-real-ip\"); }\n";
    for (what, tail) in [
        ("a module", "mod z { #![cfg(test)] fn t() {} }\n"),
        ("a function", "fn other() { #![cfg(test)] let _ = 1; }\n"),
        ("a macro", "macro_rules! m { () => { #![cfg(test)] }; }\n"),
        (
            "a test-only module",
            "#[cfg(test)]\nmod z { #![cfg(test)] }\n",
        ),
    ] {
        let src = format!("{reader}{tail}");
        let files = [
            ("kernel/x/src/oauth.rs", src.as_str()),
            ("kernel/x/src/oauth/inner.rs", reader),
        ];
        let p = problems(&files, &[]);
        assert_eq!(p.len(), 2, "{what}: {p:?}");
    }
    // At the top it is the file's, after the doc comment and after other inner attributes, and
    // for the cfgs that are test-only; not for the ones that are also a production build.
    for (cfg, whole) in [
        ("test", true),
        ("all(test, feature = \"x\")", true),
        ("not(test)", false),
        ("any(test, unix)", false),
        ("feature = \"test\"", false),
    ] {
        let src = format!("//! docs\n#![allow(dead_code)]\n#![cfg({cfg})]\n{reader}");
        let files = [
            ("kernel/x/src/oauth.rs", src.as_str()),
            ("kernel/x/src/oauth/inner.rs", reader),
        ];
        let p = problems(&files, &[]);
        assert_eq!(p.len(), if whole { 0 } else { 2 }, "{cfg}: {p:?}");
    }
}

#[test]
fn a_raw_c_string_is_lexed_like_a_raw_string() {
    // `cr"\"` has no escape and ends at its second quote. Read as an ordinary string it ran on,
    // took the code after it for a string and the next string for code, and the reader below
    // went out of sight.
    for lit in [r#"cr"\""#, r##"cr#"\"#"##, r#"br"\""#, r#"r"\""#] {
        let src = format!(
            "fn a() {{ let _ = {lit}; }}\nfn late(h: &HeaderMap) {{ h.get(\"x-real-ip\"); }}\n"
        );
        let p = problems(&[("kernel/x/src/a.rs", &src)], &[]);
        assert_eq!(p.len(), 1, "{lit}: {p:?}");
    }
}

#[test]
fn comments_and_prose_are_not_readers() {
    let src = "\
//! The address comes from `X-Forwarded-For` and `ConnectInfo`.
/// Reads \"x-forwarded-for\" in a doc comment.
fn f() {
    // h.get(\"x-real-ip\") and peer_addr()
    /* nested /* h.get(\"forwarded\") */ ConnectInfo */
    let _ = \"take the client address from X-Forwarded-For\";
    let _ = \"the forwarded text of a refused post\";
    let forwarded = 1;
}
";
    assert!(problems(&[("kernel/x/src/a.rs", src)], &[]).is_empty());
}

#[test]
fn an_exemption_is_one_function_one_name_and_a_number_of_reads() {
    let src = "\
use axum::extract::ConnectInfo;

fn rule(e: &Extensions, h: &HeaderMap) {
    e.get::<ConnectInfo<SocketAddr>>();
    h.get(\"x-forwarded-for\");
}

fn other(e: &Extensions) { e.get::<ConnectInfo<SocketAddr>>(); }

fn nested() {
    fn inner(e: &Extensions) { e.get::<ConnectInfo<SocketAddr>>(); }
}
";
    let allowed = [
        ("x/src/a.rs", "rule", "ConnectInfo", 1),
        ("x/src/a.rs", "rule", "x-forwarded-for", 1),
        ("x/src/a.rs", "<use>", "ConnectInfo", 1),
    ];
    let p = problems(&[("x/src/a.rs", src)], &allowed);
    assert_eq!(p.len(), 2, "{p:?}");
    assert!(
        p.iter()
            .any(|x| x.contains("a.rs:8") && x.contains("`other`")),
        "{p:?}"
    );
    assert!(
        p.iter()
            .any(|x| x.contains("a.rs:11") && x.contains("`inner`")),
        "{p:?}"
    );
    // A second header read in an exempt function (the shape of `client()` in sign.rs, the
    // function that records the address of the evidence) is not covered by the exemption for the
    // peer, nor is a second read of the one it has.
    let src = "\
fn client(e: &Extensions, h: &HeaderMap) -> String {
    let peer = e.get::<ConnectInfo<SocketAddr>>();
    h.get(\"x-real-ip\").or(peer)
}
";
    let p = problems(
        &[("x/src/a.rs", src)],
        &[("x/src/a.rs", "client", "ConnectInfo", 1)],
    );
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(p[0].contains("x-real-ip"), "{p:?}");
    let src =
        "fn client(e: &Extensions) { e.get::<ConnectInfo<A>>(); e.get::<ConnectInfo<B>>(); }\n";
    let p = problems(
        &[("x/src/a.rs", src)],
        &[("x/src/a.rs", "client", "ConnectInfo", 1)],
    );
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(p[0].contains("2 times"), "{p:?}");
    // The whole path counts, not its end: a look-alike directory is not exempt.
    let src = "fn rule(e: &Extensions) { e.get::<ConnectInfo<SocketAddr>>(); }\n";
    let allowed = [("x/src/a.rs", "rule", "ConnectInfo", 1)];
    let p = problems(&[("evil-x/src/a.rs", src), ("x/src/a.rs", src)], &allowed);
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(p[0].contains("evil-x/src/a.rs"), "{p:?}");
    // Another file with a function of the same name is not exempt.
    let p = problems(&[("x/src/a.rs", src), ("x/src/b.rs", src)], &allowed);
    assert!(p.iter().any(|x| x.contains("x/src/b.rs")), "{p:?}");
    // The reader went away or reads less: its exemption must go or shrink too.
    let p = problems(
        &[("x/src/a.rs", "fn rule() {}\n")],
        &[("x/src/a.rs", "rule", "ConnectInfo", 1)],
    );
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(p[0].contains("remove or lower"), "{p:?}");
}

#[test]
fn the_scan_reports_how_much_of_a_file_it_read() {
    let mut src = String::from("#[cfg(test)]\nmod error_tests;\n");
    for i in 0..50 {
        src.push_str(&format!("fn f{i}() {{}}\n"));
    }
    let r = scan(&tree(&[("kernel/x/src/lib.rs", &src)]), &[]);
    assert_eq!(r.scanned_lines["kernel/x/src/lib.rs"], 50);
}
