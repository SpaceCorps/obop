//! Sessions, password and account management of the signed-in human, and the administrator's
//! view of the accounts (B7, D59). Every change that matters is written to the security event
//! log (`security_event`) with who did it; nothing logged or returned is a secret.

use crate::{Auth, Change, hex, random_bytes, str_list, token_hash};
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
    ///
    /// Every caller has just changed something (a password, the groups, the account itself) that
    /// the other sessions must not outlive, so a refused list or revoke is never "0 ended": it is
    /// tried again, and if it is still refused the request fails as a partial one
    /// ([`Auth::needed`]).
    pub(crate) fn revoke_user_sessions(
        &self,
        name: &str,
        keep: Option<&str>,
        ch: &Change,
    ) -> Result<usize, MfaError> {
        self.revoke_sessions_until(name, keep, None, ch)
    }

    /// As [`Auth::revoke_user_sessions`], limited to the sessions that began at `until` or
    /// before: finishing a revocation later must not sign out the sessions made since.
    fn revoke_sessions_until(
        &self,
        name: &str,
        keep: Option<&str>,
        until: Option<u64>,
        ch: &Change,
    ) -> Result<usize, MfaError> {
        let mut n = 0;
        self.needed(ch, "ending the sessions", || {
            for (id, t) in self.backend.list("auth_token")? {
                let t = J::Object(t);
                if t["owner"].as_str() == Some(name)
                    && t["kind"].as_str() == Some("user")
                    && t["revoked"].as_bool() != Some(true)
                    && keep != Some(id.as_str())
                    && until.is_none_or(|u| t["created_at"].as_u64().is_none_or(|c| c <= u))
                {
                    let mut p = Fields::new();
                    p.insert("revoked".into(), J::Bool(true));
                    if self.backend.update("auth_token", &id, &p)? {
                        n += 1;
                    }
                }
            }
            Ok(())
        })?;
        Ok(n)
    }

    /// Write down, before the change is made, that its sessions are owed a revocation (`op`
    /// names the change; `keep` is the session that made it and stays). If the change commits
    /// and ending the sessions is then refused for long enough, the repeat of the same request
    /// finds this and finishes the work (D74); without it the repeat would meet "two-factor is
    /// not on" or "no such passkey" and the sessions of a stolen password would live on.
    pub(crate) fn owe_revocation(
        &self,
        name: &str,
        op: &str,
        keep: Option<&str>,
    ) -> Result<(), MfaError> {
        let mut p = Fields::new();
        p.insert(
            "sessions_owed".into(),
            json!({"op": op, "keep": keep, "at": self.now()}),
        );
        self.put(name, &p)
    }

    /// The revocation was done: forget it is owed. Best effort: a note left behind only makes the
    /// next repeat of the same change end the sessions that began before it, once more.
    pub(crate) fn clear_owed(&self, name: &str) {
        let mut p = Fields::new();
        p.insert("sessions_owed".into(), J::Null);
        let _ = obop_storage::fault::patiently(3, std::time::Duration::from_millis(100), || {
            obop_storage::fault::bookkeeping(|| self.backend.update("auth_user", name, &p))
        });
    }

    /// A repeat of the change `op` that already happened: if it left a revocation owed, do it
    /// now and say how many sessions ended (`Some`); else `None`, and the caller answers as it did.
    pub(crate) fn finish_owed(
        &self,
        name: &str,
        op: &str,
        ch: &Change,
    ) -> Result<Option<usize>, MfaError> {
        let u = self.user_record(name)?;
        let owed = &u["sessions_owed"];
        if owed["op"].as_str() != Some(op) {
            return Ok(None);
        }
        let keep = owed["keep"].as_str().map(String::from);
        let until = owed["at"].as_u64();
        let n = self.revoke_sessions_until(name, keep.as_deref(), until, ch)?;
        self.clear_owed(name);
        Ok(Some(n))
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
        let ch = Change::new(name, "sessions_revoked", name, "sessions signed out");
        let n = self.revoke_user_sessions(name, keep_current.then_some(h.as_str()), &ch)?;
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
        let ch = Change::new(name, "password_change", name, "password changed");
        let n = self.revoke_user_sessions(name, Some(&h), &ch)?;
        // The session that proved the old password may now do everything its person may, unless
        // two-factor enrolment is still owed.
        let groups = str_list(&u["groups"]);
        let still_limited = self.enrolment_required_for(&groups) && !self.has_second_factor(name);
        let mut lift = Fields::new();
        lift.insert("limited".into(), J::Bool(still_limited));
        self.needed(&ch, "lifting the limit of this session", || {
            self.backend.update("auth_token", &h, &lift)
        })?;
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

    /// How many enabled administrators there are. A refused list is the store's trouble, not
    /// "none": the callers refuse to take the last administrator away, and must not do that (a 400)
    /// or skip it (a lockout) because the lookup failed.
    pub(crate) fn enabled_admins(&self) -> Result<usize, MfaError> {
        Ok(self
            .backend
            .list("auth_user")
            .map_err(MfaError::Unavailable)?
            .into_iter()
            .filter(|(_, u)| {
                str_list(&u["groups"]).iter().any(|g| g == ADMIN_GROUP)
                    && u.get("disabled").and_then(J::as_bool) != Some(true)
            })
            .count())
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
        let u = self.known_user(&target)?;
        let is_admin = str_list(&u["groups"]).iter().any(|g| g == ADMIN_GROUP);
        let was = u["disabled"].as_bool() == Some(true);
        if disabled && !was && is_admin && self.enabled_admins()? <= 1 {
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
            let ch = Change::new(&target, "user_disable", actor, "account disabled");
            let n = self.revoke_user_sessions(&target, None, &ch)?;
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
        self.known_user(&target)?;
        let hash = self
            .hash_password(temporary)
            .map_err(MfaError::Unavailable)?;
        let mut patch = Fields::new();
        patch.insert("password_hash".into(), J::from(hash));
        patch.insert("must_change_password".into(), J::Bool(true));
        self.put(&target, &patch)?;
        let ch = Change::new(&target, "password_reset", actor, "temporary password set");
        let n = self.revoke_user_sessions(&target, None, &ch)?;
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
