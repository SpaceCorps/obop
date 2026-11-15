//! Google Drive as a connected account (the `google_drive` provider, engine `browse`).
//!
//! An administrator enters the OAuth client id and secret of their own Google Cloud project under
//! the provider policy (the secret is sealed), starts the authorization from the Google Drive page
//! of the back office, and the callback keeps the sealed tokens on an instance-scoped
//! `connection`. The Drive browser of the Documents app reads its credentials from that
//! connection at request time, so nothing is configured in the environment.
//!
//! The permission group that may browse is the provider policy's `allowed_group` (empty means
//! `platform.admin`: the connection is one Drive for the whole instance).
//!
//! The endpoints are Google's. For tests and local development they can be pointed at a fake on a
//! loopback address (`OBOP_GOOGLE_AUTH_URL`, `OBOP_GOOGLE_TOKEN_URL`, `OBOP_GOOGLE_API_BASE`);
//! anything else is ignored, so no setting can send the client secret to another host.
//!
//! Not run against Google itself.

use obop_storage::Fields;
use serde_json::{Value as J, json};

use crate::model::{CONNECTION, Connection, provider_policy};
use crate::net::Request;
use crate::oauth::Tokens;
use crate::runtime::{PURPOSE_CLIENT, PURPOSE_OAUTH, Runtime};
use crate::util::{now, str_of};

pub const PROVIDER: &str = "google_drive";
const GOOGLE_API: &str = "https://www.googleapis.com";
const DEFAULT_GROUP: &str = "platform.admin";
/// The scope that lets obop make folders, Docs and uploads (only what the app creates).
pub const WRITE_SCOPE: &str = "https://www.googleapis.com/auth/drive.file";

/// An address from `name` when it is plain http on a literal loopback host.
fn loopback_override(name: &str) -> Option<String> {
    let v = std::env::var(name).ok()?;
    let u = url::Url::parse(v.trim()).ok()?;
    (u.scheme() == "http" && matches!(u.host_str(), Some("127.0.0.1" | "localhost" | "[::1]")))
        .then(|| v.trim().trim_end_matches('/').to_string())
}

/// The addresses of Google's authorization, token and API endpoints.
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub auth_url: String,
    pub token_url: String,
    pub api_base: String,
}

static OVERRIDE: std::sync::RwLock<Option<Endpoints>> = std::sync::RwLock::new(None);

/// Point the Google addresses at a fake on a loopback address, for tests of this module's users
/// (`None` goes back to the environment and Google). Anything but plain http to a literal loopback
/// host is refused, like the environment settings.
pub fn use_endpoints(e: Option<Endpoints>) -> Result<(), String> {
    if let Some(e) = &e {
        for a in [&e.auth_url, &e.token_url, &e.api_base] {
            let ok = url::Url::parse(a).is_ok_and(|u| {
                u.scheme() == "http"
                    && matches!(u.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))
            });
            if !ok {
                return Err(format!("{a} is not a loopback address"));
            }
        }
    }
    *OVERRIDE.write().map_err(|_| "lock poisoned")? = e;
    Ok(())
}

pub fn endpoints() -> Endpoints {
    if let Ok(g) = OVERRIDE.read()
        && let Some(e) = g.as_ref()
    {
        return e.clone();
    }
    Endpoints {
        auth_url: loopback_override("OBOP_GOOGLE_AUTH_URL")
            .unwrap_or_else(|| "https://accounts.google.com/o/oauth2/v2/auth".into()),
        token_url: loopback_override("OBOP_GOOGLE_TOKEN_URL")
            .unwrap_or_else(|| "https://oauth2.googleapis.com/token".into()),
        api_base: loopback_override("OBOP_GOOGLE_API_BASE").unwrap_or_else(|| GOOGLE_API.into()),
    }
}

/// Where the Google Drive page stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// No OAuth client id and secret yet.
    NotConfigured,
    /// The client is set; nobody has signed in with Google yet.
    NeedsConsent,
    Connected,
    /// The last sign-in or check failed; `last_error` says why.
    Error,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotConfigured => "not_configured",
            Self::NeedsConsent => "needs_consent",
            Self::Connected => "connected",
            Self::Error => "error",
        }
    }
}

/// What the back office shows, without a single secret.
#[derive(Debug, Clone)]
pub struct Status {
    pub state: State,
    /// The OAuth client id the administrator entered (an identifier, not a secret).
    pub client_id: String,
    /// The secret is stored; its value is never returned.
    pub secret_set: bool,
    /// The Google account that signed in.
    pub account: String,
    /// The address to register in Google Cloud Console, when `OBOP_PUBLIC_URL` is set.
    pub redirect_uri: Option<String>,
    pub connection: Option<String>,
    pub last_error: String,
    pub browse_group: String,
    /// The drive the instance works in: `root` is My Drive, empty is none chosen.
    pub workspace_id: String,
    pub workspace_name: String,
    /// The group that may create things in the workspace.
    pub write_group: String,
    /// Whether the sign-in granted the write scope (an earlier sign-in may not have).
    pub write_granted: bool,
}

impl Status {
    pub fn to_json(&self) -> J {
        json!({
            "state": self.state.as_str(),
            "client_id": self.client_id,
            "secret_set": self.secret_set,
            "account": self.account,
            "redirect_uri": self.redirect_uri,
            "connection": self.connection,
            "last_error": self.last_error,
            "browse_group": self.browse_group,
            "workspace_id": self.workspace_id,
            "workspace_name": self.workspace_name,
            "write_group": self.write_group,
            "write_granted": self.write_granted,
        })
    }
}

/// What the Drive browser signs in with.
#[derive(Clone)]
pub struct Credentials {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    pub api_base: String,
    pub token_url: String,
    /// The scopes the sign-in was granted, space separated.
    pub scope: String,
}

impl Credentials {
    pub fn can_write(&self) -> bool {
        self.scope.split_whitespace().any(|s| s == WRITE_SCOPE)
    }
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("client_id", &self.client_id)
            .field("api_base", &self.api_base)
            .finish_non_exhaustive()
    }
}

impl Runtime {
    /// The Drive connection in use: the newest of the instance's connections that is not revoked.
    fn drive_connection(&self) -> Result<Option<Connection>, String> {
        let mut best: Option<Connection> = None;
        for (id, f) in self.backend.list(CONNECTION)? {
            if str_of(&f, "provider") != PROVIDER || str_of(&f, "state") == "revoked" {
                continue;
            }
            // Ids are time ordered (UUID v7): the greatest is the newest.
            if best.as_ref().is_none_or(|b| id > b.id) {
                best = Some(Connection::from_fields(&id, &f)?);
            }
        }
        Ok(best)
    }

    /// The group that may browse the Drive: the policy's `allowed_group`, else `platform.admin`.
    pub fn drive_browse_group(&self) -> String {
        provider_policy(&*self.backend, PROVIDER)
            .map(|p| p.allowed_group)
            .filter(|g| !g.is_empty())
            .unwrap_or_else(|| DEFAULT_GROUP.into())
    }

    /// The group that may create things in the workspace drive: the policy's `write_group`, else
    /// `platform.admin`.
    pub fn drive_write_group(&self) -> String {
        provider_policy(&*self.backend, PROVIDER)
            .map(|p| p.write_group)
            .filter(|g| !g.is_empty())
            .unwrap_or_else(|| DEFAULT_GROUP.into())
    }

    /// The workspace drive an administrator chose: its id (`root` is My Drive) and its name.
    pub fn drive_workspace(&self) -> Option<(String, String)> {
        provider_policy(&*self.backend, PROVIDER)
            .filter(|p| !p.workspace_id.is_empty())
            .map(|p| (p.workspace_id, p.workspace_name))
    }

    /// The scopes the stored sign-in was granted; empty when nobody has signed in.
    fn drive_scope(&self, c: Option<&Connection>) -> String {
        let Some(c) = c.filter(|c| !c.oauth_tokens.is_empty()) else {
            return String::new();
        };
        self.secrets
            .open(PURPOSE_OAUTH, &c.oauth_tokens)
            .ok()
            .and_then(|t| serde_json::from_str::<J>(&t).ok())
            .and_then(|v| Tokens::from_json(&v))
            .map(|t| t.scope)
            .unwrap_or_default()
    }

    pub fn drive_status(&self) -> Result<Status, String> {
        let pp = provider_policy(&*self.backend, PROVIDER);
        let client_id = pp.as_ref().map_or("", |p| p.oauth_client_id.as_str());
        let secret_set = pp
            .as_ref()
            .is_some_and(|p| !p.oauth_client_secret.is_empty());
        let conn = self.drive_connection()?;
        let tokens = conn.as_ref().is_some_and(|c| !c.oauth_tokens.is_empty());
        let (state, last_error) = if client_id.is_empty() || !secret_set {
            (State::NotConfigured, String::new())
        } else if !tokens {
            (State::NeedsConsent, String::new())
        } else if conn.as_ref().is_some_and(|c| c.state == "error") {
            let why = self
                .backend
                .get(
                    CONNECTION,
                    &conn.as_ref().map_or_else(String::new, |c| c.id.clone()),
                )?
                .map(|f| str_of(&f, "last_error").to_string())
                .unwrap_or_default();
            (State::Error, why)
        } else {
            (State::Connected, String::new())
        };
        let scope = self.drive_scope(conn.as_ref());
        let (workspace_id, workspace_name) = self.drive_workspace().unwrap_or_default();
        Ok(Status {
            write_granted: scope.split_whitespace().any(|s| s == WRITE_SCOPE),
            workspace_id,
            workspace_name,
            write_group: self.drive_write_group(),
            state,
            client_id: client_id.into(),
            secret_set,
            account: conn
                .as_ref()
                .map_or_else(String::new, |c| c.cfg("account").to_string()),
            redirect_uri: self
                .cfg
                .public_url
                .as_ref()
                .map(|b| format!("{b}/oauth/callback")),
            connection: conn.map(|c| c.id),
            last_error,
            browse_group: self.drive_browse_group(),
        })
    }

    /// The client and the refresh token of the connected Drive, opened; `None` when the Drive is
    /// not configured or nobody has signed in. Never log or return the result.
    pub fn drive_credentials(&self) -> Result<Option<Credentials>, String> {
        let Some(pp) = provider_policy(&*self.backend, PROVIDER) else {
            return Ok(None);
        };
        if pp.oauth_client_id.is_empty() || pp.oauth_client_secret.is_empty() {
            return Ok(None);
        }
        let Some(c) = self.drive_connection()? else {
            return Ok(None);
        };
        if c.oauth_tokens.is_empty() || c.state == "error" {
            return Ok(None);
        }
        let secret = self.secrets.open(PURPOSE_CLIENT, &pp.oauth_client_secret)?;
        let text = self.secrets.open(PURPOSE_OAUTH, &c.oauth_tokens)?;
        let tokens = Tokens::from_json(
            &serde_json::from_str(&text).map_err(|_| "stored tokens are malformed")?,
        )
        .ok_or("stored tokens are malformed")?;
        let Some(refresh_token) = tokens.refresh_token else {
            return Err(
                "Google sent no refresh token: connect again (revoke the app's access at myaccount.google.com/permissions first)".into(),
            );
        };
        let e = endpoints();
        Ok(Some(Credentials {
            client_id: pp.oauth_client_id,
            client_secret: secret,
            refresh_token,
            api_base: e.api_base,
            token_url: e.token_url,
            scope: tokens.scope,
        }))
    }

    /// After the callback stored the tokens: ask Drive who signed in, keep the account, and mark
    /// the connection connected, or in error with the reason.
    pub(crate) fn drive_authorized(&self, c: &Connection, tokens: &Tokens) -> Result<(), String> {
        let e = endpoints();
        let url = format!("{}/drive/v3/about?fields=user(emailAddress)", e.api_base);
        let host = url::Url::parse(&url)
            .ok()
            .and_then(|u| u.host_str().map(String::from))
            .unwrap_or_default();
        let asked = if tokens.refresh_token.is_none() {
            Err("Google sent no refresh token. Remove obop under myaccount.google.com/permissions and connect again".to_string())
        } else {
            self.client
                .send(Request {
                    method: "GET",
                    url: &url,
                    headers: vec![
                        (
                            "authorization".into(),
                            format!("Bearer {}", tokens.access_token),
                        ),
                        ("accept".into(), "application/json".into()),
                    ],
                    body: None,
                    allowed_hosts: &[host],
                })
                .map_err(|e| e.to_string())
                .and_then(|r| {
                    if r.status == 200 {
                        serde_json::from_slice::<J>(&r.body)
                            .map_err(|_| "Drive did not answer JSON".to_string())
                    } else {
                        Err(format!(
                            "Google Drive answered HTTP {} (is the Drive API enabled in the Google Cloud project?)",
                            r.status
                        ))
                    }
                })
        };
        let mut f = Fields::new();
        let result = match asked {
            Ok(v) => {
                let account = v["user"]["emailAddress"].as_str().unwrap_or("");
                let mut cfg = c.config.clone();
                cfg.insert(
                    "account".into(),
                    account.chars().take(120).collect::<String>().into(),
                );
                f.insert("config".into(), J::Object(cfg).to_string().into());
                f.insert("state".into(), "connected".into());
                f.insert("last_error".into(), "".into());
                f.insert("last_sync_at".into(), crate::util::iso(now()).into());
                Ok(())
            }
            Err(why) => {
                f.insert("state".into(), "error".into());
                f.insert(
                    "last_error".into(),
                    why.chars().take(300).collect::<String>().into(),
                );
                Err(why)
            }
        };
        obop_storage::fault::needed(|| self.backend.update(CONNECTION, &c.id, &f).map(|_| ()))?;
        result
    }
}
