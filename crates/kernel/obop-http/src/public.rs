//! Mounts the public website and web shop of `obop-site` on an axum router and lays out the
//! instance: public pages at `/`, the backoffice build under an admin prefix (`/admin`), the
//! REST API at `/api`. This is the only place the framework-neutral `Req`/`Resp` of that crate
//! meet axum (D29).
//!
//! The public routes are a fallback: every route registered before them (the REST API, the
//! backoffice) wins, and anything else is answered by the site, including its real 404 page.

use super::{HttpState, api_routes, blocking};
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{ConnectInfo, Request, State},
    http::{HeaderName, HeaderValue, StatusCode, header},
    response::Response,
};
use obop_site::{MAX_BODY, Site, http::Req};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::services::{ServeDir, ServeFile};

/// Request headers handed to the site (everything else is dropped).
const FORWARDED: [&str; 9] = [
    "accept",
    "accept-language",
    "cookie",
    "host",
    "if-none-match",
    "if-modified-since",
    "origin",
    "x-forwarded-proto",
    "content-type",
];

#[derive(Clone)]
pub struct PublicConfig {
    /// Where the backoffice build is served, for example `/admin`.
    pub admin_prefix: String,
    /// Take the client address from the last entry of `X-Forwarded-For` (set only behind a
    /// proxy you control; otherwise anybody could pick their own rate-limit identity).
    pub trust_proxy: bool,
}

impl Default for PublicConfig {
    fn default() -> Self {
        PublicConfig {
            admin_prefix: "/admin".into(),
            trust_proxy: false,
        }
    }
}

/// A usable admin prefix: one path segment, lowercase letters, digits, `-` and `_`.
pub fn admin_prefix_ok(p: &str) -> bool {
    let Some(rest) = p.strip_prefix('/') else {
        return false;
    };
    !rest.is_empty()
        && rest.len() <= 32
        && !matches!(
            rest,
            "api" | "assets" | "shop" | "blog" | "cart" | "contact"
        )
        && rest
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// The instance with the public site enabled.
pub fn router(
    state: Arc<HttpState>,
    static_dir: Option<&str>,
    site: Arc<Site>,
    cfg: PublicConfig,
) -> Router {
    let mut app = api_routes(state);
    if let Some(dir) = static_dir {
        let admin = ServeDir::new(dir).fallback(ServeFile::new(format!("{dir}/index.html")));
        app = app.nest_service(&cfg.admin_prefix, admin);
    }
    let h = Arc::new((site, cfg));
    app.merge(Router::new().fallback(handle).with_state(h))
}

type Shared = Arc<(Arc<Site>, PublicConfig)>;

async fn handle(State(shared): State<Shared>, req: Request) -> Response {
    let (site, cfg) = (shared.0.clone(), shared.1.clone());
    let (parts, body) = req.into_parts();
    // The REST API answers JSON, not the HTML 404 page.
    if parts.uri.path() == "/api" || parts.uri.path().starts_with("/api/") {
        return json_404();
    }
    let bytes = match to_bytes(body, MAX_BODY + 1).await {
        Ok(b) => b,
        Err(_) => return too_large(),
    };
    let target = parts.uri.path_and_query().map_or("/", |p| p.as_str());
    let mut r = Req::new(parts.method.as_str(), target);
    for name in FORWARDED {
        let vals: Vec<&str> = parts
            .headers
            .get_all(name)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .collect();
        if !vals.is_empty() {
            let sep = if name == "cookie" { "; " } else { ", " };
            r.headers.insert(name.to_string(), vals.join(sep));
        }
    }
    r.body = bytes.to_vec();
    r.client = client_address(&parts, cfg.trust_proxy);
    let resp = blocking(move || site.handle(&r)).await;
    let mut out = Response::builder()
        .status(StatusCode::from_u16(resp.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR));
    for (k, v) in &resp.headers {
        if let (Ok(k), Ok(v)) = (
            HeaderName::from_bytes(k.as_bytes()),
            HeaderValue::from_str(v),
        ) {
            out = out.header(k, v);
        }
    }
    out.body(Body::from(resp.body)).unwrap_or_else(|_| {
        Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::empty())
            .expect("static response")
    })
}

/// The address used to key rate limits: the socket peer, or behind a trusted proxy the last
/// (nearest) entry of `X-Forwarded-For`.
fn client_address(parts: &axum::http::request::Parts, trust_proxy: bool) -> String {
    if trust_proxy
        && let Some(v) = parts
            .headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
        && let Some(last) = v.rsplit(',').next()
    {
        return last.trim().chars().take(64).collect();
    }
    parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip().to_string())
        .unwrap_or_default()
}

fn json_404() -> Response {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{\"error\":\"not found\"}"))
        .expect("static response")
}

fn too_large() -> Response {
    Response::builder()
        .status(StatusCode::PAYLOAD_TOO_LARGE)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from("request body too large\n"))
        .expect("static response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_prefixes_are_one_safe_segment() {
        assert!(admin_prefix_ok("/admin"));
        assert!(admin_prefix_ok("/back-office_2"));
        for bad in [
            "admin", "/", "", "/a/b", "/Admin", "/api", "/shop", "/a b", "/..", "//x",
        ] {
            assert!(!admin_prefix_ok(bad), "{bad}");
        }
    }
}
