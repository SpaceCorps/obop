//! OAuth 2.0 authorization code flow with PKCE (S256) as a client.
//!
//! The instance administrator supplies the client id and secret of a provider (obop ships none).
//! `begin` stores a single-use `state` with the PKCE verifier (sealed) and the connection it
//! belongs to; the callback must present that `state`, which is how a redirect is tied to the
//! person and connection that started it. Tokens are stored sealed, and `fresh` refreshes an
//! access token that is about to expire. Token requests go through the hardened client, so the
//! token endpoint must be on the provider's host allow-list.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use obop_storage::{Backend, Fields};
use serde_json::Value as J;
use sha2::{Digest, Sha256};

use crate::net::{Client, NetError, Request};
use crate::secretbox::SecretBox;
use crate::util::{now, random_hex};

pub const STATE_TTL_SECS: u64 = 600;
const STATE_MODEL: &str = "connector_oauth_state";

#[derive(Debug, Clone)]
pub struct OAuthConfig {
    pub auth_url: String,
    pub token_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub scopes: String,
    pub redirect_uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: u64,
    pub scope: String,
}

impl Tokens {
    pub fn to_json(&self) -> J {
        serde_json::json!({
            "access_token": self.access_token,
            "refresh_token": self.refresh_token,
            "expires_at": self.expires_at,
            "scope": self.scope,
        })
    }

    pub fn from_json(v: &J) -> Option<Self> {
        Some(Self {
            access_token: v["access_token"].as_str()?.to_string(),
            refresh_token: v["refresh_token"].as_str().map(String::from),
            expires_at: v["expires_at"].as_u64().unwrap_or(0),
            scope: v["scope"].as_str().unwrap_or("").to_string(),
        })
    }
}

/// A fresh PKCE pair: `(verifier, challenge)` with `challenge = BASE64URL(SHA256(verifier))`.
pub fn pkce_pair() -> (String, String) {
    let mut b = [0u8; 32];
    getrandom::fill(&mut b).expect("operating system randomness");
    let verifier = B64URL.encode(b);
    let challenge = challenge_of(&verifier);
    (verifier, challenge)
}

pub fn challenge_of(verifier: &str) -> String {
    B64URL.encode(Sha256::digest(verifier.as_bytes()))
}

fn enc(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

pub fn authorization_url(
    cfg: &OAuthConfig,
    state: &str,
    challenge: &str,
) -> Result<String, String> {
    let mut u = url::Url::parse(&cfg.auth_url).map_err(|_| "authorization URL is not valid")?;
    if u.scheme() != "https" && !is_loopback_http(&u) {
        return Err("authorization URL must be https".into());
    }
    u.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &cfg.client_id)
        .append_pair("redirect_uri", &cfg.redirect_uri)
        .append_pair("scope", &cfg.scopes)
        .append_pair("state", state)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256");
    Ok(u.to_string())
}

fn is_loopback_http(u: &url::Url) -> bool {
    u.scheme() == "http" && matches!(u.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))
}

/// What a begun flow remembers until the callback.
#[derive(Debug, PartialEq, Eq)]
pub struct Pending {
    pub connection: String,
    pub actor: String,
}

/// Start a flow: returns the URL to send the person to.
pub fn begin(
    backend: &dyn Backend,
    secrets: &SecretBox,
    cfg: &OAuthConfig,
    connection: &str,
    actor: &str,
) -> Result<String, String> {
    let (verifier, challenge) = pkce_pair();
    let state = random_hex(24);
    let mut f = Fields::new();
    f.insert("connection".into(), connection.into());
    f.insert("actor".into(), actor.into());
    f.insert(
        "verifier".into(),
        secrets
            .seal(&format!("oauth-state:{state}"), &verifier)
            .into(),
    );
    f.insert("created".into(), now().into());
    backend.insert(STATE_MODEL, &state, &f)?;
    authorization_url(cfg, &state, &challenge)
}

/// Consume a `state` (it works once). Returns what the flow was started for and the PKCE
/// verifier to present at the token endpoint.
pub fn take_state(
    backend: &dyn Backend,
    secrets: &SecretBox,
    state: &str,
) -> Result<(Pending, String), String> {
    let bad = || "the authorization link is invalid or has expired".to_string();
    // A state the store would not give is "later": the link is not spent, and is not "invalid".
    let f = backend.get(STATE_MODEL, state)?.ok_or_else(bad)?;
    // Single use, and consumed atomically: two callbacks with one state cannot both pass, and a
    // state that could not be consumed is not used (the link works again when the store answers).
    if !backend.delete(STATE_MODEL, state)? {
        return Err(bad());
    }
    if f.get("created").and_then(J::as_u64).unwrap_or(0) + STATE_TTL_SECS < now() {
        return Err(bad());
    }
    let verifier = secrets
        .open(
            &format!("oauth-state:{state}"),
            f.get("verifier").and_then(J::as_str).unwrap_or(""),
        )
        .map_err(|_| bad())?;
    Ok((
        Pending {
            connection: f.get("connection").and_then(J::as_str).unwrap_or("").into(),
            actor: f.get("actor").and_then(J::as_str).unwrap_or("").into(),
        },
        verifier,
    ))
}

fn token_request(
    client: &Client,
    cfg: &OAuthConfig,
    form: &[(&str, &str)],
    hosts: &[String],
) -> Result<Tokens, String> {
    let body = form
        .iter()
        .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
        .collect::<Vec<_>>()
        .join("&");
    let resp = client
        .send(Request {
            method: "POST",
            url: &cfg.token_url,
            headers: vec![
                (
                    "content-type".into(),
                    "application/x-www-form-urlencoded".into(),
                ),
                ("accept".into(), "application/json".into()),
            ],
            body: Some(body.into_bytes()),
            allowed_hosts: hosts,
        })
        .map_err(|e: NetError| e.to_string())?;
    let v: J =
        serde_json::from_slice(&resp.body).map_err(|_| "the token endpoint did not answer JSON")?;
    if resp.status != 200 {
        // Report the OAuth error code, never the body.
        let code = v["error"].as_str().unwrap_or("error");
        return Err(format!("the token endpoint refused the request ({code})"));
    }
    let access = v["access_token"]
        .as_str()
        .ok_or("the token endpoint sent no access token")?;
    Ok(Tokens {
        access_token: access.into(),
        refresh_token: v["refresh_token"].as_str().map(String::from),
        expires_at: now() + v["expires_in"].as_u64().unwrap_or(3600),
        scope: v["scope"].as_str().unwrap_or(&cfg.scopes).into(),
    })
}

pub fn exchange_code(
    client: &Client,
    cfg: &OAuthConfig,
    code: &str,
    verifier: &str,
    hosts: &[String],
) -> Result<Tokens, String> {
    token_request(
        client,
        cfg,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &cfg.redirect_uri),
            ("client_id", &cfg.client_id),
            ("client_secret", &cfg.client_secret),
            ("code_verifier", verifier),
        ],
        hosts,
    )
}

pub fn refresh(
    client: &Client,
    cfg: &OAuthConfig,
    refresh_token: &str,
    hosts: &[String],
) -> Result<Tokens, String> {
    let mut t = token_request(
        client,
        cfg,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &cfg.client_id),
            ("client_secret", &cfg.client_secret),
        ],
        hosts,
    )?;
    // Providers may omit the refresh token on refresh: keep the one we have.
    t.refresh_token
        .get_or_insert_with(|| refresh_token.to_string());
    Ok(t)
}

/// Whether an access token should be refreshed now.
pub fn needs_refresh(t: &Tokens, at: u64) -> bool {
    t.expires_at <= at + 60
}

/// Remove flows that were never completed.
pub fn sweep(backend: &dyn Backend) -> usize {
    let cutoff = now();
    let mut n = 0;
    for (id, f) in backend.list(STATE_MODEL).unwrap_or_default() {
        if f.get("created").and_then(J::as_u64).unwrap_or(0) + STATE_TTL_SECS < cutoff
            && backend.delete(STATE_MODEL, &id).unwrap_or(false)
        {
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::ClientConfig;
    use crate::net::testserver::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[test]
    fn pkce_challenge_matches_the_rfc_example() {
        // RFC 7636 appendix B.
        assert_eq!(
            challenge_of("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        let (v, c) = pkce_pair();
        assert!(v.len() >= 43 && challenge_of(&v) == c);
    }

    #[test]
    fn a_state_that_could_not_be_consumed_is_not_used_and_one_the_store_would_not_give_is_not_invalid()
     {
        use obop_storage::testing::{Flaky, Mode};
        let b = obop_storage::open("memory:").unwrap();
        let flaky = Flaky::new(b.clone());
        let secrets = SecretBox::for_tests();
        let mut f = Fields::new();
        f.insert("connection".into(), "c1".into());
        f.insert("actor".into(), "root".into());
        f.insert(
            "verifier".into(),
            secrets.seal("oauth-state:st1", "the-verifier").into(),
        );
        f.insert("created".into(), now().into());
        b.insert(STATE_MODEL, "st1", &f).unwrap();
        // Refused to read: "later", not "the link is invalid".
        flaky.refuse_model(STATE_MODEL, obop_storage::testing::Mode::Reads);
        let e = take_state(&*flaky, &secrets, "st1").unwrap_err();
        assert!(e.starts_with("storage_busy"), "{e}");
        // Refused to consume: the state is not used (it would stay valid for a replay).
        flaky.refuse_model(STATE_MODEL, Mode::Updates);
        let e = take_state(&*flaky, &secrets, "st1").unwrap_err();
        assert!(e.starts_with("storage_busy"), "{e}");
        flaky.allow_models();
        assert!(b.get(STATE_MODEL, "st1").unwrap().is_some(), "not spent");
        // Once, and once only.
        assert!(take_state(&*flaky, &secrets, "st1").is_ok());
        assert!(take_state(&*flaky, &secrets, "st1").is_err());
    }

    fn cfg(port: u16) -> OAuthConfig {
        OAuthConfig {
            auth_url: format!("http://127.0.0.1:{port}/authorize"),
            token_url: format!("http://127.0.0.1:{port}/token"),
            client_id: "client-1".into(),
            client_secret: "client-secret-1".into(),
            scopes: "calendar.read".into(),
            redirect_uri: "http://127.0.0.1:8080/oauth/callback".into(),
        }
    }

    fn form(req: &str) -> HashMap<String, String> {
        let body = req.split("\r\n\r\n").nth(1).unwrap_or("");
        url::form_urlencoded::parse(body.as_bytes())
            .into_owned()
            .collect()
    }

    /// A fake provider: it remembers the challenge it was given and only issues tokens for a
    /// matching verifier, as a real one does.
    #[test]
    fn full_flow_against_a_local_fake_provider() {
        let challenges = Arc::new(Mutex::new(HashMap::<String, String>::new()));
        let seen = challenges.clone();
        let server = serve(move |req| {
            let f = form(req);
            let json = match f.get("grant_type").map(String::as_str) {
                Some("authorization_code") => {
                    let challenge = seen.lock().unwrap().get(&f["code"]).cloned();
                    let good = f["client_secret"] == "client-secret-1"
                        && challenge.is_some_and(|c| c == challenge_of(&f["code_verifier"]));
                    if good {
                        r#"{"access_token":"at-1","refresh_token":"rt-1","expires_in":3600,"scope":"calendar.read"}"#
                    } else {
                        return "HTTP/1.1 400 Bad Request\r\ncontent-length: 25\r\nconnection: close\r\n\r\n{\"error\":\"invalid_grant\"}".into();
                    }
                }
                Some("refresh_token") if f["refresh_token"] == "rt-1" => {
                    r#"{"access_token":"at-2","expires_in":10}"#
                }
                _ => return "HTTP/1.1 400 Bad Request\r\ncontent-length: 27\r\nconnection: close\r\n\r\n{\"error\":\"invalid_request\"}".into(),
            };
            ok(json)
        });
        let c = cfg(server.port);
        let client = Client::new(ClientConfig {
            allow_loopback: true,
            timeout: Duration::from_secs(5),
            ..ClientConfig::default()
        });
        let hosts = vec!["127.0.0.1".to_string()];
        let backend = obop_storage::open("memory:").unwrap();
        let sb = SecretBox::for_tests();

        let url = begin(&*backend, &sb, &c, "conn-1", "ada").unwrap();
        let u = url::Url::parse(&url).unwrap();
        let q: HashMap<_, _> = u.query_pairs().into_owned().collect();
        assert_eq!(q["response_type"], "code");
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(q["client_id"], "client-1");
        assert_eq!(q["redirect_uri"], c.redirect_uri);
        // The verifier is stored sealed, never in the URL.
        let stored = backend.get(STATE_MODEL, &q["state"]).unwrap().unwrap();
        assert!(stored["verifier"].as_str().unwrap().starts_with("e2."));
        assert!(!url.contains(stored["verifier"].as_str().unwrap()));

        // The user agrees; the provider hands the browser a code bound to the challenge.
        challenges
            .lock()
            .unwrap()
            .insert("code-1".into(), q["code_challenge"].clone());
        // A callback with an unknown state is refused.
        assert!(take_state(&*backend, &sb, "nope").is_err());
        let (pending, verifier) = take_state(&*backend, &sb, &q["state"]).unwrap();
        assert_eq!(
            pending,
            Pending {
                connection: "conn-1".into(),
                actor: "ada".into()
            }
        );
        // The state works once.
        assert!(take_state(&*backend, &sb, &q["state"]).is_err());

        let t = exchange_code(&client, &c, "code-1", &verifier, &hosts).unwrap();
        assert_eq!(
            (t.access_token.as_str(), t.refresh_token.as_deref()),
            ("at-1", Some("rt-1"))
        );
        // A wrong verifier (an attacker who stole the code) gets nothing.
        let e = exchange_code(&client, &c, "code-1", "not-the-verifier", &hosts).unwrap_err();
        assert!(e.contains("invalid_grant") && !e.contains("client-secret"));

        // Refresh keeps the refresh token when the provider does not send a new one.
        let r = refresh(&client, &c, "rt-1", &hosts).unwrap();
        assert_eq!(
            (r.access_token.as_str(), r.refresh_token.as_deref()),
            ("at-2", Some("rt-1"))
        );
        assert!(needs_refresh(&r, now()), "10 s left is within the margin");
        assert!(!needs_refresh(&t, now()));
        assert!(refresh(&client, &c, "stolen", &hosts).is_err());
        assert_eq!(Tokens::from_json(&t.to_json()), Some(t));
    }

    #[test]
    fn expired_states_are_refused_and_swept() {
        let backend = obop_storage::open("memory:").unwrap();
        let sb = SecretBox::for_tests();
        let mut f = Fields::new();
        f.insert("connection".into(), "c".into());
        f.insert("actor".into(), "a".into());
        f.insert("verifier".into(), sb.seal("oauth-state:old", "v").into());
        f.insert("created".into(), (now() - STATE_TTL_SECS - 5).into());
        backend.insert(STATE_MODEL, "old", &f).unwrap();
        backend.insert(STATE_MODEL, "old2", &f).unwrap();
        assert_eq!(sweep(&*backend), 2);
        backend.insert(STATE_MODEL, "old", &f).unwrap();
        assert!(take_state(&*backend, &sb, "old").is_err());
    }

    #[test]
    fn authorization_url_must_be_https() {
        let mut c = cfg(1);
        c.auth_url = "http://accounts.example.com/authorize".into();
        assert!(authorization_url(&c, "s", "c").is_err());
        c.auth_url = "https://accounts.example.com/authorize?prompt=consent".into();
        let u = authorization_url(&c, "s", "c").unwrap();
        assert!(u.contains("prompt=consent") && u.contains("state=s"));
    }

    #[test]
    fn token_endpoint_host_must_be_allowed() {
        let client = Client::new(ClientConfig::default());
        let c = OAuthConfig {
            token_url: "https://evil.example.net/token".into(),
            ..cfg(1)
        };
        let e = exchange_code(&client, &c, "c", "v", &["accounts.example.com".to_string()])
            .unwrap_err();
        assert!(e.contains("allow-list"));
    }
}
