//! The token endpoint and the grants behind it.
//!
//! A *grant* is one person's consent for one application: the groups handed over, the resource,
//! an end date, and the chain of refresh tokens. Every refresh token is single use. Presenting
//! one a second time means two parties hold it, so the whole grant is revoked (RFC 9700 4.14.2).
//! The same holds for a code that is presented twice.

use crate::client::Class;
use crate::flow::{is_challenge, is_verifier};
use crate::util::{clean_text, ct_eq, rand_hex, s256, sha_hex};
use crate::{OAuth, OAuthError, api_resource, mcp_resource, redirect};
use obop_commands::devices as dev;
use obop_storage::fault::{self, Fault};
use obop_storage::{ApplyError, Fields, Op};
use serde_json::{Value as J, json};

pub const GRANT_MODEL: &str = "oauth_grant";
pub const REFRESH_MODEL: &str = "oauth_refresh";

/// The store refused a call (full pool, unreachable): a client may retry, nothing was decided.
fn storage_busy() -> OAuthError {
    OAuthError::storage_busy()
}

/// Is this error text the store's refusal (a full pool, an unreachable database)?
fn is_busy(e: &str) -> bool {
    Fault::is_transient(e)
}

/// How often and how long a step that must not be lost (a revocation, the undoing of a token
/// that was just issued) is tried again when the store refuses it.
const TRIES: u32 = 4;
const PAUSE: std::time::Duration = std::time::Duration::from_millis(150);

fn fields(v: J) -> Fields {
    v.as_object().cloned().unwrap_or_default()
}

/// A parsed `application/x-www-form-urlencoded` body.
#[derive(Debug, Default, Clone)]
pub struct Form(pub Vec<(String, String)>);

impl Form {
    pub fn get(&self, k: &str) -> Option<&str> {
        self.0.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str())
    }

    /// A parameter must not appear more than once (RFC 6749 3.2).
    fn has_duplicates(&self) -> bool {
        self.0
            .iter()
            .enumerate()
            .any(|(i, (k, _))| self.0[..i].iter().any(|(o, _)| o == k && k != "resource"))
    }

    fn required(&self, k: &str) -> Result<&str, OAuthError> {
        self.get(k)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| OAuthError::new(400, "invalid_request", &format!("{k} is required")))
    }
}

fn invalid_grant(why: &str) -> OAuthError {
    OAuthError::new(400, "invalid_grant", why)
}

/// The resource a token of `class` is bound to, and the sentence for a request that names another.
fn resource_for(class: Class, base: &str) -> (String, &'static str) {
    match class {
        Class::Mcp => (
            mcp_resource(base),
            "resource must be the /mcp endpoint of this instance",
        ),
        Class::Mobile => (
            api_resource(base),
            "resource must be the /api endpoint of this instance",
        ),
    }
}

/// A `resource` parameter, when the request has one, must be the resource of the client's class.
fn check_resource(form: &Form, expected: &str, why: &'static str) -> Result<(), OAuthError> {
    match form.get("resource") {
        Some(r) if r.trim_end_matches('/') != expected => {
            Err(OAuthError::new(400, "invalid_target", why))
        }
        _ => Ok(()),
    }
}

/// What a mobile app says about the device in the token request (`device_name`,
/// `device_platform`, `device_app_version`), checked. An empty value is no value; anything else
/// that does not pass is refused, before the code is spent.
fn device_metadata(form: &Form) -> Result<dev::Metadata, OAuthError> {
    let bad = |what: &str| OAuthError::new(400, "invalid_request", what);
    let given = |k: &str| form.get(k).map(str::trim).filter(|v| !v.is_empty());
    Ok(dev::Metadata {
        name: given("device_name")
            .map(|v| {
                dev::name_of(v).ok_or_else(|| {
                    bad("device_name must be 1 to 80 characters without control characters")
                })
            })
            .transpose()?,
        platform: given("device_platform")
            .map(|v| {
                dev::platform_of(v).ok_or_else(|| {
                    bad("device_platform must be lower case letters, digits, . _ - and at most 24 characters, starting with a letter")
                })
            })
            .transpose()?,
        app_version: version_param(form)?,
    })
}

/// `device_app_version` of a token request, checked.
fn version_param(form: &Form) -> Result<Option<String>, OAuthError> {
    form.get("device_app_version")
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| {
            dev::version_of(v).ok_or_else(|| {
                OAuthError::new(
                    400,
                    "invalid_request",
                    "device_app_version must be letters, digits, . _ + - and at most 32 characters",
                )
            })
        })
        .transpose()
}

impl OAuth {
    /// `POST /oauth/token`: `authorization_code` or `refresh_token`.
    pub fn token(&self, base: &str, ip: &str, form: &Form) -> Result<J, OAuthError> {
        self.housekeeping();
        self.limit(
            &format!("oauth:rl:token:{}", &sha_hex(ip)[..24]),
            self.cfg.token_per_ip_minute,
            60_000,
        )?;
        if form.has_duplicates() {
            return Err(OAuthError::new(
                400,
                "invalid_request",
                "a parameter was repeated",
            ));
        }
        // A resource is the /mcp endpoint of an MCP client or the /api endpoint of a mobile app; which
        // one is right is known once the code or the refresh token has been found (below).
        if let Some(r) = form.get("resource")
            && r.trim_end_matches('/') != mcp_resource(base)
            && r.trim_end_matches('/') != api_resource(base)
        {
            return Err(OAuthError::new(
                400,
                "invalid_target",
                "resource must be the /mcp endpoint of this instance",
            ));
        }
        match form.get("grant_type") {
            Some("authorization_code") => self.exchange_code(base, form),
            Some("refresh_token") => self.exchange_refresh(base, form),
            Some(_) => Err(OAuthError::new(
                400,
                "unsupported_grant_type",
                "grant_type must be authorization_code or refresh_token",
            )),
            None => Err(OAuthError::new(
                400,
                "invalid_request",
                "grant_type is required",
            )),
        }
    }

    fn exchange_code(&self, base: &str, form: &Form) -> Result<J, OAuthError> {
        let code = form.required("code")?;
        let client_id = form.required("client_id")?;
        let verifier = form.required("code_verifier")?;
        let redirect_uri = form.required("redirect_uri")?;
        let key = format!("oauth:code:{}", sha_hex(code));
        let used_key = format!("oauth:used:{}", sha_hex(code));
        let storage = |_| storage_busy();
        // The one who deletes the code owns it; a second presentation finds nothing.
        let raw = self.backend.kv_get(&key).map_err(storage)?;
        // What the request must get right about the class of the client is checked before the code
        // is spent: a wrong resource, or a device the app described in a way that is refused, does
        // not cost the person their sign-in.
        let mut meta = dev::Metadata::default();
        if let Some(peek) = raw
            .as_deref()
            .and_then(|s| serde_json::from_str::<J>(s).ok())
        {
            let class = Class::of_record(&peek);
            let (expected, why) = resource_for(class, base);
            check_resource(form, &expected, why)?;
            if class.is_mobile() {
                meta = device_metadata(form)?;
            }
        }
        let consumed = raw.is_some() && self.backend.kv_delete(&key).map_err(storage)?;
        if !consumed {
            // A code that was already exchanged: whoever holds the tokens it produced may be
            // the thief, so end that grant.
            if let Some(g) = self.backend.kv_get(&used_key).map_err(storage)? {
                self.revoke_grant_internal(
                    &g,
                    "oauth_code_reuse",
                    "an authorization code was used twice",
                )?;
            }
            return Err(invalid_grant(
                "the code is invalid, expired or already used",
            ));
        }
        let raw = raw.unwrap_or_default();
        let rec: J =
            serde_json::from_str(&raw).map_err(|_| invalid_grant("the code is invalid"))?;
        // From here on the code is used up. When the store refuses a step and no token came of it,
        // the code is put back, so the client's retry (the 503 says to retry) is not met with
        // "invalid_grant", which is final and sends the person through the sign-in again.
        let give_back = |e: OAuthError| -> OAuthError {
            let ttl_ms = rec["exp"]
                .as_u64()
                .unwrap_or(0)
                .saturating_sub(self.now())
                .max(1)
                * 1000;
            let put = fault::patiently(TRIES, PAUSE, || {
                fault::bookkeeping(|| self.backend.kv_put(&key, &raw, Some(ttl_ms)))
            });
            if put.is_err() {
                eprintln!(
                    "oauth: an authorization code could not be put back, the client signs in again"
                );
            }
            e
        };
        if rec["exp"].as_u64().unwrap_or(0) <= self.now() {
            return Err(invalid_grant(
                "the code is invalid, expired or already used",
            ));
        }
        let class = Class::of_record(&rec);
        let (resource, _) = resource_for(class, base);
        let resource = resource.as_str();
        if rec["client_id"].as_str() != Some(client_id) {
            return Err(invalid_grant("the code was issued to another client"));
        }
        if rec["redirect_uri"].as_str() != Some(redirect_uri) {
            return Err(invalid_grant(
                "redirect_uri does not match the authorization request",
            ));
        }
        if rec["resource"].as_str() != Some(resource) {
            return Err(OAuthError::new(
                400,
                "invalid_target",
                "the code is for another resource",
            ));
        }
        if !is_verifier(verifier)
            || !is_challenge(rec["challenge"].as_str().unwrap_or(""))
            || !ct_eq(&s256(verifier), rec["challenge"].as_str().unwrap_or("-"))
        {
            return Err(invalid_grant("PKCE verification failed"));
        }
        if self.client(client_id).is_none() {
            // A lookup the store refused is not "unknown client" (that is final).
            if fault::noted().is_some() {
                return Err(give_back(storage_busy()));
            }
            return Err(OAuthError::new(401, "invalid_client", "unknown client"));
        }
        let owner = rec["owner"].as_str().unwrap_or("");
        let principal = match self.tokens().principal_of(owner) {
            Some(p) => p,
            None if fault::noted().is_some() => return Err(give_back(storage_busy())),
            None => return Err(invalid_grant("the account is not available")),
        };
        let scopes: Vec<String> = rec["scopes"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .filter(|g| principal.groups.contains(g))
                    .collect()
            })
            .unwrap_or_default();
        if scopes.is_empty() {
            return Err(invalid_grant(
                "the account no longer holds the granted groups",
            ));
        }
        let now = self.now();
        let days = rec["days"].as_u64().unwrap_or(1).clamp(1, 90);
        let grant_id = format!("og_{}", rand_hex::<12>());
        let client_name = rec["client_name"].as_str().unwrap_or("");
        let device_id = class.is_mobile().then(|| uuid::Uuid::now_v7().to_string());
        let label = format!(
            "{} ({})",
            clean_text(client_name, 60),
            if class.is_mobile() { "mobile" } else { "OAuth" }
        );
        let issued = match &device_id {
            Some(device) => self.tokens().issue_mobile_access(
                owner,
                &scopes,
                &label,
                self.cfg.access_ttl_secs,
                resource,
                &grant_id,
                device,
            ),
            None => self.tokens().issue_oauth_access(
                owner,
                &scopes,
                &label,
                self.cfg.access_ttl_secs,
                resource,
                &grant_id,
            ),
        };
        let (access, access_id) = issued.map_err(|e| {
            // The account lookup inside answers "unknown or disabled" for a refused read too:
            // the fault noted says which it was.
            if is_busy(&e) || fault::noted().is_some() {
                give_back(storage_busy())
            } else {
                invalid_grant(&e)
            }
        })?;
        let refresh = format!("obor_{}", rand_hex::<32>());
        let refresh_hash = sha_hex(&refresh);
        let expires = now + days * 86_400;
        let mut grant = fields(json!({
            "owner": owner,
            "client_id": client_id,
            "client_name": clean_text(client_name, 80),
            "redirect_host": redirect::display_host(redirect_uri),
            "scopes": scopes,
            "resource": resource,
            "created_at": now,
            "expires_at": expires,
            "last_used_at": now,
            "revoked": false,
            "access_id": access_id,
            "refresh_id": refresh_hash,
        }));
        let clean_name = clean_text(client_name, 80);
        let mut ops = Vec::new();
        // A mobile app's sign-in is a device, written in the same batch as the family it is bound to.
        let registered = device_id.as_ref().map(|device| {
            grant.insert("class".into(), J::from(class.as_str()));
            grant.insert("device".into(), J::from(device.as_str()));
            let record = dev::new_record(owner, &clean_name, &meta, &grant_id, now, expires);
            (device.clone(), record)
        });
        ops.push(Op::Insert {
            model: GRANT_MODEL.into(),
            id: grant_id.clone(),
            fields: grant,
        });
        ops.push(Op::Insert {
            model: REFRESH_MODEL.into(),
            id: refresh_hash,
            fields: fields(json!({
                "grant": grant_id,
                "client_id": client_id,
                "used": false,
                "expires_at": expires,
            })),
        });
        if let Some((device, record)) = &registered {
            ops.push(Op::Insert {
                model: dev::MODEL.into(),
                id: device.clone(),
                fields: record.clone(),
            });
        }
        if self.backend.apply_checked(&ops).is_err() {
            // The access token was issued for nothing: it is revoked again, although the request
            // already hit a fault (that is why), and it is tried again if the store refuses.
            self.unissue(&access_id);
            return Err(give_back(storage_busy()));
        }
        // The marker that lets a second use of the code end this grant. Losing it only weakens the
        // detection, so it is tried again but never fails the exchange, which has committed.
        let _ = fault::patiently(TRIES, PAUSE, || {
            fault::allow(|| self.backend.kv_put(&used_key, &grant_id, Some(600_000)))
        });
        if let Some((device, record)) = &registered {
            self.audit(owner, dev::EVENT_REGISTERED, &dev::describe(record, device));
        }
        Ok(token_response(
            &access,
            &refresh,
            self.cfg.access_ttl_secs,
            &scopes,
        ))
    }

    fn exchange_refresh(&self, base: &str, form: &Form) -> Result<J, OAuthError> {
        let presented = form.required("refresh_token")?;
        let client_id = form.required("client_id")?;
        let hash = sha_hex(presented);
        let gone = || invalid_grant("the refresh token is invalid, expired or revoked");
        // A lookup that was refused because the store is busy is not "no such token": the client
        // would drop a refresh token that is fine (invalid_grant is final), so it is told to try
        // again (D74).
        let found = |r: Result<Option<Fields>, String>| match r {
            Ok(Some(x)) => Ok(x),
            Ok(None) => Err(gone()),
            Err(e) if Fault::is_transient(&e) => Err(storage_busy()),
            Err(_) => Err(gone()),
        };
        let rec = found(self.backend.get(REFRESH_MODEL, &hash))?;
        let grant_id = rec
            .get("grant")
            .and_then(J::as_str)
            .unwrap_or("")
            .to_string();
        let grant = found(self.backend.get(GRANT_MODEL, &grant_id))?;
        if rec.get("client_id").and_then(J::as_str) != Some(client_id)
            || grant.get("client_id").and_then(J::as_str) != Some(client_id)
        {
            return Err(gone());
        }
        let class = Class::of_fields(&grant);
        let (resource, why_resource) = resource_for(class, base);
        let resource = resource.as_str();
        check_resource(form, resource, why_resource)?;
        // The app may say which version it is now; a value that is refused is refused at once.
        let app_version = if class.is_mobile() {
            version_param(form)?
        } else {
            None
        };
        let now = self.now();
        if grant.get("revoked").and_then(J::as_bool) == Some(true) {
            // A grant that was ended while the revocation of its access token was refused (or of
            // its device): this is where the client comes back, so the step is finished here (it
            // is safe to repeat).
            self.finish_ended(&grant, "the connection had ended")?;
            return Err(gone());
        }
        if grant.get("expires_at").and_then(J::as_u64).unwrap_or(0) <= now {
            return Err(gone());
        }
        // A used token is being presented again: two parties hold it.
        if rec.get("used").and_then(J::as_bool) == Some(true) {
            // If the grant cannot be ended now the client is told to try again, and the repeat
            // meets the same used token: it is never answered as if the grant had been ended.
            self.revoke_grant_internal(
                &grant_id,
                "oauth_refresh_reuse",
                "a refresh token was used twice, so the connection was ended",
            )?;
            return Err(gone());
        }
        let owner = grant
            .get("owner")
            .and_then(J::as_str)
            .unwrap_or("")
            .to_string();
        let principal = match self.tokens().principal_of(&owner) {
            Some(p) => p,
            // The lookup was refused, not answered: the account may be perfectly fine. Ending the
            // grant for good because the store was busy for a moment would disconnect the person.
            None if obop_storage::fault::noted().is_some() => return Err(storage_busy()),
            None => {
                self.revoke_grant_internal(
                    &grant_id,
                    "oauth_revoked",
                    "the account is not available",
                )?;
                return Err(gone());
            }
        };
        let granted: Vec<String> = grant
            .get("scopes")
            .and_then(J::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        // The owner may have lost groups since: never more than the owner holds today.
        let mut scopes: Vec<String> = granted
            .iter()
            .filter(|g| principal.groups.contains(*g))
            .cloned()
            .collect();
        if let Some(s) = form.get("scope") {
            let asked: Vec<&str> = s.split_whitespace().collect();
            if asked.iter().any(|g| !granted.iter().any(|x| x == g)) {
                return Err(OAuthError::new(
                    400,
                    "invalid_scope",
                    "a refresh cannot widen the grant",
                ));
            }
            scopes.retain(|g| asked.contains(&g.as_str()));
        }
        if scopes.is_empty() {
            return Err(invalid_grant(
                "the account no longer holds the granted groups",
            ));
        }
        // The device of a mobile app must still be in force; a family whose device was ended is
        // ended with it (a revocation that was refused half way finishes here).
        let device = if class.is_mobile() {
            let did = grant
                .get("device")
                .and_then(J::as_str)
                .unwrap_or("")
                .to_string();
            match self.backend.get(dev::MODEL, &did) {
                Ok(Some(d)) if dev::is_active(&d) => Some(did),
                Ok(_) => {
                    self.revoke_grant_internal(
                        &grant_id,
                        "oauth_revoked",
                        "the device was revoked",
                    )?;
                    return Err(gone());
                }
                // A lookup the store refused says nothing about the device: try again.
                Err(_) => return Err(storage_busy()),
            }
        } else {
            None
        };
        let client_name = grant.get("client_name").and_then(J::as_str).unwrap_or("");
        let label = format!(
            "{} ({})",
            clean_text(client_name, 60),
            if class.is_mobile() { "mobile" } else { "OAuth" }
        );
        let issued = match &device {
            Some(did) => self.tokens().issue_mobile_access(
                &owner,
                &scopes,
                &label,
                self.cfg.access_ttl_secs,
                resource,
                &grant_id,
                did,
            ),
            None => self.tokens().issue_oauth_access(
                &owner,
                &scopes,
                &label,
                self.cfg.access_ttl_secs,
                resource,
                &grant_id,
            ),
        };
        let (access, access_id) = issued.map_err(|e| {
            if is_busy(&e) || fault::noted().is_some() {
                storage_busy()
            } else {
                invalid_grant(&e)
            }
        })?;
        let new_refresh = format!("obor_{}", rand_hex::<32>());
        let new_hash = sha_hex(&new_refresh);
        let old_access = grant
            .get("access_id")
            .and_then(J::as_str)
            .unwrap_or("")
            .to_string();
        let mut ops = vec![
            Op::Patch {
                model: REFRESH_MODEL.into(),
                id: hash.clone(),
                patch: fields(json!({"used": true})),
                expect: vec![("used".into(), J::Bool(false))],
            },
            Op::Insert {
                model: REFRESH_MODEL.into(),
                id: new_hash.clone(),
                fields: fields(json!({
                    "grant": grant_id,
                    "client_id": client_id,
                    "used": false,
                    "expires_at": grant.get("expires_at").cloned().unwrap_or(J::Null),
                })),
            },
            Op::Patch {
                model: GRANT_MODEL.into(),
                id: grant_id.clone(),
                patch: fields(json!({
                    "access_id": access_id,
                    "refresh_id": new_hash,
                    "last_used_at": now,
                })),
                expect: vec![("revoked".into(), J::Bool(false))],
            },
        ];
        // The device is seen at a refresh, not at every request (that would be a write for each
        // call): the time of the last refresh is its `last_seen`, and the version the app
        // reports now is its `app_version`. The batch refuses if the device was ended meanwhile.
        if let Some(did) = &device {
            let mut seen = Fields::new();
            seen.insert("last_seen".into(), J::from(dev::instant(now)));
            if let Some(v) = &app_version {
                seen.insert("app_version".into(), J::from(v.as_str()));
            }
            ops.push(Op::Patch {
                model: dev::MODEL.into(),
                id: did.clone(),
                patch: seen,
                expect: vec![("state".into(), J::from(dev::ACTIVE))],
            });
        }
        match self.backend.apply_checked(&ops) {
            Ok(()) => {
                // The rotation has committed, and the client is given its new tokens whatever
                // happens next. The access token it replaces is revoked, tried again if the store
                // refuses; if that is lost the old token ends by itself with its lifetime.
                self.retire(&old_access);
                Ok(token_response(
                    &access,
                    &new_refresh,
                    self.cfg.access_ttl_secs,
                    &scopes,
                ))
            }
            Err(ApplyError::Conflict { index: 0, .. }) => {
                // Somebody used this token between our read and our write: the same case as
                // presenting it twice.
                self.unissue(&access_id);
                self.revoke_grant_internal(
                    &grant_id,
                    "oauth_refresh_reuse",
                    "a refresh token was used twice, so the connection was ended",
                )?;
                Err(gone())
            }
            Err(ApplyError::Conflict { .. }) => {
                self.unissue(&access_id);
                Err(gone())
            }
            Err(ApplyError::Storage(_)) => {
                // The rotation was refused: the access token issued for it is an orphan, revoked
                // again (the guard lets this through although the request hit a fault).
                self.unissue(&access_id);
                Err(storage_busy())
            }
        }
    }

    /// Undo an access token that was issued in this request and will never be handed out (the
    /// grant write that was to go with it was refused). A write that undoes what the request did,
    /// so the guard lets it through after a fault, and it is tried again while the store refuses;
    /// an orphan that stays is an access token nobody holds, and ends with its lifetime.
    fn unissue(&self, access_id: &str) {
        let r = fault::patiently(TRIES, PAUSE, || {
            fault::bookkeeping(|| self.tokens().try_revoke_token_id(access_id))
        });
        if let Err(e) = r {
            eprintln!(
                "oauth: an access token that was issued for a failed request could not be revoked ({e}); it ends with its lifetime"
            );
        }
    }

    /// Revoke the access token a rotation replaced.
    fn retire(&self, access_id: &str) {
        let r = fault::patiently(TRIES, PAUSE, || {
            fault::allow(|| self.tokens().try_revoke_token_id(access_id))
        });
        if let Err(e) = r {
            eprintln!(
                "oauth: the access token replaced by a refresh could not be revoked ({e}); it ends with its lifetime"
            );
        }
    }

    /// `POST /oauth/revoke` (RFC 7009): a refresh or access token the client holds. The answer is
    /// the same whether or not anything was revoked.
    pub fn revoke_presented(&self, ip: &str, form: &Form) -> Result<(), OAuthError> {
        self.limit(
            &format!("oauth:rl:token:{}", &sha_hex(ip)[..24]),
            self.cfg.token_per_ip_minute,
            60_000,
        )?;
        let token = form.required("token")?;
        let client_id = form.required("client_id")?;
        // The answer is the same whether or not anything was revoked, but a store that refuses is
        // not "nothing to revoke": the client is told to ask again (RFC 7009 allows a 503).
        let busy = |e: String| {
            if is_busy(&e) {
                storage_busy()
            } else {
                invalid_grant("the token could not be looked up")
            }
        };
        let found = self
            .backend
            .get(REFRESH_MODEL, &sha_hex(token))
            .map_err(busy)?
            .and_then(|r| r.get("grant").and_then(J::as_str).map(String::from));
        let grant_id = match found {
            Some(g) => Some(g),
            None => {
                let g = self.tokens().grant_of(token);
                // `grant_of` cannot say "refused": it answers "none". The fault noted tells.
                if g.is_none() && fault::noted().is_some() {
                    return Err(storage_busy());
                }
                g
            }
        };
        if let Some(g) = grant_id
            && let Some(grant) = self.backend.get(GRANT_MODEL, &g).map_err(busy)?
            && grant.get("client_id").and_then(J::as_str) == Some(client_id)
        {
            self.revoke_grant_internal(
                &g,
                "oauth_revoked",
                "the application revoked its own access",
            )?;
        }
        Ok(())
    }

    /// End a grant: no new tokens, and the access token in circulation stops working.
    ///
    /// A revocation is a security decision, so a step the store refuses is tried again, and if it
    /// still cannot be made the caller gets a 503, never an answer that says it was done. What was
    /// done stays done, and doing the rest again is safe: the grant is marked once, the access token
    /// it names is revoked every time.
    fn revoke_grant_internal(
        &self,
        grant_id: &str,
        kind: &str,
        why: &str,
    ) -> Result<bool, OAuthError> {
        let grant = match fault::patiently(TRIES, PAUSE, || self.backend.get(GRANT_MODEL, grant_id))
        {
            Ok(Some(g)) => g,
            Ok(None) => return Ok(false),
            Err(_) => return Err(storage_busy()),
        };
        let already = grant.get("revoked").and_then(J::as_bool) == Some(true);
        if !already {
            let patch = fields(json!({"revoked": true, "revoked_at": self.now()}));
            fault::patiently(TRIES, PAUSE, || {
                fault::allow(|| self.backend.update(GRANT_MODEL, grant_id, &patch))
            })
            .map_err(|_| storage_busy())?;
        }
        // The access token may have been replaced by a refresh while we looked: revoke what the
        // record names now.
        let current = fault::patiently(TRIES, PAUSE, || self.backend.get(GRANT_MODEL, grant_id))
            .ok()
            .flatten()
            .unwrap_or(grant);
        let owner = current.get("owner").and_then(J::as_str).unwrap_or("");
        if !already {
            self.audit_grant(owner, kind, &current, why);
        }
        self.finish_ended(&current, why)?;
        Ok(true)
    }

    /// The rest of ending a grant, each step safe to repeat: the device of a mobile app is marked
    /// revoked (which stops its access token at the next request whatever happens to the token's
    /// own record) and the access token the grant names is revoked. Both are tried even when the
    /// first is refused: the token stops if either of them goes through, and the caller is told
    /// (503) if one did not, so that asking again finishes it.
    fn finish_ended(&self, grant: &Fields, why: &str) -> Result<(), OAuthError> {
        let device = self.end_device_of(grant, why);
        let access = self.revoke_access_of(grant);
        device.and(access)
    }

    /// Mark the device a grant is bound to as revoked, if it is in force, and say so in the
    /// security log. A step the store refuses is tried again and then reported (503).
    fn end_device_of(&self, grant: &Fields, why: &str) -> Result<(), OAuthError> {
        let Some(did) = grant.get("device").and_then(J::as_str) else {
            return Ok(());
        };
        let rec = fault::patiently(TRIES, PAUSE, || self.backend.get(dev::MODEL, did))
            .map_err(|_| storage_busy())?;
        let Some(rec) = rec.filter(dev::is_active) else {
            return Ok(());
        };
        fault::patiently(TRIES, PAUSE, || {
            fault::allow(|| {
                self.backend
                    .update(dev::MODEL, did, &dev::revoked_patch(self.now()))
            })
        })
        .map_err(|_| storage_busy())?;
        let owner = grant.get("owner").and_then(J::as_str).unwrap_or("");
        self.audit(
            owner,
            dev::EVENT_REVOKED,
            &format!("{}: {why}", dev::describe(&rec, did)),
        );
        Ok(())
    }

    /// End the device `id`: its refresh-token family and its access token, and the record. Safe to
    /// repeat; a step the store refuses is tried again and then reported (503), never skipped.
    pub fn revoke_device(&self, id: &str, actor: &str) -> Result<(), OAuthError> {
        let rec = fault::patiently(TRIES, PAUSE, || self.backend.get(dev::MODEL, id))
            .map_err(|_| storage_busy())?;
        let Some(rec) = rec else {
            return Err(OAuthError::new(404, "not_found", "no such device"));
        };
        let why = format!("revoked by {actor}");
        let grant = rec.get("grant").and_then(J::as_str).unwrap_or("");
        if self.revoke_grant_internal(grant, "oauth_revoked", &why)? {
            return Ok(());
        }
        // The family is gone (forgotten after it ended): only the record is left to mark.
        let mut stub = Fields::new();
        stub.insert("device".into(), J::from(id));
        stub.insert("owner".into(), rec.get("user").cloned().unwrap_or(J::Null));
        self.end_device_of(&stub, &why)
    }

    /// Revoke the access token a grant names. Safe to repeat; a step the store refuses is tried
    /// again and then reported (503), never skipped.
    fn revoke_access_of(&self, grant: &Fields) -> Result<(), OAuthError> {
        if let Some(a) = grant.get("access_id").and_then(J::as_str) {
            fault::patiently(TRIES, PAUSE, || {
                fault::allow(|| self.tokens().try_revoke_token_id(a))
            })
            .map_err(|_| storage_busy())?;
        }
        Ok(())
    }

    fn audit_grant(&self, owner: &str, kind: &str, current: &Fields, why: &str) {
        self.audit(
            owner,
            kind,
            &format!(
                "{} ({}): {}",
                current.get("client_name").and_then(J::as_str).unwrap_or(""),
                current.get("client_id").and_then(J::as_str).unwrap_or(""),
                why
            ),
        );
    }

    /// The person's connected applications that are still in force, newest first.
    pub fn grants_of(&self, owner: &str) -> Vec<J> {
        let now = self.now();
        let mut rows: Vec<(u64, J)> = self
            .backend
            .find(GRANT_MODEL, "owner", &J::from(owner))
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, g)| {
                g.get("revoked").and_then(J::as_bool) != Some(true)
                    && g.get("expires_at").and_then(J::as_u64).unwrap_or(0) > now
                    // A mobile app is listed as a device, not as a connected application.
                    && !Class::of_fields(g).is_mobile()
            })
            .map(|(id, g)| {
                let created = g.get("created_at").and_then(J::as_u64).unwrap_or(0);
                (
                    created,
                    json!({
                        "id": id,
                        "client_id": g.get("client_id"),
                        "client_name": g.get("client_name"),
                        "redirect_host": g.get("redirect_host"),
                        "scopes": g.get("scopes"),
                        "created_at": obop_commands::audit::rfc3339(created),
                        "expires_at": g.get("expires_at").and_then(J::as_u64).map(obop_commands::audit::rfc3339),
                        "last_used_at": g.get("last_used_at").and_then(J::as_u64).map(obop_commands::audit::rfc3339),
                    }),
                )
            })
            .collect();
        rows.sort_by_key(|a| std::cmp::Reverse(a.0));
        rows.into_iter().map(|(_, j)| j).collect()
    }

    /// The person revokes one of their connected applications. `false` when there is none by
    /// that id (or it belongs to somebody else: the answer is the same). A store that refuses is
    /// an error (503), never "revoked" and never "no such grant".
    pub fn revoke_grant(&self, owner: &str, grant_id: &str) -> Result<bool, OAuthError> {
        let mine = self
            .backend
            .get(GRANT_MODEL, grant_id)
            .map_err(|_| storage_busy())?
            .is_some_and(|g| g.get("owner").and_then(J::as_str) == Some(owner));
        if !mine {
            return Ok(false);
        }
        self.revoke_grant_internal(grant_id, "oauth_revoked", "revoked by the owner")
    }

    /// Forget grants that ended a week ago and their refresh tokens.
    pub(crate) fn purge_grants(&self) {
        let cutoff = self.now().saturating_sub(7 * 86_400);
        let mut dead = Vec::new();
        let mut devices = Vec::new();
        let _ = self.backend.scan(GRANT_MODEL, &mut |id, g| {
            let ended = if g.get("revoked").and_then(J::as_bool) == Some(true) {
                g.get("revoked_at").and_then(J::as_u64).unwrap_or(0)
            } else {
                g.get("expires_at").and_then(J::as_u64).unwrap_or(0)
            };
            if ended <= cutoff {
                dead.push(id.to_string());
                if let Some(d) = g.get("device").and_then(J::as_str) {
                    devices.push(d.to_string());
                }
            }
            Ok(true)
        });
        // A device is forgotten with the family it belonged to, a week after it ended.
        for d in devices {
            let _ = self.backend.delete(dev::MODEL, &d);
        }
        for g in dead {
            for (rid, _) in self
                .backend
                .find(REFRESH_MODEL, "grant", &J::from(g.as_str()))
                .unwrap_or_default()
            {
                let _ = self.backend.delete(REFRESH_MODEL, &rid);
            }
            let _ = self.backend.delete(GRANT_MODEL, &g);
        }
    }
}

fn token_response(access: &str, refresh: &str, ttl: u64, scopes: &[String]) -> J {
    json!({
        "access_token": access,
        "token_type": "Bearer",
        "expires_in": ttl,
        "refresh_token": refresh,
        "scope": scopes.join(" "),
    })
}
