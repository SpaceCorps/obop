//! Provisioning on Sliplane through its CLI: one database per business (D6) on a shared cell
//! PostgreSQL, one service per instance. The CLI is the only thing that talks to Sliplane, so
//! it is behind a trait and the whole flow is tested without a network.

use crate::provision::{InstanceSpec, Provisioned, Provisioner};
use obop_auth::Auth;
use serde_json::Value as J;
use std::sync::Arc;

/// Runs `sliplane` with the given arguments and returns stdout.
pub trait Runner: Send + Sync {
    fn run(&self, args: &[String]) -> Result<String, String>;
}

pub struct Cli;

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
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }
}

/// The shared PostgreSQL cluster that holds one database per business.
pub trait CellDb: Send + Sync {
    /// Create role and database for `slug`; returns the instance's connection URL.
    fn create_database(&self, slug: &str) -> Result<String, String>;
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
    let host = host_and_db.split('/').next().unwrap_or("");
    Ok((host.to_string(), host_and_db.to_string()))
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

pub struct PgCell {
    pub admin_url: String,
}

impl CellDb for PgCell {
    fn create_database(&self, slug: &str) -> Result<String, String> {
        let name = ident(slug)?;
        let (host, _) = split_url(&self.admin_url)?;
        let pw = random_password();
        let mut c = postgres::Client::connect(&self.admin_url, postgres::NoTls)
            .map_err(|e| e.to_string())?;
        // `name` is restricted to [a-z0-9_] by `ident`, so interpolating it is safe.
        c.batch_execute(&format!("CREATE ROLE {name} LOGIN PASSWORD '{pw}'"))
            .map_err(|e| e.to_string())?;
        if let Err(e) = c.batch_execute(&format!("CREATE DATABASE {name} OWNER {name}")) {
            let _ = c.batch_execute(&format!("DROP ROLE IF EXISTS {name}"));
            return Err(e.to_string());
        }
        Ok(format!("postgres://{name}:{pw}@{host}/{name}"))
    }

    fn seed_admin(&self, url: &str, email: &str, hash: &str) -> Result<(), String> {
        let backend = obop_storage::open(url)?;
        Auth::new(backend).create_user_hashed(
            email,
            hash,
            &[obop_commands::ADMIN_GROUP.to_string()],
        )
    }

    fn drop_database(&self, slug: &str) -> Result<(), String> {
        let name = ident(slug)?;
        let mut c = postgres::Client::connect(&self.admin_url, postgres::NoTls)
            .map_err(|e| e.to_string())?;
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
}

pub struct Sliplane {
    cfg: Config,
    runner: Arc<dyn Runner>,
    cell: Arc<dyn CellDb>,
}

impl Sliplane {
    pub fn new(cfg: Config, runner: Arc<dyn Runner>, cell: Arc<dyn CellDb>) -> Self {
        Self { cfg, runner, cell }
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

impl Provisioner for Sliplane {
    fn create(&self, spec: &InstanceSpec) -> Result<Provisioned, String> {
        let db_url = self.cell.create_database(&spec.slug)?;
        let result = (|| {
            self.cell
                .seed_admin(&db_url, &spec.owner_email, &spec.password_hash)?;
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
                Source::Image(i) => args.extend(["--image".into(), i.clone()]),
                Source::Repo { url, branch } => args.extend([
                    "--repo".into(),
                    url.clone(),
                    "--branch".into(),
                    branch.clone(),
                ]),
            }
            args.extend(["--secret-env".into(), format!("OBOP_DATABASE_URL={db_url}")]);
            if let Some(m) = &self.cfg.marketplace_url {
                args.extend(["--env".into(), format!("OBOP_MARKETPLACE_URL={m}")]);
            }
            let out = self.runner.run(&args)?;
            let j: J = serde_json::from_str(&out)
                .map_err(|e| format!("unexpected sliplane output: {e}"))?;
            let id = j["id"]
                .as_str()
                .ok_or("sliplane returned no service id")?
                .to_string();
            let domain = j["network"]["managedDomain"]
                .as_str()
                .ok_or("sliplane returned no domain")?;
            Ok(Provisioned {
                external_id: id,
                url: format!("https://{domain}"),
            })
        })();
        if result.is_err() {
            // Do not leave a database behind for an instance that never started.
            let _ = self.cell.drop_database(&spec.slug);
        }
        result
    }

    fn delete(&self, slug: &str, external_id: &str) -> Result<(), String> {
        if !external_id.is_empty() {
            // Only ever delete a service this provisioner created: check the name first.
            let mut get = self.base(&["services", "get"]);
            get.extend(["--service-id".into(), external_id.into()]);
            let j: J = serde_json::from_str(&self.runner.run(&get)?).map_err(|e| e.to_string())?;
            if j["name"].as_str() != Some(&Self::service_name(slug)) {
                return Err(
                    "refusing to delete: the service name does not match this instance".into(),
                );
            }
            let mut del = self.base(&["services", "delete"]);
            del.extend(["--service-id".into(), external_id.into()]);
            self.runner.run(&del)?;
        }
        self.cell.drop_database(slug)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

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
        fn create_database(&self, slug: &str) -> Result<String, String> {
            self.log.lock().unwrap().push(format!("create {slug}"));
            Ok(format!("postgres://obop_{slug}:pw@db:5432/obop_{slug}"))
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
        }
    }

    fn spec() -> InstanceSpec {
        InstanceSpec {
            slug: "acme".into(),
            owner_email: "ada@example.test".into(),
            password_hash: "$argon2id$v=19$x".into(),
        }
    }

    const CREATED: &str = r#"{"id":"service_9","name":"obop-acme","network":{"managedDomain":"obop-acme.sliplane.app"}}"#;

    #[test]
    fn create_makes_the_database_then_the_service() {
        let (runner, cell) = (
            Arc::new(FakeRunner::default()),
            Arc::new(FakeCell::default()),
        );
        runner.reply.lock().unwrap().push(Ok(CREATED.into()));
        let s = Sliplane::new(
            cfg(Source::Image("ghcr.io/x/obop:1".into())),
            runner.clone(),
            cell.clone(),
        );
        let p = s.create(&spec()).unwrap();
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
            "--secret-env OBOP_DATABASE_URL=postgres://obop_acme:pw@db:5432/obop_acme",
        ] {
            assert!(args.contains(want), "{want} missing from {args}");
        }
        assert!(
            !args.contains("--env OBOP_DATABASE_URL"),
            "the database URL is a secret env, never a plain one"
        );
    }

    #[test]
    fn repo_source_and_marketplace_env() {
        let (runner, cell) = (
            Arc::new(FakeRunner::default()),
            Arc::new(FakeCell::default()),
        );
        runner.reply.lock().unwrap().push(Ok(CREATED.into()));
        let mut c = cfg(Source::Repo {
            url: "https://github.com/SpaceCorps/obop".into(),
            branch: "deploy".into(),
        });
        c.marketplace_url = Some("https://market.example.test".into());
        Sliplane::new(c, runner.clone(), cell)
            .create(&spec())
            .unwrap();
        let args = runner.calls.lock().unwrap()[0].join(" ");
        assert!(args.contains("--repo https://github.com/SpaceCorps/obop --branch deploy"));
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
            .create(&spec())
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
            s.delete("acme", "service_9")
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
        s.delete("acme", "service_9").unwrap();
        assert!(
            runner.calls.lock().unwrap()[2]
                .join(" ")
                .contains("services delete")
        );
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
        let cell = PgCell { admin_url: admin };
        let slug = format!("t{}", std::process::id());
        let url = cell.create_database(&slug).unwrap();
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
}
