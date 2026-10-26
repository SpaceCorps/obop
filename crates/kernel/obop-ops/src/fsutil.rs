//! Files of a backup: who may read them, how they appear, and what survives a crash (D73).
//!
//! A backup holds every record of the instance. Files and directories made here are readable by
//! their owner only (Unix modes 0600 and 0700). A file is written under a name of its own
//! (`<name>.<pid>-<n>.part`, created new, so two writers never share one), flushed to disk, and
//! only then given its final name; the directory is flushed after the rename so that the rename
//! itself survives a power loss. A final name is published without replacing: a second writer of
//! the same name is refused, whatever the order of events.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// The suffix of a file that is still being written.
pub const PART: &str = ".part";
/// A part file that nothing has written to for this long belongs to a run that died.
pub const STALE_PART: Duration = Duration::from_secs(6 * 3600);

/// The names of the part files of backups in `dir` (what a killed run left, or a copy that is
/// still going on), sorted. A directory that does not exist has none.
pub fn part_files(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| n.starts_with("obop-backup-") && n.ends_with(PART))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// A name for a partial file next to `target` that no other writer uses.
pub fn part_name(target: &Path) -> PathBuf {
    let mut s = target.as_os_str().to_os_string();
    s.push(format!(
        ".{}-{}{PART}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    PathBuf::from(s)
}

/// Make `dir` and its missing parents. On Unix the directories made here are 0700.
pub fn create_dir_private(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// Create a file that must not exist yet, for writing. On Unix it is 0600.
pub fn create_new_private(path: &Path) -> std::io::Result<File> {
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(path)
}

/// Flush a directory, so that a rename in it survives a crash. Where a directory cannot be opened
/// (Windows) the rename is as durable as the file system makes it.
pub fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    if let Ok(d) = File::open(if dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        dir
    }) {
        let _ = d.sync_all();
    }
    #[cfg(not(unix))]
    let _ = dir;
}

/// Give the finished `part` file the final name `to`, failing when `to` exists. A hard link is
/// made first, which fails if the name is taken, whoever took it a moment ago; where the file
/// system has no hard links the name is checked and the file renamed.
pub fn publish_new(part: &Path, to: &Path) -> Result<(), String> {
    match std::fs::hard_link(part, to) {
        Ok(()) => {
            let _ = std::fs::remove_file(part);
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(format!(
            "{} already exists; a backup is never overwritten",
            to.display()
        )),
        Err(_) => {
            if to.exists() {
                return Err(format!(
                    "{} already exists; a backup is never overwritten",
                    to.display()
                ));
            }
            std::fs::rename(part, to).map_err(|e| format!("cannot finish {}: {e}", to.display()))
        }
    }
}

/// Write `bytes` to `path` so that the file is either not there or complete: a part file of its
/// own, flushed, then renamed over `path` (which may exist: this replaces), then the directory
/// flushed. Private on Unix.
pub fn write_atomic_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let part = part_name(path);
    let written = (|| {
        let mut f = create_new_private(&part)?;
        f.write_all(bytes)?;
        f.sync_all()
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&part, path) {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    if let Some(dir) = path.parent() {
        sync_dir(dir);
    }
    Ok(())
}

/// Remove part files of backups in `dir` that have not been written to for `older_than`: what a
/// run that was killed left behind. Returns the names removed. Files that are not part files of a
/// backup are never touched.
pub fn sweep_parts(dir: &Path, older_than: Duration) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let now = SystemTime::now();
    let mut gone = Vec::new();
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !(name.starts_with("obop-backup-") && name.ends_with(PART)) {
            continue;
        }
        let idle = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok());
        if idle.is_some_and(|i| i >= older_than) && std::fs::remove_file(e.path()).is_ok() {
            gone.push(name);
        }
    }
    gone.sort();
    gone
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("obop-fsutil-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn part_names_are_unique_and_a_name_is_published_once() {
        let d = tmp("publish");
        let to = d.join("obop-backup-x.obbk");
        let (a, b) = (part_name(&to), part_name(&to));
        assert_ne!(a, b);
        assert!(a.to_string_lossy().ends_with(".part"));
        std::fs::write(&a, b"first").unwrap();
        std::fs::write(&b, b"second").unwrap();
        publish_new(&a, &to).unwrap();
        assert_eq!(std::fs::read(&to).unwrap(), b"first");
        assert!(!a.exists(), "the part file is gone once published");
        let e = publish_new(&b, &to).unwrap_err();
        assert!(e.contains("never overwritten"), "{e}");
        assert_eq!(std::fs::read(&to).unwrap(), b"first", "not replaced");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_atomic_write_replaces_whole_or_not_at_all_and_leaves_no_part_file() {
        let d = tmp("atomic");
        let f = d.join("m.json");
        write_atomic_private(&f, b"one").unwrap();
        write_atomic_private(&f, b"two").unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"two");
        let left: Vec<_> = std::fs::read_dir(&d)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(left, ["m.json"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn only_idle_part_files_of_backups_are_swept() {
        let d = tmp("sweep");
        std::fs::write(d.join("obop-backup-20260101T000000Z.obbk.1-0.part"), b"x").unwrap();
        std::fs::write(d.join("notes.part"), b"x").unwrap();
        std::fs::write(d.join("obop-backup-20260101T000000Z.obbk"), b"x").unwrap();
        // Just written: still in use.
        assert!(sweep_parts(&d, Duration::from_secs(3600)).is_empty());
        // Idle for longer than the limit: swept, and only the backup's part file.
        let gone = sweep_parts(&d, Duration::ZERO);
        assert_eq!(gone, ["obop-backup-20260101T000000Z.obbk.1-0.part"]);
        assert!(d.join("notes.part").exists());
        assert!(d.join("obop-backup-20260101T000000Z.obbk").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn files_and_directories_made_here_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let d = tmp("mode");
        let dir = d.join("a").join("b");
        create_dir_private(&dir).unwrap();
        for p in [d.join("a"), dir.clone()] {
            assert_eq!(
                std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
                0o700,
                "{}",
                p.display()
            );
        }
        let f = dir.join("backup.obbk");
        write_atomic_private(&f, b"x").unwrap();
        assert_eq!(
            std::fs::metadata(&f).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let part = dir.join("p.part");
        drop(create_new_private(&part).unwrap());
        assert_eq!(
            std::fs::metadata(&part).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}
