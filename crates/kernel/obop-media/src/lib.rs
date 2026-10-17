//! Uploaded images and files, made safe before they are stored (D60).
//!
//! **Images.** The type comes from the first bytes (magic numbers), never from the name or the
//! `Content-Type` the client sent; a declared type that disagrees with the bytes is refused
//! (a polyglot or a mislabelled file is not an image). The dimensions are read from the header
//! and checked against limits *before* any pixel is allocated, and the decoder runs with
//! allocation limits, so a small file that claims to be 60,000 pixels wide (a decompression
//! bomb) is refused cheaply. The accepted image is then **decoded and encoded again**: EXIF
//! (including GPS), XMP, IPTC, ICC profiles, comments, thumbnails embedded in the file, appended
//! archives or scripts and every other byte the decoder does not turn into pixels are gone. The
//! EXIF orientation is applied to the pixels first, so a phone photo is not turned sideways by
//! stripping it. Output: JPEG stays JPEG, PNG stays PNG, GIF becomes a PNG of its first frame,
//! WebP becomes JPEG (or PNG when it has transparency). Animation is not kept.
//!
//! **SVG is rejected.** SVG is a document that can carry script; sanitising it safely is a
//! project of its own, so obop does not accept it. Use PNG, JPEG, WebP or GIF.
//!
//! **Files** (documents, not images in an image field) are checked against a short list of types
//! by magic number and kept byte for byte; they are always served as downloads.
//!
//! Dependency: the `image` crate (MIT OR Apache-2.0) with only the `png`, `jpeg`, `gif` and `webp`
//! features, no `rayon`.

use image::{DynamicImage, ImageDecoder, ImageFormat, imageops::FilterType};
use std::io::Cursor;

pub mod fixtures;

/// Limits applied to every image before and while it is decoded.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Largest accepted upload in bytes.
    pub max_bytes: usize,
    pub max_width: u32,
    pub max_height: u32,
    /// Largest `width * height`.
    pub max_pixels: u64,
    /// Decoder allocation budget in bytes.
    pub max_alloc: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_IMAGE_BYTES,
            max_width: 8000,
            max_height: 8000,
            max_pixels: 36_000_000,
            max_alloc: 320 * 1024 * 1024,
        }
    }
}

/// Default size cap of an `image` field (5 MiB) and of a `file` field (25 MiB).
pub const DEFAULT_IMAGE_BYTES: usize = 5 * 1024 * 1024;
pub const DEFAULT_FILE_BYTES: usize = 25 * 1024 * 1024;
/// No field may be configured above this.
pub const HARD_MAX_BYTES: usize = 64 * 1024 * 1024;

/// Why an upload was refused. Messages are safe to show to the uploader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaError {
    Empty,
    TooLarge {
        max: usize,
    },
    /// SVG, HTML, XML or anything else that is a document, not a picture.
    Svg,
    /// The bytes are not one of the accepted types.
    Unsupported,
    /// The declared type is not what the bytes are.
    Mislabelled {
        declared: String,
        actual: &'static str,
    },
    /// The image is larger than the limits allow.
    Dimensions,
    /// The bytes claim to be an image and are not a valid one.
    Corrupt,
}

impl std::fmt::Display for MediaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("the file is empty"),
            Self::TooLarge { max } => write!(f, "the file is larger than {} KiB", max / 1024),
            Self::Svg => f.write_str(
                "SVG and other markup files are not accepted; use PNG, JPEG, WebP or GIF",
            ),
            Self::Unsupported => f.write_str("this file type is not accepted"),
            Self::Mislabelled { declared, actual } => write!(
                f,
                "the file says it is {declared} but its content is {actual}"
            ),
            Self::Dimensions => f.write_str("the image is too large in pixels"),
            Self::Corrupt => f.write_str("the image is damaged or not a valid image"),
        }
    }
}

impl std::error::Error for MediaError {}

/// Image formats accepted in an image field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Png,
    Jpeg,
    Gif,
    Webp,
}

impl Kind {
    pub fn mime(self) -> &'static str {
        match self {
            Kind::Png => "image/png",
            Kind::Jpeg => "image/jpeg",
            Kind::Gif => "image/gif",
            Kind::Webp => "image/webp",
        }
    }

    fn format(self) -> ImageFormat {
        match self {
            Kind::Png => ImageFormat::Png,
            Kind::Jpeg => ImageFormat::Jpeg,
            Kind::Gif => ImageFormat::Gif,
            Kind::Webp => ImageFormat::WebP,
        }
    }
}

/// The image format of `bytes` by magic number.
pub fn sniff(bytes: &[u8]) -> Option<Kind> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some(Kind::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(Kind::Jpeg)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(Kind::Gif)
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(Kind::Webp)
    } else {
        None
    }
}

/// Whether the start of `bytes` looks like markup (SVG, XML, HTML): those are never accepted as
/// pictures, whatever they are labelled.
fn looks_like_markup(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(2048)];
    let head = head.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(head);
    let text = String::from_utf8_lossy(head).to_ascii_lowercase();
    let t = text.trim_start();
    [
        "<svg",
        "<?xml",
        "<!doctype",
        "<html",
        "<script",
        "<!--",
        "<head",
        "<body",
    ]
    .iter()
    .any(|p| t.starts_with(p))
        || text.contains("<svg")
}

fn declared_ok(declared: &str, actual: &'static str) -> Result<(), MediaError> {
    let d = declared
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if d.is_empty() || d == "application/octet-stream" || d == actual {
        return Ok(());
    }
    // image/jpg is a common wrong spelling of the right thing.
    if d == "image/jpg" && actual == "image/jpeg" {
        return Ok(());
    }
    if d == "image/svg+xml" {
        return Err(MediaError::Svg);
    }
    Err(MediaError::Mislabelled {
        declared: d,
        actual,
    })
}

/// An accepted, cleaned image.
#[derive(Debug, Clone)]
pub struct Processed {
    pub bytes: Vec<u8>,
    pub content_type: &'static str,
    pub width: u32,
    pub height: u32,
}

fn decode(bytes: &[u8], kind: Kind, lim: &Limits) -> Result<DynamicImage, MediaError> {
    let reader = image::ImageReader::with_format(Cursor::new(bytes), kind.format());
    let too_big = |w: u32, h: u32| {
        w == 0
            || h == 0
            || w > lim.max_width
            || h > lim.max_height
            || u64::from(w) * u64::from(h) > lim.max_pixels
    };
    let mut decoder = reader.into_decoder().map_err(|e| {
        // A decoder may refuse an enormous header with a generic error; say what it was.
        match (e, header_dimensions(bytes, kind)) {
            (image::ImageError::Limits(_), _) => MediaError::Dimensions,
            (_, Some((w, h))) if too_big(w, h) => MediaError::Dimensions,
            _ => MediaError::Corrupt,
        }
    })?;
    let (w, h) = decoder.dimensions();
    if too_big(w, h) {
        return Err(MediaError::Dimensions);
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(lim.max_width);
    limits.max_image_height = Some(lim.max_height);
    limits.max_alloc = Some(lim.max_alloc);
    decoder
        .set_limits(limits)
        .map_err(|_| MediaError::Dimensions)?;
    let orientation = decoder.orientation().ok();
    let mut img = DynamicImage::from_decoder(decoder).map_err(|e| match e {
        image::ImageError::Limits(_) => MediaError::Dimensions,
        _ => MediaError::Corrupt,
    })?;
    if let Some(o) = orientation {
        img.apply_orientation(o);
    }
    Ok(img)
}

/// Width and height as the first bytes of a PNG or GIF state them (the formats where it is a
/// fixed offset), without trusting a decoder to survive an absurd value.
fn header_dimensions(bytes: &[u8], kind: Kind) -> Option<(u32, u32)> {
    match kind {
        Kind::Png if bytes.len() >= 24 => Some((
            u32::from_be_bytes(bytes[16..20].try_into().ok()?),
            u32::from_be_bytes(bytes[20..24].try_into().ok()?),
        )),
        Kind::Gif if bytes.len() >= 10 => Some((
            u32::from(u16::from_le_bytes(bytes[6..8].try_into().ok()?)),
            u32::from(u16::from_le_bytes(bytes[8..10].try_into().ok()?)),
        )),
        _ => None,
    }
}

fn has_transparency(img: &DynamicImage) -> bool {
    img.color().has_alpha() && img.to_rgba8().pixels().any(|p| p.0[3] < 255)
}

fn encode(img: &DynamicImage, as_jpeg: bool) -> Result<(Vec<u8>, &'static str), MediaError> {
    let mut out = Vec::new();
    if as_jpeg {
        let rgb = DynamicImage::ImageRgb8(img.to_rgb8());
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 88);
        rgb.write_with_encoder(enc)
            .map_err(|_| MediaError::Corrupt)?;
        Ok((out, "image/jpeg"))
    } else {
        img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .map_err(|_| MediaError::Corrupt)?;
        Ok((out, "image/png"))
    }
}

/// Validate and clean an uploaded image. `declared` is the `Content-Type` the client sent (may
/// be empty). See the module comment for what is kept and what is not.
pub fn process_image(bytes: &[u8], declared: &str, lim: &Limits) -> Result<Processed, MediaError> {
    if bytes.is_empty() {
        return Err(MediaError::Empty);
    }
    if bytes.len() > lim.max_bytes {
        return Err(MediaError::TooLarge { max: lim.max_bytes });
    }
    let Some(kind) = sniff(bytes) else {
        return Err(if looks_like_markup(bytes) || declared.contains("svg") {
            MediaError::Svg
        } else {
            MediaError::Unsupported
        });
    };
    declared_ok(declared, kind.mime())?;
    let img = decode(bytes, kind, lim)?;
    let (width, height) = (img.width(), img.height());
    let as_jpeg = match kind {
        Kind::Jpeg => true,
        Kind::Png | Kind::Gif => false,
        Kind::Webp => !has_transparency(&img),
    };
    let (bytes, content_type) = encode(&img, as_jpeg)?;
    Ok(Processed {
        bytes,
        content_type,
        width,
        height,
    })
}

/// A copy of an already stored (cleaned) image scaled to `width` pixels wide, never larger than
/// the original. Same format as the stored image.
pub fn thumbnail(bytes: &[u8], width: u32, lim: &Limits) -> Result<Processed, MediaError> {
    let kind = sniff(bytes).ok_or(MediaError::Unsupported)?;
    let img = decode(bytes, kind, lim)?;
    let width = width.clamp(16, img.width().max(16));
    let scaled = if width >= img.width() {
        img
    } else {
        let height =
            ((u64::from(img.height()) * u64::from(width)) / u64::from(img.width())).max(1) as u32;
        img.resize_exact(width, height, FilterType::Lanczos3)
    };
    let (w, h) = (scaled.width(), scaled.height());
    let (bytes, content_type) = encode(&scaled, kind == Kind::Jpeg)?;
    Ok(Processed {
        bytes,
        content_type,
        width: w,
        height: h,
    })
}

/// Read the pixel size of an image from its header without decoding it.
pub fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let kind = sniff(bytes)?;
    let d = image::ImageReader::with_format(Cursor::new(bytes), kind.format())
        .into_decoder()
        .ok()?;
    Some(d.dimensions())
}

/// Types a `file` field accepts, with how each is recognised.
pub const FILE_TYPES: [&str; 14] = [
    "application/pdf",
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "text/plain",
    "text/csv",
    "application/zip",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    "application/vnd.oasis.opendocument.text",
    "application/vnd.oasis.opendocument.spreadsheet",
    "application/vnd.oasis.opendocument.presentation",
];

/// Check an uploaded document and return the content type to store. Only [`FILE_TYPES`]; the
/// bytes must be what the type says (PDF, PNG, JPEG, GIF and WebP by magic number, ZIP-based
/// formats by the ZIP signature, text by being valid UTF-8 without NUL bytes). Never HTML, SVG,
/// scripts or executables. The bytes are kept as uploaded (they are served as downloads only).
pub fn check_file(bytes: &[u8], declared: &str, max: usize) -> Result<&'static str, MediaError> {
    if bytes.is_empty() {
        return Err(MediaError::Empty);
    }
    if bytes.len() > max {
        return Err(MediaError::TooLarge { max });
    }
    let d = declared
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if d.contains("svg")
        || d.contains("html")
        || d.contains("javascript")
        || matches!(d.as_str(), "text/xml" | "application/xml")
        || d.ends_with("+xml")
    {
        return Err(MediaError::Svg);
    }
    if let Some(k) = sniff(bytes) {
        declared_ok(&d, k.mime())?;
        return Ok(k.mime());
    }
    if bytes.starts_with(b"%PDF-") {
        declared_ok(&d, "application/pdf")?;
        return Ok("application/pdf");
    }
    if bytes.starts_with(b"PK\x03\x04") {
        return match FILE_TYPES.iter().copied().find(|t| {
            *t == d
                && (t.contains("zip") || t.contains("officedocument") || t.contains("opendocument"))
        }) {
            Some(t) => Ok(t),
            None => Err(MediaError::Mislabelled {
                declared: d,
                actual: "a ZIP archive",
            }),
        };
    }
    if looks_like_markup(bytes) {
        return Err(MediaError::Svg);
    }
    let plain = |b: &u8| matches!(b, b'\t' | b'\n' | b'\r' | 0x20..=0x7E | 0x80..=0xFF);
    if bytes.iter().all(plain) && std::str::from_utf8(bytes).is_ok() {
        return match d.as_str() {
            "" | "application/octet-stream" | "text/plain" => Ok("text/plain"),
            "text/csv" => Ok("text/csv"),
            _ => Err(MediaError::Mislabelled {
                declared: d,
                actual: "plain text",
            }),
        };
    }
    Err(MediaError::Unsupported)
}

#[cfg(test)]
mod tests;
