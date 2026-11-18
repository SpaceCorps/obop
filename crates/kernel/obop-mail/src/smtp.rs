//! An SMTP client for one message at a time (RFC 5321, 3207, 4954), over `rustls`.
//!
//! The host is vetted by the Connectors' outbound client first (the operator's host allow-list, a
//! public address; loopback only with `OBOP_CONNECTORS_ALLOW_LOOPBACK=1`), and the connection goes
//! to the address that was vetted. `starttls` never falls back to plain text; a user name and
//! password never travel without TLS except to a loopback address. Errors name the step and the
//! server's answer, never a credential.

use crate::Failure;
use crate::mime::Message;
use base64::Engine as _;
use obop_connectors::net::{Client, NetError};
use rustls::pki_types::{CertificateDer, ServerName};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::Duration;

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const IO_TIMEOUT: Duration = Duration::from_secs(60);
/// The longest server reply read (all its lines together).
const MAX_REPLY: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Security {
    StartTls,
    Tls,
    None,
}

impl Security {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "starttls" | "" => Some(Self::StartTls),
            "tls" => Some(Self::Tls),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}

/// One relay as a mail server names it.
#[derive(Clone)]
pub struct Relay {
    pub host: String,
    pub port: u16,
    pub security: Security,
    pub username: String,
    pub password: String,
    /// The name this side gives in `EHLO`.
    pub helo: String,
}

impl std::fmt::Debug for Relay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Relay")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("security", &self.security)
            .finish_non_exhaustive()
    }
}

/// The trust anchors: the Mozilla roots, plus any a test adds.
#[derive(Clone, Default)]
pub struct Trust {
    pub extra: Vec<CertificateDer<'static>>,
}

impl Trust {
    fn store(&self) -> rustls::RootCertStore {
        let mut s =
            rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        for c in &self.extra {
            let _ = s.add(c.clone());
        }
        s
    }
}

enum Conn {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
    Gone,
}

impl Read for Conn {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Conn::Plain(s) => s.read(b),
            Conn::Tls(s) => s.read(b),
            Conn::Gone => Err(std::io::ErrorKind::NotConnected.into()),
        }
    }
}

impl Write for Conn {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        match self {
            Conn::Plain(s) => s.write(b),
            Conn::Tls(s) => s.write(b),
            Conn::Gone => Err(std::io::ErrorKind::NotConnected.into()),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Conn::Plain(s) => s.flush(),
            Conn::Tls(s) => s.flush(),
            Conn::Gone => Ok(()),
        }
    }
}

struct Reply {
    code: u16,
    lines: Vec<String>,
}

impl Reply {
    fn text(&self) -> String {
        let t = self.lines.join(" ");
        t.chars().take(300).collect()
    }
}

struct Session {
    conn: Conn,
    buf: Vec<u8>,
    tls: bool,
}

fn io(step: &str, e: std::io::Error) -> Failure {
    Failure::Temporary(format!("{step}: {e}"))
}

impl Session {
    fn read_line(&mut self, total: &mut usize) -> Result<String, Failure> {
        loop {
            if let Some(i) = self.buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = self.buf.drain(..=i).collect();
                *total += line.len();
                let s = String::from_utf8_lossy(&line);
                return Ok(s.trim_end_matches(['\r', '\n']).to_string());
            }
            if self.buf.len() + *total > MAX_REPLY {
                return Err(Failure::Temporary("the server's reply is too long".into()));
            }
            let mut chunk = [0u8; 4096];
            let n = self
                .conn
                .read(&mut chunk)
                .map_err(|e| io("reading the server's reply", e))?;
            if n == 0 {
                return Err(Failure::Temporary(
                    "the server closed the connection".into(),
                ));
            }
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }

    fn reply(&mut self) -> Result<Reply, Failure> {
        let mut total = 0;
        let mut lines = Vec::new();
        loop {
            let line = self.read_line(&mut total)?;
            let code = line
                .get(..3)
                .and_then(|c| c.parse::<u16>().ok())
                .ok_or_else(|| Failure::Temporary("the server's reply is not SMTP".into()))?;
            let more = line.as_bytes().get(3) == Some(&b'-');
            lines.push(line.get(4..).unwrap_or("").to_string());
            if !more {
                return Ok(Reply { code, lines });
            }
        }
    }

    fn send_line(&mut self, line: &str) -> Result<(), Failure> {
        self.conn
            .write_all(format!("{line}\r\n").as_bytes())
            .and_then(|()| self.conn.flush())
            .map_err(|e| io("writing to the server", e))
    }

    /// Send a command and require one of `ok`. `step` names it in an error; `shown` is what the
    /// error may quote of the command (never a credential).
    fn command(&mut self, line: &str, ok: &[u16], step: &str) -> Result<Reply, Failure> {
        self.send_line(line)?;
        self.expect(ok, step)
    }

    fn expect(&mut self, ok: &[u16], step: &str) -> Result<Reply, Failure> {
        let r = self.reply()?;
        if ok.contains(&r.code) {
            return Ok(r);
        }
        let why = format!("{step}: the server answered {} {}", r.code, r.text());
        Err(if r.code >= 500 {
            Failure::Permanent(why)
        } else {
            Failure::Temporary(why)
        })
    }

    fn upgrade(&mut self, host: &str, trust: &Trust) -> Result<(), Failure> {
        if !self.buf.is_empty() {
            return Err(Failure::Permanent(
                "the server sent data before the TLS handshake; refused".into(),
            ));
        }
        let Conn::Plain(tcp) = std::mem::replace(&mut self.conn, Conn::Gone) else {
            return Err(Failure::Permanent("TLS was started twice".into()));
        };
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let cfg = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| Failure::Permanent(format!("TLS setup: {e}")))?
            .with_root_certificates(trust.store())
            .with_no_client_auth();
        let name = ServerName::try_from(host.to_string())
            .map_err(|_| Failure::Permanent(format!("{host} is not a valid TLS server name")))?;
        let conn = rustls::ClientConnection::new(Arc::new(cfg), name)
            .map_err(|e| Failure::Permanent(format!("TLS setup: {e}")))?;
        let mut tls = rustls::StreamOwned::new(conn, tcp);
        while tls.conn.is_handshaking() {
            tls.conn
                .complete_io(&mut tls.sock)
                .map_err(|e| Failure::Permanent(format!("TLS handshake with {host}: {e}")))?;
        }
        self.conn = Conn::Tls(Box::new(tls));
        self.tls = true;
        Ok(())
    }
}

fn ehlo(s: &mut Session, helo: &str) -> Result<Vec<String>, Failure> {
    let r = s.command(&format!("EHLO {helo}"), &[250], "EHLO")?;
    Ok(r.lines
        .iter()
        .skip(1)
        .map(|l| l.to_ascii_uppercase())
        .collect())
}

fn vet(client: &Client, relay: &Relay) -> Result<Vec<SocketAddr>, Failure> {
    let host = relay.host.trim().to_ascii_lowercase();
    if host.is_empty() {
        return Err(Failure::Permanent("the mail server has no host".into()));
    }
    let shown = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.clone()
    };
    // The outbound client checks a URL; the scheme only satisfies its rule, nothing is sent.
    let url = format!("https://{shown}:{}/", relay.port);
    client
        .vet(&url, std::slice::from_ref(&host))
        .map(|v| v.addrs)
        .map_err(|e| match e {
            NetError::Resolve(_) | NetError::Timeout | NetError::Transport(_) => {
                Failure::Temporary(e.to_string())
            }
            other => Failure::Permanent(other.to_string()),
        })
}

/// Send one message. `Ok` carries the server's answer to the end of `DATA` (its queue id, as a
/// reference in the log).
pub fn send(
    client: &Client,
    trust: &Trust,
    relay: &Relay,
    msg: &Message,
) -> Result<String, Failure> {
    // Built first: a message that cannot be built is refused before the relay is contacted.
    let body = msg.to_bytes().map_err(Failure::Permanent)?;
    let addrs = vet(client, relay)?;
    let loopback = addrs.iter().all(|a| a.ip().is_loopback());
    let mut last = None;
    let mut tcp = None;
    for a in &addrs {
        match TcpStream::connect_timeout(a, CONNECT_TIMEOUT) {
            Ok(s) => {
                tcp = Some(s);
                break;
            }
            Err(e) => last = Some(e),
        }
    }
    let tcp = tcp.ok_or_else(|| {
        Failure::Temporary(format!(
            "could not connect to {}:{}: {}",
            relay.host,
            relay.port,
            last.map_or_else(|| "no address".to_string(), |e| e.to_string())
        ))
    })?;
    let _ = tcp.set_read_timeout(Some(IO_TIMEOUT));
    let _ = tcp.set_write_timeout(Some(IO_TIMEOUT));
    let mut s = Session {
        conn: Conn::Plain(tcp),
        buf: Vec::new(),
        tls: false,
    };
    let host = relay.host.trim();
    if relay.security == Security::Tls {
        s.upgrade(host, trust)?;
    }
    s.expect(&[220], "greeting")?;
    let mut ext = ehlo(&mut s, &relay.helo)?;
    if relay.security == Security::StartTls {
        if !ext.iter().any(|l| l == "STARTTLS") {
            return Err(Failure::Permanent(
                "the server does not offer STARTTLS; choose the security it supports".into(),
            ));
        }
        s.command("STARTTLS", &[220], "STARTTLS")?;
        s.upgrade(host, trust)?;
        ext = ehlo(&mut s, &relay.helo)?;
    }
    if !relay.username.is_empty() {
        if !s.tls && !loopback {
            return Err(Failure::Permanent(
                "a password is never sent without TLS; choose STARTTLS or TLS".into(),
            ));
        }
        let mechs: Vec<&str> = ext
            .iter()
            .filter_map(|l| l.strip_prefix("AUTH ").or_else(|| l.strip_prefix("AUTH=")))
            .flat_map(str::split_whitespace)
            .collect();
        let refused = |e: Failure| match e {
            Failure::Permanent(_) => {
                Failure::Permanent("the server refused the user name or password".into())
            }
            t => t,
        };
        if mechs.contains(&"PLAIN") {
            let token = B64.encode(format!("\0{}\0{}", relay.username, relay.password));
            s.command(&format!("AUTH PLAIN {token}"), &[235], "AUTH")
                .map_err(refused)?;
        } else if mechs.contains(&"LOGIN") {
            s.command("AUTH LOGIN", &[334], "AUTH")?;
            s.command(&B64.encode(&relay.username), &[334], "AUTH")
                .map_err(refused)?;
            s.command(&B64.encode(&relay.password), &[235], "AUTH")
                .map_err(refused)?;
        } else {
            return Err(Failure::Permanent(
                "the server offers neither AUTH PLAIN nor AUTH LOGIN".into(),
            ));
        }
    }
    s.command(&format!("MAIL FROM:<{}>", msg.from), &[250], "MAIL FROM")?;
    for r in msg.recipients() {
        s.command(
            &format!("RCPT TO:<{r}>"),
            &[250, 251],
            &format!("RCPT TO {r}"),
        )?;
    }
    s.command("DATA", &[354], "DATA")?;
    let mut out = Vec::with_capacity(body.len() + 16);
    for line in body.split_inclusive(|&b| b == b'\n') {
        if line.first() == Some(&b'.') {
            out.push(b'.');
        }
        out.extend_from_slice(line);
    }
    out.extend_from_slice(b".\r\n");
    s.conn
        .write_all(&out)
        .and_then(|()| s.conn.flush())
        .map_err(|e| io("sending the message", e))?;
    let done = s.expect(&[250], "end of DATA")?;
    let _ = s.send_line("QUIT");
    Ok(done.text())
}
