//! Sign-in with an external identity provider (D91, Proposed): the port between the HTTP leg
//! (`obop-http`: the browser round trip and the two calls to the provider) and the identity
//! provider of the instance (`obop-auth`: the checks of the id token, the binding of a Microsoft
//! identity to an account, the one-time code and the session). Nothing here touches the network
//! and nothing here mints a session: `obop-auth` is the only crate that does.
//!
//! The provider is Microsoft Entra, one tenant, pinned in the configuration. The id token is
//! verified and dropped; no token of the provider is stored.

use crate::{LoginOutcome, MfaError, Principal};

/// The configuration of the one provider, as an administrator sees it. The client secret is never
/// part of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsoProvider {
    /// `entra`.
    pub kind: String,
    /// The pinned tenant (a GUID). Never `common`, `organizations` or `consumers`.
    pub tenant: String,
    pub client_id: String,
    /// Whether the sign-in is offered (the button on the sign-in page).
    pub enabled: bool,
    /// Whether a client secret is stored (sealed under `OBOP_SECRET_KEY`).
    pub secret_set: bool,
    pub updated_at: Option<String>,
    pub updated_by: Option<String>,
}

/// What the browser leg needs to talk to the provider. The secret is opened for this one use.
#[derive(Clone)]
pub struct SsoLeg {
    /// `https://login.microsoftonline.com` (a loopback address in tests only).
    pub authority: String,
    pub tenant: String,
    pub client_id: String,
    pub client_secret: String,
}

impl std::fmt::Debug for SsoLeg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SsoLeg")
            .field("authority", &self.authority)
            .field("tenant", &self.tenant)
            .field("client_id", &self.client_id)
            .field("client_secret", &"(sealed)")
            .finish()
    }
}

/// The three addresses of the provider's discovery document that the leg uses, all checked to lie
/// under the pinned tenant of the authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsoEndpoints {
    pub authorization: String,
    pub token: String,
    pub jwks: String,
}

/// A verified identity: what an id token that passed every check says. Handed back to
/// `SsoApi::begin` and nowhere else; it carries no token and no name or address of the person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsoIdentity {
    pub issuer: String,
    pub tenant: String,
    /// The object id of the person in the directory (the stable identifier; never an e-mail).
    pub oid: String,
    /// The `amr` claim as a list (absent: empty).
    pub amr: Vec<String>,
    pub auth_time: Option<u64>,
}

impl SsoIdentity {
    /// Whether the provider says a second factor was used.
    pub fn mfa(&self) -> bool {
        self.amr.iter().any(|m| m == "mfa")
    }
}

/// Why a step of the Microsoft sign-in did not go through. The person is told one thing for
/// `Refused` and `UnknownKey`; the security log gets the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SsoError {
    /// Not a valid sign-in: a check of the token failed, nobody is bound, the account is
    /// disabled. The text is for the log, never for the browser.
    Refused(String),
    /// The id token names a key the cached key set does not hold: fetch the key set again (once,
    /// rate limited) and retry before refusing.
    UnknownKey,
    /// Bad input from an administrator (a tenant that is not a GUID, an object id that is not
    /// one, a rule of the break-glass guard).
    Invalid(String),
    /// The store or the key ring could not do it.
    Unavailable(String),
    /// The caller may not do this (not an administrator): answered like an unknown command.
    NotFound,
}

/// The identity provider's side of the sign-in with Microsoft (D91). Implemented by `obop-auth`.
pub trait SsoApi: Send + Sync {
    /// Whether the sign-in button is offered: a provider is configured and enabled. A store that
    /// does not answer is "no" (nothing is offered on a guess).
    fn offered(&self) -> bool;

    /// The configuration, without the secret; `None` when none is stored.
    fn provider(&self) -> Result<Option<SsoProvider>, SsoError>;

    /// The configuration of an enabled provider with the secret opened, for the one HTTP leg.
    /// `Ok(None)` when none is stored or it is not enabled.
    fn leg(&self) -> Result<Option<SsoLeg>, SsoError>;

    /// Check the discovery document of the pinned tenant: its `issuer` is exactly the issuer of
    /// the tenant and its three endpoints lie under the authority and the tenant.
    fn endpoints(&self, discovery: &[u8]) -> Result<SsoEndpoints, SsoError>;

    /// Verify an id token against the key set (the JSON of the `jwks_uri`) and the `nonce` of the
    /// flow it was asked for. Checks the signature, the algorithm, the issuer, the tenant, the
    /// audience, the nonce and the times; see D91.
    fn verify(&self, id_token: &str, jwks: &[u8], nonce: &str) -> Result<SsoIdentity, SsoError>;

    /// A verified identity asks to sign in: find the account it is bound to and, if that account
    /// may sign in, return a one-time code (single use, 60 seconds, stored hashed) for
    /// `exchange`. Every refusal is `Refused`; the log says which. `ip` is for the log only.
    fn begin(&self, identity: &SsoIdentity, ip: &str) -> Result<String, SsoError>;

    /// Spend the one-time code and give the sign-in step its result, as the password step does:
    /// a session, a session limited to enrolment, or the demand for an obop second factor.
    fn exchange(&self, code: &str) -> Result<LoginOutcome, MfaError>;

    /// Write a line to the security log (`oidc_*`); never a token, a code or a secret.
    fn audit(&self, user: &str, kind: &str, detail: &str);

    // ---- administration (an administrator whose session is whole; agents only propose) ----

    /// Store the configuration. The tenant must be a GUID (`common`, `organizations`,
    /// `consumers` and anything else are refused). `client_secret` `None` keeps the stored one.
    fn configure(
        &self,
        admin: &Principal,
        tenant: &str,
        client_id: &str,
        client_secret: Option<&str>,
        enabled: bool,
    ) -> Result<SsoProvider, SsoError>;

    /// Remove the configuration (the bindings stay; nobody can sign in with Microsoft meanwhile).
    /// SSO-only accounts then have no way in except the break-glass administrators' help, so the
    /// log entry says how many there were.
    fn clear(&self, admin: &Principal) -> Result<(), SsoError>;

    /// Bind the object id `oid` (in the pinned tenant) to the existing account `user`. The
    /// account must exist and be enabled, and neither may be bound already. Creates no account,
    /// no person, no grant and no group.
    fn bind(&self, admin: &Principal, user: &str, oid: &str)
    -> Result<serde_json::Value, SsoError>;

    /// Remove the binding of `user`. Refused while the account is SSO-only (turn that off first).
    fn unbind(&self, admin: &Principal, user: &str) -> Result<serde_json::Value, SsoError>;

    /// Make the account SSO-only (it refuses the password and the passkey on its own) or turn
    /// that off. Turning it on needs a binding, and leaves at least two other enabled
    /// administrators that are not SSO-only and hold a working second factor (break-glass).
    fn set_sso_only(
        &self,
        admin: &Principal,
        user: &str,
        on: bool,
    ) -> Result<serde_json::Value, SsoError>;

    /// The accounts with their binding and SSO-only flag, and how many break-glass administrators
    /// the instance has. For administrators.
    fn bindings(&self, admin: &Principal) -> Result<serde_json::Value, SsoError>;
}
