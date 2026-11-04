//! The scan that keeps every other place of the workspace from reading the client's address on
//! its own (D92, docs/ISSUES.md O6): the one rule is `addr::client_address`, which the signer
//! door, the OAuth pages and endpoints, passkeys, Microsoft sign-in and the public site all go
//! through. A second reader of the forwarded header or of the peer could choose its own bucket or
//! record another address, and the tests of its door would not notice.
//!
//! **What it catches.** In the non-test Rust code of every crate under `crates/` (walked
//! recursively; `#[cfg(test)]` items and modules, the files such modules are declared in, and
//! `tests/` and `benches/` directories are left out, and comments are blanked): a string literal
//! that is exactly the name of a client-address header (`x-forwarded-for`, `forwarded`,
//! `x-real-ip`, `cf-connecting-ip`, `true-client-ip`, `x-client-ip`, `x-cluster-client-ip`,
//! `fastly-client-ip`, in any case), and the identifiers `ConnectInfo`, `peer_addr`,
//! `remote_addr` and the constants of those headers (`FORWARDED`, `X_FORWARDED_FOR`, and so on).
//! The places that may read them are named by function in [`ALLOWED`], not by file; an exemption
//! that no longer matches anything is itself a failure.
//!
//! **What it does not catch.** A header name that is built at run time (`format!("x-{}", ...)`),
//! one that is read through a constant in another crate, an address taken from a socket by
//! another way (a `TcpStream` the code accepted itself), `x-forwarded-proto` and `x-forwarded-host`
//! (they decide the scheme and host of the page, not the client), and Rust outside `crates/`.
//! It is a textual scan, not a parser: it is there to make a second reader a deliberate act, not
//! to stop one who works around it.

use crate::src_scan::{enclosing_fn, line_of, non_test, read_tree, tree, words};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Where the address may be read, by file (the end of its path from `crates/`) and function.
/// `<use>` is a `use` declaration at the top of a file.
const ALLOWED: [(&str, &str); 9] = [
    // The rule itself.
    ("obop-http/src/addr.rs", "client_address"),
    ("obop-http/src/addr.rs", "forwarded_entry"),
    ("obop-http/src/addr.rs", "<use>"),
    // The log entry of the signer door: the lines as they came and the peer, beside the address
    // the rule gave.
    ("obop-http/src/sign.rs", "forwarded_lines"),
    ("obop-http/src/sign.rs", "client"),
    ("obop-http/src/sign.rs", "<use>"),
    // The access rule of /metrics: a request that crossed a proxy at all, or a peer that is not
    // loopback, is not let in.
    ("obop-http/src/ops.rs", "has_proxy_header"),
    ("obop-http/src/ops.rs", "authorize"),
    ("obop-http/src/ops.rs", "<use>"),
];

const HEADER_NAMES: [&str; 8] = [
    "x-forwarded-for",
    "forwarded",
    "x-real-ip",
    "cf-connecting-ip",
    "true-client-ip",
    "x-client-ip",
    "x-cluster-client-ip",
    "fastly-client-ip",
];

/// `FORWARDED` is also the name of a local constant (the headers a handler passes on); it counts
/// as the header only as `header::FORWARDED` or in a `use` of the `header` module.
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

/// What the scan found: the readers that are not allowed, how many code lines of each file were
/// read after the test items were taken out (a file that is read in part only shows here), and
/// the exemptions that matched nothing.
pub(crate) struct Report {
    pub problems: Vec<String>,
    pub scanned_lines: BTreeMap<String, usize>,
}

/// Scan `files` (paths from `crates/`, with `/`) and report the readers of the client address
/// that `allowed` does not name.
pub(crate) fn scan(files: &BTreeMap<String, String>, allowed: &[(&str, &str)]) -> Report {
    let lexed = non_test(files);

    let mut problems = Vec::new();
    let mut scanned_lines = BTreeMap::new();
    let mut used: BTreeSet<(String, String)> = BTreeSet::new();
    for (path, l) in &lexed {
        let text = String::from_utf8_lossy(&l.code).into_owned();
        scanned_lines.insert(
            (*path).to_string(),
            text.lines().filter(|x| !x.trim().is_empty()).count(),
        );
        let mut hits: Vec<(usize, String)> = Vec::new();
        for id in IDENTIFIERS {
            hits.extend(
                words(&l.code, id)
                    .into_iter()
                    .map(|at| (at, id.to_string())),
            );
        }
        for at in words(&l.code, "FORWARDED") {
            let stmt = &text[text[..at].rfind([';', '}']).map_or(0, |p| p + 1)..at];
            let stmt = stmt.trim_start();
            let in_use = stmt.starts_with("use ") || stmt.starts_with("pub use ");
            if text[..at].trim_end().ends_with("::") || (in_use && stmt.contains("header")) {
                hits.push((at, "FORWARDED".to_string()));
            }
        }
        for (at, _, s) in &l.strings {
            let lower = s.trim().to_ascii_lowercase();
            if HEADER_NAMES.contains(&lower.as_str()) {
                hits.push((*at, format!("{s:?}")));
            }
        }
        hits.sort();
        for (at, what) in hits {
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
            let ok = allowed
                .iter()
                .find(|(file, f)| path.ends_with(file) && *f == func);
            match ok {
                Some((file, f)) => {
                    used.insert(((*file).to_string(), (*f).to_string()));
                }
                None => problems.push(format!(
                    "{path}:{} reads the client address itself ({what}) in `{func}`; go through \
                     addr::client_address, or name the function in ALLOWED",
                    line_of(&l.code, at)
                )),
            }
        }
    }
    for (file, f) in allowed {
        if !used.contains(&((*file).to_string(), (*f).to_string())) {
            problems.push(format!(
                "ALLOWED names `{f}` in {file}, which no longer reads the address: remove the exemption"
            ));
        }
    }
    Report {
        problems,
        scanned_lines,
    }
}

#[test]
fn no_place_reads_the_forwarded_header_or_the_peer_but_the_rule_and_the_log_of_the_door() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = BTreeMap::new();
    read_tree(&root, &root, &mut files);
    assert!(
        files.len() > 300,
        "the scan found only {} files under crates/",
        files.len()
    );
    let report = scan(&files, &ALLOWED);
    assert!(report.problems.is_empty(), "{}", report.problems.join("\n"));
    // The failure the first version of this scan had: it cut a file at its first `#[cfg(test)]`
    // and read 40 lines of a 1,100-line router. The routers must be read to their end.
    for (file, at_least) in [
        ("kernel/obop-http/src/lib.rs", 700),
        ("kernel/obop-http/src/oauth.rs", 700),
        ("kernel/obop-http/src/sign.rs", 200),
        ("kernel/obop-commands/src/sign/door.rs", 400),
        ("obop/src/main.rs", 1000),
    ] {
        let n = report.scanned_lines.get(file).copied().unwrap_or(0);
        assert!(n >= at_least, "{file}: only {n} code lines were scanned");
    }
}

fn problems(files: &[(&str, &str)], allowed: &[(&str, &str)]) -> Vec<String> {
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
fn the_whole_header_family_and_the_peer_are_found_in_any_spelling() {
    for read in [
        "h.get(\"x-real-ip\")",
        "h.get(\"X-Real-IP\")",
        "h.get(\"cf-connecting-ip\")",
        "h.get(\"true-client-ip\")",
        "h.get(\"x-client-ip\")",
        "h.get(\"forwarded\")",
        "h.get(\"Forwarded\")",
        "h.get(\"x-forwarded-for\")",
        "h.get(\"fastly-client-ip\")",
        "h.get(r#\"x-real-ip\"#)",
        "h.get(HeaderName::from_static(\"x-cluster-client-ip\"))",
        "h.get(header::FORWARDED)",
        "h.get(X_FORWARDED_FOR)",
        "s.peer_addr()",
        "s.remote_addr()",
        "e.get::<ConnectInfo<SocketAddr>>()",
    ] {
        let src = format!("fn limiter(h: &HeaderMap) {{ let _ = {read}; }}\n");
        let p = problems(&[("kernel/obop-http/src/oauth.rs", &src)], &[]);
        assert_eq!(p.len(), 1, "{read}: {p:?}");
    }
    // An import under another name is a reader too (the `use` is where the name enters).
    let src = "use axum::extract::ConnectInfo as Peer;\n";
    assert_eq!(problems(&[("kernel/x/src/a.rs", src)], &[]).len(), 1);
    let src = "use axum::http::header::{ACCEPT, FORWARDED};
";
    assert_eq!(problems(&[("kernel/x/src/a.rs", src)], &[]).len(), 1);
    // A local constant of that name (the headers a handler hands on) is not the header.
    let src = "const FORWARDED: [&str; 2] = [\"accept\", \"cookie\"];
fn f() { for n in FORWARDED { let _ = n; } }
";
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
        let _ = \"} {\"; let _ = r#\"}}} {{{\"#; let _ = '}'; let _ = '{';
        let _ = h.get(\"x-forwarded-for\");
        let _: Option<&'static str> = None;
    }
}

#[cfg(test)]
fn helper() { let _ = e.get::<ConnectInfo<SocketAddr>>(); }

#[cfg(all(test, feature = \"x\"))]
use axum::extract::ConnectInfo;

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
    ];
    let p = problems(&files, &[]);
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(p[0].contains("kernel/x/src/widget.rs"), "{p:?}");
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
fn an_exemption_is_one_function_and_a_stale_one_is_an_error() {
    let src = "\
use axum::extract::ConnectInfo;

fn rule(e: &Extensions) { e.get::<ConnectInfo<SocketAddr>>(); }

fn other(e: &Extensions) { e.get::<ConnectInfo<SocketAddr>>(); }
";
    let allowed = [("x/src/a.rs", "rule"), ("x/src/a.rs", "<use>")];
    let p = problems(&[("x/src/a.rs", src)], &allowed);
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(p[0].contains("a.rs:5") && p[0].contains("`other`"), "{p:?}");
    // Another file with a function of the same name is not exempt.
    let p = problems(
        &[
            ("x/src/a.rs", src),
            (
                "x/src/b.rs",
                "fn rule(e: &Extensions) { e.get::<ConnectInfo<SocketAddr>>(); }\n",
            ),
        ],
        &allowed,
    );
    assert!(p.iter().any(|x| x.contains("b.rs")), "{p:?}");
    // The reader went away: its exemption must go too.
    let p = problems(
        &[("x/src/a.rs", "fn rule() {}\n")],
        &[("x/src/a.rs", "rule")],
    );
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(p[0].contains("no longer reads"), "{p:?}");
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
