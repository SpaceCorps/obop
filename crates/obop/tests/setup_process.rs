//! The first-administrator set-up with the real binary (D96, PROPOSED): two `obop serve` processes
//! on one empty SQLite database each print a set-up code in their own log; a code that one printed
//! works against the other; the creation burns every code, both processes answer "closed" from then
//! on, and the code is in no answer and in no row of the database, only in the log that printed it.
//! `obop admin create` takes the same lock and writes its line to the security log.

use serde_json::{Value as J, json};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const PW: &str = "a long enough password 1";

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("obop-setup-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
    fn file(&self) -> PathBuf {
        self.0.join("setup.sqlite")
    }
    fn url(&self) -> String {
        format!("sqlite://{}", self.file().display())
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn obop(args: &[&str], env: &[(&str, &str)]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_obop"));
    c.env_clear();
    for keep in ["SystemRoot", "SYSTEMROOT", "PATH", "TEMP", "TMP", "HOME"] {
        if let Ok(v) = std::env::var(keep) {
            c.env(keep, v);
        }
    }
    c.envs(env.iter().copied()).args(args);
    c
}

struct Server {
    child: Child,
    port: u16,
    log: PathBuf,
}

impl Server {
    fn log(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    /// The code this server printed, read from its log.
    fn code(&self) -> String {
        let log = self.log();
        log.lines()
            .find_map(|l| l.strip_prefix("Set-up code: "))
            .unwrap_or_else(|| panic!("no set-up code in the log:\n{log}"))
            .trim()
            .to_string()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.log);
    }
}

fn get(port: u16, path: &str) -> (u16, String) {
    request(port, "GET", path, None)
}

fn post(port: u16, path: &str, body: &J) -> (u16, String) {
    request(port, "POST", path, Some(body))
}

fn request(port: u16, method: &str, path: &str, body: Option<&J>) -> (u16, String) {
    let Ok(mut s) = TcpStream::connect(("127.0.0.1", port)) else {
        return (0, String::new());
    };
    s.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
    let body = body.map(J::to_string).unwrap_or_default();
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    if s.write_all(req.as_bytes()).is_err() {
        return (0, String::new());
    }
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    let status = out
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (status, out)
}

fn json_of(raw: &str) -> J {
    let payload = raw.split_once("\r\n\r\n").map_or("", |(_, p)| p);
    match (payload.find('{'), payload.rfind('}')) {
        (Some(a), Some(b)) if a <= b => serde_json::from_str(&payload[a..=b]).unwrap_or(J::Null),
        _ => J::Null,
    }
}

fn serve(dir: &Dir, tag: &str, env: &[(&str, &str)]) -> Server {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let log = dir.0.join(format!("serve-{tag}-{port}.log"));
    let mut command = obop(
        &[
            "serve",
            "--addr",
            &format!("127.0.0.1:{port}"),
            "--db",
            &dir.url(),
        ],
        env,
    );
    command
        .stdout(Stdio::null())
        .stderr(Stdio::from(std::fs::File::create(&log).unwrap()));
    let child = command.spawn().expect("spawn obop serve");
    let mut s = Server { child, port, log };
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(status) = s.child.try_wait().unwrap() {
            panic!("obop serve exited ({status}):\n{}", s.log());
        }
        if get(port, "/healthz").0 == 200 {
            break;
        }
        assert!(Instant::now() < deadline, "no start:\n{}", s.log());
        std::thread::sleep(Duration::from_millis(100));
    }
    s
}

fn keyed() -> Vec<(&'static str, &'static str)> {
    vec![("OBOP_SECRET_KEY", KEY)]
}

/// Every row of the database as text, for the search for the code.
fn database_text(dir: &Dir) -> String {
    let mut out = String::new();
    let c = rusqlite::Connection::open(dir.file()).unwrap();
    let tables: Vec<String> = c
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    for t in tables {
        let mut st = c.prepare(&format!("SELECT * FROM \"{t}\"")).unwrap();
        let n = st.column_count();
        let mut rows = st.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            for i in 0..n {
                if let Ok(v) = row.get::<_, rusqlite::types::Value>(i) {
                    out.push_str(&format!("{v:?}\n"));
                }
            }
        }
    }
    out
}

#[test]
fn two_processes_on_one_empty_database_each_print_a_code_that_works_against_the_other() {
    let dir = Dir::new("two");
    let a = serve(&dir, "a", &keyed());
    let b = serve(&dir, "b", &keyed());
    let (code_a, code_b) = (a.code(), b.code());
    assert_ne!(code_a, code_b);
    assert!(
        a.log()
            .contains("No users yet. Open this instance in a browser")
    );
    assert!(
        a.log()
            .contains("From a shell: obop admin create NAME --all-groups")
    );
    for s in [&a, &b] {
        let (st, raw) = get(s.port, "/api/auth/setup");
        assert_eq!(st, 200, "{raw}");
        assert_eq!(json_of(&raw)["open"], true);
        assert_eq!(json_of(&raw)["code_active"], true);
        assert!(!raw.contains(&code_a[..5]) && !raw.contains(&code_b[..5]));
    }
    // The code process A printed, sent to process B.
    let (st, raw) = post(
        b.port,
        "/api/auth/setup",
        &json!({"code": code_a, "username": "root", "password": PW, "instance_name": "Acme Test"}),
    );
    assert_eq!(st, 200, "{raw}");
    assert_eq!(json_of(&raw)["mfa_enrolment_required"], true);
    assert!(!raw.contains(&code_a[..5]));
    // Closed for good on both, with either code.
    for (s, c) in [(&a, &code_b), (&b, &code_a)] {
        assert_eq!(
            json_of(&get(s.port, "/api/auth/setup").1),
            json!({"open": false})
        );
        let (st, raw) = post(
            s.port,
            "/api/auth/setup",
            &json!({"code": c, "username": "late", "password": PW}),
        );
        assert_eq!(st, 409, "{raw}");
    }
    // Nothing in the database holds a code; the log holds the three kinds of lines.
    let text = database_text(&dir);
    for c in [&code_a, &code_b] {
        assert!(!text.contains(c.as_str()));
        assert!(!text.contains(&c.replace('-', "")));
    }
    assert!(text.contains("setup_open"));
    assert!(text.contains("setup_complete"));
    // A restart with an account prints no code.
    drop(a);
    let c = serve(&dir, "c", &keyed());
    assert!(!c.log().contains("Set-up code:"), "{}", c.log());
}

#[test]
fn without_a_server_key_no_code_is_printed_and_the_page_says_why() {
    let dir = Dir::new("nokey");
    let s = serve(&dir, "a", &[]);
    assert!(!s.log().contains("Set-up code:"));
    assert!(
        s.log().contains("OBOP_SECRET_KEY is not set"),
        "{}",
        s.log()
    );
    let (_, raw) = get(s.port, "/api/auth/setup");
    assert_eq!(json_of(&raw)["needs_secret_key"], true);
}

#[test]
fn an_account_made_at_the_terminal_closes_the_page_and_is_logged() {
    let dir = Dir::new("cli");
    let s = serve(&dir, "a", &keyed());
    let code = s.code();
    let mut c = obop(
        &["admin", "create", "op", "--all-groups", "--db", &dir.url()],
        &keyed(),
    );
    c.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = c.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{PW}\n").as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("created op with groups: "));
    let (st, raw) = post(
        s.port,
        "/api/auth/setup",
        &json!({"code": code, "username": "root", "password": PW}),
    );
    assert_eq!(st, 409, "{raw}");
    let text = database_text(&dir);
    assert!(text.contains("user_create"));
    assert!(text.contains("the first account was created from the command line"));
}

#[test]
fn under_dev_auth_there_is_no_code_and_no_route() {
    let dir = Dir::new("dev");
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let log = dir.0.join("dev.log");
    let child = obop(
        &[
            "serve",
            "--dev-auth",
            "--addr",
            &format!("127.0.0.1:{port}"),
            "--db",
            &dir.url(),
        ],
        &keyed(),
    )
    .stdout(Stdio::null())
    .stderr(Stdio::from(std::fs::File::create(&log).unwrap()))
    .spawn()
    .unwrap();
    let s = Server { child, port, log };
    let deadline = Instant::now() + Duration::from_secs(60);
    while get(port, "/healthz").0 != 200 {
        assert!(Instant::now() < deadline, "{}", s.log());
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(!s.log().contains("Set-up code:"));
    assert_eq!(get(port, "/api/auth/setup").0, 404);
}
