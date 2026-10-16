//! The forum's pages: routing, rendering (HTML and Markdown), forms, feeds and the sitemap.

use crate::Community;
use crate::accounts::{self, CSRF_COOKIE, SESSION_COOKIE, SESSION_SECS};
use crate::http::{Resp, cookie, expire_cookie};
use crate::i18n::{Lang, t};
use crate::layout::{Ctx, Page, Section, User, render, with_lang};
use crate::md;
use crate::store::{self, Data, FErr, ModAction, Thread, paginate};
use crate::util::{self, esc, slugify, truncate_chars, urlenc};
use obop_commands::Authenticator;
use serde_json::{Value as J, json};
use std::fmt::Write as _;

const NAME_MAX: usize = 120;

pub struct SitemapUrl {
    pub loc: String,
    pub lastmod: Option<u64>,
}

enum Route<'a> {
    Index,
    Css,
    Sitemap,
    Feed(Option<&'a str>),
    Search,
    Login,
    Signup,
    Logout,
    ModLog,
    ModPost,
    Category(&'a str),
    NewThread(&'a str),
    Thread(&'a str),
    Reply(&'a str),
    Edit(&'a str),
    User(&'a str),
    Unknown,
}

fn route(path: &str) -> Route<'_> {
    let p = path.trim_end_matches('/');
    let rest = p.strip_prefix("/community").unwrap_or("");
    let parts: Vec<&str> = rest.split('/').skip(1).collect();
    match parts.as_slice() {
        [] => Route::Index,
        ["assets", "app.css"] => Route::Css,
        ["sitemap.xml"] => Route::Sitemap,
        ["feed.xml"] => Route::Feed(None),
        ["search"] => Route::Search,
        ["login"] => Route::Login,
        ["signup"] => Route::Signup,
        ["logout"] => Route::Logout,
        ["mod", "log"] => Route::ModLog,
        ["mod"] => Route::ModPost,
        ["c", slug] => Route::Category(slug),
        ["c", slug, "feed.xml"] => Route::Feed(Some(slug)),
        ["c", slug, "new"] => Route::NewThread(slug),
        ["t", seg] => Route::Thread(seg),
        ["t", seg, "reply"] => Route::Reply(seg),
        ["p", id, "edit"] => Route::Edit(id),
        ["u", name] => Route::User(name),
        _ => Route::Unknown,
    }
}

fn allowed(r: &Route) -> &'static [&'static str] {
    match r {
        Route::Login | Route::Signup | Route::NewThread(_) | Route::Reply(_) | Route::Edit(_) => {
            &["GET", "HEAD", "POST"]
        }
        Route::Logout | Route::ModPost => &["POST"],
        Route::Unknown => &["GET", "HEAD"],
        _ => &["GET", "HEAD"],
    }
}

fn decode(seg: &str) -> String {
    form_urlencoded::parse(seg.replace('+', "%2B").as_bytes())
        .next()
        .map(|(k, v)| {
            if v.is_empty() {
                k.into_owned()
            } else {
                format!("{k}={v}")
            }
        })
        .unwrap_or_default()
}

/// `123-some-title` -> (123, "some-title")
fn parse_thread_seg(seg: &str) -> Option<(u64, &str)> {
    let digits = seg.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits > 12 {
        return None;
    }
    let id = seg[..digits].parse().ok()?;
    let rest = &seg[digits..];
    if !rest.is_empty() && !rest.starts_with('-') {
        return None;
    }
    Some((id, rest.trim_start_matches('-')))
}

pub fn thread_path(t: &Thread, visible_title: bool) -> String {
    let slug = if visible_title {
        slugify(&t.title)
    } else {
        "removed".to_string()
    };
    format!("/community/t/{}-{}", t.id, slug)
}

fn cat_name(lang: Lang, d: &Data, slug: &str) -> String {
    let key = format!("cat.{slug}.name");
    let v = t(lang, &key);
    if v != "?" {
        v.to_string()
    } else {
        d.category(slug)
            .map_or_else(|| slug.to_string(), |c| c.name.clone())
    }
}

fn cat_desc(lang: Lang, slug: &str) -> String {
    let key = format!("cat.{slug}.desc");
    let v = t(lang, &key);
    if v != "?" {
        v.to_string()
    } else {
        String::new()
    }
}

fn is_staff_name(app: &Community, d: &Data, email: &str) -> bool {
    let _ = d;
    app.forum.is_staff(email)
}

fn safe_next(next: &str) -> Option<String> {
    let ok = next.len() <= 300
        && next.starts_with('/')
        && !next.starts_with("//")
        && !next.contains('\\')
        && !next.chars().any(char::is_control)
        && (next.starts_with("/community") || next.starts_with("/status"));
    ok.then(|| next.to_string())
}

// ---------------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------------

fn categories_list(ctx: &Ctx, d: &Data) -> (String, String) {
    let mut h = String::from("<ul class=\"list\">");
    let mut m = String::new();
    for c in &d.categories {
        let name = cat_name(ctx.lang, d, &c.slug);
        let _ = write!(
            h,
            "<li><a href=\"{}\">{}</a></li>",
            esc(&ctx.lpath(&format!("/community/c/{}", c.slug))),
            esc(&name)
        );
        let _ = writeln!(m, "- [{name}](/community/c/{})", c.slug);
    }
    h.push_str("</ul>");
    (h, m)
}

fn search_form(ctx: &Ctx, q: &str) -> String {
    format!(
        "<form role=\"search\" action=\"/community/search\" method=\"get\"><label for=\"q2\">{}</label><input id=\"q2\" type=\"search\" name=\"q\" value=\"{}\" maxlength=\"100\">{}<p><button type=\"submit\">{}</button></p></form>",
        esc(ctx.t("nav.search_ph")),
        esc(q),
        if ctx.lang == Lang::En {
            String::new()
        } else {
            format!(
                "<input type=\"hidden\" name=\"lang\" value=\"{}\">",
                ctx.lang.code()
            )
        },
        esc(ctx.t("nav.search"))
    )
}

impl Community {
    fn error_page(
        &self,
        ctx: &mut Ctx,
        status: u16,
        title_key: &str,
        text_key: &str,
        with_nav: bool,
    ) -> Resp {
        let title = ctx.t(title_key);
        let text = ctx.t(text_key);
        let mut p = Page::new(title, text, &ctx.req.path.clone());
        p.status = status;
        p.noindex = true;
        p.private = true;
        p.crumbs = vec![(ctx.t("nav.community").into(), "/community".into())];
        let mut body = format!("<h1>{}</h1><p>{}</p>", esc(title), esc(text));
        let mut md = format!("{text}\n");
        if with_nav {
            let d = self.forum.lock();
            let (list, lmd) = categories_list(ctx, &d);
            let _ = write!(
                body,
                "<h2>{}</h2>{}{}",
                esc(ctx.t("forum.categories")),
                list,
                search_form(ctx, "")
            );
            let _ = write!(md, "\n## {}\n\n{}", ctx.t("forum.categories"), lmd);
        }
        p.body = body;
        p.md = md;
        let mut r = render(ctx, p, Section::Forum);
        if status == 429 {
            r = r.header("retry-after", "600");
        }
        r
    }

    fn forbidden(&self, ctx: &mut Ctx) -> Resp {
        self.error_page(ctx, 403, "err.403_title", "err.403_text", false)
    }

    fn not_found(&self, ctx: &mut Ctx) -> Resp {
        self.error_page(ctx, 404, "err.404_title", "err.404_text", true)
    }

    fn ferr_page(&self, ctx: &mut Ctx, e: &FErr) -> Resp {
        match e {
            FErr::NotFound => self.not_found(ctx),
            FErr::Gone => self.error_page(ctx, 410, "forum.removed_title", "err.410_text", true),
            FErr::Forbidden => self.forbidden(ctx),
            FErr::Rate(_) => self.error_page(ctx, 429, "err.429_title", "err.429_text", false),
            FErr::Invalid(_) | FErr::Storage(_) => {
                self.error_page(ctx, 500, "err.400_title", "err.500", false)
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Routing
    // -----------------------------------------------------------------------------------------

    pub(crate) fn forum_route(&self, ctx: &mut Ctx) -> Resp {
        let path = ctx.req.path.clone();
        let method = ctx.req.method.clone();
        // `/community/` redirects to the canonical `/community`.
        if path == "/community/" {
            return Resp::redirect(301, &with_lang("/community", ctx.lang));
        }
        let r = route(&path);
        if matches!(r, Route::Unknown) {
            return self.not_found(ctx);
        }
        if !allowed(&r).contains(&method.as_str()) {
            let mut resp = self.error_page(ctx, 405, "err.405_title", "err.405_title", false);
            resp = resp.header("allow", &allowed(&r).join(", "));
            return resp;
        }
        let post = method == "POST";
        if post && !self.csrf_valid(ctx) {
            return self.forbidden(ctx);
        }
        match r {
            Route::Index => self.index(ctx),
            Route::Css => Resp::new(200, "text/css; charset=utf-8", crate::assets::CSS)
                .header("cache-control", "public, max-age=31536000, immutable")
                .conditional(ctx.req, None),
            Route::Sitemap => self.sitemap_response(ctx),
            Route::Feed(cat) => self.feed(ctx, cat),
            Route::Search => self.search(ctx),
            Route::Login => {
                if post {
                    self.login_post(ctx)
                } else {
                    self.login_get(ctx, None)
                }
            }
            Route::Signup => {
                if post {
                    self.signup_post(ctx)
                } else {
                    self.signup_get(ctx, None, "", "")
                }
            }
            Route::Logout => self.logout(ctx),
            Route::ModLog => self.mod_log(ctx),
            Route::ModPost => self.mod_post(ctx),
            Route::Category(slug) => self.category(ctx, slug),
            Route::NewThread(slug) => {
                if post {
                    self.new_thread_post(ctx, slug)
                } else {
                    self.new_thread_get(ctx, slug)
                }
            }
            Route::Thread(seg) => self.thread(ctx, seg),
            Route::Reply(seg) => {
                if post {
                    self.reply_post(ctx, seg)
                } else {
                    self.reply_get(ctx, seg)
                }
            }
            Route::Edit(id) => {
                if post {
                    self.edit_post(ctx, id)
                } else {
                    self.edit_get(ctx, id)
                }
            }
            Route::User(name) => self.profile(ctx, &decode(name)),
            Route::Unknown => self.not_found(ctx),
        }
    }

    /// Double-submit CSRF check: logged-in forms carry a token derived from the session;
    /// anonymous forms carry the value of the `obop_csrf` cookie. Cross-origin posts are refused.
    fn csrf_valid(&self, ctx: &Ctx) -> bool {
        if let Some(origin) = ctx.req.header("origin") {
            let host = ctx.req.header("host").unwrap_or("");
            let o_host = origin.split("://").nth(1).unwrap_or("");
            if origin != "null" && o_host != host {
                return false;
            }
            if origin == "null" {
                return false;
            }
        }
        let form = ctx.req.form();
        let submitted = form.get("csrf");
        let expected = match &ctx.session_raw {
            Some(raw) if ctx.user.is_some() => accounts::csrf_for_session(raw),
            _ => ctx.req.cookie(CSRF_COOKIE).unwrap_or_default(),
        };
        accounts::csrf_ok(&expected, submitted)
    }

    fn login_redirect(&self, ctx: &mut Ctx) -> Resp {
        let next = urlenc(&ctx.req.path);
        Resp::redirect(
            302,
            &with_lang(&format!("/community/login?next={next}"), ctx.lang),
        )
    }

    // -----------------------------------------------------------------------------------------
    // Reading pages
    // -----------------------------------------------------------------------------------------

    fn thread_item(&self, ctx: &Ctx, d: &Data, t: &Thread) -> (String, String) {
        let staff = ctx.staff();
        let removed = t.hidden && !staff;
        let title = if removed {
            ctx.t("forum.removed_title").to_string()
        } else {
            t.title.clone()
        };
        let href = ctx.lpath(&thread_path(t, !removed));
        let mut badges = String::new();
        let mut mdb = String::new();
        for (on, key) in [
            (t.pinned, "forum.pinned"),
            (t.locked, "forum.locked"),
            (t.hidden, "forum.hidden"),
        ] {
            if on {
                let _ = write!(badges, "<span class=\"badge\">{}</span>", esc(ctx.t(key)));
                let _ = write!(mdb, " [{}]", ctx.t(key));
            }
        }
        let name = d.display_name(&t.author);
        let replies = d.visible_replies(t.id);
        let h = format!(
            "<li><h3><a href=\"{}\">{}</a>{}</h3><div class=\"meta\"><span>{}</span><span>{}</span><span>{}</span></div></li>",
            esc(&href),
            esc(&title),
            badges,
            esc(&ctx.tf("forum.by", &[("name", &name)])),
            esc(&ctx.tf("forum.replies_n", &[("n", &ctx.num(replies as u64))])),
            esc(&ctx.tf("forum.latest", &[("date", &ctx.date(t.last_activity))])),
        );
        let m = format!(
            "- [{}]({}){} - {} - {} - {}\n",
            title,
            ctx.abs(&thread_path(t, !removed)),
            mdb,
            ctx.tf("forum.by", &[("name", &name)]),
            ctx.tf("forum.replies_n", &[("n", &ctx.num(replies as u64))]),
            ctx.tf("forum.latest", &[("date", &ctx.date(t.last_activity))]),
        );
        (h, m)
    }

    fn pager(&self, ctx: &Ctx, base: &str, page: usize, pages: usize) -> String {
        if pages <= 1 {
            return String::new();
        }
        let url = |p: usize| {
            if p == 1 {
                ctx.lpath(base)
            } else {
                let sep = if base.contains('?') { '&' } else { '?' };
                ctx.lpath(&format!("{base}{sep}page={p}"))
            }
        };
        let mut h = format!(
            "<nav class=\"pager\" aria-label=\"{}\"><ul>",
            esc(ctx.t("forum.pagination"))
        );
        if page > 1 {
            let _ = write!(
                h,
                "<li><a rel=\"prev\" href=\"{}\">{}</a></li>",
                esc(&url(page - 1)),
                esc(ctx.t("forum.prev"))
            );
        }
        let _ = write!(
            h,
            "<li aria-current=\"page\">{}</li>",
            esc(&ctx.tf(
                "forum.page_of",
                &[("p", &page.to_string()), ("n", &pages.to_string())]
            ))
        );
        if page < pages {
            let _ = write!(
                h,
                "<li><a rel=\"next\" href=\"{}\">{}</a></li>",
                esc(&url(page + 1)),
                esc(ctx.t("forum.next"))
            );
        }
        h.push_str("</ul></nav>");
        h
    }

    fn page_param(ctx: &Ctx) -> usize {
        ctx.req
            .q("page")
            .and_then(|p| p.trim().parse::<usize>().ok())
            .unwrap_or(1)
    }

    fn paged_path(base: &str, page: usize) -> String {
        if page <= 1 {
            base.to_string()
        } else {
            let sep = if base.contains('?') { '&' } else { '?' };
            format!("{base}{sep}page={page}")
        }
    }

    fn index(&self, ctx: &mut Ctx) -> Resp {
        let staff = ctx.staff();
        let d = self.forum.lock();
        let mut p = Page::new(
            &format!("{} | {}", ctx.t("forum.title"), ctx.t("nav.community")),
            ctx.t("forum.intro"),
            "/community",
        );
        p.title = ctx.t("forum.title").to_string();
        p.crumbs = vec![(ctx.t("nav.community").into(), "/community".into())];
        p.feeds = vec![(ctx.t("forum.feed_all").into(), "/community/feed.xml".into())];
        p.last_modified = Some(d.last_modified()).filter(|t| *t > 0);
        let mut b = format!(
            "<h1>{}</h1><p class=\"lede\">{}</p>",
            esc(ctx.t("forum.title")),
            esc(ctx.t("forum.intro"))
        );
        let mut m = format!("{}\n", ctx.t("forum.intro"));
        let pinned = d.pinned_announcements(staff);
        if !pinned.is_empty() {
            let _ = write!(
                b,
                "<h2>{}</h2><ul class=\"list\">",
                esc(ctx.t("forum.pinned_announcements"))
            );
            let _ = write!(m, "\n## {}\n\n", ctx.t("forum.pinned_announcements"));
            for t in pinned.iter().take(5) {
                let (h, mm) = self.thread_item(ctx, &d, t);
                b.push_str(&h);
                m.push_str(&mm);
            }
            b.push_str("</ul>");
        }
        let _ = write!(
            b,
            "<h2>{}</h2><ul class=\"list\">",
            esc(ctx.t("forum.categories"))
        );
        let _ = write!(m, "\n## {}\n\n", ctx.t("forum.categories"));
        let mut ld_items = Vec::new();
        for (i, c) in d.categories.iter().enumerate() {
            let (nt, np, latest) = store::category_stats(&d, &c.slug, staff);
            let name = cat_name(ctx.lang, &d, &c.slug);
            let desc = cat_desc(ctx.lang, &c.slug);
            let when = latest.map_or_else(
                || ctx.t("forum.no_activity").to_string(),
                |ts| ctx.tf("forum.latest", &[("date", &ctx.date(ts))]),
            );
            let _ = write!(
                b,
                "<li><h3><a href=\"{}\">{}</a></h3><p>{}</p><div class=\"meta\"><span>{}</span><span>{}</span><span>{}</span></div></li>",
                esc(&ctx.lpath(&format!("/community/c/{}", c.slug))),
                esc(&name),
                esc(&desc),
                esc(&ctx.tf("forum.threads_n", &[("n", &ctx.num(nt as u64))])),
                esc(&ctx.tf("forum.posts_n", &[("n", &ctx.num(np as u64))])),
                esc(&when),
            );
            let _ = writeln!(
                m,
                "- [{}]({}) - {} - {} - {} - {}",
                name,
                ctx.abs(&format!("/community/c/{}", c.slug)),
                desc,
                ctx.tf("forum.threads_n", &[("n", &ctx.num(nt as u64))]),
                ctx.tf("forum.posts_n", &[("n", &ctx.num(np as u64))]),
                when
            );
            ld_items.push(json!({"@type": "ListItem", "position": i + 1, "name": name, "url": ctx.abs(&format!("/community/c/{}", c.slug))}));
        }
        b.push_str("</ul>");
        let latest = d.latest_threads(10, staff);
        let _ = write!(b, "<h2>{}</h2>", esc(ctx.t("forum.recent")));
        let _ = write!(m, "\n## {}\n\n", ctx.t("forum.recent"));
        if latest.is_empty() {
            let _ = write!(b, "<p>{}</p>", esc(ctx.t("forum.no_threads")));
            let _ = writeln!(m, "{}", ctx.t("forum.no_threads"));
        } else {
            b.push_str("<ul class=\"list\">");
            for t in latest {
                let (h, mm) = self.thread_item(ctx, &d, t);
                b.push_str(&h);
                m.push_str(&mm);
            }
            b.push_str("</ul>");
        }
        p.jsonld = vec![
            json!({"@context": "https://schema.org", "@type": "CollectionPage", "name": ctx.t("forum.title"), "description": ctx.t("forum.intro"), "url": ctx.abs(&with_lang("/community", ctx.lang)), "inLanguage": ctx.lang.code(),
                "mainEntity": {"@type": "ItemList", "itemListElement": ld_items}}),
            json!({"@context": "https://schema.org", "@type": "WebSite", "name": "obop community", "url": ctx.abs("/community"),
                "potentialAction": {"@type": "SearchAction", "target": ctx.abs("/community/search?q={search_term_string}"), "query-input": "required name=search_term_string"}}),
        ];
        p.body = b;
        p.md = m;
        drop(d);
        render(ctx, p, Section::Forum)
    }

    fn category(&self, ctx: &mut Ctx, slug: &str) -> Resp {
        let staff = ctx.staff();
        let slug = decode(slug);
        let d = self.forum.lock();
        let Some(cat) = d.category(&slug).cloned() else {
            drop(d);
            return self.not_found(ctx);
        };
        let threads = d.category_threads(&slug, staff);
        let page = Self::page_param(ctx);
        let Some((off, pages)) = paginate(threads.len(), store::PAGE_THREADS, page) else {
            drop(d);
            return self.not_found(ctx);
        };
        let base = format!("/community/c/{slug}");
        let name = cat_name(ctx.lang, &d, &slug);
        let desc = cat_desc(ctx.lang, &slug);
        let mut title = name.clone();
        let mut description = desc.clone();
        if page > 1 {
            let pg = ctx.tf(
                "forum.page_of",
                &[("p", &page.to_string()), ("n", &pages.to_string())],
            );
            title = format!("{title} ({pg})");
            description = format!("{description} ({pg})");
        }
        let mut p = Page::new(&title, &description, &Self::paged_path(&base, page));
        p.crumbs = vec![
            (ctx.t("nav.community").into(), "/community".into()),
            (name.clone(), base.clone()),
        ];
        p.feeds = vec![(format!("{name} (Atom)"), format!("{base}/feed.xml"))];
        p.last_modified = Some(d.category_last_modified(&slug)).filter(|t| *t > 0);
        p.prev = (page > 1).then(|| Self::paged_path(&base, page - 1));
        p.next = (page < pages).then(|| Self::paged_path(&base, page + 1));
        let mut b = format!(
            "<h1>{}</h1><p class=\"lede\">{}</p>",
            esc(&name),
            esc(&desc)
        );
        let mut m = format!("{desc}\n\n");
        let can_post = ctx.user.is_some() && (!cat.staff_only || staff);
        if can_post {
            let _ = write!(
                b,
                "<p><a class=\"btn\" href=\"{}\">{}</a></p>",
                esc(&ctx.lpath(&format!("{base}/new"))),
                esc(ctx.t("forum.new_thread"))
            );
        } else if cat.staff_only {
            let _ = write!(
                b,
                "<p class=\"notice\">{}</p>",
                esc(ctx.t("forum.staff_only"))
            );
            let _ = writeln!(m, "{}\n", ctx.t("forum.staff_only"));
        } else {
            let _ = write!(
                b,
                "<p class=\"notice\">{} <a href=\"{}\">{}</a> · <a href=\"{}\">{}</a></p>",
                esc(ctx.t("forum.login_to_post")),
                esc(&ctx.lpath(&format!(
                    "/community/login?next={}",
                    urlenc(&format!("{base}/new"))
                ))),
                esc(ctx.t("nav.login")),
                esc(&ctx.lpath("/community/signup")),
                esc(ctx.t("nav.signup")),
            );
        }
        let slice: Vec<&&Thread> = threads.iter().skip(off).take(store::PAGE_THREADS).collect();
        let mut ld_items = Vec::new();
        if slice.is_empty() {
            let _ = write!(b, "<p>{}</p>", esc(ctx.t("forum.no_threads")));
            let _ = writeln!(m, "{}", ctx.t("forum.no_threads"));
        } else {
            b.push_str("<ul class=\"list\">");
            for (i, t) in slice.iter().enumerate() {
                let (h, mm) = self.thread_item(ctx, &d, t);
                b.push_str(&h);
                m.push_str(&mm);
                if !t.hidden {
                    ld_items.push(json!({"@type": "ListItem", "position": off + i + 1, "name": t.title, "url": ctx.abs(&thread_path(t, true))}));
                }
            }
            b.push_str("</ul>");
        }
        b.push_str(&self.pager(ctx, &base, page, pages));
        p.jsonld = vec![json!({
            "@context": "https://schema.org", "@type": "CollectionPage", "name": name, "description": desc,
            "url": ctx.abs(&with_lang(&Self::paged_path(&base, page), ctx.lang)), "inLanguage": ctx.lang.code(),
            "mainEntity": {"@type": "ItemList", "numberOfItems": threads.len(), "itemListElement": ld_items},
        })];
        p.body = b;
        p.md = m;
        drop(d);
        render(ctx, p, Section::Forum)
    }

    fn author_ld(&self, ctx: &Ctx, d: &Data, email: &str) -> J {
        let name = d.display_name(email);
        match d.members.get(email) {
            Some(_) => {
                json!({"@type": "Person", "name": name, "url": ctx.abs(&format!("/community/u/{}", urlenc(&name)))})
            }
            None => json!({"@type": "Person", "name": name}),
        }
    }

    fn thread(&self, ctx: &mut Ctx, seg: &str) -> Resp {
        let staff = ctx.staff();
        let seg = decode(seg);
        let Some((id, slug_part)) = parse_thread_seg(&seg) else {
            return self.not_found(ctx);
        };
        let d = self.forum.lock();
        let Some(t) = d.threads.get(&id).cloned() else {
            drop(d);
            return self.not_found(ctx);
        };
        if t.deleted {
            drop(d);
            return self.error_page(ctx, 410, "forum.removed_title", "err.410_text", true);
        }
        let removed_view = t.hidden && !staff;
        let canonical = thread_path(&t, !removed_view);
        if canonical != format!("/community/t/{id}-{slug_part}") {
            let q = if ctx.req.query.is_empty() {
                String::new()
            } else {
                format!("?{}", ctx.req.query)
            };
            drop(d);
            return Resp::redirect(301, &format!("{canonical}{q}"));
        }
        let page = Self::page_param(ctx);
        let posts: Vec<_> = d.thread_posts(id);
        let (off, pages) = if removed_view {
            (0, 1)
        } else {
            match paginate(posts.len(), store::PAGE_POSTS, page) {
                Some(x) => x,
                None => {
                    drop(d);
                    return self.not_found(ctx);
                }
            }
        };
        let cname = cat_name(ctx.lang, &d, &t.cat);
        let visible_replies = d.visible_replies(id);
        let title_text = if removed_view {
            ctx.t("forum.removed_title").to_string()
        } else {
            t.title.clone()
        };
        let first_body = posts.first().map(|p| p.body.clone()).unwrap_or_default();
        let desc = if removed_view {
            ctx.t("forum.removed").to_string()
        } else {
            let e = md::excerpt(&first_body, 160);
            if e.is_empty() { title_text.clone() } else { e }
        };
        let mut title = format!("{title_text} – {cname}");
        let mut description = desc.clone();
        if page > 1 {
            let pg = ctx.tf(
                "forum.page_of",
                &[("p", &page.to_string()), ("n", &pages.to_string())],
            );
            title = format!("{title} ({pg})");
            description = format!("{description} ({pg})");
        }
        let mut p = Page::new(
            &truncate_chars(&title, 110),
            &description,
            &Self::paged_path(&canonical, page),
        );
        p.noindex = t.hidden || (t.locked && visible_replies == 0);
        p.og_type = "article";
        p.crumbs = vec![
            (ctx.t("nav.community").into(), "/community".into()),
            (cname.clone(), format!("/community/c/{}", t.cat)),
            (truncate_chars(&title_text, 80), canonical.clone()),
        ];
        p.feeds = vec![(
            format!("{cname} (Atom)"),
            format!("/community/c/{}/feed.xml", t.cat),
        )];
        p.last_modified = Some(t.updated.max(t.last_activity));
        p.prev = (page > 1).then(|| Self::paged_path(&canonical, page - 1));
        p.next = (page < pages).then(|| Self::paged_path(&canonical, page + 1));
        let mut b = format!("<h1>{}", esc(&title_text));
        let mut m = format!(
            "Category: [{}]({})\n\n",
            cname,
            ctx.abs(&format!("/community/c/{}", t.cat))
        );
        for (on, key) in [
            (t.pinned, "forum.pinned"),
            (t.locked, "forum.locked"),
            (t.hidden, "forum.hidden"),
        ] {
            if on {
                let _ = write!(b, "<span class=\"badge\">{}</span>", esc(ctx.t(key)));
                let _ = write!(m, "[{}] ", ctx.t(key));
            }
        }
        let _ = write!(
            b,
            "</h1><p class=\"meta\"><a href=\"{}\">{}</a><span>{}</span><span>{}</span></p>",
            esc(&ctx.lpath(&format!("/community/c/{}", t.cat))),
            esc(&cname),
            esc(&ctx.tf("forum.by", &[("name", &d.display_name(&t.author))])),
            esc(&ctx.tf(
                "forum.replies_n",
                &[("n", &ctx.num(visible_replies as u64))]
            )),
        );
        if removed_view {
            let _ = write!(b, "<p class=\"notice\">{}</p>", esc(ctx.t("forum.removed")));
            let _ = writeln!(m, "{}", ctx.t("forum.removed"));
        } else {
            if staff {
                b.push_str(&self.mod_bar(ctx, &d, &t));
            }
            let mut comments = Vec::new();
            for (i, post) in posts.iter().enumerate().skip(off).take(store::PAGE_POSTS) {
                let (h, mm) = self.post_view(ctx, &d, post, i + 1, id);
                b.push_str(&h);
                m.push_str(&mm);
                if i > 0 && !post.hidden {
                    comments.push(json!({
                        "@type": "Comment", "@id": ctx.abs(&format!("{canonical}#post-{}", post.id)),
                        "url": ctx.abs(&format!("{canonical}#post-{}", post.id)),
                        "text": md::excerpt(&post.body, 500), "dateCreated": util::rfc3339(post.created),
                        "author": self.author_ld(ctx, &d, &post.author),
                    }));
                }
            }
            b.push_str(&self.pager(ctx, &canonical, page, pages));
            // Reply form, or the reason there is none.
            if t.locked && !staff {
                let _ = write!(
                    b,
                    "<p class=\"notice\">{}</p>",
                    esc(ctx.t("forum.locked_msg"))
                );
                let _ = writeln!(m, "\n{}", ctx.t("forum.locked_msg"));
            } else if ctx.user.is_some() {
                b.push_str(&reply_form(ctx, id, "", None));
            } else {
                let _ = write!(
                    b,
                    "<p class=\"notice\">{} <a href=\"{}\">{}</a> · <a href=\"{}\">{}</a></p>",
                    esc(ctx.t("forum.login_to_reply")),
                    esc(&ctx.lpath(&format!("/community/login?next={}", urlenc(&canonical)))),
                    esc(ctx.t("nav.login")),
                    esc(&ctx.lpath("/community/signup")),
                    esc(ctx.t("nav.signup")),
                );
                let _ = writeln!(m, "\n{}", ctx.t("forum.login_to_reply"));
            }
            let mut ld = json!({
                "@context": "https://schema.org", "@type": "DiscussionForumPosting",
                "@id": ctx.abs(&canonical), "url": ctx.abs(&canonical), "mainEntityOfPage": ctx.abs(&canonical),
                "headline": truncate_chars(&t.title, 110), "text": md::excerpt(&first_body, 500),
                "datePublished": util::rfc3339(t.created), "dateModified": util::rfc3339(t.updated.max(t.last_activity)),
                "author": self.author_ld(ctx, &d, &t.author), "articleSection": cname,
                "commentCount": visible_replies,
                "interactionStatistic": {"@type": "InteractionCounter", "interactionType": "https://schema.org/CommentAction", "userInteractionCount": visible_replies},
            });
            if !comments.is_empty() {
                ld["comment"] = J::Array(comments);
            }
            p.jsonld = vec![ld];
        }
        p.body = b;
        p.md = m;
        drop(d);
        render(ctx, p, Section::Forum)
    }

    fn post_view(
        &self,
        ctx: &Ctx,
        d: &Data,
        post: &store::Post,
        n: usize,
        tid: u64,
    ) -> (String, String) {
        let staff = ctx.staff();
        let name = d.display_name(&post.author);
        let author_staff = is_staff_name(self, d, &post.author);
        let hidden_for_viewer = post.hidden && !staff;
        let mut h = format!(
            "<article class=\"post\" id=\"post-{}\"><header><span>",
            post.id
        );
        let _ = write!(
            h,
            "<a href=\"{}\"><strong>{}</strong></a>",
            esc(&ctx.lpath(&format!("/community/u/{}", urlenc(&name)))),
            esc(&name)
        );
        if author_staff {
            let _ = write!(
                h,
                "<span class=\"badge staff\">{}</span>",
                esc(ctx.t("forum.staff"))
            );
        }
        if post.hidden {
            let _ = write!(
                h,
                "<span class=\"badge\">{}</span>",
                esc(ctx.t("forum.hidden"))
            );
        }
        let _ = write!(h, "</span><span>{}", ctx.time_html(post.created));
        if post.edited.is_some() {
            let _ = write!(h, " ({})", esc(ctx.t("forum.edited")));
        }
        let _ = write!(
            h,
            " · <a href=\"#post-{}\">{}</a></span></header>",
            post.id,
            esc(&ctx.tf("forum.post_n", &[("n", &n.to_string())]))
        );
        let mut m = format!(
            "### {} - {} - {}\n\n",
            ctx.tf("forum.post_n", &[("n", &n.to_string())]),
            name,
            ctx.datetime(post.created)
        );
        if hidden_for_viewer {
            let _ = write!(
                h,
                "<div class=\"body\"><p class=\"removed\">{}</p></div>",
                esc(ctx.t("forum.removed"))
            );
            let _ = writeln!(m, "_{}_\n", ctx.t("forum.removed"));
        } else {
            let _ = write!(h, "<div class=\"body\">{}</div>", md::render(&post.body));
            let _ = writeln!(m, "{}\n", post.body);
        }
        // Actions: edit within the window; hide/unhide for staff.
        let mut actions = String::new();
        if let Some(u) = &ctx.user {
            if u.email == post.author
                && !post.hidden
                && ctx.now.saturating_sub(post.created) <= store::EDIT_WINDOW_SECS
            {
                let _ = write!(
                    actions,
                    "<a href=\"{}\">{}</a>",
                    esc(&ctx.lpath(&format!("/community/p/{}/edit", post.id))),
                    esc(ctx.t("forum.edit"))
                );
            }
            if u.staff && n > 1 {
                let (act, key) = if post.hidden {
                    ("unhide", "mod.unhide")
                } else {
                    ("hide", "mod.hide")
                };
                actions.push_str(&mod_form(
                    ctx,
                    &format!("post:{}", post.id),
                    act,
                    "",
                    ctx.t(key),
                    &format!("/community/t/{tid}"),
                ));
            }
        }
        if !actions.is_empty() {
            let _ = write!(h, "<div class=\"actions\">{actions}</div>");
        }
        h.push_str("</article>");
        (h, m)
    }

    fn mod_bar(&self, ctx: &Ctx, d: &Data, t: &Thread) -> String {
        let target = format!("thread:{}", t.id);
        let ret = thread_path(t, true);
        let mut h = format!(
            "<div class=\"modbar panel\" role=\"group\" aria-label=\"{}\">",
            esc(ctx.t("mod.panel"))
        );
        let pairs = [
            (
                if t.pinned { "unpin" } else { "pin" },
                if t.pinned { "mod.unpin" } else { "mod.pin" },
            ),
            (
                if t.locked { "unlock" } else { "lock" },
                if t.locked { "mod.unlock" } else { "mod.lock" },
            ),
            (
                if t.hidden { "unhide" } else { "hide" },
                if t.hidden { "mod.unhide" } else { "mod.hide" },
            ),
            ("delete", "mod.delete"),
        ];
        for (act, key) in pairs {
            h.push_str(&mod_form(ctx, &target, act, "", ctx.t(key), &ret));
        }
        let _ = write!(
            h,
            "<form action=\"/community/mod\" method=\"post\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><input type=\"hidden\" name=\"target\" value=\"{}\"><input type=\"hidden\" name=\"action\" value=\"move\"><input type=\"hidden\" name=\"return\" value=\"{}\"><label class=\"sr\" for=\"mv\">{}</label><select id=\"mv\" name=\"arg\">",
            esc(&ctx.csrf),
            esc(&target),
            esc(&ret),
            esc(ctx.t("mod.move"))
        );
        for c in &d.categories {
            let _ = write!(
                h,
                "<option value=\"{}\"{}>{}</option>",
                esc(&c.slug),
                if c.slug == t.cat { " selected" } else { "" },
                esc(&cat_name(ctx.lang, d, &c.slug))
            );
        }
        let _ = write!(
            h,
            "</select><button type=\"submit\" class=\"secondary\">{}</button></form></div>",
            esc(ctx.t("mod.move"))
        );
        h
    }

    fn profile(&self, ctx: &mut Ctx, name: &str) -> Resp {
        let staff = ctx.staff();
        let d = self.forum.lock();
        let Some(m) = d.member_by_name(name).cloned() else {
            drop(d);
            return self.not_found(ctx);
        };
        let path = format!("/community/u/{}", urlenc(&m.name));
        let posts = d.post_count_by(&m.email);
        let threads = d.threads_by(&m.email, 10, staff);
        let is_staff = self.forum.is_staff(&m.email);
        let title = ctx.tf("forum.profile", &[("name", &m.name)]);
        let desc = format!(
            "{} · {} · {}",
            title,
            ctx.tf("forum.posts_n", &[("n", &ctx.num(posts as u64))]),
            ctx.tf("forum.joined", &[("date", &ctx.date(m.joined))])
        );
        let mut p = Page::new(&title, &desc, &path);
        p.crumbs = vec![
            (ctx.t("nav.community").into(), "/community".into()),
            (m.name.clone(), path.clone()),
        ];
        let mut b = format!("<h1>{}", esc(&m.name));
        if is_staff {
            let _ = write!(
                b,
                "<span class=\"badge staff\">{}</span>",
                esc(ctx.t("forum.staff"))
            );
        }
        let _ = write!(
            b,
            "</h1><p class=\"meta\"><span>{}</span><span>{}</span></p><h2>{}</h2>",
            esc(&ctx.tf("forum.joined", &[("date", &ctx.date(m.joined))])),
            esc(&ctx.tf("forum.posts_n", &[("n", &ctx.num(posts as u64))])),
            esc(&ctx.tf("forum.recent_by", &[("name", &m.name)])),
        );
        let mut md = format!(
            "{}\n\n- {}\n- {}\n\n## {}\n\n",
            m.name,
            ctx.tf("forum.joined", &[("date", &ctx.date(m.joined))]),
            ctx.tf("forum.posts_n", &[("n", &ctx.num(posts as u64))]),
            ctx.tf("forum.recent_by", &[("name", &m.name)])
        );
        if threads.is_empty() {
            let _ = write!(b, "<p>{}</p>", esc(ctx.t("forum.no_threads")));
        } else {
            b.push_str("<ul class=\"list\">");
            for t in threads {
                let (h, mm) = self.thread_item(ctx, &d, t);
                b.push_str(&h);
                md.push_str(&mm);
            }
            b.push_str("</ul>");
        }
        p.jsonld = vec![json!({
            "@context": "https://schema.org", "@type": "ProfilePage", "url": ctx.abs(&path),
            "dateCreated": util::rfc3339(m.joined),
            "mainEntity": {"@type": "Person", "name": m.name, "url": ctx.abs(&path)},
        })];
        p.body = b;
        p.md = md;
        drop(d);
        render(ctx, p, Section::Forum)
    }

    fn search(&self, ctx: &mut Ctx) -> Resp {
        let staff = ctx.staff();
        let q: String = util::clean_line(&ctx.req.q("q").unwrap_or_default())
            .chars()
            .take(100)
            .collect();
        let base = format!("/community/search?q={}", urlenc(&q));
        let title = if q.is_empty() {
            ctx.t("forum.search_h").to_string()
        } else {
            ctx.tf("forum.search_title", &[("q", &q)])
        };
        let page = Self::page_param(ctx);
        let d = self.forum.lock();
        let hits = d.search(&q, staff);
        let Some((off, pages)) = paginate(hits.len(), store::PAGE_SEARCH, page) else {
            drop(d);
            return self.not_found(ctx);
        };
        let mut p = Page::new(
            &title,
            ctx.t("nav.search_ph"),
            &Self::paged_path(&base, page),
        );
        p.noindex = true;
        p.crumbs = vec![
            (ctx.t("nav.community").into(), "/community".into()),
            (ctx.t("forum.search_h").into(), "/community/search".into()),
        ];
        p.prev = (page > 1).then(|| Self::paged_path(&base, page - 1));
        p.next = (page < pages).then(|| Self::paged_path(&base, page + 1));
        let mut b = format!("<h1>{}</h1>{}", esc(&title), search_form(ctx, &q));
        let mut m = format!("{title}\n\n");
        if !q.is_empty() {
            if hits.is_empty() {
                let _ = write!(b, "<p>{}</p>", esc(ctx.t("forum.no_results")));
                let _ = writeln!(m, "{}", ctx.t("forum.no_results"));
            } else {
                let _ = write!(
                    b,
                    "<p class=\"meta\">{}</p><ul class=\"list\">",
                    esc(&ctx.tf("forum.results_n", &[("n", &ctx.num(hits.len() as u64))]))
                );
                for (id, snippet) in hits.iter().skip(off).take(store::PAGE_SEARCH) {
                    let Some(t) = d.threads.get(id) else { continue };
                    let removed = t.hidden && !staff;
                    let ttl = if removed {
                        ctx.t("forum.removed_title").to_string()
                    } else {
                        t.title.clone()
                    };
                    let href = thread_path(t, !removed);
                    let _ = write!(
                        b,
                        "<li><h3><a href=\"{}\">{}</a></h3><p>{}</p><div class=\"meta\"><span>{}</span><span>{}</span></div></li>",
                        esc(&ctx.lpath(&href)),
                        esc(&ttl),
                        esc(snippet),
                        esc(&cat_name(ctx.lang, &d, &t.cat)),
                        esc(&ctx.tf("forum.latest", &[("date", &ctx.date(t.last_activity))])),
                    );
                    let _ = writeln!(m, "- [{}]({}) - {}", ttl, ctx.abs(&href), snippet);
                }
                b.push_str("</ul>");
                b.push_str(&self.pager(ctx, &base, page, pages));
            }
        }
        p.body = b;
        p.md = m;
        drop(d);
        render(ctx, p, Section::Forum)
    }

    // -----------------------------------------------------------------------------------------
    // Feeds and sitemap
    // -----------------------------------------------------------------------------------------

    fn feed(&self, ctx: &mut Ctx, cat: Option<&str>) -> Resp {
        let d = self.forum.lock();
        let slug = cat.map(decode);
        if let Some(s) = &slug
            && d.category(s).is_none()
        {
            drop(d);
            return self.not_found(ctx);
        }
        let threads: Vec<&Thread> = match &slug {
            Some(s) => d
                .category_threads(s, false)
                .into_iter()
                .filter(|t| !t.hidden)
                .collect(),
            None => d.latest_threads(30, false),
        };
        let mut threads = threads;
        threads.sort_by_key(|t| std::cmp::Reverse(t.last_activity));
        threads.truncate(30);
        let (self_path, title) = match &slug {
            Some(s) => (
                format!("/community/c/{s}/feed.xml"),
                format!("obop community – {}", cat_name(ctx.lang, &d, s)),
            ),
            None => (
                "/community/feed.xml".to_string(),
                "obop community".to_string(),
            ),
        };
        let updated = threads.iter().map(|t| t.last_activity).max().unwrap_or(0);
        let mut x = String::from(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<feed xmlns=\"http://www.w3.org/2005/Atom\">\n",
        );
        let _ = writeln!(x, "<title>{}</title>", esc(&title));
        let _ = writeln!(x, "<id>{}</id>", esc(&ctx.abs(&self_path)));
        let _ = writeln!(
            x,
            "<link rel=\"self\" type=\"application/atom+xml\" href=\"{}\"/>",
            esc(&ctx.abs(&self_path))
        );
        let _ = writeln!(
            x,
            "<link rel=\"alternate\" type=\"text/html\" href=\"{}\"/>",
            esc(&ctx.abs(
                slug.as_ref()
                    .map_or("/community".to_string(), |s| format!("/community/c/{s}"))
                    .as_str()
            ))
        );
        let _ = writeln!(x, "<updated>{}</updated>", util::rfc3339(updated));
        for t in threads {
            let url = ctx.abs(&thread_path(t, true));
            let summary = d
                .thread_posts(t.id)
                .first()
                .map(|p| md::excerpt(&p.body, 280))
                .unwrap_or_default();
            let _ = writeln!(
                x,
                "<entry><title>{}</title><id>{}</id><link rel=\"alternate\" type=\"text/html\" href=\"{}\"/><published>{}</published><updated>{}</updated><author><name>{}</name></author><category term=\"{}\"/><summary>{}</summary></entry>",
                esc(&t.title),
                esc(&url),
                esc(&url),
                util::rfc3339(t.created),
                util::rfc3339(t.last_activity),
                esc(&d.display_name(&t.author)),
                esc(&t.cat),
                esc(&summary),
            );
        }
        x.push_str("</feed>\n");
        drop(d);
        Resp::new(200, "application/atom+xml; charset=utf-8", x)
            .header("cache-control", "public, max-age=300")
            .header("x-content-type-options", "nosniff")
            .conditional(ctx.req, Some(updated).filter(|u| *u > 0))
    }

    /// Public URLs of the forum for the sitemap: the index, categories and every indexable
    /// thread (first page). The static site's sitemap index should list `/community/sitemap.xml`;
    /// this is also what that file contains.
    pub fn forum_sitemap_urls(&self, base: &str) -> Vec<SitemapUrl> {
        let d = self.forum.lock();
        let mut v = vec![SitemapUrl {
            loc: format!("{base}/community"),
            lastmod: Some(d.last_modified()).filter(|t| *t > 0),
        }];
        for c in &d.categories {
            v.push(SitemapUrl {
                loc: format!("{base}/community/c/{}", c.slug),
                lastmod: Some(d.category_last_modified(&c.slug)).filter(|t| *t > 0),
            });
        }
        for t in d.threads.values() {
            if t.deleted || t.hidden || (t.locked && d.visible_replies(t.id) == 0) {
                continue;
            }
            v.push(SitemapUrl {
                loc: format!("{base}{}", thread_path(t, true)),
                lastmod: Some(t.updated.max(t.last_activity)),
            });
        }
        v
    }

    fn sitemap_response(&self, ctx: &mut Ctx) -> Resp {
        let urls = self.forum_sitemap_urls(&ctx.base);
        let lm = urls.iter().filter_map(|u| u.lastmod).max();
        Resp::new(200, "application/xml; charset=utf-8", sitemap_xml(&urls))
            .header("cache-control", "public, max-age=600")
            .conditional(ctx.req, lm)
    }

    // -----------------------------------------------------------------------------------------
    // Accounts
    // -----------------------------------------------------------------------------------------

    fn auth_form_ctx(&self, ctx: &mut Ctx) {
        // Anonymous forms need a CSRF cookie to compare against.
        if ctx.user.is_none() && ctx.csrf.is_empty() {
            let tok = util::random_token();
            ctx.set_cookies
                .push(cookie(CSRF_COOKIE, &tok, 4 * 3600, ctx.req.is_https()));
            ctx.csrf = tok;
        }
    }

    fn login_get(&self, ctx: &mut Ctx, error: Option<&str>) -> Resp {
        self.auth_form_ctx(ctx);
        let next = ctx
            .req
            .q("next")
            .and_then(|n| safe_next(&n))
            .unwrap_or_else(|| ctx.req.form().get("next").to_string());
        let next = safe_next(&next).unwrap_or_default();
        let email = ctx.req.form().get("email").to_string();
        let mut p = Page::new(
            ctx.t("auth.login_title"),
            ctx.t("auth.shared"),
            "/community/login",
        );
        p.noindex = true;
        p.private = true;
        p.crumbs = vec![
            (ctx.t("nav.community").into(), "/community".into()),
            (ctx.t("auth.login_title").into(), "/community/login".into()),
        ];
        let mut b = format!(
            "<h1>{}</h1><p class=\"lede\">{}</p>",
            esc(ctx.t("auth.login_title")),
            esc(ctx.t("auth.shared"))
        );
        if let Some(e) = error {
            let _ = write!(b, "<div class=\"alert\" role=\"alert\">{}</div>", esc(e));
        }
        let _ = write!(
            b,
            "<form action=\"{}\" method=\"post\" novalidate><input type=\"hidden\" name=\"csrf\" value=\"{}\"><input type=\"hidden\" name=\"next\" value=\"{}\">{}\
<label for=\"email\">{}</label><input id=\"email\" name=\"email\" type=\"email\" autocomplete=\"username\" required maxlength=\"120\" value=\"{}\">\
<label for=\"password\">{}</label><input id=\"password\" name=\"password\" type=\"password\" autocomplete=\"current-password\" required maxlength=\"200\">\
<p><button type=\"submit\">{}</button></p></form><p>{} <a href=\"{}\">{}</a></p>",
            esc(&ctx.lpath("/community/login")),
            esc(&ctx.csrf),
            esc(&next),
            honeypot(),
            esc(ctx.t("auth.email")),
            esc(&email),
            esc(ctx.t("auth.password")),
            esc(ctx.t("nav.login")),
            esc(ctx.t("auth.no_account")),
            esc(&ctx.lpath("/community/signup")),
            esc(ctx.t("nav.signup")),
        );
        p.body = b;
        p.md = format!(
            "{}\n\n{}\n",
            ctx.t("auth.login_title"),
            ctx.t("auth.shared")
        );
        let status = if error.is_some() { 400 } else { 200 };
        p.status = status;
        render(ctx, p, Section::Forum)
    }

    fn signup_get(&self, ctx: &mut Ctx, error: Option<&str>, email: &str, name: &str) -> Resp {
        self.auth_form_ctx(ctx);
        let next = ctx
            .req
            .q("next")
            .and_then(|n| safe_next(&n))
            .unwrap_or_else(|| safe_next(ctx.req.form().get("next")).unwrap_or_default());
        let mut p = Page::new(
            ctx.t("auth.signup_title"),
            ctx.t("auth.shared"),
            "/community/signup",
        );
        p.noindex = true;
        p.private = true;
        p.crumbs = vec![
            (ctx.t("nav.community").into(), "/community".into()),
            (
                ctx.t("auth.signup_title").into(),
                "/community/signup".into(),
            ),
        ];
        let mut b = format!(
            "<h1>{}</h1><p class=\"lede\">{}</p>",
            esc(ctx.t("auth.signup_title")),
            esc(ctx.t("auth.shared"))
        );
        if let Some(e) = error {
            let _ = write!(b, "<div class=\"alert\" role=\"alert\">{}</div>", esc(e));
        }
        let _ = write!(
            b,
            "<form action=\"{}\" method=\"post\" novalidate><input type=\"hidden\" name=\"csrf\" value=\"{}\"><input type=\"hidden\" name=\"next\" value=\"{}\">{}\
<label for=\"email\">{}</label><input id=\"email\" name=\"email\" type=\"email\" autocomplete=\"email\" required maxlength=\"120\" value=\"{}\">\
<label for=\"display\">{}</label><input id=\"display\" name=\"display\" type=\"text\" autocomplete=\"nickname\" maxlength=\"24\" aria-describedby=\"display-help\" value=\"{}\"><p class=\"help\" id=\"display-help\">{}</p>\
<label for=\"password\">{}</label><input id=\"password\" name=\"password\" type=\"password\" autocomplete=\"new-password\" required minlength=\"12\" maxlength=\"200\" aria-describedby=\"pw-help\"><p class=\"help\" id=\"pw-help\">{}</p>\
<p><button type=\"submit\">{}</button></p></form><p>{} <a href=\"{}\">{}</a></p>",
            esc(&ctx.lpath("/community/signup")),
            esc(&ctx.csrf),
            esc(&next),
            honeypot(),
            esc(ctx.t("auth.email")),
            esc(email),
            esc(ctx.t("auth.display_name")),
            esc(name),
            esc(ctx.t("auth.display_help")),
            esc(ctx.t("auth.password")),
            esc(ctx.t("auth.password_help")),
            esc(ctx.t("nav.signup")),
            esc(ctx.t("auth.have_account")),
            esc(&ctx.lpath("/community/login")),
            esc(ctx.t("nav.login")),
        );
        p.body = b;
        p.md = format!(
            "{}\n\n{}\n",
            ctx.t("auth.signup_title"),
            ctx.t("auth.shared")
        );
        p.status = if error.is_some() { 400 } else { 200 };
        render(ctx, p, Section::Forum)
    }

    fn start_session(&self, ctx: &mut Ctx, email: &str) -> Result<(), String> {
        self.sessions.purge(ctx.now);
        let raw = self.sessions.create(email, ctx.now)?;
        ctx.set_cookies.push(cookie(
            SESSION_COOKIE,
            &raw,
            SESSION_SECS,
            ctx.req.is_https(),
        ));
        ctx.set_cookies
            .push(expire_cookie(CSRF_COOKIE, ctx.req.is_https()));
        Ok(())
    }

    fn after_login(&self, ctx: &mut Ctx, next: &str) -> Resp {
        let to = safe_next(next).unwrap_or_else(|| "/community".into());
        let mut r = Resp::redirect(303, &to);
        for c in &ctx.set_cookies {
            r = r.add_header("set-cookie", c);
        }
        layout_headers(r)
    }

    fn login_post(&self, ctx: &mut Ctx) -> Resp {
        let form = ctx.req.form();
        if !form.get("website").is_empty() {
            return self.error_page(ctx, 400, "err.400_title", "err.rejected", false);
        }
        let email = form.get("email").trim().to_ascii_lowercase();
        let next = form.get("next").to_string();
        if email.len() > NAME_MAX || form.get("password").len() > 200 {
            return self.login_get(ctx, Some(ctx.t("err.invalid_login")));
        }
        match self.auth.login(&email, form.get("password")) {
            Ok(tok) => {
                // `Auth` issues a bearer token; the forum keeps its own cookie session instead.
                self.auth.revoke(&tok);
                if let Err(e) = self.forum.register(&email, None, ctx.now) {
                    return self.ferr_page(ctx, &e);
                }
                if self.start_session(ctx, &email).is_err() {
                    return self.error_page(ctx, 500, "err.400_title", "err.500", false);
                }
                self.after_login(ctx, &next)
            }
            Err(e) if e.contains("too many") => self
                .login_get(ctx, Some(ctx.t("err.too_many_logins")))
                .tap_status(429),
            Err(_) => self
                .login_get(ctx, Some(ctx.t("err.invalid_login")))
                .tap_status(401),
        }
    }

    fn signup_post(&self, ctx: &mut Ctx) -> Resp {
        let form = ctx.req.form();
        if !form.get("website").is_empty() {
            return self.error_page(ctx, 400, "err.400_title", "err.rejected", false);
        }
        let email = form.get("email").trim().to_ascii_lowercase();
        let display = form.get("display").trim().to_string();
        let password = form.get("password").to_string();
        let next = form.get("next").to_string();
        let again =
            |ctx: &mut Ctx, key: &str| self.signup_get(ctx, Some(ctx.t(key)), &email, &display);
        if !store::valid_email(&email) {
            return again(ctx, "err.email_invalid");
        }
        if password.chars().count() < 12 {
            return again(ctx, "err.password_short");
        }
        if password.len() > 200 {
            return again(ctx, "err.rejected");
        }
        if !display.is_empty() {
            let staff = self.forum.is_staff(&email);
            if !store::valid_name_shape(&display) || (!staff && store::is_reserved_name(&display)) {
                return again(ctx, "err.name_invalid");
            }
            if !self.forum.name_available(&display) {
                return again(ctx, "err.name_taken");
            }
        }
        if let Err(e) = self.auth.create_user(&email, &password, &[]) {
            let key = if e.contains("exists") {
                "err.account_exists"
            } else if e.contains("12 characters") {
                "err.password_short"
            } else {
                "err.email_invalid"
            };
            return again(ctx, key);
        }
        let name = if display.is_empty() {
            None
        } else {
            Some(display.as_str())
        };
        match self.forum.register(&email, name, ctx.now) {
            Ok(_) => {}
            Err(FErr::Invalid(k)) => {
                // Lost a race for the name: keep the account, fall back to a derived name.
                let _ = k;
                if let Err(e) = self.forum.register(&email, None, ctx.now) {
                    return self.ferr_page(ctx, &e);
                }
            }
            Err(e) => return self.ferr_page(ctx, &e),
        }
        if self.start_session(ctx, &email).is_err() {
            return self.error_page(ctx, 500, "err.400_title", "err.500", false);
        }
        self.after_login(ctx, &next)
    }

    fn logout(&self, ctx: &mut Ctx) -> Resp {
        if let Some(raw) = ctx.req.cookie(SESSION_COOKIE) {
            self.sessions.revoke(&raw);
        }
        let mut r = Resp::redirect(303, &with_lang("/community", ctx.lang));
        r = r.add_header(
            "set-cookie",
            &expire_cookie(SESSION_COOKIE, ctx.req.is_https()),
        );
        layout_headers(r)
    }

    // -----------------------------------------------------------------------------------------
    // Writing
    // -----------------------------------------------------------------------------------------

    fn current(&self, ctx: &Ctx) -> Option<User> {
        ctx.user.clone()
    }

    fn compose_page(&self, ctx: &mut Ctx, kind: Compose, error: Option<&str>) -> Resp {
        let (title_key, action, back_path) = match &kind {
            Compose::Thread { slug, .. } => (
                "forum.new_thread",
                format!("/community/c/{slug}/new"),
                format!("/community/c/{slug}"),
            ),
            Compose::Reply { tid, .. } => (
                "forum.reply",
                format!("/community/t/{tid}/reply"),
                format!("/community/t/{tid}"),
            ),
            Compose::Edit { pid, tid, .. } => (
                "forum.edit_post",
                format!("/community/p/{pid}/edit"),
                format!("/community/t/{tid}"),
            ),
        };
        let heading = ctx.t(title_key);
        let mut p = Page::new(heading, ctx.t("forum.md_help"), &action);
        p.noindex = true;
        p.private = true;
        p.crumbs = vec![
            (ctx.t("nav.community").into(), "/community".into()),
            (heading.into(), action.clone()),
        ];
        let mut b = format!("<h1>{}</h1>", esc(heading));
        if let Some(e) = error {
            let _ = write!(
                b,
                "<div class=\"alert\" role=\"alert\" id=\"form-error\">{}</div>",
                esc(e)
            );
        }
        let (title_val, body_val, cat_sel) = match &kind {
            Compose::Thread { title, body, .. } => (Some(title.clone()), body.clone(), None),
            Compose::Reply { body, .. } | Compose::Edit { body, .. } => {
                (None, body.clone(), None::<String>)
            }
        };
        let _ = cat_sel;
        b.push_str(&compose_form(
            ctx,
            &action,
            title_val.as_deref(),
            &body_val,
            &back_path,
            matches!(kind, Compose::Edit { .. }),
        ));
        p.body = b;
        p.md = format!("{heading}\n\n{}\n", ctx.t("forum.md_help"));
        p.status = if error.is_some() { 400 } else { 200 };
        render(ctx, p, Section::Forum)
    }

    fn write_error(&self, ctx: &mut Ctx, e: FErr, kind: Compose) -> Resp {
        match e {
            FErr::Invalid(k) => self.compose_page(ctx, kind, Some(ctx.t(k))),
            FErr::Rate(k) => {
                let msg = ctx.t(k);
                let mut r = self.compose_page(ctx, kind, Some(msg));
                r.status = 429;
                r.header("retry-after", "600")
            }
            other => self.ferr_page(ctx, &other),
        }
    }

    fn new_thread_get(&self, ctx: &mut Ctx, slug: &str) -> Resp {
        let slug = decode(slug);
        if self.current(ctx).is_none() {
            return self.login_redirect(ctx);
        }
        let (exists, staff_only) = {
            let d = self.forum.lock();
            (
                d.category(&slug).is_some(),
                d.category(&slug).is_some_and(|c| c.staff_only),
            )
        };
        if !exists {
            return self.not_found(ctx);
        }
        if staff_only && !ctx.staff() {
            return self.forbidden(ctx);
        }
        self.compose_page(
            ctx,
            Compose::Thread {
                slug,
                title: String::new(),
                body: String::new(),
            },
            None,
        )
    }

    fn honeypot_tripped(ctx: &Ctx) -> bool {
        !ctx.req.form().get("website").is_empty()
    }

    fn new_thread_post(&self, ctx: &mut Ctx, slug: &str) -> Resp {
        let slug = decode(slug);
        let Some(u) = self.current(ctx) else {
            return self.forbidden(ctx);
        };
        if Self::honeypot_tripped(ctx) {
            return self.error_page(ctx, 400, "err.400_title", "err.rejected", false);
        }
        if self.forum.lock().category(&slug).is_none() {
            return self.not_found(ctx);
        }
        let f = ctx.req.form();
        let (title, body) = (f.get("title").to_string(), f.get("body").to_string());
        let kind = Compose::Thread {
            slug: slug.clone(),
            title: title.clone(),
            body: body.clone(),
        };
        match self
            .forum
            .create_thread(&u.email, &slug, &title, &body, ctx.now)
        {
            Ok(id) => {
                let d = self.forum.lock();
                let path = d
                    .threads
                    .get(&id)
                    .map(|t| thread_path(t, true))
                    .unwrap_or_else(|| "/community".into());
                drop(d);
                layout_headers(Resp::redirect(303, &with_lang(&path, ctx.lang)))
            }
            Err(FErr::NotFound) => self.not_found(ctx),
            Err(e) => self.write_error(ctx, e, kind),
        }
    }

    fn thread_id_of(seg: &str) -> Option<u64> {
        parse_thread_seg(&decode(seg)).map(|x| x.0)
    }

    fn reply_get(&self, ctx: &mut Ctx, seg: &str) -> Resp {
        let Some(tid) = Self::thread_id_of(seg) else {
            return self.not_found(ctx);
        };
        if self.current(ctx).is_none() {
            return self.login_redirect(ctx);
        }
        if !self.forum.lock().threads.contains_key(&tid) {
            return self.not_found(ctx);
        }
        self.compose_page(
            ctx,
            Compose::Reply {
                tid,
                body: String::new(),
            },
            None,
        )
    }

    fn reply_post(&self, ctx: &mut Ctx, seg: &str) -> Resp {
        let Some(tid) = Self::thread_id_of(seg) else {
            return self.not_found(ctx);
        };
        let Some(u) = self.current(ctx) else {
            return self.forbidden(ctx);
        };
        if Self::honeypot_tripped(ctx) {
            return self.error_page(ctx, 400, "err.400_title", "err.rejected", false);
        }
        let body = ctx.req.form().get("body").to_string();
        match self.forum.reply(&u.email, tid, &body, ctx.now) {
            Ok(pid) => {
                let d = self.forum.lock();
                let path = d
                    .threads
                    .get(&tid)
                    .map(|t| thread_path(t, true))
                    .unwrap_or_else(|| "/community".into());
                let total = d.thread_posts(tid).len();
                drop(d);
                let page = total.div_ceil(store::PAGE_POSTS).max(1);
                let loc = format!(
                    "{}#post-{pid}",
                    with_lang(&Self::paged_path(&path, page), ctx.lang)
                );
                layout_headers(Resp::redirect(303, &loc))
            }
            Err(e) => self.write_error(ctx, e, Compose::Reply { tid, body }),
        }
    }

    fn edit_target(&self, ctx: &Ctx, id: &str) -> Result<(u64, u64, String), FErr> {
        let pid: u64 = id.parse().map_err(|_| FErr::NotFound)?;
        let u = self.current(ctx).ok_or(FErr::Forbidden)?;
        let d = self.forum.lock();
        let p = d.posts.get(&pid).ok_or(FErr::NotFound)?;
        let t = d.threads.get(&p.thread).ok_or(FErr::NotFound)?;
        if t.deleted {
            return Err(FErr::Gone);
        }
        if p.author != u.email || p.hidden || t.hidden {
            return Err(FErr::Forbidden);
        }
        Ok((pid, p.thread, p.body.clone()))
    }

    fn edit_get(&self, ctx: &mut Ctx, id: &str) -> Resp {
        if self.current(ctx).is_none() {
            return self.login_redirect(ctx);
        }
        match self.edit_target(ctx, id) {
            Ok((pid, tid, body)) => self.compose_page(ctx, Compose::Edit { pid, tid, body }, None),
            Err(e) => self.ferr_page(ctx, &e),
        }
    }

    fn edit_post(&self, ctx: &mut Ctx, id: &str) -> Resp {
        let (pid, tid, _) = match self.edit_target(ctx, id) {
            Ok(x) => x,
            Err(e) => return self.ferr_page(ctx, &e),
        };
        let Some(u) = self.current(ctx) else {
            return self.forbidden(ctx);
        };
        let body = ctx.req.form().get("body").to_string();
        match self.forum.edit_post(&u.email, pid, &body, ctx.now) {
            Ok(_) => {
                let d = self.forum.lock();
                let path = d
                    .threads
                    .get(&tid)
                    .map(|t| thread_path(t, true))
                    .unwrap_or_else(|| "/community".into());
                drop(d);
                layout_headers(Resp::redirect(
                    303,
                    &format!("{}#post-{pid}", with_lang(&path, ctx.lang)),
                ))
            }
            Err(e) => self.write_error(ctx, e, Compose::Edit { pid, tid, body }),
        }
    }

    // -----------------------------------------------------------------------------------------
    // Moderation
    // -----------------------------------------------------------------------------------------

    fn mod_post(&self, ctx: &mut Ctx) -> Resp {
        let Some(u) = self.current(ctx) else {
            return self.forbidden(ctx);
        };
        if !u.staff {
            return self.forbidden(ctx);
        }
        let f = ctx.req.form();
        let target = f.get("target").to_string();
        let Some(action) = ModAction::parse(f.get("action"), f.get("arg")) else {
            return self.forbidden(ctx);
        };
        let ret = safe_next(f.get("return")).unwrap_or_else(|| "/community".into());
        let res = match target.split_once(':') {
            Some(("thread", id)) => id
                .parse::<u64>()
                .map_err(|_| FErr::NotFound)
                .and_then(|id| {
                    self.forum
                        .moderate_thread(&u.email, id, action.clone(), ctx.now)
                }),
            Some(("post", id)) => {
                let hide = matches!(action, ModAction::Hide);
                if !matches!(action, ModAction::Hide | ModAction::Unhide) {
                    Err(FErr::Forbidden)
                } else {
                    id.parse::<u64>()
                        .map_err(|_| FErr::NotFound)
                        .and_then(|id| {
                            self.forum
                                .moderate_post(&u.email, id, hide, ctx.now)
                                .map(|_| ())
                        })
                }
            }
            _ => Err(FErr::NotFound),
        };
        match res {
            Ok(()) => {
                // A moved thread keeps its URL; a deleted one has none, so go to the category.
                let dest = if action == ModAction::Delete {
                    "/community".to_string()
                } else {
                    ret
                };
                layout_headers(Resp::redirect(303, &with_lang(&dest, ctx.lang)))
            }
            Err(FErr::Invalid(k)) => self.error_page(ctx, 400, "err.400_title", k, false),
            Err(e) => self.ferr_page(ctx, &e),
        }
    }

    fn mod_log(&self, ctx: &mut Ctx) -> Resp {
        if ctx.user.is_none() {
            return self.login_redirect(ctx);
        }
        if !ctx.staff() {
            return self.forbidden(ctx);
        }
        let d = self.forum.lock();
        let title = ctx.t("nav.mod_log");
        let mut p = Page::new(title, title, "/community/mod/log");
        p.noindex = true;
        p.private = true;
        p.crumbs = vec![
            (ctx.t("nav.community").into(), "/community".into()),
            (title.into(), "/community/mod/log".into()),
        ];
        let mut b = format!("<h1>{}</h1>", esc(title));
        let mut m = format!("{title}\n\n");
        if d.audit.is_empty() {
            let _ = write!(b, "<p>{}</p>", esc(ctx.t("mod.empty_log")));
            let _ = writeln!(m, "{}", ctx.t("mod.empty_log"));
        } else {
            let _ = write!(
                b,
                "<table class=\"data\"><thead><tr><th>{}</th><th>{}</th><th>{}</th><th>{}</th></tr></thead><tbody>",
                esc(ctx.t("mod.when")),
                esc(ctx.t("mod.who")),
                esc(ctx.t("mod.action")),
                esc(ctx.t("mod.target"))
            );
            for a in d.audit.iter().rev().take(200) {
                let tgt = match a.target.split_once(':') {
                    Some(("thread", id)) => format!(
                        "<a href=\"/community/t/{}\">{}</a>",
                        esc(id),
                        esc(&a.target)
                    ),
                    _ => esc(&a.target),
                };
                let _ = write!(
                    b,
                    "<tr><td>{}</td><td>{}</td><td>{}{}</td><td>{}</td></tr>",
                    ctx.time_html(a.at),
                    esc(&a.actor),
                    esc(&a.action),
                    if a.detail.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", esc(&a.detail))
                    },
                    tgt
                );
                let _ = writeln!(
                    m,
                    "- {} - {} - {} - {}",
                    ctx.datetime(a.at),
                    a.actor,
                    a.action,
                    a.target
                );
            }
            b.push_str("</tbody></table>");
        }
        p.body = b;
        p.md = m;
        drop(d);
        render(ctx, p, Section::Forum)
    }
}

trait TapStatus {
    fn tap_status(self, s: u16) -> Self;
}
impl TapStatus for Resp {
    fn tap_status(mut self, s: u16) -> Self {
        self.status = s;
        self
    }
}

fn layout_headers(r: Resp) -> Resp {
    crate::layout::security_headers(r)
}

enum Compose {
    Thread {
        slug: String,
        title: String,
        body: String,
    },
    Reply {
        tid: u64,
        body: String,
    },
    Edit {
        pid: u64,
        tid: u64,
        body: String,
    },
}

fn honeypot() -> &'static str {
    "<div class=\"hp\" aria-hidden=\"true\"><label for=\"website\">Website</label><input id=\"website\" name=\"website\" type=\"text\" tabindex=\"-1\" autocomplete=\"off\"></div>"
}

fn compose_form(
    ctx: &Ctx,
    action: &str,
    title: Option<&str>,
    body: &str,
    back: &str,
    edit: bool,
) -> String {
    let mut h = format!(
        "<form action=\"{}\" method=\"post\"><input type=\"hidden\" name=\"csrf\" value=\"{}\">{}",
        esc(&ctx.lpath(action)),
        esc(&ctx.csrf),
        honeypot()
    );
    if let Some(t) = title {
        let _ = write!(
            h,
            "<label for=\"title\">{}</label><input id=\"title\" name=\"title\" type=\"text\" required maxlength=\"{}\" value=\"{}\">",
            esc(ctx.t("forum.title_label")),
            store::MAX_TITLE,
            esc(t)
        );
    }
    let _ = write!(
        h,
        "<label for=\"body\">{}</label><textarea id=\"body\" name=\"body\" required maxlength=\"{}\" aria-describedby=\"md-help\">{}</textarea><p class=\"help\" id=\"md-help\">{}</p><p><button type=\"submit\">{}</button> <a class=\"btn secondary\" href=\"{}\">←</a></p></form>",
        esc(ctx.t("forum.body_label")),
        store::MAX_BODY,
        esc(body),
        esc(ctx.t("forum.md_help")),
        esc(ctx.t(if edit {
            "forum.save"
        } else if title.is_some() {
            "forum.post_thread"
        } else {
            "forum.post_reply"
        })),
        esc(&ctx.lpath(back)),
    );
    h
}

fn reply_form(ctx: &Ctx, tid: u64, body: &str, _err: Option<&str>) -> String {
    format!(
        "<section aria-labelledby=\"reply-h\"><h2 id=\"reply-h\">{}</h2>{}</section>",
        esc(ctx.t("forum.reply")),
        compose_form(
            ctx,
            &format!("/community/t/{tid}/reply"),
            None,
            body,
            &format!("/community/t/{tid}"),
            false
        )
    )
}

fn mod_form(ctx: &Ctx, target: &str, action: &str, arg: &str, label: &str, ret: &str) -> String {
    format!(
        "<form action=\"/community/mod\" method=\"post\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><input type=\"hidden\" name=\"target\" value=\"{}\"><input type=\"hidden\" name=\"action\" value=\"{}\"><input type=\"hidden\" name=\"arg\" value=\"{}\"><input type=\"hidden\" name=\"return\" value=\"{}\"><button type=\"submit\" class=\"secondary\">{}</button></form>",
        esc(&ctx.csrf),
        esc(target),
        esc(action),
        esc(arg),
        esc(ret),
        esc(label)
    )
}

/// XML sitemap with `xhtml:link` language alternates.
pub fn sitemap_xml(urls: &[SitemapUrl]) -> String {
    let mut x = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">\n",
    );
    for u in urls {
        let _ = write!(x, "<url><loc>{}</loc>", esc(&u.loc));
        if let Some(l) = u.lastmod {
            let _ = write!(x, "<lastmod>{}</lastmod>", util::rfc3339(l));
        }
        for l in Lang::ALL {
            let _ = write!(
                x,
                "<xhtml:link rel=\"alternate\" hreflang=\"{}\" href=\"{}\"/>",
                l.code(),
                esc(&with_lang(&u.loc, l))
            );
        }
        let _ = write!(
            x,
            "<xhtml:link rel=\"alternate\" hreflang=\"x-default\" href=\"{}\"/>",
            esc(&u.loc)
        );
        x.push_str("</url>\n");
    }
    x.push_str("</urlset>\n");
    x
}
