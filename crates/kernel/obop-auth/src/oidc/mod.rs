//! Sign-in with Microsoft Entra (D91, Proposed): the provider's configuration, the checks of the
//! id token, the binding of a Microsoft identity to an account, the one-time code and the
//! session. The browser round trip and the two calls to the provider are in `obop-http`; this
//! crate never touches the network and is the only one that mints a session.
//!
//! Records (outside the module registry, like the other `auth_*` records):
//!
//! - `auth_sso_provider`, one record `provider`: kind, tenant, client id, the client secret sealed
//!   under `OBOP_SECRET_KEY` (purpose `sso-client-secret`), enabled, who and when. Re-sealed by
//!   `obop secrets rotate` (the inventory in `obop-keys`).
//! - `auth_sso_link`, one record per binding, keyed by the SHA-256 of `issuer|tenant|oid`, so the
//!   store's own unique key is the rule "one Microsoft identity, one account": the account, the
//!   object id and the tenant, who bound it and when. Never by e-mail, name or user principal name.
//! - `sso_only` on `auth_user`: the account has no password or passkey sign-in of its own.
//! - Short-lived: the one-time code, in the shared key-value store as `auth.sso:<SHA-256>`.
//!
//! Nothing of the provider is kept but the binding: the id token is read and dropped.

pub mod verify;

#[cfg(any(test, feature = "test-support"))]
pub mod soft;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod verify_tests;

use crate::mfa::SessionError;
use crate::webauthn::sha256;
use crate::{Auth, Change, Epoch, bump_epoch, epoch_of, hex, random_bytes, str_list};
use obop_commands::{
    ADMIN_GROUP, LoginOutcome, MfaError, Principal, SsoApi, SsoEndpoints, SsoError, SsoIdentity,
    SsoLeg, SsoProvider,
};
use obop_storage::Fields;
use serde_json::{Value as J, json};
use verify::{Expect, Jwks, VerifyError};

pub const PROVIDER_TABLE: &str = "auth_sso_provider";
pub const LINK_TABLE: &str = "auth_sso_link";
const PROVIDER_ID: &str = "provider";
/// The purpose the client secret is sealed for.
pub const SECRET_PURPOSE: &str = "sso-client-secret";
/// How long a one-time code lives, in seconds.
pub const CODE_SECS: u64 = 60;
/// Administrators that stay able to sign in locally (password and a second factor) when an account
/// is made SSO-only.
pub const BREAK_GLASS_MIN: usize = 2;
const MAX_SECRET_CHARS: usize = 512;
/// How long an anonymous refusal is written once (`audit_throttled`), in milliseconds.
const THROTTLE_MS: u64 = 10 * 60 * 1000;

/// The `OBOP_OIDC_TEST_AUTHORITY` of a development instance: a literal loopback address over
/// http that stands in for `https://login.microsoftonline.com`. For tests only, like
/// `OBOP_CONNECTORS_ALLOW_LOOPBACK` (and, like it, of no use without it: the outbound client
/// refuses a loopback address unless that is set). Anything else is ignored with a line on stderr.
pub(crate) fn authority_from_env() -> Option<String> {
    authority_from(std::env::var("OBOP_OIDC_TEST_AUTHORITY").ok().as_deref())
}

/// The checks of [`authority_from_env`] on the value, so a test can try them without the process
/// environment.
fn authority_from(value: Option<&str>) -> Option<String> {
    let v = value?.trim().trim_end_matches('/').to_string();
    if v.is_empty() || v == verify::AUTHORITY {
        return None;
    }
    if verify::authority_ok(&v) {
        eprintln!(
            "WARNING: OBOP_OIDC_TEST_AUTHORITY is set: Microsoft sign-in trusts {v} as the authority. Tests only."
        );
        Some(v)
    } else {
        eprintln!(
            "OBOP_OIDC_TEST_AUTHORITY is ignored: it must be http://127.0.0.1:<port> or http://[::1]:<port>"
        );
        None
    }
}

fn unavailable(e: impl ToString) -> SsoError {
    SsoError::Unavailable(e.to_string())
}

fn invalid(m: impl Into<String>) -> SsoError {
    SsoError::Invalid(m.into())
}

/// The key of a binding: the SHA-256 of `issuer|tenant|oid`, so the store's unique key is the rule.
pub fn link_key(issuer: &str, tenant: &str, oid: &str) -> String {
    hex(&sha256(
        format!("obop-sso-link\n{issuer}\n{tenant}\n{oid}").as_bytes(),
    ))
}

/// The shortest browser secret `begin` accepts (the cookie is 43 characters).
const MIN_BINDER: usize = 16;

fn binder_hash(binder: &str) -> String {
    hex(&sha256(format!("obop-sso-binder\n{binder}").as_bytes()))
}

fn code_key(code: &str) -> String {
    format!("auth.sso:{}", hex(&sha256(code.as_bytes())))
}

fn clean_client_id(raw: &str) -> Result<String, SsoError> {
    let c = raw.trim().to_ascii_lowercase();
    if verify::is_guid(&c) {
        Ok(c)
    } else {
        Err(invalid(
            "the client id is the application (client) id of the app registration, a GUID",
        ))
    }
}

impl Auth {
    // ---- authority ---------------------------------------------------------------------------

    /// The authority the pinned tenant lives under: Microsoft's, or in a test the stand-in.
    pub fn oidc_authority(&self) -> String {
        self.oidc_authority
            .read()
            .ok()
            .and_then(|a| a.clone())
            .unwrap_or_else(|| verify::AUTHORITY.to_string())
    }

    /// For tests: use `authority` (a loopback address) instead of Microsoft's.
    pub fn set_oidc_authority_for_test(&self, authority: &str) {
        assert!(verify::authority_ok(authority), "not an authority");
        *self.oidc_authority.write().unwrap() = Some(authority.to_string());
    }

    // ---- state ---------------------------------------------------------------------------------

    fn provider_record(&self) -> Result<Option<J>, SsoError> {
        Ok(self
            .backend
            .get(PROVIDER_TABLE, PROVIDER_ID)
            .map_err(unavailable)?
            .map(J::Object))
    }

    fn view(r: &J) -> SsoProvider {
        SsoProvider {
            kind: r["kind"].as_str().unwrap_or("entra").to_string(),
            tenant: r["tenant"].as_str().unwrap_or("").to_string(),
            client_id: r["client_id"].as_str().unwrap_or("").to_string(),
            enabled: r["enabled"].as_bool() == Some(true),
            secret_set: r["client_secret"].is_string(),
            updated_at: r["updated_at"].as_str().map(String::from),
            updated_by: r["updated_by"].as_str().map(String::from),
        }
    }

    /// Whether the account signs in with Microsoft only.
    pub(crate) fn is_sso_only(&self, name: &str) -> bool {
        self.backend
            .get("auth_user", name)
            .ok()
            .flatten()
            .is_some_and(|u| u.get("sso_only").and_then(J::as_bool) == Some(true))
    }

    /// The binding of an account as `(link id, record)`, with a lookup that reports a refusal.
    fn link_of(&self, user: &str) -> Result<Option<(String, J)>, SsoError> {
        Ok(self
            .backend
            .find(LINK_TABLE, "user", &J::from(user))
            .map_err(unavailable)?
            .into_iter()
            .next()
            .map(|(k, f)| (k, J::Object(f))))
    }

    /// A working second factor: a passkey, or an authenticator app whose secret the key ring
    /// of this server really opens (a replaced `OBOP_SECRET_KEY` leaves the flag on and the
    /// secret unreadable). A lookup the store refused is "none" (the guard fails closed).
    fn working_second_factor(&self, name: &str, u: &J) -> bool {
        self.working_totp(name, u) || self.passkeys_checked(name).is_ok_and(|p| !p.is_empty())
    }

    /// An authenticator app that is on and whose secret this server's key ring opens.
    pub(crate) fn working_totp(&self, name: &str, u: &J) -> bool {
        u["mfa_enabled"].as_bool() == Some(true)
            && u["totp_secret"]
                .as_str()
                .is_some_and(|sealed| self.open_secret(name, sealed).is_ok())
    }

    /// Whether `u` is a break-glass administrator: enabled, in the administrator group, not
    /// Microsoft-only, with a working second factor.
    fn is_break_glass(&self, name: &str, u: &J) -> bool {
        str_list(&u["groups"]).iter().any(|g| g == ADMIN_GROUP)
            && u["disabled"].as_bool() != Some(true)
            && u["sso_only"].as_bool() != Some(true)
            && self.working_second_factor(name, u)
    }

    /// How many enabled administrators other than `except` sign in without Microsoft only. The
    /// "last administrator" rule of disabling, offboarding and removing the group counts these,
    /// not the Microsoft-only ones: an administrator who has no sign-in of their own does not
    /// keep the instance reachable when Microsoft is not.
    pub(crate) fn other_local_admins(&self, except: &str) -> Result<usize, MfaError> {
        Ok(self
            .backend
            .list("auth_user")
            .map_err(MfaError::Unavailable)?
            .into_iter()
            .filter(|(name, u)| {
                name != except
                    && str_list(&u["groups"]).iter().any(|g| g == ADMIN_GROUP)
                    && u.get("disabled").and_then(J::as_bool) != Some(true)
                    && u.get("sso_only").and_then(J::as_bool) != Some(true)
            })
            .count())
    }

    /// Whether taking away the local sign-in of `target` (the account is disabled, offboarded,
    /// loses the administrator group, or its last second factor goes) would leave fewer than
    /// [`BREAK_GLASS_MIN`] break-glass administrators while an enabled account signs in with
    /// Microsoft only. `Some(why)` when it would. Only an account that is a break-glass
    /// administrator now can be the cause, so every other change passes.
    pub(crate) fn break_glass_shortfall(&self, target: &str) -> Result<Option<String>, MfaError> {
        let users = self
            .backend
            .list("auth_user")
            .map_err(MfaError::Unavailable)?;
        let microsoft_only = users
            .iter()
            .filter(|(n, u)| {
                n != target
                    && u.get("sso_only").and_then(J::as_bool) == Some(true)
                    && u.get("disabled").and_then(J::as_bool) != Some(true)
            })
            .count();
        if microsoft_only == 0 {
            return Ok(None);
        }
        let Some((_, me)) = users.iter().find(|(n, _)| n == target) else {
            return Ok(None);
        };
        if !self.is_break_glass(target, &J::Object(me.clone())) {
            return Ok(None);
        }
        let rest = self
            .break_glass(Some(target))
            .map_err(|e| MfaError::Unavailable(describe_sso(e)))?;
        if rest.len() >= BREAK_GLASS_MIN {
            return Ok(None);
        }
        Ok(Some(format!(
            "{microsoft_only} account(s) sign in with Microsoft only, and {target} is one of the administrators who can still sign in without it; {} would remain, fewer than {BREAK_GLASS_MIN}. Add another administrator with a password and a second factor first, or give the Microsoft-only accounts their own sign-in back",
            rest.len()
        )))
    }

    /// The guard for a request of a person (REST, MCP, the sign-in page): refuse. For the
    /// operator at the command line (database access is the authority, and the way out when an
    /// account is compromised) the same finding is a warning in the log and on screen.
    pub(crate) fn guard_break_glass(
        &self,
        target: &str,
        operator: bool,
    ) -> Result<Option<String>, MfaError> {
        match self.break_glass_shortfall(target)? {
            None => Ok(None),
            Some(why) if operator => {
                eprintln!("WARNING: {why}");
                Ok(Some(format!("WARNING: {why}")))
            }
            Some(why) => Err(MfaError::Invalid(why)),
        }
    }

    /// The enabled administrators other than `except` that are not SSO-only and hold a working
    /// second factor: the ones that can still sign in when Microsoft cannot (break-glass).
    fn break_glass(&self, except: Option<&str>) -> Result<Vec<String>, SsoError> {
        let mut out = Vec::new();
        for (name, u) in self.backend.list("auth_user").map_err(unavailable)? {
            let u = J::Object(u);
            if Some(name.as_str()) != except
                && str_list(&u["groups"]).iter().any(|g| g == ADMIN_GROUP)
                && u["disabled"].as_bool() != Some(true)
                && u["sso_only"].as_bool() != Some(true)
                && self.working_second_factor(&name, &u)
            {
                out.push(name);
            }
        }
        out.sort();
        Ok(out)
    }

    fn sso_event(&self, user: &str, kind: &str, actor: &str, detail: &str) {
        self.event(user, kind, actor, detail);
    }

    /// The administrator a call runs as (a person whose session is whole), and the account that
    /// administrator signs in as (an approved proposal of an agent runs as `alice/agent`).
    fn sso_admin<'a>(&self, p: &'a Principal) -> Result<(&'a str, &'a str), SsoError> {
        crate::admin::admin_of(p).map_err(|_| SsoError::NotFound)
    }

    /// The binding a one-time code was issued for, if it is still the account's and the provider
    /// that vouched for it is still on and still the tenant of that binding. Read again when the
    /// code is spent: unbinding, switching the provider off or clearing it ends a code that is
    /// in the air (the person has up to 60 seconds).
    fn binding_holds(&self, user: &str, link_id: &str) -> Result<Option<&'static str>, MfaError> {
        let link = self
            .backend
            .get(LINK_TABLE, link_id)
            .map_err(MfaError::Unavailable)?;
        let Some(link) = link.map(J::Object) else {
            return Ok(Some("the account was unbound before the sign-in finished"));
        };
        if link["user"].as_str() != Some(user) {
            return Ok(Some("the binding belongs to another account now"));
        }
        let provider = self
            .provider_record()
            .map_err(|e| MfaError::Unavailable(describe_sso(e)))?;
        match provider.map(|p| Self::view(&p)) {
            Some(v) if v.enabled && link["tid"].as_str() == Some(v.tenant.as_str()) => Ok(None),
            Some(v) if !v.enabled => Ok(Some(
                "the provider was switched off before the sign-in finished",
            )),
            Some(_) => Ok(Some(
                "the provider is of another tenant than the binding, since the sign-in began",
            )),
            None => Ok(Some("the provider was removed before the sign-in finished")),
        }
    }

    // ---- the one-time code -----------------------------------------------------------------

    fn code_take(&self, code: &str) -> Option<J> {
        let key = code_key(code);
        let raw = self.backend.kv_get(&key).ok().flatten();
        let used = self.backend.kv_delete(&key).unwrap_or(false);
        let rec: J = serde_json::from_str(&raw?).ok()?;
        (used && rec["expires_at"].as_u64().is_some_and(|e| e > self.now())).then_some(rec)
    }

    // ---- the operator's own tools (database access is the authority, as for `admin create`) ----

    /// Bind from the command line. Same rules as the administrator's.
    pub fn operator_sso_bind(&self, user: &str, oid: &str) -> Result<J, String> {
        self.bind_as("operator", user, oid).map_err(describe_sso)
    }

    pub fn operator_sso_unbind(&self, user: &str) -> Result<J, String> {
        self.unbind_as("operator", user).map_err(describe_sso)
    }

    pub fn operator_sso_only(&self, user: &str, on: bool) -> Result<J, String> {
        self.sso_only_as("operator", user, on).map_err(describe_sso)
    }

    pub fn operator_sso_configure(
        &self,
        tenant: &str,
        client_id: &str,
        client_secret: Option<&str>,
        enabled: Option<bool>,
    ) -> Result<SsoProvider, String> {
        self.configure_as("operator", tenant, client_id, client_secret, enabled)
            .map_err(describe_sso)
    }

    pub fn operator_sso_clear(&self) -> Result<(), String> {
        self.clear_as("operator").map_err(describe_sso)
    }

    /// The accounts with their binding, and the break-glass administrators, for the command line.
    pub fn operator_sso_status(&self) -> Result<J, String> {
        self.bindings_impl().map_err(describe_sso)
    }

    // ---- configuration ------------------------------------------------------------------------

    fn configure_as(
        &self,
        actor: &str,
        tenant: &str,
        client_id: &str,
        client_secret: Option<&str>,
        enabled: Option<bool>,
    ) -> Result<SsoProvider, SsoError> {
        let tenant = verify::tenant_id(tenant).map_err(SsoError::Invalid)?;
        let client_id = clean_client_id(client_id)?;
        let _g = self
            .mfa_guard()
            .map_err(|e| unavailable(format!("{e:?}")))?;
        let before = self.provider_record()?;
        let sealed = match client_secret {
            Some(s) => {
                let s = s.trim();
                if s.is_empty()
                    || s.chars().count() > MAX_SECRET_CHARS
                    || s.chars().any(char::is_control)
                {
                    return Err(invalid(
                        "the client secret is empty, too long or has control characters",
                    ));
                }
                if self.config.keys.is_empty() {
                    return Err(unavailable(
                        "the client secret is sealed with the server secret OBOP_SECRET_KEY, which is not set",
                    ));
                }
                Some(
                    self.config
                        .keys
                        .seal_str(SECRET_PURPOSE, s)
                        .map_err(|e| unavailable(String::from(e)))?,
                )
            }
            None => before
                .as_ref()
                .and_then(|b| b["client_secret"].as_str())
                .map(String::from),
        };
        if sealed.is_none() {
            return Err(invalid("a client secret is needed"));
        }
        // Whatever the record says (it may have been cleared): no binding of another tenant stays
        // under a provider of this one. Such a binding could never match a sign-in, and an account
        // made Microsoft-only on it would be locked out.
        let foreign = self
            .backend
            .list(LINK_TABLE)
            .map_err(unavailable)?
            .into_iter()
            .filter(|(_, l)| l.get("tid").and_then(J::as_str) != Some(tenant.as_str()))
            .count();
        if foreign > 0 {
            return Err(invalid(format!(
                "{foreign} account(s) are bound to another tenant: unbind them before this tenant is set"
            )));
        }
        // A first configuration is switched on; a later one keeps the switch unless it is given.
        let enabled = enabled.unwrap_or_else(|| {
            before
                .as_ref()
                .is_none_or(|b| b["enabled"].as_bool() == Some(true))
        });
        let mut f = Fields::new();
        f.insert("kind".into(), J::from("entra"));
        f.insert("tenant".into(), J::from(tenant.as_str()));
        f.insert("client_id".into(), J::from(client_id.as_str()));
        f.insert("client_secret".into(), sealed.map_or(J::Null, J::from));
        f.insert("enabled".into(), J::Bool(enabled));
        f.insert(
            "updated_at".into(),
            J::from(obop_commands::audit::rfc3339(self.now())),
        );
        f.insert("updated_by".into(), J::from(actor));
        if before.is_some() {
            self.backend
                .update(PROVIDER_TABLE, PROVIDER_ID, &f)
                .map_err(unavailable)?;
        } else {
            self.backend
                .insert(PROVIDER_TABLE, PROVIDER_ID, &f)
                .map_err(unavailable)?;
        }
        self.sso_event(
            "-",
            "oidc_provider_configured",
            actor,
            &format!(
                "tenant {tenant}, client id {client_id}, {}; client secret {}",
                if enabled { "enabled" } else { "not enabled" },
                if client_secret.is_some() {
                    "replaced"
                } else {
                    "kept"
                }
            ),
        );
        Ok(Self::view(&J::Object(f)))
    }

    fn clear_as(&self, actor: &str) -> Result<(), SsoError> {
        let _g = self
            .mfa_guard()
            .map_err(|e| unavailable(format!("{e:?}")))?;
        if self.provider_record()?.is_none() {
            return Err(invalid("no provider is configured"));
        }
        let only = self
            .backend
            .list("auth_user")
            .map_err(unavailable)?
            .iter()
            .filter(|(_, u)| u.get("sso_only").and_then(J::as_bool) == Some(true))
            .count();
        self.backend
            .delete(PROVIDER_TABLE, PROVIDER_ID)
            .map_err(unavailable)?;
        self.sso_event(
            "-",
            "oidc_provider_cleared",
            actor,
            &format!("the provider's configuration was removed; {only} SSO-only account(s) can sign in only through the break-glass administrators until it is configured again"),
        );
        Ok(())
    }

    // ---- binding ---------------------------------------------------------------------------------

    fn bind_as(&self, actor: &str, user: &str, oid: &str) -> Result<J, SsoError> {
        let user = user.trim().to_lowercase();
        let oid = oid.trim().to_ascii_lowercase();
        if !verify::is_guid(&oid) {
            return Err(invalid(
                "the object id is the user's object id in the directory, a GUID",
            ));
        }
        let _g = self
            .mfa_guard()
            .map_err(|e| unavailable(format!("{e:?}")))?;
        // Read under the lock: a provider changed in between cannot get a binding of its old tenant.
        let provider = self
            .provider_record()?
            .ok_or_else(|| invalid("configure the provider before binding an account"))?;
        let tenant = provider["tenant"].as_str().unwrap_or("").to_string();
        let issuer = verify::issuer(&self.oidc_authority(), &tenant);
        let u = self.user_record(&user).map_err(|e| match e {
            MfaError::NotFound => invalid(format!("no user {user}")),
            other => unavailable(format!("{other:?}")),
        })?;
        if u["disabled"].as_bool() == Some(true) {
            return Err(invalid("the account is disabled: enable it first"));
        }
        if self.link_of(&user)?.is_some() {
            return Err(invalid(format!(
                "{user} is bound to a Microsoft identity already: unbind it first"
            )));
        }
        let id = link_key(&issuer, &tenant, &oid);
        let mut f = Fields::new();
        f.insert("user".into(), J::from(user.as_str()));
        f.insert("issuer".into(), J::from(issuer.as_str()));
        f.insert("tid".into(), J::from(tenant.as_str()));
        f.insert("oid".into(), J::from(oid.as_str()));
        f.insert(
            "bound_at".into(),
            J::from(obop_commands::audit::rfc3339(self.now())),
        );
        f.insert("bound_by".into(), J::from(actor));
        self.backend.insert(LINK_TABLE, &id, &f).map_err(|e| {
            if e.contains("duplicate") || e.contains("UNIQUE") || e.contains("unique") {
                invalid("that object id is bound to another account already")
            } else {
                unavailable(e)
            }
        })?;
        self.sso_event(
            &user,
            "oidc_bind",
            actor,
            &format!("bound to object id {oid} in tenant {tenant}"),
        );
        Ok(json!({"user": user, "oid": oid, "tenant": tenant}))
    }

    fn unbind_as(&self, actor: &str, user: &str) -> Result<J, SsoError> {
        let user = user.trim().to_lowercase();
        let _g = self
            .mfa_guard()
            .map_err(|e| unavailable(format!("{e:?}")))?;
        let u = self.user_record(&user).map_err(|e| match e {
            MfaError::NotFound => invalid(format!("no user {user}")),
            other => unavailable(format!("{other:?}")),
        })?;
        if u["sso_only"].as_bool() == Some(true) {
            return Err(invalid(
                "the account signs in with Microsoft only: turn that off before unbinding it",
            ));
        }
        let ch = Change::new(&user, "oidc_unbind", actor, "unbound from Microsoft");
        let Some((id, link)) = self.link_of(&user)? else {
            // The repeat of an unbinding that could not end the sessions finishes that (D74).
            if let Some(n) = self
                .finish_owed(&user, "sso_unbind", &ch)
                .map_err(|e| unavailable(format!("{e:?}")))?
            {
                self.sso_event(
                    &user,
                    "oidc_unbind",
                    actor,
                    &format!("unbound; {n} session(s) signed out (repeated)"),
                );
                return Ok(json!({"user": user, "changed": false, "sessions_ended": n}));
            }
            return Err(invalid(format!(
                "{user} is not bound to a Microsoft identity"
            )));
        };
        // What a binding gave the account (sessions, and a sign-in in the air) ends with it. The
        // note is written first, the credential epoch must change (a code issued before this
        // cannot be spent after it), then the binding goes and the sessions end.
        let previous = self
            .owe_revocation(&user, "sso_unbind", None)
            .map_err(|e| unavailable(format!("{e:?}")))?;
        let before = obop_storage::fault::change_count();
        let mut p = Fields::new();
        bump_epoch(&mut p);
        if let Err(e) = self.put(&user, &p) {
            self.retract_owed(&user, "sso_unbind", &e, before, previous);
            return Err(unavailable(format!("{e:?}")));
        }
        self.backend.delete(LINK_TABLE, &id).map_err(unavailable)?;
        let n = self
            .revoke_user_sessions(&user, None, &ch)
            .map_err(|e| unavailable(format!("{e:?}")))?;
        self.clear_owed(&user, "sso_unbind");
        self.sso_event(
            &user,
            "oidc_unbind",
            actor,
            &format!(
                "unbound from object id {}; {n} session(s) signed out; agent tokens made in those sessions keep working until the account is offboarded",
                link["oid"].as_str().unwrap_or("?")
            ),
        );
        Ok(json!({"user": user, "oid": link["oid"], "changed": true, "sessions_ended": n}))
    }

    fn sso_only_as(&self, actor: &str, user: &str, on: bool) -> Result<J, SsoError> {
        let user = user.trim().to_lowercase();
        let _g = self
            .mfa_guard()
            .map_err(|e| unavailable(format!("{e:?}")))?;
        let u = self.user_record(&user).map_err(|e| match e {
            MfaError::NotFound => invalid(format!("no user {user}")),
            other => unavailable(format!("{other:?}")),
        })?;
        let was = u["sso_only"].as_bool() == Some(true);
        let ch = Change::new(
            &user,
            "oidc_sso_only",
            actor,
            "signs in with Microsoft only",
        );
        if was == on {
            // The repeat of a request that changed the flag and could not end the sessions
            // finishes that (D74).
            if on
                && let Some(n) = self
                    .finish_owed(&user, "sso_only", &ch)
                    .map_err(|e| unavailable(format!("{e:?}")))?
            {
                self.sso_event(
                    &user,
                    "oidc_sso_only",
                    actor,
                    &format!("signs in with Microsoft only; {n} session(s) signed out (repeated)"),
                );
            }
            return Ok(json!({"user": user, "sso_only": on, "changed": false}));
        }
        if !on {
            let mut p = Fields::new();
            p.insert("sso_only".into(), J::Bool(false));
            // A password that has been unused since the account was made Microsoft-only is not
            // handed back as it was: the person has to choose a new one.
            p.insert("must_change_password".into(), J::Bool(true));
            bump_epoch(&mut p);
            self.put(&user, &p)
                .map_err(|e| unavailable(format!("{e:?}")))?;
            self.sso_event(
                &user,
                "oidc_sso_only",
                actor,
                "turned off: the account may use its password again (it must be changed at the next sign-in)",
            );
            return Ok(json!({"user": user, "sso_only": false, "changed": true}));
        }
        // Turning it on.
        let provider = self.provider_record()?;
        if !provider.as_ref().is_some_and(|p| Self::view(p).enabled) {
            return Err(invalid(
                "the Microsoft sign-in is not enabled: an account cannot be made SSO-only without it",
            ));
        }
        if u["disabled"].as_bool() == Some(true) {
            return Err(invalid("the account is disabled: enable it first"));
        }
        let Some((_, link)) = self.link_of(&user)? else {
            return Err(invalid(format!(
                "{user} is not bound to a Microsoft identity: bind it first, or the person could not sign in"
            )));
        };
        let current = provider.as_ref().map(|p| Self::view(p).tenant);
        if link["tid"].as_str() != current.as_deref()
            || link["issuer"].as_str()
                != current
                    .as_deref()
                    .map(|t| verify::issuer(&self.oidc_authority(), t))
                    .as_deref()
        {
            return Err(invalid(format!(
                "the binding of {user} is not to the tenant of the provider (the provider was changed after it was made): unbind and bind again"
            )));
        }
        // The first real use of a binding proves the object id is the person's and that Entra
        // sends what this server needs (the profile scope, the optional claims): without it a
        // wrong id or an app registration that does not work would lock the person out.
        if link["last_login_at"].as_str().is_none() {
            return Err(invalid(format!(
                "{user} has never signed in with Microsoft on this binding: ask them to sign in once with the button first, so that a wrong object id or an app registration that does not work shows now and not as a lockout"
            )));
        }
        let rest = self.break_glass(Some(&user))?;
        if rest.len() < BREAK_GLASS_MIN {
            return Err(invalid(format!(
                "at least {BREAK_GLASS_MIN} other enabled administrators must keep signing in locally (password and an authenticator app or a passkey, not SSO-only) before an account is made SSO-only; there are {} now{}",
                rest.len(),
                if rest.is_empty() {
                    String::new()
                } else {
                    format!(": {}", rest.join(", "))
                }
            )));
        }
        let previous = self
            .owe_revocation(&user, "sso_only", None)
            .map_err(|e| unavailable(format!("{e:?}")))?;
        let before = obop_storage::fault::change_count();
        let mut p = Fields::new();
        p.insert("sso_only".into(), J::Bool(true));
        // A temporary password the person can no longer use would limit every session to a
        // password change.
        p.insert("must_change_password".into(), J::Bool(false));
        bump_epoch(&mut p);
        if let Err(e) = self.put(&user, &p) {
            self.retract_owed(&user, "sso_only", &e, before, previous);
            return Err(unavailable(format!("{e:?}")));
        }
        let n = self
            .revoke_user_sessions(&user, None, &ch)
            .map_err(|e| unavailable(format!("{e:?}")))?;
        self.clear_owed(&user, "sso_only");
        self.sso_event(
            &user,
            "oidc_sso_only",
            actor,
            &format!(
                "signs in with Microsoft only; {n} session(s) signed out; {} break-glass administrator(s) remain",
                rest.len()
            ),
        );
        Ok(json!({"user": user, "sso_only": true, "changed": true, "break_glass": rest}))
    }

    fn bindings_impl(&self) -> Result<J, SsoError> {
        let links = self.backend.list(LINK_TABLE).map_err(unavailable)?;
        let provider_tenant = self.provider_record()?.map(|r| Self::view(&r).tenant);
        let mut rows: Vec<J> = Vec::new();
        for (name, u) in self.backend.list("auth_user").map_err(unavailable)? {
            let link = links
                .iter()
                .find(|(_, l)| l.get("user").and_then(J::as_str) == Some(name.as_str()));
            let current = provider_tenant.as_deref();
            rows.push(json!({
                "name": name,
                "bound": link.is_some(),
                "oid": link.and_then(|(_, l)| l.get("oid")).cloned().unwrap_or(J::Null),
                "sso_only": u.get("sso_only").and_then(J::as_bool) == Some(true),
                // A binding of another tenant than the provider's can never match a sign-in.
                "binding_current": link.map(|(_, l)| l.get("tid").and_then(J::as_str) == current),
                "last_login_at": link.and_then(|(_, l)| l.get("last_login_at")).cloned().unwrap_or(J::Null),
            }));
        }
        rows.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        let provider = self.provider_record()?.map(|r| Self::view(&r));
        let bg = self.break_glass(None)?;
        Ok(json!({
            "provider": provider.map(|p| json!({
                "kind": p.kind, "tenant": p.tenant, "client_id": p.client_id,
                "enabled": p.enabled, "secret_set": p.secret_set,
            })),
            "accounts": rows,
            "break_glass": bg,
            "break_glass_enough": bg.len() >= BREAK_GLASS_MIN,
        }))
    }
}

fn describe_sso(e: SsoError) -> String {
    match e {
        SsoError::Refused(m) | SsoError::Invalid(m) | SsoError::Unavailable(m) => m,
        SsoError::UnknownKey => "the signing key is not known".into(),
        SsoError::NotFound => "not allowed".into(),
    }
}

impl SsoApi for Auth {
    fn offered(&self) -> bool {
        self.provider_record()
            .ok()
            .flatten()
            .is_some_and(|r| Self::view(&r).enabled && r["client_secret"].is_string())
    }

    fn provider(&self) -> Result<Option<SsoProvider>, SsoError> {
        Ok(self.provider_record()?.map(|r| Self::view(&r)))
    }

    fn leg(&self) -> Result<Option<SsoLeg>, SsoError> {
        let Some(r) = self.provider_record()? else {
            return Ok(None);
        };
        let v = Self::view(&r);
        if !v.enabled {
            return Ok(None);
        }
        let sealed = r["client_secret"]
            .as_str()
            .ok_or_else(|| unavailable("no client secret is stored"))?;
        if self.config.keys.is_empty() {
            return Err(unavailable("the server secret OBOP_SECRET_KEY is not set"));
        }
        let secret = self
            .config
            .keys
            .open_str(SECRET_PURPOSE, sealed)
            .map_err(|_| {
                unavailable("the client secret could not be opened with the server key ring")
            })?;
        // A tenant stored by a hand that skipped the checks is not used.
        let tenant = verify::tenant_id(&v.tenant)
            .map_err(|_| unavailable("the stored tenant is not valid"))?;
        Ok(Some(SsoLeg {
            authority: self.oidc_authority(),
            tenant,
            client_id: v.client_id,
            client_secret: secret,
        }))
    }

    fn endpoints(&self, discovery: &[u8]) -> Result<SsoEndpoints, SsoError> {
        let p = self
            .provider_record()?
            .ok_or_else(|| unavailable("no provider is configured"))?;
        let tenant = verify::tenant_id(p["tenant"].as_str().unwrap_or(""))
            .map_err(|_| unavailable("the stored tenant is not valid"))?;
        let e = verify::parse_discovery(discovery, &self.oidc_authority(), &tenant)
            .map_err(SsoError::Refused)?;
        Ok(SsoEndpoints {
            authorization: e.authorization,
            token: e.token,
            jwks: e.jwks,
        })
    }

    fn verify(&self, id_token: &str, jwks: &[u8], nonce: &str) -> Result<SsoIdentity, SsoError> {
        let p = self
            .provider_record()?
            .ok_or_else(|| unavailable("no provider is configured"))?;
        let tenant = verify::tenant_id(p["tenant"].as_str().unwrap_or(""))
            .map_err(|_| unavailable("the stored tenant is not valid"))?;
        let client_id = p["client_id"].as_str().unwrap_or("");
        let keys = Jwks::parse(jwks).map_err(unavailable)?;
        let authority = self.oidc_authority();
        let exp = Expect {
            authority: &authority,
            tenant: &tenant,
            client_id,
            nonce,
            now: self.now(),
        };
        verify::verify_id_token(id_token, &keys, &exp).map_err(|e| match e {
            VerifyError::UnknownKey => SsoError::UnknownKey,
            VerifyError::Refused(why) => SsoError::Refused(why.to_string()),
        })
    }

    fn begin(&self, identity: &SsoIdentity, binder: &str) -> Result<String, SsoError> {
        if binder.len() < MIN_BINDER {
            return Err(invalid("the browser secret of the sign-in is missing"));
        }
        let refuse = |user: &str, why: &str| -> SsoError {
            self.event(user, "oidc_refused", "-", why);
            SsoError::Refused(why.to_string())
        };
        // The tenant of the identity is the pinned one (verified), but a provider changed since
        // the flow began is not the one it was checked against.
        let Some(p) = self.provider_record()? else {
            return Err(refuse("-", "no provider is configured"));
        };
        let v = Self::view(&p);
        if !v.enabled || v.tenant != identity.tenant {
            return Err(refuse(
                "-",
                "the provider was changed or switched off during the sign-in",
            ));
        }
        let id = link_key(&identity.issuer, &identity.tenant, &identity.oid);
        let link = self.backend.get(LINK_TABLE, &id).map_err(unavailable)?;
        let Some(link) = link.map(J::Object) else {
            return Err(refuse(
                "-",
                &format!(
                    "no account is bound to object id {} in tenant {}",
                    identity.oid, identity.tenant
                ),
            ));
        };
        let user = link["user"].as_str().unwrap_or("").to_string();
        let u = match self.user_record(&user) {
            Ok(u) => u,
            Err(MfaError::NotFound) => {
                return Err(refuse(&user, "the account of the binding does not exist"));
            }
            Err(e) => return Err(unavailable(format!("{e:?}"))),
        };
        if u["disabled"].as_bool() == Some(true) {
            return Err(refuse(&user, "the account is disabled"));
        }
        let code = format!("obopsso_{}", hex(&random_bytes::<32>()));
        let rec = json!({
            "user": user,
            "link": id,
            "epoch": epoch_of(&u),
            "mfa": identity.mfa(),
            "binder": binder_hash(binder),
            "expires_at": self.now() + CODE_SECS,
        });
        self.backend
            .kv_put(&code_key(&code), &rec.to_string(), Some(CODE_SECS * 1000))
            .map_err(unavailable)?;
        Ok(code)
    }

    fn exchange(&self, code: &str, binder: &str) -> Result<(String, LoginOutcome), MfaError> {
        let rec = self.code_take(code).ok_or(MfaError::Expired)?;
        let name = rec["user"].as_str().ok_or(MfaError::Expired)?.to_string();
        // The code is spent. Presented by another browser than the one that signed in, it is
        // worth nothing: a link that carries a code to someone else signs that person in as the
        // one who made the link.
        let held = rec["binder"].as_str().unwrap_or("");
        if binder.len() < MIN_BINDER
            || !crate::totp::ct_eq(held.as_bytes(), binder_hash(binder).as_bytes())
        {
            self.event(
                &name,
                "oidc_refused",
                "-",
                "the one-time code was presented by another browser than the one that signed in",
            );
            return Err(MfaError::Expired);
        }
        let pending: Epoch = rec["epoch"].as_str().map(String::from);
        let mfa = rec["mfa"].as_bool() == Some(true);
        // The account as it is now: disabled, removed or changed since the callback is no sign-in.
        let u = self.user_record(&name).map_err(|e| match e {
            MfaError::NotFound => MfaError::Expired,
            other => other,
        })?;
        if u["disabled"].as_bool() == Some(true) {
            self.event(
                &name,
                "oidc_refused",
                "-",
                "the account was disabled before the sign-in finished",
            );
            return Err(MfaError::Expired);
        }
        if epoch_of(&u) != pending {
            self.event(
                &name,
                "oidc_refused",
                "-",
                "the credentials of the account changed before the sign-in finished",
            );
            return Err(MfaError::Expired);
        }
        // The binding and the provider as they are now, not as they were when the code was made.
        if let Some(why) = self.binding_holds(&name, rec["link"].as_str().unwrap_or(""))? {
            self.event(&name, "oidc_refused", "-", why);
            return Err(MfaError::Expired);
        }
        // The first sign-in on a binding is what lets an administrator make the account
        // Microsoft-only. A refused write only means the administrator is asked to sign in again.
        if let Ok(Some((id, _))) = self.link_of(&name) {
            let mut p = Fields::new();
            p.insert(
                "last_login_at".into(),
                J::from(obop_commands::audit::rfc3339(self.now())),
            );
            let _ = self.backend.update(LINK_TABLE, &id, &p);
        }
        let sso_only = u["sso_only"].as_bool() == Some(true);
        self.purge_tokens();
        let out = if sso_only && mfa {
            // Microsoft says a second factor was used, and for an account that has no other way in
            // that is its second factor: whole session, no obop code asked.
            self.session_outcome(&name, false, &pending)
                .map_err(SessionError::mfa)?
        } else {
            // A refusal at this point (the account went while the sign-in finished) is answered
            // like any other expired sign-in; only the store's own trouble is a 503.
            self.after_first_factor(&name, &pending).map_err(|e| {
                if e == "invalid username or password" {
                    MfaError::Expired
                } else {
                    MfaError::Unavailable(e)
                }
            })?
        };
        self.event(
            &name,
            "oidc_login",
            &name,
            &format!(
                "signed in with Microsoft; the provider {} a second factor{}",
                if mfa { "reports" } else { "does not report" },
                match &out {
                    LoginOutcome::Session {
                        mfa_enrolment_required: true,
                        ..
                    } => "; the session is limited to enrolment",
                    LoginOutcome::MfaRequired { .. } => "; an obop second factor is asked",
                    _ => "",
                }
            ),
        );
        Ok((name, out))
    }

    fn audit(&self, user: &str, kind: &str, detail: &str) {
        // Only the kinds of this sign-in; a caller cannot write any other entry.
        let kind = match kind {
            "oidc_refused" | "oidc_start" | "oidc_cancelled" | "oidc_error" => kind,
            _ => "oidc_error",
        };
        self.event(user, kind, "-", detail);
    }

    fn audit_throttled(&self, source: &str, kind: &str, detail: &str) {
        let kind = match kind {
            "oidc_refused" | "oidc_start" | "oidc_cancelled" | "oidc_error" => kind,
            _ => "oidc_error",
        };
        // One event per source, kind and text in ten minutes, whichever process sees it first;
        // the store counts the others. An unreadable counter writes the event (an unthrottled
        // line is better than a missing one).
        let key = format!(
            "auth.evt:sso:{}",
            hex(&sha256(format!("{source}\n{kind}\n{detail}").as_bytes()))
        );
        let first = self
            .backend
            .counter_add(&key, THROTTLE_MS, false)
            .map_or(true, |n| n == 1);
        if first {
            self.event(
                "-",
                kind,
                "-",
                &format!("{detail} (more of the same from this address in the next 10 minutes are counted, not written)"),
            );
        }
    }

    fn configure(
        &self,
        admin: &Principal,
        tenant: &str,
        client_id: &str,
        client_secret: Option<&str>,
        enabled: Option<bool>,
    ) -> Result<SsoProvider, SsoError> {
        let (actor, _) = self.sso_admin(admin)?;
        self.configure_as(actor, tenant, client_id, client_secret, enabled)
    }

    fn clear(&self, admin: &Principal) -> Result<(), SsoError> {
        let (actor, _) = self.sso_admin(admin)?;
        self.clear_as(actor)
    }

    fn bind(&self, admin: &Principal, user: &str, oid: &str) -> Result<J, SsoError> {
        let (actor, _) = self.sso_admin(admin)?;
        self.bind_as(actor, user, oid)
    }

    fn unbind(&self, admin: &Principal, user: &str) -> Result<J, SsoError> {
        let (actor, _) = self.sso_admin(admin)?;
        self.unbind_as(actor, user)
    }

    fn set_sso_only(&self, admin: &Principal, user: &str, on: bool) -> Result<J, SsoError> {
        let (actor, _) = self.sso_admin(admin)?;
        self.sso_only_as(actor, user, on)
    }

    fn bindings(&self, admin: &Principal) -> Result<J, SsoError> {
        self.sso_admin(admin)?;
        self.bindings_impl()
    }
}
