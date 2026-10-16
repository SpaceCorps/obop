//! Where packages come from besides an admin's own upload: a bundled catalog that ships in the
//! binary, and any HTTPS or HTTP host serving `index.json` plus `packages/<id>.toml`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listing {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub author: String,
}

pub trait Marketplace: Send + Sync {
    fn list(&self) -> Result<Vec<Listing>, String>;
    /// The `obop.toml` text of one package. The caller validates it exactly as an upload.
    fn fetch(&self, id: &str) -> Result<String, String>;
}

/// Packages compiled into the binary from `packages/*.toml`.
pub struct Bundled;

/// A bundled package: its id, the English `obop.toml` and one labels document per language
/// (`packages/labels/<lang>/<id>.toml`, D52).
type Bundle = (&'static str, &'static str, [&'static str; 6]);

macro_rules! bundle {
    ($id:literal) => {
        (
            $id,
            include_str!(concat!("../../../../packages/", $id, ".toml")),
            [
                include_str!(concat!("../../../../packages/labels/de/", $id, ".toml")),
                include_str!(concat!("../../../../packages/labels/fr/", $id, ".toml")),
                include_str!(concat!("../../../../packages/labels/pl/", $id, ".toml")),
                include_str!(concat!("../../../../packages/labels/fi/", $id, ".toml")),
                include_str!(concat!("../../../../packages/labels/sv/", $id, ".toml")),
                include_str!(concat!("../../../../packages/labels/es/", $id, ".toml")),
            ],
        )
    };
}

const BUNDLED: &[Bundle] = &[
    bundle!("accounting"),
    bundle!("appointments"),
    bundle!("dashboards"),
    bundle!("documents"),
    bundle!("ecommerce"),
    bundle!("email_marketing"),
    bundle!("expenses"),
    bundle!("field_service"),
    bundle!("fleet"),
    bundle!("helpdesk"),
    bundle!("hr"),
    bundle!("inventory"),
    bundle!("knowledge"),
    bundle!("manufacturing"),
    bundle!("planning"),
    bundle!("pos"),
    bundle!("projects"),
    bundle!("purchase"),
    bundle!("sign"),
    bundle!("subscriptions"),
    bundle!("timesheets"),
    bundle!("website"),
];

/// The English manifest with its labels documents appended: one TOML document, which is what
/// is installed, stored and served (an upload of the same text installs identically).
fn combined((_, manifest, labels): &Bundle) -> String {
    let mut text = manifest.trim_end().to_string();
    for l in labels.iter().filter(|l| !l.trim().is_empty()) {
        text.push_str("\n\n");
        text.push_str(l.trim_end());
    }
    text.push('\n');
    text
}

impl Bundled {
    /// The labels documents of a bundled package, concatenated (empty when it has none).
    pub fn labels_text(id: &str) -> String {
        BUNDLED
            .iter()
            .find(|b| b.0 == id)
            .map(|(_, _, labels)| {
                labels
                    .iter()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| l.trim_end())
                    .collect::<Vec<_>>()
                    .join("\n\n")
            })
            .unwrap_or_default()
    }
}

impl Marketplace for Bundled {
    fn list(&self) -> Result<Vec<Listing>, String> {
        BUNDLED
            .iter()
            .map(|b| {
                let m = crate::manifest::Manifest::parse(&combined(b))?;
                Ok(Listing {
                    id: m.module.id,
                    name: m.module.name,
                    version: m.module.version,
                    description: m.module.description,
                    author: m.module.author,
                })
            })
            .collect()
    }

    fn fetch(&self, id: &str) -> Result<String, String> {
        BUNDLED
            .iter()
            .find(|b| b.0 == id)
            .map(combined)
            .ok_or_else(|| format!("no package {id} in the bundled catalog"))
    }
}

/// A remote catalog. Only the configured base URL is ever contacted, and package ids are
/// restricted to identifier characters so a listing cannot steer a request elsewhere.
pub struct Http {
    base: String,
}

impl Http {
    pub fn new(base: &str) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
        }
    }

    fn get(&self, path: &str) -> Result<String, String> {
        let url = format!("{}/{path}", self.base);
        let mut resp = ureq::get(&url)
            .call()
            .map_err(|e| format!("marketplace request failed: {e}"))?;
        resp.body_mut()
            .with_config()
            .limit(crate::manifest::MAX_BYTES as u64 * 4)
            .read_to_string()
            .map_err(|e| e.to_string())
    }
}

impl Marketplace for Http {
    fn list(&self) -> Result<Vec<Listing>, String> {
        serde_json::from_str(&self.get("index.json")?)
            .map_err(|e| format!("bad marketplace index: {e}"))
    }

    fn fetch(&self, id: &str) -> Result<String, String> {
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err("invalid package id".into());
        }
        self.get(&format!("packages/{id}.toml"))
    }
}

/// Several catalogs, first match wins.
pub struct Chain(pub Vec<Box<dyn Marketplace>>);

impl Marketplace for Chain {
    fn list(&self) -> Result<Vec<Listing>, String> {
        let mut all: Vec<Listing> = Vec::new();
        for m in &self.0 {
            if let Ok(l) = m.list() {
                for x in l {
                    if !all.iter().any(|y| y.id == x.id) {
                        all.push(x);
                    }
                }
            }
        }
        Ok(all)
    }

    fn fetch(&self, id: &str) -> Result<String, String> {
        let mut last = format!("no package {id}");
        for m in &self.0 {
            match m.fetch(id) {
                Ok(t) => return Ok(t),
                Err(e) => last = e,
            }
        }
        Err(last)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_catalog_is_valid() {
        let l = Bundled.list().unwrap();
        let ids: Vec<_> = l.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "accounting",
                "appointments",
                "dashboards",
                "documents",
                "ecommerce",
                "email_marketing",
                "expenses",
                "field_service",
                "fleet",
                "helpdesk",
                "hr",
                "inventory",
                "knowledge",
                "manufacturing",
                "planning",
                "pos",
                "projects",
                "purchase",
                "sign",
                "subscriptions",
                "timesheets",
                "website"
            ]
        );
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(
            ids, sorted,
            "the catalogue is listed in a stable, sorted order"
        );
        for x in &l {
            assert!(
                crate::manifest::Manifest::parse(&Bundled.fetch(&x.id).unwrap())
                    .unwrap()
                    .to_module()
                    .is_ok(),
                "{}",
                x.id
            );
        }
        assert!(Bundled.fetch("nope").is_err());
    }

    #[test]
    fn http_rejects_path_tricks() {
        assert!(
            Http::new("http://127.0.0.1:1")
                .fetch("../etc/passwd")
                .is_err()
        );
        assert!(Http::new("http://127.0.0.1:1").fetch("").is_err());
    }
}
