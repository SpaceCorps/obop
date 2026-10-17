//! Two-factor authentication (D53): TOTP enrolment, the second sign-in step, recovery codes,
//! the admin policy and the security event log. Secrets are sealed with `OBOP_SECRET_KEY`
//! (see `seal`); recovery codes are stored as SHA-256 hashes of 80-bit random codes; the event
//! log (`security_event`) never holds a code or a secret.

use crate::{Auth, hex, now_system, random_bytes, str_list, token_hash};
use crate::{seal, totp};
use obop_commands::{ADMIN_GROUP, LoginOutcome, MfaApi, MfaError, Principal, PrincipalKind};
use obop_keys::KeyRing;
use obop_storage::Fields;
use serde_json::{Value as J, json};
use sha2::{Digest, Sha256};
use std::time::Duration;

/// How long the password step stays valid for the code step.
pub const MFA_TOKEN_SECS: u64 = 5 * 60;
/// Wrong codes one `mfa_token` survives.
pub const MFA_TOKEN_ATTEMPTS: u64 = 5;
pub const RECOVERY_CODES: usize = 10;
/// A failed code is written to the event log at most this often per user.
const FAILED_EVENT_EVERY: Duration = Duration::from_secs(10);

/// Table of the append-only event log; the `security_event` model of the `platform` module
/// reads it.
pub const EVENT_TABLE: &str = obop_commands::audit::EVENT_TABLE;
const MFA_TOKEN_TABLE: &str = "auth_mfa_token";

#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// The server key ring (`OBOP_SECRET_KEY`, then `OBOP_SECRET_KEY_PREVIOUS`) that seals TOTP
    /// secrets. Empty: two-factor cannot be enabled and the admin policy is not applied (nobody
    /// could enrol).
    pub keys: KeyRing,
    /// The default of `security.require_mfa_for_admins` until an admin stores a value.
    pub default_require_admins: bool,
    /// Shown in the authenticator app next to the account.
    pub issuer: String,
}

impl AuthConfig {
    /// `OBOP_SECRET_KEY` (64 hex digits recommended), `OBOP_SECRET_KEY_PREVIOUS`, `OBOP_DEV_INSECURE_KEY=1` for an
    /// explicit, logged development fallback, `OBOP_REQUIRE_MFA_FOR_ADMINS=0` to change the
    /// default of the admin policy, `OBOP_MFA_ISSUER`.
    pub fn from_env() -> Self {
        let flag = |n: &str| std::env::var(n).ok();
        let keys = match KeyRing::from_env() {
            Ok(k) => {
                if k.current()
                    .is_some_and(|c| c.source() == obop_keys::KeySource::Development)
                {
                    eprintln!(
                        "WARNING: OBOP_DEV_INSECURE_KEY is set: two-factor secrets are sealed with a public key. Development only."
                    );
                }
                k
            }
            Err(e) => {
                eprintln!("two-factor authentication is off: {e}");
                KeyRing::default()
            }
        };
        Self {
            keys,
            default_require_admins: !flag("OBOP_REQUIRE_MFA_FOR_ADMINS").is_some_and(|v| {
                matches!(
                    v.trim().to_lowercase().as_str(),
                    "0" | "false" | "no" | "off"
                )
            }),
            issuer: flag("OBOP_MFA_ISSUER")
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "obop".into()),
        }
    }

    /// The public development key and no admin policy, for tests.
    pub fn insecure_dev() -> Self {
        Self {
            keys: KeyRing::from_vars(None, None, true).unwrap_or_default(),
            default_require_admins: false,
            issuer: "obop".into(),
        }
    }
}

/// 16 base32 characters (80 bits) in four groups.
fn new_recovery_code() -> String {
    let s = totp::base32_encode(&random_bytes::<10>());
    format!("{}-{}-{}-{}", &s[..4], &s[4..8], &s[8..12], &s[12..16])
}

fn normalize_recovery(code: &str) -> String {
    code.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_uppercase()
}

fn recovery_hash(user: &str, code: &str) -> String {
    hex(&Sha256::digest(
        format!("obop-recovery:{user}:{}", normalize_recovery(code)).as_bytes(),
    ))
}

fn person(p: &Principal) -> Result<&str, MfaError> {
    // Agent tokens are not people: they cannot enrol, disable or reset anything.
    if p.kind == PrincipalKind::User {
        Ok(&p.id)
    } else {
        Err(MfaError::NotFound)
    }
}

impl Auth {
    // ---- policy -------------------------------------------------------------------------

    fn security_setting(&self, key: &str) -> Option<bool> {
        let f = self.backend.get("obop_setting", "instance").ok()??;
        let v: J = serde_json::from_str(f.get("value")?.as_str()?).ok()?;
        v["security"][key].as_bool()
    }

    /// `security.require_mfa_for_admins`: the stored value, else the configured default.
    pub fn require_mfa_for_admins(&self) -> bool {
        self.security_setting("require_mfa_for_admins")
            .unwrap_or(self.config.default_require_admins)
    }

    /// `security.require_mfa_for_all` (off by default).
    pub fn require_mfa_for_all(&self) -> bool {
        self.security_setting("require_mfa_for_all")
            .unwrap_or(false)
    }

    pub(crate) fn enrolment_required_for(&self, groups: &[String]) -> bool {
        // Without a server key nobody could enrol, so the policy cannot be applied.
        !self.config.keys.is_empty()
            && (self.require_mfa_for_all()
                || (self.require_mfa_for_admins() && groups.iter().any(|g| g == ADMIN_GROUP)))
    }

    pub fn mfa_enabled(&self, username: &str) -> bool {
        self.backend
            .get("auth_user", &username.trim().to_lowercase())
            .ok()
            .flatten()
            .is_some_and(|u| u.get("mfa_enabled").and_then(J::as_bool) == Some(true))
    }

    // ---- event log ----------------------------------------------------------------------

    /// Append to the security event log. Never pass a code, a secret or a password.
    pub(crate) fn event(&self, user: &str, kind: &str, actor: &str, detail: &str) {
        obop_commands::audit::record_event(&*self.backend, self.now(), user, kind, actor, detail);
    }

    fn failed_code_event(&self, user: &str) {
        // At most one event per user and window, whichever process sees the failure first.
        let first = self
            .backend
            .counter_add(
                &format!("auth.evt:code_failed:{user}"),
                FAILED_EVENT_EVERY.as_millis() as u64,
                false,
            )
            .map_or(true, |n| n == 1);
        if first {
            self.event(user, "code_failed", user, "wrong second-factor code");
        }
    }

    // ---- secrets ------------------------------------------------------------------------

    fn ring(&self) -> Result<&KeyRing, MfaError> {
        if self.config.keys.is_empty() {
            return Err(MfaError::Unavailable(
                "two-factor authentication needs the server secret OBOP_SECRET_KEY (32 bytes, hex or base64); it is not set".into(),
            ));
        }
        Ok(&self.config.keys)
    }

    fn open_secret(&self, user: &str, sealed: &str) -> Result<Vec<u8>, MfaError> {
        seal::open_any(self.ring()?, user, sealed).map_err(MfaError::Unavailable)
    }

    /// The stored user as a JSON object (a missing key reads as null, unlike `Fields`).
    pub(crate) fn user_record(&self, name: &str) -> Result<J, MfaError> {
        self.backend
            .get("auth_user", name)
            .ok()
            .flatten()
            .map(J::Object)
            .ok_or(MfaError::NotFound)
    }

    pub(crate) fn put(&self, name: &str, patch: &Fields) -> Result<(), MfaError> {
        match self.backend.update("auth_user", name, patch) {
            Ok(true) => Ok(()),
            Ok(false) => Err(MfaError::NotFound),
            Err(e) => Err(MfaError::Unavailable(e)),
        }
    }

    pub(crate) fn password_ok(&self, name: &str, password: &str) -> bool {
        use argon2::password_hash::{PasswordVerifier, phc::PasswordHash};
        let stored = self
            .backend
            .get("auth_user", name)
            .ok()
            .flatten()
            .map(J::Object);
        let dummy = self
            .hash_password("dummy-password-for-timing")
            .unwrap_or_default();
        let hash = stored
            .as_ref()
            .and_then(|u| u["password_hash"].as_str().map(String::from))
            .unwrap_or(dummy);
        let ok = hash
            .parse::<PasswordHash>()
            .is_ok_and(|h| self.hasher.verify_password(password.as_bytes(), &h).is_ok());
        ok && stored.is_some()
    }

    /// Check a TOTP code or a recovery code for `name`. Consumes what it accepts (the step, or
    /// the recovery code). Counts a miss toward the lockout. Call with `mfa_lock` held.
    pub(crate) fn check_second_factor(
        &self,
        name: &str,
        code: Option<&str>,
        recovery: Option<&str>,
    ) -> Result<(), MfaError> {
        if self.locked_out(name) {
            return Err(MfaError::Locked(
                "too many attempts, try again in a minute".into(),
            ));
        }
        let u = self.user_record(name)?;
        let ok = if let Some(code) = code.filter(|c| !c.trim().is_empty()) {
            let sealed = u["totp_secret"].as_str().ok_or(MfaError::NotFound)?;
            let secret = self.open_secret(name, sealed)?;
            match totp::verify(&secret, code, self.now(), u["totp_last_step"].as_u64()) {
                Some(step) => {
                    let mut p = Fields::new();
                    p.insert("totp_last_step".into(), J::from(step));
                    self.put(name, &p)?;
                    true
                }
                None => false,
            }
        } else if let Some(rc) = recovery.filter(|c| !c.trim().is_empty()) {
            let want = recovery_hash(name, rc);
            let mut left = str_list(&u["recovery"]);
            let before = left.len();
            // Compare all hashes in constant time; no early exit.
            let mut hit = false;
            let mut keep = Vec::new();
            for h in left.drain(..) {
                if totp::ct_eq(h.as_bytes(), want.as_bytes()) && !hit {
                    hit = true;
                } else {
                    keep.push(h);
                }
            }
            debug_assert!(keep.len() + usize::from(hit) == before);
            if hit {
                let mut p = Fields::new();
                p.insert("recovery".into(), json!(keep));
                self.put(name, &p)?;
                self.event(
                    name,
                    "recovery_used",
                    name,
                    &format!("{} recovery codes left", keep.len()),
                );
            }
            hit
        } else {
            return Err(MfaError::Invalid("enter a code or a recovery code".into()));
        };
        if ok {
            Ok(())
        } else {
            self.record_failure(name);
            self.failed_code_event(name);
            if self.locked_out(name) {
                self.event(name, "lockout", name, "too many failed attempts");
            }
            Err(MfaError::Denied("that code is not valid".into()))
        }
    }

    fn fresh_recovery(&self, name: &str) -> Result<(Vec<String>, Fields), MfaError> {
        let codes: Vec<String> = (0..RECOVERY_CODES).map(|_| new_recovery_code()).collect();
        let mut p = Fields::new();
        p.insert(
            "recovery".into(),
            json!(
                codes
                    .iter()
                    .map(|c| recovery_hash(name, c))
                    .collect::<Vec<_>>()
            ),
        );
        Ok((codes, p))
    }

    // ---- sign-in ------------------------------------------------------------------------

    /// Step one of sign-in. A user with a second factor gets an `mfa_token`, never a session.
    pub(crate) fn login_step_impl(
        &self,
        username: &str,
        password: &str,
    ) -> Result<LoginOutcome, String> {
        let name = username.trim().to_lowercase();
        if self.locked_out(&name) {
            return Err("too many attempts, try again in a minute".into());
        }
        if !self.password_ok(&name, password) {
            self.record_failure(&name);
            return Err("invalid username or password".into());
        }
        // Same answer as a wrong password, so nobody can tell a disabled account from none; the
        // security log says what happened.
        if self.is_disabled(&name) {
            self.event(&name, "login_refused", &name, "the account is disabled");
            return Err("invalid username or password".into());
        }
        self.purge_tokens();
        if self.mfa_enabled(&name) {
            let token = format!("obopmfa_{}", hex(&random_bytes::<32>()));
            let mut f = Fields::new();
            f.insert("user".into(), J::from(name.as_str()));
            f.insert("expires_at".into(), J::from(self.now() + MFA_TOKEN_SECS));
            f.insert("attempts".into(), J::from(0u64));
            self.backend
                .insert(MFA_TOKEN_TABLE, &token_hash(&token), &f)?;
            return Ok(LoginOutcome::MfaRequired { mfa_token: token });
        }
        let groups = self
            .backend
            .get("auth_user", &name)
            .ok()
            .flatten()
            .map(|u| str_list(u.get("groups").unwrap_or(&J::Null)))
            .unwrap_or_default();
        let limited = self.enrolment_required_for(&groups);
        if limited {
            self.event(
                &name,
                "limited_session",
                &name,
                "signed in without two-factor; only enrolment is allowed",
            );
        }
        self.session_outcome(&name, limited)
    }

    pub(crate) fn login_mfa_impl(
        &self,
        mfa_token: &str,
        code: Option<&str>,
        recovery: Option<&str>,
    ) -> Result<LoginOutcome, MfaError> {
        let _g = self.mfa_guard()?;
        let key = token_hash(mfa_token);
        let expired = || MfaError::Expired;
        let rec = self
            .backend
            .get(MFA_TOKEN_TABLE, &key)
            .ok()
            .flatten()
            .map(J::Object)
            .ok_or_else(expired)?;
        let name = rec["user"].as_str().ok_or_else(expired)?.to_string();
        if rec["expires_at"].as_u64().is_none_or(|e| e <= self.now()) {
            let _ = self.backend.delete(MFA_TOKEN_TABLE, &key);
            return Err(expired());
        }
        let attempts = rec["attempts"].as_u64().unwrap_or(0);
        if attempts >= MFA_TOKEN_ATTEMPTS {
            let _ = self.backend.delete(MFA_TOKEN_TABLE, &key);
            return Err(MfaError::Locked(
                "too many wrong codes for this sign-in; enter your password again".into(),
            ));
        }
        match self.check_second_factor(&name, code, recovery) {
            Ok(()) => {
                // Single use: the token is gone whatever happens next.
                let _ = self.backend.delete(MFA_TOKEN_TABLE, &key);
                self.session_outcome(&name, false)
                    .map_err(MfaError::Unavailable)
            }
            Err(e @ (MfaError::Denied(_) | MfaError::Locked(_))) => {
                let mut p = Fields::new();
                p.insert("attempts".into(), J::from(attempts + 1));
                let _ = self.backend.update(MFA_TOKEN_TABLE, &key, &p);
                Err(e)
            }
            Err(e) => Err(e),
        }
    }

    /// A session for `name`. It is limited to the account routes when two-factor enrolment is
    /// owed (`enrol`) or an administrator reset the password and a new one is owed.
    fn session_outcome(&self, name: &str, enrol: bool) -> Result<LoginOutcome, String> {
        let must_change = self.must_change_password(name);
        let token = self.issue_session(name, enrol || must_change)?;
        if must_change {
            self.event(
                name,
                "limited_session",
                name,
                "signed in with a temporary password; only a password change is allowed",
            );
        }
        Ok(LoginOutcome::Session {
            token,
            mfa_enrolment_required: enrol,
            password_change_required: must_change,
        })
    }

    /// Turn two-factor off. Every other session of the person ends (all of them when the caller
    /// is not a session): a stolen session must not outlive the second factor.
    fn disable_impl(
        &self,
        p: &Principal,
        session: Option<&str>,
        password: &str,
        code: Option<&str>,
        recovery: Option<&str>,
    ) -> Result<(), MfaError> {
        let name = person(p)?;
        let _g = self.mfa_guard()?;
        let u = self.user_record(name)?;
        if u["mfa_enabled"].as_bool() != Some(true) {
            return Err(MfaError::Invalid("two-factor is not on".into()));
        }
        if !self.password_ok(name, password) {
            self.record_failure(name);
            return Err(MfaError::BadPassword);
        }
        self.check_second_factor(name, code, recovery)?;
        self.clear_factor(name)?;
        let keep = session.map(token_hash);
        let n = self.revoke_user_sessions(name, keep.as_deref());
        self.event(
            name,
            "disable",
            name,
            &format!("turned off by the user; {n} other session(s) signed out"),
        );
        Ok(())
    }

    // ---- operator -----------------------------------------------------------------------

    /// Revoke every session of a user (not agent tokens: those are separate credentials).
    fn revoke_sessions(&self, name: &str) {
        self.revoke_user_sessions(name, None);
    }

    fn clear_factor(&self, name: &str) -> Result<(), MfaError> {
        let mut p = Fields::new();
        for k in ["totp_secret", "totp_pending", "totp_last_step", "recovery"] {
            p.insert(k.into(), J::Null);
        }
        p.insert("mfa_enabled".into(), J::Bool(false));
        self.put(name, &p)
    }

    /// Break-glass: remove a user's second factor with direct database access (`obop admin
    /// reset-mfa USER --db URL`). Revokes the user's sessions and writes an audit record.
    pub fn operator_reset_mfa(&self, username: &str) -> Result<(), String> {
        let name = username.trim().to_lowercase();
        let _g = self.backend.lock("auth.mfa")?;
        self.user_record(&name)
            .map_err(|_| format!("no user {name}"))?;
        self.clear_factor(&name).map_err(|e| format!("{e:?}"))?;
        self.revoke_sessions(&name);
        self.event(&name, "reset", "operator", "reset from the command line");
        Ok(())
    }

    /// For tests: pretend it is `secs` seconds after the epoch.
    pub fn set_clock_for_test(&self, secs: u64) {
        self.fake_now
            .store(secs, std::sync::atomic::Ordering::Relaxed);
    }

    pub(crate) fn now(&self) -> u64 {
        match self.fake_now.load(std::sync::atomic::Ordering::Relaxed) {
            0 => now_system(),
            t => t,
        }
    }
}

impl MfaApi for Auth {
    fn status(&self, p: &Principal) -> Result<J, MfaError> {
        let name = person(p)?;
        let u = self.user_record(name)?;
        let key = !self.config.keys.is_empty();
        Ok(json!({
            "user": name,
            "available": key,
            "enabled": u["mfa_enabled"].as_bool() == Some(true),
            "pending": u["totp_pending"].is_string(),
            "recovery_remaining": str_list(&u["recovery"]).len(),
            "limited": p.mfa_limited,
            "password_change_required": u["must_change_password"].as_bool() == Some(true),
            "is_admin": p.groups.contains(ADMIN_GROUP),
            "require_admins": self.require_mfa_for_admins(),
            "require_all": self.require_mfa_for_all(),
        }))
    }

    fn begin(&self, p: &Principal) -> Result<J, MfaError> {
        let name = person(p)?;
        let ring = self.ring()?;
        let _g = self.mfa_guard()?;
        let u = self.user_record(name)?;
        if u["mfa_enabled"].as_bool() == Some(true) {
            return Err(MfaError::Invalid(
                "two-factor is already on; turn it off first to enrol a new device".into(),
            ));
        }
        let secret = random_bytes::<{ totp::SECRET_LEN }>();
        let b32 = totp::base32_encode(&secret);
        let mut f = Fields::new();
        f.insert(
            "totp_pending".into(),
            J::from(seal::seal_new(ring, name, &secret).map_err(MfaError::Unavailable)?),
        );
        self.put(name, &f)?;
        self.event(name, "enrol_begin", name, "secret issued, not active");
        Ok(json!({
            "secret": b32,
            "uri": totp::otpauth_uri(&self.config.issuer, name, &b32),
            "issuer": self.config.issuer,
            "account": name,
            "digits": totp::DIGITS,
            "period": totp::PERIOD,
        }))
    }

    fn confirm(&self, p: &Principal, session: &str, code: &str) -> Result<J, MfaError> {
        let name = person(p)?;
        let _g = self.mfa_guard()?;
        if self.locked_out(name) {
            return Err(MfaError::Locked(
                "too many attempts, try again in a minute".into(),
            ));
        }
        let u = self.user_record(name)?;
        let pending = u["totp_pending"]
            .as_str()
            .ok_or_else(|| MfaError::Invalid("start enrolment first".into()))?;
        let secret = self.open_secret(name, pending)?;
        let Some(step) = totp::verify(&secret, code, self.now(), None) else {
            self.record_failure(name);
            self.failed_code_event(name);
            if self.locked_out(name) {
                self.event(name, "lockout", name, "too many failed attempts");
            }
            return Err(MfaError::Denied(
                "that code is not valid; check the time on your device".into(),
            ));
        };
        let (codes, mut f) = self.fresh_recovery(name)?;
        f.insert("totp_secret".into(), J::from(pending));
        f.insert("totp_pending".into(), J::Null);
        f.insert("totp_last_step".into(), J::from(step));
        f.insert("mfa_enabled".into(), J::Bool(true));
        self.put(name, &f)?;
        // The session that just proved the factor is no longer limited.
        let mut lift = Fields::new();
        lift.insert("limited".into(), J::Bool(false));
        let _ = self
            .backend
            .update("auth_token", &token_hash(session), &lift);
        self.event(name, "enrol_confirm", name, "two-factor is on");
        Ok(json!({"recovery_codes": codes}))
    }

    fn disable(
        &self,
        p: &Principal,
        password: &str,
        code: Option<&str>,
        recovery: Option<&str>,
    ) -> Result<(), MfaError> {
        self.disable_impl(p, None, password, code, recovery)
    }

    fn disable_in_session(
        &self,
        p: &Principal,
        session: &str,
        password: &str,
        code: Option<&str>,
        recovery: Option<&str>,
    ) -> Result<(), MfaError> {
        self.disable_impl(p, Some(session), password, code, recovery)
    }

    fn regenerate_recovery(
        &self,
        p: &Principal,
        password: &str,
        code: Option<&str>,
        recovery: Option<&str>,
    ) -> Result<J, MfaError> {
        let name = person(p)?;
        let _g = self.mfa_guard()?;
        let u = self.user_record(name)?;
        if u["mfa_enabled"].as_bool() != Some(true) {
            return Err(MfaError::Invalid("two-factor is not on".into()));
        }
        if !self.password_ok(name, password) {
            self.record_failure(name);
            return Err(MfaError::BadPassword);
        }
        self.check_second_factor(name, code, recovery)?;
        let (codes, f) = self.fresh_recovery(name)?;
        self.put(name, &f)?;
        self.event(name, "recovery_regenerate", name, "10 new recovery codes");
        Ok(json!({"recovery_codes": codes}))
    }

    fn admin_reset(&self, admin: &Principal, target: &str) -> Result<(), MfaError> {
        let actor = person(admin)?;
        if !admin.groups.contains(ADMIN_GROUP) || admin.mfa_limited {
            return Err(MfaError::NotFound);
        }
        let target = target.trim().to_lowercase();
        if target == actor {
            return Err(MfaError::Invalid(
                "you cannot reset your own two-factor here; turn it off in Security with your code"
                    .into(),
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
        if u["mfa_enabled"].as_bool() != Some(true) && !u["totp_pending"].is_string() {
            return Err(MfaError::Invalid(format!(
                "{target} has no two-factor to reset"
            )));
        }
        self.clear_factor(&target)?;
        self.revoke_sessions(&target);
        self.event(&target, "reset", actor, "reset by an administrator");
        Ok(())
    }
    fn sessions(&self, p: &Principal, session: &str) -> Result<J, MfaError> {
        self.sessions_impl(p, session)
    }

    fn revoke_sessions(
        &self,
        p: &Principal,
        session: &str,
        keep_current: bool,
    ) -> Result<usize, MfaError> {
        self.revoke_sessions_impl(p, session, keep_current)
    }

    fn change_password(
        &self,
        p: &Principal,
        session: &str,
        current: &str,
        new: &str,
        code: Option<&str>,
        recovery_code: Option<&str>,
    ) -> Result<(), MfaError> {
        self.change_password_impl(p, session, current, new, code, recovery_code)
    }

    fn users(&self, admin: &Principal) -> Result<J, MfaError> {
        self.users_impl(admin)
    }

    fn set_user_disabled(
        &self,
        admin: &Principal,
        target: &str,
        disabled: bool,
    ) -> Result<(), MfaError> {
        self.set_user_disabled_impl(admin, target, disabled)
    }

    fn reset_user_password(
        &self,
        admin: &Principal,
        target: &str,
        temporary: &str,
    ) -> Result<(), MfaError> {
        self.reset_user_password_impl(admin, target, temporary)
    }
}
