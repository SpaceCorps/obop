//! The enrolment texts with the real binary and a real socket (docs/spec/branding.md, slice S1e,
//! sections 4 and 5): a running `obop serve` over a SQLite file writes the name of the instance
//! brand into the label and `issuer=` of a new authenticator entry and into the relying party name
//! of the passkey registration options, reads it from the row at each enrolment (a change made at
//! the terminal applies to the very next one, with no wait and no restart), lets `OBOP_MFA_ISSUER`
//! win for the authenticator entry only, and refuses both enrolments with a 503 `brand_unavailable`
//! (no `Retry-After`) when the stored name cannot be shown, until it is repaired at the terminal.
//!
//! This is also the test that holds the wiring of `OBOP_MFA_ISSUER` into the server's
//! `AuthConfig::from_env` (`brand::env_issuer`), which no unit test can: the workspace forbids
//! `unsafe`, so no test can set a variable of its own process.

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
        let d = std::env::temp_dir().join(format!("obop-brand-enrol-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
    fn url(&self) -> String {
        format!("sqlite://{}", self.0.join("enrol.sqlite").display())
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The binary with the settings of a test and nothing else in its environment.
fn obop(args: &[&str], env: &[(&str, &str)]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_obop"));
    c.env_clear();
    for keep in ["SystemRoot", "SYSTEMROOT", "PATH", "TEMP", "TMP", "HOME"] {
        if let Ok(v) = std::env::var(keep) {
            c.env(keep, v);
        }
    }
    c.env("OBOP_SECRET_KEY", KEY)
        .env("OBOP_REQUIRE_MFA_FOR_ADMINS", "0")
        .env("OBOP_PUBLIC_URL", "https://inst.test")
        .envs(env.iter().copied())
        .args(args);
    c
}

/// A command of the binary that must succeed, with this on its standard input.
fn cli(url: &str, args: &[&str], stdin: Option<&str>) {
    let mut c = obop(args, &[]);
    c.args(["--db", url])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = c.spawn().unwrap();
    let mut pipe = child.stdin.take().unwrap();
    if let Some(s) = stdin {
        pipe.write_all(s.as_bytes()).unwrap();
    }
    drop(pipe);
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

struct Server {
    child: Child,
    port: u16,
    /// What the server wrote to its standard error, kept for the message of a failure. A server
    /// whose standard error is a pipe nobody reads ([`serve_with_closed_stderr`]) has no file.
    log: PathBuf,
}

impl Server {
    fn log(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    /// What a server that already exited wrote to a standard error that is a pipe, for the message
    /// of a failure to start. Only for a child that is gone: the read ends at the end of the pipe.
    fn log_of_the_dead(&mut self) -> String {
        let mut text = self.log();
        if let Some(mut pipe) = self.child.stderr.take() {
            let _ = pipe.read_to_string(&mut text);
        }
        text
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.log);
    }
}

/// The status line of `GET path` on this port, or 0 when nothing answered.
fn get_status(port: u16, path: &str) -> u16 {
    let Ok(mut s) = TcpStream::connect(("127.0.0.1", port)) else {
        return 0;
    };
    let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
    let req = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    if s.write_all(req.as_bytes()).is_err() {
        return 0;
    }
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    out.split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// Starts `obop serve` on a free port and returns once it answers. The port is found by binding and
/// dropping, so another process can take it in the gap; therefore a listener is not taken for the
/// server: the child must still be running while it listens, it must answer `/healthz`, and it must
/// still be running after that (a child that could not bind exits at once). A child that died is
/// reported with what it wrote to its standard error.
fn serve(url: &str, env: &[(&str, &str)]) -> Server {
    serve_with(url, env, false)
}

/// The same server, but its standard error is a pipe whose read end this test closes as soon as the
/// server answers: from then on every write of the server to its standard error fails, as it does
/// when a log collector goes away. A line written with `eprintln!` then panics, and the panic of a
/// handler is a dropped connection instead of the answer the line is about.
fn serve_with_closed_stderr(url: &str, env: &[(&str, &str)]) -> Server {
    serve_with(url, env, true)
}

fn serve_with(url: &str, env: &[(&str, &str)], closed_stderr: bool) -> Server {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let log = std::env::temp_dir().join(format!(
        "obop-brand-enrol-serve-{}-{port}.log",
        std::process::id()
    ));
    let mut command = obop(
        &["serve", "--addr", &format!("127.0.0.1:{port}"), "--db", url],
        env,
    );
    command.stdout(Stdio::null());
    if closed_stderr {
        command.stderr(Stdio::piped());
    } else {
        command.stderr(Stdio::from(std::fs::File::create(&log).unwrap()));
    }
    let child = command.spawn().expect("spawn obop serve");
    let mut s = Server { child, port, log };
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(status) = s.child.try_wait().unwrap() {
            panic!(
                "obop serve exited ({status}) before it came up:\n{}",
                s.log_of_the_dead()
            );
        }
        if get_status(port, "/healthz") == 200 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "obop serve did not come up:\n{}",
            s.log()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    if let Some(status) = s.child.try_wait().unwrap() {
        panic!(
            "something else answered on port {port}; obop serve exited ({status}):\n{}",
            s.log_of_the_dead()
        );
    }
    // Close the read end: what the server wrote while it started sits in the pipe and is thrown
    // away with it.
    drop(s.child.stderr.take());
    s
}

/// What a POST with a JSON body answered.
struct Answer {
    status: u16,
    head: String,
    body: J,
}

impl Answer {
    fn has_header(&self, name: &str) -> bool {
        self.head.lines().skip(1).any(|l| {
            l.to_ascii_lowercase()
                .starts_with(&format!("{}:", name.to_ascii_lowercase()))
        })
    }
}

fn post(port: u16, path: &str, token: Option<&str>, body: &J) -> Answer {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
    let body = body.to_string();
    let mut req = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
        body.len()
    );
    if let Some(t) = token {
        req.push_str(&format!("Authorization: Bearer {t}\r\n"));
    }
    req.push_str("\r\n");
    req.push_str(&body);
    s.write_all(req.as_bytes()).unwrap();
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    let (head, payload) = out.split_once("\r\n\r\n").unwrap_or((&out, ""));
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    // The body may be chunked; the JSON is the text from the first `{` to the last `}`.
    let json_text = match (payload.find('{'), payload.rfind('}')) {
        (Some(a), Some(b)) if a <= b => &payload[a..=b],
        _ => "null",
    };
    Answer {
        status,
        head: head.to_string(),
        body: serde_json::from_str(json_text).unwrap_or(J::Null),
    }
}

fn login(port: u16) -> String {
    let a = post(
        port,
        "/api/auth/login",
        None,
        &json!({"username": "ada", "password": PW}),
    );
    assert_eq!(a.status, 200, "{}", a.body);
    a.body["token"].as_str().unwrap().to_string()
}

/// The answer of a TOTP enrolment.
fn begin(port: u16, token: &str) -> Answer {
    post(port, "/api/auth/mfa/begin", Some(token), &json!({}))
}

fn options(port: u16, token: &str) -> Answer {
    post(
        port,
        "/api/auth/passkeys/register/options",
        Some(token),
        &json!({"password": PW}),
    )
}

fn is_brand_unavailable(a: &Answer, what: &str) {
    assert_eq!(a.status, 503, "{what}: {}", a.body);
    assert_eq!(a.body["code"], json!("brand_unavailable"), "{what}");
    assert!(!a.has_header("retry-after"), "{what}: {}", a.head);
    assert!(
        a.body["hint"]
            .as_str()
            .unwrap()
            .contains("obop admin brand status"),
        "{what}: {}",
        a.body
    );
}

/// The row of the brand holds a name that cannot be shown (written around the command: a restore,
/// an import, a direct edit).
fn store_a_name_that_cannot_be_shown(url: &str) {
    let backend = obop_storage::open(url).unwrap();
    let mut row = obop_storage::Fields::new();
    row.insert("name".into(), json!("a<b"));
    backend
        .apply_checked(&[obop_storage::Op::Put {
            model: "instance_brand".into(),
            id: "instance".into(),
            fields: row,
        }])
        .unwrap();
}

/// The row of the brand is there and cannot be decoded (a store that is up and cannot answer this
/// read; here a row that holds a number where a record belongs). Written around the store: a write
/// through it reads the row first.
fn store_a_row_that_cannot_be_decoded(dir: &Dir) {
    let conn = rusqlite::Connection::open(dir.0.join("enrol.sqlite")).unwrap();
    conn.busy_timeout(Duration::from_secs(10)).unwrap();
    let changed = conn
        .execute(
            "UPDATE obop_record SET data = '5' WHERE model = 'instance_brand' AND id = 'instance'",
            (),
        )
        .unwrap();
    assert_eq!(changed, 1);
}

#[test]
fn the_running_server_writes_the_name_of_the_record_and_refuses_when_it_cannot_read_it() {
    let dir = Dir::new("flow");
    let url = dir.url();
    cli(
        &url,
        &["admin", "create", "ada", "--all-groups"],
        Some(&format!("{PW}\n")),
    );

    // No brand: what an instance always sent.
    let server = serve(&url, &[]);
    let token = login(server.port);
    let a = begin(server.port, &token);
    assert_eq!(a.status, 200, "{}", a.body);
    assert_eq!(a.body["issuer"], json!("obop"));
    assert!(
        a.body["uri"]
            .as_str()
            .unwrap()
            .starts_with("otpauth://totp/obop:ada?secret="),
        "{}",
        a.body
    );
    assert!(
        a.body["uri"]
            .as_str()
            .unwrap()
            .contains("&issuer=obop&algorithm=SHA1&digits=6&period=30"),
        "{}",
        a.body
    );
    let o = options(server.port, &token);
    assert_eq!(o.status, 200, "{}", o.body);
    assert_eq!(o.body["publicKey"]["rp"]["name"], json!("obop"));

    // The brand is set at the terminal while the server runs: the next enrolment has the name, with
    // no wait for a cache and no restart, and so does the one after a second change.
    for name in ["MOHV", "Other"] {
        cli(&url, &["admin", "brand", "configure", "--name", name], None);
        let a = begin(server.port, &token);
        assert_eq!(a.status, 200, "{name}: {}", a.body);
        assert_eq!(a.body["issuer"], json!(name));
        let uri = a.body["uri"].as_str().unwrap();
        assert!(
            uri.starts_with(&format!("otpauth://totp/{name}:ada?secret=")),
            "{uri}"
        );
        assert!(uri.contains(&format!("&issuer={name}&")), "{uri}");
        let o = options(server.port, &token);
        assert_eq!(o.status, 200, "{name}: {}", o.body);
        assert_eq!(o.body["publicKey"]["rp"]["name"], json!(name));
        assert_eq!(o.body["publicKey"]["rp"]["id"], json!("inst.test"));
    }
    cli(
        &url,
        &["admin", "brand", "configure", "--name", "MOHV"],
        None,
    );

    // A stored name that cannot be shown (written around the command): both enrolments are
    // refused, with the answer of the branding and no Retry-After; sign-in is not affected.
    store_a_name_that_cannot_be_shown(&url);
    is_brand_unavailable(&begin(server.port, &token), "begin");
    is_brand_unavailable(&options(server.port, &token), "options");
    assert!(!login(server.port).is_empty(), "sign-in still works");
    // The refusal is in the log of the server, with its cause named and none of the stored text: an
    // operator who sees `brand_unavailable` in an answer finds out here which cause it was.
    let log = server.log();
    assert!(
        log.contains("an enrolment was refused: the instance branding could not be read: the stored name cannot be shown"),
        "{log}"
    );
    assert!(!log.contains("a<b"), "{log}");

    // The other cause: the row is there and cannot be decoded (a store that is up and cannot answer
    // this read; here a row that holds a number where a record belongs). The answer is the same and
    // says nothing of the store; the log has the store's own text under the cause `store read`.
    store_a_row_that_cannot_be_decoded(&dir);
    let a = begin(server.port, &token);
    is_brand_unavailable(&a, "begin with a row that cannot be decoded");
    assert!(
        a.body["error"]
            .as_str()
            .unwrap()
            .ends_with("the store could not be read"),
        "{}",
        a.body
    );
    is_brand_unavailable(
        &options(server.port, &token),
        "options with a row that cannot be decoded",
    );
    let log = server.log();
    assert!(log.contains("cause: store read"), "{log}");
    assert!(log.contains("obop admin brand status"), "{log}");
    // Back to the stored name that cannot be shown, for the rest of the flow.
    // (Written around the store, as the row was spoiled: a write through it reads the row first.)
    {
        let conn = rusqlite::Connection::open(dir.0.join("enrol.sqlite")).unwrap();
        conn.busy_timeout(Duration::from_secs(10)).unwrap();
        let changed = conn
            .execute(
                "UPDATE obop_record SET data = ?1 WHERE model = 'instance_brand' AND id = 'instance'",
                (json!({"name": "a<b"}).to_string(),),
            )
            .unwrap();
        assert_eq!(changed, 1);
    }
    is_brand_unavailable(&begin(server.port, &token), "begin, name again");

    // OBOP_MFA_ISSUER: the authenticator entry needs no read and is made; the passkey still reads
    // the row and is refused.
    drop(server);
    let server = serve(&url, &[("OBOP_MFA_ISSUER", "Acme")]);
    let token = login(server.port);
    let a = begin(server.port, &token);
    assert_eq!(a.status, 200, "{}", a.body);
    assert_eq!(a.body["issuer"], json!("Acme"));
    assert!(
        a.body["uri"]
            .as_str()
            .unwrap()
            .starts_with("otpauth://totp/Acme:ada?secret="),
        "{}",
        a.body
    );
    is_brand_unavailable(
        &options(server.port, &token),
        "options with the variable set",
    );

    // The repair at the terminal; the variable still wins for the authenticator, the passkey has
    // the name of the record again.
    cli(
        &url,
        &["admin", "brand", "configure", "--name", "MOHV"],
        None,
    );
    let a = begin(server.port, &token);
    assert_eq!(a.body["issuer"], json!("Acme"));
    let o = options(server.port, &token);
    assert_eq!(o.status, 200, "{}", o.body);
    assert_eq!(o.body["publicKey"]["rp"]["name"], json!("MOHV"));
    // And the variable that says `obop` beats a record that says MOHV.
    drop(server);
    let server = serve(&url, &[("OBOP_MFA_ISSUER", "obop")]);
    let token = login(server.port);
    let a = begin(server.port, &token);
    assert_eq!(a.body["issuer"], json!("obop"));
    assert!(
        a.body["uri"]
            .as_str()
            .unwrap()
            .starts_with("otpauth://totp/obop:ada?secret="),
        "{}",
        a.body
    );
    assert!(
        !server.log().contains("WARNING: OBOP_MFA_ISSUER"),
        "a plain value is no warning: {}",
        server.log()
    );
    // A value with a link character is warned about once at start, and used as it is (it is
    // escaped in the label and in issuer=): the warning does not change the output.
    drop(server);
    let server = serve(&url, &[("OBOP_MFA_ISSUER", "Acme: Corp")]);
    let log = server.log();
    // Exactly one line: `obop serve` builds its authentication in more than one place (the one it
    // serves with and the count of users it reports), and each used to write the warning.
    assert_eq!(
        log.matches("WARNING: OBOP_MFA_ISSUER holds a colon")
            .count(),
        1,
        "{log}"
    );
    assert!(
        !log.contains("Acme"),
        "the warning does not repeat the value: {log}"
    );
    let token = login(server.port);
    let a = begin(server.port, &token);
    assert_eq!(a.status, 200, "{}", a.body);
    assert_eq!(a.body["issuer"], json!("Acme: Corp"));
    assert!(
        a.body["uri"]
            .as_str()
            .unwrap()
            .starts_with("otpauth://totp/Acme%3A%20Corp:ada?secret="),
        "{}",
        a.body
    );
    // And a request does not write it again.
    assert_eq!(
        server
            .log()
            .matches("WARNING: OBOP_MFA_ISSUER holds a colon")
            .count(),
        1,
        "{}",
        server.log()
    );
    // A value with spaces around it is used as it is: nothing between the environment and the
    // address trims it (the spaces are escaped in the label and in issuer=, as any character is).
    drop(server);
    let server = serve(&url, &[("OBOP_MFA_ISSUER", " Acme ")]);
    let token = login(server.port);
    let a = begin(server.port, &token);
    assert_eq!(a.status, 200, "{}", a.body);
    assert_eq!(a.body["issuer"], json!(" Acme "));
    let uri = a.body["uri"].as_str().unwrap();
    assert!(
        uri.starts_with("otpauth://totp/%20Acme%20:ada?secret="),
        "{uri}"
    );
    assert!(uri.contains("&issuer=%20Acme%20&"), "{uri}");
}

/// The log lines of a refused enrolment must not turn the 503 into a dropped connection when the
/// standard error of the server cannot be written. `eprintln!` panics then, the panic of a handler
/// is "handler panicked" in the server's `blocking`, and the person gets no answer at all: so the
/// lines are written with `writeln!` and the result ignored (`obop-http` `log_line`, `enrolment_error`
/// in `obop-commands`). The standard error here is a pipe whose read end is closed once the server
/// is up. Each of the two causes has its own line, and the test holds both.
#[test]
fn a_refused_enrolment_is_answered_when_the_standard_error_of_the_server_cannot_be_written() {
    let dir = Dir::new("closed-stderr");
    let url = dir.url();
    cli(
        &url,
        &["admin", "create", "ada", "--all-groups"],
        Some(&format!("{PW}\n")),
    );
    let mut server = serve_with_closed_stderr(&url, &[]);
    let token = login(server.port);
    // The enrolment that is not refused works with such a standard error as well.
    let a = begin(server.port, &token);
    assert_eq!(a.status, 200, "{}", a.body);

    let alive = |server: &mut Server, what: &str| {
        assert_eq!(get_status(server.port, "/healthz"), 200, "{what}");
        assert!(
            server.child.try_wait().unwrap().is_none(),
            "the server is still running: {what}"
        );
    };

    // The cause the answer states for itself: a stored name that cannot be shown. The route writes
    // its line (`obop-http` `log_line`).
    store_a_name_that_cannot_be_shown(&url);
    is_brand_unavailable(&begin(server.port, &token), "begin, name");
    is_brand_unavailable(&options(server.port, &token), "options, name");
    alive(&mut server, "after a stored name that cannot be shown");

    // The other cause: a row that cannot be decoded. `enrolment_error` writes the store's own text
    // and the route a second line.
    store_a_row_that_cannot_be_decoded(&dir);
    let a = begin(server.port, &token);
    is_brand_unavailable(&a, "begin, row");
    assert!(
        a.body["error"]
            .as_str()
            .unwrap()
            .ends_with("the store could not be read"),
        "{}",
        a.body
    );
    is_brand_unavailable(&options(server.port, &token), "options, row");
    alive(&mut server, "after a row that cannot be decoded");

    // The refusals did not stop it from answering: sign-in and a second refusal.
    assert!(!login(server.port).is_empty(), "sign-in still works");
    is_brand_unavailable(&begin(server.port, &token), "begin, row, again");
}
