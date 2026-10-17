//! Updating the instance itself from inside it (D63).
//!
//! An instance cannot hold the hosting provider's credentials, so it never upgrades itself. A
//! *managed* instance (one created by a control plane) holds a narrow per-instance update token
//! and asks the control plane to move it to a published release; the control plane runs the
//! rolling-upgrade path it already has (redeploy the image, wait for the health check, roll back
//! when the new version does not come up). A *self-hosted* instance has no control plane: it
//! gets the exact commands to run instead.
//!
//! This module is the instance's half. It reads the release feed, judges each release against
//! the installed modules (their declared core ranges), takes a backup, asks the control plane,
//! and follows the result. Everything it does is an admin command, audited in `security_event`,
//! and an agent may only propose it.

use crate::release::{Feed, Release, fetch_feed, http_get, http_post};
use obop_keys::KeyRing;
use obop_pkg::Installer;
use obop_pkg::installer::CoreCompat;
use obop_pkg::semver::Version;
use obop_storage::{Backend, Fields, Op};
use serde_json::{Value as J, json};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const TABLE: &str = "obop_update";
const LOCK: &str = "instance.update";
const TOKEN_PURPOSE: &str = "update-token";
/// How long a request may stay `running` with the control plane silent before it is called lost.
const LOST_AFTER_SECS: u64 = 30 * 60;

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Writes a backup into a directory and returns the path of the data file.
pub type BackupFn = Arc<dyn Fn(&Path) -> Result<String, String> + Send + Sync>;

/// Where the updater reads from; all of it from the environment.
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// The release feed, or none when `OBOP_UPDATE_FEED=off`.
    pub feed: Option<String>,
    /// The control plane (`OBOP_UPDATE_URL`), for a managed instance.
    pub control_url: Option<String>,
    /// The update token (`OBOP_UPDATE_TOKEN`); sealed into the database at start-up.
    pub token: Option<String>,
    /// Where backups go (`OBOP_BACKUP_DIR`).
    pub backup_dir: Option<PathBuf>,
}

impl Config {
    pub fn from_env() -> Config {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let control_url = var("OBOP_UPDATE_URL").map(|u| u.trim_end_matches('/').to_string());
        let feed = match var("OBOP_UPDATE_FEED") {
            Some(f) if f.trim().eq_ignore_ascii_case("off") => None,
            Some(f) => Some(f),
            None => Some(
                control_url
                    .as_ref()
                    .map_or(crate::release::DEFAULT_FEED.to_string(), |c| {
                        format!("{c}/api/releases")
                    }),
            ),
        };
        Config {
            feed,
            control_url,
            token: var("OBOP_UPDATE_TOKEN"),
            backup_dir: var("OBOP_BACKUP_DIR").map(PathBuf::from),
        }
    }
}

/// How this instance gets updated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A control plane runs the upgrade when asked.
    Managed,
    /// Nobody does it for you: the page shows the commands.
    SelfHosted,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Managed => "managed",
            Self::SelfHosted => "self_hosted",
        }
    }
}

/// One release as the page shows it.
#[derive(Debug, Clone)]
pub struct ReleaseView {
    pub release: Release,
    /// Newer than the running core.
    pub newer: bool,
    /// Can be installed over the running core now: nothing blocks it.
    pub installable: bool,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    /// Every installed package against this core.
    pub modules: Vec<CoreCompat>,
}

#[derive(Debug, Clone)]
pub struct Check {
    pub mode: Mode,
    pub current: String,
    /// The newest published release, if the feed has any.
    pub latest: Option<String>,
    pub update_available: bool,
    pub feed: String,
    pub feed_error: Option<String>,
    pub checked_at: u64,
    pub releases: Vec<ReleaseView>,
    pub dropped: Vec<String>,
    /// For a self-hosted instance (and as a manual route for a managed one): what to run.
    pub instructions: Vec<String>,
    pub backup_dir: Option<String>,
}

/// The state of the last update request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    /// `idle`, `requested`, `running`, `succeeded`, `failed` or `rolled_back`.
    pub state: String,
    /// `update` or `rollback`.
    pub kind: String,
    pub target: String,
    pub from: String,
    pub phase: String,
    pub error: String,
    pub backup: String,
    pub started_at: u64,
    pub started_by: String,
    pub finished_at: u64,
    /// The version the running process reports.
    pub current: String,
    /// Whether the last successful update can be reversed through the control plane.
    pub can_rollback: bool,
}

impl Status {
    fn from_fields(f: &Fields, current: &str) -> Status {
        let t = |k: &str| f.get(k).and_then(J::as_str).unwrap_or("").to_string();
        let n = |k: &str| f.get(k).and_then(J::as_u64).unwrap_or(0);
        let state = if t("state").is_empty() {
            "idle".to_string()
        } else {
            t("state")
        };
        let can_rollback = state == "succeeded"
            && t("kind") == "update"
            && !t("from").is_empty()
            && t("target") == current;
        Status {
            state,
            kind: t("kind"),
            target: t("target"),
            from: t("from"),
            phase: t("phase"),
            error: t("error"),
            backup: t("backup"),
            started_at: n("started_at"),
            started_by: t("started_by"),
            finished_at: n("finished_at"),
            current: current.to_string(),
            can_rollback,
        }
    }

    pub fn busy(&self) -> bool {
        matches!(self.state.as_str(), "requested" | "running")
    }
}

pub struct Updater {
    backend: Arc<dyn Backend>,
    installer: Arc<Installer>,
    ring: KeyRing,
    cfg: Config,
    backup: Mutex<Option<BackupFn>>,
    core: Version,
}

impl Updater {
    pub fn new(
        backend: Arc<dyn Backend>,
        installer: Arc<Installer>,
        ring: KeyRing,
        cfg: Config,
    ) -> Arc<Self> {
        let core = installer.core_version();
        Self::new_for_core(backend, installer, ring, cfg, core)
    }

    /// As `new`, for a process that reports `core` (tests that stand in for the restarted
    /// instance after an update).
    pub fn new_for_core(
        backend: Arc<dyn Backend>,
        installer: Arc<Installer>,
        ring: KeyRing,
        cfg: Config,
        core: Version,
    ) -> Arc<Self> {
        let me = Arc::new(Self {
            core,
            backend,
            installer,
            ring,
            cfg,
            backup: Mutex::new(None),
        });
        me.adopt_config();
        me
    }

    /// Say how to take a backup (the binary knows the database URL and the blob store).
    pub fn set_backup(&self, f: BackupFn) {
        *self.backup.lock().unwrap_or_else(|e| e.into_inner()) = Some(f);
    }

    pub fn core_version(&self) -> &Version {
        &self.core
    }

    /// Keep the update token and the control plane address in the database, the token sealed
    /// under the instance's key, so the environment variable is only needed to hand them over.
    fn adopt_config(&self) {
        let (Some(token), Some(url)) = (&self.cfg.token, &self.cfg.control_url) else {
            return;
        };
        let stored = self.stored_token();
        if stored.as_deref() == Some(token.as_str()) {
            return;
        }
        let Ok(sealed) = self.ring.seal_str(TOKEN_PURPOSE, token) else {
            eprintln!("update token not stored: this instance has no encryption key");
            return;
        };
        let mut f = Fields::new();
        f.insert("token".into(), J::from(sealed));
        f.insert("control_url".into(), J::from(url.as_str()));
        f.insert("stored_at".into(), J::from(now()));
        let r = self.backend.update(TABLE, "config", &f);
        if !matches!(r, Ok(true))
            && let Err(e) = self.backend.insert(TABLE, "config", &f)
        {
            eprintln!("could not store the update token: {e}");
        }
    }

    fn stored_token(&self) -> Option<String> {
        let f = self.backend.get(TABLE, "config").ok().flatten()?;
        let sealed = f.get("token")?.as_str()?;
        self.ring.open_str(TOKEN_PURPOSE, sealed).ok()
    }

    fn token(&self) -> Option<String> {
        self.cfg.token.clone().or_else(|| self.stored_token())
    }

    fn control_url(&self) -> Option<String> {
        self.cfg.control_url.clone().or_else(|| {
            self.backend
                .get(TABLE, "config")
                .ok()
                .flatten()?
                .get("control_url")?
                .as_str()
                .map(String::from)
        })
    }

    pub fn mode(&self) -> Mode {
        if self.token().is_some() && self.control_url().is_some() {
            Mode::Managed
        } else {
            Mode::SelfHosted
        }
    }

    fn feed_url(&self) -> Option<String> {
        self.cfg.feed.clone()
    }

    // ---- the check ---------------------------------------------------------------------

    /// Read the feed and judge every release against this instance. The feed is only read
    /// here, when an administrator asks.
    pub fn check(&self) -> Check {
        let mode = self.mode();
        let mut out = Check {
            mode,
            current: self.core.to_string(),
            latest: None,
            update_available: false,
            feed: self.feed_url().unwrap_or_default(),
            feed_error: None,
            checked_at: now(),
            releases: Vec::new(),
            dropped: Vec::new(),
            instructions: Vec::new(),
            backup_dir: self
                .cfg
                .backup_dir
                .as_ref()
                .map(|d| d.display().to_string()),
        };
        let feed = match &self.cfg.feed {
            None => {
                out.feed_error =
                    Some("the release feed is switched off (OBOP_UPDATE_FEED=off)".into());
                Feed::default()
            }
            Some(url) => match fetch_feed(url) {
                Ok(f) => f,
                Err(e) => {
                    out.feed_error = Some(e);
                    Feed::default()
                }
            },
        };
        self.judge(feed, &mut out);
        out
    }

    /// As `check`, over a feed already in hand (tests, and `check` itself).
    pub fn judge(&self, feed: Feed, out: &mut Check) {
        out.dropped = feed.dropped;
        let pending = self.installer.updates();
        let offered = self.installer.offered();
        for r in feed.releases {
            let Some(v) = r.semver() else { continue };
            let newer = v > self.core;
            let modules = if newer {
                self.installer.core_compat(&v)
            } else {
                Vec::new()
            };
            let mut blockers = Vec::new();
            let mut warnings = Vec::new();
            if newer {
                for m in modules.iter().filter(|m| !m.compatible) {
                    let mut why = format!(
                        "{} {} would not run on core {v}: {}",
                        m.id,
                        m.version,
                        m.reasons.join("; ")
                    );
                    // A newer version of the module that fits both cores can be installed first.
                    if let Some(l) = offered.get(&m.id)
                        && obop_pkg::semver::Req::parse(&l.requires_core)
                            .is_ok_and(|q| q.matches(&v) && q.matches(&self.core))
                        && pending.iter().any(|u| u.id == m.id)
                    {
                        why.push_str(&format!(
                            "; version {} of it supports both: update the module first (Modules, Updates)",
                            l.version
                        ));
                    }
                    blockers.push(why);
                }
                if !r.min_from.is_empty()
                    && obop_pkg::semver::Req::parse(&format!(">={}", r.min_from))
                        .is_ok_and(|q| !q.matches(&self.core))
                {
                    blockers.push(format!(
                        "{v} can only be installed over {} or newer; this instance runs {}",
                        r.min_from, self.core
                    ));
                }
                if v.major == 0 && (v.minor != self.core.minor || v.major != self.core.major) {
                    warnings.push(
                        "while the core is below 1.0 a new minor version may change behaviour; read the release notes"
                            .into(),
                    );
                }
                if !pending.is_empty() {
                    warnings.push(format!(
                        "{} module update(s) are waiting; apply them first so the modules are current before the core moves",
                        pending.len()
                    ));
                }
                warnings.push(
                    "the database is migrated forward when the new version starts; going back to an older version may need the backup"
                        .into(),
                );
            }
            out.releases.push(ReleaseView {
                installable: newer && blockers.is_empty(),
                newer,
                release: r,
                blockers,
                warnings,
                modules,
            });
        }
        let newest = out.releases.first().map(|r| r.release.version.clone());
        out.update_available = out.releases.first().is_some_and(|r| r.newer);
        out.latest = newest;
        let target = out
            .releases
            .iter()
            .find(|r| r.newer)
            .map(|r| r.release.clone());
        out.instructions = self.instructions(target.as_ref());
    }

    /// The exact commands for updating by hand.
    pub fn instructions(&self, target: Option<&Release>) -> Vec<String> {
        let image = target
            .map(|r| r.image.clone())
            .filter(|i| !i.is_empty())
            .or_else(|| target.map(|r| format!("ghcr.io/spacecorps/obop:{}", r.version)))
            .unwrap_or_else(|| "ghcr.io/spacecorps/obop:<version>".into());
        vec![
            "1. Back up the database: obop backup --db \"$OBOP_DATABASE_URL\" --out ./backups".into(),
            format!("2. Fetch the new image: docker pull {image}"),
            format!("3. Restart the container (or process) on {image}, with the same environment and OBOP_AUTO_MIGRATE=1 (or run step 4 first)"),
            "4. Apply the schema steps: obop migrate plan, then obop migrate run".into(),
            "5. Check: obop update check (the running version should be the new one), and sign in".into(),
            "To go back: restart on the previous image; if it refuses the database, restore step 1 with obop restore.".into(),
        ]
    }

    // ---- the request -------------------------------------------------------------------

    fn state_fields(&self) -> Fields {
        self.backend
            .get(TABLE, "state")
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// Persist `f` as the request state, replacing the previous one.
    fn save_state(&self, f: &Fields) -> Result<(), String> {
        self.backend
            .apply_checked(&[Op::Put {
                model: TABLE.into(),
                id: "state".into(),
                fields: f.clone(),
            }])
            .map_err(|e| e.to_string())
    }

    fn audit(&self, by: &str, kind: &str, detail: &str) {
        self.installer.audit(by, kind, "instance", detail);
    }

    fn rate_limit(&self, what: &str, per_hour: u64) -> Result<(), String> {
        let n = self
            .backend
            .counter_add(&format!("update.{what}"), 3_600_000, false)
            .map_err(|e| e.to_string())?;
        if n > per_hour {
            return Err(format!(
                "too many update requests this hour ({per_hour} at most); wait before trying again"
            ));
        }
        Ok(())
    }

    /// Ask for the update to `version`: checks, backup, then the request to the control plane.
    pub fn start(&self, version: &str, by: &str, skip_backup: bool) -> Result<Status, String> {
        let _lock = self
            .backend
            .lock(LOCK)
            .map_err(|e| format!("another update request is being made ({e})"))?;
        let (url, token) = self.control()?;
        let st = self.status_inner();
        if st.busy() {
            return Err(format!(
                "an update to {} is already {}; wait for it to finish",
                st.target, st.state
            ));
        }
        self.rate_limit("start", 6)?;
        let check = self.check();
        let Some(view) = check.releases.iter().find(|r| r.release.version == version) else {
            return Err(match &check.feed_error {
                Some(e) => format!("cannot confirm that {version} is published: {e}"),
                None => format!("{version} is not a published release"),
            });
        };
        if !view.newer {
            return Err(format!(
                "{version} is not newer than the running version {}; use rollback to go back",
                self.core
            ));
        }
        if !view.blockers.is_empty() {
            return Err(format!(
                "{version} cannot be installed here yet: {}",
                view.blockers.join("; ")
            ));
        }
        // The backup comes first: no backup, no request (unless the administrator says they
        // have their own).
        let backup = self.take_backup(skip_backup)?;
        let mut f = Fields::new();
        for (k, v) in [
            ("state", "requested"),
            ("kind", "update"),
            ("target", version),
            ("from", &self.core.to_string()),
            ("phase", "Asking the control plane"),
            ("error", ""),
            ("backup", backup.as_str()),
            ("started_by", by),
        ] {
            f.insert(k.into(), J::from(v));
        }
        f.insert("started_at".into(), J::from(now()));
        self.save_state(&f)?;
        self.audit(
            by,
            "instance_update_requested",
            &format!(
                "{} -> {version}; backup {}",
                self.core,
                if backup.is_empty() {
                    "none (confirmed)"
                } else {
                    &backup
                }
            ),
        );
        let body = json!({"version": version, "from_version": self.core.to_string()});
        match http_post(&format!("{url}/api/platform/update/start"), &token, &body) {
            Ok(_) => {
                f.insert("state".into(), J::from("running"));
                f.insert(
                    "phase".into(),
                    J::from("The control plane is upgrading this instance"),
                );
                self.save_state(&f)?;
            }
            Err(e) => {
                f.insert("state".into(), J::from("failed"));
                f.insert("error".into(), J::from(e.as_str()));
                f.insert("phase".into(), J::from(""));
                f.insert("finished_at".into(), J::from(now()));
                self.save_state(&f)?;
                self.audit(by, "instance_update_failed", &format!("{version}: {e}"));
                return Err(e);
            }
        }
        Ok(Status::from_fields(
            &self.state_fields(),
            &self.core.to_string(),
        ))
    }

    fn take_backup(&self, skip: bool) -> Result<String, String> {
        let f = self
            .backup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        match (&self.cfg.backup_dir, f) {
            (Some(dir), Some(f)) => f(dir).map_err(|e| {
                format!("the backup failed, so nothing was changed: {e}")
            }),
            _ if skip => Ok(String::new()),
            _ => Err(
                "no backup directory is configured (OBOP_BACKUP_DIR), so no backup can be taken here. Take your own (obop backup, or pg_dump) and confirm with skip_backup"
                    .into(),
            ),
        }
    }

    /// Ask the control plane to put the instance back on the version before the last update.
    pub fn rollback(&self, by: &str) -> Result<Status, String> {
        let _lock = self
            .backend
            .lock(LOCK)
            .map_err(|e| format!("another update request is being made ({e})"))?;
        let (url, token) = self.control()?;
        let st = self.status_inner();
        if st.busy() {
            return Err(format!("an update to {} is still {}", st.target, st.state));
        }
        if !st.can_rollback {
            return Err(
                "there is no update to go back from: the last update did not finish on this version, or it is already rolled back"
                    .into(),
            );
        }
        self.rate_limit("rollback", 6)?;
        let mut f = Fields::new();
        for (k, v) in [
            ("state", "requested"),
            ("kind", "rollback"),
            ("target", st.from.as_str()),
            ("from", st.target.as_str()),
            ("phase", "Asking the control plane"),
            ("error", ""),
            ("backup", st.backup.as_str()),
            ("started_by", by),
        ] {
            f.insert(k.into(), J::from(v));
        }
        f.insert("started_at".into(), J::from(now()));
        self.save_state(&f)?;
        self.audit(
            by,
            "instance_rollback_requested",
            &format!("{} -> {}", st.target, st.from),
        );
        match http_post(
            &format!("{url}/api/platform/update/rollback"),
            &token,
            &json!({}),
        ) {
            Ok(_) => {
                f.insert("state".into(), J::from("running"));
                f.insert(
                    "phase".into(),
                    J::from("The control plane is restoring the previous version"),
                );
                self.save_state(&f)?;
            }
            Err(e) => {
                f.insert("state".into(), J::from("failed"));
                f.insert("error".into(), J::from(e.as_str()));
                f.insert("finished_at".into(), J::from(now()));
                self.save_state(&f)?;
                self.audit(by, "instance_update_failed", &format!("rollback: {e}"));
                return Err(e);
            }
        }
        Ok(Status::from_fields(
            &self.state_fields(),
            &self.core.to_string(),
        ))
    }

    fn control(&self) -> Result<(String, String), String> {
        match (self.control_url(), self.token()) {
            (Some(u), Some(t)) => Ok((u, t)),
            _ => Err(
                "this instance is not managed by a control plane, so it cannot update itself. Follow the steps shown on this page (docker pull, obop migrate), or run `obop update check`"
                    .into(),
            ),
        }
    }

    // ---- following it ------------------------------------------------------------------

    /// The state of the last request, refreshed from the control plane while one is running.
    /// Safe to call from several processes: a terminal change is recorded once.
    pub fn status(&self) -> Status {
        self.status_inner()
    }

    fn status_inner(&self) -> Status {
        let cur = self.core.to_string();
        let mut f = self.state_fields();
        let st = Status::from_fields(&f, &cur);
        if !st.busy() {
            return st;
        }
        let mut next = f.clone();
        let mut terminal = false;
        // The running process is the new version: the redeploy finished.
        if st.state == "running" && st.target == cur && st.kind == "update" {
            next.insert("state".into(), J::from("succeeded"));
            next.insert("phase".into(), J::from(""));
            terminal = true;
        } else if let (Some(url), Some(token)) = (self.control_url(), self.token()) {
            match http_get(&format!("{url}/api/platform/update/status"), Some(&token))
                .and_then(|t| serde_json::from_str::<J>(&t).map_err(|e| e.to_string()))
            {
                Ok(r) => {
                    let s = |k: &str| r.get(k).and_then(J::as_str).unwrap_or("").to_string();
                    match s("state").as_str() {
                        "succeeded" => {
                            next.insert("state".into(), J::from("succeeded"));
                            next.insert("phase".into(), J::from(""));
                            terminal = true;
                        }
                        "failed" | "rolled_back" => {
                            next.insert("state".into(), J::from(s("state")));
                            next.insert("error".into(), J::from(s("error")));
                            next.insert("phase".into(), J::from(""));
                            terminal = true;
                        }
                        "running" => {
                            next.insert("state".into(), J::from("running"));
                            next.insert("phase".into(), J::from(s("phase")));
                        }
                        _ => {
                            if now().saturating_sub(st.started_at) > LOST_AFTER_SECS {
                                next.insert("state".into(), J::from("failed"));
                                next.insert(
                                    "error".into(),
                                    J::from("the control plane has no record of this request"),
                                );
                                terminal = true;
                            }
                        }
                    }
                }
                Err(e) => {
                    // The instance restarts during its own update: silence is expected.
                    next.insert(
                        "phase".into(),
                        J::from(format!("Waiting for the control plane ({e})")),
                    );
                    if now().saturating_sub(st.started_at) > LOST_AFTER_SECS {
                        next.insert("state".into(), J::from("failed"));
                        next.insert("error".into(), J::from(format!("no answer: {e}")));
                        terminal = true;
                    }
                }
            }
        }
        if terminal {
            next.insert("finished_at".into(), J::from(now()));
        }
        if next != f {
            // Record it once, whichever process sees it first.
            let won = self
                .backend
                .apply_checked(&[Op::Patch {
                    model: TABLE.into(),
                    id: "state".into(),
                    patch: next.clone(),
                    expect: vec![("state".into(), f.get("state").cloned().unwrap_or(J::Null))],
                }])
                .is_ok();
            if won && terminal {
                let state = next.get("state").and_then(J::as_str).unwrap_or("");
                let kind = if state == "succeeded" {
                    "instance_update_finished"
                } else {
                    "instance_update_failed"
                };
                self.audit(
                    "control-plane",
                    kind,
                    &format!(
                        "{} {} -> {}: {state}{}",
                        st.kind,
                        st.from,
                        st.target,
                        next.get("error")
                            .and_then(J::as_str)
                            .filter(|e| !e.is_empty())
                            .map_or(String::new(), |e| format!(" ({e})"))
                    ),
                );
            }
            f = next;
        }
        Status::from_fields(&f, &cur)
    }
}
