//! The control plane: sign up, then get your own obop instance. Accounts reuse `obop-auth`;
//! instances are records, and a `Provisioner` makes them real (a local process, a Sliplane
//! service with its own database, or a fake for tests).

pub mod provision;
pub mod sliplane;

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
        }
    }
}

pub struct Platform {
    auth: Arc<Auth>,
    backend: Arc<dyn Backend>,
    provisioner: Arc<dyn Provisioner>,
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
        f.insert("created_at".into(), J::from(now()));
        self.backend.insert(TABLE, &id, &f)?;
        let me = self.clone();
        let spec = InstanceSpec {
            slug: slug.clone(),
            owner_email: owner,
            password_hash: hash,
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
            let _ = me.backend.update(TABLE, &rec, &patch);
        });
        Ok(Instance::from_fields(&id, &f))
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
            let _ = me.backend.update(TABLE, &rec, &patch);
        });
        Ok(i)
    }
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
}
