//! The back office shell (`index.html` of the Vite+ build) and the instance default language.
//!
//! The sign-in, two-factor, enrolment and forced-password screens are shown before anyone is
//! signed in, so before the instance settings can be read. `OBOP_DEFAULT_LANGUAGE` names the
//! language of the instance for them (D52 amendment). When it is set, the shell is served with
//! `<meta name="obop-default-language" content="sv">` added to its `<head>` at request time, and
//! the page reads it before its first render. The meta tag carries a language code and nothing
//! else; no other setting is public.
//!
//! The shell is the one document that must never be reused without asking the server: its
//! content depends on the build on disk and on this setting, so every HTML answer of the service
//! says `Cache-Control: no-cache`.
//!
//! Only the shell is touched. A request is first answered exactly as `ServeDir` answers it, and an
//! answer for a file that is not HTML (a script, a style sheet, a font, a picture, a PDF) goes out
//! byte for byte: `304`, `206`, `HEAD` and `Last-Modified` as they are, with or without a default
//! language. Only when the answer is HTML, or is one of the empty answers that cannot be told from
//! it (`304`, `412` and `416` carry no `Content-Type`), the document itself is asked for again
//! without the request's validators and range. That is what keeps a browser from holding on to a
//! copy of the page from before the setting existed: a `304` or a `206` for the shell would keep
//! it without the tag. The answer is then a `200` with the tag and without `ETag` and
//! `Last-Modified`.

use axum::{
    body::{Body, Bytes},
    extract::Request,
    http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, Version, header},
    response::{IntoResponse, Response},
};
use http_body_util::{BodyExt, LengthLimitError, Limited};
use tower::ServiceExt;
use tower_http::services::{ServeDir, ServeFile};

/// The setting.
pub const ENV: &str = "OBOP_DEFAULT_LANGUAGE";

/// The languages the interface ships (`web/packages/ui/src/i18n/locales.ts`); the same seven as the
/// server-rendered pages of the OAuth sign-in.
pub use obop_oauth::page::LANGS;

/// The name of the meta tag the page reads.
pub const META_NAME: &str = "obop-default-language";

/// The largest HTML document this service will rewrite. A real shell is a few kilobytes; a larger
/// document is served as it is, without the tag.
const MAX_SHELL_BYTES: usize = 4 * 1024 * 1024;

/// A language tag (`sv`, `sv-SE`, `SV`) as the shipped language it names (`sv`), or why it is not
/// one. Empty (or only spaces) means no default and is `Ok(None)`. Only the language part is kept:
/// the interface language is chosen by language alone, the region is a separate preference.
pub fn parse(raw: &str) -> Result<Option<&'static str>, String> {
    let tag = raw.trim();
    if tag.is_empty() {
        return Ok(None);
    }
    let bad = |why: &str| {
        Err(format!(
            "{ENV} is {raw:?}: {why}; use one of {}",
            LANGS.join(", ")
        ))
    };
    let mut parts = tag.split('-');
    let language = parts.next().unwrap_or_default().to_ascii_lowercase();
    // A BCP 47 tag is subtags of 1 to 8 letters or digits joined by hyphens.
    let subtag_ok =
        |s: &str| (1..=8).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric());
    if !subtag_ok(&language) || !parts.all(subtag_ok) {
        return bad("not a language tag such as sv or sv-SE");
    }
    match LANGS.into_iter().find(|l| *l == language) {
        Some(l) => Ok(Some(l)),
        None => bad("this version does not ship that language"),
    }
}

/// `OBOP_DEFAULT_LANGUAGE`, validated. Unset or empty is no default. A value that is not
/// valid Unicode is an error too: it must not read as "not set".
pub fn from_env() -> Result<Option<&'static str>, String> {
    match std::env::var(ENV) {
        Ok(v) => parse(&v),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(format!(
            "{ENV} is not valid text; use one of {}",
            LANGS.join(", ")
        )),
    }
}

/// `value` as the content of an HTML attribute in double quotes.
fn attribute(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
    out
}

/// The meta tag for `language`, escaped.
pub fn meta_tag(language: &str) -> String {
    format!(
        "<meta name=\"{META_NAME}\" content=\"{}\">",
        attribute(language)
    )
}

fn starts_with_ignore_case(hay: &[u8], at: usize, needle: &[u8]) -> bool {
    hay.get(at..at + needle.len())
        .is_some_and(|s| s.eq_ignore_ascii_case(needle))
}

fn contains_ignore_case(hay: &[u8], needle: &[u8]) -> bool {
    (0..hay.len()).any(|i| starts_with_ignore_case(hay, i, needle))
}

/// Where the content of `<head>` starts: just after the end of the opening tag. Comments are
/// skipped, `<header>` is not `<head>`, and a `<body>` before any `<head>` means there is none.
fn head_content_start(html: &[u8]) -> Option<usize> {
    let mut i = 0;
    while let Some(off) = html.get(i..)?.iter().position(|b| *b == b'<') {
        i += off;
        if html[i..].starts_with(b"<!--") {
            let end = html[i + 4..].windows(3).position(|w| w == b"-->")?;
            i += 4 + end + 3;
            continue;
        }
        let name_end = i + 5;
        if starts_with_ignore_case(html, i, b"<head")
            && matches!(
                html.get(name_end),
                Some(b'>' | b'/' | b' ' | b'\t' | b'\r' | b'\n' | 0x0c)
            )
        {
            // The end of the tag: the first `>` outside a quoted attribute value.
            let mut quote: Option<u8> = None;
            for (k, b) in html[name_end..].iter().enumerate() {
                match (quote, *b) {
                    (None, b'"' | b'\'') => quote = Some(*b),
                    (Some(q), b) if b == q => quote = None,
                    (None, b'>') => return Some(name_end + k + 1),
                    _ => {}
                }
            }
            return None;
        }
        if starts_with_ignore_case(html, i, b"<body") {
            return None;
        }
        i += 1;
    }
    None
}

/// `html` with the meta tag of `language` as the first thing in its `<head>`; `None` when the
/// document cannot take it (no `<head>`) or already has the tag, in which case the caller serves
/// the document as it is.
pub fn inject(html: &[u8], language: &str) -> Option<Vec<u8>> {
    if contains_ignore_case(html, META_NAME.as_bytes()) {
        return None;
    }
    let at = head_content_start(html)?;
    let tag = meta_tag(language);
    let mut out = Vec::with_capacity(html.len() + tag.len());
    out.extend_from_slice(&html[..at]);
    out.extend_from_slice(tag.as_bytes());
    out.extend_from_slice(&html[at..]);
    Some(out)
}

fn is_html(h: &HeaderMap) -> bool {
    h.get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.trim_start().to_ascii_lowercase().starts_with("text/html"))
}

/// The request headers that make `ServeDir` answer `304`, `412`, `206` or `416` instead of `200`.
const VALIDATORS: [header::HeaderName; 6] = [
    header::IF_MODIFIED_SINCE,
    header::IF_NONE_MATCH,
    header::IF_UNMODIFIED_SINCE,
    header::IF_MATCH,
    header::IF_RANGE,
    header::RANGE,
];

/// What a request asked for, kept so that the document can be asked for again without validators.
struct Asked {
    method: Method,
    uri: Uri,
    version: Version,
    headers: HeaderMap,
}

impl Asked {
    fn of(req: &Request) -> Self {
        Asked {
            method: req.method().clone(),
            uri: req.uri().clone(),
            version: req.version(),
            headers: req.headers().clone(),
        }
    }

    fn is_head(&self) -> bool {
        self.method == Method::HEAD
    }

    /// The same request as a plain `GET`: no validators and no range, so only a whole file answers.
    fn plain(&self) -> Request {
        let mut req = Request::new(Body::empty());
        *req.method_mut() = Method::GET;
        *req.uri_mut() = self.uri.clone();
        *req.version_mut() = self.version;
        let headers = req.headers_mut();
        for (name, value) in &self.headers {
            if !VALIDATORS.contains(name) {
                headers.append(name.clone(), value.clone());
            }
        }
        req
    }
}

/// Why a body could not be read for rewriting.
#[derive(Debug)]
enum Unread {
    /// It is longer than the limit.
    TooLarge,
    /// The read itself failed.
    Failed(String),
}

/// The whole body, or why not; never more than `limit` bytes are held.
async fn read_limited(body: Body, limit: usize) -> Result<Bytes, Unread> {
    match Limited::new(body, limit).collect().await {
        Ok(all) => Ok(all.to_bytes()),
        Err(e) if e.is::<LengthLimitError>() => Err(Unread::TooLarge),
        Err(e) => Err(Unread::Failed(e.to_string())),
    }
}

/// An HTML answer is never reused without asking the server.
fn no_cache(mut resp: Response) -> Response {
    if is_html(resp.headers()) {
        resp.headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    }
    resp
}

/// A `HEAD` request gets the headers of the answer to the `GET` and no body.
fn for_method(resp: Response, asked: &Asked) -> Response {
    if !asked.is_head() {
        return resp;
    }
    let (parts, _) = resp.into_parts();
    Response::from_parts(parts, Body::empty())
}

/// Serves the files of a back office build, the shell for every path that is not a file, and the
/// shell with the default language when there is one.
#[derive(Clone)]
pub struct Shell {
    files: ServeDir<ServeFile>,
    language: Option<&'static str>,
    /// The largest document that is rewritten.
    limit: usize,
}

impl Shell {
    /// Files from `dir`; `language` (from [`parse`] or [`from_env`]) is added to the shell.
    pub fn new(dir: &str, language: Option<&'static str>) -> Self {
        if language.is_some() {
            Self::check_shell(dir);
        }
        Shell {
            files: ServeDir::new(dir).fallback(ServeFile::new(format!("{dir}/index.html"))),
            language,
            limit: MAX_SHELL_BYTES,
        }
    }

    /// Says at start-up when the build cannot take the tag, so a missing default language is not
    /// a mystery later. The server still starts: the build may be copied in afterwards.
    fn check_shell(dir: &str) {
        match std::fs::read(format!("{dir}/index.html")) {
            Ok(html) => {
                if inject(&html, "en").is_none()
                    && !contains_ignore_case(&html, META_NAME.as_bytes())
                {
                    eprintln!(
                        "warning: {dir}/index.html has no <head>: {ENV} cannot be added to the sign-in screens"
                    );
                }
            }
            Err(e) => eprintln!(
                "warning: cannot read {dir}/index.html ({e}): the back office is not served until it exists"
            ),
        }
    }

    /// This service as a router that answers every path, to mount under a prefix.
    pub fn into_router(self) -> axum::Router {
        axum::Router::new().fallback(move |req: Request| {
            let shell = self.clone();
            async move { shell.serve(req).await }
        })
    }

    /// The answer of the file service to a request, as it gives it.
    async fn ask(&self, req: Request) -> Response {
        match self.files.clone().oneshot(req).await {
            Ok(r) => r.into_response(),
            Err(never) => match never {},
        }
    }

    /// The answer to one request.
    pub async fn serve(&self, req: Request) -> Response {
        let Some(language) = self.language else {
            return no_cache(self.ask(req).await);
        };
        let asked = Asked::of(&req);
        let first = self.ask(req).await;
        // `304`, `412` and `416` carry no `Content-Type`: the file they speak of may be the shell.
        let empty = matches!(
            first.status(),
            StatusCode::NOT_MODIFIED
                | StatusCode::PRECONDITION_FAILED
                | StatusCode::RANGE_NOT_SATISFIABLE
        );
        if !empty && !is_html(first.headers()) {
            // A file: exactly the answer of the file service (`304`, `206`, `HEAD` and all).
            return first;
        }
        // The shell, or an answer that may be about it. A `200` to a `GET` is the whole document;
        // anything else (a `HEAD`, a `206`, an empty answer) is asked for again as a plain `GET`.
        let whole = asked.method == Method::GET
            && first.status() == StatusCode::OK
            && is_html(first.headers());
        let (doc, original) = if whole {
            (first, None)
        } else {
            (self.ask(asked.plain()).await, Some(first))
        };
        if doc.status() != StatusCode::OK
            || !is_html(doc.headers())
            || doc.headers().contains_key(header::CONTENT_ENCODING)
        {
            // Not a document that can be rewritten. An empty answer about a file that turns out
            // not to be HTML (a script that did not change) stays what it was.
            return match original {
                Some(o) if empty && !is_html(doc.headers()) => o,
                _ => for_method(no_cache(doc), &asked),
            };
        }
        let declared = doc
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());
        if declared.is_some_and(|n| n > self.limit as u64) {
            // Too large to be a shell: never rewritten, so the answer to the request as it was
            // asked (`304`, `206`, `HEAD` or the file itself, streamed) is the right one.
            return match original {
                Some(o) => no_cache(o),
                None => for_method(no_cache(doc), &asked),
            };
        }
        let (mut parts, body) = doc.into_parts();
        let bytes = match read_limited(body, self.limit).await {
            Ok(b) => b,
            Err(Unread::TooLarge) => {
                // No length was declared and the document is too large to rewrite: send it as it
                // is, from a fresh answer because the first was partly read.
                return for_method(no_cache(self.ask(asked.plain()).await), &asked);
            }
            Err(Unread::Failed(e)) => {
                eprintln!("cannot read the back office shell: {e}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "cannot read the shell")
                    .into_response();
            }
        };
        let bytes = match inject(&bytes, language) {
            Some(v) => {
                // The page is not the file any more: the file's validators do not describe it.
                for name in [header::ETAG, header::LAST_MODIFIED, header::ACCEPT_RANGES] {
                    parts.headers.remove(name);
                }
                Bytes::from(v)
            }
            None => bytes,
        };
        parts
            .headers
            .insert(header::CONTENT_LENGTH, HeaderValue::from(bytes.len()));
        parts
            .headers
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
        for_method(Response::from_parts(parts, Body::from(bytes)), &asked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    const PAGE: &str = "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>obop</title></head><body><div id=\"root\"></div></body></html>";

    #[test]
    fn the_setting_takes_a_shipped_language_and_nothing_else() {
        for (raw, want) in [
            ("sv", "sv"),
            ("SV", "sv"),
            (" fi ", "fi"),
            ("sv-SE", "sv"),
            ("de-AT", "de"),
            ("en", "en"),
            ("es", "es"),
            ("fr", "fr"),
            ("pl", "pl"),
        ] {
            assert_eq!(parse(raw), Ok(Some(want)), "{raw:?}");
        }
        for empty in ["", "   ", "\t"] {
            assert_eq!(parse(empty), Ok(None), "{empty:?}");
        }
        for bad in [
            "xx",
            "ja",
            "pt-BR",
            "swedish",
            "sv_SE",
            "sv-",
            "-sv",
            "sv--SE",
            "s v",
            "sv,fi",
            "\"sv\"",
            "sv\">",
            "en-XA-toolongsubtag",
            "své",
        ] {
            let e = parse(bad).expect_err(bad);
            assert!(e.starts_with("OBOP_DEFAULT_LANGUAGE is "), "{bad:?}: {e}");
            assert!(e.contains("en, de, fr, pl, fi, sv, es"), "{bad:?}: {e}");
        }
    }

    #[test]
    fn the_shipped_languages_are_the_interface_languages() {
        assert_eq!(LANGS, ["en", "de", "fr", "pl", "fi", "sv", "es"]);
    }

    #[test]
    fn the_tag_is_added_once_as_the_first_thing_in_the_head() {
        let out = String::from_utf8(inject(PAGE.as_bytes(), "sv").unwrap()).unwrap();
        let tag = "<meta name=\"obop-default-language\" content=\"sv\">";
        assert_eq!(out.matches(tag).count(), 1, "{out}");
        assert_eq!(out.matches("obop-default-language").count(), 1);
        assert_eq!(out, PAGE.replacen("<head>", &format!("<head>{tag}"), 1));
    }

    #[test]
    fn the_value_is_escaped_whatever_it_is() {
        let out =
            String::from_utf8(inject(PAGE.as_bytes(), "\"><script>x()</script>").unwrap()).unwrap();
        assert!(
            out.contains("content=\"&quot;&gt;&lt;script&gt;x()&lt;/script&gt;\">"),
            "{out}"
        );
        assert!(!out.contains("<script>"));
        assert_eq!(
            meta_tag("a&b'c"),
            "<meta name=\"obop-default-language\" content=\"a&amp;b&#39;c\">"
        );
    }

    #[test]
    fn it_finds_the_real_head_and_only_that() {
        let tag = "<meta name=\"obop-default-language\" content=\"fi\">";
        // Upper case, attributes (with a `>` inside a value), a comment that mentions a head.
        let page = "<!-- <head> --><HTML><HEAD data-x=\"a>b\" class='c>d'><title>t</title></HEAD><body></body></HTML>";
        let out = String::from_utf8(inject(page.as_bytes(), "fi").unwrap()).unwrap();
        assert_eq!(
            out,
            page.replacen(
                "<HEAD data-x=\"a>b\" class='c>d'>",
                &format!("<HEAD data-x=\"a>b\" class='c>d'>{tag}"),
                1
            )
        );
        // `<header>` is not `<head>`, and a document with no head is left alone.
        assert_eq!(
            inject(b"<html><body><header>x</header></body></html>", "fi"),
            None
        );
        assert_eq!(
            inject(b"<html><body></body><head></head></html>", "fi"),
            None
        );
        assert_eq!(inject(b"", "fi"), None);
        assert_eq!(inject(b"<head", "fi"), None);
        assert_eq!(inject(b"<!-- unterminated <head>", "fi"), None);
        // A document that already has the tag keeps its own: never two.
        let has = format!("<html><head>{tag}</head></html>");
        assert_eq!(inject(has.as_bytes(), "sv"), None);
        let has_upper = has.to_uppercase();
        assert_eq!(inject(has_upper.as_bytes(), "sv"), None);
    }

    #[test]
    fn bytes_that_are_not_utf8_pass_through_unchanged_around_the_tag() {
        let mut page = b"<html><head></head><body>".to_vec();
        page.extend_from_slice(&[0xff, 0xfe, 0x80]);
        page.extend_from_slice(b"</body></html>");
        let out = inject(&page, "de").unwrap();
        assert!(out.ends_with(&[
            0xff, 0xfe, 0x80, b'<', b'/', b'b', b'o', b'd', b'y', b'>', b'<', b'/', b'h', b't',
            b'm', b'l', b'>'
        ]));
    }

    /// A build directory that is removed when the guard goes out of scope.
    struct Build(std::path::PathBuf);
    impl Build {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("obop-shell-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("assets")).unwrap();
            std::fs::write(dir.join("index.html"), PAGE).unwrap();
            std::fs::write(dir.join("assets/app.js"), "console.log('<head>')").unwrap();
            std::fs::write(dir.join("assets/app.css"), "body{margin:0}").unwrap();
            std::fs::write(
                dir.join("assets/doc.pdf"),
                "%PDF-1.4 a document that is not text/html",
            )
            .unwrap();
            Build(dir)
        }
        fn path(&self) -> &str {
            self.0.to_str().unwrap()
        }
    }
    impl Drop for Build {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    async fn get(
        shell: &Shell,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
    ) -> (StatusCode, HeaderMap, Vec<u8>) {
        let mut req = Request::builder().method(method).uri(path);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let resp = shell.serve(req.body(Body::empty()).unwrap()).await;
        let (parts, body) = resp.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes().to_vec();
        (parts.status, parts.headers, bytes)
    }

    #[tokio::test]
    async fn the_shell_carries_the_default_language_at_every_address_that_serves_it() {
        let build = Build::new("served");
        let shell = Shell::new(build.path(), Some("sv"));
        let tag = "<meta name=\"obop-default-language\" content=\"sv\">";
        // The root, the file itself, and a client-side route that is not a file.
        for path in ["/", "/index.html", "/customize", "/crm/lead/42"] {
            let (status, headers, body) = get(&shell, "GET", path, &[]).await;
            let text = String::from_utf8(body).unwrap();
            assert_eq!(status, StatusCode::OK, "{path}");
            assert_eq!(text.matches(tag).count(), 1, "{path}: {text}");
            assert_eq!(text.matches("obop-default-language").count(), 1, "{path}");
            assert_eq!(headers[header::CACHE_CONTROL], "no-cache", "{path}");
            assert_eq!(
                headers[header::CONTENT_LENGTH].to_str().unwrap(),
                text.len().to_string(),
                "{path}"
            );
            assert!(
                headers.get(header::ETAG).is_none() && headers.get(header::LAST_MODIFIED).is_none(),
                "{path}"
            );
            assert!(
                headers[header::CONTENT_TYPE]
                    .to_str()
                    .unwrap()
                    .starts_with("text/html")
            );
        }
    }

    #[tokio::test]
    async fn without_a_default_the_document_is_the_file_byte_for_byte() {
        let build = Build::new("plain");
        let shell = Shell::new(build.path(), None);
        for path in ["/", "/index.html", "/customize"] {
            let (status, headers, body) = get(&shell, "GET", path, &[]).await;
            assert_eq!(status, StatusCode::OK, "{path}");
            assert_eq!(body, PAGE.as_bytes(), "{path}");
            assert_eq!(headers[header::CACHE_CONTROL], "no-cache", "{path}");
        }
    }

    #[tokio::test]
    async fn files_that_are_not_html_are_not_touched() {
        let build = Build::new("files");
        for language in [None, Some("sv")] {
            let shell = Shell::new(build.path(), language);
            let (status, headers, body) = get(&shell, "GET", "/assets/app.js", &[]).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(body, b"console.log('<head>')");
            assert!(headers.get(header::CACHE_CONTROL).is_none());
            let (_, _, css) = get(&shell, "GET", "/assets/app.css", &[]).await;
            assert_eq!(css, b"body{margin:0}");
        }
    }

    #[tokio::test]
    async fn a_validator_from_before_the_setting_cannot_keep_the_old_page() {
        let build = Build::new("validators");
        // The browser has the plain shell, with its validators, from a server without a default.
        let plain = Shell::new(build.path(), None);
        let (_, first, _) = get(&plain, "GET", "/", &[]).await;
        let etag = first
            .get(header::ETAG)
            .map(|v| v.to_str().unwrap().to_string());
        let modified = first[header::LAST_MODIFIED].to_str().unwrap().to_string();
        // The server restarts with a default: the same request must get the tagged page.
        let shell = Shell::new(build.path(), Some("sv"));
        let mut cond = vec![("if-modified-since", modified.as_str())];
        if let Some(e) = etag.as_deref() {
            cond.push(("if-none-match", e));
        }
        let (status, _, body) = get(&shell, "GET", "/", &cond).await;
        assert_eq!(status, StatusCode::OK);
        assert!(String::from_utf8(body).unwrap().contains("content=\"sv\""));
        // And a range request gets the whole document, not a slice with the tag cut off.
        let (status, _, body) = get(&shell, "GET", "/", &[("range", "bytes=0-9")]).await;
        assert_eq!(status, StatusCode::OK);
        assert!(String::from_utf8(body).unwrap().contains("content=\"sv\""));
    }

    #[tokio::test]
    async fn a_head_request_reports_the_length_of_the_document_it_would_send() {
        let build = Build::new("head");
        let shell = Shell::new(build.path(), Some("sv"));
        let (_, _, full) = get(&shell, "GET", "/", &[]).await;
        let (status, headers, body) = get(&shell, "HEAD", "/", &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.is_empty());
        assert_eq!(
            headers[header::CONTENT_LENGTH].to_str().unwrap(),
            full.len().to_string()
        );
    }

    #[tokio::test]
    async fn a_shell_that_cannot_take_the_tag_is_served_as_it_is() {
        let build = Build::new("nohead");
        std::fs::write(build.0.join("index.html"), "<html><body>x</body></html>").unwrap();
        let shell = Shell::new(build.path(), Some("sv"));
        let (status, headers, body) = get(&shell, "GET", "/", &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, b"<html><body>x</body></html>");
        assert_eq!(headers[header::CACHE_CONTROL], "no-cache");
        // A missing file is the server's 404, not an empty page.
        let gone = Build::new("gone");
        std::fs::remove_file(gone.0.join("index.html")).unwrap();
        let shell = Shell::new(gone.path(), Some("sv"));
        let (status, _, _) = get(&shell, "GET", "/", &[]).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    /// The request headers of a set of conditions a browser sends, with the answer each should get
    /// from a file service: `(name, headers)`.
    fn conditions(modified: &str) -> Vec<(&'static str, Vec<(&'static str, &str)>)> {
        vec![
            ("plain", vec![]),
            ("fresh copy", vec![("if-modified-since", modified)]),
            (
                "stale copy",
                vec![("if-modified-since", "Mon, 01 Jan 1990 00:00:00 GMT")],
            ),
            ("etag", vec![("if-none-match", "\"abc\"")]),
            ("etag star", vec![("if-none-match", "*")]),
            (
                "unmodified since long ago",
                vec![("if-unmodified-since", "Mon, 01 Jan 1990 00:00:00 GMT")],
            ),
            ("range", vec![("range", "bytes=0-9")]),
            (
                "range beyond the end",
                vec![("range", "bytes=99999-999999")],
            ),
            (
                "range kept only if unchanged",
                vec![
                    ("range", "bytes=0-9"),
                    ("if-range", "Mon, 01 Jan 1990 00:00:00 GMT"),
                ],
            ),
        ]
    }

    /// The `Last-Modified` the file service gives a path, as a browser would store it.
    async fn modified_of(dir: &str, path: &str) -> String {
        let plain = Shell::new(dir, None);
        let (status, headers, _) = get(&plain, "GET", path, &[]).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        headers[header::LAST_MODIFIED].to_str().unwrap().to_string()
    }

    /// Every header of an answer, sorted, as text: the answer compared as a whole.
    fn shown(headers: &HeaderMap) -> Vec<String> {
        let mut all: Vec<String> = headers
            .iter()
            .map(|(k, v)| format!("{k}: {}", v.to_str().unwrap_or("?")))
            .collect();
        all.sort();
        all
    }

    #[tokio::test]
    async fn a_file_that_is_not_html_is_answered_exactly_as_without_a_default_language() {
        let build = Build::new("assets");
        let without = Shell::new(build.path(), None);
        let with = Shell::new(build.path(), Some("sv"));
        for path in ["/assets/app.js", "/assets/app.css", "/assets/doc.pdf"] {
            let modified = modified_of(build.path(), path).await;
            for method in ["GET", "HEAD"] {
                for (what, headers) in conditions(&modified) {
                    let a = get(&without, method, path, &headers).await;
                    let b = get(&with, method, path, &headers).await;
                    let context = format!("{method} {path} ({what})");
                    assert_eq!(a.0, b.0, "status of {context}");
                    assert_eq!(shown(&a.1), shown(&b.1), "headers of {context}");
                    assert_eq!(a.2, b.2, "body of {context}");
                }
            }
        }
    }

    #[tokio::test]
    async fn a_file_that_is_not_html_keeps_its_304_206_412_416_and_head_with_a_default_language() {
        let build = Build::new("revalidate");
        let shell = Shell::new(build.path(), Some("sv"));
        let path = "/assets/app.js";
        let file = "console.log('<head>')";
        let modified = modified_of(build.path(), path).await;
        // A copy that is still current: 304 and nothing else.
        let (status, _, body) = get(
            &shell,
            "GET",
            path,
            &[("if-modified-since", modified.as_str())],
        )
        .await;
        assert_eq!((status, body.len()), (StatusCode::NOT_MODIFIED, 0));
        // A copy older than the file: the file.
        let (status, headers, body) = get(
            &shell,
            "GET",
            path,
            &[("if-modified-since", "Mon, 01 Jan 1990 00:00:00 GMT")],
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, file.as_bytes());
        assert_eq!(headers[header::LAST_MODIFIED].to_str().unwrap(), modified);
        // A range: 206 with its Content-Range and exactly those bytes.
        let (status, headers, body) = get(&shell, "GET", path, &[("range", "bytes=0-9")]).await;
        assert_eq!(status, StatusCode::PARTIAL_CONTENT);
        assert_eq!(body, &file.as_bytes()[..10]);
        assert_eq!(
            headers[header::CONTENT_RANGE].to_str().unwrap(),
            format!("bytes 0-9/{}", file.len())
        );
        // A range beyond the end and a precondition that fails keep their statuses.
        let (status, _, _) = get(&shell, "GET", path, &[("range", "bytes=999-9999")]).await;
        assert_eq!(status, StatusCode::RANGE_NOT_SATISFIABLE);
        let (status, _, _) = get(
            &shell,
            "GET",
            path,
            &[("if-unmodified-since", "Mon, 01 Jan 1990 00:00:00 GMT")],
        )
        .await;
        assert_eq!(status, StatusCode::PRECONDITION_FAILED);
        // HEAD: the headers of the file, its length, no body, and its validators.
        let (status, headers, body) = get(&shell, "HEAD", path, &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.is_empty());
        assert_eq!(
            headers[header::CONTENT_LENGTH].to_str().unwrap(),
            file.len().to_string()
        );
        assert_eq!(headers[header::LAST_MODIFIED].to_str().unwrap(), modified);
        assert!(headers.get(header::CACHE_CONTROL).is_none());
    }

    #[tokio::test]
    async fn the_shell_is_never_answered_with_a_304_or_a_206_that_lacks_the_tag() {
        let build = Build::new("never");
        let shell = Shell::new(build.path(), Some("sv"));
        let modified = modified_of(build.path(), "/").await;
        let tag = "<meta name=\"obop-default-language\" content=\"sv\">";
        // The shell at every address that serves it, including a missing script that falls back
        // to it, with every condition a browser could send.
        for path in [
            "/",
            "/index.html",
            "/customize",
            "/crm/lead/42",
            "/assets/gone.js",
        ] {
            let (_, _, full) = get(&shell, "GET", path, &[]).await;
            assert!(String::from_utf8(full.clone()).unwrap().contains(tag));
            for (what, headers) in conditions(&modified) {
                let context = format!("{path} ({what})");
                let (status, answer, body) = get(&shell, "GET", path, &headers).await;
                assert_eq!(status, StatusCode::OK, "{context}");
                assert_eq!(body, full, "{context}");
                assert_eq!(String::from_utf8_lossy(&body).matches(tag).count(), 1);
                assert_eq!(answer[header::CACHE_CONTROL], "no-cache", "{context}");
                assert!(answer.get(header::CONTENT_RANGE).is_none(), "{context}");
                assert!(answer.get(header::ETAG).is_none(), "{context}");
                assert!(answer.get(header::LAST_MODIFIED).is_none(), "{context}");
                // And a HEAD with the same conditions reports that document's length.
                let (status, answer, body) = get(&shell, "HEAD", path, &headers).await;
                assert_eq!(status, StatusCode::OK, "HEAD {context}");
                assert!(body.is_empty(), "HEAD {context}");
                assert_eq!(
                    answer[header::CONTENT_LENGTH].to_str().unwrap(),
                    full.len().to_string(),
                    "HEAD {context}"
                );
            }
        }
    }

    #[tokio::test]
    async fn an_html_file_over_the_limit_is_served_as_it_is_and_never_a_500() {
        let build = Build::new("big");
        let big = format!(
            "<html><head><title>big</title></head><body>{}</body></html>",
            "x".repeat(5 * 1024 * 1024)
        );
        std::fs::write(build.0.join("big.html"), &big).unwrap();
        let shell = Shell::new(build.path(), Some("sv"));
        let modified = modified_of(build.path(), "/big.html").await;
        // The whole file, untouched: no tag, the file's own length and validators.
        let (status, headers, body) = get(&shell, "GET", "/big.html", &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body == big.as_bytes(), "the document is served unchanged");
        assert!(!String::from_utf8_lossy(&body).contains("obop-default-language"));
        assert_eq!(headers[header::CACHE_CONTROL], "no-cache");
        assert_eq!(
            headers[header::CONTENT_LENGTH].to_str().unwrap(),
            big.len().to_string()
        );
        // Being unchanged, it keeps its 304, its 206 and its HEAD.
        let (status, _, body) = get(
            &shell,
            "GET",
            "/big.html",
            &[("if-modified-since", modified.as_str())],
        )
        .await;
        assert_eq!((status, body.len()), (StatusCode::NOT_MODIFIED, 0));
        let (status, _, body) = get(&shell, "GET", "/big.html", &[("range", "bytes=0-9")]).await;
        assert_eq!(status, StatusCode::PARTIAL_CONTENT);
        assert_eq!(body, &big.as_bytes()[..10]);
        let (status, headers, body) = get(&shell, "HEAD", "/big.html", &[]).await;
        assert_eq!((status, body.len()), (StatusCode::OK, 0));
        assert_eq!(
            headers[header::CONTENT_LENGTH].to_str().unwrap(),
            big.len().to_string()
        );
    }

    #[tokio::test]
    async fn a_body_is_read_up_to_the_limit_and_no_further() {
        use http_body_util::Full;
        let body = |n: usize| Body::new(Full::new(Bytes::from(vec![b'x'; n])));
        assert_eq!(read_limited(body(8), 8).await.unwrap().len(), 8);
        assert!(matches!(
            read_limited(body(9), 8).await,
            Err(Unread::TooLarge)
        ));
        assert_eq!(read_limited(body(0), 8).await.unwrap().len(), 0);
    }
}
