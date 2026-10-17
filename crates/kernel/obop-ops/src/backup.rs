//! `obop backup`, `obop backup verify` and `obop restore` (B4, D59): a consistent logical
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
//! Secrets stay sealed exactly as stored: a backup is as safe as the server key, which is NOT in
//! it. Short-lived values (sessions, pending approvals, rate-limit counters) are not backed up;
//! people sign in again after a restore and agents re-propose pending approvals.
//!
//! The checksums detect damage and accidental edits. They do not authenticate: someone who can
//! rewrite both files can forge a pair, so keep backups where only the operator can write.

use base64::Engine as _;
use obop_storage::blob::{BlobInfo, Blobs, ExportedBlob};
use obop_storage::{Backend, Fields, Row};
use serde_json::{Value as J, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The layout of the data file this build writes and reads.
pub const FORMAT: u64 = 1;
const BATCH: usize = 1000;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
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
    pub records: usize,
    pub models: BTreeMap<String, usize>,
    /// Uploaded blobs whose bytes are in the data file.
    pub blobs: usize,
}

impl Manifest {
    fn to_json(&self) -> J {
        json!({
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
        })
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
            records: n("records")? as usize,
            models: j["models"]
                .as_object()
                .ok_or("the manifest has no models")?
                .iter()
                .map(|(k, v)| (k.clone(), v.as_u64().unwrap_or(0) as usize))
                .collect(),
            blobs: j["blobs"].as_u64().unwrap_or(0) as usize,
        })
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

/// Where the backup goes: a path ending in `.gz` is the data file; anything else is a directory
/// (created if needed) that gets a timestamped file name.
fn resolve_out(out: &Path) -> PathBuf {
    if out.is_dir() || !out.to_string_lossy().ends_with(".gz") {
        let stamp = rfc3339(now_secs()).replace(['-', ':'], "");
        out.join(format!("obop-backup-{stamp}.jsonl.gz"))
    } else {
        out.to_path_buf()
    }
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
    let data_path = resolve_out(out);
    if let Some(dir) = data_path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    if data_path.exists() {
        return Err(format!(
            "{} already exists; a backup is never overwritten",
            data_path.display()
        ));
    }
    let part = {
        let mut s = data_path.as_os_str().to_os_string();
        s.push(".part");
        PathBuf::from(s)
    };
    let file = File::create(&part).map_err(|e| format!("cannot write {}: {e}", part.display()))?;
    let tee = Tee {
        inner: BufWriter::new(file),
        hash: Sha256::new(),
        bytes: 0,
    };
    let mut gz = flate2::write::GzEncoder::new(tee, flate2::Compression::default());
    let created_at = rfc3339(now_secs());
    let header = json!({
        "type": "header", "format": FORMAT, "core_version": obop_types::CORE_VERSION,
        "schema_version": schema_version, "created_at": created_at, "source": backend.kind(),
    });
    let io = |e: std::io::Error| format!("write failed: {e}");
    writeln!(gz, "{header}").map_err(io)?;
    let mut content = Sha256::new();
    let mut records = 0usize;
    let mut models: BTreeMap<String, usize> = BTreeMap::new();
    let result = backend.export(BATCH, &mut |batch| {
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
    });
    if let Err(e) = result {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    let mut blob_count = 0usize;
    if let Some(blobs) = blobs {
        let written = (|| -> Result<(), String> {
            for b in blobs.export()? {
                let b = b?;
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
            Ok(())
        })();
        if let Err(e) = written {
            let _ = std::fs::remove_file(&part);
            return Err(e);
        }
    }
    let content_sha256 = hex(&content.finalize());
    writeln!(
        gz,
        "{}",
        json!({"type": "end", "records": records, "blobs": blob_count,
               "content_sha256": content_sha256})
    )
    .map_err(io)?;
    let mut tee = gz.finish().map_err(io)?;
    tee.flush().map_err(io)?;
    tee.inner.get_ref().sync_all().map_err(io)?;
    let (data_sha256, data_bytes) = (hex(&tee.hash.clone().finalize()), tee.bytes);
    drop(tee);
    std::fs::rename(&part, &data_path).map_err(|e| format!("cannot finish the backup: {e}"))?;
    let manifest = Manifest {
        format: FORMAT,
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
        records,
        models,
        blobs: blob_count,
    };
    let manifest_path = manifest_path_for(&data_path);
    std::fs::write(&manifest_path, format!("{:#}\n", manifest.to_json()))
        .map_err(|e| format!("cannot write {}: {e}", manifest_path.display()))?;
    Ok(Created {
        data_path,
        manifest_path,
        manifest,
    })
}

/// The manifest next to a data file (or the manifest itself when that is what `file` names).
fn read_manifest(file: &Path) -> Result<(Manifest, PathBuf), String> {
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
    Ok((Manifest::from_json(&j)?, data))
}

/// What `verify` found in a sound backup.
#[derive(Debug, Clone)]
pub struct Verified {
    pub manifest: Manifest,
}

/// Open the data file for reading its lines. The caller checks the stored checksum first.
fn lines(data: &Path) -> Result<std::io::Lines<BufReader<flate2::read::GzDecoder<File>>>, String> {
    let f = File::open(data).map_err(|e| format!("cannot read {}: {e}", data.display()))?;
    Ok(BufReader::new(flate2::read::GzDecoder::new(f)).lines())
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
    Ok((hex(&h.finalize()), n))
}

/// Check a backup without touching any database: format, checksum of the file as stored, then
/// a full pass over the records (gzip integrity, line shape, counts, content checksum).
pub fn verify(file: &Path) -> Result<Verified, String> {
    let (m, data) = read_manifest(file)?;
    if m.format != FORMAT {
        return Err(format!(
            "backup format {} is not supported by this obop (it reads format {FORMAT})",
            m.format
        ));
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
    let mut it = lines(&data)?;
    let bad = |e: std::io::Error| format!("the data file cannot be decompressed: {e}");
    let head: J = serde_json::from_str(&it.next().ok_or("the data file is empty")?.map_err(bad)?)
        .map_err(|_| "the first line is not a header".to_string())?;
    if head["type"] != "header"
        || head["format"].as_u64() != Some(m.format)
        || head["schema_version"].as_u64() != Some(m.schema_version)
        || head["core_version"].as_str() != Some(m.core_version.as_str())
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
    let content = hex(&content.finalize());
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
    Ok(Verified { manifest: m })
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
    // Every check that can fail without writing happens before anything is written.
    let v = verify(file)?;
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
        });
    }
    let (_, data) = read_manifest(file)?;
    let mut it = lines(&data)?;
    it.next(); // the header, checked by verify
    let mut written = 0usize;
    let mut first_blob: Option<J> = None;
    let mut next = || -> Result<Option<Vec<Row>>, String> {
        let mut batch: Vec<Row> = Vec::new();
        while batch.len() < BATCH {
            let Some(line) = it.next() else { break };
            let line = line.map_err(|e| format!("read failed: {e}"))?;
            let j: J = serde_json::from_str(&line).map_err(|_| "a line is not JSON")?;
            if j["type"] == "end" {
                break;
            }
            if j["blob"].is_string() {
                first_blob = Some(j);
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
            return Ok(None);
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
        for j in blob_lines.drain(..) {
            put(&j)?;
        }
        for line in it {
            let line = line.map_err(|e| format!("read failed: {e}"))?;
            let j: J = serde_json::from_str(&line).map_err(|_| "a line is not JSON")?;
            if j["blob"].is_string() {
                put(&j)?;
            }
        }
    }
    if n != m.records {
        // Cannot happen after a verified read; if it does, the import was one transaction, but
        // say so rather than claim success.
        return Err(format!("restored {n} records, expected {}", m.records));
    }
    Ok(Restored {
        records: n,
        manifest: m,
        target_records,
        needs_migration,
        dry_run: false,
    })
}

#[cfg(test)]
mod tests;
