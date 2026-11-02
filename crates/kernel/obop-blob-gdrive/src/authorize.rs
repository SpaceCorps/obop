//! `obop storage drive-authorize`: get a refresh token for the Drive store.
//!
//! The authorization code flow with PKCE (S256) and a loopback redirect, the way Google asks
//! desktop applications to do it: the operator opens the printed link, signs in and agrees, the
//! browser is sent to `http://127.0.0.1:<port>/` where this process is listening for that one
//! request, and the code in it is exchanged for the tokens. Only the refresh token is returned.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use obop_storage::Result;
use serde_json::Value as J;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::time::{Duration, Instant};

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const SCOPE_PREFIX: &str = "https://www.googleapis.com/auth/";

pub struct AuthorizeOptions {
    pub client_id: String,
    pub client_secret: String,
    /// `drive.file` (files this app creates; the default) or `drive` (every file).
    pub scope: String,
    pub auth_url: String,
    pub token_url: String,
    /// How long to wait for the browser.
    pub wait: Duration,
}

impl AuthorizeOptions {
    pub fn new(client_id: String, client_secret: String, scope: &str) -> Self {
        Self {
            client_id,
            client_secret,
            scope: scope.into(),
            auth_url: AUTH_URL.into(),
            token_url: super::TOKEN_URL.into(),
            wait: Duration::from_secs(300),
        }
    }
}

fn random_b64(n: usize) -> Result<String> {
    let mut b = vec![0u8; n];
    getrandom::fill(&mut b).map_err(|e| e.to_string())?;
    Ok(URL_SAFE_NO_PAD.encode(b))
}

/// Run the flow. `show` receives the link the operator must open; the refresh token comes back.
pub fn authorize(opts: &AuthorizeOptions, show: &mut dyn FnMut(&str)) -> Result<String> {
    if !matches!(
        opts.scope.as_str(),
        "drive.file" | "drive" | "drive.readonly"
    ) {
        return Err("--scope must be drive.file, drive.readonly or drive".into());
    }
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect = format!("http://127.0.0.1:{port}");
    let verifier = random_b64(32)?;
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_b64(16)?;

    let mut link =
        url::Url::parse(&opts.auth_url).map_err(|_| "the authorization URL is invalid")?;
    link.query_pairs_mut()
        .append_pair("client_id", &opts.client_id)
        .append_pair("redirect_uri", &redirect)
        .append_pair("response_type", "code")
        .append_pair("scope", &format!("{SCOPE_PREFIX}{}", opts.scope))
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent")
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state);
    show(link.as_str());

    let code = wait_for_code(&listener, &state, opts.wait)?;

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .into();
    let mut resp = agent
        .post(&opts.token_url)
        .send_form([
            ("client_id", opts.client_id.as_str()),
            ("client_secret", opts.client_secret.as_str()),
            ("code", code.as_str()),
            ("code_verifier", verifier.as_str()),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect.as_str()),
        ])
        .map_err(|_| "Google sign-in unreachable")?;
    let status = resp.status().as_u16();
    let v: J = serde_json::from_str(&resp.body_mut().read_to_string().unwrap_or_default())
        .unwrap_or(J::Null);
    if status != 200 {
        let code = v["error"].as_str().unwrap_or("unknown");
        return Err(format!(
            "Google refused the authorization code (HTTP {status}, {})",
            super::clean(code, 60)
        ));
    }
    v["refresh_token"]
        .as_str()
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            "Google sent no refresh token: remove this app at https://myaccount.google.com/permissions and run the command again".into()
        })
}

/// Wait for the one request that carries `code` and `state`; other requests (a favicon) get a 404.
fn wait_for_code(listener: &TcpListener, state: &str, wait: Duration) -> Result<String> {
    let deadline = Instant::now() + wait;
    loop {
        match listener.accept() {
            Ok((mut s, _)) => {
                if let Some(result) = handle(&mut s, state) {
                    return result;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err("no answer from the browser: run the command again".into());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

fn handle(s: &mut TcpStream, state: &str) -> Option<Result<String>> {
    let _ = s.set_nonblocking(false);
    let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buf = [0u8; 8192];
    let mut n = 0;
    while n < buf.len() {
        match s.read(&mut buf[n..]) {
            Ok(0) | Err(_) => break,
            Ok(k) => {
                n += k;
                if buf[..n].windows(2).any(|w| w == b"\r\n") {
                    break;
                }
            }
        }
    }
    let head = String::from_utf8_lossy(&buf[..n]).into_owned();
    let target = head
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("GET "))
        .and_then(|l| l.split(' ').next())
        .unwrap_or("");
    let parsed = url::Url::parse(&format!("http://127.0.0.1{target}")).ok();
    let q = |name: &str| -> Option<String> {
        parsed.as_ref().and_then(|u| {
            u.query_pairs()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.into_owned())
        })
    };
    let reply = |s: &mut TcpStream, status: &str, text: &str| {
        let _ = write!(
            s,
            "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
            text.len()
        );
    };
    if q("state").is_none() && q("error").is_none() {
        reply(s, "404 Not Found", "not found");
        return None;
    }
    if q("state").as_deref() != Some(state) {
        reply(
            s,
            "400 Bad Request",
            "wrong state: close this tab and run the command again",
        );
        return Some(Err(
            "the browser answered with a different state: run the command again".into(),
        ));
    }
    if let Some(e) = q("error") {
        reply(
            s,
            "200 OK",
            "Access was not granted. You can close this tab.",
        );
        return Some(Err(format!(
            "Google did not grant access ({})",
            super::clean(&e, 60)
        )));
    }
    match q("code").filter(|c| !c.is_empty()) {
        Some(code) => {
            reply(s, "200 OK", "obop is authorised. You can close this tab.");
            Some(Ok(code))
        }
        None => {
            reply(s, "400 Bad Request", "no code");
            Some(Err("the browser answered without a code".into()))
        }
    }
}
