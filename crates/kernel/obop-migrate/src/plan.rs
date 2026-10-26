//! A migration plan: the pending steps of one database towards the schema of this build, with the
//! cost of each (OBR-DAT-051 item 4, OBR-DAT-053 item 1b).
//!
//! `obop migrate plan` stores the plan it printed as a record (`obop_migration_plan`, keyed by a
//! digest of its contents); in a production run `obop migrate run` refuses unless it finds the
//! stored plan for exactly this database state and this build, so what the operator read is what
//! runs. A plan is invalidated by anything that changes the answer: another version in the
//! database, a changed step, a different core version.

use crate::ddl::Class as Ddl;
use crate::ledger::{self, Class};
use obop_storage::Fields;
use serde_json::{Value as J, json};

#[derive(Debug, Clone, PartialEq)]
pub struct PlanStep {
    pub version: u32,
    pub name: String,
    pub class: Class,
    pub ddl: Ddl,
    /// The previous release keeps working against the schema this step leaves (expand-only).
    pub compat_n_minus_1: bool,
    /// The step needs the service stopped.
    pub downtime: bool,
    pub checksum: String,
    /// The models (tables) the step touches and how many records each holds now.
    pub tables: Vec<(String, u64)>,
    /// A long-lock statement on a table above `ddl::WINDOW_ROWS` rows: runs only with `--window`.
    pub needs_window: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// 16 hex digits of a digest of everything below but `created_at`.
    pub id: String,
    pub from: u32,
    pub to: u32,
    pub core_version: String,
    pub steps: Vec<PlanStep>,
    pub created_at: u64,
}

impl Plan {
    pub fn downtime(&self) -> bool {
        self.steps.iter().any(|s| s.downtime)
    }

    pub fn needs_window(&self) -> bool {
        self.steps.iter().any(|s| s.needs_window)
    }

    /// The largest number of records in any table the plan touches.
    pub fn largest_table(&self) -> u64 {
        self.steps
            .iter()
            .flat_map(|s| s.tables.iter().map(|t| t.1))
            .max()
            .unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn digest(from: u32, to: u32, core: &str, steps: &[PlanStep]) -> String {
        let mut text = format!("obop-plan/v1\nfrom={from}\nto={to}\ncore={core}\n");
        for s in steps {
            text.push_str(&format!("step={}:{}\n", s.version, s.checksum));
        }
        ledger::sha256_hex(text.as_bytes())[..16].to_string()
    }

    pub fn to_fields(&self) -> Fields {
        let mut f = Fields::new();
        f.insert("from".into(), J::from(self.from));
        f.insert("to".into(), J::from(self.to));
        f.insert("core_version".into(), J::from(self.core_version.as_str()));
        f.insert("created_at".into(), J::from(self.created_at));
        f.insert(
            "steps".into(),
            J::Array(
                self.steps
                    .iter()
                    .map(|s| {
                        json!({
                            "version": s.version, "name": s.name, "class": s.class.as_str(),
                            "ddl": s.ddl.as_str(), "compat_n_minus_1": s.compat_n_minus_1,
                            "downtime": s.downtime, "checksum": s.checksum,
                            "tables": s.tables.iter().map(|(t, n)| json!([t, n])).collect::<Vec<_>>(),
                            "needs_window": s.needs_window,
                        })
                    })
                    .collect(),
            ),
        );
        f
    }

    pub fn from_fields(id: &str, f: &Fields) -> Option<Plan> {
        let steps = f
            .get("steps")?
            .as_array()?
            .iter()
            .map(|s| {
                Some(PlanStep {
                    version: s["version"].as_u64()? as u32,
                    name: s["name"].as_str()?.to_string(),
                    class: Class::parse(s["class"].as_str()?)?,
                    ddl: Ddl::parse(s["ddl"].as_str()?)?,
                    compat_n_minus_1: s["compat_n_minus_1"].as_bool()?,
                    downtime: s["downtime"].as_bool()?,
                    checksum: s["checksum"].as_str()?.to_string(),
                    tables: s["tables"]
                        .as_array()?
                        .iter()
                        .filter_map(|t| Some((t[0].as_str()?.to_string(), t[1].as_u64()?)))
                        .collect(),
                    needs_window: s["needs_window"].as_bool()?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Plan {
            id: id.to_string(),
            from: f.get("from")?.as_u64()? as u32,
            to: f.get("to")?.as_u64()? as u32,
            core_version: f.get("core_version")?.as_str()?.to_string(),
            steps,
            created_at: f.get("created_at")?.as_u64()?,
        })
    }

    /// The plan in words, one line per step.
    pub fn describe(&self) -> String {
        if self.steps.is_empty() {
            return format!(
                "nothing to do: the database is at schema version {}",
                self.from
            );
        }
        let mut s = format!(
            "plan {}: schema version {} to {} with obop {}\n",
            self.id, self.from, self.to, self.core_version
        );
        for st in &self.steps {
            let tables = if st.tables.is_empty() {
                "no table named".to_string()
            } else {
                st.tables
                    .iter()
                    .map(|(t, n)| format!("{t} ({n} records)"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            s.push_str(&format!(
                "  step {}: {} [{}, ddl {}, {}; {}]{}\n",
                st.version,
                st.name,
                st.class.as_str(),
                st.ddl.as_str(),
                if st.compat_n_minus_1 {
                    "previous release keeps working"
                } else if st.downtime {
                    "DOWNTIME: stop the service"
                } else {
                    "NOT compatible with the previous release"
                },
                tables,
                if st.needs_window {
                    " NEEDS --window (long lock on a big table)"
                } else {
                    ""
                }
            ));
        }
        if self.downtime() {
            s.push_str("this release needs downtime\n");
        }
        s
    }
}
