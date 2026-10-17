//! Schema versioning and `obop migrate` (B5, D59).
//!
//! A database carries its schema version in one record (`obop_meta/schema`). Today the schema is
//! the JSON-document layout of `obop-storage` plus the shapes the modules store in it; a change
//! to any stored shape gets a *step*: a Rust function registered, in order, in [`steps`]. Steps
//! are numbered from 1 without gaps, run in order, and must be idempotent.
//!
//! Two kinds of step:
//!
//! - [`Run::Ops`] returns the writes it wants; the runner applies them together with the step's
//!   version record in **one transaction**. A failure leaves nothing of the step behind.
//! - [`Run::Direct`] does its own writes (and none when `Ctx::dry_run` is set), in batches, for work too large for one transaction (the
//!   re-sealing of secrets). It must be safe to stop at any point and run again; the runner
//!   records the version only after it returns.
//!
//! Runners take the cross-process lock `obop.migrate` and re-read the version inside it, so two
//! processes started together apply each step once. A binary refuses to serve a database whose
//! schema is newer than it knows (a downgrade cannot be done safely), and by default runs
//! pending steps itself only when asked to (`--auto-migrate`; the default is on for SQLite and
//! off for PostgreSQL, where an operator runs `obop migrate run` before the new version starts).

use obop_keys::KeyRing;
use obop_keys::reseal;
use obop_storage::{Backend, Fields, Op};
use serde_json::Value as J;
use std::sync::Arc;

pub const META_MODEL: &str = "obop_meta";
pub const META_ID: &str = "schema";
const LOCK: &str = "obop.migrate";

/// What a step may use.
pub struct Ctx<'a> {
    pub backend: &'a Arc<dyn Backend>,
    pub ring: &'a KeyRing,
    pub dry_run: bool,
}

pub type OpsFn = fn(&Ctx) -> Result<Vec<Op>, String>;
pub type DirectFn = fn(&Ctx) -> Result<String, String>;

#[derive(Clone, Copy)]
pub enum Run {
    Ops(OpsFn),
    Direct(DirectFn),
}

#[derive(Clone, Copy)]
pub struct Step {
    pub version: u32,
    pub name: &'static str,
    pub run: Run,
}

/// The steps of this build, in order. Version 1 is the layout of today.
pub fn steps() -> Vec<Step> {
    vec![
        Step {
            version: 1,
            name: "adopt: record the schema version of a database created before versioning",
            run: Run::Direct(|_| Ok("nothing to change: the stored formats are version 1".into())),
        },
        Step {
            version: 2,
            name: "seal_format: re-seal secrets stored in the first envelope formats",
            run: Run::Direct(seal_format),
        },
    ]
}

/// Step 2: values sealed before the key ring (`v1.` second-factor secrets, `obs1:` connector
/// secrets) are re-sealed in the key ring's envelope with the key that opens them.
fn seal_format(c: &Ctx) -> Result<String, String> {
    if c.ring.is_empty() {
        return Err(
            "the stored secrets need the server key to be upgraded: set OBOP_SECRET_KEY \
             (and OBOP_SECRET_KEY_PREVIOUS for earlier keys)"
                .into(),
        );
    }
    let r = reseal::run(
        c.backend,
        c.ring,
        reseal::Options {
            only_legacy: true,
            dry_run: c.dry_run,
            ..reseal::Options::default()
        },
        &mut |_| {},
    )?;
    if !r.unopenable.is_empty() {
        return Err(format!(
            "{} secret(s) cannot be opened with the configured keys (first: {}); put the key \
             that sealed them in OBOP_SECRET_KEY_PREVIOUS",
            r.unopenable.len(),
            r.unopenable[0].place
        ));
    }
    if r.changed_meanwhile > 0 {
        return Err(format!(
            "{} secret(s) changed while they were re-sealed; run the migration again",
            r.changed_meanwhile
        ));
    }
    Ok(format!("{} secret(s) re-sealed", r.resealed))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// The version in the database; `None` when it has no version record.
    pub stored: Option<u32>,
    /// The version this build writes.
    pub latest: u32,
    /// The database holds no records at all.
    pub empty: bool,
    /// Steps that have not run: `(version, name)`.
    pub pending: Vec<(u32, &'static str)>,
    /// The database is newer than this build.
    pub ahead: bool,
}

impl Status {
    pub fn describe(&self) -> String {
        let at = self.stored.map_or_else(
            || "none (older than versioning)".to_string(),
            |v| v.to_string(),
        );
        let mut s = format!(
            "schema version {at}; this build writes version {}",
            self.latest
        );
        if self.empty && self.stored.is_none() {
            s = format!("empty database; this build writes version {}", self.latest);
        }
        if self.ahead {
            s.push_str("; the database is NEWER than this build");
        } else if self.pending.is_empty() {
            s.push_str("; up to date");
        } else {
            s.push_str(&format!("; {} step(s) pending", self.pending.len()));
        }
        s
    }
}

#[derive(Debug, Clone, Default)]
pub struct RunReport {
    /// `(version, name, what it did)` for each step that ran (or would run).
    pub applied: Vec<(u32, &'static str, String)>,
    pub dry_run: bool,
}

pub struct Migrator {
    backend: Arc<dyn Backend>,
    ring: KeyRing,
    steps: Vec<Step>,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Migrator {
    pub fn new(backend: Arc<dyn Backend>, ring: KeyRing) -> Migrator {
        Self::with_steps(backend, ring, steps())
    }

    /// As `new`, with a different list of steps (tests). The list must be numbered 1, 2, 3 ...
    pub fn with_steps(backend: Arc<dyn Backend>, ring: KeyRing, steps: Vec<Step>) -> Migrator {
        for (i, s) in steps.iter().enumerate() {
            assert_eq!(
                s.version as usize,
                i + 1,
                "migration steps must be numbered 1, 2, 3 ... without gaps"
            );
        }
        Migrator {
            backend,
            ring,
            steps,
        }
    }

    pub fn latest(&self) -> u32 {
        self.steps.len() as u32
    }

    fn stored(&self) -> Result<Option<u32>, String> {
        Ok(self
            .backend
            .get(META_MODEL, META_ID)?
            .and_then(|f| f.get("version").and_then(J::as_u64))
            .map(|v| v as u32))
    }

    pub fn status(&self) -> Result<Status, String> {
        let stored = self.stored()?;
        let empty = stored.is_none() && self.backend.total()? == 0;
        let current = stored.unwrap_or(0);
        let latest = self.latest();
        Ok(Status {
            stored,
            latest,
            empty,
            pending: self
                .steps
                .iter()
                .filter(|s| s.version > current)
                .map(|s| (s.version, s.name))
                .collect(),
            ahead: current > latest,
        })
    }

    /// A database with no records is at the latest version by construction: stamp it so the
    /// first commands that fill it do not look like an old database. Returns whether it stamped.
    pub fn stamp_if_fresh(&self) -> Result<bool, String> {
        if self.stored()?.is_some() || self.backend.total()? != 0 {
            return Ok(false);
        }
        let _g = self.backend.lock(LOCK)?;
        if self.stored()?.is_some() || self.backend.total()? != 0 {
            return Ok(false);
        }
        self.backend
            .apply(&[self.stamp(self.latest(), "created at this version")])?;
        Ok(true)
    }

    fn stamp(&self, version: u32, what: &str) -> Op {
        let mut f = Fields::new();
        f.insert("version".into(), J::from(version));
        f.insert("core".into(), J::from(obop_types::CORE_VERSION));
        f.insert("step".into(), J::from(what));
        f.insert("at".into(), J::from(now_secs()));
        Op::Put {
            model: META_MODEL.into(),
            id: META_ID.into(),
            fields: f,
        }
    }

    fn history(&self, s: &Step, did: &str) -> Op {
        let mut f = Fields::new();
        f.insert("version".into(), J::from(s.version));
        f.insert("name".into(), J::from(s.name));
        f.insert("result".into(), J::from(did));
        f.insert("core".into(), J::from(obop_types::CORE_VERSION));
        f.insert("at".into(), J::from(now_secs()));
        Op::Put {
            model: META_MODEL.into(),
            id: format!("step-{:04}", s.version),
            fields: f,
        }
    }

    /// Run the pending steps in order. With `dry_run`, only say which would run and what each
    /// reports (an `Ops` step reports the number of writes it plans; later steps are planned
    /// against the database as it is, not as the earlier steps would leave it).
    pub fn run(&self, dry_run: bool) -> Result<RunReport, String> {
        let _g = self.backend.lock(LOCK)?;
        let mut report = RunReport {
            dry_run,
            ..RunReport::default()
        };
        // The version is read inside the lock: another process may have just finished.
        let current = self.stored()?.unwrap_or(0);
        if current > self.latest() {
            return Err(format!(
                "the database is at schema version {current}, newer than this build ({}); \
                 run a newer obop, or restore a backup taken before the upgrade",
                self.latest()
            ));
        }
        let ctx = Ctx {
            backend: &self.backend,
            ring: &self.ring,
            dry_run,
        };
        for s in self.steps.iter().filter(|s| s.version > current) {
            let fail = |e: String| {
                format!(
                    "step {} ({}) failed: {e}; the database is at version {} and nothing of this step was kept",
                    s.version,
                    s.name,
                    s.version - 1
                )
            };
            match s.run {
                Run::Ops(f) => {
                    let mut ops = f(&ctx).map_err(fail)?;
                    let n = ops.len();
                    if dry_run {
                        report
                            .applied
                            .push((s.version, s.name, format!("{n} write(s) planned")));
                        continue;
                    }
                    ops.push(self.history(s, &format!("{n} write(s)")));
                    ops.push(self.stamp(s.version, s.name));
                    self.backend.apply(&ops).map_err(fail)?;
                    report
                        .applied
                        .push((s.version, s.name, format!("{n} write(s)")));
                }
                Run::Direct(f) => {
                    let did = f(&ctx).map_err(fail)?;
                    if dry_run {
                        report
                            .applied
                            .push((s.version, s.name, format!("would run: {did}")));
                        continue;
                    }
                    self.backend
                        .apply(&[self.history(s, &did), self.stamp(s.version, s.name)])?;
                    report.applied.push((s.version, s.name, did));
                }
            }
        }
        Ok(report)
    }

    /// What a server does at start: refuse a newer database, run pending steps when `auto`,
    /// otherwise refuse with the exact command to run. Returns lines to print.
    pub fn check_for_serve(&self, auto: bool, db_hint: &str) -> Result<Vec<String>, String> {
        let st = self.status()?;
        if st.ahead {
            return Err(format!(
                "the database is at schema version {}, newer than this build ({}). Run a newer \
                 obop, or restore a backup taken before the upgrade.",
                st.stored.unwrap_or(0),
                st.latest
            ));
        }
        if st.pending.is_empty() {
            return Ok(Vec::new());
        }
        if !auto {
            return Err(format!(
                "the database needs {} migration step(s) before this version can serve it \
                 (it is at schema version {}, this build writes {}). Take a backup, then run:\n  \
                 obop migrate run --db {db_hint}\nor start with --auto-migrate. See docs/DEPLOY.md.",
                st.pending.len(),
                st.stored.unwrap_or(0),
                st.latest
            ));
        }
        let r = self.run(false)?;
        Ok(r.applied
            .iter()
            .map(|(v, n, d)| format!("migrated to {v}: {n} ({d})"))
            .collect())
    }
}

#[cfg(test)]
mod tests;
