//! Images and files held by records (D60): the rules around the blob store.
//!
//! - **Upload** (`BlobService::upload`): the caller names the target field (`model.field`) and
//!   needs write access to it; the field's kind decides what is accepted and how large. Images
//!   are decoded and re-encoded (`obop-media`), files are checked against a short list. Nothing
//!   is public and nothing is attached yet: the blob waits as an upload of that principal.
//! - **Reference** (a guard on `create` and `update` of models with such fields): a blob id may be
//!   saved into a field only by the principal who uploaded it, or when it is already held by this
//!   very field, or when the principal can already read a record that holds it. Knowing a hash is
//!   not enough, and a guessed id answers like a missing one.
//! - **Hold** (after `create`, `update`, `delete` and erasure): saving a record attaches its
//!   blobs, replacing a value detaches the old one, deleting a record detaches all of them; a
//!   blob nobody holds is deleted (see `obop_storage::blob`).
//! - **Read** (`BlobService::get`): a signed-in user who may read a record that holds the blob.
//!   Agents are refused: they get ids and metadata through the generated commands and never the
//!   bytes (a dedicated `blob.read` command is not part of obop and would have to be added and
//!   granted on purpose).
//! - **Public read** (`PublicApi::public_blob`): only blobs held by a *published* record through
//!   a field the package declared public, see `public.rs`.

use crate::engine::Models as ModelsMap;
use crate::store::{Store, id_of};
use crate::{Dispatcher, Guard, Principal, PrincipalKind, Value, Verdict};
use obop_media::{Limits, MediaError};
use obop_registry::{FieldKind, ModelDef};
use obop_storage::Fields;
use obop_storage::blob::{self, BlobInfo, NewBlob, Owner, THUMB_WIDTHS};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Why a blob request was refused. Each variant maps to one HTTP status in `obop-http`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlobError {
    /// 400: the request is malformed (unknown field, not a blob field).
    Bad(String),
    /// 413: larger than the field allows.
    TooLarge(usize),
    /// 415: not an accepted type, or not what it says it is.
    Unsupported(String),
    /// 404: no such blob, or it is not visible to the caller (the two look the same).
    NotFound,
    /// 403: the caller may not write the target field.
    Forbidden,
    /// 429: too many uploads; the number is the seconds to wait.
    Rate(u64),
    Storage(String),
}

/// A blob ready to be sent.
#[derive(Debug, Clone)]
pub struct Served {
    pub content_type: String,
    pub bytes: Vec<u8>,
    /// Strong validator (the content hash, plus the width for a thumbnail), quoted.
    pub etag: String,
    pub filename: String,
    /// Files are downloads, images are shown.
    pub attachment: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// Uploads one principal may make in ten minutes, unless the operator changes it.
pub const DEFAULT_UPLOADS_PER_WINDOW: usize = 40;
const WINDOW: Duration = Duration::from_secs(600);

/// A blob larger than this is never loaded whole by the HTTP layer.
const MAX_SERVE: u64 = 64 * 1024 * 1024;

pub struct BlobService {
    models: ModelsMap,
    store: Store,
    per_window: std::sync::atomic::AtomicUsize,
    hits: Mutex<HashMap<String, VecDeque<Instant>>>,
}

/// File name safe for storage and headers: the last path component, no control characters,
/// quotes or separators, at most 120 characters.
pub fn clean_filename(raw: &str) -> String {
    let last = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let s: String = last
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '"' | '<' | '>' | ':' | '|' | '?' | '*' | ';'))
        .take(120)
        .collect();
    let s = s.trim().trim_start_matches('.').to_string();
    if s.is_empty() { "file".into() } else { s }
}

/// `Content-Disposition` for a download: ASCII fallback plus the RFC 5987 form.
pub fn disposition(attachment: bool, filename: &str) -> String {
    let ascii: String = filename
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut enc = String::new();
    for b in filename.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            enc.push(b as char);
        } else {
            enc.push_str(&format!("%{b:02X}"));
        }
    }
    format!(
        "{}; filename=\"{}\"; filename*=UTF-8''{enc}",
        if attachment { "attachment" } else { "inline" },
        ascii.replace('"', "_")
    )
}

impl BlobService {
    pub fn new(d: &Dispatcher, store: Store) -> Arc<Self> {
        Arc::new(Self {
            models: d.models.clone(),
            store,
            per_window: std::sync::atomic::AtomicUsize::new(DEFAULT_UPLOADS_PER_WINDOW),
            hits: Mutex::default(),
        })
    }

    /// Change how many uploads one principal may make in ten minutes (tests, bulk imports).
    pub fn set_upload_limit(&self, per_ten_minutes: usize) {
        self.per_window
            .store(per_ten_minutes, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn blobs(&self) -> Arc<dyn blob::Blobs> {
        self.store.blobs()
    }

    fn target(&self, target: &str) -> Result<(Arc<ModelDef>, usize), BlobError> {
        let Some((model, field)) = target.split_once('.') else {
            return Err(BlobError::Bad("field must be model.field".into()));
        };
        let models = self.models.read().unwrap();
        let def = models
            .get(model)
            .map(|m| m.def.clone())
            .ok_or_else(|| BlobError::Bad(format!("no field {target}")))?;
        let idx = def
            .fields
            .iter()
            .position(|f| f.name == field && f.kind.is_blob())
            .ok_or_else(|| BlobError::Bad(format!("{target} is not an image or file field")))?;
        Ok((def, idx))
    }

    fn rate(&self, who: &str) -> Result<(), BlobError> {
        let now = Instant::now();
        let mut g = self.hits.lock().unwrap();
        if g.len() > 10_000 {
            g.retain(|_, q| q.back().is_some_and(|t| now.duration_since(*t) < WINDOW));
        }
        let q = g.entry(who.to_string()).or_default();
        while q.front().is_some_and(|t| now.duration_since(*t) >= WINDOW) {
            q.pop_front();
        }
        if q.len() >= self.per_window.load(std::sync::atomic::Ordering::Relaxed) {
            let wait = q.front().map_or(60, |t| {
                WINDOW
                    .saturating_sub(now.duration_since(*t))
                    .as_secs()
                    .max(1)
            });
            return Err(BlobError::Rate(wait));
        }
        q.push_back(now);
        Ok(())
    }

    /// The largest body `upload` could accept for `target` (for the HTTP body limit).
    pub fn max_upload(&self, target: &str) -> usize {
        self.target(target)
            .ok()
            .and_then(|(d, i)| d.fields[i].kind.max_bytes())
            .unwrap_or(0)
    }

    /// Store an upload for `target` (`model.field`). `declared` is the client's `Content-Type`.
    pub fn upload(
        &self,
        p: &Principal,
        target: &str,
        declared: &str,
        filename: &str,
        bytes: &[u8],
    ) -> Result<BlobInfo, BlobError> {
        let (def, idx) = self.target(target)?;
        let f = &def.fields[idx];
        if !p.allows(def.write_group) || !p.allows(f.permission_group) {
            return Err(BlobError::Forbidden);
        }
        self.rate(&p.id)?;
        let max = f.kind.max_bytes().unwrap_or(0);
        let media_err = |e: MediaError| match e {
            MediaError::TooLarge { max } => BlobError::TooLarge(max),
            e => BlobError::Unsupported(e.to_string()),
        };
        let (content_type, out, dims) = match &f.kind {
            FieldKind::Image(_) => {
                let lim = Limits {
                    max_bytes: max,
                    ..Limits::default()
                };
                let pi = obop_media::process_image(bytes, declared, &lim).map_err(media_err)?;
                (
                    pi.content_type.to_string(),
                    pi.bytes,
                    Some((pi.width, pi.height)),
                )
            }
            _ => {
                let ct = obop_media::check_file(bytes, declared, max).map_err(media_err)?;
                let dims = obop_media::dimensions(bytes);
                (ct.to_string(), bytes.to_vec(), dims)
            }
        };
        self.store
            .blobs
            .put(
                &out,
                NewBlob {
                    content_type,
                    width: dims.map(|d| d.0),
                    height: dims.map(|d| d.1),
                    uploader: p.id.clone(),
                    filename: clean_filename(filename),
                },
            )
            .map_err(BlobError::Storage)
    }

    /// Whether `p` may read the bytes of a blob.
    fn may_read(&self, p: &Principal, info: &BlobInfo) -> bool {
        if p.kind == PrincipalKind::Agent {
            return false;
        }
        if info.pending.iter().any(|(by, ..)| by == &p.id) {
            return true;
        }
        let models = self.models.read().unwrap();
        info.owners.iter().any(|o| can_read_owner(p, &models, o))
    }

    /// The blob for a signed-in caller; `width` asks for a thumbnail.
    pub fn get(&self, p: &Principal, id: &str, width: Option<u32>) -> Result<Served, BlobError> {
        let info = self
            .store
            .blobs
            .info(id)
            .map_err(BlobError::Storage)?
            .ok_or(BlobError::NotFound)?;
        if !self.may_read(p, &info) {
            return Err(BlobError::NotFound);
        }
        serve(&self.store, &info, width)
    }
}

fn can_read_owner(p: &Principal, models: &HashMap<String, crate::ModelMeta>, o: &Owner) -> bool {
    models.get(&o.model).is_some_and(|m| {
        p.allows(m.def.read_group)
            && m.def
                .fields
                .iter()
                .find(|f| f.name == o.field)
                .is_some_and(|f| p.allows(f.permission_group))
    })
}

/// Read a blob (or a cached thumbnail of it) into a response. Thumbnails are made on first use
/// from the stored, already cleaned image, with the same decoding limits, and cached.
pub(crate) fn serve(
    store: &Store,
    info: &BlobInfo,
    width: Option<u32>,
) -> Result<Served, BlobError> {
    let filename = info
        .owners
        .iter()
        .map(|o| o.filename.as_str())
        .chain(info.pending.iter().map(|p| p.2.as_str()))
        .find(|f| !f.is_empty())
        .unwrap_or("file")
        .to_string();
    let is_image = info.content_type == "image/png" || info.content_type == "image/jpeg";
    if let Some(w) = width {
        if !is_image || !THUMB_WIDTHS.contains(&w) {
            return Err(BlobError::NotFound);
        }
        // Never scale up: ask for the original when it is no wider than the request.
        if info.width.is_some_and(|ow| ow > w) {
            let variant = format!("w{w}");
            let bytes = match store.blobs.derived(&info.id, &variant) {
                Ok(Some(b)) => b,
                _ => {
                    let original = store
                        .blobs
                        .read_all(&info.id, MAX_SERVE)
                        .map_err(BlobError::Storage)?
                        .ok_or(BlobError::NotFound)?;
                    let t = obop_media::thumbnail(&original, w, &Limits::default())
                        .map_err(|e| BlobError::Unsupported(e.to_string()))?;
                    let _ = store.blobs.put_derived(&info.id, &variant, &t.bytes);
                    t.bytes
                }
            };
            return Ok(Served {
                content_type: info.content_type.clone(),
                etag: format!("\"{}-w{w}\"", info.id),
                bytes,
                filename,
                attachment: false,
                width: Some(w),
                height: info
                    .height
                    .zip(info.width)
                    .map(|(h, ow)| ((u64::from(h) * u64::from(w)) / u64::from(ow)).max(1) as u32),
            });
        }
    }
    let bytes = store
        .blobs
        .read_all(&info.id, MAX_SERVE)
        .map_err(BlobError::Storage)?
        .ok_or(BlobError::NotFound)?;
    Ok(Served {
        content_type: info.content_type.clone(),
        etag: format!("\"{}\"", info.id),
        bytes,
        filename,
        attachment: !is_image,
        width: info.width,
        height: info.height,
    })
}

// ---- validation, references and holds -------------------------------------------------------

/// The shape check of `validate`: a well-formed id of an existing blob of the right kind.
pub(crate) fn check_field_value(
    store: &Store,
    model: &str,
    name: &str,
    kind: &FieldKind,
    id: &str,
) -> Result<(), String> {
    let bad = || format!("{model}.{name} must be {}", kind.describe());
    if !blob::valid_id(id) {
        return Err(bad());
    }
    let info = store.blobs.info(id)?.ok_or_else(bad)?;
    if matches!(kind, FieldKind::Image(_))
        && !matches!(info.content_type.as_str(), "image/png" | "image/jpeg")
    {
        return Err(bad());
    }
    Ok(())
}

fn blob_fields(def: &ModelDef) -> Vec<&obop_registry::FieldDef> {
    def.fields.iter().filter(|f| f.kind.is_blob()).collect()
}

/// The guard that decides who may put which blob into a field (see the module comment). `None`
/// for a model without image or file fields.
pub(crate) fn reference_guard(def: &ModelDef, store: &Store, models: ModelsMap) -> Option<Guard> {
    if blob_fields(def).is_empty() {
        return None;
    }
    let (def, store) = (def.clone(), store.clone());
    Some(Box::new(move |p, input| {
        let Value::Map(m) = input else {
            return Ok(Verdict::default());
        };
        let record = id_of(input).ok();
        for f in blob_fields(&def) {
            let Some(Value::Text(id)) = m.get(f.name) else {
                continue;
            };
            let none = || format!("{}.{} must be {}", def.name, f.name, f.kind.describe());
            let info = blob::valid_id(id)
                .then(|| store.blobs.info(id))
                .transpose()?
                .flatten()
                .ok_or_else(none)?;
            let by_me = info.pending.iter().any(|(by, ..)| by == &p.id);
            let held_here = record.as_deref().is_some_and(|r| {
                info.owners
                    .iter()
                    .any(|o| o.model == def.name && o.record == r && o.field == f.name)
            });
            let readable = || {
                let models = models.read().unwrap();
                info.owners.iter().any(|o| can_read_owner(p, &models, o))
            };
            if !(by_me || held_here || readable()) {
                return Err(none());
            }
        }
        Ok(Verdict::default())
    }))
}

fn owner_of(def: &ModelDef, record: &str, field: &str, filename: &str) -> Owner {
    Owner {
        filename: filename.to_string(),
        ..Owner::new(def.name, record, field)
    }
}

fn text_of(v: Option<&serde_json::Value>) -> Option<&str> {
    v.and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
}

/// After a record was created with blob values: hold them. The file name travels from the upload.
pub(crate) fn hold_created(store: &Store, def: &ModelDef, id: &str, rec: &Fields) {
    for f in blob_fields(def) {
        if let Some(b) = text_of(rec.get(f.name)) {
            let name = upload_name(store, b);
            let _ = store.blobs.attach(b, &owner_of(def, id, f.name, &name));
        }
    }
}

/// After an update: detach what was replaced or cleared, attach what is new.
pub(crate) fn hold_updated(
    store: &Store,
    def: &ModelDef,
    id: &str,
    before: &Fields,
    after: &Fields,
) {
    for f in blob_fields(def) {
        let (old, new) = (text_of(before.get(f.name)), text_of(after.get(f.name)));
        if old == new {
            continue;
        }
        if let Some(o) = old {
            let _ = store.blobs.detach(o, &owner_of(def, id, f.name, ""));
        }
        if let Some(n) = new {
            let name = upload_name(store, n);
            let _ = store.blobs.attach(n, &owner_of(def, id, f.name, &name));
        }
    }
}

/// Before a record goes away: release everything it held.
pub(crate) fn release_record(store: &Store, model: &str, id: &str) {
    let _ = store.blobs.detach_record(model, id);
}

fn upload_name(store: &Store, id: &str) -> String {
    store
        .blobs
        .info(id)
        .ok()
        .flatten()
        .and_then(|i| {
            i.pending
                .iter()
                .map(|p| p.2.clone())
                .chain(i.owners.iter().map(|o| o.filename.clone()))
                .find(|f| !f.is_empty())
        })
        .unwrap_or_default()
}

impl Store {
    /// Erase the image and file values of one record (a person asked to be forgotten): the fields
    /// are cleared and the blobs are released, which deletes them when nothing else holds them.
    /// Returns how many blob fields were cleared.
    pub fn erase_blob_fields(&self, def: &ModelDef, id: &str) -> Result<usize, String> {
        let Some(rec) = self.backend.get(def.name, id)? else {
            return Ok(0);
        };
        let mut patch = Fields::new();
        for f in blob_fields(def) {
            if rec.get(f.name).is_some_and(|v| !v.is_null()) {
                patch.insert(f.name.to_string(), serde_json::Value::Null);
            }
        }
        let n = patch.len();
        if n > 0 {
            self.backend.update(def.name, id, &patch)?;
        }
        self.blobs.detach_record(def.name, id)?;
        Ok(n)
    }
}

impl Dispatcher {
    /// [`Store::erase_blob_fields`] for a registered model.
    pub fn erase_blob_fields(&self, store: &Store, model: &str, id: &str) -> Result<usize, String> {
        let def = self
            .model_def(model)
            .ok_or_else(|| format!("no model {model}"))?;
        store.erase_blob_fields(&def, id)
    }
}

#[cfg(test)]
mod tests;
