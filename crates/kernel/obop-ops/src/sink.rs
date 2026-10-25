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
        format!("fs:{}", self.dir.display())
    }

    fn put(&self, name: &str, src: &Path) -> Result<(), String> {
        let to = self.path(name)?;
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| format!("cannot create {}: {e}", self.dir.display()))?;
        let (want, _) = file_digest(src)?;
        if to.exists() {
            let (have, _) = file_digest(&to)?;
            return if have == want {
                Ok(())
            } else {
                Err(format!(
                    "{name} already exists in {} with different content; objects are write-once",
                    self.name()
                ))
            };
        }
        let part = self.dir.join(format!("{name}.part"));
        {
            let mut from =
                File::open(src).map_err(|e| format!("cannot read {}: {e}", src.display()))?;
            let mut out =
                File::create(&part).map_err(|e| format!("cannot write {}: {e}", part.display()))?;
            std::io::copy(&mut from, &mut out).map_err(|e| format!("copy failed: {e}"))?;
            out.flush().map_err(|e| format!("copy failed: {e}"))?;
            out.sync_all().map_err(|e| format!("copy failed: {e}"))?;
        }
        std::fs::rename(&part, &to).map_err(|e| format!("cannot finish {name}: {e}"))
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
