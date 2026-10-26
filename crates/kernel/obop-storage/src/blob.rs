//! Binary attachments: images and files that belong to a record (D60).
//!
//! Two small layers, so an object store can be added without touching the rest:
//!
//! - [`BlobBytes`] stores bytes under a key. Implementations: a directory ([`FsBytes`]), the
//!   database ([`DbBytes`]: PostgreSQL or SQLite, chunked, streamed back chunk by chunk) and memory
//!   ([`MemoryBytes`], tests). **An S3-compatible store is one more implementation of this trait**
//!   (`put`, `open`, `delete`, `keys`); nothing else changes.
//! - [`Blobs`] is what the rest of obop uses: content-addressed by SHA-256 (the id is the 64
//!   lowercase hex characters of the stored bytes), with the metadata, the owning records and
//!   the pending uploads kept as ordinary records of the main store (model `obop_blob`), so a
//!   backup of the database carries them and every change is a compare-and-set batch.
//!
//! The lifecycle: `put` stores the bytes and remembers who uploaded them (a *pending* claim).
//! Saving a record that names the blob `attach`es the owner (model, record, field) and drops the
//! claim. Deleting the record or replacing the value `detach`es it; the bytes go away when no
//! owner and no claim is left. `gc` removes claims that were never used and bytes without
//! metadata. The same content uploaded twice is stored once and counts as two owners.
//!
//! Nothing here looks inside the bytes: sniffing, decoding limits and re-encoding are done by the
//! caller (`obop-media`) before `put`. A virus scanner plugs in as [`Scanner`].

use crate::pgpool::{Class, PgPool, PgPoolConfig, tune};
use crate::tls::PgConnector;
use crate::{ApplyError, Backend, Fields, Op, Result};
use serde_json::{Value as J, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub const MODEL: &str = "obop_blob";
/// Widths (pixels) of the cached thumbnails; also the only widths a request may ask for.
pub const THUMB_WIDTHS: [u32; 4] = [320, 640, 960, 1280];

/// The id of `bytes`: lowercase hex SHA-256.
pub fn id_of(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(64);
    for b in Sha256::digest(bytes) {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// A well-formed blob id: 64 lowercase hex characters. Everything that takes an id from outside
/// (a URL, a field value) checks this before the id is used for anything, so a path, a `..` or a
/// SQL fragment never reaches storage.
pub fn valid_id(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The record (and field) a blob belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owner {
    pub model: String,
    pub record: String,
    pub field: String,
    /// The name the uploader gave the file; used for `Content-Disposition`.
    pub filename: String,
}

impl Owner {
    pub fn new(model: &str, record: &str, field: &str) -> Owner {
        Owner {
            model: model.into(),
            record: record.into(),
            field: field.into(),
            filename: String::new(),
        }
    }

    fn same(&self, o: &Owner) -> bool {
        self.model == o.model && self.record == o.record && self.field == o.field
    }

    fn to_json(&self) -> J {
        json!({"model": self.model, "record": self.record, "field": self.field, "filename": self.filename})
    }

    fn from_json(v: &J) -> Option<Owner> {
        let s = |k: &str| v.get(k).and_then(J::as_str).map(String::from);
        Some(Owner {
            model: s("model")?,
            record: s("record")?,
            field: s("field")?,
            filename: s("filename").unwrap_or_default(),
        })
    }
}

/// What is stored about a blob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobInfo {
    pub id: String,
    pub size: u64,
    pub content_type: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub created: u64,
    pub owners: Vec<Owner>,
    /// Uploaders who have not attached it yet: (principal id, unix time, file name).
    pub pending: Vec<Claim>,
}

/// An upload not attached yet: (principal id, unix time, file name).
pub type Claim = (String, u64, String);

/// What the caller says about bytes it hands to [`Blobs::put`].
#[derive(Debug, Clone, Default)]
pub struct NewBlob {
    pub content_type: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// The principal that uploads; remembered until the blob is attached (or `gc` forgets it).
    pub uploader: String,
    pub filename: String,
}

/// One blob with everything needed to recreate it elsewhere (backup, migration).
#[derive(Debug, Clone)]
pub struct ExportedBlob {
    pub info: BlobInfo,
    pub bytes: Vec<u8>,
}

/// Looks at bytes before they are stored. Plug a virus scanner in here: return `Err(reason)` to
/// refuse the upload. The default is no scanner (nothing is sent anywhere).
pub trait Scanner: Send + Sync {
    fn scan(&self, bytes: &[u8], content_type: &str) -> std::result::Result<(), String>;
}

/// Where bytes live. Keys are `<64 hex>` or `<64 hex>.w<width>` (a cached thumbnail); an
/// implementation must refuse anything else.
pub trait BlobBytes: Send + Sync {
    fn put(&self, key: &str, bytes: &[u8]) -> Result<()>;
    /// Stream the bytes back; `None` if the key is unknown.
    fn open(&self, key: &str) -> Result<Option<Box<dyn Read + Send>>>;
    fn delete(&self, key: &str) -> Result<bool>;
    fn keys(&self) -> Result<Vec<String>>;
}

/// A key is the id, optionally followed by `.w` and a width.
pub fn valid_key(key: &str) -> bool {
    let (id, rest) = match key.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (key, None),
    };
    valid_id(id)
        && rest.is_none_or(|r| {
            r.strip_prefix('w').is_some_and(|w| {
                !w.is_empty() && w.len() <= 5 && w.bytes().all(|c| c.is_ascii_digit())
            })
        })
}

fn check_key(key: &str) -> Result<()> {
    if valid_key(key) {
        Ok(())
    } else {
        Err("invalid blob key".into())
    }
}

// ---- memory ---------------------------------------------------------------------------------

#[derive(Default)]
pub struct MemoryBytes {
    map: Mutex<HashMap<String, Arc<[u8]>>>,
}

struct ArcCursor(Cursor<Arc<[u8]>>);

impl Read for ArcCursor {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

impl BlobBytes for MemoryBytes {
    fn put(&self, key: &str, bytes: &[u8]) -> Result<()> {
        check_key(key)?;
        self.map
            .lock()
            .unwrap()
            .insert(key.into(), Arc::from(bytes));
        Ok(())
    }
    fn open(&self, key: &str) -> Result<Option<Box<dyn Read + Send>>> {
        check_key(key)?;
        Ok(self
            .map
            .lock()
            .unwrap()
            .get(key)
            .map(|b| Box::new(ArcCursor(Cursor::new(b.clone()))) as Box<dyn Read + Send>))
    }
    fn delete(&self, key: &str) -> Result<bool> {
        check_key(key)?;
        Ok(self.map.lock().unwrap().remove(key).is_some())
    }
    fn keys(&self) -> Result<Vec<String>> {
        Ok(self.map.lock().unwrap().keys().cloned().collect())
    }
}

// ---- directory ------------------------------------------------------------------------------

/// Bytes as files below a directory: `<dir>/<first two hex>/<key>`. The path is built only from
/// a validated key, so no id can leave the directory.
pub struct FsBytes {
    dir: PathBuf,
}

impl FsBytes {
    pub fn new(dir: impl Into<PathBuf>) -> Result<Self> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir).map_err(|e| format!("blob directory: {e}"))?;
        Ok(Self { dir })
    }

    fn path(&self, key: &str) -> Result<PathBuf> {
        check_key(key)?;
        Ok(self.dir.join(&key[..2]).join(key))
    }
}

impl BlobBytes for FsBytes {
    fn put(&self, key: &str, bytes: &[u8]) -> Result<()> {
        let path = self.path(key)?;
        let parent = path.parent().ok_or("invalid blob path")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        // Write beside the target and rename, so a reader never sees half a file and a crash
        // leaves no truncated blob under its final name.
        let tmp = parent.join(format!(".{key}.{}.tmp", std::process::id()));
        let mut f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        f.write_all(bytes)
            .and_then(|()| f.sync_all())
            .map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            e.to_string()
        })
    }
    fn open(&self, key: &str) -> Result<Option<Box<dyn Read + Send>>> {
        match std::fs::File::open(self.path(key)?) {
            Ok(f) => Ok(Some(Box::new(f))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    fn delete(&self, key: &str) -> Result<bool> {
        match std::fs::remove_file(self.path(key)?) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.to_string()),
        }
    }
    fn keys(&self) -> Result<Vec<String>> {
        let mut out = Vec::new();
        for sub in std::fs::read_dir(&self.dir).map_err(|e| e.to_string())? {
            let sub = sub.map_err(|e| e.to_string())?.path();
            if !sub.is_dir() {
                continue;
            }
            for f in std::fs::read_dir(&sub).map_err(|e| e.to_string())? {
                if let Some(name) = f.map_err(|e| e.to_string())?.file_name().to_str()
                    && valid_key(name)
                {
                    out.push(name.to_string());
                }
            }
        }
        Ok(out)
    }
}

// ---- database -------------------------------------------------------------------------------

const CHUNK: usize = 512 * 1024;

/// The first integer of the two-integer advisory keys that serialise writers of one blob key.
/// The record batches use another (`pg.rs`), and the one-integer keys of `Backend::lock` are a
/// separate key space, so none of them meets this one.
const BLOB_LOCK_NAMESPACE: i32 = 0x626c;

enum Db {
    Sqlite(Mutex<rusqlite::Connection>),
    Pg(PgPool),
}

/// Bytes in a table of the database itself, in 512 KiB chunks (so a download streams chunk by
/// chunk and never holds the whole object in a connection). It uses a small pool of its own
/// (D74) on the same database, so a backup of the database holds the blobs. PostgreSQL uses `bytea` rows
/// instead of large objects: they are covered by ordinary dumps and replication.
pub struct DbBytes {
    db: Arc<Db>,
}

impl DbBytes {
    /// Open the byte table of `url` (`sqlite://PATH` or `postgres://...`). An in-memory SQLite
    /// database is not shared between connections, so use [`MemoryBytes`] there.
    pub fn open(url: &str) -> Result<Self> {
        Self::open_with(url, None)
    }

    /// As [`DbBytes::open`], with the pool settings given instead of read from the environment.
    pub fn open_with(url: &str, pool: Option<PgPoolConfig>) -> Result<Self> {
        let db = if let Some(path) = url
            .strip_prefix("sqlite://")
            .or_else(|| url.strip_prefix("sqlite:"))
        {
            if path.is_empty() || path == ":memory:" {
                return Err("an in-memory SQLite database cannot share a byte table".into());
            }
            let conn = rusqlite::Connection::open(path).map_err(|e| e.to_string())?;
            conn.execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA busy_timeout = 5000;
                 CREATE TABLE IF NOT EXISTS obop_blob_data (
                     key  TEXT NOT NULL,
                     seq  INTEGER NOT NULL,
                     data BLOB NOT NULL,
                     PRIMARY KEY (key, seq)
                 ) WITHOUT ROWID;",
            )
            .map_err(|e| e.to_string())?;
            Db::Sqlite(Mutex::new(conn))
        } else if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            let cfg = Arc::new(match pool {
                Some(c) => c,
                None => PgPoolConfig::from_env()?,
            });
            let connector = PgConnector::new(url)?;
            let mut client = tune(connector.clone(), "obop-blob", &cfg).connect()?;
            client
                .batch_execute(
                    "CREATE TABLE IF NOT EXISTS obop_blob_data (
                         key  TEXT NOT NULL,
                         seq  INTEGER NOT NULL,
                         data BYTEA NOT NULL,
                         PRIMARY KEY (key, seq)
                     );",
                )
                .map_err(|e| e.to_string())?;
            let pool = PgPool::new("blob", connector, cfg.blob_size, &cfg);
            pool.seed(client);
            Db::Pg(pool)
        } else {
            return Err("unsupported database url for blobs".into());
        };
        Ok(Self { db: Arc::new(db) })
    }

    fn chunk(&self, key: &str, seq: i32) -> Result<Option<Vec<u8>>> {
        match &*self.db {
            Db::Sqlite(c) => {
                let g = c.lock().unwrap();
                let mut st = g
                    .prepare_cached("SELECT data FROM obop_blob_data WHERE key = ?1 AND seq = ?2")
                    .map_err(|e| e.to_string())?;
                let mut rows = st.query((key, seq)).map_err(|e| e.to_string())?;
                match rows.next().map_err(|e| e.to_string())? {
                    Some(r) => Ok(Some(r.get::<_, Vec<u8>>(0).map_err(|e| e.to_string())?)),
                    None => Ok(None),
                }
            }
            Db::Pg(p) => p.read(Class::Point, |c| {
                Ok(c.query_opt(
                    "SELECT data FROM obop_blob_data WHERE key = $1 AND seq = $2",
                    &[&key, &seq],
                )?
                .map(|r| r.get::<_, Vec<u8>>(0)))
            }),
        }
    }
}

struct DbReader {
    db: DbBytes,
    key: String,
    seq: i32,
    buf: Cursor<Vec<u8>>,
    done: bool,
}

impl Read for DbReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        loop {
            let n = self.buf.read(out)?;
            if n > 0 || self.done {
                return Ok(n);
            }
            match self.db.chunk(&self.key, self.seq) {
                Ok(Some(c)) => {
                    self.buf = Cursor::new(c);
                    self.seq += 1;
                }
                Ok(None) => self.done = true,
                Err(e) => return Err(std::io::Error::other(e)),
            }
        }
    }
}

impl BlobBytes for DbBytes {
    fn put(&self, key: &str, bytes: &[u8]) -> Result<()> {
        check_key(key)?;
        match &*self.db {
            Db::Sqlite(c) => {
                let mut g = c.lock().unwrap();
                let tx = g.transaction().map_err(|e| e.to_string())?;
                tx.execute("DELETE FROM obop_blob_data WHERE key = ?1", (key,))
                    .map_err(|e| e.to_string())?;
                for (seq, part) in bytes.chunks(CHUNK).enumerate() {
                    tx.execute(
                        "INSERT INTO obop_blob_data (key, seq, data) VALUES (?1, ?2, ?3)",
                        (key, seq as i32, part),
                    )
                    .map_err(|e| e.to_string())?;
                }
                if bytes.is_empty() {
                    tx.execute(
                        "INSERT INTO obop_blob_data (key, seq, data) VALUES (?1, 0, ?2)",
                        (key, Vec::<u8>::new()),
                    )
                    .map_err(|e| e.to_string())?;
                }
                tx.commit().map_err(|e| e.to_string())
            }
            // Two writers of one key (the same content uploaded twice at once: the key is the hash
            // of the bytes) must not both think the key is free: the second would wait for the
            // first at the primary key and then fail with a duplicate key. The transaction takes
            // the advisory lock of the key first, so the second one waits, then replaces the
            // rows of the first with the same bytes. The lock is the transaction's own, so it is
            // gone with the commit; it is in a key space of its own (not the batch locks').
            Db::Pg(p) => p.tx_retry(Class::Batch, |c| {
                c.execute(
                    "SELECT pg_advisory_xact_lock($1, hashtext($2))",
                    &[&BLOB_LOCK_NAMESPACE, &key],
                )?;
                c.execute("DELETE FROM obop_blob_data WHERE key = $1", &[&key])?;
                let mut parts: Vec<&[u8]> = bytes.chunks(CHUNK).collect();
                if parts.is_empty() {
                    parts.push(&[]);
                }
                for (seq, part) in parts.into_iter().enumerate() {
                    c.execute(
                        "INSERT INTO obop_blob_data (key, seq, data) VALUES ($1, $2, $3)",
                        &[&key, &(seq as i32), &part],
                    )?;
                }
                Ok(())
            }),
        }
    }
    fn open(&self, key: &str) -> Result<Option<Box<dyn Read + Send>>> {
        check_key(key)?;
        let Some(first) = self.chunk(key, 0)? else {
            return Ok(None);
        };
        Ok(Some(Box::new(DbReader {
            db: DbBytes {
                db: self.db.clone(),
            },
            key: key.into(),
            seq: 1,
            buf: Cursor::new(first),
            done: false,
        })))
    }
    fn delete(&self, key: &str) -> Result<bool> {
        check_key(key)?;
        match &*self.db {
            Db::Sqlite(c) => c
                .lock()
                .unwrap()
                .execute("DELETE FROM obop_blob_data WHERE key = ?1", (key,))
                .map(|n| n > 0)
                .map_err(|e| e.to_string()),
            Db::Pg(p) => p.run(Class::Point, |c| {
                Ok(c.execute("DELETE FROM obop_blob_data WHERE key = $1", &[&key])? > 0)
            }),
        }
    }
    fn keys(&self) -> Result<Vec<String>> {
        match &*self.db {
            Db::Sqlite(c) => {
                let g = c.lock().unwrap();
                let mut st = g
                    .prepare("SELECT DISTINCT key FROM obop_blob_data")
                    .map_err(|e| e.to_string())?;
                let rows = st
                    .query_map((), |r| r.get::<_, String>(0))
                    .map_err(|e| e.to_string())?;
                rows.map(|r| r.map_err(|e| e.to_string())).collect()
            }
            Db::Pg(p) => p.read(Class::List, |c| {
                Ok(c.query("SELECT DISTINCT key FROM obop_blob_data", &[])?
                    .into_iter()
                    .map(|r| r.get::<_, String>(0))
                    .collect())
            }),
        }
    }
}

/// Choose where bytes go for the database at `url`: the directory `dir` when given (the
/// `OBOP_BLOB_DIR` setting); else a directory beside a SQLite file (`<file>.blobs`); else the
/// database itself (PostgreSQL); in-memory databases keep bytes in memory.
pub fn open_bytes(url: &str, dir: Option<&str>) -> Result<Arc<dyn BlobBytes>> {
    if let Some(d) = dir.filter(|d| !d.trim().is_empty()) {
        return Ok(Arc::new(FsBytes::new(d)?));
    }
    if url == "memory:" {
        return Ok(Arc::new(MemoryBytes::default()));
    }
    if let Some(path) = url
        .strip_prefix("sqlite://")
        .or_else(|| url.strip_prefix("sqlite:"))
    {
        return if path.is_empty() || path == ":memory:" {
            Ok(Arc::new(MemoryBytes::default()))
        } else {
            Ok(Arc::new(FsBytes::new(format!("{path}.blobs"))?))
        };
    }
    Ok(Arc::new(DbBytes::open(url)?))
}

// ---- the catalogue --------------------------------------------------------------------------

/// What the rest of obop uses. See the module comment for the lifecycle.
pub trait Blobs: Send + Sync {
    /// Store `bytes` (idempotent: the same content is stored once) and remember the uploader's
    /// claim. The id of the result is the SHA-256 of the bytes.
    fn put(&self, bytes: &[u8], meta: NewBlob) -> Result<BlobInfo>;
    fn info(&self, id: &str) -> Result<Option<BlobInfo>>;
    /// Stream the bytes; `None` if the blob does not exist.
    fn read(&self, id: &str) -> Result<Option<Box<dyn Read + Send>>>;
    /// All bytes, refusing more than `max` (for small public objects and thumbnails).
    fn read_all(&self, id: &str, max: u64) -> Result<Option<Vec<u8>>> {
        let Some(mut r) = self.read(id)? else {
            return Ok(None);
        };
        let mut out = Vec::new();
        (&mut r)
            .take(max + 1)
            .read_to_end(&mut out)
            .map_err(|e| e.to_string())?;
        if out.len() as u64 > max {
            return Err("blob is larger than the limit".into());
        }
        Ok(Some(out))
    }
    /// Record that `owner` holds the blob and drop the uploaders' claims on it.
    fn attach(&self, id: &str, owner: &Owner) -> Result<()>;
    /// Remove `owner`'s hold. The blob is deleted when nobody holds it and no upload is pending.
    fn detach(&self, id: &str, owner: &Owner) -> Result<()>;
    /// Remove every hold of one record (it was deleted or erased); returns the blobs it held.
    fn detach_record(&self, model: &str, record: &str) -> Result<Vec<String>>;
    /// The blobs a record holds, with the field.
    fn of_record(&self, model: &str, record: &str) -> Result<Vec<(String, BlobInfo)>>;
    /// A cached derived object (a thumbnail), keyed by `variant` such as `w640`.
    fn derived(&self, id: &str, variant: &str) -> Result<Option<Vec<u8>>>;
    fn put_derived(&self, id: &str, variant: &str, bytes: &[u8]) -> Result<()>;
    /// Forget claims older than `max_age_secs`, delete blobs nobody holds, delete bytes without
    /// metadata. Returns the number of blobs removed.
    fn gc(&self, max_age_secs: u64) -> Result<usize>;
    /// Every blob with its bytes, one at a time, for `obop backup` and migration to another
    /// store. Reads a snapshot of the ids; blobs added meanwhile are not included.
    fn export(&self) -> Result<Box<dyn Iterator<Item = Result<ExportedBlob>> + '_>>;
    /// Restore one exported blob (checks that the bytes hash to the id). Idempotent.
    fn import(&self, blob: &ExportedBlob) -> Result<()>;
}

/// [`Blobs`] over the record store (`meta`) and a [`BlobBytes`].
pub struct Catalog {
    meta: Arc<dyn Backend>,
    bytes: Arc<dyn BlobBytes>,
    scanner: Option<Arc<dyn Scanner>>,
}

impl Catalog {
    pub fn new(meta: Arc<dyn Backend>, bytes: Arc<dyn BlobBytes>) -> Self {
        Self {
            meta,
            bytes,
            scanner: None,
        }
    }

    /// Everything in memory, for tests.
    pub fn memory(meta: Arc<dyn Backend>) -> Self {
        Self::new(meta, Arc::new(MemoryBytes::default()))
    }

    pub fn with_scanner(mut self, scanner: Arc<dyn Scanner>) -> Self {
        self.scanner = Some(scanner);
        self
    }

    fn parse(id: &str, f: &Fields) -> BlobInfo {
        let n = |k: &str| f.get(k).and_then(J::as_u64);
        let owners = f
            .get("owners")
            .and_then(J::as_array)
            .map(|a| a.iter().filter_map(Owner::from_json).collect())
            .unwrap_or_default();
        let pending = f
            .get("pending")
            .and_then(J::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|p| {
                        Some((
                            p.get("by")?.as_str()?.to_string(),
                            p.get("at")?.as_u64()?,
                            p.get("filename")
                                .and_then(J::as_str)
                                .unwrap_or("")
                                .to_string(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        BlobInfo {
            id: id.into(),
            size: n("size").unwrap_or(0),
            content_type: f
                .get("content_type")
                .and_then(J::as_str)
                .unwrap_or("application/octet-stream")
                .into(),
            width: n("width").map(|w| w as u32),
            height: n("height").map(|w| w as u32),
            created: n("created").unwrap_or(0),
            owners,
            pending,
        }
    }

    fn fields(info: &BlobInfo) -> Fields {
        let mut f = Fields::new();
        f.insert("size".into(), info.size.into());
        f.insert("content_type".into(), info.content_type.clone().into());
        f.insert("created".into(), info.created.into());
        f.insert("width".into(), info.width.map_or(J::Null, J::from));
        f.insert("height".into(), info.height.map_or(J::Null, J::from));
        Self::set_holds(&mut f, &info.owners, &info.pending);
        f
    }

    fn set_holds(f: &mut Fields, owners: &[Owner], pending: &[Claim]) {
        f.insert(
            "owners".into(),
            J::Array(owners.iter().map(Owner::to_json).collect()),
        );
        f.insert(
            "pending".into(),
            J::Array(
                pending
                    .iter()
                    .map(|(by, at, filename)| json!({"by": by, "at": at, "filename": filename}))
                    .collect(),
            ),
        );
    }

    /// Change the owners and claims of a blob with compare-and-set, retrying when another writer
    /// got in between. `change` returns `false` for "nothing to do". The blob is deleted when it
    /// ends up with neither. Returns `false` if there is no such blob.
    fn mutate(
        &self,
        id: &str,
        change: &dyn Fn(&mut Vec<Owner>, &mut Vec<Claim>) -> bool,
    ) -> Result<bool> {
        for _ in 0..16 {
            let Some(cur) = self.meta.get(MODEL, id)? else {
                return Ok(false);
            };
            let info = Self::parse(id, &cur);
            let (mut owners, mut pending) = (info.owners.clone(), info.pending.clone());
            if !change(&mut owners, &mut pending) {
                return Ok(true);
            }
            if owners.is_empty() && pending.is_empty() {
                let r = self.meta.apply_checked(&[Op::Patch {
                    model: MODEL.into(),
                    id: id.into(),
                    patch: Fields::new(),
                    expect: vec![
                        (
                            "owners".into(),
                            cur.get("owners").cloned().unwrap_or(J::Null),
                        ),
                        (
                            "pending".into(),
                            cur.get("pending").cloned().unwrap_or(J::Null),
                        ),
                    ],
                }]);
                match r {
                    Ok(()) => {}
                    Err(ApplyError::Conflict { .. }) => continue,
                    Err(e) => return Err(e.to_string()),
                }
                // Nobody holds it: remove the metadata first (so nothing can attach to bytes
                // that are about to go), then the bytes and their thumbnails.
                self.meta.delete(MODEL, id)?;
                self.remove_bytes(id);
                return Ok(true);
            }
            let mut patch = Fields::new();
            Self::set_holds(&mut patch, &owners, &pending);
            let r = self.meta.apply_checked(&[Op::Patch {
                model: MODEL.into(),
                id: id.into(),
                patch,
                expect: vec![
                    (
                        "owners".into(),
                        cur.get("owners").cloned().unwrap_or(J::Null),
                    ),
                    (
                        "pending".into(),
                        cur.get("pending").cloned().unwrap_or(J::Null),
                    ),
                ],
            }]);
            match r {
                Ok(()) => return Ok(true),
                Err(ApplyError::Conflict { .. }) => continue,
                Err(e) => return Err(e.to_string()),
            }
        }
        Err("the blob is being changed by someone else; try again".into())
    }

    fn remove_bytes(&self, id: &str) {
        let _ = self.bytes.delete(id);
        for w in THUMB_WIDTHS {
            let _ = self.bytes.delete(&format!("{id}.w{w}"));
        }
    }
}

impl Blobs for Catalog {
    fn put(&self, bytes: &[u8], meta: NewBlob) -> Result<BlobInfo> {
        if let Some(s) = &self.scanner {
            s.scan(bytes, &meta.content_type)?;
        }
        let id = id_of(bytes);
        let claim = (meta.uploader.clone(), now(), meta.filename.clone());
        // Bytes first, then the metadata that points at them.
        if self.meta.get(MODEL, &id)?.is_none() || self.bytes.open(&id)?.is_none() {
            self.bytes.put(&id, bytes)?;
        }
        let info = BlobInfo {
            id: id.clone(),
            size: bytes.len() as u64,
            content_type: meta.content_type,
            width: meta.width,
            height: meta.height,
            created: now(),
            owners: Vec::new(),
            pending: vec![claim.clone()],
        };
        match self.meta.insert(MODEL, &id, &Self::fields(&info)) {
            Ok(()) => Ok(info),
            Err(_) => {
                // Already there (same content): add this uploader's claim.
                self.mutate(&id, &|_, pending| {
                    pending.retain(|p| p.0 != claim.0);
                    pending.push(claim.clone());
                    true
                })?;
                self.info(&id)?
                    .ok_or_else(|| "could not store the blob".into())
            }
        }
    }

    fn info(&self, id: &str) -> Result<Option<BlobInfo>> {
        if !valid_id(id) {
            return Ok(None);
        }
        Ok(self.meta.get(MODEL, id)?.map(|f| Self::parse(id, &f)))
    }

    fn read(&self, id: &str) -> Result<Option<Box<dyn Read + Send>>> {
        if !valid_id(id) || self.meta.get(MODEL, id)?.is_none() {
            return Ok(None);
        }
        self.bytes.open(id)
    }

    fn attach(&self, id: &str, owner: &Owner) -> Result<()> {
        if !valid_id(id) {
            return Err("invalid blob id".into());
        }
        let found = self.mutate(id, &|owners, pending| {
            if !owners.iter().any(|o| o.same(owner)) {
                owners.push(owner.clone());
            }
            pending.clear();
            true
        })?;
        if found {
            Ok(())
        } else {
            Err("no such blob".into())
        }
    }

    fn detach(&self, id: &str, owner: &Owner) -> Result<()> {
        if !valid_id(id) {
            return Ok(());
        }
        self.mutate(id, &|owners, _| {
            let before = owners.len();
            owners.retain(|o| !o.same(owner));
            owners.len() != before
        })?;
        Ok(())
    }

    fn detach_record(&self, model: &str, record: &str) -> Result<Vec<String>> {
        let mut gone = Vec::new();
        for (id, f) in self.meta.list(MODEL)? {
            let info = Self::parse(&id, &f);
            if info
                .owners
                .iter()
                .any(|o| o.model == model && o.record == record)
            {
                self.mutate(&id, &|owners, _| {
                    let before = owners.len();
                    owners.retain(|o| !(o.model == model && o.record == record));
                    owners.len() != before
                })?;
                gone.push(id);
            }
        }
        Ok(gone)
    }

    fn of_record(&self, model: &str, record: &str) -> Result<Vec<(String, BlobInfo)>> {
        let mut out = Vec::new();
        for (id, f) in self.meta.list(MODEL)? {
            let info = Self::parse(&id, &f);
            for o in info
                .owners
                .iter()
                .filter(|o| o.model == model && o.record == record)
            {
                out.push((o.field.clone(), info.clone()));
            }
        }
        Ok(out)
    }

    fn derived(&self, id: &str, variant: &str) -> Result<Option<Vec<u8>>> {
        if !valid_id(id) {
            return Ok(None);
        }
        let Some(mut r) = self.bytes.open(&format!("{id}.{variant}"))? else {
            return Ok(None);
        };
        let mut out = Vec::new();
        r.read_to_end(&mut out).map_err(|e| e.to_string())?;
        Ok(Some(out))
    }

    fn put_derived(&self, id: &str, variant: &str, bytes: &[u8]) -> Result<()> {
        if !valid_id(id) || self.meta.get(MODEL, id)?.is_none() {
            return Err("no such blob".into());
        }
        self.bytes.put(&format!("{id}.{variant}"), bytes)
    }

    fn gc(&self, max_age_secs: u64) -> Result<usize> {
        let t = now();
        let mut removed = 0;
        let ids: Vec<String> = self.meta.list(MODEL)?.into_iter().map(|(i, _)| i).collect();
        for id in &ids {
            let before = self.meta.get(MODEL, id)?.is_some();
            self.mutate(id, &|_, pending| {
                let n = pending.len();
                pending.retain(|p| t.saturating_sub(p.1) < max_age_secs);
                pending.len() != n
            })?;
            if before && self.meta.get(MODEL, id)?.is_none() {
                removed += 1;
            }
        }
        // Bytes whose metadata is gone (a crash between the two steps).
        for key in self.bytes.keys()? {
            let id = key.split('.').next().unwrap_or("");
            if self.meta.get(MODEL, id)?.is_none() {
                self.bytes.delete(&key)?;
            }
        }
        Ok(removed)
    }

    fn export(&self) -> Result<Box<dyn Iterator<Item = Result<ExportedBlob>> + '_>> {
        let ids: Vec<String> = self.meta.list(MODEL)?.into_iter().map(|(i, _)| i).collect();
        Ok(Box::new(ids.into_iter().filter_map(move |id| {
            let info = match self.info(&id) {
                Ok(Some(i)) => i,
                Ok(None) => return None,
                Err(e) => return Some(Err(e)),
            };
            Some(
                self.read_all(&id, u64::MAX / 2)
                    .and_then(|b| b.ok_or_else(|| format!("blob {id} has no bytes")))
                    .map(|bytes| ExportedBlob { info, bytes }),
            )
        })))
    }

    fn import(&self, blob: &ExportedBlob) -> Result<()> {
        if id_of(&blob.bytes) != blob.info.id || !valid_id(&blob.info.id) {
            return Err("the bytes do not match the blob id".into());
        }
        self.bytes.put(&blob.info.id, &blob.bytes)?;
        let f = Self::fields(&blob.info);
        match self.meta.insert(MODEL, &blob.info.id, &f) {
            Ok(()) => Ok(()),
            Err(_) => self.meta.update(MODEL, &blob.info.id, &f).map(|_| ()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        Catalog::memory(crate::open("memory:").unwrap())
    }

    fn up(c: &Catalog, bytes: &[u8], by: &str) -> BlobInfo {
        c.put(
            bytes,
            NewBlob {
                content_type: "image/png".into(),
                uploader: by.into(),
                ..NewBlob::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn ids_are_content_hashes_and_malformed_ones_never_pass() {
        let c = catalog();
        let a = up(&c, b"hello", "u1");
        assert_eq!(
            a.id,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert!(valid_id(&a.id));
        for bad in [
            "",
            "../etc/passwd",
            &"A".repeat(64),
            &format!("{}/x", "a".repeat(62)),
            &"a".repeat(63),
            &"a".repeat(65),
            "..%2f..%2f",
            "a'; DROP TABLE obop_record; --",
        ] {
            assert!(!valid_id(bad), "{bad}");
            assert!(c.info(bad).unwrap().is_none());
            assert!(c.read(bad).unwrap().is_none());
        }
        assert!(valid_key(&format!("{}.w640", a.id)));
        assert!(!valid_key(&format!("{}.w640/..", a.id)));
        assert!(!valid_key(&format!("{}.png", a.id)));
    }

    #[test]
    fn lifecycle_pending_attach_detach_and_delete() {
        let c = catalog();
        let b = up(&c, b"one", "u1");
        assert_eq!(b.pending.len(), 1);
        let again = up(&c, b"one", "u2"); // same content, second uploader
        assert_eq!(again.pending.len(), 2);
        let o = Owner::new("product", "p1", "image");
        c.attach(&b.id, &o).unwrap();
        c.attach(&b.id, &o).unwrap(); // idempotent
        let i = c.info(&b.id).unwrap().unwrap();
        assert_eq!((i.owners.len(), i.pending.len()), (1, 0));
        let o2 = Owner::new("product", "p2", "image");
        c.attach(&b.id, &o2).unwrap();
        c.detach(&b.id, &o).unwrap();
        assert!(c.read(&b.id).unwrap().is_some(), "still held by p2");
        assert_eq!(c.of_record("product", "p2").unwrap().len(), 1);
        assert_eq!(
            c.detach_record("product", "p2").unwrap(),
            vec![b.id.clone()]
        );
        assert!(c.info(&b.id).unwrap().is_none(), "nobody holds it any more");
        assert!(c.read(&b.id).unwrap().is_none());
        assert!(c.attach(&b.id, &o).is_err(), "cannot attach what is gone");
    }

    #[test]
    fn gc_drops_unused_uploads_and_orphan_bytes() {
        let c = catalog();
        let kept = up(&c, b"kept", "u");
        c.attach(&kept.id, &Owner::new("m", "r", "f")).unwrap();
        let unused = up(&c, b"unused", "u");
        assert_eq!(c.gc(3600).unwrap(), 0, "a fresh upload is not collected");
        assert!(c.info(&unused.id).unwrap().is_some());
        assert_eq!(c.gc(0).unwrap(), 1);
        assert!(c.info(&unused.id).unwrap().is_none());
        assert!(c.info(&kept.id).unwrap().is_some());
        // Bytes with no metadata are removed.
        let stray = id_of(b"stray");
        c.bytes.put(&stray, b"stray").unwrap();
        c.gc(3600).unwrap();
        assert!(c.bytes.open(&stray).unwrap().is_none());
    }

    #[test]
    fn export_and_import_round_trip_and_reject_wrong_bytes() {
        let c = catalog();
        let b = up(&c, b"export me", "u");
        c.attach(&b.id, &Owner::new("m", "r", "f")).unwrap();
        let all: Vec<_> = c.export().unwrap().collect::<Result<_>>().unwrap();
        assert_eq!(all.len(), 1);
        let other = catalog();
        other.import(&all[0]).unwrap();
        other.import(&all[0]).unwrap();
        let back = other.read_all(&b.id, 100).unwrap().unwrap();
        assert_eq!(back, b"export me");
        assert_eq!(other.info(&b.id).unwrap().unwrap().owners.len(), 1);
        let mut bad = all[0].clone();
        bad.bytes = b"tampered".to_vec();
        assert!(other.import(&bad).is_err());
    }

    #[test]
    fn a_scanner_can_refuse_an_upload() {
        struct No;
        impl Scanner for No {
            fn scan(&self, b: &[u8], _: &str) -> std::result::Result<(), String> {
                if b.starts_with(b"EICAR") {
                    Err("infected".into())
                } else {
                    Ok(())
                }
            }
        }
        let c = catalog().with_scanner(Arc::new(No));
        assert!(
            c.put(b"EICAR test", NewBlob::default())
                .is_err_and(|e| e == "infected")
        );
        assert!(c.put(b"fine", NewBlob::default()).is_ok());
    }

    fn bytes_contract(b: &dyn BlobBytes) {
        let id = id_of(b"x");
        let big: Vec<u8> = (0..(CHUNK * 2 + 17)).map(|i| (i % 251) as u8).collect();
        b.put(&id, &big).unwrap();
        let mut got = Vec::new();
        b.open(&id).unwrap().unwrap().read_to_end(&mut got).unwrap();
        assert_eq!(got, big, "streamed back whole");
        b.put(&id, b"small").unwrap();
        let mut got = Vec::new();
        b.open(&id).unwrap().unwrap().read_to_end(&mut got).unwrap();
        assert_eq!(got, b"small", "replaced");
        assert!(b.keys().unwrap().contains(&id));
        assert!(b.delete(&id).unwrap());
        assert!(!b.delete(&id).unwrap());
        assert!(b.open(&id).unwrap().is_none());
        for bad in ["../x", "", "zz", "a/b"] {
            assert!(b.put(bad, b"x").is_err(), "{bad}");
            assert!(b.open(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn memory_directory_and_sqlite_bytes_meet_the_contract() {
        bytes_contract(&MemoryBytes::default());
        let dir = std::env::temp_dir().join(format!("obop-blobs-{}", std::process::id()));
        bytes_contract(&FsBytes::new(&dir).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
        let db = std::env::temp_dir().join(format!("obop-blobdb-{}.db", std::process::id()));
        bytes_contract(&DbBytes::open(&format!("sqlite://{}", db.display())).unwrap());
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", db.display()));
        }
    }

    #[test]
    fn postgres_bytes_meet_the_contract_when_available() {
        let Ok(url) = std::env::var("OBOP_TEST_POSTGRES_URL") else {
            return;
        };
        bytes_contract(&DbBytes::open(&url).unwrap());
    }

    #[test]
    fn the_same_bytes_uploaded_twice_at_once_are_stored_once_and_both_uploads_succeed() {
        let Ok(url) = std::env::var("OBOP_TEST_POSTGRES_URL") else {
            return;
        };
        // D74: the key is the hash of the bytes, so two uploads of one file race on one key. The
        // second used to wait for the first at the primary key and fail with a duplicate key.
        let b = Arc::new(DbBytes::open(&url).unwrap());
        // Several chunks, so the writers overlap for a while.
        let bytes: Arc<Vec<u8>> = Arc::new(
            (0..(CHUNK * 24 + 5))
                .map(|i| (i % 249) as u8)
                .collect::<Vec<u8>>(),
        );
        let id = id_of(&bytes);
        let barrier = Arc::new(std::sync::Barrier::new(6));
        let hs: Vec<_> = (0..6)
            .map(|_| {
                let (b, bytes, id, barrier) =
                    (b.clone(), bytes.clone(), id.clone(), barrier.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    b.put(&id, &bytes)
                })
            })
            .collect();
        for h in hs {
            h.join()
                .unwrap()
                .expect("every upload of the same bytes succeeds");
        }
        let mut got = Vec::new();
        b.open(&id).unwrap().unwrap().read_to_end(&mut got).unwrap();
        assert!(got == *bytes, "stored once, whole");
        assert!(b.delete(&id).unwrap());
    }

    #[test]
    fn two_writers_cannot_both_attach_and_detach_inconsistently() {
        let c = Arc::new(catalog());
        let b = up(&c, b"shared", "u");
        let mut hs = Vec::new();
        for n in 0..8 {
            let (c, id) = (c.clone(), b.id.clone());
            hs.push(std::thread::spawn(move || {
                c.attach(&id, &Owner::new("m", &format!("r{n}"), "f"))
                    .unwrap();
            }));
        }
        for h in hs {
            h.join().unwrap();
        }
        assert_eq!(c.info(&b.id).unwrap().unwrap().owners.len(), 8);
    }
}
