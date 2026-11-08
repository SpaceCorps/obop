//! The enrolment texts with the real binary and a real socket (docs/spec/branding.md, slice S1e,
//! sections 4 and 5): a running `obop serve` over a SQLite file writes the name of the instance
//! brand into the label and `issuer=` of a new authenticator entry and into the relying party name
//! of the passkey registration options, reads it from the row at each enrolment (a change made at
//! the terminal applies to the very next one, with no wait and no restart), lets `OBOP_MFA_ISSUER`
//! win for the authenticator entry only, and refuses both enrolments with a 503 `brand_unavailable`
//! (no `Retry-After`) when the stored name cannot be shown, until it is repaired at the terminal.
//! The same flow is done by hand with `curl` in the report of the slice.

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
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn serve(url: &str, env: &[(&str, &str)]) -> Server {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let child = obop(
        &["serve", "--addr", &format!("127.0.0.1:{port}"), "--db", url],
        env,
    )
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()
    .expect("spawn obop serve");
    let s = Server { child, port };
    let deadline = Instant::now() + Duration::from_secs(60);
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(Instant::now() < deadline, "obop serve did not come up");
        std::thread::sleep(Duration::from_millis(100));
    }
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
    {
        let backend = obop_storage::open(&url).unwrap();
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
    is_brand_unavailable(&begin(server.port, &token), "begin");
    is_brand_unavailable(&options(server.port, &token), "options");
    assert!(!login(server.port).is_empty(), "sign-in still works");

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
}
