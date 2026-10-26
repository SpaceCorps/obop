//! The control plane: sign up, then get your own obop instance. Accounts reuse `obop-auth`;
//! instances are records, and a `Provisioner` makes them real (a local process, a Sliplane
//! service with its own database, or a fake for tests).

pub mod provision;
pub mod sliplane;
pub mod update;

use obop_auth::Auth;
use obop_commands::Authenticator;
use obop_storage::{Backend, Fields};
use provision::{InstanceSpec, Provisioner};
use serde_json::{Value as J, json};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const TABLE: &str = "platform_instance";
pub const MAX_INSTANCES_PER_ACCOUNT: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instance {
    pub id: String,
    pub owner: String,
    pub name: String,
    pub slug: String,
    /// `provisioning`, `running`, `failed`, `deleting` or `deleted`.
    pub status: String,
    pub url: String,
    pub error: String,
    /// What the background job is doing right now, e.g. "Building the service (1m 20s)".
    pub phase: String,
    /// The image the instance runs, when its provisioner runs images (empty otherwise).
    pub image: String,
}

impl Instance {
    pub fn to_json(&self) -> J {
        json!({"id": self.id, "name": self.name, "slug": self.slug, "status": self.status, "url": self.url, "error": self.error, "phase": self.phase})
    }

    fn from_fields(id: &str, f: &Fields) -> Instance {
        let g = |k: &str| f.get(k).and_then(J::as_str).unwrap_or("").to_string();
        Instance {
            id: id.into(),
            owner: g("owner"),
            name: g("name"),
            slug: g("slug"),
            status: g("status"),
            url: g("url"),
            error: g("error"),
            phase: g("phase"),
            image: g("image"),
        }
    }
}

pub struct Platform {
    auth: Arc<Auth>,
    backend: Arc<dyn Backend>,
    provisioner: Arc<dyn Provisioner>,
    /// The address new instances are told to reach this control plane at (D63).
    public_url: std::sync::Mutex<String>,
}

/// `Some(slug)` when `name` can be a subdomain: 3 to 30 characters, lowercase letters, digits
/// and single hyphens inside.
pub fn slugify(name: &str) -> Option<String> {
    let s: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let s = s
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    (3..=30).contains(&s.len()).then_some(s)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Platform {
    pub fn new(
        auth: Arc<Auth>,
        backend: Arc<dyn Backend>,
        provisioner: Arc<dyn Provisioner>,
    ) -> Arc<Self> {
        Arc::new(Self {
            auth,
            backend,
            provisioner,
            public_url: std::sync::Mutex::default(),
        })
    }

    pub fn authenticator(&self) -> Arc<dyn Authenticator> {
        self.auth.clone()
    }

    fn owner(&self, token: &str) -> Result<String, String> {
        self.auth
            .resolve(token)
            .map(|p| p.id)
            .ok_or_else(|| "not signed in".to_string())
    }

    /// Create an account and sign it in.
    pub fn signup(&self, email: &str, password: &str) -> Result<String, String> {
        if !email.contains('@') || email.len() > 120 {
            return Err("enter a valid email address".into());
        }
        self.auth.create_user(email, password, &[])?;
        self.auth.login(email, password)
    }

    pub fn login(&self, email: &str, password: &str) -> Result<String, String> {
        self.auth.login(email, password)
    }

    pub fn me(&self, token: &str) -> Result<String, String> {
        self.owner(token)
    }

    pub fn list(&self, token: &str) -> Result<Vec<Instance>, String> {
        let owner = self.owner(token)?;
        Ok(self
            .backend
            .list(TABLE)?
            .iter()
            .map(|(id, f)| Instance::from_fields(id, f))
            .filter(|i| i.owner == owner && i.status != "deleted")
            .collect())
    }

    /// Start provisioning an instance. Returns at once with status `provisioning`; the work runs
    /// in the background and the record turns `running` or `failed`.
    pub fn create_instance(self: &Arc<Self>, token: &str, name: &str) -> Result<Instance, String> {
        let owner = self.owner(token)?;
        let slug = slugify(name).ok_or("name must be 3 to 30 letters, digits or hyphens")?;
        // Several control-plane processes may share the database: the name check and the insert
        // are one step, or two of them could both take the same name (B1).
        let _names = self.backend.lock("platform.instances")?;
        let all: Vec<Instance> = self
            .backend
            .list(TABLE)?
            .iter()
            .map(|(id, f)| Instance::from_fields(id, f))
            .collect();
        if all.iter().any(|i| i.slug == slug && i.status != "deleted") {
            return Err(format!("the name {slug} is taken"));
        }
        if all
            .iter()
            .filter(|i| i.owner == owner && i.status != "deleted")
            .count()
            >= MAX_INSTANCES_PER_ACCOUNT
        {
            return Err(format!(
                "an account can have {MAX_INSTANCES_PER_ACCOUNT} instances"
            ));
        }
        let hash = self.auth.password_hash(&owner).ok_or("account not found")?;
        let id = uuid::Uuid::now_v7().to_string();
        let mut f = Fields::new();
        for (k, v) in [
            ("owner", owner.as_str()),
            ("name", name.trim()),
            ("slug", &slug),
            ("status", "provisioning"),
            ("url", ""),
            ("error", ""),
            ("external_id", ""),
        ] {
            f.insert(k.into(), J::from(v));
        }
        f.insert(
            "image".into(),
            J::from(self.provisioner.image().unwrap_or_default()),
        );
        f.insert("created_at".into(), J::from(now()));
        self.backend.insert(TABLE, &id, &f)?;
        // The instance's own update token (D63): handed to the provisioner once, kept here as
        // a hash.
        let update_token = update::new_token();
        self.store_update_token(&id, &update_token)?;
        let me = self.clone();
        let spec = InstanceSpec {
            slug: slug.clone(),
            owner_email: owner,
            password_hash: hash,
            update_token,
            control_url: self.public_url(),
        };
        let rec = id.clone();
        std::thread::spawn(move || {
            let report = |ph: &str| me.set_phase(&rec, ph);
            let mut patch = Fields::new();
            match me.provisioner.create(&spec, &report) {
                Ok(p) => {
                    patch.insert("status".into(), J::from("running"));
                    patch.insert("phase".into(), J::from(""));
                    patch.insert("url".into(), J::from(p.url));
                    patch.insert("external_id".into(), J::from(p.external_id));
                }
                Err(e) => {
                    patch.insert("status".into(), J::from("failed"));
                    patch.insert("phase".into(), J::from(""));
                    patch.insert("error".into(), J::from(e));
                }
            }
            me.finish_record(&rec, &patch);
        });
        Ok(Instance::from_fields(&id, &f))
    }

    /// Move every running instance to `image`, one at a time, oldest first, each only after the
    /// previous one answers its health check; the first failure stops the run and leaves the
    /// rest untouched. With `only`, just that instance (for a try-out, or a rollback to an
    /// older image). With `dry_run`, say what would happen and change nothing.
    pub fn upgrade_instances(
        &self,
        image: &str,
        only: Option<&str>,
        dry_run: bool,
        progress: provision::Progress,
    ) -> Result<UpgradeReport, String> {
        if image.is_empty() || image.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(
                "the image must be a reference such as ghcr.io/spacecorps/obop:<commit>".into(),
            );
        }
        let mut all: Vec<(String, u64, Fields)> = self
            .backend
            .list(TABLE)?
            .into_iter()
            .filter(|(_, f)| f.get("status").and_then(J::as_str) != Some("deleted"))
            .map(|(id, f)| {
                let at = f.get("created_at").and_then(J::as_u64).unwrap_or(0);
                (id, at, f)
            })
            .collect();
        all.sort_by_key(|(id, at, _)| (*at, id.clone()));
        if let Some(slug) = only
            && !all
                .iter()
                .any(|(_, _, f)| f.get("slug").and_then(J::as_str) == Some(slug))
        {
            return Err(format!("no instance {slug}"));
        }
        let mut report = UpgradeReport::default();
        for (id, _, f) in all {
            let i = Instance::from_fields(&id, &f);
            if only.is_some_and(|s| s != i.slug) {
                continue;
            }
            let ext = f
                .get("external_id")
                .and_then(J::as_str)
                .unwrap_or("")
                .to_string();
            let step = |outcome| UpgradeStep {
                slug: i.slug.clone(),
                from: i.image.clone(),
                to: image.to_string(),
                outcome,
            };
            if report.stopped {
                report.steps.push(step(UpgradeOutcome::NotAttempted));
            } else if i.status != "running" || ext.is_empty() {
                report
                    .steps
                    .push(step(UpgradeOutcome::Skipped(format!("it is {}", i.status))));
            } else if i.image == image {
                report.steps.push(step(UpgradeOutcome::Skipped(
                    "already on this image".into(),
                )));
            } else if dry_run {
                report.steps.push(step(UpgradeOutcome::WouldUpgrade));
            } else {
                progress(&format!("{}: upgrading to {image}", i.slug));
                let phase = |ph: &str| {
                    self.set_phase(&id, ph);
                    progress(&format!("{}: {ph}", i.slug));
                };
                let result = self.provisioner.upgrade(&i.slug, &ext, image, &phase);
                let mut patch = Fields::new();
                patch.insert("phase".into(), J::from(""));
                match result {
                    Ok(()) => {
                        patch.insert("image".into(), J::from(image));
                        report.steps.push(step(UpgradeOutcome::Upgraded));
                    }
                    Err(e) => {
                        patch.insert(
                            "error".into(),
                            J::from(format!("upgrade to {image} failed: {e}")),
                        );
                        report.steps.push(step(UpgradeOutcome::Failed(e)));
                        report.stopped = true;
                    }
                }
                self.finish_record(&id, &patch);
            }
        }
        Ok(report)
    }

    /// The last write of a provisioning, deletion or upgrade that ran on a thread of its own:
    /// nobody is waiting for an answer, so a refusal by a busy store (D74) is waited out, up to
    /// about a minute, rather than leaving the instance `provisioning` or `deleting` for good.
    pub(crate) fn finish_record(&self, id: &str, patch: &Fields) {
        let r = obop_storage::fault::patiently(60, std::time::Duration::from_secs(1), || {
            self.backend.update(TABLE, id, patch)
        });
        if let Err(e) = r {
            eprintln!("platform: the record of instance {id} could not be updated: {e}");
        }
    }

    fn set_phase(&self, id: &str, phase: &str) {
        let mut p = Fields::new();
        p.insert("phase".into(), J::from(phase));
        let _ = self.backend.update(TABLE, id, &p);
    }

    pub fn get(&self, token: &str, id: &str) -> Result<Instance, String> {
        let owner = self.owner(token)?;
        let f = self.backend.get(TABLE, id)?.ok_or("no such instance")?;
        let i = Instance::from_fields(id, &f);
        // Someone else's instance looks the same as one that does not exist.
        if i.owner != owner || i.status == "deleted" {
            Err("no such instance".into())
        } else {
            Ok(i)
        }
    }

    /// Start tearing an instance down. Returns at once with status `deleting`; the provisioner
    /// (a Sliplane service and a database, which takes a while) runs in the background and the
    /// record turns `deleted`, or `failed` with the reason so the delete can be retried.
    pub fn delete_instance(self: &Arc<Self>, token: &str, id: &str) -> Result<Instance, String> {
        let mut i = self.get(token, id)?;
        match i.status.as_str() {
            "deleting" => return Ok(i),
            "provisioning" => return Err("wait until provisioning finishes".into()),
            _ => {}
        }
        let f = self.backend.get(TABLE, id)?.ok_or("no such instance")?;
        let ext = f
            .get("external_id")
            .and_then(J::as_str)
            .unwrap_or("")
            .to_string();
        let mut p = Fields::new();
        p.insert("status".into(), J::from("deleting"));
        p.insert("error".into(), J::from(""));
        p.insert("phase".into(), J::from("Queued"));
        self.backend.update(TABLE, id, &p)?;
        i.status = "deleting".into();
        let (me, slug, rec) = (self.clone(), i.slug.clone(), id.to_string());
        std::thread::spawn(move || {
            let report = |ph: &str| me.set_phase(&rec, ph);
            let mut patch = Fields::new();
            patch.insert("phase".into(), J::from(""));
            match me.provisioner.delete(&slug, &ext, &report) {
                Ok(()) => {
                    patch.insert("status".into(), J::from("deleted"));
                }
                Err(e) => {
                    patch.insert("status".into(), J::from("failed"));
                    patch.insert("error".into(), J::from(format!("delete failed: {e}")));
                }
            }
            me.finish_record(&rec, &patch);
        });
        Ok(i)
    }
}

/// What happened to one instance in `upgrade_instances`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpgradeOutcome {
    Upgraded,
    WouldUpgrade,
    Skipped(String),
    Failed(String),
    /// An earlier instance failed, so this one was left alone.
    NotAttempted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpgradeStep {
    pub slug: String,
    pub from: String,
    pub to: String,
    pub outcome: UpgradeOutcome,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpgradeReport {
    pub steps: Vec<UpgradeStep>,
    /// A step failed; the ones after it were not attempted.
    pub stopped: bool,
}

/// The image reference for an upgrade: `arg` as it is when it names a registry path, else the
/// repository of `current` (the image new instances use) with `arg` as the tag.
pub fn resolve_image(arg: &str, current: Option<&str>) -> String {
    if arg.contains('/') {
        return arg.to_string();
    }
    let repo = current
        .map(|c| {
            // Keep a registry port (`host:5000/x`), drop only a trailing tag.
            match c.rsplit_once(':') {
                Some((r, tag)) if !tag.contains('/') => r.to_string(),
                _ => c.to_string(),
            }
        })
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| "ghcr.io/spacecorps/obop".to_string());
    format!("{repo}:{}", arg.trim_start_matches(':'))
}

/// Adapter that lets the HTTP layer drive the platform.
pub struct Api(pub Arc<Platform>);

impl obop_http::PlatformApi for Api {
    fn signup(&self, email: &str, password: &str) -> Result<String, String> {
        Platform::signup(&self.0, email, password)
    }
    fn login(&self, email: &str, password: &str) -> Result<String, String> {
        Platform::login(&self.0, email, password)
    }
    fn login_step(
        &self,
        email: &str,
        password: &str,
    ) -> Result<obop_commands::LoginOutcome, String> {
        self.0.auth.login_step(email, password)
    }
    fn authenticator(&self) -> Option<Arc<dyn Authenticator>> {
        Some(self.0.authenticator())
    }
    fn me(&self, token: &str) -> Result<String, String> {
        Platform::me(&self.0, token)
    }
    fn list(&self, token: &str) -> Result<Vec<J>, String> {
        Platform::list(&self.0, token).map(|v| v.iter().map(Instance::to_json).collect())
    }
    fn create(&self, token: &str, name: &str) -> Result<J, String> {
        Platform::create_instance(&self.0, token, name).map(|i| i.to_json())
    }
    fn delete(&self, token: &str, id: &str) -> Result<J, String> {
        Platform::delete_instance(&self.0, token, id).map(|i| i.to_json())
    }
    fn releases(&self) -> Result<J, String> {
        Ok(self.0.feed_json())
    }
    fn update_status(&self, t: &str) -> Result<J, String> {
        Platform::update_status(&self.0, t)
    }
    fn update_start(&self, t: &str, version: &str, from: &str) -> Result<J, String> {
        Platform::update_start(&self.0, t, version, from)
    }
    fn update_rollback(&self, t: &str) -> Result<J, String> {
        Platform::update_rollback(&self.0, t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use provision::Fake;

    fn platform() -> (Arc<Platform>, Arc<Fake>) {
        let backend = obop_storage::open("memory:").unwrap();
        let fake = Arc::new(Fake::default());
        (
            Platform::new(
                Arc::new(Auth::with_test_params(backend.clone())),
                backend,
                fake.clone(),
            ),
            fake,
        )
    }

    fn wait_for(p: &Platform, tok: &str, id: &str, status: &str) -> Instance {
        for _ in 0..200 {
            let i = p.get(tok, id).unwrap();
            if i.status == status {
                return i;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("never reached {status}");
    }

    #[test]
    fn slugs() {
        assert_eq!(
            slugify("  Acme Trading Ltd. ").as_deref(),
            Some("acme-trading-ltd")
        );
        assert_eq!(slugify("ab"), None);
        assert_eq!(slugify(&"x".repeat(31)), None);
        assert_eq!(slugify("a--b").as_deref(), Some("a-b"));
    }

    #[test]
    fn signup_then_instance_becomes_running_with_the_same_credentials() {
        let (p, fake) = platform();
        let tok = p
            .signup("ada@example.test", "correct horse battery")
            .unwrap();
        let i = p.create_instance(&tok, "Acme Trading").unwrap();
        assert_eq!(
            (i.status.as_str(), i.slug.as_str()),
            ("provisioning", "acme-trading")
        );
        let i = wait_for(&p, &tok, &i.id, "running");
        assert_eq!(i.url, "https://acme-trading.fake.test");
        let specs = fake.created.lock().unwrap();
        assert_eq!(specs[0].owner_email, "ada@example.test");
        assert!(
            specs[0].password_hash.starts_with("$argon2id$"),
            "the hash travels, never the password"
        );
    }

    #[test]
    fn delete_does_not_wait_for_the_provisioner() {
        let (p, fake) = platform();
        let t = p.signup("a@example.test", "correct horse battery").unwrap();
        let i = p.create_instance(&t, "alpha-one").unwrap();
        let i = wait_for(&p, &t, &i.id, "running");
        *fake.delete_delay.lock().unwrap() = std::time::Duration::from_millis(400);
        let started = std::time::Instant::now();
        let d = p.delete_instance(&t, &i.id).unwrap();
        assert!(
            started.elapsed() < std::time::Duration::from_millis(200),
            "returned before teardown finished"
        );
        assert_eq!(d.status, "deleting");
        // A second delete while it runs is harmless, and the name stays taken until it is done.
        assert_eq!(p.delete_instance(&t, &i.id).unwrap().status, "deleting");
        assert!(
            p.create_instance(&t, "alpha-one")
                .unwrap_err()
                .contains("taken")
        );
        wait_for_gone(&p, &t, &i.id);
        p.create_instance(&t, "alpha-one").unwrap();
    }

    #[test]
    fn a_busy_store_does_not_leave_an_instance_deleting_for_ever() {
        use obop_storage::testing::{Flaky, Mode};
        // D74: the provisioner finishes while the store refuses every call. Its last write is
        // waited out and lands when the store answers again; it is not dropped.
        let (flaky, backend) = Flaky::memory();
        let fake = Arc::new(Fake::default());
        let p = Platform::new(
            Arc::new(Auth::with_test_params(backend.clone())),
            backend,
            fake.clone(),
        );
        let t = p.signup("a@example.test", "correct horse battery").unwrap();
        let i = p.create_instance(&t, "alpha-one").unwrap();
        let i = wait_for(&p, &t, &i.id, "running");
        *fake.delete_delay.lock().unwrap() = std::time::Duration::from_millis(300);
        assert_eq!(p.delete_instance(&t, &i.id).unwrap().status, "deleting");
        flaky.set(Mode::All);
        // The provisioner is done after 300 ms; its final write is refused for a while.
        std::thread::sleep(std::time::Duration::from_millis(1500));
        assert!(flaky.refused() >= 1, "the final write met the busy store");
        flaky.set(Mode::Healthy);
        wait_for_gone(&p, &t, &i.id);
    }

    fn wait_for_gone(p: &Platform, tok: &str, id: &str) {
        for _ in 0..300 {
            if p.get(tok, id).is_err() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("instance never disappeared");
    }

    #[test]
    fn a_failed_delete_is_reported_and_can_be_retried() {
        let (p, fake) = platform();
        let t = p.signup("a@example.test", "correct horse battery").unwrap();
        let i = p.create_instance(&t, "alpha-one").unwrap();
        wait_for(&p, &t, &i.id, "running");
        *fake.delete_fail.lock().unwrap() = Some("sliplane unreachable".into());
        p.delete_instance(&t, &i.id).unwrap();
        let failed = wait_for(&p, &t, &i.id, "failed");
        assert!(failed.error.contains("sliplane unreachable"));
        *fake.delete_fail.lock().unwrap() = None;
        p.delete_instance(&t, &i.id).unwrap();
        wait_for_gone(&p, &t, &i.id);
    }

    #[test]
    fn names_are_unique_and_accounts_are_capped() {
        let (p, _) = platform();
        let a = p.signup("a@example.test", "correct horse battery").unwrap();
        let b = p.signup("b@example.test", "another long password").unwrap();
        p.create_instance(&a, "alpha-one").unwrap();
        assert!(
            p.create_instance(&b, "alpha-one")
                .unwrap_err()
                .contains("taken")
        );
        assert!(p.create_instance(&a, "x").is_err());
        p.create_instance(&a, "alpha-two").unwrap();
        p.create_instance(&a, "alpha-three").unwrap();
        assert!(
            p.create_instance(&a, "alpha-four")
                .unwrap_err()
                .contains("instances")
        );
    }

    #[test]
    fn accounts_only_see_their_own_instances() {
        let (p, _) = platform();
        let a = p.signup("a@example.test", "correct horse battery").unwrap();
        let b = p.signup("b@example.test", "another long password").unwrap();
        let i = p.create_instance(&a, "alpha-one").unwrap();
        assert!(p.list(&b).unwrap().is_empty());
        assert_eq!(p.get(&b, &i.id).unwrap_err(), "no such instance");
        assert_eq!(
            p.delete_instance(&b, &i.id).unwrap_err(),
            "no such instance"
        );
        assert!(p.list("obop_forged").is_err());
    }

    #[test]
    fn failures_are_recorded_and_delete_frees_the_name() {
        let (p, fake) = platform();
        let t = p.signup("a@example.test", "correct horse battery").unwrap();
        *fake.fail_with.lock().unwrap() = Some("no capacity".into());
        let i = p.create_instance(&t, "alpha-one").unwrap();
        let i = wait_for(&p, &t, &i.id, "failed");
        assert_eq!(i.error, "no capacity");
        let d = p.delete_instance(&t, &i.id).unwrap();
        assert_eq!(d.status, "deleting", "delete returns at once");
        for _ in 0..200 {
            if p.list(&t).unwrap().is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(p.list(&t).unwrap().is_empty());
        assert!(
            fake.deleted
                .lock()
                .unwrap()
                .contains(&"alpha-one".to_string())
        );
        *fake.fail_with.lock().unwrap() = None;
        p.create_instance(&t, "alpha-one").unwrap();
    }

    #[test]
    fn weak_passwords_and_duplicate_emails_are_rejected() {
        let (p, _) = platform();
        assert!(p.signup("nobody", "correct horse battery").is_err());
        assert!(p.signup("a@example.test", "short").is_err());
        p.signup("a@example.test", "correct horse battery").unwrap();
        assert!(
            p.signup("A@example.test", "correct horse battery")
                .unwrap_err()
                .contains("exists")
        );
    }
    fn running_instances(p: &Arc<Platform>, names: &[&str]) -> String {
        let tok = p
            .signup("ops@example.test", "correct horse battery")
            .unwrap();
        for name in names {
            let i = p.create_instance(&tok, name).unwrap();
            wait_for(p, &tok, &i.id, "running");
            // Creation order is what the upgrade follows: keep the timestamps apart.
            std::thread::sleep(std::time::Duration::from_millis(1100));
        }
        tok
    }

    #[test]
    fn upgrades_go_one_by_one_oldest_first_and_record_the_image() {
        let (p, fake) = platform();
        *fake.image.lock().unwrap() = Some("ghcr.io/spacecorps/obop:old".into());
        running_instances(&p, &["alpha-one", "alpha-two", "alpha-three"]);
        let seen = std::sync::Mutex::new(Vec::<String>::new());
        let r = p
            .upgrade_instances("ghcr.io/spacecorps/obop:new", None, false, &|l| {
                seen.lock().unwrap().push(l.into())
            })
            .unwrap();
        assert!(!r.stopped);
        let slugs: Vec<_> = r.steps.iter().map(|s| s.slug.as_str()).collect();
        assert_eq!(slugs, ["alpha-one", "alpha-two", "alpha-three"]);
        assert!(
            r.steps.iter().all(|s| s.outcome == UpgradeOutcome::Upgraded
                && s.from == "ghcr.io/spacecorps/obop:old")
        );
        assert_eq!(
            *fake.upgraded.lock().unwrap(),
            [
                (
                    "alpha-one".to_string(),
                    "ghcr.io/spacecorps/obop:new".to_string()
                ),
                ("alpha-two".into(), "ghcr.io/spacecorps/obop:new".into()),
                ("alpha-three".into(), "ghcr.io/spacecorps/obop:new".into()),
            ]
        );
        assert!(
            seen.lock()
                .unwrap()
                .iter()
                .any(|l| l.contains("alpha-two: upgrading"))
        );
        // A second run has nothing to do: they are on it.
        let again = p
            .upgrade_instances("ghcr.io/spacecorps/obop:new", None, false, &|_| {})
            .unwrap();
        assert!(
            again
                .steps
                .iter()
                .all(|s| matches!(&s.outcome, UpgradeOutcome::Skipped(w) if w.contains("already")))
        );
        assert_eq!(fake.upgraded.lock().unwrap().len(), 3);
    }

    #[test]
    fn the_first_failure_stops_the_run_and_leaves_the_rest_untouched() {
        let (p, fake) = platform();
        *fake.image.lock().unwrap() = Some("ghcr.io/spacecorps/obop:old".into());
        let tok = running_instances(&p, &["alpha-one", "alpha-two", "alpha-three"]);
        *fake.upgrade_fail_for.lock().unwrap() = Some("alpha-two".into());
        let r = p
            .upgrade_instances("ghcr.io/spacecorps/obop:new", None, false, &|_| {})
            .unwrap();
        assert!(r.stopped);
        let outcomes: Vec<_> = r.steps.iter().map(|s| s.outcome.clone()).collect();
        assert_eq!(outcomes[0], UpgradeOutcome::Upgraded);
        assert!(matches!(&outcomes[1], UpgradeOutcome::Failed(e) if e.contains("health check")));
        assert_eq!(outcomes[2], UpgradeOutcome::NotAttempted);
        assert_eq!(
            fake.upgraded.lock().unwrap().len(),
            1,
            "alpha-three was not touched"
        );
        // The record keeps what each instance runs, and says what failed.
        let list = p.list(&tok).unwrap();
        let img = |slug: &str| list.iter().find(|i| i.slug == slug).unwrap().image.clone();
        assert_eq!(img("alpha-one"), "ghcr.io/spacecorps/obop:new");
        assert_eq!(img("alpha-two"), "ghcr.io/spacecorps/obop:old");
        assert_eq!(img("alpha-three"), "ghcr.io/spacecorps/obop:old");
        assert!(
            list.iter()
                .find(|i| i.slug == "alpha-two")
                .unwrap()
                .error
                .contains("upgrade to")
        );
        // After the fault is fixed, the same command carries on where it stopped.
        *fake.upgrade_fail_for.lock().unwrap() = None;
        let r = p
            .upgrade_instances("ghcr.io/spacecorps/obop:new", None, false, &|_| {})
            .unwrap();
        assert!(!r.stopped);
        assert!(matches!(&r.steps[0].outcome, UpgradeOutcome::Skipped(_)));
        assert_eq!(fake.upgraded.lock().unwrap().len(), 3);
    }

    #[test]
    fn a_dry_run_changes_nothing_and_only_picks_one_instance() {
        let (p, fake) = platform();
        *fake.image.lock().unwrap() = Some("ghcr.io/spacecorps/obop:old".into());
        running_instances(&p, &["alpha-one", "alpha-two"]);
        let r = p
            .upgrade_instances("ghcr.io/spacecorps/obop:new", None, true, &|_| {})
            .unwrap();
        assert!(
            r.steps
                .iter()
                .all(|s| s.outcome == UpgradeOutcome::WouldUpgrade)
        );
        assert!(fake.upgraded.lock().unwrap().is_empty());
        let r = p
            .upgrade_instances(
                "ghcr.io/spacecorps/obop:old2",
                Some("alpha-two"),
                false,
                &|_| {},
            )
            .unwrap();
        assert_eq!(r.steps.len(), 1);
        assert_eq!(r.steps[0].slug, "alpha-two");
        assert_eq!(fake.upgraded.lock().unwrap().len(), 1);
        assert!(
            p.upgrade_instances("x:y", Some("nobody"), false, &|_| {})
                .unwrap_err()
                .contains("no instance")
        );
        assert!(
            p.upgrade_instances("bad image", None, false, &|_| {})
                .is_err()
        );
    }

    #[test]
    fn instances_that_are_not_running_are_skipped_and_a_provisioner_without_upgrades_says_so() {
        let (p, fake) = platform();
        let tok = running_instances(&p, &["alpha-one"]);
        let id = p.list(&tok).unwrap()[0].id.clone();
        *fake.delete_delay.lock().unwrap() = std::time::Duration::from_millis(800);
        p.delete_instance(&tok, &id).unwrap();
        let r = p.upgrade_instances("x/y:z", None, false, &|_| {}).unwrap();
        assert!(
            matches!(&r.steps[0].outcome, UpgradeOutcome::Skipped(w) if w.contains("deleting")),
            "{r:?}"
        );
        // The default provisioner cannot upgrade in place.
        struct Bare;
        impl Provisioner for Bare {
            fn create(
                &self,
                _: &InstanceSpec,
                _: provision::Progress,
            ) -> Result<provision::Provisioned, String> {
                Ok(provision::Provisioned {
                    external_id: "1".into(),
                    url: "u".into(),
                })
            }
            fn delete(&self, _: &str, _: &str, _: provision::Progress) -> Result<(), String> {
                Ok(())
            }
        }
        assert!(
            Bare.upgrade("a", "1", "i", &|_| {})
                .unwrap_err()
                .contains("cannot upgrade")
        );
    }

    #[test]
    fn a_short_tag_is_resolved_against_the_repository_of_the_current_image() {
        assert_eq!(
            resolve_image("abc123", Some("ghcr.io/spacecorps/obop:old")),
            "ghcr.io/spacecorps/obop:abc123"
        );
        assert_eq!(
            resolve_image("abc123", Some("ghcr.io/spacecorps/obop")),
            "ghcr.io/spacecorps/obop:abc123"
        );
        assert_eq!(
            resolve_image("abc123", Some("localhost:5000/obop")),
            "localhost:5000/obop:abc123"
        );
        assert_eq!(
            resolve_image("abc123", None),
            "ghcr.io/spacecorps/obop:abc123"
        );
        assert_eq!(
            resolve_image("ghcr.io/other/obop:1", Some("ghcr.io/x/y:2")),
            "ghcr.io/other/obop:1"
        );
    }
}

#[cfg(test)]
mod update_tests;
