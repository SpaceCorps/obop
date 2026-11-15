//! The Drive the back office may browse (`/api/drive/*`): `obop-blob-gdrive`'s read-only browser
//! behind the `obop_http::drive::DriveSource` port.
//!
//! Where the credentials come from, in this order:
//!
//! 1. the environment, when `OBOP_GDRIVE_BROWSE_REFRESH_TOKEN` is set (an operator who prefers
//!    to configure a Drive outside the database);
//! 2. the Google Drive page of the back office: the OAuth client under the provider policy and
//!    the tokens of the instance's Drive connection, read at request time, so connecting,
//!    disconnecting or changing the client takes effect without a restart.

use obop_blob_gdrive::{Config, DriveBrowser, Workspace};
use obop_connectors::Runtime;
use obop_http::drive::{DriveCreated, DriveEntry, DriveFile, DrivePage, DriveSource};
use serde_json::{Value as J, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct Source {
    env: Option<DriveBrowser>,
    rt: Arc<Runtime>,
    /// The browser built for the credentials last read, so its access token is reused.
    cached: Mutex<Option<(Key, Arc<DriveBrowser>)>>,
}

#[derive(PartialEq, Eq)]
struct Key(String, String, String, String);

impl Source {
    pub fn new(rt: Arc<Runtime>, env: Option<DriveBrowser>) -> Self {
        Self {
            env,
            rt,
            cached: Mutex::new(None),
        }
    }

    /// The browser to use now, or why there is none.
    fn browser(&self) -> Result<Arc<DriveBrowser>, String> {
        let c = self
            .rt
            .drive_credentials()?
            .ok_or("no Google Drive is connected")?;
        let key = Key(
            c.client_id.clone(),
            c.client_secret.clone(),
            c.refresh_token.clone(),
            c.api_base.clone(),
        );
        let mut slot = self.cached.lock().map_err(|_| "lock poisoned")?;
        if let Some((k, b)) = slot.as_ref()
            && *k == key
        {
            return Ok(b.clone());
        }
        let b = Arc::new(DriveBrowser::new(Config {
            // Unused here: a browser is not tied to a folder.
            folder_id: "root".into(),
            client_id: c.client_id,
            client_secret: c.client_secret,
            refresh_token: c.refresh_token,
            api_base: c.api_base,
            token_url: c.token_url,
            retry_pause: Duration::from_millis(500),
            max_attempts: 4,
        })?);
        *slot = Some((key, b.clone()));
        Ok(b)
    }

    fn with<T>(&self, f: impl FnOnce(&DriveBrowser) -> Result<T, String>) -> Result<T, String> {
        match &self.env {
            Some(b) => f(b),
            None => f(&*self.browser()?),
        }
    }

    /// The workspace an administrator chose on the Google Drive page. A Drive set in the
    /// environment has none (its token is read-only and instance wide, as before).
    fn ws(&self) -> Option<Workspace> {
        if self.env.is_some() {
            return None;
        }
        self.rt
            .drive_workspace()
            .and_then(|(id, _)| Workspace::parse(&id))
    }

    fn made(c: obop_blob_gdrive::Created) -> DriveCreated {
        DriveCreated {
            id: c.id,
            name: c.name,
            mime: c.mime,
            folder: c.folder,
            web_link: c.web_link,
        }
    }

    fn need_ws(&self) -> Result<Workspace, String> {
        self.ws()
            .ok_or_else(|| "no workspace drive is chosen".to_string())
    }
}

impl DriveSource for Source {
    fn list(&self, parent: &str, search: &str, page: Option<&str>) -> Result<DrivePage, String> {
        let ws = self.ws();
        let p = self.with(|b| b.list_in(ws.as_ref(), parent, search, page))?;
        Ok(DrivePage {
            entries: p
                .entries
                .into_iter()
                .map(|e| DriveEntry {
                    id: e.id,
                    name: e.name,
                    mime: e.mime,
                    folder: e.folder,
                    modified: e.modified,
                    size: e.size,
                    web_link: e.web_link,
                    importable: e.importable,
                })
                .collect(),
            next: p.next,
        })
    }

    fn fetch(&self, id: &str, max: usize) -> Result<DriveFile, String> {
        let ws = self.ws();
        let f = self.with(|b| b.fetch_in(ws.as_ref(), id, max))?;
        Ok(DriveFile {
            id: f.id,
            name: f.name,
            mime: f.mime,
            modified: f.modified,
            web_link: f.web_link,
            bytes: f.bytes,
        })
    }

    fn connected(&self) -> bool {
        self.env.is_some() || matches!(self.rt.drive_credentials(), Ok(Some(_)))
    }

    fn browse_group(&self) -> Option<String> {
        Some(self.rt.drive_browse_group())
    }

    fn workspace(&self) -> Option<(String, String)> {
        if self.env.is_some() {
            return None;
        }
        self.rt.drive_workspace()
    }

    fn write_group(&self) -> Option<String> {
        Some(self.rt.drive_write_group())
    }

    fn can_write(&self) -> bool {
        self.env.is_none() && matches!(self.rt.drive_credentials(), Ok(Some(c)) if c.can_write())
    }

    fn drives(&self) -> Result<Vec<(String, String)>, String> {
        Ok(self
            .with(|b| b.drives())?
            .into_iter()
            .map(|d| (d.id, d.name))
            .collect())
    }

    fn create_folder(&self, parent: &str, name: &str) -> Result<DriveCreated, String> {
        let ws = self.need_ws()?;
        self.with(|b| b.create_folder_in(&ws, parent, name))
            .map(Self::made)
    }

    fn create_doc(&self, parent: &str, name: &str) -> Result<DriveCreated, String> {
        let ws = self.need_ws()?;
        self.with(|b| b.create_doc_in(&ws, parent, name))
            .map(Self::made)
    }

    fn upload(
        &self,
        parent: &str,
        name: &str,
        mime: &str,
        bytes: &[u8],
    ) -> Result<DriveCreated, String> {
        let ws = self.need_ws()?;
        self.with(|b| b.upload_in(&ws, parent, name, mime, bytes))
            .map(Self::made)
    }

    fn settings(&self) -> Option<J> {
        let mut j = self.rt.drive_status().ok()?.to_json();
        j["source"] = json!(if self.env.is_some() { "env" } else { "admin" });
        Some(j)
    }
}
