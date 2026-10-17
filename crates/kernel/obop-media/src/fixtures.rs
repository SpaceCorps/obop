//! Tiny valid images made in code, for tests and for `obop site seed` demo content. No binary
//! files are committed. Every function is deterministic: the same arguments give the same
//! bytes, and a different `seed` gives different colours (so different content hashes).

use image::{DynamicImage, ImageFormat, RgbImage, RgbaImage};
use std::io::Cursor;

fn rgb(w: u32, h: u32, seed: u8) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| {
        let a = (x * 255 / w.max(1)) as u8;
        let b = (y * 255 / h.max(1)) as u8;
        image::Rgb([
            a.wrapping_add(seed.wrapping_mul(37)),
            b,
            seed.wrapping_mul(71).wrapping_add(a / 2),
        ])
    })
}

fn encode(img: DynamicImage, fmt: ImageFormat) -> Vec<u8> {
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), fmt)
        .expect("fixture encodes");
    out
}

pub fn png(w: u32, h: u32, seed: u8) -> Vec<u8> {
    encode(DynamicImage::ImageRgb8(rgb(w, h, seed)), ImageFormat::Png)
}

/// A PNG with a transparent corner.
pub fn png_alpha(w: u32, h: u32, seed: u8) -> Vec<u8> {
    let img = RgbaImage::from_fn(w, h, |x, y| {
        let c = rgb(1, 1, seed).get_pixel(0, 0).0;
        image::Rgba([
            c[0],
            c[1],
            c[2],
            if x < w / 4 && y < h / 4 { 0 } else { 255 },
        ])
    });
    encode(DynamicImage::ImageRgba8(img), ImageFormat::Png)
}

pub fn jpeg(w: u32, h: u32, seed: u8) -> Vec<u8> {
    encode(DynamicImage::ImageRgb8(rgb(w, h, seed)), ImageFormat::Jpeg)
}

/// A simple illustrated scene (a gradient sky, a sun and a rounded object in a contrasting
/// colour) as a PNG, for demo content. Different seeds give different palettes.
pub fn art(w: u32, h: u32, seed: u8) -> Vec<u8> {
    // Six hand-picked palettes: (sky top, sky bottom, sun, object)
    type Palette = ([u8; 3], [u8; 3], [u8; 3], [u8; 3]);
    const PALETTES: [Palette; 6] = [
        (
            [186, 226, 205],
            [236, 244, 224],
            [250, 214, 120],
            [45, 106, 79],
        ),
        (
            [246, 214, 190],
            [252, 240, 226],
            [236, 146, 104],
            [122, 62, 44],
        ),
        (
            [190, 208, 238],
            [232, 238, 250],
            [250, 232, 160],
            [52, 78, 130],
        ),
        (
            [226, 200, 226],
            [246, 232, 246],
            [252, 220, 170],
            [116, 66, 120],
        ),
        (
            [208, 226, 190],
            [240, 246, 222],
            [248, 200, 120],
            [88, 112, 50],
        ),
        (
            [232, 214, 190],
            [248, 240, 226],
            [212, 130, 100],
            [96, 84, 64],
        ),
    ];
    let (top, bottom, sun, obj) = PALETTES[seed as usize % PALETTES.len()];
    let (wf, hf) = (w as f32, h as f32);
    // Seeds beyond the palettes move the sun and the object, so every seed is a different
    // picture (and so a different content hash).
    let k = f32::from(seed / PALETTES.len() as u8 % 10);
    let (cx, cy, r) = (
        wf * (0.72 - k * 0.02),
        hf * (0.30 + k * 0.01),
        hf.min(wf) * 0.17,
    );
    // The object: a rounded rectangle standing on the "table" in the lower third.
    let (bx0, bx1, by0, by1) = (
        wf * (0.20 + k * 0.025),
        wf * (0.54 + k * 0.025),
        hf * (0.40 - k * 0.01),
        hf * 0.80,
    );
    let corner = hf.min(wf) * 0.10;
    let img = RgbImage::from_fn(w, h, |x, y| {
        let (xf, yf) = (x as f32 + 0.5, y as f32 + 0.5);
        let mix = |a: [u8; 3], b: [u8; 3], t: f32| {
            let m = |i: usize| (f32::from(a[i]) * (1.0 - t) + f32::from(b[i]) * t) as u8;
            image::Rgb([m(0), m(1), m(2)])
        };
        if yf > hf * 0.80 {
            // The table.
            return mix(
                obj,
                [obj[0] / 2, obj[1] / 2, obj[2] / 2],
                (yf - hf * 0.8) / (hf * 0.2),
            );
        }
        let inside = xf > bx0 && xf < bx1 && yf > by0 && yf < by1 && {
            let dx = (bx0 + corner - xf).max(xf - (bx1 - corner)).max(0.0);
            let dy = (by0 + corner - yf).max(yf - (by1 - corner)).max(0.0);
            dx * dx + dy * dy <= corner * corner
        };
        if inside {
            let shade = 1.0 - (xf - bx0) / (bx1 - bx0) * 0.25;
            return image::Rgb([
                (f32::from(obj[0]) * shade) as u8,
                (f32::from(obj[1]) * shade) as u8,
                (f32::from(obj[2]) * shade) as u8,
            ]);
        }
        let d = ((xf - cx).powi(2) + (yf - cy).powi(2)).sqrt();
        if d < r {
            return image::Rgb(sun);
        }
        mix(top, bottom, yf / (hf * 0.8))
    });
    encode(DynamicImage::ImageRgb8(img), ImageFormat::Png)
}

pub fn gif(w: u32, h: u32, seed: u8) -> Vec<u8> {
    encode(
        DynamicImage::ImageRgba8(DynamicImage::ImageRgb8(rgb(w, h, seed)).to_rgba8()),
        ImageFormat::Gif,
    )
}

/// A lossless WebP.
pub fn webp(w: u32, h: u32, seed: u8) -> Vec<u8> {
    encode(
        DynamicImage::ImageRgba8(DynamicImage::ImageRgb8(rgb(w, h, seed)).to_rgba8()),
        ImageFormat::WebP,
    )
}

/// Insert an APP1 segment right after the JPEG start marker.
fn with_app1(jpeg: &[u8], payload: &[u8]) -> Vec<u8> {
    assert!(jpeg.starts_with(&[0xFF, 0xD8]));
    let len = (payload.len() + 2) as u16;
    let mut out = vec![0xFF, 0xD8, 0xFF, 0xE1];
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(payload);
    out.extend_from_slice(&jpeg[2..]);
    out
}

/// Text that appears in [`jpeg_with_gps`] and must never survive processing.
pub const GPS_MARKER: &str = "GPS-LAT-48.8584N-LON-2.2945E";

/// A JPEG whose EXIF carries a GPS marker string and an author name (a minimal EXIF block; what
/// matters for the test is that the bytes are present before and gone after).
pub fn jpeg_with_gps(w: u32, h: u32, seed: u8) -> Vec<u8> {
    let mut exif = b"Exif\0\0MM\0\x2a\0\0\0\x08".to_vec();
    exif.extend_from_slice(GPS_MARKER.as_bytes());
    exif.extend_from_slice(b"\0Artist=Jane Photographer\0");
    with_app1(&jpeg(w, h, seed), &exif)
}

/// A JPEG with an EXIF orientation of 6 (rotate 90 degrees clockwise to display): the stored
/// pixels are `w` wide and `h` tall, the picture is meant to be shown `h` wide and `w` tall.
pub fn jpeg_rotated(w: u32, h: u32, seed: u8) -> Vec<u8> {
    // TIFF header (big endian), one IFD entry: Orientation (0x0112), SHORT, count 1, value 6.
    let mut exif = b"Exif\0\0MM\0\x2a\0\0\0\x08".to_vec();
    exif.extend_from_slice(&[
        0, 1, 0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0,
    ]);
    with_app1(&jpeg(w, h, seed), &exif)
}

/// A valid JPEG with a script appended after the end-of-image marker.
pub fn jpeg_with_trailing_script(seed: u8) -> Vec<u8> {
    let mut b = jpeg(8, 8, seed);
    b.extend_from_slice(b"<script>alert('polyglot')</script>");
    b
}

/// A PNG that tells the truth in its header about being enormous (`w` by `h`) and contains no
/// pixel data: the size of a decompression bomb with the weight of a postcard.
pub fn png_bomb(w: u32, h: u32) -> Vec<u8> {
    fn crc32(data: &[u8]) -> u32 {
        let mut c = 0xFFFF_FFFFu32;
        for &b in data {
            c ^= u32::from(b);
            for _ in 0..8 {
                c = if c & 1 == 1 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
        }
        !c
    }
    fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut v = (data.len() as u32).to_be_bytes().to_vec();
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        v.extend_from_slice(&body);
        v.extend_from_slice(&crc32(&body).to_be_bytes());
        v
    }
    let mut ihdr = w.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    out.extend(chunk(b"IHDR", &ihdr));
    out.extend(chunk(b"IEND", &[]));
    out
}

/// An SVG with a script, as an attacker would upload it.
pub const SVG_WITH_SCRIPT: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"><script>alert(document.cookie)</script><rect width=\"10\" height=\"10\"/></svg>";
