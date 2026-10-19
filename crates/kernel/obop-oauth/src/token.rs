//! The token endpoint and the grants behind it.
//!
//! A *grant* is one person's consent for one application: the groups handed over, the resource,
//! an end date, and the chain of refresh tokens. Every refresh token is single use. Presenting
//! one a second time means two parties hold it, so the whole grant is revoked (RFC 9700 4.14.2).
//! The same holds for a code that is presented twice.

use crate::flow::{is_challenge, is_verifier};
use crate::util::{clean_text, ct_eq, rand_hex, s256, sha_hex};
use crate::{OAuth, OAuthError, mcp_resource, redirect};
use obop_storage::{ApplyError, Fields, Op};
use serde_json::{Value as J, json};

pub const GRANT_MODEL: &str = "oauth_grant";
pub const REFRESH_MODEL: &str = "oauth_refresh";

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
        let resource = mcp_resource(base);
        if let Some(r) = form.get("resource")
            && r.trim_end_matches('/') != resource
        {
            return Err(OAuthError::new(
                400,
                "invalid_target",
                "resource must be the /mcp endpoint of this instance",
            ));
        }
        match form.get("grant_type") {
            Some("authorization_code") => self.exchange_code(&resource, form),
            Some("refresh_token") => self.exchange_refresh(&resource, form),
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

    fn exchange_code(&self, resource: &str, form: &Form) -> Result<J, OAuthError> {
        let code = form.required("code")?;
        let client_id = form.required("client_id")?;
        let verifier = form.required("code_verifier")?;
        let redirect_uri = form.required("redirect_uri")?;
        let key = format!("oauth:code:{}", sha_hex(code));
        let used_key = format!("oauth:used:{}", sha_hex(code));
        let storage = |_| OAuthError::new(503, "temporarily_unavailable", "storage is busy");
        // The one who deletes the code owns it; a second presentation finds nothing.
        let raw = self.backend.kv_get(&key).map_err(storage)?;
        let consumed = raw.is_some() && self.backend.kv_delete(&key).map_err(storage)?;
        if !consumed {
            // A code that was already exchanged: whoever holds the tokens it produced may be
            // the thief, so end that grant.
            if let Some(g) = self.backend.kv_get(&used_key).map_err(storage)? {
                self.revoke_grant_internal(
                    &g,
                    "oauth_code_reuse",
                    "an authorization code was used twice",
                );
            }
            return Err(invalid_grant(
                "the code is invalid, expired or already used",
            ));
        }
        let rec: J = serde_json::from_str(&raw.unwrap_or_default())
            .map_err(|_| invalid_grant("the code is invalid"))?;
        if rec["exp"].as_u64().unwrap_or(0) <= self.now() {
            return Err(invalid_grant(
                "the code is invalid, expired or already used",
            ));
        }
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
            return Err(OAuthError::new(401, "invalid_client", "unknown client"));
        }
        let owner = rec["owner"].as_str().unwrap_or("");
        let principal = self
            .tokens()
            .principal_of(owner)
            .ok_or_else(|| invalid_grant("the account is not available"))?;
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
        let label = format!("{} (OAuth)", clean_text(client_name, 60));
        let (access, access_id) = self
            .tokens()
            .issue_oauth_access(
                owner,
                &scopes,
                &label,
                self.cfg.access_ttl_secs,
                resource,
                &grant_id,
            )
            .map_err(|e| invalid_grant(&e))?;
        let refresh = format!("obor_{}", rand_hex::<32>());
        let refresh_hash = sha_hex(&refresh);
        let expires = now + days * 86_400;
        let ops = vec![
            Op::Insert {
                model: GRANT_MODEL.into(),
                id: grant_id.clone(),
                fields: fields(json!({
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
                })),
            },
            Op::Insert {
                model: REFRESH_MODEL.into(),
                id: refresh_hash,
                fields: fields(json!({
                    "grant": grant_id,
                    "client_id": client_id,
                    "used": false,
                    "expires_at": expires,
                })),
            },
        ];
        if self.backend.apply_checked(&ops).is_err() {
            self.tokens().revoke_token_id(&access_id);
            return Err(OAuthError::new(
                503,
                "temporarily_unavailable",
                "storage is busy",
            ));
        }
        let _ = self.backend.kv_put(&used_key, &grant_id, Some(600_000));
        Ok(token_response(
            &access,
            &refresh,
            self.cfg.access_ttl_secs,
            &scopes,
        ))
    }

    fn exchange_refresh(&self, resource: &str, form: &Form) -> Result<J, OAuthError> {
        let presented = form.required("refresh_token")?;
        let client_id = form.required("client_id")?;
        let hash = sha_hex(presented);
        let gone = || invalid_grant("the refresh token is invalid, expired or revoked");
        let rec = self
            .backend
            .get(REFRESH_MODEL, &hash)
            .ok()
            .flatten()
            .ok_or_else(gone)?;
        let grant_id = rec
            .get("grant")
            .and_then(J::as_str)
            .unwrap_or("")
            .to_string();
        let grant = self
            .backend
            .get(GRANT_MODEL, &grant_id)
            .ok()
            .flatten()
            .ok_or_else(gone)?;
        if rec.get("client_id").and_then(J::as_str) != Some(client_id)
            || grant.get("client_id").and_then(J::as_str) != Some(client_id)
        {
            return Err(gone());
        }
        let now = self.now();
        if grant.get("revoked").and_then(J::as_bool) == Some(true)
            || grant.get("expires_at").and_then(J::as_u64).unwrap_or(0) <= now
        {
            return Err(gone());
        }
        // A used token is being presented again: two parties hold it.
        if rec.get("used").and_then(J::as_bool) == Some(true) {
            self.revoke_grant_internal(
                &grant_id,
                "oauth_refresh_reuse",
                "a refresh token was used twice, so the connection was ended",
            );
            return Err(gone());
        }
        let owner = grant
            .get("owner")
            .and_then(J::as_str)
            .unwrap_or("")
            .to_string();
        let principal = match self.tokens().principal_of(&owner) {
            Some(p) => p,
            None => {
                self.revoke_grant_internal(
                    &grant_id,
                    "oauth_revoked",
                    "the account is not available",
                );
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
        let client_name = grant.get("client_name").and_then(J::as_str).unwrap_or("");
        let label = format!("{} (OAuth)", clean_text(client_name, 60));
        let (access, access_id) = self
            .tokens()
            .issue_oauth_access(
                &owner,
                &scopes,
                &label,
                self.cfg.access_ttl_secs,
                resource,
                &grant_id,
            )
            .map_err(|e| invalid_grant(&e))?;
        let new_refresh = format!("obor_{}", rand_hex::<32>());
        let new_hash = sha_hex(&new_refresh);
        let old_access = grant
            .get("access_id")
            .and_then(J::as_str)
            .unwrap_or("")
            .to_string();
        let ops = vec![
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
        match self.backend.apply_checked(&ops) {
            Ok(()) => {
                self.tokens().revoke_token_id(&old_access);
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
                self.tokens().revoke_token_id(&access_id);
                self.revoke_grant_internal(
                    &grant_id,
                    "oauth_refresh_reuse",
                    "a refresh token was used twice, so the connection was ended",
                );
                Err(gone())
            }
            Err(ApplyError::Conflict { .. }) => {
                self.tokens().revoke_token_id(&access_id);
                Err(gone())
            }
            Err(ApplyError::Storage(_)) => {
                self.tokens().revoke_token_id(&access_id);
                Err(OAuthError::new(
                    503,
                    "temporarily_unavailable",
                    "storage is busy",
                ))
            }
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
        let grant_id = self
            .backend
            .get(REFRESH_MODEL, &sha_hex(token))
            .ok()
            .flatten()
            .and_then(|r| r.get("grant").and_then(J::as_str).map(String::from))
            .or_else(|| self.tokens().grant_of(token));
        if let Some(g) = grant_id
            && let Some(grant) = self.backend.get(GRANT_MODEL, &g).ok().flatten()
            && grant.get("client_id").and_then(J::as_str) == Some(client_id)
        {
            self.revoke_grant_internal(
                &g,
                "oauth_revoked",
                "the application revoked its own access",
            );
        }
        Ok(())
    }

    /// End a grant: no new tokens, and the access token in circulation stops working.
    fn revoke_grant_internal(&self, grant_id: &str, kind: &str, why: &str) -> bool {
        let Some(grant) = self.backend.get(GRANT_MODEL, grant_id).ok().flatten() else {
            return false;
        };
        if grant.get("revoked").and_then(J::as_bool) == Some(true) {
            return true;
        }
        let _ = self.backend.update(
            GRANT_MODEL,
            grant_id,
            &fields(json!({"revoked": true, "revoked_at": self.now()})),
        );
        // The access token may have been replaced by a refresh while we looked: revoke what the
        // record names now.
        let current = self
            .backend
            .get(GRANT_MODEL, grant_id)
            .ok()
            .flatten()
            .unwrap_or(grant);
        if let Some(a) = current.get("access_id").and_then(J::as_str) {
            self.tokens().revoke_token_id(a);
        }
        let owner = current.get("owner").and_then(J::as_str).unwrap_or("");
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
        true
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
    /// that id (or it belongs to somebody else: the answer is the same).
    pub fn revoke_grant(&self, owner: &str, grant_id: &str) -> bool {
        let mine = self
            .backend
            .get(GRANT_MODEL, grant_id)
            .ok()
            .flatten()
            .is_some_and(|g| g.get("owner").and_then(J::as_str) == Some(owner));
        mine && self.revoke_grant_internal(grant_id, "oauth_revoked", "revoked by the owner")
    }

    /// Forget grants that ended a week ago and their refresh tokens.
    pub(crate) fn purge_grants(&self) {
        let cutoff = self.now().saturating_sub(7 * 86_400);
        let mut dead = Vec::new();
        let _ = self.backend.scan(GRANT_MODEL, &mut |id, g| {
            let ended = if g.get("revoked").and_then(J::as_bool) == Some(true) {
                g.get("revoked_at").and_then(J::as_u64).unwrap_or(0)
            } else {
                g.get("expires_at").and_then(J::as_u64).unwrap_or(0)
            };
            if ended <= cutoff {
                dead.push(id.to_string());
            }
            Ok(true)
        });
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
