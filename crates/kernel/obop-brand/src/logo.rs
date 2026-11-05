//! The logo route's fixed answer: the path, and the headers that never change (docs/spec/branding.md,
//! section 4, "The logo route"). The route lives in `obop-http` and the shop's favicon in
//! `obop-site`; neither can depend on the other, so the constants live here.

/// `GET /api/brand/logo.svg`: public, takes no path or id, serves the artwork or the tile and
/// nothing else. A query string is ignored except that it is part of the browser's cache key.
pub const LOGO_PATH: &str = "/api/brand/logo.svg";

/// `Content-Type` of the logo.
pub const LOGO_CONTENT_TYPE: &str = "image/svg+xml";

/// Value of `X-Content-Type-Options`.
pub const LOGO_NOSNIFF: &str = "nosniff";

/// Value of `Content-Security-Policy` on the logo: nothing may load, no style may apply, and the
/// document is sandboxed, so an SVG opened on its own address runs nothing.
pub const LOGO_CSP: &str = "default-src 'none'; style-src 'none'; sandbox";

/// Value of `Cache-Control`: the browser revalidates every time; the address carries `?v=`, so a
/// new logo is a new address.
pub const LOGO_CACHE_CONTROL: &str = "no-cache";

/// The headers every answer with the logo carries, besides the `ETag`, as `(name, value)`.
pub const LOGO_HEADERS: [(&str, &str); 4] = [
    ("Content-Type", LOGO_CONTENT_TYPE),
    ("X-Content-Type-Options", LOGO_NOSNIFF),
    ("Content-Security-Policy", LOGO_CSP),
    ("Cache-Control", LOGO_CACHE_CONTROL),
];

/// The address of the logo with its version, as the tab icon link and the shop's logo address
/// carry it: `/api/brand/logo.svg?v=<v>`.
pub fn logo_url(v: &str) -> String {
    format!("{LOGO_PATH}?v={v}")
}

/// The `ETag` header value for a version `v`: the quoted strong validator, `"<v>"`.
pub fn logo_etag(v: &str) -> String {
    format!("\"{v}\"")
}
