//! The logo route end to end (docs/spec/branding.md, slice S1b): `GET /api/brand/logo.svg` in the
//! three layouts the server is mounted in (the API alone, the back office build served beside it,
//! the instance with the public site and the back office under `/admin`), on the real `App` with
//! the real reader of its dispatcher, over the real router. The fault-injection double
//! (`Flaky`) stands under the router for the part of D74: what the request that runs a failed
//! refill is answered, and what a stored row that fails the read check is served as.
//!
//! What the route answers with no brand row is held by `pins_what_the_logo_address_answers_*` in
//! `mcp_tests` and `public_site_tests` (the golden digests of the first commit of the work), whose
//! `GET` and `POST` rows moved with this slice and say why; the tests here hold the same answers by
//! reading them, so a digest that moved cannot be re-pinned without the reason being visible.

use crate::app::App;
use crate::brand_row_tests::{FIELDS, set_brand};
use axum::{
    body::Body,
    http::{HeaderMap, Request},
};
use http_body_util::BodyExt;
use obop_brand::logo::{LOGO_CACHE_CONTROL, LOGO_CONTENT_TYPE, LOGO_CSP, LOGO_NOSNIFF};
use obop_commands::{ADMIN_GROUP, Authenticator, Principal, Store};
use obop_http::mcp::McpConfig;
use obop_storage::Op;
use obop_storage::testing::{Flaky, Mode};
use serde_json::{Value as J, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tower::ServiceExt;

const PATH: &str = "/api/brand/logo.svg";
const HOST: &str = "inst.test";

/// The spelling of the headers in the spec (section 4, "The logo route"), written out here so that a
/// slip in the constants of `obop-brand` is not copied into the test.
const SPEC_CONTENT_TYPE: &str = "image/svg+xml";
const SPEC_NOSNIFF: &str = "nosniff";
const SPEC_CSP: &str = "default-src 'none'; style-src 'none'; sandbox";
const SPEC_CACHE_CONTROL: &str = "no-cache";

/// The token `admin` is a person with `platform.admin`, the only one who may change the brand.
struct Tokens;

impl Authenticator for Tokens {
    fn resolve(&self, token: &str) -> Option<Principal> {
        (token == "admin").then(|| Principal::user("ada", &[ADMIN_GROUP]))
    }
    fn login(&self, _: &str, _: &str) -> Result<String, String> {
        Err("no login in this test".into())
    }
    fn issue_agent_token(&self, _: &Principal, _: &[String], _: &str) -> Result<String, String> {
        Err("no agent tokens in this test".into())
    }
}

/// A scratch directory with a back office build in it, removed when it is dropped.
struct Build(std::path::PathBuf);

impl Build {
    fn new() -> Build {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "obop-brand-http-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("index.html"), "<html>BACKOFFICE-SHELL</html>").unwrap();
        std::fs::write(dir.join("assets/app.js"), "console.log('admin')").unwrap();
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

/// One mounting of the server: its name in a failure, its router and the app behind it.
struct Layout {
    name: &'static str,
    router: axum::Router,
    app: App,
    _build: Option<Build>,
}

fn state(app: &App) -> Arc<obop_http::HttpState> {
    obop_http::HttpState::with_mcp(
        app.dispatcher.clone(),
        Arc::new(Tokens),
        obop_http::mcp::McpConfig::default(),
    )
}

/// The API alone (`obop serve`), the back office build beside it (`--static`) and the instance with
/// the public site (`--public-site`), each over an app of its own.
fn layouts() -> Vec<Layout> {
    let plain = App::new();
    let plain_router = obop_http::router(state(&plain), None);
    let beside = App::new();
    let build = Build::new();
    let beside_router = obop_http::router(state(&beside), Some(build.path()));
    let public = App::new();
    let build_public = Build::new();
    let site = Arc::new(obop_site::Site::new(
        obop_commands::PublicApi::new(public.dispatcher.clone(), public.store.clone()),
        obop_site::Config::default(),
    ));
    let public_router = obop_http::public::router(
        state(&public),
        Some(build_public.path()),
        site,
        obop_http::public::PublicConfig::default(),
    );
    vec![
        Layout {
            name: "api",
            router: plain_router,
            app: plain,
            _build: None,
        },
        Layout {
            name: "back_office",
            router: beside_router,
            app: beside,
            _build: Some(build),
        },
        Layout {
            name: "public_site",
            router: public_router,
            app: public,
            _build: Some(build_public),
        },
    ]
}

/// The API alone, over an app of its own.
fn plain() -> Layout {
    let app = App::new();
    Layout {
        name: "api",
        router: obop_http::router(state(&app), None),
        app,
        _build: None,
    }
}

/// What a request was answered, as a test reads it.
struct Answer {
    status: u16,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl Answer {
    fn header(&self, name: &str) -> &str {
        self.headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    }

    fn has(&self, name: &str) -> bool {
        self.headers.contains_key(name)
    }

    fn text(&self) -> String {
        String::from_utf8(self.body.clone()).expect("the body is text")
    }

    /// Every header as `name: value`, sorted: what two answers must share to be the same answer.
    fn header_lines(&self) -> Vec<String> {
        let mut lines: Vec<String> = self
            .headers
            .iter()
            .map(|(k, v)| format!("{k}: {}", String::from_utf8_lossy(v.as_bytes())))
            .collect();
        lines.sort();
        lines
    }
}

async fn ask(r: &axum::Router, method: &str, path: &str, headers: &[(&str, &str)]) -> Answer {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("host", HOST);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let resp = r
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let (status, headers) = (resp.status().as_u16(), resp.headers().clone());
    let body = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    Answer {
        status,
        headers,
        body,
    }
}

async fn get(r: &axum::Router, path: &str) -> Answer {
    ask(r, "GET", path, &[]).await
}

/// A call of a command as the administrator, over REST.
async fn call(r: &axum::Router, command: &str, body: J) -> (u16, J, HeaderMap) {
    let req = Request::post(format!("/api/instance_brand/{command}"))
        .header("host", HOST)
        .header("authorization", "Bearer admin")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = r.clone().oneshot(req).await.unwrap();
    let (status, headers) = (resp.status().as_u16(), resp.headers().clone());
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(J::Null),
        headers,
    )
}

/// The `v` the spec defines, computed here from the bytes served and not by `obop-brand`: the first
/// 16 hex digits of their SHA-256.
fn version_of(bytes: &[u8]) -> String {
    obop_oauth::util::sha_hex(std::str::from_utf8(bytes).unwrap())[..16].to_string()
}

/// The artwork `set_brand` stores: the canonical text of the placeholder "M".
fn artwork() -> &'static str {
    FIELDS
        .iter()
        .find(|(k, _)| *k == "logo_svg")
        .map(|(_, v)| *v)
        .unwrap()
}

/// The `logo_svg` the store holds for the brand row, as stored.
fn stored_artwork(app: &App) -> String {
    let row = app
        .store
        .backend()
        .get("instance_brand", "instance")
        .unwrap()
        .expect("the brand row exists");
    row["logo_svg"].as_str().unwrap().to_string()
}

/// A record written straight to the store, around the commands, as a restore or an import would.
fn write_around_the_command(app: &App, model: &str, row: J) {
    app.store
        .backend()
        .apply(&[Op::Put {
            model: model.into(),
            id: "instance".into(),
            fields: row.as_object().unwrap().clone(),
        }])
        .unwrap();
}

// ---- No brand set -----------------------------------------------------------------------------

/// With no row a `GET` or `HEAD` of the path is answered exactly as the layout answers a
/// one-segment path of the API that no route owns (`/api/nothing`), status, headers and body, byte
/// for byte. In the API alone and beside the back office that is the JSON with a code and a hint;
/// in the public site's router it is the short JSON of `json_404`. A set brand shows itself by the
/// 200 and the artwork (public by design), and the route shows nothing else about the instance.
/// The route does not hide that it exists: a path of the same shape with two segments is matched by
/// `/api/{model}/{command}`, which takes `POST` only, so it answers 405 with `Allow: POST`, and the
/// logo path answers `POST` (and every other method but `GET` and `HEAD`) with 405 and `Allow:
/// GET,HEAD`; the relation is pinned below.
#[tokio::test]
async fn with_no_brand_the_logo_path_answers_as_an_unknown_api_path_in_every_layout() {
    for l in layouts() {
        for method in ["GET", "HEAD"] {
            let logo = ask(&l.router, method, PATH, &[]).await;
            let unknown = ask(&l.router, method, "/api/nothing", &[]).await;
            assert_eq!(logo.status, 404, "{} {method}", l.name);
            assert_eq!(logo.status, unknown.status, "{} {method}", l.name);
            assert_eq!(
                logo.header_lines(),
                unknown.header_lines(),
                "{} {method}: headers",
                l.name
            );
            assert_eq!(logo.body, unknown.body, "{} {method}: body", l.name);
            assert!(
                !logo.header("content-type").contains("svg"),
                "{} {method}: nothing of the logo",
                l.name
            );
            if method == "HEAD" {
                assert!(logo.body.is_empty(), "{}", l.name);
            }
        }
        // The same with a query string and with a validator: nothing changes the answer.
        let plain = get(&l.router, PATH).await;
        for (path, headers) in [
            (format!("{PATH}?v=0123456789abcdef"), vec![]),
            (format!("{PATH}?"), vec![]),
            (PATH.to_string(), vec![("if-none-match", "*")]),
            (PATH.to_string(), vec![("cookie", "obop_lang=sv")]),
        ] {
            let other = ask(&l.router, "GET", &path, &headers).await;
            assert_eq!(other.status, 404, "{} {path}", l.name);
            assert_eq!(other.body, plain.body, "{} {path}", l.name);
            assert_eq!(other.header_lines(), plain.header_lines(), "{}", l.name);
        }
        // What those answers are, written out.
        let body = plain.text();
        match l.name {
            "public_site" => assert_eq!(body, r#"{"error":"not found"}"#),
            _ => {
                let v: J = serde_json::from_str(&body).unwrap();
                assert_eq!(v["error"], "not found", "{}", l.name);
                assert_eq!(v["code"], "not_found", "{}", l.name);
            }
        }
        assert_eq!(
            plain.header("content-type"),
            "application/json",
            "{}",
            l.name
        );
        // The neighbour of the same shape (two segments, and a `.svg` at the end) is not an unknown
        // path of the API but a command of a model that does not exist: refused for its method.
        // The difference is the one the doc comment states, and no other.
        let shape = ask(&l.router, "GET", "/api/zzz/logo.svg", &[]).await;
        assert_eq!(shape.status, 405, "{}", l.name);
        assert_eq!(shape.header("allow"), "POST", "{}", l.name);
        assert!(shape.body.is_empty(), "{}", l.name);
        assert_ne!(shape.status, plain.status, "{}", l.name);
        let shape_head = ask(&l.router, "HEAD", "/api/zzz/logo.svg", &[]).await;
        assert_eq!(shape_head.status, 405, "{}", l.name);
        let post_shape = ask(&l.router, "POST", "/api/zzz/logo.svg", &[]).await;
        let post_logo = ask(&l.router, "POST", PATH, &[]).await;
        assert_eq!(
            post_shape.status, 401,
            "{}: reaches the command route",
            l.name
        );
        assert_eq!(post_logo.status, 405, "{}", l.name);
        assert_eq!(post_logo.header("allow"), "GET,HEAD", "{}", l.name);
        // A one-segment path that is not a route is what the logo path is answered as.
        for unknown in ["/api/nothing", "/api/logo.svg", "/api/brand"] {
            let u = ask(&l.router, "GET", unknown, &[]).await;
            assert_eq!(u.status, 404, "{} {unknown}", l.name);
            assert_eq!(u.body, plain.body, "{} {unknown}", l.name);
        }
    }
}

/// A `clear` puts the path back to the unknown path at once, in the process that cleared.
#[tokio::test]
async fn clearing_the_brand_takes_the_logo_away_at_once() {
    let l = plain();
    let (s, _, _) = call(&l.router, "configure", json!({"name": "MOHV"})).await;
    assert_eq!(s, 200);
    assert_eq!(get(&l.router, PATH).await.status, 200);
    let (s, v, _) = call(&l.router, "clear", json!({})).await;
    assert_eq!(s, 200, "{v}");
    let gone = get(&l.router, PATH).await;
    let unknown = get(&l.router, "/api/nothing").await;
    assert_eq!((gone.status, gone.body), (404, unknown.body));
}

// ---- A brand with an artwork ---------------------------------------------------------------------

/// The headers of the spec, the artwork byte for byte as the store holds it, `v` from the bytes
/// served, and a `Content-Length` that is theirs: in every layout, with no token and no cookie.
#[tokio::test]
async fn with_a_brand_the_stored_artwork_is_served_with_the_fixed_headers_in_every_layout() {
    for l in layouts() {
        set_brand(&l.app);
        let a = get(&l.router, PATH).await;
        assert_eq!(a.status, 200, "{}", l.name);
        assert_eq!(a.header("content-type"), SPEC_CONTENT_TYPE, "{}", l.name);
        assert_eq!(
            a.header("x-content-type-options"),
            SPEC_NOSNIFF,
            "{}",
            l.name
        );
        assert_eq!(a.header("content-security-policy"), SPEC_CSP, "{}", l.name);
        assert_eq!(a.header("cache-control"), SPEC_CACHE_CONTROL, "{}", l.name);
        // The constants that the shop favicon is to share (slice S1f, not built) say the same.
        assert_eq!(SPEC_CONTENT_TYPE, LOGO_CONTENT_TYPE);
        assert_eq!(SPEC_NOSNIFF, LOGO_NOSNIFF);
        assert_eq!(SPEC_CSP, LOGO_CSP);
        assert_eq!(SPEC_CACHE_CONTROL, LOGO_CACHE_CONTROL);
        // The artwork as the store holds it, byte for byte (it is in canonical form), and not as
        // the request or any other text had it.
        let stored = stored_artwork(&l.app);
        assert_eq!(a.text(), stored, "{}", l.name);
        assert_eq!(a.text(), artwork(), "{}", l.name);
        // `v` is the first 16 hex digits of the SHA-256 of the served bytes; the ETag is `v`, quoted.
        let v = version_of(&a.body);
        assert_eq!(v.len(), 16);
        assert_eq!(a.header("etag"), format!("\"{v}\""), "{}", l.name);
        assert_eq!(l.app.dispatcher.brand().current().v(), v, "{}", l.name);
        assert_eq!(
            a.header("content-length"),
            a.body.len().to_string(),
            "{}",
            l.name
        );
        // No sign-in, no cookie: the answer neither needs nor sets one.
        assert!(!a.has("set-cookie"), "{}", l.name);
        assert!(!a.has("www-authenticate"), "{}", l.name);
        // A token or a cookie that is wrong changes nothing: the route does not look at them.
        let with = ask(
            &l.router,
            "GET",
            PATH,
            &[
                ("authorization", "Bearer not-a-token"),
                ("cookie", "obop_session=none; obop_lang=de"),
                ("accept-language", "sv"),
            ],
        )
        .await;
        assert_eq!(with.status, 200, "{}", l.name);
        assert_eq!(with.body, a.body, "{}", l.name);
        assert_eq!(with.header_lines(), a.header_lines(), "{}", l.name);
    }
}

/// The query string is ignored: whatever it holds, the bytes and the headers are those of the path
/// alone (the `?v=` of the icon address is the browser's cache key and nothing more).
#[tokio::test]
async fn the_query_string_changes_nothing() {
    for l in layouts() {
        set_brand(&l.app);
        let base = get(&l.router, PATH).await;
        assert_eq!(base.status, 200);
        for query in [
            "?v=0123456789abcdef",
            "?v=",
            "?",
            "?v=other&x=1",
            "?v=%00",
            "?w=1#fragment",
        ] {
            let q = get(&l.router, &format!("{PATH}{query}")).await;
            assert_eq!(q.status, 200, "{} {query}", l.name);
            assert_eq!(q.body, base.body, "{} {query}", l.name);
            assert_eq!(q.header_lines(), base.header_lines(), "{} {query}", l.name);
        }
    }
}

/// `HEAD` is the `GET` without a body, headers and length included (it comes from the `GET` route).
/// Before the route existed a `HEAD` of the path was a 405 with `Allow: POST`; it is now the answer
/// of the route, a deliberate change of the answer of the path (spec, sections 4, 12 and 13). This
/// test holds it with a brand, the first test of this file without one, and the test over a socket
/// on the wire.
#[tokio::test]
async fn head_is_the_get_without_a_body() {
    for l in layouts() {
        set_brand(&l.app);
        let g = get(&l.router, PATH).await;
        let h = ask(&l.router, "HEAD", PATH, &[]).await;
        assert_eq!(h.status, 200, "{}", l.name);
        assert!(h.body.is_empty(), "{}", l.name);
        assert_eq!(h.header_lines(), g.header_lines(), "{}", l.name);
        assert_eq!(h.header("content-length"), g.body.len().to_string());
        // A `HEAD` with the validator is a 304, as the `GET` is, and its `Content-Length` is that of
        // the 200 (the handler sets it, RFC 9110 section 8.6 allows no other value; the server layer
        // sends it with a `HEAD` 304, and `the_304_on_the_wire_...` holds it over a socket).
        let etag = g.header("etag").to_string();
        let h304 = ask(&l.router, "HEAD", PATH, &[("if-none-match", &etag)]).await;
        assert_eq!(h304.status, 304, "{}", l.name);
        assert!(h304.body.is_empty());
        assert_eq!(
            h304.header("content-length"),
            g.body.len().to_string(),
            "{}",
            l.name
        );
    }
}

/// The validator: quoted, a strong or weak match of any entry of the list or `*` is a 304 that
/// carries what a 304 must carry (the `ETag` and `Cache-Control`) and the security headers, and no
/// body and no `Content-Type`; anything else is the whole answer. The `Content-Length` of a `GET` 304
/// is not asserted here: the handler sets the length of the 200 on it, the HTTP server omits it from
/// a `GET` 304, and a router called without a server shows the handler's header, which no client
/// sees. What a client sees, on a `GET` and on a `HEAD`, is held over a socket by
/// `the_304_on_the_wire_...`.
#[tokio::test]
async fn a_matching_validator_is_a_304_with_the_headers_a_304_carries() {
    for l in layouts() {
        set_brand(&l.app);
        let full = get(&l.router, PATH).await;
        let etag = full.header("etag").to_string();
        assert!(
            etag.starts_with('"') && etag.ends_with('"') && etag.len() == 18,
            "{}: {etag}",
            l.name
        );
        let weak = format!("W/{etag}");
        let list = format!("\"zzz\", {etag}");
        let weak_list = format!("\"zzz\" ,W/{etag}, \"yyy\"");
        for inm in [
            etag.as_str(),
            weak.as_str(),
            list.as_str(),
            weak_list.as_str(),
            "*",
        ] {
            let a = ask(&l.router, "GET", PATH, &[("if-none-match", inm)]).await;
            assert_eq!(a.status, 304, "{}: {inm}", l.name);
            assert!(a.body.is_empty(), "{}: {inm}", l.name);
            assert_eq!(a.header("etag"), etag, "{}: {inm}", l.name);
            assert_eq!(a.header("cache-control"), SPEC_CACHE_CONTROL, "{}", l.name);
            assert_eq!(a.header("x-content-type-options"), SPEC_NOSNIFF);
            assert_eq!(a.header("content-security-policy"), SPEC_CSP);
            assert!(!a.has("content-type"), "{}: {inm}", l.name);
            assert!(!a.has("set-cookie"), "{}: {inm}", l.name);
        }
        // Two header lines are one list.
        let two = ask(
            &l.router,
            "GET",
            PATH,
            &[("if-none-match", "\"zzz\""), ("if-none-match", &etag)],
        )
        .await;
        assert_eq!(two.status, 304, "{}", l.name);
        // What is not this validator: the whole answer.
        let bare = etag.trim_matches('"').to_string();
        for inm in [
            "\"zzz\"",
            "",
            bare.as_str(),
            "\"0000000000000000\"",
            &format!("{etag}x"),
        ] {
            let a = ask(&l.router, "GET", PATH, &[("if-none-match", inm)]).await;
            assert_eq!(a.status, 200, "{}: {inm:?}", l.name);
            assert_eq!(a.body, full.body, "{}: {inm:?}", l.name);
        }
    }
}

/// A new artwork through the command shows on the next request of the same process, with a new
/// `ETag` (so the old validator is no longer a match), and `clear` and the removal of the artwork
/// do the same: the reader's cell follows the command at once, not after two seconds.
#[tokio::test]
async fn a_configure_through_the_command_shows_on_the_route_at_once() {
    for l in layouts() {
        // The reader has the default in its cell.
        assert_eq!(get(&l.router, PATH).await.status, 404, "{}", l.name);
        // A name alone: the generated tile.
        let (s, v, _) = call(&l.router, "configure", json!({"name": "MOHV"})).await;
        assert_eq!(s, 200, "{}: {v}", l.name);
        let tile = get(&l.router, PATH).await;
        assert_eq!(tile.status, 200, "{}", l.name);
        assert_eq!(tile.text(), obop_brand::tile::svg("MOHV", None, None));
        assert!(tile.text().contains(">M</text>"), "the glyph of the name");
        assert!(!tile.text().contains("obop"));
        assert_eq!(
            tile.header("etag"),
            format!("\"{}\"", version_of(&tile.body))
        );
        // The artwork replaces it.
        let (s, v, _) = call(&l.router, "configure", json!({"logo_svg": artwork()})).await;
        assert_eq!(s, 200, "{}: {v}", l.name);
        let art = get(&l.router, PATH).await;
        assert_eq!(art.text(), artwork(), "{}", l.name);
        assert_ne!(art.header("etag"), tile.header("etag"), "{}", l.name);
        // The validator of the tile is not a match for the artwork.
        let stale = ask(
            &l.router,
            "GET",
            PATH,
            &[("if-none-match", tile.header("etag"))],
        )
        .await;
        assert_eq!(stale.status, 200, "{}", l.name);
        assert_eq!(stale.text(), artwork());
        // The name alone changes the tile and not the artwork; removing the artwork gives the tile
        // of the new name.
        let (s, _, _) = call(&l.router, "configure", json!({"name": "Zed"})).await;
        assert_eq!(s, 200);
        assert_eq!(get(&l.router, PATH).await.body, art.body, "{}", l.name);
        let (s, _, _) = call(&l.router, "configure", json!({"logo_svg": ""})).await;
        assert_eq!(s, 200);
        let zed = get(&l.router, PATH).await;
        assert_eq!(zed.text(), obop_brand::tile::svg("Zed", None, None));
        assert!(zed.text().contains(">Z</text>"));
        // A `clear` removes the logo with the row.
        let (s, _, _) = call(&l.router, "clear", json!({})).await;
        assert_eq!(s, 200);
        assert_eq!(get(&l.router, PATH).await.status, 404, "{}", l.name);
    }
}

/// The tile follows the accent the theme stores, and so do `v` and the `ETag`: a theme change alone
/// is a new logo. (It reaches the route within the reader's two seconds; here the theme is stored
/// before the reader's first look at it.)
#[tokio::test]
async fn the_tile_takes_the_accent_of_the_theme_and_its_version_follows() {
    let slate = plain();
    let (s, _, _) = call(&slate.router, "configure", json!({"name": "MOHV"})).await;
    assert_eq!(s, 200);
    let slate = get(&slate.router, PATH).await;
    assert_eq!(slate.text(), obop_brand::tile::svg("MOHV", None, None));

    let l = plain();
    write_around_the_command(
        &l.app,
        "obop_setting",
        json!({"value": r##"{"theme":{"accent":"#b45309"}}"##}),
    );
    let (s, _, _) = call(&l.router, "configure", json!({"name": "MOHV"})).await;
    assert_eq!(s, 200);
    let amber = get(&l.router, PATH).await;
    assert_eq!(
        amber.text(),
        obop_brand::tile::svg("MOHV", Some("#b45309"), None)
    );
    assert_ne!(amber.body, slate.body);
    assert_ne!(amber.header("etag"), slate.header("etag"));
}

// ---- A stored row that fails the read check -------------------------------------------------------

/// A stored artwork that fails the read check (a script, a link, a byte order mark) is never
/// served: the route sends the tile of the name, and its `ETag` is the tile's.
#[tokio::test]
async fn a_stored_artwork_that_fails_the_read_check_is_served_as_the_tile() {
    let hostile = [
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\"><script>alert(1)</script></svg>",
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\"><a href=\"https://x.example\"><rect width=\"1\" height=\"1\"/></a></svg>",
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\" onload=\"x()\"><rect width=\"1\" height=\"1\"/></svg>",
        "\u{feff}<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\"><rect width=\"1\" height=\"1\"/></svg>",
        "</svg><form action=\"https://x.example\">",
        "not an svg at all",
    ];
    for (i, svg) in hostile.iter().enumerate() {
        // A reader that has not looked yet, in each layout: the cell of a reader that has holds
        // what it read for two seconds.
        for l in layouts() {
            write_around_the_command(
                &l.app,
                "instance_brand",
                json!({"name": "MOHV", "logo_svg": svg}),
            );
            let a = get(&l.router, PATH).await;
            assert_eq!(a.status, 200, "{} #{i}", l.name);
            let tile = obop_brand::tile::svg("MOHV", None, None);
            assert_eq!(
                a.text(),
                tile,
                "{} #{i}: the tile, not the stored text",
                l.name
            );
            assert!(!a.text().contains("<script"), "{} #{i}", l.name);
            assert!(!a.text().contains("href"), "{} #{i}", l.name);
            assert_eq!(a.header("etag"), format!("\"{}\"", version_of(&a.body)));
            // `get` says which field fell back, and why.
            let (s, v, _) = call(&l.router, "get", json!({})).await;
            assert_eq!(s, 200, "{v}");
            let fallbacks: J =
                serde_json::from_str(v["value"]["fallbacks"].as_str().unwrap()).unwrap();
            assert_eq!(fallbacks[0]["field"], "logo_svg", "{} #{i}: {v}", l.name);
        }
    }
}

/// A stored artwork that parses but is not in canonical form is no fallback: the route serves the
/// printer's output for it, and the `ETag` is the version of those bytes. The stored text is served
/// nowhere.
#[tokio::test]
async fn a_valid_artwork_that_is_not_canonical_is_served_re_serialised() {
    let stored = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64.0 64\">\n  <rect width=\"64.00\" height=\"64\" rx=\".5\" fill=\"#495670\"/>\n</svg>\n";
    let canonical = obop_brand::svg::parse_str(stored).unwrap().print();
    assert_ne!(canonical, stored, "the case is not canonical to begin with");
    let l = plain();
    write_around_the_command(
        &l.app,
        "instance_brand",
        json!({"name": "MOHV", "logo_svg": stored}),
    );
    let a = get(&l.router, PATH).await;
    assert_eq!(a.status, 200);
    assert_eq!(a.text(), canonical);
    assert_ne!(a.text(), stored);
    assert!(a.text().contains("rx=\"0.5\""), "{}", a.text());
    assert_eq!(
        a.header("etag"),
        format!("\"{}\"", version_of(a.text().as_bytes()))
    );
    let (s, v, _) = call(&l.router, "get", json!({})).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["value"]["fallbacks"], "[]", "no fallback: {v}");
}

// ---- Other methods --------------------------------------------------------------------------------

/// A `POST` to the path is refused by the route that owns the path and no longer reaches
/// `/api/{model}/{command}`: 405, with and without a token, and with `Allow` naming the methods of
/// the route. (Before the route existed it was a 401 without a token and a 404 `not_found` JSON
/// with one: the golden digests of the first commit say so.)
#[tokio::test]
async fn a_post_to_the_logo_path_is_a_405_and_does_not_reach_the_command_route() {
    for l in layouts() {
        for headers in [vec![], vec![("authorization", "Bearer admin")]] {
            let a = ask(&l.router, "POST", PATH, &headers).await;
            assert_eq!(a.status, 405, "{} {headers:?}", l.name);
            assert!(a.body.is_empty(), "{}: no body: {}", l.name, a.text());
            let allow = a.header("allow");
            assert!(
                allow.contains("GET") && !allow.contains("POST"),
                "{}: Allow: {allow}",
                l.name
            );
            assert!(!a.has("www-authenticate"), "{}", l.name);
        }
        // With a brand it is the same.
        set_brand(&l.app);
        let a = ask(
            &l.router,
            "POST",
            PATH,
            &[("authorization", "Bearer admin")],
        )
        .await;
        assert_eq!(a.status, 405, "{}", l.name);
        // The command route itself is untouched: a command that does not exist is a 404.
        let (s, _, _) = call(&l.router, "nothing", json!({})).await;
        assert_eq!(s, 404, "{}", l.name);
    }
}

/// Every method but `GET` and `HEAD` is refused by the route that owns the path: 405, `Allow:
/// GET,HEAD` and no body, with a brand and without one, with a token and without (it was `Allow:
/// POST` before the route existed; the golden digests hold the `GET` and the `POST` only, so this is
/// the test that holds the others). An `OPTIONS` that the cross-origin layer does not take is one of
/// them; with `OBOP_ALLOWED_ORIGINS` set the layer answers a preflight first, see below. A `HEAD` is
/// not among them (it is the `GET` without the body: `head_is_the_get_without_a_body`).
#[tokio::test]
async fn every_other_method_on_the_logo_path_is_a_405_that_names_get_and_head() {
    for l in layouts() {
        for brand in [false, true] {
            if brand {
                set_brand(&l.app);
            }
            for method in ["PUT", "PATCH", "DELETE", "OPTIONS", "POST"] {
                for headers in [vec![], vec![("authorization", "Bearer admin")]] {
                    let a = ask(&l.router, method, PATH, &headers).await;
                    let what = format!("{} {method} brand={brand} {headers:?}", l.name);
                    assert_eq!(a.status, 405, "{what}");
                    assert_eq!(a.header("allow"), "GET,HEAD", "{what}");
                    assert!(a.body.is_empty(), "{what}");
                    assert!(!a.has("www-authenticate"), "{what}");
                }
            }
        }
    }
}

// ---- Cross-origin access -------------------------------------------------------------------------

/// The `access-control-*` headers of an answer, sorted, as `name: value`.
fn cross_origin(a: &Answer) -> Vec<String> {
    a.header_lines()
        .into_iter()
        .filter(|l| l.starts_with("access-control-"))
        .collect()
}

/// The route adds no cross-origin header of its own. With `OBOP_ALLOWED_ORIGINS` set, the layer that
/// every route goes through decides, and the logo route is answered exactly as `/api/openapi.json`,
/// the other public route, is: no `Access-Control-Allow-Origin` without an `Origin` or for one that
/// is not listed, the listed one echoed (never `*`) with `Vary: Origin` and no credentials, on the
/// 200 and on the 304, and a preflight as for the other route: a request the layer accepts from the
/// listed origin is a 204 that echoes it, one from another origin is a 403 with nothing of the
/// kind, and the `Allow` of both names the methods of the route that owns the path.
#[tokio::test]
async fn the_logo_route_has_the_cross_origin_headers_of_the_layer_and_none_of_its_own() {
    const LISTED: &str = "https://app.example.com";
    const FOREIGN: &str = "https://elsewhere.example";
    let app = App::new();
    let router = obop_http::router(
        obop_http::HttpState::with_mcp(
            app.dispatcher.clone(),
            Arc::new(Tokens),
            McpConfig {
                allowed_origins: vec![LISTED.to_string()],
                ..McpConfig::default()
            },
        ),
        None,
    );
    set_brand(&app);
    let etag = get(&router, PATH).await.header("etag").to_string();
    for origin in [None, Some(FOREIGN), Some(LISTED)] {
        let headers: Vec<(&str, &str)> = origin.map(|o| ("origin", o)).into_iter().collect();
        let mut revalidating = headers.clone();
        revalidating.push(("if-none-match", &etag));
        for (answered, status) in [(&headers, 200), (&revalidating, 304)] {
            let logo = ask(&router, "GET", PATH, answered).await;
            let other = ask(&router, "GET", "/api/openapi.json", &headers).await;
            let what = format!("origin {origin:?} -> {status}");
            assert_eq!(logo.status, status, "{what}");
            assert_eq!(other.status, 200, "{what}");
            assert_eq!(cross_origin(&logo), cross_origin(&other), "{what}");
            assert!(!logo.has("access-control-allow-credentials"), "{what}");
            assert_ne!(logo.header("access-control-allow-origin"), "*", "{what}");
            if origin == Some(LISTED) {
                assert_eq!(logo.header("access-control-allow-origin"), LISTED, "{what}");
                assert!(
                    logo.header("vary").to_lowercase().contains("origin"),
                    "{what}"
                );
            } else {
                assert!(!logo.has("access-control-allow-origin"), "{what}");
            }
        }
    }
    // A preflight is the layer's: the same as for the other route, and nothing from the handler.
    // The request headers it asks for are ones the layer knows (`accept`, `authorization`) or none:
    // the layer refuses a list with a name it does not know for every origin, so a preflight that
    // asked for one would show nothing of the listed origin (the case after the loop).
    for origin in [FOREIGN, LISTED] {
        for asked in [Some("accept, authorization"), None] {
            let mut h = vec![("origin", origin), ("access-control-request-method", "GET")];
            h.extend(asked.map(|list| ("access-control-request-headers", list)));
            let logo = ask(&router, "OPTIONS", PATH, &h).await;
            let other = ask(&router, "OPTIONS", "/api/openapi.json", &h).await;
            let what = format!("preflight from {origin} asking for {asked:?}");
            assert_eq!(logo.status, other.status, "{what}");
            assert_eq!(cross_origin(&logo), cross_origin(&other), "{what}");
            assert!(logo.body.is_empty(), "{what}");
            assert!(!logo.has("access-control-allow-credentials"), "{what}");
            assert_ne!(logo.header("access-control-allow-origin"), "*", "{what}");
            if origin == LISTED {
                // Accepted: 204, the listed origin echoed (never `*`) with `Vary: Origin`, a
                // list of methods that includes `GET`, and no credentials. The list is the
                // layer's, not the route's (the `Allow` below is the route's): that it is the
                // same as for the other route is the comparison of the `access-control-*` headers
                // above, and its wording is pinned by `cors_tests.rs`, not here.
                assert_eq!(logo.status, 204, "{what}");
                assert_eq!(logo.header("access-control-allow-origin"), LISTED, "{what}");
                assert!(logo.header("access-control-allow-methods").contains("GET"));
                assert!(
                    logo.header("vary").to_lowercase().contains("origin"),
                    "{what}"
                );
            } else {
                assert_eq!(logo.status, 403, "{what}");
                assert!(!logo.has("access-control-allow-origin"), "{what}");
            }
            // The `Allow` of that answer names the methods of the route that owns the path, as it
            // does for the other route. It was `POST` while the path belonged to
            // `/api/{model}/{command}`: a deliberate change of the answer of the path (spec,
            // sections 4, 12 and 13).
            assert_eq!(logo.header("allow"), "GET,HEAD", "{what}");
            assert_eq!(logo.header("allow"), other.header("allow"), "{what}");
        }
    }
    // A preflight that asks for a header the layer does not know (`if-none-match`) is refused, for
    // the listed origin too, and for the other route alike: the 403 has no `access-control-*`.
    let h = [
        ("origin", LISTED),
        ("access-control-request-method", "GET"),
        ("access-control-request-headers", "if-none-match"),
    ];
    let logo = ask(&router, "OPTIONS", PATH, &h).await;
    let other = ask(&router, "OPTIONS", "/api/openapi.json", &h).await;
    assert_eq!((logo.status, other.status), (403, 403));
    assert!(cross_origin(&logo).is_empty() && cross_origin(&other).is_empty());
}

// ---- On the wire ----------------------------------------------------------------------------------

/// What a client of a socket reads: the status line, the header lines (the names in lower case, as
/// the server sends them) and the body.
struct Wire {
    status: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Wire {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// The names of the headers, sorted, without `date` and `connection`, which the server sets and
    /// the route and its layers do not.
    fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .headers
            .iter()
            .map(|(k, _)| k.as_str())
            .filter(|k| !matches!(*k, "date" | "connection"))
            .collect();
        names.sort_unstable();
        names
    }
}

/// The router served on a loopback socket the way `obop_http::serve` serves it, and its address.
async fn on_a_socket(router: axum::Router) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    addr
}

/// One request over TCP, read to the end of the connection (it says `Connection: close`), with a
/// bound on every read, so that a server that does not answer fails the test and does not hang it.
async fn on_the_wire(addr: std::net::SocketAddr, method: &str, headers: &[(&str, &str)]) -> Wire {
    use std::io::{Read, Write};
    let mut request = format!("{method} {PATH} HTTP/1.1\r\nHost: {HOST}\r\nConnection: close\r\n");
    for (k, v) in headers {
        request.push_str(&format!("{k}: {v}\r\n"));
    }
    request.push_str("\r\n");
    tokio::task::spawn_blocking(move || {
        let mut s = std::net::TcpStream::connect(addr).unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(20)))
            .unwrap();
        s.write_all(request.as_bytes()).unwrap();
        let mut raw = Vec::new();
        s.read_to_end(&mut raw).expect("the server answered");
        let split = raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .expect("a response head");
        let head = String::from_utf8(raw[..split].to_vec()).unwrap();
        let mut lines = head.split("\r\n");
        let status = lines.next().unwrap().to_string();
        let headers = lines
            .map(|l| {
                let (k, v) = l.split_once(':').expect("a header line");
                (k.trim().to_ascii_lowercase(), v.trim().to_string())
            })
            .collect();
        Wire {
            status,
            headers,
            body: raw[split + 4..].to_vec(),
        }
    })
    .await
    .unwrap()
}

/// What a client sees of a 304, over a real socket, because a router called without a server shows
/// the headers the handler set and not the ones the server sends. The handler sets the length of
/// the 200 on a 304; the HTTP server sends it on a `HEAD` 304 and leaves it out of a `GET` 304
/// (RFC 9110 section 8.6 allows both), and `Transfer-Encoding` is on neither. The header sets are
/// pinned whole: a `GET` 304 is the validators and the security headers, and a `HEAD` 304 is the
/// same with the length; a 200 and a `HEAD` 200 carry the length and the type.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_304_on_the_wire_carries_the_length_on_a_head_and_none_on_a_get() {
    for l in layouts() {
        set_brand(&l.app);
        let addr = on_a_socket(l.router.clone()).await;
        let what = l.name;

        let full = on_the_wire(addr, "GET", &[]).await;
        assert_eq!(full.status, "HTTP/1.1 200 OK", "{what}");
        let length = full.body.len().to_string();
        assert_eq!(
            full.header("content-length"),
            Some(length.as_str()),
            "{what}"
        );
        assert_eq!(full.body, artwork().as_bytes(), "{what}");
        let etag = full.header("etag").expect("an ETag").to_string();
        let head = on_the_wire(addr, "HEAD", &[]).await;
        assert_eq!(head.status, "HTTP/1.1 200 OK", "{what}");
        assert!(head.body.is_empty(), "{what}");
        assert_eq!(
            head.header("content-length"),
            Some(length.as_str()),
            "{what}"
        );
        assert_eq!(head.names(), full.names(), "{what}: HEAD 200");

        let revalidating = [("if-none-match", etag.as_str())];
        let get_304 = on_the_wire(addr, "GET", &revalidating).await;
        assert_eq!(get_304.status, "HTTP/1.1 304 Not Modified", "{what}");
        assert!(get_304.body.is_empty(), "{what}");
        assert_eq!(get_304.header("etag"), Some(etag.as_str()), "{what}");
        assert_eq!(get_304.header("cache-control"), Some("no-cache"), "{what}");
        assert_eq!(
            get_304.names(),
            GET_304_HEADERS,
            "{what}: the headers of a GET 304 on the wire"
        );

        let head_304 = on_the_wire(addr, "HEAD", &revalidating).await;
        assert_eq!(head_304.status, "HTTP/1.1 304 Not Modified", "{what}");
        assert!(head_304.body.is_empty(), "{what}");
        assert_eq!(
            head_304.header("content-length"),
            Some(length.as_str()),
            "{what}: the length of the 200"
        );
        assert_eq!(
            head_304.names(),
            HEAD_304_HEADERS,
            "{what}: the headers of a HEAD 304 on the wire"
        );
    }
}

/// The names of the headers of a 304 as the server sends it: the validator, the cache policy and
/// the two security headers of the 200, and no `Content-Type`, no `Content-Length` (a `GET` 304) and
/// no `Transfer-Encoding`.
const GET_304_HEADERS: [&str; 4] = [
    "cache-control",
    "content-security-policy",
    "etag",
    "x-content-type-options",
];

/// The same with the length of the 200, which the server sends with a `HEAD` 304.
const HEAD_304_HEADERS: [&str; 5] = [
    "cache-control",
    "content-length",
    "content-security-policy",
    "etag",
    "x-content-type-options",
];

// ---- Hosting --------------------------------------------------------------------------------------

/// In the instance with the public site the back office is served under `/admin`, and the route is
/// the one at the root, which is what the tab icon of a page under `/admin` links to: neither the
/// public site nor the back office build takes the path, and under the prefix the path is the back
/// office's own (its shell), not the logo.
#[tokio::test]
async fn under_the_admin_prefix_the_route_is_still_at_the_root() {
    let l = layouts().pop().unwrap();
    assert_eq!(l.name, "public_site");
    set_brand(&l.app);
    let at_root = get(&l.router, PATH).await;
    assert_eq!(at_root.status, 200);
    assert_eq!(at_root.text(), artwork());
    // The back office is where it was.
    let shell = get(&l.router, "/admin/").await;
    assert_eq!(shell.status, 200);
    assert!(shell.text().contains("BACKOFFICE-SHELL"));
    // Under the prefix the path is the back office build's: its shell, never the logo.
    let under = get(&l.router, &format!("/admin{PATH}")).await;
    assert_eq!(under.status, 200);
    assert!(
        under.text().contains("BACKOFFICE-SHELL"),
        "{}",
        under.text()
    );
    assert!(!under.header("content-type").contains("svg"));
    // The public site's own pages are not shadowed either.
    assert_ne!(get(&l.router, "/favicon.svg").await.text(), artwork());
}

// ---- The store that fails (D74) -------------------------------------------------------------------

fn flaky_app() -> (Arc<Flaky>, App) {
    let (flaky, backend) = Flaky::memory();
    (flaky, App::with_store(Store::over(backend)))
}

fn router_of(app: &App) -> axum::Router {
    obop_http::router(state(app), None)
}

/// While the reader's cell is fresh (two seconds after the read that filled it) the route asks the
/// cell and not the store, so a store that fails in the meantime is not met and the logo is served.
/// The test needs a moment inside those two seconds; on a machine too slow to give it, it starts
/// again with a fresh read, and it fails, never skips, if five tries were each too slow.
#[tokio::test]
async fn while_the_cell_is_fresh_a_failing_store_is_not_met() {
    for _try in 0..5 {
        let (flaky, app) = flaky_app();
        let router = router_of(&app);
        // Taken before the read that fills the cell, so it is never younger than the cell.
        let started = std::time::Instant::now();
        set_brand(&app);
        let good = get(&router, PATH).await;
        assert_eq!(good.status, 200);
        flaky.refuse_model("instance_brand", Mode::Reads);
        let cached = get(&router, PATH).await;
        if started.elapsed() >= std::time::Duration::from_millis(1500) {
            continue;
        }
        assert_eq!(cached.status, 200, "{}", cached.text());
        assert_eq!(cached.body, good.body);
        assert_eq!(flaky.refused(), 0, "the cache-hit path reads nothing");
        return;
    }
    panic!("five tries were each slower than 1.5 s: the machine cannot run this test");
}

/// The bound of the reader's cell: the constant `DEFAULT_BOUND` of
/// `crates/kernel/obop-commands/src/brand/reader.rs` (not exported, and pinned at two seconds by
/// `brand/tests/cache.rs`). The dispatcher's reader has this bound, and a test in this crate cannot
/// make it shorter or expire the cell by hand. If the constant grows, the tests below that wait
/// for the cell to go stale fail loudly (the request they expect to be refused is answered 200, or
/// the wait is far too long), not silently. If it shrinks they do not notice: what they wait for is
/// two seconds, and a shorter cell is stale by then, so every one of them still passes (with one
/// second all of this module passes). A shorter bound is held by the pin in
/// `brand/tests/cache.rs` (`the_bound_is_two_seconds`) alone.
const READER_BOUND: std::time::Duration = std::time::Duration::from_secs(2);

/// A `GET /healthz` that the runtime's worker must poll, answered within `bound`, and the answer.
/// The test body runs on the thread that drives the runtime (`block_on`), not on a worker, so a
/// request polled there is not delayed by a worker that is held in the store; a task spawned on
/// the runtime is. The wait for the task is not under `tokio::time::timeout`, because a held
/// worker is also the thread that drives that timer; it is the elapsed time that is checked, and
/// the hold in the store has a bound of its own (30 seconds), so a runtime that is held fails the
/// test and does not hang it.
async fn healthz_within(router: &axum::Router, bound: std::time::Duration, why: &str) -> Answer {
    let r = router.clone();
    let started = std::time::Instant::now();
    let answer = tokio::spawn(async move { ask(&r, "GET", "/healthz", &[]).await })
        .await
        .unwrap();
    assert!(
        started.elapsed() < bound,
        "/healthz was answered only after {:?} ({why}): the runtime is held",
        started.elapsed()
    );
    answer
}

/// Returns when the cell that the read before `filled` made is stale. `filled` is an instant taken
/// after that read: the cell is dated from the start of the read, so it is at least as old as
/// `filled` is, and it is stale once the bound has gone by since `filled`, however long the machine
/// took between the read and the instant. The loop is on the clock, so an early wake-up does not
/// end it early.
fn wait_until_stale(filled: std::time::Instant) {
    let stale = READER_BOUND + std::time::Duration::from_millis(50);
    while filled.elapsed() <= stale {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// After a good read, a store that then fails: once the cell has expired, the request that runs the
/// refill meets the fault and is answered by D74, 503 with `Retry-After`, never with the logo and
/// never with a page that says `obop`; the next request reads again (a failed refill is not kept)
/// and is answered the same way; after the store recovers, the logo is back. (While the cell is
/// fresh the store is not met: the test above.)
#[tokio::test]
async fn after_a_good_read_a_failing_store_answers_the_request_that_runs_the_refill_with_a_503() {
    let (flaky, app) = flaky_app();
    let router = router_of(&app);
    set_brand(&app);
    let good = get(&router, PATH).await;
    assert_eq!(good.status, 200);
    let logo = good.body.clone();
    // Taken after the read that filled the cell (`set_brand` read it): the cell is dated from the
    // start of that read, so it is at least as old as this instant.
    let filled = std::time::Instant::now();

    flaky.refuse_model("instance_brand", Mode::Reads);
    wait_until_stale(filled);

    let refused_before = flaky.refused();
    let first = get(&router, PATH).await;
    assert_eq!(first.status, 503, "{}", first.text());
    assert_eq!(first.header("retry-after"), "1");
    assert_eq!(first.header("x-obop-applied"), "none");
    let body: J = serde_json::from_slice(&first.body).unwrap();
    assert_eq!(body["code"], "storage_busy", "{body}");
    assert!(!first.text().contains("<svg"), "not the logo");
    assert!(
        flaky.refused() > refused_before,
        "the refill read the store"
    );

    // Nothing was kept: the next request reads again, and is answered the same way.
    let refused_before = flaky.refused();
    let second = get(&router, PATH).await;
    assert_eq!(second.status, 503, "{}", second.text());
    assert_eq!(second.header("retry-after"), "1");
    assert!(
        flaky.refused() > refused_before,
        "a failed refill is not kept: the next call read again"
    );

    // The store recovers: the next request reads, and the logo is back.
    flaky.allow_models();
    let back = get(&router, PATH).await;
    assert_eq!(back.status, 200);
    assert_eq!(back.body, logo);
}

/// `get` under the same failure is the error, never `set: false` with the defaults: a person who
/// asks what the brand is is told that the store cannot be read.
#[tokio::test]
async fn get_under_a_failing_store_is_the_error_and_never_not_set() {
    let (flaky, app) = flaky_app();
    let router = router_of(&app);
    set_brand(&app);
    let (s, v, _) = call(&router, "get", json!({})).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["value"]["set"], true);

    flaky.refuse_model("instance_brand", Mode::Reads);
    for _ in 0..2 {
        let (s, v, h) = call(&router, "get", json!({})).await;
        assert_eq!(s, 503, "{v}");
        assert_eq!(h["retry-after"], "1");
        assert_eq!(v["code"], "storage_busy", "{v}");
        assert!(v.get("value").is_none(), "not an answer: {v}");
        assert!(!v.to_string().contains("\"set\""), "never set: false: {v}");
    }
    // The layer above would answer 503 whatever the command said, because the request met a fault;
    // so the command itself is asked too, outside any request: it must fail, not say "not set".
    let asked = app.dispatcher.call(
        obop_commands::Surface::Cli,
        &Principal::user("ada", &[ADMIN_GROUP]),
        &mut obop_commands::Vault::new(),
        "instance_brand.get",
        &obop_commands::Value::map([]),
    );
    let text = match asked {
        Err(e) => format!("{e:?}"),
        Ok(_) => panic!("a store that cannot be read is an error, not `set: false`"),
    };
    assert!(
        text.contains("storage_busy") || text.contains("connections"),
        "{text}"
    );

    flaky.allow_models();
    let (s, v, _) = call(&router, "get", json!({})).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["value"]["set"], true);
}

/// A process that never had a good read: the request that runs the first refill meets the fault and
/// is answered by D74; nothing is pinned, neither the default nor the failure, so when the store
/// answers the next request is the answer of the store (here: no row, the default, the 404 of an
/// unknown path). A refill that fails on the second of its two reads (the theme setting, after the
/// row was read) fails as a whole.
#[tokio::test]
async fn a_process_that_never_had_a_good_read_is_answered_by_d74_and_pins_nothing() {
    for model in ["instance_brand", "obop_setting"] {
        let (flaky, app) = flaky_app();
        let router = router_of(&app);
        flaky.refuse_model(model, Mode::Reads);
        let first = get(&router, PATH).await;
        assert_eq!(first.status, 503, "{model}: {}", first.text());
        assert_eq!(first.header("retry-after"), "1", "{model}");
        let again = get(&router, PATH).await;
        assert_eq!(again.status, 503, "{model}: read again, nothing kept");
        flaky.allow_models();
        let unknown = get(&router, "/api/nothing").await;
        let healed = get(&router, PATH).await;
        assert_eq!(healed.status, 404, "{model}: the default, no row");
        assert_eq!(healed.body, unknown.body, "{model}");
    }
}

/// A caller that arrives behind the refill that fails, in a process that never had a good read,
/// waits for it and is answered with the default (here the 404), not with an error; the request
/// that ran the refill is the one that is answered 503. The refill is held inside the store, after
/// its first read, until the second request is on its way; which of the two paths the second
/// request takes is not asserted (it waits, or it arrives after the failure and reads for itself
/// and gets the row's absence), only that it is never an error and the first always is.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_request_that_arrives_behind_a_failing_first_refill_is_not_answered_with_an_error() {
    let (flaky, app) = flaky_app();
    let router = router_of(&app);
    let (parked_tx, parked_rx) = std::sync::mpsc::channel::<()>();
    let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
    let f = flaky.clone();
    flaky.after_the_get_of(
        "instance_brand",
        0,
        Box::new(move |_| {
            // The refill has read the row; its next read, of the theme, is refused.
            f.refuse_next(1);
            parked_tx.send(()).unwrap();
            // A bound, so that a reader that is asked on the thread that polls the request (and
            // holds the runtime still while it is parked here) fails the test instead of hanging it.
            let _ = go_rx.recv_timeout(std::time::Duration::from_secs(30));
        }),
    );
    let first = {
        let r = router.clone();
        tokio::spawn(async move { get(&r, PATH).await })
    };
    // Bounded, so that a route that never reads the store through the reader fails this test (the
    // sender lives inside the double, so an unbounded `recv` would wait for ever).
    let parked = tokio::task::spawn_blocking(move || {
        parked_rx.recv_timeout(std::time::Duration::from_secs(20))
    })
    .await
    .unwrap();
    assert!(
        parked.is_ok(),
        "the first request never reached the store: the route does not read it through the reader"
    );
    let second = {
        let r = router.clone();
        tokio::spawn(async move { get(&r, PATH).await })
    };
    // Let the second request reach the reader before the first is let go.
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
    std::thread::sleep(std::time::Duration::from_millis(100));
    go_tx.send(()).unwrap();
    let (first, second) = (first.await.unwrap(), second.await.unwrap());
    assert_eq!(
        first.status,
        503,
        "the request that ran the refill: {}",
        first.text()
    );
    assert_eq!(first.header("retry-after"), "1");
    assert_eq!(second.status, 404, "behind it: the default, no error");
}

/// What a refill that is held inside the store does to the server (spec, section 13, "A blocked
/// store"), through the route: the request that runs the refill waits for the store, and nothing
/// else does. The runtime has one worker, so a read on the thread that polls the requests would stop
/// everything below. While the refill is parked, `/healthz` answers, and a hundred requests to the
/// logo route are answered with the last good logo, before the refill is let go; the double counts
/// one read for all of it, the owner's (the route has one flight, and the callers read nothing).
/// After the release the owner is answered with the logo and the cell is fresh again. Every wait is
/// on a condition and has a bound, so a route that waits for the refill fails the test.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn while_a_refill_is_parked_in_the_store_the_logo_is_served_from_the_last_good_value() {
    let (flaky, app) = flaky_app();
    let router = router_of(&app);
    set_brand(&app);
    let good = get(&router, PATH).await;
    assert_eq!(good.status, 200);
    let filled = std::time::Instant::now();
    tokio::task::spawn_blocking(move || wait_until_stale(filled))
        .await
        .unwrap();

    // The refill that the next request runs parks in the store after it has read the row.
    let (parked_tx, parked_rx) = std::sync::mpsc::channel::<()>();
    let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
    flaky.after_the_get_of(
        "instance_brand",
        0,
        Box::new(move |_| {
            parked_tx.send(()).unwrap();
            // Bounded, and let go at once when the test fails (the sender is dropped with it).
            let _ = go_rx.recv_timeout(std::time::Duration::from_secs(30));
        }),
    );
    flaky.reset_reads();
    let owner = {
        let r = router.clone();
        tokio::spawn(async move { get(&r, PATH).await })
    };
    let parked = tokio::task::spawn_blocking(move || {
        parked_rx.recv_timeout(std::time::Duration::from_secs(20))
    })
    .await
    .unwrap();
    assert!(
        parked.is_ok(),
        "the request never reached the store: the cell was not stale"
    );
    assert_eq!(flaky.reads().gets, 1, "the owner has read the row");

    let bound = std::time::Duration::from_secs(20);
    let health = healthz_within(&router, bound, "while the refill is parked").await;
    assert_eq!(health.status, 200);

    let started = std::time::Instant::now();
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..100 {
        let r = router.clone();
        set.spawn(async move { get(&r, PATH).await });
    }
    let mut answers = Vec::new();
    while let Some(a) = set.join_next().await {
        answers.push(a.unwrap());
    }
    assert!(
        started.elapsed() < bound,
        "the hundred were answered only after {:?}: a request waited for the parked refill",
        started.elapsed()
    );
    assert_eq!(answers.len(), 100);
    for a in &answers {
        assert_eq!(a.status, 200, "{}", a.text());
        assert_eq!(a.body, good.body, "the last good logo");
    }
    assert_eq!(
        flaky.reads().gets,
        1,
        "the hundred read nothing: one flight, the owner's"
    );
    assert!(!owner.is_finished(), "the owner is still in the store");

    go_tx.send(()).unwrap();
    let owner = owner.await.unwrap();
    assert_eq!((owner.status, owner.body), (200, good.body.clone()));
    assert_eq!(flaky.reads().gets, 2, "the row and the theme: one flight");
    // The cell is fresh again: the next request reads nothing.
    assert_eq!(get(&router, PATH).await.body, good.body);
    assert_eq!(flaky.reads().gets, 2);
}

/// A burst of requests at a reader whose cell is empty costs one flight, two reads of the store (the
/// row and the theme setting), however many arrive and whether or not a row exists: the route has
/// no read of its own and the reader's single flight is the whole of the cost (spec, section 9).
/// The count holds only while the cell stays fresh, so a burst that took more than 1.5 seconds on a
/// slow machine is started again, and five slow tries fail the test.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_burst_of_cold_requests_costs_one_flight_of_two_reads() {
    for row in [true, false] {
        let mut counted = false;
        for _try in 0..5 {
            let (flaky, app) = flaky_app();
            let router = router_of(&app);
            if row {
                write_around_the_command(&app, "instance_brand", json!({"name": "MOHV"}));
            }
            flaky.reset_reads();
            let started = std::time::Instant::now();
            let mut set = tokio::task::JoinSet::new();
            for _ in 0..200 {
                let r = router.clone();
                set.spawn(async move { get(&r, PATH).await.status });
            }
            let mut statuses = Vec::new();
            while let Some(s) = set.join_next().await {
                statuses.push(s.unwrap());
            }
            if started.elapsed() >= std::time::Duration::from_millis(1500) {
                continue;
            }
            let want = if row { 200 } else { 404 };
            assert!(
                statuses.iter().all(|s| *s == want),
                "row={row}: {statuses:?}"
            );
            assert_eq!(statuses.len(), 200);
            assert_eq!(flaky.reads().gets, 2, "row={row}: one flight, two reads");
            counted = true;
            break;
        }
        assert!(
            counted,
            "row={row}: five bursts were each slower than 1.5 s"
        );
    }
}

/// A process that never had a good read has no logo to serve while the first read waits for the
/// store. The double holds the refill after its first read (the row of `instance_brand`) until the
/// test lets it go; it then notes `Fault::Timeout` on the thread, as the pool does when its bound
/// runs out, and refuses the refill's next read (the theme setting). No bound is shortened and none
/// is waited for: the test pins what the route and D74 do with a timeout, not the wording or the
/// timing of the pool's own. The request that runs that refill is the one that waits for the
/// store, and it is answered by D74: 503 with `Retry-After` 5 and the code word `storage_timeout`,
/// not the 404 of an unknown path and not an error page of the route's own. `/healthz` answers while
/// the read is held, and the failure is not kept: when the store answers, the route answers from it.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn a_first_read_that_times_out_in_the_store_is_answered_503_with_retry_after() {
    let (flaky, app) = flaky_app();
    let router = router_of(&app);
    let (parked_tx, parked_rx) = std::sync::mpsc::channel::<()>();
    let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
    let f = flaky.clone();
    flaky.after_the_get_of(
        "instance_brand",
        0,
        Box::new(move |_| {
            parked_tx.send(()).unwrap();
            let _ = go_rx.recv_timeout(std::time::Duration::from_secs(30));
            // The bound has run out: the timeout of the pool, noted on this thread as the pool notes
            // it (the first fault of a request is the one it is answered with), and the next read of
            // the refill, the theme setting, is refused.
            obop_storage::fault::note(obop_storage::fault::Fault::Timeout);
            f.refuse_next(1);
        }),
    );
    // What the store has been asked for (and what it refused) before the first request: the
    // refill's reads are counted from here.
    let gets_before = flaky.reads().gets;
    let refused_before = flaky.refused();
    let first = {
        let r = router.clone();
        tokio::spawn(async move { get(&r, PATH).await })
    };
    let parked = tokio::task::spawn_blocking(move || {
        parked_rx.recv_timeout(std::time::Duration::from_secs(20))
    })
    .await
    .unwrap();
    assert!(parked.is_ok(), "the request never reached the store");
    let health = healthz_within(
        &router,
        std::time::Duration::from_secs(20),
        "while the first read is held",
    )
    .await;
    assert_eq!(health.status, 200);
    assert!(!first.is_finished(), "the request waits for the store");

    go_tx.send(()).unwrap();
    let first = first.await.unwrap();
    assert_eq!(first.status, 503, "{}", first.text());
    assert_eq!(first.header("retry-after"), "5");
    assert_eq!(first.header("x-obop-applied"), "none");
    let body: J = serde_json::from_slice(&first.body).unwrap();
    assert_eq!(body["code"], "storage_timeout", "{body}");
    assert!(!first.text().contains("<svg"), "not the logo");
    // The noted timeout alone would give this answer, so the refusal is observed on its own: the
    // refill read the row (one `get`, the one that was held), its next read, the theme setting, was
    // refused by the double before it was counted, and nothing else was refused.
    assert_eq!(
        flaky.refused() - refused_before,
        1,
        "the refill's second read was refused"
    );
    assert_eq!(
        flaky.reads().gets - gets_before,
        1,
        "the refill read the row and met the refusal at the theme setting"
    );

    // Nothing was kept: the store answers, and so does the route (no row: the unknown-path 404).
    // A refill that had ended well, with no row, would be kept for the bound and the request below
    // would read nothing; a failed one is read again, the row and the theme setting, two more `get`s.
    flaky.allow_models();
    let unknown = get(&router, "/api/nothing").await;
    let healed = get(&router, PATH).await;
    assert_eq!(healed.status, 404);
    assert_eq!(healed.body, unknown.body);
    assert_eq!(
        flaky.reads().gets - gets_before,
        3,
        "the failed refill was not kept: the next request read the row and the theme setting again"
    );
}

/// The store is never read on the thread that polls the request: the PostgreSQL client panics on an
/// async worker, so the reader is asked through the blocking pool (`brand::current`). The test
/// polls the request on its own thread (a current-thread runtime) and notes, from inside the store,
/// which thread read it.
#[tokio::test]
async fn the_store_is_read_on_a_blocking_thread_and_never_on_the_one_that_polls_the_request() {
    let (flaky, app) = flaky_app();
    let router = router_of(&app);
    let polling = std::thread::current().id();
    let seen = Arc::new(std::sync::Mutex::new(None));
    let noted = seen.clone();
    flaky.after_the_get_of(
        "instance_brand",
        0,
        Box::new(move |_| {
            *noted.lock().unwrap() = Some(std::thread::current().id());
        }),
    );
    assert_eq!(get(&router, PATH).await.status, 404);
    let reader = seen.lock().unwrap().expect("the reader read the row");
    assert_ne!(reader, polling, "the refill ran on the polling thread");
}

// ---- The runtime of the server ----------------------------------------------------------------

const SCRATCH_PREFIX: &str = "obop-brand-rt-";

/// Removes the scratch directories that earlier runs of the test below left behind: those of
/// another process that are more than ten minutes old, so a run in parallel keeps its own. Where the
/// directory is still in use, the removal fails and the directory stays for the next run. (The same
/// as `sweep_scratch` of `brand_commands_tests`, for this prefix.)
fn sweep_scratch() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some((pid, nanos)) = name
            .strip_prefix(SCRATCH_PREFIX)
            .and_then(|rest| rest.split_once('-'))
        else {
            continue;
        };
        let (Ok(pid), Ok(nanos)) = (pid.parse::<u32>(), nanos.parse::<u128>()) else {
            continue;
        };
        if pid != std::process::id() && now.saturating_sub(nanos) > 600_000_000_000 {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// The route on a multi-threaded runtime over every store the tests have (SQLite always, PostgreSQL
/// when `OBOP_TEST_POSTGRES_URL` is set): the reader is asked through the blocking pool, because the
/// PostgreSQL client panics on an async worker. With no PostgreSQL this proves the route works on
/// the runtime of `serve` and that nothing on the async path reads the store; the PostgreSQL case
/// runs where the variable is set.
#[test]
fn the_route_works_on_the_runtime_of_the_server_over_every_store() {
    // Removes the directory when the test ends, by a failed assertion too. A name that no other run
    // has: a process id is reused (often, on Windows), and a directory that an earlier run left
    // behind holds its database, with the brand row the first assertion below says is not there.
    // On Windows the removal can fail with a sharing violation while the process lives (the
    // application keeps the database file open), so a run cannot always clean up after itself;
    // `sweep_scratch` of the next run does.
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    sweep_scratch();
    let dir = Scratch(std::env::temp_dir().join(format!(
        "{SCRATCH_PREFIX}{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )));
    std::fs::create_dir_all(&dir.0).unwrap();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let mut stores = vec![(
        Store::open(&format!("sqlite://{}", dir.0.join("brand.db").display())).unwrap(),
        None,
    )];
    if let Some(db) = obop_storage::testing::TestDb::new() {
        stores.push((Store::open(db.url()).unwrap(), Some(db)));
    }
    for (store, _db) in stores {
        let app = App::with_store(store);
        let router = obop_http::router(state(&app), None);
        rt.block_on(async {
            assert_eq!(get(&router, PATH).await.status, 404, "no brand");
        });
        // `set_brand` reads the store from this thread, which is not a worker of the runtime.
        set_brand(&app);
        rt.block_on(async {
            let a = get(&router, PATH).await;
            assert_eq!(a.status, 200);
            assert_eq!(a.text(), artwork());
            // Many requests at once on the workers.
            let mut set = tokio::task::JoinSet::new();
            for _ in 0..16 {
                let r = router.clone();
                set.spawn(async move { get(&r, PATH).await.status });
            }
            while let Some(s) = set.join_next().await {
                assert_eq!(s.unwrap(), 200);
            }
        });
    }
}
