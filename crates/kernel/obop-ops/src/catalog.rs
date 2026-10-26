//! What the instance remembers about its own backups, drills and storage (OBR-DAT-047, 049, 056,
//! D73): small records in the database next to the data, so the metrics, the preflight of a
//! migration and `obop storage check` can answer "when was the last good backup" without a
//! file listing, and so the answer survives a restart and is the same for every process.
//!
//! - `obop_backup`: one record per backup the instance took (file name as id): when, size, format,
//!   key id, whether it was verified (and authenticated), how long it took, and its copies off
//!   the host. The backup itself does not contain its own record (it is written after); the next
//!   backup contains the previous one's.
//! - `obop_restore_drill`: one record per restore drill: when, backup, measured RPO and RTO, the
//!   checks and whether every one passed.
//! - `obop_storage_attestation`: one record per plane: where the data lives and how it is
//!   encrypted at rest, attested by a person (OBR-DAT-056).

use obop_storage::{Backend, Fields, Op};
use serde_json::{Value as J, json};

pub const BACKUP_MODEL: &str = "obop_backup";
pub const DRILL_MODEL: &str = "obop_restore_drill";
pub const ATTESTATION_MODEL: &str = "obop_storage_attestation";

/// A copy of a backup somewhere else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copy {
    /// The sink, for example `fs:/mnt/offsite`.
    pub sink: String,
    pub name: String,
    /// Unix seconds at which the copy was read back and its hash matched.
    pub verified_at: u64,
    /// SHA-256 of the data file as the sink holds it (hex), so that a later run can read the
    /// sink again and prove the copy is still there before it deletes the local file. Absent in
    /// records from before the review of D73.
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BackupRecord {
    /// The file name, unique.
    pub id: String,
    /// Where the file is on the machine that took the backup.
    pub path: String,
    pub created_at: u64,
    pub format: u64,
    pub key_id: Option<String>,
    pub bytes: u64,
    pub records: u64,
    pub schema_version: u64,
    pub duration_ms: u64,
    /// The backup was read back and (for format 2) authenticated with its key.
    pub verified: bool,
    pub authenticated: bool,
    /// `scheduled`, `manual`, `pre-migration`, `pre-update`.
    pub reason: String,
    pub copies: Vec<Copy>,
    /// The machine whose disk holds `path`; only that machine removes the file. Several
    /// processes, on several machines, share one catalogue; the files are where they were taken.
    pub host: Option<String>,
}

impl BackupRecord {
    pub fn to_fields(&self) -> Fields {
        let mut f = Fields::new();
        let mut set = |k: &str, v: J| f.insert(k.into(), v);
        set("path", J::from(self.path.as_str()));
        set("created_at", J::from(self.created_at));
        set("format", J::from(self.format));
        set("key_id", self.key_id.as_deref().map_or(J::Null, J::from));
        set("bytes", J::from(self.bytes));
        set("records", J::from(self.records));
        set("schema_version", J::from(self.schema_version));
        set("duration_ms", J::from(self.duration_ms));
        set("verified", J::from(self.verified));
        set("authenticated", J::from(self.authenticated));
        set("reason", J::from(self.reason.as_str()));
        set(
            "copies",
            J::Array(
                self.copies
                    .iter()
                    .map(|c| {
                        json!({
                            "sink": c.sink, "name": c.name, "verified_at": c.verified_at,
                            "sha256": c.sha256,
                        })
                    })
                    .collect(),
            ),
        );
        set("host", self.host.as_deref().map_or(J::Null, J::from));
        f
    }

    pub fn from_fields(id: &str, f: &Fields) -> Option<BackupRecord> {
        Some(BackupRecord {
            id: id.to_string(),
            path: f.get("path")?.as_str()?.to_string(),
            created_at: f.get("created_at")?.as_u64()?,
            format: f.get("format")?.as_u64()?,
            key_id: f.get("key_id").and_then(J::as_str).map(String::from),
            bytes: f.get("bytes")?.as_u64()?,
            records: f.get("records")?.as_u64()?,
            schema_version: f.get("schema_version")?.as_u64()?,
            duration_ms: f.get("duration_ms")?.as_u64()?,
            verified: f.get("verified")?.as_bool()?,
            authenticated: f.get("authenticated")?.as_bool()?,
            reason: f.get("reason")?.as_str()?.to_string(),
            copies: f
                .get("copies")?
                .as_array()?
                .iter()
                .filter_map(|c| {
                    Some(Copy {
                        sink: c["sink"].as_str()?.to_string(),
                        name: c["name"].as_str()?.to_string(),
                        verified_at: c["verified_at"].as_u64()?,
                        sha256: c["sha256"].as_str().map(String::from),
                    })
                })
                .collect(),
            host: f.get("host").and_then(J::as_str).map(String::from),
        })
    }
}

/// One check of a restore drill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrillCheck {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DrillRecord {
    pub id: String,
    pub at: u64,
    /// The store the drill covers: `main` (the instance database).
    pub plane: String,
    pub backup_id: String,
    /// Measured recovery point: seconds between the backup and the drill, which is how much
    /// would be lost if the primary were lost at that moment and this backup were the newest.
    pub rpo_seconds: u64,
    /// Measured recovery time: seconds from the start of the restore to the last check.
    pub rto_seconds: u64,
    pub records: u64,
    pub checks: Vec<DrillCheck>,
    pub passed: bool,
}

impl DrillRecord {
    pub fn to_fields(&self) -> Fields {
        let mut f = Fields::new();
        f.insert("at".into(), J::from(self.at));
        f.insert("plane".into(), J::from(self.plane.as_str()));
        f.insert("backup_id".into(), J::from(self.backup_id.as_str()));
        f.insert("rpo_seconds".into(), J::from(self.rpo_seconds));
        f.insert("rto_seconds".into(), J::from(self.rto_seconds));
        f.insert("records".into(), J::from(self.records));
        f.insert("passed".into(), J::from(self.passed));
        f.insert(
            "checks".into(),
            J::Array(
                self.checks
                    .iter()
                    .map(|c| json!({"name": c.name, "ok": c.ok, "detail": c.detail}))
                    .collect(),
            ),
        );
        f
    }

    pub fn from_fields(id: &str, f: &Fields) -> Option<DrillRecord> {
        Some(DrillRecord {
            id: id.to_string(),
            at: f.get("at")?.as_u64()?,
            plane: f.get("plane")?.as_str()?.to_string(),
            backup_id: f.get("backup_id")?.as_str()?.to_string(),
            rpo_seconds: f.get("rpo_seconds")?.as_u64()?,
            rto_seconds: f.get("rto_seconds")?.as_u64()?,
            records: f.get("records")?.as_u64()?,
            passed: f.get("passed")?.as_bool()?,
            checks: f
                .get("checks")?
                .as_array()?
                .iter()
                .filter_map(|c| {
                    Some(DrillCheck {
                        name: c["name"].as_str()?.to_string(),
                        ok: c["ok"].as_bool()?,
                        detail: c["detail"].as_str()?.to_string(),
                    })
                })
                .collect(),
        })
    }
}

/// An attestation that a plane's storage is encrypted at rest (OBR-DAT-056 item 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attestation {
    pub plane: String,
    pub host: String,
    pub provider: String,
    /// For example `LUKS2 AES-256-XTS`, `provider-managed volume encryption AES-256`.
    pub mechanism: String,
    /// Where the keys are held: provider KMS, a separate custodian, never the same volume.
    pub key_custody: String,
    /// Who can read the keys.
    pub key_readers: String,
    /// Unix seconds of the attestation.
    pub at: u64,
    /// Where the evidence is kept (a ticket, a document, a provider test result).
    pub evidence: String,
    pub attested_by: String,
}

impl Attestation {
    pub fn to_fields(&self) -> Fields {
        let mut f = Fields::new();
        for (k, v) in [
            ("plane", &self.plane),
            ("host", &self.host),
            ("provider", &self.provider),
            ("mechanism", &self.mechanism),
            ("key_custody", &self.key_custody),
            ("key_readers", &self.key_readers),
            ("evidence", &self.evidence),
            ("attested_by", &self.attested_by),
        ] {
            f.insert(k.into(), J::from(v.as_str()));
        }
        f.insert("at".into(), J::from(self.at));
        f
    }

    pub fn from_fields(f: &Fields) -> Option<Attestation> {
        let s = |k: &str| f.get(k).and_then(J::as_str).map(String::from);
        Some(Attestation {
            plane: s("plane")?,
            host: s("host")?,
            provider: s("provider")?,
            mechanism: s("mechanism")?,
            key_custody: s("key_custody")?,
            key_readers: s("key_readers")?,
            evidence: s("evidence")?,
            attested_by: s("attested_by")?,
            at: f.get("at")?.as_u64()?,
        })
    }
}

/// Read and write the three catalogues.
pub struct Catalog<'a> {
    pub backend: &'a dyn Backend,
}

impl<'a> Catalog<'a> {
    pub fn new(backend: &'a dyn Backend) -> Self {
        Catalog { backend }
    }

    /// Record a backup (replacing the record of the same file name).
    pub fn record_backup(&self, b: &BackupRecord) -> Result<(), String> {
        self.backend.apply(&[Op::Put {
            model: BACKUP_MODEL.into(),
            id: b.id.clone(),
            fields: b.to_fields(),
        }])
    }

    /// Every recorded backup, oldest first.
    pub fn backups(&self) -> Result<Vec<BackupRecord>, String> {
        let mut v: Vec<BackupRecord> = self
            .backend
            .list(BACKUP_MODEL)?
            .iter()
            .filter_map(|(id, f)| BackupRecord::from_fields(id, f))
            .collect();
        v.sort_by_key(|b| (b.created_at, b.id.clone()));
        Ok(v)
    }

    pub fn forget_backup(&self, id: &str) -> Result<(), String> {
        self.backend.delete(BACKUP_MODEL, id).map(|_| ())
    }

    /// The newest backup that was verified, if any.
    pub fn latest_verified(&self) -> Result<Option<BackupRecord>, String> {
        Ok(self.backups()?.into_iter().rev().find(|b| b.verified))
    }

    pub fn record_drill(&self, d: &DrillRecord) -> Result<(), String> {
        self.backend.apply(&[Op::Put {
            model: DRILL_MODEL.into(),
            id: d.id.clone(),
            fields: d.to_fields(),
        }])
    }

    pub fn drills(&self) -> Result<Vec<DrillRecord>, String> {
        let mut v: Vec<DrillRecord> = self
            .backend
            .list(DRILL_MODEL)?
            .iter()
            .filter_map(|(id, f)| DrillRecord::from_fields(id, f))
            .collect();
        v.sort_by_key(|d| (d.at, d.id.clone()));
        Ok(v)
    }

    pub fn latest_passed_drill(&self) -> Result<Option<DrillRecord>, String> {
        Ok(self.drills()?.into_iter().rev().find(|d| d.passed))
    }

    /// Record an attestation for its plane (the newest one per plane is the one that counts; the
    /// older ones stay as history).
    pub fn attest(&self, a: &Attestation) -> Result<(), String> {
        self.backend.apply(&[Op::Put {
            model: ATTESTATION_MODEL.into(),
            id: format!("{}-{:012}", a.plane, a.at),
            fields: a.to_fields(),
        }])
    }

    pub fn attestations(&self) -> Result<Vec<Attestation>, String> {
        let mut v: Vec<Attestation> = self
            .backend
            .list(ATTESTATION_MODEL)?
            .iter()
            .filter_map(|(_, f)| Attestation::from_fields(f))
            .collect();
        v.sort_by_key(|a| (a.plane.clone(), a.at));
        Ok(v)
    }

    /// The newest attestation of `plane`.
    pub fn attestation(&self, plane: &str) -> Result<Option<Attestation>, String> {
        Ok(self.attestations()?.into_iter().rfind(|a| a.plane == plane))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backup(id: &str, at: u64, verified: bool) -> BackupRecord {
        BackupRecord {
            id: id.into(),
            path: format!("/b/{id}"),
            created_at: at,
            format: 2,
            key_id: Some("abcd1234".into()),
            bytes: 10,
            records: 3,
            schema_version: 2,
            duration_ms: 5,
            verified,
            authenticated: verified,
            reason: "scheduled".into(),
            copies: vec![Copy {
                sink: "fs:/x".into(),
                name: id.into(),
                verified_at: at + 1,
                sha256: Some("ab".repeat(32)),
            }],
            host: Some("db1".into()),
        }
    }

    #[test]
    fn backups_drills_and_attestations_round_trip_and_sort() {
        let b = obop_storage::open("memory:").unwrap();
        let c = Catalog::new(&*b);
        assert!(c.latest_verified().unwrap().is_none());
        c.record_backup(&backup("b", 200, true)).unwrap();
        c.record_backup(&backup("a", 100, true)).unwrap();
        c.record_backup(&backup("c", 300, false)).unwrap();
        let all = c.backups().unwrap();
        assert_eq!(
            all.iter().map(|b| b.id.as_str()).collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        assert_eq!(all[0], backup("a", 100, true));
        assert_eq!(c.latest_verified().unwrap().unwrap().id, "b");
        c.forget_backup("b").unwrap();
        assert_eq!(c.latest_verified().unwrap().unwrap().id, "a");

        let d = DrillRecord {
            id: "drill-1".into(),
            at: 50,
            plane: "main".into(),
            backup_id: "a".into(),
            rpo_seconds: 7,
            rto_seconds: 3,
            records: 9,
            checks: vec![DrillCheck {
                name: "counts".into(),
                ok: true,
                detail: "9 = 9".into(),
            }],
            passed: true,
        };
        c.record_drill(&d).unwrap();
        c.record_drill(&DrillRecord {
            id: "drill-2".into(),
            at: 60,
            passed: false,
            ..d.clone()
        })
        .unwrap();
        assert_eq!(c.drills().unwrap().len(), 2);
        assert_eq!(c.latest_passed_drill().unwrap().unwrap(), d);

        let a = Attestation {
            plane: "main".into(),
            host: "db1".into(),
            provider: "acme cloud".into(),
            mechanism: "provider volume encryption AES-256".into(),
            key_custody: "provider KMS".into(),
            key_readers: "the provider's key service only".into(),
            at: 5,
            evidence: "ticket 42".into(),
            attested_by: "root".into(),
        };
        c.attest(&a).unwrap();
        c.attest(&Attestation {
            at: 9,
            host: "db2".into(),
            ..a.clone()
        })
        .unwrap();
        assert_eq!(c.attestations().unwrap().len(), 2);
        assert_eq!(c.attestation("main").unwrap().unwrap().host, "db2");
        assert!(c.attestation("other").unwrap().is_none());
    }
}
