//! The authorization endpoint: validating a request, and the browser flow that follows it
//! (sign in, a second factor when the account has one, consent). A flow is one short-lived
//! record in the shared storage, found by a random id and bound to the browser that started it by
//! a second random value held in a cookie (only its hash is stored). Every step after the first
//! must present both and the CSRF token of the flow, so a page on another site can neither
//! continue nor complete somebody's consent.

use crate::client::{Class, Client};
use crate::util::{clean_text, ct_eq, rand_hex, sha_hex};
use crate::{OAuth, OAuthError, api_resource, mcp_resource, redirect};
use obop_commands::LoginOutcome;
use serde_json::{Value as J, json};

/// The parameters of `GET /oauth/authorize`.
#[derive(Debug, Default, Clone)]
pub struct AuthorizeParams {
    pub response_type: Option<String>,
    pub client_id: Option<String>,
    pub redirect_uri: Option<String>,
    pub scope: Option<String>,
    pub state: Option<String>,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    pub resources: Vec<String>,
    pub lang: Option<String>,
}

/// A request that passed every check.
#[derive(Debug, Clone)]
pub struct Validated {
    pub client: Client,
    pub redirect_uri: String,
    pub state: Option<String>,
    pub challenge: String,
    pub resource: String,
    pub hint: Vec<String>,
}

/// Why an authorization request was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthzError {
    /// The client or the redirect URI cannot be trusted: say so on a page, never redirect.
    Page { key: &'static str },
    /// Everything up to the redirect URI is fine: send the error there (RFC 6749 4.1.2.1).
    Redirect {
        redirect_uri: String,
        state: Option<String>,
        error: &'static str,
        description: &'static str,
    },
}

pub fn is_challenge(s: &str) -> bool {
    s.len() == 43
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub fn is_verifier(s: &str) -> bool {
    (43..=128).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~'))
}

/// A group that runs the instance or an area of it. It is ticked last on the consent page of an
/// MCP client, and a mobile app is never handed one.
pub(crate) fn is_admin_like(g: &str) -> bool {
    g.starts_with("platform.") || g.ends_with(".admin")
}

/// What the page needs to show a step.
#[derive(Debug, Clone)]
pub struct FlowCtx {
    pub id: String,
    pub csrf: String,
    pub lang: String,
    pub client_name: String,
    pub redirect_origin: String,
    pub redirect_host: String,
    pub user: Option<String>,
    /// What kind of application asks (a mobile app is told so, and is offered no administrative
    /// group).
    pub class: Class,
    /// The groups the signed-in person holds: the most they can hand over.
    pub groups: Vec<String>,
    /// The ones to tick at first.
    pub preselected: Vec<String>,
}

/// Why a step of the flow was not accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowError {
    /// Unknown, expired or finished: start again from the app.
    Gone,
    /// The browser binding or the CSRF token did not match.
    Forbidden,
    /// Too many attempts; seconds to wait.
    Busy(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginOut {
    /// Signed in: show the consent step.
    Consent,
    /// A second factor is needed; the token goes into the next form.
    Mfa(String),
    /// The code was wrong: ask again with the same token.
    MfaRetry(String),
    /// Refused, for this reason (a key of the page catalogue).
    Failed(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecideOut {
    /// Send the browser here.
    Redirect(String),
    /// Show the consent step again with this message.
    Retry(&'static str),
}

pub const EXPIRY_DAYS: [u32; 4] = [1, 7, 30, 90];
pub const DEFAULT_DAYS: u32 = 30;
const MAX_LOGIN_ATTEMPTS: u64 = 8;

fn flow_key(id: &str) -> String {
    format!("oauth:flow:{id}")
}

impl OAuth {
    /// Check an authorization request against the registered client, the PKCE rules and this
    /// instance's resource identifier.
    pub fn validate_authorize(
        &self,
        base: &str,
        p: &AuthorizeParams,
    ) -> Result<Validated, AuthzError> {
        let page = |key| AuthzError::Page { key };
        let client = p
            .client_id
            .as_deref()
            .and_then(|id| self.client(id))
            .ok_or(page("err.client"))?;
        let matches = |u: &str| match client.class {
            Class::Mcp => redirect::matches(&client.redirect_uris, u),
            Class::Mobile => redirect::matches_mobile(&client.redirect_uris, u),
        };
        let redirect_uri = match p.redirect_uri.as_deref() {
            Some(u) if matches(u) => u.to_string(),
            Some(_) => return Err(page("err.redirect")),
            None if client.redirect_uris.len() == 1 => client.redirect_uris[0].clone(),
            None => return Err(page("err.redirect")),
        };
        let state = p
            .state
            .as_deref()
            .map(|s| s.chars().take(512).collect::<String>());
        let fail = |error: &'static str, description: &'static str| AuthzError::Redirect {
            redirect_uri: redirect_uri.clone(),
            state: state.clone(),
            error,
            description,
        };
        if p.response_type.as_deref() != Some("code") {
            return Err(fail(
                "unsupported_response_type",
                "only response_type=code is supported",
            ));
        }
        let challenge = match p.code_challenge.as_deref() {
            Some(c) if is_challenge(c) => c.to_string(),
            _ => {
                return Err(fail(
                    "invalid_request",
                    "code_challenge is required (PKCE with S256)",
                ));
            }
        };
        if p.code_challenge_method.as_deref() != Some("S256") {
            return Err(fail(
                "invalid_request",
                "code_challenge_method must be S256",
            ));
        }
        let (resource, wrong_resource) = match client.class {
            Class::Mcp => (
                mcp_resource(base),
                "resource must be the /mcp endpoint of this instance",
            ),
            Class::Mobile => (
                api_resource(base),
                "resource must be the /api endpoint of this instance",
            ),
        };
        let ok_resource = |r: &str| r.trim_end_matches('/') == resource;
        if p.resources.iter().any(|r| !ok_resource(r)) {
            return Err(fail("invalid_target", wrong_resource));
        }
        let hint = p
            .scope
            .as_deref()
            .unwrap_or("")
            .split_whitespace()
            .take(64)
            .map(|s| s.chars().take(100).collect())
            .collect();
        Ok(Validated {
            client,
            redirect_uri,
            state,
            challenge,
            resource,
            hint,
        })
    }

    /// The redirect that carries a refusal back to the application.
    pub fn error_redirect(
        &self,
        base: &str,
        redirect_uri: &str,
        state: Option<&str>,
        error: &str,
        description: &str,
    ) -> String {
        let mut params = vec![("error", error), ("error_description", description)];
        if let Some(s) = state {
            params.push(("state", s));
        }
        params.push(("iss", base));
        redirect::with_params(redirect_uri, &params)
    }

    /// Start the browser flow for a valid request. Returns the flow id and the secret that goes
    /// into the cookie.
    pub fn begin(
        &self,
        v: &Validated,
        ip: &str,
        lang: &str,
    ) -> Result<(String, String), OAuthError> {
        self.housekeeping();
        self.limit(
            &format!("oauth:rl:authz:{}", &sha_hex(ip)[..24]),
            self.cfg.authorize_per_ip,
            600_000,
        )?;
        let id = rand_hex::<16>();
        let binder = rand_hex::<32>();
        let rec = json!({
            "client_id": v.client.id,
            "client_name": v.client.name,
            "class": v.client.class.as_str(),
            "redirect_uri": v.redirect_uri,
            "state": v.state,
            "challenge": v.challenge,
            "resource": v.resource,
            "hint": v.hint,
            "csrf": rand_hex::<16>(),
            "binder": sha_hex(&binder),
            "user": J::Null,
            "attempts": 0,
            "lang": lang,
        });
        self.flow_put(&id, &rec)
            .map_err(|_| OAuthError::storage_busy())?;
        Ok((id, binder))
    }

    fn flow_put(&self, id: &str, rec: &J) -> Result<(), String> {
        self.backend.kv_put(
            &flow_key(id),
            &rec.to_string(),
            Some(self.cfg.flow_ttl_secs * 1000),
        )
    }

    /// The flow `id` for the browser holding `binder`; with `csrf`, also that token.
    fn load(&self, id: &str, binder: &str, csrf: Option<&str>) -> Result<J, FlowError> {
        if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(FlowError::Gone);
        }
        let raw = self
            .backend
            .kv_get(&flow_key(id))
            .ok()
            .flatten()
            .ok_or(FlowError::Gone)?;
        let rec: J = serde_json::from_str(&raw).map_err(|_| FlowError::Gone)?;
        if !ct_eq(&sha_hex(binder), rec["binder"].as_str().unwrap_or("-")) {
            return Err(FlowError::Forbidden);
        }
        if let Some(c) = csrf
            && !ct_eq(c, rec["csrf"].as_str().unwrap_or("-"))
        {
            return Err(FlowError::Forbidden);
        }
        Ok(rec)
    }

    fn ctx(&self, id: &str, rec: &J) -> FlowCtx {
        let user = rec["user"].as_str().map(String::from);
        let class = Class::of_record(rec);
        let mut groups: Vec<String> = user
            .as_deref()
            .and_then(|u| self.tokens().principal_of(u))
            .map(|p| p.groups.into_iter().collect())
            .unwrap_or_default();
        // A phone is lost, so a mobile app is never offered a group that runs the instance.
        if class.is_mobile() {
            groups.retain(|g| !is_admin_like(g));
        }
        groups.sort();
        let hint: Vec<&str> = rec["hint"]
            .as_array()
            .map(|a| a.iter().filter_map(J::as_str).collect())
            .unwrap_or_default();
        let requested: Vec<String> = groups
            .iter()
            .filter(|g| hint.contains(&g.as_str()))
            .cloned()
            .collect();
        let preselected = if requested.is_empty() {
            groups
                .iter()
                .filter(|g| !is_admin_like(g))
                .cloned()
                .collect()
        } else {
            requested
        };
        let uri = rec["redirect_uri"].as_str().unwrap_or("");
        FlowCtx {
            id: id.to_string(),
            csrf: rec["csrf"].as_str().unwrap_or("").to_string(),
            lang: rec["lang"].as_str().unwrap_or("en").to_string(),
            client_name: rec["client_name"].as_str().unwrap_or("").to_string(),
            redirect_origin: redirect::origin(uri),
            redirect_host: redirect::display_host(uri),
            user,
            class,
            groups,
            preselected,
        }
    }

    /// The flow as the page shows it (no CSRF check: this is a read).
    pub fn view(&self, id: &str, binder: &str) -> Result<FlowCtx, FlowError> {
        let rec = self.load(id, binder, None)?;
        Ok(self.ctx(id, &rec))
    }

    fn count_attempt(&self, id: &str, rec: &mut J) -> Result<(), FlowError> {
        let n = rec["attempts"].as_u64().unwrap_or(0) + 1;
        if n >= MAX_LOGIN_ATTEMPTS {
            let _ = self.backend.kv_delete(&flow_key(id));
            return Err(FlowError::Gone);
        }
        rec["attempts"] = J::from(n);
        Ok(())
    }

    /// First sign-in step. The session the sign-in creates is revoked at once: all the flow needs
    /// is to know who the person is.
    pub fn login(
        &self,
        id: &str,
        binder: &str,
        csrf: &str,
        ip: &str,
        username: &str,
        password: &str,
    ) -> Result<LoginOut, FlowError> {
        let mut rec = self.load(id, binder, Some(csrf))?;
        self.limit(
            &format!("oauth:rl:login:{}", &sha_hex(ip)[..24]),
            self.cfg.login_per_ip,
            600_000,
        )
        .map_err(|e| FlowError::Busy(e.retry_after.unwrap_or(60)))?;
        match self.auth.login_step(username, password) {
            Ok(out) => self.after_login(id, &mut rec, out),
            Err(e) => {
                self.count_attempt(id, &mut rec)?;
                let _ = self.flow_put(id, &rec);
                Ok(LoginOut::Failed(if e.contains("too many") {
                    "err.locked"
                } else {
                    "err.login"
                }))
            }
        }
    }

    /// Second sign-in step: a code from the authenticator app, or a recovery code.
    pub fn login_mfa(
        &self,
        id: &str,
        binder: &str,
        csrf: &str,
        ip: &str,
        mfa_token: &str,
        code: &str,
    ) -> Result<LoginOut, FlowError> {
        let mut rec = self.load(id, binder, Some(csrf))?;
        self.limit(
            &format!("oauth:rl:login:{}", &sha_hex(ip)[..24]),
            self.cfg.login_per_ip,
            600_000,
        )
        .map_err(|e| FlowError::Busy(e.retry_after.unwrap_or(60)))?;
        let code = code.trim();
        let digits = code.chars().filter(char::is_ascii_digit).count();
        let (totp, recovery) = if code.len() == 6 && digits == 6 {
            (Some(code), None)
        } else {
            (None, Some(code))
        };
        match self.auth.login_mfa(mfa_token, totp, recovery) {
            Ok(out) => self.after_login(id, &mut rec, out),
            Err(obop_commands::MfaError::Denied(_)) => {
                self.count_attempt(id, &mut rec)?;
                let _ = self.flow_put(id, &rec);
                Ok(LoginOut::MfaRetry(mfa_token.to_string()))
            }
            Err(_) => {
                self.count_attempt(id, &mut rec)?;
                let _ = self.flow_put(id, &rec);
                Ok(LoginOut::Failed("err.code_expired"))
            }
        }
    }

    /// Options for a passkey sign-in on the page: the second step of `mfa_token`, or usernameless.
    pub fn passkey_options(
        &self,
        ctx: &obop_commands::WebauthnCtx,
        id: &str,
        binder: &str,
        csrf: &str,
        mfa_token: Option<&str>,
    ) -> Result<J, FlowError> {
        let _ = self.load(id, binder, Some(csrf))?;
        let api = self.auth.passkeys().ok_or(FlowError::Forbidden)?;
        api.passkey_login_options(ctx, mfa_token)
            .map_err(|e| match e {
                obop_commands::MfaError::Locked(_) => FlowError::Busy(60),
                _ => FlowError::Forbidden,
            })
    }

    /// The browser's answer to a passkey request: signs in without a password, or completes the
    /// second step of `mfa_token`.
    pub fn login_passkey(
        &self,
        ctx: &obop_commands::WebauthnCtx,
        id: &str,
        binder: &str,
        csrf: &str,
        mfa_token: Option<&str>,
        credential: &J,
    ) -> Result<LoginOut, FlowError> {
        let mut rec = self.load(id, binder, Some(csrf))?;
        self.limit(
            &format!("oauth:rl:login:{}", &sha_hex(&ctx.ip)[..24]),
            self.cfg.login_per_ip,
            600_000,
        )
        .map_err(|e| FlowError::Busy(e.retry_after.unwrap_or(60)))?;
        let api = self.auth.passkeys().ok_or(FlowError::Forbidden)?;
        match api.passkey_login(ctx, mfa_token, credential) {
            Ok(out) => self.after_login(id, &mut rec, out),
            Err(obop_commands::MfaError::Locked(_)) => {
                self.count_attempt(id, &mut rec)?;
                let _ = self.flow_put(id, &rec);
                Ok(LoginOut::Failed("err.locked"))
            }
            Err(obop_commands::MfaError::Denied(_)) => {
                self.count_attempt(id, &mut rec)?;
                let _ = self.flow_put(id, &rec);
                Ok(match mfa_token {
                    Some(t) => LoginOut::MfaRetry(t.to_string()),
                    None => LoginOut::Failed("err.passkey"),
                })
            }
            Err(_) => {
                self.count_attempt(id, &mut rec)?;
                let _ = self.flow_put(id, &rec);
                Ok(LoginOut::Failed(if mfa_token.is_some() {
                    "err.code_expired"
                } else {
                    "err.passkey"
                }))
            }
        }
    }

    /// What can complete the second step of `mfa_token` (`totp`, `passkey`).
    pub fn mfa_methods(&self, mfa_token: &str) -> Vec<&'static str> {
        self.auth
            .passkeys()
            .map_or_else(|| vec!["totp"], |p| p.methods(mfa_token))
    }

    fn after_login(&self, id: &str, rec: &mut J, out: LoginOutcome) -> Result<LoginOut, FlowError> {
        match out {
            LoginOutcome::MfaRequired { mfa_token } => Ok(LoginOut::Mfa(mfa_token)),
            LoginOutcome::Session {
                token,
                mfa_enrolment_required,
                password_change_required,
            } => {
                let who = self.auth.resolve(&token);
                self.tokens().revoke_session(&token);
                if mfa_enrolment_required {
                    return Ok(LoginOut::Failed("err.enrol"));
                }
                if password_change_required {
                    return Ok(LoginOut::Failed("err.pwchange"));
                }
                let Some(p) = who.filter(|p| p.kind == obop_commands::PrincipalKind::User) else {
                    return Ok(LoginOut::Failed("err.login"));
                };
                rec["user"] = J::from(p.id);
                rec["attempts"] = J::from(0);
                self.flow_put(id, rec).map_err(|_| FlowError::Gone)?;
                Ok(LoginOut::Consent)
            }
        }
    }

    /// "Not you?": forget who signed in.
    pub fn switch_user(&self, id: &str, binder: &str, csrf: &str) -> Result<(), FlowError> {
        let mut rec = self.load(id, binder, Some(csrf))?;
        rec["user"] = J::Null;
        self.flow_put(id, &rec).map_err(|_| FlowError::Gone)
    }

    /// The person's answer. Approving needs at least one group, only groups the person holds,
    /// and one of the offered lifetimes.
    #[allow(clippy::too_many_arguments)]
    pub fn decide(
        &self,
        base: &str,
        id: &str,
        binder: &str,
        csrf: &str,
        approve: bool,
        groups: &[String],
        days: u32,
    ) -> Result<DecideOut, FlowError> {
        let rec = self.load(id, binder, Some(csrf))?;
        let Some(user) = rec["user"].as_str().map(String::from) else {
            return Err(FlowError::Forbidden);
        };
        let uri = rec["redirect_uri"].as_str().unwrap_or("").to_string();
        let state = rec["state"].as_str().map(String::from);
        let client_name = rec["client_name"].as_str().unwrap_or("").to_string();
        let client_id = rec["client_id"].as_str().unwrap_or("").to_string();
        if !approve {
            if !self.backend.kv_delete(&flow_key(id)).unwrap_or(false) {
                return Err(FlowError::Gone);
            }
            self.audit(
                &user,
                "oauth_denied",
                &format!("{} ({client_id}) was refused", clean_text(&client_name, 80)),
            );
            return Ok(DecideOut::Redirect(self.error_redirect(
                base,
                &uri,
                state.as_deref(),
                "access_denied",
                "the person refused",
            )));
        }
        let Some(principal) = self.tokens().principal_of(&user) else {
            return Err(FlowError::Gone);
        };
        // The agent may only narrow what the owner holds (D18).
        if groups.iter().any(|g| !principal.groups.contains(g)) {
            return Ok(DecideOut::Retry("err.scope_widen"));
        }
        let class = Class::of_record(&rec);
        if class.is_mobile() && groups.iter().any(|g| is_admin_like(g)) {
            return Ok(DecideOut::Retry("err.mobile_admin"));
        }
        let mut chosen: Vec<String> = groups.to_vec();
        chosen.sort();
        chosen.dedup();
        if chosen.is_empty() {
            return Ok(DecideOut::Retry("err.no_scope"));
        }
        if !EXPIRY_DAYS.contains(&days) {
            return Ok(DecideOut::Retry("err.expiry"));
        }
        // Whoever deletes the flow first is the one answer.
        if !self.backend.kv_delete(&flow_key(id)).unwrap_or(false) {
            return Err(FlowError::Gone);
        }
        let code = format!("oac_{}", rand_hex::<32>());
        let now = self.now();
        let entry = json!({
            "client_id": client_id,
            "client_name": client_name,
            "class": class.as_str(),
            "redirect_uri": uri,
            "challenge": rec["challenge"],
            "resource": rec["resource"],
            "owner": user,
            "scopes": chosen,
            "days": days,
            "exp": now + self.cfg.code_ttl_secs,
        });
        self.backend
            .kv_put(
                &format!("oauth:code:{}", sha_hex(&code)),
                &entry.to_string(),
                Some(self.cfg.code_ttl_secs * 1000 * 2),
            )
            .map_err(|_| FlowError::Gone)?;
        self.touch_client(&client_id);
        self.audit(
            &user,
            "oauth_authorize",
            &format!(
                "{} ({client_id}) was given {} for {days} days",
                clean_text(&client_name, 80),
                chosen.join(", ")
            ),
        );
        let mut params = vec![("code", code.as_str())];
        if let Some(s) = state.as_deref() {
            params.push(("state", s));
        }
        params.push(("iss", base));
        Ok(DecideOut::Redirect(redirect::with_params(&uri, &params)))
    }
}
