//! The kinds the code writes to the security log, against the list in the description of the
//! field `security_event.kind` (`kernel/obop-pkg/src/installer.rs`, the text that OpenAPI, MCP
//! and `/api/meta` show). An agent or an auditor reads that text as the list of entries the log
//! can hold; it was missing `plan_lost`, the Drive kinds and others until this test existed.
//!
//! **What it checks.** In the non-test Rust code of every crate under `crates/` but the control
//! plane (`obop-platform`, which has a log of its own in its own database): every string literal
//! that is passed as the kind to one of the functions in [`WRITERS`] is in the list; and every
//! kind in the list appears as a string literal somewhere in that code (or is one of the two
//! that are built with `format!`, named in [`BUILT`]), so that a kind that is no longer written
//! does not stay on the list.
//!
//! **What it does not check.** A kind that reaches a writer through a variable or a constant
//! (`EVENT_REGISTERED`, a `match` arm) is not seen as an argument; the second check still sees its
//! literal. A kind built at run time is not seen at all. A new function that wraps one of the
//! writers must be added to [`WRITERS`]. It is a textual scan (`src_scan.rs`), not a parser.

use crate::src_scan::{Lexed, is_ident, line_of, non_test, read_tree, tree};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The functions that take the kind of an entry of the log: the name, the number of arguments
/// the one meant has (0: any; the receiver of a method is not counted) and the place of the
/// kind among them (from 0).
const WRITERS: [(&str, usize, usize); 12] = [
    ("record_event", 6, 3),
    // The wrapper of the command line: `record_event(backend, kind, detail)`.
    ("record_event", 3, 1),
    ("try_record_event", 6, 3),
    ("event_op", 5, 2),
    ("audit_event", 5, 3),
    ("audit_row", 4, 2),
    // `audit(user, kind, ...)` of the installer, the authentication, OAuth and Microsoft sign-in.
    ("audit", 0, 1),
    ("audit_throttled", 0, 1),
    // `event(user, kind, actor, detail)` of the sign-in code.
    ("event", 0, 1),
    // `record(state, who, kind, detail)` of the Drive routes, and the reply that builds it.
    ("record", 4, 2),
    ("create_reply", 5, 2),
    ("Change::new", 4, 1),
];

/// Where the list is.
const DESCRIPTION_FILE: &str = "kernel/obop-pkg/src/installer.rs";
/// The text before the list; the back office reads the list after it (`securityEvents.test.ts`).
const LEAD: &str = "What happened: ";

/// The control plane writes to the same table in its own database.
const OTHER_LOG: &str = "kernel/obop-platform/";

/// Kinds that the code builds at run time (`format!("{kind}_failed")` in the Drive routes).
const BUILT: [&str; 2] = ["drive_create_failed", "drive_upload_failed"];

/// The argument ranges of the call whose `(` is at `open`.
fn args(l: &Lexed, open: usize) -> Vec<(usize, usize)> {
    let code = &l.code;
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut from = open + 1;
    for (k, c) in code.iter().enumerate().skip(open) {
        match c {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    // A comma after the last argument leaves nothing to add (a string literal is
                    // blank in the code, so it counts as something).
                    if !code[from..k].iter().all(u8::is_ascii_whitespace)
                        || l.strings.iter().any(|(s, e, _)| *s >= from && *e <= k)
                    {
                        out.push((from, k));
                    }
                    break;
                }
            }
            b',' if depth == 1 => {
                out.push((from, k));
                from = k + 1;
            }
            _ => {}
        }
    }
    out
}

/// The string literal that is the whole of the argument `a..b`, if it is one.
fn literal(l: &Lexed, a: usize, b: usize) -> Option<String> {
    let blank = |x: usize, y: usize| l.code[x..y].iter().all(u8::is_ascii_whitespace);
    l.strings
        .iter()
        .find(|(s, e, _)| *s >= a && *e <= b && blank(a, *s) && blank(*e, b))
        .map(|(_, _, c)| c.clone())
}

/// The kinds passed as literals to the writers in one file, with the line of each.
fn kinds_in(l: &Lexed) -> Vec<(String, usize)> {
    let code = &l.code;
    let mut out = Vec::new();
    for (name, arity, place) in WRITERS {
        let n = name.as_bytes();
        let mut from = 0;
        while let Some(rel) = code[from..].windows(n.len()).position(|w| w == n) {
            let at = from + rel;
            from = at + n.len();
            let open = at + n.len();
            if (at > 0 && is_ident(code[at - 1])) || code.get(open) != Some(&b'(') {
                continue;
            }
            // A definition names the parameter, it does not pass a kind.
            if at >= 3 && &code[at - 3..at] == b"fn " {
                continue;
            }
            let given = args(l, open);
            if arity != 0 && given.len() != arity {
                continue;
            }
            if let Some(&(a, b)) = given.get(place)
                && let Some(kind) = literal(l, a, b)
            {
                out.push((kind, line_of(code, at)));
            }
        }
    }
    out
}

/// The kinds the list in the description names, from the text of the file that holds it.
fn listed(lexed: &BTreeMap<String, Lexed>) -> Vec<String> {
    let l = lexed
        .get(DESCRIPTION_FILE)
        .unwrap_or_else(|| panic!("{DESCRIPTION_FILE} is not among the files"));
    let text = l
        .strings
        .iter()
        .map(|(_, _, c)| c)
        .find(|c| c.starts_with(LEAD))
        .unwrap_or_else(|| panic!("no description starts with {LEAD:?}"));
    text[LEAD.len()..].split(", ").map(str::to_string).collect()
}

/// (kind -> places) of what the code passes to the writers, and every string literal of the
/// non-test code outside the description itself.
fn written_by(
    lexed: &BTreeMap<String, Lexed>,
) -> (BTreeMap<String, Vec<String>>, BTreeSet<String>) {
    let mut kinds: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut literals = BTreeSet::new();
    for (path, l) in lexed {
        for (_, _, c) in &l.strings {
            if !c.starts_with(LEAD) {
                literals.insert(c.clone());
            }
        }
        if path.starts_with(OTHER_LOG) {
            continue;
        }
        for (k, line) in kinds_in(l) {
            kinds.entry(k).or_default().push(format!("{path}:{line}"));
        }
    }
    (kinds, literals)
}

#[test]
fn the_kinds_the_code_writes_are_the_kinds_the_description_lists() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = BTreeMap::new();
    read_tree(&root, &root, &mut files);
    let lexed = non_test(&files);
    let list = listed(&lexed);
    let (written, literals) = written_by(&lexed);

    let mut seen = BTreeSet::new();
    for k in &list {
        assert!(seen.insert(k), "{k} is listed twice");
    }
    // The scan found the writers it is meant to find (a scan that reads nothing finds no gap).
    for k in [
        "plan_lost",
        "sign_signed",
        "sign_refused",
        "sign_link_issued",
        "oauth_authorize",
        "passkey_login",
        "oidc_login",
        "drive_import",
        "migration_run",
        "user_create",
        "lockout",
        "settings_change",
    ] {
        assert!(
            written.contains_key(k),
            "the scan did not find a writer of {k}"
        );
    }
    assert!(written.len() > 40, "only {} kinds found", written.len());

    let missing: Vec<String> = written
        .iter()
        .filter(|(k, _)| !list.contains(k))
        .map(|(k, at)| format!("{k} (written at {})", at.join(", ")))
        .collect();
    assert!(
        missing.is_empty(),
        "the code writes kinds that the description of security_event.kind (installer.rs) does \
         not list:\n{}",
        missing.join("\n")
    );
    let gone: Vec<&String> = list
        .iter()
        .filter(|k| !literals.contains(*k) && !BUILT.contains(&k.as_str()))
        .collect();
    assert!(
        gone.is_empty(),
        "the description lists kinds that no code writes any more: {gone:?}"
    );
}

#[test]
fn the_places_of_the_kind_in_the_writers_are_read_and_test_code_is_left_out() {
    let src = r#"
fn a(db: &dyn Backend) {
    record_event(db, 1, "u", "alpha_one", "root", "d");
    obop_commands::audit::try_record_event(db, 1, "u", "alpha_two", "root", "d").ok();
    let op = event_op(1, "u", "alpha_three", "root", "d");
    obop_ops::audit_event(db, 1, "root", "alpha_four", "d");
    let _ = audit_row(1, "root", "alpha_five", "d");
}
fn b(&self) {
    self.audit("u", "beta_one", "d");
    self.event("u", "beta_two", "a", "d");
    self.audit_throttled(src, "beta_three", line, "d");
    record(s, who, "beta_four", &format!("{x}, {y}"));
    create_reply(&s, &p.id, "beta_five", parent, made);
    let c = Change::new(&target, "beta_six", actor, "a, b");
}
fn not_literals(db: &dyn Backend, kind: &str) {
    record_event(db, 1, "u", kind, "root", "d");
    record_event(db, 1, "u", &format!("x_{kind}"), "root", "d");
    self.audit("u", if a { "left_one" } else { "right_one" }, "d");
    record_event(db, 1, "alpha_not_the_kind", kind, "root", "d");
    // record_event(db, 1, "u", "in_a_comment", "root", "d");
    let _ = "record_event(db, 1, \"u\", \"in_a_string\", \"root\", \"d\")";
}
fn event(&self, user: &str, kind: &str, actor: &str, detail: &str) {}
fn audit(&self, user: &str, kind: &str) {}
fn my_audit(&self) { my_audit("u", "not_a_writer", "d"); }
#[cfg(test)]
mod tests {
    fn t(db: &dyn Backend) { record_event(db, 1, "u", "only_in_a_test", "root", "d"); }
}
fn late(db: &dyn Backend) { record_event(db, 1, "u", "late_one", "root", "d"); }
"#;
    let files = tree(&[("kernel/x/src/lib.rs", src)]);
    let lexed = non_test(&files);
    let (found, _) = written_by(&lexed);
    let got: Vec<&str> = found.keys().map(String::as_str).collect();
    assert_eq!(
        got,
        [
            "alpha_four",
            "alpha_one",
            "alpha_three",
            "alpha_two",
            "alpha_five",
            "beta_five",
            "beta_four",
            "beta_one",
            "beta_six",
            "beta_three",
            "beta_two",
            "late_one",
        ]
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
    );
    // The control plane's own log is not this one.
    let files = tree(&[(
        "kernel/obop-platform/src/update.rs",
        r#"fn f(&self) { self.audit("s", "release_published", "a", "d"); }"#,
    )]);
    assert!(written_by(&non_test(&files)).0.is_empty());
}

#[test]
fn the_list_is_read_from_the_text_of_the_description() {
    let src = r#"
fn f() -> &'static str {
    "What happened: one, two_two, three"
}
"#;
    let files = tree(&[(DESCRIPTION_FILE, src)]);
    assert_eq!(listed(&non_test(&files)), ["one", "two_two", "three"]);
}
