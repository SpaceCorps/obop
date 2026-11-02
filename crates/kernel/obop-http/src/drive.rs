//! The Drive browser's three routes: whether a Drive is connected, a listing of it, and the import
//! of one file into an upload (`/api/drive/*`).
//!
//! The Drive itself is behind the [`DriveSource`] port, set by the composition root; this module
//! speaks HTTP and applies the rules:
//!
//! - a bearer token of a person (not an agent) is needed, as for the blob routes;
//! - only people in the browse group may look, because the connection is one Drive for the whole
//!   instance: `OBOP_DRIVE_BROWSE_GROUP` when set, else the group an administrator chose on the
//!   Google Drive page of the back office, else `platform.admin`;
//! - an import goes through [`obop_commands::BlobService::upload`], so the target field's own
//!   write rights, size limit, type check and upload rate limit apply, and the bytes are stored
//!   like any other upload; the caller then creates or updates the record with the returned id;
//! - every import is written to the security event log (`drive_import`).
//!
//! With a workspace drive chosen and the write scope granted, people in the write group can also
//! make a folder, an empty Google Doc or an uploaded file in it (`POST /api/drive/folders|docs|
//! upload`); every such write is logged (`drive_create`, `drive_upload`). Nothing already in Drive
//! is ever changed or deleted.

use super::{HttpState, blocking};
use axum::{
    extract::{Query, Request, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use obop_commands::{BlobError, PrincipalKind};
use serde_json::{Value as J, json};
use std::collections::HashMap;
use std::sync::Arc;

/// One row of a listing.
#[derive(Debug, Clone)]
pub struct DriveEntry {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub folder: bool,
    pub modified: String,
    pub size: Option<u64>,
    pub web_link: String,
    pub importable: bool,
}

/// One page of a listing.
#[derive(Debug, Clone, Default)]
pub struct DrivePage {
    pub entries: Vec<DriveEntry>,
    pub next: Option<String>,
}

/// A file brought in from Drive.
#[derive(Debug, Clone)]
pub struct DriveFile {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub modified: String,
    pub web_link: String,
    pub bytes: Vec<u8>,
}

/// A folder, Google Doc or file just made in Drive.
#[derive(Debug, Clone)]
pub struct DriveCreated {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub folder: bool,
    pub web_link: String,
}

/// The most an upload from the computer may be: what the Documents file field takes (25 MiB).
pub const UPLOAD_MAX: usize = 25 * 1024 * 1024;

fn unsupported<T>() -> Result<T, String> {
    Err("this Drive cannot do that".into())
}

/// A Drive the instance can read, set by the composition root.
pub trait DriveSource: Send + Sync {
    fn list(&self, parent: &str, search: &str, page: Option<&str>) -> Result<DrivePage, String>;
    fn fetch(&self, id: &str, max: usize) -> Result<DriveFile, String>;
    /// Whether a Drive is signed in right now (configured and authorised).
    fn connected(&self) -> bool {
        true
    }
    /// The group an administrator chose for browsing, when the source knows one.
    fn browse_group(&self) -> Option<String> {
        None
    }
    /// What the Google Drive page of the back office shows to an administrator (never a secret).
    fn settings(&self) -> Option<J> {
        None
    }
    /// The workspace drive (id and name) the instance works in, when one is chosen. Browsing,
    /// search and import are limited to it.
    fn workspace(&self) -> Option<(String, String)> {
        None
    }
    /// The group that may create things in the workspace, when the source knows one.
    fn write_group(&self) -> Option<String> {
        None
    }
    /// Whether the sign-in was granted the right to create things.
    fn can_write(&self) -> bool {
        false
    }
    /// The shared drives the account belongs to, as (id, name), for the workspace picker.
    fn drives(&self) -> Result<Vec<(String, String)>, String> {
        unsupported()
    }
    /// Make a folder in `parent` (empty: the top of the workspace).
    fn create_folder(&self, _parent: &str, _name: &str) -> Result<DriveCreated, String> {
        unsupported()
    }
    /// Make an empty Google Doc in `parent`.
    fn create_doc(&self, _parent: &str, _name: &str) -> Result<DriveCreated, String> {
        unsupported()
    }
    /// Upload a file into `parent`.
    fn upload(
        &self,
        _parent: &str,
        _name: &str,
        _mime: &str,
        _bytes: &[u8],
    ) -> Result<DriveCreated, String> {
        unsupported()
    }
}

/// The group that may browse: `OBOP_DRIVE_BROWSE_GROUP`, else the administrator's choice, else
/// `platform.admin`.
fn browse_group(src: &dyn DriveSource) -> String {
    std::env::var("OBOP_DRIVE_BROWSE_GROUP")
        .ok()
        .map(|g| g.trim().to_string())
        .filter(|g| !g.is_empty())
        .or_else(|| src.browse_group())
        .unwrap_or_else(|| "platform.admin".into())
}

/// The group that may create things in the workspace.
fn write_group(src: &dyn DriveSource) -> String {
    src.write_group().unwrap_or_else(|| "platform.admin".into())
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, axum::Json(json!({"error": message}))).into_response()
}

enum Gate {
    /// No Drive is connected.
    Off,
    Refused(Response),
    Open(obop_commands::Principal, Arc<dyn DriveSource>),
}

fn gate(s: &Arc<HttpState>, h: &HeaderMap) -> Gate {
    let (p, _) = match s.auth(h) {
        Ok(x) => x,
        Err(r) => return Gate::Refused(r.into_response()),
    };
    let Some(src) = s.drive.get().cloned().filter(|d| d.connected()) else {
        return Gate::Off;
    };
    if p.kind == PrincipalKind::Agent || !p.allows(&browse_group(&*src)) {
        return Gate::Refused(error(
            StatusCode::FORBIDDEN,
            "you may not browse the connected Google Drive",
        ));
    }
    Gate::Open(p, src)
}

/// Like [`gate`], for a write: a person in the write group, with a workspace chosen and the write
/// scope granted.
fn write_gate(s: &Arc<HttpState>, h: &HeaderMap) -> Gate {
    let (p, src) = match gate(s, h) {
        Gate::Open(p, src) => (p, src),
        other => return other,
    };
    if !p.allows(&write_group(&*src)) {
        return Gate::Refused(error(
            StatusCode::FORBIDDEN,
            "you may not create things in the connected Google Drive",
        ));
    }
    if src.workspace().is_none() {
        return Gate::Refused(error(
            StatusCode::CONFLICT,
            "choose a workspace drive on the Google Drive page first",
        ));
    }
    if !src.can_write() {
        return Gate::Refused(error(
            StatusCode::CONFLICT,
            "sign in with Google again to allow creating files",
        ));
    }
    Gate::Open(p, src)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn created_json(c: &DriveCreated) -> J {
    json!({"id": c.id, "name": c.name, "mime": c.mime, "folder": c.folder, "web_link": c.web_link})
}

/// `GET /api/drive/status`: `{"connected": bool, "may_browse": bool}`; the back office shows its
/// button from it. It also says which `workspace` drive is chosen (`{id, name}` or null) and
/// whether this person `may_write` there. An administrator of connectors also gets `may_configure` and `settings` (the
/// state of the Google Drive page: client id, whether a secret is stored, the signed-in account,
/// the redirect address to register with Google, the last error and the browse group).
pub async fn status(State(s): State<Arc<HttpState>>, h: HeaderMap) -> Response {
    blocking(move || {
        let (p, _) = match s.auth(&h) {
            Ok(x) => x,
            Err(r) => return r.into_response(),
        };
        let person = p.kind != PrincipalKind::Agent;
        let src = s.drive.get().cloned();
        let connected = src.as_ref().is_some_and(|d| d.connected());
        let may =
            connected && person && src.as_ref().is_some_and(|d| p.allows(&browse_group(&**d)));
        let may_configure = person && p.allows("connectors.admin");
        let workspace = src.as_ref().and_then(|d| d.workspace());
        let may_write = may
            && workspace.is_some()
            && src
                .as_ref()
                .is_some_and(|d| d.can_write() && p.allows(&write_group(&**d)));
        let mut out = json!({
            "connected": connected, "may_browse": may, "may_configure": may_configure,
            "may_write": may_write,
            "workspace": workspace.map(|(id, name)| json!({"id": id, "name": name})),
        });
        if may_configure && let Some(settings) = src.as_ref().and_then(|d| d.settings()) {
            out["settings"] = settings;
        }
        axum::Json(out).into_response()
    })
    .await
}

/// `GET /api/drive/files?parent=<folder id>&q=<name>&page=<token>`.
pub async fn files(
    State(s): State<Arc<HttpState>>,
    Query(q): Query<HashMap<String, String>>,
    h: HeaderMap,
) -> Response {
    blocking(move || {
        let src = match gate(&s, &h) {
            Gate::Off => return error(StatusCode::NOT_FOUND, "no Google Drive is connected"),
            Gate::Refused(r) => return r,
            Gate::Open(_, src) => src,
        };
        let get = |k: &str| q.get(k).map(String::as_str).unwrap_or("");
        let page = Some(get("page")).filter(|p| !p.is_empty());
        match src.list(get("parent"), get("q"), page) {
            Ok(p) => axum::Json(json!({
                "files": p.entries.iter().map(|e| json!({
                    "id": e.id, "name": e.name, "mime": e.mime, "folder": e.folder,
                    "modified": e.modified, "size": e.size, "web_link": e.web_link,
                    "importable": e.importable,
                })).collect::<Vec<_>>(),
                "next": p.next,
            }))
            .into_response(),
            Err(m) => error(StatusCode::BAD_GATEWAY, &m),
        }
    })
    .await
}

/// `POST /api/drive/import` with `{"id": "<Drive file id>", "field": "model.field"}`: fetch the
/// file and store it as an upload for `field`; the reply is the blob (`id`, `content_type`,
/// `size`) and where it came from (`drive`).
pub async fn import(
    State(s): State<Arc<HttpState>>,
    h: HeaderMap,
    axum::Json(body): axum::Json<J>,
) -> Response {
    blocking(move || {
        let (p, src) = match gate(&s, &h) {
            Gate::Off => return error(StatusCode::NOT_FOUND, "no Google Drive is connected"),
            Gate::Refused(r) => return r,
            Gate::Open(p, src) => (p, src),
        };
        let (Some(id), Some(field)) = (body["id"].as_str(), body["field"].as_str()) else {
            return error(
                StatusCode::BAD_REQUEST,
                "send {\"id\": ..., \"field\": \"model.field\"}",
            );
        };
        let Some(svc) = s.dispatcher.blob_service() else {
            return error(StatusCode::NOT_FOUND, "not found");
        };
        let max = svc.max_upload(field);
        if max == 0 {
            return error(
                StatusCode::BAD_REQUEST,
                "field must name an image or file field as model.field",
            );
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let file = match src.fetch(id, max) {
            Ok(f) => f,
            Err(m) => {
                obop_commands::audit::record_event(
                    &*s.shared.backend,
                    now,
                    &p.id,
                    "drive_import_failed",
                    &p.id,
                    &format!("{id} into {field}: {m}"),
                );
                return error(StatusCode::BAD_GATEWAY, &m);
            }
        };
        match svc.upload(&p, field, &file.mime, &file.name, &file.bytes) {
            Ok(info) => {
                obop_commands::audit::record_event(
                    &*s.shared.backend,
                    now,
                    &p.id,
                    "drive_import",
                    &p.id,
                    &format!("{} ({}) into {field} as {}", file.id, file.name, info.id),
                );
                (
                    StatusCode::CREATED,
                    axum::Json(json!({
                        "id": info.id, "content_type": info.content_type, "size": info.size,
                        "filename": obop_commands::clean_filename(&file.name),
                        "drive": {
                            "id": file.id, "name": file.name, "mime": file.mime,
                            "modified": file.modified, "web_link": file.web_link,
                        },
                    })),
                )
                    .into_response()
            }
            Err(BlobError::Bad(m)) => error(StatusCode::BAD_REQUEST, &m),
            Err(BlobError::TooLarge(m)) => error(
                StatusCode::PAYLOAD_TOO_LARGE,
                &format!("the file is larger than {} KiB", m / 1024),
            ),
            Err(BlobError::Unsupported(m)) => error(StatusCode::UNSUPPORTED_MEDIA_TYPE, &m),
            Err(BlobError::Forbidden) => {
                error(StatusCode::FORBIDDEN, "you may not upload to this field")
            }
            Err(BlobError::Rate(_)) => error(
                StatusCode::TOO_MANY_REQUESTS,
                "too many uploads, try again later",
            ),
            Err(_) => error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not store the file",
            ),
        }
    })
    .await
}

/// `GET /api/drive/drives` (connectors administrators): the shared drives the connected account
/// belongs to, for choosing the workspace drive.
pub async fn drives(State(s): State<Arc<HttpState>>, h: HeaderMap) -> Response {
    blocking(move || {
        let (p, _) = match s.auth(&h) {
            Ok(x) => x,
            Err(r) => return r.into_response(),
        };
        if p.kind == PrincipalKind::Agent || !p.allows("connectors.admin") {
            return error(StatusCode::FORBIDDEN, "only an administrator of connectors may do that");
        }
        let Some(src) = s.drive.get().cloned().filter(|d| d.connected()) else {
            return error(StatusCode::NOT_FOUND, "no Google Drive is connected");
        };
        match src.drives() {
            Ok(d) => axum::Json(json!({
                "drives": d.iter().map(|(id, name)| json!({"id": id, "name": name})).collect::<Vec<_>>(),
            }))
            .into_response(),
            Err(m) => error(StatusCode::BAD_GATEWAY, &m),
        }
    })
    .await
}

fn record(s: &HttpState, who: &str, kind: &str, detail: &str) {
    obop_commands::audit::record_event(&*s.shared.backend, now_secs(), who, kind, who, detail);
}

fn create_reply(
    s: &HttpState,
    who: &str,
    kind: &str,
    parent: &str,
    made: Result<DriveCreated, String>,
) -> Response {
    match made {
        Ok(c) => {
            record(
                s,
                who,
                kind,
                &format!(
                    "{} ({}) in {}",
                    c.id,
                    c.name,
                    if parent.is_empty() {
                        "the workspace"
                    } else {
                        parent
                    }
                ),
            );
            (StatusCode::CREATED, axum::Json(created_json(&c))).into_response()
        }
        Err(m) => {
            record(s, who, &format!("{kind}_failed"), &format!("{parent}: {m}"));
            error(StatusCode::BAD_GATEWAY, &m)
        }
    }
}

/// `POST /api/drive/folders` and `POST /api/drive/docs` with `{"name": ..., "parent": ...}`
/// (`parent` is optional: the top of the workspace).
async fn make(s: Arc<HttpState>, h: HeaderMap, body: J, doc: bool) -> Response {
    blocking(move || {
        let (p, src) = match write_gate(&s, &h) {
            Gate::Off => return error(StatusCode::NOT_FOUND, "no Google Drive is connected"),
            Gate::Refused(r) => return r,
            Gate::Open(p, src) => (p, src),
        };
        let Some(name) = body["name"].as_str() else {
            return error(
                StatusCode::BAD_REQUEST,
                "send {\"name\": ..., \"parent\": ...}",
            );
        };
        let parent = body["parent"].as_str().unwrap_or("");
        let made = if doc {
            src.create_doc(parent, name)
        } else {
            src.create_folder(parent, name)
        };
        create_reply(&s, &p.id, "drive_create", parent, made)
    })
    .await
}

pub async fn create_folder(
    State(s): State<Arc<HttpState>>,
    h: HeaderMap,
    axum::Json(body): axum::Json<J>,
) -> Response {
    make(s, h, body, false).await
}

pub async fn create_doc(
    State(s): State<Arc<HttpState>>,
    h: HeaderMap,
    axum::Json(body): axum::Json<J>,
) -> Response {
    make(s, h, body, true).await
}

/// `POST /api/drive/upload?parent=<folder id>&name=<file name>` with the file as the body and its
/// type as `Content-Type`: up to [`UPLOAD_MAX`].
pub async fn upload(
    State(s): State<Arc<HttpState>>,
    Query(q): Query<HashMap<String, String>>,
    req: Request,
) -> Response {
    let (parts, body) = req.into_parts();
    let headers = parts.headers;
    // Who is asking is decided before the body is read, so a refused caller buffers nothing.
    let (s2, h2) = (s.clone(), headers.clone());
    let (p, src) = match blocking(move || write_gate(&s2, &h2)).await {
        Gate::Off => return error(StatusCode::NOT_FOUND, "no Google Drive is connected"),
        Gate::Refused(r) => return r,
        Gate::Open(p, src) => (p, src),
    };
    if headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok())
        .is_some_and(|n| n > UPLOAD_MAX)
    {
        return error(
            StatusCode::PAYLOAD_TOO_LARGE,
            &format!("the file is larger than {} MiB", UPLOAD_MAX >> 20),
        );
    }
    let bytes = match axum::body::to_bytes(body, UPLOAD_MAX + 1).await {
        Ok(b) if b.len() <= UPLOAD_MAX => b,
        _ => {
            return error(
                StatusCode::PAYLOAD_TOO_LARGE,
                &format!("the file is larger than {} MiB", UPLOAD_MAX >> 20),
            );
        }
    };
    let name = q.get("name").cloned().unwrap_or_default();
    let parent = q.get("parent").cloned().unwrap_or_default();
    let mime = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap_or("").trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "application/octet-stream".into());
    blocking(move || {
        let made = src.upload(&parent, &name, &mime, &bytes);
        create_reply(&s, &p.id, "drive_upload", &parent, made)
    })
    .await
}
