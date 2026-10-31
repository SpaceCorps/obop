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

/// The `OBOP_OIDC_TEST_AUTHORITY` of a development instance: a literal loopback address over
/// http that stands in for `https://login.microsoftonline.com`. For tests only, like
/// `OBOP_CONNECTORS_ALLOW_LOOPBACK` (and, like it, of no use without it: the outbound client
/// refuses a loopback address unless that is set). Anything else is ignored with a line on stderr.
pub(crate) fn authority_from_env() -> Option<String> {
    let v = std::env::var("OBOP_OIDC_TEST_AUTHORITY").ok()?;
    let v = v.trim().trim_end_matches('/').to_string();
    if v.is_empty() {
        return None;
    }
    if v == verify::AUTHORITY {
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

    /// A working second factor: a passkey, or an authenticator app when the key ring that opens
    /// its secret is there. A lookup the store refused is "none" (the guard fails closed).
    fn working_second_factor(&self, name: &str, u: &J) -> bool {
        (u["mfa_enabled"].as_bool() == Some(true) && !self.config.keys.is_empty())
            || self.passkeys_checked(name).is_ok_and(|p| !p.is_empty())
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
        enabled: bool,
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
        enabled: bool,
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
        if let Some(b) = &before
            && b["tenant"].as_str() != Some(tenant.as_str())
            && b["tenant"].is_string()
            && !self
                .backend
                .list(LINK_TABLE)
                .map_err(unavailable)?
                .is_empty()
        {
            return Err(invalid(
                "accounts are bound to the current tenant: unbind them before the tenant changes",
            ));
        }
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
        let provider = self
            .provider_record()?
            .ok_or_else(|| invalid("configure the provider before binding an account"))?;
        let tenant = provider["tenant"].as_str().unwrap_or("").to_string();
        let issuer = verify::issuer(&self.oidc_authority(), &tenant);
        let _g = self
            .mfa_guard()
            .map_err(|e| unavailable(format!("{e:?}")))?;
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
        let Some((id, link)) = self.link_of(&user)? else {
            return Err(invalid(format!(
                "{user} is not bound to a Microsoft identity"
            )));
        };
        self.backend.delete(LINK_TABLE, &id).map_err(unavailable)?;
        // A sign-in that began with the binding must not finish after it: the epoch changes.
        let mut p = Fields::new();
        bump_epoch(&mut p);
        let _ = self.put(&user, &p);
        self.sso_event(
            &user,
            "oidc_unbind",
            actor,
            &format!(
                "unbound from object id {}",
                link["oid"].as_str().unwrap_or("?")
            ),
        );
        Ok(json!({"user": user, "oid": link["oid"]}))
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
            bump_epoch(&mut p);
            self.put(&user, &p)
                .map_err(|e| unavailable(format!("{e:?}")))?;
            self.sso_event(
                &user,
                "oidc_sso_only",
                actor,
                "turned off: the account may use its password again",
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
        if self.link_of(&user)?.is_none() {
            return Err(invalid(format!(
                "{user} is not bound to a Microsoft identity: bind it first, or the person could not sign in"
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
        let mut rows: Vec<J> = Vec::new();
        for (name, u) in self.backend.list("auth_user").map_err(unavailable)? {
            let link = links
                .iter()
                .find(|(_, l)| l.get("user").and_then(J::as_str) == Some(name.as_str()));
            rows.push(json!({
                "name": name,
                "bound": link.is_some(),
                "oid": link.and_then(|(_, l)| l.get("oid")).cloned().unwrap_or(J::Null),
                "sso_only": u.get("sso_only").and_then(J::as_bool) == Some(true),
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

    fn begin(&self, identity: &SsoIdentity, ip: &str) -> Result<String, SsoError> {
        let _ = ip;
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
            "epoch": epoch_of(&u),
            "mfa": identity.mfa(),
            "expires_at": self.now() + CODE_SECS,
        });
        self.backend
            .kv_put(&code_key(&code), &rec.to_string(), Some(CODE_SECS * 1000))
            .map_err(unavailable)?;
        Ok(code)
    }

    fn exchange(&self, code: &str) -> Result<LoginOutcome, MfaError> {
        let rec = self.code_take(code).ok_or(MfaError::Expired)?;
        let name = rec["user"].as_str().ok_or(MfaError::Expired)?.to_string();
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
        let sso_only = u["sso_only"].as_bool() == Some(true);
        self.purge_tokens();
        let out = if sso_only && mfa {
            // Microsoft says a second factor was used, and for an account that has no other way in
            // that is its second factor: whole session, no obop code asked.
            self.session_outcome(&name, false, &pending)
                .map_err(SessionError::mfa)?
        } else {
            self.after_first_factor(&name, &pending)
                .map_err(MfaError::Unavailable)?
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
        Ok(out)
    }

    fn audit(&self, user: &str, kind: &str, detail: &str) {
        // Only the kinds of this sign-in; a caller cannot write any other entry.
        let kind = match kind {
            "oidc_refused" | "oidc_start" | "oidc_cancelled" | "oidc_error" => kind,
            _ => "oidc_error",
        };
        self.event(user, kind, "-", detail);
    }

    fn configure(
        &self,
        admin: &Principal,
        tenant: &str,
        client_id: &str,
        client_secret: Option<&str>,
        enabled: bool,
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
