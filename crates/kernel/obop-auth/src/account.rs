//! Sessions, password and account management of the signed-in human, and the administrator's
//! view of the accounts (B7, D59). Every change that matters is written to the security event
//! log (`security_event`) with who did it; nothing logged or returned is a secret.

use crate::{Auth, hex, random_bytes, str_list, token_hash};
use obop_commands::audit::rfc3339;
use obop_commands::{ADMIN_GROUP, MfaError, Principal, PrincipalKind};
use obop_storage::Fields;
use serde_json::{Value as J, json};

/// Shortest password anyone may choose.
pub const MIN_PASSWORD_CHARS: usize = 12;

fn person(p: &Principal) -> Result<&str, MfaError> {
    // Agent tokens are not people: they have no password, sessions or accounts to manage.
    if p.kind == PrincipalKind::User {
        Ok(&p.id)
    } else {
        Err(MfaError::NotFound)
    }
}

fn admin_name(p: &Principal) -> Result<&str, MfaError> {
    let n = person(p)?;
    if p.groups.contains(ADMIN_GROUP) && !p.mfa_limited {
        Ok(n)
    } else {
        Err(MfaError::NotFound)
    }
}

impl Auth {
    /// Whether an administrator disabled the account.
    pub(crate) fn is_disabled(&self, name: &str) -> bool {
        self.backend
            .get("auth_user", name)
            .ok()
            .flatten()
            .is_some_and(|u| u.get("disabled").and_then(J::as_bool) == Some(true))
    }

    pub(crate) fn must_change_password(&self, name: &str) -> bool {
        self.backend
            .get("auth_user", name)
            .ok()
            .flatten()
            .is_some_and(|u| u.get("must_change_password").and_then(J::as_bool) == Some(true))
    }

    /// Revoke the sessions of a user, all but the one whose token hash is `keep`. Agent tokens
    /// are separate credentials and are not touched. Returns how many ended.
    pub(crate) fn revoke_user_sessions(&self, name: &str, keep: Option<&str>) -> usize {
        let mut n = 0;
        for (id, t) in self.backend.list("auth_token").unwrap_or_default() {
            let t = J::Object(t);
            if t["owner"].as_str() == Some(name)
                && t["kind"].as_str() == Some("user")
                && t["revoked"].as_bool() != Some(true)
                && keep != Some(id.as_str())
            {
                let mut p = Fields::new();
                p.insert("revoked".into(), J::Bool(true));
                if self.backend.update("auth_token", &id, &p).unwrap_or(false) {
                    n += 1;
                }
            }
        }
        n
    }

    /// Remove tokens that ended a day ago or more, and spent second-step tokens. Runs at most
    /// once every ten minutes across all server processes.
    pub(crate) fn purge_tokens(&self) {
        let first = self
            .backend
            .counter_add("auth.purge", 600_000, false)
            .is_ok_and(|n| n == 1);
        if !first {
            return;
        }
        let cutoff = self.now().saturating_sub(86_400);
        for (id, t) in self.backend.list("auth_token").unwrap_or_default() {
            let ended = t
                .get("expires_at")
                .and_then(J::as_u64)
                .is_some_and(|e| e < cutoff)
                || (t.get("revoked").and_then(J::as_bool) == Some(true)
                    && t.get("created_at")
                        .and_then(J::as_u64)
                        .is_none_or(|c| c < cutoff));
            if ended {
                let _ = self.backend.delete("auth_token", &id);
            }
        }
        for (id, t) in self.backend.list("auth_mfa_token").unwrap_or_default() {
            if t.get("expires_at")
                .and_then(J::as_u64)
                .is_none_or(|e| e < cutoff)
            {
                let _ = self.backend.delete("auth_mfa_token", &id);
            }
        }
    }

    // ---- the person's own sessions ----------------------------------------------------------

    pub(crate) fn sessions_impl(&self, p: &Principal, session: &str) -> Result<J, MfaError> {
        let name = person(p)?;
        let (now, current) = (self.now(), token_hash(session));
        let mut rows: Vec<(u64, J)> = self
            .backend
            .list("auth_token")
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, t)| {
                t.get("owner").and_then(J::as_str) == Some(name)
                    && t.get("kind").and_then(J::as_str) == Some("user")
                    && t.get("revoked").and_then(J::as_bool) != Some(true)
                    && t.get("expires_at")
                        .and_then(J::as_u64)
                        .is_none_or(|e| e > now)
            })
            .map(|(id, t)| {
                let created = t.get("created_at").and_then(J::as_u64);
                (
                    created.unwrap_or(0),
                    json!({
                        // A prefix of the hash that is stored; it cannot be turned into the token.
                        "id": &id[..12.min(id.len())],
                        "created_at": created.map(rfc3339),
                        "expires_at": t.get("expires_at").and_then(J::as_u64).map(rfc3339),
                        "current": id == current,
                        "limited": t.get("limited").and_then(J::as_bool) == Some(true),
                    }),
                )
            })
            .collect();
        rows.sort_by_key(|r| std::cmp::Reverse(r.0));
        Ok(json!({"user": name, "sessions": rows.into_iter().map(|(_, r)| r).collect::<Vec<_>>()}))
    }

    pub(crate) fn revoke_sessions_impl(
        &self,
        p: &Principal,
        session: &str,
        keep_current: bool,
    ) -> Result<usize, MfaError> {
        let name = person(p)?;
        let h = token_hash(session);
        let n = self.revoke_user_sessions(name, keep_current.then_some(h.as_str()));
        self.event(
            name,
            "sessions_revoked",
            name,
            &format!(
                "{n} session(s) signed out{}",
                if keep_current { ", this one kept" } else { "" }
            ),
        );
        Ok(n)
    }

    // ---- password -----------------------------------------------------------------------------

    pub(crate) fn change_password_impl(
        &self,
        p: &Principal,
        session: &str,
        current: &str,
        new: &str,
        code: Option<&str>,
        recovery: Option<&str>,
    ) -> Result<(), MfaError> {
        let name = person(p)?;
        let _g = self.mfa_guard()?;
        if self.locked_out(name) {
            return Err(MfaError::Locked(
                "too many attempts, try again in a minute".into(),
            ));
        }
        let u = self.user_record(name)?;
        if !self.password_ok(name, current) {
            self.record_failure(name);
            self.event(
                name,
                "password_change_failed",
                name,
                "wrong current password",
            );
            return Err(MfaError::BadPassword);
        }
        // With two-factor on, the password alone does not change the password.
        if self.has_second_factor(name) {
            self.check_second_factor(name, code, recovery)?;
        }
        if new.chars().count() < MIN_PASSWORD_CHARS {
            return Err(MfaError::Invalid(format!(
                "the new password must be at least {MIN_PASSWORD_CHARS} characters"
            )));
        }
        if new == current {
            return Err(MfaError::Invalid(
                "the new password must differ from the current one".into(),
            ));
        }
        let hash = self.hash_password(new).map_err(MfaError::Unavailable)?;
        let mut patch = Fields::new();
        patch.insert("password_hash".into(), J::from(hash));
        patch.insert("must_change_password".into(), J::Bool(false));
        patch.insert("password_changed_at".into(), J::from(self.now()));
        self.put(name, &patch)?;
        let h = token_hash(session);
        let n = self.revoke_user_sessions(name, Some(&h));
        // The session that proved the old password may now do everything its person may, unless
        // two-factor enrolment is still owed.
        let groups = str_list(&u["groups"]);
        let still_limited = self.enrolment_required_for(&groups) && !self.has_second_factor(name);
        let mut lift = Fields::new();
        lift.insert("limited".into(), J::Bool(still_limited));
        let _ = self.backend.update("auth_token", &h, &lift);
        self.event(
            name,
            "password_change",
            name,
            &format!("password changed; {n} other session(s) signed out"),
        );
        Ok(())
    }

    // ---- administrators ---------------------------------------------------------------------

    pub(crate) fn users_impl(&self, admin: &Principal) -> Result<J, MfaError> {
        admin_name(admin)?;
        let now = self.now();
        let mut sessions: std::collections::HashMap<String, usize> = Default::default();
        for (_, t) in self.backend.list("auth_token").unwrap_or_default() {
            if t.get("kind").and_then(J::as_str) == Some("user")
                && t.get("revoked").and_then(J::as_bool) != Some(true)
                && t.get("expires_at")
                    .and_then(J::as_u64)
                    .is_none_or(|e| e > now)
                && let Some(o) = t.get("owner").and_then(J::as_str)
            {
                *sessions.entry(o.to_string()).or_default() += 1;
            }
        }
        let mut passkeys: std::collections::HashMap<String, usize> = Default::default();
        for (_, k) in self
            .backend
            .list(crate::passkey::PASSKEY_TABLE)
            .unwrap_or_default()
        {
            if let Some(o) = k.get("user").and_then(J::as_str) {
                *passkeys.entry(o.to_string()).or_default() += 1;
            }
        }
        let mut users: Vec<J> = self
            .backend
            .list("auth_user")
            .unwrap_or_default()
            .into_iter()
            .map(|(name, u)| {
                json!({
                    "name": name,
                    "groups": str_list(&u["groups"]),
                    "admin": str_list(&u["groups"]).iter().any(|g| g == ADMIN_GROUP),
                    "disabled": u.get("disabled").and_then(J::as_bool) == Some(true),
                    "offboarded": u.get("offboarded").and_then(J::as_bool) == Some(true),
                    "mfa_enabled": u.get("mfa_enabled").and_then(J::as_bool) == Some(true)
                        || passkeys.get(&name).is_some_and(|n| *n > 0),
                    "passkeys": passkeys.get(&name).copied().unwrap_or(0),
                    "must_change_password": u.get("must_change_password").and_then(J::as_bool) == Some(true),
                    "sessions": sessions.get(&name).copied().unwrap_or(0),
                })
            })
            .collect();
        users.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        Ok(json!({"users": users}))
    }

    pub(crate) fn enabled_admins(&self) -> usize {
        self.backend
            .list("auth_user")
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, u)| {
                str_list(&u["groups"]).iter().any(|g| g == ADMIN_GROUP)
                    && u.get("disabled").and_then(J::as_bool) != Some(true)
            })
            .count()
    }

    pub(crate) fn set_user_disabled_impl(
        &self,
        admin: &Principal,
        target: &str,
        disabled: bool,
    ) -> Result<(), MfaError> {
        let actor = admin_name(admin)?;
        // An approved proposal of an agent runs as `alice/agent`, which may not disable `alice`.
        let user = actor.strip_suffix("/agent").unwrap_or(actor);
        self.set_disabled_as(actor, Some(user), target, disabled)
    }

    /// `actor_user`: the account the actor signs in as, which may not disable itself; `None` for
    /// the operator's CLI.
    pub(crate) fn set_disabled_as(
        &self,
        actor: &str,
        actor_user: Option<&str>,
        target: &str,
        disabled: bool,
    ) -> Result<(), MfaError> {
        let target = target.trim().to_lowercase();
        if actor_user == Some(target.as_str()) {
            return Err(MfaError::Invalid(
                "you cannot disable your own account".into(),
            ));
        }
        let _g = self.mfa_guard()?;
        let u = self
            .backend
            .get("auth_user", &target)
            .ok()
            .flatten()
            .map(J::Object)
            .ok_or_else(|| MfaError::Invalid(format!("no user {target}")))?;
        let is_admin = str_list(&u["groups"]).iter().any(|g| g == ADMIN_GROUP);
        let was = u["disabled"].as_bool() == Some(true);
        if disabled && !was && is_admin && self.enabled_admins() <= 1 {
            return Err(MfaError::Invalid(
                "the last enabled administrator cannot be disabled".into(),
            ));
        }
        let mut patch = Fields::new();
        patch.insert("disabled".into(), J::Bool(disabled));
        let was_offboarded = u["offboarded"].as_bool() == Some(true);
        if !disabled && was_offboarded {
            patch.insert("offboarded".into(), J::Bool(false));
        }
        self.put(&target, &patch)?;
        if disabled {
            let n = self.revoke_user_sessions(&target, None);
            self.event(
                &target,
                "user_disable",
                actor,
                &format!("account disabled; {n} session(s) signed out; its tokens stop working"),
            );
        } else if was_offboarded {
            self.event(
                &target,
                "user_enable",
                actor,
                "account enabled after offboarding; the sessions, agent tokens and connected apps revoked then stay revoked",
            );
        } else {
            self.event(&target, "user_enable", actor, "account enabled");
        }
        Ok(())
    }

    pub(crate) fn reset_user_password_impl(
        &self,
        admin: &Principal,
        target: &str,
        temporary: &str,
    ) -> Result<(), MfaError> {
        let actor = admin_name(admin)?;
        let target = target.trim().to_lowercase();
        if target == actor {
            return Err(MfaError::Invalid(
                "change your own password under Security with your current password".into(),
            ));
        }
        if temporary.chars().count() < MIN_PASSWORD_CHARS {
            return Err(MfaError::Invalid(format!(
                "the temporary password must be at least {MIN_PASSWORD_CHARS} characters"
            )));
        }
        let _g = self.mfa_guard()?;
        self.user_record(&target)
            .map_err(|_| MfaError::Invalid(format!("no user {target}")))?;
        let hash = self
            .hash_password(temporary)
            .map_err(MfaError::Unavailable)?;
        let mut patch = Fields::new();
        patch.insert("password_hash".into(), J::from(hash));
        patch.insert("must_change_password".into(), J::Bool(true));
        self.put(&target, &patch)?;
        let n = self.revoke_user_sessions(&target, None);
        // A person locked out by wrong guesses may need this to get back in at all.
        let _ = self.backend.counter_reset(&Self::failure_key(&target));
        self.event(
            &target,
            "password_reset",
            actor,
            &format!("temporary password set; {n} session(s) signed out; a new password is required at the next sign-in"),
        );
        Ok(())
    }
}

/// A random temporary password for tools that need to suggest one (24 hex digits, 96 bits).
pub fn suggest_temporary_password() -> String {
    hex(&random_bytes::<12>())
}

#[cfg(test)]
mod tests;
