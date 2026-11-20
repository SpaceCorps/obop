//! Provisioning on Sliplane through its CLI: one database per business (D6) on a shared cell
//! PostgreSQL, one service per instance. The CLI is the only thing that talks to Sliplane, so
//! it is behind a trait and the whole flow is tested without a network.

use crate::provision::{InstanceSpec, Progress, Provisioned, Provisioner};
use obop_auth::Auth;
use serde_json::Value as J;
use std::sync::Arc;
use std::time::Duration;

/// Runs `sliplane` with the given arguments and returns stdout.
pub trait Runner: Send + Sync {
    fn run(&self, args: &[String]) -> Result<String, String>;
}

pub struct Cli;

/// The CLI prints warnings and a JSON error envelope on stderr. Keep the part a person needs:
/// `code: message`, with the code kept so callers can recognise `not_found`.
pub fn clean_error(stderr: &str) -> String {
    let body: String = stderr
        .lines()
        .filter(|l| !l.trim_start().starts_with("warning:"))
        .collect::<Vec<_>>()
        .join("\n");
    match serde_json::from_str::<J>(body.trim()) {
        Ok(j) => {
            let code = j["code"].as_str().unwrap_or("error");
            let msg = j["error"].as_str().unwrap_or("");
            let detail = j["detail"].as_str().unwrap_or("");
            // Detail repeats the API body; keep it short and only when it adds something.
            let tail = if detail.is_empty() || detail.contains(msg) {
                String::new()
            } else {
                format!(" ({})", detail.chars().take(160).collect::<String>())
            };
            format!("{code}: {msg}{tail}")
        }
        Err(_) => body.trim().to_string(),
    }
}

impl Runner for Cli {
    fn run(&self, args: &[String]) -> Result<String, String> {
        let out = std::process::Command::new("sliplane")
            .arg("--json")
            .args(args)
            .output()
            .map_err(|e| format!("cannot run the sliplane CLI: {e}"))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            Err(clean_error(&String::from_utf8_lossy(&out.stderr)))
        }
    }
}

/// The shared PostgreSQL cluster that holds one database per business.
pub trait CellDb: Send + Sync {
    /// Create role and database for `slug`. The instance connects with `instance` (a name that
    /// resolves inside the hosting network); the control plane reaches the same database with
    /// `control`. They differ only in host when the two sit on different networks.
    fn create_database(&self, slug: &str) -> Result<DbUrls, String>;
    /// Create the first admin in the new database from an existing password hash.
    fn seed_admin(&self, url: &str, email: &str, hash: &str) -> Result<(), String>;
    fn drop_database(&self, slug: &str) -> Result<(), String>;
}

/// `postgres://user:pass@host:port/db` taken apart just enough to rebuild it for another role.
fn split_url(url: &str) -> Result<(String, String), String> {
    let rest = url
        .strip_prefix("postgres://")
        .or_else(|| url.strip_prefix("postgresql://"))
        .ok_or("expected a postgres:// URL")?;
    let (_, host_and_db) = rest.rsplit_once('@').ok_or("the URL needs credentials")?;
    let host = host_and_db.split(['/', '?']).next().unwrap_or("");
    Ok((host.to_string(), host_and_db.to_string()))
}

/// The TLS settings of the admin URL (`sslmode`, `sslrootcert`; D66) as a query to put on the
/// URLs of a new database, so an instance and the control plane reach it as the cell is reached
/// (I4). Empty when the admin URL sets neither. Other parameters stay with the admin URL.
fn tls_query(url: &str) -> String {
    let Some((_, query)) = url.split_once('?') else {
        return String::new();
    };
    let kept: Vec<&str> = query
        .split('&')
        .filter(|kv| {
            let key = kv.split('=').next().unwrap_or("");
            key == "sslmode" || key == "sslrootcert"
        })
        .collect();
    if kept.is_empty() {
        String::new()
    } else {
        format!("?{}", kept.join("&"))
    }
}

/// The two URLs of the role and database `name`, with the admin URL's TLS settings.
fn database_urls(
    admin_url: &str,
    instance_host: Option<&str>,
    name: &str,
    pw: &str,
) -> Result<DbUrls, String> {
    let (host, _) = split_url(admin_url)?;
    let instance_host = instance_host.unwrap_or(&host);
    let q = tls_query(admin_url);
    Ok(DbUrls {
        instance: format!("postgres://{name}:{pw}@{instance_host}/{name}{q}"),
        control: format!("postgres://{name}:{pw}@{host}/{name}{q}"),
    })
}

/// U48: with `sslmode=verify-full` an instance checks the name it connects to, which is the
/// separate instance host when one is set; a cell certificate that does not name it fails every
/// new instance at start. Before anything is created the control plane connects to that host
/// with the cell's own settings (`connect`, given the admin URL with the host replaced) and
/// refuses when that fails. Nothing to check for another `sslmode` or the admin host itself.
fn check_instance_host(
    admin_url: &str,
    instance_host: Option<&str>,
    connect: impl Fn(&str) -> Result<(), String>,
) -> Result<(), String> {
    let (host, host_and_db) = split_url(admin_url)?;
    let Some(other) = instance_host.filter(|h| *h != host) else {
        return Ok(());
    };
    let verify_full = admin_url
        .split_once('?')
        .is_some_and(|(_, q)| q.split('&').any(|kv| kv == "sslmode=verify-full"));
    if !verify_full {
        return Ok(());
    }
    let at = admin_url.len() - host_and_db.len();
    let probe = format!("{}{other}{}", &admin_url[..at], &host_and_db[host.len()..]);
    connect(&probe).map_err(|e| {
        format!(
            "the cell uses sslmode=verify-full and the instance host {other} (OBOP_CELL_INSTANCE_HOST) does not pass that check: the cell's certificate must name {other} too, or use sslmode=verify-ca for the cell (docs/DEPLOY.md); nothing was created ({e})"
        )
    })
}

fn ident(slug: &str) -> Result<String, String> {
    if slug.is_empty()
        || !slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("invalid slug".into());
    }
    Ok(format!("obop_{}", slug.replace('-', "_")))
}

fn random_password() -> String {
    let mut b = [0u8; 24];
    getrandom::fill(&mut b).expect("operating system randomness");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[derive(Debug, Clone)]
pub struct DbUrls {
    pub instance: String,
    pub control: String,
}

pub struct PgCell {
    pub admin_url: String,
    /// `host:port` instances use to reach the cell, when it differs from the admin URL's host.
    pub instance_host: Option<String>,
}

impl CellDb for PgCell {
    fn create_database(&self, slug: &str) -> Result<DbUrls, String> {
        let name = ident(slug)?;
        // Checked before anything is created.
        split_url(&self.admin_url)?;
        check_instance_host(&self.admin_url, self.instance_host.as_deref(), |url| {
            obop_storage::tls::connect(url).map(|_| ())
        })?;
        let pw = random_password();
        let mut c = obop_storage::tls::connect(&self.admin_url)?;
        // `name` is restricted to [a-z0-9_] by `ident`, so interpolating it is safe.
        c.batch_execute(&format!("CREATE ROLE {name} LOGIN PASSWORD '{pw}'"))
            .map_err(|e| e.to_string())?;
        if let Err(e) = c.batch_execute(&format!("CREATE DATABASE {name} OWNER {name}")) {
            let _ = c.batch_execute(&format!("DROP ROLE IF EXISTS {name}"));
            return Err(e.to_string());
        }
        database_urls(&self.admin_url, self.instance_host.as_deref(), &name, &pw)
    }

    fn seed_admin(&self, url: &str, email: &str, hash: &str) -> Result<(), String> {
        // Another database than the one this process serves from: not under its pool settings.
        let backend = obop_storage::open_with(url, Some(obop_storage::PgPoolConfig::one_shot()))?;
        Auth::new(backend).create_user_hashed(
            email,
            hash,
            &[obop_commands::ADMIN_GROUP.to_string()],
        )
    }

    fn drop_database(&self, slug: &str) -> Result<(), String> {
        let name = ident(slug)?;
        let mut c = obop_storage::tls::connect(&self.admin_url)?;
        c.batch_execute(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
            .map_err(|e| e.to_string())?;
        c.batch_execute(&format!("DROP ROLE IF EXISTS {name}"))
            .map_err(|e| e.to_string())
    }
}

#[derive(Debug, Clone)]
pub enum Source {
    /// A container image Sliplane pulls.
    Image(String),
    /// A repository Sliplane builds from its Dockerfile.
    Repo { url: String, branch: String },
}

#[derive(Debug, Clone)]
pub struct Config {
    pub account: String,
    pub project_id: String,
    pub server_id: String,
    pub source: Source,
    /// A catalog base URL handed to every instance as `OBOP_MARKETPLACE_URL`.
    pub marketplace_url: Option<String>,
    /// A directory on a volume the operator has mounted into every instance, handed to each as
    /// OBOP_BACKUP_DIR: the place the verified backup before a module update that rewrites
    /// records is taken (D73). Unset, such an update is refused on an instance until its
    /// operator sets it (OBOP_INSTANCE_BACKUP_DIR on the control plane). Creating the volume is
    /// the operator's step: the provisioner passes the path and does not make storage.
    pub backup_dir: Option<String>,
    /// How often to ask Sliplane whether the service is live, and how long to wait.
    pub poll: std::time::Duration,
    pub timeout: std::time::Duration,
    /// After Sliplane reports the service live, wait until its public address resolves and
    /// accepts connections before calling the instance running. New names reach public DNS a
    /// while after the service is live, and a browser that asks too early caches "not found".
    pub check_reachable: bool,
    /// A Sliplane registry credential id for a private image (`sliplane credentials`), passed
    /// as `--registry-credential-id` when a service is created or moved to an image.
    pub registry_credential: Option<String>,
    /// How long to let a redeploy settle before asking whether the service is live: a service
    /// still reports the old deployment as live for a moment after `deploy`.
    pub settle: std::time::Duration,
    /// A domain we control with a wildcard record pointing at the Sliplane server (for example
    /// `obop.dev`): each new instance is also given `<slug>.<domain>` and uses it as soon as it
    /// resolves. Without it, or while the name does not resolve, the instance keeps its
    /// `*.sliplane.app` address.
    pub instance_domain: Option<String>,
}

impl Config {
    pub const DEFAULT_POLL: std::time::Duration = std::time::Duration::from_secs(5);
    pub const DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20 * 60);
}

/// Asks a running instance whether it is healthy (after an upgrade).
pub trait Health: Send + Sync {
    /// `Ok` when `GET {base}/readyz` answers 200: the server is up, its database answers and its
    /// schema is one it serves (D67). An image from before `/readyz` answers it 404 and is asked
    /// `GET {base}/api/openapi.json` instead (I7): that one answers once the server has opened its
    /// database (a version that cannot read the schema, or a failed migration, never listens).
    fn check(&self, base: &str) -> Result<(), String>;
}

/// One round of the health check with `get` answering the status of a URL: `/readyz`, and
/// `/api/openapi.json` only when `/readyz` is not there (404).
fn ready_once(base: &str, get: &mut dyn FnMut(&str) -> Result<u16, String>) -> Result<(), String> {
    let base = base.trim_end_matches('/');
    let readyz = format!("{base}/readyz");
    match get(&readyz)? {
        200 => Ok(()),
        404 => {
            let old = format!("{base}/api/openapi.json");
            match get(&old)? {
                200 => Ok(()),
                s => Err(format!("{old} answered {s} (no /readyz)")),
            }
        }
        s => Err(format!("{readyz} answered {s}")),
    }
}

/// The health check over HTTPS, retried for up to a minute while the new version starts.
pub struct HttpHealth;

impl Health for HttpHealth {
    fn check(&self, base: &str) -> Result<(), String> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .http_status_as_error(false)
            .build()
            .into();
        let mut get = |url: &str| -> Result<u16, String> {
            agent
                .get(url)
                .call()
                .map(|r| r.status().as_u16())
                .map_err(|e| format!("{url}: {e}"))
        };
        let mut last = String::new();
        for _ in 0..20 {
            match ready_once(base, &mut get) {
                Ok(()) => return Ok(()),
                Err(e) => last = e,
            }
            std::thread::sleep(Duration::from_secs(3));
        }
        Err(format!(
            "{} was not ready within a minute ({last})",
            base.trim_end_matches('/')
        ))
    }
}

pub struct Sliplane {
    cfg: Config,
    runner: Arc<dyn Runner>,
    cell: Arc<dyn CellDb>,
    health: Arc<dyn Health>,
}

impl Sliplane {
    pub fn new(cfg: Config, runner: Arc<dyn Runner>, cell: Arc<dyn CellDb>) -> Self {
        Self {
            cfg,
            runner,
            cell,
            health: Arc::new(HttpHealth),
        }
    }

    /// Replace the health check (tests).
    pub fn with_health(mut self, health: Arc<dyn Health>) -> Self {
        self.health = health;
        self
    }

    fn service_name(slug: &str) -> String {
        format!("obop-{slug}")
    }

    fn base(&self, cmd: &[&str]) -> Vec<String> {
        let mut v: Vec<String> = cmd.iter().map(|s| s.to_string()).collect();
        v.extend([
            "-a".into(),
            self.cfg.account.clone(),
            "--project-id".into(),
            self.cfg.project_id.clone(),
        ]);
        v
    }
}

impl Sliplane {
    /// Poll until Sliplane reports the service live, reporting progress. A failed build or
    /// deploy shows up as an event; its message becomes the error.
    fn wait_live(&self, service_id: &str, progress: Progress) -> Result<(), String> {
        let started = std::time::Instant::now();
        let mut failures = 0u32;
        loop {
            let mut get = self.base(&["services", "get"]);
            get.extend(["--service-id".into(), service_id.into()]);
            match self.runner.run(&get) {
                Ok(out) => {
                    failures = 0;
                    let j: J = serde_json::from_str(&out).map_err(|e| e.to_string())?;
                    if j["status"].as_str() == Some("live") {
                        return Ok(());
                    }
                }
                // Sliplane can answer "not found" for a service it created a moment ago, and the
                // network can blip: keep waiting, and give up only after repeated failures.
                Err(e) => {
                    failures += 1;
                    let young =
                        e.contains("not_found") && started.elapsed() < Duration::from_secs(90);
                    if !young && failures >= 3 {
                        return Err(e);
                    }
                }
            }
            let mut ev = self.base(&["services", "events"]);
            ev.extend(["--service-id".into(), service_id.into()]);
            if let Ok(out) = self.runner.run(&ev)
                && let Ok(J::Array(events)) = serde_json::from_str::<J>(&out)
                && let Some(f) = events
                    .iter()
                    .find(|e| e["type"].as_str().is_some_and(|t| t.ends_with("_failed")))
            {
                let msg = f["message"].as_str().unwrap_or("the build failed");
                return Err(format!(
                    "the service did not start: {}",
                    msg.chars().take(300).collect::<String>()
                ));
            }
            if started.elapsed() > self.cfg.timeout {
                return Err("the service did not come up in time".into());
            }
            let secs = started.elapsed().as_secs();
            progress(&format!(
                "Building and starting the service ({}m {:02}s)",
                secs / 60,
                secs % 60
            ));
            std::thread::sleep(self.cfg.poll);
        }
    }

    /// Wait for the public address to resolve and accept a TCP connection on 443.
    fn wait_reachable(&self, domain: &str, progress: Progress) -> Result<(), String> {
        use std::net::{TcpStream, ToSocketAddrs};
        let started = std::time::Instant::now();
        loop {
            let ok = (domain, 443)
                .to_socket_addrs()
                .ok()
                .and_then(|mut a| a.next())
                .is_some_and(|addr| {
                    TcpStream::connect_timeout(&addr, Duration::from_secs(3)).is_ok()
                });
            if ok {
                return Ok(());
            }
            if started.elapsed() > Duration::from_secs(10 * 60) {
                return Err(format!(
                    "the service is live but {domain} does not resolve yet; try again in a few minutes"
                ));
            }
            let secs = started.elapsed().as_secs();
            progress(&format!(
                "Waiting for the address to resolve ({}m {:02}s)",
                secs / 60,
                secs % 60
            ));
            std::thread::sleep(Duration::from_secs(5));
        }
    }

    /// Attach `<slug>.<instance_domain>` to the service. `Some(name)` once the name resolves
    /// (waiting up to three minutes), `None` when no domain is configured, Sliplane refuses it
    /// or DNS has no wildcard record: the instance then keeps its managed address.
    fn custom_domain(&self, slug: &str, id: &str, progress: Progress) -> Option<String> {
        use std::net::ToSocketAddrs;
        let base = self.cfg.instance_domain.as_deref()?.trim_matches('.');
        if base.is_empty() {
            return None;
        }
        let name = format!("{slug}.{base}");
        let mut add = self.base(&["services", "add-domain"]);
        add.extend([
            "--service-id".into(),
            id.into(),
            "--domain".into(),
            name.clone(),
        ]);
        self.runner.run(&add).ok()?;
        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_secs(180) {
            if (name.as_str(), 443).to_socket_addrs().is_ok() {
                return Some(name);
            }
            progress(&format!("Waiting for {name} to resolve"));
            std::thread::sleep(self.cfg.poll);
        }
        None
    }

    /// Delete a service after checking that it is one of ours.
    fn delete_service(&self, slug: &str, id: &str) -> Result<(), String> {
        let mut get = self.base(&["services", "get"]);
        get.extend(["--service-id".into(), id.into()]);
        let out = match self.runner.run(&get) {
            Ok(o) => o,
            // Already gone is fine: the goal is that it does not exist.
            Err(e) if e.contains("not_found") || e.contains("does not exist") => return Ok(()),
            Err(e) => return Err(e),
        };
        let j: J = serde_json::from_str(&out).map_err(|e| e.to_string())?;
        if j["name"].as_str() != Some(&Self::service_name(slug)) {
            return Err("refusing to delete: the service name does not match this instance".into());
        }
        let mut del = self.base(&["services", "delete"]);
        del.extend(["--service-id".into(), id.into()]);
        self.runner.run(&del).map(|_| ())
    }
}

/// 32 random bytes as 64 hex digits, for `OBOP_SECRET_KEY`.
fn new_secret_key() -> Result<String, String> {
    let mut b = [0u8; 32];
    getrandom::fill(&mut b).map_err(|e| format!("no operating system randomness: {e}"))?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

impl Provisioner for Sliplane {
    fn create(&self, spec: &InstanceSpec, progress: Progress) -> Result<Provisioned, String> {
        progress("Creating the database");
        let urls = self.cell.create_database(&spec.slug)?;
        let mut service_id: Option<String> = None;
        let result = (|| {
            progress("Creating your admin account");
            self.cell
                .seed_admin(&urls.control, &spec.owner_email, &spec.password_hash)?;
            progress("Creating the service");
            let mut args = self.base(&["services", "create"]);
            args.extend([
                "--name".into(),
                Self::service_name(&spec.slug),
                "--server-id".into(),
                self.cfg.server_id.clone(),
                "--public".into(),
                "--protocol".into(),
                "http".into(),
            ]);
            match &self.cfg.source {
                Source::Image(i) => {
                    args.extend(["--image".into(), i.clone()]);
                    if let Some(c) = &self.cfg.registry_credential {
                        args.extend(["--registry-credential-id".into(), c.clone()]);
                    }
                }
                // Instances must not rebuild on every push: upgrades are deliberate.
                Source::Repo { url, branch } => args.extend([
                    "--repo".into(),
                    url.clone(),
                    "--branch".into(),
                    branch.clone(),
                    "--auto-deploy".into(),
                    "false".into(),
                ]),
            }
            args.extend([
                "--secret-env".into(),
                format!("OBOP_DATABASE_URL={}", urls.instance),
            ]);
            // D53: every instance seals its two-factor secrets under its own random key. It
            // exists only as this secret env var; it is never logged, stored by the control
            // plane or put on the command line of anything but this one call.
            args.extend([
                "--secret-env".into(),
                format!("OBOP_SECRET_KEY={}", new_secret_key()?),
            ]);
            if let Some(m) = &self.cfg.marketplace_url {
                args.extend(["--env".into(), format!("OBOP_MARKETPLACE_URL={m}")]);
            }
            if let Some(d) = &self.cfg.backup_dir {
                args.extend(["--env".into(), format!("OBOP_BACKUP_DIR={d}")]);
            }
            // D63: the instance may ask this control plane to upgrade it (and nothing else)
            // with its own token, which is a secret like the key above.
            if !spec.update_token.is_empty() && !spec.control_url.is_empty() {
                args.extend([
                    "--secret-env".into(),
                    format!("OBOP_UPDATE_TOKEN={}", spec.update_token),
                    "--env".into(),
                    format!("OBOP_UPDATE_URL={}", spec.control_url),
                ]);
            }
            // One process per instance, so it may run its own schema migrations when a new
            // version starts (docs/DEPLOY.md, "Upgrades"): an upgrade is then one redeploy.
            args.extend(["--env".into(), "OBOP_AUTO_MIGRATE=1".into()]);
            let out = self.runner.run(&args)?;
            let j: J = serde_json::from_str(&out)
                .map_err(|e| format!("unexpected sliplane output: {e}"))?;
            let id = j["id"]
                .as_str()
                .ok_or("sliplane returned no service id")?
                .to_string();
            service_id = Some(id.clone());
            let domain = j["network"]["managedDomain"]
                .as_str()
                .ok_or("sliplane returned no domain")?;
            progress("Building and starting the service (0m 00s)");
            self.wait_live(&id, progress)?;
            if self.cfg.check_reachable {
                self.wait_reachable(domain, progress)?;
            }
            let host = self
                .custom_domain(&spec.slug, &id, progress)
                .unwrap_or_else(|| domain.to_string());
            Ok(Provisioned {
                external_id: id,
                url: format!("https://{host}"),
            })
        })();
        if result.is_err() {
            // Leave nothing behind: a failed instance must not hold its name or database.
            if let Some(id) = &service_id {
                let _ = self.delete_service(&spec.slug, id);
            }
            let _ = self.cell.drop_database(&spec.slug);
        }
        result
    }

    fn image(&self) -> Option<String> {
        match &self.cfg.source {
            Source::Image(i) => Some(i.clone()),
            Source::Repo { .. } => None,
        }
    }

    fn upgrade(
        &self,
        slug: &str,
        external_id: &str,
        image: &str,
        progress: Progress,
    ) -> Result<(), String> {
        // Only ever touch a service that is ours.
        let mut get = self.base(&["services", "get"]);
        get.extend(["--service-id".into(), external_id.into()]);
        let j: J = serde_json::from_str(&self.runner.run(&get)?).map_err(|e| e.to_string())?;
        if j["name"].as_str() != Some(&Self::service_name(slug)) {
            return Err(
                "refusing to upgrade: the service name does not match this instance".into(),
            );
        }
        let domain = j["network"]["managedDomain"]
            .as_str()
            .ok_or("sliplane returned no domain")?
            .to_string();
        progress(&format!("Pointing the service at {image}"));
        let mut upd = self.base(&["services", "update"]);
        upd.extend([
            "--service-id".into(),
            external_id.into(),
            "--image".into(),
            image.into(),
            // Add to the environment, never replace it: the secrets stay as they are.
            "--env".into(),
            "OBOP_AUTO_MIGRATE=1".into(),
        ]);
        // Instances created before the control plane knew its backup directory get it with the
        // next upgrade (D73): merged in like the line above, so nothing else is replaced.
        if let Some(d) = &self.cfg.backup_dir {
            upd.extend(["--env".into(), format!("OBOP_BACKUP_DIR={d}")]);
        }
        upd.push("--merge-env".into());
        if let Some(c) = &self.cfg.registry_credential {
            upd.extend(["--registry-credential-id".into(), c.clone()]);
        }
        self.runner.run(&upd)?;
        progress("Deploying the new version");
        let mut dep = self.base(&["services", "deploy"]);
        dep.extend(["--service-id".into(), external_id.into()]);
        self.runner.run(&dep)?;
        std::thread::sleep(self.cfg.settle);
        self.wait_live(external_id, progress)?;
        // The service must report the image we asked for, when it says which it runs.
        let mut again = self.base(&["services", "get"]);
        again.extend(["--service-id".into(), external_id.into()]);
        if let Ok(out) = self.runner.run(&again)
            && let Ok(now) = serde_json::from_str::<J>(&out)
            && let Some(running) = now["image"].as_str().or(now["source"]["image"].as_str())
            && running != image
        {
            return Err(format!(
                "the service is live but runs {running}, not {image}"
            ));
        }
        progress("Waiting for the new version to answer");
        self.health.check(&format!("https://{domain}"))
    }

    fn delete(&self, slug: &str, external_id: &str, progress: Progress) -> Result<(), String> {
        if !external_id.is_empty() {
            progress("Removing the service");
            self.delete_service(slug, external_id)?;
        }
        progress("Removing the database");
        self.cell.drop_database(slug)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn a_new_database_keeps_the_tls_settings_of_the_cell() {
        let u = database_urls(
            "postgres://admin:x@db.example.com:5432/postgres?sslmode=verify-full&sslrootcert=/etc/obop/ca.pem&application_name=cp",
            Some("db.internal:5432"),
            "obop_acme",
            "pw",
        )
        .unwrap();
        assert_eq!(
            u.instance,
            "postgres://obop_acme:pw@db.internal:5432/obop_acme?sslmode=verify-full&sslrootcert=/etc/obop/ca.pem"
        );
        assert_eq!(
            u.control,
            "postgres://obop_acme:pw@db.example.com:5432/obop_acme?sslmode=verify-full&sslrootcert=/etc/obop/ca.pem"
        );
        // A URL with no path before its query, and one with no TLS settings.
        let u = database_urls("postgres://a:b@db?sslmode=require", None, "obop_x", "pw").unwrap();
        assert_eq!(u.control, "postgres://obop_x:pw@db/obop_x?sslmode=require");
        let u = database_urls("postgres://a:b@db:5432/postgres", None, "obop_x", "pw").unwrap();
        assert_eq!(u.instance, "postgres://obop_x:pw@db:5432/obop_x");
    }

    #[test]
    fn verify_full_with_a_separate_instance_host_is_checked_before_anything_is_made() {
        let admin = "postgres://admin:x@db.example.com:5432/postgres?sslmode=verify-full&sslrootcert=/etc/obop/ca.pem";
        let asked = std::cell::RefCell::new(Vec::new());
        let refuse = |url: &str| -> Result<(), String> {
            asked.borrow_mut().push(url.to_string());
            Err("certificate name mismatch".into())
        };
        let e = check_instance_host(admin, Some("db.internal:5432"), refuse).unwrap_err();
        assert!(
            e.contains("verify-ca") && e.contains("db.internal:5432"),
            "{e}"
        );
        assert!(!e.contains("admin:x"), "no credentials in the message: {e}");
        assert_eq!(
            asked.borrow().as_slice(),
            [
                "postgres://admin:x@db.internal:5432/postgres?sslmode=verify-full&sslrootcert=/etc/obop/ca.pem"
            ]
        );
        // The host passes the check.
        assert!(check_instance_host(admin, Some("db.internal:5432"), |_| Ok(())).is_ok());
        // Nothing to check: verify-ca, no instance host, the admin host itself.
        let never = |_: &str| -> Result<(), String> { panic!("no check expected") };
        let ca = admin.replace("verify-full", "verify-ca");
        assert!(check_instance_host(&ca, Some("db.internal:5432"), never).is_ok());
        assert!(check_instance_host(admin, None, never).is_ok());
        assert!(check_instance_host(admin, Some("db.example.com:5432"), never).is_ok());
    }

    type Asked = std::rc::Rc<std::cell::RefCell<Vec<String>>>;

    fn answering(
        map: Vec<(&'static str, u16)>,
    ) -> (Asked, impl FnMut(&str) -> Result<u16, String>) {
        let asked: Asked = Default::default();
        let a = asked.clone();
        let get = move |url: &str| {
            a.borrow_mut().push(url.to_string());
            Ok(map
                .iter()
                .find(|(p, _)| url.ends_with(p))
                .map_or(500, |(_, s)| *s))
        };
        (asked, get)
    }

    #[test]
    fn the_health_check_asks_readyz_and_the_old_route_only_where_readyz_is_missing() {
        // A current image: ready, and the old route is never asked.
        let (asked, mut get) = answering(vec![("/readyz", 200), ("/api/openapi.json", 200)]);
        assert!(ready_once("https://a.example/", &mut get).is_ok());
        assert_eq!(
            *asked.borrow(),
            vec!["https://a.example/readyz".to_string()]
        );
        // Not ready (its database needs migration): no fall back to the route that would pass.
        let (asked, mut get) = answering(vec![("/readyz", 503), ("/api/openapi.json", 200)]);
        let e = ready_once("https://a.example", &mut get).unwrap_err();
        assert!(e.contains("503"), "{e}");
        assert_eq!(asked.borrow().len(), 1);
        // An image from before /readyz.
        let (asked, mut get) = answering(vec![("/readyz", 404), ("/api/openapi.json", 200)]);
        assert!(ready_once("https://a.example", &mut get).is_ok());
        assert_eq!(asked.borrow().len(), 2);
        let (_, mut get) = answering(vec![("/readyz", 404), ("/api/openapi.json", 502)]);
        assert!(ready_once("https://a.example", &mut get).is_err());
    }

    #[derive(Default)]
    struct FakeRunner {
        calls: Mutex<Vec<Vec<String>>>,
        reply: Mutex<Vec<Result<String, String>>>,
    }
    impl Runner for FakeRunner {
        fn run(&self, args: &[String]) -> Result<String, String> {
            self.calls.lock().unwrap().push(args.to_vec());
            self.reply.lock().unwrap().remove(0)
        }
    }

    #[derive(Default)]
    struct FakeCell {
        log: Mutex<Vec<String>>,
    }
    impl CellDb for FakeCell {
        fn create_database(&self, slug: &str) -> Result<DbUrls, String> {
            self.log.lock().unwrap().push(format!("create {slug}"));
            Ok(DbUrls {
                instance: format!("postgres://obop_{slug}:pw@db.internal:5432/obop_{slug}"),
                control: format!("postgres://obop_{slug}:pw@db:5432/obop_{slug}"),
            })
        }
        fn seed_admin(&self, _: &str, email: &str, hash: &str) -> Result<(), String> {
            assert!(hash.starts_with("$argon2id$"));
            self.log.lock().unwrap().push(format!("admin {email}"));
            Ok(())
        }
        fn drop_database(&self, slug: &str) -> Result<(), String> {
            self.log.lock().unwrap().push(format!("drop {slug}"));
            Ok(())
        }
    }

    fn cfg(source: Source) -> Config {
        Config {
            account: "acct".into(),
            project_id: "project_1".into(),
            server_id: "server_1".into(),
            source,
            marketplace_url: None,
            backup_dir: None,
            poll: std::time::Duration::ZERO,
            timeout: std::time::Duration::from_secs(5),
            check_reachable: false,
            registry_credential: None,
            settle: std::time::Duration::ZERO,
            instance_domain: None,
        }
    }

    const LIVE: &str = r#"{"status":"live"}"#;
    const NOOP: Progress<'static> = &|_| {};

    fn spec() -> InstanceSpec {
        InstanceSpec {
            slug: "acme".into(),
            owner_email: "ada@example.test".into(),
            password_hash: "$argon2id$v=19$x".into(),
            update_token: "obu_0123456789abcdef0123456789abcdef0123456789abcdef".into(),
            control_url: "https://control.example.test".into(),
        }
    }

    const CREATED: &str = r#"{"id":"service_9","name":"obop-acme","network":{"managedDomain":"obop-acme.sliplane.app"}}"#;

    #[test]
    fn a_refused_custom_domain_keeps_the_managed_address() {
        let (runner, cell) = (
            Arc::new(FakeRunner::default()),
            Arc::new(FakeCell::default()),
        );
        runner.reply.lock().unwrap().extend([
            Ok(CREATED.into()),
            Ok(LIVE.into()),
            Err("domain refused".into()),
        ]);
        let mut c = cfg(Source::Image("ghcr.io/x/obop:1".into()));
        c.instance_domain = Some("obop.dev".into());
        let s = Sliplane::new(c, runner.clone(), cell);
        let p = s.create(&spec(), NOOP).unwrap();
        assert_eq!(p.url, "https://obop-acme.sliplane.app");
        let calls = runner.calls.lock().unwrap();
        let add = calls.last().unwrap();
        assert!(add.contains(&"add-domain".to_string()));
        assert!(add.contains(&"acme.obop.dev".to_string()));
    }

    #[test]
    fn create_makes_the_database_then_the_service() {
        let (runner, cell) = (
            Arc::new(FakeRunner::default()),
            Arc::new(FakeCell::default()),
        );
        runner
            .reply
            .lock()
            .unwrap()
            .extend([Ok(CREATED.into()), Ok(LIVE.into())]);
        let s = Sliplane::new(
            cfg(Source::Image("ghcr.io/x/obop:1".into())),
            runner.clone(),
            cell.clone(),
        );
        let seen = Mutex::new(Vec::<String>::new());
        let p = s
            .create(&spec(), &|ph| seen.lock().unwrap().push(ph.into()))
            .unwrap();
        assert_eq!(
            *seen.lock().unwrap(),
            [
                "Creating the database",
                "Creating your admin account",
                "Creating the service",
                "Building and starting the service (0m 00s)"
            ]
        );
        assert_eq!(
            (p.external_id.as_str(), p.url.as_str()),
            ("service_9", "https://obop-acme.sliplane.app")
        );
        assert_eq!(
            *cell.log.lock().unwrap(),
            ["create acme", "admin ada@example.test"]
        );
        let args = runner.calls.lock().unwrap()[0].join(" ");
        for want in [
            "services create",
            "-a acct",
            "--project-id project_1",
            "--name obop-acme",
            "--server-id server_1",
            "--public",
            "--image ghcr.io/x/obop:1",
            "--secret-env OBOP_DATABASE_URL=postgres://obop_acme:pw@db.internal:5432/obop_acme",
        ] {
            assert!(args.contains(want), "{want} missing from {args}");
        }
        assert!(
            !args.contains("--env OBOP_DATABASE_URL"),
            "the database URL is a secret env, never a plain one"
        );
    }

    /// D53: each instance gets its own 32-byte key, passed only as a secret env.
    #[test]
    fn every_instance_gets_its_own_secret_key_as_a_secret_env() {
        let key_of = || {
            let (runner, cell) = (
                Arc::new(FakeRunner::default()),
                Arc::new(FakeCell::default()),
            );
            runner
                .reply
                .lock()
                .unwrap()
                .extend([Ok(CREATED.into()), Ok(LIVE.into())]);
            Sliplane::new(
                cfg(Source::Image("ghcr.io/x/obop:1".into())),
                runner.clone(),
                cell,
            )
            .create(&spec(), NOOP)
            .unwrap();
            let call = runner.calls.lock().unwrap()[0].clone();
            assert!(
                !call.join(" ").contains("--env OBOP_SECRET_KEY"),
                "never a plain env"
            );
            let at = call
                .iter()
                .position(|a| a.starts_with("OBOP_SECRET_KEY="))
                .expect("OBOP_SECRET_KEY passed");
            assert_eq!(call[at - 1], "--secret-env");
            call[at].trim_start_matches("OBOP_SECRET_KEY=").to_string()
        };
        let (a, b) = (key_of(), key_of());
        assert_eq!(a.len(), 64);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b, "a fresh key per instance");
        assert!(
            obop_auth::seal::parse_key(&a).is_ok(),
            "the instance accepts it"
        );
    }

    #[test]
    fn repo_source_and_marketplace_env() {
        let (runner, cell) = (
            Arc::new(FakeRunner::default()),
            Arc::new(FakeCell::default()),
        );
        runner
            .reply
            .lock()
            .unwrap()
            .extend([Ok(CREATED.into()), Ok(LIVE.into())]);
        let mut c = cfg(Source::Repo {
            url: "https://github.com/SpaceCorps/obop".into(),
            branch: "deploy".into(),
        });
        c.marketplace_url = Some("https://market.example.test".into());
        c.backup_dir = Some("/data/backups".into());
        Sliplane::new(c, runner.clone(), cell)
            .create(&spec(), NOOP)
            .unwrap();
        let args = runner.calls.lock().unwrap()[0].join(" ");
        assert!(
            args.contains("--env OBOP_BACKUP_DIR=/data/backups"),
            "{args}"
        );
        assert!(args.contains(
            "--repo https://github.com/SpaceCorps/obop --branch deploy --auto-deploy false"
        ));
        assert!(args.contains("--env OBOP_MARKETPLACE_URL=https://market.example.test"));
    }

    #[test]
    fn a_failed_service_drops_its_database() {
        let (runner, cell) = (
            Arc::new(FakeRunner::default()),
            Arc::new(FakeCell::default()),
        );
        runner
            .reply
            .lock()
            .unwrap()
            .push(Err("rate_limited".into()));
        let e = Sliplane::new(cfg(Source::Image("i".into())), runner, cell.clone())
            .create(&spec(), NOOP)
            .unwrap_err();
        assert_eq!(e, "rate_limited");
        assert_eq!(cell.log.lock().unwrap().last().unwrap(), "drop acme");
    }

    #[test]
    fn delete_checks_the_name_before_deleting() {
        let (runner, cell) = (
            Arc::new(FakeRunner::default()),
            Arc::new(FakeCell::default()),
        );
        runner
            .reply
            .lock()
            .unwrap()
            .push(Ok(r#"{"name":"hangar"}"#.into()));
        let s = Sliplane::new(cfg(Source::Image("i".into())), runner.clone(), cell.clone());
        assert!(
            s.delete("acme", "service_9", NOOP)
                .unwrap_err()
                .contains("refusing")
        );
        assert_eq!(
            runner.calls.lock().unwrap().len(),
            1,
            "no delete call was made"
        );
        assert!(cell.log.lock().unwrap().is_empty());

        runner
            .reply
            .lock()
            .unwrap()
            .extend([Ok(r#"{"name":"obop-acme"}"#.into()), Ok("{}".into())]);
        s.delete("acme", "service_9", NOOP).unwrap();
        assert!(
            runner.calls.lock().unwrap()[2]
                .join(" ")
                .contains("services delete")
        );
        assert_eq!(*cell.log.lock().unwrap(), ["drop acme"]);
    }

    #[test]
    fn waits_for_the_build_and_reports_elapsed_time() {
        let (runner, cell) = (
            Arc::new(FakeRunner::default()),
            Arc::new(FakeCell::default()),
        );
        // created, then pending (get + events) twice, then live
        runner.reply.lock().unwrap().extend([
            Ok(CREATED.into()),
            Ok(r#"{"status":"pending"}"#.into()),
            Ok("[]".into()),
            Ok(r#"{"status":"pending"}"#.into()),
            Ok("[]".into()),
            Ok(LIVE.into()),
        ]);
        let seen = Mutex::new(Vec::<String>::new());
        Sliplane::new(cfg(Source::Image("i".into())), runner, cell)
            .create(&spec(), &|ph| seen.lock().unwrap().push(ph.into()))
            .unwrap();
        let building = seen
            .lock()
            .unwrap()
            .iter()
            .filter(|p| p.starts_with("Building and starting"))
            .count();
        assert_eq!(building, 3, "initial report plus one per poll");
    }

    #[test]
    fn a_failed_build_is_reported_and_cleaned_up() {
        let (runner, cell) = (
            Arc::new(FakeRunner::default()),
            Arc::new(FakeCell::default()),
        );
        runner.reply.lock().unwrap().extend([
            Ok(CREATED.into()),
            Ok(r#"{"status":"pending"}"#.into()),
            Ok(r#"[{"type":"service_build_failed","message":"process did not complete successfully: exit code: 1"}]"#.into()),
            // cleanup: the name check, then the delete
            Ok(r#"{"name":"obop-acme"}"#.into()),
            Ok("{}".into()),
        ]);
        let e = Sliplane::new(cfg(Source::Image("i".into())), runner.clone(), cell.clone())
            .create(&spec(), NOOP)
            .unwrap_err();
        assert!(e.contains("exit code: 1"), "{e}");
        assert!(
            runner
                .calls
                .lock()
                .unwrap()
                .last()
                .unwrap()
                .join(" ")
                .contains("services delete")
        );
        assert_eq!(cell.log.lock().unwrap().last().unwrap(), "drop acme");
    }

    #[test]
    fn a_just_created_service_that_is_briefly_not_found_is_waited_for() {
        let (runner, cell) = (
            Arc::new(FakeRunner::default()),
            Arc::new(FakeCell::default()),
        );
        runner.reply.lock().unwrap().extend([
            Ok(CREATED.into()),
            Err("not_found: The resource does not exist.".into()),
            Err("not_found: The resource does not exist.".into()),
            Err("not_found: The resource does not exist.".into()),
            Err("not_found: The resource does not exist.".into()),
            Ok(LIVE.into()),
        ]);
        Sliplane::new(cfg(Source::Image("i".into())), runner, cell)
            .create(&spec(), NOOP)
            .unwrap();
    }

    #[test]
    fn errors_from_the_cli_are_short_and_keep_their_code() {
        let raw = "warning: API keys are stored unencrypted in /root/.config/sliplane-cli/secrets.json (SLIPLANE_ALLOW_PLAINTEXT_STORE=1).\n{\n  \"error\": \"The resource does not exist.\",\n  \"code\": \"not_found\",\n  \"detail\": \"HTTP 404: {\\\"code\\\":\\\"not_found\\\",\\\"message\\\":\\\"Service not found\\\"}\"\n}";
        let e = clean_error(raw);
        assert!(
            e.starts_with("not_found: The resource does not exist."),
            "{e}"
        );
        assert!(!e.contains("warning") && !e.contains("secrets.json"));
        assert_eq!(clean_error("plain failure"), "plain failure");
    }

    #[test]
    fn times_out_when_the_service_never_comes_up() {
        let (runner, cell) = (
            Arc::new(FakeRunner::default()),
            Arc::new(FakeCell::default()),
        );
        let mut c = cfg(Source::Image("i".into()));
        c.timeout = std::time::Duration::ZERO;
        runner.reply.lock().unwrap().extend([
            Ok(CREATED.into()),
            Ok(r#"{"status":"pending"}"#.into()),
            Ok("[]".into()),
            Ok(r#"{"name":"obop-acme"}"#.into()),
            Ok("{}".into()),
        ]);
        let e = Sliplane::new(c, runner, cell)
            .create(&spec(), NOOP)
            .unwrap_err();
        assert!(e.contains("in time"));
    }

    #[test]
    fn deleting_a_service_that_is_already_gone_still_drops_the_database() {
        let (runner, cell) = (
            Arc::new(FakeRunner::default()),
            Arc::new(FakeCell::default()),
        );
        runner
            .reply
            .lock()
            .unwrap()
            .push(Err("code: not_found".into()));
        Sliplane::new(cfg(Source::Image("i".into())), runner, cell.clone())
            .delete("acme", "service_9", NOOP)
            .unwrap();
        assert_eq!(*cell.log.lock().unwrap(), ["drop acme"]);
    }

    #[test]
    fn identifiers_are_locked_down() {
        assert_eq!(ident("acme-co").unwrap(), "obop_acme_co");
        assert!(ident("a; DROP DATABASE x").is_err());
        assert!(ident("").is_err());
        assert!(ident("Acme").is_err());
        assert_eq!(
            split_url("postgres://u:p@host:5432/db").unwrap().0,
            "host:5432"
        );
    }

    /// Against a real PostgreSQL: `OBOP_TEST_POSTGRES_URL=postgres://postgres:...@127.0.0.1:55432/postgres`.
    #[test]
    fn pg_cell_creates_and_drops_a_working_database() {
        let Ok(admin) = std::env::var("OBOP_TEST_POSTGRES_URL") else {
            return;
        };
        let cell = PgCell {
            admin_url: admin,
            instance_host: None,
        };
        let slug = format!("t{}", std::process::id());
        let url = cell.create_database(&slug).unwrap().control;
        let auth = Auth::with_test_params(obop_storage::open(&url).unwrap());
        auth.create_user_hashed(
            "a@b.test",
            &Auth::with_test_params(obop_storage::open("memory:").unwrap())
                .hash_for_test("long enough pw!"),
            &[],
        )
        .unwrap();
        assert_eq!(auth.user_count(), 1);
        cell.drop_database(&slug).unwrap();
        assert!(obop_storage::open(&url).is_err(), "the database is gone");
    }
    struct Healthy(Mutex<Vec<String>>, Result<(), String>);
    impl Health for Healthy {
        fn check(&self, base: &str) -> Result<(), String> {
            self.0.lock().unwrap().push(base.into());
            self.1.clone()
        }
    }

    const IMG: &str = "ghcr.io/spacecorps/obop:abc123";

    fn upgrader(
        replies: Vec<Result<String, String>>,
        health: Result<(), String>,
    ) -> (Sliplane, Arc<FakeRunner>, Arc<Healthy>) {
        let runner = Arc::new(FakeRunner::default());
        runner.reply.lock().unwrap().extend(replies);
        let h = Arc::new(Healthy(Mutex::default(), health));
        let mut c = cfg(Source::Image("ghcr.io/spacecorps/obop:old".into()));
        c.registry_credential = Some("cred_1".into());
        let s =
            Sliplane::new(c, runner.clone(), Arc::new(FakeCell::default())).with_health(h.clone());
        (s, runner, h)
    }

    #[test]
    fn an_upgrade_points_the_service_at_the_image_redeploys_and_checks_health() {
        let live = format!(r#"{{"status":"live","image":"{IMG}"}}"#);
        let (s, runner, health) = upgrader(
            vec![
                Ok(CREATED.into()),
                Ok("{}".into()),
                Ok("{}".into()),
                Ok(LIVE.into()),
                Ok(live),
            ],
            Ok(()),
        );
        let seen = Mutex::new(Vec::<String>::new());
        s.upgrade("acme", "service_9", IMG, &|p| {
            seen.lock().unwrap().push(p.into())
        })
        .unwrap();
        let calls: Vec<String> = runner
            .calls
            .lock()
            .unwrap()
            .iter()
            .map(|c| c.join(" "))
            .collect();
        assert!(
            calls[0].starts_with("services get") && calls[0].contains("--service-id service_9")
        );
        let update = &calls[1];
        for want in [
            "services update",
            "-a acct",
            "--project-id project_1",
            "--service-id service_9",
            "--image ghcr.io/spacecorps/obop:abc123",
            "--env OBOP_AUTO_MIGRATE=1",
            "--merge-env",
            "--registry-credential-id cred_1",
        ] {
            assert!(update.contains(want), "{want} missing from {update}");
        }
        // The environment is only ever merged: no secret is dropped.
        assert!(!update.contains("--replace-env") && !update.contains("OBOP_SECRET_KEY"));
        assert!(calls[2].starts_with("services deploy") && calls[2].contains("service_9"));
        assert_eq!(
            *health.0.lock().unwrap(),
            ["https://obop-acme.sliplane.app"]
        );
        let seen = seen.lock().unwrap();
        assert!(
            seen[0].contains(IMG) && seen.last().unwrap().contains("answer"),
            "{seen:?}"
        );
    }

    /// An instance made before the control plane knew its backup directory gets it with its next
    /// upgrade, merged into the environment like the rest (nothing replaced).
    #[test]
    fn an_upgrade_also_hands_a_configured_backup_directory_to_older_instances() {
        let live = format!(r#"{{"status":"live","image":"{IMG}"}}"#);
        let runner = Arc::new(FakeRunner::default());
        runner.reply.lock().unwrap().extend(vec![
            Ok(CREATED.into()),
            Ok("{}".into()),
            Ok("{}".into()),
            Ok(LIVE.into()),
            Ok(live),
        ]);
        let mut c = cfg(Source::Image("ghcr.io/spacecorps/obop:old".into()));
        c.backup_dir = Some("/data/backups".into());
        let s = Sliplane::new(c, runner.clone(), Arc::new(FakeCell::default()))
            .with_health(Arc::new(Healthy(Mutex::default(), Ok(()))));
        s.upgrade("acme", "service_9", IMG, &|_| {}).unwrap();
        let update = runner.calls.lock().unwrap()[1].join(" ");
        assert!(
            update.contains("--env OBOP_BACKUP_DIR=/data/backups"),
            "{update}"
        );
        assert!(update.contains("--merge-env") && !update.contains("--replace-env"));
        // Without a configured directory nothing is added.
        let runner2 = Arc::new(FakeRunner::default());
        let live = format!(r#"{{"status":"live","image":"{IMG}"}}"#);
        runner2.reply.lock().unwrap().extend(vec![
            Ok(CREATED.into()),
            Ok("{}".into()),
            Ok("{}".into()),
            Ok(LIVE.into()),
            Ok(live),
        ]);
        let s2 = Sliplane::new(
            cfg(Source::Image("ghcr.io/spacecorps/obop:old".into())),
            runner2.clone(),
            Arc::new(FakeCell::default()),
        )
        .with_health(Arc::new(Healthy(Mutex::default(), Ok(()))));
        s2.upgrade("acme", "service_9", IMG, &|_| {}).unwrap();
        assert!(
            !runner2.calls.lock().unwrap()[1]
                .join(" ")
                .contains("OBOP_BACKUP_DIR")
        );
    }

    #[test]
    fn an_upgrade_refuses_a_service_that_is_not_the_instances_own() {
        let (s, runner, health) = upgrader(
            vec![Ok(r#"{"id":"service_9","name":"someone-else"}"#.into())],
            Ok(()),
        );
        let e = s.upgrade("acme", "service_9", IMG, NOOP).unwrap_err();
        assert!(e.contains("does not match"), "{e}");
        assert_eq!(runner.calls.lock().unwrap().len(), 1, "nothing was changed");
        assert!(health.0.lock().unwrap().is_empty());
    }

    #[test]
    fn an_upgrade_fails_when_the_service_does_not_come_up_or_runs_another_image_or_is_unhealthy() {
        // The deploy fails: the events say why.
        let (s, _, _) = upgrader(
            vec![
                Ok(CREATED.into()),
                Ok("{}".into()),
                Ok("{}".into()),
                Ok(r#"{"status":"deploying"}"#.into()),
                Ok(r#"[{"type":"deploy_failed","message":"image not found"}]"#.into()),
            ],
            Ok(()),
        );
        let e = s.upgrade("acme", "service_9", IMG, NOOP).unwrap_err();
        assert!(
            e.contains("did not start") && e.contains("image not found"),
            "{e}"
        );
        // Live, but on the old image.
        let (s, _, h) = upgrader(
            vec![
                Ok(CREATED.into()),
                Ok("{}".into()),
                Ok("{}".into()),
                Ok(LIVE.into()),
                Ok(r#"{"status":"live","image":"ghcr.io/spacecorps/obop:old"}"#.into()),
            ],
            Ok(()),
        );
        let e = s.upgrade("acme", "service_9", IMG, NOOP).unwrap_err();
        assert!(e.contains("runs ghcr.io/spacecorps/obop:old"), "{e}");
        assert!(
            h.0.lock().unwrap().is_empty(),
            "no health check on the wrong version"
        );
        // Live on the new image but the health check fails.
        let live = format!(r#"{{"status":"live","image":"{IMG}"}}"#);
        let (s, _, _) = upgrader(
            vec![
                Ok(CREATED.into()),
                Ok("{}".into()),
                Ok("{}".into()),
                Ok(LIVE.into()),
                Ok(live),
            ],
            Err("did not answer 200".into()),
        );
        assert_eq!(
            s.upgrade("acme", "service_9", IMG, NOOP).unwrap_err(),
            "did not answer 200"
        );
    }

    #[test]
    fn instances_created_from_an_image_report_it_and_start_with_auto_migrate() {
        let (runner, cell) = (
            Arc::new(FakeRunner::default()),
            Arc::new(FakeCell::default()),
        );
        runner
            .reply
            .lock()
            .unwrap()
            .extend([Ok(CREATED.into()), Ok(LIVE.into())]);
        let mut c = cfg(Source::Image("ghcr.io/spacecorps/obop:old".into()));
        c.registry_credential = Some("cred_1".into());
        let s = Sliplane::new(c, runner.clone(), cell);
        assert_eq!(s.image().as_deref(), Some("ghcr.io/spacecorps/obop:old"));
        s.create(&spec(), NOOP).unwrap();
        let args = runner.calls.lock().unwrap()[0].join(" ");
        assert!(args.contains("--env OBOP_AUTO_MIGRATE=1"), "{args}");
        assert!(args.contains("--registry-credential-id cred_1"), "{args}");
        // A repository source has no image.
        let repo = Sliplane::new(
            cfg(Source::Repo {
                url: "https://x/y".into(),
                branch: "deploy".into(),
            }),
            Arc::new(FakeRunner::default()),
            Arc::new(FakeCell::default()),
        );
        assert_eq!(repo.image(), None);
    }
}
