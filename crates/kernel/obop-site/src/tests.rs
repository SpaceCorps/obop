//! HTTP-level tests of the public site over the real bundled packages: rendering, SEO, languages,
//! and the security properties of docs/STOREFRONT.md ("Threat model"), each with a named test.

use super::*;
use obop_commands::{Dispatcher, Store};
use obop_pkg::{Bundled, Installer, Marketplace};
use std::sync::Arc;

struct Shop {
    site: Site,
    store: Store,
    d: Arc<Dispatcher>,
}

fn build(store: Store, seeded: bool) -> Shop {
    let d = Arc::new(Dispatcher::new());
    base::register(&d, &store);
    product::register(&d, &store);
    let inst = Installer::new(
        store.clone(),
        &d,
        vec![base::module(), product::module()],
        Arc::new(Bundled),
    );
    if store
        .backend()
        .get("obop_package", "website")
        .unwrap()
        .is_some()
    {
        assert!(inst.load_installed().is_empty());
    } else {
        for id in ["website", "ecommerce"] {
            inst.install(&Bundled.fetch(id).unwrap(), "marketplace", "test")
                .unwrap_or_else(|e| panic!("{id}: {e}"));
        }
    }
    if seeded {
        seed::seed(&store, &d).unwrap();
    }
    let site = Site::new(
        PublicApi::new(d.clone(), store.clone()),
        Config {
            settings_ttl: Duration::ZERO,
            ..Config::default()
        },
    );
    Shop { site, store, d }
}

fn shop() -> Shop {
    build(Store::new(), true)
}

fn req(method: &str, path: &str) -> Req {
    let mut r = Req::new(method, path)
        .with_header("host", "shop.test")
        .with_header("accept-language", "en");
    r.client = "203.0.113.7".into();
    r
}

fn get(s: &Shop, path: &str) -> Resp {
    s.site.handle(&req("GET", path))
}

fn get_h(s: &Shop, path: &str, headers: &[(&str, &str)]) -> Resp {
    let mut r = req("GET", path);
    for (k, v) in headers {
        r = r.with_header(k, v);
    }
    s.site.handle(&r)
}

const LINK_ANY: &str = "<link rel=\"alternate\" hreflang=";
const LINK_EN: &str = "<link rel=\"alternate\" hreflang=\"en\"";
const LINK_DE: &str = "<link rel=\"alternate\" hreflang=\"de\"";
const LINK_FR: &str = "<link rel=\"alternate\" hreflang=\"fr\"";
const LINK_PL: &str = "<link rel=\"alternate\" hreflang=\"pl\"";
const LINK_FI: &str = "<link rel=\"alternate\" hreflang=\"fi\"";
const LINK_SV: &str = "<link rel=\"alternate\" hreflang=\"sv\"";
const LINK_ES: &str = "<link rel=\"alternate\" hreflang=\"es\"";
const LINK_X: &str = "<link rel=\"alternate\" hreflang=\"x-default\"";

const CSRF: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// A POST that carries a valid double-submit token.
fn post(s: &Shop, path: &str, fields: &[(&str, &str)]) -> Resp {
    post_from(s, path, fields, "203.0.113.7")
}

fn post_from(s: &Shop, path: &str, fields: &[(&str, &str)], client: &str) -> Resp {
    let mut all: Vec<(&str, &str)> = vec![("csrf", CSRF)];
    all.extend_from_slice(fields);
    let body = all
        .iter()
        .map(|(k, v)| format!("{}={}", util::urlenc(k), util::urlenc(v)))
        .collect::<Vec<_>>()
        .join("&");
    let mut r = req("POST", path)
        .with_header("cookie", &format!("obop_csrf={CSRF}"))
        .with_header("origin", "http://shop.test")
        .with_header("content-type", "application/x-www-form-urlencoded")
        .with_body(&body);
    r.client = client.into();
    s.site.handle(&r)
}

/// A POST from an address nobody used before, so rate limits do not interfere.
fn fresh_post(s: &Shop, path: &str, fields: &[(&str, &str)]) -> Resp {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    post_from(s, path, fields, &format!("10.9.{}.{}", n / 250, n % 250))
}

fn listing(s: &Shop, slug: &str) -> PublicRecord {
    let (rows, _) = s
        .site
        .api()
        .list("ecommerce_listing", &[], None, 0)
        .unwrap();
    rows.into_iter()
        .find(|r| r.text("slug") == slug)
        .unwrap_or_else(|| panic!("no listing {slug}"))
}

fn methods(s: &Shop) -> Vec<PublicRecord> {
    s.site
        .api()
        .list("ecommerce_shipping_method", &[], None, 0)
        .unwrap()
        .0
}

fn order_fields<'a>(
    lines: &'a [(&'a str, &'a str)],
    extra: &[(&'a str, &'a str)],
) -> Vec<(&'a str, &'a str)> {
    let mut f: Vec<(&str, &str)> = vec![
        ("customer_name", "Test Buyer"),
        ("customer_email", "buyer@example.test"),
        ("shipping_address", "1 Example Street\n00000 Testville"),
        ("idem", "00112233445566778899aabbccddeeff"),
    ];
    for (id, q) in lines {
        f.push(("id", id));
        f.push(("qty", q));
    }
    f.extend_from_slice(extra);
    f
}

fn text(r: &Resp) -> String {
    r.body_str()
}

fn set_site(s: &Shop, site: serde_json::Value) {
    let value = serde_json::json!({"site": site}).to_string();
    let mut f = obop_storage::Fields::new();
    f.insert("value".into(), serde_json::Value::String(value));
    let b = s.store.backend();
    if !b.update("obop_setting", "instance", &f).unwrap() {
        b.insert("obop_setting", "instance", &f).unwrap();
    }
}

fn set_state(s: &Shop, model: &str, id: &str, field: &str, v: Value) {
    s.store
        .update(model, id, [(field.to_string(), v)].into())
        .unwrap();
}

fn page_id(s: &Shop, slug: &str) -> String {
    s.site
        .api()
        .find_by("website_page", "slug", slug)
        .map(|l| match l {
            Lookup::Found(r) => r.id,
            other => panic!("{slug}: {other:?}"),
        })
        .unwrap()
}

fn add_page(s: &Shop, slug: &str, title: &str, body: &str, state: &str) -> String {
    s.store
        .insert(
            "website_page",
            [
                ("slug", Value::text(slug)),
                ("title", Value::text(title)),
                ("body", Value::text(body)),
                ("state", Value::text(state)),
                ("published_on", Value::text("2026-10-01")),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
        )
        .unwrap()
}

// ---- rendering and SEO -------------------------------------------------------------------

#[test]
fn the_home_page_has_unique_seo_metadata_and_structured_data() {
    let s = shop();
    let r = get(&s, "/");
    assert_eq!(r.status, 200);
    let h = text(&r);
    assert!(
        h.contains("<title>Fernhill Goods | Small-batch tea, ceramics and gifts</title>"),
        "{h}"
    );
    assert!(h.contains("name=\"description\" content=\"Small-batch tea, handmade ceramics"));
    assert!(h.contains("<link rel=\"canonical\" href=\"http://shop.test/\">"));
    assert!(h.contains("property=\"og:title\"") && h.contains("name=\"twitter:card\""));
    assert!(h.contains("\"@type\":\"WebSite\"") && h.contains("\"@type\":\"Organization\""));
    assert!(
        h.contains("<html lang=\"en\">") && h.contains("rel=\"alternate\" type=\"text/markdown\"")
    );
    assert!(h.contains("rel=\"alternate\" type=\"application/atom+xml\""));
    assert!(h.contains("<main id=\"main\"") && h.contains("class=\"skip\""));
    assert_eq!(h.matches("<h1").count(), 1, "one h1 per page");
    // The shop and the blog are on the home page.
    assert!(h.contains("Featured products") && h.contains("From the blog"));
}

#[test]
fn an_instance_with_nothing_published_shows_a_friendly_page_with_a_link_to_the_admin() {
    let s = build(Store::new(), false);
    let r = get(&s, "/");
    assert_eq!(r.status, 200);
    let h = text(&r);
    assert!(h.contains("This site is not published yet"));
    assert!(h.contains("href=\"/admin/\""));
    assert!(h.contains("noindex"));
}

#[test]
fn a_page_renders_markdown_with_the_title_as_the_only_h1() {
    let s = shop();
    let r = get(&s, "/about");
    assert_eq!(r.status, 200);
    let h = text(&r);
    assert!(h.contains("<h1>About Fernhill Goods</h1>"));
    assert!(h.contains("<h2>Who we are</h2>") && h.contains("<blockquote>"));
    assert!(h.contains("<link rel=\"canonical\" href=\"http://shop.test/about\">"));
    assert!(h.contains("\"@type\":\"BreadcrumbList\"") && h.contains("\"@type\":\"WebPage\""));
    assert_eq!(get(&s, "/home").status, 301);
    assert_eq!(get(&s, "/home").get_header("location"), Some("/"));
}

#[test]
fn unknown_unpublished_and_archived_pages_use_real_status_codes() {
    let s = shop();
    add_page(&s, "secret-draft", "Draft", "x", "draft");
    add_page(&s, "old-news", "Old", "x", "archived");
    assert_eq!(get(&s, "/no-such-page").status, 404);
    assert_eq!(get(&s, "/secret-draft").status, 404);
    assert_eq!(get(&s, "/old-news").status, 410);
    let (a, b) = (get(&s, "/no-such-page"), get(&s, "/secret-draft"));
    assert_eq!(
        a.body, b.body,
        "an unpublished page and a missing one are the same response"
    );
    assert!(text(&a).contains("noindex") && a.get_header("x-robots-tag") == Some("noindex"));
    assert_eq!(get(&s, "/shop/not-an-id").status, 404);
    assert_eq!(get(&s, "/blog/nope").status, 404);
}

#[test]
fn the_blog_lists_posts_newest_first_and_posts_have_article_data() {
    let s = shop();
    let h = text(&get(&s, "/blog"));
    let a = h.find("How we pick our teas").unwrap();
    let b = h.find("Caring for handmade ceramics").unwrap();
    let c = h.find("A slower morning ritual").unwrap();
    assert!(a < b && b < c, "newest first");
    assert!(h.contains("\"@type\":\"ItemList\""));
    let p = text(&get(&s, "/blog/how-we-pick-our-teas"));
    assert!(
        p.contains("\"@type\":\"BlogPosting\"") && p.contains("\"datePublished\":\"2026-09-12\"")
    );
    assert!(p.contains("The Fernhill team") && p.contains("September 12, 2026"));
    assert!(
        !p.contains("Editorial team"),
        "the private author field is not public"
    );
}

#[test]
fn a_product_page_has_product_and_offer_data_with_price_currency_and_availability() {
    let s = shop();
    let l = listing(&s, "stoneware-teapot");
    let r = get(&s, &product_path_of(&l));
    assert_eq!(r.status, 200);
    let h = text(&r);
    assert!(h.contains("\"@type\":\"Product\"") && h.contains("\"@type\":\"Offer\""));
    assert!(h.contains("\"price\":\"48.00\"") && h.contains("\"priceCurrency\":\"EUR\""));
    assert!(h.contains("https://schema.org/InStock"));
    assert!(h.contains("€48.00") && h.contains("data-add="));
    // Out of stock and backorder are told apart.
    let out = text(&get(
        &s,
        &product_path_of(&listing(&s, "pour-over-dripper")),
    ));
    assert!(out.contains("https://schema.org/OutOfStock") && !out.contains("data-add="));
    let back = text(&get(
        &s,
        &product_path_of(&listing(&s, "tea-cups-set-of-two")),
    ));
    assert!(back.contains("https://schema.org/BackOrder") && back.contains("data-add="));
}

fn product_path_of(l: &PublicRecord) -> String {
    format!("/shop/{}-{}", l.id, l.text("slug"))
}

#[test]
fn a_wrong_product_slug_redirects_to_the_canonical_url_and_nothing_else() {
    let s = shop();
    let l = listing(&s, "sencha-green-tea");
    let r = get(&s, &format!("/shop/{}-old-name", l.id));
    assert_eq!(r.status, 301);
    assert_eq!(r.get_header("location"), Some(product_path_of(&l).as_str()));
    // The redirect target is built from the stored slug, never from the request.
    let r = get(&s, &format!("/shop/{}-%0d%0aSet-Cookie:x=1", l.id));
    assert_eq!(r.status, 301);
    assert!(!r.get_header("location").unwrap().contains('\r'));
    assert!(r.get_header("set-cookie").is_none());
}

#[test]
fn the_shop_filters_by_category_searches_and_paginates() {
    let s = shop();
    set_site(&s, serde_json::json!({"shop_page_size": 4}));
    let all = text(&get(&s, "/shop"));
    assert!(
        all.contains("8 products") && all.contains("Page 1 of 2") && all.contains("rel=\"next\"")
    );
    assert_eq!(all.matches("class=\"card\"").count(), 4);
    let p2 = get(&s, "/shop?page=2");
    assert_eq!(p2.status, 200);
    assert!(text(&p2).contains("rel=\"prev\""));
    assert_eq!(get(&s, "/shop?page=9").status, 404);
    let tea = text(&get(&s, "/shop?cat=tea"));
    assert!(tea.contains("3 products") && tea.contains("Sencha") && !tea.contains("Teapot"));
    assert_eq!(get(&s, "/shop?cat=nope").status, 404);
    let q = get(&s, "/shop?q=teapot");
    let qh = text(&q);
    assert!(
        qh.contains("1 products")
            && qh.contains("Stoneware Teapot")
            && !qh.contains("Sencha Green")
    );
    assert!(
        qh.contains("noindex"),
        "search results stay out of the index"
    );
    assert!(text(&get(&s, "/shop?q=zzzz")).contains("No products match"));
}

#[test]
fn unpublished_listings_and_categories_are_not_shown_anywhere() {
    let s = shop();
    let l = listing(&s, "linen-tea-towel");
    set_state(
        &s,
        "ecommerce_listing",
        &l.id,
        "published",
        Value::Bool(false),
    );
    for path in ["/shop", "/sitemap.xml", "/llms.txt", "/"] {
        assert!(!text(&get(&s, path)).contains("Linen Tea Towel"), "{path}");
    }
    assert_eq!(get(&s, &product_path_of(&l)).status, 404);
}

// ---- languages ---------------------------------------------------------------------------

#[test]
fn language_comes_from_query_then_cookie_then_header_then_the_default() {
    let s = shop();
    let lang_of = |r: &Resp| text(r).split("<html lang=\"").nth(1).unwrap()[..2].to_string();
    assert_eq!(
        lang_of(&get_h(&s, "/", &[("accept-language", "fr-CH, fr;q=0.9")])),
        "fr"
    );
    let both = get_h(
        &s,
        "/?lang=de",
        &[("accept-language", "fr"), ("cookie", "lang=pl")],
    );
    assert_eq!(lang_of(&both), "de");
    assert!(
        both.get_header("set-cookie")
            .unwrap()
            .starts_with("lang=de;")
    );
    assert_eq!(
        lang_of(&get_h(
            &s,
            "/",
            &[("accept-language", "fr"), ("cookie", "lang=pl")]
        )),
        "pl"
    );
    assert_eq!(lang_of(&get_h(&s, "/", &[("accept-language", "ja")])), "en");
    set_site(&s, serde_json::json!({"default_lang": "sv"}));
    assert_eq!(lang_of(&get_h(&s, "/", &[("accept-language", "ja")])), "sv");
    // An invalid language is ignored and never reaches a header.
    let bad = get(&s, "/?lang=en%0d%0aSet-Cookie:x=1");
    assert!(bad.get_header("set-cookie").is_none());
}

#[test]
fn chrome_dates_and_money_follow_the_visitors_language() {
    let s = shop();
    let l = listing(&s, "stoneware-teapot");
    let de = text(&get(&s, &format!("{}?lang=de", product_path_of(&l))));
    assert!(
        de.contains("48,00\u{a0}€") && de.contains("In den Warenkorb") && de.contains("Auf Lager")
    );
    let pl = text(&get(&s, "/blog?lang=pl"));
    assert!(pl.contains("12 września 2026"), "{pl}");
    let fr = text(&get(&s, "/shop?lang=fr"));
    assert!(fr.contains("12,50\u{a0}€") && fr.contains("Rechercher des produits"));
}

#[test]
fn a_translation_is_used_when_present_and_hreflang_lists_only_existing_ones() {
    let s = shop();
    let about_de = text(&get(&s, "/about?lang=de"));
    assert!(about_de.contains("<h1>Über Fernhill Goods</h1>"));
    assert!(about_de.contains("<link rel=\"canonical\" href=\"http://shop.test/about?lang=de\">"));
    for want in [LINK_EN, LINK_DE, LINK_FR, LINK_X] {
        assert!(about_de.contains(want), "{want}");
    }
    for absent in [LINK_PL, LINK_FI, LINK_SV, LINK_ES] {
        assert!(!about_de.contains(absent), "{absent}");
    }
    // No Polish translation: the English text is served, marked as English inside a Polish page.
    let pl = text(&get(&s, "/about?lang=pl"));
    assert!(pl.contains("<h1>About Fernhill Goods</h1>") && pl.contains("<html lang=\"pl\">"));
    assert!(pl.contains("<div lang=\"en\">"));
    assert!(pl.contains("<link rel=\"canonical\" href=\"http://shop.test/about\">"));
    // A page without translations has no alternates.
    let d = text(&get(&s, "/delivery?lang=de"));
    assert!(!d.contains(LINK_ANY));
    // Listings and posts too.
    let l = listing(&s, "sencha-green-tea");
    let sencha = text(&get(&s, &format!("{}?lang=de", product_path_of(&l))));
    assert!(sencha.contains("Sencha Grüntee, 100 g"));
    let blog = text(&get(&s, "/blog/how-we-pick-our-teas?lang=de"));
    assert!(blog.contains("Wie wir unsere Tees auswählen"));
}

#[test]
fn an_unpublished_translation_is_never_used() {
    let s = shop();
    let (rows, _) = s
        .store
        .backend()
        .list("website_page_translation")
        .map(|r| (r, ()))
        .unwrap();
    for (id, _) in rows {
        set_state(
            &s,
            "website_page_translation",
            &id,
            "published",
            Value::Bool(false),
        );
    }
    let h = text(&get(&s, "/about?lang=de"));
    assert!(h.contains("About Fernhill Goods") && !h.contains("Über Fernhill"));
    assert!(!h.contains(LINK_ANY));
}

// ---- agents and caches -------------------------------------------------------------------

#[test]
fn every_page_is_also_markdown_for_agents() {
    let s = shop();
    let l = listing(&s, "sencha-green-tea");
    for path in [
        "/".to_string(),
        "/about".into(),
        "/blog".into(),
        "/blog/how-we-pick-our-teas".into(),
        "/shop".into(),
        product_path_of(&l),
        "/contact".into(),
    ] {
        for r in [
            get_h(&s, &path, &[("accept", "text/markdown")]),
            get(
                &s,
                &format!(
                    "{path}{}format=md",
                    if path.contains('?') { "&" } else { "?" }
                ),
            ),
        ] {
            assert_eq!(r.status, 200, "{path}");
            assert!(
                r.get_header("content-type")
                    .unwrap()
                    .starts_with("text/markdown"),
                "{path}"
            );
            let md = text(&r);
            assert!(md.starts_with("# "), "{path}: {md}");
            assert!(md.contains("Source: http://shop.test"), "{path}");
            assert!(!md.contains("<script") && !md.contains("<div"), "{path}");
            assert!(r.get_header("vary").unwrap().contains("Accept"));
        }
    }
    let md = text(&get(&s, &format!("{}?format=md", product_path_of(&l))));
    assert!(md.contains("**€12.50**"), "{md}");
}

#[test]
fn pages_carry_validators_and_answer_304() {
    let s = shop();
    let r = get(&s, "/about");
    let etag = r.get_header("etag").unwrap().to_string();
    assert!(r.get_header("last-modified").is_some(), "from published_on");
    assert!(
        r.get_header("cache-control")
            .unwrap()
            .contains("max-age=60")
    );
    let again = get_h(&s, "/about", &[("if-none-match", &etag)]);
    assert_eq!((again.status, again.body.len()), (304, 0));
    let lm = r.get_header("last-modified").unwrap().to_string();
    assert_eq!(
        get_h(&s, "/about", &[("if-modified-since", &lm)]).status,
        304
    );
    // HEAD works too.
    let mut h = req("HEAD", "/about");
    h.client = "x".into();
    assert_eq!(s.site.handle(&h).status, 200);
}

#[test]
fn the_sitemap_has_lastmod_alternates_and_only_published_urls() {
    let s = shop();
    add_page(&s, "hidden", "Hidden", "x", "draft");
    let x = text(&get(&s, "/sitemap.xml"));
    assert!(x.starts_with("<?xml") && x.contains("<loc>http://shop.test/</loc>"));
    assert!(x.contains("<loc>http://shop.test/about</loc><lastmod>2026-09-01</lastmod>"));
    assert!(x.contains(
        "<loc>http://shop.test/blog/how-we-pick-our-teas</loc><lastmod>2026-09-12</lastmod>"
    ));
    assert!(x.contains(
        "xhtml:link rel=\"alternate\" hreflang=\"de\" href=\"http://shop.test/about?lang=de\""
    ));
    assert!(x.contains("/shop/") && x.contains("/contact"));
    assert!(!x.contains("hidden"));
    assert_eq!(
        get(&s, "/sitemap.xml").get_header("content-type"),
        Some("application/xml; charset=utf-8")
    );
}

#[test]
fn robots_allows_crawlers_and_blocks_private_paths() {
    let s = shop();
    let t = text(&get(&s, "/robots.txt"));
    assert!(t.contains("User-agent: *\nAllow: /\n"));
    for p in [
        "/admin/",
        "/api/",
        "/cart",
        "/shop/checkout",
        "/shop/order/",
    ] {
        assert!(t.contains(&format!("Disallow: {p}\n")), "{p}");
    }
    assert!(t.contains("Sitemap: http://shop.test/sitemap.xml"));
    set_site(&s, serde_json::json!({"noindex": true}));
    assert!(text(&get(&s, "/robots.txt")).contains("Disallow: /\n"));
    assert!(text(&get(&s, "/about")).contains("content=\"noindex, follow\""));
}

#[test]
fn the_feed_llms_txt_and_security_txt_are_generated_from_published_content() {
    let s = shop();
    let f = get(&s, "/feed.xml");
    assert_eq!(
        f.get_header("content-type"),
        Some("application/atom+xml; charset=utf-8")
    );
    let x = text(&f);
    assert!(
        x.contains("<feed xmlns=\"http://www.w3.org/2005/Atom\"")
            && x.matches("<entry>").count() == 3
    );
    assert!(
        x.contains("<id>http://shop.test/blog/how-we-pick-our-teas</id>")
            && x.contains("<name>The Fernhill team</name>")
    );
    let l = text(&get(&s, "/llms.txt"));
    assert!(l.starts_with("# Fernhill Goods\n") && l.contains("> Small-batch tea"));
    assert!(l.contains("[About Fernhill Goods](http://shop.test/about?format=md)"));
    assert!(l.contains("Stoneware Teapot") && l.contains("€48.00"));
    assert_eq!(get(&s, "/.well-known/security.txt").status, 404);
    set_site(
        &s,
        serde_json::json!({"security_contact": "security@example.test"}),
    );
    let t = text(&get(&s, "/.well-known/security.txt"));
    assert!(
        t.contains("Contact: mailto:security@example.test\nExpires: 20")
            && t.contains("Canonical: http://shop.test/.well-known/security.txt")
    );
}

#[test]
fn methods_other_than_get_head_and_the_two_forms_are_refused() {
    let s = shop();
    for (m, p) in [
        ("PUT", "/"),
        ("DELETE", "/about"),
        ("POST", "/"),
        ("POST", "/shop"),
        ("PATCH", "/contact"),
    ] {
        let mut r = req(m, p);
        r.client = "x".into();
        let r = s.site.handle(&r);
        assert_eq!(r.status, 405, "{m} {p}");
        assert!(r.get_header("allow").is_some());
    }
}

#[test]
fn a_disabled_site_answers_503_and_robots_blocks_everything() {
    let s = shop();
    set_site(&s, serde_json::json!({"enabled": false}));
    for p in ["/", "/shop", "/about", "/cart"] {
        let r = get(&s, p);
        assert_eq!(r.status, 503, "{p}");
        assert_eq!(r.get_header("retry-after"), Some("3600"));
    }
    assert!(text(&get(&s, "/robots.txt")).contains("Disallow: /\n"));
    assert_eq!(get(&s, "/assets/site.css").status, 200, "assets still load");
}

#[test]
fn branding_comes_from_the_settings_with_a_contrast_safe_accent() {
    let s = shop();
    set_site(
        &s,
        serde_json::json!({
            "title": "Café <Nord>", "tagline": "Tea & \"cake\"", "accent": "#ffff00",
            "footer_text": "Footer <b>text</b>", "social": {"Mastodon": "https://social.example/@nord"},
            "logo_url": "https://cdn.example/logo.svg",
        }),
    );
    let h = text(&get(&s, "/about"));
    assert!(h.contains("Café &lt;Nord&gt;") && !h.contains("Café <Nord>"));
    assert!(h.contains("Footer &lt;b&gt;text&lt;/b&gt;") && h.contains("rel=\"me noopener\""));
    assert!(h.contains("src=\"https://cdn.example/logo.svg\""));
    let csp = get(&s, "/about")
        .get_header("content-security-policy")
        .unwrap()
        .to_string();
    assert!(
        csp.contains("img-src 'self' data: https://cdn.example;"),
        "{csp}"
    );
    let css = text(&get(&s, "/assets/site.css"));
    // Yellow on white is unreadable, so the fill was darkened until it passes.
    let fill = css
        .split("--fill:")
        .nth(1)
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    assert!(theme::contrast(fill, theme::LIGHT_PANEL) >= 3.0, "{fill}");
    assert!(css.contains("prefers-color-scheme:dark"));
    let r = get(&s, "/assets/site.css?v=abc");
    assert!(r.get_header("cache-control").unwrap().contains("immutable"));
}

#[test]
fn the_menu_is_the_published_menu_items_and_ignores_unsafe_urls() {
    let s = shop();
    let id = s
        .store
        .insert(
            "website_menu_item",
            [
                ("label".to_string(), Value::text("Evil")),
                ("url".to_string(), Value::text("javascript:alert(1)")),
                ("visible".to_string(), Value::Bool(true)),
                ("sequence".to_string(), Value::Int(9)),
            ]
            .into(),
        )
        .unwrap();
    let h = text(&get(&s, "/about"));
    assert!(h.contains(">Journal</a>") && h.contains(">Delivery</a>"));
    assert!(!h.contains("javascript:") && !h.contains("Evil"));
    set_state(&s, "website_menu_item", &id, "visible", Value::Bool(false));
}

#[test]
fn the_contact_page_text_comes_from_a_published_contact_page() {
    let s = shop();
    let h = text(&get(&s, "/contact"));
    assert!(h.contains("We usually reply within two working days."));
    assert!(h.contains("name=\"csrf\"") && h.contains("name=\"website\""));
    assert!(
        get(&s, "/contact")
            .get_header("set-cookie")
            .unwrap()
            .starts_with("obop_csrf=")
    );
    assert!(
        get(&s, "/contact")
            .get_header("cache-control")
            .unwrap()
            .contains("no-store")
    );
}

// ---- cart and checkout -------------------------------------------------------------------

#[test]
fn the_cart_page_shows_server_prices_and_works_without_javascript() {
    let s = shop();
    let (sencha, teapot) = (
        listing(&s, "sencha-green-tea"),
        listing(&s, "stoneware-teapot"),
    );
    let h = text(&get(&s, &format!("/cart?add={}", sencha.id)));
    assert!(h.contains("Sencha Green Tea, 100 g") && h.contains("€12.50"));
    assert!(h.contains("data-cart-page") && h.contains(&format!("data-items=\"{}.1\"", sencha.id)));
    // A second product through the plain form, and a quantity.
    let two = text(&get(
        &s,
        &format!("/cart?items={}.2&add={}", sencha.id, teapot.id),
    ));
    assert!(
        two.contains("€25.00") && two.contains("€48.00") && two.contains("€73.00"),
        "{two}"
    );
    // Delivery options and their prices come from the shipping methods; free over the threshold.
    assert!(two.contains("Standard delivery") && two.contains("Express delivery"));
    assert!(two.contains("name=\"shipping_method_id\""));
    // Quantity 0 removes; unknown ids are dropped; the cart is never cached.
    let gone = text(&get(&s, &format!("/cart?id={}&qty=0", sencha.id)));
    assert!(gone.contains("Your cart is empty."));
    let junk = text(&get(&s, "/cart?items=not-an-id.3~zzz"));
    assert!(junk.contains("Your cart is empty."));
    let r = get(&s, &format!("/cart?add={}", sencha.id));
    assert!(r.get_header("cache-control").unwrap().contains("no-store"));
    assert!(text(&r).contains("noindex"));
}

fn new_orders(s: &Shop) -> usize {
    s.store.count("ecommerce_order")
}

#[test]
fn checkout_creates_one_order_with_lines_priced_by_the_server() {
    let s = shop();
    let teapot = listing(&s, "stoneware-teapot");
    let std = methods(&s)
        .into_iter()
        .find(|m| m.text("name") == "Standard delivery")
        .unwrap();
    let lines = [(teapot.id.as_str(), "2")];
    let r = post(
        &s,
        "/shop/checkout",
        &order_fields(&lines, &[("shipping_method_id", &std.id)]),
    );
    assert_eq!(r.status, 303, "{}", text(&r));
    let loc = r.get_header("location").unwrap().to_string();
    assert!(loc.starts_with("/shop/order/") && loc.len() == "/shop/order/".len() + 64);
    assert_eq!(new_orders(&s), 1);
    assert_eq!(s.store.count("ecommerce_order_line"), 1);
    let b = s.store.backend();
    let (_, order) = b.list("ecommerce_order").unwrap().remove(0);
    // 2 x 48.00, and free delivery over 50.00.
    assert_eq!(order["subtotal"], 9600);
    assert_eq!(order["shipping_total"], 0);
    assert_eq!(order["total"], 9600);
    assert_eq!(order["state"], "pending");
    assert_eq!(order["currency"], "EUR");
    assert!(order["number"].as_str().unwrap().starts_with('W'));
    let (_, line) = b.list("ecommerce_order_line").unwrap().remove(0);
    assert_eq!(line["unit_price"], 4800);
    assert_eq!(line["quantity"], 2);
    assert_eq!(line["subtotal"], 9600);
    assert_eq!(line["description"], "Stoneware Teapot, 0.8 l");
    assert!(
        line["product_id"].is_string(),
        "the product behind the listing"
    );
    assert_eq!(line["listing_id"], teapot.id.as_str());
    // The confirmation page shows the order to whoever holds the link.
    let page = get(&s, &loc);
    assert_eq!(page.status, 200);
    let h = text(&page);
    assert!(h.contains("Thank you for your order") && h.contains("buyer@example.test"));
    assert!(h.contains("Stoneware Teapot, 0.8 l") && h.contains("€96.00"));
    assert!(h.contains("data-clear-cart") && h.contains("we will contact you"));
}

#[test]
fn security_checkout_never_trusts_prices_totals_or_state_from_the_browser() {
    let s = shop();
    let l = listing(&s, "sencha-green-tea");
    let std = methods(&s).remove(0);
    let lines = [(l.id.as_str(), "1")];
    let forged = [
        ("price", "1"),
        ("unit_price", "1"),
        ("subtotal", "1"),
        ("total", "1"),
        ("shipping_total", "0"),
        ("discount_total", "9999"),
        ("state", "paid"),
        ("number", "FREE"),
        ("access_token", "a".repeat(64).leak()),
        ("product_id", "x"),
        ("currency", "USD"),
        ("shipping_method_id", &std.id),
    ];
    let r = post(&s, "/shop/checkout", &order_fields(&lines, &forged));
    assert_eq!(r.status, 303);
    let (_, o) = s.store.backend().list("ecommerce_order").unwrap().remove(0);
    assert_eq!(o["subtotal"], 1250);
    assert_eq!(o["shipping_total"], 490);
    assert_eq!(o["total"], 1740);
    assert_eq!(o["state"], "pending");
    assert_eq!(o["currency"], "EUR");
    assert_eq!(o["discount_total"], 0);
    assert_ne!(o["number"], "FREE");
    assert_ne!(o["access_token"], "a".repeat(64).as_str());
    let (_, line) = s
        .store
        .backend()
        .list("ecommerce_order_line")
        .unwrap()
        .remove(0);
    assert_eq!(line["unit_price"], 1250);
}

#[test]
fn security_checkout_refuses_unpublished_and_unknown_listings() {
    let s = shop();
    let hidden = listing(&s, "linen-tea-towel");
    set_state(
        &s,
        "ecommerce_listing",
        &hidden.id,
        "published",
        Value::Bool(false),
    );
    for id in [hidden.id.as_str(), "00000000-0000-7000-8000-000000000000"] {
        let lines = [(id, "1")];
        let std = methods(&s).remove(0);
        let r = post(
            &s,
            "/shop/checkout",
            &order_fields(&lines, &[("shipping_method_id", &std.id)]),
        );
        assert_eq!(r.status, 400, "{id}");
        assert!(text(&r).contains("no longer available"));
    }
    assert_eq!(new_orders(&s), 0);
    // A shipping method that is not an active public one is refused too.
    let l = listing(&s, "sencha-green-tea");
    let lines = [(l.id.as_str(), "1")];
    let r = post(
        &s,
        "/shop/checkout",
        &order_fields(&lines, &[("shipping_method_id", "nope")]),
    );
    assert_eq!(r.status, 400);
    assert_eq!(new_orders(&s), 0);
}

#[test]
fn security_checkout_validates_quantities_stock_and_fields() {
    let s = shop();
    let std = methods(&s).remove(0);
    let sencha = listing(&s, "sencha-green-tea"); // block_when_out, 40 in stock
    let dripper = listing(&s, "pour-over-dripper"); // out of stock
    let cups = listing(&s, "tea-cups-set-of-two"); // backorder
    for q in ["0", "-1", "100", "abc", "1.5"] {
        let lines = [(sencha.id.as_str(), q)];
        let r = fresh_post(
            &s,
            "/shop/checkout",
            &order_fields(&lines, &[("shipping_method_id", &std.id)]),
        );
        assert!(r.status == 400, "qty {q}: {}", r.status);
    }
    let lines = [(sencha.id.as_str(), "40")];
    assert_eq!(
        fresh_post(
            &s,
            "/shop/checkout",
            &order_fields(&lines, &[("shipping_method_id", &std.id)])
        )
        .status,
        303,
        "40 is exactly the stock"
    );
    let over = [(sencha.id.as_str(), "41")];
    let mut f = order_fields(&over, &[("shipping_method_id", &std.id)]);
    f.retain(|(k, _)| *k != "idem");
    f.push(("idem", "0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a"));
    let r = fresh_post(&s, "/shop/checkout", &f);
    assert_eq!(r.status, 400);
    assert!(text(&r).contains("Not enough stock"));
    let lines = [(dripper.id.as_str(), "1")];
    assert_eq!(
        fresh_post(
            &s,
            "/shop/checkout",
            &order_fields(&lines, &[("shipping_method_id", &std.id)])
        )
        .status,
        400
    );
    let lines = [(cups.id.as_str(), "3")];
    assert_eq!(
        fresh_post(&s, "/shop/checkout", &{
            let mut f = order_fields(&lines, &[("shipping_method_id", &std.id)]);
            f.retain(|(k, _)| *k != "idem");
            f.push(("idem", "ffeeddccbbaa99887766554433221100"));
            f
        })
        .status,
        303,
        "backorder is allowed"
    );
    // Field validation.
    let l = [(sencha.id.as_str(), "1")];
    for (field, bad) in [
        ("customer_email", "nope"),
        ("customer_email", "a@b"),
        ("customer_name", ""),
        ("shipping_address", "x"),
    ] {
        let mut f = order_fields(&l, &[("shipping_method_id", &std.id)]);
        f.retain(|(k, _)| *k != field);
        f.push((field, bad));
        assert_eq!(
            fresh_post(&s, "/shop/checkout", &f).status,
            400,
            "{field}={bad}"
        );
    }
    // No empty carts.
    assert_eq!(
        fresh_post(&s, "/shop/checkout", &order_fields(&[], &[])).status,
        400
    );
    assert_eq!(new_orders(&s), 2);
}

#[test]
fn security_checkout_needs_a_csrf_token_that_matches_the_cookie() {
    let s = shop();
    let l = listing(&s, "sencha-green-tea");
    let std = methods(&s).remove(0);
    let lines = [(l.id.as_str(), "1")];
    let fields = order_fields(&lines, &[("shipping_method_id", &std.id)]);
    let raw = |csrf: Option<&str>, cookie: Option<&str>, origin: Option<&str>| {
        let mut all: Vec<(&str, &str)> = fields.clone();
        if let Some(c) = csrf {
            all.push(("csrf", c));
        }
        let body = all
            .iter()
            .map(|(k, v)| format!("{}={}", util::urlenc(k), util::urlenc(v)))
            .collect::<Vec<_>>()
            .join("&");
        let mut r = req("POST", "/shop/checkout").with_body(&body);
        r.client = "203.0.113.9".into();
        if let Some(c) = cookie {
            r = r.with_header("cookie", &format!("obop_csrf={c}"));
        }
        if let Some(o) = origin {
            r = r.with_header("origin", o);
        }
        s.site.handle(&r)
    };
    assert_eq!(raw(None, None, None).status, 403, "no token at all");
    assert_eq!(raw(Some(CSRF), None, None).status, 403, "no cookie");
    assert_eq!(raw(None, Some(CSRF), None).status, 403, "no field");
    assert_eq!(
        raw(Some("x"), Some("x"), None).status,
        403,
        "not a real token"
    );
    let other = "f".repeat(64);
    assert_eq!(raw(Some(CSRF), Some(&other), None).status, 403, "mismatch");
    assert_eq!(
        raw(Some(CSRF), Some(CSRF), Some("http://evil.test")).status,
        403,
        "cross-origin"
    );
    assert_eq!(
        raw(Some(CSRF), Some(CSRF), Some("null")).status,
        403,
        "opaque origin"
    );
    assert_eq!(new_orders(&s), 0);
    assert_eq!(
        raw(Some(CSRF), Some(CSRF), Some("http://shop.test")).status,
        303
    );
    assert_eq!(new_orders(&s), 1);
}

#[test]
fn security_the_honeypot_creates_nothing_and_looks_like_success() {
    let s = shop();
    let l = listing(&s, "sencha-green-tea");
    let std = methods(&s).remove(0);
    let lines = [(l.id.as_str(), "1")];
    let r = post(
        &s,
        "/shop/checkout",
        &order_fields(
            &lines,
            &[
                ("shipping_method_id", &std.id),
                ("website", "http://spam.test"),
            ],
        ),
    );
    assert_eq!(r.status, 303);
    assert_eq!(new_orders(&s), 0);
    let r = post(
        &s,
        "/contact",
        &[
            ("name", "Bot"),
            ("email", "bot@example.test"),
            ("message", "buy now"),
            ("consent", "1"),
            ("website", "x"),
        ],
    );
    assert_eq!(r.status, 303);
    assert_eq!(s.store.count("website_form_submission"), 0);
}

#[test]
fn security_a_double_submit_creates_one_order() {
    let s = shop();
    let l = listing(&s, "sencha-green-tea");
    let std = methods(&s).remove(0);
    let lines = [(l.id.as_str(), "1")];
    let f = order_fields(&lines, &[("shipping_method_id", &std.id)]);
    let first = post(&s, "/shop/checkout", &f);
    let second = post(&s, "/shop/checkout", &f);
    assert_eq!((first.status, second.status), (303, 303));
    assert_eq!(first.get_header("location"), second.get_header("location"));
    assert_eq!(new_orders(&s), 1);
    // A new idempotency key is a new order.
    let mut g = f.clone();
    g.retain(|(k, _)| *k != "idem");
    g.push(("idem", "11112222333344445555666677778888"));
    assert_eq!(post(&s, "/shop/checkout", &g).status, 303);
    assert_eq!(new_orders(&s), 2);
}

#[test]
fn security_checkout_is_rate_limited_per_address_and_bodies_are_capped() {
    let s = shop();
    let l = listing(&s, "sencha-green-tea");
    let std = methods(&s).remove(0);
    let lines = [(l.id.as_str(), "1")];
    let mut codes = Vec::new();
    for n in 0..12 {
        let idem = format!("{n:032x}");
        let mut f = order_fields(&lines, &[("shipping_method_id", &std.id)]);
        f.retain(|(k, _)| *k != "idem");
        f.push(("idem", &idem));
        codes.push(post_from(&s, "/shop/checkout", &f, "198.51.100.20").status);
    }
    assert_eq!(codes.iter().filter(|c| **c == 303).count(), 8, "{codes:?}");
    assert!(codes.contains(&429));
    // Another address is not affected by the first one's limit.
    let ok = post_from(
        &s,
        "/shop/checkout",
        &order_fields(&lines, &[("shipping_method_id", &std.id)]),
        "198.51.100.21",
    );
    assert_eq!(ok.status, 303);
    // Oversized bodies are refused before parsing.
    let big = "x".repeat(MAX_BODY + 1);
    let mut r = req("POST", "/shop/checkout").with_body(&big);
    r.client = "z".into();
    assert_eq!(s.site.handle(&r).status, 413);
    let r = post(
        &s,
        "/contact",
        &[("name", "x"), ("message", &"y".repeat(MAX_BODY))],
    );
    assert_eq!(r.status, 413);
}

#[test]
fn security_the_model_rate_limit_stops_a_flood_of_orders() {
    let s = shop();
    let l = listing(&s, "sencha-green-tea");
    let std = methods(&s).remove(0);
    let lines = [(l.id.as_str(), "1")];
    // The package declares 10 orders per hour per source, below the burst limit's reach when
    // the burst is spread out: use distinct idempotency keys and the same source.
    let mut denied = 0;
    for n in 0..14 {
        let idem = format!("{n:032x}");
        let mut f = order_fields(&lines, &[("shipping_method_id", &std.id)]);
        f.retain(|(k, _)| *k != "idem");
        f.push(("idem", &idem));
        s.site.reset_burst_limits_for_test("192.0.2.50"); // keep the package limit
        if post_from(&s, "/shop/checkout", &f, "192.0.2.50").status == 429 {
            denied += 1;
        }
    }
    assert_eq!(new_orders(&s), 10);
    assert_eq!(denied, 4);
}

#[test]
fn security_the_confirmation_page_is_reachable_only_with_the_token_and_shows_only_that_order() {
    let s = shop();
    let l = listing(&s, "sencha-green-tea");
    let std = methods(&s).remove(0);
    let lines = [(l.id.as_str(), "1")];
    let a = post(
        &s,
        "/shop/checkout",
        &order_fields(&lines, &[("shipping_method_id", &std.id)]),
    );
    let mut f = order_fields(
        &lines,
        &[
            ("shipping_method_id", &std.id),
            ("customer_email", "other@example.test"),
        ],
    );
    f.retain(|(k, _)| *k != "idem");
    f.push(("idem", "99998888777766665555444433332222"));
    f.retain(|(k, v)| !(*k == "customer_email" && *v == "buyer@example.test"));
    post(&s, "/shop/checkout", &f);
    assert_eq!(new_orders(&s), 2);
    let loc = a.get_header("location").unwrap().to_string();
    let page = get(&s, &loc);
    let h = text(&page);
    assert!(h.contains("buyer@example.test") && !h.contains("other@example.test"));
    assert_eq!(page.get_header("referrer-policy"), Some("no-referrer"));
    assert!(
        page.get_header("cache-control")
            .unwrap()
            .contains("no-store")
    );
    assert!(h.contains("noindex"));
    // Wrong, short, guessed and listing-style tokens are all the same 404.
    let (none, short, wrong) = (
        get(&s, "/shop/order/"),
        get(&s, "/shop/order/abc"),
        get(&s, &format!("/shop/order/{}", "0".repeat(64))),
    );
    assert_eq!((none.status, short.status, wrong.status), (404, 404, 404));
    assert_eq!(short.body, wrong.body);
    // The order number is not a token.
    let (_, o) = s.store.backend().list("ecommerce_order").unwrap().remove(0);
    let number = o["number"].as_str().unwrap();
    assert_eq!(get(&s, &format!("/shop/order/{number}")).status, 404);
    // Brute force is slowed down.
    let mut statuses = Vec::new();
    for n in 0..70 {
        statuses.push(get(&s, &format!("/shop/order/{n:064x}")).status);
    }
    assert!(statuses.contains(&429));
}

#[test]
fn security_nothing_a_visitor_submitted_appears_on_any_public_page() {
    let s = shop();
    let l = listing(&s, "sencha-green-tea");
    let std = methods(&s).remove(0);
    let lines = [(l.id.as_str(), "1")];
    post(
        &s,
        "/shop/checkout",
        &order_fields(
            &lines,
            &[
                ("shipping_method_id", &std.id),
                ("customer_name", "Zelda Secretname"),
            ],
        ),
    );
    post(
        &s,
        "/contact",
        &[
            ("name", "Quentin Hidden"),
            ("email", "quentin@example.test"),
            ("message", "private words"),
            ("phone", "+000 111"),
            ("consent", "1"),
        ],
    );
    assert_eq!(s.store.count("website_form_submission"), 1);
    for path in [
        "/",
        "/shop",
        "/blog",
        "/about",
        "/contact",
        "/sitemap.xml",
        "/feed.xml",
        "/llms.txt",
        "/cart",
        "/robots.txt",
    ] {
        let t = text(&get(&s, path));
        for secret in [
            "buyer@example.test",
            "Zelda",
            "Quentin",
            "quentin@example",
            "private words",
            "+000",
            "Testville",
        ] {
            assert!(!t.contains(secret), "{path} leaks {secret}");
        }
    }
    // The contact submission got its server-side values and the governed first state.
    let (_, sub) = s
        .store
        .backend()
        .list("website_form_submission")
        .unwrap()
        .remove(0);
    assert_eq!(sub["form_name"], "contact");
    assert_eq!(sub["state"], "new");
    assert_eq!(sub["consent_given"], true);
}

#[test]
fn security_mass_assignment_on_the_contact_form_is_ignored() {
    let s = shop();
    let r = post(
        &s,
        "/contact",
        &[
            ("name", "Real Name"),
            ("email", "a@b.test"),
            ("message", "hello"),
            ("consent", "1"),
            ("state", "handled"),
            ("form_name", "evil"),
            ("source_page", "x"),
            ("id", "forced-id"),
            ("submitted_on", "1999-01-01"),
        ],
    );
    assert_eq!(r.status, 303);
    let (id, sub) = s
        .store
        .backend()
        .list("website_form_submission")
        .unwrap()
        .remove(0);
    assert_eq!(sub["state"], "new");
    assert_eq!(sub["form_name"], "contact");
    assert_eq!(sub["source_page"], "contact");
    assert_ne!(id, "forced-id");
    assert_ne!(sub["submitted_on"], "1999-01-01");
}

#[test]
fn the_contact_form_validates_and_keeps_what_the_visitor_typed_escaped() {
    let s = shop();
    let r = post(
        &s,
        "/contact",
        &[
            ("name", "<script>alert(1)</script>"),
            ("email", "not-an-email"),
            ("message", ""),
            ("consent", ""),
        ],
    );
    assert_eq!(r.status, 400);
    let h = text(&r);
    assert!(h.contains("role=\"alert\"") && h.contains("Enter a valid email address."));
    assert!(h.contains("Write a message.") && h.contains("Please agree to be contacted."));
    assert!(!h.contains("<script>alert(1)"), "{h}");
    assert!(h.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert_eq!(s.store.count("website_form_submission"), 0);
    let ok = post(
        &s,
        "/contact",
        &[
            ("name", "Ada"),
            ("email", "ada@example.test"),
            ("message", "Hi there"),
            ("consent", "1"),
        ],
    );
    assert_eq!(ok.status, 303);
    assert_eq!(ok.get_header("location"), Some("/contact?sent=1"));
    assert!(text(&get(&s, "/contact?sent=1")).contains("Message sent"));
}

#[test]
fn security_contact_submissions_are_rate_limited_by_the_package() {
    let s = shop();
    let mut codes = Vec::new();
    for n in 0..13 {
        let msg = format!("message {n}");
        codes.push(
            post_from(
                &s,
                "/contact",
                &[
                    ("name", "Ada"),
                    ("email", "ada@example.test"),
                    ("message", &msg),
                    ("consent", "1"),
                ],
                "192.0.2.77",
            )
            .status,
        );
    }
    assert_eq!(codes.iter().filter(|c| **c == 303).count(), 10, "{codes:?}");
    assert_eq!(codes.iter().filter(|c| **c == 429).count(), 3);
    assert_eq!(s.store.count("website_form_submission"), 10);
}

// ---- content is hostile -----------------------------------------------------------------

#[test]
fn security_everything_owner_written_is_escaped_in_every_place_it_is_used() {
    let s = shop();
    let evil = "<script>alert(1)</script><img src=x onerror=alert(2)>";
    let l = listing(&s, "stoneware-teapot");
    for (field, model, id) in [
        ("title", "ecommerce_listing", l.id.clone()),
        ("summary", "ecommerce_listing", l.id.clone()),
        ("sku", "ecommerce_listing", l.id.clone()),
    ] {
        set_state(&s, model, &id, field, Value::text(evil));
    }
    let cat = s
        .site
        .api()
        .list("ecommerce_category", &[], None, 0)
        .unwrap()
        .0
        .remove(0);
    set_state(&s, "ecommerce_category", &cat.id, "name", Value::text(evil));
    let about = page_id(&s, "about");
    set_state(&s, "website_page", &about, "title", Value::text(evil));
    set_state(
        &s,
        "website_page",
        &about,
        "seo_description",
        Value::text(evil),
    );
    let (posts, _) = s
        .site
        .api()
        .list("website_blog_post", &[], None, 0)
        .unwrap();
    set_state(
        &s,
        "website_blog_post",
        &posts[0].id,
        "title",
        Value::text(evil),
    );
    set_state(
        &s,
        "website_blog_post",
        &posts[0].id,
        "byline",
        Value::text(evil),
    );
    set_state(
        &s,
        "website_blog_post",
        &posts[0].id,
        "tags",
        Value::text(evil),
    );
    set_site(
        &s,
        serde_json::json!({"title": evil, "tagline": evil, "footer_text": evil, "logo_text": evil}),
    );
    let pids: Vec<String> = [
        "/",
        "/shop",
        "/blog",
        "/about",
        "/contact",
        "/shop?cat=tea",
        "/sitemap.xml",
        "/feed.xml",
        "/llms.txt",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let mut paths = pids;
    paths.push(format!("/shop/{}-{}", l.id, l.text("slug")));
    paths.push(format!("/blog/{}", posts[0].text("slug")));
    paths.push("/about?format=md".into());
    for p in paths {
        let t = text(&get(&s, &p));
        assert!(!t.contains("<script>alert"), "{p}: {t}");
        assert!(!t.contains("<img src=x"), "{p}");
        assert!(!t.contains("onerror=alert(2)>"), "{p}");
    }
}

#[test]
fn security_json_ld_cannot_close_its_script_tag() {
    let s = shop();
    let l = listing(&s, "stoneware-teapot");
    set_state(
        &s,
        "ecommerce_listing",
        &l.id,
        "title",
        Value::text("</script><script>alert(1)</script>"),
    );
    let t = text(&get(&s, &format!("/shop/{}-{}", l.id, l.text("slug"))));
    assert!(!t.contains("</script><script>alert"));
    // Only our own script tags exist: the cart script plus one JSON-LD block per entry.
    let opens = t.matches("<script").count();
    let closes = t.matches("</script>").count();
    assert_eq!(opens, closes);
    assert!(t.contains("\\u003c/script\\u003e"));
}

#[test]
fn security_dangerous_links_and_html_in_markdown_bodies_are_neutralised() {
    let s = shop();
    add_page(
        &s,
        "evil",
        "Evil",
        "[a](javascript:alert(1)) [b](data:text/html,x) [c](//evil.test) <a href=\"javascript:alert(1)\">d</a> ![e](javascript:alert(1)) <iframe src=\"https://evil.test\"></iframe> <svg onload=alert(1)>",
        "published",
    );
    let html = text(&get(&s, "/evil")).to_lowercase();
    // The raw HTML the author typed is shown as text, so look at real tags only.
    let body = html
        .split("<article>")
        .nth(1)
        .unwrap()
        .split("</article>")
        .next()
        .unwrap();
    for bad in [
        "<a href=\"javascript",
        "<a href=\"data:",
        "<a href=\"//",
        "<a href=\"vbscript",
    ] {
        assert!(!body.contains(bad), "{bad}: {body}");
    }
    for bad in ["<iframe", "<svg", "<img", "<script"] {
        assert!(!body.contains(bad), "{bad}: {body}");
    }
    let md = text(&get(&s, "/evil?format=md")).to_lowercase();
    for bad in ["](javascript", "](data:", "](//", "<iframe", "<svg", "<a "] {
        assert!(!md.contains(bad), "{bad}: {md}");
    }
}

#[test]
fn security_every_response_carries_the_clickjacking_and_content_policy_headers() {
    let s = shop();
    let l = listing(&s, "sencha-green-tea");
    for p in [
        "/",
        "/shop",
        "/about",
        "/missing",
        "/contact",
        "/cart",
        "/sitemap.xml",
        "/robots.txt",
        "/feed.xml",
        "/assets/site.css",
        "/assets/cart.js",
        &product_path_of(&l),
    ] {
        let r = get(&s, p);
        assert_eq!(r.get_header("x-frame-options"), Some("DENY"), "{p}");
        assert_eq!(
            r.get_header("x-content-type-options"),
            Some("nosniff"),
            "{p}"
        );
        let csp = r.get_header("content-security-policy").unwrap();
        assert!(
            csp.contains("default-src 'none'")
                && csp.contains("frame-ancestors 'none'")
                && csp.contains("base-uri 'none'")
                && csp.contains("form-action 'self'"),
            "{p}: {csp}"
        );
        assert!(
            !csp.contains("unsafe-inline") && !csp.contains("unsafe-eval"),
            "{p}"
        );
        assert!(
            r.get_header("referrer-policy").is_some()
                && r.get_header("permissions-policy").is_some(),
            "{p}"
        );
    }
    // No inline script or style attribute needs the policy to be loosened.
    for p in [
        "/",
        "/shop",
        "/cart?add=00000000-0000-7000-8000-000000000000",
        "/contact",
    ] {
        let t = text(&get(&s, p));
        assert!(!t.contains(" style=\""), "{p}");
        assert!(!t.contains("onclick=") && !t.contains("javascript:"), "{p}");
        assert!(!t.contains("<script>"), "{p}: scripts are external files");
    }
    // HTTPS requests get HSTS.
    let r = get_h(&s, "/", &[("x-forwarded-proto", "https")]);
    assert!(r.get_header("strict-transport-security").is_some());
}

#[test]
fn security_caches_are_told_what_the_response_depends_on() {
    let s = shop();
    for p in ["/", "/about", "/shop", "/blog"] {
        let r = get(&s, p);
        let vary = r.get_header("vary").unwrap().to_lowercase();
        for h in ["accept", "accept-language", "cookie"] {
            assert!(vary.contains(h), "{p}: {vary}");
        }
        assert!(
            r.get_header("cache-control").unwrap().starts_with("public"),
            "{p}"
        );
    }
    // A response that sets a cookie is never stored by a shared cache.
    let r = get(&s, "/about?lang=de");
    assert!(r.get_header("set-cookie").is_some());
    assert!(r.get_header("cache-control").unwrap().contains("no-store"));
    // The Host header cannot choose canonical URLs once the owner has set the base URL.
    set_site(&s, serde_json::json!({"base_url": "https://real.example"}));
    let r = get_h(&s, "/about", &[("host", "evil.example")]);
    let t = text(&r);
    assert!(t.contains("href=\"https://real.example/about\"") && !t.contains("evil.example"));
    // Without it, a hostile Host value is not echoed into the page.
    set_site(&s, serde_json::json!({}));
    let r = get_h(&s, "/about", &[("host", "a\"><script>alert(1)</script>")]);
    assert!(!text(&r).contains("<script>alert"));
}

#[test]
fn security_unpublished_and_nonexistent_products_are_indistinguishable() {
    let s = shop();
    let l = listing(&s, "linen-tea-towel");
    set_state(
        &s,
        "ecommerce_listing",
        &l.id,
        "published",
        Value::Bool(false),
    );
    let hidden = get(&s, &format!("/shop/{}-{}", l.id, l.text("slug")));
    let missing = get(
        &s,
        "/shop/00000000-0000-7000-8000-000000000000-linen-tea-towel",
    );
    assert_eq!((hidden.status, missing.status), (404, 404));
    assert_eq!(hidden.body, missing.body);
    assert_eq!(hidden.headers.len(), missing.headers.len());
}

#[test]
fn security_private_fields_of_published_records_are_never_rendered() {
    let s = shop();
    let l = listing(&s, "stoneware-teapot");
    let (_, raw) = s
        .store
        .backend()
        .list("ecommerce_listing")
        .unwrap()
        .into_iter()
        .find(|(id, _)| *id == l.id)
        .unwrap();
    let product_id = raw["product_id"].as_str().unwrap().to_string();
    let author_page = s.store.backend().list("website_blog_post").unwrap();
    for path in [
        product_path_of(&l),
        "/shop".into(),
        "/blog".into(),
        "/llms.txt".into(),
        "/sitemap.xml".into(),
        "/feed.xml".into(),
    ] {
        let t = text(&get(&s, &path));
        assert!(
            !t.contains(&product_id),
            "{path} shows the internal product id"
        );
        assert!(
            !t.contains("Editorial team"),
            "{path} shows the private author name"
        );
    }
    assert!(!author_page.is_empty());
    // Stock is shown as a state, never as a number.
    assert!(!text(&get(&s, &product_path_of(&l))).contains("stock_quantity"));
}

#[test]
fn security_the_site_makes_no_outbound_requests_and_logs_nothing() {
    let manifest = include_str!("../Cargo.toml");
    for dep in [
        "ureq", "reqwest", "hyper", "curl", "tokio", "tracing", "log =",
    ] {
        assert!(!manifest.contains(dep), "{dep}");
    }
    for (name, src) in [
        ("lib", include_str!("lib.rs")),
        ("content", include_str!("content.rs")),
        ("shop", include_str!("shop.rs")),
        ("seo", include_str!("seo.rs")),
        ("layout", include_str!("layout.rs")),
    ] {
        for call in [
            "println!",
            "eprintln!",
            "dbg!",
            "print!(",
            "std::fs",
            "std::net",
            "Command::new",
        ] {
            assert!(!src.contains(call), "{name} uses {call}");
        }
    }
}

#[test]
fn a_listing_with_a_mismatched_currency_cannot_be_ordered_with_others() {
    let s = shop();
    let a = listing(&s, "sencha-green-tea");
    let b = listing(&s, "stoneware-teapot");
    set_state(
        &s,
        "ecommerce_listing",
        &b.id,
        "currency",
        Value::text("USD"),
    );
    let std = methods(&s).remove(0);
    let lines = [(a.id.as_str(), "1"), (b.id.as_str(), "1")];
    let r = post(
        &s,
        "/shop/checkout",
        &order_fields(&lines, &[("shipping_method_id", &std.id)]),
    );
    assert_eq!(r.status, 400);
    assert!(text(&r).contains("same currency"));
    assert_eq!(new_orders(&s), 0);
}

#[test]
fn uninstalling_the_shop_removes_its_pages_but_not_the_website() {
    let s = shop();
    s.d.unregister_module("ecommerce");
    assert_eq!(get(&s, "/shop").status, 404);
    assert_eq!(get(&s, "/cart").status, 404);
    let h = text(&get(&s, "/about"));
    assert!(!h.contains("cartlink") && !h.contains("/assets/cart.js"));
    assert_eq!(get(&s, "/about").status, 200);
}

// ---- storage backends --------------------------------------------------------------------

fn full_flow(s: &Shop) {
    assert_eq!(get(s, "/").status, 200);
    let l = listing(s, "sencha-green-tea");
    assert_eq!(get(s, &product_path_of(&l)).status, 200);
    let std = methods(s).remove(0);
    let lines = [(l.id.as_str(), "2")];
    // Idempotency keys now live in the database for a day: a fresh key per run, or a rerun on a
    // persistent database would be told its order was already placed.
    let idem = format!(
        "{:032x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.as_nanos())
    );
    let mut fields = order_fields(&lines, &[("shipping_method_id", &std.id)]);
    fields.retain(|(k, _)| *k != "idem");
    fields.push(("idem", &idem));
    let r = post(s, "/shop/checkout", &fields);
    assert_eq!(r.status, 303);
    let page = get(s, r.get_header("location").unwrap());
    assert!(text(&page).contains("buyer@example.test"));
    let c = post(
        s,
        "/contact",
        &[
            ("name", "Ada"),
            ("email", "ada@example.test"),
            ("message", "Hello"),
            ("consent", "1"),
        ],
    );
    assert_eq!(c.status, 303);
}

#[test]
fn the_whole_flow_works_on_memory() {
    full_flow(&shop());
}

#[test]
fn the_whole_flow_works_on_sqlite_and_persists() {
    let path = std::env::temp_dir().join(format!("obop-site-test-{}.db", std::process::id()));
    let url = format!("sqlite://{}", path.display());
    {
        let s = build(Store::open(&url).unwrap(), true);
        full_flow(&s);
    }
    // A second process sees the same content.
    let again = build(Store::open(&url).unwrap(), false);
    assert!(text(&get(&again, "/about")).contains("About Fernhill Goods"));
    assert_eq!(again.store.count("ecommerce_order"), 1);
    for ext in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
    }
}

#[test]
fn the_whole_flow_works_on_postgres_when_available() {
    let Ok(url) = std::env::var("OBOP_TEST_POSTGRES_URL") else {
        eprintln!("skipped: set OBOP_TEST_POSTGRES_URL to run against PostgreSQL");
        return;
    };
    let store = Store::open(&url).unwrap();
    for model in [
        "website_page",
        "website_page_translation",
        "website_blog_post",
        "website_blog_post_translation",
        "website_menu_item",
        "website_form_submission",
        "ecommerce_category",
        "ecommerce_listing",
        "ecommerce_listing_translation",
        "ecommerce_shipping_method",
        "ecommerce_order",
        "ecommerce_order_line",
        "product",
    ] {
        for (id, _) in store.backend().list(model).unwrap() {
            store.backend().delete(model, &id).unwrap();
        }
    }
    let s = build(store, true);
    full_flow(&s);
}

#[test]
fn the_seed_refuses_to_run_twice_and_needs_the_packages() {
    let s = shop();
    assert!(
        seed::seed(&s.store, &s.d)
            .unwrap_err()
            .contains("already has")
    );
    let bare = Dispatcher::new();
    assert!(
        seed::seed(&Store::new(), &bare)
            .unwrap_err()
            .contains("install the website")
    );
}

#[test]
fn the_seed_never_overwrites_existing_site_settings() {
    let store = Store::new();
    let d = Arc::new(Dispatcher::new());
    base::register(&d, &store);
    product::register(&d, &store);
    let inst = Installer::new(
        store.clone(),
        &d,
        vec![base::module(), product::module()],
        Arc::new(Bundled),
    );
    for id in ["website", "ecommerce"] {
        inst.install(&Bundled.fetch(id).unwrap(), "marketplace", "test")
            .unwrap();
    }
    inst.set_settings(r##"{"theme":{"accent":"#123456"},"site":{"title":"Mine"}}"##)
        .unwrap();
    seed::seed(&store, &d).unwrap();
    let v: serde_json::Value = serde_json::from_str(&inst.settings()).unwrap();
    assert_eq!(v["site"]["title"], "Mine");
    assert_eq!(v["site"]["tagline"], "Small-batch tea, ceramics and gifts");
    assert_eq!(v["theme"]["accent"], "#123456");
}
