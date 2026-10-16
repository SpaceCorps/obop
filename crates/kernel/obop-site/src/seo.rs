//! The machine-facing documents of the site: `robots.txt`, `sitemap.xml`, the Atom feed,
//! `llms.txt`, `security.txt` and the favicon, plus the site-wide JSON-LD blocks.

use crate::Site;
use crate::http::Resp;
use crate::i18n::Lang;
use crate::layout::{self, Ctx};
use crate::shop::product_path;
use crate::theme;
use crate::util::{self, date_to_ts, esc, rfc3339};
use obop_commands::PublicRecord;
use serde_json::{Value as J, json};
use std::collections::BTreeMap;

fn doc(ctx: &Ctx, ctype: &str, body: String, max_age: u32) -> Resp {
    let r = Resp::new(200, ctype, body)
        .header(
            "cache-control",
            &format!("public, max-age={max_age}, stale-while-revalidate=3600"),
        )
        .conditional(ctx.req, None);
    layout::security_headers(r, ctx)
}

pub fn robots(ctx: &Ctx) -> Resp {
    let mut t = String::from("User-agent: *\n");
    if ctx.s.noindex || !ctx.s.enabled {
        t.push_str("Disallow: /\n");
    } else {
        t.push_str("Allow: /\n");
        if let Some(a) = &ctx.admin_prefix {
            t.push_str(&format!("Disallow: {a}/\n"));
        }
        for p in ["/api/", "/cart", "/shop/checkout", "/shop/order/"] {
            t.push_str(&format!("Disallow: {p}\n"));
        }
        t.push_str(&format!("\nSitemap: {}\n", ctx.abs("/sitemap.xml")));
    }
    t.push_str(&format!(
        "\n# Plain-text summary for AI agents: {}\n",
        ctx.abs("/llms.txt")
    ));
    doc(ctx, "text/plain; charset=utf-8", t, 600)
}

pub fn favicon(ctx: &Ctx) -> Resp {
    let a = theme::resolve(&ctx.s.accent);
    let initial: String = ctx
        .s
        .logo_text
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "S".into());
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\"><rect width=\"64\" height=\"64\" rx=\"14\" fill=\"{}\"/><text x=\"32\" y=\"44\" font-family=\"Georgia,serif\" font-size=\"36\" font-weight=\"700\" text-anchor=\"middle\" fill=\"{}\">{}</text></svg>",
        a.light.fill,
        a.light.on_fill,
        esc(&initial)
    );
    doc(ctx, "image/svg+xml", svg, 86_400)
}

pub fn security_txt(ctx: &Ctx) -> Resp {
    let Some(contact) = &ctx.s.security_contact else {
        return layout::render(ctx, {
            let mut p = layout::Page::new(ctx.t("e404.title"), ctx.t("e404.body"), "/");
            p.status = 404;
            p.noindex = true;
            p.body = format!("<h1>{}</h1>", esc(ctx.t("e404.title")));
            p.md = format!("{}\n", ctx.t("e404.body"));
            p
        });
    };
    let expires = util::now() + 365 * 86_400;
    let t = format!(
        "Contact: {contact}\nExpires: {}\nPreferred-Languages: {}\nCanonical: {}\n",
        rfc3339(expires),
        ctx.s.default_lang.code(),
        ctx.abs("/.well-known/security.txt")
    );
    doc(ctx, "text/plain; charset=utf-8", t, 3600)
}

// ---- JSON-LD shared by pages -------------------------------------------------------------

pub fn organization_ld(ctx: &Ctx) -> J {
    let mut o = json!({
        "@context": "https://schema.org",
        "@type": "Organization",
        "name": ctx.s.title,
        "url": ctx.abs("/"),
    });
    if let Some(u) = &ctx.s.logo_url {
        o["logo"] = json!(if u.starts_with('/') {
            ctx.abs(u)
        } else {
            u.clone()
        });
    }
    if !ctx.s.social.is_empty() {
        o["sameAs"] = json!(
            ctx.s
                .social
                .iter()
                .map(|(_, u)| u.clone())
                .collect::<Vec<_>>()
        );
    }
    o
}

pub fn website_ld(ctx: &Ctx) -> J {
    let mut o = json!({
        "@context": "https://schema.org",
        "@type": "WebSite",
        "name": ctx.s.title,
        "url": ctx.abs("/"),
        "inLanguage": ctx.s.default_lang.code(),
    });
    if !ctx.s.tagline.is_empty() {
        o["description"] = json!(ctx.s.tagline);
    }
    if ctx.caps.shop {
        o["potentialAction"] = json!({
            "@type": "SearchAction",
            "target": {"@type": "EntryPoint", "urlTemplate": format!("{}/shop?q={{search_term_string}}", ctx.base)},
            "query-input": "required name=search_term_string",
        });
    }
    o
}

pub fn item_list_ld(ctx: &Ctx, name: &str, items: &[(String, String)]) -> J {
    json!({
        "@context": "https://schema.org",
        "@type": "ItemList",
        "name": name,
        "numberOfItems": items.len(),
        "itemListElement": items.iter().enumerate().map(|(i, (title, path))| json!({
            "@type": "ListItem", "position": i + 1, "name": title, "url": ctx.abs(path),
        })).collect::<Vec<_>>(),
    })
}

// ---- sitemap -----------------------------------------------------------------------------

struct Url {
    loc: String,
    lastmod: Option<String>,
    alternates: Vec<(Lang, String)>,
}

fn translated_langs(site: &Site, model: &str, ref_field: &str) -> BTreeMap<String, Vec<Lang>> {
    let mut by: BTreeMap<String, Vec<Lang>> = BTreeMap::new();
    if let Ok((rows, _)) = site.api().list(model, &[], None, 0) {
        for r in rows {
            let id = r
                .fields
                .get(ref_field)
                .and_then(|v| v.get("id"))
                .and_then(J::as_str);
            if let (Some(id), Some(l)) = (id, Lang::parse(r.text("lang"))) {
                by.entry(id.to_string()).or_default().push(l);
            }
        }
    }
    by
}

pub fn sitemap(site: &Site, ctx: &Ctx) -> Resp {
    let mut urls: Vec<Url> = Vec::new();
    let default = ctx.s.default_lang;
    let page_langs = translated_langs(site, "website_page_translation", "page_id");
    let post_langs = translated_langs(site, "website_blog_post_translation", "post_id");
    let listing_langs = translated_langs(site, "ecommerce_listing_translation", "listing_id");
    let push =
        |urls: &mut Vec<Url>, path: String, lastmod: Option<String>, langs: Option<&Vec<Lang>>| {
            let alternates = langs
                .map(|ls| {
                    let mut all = vec![default];
                    for l in ls {
                        if !all.contains(l) {
                            all.push(*l);
                        }
                    }
                    all.into_iter()
                        .map(|l| {
                            (
                                l,
                                if l == default {
                                    ctx.abs(&path)
                                } else {
                                    ctx.abs(&layout::with_lang(&path, l))
                                },
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            urls.push(Url {
                loc: ctx.abs(&path),
                lastmod,
                alternates,
            });
        };
    let day = |r: &PublicRecord| {
        let d = r.text("published_on");
        date_to_ts(d).map(|_| d.to_string())
    };
    let mut newest_page: Option<String> = None;
    let mut home_done = false;
    if ctx.caps.pages
        && let Ok((pages, _)) = site.api().list("website_page", &[], None, 0)
    {
        for p in &pages {
            let slug = p.text("slug");
            if !crate::valid_slug(slug) {
                continue;
            }
            let lm = day(p);
            if newest_page < lm {
                newest_page = lm.clone();
            }
            if slug == "home" {
                push(&mut urls, "/".into(), lm, page_langs.get(&p.id));
                home_done = true;
            } else if !is_reserved(slug) {
                push(&mut urls, format!("/{slug}"), lm, page_langs.get(&p.id));
            }
        }
    }
    if !home_done {
        push(&mut urls, "/".into(), newest_page, None);
    }
    if ctx.caps.blog
        && let Ok((posts, _)) = site.api().list("website_blog_post", &[], None, 0)
    {
        let newest = posts.iter().filter_map(day).max();
        push(&mut urls, "/blog".into(), newest, None);
        for p in &posts {
            if crate::valid_slug(p.text("slug")) {
                push(
                    &mut urls,
                    format!("/blog/{}", p.text("slug")),
                    day(p),
                    post_langs.get(&p.id),
                );
            }
        }
    }
    if ctx.caps.shop {
        push(&mut urls, "/shop".into(), None, None);
        if let Ok((items, _)) = site.api().list("ecommerce_listing", &[], None, 0) {
            for l in &items {
                if crate::valid_slug(l.text("slug")) {
                    push(&mut urls, product_path(l), None, listing_langs.get(&l.id));
                }
            }
        }
    }
    if ctx.caps.contact {
        push(&mut urls, "/contact".into(), None, None);
    }
    let mut x = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">\n",
    );
    for u in urls {
        x.push_str(&format!("<url><loc>{}</loc>", esc(&u.loc)));
        if let Some(l) = &u.lastmod {
            x.push_str(&format!("<lastmod>{}</lastmod>", esc(l)));
        }
        for (l, href) in &u.alternates {
            x.push_str(&format!(
                "<xhtml:link rel=\"alternate\" hreflang=\"{}\" href=\"{}\"/>",
                l.code(),
                esc(href)
            ));
        }
        x.push_str("</url>\n");
    }
    x.push_str("</urlset>\n");
    doc(ctx, "application/xml; charset=utf-8", x, 600)
}

/// Slugs the router uses for its own pages: a page with one of these slugs is not served at
/// `/{slug}` (the contact page is the exception: its text is shown above the form).
pub fn is_reserved(slug: &str) -> bool {
    matches!(
        slug,
        "blog"
            | "shop"
            | "cart"
            | "contact"
            | "sitemap.xml"
            | "feed.xml"
            | "llms.txt"
            | "robots.txt"
            | "admin"
            | "api"
            | "assets"
            | "home"
    )
}

// ---- Atom feed ---------------------------------------------------------------------------

pub fn feed(site: &Site, ctx: &Ctx) -> Resp {
    if !ctx.caps.blog {
        return site.not_found(ctx);
    }
    let posts = site
        .api()
        .list("website_blog_post", &[], Some(30), 0)
        .map(|(r, _)| r)
        .unwrap_or_default();
    let ts = |p: &PublicRecord| date_to_ts(p.text("published_on"));
    let updated = posts.iter().filter_map(ts).max().unwrap_or_else(util::now);
    let mut x = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<feed xmlns=\"http://www.w3.org/2005/Atom\" xml:lang=\"{}\">\n<title>{}</title>\n<subtitle>{}</subtitle>\n<link rel=\"self\" type=\"application/atom+xml\" href=\"{}\"/>\n<link rel=\"alternate\" type=\"text/html\" href=\"{}\"/>\n<id>{}</id>\n<updated>{}</updated>\n",
        ctx.s.default_lang.code(),
        esc(&format!(
            "{} - {}",
            ctx.s.title,
            crate::i18n::t(ctx.s.default_lang, "blog.title")
        )),
        esc(&ctx.s.tagline),
        esc(&ctx.abs("/feed.xml")),
        esc(&ctx.abs("/blog")),
        esc(&ctx.abs("/blog")),
        rfc3339(updated),
    );
    for p in posts {
        let url = ctx.abs(&format!("/blog/{}", p.text("slug")));
        let when = ts(&p).unwrap_or(updated);
        let author = match p.text("byline") {
            "" => ctx.s.title.as_str(),
            b => b,
        };
        let summary = match p.text("summary") {
            "" => crate::md::excerpt(p.text("body"), 280),
            s => s.to_string(),
        };
        x.push_str(&format!(
            "<entry><title>{}</title><link rel=\"alternate\" type=\"text/html\" href=\"{}\"/><id>{}</id><published>{}</published><updated>{}</updated><author><name>{}</name></author><summary>{}</summary></entry>\n",
            esc(p.text("title")),
            esc(&url),
            esc(&url),
            rfc3339(when),
            rfc3339(when),
            esc(author),
            esc(&summary),
        ));
    }
    x.push_str("</feed>\n");
    doc(ctx, "application/atom+xml; charset=utf-8", x, 600)
}

// ---- llms.txt ----------------------------------------------------------------------------

pub fn llms(site: &Site, ctx: &Ctx) -> Resp {
    let md = |path: &str| {
        let sep = if path.contains('?') { '&' } else { '?' };
        ctx.abs(&format!("{path}{sep}format=md"))
    };
    let one_line = |s: &str| util::clean_line(s).replace(['[', ']', '<', '>'], "");
    let mut t = format!("# {}\n\n", one_line(&ctx.s.title));
    if !ctx.s.tagline.is_empty() {
        t.push_str(&format!("> {}\n\n", one_line(&ctx.s.tagline)));
    }
    t.push_str("Every page is also available as Markdown: add ?format=md or send Accept: text/markdown.\n\n");
    if ctx.caps.pages
        && let Ok((pages, _)) = site.api().list("website_page", &[], None, 0)
        && !pages.is_empty()
    {
        t.push_str("## Pages\n\n");
        for p in pages.iter().filter(|p| crate::valid_slug(p.text("slug"))) {
            let path = match p.text("slug") {
                "home" => "/".to_string(),
                s => format!("/{s}"),
            };
            let desc = match p.text("seo_description") {
                "" => crate::md::excerpt(p.text("body"), 140),
                d => d.to_string(),
            };
            t.push_str(&format!(
                "- [{}]({}): {}\n",
                one_line(p.text("title")),
                md(&path),
                one_line(&desc)
            ));
        }
        t.push('\n');
    }
    if ctx.caps.blog
        && let Ok((posts, _)) = site.api().list("website_blog_post", &[], None, 0)
        && !posts.is_empty()
    {
        t.push_str("## Blog\n\n");
        for p in posts.iter().filter(|p| crate::valid_slug(p.text("slug"))) {
            let desc = match p.text("summary") {
                "" => crate::md::excerpt(p.text("body"), 140),
                d => d.to_string(),
            };
            t.push_str(&format!(
                "- [{}]({}): {}\n",
                one_line(p.text("title")),
                md(&format!("/blog/{}", p.text("slug"))),
                one_line(&desc)
            ));
        }
        t.push('\n');
    }
    if ctx.caps.shop
        && let Ok((items, _)) = site.api().list("ecommerce_listing", &[], None, 0)
        && !items.is_empty()
    {
        t.push_str("## Shop\n\n");
        for l in items.iter().filter(|l| crate::valid_slug(l.text("slug"))) {
            let cur = crate::shop::currency_of(l, &ctx.s);
            let price = l.int("price").map_or(String::new(), |p| {
                crate::i18n::fmt_money(ctx.s.default_lang, p, &cur)
            });
            t.push_str(&format!(
                "- [{}]({}): {} {}\n",
                one_line(l.text("title")),
                md(&product_path(l)),
                one_line(&price),
                one_line(l.text("summary"))
            ));
        }
        t.push('\n');
    }
    t.push_str(&format!(
        "## Optional\n\n- [Sitemap]({})\n- [Feed]({})\n",
        ctx.abs("/sitemap.xml"),
        ctx.abs("/feed.xml")
    ));
    doc(ctx, "text/plain; charset=utf-8", t, 600)
}
