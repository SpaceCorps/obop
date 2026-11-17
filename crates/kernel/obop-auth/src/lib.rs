//! Identity: users with Argon2id password hashes, and bearer tokens stored only as SHA-256
//! hashes. Session tokens expire; agent tokens carry scopes that narrow their owner's groups
//! (D18). Records live in the storage backend under `auth_user` and `auth_token`, outside the
//! module registry, so no generated command can read or write them.

use argon2::password_hash::{PasswordHasher, phc::PasswordHash};
use argon2::{Argon2, Params};
use obop_commands::{Authenticator, LoginOutcome, MfaApi, MfaError, Principal, PrincipalKind};
use obop_storage::{Backend, Fields};
use serde_json::{Value as J, json};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

mod account;
mod admin;
mod mfa;
mod oauth;
pub mod oidc;
mod passkey;
pub mod seal;
pub mod totp;
pub mod webauthn;

pub use account::{MIN_PASSWORD_CHARS, suggest_temporary_password};
pub use admin::{ImportOutcome, ImportRow, clean_groups};
pub use mfa::{AuthConfig, EVENT_TABLE};
pub use passkey::{CHALLENGE_SECS, MAX_PASSKEYS, PASSKEY_TABLE, ceremony_site_ok};

const SESSION_SECS: u64 = 12 * 3600;
const MAX_FAILURES: u32 = 5;
const LOCKOUT: Duration = Duration::from_secs(60);

pub struct Auth {
    backend: Arc<dyn Backend>,
    hasher: Argon2<'static>,
    config: AuthConfig,
    fake_now: AtomicU64,
    /// The authority Microsoft sign-in trusts: `None` is Microsoft's own (D91); a loopback
    /// address stands in for it in tests (`OBOP_OIDC_TEST_AUTHORITY`, honoured only together with
    /// `OBOP_CONNECTORS_ALLOW_LOOPBACK`).
    oidc_authority: std::sync::RwLock<Option<String>>,
}

fn now_system() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).expect("operating system randomness");
    b
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn token_hash(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

/// The credential epoch of an account: a random value that every change which ends sessions
/// writes in the same atomic write as the change itself (a new password, a reset, a disable or an
/// offboarding, new groups, a removed second factor or passkey). A sign-in reads it before it
/// looks at any credential and compares it once more after its session is written: if it is
/// another value, a revocation ran in between whose list of sessions may not hold this one, so
/// the session is ended and not handed out. If it is the same, the revocation has not committed
/// yet, and the list it takes afterwards holds the session. No lock is needed between the two.
pub(crate) type Epoch = Option<String>;

pub(crate) fn epoch_of(u: &J) -> Epoch {
    u.get("epoch").and_then(J::as_str).map(String::from)
}

/// Put a new credential epoch into `patch` (see [`Epoch`]).
pub(crate) fn bump_epoch(patch: &mut Fields, now: u64) {
    patch.insert("epoch".into(), J::from(hex(&random_bytes::<12>())));
    // When the credentials changed, in the same write: what a revocation finished later ends is
    // cut off here and not at the note written before the change (L16).
    patch.insert("epoch_at".into(), J::from(now));
}

fn str_list(v: &J) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

impl Auth {
    /// Configuration from the environment (`AuthConfig::from_env`).
    pub fn new(backend: Arc<dyn Backend>) -> Self {
        Self::with_config(backend, AuthConfig::from_env())
    }

    pub fn with_config(backend: Arc<dyn Backend>, config: AuthConfig) -> Self {
        Self {
            backend,
            hasher: Argon2::default(),
            config,
            fake_now: AtomicU64::new(0),
            oidc_authority: std::sync::RwLock::new(oidc::authority_from_env()),
        }
    }

    /// Cheap hashing parameters and the public development key, for tests only. No admin policy.
    pub fn with_test_params(backend: Arc<dyn Backend>) -> Self {
        Self::with_test_config(backend, AuthConfig::insecure_dev())
    }

    /// Cheap hashing parameters and an explicit two-factor configuration, for tests only.
    pub fn with_test_config(backend: Arc<dyn Backend>, config: AuthConfig) -> Self {
        let params = Params::new(8, 1, 1, None).expect("valid params");
        Self {
            hasher: Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params),
            // A test chooses its authority (`set_oidc_authority_for_test`); the environment of the
            // process that runs the tests does not.
            oidc_authority: std::sync::RwLock::new(None),
            ..Self::with_config(backend, config)
        }
    }

    pub fn user_count(&self) -> usize {
        self.backend.count("auth_user").unwrap_or(0)
    }

    pub(crate) fn hash_password(&self, password: &str) -> Result<String, String> {
        self.hasher
            .hash_password(password.as_bytes())
            .map(|h: PasswordHash| h.to_string())
            .map_err(|e| e.to_string())
    }

    /// Create a user. Usernames are lowercased and unique.
    pub fn create_user(
        &self,
        username: &str,
        password: &str,
        groups: &[String],
    ) -> Result<(), String> {
        let name = username.trim().to_lowercase();
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-@".contains(c))
        {
            return Err("username must be letters, digits or . _ - @".into());
        }
        if password.chars().count() < 12 {
            return Err("password must be at least 12 characters".into());
        }
        let mut f = Fields::new();
        f.insert(
            "password_hash".into(),
            J::from(self.hash_password(password)?),
        );
        f.insert("groups".into(), json!(groups));
        self.backend.insert("auth_user", &name, &f).map_err(|e| {
            if e.contains("duplicate") || e.contains("UNIQUE") || e.contains("unique") {
                format!("user {name} exists")
            } else {
                e
            }
        })
    }

    /// Hash a password with this instance's parameters. For tests of provisioning code.
    pub fn hash_for_test(&self, password: &str) -> String {
        self.hash_password(password).expect("hashing")
    }

    /// Create a user from an existing Argon2 hash, so an instance can accept the credentials an
    /// account was created with without ever seeing the password.
    pub fn create_user_hashed(
        &self,
        username: &str,
        password_hash: &str,
        groups: &[String],
    ) -> Result<(), String> {
        let name = username.trim().to_lowercase();
        if !password_hash.starts_with("$argon2id$") {
            return Err("not an argon2id hash".into());
        }
        let mut f = Fields::new();
        f.insert("password_hash".into(), J::from(password_hash));
        f.insert("groups".into(), json!(groups));
        self.backend.insert("auth_user", &name, &f)
    }

    /// The stored hash of a user, for provisioning that user elsewhere.
    pub fn password_hash(&self, username: &str) -> Option<String> {
        let u = self
            .backend
            .get("auth_user", &username.trim().to_lowercase())
            .ok()??;
        u["password_hash"].as_str().map(String::from)
    }

    /// The principal of an existing user, for operator tooling that already has database access.
    pub fn principal(&self, name: &str) -> Option<Principal> {
        self.user_principal(&name.trim().to_lowercase())
    }

    fn user_principal(&self, name: &str) -> Option<Principal> {
        let u = self.backend.get("auth_user", name).ok()??;
        // A disabled account holds no principal: its sessions and agent tokens stop working at
        // once, and work again if an administrator enables it.
        if u.get("disabled").and_then(J::as_bool) == Some(true) {
            return None;
        }
        let groups: Vec<_> = str_list(&u["groups"]);
        Some(Principal::user(
            name,
            &groups.iter().map(String::as_str).collect::<Vec<_>>(),
        ))
    }

    fn issue_session(&self, owner: &str, limited: bool) -> Result<String, String> {
        self.issue_with(owner, "user", &[], "session", Some(SESSION_SECS), limited)
    }

    fn issue(
        &self,
        owner: &str,
        kind: &str,
        scopes: &[String],
        label: &str,
        ttl: Option<u64>,
    ) -> Result<String, String> {
        self.issue_with(owner, kind, scopes, label, ttl, false)
    }

    fn issue_with(
        &self,
        owner: &str,
        kind: &str,
        scopes: &[String],
        label: &str,
        ttl: Option<u64>,
        limited: bool,
    ) -> Result<String, String> {
        let token = format!("obop_{}", hex(&random_bytes::<32>()));
        let mut f = Fields::new();
        f.insert("owner".into(), J::from(owner));
        f.insert("kind".into(), J::from(kind));
        f.insert("scopes".into(), json!(scopes));
        f.insert("label".into(), J::from(label));
        f.insert(
            "expires_at".into(),
            ttl.map_or(J::Null, |t| J::from(self.now() + t)),
        );
        f.insert("revoked".into(), J::Bool(false));
        f.insert("limited".into(), J::Bool(limited));
        f.insert("created_at".into(), J::from(self.now()));
        self.backend.insert("auth_token", &token_hash(&token), &f)?;
        Ok(token)
    }

    pub fn revoke(&self, token: &str) -> bool {
        let mut p = Fields::new();
        p.insert("revoked".into(), J::Bool(true));
        self.backend
            .update("auth_token", &token_hash(token), &p)
            .unwrap_or(false)
    }

    /// Failed attempts per user live in the storage backend (an atomic expiring counter), so
    /// the lockout holds across server processes and restarts (B1).
    pub(crate) fn failure_key(name: &str) -> String {
        format!("auth.fail:{name}")
    }

    /// Too many wrong passwords lately. A counter the store would not give is not "none": the
    /// answer is "locked", so that the attempts cannot go on unseen while the count cannot be
    /// kept (D74).
    pub(crate) fn locked_out(&self, name: &str) -> bool {
        self.backend
            .counter_get(&Self::failure_key(name))
            .map_or(true, |n| n >= u64::from(MAX_FAILURES))
    }

    /// The count was read and has reached the limit. This, not [`Auth::locked_out`], is what a
    /// log entry that says "too many failed attempts" may be written from: a count the store would
    /// not give locks the sign-in (fail closed) but is no fact for the log.
    pub(crate) fn lockout_reached(&self, name: &str) -> bool {
        self.backend
            .counter_get(&Self::failure_key(name))
            .is_ok_and(|n| n >= u64::from(MAX_FAILURES))
    }

    /// Count a wrong guess. A guess that could not be counted is an error, tried again first: the
    /// answer is the store's trouble, not a plain "wrong password" that leaves the attempt
    /// uncounted for whoever is guessing while the pool is full.
    pub(crate) fn record_failure(&self, name: &str) -> Result<(), String> {
        obop_storage::fault::patiently(3, Duration::from_millis(100), || {
            self.backend
                .counter_add(&Self::failure_key(name), LOCKOUT.as_millis() as u64, true)
                .map(|_| ())
        })
    }

    /// For tests: lift every lockout of `name`.
    #[cfg(test)]
    pub(crate) fn clear_failures_for_test(&self, name: &str) {
        let _ = self.backend.counter_reset(&Self::failure_key(name));
    }

    /// Serialises read-modify-write of two-factor state across server processes.
    pub(crate) fn mfa_guard(&self) -> Result<obop_storage::LockGuard, MfaError> {
        self.backend.lock("auth.mfa").map_err(MfaError::Unavailable)
    }

    /// Evaluate `f`, whose store reads cannot report a refusal (they answer "no" or "none" instead:
    /// `has_second_factor`, `must_change_password`), until it ran without the store refusing a
    /// call, at most five times. A value computed from a refused read must not be written, so a
    /// lasting refusal is an error, with the fault noted as before.
    pub(crate) fn settled<T>(&self, mut f: impl FnMut() -> T) -> Result<T, MfaError> {
        use obop_storage::fault;
        let earlier = fault::take();
        let mut last = None;
        for attempt in 0..5 {
            if attempt > 0 {
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            let v = f();
            match fault::take() {
                None => {
                    if let Some(e) = earlier {
                        fault::note(e);
                    }
                    return Ok(v);
                }
                Some(fl) => last = Some(fl),
            }
        }
        let fl = last.unwrap_or(fault::Fault::Busy);
        fault::note(earlier.unwrap_or(fl));
        Err(MfaError::Unavailable(
            fl.message("a lookup this change depends on was refused"),
        ))
    }

    /// A step the request needs after one of its changes committed (ending the sessions of an
    /// account that was just disabled, relimiting the sessions of a changed group list). A refused
    /// step is tried again a few times, with its writes allowed although an earlier call of the
    /// request was refused (it rests on the change just made, not on a read). If it is still
    /// refused the request is a partial one: the entry of the change that did commit goes to the
    /// security log (`kind`, saying the step is missing), the request is marked partial and the
    /// error is returned, never "nothing to do". The step must be safe to run again: it reads what
    /// is left to do each time; `f` keeps its own tally.
    pub(crate) fn needed<T>(
        &self,
        ch: &Change,
        what: &str,
        mut f: impl FnMut() -> Result<T, String>,
    ) -> Result<T, MfaError> {
        obop_storage::fault::patiently(5, std::time::Duration::from_millis(150), || {
            obop_storage::fault::allow(&mut f)
        })
        .map_err(|e| self.incomplete(ch, what, e))
    }

    /// The request is a partial one: mark it, say so in the security log and return the error that
    /// ends it. The entry is the one of the change that did commit (its own kind, its own actor
    /// and what it did), with what is missing added, so a partial failure does not replace it.
    pub(crate) fn incomplete(&self, ch: &Change, what: &str, e: String) -> MfaError {
        obop_storage::fault::mark_partial();
        self.event(
            &ch.user,
            ch.kind,
            &ch.actor,
            &format!(
                "{}; INCOMPLETE: {what} was refused by the store ({e}); {}",
                ch.detail, ch.then
            ),
        );
        MfaError::Unavailable(e)
    }
}

/// A change of an account that has committed (or is about to), as the security log names it: who
/// did what to whom. The steps that follow it and may be refused carry it, so that the entry of a
/// partial failure is the entry of the change, not a stranger's (D74).
pub(crate) struct Change {
    pub user: String,
    pub kind: &'static str,
    pub actor: String,
    pub detail: String,
    /// What to do about it if a step is missing (the log says so).
    pub then: &'static str,
}

impl Change {
    pub(crate) fn new(user: &str, kind: &'static str, actor: &str, detail: &str) -> Self {
        Self {
            user: user.into(),
            kind,
            actor: actor.into(),
            detail: detail.into(),
            then: "repeat the request",
        }
    }

    /// For a change a repeat cannot finish (it needs what the change replaced): say what can.
    pub(crate) fn then(mut self, what: &'static str) -> Self {
        self.then = what;
        self
    }
}

impl Auth {
    /// The principal of a live token and the audience it is bound to (OAuth tokens only).
    fn resolve_full(&self, token: &str) -> Option<(Principal, Option<String>)> {
        let t = self.backend.get("auth_token", &token_hash(token)).ok()??;
        if t["revoked"].as_bool() == Some(true)
            || t["expires_at"].as_u64().is_some_and(|e| e <= self.now())
        {
            return None;
        }
        let owner = t["owner"].as_str()?;
        let mut p = self.user_principal(owner)?;
        match t["kind"].as_str() {
            // The access token of a mobile app (stored as an agent token, see `oauth.rs`): the
            // person themselves, narrowed to the groups they handed over (never more than they
            // hold today, and never one a phone may not have), for as long as the device it was
            // issued to is in force. The device is read at every use: one lookup by key, and a
            // lookup the store refuses is no principal (fail closed, D74).
            Some("agent") if t.get("mobile").and_then(J::as_bool) == Some(true) => {
                let device = t["device"].as_str()?;
                let rec = self
                    .backend
                    .get(obop_commands::devices::MODEL, device)
                    .ok()??;
                if !obop_commands::devices::is_active(&rec)
                    || !obop_commands::devices::is_ours(&rec)
                {
                    return None;
                }
                p.scopes = Some(
                    str_list(&t["scopes"])
                        .into_iter()
                        .filter(|g| !obop_commands::devices::never_for_mobile(g))
                        .collect(),
                );
                p.device = Some(device.to_string());
            }
            Some("agent") => {
                p.id = format!("{owner}/agent");
                p.kind = PrincipalKind::Agent;
                p.scopes = Some(str_list(&t["scopes"]).into_iter().collect());
            }
            _ => p.mfa_limited = t["limited"].as_bool() == Some(true),
        }
        Some((p, t.get("audience").and_then(J::as_str).map(String::from)))
    }
}

impl Auth {
    /// `obop agent-token USER --scopes ...`: the operator, whose authority is the access to the
    /// database, issues an agent token of `owner` by the same rule as the owner would. The log
    /// names the actor `operator`, as for the other operator commands (D69).
    pub fn issue_agent_token_by_operator(
        &self,
        owner: &Principal,
        scopes: &[String],
        label: &str,
        registered: &[String],
    ) -> Result<String, String> {
        self.issue_agent_token_as("operator", owner, scopes, label, registered)
    }

    /// The one rule for issuing an agent token (D69 amendment of 2026-11-14). `actor` is who the
    /// security log names: the owner, or `operator` for the command line.
    fn issue_agent_token_as(
        &self,
        actor: &str,
        owner: &Principal,
        scopes: &[String],
        label: &str,
        registered: &[String],
    ) -> Result<String, String> {
        if !owner.is_person() {
            return Err("only a human user can issue agent tokens".into());
        }
        if scopes.is_empty() {
            return Err("an agent token needs at least one scope".into());
        }
        let label = match label.trim() {
            "" => "agent",
            l => l,
        };
        if label.chars().count() > AGENT_LABEL_MAX || label.chars().any(char::is_control) {
            return Err(format!(
                "the label of an agent token is at most {AGENT_LABEL_MAX} characters on one line"
            ));
        }
        // An administrator holds every group (`Principal::allows`), so may hand on any group the
        // instance has; a session limited to enrolment holds nothing (D53).
        let admin = owner.groups.contains(obop_commands::ADMIN_GROUP) && !owner.mfa_limited;
        let mut through_admin = Vec::new();
        for s in scopes {
            if s == obop_commands::ANY_GROUP {
                return Err(
                    "`*` names no group and gives a token nothing: list the groups the agent may use"
                        .into(),
                );
            }
            if owner.groups.contains(s) {
                continue;
            }
            if !admin {
                return Err(format!("you do not hold {s}, so you cannot delegate it"));
            }
            if !registered.contains(s) {
                return Err(format!(
                    "{s} is not a registered permission group, so it cannot be delegated"
                ));
            }
            through_admin.push(s.as_str());
        }
        // An account bound to a Microsoft identity hands on tokens that end by themselves (N18,
        // ratified with D91 by OR-38): the person's access is decided in the tenant, and a token
        // that outlived a change there would not hear of it.
        let bound = self
            .link_of(&owner.id)
            .map_err(|e| format!("{e:?}"))?
            .is_some();
        let ttl = bound.then_some(oidc::MICROSOFT_AGENT_TOKEN_SECS);
        let token = self.issue(&owner.id, "agent", scopes, label, ttl)?;
        // The log says what the token may do and how the right to give it was held; never the token.
        let mut detail = format!("label {label:?}; scopes {}", scopes.join(", "));
        if bound {
            detail.push_str(&format!(
                "; ends in {} days (the account signs in with Microsoft)",
                oidc::MICROSOFT_AGENT_TOKEN_SECS / 86_400
            ));
        }
        if !through_admin.is_empty() {
            detail.push_str(&format!(
                "; held through {}: {}",
                obop_commands::ADMIN_GROUP,
                through_admin.join(", ")
            ));
        }
        self.event(&owner.id, "agent_token_issued", actor, &detail);
        Ok(token)
    }
}

impl Authenticator for Auth {
    fn resolve(&self, token: &str) -> Option<Principal> {
        self.resolve_full(token).map(|(p, _)| p)
    }

    fn resolve_with_audience(&self, token: &str) -> Option<(Principal, Option<String>)> {
        self.resolve_full(token)
    }

    fn oauth(&self) -> Option<&dyn obop_commands::OAuthTokens> {
        Some(self)
    }

    fn active_session_count(&self) -> Option<u64> {
        let now = self.now();
        let tokens = self.backend.list("auth_token").ok()?;
        Some(
            tokens
                .iter()
                .filter(|(_, t)| {
                    t.get("kind").and_then(J::as_str) == Some("user")
                        && t.get("label").and_then(J::as_str) == Some("session")
                        && t.get("revoked").and_then(J::as_bool) != Some(true)
                        && t.get("expires_at")
                            .and_then(J::as_u64)
                            .is_none_or(|e| e > now)
                })
                .count() as u64,
        )
    }

    fn login(&self, username: &str, password: &str) -> Result<String, String> {
        match self.login_step_impl(username, password)? {
            LoginOutcome::Session { token, .. } => Ok(token),
            LoginOutcome::MfaRequired { .. } => {
                Err("a two-factor code is required: use the two-step sign-in".into())
            }
        }
    }

    fn login_step(&self, username: &str, password: &str) -> Result<LoginOutcome, String> {
        self.login_step_impl(username, password)
    }

    fn login_mfa(
        &self,
        mfa_token: &str,
        code: Option<&str>,
        recovery_code: Option<&str>,
    ) -> Result<LoginOutcome, MfaError> {
        self.login_mfa_impl(mfa_token, code, recovery_code)
    }

    fn mfa(&self) -> Option<&dyn MfaApi> {
        Some(self)
    }

    fn passkeys(&self) -> Option<&dyn obop_commands::PasskeyApi> {
        Some(self)
    }

    fn sso(&self) -> Option<&dyn obop_commands::SsoApi> {
        Some(self)
    }

    fn accounts(&self) -> Option<&dyn obop_commands::AccountsApi> {
        Some(self)
    }

    fn issue_agent_token(
        &self,
        owner: &Principal,
        scopes: &[String],
        label: &str,
    ) -> Result<String, String> {
        // No list of registered groups from the caller: only what the owner holds by name.
        self.issue_agent_token_among(owner, scopes, label, &[])
    }

    fn issue_agent_token_among(
        &self,
        owner: &Principal,
        scopes: &[String],
        label: &str,
        registered: &[String],
    ) -> Result<String, String> {
        self.issue_agent_token_as(&owner.id, owner, scopes, label, registered)
    }

    fn agent_tokens_of(&self, owner: &Principal) -> Option<Vec<obop_commands::AgentTokenInfo>> {
        if !owner.is_person() {
            return Some(Vec::new());
        }
        let now = self.now();
        let mut out: Vec<_> = self
            .backend
            .find("auth_token", "owner", &J::from(owner.id.as_str()))
            .ok()?
            .into_iter()
            .filter(|(_, t)| plain_agent_token(t, now))
            .map(|(id, t)| {
                let mut scopes = t.get("scopes").map(str_list).unwrap_or_default();
                scopes.sort_unstable();
                obop_commands::AgentTokenInfo {
                    id,
                    label: t
                        .get("label")
                        .and_then(J::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    scopes,
                    created_at: t.get("created_at").and_then(J::as_u64).unwrap_or(0),
                }
            })
            .collect();
        out.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        Some(out)
    }

    fn revoke_agent_token(&self, owner: &Principal, id: &str) -> bool {
        if !owner.is_person() {
            return false;
        }
        let Ok(Some(t)) = self.backend.get("auth_token", id) else {
            return false;
        };
        // Somebody else's token answers like one that is not there.
        if t.get("owner").and_then(J::as_str) != Some(owner.id.as_str())
            || !plain_agent_token(&t, self.now())
        {
            return false;
        }
        let mut p = Fields::new();
        p.insert("revoked".into(), J::Bool(true));
        if !self.backend.update("auth_token", id, &p).unwrap_or(false) {
            return false;
        }
        let label = t.get("label").and_then(J::as_str).unwrap_or_default();
        self.event(
            &owner.id,
            "agent_token_revoked",
            &owner.id,
            &format!("label {label:?}"),
        );
        true
    }
}

/// The longest label of an agent token, in characters.
pub const AGENT_LABEL_MAX: usize = 80;

/// An agent token a person issued by hand and that still works: not revoked, not expired, and
/// not the access token of a connected application or of a phone (those are shown, and ended,
/// with their grant or device).
fn plain_agent_token(t: &Fields, now: u64) -> bool {
    t.get("kind").and_then(J::as_str) == Some("agent")
        && t.get("revoked").and_then(J::as_bool) != Some(true)
        && t.get("expires_at")
            .and_then(J::as_u64)
            .is_none_or(|e| e > now)
        && t.get("oauth_grant").is_none_or(J::is_null)
        && t.get("audience").is_none_or(J::is_null)
        && t.get("mobile").and_then(J::as_bool) != Some(true)
}

#[cfg(test)]
mod admin_tests;
#[cfg(test)]
mod brand_enrol_tests;
#[cfg(test)]
mod device_tests;
#[cfg(test)]
mod epoch_tests;
#[cfg(test)]
mod lockout_tests;
#[cfg(test)]
mod mfa_tests;
#[cfg(test)]
mod passkey_tests;
#[cfg(test)]
mod signin_fault_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn auth() -> Auth {
        Auth::with_test_params(obop_storage::open("memory:").unwrap())
    }

    fn groups(g: &[&str]) -> Vec<String> {
        g.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn login_resolves_to_the_user() {
        let a = auth();
        a.create_user("Ada", "correct horse battery", &groups(&["base.user"]))
            .unwrap();
        let t = a.login("ada", "correct horse battery").unwrap();
        let p = a.resolve(&t).unwrap();
        assert_eq!((p.id.as_str(), p.kind), ("ada", PrincipalKind::User));
        assert!(p.allows("base.user") && !p.allows("sales.user"));
    }

    #[test]
    fn wrong_password_and_unknown_user_fail_the_same_way() {
        let a = auth();
        a.create_user("ada", "correct horse battery", &[]).unwrap();
        assert_eq!(a.login("ada", "nope"), a.login("ghost", "nope"));
        assert!(a.resolve("obop_made_up").is_none());
    }

    #[test]
    fn lockout_after_repeated_failures() {
        let a = auth();
        a.create_user("ada", "correct horse battery", &[]).unwrap();
        for _ in 0..MAX_FAILURES {
            assert!(a.login("ada", "wrong").is_err());
        }
        let e = a.login("ada", "correct horse battery").unwrap_err();
        assert!(e.contains("too many"));
    }

    #[test]
    fn weak_passwords_and_duplicates_are_rejected() {
        let a = auth();
        assert!(a.create_user("ada", "short", &[]).is_err());
        a.create_user("ada", "correct horse battery", &[]).unwrap();
        assert!(
            a.create_user("ADA", "correct horse battery", &[])
                .unwrap_err()
                .contains("exists")
        );
        assert!(a.create_user("a b", "correct horse battery", &[]).is_err());
    }

    /// D69 amendment of 2026-11-14: the administrator group holds every group, so its holder may
    /// hand on a registered group the account does not list by name. Nobody else gains anything.
    #[test]
    fn an_administrator_delegates_any_registered_group_and_nobody_else_does() {
        let a = auth();
        a.create_user(
            "root",
            "correct horse battery",
            &groups(&["platform.admin"]),
        )
        .unwrap();
        a.create_user("ada", "correct horse battery", &groups(&["base.user"]))
            .unwrap();
        let admin = a.principal("root").unwrap();
        let ada = a.principal("ada").unwrap();
        let registered = groups(&["platform.admin", "base.user", "documents.user"]);
        let wanted = groups(&["documents.user"]);
        // A caller that does not say which groups exist changes nothing: only what the account lists.
        assert!(a.issue_agent_token(&admin, &wanted, "x").is_err());
        let t = a
            .issue_agent_token_among(&admin, &wanted, "files", &registered)
            .unwrap();
        let agent = a.resolve(&t).unwrap();
        assert_eq!(agent.kind, PrincipalKind::Agent);
        assert!(agent.allows("documents.user"));
        assert!(
            !agent.allows("base.user") && !agent.allows("platform.admin"),
            "the token is its scopes, not the administrator"
        );
        // Not a group that does not exist, and never `*`.
        assert_eq!(
            a.issue_agent_token_among(&admin, &groups(&["nope.user"]), "x", &registered)
                .unwrap_err(),
            "nope.user is not a registered permission group, so it cannot be delegated"
        );
        assert!(
            a.issue_agent_token_among(&admin, &groups(&["*"]), "x", &registered)
                .unwrap_err()
                .starts_with("`*` names no group")
        );
        // A person who is not an administrator hands on what their account lists, as before.
        assert_eq!(
            a.issue_agent_token_among(&ada, &wanted, "x", &registered)
                .unwrap_err(),
            "you do not hold documents.user, so you cannot delegate it"
        );
        assert!(
            a.issue_agent_token_among(&ada, &groups(&["base.user"]), "x", &registered)
                .is_ok()
        );
        // A session limited to enrolment holds nothing to hand on.
        let mut limited = admin.clone();
        limited.mfa_limited = true;
        assert!(
            a.issue_agent_token_among(&limited, &wanted, "x", &registered)
                .is_err()
        );
        // The label is short and on one line; an empty one is "agent".
        let own = groups(&["platform.admin"]);
        assert!(
            a.issue_agent_token_among(&admin, &own, &"x".repeat(81), &registered)
                .is_err()
        );
        assert!(
            a.issue_agent_token_among(&admin, &own, "two\nlines", &registered)
                .is_err()
        );
        assert!(
            a.issue_agent_token_among(&admin, &own, "  ", &registered)
                .is_ok()
        );
        assert!(
            a.agent_tokens_of(&admin)
                .unwrap()
                .iter()
                .any(|t| t.label == "agent")
        );
    }

    #[test]
    fn a_person_lists_and_ends_their_own_agent_tokens_and_no_one_elses() {
        let a = auth();
        a.create_user(
            "root",
            "correct horse battery",
            &groups(&["platform.admin"]),
        )
        .unwrap();
        a.create_user(
            "ada",
            "correct horse battery",
            &groups(&["base.user", "sales.user"]),
        )
        .unwrap();
        let root = a.principal("root").unwrap();
        let ada = a.principal("ada").unwrap();
        let token = a
            .issue_agent_token(&ada, &groups(&["sales.user", "base.user"]), "crm bot")
            .unwrap();
        // A session of the same person is not an agent token.
        a.login("ada", "correct horse battery").unwrap();
        let list = a.agent_tokens_of(&ada).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].label, "crm bot");
        assert_eq!(list[0].scopes, groups(&["base.user", "sales.user"]));
        assert_ne!(list[0].id, token, "the id is not the token");
        let id = list[0].id.clone();
        // Nobody else sees or ends it, an administrator included, and an agent ends nothing.
        assert!(a.agent_tokens_of(&root).unwrap().is_empty());
        assert!(!a.revoke_agent_token(&root, &id));
        let agent = a.resolve(&token).unwrap();
        assert!(!a.revoke_agent_token(&agent, &id));
        assert!(a.agent_tokens_of(&agent).unwrap().is_empty());
        assert!(a.resolve(&token).is_some());
        // Its owner ends it, once.
        assert!(a.revoke_agent_token(&ada, &id));
        assert!(a.resolve(&token).is_none());
        assert!(a.agent_tokens_of(&ada).unwrap().is_empty());
        assert!(!a.revoke_agent_token(&ada, &id));
        assert!(!a.revoke_agent_token(&ada, "not-an-id"));
    }

    #[test]
    fn agent_tokens_are_scoped_to_what_the_owner_holds() {
        let a = auth();
        a.create_user(
            "ada",
            "correct horse battery",
            &groups(&["base.user", "sales.user"]),
        )
        .unwrap();
        let human = a
            .resolve(&a.login("ada", "correct horse battery").unwrap())
            .unwrap();
        assert!(
            a.issue_agent_token(&human, &groups(&["invoicing.manager"]), "x")
                .is_err()
        );
        assert!(a.issue_agent_token(&human, &[], "x").is_err());
        let t = a
            .issue_agent_token(&human, &groups(&["sales.user"]), "crm bot")
            .unwrap();
        let agent = a.resolve(&t).unwrap();
        assert_eq!(agent.kind, PrincipalKind::Agent);
        assert!(
            agent.allows("sales.user") && !agent.allows("base.user"),
            "scope narrows the owner's groups"
        );
        // An agent cannot mint further tokens.
        assert!(
            a.issue_agent_token(&agent, &groups(&["sales.user"]), "y")
                .is_err()
        );
    }

    #[test]
    fn revoked_and_deleted_owners_stop_working() {
        let a = auth();
        a.create_user("ada", "correct horse battery", &groups(&["base.user"]))
            .unwrap();
        let t = a.login("ada", "correct horse battery").unwrap();
        assert!(a.revoke(&t));
        assert!(a.resolve(&t).is_none());
    }

    #[test]
    fn a_hash_moves_between_databases_and_the_same_password_works() {
        let a = auth();
        a.create_user("ada@example.test", "correct horse battery", &[])
            .unwrap();
        let h = a.password_hash("ada@example.test").unwrap();
        let b = Auth::with_test_params(obop_storage::open("memory:").unwrap());
        b.create_user_hashed("ada@example.test", &h, &groups(&["platform.admin"]))
            .unwrap();
        let t = b
            .login("ada@example.test", "correct horse battery")
            .unwrap();
        assert!(b.resolve(&t).unwrap().allows("anything"), "platform.admin");
        assert!(b.create_user_hashed("x", "plain", &[]).is_err());
    }

    #[test]
    fn tokens_are_stored_hashed() {
        let b = obop_storage::open("memory:").unwrap();
        let a = Auth::with_test_params(b.clone());
        a.create_user("ada", "correct horse battery", &[]).unwrap();
        let t = a.login("ada", "correct horse battery").unwrap();
        assert!(
            b.get("auth_token", &t).unwrap().is_none(),
            "the raw token is never a key"
        );
        assert!(b.get("auth_token", &token_hash(&t)).unwrap().is_some());
        let user = b.get("auth_user", "ada").unwrap().unwrap();
        assert!(
            user["password_hash"]
                .as_str()
                .unwrap()
                .starts_with("$argon2id$")
        );
    }
}
