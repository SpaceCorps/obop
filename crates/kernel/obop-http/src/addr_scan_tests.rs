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

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// A source with comments, string literals and char literals blanked to spaces (line breaks
/// kept, so that offsets and line numbers stay), and the content of each string literal.
struct Lexed {
    code: Vec<u8>,
    strings: Vec<(usize, String)>,
}

fn blank(code: &mut [u8], from: usize, to: usize) {
    for b in &mut code[from..to] {
        if *b != b'\n' {
            *b = b' ';
        }
    }
}

fn lex(src: &str) -> Lexed {
    let b = src.as_bytes();
    let mut code = b.to_vec();
    let mut strings = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                let end = b[i..]
                    .iter()
                    .position(|c| *c == b'\n')
                    .map_or(b.len(), |p| i + p);
                blank(&mut code, i, end);
                i = end;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let mut depth = 1;
                let mut j = i + 2;
                while j < b.len() && depth > 0 {
                    if b[j] == b'/' && b.get(j + 1) == Some(&b'*') {
                        depth += 1;
                        j += 2;
                    } else if b[j] == b'*' && b.get(j + 1) == Some(&b'/') {
                        depth -= 1;
                        j += 2;
                    } else {
                        j += 1;
                    }
                }
                blank(&mut code, i, j);
                i = j;
            }
            b'r' if (i == 0
                || !is_ident(b[i - 1])
                || (b[i - 1] == b'b' && (i == 1 || !is_ident(b[i - 2]))))
                && raw_string(b, i).is_some() =>
            {
                let (open, close, hashes) = raw_string(b, i).unwrap();
                strings.push((i, src[open..close].to_string()));
                blank(&mut code, i, close + 1 + hashes);
                i = close + 1 + hashes;
            }
            b'"' => {
                let mut j = i + 1;
                while j < b.len() && b[j] != b'"' {
                    j += if b[j] == b'\\' { 2 } else { 1 };
                }
                let end = j.min(b.len());
                strings.push((i, src[i + 1..end].to_string()));
                blank(&mut code, i, (end + 1).min(b.len()));
                i = end + 1;
            }
            b'\'' => {
                // A char literal, or a lifetime (which stays as it is).
                let next = src[i + 1..].chars().next();
                let literal_end = match next {
                    Some('\\') => {
                        let mut j = i + 2;
                        j += src[j..].chars().next().map_or(0, char::len_utf8);
                        while j < b.len() && b[j] != b'\'' {
                            j += 1;
                        }
                        Some(j)
                    }
                    Some(c) if src[i + 1 + c.len_utf8()..].starts_with('\'') => {
                        Some(i + 1 + c.len_utf8())
                    }
                    _ => None,
                };
                match literal_end {
                    Some(end) => {
                        blank(&mut code, i, (end + 1).min(b.len()));
                        i = end + 1;
                    }
                    None => i += 1,
                }
            }
            _ => i += 1,
        }
    }
    Lexed { code, strings }
}

/// A raw string starting at the `r` at `i`: the offsets of its content and of the closing quote,
/// and the number of `#`.
fn raw_string(b: &[u8], i: usize) -> Option<(usize, usize, usize)> {
    let mut j = i + 1;
    while b.get(j) == Some(&b'#') {
        j += 1;
    }
    if b.get(j) != Some(&b'"') {
        return None;
    }
    let hashes = j - i - 1;
    let open = j + 1;
    let mut close = open;
    while close < b.len() {
        if b[close] == b'"'
            && b[close + 1..].iter().take(hashes).all(|c| *c == b'#')
            && b.len() >= close + 1 + hashes
        {
            return Some((open, close, hashes));
        }
        close += 1;
    }
    None
}

/// The offset after the `}` that closes the `{` at `open`.
fn after_brace(code: &[u8], open: usize) -> usize {
    let mut depth = 0;
    for (k, c) in code.iter().enumerate().skip(open) {
        match c {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return k + 1;
                }
            }
            _ => {}
        }
    }
    code.len()
}

/// The offset after the `]` that closes the `[` at `open`.
fn after_bracket(code: &[u8], open: usize) -> usize {
    let mut depth = 0;
    for (k, c) in code.iter().enumerate().skip(open) {
        match c {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return k + 1;
                }
            }
            _ => {}
        }
    }
    code.len()
}

fn skip_ws(code: &[u8], mut j: usize) -> usize {
    while code.get(j).is_some_and(|c| c.is_ascii_whitespace()) {
        j += 1;
    }
    j
}

fn word_at(code: &[u8], j: usize) -> &str {
    let end = code[j..]
        .iter()
        .position(|c| !is_ident(*c))
        .map_or(code.len(), |p| j + p);
    std::str::from_utf8(&code[j..end]).unwrap_or("")
}

/// Whether the attribute text `#[cfg(...)]` is one that holds in a test build only.
fn is_test_cfg(attr: &str) -> bool {
    let inner = attr.replace(' ', "");
    inner.starts_with("#[cfg(")
        && !inner.contains("not(")
        && inner
            .trim_start_matches("#[cfg(")
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .any(|w| w == "test")
}

/// Blank every item that is compiled in a test build only (`#[cfg(test)]` and the like, with
/// the other attributes that follow it): inline modules and functions by their braces,
/// declarations by their `;`. Returns the names of the modules that are declared out of line
/// (`#[cfg(test)] mod name;`): their files are test code, and so is whatever they declare.
fn strip_test_items(lexed: &mut Lexed) -> Vec<String> {
    let mut out_of_line = Vec::new();
    let mut from = 0;
    loop {
        let Some(rel) = lexed.code[from..].windows(2).position(|w| w == b"#[") else {
            break;
        };
        let start = from + rel;
        let attr_end = after_bracket(&lexed.code, start + 1);
        let attr = String::from_utf8_lossy(&lexed.code[start..attr_end]).into_owned();
        if !is_test_cfg(&attr) {
            from = start + 2;
            continue;
        }
        // The other attributes of the same item, then the item.
        let mut j = skip_ws(&lexed.code, attr_end);
        while lexed.code[j..].starts_with(b"#[") {
            j = skip_ws(&lexed.code, after_bracket(&lexed.code, j + 1));
        }
        let mut k = j;
        if word_at(&lexed.code, k) == "pub" {
            k += 3;
            k = skip_ws(&lexed.code, k);
            if lexed.code.get(k) == Some(&b'(') {
                k = lexed.code[k..]
                    .iter()
                    .position(|c| *c == b')')
                    .map_or(k, |p| k + p + 1);
                k = skip_ws(&lexed.code, k);
            }
        }
        let end;
        if word_at(&lexed.code, k) == "mod" {
            let name_at = skip_ws(&lexed.code, k + 3);
            let name = word_at(&lexed.code, name_at).to_string();
            let after = skip_ws(&lexed.code, name_at + name.len());
            if lexed.code.get(after) == Some(&b';') {
                out_of_line.push(name);
                end = after + 1;
            } else {
                end = after_brace(&lexed.code, after);
            }
        } else {
            let stop = lexed.code[j..]
                .iter()
                .position(|c| *c == b';' || *c == b'{')
                .map_or(lexed.code.len(), |p| j + p);
            end = if lexed.code.get(stop) == Some(&b'{') {
                after_brace(&lexed.code, stop)
            } else {
                (stop + 1).min(lexed.code.len())
            };
        }
        blank(&mut lexed.code, start, end);
        lexed.strings.retain(|(at, _)| *at < start || *at >= end);
        from = end;
    }
    out_of_line
}

/// The name of the function whose body contains `at`, the innermost one.
fn enclosing_fn(code: &[u8], at: usize) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    let mut from = 0;
    while let Some(rel) = code[from..].windows(3).position(|w| w == b"fn ") {
        let pos = from + rel;
        from = pos + 3;
        if pos > 0 && is_ident(code[pos - 1]) {
            continue;
        }
        let name_at = skip_ws(code, pos + 3);
        let name = word_at(code, name_at).to_string();
        if name.is_empty() {
            continue;
        }
        // The signature ends at a `{` (a body) or a `;` (no body), outside the parentheses.
        let mut depth = 0i32;
        let mut open = None;
        for (k, c) in code.iter().enumerate().skip(name_at + name.len()) {
            match c {
                b'(' | b'[' => depth += 1,
                b')' | b']' => depth -= 1,
                b';' if depth == 0 => break,
                b'{' if depth == 0 => {
                    open = Some(k);
                    break;
                }
                _ => {}
            }
        }
        let Some(open) = open else { continue };
        let end = after_brace(code, open);
        if open <= at && at < end && best.as_ref().is_none_or(|(len, _)| end - open < *len) {
            best = Some((end - open, name));
        }
    }
    best.map(|(_, n)| n)
}

fn words(code: &[u8], word: &str) -> Vec<usize> {
    let w = word.as_bytes();
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(rel) = code[from..].windows(w.len()).position(|x| x == w) {
        let at = from + rel;
        from = at + w.len();
        let before = at == 0 || !is_ident(code[at - 1]);
        let after = code.get(at + w.len()).is_none_or(|c| !is_ident(*c));
        if before && after {
            found.push(at);
        }
    }
    found
}

fn line_of(src: &[u8], at: usize) -> usize {
    1 + src[..at].iter().filter(|c| **c == b'\n').count()
}

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
    let mut lexed: BTreeMap<&str, Lexed> = BTreeMap::new();
    let mut test_prefixes: Vec<String> = Vec::new();
    for (path, src) in files {
        let mut l = lex(src);
        // `#![cfg(test)]` at the top makes the whole file test code.
        let whole = String::from_utf8_lossy(&l.code).contains("#![cfg(test)]");
        let names = strip_test_items(&mut l);
        let (dir, file) = path.rsplit_once('/').unwrap_or(("", path));
        let child_dir = if matches!(file, "lib.rs" | "main.rs" | "mod.rs") {
            dir.to_string()
        } else {
            format!("{dir}/{}", file.trim_end_matches(".rs"))
        };
        for n in names {
            test_prefixes.push(format!("{child_dir}/{n}"));
        }
        if whole {
            test_prefixes.push(path.trim_end_matches(".rs").to_string());
        }
        lexed.insert(path.as_str(), l);
    }
    let is_test_file = |path: &str| {
        path.split('/').any(|c| matches!(c, "tests" | "benches"))
            || test_prefixes
                .iter()
                .any(|p| path == format!("{p}.rs") || path.starts_with(&format!("{p}/")))
    };

    let mut problems = Vec::new();
    let mut scanned_lines = BTreeMap::new();
    let mut used: BTreeSet<(String, String)> = BTreeSet::new();
    for (path, l) in &lexed {
        if is_test_file(path) {
            continue;
        }
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
        for (at, s) in &l.strings {
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

fn read_tree(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        if path.is_dir() {
            if !matches!(name.as_str(), "target" | "node_modules" | ".git") {
                read_tree(root, &path, out);
            }
        } else if name.ends_with(".rs") {
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.insert(rel, std::fs::read_to_string(&path).unwrap());
        }
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

fn tree(files: &[(&str, &str)]) -> BTreeMap<String, String> {
    files
        .iter()
        .map(|(p, s)| (p.to_string(), s.to_string()))
        .collect()
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
