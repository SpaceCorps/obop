//! TLS to PostgreSQL (D66). The URL tests and the refusal tests run everywhere. The tests that
//! need a TLS-enabled PostgreSQL start a throwaway `postgres:17` container with a certificate
//! made on the spot, and run only when `OBOP_TEST_DOCKER_TLS=1` asks for them (the way
//! `OBOP_TEST_POSTGRES_URL` asks for the tests against a PostgreSQL server); without it they skip,
//! with a message, and never call `docker`. They used to start whenever a `docker` binary was on
//! the path, and failed ("the TLS test database did not start") on a machine where Docker was
//! installed but not ready. Once asked for, they fail when Docker or the image is missing, so a
//! CI job that asks for them cannot lose the coverage unseen. The CI job `rust` sets the variable
//! (`.github/workflows/ci.yml`). The rule itself is tested without Docker, with a `docker` that
//! does not exist put in its place for the thread (`Gate`, at the end of this file).

use super::*;
use crate::tls::{PgConnector, RootCert, SslMode, parse_url};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;

// ---- URL parsing -------------------------------------------------------------------------

#[test]
fn a_url_without_sslmode_means_no_tls_and_is_passed_on_unchanged() {
    for url in [
        "postgres://u:p@h:5432/db",
        "postgresql://u@h/db?application_name=x&connect_timeout=3",
        "postgres://u:p%40w@h/db",
    ] {
        let s = parse_url(url).unwrap();
        assert_eq!(s.mode, SslMode::Disable, "{url}");
        assert_eq!(s.root_cert, None);
        assert_eq!(s.conn, url, "the connection string must not change");
    }
}

#[test]
fn every_documented_sslmode_is_read_and_removed_from_the_connection_string() {
    for (name, mode) in [
        ("disable", SslMode::Disable),
        ("prefer", SslMode::Prefer),
        ("require", SslMode::Require),
        ("verify-ca", SslMode::VerifyCa),
        ("verify-full", SslMode::VerifyFull),
    ] {
        let s = parse_url(&format!("postgres://u@h/db?sslmode={name}")).unwrap();
        assert_eq!(s.mode, mode);
        assert_eq!(s.conn, "postgres://u@h/db");
        assert_eq!(mode.as_str(), name);
    }
    let s = parse_url("postgres://u@h/db?application_name=x&sslmode=require&connect_timeout=3")
        .unwrap();
    assert_eq!(
        s.conn,
        "postgres://u@h/db?application_name=x&connect_timeout=3"
    );
}

#[test]
fn sslrootcert_is_a_path_or_system_and_needs_a_mode_that_uses_it() {
    let s =
        parse_url("postgres://u@h/db?sslmode=verify-full&sslrootcert=%2Fetc%2Fca%20x.pem").unwrap();
    assert_eq!(s.root_cert, Some(RootCert::File("/etc/ca x.pem".into())));
    assert_eq!(s.conn, "postgres://u@h/db");
    let s = parse_url("postgres://u@h/db?sslmode=verify-ca&sslrootcert=system").unwrap();
    assert_eq!(s.root_cert, Some(RootCert::Bundled));
    for url in [
        "postgres://u@h/db?sslrootcert=/ca.pem",
        "postgres://u@h/db?sslmode=disable&sslrootcert=/ca.pem",
        "postgres://u@h/db?sslmode=prefer&sslrootcert=/ca.pem",
    ] {
        let e = parse_url(url).unwrap_err();
        assert!(e.contains("sslrootcert needs sslmode"), "{url}: {e}");
    }
}

#[test]
fn unknown_duplicate_and_unsupported_ssl_options_are_refused_with_a_clear_message() {
    let e = parse_url("postgres://u@h/db?sslmode=allow").unwrap_err();
    assert!(
        e.contains("unsupported sslmode") && e.contains("verify-full"),
        "{e}"
    );
    assert!(parse_url("postgres://u@h/db?sslmode=yes").is_err());
    assert!(parse_url("postgres://u@h/db?sslmode=require&sslmode=disable").is_err());
    assert!(parse_url("postgres://u@h/db?sslmode=require&sslrootcert=/a&sslrootcert=/b").is_err());
    assert!(parse_url("postgres://u@h/db?sslmode=require&sslrootcert=").is_err());
    for k in ["sslcert", "sslkey", "sslcrl"] {
        let e = parse_url(&format!("postgres://u@h/db?sslmode=require&{k}=/x")).unwrap_err();
        assert!(e.contains("not supported"), "{e}");
    }
}

#[test]
fn errors_never_contain_the_password() {
    for url in [
        "postgres://u:hunter2secret@h/db?sslmode=allow",
        "postgres://u:hunter2secret@h/db?sslmode=verify-full&sslrootcert=/does/not/exist.pem",
        "postgres://u:hunter2secret@h:notaport/db",
    ] {
        let e = PgConnector::new(url).err().expect("must fail");
        assert!(!e.contains("hunter2secret"), "{e}");
    }
}

#[test]
fn a_missing_or_empty_root_certificate_file_is_refused_when_the_url_is_read() {
    let e =
        PgConnector::new("postgres://u@h/db?sslmode=verify-full&sslrootcert=/does/not/exist.pem")
            .err()
            .unwrap();
    assert!(e.contains("cannot read sslrootcert"), "{e}");
    let dir = scratch("emptyroot");
    let f = dir.path().join("empty.pem");
    std::fs::write(&f, "not a certificate\n").unwrap();
    let e = PgConnector::new(&format!(
        "postgres://u@h/db?sslmode=require&sslrootcert={}",
        f.display()
    ))
    .err()
    .unwrap();
    assert!(e.contains("holds no certificate"), "{e}");
}

#[test]
fn a_plain_url_builds_a_connector_without_tls() {
    let c = PgConnector::new("postgres://u:p@127.0.0.1:1/db").unwrap();
    assert_eq!(c.mode(), SslMode::Disable);
    let c = PgConnector::new("postgres://u:p@127.0.0.1:1/db?sslmode=verify-full").unwrap();
    assert_eq!(c.mode(), SslMode::VerifyFull);
}

// ---- refusal against a server that does not speak TLS -----------------------------------------

/// A server that answers the SSL request with `N` (no TLS) and then hangs up.
fn server_without_tls() -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { return };
            let mut buf = [0u8; 8];
            if s.read_exact(&mut buf).is_ok() {
                let _ = s.write_all(b"N");
            }
        }
    });
    format!("127.0.0.1:{port}")
}

#[test]
fn require_and_verify_modes_refuse_a_server_that_has_no_tls() {
    let addr = server_without_tls();
    for mode in ["require", "verify-ca", "verify-full"] {
        let c = PgConnector::new(&format!("postgres://u:p@{addr}/db?sslmode={mode}")).unwrap();
        let e = c.connect().err().expect("must be refused");
        assert!(
            e.to_lowercase().contains("tls") || e.to_lowercase().contains("ssl"),
            "{mode}: {e}"
        );
    }
}

// ---- against a real PostgreSQL with TLS -----------------------------------------------------

/// A directory of this test's own, removed again when the guard is dropped.
fn scratch(tag: &str) -> crate::testing::TempPath {
    crate::testing::TempPath::dir(&format!("obop-tls-{tag}"))
}

/// The variable that asks for the tests that start a container.
const DOCKER_TLS: &str = "OBOP_TEST_DOCKER_TLS";

/// Does the value of [`DOCKER_TLS`] ask for the container tests? `1` or `true`; anything else,
/// and no value, is "no".
fn docker_tls_requested(value: Option<&str>) -> bool {
    matches!(
        value.map(|v| v.trim().to_ascii_lowercase()).as_deref(),
        Some("1" | "true")
    )
}

thread_local! {
    /// How often this thread ran `docker`: the tests of the gate read it.
    static DOCKER_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// What the tests of the gate put in place of the machine: the value `TlsPg::start` reads for
    /// [`DOCKER_TLS`] (the outer `Option` says whether it is replaced at all) and the name of the
    /// `docker` binary. Per thread, so the tests that run beside them see the real ones.
    static GATE_VALUE: std::cell::RefCell<Option<Option<String>>> =
        const { std::cell::RefCell::new(None) };
    static DOCKER_BIN: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

/// The value of [`DOCKER_TLS`] as `TlsPg::start` sees it.
fn gate_value() -> Option<String> {
    match GATE_VALUE.with(|v| v.borrow().clone()) {
        Some(replaced) => replaced,
        None => std::env::var(DOCKER_TLS).ok(),
    }
}

/// The program `docker` stands for.
fn docker_bin() -> String {
    DOCKER_BIN
        .with(|b| b.borrow().clone())
        .unwrap_or_else(|| "docker".into())
}

/// Puts a value for [`DOCKER_TLS`] and a `docker` binary in place of the machine's for this
/// thread, and puts them back when dropped.
struct Gate;

impl Gate {
    fn with(value: Option<&str>, docker: &str) -> Gate {
        GATE_VALUE.with(|v| *v.borrow_mut() = Some(value.map(str::to_string)));
        DOCKER_BIN.with(|b| *b.borrow_mut() = Some(docker.to_string()));
        Gate
    }
}

impl Drop for Gate {
    fn drop(&mut self) {
        GATE_VALUE.with(|v| *v.borrow_mut() = None);
        DOCKER_BIN.with(|b| *b.borrow_mut() = None);
    }
}

/// A program that no machine has, for the tests that need Docker to be missing.
const NO_SUCH_DOCKER: &str = "obop-no-such-docker-binary";

fn docker(args: &[&str]) -> Option<String> {
    DOCKER_CALLS.with(|n| n.set(n.get() + 1));
    let o = Command::new(docker_bin()).args(args).output().ok()?;
    o.status
        .success()
        .then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

struct TlsPg {
    name: String,
    port: u16,
    /// The server certificate (it names `localhost` only), and an unrelated certificate.
    cert: std::path::PathBuf,
    other: std::path::PathBuf,
    /// The directory of the two certificates; last, so that it goes after the container.
    _certs: crate::testing::TempPath,
}

impl Drop for TlsPg {
    fn drop(&mut self) {
        let _ = Command::new(docker_bin())
            .args(["rm", "-f", "-v", &self.name])
            .output();
    }
}

const SETUP: &str = "set -e; mkdir -p /certs; cd /certs; \
    mk() { openssl req -x509 -newkey rsa:2048 -nodes -keyout $1.key -out $1.crt -days 2 \
      -subj /CN=localhost -addext subjectAltName=DNS:localhost \
      -addext basicConstraints=critical,CA:FALSE 2>/dev/null; }; \
    mk server; mk other; chown postgres server.key; chmod 600 server.key; \
    exec docker-entrypoint.sh postgres -c ssl=on \
      -c ssl_cert_file=/certs/server.crt -c ssl_key_file=/certs/server.key";

impl TlsPg {
    /// None (after saying why) unless `OBOP_TEST_DOCKER_TLS=1` asks for the container tests;
    /// once asked for, a missing Docker or image is a failure.
    fn start() -> Option<Self> {
        Self::start_if(docker_tls_requested(gate_value().as_deref()))
    }

    /// [`TlsPg::start`] with the answer of the gate given. `false` returns before anything calls
    /// `docker`.
    fn start_if(requested: bool) -> Option<Self> {
        if !requested {
            eprintln!(
                "skipped: set {DOCKER_TLS}=1 to run against a throwaway postgres:17 container (needs Docker)"
            );
            return None;
        }
        assert!(
            docker(&["info", "--format", "{{.ServerVersion}}"]).is_some(),
            "{DOCKER_TLS} is set but Docker is not available"
        );
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let name = format!(
            "obop-tls-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let _ = docker(&["rm", "-f", "-v", &name]);
        let started = docker(&[
            "run",
            "-d",
            "--name",
            &name,
            "-e",
            "POSTGRES_PASSWORD=obop-test",
            "-p",
            "127.0.0.1::5432",
            "--entrypoint",
            "sh",
            "postgres:17",
            "-c",
            SETUP,
        ]);
        assert!(
            started.is_some(),
            "{DOCKER_TLS} is set but postgres:17 could not be started"
        );
        let mut me = TlsPg {
            name: name.clone(),
            port: 0,
            cert: std::path::PathBuf::new(),
            other: std::path::PathBuf::new(),
            _certs: scratch("certs"),
        };
        let mapped =
            docker(&["port", &name, "5432/tcp"]).expect("the mapped port of the container");
        me.port = mapped
            .lines()
            .next()
            .and_then(|l| l.rsplit(':').next())
            .and_then(|p| p.parse().ok())
            .expect("a port number");
        let dir = me._certs.path().to_path_buf();
        for (f, slot) in [("server.crt", 0), ("other.crt", 1)] {
            let mut pem = None;
            for _ in 0..60 {
                // The file exists as soon as `openssl` opens it, before it holds the certificate:
                // wait for the whole PEM, not for a `cat` that succeeds on an empty file.
                pem = docker(&["exec", &name, "cat", &format!("/certs/{f}")])
                    .filter(|p| p.contains("END CERTIFICATE"));
                if pem.is_some() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            let p = dir.join(f);
            std::fs::write(&p, pem.expect("certificate generated")).unwrap();
            if slot == 0 { me.cert = p } else { me.other = p }
        }
        for _ in 0..120 {
            if PgConnector::new(&me.url("disable", "127.0.0.1", ""))
                .unwrap()
                .connect()
                .is_ok()
            {
                return Some(me);
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        panic!("the TLS test database did not start");
    }

    fn url(&self, mode: &str, host: &str, extra: &str) -> String {
        format!(
            "postgres://postgres:obop-test@{host}:{}/postgres?sslmode={mode}{extra}",
            self.port
        )
    }

    fn root(&self) -> String {
        format!("&sslrootcert={}", self.cert.display())
    }

    fn encrypted(&self, url: &str) -> Result<bool> {
        let mut c = PgConnector::new(url)?.connect()?;
        let row = c
            .query_one(
                "SELECT ssl FROM pg_stat_ssl WHERE pid = pg_backend_pid()",
                &[],
            )
            .map_err(|e| e.to_string())?;
        Ok(row.get(0))
    }
}

macro_rules! tls_pg {
    () => {
        match TlsPg::start() {
            Some(p) => p,
            None => return,
        }
    };
}

#[test]
fn a_plain_url_still_connects_and_is_not_encrypted() {
    let pg = tls_pg!();
    let plain = format!(
        "postgres://postgres:obop-test@127.0.0.1:{}/postgres",
        pg.port
    );
    assert_eq!(pg.encrypted(&plain), Ok(false));
    assert_eq!(pg.encrypted(&pg.url("disable", "127.0.0.1", "")), Ok(false));
}

#[test]
fn require_and_prefer_encrypt_the_connection_without_checking_the_certificate() {
    let pg = tls_pg!();
    assert_eq!(pg.encrypted(&pg.url("require", "127.0.0.1", "")), Ok(true));
    assert_eq!(pg.encrypted(&pg.url("prefer", "127.0.0.1", "")), Ok(true));
}

#[test]
fn verify_full_accepts_a_trusted_certificate_that_names_the_host() {
    let pg = tls_pg!();
    assert_eq!(
        pg.encrypted(&pg.url("verify-full", "localhost", &pg.root())),
        Ok(true)
    );
    assert_eq!(
        pg.encrypted(&pg.url("verify-ca", "localhost", &pg.root())),
        Ok(true)
    );
}

#[test]
fn verify_full_refuses_a_host_name_the_certificate_does_not_carry() {
    let pg = tls_pg!();
    let e = pg
        .encrypted(&pg.url("verify-full", "127.0.0.1", &pg.root()))
        .unwrap_err();
    assert!(e.contains("TLS"), "{e}");
}

#[test]
fn verify_ca_accepts_a_name_mismatch_but_verify_full_does_not() {
    let pg = tls_pg!();
    assert_eq!(
        pg.encrypted(&pg.url("verify-ca", "127.0.0.1", &pg.root())),
        Ok(true)
    );
    assert!(
        pg.encrypted(&pg.url("verify-full", "127.0.0.1", &pg.root()))
            .is_err()
    );
}

#[test]
fn verify_modes_refuse_a_certificate_signed_by_nobody_we_trust() {
    let pg = tls_pg!();
    // No root file: the bundled public roots do not know this certificate.
    assert!(
        pg.encrypted(&pg.url("verify-full", "localhost", ""))
            .is_err()
    );
    assert!(pg.encrypted(&pg.url("verify-ca", "localhost", "")).is_err());
    // An unrelated certificate as the only root.
    let other = format!("&sslrootcert={}", pg.other.display());
    assert!(
        pg.encrypted(&pg.url("verify-full", "localhost", &other))
            .is_err()
    );
    assert!(
        pg.encrypted(&pg.url("verify-ca", "localhost", &other))
            .is_err()
    );
}

#[test]
fn require_with_a_root_certificate_is_checked_like_verify_ca() {
    let pg = tls_pg!();
    let other = format!("&sslrootcert={}", pg.other.display());
    assert!(
        pg.encrypted(&pg.url("require", "127.0.0.1", &other))
            .is_err()
    );
    assert_eq!(
        pg.encrypted(&pg.url("require", "127.0.0.1", &pg.root())),
        Ok(true)
    );
}

#[test]
fn the_storage_backend_uses_tls_for_its_main_and_lock_connections() {
    let pg = tls_pg!();
    // Several containers start at once on a busy machine: allow a slow first TLS handshake.
    let patient = PgPoolConfig {
        acquire_timeout: std::time::Duration::from_secs(30),
        ..PgPoolConfig::default()
    };
    let b = open_with(
        &pg.url("verify-full", "localhost", &pg.root()),
        Some(patient),
    )
    .unwrap();
    let mut f = Fields::new();
    f.insert("n".into(), J::from(1));
    b.insert("tlsmodel", "a", &f).unwrap();
    assert_eq!(b.get("tlsmodel", "a").unwrap().unwrap()["n"], J::from(1));
    let guard = b.lock("tls-lock").unwrap();
    // The server sees only encrypted sessions from the backend.
    let mut c = PgConnector::new(&pg.url("require", "127.0.0.1", ""))
        .unwrap()
        .connect()
        .unwrap();
    let rows = c
        .query(
            "SELECT s.ssl FROM pg_stat_ssl s JOIN pg_stat_activity a USING (pid) \
             WHERE a.datname = 'postgres' AND a.pid <> pg_backend_pid() \
             AND a.backend_type = 'client backend'",
            &[],
        )
        .unwrap();
    assert!(rows.len() >= 2, "main and lock connections are open");
    assert!(
        rows.iter().all(|r| r.get::<_, bool>(0)),
        "all backend sessions use TLS"
    );
    drop(guard);
    let wrong = open(&pg.url("verify-full", "127.0.0.1", &pg.root()));
    assert!(
        wrong.is_err(),
        "opening is refused on a verification failure"
    );
}

#[test]
fn only_postgres_urls_are_accepted_so_no_ssl_key_can_be_ignored() {
    assert!(PgConnector::new("host=h sslmode=verify-full").is_err());
    assert!(PgConnector::new("mysql://u@h/db").is_err());
}

// ---- the gate of the container tests ----------------------------------------------------------

#[test]
fn only_one_or_true_asks_for_the_container_tests() {
    assert!(!docker_tls_requested(None));
    for no in ["", " ", "0", "false", "no", "off", "2", "yes"] {
        assert!(!docker_tls_requested(Some(no)), "{no:?}");
    }
    for yes in ["1", "true", "TRUE", " 1 "] {
        assert!(docker_tls_requested(Some(yes)), "{yes:?}");
    }
}

#[test]
fn without_the_variable_the_container_tests_skip_and_never_call_docker() {
    let before = DOCKER_CALLS.with(|n| n.get());
    assert!(TlsPg::start_if(docker_tls_requested(None)).is_none());
    assert!(TlsPg::start_if(false).is_none());
    assert_eq!(
        DOCKER_CALLS.with(|n| n.get()),
        before,
        "a skipped test must not run docker"
    );
}

// The rule the gate exists for: once the variable asks for the container tests, a machine
// without Docker fails them instead of skipping them. These run everywhere, because they put a
// missing `docker` in place of the machine's.

#[test]
#[should_panic(expected = "is set but Docker is not available")]
fn once_asked_for_a_missing_docker_fails_the_container_tests() {
    let _gate = Gate::with(Some("1"), NO_SUCH_DOCKER);
    let _ = TlsPg::start_if(true);
}

#[test]
#[should_panic(expected = "is set but Docker is not available")]
fn start_follows_the_variable_and_fails_without_docker_once_it_is_set() {
    let _gate = Gate::with(Some("1"), NO_SUCH_DOCKER);
    let _ = TlsPg::start();
}

#[test]
fn start_skips_without_calling_docker_when_the_variable_is_not_set_or_not_asking() {
    let before = DOCKER_CALLS.with(|n| n.get());
    for value in [None, Some(""), Some("0"), Some("no")] {
        let _gate = Gate::with(value, NO_SUCH_DOCKER);
        assert!(TlsPg::start().is_none(), "{value:?}");
    }
    assert_eq!(
        DOCKER_CALLS.with(|n| n.get()),
        before,
        "a skipped test must not run docker"
    );
}

#[test]
fn the_gate_doubles_are_put_back_when_the_guard_is_dropped() {
    {
        let _gate = Gate::with(Some("1"), NO_SUCH_DOCKER);
        assert_eq!(gate_value().as_deref(), Some("1"));
        assert_eq!(docker_bin(), NO_SUCH_DOCKER);
    }
    assert_eq!(docker_bin(), "docker");
    assert_eq!(gate_value(), std::env::var(DOCKER_TLS).ok());
}
