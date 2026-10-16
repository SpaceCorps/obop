//! Making an instance real. The control plane only knows this trait.

use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct InstanceSpec {
    pub slug: String,
    pub owner_email: String,
    /// Argon2id hash of the owner's password: the instance's first admin gets the same login.
    pub password_hash: String,
}

#[derive(Debug, Clone)]
pub struct Provisioned {
    /// Whatever the provisioner needs to find the instance again (a service id, a port).
    pub external_id: String,
    pub url: String,
}

pub trait Provisioner: Send + Sync {
    fn create(&self, spec: &InstanceSpec) -> Result<Provisioned, String>;
    fn delete(&self, slug: &str, external_id: &str) -> Result<(), String>;
}

/// Records what it was asked to do. For tests.
#[derive(Default)]
pub struct Fake {
    pub created: Mutex<Vec<InstanceSpec>>,
    pub deleted: Mutex<Vec<String>>,
    pub fail_with: Mutex<Option<String>>,
}

impl Provisioner for Fake {
    fn create(&self, spec: &InstanceSpec) -> Result<Provisioned, String> {
        if let Some(e) = self.fail_with.lock().unwrap().clone() {
            return Err(e);
        }
        self.created.lock().unwrap().push(spec.clone());
        Ok(Provisioned {
            external_id: format!("fake-{}", spec.slug),
            url: format!("https://{}.fake.test", spec.slug),
        })
    }

    fn delete(&self, slug: &str, _: &str) -> Result<(), String> {
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
    fn create(&self, spec: &InstanceSpec) -> Result<Provisioned, String> {
        std::fs::create_dir_all(&self.data_dir).map_err(|e| e.to_string())?;
        let db = format!("sqlite://{}", self.db_path(&spec.slug).display());
        // Seed the first admin before the server starts, with the owner's own password hash.
        let backend = obop_storage::open(&db)?;
        obop_auth::Auth::new(backend).create_user_hashed(
            &spec.owner_email,
            &spec.password_hash,
            &[obop_commands::ADMIN_GROUP.to_string()],
        )?;
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .map_err(|e| e.to_string())?
            .port();
        let addr = format!("127.0.0.1:{port}");
        let mut cmd = std::process::Command::new(&self.exe);
        cmd.args(["serve", "--addr", &addr, "--db", &db]);
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

    fn delete(&self, slug: &str, _: &str) -> Result<(), String> {
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
