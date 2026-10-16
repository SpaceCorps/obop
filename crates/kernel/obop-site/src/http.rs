//! A framework-neutral request and response, so this crate needs no web framework (axum lives
//! only in `obop-http`, D29).

use crate::util;
use std::collections::HashMap;

/// Largest request body any route accepts.
pub const MAX_BODY: usize = 32 * 1024;

#[derive(Debug, Clone, Default)]
pub struct Req {
    pub method: String,
    /// Path without the query string, percent-encoded as received.
    pub path: String,
    /// Raw query string without the `?`.
    pub query: String,
    /// Header names lower-cased.
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    /// The client address as the server saw it, used only to key rate limits (hashed first).
    pub client: String,
}

impl Req {
    pub fn new(method: &str, uri: &str) -> Req {
        let (path, query) = uri.split_once('?').unwrap_or((uri, ""));
        Req {
            method: method.to_ascii_uppercase(),
            path: path.into(),
            query: query.into(),
            ..Req::default()
        }
    }

    pub fn with_header(mut self, k: &str, v: &str) -> Req {
        self.headers.insert(k.to_ascii_lowercase(), v.into());
        self
    }

    pub fn with_body(mut self, body: &str) -> Req {
        self.body = body.as_bytes().to_vec();
        self
    }

    pub fn header(&self, k: &str) -> Option<&str> {
        self.headers.get(k).map(String::as_str)
    }

    pub fn query_pairs(&self) -> Vec<(String, String)> {
        form_urlencoded::parse(self.query.as_bytes())
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect()
    }

    pub fn q(&self, key: &str) -> Option<String> {
        self.query_pairs()
            .into_iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    pub fn form(&self) -> Form {
        Form(
            form_urlencoded::parse(&self.body)
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect(),
        )
    }

    pub fn cookie(&self, name: &str) -> Option<String> {
        self.header("cookie")?.split(';').find_map(|p| {
            let (k, v) = p.trim().split_once('=')?;
            (k == name).then(|| v.to_string())
        })
    }

    pub fn is_https(&self) -> bool {
        self.header("x-forwarded-proto").is_some_and(|p| {
            p.split(',')
                .next()
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case("https")
        })
    }

    /// `?format=md` or an `Accept` header that prefers `text/markdown` over `text/html`.
    pub fn wants_markdown(&self) -> bool {
        if let Some(f) = self.q("format") {
            return f == "md" || f == "markdown";
        }
        let Some(accept) = self.header("accept") else {
            return false;
        };
        let (mut md, mut html): (f32, f32) = (-1.0, -1.0);
        for part in accept.split(',') {
            let mut it = part.split(';');
            let ty = it.next().unwrap_or("").trim().to_ascii_lowercase();
            let q = it
                .find_map(|p| {
                    p.trim()
                        .strip_prefix("q=")
                        .and_then(|v| v.parse::<f32>().ok())
                })
                .unwrap_or(1.0);
            match ty.as_str() {
                "text/markdown" => md = md.max(q),
                "text/html" | "application/xhtml+xml" => html = html.max(q),
                _ => {}
            }
        }
        md > 0.0 && md >= html
    }

    /// Host and scheme of the request, e.g. `https://obop.example`.
    pub fn origin(&self) -> String {
        let host = self.header("host").unwrap_or("localhost");
        let scheme = if self.is_https() { "https" } else { "http" };
        format!("{scheme}://{host}")
    }
}

pub struct Form(pub Vec<(String, String)>);

impl Form {
    /// Every value of a repeated field (checkboxes).
    pub fn all(&self, k: &str) -> Vec<String> {
        self.0
            .iter()
            .filter(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .collect()
    }

    pub fn get(&self, k: &str) -> &str {
        self.0
            .iter()
            .find(|(n, _)| n == k)
            .map_or("", |(_, v)| v.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct Resp {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Resp {
    pub fn new(status: u16, content_type: &str, body: impl Into<Vec<u8>>) -> Resp {
        Resp {
            status,
            headers: vec![("content-type".into(), content_type.into())],
            body: body.into(),
        }
    }

    pub fn text(status: u16, body: &str) -> Resp {
        Resp::new(status, "text/plain; charset=utf-8", body)
    }

    pub fn redirect(status: u16, location: &str) -> Resp {
        let mut r = Resp::new(
            status,
            "text/plain; charset=utf-8",
            format!("Redirecting to {location}\n"),
        );
        r.headers.push(("location".into(), location.into()));
        r.headers.push(("cache-control".into(), "no-store".into()));
        r
    }

    pub fn header(mut self, k: &str, v: &str) -> Resp {
        self.headers.retain(|(n, _)| !n.eq_ignore_ascii_case(k));
        self.headers.push((k.into(), v.into()));
        self
    }

    pub fn add_header(mut self, k: &str, v: &str) -> Resp {
        self.headers.push((k.into(), v.into()));
        self
    }

    pub fn get_header(&self, k: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(k))
            .map(|(_, v)| v.as_str())
    }

    pub fn body_str(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// Add `ETag` (strong, from the body) and optionally `Last-Modified`, and answer `304` when
    /// the request's validators match.
    pub fn conditional(mut self, req: &Req, last_modified: Option<u64>) -> Resp {
        if self.status != 200 {
            return self;
        }
        let etag = format!(
            "\"{}\"",
            &util::sha256_hex(&String::from_utf8_lossy(&self.body))[..32]
        );
        let lm = last_modified.map(util::http_date);
        let not_modified = match req.header("if-none-match") {
            Some(inm) => inm
                .split(',')
                .any(|t| t.trim().trim_start_matches("W/") == etag || t.trim() == "*"),
            None => match (
                req.header("if-modified-since")
                    .and_then(util::parse_http_date),
                last_modified,
            ) {
                (Some(since), Some(lm)) => lm <= since,
                _ => false,
            },
        };
        self.headers.push(("etag".into(), etag));
        if let Some(lm) = lm {
            self.headers.push(("last-modified".into(), lm));
        }
        if not_modified && (req.method == "GET" || req.method == "HEAD") {
            self.status = 304;
            self.body.clear();
        }
        self
    }
}

pub fn cookie(name: &str, value: &str, max_age: u64, https: bool) -> String {
    format!(
        "{name}={value}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Lax{}",
        if https { "; Secure" } else { "" }
    )
}

pub fn expire_cookie(name: &str, https: bool) -> String {
    cookie(name, "", 0, https)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_negotiation() {
        let r = |a: &str| Req::new("GET", "/x").with_header("accept", a);
        assert!(r("text/markdown").wants_markdown());
        assert!(r("text/markdown, text/html;q=0.5").wants_markdown());
        assert!(!r("text/html,application/xhtml+xml,*/*;q=0.8").wants_markdown());
        assert!(!r("text/html, text/markdown;q=0.5").wants_markdown());
        assert!(!Req::new("GET", "/x").wants_markdown());
        assert!(Req::new("GET", "/x?format=md").wants_markdown());
    }

    #[test]
    fn parsing() {
        let r =
            Req::new("get", "/a/b?x=1&y=a%20b").with_header("cookie", "a=1; obop_session=tok; b=2");
        assert_eq!(r.q("y").as_deref(), Some("a b"));
        assert_eq!(r.cookie("obop_session").as_deref(), Some("tok"));
        assert_eq!(r.method, "GET");
        let f = Req::new("POST", "/").with_body("a=1&b=%C3%A4").form();
        assert_eq!((f.get("a"), f.get("b"), f.get("zz")), ("1", "ä", ""));
        assert!(
            Req::new("GET", "/")
                .with_header("x-forwarded-proto", "https")
                .is_https()
        );
    }

    #[test]
    fn conditional_requests() {
        let req = Req::new("GET", "/");
        let r = Resp::text(200, "hello").conditional(&req, Some(1_000_000));
        let etag = r.get_header("etag").unwrap().to_string();
        assert_eq!(r.status, 200);
        let r2 = Resp::text(200, "hello").conditional(
            &req.clone().with_header("if-none-match", &etag),
            Some(1_000_000),
        );
        assert_eq!((r2.status, r2.body.len()), (304, 0));
        let lm = r.get_header("last-modified").unwrap().to_string();
        let r3 = Resp::text(200, "hello").conditional(
            &req.clone().with_header("if-modified-since", &lm),
            Some(1_000_000),
        );
        assert_eq!(r3.status, 304);
        let r4 =
            Resp::text(200, "changed").conditional(&req.with_header("if-none-match", &etag), None);
        assert_eq!(r4.status, 200);
    }
}
