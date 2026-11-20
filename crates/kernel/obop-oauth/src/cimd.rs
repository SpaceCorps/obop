//! Client ID Metadata Documents (E7i; draft-ietf-oauth-client-id-metadata-document): an MCP client
//! may name itself by an `https` address instead of registering, and the server reads the client's
//! metadata from that address. Off unless the operator lists the hosts whose documents are read
//! (`OBOP_OAUTH_CIMD_HOSTS`): a document is fetched only from a listed host, only through the
//! hardened outbound client of the connectors (`https`, public addresses only, no redirects, a
//! size cap and a deadline, set up by `obop-http`), and kept for an hour in the shared store.
//!
//! The document must name its own address as `client_id`, list its redirect URIs (held to the same
//! rules as a registration, the operator's host list of E1c included), and be a public client
//! (`token_endpoint_auth_method` `none` or absent). A client named this way is an MCP client; a
//! mobile app registers as before.

use crate::client::{Class, Client};
use crate::util::{clean_text, sha_hex};
use crate::{OAuth, redirect};
use serde_json::{Value as J, json};
use std::sync::Arc;

/// Reads a metadata document: the address in, the body out (or why not). Set by the HTTP layer.
pub type CimdFetch = dyn Fn(&str) -> Result<Vec<u8>, String> + Send + Sync;

/// The largest document read, in bytes (the draft suggests 5 KiB).
pub const MAX_DOCUMENT: usize = 5 * 1024;
/// How long a document that was read is kept, in seconds.
pub const CACHE_SECS: u64 = 3600;
/// Fetches of documents per minute, for the whole instance.
pub const FETCHES_PER_MINUTE: u64 = 30;

/// Whether a host is on the operator's list: an exact name, or `*.suffix` for its subdomains.
fn listed(host: &str, hosts: &[String]) -> bool {
    hosts.iter().any(|p| match p.strip_prefix("*.") {
        Some(suffix) => host.len() > suffix.len() + 1 && host.ends_with(&format!(".{suffix}")),
        None => host == p,
    })
}

/// The host of `id` when it is an address this instance may read a document from: `https` (plain
/// `http` only to a literal loopback address when `loopback` is set, for tests), a path other than
/// `/`, no port other than the scheme's, no user information, query, fragment or dot segment, and a
/// host on the operator's list.
pub fn document_host(id: &str, hosts: &[String], loopback: bool) -> Option<String> {
    if id.len() > 512 || id.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None;
    }
    let (scheme, rest) = id.split_once("://")?;
    let (authority, path) = rest.split_at(rest.find('/')?);
    if path == "/"
        || path.contains(['?', '#', '\\'])
        || path.split('/').any(|s| s == "." || s == "..")
        || authority.contains('@')
    {
        return None;
    }
    let host = match scheme {
        "https" if !authority.contains(':') => authority.to_ascii_lowercase(),
        "http" if loopback => {
            let (h, port) = authority.rsplit_once(':')?;
            port.parse::<u16>().ok()?;
            (h == "127.0.0.1").then(|| h.to_string())?
        }
        _ => return None,
    };
    (!host.is_empty() && listed(&host, hosts)).then_some(host)
}

/// The client a document describes, or why it is not one.
pub fn parse_document(id: &str, body: &[u8], oauth: &OAuth) -> Result<Client, String> {
    if body.len() > MAX_DOCUMENT {
        return Err(format!("the document is larger than {MAX_DOCUMENT} bytes"));
    }
    let doc: J =
        serde_json::from_slice(body).map_err(|_| "the document is not JSON".to_string())?;
    let obj = doc.as_object().ok_or("the document is not a JSON object")?;
    if obj.get("client_id").and_then(J::as_str) != Some(id) {
        return Err("the document's client_id is not its own address".into());
    }
    if obj.contains_key("client_secret") || obj.contains_key("client_secret_expires_at") {
        return Err("a document must not carry a client secret".into());
    }
    match obj.get("token_endpoint_auth_method").and_then(J::as_str) {
        None | Some("none") => {}
        Some(_) => return Err("only public clients are supported".into()),
    }
    let uris = obj
        .get("redirect_uris")
        .and_then(J::as_array)
        .ok_or("redirect_uris is required")?;
    if uris.is_empty() || uris.len() > oauth.cfg.max_redirect_uris {
        return Err("the number of redirect URIs is out of range".into());
    }
    let mut redirect_uris = Vec::new();
    for u in uris {
        let u = u.as_str().ok_or("a redirect URI must be a string")?;
        redirect::validate(u).map_err(String::from)?;
        if !oauth.cfg.mcp_redirect_allowed(u) {
            return Err("a redirect host is not on the operator's list".into());
        }
        if !redirect_uris.iter().any(|x| x == u) {
            redirect_uris.push(u.to_string());
        }
    }
    let name = clean_text(obj.get("client_name").and_then(J::as_str).unwrap_or(""), 80);
    Ok(Client {
        id: id.to_string(),
        name: if name.is_empty() {
            "Unnamed application".into()
        } else {
            name
        },
        redirect_uris,
        class: Class::Mcp,
        scope: Vec::new(),
    })
}

fn cache_key(id: &str) -> String {
    format!("oauth:cimd:{}", &sha_hex(id)[..40])
}

impl OAuth {
    /// Let the server read Client ID Metadata Documents through `fetch` (the hardened outbound
    /// client). Without it, or without the operator's host list, an address is no client.
    pub fn set_cimd_fetcher(&self, fetch: Arc<CimdFetch>) {
        let _ = self.cimd.set(fetch);
    }

    /// Whether this instance reads Client ID Metadata Documents (the metadata says so).
    pub fn cimd_enabled(&self) -> bool {
        self.cfg.cimd_hosts.is_some() && self.cimd.get().is_some()
    }

    /// The client an address names, from the cache or read now; `None` when the address is not
    /// one this instance reads, or its document cannot be used.
    pub(crate) fn cimd_client(&self, id: &str) -> Option<Client> {
        let hosts = self.cfg.cimd_hosts.as_deref()?;
        let fetch = self.cimd.get()?;
        document_host(id, hosts, self.cfg.cimd_allow_loopback)?;
        let key = cache_key(id);
        if let Ok(Some(raw)) = self.backend.kv_get(&key)
            && let Ok(c) = parse_document(id, raw.as_bytes(), self)
        {
            return Some(c);
        }
        self.limit("oauth:rl:cimd:all", FETCHES_PER_MINUTE, 60_000)
            .ok()?;
        let body = match fetch(id) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("oauth: the client metadata document could not be read: {e}");
                return None;
            }
        };
        match parse_document(id, &body, self) {
            Ok(c) => {
                let text = String::from_utf8_lossy(&body).into_owned();
                let _ = self.backend.kv_put(&key, &text, Some(CACHE_SECS * 1000));
                Some(c)
            }
            Err(e) => {
                eprintln!("oauth: the client metadata document was refused: {e}");
                None
            }
        }
    }
}

/// The metadata of the authorization server, with the switch of the draft when this instance
/// reads documents (`json!` keeps the document as it was when it does not).
pub fn with_cimd(mut doc: J, enabled: bool) -> J {
    if enabled {
        doc["client_id_metadata_document_supported"] = json!(true);
    }
    doc
}
