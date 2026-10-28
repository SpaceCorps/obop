//! `obop --version` through the real binary: the name and the core version it always printed,
//! then the commit the build was made from in parentheses (`crates/obop/build.rs`), so a check that
//! pins a build can tell two builds of the same version apart.

use std::process::Command;

fn version(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_obop"))
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(out.stderr.is_empty(), "{:?}", out.stderr);
    String::from_utf8(out.stdout).unwrap()
}

/// `obop 0.2.0 (abcdef012345)`: the commit between the parentheses, or `unknown`.
fn commit_of(line: &str) -> &str {
    let rest = line
        .strip_prefix(&format!("obop {} (", env!("CARGO_PKG_VERSION")))
        .unwrap_or_else(|| panic!("the prefix of earlier builds is gone: {line:?}"));
    rest.strip_suffix(")\n")
        .unwrap_or_else(|| panic!("not closed by a parenthesis and a line end: {line:?}"))
}

#[test]
fn the_version_line_keeps_its_prefix_and_ends_in_the_commit() {
    let line = version(&["--version"]);
    assert!(line.starts_with("obop 0.2.0 ("), "{line:?}");
    assert_eq!(line.lines().count(), 1, "{line:?}");
    let commit = commit_of(&line);
    assert!(
        commit == "unknown"
            || (commit.len() == 12
                && commit
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))),
        "twelve lower case hexadecimal digits, or unknown: {commit:?}"
    );
    // With no argument it is the same line.
    assert_eq!(version(&[]), line);
}

/// One line of `git -C root args...`, when git ran and succeeded.
fn git(root: &std::path::Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8(out.stdout).ok())
        .flatten()
        .map(|s| s.trim().to_string())
}

/// In a checkout of this workspace the build knows the commit it was made from: it is the first
/// twelve digits of `git rev-parse HEAD`. Skipped when the build was told another commit
/// (`OBOP_BUILD_COMMIT`, as the Docker build does), when there is no git program, and when the
/// sources are not the top of a repository (a copy unpacked inside another one says `unknown`).
#[test]
fn in_a_checkout_the_commit_is_the_head_of_the_repository() {
    if std::env::var_os("OBOP_BUILD_COMMIT").is_some() {
        return;
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let Some(top) = git(&root, &["rev-parse", "--show-toplevel"]) else {
        return;
    };
    if std::path::Path::new(&top).canonicalize().ok().as_deref() != Some(root.as_path()) {
        return;
    }
    let head = git(&root, &["rev-parse", "HEAD"]).expect("a checkout has a HEAD");
    let line = version(&["--version"]);
    let commit = commit_of(&line);
    assert!(
        head.starts_with(commit) && commit.len() == 12,
        "the binary says {commit}, HEAD is {head}"
    );
}
