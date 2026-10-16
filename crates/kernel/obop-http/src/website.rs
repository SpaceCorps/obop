//! Static serving for the marketing site (`web/site`), done properly for people, crawlers and
//! AI agents:
//!
//! - an unknown path answers **404** with the body of `/404.html` (never a soft 404);
//! - `/docs/x` redirects (301) to `/docs/x/` when that is a directory with an `index.html`;
//! - `Accept: text/markdown` gets the page's Markdown mirror (`/docs/x/` -> `/docs/x.md`,
//!   `/` -> `/index.md`, `/de/` -> `/de/index.md`) with `Vary: Accept`;
//! - correct content types with charset, ETag and `304`, long-lived caching for fingerprinted
//!   assets, revalidation for pages, `nosniff`, gzip for text;
//! - `/app/` is the sign-up single-page app and falls back to its own `index.html`.

use axum::{
    Router,
    body::Body,
    extract::State,
    http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::any,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tower_http::compression::CompressionLayer;

/// A router whose fallback serves `root`; mount it as the control plane's fallback service.
pub fn service(root: PathBuf) -> Router {
    Router::new()
        .fallback(any(handle))
        .with_state(Arc::new(root))
        .layer(CompressionLayer::new())
}

/// Decode and normalise a request path into safe segments, or `None` if it tries to escape.
fn segments(uri: &Uri) -> Option<Vec<String>> {
    let raw = uri.path();
    let mut out = Vec::new();
    for part in raw.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        let decoded = percent_decode(part)?;
        if decoded == ".." || decoded.contains(['/', '\\', '\0']) {
            return None;
        }
        out.push(decoded);
    }
    Some(out)
}

fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let h = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(h, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Whether the client asks for Markdown ahead of HTML (`Accept: text/markdown` with q > 0 and
/// not ranked below `text/html`).
fn wants_markdown(h: &HeaderMap) -> bool {
    let Some(accept) = h.get(header::ACCEPT).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let q = |mime: &str| -> Option<f32> {
        accept.split(',').find_map(|part| {
            let mut it = part.trim().split(';');
            (it.next()?.trim().eq_ignore_ascii_case(mime)).then(|| {
                it.find_map(|p| p.trim().strip_prefix("q=").and_then(|v| v.parse().ok()))
                    .unwrap_or(1.0)
            })
        })
    };
    match (q("text/markdown"), q("text/html")) {
        (Some(m), Some(h)) => {
            m > 0.0 && m >= h && accept.find("text/markdown") < accept.find("text/html")
        }
        (Some(m), None) => m > 0.0,
        _ => false,
    }
}

fn is_file(p: &Path) -> bool {
    p.is_file()
}

fn content_type(path: &Path) -> HeaderValue {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let mut s = mime.essence_str().to_string();
    match path.extension().and_then(|e| e.to_str()) {
        Some("md") => s = "text/markdown".into(),
        Some("xml") => s = "application/xml".into(),
        Some("txt") => s = "text/plain".into(),
        _ => {}
    }
    if s.starts_with("text/")
        || s == "application/xml"
        || s == "application/json"
        || s == "image/svg+xml"
        || s == "application/javascript"
        || s == "text/javascript"
    {
        s.push_str("; charset=utf-8");
    }
    HeaderValue::from_str(&s).unwrap_or(HeaderValue::from_static("application/octet-stream"))
}

fn cache_control(path: &Path) -> &'static str {
    let p = path.to_string_lossy();
    let fingerprinted = p.contains("/assets/")
        && path.file_name().is_some_and(|f| {
            f.to_string_lossy().matches('-').count() >= 1 && f.to_string_lossy().len() > 14
        });
    if fingerprinted || p.contains("/fonts/") {
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=300, must-revalidate"
    }
}

fn etag(len: u64, mtime: Option<std::time::SystemTime>) -> String {
    let secs = mtime
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    format!("W/\"{len:x}-{secs:x}\"")
}

async fn respond(
    path: &Path,
    status: StatusCode,
    req_headers: &HeaderMap,
    head: bool,
    vary_accept: bool,
) -> Option<Response> {
    let meta = tokio::fs::metadata(path).await.ok()?;
    let tag = etag(meta.len(), meta.modified().ok());
    let mut resp = if status == StatusCode::OK
        && req_headers
            .get(header::IF_NONE_MATCH)
            .and_then(|v| v.to_str().ok())
            == Some(tag.as_str())
    {
        let mut r = Response::new(Body::empty());
        *r.status_mut() = StatusCode::NOT_MODIFIED;
        r
    } else {
        let bytes = tokio::fs::read(path).await.ok()?;
        let mut r = Response::new(if head {
            Body::empty()
        } else {
            Body::from(bytes)
        });
        *r.status_mut() = status;
        r
    };
    let h = resp.headers_mut();
    h.insert(header::CONTENT_TYPE, content_type(path));
    h.insert(header::ETAG, HeaderValue::from_str(&tag).ok()?);
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(if status == StatusCode::OK {
            cache_control(path)
        } else {
            "no-store"
        }),
    );
    if vary_accept {
        h.insert(header::VARY, HeaderValue::from_static("Accept"));
    }
    Some(resp)
}

async fn handle(
    State(root): State<Arc<PathBuf>>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    let head = method == Method::HEAD;
    if method != Method::GET && !head {
        let mut r = StatusCode::METHOD_NOT_ALLOWED.into_response();
        r.headers_mut()
            .insert(header::ALLOW, HeaderValue::from_static("GET, HEAD"));
        return r;
    }
    let Some(segs) = segments(&uri) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let trailing = uri.path().ends_with('/') || segs.is_empty();
    let mut base = root.as_ref().clone();
    for s in &segs {
        base.push(s);
    }

    // Markdown for agents.
    if wants_markdown(&headers) && (trailing || base.extension().is_none()) {
        let candidates: Vec<PathBuf> = if segs.is_empty() {
            vec![root.join("index.md")]
        } else {
            vec![base.with_extension("md"), base.join("index.md")]
        };
        for c in candidates {
            if is_file(&c)
                && let Some(r) = respond(&c, StatusCode::OK, &headers, head, true).await
            {
                return r;
            }
        }
    }

    // A file that exists.
    if !trailing && is_file(&base) {
        let vary = base.extension().is_some_and(|e| e == "html");
        if let Some(r) = respond(&base, StatusCode::OK, &headers, head, vary).await {
            return r;
        }
    }
    // A directory: redirect to the slash form, or serve its index.
    let index = base.join("index.html");
    if is_file(&index) {
        if !trailing {
            let mut loc = format!("{}/", uri.path());
            if let Some(q) = uri.query() {
                loc.push('?');
                loc.push_str(q);
            }
            let mut r = StatusCode::MOVED_PERMANENTLY.into_response();
            if let Ok(v) = HeaderValue::from_str(&loc) {
                r.headers_mut().insert(header::LOCATION, v);
            }
            return r;
        }
        if let Some(r) = respond(&index, StatusCode::OK, &headers, head, true).await {
            return r;
        }
    }
    // The sign-up single-page app falls back to its own shell.
    if segs.first().is_some_and(|s| s == "app")
        && let Some(r) = respond(
            &root.join("app").join("index.html"),
            StatusCode::OK,
            &headers,
            head,
            false,
        )
        .await
    {
        return r;
    }
    // Not found: a real 404 with the site's own page.
    if let Some(r) = respond(
        &root.join("404.html"),
        StatusCode::NOT_FOUND,
        &headers,
        head,
        true,
    )
    .await
    {
        return r;
    }
    (StatusCode::NOT_FOUND, "Not found").into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn site() -> (tempfile_dir::Dir, Router) {
        let d = tempfile_dir::Dir::new();
        let w = |p: &str, c: &str| {
            let f = d.path().join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, c).unwrap();
        };
        w("index.html", "<h1>home</h1>");
        w("index.md", "# home md");
        w("docs/start/index.html", "<h1>start</h1>");
        w("docs/start.md", "# start md");
        w("de/index.html", "<h1>hallo</h1>");
        w("de/index.md", "# hallo md");
        w("404.html", "<h1>nothing here</h1>");
        w("app/index.html", "<div id=root></div>");
        w("assets/app-abcdef123456.js", "console.log(1)");
        w("robots.txt", "User-agent: *");
        w("sitemap.xml", "<urlset/>");
        let r = service(d.path().to_path_buf());
        (d, r)
    }

    async fn get(r: &Router, path: &str, accept: Option<&str>) -> (StatusCode, HeaderMap, String) {
        let mut b = Request::get(path);
        if let Some(a) = accept {
            b = b.header("accept", a);
        }
        let resp = r
            .clone()
            .oneshot(b.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let (s, h) = (resp.status(), resp.headers().clone());
        (
            s,
            h,
            String::from_utf8_lossy(&resp.into_body().collect().await.unwrap().to_bytes())
                .into_owned(),
        )
    }

    #[tokio::test]
    async fn pages_files_and_real_404s() {
        let (_d, r) = site();
        assert_eq!(get(&r, "/", None).await.2, "<h1>home</h1>");
        assert_eq!(get(&r, "/docs/start/", None).await.2, "<h1>start</h1>");
        let (s, h, body) = get(&r, "/nope/", None).await;
        assert_eq!(
            (s, body.as_str()),
            (StatusCode::NOT_FOUND, "<h1>nothing here</h1>")
        );
        assert_eq!(h[header::CACHE_CONTROL], "no-store");
        assert_eq!(get(&r, "/docs/nope", None).await.0, StatusCode::NOT_FOUND);
        let (s, h, _) = get(&r, "/robots.txt", None).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(h[header::CONTENT_TYPE], "text/plain; charset=utf-8");
        assert_eq!(
            get(&r, "/sitemap.xml", None).await.1[header::CONTENT_TYPE],
            "application/xml; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn slashless_directories_redirect_and_keep_the_query() {
        let (_d, r) = site();
        let (s, h, _) = get(&r, "/docs/start?x=1", None).await;
        assert_eq!(
            (s, h[header::LOCATION].to_str().unwrap()),
            (StatusCode::MOVED_PERMANENTLY, "/docs/start/?x=1")
        );
    }

    #[tokio::test]
    async fn markdown_is_negotiated_for_agents() {
        let (_d, r) = site();
        for (path, want) in [
            ("/", "# home md"),
            ("/docs/start/", "# start md"),
            ("/de/", "# hallo md"),
        ] {
            let (s, h, body) = get(&r, path, Some("text/markdown")).await;
            assert_eq!((s, body.as_str()), (StatusCode::OK, want), "{path}");
            assert_eq!(h[header::CONTENT_TYPE], "text/markdown; charset=utf-8");
            assert_eq!(h[header::VARY], "Accept");
        }
        // Browsers asking for HTML first keep getting HTML; a page without a mirror falls back.
        assert_eq!(
            get(&r, "/", Some("text/html,text/markdown;q=0.5")).await.2,
            "<h1>home</h1>"
        );
        assert_eq!(get(&r, "/", Some("text/html")).await.2, "<h1>home</h1>");
        assert_eq!(
            get(&r, "/docs/start/", Some("text/markdown;q=0")).await.2,
            "<h1>start</h1>"
        );
        assert_eq!(
            get(&r, "/app/", Some("text/markdown")).await.2,
            "<div id=root></div>"
        );
    }

    #[tokio::test]
    async fn the_app_falls_back_to_its_shell_and_assets_are_immutable() {
        let (_d, r) = site();
        assert_eq!(
            get(&r, "/app/anything/deep", None).await,
            get(&r, "/app/anything/deep", None).await
        );
        let (s, _, body) = get(&r, "/app/anything/deep", None).await;
        assert_eq!((s, body.as_str()), (StatusCode::OK, "<div id=root></div>"));
        let (_, h, _) = get(&r, "/assets/app-abcdef123456.js", None).await;
        assert_eq!(
            h[header::CACHE_CONTROL],
            "public, max-age=31536000, immutable"
        );
        assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    }

    #[tokio::test]
    async fn etags_give_304_and_traversal_is_refused() {
        let (_d, r) = site();
        let (_, h, _) = get(&r, "/robots.txt", None).await;
        let tag = h[header::ETAG].to_str().unwrap().to_string();
        let resp = r
            .clone()
            .oneshot(
                Request::get("/robots.txt")
                    .header("if-none-match", tag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
        for bad in [
            "/../etc/passwd",
            "/%2e%2e/etc/passwd",
            "/docs/%2e%2e/%2e%2e/x",
            "/a%5cb",
            "/%00",
        ] {
            let s = get(&r, bad, None).await.0;
            assert!(
                s == StatusCode::BAD_REQUEST || s == StatusCode::NOT_FOUND,
                "{bad} -> {s}"
            );
        }
        let post = r
            .clone()
            .oneshot(Request::post("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(post.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    /// A tiny self-cleaning temp directory so the tests need no extra dependency.
    mod tempfile_dir {
        pub struct Dir(std::path::PathBuf);
        impl Dir {
            pub fn new() -> Self {
                let p = std::env::temp_dir().join(format!(
                    "obop-site-test-{}-{}",
                    std::process::id(),
                    uuid_like()
                ));
                std::fs::create_dir_all(&p).unwrap();
                Dir(p)
            }
            pub fn path(&self) -> &std::path::Path {
                &self.0
            }
        }
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        fn uuid_like() -> u128 {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        }
    }
}
