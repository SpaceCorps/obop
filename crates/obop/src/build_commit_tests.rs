//! The commit of `obop --version`: what counts as a commit, the line, and the git fallback of
//! `build.rs` (`build_commit::git_head`) against real repositories in temporary directories:
//! the commit it names, the checkout it accepts, and that what it asks Cargo to watch changes
//! whenever `HEAD` moves, also when the branch is packed and in a linked worktree.

use crate::build_commit::*;
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn a_hash_is_cut_to_twelve_lower_case_digits() {
    let full = "5EE5A96E1F2A9B7C0D3E4F5A6B7C8D9E0F1A2B3C";
    assert_eq!(short(full).as_deref(), Some("5ee5a96e1f2a"));
    assert_eq!(short(&full.to_lowercase()).as_deref(), Some("5ee5a96e1f2a"));
    // Exactly twelve is kept; a SHA-256 hash is cut like a SHA-1 one.
    assert_eq!(short("0123456789ab").as_deref(), Some("0123456789ab"));
    assert_eq!(short(&"f".repeat(64)).as_deref(), Some("ffffffffffff"));
    // The line a command prints around it does not matter.
    assert_eq!(
        short("  0123456789abcdef0123\n").as_deref(),
        Some("0123456789ab")
    );
}

#[test]
fn anything_that_is_not_a_hash_is_no_commit() {
    for bad in [
        "",
        "   ",
        "abc1234",
        "0123456789a",
        &"a".repeat(65),
        "0123456789abg",
        "0123456789ab-dirty",
        "unknown",
        "main",
        "0123456789ab\ncargo:rustc-env=X=1",
        "0123 456789ab",
    ] {
        assert_eq!(short(bad), None, "{bad:?}");
    }
}

#[test]
fn the_version_line_keeps_the_prefix_of_every_earlier_build() {
    let line = version_line("obop", "0.2.0", "5ee5a96e1f2a");
    assert_eq!(line, "obop 0.2.0 (5ee5a96e1f2a)");
    assert!(line.starts_with("obop 0.2.0"));
    assert_eq!(
        version_line("obop", "0.2.0", UNKNOWN),
        "obop 0.2.0 (unknown)"
    );
}

// ---- the git fallback ----------------------------------------------------------------------

/// A scratch directory, removed when the value is dropped (bind it to a name).
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let d =
            std::env::temp_dir().join(format!("obop-build-commit-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Scratch(d)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `git args...` in `dir`, cut off from the user's configuration, and its first line.
fn git(dir: &Path, args: &[&str]) -> String {
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let out = Command::new("git")
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", null)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .args([
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@example.test",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=",
        ])
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}

fn has_git() -> bool {
    let found = Command::new("git").arg("--version").output().is_ok();
    if !found {
        eprintln!("SKIPPED: this machine has no git program, so no repository can be made");
    }
    found
}

/// A repository whose workspace root has the crate directory `crates/obop` (git tracks files,
/// not directories, so the crate holds one) and one commit on `main`.
struct Repo {
    _scratch: Scratch,
    root: PathBuf,
}

impl Repo {
    fn new(tag: &str) -> Repo {
        let scratch = Scratch::new(tag);
        let root = scratch.0.join("repo");
        std::fs::create_dir_all(root.join("crates/obop")).unwrap();
        std::fs::write(root.join("crates/obop/keep.txt"), "x").unwrap();
        git(&root, &["init", "-q"]);
        git(&root, &["symbolic-ref", "HEAD", "refs/heads/main"]);
        git(&root, &["add", "."]);
        git(&root, &["commit", "-q", "-m", "first"]);
        Repo {
            _scratch: scratch,
            root,
        }
    }

    fn crate_dir(&self) -> PathBuf {
        self.root.join("crates/obop")
    }

    fn head(&self) -> Option<Head> {
        git_head(&self.crate_dir(), &self.root)
    }
}

/// What a change anywhere in these paths looks like: every file's name and bytes, a directory's
/// listing in order. Equal before and after means Cargo would not run the script again.
fn state(paths: &[PathBuf]) -> Vec<(String, Vec<u8>)> {
    fn walk(path: &Path, name: String, out: &mut Vec<(String, Vec<u8>)>) {
        if path.is_dir() {
            out.push((format!("{name}/"), Vec::new()));
            let mut kids: Vec<_> = std::fs::read_dir(path)
                .unwrap()
                .map(|e| e.unwrap().path())
                .collect();
            kids.sort();
            for kid in kids {
                let leaf = kid.file_name().unwrap().to_string_lossy().to_string();
                walk(&kid, format!("{name}/{leaf}"), out);
            }
        } else {
            out.push((name, std::fs::read(path).unwrap_or_default()));
        }
    }
    let mut out = Vec::new();
    for (i, p) in paths.iter().enumerate() {
        walk(p, format!("{i}"), &mut out);
    }
    out
}

/// Make `HEAD` name another commit with `how`, and check what the build needs to see: the commit
/// the fallback names is the commit `git rev-parse HEAD` names, before and after, and something
/// the first answer asked to watch is different afterwards.
fn the_build_notices(repo_dir: &Path, root: &Path, how: impl FnOnce()) {
    let before = git_head(repo_dir, root).expect("a checkout with a commit");
    assert_eq!(
        before.commit,
        short(&git(repo_dir, &["rev-parse", "HEAD"])).unwrap()
    );
    let seen = state(&before.watch);
    how();
    let now = short(&git(repo_dir, &["rev-parse", "HEAD"])).unwrap();
    assert_ne!(now, before.commit, "the test did not move HEAD");
    assert_ne!(
        state(&before.watch),
        seen,
        "HEAD moved from {} to {now} and nothing the build watches ({:?}) changed",
        before.commit,
        before.watch
    );
    assert_eq!(git_head(repo_dir, root).unwrap().commit, now);
}

fn commit(dir: &Path, message: &str) {
    git(dir, &["commit", "-q", "--allow-empty", "-m", message]);
}

#[test]
fn a_checkout_names_its_head_and_a_new_commit_is_noticed() {
    if !has_git() {
        return;
    }
    let repo = Repo::new("loose");
    let head = repo.head().unwrap();
    assert_eq!(head.commit.len(), 12);
    // HEAD, the branches and the reflog exist and are watched; a file that does not exist is not.
    let names: Vec<String> = head
        .watch
        .iter()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .collect();
    for want in ["/HEAD", "/refs/heads", "/logs/HEAD"] {
        assert!(
            names.iter().any(|n| n.ends_with(want)),
            "{want} in {names:?}"
        );
    }
    assert!(head.watch.iter().all(|p| p.exists()), "{:?}", head.watch);
    the_build_notices(&repo.crate_dir(), &repo.root, || {
        commit(&repo.root, "second")
    });
}

/// The case of a branch whose ref is only in `packed-refs` (after `git gc` or `git pack-refs`): a
/// new commit writes a loose file of the branch that did not exist when the build looked, and
/// leaves `packed-refs` as it was.
#[test]
fn a_new_commit_on_a_packed_branch_is_noticed() {
    if !has_git() {
        return;
    }
    let repo = Repo::new("packed");
    git(&repo.root, &["pack-refs", "--all", "--prune"]);
    assert!(
        !repo.root.join(".git/refs/heads/main").exists(),
        "the branch is only packed"
    );
    let packed = std::fs::read(repo.root.join(".git/packed-refs")).unwrap();
    the_build_notices(&repo.crate_dir(), &repo.root, || {
        commit(&repo.root, "second")
    });
    assert_eq!(
        std::fs::read(repo.root.join(".git/packed-refs")).unwrap(),
        packed,
        "a commit leaves packed-refs alone: that file is not what shows it"
    );
}

/// With no reflog the loose branch file and the listing of `refs/heads` are all there is.
#[test]
fn a_move_is_noticed_without_a_reflog_too() {
    if !has_git() {
        return;
    }
    let repo = Repo::new("noreflog");
    git(&repo.root, &["config", "core.logAllRefUpdates", "false"]);
    std::fs::remove_dir_all(repo.root.join(".git/logs")).unwrap();
    git(&repo.root, &["pack-refs", "--all", "--prune"]);
    let first = repo.head().unwrap();
    assert!(
        !first.watch.iter().any(|p| p.ends_with("logs/HEAD")),
        "{:?}",
        first.watch
    );
    let old = git(&repo.root, &["rev-parse", "HEAD"]);
    the_build_notices(&repo.crate_dir(), &repo.root, || {
        commit(&repo.root, "second")
    });
    // A branch moved back by hand, loose over packed.
    the_build_notices(&repo.crate_dir(), &repo.root, || {
        git(&repo.root, &["update-ref", "refs/heads/main", &old]);
    });
}

#[test]
fn a_checkout_of_another_branch_or_a_commit_is_noticed() {
    if !has_git() {
        return;
    }
    let repo = Repo::new("checkout");
    git(&repo.root, &["branch", "other"]);
    git(&repo.root, &["checkout", "-q", "other"]);
    commit(&repo.root, "on other");
    // Another branch: HEAD names another ref.
    the_build_notices(&repo.crate_dir(), &repo.root, || {
        git(&repo.root, &["checkout", "-q", "main"]);
    });
    // A detached HEAD: HEAD holds a hash.
    the_build_notices(&repo.crate_dir(), &repo.root, || {
        git(&repo.root, &["checkout", "-q", "--detach", "other"]);
    });
    // And another commit while detached.
    the_build_notices(&repo.crate_dir(), &repo.root, || {
        commit(&repo.root, "detached")
    });
}

/// A linked worktree: `.git` is a file, `HEAD` and the reflog are the worktree's own, the
/// branches are the repository's.
#[test]
fn a_linked_worktree_is_a_checkout_too() {
    if !has_git() {
        return;
    }
    let repo = Repo::new("worktree");
    let wt = repo._scratch.0.join("wt");
    git(
        &repo.root,
        &["worktree", "add", "-q", "-b", "wt", wt.to_str().unwrap()],
    );
    assert!(wt.join(".git").is_file(), ".git of a worktree is a file");
    let dir = wt.join("crates/obop");
    let head = git_head(&dir, &wt).expect("a worktree is a checkout");
    assert_eq!(head.commit, repo.head().unwrap().commit);
    the_build_notices(&dir, &wt, || commit(&wt, "in the worktree"));
    // The same with the branch packed away.
    git(&wt, &["pack-refs", "--all", "--prune"]);
    the_build_notices(&dir, &wt, || commit(&wt, "again, packed"));
    // The main checkout did not move.
    assert_ne!(
        repo.head().unwrap().commit,
        git_head(&dir, &wt).unwrap().commit
    );
}

#[test]
fn what_is_not_the_checkout_of_the_workspace_has_no_commit() {
    if !has_git() {
        return;
    }
    let repo = Repo::new("notroot");
    // The sources are a subdirectory of some other repository: its commit is not ours.
    assert_eq!(git_head(&repo.crate_dir(), &repo.root.join("crates")), None);
    assert_eq!(git_head(&repo.crate_dir(), &repo.crate_dir()), None);
    // A repository with no commit yet.
    let scratch = Scratch::new("unborn");
    let root = scratch.0.join("repo");
    std::fs::create_dir_all(root.join("crates/obop")).unwrap();
    git(&root, &["init", "-q"]);
    assert_eq!(git_head(&root.join("crates/obop"), &root), None);
    // A tree with no repository at all (a copy of the sources, as in the image build).
    let plain = Scratch::new("plain");
    std::fs::create_dir_all(plain.0.join("crates/obop")).unwrap();
    assert_eq!(git_head(&plain.0.join("crates/obop"), &plain.0), None);
}
