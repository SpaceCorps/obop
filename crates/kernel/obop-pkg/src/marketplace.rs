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

const BUNDLED: &[(&str, &str)] = &[
    (
        "accounting",
        include_str!("../../../../packages/accounting.toml"),
    ),
    (
        "appointments",
        include_str!("../../../../packages/appointments.toml"),
    ),
    (
        "dashboards",
        include_str!("../../../../packages/dashboards.toml"),
    ),
    (
        "documents",
        include_str!("../../../../packages/documents.toml"),
    ),
    (
        "ecommerce",
        include_str!("../../../../packages/ecommerce.toml"),
    ),
    (
        "email_marketing",
        include_str!("../../../../packages/email_marketing.toml"),
    ),
    (
        "expenses",
        include_str!("../../../../packages/expenses.toml"),
    ),
    (
        "field_service",
        include_str!("../../../../packages/field_service.toml"),
    ),
    ("fleet", include_str!("../../../../packages/fleet.toml")),
    (
        "helpdesk",
        include_str!("../../../../packages/helpdesk.toml"),
    ),
    ("hr", include_str!("../../../../packages/hr.toml")),
    (
        "inventory",
        include_str!("../../../../packages/inventory.toml"),
    ),
    (
        "knowledge",
        include_str!("../../../../packages/knowledge.toml"),
    ),
    (
        "manufacturing",
        include_str!("../../../../packages/manufacturing.toml"),
    ),
    (
        "planning",
        include_str!("../../../../packages/planning.toml"),
    ),
    ("pos", include_str!("../../../../packages/pos.toml")),
    (
        "projects",
        include_str!("../../../../packages/projects.toml"),
    ),
    (
        "purchase",
        include_str!("../../../../packages/purchase.toml"),
    ),
    ("sign", include_str!("../../../../packages/sign.toml")),
    (
        "subscriptions",
        include_str!("../../../../packages/subscriptions.toml"),
    ),
    (
        "timesheets",
        include_str!("../../../../packages/timesheets.toml"),
    ),
    ("website", include_str!("../../../../packages/website.toml")),
];

impl Marketplace for Bundled {
    fn list(&self) -> Result<Vec<Listing>, String> {
        BUNDLED
            .iter()
            .map(|(_, text)| {
                let m = crate::manifest::Manifest::parse(text)?;
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
            .find(|(i, _)| *i == id)
            .map(|(_, t)| t.to_string())
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
