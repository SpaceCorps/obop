//! The request context, the `Page` model and the HTML/Markdown shell around every page:
//! landmarks, skip link, language switcher, SEO and agent-readiness metadata, security and cache
//! headers. All text that did not come from this crate is escaped on the way in.

use crate::http::{Req, Resp};
use crate::i18n::{self, Lang, Source, t, tf};
use crate::settings::Settings;
use crate::util::{esc, sha256_hex};
use serde_json::{Value as J, json};
use std::sync::Arc;

/// What the instance can show, decided by which packages are installed and public.
#[derive(Debug, Clone, Copy, Default)]
pub struct Caps {
    pub shop: bool,
    pub blog: bool,
    pub pages: bool,
    pub contact: bool,
}

#[derive(Debug, Clone)]
pub struct MenuItem {
    pub label: String,
    pub url: String,
    pub new_window: bool,
}

pub struct Ctx<'a> {
    pub req: &'a Req,
    pub lang: Lang,
    pub lang_source: Source,
    pub md: bool,
    pub base: String,
    pub s: Arc<Settings>,
    pub caps: Caps,
    pub menu: Vec<MenuItem>,
    pub admin_prefix: Option<String>,
    /// CSRF token for forms rendered in this response (the value of the `obop_csrf` cookie).
    pub csrf: String,
    pub set_cookies: Vec<String>,
    pub css_version: String,
    pub js_version: String,
}

impl Ctx<'_> {
    pub fn t(&self, key: &str) -> &'static str {
        t(self.lang, key)
    }

    pub fn tf(&self, key: &str, args: &[(&str, &str)]) -> String {
        tf(self.lang, key, args)
    }

    pub fn abs(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    /// `path` carrying `lang=` when the visitor's language is not the site's.
    pub fn lpath(&self, path: &str) -> String {
        if self.lang == self.s.default_lang {
            path.to_string()
        } else {
            with_lang(path, self.lang)
        }
    }

    pub fn money(&self, minor: i64, currency: &str) -> String {
        i18n::fmt_money(self.lang, minor, currency)
    }

    pub fn date(&self, iso: &str) -> String {
        i18n::fmt_date(self.lang, iso)
    }
}

/// Add `lang=` to a path that may already have a query.
pub fn with_lang(path: &str, lang: Lang) -> String {
    let sep = if path.contains('?') { '&' } else { '?' };
    format!("{path}{sep}lang={}", lang.code())
}

/// Which language versions of a page exist.
#[derive(Debug, Clone, Default)]
pub enum Variants {
    /// No alternates (private or noindex pages).
    #[default]
    None,
    /// The chrome speaks every language and the page text follows it (lists, home).
    Chrome,
    /// A record with these translations (the site's default language is always included).
    Only(Vec<Lang>),
}

/// The picture of a page for link previews: a path below the site, its size and alt text.
#[derive(Debug, Clone)]
pub struct PageImage {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub alt: String,
}

pub struct Page {
    pub status: u16,
    pub title: String,
    pub description: String,
    /// Canonical path with its query where it matters (`?page=2`), without `lang`.
    pub path: String,
    pub noindex: bool,
    pub crumbs: Vec<(String, String)>,
    pub jsonld: Vec<J>,
    pub body: String,
    pub md: String,
    pub prev: Option<String>,
    pub next: Option<String>,
    /// Not for shared caches: forms with tokens, order pages.
    pub private: bool,
    /// Unix seconds of the last change, when known.
    pub last_modified: Option<u64>,
    pub og_type: &'static str,
    pub variants: Variants,
    /// Language of the record text when it differs from the chrome language.
    pub content_lang: Option<Lang>,
    /// Attributes for `<body>`: pre-escaped by this crate (cart page flags).
    pub body_attrs: String,
    pub extra_headers: Vec<(String, String)>,
    /// Open Graph and Twitter image, when the page has a public picture.
    pub image: Option<PageImage>,
}

impl Page {
    pub fn new(title: &str, description: &str, path: &str) -> Page {
        Page {
            status: 200,
            title: title.into(),
            description: description.into(),
            path: path.into(),
            noindex: false,
            crumbs: Vec::new(),
            jsonld: Vec::new(),
            body: String::new(),
            md: String::new(),
            prev: None,
            next: None,
            private: false,
            last_modified: None,
            og_type: "website",
            variants: Variants::None,
            content_lang: None,
            body_attrs: String::new(),
            extra_headers: Vec::new(),
            image: None,
        }
    }
}

/// JSON for a `<script type="application/ld+json">`: `<`, `>` and `&` are escaped so a value can
/// never close the tag.
pub fn json_script(v: &J) -> String {
    let s = serde_json::to_string(v).unwrap_or_else(|_| "{}".into());
    format!(
        "<script type=\"application/ld+json\">{}</script>",
        s.replace('<', "\\u003c")
            .replace('>', "\\u003e")
            .replace('&', "\\u0026")
    )
}

fn breadcrumb_ld(ctx: &Ctx, crumbs: &[(String, String)]) -> J {
    json!({
        "@context": "https://schema.org",
        "@type": "BreadcrumbList",
        "itemListElement": crumbs.iter().enumerate().map(|(i, (name, path))| json!({
            "@type": "ListItem", "position": i + 1, "name": name, "item": ctx.abs(path),
        })).collect::<Vec<_>>(),
    })
}

/// The path this page is canonically served at: in the language its text is written in.
fn canonical_path(ctx: &Ctx, page: &Page) -> String {
    let lang = match &page.variants {
        Variants::Only(_) => page.content_lang.unwrap_or(ctx.s.default_lang),
        Variants::Chrome => ctx.lang,
        Variants::None => return page.path.clone(),
    };
    if lang == ctx.s.default_lang {
        page.path.clone()
    } else {
        with_lang(&page.path, lang)
    }
}

pub fn site_title(ctx: &Ctx, page: &Page) -> String {
    if page.path == "/" && !ctx.s.tagline.is_empty() {
        format!("{} | {}", ctx.s.title, ctx.s.tagline)
    } else if page.path == "/" {
        ctx.s.title.clone()
    } else {
        format!("{} | {}", page.title, ctx.s.title)
    }
}

fn head(ctx: &Ctx, page: &Page) -> String {
    let canonical_path = canonical_path(ctx, page);
    let canonical = ctx.abs(&canonical_path);
    let title = site_title(ctx, page);
    let noindex = page.noindex || ctx.s.noindex || page.status >= 400;
    let mut h = String::new();
    h.push_str("<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    h.push_str(&format!("<title>{}</title>\n", esc(&title)));
    h.push_str(&format!(
        "<meta name=\"description\" content=\"{}\">\n",
        esc(&page.description)
    ));
    h.push_str(if noindex {
        "<meta name=\"robots\" content=\"noindex, follow\">\n"
    } else {
        "<meta name=\"robots\" content=\"index, follow, max-snippet:-1, max-image-preview:large\">\n"
    });
    h.push_str("<meta name=\"color-scheme\" content=\"light dark\">\n");
    h.push_str(&format!(
        "<link rel=\"canonical\" href=\"{}\">\n",
        esc(&canonical)
    ));
    if !noindex {
        let langs: Vec<Lang> = match &page.variants {
            Variants::None => Vec::new(),
            Variants::Chrome => Lang::ALL.to_vec(),
            Variants::Only(l) => l.clone(),
        };
        let alt = |l: Lang| {
            if l == ctx.s.default_lang {
                page.path.clone()
            } else {
                with_lang(&page.path, l)
            }
        };
        if langs.len() > 1 {
            for l in &langs {
                h.push_str(&format!(
                    "<link rel=\"alternate\" hreflang=\"{}\" href=\"{}\">\n",
                    l.code(),
                    esc(&ctx.abs(&alt(*l)))
                ));
            }
            h.push_str(&format!(
                "<link rel=\"alternate\" hreflang=\"x-default\" href=\"{}\">\n",
                esc(&ctx.abs(&page.path))
            ));
        }
    }
    let sep = if canonical_path.contains('?') {
        '&'
    } else {
        '?'
    };
    h.push_str(&format!(
        "<link rel=\"alternate\" type=\"text/markdown\" href=\"{}\">\n",
        esc(&ctx.abs(&format!("{canonical_path}{sep}format=md")))
    ));
    if ctx.caps.blog {
        h.push_str(&format!(
            "<link rel=\"alternate\" type=\"application/atom+xml\" title=\"{}\" href=\"{}\">\n",
            esc(&format!("{} - {}", ctx.s.title, ctx.t("blog.title"))),
            esc(&ctx.abs("/feed.xml"))
        ));
    }
    if let Some(p) = &page.prev {
        h.push_str(&format!(
            "<link rel=\"prev\" href=\"{}\">\n",
            esc(&ctx.abs(&ctx.lpath(p)))
        ));
    }
    if let Some(n) = &page.next {
        h.push_str(&format!(
            "<link rel=\"next\" href=\"{}\">\n",
            esc(&ctx.abs(&ctx.lpath(n)))
        ));
    }
    let content_lang = page.content_lang.unwrap_or(ctx.lang);
    for (p, c) in [
        ("og:site_name", ctx.s.title.clone()),
        ("og:type", page.og_type.to_string()),
        ("og:title", page.title.clone()),
        ("og:description", page.description.clone()),
        ("og:url", canonical.clone()),
        ("og:locale", content_lang.og_locale().to_string()),
    ] {
        h.push_str(&format!(
            "<meta property=\"{p}\" content=\"{}\">\n",
            esc(&c)
        ));
    }
    if let Some(i) = &page.image {
        let url = ctx.abs(&i.path);
        for (p, c) in [
            ("og:image", url.clone()),
            ("og:image:width", i.width.to_string()),
            ("og:image:height", i.height.to_string()),
            ("og:image:alt", i.alt.clone()),
        ] {
            h.push_str(&format!(
                "<meta property=\"{p}\" content=\"{}\">\n",
                esc(&c)
            ));
        }
    }
    for (n, c) in [
        (
            "twitter:card",
            if page.image.is_some() {
                "summary_large_image"
            } else {
                "summary"
            }
            .to_string(),
        ),
        ("twitter:title", page.title.clone()),
        ("twitter:description", page.description.clone()),
    ] {
        h.push_str(&format!("<meta name=\"{n}\" content=\"{}\">\n", esc(&c)));
    }
    if let Some(i) = &page.image {
        h.push_str(&format!(
            "<meta name=\"twitter:image\" content=\"{}\">\n<meta name=\"twitter:image:alt\" content=\"{}\">\n",
            esc(&ctx.abs(&i.path)),
            esc(&i.alt)
        ));
    }
    h.push_str("<link rel=\"icon\" href=\"/favicon.svg\" type=\"image/svg+xml\">\n");
    h.push_str(&format!(
        "<link rel=\"stylesheet\" href=\"/assets/site.css?v={}\">\n",
        ctx.css_version
    ));
    if !page.crumbs.is_empty() && page.path != "/" {
        h.push_str(&json_script(&breadcrumb_ld(ctx, &page.crumbs)));
        h.push('\n');
    }
    for ld in &page.jsonld {
        h.push_str(&json_script(ld));
        h.push('\n');
    }
    h
}

fn nav(ctx: &Ctx, page: &Page) -> String {
    let mut items: Vec<(String, String, bool)> = Vec::new();
    if ctx.menu.is_empty() {
        if ctx.caps.shop {
            items.push((ctx.t("nav.shop").into(), "/shop".into(), false));
        }
        if ctx.caps.blog {
            items.push((ctx.t("nav.blog").into(), "/blog".into(), false));
        }
        if ctx.caps.contact {
            items.push((ctx.t("nav.contact").into(), "/contact".into(), false));
        }
    } else {
        for m in &ctx.menu {
            items.push((m.label.clone(), m.url.clone(), m.new_window));
        }
    }
    let mut h = format!(
        "<nav class=\"main\" aria-label=\"{}\"><ul>",
        esc(ctx.t("nav.main"))
    );
    for (label, url, new_window) in items {
        let local = url.starts_with('/');
        let here = local && {
            let base = url.trim_end_matches('/');
            let cur = page.path.split('?').next().unwrap_or("");
            cur == base || cur.starts_with(&format!("{base}/"))
        };
        let href = if local { ctx.lpath(&url) } else { url.clone() };
        h.push_str(&format!(
            "<li><a href=\"{}\"{}{}>{}</a></li>",
            esc(&href),
            if here { " aria-current=\"page\"" } else { "" },
            if new_window {
                " target=\"_blank\" rel=\"noopener\""
            } else {
                ""
            },
            esc(&label)
        ));
    }
    h.push_str("</ul></nav>");
    h
}

fn header(ctx: &Ctx, page: &Page) -> String {
    let mut h = String::new();
    h.push_str(&format!(
        "<a class=\"skip\" href=\"#main\">{}</a>\n",
        esc(ctx.t("skip"))
    ));
    h.push_str("<header class=\"site\"><div class=\"wrap bar\">\n");
    h.push_str(&format!(
        "<a class=\"brand\" href=\"{}\">",
        esc(&ctx.lpath("/"))
    ));
    if let Some(u) = &ctx.s.logo_url {
        h.push_str(&format!("<img src=\"{}\" alt=\"\" height=\"34\">", esc(u)));
    }
    h.push_str(&format!("<span>{}</span></a>\n", esc(&ctx.s.logo_text)));
    h.push_str(&nav(ctx, page));
    if ctx.caps.shop {
        h.push_str(&format!(
            "<a class=\"cartlink\" href=\"{}\"><span>{}</span><span class=\"n\" data-cart-count aria-hidden=\"true\"></span></a>",
            esc(&ctx.lpath("/cart")),
            esc(ctx.t("nav.cart"))
        ));
    }
    h.push_str("\n</div></header>\n");
    h
}

fn footer(ctx: &Ctx, page: &Page) -> String {
    let mut h = String::from("<footer class=\"site\"><div class=\"wrap cols\">\n<div>");
    h.push_str(&format!("<p><strong>{}</strong></p>", esc(&ctx.s.title)));
    if !ctx.s.tagline.is_empty() {
        h.push_str(&format!("<p>{}</p>", esc(&ctx.s.tagline)));
    }
    if !ctx.s.footer_text.is_empty() {
        h.push_str(&format!("<p>{}</p>", esc(&ctx.s.footer_text)));
    }
    if ctx.caps.shop {
        h.push_str(&format!("<p>{}</p>", esc(ctx.t("footer.prices"))));
    }
    let canonical = canonical_path(ctx, page);
    let sep = if canonical.contains('?') { '&' } else { '?' };
    h.push_str(&format!(
        "<p><a href=\"{}\" type=\"text/markdown\">{}</a>",
        esc(&format!("{canonical}{sep}format=md")),
        esc(ctx.t("footer.md"))
    ));
    if ctx.caps.blog {
        h.push_str(&format!(
            " · <a href=\"/feed.xml\">{}</a>",
            esc(ctx.t("footer.feed"))
        ));
    }
    h.push_str("</p></div>\n");
    if !ctx.s.social.is_empty() {
        h.push_str(&format!("<div><h2>{}</h2><ul>", esc(ctx.t("nav.social"))));
        for (label, url) in &ctx.s.social {
            h.push_str(&format!(
                "<li><a href=\"{}\" rel=\"me noopener\">{}</a></li>",
                esc(url),
                esc(label)
            ));
        }
        h.push_str("</ul></div>\n");
    }
    h.push_str(&format!(
        "<nav aria-label=\"{0}\"><h2>{0}</h2><ul>",
        esc(ctx.t("nav.language"))
    ));
    for l in Lang::ALL {
        // Always carry `lang`: choosing the site's own language must also set the cookie.
        let href = with_lang(&page.path, l);
        h.push_str(&format!(
            "<li><a href=\"{}\" lang=\"{}\" hreflang=\"{}\"{}>{}</a></li>",
            esc(&href),
            l.code(),
            l.code(),
            if l == ctx.lang {
                " aria-current=\"true\""
            } else {
                ""
            },
            esc(l.name())
        ));
    }
    h.push_str("</ul></nav>\n</div></footer>\n");
    h
}

fn html_doc(ctx: &Ctx, page: &Page) -> String {
    let mut o = String::with_capacity(page.body.len() + 6000);
    o.push_str(&format!(
        "<!doctype html>\n<html lang=\"{}\">\n<head>\n",
        ctx.lang.code()
    ));
    o.push_str(&head(ctx, page));
    o.push_str(&format!("</head>\n<body{}>\n", page.body_attrs));
    o.push_str(&header(ctx, page));
    o.push_str("<main id=\"main\" class=\"wrap\" tabindex=\"-1\">\n");
    if page.crumbs.len() > 1 {
        o.push_str(&format!(
            "<nav class=\"crumbs\" aria-label=\"{}\"><ol>",
            esc(ctx.t("nav.breadcrumb"))
        ));
        let last = page.crumbs.len() - 1;
        for (i, (name, path)) in page.crumbs.iter().enumerate() {
            if i == last {
                o.push_str(&format!("<li aria-current=\"page\">{}</li>", esc(name)));
            } else {
                o.push_str(&format!(
                    "<li><a href=\"{}\">{}</a></li>",
                    esc(&ctx.lpath(path)),
                    esc(name)
                ));
            }
        }
        o.push_str("</ol></nav>\n");
    }
    match page.content_lang {
        Some(l) if l != ctx.lang => {
            o.push_str(&format!("<div lang=\"{}\">{}</div>", l.code(), page.body))
        }
        _ => o.push_str(&page.body),
    }
    o.push_str("\n</main>\n");
    o.push_str(&footer(ctx, page));
    if ctx.caps.shop {
        o.push_str(&format!(
            "<script src=\"/assets/cart.js?v={}\" defer></script>\n",
            ctx.js_version
        ));
    }
    o.push_str("</body>\n</html>\n");
    o
}

fn md_doc(ctx: &Ctx, page: &Page) -> String {
    let canonical = ctx.abs(&canonical_path(ctx, page));
    // Nothing in a Markdown rendering may start an HTML tag in the reader.
    let line = |s: &str| s.replace(['<', '>', '\n', '\r'], " ");
    format!(
        "# {}\n\n> {}\n\n{}\n\n---\nSource: {}\n",
        line(&page.title),
        line(&page.description),
        page.md.trim_end().replace('<', "&lt;"),
        canonical
    )
}

/// Content security policy of every public page. Same-origin styles and scripts only; the one
/// extra origin is the https host of an owner-chosen logo image.
pub fn csp(s: &Settings) -> String {
    let logo = s
        .logo_url
        .as_deref()
        .filter(|u| u.starts_with("https://"))
        .map(|u| {
            let rest = &u["https://".len()..];
            format!(" https://{}", rest.split('/').next().unwrap_or(""))
        })
        .unwrap_or_default();
    format!(
        "default-src 'none'; style-src 'self'; script-src 'self'; img-src 'self' data:{logo}; form-action 'self'; base-uri 'none'; frame-ancestors 'none'; connect-src 'self'"
    )
}

pub fn security_headers(r: Resp, ctx: &Ctx) -> Resp {
    let mut r = r
        .header("x-content-type-options", "nosniff")
        .header("referrer-policy", "strict-origin-when-cross-origin")
        .header("content-security-policy", &csp(&ctx.s))
        .header("x-frame-options", "DENY")
        .header("cross-origin-opener-policy", "same-origin")
        .header(
            "permissions-policy",
            "camera=(), microphone=(), geolocation=(), payment=()",
        );
    if ctx.req.is_https() {
        r = r.header("strict-transport-security", "max-age=31536000");
    }
    r
}

/// Turn a `Page` into the final response: HTML or Markdown, cache and validator headers, cookies.
pub fn render(ctx: &Ctx, page: Page) -> Resp {
    let (ctype, body) = if ctx.md {
        ("text/markdown; charset=utf-8", md_doc(ctx, &page))
    } else {
        ("text/html; charset=utf-8", html_doc(ctx, &page))
    };
    let mut r = Resp::new(page.status, ctype, body);
    let personalised = page.private || !ctx.set_cookies.is_empty();
    let get = matches!(ctx.req.method.as_str(), "GET" | "HEAD");
    if page.status >= 400 || page.status == 503 {
        r = r.header("cache-control", "no-cache");
    } else if personalised || !get {
        r = r.header("cache-control", "private, no-store");
    } else {
        r = r.header(
            "cache-control",
            "public, max-age=60, stale-while-revalidate=300",
        );
    }
    // The text depends on the negotiated format, the language and the language cookie.
    r = r.header("vary", "Accept, Accept-Language, Cookie");
    r = r.header(
        "content-language",
        page.content_lang.unwrap_or(ctx.lang).code(),
    );
    if page.noindex || ctx.s.noindex || page.status >= 400 {
        r = r.header("x-robots-tag", "noindex");
    }
    r = r.header(
        "link",
        &format!(
            "<{}>; rel=\"canonical\"",
            ctx.abs(&canonical_path(ctx, &page))
        ),
    );
    for (k, v) in &page.extra_headers {
        r = r.header(k, v);
    }
    for c in &ctx.set_cookies {
        r = r.add_header("set-cookie", c);
    }
    let r = if personalised || !get || page.status != 200 {
        r
    } else {
        r.conditional(ctx.req, page.last_modified)
    };
    security_headers(r, ctx)
}

pub fn css_version(css: &str) -> String {
    sha256_hex(css)[..10].to_string()
}
