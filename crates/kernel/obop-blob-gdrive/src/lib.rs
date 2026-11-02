//! Uploaded files on Google Drive: one more implementation of
//! [`obop_storage::blob::BlobBytes`] (D60), so pictures and files can live in a Drive folder while
//! the records stay in PostgreSQL.
//!
//! Each blob is one Drive file inside one folder, named by its key (`<64 hex>` or
//! `<64 hex>.w<width>`). The store talks to the Drive v3 REST API with an OAuth2 refresh token
//! (`OBOP_GDRIVE_CLIENT_ID`, `OBOP_GDRIVE_CLIENT_SECRET`, `OBOP_GDRIVE_REFRESH_TOKEN`, all of them
//! secrets) and mints short-lived access tokens itself. Requests that fail with 429, 5xx or a Drive
//! rate-limit 403 are retried a bounded number of times with a growing pause; a 401 refreshes the
//! access token once. No credential is ever put in a URL, a log line or an error message.
//!
//! Select it with `OBOP_BLOB_STORE=gdrive://<FOLDER_ID>` ([`open_configured`]). Nothing here has
//! been run against the real Google service: the tests use a fake Drive on a loopback port.

mod authorize;
mod browse;
#[cfg(test)]
mod browse_tests;
#[cfg(test)]
mod tests;

pub use authorize::{AuthorizeOptions, authorize};
pub use browse::{DriveBrowser, Entry, Fetched, Page};

use obop_storage::Result;
use obop_storage::blob::{BlobBytes, valid_key};
use serde_json::{Value as J, json};
use std::collections::{BTreeSet, HashMap};
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The scheme of the `OBOP_BLOB_STORE` setting.
pub const SCHEME: &str = "gdrive://";

const API: &str = "https://www.googleapis.com";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const FOLDER_MIME: &str = "application/vnd.google-apps.folder";

/// Where the store is and who it acts as.
#[derive(Clone)]
pub struct Config {
    pub folder_id: String,
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    /// `https://www.googleapis.com` (a loopback address in tests).
    pub api_base: String,
    /// Where access tokens are minted (a loopback address in tests).
    pub token_url: String,
    /// The first pause between two attempts; it doubles each time.
    pub retry_pause: Duration,
    /// Attempts per request, the first included.
    pub max_attempts: u32,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("folder_id", &self.folder_id)
            .field("api_base", &self.api_base)
            .field("credentials", &"<redacted>")
            .finish()
    }
}

/// A Drive id is letters, digits, `-` and `_`; anything else is refused before it reaches a URL.
fn valid_drive_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

fn need_env(name: &str) -> Result<String> {
    match std::env::var(name) {
        Ok(v) if !v.trim().is_empty() => Ok(v.trim().to_string()),
        _ => Err(format!(
            "{name} is not set: Google Drive file storage needs OBOP_GDRIVE_CLIENT_ID, OBOP_GDRIVE_CLIENT_SECRET and OBOP_GDRIVE_REFRESH_TOKEN (see docs/DEPLOY.md, File storage)"
        )),
    }
}

impl Config {
    /// The credentials from the environment, the folder from `folder_id`.
    pub fn from_env(folder_id: &str) -> Result<Self> {
        if !valid_drive_id(folder_id) {
            return Err(
                "OBOP_BLOB_STORE: the folder id after gdrive:// is empty or has characters a Drive id never has"
                    .into(),
            );
        }
        Ok(Self {
            folder_id: folder_id.into(),
            client_id: need_env("OBOP_GDRIVE_CLIENT_ID")?,
            client_secret: need_env("OBOP_GDRIVE_CLIENT_SECRET")?,
            refresh_token: need_env("OBOP_GDRIVE_REFRESH_TOKEN")?,
            api_base: API.into(),
            token_url: TOKEN_URL.into(),
            retry_pause: Duration::from_millis(500),
            max_attempts: 5,
        })
    }
}

/// Choose where uploaded bytes go for the database at `url`, with the settings of the
/// environment: `OBOP_BLOB_STORE` (`gdrive://<folder id>`) or else the rules of
/// [`obop_storage::blob::open_bytes`] with `OBOP_BLOB_DIR`. Setting both is refused, because
/// neither could be said to win.
pub fn open_configured(url: &str) -> Result<Arc<dyn BlobBytes>> {
    let dir = std::env::var("OBOP_BLOB_DIR").ok();
    let store = std::env::var("OBOP_BLOB_STORE").ok();
    open_with(url, dir.as_deref(), store.as_deref())
}

/// [`open_configured`] with the two settings given instead of read.
pub fn open_with(url: &str, dir: Option<&str>, store: Option<&str>) -> Result<Arc<dyn BlobBytes>> {
    let store = store.map(str::trim).filter(|s| !s.is_empty());
    let dir = dir.map(str::trim).filter(|s| !s.is_empty());
    let Some(store) = store else {
        return obop_storage::blob::open_bytes(url, dir);
    };
    if dir.is_some() {
        return Err(
            "OBOP_BLOB_STORE and OBOP_BLOB_DIR are both set: choose one place for uploaded files"
                .into(),
        );
    }
    match store.strip_prefix(SCHEME) {
        Some(folder) => Ok(Arc::new(DriveBytes::new(Config::from_env(folder)?)?)),
        None => Err("OBOP_BLOB_STORE must look like gdrive://<FOLDER_ID>".into()),
    }
}

/// The configured Drive store, if `OBOP_BLOB_STORE` names one (for `obop storage check`).
pub fn configured_drive() -> Result<Option<DriveBytes>> {
    match std::env::var("OBOP_BLOB_STORE") {
        Ok(s) if !s.trim().is_empty() => match s.trim().strip_prefix(SCHEME) {
            Some(folder) => Ok(Some(DriveBytes::new(Config::from_env(folder)?)?)),
            None => Err("OBOP_BLOB_STORE must look like gdrive://<FOLDER_ID>".into()),
        },
        _ => Ok(None),
    }
}

type Response = ureq::http::Response<ureq::Body>;

/// The blob store over one Drive folder.
pub struct DriveBytes {
    cfg: Config,
    agent: ureq::Agent,
    token: Mutex<Option<(String, Instant)>>,
    /// key -> Drive file id, filled as keys are found or created and dropped when Drive disagrees.
    ids: Mutex<HashMap<String, String>>,
}

impl DriveBytes {
    pub fn new(cfg: Config) -> Result<Self> {
        if !valid_drive_id(&cfg.folder_id) {
            return Err("the Drive folder id is not valid".into());
        }
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_global(Some(Duration::from_secs(300)))
            .build()
            .into();
        Ok(Self {
            cfg,
            agent,
            token: Mutex::new(None),
            ids: Mutex::new(HashMap::new()),
        })
    }

    /// A usable access token: the cached one until a minute before it expires, else a new one.
    fn access_token(&self, force: bool) -> Result<String> {
        let mut slot = self.token.lock().map_err(|_| "token lock poisoned")?;
        if !force
            && let Some((t, until)) = slot.as_ref()
            && Instant::now() < *until
        {
            return Ok(t.clone());
        }
        let mut resp = self
            .agent
            .post(&self.cfg.token_url)
            .send_form([
                ("client_id", self.cfg.client_id.as_str()),
                ("client_secret", self.cfg.client_secret.as_str()),
                ("refresh_token", self.cfg.refresh_token.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .map_err(|e| format!("Google sign-in unreachable: {}", transport(&e)))?;
        let status = resp.status().as_u16();
        let body = resp.body_mut().read_to_string().unwrap_or_default();
        let v: J = serde_json::from_str(&body).unwrap_or(J::Null);
        if status != 200 {
            // Only the error code: the body of a token reply is never echoed.
            let code = v["error"].as_str().unwrap_or("unknown").to_string();
            return Err(format!(
                "Google refused to issue an access token (HTTP {status}, {}): check OBOP_GDRIVE_CLIENT_ID, OBOP_GDRIVE_CLIENT_SECRET and OBOP_GDRIVE_REFRESH_TOKEN",
                clean(&code, 60)
            ));
        }
        let token = v["access_token"]
            .as_str()
            .filter(|t| !t.is_empty())
            .ok_or("Google's token reply had no access token")?
            .to_string();
        let secs = v["expires_in"].as_u64().unwrap_or(300).clamp(120, 3600);
        *slot = Some((
            token.clone(),
            Instant::now() + Duration::from_secs(secs - 60),
        ));
        Ok(token)
    }

    /// Send one request, retrying what is worth retrying; returns the first reply that is not a
    /// transient failure (the caller decides what a 404 or a 4xx means).
    fn call(
        &self,
        send: &dyn Fn(&ureq::Agent, &str) -> std::result::Result<Response, ureq::Error>,
    ) -> Result<Response> {
        let mut refreshed = false;
        let mut pause = self.cfg.retry_pause;
        let mut last = String::new();
        for attempt in 1..=self.cfg.max_attempts.max(1) {
            let token = self.access_token(false)?;
            match send(&self.agent, &token) {
                Err(e) => last = format!("Google Drive unreachable: {}", transport(&e)),
                Ok(mut r) => {
                    let status = r.status().as_u16();
                    if status == 401 && !refreshed {
                        refreshed = true;
                        self.access_token(true)?;
                        continue;
                    }
                    let retry = status == 429
                        || status >= 500
                        || (status == 403 && {
                            let body = r.body_mut().read_to_string().unwrap_or_default();
                            let limited = body.contains("ateLimitExceeded")
                                || body.contains("rateLimitExceeded")
                                || body.contains("quotaExceeded");
                            if !limited {
                                return Err(drive_error(status, &body));
                            }
                            true
                        });
                    if !retry {
                        return Ok(r);
                    }
                    last = format!("Google Drive answered HTTP {status}");
                }
            }
            if attempt < self.cfg.max_attempts {
                std::thread::sleep(pause);
                pause = pause.saturating_mul(2);
            }
        }
        Err(format!(
            "{last}; gave up after {} attempts",
            self.cfg.max_attempts
        ))
    }

    /// Fail with Drive's own message unless the reply is a success.
    fn ok(mut r: Response) -> Result<Response> {
        let status = r.status().as_u16();
        if (200..300).contains(&status) {
            return Ok(r);
        }
        let body = r.body_mut().read_to_string().unwrap_or_default();
        Err(drive_error(status, &body))
    }

    fn json(mut r: Response) -> Result<J> {
        let body = r.body_mut().read_to_string().map_err(|e| transport(&e))?;
        serde_json::from_str(&body)
            .map_err(|_| "Google Drive sent an answer that is not JSON".into())
    }

    fn list(&self, query: &str, page: Option<&str>) -> Result<J> {
        let r = self.call(&|agent, token| {
            let mut req = agent
                .get(format!("{}/drive/v3/files", self.cfg.api_base))
                .header("Authorization", format!("Bearer {token}"))
                .query("q", query)
                .query("fields", "nextPageToken,files(id,name)")
                .query("pageSize", "1000")
                .query("spaces", "drive");
            if let Some(p) = page {
                req = req.query("pageToken", p);
            }
            req.call()
        })?;
        Self::json(Self::ok(r)?)
    }

    /// Every Drive file of the folder called `key` (Drive allows duplicate names).
    fn find_all(&self, key: &str) -> Result<Vec<String>> {
        let q = format!(
            "name = '{key}' and '{}' in parents and trashed = false",
            self.cfg.folder_id
        );
        let v = self.list(&q, None)?;
        Ok(v["files"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|f| f["id"].as_str())
                    .filter(|id| valid_drive_id(id))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default())
    }

    fn find(&self, key: &str) -> Result<Option<String>> {
        if let Some(id) = self.ids.lock().map_err(|_| "cache lock poisoned")?.get(key) {
            return Ok(Some(id.clone()));
        }
        let found = self.find_all(key)?.into_iter().next();
        if let Some(id) = &found {
            self.ids
                .lock()
                .map_err(|_| "cache lock poisoned")?
                .insert(key.into(), id.clone());
        }
        Ok(found)
    }

    fn forget(&self, key: &str) {
        if let Ok(mut m) = self.ids.lock() {
            m.remove(key);
        }
    }

    fn create(&self, key: &str, bytes: &[u8]) -> Result<()> {
        let mut boundary = [0u8; 16];
        getrandom::fill(&mut boundary).map_err(|e| e.to_string())?;
        let boundary: String = boundary.iter().map(|b| format!("{b:02x}")).collect();
        let meta = json!({ "name": key, "parents": [self.cfg.folder_id] }).to_string();
        let mut body = Vec::with_capacity(bytes.len() + meta.len() + 200);
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n--{boundary}\r\nContent-Type: application/octet-stream\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--").as_bytes());
        let r = self.call(&|agent, token| {
            agent
                .post(format!("{}/upload/drive/v3/files", self.cfg.api_base))
                .header("Authorization", format!("Bearer {token}"))
                .header(
                    "Content-Type",
                    format!("multipart/related; boundary={boundary}"),
                )
                .query("uploadType", "multipart")
                .query("fields", "id")
                .send(&body[..])
        })?;
        let v = Self::json(Self::ok(r)?)?;
        if let Some(id) = v["id"].as_str().filter(|id| valid_drive_id(id)) {
            self.ids
                .lock()
                .map_err(|_| "cache lock poisoned")?
                .insert(key.into(), id.into());
        }
        Ok(())
    }

    /// Create a folder (as this app, which is what the narrow `drive.file` scope can use) and
    /// return its id: for `obop storage drive-folder`.
    pub fn create_folder(&self, name: &str, parent: Option<&str>) -> Result<String> {
        if name.trim().is_empty() || name.len() > 200 || name.chars().any(char::is_control) {
            return Err("the folder name must be 1 to 200 characters on one line".into());
        }
        let mut meta = json!({ "name": name, "mimeType": FOLDER_MIME });
        if let Some(p) = parent {
            if !valid_drive_id(p) {
                return Err("the parent folder id is not valid".into());
            }
            meta["parents"] = json!([p]);
        }
        let body = meta.to_string();
        let r = self.call(&|agent, token| {
            agent
                .post(format!("{}/drive/v3/files", self.cfg.api_base))
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .query("fields", "id")
                .send(body.as_bytes())
        })?;
        let v = Self::json(Self::ok(r)?)?;
        v["id"]
            .as_str()
            .filter(|id| valid_drive_id(id))
            .map(str::to_string)
            .ok_or_else(|| "Google Drive did not return the new folder's id".into())
    }

    /// Whether the folder can be reached with these credentials: its name, for
    /// `obop storage check`.
    pub fn probe(&self) -> Result<String> {
        let r = self.call(&|agent, token| {
            agent
                .get(format!(
                    "{}/drive/v3/files/{}",
                    self.cfg.api_base, self.cfg.folder_id
                ))
                .header("Authorization", format!("Bearer {token}"))
                .query("fields", "id,name,mimeType,trashed")
                .call()
        })?;
        if r.status().as_u16() == 404 {
            return Err("Google Drive cannot find the folder with these credentials (with the drive.file scope only a folder created by this app is visible: make one with `obop storage drive-folder`)".into());
        }
        let v = Self::json(Self::ok(r)?)?;
        if v["mimeType"].as_str() != Some(FOLDER_MIME) {
            return Err("the Drive id in OBOP_BLOB_STORE is not a folder".into());
        }
        if v["trashed"].as_bool() == Some(true) {
            return Err("the Drive folder is in the trash".into());
        }
        Ok(v["name"].as_str().unwrap_or("").to_string())
    }
}

impl BlobBytes for DriveBytes {
    fn put(&self, key: &str, bytes: &[u8]) -> Result<()> {
        check_key(key)?;
        if let Some(id) = self.find(key)? {
            let r = self.call(&|agent, token| {
                agent
                    .patch(format!("{}/upload/drive/v3/files/{id}", self.cfg.api_base))
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/octet-stream")
                    .query("uploadType", "media")
                    .query("fields", "id")
                    .send(bytes)
            })?;
            if r.status().as_u16() != 404 {
                Self::ok(r)?;
                return Ok(());
            }
            // The file is gone (deleted in Drive): fall through and make it again.
            self.forget(key);
        }
        self.create(key, bytes)
    }

    fn open(&self, key: &str) -> Result<Option<Box<dyn Read + Send>>> {
        check_key(key)?;
        for _ in 0..2 {
            let Some(id) = self.find(key)? else {
                return Ok(None);
            };
            let r = self.call(&|agent, token| {
                agent
                    .get(format!("{}/drive/v3/files/{id}", self.cfg.api_base))
                    .header("Authorization", format!("Bearer {token}"))
                    .query("alt", "media")
                    .call()
            })?;
            if r.status().as_u16() == 404 {
                // The cached id is stale; ask Drive again once.
                self.forget(key);
                continue;
            }
            return Ok(Some(Box::new(Self::ok(r)?.into_body().into_reader())));
        }
        Ok(None)
    }

    fn delete(&self, key: &str) -> Result<bool> {
        check_key(key)?;
        self.forget(key);
        let ids = self.find_all(key)?;
        let mut any = false;
        for id in ids {
            let r = self.call(&|agent, token| {
                agent
                    .delete(format!("{}/drive/v3/files/{id}", self.cfg.api_base))
                    .header("Authorization", format!("Bearer {token}"))
                    .call()
            })?;
            if r.status().as_u16() != 404 {
                Self::ok(r)?;
                any = true;
            }
        }
        Ok(any)
    }

    fn keys(&self) -> Result<Vec<String>> {
        let q = format!("'{}' in parents and trashed = false", self.cfg.folder_id);
        let mut keys = BTreeSet::new();
        let mut page: Option<String> = None;
        loop {
            let v = self.list(&q, page.as_deref())?;
            if let Some(files) = v["files"].as_array() {
                for f in files {
                    if let (Some(name), Some(id)) = (f["name"].as_str(), f["id"].as_str())
                        && valid_key(name)
                        && valid_drive_id(id)
                    {
                        keys.insert(name.to_string());
                        self.ids
                            .lock()
                            .map_err(|_| "cache lock poisoned")?
                            .entry(name.to_string())
                            .or_insert_with(|| id.to_string());
                    }
                }
            }
            match v["nextPageToken"].as_str().filter(|t| !t.is_empty()) {
                Some(t) => page = Some(t.to_string()),
                None => return Ok(keys.into_iter().collect()),
            }
        }
    }
}

fn check_key(key: &str) -> Result<()> {
    if valid_key(key) {
        Ok(())
    } else {
        Err("invalid blob key".into())
    }
}

/// One line of printable text, at most `max` characters.
fn clean(s: &str, max: usize) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(max)
        .collect()
}

/// What went wrong on the wire, without the address (it may carry an id) or any header.
fn transport(e: &ureq::Error) -> String {
    match e {
        ureq::Error::Timeout(_) => "timed out".into(),
        ureq::Error::HostNotFound => "host not found".into(),
        ureq::Error::ConnectionFailed => "connection failed".into(),
        _ => "network error".into(),
    }
}

/// Drive's own explanation of a failure: the status and the `error.message` it sent.
fn drive_error(status: u16, body: &str) -> String {
    let message = serde_json::from_str::<J>(body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(|m| clean(m, 200)))
        .unwrap_or_default();
    if message.is_empty() {
        format!("Google Drive answered HTTP {status}")
    } else {
        format!("Google Drive answered HTTP {status}: {message}")
    }
}
