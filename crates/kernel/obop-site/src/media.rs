//! Public pictures (D60): `/media/{id}.{png|jpg}` and the markup that uses them.
//!
//! A picture is public only when `PublicApi::public_blob` says so: held by a published record
//! through a field its package declared public (and, for a gallery image, with its listing
//! published). Everything else, including a blob that exists but is private, answers a plain
//! `404`. URLs carry the content hash, so responses are immutable and cacheable for a year;
//! unpublishing a record stops the server from answering, but a cache or CDN that already holds
//! the bytes keeps them until it is purged (see docs/STOREFRONT.md).
//!
//! The markup is the responsive kind: `srcset` over the fixed thumbnail widths, `sizes` chosen by
//! the caller, `width` and `height` so the page does not jump, lazy loading except for the
//! picture that is the page's main content.

use crate::http::Resp;
use crate::layout::{Ctx, security_headers};
use crate::util::esc;
use crate::{Site, valid_hex_id};
use obop_commands::{PublicFilter, PublicRecord, Value};
use obop_storage::blob::THUMB_WIDTHS;
use std::time::Duration;

/// A public picture and what the markup needs to know about it.
#[derive(Debug, Clone)]
pub(crate) struct Pic {
    pub id: String,
    pub ext: &'static str,
    pub width: u32,
    pub height: u32,
}

fn ext_of(content_type: &str) -> Option<&'static str> {
    match content_type {
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        _ => None,
    }
}

impl Pic {
    pub fn path(&self, width: Option<u32>) -> String {
        match width {
            Some(w) => format!("/media/{}.{}?w={w}", self.id, self.ext),
            None => format!("/media/{}.{}", self.id, self.ext),
        }
    }

    fn scaled_height(&self, w: u32) -> u32 {
        ((u64::from(self.height) * u64::from(w)) / u64::from(self.width.max(1))).max(1) as u32
    }

    /// The URL and size to put in a social card or structured data: about 1200 pixels wide.
    pub fn social(&self) -> (String, u32, u32) {
        if self.width > 1280 {
            (self.path(Some(1280)), 1280, self.scaled_height(1280))
        } else {
            (self.path(None), self.width, self.height)
        }
    }

    /// An `<img>` with `srcset`, `sizes`, intrinsic size and an alt text. `eager` is for the one
    /// picture that is the main content of the page.
    pub fn html(&self, alt: &str, sizes: &str, eager: bool) -> String {
        let mut set: Vec<String> = THUMB_WIDTHS
            .iter()
            .filter(|w| **w < self.width)
            .map(|w| format!("{} {w}w", self.path(Some(*w))))
            .collect();
        if self.width <= *THUMB_WIDTHS.last().unwrap_or(&1280) {
            set.push(format!("{} {}w", self.path(None), self.width));
        }
        let (src, w, h) = if self.width > 640 {
            (self.path(Some(640)), 640, self.scaled_height(640))
        } else {
            (self.path(None), self.width, self.height)
        };
        format!(
            "<img src=\"{}\" srcset=\"{}\" sizes=\"{}\" width=\"{w}\" height=\"{h}\" alt=\"{}\" {} decoding=\"async\">",
            esc(&src),
            esc(&set.join(", ")),
            esc(sizes),
            esc(alt),
            if eager {
                "fetchpriority=\"high\""
            } else {
                "loading=\"lazy\""
            },
        )
    }
}

impl Site {
    /// The public picture behind a blob id, if the blob is public and is a PNG or JPEG.
    pub(crate) fn pic(&self, id: &str) -> Option<Pic> {
        if !valid_hex_id(id) {
            return None;
        }
        let (w, h, ct) = self.api.public_blob_info(id)?;
        Some(Pic {
            id: id.to_string(),
            ext: ext_of(&ct)?,
            width: w?.max(1),
            height: h?.max(1),
        })
    }

    /// Absolute URLs of the public pictures of a listing: its main picture, then its gallery.
    pub(crate) fn listing_image_urls(&self, ctx: &Ctx, listing: &PublicRecord) -> Vec<String> {
        let mut ids = vec![listing.text("image").to_string()];
        if let Ok((rows, _)) = self.api.list(
            "ecommerce_listing_image",
            &[(
                "listing_id".to_string(),
                PublicFilter::Eq(Value::text(&listing.id)),
            )],
            Some(20),
            0,
        ) {
            ids.extend(rows.iter().map(|r| r.text("image").to_string()));
        }
        let mut out: Vec<String> = Vec::new();
        for id in ids {
            if let Some(pic) = self.pic(&id) {
                let url = ctx.abs(&pic.social().0);
                if !out.contains(&url) {
                    out.push(url);
                }
            }
        }
        out
    }

    /// `GET /media/{id}.{ext}[?w=640]`.
    pub(crate) fn media(&self, ctx: &Ctx, rest: &str) -> Resp {
        let plain = |status: u16, msg: &str| {
            security_headers(
                Resp::text(status, msg).header("cache-control", "no-store"),
                ctx,
            )
        };
        let src = self.source(ctx.req);
        if !self.allow(&format!("media:{src}"), 900, Duration::from_secs(60)) {
            return plain(429, "slow down\n").header("retry-after", "60");
        }
        let (id, ext) = rest.split_once('.').unwrap_or((rest, ""));
        if !valid_hex_id(id) || !matches!(ext, "" | "png" | "jpg") {
            return plain(404, "not found\n");
        }
        let width = match ctx.req.q("w") {
            None => None,
            Some(w) => match w.parse::<u32>() {
                Ok(w) if THUMB_WIDTHS.contains(&w) => Some(w),
                _ => return plain(404, "not found\n"),
            },
        };
        let Some(served) = self.api.public_blob(id, width) else {
            return plain(404, "not found\n");
        };
        if !ext.is_empty() && ext_of(&served.content_type) != Some(ext) {
            return plain(404, "not found\n");
        }
        let inm = ctx.req.header("if-none-match").unwrap_or("");
        let not_modified = inm
            .split(',')
            .any(|t| t.trim().trim_start_matches("W/") == served.etag || t.trim() == "*");
        let mut r = if not_modified {
            Resp::new(304, &served.content_type, Vec::new())
        } else {
            Resp::new(200, &served.content_type, served.bytes)
        };
        r = r
            .header("etag", &served.etag)
            .header("cache-control", "public, max-age=31536000, immutable")
            .header("cross-origin-resource-policy", "cross-origin");
        security_headers(r, ctx)
    }
}
