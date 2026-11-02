//! Sign-in with an external identity provider (D91, ratified by OR-38): the port between the HTTP leg
//! (`obop-http`: the browser round trip and the two calls to the provider) and the identity
//! provider of the instance (`obop-auth`: the checks of the id token, the binding of a Microsoft
//! identity to an account, the one-time code and the session). Nothing here touches the network
//! and nothing here mints a session: `obop-auth` is the only crate that does.
//!
//! The provider is Microsoft Entra, one tenant, pinned in the configuration. The id token is
//! verified and dropped; no token of the provider is stored.

use crate::{LoginOutcome, MfaError, Principal};

/// What a line of [`SsoApi::audit_throttled`] is about. A fixed set: it is part of the key under
/// which a line is counted, so a caller cannot choose it, and it decides which hourly ceiling
/// the line is counted against (`group`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnonLine {
    /// A sign-in was begun: the harmless line of the success path of the start.
    Start,
    /// A callback with a state that is not a flow of this server (unknown, spent or expired):
    /// nobody has signed in for it, and anybody can send it.
    NoFlow,
    // A sign-in went wrong, at its start or after it was begun; each word is what stopped it.
    /// The provider's settings could not be read.
    Settings,
    /// The discovery document could not be used.
    Discovery,
    /// The browser that came back is not the one that left.
    Browser,
    /// The provider ended the sign-in with an error (or the person said no).
    Provider,
    /// The provider's redirect carried no code.
    NoCode,
    /// The provider was switched off meanwhile.
    SwitchedOff,
    /// Too many calls to the provider were in the air.
    Busy,
    /// The token endpoint could not be reached.
    TokenUnreachable,
    /// The token endpoint answered 429 or 5xx.
    TokenUnwell,
    /// The token endpoint refused (an expired client secret shows here).
    TokenRefused,
    /// The key set could not be fetched.
    KeySet,
    /// The key set could not be used.
    KeySetUnusable,
    /// The id token was refused.
    IdToken,
    /// The account side could not finish the sign-in.
    Begin,
}

/// The groups of [`AnonLine`] that have an hourly ceiling each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnonGroup {
    /// The lines of begun sign-ins.
    Start,
    /// The lines of callbacks that carry no flow of this server.
    NoFlow,
    /// The lines of sign-ins that went wrong, at their start or after they were begun.
    Failed,
}

impl AnonGroup {
    pub const ALL: [AnonGroup; 3] = [AnonGroup::Start, AnonGroup::NoFlow, AnonGroup::Failed];

    pub fn word(self) -> &'static str {
        match self {
            AnonGroup::Start => "start",
            AnonGroup::NoFlow => "no_flow",
            AnonGroup::Failed => "failed",
        }
    }
}

impl AnonLine {
    /// Every line, so that a test can forget the counters of all of them.
    pub const ALL: [AnonLine; 16] = [
        AnonLine::Start,
        AnonLine::NoFlow,
        AnonLine::Settings,
        AnonLine::Discovery,
        AnonLine::Browser,
        AnonLine::Provider,
        AnonLine::NoCode,
        AnonLine::SwitchedOff,
        AnonLine::Busy,
        AnonLine::TokenUnreachable,
        AnonLine::TokenUnwell,
        AnonLine::TokenRefused,
        AnonLine::KeySet,
        AnonLine::KeySetUnusable,
        AnonLine::IdToken,
        AnonLine::Begin,
    ];

    /// The ceiling the line is counted against: a begun sign-in, a callback that belongs to no
    /// sign-in of this server, or a sign-in that went wrong after it was begun.
    pub fn group(self) -> AnonGroup {
        match self {
            AnonLine::Start => AnonGroup::Start,
            AnonLine::NoFlow => AnonGroup::NoFlow,
            _ => AnonGroup::Failed,
        }
    }

    /// The word the line is keyed by.
    pub fn word(self) -> &'static str {
        match self {
            AnonLine::Start => "start",
            AnonLine::NoFlow => "no_flow",
            AnonLine::Settings => "settings",
            AnonLine::Discovery => "discovery",
            AnonLine::Browser => "browser",
            AnonLine::Provider => "provider",
            AnonLine::NoCode => "no_code",
            AnonLine::SwitchedOff => "switched_off",
            AnonLine::Busy => "busy",
            AnonLine::TokenUnreachable => "token_unreachable",
            AnonLine::TokenUnwell => "token_unwell",
            AnonLine::TokenRefused => "token_refused",
            AnonLine::KeySet => "key_set",
            AnonLine::KeySetUnusable => "key_set_unusable",
            AnonLine::IdToken => "id_token",
            AnonLine::Begin => "begin",
        }
    }
}

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

/// A Microsoft sign-in that finished for an OAuth flow (the phone sign-in, D91 and the D64
/// amendment): what the OAuth pages need to go on, and nothing the provider sent. It is made and
/// used inside one request; no one-time code stands between the two halves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsoFlowSignIn {
    /// The account the identity is bound to.
    pub user: String,
    /// What the sign-in step gives, as the password step does: a session (the pages read who it
    /// is and end it at once), a session limited to enrolment or a password change, or the demand
    /// for an obop second factor.
    pub outcome: LoginOutcome,
    /// The credential epoch of the account that the sign-in compared: the flow is bound to this
    /// value and not to one read later.
    pub epoch: Option<String>,
    /// Whether the provider reported a second factor.
    pub mfa: bool,
    /// The `auth_time` of the id token when it carried a usable one. Kept for the step-up (D91
    /// Decision 3); nothing reads it yet.
    pub auth_time: Option<u64>,
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
    /// `exchange`. Every refusal is `Refused`; the log says which. `binder` is a secret that only
    /// the browser that signed in holds (an HttpOnly cookie): the code is worth nothing to a
    /// browser that does not present it, so a code cannot be carried to another browser in a link
    /// (login CSRF). Only its hash is stored.
    fn begin(&self, identity: &SsoIdentity, binder: &str) -> Result<String, SsoError>;

    /// Spend the one-time code and give the sign-in step its result, as the password step does:
    /// a session, a session limited to enrolment, or the demand for an obop second factor. The
    /// code is spent also when `binder` is not the one it was issued for. The account's name
    /// comes with the outcome, so the page can show who is signed in (also while a second factor
    /// is still asked for).
    fn exchange(&self, code: &str, binder: &str) -> Result<(String, LoginOutcome), MfaError>;

    /// The phone sign-in: a verified identity signs in to an OAuth flow. `begin` and `exchange` in
    /// one call, with the same checks and the same effects (the account, its binding and the
    /// provider as they are now, the credential epoch compared, `last_login_at` stamped, one
    /// `oidc_login` line that says it was the OAuth sign-in pages), and no one-time code written
    /// anywhere. Returns the epoch that was compared.
    fn sign_in_for_flow(&self, identity: &SsoIdentity) -> Result<SsoFlowSignIn, MfaError>;

    /// The demand for an obop second factor again, for a flow whose first factor (a Microsoft
    /// sign-in) was accepted earlier at `epoch` and whose page was reloaded: a fresh `mfa_token`.
    /// Refused (`Expired`) when the account is disabled, its credentials changed since `epoch`, or
    /// it has no second factor any more. Never returns a session.
    fn second_step_for_flow(&self, user: &str, epoch: Option<&str>) -> Result<String, MfaError>;

    /// Write a line to the security log (`oidc_*`); never a token, a code or a secret.
    fn audit(&self, user: &str, kind: &str, detail: &str);

    /// A line about a sign-in that has no account to be named under (a begun sign-in, a callback
    /// that belongs to none, a sign-in that went wrong on the way): at most one security event per
    /// `source`, kind and [`AnonLine`] in ten minutes (the text is not part of that, it can be
    /// the caller's), and at most sixty lines an hour for all sources together in each of the
    /// three groups of [`AnonLine::group`], so that a flood of one group does not hide another.
    /// The first line past the ceiling of a group says so, and the first line of its next hour
    /// says how many were not written. `source` is the caller's address as the HTTP layer buckets
    /// it (an IPv6 address by its /64). `detail` must be a fixed text: nothing the caller sent
    /// goes into it, except a word out of a fixed set.
    fn audit_throttled(&self, source: &str, kind: &str, line: AnonLine, detail: &str);

    // ---- administration (an administrator whose session is whole; agents only propose) ----

    /// Store the configuration. The tenant must be a GUID (`common`, `organizations`,
    /// `consumers` and anything else are refused). `client_secret` `None` keeps the stored one,
    /// and `enabled` `None` keeps the stored switch (a first configuration is switched on).
    fn configure(
        &self,
        admin: &Principal,
        tenant: &str,
        client_id: &str,
        client_secret: Option<&str>,
        enabled: Option<bool>,
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
