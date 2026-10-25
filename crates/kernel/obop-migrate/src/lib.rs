//! Schema versioning and `obop migrate` (B5, D59, D73).
//!
//! A database carries its schema version in one record (`obop_meta/schema`). Today the schema is
//! the JSON-document layout of `obop-storage` plus the shapes the modules store in it; a change
//! to any stored shape gets a *step*: a Rust function registered, in order, in [`steps`]. Steps
//! are numbered from 1 without gaps, run in order, and must be idempotent.
//!
//! Two kinds of step:
//!
//! - [`Run::Ops`] returns the writes it wants; the runner applies them together with the step's
//!   version record and its ledger row in **one transaction**. A failure leaves nothing of the
//!   step behind.
//! - [`Run::Direct`] does its own writes (and none when `Ctx::dry_run` is set), in batches, for work too large for one transaction (the
//!   re-sealing of secrets). It must be safe to stop at any point and run again; the runner
//!   writes a `partial` ledger row before it starts and records the version only after it
//!   returns.
//!
//! Every step that runs leaves a row in the migration ledger ([`ledger`], model `obop_migration`)
//! with the SHA-256 of its definition, how long it took, who ran it and what backup preceded it.
//! A step whose definition changed after it ran (drift) stops the server from starting
//! ([`Migrator::check_for_serve`]) and is reported by `obop migrate verify`.
//!
//! Runners take the cross-process lock `obop.migrate` and re-read the version inside it, so two
//! processes started together apply each step once. A binary refuses to serve a database whose
//! schema is newer than it knows (a downgrade cannot be done safely), and by default runs
//! pending steps itself only when asked to (`--auto-migrate`; the default is on for SQLite and
//! off for PostgreSQL, where an operator runs `obop migrate run` before the new version starts).

use obop_keys::KeyRing;
use obop_storage::{Backend, Fields, Op};
use serde_json::Value as J;
use std::sync::Arc;
use std::time::Instant;

pub mod ddl;
pub mod ledger;
pub mod plan;
mod steps;

pub use ddl::Class as DdlClass;
pub use ledger::{Class, Entry, Status as EntryStatus};
pub use plan::{Plan, PlanStep};

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
    /// Expand, backfill or contract (ENGINEERING 4.3). A contract step never ships in the release
    /// of its expand.
    pub class: Class,
    /// The cost of the DDL the step issues, if any (OBR-DAT-051 item 4).
    pub ddl: DdlClass,
    /// The previous release keeps serving against the schema this step leaves. A step that is not
    /// must declare `downtime`.
    pub compat_n_minus_1: bool,
    pub downtime: bool,
    /// The text of the step's source file; part of its checksum.
    pub source: &'static str,
    /// The models the step reads or writes at scale, for the plan's row counts and the window rule.
    pub tables: &'static [&'static str],
}

impl Step {
    pub const fn new(version: u32, name: &'static str, run: Run) -> Step {
        Step {
            version,
            name,
            run,
            class: Class::Expand,
            ddl: DdlClass::None,
            compat_n_minus_1: true,
            downtime: false,
            source: "",
            tables: &[],
        }
    }

    pub const fn class(mut self, c: Class) -> Step {
        self.class = c;
        self
    }

    pub const fn ddl(mut self, d: DdlClass) -> Step {
        self.ddl = d;
        self
    }

    pub const fn source(mut self, s: &'static str) -> Step {
        self.source = s;
        self
    }

    pub const fn tables(mut self, t: &'static [&'static str]) -> Step {
        self.tables = t;
        self
    }

    /// Not compatible with the previous release: the service must be stopped.
    pub const fn needs_downtime(mut self) -> Step {
        self.compat_n_minus_1 = false;
        self.downtime = true;
        self
    }

    /// SHA-256 of the step's definition, hex: what the ledger records and compares.
    pub fn checksum(&self) -> String {
        let run = match self.run {
            Run::Ops(_) => "ops",
            Run::Direct(_) => "direct",
        };
        let text = format!(
            "obop-step/v1\nmodule=core\nstep={:04}\nkind=rust\nname={}\nclass={}\nddl={}\nrun={run}\ncompat={}\ndowntime={}\ntables={}\nsource={}\n",
            self.version,
            self.name,
            self.class.as_str(),
            self.ddl.as_str(),
            self.compat_n_minus_1,
            self.downtime,
            self.tables.join(","),
            ledger::source_digest(self.source),
        );
        ledger::sha256_hex(text.as_bytes())
    }
}

/// The steps of this build, in order. Version 1 is the layout of today.
pub fn steps() -> Vec<Step> {
    vec![
        Step::new(
            1,
            "adopt: record the schema version of a database created before versioning",
            Run::Direct(steps::s0001_adopt::run),
        )
        .source(include_str!("steps/s0001_adopt.rs")),
        Step::new(
            2,
            "seal_format: re-seal secrets stored in the first envelope formats",
            Run::Direct(steps::s0002_seal_format::run),
        )
        .class(Class::Backfill)
        .source(include_str!("steps/s0002_seal_format.rs")),
    ]
}

/// A release check: a step that is not compatible with the previous release must say that the
/// release needs downtime (OBR-DAT-053 item 4).
pub fn check_declarations(steps: &[Step]) -> Result<(), String> {
    for s in steps {
        if !s.compat_n_minus_1 && !s.downtime {
            return Err(format!(
                "step {} ({}) is not compatible with the previous release; declare downtime or make it expand-only",
                s.version, s.name
            ));
        }
    }
    Ok(())
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

/// What a run records and how it behaves beyond the steps themselves.
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    pub dry_run: bool,
    /// The verified backup taken before this run: written to every ledger row of the run.
    pub backup_id: Option<String>,
    /// The PostgreSQL restore point created before this run.
    pub restore_point: Option<String>,
    /// Allow a long-lock statement on a table above `ddl::WINDOW_ROWS` records (a named window).
    pub window: bool,
}

/// A step whose definition is no longer the one that ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drift {
    pub version: u32,
    pub name: String,
    pub recorded: String,
    pub current: String,
}

impl Drift {
    pub fn message(&self) -> String {
        format!(
            "migration step {} ({}) was changed after it ran on this database (the ledger holds checksum {}, this build computes {}). An applied step is never edited: use the obop version that wrote the database, restore a backup, or put the step back as it was and add a new step",
            self.version,
            self.name,
            &self.recorded[..self.recorded.len().min(12)],
            &self.current[..self.current.len().min(12)]
        )
    }
}

/// What `obop migrate verify` found.
#[derive(Debug, Clone, Default)]
pub struct Verify {
    /// Things that are wrong; an empty list is a clean ledger.
    pub problems: Vec<String>,
    /// Things worth knowing that are not wrong.
    pub notes: Vec<String>,
}

impl Verify {
    pub fn ok(&self) -> bool {
        self.problems.is_empty()
    }
}

pub struct Migrator {
    backend: Arc<dyn Backend>,
    ring: KeyRing,
    steps: Vec<Step>,
    actor: String,
    window_rows: u64,
}

fn now_secs() -> u64 {
    ledger::now_secs()
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
            actor: "operator".into(),
            window_rows: ddl::WINDOW_ROWS,
        }
    }

    /// The table size above which a long-lock step needs `--window` (tests lower it).
    pub fn with_window_rows(mut self, rows: u64) -> Migrator {
        self.window_rows = rows;
        self
    }

    /// Who the ledger names as running the steps (`operator` by default; a server that migrates
    /// at start says `server`).
    pub fn by(mut self, actor: &str) -> Migrator {
        self.actor = actor.to_string();
        self
    }

    pub fn latest(&self) -> u32 {
        self.steps.len() as u32
    }

    pub fn steps(&self) -> &[Step] {
        &self.steps
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

    // ---- the ledger ----------------------------------------------------------------------

    fn entry(
        &self,
        s: &Step,
        status: ledger::Status,
        duration_ms: u64,
        note: &str,
        opts: &RunOptions,
    ) -> Entry {
        Entry {
            id: ledger::core_id(s.version),
            module: "core".into(),
            kind: "rust".into(),
            step: format!("{:04}", s.version),
            version: Some(s.version),
            name: s.name.to_string(),
            class: s.class,
            ddl: s.ddl,
            checksum: s.checksum(),
            status,
            at: now_secs(),
            duration_ms,
            by: self.actor.clone(),
            role: "application".into(),
            app_version: obop_types::CORE_VERSION.into(),
            backup_id: opts.backup_id.clone(),
            restore_point: opts.restore_point.clone(),
            note: note.to_string(),
        }
    }

    /// Every row of the ledger, in the order written.
    pub fn ledger(&self) -> Result<Vec<Entry>, String> {
        ledger::read_all(&*self.backend)
    }

    /// The core steps that ran, per the ledger, whose definition is no longer this build's.
    pub fn drift(&self) -> Result<Vec<Drift>, String> {
        let current = self.stored()?.unwrap_or(0);
        let rows = self.ledger()?;
        let mut out = Vec::new();
        for s in self.steps.iter().filter(|s| s.version <= current) {
            let id = ledger::core_id(s.version);
            let Some(row) = rows.iter().find(|r| r.id == id) else {
                continue;
            };
            if matches!(
                row.status,
                ledger::Status::Applied | ledger::Status::Adopted
            ) && row.checksum != s.checksum()
            {
                out.push(Drift {
                    version: s.version,
                    name: s.name.to_string(),
                    recorded: row.checksum.clone(),
                    current: s.checksum(),
                });
            }
        }
        Ok(out)
    }

    /// Compare the ledger with the code and the version: drift, a step that was recorded as
    /// unfinished, rows for steps this build does not know, steps below the version with no row.
    /// What it cannot compare yet is said in `notes`: the live catalogue (hand-made indexes and
    /// missing triggers) belongs to the DDL component of D77.
    pub fn verify(&self) -> Result<Verify, String> {
        let mut v = Verify::default();
        let current = self.stored()?.unwrap_or(0);
        if current > self.latest() {
            v.problems.push(format!(
                "the database is at schema version {current}, newer than this build ({})",
                self.latest()
            ));
        }
        let rows = self.ledger()?;
        for d in self.drift()? {
            v.problems.push(d.message());
        }
        let core_rows: Vec<&Entry> = rows.iter().filter(|r| r.module == "core").collect();
        for s in &self.steps {
            let row = core_rows
                .iter()
                .find(|r| r.id == ledger::core_id(s.version));
            match row {
                None if s.version <= current => v.notes.push(format!(
                    "step {} ({}) has no ledger row yet; the next run or start adopts it",
                    s.version, s.name
                )),
                Some(r) if r.status == ledger::Status::Partial && s.version <= current => {
                    v.problems.push(format!(
                        "step {} ({}) is recorded as unfinished but the schema version is past it",
                        s.version, s.name
                    ));
                }
                Some(r) if r.status == ledger::Status::Partial => v.notes.push(format!(
                    "step {} ({}) was interrupted; the next run resumes it",
                    s.version, s.name
                )),
                _ => {}
            }
        }
        for r in &core_rows {
            if let Some(n) = r.version
                && r.id == ledger::core_id(n)
                && n > self.latest()
            {
                v.problems.push(format!(
                    "the ledger holds step {n} ({}), which this build does not have",
                    r.name
                ));
            }
        }
        let failed = rows
            .iter()
            .filter(|r| r.status == ledger::Status::Failed)
            .count();
        if failed > 0 {
            v.notes.push(format!(
                "{failed} failed attempt(s) are recorded in the ledger"
            ));
        }
        v.notes.push(format!(
            "{} ledger row(s); the live catalogue (indexes, triggers) is not compared yet (D77)",
            rows.len()
        ));
        Ok(v)
    }

    /// Record the steps up to `current` that have no row, without replaying them. `what` says why
    /// they have none.
    fn adopt_ops(&self, upto: u32, what: &str) -> Result<Vec<Op>, String> {
        let rows = self.ledger()?;
        let mut ops = Vec::new();
        for s in self.steps.iter().filter(|s| s.version <= upto) {
            if rows.iter().any(|r| r.id == ledger::core_id(s.version)) {
                continue;
            }
            // A database from before the ledger kept a line per step; use its time when it has one.
            let at = self
                .backend
                .get(META_MODEL, &format!("step-{:04}", s.version))?
                .and_then(|f| f.get("at").and_then(J::as_u64));
            let mut e = self.entry(s, ledger::Status::Adopted, 0, what, &RunOptions::default());
            if let Some(at) = at {
                e.at = at;
            }
            ops.push(e.put());
        }
        Ok(ops)
    }

    /// Write `adopted` rows for steps that ran before the ledger existed. Under the lock, once.
    fn adopt_missing(&self) -> Result<usize, String> {
        let current = self.stored()?.unwrap_or(0).min(self.latest());
        let ops = self.adopt_ops(
            current,
            "ran before the ledger existed; recorded without being replayed",
        )?;
        let n = ops.len();
        if n > 0 {
            self.backend.apply(&ops)?;
        }
        Ok(n)
    }

    // ---- plan ----------------------------------------------------------------------------

    /// The pending steps with their cost. Reads counts, writes nothing.
    pub fn plan(&self) -> Result<Plan, String> {
        let current = self.stored()?.unwrap_or(0);
        if current > self.latest() {
            return Err(format!(
                "the database is at schema version {current}, newer than this build ({})",
                self.latest()
            ));
        }
        let mut steps = Vec::new();
        for s in self.steps.iter().filter(|s| s.version > current) {
            let mut tables = Vec::new();
            for t in s.tables {
                tables.push((t.to_string(), self.backend.count(t)? as u64));
            }
            let big = tables.iter().map(|t| t.1).max().unwrap_or(0);
            steps.push(PlanStep {
                version: s.version,
                name: s.name.to_string(),
                class: s.class,
                ddl: s.ddl,
                compat_n_minus_1: s.compat_n_minus_1,
                downtime: s.downtime,
                checksum: s.checksum(),
                needs_window: s.ddl.holds_a_long_lock() && big > self.window_rows,
                tables,
            });
        }
        let core = obop_types::CORE_VERSION.to_string();
        Ok(Plan {
            id: Plan::digest(current, self.latest(), &core, &steps),
            from: current,
            to: self.latest(),
            core_version: core,
            steps,
            created_at: now_secs(),
        })
    }

    /// Keep a plan so that a production `run` can find it.
    pub fn store_plan(&self, plan: &Plan) -> Result<(), String> {
        self.backend.apply(&[Op::Put {
            model: ledger::PLAN_MODEL.into(),
            id: plan.id.clone(),
            fields: plan.to_fields(),
        }])
    }

    /// The stored plan with this id, if any.
    pub fn stored_plan(&self, id: &str) -> Result<Option<Plan>, String> {
        Ok(self
            .backend
            .get(ledger::PLAN_MODEL, id)?
            .and_then(|f| Plan::from_fields(id, &f)))
    }

    // ---- running -------------------------------------------------------------------------

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
        let mut ops = self.adopt_ops(self.latest(), "created at this version; no step was run")?;
        ops.push(self.stamp(self.latest(), "created at this version"));
        self.backend.apply(&ops)?;
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

    /// The message for a step that failed, after recording the attempt in the ledger.
    fn failed(&self, s: &Step, e: &str, started: Instant, opts: &RunOptions) -> String {
        if !opts.dry_run {
            let mut row = self.entry(
                s,
                ledger::Status::Failed,
                started.elapsed().as_millis() as u64,
                &e.chars().take(500).collect::<String>(),
                opts,
            );
            row.id = ledger::core_failed_id(s.version, ledger::now_ms());
            // Best effort: the failure itself is what the operator needs to see.
            let _ = self.backend.apply(&[row.insert()]);
        }
        format!(
            "step {} ({}) failed: {e}; the database is at version {} and nothing of this step was kept",
            s.version,
            s.name,
            s.version - 1
        )
    }

    /// Run the pending steps in order. With `dry_run`, only say which would run and what each
    /// reports (an `Ops` step reports the number of writes it plans; later steps are planned
    /// against the database as it is, not as the earlier steps would leave it).
    pub fn run(&self, dry_run: bool) -> Result<RunReport, String> {
        self.run_with(&RunOptions {
            dry_run,
            ..RunOptions::default()
        })
    }

    pub fn run_with(&self, opts: &RunOptions) -> Result<RunReport, String> {
        let dry_run = opts.dry_run;
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
        if !dry_run {
            self.adopt_missing()?;
        }
        if let Some(d) = self.drift()?.first() {
            return Err(d.message());
        }
        if !opts.window {
            let plan = self.plan()?;
            if let Some(s) = plan.steps.iter().find(|s| s.needs_window) {
                return Err(format!(
                    "step {} ({}) holds a long lock on a table of more than {} records; run it in a maintenance window with --window",
                    s.version, s.name, self.window_rows
                ));
            }
        }
        let ctx = Ctx {
            backend: &self.backend,
            ring: &self.ring,
            dry_run,
        };
        for s in self.steps.iter().filter(|s| s.version > current) {
            let started = Instant::now();
            match s.run {
                Run::Ops(f) => {
                    let mut ops = f(&ctx).map_err(|e| self.failed(s, &e, started, opts))?;
                    let n = ops.len();
                    if dry_run {
                        report
                            .applied
                            .push((s.version, s.name, format!("{n} write(s) planned")));
                        continue;
                    }
                    let note = format!("{n} write(s)");
                    ops.push(
                        self.entry(
                            s,
                            ledger::Status::Applied,
                            started.elapsed().as_millis() as u64,
                            &note,
                            opts,
                        )
                        .put(),
                    );
                    ops.push(self.history(s, &note));
                    ops.push(self.stamp(s.version, s.name));
                    self.backend
                        .apply(&ops)
                        .map_err(|e| self.failed(s, &e, started, opts))?;
                    report.applied.push((s.version, s.name, note));
                }
                Run::Direct(f) => {
                    if !dry_run {
                        // Written before the work: if the process dies in the middle, the ledger
                        // says `partial` and the next run resumes (steps are idempotent).
                        self.backend
                            .apply(&[self
                                .entry(s, ledger::Status::Partial, 0, "started", opts)
                                .put()])
                            .map_err(|e| self.failed(s, &e, started, opts))?;
                    }
                    let did = f(&ctx).map_err(|e| self.failed(s, &e, started, opts))?;
                    if dry_run {
                        report
                            .applied
                            .push((s.version, s.name, format!("would run: {did}")));
                        continue;
                    }
                    self.backend
                        .apply(&[
                            self.entry(
                                s,
                                ledger::Status::Applied,
                                started.elapsed().as_millis() as u64,
                                &did,
                                opts,
                            )
                            .put(),
                            self.history(s, &did),
                            self.stamp(s.version, s.name),
                        ])
                        .map_err(|e| self.failed(s, &e, started, opts))?;
                    report.applied.push((s.version, s.name, did));
                }
            }
        }
        Ok(report)
    }

    /// What a server does at start: refuse a newer database or a changed step, run pending steps
    /// when `auto`, otherwise refuse with the exact command to run. Returns lines to print.
    pub fn check_for_serve(&self, auto: bool, db_hint: &str) -> Result<Vec<String>, String> {
        self.check_for_serve_with(auto, db_hint, &RunOptions::default())
    }

    /// As [`Migrator::check_for_serve`], with what a run records (a backup taken first).
    pub fn check_for_serve_with(
        &self,
        auto: bool,
        db_hint: &str,
        opts: &RunOptions,
    ) -> Result<Vec<String>, String> {
        let st = self.status()?;
        if st.ahead {
            return Err(format!(
                "the database is at schema version {}, newer than this build ({}). Run a newer \
                 obop, or restore a backup taken before the upgrade.",
                st.stored.unwrap_or(0),
                st.latest
            ));
        }
        if let Some(d) = self.drift()?.first() {
            return Err(d.message());
        }
        if st.pending.is_empty() {
            // A database from before the ledger: record its history once.
            if st.stored.is_some() && self.ledger()?.iter().all(|r| r.module != "core") {
                let _g = self.backend.lock(LOCK)?;
                self.adopt_missing()?;
            }
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
        let r = self.run_with(opts)?;
        Ok(r.applied
            .iter()
            .map(|(v, n, d)| format!("migrated to {v}: {n} ({d})"))
            .collect())
    }

    /// Record a rollback by restore: why, from which backup, what the restored database held
    /// before and what the migration had written. Written into the restored database, so it is
    /// the first thing its ledger shows afterwards.
    pub fn record_rollback(
        &self,
        reason: &str,
        backup_id: &str,
        schema_before: Option<u32>,
        schema_after: Option<u32>,
        discarded: &[Entry],
    ) -> Result<(), String> {
        let id = format!("core/rollback-{}", ledger::now_ms());
        let mut f = Fields::new();
        f.insert("module".into(), J::from("core"));
        f.insert("kind".into(), J::from("rollback"));
        f.insert("step".into(), J::from("rollback"));
        f.insert(
            "name".into(),
            J::from("rollback by restore of a verified backup"),
        );
        f.insert("class".into(), J::from("contract"));
        f.insert("ddl".into(), J::from("none"));
        f.insert(
            "checksum".into(),
            J::from(ledger::sha256_hex(reason.as_bytes())),
        );
        f.insert("status".into(), J::from("applied"));
        f.insert("at".into(), J::from(now_secs()));
        f.insert("duration_ms".into(), J::from(0));
        f.insert("by".into(), J::from(self.actor.as_str()));
        f.insert("role".into(), J::from("application"));
        f.insert("app_version".into(), J::from(obop_types::CORE_VERSION));
        f.insert("backup_id".into(), J::from(backup_id));
        f.insert("restore_point".into(), J::Null);
        f.insert("version".into(), schema_after.map_or(J::Null, J::from));
        f.insert("note".into(), J::from(reason));
        f.insert(
            "schema_before".into(),
            schema_before.map_or(J::Null, J::from),
        );
        f.insert(
            "discarded".into(),
            J::Array(
                discarded
                    .iter()
                    .map(|e| {
                        serde_json::json!({"id": e.id, "name": e.name, "status": e.status.as_str(), "at": e.at})
                    })
                    .collect(),
            ),
        );
        self.backend.apply(&[Op::Insert {
            model: ledger::MODEL.into(),
            id,
            fields: f,
        }])
    }
}

#[cfg(test)]
mod ledger_tests;
#[cfg(test)]
mod tests;
