//! The identity half of the OAuth authorization server (D64): short-lived scoped agent tokens
//! that carry an audience and the grant they belong to. The protocol itself lives in `obop-oauth`.

use super::{Auth, hex, random_bytes, token_hash};
use obop_commands::{OAuthTokens, Principal, PrincipalKind};
use obop_storage::Fields;
use serde_json::{Value as J, json};

impl OAuthTokens for Auth {
    fn principal_of(&self, user: &str) -> Option<Principal> {
        self.user_principal(&user.trim().to_lowercase())
    }

    fn issue_oauth_access(
        &self,
        owner: &str,
        scopes: &[String],
        label: &str,
        ttl_secs: u64,
        audience: &str,
        grant: &str,
    ) -> Result<(String, String), String> {
        let p = self
            .user_principal(owner)
            .ok_or("the account is unknown or disabled")?;
        if p.kind != PrincipalKind::User {
            return Err("only a human user can delegate".into());
        }
        if scopes.is_empty() {
            return Err("a token needs at least one scope".into());
        }
        // Agents may only narrow what the owner holds (D18).
        if let Some(bad) = scopes.iter().find(|s| !p.groups.contains(*s)) {
            return Err(format!("{owner} does not hold {bad}"));
        }
        let token = format!("obop_{}", hex(&random_bytes::<32>()));
        let id = token_hash(&token);
        let mut f = Fields::new();
        f.insert("owner".into(), J::from(owner));
        f.insert("kind".into(), J::from("agent"));
        f.insert("scopes".into(), json!(scopes));
        f.insert("label".into(), J::from(label));
        f.insert("expires_at".into(), J::from(self.now() + ttl_secs));
        f.insert("revoked".into(), J::Bool(false));
        f.insert("limited".into(), J::Bool(false));
        f.insert("created_at".into(), J::from(self.now()));
        f.insert("audience".into(), J::from(audience));
        f.insert("oauth_grant".into(), J::from(grant));
        self.backend.insert("auth_token", &id, &f)?;
        Ok((token, id))
    }

    fn revoke_token_id(&self, id: &str) -> bool {
        self.try_revoke_token_id(id).unwrap_or(false)
    }

    fn try_revoke_token_id(&self, id: &str) -> Result<bool, String> {
        let mut p = Fields::new();
        p.insert("revoked".into(), J::Bool(true));
        self.backend.update("auth_token", id, &p)
    }

    fn revoke_session(&self, token: &str) -> bool {
        self.revoke(token)
    }

    fn grant_of(&self, token: &str) -> Option<String> {
        let t = self.backend.get("auth_token", &token_hash(token)).ok()??;
        t.get("oauth_grant").and_then(J::as_str).map(String::from)
    }

    fn audit(&self, user: &str, kind: &str, actor: &str, detail: &str) {
        self.event(user, kind, actor, detail);
    }
}
