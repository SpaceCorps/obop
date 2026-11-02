//! The commit a build was made from, for `obop --version`.
//!
//! `build.rs` finds it (the environment variable `OBOP_BUILD_COMMIT`, which the Docker build sets
//! from its build argument, else `git rev-parse HEAD` in the checkout) and hands it to the
//! compiler as `OBOP_BUILD_COMMIT`; `main` prints it. The build script and `tests/version.rs`
//! include this file by path, so the rule that decides what counts as a commit, and what makes
//! the build look at git again, is the same in all three. Its tests are in `build_commit_tests.rs`.

// The build script uses `short` and `git_head`; the program uses `version_line`.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// How many hexadecimal digits of the hash the version line shows.
pub const SHORT: usize = 12;

/// What the line shows when the build knew no commit (no `.git`, no build argument).
pub const UNKNOWN: &str = "unknown";

/// The first twelve digits, in lower case, of a commit hash: `raw` must be a hash of twelve to 64
/// hexadecimal digits (a SHA-1 is 40, a SHA-256 is 64), with nothing else around it but
/// whitespace. Anything else is not a commit (`None`).
pub fn short(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let digits = (SHORT..=64).contains(&raw.len()) && raw.bytes().all(|b| b.is_ascii_hexdigit());
    digits.then(|| raw[..SHORT].to_ascii_lowercase())
}

/// The line `obop --version` prints: `obop 0.2.1 (abcdef012345)`. The prefix `obop 0.2.1` is the
/// one earlier builds printed alone.
pub fn version_line(name: &str, version: &str, commit: &str) -> String {
    format!("{name} {version} ({commit})")
}

/// The commit of a checkout and what Cargo has to watch to notice that it moved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    /// The first twelve digits of `git rev-parse HEAD`.
    pub commit: String,
    /// Files and directories that change whenever `HEAD` names another commit (see `git_head`).
    pub watch: Vec<PathBuf>,
}

/// One line of the output of `git args...` run with `dir` as its working directory, when git ran
/// and succeeded.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?;
    let line = text.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_string())
}

/// The commit `HEAD` names in the checkout that `dir` (a directory of the workspace, the crate's)
/// is in, run as `git rev-parse HEAD` with `dir` as the working directory. `None` when there is no
/// git program, no checkout, no commit yet, or when the checkout is not the workspace itself: its
/// top must be `root`, or a copy of the sources unpacked inside another repository would report
/// that repository's commit.
///
/// `watch` is what Cargo must name in `rerun-if-changed` for the build to run again when the
/// answer changes, whatever way git moved it. A directory is scanned by Cargo for any change
/// below it, and only paths that exist are listed (Cargo runs a script again at every build for a
/// path that is missing).
///
/// - `HEAD`: a checkout of another branch, a tag or a commit.
/// - `refs/heads` (the directory): a commit on a branch writes the loose file of the branch. This
///   is what catches a branch that was packed (`git pack-refs`, `git gc`): its commit is then in
///   `packed-refs`, a new commit writes a new loose file and leaves `packed-refs` alone, so the
///   file of the branch does not exist yet when the build looks, and a watch of that one file
///   (or of `packed-refs`) would miss it.
/// - `packed-refs`: a branch that is only packed moves by a pack, a fetch or `git update-ref`.
/// - `reftable`: the same for a repository with the reftable format, where refs are not files.
/// - `logs/HEAD`: the reflog of `HEAD`, which every commit, checkout and reset appends to, as a
///   second signal where the repository keeps one.
///
/// In a linked worktree `git rev-parse --git-path` names the worktree's own `HEAD` and reflog and
/// the refs of the repository they share.
pub fn git_head(dir: &Path, root: &Path) -> Option<Head> {
    let top = PathBuf::from(git(dir, &["rev-parse", "--show-toplevel"])?)
        .canonicalize()
        .ok()?;
    if top != root.canonicalize().ok()? {
        return None;
    }
    let commit = short(&git(dir, &["rev-parse", "HEAD"])?)?;
    let mut watch = Vec::new();
    for name in ["HEAD", "refs/heads", "packed-refs", "reftable", "logs/HEAD"] {
        let Some(path) = git(dir, &["rev-parse", "--git-path", name]) else {
            continue;
        };
        let path = dir.join(path);
        if path.exists() && !watch.contains(&path) {
            watch.push(path);
        }
    }
    Some(Head { commit, watch })
}
