//! Tells the compiler which commit this build is made from, as `OBOP_BUILD_COMMIT`, for
//! `obop --version`, and where it came from, as `OBOP_BUILD_COMMIT_SOURCE` (`env`, `git` or
//! `none`; `tests/version.rs` reads it). The commit is, in this order:
//!
//! 1. the environment variable `OBOP_BUILD_COMMIT`, a hash of twelve to 64 hexadecimal digits: the
//!    Docker build sets it from its build argument, because the image has no `.git`. A value that
//!    is no hash is ignored with a warning (it is not echoed) and the next step decides;
//! 2. `git rev-parse HEAD`, run in this crate's directory, when git is installed and the sources
//!    are the top of their own checkout (not some other repository that contains a copy of them);
//! 3. `unknown`.
//!
//! It never fails the build and adds no dependency.
//!
//! **When it runs again.** Cargo runs a build script again only when a path named by
//! `rerun-if-changed`, or an environment variable named by `rerun-if-env-changed`, changed. This
//! script names its own files, `OBOP_BUILD_COMMIT`, and (in a checkout) the places where git
//! records what `HEAD` is: `HEAD`, the directory `refs/heads`, `packed-refs`, `reftable` and the
//! reflog `logs/HEAD`, those that exist (`build_commit::git_head` says why each is there). A commit,
//! a checkout, a reset, a pull, `git pack-refs` and a linked worktree all change at least one of
//! them. What it does not see: edits that are not committed (there is no `-dirty` mark for that
//! reason: it would go stale at the first edit), and a source tree that becomes a checkout later
//! or whose git was missing at the first build, which keeps `unknown` until this file or the
//! variable changes (`touch crates/obop/build.rs`).

#[path = "src/build_commit.rs"]
mod build_commit;

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/build_commit.rs");
    println!("cargo:rerun-if-env-changed=OBOP_BUILD_COMMIT");
    let (commit, source) = if let Some(commit) = from_env() {
        (commit, "env")
    } else if let Some(head) = from_git() {
        for path in &head.watch {
            println!("cargo:rerun-if-changed={}", path.display());
        }
        (head.commit, "git")
    } else {
        (build_commit::UNKNOWN.to_string(), "none")
    };
    println!("cargo:rustc-env=OBOP_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=OBOP_BUILD_COMMIT_SOURCE={source}");
}

fn from_env() -> Option<String> {
    let raw = std::env::var("OBOP_BUILD_COMMIT").ok()?;
    if raw.trim().is_empty() {
        return None;
    }
    let commit = build_commit::short(&raw);
    if commit.is_none() {
        // The value is not echoed: it is not ours to print into the build output.
        println!(
            "cargo:warning=OBOP_BUILD_COMMIT is not a commit hash (twelve to 64 hexadecimal digits) and is ignored"
        );
    }
    commit
}

fn from_git() -> Option<build_commit::Head> {
    let dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR")?);
    let root = dir.join("../..");
    build_commit::git_head(&dir, &root)
}
