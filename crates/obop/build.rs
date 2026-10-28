//! Tells the compiler which commit this build is made from, as `OBOP_BUILD_COMMIT`, for
//! `obop --version`. The commit is, in this order:
//!
//! 1. the environment variable `OBOP_BUILD_COMMIT`, a hash of twelve or more hexadecimal digits:
//!    the Docker build sets it from its build argument, because the image has no `.git`;
//! 2. `git rev-parse HEAD`, when this crate is built in a checkout of the obop workspace itself
//!    (not in some other repository that happens to contain a copy of it);
//! 3. `unknown`.
//!
//! It never fails the build: no git program, no `.git`, an unreadable repository and a value that
//! is no hash all end in the next step. It adds no `-dirty` mark: Cargo runs a build script again
//! only when a file it names changes, so a mark for the state of the working tree would go stale
//! at the first edit, which is worse than none.

#[path = "src/build_commit.rs"]
mod build_commit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/build_commit.rs");
    println!("cargo:rerun-if-env-changed=OBOP_BUILD_COMMIT");
    let commit = from_env()
        .or_else(from_git)
        .unwrap_or_else(|| build_commit::UNKNOWN.to_string());
    println!("cargo:rustc-env=OBOP_BUILD_COMMIT={commit}");
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

/// One line of the output of `git -C dir args...`, when git ran and succeeded.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
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

fn from_git() -> Option<String> {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR")?);
    let root = manifest.join("../..").canonicalize().ok()?;
    // The repository must be this workspace: a copy of the sources unpacked inside another
    // repository would report that repository's commit.
    let top = PathBuf::from(git(&root, &["rev-parse", "--show-toplevel"])?)
        .canonicalize()
        .ok()?;
    if top != root {
        return None;
    }
    let head = git(&root, &["rev-parse", "HEAD"])?;
    // Build again when the commit changes: HEAD itself (a checkout), the branch it names (a new
    // commit) and, once the branch has been packed, the list of packed references.
    let mut watched = vec![git(&root, &["rev-parse", "--git-path", "HEAD"])];
    if let Some(branch) = git(&root, &["symbolic-ref", "-q", "HEAD"]) {
        watched.push(git(&root, &["rev-parse", "--git-path", &branch]));
    }
    watched.push(git(&root, &["rev-parse", "--git-path", "packed-refs"]));
    for path in watched.into_iter().flatten() {
        let path = root.join(path);
        if path.exists() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    build_commit::short(&head)
}
