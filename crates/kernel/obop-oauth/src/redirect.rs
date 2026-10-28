//! Redirect URIs: what may be registered, how a presented one is matched, and how a response is
//! appended to one. A redirect URI is the one place a code can leave the server, so the rules are
//! strict: `https` anywhere, plain `http` only for a loopback host (RFC 8252), no wildcards, no
//! fragment, no user information, nothing that a browser could read differently from us.
//!
//! A mobile app (the D64 amendment) follows RFC 8252 for native apps more narrowly, see
//! [`validate_mobile`]: a private-use scheme in reverse domain form, an `https` link the system
//! hands to the app, or a loopback address by number. It never gets `localhost`, because the name
//! can be made to point elsewhere and the numbers cannot.

use crate::util::pct;

/// `(scheme, host, port, path-and-query)` of a URI that passed [`validate`].
fn parts(uri: &str) -> Option<(&str, &str, Option<u16>, &str)> {
    fn port_of(p: &str) -> Option<u16> {
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        p.parse::<u16>().ok().filter(|p| *p != 0)
    }
    let (scheme, rest) = uri.split_once("://")?;
    let end = rest.find(['/', '?']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
        let (h, after) = v6.split_once(']')?;
        let port = match after {
            "" => None,
            p => Some(port_of(p.strip_prefix(':')?)?),
        };
        (&authority[..h.len() + 2], port)
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h, Some(port_of(p)?)),
            None => (authority, None),
        }
    };
    Some((scheme, host, port, tail))
}

pub fn is_loopback_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "[::1]")
}

/// Whether `uri` may be registered (and presented). The reason is a short English sentence for
/// the registration error.
pub fn validate(uri: &str) -> Result<(), &'static str> {
    if uri.is_empty() || uri.len() > 2048 {
        return Err("a redirect URI must be 1 to 2048 characters");
    }
    if uri
        .chars()
        .any(|c| c.is_control() || c.is_whitespace() || matches!(c, '*' | '\\' | '"' | '<' | '>'))
    {
        return Err(
            "a redirect URI must not contain wildcards, spaces, backslashes or control characters",
        );
    }
    if uri.contains('#') {
        return Err("a redirect URI must not have a fragment");
    }
    let Some((scheme, host, _port, tail)) = parts(uri) else {
        return Err("not an absolute URI with a valid host and port");
    };
    if !tail.is_empty() && !tail.starts_with('/') && !tail.starts_with('?') {
        return Err("not an absolute URI with a valid host and port");
    }
    if uri[scheme.len() + 3..]
        .split(['/', '?'])
        .next()
        .unwrap_or("")
        .contains('@')
    {
        return Err("a redirect URI must not carry user information");
    }
    let host_ok = !host.is_empty()
        && (host.starts_with('[')
            || host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-'));
    if !host_ok || host.starts_with('.') || host.ends_with('.') {
        return Err("the host of a redirect URI must be plain ASCII");
    }
    match scheme {
        "https" => Ok(()),
        "http" if is_loopback_host(&host.to_ascii_lowercase()) => Ok(()),
        "http" => Err("plain http is allowed only for localhost, 127.0.0.1 and [::1]"),
        _ => Err("a redirect URI must use https (or http on a loopback address)"),
    }
}

/// Whether `presented` is one of the `registered` URIs. The match is exact, except that a loopback
/// `http` URI may use any port (RFC 8252 section 7.3): the port of a native app's listener is
/// not known when it registers.
pub fn matches(registered: &[String], presented: &str) -> bool {
    if validate(presented).is_err() {
        return false;
    }
    if registered.iter().any(|r| r == presented) {
        return true;
    }
    let Some((ps, ph, _, pt)) = parts(presented) else {
        return false;
    };
    ps == "http"
        && is_loopback_host(ph)
        && registered
            .iter()
            .any(|r| parts(r).is_some_and(|(rs, rh, _, rt)| rs == "http" && rh == ph && rt == pt))
}

/// The scheme and the path of a private-use redirect URI of the form `scheme:/path`, if `uri` is
/// one that [`validate_mobile`] accepts as such.
fn private_use(uri: &str) -> Option<(&str, &str)> {
    let (scheme, path) = uri.split_once(':')?;
    let labels_ok = scheme.split('.').count() >= 2
        && scheme.split('.').all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        });
    let path_ok = path.starts_with('/')
        && !path.starts_with("//")
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-' | b'/'));
    (scheme.len() <= 128
        && scheme.starts_with(|c: char| c.is_ascii_lowercase())
        && labels_ok
        && path_ok
        && uri.len() <= 256)
        .then_some((scheme, path))
}

fn is_loopback_ip(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "[::1]")
}

/// Whether a mobile client may register (and present) `uri`: RFC 8252 section 7.
///
/// - a private-use scheme in reverse domain form with a path and nothing else, such as
///   `org.obop.mileage:/oauth` (no authority, no query, no fragment: section 7.1);
/// - an `https` link the system opens in the app (section 7.2), by the rules of every `https`
///   redirect here, except that its host is not a loopback name;
/// - `http` to `127.0.0.1` or `[::1]` with any port (section 7.3), never `localhost`.
///
/// Everything else is refused: a scheme without a dot (`claude://callback`, `javascript:`), `http`
/// to any other host, wildcards, user information, a fragment.
pub fn validate_mobile(uri: &str) -> Result<(), &'static str> {
    if private_use(uri).is_some() {
        return Ok(());
    }
    if let Some((scheme, host, _, _)) = parts(uri) {
        let host = host.to_ascii_lowercase();
        match (scheme, validate(uri)) {
            ("https", Ok(())) if !is_loopback_host(&host) => return Ok(()),
            ("http", Ok(())) if is_loopback_ip(&host) => return Ok(()),
            ("http", Ok(())) => {
                return Err(
                    "a mobile app may use plain http only for 127.0.0.1 and [::1], not for localhost or any other host",
                );
            }
            ("https", Ok(())) => return Err("a mobile app may not use https to a loopback host"),
            (_, Err(e)) => return Err(e),
            _ => {}
        }
    }
    Err(
        "a mobile redirect URI must be a private-use scheme in reverse domain form such as org.example.app:/oauth, an https link, or http on 127.0.0.1 or [::1]",
    )
}

/// As [`matches`], for a mobile client: the match is exact, except that the port of an `http`
/// loopback address (by number) may change.
pub fn matches_mobile(registered: &[String], presented: &str) -> bool {
    if validate_mobile(presented).is_err() {
        return false;
    }
    if registered.iter().any(|r| r == presented) {
        return true;
    }
    let Some((ps, ph, _, pt)) = parts(presented) else {
        return false;
    };
    ps == "http"
        && is_loopback_ip(ph)
        && registered
            .iter()
            .any(|r| parts(r).is_some_and(|(rs, rh, _, rt)| rs == "http" && rh == ph && rt == pt))
}

/// What the form-action of the consent page allows: `scheme://authority` of a valid URI, or
/// `scheme:` for a private-use one.
pub fn origin(uri: &str) -> String {
    if let Some((scheme, _)) = private_use(uri) {
        return format!("{scheme}:");
    }
    match parts(uri) {
        Some((scheme, host, port, _)) => match port {
            Some(p) => format!("{scheme}://{host}:{p}"),
            None => format!("{scheme}://{host}"),
        },
        None => String::new(),
    }
}

/// Where the browser (or the phone) will go, shown to the person: the host with its port, or the
/// scheme of a private-use URI.
pub fn display_host(uri: &str) -> String {
    if let Some((scheme, _)) = private_use(uri) {
        return scheme.to_string();
    }
    origin(uri)
        .split_once("://")
        .map_or(String::new(), |(_, a)| a.to_string())
}

/// `uri` with the response parameters appended to its query.
pub fn with_params(uri: &str, params: &[(&str, &str)]) -> String {
    let mut out = String::from(uri);
    let mut sep = if uri.contains('?') { '&' } else { '?' };
    for (k, v) in params {
        out.push(sep);
        sep = '&';
        out.push_str(k);
        out.push('=');
        out.push_str(&pct(v));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registerable_uris() {
        for ok in [
            "https://claude.ai/api/mcp/auth_callback",
            "https://example.com/cb?x=1",
            "http://localhost:33418/callback",
            "http://127.0.0.1/cb",
            "http://[::1]:8080/cb",
            "https://example.com:8443/cb",
        ] {
            assert_eq!(validate(ok), Ok(()), "{ok}");
        }
        for bad in [
            "",
            "http://example.com/cb",
            "ftp://example.com/cb",
            "claude://callback",
            "https://example.com/cb#frag",
            "https://*.example.com/cb",
            "https://example.com/*",
            "https://user:pw@example.com/cb",
            "https://example.com@evil.test/cb",
            "https://example.com\\@evil.test/cb",
            "https:///cb",
            "https://exa mple.com/cb",
            "https://example.com:99999/cb",
            "https://example.com:0/cb",
            "https://bücher.example/cb",
            "http://localhost.evil.test/cb",
            "http://0.0.0.0/cb",
            "javascript:alert(1)",
            "//evil.test/cb",
            "/relative",
        ] {
            assert!(validate(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn matching_is_exact_except_for_the_loopback_port() {
        let reg = vec![
            "https://claude.ai/api/mcp/auth_callback".to_string(),
            "http://localhost:1234/callback".to_string(),
        ];
        assert!(matches(&reg, "https://claude.ai/api/mcp/auth_callback"));
        assert!(!matches(&reg, "https://claude.ai/api/mcp/auth_callback/"));
        assert!(!matches(
            &reg,
            "https://claude.ai/api/mcp/auth_callback?x=1"
        ));
        assert!(!matches(
            &reg,
            "https://claude.ai.evil.test/api/mcp/auth_callback"
        ));
        assert!(!matches(&reg, "http://claude.ai/api/mcp/auth_callback"));
        assert!(matches(&reg, "http://localhost:1234/callback"));
        assert!(matches(&reg, "http://localhost:55555/callback"));
        assert!(!matches(&reg, "http://localhost:55555/other"));
        assert!(!matches(&reg, "http://127.0.0.1:1234/callback"));
        assert!(!matches(&reg, "https://localhost:1234/callback"));
    }

    #[test]
    fn responses_are_appended_and_encoded() {
        assert_eq!(
            with_params("https://a.test/cb", &[("code", "x y"), ("state", "a&b")]),
            "https://a.test/cb?code=x%20y&state=a%26b"
        );
        assert_eq!(
            with_params("https://a.test/cb?k=1", &[("code", "c")]),
            "https://a.test/cb?k=1&code=c"
        );
        assert_eq!(origin("http://localhost:3/cb"), "http://localhost:3");
        assert_eq!(display_host("https://claude.ai/x"), "claude.ai");
    }
}
