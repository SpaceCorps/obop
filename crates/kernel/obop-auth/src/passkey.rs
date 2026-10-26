//! Passkeys (WebAuthn, D65): registration and authentication ceremonies, the credential list of a
//! person, passwordless sign-in, and the second factor that satisfies the same policy as TOTP
//! (D53). Credentials are public keys in `auth_passkey` (nothing secret, so nothing sealed);
//! challenges are single use, expire after two minutes and live in the shared key-value store
//! (D59) so any server process can finish a ceremony another one started. The protocol checks are
//! in `webauthn`; this file holds the rules around them.

use crate::webauthn::{self as wa, b64url, b64url_decode, sha256};
use crate::{Auth, Change, hex, random_bytes, token_hash};
use obop_commands::audit::rfc3339;
use obop_commands::{
    ADMIN_GROUP, LoginOutcome, MfaError, PasskeyApi, Principal, PrincipalKind, Reauth, WebauthnCtx,
};
use obop_storage::{Fields, Op};
use serde_json::{Value as J, json};

pub const PASSKEY_TABLE: &str = "auth_passkey";
/// How long a challenge may wait for the browser's answer.
pub const CHALLENGE_SECS: u64 = 120;
/// How long a passkey proof stands in for a code.
pub const PROOF_SECS: u64 = 60;
pub const MAX_PASSKEYS: usize = 10;
const MAX_NAME_CHARS: usize = 64;
/// Ceremonies per client address and ten minutes (options and answers each).
const OPTIONS_PER_IP: u64 = 60;
const VERIFY_PER_IP: u64 = 40;
const TRANSPORTS: [&str; 6] = ["usb", "nfc", "ble", "internal", "hybrid", "smart-card"];

fn person(p: &Principal) -> Result<&str, MfaError> {
    // Agent tokens are not people: they cannot hold a passkey.
    if p.kind == PrincipalKind::User {
        Ok(&p.id)
    } else {
        Err(MfaError::NotFound)
    }
}

fn denied(m: &str) -> MfaError {
    MfaError::Denied(m.into())
}

fn challenge_key(challenge: &str) -> String {
    format!("auth.wa:{}", hex(&sha256(challenge.as_bytes())))
}

pub(crate) fn proof_key(user: &str) -> String {
    format!("auth.pkproof:{user}")
}

/// A name the person can read in a list: control characters gone, spaces collapsed, at most 64
/// characters.
fn clean_name(s: &str) -> String {
    let mut out = String::new();
    for w in s.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.extend(w.chars().filter(|c| !c.is_control()));
    }
    out.chars().take(MAX_NAME_CHARS).collect()
}

/// An IP address is not a relying party id (browsers refuse it), and credentials need a secure
/// context: https, or localhost for development.
pub fn ceremony_site_ok(ctx: &WebauthnCtx) -> Result<(), String> {
    let host = ctx.rp_id.as_str();
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if host.is_empty()
        || bare.parse::<std::net::IpAddr>().is_ok()
        || host.chars().all(|c| c.is_ascii_digit() || c == '.')
    {
        return Err("passkeys need the instance to be reached by a host name, not an IP address (set OBOP_PUBLIC_URL)".into());
    }
    let local = host == "localhost" || host.ends_with(".localhost");
    if !(ctx.origin.starts_with("https://") || (local && ctx.origin.starts_with("http://"))) {
        return Err("passkeys need a secure connection (https)".into());
    }
    Ok(())
}

impl Auth {
    // ---- state ----------------------------------------------------------------------------

    /// The person's passkeys as `(id, record)`.
    pub(crate) fn passkeys_of(&self, name: &str) -> Vec<(String, J)> {
        self.backend
            .find(PASSKEY_TABLE, "user", &J::from(name))
            .unwrap_or_default()
            .into_iter()
            .map(|(k, f)| (k, J::Object(f)))
            .collect()
    }

    /// The passkeys of a person, with a lookup that reports a refusal of the store: for the places
    /// that decide something from the list, where "none" read from a refused lookup would be wrong.
    pub(crate) fn passkeys_checked(&self, name: &str) -> Result<Vec<(String, J)>, MfaError> {
        Ok(self
            .backend
            .find(PASSKEY_TABLE, "user", &J::from(name))
            .map_err(MfaError::Unavailable)?
            .into_iter()
            .map(|(k, f)| (k, J::Object(f)))
            .collect())
    }

    pub fn passkey_count(&self, name: &str) -> usize {
        self.passkeys_of(&name.trim().to_lowercase()).len()
    }

    /// A second factor of any kind: an authenticator app or a passkey.
    pub fn has_second_factor(&self, name: &str) -> bool {
        self.mfa_enabled(name) || !self.passkeys_of(name).is_empty()
    }

    /// Remove every passkey of the person (the break-glass reset and the administrator's reset).
    ///
    /// A refused lookup or delete is an error, not "none left": a reset that leaves a passkey
    /// behind must not say it is done.
    pub(crate) fn clear_passkeys(&self, name: &str) -> Result<usize, MfaError> {
        let unavailable = MfaError::Unavailable;
        let mut n = 0;
        for (id, _) in self
            .backend
            .find(PASSKEY_TABLE, "user", &J::from(name))
            .map_err(unavailable)?
        {
            if self
                .backend
                .delete(PASSKEY_TABLE, &id)
                .map_err(unavailable)?
            {
                n += 1;
            }
        }
        let mut p = Fields::new();
        p.insert("passkey_login".into(), J::Bool(false));
        self.backend
            .update("auth_user", name, &p)
            .map_err(unavailable)?;
        self.backend
            .kv_delete(&proof_key(name))
            .map_err(unavailable)?;
        Ok(n)
    }

    fn user_handle(&self, name: &str) -> Result<Vec<u8>, MfaError> {
        let u = self.user_record(name)?;
        if let Some(h) = u["webauthn_handle"].as_str().and_then(b64url_decode) {
            return Ok(h);
        }
        // Random and unrelated to the account name, so it carries no personal data (WebAuthn 5.4.3).
        let h = random_bytes::<32>();
        let mut f = Fields::new();
        f.insert("webauthn_handle".into(), J::from(b64url(&h)));
        self.put(name, &f)?;
        Ok(h.to_vec())
    }

    fn handle_of(&self, name: &str) -> Option<Vec<u8>> {
        self.user_record(name)
            .ok()?
            .get("webauthn_handle")?
            .as_str()
            .and_then(b64url_decode)
    }

    fn site(&self, ctx: &WebauthnCtx) -> Result<(), MfaError> {
        ceremony_site_ok(ctx).map_err(MfaError::Unavailable)
    }

    fn pk_limit(&self, ctx: &WebauthnCtx, what: &str, max: u64) -> Result<(), MfaError> {
        let who = hex(&sha256(ctx.ip.as_bytes()));
        obop_storage::limit::hit(
            &*self.backend,
            &format!("auth.pk:{what}:{}", &who[..24]),
            max,
            600_000,
        )
        .map_err(|wait| MfaError::Locked(format!("too many attempts, try again in {wait} seconds")))
    }

    fn failed_pk_event(&self, user: &str, why: &str) {
        let first = self
            .backend
            .counter_add(&format!("auth.evt:passkey_failed:{user}"), 10_000, false)
            .map_or(true, |n| n == 1);
        if first {
            self.event(user, "passkey_failed", user, why);
        }
    }

    // ---- challenges -----------------------------------------------------------------------

    /// A fresh challenge, remembered for two minutes for one purpose (and one person, one pending
    /// sign-in). Returns the challenge as base64url.
    fn new_challenge(
        &self,
        ctx: &WebauthnCtx,
        purpose: &str,
        user: Option<&str>,
        mfa: Option<&str>,
    ) -> Result<String, MfaError> {
        let challenge = b64url(&random_bytes::<32>());
        let rec = json!({
            "purpose": purpose,
            "user": user,
            "mfa": mfa,
            "rp_id": ctx.rp_id,
            "origin": ctx.origin,
            "expires_at": self.now() + CHALLENGE_SECS,
        });
        self.backend
            .kv_put(
                &challenge_key(&challenge),
                &rec.to_string(),
                Some(CHALLENGE_SECS * 1000),
            )
            .map_err(MfaError::Unavailable)?;
        Ok(challenge)
    }

    /// Look a challenge up and use it up, whatever happens next: a second answer to the same
    /// challenge finds nothing (and of two answers racing, only the delete that succeeds goes on).
    fn take_challenge(
        &self,
        challenge: &str,
        purpose: &str,
        ctx: &WebauthnCtx,
    ) -> Result<J, MfaError> {
        let key = challenge_key(challenge);
        let raw = self.backend.kv_get(&key).ok().flatten();
        let used = self.backend.kv_delete(&key).unwrap_or(false);
        let gone = || denied("this request has expired or was already used; start again");
        let (Some(raw), true) = (raw, used) else {
            return Err(gone());
        };
        let rec: J = serde_json::from_str(&raw).map_err(|_| gone())?;
        if rec["expires_at"].as_u64().is_none_or(|e| e <= self.now()) {
            return Err(gone());
        }
        if rec["purpose"].as_str() != Some(purpose)
            || rec["rp_id"].as_str() != Some(&ctx.rp_id)
            || rec["origin"].as_str() != Some(&ctx.origin)
        {
            return Err(denied("this request was made for something else"));
        }
        Ok(rec)
    }

    // ---- re-authentication ------------------------------------------------------------------

    /// The password, and the second factor when one is on (a passkey proof counts).
    fn reauth(&self, name: &str, r: &Reauth<'_>) -> Result<(), MfaError> {
        if self.locked_out(name) {
            return Err(MfaError::Locked(
                "too many attempts, try again in a minute".into(),
            ));
        }
        if !self.password_ok(name, r.password) {
            self.record_failure(name);
            self.event(
                name,
                "passkey_failed",
                name,
                "wrong password while changing passkeys",
            );
            return Err(MfaError::BadPassword);
        }
        if self.has_second_factor(name) {
            self.check_second_factor(name, r.code, r.recovery_code)?;
        }
        Ok(())
    }

    /// A passkey proof given beforehand stands in for a code, once.
    pub(crate) fn take_proof(&self, name: &str) -> bool {
        let key = proof_key(name);
        let raw = self.backend.kv_get(&key).ok().flatten();
        let used = self.backend.kv_delete(&key).unwrap_or(false);
        used && raw
            .and_then(|r| serde_json::from_str::<J>(&r).ok())
            .and_then(|v| v["expires_at"].as_u64())
            .is_some_and(|e| e > self.now())
    }

    // ---- the assertion ----------------------------------------------------------------------

    /// Everything that makes an answer to a request challenge genuine. `bound_user` is the person
    /// the challenge was issued for (a second step or a proof), else the credential names the
    /// person (passwordless). On success the counter and the last use are updated.
    fn verify_assertion(
        &self,
        ctx: &WebauthnCtx,
        purpose: &str,
        credential: &J,
        require_uv: bool,
    ) -> Result<(String, J), MfaError> {
        let bad = |e: String| MfaError::Denied(e);
        let cred = wa::credential(credential).map_err(bad)?;
        let cd =
            wa::client_data(&cred.client_data_json, "webauthn.get", &ctx.origin).map_err(bad)?;
        let rec = self.take_challenge(&cd.challenge, purpose, ctx)?;
        let id = hex(&sha256(&cred.raw_id));
        let stored = self
            .backend
            .get(PASSKEY_TABLE, &id)
            .ok()
            .flatten()
            .map(J::Object)
            .ok_or_else(|| denied("this passkey is not registered here"))?;
        let owner = stored["user"].as_str().unwrap_or("").to_string();
        if let Some(bound) = rec["user"].as_str()
            && bound != owner
        {
            self.event(
                bound,
                "passkey_failed",
                bound,
                "a passkey of another account was presented",
            );
            return Err(denied("this passkey belongs to a different account"));
        }
        if stored["rp_id"].as_str() != Some(&ctx.rp_id) {
            return Err(denied(
                "this passkey was registered for a different address",
            ));
        }
        if purpose == "login" {
            // Passwordless: only a passkey that was user-verified when it was made, for a person
            // who allowed it, and not for a disabled account.
            if stored["user_verified"].as_bool() != Some(true)
                || !self
                    .user_record(&owner)
                    .is_ok_and(|u| u["passkey_login"].as_bool() == Some(true))
            {
                return Err(denied(
                    "this passkey cannot be used to sign in without a password",
                ));
            }
            if self.is_disabled(&owner) {
                self.event(&owner, "login_refused", &owner, "the account is disabled");
                return Err(denied("this passkey cannot be used to sign in"));
            }
        }
        let a = wa::assertion(&cred.response).map_err(bad)?;
        let auth = wa::parse_auth_data(&a.auth_data_bytes, false).map_err(bad)?;
        if auth.rp_id_hash != sha256(ctx.rp_id.as_bytes()) {
            return Err(denied("the passkey answered for a different relying party"));
        }
        if !auth.user_present() {
            return Err(denied(
                "the passkey did not confirm that a person was there",
            ));
        }
        if require_uv && !auth.user_verified() {
            return Err(denied(
                "the passkey did not verify the person (PIN, fingerprint or face)",
            ));
        }
        // The user handle, when the authenticator sends one, must be this person's. A usernameless
        // sign-in cannot do without it.
        match (&a.user_handle, self.handle_of(&owner)) {
            (Some(h), Some(want)) if *h == want => {}
            (None, Some(_)) if rec["user"].as_str().is_some() => {}
            _ => return Err(denied("the passkey names a different account")),
        }
        let key = wa::PublicKey {
            alg: stored["alg"].as_i64().unwrap_or(0),
            key: stored["key"]
                .as_str()
                .and_then(b64url_decode)
                .unwrap_or_default(),
        };
        let msg = wa::assertion_message(&a.auth_data_bytes, &cd.raw);
        if !wa::verify_signature(&key, &msg, &a.signature) {
            return Err(denied("the passkey's signature is not valid"));
        }
        // Clone detection (WebAuthn 6.1.1): a counter that does not go up means the credential may
        // exist twice. Authenticators without a counter always send zero and are let through.
        let before = stored["sign_count"].as_u64().unwrap_or(0);
        let now = u64::from(auth.counter);
        if (now != 0 || before != 0) && now <= before {
            self.event(
                &owner,
                "passkey_clone",
                &owner,
                &format!(
                    "the sign counter of passkey \"{}\" did not increase; it may have been copied",
                    stored["name"].as_str().unwrap_or("")
                ),
            );
            return Err(denied(
                "this passkey looks cloned (its counter went backwards) and was refused",
            ));
        }
        let mut p = Fields::new();
        p.insert("sign_count".into(), J::from(now));
        p.insert("last_used_at".into(), J::from(self.now()));
        p.insert("backup_state".into(), J::Bool(auth.backed_up()));
        let _ = self.backend.update(PASSKEY_TABLE, &id, &p);
        Ok((owner, rec))
    }

    fn mfa_token_user(&self, mfa_token: &str) -> Result<(String, u64), MfaError> {
        let key = token_hash(mfa_token);
        let rec = self
            .backend
            .get("auth_mfa_token", &key)
            .ok()
            .flatten()
            .map(J::Object)
            .ok_or(MfaError::Expired)?;
        if rec["expires_at"].as_u64().is_none_or(|e| e <= self.now()) {
            let _ = self.backend.delete("auth_mfa_token", &key);
            return Err(MfaError::Expired);
        }
        Ok((
            rec["user"].as_str().ok_or(MfaError::Expired)?.to_string(),
            rec["attempts"].as_u64().unwrap_or(0),
        ))
    }

    fn request_options(
        &self,
        ctx: &WebauthnCtx,
        challenge: &str,
        creds: &[(String, J)],
        uv: &str,
    ) -> J {
        let allow: Vec<J> = creds
            .iter()
            .filter_map(|(_, r)| {
                let mut c = json!({"type": "public-key", "id": r["cred_id"]});
                if let Some(t) = r["transports"].as_array()
                    && !t.is_empty()
                {
                    c["transports"] = J::Array(t.clone());
                }
                (!r["cred_id"].is_null()).then_some(c)
            })
            .collect();
        json!({"publicKey": {
            "challenge": challenge,
            "rpId": ctx.rp_id,
            "timeout": 120_000,
            "userVerification": uv,
            "allowCredentials": allow,
        }})
    }

    fn login_second(
        &self,
        ctx: &WebauthnCtx,
        mfa_token: &str,
        credential: &J,
    ) -> Result<LoginOutcome, MfaError> {
        let _g = self.mfa_guard()?;
        let (name, attempts) = self.mfa_token_user(mfa_token)?;
        let key = token_hash(mfa_token);
        if attempts >= crate::mfa::MFA_TOKEN_ATTEMPTS {
            let _ = self.backend.delete("auth_mfa_token", &key);
            return Err(MfaError::Locked(
                "too many wrong answers for this sign-in; enter your password again".into(),
            ));
        }
        if self.locked_out(&name) {
            return Err(MfaError::Locked(
                "too many attempts, try again in a minute".into(),
            ));
        }
        let result = self
            .verify_assertion(ctx, "second", credential, false)
            .and_then(|(user, rec)| {
                if user == name && rec["mfa"].as_str() == Some(key.as_str()) {
                    Ok(())
                } else {
                    Err(denied("this answer belongs to a different sign-in"))
                }
            });
        match result {
            Ok(()) => {
                // Single use: the pending sign-in is gone whatever happens next.
                let _ = self.backend.delete("auth_mfa_token", &key);
                self.event(&name, "passkey_used", &name, "second step of sign-in");
                self.session_outcome(&name, false)
                    .map_err(MfaError::Unavailable)
            }
            Err(e) => {
                let mut p = Fields::new();
                p.insert("attempts".into(), J::from(attempts + 1));
                let _ = self.backend.update("auth_mfa_token", &key, &p);
                self.record_failure(&name);
                self.failed_pk_event(&name, "a passkey answer was refused in the second step");
                if self.locked_out(&name) {
                    self.event(&name, "lockout", &name, "too many failed attempts");
                }
                Err(e)
            }
        }
    }

    fn login_passwordless(
        &self,
        ctx: &WebauthnCtx,
        credential: &J,
    ) -> Result<LoginOutcome, MfaError> {
        let (name, _) = self.verify_assertion(ctx, "login", credential, true)?;
        self.purge_tokens();
        self.event(
            &name,
            "passkey_login",
            &name,
            "signed in with a passkey alone",
        );
        self.session_outcome(&name, false)
            .map_err(MfaError::Unavailable)
    }
}

impl PasskeyApi for Auth {
    fn list(&self, p: &Principal, ctx: &WebauthnCtx) -> Result<J, MfaError> {
        let name = person(p)?;
        let u = self.user_record(name)?;
        let mut rows = self.passkeys_of(name);
        rows.sort_by_key(|(_, r)| r["created_at"].as_u64().unwrap_or(0));
        let uv_any = rows.iter().any(|(_, r)| r["user_verified"] == true);
        let list: Vec<J> = rows
            .iter()
            .map(|(id, r)| {
                json!({
                    "id": id,
                    "name": r["name"],
                    "created_at": r["created_at"].as_u64().map(rfc3339),
                    "last_used_at": r["last_used_at"].as_u64().map(rfc3339),
                    "user_verified": r["user_verified"],
                    "discoverable": r["discoverable"],
                    "backed_up": r["backup_state"],
                    "transports": r["transports"],
                })
            })
            .collect();
        let site = ceremony_site_ok(ctx);
        let groups: Vec<String> = p.groups.iter().cloned().collect();
        Ok(json!({
            "available": site.is_ok(),
            "unavailable_reason": site.err(),
            "rp_id": ctx.rp_id,
            "passkeys": list,
            "max": MAX_PASSKEYS,
            "passwordless": u["passkey_login"].as_bool() == Some(true),
            "passwordless_possible": uv_any,
            "totp": u["mfa_enabled"].as_bool() == Some(true),
            "policy_requires_second_factor": self.enrolment_required_for(&groups),
            "is_admin": p.groups.contains(ADMIN_GROUP),
        }))
    }

    fn register_options(
        &self,
        p: &Principal,
        ctx: &WebauthnCtx,
        reauth: &Reauth<'_>,
    ) -> Result<J, MfaError> {
        let name = person(p)?;
        self.site(ctx)?;
        self.pk_limit(ctx, "options", OPTIONS_PER_IP)?;
        let _g = self.mfa_guard()?;
        self.user_record(name)?;
        self.reauth(name, reauth)?;
        let existing = self.passkeys_of(name);
        if existing.len() >= MAX_PASSKEYS {
            return Err(MfaError::Invalid(format!(
                "you can have at most {MAX_PASSKEYS} passkeys; remove one first"
            )));
        }
        let handle = self.user_handle(name)?;
        let challenge = self.new_challenge(ctx, "register", Some(name), None)?;
        let exclude: Vec<J> = existing
            .iter()
            .filter(|(_, r)| !r["cred_id"].is_null())
            .map(|(_, r)| json!({"type": "public-key", "id": r["cred_id"]}))
            .collect();
        Ok(json!({"publicKey": {
            "rp": {"id": ctx.rp_id, "name": ctx.rp_name},
            "user": {"id": b64url(&handle), "name": name, "displayName": name},
            "challenge": challenge,
            "pubKeyCredParams": wa::ALGS.iter().map(|a| json!({"type": "public-key", "alg": a})).collect::<Vec<_>>(),
            "timeout": 120_000,
            "attestation": "none",
            "excludeCredentials": exclude,
            "authenticatorSelection": {"residentKey": "preferred", "userVerification": "preferred"},
            "extensions": {"credProps": true},
        }}))
    }

    fn register_finish(
        &self,
        p: &Principal,
        session: &str,
        ctx: &WebauthnCtx,
        name_given: &str,
        credential: &J,
    ) -> Result<J, MfaError> {
        let name = person(p)?;
        self.site(ctx)?;
        self.pk_limit(ctx, "verify", VERIFY_PER_IP)?;
        let _g = self.mfa_guard()?;
        let u = self.user_record(name)?;
        let bad = |e: String| MfaError::Denied(e);
        let cred = wa::credential(credential).map_err(bad)?;
        let cd =
            wa::client_data(&cred.client_data_json, "webauthn.create", &ctx.origin).map_err(bad)?;
        let rec = self.take_challenge(&cd.challenge, "register", ctx)?;
        if rec["user"].as_str() != Some(name) {
            return Err(denied("this request was made for a different account"));
        }
        let auth = wa::registration(&cred.response).map_err(bad)?;
        if auth.rp_id_hash != sha256(ctx.rp_id.as_bytes()) {
            return Err(denied("the passkey was made for a different relying party"));
        }
        if !auth.user_present() {
            return Err(denied(
                "the passkey did not confirm that a person was there",
            ));
        }
        let Some(att) = auth.attested.as_ref() else {
            return Err(denied("the registration carries no credential"));
        };
        if att.credential_id != cred.raw_id {
            return Err(denied("the credential id does not match the attested one"));
        }
        let existing = self.passkeys_of(name);
        if existing.len() >= MAX_PASSKEYS {
            return Err(MfaError::Invalid(format!(
                "you can have at most {MAX_PASSKEYS} passkeys; remove one first"
            )));
        }
        let id = hex(&sha256(&cred.raw_id));
        if self
            .backend
            .get(PASSKEY_TABLE, &id)
            .ok()
            .flatten()
            .is_some()
        {
            return Err(MfaError::Invalid(
                "this passkey is already registered".into(),
            ));
        }
        let mut label = clean_name(name_given);
        if label.is_empty() {
            label = format!("Passkey {}", existing.len() + 1);
        }
        let transports: Vec<&str> = cred.response["transports"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(J::as_str)
                    .filter(|t| TRANSPORTS.contains(t))
                    .collect()
            })
            .unwrap_or_default();
        let mut f = Fields::new();
        f.insert("user".into(), J::from(name));
        f.insert("name".into(), J::from(label.as_str()));
        f.insert("cred_id".into(), J::from(b64url(&cred.raw_id)));
        f.insert("alg".into(), J::from(att.key.alg));
        f.insert("key".into(), J::from(b64url(&att.key.key)));
        f.insert("sign_count".into(), J::from(u64::from(auth.counter)));
        f.insert("user_verified".into(), J::Bool(auth.user_verified()));
        f.insert("backup_eligible".into(), J::Bool(auth.backup_eligible()));
        f.insert("backup_state".into(), J::Bool(auth.backed_up()));
        f.insert(
            "discoverable".into(),
            cred.extensions["credProps"]["rk"]
                .as_bool()
                .map_or(J::Null, J::Bool),
        );
        f.insert("transports".into(), json!(transports));
        f.insert("aaguid".into(), J::from(hex(&att.aaguid)));
        f.insert("rp_id".into(), J::from(ctx.rp_id.as_str()));
        f.insert("created_at".into(), J::from(self.now()));
        f.insert("last_used_at".into(), J::Null);
        self.backend
            .insert(PASSKEY_TABLE, &id, &f)
            .map_err(MfaError::Unavailable)?;
        // A person's first second factor also gets recovery codes (once), as TOTP does.
        let had_factor = u["mfa_enabled"].as_bool() == Some(true) || !existing.is_empty();
        let mut out = json!({"id": id, "name": label, "user_verified": auth.user_verified()});
        if !had_factor && crate::str_list(&u["recovery"]).is_empty() {
            let (codes, patch) = self.fresh_recovery(name)?;
            self.put(name, &patch)?;
            out["recovery_codes"] = json!(codes);
        }
        // The session that just enrolled is no longer limited to enrolment.
        let mut lift = Fields::new();
        lift.insert("limited".into(), J::Bool(false));
        let _ = self
            .backend
            .update("auth_token", &token_hash(session), &lift);
        self.event(
            name,
            "passkey_register",
            name,
            &format!(
                "added \"{label}\" ({})",
                if auth.user_verified() {
                    "user verified"
                } else {
                    "not user verified"
                }
            ),
        );
        Ok(out)
    }

    fn rename(
        &self,
        p: &Principal,
        id: &str,
        name_given: &str,
        reauth: &Reauth<'_>,
    ) -> Result<(), MfaError> {
        let name = person(p)?;
        let _g = self.mfa_guard()?;
        self.reauth(name, reauth)?;
        let label = clean_name(name_given);
        if label.is_empty() {
            return Err(MfaError::Invalid("give the passkey a name".into()));
        }
        let rec = self.own_passkey(name, id)?;
        let mut patch = Fields::new();
        patch.insert("name".into(), J::from(label.as_str()));
        self.backend
            .update(PASSKEY_TABLE, id, &patch)
            .map_err(MfaError::Unavailable)?;
        self.event(
            name,
            "passkey_rename",
            name,
            &format!(
                "renamed \"{}\" to \"{label}\"",
                rec["name"].as_str().unwrap_or("")
            ),
        );
        Ok(())
    }

    fn remove(
        &self,
        p: &Principal,
        session: &str,
        id: &str,
        reauth: &Reauth<'_>,
    ) -> Result<(), MfaError> {
        let name = person(p)?;
        let _g = self.mfa_guard()?;
        self.reauth(name, reauth)?;
        let op = format!("passkey_remove:{id}");
        let rec = match self.own_passkey(name, id) {
            Ok(r) => r,
            // The repeat of a removal that could not end the other sessions finishes that (D74).
            Err(MfaError::Invalid(m)) if m == "no such passkey" => {
                let ch = Change::new(name, "passkey_remove", name, "passkey removed");
                if let Some(n) = self.finish_owed(name, &op, &ch)? {
                    self.event(
                        name,
                        "passkey_remove",
                        name,
                        &format!("removed; {n} other session(s) signed out (repeated)"),
                    );
                    return Ok(());
                }
                return Err(MfaError::Invalid(m));
            }
            Err(e) => return Err(e),
        };
        // What is left is read with a lookup that reports a refusal: a refused list is not "none"
        // (it made `len() - 1` underflow, and a person with other factors lose their recovery codes).
        let all = self.passkeys_checked(name)?;
        let left = all.len().saturating_sub(1);
        let totp = self.settled(|| self.mfa_enabled(name))?;
        let groups: Vec<String> = p.groups.iter().cloned().collect();
        if left == 0 && !totp && self.enrolment_required_for(&groups) {
            return Err(MfaError::Invalid(
                "your administrator requires a second factor, and this is your last one: add another passkey or an authenticator app first".into(),
            ));
        }
        let keep = token_hash(session);
        let mut patch = Fields::new();
        if !all
            .iter()
            .any(|(k, r)| k != id && r["user_verified"] == true)
        {
            patch.insert("passkey_login".into(), J::Bool(false));
        }
        if left == 0 && !totp {
            // No second factor left, so recovery codes protect nothing.
            patch.insert("recovery".into(), J::Null);
        }
        // What the repeat needs to finish the work goes into the same atomic batch as the removal:
        // the passkey, the user's record and the note that its other sessions are owed an end are
        // one change, so none of them can be missing (D74).
        patch.insert(
            "sessions_owed".into(),
            json!({"op": op, "keep": keep, "at": self.now()}),
        );
        self.backend
            .apply(&[
                Op::Delete {
                    model: PASSKEY_TABLE.into(),
                    id: id.into(),
                },
                Op::Merge {
                    model: "auth_user".into(),
                    id: name.into(),
                    patch,
                },
            ])
            .map_err(MfaError::Unavailable)?;
        // Like turning off the authenticator app: a stolen session must not outlive the factor.
        let ch = Change::new(
            name,
            "passkey_remove",
            name,
            &format!("removed \"{}\"", rec["name"].as_str().unwrap_or("")),
        );
        let n = self.revoke_user_sessions(name, Some(&keep), &ch)?;
        self.clear_owed(name);
        self.event(
            name,
            "passkey_remove",
            name,
            &format!(
                "removed \"{}\"; {n} other session(s) signed out",
                rec["name"].as_str().unwrap_or("")
            ),
        );
        Ok(())
    }

    fn set_passwordless(
        &self,
        p: &Principal,
        on: bool,
        reauth: &Reauth<'_>,
    ) -> Result<(), MfaError> {
        let name = person(p)?;
        let _g = self.mfa_guard()?;
        self.reauth(name, reauth)?;
        if on
            && !self
                .passkeys_of(name)
                .iter()
                .any(|(_, r)| r["user_verified"] == true)
        {
            return Err(MfaError::Invalid(
                "signing in with a passkey alone needs a passkey that verifies you (PIN, fingerprint or face); add one first".into(),
            ));
        }
        let mut patch = Fields::new();
        patch.insert("passkey_login".into(), J::Bool(on));
        self.put(name, &patch)?;
        self.event(
            name,
            "passkey_passwordless",
            name,
            if on {
                "sign-in with a passkey alone is on"
            } else {
                "sign-in with a passkey alone is off"
            },
        );
        Ok(())
    }

    fn proof_options(&self, p: &Principal, ctx: &WebauthnCtx) -> Result<J, MfaError> {
        let name = person(p)?;
        self.site(ctx)?;
        self.pk_limit(ctx, "options", OPTIONS_PER_IP)?;
        let creds = self.passkeys_of(name);
        if creds.is_empty() {
            return Err(MfaError::Invalid("you have no passkey".into()));
        }
        let challenge = self.new_challenge(ctx, "proof", Some(name), None)?;
        Ok(self.request_options(ctx, &challenge, &creds, "preferred"))
    }

    fn proof(&self, p: &Principal, ctx: &WebauthnCtx, credential: &J) -> Result<(), MfaError> {
        let name = person(p)?;
        self.site(ctx)?;
        self.pk_limit(ctx, "verify", VERIFY_PER_IP)?;
        if self.locked_out(name) {
            return Err(MfaError::Locked(
                "too many attempts, try again in a minute".into(),
            ));
        }
        let _g = self.mfa_guard()?;
        match self.verify_assertion(ctx, "proof", credential, false) {
            Ok((user, _)) if user == name => {
                let rec = json!({"expires_at": self.now() + PROOF_SECS});
                self.backend
                    .kv_put(&proof_key(name), &rec.to_string(), Some(PROOF_SECS * 1000))
                    .map_err(MfaError::Unavailable)?;
                Ok(())
            }
            Ok(_) => Err(denied("this passkey belongs to a different account")),
            Err(e) => {
                self.record_failure(name);
                self.failed_pk_event(name, "a passkey proof was refused");
                Err(e)
            }
        }
    }

    fn passkey_login_options(
        &self,
        ctx: &WebauthnCtx,
        mfa_token: Option<&str>,
    ) -> Result<J, MfaError> {
        self.site(ctx)?;
        self.pk_limit(ctx, "options", OPTIONS_PER_IP)?;
        match mfa_token {
            None => {
                let challenge = self.new_challenge(ctx, "login", None, None)?;
                // Usernameless: the authenticator offers its discoverable credentials.
                Ok(self.request_options(ctx, &challenge, &[], "required"))
            }
            Some(tok) => {
                let (name, _) = self.mfa_token_user(tok)?;
                let creds = self.passkeys_of(&name);
                if creds.is_empty() {
                    return Err(MfaError::Invalid("this account has no passkey".into()));
                }
                let challenge =
                    self.new_challenge(ctx, "second", Some(&name), Some(&token_hash(tok)))?;
                Ok(self.request_options(ctx, &challenge, &creds, "preferred"))
            }
        }
    }

    fn passkey_login(
        &self,
        ctx: &WebauthnCtx,
        mfa_token: Option<&str>,
        credential: &J,
    ) -> Result<LoginOutcome, MfaError> {
        self.site(ctx)?;
        self.pk_limit(ctx, "verify", VERIFY_PER_IP)?;
        match mfa_token {
            Some(tok) => self.login_second(ctx, tok, credential),
            None => self.login_passwordless(ctx, credential),
        }
    }

    fn methods(&self, mfa_token: &str) -> Vec<&'static str> {
        let Ok((name, _)) = self.mfa_token_user(mfa_token) else {
            return Vec::new();
        };
        let mut m = Vec::new();
        if self.mfa_enabled(&name) {
            m.push("totp");
        }
        if !self.passkeys_of(&name).is_empty() {
            m.push("passkey");
        }
        m
    }
}

impl Auth {
    /// The passkey `id` when it is the person's own.
    fn own_passkey(&self, name: &str, id: &str) -> Result<J, MfaError> {
        // A refused lookup is the store's trouble, not "no such passkey".
        self.backend
            .get(PASSKEY_TABLE, id)
            .map_err(MfaError::Unavailable)?
            .map(J::Object)
            .filter(|r| r["user"].as_str() == Some(name))
            .ok_or_else(|| MfaError::Invalid("no such passkey".into()))
    }
}
