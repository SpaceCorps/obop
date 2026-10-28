//! Dynamic client registration (RFC 7591) of public clients. Anybody may register, so the
//! registration is capped, rate limited, and forgotten when unused; a client name is only ever
//! shown as plain text next to the address the person will be sent to.
//!
//! A client has a class. The class of every client that registered before classes existed, and of
//! one that does not ask for another, is `mcp`: an agent's connector, whose access token is a
//! scoped agent token bound to the `/mcp` endpoint. The class `mobile` (the D64 amendment) is for
//! a native app: redirect URIs by the rules of RFC 8252, an access token bound to the `/api`
//! endpoint that is the person themselves narrowed to the groups they handed over, and a `device`
//! record for each sign-in. The class is a label the client picks and the person is shown on the
//! consent page; it grants nothing by itself (docs/DEPLOY.md "Mobile apps" says what an attacker
//! who registers a mobile client can and cannot do).

use crate::util::{clean_text, rand_hex};
use crate::{OAuth, OAuthError, redirect};
use obop_storage::Fields;
use serde_json::{Value as J, json};

pub const CLIENT_MODEL: &str = "oauth_client";

/// What kind of application a client is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Class {
    /// An agent's connector (Claude and the like): `/mcp`, a scoped agent token.
    #[default]
    Mcp,
    /// A native mobile app: `/api`, the person narrowed, a device record.
    Mobile,
}

impl Class {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mcp => "mcp",
            Self::Mobile => "mobile",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "mcp" => Some(Self::Mcp),
            "mobile" => Some(Self::Mobile),
            _ => None,
        }
    }

    /// The class a stored record names: `mcp` when it names none, as every record made before
    /// classes existed.
    pub fn of_fields(rec: &Fields) -> Self {
        rec.get("class")
            .and_then(J::as_str)
            .and_then(Self::parse)
            .unwrap_or_default()
    }

    /// As [`Class::of_fields`], for a record held as a JSON value.
    pub fn of_record(rec: &J) -> Self {
        rec.as_object().map(Self::of_fields).unwrap_or_default()
    }

    pub fn is_mobile(self) -> bool {
        self == Self::Mobile
    }
}

#[derive(Debug, Clone)]
pub struct Client {
    pub id: String,
    pub name: String,
    pub redirect_uris: Vec<String>,
    pub class: Class,
}

fn fields(v: J) -> Fields {
    v.as_object().cloned().unwrap_or_default()
}

impl OAuth {
    /// `POST /oauth/register`. `ip` keys the rate limit.
    pub fn register(&self, ip: &str, body: &J) -> Result<J, OAuthError> {
        self.housekeeping();
        let ip_key = crate::util::sha_hex(ip);
        self.limit(
            &format!("oauth:rl:reg:{}", &ip_key[..24]),
            self.cfg.register_per_ip_hour,
            3_600_000,
        )?;
        self.limit("oauth:rl:reg:all", self.cfg.register_global_hour, 3_600_000)?;
        let bad = |e: &str| OAuthError::new(400, "invalid_client_metadata", e);
        let obj = body
            .as_object()
            .ok_or_else(|| bad("the body must be a JSON object"))?;
        let uris = obj
            .get("redirect_uris")
            .and_then(J::as_array)
            .ok_or_else(|| {
                OAuthError::new(400, "invalid_redirect_uri", "redirect_uris is required")
            })?;
        if uris.is_empty() || uris.len() > self.cfg.max_redirect_uris {
            return Err(OAuthError::new(
                400,
                "invalid_redirect_uri",
                &format!("give 1 to {} redirect URIs", self.cfg.max_redirect_uris),
            ));
        }
        let class = match obj.get("obop_client_class") {
            None => Class::Mcp,
            Some(c) => c
                .as_str()
                .and_then(Class::parse)
                .ok_or_else(|| bad("obop_client_class must be \"mcp\" or \"mobile\""))?,
        };
        let mut redirect_uris: Vec<String> = Vec::new();
        for u in uris {
            let u = u.as_str().ok_or_else(|| {
                OAuthError::new(
                    400,
                    "invalid_redirect_uri",
                    "a redirect URI must be a string",
                )
            })?;
            let checked = match class {
                Class::Mcp => redirect::validate(u),
                Class::Mobile => redirect::validate_mobile(u),
            };
            checked.map_err(|e| OAuthError::new(400, "invalid_redirect_uri", e))?;
            if !redirect_uris.iter().any(|x| x == u) {
                redirect_uris.push(u.to_string());
            }
        }
        if let Some(m) = obj.get("token_endpoint_auth_method")
            && m.as_str() != Some("none")
        {
            return Err(bad(
                "only public clients are supported: token_endpoint_auth_method must be none",
            ));
        }
        if let Some(g) = obj.get("grant_types") {
            let list: Vec<&str> = g
                .as_array()
                .ok_or_else(|| bad("grant_types must be an array"))?
                .iter()
                .filter_map(J::as_str)
                .collect();
            if !list.contains(&"authorization_code")
                || list
                    .iter()
                    .any(|g| !matches!(*g, "authorization_code" | "refresh_token"))
            {
                return Err(bad(
                    "grant_types may only be authorization_code and refresh_token",
                ));
            }
        }
        if let Some(r) = obj.get("response_types")
            && r.as_array()
                .map(|a| a.iter().filter_map(J::as_str).collect::<Vec<_>>())
                != Some(vec!["code"])
        {
            return Err(bad("response_types must be [\"code\"]"));
        }
        let name = clean_text(obj.get("client_name").and_then(J::as_str).unwrap_or(""), 80);
        let name = if name.is_empty() {
            "Unnamed application".to_string()
        } else {
            name
        };
        let now = self.now();
        if self.backend.count(CLIENT_MODEL).unwrap_or(0) >= self.cfg.max_clients {
            self.purge_clients();
            if self.backend.count(CLIENT_MODEL).unwrap_or(0) >= self.cfg.max_clients {
                return Err(OAuthError::unavailable(
                    "this instance cannot register more applications right now",
                    60,
                ));
            }
        }
        let id = format!("oc_{}", rand_hex::<16>());
        let mut record = fields(json!({
            "name": name,
            "redirect_uris": redirect_uris,
            "created_at": now,
            "last_used_at": now,
            "used": false,
        }));
        // Only a class other than the default is written, so a record of an MCP client is what
        // it always was.
        if class.is_mobile() {
            record.insert("class".into(), J::from(class.as_str()));
        }
        self.backend
            .insert(CLIENT_MODEL, &id, &record)
            .map_err(|_| OAuthError::storage_busy())?;
        let mut doc = json!({
            "client_id": id,
            "client_id_issued_at": now,
            "client_name": name,
            "redirect_uris": redirect_uris,
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none",
        });
        if class.is_mobile() {
            doc["obop_client_class"] = J::from(class.as_str());
        }
        Ok(doc)
    }

    fn client_expired(&self, rec: &Fields) -> bool {
        let now = self.now();
        let created = rec.get("created_at").and_then(J::as_u64).unwrap_or(0);
        let last = rec
            .get("last_used_at")
            .and_then(J::as_u64)
            .unwrap_or(created);
        if rec.get("used").and_then(J::as_bool) == Some(true) {
            last + self.cfg.idle_client_secs <= now
        } else {
            created + self.cfg.unused_client_secs <= now
        }
    }

    /// The registered client, unless unknown or expired.
    pub fn client(&self, id: &str) -> Option<Client> {
        if !id.starts_with("oc_") || id.len() > 64 {
            return None;
        }
        let rec = self.backend.get(CLIENT_MODEL, id).ok()??;
        if self.client_expired(&rec) {
            return None;
        }
        Some(Client {
            id: id.to_string(),
            name: rec
                .get("name")
                .and_then(J::as_str)
                .unwrap_or("")
                .to_string(),
            redirect_uris: rec
                .get("redirect_uris")
                .and_then(J::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            class: Class::of_fields(&rec),
        })
    }

    /// Remember that a client was used (a code was approved for it).
    pub(crate) fn touch_client(&self, id: &str) {
        let _ = self.backend.update(
            CLIENT_MODEL,
            id,
            &fields(json!({"used": true, "last_used_at": self.now()})),
        );
    }

    /// Forget expired clients.
    pub(crate) fn purge_clients(&self) -> usize {
        let mut dead = Vec::new();
        let _ = self.backend.scan(CLIENT_MODEL, &mut |id, rec| {
            if self.client_expired(rec) {
                dead.push(id.to_string());
            }
            Ok(true)
        });
        dead.iter()
            .filter(|id| self.backend.delete(CLIENT_MODEL, id).unwrap_or(false))
            .count()
    }
}
