//! The back office shell (`index.html` of the Vite+ build), the instance default language and the
//! instance brand.
//!
//! The sign-in, two-factor, enrolment and forced-password screens are shown before anyone is
//! signed in, so before the instance settings can be read. `OBOP_DEFAULT_LANGUAGE` names the
//! language of the instance for them (D52 amendment). When it is set, the shell is served with
//! `<meta name="obop-default-language" content="sv">` added to its `<head>` at request time, and
//! the page reads it before its first render. The meta tag carries a language code and nothing
//! else.
//!
//! **The brand** (docs/spec/branding.md, section 4, D52 amendment of section 15.1). When the
//! instance has a brand row, the shell is also rewritten with it: the first `<title>` becomes the
//! name, every `<link rel="icon">` (whatever its `href`: the build prefixes the base, so under
//! `/admin/` it reads `/admin/favicon.svg`) becomes the one link to the logo route with the logo's
//! version, and `<meta name="obop-brand" content="…">` is added, one JSON object with the `name`,
//! the effective `slug`, the non-empty `messages`, the logo's version `v` and the theme `accent`
//! when it is a valid `#rrggbb`, and nothing else (no titles, no switch). These are public by
//! design. The page reads the tag before its first render, as it reads the language.
//!
//! The brand comes from the reader of the dispatcher, asked through `blocking` (the PostgreSQL
//! client panics on an async worker), and only when the answer is the HTML shell itself: a script,
//! a style sheet or a picture never asks it. While the store cannot be read the request that ran
//! the failing refill is answered 503 by D74 (`ops::storage_faults`), also for an instance with no
//! row, where the index used to be served from the file; a request that arrives while a refill
//! runs gets the last good brand. With no row and no default language the answer is the file
//! service's answer, byte for byte.
//!
//! The shell is the one document that must never be reused without asking the server: its
//! content depends on the build on disk, on this setting and on the brand row, so every HTML
//! answer of the service says `Cache-Control: no-cache`.
//!
//! Only the shell is touched. A request is first answered exactly as `ServeDir` answers it, and an
//! answer for a file that is not HTML (a script, a style sheet, a font, a picture, a PDF) goes out
//! byte for byte: `304`, `206`, `HEAD` and `Last-Modified` as they are, with or without a default
//! language. Only when the answer is HTML, or is one of the empty answers that cannot be told from
//! it (`304` and `412` carry no `Content-Type`; a `416` of the file service carries the one of the
//! file, so it is told by it like any other answer), the document itself is asked for again
//! without the request's validators and range. That is what keeps a browser from holding on to a
//! copy of the page from before the setting existed: a `304` or a `206` for the shell would keep
//! it without the tag. The answer is then a `200` with the tag and without `ETag` and
//! `Last-Modified`.

use super::{HttpState, blocking};
use axum::{
    body::{Body, Bytes},
    extract::Request,
    http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, Version, header},
    response::{IntoResponse, Response},
};
use http_body_util::{BodyExt, LengthLimitError, Limited};
use obop_brand::logo::logo_url;
use obop_commands::brand::{Reader, Shown};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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

// ---- The brand ---------------------------------------------------------------------------------

/// The name of the meta tag that carries the public subset of the brand.
pub const BRAND_META_NAME: &str = "obop-brand";

/// An opening or closing tag found in the head: where it starts, where it ends (after its `>`), its
/// lower-case name and whether it is a closing tag.
#[derive(Debug)]
struct Tag {
    start: usize,
    end: usize,
    name: String,
    closing: bool,
}

/// The end of the tag whose name ends at `from`: just after the first `>` outside a quoted
/// attribute value.
fn tag_end(html: &[u8], from: usize) -> Option<usize> {
    let mut quote: Option<u8> = None;
    for (k, b) in html.get(from..)?.iter().enumerate() {
        match (quote, *b) {
            (None, b'"' | b'\'') => quote = Some(*b),
            (Some(q), b) if b == q => quote = None,
            (None, b'>') => return Some(from + k + 1),
            _ => {}
        }
    }
    None
}

/// Where `needle` starts in `html[from..]`, ignoring ASCII case.
fn find_ignore_case(html: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    (from..html.len()).find(|i| starts_with_ignore_case(html, *i, needle))
}

/// The tags of `html[from..to]` in document order. Comments, declarations and the text of `script`,
/// `style`, `textarea` and `title` (which is not markup) are skipped, so a `<link` or a `<title`
/// inside them is not found. A tag that never ends stops the scan.
fn scan_tags(html: &[u8], from: usize, to: usize) -> Vec<Tag> {
    let html = &html[..to.min(html.len())];
    let mut tags = Vec::new();
    let mut i = from;
    while let Some(off) = html
        .get(i..)
        .and_then(|h| h.iter().position(|b| *b == b'<'))
    {
        i += off;
        if html[i..].starts_with(b"<!--") {
            match html[i + 4..].windows(3).position(|w| w == b"-->") {
                Some(end) => i += 4 + end + 3,
                None => break,
            }
            continue;
        }
        let closing = html.get(i + 1) == Some(&b'/');
        let name_start = i + 1 + usize::from(closing);
        let name_len = html[name_start.min(html.len())..]
            .iter()
            .take_while(|b| b.is_ascii_alphanumeric() || **b == b'-')
            .count();
        if name_len == 0 || !html[name_start].is_ascii_alphabetic() {
            // `<!doctype>`, `<?…>` or a stray `<`.
            i += 1;
            continue;
        }
        let name =
            String::from_utf8_lossy(&html[name_start..name_start + name_len]).to_ascii_lowercase();
        let Some(end) = tag_end(html, name_start + name_len) else {
            break;
        };
        let raw_text =
            !closing && matches!(name.as_str(), "script" | "style" | "textarea" | "title");
        tags.push(Tag {
            start: i,
            end,
            name: name.clone(),
            closing,
        });
        i = end;
        if raw_text {
            let close = format!("</{name}");
            i = find_ignore_case(html, i, close.as_bytes()).unwrap_or(html.len());
        }
    }
    tags
}

/// The attributes of an opening tag as `(lower-case name, value)`, values unquoted and not
/// unescaped (only `rel` is looked at, and a character reference in it names no icon).
fn attributes(html: &[u8], tag: &Tag) -> Vec<(String, String)> {
    let body = &html[tag.start + 1 + tag.name.len()..tag.end.saturating_sub(1)];
    let mut out = Vec::new();
    let mut i = 0;
    while i < body.len() {
        while i < body.len() && (body[i].is_ascii_whitespace() || body[i] == b'/') {
            i += 1;
        }
        let from = i;
        while i < body.len() && !body[i].is_ascii_whitespace() && body[i] != b'=' && body[i] != b'/'
        {
            i += 1;
        }
        if from == i {
            break;
        }
        let name = String::from_utf8_lossy(&body[from..i]).to_ascii_lowercase();
        while i < body.len() && body[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if body.get(i) == Some(&b'=') {
            i += 1;
            while i < body.len() && body[i].is_ascii_whitespace() {
                i += 1;
            }
            match body.get(i) {
                Some(&q) if q == b'"' || q == b'\'' => {
                    let start = i + 1;
                    let len = body[start..].iter().position(|b| *b == q);
                    let end = len.map_or(body.len(), |l| start + l);
                    value = String::from_utf8_lossy(&body[start..end]).into_owned();
                    i = (end + 1).min(body.len());
                }
                _ => {
                    let start = i;
                    while i < body.len() && !body[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    value = String::from_utf8_lossy(&body[start..i]).into_owned();
                }
            }
        }
        out.push((name, value));
    }
    out
}

/// Whether a `<link>` is a tab icon: its `rel` has the token `icon` (`icon`, `shortcut icon`), in
/// any case. `apple-touch-icon` and `mask-icon` are other tokens.
fn is_icon_link(html: &[u8], tag: &Tag) -> bool {
    tag.name == "link"
        && !tag.closing
        && attributes(html, tag).iter().any(|(k, v)| {
            k == "rel"
                && v.split_ascii_whitespace()
                    .any(|t| t.eq_ignore_ascii_case("icon"))
        })
}

/// A JSON string, as `serde_json` writes it.
fn json_text(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

/// What a branded shell carries of the brand, and nothing else: the public subset of the record.
struct Public {
    name: String,
    slug: String,
    messages: Vec<(&'static str, String)>,
    v: String,
    accent: Option<String>,
}

impl Public {
    fn of(shown: &Shown) -> Public {
        let brand = shown.brand();
        Public {
            name: brand.name().to_string(),
            slug: brand.slug(),
            messages: brand
                .messages()
                .map(|(lang, text)| (lang, text.to_string()))
                .collect(),
            v: shown.v().to_string(),
            accent: shown.theme_accent().map(str::to_string),
        }
    }

    /// The value of the `obop-brand` tag: one JSON object with the `name`, the effective `slug`,
    /// the non-empty `messages`, the logo's version `v` and, only when the theme stores a valid
    /// `#rrggbb`, the `accent`. No titles, no switch and no other key.
    fn json(&self) -> String {
        let messages: Vec<String> = self
            .messages
            .iter()
            .map(|(lang, text)| format!("{}:{}", json_text(lang), json_text(text)))
            .collect();
        let mut out = format!(
            "{{\"name\":{},\"slug\":{},\"messages\":{{{}}},\"v\":{}",
            json_text(&self.name),
            json_text(&self.slug),
            messages.join(","),
            json_text(&self.v),
        );
        if let Some(accent) = &self.accent {
            out.push_str(&format!(",\"accent\":{}", json_text(accent)));
        }
        out.push('}');
        out
    }

    /// The meta tag of the brand, attribute-escaped.
    fn meta_tag(&self) -> String {
        format!(
            "<meta name=\"{BRAND_META_NAME}\" content=\"{}\">",
            attribute(&self.json())
        )
    }

    /// The one icon link of a branded shell.
    fn icon_link(&self) -> String {
        format!(
            "<link rel=\"icon\" href=\"{}\" type=\"image/svg+xml\">",
            attribute(&logo_url(&self.v))
        )
    }
}

/// Whether a `<meta>` declares the character encoding of the document: a `charset` attribute, or
/// `http-equiv="content-type"` (an ASCII case-insensitive match of the whole value, as the HTML
/// standard has it: a value with a space around it declares nothing).
fn is_charset_meta(html: &[u8], tag: &Tag) -> bool {
    tag.name == "meta"
        && !tag.closing
        && attributes(html, tag).iter().any(|(k, v)| {
            k == "charset" || (k == "http-equiv" && v.eq_ignore_ascii_case("content-type"))
        })
}

/// Whether a `<meta>` is the tag of the brand: its `name` is `obop-brand`, in any case (the page
/// reads it in lower case only, so refusing the other cases is the safe side).
fn is_brand_meta(html: &[u8], tag: &Tag) -> bool {
    tag.name == "meta"
        && !tag.closing
        && attributes(html, tag)
            .iter()
            .any(|(k, v)| k == "name" && v.eq_ignore_ascii_case(BRAND_META_NAME))
}

/// `html` with `edits` made: each is `(from, to, what replaces html[from..to])`, in any order but
/// not overlapping (an insertion has `from == to`). `Err` for edits that overlap, which the scan
/// of a head that is read correctly never makes.
fn apply_edits(
    html: &[u8],
    mut edits: Vec<(usize, usize, String)>,
) -> Result<Vec<u8>, &'static str> {
    edits.sort_by_key(|e| (e.0, e.1));
    let mut out = Vec::with_capacity(html.len() + 512);
    let mut at = 0;
    for (from, to, text) in edits {
        if from < at {
            return Err("its head cannot be read");
        }
        out.extend_from_slice(&html[at..from]);
        out.extend_from_slice(text.as_bytes());
        at = to;
    }
    out.extend_from_slice(&html[at..]);
    Ok(out)
}

/// `html` with the brand: the first `<title>` replaced by the escaped name, every
/// `<link rel="icon">` of the head replaced by the one link to the logo (the first in place, the
/// others dropped) and the `obop-brand` tag added to the head, right after the tag that declares
/// the character encoding (so that this declaration stays within the first 1024 bytes of the
/// document whatever the brand holds; as the first thing in the head when there is none). Only
/// the head is read and nothing else is touched. `Err` names, in a fixed phrase, why the document
/// cannot take it (no `<head>`, no `<title>`, a `<title>` that is not closed, no icon link, a head
/// that already has a `<meta>` named `obop-brand`, or a head that cannot be read); the caller then
/// serves it as it is.
fn brand_rewrite(html: &[u8], shown: &Shown) -> Result<Vec<u8>, &'static str> {
    let public = Public::of(shown);
    let head = head_content_start(html).ok_or("it has no <head>")?;
    let head_end = find_ignore_case(html, head, b"</head").unwrap_or(html.len());
    let tags = scan_tags(html, head, head_end);
    // A tag that is there already, not the words: a comment or a script that mentions the name
    // (a build that inlines the app has the literal in it) is not one.
    if tags.iter().any(|t| is_brand_meta(html, t)) {
        return Err("it already has an obop-brand tag");
    }
    let title = tags
        .iter()
        .find(|t| t.name == "title" && !t.closing)
        .ok_or("it has no <title>")?;
    let title_close =
        find_ignore_case(html, title.end, b"</title").ok_or("its <title> is not closed")?;
    let title_end = tag_end(html, title_close + 7).ok_or("its <title> is not closed")?;
    let icons: Vec<&Tag> = tags.iter().filter(|t| is_icon_link(html, t)).collect();
    if icons.is_empty() {
        return Err("it has no icon link");
    }
    // After the declaration of the encoding when the head has one, else first in the head.
    let tag_at = tags
        .iter()
        .find(|t| is_charset_meta(html, t))
        .map_or(head, |t| t.end);
    // The edits: (from, to, what replaces html[from..to]).
    let mut edits: Vec<(usize, usize, String)> = vec![
        (tag_at, tag_at, public.meta_tag()),
        (
            title.start,
            title_end,
            format!("<title>{}</title>", attribute(&public.name)),
        ),
    ];
    for (n, icon) in icons.iter().enumerate() {
        edits.push((
            icon.start,
            icon.end,
            if n == 0 {
                public.icon_link()
            } else {
                String::new()
            },
        ));
    }
    apply_edits(html, edits)
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

/// The headers of a file that stop describing the answer once the document in it is rewritten.
/// The file service sends `Last-Modified` and `Accept-Ranges` and no `ETag` today; the three are
/// named so that one that starts to send an `ETag` cannot make a cached copy of the old page valid.
const FILE_VALIDATORS: [header::HeaderName; 3] =
    [header::ETAG, header::LAST_MODIFIED, header::ACCEPT_RANGES];

fn forget_file_validators(headers: &mut HeaderMap) {
    for name in FILE_VALIDATORS {
        headers.remove(name);
    }
}

/// A `HEAD` request gets the headers of the answer to the `GET` and no body.
fn for_method(resp: Response, asked: &Asked) -> Response {
    if !asked.is_head() {
        return resp;
    }
    let (parts, _) = resp.into_parts();
    Response::from_parts(parts, Body::empty())
}

/// Where the one line about a shell that cannot take the brand is written. The default writes to
/// standard error; a test injects a sink and counts the lines.
pub type Sink = Arc<dyn Fn(&str) + Send + Sync>;

/// Serves the files of a back office build, the shell for every path that is not a file, and the
/// shell with the default language and the instance brand when there are any.
#[derive(Clone)]
pub struct Shell {
    files: ServeDir<ServeFile>,
    language: Option<&'static str>,
    /// The largest document that is rewritten.
    limit: usize,
    /// The reader of the instance brand. `None`: this shell has no store behind it (a test rig, or
    /// a dispatcher that never ran `register_admin`), and no brand is ever applied.
    brand: Option<Arc<Reader>>,
    /// Whether the line about a shell that cannot take the brand was written (once per shell, and a
    /// process has one).
    noted: Arc<AtomicBool>,
    sink: Sink,
}

impl Shell {
    /// Files from `dir`; `language` (from [`parse`] or [`from_env`]) is added to the shell. This
    /// shell reads no brand; the server builds its shell with `Shell::for_state`.
    pub fn new(dir: &str, language: Option<&'static str>) -> Self {
        Self::build(dir, language, None)
    }

    /// As [`Shell::new`], with the reader of the instance brand: a shell that is rewritten with the
    /// brand when a row exists. An inert reader (no store) is no reader.
    pub fn with_brand(dir: &str, language: Option<&'static str>, reader: Arc<Reader>) -> Self {
        Self::build(dir, language, Some(reader).filter(|r| !r.is_inert()))
    }

    /// The shell of a server: the default language of the state and the reader of its dispatcher,
    /// so every place that mounts the back office reads the same brand.
    pub(crate) fn for_state(dir: &str, state: &HttpState) -> Self {
        Self::with_brand(dir, state.default_language(), state.dispatcher.brand())
    }

    /// The same shell writing its one line about a shell that cannot take the brand to `sink`.
    pub fn with_sink(mut self, sink: Sink) -> Self {
        self.sink = sink;
        self
    }

    fn build(dir: &str, language: Option<&'static str>, brand: Option<Arc<Reader>>) -> Self {
        if language.is_some() {
            Self::check_shell(dir);
        }
        Shell {
            files: ServeDir::new(dir).fallback(ServeFile::new(format!("{dir}/index.html"))),
            language,
            limit: MAX_SHELL_BYTES,
            brand,
            noted: Arc::new(AtomicBool::new(false)),
            // Not `eprintln!`, which panics when standard error cannot be written.
            sink: Arc::new(|line| {
                use std::io::Write;
                let _ = writeln!(std::io::stderr(), "{line}");
            }),
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

    /// The brand for display, asked on a blocking thread: the cache-hit path of the reader takes a
    /// standard mutex for a moment, and the one refill that reads the store runs where the store's
    /// client may run. A store fault is noted on that thread and reaches the request (D74).
    async fn shown(&self) -> Option<Arc<Shown>> {
        let reader = self.brand.clone()?;
        Some(blocking(move || reader.current()).await)
    }

    /// Writes the one line about a shell that cannot take a brand that is set, the first time.
    fn cannot_take(&self, why: &str) {
        if !self.noted.swap(true, Ordering::Relaxed) {
            (self.sink)(&format!(
                "obop: the back office shell cannot take the instance brand ({why}): the tab keeps the title and the icon it was built with (obop admin brand status shows the brand)"
            ));
        }
    }

    /// The answer to one request.
    pub async fn serve(&self, req: Request) -> Response {
        if self.language.is_none() && self.brand.is_none() {
            return no_cache(self.ask(req).await);
        }
        let asked = Asked::of(&req);
        let first = self.ask(req).await;
        // `304` and `412` carry no `Content-Type`: the file they speak of may be the shell. (A `416`
        // does, the file's own, so it is told from a script by it and needs no arm here.)
        let empty = matches!(
            first.status(),
            StatusCode::NOT_MODIFIED | StatusCode::PRECONDITION_FAILED
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
            // not to be HTML (a script that did not change) stays what it was; anything else is
            // the answer of the second ask, shaped for the method that was asked. (A document that
            // is not a `200` HTML answer the second time, though the first said it might be one,
            // is a file that changed between the two asks or one with a `Content-Encoding`, which
            // the file service here does not send: no test can reach it without a race, so it has
            // no arm of its own.)
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
        // The shell. Whether a brand is set decides whether it is rewritten when no default
        // language is: with no row and no language the answer is the file service's, as it was.
        let shown = self.shown().await;
        let branded = shown.as_deref().filter(|s| s.is_set());
        if self.language.is_none() && branded.is_none() {
            return no_cache(original.unwrap_or(doc));
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
        let mut page: Option<Vec<u8>> = self.language.and_then(|l| inject(&bytes, l));
        if let Some(shown) = branded {
            match brand_rewrite(page.as_deref().unwrap_or(&bytes), shown) {
                Ok(v) => page = Some(v),
                Err(why) => self.cannot_take(why),
            }
        }
        let bytes = match page {
            Some(v) => {
                // The page is not the file any more: the file's validators do not describe it.
                forget_file_validators(&mut parts.headers);
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

    #[tokio::test]
    async fn a_document_of_exactly_the_limit_is_rewritten_and_one_byte_more_is_not() {
        let build = Build::new("edge");
        let (head, tail) = (
            "<html><head><title>obop</title></head><body>",
            "</body></html>",
        );
        let page = |n: usize| format!("{head}{}{tail}", "x".repeat(n - head.len() - tail.len()));
        let mut shell = Shell::new(build.path(), Some("sv"));
        shell.limit = 1000;
        for (n, tagged) in [(999, true), (1000, true), (1001, false)] {
            std::fs::write(build.0.join("index.html"), page(n)).unwrap();
            let (status, _, body) = get(&shell, "GET", "/", &[]).await;
            assert_eq!(status, StatusCode::OK, "{n} bytes");
            let text = String::from_utf8(body).unwrap();
            assert_eq!(
                text.contains("obop-default-language"),
                tagged,
                "{n} bytes: rewritten or not"
            );
            if !tagged {
                assert_eq!(text, page(n), "{n} bytes: the file as it is");
            }
        }
    }

    // ---- The brand (S2) ---------------------------------------------------------------------------

    use obop_commands::brand::{Action, apply};
    use obop_storage::testing::Flaky;
    use obop_storage::{Backend, Op};
    use std::time::Duration;

    /// A shell as the Vite build writes it: an icon link whose `href` carries the base, a title, a
    /// script and a style sheet.
    const BUILT: &str = "<!doctype html>\n<html lang=\"en\">\n  <head>\n    <meta charset=\"UTF-8\" />\n    <link rel=\"icon\" href=\"/admin/favicon.svg\" type=\"image/svg+xml\" />\n    <title>obop</title>\n    <script type=\"module\" crossorigin src=\"/admin/assets/app.js\"></script>\n    <link rel=\"stylesheet\" crossorigin href=\"/admin/assets/app.css\">\n  </head>\n  <body><div id=\"root\"></div></body>\n</html>\n";

    fn put_theme(backend: &dyn Backend, accent: &str) {
        let value = serde_json::json!({"theme": {"accent": accent}}).to_string();
        let fields = serde_json::json!({ "value": value });
        backend
            .apply(&[Op::Put {
                model: "obop_setting".into(),
                id: "instance".into(),
                fields: fields.as_object().unwrap().clone(),
            }])
            .unwrap();
    }

    /// The brand a row with these fields shows, read through the real reader over a memory store.
    fn shown_of(pairs: &[(&str, &str)], accent: Option<&str>) -> Arc<Shown> {
        let (_flaky, backend) = Flaky::memory();
        if let Some(a) = accent {
            put_theme(&*backend, a);
        }
        let fields = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        apply(&*backend, None, "test", &Action::Configure(fields), false).unwrap();
        Reader::with_bound(backend, Duration::ZERO).current()
    }

    fn rewritten(html: &str, shown: &Shown) -> Result<String, &'static str> {
        brand_rewrite(html.as_bytes(), shown).map(|v| String::from_utf8(v).unwrap())
    }

    fn meta_of(out: &str) -> serde_json::Value {
        let start = out
            .find("<meta name=\"obop-brand\" content=\"")
            .expect("the tag")
            + "<meta name=\"obop-brand\" content=\"".len();
        let end = start + out[start..].find("\">").unwrap();
        let raw = out[start..end]
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&");
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{e}: {raw}"))
    }

    #[test]
    fn a_branded_shell_has_the_name_the_one_icon_and_the_tag_and_nothing_else_changed() {
        let shown = shown_of(&[("name", "MOHV")], None);
        let out = rewritten(BUILT, &shown).unwrap();
        let v = shown.v();
        assert_eq!(v.len(), 16);
        let tag = format!(
            "<meta name=\"obop-brand\" content=\"{{&quot;name&quot;:&quot;MOHV&quot;,&quot;slug&quot;:&quot;mohv&quot;,&quot;messages&quot;:{{}},&quot;v&quot;:&quot;{v}&quot;}}\">"
        );
        let want = BUILT
            .replace(
                "<meta charset=\"UTF-8\" />",
                &format!("<meta charset=\"UTF-8\" />{tag}"),
            )
            .replace(
                "<link rel=\"icon\" href=\"/admin/favicon.svg\" type=\"image/svg+xml\" />",
                &format!(
                    "<link rel=\"icon\" href=\"/api/brand/logo.svg?v={v}\" type=\"image/svg+xml\">"
                ),
            )
            .replace("<title>obop</title>", "<title>MOHV</title>");
        assert_eq!(out, want);
        assert!(!out.contains("favicon.svg"), "the old icon is gone");
        assert!(out.contains("assets/app.css"), "the style sheet is kept");
        assert!(out.contains("<title>MOHV</title>"));
    }

    #[test]
    fn every_icon_link_is_replaced_whatever_its_href_and_attribute_order() {
        let shown = shown_of(&[("name", "MOHV")], None);
        let link = format!(
            "<link rel=\"icon\" href=\"/api/brand/logo.svg?v={}\" type=\"image/svg+xml\">",
            shown.v()
        );
        for icon in [
            "<link rel=\"icon\" href=\"/favicon.svg\" type=\"image/svg+xml\" />",
            "<link rel=\"icon\" href=\"/admin/favicon.svg\" type=\"image/svg+xml\" />",
            "<link href='/somewhere/else.png' rel='icon'>",
            "<LINK REL=\"SHORTCUT ICON\" HREF=/x.ico>",
            "<link rel=icon href=/x.ico>",
            // `rel` is the last attribute and unquoted: the value ends at the `>` of the tag.
            "<link href=/x.ico rel=icon>",
            // A `/` separates attributes as a space does.
            "<link/rel=icon href=/x.ico>",
            "<link\n  rel=\"icon\"\n  href=\"/a>b.svg\"\n  title=\"a > b\">",
            // A `>` inside a value ends no tag, in either kind of quote.
            "<link rel='icon' href='/a>b.svg'>",
            "<link href='/a>b.svg' title='a > b' rel=\"icon\">",
            "<link rel=\"icon\" href=\"/a.png\" sizes=\"32x32\"><link rel=\"icon\" href=\"/b.png\" sizes=\"16x16\">",
        ] {
            let page = format!("<html><head><title>obop</title>{icon}</head><body></body></html>");
            let out = rewritten(&page, &shown).unwrap();
            assert_eq!(out.matches("<link").count(), 1, "{icon}: {out}");
            assert!(out.contains(&link), "{icon}: {out}");
            assert!(
                !out.contains("favicon") && !out.contains("x.ico") && !out.contains("b.svg"),
                "{icon}: {out}"
            );
        }
        // The first icon link is the one that stays, in its place; the others are dropped from
        // theirs, whatever stands between them.
        let page = "<html><head><title>obop</title><link rel=\"icon\" href=\"/a.png\"><link rel=\"stylesheet\" href=\"/s.css\"><link rel=\"icon\" href=\"/b.png\"></head></html>";
        let out = rewritten(page, &shown).unwrap();
        assert!(
            out.contains(&format!(
                "{link}<link rel=\"stylesheet\" href=\"/s.css\"></head>"
            )),
            "{out}"
        );
        assert!(!out.contains(".png"), "{out}");
    }

    #[test]
    fn links_that_are_not_the_tab_icon_and_text_that_only_looks_like_a_tag_are_left_alone() {
        let shown = shown_of(&[("name", "MOHV")], None);
        let kept = [
            "<link rel=\"stylesheet\" href=\"/a.css\">",
            "<link rel=\"apple-touch-icon\" href=\"/touch.png\">",
            "<link rel=\"mask-icon\" href=\"/mask.svg\">",
            "<link rel=\"icons\" href=\"/not-an-icon.svg\">",
            "<link rel=\"preload\" as=\"image\" href=\"/icon.svg\">",
            // Only a `<link>` has a tab icon, and only through its `rel`.
            "<meta rel=\"icon\" content=\"x\">",
            "<link title=\"icon\" rel=\"stylesheet\" href=\"/s.css\">",
            // A tag inside the quoted value of an attribute is no tag, in either kind of quote.
            "<meta name='x' content='a><link rel=\"icon\" href=\"/in-value.svg\">'>",
            "<meta name=\"x\" content=\"a><link rel='icon' href='/in-value2.svg'>\">",
            "<!-- <link rel=\"icon\" href=\"/commented.svg\"> <title>commented</title> -->",
            "<script>var s = '<link rel=\"icon\" href=\"/in-script.svg\"> <title>script</title>';</script>",
            "<style>/* <link rel=\"icon\" href=\"/in-style.svg\"> */</style>",
            "<textarea><link rel=\"icon\" href=\"/in-textarea.svg\"></textarea>",
            // A closing tag names no icon, whatever it holds.
            "</link rel=\"icon\" href=\"/closing.svg\">",
        ];
        let page = format!(
            "<html><head>{}<link rel=\"icon\" href=\"/real.svg\"><title>obop</title></head><body></body></html>",
            kept.concat()
        );
        let out = rewritten(&page, &shown).unwrap();
        for k in kept {
            assert!(out.contains(k), "{k} was touched: {out}");
        }
        assert!(!out.contains("/real.svg"), "{out}");
        assert!(out.contains("<title>MOHV</title>"));
        assert!(!out.contains("<title>obop</title>"));
        // The first `<title>` of the head is the one that changes; a later one (in the body, as an
        // SVG has) is not.
        let page = "<html><head><link rel=\"icon\" href=\"/x.svg\"><TITLE data-x=\"1\">obop</TITLE></head><body><svg><title>drawing</title></svg></body></html>";
        let out = rewritten(page, &shown).unwrap();
        assert!(out.contains("<title>MOHV</title>"), "{out}");
        assert!(out.contains("<svg><title>drawing</title></svg>"), "{out}");
        assert!(!out.contains("data-x"), "{out}");
    }

    #[test]
    fn only_the_head_is_read_so_an_icon_link_or_a_title_of_the_body_is_never_the_one() {
        let shown = shown_of(&[("name", "MOHV")], None);
        // An icon link in the body stays; the one of the head is replaced.
        let page = "<html><head><title>obop</title><link rel=\"icon\" href=\"/a.ico\"></head><body><link rel=\"icon\" href=\"/b.ico\"><svg><title>drawing</title></svg></body></html>";
        let out = rewritten(page, &shown).unwrap();
        assert!(out.contains("<link rel=\"icon\" href=\"/b.ico\">"), "{out}");
        assert!(out.contains("<svg><title>drawing</title></svg>"), "{out}");
        assert!(!out.contains("/a.ico"), "{out}");
        assert_eq!(out.matches("<link").count(), 2, "{out}");
        // A head without a title has none, whatever the body has.
        let page = "<html><head><link rel=\"icon\" href=\"/x\"></head><body><svg><title>d</title></svg></body></html>";
        assert_eq!(rewritten(page, &shown), Err("it has no <title>"));
        // A head without an icon link has none, whatever the body has.
        let page = "<html><head><title>obop</title></head><body><link rel=\"icon\" href=\"/b.ico\"></body></html>";
        assert_eq!(rewritten(page, &shown), Err("it has no icon link"));
    }

    #[test]
    fn a_closing_title_before_the_title_is_not_the_title() {
        let shown = shown_of(&[("name", "MOHV")], None);
        let link = format!(
            "<link rel=\"icon\" href=\"/api/brand/logo.svg?v={}\" type=\"image/svg+xml\">",
            shown.v()
        );
        // The stray `</title>` stays where it is; the `<title>` that opens one is the one replaced.
        let page = "<html><head></title><link rel=\"icon\" href=\"/x.svg\"><title>obop</title></head><body></body></html>";
        let out = rewritten(page, &shown).unwrap();
        assert!(
            out.ends_with(&format!(
                "</title>{link}<title>MOHV</title></head><body></body></html>"
            )),
            "{out}"
        );
    }

    #[test]
    fn the_text_of_a_title_is_not_markup_so_a_link_inside_it_is_no_icon() {
        let shown = shown_of(&[("name", "MOHV")], None);
        let page = "<html><head><link rel=\"icon\" href=\"/real.ico\"><title>obop <link rel=\"icon\" href=\"/t.ico\"> x</title></head><body></body></html>";
        let out = rewritten(page, &shown).unwrap();
        assert!(out.contains("<title>MOHV</title>"), "{out}");
        assert!(
            !out.contains("/t.ico") && !out.contains("/real.ico"),
            "{out}"
        );
        assert_eq!(out.matches("<link rel=\"icon\"").count(), 1, "{out}");
    }

    #[test]
    fn the_tag_follows_the_declaration_of_the_encoding_so_that_it_stays_near_the_top() {
        let shown = shown_of(&[("name", "MOHV")], None);
        let tag = "<meta name=\"obop-brand\"";
        let page = |meta: &str| {
            format!(
                "<html><head>{meta}<title>obop</title><link rel=\"icon\" href=\"/x\"></head></html>"
            )
        };
        // Wherever the declaration is in the head, the tag comes right after it.
        for meta in [
            "<meta charset=\"UTF-8\" />",
            "<META CHARSET=utf-8>",
            "<meta http-equiv=\"Content-Type\" content=\"text/html; charset=utf-8\">",
            "<meta  http-equiv='content-type' content='text/html'>",
        ] {
            let out = rewritten(&page(meta), &shown).unwrap();
            assert!(out.contains(&format!("<head>{meta}{tag}")), "{meta}: {out}");
        }
        let out = rewritten(
            &format!(
                "<html><head><meta name=\"viewport\" content=\"x\">{}<title>obop</title><link rel=\"icon\" href=\"/x\"></head></html>",
                "<meta charset=\"UTF-8\">"
            ),
            &shown,
        )
        .unwrap();
        assert!(
            out.contains(&format!(
                "<meta name=\"viewport\" content=\"x\"><meta charset=\"UTF-8\">{tag}"
            )),
            "{out}"
        );
        // With two declarations the tag follows the first one, the one a browser reads.
        let out = rewritten(
            &page(
                "<meta charset=\"UTF-8\"><meta http-equiv=\"content-type\" content=\"text/html\">",
            ),
            &shown,
        )
        .unwrap();
        assert!(
            out.contains(&format!("<head><meta charset=\"UTF-8\">{tag}")),
            "{out}"
        );
        // A head with no declaration (or only other meta tags) gets the tag first. A value of
        // `http-equiv` with a space around it declares nothing (the standard matches the whole
        // value), and neither does another `http-equiv`.
        for meta in [
            "",
            "<meta name=\"viewport\" content=\"width=1\">",
            "<meta http-equiv=\" content-type \" content=\"text/html\">",
            "<meta http-equiv=\"refresh\" content=\"9\">",
            // A closing tag declares nothing, whatever it holds.
            "</meta charset=\"UTF-8\">",
        ] {
            let out = rewritten(&page(meta), &shown).unwrap();
            assert!(out.contains(&format!("<head>{tag}")), "{meta}: {out}");
        }
        // Long non-ASCII messages do not push the declaration out of the first 1024 bytes.
        let long = "Välkommen åt alla ÅÄÖ ".repeat(10).trim_end().to_string();
        let pairs: Vec<(String, String)> = LANGS
            .iter()
            .map(|l| (format!("message_{l}"), long.clone()))
            .collect();
        let mut fields: Vec<(&str, &str)> = vec![("name", "Åkerblom Öst")];
        fields.extend(pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        let big = shown_of(&fields, None);
        let out = rewritten(BUILT, &big).unwrap();
        let charset = out.find("<meta charset").expect("the declaration");
        assert!(charset < 1024, "the declaration starts at byte {charset}");
        assert!(out.len() > 2048, "the tag is long: {}", out.len());
    }

    #[test]
    fn edits_that_overlap_are_refused_and_the_rest_are_made_in_document_order() {
        let html = b"0123456789";
        let edits = vec![
            (6, 8, "-".to_string()),
            (2, 2, "<>".to_string()),
            (0, 1, String::new()),
        ];
        assert_eq!(
            apply_edits(html, edits).map(|v| String::from_utf8(v).unwrap()),
            Ok("1<>2345-89".to_string())
        );
        // A second edit that starts inside the first.
        assert_eq!(
            apply_edits(html, vec![(2, 6, "x".into()), (4, 5, "y".into())]),
            Err("its head cannot be read")
        );
        // One that starts where the first ends is no overlap.
        assert_eq!(
            apply_edits(html, vec![(2, 4, "x".into()), (4, 5, "y".into())]),
            Ok(b"01xy56789".to_vec())
        );
    }

    #[test]
    fn an_insertion_and_a_replacement_at_the_same_place_are_made_insertion_first() {
        let html = b"0123456789";
        // The same place, the replacement listed first: the insertion still goes in before it.
        assert_eq!(
            apply_edits(html, vec![(2, 4, "x".into()), (2, 2, "<>".into())]),
            Ok(b"01<>x456789".to_vec())
        );
        // Two insertions at one place keep the order they were listed in.
        assert_eq!(
            apply_edits(html, vec![(5, 5, "a".into()), (5, 5, "b".into())]),
            Ok(b"01234ab56789".to_vec())
        );
    }

    #[test]
    fn a_less_than_sign_before_a_digit_or_at_the_end_is_no_tag() {
        let shown = shown_of(&[("name", "MOHV")], None);
        // `<3` opens no tag: it must not swallow the icon link that follows it.
        let page = "<html><head>a <3 b <link rel=\"icon\" href=\"/x.ico\"><title>obop</title></head></html>";
        let out = rewritten(page, &shown).unwrap();
        assert!(
            out.contains("a <3 b <link rel=\"icon\" href=\"/api/brand/logo.svg?v="),
            "{out}"
        );
        assert!(!out.contains("/x.ico"), "{out}");
        // A document that ends in the middle of a tag name is read to its end without a panic.
        for end in ["<", "</", "<li", "<link rel"] {
            let page = format!("<head><title>obop</title><link rel=\"icon\" href=\"/x\">{end}");
            let out = rewritten(&page, &shown).unwrap();
            assert!(out.contains("<title>MOHV</title>"), "{out}");
        }
    }

    /// Documents made of the pieces that make a head hard to read (quotes, comments, half tags,
    /// bytes that are not text) never panic, and a document that is branded has the tag once.
    #[test]
    fn whatever_the_head_holds_it_is_read_without_a_panic_and_gets_one_tag() {
        const PIECES: [&[u8]; 28] = [
            b"<head>",
            b"</head>",
            b"<head ",
            b"<title>",
            b"</title>",
            b"<title",
            b"<link rel=\"icon\" href=\"/x\">",
            b"<link rel=icon href=x>",
            b"<link rel='icon' href='a>b'>",
            b"<meta charset=utf-8>",
            b"<meta http-equiv=content-type>",
            b"<!--",
            b"-->",
            b"<script>",
            b"</script>",
            b"<textarea>",
            b"</textarea>",
            b"\"",
            b"'",
            b">",
            b"<",
            b"</",
            b"<3",
            b"=",
            b" ",
            b"\n",
            b"\xff\xfe",
            b"text",
        ];
        let shown = shown_of(&[("name", "MOHV")], None);
        let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let (mut branded, mut refused) = (0, 0);
        for _ in 0..20_000 {
            let mut doc: Vec<u8> = Vec::new();
            for _ in 0..(next() % 24) {
                doc.extend_from_slice(PIECES[(next() % PIECES.len() as u64) as usize]);
            }
            match brand_rewrite(&doc, &shown) {
                Ok(out) => {
                    branded += 1;
                    let text = String::from_utf8_lossy(&out);
                    assert_eq!(
                        text.matches("<meta name=\"obop-brand\" content=\"").count(),
                        1,
                        "{}",
                        String::from_utf8_lossy(&doc)
                    );
                    assert!(text.contains("<title>MOHV</title>"), "{text}");
                }
                Err(why) => {
                    refused += 1;
                    assert!(
                        [
                            "it has no <head>",
                            "it has no <title>",
                            "its <title> is not closed",
                            "it has no icon link",
                            "its head cannot be read",
                        ]
                        .contains(&why),
                        "{why}"
                    );
                }
            }
        }
        assert!(
            branded > 100 && refused > 100,
            "{branded} branded, {refused} refused"
        );
    }

    #[test]
    fn the_headers_that_describe_the_file_are_all_dropped_and_the_others_kept() {
        let mut headers = HeaderMap::new();
        for (name, value) in [
            (header::ETAG, "\"abc\""),
            (header::LAST_MODIFIED, "Mon, 01 Jan 2026 00:00:00 GMT"),
            (header::ACCEPT_RANGES, "bytes"),
            (header::CONTENT_TYPE, "text/html"),
            (header::VARY, "accept-encoding"),
        ] {
            headers.insert(name, HeaderValue::from_static(value));
        }
        forget_file_validators(&mut headers);
        let mut left: Vec<&str> = headers.keys().map(|k| k.as_str()).collect();
        left.sort_unstable();
        assert_eq!(left, ["content-type", "vary"]);
    }

    #[test]
    fn a_shell_that_cannot_take_the_brand_says_in_a_fixed_word_why() {
        let shown = shown_of(&[("name", "MOHV")], None);
        for (page, why) in [
            (
                "<html><body><title>obop</title></body></html>",
                "it has no <head>",
            ),
            (
                "<html><head><link rel=\"icon\" href=\"/x\"></head></html>",
                "it has no <title>",
            ),
            (
                "<html><head><title>obop</title></head></html>",
                "it has no icon link",
            ),
            (
                "<html><head><link rel=\"icon\" href=\"/x\"><title>obop</head></html>",
                "its <title> is not closed",
            ),
            (
                "<html><head><meta name=\"obop-brand\" content=\"{}\"><link rel=\"icon\" href=\"/x\"><title>obop</title></head></html>",
                "it already has an obop-brand tag",
            ),
            // The tag as the HTML reader finds it: any case, any quoting, any order.
            (
                "<html><head><META NAME=OBOP-BRAND content=x><link rel=\"icon\" href=\"/x\"><title>obop</title></head></html>",
                "it already has an obop-brand tag",
            ),
            (
                "<html><head><link rel=\"icon\" href=\"/x\"><title>obop</title><meta content=x name='obop-brand'/></head></html>",
                "it already has an obop-brand tag",
            ),
            ("", "it has no <head>"),
            ("<head><title>obop", "its <title> is not closed"),
        ] {
            assert_eq!(rewritten(page, &shown), Err(why), "{page}");
        }
    }

    #[test]
    fn words_that_mention_the_tag_are_not_the_tag() {
        let shown = shown_of(&[("name", "MOHV")], None);
        // A comment, a script or a style sheet that says `obop-brand` (a build that inlines the app
        // has the name in it), a `<meta>` that has it as a value or under another name, in the head
        // or in the body, is not a tag of the brand: the shell is branded and keeps all of them.
        let mentions = [
            "<!-- obop-brand -->",
            "<script>var m = 'meta[name=\"obop-brand\"]';</script>",
            "<style>/* obop-brand */</style>",
            "<meta name=\"description\" content=\"obop-brand\">",
            "<meta name=\"obop-brand-x\" content=\"x\">",
            // An element that is not a `<meta>` and has the name is not the tag either.
            "<link rel=\"stylesheet\" name=\"obop-brand\" href=\"/x.css\">",
            // A closing tag is no tag.
            "</meta name=\"obop-brand\">",
        ];
        for mention in mentions {
            let page = format!(
                "<html><head>{mention}<link rel=\"icon\" href=\"/x\"><title>obop</title></head><body>{mention}</body></html>"
            );
            let out = rewritten(&page, &shown).unwrap_or_else(|why| panic!("{mention}: {why}"));
            assert_eq!(out.matches(mention).count(), 2, "{mention}: kept: {out}");
            assert_eq!(
                out.matches("<meta name=\"obop-brand\" content=\"").count(),
                1,
                "{mention}: {out}"
            );
            assert!(out.contains("<title>MOHV</title>"), "{mention}: {out}");
        }
    }

    #[test]
    fn the_tag_has_the_name_the_slug_the_messages_that_are_set_the_version_and_a_valid_accent_only()
    {
        let shown = shown_of(
            &[
                ("name", "MOHV"),
                ("slug", "mohv-app"),
                ("title_sv", "Logga in till MOHV"),
                ("title_en", "Welcome"),
                ("message_en", "Welcome to MOHV"),
                ("message_sv", "Välkommen"),
                ("hide_product_docs", "true"),
            ],
            Some("#c2410c"),
        );
        let meta = meta_of(&rewritten(BUILT, &shown).unwrap());
        let mut keys: Vec<&str> = meta
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["accent", "messages", "name", "slug", "v"],
            "no titles, no switch, nothing else"
        );
        assert_eq!(meta["name"], "MOHV");
        assert_eq!(meta["slug"], "mohv-app");
        assert_eq!(
            meta["messages"],
            serde_json::json!({"en": "Welcome to MOHV", "sv": "Välkommen"})
        );
        assert_eq!(meta["v"], shown.v());
        assert_eq!(meta["accent"], "#c2410c");
        // The bytes of a tag with two messages: a comma and no space between the entries.
        let two = shown_of(
            &[
                ("name", "MOHV"),
                ("message_en", "Hi"),
                ("message_sv", "Hej"),
            ],
            None,
        );
        assert_eq!(
            Public::of(&two).json(),
            format!(
                "{{\"name\":\"MOHV\",\"slug\":\"mohv\",\"messages\":{{\"en\":\"Hi\",\"sv\":\"Hej\"}},\"v\":\"{}\"}}",
                two.v()
            )
        );
        // The order the page reads: name, slug, messages, v, accent.
        let raw = Public::of(&shown).json();
        let at = |k: &str| raw.find(&format!("\"{k}\":")).unwrap();
        assert!(at("name") < at("slug") && at("slug") < at("messages"));
        assert!(at("messages") < at("v") && at("v") < at("accent"));
    }

    #[test]
    fn a_stored_accent_that_is_not_a_valid_hex_colour_is_left_out_of_the_tag() {
        for bad in [
            "red",
            "#12345",
            "#1234567",
            "c2410c",
            "#ggg000",
            "",
            "rgb(1,2,3)",
        ] {
            let shown = shown_of(&[("name", "MOHV")], Some(bad));
            let meta = meta_of(&rewritten(BUILT, &shown).unwrap());
            assert!(meta.get("accent").is_none(), "{bad:?}: {meta}");
        }
        // The theme stores none.
        let shown = shown_of(&[("name", "MOHV")], None);
        assert!(
            meta_of(&rewritten(BUILT, &shown).unwrap())
                .get("accent")
                .is_none()
        );
        // A valid one is read in lower case.
        let shown = shown_of(&[("name", "MOHV")], Some("#C2410C"));
        let meta = meta_of(&rewritten(BUILT, &shown).unwrap());
        assert_eq!(meta["accent"], "#c2410c");
    }

    #[test]
    fn a_name_with_markup_characters_is_escaped_in_the_title_and_the_tag() {
        // The characters the policy allows in a name and that HTML treats as markup.
        let shown = shown_of(
            &[
                ("name", "Tom & Jerry's"),
                ("message_en", "He said \"hi\" & left"),
            ],
            None,
        );
        let out = rewritten(BUILT, &shown).unwrap();
        assert!(
            out.contains("<title>Tom &amp; Jerry&#39;s</title>"),
            "{out}"
        );
        assert!(!out.contains("Jerry's"), "no raw quote of the name: {out}");
        let meta = meta_of(&out);
        assert_eq!(meta["name"], "Tom & Jerry's");
        assert_eq!(meta["messages"]["en"], "He said \"hi\" & left");
        // The attribute never ends early: the one `">` after the content is the end of the tag.
        let tag_start = out.find("<meta name=\"obop-brand\"").unwrap();
        let tag = &out[tag_start..tag_start + out[tag_start..].find('>').unwrap() + 1];
        assert_eq!(tag.matches('"').count(), 4, "{tag}");
    }

    #[test]
    fn a_row_written_around_the_command_with_markup_shows_the_default_not_the_markup() {
        let (_flaky, backend) = Flaky::memory();
        let fields =
            serde_json::json!({"name": "<script>alert(1)</script>", "message_en": "<b>x</b>"});
        backend
            .apply(&[Op::Put {
                model: "instance_brand".into(),
                id: "instance".into(),
                fields: fields.as_object().unwrap().clone(),
            }])
            .unwrap();
        let shown = Reader::with_bound(backend, Duration::ZERO)
            .with_sink(Arc::new(|_| {}))
            .current();
        let out = rewritten(BUILT, &shown).unwrap();
        assert!(out.contains("<title>obop</title>"), "{out}");
        assert!(!out.contains("<script>alert"), "{out}");
        assert!(meta_of(&out)["messages"].as_object().unwrap().is_empty());
    }

    /// A shell served over a memory store through the real reader.
    fn branded_shell(
        build: &Build,
        language: Option<&'static str>,
        pairs: &[(&str, &str)],
    ) -> (Shell, Arc<Reader>) {
        let (_flaky, backend) = Flaky::memory();
        if !pairs.is_empty() {
            let fields = pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            apply(&*backend, None, "test", &Action::Configure(fields), false).unwrap();
        }
        let reader = Arc::new(Reader::with_bound(backend, Duration::ZERO));
        (
            Shell::with_brand(build.path(), language, reader.clone()),
            reader,
        )
    }

    #[tokio::test]
    async fn a_shell_with_a_brand_is_a_200_without_validators_at_every_address_and_for_every_condition()
     {
        let build = Build::new("branded");
        std::fs::write(build.0.join("index.html"), BUILT).unwrap();
        let (shell, reader) = branded_shell(&build, None, &[("name", "MOHV")]);
        let v = reader.current().v().to_string();
        let modified = modified_of(build.path(), "/").await;
        let (_, _, full) = get(&shell, "GET", "/", &[]).await;
        let text = String::from_utf8(full.clone()).unwrap();
        assert!(text.contains("<title>MOHV</title>") && text.contains(&format!("logo.svg?v={v}")));
        for path in [
            "/",
            "/index.html",
            "/customize",
            "/crm/lead/42",
            "/assets/gone.js",
        ] {
            for (what, headers) in conditions(&modified) {
                let context = format!("{path} ({what})");
                let (status, answer, body) = get(&shell, "GET", path, &headers).await;
                assert_eq!(status, StatusCode::OK, "{context}");
                assert_eq!(body, full, "{context}");
                assert_eq!(answer[header::CACHE_CONTROL], "no-cache", "{context}");
                for gone in [
                    header::ETAG,
                    header::LAST_MODIFIED,
                    header::CONTENT_RANGE,
                    header::ACCEPT_RANGES,
                ] {
                    assert!(answer.get(&gone).is_none(), "{context}: {gone}");
                }
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
    async fn the_language_tag_and_the_brand_tag_are_both_there_once() {
        let build = Build::new("both");
        std::fs::write(build.0.join("index.html"), BUILT).unwrap();
        let (shell, _) = branded_shell(&build, Some("sv"), &[("name", "MOHV")]);
        let (_, _, body) = get(&shell, "GET", "/", &[]).await;
        let text = String::from_utf8(body).unwrap();
        assert_eq!(
            text.matches("name=\"obop-default-language\" content=\"sv\"")
                .count(),
            1
        );
        assert_eq!(text.matches("name=\"obop-brand\"").count(), 1);
        assert_eq!(text.matches("<title>").count(), 1);
        // A name that spells the other tag cannot take it away: the language tag goes in first.
        let (shell, _) = branded_shell(&build, Some("sv"), &[("name", "obop-default-language")]);
        let (_, _, body) = get(&shell, "GET", "/", &[]).await;
        let text = String::from_utf8(body).unwrap();
        assert_eq!(
            text.matches("name=\"obop-default-language\" content=\"sv\"")
                .count(),
            1,
            "{text}"
        );
    }

    #[tokio::test]
    async fn with_no_row_and_no_language_the_shell_is_the_file_and_asks_the_reader_only_for_the_document()
     {
        let build = Build::new("norow");
        let (flaky, backend) = Flaky::memory();
        let reader = Arc::new(Reader::with_bound(backend, Duration::from_secs(60)));
        let plain = Shell::new(build.path(), None);
        let shell = Shell::with_brand(build.path(), None, reader);
        let modified = modified_of(build.path(), "/").await;
        // Files that are not the shell cause no read.
        flaky.reset_reads();
        for path in ["/assets/app.js", "/assets/app.css", "/assets/doc.pdf"] {
            for (_what, headers) in conditions(&modified) {
                for method in ["GET", "HEAD"] {
                    let a = get(&plain, method, path, &headers).await;
                    let b = get(&shell, method, path, &headers).await;
                    assert_eq!(
                        (a.0, shown(&a.1), a.2),
                        (b.0, shown(&b.1), b.2),
                        "{method} {path}"
                    );
                }
            }
        }
        assert_eq!(
            flaky.reads().gets,
            0,
            "a script, a style sheet or a picture asks nobody"
        );
        // The shell is the file's answer, validators and ranges included, for every condition.
        for path in ["/", "/index.html", "/customize", "/assets/gone.js"] {
            for (what, headers) in conditions(&modified) {
                for method in ["GET", "HEAD"] {
                    let a = get(&plain, method, path, &headers).await;
                    let b = get(&shell, method, path, &headers).await;
                    let context = format!("{method} {path} ({what})");
                    assert_eq!(a.0, b.0, "status of {context}");
                    assert_eq!(shown(&a.1), shown(&b.1), "headers of {context}");
                    assert_eq!(a.2, b.2, "body of {context}");
                }
            }
        }
        assert!(flaky.reads().gets > 0, "the document asked the reader");
    }

    #[tokio::test]
    async fn a_shell_that_cannot_take_the_brand_is_served_as_it_is_with_one_line_on_stderr_once() {
        let build = Build::new("cannot");
        for (page, what, why) in [
            ("<html><body>x</body></html>", "no head", "it has no <head>"),
            (
                "<html><head><link rel=\"icon\" href=\"/x\"></head><body></body></html>",
                "no title",
                "it has no <title>",
            ),
            (
                "<html><head><title>obop</title></head><body></body></html>",
                "no icon link",
                "it has no icon link",
            ),
        ] {
            std::fs::write(build.0.join("index.html"), page).unwrap();
            let (shell, _) = branded_shell(&build, None, &[("name", "MOHV")]);
            let lines = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
            let sink = lines.clone();
            let shell =
                shell.with_sink(Arc::new(move |l| sink.lock().unwrap().push(l.to_string())));
            for path in ["/", "/customize", "/index.html"] {
                let (status, headers, body) = get(&shell, "GET", path, &[]).await;
                assert_eq!(status, StatusCode::OK, "{what}");
                assert_eq!(body, page.as_bytes(), "{what}: served as it is");
                assert_eq!(headers[header::CACHE_CONTROL], "no-cache", "{what}");
            }
            let lines = lines.lock().unwrap();
            // The whole line, as an operator reads it: what is wrong, why, what the tab does
            // instead and where to look. It holds nothing of the brand.
            assert_eq!(
                *lines,
                [format!(
                    "obop: the back office shell cannot take the instance brand ({why}): the tab keeps the title and the icon it was built with (obop admin brand status shows the brand)"
                )],
                "{what}: one line, once"
            );
        }
    }

    /// The default language is not lost when the brand cannot be applied: the shell is the file
    /// with the language tag and nothing of the brand.
    #[tokio::test]
    async fn a_shell_that_cannot_take_the_brand_still_carries_the_default_language() {
        let build = Build::new("cannot-language");
        for page in [
            "<html><head><meta charset=\"utf-8\"><link rel=\"icon\" href=\"/x\"></head><body></body></html>",
            "<html><head><title>obop</title></head><body></body></html>",
        ] {
            std::fs::write(build.0.join("index.html"), page).unwrap();
            let (shell, _) = branded_shell(&build, Some("sv"), &[("name", "MOHV")]);
            let shell = shell.with_sink(Arc::new(|_| {}));
            for path in ["/", "/customize"] {
                let (status, _, body) = get(&shell, "GET", path, &[]).await;
                assert_eq!(status, StatusCode::OK);
                let text = String::from_utf8(body).unwrap();
                assert_eq!(
                    text,
                    inject(page.as_bytes(), "sv")
                        .map(|v| String::from_utf8(v).unwrap())
                        .unwrap(),
                    "{page}: the file with the language tag only"
                );
                assert!(!text.contains("obop-brand") && !text.contains("MOHV"));
            }
        }
    }

    /// The shell of a server takes both settings of its state: the default language of the state and
    /// the reader of its dispatcher.
    #[tokio::test]
    async fn the_shell_of_a_state_has_its_default_language_and_the_brand_of_its_dispatcher() {
        let build = Build::new("for-state");
        std::fs::write(build.0.join("index.html"), BUILT).unwrap();
        let (_flaky, backend) = Flaky::memory();
        let fields = [("name".to_string(), "MOHV".to_string())]
            .into_iter()
            .collect();
        apply(&*backend, None, "test", &Action::Configure(fields), false).unwrap();
        let state_of = |language: Option<&'static str>| {
            let dispatcher = Arc::new(obop_commands::Dispatcher::new());
            dispatcher.install_brand(backend.clone());
            let state = HttpState::with_mcp(
                dispatcher,
                Arc::new(crate::passkeys::tests::Nobody),
                crate::mcp::McpConfig::default(),
            );
            state.set_default_language(language);
            state
        };
        let page = |state: &HttpState| {
            let shell = Shell::for_state(build.path(), state).with_sink(Arc::new(|_| {}));
            async move { String::from_utf8(get(&shell, "GET", "/", &[]).await.2).unwrap() }
        };
        let with = page(&state_of(Some("sv"))).await;
        assert!(with.contains("<meta name=\"obop-default-language\" content=\"sv\">"));
        assert!(with.contains("<title>MOHV</title>"), "{with}");
        let without = page(&state_of(None)).await;
        assert!(!without.contains("obop-default-language"), "{without}");
        assert!(without.contains("<title>MOHV</title>"), "{without}");
    }

    /// A reader with no store has only the default brand to give: the shell keeps no reader for
    /// it, so no request of the document takes the hop to a blocking thread for nothing.
    #[test]
    fn a_reader_with_no_store_is_no_reader_and_one_with_a_store_is() {
        let build = Build::new("inert");
        let inert = Shell::with_brand(build.path(), None, Arc::new(Reader::inert()));
        assert!(inert.brand.is_none());
        let (with_store, _) = branded_shell(&build, None, &[]);
        assert!(with_store.brand.is_some());
    }
}
