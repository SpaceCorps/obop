//! Account administration beyond enable and disable (D69): creating a user, editing a user's
//! permission groups and offboarding. The port is implemented by `obop-auth`; the REST routes
//! under `/api/auth/users`, the `user_account` commands of MCP and the `obop admin` CLI all end
//! here, so one rule set governs every surface.

use crate::{MfaError, Principal};

/// A user an administrator creates.
#[derive(Debug, Clone)]
pub struct NewUser<'a> {
    pub name: &'a str,
    /// The initial password. `None`: the server makes a random temporary one and returns it once.
    pub password: Option<&'a str>,
    pub groups: &'a [String],
    /// The person must choose a new password at their first sign-in (default: yes).
    pub must_change_password: bool,
}

/// The administrator's account tools. Every method needs a signed-in, enabled administrator
/// whose session is not limited to two-factor enrolment; anything else is `NotFound`, like an
/// unknown command. Nothing returned is a secret except the temporary password that
/// `create_user` hands back once.
pub trait AccountsApi: Send + Sync {
    /// The current principal of an enabled administrator, read from the stored account rather
    /// than from a token: used when an approved proposal runs, so a plan made by an
    /// administrator who has since lost the group or been disabled does nothing.
    fn administrator(&self, name: &str) -> Option<Principal>;

    /// Create an account. `known` lists the registered permission groups; every group must be
    /// one of them. Returns `{user, groups, must_change_password, temporary_password?}`.
    fn create_user(
        &self,
        admin: &Principal,
        new: &NewUser<'_>,
        known: &[String],
    ) -> Result<serde_json::Value, MfaError>;

    /// Replace the groups of an account. Refused: unknown groups (a group the person already
    /// holds may stay), taking `platform.admin` from oneself or from the last enabled
    /// administrator. Takes effect on the next request of every live session. Returns
    /// `{user, before, after}`.
    fn set_user_groups(
        &self,
        admin: &Principal,
        target: &str,
        groups: &[String],
        known: &[String],
    ) -> Result<serde_json::Value, MfaError>;

    /// Disable the account and end everything it could use: sessions, agent tokens, connected
    /// applications and pending passkey proofs. Enabling the account later does not bring the
    /// revoked credentials back. Returns `{user, revoked_sessions, revoked_agent_tokens,
    /// revoked_apps}`.
    fn offboard_user(
        &self,
        admin: &Principal,
        target: &str,
        note: &str,
    ) -> Result<serde_json::Value, MfaError>;
}
