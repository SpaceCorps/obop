//! The Drive the back office may browse (`/api/drive/*`): `obop-blob-gdrive`'s read-only browser
//! behind the `obop_http::drive::DriveSource` port.

use obop_blob_gdrive::DriveBrowser;
use obop_http::drive::{DriveEntry, DriveFile, DrivePage, DriveSource};

pub struct Source(pub DriveBrowser);

impl DriveSource for Source {
    fn list(&self, parent: &str, search: &str, page: Option<&str>) -> Result<DrivePage, String> {
        let p = self.0.list(parent, search, page)?;
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
        let f = self.0.fetch(id, max)?;
        Ok(DriveFile {
            id: f.id,
            name: f.name,
            mime: f.mime,
            modified: f.modified,
            web_link: f.web_link,
            bytes: f.bytes,
        })
    }
}
