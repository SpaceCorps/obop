//! The checks of an OpenID Connect id token from Microsoft Entra (D91, Proposed), with no storage
//! and no network: a compact JWS, RS256 only, signature by `ring`, then the claims. Every function
//! takes untrusted input and answers a plain reason; none of them panics on bad input.
//!
//! What is checked, in this order, and why:
//!
//! 1. The shape: three base64url parts, no padding, at most 32 KiB. A JWS with a different count
//!    of parts (an encrypted token has five) is refused.
//! 2. The header: `alg` is exactly `RS256` (so `none`, `HS256` with the public key as the secret,
//!    `RS384`, `ES256` and every other value are refused before any key is looked at), there is
//!    no `crit`, and a `kid` names the key. Keys in the header (`jwk`, `jku`, `x5u`) are never
//!    used: the only keys are those of the key set the caller fetched from the pinned tenant.
//! 3. The signature, over the first two parts as they came.
//! 4. The claims, only after the signature: `iss` exactly `<authority>/<tenant>/v2.0` for the
//!    configured tenant, `tid` the configured tenant, `aud` the client id (a string, or a list of
//!    exactly one), `azp` the client id when present, `nonce` equal to the one of this flow,
//!    `exp` in the future, `nbf` and `iat` not in the future (a skew of two minutes), `oid` and
//!    `tid` GUIDs, `amr` read as a list of strings (absent or of another type: no `mfa`), and
//!    `auth_time` kept when it is a number.
//!
//! The issuer format `https://login.microsoftonline.com/<tenant id>/v2.0` is Microsoft's
//! documented one for v2.0 tokens (general knowledge, not tried against a real tenant).

use crate::totp::ct_eq;
use crate::webauthn::{b64url_decode, rsa_public_key_der};
use obop_commands::SsoIdentity;
use ring::signature::{RSA_PKCS1_2048_8192_SHA256, UnparsedPublicKey};
use serde_json::Value as J;

/// How far the clocks of the provider and of this server may differ.
pub const SKEW_SECS: u64 = 120;
/// The largest id token read. Real ones are 1 to 3 KiB.
pub const MAX_TOKEN_BYTES: usize = 32 * 1024;
/// The largest key set read (Entra publishes a handful of keys).
pub const MAX_JWKS_BYTES: usize = 256 * 1024;
const MAX_KEYS: usize = 100;
const MAX_KID: usize = 128;

/// The one authority of production.
pub const AUTHORITY: &str = "https://login.microsoftonline.com";

/// Why a token or a key set was not accepted. The text is for the security log, never shown to
/// the person who tried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    /// The key the header names is not in the key set: fetch it again, once, and retry.
    UnknownKey,
    Refused(&'static str),
}

fn refuse<T>(why: &'static str) -> Result<T, VerifyError> {
    Err(VerifyError::Refused(why))
}

// ---- tenant and authority ---------------------------------------------------------------------

/// A GUID in the 8-4-4-4-12 form.
pub fn is_guid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

/// Microsoft's documented tenant id of personal accounts (the GUID that `consumers` stands for).
const CONSUMERS_TENANT: &str = "9188040d-6c67-4c5b-b112-36a304b66dad";

/// The configured tenant: a GUID, lower case. The multi-tenant authorities and anything that is
/// not a GUID are refused, with the reason.
pub fn tenant_id(raw: &str) -> Result<String, String> {
    let t = raw.trim().to_ascii_lowercase();
    match t.as_str() {
        "common" | "organizations" | "consumers" => Err(format!(
            "{t:?} is a multi-tenant authority: sign-in is pinned to one tenant, given as its id (a GUID)"
        )),
        // The consumer (personal Microsoft account) tenant by its id: the GUID form of `consumers`.
        CONSUMERS_TENANT => Err(format!(
            "{t:?} is the tenant of personal Microsoft accounts, the same as consumers: sign-in is pinned to one organisation's tenant"
        )),
        _ if is_guid(&t) => Ok(t),
        _ => Err(
            "the tenant must be the tenant id, a GUID such as 00000000-0000-0000-0000-000000000000"
                .into(),
        ),
    }
}

/// An authority is the production one, or (tests only) a literal loopback address over http.
/// No path, no user information, no query.
pub fn authority_ok(a: &str) -> bool {
    if a == AUTHORITY {
        return true;
    }
    let Some(rest) = a.strip_prefix("http://") else {
        return false;
    };
    let Some((host, port)) = rest.rsplit_once(':') else {
        return false;
    };
    matches!(host, "127.0.0.1" | "[::1]")
        && !port.is_empty()
        && port.len() <= 5
        && port.bytes().all(|c| c.is_ascii_digit())
}

/// The issuer of v2.0 tokens of a tenant.
pub fn issuer(authority: &str, tenant: &str) -> String {
    format!("{authority}/{tenant}/v2.0")
}

// ---- the key set ------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Jwk {
    pub kid: String,
    /// The `RSAPublicKey` DER, ready for `ring`.
    pub der: Vec<u8>,
}

/// The keys of a JWKS that can verify an RS256 signature.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Jwks {
    pub keys: Vec<Jwk>,
}

impl Jwks {
    /// Read a key set. Keys of another type, `use` other than `sig`, `alg` other than `RS256`,
    /// `key_ops` without `verify`, or with a modulus outside 2048 to 4096 bits, are left out. A
    /// document that is not a key set, or leaves no key, is an error.
    pub fn parse(doc: &[u8]) -> Result<Self, String> {
        if doc.len() > MAX_JWKS_BYTES {
            return Err("the key set is too large".into());
        }
        let v: J = serde_json::from_slice(doc).map_err(|_| "the key set is not JSON")?;
        let list = v["keys"].as_array().ok_or("the key set has no keys")?;
        if list.len() > MAX_KEYS {
            return Err("the key set has too many keys".into());
        }
        let mut keys = Vec::new();
        for k in list {
            let usable = k["kty"].as_str() == Some("RSA")
                && k.get("use").is_none_or(|u| u.as_str() == Some("sig"))
                && k.get("alg").is_none_or(|a| a.as_str() == Some("RS256"))
                && k.get("key_ops").is_none_or(|ops| {
                    ops.as_array()
                        .is_some_and(|o| o.iter().any(|x| x.as_str() == Some("verify")))
                });
            let (Some(kid), true) = (
                k["kid"]
                    .as_str()
                    .filter(|s| !s.is_empty() && s.len() <= MAX_KID),
                usable,
            ) else {
                continue;
            };
            let n = k["n"].as_str().and_then(b64url_decode);
            let e = k["e"].as_str().and_then(b64url_decode);
            // The size is that of the number: zeros in front do not make a 1024-bit key a 2048-bit one.
            let n = n.map(|n| {
                let lead = n.iter().take_while(|b| **b == 0).count();
                n[lead..].to_vec()
            });
            if let (Some(n), Some(e)) = (n, e)
                && (256..=512).contains(&n.len())
                && !e.is_empty()
                && e.len() <= 4
            {
                keys.push(Jwk {
                    kid: kid.to_string(),
                    der: rsa_public_key_der(&n, &e),
                });
            }
        }
        if keys.is_empty() {
            return Err("the key set holds no key that can verify an RS256 signature".into());
        }
        Ok(Self { keys })
    }

    fn find(&self, kid: &str) -> Result<&Jwk, VerifyError> {
        let mut hits = self.keys.iter().filter(|k| k.kid == kid);
        match (hits.next(), hits.next()) {
            (None, _) => Err(VerifyError::UnknownKey),
            (Some(k), None) => Ok(k),
            // Two keys with one name: which one signed is a guess, and a guess is not a check.
            (Some(_), Some(_)) => refuse("two keys in the key set have the same kid"),
        }
    }
}

// ---- the discovery document ---------------------------------------------------------------------

/// The addresses of a discovery document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    pub authorization: String,
    pub token: String,
    pub jwks: String,
}

fn under(authority: &str, tenant: &str, url: &str) -> bool {
    let prefix = format!("{authority}/{tenant}/");
    url.len() <= 512
        && url.strip_prefix(&prefix).is_some_and(|rest| {
            !rest.is_empty()
                && rest
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"/._~-".contains(&c))
                && !rest.split('/').any(|seg| seg == ".." || seg == ".")
        })
}

/// Read the discovery document of the pinned tenant. Its `issuer` is exactly the issuer of the
/// tenant, and each of the three endpoints lies under `<authority>/<tenant>/` (so a document
/// that points the browser or the server anywhere else is refused).
pub fn parse_discovery(doc: &[u8], authority: &str, tenant: &str) -> Result<Endpoints, String> {
    if doc.len() > MAX_JWKS_BYTES {
        return Err("the discovery document is too large".into());
    }
    let v: J = serde_json::from_slice(doc).map_err(|_| "the discovery document is not JSON")?;
    if v["issuer"].as_str() != Some(issuer(authority, tenant).as_str()) {
        return Err("the discovery document names another issuer than the pinned tenant".into());
    }
    let get = |k: &str| -> Result<String, String> {
        let u = v[k].as_str().unwrap_or("");
        if under(authority, tenant, u) {
            Ok(u.to_string())
        } else {
            Err(format!(
                "the discovery document's {k} is not under the pinned tenant"
            ))
        }
    };
    Ok(Endpoints {
        authorization: get("authorization_endpoint")?,
        token: get("token_endpoint")?,
        jwks: get("jwks_uri")?,
    })
}

// ---- the token ---------------------------------------------------------------------------------

/// What a token is checked against.
#[derive(Debug, Clone, Copy)]
pub struct Expect<'a> {
    pub authority: &'a str,
    /// The pinned tenant, a lower-case GUID (`tenant_id`).
    pub tenant: &'a str,
    pub client_id: &'a str,
    /// The nonce of this flow.
    pub nonce: &'a str,
    /// Unix seconds.
    pub now: u64,
}

fn text<'a>(v: &'a J, k: &str) -> Option<&'a str> {
    v[k].as_str().filter(|s| !s.is_empty())
}

/// Check an id token. See the module description.
pub fn verify_id_token(
    token: &str,
    jwks: &Jwks,
    exp: &Expect<'_>,
) -> Result<SsoIdentity, VerifyError> {
    if token.len() > MAX_TOKEN_BYTES {
        return refuse("the token is too large");
    }
    let parts: Vec<&str> = token.split('.').collect();
    let [header, payload, signature] = parts[..] else {
        return refuse("not a compact JWS with three parts");
    };
    if header.is_empty() || payload.is_empty() || signature.is_empty() {
        return refuse("a part of the token is empty");
    }
    let h: J = b64url_decode(header)
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or(VerifyError::Refused("the header is not base64url JSON"))?;
    if !h.is_object() {
        return refuse("the header is not an object");
    }
    // The algorithm is fixed by the server, not chosen by the token: one exact value.
    if h["alg"].as_str() != Some("RS256") {
        return refuse("the algorithm is not RS256");
    }
    if h.get("crit").is_some() {
        return refuse("the header has critical extensions");
    }
    let Some(kid) = text(&h, "kid").filter(|k| k.len() <= MAX_KID) else {
        return refuse("the header names no key");
    };
    let key = jwks.find(kid)?;
    let sig =
        b64url_decode(signature).ok_or(VerifyError::Refused("the signature is not base64url"))?;
    let signed = &token[..header.len() + 1 + payload.len()];
    if UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, &key.der)
        .verify(signed.as_bytes(), &sig)
        .is_err()
    {
        return refuse("the signature does not verify");
    }
    // Only a token the provider signed is read as claims.
    let c: J = b64url_decode(payload)
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or(VerifyError::Refused("the claims are not base64url JSON"))?;
    claims(&c, exp)
}

fn claims(c: &J, exp: &Expect<'_>) -> Result<SsoIdentity, VerifyError> {
    let want_iss = issuer(exp.authority, exp.tenant);
    if c["iss"].as_str() != Some(want_iss.as_str()) {
        return refuse("the issuer is not the pinned tenant's");
    }
    let tid = text(c, "tid").map(str::to_ascii_lowercase);
    match tid.as_deref() {
        Some(t) if is_guid(t) => {
            if t != exp.tenant {
                return refuse("the tenant is not the pinned tenant");
            }
        }
        _ => return refuse("the token carries no tenant id"),
    }
    let aud_ok = match &c["aud"] {
        J::String(a) => a == exp.client_id,
        J::Array(a) => a.len() == 1 && a[0].as_str() == Some(exp.client_id),
        _ => false,
    };
    if !aud_ok {
        return refuse("the audience is not this client");
    }
    if let Some(azp) = c.get("azp")
        && azp.as_str() != Some(exp.client_id)
    {
        return refuse("the authorized party is not this client");
    }
    match c["nonce"].as_str() {
        Some(n) if !exp.nonce.is_empty() && ct_eq(n.as_bytes(), exp.nonce.as_bytes()) => {}
        _ => return refuse("the nonce is not the one of this sign-in"),
    }
    let time = |k: &str| -> Result<Option<u64>, VerifyError> {
        match c.get(k) {
            None | Some(J::Null) => Ok(None),
            Some(v) => v
                .as_u64()
                .or_else(|| {
                    v.as_f64()
                        .filter(|f| *f >= 0.0 && f.fract() == 0.0)
                        .map(|f| f as u64)
                })
                .map(Some)
                .ok_or(VerifyError::Refused("a time claim is not a number")),
        }
    };
    let Some(exp_at) = time("exp")? else {
        return refuse("the token has no expiry");
    };
    if exp_at.saturating_add(SKEW_SECS) <= exp.now {
        return refuse("the token has expired");
    }
    if let Some(nbf) = time("nbf")?
        && nbf > exp.now.saturating_add(SKEW_SECS)
    {
        return refuse("the token is not valid yet");
    }
    if let Some(iat) = time("iat")?
        && iat > exp.now.saturating_add(SKEW_SECS)
    {
        return refuse("the token was issued in the future");
    }
    let oid = text(c, "oid").map(str::to_ascii_lowercase);
    let Some(oid) = oid.filter(|o| is_guid(o)) else {
        return refuse(
            "the token carries no object id (oid); Entra sends it only when the profile scope is asked for and granted, so the scope, the app registration's permissions or the administrator's consent may be missing",
        );
    };
    // `amr` is a list of strings. Anything else says nothing about a second factor.
    let amr: Vec<String> = c["amr"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let auth_time = time("auth_time").ok().flatten();
    Ok(SsoIdentity {
        issuer: want_iss,
        tenant: exp.tenant.to_string(),
        oid,
        amr,
        auth_time,
    })
}
