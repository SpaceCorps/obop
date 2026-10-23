//! Cross-origin access for browser clients of the REST API (D68).
//!
//! Off unless `OBOP_ALLOWED_ORIGINS` lists exact origins (`https://app.example.com`, or
//! `http://localhost:5173` on loopback). The list applies to `/api/...` (and, as before, to
//! `/mcp`, which keeps its own handling). An origin that is on the list gets the CORS headers; any
//! other origin gets none, and a preflight from it is refused. The origin echoed back is always
//! the one the list holds, never a pattern, never `*`. Requests are authenticated by bearer token
//! only, so `Access-Control-Allow-Credentials` is never sent: a page cannot use the visitor's
//! cookies against the API. The sign-in and consent pages of the OAuth server are not under
//! `/api` and get nothing from here.

use super::HttpState;
use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

/// Methods the REST API has.
const METHODS: &str = "GET, POST, OPTIONS";
/// Request headers a browser client may send (lowercase).
const REQUEST_HEADERS: [&str; 4] = ["authorization", "content-type", "accept", "accept-language"];
const ALLOW_HEADERS: &str = "Authorization, Content-Type, Accept, Accept-Language";
/// Response headers a page may read besides the safelisted ones.
const EXPOSE_HEADERS: &str = "Link, Retry-After, WWW-Authenticate";
/// Seconds a browser may cache a preflight answer (Chrome caps it at 600).
const MAX_AGE: &str = "600";

/// The validated allow-list.
#[derive(Clone, Debug, Default)]
pub struct Cors {
    origins: Vec<String>,
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "[::1]") || host.ends_with(".localhost")
}

/// One list entry, normalised (lowercase, no trailing slash), or why it is refused.
pub fn validate_origin(raw: &str) -> Result<String, String> {
    let bad = |why: &str| Err(format!("{raw:?}: {why}"));
    let o = raw.trim();
    if o.contains('*') {
        return bad("wildcards are not allowed; list each origin exactly");
    }
    if o.eq_ignore_ascii_case("null") {
        return bad("the origin \"null\" cannot be allowed");
    }
    let o = o.trim_end_matches('/').to_ascii_lowercase();
    let Some((scheme, authority)) = o.split_once("://") else {
        return bad("an origin looks like https://app.example.com");
    };
    if !matches!(scheme, "http" | "https") {
        return bad("the scheme must be https (or http on localhost)");
    }
    if authority.is_empty()
        || authority
            .chars()
            .any(|c| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']')))
    {
        return bad(
            "an origin has a host and an optional port and nothing else (no path, query, user name or spaces)",
        );
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if !p.contains(']') => (h, Some(p)),
        _ => (authority, None),
    };
    if host.is_empty() || host.starts_with('.') || host.ends_with('.') || host.contains("..") {
        return bad("the host name is not valid");
    }
    if let Some(p) = port
        && !p.parse::<u16>().is_ok_and(|n| n != 0)
    {
        return bad("the port must be a number from 1 to 65535");
    }
    if scheme == "http" && !is_loopback_host(host) {
        return bad("plain http is only allowed for localhost, 127.0.0.1 and [::1]; use https");
    }
    Ok(o)
}

/// Parse a comma-separated list. An empty list is valid (CORS off). Duplicates collapse.
pub fn parse_list(raw: &str) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for item in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let o = validate_origin(item)
            .map_err(|e| format!("OBOP_ALLOWED_ORIGINS has an invalid entry {e}"))?;
        if !out.contains(&o) {
            out.push(o);
        }
    }
    Ok(out)
}

/// `OBOP_ALLOWED_ORIGINS`, validated. Unset or empty is an empty list.
pub fn from_env() -> Result<Vec<String>, String> {
    parse_list(&std::env::var("OBOP_ALLOWED_ORIGINS").unwrap_or_default())
}

impl Cors {
    /// From a list already validated by `parse_list`; entries that fail validation are dropped,
    /// so a list built by hand can never allow more than the rules do.
    pub fn new(origins: &[String]) -> Self {
        let mut v: Vec<String> = Vec::new();
        for o in origins {
            if let Ok(o) = validate_origin(o)
                && !v.contains(&o)
            {
                v.push(o);
            }
        }
        Cors { origins: v }
    }

    pub fn is_off(&self) -> bool {
        self.origins.is_empty()
    }

    /// Exact match on what the browser sent: no case folding, no trailing slash, no patterns.
    pub fn allows(&self, origin: &str) -> bool {
        self.origins.iter().any(|o| o == origin)
    }
}

fn is_api_path(p: &str) -> bool {
    p.starts_with("/api/")
}

fn header_str<'a>(h: &'a HeaderMap, name: &HeaderName) -> Option<&'a str> {
    h.get(name).and_then(|v| v.to_str().ok())
}

fn add_vary(h: &mut HeaderMap) {
    h.append(header::VARY, HeaderValue::from_static("Origin"));
}

fn static_value(s: &'static str) -> HeaderValue {
    HeaderValue::from_static(s)
}

fn preflight(cors: &Cors, h: &HeaderMap) -> Response {
    let refuse = || {
        let mut r = Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body(Body::empty())
            .expect("static response");
        add_vary(r.headers_mut());
        r
    };
    let Some(origin) = header_str(h, &header::ORIGIN).filter(|o| cors.allows(o)) else {
        return refuse();
    };
    let method_ok = header_str(h, &header::ACCESS_CONTROL_REQUEST_METHOD)
        .is_some_and(|m| matches!(m, "GET" | "POST"));
    let headers_ok = header_str(h, &header::ACCESS_CONTROL_REQUEST_HEADERS).is_none_or(|list| {
        list.split(',')
            .map(|x| x.trim().to_ascii_lowercase())
            .filter(|x| !x.is_empty())
            .all(|x| REQUEST_HEADERS.contains(&x.as_str()))
    });
    let Ok(origin) = HeaderValue::from_str(origin) else {
        return refuse();
    };
    if !method_ok || !headers_ok {
        return refuse();
    }
    let mut r = Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .expect("static response");
    let hs = r.headers_mut();
    hs.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    hs.insert(header::ACCESS_CONTROL_ALLOW_METHODS, static_value(METHODS));
    hs.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        static_value(ALLOW_HEADERS),
    );
    hs.insert(header::ACCESS_CONTROL_MAX_AGE, static_value(MAX_AGE));
    hs.append(header::VARY, static_value("Origin"));
    hs.append(
        header::VARY,
        static_value("Access-Control-Request-Method, Access-Control-Request-Headers"),
    );
    r
}

/// The CORS layer. With the list empty (the default) or off `/api/`, it does nothing at all.
pub(crate) async fn layer(State(s): State<Arc<HttpState>>, req: Request, next: Next) -> Response {
    let cors = &s.cors;
    if cors.is_off() || !is_api_path(req.uri().path()) {
        return next.run(req).await;
    }
    let origin = header_str(req.headers(), &header::ORIGIN).map(str::to_string);
    if req.method() == Method::OPTIONS
        && origin.is_some()
        && req
            .headers()
            .contains_key(header::ACCESS_CONTROL_REQUEST_METHOD)
    {
        return preflight(cors, req.headers());
    }
    let mut resp = next.run(req).await;
    // The answer depends on the Origin header whether or not this caller got CORS headers, so a
    // shared cache must not hand one caller's answer to another.
    add_vary(resp.headers_mut());
    if let Some(o) = origin.filter(|o| cors.allows(o))
        && let Ok(v) = HeaderValue::from_str(&o)
    {
        let hs = resp.headers_mut();
        hs.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, v);
        hs.insert(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            static_value(EXPOSE_HEADERS),
        );
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_entries_are_normalised() {
        assert_eq!(
            parse_list("https://App.Example.com/, http://localhost:5173 ,https://a.b:8443")
                .unwrap(),
            [
                "https://app.example.com",
                "http://localhost:5173",
                "https://a.b:8443"
            ]
        );
        assert_eq!(
            parse_list("http://127.0.0.1:3000,http://[::1]:3000,http://x.localhost")
                .unwrap()
                .len(),
            3
        );
        assert!(parse_list("").unwrap().is_empty());
        assert!(parse_list(" , ,").unwrap().is_empty());
        assert_eq!(
            parse_list("https://a.example,https://a.example").unwrap(),
            ["https://a.example"]
        );
    }

    #[test]
    fn wildcards_and_other_invalid_values_are_refused_with_a_message_that_names_the_entry() {
        for bad in [
            "*",
            "https://*.example.com",
            "https://app.example.com,*",
            "null",
            "app.example.com",
            "ftp://app.example.com",
            "http://app.example.com",
            "http://192.168.1.5:3000",
            "https://app.example.com/path",
            "https://app.example.com?x=1",
            "https://user@app.example.com",
            "https://app.example.com:0",
            "https://app.example.com:99999",
            "https://app.example.com:abc",
            "https://",
            "https://app example.com",
            "https://.example.com",
        ] {
            let e = parse_list(bad).unwrap_err();
            assert!(
                e.starts_with("OBOP_ALLOWED_ORIGINS has an invalid entry"),
                "{bad}: {e}"
            );
        }
        let e = parse_list("https://ok.example,*").unwrap_err();
        assert!(e.contains("\"*\"") && e.contains("wildcards"), "{e}");
    }

    #[test]
    fn matching_is_exact_on_what_the_browser_sent() {
        let c = Cors::new(&["https://app.example.com".into()]);
        assert!(c.allows("https://app.example.com"));
        for no in [
            "https://APP.example.com",
            "https://app.example.com/",
            "https://app.example.com:443",
            "http://app.example.com",
            "https://evil.app.example.com",
            "https://app.example.com.evil.test",
            "null",
            "",
        ] {
            assert!(!c.allows(no), "{no}");
        }
        assert!(Cors::new(&["*".into(), "http://evil.test".into()]).is_off());
    }
}
