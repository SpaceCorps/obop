//! `obop --version` through the real binary: the name and the core version it always printed,
//! then the commit the build was made from in parentheses (`crates/obop/build.rs`), so a check that
//! pins a build can tell two builds of the same version apart.

use std::path::Path;
use std::process::Command;

// The rule the build script used, to ask it again here.
#[path = "../src/build_commit.rs"]
mod build_commit;

/// Where the build script took the commit from: `env` (the variable `OBOP_BUILD_COMMIT` was set
/// for the build, as the Docker build does), `git` or `none`. Fixed when the program was built, so
/// it does not depend on the environment this test runs in.
const SOURCE: &str = env!("OBOP_BUILD_COMMIT_SOURCE");
/// The commit as the build script handed it to the compiler.
const BAKED: &str = env!("OBOP_BUILD_COMMIT");

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

/// The binary says what the build script decided, and the build script decided what git says now:
/// in a checkout of this workspace the first twelve digits of `git rev-parse HEAD`, and `unknown`
/// only where git has no commit for these sources. Nothing here is skipped: there is a branch for
/// each place the commit can come from, whatever the environment of the test is.
#[test]
fn the_commit_is_the_one_the_build_found_and_in_a_checkout_it_is_head() {
    let line = version(&["--version"]);
    let commit = commit_of(&line);
    assert_eq!(
        commit, BAKED,
        "the binary prints what the build handed over"
    );

    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let in_git = build_commit::git_head(crate_dir, &crate_dir.join("../.."));
    match SOURCE {
        // Told by the variable (the image build). A hash, cut to twelve digits; git is not asked.
        "env" => assert_eq!(build_commit::short(commit).as_deref(), Some(commit)),
        // Found in a checkout: it must still be there and must say the same. A commit made since
        // the build has made Cargo build again (the script watches what git changes).
        "git" => {
            let head = in_git.expect("the build found a checkout; the test finds none");
            assert_eq!(commit, head.commit, "the binary is not at HEAD");
        }
        // Found nothing: then git has nothing for these sources either (a copy of the sources, no
        // git program, no commit yet). A checkout that the build missed is a failure.
        "none" => {
            assert_eq!(commit, "unknown");
            assert_eq!(
                in_git, None,
                "the build found no commit, but these sources are a checkout with one"
            );
        }
        other => panic!("OBOP_BUILD_COMMIT_SOURCE is {other:?}"),
    }
}
