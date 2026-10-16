//! The public, server-rendered parts of the obop website that are dynamic: the community forum
//! (`/community`) and the status page (`/status`). Plain HTML, no client-side framework, with
//! full SEO and agent-readiness metadata; accounts are the control-plane accounts of `obop-auth`.
//!
//! This crate has no web framework: [`Community::handle`] maps a framework-neutral [`Req`] to a
//! [`Resp`], and `obop-http` (the only crate allowed to use axum, D29) adapts it to the router.
//! See `docs/COMMUNITY.md`.

pub mod accounts;
pub mod assets;
pub mod forum;
pub mod http;
pub mod i18n;
pub mod layout;
pub mod md;
pub mod status;
pub mod status_pages;
pub mod store;
pub mod util;

pub use forum::{SitemapUrl, sitemap_xml};
pub use http::{MAX_BODY, Req, Resp};

use accounts::{CSRF_COOKIE, SESSION_COOKIE, Sessions};
use layout::{Ctx, User};
use obop_auth::Auth;
use obop_storage::Backend;
use status::Status;
use std::collections::HashSet;
use std::sync::Arc;
use store::Forum;

/// Settings read from the environment by [`Config::from_env`].
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Staff: moderate the forum and run the status console (`OBOP_PLATFORM_ADMINS`,
    /// comma-separated emails).
    pub admins: HashSet<String>,
    /// Public origin used for canonical and absolute URLs, e.g. `https://obop.example`
    /// (`OBOP_PUBLIC_URL`). When unset it is taken from the request's `Host`.
    pub public_url: Option<String>,
}

impl Config {
    pub fn from_env() -> Config {
        Config {
            admins: parse_admins(&std::env::var("OBOP_PLATFORM_ADMINS").unwrap_or_default()),
            public_url: std::env::var("OBOP_PUBLIC_URL")
                .ok()
                .map(|u| u.trim().trim_end_matches('/').to_string())
                .filter(|u| u.starts_with("http://") || u.starts_with("https://")),
        }
    }
}

pub fn parse_admins(s: &str) -> HashSet<String> {
    s.split(',')
        .map(|e| e.trim().to_ascii_lowercase())
        .filter(|e| !e.is_empty())
        .collect()
}

pub struct Community {
    pub cfg: Config,
    pub auth: Arc<Auth>,
    pub forum: Forum,
    pub sessions: Sessions,
    pub status: Arc<Status>,
}

impl Community {
    /// Open the forum and status data in `backend` (seeding the categories on first run).
    pub fn open(
        cfg: Config,
        auth: Arc<Auth>,
        backend: Arc<dyn Backend>,
    ) -> Result<Arc<Community>, String> {
        Ok(Arc::new(Community {
            forum: Forum::open(backend.clone(), cfg.admins.clone())?,
            sessions: Sessions::new(backend.clone()),
            status: Status::open(backend)?,
            auth,
            cfg,
        }))
    }

    /// True for the path prefixes this crate serves.
    pub fn owns(path: &str) -> bool {
        path == "/community"
            || path.starts_with("/community/")
            || path == "/status"
            || path.starts_with("/status/")
    }

    pub fn handle(&self, req: &Req) -> Resp {
        self.handle_at(req, util::now())
    }

    fn origin(&self, req: &Req) -> String {
        self.cfg.public_url.clone().unwrap_or_else(|| {
            // Only trust a plausible host name for absolute URLs.
            let host = req.header("host").unwrap_or("localhost");
            let ok = !host.is_empty()
                && host.len() <= 255
                && host
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || ".-:[]".contains(c));
            let host = if ok { host } else { "localhost" };
            format!("{}://{host}", if req.is_https() { "https" } else { "http" })
        })
    }

    /// Handle a request at time `now` (tests pass their own clock).
    pub fn handle_at(&self, req: &Req, now: u64) -> Resp {
        if req.body.len() > MAX_BODY {
            let mut c = self.context(req, now);
            let msg = c.t("err.413");
            let mut r = Resp::text(413, msg);
            r = layout::security_headers(r);
            c.set_cookies.clear();
            return r;
        }
        let mut ctx = self.context(req, now);
        let mut resp = if req.path.starts_with("/status") {
            self.status_route(&mut ctx)
        } else {
            self.forum_route(&mut ctx)
        };
        // A language chosen with ?lang= is remembered.
        if let Some(l) = req.q("lang").and_then(|l| i18n::Lang::from_code(&l))
            && req.cookie(i18n::COOKIE).as_deref() != Some(l.code())
            && resp.status < 500
        {
            resp = resp
                .add_header(
                    "set-cookie",
                    &http::cookie(i18n::COOKIE, l.code(), 365 * 86_400, req.is_https()),
                )
                .header("cache-control", "private, no-store");
            resp.headers.retain(|(k, _)| {
                !k.eq_ignore_ascii_case("etag") && !k.eq_ignore_ascii_case("last-modified")
            });
        }
        resp
    }

    fn context<'a>(&self, req: &'a Req, now: u64) -> Ctx<'a> {
        let (lang, source) = i18n::negotiate(
            req.q("lang").as_deref(),
            req.cookie(i18n::COOKIE).as_deref(),
            req.header("accept-language"),
        );
        let raw = req.cookie(SESSION_COOKIE);
        let mut user = None;
        let mut session_raw = None;
        if let Some(raw) = raw
            && let Some(email) = self.sessions.lookup(&raw, now)
        {
            // The member profile exists for every session (created at login).
            if let Ok(m) = self.forum.register(&email, None, now) {
                user = Some(User {
                    staff: self.forum.is_staff(&email),
                    email,
                    name: m.name,
                });
                session_raw = Some(raw);
            }
        }
        let csrf = match (&user, &session_raw) {
            (Some(_), Some(raw)) => accounts::csrf_for_session(raw),
            _ => req
                .cookie(CSRF_COOKIE)
                .filter(|c| c.len() == 64 && c.bytes().all(|b| b.is_ascii_hexdigit()))
                .unwrap_or_default(),
        };
        Ctx {
            req,
            lang,
            lang_source: source,
            md: req.wants_markdown(),
            base: self.origin(req),
            user,
            now,
            session_raw,
            csrf,
            set_cookies: Vec::new(),
        }
    }

    /// Every public URL this crate serves, for the main site's sitemap (index). Pass the public
    /// origin, e.g. `https://obop.example`. The same list is served at `/community/sitemap.xml`.
    pub fn sitemap_urls(&self, base: &str) -> Vec<SitemapUrl> {
        let base = base.trim_end_matches('/');
        let mut v = self.forum_sitemap_urls(base);
        let snap = self.status.snapshot(util::now());
        v.push(SitemapUrl {
            loc: format!("{base}/status"),
            lastmod: Some(snap.last_change).filter(|t| *t > 0),
        });
        v
    }

    /// Start the background status checker (see `status::spawn_checker`).
    pub fn spawn_checker(
        &self,
        probes: Vec<status::Probe>,
        interval: std::time::Duration,
    ) -> std::thread::JoinHandle<()> {
        status::spawn_checker(self.status.clone(), probes, interval)
    }
}
