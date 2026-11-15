//! Golden pins of the back office shell for an instance with no brand row (slice S2 of the branding
//! work, docs/spec/branding.md, section 13, "No row, same bytes"; taken before the shell was
//! changed). They were printed on origin/main e6a0ad0d (S1c merged, nothing of S2 yet) by
//! `OBOP_PRINT_PINS=1` and are the answers the shell gave before the reader reached it: the index
//! document at every address that serves it, with and without `OBOP_DEFAULT_LANGUAGE`, the answers to
//! the validators and ranges of a browser (304, 412, 206, 416, `HEAD`), the favicon request, a script
//! and a style sheet of the build, and a path that is a file that does not exist (the shell again).
//!
//! The one deliberate change of S2 that the table does not hold is the 503 of D74 while the store
//! cannot be read (spec section 14); `shell_tests.rs` holds that. With the store answering and no row
//! every digest of this table must stay as printed. A digest that moves is a finding, not a pin to
//! re-print.
//!
//! The files of the build have a fixed modification time, so `Last-Modified` and the answers to
//! `If-Modified-Since` are the same on every run. The answer is hashed with every header sorted by
//! name, `Content-Length` included, a blank line and the body.

use crate::app::{App, DevAuth};
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};
use tower::ServiceExt;

/// The shell as the Vite build writes it: the base is `/` or `/admin/`.
fn index_html(base: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\">\n  <head>\n    <meta charset=\"UTF-8\" />\n    <meta name=\"color-scheme\" content=\"light dark\" />\n    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\" />\n    <link rel=\"icon\" href=\"{base}favicon.svg\" type=\"image/svg+xml\" />\n    <title>obop</title>\n    <script type=\"module\" crossorigin src=\"{base}assets/app.js\"></script>\n    <link rel=\"stylesheet\" crossorigin href=\"{base}assets/app.css\">\n  </head>\n  <body>\n    <div id=\"root\"></div>\n  </body>\n</html>\n"
    )
}

const FAVICON: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\"><rect width=\"64\" height=\"64\" rx=\"14\" fill=\"#6d3fd0\"/></svg>";
const SCRIPT: &str = "console.log('admin');";
const SHEET: &str = "body{margin:0}";

/// The modification time every file of the build has: Tue, 14 Nov 2023 22:13:20 GMT.
const MODIFIED: &str = "Tue, 14 Nov 2023 22:13:20 GMT";
const LONG_AGO: &str = "Mon, 01 Jan 1990 00:00:00 GMT";

/// A scratch back office build, removed when it is dropped.
pub(crate) struct Build(std::path::PathBuf);

impl Build {
    pub(crate) fn new(base: &str) -> Build {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "obop-shell-pin-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        for (name, text) in [
            ("index.html", index_html(base)),
            ("favicon.svg", FAVICON.to_string()),
            ("assets/app.js", SCRIPT.to_string()),
            ("assets/app.css", SHEET.to_string()),
        ] {
            let path = dir.join(name);
            std::fs::write(&path, text).unwrap();
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000))
                .unwrap();
        }
        Build(dir)
    }

    pub(crate) fn path(&self) -> &str {
        self.0.to_str().unwrap()
    }
}

impl Drop for Build {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// One mounting of the server with a back office build in it.
pub(crate) struct Mounted {
    pub(crate) router: axum::Router,
    pub(crate) app: App,
    /// Where the back office is: `""` for the root, `"/admin"` under the public site.
    pub(crate) prefix: &'static str,
    #[allow(dead_code)]
    pub(crate) build: Build,
}

/// The back office beside the API (`--static`, prefix `""`) or under the public site (`/admin`),
/// with the default language the sign-in screens start in, if any.
pub(crate) fn mounted(public: bool, language: Option<&'static str>) -> Mounted {
    mounted_over(App::new(), public, language)
}

/// As [`mounted`], over an app the caller made (a store that can be made to fail, for instance).
pub(crate) fn mounted_over(app: App, public: bool, language: Option<&'static str>) -> Mounted {
    let prefix = if public { "/admin" } else { "" };
    let build = Build::new(&format!("{prefix}/"));
    let state = obop_http::HttpState::with_mcp(
        app.dispatcher.clone(),
        Arc::new(DevAuth),
        obop_http::mcp::McpConfig::default(),
    );
    state.set_default_language(language);
    let router = if public {
        let site = Arc::new(obop_site::Site::new(
            obop_commands::PublicApi::new(app.dispatcher.clone(), app.store.clone()),
            obop_site::Config::default(),
        ));
        obop_http::public::router(
            state,
            Some(build.path()),
            site,
            obop_http::public::PublicConfig::default(),
        )
    } else {
        obop_http::router(state, Some(build.path()))
    };
    Mounted {
        router,
        app,
        prefix,
        build,
    }
}

/// An answer as one text: the status, every header sorted by name (the length included), a blank
/// line and the body.
pub(crate) async fn exact(
    r: &axum::Router,
    method: &str,
    path: &str,
    h: &[(&str, &str)],
) -> String {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "inst.test");
    for (k, v) in h {
        req = req.header(*k, *v);
    }
    let resp = r
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status().as_u16();
    let mut headers: Vec<String> = resp
        .headers()
        .iter()
        .map(|(k, v)| format!("{k}: {}", String::from_utf8_lossy(v.as_bytes())))
        .collect();
    headers.sort();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    format!(
        "{status}\n{}\n\n{}",
        headers.join("\n"),
        String::from_utf8_lossy(&body)
    )
}

type Request3 = (
    &'static str,
    &'static str,
    &'static str,
    Vec<(&'static str, &'static str)>,
);

/// The requests that are pinned, as `(name, method, path under the back office, headers)`.
fn requests() -> Vec<Request3> {
    vec![
        ("index", "GET", "/", vec![]),
        ("index_html", "GET", "/index.html", vec![]),
        ("route", "GET", "/customize", vec![]),
        ("deep_route", "GET", "/crm/lead/42", vec![]),
        ("missing_script", "GET", "/assets/gone.js", vec![]),
        ("head", "HEAD", "/", vec![]),
        (
            "fresh_copy",
            "GET",
            "/",
            vec![("if-modified-since", MODIFIED)],
        ),
        (
            "stale_copy",
            "GET",
            "/",
            vec![("if-modified-since", LONG_AGO)],
        ),
        ("etag", "GET", "/", vec![("if-none-match", "\"abc\"")]),
        ("etag_star", "GET", "/", vec![("if-none-match", "*")]),
        (
            "unmodified_since",
            "GET",
            "/",
            vec![("if-unmodified-since", LONG_AGO)],
        ),
        ("range", "GET", "/", vec![("range", "bytes=0-9")]),
        (
            "range_beyond",
            "GET",
            "/",
            vec![("range", "bytes=99999-999999")],
        ),
        (
            "range_if_range_stale",
            "GET",
            "/",
            vec![("range", "bytes=0-9"), ("if-range", LONG_AGO)],
        ),
        (
            "head_fresh_copy",
            "HEAD",
            "/",
            vec![("if-modified-since", MODIFIED)],
        ),
        ("favicon", "GET", "/favicon.svg", vec![]),
        ("favicon_head", "HEAD", "/favicon.svg", vec![]),
        (
            "favicon_fresh_copy",
            "GET",
            "/favicon.svg",
            vec![("if-modified-since", MODIFIED)],
        ),
        ("script", "GET", "/assets/app.js", vec![]),
        (
            "script_fresh_copy",
            "GET",
            "/assets/app.js",
            vec![("if-modified-since", MODIFIED)],
        ),
        (
            "script_range",
            "GET",
            "/assets/app.js",
            vec![("range", "bytes=0-6")],
        ),
        ("sheet", "GET", "/assets/app.css", vec![]),
    ]
}

/// The rows of `SHELL_PINS` for one mounting: `<layout>/<language>/<request>`.
pub(crate) async fn table(
    public: bool,
    language: Option<&'static str>,
    prepare: impl Fn(&Mounted),
) -> Vec<(String, String)> {
    let m = mounted(public, language);
    prepare(&m);
    let layout = if public { "public" } else { "static" };
    let lang = language.unwrap_or("none");
    let mut now = Vec::new();
    for (name, method, path, headers) in requests() {
        let full = format!("{}{path}", m.prefix);
        now.push((
            format!("{layout}/{lang}/{name}"),
            crate::mfa_tests::pin::sha(&exact(&m.router, method, &full, &headers).await),
        ));
    }
    now
}

#[tokio::test]
async fn pins_the_shell_as_an_instance_with_no_brand_serves_it() {
    use crate::mfa_tests::pin;
    let mut now = Vec::new();
    for public in [false, true] {
        for language in [None, Some("sv")] {
            now.extend(table(public, language, |_| {}).await);
        }
    }
    pin::check("SHELL_PINS", SHELL_PINS, &now);
}

// Printed by `OBOP_PRINT_PINS=1` on origin/main e6a0ad0d and pasted here.
#[rustfmt::skip]
pub(crate) const SHELL_PINS: &[(&str, &str)] = &[
    ("static/none/index", "48361a50b582a84aaf2441c61a7448023913d7ba4cbe1dc850431690cd181918"),
    ("static/none/index_html", "48361a50b582a84aaf2441c61a7448023913d7ba4cbe1dc850431690cd181918"),
    ("static/none/route", "48361a50b582a84aaf2441c61a7448023913d7ba4cbe1dc850431690cd181918"),
    ("static/none/deep_route", "48361a50b582a84aaf2441c61a7448023913d7ba4cbe1dc850431690cd181918"),
    ("static/none/missing_script", "48361a50b582a84aaf2441c61a7448023913d7ba4cbe1dc850431690cd181918"),
    ("static/none/head", "dfea629f2ee6d410c3140e10c6f92f4fde4aeee37f8045432bd87e45ee49e544"),
    ("static/none/fresh_copy", "def7a517696898a9f0a840ed6a1f9edd3d0b4875ab5d5f9736c4f0a8f4fa4abc"),
    ("static/none/stale_copy", "48361a50b582a84aaf2441c61a7448023913d7ba4cbe1dc850431690cd181918"),
    ("static/none/etag", "48361a50b582a84aaf2441c61a7448023913d7ba4cbe1dc850431690cd181918"),
    ("static/none/etag_star", "48361a50b582a84aaf2441c61a7448023913d7ba4cbe1dc850431690cd181918"),
    ("static/none/unmodified_since", "da8d1dc074834a87e2424f85cb0a45ed22f72cd907222a64faf4f6f8879912e0"),
    ("static/none/range", "253e7010ef735b9fac8502d707462d8e9a002a772ab56fdcd7830635327bce62"),
    ("static/none/range_beyond", "f40b5c8045083b5d4abae95531cd9cd8bc28e0b890b9e4bf6949bc8d1ae31c8d"),
    ("static/none/range_if_range_stale", "253e7010ef735b9fac8502d707462d8e9a002a772ab56fdcd7830635327bce62"),
    ("static/none/head_fresh_copy", "def7a517696898a9f0a840ed6a1f9edd3d0b4875ab5d5f9736c4f0a8f4fa4abc"),
    ("static/none/favicon", "d9b6367e829bbbb2d7bf0f1a98fb23d657e4ae7e8ccbc8eef843b4e96dbf2349"),
    ("static/none/favicon_head", "cd83b0d6f19bfc062f8fbb3e79209c09e1fa356bac63b6f0611b20e8f6682d7e"),
    ("static/none/favicon_fresh_copy", "def7a517696898a9f0a840ed6a1f9edd3d0b4875ab5d5f9736c4f0a8f4fa4abc"),
    ("static/none/script", "4a28497ad85760027c3ac5c74b710f8d71a6e64c21634990726d929e97a755cb"),
    ("static/none/script_fresh_copy", "def7a517696898a9f0a840ed6a1f9edd3d0b4875ab5d5f9736c4f0a8f4fa4abc"),
    ("static/none/script_range", "c8edcb41a8f2b3d6944f71a6ccb199dd58aea6174eae848b214013d2a15c1890"),
    ("static/none/sheet", "fb7f56f2c905dede4e6e4731b501fbd2839ae47cc12def6a59f7488818642a87"),
    ("static/sv/index", "6713ccd58dc48f1ac05787ebbf290871855d21b6eaf96c9d903a5902357616b4"),
    ("static/sv/index_html", "6713ccd58dc48f1ac05787ebbf290871855d21b6eaf96c9d903a5902357616b4"),
    ("static/sv/route", "6713ccd58dc48f1ac05787ebbf290871855d21b6eaf96c9d903a5902357616b4"),
    ("static/sv/deep_route", "6713ccd58dc48f1ac05787ebbf290871855d21b6eaf96c9d903a5902357616b4"),
    ("static/sv/missing_script", "6713ccd58dc48f1ac05787ebbf290871855d21b6eaf96c9d903a5902357616b4"),
    ("static/sv/head", "de1445aff15d5ebad702f0d3fab4135807170e438bc4df3ba4004b2fcd937345"),
    ("static/sv/fresh_copy", "6713ccd58dc48f1ac05787ebbf290871855d21b6eaf96c9d903a5902357616b4"),
    ("static/sv/stale_copy", "6713ccd58dc48f1ac05787ebbf290871855d21b6eaf96c9d903a5902357616b4"),
    ("static/sv/etag", "6713ccd58dc48f1ac05787ebbf290871855d21b6eaf96c9d903a5902357616b4"),
    ("static/sv/etag_star", "6713ccd58dc48f1ac05787ebbf290871855d21b6eaf96c9d903a5902357616b4"),
    ("static/sv/unmodified_since", "6713ccd58dc48f1ac05787ebbf290871855d21b6eaf96c9d903a5902357616b4"),
    ("static/sv/range", "6713ccd58dc48f1ac05787ebbf290871855d21b6eaf96c9d903a5902357616b4"),
    ("static/sv/range_beyond", "6713ccd58dc48f1ac05787ebbf290871855d21b6eaf96c9d903a5902357616b4"),
    ("static/sv/range_if_range_stale", "6713ccd58dc48f1ac05787ebbf290871855d21b6eaf96c9d903a5902357616b4"),
    ("static/sv/head_fresh_copy", "de1445aff15d5ebad702f0d3fab4135807170e438bc4df3ba4004b2fcd937345"),
    ("static/sv/favicon", "d9b6367e829bbbb2d7bf0f1a98fb23d657e4ae7e8ccbc8eef843b4e96dbf2349"),
    ("static/sv/favicon_head", "cd83b0d6f19bfc062f8fbb3e79209c09e1fa356bac63b6f0611b20e8f6682d7e"),
    ("static/sv/favicon_fresh_copy", "def7a517696898a9f0a840ed6a1f9edd3d0b4875ab5d5f9736c4f0a8f4fa4abc"),
    ("static/sv/script", "4a28497ad85760027c3ac5c74b710f8d71a6e64c21634990726d929e97a755cb"),
    ("static/sv/script_fresh_copy", "def7a517696898a9f0a840ed6a1f9edd3d0b4875ab5d5f9736c4f0a8f4fa4abc"),
    ("static/sv/script_range", "c8edcb41a8f2b3d6944f71a6ccb199dd58aea6174eae848b214013d2a15c1890"),
    ("static/sv/sheet", "fb7f56f2c905dede4e6e4731b501fbd2839ae47cc12def6a59f7488818642a87"),
    ("public/none/index", "6c235ccbe08b0338f98a208ac39e50edb8496266fb3c1bf74924d9047207cde7"),
    ("public/none/index_html", "6c235ccbe08b0338f98a208ac39e50edb8496266fb3c1bf74924d9047207cde7"),
    ("public/none/route", "6c235ccbe08b0338f98a208ac39e50edb8496266fb3c1bf74924d9047207cde7"),
    ("public/none/deep_route", "6c235ccbe08b0338f98a208ac39e50edb8496266fb3c1bf74924d9047207cde7"),
    ("public/none/missing_script", "6c235ccbe08b0338f98a208ac39e50edb8496266fb3c1bf74924d9047207cde7"),
    ("public/none/head", "bab77e589f148f310dc5aab11bb7c4ad3e56c7c9ced2b656c3f3c70a79c3fc1c"),
    ("public/none/fresh_copy", "def7a517696898a9f0a840ed6a1f9edd3d0b4875ab5d5f9736c4f0a8f4fa4abc"),
    ("public/none/stale_copy", "6c235ccbe08b0338f98a208ac39e50edb8496266fb3c1bf74924d9047207cde7"),
    ("public/none/etag", "6c235ccbe08b0338f98a208ac39e50edb8496266fb3c1bf74924d9047207cde7"),
    ("public/none/etag_star", "6c235ccbe08b0338f98a208ac39e50edb8496266fb3c1bf74924d9047207cde7"),
    ("public/none/unmodified_since", "da8d1dc074834a87e2424f85cb0a45ed22f72cd907222a64faf4f6f8879912e0"),
    ("public/none/range", "0dcc139e3efcd7b92fd7f57a704469d0ef3ac84bee4a2228b4cf782512cdf6ad"),
    ("public/none/range_beyond", "79a71f3f37f364b8ce889c6d415c9701b7562285efc26bceeadce58ee767ec2e"),
    ("public/none/range_if_range_stale", "0dcc139e3efcd7b92fd7f57a704469d0ef3ac84bee4a2228b4cf782512cdf6ad"),
    ("public/none/head_fresh_copy", "def7a517696898a9f0a840ed6a1f9edd3d0b4875ab5d5f9736c4f0a8f4fa4abc"),
    ("public/none/favicon", "d9b6367e829bbbb2d7bf0f1a98fb23d657e4ae7e8ccbc8eef843b4e96dbf2349"),
    ("public/none/favicon_head", "cd83b0d6f19bfc062f8fbb3e79209c09e1fa356bac63b6f0611b20e8f6682d7e"),
    ("public/none/favicon_fresh_copy", "def7a517696898a9f0a840ed6a1f9edd3d0b4875ab5d5f9736c4f0a8f4fa4abc"),
    ("public/none/script", "4a28497ad85760027c3ac5c74b710f8d71a6e64c21634990726d929e97a755cb"),
    ("public/none/script_fresh_copy", "def7a517696898a9f0a840ed6a1f9edd3d0b4875ab5d5f9736c4f0a8f4fa4abc"),
    ("public/none/script_range", "c8edcb41a8f2b3d6944f71a6ccb199dd58aea6174eae848b214013d2a15c1890"),
    ("public/none/sheet", "fb7f56f2c905dede4e6e4731b501fbd2839ae47cc12def6a59f7488818642a87"),
    ("public/sv/index", "68fe3b3cd77a60a4e0687159aed78def01d3b79adfcbcd872bb53a93b60005ee"),
    ("public/sv/index_html", "68fe3b3cd77a60a4e0687159aed78def01d3b79adfcbcd872bb53a93b60005ee"),
    ("public/sv/route", "68fe3b3cd77a60a4e0687159aed78def01d3b79adfcbcd872bb53a93b60005ee"),
    ("public/sv/deep_route", "68fe3b3cd77a60a4e0687159aed78def01d3b79adfcbcd872bb53a93b60005ee"),
    ("public/sv/missing_script", "68fe3b3cd77a60a4e0687159aed78def01d3b79adfcbcd872bb53a93b60005ee"),
    ("public/sv/head", "56dd541b495321667bac944e7863a0613c6897ad1c50e48ac5f06df34ab482de"),
    ("public/sv/fresh_copy", "68fe3b3cd77a60a4e0687159aed78def01d3b79adfcbcd872bb53a93b60005ee"),
    ("public/sv/stale_copy", "68fe3b3cd77a60a4e0687159aed78def01d3b79adfcbcd872bb53a93b60005ee"),
    ("public/sv/etag", "68fe3b3cd77a60a4e0687159aed78def01d3b79adfcbcd872bb53a93b60005ee"),
    ("public/sv/etag_star", "68fe3b3cd77a60a4e0687159aed78def01d3b79adfcbcd872bb53a93b60005ee"),
    ("public/sv/unmodified_since", "68fe3b3cd77a60a4e0687159aed78def01d3b79adfcbcd872bb53a93b60005ee"),
    ("public/sv/range", "68fe3b3cd77a60a4e0687159aed78def01d3b79adfcbcd872bb53a93b60005ee"),
    ("public/sv/range_beyond", "68fe3b3cd77a60a4e0687159aed78def01d3b79adfcbcd872bb53a93b60005ee"),
    ("public/sv/range_if_range_stale", "68fe3b3cd77a60a4e0687159aed78def01d3b79adfcbcd872bb53a93b60005ee"),
    ("public/sv/head_fresh_copy", "56dd541b495321667bac944e7863a0613c6897ad1c50e48ac5f06df34ab482de"),
    ("public/sv/favicon", "d9b6367e829bbbb2d7bf0f1a98fb23d657e4ae7e8ccbc8eef843b4e96dbf2349"),
    ("public/sv/favicon_head", "cd83b0d6f19bfc062f8fbb3e79209c09e1fa356bac63b6f0611b20e8f6682d7e"),
    ("public/sv/favicon_fresh_copy", "def7a517696898a9f0a840ed6a1f9edd3d0b4875ab5d5f9736c4f0a8f4fa4abc"),
    ("public/sv/script", "4a28497ad85760027c3ac5c74b710f8d71a6e64c21634990726d929e97a755cb"),
    ("public/sv/script_fresh_copy", "def7a517696898a9f0a840ed6a1f9edd3d0b4875ab5d5f9736c4f0a8f4fa4abc"),
    ("public/sv/script_range", "c8edcb41a8f2b3d6944f71a6ccb199dd58aea6174eae848b214013d2a15c1890"),
    ("public/sv/sheet", "fb7f56f2c905dede4e6e4731b501fbd2839ae47cc12def6a59f7488818642a87"),
];
