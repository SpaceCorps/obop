//! The OAuth 2.1 authorization server of an obop instance (D64), for the MCP authorization flow:
//! dynamic client registration of public clients (RFC 7591), the authorization code grant with
//! mandatory PKCE S256 and a resource indicator (RFC 7636, RFC 8707), refresh tokens with
//! rotation and reuse detection, revocation (RFC 7009), and the metadata documents (RFC 8414,
//! RFC 9728).
//!
//! This crate has no web framework and does not render the HTTP exchange: `obop-http` carries
//! requests in and out. It holds the protocol rules, the state in the shared storage (D59) and
//! the server-rendered pages. Access tokens are the scoped agent tokens of D18, issued through
//! [`obop_commands::OAuthTokens`] with a short life and an audience; refresh tokens, codes and
//! the browser binding of a sign-in are stored only as SHA-256 hashes, so nothing here needs the
//! key ring and no secret is ever written in the clear.

pub mod client;
mod devices;
pub mod flow;
pub mod page;
pub mod redirect;
pub mod token;
pub mod util;

use obop_commands::{Authenticator, OAuthTokens};
use obop_storage::Backend;
use serde_json::{Value as J, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

pub use client::{Class, Client};
pub use flow::{AuthorizeParams, AuthzError, DecideOut, FlowCtx, FlowError, LoginOut, Validated};
pub use token::Form;

/// Limits and lifetimes. The defaults are the ones D64 states.
#[derive(Debug, Clone)]
pub struct OAuthConfig {
    /// Lifetime of an access token.
    pub access_ttl_secs: u64,
    /// Lifetime of an authorization code.
    pub code_ttl_secs: u64,
    /// How long a sign-in and consent can take.
    pub flow_ttl_secs: u64,
    /// Registered clients, all together.
    pub max_clients: usize,
    pub max_redirect_uris: usize,
    /// Registrations per client address and hour, and in all.
    pub register_per_ip_hour: u64,
    pub register_global_hour: u64,
    /// Authorization starts and sign-in attempts per client address and ten minutes.
    pub authorize_per_ip: u64,
    pub login_per_ip: u64,
    /// Token and revocation requests per client address and minute.
    pub token_per_ip_minute: u64,
    /// A client that never got a code approved is forgotten after this long.
    pub unused_client_secs: u64,
    /// A client nobody has used for this long is forgotten.
    pub idle_client_secs: u64,
}

impl Default for OAuthConfig {
    fn default() -> Self {
        Self {
            access_ttl_secs: 3600,
            code_ttl_secs: 60,
            flow_ttl_secs: 600,
            max_clients: 2000,
            max_redirect_uris: 5,
            register_per_ip_hour: 20,
            register_global_hour: 300,
            authorize_per_ip: 60,
            login_per_ip: 30,
            token_per_ip_minute: 120,
            unused_client_secs: 24 * 3600,
            idle_client_secs: 180 * 24 * 3600,
        }
    }
}

/// A refusal, ready to be written as an OAuth error response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthError {
    pub status: u16,
    pub error: &'static str,
    pub description: String,
    pub retry_after: Option<u64>,
}

impl OAuthError {
    pub fn new(status: u16, error: &'static str, description: &str) -> Self {
        Self {
            status,
            error,
            description: description.to_string(),
            retry_after: None,
        }
    }

    /// The store refused a call (a full pool, an unreachable database): nothing was decided and
    /// the client may try again. A 503 always says for how long to wait (D74).
    pub fn storage_busy() -> Self {
        Self::unavailable("storage is busy", 1)
    }

    /// A 503 with the seconds to wait before the client asks again.
    pub fn unavailable(description: &str, wait: u64) -> Self {
        Self {
            retry_after: Some(wait),
            ..Self::new(503, "temporarily_unavailable", description)
        }
    }

    pub fn busy(wait: u64) -> Self {
        Self {
            retry_after: Some(wait),
            ..Self::new(
                429,
                "temporarily_unavailable",
                "too many requests, try again later",
            )
        }
    }

    pub fn json(&self) -> J {
        json!({"error": self.error, "error_description": self.description})
    }
}

pub struct OAuth {
    pub(crate) backend: Arc<dyn Backend>,
    pub(crate) auth: Arc<dyn Authenticator>,
    pub(crate) cfg: OAuthConfig,
    skew: AtomicU64,
}

impl OAuth {
    /// `None` when the identity provider cannot issue OAuth tokens (`--dev-auth`): there is then
    /// no authorization server, and the metadata says so.
    pub fn new(
        backend: Arc<dyn Backend>,
        auth: Arc<dyn Authenticator>,
        cfg: OAuthConfig,
    ) -> Option<Arc<Self>> {
        auth.oauth()?;
        Some(Arc::new(Self {
            backend,
            auth,
            cfg,
            skew: AtomicU64::new(0),
        }))
    }

    pub(crate) fn tokens(&self) -> &dyn OAuthTokens {
        self.auth.oauth().expect("checked in OAuth::new")
    }

    pub fn config(&self) -> &OAuthConfig {
        &self.cfg
    }

    pub fn now(&self) -> u64 {
        util::now_system() + self.skew.load(Ordering::Relaxed)
    }

    /// Move this server's clock forward. For tests of expiry only.
    pub fn advance_clock_for_test(&self, secs: u64) {
        self.skew.fetch_add(secs, Ordering::Relaxed);
    }

    /// Count a hit for `key`; `Err` carries the seconds to wait.
    pub(crate) fn limit(&self, key: &str, max: u64, window_ms: u64) -> Result<(), OAuthError> {
        obop_storage::limit::hit(&*self.backend, key, max, window_ms).map_err(OAuthError::busy)
    }

    pub(crate) fn audit(&self, user: &str, kind: &str, detail: &str) {
        self.tokens().audit(user, kind, user, detail);
    }

    /// Housekeeping at most once an hour across all processes: forget clients nobody uses and
    /// grants that ended a week ago, with their refresh tokens.
    pub fn housekeeping(&self) {
        if self
            .backend
            .counter_add("oauth.housekeeping", 3_600_000, false)
            .is_ok_and(|n| n == 1)
        {
            self.purge_clients();
            self.purge_grants();
        }
    }
}

/// The resource identifier of the MCP endpoint of an instance.
pub fn mcp_resource(base: &str) -> String {
    format!("{base}/mcp")
}

/// The resource identifier of the REST surface of an instance: the audience of the access token of
/// a mobile app (the D64 amendment).
pub fn api_resource(base: &str) -> String {
    format!("{base}/api")
}

/// RFC 8414 metadata of the authorization server of the instance at `base`.
pub fn authorization_server_metadata(base: &str) -> J {
    json!({
        "issuer": base,
        "authorization_endpoint": format!("{base}/oauth/authorize"),
        "token_endpoint": format!("{base}/oauth/token"),
        "registration_endpoint": format!("{base}/oauth/register"),
        "revocation_endpoint": format!("{base}/oauth/revoke"),
        "response_types_supported": ["code"],
        "response_modes_supported": ["query"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "revocation_endpoint_auth_methods_supported": ["none"],
        "authorization_response_iss_parameter_supported": true,
        // A client says which it is when it registers (`obop_client_class`): an MCP client by
        // default, or a mobile app, whose access token is for /api (docs/DEPLOY.md "Mobile apps").
        "obop_client_classes_supported": ["mcp", "mobile"],
        "service_documentation": "https://obop.dev/docs/agents/mcp/",
    })
}

#[cfg(test)]
mod tests;
