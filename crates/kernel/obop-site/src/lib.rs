//! The public website and web shop of an instance (D50, docs/STOREFRONT.md).
//!
//! Server-rendered HTML for ordinary visitors, built only from what the installed `website` and
//! `ecommerce` packages declare public and what is published. No framework: [`Site::handle`]
//! takes a [`Req`] and returns a [`Resp`]; `obop-http` is the one place they meet axum (D29).
//!
//! Everything a visitor can read or write goes through `obop_commands::PublicApi`, so the
//! declarations of the packages are the whole attack surface: whitelisted fields of published
//! records, submissions restricted to declared fields, prices always taken from the listing.

pub mod assets;
mod checkout;
mod content;
mod countries;
pub mod http;
pub mod i18n;
pub mod layout;
pub mod md;
mod media;
mod orderpage;
pub mod pricing;
pub mod seed;
mod seo;
pub mod settings;
mod shop;
pub mod theme;
pub mod util;

use http::{Req, Resp};
use i18n::{Lang, Source};
use layout::{Caps, Ctx, MenuItem, Page};
use obop_commands::{Lookup, PaymentPort, PublicApi, PublicFilter, PublicRecord, Value};
use settings::Settings;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use util::{ct_eq, esc, random_token, sha256_hex};

pub use http::MAX_BODY;

#[derive(Debug, Clone)]
pub struct Config {
    /// Public origin (`OBOP_PUBLIC_URL`) used when the `site.base_url` setting is empty.
    pub public_url: Option<String>,
    /// Where the backoffice is served (`/admin`), for the "sign in" link and `robots.txt`.
    pub admin_prefix: Option<String>,
    /// How long settings and the menu are cached.
    pub settings_ttl: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            public_url: None,
            admin_prefix: Some("/admin".into()),
            settings_ttl: Duration::from_secs(5),
        }
    }
}

impl Config {
    pub fn from_env() -> Config {
        Config {
            public_url: std::env::var("OBOP_PUBLIC_URL")
                .ok()
                .map(|u| u.trim().trim_end_matches('/').to_string())
                .filter(|u| settings::origin_ok(u)),
            ..Config::default()
        }
    }
}

struct Cached {
    at: Instant,
    settings: Arc<Settings>,
    menu: Vec<MenuItem>,
    css: Arc<(String, String)>,
}

/// What claiming an idempotency key came to.
enum Claim {
    /// This request owns the key and must give it back if it places no order.
    Taken,
    /// An order (this token) already owns it: the submit is a repeat.
    Existing(String),
    /// The store refused to say. Nothing is claimed, and nothing may be released.
    Refused,
}

pub struct Site {
    api: PublicApi,
    cfg: Config,
    cached: Mutex<Option<Cached>>,
    /// Whatever takes payments (D62); without one orders stay pending and the shop writes to
    /// the buyer.
    payments: Option<Arc<dyn PaymentPort>>,
}

impl Site {
    pub fn new(api: PublicApi, cfg: Config) -> Site {
        Site {
            api,
            cfg,
            cached: Mutex::new(None),
            payments: None,
        }
    }

    /// Take payments through `port` (the payment module of the instance).
    pub fn with_payments(mut self, port: Arc<dyn PaymentPort>) -> Site {
        self.payments = Some(port);
        self
    }

    fn settings_snapshot(&self) -> Arc<Settings> {
        self.refresh().0
    }

    /// Whether the instance has anything to show publicly at all (a package declared it).
    pub fn has_public_models(&self) -> bool {
        ["website_page", "website_blog_post", "ecommerce_listing"]
            .iter()
            .any(|m| self.api.is_readable(m))
    }

    pub fn api(&self) -> &PublicApi {
        &self.api
    }

    fn refresh(&self) -> (Arc<Settings>, Vec<MenuItem>, Arc<(String, String)>) {
        let mut g = self.cached.lock().unwrap();
        if let Some(c) = g.as_ref()
            && c.at.elapsed() < self.cfg.settings_ttl
        {
            return (c.settings.clone(), c.menu.clone(), c.css.clone());
        }
        let s = Arc::new(Settings::from_map(&self.api.site_settings()));
        let menu = self.load_menu();
        let css = assets::css(&s.accent);
        let version = layout::css_version(&css);
        let c = Cached {
            at: Instant::now(),
            settings: s.clone(),
            menu: menu.clone(),
            css: Arc::new((css, version)),
        };
        let css = c.css.clone();
        *g = Some(c);
        (s, menu, css)
    }

    fn load_menu(&self) -> Vec<MenuItem> {
        let Ok((rows, _)) = self.api.list("website_menu_item", &[], Some(50), 0) else {
            return Vec::new();
        };
        rows.iter()
            .filter_map(|r| {
                let (label, url) = (r.text("label").trim(), r.text("url").trim());
                let ok = md::safe_url(url) && !url.starts_with('#') && !url.starts_with("mailto:");
                (!label.is_empty() && label.chars().count() <= 60 && ok).then(|| MenuItem {
                    label: label.to_string(),
                    url: url.to_string(),
                    new_window: r.flag("new_window"),
                })
            })
            .collect()
    }

    fn caps(&self) -> Caps {
        let sub = |m: &str| self.api.spec(m).is_some_and(|s| s.submit.is_some());
        Caps {
            shop: self.api.is_readable("ecommerce_listing"),
            blog: self.api.is_readable("website_blog_post"),
            pages: self.api.is_readable("website_page"),
            contact: sub("website_form_submission"),
        }
    }

    /// Count a hit for `key`; false when more than `max` fall in the last `window`. The count
    /// lives in the storage backend (a fixed window), so every server process shares it.
    fn allow(&self, key: &str, max: usize, window: Duration) -> bool {
        obop_storage::limit::allow(
            &**self.api.backend(),
            &format!("rl:site:{key}"),
            max as u64,
            window.as_millis() as u64,
        )
    }

    /// Claim an idempotency key for an order about to be placed: [`Claim::Taken`] when this call
    /// owns it, [`Claim::Existing`] with the token of the order that already owns it. Shared by
    /// every server process, so a double submit that lands on two of them still makes one order.
    /// A claim the store refused is **not** a claim ([`Claim::Refused`]): the submit is answered
    /// at once (the 503 layer says so) and releases nothing, or it would delete the claim of the
    /// other submit of the same form and the next one would make a second order (D74).
    fn claim_idem(&self, key: &str, token: &str) -> Claim {
        let k = format!("idem:site:{key}");
        let b = self.api.backend();
        let at = util::now();
        let mine = format!("{token}|{at}");
        match b.kv_put_new(&k, &mine, Some(86_400_000)) {
            Ok(true) => return Claim::Taken,
            Ok(false) => {}
            Err(_) => return Claim::Refused,
        }
        let Ok(found) = b.kv_get(&k) else {
            return Claim::Refused;
        };
        let existing = found.unwrap_or_else(|| mine.clone());
        let (owner, since) = existing
            .split_once('|')
            .map_or((existing.as_str(), 0), |(o, t)| {
                (o, t.parse::<u64>().unwrap_or(0))
            });
        // A claim that is old and has no order behind it is the ghost of a checkout that did not
        // finish and could not give its key back (the store refused for longer than the tries):
        // taken over, so the buyer's resubmit makes the order instead of being sent to a page
        // that does not exist (D74). A young claim is a submit still being processed.
        if at.saturating_sub(since) > 120
            && matches!(self.api.receipt("ecommerce_order", owner), Ok(None))
            && b.kv_swap(&k, &existing, &mine).unwrap_or(false)
        {
            return Claim::Taken;
        }
        Claim::Existing(owner.to_string())
    }

    /// Give back the key of an order that was not placed. `held` is the key this request really
    /// took (`None` when it took none), `token` the token it claimed it with.
    fn release_idem_key(&self, held: &Option<String>, token: &str) {
        if let Some(k) = held {
            self.release_idem(k, token);
        }
    }

    /// Give a claimed key back (the order was not placed), if it is still this request's: a claim
    /// that another submit owns now (after a takeover) is left alone.
    ///
    /// This undoes what the request itself did, so it is allowed after a refused call and tried
    /// again while the store is busy (D74): a key that stays claimed for a day turns a buyer's
    /// second submit into a ghost order.
    fn release_idem(&self, key: &str, token: &str) {
        let k = format!("idem:site:{key}");
        let b = self.api.backend();
        let mine = format!("{token}|");
        // The site logs nothing (a test holds it to that). A key that still cannot be given back
        // after the tries expires with its day; the checkout that needed it is answered 503.
        let _ = obop_storage::fault::patiently(4, Duration::from_millis(150), || {
            obop_storage::fault::bookkeeping(|| match b.kv_get(&k) {
                Ok(Some(v)) if !v.starts_with(&mine) => Ok(false),
                // Mine, or the store would not say: this request took it, so it gives it back.
                _ => b.kv_delete(&k),
            })
        });
    }

    /// Whether `key` has already had `max` hits in its window (nothing is counted). The count
    /// is the one `record` keeps, in the storage backend.
    fn limited(&self, key: &str, max: usize) -> bool {
        self.api
            .backend()
            .counter_get(&format!("rl:site:{key}"))
            .is_ok_and(|n| n >= max as u64)
    }

    /// Count one hit for `key` in a window of `window`, without checking a limit.
    fn record(&self, key: &str, window: Duration) {
        let _ = self.api.backend().counter_add(
            &format!("rl:site:{key}"),
            window.as_millis() as u64,
            false,
        );
    }

    /// The origin used in canonical links, feeds and sitemaps: the owner's setting, the
    /// configured public URL, or (last resort) the request's own `Host`.
    fn base(&self, req: &Req, s: &Settings) -> String {
        if let Some(u) = &s.base_url {
            return u.clone();
        }
        if let Some(u) = &self.cfg.public_url {
            return u.clone();
        }
        let host = req.header("host").unwrap_or("localhost");
        let host_ok = !host.is_empty()
            && host.len() <= 255
            && host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || ".-:[]".contains(c));
        let host = if host_ok { host } else { "localhost" };
        format!("{}://{host}", if req.is_https() { "https" } else { "http" })
    }

    fn ctx<'a>(&self, req: &'a Req) -> (Ctx<'a>, Arc<(String, String)>) {
        let (s, menu, css) = self.refresh();
        let from_query = req.q("lang").and_then(|l| Lang::parse(&l));
        let from_cookie = req.cookie("lang").and_then(|l| Lang::parse(&l));
        let from_header = req.header("accept-language").and_then(Lang::from_accept);
        let (lang, lang_source) = match (from_query, from_cookie, from_header) {
            (Some(l), _, _) => (l, Source::Query),
            (_, Some(l), _) => (l, Source::Cookie),
            (_, _, Some(l)) => (l, Source::Header),
            _ => (s.default_lang, Source::Default),
        };
        let mut set_cookies = Vec::new();
        if lang_source == Source::Query && from_cookie != Some(lang) {
            set_cookies.push(http::cookie(
                "lang",
                lang.code(),
                31_536_000,
                req.is_https(),
            ));
        }
        let ctx = Ctx {
            req,
            lang,
            lang_source,
            md: req.wants_markdown(),
            base: self.base(req, &s),
            caps: self.caps(),
            menu,
            admin_prefix: self.cfg.admin_prefix.clone(),
            csrf: String::new(),
            set_cookies,
            css_version: css.1.clone(),
            js_version: layout::css_version(assets::JS),
            s,
        };
        (ctx, css)
    }

    /// Answer one request.
    pub fn handle(&self, req: &Req) -> Resp {
        let (mut ctx, css) = self.ctx(req);
        let method = req.method.as_str();
        let path = req.path.as_str();

        // Assets and robots work even when the site is switched off.
        if method == "GET" || method == "HEAD" {
            match path {
                "/assets/site.css" => {
                    return asset(css.0.as_str(), "text/css; charset=utf-8", req, &ctx);
                }
                "/assets/cart.js" => {
                    return asset(assets::JS, "text/javascript; charset=utf-8", req, &ctx);
                }
                "/robots.txt" => return seo::robots(&ctx),
                "/favicon.svg" | "/favicon.ico" => return seo::favicon(&ctx),
                _ => {}
            }
        }
        if !ctx.s.enabled {
            return self.closed(&ctx);
        }
        let known_post = matches!(path, "/shop/checkout" | "/contact" | "/shop/discount/check")
            || path.starts_with("/shop/fake-pay/");
        if !matches!(method, "GET" | "HEAD") && !(method == "POST" && known_post) {
            return method_not_allowed(&ctx, if known_post { "POST" } else { "GET, HEAD" });
        }
        if req.body.len() > MAX_BODY {
            return self.error(&ctx, 413, "e413.title", "err.too_large");
        }
        let segs: Vec<String> = path
            .trim_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .map(util::pct_decode)
            .collect();
        let segs: Vec<&str> = segs.iter().map(String::as_str).collect();
        match (method, segs.as_slice()) {
            (_, []) => self.home(&mut ctx),
            ("GET" | "HEAD", ["media", rest]) => self.media(&ctx, rest),
            ("GET" | "HEAD", ["sitemap.xml"]) => seo::sitemap(self, &ctx),
            ("GET" | "HEAD", ["feed.xml"]) => seo::feed(self, &ctx),
            ("GET" | "HEAD", ["llms.txt"]) => seo::llms(self, &ctx),
            ("GET" | "HEAD", [".well-known", "security.txt"]) => seo::security_txt(&ctx),
            ("GET" | "HEAD", ["blog"]) => self.blog_index(&mut ctx),
            ("GET" | "HEAD", ["blog", slug]) => self.blog_post(&mut ctx, slug),
            ("GET" | "HEAD", ["shop"]) => self.shop_index(&mut ctx),
            ("GET" | "HEAD", ["shop", product]) => self.product(&mut ctx, product),
            ("GET" | "HEAD", ["shop", "order", token]) => self.order_page(&mut ctx, token),
            ("GET" | "HEAD", ["shop", "order", token, "receipt"]) => {
                self.receipt_page(&mut ctx, token)
            }
            ("GET" | "HEAD", ["shop", "pay", token]) => self.pay_page(&mut ctx, token),
            ("GET" | "HEAD", ["shop", "pay", token, "continue"]) => {
                self.pay_continue(&mut ctx, token)
            }
            ("GET" | "HEAD", ["shop", "fake-pay", session]) => {
                self.fake_pay_page(&mut ctx, session)
            }
            ("POST", ["shop", "fake-pay", session]) => self.fake_pay_decide(&mut ctx, session),
            ("POST", ["shop", "discount", "check"]) => self.discount_check(&mut ctx),
            ("GET" | "HEAD", ["cart"]) => self.cart(&mut ctx, None),
            ("POST", ["shop", "checkout"]) => self.checkout(&mut ctx),
            ("GET" | "HEAD" | "POST", ["contact"]) => self.contact(&mut ctx),
            ("GET" | "HEAD", [slug]) => self.page(&mut ctx, slug),
            _ => self.not_found(&ctx),
        }
    }

    // ---- shared page plumbing -------------------------------------------------------------

    fn closed(&self, ctx: &Ctx) -> Resp {
        let mut p = Page::new(ctx.t("closed.title"), ctx.t("closed.body"), &ctx.req.path);
        p.status = 503;
        p.noindex = true;
        p.body = format!(
            "<h1>{}</h1><p class=\"lede\">{}</p>",
            esc(ctx.t("closed.title")),
            esc(ctx.t("closed.body"))
        );
        p.md = format!("{}\n", ctx.t("closed.body"));
        layout::render(ctx, p).header("retry-after", "3600")
    }

    fn error(&self, ctx: &Ctx, status: u16, title: &str, body: &str) -> Resp {
        let mut p = Page::new(ctx.t(title), ctx.t(body), "/");
        p.status = status;
        p.noindex = true;
        p.body = format!(
            "<h1>{}</h1><p class=\"lede\">{}</p><p><a class=\"btn\" href=\"{}\">{}</a></p>",
            esc(ctx.t(title)),
            esc(ctx.t(body)),
            esc(&ctx.lpath("/")),
            esc(ctx.t("nav.home"))
        );
        p.md = format!("{}\n", ctx.t(body));
        layout::render(ctx, p)
    }

    fn not_found(&self, ctx: &Ctx) -> Resp {
        self.error(ctx, 404, "e404.title", "e404.body")
    }

    fn gone(&self, ctx: &Ctx) -> Resp {
        self.error(ctx, 410, "e410.title", "e410.body")
    }

    /// Make sure the visitor has a CSRF cookie and put its value into `ctx.csrf` (double
    /// submit: the same value goes into the form).
    fn ensure_csrf(&self, ctx: &mut Ctx) {
        match ctx.req.cookie("obop_csrf").filter(|c| valid_token(c)) {
            Some(c) => ctx.csrf = c,
            None => {
                let t = random_token();
                ctx.set_cookies
                    .push(http::cookie("obop_csrf", &t, 86_400, ctx.req.is_https()));
                ctx.csrf = t;
            }
        }
    }

    /// The checks every state-changing request passes: same-origin and a matching CSRF token.
    fn check_post(&self, ctx: &Ctx, form: &http::Form) -> Result<(), Resp> {
        let refuse = |s: &Site| s.error(ctx, 403, "e403.title", "err.csrf");
        if let Some(origin) = ctx.req.header("origin") {
            let host = ctx.req.header("host").unwrap_or("");
            let origin_host = origin.split_once("://").map_or("", |(_, h)| h);
            if origin == "null" || origin_host != host {
                return Err(refuse(self));
            }
        }
        let cookie = ctx.req.cookie("obop_csrf").unwrap_or_default();
        let field = form.get("csrf");
        if !valid_token(&cookie) || !ct_eq(&cookie, field) {
            return Err(refuse(self));
        }
        Ok(())
    }

    /// A short opaque key for rate limits; the address itself is never stored or logged.
    fn source(&self, req: &Req) -> String {
        sha256_hex(&format!("obop-site-src:{}", req.client))[..24].to_string()
    }

    /// For tests: lift the burst limits of one client address, leaving the packages' own.
    #[cfg(test)]
    pub(crate) fn reset_burst_limits_for_test(&self, client: &str) {
        let src = sha256_hex(&format!("obop-site-src:{client}"))[..24].to_string();
        for kind in ["checkout", "order"] {
            let _ = self
                .api
                .backend()
                .counter_reset(&format!("rl:site:{kind}:{src}"));
        }
    }

    // ---- lookups the pages share ----------------------------------------------------------

    /// Translations of one record: the one for `lang` (if any) and every language that has one.
    fn translations(&self, model: &str, ref_field: &str, id: &str) -> Vec<PublicRecord> {
        self.api
            .list(
                model,
                &[(ref_field.to_string(), PublicFilter::Eq(Value::text(id)))],
                Some(20),
                0,
            )
            .map(|(rows, _)| rows)
            .unwrap_or_default()
    }

    /// All translations into one language, keyed by the id of the translated record.
    fn translations_into(
        &self,
        model: &str,
        ref_field: &str,
        lang: Lang,
    ) -> HashMap<String, PublicRecord> {
        let rows = self
            .api
            .list(
                model,
                &[(
                    "lang".to_string(),
                    PublicFilter::Eq(Value::text(lang.code())),
                )],
                None,
                0,
            )
            .map(|(r, _)| r)
            .unwrap_or_default();
        rows.into_iter()
            .filter_map(|r| {
                let id = r.fields.get(ref_field)?.get("id")?.as_str()?.to_string();
                Some((id, r))
            })
            .collect()
    }

    fn find(&self, model: &str, slug: &str) -> Lookup {
        if !valid_slug(slug) {
            return Lookup::NotFound;
        }
        self.api
            .find_by(model, "slug", slug)
            .unwrap_or(Lookup::NotFound)
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_media;
#[cfg(test)]
mod tests_shop;

fn method_not_allowed(ctx: &Ctx, allow: &str) -> Resp {
    let mut p = Page::new(ctx.t("e405.title"), ctx.t("e405.title"), "/");
    p.status = 405;
    p.noindex = true;
    p.body = format!("<h1>{}</h1>", esc(ctx.t("e405.title")));
    p.md = format!("{}\n", ctx.t("e405.title"));
    layout::render(ctx, p).header("allow", allow)
}

/// A static asset whose URL carries a hash of its content (`?v=`): cacheable for a year.
fn asset(body: &str, ctype: &str, req: &Req, ctx: &Ctx) -> Resp {
    let r = Resp::new(200, ctype, body);
    let r = if req.q("v").is_some() {
        r.header("cache-control", "public, max-age=31536000, immutable")
    } else {
        r.header("cache-control", "public, max-age=3600")
    };
    layout::security_headers(r.conditional(req, None), ctx)
}

pub(crate) fn valid_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 120
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// A blob id: 64 lowercase hex characters.
pub(crate) fn valid_hex_id(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}

pub(crate) fn valid_token(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}
