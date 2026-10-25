//! The migration ledger (OBR-DAT-051, D73): one record per migration step that ever ran against a
//! database, in the model `obop_migration`, written in the same transaction as the step.
//!
//! ```text
//! core/0002                 module "core",  kind "rust":    step 2 of obop-migrate
//! core/0002/failed-<ms>     one row per failed attempt (the step's own row stays as it was)
//! pkg/<id>/<from>-<to>/<ms>-<n>   module <id>, kind "package": a [[migration]] step of module.upgrade
//! core/rollback-<ms>        a rollback by restore, with the reason
//! ```
//!
//! The ledger is the record of what ran; the integer in `obop_meta/schema` stays as the derived
//! "how far" value that servers compare. Each row carries the SHA-256 of the step's definition:
//! for a Rust step the declared identity plus the text of its source file, for a package step the
//! declared operation. A definition that no longer matches the row of a step that already ran
//! (drift) stops the server from starting and names the step, because an edited migration means
//! the database in front of the operator was not produced by the code in front of them.
//!
//! Rows are append-only by convention here (a step's row moves `partial` to `applied` once, in the
//! transaction that completes it; attempts that failed are separate rows). Making that a database
//! guarantee is D77.

use obop_storage::{Backend, Fields, Op};
use serde_json::Value as J;
use sha2::{Digest, Sha256};

pub use crate::ddl::Class as Ddl;

pub const MODEL: &str = "obop_migration";
/// Stored plans (see `plan.rs`).
pub const PLAN_MODEL: &str = "obop_migration_plan";

/// What a step does to data, for `expand`/`backfill`/`contract` release planning (ENGINEERING 4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// Adds something the previous release ignores.
    Expand,
    /// Fills or converts existing data.
    Backfill,
    /// Removes something the previous release may still use; only in a release that is not the
    /// one of its expand.
    Contract,
}

impl Class {
    pub fn as_str(self) -> &'static str {
        match self {
            Class::Expand => "expand",
            Class::Backfill => "backfill",
            Class::Contract => "contract",
        }
    }

    pub fn parse(s: &str) -> Option<Class> {
        Some(match s {
            "expand" => Class::Expand,
            "backfill" => Class::Backfill,
            "contract" => Class::Contract,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The step ran to its end.
    Applied,
    /// An attempt that did not (a separate row; the step's own row is unchanged).
    Failed,
    /// A step that works in batches began and has not finished; the next run resumes it.
    Partial,
    /// A step from before the ledger, recorded without being replayed.
    Adopted,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Applied => "applied",
            Status::Failed => "failed",
            Status::Partial => "partial",
            Status::Adopted => "adopted",
        }
    }

    pub fn parse(s: &str) -> Option<Status> {
        Some(match s {
            "applied" => Status::Applied,
            "failed" => Status::Failed,
            "partial" => Status::Partial,
            "adopted" => Status::Adopted,
            _ => return None,
        })
    }
}

/// One row of the ledger.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: String,
    /// `core`, or the id of the package.
    pub module: String,
    /// `rust` (a step of obop-migrate), `package` (a `[[migration]]` of a package) or `sql`
    /// (reserved for SQL packages, OBR-DAT-029).
    pub kind: String,
    /// The step within its module: `0002`, or `0.1.0->0.2.0#1`.
    pub step: String,
    /// The schema version after a core step.
    pub version: Option<u32>,
    pub name: String,
    pub class: Class,
    pub ddl: Ddl,
    /// SHA-256 of the step definition, hex.
    pub checksum: String,
    pub status: Status,
    /// Unix seconds.
    pub at: u64,
    pub duration_ms: u64,
    /// Who ran it: `operator`, `server` (auto-migrate), or the administrator of `module.upgrade`.
    pub by: String,
    /// The database role it ran under. One application role exists today (`application`); the
    /// role separation of D77 makes DDL steps run as `owner`.
    pub role: String,
    pub app_version: String,
    /// The verified backup taken before the run (the file name), if there was one.
    pub backup_id: Option<String>,
    /// The PostgreSQL restore point created before the run, if there was one.
    pub restore_point: Option<String>,
    /// What the step reported, or why it failed. Never a secret: steps report counts and places.
    pub note: String,
}

impl Entry {
    pub fn to_fields(&self) -> Fields {
        let mut f = Fields::new();
        let mut s = |k: &str, v: &str| f.insert(k.into(), J::from(v));
        s("module", &self.module);
        s("kind", &self.kind);
        s("step", &self.step);
        s("name", &self.name);
        s("class", self.class.as_str());
        s("ddl", self.ddl.as_str());
        s("checksum", &self.checksum);
        s("status", self.status.as_str());
        s("by", &self.by);
        s("role", &self.role);
        s("app_version", &self.app_version);
        s("note", &self.note);
        f.insert("version".into(), self.version.map_or(J::Null, J::from));
        f.insert("at".into(), J::from(self.at));
        f.insert("duration_ms".into(), J::from(self.duration_ms));
        f.insert(
            "backup_id".into(),
            self.backup_id.as_deref().map_or(J::Null, J::from),
        );
        f.insert(
            "restore_point".into(),
            self.restore_point.as_deref().map_or(J::Null, J::from),
        );
        f
    }

    pub fn from_fields(id: &str, f: &Fields) -> Option<Entry> {
        let s = |k: &str| f.get(k).and_then(J::as_str).map(String::from);
        Some(Entry {
            id: id.to_string(),
            module: s("module")?,
            kind: s("kind")?,
            step: s("step")?,
            version: f.get("version").and_then(J::as_u64).map(|v| v as u32),
            name: s("name").unwrap_or_default(),
            class: Class::parse(&s("class")?)?,
            ddl: Ddl::parse(&s("ddl")?)?,
            checksum: s("checksum")?,
            status: Status::parse(&s("status")?)?,
            at: f.get("at").and_then(J::as_u64).unwrap_or(0),
            duration_ms: f.get("duration_ms").and_then(J::as_u64).unwrap_or(0),
            by: s("by").unwrap_or_default(),
            role: s("role").unwrap_or_default(),
            app_version: s("app_version").unwrap_or_default(),
            backup_id: s("backup_id"),
            restore_point: s("restore_point"),
            note: s("note").unwrap_or_default(),
        })
    }

    /// The storage operation that writes this row, replacing a row of the same id.
    pub fn put(&self) -> Op {
        Op::Put {
            model: MODEL.into(),
            id: self.id.clone(),
            fields: self.to_fields(),
        }
    }

    /// The storage operation that adds this row; the batch fails if the id exists.
    pub fn insert(&self) -> Op {
        Op::Insert {
            model: MODEL.into(),
            id: self.id.clone(),
            fields: self.to_fields(),
        }
    }
}

/// The id of a core step's row.
pub fn core_id(version: u32) -> String {
    format!("core/{version:04}")
}

/// The id of a failed attempt of a core step.
pub fn core_failed_id(version: u32, at_ms: u64) -> String {
    format!("core/{version:04}/failed-{at_ms}")
}

/// Every row of the ledger, in the order they were written.
pub fn read_all(backend: &dyn Backend) -> Result<Vec<Entry>, String> {
    let mut v = Vec::new();
    for (id, f) in backend.list(MODEL)? {
        if let Some(e) = Entry::from_fields(&id, &f) {
            v.push(e);
        }
    }
    Ok(v)
}

/// The text of a source file reduced to what changes behaviour for a human reviewer's purposes:
/// line endings, indentation and trailing spaces, blank lines and comment-only lines do not
/// count, so reformatting and a new comment are not a different migration but a changed
/// statement is.
pub fn normalise_source(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with("//") {
            continue;
        }
        out.push_str(l);
        out.push('\n');
    }
    out
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    obop_keys::hex(&Sha256::digest(bytes))
}

/// SHA-256 of the normalised text of a source file, hex.
pub fn source_digest(text: &str) -> String {
    sha256_hex(normalise_source(text).as_bytes())
}

/// The checksum of a package data step: its operation and what it names, nothing that is only a
/// description. Two upgrades that declare the same step have the same checksum.
pub fn package_checksum(
    op: &str,
    model: &str,
    field: &str,
    to: Option<&str>,
    value: Option<&J>,
) -> String {
    let canonical = serde_json::json!({
        "v": 1, "op": op, "model": model, "field": field, "to": to, "value": value,
    });
    sha256_hex(canonical.to_string().as_bytes())
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Rows for the data steps of one package upgrade (or rollback), to be put into the same batch
/// as the package record and the data writes. `steps` are `(op, model, field, to, value,
/// summary)`.
pub struct PackageStep<'a> {
    pub op: &'a str,
    pub model: &'a str,
    pub field: &'a str,
    pub to: Option<&'a str>,
    pub value: Option<&'a J>,
    pub summary: &'a str,
}

#[allow(clippy::too_many_arguments)]
pub fn package_entries(
    module: &str,
    from: &str,
    to: &str,
    rollback: bool,
    steps: &[PackageStep],
    by: &str,
    backup_id: Option<&str>,
) -> Vec<Entry> {
    let ms = now_ms();
    let at = now_secs();
    steps
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let n = i + 1;
            Entry {
                id: format!(
                    "pkg/{module}/{from}-{to}/{}{ms}-{n}",
                    if rollback { "rollback-" } else { "" }
                ),
                module: module.to_string(),
                kind: "package".into(),
                step: format!("{from}->{to}#{n}"),
                version: None,
                name: s.summary.to_string(),
                class: match s.op {
                    "drop_field" => Class::Contract,
                    "rename_field" | "set_default" => Class::Backfill,
                    _ => Class::Backfill,
                },
                ddl: Ddl::None,
                checksum: package_checksum(s.op, s.model, s.field, s.to, s.value),
                status: Status::Applied,
                at,
                duration_ms: 0,
                by: by.to_string(),
                role: "application".into(),
                app_version: obop_types::CORE_VERSION.into(),
                backup_id: backup_id.map(String::from),
                restore_point: None,
                note: if rollback {
                    "rollback of module.upgrade".into()
                } else {
                    String::new()
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_round_trip_through_a_record() {
        let e = Entry {
            id: core_id(2),
            module: "core".into(),
            kind: "rust".into(),
            step: "0002".into(),
            version: Some(2),
            name: "x".into(),
            class: Class::Backfill,
            ddl: Ddl::None,
            checksum: "ab".repeat(32),
            status: Status::Adopted,
            at: 5,
            duration_ms: 7,
            by: "operator".into(),
            role: "application".into(),
            app_version: "0.2.0".into(),
            backup_id: Some("b.jsonl.gz".into()),
            restore_point: None,
            note: "n".into(),
        };
        assert_eq!(Entry::from_fields(&e.id, &e.to_fields()), Some(e.clone()));
        assert_eq!(core_id(2), "core/0002");
        assert_eq!(core_failed_id(2, 99), "core/0002/failed-99");
        let mut broken = e.to_fields();
        broken.insert("status".into(), J::from("nonsense"));
        assert_eq!(Entry::from_fields("x", &broken), None);
    }

    #[test]
    fn reformatting_and_comments_do_not_change_a_source_digest_but_a_statement_does() {
        let a = "fn run() {\r\n    // note\r\n    do_it(1);\r\n}\r\n";
        let b = "fn run() {\n\n        do_it(1);   \n    // another note\n}\n";
        assert_eq!(source_digest(a), source_digest(b));
        assert_ne!(
            source_digest(a),
            source_digest(&a.replace("do_it(1)", "do_it(2)"))
        );
    }

    #[test]
    fn a_package_checksum_ignores_the_description_and_follows_the_operation() {
        let v = serde_json::json!("n/a");
        let a = package_checksum("set_default", "m", "f", None, Some(&v));
        assert_eq!(a, package_checksum("set_default", "m", "f", None, Some(&v)));
        assert_ne!(a, package_checksum("set_default", "m", "g", None, Some(&v)));
        assert_ne!(a, package_checksum("drop_field", "m", "f", None, Some(&v)));
        assert_eq!(a.len(), 64);
    }
}
