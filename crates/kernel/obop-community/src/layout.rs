//! The request context, the `Page` model and the HTML/Markdown shell shared by the forum and the
//! status page: landmarks, skip link, language switcher, SEO and agent-readiness metadata.

use crate::assets;
use crate::http::{Req, Resp};
use crate::i18n::{self, Lang, Source, t, tf};
use crate::util::{esc, sha256_hex, urlenc};
use serde_json::{Value as J, json};

pub const SITE: &str = "obop";

#[derive(Debug, Clone)]
pub struct User {
    pub email: String,
    pub name: String,
    pub staff: bool,
}

pub struct Ctx<'a> {
    pub req: &'a Req,
    pub lang: Lang,
    pub lang_source: Source,
    pub md: bool,
    pub base: String,
    pub user: Option<User>,
    pub now: u64,
    /// Session token as sent by the browser (needed to derive the CSRF token).
    pub session_raw: Option<String>,
    /// CSRF token to embed in forms rendered for this request.
    pub csrf: String,
    pub set_cookies: Vec<String>,
}

impl Ctx<'_> {
    pub fn t(&self, key: &str) -> &'static str {
        t(self.lang, key)
    }

    pub fn tf(&self, key: &str, args: &[(&str, &str)]) -> String {
        tf(self.lang, key, args)
    }

    pub fn staff(&self) -> bool {
        self.user.as_ref().is_some_and(|u| u.staff)
    }

    pub fn abs(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    /// `path` carrying the language when it is not English.
    pub fn lpath(&self, path: &str) -> String {
        with_lang(path, self.lang)
    }

    pub fn date(&self, ts: u64) -> String {
        i18n::fmt_date(self.lang, ts)
    }

    pub fn datetime(&self, ts: u64) -> String {
        i18n::fmt_datetime(self.lang, ts)
    }

    pub fn num(&self, n: u64) -> String {
        i18n::fmt_num(self.lang, n)
    }

    /// `<time>` element for a timestamp.
    pub fn time_html(&self, ts: u64) -> String {
        format!(
            "<time datetime=\"{}\">{}</time>",
            crate::util::rfc3339(ts),
            esc(&self.datetime(ts))
        )
    }
}

/// Add `lang=` (when not English) to a path that may already have a query.
pub fn with_lang(path: &str, lang: Lang) -> String {
    if lang == Lang::En {
        return path.to_string();
    }
    let sep = if path.contains('?') { '&' } else { '?' };
    format!("{path}{sep}lang={}", lang.code())
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Forum,
    Status,
}

pub struct Page {
    pub status: u16,
    pub title: String,
    pub description: String,
    /// Canonical path (with query where it matters, e.g. `?page=2`), without `lang`.
    pub path: String,
    pub noindex: bool,
    pub crumbs: Vec<(String, String)>,
    pub jsonld: Vec<J>,
    pub body: String,
    pub md: String,
    pub prev: Option<String>,
    pub next: Option<String>,
    /// Never cached by shared caches (forms with tokens, personalised pages).
    pub private: bool,
    pub last_modified: Option<u64>,
    pub og_type: &'static str,
    /// `(title, path)` of Atom feeds for autodiscovery.
    pub feeds: Vec<(String, String)>,
    pub refresh_js: bool,
    pub extra_headers: Vec<(String, String)>,
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
            feeds: Vec::new(),
            refresh_js: false,
            extra_headers: Vec::new(),
        }
    }
}

fn json_script(v: &J) -> String {
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

pub fn css_version() -> String {
    sha256_hex(assets::CSS)[..10].to_string()
}

fn head(ctx: &Ctx, page: &Page, section: Section) -> String {
    let lang = ctx.lang;
    let canonical_path = with_lang(&page.path, lang);
    let canonical = ctx.abs(&canonical_path);
    let title = format!("{} | {}", page.title, SITE);
    let mut h = String::new();
    h.push_str("<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    h.push_str(&format!("<title>{}</title>\n", esc(&title)));
    h.push_str(&format!(
        "<meta name=\"description\" content=\"{}\">\n",
        esc(&page.description)
    ));
    h.push_str(if page.noindex {
        "<meta name=\"robots\" content=\"noindex, follow\">\n"
    } else {
        "<meta name=\"robots\" content=\"index, follow, max-snippet:-1, max-image-preview:large\">\n"
    });
    h.push_str("<meta name=\"color-scheme\" content=\"light dark\">\n");
    h.push_str(&format!(
        "<link rel=\"canonical\" href=\"{}\">\n",
        esc(&canonical)
    ));
    if !page.noindex {
        for l in Lang::ALL {
            h.push_str(&format!(
                "<link rel=\"alternate\" hreflang=\"{}\" href=\"{}\">\n",
                l.code(),
                esc(&ctx.abs(&with_lang(&page.path, l)))
            ));
        }
        h.push_str(&format!(
            "<link rel=\"alternate\" hreflang=\"x-default\" href=\"{}\">\n",
            esc(&ctx.abs(&page.path))
        ));
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
    for (ft, fp) in &page.feeds {
        h.push_str(&format!(
            "<link rel=\"alternate\" type=\"application/atom+xml\" title=\"{}\" href=\"{}\">\n",
            esc(ft),
            esc(&ctx.abs(fp))
        ));
    }
    if let Some(p) = &page.prev {
        h.push_str(&format!(
            "<link rel=\"prev\" href=\"{}\">\n",
            esc(&ctx.abs(&with_lang(p, lang)))
        ));
    }
    if let Some(n) = &page.next {
        h.push_str(&format!(
            "<link rel=\"next\" href=\"{}\">\n",
            esc(&ctx.abs(&with_lang(n, lang)))
        ));
    }
    for (p, c) in [
        ("og:site_name", SITE.to_string()),
        ("og:type", page.og_type.to_string()),
        ("og:title", page.title.clone()),
        ("og:description", page.description.clone()),
        ("og:url", canonical.clone()),
        ("og:locale", lang.og_locale().to_string()),
    ] {
        h.push_str(&format!(
            "<meta property=\"{p}\" content=\"{}\">\n",
            esc(&c)
        ));
    }
    for l in Lang::ALL.iter().filter(|l| **l != lang) {
        h.push_str(&format!(
            "<meta property=\"og:locale:alternate\" content=\"{}\">\n",
            l.og_locale()
        ));
    }
    for (n, c) in [
        ("twitter:card", "summary".to_string()),
        ("twitter:title", page.title.clone()),
        ("twitter:description", page.description.clone()),
    ] {
        h.push_str(&format!("<meta name=\"{n}\" content=\"{}\">\n", esc(&c)));
    }
    h.push_str(&format!(
        "<link rel=\"stylesheet\" href=\"/community/assets/app.css?v={}\">\n",
        css_version()
    ));
    if !page.crumbs.is_empty() {
        h.push_str(&json_script(&breadcrumb_ld(ctx, &page.crumbs)));
        h.push('\n');
    }
    for ld in &page.jsonld {
        h.push_str(&json_script(ld));
        h.push('\n');
    }
    if page.refresh_js {
        h.push_str("<script src=\"/status/assets/refresh.js\" defer></script>\n");
    }
    let _ = section;
    h
}

fn nav_link(ctx: &Ctx, section: Section, want: Option<Section>, href: &str, label: &str) -> String {
    let cur = want.is_some() && want == Some(section);
    format!(
        "<li><a href=\"{}\"{}>{}</a></li>",
        esc(&ctx.lpath(href)),
        if cur { " aria-current=\"page\"" } else { "" },
        esc(label)
    )
}

fn header(ctx: &Ctx, section: Section) -> String {
    let mut h = String::new();
    h.push_str(&format!(
        "<a class=\"skip\" href=\"#main\">{}</a>\n",
        esc(ctx.t("skip"))
    ));
    h.push_str("<header class=\"site\"><div class=\"wrap\">\n");
    h.push_str("<a class=\"brand\" href=\"/\">obop</a>\n");
    h.push_str(&format!(
        "<nav aria-label=\"{}\"><ul>",
        esc(ctx.t("nav.main"))
    ));
    h.push_str(&nav_link(ctx, section, None, "/", ctx.t("nav.home")));
    h.push_str(&nav_link(ctx, section, None, "/docs/", ctx.t("nav.docs")));
    h.push_str(&nav_link(
        ctx,
        section,
        Some(Section::Forum),
        "/community",
        ctx.t("nav.community"),
    ));
    h.push_str(&nav_link(
        ctx,
        section,
        Some(Section::Status),
        "/status",
        ctx.t("nav.status"),
    ));
    h.push_str("</ul></nav>\n");
    h.push_str(&format!(
        "<form class=\"search\" role=\"search\" action=\"/community/search\" method=\"get\">\
<label class=\"sr\" for=\"q\">{}</label><input id=\"q\" type=\"search\" name=\"q\" maxlength=\"100\" placeholder=\"{}\" autocomplete=\"off\">\
{}<button type=\"submit\" class=\"secondary\">{}</button></form>\n",
        esc(ctx.t("nav.search")),
        esc(ctx.t("nav.search_ph")),
        if ctx.lang == Lang::En { String::new() } else { format!("<input type=\"hidden\" name=\"lang\" value=\"{}\">", ctx.lang.code()) },
        esc(ctx.t("nav.search")),
    ));
    h.push_str("<div class=\"acct\">");
    match &ctx.user {
        Some(u) => {
            h.push_str(&format!(
                "<span>{}</span>",
                esc(&ctx.tf("nav.account", &[("name", &u.name)]))
            ));
            if u.staff {
                h.push_str(&format!(
                    "<a href=\"{}\">{}</a>",
                    esc(&ctx.lpath("/community/mod/log")),
                    esc(ctx.t("nav.mod_log"))
                ));
            }
            h.push_str(&format!(
                "<form action=\"/community/logout\" method=\"post\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><button class=\"link\" type=\"submit\">{}</button></form>",
                esc(&ctx.csrf),
                esc(ctx.t("nav.logout"))
            ));
        }
        None => {
            let next = urlenc(&ctx.req.path);
            h.push_str(&format!(
                "<a href=\"{}\">{}</a><a href=\"{}\">{}</a>",
                esc(&ctx.lpath(&format!("/community/login?next={next}"))),
                esc(ctx.t("nav.login")),
                esc(&ctx.lpath("/community/signup")),
                esc(ctx.t("nav.signup")),
            ));
        }
    }
    h.push_str("</div></div></header>\n");
    h
}

fn footer(ctx: &Ctx, page: &Page) -> String {
    let mut h = String::from("<footer class=\"site\"><div class=\"wrap\">\n");
    h.push_str(&format!(
        "<nav aria-label=\"{}\"><ul>",
        esc(ctx.t("nav.language"))
    ));
    for l in Lang::ALL {
        let href = with_lang(&page.path, l);
        // Switching language must set the cookie even for English, so always carry `lang`.
        let href = if l == Lang::En {
            let sep = if href.contains('?') { '&' } else { '?' };
            format!("{href}{sep}lang=en")
        } else {
            href
        };
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
    h.push_str("</ul></nav>\n");
    let canonical_path = with_lang(&page.path, ctx.lang);
    let sep = if canonical_path.contains('?') {
        '&'
    } else {
        '?'
    };
    h.push_str(&format!(
        "<p>{} <a href=\"{}\" type=\"text/markdown\">{}</a>",
        esc(ctx.t("footer.text")),
        esc(&format!("{canonical_path}{sep}format=md")),
        esc(ctx.t("footer.md"))
    ));
    for (_, fp) in page.feeds.iter().take(1) {
        h.push_str(&format!(
            " · <a href=\"{}\">{}</a>",
            esc(fp),
            esc(ctx.t("footer.feed"))
        ));
    }
    h.push_str("</p>\n</div></footer>\n");
    h
}

fn html_doc(ctx: &Ctx, page: &Page, section: Section) -> String {
    let mut o = String::with_capacity(page.body.len() + 4096);
    o.push_str(&format!(
        "<!doctype html>\n<html lang=\"{}\">\n<head>\n",
        ctx.lang.code()
    ));
    o.push_str(&head(ctx, page, section));
    o.push_str("</head>\n<body>\n");
    o.push_str(&header(ctx, section));
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
    o.push_str(&page.body);
    o.push_str("\n</main>\n");
    o.push_str(&footer(ctx, page));
    o.push_str("</body>\n</html>\n");
    o
}

fn md_doc(ctx: &Ctx, page: &Page) -> String {
    let canonical = ctx.abs(&with_lang(&page.path, ctx.lang));
    format!(
        "# {}\n\n> {}\n\n{}\n\n---\nSource: {}\n",
        page.title,
        page.description,
        page.md.trim_end(),
        canonical
    )
}

pub const CSP: &str = "default-src 'none'; style-src 'self'; script-src 'self'; img-src 'self' data:; form-action 'self'; base-uri 'none'; frame-ancestors 'none'";

pub fn security_headers(r: Resp) -> Resp {
    r.header("x-content-type-options", "nosniff")
        .header("referrer-policy", "strict-origin-when-cross-origin")
        .header("content-security-policy", CSP)
        .header("x-frame-options", "DENY")
        .header(
            "permissions-policy",
            "camera=(), microphone=(), geolocation=()",
        )
}

/// Turn a `Page` into the final response: HTML or Markdown, cache and validator headers, cookies.
pub fn render(ctx: &Ctx, page: Page, section: Section) -> Resp {
    let md = ctx.md;
    let (ctype, body) = if md {
        ("text/markdown; charset=utf-8", md_doc(ctx, &page))
    } else {
        ("text/html; charset=utf-8", html_doc(ctx, &page, section))
    };
    let mut r = Resp::new(page.status, ctype, body);
    let personalised = ctx.user.is_some() || page.private || !ctx.set_cookies.is_empty();
    if page.status >= 400 {
        r = r.header("cache-control", "no-cache");
    } else if personalised || !matches!(ctx.req.method.as_str(), "GET" | "HEAD") {
        r = r.header("cache-control", "private, no-store");
    } else {
        r = r.header(
            "cache-control",
            "public, max-age=60, stale-while-revalidate=300",
        );
    }
    r = r.header("vary", "Accept, Accept-Language, Cookie");
    r = r.header("content-language", ctx.lang.code());
    if page.noindex || page.status >= 400 {
        r = r.header("x-robots-tag", "noindex");
    }
    r = r.header(
        "link",
        &format!(
            "<{}>; rel=\"canonical\"",
            ctx.abs(&with_lang(&page.path, ctx.lang))
        ),
    );
    for (k, v) in &page.extra_headers {
        r = r.header(k, v);
    }
    for c in &ctx.set_cookies {
        r = r.add_header("set-cookie", c);
    }
    let lm = if personalised {
        None
    } else {
        page.last_modified
    };
    let r = if personalised && page.status == 200 {
        r
    } else {
        r.conditional(ctx.req, lm)
    };
    security_headers(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang_paths() {
        assert_eq!(with_lang("/community", Lang::En), "/community");
        assert_eq!(with_lang("/community", Lang::De), "/community?lang=de");
        assert_eq!(with_lang("/c/x?page=2", Lang::Fi), "/c/x?page=2&lang=fi");
    }

    #[test]
    fn json_ld_cannot_break_out_of_the_script_tag() {
        let s = json_script(&json!({"name": "</script><script>alert(1)</script>"}));
        assert_eq!(s.matches("</script>").count(), 1);
        assert!(!s.contains("<script>alert"));
    }
}
