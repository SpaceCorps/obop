//! Read-only browsing of a Google Drive: list a folder, search by name, and fetch one file (a Google
//! Doc, Sheet or Slides file is exported as PDF). It is what the back office's Drive browser
//! calls to bring a document into the Documents app.
//!
//! It signs in with its own refresh token, `OBOP_GDRIVE_BROWSE_REFRESH_TOKEN`, minted with the
//! `drive.readonly` scope (`obop storage drive-authorize --scope drive.readonly`); the client id and
//! secret are the ones of the file store (`OBOP_GDRIVE_CLIENT_ID`, `OBOP_GDRIVE_CLIENT_SECRET`).
//! Nothing is ever written to Drive. Not run against the real Google service: the tests use a
//! fake Drive on a loopback port.

use super::{API, Config, DriveBytes, FOLDER_MIME, TOKEN_URL, clean, need_env, valid_drive_id};
use obop_storage::Result;
use serde_json::Value as J;
use std::io::Read;
use std::time::Duration;

const APPS: &str = "application/vnd.google-apps.";
/// The Google file types Drive can export as PDF.
const EXPORTABLE: [&str; 4] = ["document", "spreadsheet", "presentation", "drawing"];

/// One row of a listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub folder: bool,
    /// RFC 3339, as Drive states it.
    pub modified: String,
    pub size: Option<u64>,
    /// The address of the file in Google's own viewer.
    pub web_link: String,
    /// Whether [`DriveBrowser::fetch`] can bring it in: a plain file or an exportable Google file.
    pub importable: bool,
}

/// One page of a listing; `next` is the token to ask the following page with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Page {
    pub entries: Vec<Entry>,
    pub next: Option<String>,
}

/// A file brought in, with the name and type it will have in obop.
#[derive(Debug, Clone)]
pub struct Fetched {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub modified: String,
    pub web_link: String,
    pub bytes: Vec<u8>,
}

pub struct DriveBrowser {
    drive: DriveBytes,
}

impl DriveBrowser {
    /// The browser the environment configures, or `None` when `OBOP_GDRIVE_BROWSE_REFRESH_TOKEN` is
    /// not set (the feature is then off).
    pub fn from_env() -> Result<Option<Self>> {
        match std::env::var("OBOP_GDRIVE_BROWSE_REFRESH_TOKEN") {
            Ok(t) if !t.trim().is_empty() => {}
            _ => return Ok(None),
        }
        Ok(Some(Self::new(Config {
            // Unused here: a browser is not tied to a folder.
            folder_id: "root".into(),
            client_id: need_env("OBOP_GDRIVE_CLIENT_ID")?,
            client_secret: need_env("OBOP_GDRIVE_CLIENT_SECRET")?,
            refresh_token: need_env("OBOP_GDRIVE_BROWSE_REFRESH_TOKEN")?,
            api_base: loopback_override("OBOP_GDRIVE_API_BASE").unwrap_or_else(|| API.into()),
            token_url: loopback_override("OBOP_GDRIVE_TOKEN_URL")
                .unwrap_or_else(|| TOKEN_URL.into()),
            retry_pause: Duration::from_millis(500),
            max_attempts: 4,
        })?))
    }

    pub fn new(cfg: Config) -> Result<Self> {
        Ok(Self {
            drive: DriveBytes::new(cfg)?,
        })
    }

    /// A reachability check for `obop storage check`: who the token belongs to.
    pub fn probe(&self) -> Result<String> {
        let r = self.drive.call(&|agent, token| {
            agent
                .get(format!("{}/drive/v3/about", self.drive.cfg.api_base))
                .header("Authorization", format!("Bearer {token}"))
                .query("fields", "user(emailAddress)")
                .call()
        })?;
        let v = DriveBytes::json(DriveBytes::ok(r)?)?;
        Ok(v["user"]["emailAddress"]
            .as_str()
            .map(|s| clean(s, 120))
            .unwrap_or_else(|| "(unknown)".into()))
    }

    /// The children of `parent` (a folder id, or `root`), or, when `search` is not empty, the
    /// files anywhere whose name contains it. Folders come first.
    pub fn list(&self, parent: &str, search: &str, page: Option<&str>) -> Result<Page> {
        let parent = if parent.is_empty() { "root" } else { parent };
        if !valid_drive_id(parent) {
            return Err("that is not a Drive folder id".into());
        }
        let search = search.trim();
        let q = if search.is_empty() {
            format!("'{parent}' in parents and trashed = false")
        } else {
            let esc = search.replace('\\', "\\\\").replace('\'', "\\'");
            format!("name contains '{esc}' and trashed = false")
        };
        let r = self.drive.call(&|agent, token| {
            let mut req = agent
                .get(format!("{}/drive/v3/files", self.drive.cfg.api_base))
                .header("Authorization", format!("Bearer {token}"))
                .query("q", &q)
                .query(
                    "fields",
                    "nextPageToken,files(id,name,mimeType,modifiedTime,size,webViewLink)",
                )
                .query("orderBy", "folder,name")
                .query("pageSize", "50");
            if let Some(p) = page {
                req = req.query("pageToken", p);
            }
            req.call()
        })?;
        let v = DriveBytes::json(DriveBytes::ok(r)?)?;
        let entries = v["files"]
            .as_array()
            .map(|a| a.iter().filter_map(entry).collect())
            .unwrap_or_default();
        Ok(Page {
            entries,
            next: v["nextPageToken"].as_str().map(str::to_string),
        })
    }

    /// Bring one file in, at most `max` bytes. A Google Doc, Sheet, Slides or Drawing comes as PDF.
    pub fn fetch(&self, id: &str, max: usize) -> Result<Fetched> {
        if !valid_drive_id(id) {
            return Err("that is not a Drive file id".into());
        }
        let r = self.drive.call(&|agent, token| {
            agent
                .get(format!("{}/drive/v3/files/{id}", self.drive.cfg.api_base))
                .header("Authorization", format!("Bearer {token}"))
                .query(
                    "fields",
                    "id,name,mimeType,modifiedTime,size,webViewLink,trashed",
                )
                .call()
        })?;
        let meta = DriveBytes::json(DriveBytes::ok(r)?)?;
        let e = entry(&meta).ok_or("Google Drive sent a file without a name")?;
        if meta["trashed"].as_bool() == Some(true) {
            return Err("that file is in the Drive bin".into());
        }
        if !e.importable {
            return Err(if e.folder {
                "that is a folder".into()
            } else {
                "obop cannot bring that kind of Google file in (only Docs, Sheets, Slides, Drawings and ordinary files)"
                    .into()
            });
        }
        if e.size.is_some_and(|s| s as usize > max) {
            return Err(format!(
                "the file is larger than {} KiB, the most a document may hold",
                max / 1024
            ));
        }
        let export = e.mime.starts_with(APPS);
        let r = self.drive.call(&|agent, token| {
            let req = if export {
                agent
                    .get(format!(
                        "{}/drive/v3/files/{id}/export",
                        self.drive.cfg.api_base
                    ))
                    .query("mimeType", "application/pdf")
            } else {
                agent
                    .get(format!("{}/drive/v3/files/{id}", self.drive.cfg.api_base))
                    .query("alt", "media")
            };
            req.header("Authorization", format!("Bearer {token}"))
                .call()
        })?;
        let mut body = DriveBytes::ok(r)?.into_body().into_reader();
        let mut bytes = Vec::new();
        body.by_ref()
            .take(max as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "reading the file from Google Drive failed")?;
        if bytes.len() > max {
            return Err(format!(
                "the file is larger than {} KiB, the most a document may hold",
                max / 1024
            ));
        }
        let (name, mime) = if export {
            let n = if e.name.to_lowercase().ends_with(".pdf") {
                e.name.clone()
            } else {
                format!("{}.pdf", e.name)
            };
            (n, "application/pdf".to_string())
        } else {
            (e.name.clone(), e.mime.clone())
        };
        Ok(Fetched {
            id: e.id,
            name,
            mime,
            modified: e.modified,
            web_link: e.web_link,
            bytes,
        })
    }
}

/// `OBOP_GDRIVE_API_BASE` and `OBOP_GDRIVE_TOKEN_URL` point the browser at a fake Drive for the end
/// to end tests. They are honoured only for a loopback address, so no setting can send the
/// credentials anywhere else.
fn loopback_override(name: &str) -> Option<String> {
    let v = std::env::var(name).ok()?;
    let v = v.trim().trim_end_matches('/');
    (v.starts_with("http://127.0.0.1:") || v.starts_with("http://localhost:"))
        .then(|| v.to_string())
}

fn entry(f: &J) -> Option<Entry> {
    let id = f["id"].as_str().filter(|i| valid_drive_id(i))?;
    let name = f["name"].as_str()?;
    let mime = f["mimeType"].as_str().unwrap_or("application/octet-stream");
    let folder = mime == FOLDER_MIME;
    let google = mime.starts_with(APPS);
    let importable = !folder
        && (!google
            || mime
                .strip_prefix(APPS)
                .is_some_and(|k| EXPORTABLE.contains(&k)));
    Some(Entry {
        id: id.into(),
        name: clean(name, 200),
        mime: mime.into(),
        folder,
        modified: f["modifiedTime"].as_str().unwrap_or("").into(),
        size: f["size"].as_str().and_then(|s| s.parse().ok()),
        web_link: f["webViewLink"].as_str().unwrap_or("").into(),
        importable,
    })
}
