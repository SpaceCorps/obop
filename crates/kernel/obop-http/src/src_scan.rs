//! The reading of the Rust source of the workspace for the tests that keep a rule in step with
//! the code: a mini lexer (comments blanked, string and char literals kept apart from the code,
//! raw strings, lifetimes told from char literals), the removal of what is compiled in a test
//! build only (`#[cfg(test)]` items and modules, the files such modules are declared in, `tests/`
//! and `benches/` directories) and a few helpers. It is a textual scan, not a parser.

use std::collections::BTreeMap;
use std::path::Path;

pub(crate) fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// A source with comments, string literals and char literals blanked to spaces (line breaks
/// kept, so that offsets and line numbers stay), and the content of each string literal.
pub(crate) struct Lexed {
    pub code: Vec<u8>,
    /// Start and end of each string literal in the source, and its content.
    pub strings: Vec<(usize, usize, String)>,
    /// What [`strip_test_items`] took out as test code.
    pub stripped: Vec<Stripped>,
}

pub(crate) fn blank(code: &mut [u8], from: usize, to: usize) {
    for b in &mut code[from..to] {
        if *b != b'\n' {
            *b = b' ';
        }
    }
}

pub(crate) fn lex(src: &str) -> Lexed {
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
            // `r"..."`, `br"..."` (bytes) and `cr"..."` (a C string): no escape inside.
            b'r' if (i == 0
                || !is_ident(b[i - 1])
                || (matches!(b[i - 1], b'b' | b'c') && (i == 1 || !is_ident(b[i - 2]))))
                && raw_string(b, i).is_some() =>
            {
                let (open, close, hashes) = raw_string(b, i).unwrap();
                strings.push((i, close + 1 + hashes, src[open..close].to_string()));
                blank(&mut code, i, close + 1 + hashes);
                i = close + 1 + hashes;
            }
            b'"' => {
                let mut j = i + 1;
                while j < b.len() && b[j] != b'"' {
                    j += if b[j] == b'\\' { 2 } else { 1 };
                }
                let end = j.min(b.len());
                strings.push((i, (end + 1).min(b.len()), src[i + 1..end].to_string()));
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
    Lexed {
        code,
        strings,
        stripped: Vec::new(),
    }
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
pub(crate) fn after_brace(code: &[u8], open: usize) -> usize {
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

pub(crate) fn skip_ws(code: &[u8], mut j: usize) -> usize {
    while code.get(j).is_some_and(|c| c.is_ascii_whitespace()) {
        j += 1;
    }
    j
}

pub(crate) fn word_at(code: &[u8], j: usize) -> &str {
    let end = code[j..]
        .iter()
        .position(|c| !is_ident(*c))
        .map_or(code.len(), |p| j + p);
    std::str::from_utf8(&code[j..end]).unwrap_or("")
}

/// What a `#[cfg(...)]` attribute that holds in a test build only took out of the code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Taken {
    /// `mod name { ... }` or `mod name;` (the files of a module declared out of line are test code
    /// too, see [`non_test`]).
    Module,
    /// Any other item: `fn`, `impl`, `use`, `struct`, `enum`, `const`, `static`, `type`, `trait`,
    /// `extern`, `macro_rules!`. It ends where the item does: at its matching `}` or its `;`.
    Item,
    /// What is not an item: a struct field, an enum variant, a match arm, a statement, an argument.
    /// It ends at the next `,` or `;` at its own depth (or after its block), never later.
    Construct,
}

/// One place where test-only code was taken out: byte offsets in the source and what it was.
#[derive(Debug, Clone)]
pub(crate) struct Stripped {
    pub start: usize,
    pub end: usize,
    pub what: Taken,
}

/// Split `s` at the commas that are not inside parentheses.
fn split_top(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut from) = (0i32, 0);
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&s[from..i]);
                from = i + 1;
            }
            _ => {}
        }
    }
    out.push(&s[from..]);
    out
}

/// Whether the predicate of a `cfg` holds in a test build only: `test`; `all(...)` when any of its
/// parts does (the item needs them all); `any(...)` only when every part does (any other part
/// puts the item in a production build too); never `not(...)`, a feature or a platform.
fn test_only(pred: &str) -> bool {
    let p = pred.trim();
    if p == "test" {
        return true;
    }
    for (name, needs_all) in [("all", false), ("any", true)] {
        if let Some(inner) = p
            .strip_prefix(name)
            .map(str::trim_start)
            .and_then(|r| r.strip_prefix('('))
            .and_then(|r| r.strip_suffix(')'))
        {
            let parts: Vec<bool> = split_top(inner)
                .into_iter()
                .filter(|x| !x.trim().is_empty())
                .map(test_only)
                .collect();
            return if needs_all {
                !parts.is_empty() && parts.iter().all(|b| *b)
            } else {
                parts.iter().any(|b| *b)
            };
        }
    }
    false
}

/// Whether the attribute text `#[cfg(...)]` is one that holds in a test build only.
pub(crate) fn is_test_cfg(attr: &str) -> bool {
    let a = attr.trim();
    a.strip_prefix("#[")
        .map(str::trim_start)
        .and_then(|r| r.strip_prefix("cfg"))
        .map(str::trim_start)
        .and_then(|r| r.strip_prefix('('))
        .and_then(|r| r.strip_suffix(']'))
        .map(str::trim_end)
        .and_then(|r| r.strip_suffix(')'))
        .is_some_and(test_only)
}

/// The keywords that start an item.
const ITEM_HEADS: [&str; 13] = [
    "mod",
    "fn",
    "impl",
    "use",
    "struct",
    "enum",
    "union",
    "trait",
    "type",
    "const",
    "static",
    "extern",
    "macro_rules",
];

/// The end of an item that starts at `from`: after the `;` that ends it, or after its `{}` body
/// (and the `;` that follows a body in a `use a::{b, c};` or a `const X: S = S { .. };`).
fn item_end(code: &[u8], from: usize) -> usize {
    let mut depth = 0i32;
    for (k, c) in code.iter().enumerate().skip(from) {
        match c {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth -= 1,
            b';' if depth <= 0 => return k + 1,
            b'{' if depth <= 0 => {
                let after = after_brace(code, k);
                let next = skip_ws(code, after);
                return if code.get(next) == Some(&b';') {
                    next + 1
                } else {
                    after
                };
            }
            _ => {}
        }
    }
    code.len()
}

/// The end of a construct that is not an item and starts at `from`: after the `,` or `;` at its
/// own depth, before the delimiter that closes what it is in, or after its block when that is
/// followed by the start of something else (`else`, a method call and `?` continue it).
fn construct_end(code: &[u8], from: usize) -> usize {
    let mut depth = 0i32;
    let mut k = from;
    while k < code.len() {
        match code[k] {
            b'(' | b'[' => depth += 1,
            b')' | b']' if depth == 0 => return k,
            b')' | b']' => depth -= 1,
            b'}' if depth == 0 => return k,
            b',' | b';' if depth == 0 => return k + 1,
            b'{' => {
                let after = after_brace(code, k);
                if depth == 0 {
                    let next = skip_ws(code, after);
                    match code.get(next) {
                        Some(b',' | b';') => return next + 1,
                        Some(b'.' | b'?') => {}
                        _ if word_at(code, next) == "else" => {}
                        _ => return after,
                    }
                }
                k = after;
                continue;
            }
            _ => {}
        }
        k += 1;
    }
    code.len()
}

/// Blank every item or construct that is compiled in a test build only (`#[cfg(test)]`,
/// `#[cfg(all(test, ..))]`, `#[cfg(any(test, ..))]` when every branch is a test), and record what
/// was taken. Only the thing the attribute is on goes: an item by its end, a field, variant, arm or
/// statement by its `,` or `;`. Returns the names of the modules declared out of line
/// (`#[cfg(test)] mod name;`): their files are test code, and so is whatever they declare.
pub(crate) fn strip_test_items(lexed: &mut Lexed) -> Vec<String> {
    let mut out_of_line = Vec::new();
    let mut from = 0;
    while let Some(rel) = lexed.code[from..].windows(2).position(|w| w == b"#[") {
        let start = from + rel;
        let attr_end = after_bracket(&lexed.code, start + 1);
        let attr = String::from_utf8_lossy(&lexed.code[start..attr_end]).into_owned();
        if !is_test_cfg(&attr) {
            from = start + 2;
            continue;
        }
        // The other attributes of the same thing, then the thing.
        let mut j = skip_ws(&lexed.code, attr_end);
        while lexed.code[j..].starts_with(b"#[") {
            j = skip_ws(&lexed.code, after_bracket(&lexed.code, j + 1));
        }
        // Visibility and qualifiers do not make an item head of a field (`pub examined: ..`).
        let mut k = j;
        loop {
            match word_at(&lexed.code, k) {
                "pub" => {
                    k = skip_ws(&lexed.code, k + 3);
                    if lexed.code.get(k) == Some(&b'(') {
                        k = lexed.code[k..]
                            .iter()
                            .position(|c| *c == b')')
                            .map_or(k, |p| k + p + 1);
                        k = skip_ws(&lexed.code, k);
                    }
                }
                w @ ("async" | "unsafe" | "default") => k = skip_ws(&lexed.code, k + w.len()),
                _ => break,
            }
        }
        let head = word_at(&lexed.code, k).to_string();
        let (end, what);
        if head == "mod" {
            let name_at = skip_ws(&lexed.code, k + 3);
            let name = word_at(&lexed.code, name_at).to_string();
            let after = skip_ws(&lexed.code, name_at + name.len());
            what = Taken::Module;
            if lexed.code.get(after) == Some(&b';') {
                out_of_line.push(name);
                end = after + 1;
            } else {
                end = after_brace(&lexed.code, after);
            }
        } else if ITEM_HEADS.contains(&head.as_str())
            && lexed.code.get(k + head.len()) != Some(&b':')
        {
            what = Taken::Item;
            end = item_end(&lexed.code, k);
        } else {
            what = Taken::Construct;
            end = construct_end(&lexed.code, j);
        }
        blank(&mut lexed.code, start, end);
        lexed.strings.retain(|(at, _, _)| *at < start || *at >= end);
        lexed.stripped.push(Stripped { start, end, what });
        from = end;
    }
    out_of_line
}

/// Whether the file is test code as a whole: one of the inner attributes at its very top
/// (`#![cfg(test)]`, `#![cfg(all(test, ..))]`, after any other inner attributes; comments are
/// blank already) holds in a test build only. An inner attribute anywhere else belongs to the
/// module, function or macro it is in, not to the file.
pub(crate) fn is_test_file(code: &[u8]) -> bool {
    let mut j = skip_ws(code, 0);
    while code[j..].starts_with(b"#![") {
        let end = after_bracket(code, j + 2);
        let attr = String::from_utf8_lossy(&code[j + 3..end]).into_owned();
        if is_test_cfg(&format!("#[{attr}")) {
            return true;
        }
        j = skip_ws(code, end);
    }
    false
}

/// The name of the function whose body contains `at`, the innermost one.
pub(crate) fn enclosing_fn(code: &[u8], at: usize) -> Option<String> {
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

pub(crate) fn words(code: &[u8], word: &str) -> Vec<usize> {
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

pub(crate) fn line_of(src: &[u8], at: usize) -> usize {
    1 + src[..at].iter().filter(|c| **c == b'\n').count()
}

/// One source file as text with the items and constructs that only a test build compiles
/// (`#[cfg(test)]`, see [`strip_test_items`]) replaced by spaces, line breaks kept. Comments and
/// string literals are left as written, for a test that reads the keys a file asks for; it ends
/// where the attribute's thing ends, not at the first `#[cfg(test)]` of the file.
pub(crate) fn without_test_items(src: &str) -> String {
    let mut l = lex(src);
    let _ = strip_test_items(&mut l);
    let mut out = src.as_bytes().to_vec();
    for s in &l.stripped {
        for b in &mut out[s.start..s.end] {
            if *b != b'\n' {
                *b = b' ';
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The non-test code of every file of `files` (paths from `crates/`, with `/`): comments and
/// literals blanked, test items taken out, the files of test modules left out altogether.
pub(crate) fn non_test(files: &BTreeMap<String, String>) -> BTreeMap<String, Lexed> {
    let mut lexed: BTreeMap<String, Lexed> = BTreeMap::new();
    let mut test_prefixes: Vec<String> = Vec::new();
    for (path, src) in files {
        let mut l = lex(src);
        // `#![cfg(test)]` at the top makes the whole file test code.
        let whole = is_test_file(&l.code);
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
        lexed.insert(path.clone(), l);
    }
    lexed.retain(|path, _| {
        !(path.split('/').any(|c| matches!(c, "tests" | "benches"))
            || test_prefixes
                .iter()
                .any(|p| *path == format!("{p}.rs") || path.starts_with(&format!("{p}/"))))
    });
    lexed
}

pub(crate) fn read_tree(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
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

pub(crate) fn tree(files: &[(&str, &str)]) -> BTreeMap<String, String> {
    files
        .iter()
        .map(|(p, s)| (p.to_string(), s.to_string()))
        .collect()
}
