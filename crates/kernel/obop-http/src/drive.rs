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
//! Nothing is ever written to Drive.

use super::{HttpState, blocking};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
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

/// `GET /api/drive/status`: `{"connected": bool, "may_browse": bool}`; the back office shows its
/// button from it. An administrator of connectors also gets `may_configure` and `settings` (the
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
        let mut out =
            json!({"connected": connected, "may_browse": may, "may_configure": may_configure});
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
