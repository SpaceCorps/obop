//! Pages, the home page, the blog and the contact form.

use crate::http::Resp;
use crate::i18n::Lang;
use crate::layout::{self, Ctx, Page, Variants};
use crate::seo;
use crate::shop::{self, product_card};
use crate::util::{self, clean_line, date_to_ts, esc, truncate_chars};
use crate::{Site, md};
use obop_commands::{Lookup, PublicRecord, Submission, SubmitError, Value};
use serde_json::json;
use std::collections::BTreeMap;

/// The text of a record in the visitor's language, with the base text where a translation is
/// missing.
pub(crate) struct Loc {
    pub text: BTreeMap<&'static str, String>,
    /// Language the text is written in.
    pub lang: Lang,
    /// Languages that exist for the record, the site's default first.
    pub langs: Vec<Lang>,
}

impl Loc {
    pub fn get(&self, k: &str) -> &str {
        self.text.get(k).map_or("", String::as_str)
    }
}

const PER_PAGE_POSTS: usize = 10;

fn paged(path: &str, n: usize) -> String {
    if n <= 1 {
        path.to_string()
    } else {
        format!("{path}?page={n}")
    }
}

pub(crate) fn page_param(ctx: &Ctx) -> usize {
    ctx.req
        .q("page")
        .and_then(|p| p.parse::<usize>().ok())
        .unwrap_or(1)
        .clamp(1, 10_000)
}

/// One field of a form, rendered with its label, help text and error state.
pub(crate) struct Field<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub kind: &'a str,
    pub value: &'a str,
    pub required: bool,
    pub help: &'a str,
    pub autocomplete: &'a str,
    pub invalid: bool,
    pub maxlen: usize,
}

impl Field<'_> {
    pub fn html(&self, ctx: &Ctx) -> String {
        let help_id = format!("{}-help", self.id);
        let describedby = if self.help.is_empty() {
            String::new()
        } else {
            format!(" aria-describedby=\"{}\"", esc(&help_id))
        };
        let common = format!(
            "id=\"{id}\" name=\"{id}\"{req}{inv}{auto} maxlength=\"{max}\"{desc}",
            id = esc(self.id),
            req = if self.required { " required" } else { "" },
            inv = if self.invalid {
                " aria-invalid=\"true\""
            } else {
                ""
            },
            auto = if self.autocomplete.is_empty() {
                String::new()
            } else {
                format!(" autocomplete=\"{}\"", esc(self.autocomplete))
            },
            max = self.maxlen,
            desc = describedby,
        );
        let control = if self.kind == "textarea" {
            format!(
                "<textarea {common} rows=\"5\">{}</textarea>",
                esc(self.value)
            )
        } else {
            format!(
                "<input type=\"{}\" {common} value=\"{}\">",
                esc(self.kind),
                esc(self.value)
            )
        };
        let req = if self.required {
            format!(
                " <span class=\"req\">({})</span>",
                esc(ctx.t("co.required"))
            )
        } else {
            String::new()
        };
        let help = if self.help.is_empty() {
            String::new()
        } else {
            format!(
                "<span class=\"help\" id=\"{}\">{}</span>",
                esc(&help_id),
                esc(self.help)
            )
        };
        format!(
            "<div class=\"field\"><label for=\"{}\">{}{}</label>{}{}</div>",
            esc(self.id),
            esc(self.label),
            req,
            control,
            help
        )
    }
}

/// The summary box that lists what is wrong with a submitted form.
pub(crate) fn error_summary(ctx: &Ctx, errors: &[(&str, String)]) -> String {
    if errors.is_empty() {
        return String::new();
    }
    let mut h = format!(
        "<div class=\"err-sum\" role=\"alert\"><strong>{}</strong><ul>",
        esc(ctx.t("co.errors"))
    );
    for (id, msg) in errors {
        if id.is_empty() {
            h.push_str(&format!("<li>{}</li>", esc(msg)));
        } else {
            h.push_str(&format!(
                "<li><a href=\"#{}\">{}</a></li>",
                esc(id),
                esc(msg)
            ));
        }
    }
    h.push_str("</ul></div>");
    h
}

/// `a@b.c` shaped, no whitespace or control characters, a sane length.
pub(crate) fn email_ok(e: &str) -> bool {
    let e = e.trim();
    let Some((local, domain)) = e.split_once('@') else {
        return false;
    };
    e.len() <= 254
        && !local.is_empty()
        && local.len() <= 64
        && !domain.contains('@')
        && domain.contains('.')
        && !domain.starts_with(['.', '-'])
        && !domain.ends_with(['.', '-'])
        && !domain.contains("..")
        && e.chars()
            .all(|c| !c.is_control() && !c.is_whitespace() && !"<>\"',;\\".contains(c))
}

impl Site {
    pub(crate) fn localize(
        &self,
        ctx: &Ctx,
        rec: &PublicRecord,
        tr_model: &str,
        ref_field: &str,
        keys: &[&'static str],
    ) -> Loc {
        let default = ctx.s.default_lang;
        let trs = self.translations(tr_model, ref_field, &rec.id);
        let mut langs = vec![default];
        for t in &trs {
            if let Some(l) = Lang::parse(t.text("lang"))
                && !langs.contains(&l)
            {
                langs.push(l);
            }
        }
        let chosen = (ctx.lang != default)
            .then(|| {
                trs.iter()
                    .find(|t| Lang::parse(t.text("lang")) == Some(ctx.lang))
            })
            .flatten();
        let text = keys
            .iter()
            .map(|k| {
                let v = chosen
                    .map(|t| t.text(k))
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| rec.text(k));
                (*k, v.to_string())
            })
            .collect();
        Loc {
            text,
            lang: if chosen.is_some() { ctx.lang } else { default },
            langs,
        }
    }

    // ---- /{slug} --------------------------------------------------------------------------

    pub(crate) fn page(&self, ctx: &mut Ctx, slug: &str) -> Resp {
        if slug == "home" {
            return Resp::redirect(301, &ctx.lpath("/"));
        }
        if !ctx.caps.pages || seo::is_reserved(slug) {
            return self.not_found(ctx);
        }
        match self.find("website_page", slug) {
            Lookup::Found(rec) => self.render_page(ctx, &rec, &format!("/{slug}")),
            Lookup::Gone => self.gone(ctx),
            Lookup::NotFound => self.not_found(ctx),
        }
    }

    fn render_page(&self, ctx: &Ctx, rec: &PublicRecord, path: &str) -> Resp {
        let loc = self.localize(
            ctx,
            rec,
            "website_page_translation",
            "page_id",
            &["title", "body", "seo_title", "seo_description"],
        );
        let title = loc.get("title").to_string();
        let seo_title = match loc.get("seo_title") {
            "" => title.clone(),
            s => s.to_string(),
        };
        let desc = describe(loc.get("seo_description"), loc.get("body"), &ctx.s.tagline);
        let url = ctx.abs(path);
        let mut p = Page::new(&seo_title, &desc, path);
        p.variants = Variants::Only(loc.langs.clone());
        p.content_lang = Some(loc.lang);
        p.last_modified = date_to_ts(rec.text("published_on"));
        p.crumbs = vec![
            (ctx.t("nav.home").into(), "/".into()),
            (title.clone(), path.into()),
        ];
        p.jsonld.push(json!({
            "@context": "https://schema.org", "@type": "WebPage", "name": title,
            "description": desc, "url": url, "inLanguage": loc.lang.code(),
            "isPartOf": {"@type": "WebSite", "name": ctx.s.title, "url": ctx.abs("/")},
            "datePublished": rec.text("published_on"),
        }));
        p.body = format!(
            "<article><h1>{}</h1><div class=\"prose\">{}</div></article>",
            esc(&title),
            md::render(loc.get("body"))
        );
        p.md = format!("{}\n", md::to_markdown(loc.get("body")));
        layout::render(ctx, p)
    }

    // ---- / -------------------------------------------------------------------------------

    pub(crate) fn home(&self, ctx: &mut Ctx) -> Resp {
        let req = ctx.req;
        if !matches!(req.method.as_str(), "GET" | "HEAD") {
            return self.not_found(ctx);
        }
        let home = if ctx.caps.pages {
            match self.find("website_page", "home") {
                Lookup::Found(r) => Some(r),
                _ => None,
            }
        } else {
            None
        };
        let products = if ctx.caps.shop {
            self.api
                .list("ecommerce_listing", &[], Some(4), 0)
                .map(|(r, _)| r)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let posts = if ctx.caps.blog {
            self.api
                .list("website_blog_post", &[], Some(3), 0)
                .map(|(r, _)| r)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let other_pages = ctx.caps.pages
            && self
                .api
                .list("website_page", &[], Some(1), 0)
                .is_ok_and(|(r, _)| !r.is_empty());
        if home.is_none() && products.is_empty() && posts.is_empty() && !other_pages {
            return self.unpublished(ctx);
        }

        let (title, lede, body_md, body_html, langs, content_lang, lm, seo_desc) = match &home {
            Some(rec) => {
                let loc = self.localize(
                    ctx,
                    rec,
                    "website_page_translation",
                    "page_id",
                    &["title", "body", "seo_title", "seo_description"],
                );
                let lede = match loc.get("seo_description") {
                    "" => ctx.s.tagline.clone(),
                    d => d.to_string(),
                };
                let desc = describe(loc.get("seo_description"), loc.get("body"), &ctx.s.tagline);
                (
                    loc.get("title").to_string(),
                    lede,
                    md::to_markdown(loc.get("body")),
                    md::render(loc.get("body")),
                    loc.langs.clone(),
                    Some(loc.lang),
                    date_to_ts(rec.text("published_on")),
                    desc,
                )
            }
            None => (
                ctx.s.title.clone(),
                ctx.s.tagline.clone(),
                String::new(),
                String::new(),
                Vec::new(),
                None,
                None,
                describe("", "", &ctx.s.tagline),
            ),
        };
        let mut p = Page::new(&title, &seo_desc, "/");
        p.variants = if home.is_some() {
            Variants::Only(langs)
        } else {
            Variants::Chrome
        };
        p.content_lang = content_lang;
        p.last_modified = lm;
        p.jsonld.push(seo::website_ld(ctx));
        p.jsonld.push(seo::organization_ld(ctx));
        let mut h = format!("<section class=\"hero\"><h1>{}</h1>", esc(&title));
        if !lede.is_empty() {
            h.push_str(&format!("<p class=\"lede\">{}</p>", esc(&lede)));
        }
        h.push_str("<div class=\"actions\">");
        if ctx.caps.shop {
            h.push_str(&format!(
                "<a class=\"btn\" href=\"{}\">{}</a>",
                esc(&ctx.lpath("/shop")),
                esc(ctx.t("home.browse"))
            ));
        }
        if ctx.caps.blog {
            h.push_str(&format!(
                "<a class=\"btn secondary\" href=\"{}\">{}</a>",
                esc(&ctx.lpath("/blog")),
                esc(ctx.t("home.latest"))
            ));
        }
        h.push_str("</div></section>");
        if !body_html.is_empty() {
            h.push_str(&format!("<div class=\"prose\">{body_html}</div>"));
        }
        let mut md_out = String::new();
        if !body_md.trim().is_empty() {
            md_out.push_str(&body_md);
            md_out.push('\n');
        }
        if !products.is_empty() {
            let tr = self.shop_translations(ctx);
            h.push_str(&format!(
                "<section class=\"section\" aria-labelledby=\"feat\"><header><h2 id=\"feat\">{}</h2><a href=\"{}\">{}</a></header><ul class=\"grid\">",
                esc(ctx.t("home.featured")),
                esc(&ctx.lpath("/shop")),
                esc(ctx.t("home.all_products"))
            ));
            md_out.push_str(&format!("\n## {}\n\n", ctx.t("home.featured")));
            for r in &products {
                h.push_str(&product_card(ctx, r, tr.get(&r.id)));
                md_out.push_str(&shop::product_md_line(ctx, r, tr.get(&r.id)));
            }
            h.push_str("</ul></section>");
        }
        if !posts.is_empty() {
            let tr = self.blog_translations(ctx);
            h.push_str(&format!(
                "<section class=\"section\" aria-labelledby=\"latest\"><header><h2 id=\"latest\">{}</h2><a href=\"{}\">{}</a></header><ul class=\"posts\">",
                esc(ctx.t("home.latest")),
                esc(&ctx.lpath("/blog")),
                esc(ctx.t("home.all_posts"))
            ));
            md_out.push_str(&format!("\n## {}\n\n", ctx.t("home.latest")));
            for r in &posts {
                h.push_str(&post_card(ctx, r, tr.get(&r.id), 3));
                md_out.push_str(&post_md_line(ctx, r, tr.get(&r.id)));
            }
            h.push_str("</ul></section>");
        }
        p.body = h;
        p.md = md_out;
        layout::render(ctx, p)
    }

    /// Friendly page for an instance with nothing published yet.
    fn unpublished(&self, ctx: &Ctx) -> Resp {
        let mut p = Page::new(ctx.t("unpub.title"), ctx.t("unpub.body"), "/");
        p.noindex = true;
        let mut h = format!(
            "<section class=\"hero\"><h1>{}</h1><p class=\"lede\">{}</p>",
            esc(ctx.t("unpub.title")),
            esc(ctx.t("unpub.body"))
        );
        if let Some(a) = &ctx.admin_prefix {
            h.push_str(&format!(
                "<p><a class=\"btn secondary\" href=\"{}/\">{}</a></p>",
                esc(a),
                esc(ctx.t("unpub.owner"))
            ));
        }
        h.push_str("</section>");
        p.body = h;
        p.md = format!("{}\n", ctx.t("unpub.body"));
        layout::render(ctx, p)
    }

    // ---- blog -----------------------------------------------------------------------------

    pub(crate) fn blog_translations(&self, ctx: &Ctx) -> BTreeMap<String, PublicRecord> {
        if ctx.lang == ctx.s.default_lang {
            return BTreeMap::new();
        }
        self.translations_into("website_blog_post_translation", "post_id", ctx.lang)
            .into_iter()
            .collect()
    }

    pub(crate) fn blog_index(&self, ctx: &mut Ctx) -> Resp {
        if !ctx.caps.blog {
            return self.not_found(ctx);
        }
        let page = page_param(ctx);
        let (posts, total) = self
            .api
            .list(
                "website_blog_post",
                &[],
                Some(PER_PAGE_POSTS),
                (page - 1) * PER_PAGE_POSTS,
            )
            .unwrap_or_default();
        let pages = total.div_ceil(PER_PAGE_POSTS).max(1);
        if page > pages {
            return self.not_found(ctx);
        }
        let tr = self.blog_translations(ctx);
        let path = paged("/blog", page);
        let mut p = Page::new(ctx.t("blog.title"), ctx.t("blog.intro"), &path);
        p.variants = Variants::Chrome;
        p.og_type = "website";
        p.crumbs = vec![
            (ctx.t("nav.home").into(), "/".into()),
            (ctx.t("blog.title").into(), path.clone()),
        ];
        p.prev = (page > 1).then(|| paged("/blog", page - 1));
        p.next = (page < pages).then(|| paged("/blog", page + 1));
        p.last_modified = posts
            .iter()
            .filter_map(|r| date_to_ts(r.text("published_on")))
            .max();
        let items: Vec<(String, String)> = posts
            .iter()
            .map(|r| {
                (
                    tr.get(&r.id)
                        .map_or(r.text("title"), |t| t.text("title"))
                        .to_string(),
                    format!("/blog/{}", r.text("slug")),
                )
            })
            .collect();
        p.jsonld
            .push(seo::item_list_ld(ctx, ctx.t("blog.title"), &items));
        let mut h = format!(
            "<h1>{}</h1><p class=\"lede\">{}</p>",
            esc(ctx.t("blog.title")),
            esc(ctx.t("blog.intro"))
        );
        let mut md_out = String::new();
        if posts.is_empty() {
            h.push_str(&format!("<p>{}</p>", esc(ctx.t("blog.empty"))));
            md_out.push_str(ctx.t("blog.empty"));
        } else {
            h.push_str("<ul class=\"posts\">");
            for r in &posts {
                h.push_str(&post_card(ctx, r, tr.get(&r.id), 2));
                md_out.push_str(&post_md_line(ctx, r, tr.get(&r.id)));
            }
            h.push_str("</ul>");
        }
        h.push_str(&pager(ctx, "/blog", page, pages, &[]));
        p.body = h;
        p.md = md_out;
        layout::render(ctx, p)
    }

    pub(crate) fn blog_post(&self, ctx: &mut Ctx, slug: &str) -> Resp {
        if !ctx.caps.blog {
            return self.not_found(ctx);
        }
        let rec = match self.find("website_blog_post", slug) {
            Lookup::Found(r) => r,
            Lookup::Gone => return self.gone(ctx),
            Lookup::NotFound => return self.not_found(ctx),
        };
        let path = format!("/blog/{slug}");
        let loc = self.localize(
            ctx,
            &rec,
            "website_blog_post_translation",
            "post_id",
            &["title", "summary", "body"],
        );
        let title = loc.get("title").to_string();
        let desc = describe(loc.get("summary"), loc.get("body"), &ctx.s.tagline);
        let byline = match rec.text("byline") {
            "" => ctx.s.title.clone(),
            b => b.to_string(),
        };
        let date = rec.text("published_on");
        let mut p = Page::new(&title, &desc, &path);
        p.variants = Variants::Only(loc.langs.clone());
        p.content_lang = Some(loc.lang);
        p.og_type = "article";
        p.last_modified = date_to_ts(date);
        p.crumbs = vec![
            (ctx.t("nav.home").into(), "/".into()),
            (ctx.t("blog.title").into(), "/blog".into()),
            (title.clone(), path.clone()),
        ];
        let mut ld = json!({
            "@context": "https://schema.org", "@type": "BlogPosting",
            "headline": title, "description": desc, "url": ctx.abs(&path),
            "mainEntityOfPage": ctx.abs(&path), "inLanguage": loc.lang.code(),
            "author": {"@type": if rec.text("byline").is_empty() { "Organization" } else { "Person" }, "name": byline},
            "publisher": seo::organization_ld(ctx),
        });
        if date_to_ts(date).is_some() {
            ld["datePublished"] = json!(date);
            ld["dateModified"] = json!(date);
        }
        let tags: Vec<&str> = rec
            .text("tags")
            .split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .take(10)
            .collect();
        if !tags.is_empty() {
            ld["keywords"] = json!(tags.join(", "));
        }
        p.jsonld.push(ld);
        let mut h = format!("<article><h1>{}</h1><p class=\"meta\">", esc(&title));
        if date_to_ts(date).is_some() {
            h.push_str(&format!(
                "<time datetime=\"{}\">{}</time>",
                esc(date),
                esc(&ctx.date(date))
            ));
        }
        h.push_str(&format!(
            "<span>{}</span></p>",
            esc(&ctx.tf("blog.by", &[("name", &byline)]))
        ));
        h.push_str(&format!(
            "<div class=\"prose\">{}</div>",
            md::render(loc.get("body"))
        ));
        if !tags.is_empty() {
            h.push_str("<ul class=\"tags\">");
            for t in &tags {
                h.push_str(&format!("<li><span class=\"badge\">{}</span></li>", esc(t)));
            }
            h.push_str("</ul>");
        }
        h.push_str(&format!(
            "</article><p><a href=\"{}\">&larr; {}</a></p>",
            esc(&ctx.lpath("/blog")),
            esc(ctx.t("blog.back"))
        ));
        p.body = h;
        p.md = format!(
            "{} · {}\n\n{}\n",
            if date.is_empty() {
                String::new()
            } else {
                ctx.date(date)
            },
            ctx.tf("blog.by", &[("name", &byline)]),
            md::to_markdown(loc.get("body"))
        );
        layout::render(ctx, p)
    }

    // ---- /contact -------------------------------------------------------------------------

    pub(crate) fn contact(&self, ctx: &mut Ctx) -> Resp {
        if !ctx.caps.contact {
            return self.not_found(ctx);
        }
        let post = ctx.req.method == "POST";
        let form = ctx.req.form();
        let mut errors: Vec<(&str, String)> = Vec::new();
        if post {
            if let Err(r) = self.check_post(ctx, &form) {
                return r;
            }
            // A bot fills the hidden field; it is told everything went well.
            if !form.get("website").is_empty() {
                return Resp::redirect(303, &sent_url(ctx));
            }
            let name = clean_line(form.get("name"));
            let email = form.get("email").trim().to_string();
            let phone = clean_line(form.get("phone"));
            let message = form.get("message").trim().replace('\r', "");
            if name.is_empty() || name.chars().count() > 120 {
                errors.push(("name", ctx.t("err.name").into()));
            }
            if !email_ok(&email) {
                errors.push(("email", ctx.t("err.email").into()));
            }
            if message.is_empty() || message.chars().count() > 5000 {
                errors.push(("message", ctx.t("err.message").into()));
            }
            if form.get("consent") != "1" {
                errors.push(("consent", ctx.t("err.consent").into()));
            }
            if errors.is_empty() {
                let mut visitor = BTreeMap::new();
                visitor.insert("sender_name".to_string(), Value::text(&name));
                visitor.insert("sender_email".to_string(), Value::text(&email));
                if !phone.is_empty() {
                    visitor.insert(
                        "sender_phone".to_string(),
                        Value::text(&truncate_chars(&phone, 40)),
                    );
                }
                visitor.insert("message".to_string(), Value::text(&message));
                visitor.insert("consent_given".to_string(), Value::Bool(true));
                let server = BTreeMap::from([
                    ("form_name".to_string(), Value::text("contact")),
                    ("source_page".to_string(), Value::text("contact")),
                    (
                        "submitted_on".to_string(),
                        Value::text(&crate::i18n::today()),
                    ),
                ]);
                let sub = Submission {
                    model: "website_form_submission".into(),
                    visitor,
                    server,
                    links: vec![],
                };
                match self.api.submit_batch(&self.source(ctx.req), vec![sub]) {
                    Ok(_) => return Resp::redirect(303, &sent_url(ctx)),
                    Err(SubmitError::RateLimited { retry_after_secs }) => {
                        return self
                            .error(ctx, 429, "e429.title", "err.rate")
                            .header("retry-after", &retry_after_secs.to_string());
                    }
                    Err(_) => errors.push(("", ctx.t("err.generic").into())),
                }
            }
        }
        self.ensure_csrf(ctx);
        let sent = !post && ctx.req.q("sent").as_deref() == Some("1");
        let intro_page = if ctx.caps.pages {
            match self.find("website_page", "contact") {
                Lookup::Found(r) => Some(r),
                _ => None,
            }
        } else {
            None
        };
        let mut p = Page::new(ctx.t("contact.title"), ctx.t("contact.intro"), "/contact");
        p.variants = Variants::Chrome;
        p.private = true;
        p.noindex = sent || post;
        if post && !errors.is_empty() {
            p.status = 400;
        }
        p.crumbs = vec![
            (ctx.t("nav.home").into(), "/".into()),
            (ctx.t("contact.title").into(), "/contact".into()),
        ];
        let mut h = format!("<h1>{}</h1>", esc(ctx.t("contact.title")));
        let mut md_out = String::new();
        if sent {
            h.push_str(&format!(
                "<div class=\"notice ok\" role=\"status\"><strong>{}</strong> {}</div>",
                esc(ctx.t("contact.thanks_title")),
                esc(ctx.t("contact.thanks"))
            ));
            md_out.push_str(ctx.t("contact.thanks"));
        }
        if let Some(r) = &intro_page {
            let loc = self.localize(ctx, r, "website_page_translation", "page_id", &["body"]);
            h.push_str(&format!(
                "<div class=\"prose\">{}</div>",
                md::render(loc.get("body"))
            ));
            md_out.push_str(&md::to_markdown(loc.get("body")));
        } else {
            h.push_str(&format!(
                "<p class=\"lede\">{}</p>",
                esc(ctx.t("contact.intro"))
            ));
            md_out.push_str(ctx.t("contact.intro"));
        }
        if !sent {
            h.push_str(&error_summary(ctx, &errors));
            let bad = |id: &str| errors.iter().any(|(e, _)| *e == id);
            let field = |id, label, kind, required, auto, max| Field {
                id,
                label,
                kind,
                value: form.get(id),
                required,
                help: "",
                autocomplete: auto,
                invalid: bad(id),
                maxlen: max,
            };
            h.push_str(&format!(
                "<form class=\"formcard\" method=\"post\" action=\"/contact\" novalidate>\
<input type=\"hidden\" name=\"csrf\" value=\"{}\">\
<input type=\"hidden\" name=\"lang\" value=\"{}\">",
                esc(&ctx.csrf),
                ctx.lang.code()
            ));
            h.push_str(&field("name", ctx.t("contact.name"), "text", true, "name", 120).html(ctx));
            h.push_str(
                &field("email", ctx.t("contact.email"), "email", true, "email", 254).html(ctx),
            );
            h.push_str(&field("phone", ctx.t("contact.phone"), "tel", false, "tel", 40).html(ctx));
            h.push_str(
                &field(
                    "message",
                    ctx.t("contact.message"),
                    "textarea",
                    true,
                    "",
                    5000,
                )
                .html(ctx),
            );
            h.push_str(&format!(
                "<div class=\"hp\" aria-hidden=\"true\"><label for=\"website\">{}</label><input type=\"text\" id=\"website\" name=\"website\" tabindex=\"-1\" autocomplete=\"off\"></div>",
                esc(ctx.t("co.honeypot"))
            ));
            h.push_str(&format!(
                "<div class=\"field\"><label class=\"check\" for=\"consent\"><input type=\"checkbox\" id=\"consent\" name=\"consent\" value=\"1\"{}{}> <span>{}</span></label></div>",
                if form.get("consent") == "1" { " checked" } else { "" },
                if bad("consent") { " aria-invalid=\"true\"" } else { "" },
                esc(ctx.t("contact.consent"))
            ));
            h.push_str(&format!(
                "<button class=\"btn\" type=\"submit\">{}</button></form>",
                esc(ctx.t("contact.send"))
            ));
        }
        p.body = h;
        p.md = md_out;
        layout::render(ctx, p)
    }
}

fn sent_url(ctx: &Ctx) -> String {
    if ctx.lang == ctx.s.default_lang {
        "/contact?sent=1".to_string()
    } else {
        format!("/contact?sent=1&lang={}", ctx.lang.code())
    }
}

/// Meta description: the record's own, else the start of its text, else the site tagline.
pub(crate) fn describe(own: &str, body: &str, fallback: &str) -> String {
    let own = clean_line(own);
    if !own.is_empty() {
        return truncate_chars(&own, 200);
    }
    let from_body = md::excerpt(body, 160);
    if !from_body.is_empty() {
        return from_body;
    }
    let f = clean_line(fallback);
    if f.is_empty() {
        "-".into()
    } else {
        truncate_chars(&f, 200)
    }
}

fn post_card(ctx: &Ctx, r: &PublicRecord, tr: Option<&PublicRecord>, level: u8) -> String {
    let pick = |k: &str| {
        tr.map(|t| t.text(k))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| r.text(k))
    };
    let title = pick("title");
    let summary = match pick("summary") {
        "" => md::excerpt(
            tr.map_or(r.text("body"), |t| match t.text("body") {
                "" => r.text("body"),
                b => b,
            }),
            180,
        ),
        s => s.to_string(),
    };
    let date = r.text("published_on");
    let byline = r.text("byline");
    let mut meta = String::new();
    if date_to_ts(date).is_some() {
        meta.push_str(&format!(
            "<time datetime=\"{}\">{}</time>",
            esc(date),
            esc(&ctx.date(date))
        ));
    }
    if !byline.is_empty() {
        meta.push_str(&format!(
            "<span>{}</span>",
            esc(&ctx.tf("blog.by", &[("name", byline)]))
        ));
    }
    let lang_attr = match tr {
        Some(_) => String::new(),
        None if ctx.s.default_lang != ctx.lang => {
            format!(" lang=\"{}\"", ctx.s.default_lang.code())
        }
        None => String::new(),
    };
    format!(
        "<li class=\"post\"{lang_attr}><h{level}><a href=\"{}\">{}</a></h{level}><p class=\"meta\">{}</p><p>{}</p></li>",
        esc(&ctx.lpath(&format!("/blog/{}", r.text("slug")))),
        esc(title),
        meta,
        esc(&summary)
    )
}

fn post_md_line(ctx: &Ctx, r: &PublicRecord, tr: Option<&PublicRecord>) -> String {
    let title = tr.map_or(r.text("title"), |t| t.text("title"));
    format!(
        "- [{}]({}) {}\n",
        title.replace(['[', ']'], ""),
        ctx.abs(&format!("/blog/{}", r.text("slug"))),
        r.text("published_on")
    )
}

/// Previous/next navigation. `keep` carries extra query pairs (category, search) forward.
pub(crate) fn pager(
    ctx: &Ctx,
    base: &str,
    page: usize,
    pages: usize,
    keep: &[(&str, String)],
) -> String {
    if pages <= 1 {
        return String::new();
    }
    let href = |n: usize| {
        let mut q: Vec<String> = keep
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, v)| format!("{k}={}", util::urlenc(v)))
            .collect();
        if n > 1 {
            q.push(format!("page={n}"));
        }
        let path = if q.is_empty() {
            base.to_string()
        } else {
            format!("{base}?{}", q.join("&"))
        };
        ctx.lpath(&path)
    };
    let mut h = format!(
        "<nav class=\"pager\" aria-label=\"{}\">",
        esc(ctx.t("shop.pagination"))
    );
    if page > 1 {
        h.push_str(&format!(
            "<a class=\"btn secondary small\" rel=\"prev\" href=\"{}\">{}</a>",
            esc(&href(page - 1)),
            esc(ctx.t("shop.prev"))
        ));
    }
    h.push_str(&format!(
        "<span>{}</span>",
        esc(&ctx.tf(
            "shop.page",
            &[("n", &page.to_string()), ("m", &pages.to_string())]
        ))
    ));
    if page < pages {
        h.push_str(&format!(
            "<a class=\"btn secondary small\" rel=\"next\" href=\"{}\">{}</a>",
            esc(&href(page + 1)),
            esc(ctx.t("shop.next"))
        ));
    }
    h.push_str("</nav>");
    h
}
