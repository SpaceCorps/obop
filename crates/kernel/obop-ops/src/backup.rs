//! `obop backup`, `obop backup verify` and `obop restore` (B4, D59, D73): a consistent logical
//! export of every record, and the way back.
//!
//! The data file is gzip-compressed JSON Lines, written while the database is read from one
//! snapshot (a repeatable-read transaction on PostgreSQL, a read transaction on SQLite), a batch
//! at a time, so a large database never sits in memory and a running server is not stopped:
//!
//! ```text
//! {"type":"header","format":1,"core_version":"0.2.0","schema_version":2,"created_at":"...","source":"postgres"}
//! {"m":"<model>","i":"<id>","d":{...fields...}}      one line per record, in insertion order
//! {"blob":"<sha256 id>","ct":"image/png","w":640,"h":480,"created":1,"x":"<base64 bytes>"}
//! {"type":"end","records":N,"blobs":M,"content_sha256":"..."}
//! ```
//!
//! Uploaded pictures and files (D60) keep their records (what holds them) with the other
//! records and their bytes in a directory or a table of their own, so the bytes follow the
//! records as `blob` lines, one blob at a time, through `Blobs::export`. A restore puts them
//! back through `Blobs::import`, which checks that each hashes to its id.
//!
//! Next to it, `<data file>.manifest.json` repeats the header and adds the SHA-256 of the data
//! file as stored, the SHA-256 of the record lines, the record count and the count per model.
//! Secrets stay sealed exactly as stored. Short-lived values (sessions, pending approvals,
//! rate-limit counters) are not backed up; people sign in again after a restore and agents
//! re-propose pending approvals.
//!
//! **Format 1** (this layout, plain) detects damage and accidental edits with its checksums; it
//! does not authenticate and it does not hide the records: keep it where only the operator can
//! read and write.
//!
//! **Format 2** (D73, OBR-DAT-046) is the same stream, gzip first, sealed in chunks with
//! ChaCha20-Poly1305 (`obop_keys::stream`) under a random per-backup data key that the header
//! carries wrapped by a key of the backup key ring (`OBOP_BACKUP_KEY`, else the server key).
//! The manifest repeats the header's facts and adds the number of chunks, the head of a hash chain
//! over the chunk tags and an HMAC-SHA256 over itself under a subkey of the same key. A flipped
//! bit, a cut or extended file, a reordered chunk, an edited manifest, or a manifest and data file
//! both rewritten without the key, all fail `verify`; without the key nothing opens
//! (`restore_key_required`) and nothing is written. Format 1 backups keep restoring.

use crate::fsutil;
use base64::Engine as _;
use obop_keys::stream::{self, OpenReader, SealWriter};
use obop_keys::{KeyRing, hex, random, unhex};
use obop_storage::blob::{BlobInfo, Blobs, ExportedBlob};
use obop_storage::{Backend, Fields, Row};
use serde_json::{Value as J, json};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

/// The plain layout.
pub const FORMAT: u64 = 1;
/// The sealed layout (D73).
pub const FORMAT_SEALED: u64 = 2;
/// The stable code of the error for a sealed backup whose key is not configured.
pub const KEY_REQUIRED: &str = "restore_key_required";
/// The stable code of the error for a plain backup offered to an instance that seals its own.
pub const NOT_SEALED: &str = "backup_not_sealed";
/// The stable code of the error for a restore whose records were restored but whose uploaded
/// files were not all written.
pub const FILES_FAILED: &str = "restore_files_failed";
const BATCH: usize = 1000;
const CIPHER: &str = "chacha20poly1305-stream-v1";
const MAC_PURPOSE: &str = "backup-manifest";

/// The stable code named anywhere in a message (a gate lists the failing checks, each with the
/// error of its own), if there is one.
pub fn code_in(e: &str) -> Option<&'static str> {
    [KEY_REQUIRED, NOT_SEALED, FILES_FAILED]
        .into_iter()
        .find(|c| e.contains(c))
}

/// The code at the start of an error of this module (`restore_key_required: ...`), if it has one.
pub fn error_code(e: &str) -> Option<&'static str> {
    [KEY_REQUIRED, NOT_SEALED, FILES_FAILED]
        .into_iter()
        .find(|c| e.starts_with(c))
}

fn hexs(b: &[u8]) -> String {
    hex(b)
}

/// `YYYY-MM-DDTHH:MM:SSZ` for a Unix time.
pub fn rfc3339(secs: u64) -> String {
    let (days, rem) = ((secs / 86400) as i64, secs % 86400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Seconds since the epoch of an `rfc3339` string of this module's form.
pub fn parse_rfc3339(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    if b.len() != 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[19] != b'Z' {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, m, d) = (n(0..4)?, n(5..7)?, n(8..10)?);
    let (h, mi, se) = (n(11..13)?, n(14..16)?, n(17..19)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || h > 23 || mi > 59 || se > 60 {
        return None;
    }
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2.rem_euclid(400);
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86400 + h * 3600 + mi * 60 + se).ok()
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Counts bytes and hashes them on their way to disk.
struct Tee<W: Write> {
    inner: W,
    hash: Sha256,
    bytes: u64,
}

impl<W: Write> Write for Tee<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hash.update(&buf[..n]);
        self.bytes += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Where the gzip stream goes: straight to the file, or through the chunked cipher.
enum Sink<W: Write> {
    Plain(Tee<W>),
    Sealed(SealWriter<Tee<W>>),
}

impl<W: Write> Write for Sink<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Sink::Plain(t) => t.write(buf),
            Sink::Sealed(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Sink::Plain(t) => t.flush(),
            Sink::Sealed(s) => s.flush(),
        }
    }
}

impl<W: Write> Sink<W> {
    fn finish(self) -> std::io::Result<(Tee<W>, Option<stream::Sealed>)> {
        match self {
            Sink::Plain(t) => Ok((t, None)),
            Sink::Sealed(s) => s.finish().map(|(t, sealed)| (t, Some(sealed))),
        }
    }
}

/// The keys a backup is sealed with and opened with.
#[derive(Clone, Default)]
pub struct Keys {
    ring: KeyRing,
    from: KeysFrom,
    /// New backups are sealed with the ring's current key. False for a ring that only opens
    /// (`OBOP_BACKUP_KEY_PREVIOUS` without any current key).
    seal: bool,
    /// A plain (format 1) backup is accepted although this instance seals its own: an old backup
    /// from before a key was set, which the operator vouches for (`--allow-plain`).
    allow_plain: bool,
}

/// Where the sealing key comes from, for messages.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum KeysFrom {
    #[default]
    Nowhere,
    /// `OBOP_BACKUP_KEY`: a key of its own, which can be kept apart from the server's.
    BackupKey,
    /// The server key `OBOP_SECRET_KEY`: the backup is then as readable as the sealed secrets.
    ServerKey,
}

impl Keys {
    /// No key: format 1 on write, a refusal of any sealed backup on read.
    pub fn none() -> Keys {
        Keys::default()
    }

    /// A ring of the operator's own (tests, tooling).
    pub fn from_ring(ring: KeyRing) -> Keys {
        let from = if ring.is_empty() {
            KeysFrom::Nowhere
        } else {
            KeysFrom::BackupKey
        };
        Keys {
            seal: !ring.is_empty(),
            ring,
            from,
            allow_plain: false,
        }
    }

    /// Accept plain (format 1) backups although a key is configured (see the field).
    pub fn allowing_plain(mut self, yes: bool) -> Keys {
        self.allow_plain = yes;
        self
    }

    /// The keys of this process's environment: `OBOP_BACKUP_KEY` (and `OBOP_BACKUP_KEY_PREVIOUS`)
    /// when set, then the server key ring, but only the keys the operator provides: a key that
    /// was generated into the database is inside the backup, so a backup sealed with it could
    /// never be opened after the database is gone.
    pub fn from_env(server: &KeyRing) -> Result<Keys, String> {
        let var = |n: &str| std::env::var(n).ok().filter(|v| !v.trim().is_empty());
        Self::from_vars(
            var("OBOP_BACKUP_KEY").as_deref(),
            var("OBOP_BACKUP_KEY_PREVIOUS").as_deref(),
            server,
        )
    }

    /// As [`Keys::from_env`], with the two variables given.
    pub fn from_vars(
        current: Option<&str>,
        previous: Option<&str>,
        server: &KeyRing,
    ) -> Result<Keys, String> {
        let server = server.environment_only();
        let previous = previous.unwrap_or_default();
        let prev: Vec<&str> = previous
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect();
        let Some(current) = current.map(str::trim).filter(|c| !c.is_empty()) else {
            // No key of its own: the server key seals. Earlier backup keys that are still listed
            // open what they sealed, behind the server's keys.
            let from = if server.is_empty() {
                KeysFrom::Nowhere
            } else {
                KeysFrom::ServerKey
            };
            let old = match prev.split_first() {
                None => KeyRing::default(),
                Some((first, rest)) => KeyRing::from_secrets(first, rest)
                    .map_err(|e| format!("OBOP_BACKUP_KEY_PREVIOUS: {e}"))?,
            };
            return Ok(Keys {
                seal: !server.is_empty(),
                ring: server.merged(&old),
                from,
                allow_plain: false,
            });
        };
        let own = KeyRing::from_secrets(current, &prev)
            .map_err(|e| format!("OBOP_BACKUP_KEY or OBOP_BACKUP_KEY_PREVIOUS: {e}"))?;
        // Older backups were sealed with the server key before a key of its own existed.
        Ok(Keys {
            ring: own.merged(&server),
            from: KeysFrom::BackupKey,
            seal: true,
            allow_plain: false,
        })
    }

    /// Whether a new backup can be sealed.
    pub fn can_seal(&self) -> bool {
        self.seal && self.ring.current().is_some()
    }

    /// Whether a plain backup is refused: this instance seals its own, so a plain one cannot be
    /// told from a forgery by anyone who can write the backup directory.
    pub fn refuses_plain(&self) -> bool {
        self.can_seal() && !self.allow_plain
    }

    pub fn from(&self) -> KeysFrom {
        self.from
    }

    /// The id of the key a new backup is sealed with.
    pub fn current_id(&self) -> Option<String> {
        self.ring.current().map(|k| k.id().to_string())
    }

    pub fn describe(&self) -> &'static str {
        match self.from {
            KeysFrom::Nowhere => "no key",
            KeysFrom::BackupKey => "OBOP_BACKUP_KEY",
            KeysFrom::ServerKey => "the server key (OBOP_SECRET_KEY)",
        }
    }
}

/// What a format 2 manifest says about the sealing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encryption {
    pub cipher: String,
    /// The id of the key that wrapped the data key and made the manifest's MAC.
    pub key_id: String,
    pub chunk_size: usize,
    pub chunks: u64,
    /// Head of the hash chain over the chunk tags (hex).
    pub chain_head: String,
}

#[derive(Debug, Clone)]
pub struct Manifest {
    pub format: u64,
    pub core_version: String,
    pub schema_version: u64,
    pub created_at: String,
    pub source: String,
    pub data_file: String,
    pub data_sha256: String,
    pub data_bytes: u64,
    pub content_sha256: String,
    /// SHA-256 over the record lines alone (without the uploaded files); absent in backups made
    /// before D73. A restore drill compares the restored database with it.
    pub records_sha256: Option<String>,
    pub records: usize,
    pub models: BTreeMap<String, usize>,
    /// Uploaded blobs whose bytes are in the data file.
    pub blobs: usize,
    /// Uploaded blobs whose stored bytes no longer hash to their id when the backup was made:
    /// they are left out, because a restore could not write them (at most 50 ids are listed).
    pub blobs_damaged: Vec<String>,
    /// The name of the database the backup was taken from (`obop_meta/instance`), if it has one.
    /// A migration gate refuses a backup that names another database.
    pub database_id: Option<String>,
    /// Format 2: random id of this backup (the data key is wrapped for it).
    pub backup_id: Option<String>,
    pub encryption: Option<Encryption>,
    /// Format 2: HMAC-SHA256 of the rest of the manifest (hex).
    pub mac: Option<String>,
}

impl Manifest {
    /// The manifest as JSON, without its MAC.
    fn to_json_unsigned(&self) -> J {
        let mut j = json!({
            "kind": "obop-backup",
            "format": self.format,
            "core_version": self.core_version,
            "schema_version": self.schema_version,
            "created_at": self.created_at,
            "source": self.source,
            "data_file": self.data_file,
            "data_sha256": self.data_sha256,
            "data_bytes": self.data_bytes,
            "content_sha256": self.content_sha256,
            "records": self.records,
            "models": self.models,
            "blobs": self.blobs,
            "not_included": ["short-lived values: sessions, pending approvals, rate-limit counters"],
            "secrets": "sealed values are stored exactly as they are in the database; OBOP_SECRET_KEY is not in this backup (an instance without it keeps a generated key in the database, and that key is in the backup)",
        });
        if let Some(r) = &self.records_sha256 {
            j["records_sha256"] = J::from(r.as_str());
        }
        if let Some(d) = &self.database_id {
            j["database_id"] = J::from(d.as_str());
        }
        if !self.blobs_damaged.is_empty() {
            j["blobs_damaged"] = json!(self.blobs_damaged);
        }
        if let (Some(id), Some(e)) = (&self.backup_id, &self.encryption) {
            j["backup_id"] = J::from(id.as_str());
            j["encryption"] = json!({
                "cipher": e.cipher, "key_id": e.key_id, "chunk_size": e.chunk_size,
                "chunks": e.chunks, "chain_head": e.chain_head,
            });
        }
        j
    }

    fn to_json(&self) -> J {
        let mut j = self.to_json_unsigned();
        if let Some(mac) = &self.mac {
            j["mac"] = J::from(mac.as_str());
        }
        j
    }

    /// The bytes the MAC covers: the manifest without `mac`, as canonical JSON (sorted keys).
    fn mac_input(j: &J) -> Vec<u8> {
        let mut j = j.clone();
        if let Some(o) = j.as_object_mut() {
            o.remove("mac");
        }
        j.to_string().into_bytes()
    }

    fn from_json(j: &J) -> Result<Manifest, String> {
        if j["kind"].as_str() != Some("obop-backup") {
            return Err("not an obop backup manifest".into());
        }
        let s = |k: &str| {
            j[k].as_str()
                .map(String::from)
                .ok_or_else(|| format!("the manifest has no {k}"))
        };
        let n = |k: &str| {
            j[k].as_u64()
                .ok_or_else(|| format!("the manifest has no {k}"))
        };
        let encryption = match j.get("encryption").filter(|e| e.is_object()) {
            None => None,
            Some(e) => {
                let es = |k: &str| {
                    e[k].as_str()
                        .map(String::from)
                        .ok_or_else(|| format!("the manifest's encryption has no {k}"))
                };
                // The chunk size sizes buffers before anything is authenticated: bound it here.
                let chunk_size = e["chunk_size"]
                    .as_u64()
                    .ok_or("the manifest's encryption has no chunk_size")?;
                if !(16..=stream::MAX_CHUNK as u64).contains(&chunk_size) {
                    return Err(
                        "the manifest's encryption chunk_size is out of range: not a backup this obop wrote"
                            .into(),
                    );
                }
                Some(Encryption {
                    cipher: es("cipher")?,
                    key_id: es("key_id")?,
                    chunk_size: chunk_size as usize,
                    chunks: e["chunks"]
                        .as_u64()
                        .ok_or("the manifest's encryption has no chunks")?,
                    chain_head: es("chain_head")?,
                })
            }
        };
        Ok(Manifest {
            format: n("format")?,
            core_version: s("core_version")?,
            schema_version: n("schema_version")?,
            created_at: s("created_at")?,
            source: s("source")?,
            data_file: s("data_file")?,
            data_sha256: s("data_sha256")?,
            data_bytes: n("data_bytes")?,
            content_sha256: s("content_sha256")?,
            records_sha256: j["records_sha256"].as_str().map(String::from),
            records: n("records")? as usize,
            models: j["models"]
                .as_object()
                .ok_or("the manifest has no models")?
                .iter()
                .map(|(k, v)| (k.clone(), v.as_u64().unwrap_or(0) as usize))
                .collect(),
            blobs: j["blobs"].as_u64().unwrap_or(0) as usize,
            blobs_damaged: j["blobs_damaged"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            database_id: match j.get("database_id") {
                None | Some(J::Null) => None,
                Some(J::String(d))
                    if !d.is_empty()
                        && d.len() <= 64
                        && d.bytes().all(|c| c.is_ascii_hexdigit()) =>
                {
                    Some(d.clone())
                }
                Some(_) => {
                    return Err(
                        "the manifest's database_id is not a database id (hex, at most 64 characters): not a backup this obop wrote"
                            .into(),
                    );
                }
            },
            backup_id: j["backup_id"].as_str().map(String::from),
            encryption,
            mac: j["mac"].as_str().map(String::from),
        })
    }

    /// Whether this backup is sealed (format 2).
    pub fn sealed(&self) -> bool {
        self.format == FORMAT_SEALED
    }
}

#[derive(Debug, Clone)]
pub struct Created {
    pub data_path: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest: Manifest,
}

fn manifest_path_for(data: &Path) -> PathBuf {
    let mut s = data.as_os_str().to_os_string();
    s.push(".manifest.json");
    PathBuf::from(s)
}

/// The file name of a backup taken at `t` (Unix seconds).
pub fn default_name(t: u64, sealed: bool) -> String {
    let stamp = rfc3339(t).replace(['-', ':'], "");
    format!(
        "obop-backup-{stamp}.{}",
        if sealed { "obbk" } else { "jsonl.gz" }
    )
}

/// Where the backup goes: a path ending in `.gz` or `.obbk` is the data file; anything else is a
/// directory (created if needed) that gets a timestamped file name. A sealed backup is not gzip:
/// it cannot be given a `.gz` name, which tools would take for a file they can read.
fn resolve_out(out: &Path, sealed: bool, at: u64) -> Result<PathBuf, String> {
    let name = out.to_string_lossy();
    if out.is_dir() || !(name.ends_with(".gz") || name.ends_with(".obbk")) {
        return Ok(out.join(default_name(at, sealed)));
    }
    if sealed && name.ends_with(".gz") {
        return Err(format!(
            "{name} would hold a sealed backup (format 2), which gzip cannot read. Name the file .obbk, give a directory, or pass --plain for an unencrypted .gz"
        ));
    }
    Ok(out.to_path_buf())
}

/// Write a backup of `backend` to `out` (a directory, or the data file). `progress` is told the
/// number of records written so far after each batch.
pub fn create(
    backend: &Arc<dyn Backend>,
    schema_version: u64,
    out: &Path,
    progress: &mut dyn FnMut(usize),
) -> Result<Created, String> {
    create_with(backend, None, schema_version, out, progress)
}

/// As [`create`], and the bytes of every uploaded blob when `blobs` is given.
pub fn create_with(
    backend: &Arc<dyn Backend>,
    blobs: Option<&dyn Blobs>,
    schema_version: u64,
    out: &Path,
    progress: &mut dyn FnMut(usize),
) -> Result<Created, String> {
    create_sealed(backend, blobs, schema_version, out, &Keys::none(), progress)
}

/// As [`create_with`], sealed (format 2) when `keys` can seal, plain (format 1) when not.
pub fn create_sealed(
    backend: &Arc<dyn Backend>,
    blobs: Option<&dyn Blobs>,
    schema_version: u64,
    out: &Path,
    keys: &Keys,
    progress: &mut dyn FnMut(usize),
) -> Result<Created, String> {
    create_at(
        backend,
        blobs,
        schema_version,
        out,
        keys,
        now_secs(),
        progress,
    )
}

/// As [`create_sealed`], with the time the manifest records given (Unix seconds): the scheduler's
/// clock, which a test moves by hand.
///
/// Nothing is left behind by a run that fails or is killed except part files (`*.part`), which the
/// next backup into the directory sweeps. The finished files appear in this order: the manifest,
/// then the data file, each flushed first and published without replacing anything, then the
/// directory is flushed; a data file therefore never appears without its manifest, and a name
/// that is taken is never overwritten.
pub fn create_at(
    backend: &Arc<dyn Backend>,
    blobs: Option<&dyn Blobs>,
    schema_version: u64,
    out: &Path,
    keys: &Keys,
    at: u64,
    progress: &mut dyn FnMut(usize),
) -> Result<Created, String> {
    let sealed = keys.can_seal();
    let data_path = resolve_out(out, sealed, at)?;
    if let Some(dir) = data_path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fsutil::create_dir_private(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        // What an earlier run that was killed left behind.
        fsutil::sweep_parts(dir, fsutil::STALE_PART);
    }
    let manifest_path = manifest_path_for(&data_path);
    for p in [&data_path, &manifest_path] {
        if p.exists() {
            return Err(format!(
                "{} already exists; a backup is never overwritten",
                p.display()
            ));
        }
    }
    let (part, manifest_part) = (
        fsutil::part_name(&data_path),
        fsutil::part_name(&manifest_path),
    );
    let file = fsutil::create_new_private(&part)
        .map_err(|e| format!("cannot write {}: {e}", part.display()))?;
    let mut published = Published::default();
    let result = write_backup(
        backend,
        blobs,
        schema_version,
        (&data_path, &part),
        (&manifest_path, &manifest_part),
        file,
        keys,
        at,
        &mut published,
        progress,
    );
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
        let _ = std::fs::remove_file(&manifest_part);
        // Only what this run published: a name another run took is not ours to remove.
        if published.manifest {
            let _ = std::fs::remove_file(&manifest_path);
        }
        if published.data {
            let _ = std::fs::remove_file(&data_path);
        }
    }
    result
}

/// Which final names a run has published, so that a failure removes exactly those.
#[derive(Default)]
struct Published {
    manifest: bool,
    data: bool,
}

#[allow(clippy::too_many_arguments)]
fn write_backup(
    backend: &Arc<dyn Backend>,
    blobs: Option<&dyn Blobs>,
    schema_version: u64,
    (data_path, part): (&Path, &Path),
    (manifest_path, manifest_part): (&Path, &Path),
    file: File,
    keys: &Keys,
    at: u64,
    published: &mut Published,
    progress: &mut dyn FnMut(usize),
) -> Result<Created, String> {
    let sealed = keys.can_seal();
    let format = if sealed { FORMAT_SEALED } else { FORMAT };
    let tee = Tee {
        inner: BufWriter::new(file),
        hash: Sha256::new(),
        bytes: 0,
    };
    let created_at = rfc3339(at);
    let database_id = backend
        .get(obop_migrate::META_MODEL, obop_migrate::INSTANCE_ID)
        .ok()
        .flatten()
        .and_then(|f| f.get("id").and_then(J::as_str).map(String::from));
    let io = |e: std::io::Error| format!("write failed: {e}");
    // What a sealed backup needs before the first byte: its id, its data key, and the header.
    let mut seal_info: Option<(String, String)> = None;
    let sink = if sealed {
        let ring = &keys.ring;
        let key_id = keys.current_id().unwrap_or_default();
        let backup_id = hexs(&random::<16>());
        let data_key = random::<32>();
        let prefix = random::<7>();
        let wrapped = ring
            .wrap_data_key(&backup_id, &data_key)
            .map_err(|e| e.to_string())?;
        let header = json!({
            "kind": "obop-backup-data", "format": FORMAT_SEALED, "cipher": CIPHER,
            "chunk_size": stream::CHUNK, "backup_id": backup_id,
            "nonce_prefix": hexs(&prefix), "key_id": key_id, "wrapped_key": wrapped,
            "core_version": obop_types::CORE_VERSION, "schema_version": schema_version,
            "created_at": created_at, "source": backend.kind(),
        });
        let w = SealWriter::new(
            tee,
            header.to_string().as_bytes(),
            &data_key,
            prefix,
            stream::CHUNK,
        )
        .map_err(io)?;
        seal_info = Some((backup_id, key_id));
        Sink::Sealed(w)
    } else {
        Sink::Plain(tee)
    };
    let mut gz = flate2::write::GzEncoder::new(sink, flate2::Compression::default());
    let header = json!({
        "type": "header", "format": format, "core_version": obop_types::CORE_VERSION,
        "schema_version": schema_version, "created_at": created_at, "source": backend.kind(),
        "database_id": database_id,
    });
    writeln!(gz, "{header}").map_err(io)?;
    let mut content = Sha256::new();
    let mut records = 0usize;
    let mut models: BTreeMap<String, usize> = BTreeMap::new();
    backend.export(BATCH, &mut |batch| {
        for (m, i, d) in batch {
            let line = format!("{}\n", json!({"m": m, "i": i, "d": d}));
            content.update(line.as_bytes());
            gz.write_all(line.as_bytes())
                .map_err(|e| format!("write failed: {e}"))?;
            *models.entry(m.clone()).or_default() += 1;
        }
        records += batch.len();
        progress(records);
        Ok(())
    })?;
    let records_sha256 = hexs(&content.clone().finalize());
    let mut blob_count = 0usize;
    let mut blobs_damaged: Vec<String> = Vec::new();
    if let Some(blobs) = blobs {
        for b in blobs.export()? {
            let b = b?;
            // A file whose bytes rotted on disk no longer hashes to its id. Backing it up would
            // make a restore fail halfway; it is left out and named in the manifest instead.
            if hexs(&Sha256::digest(&b.bytes)) != b.info.id {
                if blobs_damaged.len() < 50 {
                    blobs_damaged.push(b.info.id.clone());
                }
                continue;
            }
            let line = format!(
                "{}\n",
                json!({
                    "blob": b.info.id, "ct": b.info.content_type, "w": b.info.width,
                    "h": b.info.height, "created": b.info.created,
                    "x": base64::engine::general_purpose::STANDARD.encode(&b.bytes),
                })
            );
            content.update(line.as_bytes());
            gz.write_all(line.as_bytes())
                .map_err(|e| format!("write failed: {e}"))?;
            blob_count += 1;
        }
    }
    let content_sha256 = hexs(&content.finalize());
    writeln!(
        gz,
        "{}",
        json!({"type": "end", "records": records, "blobs": blob_count,
               "content_sha256": content_sha256})
    )
    .map_err(io)?;
    let (mut tee, sealing) = gz.finish().map_err(io)?.finish().map_err(io)?;
    tee.flush().map_err(io)?;
    tee.inner.get_ref().sync_all().map_err(io)?;
    let (data_sha256, data_bytes) = (hexs(&tee.hash.clone().finalize()), tee.bytes);
    drop(tee);
    let mut manifest = Manifest {
        format,
        core_version: obop_types::CORE_VERSION.into(),
        schema_version,
        created_at,
        source: backend.kind().into(),
        data_file: data_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        data_sha256,
        data_bytes,
        content_sha256,
        records_sha256: Some(records_sha256),
        records,
        models,
        blobs: blob_count,
        blobs_damaged,
        database_id,
        backup_id: None,
        encryption: None,
        mac: None,
    };
    if let (Some((backup_id, key_id)), Some(s)) = (seal_info, sealing) {
        manifest.backup_id = Some(backup_id);
        manifest.encryption = Some(Encryption {
            cipher: CIPHER.into(),
            key_id: key_id.clone(),
            chunk_size: stream::CHUNK,
            chunks: s.chunks,
            chain_head: s.chain_head,
        });
        let tag = keys
            .ring
            .mac(
                &key_id,
                MAC_PURPOSE,
                &Manifest::mac_input(&manifest.to_json_unsigned()),
            )
            .map_err(|e| e.to_string())?;
        manifest.mac = Some(hexs(&tag));
    }
    // The manifest first, flushed, under its final name; then the data file. A crash between the
    // two leaves a manifest with no data file, which is not a backup and is never listed as one.
    let text = format!("{:#}\n", manifest.to_json());
    let mut mf = fsutil::create_new_private(manifest_part)
        .map_err(|e| format!("cannot write {}: {e}", manifest_part.display()))?;
    mf.write_all(text.as_bytes())
        .and_then(|()| mf.sync_all())
        .map_err(|e| format!("cannot write {}: {e}", manifest_part.display()))?;
    drop(mf);
    fsutil::publish_new(manifest_part, manifest_path)?;
    published.manifest = true;
    fsutil::publish_new(part, data_path)?;
    published.data = true;
    if let Some(dir) = data_path.parent() {
        fsutil::sync_dir(dir);
    }
    Ok(Created {
        data_path: data_path.to_path_buf(),
        manifest_path: manifest_path.to_path_buf(),
        manifest,
    })
}

// The manifest next to a data file (or the manifest itself when that is what `file` names).
fn read_manifest(file: &Path) -> Result<(Manifest, PathBuf), String> {
    let (m, data, _) = read_manifest_json(file)?;
    Ok((m, data))
}

fn read_manifest_json(file: &Path) -> Result<(Manifest, PathBuf, J), String> {
    let name = file.to_string_lossy().to_string();
    let (data, manifest) = if name.ends_with(".manifest.json") {
        (
            PathBuf::from(name.trim_end_matches(".manifest.json")),
            file.to_path_buf(),
        )
    } else {
        (file.to_path_buf(), manifest_path_for(file))
    };
    let text = std::fs::read_to_string(&manifest)
        .map_err(|e| format!("cannot read the manifest {}: {e}", manifest.display()))?;
    let j: J = serde_json::from_str(&text).map_err(|e| format!("the manifest is not JSON: {e}"))?;
    Ok((Manifest::from_json(&j)?, data, j))
}

/// The manifest of a backup (the data file or its manifest), unverified.
pub fn manifest_of(file: &Path) -> Result<Manifest, String> {
    read_manifest(file).map(|(m, _)| m)
}

/// What `verify` found in a sound backup.
#[derive(Debug, Clone)]
pub struct Verified {
    pub manifest: Manifest,
    /// The backup was opened with its key: every chunk and the manifest authenticated. A plain
    /// (format 1) backup is never authenticated, and a sealed one checked without its key is
    /// only checked for integrity.
    pub authenticated: bool,
}

fn key_required(key_id: &str) -> String {
    format!(
        "{KEY_REQUIRED}: this backup is sealed with key {key_id}, which is not configured. Set OBOP_BACKUP_KEY (or OBOP_SECRET_KEY, and OBOP_SECRET_KEY_PREVIOUS for earlier keys) to the key that sealed it; nothing was read or written"
    )
}

/// What can be said about a backup from its manifest alone, before any database is opened or
/// created for a restore: a sealed backup whose key is not configured, or a plain one offered to
/// an instance that seals its own, is refused without touching anything.
pub fn precheck(file: &Path, keys: &Keys) -> Result<(), String> {
    let (m, _) = read_manifest(file)?;
    if m.sealed() {
        let enc = m
            .encryption
            .as_ref()
            .ok_or("the manifest of a sealed backup has no encryption section")?;
        if keys.ring.find(&enc.key_id).is_none() {
            return Err(key_required(&enc.key_id));
        }
    } else if keys.refuses_plain() {
        return Err(not_sealed(keys));
    }
    Ok(())
}

fn not_sealed(keys: &Keys) -> String {
    format!(
        "{NOT_SEALED}: this backup is plain (format 1), but this instance seals its backups (key {}). A plain backup is not authenticated: anyone who can write the backup directory could have forged it. If it is an old backup from before a key was set and you trust where it has been, pass --allow-plain",
        keys.current_id().unwrap_or_default()
    )
}

fn io_text(e: &std::io::Error) -> String {
    match stream::stream_error(e) {
        Some(s) => format!("the sealed backup does not open: {s}"),
        None => format!("the data file cannot be decompressed: {e}"),
    }
}

/// The plaintext stream of a backup being read, shared so that the reader of a sealed backup can
/// be asked afterwards how far it got.
enum Inner {
    Plain(File),
    Sealed(Box<OpenReader<BufReader<File>>>),
}

#[derive(Clone)]
struct Shared(Rc<RefCell<Inner>>);

impl Read for Shared {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match &mut *self.0.borrow_mut() {
            Inner::Plain(f) => f.read(buf),
            Inner::Sealed(r) => r.read(buf),
        }
    }
}

impl Shared {
    /// Read what is left (a sealed stream must be read to its last chunk to be authenticated as
    /// a whole) and say how many chunks and which chain head the reader saw. Plain: nothing.
    fn finish(&self) -> std::io::Result<Option<(u64, String)>> {
        let mut inner = self.0.borrow_mut();
        let Inner::Sealed(r) = &mut *inner else {
            return Ok(None);
        };
        let mut rest = Vec::new();
        r.read_to_end(&mut rest)?;
        if !rest.is_empty() {
            return Err(std::io::Error::other("data follows the end of the backup"));
        }
        Ok(Some((r.chunks_read(), r.chain_head())))
    }
}

type Lines = std::io::Lines<BufReader<flate2::read::GzDecoder<Shared>>>;

/// The header of a sealed data file, checked against the manifest.
struct SealedHeader {
    bytes: Vec<u8>,
    backup_id: String,
    key_id: String,
    wrapped: String,
    prefix: [u8; 7],
    chunk: usize,
}

fn sealed_header(f: &mut impl Read, m: &Manifest) -> Result<SealedHeader, String> {
    let bytes = stream::read_header(f).map_err(|e| io_text(&e))?;
    let j: J = serde_json::from_slice(&bytes).map_err(|_| "the sealed header is not JSON")?;
    let s = |k: &str| {
        j[k].as_str()
            .map(String::from)
            .ok_or_else(|| format!("the sealed header has no {k}"))
    };
    if j["kind"] != "obop-backup-data" || j["format"] != FORMAT_SEALED || j["cipher"] != CIPHER {
        return Err("the sealed header is not one this obop reads".into());
    }
    let enc = m
        .encryption
        .as_ref()
        .ok_or("the manifest of a sealed backup has no encryption section")?;
    let h = SealedHeader {
        backup_id: s("backup_id")?,
        key_id: s("key_id")?,
        wrapped: s("wrapped_key")?,
        prefix: <[u8; 7]>::try_from(
            unhex(&s("nonce_prefix")?)
                .ok_or("the sealed header's nonce prefix is not hex")?
                .as_slice(),
        )
        .map_err(|_| "the sealed header's nonce prefix has the wrong length")?,
        chunk: j["chunk_size"]
            .as_u64()
            .ok_or("the sealed header has no chunk_size")? as usize,
        bytes,
    };
    if m.backup_id.as_deref() != Some(h.backup_id.as_str())
        || enc.key_id != h.key_id
        || enc.chunk_size != h.chunk
        || j["schema_version"].as_u64() != Some(m.schema_version)
        || j["core_version"].as_str() != Some(m.core_version.as_str())
    {
        return Err("the header of the data file disagrees with the manifest".into());
    }
    Ok(h)
}

/// Open the plaintext lines of a backup. Format 2 needs the key.
fn open_lines(data: &Path, m: &Manifest, keys: &Keys) -> Result<(Lines, Shared), String> {
    let cannot = |e: std::io::Error| format!("cannot read {}: {e}", data.display());
    let f = File::open(data).map_err(cannot)?;
    let shared = if m.sealed() {
        let mut r = BufReader::new(f);
        let h = sealed_header(&mut r, m)?;
        if keys.ring.find(&h.key_id).is_none() {
            return Err(key_required(&h.key_id));
        }
        let data_key = keys
            .ring
            .unwrap_data_key(&h.backup_id, &h.wrapped)
            .map_err(|_| {
                "the wrapped data key does not open: the header was changed".to_string()
            })?;
        let o =
            OpenReader::new(r, &h.bytes, &data_key, h.prefix, h.chunk).map_err(|e| io_text(&e))?;
        Shared(Rc::new(RefCell::new(Inner::Sealed(Box::new(o)))))
    } else {
        Shared(Rc::new(RefCell::new(Inner::Plain(f))))
    };
    Ok((
        BufReader::new(flate2::read::GzDecoder::new(shared.clone())).lines(),
        shared,
    ))
}

fn file_sha256(path: &Path) -> Result<(String, u64), String> {
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
    Ok((hexs(&h.finalize()), n))
}

/// Check a plain backup, or a sealed one without its key (integrity only): format, checksum of
/// the file as stored, then a full pass over the records (gzip integrity, line shape, counts,
/// content checksum). A sealed backup is opened with [`verify_with`].
pub fn verify(file: &Path) -> Result<Verified, String> {
    verify_with(file, &Keys::none())
}

/// As [`verify`]. A sealed (format 2) backup needs its key: without it this refuses with
/// `restore_key_required`; with it every chunk and the manifest are authenticated.
pub fn verify_with(file: &Path, keys: &Keys) -> Result<Verified, String> {
    verify_inner(file, keys, false)
}

/// Integrity of a sealed backup without its key: the file's checksum and size, the framing and
/// the hash chain against the manifest. It says nothing about who wrote the file
/// (`authenticated` is false).
pub fn verify_structure(file: &Path) -> Result<Verified, String> {
    verify_inner(file, &Keys::none(), true)
}

fn verify_inner(file: &Path, keys: &Keys, structure_only: bool) -> Result<Verified, String> {
    let (m, data, json) = read_manifest_json(file)?;
    if m.format != FORMAT && m.format != FORMAT_SEALED {
        return Err(format!(
            "backup format {} is not supported by this obop (it reads formats {FORMAT} and {FORMAT_SEALED})",
            m.format
        ));
    }
    // A key is configured and the file says it needs none: a plain pair can be made by anyone who
    // can write the directory, so it is refused unless the operator vouches for it.
    if !m.sealed() && keys.refuses_plain() {
        return Err(not_sealed(keys));
    }
    let (sha, bytes) = file_sha256(&data)?;
    if bytes != m.data_bytes {
        return Err(format!(
            "the data file is {bytes} bytes, the manifest says {}: truncated or replaced",
            m.data_bytes
        ));
    }
    if sha != m.data_sha256 {
        return Err(
            "the data file does not match the checksum in the manifest: damaged or changed".into(),
        );
    }
    if m.sealed() {
        let enc = m
            .encryption
            .clone()
            .ok_or("the manifest of a sealed backup has no encryption section")?;
        // Framing and hash chain need no key.
        let f = File::open(&data).map_err(|e| format!("cannot read {}: {e}", data.display()))?;
        let (chunks, head) =
            stream::scan(BufReader::new(f), enc.chunk_size).map_err(|e| io_text(&e))?;
        if chunks != enc.chunks || head != enc.chain_head {
            return Err("the chunks of the data file do not match the manifest: the file was rearranged or replaced".into());
        }
        if structure_only {
            let mut f = BufReader::new(File::open(&data).map_err(|e| e.to_string())?);
            sealed_header(&mut f, &m)?;
            return Ok(Verified {
                manifest: m,
                authenticated: false,
            });
        }
        if keys.ring.find(&enc.key_id).is_none() {
            return Err(key_required(&enc.key_id));
        }
        let mac = m
            .mac
            .as_deref()
            .and_then(unhex)
            .ok_or("the manifest of a sealed backup has no MAC: it cannot be authenticated")?;
        if !keys
            .ring
            .mac_matches(&enc.key_id, MAC_PURPOSE, &Manifest::mac_input(&json), &mac)
            .map_err(|e| e.to_string())?
        {
            return Err("the manifest does not authenticate: it was edited, or it is not from the holder of the key".into());
        }
    }
    let (mut it, shared) = open_lines(&data, &m, keys)?;
    let bad = |e: std::io::Error| io_text(&e);
    let head: J = serde_json::from_str(&it.next().ok_or("the data file is empty")?.map_err(bad)?)
        .map_err(|_| "the first line is not a header".to_string())?;
    if head["type"] != "header"
        || head["format"].as_u64() != Some(m.format)
        || head["schema_version"].as_u64() != Some(m.schema_version)
        || head["core_version"].as_str() != Some(m.core_version.as_str())
        || head["database_id"].as_str() != m.database_id.as_deref()
    {
        return Err("the header of the data file disagrees with the manifest".into());
    }
    let (mut content, mut n, mut models) =
        (Sha256::new(), 0usize, BTreeMap::<String, usize>::new());
    let mut blobs = 0usize;
    let mut end: Option<J> = None;
    for line in it {
        let line = line.map_err(bad)?;
        if end.is_some() {
            return Err("there are lines after the end marker".into());
        }
        let j: J =
            serde_json::from_str(&line).map_err(|_| format!("line {} is not JSON", n + 2))?;
        if j["type"] == "end" {
            end = Some(j);
            continue;
        }
        if let Some(id) = j["blob"].as_str() {
            if !j["x"].is_string() || id.len() != 64 {
                return Err(format!("blob {} is malformed", blobs + 1));
            }
            // An uploaded file must hash to its id: otherwise a restore would replace the
            // records and only then find that it cannot write the file.
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(j["x"].as_str().unwrap_or_default())
                .map_err(|_| format!("blob {id} is not valid base64"))?;
            if hexs(&Sha256::digest(&bytes)) != id {
                return Err(format!(
                    "blob {id} does not hash to its id: the uploaded file in this backup is damaged"
                ));
            }
            content.update(line.as_bytes());
            content.update(b"\n");
            blobs += 1;
            continue;
        }
        if blobs > 0 {
            return Err("a record follows the blobs".into());
        }
        if !(j["m"].is_string() && j["i"].is_string() && j["d"].is_object()) {
            return Err(format!("record {} is malformed", n + 1));
        }
        content.update(line.as_bytes());
        content.update(b"\n");
        *models
            .entry(j["m"].as_str().unwrap_or("").to_string())
            .or_default() += 1;
        n += 1;
    }
    let end = end.ok_or("the end marker is missing: the backup is incomplete")?;
    let content = hexs(&content.finalize());
    if n != m.records || end["records"].as_u64() != Some(n as u64) {
        return Err(format!(
            "the data file holds {n} records, the manifest says {}",
            m.records
        ));
    }
    if blobs != m.blobs || end["blobs"].as_u64().unwrap_or(0) != blobs as u64 {
        return Err(format!(
            "the data file holds {blobs} blobs, the manifest says {}",
            m.blobs
        ));
    }
    if content != m.content_sha256 || end["content_sha256"].as_str() != Some(content.as_str()) {
        return Err("the records do not match their checksum".into());
    }
    if models != m.models {
        return Err("the counts per model disagree with the manifest".into());
    }
    // A sealed stream is read to its last chunk: that is where truncation and extension show.
    let seen = shared.finish().map_err(|e| io_text(&e))?;
    if let (Some((chunks, head)), Some(enc)) = (seen, &m.encryption)
        && (chunks != enc.chunks || head != enc.chain_head)
    {
        return Err("the chunks read do not match the manifest".into());
    }
    Ok(Verified {
        authenticated: m.sealed(),
        manifest: m,
    })
}

/// How often `needle` occurs in the bytes of `file` as stored (a canary scan, OBR-DAT-056): a
/// sealed backup holds none of its records' text, so a planted string must not be found.
pub fn count_in_file(file: &Path, needle: &[u8]) -> Result<usize, String> {
    let f = File::open(file).map_err(|e| format!("cannot read {}: {e}", file.display()))?;
    count_in_reader(f, needle)
}

fn count_in_reader(mut f: impl Read, needle: &[u8]) -> Result<usize, String> {
    if needle.is_empty() {
        return Err("an empty canary matches everything".into());
    }
    let (mut buf, mut hits) = (vec![0u8; 1 << 16], 0usize);
    let mut carry: Vec<u8> = Vec::new();
    loop {
        let k = f.read(&mut buf).map_err(|e| format!("read failed: {e}"))?;
        if k == 0 {
            break;
        }
        carry.extend_from_slice(&buf[..k]);
        let mut from = 0;
        while let Some(at) = carry[from..]
            .windows(needle.len())
            .position(|w| w == needle)
        {
            hits += 1;
            from += at + needle.len();
        }
        let keep = (needle.len() - 1).min(carry.len());
        carry.drain(..carry.len() - keep);
    }
    Ok(hits)
}

/// A canary scan of a backup (the data file or its manifest) as its format makes it readable by
/// someone without the key: a sealed file is scanned as stored; a plain one only compresses its
/// records, so it is scanned after decompression, which is what `gunzip` gives anyone. Returns the
/// number of occurrences and whether the backup is sealed.
pub fn count_in_backup(file: &Path, needle: &[u8]) -> Result<(usize, bool), String> {
    let (m, data) = read_manifest(file)?;
    if m.sealed() {
        return count_in_file(&data, needle).map(|n| (n, true));
    }
    let f = File::open(&data).map_err(|e| format!("cannot read {}: {e}", data.display()))?;
    count_in_reader(flate2::read::GzDecoder::new(f), needle).map(|n| (n, false))
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RestoreOptions {
    /// Replace what the target holds. Without it a non-empty target is refused.
    pub force: bool,
    /// Check everything and report, write nothing.
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub struct Restored {
    pub manifest: Manifest,
    /// Records the target held before.
    pub target_records: usize,
    /// Records written (or that would be, on a dry run).
    pub records: usize,
    /// The backup is from an older schema: run `obop migrate run` before serving.
    pub needs_migration: bool,
    pub dry_run: bool,
    /// The backup was opened with its key and every chunk and the manifest authenticated. A plain
    /// backup is never authenticated.
    pub authenticated: bool,
}

/// Restore `file` into `backend`. `latest_schema` is the schema version this build writes.
pub fn restore(
    backend: &Arc<dyn Backend>,
    latest_schema: u64,
    file: &Path,
    opts: RestoreOptions,
    progress: &mut dyn FnMut(usize),
) -> Result<Restored, String> {
    restore_with(backend, None, latest_schema, file, opts, progress)
}

/// As [`restore`], and the bytes of the backup's blobs into `blobs` when it is given. Without
/// it the blobs in the file are left alone (their records are still restored).
pub fn restore_with(
    backend: &Arc<dyn Backend>,
    blobs: Option<&dyn Blobs>,
    latest_schema: u64,
    file: &Path,
    opts: RestoreOptions,
    progress: &mut dyn FnMut(usize),
) -> Result<Restored, String> {
    restore_sealed(
        backend,
        blobs,
        latest_schema,
        file,
        opts,
        &Keys::none(),
        progress,
    )
}

/// As [`restore_with`], for a backup that may be sealed (needs `keys`).
pub fn restore_sealed(
    backend: &Arc<dyn Backend>,
    blobs: Option<&dyn Blobs>,
    latest_schema: u64,
    file: &Path,
    opts: RestoreOptions,
    keys: &Keys,
    progress: &mut dyn FnMut(usize),
) -> Result<Restored, String> {
    restore_with_rows(
        backend,
        blobs,
        latest_schema,
        file,
        opts,
        keys,
        Vec::new(),
        progress,
    )
}

/// As [`restore_sealed`], and `extra` rows written in the same transaction as the backup's own:
/// a rollback records its reason there, so the restored database never exists without it. The
/// extra rows are in `Restored::records` neither as written nor as expected.
///
/// The records are one transaction. The uploaded files are written after it, because what holds
/// each file is read back from the records; every file's bytes were checked against its id before
/// anything was written, so what can still fail is the store itself (a full disk). Then the error
/// starts with `restore_files_failed`: the records are restored, and running the restore again
/// with `force` writes the files.
#[allow(clippy::too_many_arguments)]
pub fn restore_with_rows(
    backend: &Arc<dyn Backend>,
    blobs: Option<&dyn Blobs>,
    latest_schema: u64,
    file: &Path,
    opts: RestoreOptions,
    keys: &Keys,
    extra: Vec<Row>,
    progress: &mut dyn FnMut(usize),
) -> Result<Restored, String> {
    // Every check that can fail without writing happens before anything is written.
    let v = verify_with(file, keys)?;
    let m = v.manifest;
    if m.schema_version > latest_schema {
        return Err(format!(
            "the backup is from schema version {}, newer than this build ({latest_schema}); \
             restore it with the obop version that wrote it, or a newer one",
            m.schema_version
        ));
    }
    let target_records = backend.total()?;
    if target_records > 0 && !opts.force {
        return Err(format!(
            "the target database is not empty ({target_records} records). Restore into an empty \
             database, or pass --force to replace its contents"
        ));
    }
    let needs_migration = m.schema_version < latest_schema;
    if opts.dry_run {
        return Ok(Restored {
            records: m.records,
            manifest: m,
            target_records,
            needs_migration,
            dry_run: true,
            authenticated: v.authenticated,
        });
    }
    let (_, data) = read_manifest(file)?;
    let (mut it, _) = open_lines(&data, &m, keys)?;
    it.next(); // the header, checked by verify
    let extra_len = extra.len();
    let mut extra = Some(extra);
    let mut written = 0usize;
    let mut first_blob: Option<J> = None;
    let mut over = false;
    let mut next = || -> Result<Option<Vec<Row>>, String> {
        let mut batch: Vec<Row> = Vec::new();
        while !over && batch.len() < BATCH {
            let Some(line) = it.next() else {
                over = true;
                break;
            };
            let line = line.map_err(|e| format!("read failed: {e}"))?;
            let j: J = serde_json::from_str(&line).map_err(|_| "a line is not JSON")?;
            if j["type"] == "end" {
                over = true;
                break;
            }
            if j["blob"].is_string() {
                // The blobs follow the records; this is the first of them.
                first_blob = Some(j);
                over = true;
                break;
            }
            let fields: Fields = j["d"].as_object().cloned().ok_or("a record is malformed")?;
            batch.push((
                j["m"].as_str().unwrap_or("").to_string(),
                j["i"].as_str().unwrap_or("").to_string(),
                fields,
            ));
        }
        if batch.is_empty() {
            // The backup's own rows are all in: the caller's rows go in the same transaction.
            return Ok(extra.take().filter(|x| !x.is_empty()));
        }
        written += batch.len();
        progress(written);
        Ok(Some(batch))
    };
    let n = backend.import(opts.force, &mut next)?;
    // The blobs follow the records. Their records were restored above, so what holds each one
    // is read back from the store; a blob without a record keeps what the file says about it.
    let mut blob_lines: Vec<J> = first_blob.into_iter().collect();
    if let Some(blobs) = blobs {
        let put = |j: &J| -> Result<(), String> {
            let id = j["blob"].as_str().unwrap_or_default();
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(j["x"].as_str().unwrap_or_default())
                .map_err(|_| format!("blob {id} is not valid base64"))?;
            let info = match blobs.info(id)? {
                Some(i) => i,
                None => BlobInfo {
                    id: id.to_string(),
                    size: bytes.len() as u64,
                    content_type: j["ct"]
                        .as_str()
                        .unwrap_or("application/octet-stream")
                        .into(),
                    width: j["w"].as_u64().map(|v| v as u32),
                    height: j["h"].as_u64().map(|v| v as u32),
                    created: j["created"].as_u64().unwrap_or(0),
                    owners: Vec::new(),
                    pending: Vec::new(),
                },
            };
            blobs.import(&ExportedBlob { info, bytes })
        };
        let files_failed = |id: &str, e: String| {
            format!(
                "{FILES_FAILED}: the {} records are restored, but the uploaded file {id} could not be written: {e}. Run the same command again to write the files (a restore needs --force then, because the records are already in; a rollback does not)",
                n.saturating_sub(extra_len)
            )
        };
        for j in blob_lines.drain(..) {
            let id = j["blob"].as_str().unwrap_or_default().to_string();
            put(&j).map_err(|e| files_failed(&id, e))?;
        }
        for line in it {
            let line = line.map_err(|e| format!("read failed: {e}"))?;
            let j: J = serde_json::from_str(&line).map_err(|_| "a line is not JSON")?;
            if j["blob"].is_string() {
                let id = j["blob"].as_str().unwrap_or_default().to_string();
                put(&j).map_err(|e| files_failed(&id, e))?;
            }
        }
    }
    if n != m.records + extra_len {
        // Cannot happen after a verified read; if it does, the import was one transaction, but
        // say so rather than claim success.
        return Err(format!(
            "restored {} records, expected {}",
            n.saturating_sub(extra_len),
            m.records
        ));
    }
    Ok(Restored {
        records: m.records,
        manifest: m,
        target_records,
        needs_migration,
        dry_run: false,
        authenticated: v.authenticated,
    })
}

#[cfg(test)]
mod review_tests;
#[cfg(test)]
mod sealed_tests;
#[cfg(test)]
mod tests;
