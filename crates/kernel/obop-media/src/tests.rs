use super::*;
use crate::fixtures as fx;

fn lim() -> Limits {
    Limits::default()
}

fn has(hay: &[u8], needle: &str) -> bool {
    hay.windows(needle.len()).any(|w| w == needle.as_bytes())
}

#[test]
fn the_four_formats_are_recognised_by_their_bytes_and_come_out_clean() {
    let cases = [
        (fx::png(40, 30, 1), "image/png", "image/png"),
        (fx::jpeg(40, 30, 1), "image/jpeg", "image/jpeg"),
        (fx::gif(40, 30, 1), "image/gif", "image/png"),
        (fx::webp(40, 30, 1), "image/webp", "image/jpeg"),
    ];
    for (bytes, declared, out) in cases {
        let p = process_image(&bytes, declared, &lim()).unwrap();
        assert_eq!(p.content_type, out, "{declared}");
        assert_eq!((p.width, p.height), (40, 30));
        assert_eq!(sniff(&p.bytes).map(Kind::mime), Some(out));
    }
    // A WebP with transparency stays lossless PNG.
    let a = process_image(&fx::png_alpha(40, 30, 2), "", &lim()).unwrap();
    assert_eq!(a.content_type, "image/png");
    // The declared type is optional, and the usual alias is tolerated.
    assert!(process_image(&fx::jpeg(8, 8, 1), "image/jpg", &lim()).is_ok());
    assert!(process_image(&fx::jpeg(8, 8, 1), "application/octet-stream", &lim()).is_ok());
}

#[test]
fn exif_gps_and_author_do_not_survive() {
    let raw = fx::jpeg_with_gps(32, 32, 3);
    assert!(
        has(&raw, fx::GPS_MARKER) && has(&raw, "Jane Photographer"),
        "fixture has the data"
    );
    let p = process_image(&raw, "image/jpeg", &lim()).unwrap();
    assert!(!has(&p.bytes, fx::GPS_MARKER));
    assert!(!has(&p.bytes, "Jane Photographer"));
    assert!(!has(&p.bytes, "Exif"));
}

#[test]
fn the_exif_orientation_is_applied_before_the_metadata_is_dropped() {
    let raw = fx::jpeg_rotated(40, 20, 4);
    assert_eq!(dimensions(&raw), Some((40, 20)));
    let p = process_image(&raw, "image/jpeg", &lim()).unwrap();
    assert_eq!((p.width, p.height), (20, 40), "rotated upright");
    assert_eq!(dimensions(&p.bytes), Some((20, 40)));
}

#[test]
fn trailing_scripts_are_not_kept_and_polyglots_are_refused() {
    let p = process_image(&fx::jpeg_with_trailing_script(5), "image/jpeg", &lim()).unwrap();
    assert!(!has(&p.bytes, "<script>"));
    // A GIF header followed by HTML is not an image.
    let poly = b"GIF89a<html><script>alert(1)</script></html>".to_vec();
    assert!(matches!(
        process_image(&poly, "image/gif", &lim()).unwrap_err(),
        MediaError::Corrupt | MediaError::Dimensions
    ));
    // A PNG called a JPEG is mislabelled, not silently accepted.
    assert!(matches!(
        process_image(&fx::png(8, 8, 1), "image/jpeg", &lim()),
        Err(MediaError::Mislabelled { .. })
    ));
    // HTML with an image type.
    assert_eq!(
        process_image(b"<html><body>hi</body></html>", "image/png", &lim()).unwrap_err(),
        MediaError::Svg
    );
}

#[test]
fn svg_is_always_rejected() {
    for declared in ["image/svg+xml", "image/png", ""] {
        assert_eq!(
            process_image(fx::SVG_WITH_SCRIPT.as_bytes(), declared, &lim()).unwrap_err(),
            MediaError::Svg,
            "{declared}"
        );
    }
    // With a byte order mark and leading space as well.
    let sneaky = format!("\u{feff}  \n{}", fx::SVG_WITH_SCRIPT);
    assert_eq!(
        process_image(sneaky.as_bytes(), "image/png", &lim()).unwrap_err(),
        MediaError::Svg
    );
    // A real PNG labelled SVG is also refused.
    assert_eq!(
        process_image(&fx::png(8, 8, 1), "image/svg+xml", &lim()).unwrap_err(),
        MediaError::Svg
    );
}

#[test]
fn size_and_dimension_limits_hold_before_pixels_are_allocated() {
    let small = Limits {
        max_bytes: 100,
        ..lim()
    };
    assert!(matches!(
        process_image(&fx::png(64, 64, 1), "", &small),
        Err(MediaError::TooLarge { .. })
    ));
    assert_eq!(
        process_image(&[], "", &lim()).unwrap_err(),
        MediaError::Empty
    );
    // 60,000 x 60,000 declared in a 60-byte file.
    let bomb = fx::png_bomb(60_000, 60_000);
    assert!(bomb.len() < 100);
    assert_eq!(
        process_image(&bomb, "image/png", &lim()).unwrap_err(),
        MediaError::Dimensions
    );
    // Within width and height each, too many pixels together.
    let wide = fx::png_bomb(7000, 7000);
    assert_eq!(
        process_image(&wide, "image/png", &lim()).unwrap_err(),
        MediaError::Dimensions
    );
    // Random bytes, truncated images.
    assert_eq!(
        process_image(&[1, 2, 3, 4, 5], "", &lim()).unwrap_err(),
        MediaError::Unsupported
    );
    let mut cut = fx::png(64, 64, 1);
    cut.truncate(cut.len() / 2);
    assert_eq!(
        process_image(&cut, "", &lim()).unwrap_err(),
        MediaError::Corrupt
    );
}

#[test]
fn thumbnails_scale_down_keep_the_format_and_never_scale_up() {
    let big = process_image(&fx::png(800, 400, 6), "", &lim()).unwrap();
    let t = thumbnail(&big.bytes, 320, &lim()).unwrap();
    assert_eq!((t.width, t.height, t.content_type), (320, 160, "image/png"));
    let j = process_image(&fx::jpeg(800, 400, 6), "", &lim()).unwrap();
    assert_eq!(
        thumbnail(&j.bytes, 640, &lim()).unwrap().content_type,
        "image/jpeg"
    );
    let same = thumbnail(&big.bytes, 1280, &lim()).unwrap();
    assert_eq!(same.width, 800, "never larger than the original");
}

#[test]
fn files_are_checked_against_a_short_list() {
    assert_eq!(
        check_file(b"%PDF-1.7 ...", "application/pdf", 1000),
        Ok("application/pdf")
    );
    assert_eq!(check_file(b"a,b\n1,2\n", "text/csv", 1000), Ok("text/csv"));
    assert_eq!(check_file(b"hello", "", 1000), Ok("text/plain"));
    let docx = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
    assert_eq!(check_file(b"PK\x03\x04xxxx", docx, 1000), Ok(docx));
    assert!(check_file(b"PK\x03\x04xxxx", "text/plain", 1000).is_err());
    assert!(check_file(&fx::png(8, 8, 1), "image/png", 100_000).is_ok());
    for (bytes, declared) in [
        (&b"<html><script>x</script></html>"[..], "text/html"),
        (fx::SVG_WITH_SCRIPT.as_bytes(), "image/svg+xml"),
        (fx::SVG_WITH_SCRIPT.as_bytes(), "text/plain"),
        (b"MZ\x90\x00\x03", "application/pdf"),
        (b"\x7fELF\x02\x01", "application/octet-stream"),
        (b"%PDF-1.4", "image/png"),
    ] {
        assert!(check_file(bytes, declared, 1000).is_err(), "{declared}");
    }
    assert!(matches!(
        check_file(&[b'a'; 2000], "text/plain", 1000),
        Err(MediaError::TooLarge { .. })
    ));
}
