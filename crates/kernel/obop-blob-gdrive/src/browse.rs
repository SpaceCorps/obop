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
use serde_json::json;
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

/// The one drive an instance works in. When set, listing, search, fetch and create refuse anything
/// outside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Workspace {
    /// The connected account's own My Drive: only files the account owns.
    MyDrive,
    /// A shared drive, by its id.
    Shared(String),
}

impl Workspace {
    /// `""` is no workspace, `"root"` is My Drive, anything else a shared drive id.
    pub fn parse(id: &str) -> Option<Self> {
        match id.trim() {
            "" => None,
            "root" => Some(Self::MyDrive),
            d if valid_drive_id(d) => Some(Self::Shared(d.into())),
            _ => None,
        }
    }

    /// The folder id the workspace's top level is listed from.
    fn root(&self) -> &str {
        match self {
            Self::MyDrive => "root",
            Self::Shared(d) => d,
        }
    }
}

/// A shared drive the account belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedDrive {
    pub id: String,
    pub name: String,
}

/// A file or folder just made in Drive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Created {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub folder: bool,
    pub web_link: String,
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
        self.list_in(None, parent, search, page)
    }

    /// As [`Self::list`], inside `ws` when there is one: the top level is the workspace, a search
    /// looks only there, and a folder outside it lists nothing.
    pub fn list_in(
        &self,
        ws: Option<&Workspace>,
        parent: &str,
        search: &str,
        page: Option<&str>,
    ) -> Result<Page> {
        let top = ws.map_or("root", Workspace::root);
        let parent = if parent.is_empty() { top } else { parent };
        if !valid_drive_id(parent) {
            return Err("that is not a Drive folder id".into());
        }
        let search = search.trim();
        let mine = if ws == Some(&Workspace::MyDrive) {
            " and 'me' in owners"
        } else {
            ""
        };
        let q = if search.is_empty() {
            format!("'{parent}' in parents and trashed = false{mine}")
        } else {
            let esc = search.replace('\\', "\\\\").replace('\'', "\\'");
            format!("name contains '{esc}' and trashed = false{mine}")
        };
        let root = ws.is_none() && parent == "root" && search.is_empty() && page.is_none();
        let r = self.drive.call(&|agent, token| {
            let mut req = agent
                .get(format!("{}/drive/v3/files", self.drive.cfg.api_base))
                .header("Authorization", format!("Bearer {token}"))
                .query("q", &q)
                .query("supportsAllDrives", "true")
                .query("includeItemsFromAllDrives", "true")
                .query(
                    "fields",
                    "nextPageToken,files(id,name,mimeType,modifiedTime,size,webViewLink)",
                )
                .query("orderBy", "folder,name")
                .query("pageSize", "50");
            // Inside a folder of a shared drive, and in a search, Drive must look at every drive the
            // person can see; the plain "my files" corpus would not find them.
            match ws {
                Some(Workspace::Shared(d)) => {
                    req = req.query("corpora", "drive").query("driveId", d.as_str());
                }
                Some(Workspace::MyDrive) => {}
                None if parent != "root" || !search.is_empty() => {
                    req = req.query("corpora", "allDrives");
                }
                None => {}
            }
            if let Some(p) = page {
                req = req.query("pageToken", p);
            }
            req.call()
        })?;
        let v = DriveBytes::json(DriveBytes::ok(r)?)?;
        let mut entries: Vec<Entry> = if root {
            self.shared_drives()
        } else {
            Vec::new()
        };
        entries.extend(
            v["files"]
                .as_array()
                .map(|a| a.iter().filter_map(entry).collect::<Vec<_>>())
                .unwrap_or_default(),
        );
        Ok(Page {
            entries,
            next: v["nextPageToken"].as_str().map(str::to_string),
        })
    }

    /// The shared drives the account belongs to (the first 100), for the workspace picker.
    pub fn drives(&self) -> Result<Vec<SharedDrive>> {
        let r = self.drive.call(&|agent, token| {
            agent
                .get(format!("{}/drive/v3/drives", self.drive.cfg.api_base))
                .header("Authorization", format!("Bearer {token}"))
                .query("fields", "drives(id,name)")
                .query("pageSize", "100")
                .call()
        })?;
        let v = DriveBytes::json(DriveBytes::ok(r)?)?;
        Ok(v["drives"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|d| {
                        let id = d["id"].as_str().filter(|i| valid_drive_id(i))?;
                        Some(SharedDrive {
                            id: id.into(),
                            name: clean(d["name"].as_str()?, 200),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// The shared drives as folder rows at the top of the root listing. A failure only hides
    /// them: My Drive still lists.
    fn shared_drives(&self) -> Vec<Entry> {
        self.drives()
            .unwrap_or_default()
            .into_iter()
            .map(|d| Entry {
                web_link: format!("https://drive.google.com/drive/folders/{}", d.id),
                id: d.id,
                name: d.name,
                mime: FOLDER_MIME.into(),
                folder: true,
                modified: String::new(),
                size: None,
                importable: false,
            })
            .collect()
    }

    /// Bring one file in, at most `max` bytes. A Google Doc, Sheet, Slides or Drawing comes as PDF.
    pub fn fetch(&self, id: &str, max: usize) -> Result<Fetched> {
        self.fetch_in(None, id, max)
    }

    /// As [`Self::fetch`], refusing a file that is not in `ws`.
    pub fn fetch_in(&self, ws: Option<&Workspace>, id: &str, max: usize) -> Result<Fetched> {
        if !valid_drive_id(id) {
            return Err("that is not a Drive file id".into());
        }
        let r = self.drive.call(&|agent, token| {
            agent
                .get(format!("{}/drive/v3/files/{id}", self.drive.cfg.api_base))
                .header("Authorization", format!("Bearer {token}"))
                .query(
                    "fields",
                    "id,name,mimeType,modifiedTime,size,webViewLink,trashed,driveId,ownedByMe",
                )
                .query("supportsAllDrives", "true")
                .call()
        })?;
        let meta = DriveBytes::json(DriveBytes::ok(r)?)?;
        if let Some(ws) = ws {
            inside(ws, &meta)?;
        }
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
                    .query("supportsAllDrives", "true")
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

    /// The folder a new item goes into: the workspace's top level when `parent` is empty, else a
    /// folder that is really inside the workspace.
    fn parent_in(&self, ws: &Workspace, parent: &str) -> Result<String> {
        let parent = if parent.is_empty() { ws.root() } else { parent };
        if !valid_drive_id(parent) {
            return Err("that is not a Drive folder id".into());
        }
        if parent == ws.root() {
            return Ok(parent.into());
        }
        let r = self.drive.call(&|agent, token| {
            agent
                .get(format!(
                    "{}/drive/v3/files/{parent}",
                    self.drive.cfg.api_base
                ))
                .header("Authorization", format!("Bearer {token}"))
                .query("fields", "id,mimeType,trashed,driveId,ownedByMe")
                .query("supportsAllDrives", "true")
                .call()
        })?;
        let meta = DriveBytes::json(DriveBytes::ok(r)?)?;
        if meta["mimeType"].as_str() != Some(FOLDER_MIME) || meta["trashed"].as_bool() == Some(true)
        {
            return Err("that is not a folder you can add to".into());
        }
        inside(ws, &meta)?;
        Ok(parent.into())
    }

    fn created(&self, body: &J, fallback: &str) -> Result<Created> {
        let id = body["id"]
            .as_str()
            .filter(|i| valid_drive_id(i))
            .ok_or("Google Drive did not return the new item's id")?;
        let mime = body["mimeType"].as_str().unwrap_or("");
        Ok(Created {
            id: id.into(),
            name: clean(body["name"].as_str().unwrap_or(fallback), 200),
            folder: mime == FOLDER_MIME,
            mime: mime.into(),
            web_link: body["webViewLink"].as_str().unwrap_or("").into(),
        })
    }

    /// Make a folder in `parent` (empty: the top of the workspace).
    pub fn create_folder_in(&self, ws: &Workspace, parent: &str, name: &str) -> Result<Created> {
        self.create_json(ws, parent, name, FOLDER_MIME)
    }

    /// Make an empty Google Doc in `parent`.
    pub fn create_doc_in(&self, ws: &Workspace, parent: &str, name: &str) -> Result<Created> {
        self.create_json(ws, parent, name, "application/vnd.google-apps.document")
    }

    fn create_json(&self, ws: &Workspace, parent: &str, name: &str, mime: &str) -> Result<Created> {
        let name = item_name(name)?;
        let parent = self.parent_in(ws, parent)?;
        let body = json!({ "name": name, "mimeType": mime, "parents": [parent] }).to_string();
        let r = self.drive.call(&|agent, token| {
            agent
                .post(format!("{}/drive/v3/files", self.drive.cfg.api_base))
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .query("supportsAllDrives", "true")
                .query("fields", "id,name,mimeType,webViewLink")
                .send(body.as_bytes())
        })?;
        self.created(&DriveBytes::json(DriveBytes::ok(r)?)?, &name)
    }

    /// Upload `bytes` as a file called `name` into `parent`.
    pub fn upload_in(
        &self,
        ws: &Workspace,
        parent: &str,
        name: &str,
        mime: &str,
        bytes: &[u8],
    ) -> Result<Created> {
        let name = item_name(name)?;
        if mime.is_empty() || mime.len() > 120 || mime.chars().any(|c| c.is_control() || c == ' ') {
            return Err("the file type is not valid".into());
        }
        let parent = self.parent_in(ws, parent)?;
        let mut boundary = [0u8; 16];
        getrandom::fill(&mut boundary).map_err(|e| e.to_string())?;
        let boundary: String = boundary.iter().map(|b| format!("{b:02x}")).collect();
        let meta = json!({ "name": name, "parents": [parent] }).to_string();
        let mut body = Vec::with_capacity(bytes.len() + meta.len() + 200);
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n--{boundary}\r\nContent-Type: {mime}\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--").as_bytes());
        let r = self.drive.call(&|agent, token| {
            agent
                .post(format!("{}/upload/drive/v3/files", self.drive.cfg.api_base))
                .header("Authorization", format!("Bearer {token}"))
                .header(
                    "Content-Type",
                    format!("multipart/related; boundary={boundary}"),
                )
                .query("uploadType", "multipart")
                .query("supportsAllDrives", "true")
                .query("fields", "id,name,mimeType,webViewLink")
                .send(&body[..])
        })?;
        self.created(&DriveBytes::json(DriveBytes::ok(r)?)?, &name)
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

/// A name for a new item: one line, 1 to 200 characters.
fn item_name(name: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > 200 || n.chars().any(char::is_control) {
        return Err("the name must be 1 to 200 characters on one line".into());
    }
    Ok(n.to_string())
}

/// Whether the file or folder described by `meta` (with `driveId` and `ownedByMe`) is in `ws`.
fn inside(ws: &Workspace, meta: &J) -> Result<()> {
    let ok = match ws {
        Workspace::Shared(d) => meta["driveId"].as_str() == Some(d.as_str()),
        Workspace::MyDrive => {
            meta["driveId"].is_null() && meta["ownedByMe"].as_bool() == Some(true)
        }
    };
    if ok {
        Ok(())
    } else {
        Err("that item is outside the workspace drive".into())
    }
}
