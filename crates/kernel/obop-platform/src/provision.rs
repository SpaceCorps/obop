//! Making an instance real. The control plane only knows this trait.

use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct InstanceSpec {
    pub slug: String,
    pub owner_email: String,
    /// Argon2id hash of the owner's password: the instance's first admin gets the same login.
    pub password_hash: String,
    /// The instance's update token (D63): lets it ask this control plane to upgrade it, and
    /// nothing else. Handed over once, as a secret environment variable; the control plane
    /// keeps only its hash.
    pub update_token: String,
    /// Where the instance reaches the control plane (`OBOP_UPDATE_URL`); empty when unknown.
    pub control_url: String,
}

#[derive(Debug, Clone)]
pub struct Provisioned {
    /// Whatever the provisioner needs to find the instance again (a service id, a port).
    pub external_id: String,
    pub url: String,
}

/// Reports what a long-running step is doing right now; shown live to the person waiting.
pub type Progress<'a> = &'a (dyn Fn(&str) + Sync);

pub trait Provisioner: Send + Sync {
    fn create(&self, spec: &InstanceSpec, progress: Progress) -> Result<Provisioned, String>;
    fn delete(&self, slug: &str, external_id: &str, progress: Progress) -> Result<(), String>;
    /// The image new instances start from, when the provisioner runs images (recorded with each
    /// instance, so an upgrade knows what it replaces).
    fn image(&self) -> Option<String> {
        None
    }
    /// Move a running instance to `image` (a full reference such as
    /// `ghcr.io/spacecorps/obop:<commit>`): redeploy it and return only when the new version
    /// answers its health check. An error means the instance may be down or on either version;
    /// the caller stops and tells an operator.
    fn upgrade(
        &self,
        _slug: &str,
        _external_id: &str,
        _image: &str,
        _progress: Progress,
    ) -> Result<(), String> {
        Err("this provisioner cannot upgrade instances in place".into())
    }
}

/// Records what it was asked to do. For tests.
#[derive(Default)]
pub struct Fake {
    pub created: Mutex<Vec<InstanceSpec>>,
    pub deleted: Mutex<Vec<String>>,
    pub fail_with: Mutex<Option<String>>,
    pub delete_delay: Mutex<std::time::Duration>,
    pub delete_fail: Mutex<Option<String>>,
    /// `(slug, image)` of each upgrade, in order.
    pub upgraded: Mutex<Vec<(String, String)>>,
    /// Fail the upgrade of this slug.
    pub upgrade_fail_for: Mutex<Option<String>>,
    /// The image reported for new instances.
    pub image: Mutex<Option<String>>,
    /// How long an upgrade takes (so a page can show it running).
    pub upgrade_delay: Mutex<std::time::Duration>,
    /// Fail the next upgrade to this image (the rollback to the old one still succeeds).
    pub upgrade_fail_image: Mutex<Option<String>>,
}

impl Provisioner for Fake {
    fn create(&self, spec: &InstanceSpec, progress: Progress) -> Result<Provisioned, String> {
        progress("Creating the instance");
        if let Some(e) = self.fail_with.lock().unwrap().clone() {
            return Err(e);
        }
        self.created.lock().unwrap().push(spec.clone());
        Ok(Provisioned {
            external_id: format!("fake-{}", spec.slug),
            url: format!("https://{}.fake.test", spec.slug),
        })
    }

    fn image(&self) -> Option<String> {
        self.image.lock().unwrap().clone()
    }

    fn upgrade(&self, slug: &str, _: &str, image: &str, progress: Progress) -> Result<(), String> {
        progress("Redeploying the instance");
        std::thread::sleep(*self.upgrade_delay.lock().unwrap());
        if self.upgrade_fail_image.lock().unwrap().as_deref() == Some(image) {
            return Err("the new version did not answer its health check".into());
        }
        if self.upgrade_fail_for.lock().unwrap().as_deref() == Some(slug) {
            return Err("the new version did not answer its health check".into());
        }
        self.upgraded
            .lock()
            .unwrap()
            .push((slug.into(), image.into()));
        Ok(())
    }

    fn delete(&self, slug: &str, _: &str, progress: Progress) -> Result<(), String> {
        progress("Removing the instance");
        std::thread::sleep(*self.delete_delay.lock().unwrap());
        if let Some(e) = self.delete_fail.lock().unwrap().clone() {
            return Err(e);
        }
        self.deleted.lock().unwrap().push(slug.into());
        Ok(())
    }
}

/// Runs each instance as a child `obop serve` process on this machine, with its own SQLite
/// database. For local development of the platform itself and for trying it with no cloud.
pub struct Local {
    pub exe: std::path::PathBuf,
    pub data_dir: std::path::PathBuf,
    pub static_dir: Option<String>,
    children: Mutex<std::collections::HashMap<String, std::process::Child>>,
}

impl Local {
    pub fn new(
        exe: std::path::PathBuf,
        data_dir: std::path::PathBuf,
        static_dir: Option<String>,
    ) -> Self {
        Self {
            exe,
            data_dir,
            static_dir,
            children: Mutex::default(),
        }
    }

    fn db_path(&self, slug: &str) -> std::path::PathBuf {
        self.data_dir.join(format!("{slug}.db"))
    }
}

impl Provisioner for Local {
    fn create(&self, spec: &InstanceSpec, progress: Progress) -> Result<Provisioned, String> {
        progress("Preparing the database");
        std::fs::create_dir_all(&self.data_dir).map_err(|e| e.to_string())?;
        let db = format!("sqlite://{}", self.db_path(&spec.slug).display());
        // Seed the first admin before the server starts, with the owner's own password hash.
        let backend = obop_storage::open(&db)?;
        obop_auth::Auth::new(backend).create_user_hashed(
            &spec.owner_email,
            &spec.password_hash,
            &[obop_commands::ADMIN_GROUP.to_string()],
        )?;
        progress("Starting the server");
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .map_err(|e| e.to_string())?
            .port();
        let addr = format!("127.0.0.1:{port}");
        let mut cmd = std::process::Command::new(&self.exe);
        cmd.args(["serve", "--addr", &addr, "--db", &db]);
        if !spec.update_token.is_empty() && !spec.control_url.is_empty() {
            cmd.env("OBOP_UPDATE_TOKEN", &spec.update_token)
                .env("OBOP_UPDATE_URL", &spec.control_url);
        }
        // The control plane's own backup schedule, copy target and host name are its own: a tenant
        // that inherited them would run a second scheduler into the same directory (D73).
        for own in [
            "OBOP_BACKUP_SCHEDULE",
            "OBOP_BACKUP_COPY_DIR",
            "OBOP_HOST_ID",
        ] {
            cmd.env_remove(own);
        }
        if let Some(s) = &self.static_dir {
            cmd.args(["--static", s]);
        }
        let child = cmd
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("cannot start {}: {e}", self.exe.display()))?;
        self.children
            .lock()
            .unwrap()
            .insert(spec.slug.clone(), child);
        for _ in 0..100 {
            if std::net::TcpStream::connect(&addr).is_ok() {
                return Ok(Provisioned {
                    external_id: port.to_string(),
                    url: format!("http://{addr}"),
                });
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        Err("the instance did not start listening".into())
    }

    fn delete(&self, slug: &str, _: &str, progress: Progress) -> Result<(), String> {
        progress("Stopping the server and removing its data");
        if let Some(mut c) = self.children.lock().unwrap().remove(slug) {
            let _ = c.kill();
            let _ = c.wait();
        }
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", self.db_path(slug).display()));
        }
        Ok(())
    }
}
