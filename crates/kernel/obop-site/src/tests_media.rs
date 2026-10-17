//! Pictures on the public site (D60, docs/STOREFRONT.md "Threat model"): rendering, serving and
//! the rule that only published records through declared fields make a blob public.

use super::*;
use crate::tests::*;
use obop_storage::blob::{NewBlob, Owner};

fn get_h(s: &Shop, path: &str, headers: &[(&str, &str)]) -> Resp {
    crate::tests::get_h(s, path, headers)
}

/// The `src` of the first `<img>` whose tag contains `needle`.
fn img_src(html: &str, needle: &str) -> String {
    html.split("<img ")
        .skip(1)
        .find(|t| t.split('>').next().unwrap_or("").contains(needle))
        .and_then(|t| t.split("src=\"").nth(1))
        .and_then(|t| t.split('"').next())
        .unwrap_or_else(|| panic!("no img with {needle}"))
        .replace("&amp;", "&")
}

#[test]
fn the_seeded_shop_renders_real_images_with_srcset_dimensions_and_alt_text() {
    let s = shop();
    let h = text(&get(&s, "/shop"));
    assert!(h.contains("<img src=\"/media/"), "cards use real pictures");
    assert!(h.contains("srcset=\""), "responsive");
    assert!(h.contains("loading=\"lazy\""));
    assert!(h.contains("width=\"") && h.contains("height=\""));
    assert!(h.contains("alt=\"Illustration of sencha green tea, 100 g\""));
    // The product page: the main picture is eager, the gallery follows, no monogram.
    let l = listing(&s, "sencha-green-tea");
    let h = text(&get(&s, &format!("/shop/{}-sencha-green-tea", l.id)));
    assert!(
        h.contains("fetchpriority=\"high\""),
        "main image is the LCP element"
    );
    assert!(h.contains("class=\"thumbs\""), "gallery");
    assert_eq!(
        h.matches("<img ").count(),
        3,
        "main picture and two gallery pictures"
    );
    // Open Graph, Twitter and JSON-LD carry the picture.
    assert!(h.contains("<meta property=\"og:image\" content=\"http://shop.test/media/"));
    assert!(h.contains("<meta property=\"og:image:width\""));
    assert!(h.contains("<meta property=\"og:image:alt\""));
    assert!(h.contains("<meta name=\"twitter:card\" content=\"summary_large_image\">"));
    assert!(h.contains("<meta name=\"twitter:image\""));
    let ld = h
        .split("<script type=\"application/ld+json\">")
        .find(|b| b.contains("\"@type\":\"Product\""))
        .unwrap();
    assert!(ld.contains("\"image\":[\"http://shop.test/media/"), "{ld}");
    // A listing without a picture keeps the monogram.
    set_state(
        &s,
        "ecommerce_listing",
        &listing(&s, "linen-tea-towel").id,
        "image",
        Value::Null,
    );
    assert!(text(&get(&s, "/shop")).contains("aria-hidden=\"true\">L</div>"));
    // The sitemap lists the product pictures.
    let x = text(&get(&s, "/sitemap.xml"));
    assert!(x.contains("xmlns:image="));
    assert!(x.contains("<image:image><image:loc>http://shop.test/media/"));
    // Posts and the home page have covers too.
    assert!(text(&get(&s, "/blog")).contains("class=\"postimg\""));
    let h = text(&get(&s, "/blog/how-we-pick-our-teas"));
    assert!(h.contains("class=\"cover\"") && h.contains("og:image"));
    assert!(h.contains("\"image\":[\"http://shop.test/media/"));
    assert!(text(&get(&s, "/")).contains("class=\"cover\""));
}

#[test]
fn media_is_served_immutable_with_a_hash_etag_nosniff_and_fixed_thumbnail_widths() {
    let s = shop();
    let l = listing(&s, "sencha-green-tea");
    let html = text(&get(&s, &format!("/shop/{}-sencha-green-tea", l.id)));
    let src = img_src(&html, "fetchpriority");
    assert!(
        src.starts_with("/media/") && src.contains("?w=640"),
        "{src}"
    );
    let r = get(&s, &src);
    assert_eq!(r.status, 200);
    assert_eq!(r.get_header("content-type"), Some("image/png"));
    let cc = r.get_header("cache-control").unwrap();
    assert!(
        cc.contains("immutable") && cc.contains("max-age=31536000"),
        "{cc}"
    );
    assert_eq!(r.get_header("x-content-type-options"), Some("nosniff"));
    let etag = r.get_header("etag").unwrap().to_string();
    assert!(etag.contains("-w640"));
    assert!(r.body.starts_with(&[0x89, b'P', b'N', b'G']));
    // Revalidation answers 304 without a body.
    let r2 = get_h(&s, &src, &[("if-none-match", &etag)]);
    assert_eq!((r2.status, r2.body.len()), (304, 0));
    // The original, and only the fixed widths.
    let original = src.split('?').next().unwrap().to_string();
    let r3 = get(&s, &original);
    assert_eq!(r3.status, 200);
    assert!(r3.body.len() > r.body.len(), "the thumbnail is smaller");
    assert_eq!(get(&s, &format!("{original}?w=641")).status, 404);
    assert_eq!(get(&s, &format!("{original}?w=abc")).status, 404);
    assert_eq!(get(&s, &format!("{original}?w=320")).status, 200);
    // The extension has to match, ids are exact lowercase hex, and nothing is a path.
    assert_eq!(get(&s, &original.replace(".png", ".jpg")).status, 404);
    let id = original
        .trim_start_matches("/media/")
        .trim_end_matches(".png")
        .to_string();
    assert_eq!(
        get(&s, &format!("/media/{}.png", id.to_uppercase())).status,
        404
    );
    for bad in [
        "/media/..%2f..%2fetc%2fpasswd",
        "/media/../../etc/passwd",
        "/media/%2e%2e%2f%2e%2e%2fCargo.toml",
        "/media/short",
        "/media/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ] {
        let r = get(&s, bad);
        assert!(matches!(r.status, 404 | 301), "{bad} {}", r.status);
        assert!(!text(&r).contains("root:"), "{bad}");
    }
}

#[test]
fn security_unpublished_private_and_unattached_blobs_are_never_public() {
    let s = shop();
    let l = listing(&s, "sencha-green-tea");
    let main = l.text("image").to_string();
    assert_eq!(get(&s, &format!("/media/{main}.png")).status, 200);
    // An upload nobody holds yet.
    let loose = s
        .store
        .blobs()
        .put(
            &obop_media::fixtures::png(40, 40, 99),
            NewBlob {
                content_type: "image/png".into(),
                width: Some(40),
                height: Some(40),
                uploader: "someone".into(),
                ..NewBlob::default()
            },
        )
        .unwrap();
    assert_eq!(get(&s, &format!("/media/{}.png", loose.id)).status, 404);
    // Held by a field that no package declared public: the product's own image.
    let product = s
        .store
        .backend()
        .get("ecommerce_listing", &l.id)
        .unwrap()
        .unwrap()["product_id"]
        .as_str()
        .unwrap()
        .to_string();
    s.store
        .blobs()
        .attach(&loose.id, &Owner::new("product", &product, "image"))
        .unwrap();
    assert_eq!(get(&s, &format!("/media/{}.png", loose.id)).status, 404);
    // Gallery pictures follow their listing and their own flag.
    let (gid, g) = s
        .store
        .backend()
        .list("ecommerce_listing_image")
        .unwrap()
        .remove(0);
    let gblob = g["image"].as_str().unwrap().to_string();
    assert_eq!(get(&s, &format!("/media/{gblob}.png")).status, 200);
    set_state(
        &s,
        "ecommerce_listing_image",
        &gid,
        "published",
        Value::Bool(false),
    );
    assert_eq!(get(&s, &format!("/media/{gblob}.png")).status, 404);
    set_state(
        &s,
        "ecommerce_listing_image",
        &gid,
        "published",
        Value::Bool(true),
    );
    assert_eq!(get(&s, &format!("/media/{gblob}.png")).status, 200);
    // Unpublishing the listing takes the main picture and the gallery down with it.
    set_state(
        &s,
        "ecommerce_listing",
        &l.id,
        "published",
        Value::Bool(false),
    );
    assert_eq!(get(&s, &format!("/media/{main}.png")).status, 404);
    assert_eq!(get(&s, &format!("/media/{gblob}.png")).status, 404);
    // A draft post's cover is not public either.
    let (pid, p) = s
        .store
        .backend()
        .list("website_blog_post")
        .unwrap()
        .remove(0);
    let cover = p["cover_image"].as_str().unwrap().to_string();
    assert_eq!(get(&s, &format!("/media/{cover}.png")).status, 200);
    set_state(&s, "website_blog_post", &pid, "state", Value::text("draft"));
    assert_eq!(get(&s, &format!("/media/{cover}.png")).status, 404);
}

#[test]
fn media_answers_are_rate_limited_and_closed_with_the_site() {
    let s = shop();
    let l = listing(&s, "sencha-green-tea");
    let path = format!("/media/{}.png", l.text("image"));
    let mut last = 0;
    for _ in 0..901 {
        let mut r = req("GET", &path);
        r.client = "198.51.100.77".into();
        last = s.site.handle(&r).status;
    }
    assert_eq!(last, 429);
    set_site(&s, serde_json::json!({"enabled": false}));
    assert_eq!(get(&s, &path).status, 503);
}
