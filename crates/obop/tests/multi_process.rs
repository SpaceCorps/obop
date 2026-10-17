//! Two real `obop serve` processes on one PostgreSQL database (B1, D59): the proof that nothing
//! the server keeps in one process's memory decides an answer. Each test starts its own pair
//! against a scratch database; they are skipped unless `OBOP_TEST_POSTGRES_URL` names a server
//! (a throwaway `postgres:18` container is enough; docs/DEPLOY.md).

use serde_json::{Value as J, json};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const KEY: &str = "5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e";
const PASSWORD: &str = "correct horse battery staple";

fn pg_base() -> Option<String> {
    let u = std::env::var("OBOP_TEST_POSTGRES_URL").ok();
    if u.is_none() {
        eprintln!("skipped: set OBOP_TEST_POSTGRES_URL to run the multi-process tests");
    }
    u
}

/// A scratch database, dropped afterwards.
struct Scratch {
    admin: String,
    name: String,
    pub url: String,
}

impl Scratch {
    fn new(base: &str) -> Scratch {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let name = format!(
            "obop_mp_{}_{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let mut c = postgres::Client::connect(base, postgres::NoTls).expect("connect to postgres");
        c.batch_execute(&format!("CREATE DATABASE {name}")).unwrap();
        let (head, _) = base.rsplit_once('/').unwrap();
        Scratch {
            admin: base.into(),
            url: format!("{head}/{name}"),
            name,
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if let Ok(mut c) = postgres::Client::connect(&self.admin, postgres::NoTls) {
            let _ = c.batch_execute(&format!("DROP DATABASE IF EXISTS {} WITH (FORCE)", self.name));
        }
    }
}

struct Server {
    child: Child,
    port: u16,
    log: std::path::PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.log);
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn obop() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_obop"));
    c.env("OBOP_SECRET_KEY", KEY)
        .env("OBOP_REQUIRE_MFA_FOR_ADMINS", "0")
        .env_remove("OBOP_DATABASE_URL")
        .env_remove("OBOP_DEV_INSECURE_KEY")
        .env_remove("OBOP_SECRET_KEY_PREVIOUS");
    c
}

fn start(db: &str, env: &[(&str, &str)]) -> Server {
    start_cmd("serve", db, env)
}

/// `serve` (an instance) or `platform serve --provisioner fake` (the control plane).
fn start_cmd(kind: &str, db: &str, env: &[(&str, &str)]) -> Server {
    let port = free_port();
    let log = std::env::temp_dir().join(format!("obop-mp-{}-{port}.log", std::process::id()));
    let mut cmd = obop();
    if kind == "platform" {
        cmd.args(["platform", "serve", "--provisioner", "fake"]);
    } else {
        cmd.arg("serve");
    }
    cmd.args(["--addr", &format!("127.0.0.1:{port}"), "--db", db])
        .envs(env.iter().copied())
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(&log).unwrap());
    let child = cmd.spawn().expect("spawn obop serve");
    let s = Server { child, port, log };
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return s;
        }
        assert!(
            Instant::now() < deadline,
            "obop serve did not come up: {}",
            std::fs::read_to_string(&s.log).unwrap_or_default()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Create the first administrator with the CLI, as an operator would.
fn create_admin(db: &str, name: &str) {
    let mut c = obop()
        .args(["admin", "create", name, "--all-groups", "--db", db])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(format!("{PASSWORD}\n").as_bytes()).unwrap();
    assert!(c.wait().unwrap().success(), "admin create");
}

struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl Reply {
    fn json(&self) -> J {
        serde_json::from_str(&self.body).unwrap_or(J::Null)
    }
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// A plain HTTP/1.1 request on a fresh connection (the server closes it after the answer).
fn http(port: u16, method: &str, path: &str, token: Option<&str>, extra: &[(&str, &str)], body: Option<&J>) -> Reply {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
    let payload = body.map(J::to_string).unwrap_or_default();
    let mut req = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nAccept: application/json\r\n");
    if let Some(t) = token {
        req.push_str(&format!("Authorization: Bearer {t}\r\n"));
    }
    for (k, v) in extra {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    if body.is_some() {
        req.push_str(&format!("Content-Type: application/json\r\nContent-Length: {}\r\n", payload.len()));
    } else if method == "POST" {
        req.push_str("Content-Length: 0\r\n");
    }
    req.push_str("\r\n");
    req.push_str(&payload);
    s.write_all(req.as_bytes()).unwrap();
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).unwrap();
    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect::<Vec<_>>();
    // Chunked bodies: join the chunks.
    let body = if headers.iter().any(|(k, v)| k.eq_ignore_ascii_case("transfer-encoding") && v.contains("chunked")) {
        let mut out = String::new();
        let mut rest = body;
        while let Some((size, tail)) = rest.split_once("\r\n") {
            let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
            if n == 0 || tail.len() < n {
                break;
            }
            out.push_str(&tail[..n]);
            rest = tail[n..].trim_start_matches("\r\n");
        }
        out
    } else {
        body.to_string()
    };
    Reply { status, headers, body }
}

fn login(port: u16, user: &str, password: &str) -> Reply {
    http(port, "POST", "/api/auth/login", None, &[], Some(&json!({"username": user, "password": password})))
}

fn token_of(r: &Reply) -> String {
    r.json()["token"].as_str().unwrap_or_else(|| panic!("no token in {} {}", r.status, r.body)).to_string()
}

struct Pair {
    a: Server,
    b: Server,
    _db: Scratch,
}

fn pair(env: &[(&str, &str)]) -> Option<Pair> {
    let base = pg_base()?;
    let db = Scratch::new(&base);
    create_admin(&db.url, "ada");
    let a = start(&db.url, env);
    let b = start(&db.url, env);
    Some(Pair { a, b, _db: db })
}

#[test]
fn a_session_started_on_one_process_works_on_the_other_and_data_is_shared() {
    let Some(p) = pair(&[]) else { return };
    let t = token_of(&login(p.a.port, "ada", PASSWORD));
    for port in [p.a.port, p.b.port] {
        let r = http(port, "GET", "/api/meta", Some(&t), &[], None);
        assert_eq!(r.status, 200, "meta on {port}: {}", r.body);
    }
    // Wrong tokens are refused by both.
    assert_eq!(http(p.b.port, "GET", "/api/meta", Some("obop_nope"), &[], None).status, 401);
    // Data written through one process is read through the other.
    let made = http(p.a.port, "POST", "/api/party/create", Some(&t), &[], Some(&json!({"name": "Acme Ltd"})));
    assert!(made.status == 200, "{} {}", made.status, made.body);
    let id = made.json()["value"]["id"].as_str().unwrap().to_string();
    let got = http(p.b.port, "POST", "/api/party/get", Some(&t), &[], Some(&json!({"id": id})));
    assert_eq!(got.json()["value"]["name"], "Acme Ltd", "{}", got.body);
}

#[test]
fn an_mcp_session_opened_on_one_process_continues_on_the_other() {
    let Some(p) = pair(&[]) else { return };
    let t = token_of(&login(p.a.port, "ada", PASSWORD));
    let init = http(
        p.a.port, "POST", "/mcp", Some(&t), &[("Accept", "application/json, text/event-stream")],
        Some(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}})),
    );
    assert_eq!(init.status, 200, "{}", init.body);
    let sid = init.header("mcp-session-id").expect("session id").to_string();
    // The next calls go to the other process, with the same session id.
    let note = http(
        p.b.port, "POST", "/mcp", Some(&t), &[("Mcp-Session-Id", &sid), ("Mcp-Protocol-Version", "2025-06-18")],
        Some(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"})),
    );
    assert_eq!(note.status, 202, "{}", note.body);
    let list = http(
        p.b.port, "POST", "/mcp", Some(&t), &[("Mcp-Session-Id", &sid), ("Mcp-Protocol-Version", "2025-06-18")],
        Some(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})),
    );
    assert_eq!(list.status, 200, "{}", list.body);
    assert!(list.json()["result"]["tools"].as_array().is_some_and(|t| !t.is_empty()), "{}", list.body);
    // A session belongs to the token that opened it, on either process.
    let other = token_of(&login(p.b.port, "ada", PASSWORD));
    let stolen = http(
        p.a.port, "POST", "/mcp", Some(&other), &[("Mcp-Session-Id", &sid)],
        Some(&json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list"})),
    );
    assert_eq!(stolen.status, 404);
    // Closing on one process ends it on the other.
    assert_eq!(http(p.b.port, "DELETE", "/mcp", Some(&t), &[("Mcp-Session-Id", &sid)], None).status, 204);
    let gone = http(
        p.a.port, "POST", "/mcp", Some(&t), &[("Mcp-Session-Id", &sid)],
        Some(&json!({"jsonrpc": "2.0", "id": 4, "method": "tools/list"})),
    );
    assert_eq!(gone.status, 404);
}

#[test]
fn a_plan_an_agent_proposed_on_one_process_is_approved_on_the_other() {
    let Some(p) = pair(&[]) else { return };
    let t = token_of(&login(p.a.port, "ada", PASSWORD));
    let made = http(p.a.port, "POST", "/api/party/create", Some(&t), &[], Some(&json!({"name": "To delete"})));
    let id = made.json()["value"]["id"].as_str().unwrap().to_string();
    // An agent token with the owner's groups, issued on B.
    let me = http(p.b.port, "GET", "/api/meta", Some(&t), &[], None);
    assert_eq!(me.status, 200);
    let scopes: Vec<String> = ["base.user", "platform.admin"].iter().map(|s| s.to_string()).collect();
    let agent = http(p.b.port, "POST", "/api/auth/agent-token", Some(&t), &[], Some(&json!({"scopes": scopes, "label": "bot"})));
    let agent = token_of(&agent);
    // The agent proposes a delete on A: it needs a human.
    let proposed = http(p.a.port, "POST", "/api/party/delete", Some(&agent), &[], Some(&json!({"id": id})));
    assert_eq!(proposed.status, 202, "{}", proposed.body);
    let plan = proposed.json()["plan_id"].as_str().unwrap().to_string();
    // Nothing happened yet, on either process.
    for port in [p.a.port, p.b.port] {
        let g = http(port, "POST", "/api/party/get", Some(&t), &[], Some(&json!({"id": id})));
        assert_eq!(g.status, 200, "still there on {port}");
    }
    // The agent cannot approve its own plan; a person approves it on B.
    assert_ne!(http(p.b.port, "POST", &format!("/api/approve/{plan}"), Some(&agent), &[], None).status, 200);
    let done = http(p.b.port, "POST", &format!("/api/approve/{plan}"), Some(&t), &[], None);
    assert_eq!(done.status, 200, "{}", done.body);
    // Gone on both, and the plan cannot be approved twice anywhere.
    for port in [p.a.port, p.b.port] {
        let g = http(port, "POST", "/api/party/get", Some(&t), &[], Some(&json!({"id": id})));
        assert_ne!(g.status, 200, "deleted, seen on {port}: {}", g.body);
        assert_eq!(http(port, "POST", &format!("/api/approve/{plan}"), Some(&t), &[], None).status, 404);
    }
}

#[test]
fn parallel_writes_through_both_processes_all_land_exactly_once() {
    let Some(p) = pair(&[]) else { return };
    let t = token_of(&login(p.a.port, "ada", PASSWORD));
    let (pa, pb) = (p.a.port, p.b.port);
    let ids: Vec<String> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..8)
            .map(|i| {
                let t = t.clone();
                s.spawn(move || {
                    (0..6)
                        .map(|n| {
                            let port = if (i + n) % 2 == 0 { pa } else { pb };
                            let r = http(port, "POST", "/api/party/create", Some(&t), &[],
                                Some(&json!({"name": format!("Party {i}-{n}")})));
                            assert_eq!(r.status, 200, "{}", r.body);
                            r.json()["value"]["id"].as_str().unwrap().to_string()
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(ids.len(), 48);
    assert_eq!(unique.len(), 48, "no id twice");
    for port in [pa, pb] {
        let list = http(port, "POST", "/api/party/list", Some(&t), &[], Some(&json!({})));
        let rows = list.json()["value"].as_array().cloned().unwrap_or_default();
        assert_eq!(rows.len(), 48, "{port} sees all of them");
    }
}

#[test]
fn rate_limits_and_the_login_lockout_are_counted_across_processes() {
    let Some(p) = pair(&[("OBOP_MCP_RATE_PER_MINUTE", "10")]) else { return };
    let t = token_of(&login(p.a.port, "ada", PASSWORD));
    // 10 requests a minute per token for the pair: 6 + 6 must hit the limit on one of them.
    let mut ok = 0;
    let mut limited = 0;
    for i in 0..12 {
        let port = if i % 2 == 0 { p.a.port } else { p.b.port };
        let r = http(port, "POST", "/mcp", Some(&t), &[("Accept", "application/json")],
            Some(&json!({"jsonrpc": "2.0", "id": i, "method": "ping"})));
        match r.status {
            429 => {
                assert!(r.header("retry-after").is_some());
                limited += 1;
            }
            _ => ok += 1,
        }
    }
    assert_eq!(ok, 10, "exactly the allowance, shared: {ok} ok, {limited} limited");
    assert_eq!(limited, 2);
    // Five wrong passwords, split over the two processes, lock the account on both.
    for i in 0..5 {
        let port = if i % 2 == 0 { p.a.port } else { p.b.port };
        assert_eq!(login(port, "ada", "not the password").status, 401);
    }
    for port in [p.a.port, p.b.port] {
        let r = login(port, "ada", PASSWORD);
        assert_eq!(r.status, 429, "locked on {port}: {}", r.body);
    }
}

#[test]
fn a_database_that_needs_migration_is_refused_by_postgres_servers_until_migrated() {
    let Some(base) = pg_base() else { return };
    let db = Scratch::new(&base);
    create_admin(&db.url, "ada");
    // Make the database look like one from before schema versions.
    let mut c = postgres::Client::connect(&db.url, postgres::NoTls).unwrap();
    c.batch_execute("DELETE FROM obop_record WHERE model = 'obop_meta'").unwrap();
    let out = obop().args(["serve", "--addr", "127.0.0.1:0", "--db", &db.url]).output().unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("obop migrate run --db") && err.contains("--auto-migrate"), "{err}");
    assert!(!err.contains("obop-test"), "the password is not echoed: {err}");
    let st = obop().args(["migrate", "status", "--db", &db.url]).output().unwrap();
    assert!(String::from_utf8_lossy(&st.stdout).contains("pending"));
    let run = obop().args(["migrate", "run", "--db", &db.url]).output().unwrap();
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let s = start(&db.url, &[]);
    assert_eq!(http(s.port, "GET", "/api/openapi.json", None, &[], None).status, 200);
}

#[test]
fn two_control_plane_processes_share_accounts_the_forum_and_the_status_page() {
    let Some(base) = pg_base() else { return };
    let db = Scratch::new(&base);
    let a = start_cmd("platform", &db.url, &[("OBOP_STATUS_INTERVAL_SECS", "5")]);
    let b = start_cmd("platform", &db.url, &[("OBOP_STATUS_INTERVAL_SECS", "5")]);
    // Sign up on A; the session and the account work on B.
    let up = http(a.port, "POST", "/api/platform/signup", None, &[],
        Some(&json!({"email": "owner@example.test", "password": PASSWORD})));
    assert_eq!(up.status, 200, "{}", up.body);
    let t = token_of(&up);
    let me = http(b.port, "GET", "/api/platform/me", Some(&t), &[], None);
    assert_eq!(me.json()["email"], "owner@example.test", "{}", me.body);
    let again = http(b.port, "POST", "/api/platform/login", None, &[],
        Some(&json!({"email": "owner@example.test", "password": PASSWORD})));
    assert_eq!(again.status, 200);
    // An instance created through B is listed by A.
    let made = http(b.port, "POST", "/api/platform/instances", Some(&t), &[], Some(&json!({"name": "Acme"})));
    assert_eq!(made.status, 202, "{}", made.body);
    let listed = http(a.port, "GET", "/api/platform/instances", Some(&t), &[], None);
    assert_eq!(listed.json()["instances"].as_array().map(Vec::len), Some(1), "{}", listed.body);
    // The forum and the status page answer the same on both; one of them ran the checks.
    for port in [a.port, b.port] {
        assert_eq!(http(port, "GET", "/community", None, &[("Accept", "text/html")], None).status, 200);
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let sa = http(a.port, "GET", "/status/api/v2/summary.json", None, &[], None);
        let sb = http(b.port, "GET", "/status/api/v2/summary.json", None, &[], None);
        let comps = |r: &Reply| r.json()["components"].as_array().map(Vec::len).unwrap_or(0);
        if sa.status == 200 && comps(&sa) > 0 && comps(&sb) == comps(&sa) {
            break;
        }
        assert!(Instant::now() < deadline, "status never agreed: {} / {}", sa.body, sb.body);
        std::thread::sleep(Duration::from_millis(500));
    }
}
