//! Account administration (D69): create a user, edit the groups of a user, offboard a user, and
//! the operator's bulk import. Each rule is written once here and reached from REST, MCP (as
//! proposals an administrator approves) and the `obop admin` CLI.
//!
//! The groups of a user are read from the stored account on every request (`user_principal`),
//! so an edit takes effect on the next request of every live session and agent token; nothing
//! has to be revoked for it. Everything that changes an account is written to the security log
//! with who did it and what changed. Nothing logged or returned is a secret, except the
//! temporary password a creation hands back once.

use crate::{Auth, Change, MIN_PASSWORD_CHARS, str_list, suggest_temporary_password};
use obop_commands::{ADMIN_GROUP, AccountsApi, MfaError, NewUser, Principal, PrincipalKind};
use obop_storage::Fields;
use serde_json::{Value as J, json};

/// Longest group list and longest group name accepted.
const MAX_GROUPS: usize = 64;
const MAX_GROUP_CHARS: usize = 64;
/// Longest note kept with an offboarding.
const MAX_NOTE_CHARS: usize = 200;
/// Longest user name.
const MAX_NAME_CHARS: usize = 64;

/// One line of an import file (`obop admin import`).
#[derive(Debug, Clone)]
pub struct ImportRow {
    /// The line of the file, for messages.
    pub line: usize,
    pub name: String,
    pub groups: Vec<String>,
}

/// What an import decided for one row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportOutcome {
    pub line: usize,
    pub name: String,
    pub groups: Vec<String>,
    /// `None`: the row is fine. `Some`: why it was refused.
    pub error: Option<String>,
    /// The temporary password, set only for a real run of a fine row. Shown once.
    pub temporary_password: Option<String>,
}

/// The name the log shows for the actor and the account it signs in as. An approved proposal of
/// an agent runs as `alice/agent` (what the dispatcher calls the principal), whose account is
/// `alice`; user names cannot contain `/`, so the two cannot be mistaken for one another.
fn admin_of(p: &Principal) -> Result<(&str, &str), MfaError> {
    if p.kind == PrincipalKind::User && p.groups.contains(ADMIN_GROUP) && !p.mfa_limited {
        Ok((&p.id, p.id.strip_suffix("/agent").unwrap_or(&p.id)))
    } else {
        Err(MfaError::NotFound)
    }
}

/// Trim, drop empty entries, refuse odd names, remove duplicates (first one wins).
pub fn clean_groups(groups: &[String]) -> Result<Vec<String>, MfaError> {
    let mut out: Vec<String> = Vec::new();
    for g in groups {
        let g = g.trim();
        if g.is_empty() {
            continue;
        }
        let ok = g.chars().count() <= MAX_GROUP_CHARS
            && g.chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-:".contains(c));
        if !ok {
            return Err(MfaError::Invalid(format!(
                "{g:?} is not a permission group name (letters, digits and . _ - :)"
            )));
        }
        if !out.iter().any(|x| x == g) {
            out.push(g.to_string());
        }
    }
    if out.len() > MAX_GROUPS {
        return Err(MfaError::Invalid(format!(
            "at most {MAX_GROUPS} groups per account"
        )));
    }
    Ok(out)
}

fn valid_name(name: &str) -> Result<(), MfaError> {
    let ok = !name.is_empty()
        && name.chars().count() <= MAX_NAME_CHARS
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-@".contains(c));
    if ok {
        Ok(())
    } else {
        Err(MfaError::Invalid(
            "username must be letters, digits or . _ - @ (at most 64 characters)".into(),
        ))
    }
}

/// A note for the log: no control characters, shortened.
fn clean_note(note: &str) -> String {
    note.chars()
        .filter(|c| !c.is_control())
        .take(MAX_NOTE_CHARS)
        .collect::<String>()
        .trim()
        .to_string()
}

fn list(items: &[String]) -> String {
    if items.is_empty() {
        "(none)".into()
    } else {
        items.join(", ")
    }
}

/// What ending the tokens of an account ended.
#[derive(Default)]
pub(crate) struct Tally {
    pub sessions: usize,
    pub agents: usize,
    pub apps: usize,
    pub pending: usize,
}

impl Auth {
    /// Every group in `groups` must be registered (`known`), except those the person already
    /// holds: a group of a module that was removed can stay, it can only not be added.
    fn check_known(
        groups: &[String],
        known: &[String],
        already: &[String],
    ) -> Result<(), MfaError> {
        match groups
            .iter()
            .find(|g| !known.contains(g) && !already.contains(g))
        {
            Some(g) => Err(MfaError::Invalid(format!(
                "{g:?} is not a registered permission group; see the groups the modules declare (GET /api/meta)"
            ))),
            None => Ok(()),
        }
    }

    /// Make every live session of `name` agree with the policy for `groups`: limited to
    /// enrolment (or to the owed password change) or not. Returns how many changed. Called after
    /// the groups change, so an account that just became an administrator on an instance that
    /// demands two-factor for administrators cannot keep using a session that never proved one.
    fn resync_limits(&self, name: &str, groups: &[String], ch: &Change) -> Result<usize, MfaError> {
        let limited = self
            .settled(|| {
                self.must_change_password(name)
                    || (self.enrolment_required_for(groups) && !self.has_second_factor(name))
            })
            .map_err(|e| match e {
                MfaError::Unavailable(m) => self.incomplete(ch, "reading the two-factor policy", m),
                other => other,
            })?;
        let now = self.now();
        let mut n = 0;
        // A session that is not limited stays so unless this step runs: refused, it fails the
        // request (the groups did change) instead of reporting "no session needed a change".
        self.needed(ch, "relimiting the sessions", || {
            for (id, t) in self.backend.list("auth_token")? {
                let live = t.get("revoked").and_then(J::as_bool) != Some(true)
                    && t.get("expires_at")
                        .and_then(J::as_u64)
                        .is_none_or(|e| e > now);
                if live
                    && t.get("owner").and_then(J::as_str) == Some(name)
                    && t.get("kind").and_then(J::as_str) == Some("user")
                    && t.get("limited").and_then(J::as_bool) != Some(limited)
                {
                    let mut p = Fields::new();
                    p.insert("limited".into(), J::Bool(limited));
                    if self.backend.update("auth_token", &id, &p)? {
                        n += 1;
                    }
                }
            }
            Ok(())
        })?;
        Ok(n)
    }

    // ---- create ---------------------------------------------------------------------------

    /// `actor` is who the log names (an administrator, or `operator` for the CLI).
    pub(crate) fn create_user_as(
        &self,
        actor: &str,
        new: &NewUser<'_>,
        known: &[String],
    ) -> Result<J, MfaError> {
        let name = new.name.trim().to_lowercase();
        valid_name(&name)?;
        let groups = clean_groups(new.groups)?;
        Self::check_known(&groups, known, &[])?;
        let (password, generated) = match new.password {
            Some(p) => (p.to_string(), false),
            None => (suggest_temporary_password(), true),
        };
        if password.chars().count() < MIN_PASSWORD_CHARS {
            return Err(MfaError::Invalid(format!(
                "the password must be at least {MIN_PASSWORD_CHARS} characters"
            )));
        }
        let hash = self
            .hash_password(&password)
            .map_err(MfaError::Unavailable)?;
        let _g = self.mfa_guard()?;
        let mut f = Fields::new();
        f.insert("password_hash".into(), J::from(hash));
        f.insert("groups".into(), json!(groups));
        f.insert(
            "must_change_password".into(),
            J::Bool(new.must_change_password || generated),
        );
        f.insert("created_at".into(), J::from(self.now()));
        f.insert("created_by".into(), J::from(actor));
        self.backend.insert("auth_user", &name, &f).map_err(|e| {
            if e.contains("duplicate") || e.contains("UNIQUE") || e.contains("unique") {
                MfaError::Invalid(format!("user {name} exists"))
            } else {
                MfaError::Unavailable(e)
            }
        })?;
        self.event(
            &name,
            "user_create",
            actor,
            &format!(
                "account created; groups: {}; {}",
                list(&groups),
                if generated {
                    "a temporary password was generated"
                } else if new.must_change_password {
                    "an initial password was set and must be changed at the first sign-in"
                } else {
                    "an initial password was set"
                }
            ),
        );
        let mut out = json!({
            "user": name,
            "groups": groups,
            "must_change_password": new.must_change_password || generated,
        });
        if generated {
            out["temporary_password"] = J::from(password);
        }
        Ok(out)
    }

    // ---- groups ---------------------------------------------------------------------------

    /// `actor_user`: the account the actor signs in as (never the operator), so an administrator
    /// cannot take their own administrator access away.
    pub(crate) fn set_groups_as(
        &self,
        actor: &str,
        actor_user: Option<&str>,
        target: &str,
        groups: &[String],
        known: &[String],
    ) -> Result<J, MfaError> {
        let target = target.trim().to_lowercase();
        let after = clean_groups(groups)?;
        let _g = self.mfa_guard()?;
        let u = self.known_user(&target)?;
        let before = str_list(&u["groups"]);
        Self::check_known(&after, known, &before)?;
        let added: Vec<String> = after
            .iter()
            .filter(|g| !before.contains(g))
            .cloned()
            .collect();
        let removed: Vec<String> = before
            .iter()
            .filter(|g| !after.contains(g))
            .cloned()
            .collect();
        let mut detail = format!(
            "groups changed: added {}; removed {}; now {}",
            list(&added),
            list(&removed),
            list(&after)
        );
        if added.is_empty() && removed.is_empty() {
            // Nothing to change. The repeat of a request that changed the groups and could not
            // relimit the sessions lands here, and must finish that (it is safe to repeat, and
            // does nothing when every session already agrees).
            let ch = Change::new(&target, "user_groups", actor, "groups unchanged");
            let relimited = self.resync_limits(&target, &before, &ch)?;
            if relimited > 0 {
                self.event(
                    &target,
                    "user_groups",
                    actor,
                    &format!(
                        "{relimited} session(s) now follow the two-factor policy for the groups ({})",
                        list(&before)
                    ),
                );
            }
            return Ok(
                json!({"user": target, "before": before, "after": before, "changed": false}),
            );
        }
        if removed.iter().any(|g| g == ADMIN_GROUP) {
            if actor_user == Some(target.as_str()) {
                return Err(MfaError::Invalid(
                    "you cannot take administrator access from yourself; ask another administrator"
                        .into(),
                ));
            }
            if u["disabled"].as_bool() != Some(true) && self.enabled_admins()? <= 1 {
                return Err(MfaError::Invalid(
                    "the last enabled administrator keeps the group; make another administrator first"
                        .into(),
                ));
            }
        }
        let mut patch = Fields::new();
        patch.insert("groups".into(), json!(after));
        self.put(&target, &patch)?;
        let ch = Change::new(&target, "user_groups", actor, &detail);
        let relimited = self.resync_limits(&target, &after, &ch)?;
        if relimited > 0 {
            detail.push_str(&format!(
                "; {relimited} session(s) now follow the two-factor policy for the new groups"
            ));
        }
        self.event(&target, "user_groups", actor, &detail);
        Ok(json!({
            "user": target,
            "before": before,
            "after": after,
            "added": added,
            "removed": removed,
            "changed": true,
        }))
    }

    // ---- offboarding ----------------------------------------------------------------------

    pub(crate) fn offboard_as(
        &self,
        actor: &str,
        actor_user: Option<&str>,
        target: &str,
        note: &str,
    ) -> Result<J, MfaError> {
        let target = target.trim().to_lowercase();
        if actor_user == Some(target.as_str()) {
            return Err(MfaError::Invalid(
                "you cannot offboard your own account".into(),
            ));
        }
        let _g = self.mfa_guard()?;
        let u = self.known_user(&target)?;
        let is_admin = str_list(&u["groups"]).iter().any(|g| g == ADMIN_GROUP);
        if is_admin && u["disabled"].as_bool() != Some(true) && self.enabled_admins()? <= 1 {
            return Err(MfaError::Invalid(
                "the last enabled administrator cannot be offboarded".into(),
            ));
        }
        let note = clean_note(note);
        // Disable first: from here no token of the account resolves, whatever happens next.
        let mut patch = Fields::new();
        patch.insert("disabled".into(), J::Bool(true));
        patch.insert("offboarded".into(), J::Bool(true));
        patch.insert("offboarded_at".into(), J::from(self.now()));
        patch.insert("offboarded_by".into(), J::from(actor));
        // Written before the account goes: if ending its tokens is then refused, enabling the
        // account later finishes that first, so no token the offboarding missed comes back.
        self.owe_revocation(&target, "user_offboard", None)?;
        let before = obop_storage::fault::change_count();
        if let Err(e) = self.put(&target, &patch) {
            self.retract_owed(&target, "user_offboard", &e, before);
            return Err(e);
        }

        // Every token of the account: sessions, agent tokens, and the access tokens of connected
        // applications. They are marked revoked, so enabling the account later does not bring
        // any of them back.
        //
        // The account is already disabled and what follows is a list of things to end, each read
        // again when the step runs again: a refused list or write is tried again, and if it is
        // still refused the offboarding fails as a partial request (503) and says so in the log,
        // never "0 sessions ended". Offboarding the same account again finishes the rest, and so
        // does enabling the account (a note says what is owed, see `settle_before_enable`).
        let ch = Change::new(
            &target,
            "user_offboard",
            actor,
            "account disabled and offboarded",
        );
        let tally = self.end_account_tokens(&target, &ch)?;
        // Every token of the account is ended, so a disable that could not end its sessions is
        // finished by this as well.
        self.clear_owed(&target, "user_offboard");
        self.clear_owed(&target, "user_disable");
        let Tally {
            sessions,
            agents,
            apps,
            pending,
        } = tally;
        // A lockout counter is not worth failing for: it expires by itself.
        let _ = self.backend.counter_reset(&Self::failure_key(&target));
        let mut detail = format!(
            "account disabled and offboarded; {sessions} session(s), {agents} agent token(s) and {apps} connected app(s) ended for good; {pending} half-finished sign-in(s) dropped"
        );
        if !note.is_empty() {
            detail.push_str(&format!("; note: {note}"));
        }
        self.event(&target, "user_offboard", actor, &detail);
        Ok(json!({
            "user": target,
            "disabled": true,
            "offboarded": true,
            "revoked_sessions": sessions,
            "revoked_agent_tokens": agents,
            "revoked_apps": apps,
            "note": note,
        }))
    }

    /// End every token of an account for good: sessions, agent tokens, the access tokens of its
    /// connected applications, the applications' grants and refresh tokens, its half-finished
    /// sign-ins and its passkey proof. Each step reads again what is left to do, so it is safe to
    /// run again; a refusal of the store is tried again and then fails as a partial request.
    pub(crate) fn end_account_tokens(&self, tid: &str, ch: &Change) -> Result<Tally, MfaError> {
        let mut t = Tally::default();
        self.needed(ch, "ending the tokens of the account", || {
            for (id, tok) in self.backend.list("auth_token")? {
                if tok.get("owner").and_then(J::as_str) != Some(tid)
                    || tok.get("revoked").and_then(J::as_bool) == Some(true)
                {
                    continue;
                }
                let mut p = Fields::new();
                p.insert("revoked".into(), J::Bool(true));
                if !self.backend.update("auth_token", &id, &p)? {
                    continue;
                }
                // The access token of a connected application is counted with its grant below.
                if tok.get("oauth_grant").is_some() {
                    continue;
                }
                if tok.get("kind").and_then(J::as_str) == Some("agent") {
                    t.agents += 1;
                } else {
                    t.sessions += 1;
                }
            }
            // Connected applications: end each grant and forget its refresh tokens, or a refresh
            // token would mint a new access token the day the account is enabled again.
            for (gid, g) in self.backend.find("oauth_grant", "owner", &J::from(tid))? {
                if g.get("revoked").and_then(J::as_bool) != Some(true) {
                    let mut p = Fields::new();
                    p.insert("revoked".into(), J::Bool(true));
                    p.insert("revoked_at".into(), J::from(self.now()));
                    if self.backend.update("oauth_grant", &gid, &p)? {
                        t.apps += 1;
                    }
                }
                for (rid, _) in
                    self.backend
                        .find("oauth_refresh", "grant", &J::from(gid.as_str()))?
                {
                    self.backend.delete("oauth_refresh", &rid)?;
                }
            }
            // A half-finished sign-in and a passkey proof given a moment ago.
            for (id, _) in self.backend.find("auth_mfa_token", "user", &J::from(tid))? {
                if self.backend.delete("auth_mfa_token", &id)? {
                    t.pending += 1;
                }
            }
            self.backend.kv_delete(&crate::passkey::proof_key(tid))?;
            Ok(())
        })?;
        Ok(t)
    }

    /// Before an account is enabled: finish what a disable or an offboarding of it left undone
    /// because the store refused (a note on the account says so). Otherwise enabling would bring
    /// back the sessions, agent tokens and connected apps that were meant to end. A refusal here
    /// refuses the enabling.
    pub(crate) fn settle_before_enable(&self, name: &str, ch: &Change) -> Result<(), MfaError> {
        let u = self.user_record(name)?;
        let notes = crate::account::owed_notes(&u);
        if notes.contains_key("user_offboard") {
            // Ends every token of the account, which covers what a disable left as well.
            self.end_account_tokens(name, ch)?;
            self.clear_owed(name, "user_offboard");
            self.clear_owed(name, "user_disable");
        } else if notes.contains_key("user_disable") {
            self.finish_owed(name, "user_disable", ch)?;
        }
        Ok(())
    }

    // ---- the operator's import ------------------------------------------------------------

    /// Check `rows` and, unless `dry_run`, create every account in one transaction. All rows are
    /// checked first; if one is refused, nothing is created. The temporary passwords are
    /// generated here and returned once, in the outcomes.
    pub fn import_users(
        &self,
        rows: &[ImportRow],
        known: &[String],
        dry_run: bool,
    ) -> Vec<ImportOutcome> {
        let mut outcomes: Vec<ImportOutcome> = Vec::new();
        for r in rows {
            let name = r.name.trim().to_lowercase();
            let mut error = valid_name(&name).err().map(describe);
            let groups = match clean_groups(&r.groups) {
                Ok(g) => g,
                Err(e) => {
                    error = error.or(Some(describe(e)));
                    Vec::new()
                }
            };
            if error.is_none() {
                error = Self::check_known(&groups, known, &[]).err().map(describe);
            }
            if error.is_none() && outcomes.iter().any(|o| o.name == name) {
                error = Some(format!("{name} appears twice in the file"));
            }
            if error.is_none()
                && self
                    .backend
                    .get("auth_user", &name)
                    .ok()
                    .flatten()
                    .is_some()
            {
                error = Some(format!("user {name} exists"));
            }
            outcomes.push(ImportOutcome {
                line: r.line,
                name,
                groups,
                error,
                temporary_password: None,
            });
        }
        if dry_run || outcomes.iter().any(|o| o.error.is_some()) {
            return outcomes;
        }
        let mut batch = Vec::new();
        for o in &mut outcomes {
            let password = suggest_temporary_password();
            let Ok(hash) = self.hash_password(&password) else {
                o.error = Some("could not hash the password".into());
                continue;
            };
            let mut f = Fields::new();
            f.insert("password_hash".into(), J::from(hash));
            f.insert("groups".into(), json!(o.groups));
            f.insert("must_change_password".into(), J::Bool(true));
            f.insert("created_at".into(), J::from(self.now()));
            f.insert("created_by".into(), J::from("operator"));
            batch.push(("auth_user".to_string(), o.name.clone(), f));
            o.temporary_password = Some(password);
        }
        if outcomes.iter().any(|o| o.error.is_some()) {
            outcomes
                .iter_mut()
                .for_each(|o| o.temporary_password = None);
            return outcomes;
        }
        if let Err(e) = self.backend.insert_many(&batch) {
            for o in &mut outcomes {
                o.temporary_password = None;
                o.error = Some(format!("nothing was created: {e}"));
            }
            return outcomes;
        }
        for o in &outcomes {
            self.event(
                &o.name,
                "user_create",
                "operator",
                &format!(
                    "account created by import; groups: {}; a temporary password was generated",
                    list(&o.groups)
                ),
            );
        }
        outcomes
    }

    // ---- operator wrappers (the CLI has database access, which is its authority) -----------

    pub fn operator_create_user(&self, new: &NewUser<'_>, known: &[String]) -> Result<J, String> {
        self.create_user_as("operator", new, known)
            .map_err(describe)
    }

    pub fn operator_set_groups(
        &self,
        target: &str,
        groups: &[String],
        known: &[String],
    ) -> Result<J, String> {
        self.set_groups_as("operator", None, target, groups, known)
            .map_err(describe)
    }

    pub fn operator_offboard(&self, target: &str, note: &str) -> Result<J, String> {
        self.offboard_as("operator", None, target, note)
            .map_err(describe)
    }

    /// Enable or disable from the CLI. Same rules as the administrator's, minus "not yourself".
    pub fn operator_set_disabled(&self, target: &str, disabled: bool) -> Result<(), String> {
        self.set_disabled_as("operator", None, target, disabled)
            .map_err(describe)
    }
}

fn describe(e: MfaError) -> String {
    match e {
        MfaError::Invalid(m)
        | MfaError::Denied(m)
        | MfaError::Locked(m)
        | MfaError::Unavailable(m) => m,
        MfaError::BadPassword => "wrong password".into(),
        MfaError::Expired => "expired".into(),
        MfaError::NotFound => "not allowed".into(),
    }
}

impl AccountsApi for Auth {
    fn administrator(&self, name: &str) -> Option<Principal> {
        let p = self.user_principal(&name.trim().to_lowercase())?;
        p.groups.contains(ADMIN_GROUP).then_some(p)
    }

    fn create_user(
        &self,
        admin: &Principal,
        new: &NewUser<'_>,
        known: &[String],
    ) -> Result<J, MfaError> {
        let (actor, _) = admin_of(admin)?;
        self.create_user_as(actor, new, known)
    }

    fn set_user_groups(
        &self,
        admin: &Principal,
        target: &str,
        groups: &[String],
        known: &[String],
    ) -> Result<J, MfaError> {
        let (actor, user) = admin_of(admin)?;
        self.set_groups_as(actor, Some(user), target, groups, known)
    }

    fn offboard_user(&self, admin: &Principal, target: &str, note: &str) -> Result<J, MfaError> {
        let (actor, user) = admin_of(admin)?;
        self.offboard_as(actor, Some(user), target, note)
    }
}
