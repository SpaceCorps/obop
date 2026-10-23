//! Start-up checks of the settings of D67 and D68 with the real binary: bad values stop the server
//! before it listens, good values take effect over a real socket. No database server is needed.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn obop() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_obop"));
    c.env("OBOP_DEV_INSECURE_KEY", "1")
        .env("OBOP_REQUIRE_MFA_FOR_ADMINS", "0")
        .env_remove("OBOP_DATABASE_URL")
        .env_remove("OBOP_ALLOWED_ORIGINS")
        .env_remove("OBOP_METRICS_TOKEN")
        .env_remove("OBOP_SECRET_KEY")
        .env_remove("OBOP_SECRET_KEY_PREVIOUS");
    c
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Run `obop serve` with these variables; it must exit by itself with a message on stderr.
fn refused(env: &[(&str, &str)]) -> String {
    let out = obop()
        .args(["serve", "--addr", "127.0.0.1:0", "--db", "memory:"])
        .envs(env.iter().copied())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .expect("run obop");
    assert!(!out.status.success(), "the server must refuse {env:?}");
    String::from_utf8_lossy(&out.stderr).to_string()
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

fn start(env: &[(&str, &str)]) -> Server {
    let port = free_port();
    let child = obop()
        .args([
            "serve",
            "--addr",
            &format!("127.0.0.1:{port}"),
            "--db",
            "memory:",
        ])
        .envs(env.iter().copied())
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

/// One raw HTTP/1.1 request; returns the status and the whole response text.
fn http(port: u16, method: &str, path: &str, headers: &[(&str, &str)]) -> (u16, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let mut req =
        format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n");
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    s.write_all(req.as_bytes()).unwrap();
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    let status = out
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (status, out)
}

fn has_header(resp: &str, name: &str) -> bool {
    resp.lines().take_while(|l| !l.is_empty()).any(|l| {
        l.to_ascii_lowercase()
            .starts_with(&format!("{}:", name.to_ascii_lowercase()))
    })
}

fn header_value(resp: &str, name: &str) -> String {
    resp.lines()
        .take_while(|l| !l.is_empty())
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.eq_ignore_ascii_case(name).then(|| v.trim().to_string())
        })
        .unwrap_or_default()
}

#[test]
fn a_wildcard_in_allowed_origins_stops_the_server_with_a_clear_message() {
    for bad in ["*", "https://ok.example,*", "https://*.example.com"] {
        let e = refused(&[("OBOP_ALLOWED_ORIGINS", bad)]);
        assert!(
            e.contains("OBOP_ALLOWED_ORIGINS") && e.contains("wildcards"),
            "{bad}: {e}"
        );
    }
}

#[test]
fn other_invalid_allowed_origins_are_refused_at_startup_too() {
    for bad in [
        "http://app.example.com",
        "https://app.example.com/path",
        "app.example.com",
        "null",
    ] {
        let e = refused(&[("OBOP_ALLOWED_ORIGINS", bad)]);
        assert!(
            e.contains("OBOP_ALLOWED_ORIGINS has an invalid entry"),
            "{bad}: {e}"
        );
    }
}

#[test]
fn a_short_metrics_token_stops_the_server() {
    let e = refused(&[("OBOP_METRICS_TOKEN", "short")]);
    assert!(e.contains("OBOP_METRICS_TOKEN must be at least"), "{e}");
}

#[test]
fn a_running_server_answers_cors_health_and_metrics_as_configured() {
    let s = start(&[
        (
            "OBOP_ALLOWED_ORIGINS",
            "https://app.example.com, http://localhost:5173",
        ),
        ("OBOP_METRICS_TOKEN", "scrape-token-0123456789"),
    ]);
    // Preflight over the wire.
    let (st, r) = http(
        s.port,
        "OPTIONS",
        "/api/commands",
        &[
            ("Origin", "https://app.example.com"),
            ("Access-Control-Request-Method", "GET"),
            ("Access-Control-Request-Headers", "authorization"),
        ],
    );
    assert_eq!(st, 204, "{r}");
    assert_eq!(
        header_value(&r, "access-control-allow-origin"),
        "https://app.example.com"
    );
    assert!(!has_header(&r, "access-control-allow-credentials"));
    let (st, r) = http(
        s.port,
        "OPTIONS",
        "/api/commands",
        &[
            ("Origin", "https://evil.example"),
            ("Access-Control-Request-Method", "GET"),
        ],
    );
    assert_eq!(st, 403);
    assert!(!has_header(&r, "access-control-allow-origin"));
    // Actual request from an allowed origin.
    let (_, r) = http(
        s.port,
        "GET",
        "/healthz",
        &[("Origin", "https://app.example.com")],
    );
    assert!(
        !has_header(&r, "access-control-allow-origin"),
        "only /api has CORS"
    );
    let (st, r) = http(
        s.port,
        "GET",
        "/api/commands",
        &[("Origin", "https://app.example.com")],
    );
    assert_eq!(st, 401, "{r}");
    assert_eq!(
        header_value(&r, "access-control-allow-origin"),
        "https://app.example.com"
    );
    // Health is open; metrics wants the token even from loopback.
    assert_eq!(http(s.port, "GET", "/healthz", &[]).0, 200);
    assert_eq!(http(s.port, "GET", "/readyz", &[]).0, 200);
    assert_eq!(http(s.port, "GET", "/metrics", &[]).0, 401);
    let (st, r) = http(
        s.port,
        "GET",
        "/metrics",
        &[("Authorization", "Bearer scrape-token-0123456789")],
    );
    assert_eq!(st, 200, "{r}");
    assert!(r.contains("obop_build_info"));
}

#[test]
fn a_loopback_server_without_a_token_serves_metrics_to_a_direct_caller_only() {
    let s = start(&[]);
    assert_eq!(http(s.port, "GET", "/metrics", &[]).0, 200);
    assert_eq!(
        http(
            s.port,
            "GET",
            "/metrics",
            &[("X-Forwarded-For", "203.0.113.7")]
        )
        .0,
        404
    );
    // No allowed origins: no CORS headers at all, as before.
    let (_, r) = http(
        s.port,
        "GET",
        "/api/commands",
        &[("Origin", "https://app.example.com")],
    );
    assert!(!has_header(&r, "access-control-allow-origin"));
    assert!(!has_header(&r, "vary") || !header_value(&r, "vary").to_lowercase().contains("origin"));
}
