//! The first administrator of a new instance, made in the browser (D96, PROPOSED): the port the
//! routes `GET /api/auth/setup` and `POST /api/auth/setup` reach. Implemented by `obop-auth`; an
//! authenticator without it (`--dev-auth`, the control plane) answers `None`, and the routes are
//! then not there (404).
//!
//! No principal exists before the first account, so this is not a command of the catalogue and
//! has no MCP form: it sits beside `/api/auth/login`, which is not one either. The proof is a
//! one-time set-up code that each server process prints in its own log at start while no account
//! exists; only its SHA-256 is kept, in the shared expiring store.

use crate::LoginOutcome;

/// What the sign-in page needs to know before anyone has an account. Never a code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetupStatus {
    /// No account exists, so the set-up page is shown instead of the sign-in form.
    pub open: bool,
    /// A set-up code is in force (printed by a server process in the last 60 minutes).
    pub code_active: bool,
    /// `OBOP_SECRET_KEY` is not set: the first administrator could never enrol a second factor,
    /// so set-up is refused until it is.
    pub needs_secret_key: bool,
}

/// One attempt to create the first administrator.
#[derive(Debug, Clone, Copy)]
pub struct SetupRequest<'a> {
    /// The bucket the caller is counted under (an IPv6 address by its /64), for the limits and
    /// the throttle of the log. Never written into the text of a log line.
    pub source: &'a str,
    pub code: &'a str,
    pub username: &'a str,
    pub password: &'a str,
    /// The name of the instance, written through the brand writer before the account; `None` or
    /// blank: none is written.
    pub instance_name: Option<&'a str>,
}

/// Why an attempt created nothing. Each maps to one answer of the route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupError {
    /// An account exists: the page is closed for good (409 `setup_closed`).
    Closed,
    /// No server key (503 `setup_needs_secret_key`).
    NeedsSecretKey,
    /// Too many attempts from this address or in all; the seconds to wait (429).
    RateLimited(u64),
    /// The code is wrong, expired, burnt, missing, or no code is in force: one answer for all of
    /// them (403 `setup_code_invalid`).
    CodeInvalid,
    /// The username, the password or the instance name is refused, with the sentence (400).
    Invalid(String),
    /// The store refused a read or a write, a counter could not be kept, or the lock was not
    /// granted: nothing is decided on a guess (503).
    Unavailable(String),
}

/// The identity provider's side of the first-administrator set-up.
pub trait SetupApi: Send + Sync {
    /// The state the sign-in page shows. A count of accounts that cannot be read is an error,
    /// never "open".
    fn status(&self) -> Result<SetupStatus, SetupError>;
    /// Check, in this order: closed, server key, limits, code, input; then under the lock: closed
    /// again, code again, the instance name, the account, the codes burnt. The answer is the one a
    /// password sign-in gives (a session limited to enrolment where the policy says so).
    fn complete(&self, req: &SetupRequest<'_>) -> Result<LoginOutcome, SetupError>;
}
