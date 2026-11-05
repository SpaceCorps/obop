//! The logo route's fixed answer: the path, and the headers that never change (docs/spec/branding.md,
//! section 4, "The logo route"). The route lives in `obop-http` and the shop's favicon in
//! `obop-site`; neither can depend on the other, so the constants live here.

use crate::refusal::Refusal;
use crate::svg::{self, Svg};
use crate::tile;
use sha2::{Digest, Sha256};

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

// ---- The logo that is served --------------------------------------------------------------------

/// The version of a logo, `v`: the first 16 hex digits of the SHA-256 of the exact bytes the logo
/// route serves. The `ETag` and the `?v=` of the icon address follow what is served, also when a
/// theme change alone alters the tile.
pub fn version(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .take(8)
        .map(|b| format!("{b:02x}"))
        .collect()
}

enum Source {
    Artwork(Svg),
    Tile {
        name: String,
        theme_accent: Option<String>,
    },
}

/// The logo of a brand, as the read check leaves it: the stored artwork re-serialised by the
/// current printer when the stored text parses under the strict reader, else the generated tile
/// (and the reasons, for the `fallbacks` of `get`). The stored text is never served.
pub struct Logo {
    served: String,
    v: String,
    source: Source,
    fallback: Option<Vec<Refusal>>,
}

impl Logo {
    /// `stored` is the `logo_svg` of the row (empty: no artwork), `name` the effective name and
    /// `theme_accent` the accent the theme stores, if any. Total: whatever the stored text is, the
    /// answer is a logo.
    pub fn resolve(stored: &str, name: &str, theme_accent: Option<&str>) -> Logo {
        let tile = |fallback: Option<Vec<Refusal>>| {
            let served = tile::svg(name, theme_accent, None);
            Logo {
                v: version(served.as_bytes()),
                served,
                source: Source::Tile {
                    name: name.to_string(),
                    theme_accent: theme_accent.map(str::to_string),
                },
                fallback,
            }
        };
        if stored.is_empty() {
            return tile(None);
        }
        match svg::parse_str(stored) {
            Ok(parsed) => {
                let served = parsed.print();
                Logo {
                    v: version(served.as_bytes()),
                    served,
                    source: Source::Artwork(parsed),
                    fallback: None,
                }
            }
            Err(refusals) => tile(Some(refusals)),
        }
    }

    /// The bytes the logo route serves.
    pub fn served(&self) -> &str {
        &self.served
    }

    /// The version of the logo, `v`.
    pub fn v(&self) -> &str {
        &self.v
    }

    /// Whether the logo is the generated tile.
    pub fn is_tile(&self) -> bool {
        matches!(self.source, Source::Tile { .. })
    }

    /// Why a stored artwork was not used: the reasons of the strict reader. `None` when there was
    /// no artwork or it was used.
    pub fn fallback(&self) -> Option<&[Refusal]> {
        self.fallback.as_deref()
    }

    /// The same logo with `width` and `height` of `size` on the root, for inline use on a page.
    pub fn inline(&self, size: u32) -> String {
        match &self.source {
            Source::Artwork(parsed) => parsed.print_inline(size),
            Source::Tile { name, theme_accent } => {
                tile::svg(name, theme_accent.as_deref(), Some(size))
            }
        }
    }
}
