//! The kinds the code writes to the security log, against the list in the description of the
//! field `security_event.kind` (`kernel/obop-pkg/src/installer.rs`, the text that OpenAPI, MCP
//! and `/api/meta` show). An agent or an auditor reads that text as the list of entries the log
//! can hold; it was missing `plan_lost`, the Drive kinds and others until this test existed.
//!
//! **What it checks**, in the non-test Rust code of every crate under `crates/` but the control
//! plane (`obop-platform`, which has a log of its own in its own database):
//! 1. every string literal that is passed as the kind to one of the functions in [`WRITERS`] is in
//!    the list (a kind written and not listed);
//! 2. every kind in the list is accounted for in one of three ways: it is passed as a literal to
//!    one of the [`WRITERS`]; or it is named in [`VIA`] with the file that holds its literal (the
//!    kinds that reach a writer through a constant or a `match` arm, which the scan does not
//!    follow: for these it checks only that the literal is still in that file); or it is named
//!    in [`BUILT`] (built with `format!` from a kind that is passed to a writer, and the
//!    `format!` is still where the row says). A kind that is accounted for in none of these is
//!    "no longer written" and must leave the list. A row of [`VIA`] or [`BUILT`] that is no
//!    longer needed (its kind is passed to a writer directly now, or is not listed) is a failure
//!    too.
//!
//! **What it does not check.** The direction in (2) is strong for the kinds passed to a writer and
//! weak for those in [`VIA`]: a `VIA` kind stays on the list as long as its literal stays in its
//! file, whether or not anything still writes it. A kind built at run time in another way is not
//! seen at all. A new function that wraps one of the writers must be added to [`WRITERS`]. It is
//! a textual scan (`src_scan.rs`), not a parser.

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

/// Kinds that the code builds at run time: (the kind, the kind it is built from, the file with
/// both the writer's call and the `format!`). `format!("{kind}_failed")` in the Drive routes.
const BUILT: [(&str, &str, &str); 2] = [
    (
        "drive_create_failed",
        "drive_create",
        "kernel/obop-http/src/drive.rs",
    ),
    (
        "drive_upload_failed",
        "drive_upload",
        "kernel/obop-http/src/drive.rs",
    ),
];
/// The text the `format!` of [`BUILT`] has.
const BUILT_FORMAT: &str = "{kind}_failed";

/// Kinds that reach a writer through a constant, a `match` arm or a variable, with the file that
/// holds the literal.
const VIA: [(&str, &str); 14] = [
    ("module_install", "kernel/obop-pkg/src/installer.rs"),
    ("module_update", "kernel/obop-pkg/src/installer.rs"),
    (
        "instance_update_finished",
        "kernel/obop-update/src/updater.rs",
    ),
    ("oauth_revoked", "kernel/obop-oauth/src/token.rs"),
    ("oauth_refresh_reuse", "kernel/obop-oauth/src/token.rs"),
    ("oauth_code_reuse", "kernel/obop-oauth/src/token.rs"),
    ("device_registered", "kernel/obop-commands/src/devices.rs"),
    ("device_renamed", "kernel/obop-commands/src/devices.rs"),
    ("device_revoked", "kernel/obop-commands/src/devices.rs"),
    ("mileage_route_read", "modules/mileage/src/audit.rs"),
    (
        "oidc_provider_configured",
        "kernel/obop-auth/src/oidc/mod.rs",
    ),
    ("oidc_provider_cleared", "kernel/obop-auth/src/oidc/mod.rs"),
    ("oidc_bind", "kernel/obop-auth/src/oidc/mod.rs"),
    ("oidc_cancelled", "kernel/obop-auth/src/oidc/mod.rs"),
];

/// Whether a file's code holds `kind` as a string literal.
fn has_literal(l: &Lexed, kind: &str) -> bool {
    l.strings.iter().any(|(_, _, c)| c == kind)
}

/// The listed kinds that nothing accounts for (see the header), and the rows of `via` and `built`
/// that are not needed or do not hold.
fn unaccounted(
    list: &[String],
    written: &BTreeMap<String, Vec<String>>,
    lexed: &BTreeMap<String, Lexed>,
    via: &[(&str, &str)],
    built: &[(&str, &str, &str)],
) -> Vec<String> {
    let mut out = Vec::new();
    for k in list {
        if written.contains_key(k) {
            continue;
        }
        if let Some((_, file)) = via.iter().find(|(v, _)| v == k) {
            if !lexed.get(*file).is_some_and(|l| has_literal(l, k)) {
                out.push(format!(
                    "{k}: VIA says {file} holds its literal, and it does not"
                ));
            }
        } else if let Some((_, base, file)) = built.iter().find(|(b, _, _)| b == k) {
            let called = written
                .get(*base)
                .is_some_and(|at| at.iter().any(|a| a.starts_with(file)));
            let formatted = lexed
                .get(*file)
                .is_some_and(|l| has_literal(l, BUILT_FORMAT));
            if !called || !formatted {
                out.push(format!(
                    "{k}: BUILT says {file} passes {base} to a writer and builds {BUILT_FORMAT:?}                      (passes: {called}, builds: {formatted})"
                ));
            }
        } else {
            out.push(format!("{k}: no code passes it to a writer"));
        }
    }
    for (k, _) in via {
        if !list.iter().any(|l| l == k) {
            out.push(format!("VIA names {k}, which is not listed: drop the row"));
        } else if written.contains_key(*k) {
            out.push(format!("{k} is passed to a writer now: drop its VIA row"));
        }
    }
    for (k, _, _) in built {
        if !list.iter().any(|l| l == k) {
            out.push(format!(
                "BUILT names {k}, which is not listed: drop the row"
            ));
        }
    }
    out
}

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

/// kind -> places, of what the code passes to the writers as a literal.
fn written_by(lexed: &BTreeMap<String, Lexed>) -> BTreeMap<String, Vec<String>> {
    let mut kinds: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (path, l) in lexed {
        if path.starts_with(OTHER_LOG) {
            continue;
        }
        for (k, line) in kinds_in(l) {
            kinds.entry(k).or_default().push(format!("{path}:{line}"));
        }
    }
    kinds
}

#[test]
fn the_kinds_the_code_writes_are_the_kinds_the_description_lists() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = BTreeMap::new();
    read_tree(&root, &root, &mut files);
    let lexed = non_test(&files);
    let list = listed(&lexed);
    let written = written_by(&lexed);

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
    let gone = unaccounted(&list, &written, &lexed, &VIA, &BUILT);
    assert!(
        gone.is_empty(),
        "the description lists kinds that no code writes any more, or a row of VIA or BUILT is          stale:
{}",
        gone.join("
")
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
    let found = written_by(&lexed);
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
    assert!(written_by(&non_test(&files)).is_empty());
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

#[test]
fn a_listed_kind_that_only_a_string_somewhere_names_is_not_written() {
    // The scan used to take any string literal in the code for a write: a kind in a doc string, a
    // label or a test helper kept a dead kind on the list. Now a kind is accounted for by a
    // writer's call, or by a row of VIA that names the file, or by a row of BUILT.
    let files = tree(&[
        (
            "kernel/x/src/lib.rs",
            r#"fn f(&self) { self.audit("u", "alive", "d"); let _ = "dead_but_a_string"; }"#,
        ),
        (
            "kernel/x/src/other.rs",
            r#"const K: &str = "via_kind"; const B: &str = "{kind}_failed";"#,
        ),
        (
            "kernel/x/src/moved.rs",
            r#"fn g() { self.audit("u", "made", "d"); }"#,
        ),
        (
            "kernel/x/src/made.rs",
            r#"fn h(&self) { let _ = format!("{kind}_failed"); self.audit("u", "made", "d"); }"#,
        ),
    ]);
    let lexed = non_test(&files);
    let written = written_by(&lexed);
    let list = |names: &[&str]| names.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let none = |l: &[String], via: &[(&str, &str)], built: &[(&str, &str, &str)]| {
        unaccounted(l, &written, &lexed, via, built)
    };
    // Written: fine. A string that is no write: not.
    assert!(none(&list(&["alive"]), &[], &[]).is_empty());
    let got = none(&list(&["alive", "dead_but_a_string"]), &[], &[]);
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(got[0].contains("dead_but_a_string"), "{got:?}");
    // A VIA row holds when its file has the literal, and not when another file does.
    let via = [("via_kind", "kernel/x/src/other.rs")];
    assert!(none(&list(&["via_kind"]), &via, &[]).is_empty());
    let moved = [("via_kind", "kernel/x/src/lib.rs")];
    assert_eq!(none(&list(&["via_kind"]), &moved, &[]).len(), 1);
    // Without a row it is not accounted for.
    assert_eq!(none(&list(&["via_kind"]), &[], &[]).len(), 1);
    // A row that is not needed: its kind is not listed, or is passed to a writer now.
    assert_eq!(none(&list(&[]), &via, &[]).len(), 1);
    assert_eq!(
        none(&list(&["alive"]), &[("alive", "kernel/x/src/lib.rs")], &[]).len(),
        1
    );
    // A built kind needs the call of its base kind and the format, both in the file named.
    let built = [("made_failed", "made", "kernel/x/src/made.rs")];
    assert!(none(&list(&["made_failed"]), &[], &built).is_empty());
    let wrong_file = [("made_failed", "made", "kernel/x/src/moved.rs")];
    assert_eq!(none(&list(&["made_failed"]), &[], &wrong_file).len(), 1);
    let no_base = [("made_failed", "unmade", "kernel/x/src/made.rs")];
    assert_eq!(none(&list(&["made_failed"]), &[], &no_base).len(), 1);
    assert_eq!(none(&list(&[]), &[], &built).len(), 1);
}
