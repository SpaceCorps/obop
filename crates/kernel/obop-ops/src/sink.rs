//! Where a backup is copied to leave the machine (OBR-DAT-047 item 1, D73): the `BackupSink` port.
//!
//! A backup that sits on the database host is lost with the database host. A sink takes the data
//! file and its manifest to a place in another failure domain, and a copy only counts once it has
//! been **read back and its hash matched** ([`copy_verified`]). The port is deliberately small
//! (put, hash of what is there, get, list, delete) so that an object store is one more
//! implementation, as the blob store of D60 is.
//!
//! Shipped: [`FsSink`], a directory: a mounted network share, a second disk, an `rclone mount`
//! of an object store. Not shipped: an S3-compatible sink (docs/DEPLOY.md, "Backups off the
//! host"): a signed multipart upload client to write and test against a real store is a project of
//! its own, and an operator-run uploader (`aws s3 cp`, `rclone`) with the object lock and
//! versioning of the bucket does the same job today; `FsSink` over a mount is how obop
//! verifies such a copy by reading it back.
//!
//! Objects are write-once: `put` refuses to replace an object with different content, the way an
//! object lock does. Deleting is a separate permission (`FsSink::allow_delete`): the credential
//! that writes backups should not be able to remove them, and expiry belongs to the store's
//! lifecycle rule.

use crate::fsutil;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub trait BackupSink: Send + Sync {
    /// A short name for messages and the catalogue, such as `fs:/mnt/offsite`.
    fn name(&self) -> String;
    /// Store the file at `src` as object `name`. Refuses to replace an object that differs.
    fn put(&self, name: &str, src: &Path) -> Result<(), String>;
    /// SHA-256 (hex) and size of the object as the sink holds it, read back.
    fn digest(&self, name: &str) -> Result<(String, u64), String>;
    /// Fetch object `name` to `dst` (a drill restores from the copy alone).
    fn get(&self, name: &str, dst: &Path) -> Result<(), String>;
    /// Names of the objects, sorted.
    fn list(&self) -> Result<Vec<String>, String>;
    /// Remove an object. Sinks whose credential may not delete refuse.
    fn delete(&self, name: &str) -> Result<(), String>;
}

fn safe_name(name: &str) -> Result<(), String> {
    let ok = !name.is_empty()
        && name.len() <= 200
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if ok {
        Ok(())
    } else {
        Err(format!("{name:?} is not a safe object name"))
    }
}

pub fn file_digest(path: &Path) -> Result<(String, u64), String> {
    let mut f = File::open(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let (mut h, mut buf, mut n) = (Sha256::new(), vec![0u8; 1 << 16], 0u64);
    loop {
        let k = f.read(&mut buf).map_err(|e| format!("read failed: {e}"))?;
        if k == 0 {
            break;
        }
        h.update(&buf[..k]);
        n += k as u64;
    }
    Ok((obop_keys::hex(&h.finalize()), n))
}

/// An object that is already there is a success only when it holds the bytes being put.
fn same_content(to: &Path, want: &str, name: &str, sink: &str) -> Result<(), String> {
    let (have, _) = file_digest(to)?;
    if have == *want {
        Ok(())
    } else {
        Err(format!(
            "{name} already exists in {sink} with different content; objects are write-once"
        ))
    }
}

/// Whether copying `src` to `to` (in `dir`) would copy a file onto itself: the same directory,
/// or the same file reached by another name (a symlink, a bind mount, another spelling).
fn is_same_place(src: &Path, to: &Path, dir: &Path) -> bool {
    let canon = |p: &Path| std::fs::canonicalize(p).ok();
    if let (Some(s), Some(t)) = (canon(src), canon(to))
        && s == t
    {
        return true;
    }
    match (src.parent().and_then(canon), canon(dir)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// A directory as a sink.
pub struct FsSink {
    dir: PathBuf,
    allow_delete: bool,
}

impl FsSink {
    pub fn new(dir: impl Into<PathBuf>) -> FsSink {
        FsSink {
            dir: dir.into(),
            allow_delete: false,
        }
    }

    /// Let `delete` remove objects. Off by default: expiry is the store's lifecycle rule.
    pub fn allow_delete(mut self, yes: bool) -> FsSink {
        self.allow_delete = yes;
        self
    }

    fn path(&self, name: &str) -> Result<PathBuf, String> {
        safe_name(name)?;
        Ok(self.dir.join(name))
    }
}

impl BackupSink for FsSink {
    fn name(&self) -> String {
        // The same sink under any spelling (a trailing slash, a relative path) has one name, so
        // that a record of a copy keeps meaning the sink it was made in.
        let dir = std::path::absolute(&self.dir).unwrap_or_else(|_| self.dir.clone());
        let dir: PathBuf = dir.components().collect();
        format!("fs:{}", dir.display())
    }

    fn put(&self, name: &str, src: &Path) -> Result<(), String> {
        let to = self.path(name)?;
        fsutil::create_dir_private(&self.dir)
            .map_err(|e| format!("cannot create {}: {e}", self.dir.display()))?;
        // A sink in the directory the backup is already in is not a second copy: reading it
        // back would prove nothing, and retention would then delete the only one.
        if is_same_place(src, &to, &self.dir) {
            return Err(format!(
                "{} is the directory the backup is in: that is not a copy anywhere else. Point the off-host copy at another disk, share or mount",
                self.name()
            ));
        }
        let (want, _) = file_digest(src)?;
        // Write-once is decided at the moment of publishing (below); this is only the fast path
        // for a retry that finds its own earlier copy.
        if to.exists() {
            return same_content(&to, &want, name, &self.name());
        }
        let part = fsutil::part_name(&to);
        let copied = (|| -> Result<(), String> {
            let mut from =
                File::open(src).map_err(|e| format!("cannot read {}: {e}", src.display()))?;
            let mut out = fsutil::create_new_private(&part)
                .map_err(|e| format!("cannot write {}: {e}", part.display()))?;
            std::io::copy(&mut from, &mut out).map_err(|e| format!("copy failed: {e}"))?;
            out.flush().map_err(|e| format!("copy failed: {e}"))?;
            out.sync_all().map_err(|e| format!("copy failed: {e}"))
        })();
        if let Err(e) = copied {
            let _ = std::fs::remove_file(&part);
            return Err(e);
        }
        // Publish without replacing: if another writer got the name first, its object stays and
        // this copy is only a success when the two are the same bytes.
        match fsutil::publish_new(&part, &to) {
            Ok(()) => {
                fsutil::sync_dir(&self.dir);
                Ok(())
            }
            Err(e) => {
                let _ = std::fs::remove_file(&part);
                if to.exists() {
                    same_content(&to, &want, name, &self.name())
                } else {
                    Err(e)
                }
            }
        }
    }

    fn digest(&self, name: &str) -> Result<(String, u64), String> {
        file_digest(&self.path(name)?)
    }

    fn get(&self, name: &str, dst: &Path) -> Result<(), String> {
        let from = self.path(name)?;
        std::fs::copy(&from, dst)
            .map(|_| ())
            .map_err(|e| format!("cannot fetch {name} from {}: {e}", self.name()))
    }

    fn list(&self) -> Result<Vec<String>, String> {
        let rd = match std::fs::read_dir(&self.dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(format!("cannot list {}: {e}", self.dir.display())),
        };
        let mut v: Vec<String> = rd
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| !n.ends_with(".part"))
            .collect();
        v.sort();
        Ok(v)
    }

    fn delete(&self, name: &str) -> Result<(), String> {
        if !self.allow_delete {
            return Err(format!(
                "{} does not allow deleting: expire copies with the store's lifecycle rule",
                self.name()
            ));
        }
        std::fs::remove_file(self.path(name)?).map_err(|e| format!("cannot delete {name}: {e}"))
    }
}

/// Copy `src` to the sink as `name` and prove the copy: read it back and compare its hash and size
/// with the local file. Returns the hash.
pub fn copy_verified(sink: &dyn BackupSink, name: &str, src: &Path) -> Result<String, String> {
    let (want, bytes) = file_digest(src)?;
    sink.put(name, src)?;
    let (got, got_bytes) = sink.digest(name)?;
    if got != want || got_bytes != bytes {
        return Err(format!(
            "the copy of {name} in {} does not read back as it was written: not a backup yet",
            sink.name()
        ));
    }
    Ok(want)
}

/// A sink whose reads can be made to fail or lie, for tests of the code around the port.
#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct MemorySink {
        pub objects: Mutex<BTreeMap<String, Vec<u8>>>,
        /// `digest` reports this object as damaged.
        pub corrupt_on_read: Mutex<Option<String>>,
        pub fail_put: Mutex<bool>,
        pub deletable: bool,
    }

    impl BackupSink for MemorySink {
        fn name(&self) -> String {
            "memory".into()
        }
        fn put(&self, name: &str, src: &Path) -> Result<(), String> {
            if *self.fail_put.lock().unwrap() {
                return Err("the sink is unreachable".into());
            }
            let bytes = std::fs::read(src).map_err(|e| e.to_string())?;
            let mut o = self.objects.lock().unwrap();
            if let Some(old) = o.get(name)
                && old != &bytes
            {
                return Err("write-once".into());
            }
            o.insert(name.to_string(), bytes);
            Ok(())
        }
        fn digest(&self, name: &str) -> Result<(String, u64), String> {
            let o = self.objects.lock().unwrap();
            let b = o.get(name).ok_or("no such object")?;
            let mut b = b.clone();
            if self.corrupt_on_read.lock().unwrap().as_deref() == Some(name) {
                b[0] ^= 1;
            }
            Ok((obop_keys::hex(&Sha256::digest(&b)), b.len() as u64))
        }
        fn get(&self, name: &str, dst: &Path) -> Result<(), String> {
            let o = self.objects.lock().unwrap();
            std::fs::write(dst, o.get(name).ok_or("no such object")?).map_err(|e| e.to_string())
        }
        fn list(&self) -> Result<Vec<String>, String> {
            Ok(self.objects.lock().unwrap().keys().cloned().collect())
        }
        fn delete(&self, name: &str) -> Result<(), String> {
            if !self.deletable {
                return Err("no delete permission".into());
            }
            self.objects.lock().unwrap().remove(name);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::MemorySink;
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("obop-sink-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_copy_is_read_back_and_compared_before_it_counts() {
        let d = tmp("copy");
        let src = d.join("backup.obbk");
        std::fs::write(&src, b"sealed bytes").unwrap();
        let sink = FsSink::new(d.join("offsite"));
        let h = copy_verified(&sink, "backup.obbk", &src).unwrap();
        assert_eq!(h, file_digest(&src).unwrap().0);
        assert_eq!(sink.list().unwrap(), ["backup.obbk"]);
        // Putting the same bytes again is fine (a retry); different bytes are refused.
        sink.put("backup.obbk", &src).unwrap();
        let other = d.join("other");
        std::fs::write(&other, b"different").unwrap();
        let e = sink.put("backup.obbk", &other).unwrap_err();
        assert!(e.contains("write-once"), "{e}");
        // Fetching it back gives the same file.
        let back = d.join("back");
        sink.get("backup.obbk", &back).unwrap();
        assert_eq!(std::fs::read(back).unwrap(), b"sealed bytes");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_copy_that_does_not_read_back_identically_is_not_a_backup() {
        let d = tmp("bad");
        let src = d.join("b");
        std::fs::write(&src, b"abc").unwrap();
        let sink = MemorySink::default();
        *sink.corrupt_on_read.lock().unwrap() = Some("b".into());
        let e = copy_verified(&sink, "b", &src).unwrap_err();
        assert!(e.contains("does not read back"), "{e}");
        *sink.fail_put.lock().unwrap() = true;
        assert!(
            copy_verified(&sink, "c", &src)
                .unwrap_err()
                .contains("unreachable")
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// "Read back and hash-checked" proves a copy only if the copy is somewhere else: a sink in the
    /// directory of the backup (or reached by another name) would read back the file itself.
    #[test]
    fn a_sink_in_the_directory_of_the_backup_is_not_a_second_copy() {
        let d = tmp("same");
        let src = d.join("backup.obbk");
        std::fs::write(&src, b"sealed bytes").unwrap();
        // The same directory, spelled three ways.
        for spelled in [
            d.clone(),
            d.join("."),
            d.join("sub").join(".."),
            PathBuf::from(format!("{}/", d.display())),
        ] {
            std::fs::create_dir_all(d.join("sub")).unwrap();
            let sink = FsSink::new(spelled.clone());
            let e = copy_verified(&sink, "backup.obbk", &src).unwrap_err();
            assert!(
                e.contains("is the directory the backup is in"),
                "{spelled:?}: {e}"
            );
        }
        // Nothing was copied, and nothing was damaged.
        assert_eq!(std::fs::read(&src).unwrap(), b"sealed bytes");
        // Another directory is a copy, and one name has one sink name however it is written.
        let a = FsSink::new(d.join("off"));
        let b = FsSink::new(format!("{}/", d.join("off").display()));
        assert_eq!(a.name(), b.name());
        copy_verified(&a, "backup.obbk", &src).unwrap();
        assert_eq!(a.list().unwrap(), ["backup.obbk"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Two writers of one object name with different content: exactly one object results, it is
    /// one of the two (never a mixture), and the loser is told, not told it succeeded.
    #[test]
    fn two_puts_of_one_name_never_mix_and_the_loser_is_refused() {
        let d = tmp("race");
        let (sa, sb) = (d.join("a"), d.join("b"));
        std::fs::write(&sa, vec![0xAAu8; 4 << 20]).unwrap();
        std::fs::write(&sb, vec![0xBBu8; 4 << 20]).unwrap();
        for round in 0..6 {
            let sink = std::sync::Arc::new(FsSink::new(d.join(format!("off{round}"))));
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
            let results: Vec<Result<(), String>> = std::thread::scope(|s| {
                [&sa, &sb]
                    .into_iter()
                    .map(|src| {
                        let (sink, barrier) = (sink.clone(), barrier.clone());
                        s.spawn(move || {
                            barrier.wait();
                            sink.put("same.obbk", src)
                        })
                    })
                    .collect::<Vec<_>>()
                    .into_iter()
                    .map(|h| h.join().unwrap())
                    .collect()
            });
            let (da, db) = (file_digest(&sa).unwrap().0, file_digest(&sb).unwrap().0);
            let got = sink.digest("same.obbk").unwrap().0;
            assert!(got == da || got == db, "round {round}: a mixed object");
            assert_eq!(
                results.iter().filter(|r| r.is_ok()).count(),
                1,
                "round {round}: {results:?}"
            );
            let winner = if results[0].is_ok() { &da } else { &db };
            assert_eq!(
                &got, winner,
                "round {round}: the Ok put is the object there"
            );
            let loser = results.iter().find_map(|r| r.as_ref().err()).unwrap();
            assert!(loser.contains("write-once"), "{loser}");
            // No part file is left, and the object is still the winner's after a retry of it.
            assert_eq!(sink.list().unwrap(), ["same.obbk"]);
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn deleting_is_a_separate_permission_and_names_are_safe() {
        let d = tmp("del");
        let src = d.join("b");
        std::fs::write(&src, b"abc").unwrap();
        let sink = FsSink::new(d.join("o"));
        sink.put("b", &src).unwrap();
        assert!(sink.delete("b").unwrap_err().contains("lifecycle"));
        assert_eq!(sink.list().unwrap(), ["b"]);
        let sink = FsSink::new(d.join("o")).allow_delete(true);
        sink.delete("b").unwrap();
        assert!(sink.list().unwrap().is_empty());
        for bad in ["", "../x", "a/b", ".hidden", "a b", "x\\y"] {
            assert!(sink.put(bad, &src).is_err(), "{bad:?}");
            assert!(sink.digest(bad).is_err(), "{bad:?}");
        }
        assert!(
            FsSink::new(d.join("nothing-yet"))
                .list()
                .unwrap()
                .is_empty()
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}
