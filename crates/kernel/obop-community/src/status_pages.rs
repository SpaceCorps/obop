//! `/status`: the status page, incident pages, the staff console, the Atom history and the
//! Atlassian Statuspage v2 compatible JSON endpoints.

use crate::Community;
use crate::accounts::{self, CSRF_COOKIE};
use crate::assets;
use crate::http::{Resp, cookie};
use crate::i18n::{self, Lang};
use crate::layout::{Ctx, Page, Section, render, with_lang};
use crate::status::{
    COMPONENTS, CompState, CompView, DayState, Impact, IncStatus, Incident, Indicator, Maintenance,
    Snapshot, component_en, component_key,
};
use crate::util::{self, esc};
use serde_json::{Value as J, json};
use std::fmt::Write as _;

const PAGE_ID: &str = "obop";

enum Route<'a> {
    Page,
    Incident(&'a str),
    Atom,
    Api(&'a str),
    RefreshJs,
    Admin,
    AdminIncident,
    AdminIncidentUpdate(&'a str),
    AdminMaintenance,
    Unknown,
}

fn route(path: &str) -> Route<'_> {
    let p = path.trim_end_matches('/');
    let rest = p.strip_prefix("/status").unwrap_or("");
    let parts: Vec<&str> = rest.split('/').skip(1).collect();
    match parts.as_slice() {
        [] => Route::Page,
        ["incidents", id] => Route::Incident(id),
        ["history.atom"] => Route::Atom,
        ["api", "v2", file] => Route::Api(file),
        ["assets", "refresh.js"] => Route::RefreshJs,
        ["admin"] => Route::Admin,
        ["admin", "incident"] => Route::AdminIncident,
        ["admin", "incident", id] => Route::AdminIncidentUpdate(id),
        ["admin", "maintenance"] => Route::AdminMaintenance,
        _ => Route::Unknown,
    }
}

/// `YYYY-MM-DD HH:MM` (or with `T`), UTC.
pub fn parse_utc(s: &str) -> Option<u64> {
    let s = s.trim().replace('T', " ");
    let (d, t) = s.split_once(' ')?;
    let mut dp = d.split('-');
    let (y, mo, da): (i64, i64, i64) = (
        dp.next()?.parse().ok()?,
        dp.next()?.parse().ok()?,
        dp.next()?.parse().ok()?,
    );
    let mut tp = t.split(':');
    let (h, mi): (u64, u64) = (tp.next()?.parse().ok()?, tp.next()?.parse().ok()?);
    if !(2000..=2100).contains(&y)
        || !(1..=12).contains(&mo)
        || !(1..=31).contains(&da)
        || h > 23
        || mi > 59
    {
        return None;
    }
    let days = util::days_from_civil(y, mo, da);
    // Reject impossible dates such as 2026-02-31.
    if util::civil_from_days(days) != (y, mo, da) {
        return None;
    }
    Some(days as u64 * 86_400 + h * 3600 + mi * 60)
}

fn state_label(ctx: &Ctx, s: CompState) -> &'static str {
    ctx.t(&s.key())
}

fn day_state_key(s: DayState) -> &'static str {
    match s {
        DayState::Ok => "state.operational",
        DayState::Degraded => "state.degraded",
        DayState::Outage => "state.major",
        DayState::NoData => "state.nodata",
    }
}

fn impact_label(ctx: &Ctx, i: Impact) -> &'static str {
    ctx.t(&format!("impact.{}", i.code()))
}

fn inc_status_label(ctx: &Ctx, s: IncStatus) -> &'static str {
    ctx.t(&format!("inc.{}", s.code()))
}

fn comp_name(ctx: &Ctx, id: &str) -> String {
    component_key(id).map_or_else(|| id.to_string(), |k| ctx.t(k).to_string())
}

fn pct(ctx: &Ctx, v: Option<f64>) -> String {
    v.map_or_else(|| "–".to_string(), |p| i18n::fmt_pct(ctx.lang, p))
}

fn incident_path(i: &Incident) -> String {
    format!("/status/incidents/{}", i.id)
}

impl Community {
    pub(crate) fn status_route(&self, ctx: &mut Ctx) -> Resp {
        let path = ctx.req.path.clone();
        let method = ctx.req.method.clone();
        if path == "/status/" {
            return Resp::redirect(301, &with_lang("/status", ctx.lang));
        }
        let r = route(&path);
        let is_api = matches!(r, Route::Api(_));
        if method == "OPTIONS" && is_api {
            return cors(
                Resp::new(204, "text/plain", "")
                    .header("allow", "GET, HEAD, OPTIONS")
                    .header("cache-control", "public, max-age=86400"),
            );
        }
        let writes = matches!(
            r,
            Route::AdminIncident | Route::AdminIncidentUpdate(_) | Route::AdminMaintenance
        );
        let ok_methods: &[&str] = if writes { &["POST"] } else { &["GET", "HEAD"] };
        if matches!(r, Route::Unknown) {
            return self.status_not_found(ctx);
        }
        if !ok_methods.contains(&method.as_str()) {
            let resp = self.status_error(ctx, 405, "err.405_title", "err.405_title");
            return resp.header("allow", &ok_methods.join(", "));
        }
        if writes {
            let Some(u) = ctx.user.clone() else {
                return self.status_error(ctx, 403, "err.403_title", "err.403_text");
            };
            if !u.staff || !self.status_csrf(ctx) {
                return self.status_error(ctx, 403, "err.403_title", "err.403_text");
            }
        }
        match r {
            Route::Page => self.status_page(ctx),
            Route::Incident(id) => self.incident_page(ctx, id),
            Route::Atom => self.status_atom(ctx),
            Route::Api(file) => self.status_api(ctx, file),
            Route::RefreshJs => {
                Resp::new(200, "text/javascript; charset=utf-8", assets::REFRESH_JS)
                    .header("cache-control", "public, max-age=86400")
                    .conditional(ctx.req, None)
            }
            Route::Admin => self.admin_page(ctx, None),
            Route::AdminIncident => self.admin_create_incident(ctx),
            Route::AdminIncidentUpdate(id) => self.admin_update_incident(ctx, id),
            Route::AdminMaintenance => self.admin_create_maintenance(ctx),
            Route::Unknown => self.status_not_found(ctx),
        }
    }

    fn status_csrf(&self, ctx: &Ctx) -> bool {
        if let Some(origin) = ctx.req.header("origin") {
            let host = ctx.req.header("host").unwrap_or("");
            if origin == "null" || origin.split("://").nth(1).unwrap_or("") != host {
                return false;
            }
        }
        let expected = match &ctx.session_raw {
            Some(raw) if ctx.user.is_some() => accounts::csrf_for_session(raw),
            _ => String::new(),
        };
        accounts::csrf_ok(&expected, ctx.req.form().get("csrf"))
    }

    fn status_error(&self, ctx: &mut Ctx, status: u16, title_key: &str, text_key: &str) -> Resp {
        let title = ctx.t(title_key);
        let text = ctx.t(text_key);
        let mut p = Page::new(title, text, &ctx.req.path.clone());
        p.status = status;
        p.noindex = true;
        p.private = true;
        p.crumbs = vec![(ctx.t("nav.status").into(), "/status".into())];
        p.body = format!(
            "<h1>{}</h1><p>{}</p><p><a href=\"{}\">{}</a></p>",
            esc(title),
            esc(text),
            esc(&ctx.lpath("/status")),
            esc(ctx.t("status.back"))
        );
        p.md = format!("{text}\n");
        render(ctx, p, Section::Status)
    }

    fn status_not_found(&self, ctx: &mut Ctx) -> Resp {
        self.status_error(ctx, 404, "err.404_title", "err.404_text")
    }

    // -----------------------------------------------------------------------------------------
    // Public page
    // -----------------------------------------------------------------------------------------

    fn incident_html(&self, ctx: &Ctx, i: &Incident, full: bool) -> (String, String) {
        let comps: Vec<String> = i.components.iter().map(|c| comp_name(ctx, c)).collect();
        let mut h = format!(
            "<article class=\"card impact-{}\"><h3><a href=\"{}\">{}</a></h3><p class=\"meta\"><span><strong>{}</strong></span><span>{}: {}</span><span>{}</span>",
            i.impact.code(),
            esc(&ctx.lpath(&incident_path(i))),
            esc(&i.title),
            esc(inc_status_label(ctx, i.status)),
            esc(ctx.t("status.impact")),
            esc(impact_label(ctx, i.impact)),
            esc(&ctx.tf("inc.started", &[("date", &ctx.datetime(i.created))])),
        );
        let mut m = format!(
            "### [{}]({})\n\n- {}\n- {}: {}\n- {}\n",
            i.title,
            ctx.abs(&incident_path(i)),
            inc_status_label(ctx, i.status),
            ctx.t("status.impact"),
            impact_label(ctx, i.impact),
            ctx.tf("inc.started", &[("date", &ctx.datetime(i.created))])
        );
        if let Some(r) = i.resolved_at {
            let _ = write!(
                h,
                "<span>{}</span>",
                esc(&ctx.tf("inc.resolved_at", &[("date", &ctx.datetime(r))]))
            );
            let _ = writeln!(
                m,
                "- {}",
                ctx.tf("inc.resolved_at", &[("date", &ctx.datetime(r))])
            );
        }
        if i.auto {
            let _ = write!(h, "<span>{}</span>", esc(ctx.t("inc.auto")));
        }
        h.push_str("</p>");
        if !comps.is_empty() {
            let _ = write!(
                h,
                "<p>{}</p>",
                esc(&ctx.tf("inc.affected", &[("names", &comps.join(", "))]))
            );
            let _ = writeln!(
                m,
                "- {}",
                ctx.tf("inc.affected", &[("names", &comps.join(", "))])
            );
        }
        let ups: Vec<_> = i.updates.iter().rev().collect();
        let shown = if full { ups.len() } else { 1 };
        if full {
            let _ = write!(h, "<h4>{}</h4>", esc(ctx.t("inc.updates")));
        }
        h.push_str("<ul class=\"timeline\">");
        for u in ups.iter().take(shown) {
            let _ = write!(
                h,
                "<li><strong>{}</strong> · {}<br>{}</li>",
                esc(inc_status_label(ctx, u.status)),
                ctx.time_html(u.at),
                esc(&u.body).replace('\n', "<br>"),
            );
            let _ = writeln!(
                m,
                "  - {} ({}): {}",
                inc_status_label(ctx, u.status),
                ctx.datetime(u.at),
                u.body.replace('\n', " ")
            );
        }
        h.push_str("</ul></article>");
        m.push('\n');
        (h, m)
    }

    fn maint_html(&self, ctx: &Ctx, mt: &Maintenance) -> (String, String) {
        let window = ctx.tf(
            "maint.window",
            &[
                ("from", &ctx.datetime(mt.start)),
                ("to", &ctx.datetime(mt.end)),
            ],
        );
        let state = if mt.active(ctx.now) {
            ctx.t("maint.in_progress")
        } else {
            ctx.t("maint.upcoming")
        };
        let comps: Vec<String> = mt.components.iter().map(|c| comp_name(ctx, c)).collect();
        let mut h = format!(
            "<article class=\"card\"><h3>{}</h3><p class=\"meta\"><span><strong>{}</strong></span><span>{}</span></p>",
            esc(&mt.title),
            esc(state),
            esc(&window)
        );
        if !comps.is_empty() {
            let _ = write!(
                h,
                "<p>{}</p>",
                esc(&ctx.tf("inc.affected", &[("names", &comps.join(", "))]))
            );
        }
        if !mt.body.is_empty() {
            let _ = write!(h, "<p>{}</p>", esc(&mt.body).replace('\n', "<br>"));
        }
        h.push_str("</article>");
        let m = format!(
            "### {}\n\n- {}\n- {}\n\n{}\n\n",
            mt.title, state, window, mt.body
        );
        (h, m)
    }

    fn comp_html(&self, ctx: &Ctx, c: &CompView, staff: bool) -> (String, String) {
        let name = comp_name(ctx, &c.id);
        let state = state_label(ctx, c.state);
        let mut h = format!(
            "<section class=\"comp\" aria-labelledby=\"c-{id}\"><header><h3 id=\"c-{id}\">{name}</h3><span class=\"state {code}\">{state}</span></header>",
            id = esc(&c.id),
            name = esc(&name),
            code = c.state.code(),
            state = esc(state),
        );
        let mut m = format!(
            "- **{}**: {} (24h {}, 7d {}, 90d {})\n",
            name,
            state,
            pct(ctx, c.u24),
            pct(ctx, c.u7),
            pct(ctx, c.u90)
        );
        if c.id == "instances"
            && let Some(p) = c.pct_up
        {
            let _ = write!(
                h,
                "<p>{}</p>",
                esc(&ctx.tf("inc.pct_up", &[("pct", &i18n::fmt_pct(ctx.lang, p))]))
            );
            let _ = writeln!(
                m,
                "  - {}",
                ctx.tf("inc.pct_up", &[("pct", &i18n::fmt_pct(ctx.lang, p))])
            );
            if !c.degraded_names.is_empty() {
                let names = c.degraded_names.join(", ");
                let _ = write!(
                    h,
                    "<p>{}</p>",
                    esc(&ctx.tf("inc.degraded_list", &[("names", &names)]))
                );
                let _ = writeln!(
                    m,
                    "  - {}",
                    ctx.tf("inc.degraded_list", &[("names", &names)])
                );
            }
        }
        if staff && !c.detail.is_empty() {
            let _ = write!(h, "<p class=\"help\">{}</p>", esc(&c.detail));
        }
        let _ = write!(
            h,
            "<ol class=\"bar\" aria-label=\"{}\">",
            esc(ctx.t("status.bar_label"))
        );
        for d in &c.days {
            let date = ctx.date(d.day);
            let label = match d.pct {
                Some(p) => ctx.tf(
                    "status.day_label",
                    &[
                        ("date", &date),
                        ("state", ctx.t(day_state_key(d.state))),
                        ("pct", &i18n::fmt_pct(ctx.lang, p)),
                    ],
                ),
                None => format!("{date}: {}", ctx.t("state.nodata")),
            };
            let class = match d.state {
                DayState::Ok => "ok",
                DayState::Degraded => "degraded",
                DayState::Outage => "outage",
                DayState::NoData => "nodata",
            };
            let _ = write!(
                h,
                "<li class=\"{class}\" title=\"{l}\"><span class=\"sr\">{l}</span></li>",
                l = esc(&label)
            );
        }
        let _ = write!(
            h,
            "</ol><div class=\"barlabels\" aria-hidden=\"true\"><span>{}</span><span>{}</span></div>",
            esc(ctx.t("status.days_ago_90")),
            esc(ctx.t("status.today"))
        );
        let _ = write!(
            h,
            "<ul class=\"uptimes\"><li>{} {}: <b>{}</b></li><li>{} {}: <b>{}</b></li><li>{} {}: <b>{}</b></li></ul></section>",
            esc(ctx.t("status.uptime")),
            esc(ctx.t("status.uptime_24h")),
            esc(&pct(ctx, c.u24)),
            esc(ctx.t("status.uptime")),
            esc(ctx.t("status.uptime_7d")),
            esc(&pct(ctx, c.u7)),
            esc(ctx.t("status.uptime")),
            esc(ctx.t("status.uptime_90d")),
            esc(&pct(ctx, c.u90)),
        );
        (h, m)
    }

    fn banner_text(&self, ctx: &Ctx, snap: &Snapshot) -> String {
        if snap.indicator == Indicator::None && snap.maintenance_now {
            ctx.t("status.maintenance_now").to_string()
        } else {
            ctx.t(snap.indicator.key()).to_string()
        }
    }

    fn status_page(&self, ctx: &mut Ctx) -> Resp {
        let snap = self.status.snapshot(ctx.now);
        let staff = ctx.staff();
        let refresh = ctx.req.q("refresh").as_deref() == Some("60");
        let banner = self.banner_text(ctx, &snap);
        let mut p = Page::new(&banner, ctx.t("status.intro"), "/status");
        p.crumbs = vec![(ctx.t("nav.status").into(), "/status".into())];
        p.last_modified = Some(snap.last_change).filter(|t| *t > 0);
        p.feeds = vec![(ctx.t("status.title").into(), "/status/history.atom".into())];
        p.refresh_js = refresh;
        let icon = match snap.indicator {
            Indicator::None => "✓",
            Indicator::Minor => "!",
            _ => "✕",
        };
        let mut b = format!(
            "<h1>{}</h1><p class=\"lede\">{}</p>",
            esc(ctx.t("status.title")),
            esc(ctx.t("status.intro"))
        );
        let mut m = format!("{}\n\n**{}**\n\n", ctx.t("status.intro"), banner);
        let _ = write!(
            b,
            "<div class=\"banner {}\" role=\"status\"><span class=\"ico\" aria-hidden=\"true\">{}</span><span>{}</span></div>",
            snap.indicator.code(),
            icon,
            esc(&banner)
        );
        let checked = snap.last_check.map_or_else(
            || ctx.t("status.never").to_string(),
            |t| ctx.tf("status.updated", &[("date", &ctx.datetime(t))]),
        );
        let refresh_link = if refresh {
            format!(
                "<a href=\"{}\">{}</a>",
                esc(&ctx.lpath("/status")),
                esc(ctx.t("status.auto_off"))
            )
        } else {
            format!(
                "<a href=\"{}\">{}</a>",
                esc(&ctx.lpath("/status?refresh=60")),
                esc(ctx.t("status.auto_on"))
            )
        };
        let _ = write!(
            b,
            "<p class=\"meta\"><span>{}</span><span>{}</span><a href=\"/status/history.atom\">{}</a><a href=\"/status/api/v2/summary.json\">{}</a></p>",
            esc(&checked),
            refresh_link,
            esc(ctx.t("status.feed")),
            esc(ctx.t("status.api_link"))
        );
        let _ = writeln!(m, "{checked}\n");
        let _ = write!(b, "<h2>{}</h2>", esc(ctx.t("status.components")));
        let _ = write!(m, "## {}\n\n", ctx.t("status.components"));
        if snap.comps.is_empty() {
            let _ = write!(b, "<p>{}</p>", esc(ctx.t("state.nodata")));
        }
        for c in &snap.comps {
            let (h, mm) = self.comp_html(ctx, c, staff);
            b.push_str(&h);
            m.push_str(&mm);
        }
        let _ = write!(b, "<h2>{}</h2>", esc(ctx.t("status.active")));
        let _ = write!(m, "\n## {}\n\n", ctx.t("status.active"));
        if snap.active.is_empty() {
            let _ = write!(b, "<p>{}</p>", esc(ctx.t("status.none_active")));
            let _ = writeln!(m, "{}", ctx.t("status.none_active"));
        }
        for i in &snap.active {
            let (h, mm) = self.incident_html(ctx, i, true);
            b.push_str(&h);
            m.push_str(&mm);
        }
        let _ = write!(b, "<h2>{}</h2>", esc(ctx.t("status.maintenance")));
        let _ = write!(m, "\n## {}\n\n", ctx.t("status.maintenance"));
        if snap.maint.is_empty() {
            let _ = write!(b, "<p>{}</p>", esc(ctx.t("status.none_maint")));
            let _ = writeln!(m, "{}", ctx.t("status.none_maint"));
        }
        for mt in &snap.maint {
            let (h, mm) = self.maint_html(ctx, mt);
            b.push_str(&h);
            m.push_str(&mm);
        }
        let _ = write!(b, "<h2>{}</h2>", esc(ctx.t("status.past")));
        let _ = write!(m, "\n## {}\n\n", ctx.t("status.past"));
        if snap.past.is_empty() {
            let _ = write!(b, "<p>{}</p>", esc(ctx.t("status.none_past")));
            let _ = writeln!(m, "{}", ctx.t("status.none_past"));
        }
        for i in snap.past.iter().take(30) {
            let (h, mm) = self.incident_html(ctx, i, false);
            b.push_str(&h);
            m.push_str(&mm);
        }
        p.jsonld = vec![json!({
            "@context": "https://schema.org", "@type": "WebPage", "name": banner, "description": ctx.t("status.intro"),
            "url": ctx.abs(&with_lang("/status", ctx.lang)), "inLanguage": ctx.lang.code(),
            "dateModified": util::rfc3339(snap.last_change.max(1)),
            "isPartOf": {"@type": "WebSite", "name": "obop", "url": ctx.abs("/")},
        })];
        p.body = b;
        p.md = m;
        render(ctx, p, Section::Status)
    }

    fn incident_page(&self, ctx: &mut Ctx, id: &str) -> Resp {
        let Some(inc) = id.parse::<u64>().ok().and_then(|i| self.status.incident(i)) else {
            return self.status_error(ctx, 404, "status.not_found", "err.404_text");
        };
        let path = incident_path(&inc);
        let latest = inc
            .updates
            .last()
            .map(|u| u.body.clone())
            .unwrap_or_default();
        let mut p = Page::new(&inc.title, &util::truncate_chars(&latest, 160), &path);
        p.crumbs = vec![
            (ctx.t("nav.status").into(), "/status".into()),
            (util::truncate_chars(&inc.title, 80), path.clone()),
        ];
        p.last_modified = Some(inc.updated);
        p.og_type = "article";
        let (h, m) = self.incident_html(ctx, &inc, true);
        p.body = format!(
            "<h1>{}</h1>{}<p><a href=\"{}\">{}</a></p>",
            esc(&inc.title),
            h,
            esc(&ctx.lpath("/status")),
            esc(ctx.t("status.back"))
        );
        p.md = m;
        p.jsonld = vec![json!({
            "@context": "https://schema.org", "@type": "WebPage", "name": inc.title, "url": ctx.abs(&with_lang(&path, ctx.lang)),
            "datePublished": util::rfc3339(inc.created), "dateModified": util::rfc3339(inc.updated), "inLanguage": ctx.lang.code(),
            "isPartOf": {"@type": "WebPage", "name": ctx.t("status.title"), "url": ctx.abs("/status")},
        })];
        render(ctx, p, Section::Status)
    }

    fn status_atom(&self, ctx: &mut Ctx) -> Resp {
        let incs = self.status.incidents();
        let updated = incs.iter().map(|i| i.updated).max().unwrap_or(0);
        let mut x = String::from(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<feed xmlns=\"http://www.w3.org/2005/Atom\">\n<title>obop status</title>\n",
        );
        let _ = writeln!(x, "<id>{}</id>", esc(&ctx.abs("/status/history.atom")));
        let _ = writeln!(
            x,
            "<link rel=\"self\" type=\"application/atom+xml\" href=\"{}\"/>",
            esc(&ctx.abs("/status/history.atom"))
        );
        let _ = writeln!(
            x,
            "<link rel=\"alternate\" type=\"text/html\" href=\"{}\"/>",
            esc(&ctx.abs("/status"))
        );
        let _ = writeln!(x, "<updated>{}</updated>", util::rfc3339(updated));
        for i in incs.iter().take(50) {
            let en = i.status.code();
            let body = i.updates.last().map(|u| u.body.as_str()).unwrap_or("");
            let _ = writeln!(
                x,
                "<entry><title>{} [{}]</title><id>{}</id><link rel=\"alternate\" type=\"text/html\" href=\"{}\"/><published>{}</published><updated>{}</updated><summary>{}</summary></entry>",
                esc(&i.title),
                en,
                esc(&ctx.abs(&incident_path(i))),
                esc(&ctx.abs(&incident_path(i))),
                util::rfc3339(i.created),
                util::rfc3339(i.updated),
                esc(body),
            );
        }
        x.push_str("</feed>\n");
        Resp::new(200, "application/atom+xml; charset=utf-8", x)
            .header("cache-control", "public, max-age=60")
            .header("x-content-type-options", "nosniff")
            .conditional(ctx.req, Some(updated).filter(|u| *u > 0))
    }

    // -----------------------------------------------------------------------------------------
    // Statuspage v2 JSON
    // -----------------------------------------------------------------------------------------

    fn status_api(&self, ctx: &mut Ctx, file: &str) -> Resp {
        let snap = self.status.snapshot(ctx.now);
        let body = match file {
            "summary.json" => statuspage_summary(&snap, &ctx.base),
            "status.json" => statuspage_status(&snap, &ctx.base),
            "components.json" => statuspage_components(&snap, &ctx.base),
            "incidents.json" => statuspage_incidents(&snap, &self.status.incidents(), &ctx.base),
            _ => return self.status_not_found(ctx),
        };
        let lm = Some(snap.last_change).filter(|t| *t > 0);
        let r = Resp::new(
            200,
            "application/json; charset=utf-8",
            serde_json::to_vec(&body).unwrap_or_default(),
        )
        .header("cache-control", "public, max-age=30")
        .header("x-content-type-options", "nosniff")
        .conditional(ctx.req, lm);
        cors(r)
    }

    // -----------------------------------------------------------------------------------------
    // Staff console
    // -----------------------------------------------------------------------------------------

    fn admin_page(&self, ctx: &mut Ctx, error: Option<&str>) -> Resp {
        if ctx.user.is_none() {
            let next = util::urlenc("/status/admin");
            return Resp::redirect(
                302,
                &with_lang(&format!("/community/login?next={next}"), ctx.lang),
            );
        }
        if !ctx.staff() {
            return self.status_error(ctx, 403, "err.403_title", "err.403_text");
        }
        let snap = self.status.snapshot(ctx.now);
        let title = ctx.t("adm.title");
        let mut p = Page::new(title, title, "/status/admin");
        p.noindex = true;
        p.private = true;
        p.crumbs = vec![
            (ctx.t("nav.status").into(), "/status".into()),
            (title.into(), "/status/admin".into()),
        ];
        let mut b = format!("<h1>{}</h1>", esc(title));
        if let Some(e) = error {
            let _ = write!(b, "<div class=\"alert\" role=\"alert\">{}</div>", esc(e));
        }
        let comps_boxes = |prefix: &str| {
            let mut h = format!(
                "<fieldset><legend>{}</legend>",
                esc(ctx.t("adm.components_label"))
            );
            for (id, key) in COMPONENTS {
                let _ = write!(
                    h,
                    "<label><input type=\"checkbox\" name=\"components\" value=\"{id}\" id=\"{prefix}-{id}\" style=\"width:auto;min-height:0\"> {}</label>",
                    esc(ctx.t(key))
                );
            }
            h.push_str("</fieldset>");
            h
        };
        let impact_select = |sel: &str| {
            let mut h = String::from("<select name=\"impact\">");
            for i in [Impact::None, Impact::Minor, Impact::Major, Impact::Critical] {
                let _ = write!(
                    h,
                    "<option value=\"{}\"{}>{}</option>",
                    i.code(),
                    if i.code() == sel { " selected" } else { "" },
                    esc(impact_label(ctx, i))
                );
            }
            h.push_str("</select>");
            h
        };
        let status_select = |sel: &str| {
            let mut h = String::from("<select name=\"status\">");
            for s in [
                IncStatus::Investigating,
                IncStatus::Identified,
                IncStatus::Monitoring,
                IncStatus::Resolved,
            ] {
                let _ = write!(
                    h,
                    "<option value=\"{}\"{}>{}</option>",
                    s.code(),
                    if s.code() == sel { " selected" } else { "" },
                    esc(inc_status_label(ctx, s))
                );
            }
            h.push_str("</select>");
            h
        };
        let _ = write!(
            b,
            "<h2>{}</h2><form action=\"/status/admin/incident\" method=\"post\" class=\"panel\"><input type=\"hidden\" name=\"csrf\" value=\"{}\">\
<label for=\"t\">{}</label><input id=\"t\" name=\"title\" type=\"text\" required maxlength=\"140\">\
<label>{} {}</label><label>{} {}</label>{}\
<label for=\"b\">{}</label><textarea id=\"b\" name=\"body\" maxlength=\"4000\"></textarea><p><button type=\"submit\">{}</button></p></form>",
            esc(ctx.t("adm.new_incident")),
            esc(&ctx.csrf),
            esc(ctx.t("forum.title_label")),
            esc(ctx.t("status.impact")),
            impact_select("minor"),
            esc(ctx.t("adm.status_label")),
            status_select("investigating"),
            comps_boxes("ni"),
            esc(ctx.t("forum.body_label")),
            esc(ctx.t("adm.create")),
        );
        let _ = write!(b, "<h2>{}</h2>", esc(ctx.t("status.active")));
        if snap.active.is_empty() {
            let _ = write!(b, "<p>{}</p>", esc(ctx.t("status.none_active")));
        }
        for i in &snap.active {
            let _ = write!(
                b,
                "<form action=\"/status/admin/incident/{id}\" method=\"post\" class=\"panel\"><h3><a href=\"{href}\">{title}</a></h3><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\">\
<label>{imp} {impsel}</label><label>{st} {stsel}</label><label for=\"u{id}\">{msg}</label><textarea id=\"u{id}\" name=\"body\" maxlength=\"4000\"></textarea><p><button type=\"submit\">{go}</button></p></form>",
                id = i.id,
                href = esc(&incident_path(i)),
                title = esc(&i.title),
                csrf = esc(&ctx.csrf),
                imp = esc(ctx.t("status.impact")),
                impsel = impact_select(i.impact.code()),
                st = esc(ctx.t("adm.status_label")),
                stsel = status_select(i.status.code()),
                msg = esc(ctx.t("forum.body_label")),
                go = esc(ctx.t("adm.add_update")),
            );
        }
        let _ = write!(
            b,
            "<h2>{}</h2><form action=\"/status/admin/maintenance\" method=\"post\" class=\"panel\"><input type=\"hidden\" name=\"csrf\" value=\"{}\">\
<label for=\"mt\">{}</label><input id=\"mt\" name=\"title\" type=\"text\" required maxlength=\"140\">\
<label for=\"ms\">{}</label><input id=\"ms\" name=\"start\" type=\"text\" required placeholder=\"2026-12-01 02:00\">\
<label for=\"me\">{}</label><input id=\"me\" name=\"end\" type=\"text\" required placeholder=\"2026-12-01 03:00\">{}\
<label for=\"mb\">{}</label><textarea id=\"mb\" name=\"body\" maxlength=\"4000\"></textarea><p><button type=\"submit\">{}</button></p></form>",
            esc(ctx.t("adm.schedule_title")),
            esc(&ctx.csrf),
            esc(ctx.t("forum.title_label")),
            esc(ctx.t("adm.starts")),
            esc(ctx.t("adm.ends")),
            comps_boxes("mm"),
            esc(ctx.t("forum.body_label")),
            esc(ctx.t("adm.schedule")),
        );
        p.body = b;
        p.md = format!("{title}\n");
        p.status = if error.is_some() { 400 } else { 200 };
        render(ctx, p, Section::Status)
    }

    fn admin_redirect(&self, ctx: &Ctx) -> Resp {
        crate::layout::security_headers(Resp::redirect(303, &with_lang("/status/admin", ctx.lang)))
    }

    fn admin_create_incident(&self, ctx: &mut Ctx) -> Resp {
        let f = ctx.req.form();
        let impact = Impact::parse(f.get("impact"));
        let status = IncStatus::parse(f.get("status"));
        let comps = f.all("components");
        let (Some(impact), Some(status)) = (impact, status) else {
            return self.admin_page(ctx, Some(ctx.t("adm.invalid")));
        };
        match self.status.create_incident(
            f.get("title"),
            impact,
            status,
            &comps,
            f.get("body"),
            ctx.now,
        ) {
            Ok(_) => self.admin_redirect(ctx),
            Err(_) => self.admin_page(ctx, Some(ctx.t("adm.invalid"))),
        }
    }

    fn admin_update_incident(&self, ctx: &mut Ctx, id: &str) -> Resp {
        let Ok(id) = id.parse::<u64>() else {
            return self.status_not_found(ctx);
        };
        let f = ctx.req.form();
        let status = IncStatus::parse(f.get("status"));
        let impact = Impact::parse(f.get("impact"));
        if status.is_none() {
            return self.admin_page(ctx, Some(ctx.t("adm.invalid")));
        }
        match self
            .status
            .update_incident(id, status, impact, f.get("body"), ctx.now)
        {
            Ok(()) => self.admin_redirect(ctx),
            Err(_) => self.status_not_found(ctx),
        }
    }

    fn admin_create_maintenance(&self, ctx: &mut Ctx) -> Resp {
        let f = ctx.req.form();
        let (Some(start), Some(end)) = (parse_utc(f.get("start")), parse_utc(f.get("end"))) else {
            return self.admin_page(ctx, Some(ctx.t("adm.invalid")));
        };
        match self.status.create_maintenance(
            f.get("title"),
            f.get("body"),
            &f.all("components"),
            start,
            end,
            ctx.now,
        ) {
            Ok(_) => self.admin_redirect(ctx),
            Err(_) => self.admin_page(ctx, Some(ctx.t("adm.invalid"))),
        }
    }

    /// Anonymous visitors of the staff console get the CSRF cookie too (not needed: the console
    /// is staff-only and uses session tokens), so nothing to do; kept for symmetry.
    #[allow(dead_code)]
    fn anon_cookie(ctx: &mut Ctx) {
        let _ = (CSRF_COOKIE, cookie);
        let _ = Lang::En;
        let _ = ctx;
    }
}

fn cors(r: Resp) -> Resp {
    r.header("access-control-allow-origin", "*")
        .header("access-control-allow-methods", "GET, HEAD, OPTIONS")
        .header(
            "access-control-allow-headers",
            "Accept, Content-Type, If-None-Match, If-Modified-Since",
        )
        .header("access-control-expose-headers", "ETag, Last-Modified")
        .header("timing-allow-origin", "*")
}

// ---------------------------------------------------------------------------------------------
// Statuspage v2 shapes (https://www.atlassian.com/software/statuspage, "Statuspage API")
// ---------------------------------------------------------------------------------------------

pub fn page_json(snap: &Snapshot, base: &str) -> J {
    json!({
        "id": PAGE_ID,
        "name": "obop",
        "url": format!("{base}/status"),
        "time_zone": "Etc/UTC",
        "updated_at": util::rfc3339_ms(snap.last_change.max(snap.last_check.unwrap_or(0))),
    })
}

pub fn status_json(snap: &Snapshot) -> J {
    let description = if snap.indicator == Indicator::None && snap.maintenance_now {
        "Scheduled Maintenance In Progress"
    } else {
        snap.indicator.description()
    };
    json!({"indicator": snap.indicator.code(), "description": description})
}

fn component_json(c: &CompView, position: usize, snap: &Snapshot) -> J {
    let created = c
        .days
        .iter()
        .find(|d| d.state != DayState::NoData)
        .map_or(snap.now, |d| d.day);
    json!({
        "id": c.id,
        "name": component_en(&c.id),
        "status": c.state.statuspage(),
        "created_at": util::rfc3339_ms(created),
        "updated_at": util::rfc3339_ms(c.checked.unwrap_or(created)),
        "position": position,
        "description": J::Null,
        "showcase": true,
        "start_date": J::Null,
        "group_id": J::Null,
        "page_id": PAGE_ID,
        "group": false,
        "only_show_if_degraded": false,
    })
}

fn components_json(snap: &Snapshot) -> Vec<J> {
    snap.comps
        .iter()
        .enumerate()
        .map(|(i, c)| component_json(c, i + 1, snap))
        .collect()
}

fn incident_json(i: &Incident, snap: &Snapshot, base: &str) -> J {
    let comp_objs: Vec<J> = i
        .components
        .iter()
        .filter_map(|id| {
            snap.comps
                .iter()
                .position(|c| &c.id == id)
                .map(|p| component_json(&snap.comps[p], p + 1, snap))
        })
        .collect();
    let updates: Vec<J> = i
        .updates
        .iter()
        .enumerate()
        .rev()
        .map(|(n, u)| {
            json!({
                "id": format!("{}-{}", i.id, n + 1),
                "status": u.status.code(),
                "body": u.body,
                "incident_id": i.id.to_string(),
                "created_at": util::rfc3339_ms(u.at),
                "updated_at": util::rfc3339_ms(u.at),
                "display_at": util::rfc3339_ms(u.at),
                "affected_components": i.components.iter().map(|c| json!({
                    "code": c, "name": component_en(c), "old_status": "operational",
                    "new_status": match i.impact { Impact::None => "operational", Impact::Minor => "degraded_performance", Impact::Major => "partial_outage", Impact::Critical => "major_outage" },
                })).collect::<Vec<_>>(),
                "deliver_notifications": true,
                "custom_tweet": J::Null,
                "tweet_id": J::Null,
            })
        })
        .collect();
    let monitoring_at = i
        .updates
        .iter()
        .find(|u| u.status == IncStatus::Monitoring)
        .map(|u| util::rfc3339_ms(u.at));
    json!({
        "id": i.id.to_string(),
        "name": i.title,
        "status": i.status.code(),
        "created_at": util::rfc3339_ms(i.created),
        "updated_at": util::rfc3339_ms(i.updated),
        "monitoring_at": monitoring_at,
        "resolved_at": i.resolved_at.map(util::rfc3339_ms),
        "impact": i.impact.code(),
        "shortlink": format!("{base}/status/incidents/{}", i.id),
        "started_at": util::rfc3339_ms(i.created),
        "page_id": PAGE_ID,
        "incident_updates": updates,
        "components": comp_objs,
    })
}

fn maintenance_json(m: &Maintenance, snap: &Snapshot, base: &str) -> J {
    let status = if m.active(snap.now) {
        "in_progress"
    } else if m.end <= snap.now {
        "completed"
    } else {
        "scheduled"
    };
    let comp_objs: Vec<J> = m
        .components
        .iter()
        .filter_map(|id| {
            snap.comps
                .iter()
                .position(|c| &c.id == id)
                .map(|p| component_json(&snap.comps[p], p + 1, snap))
        })
        .collect();
    json!({
        "id": format!("m{}", m.id),
        "name": m.title,
        "status": status,
        "created_at": util::rfc3339_ms(m.created),
        "updated_at": util::rfc3339_ms(m.created),
        "monitoring_at": J::Null,
        "resolved_at": J::Null,
        "impact": "maintenance",
        "shortlink": format!("{base}/status"),
        "started_at": util::rfc3339_ms(m.start),
        "page_id": PAGE_ID,
        "scheduled_for": util::rfc3339_ms(m.start),
        "scheduled_until": util::rfc3339_ms(m.end),
        "incident_updates": [{
            "id": format!("m{}-1", m.id), "status": status, "body": m.body, "incident_id": format!("m{}", m.id),
            "created_at": util::rfc3339_ms(m.created), "updated_at": util::rfc3339_ms(m.created), "display_at": util::rfc3339_ms(m.created),
            "affected_components": [], "deliver_notifications": true, "custom_tweet": J::Null, "tweet_id": J::Null,
        }],
        "components": comp_objs,
    })
}

pub fn statuspage_summary(snap: &Snapshot, base: &str) -> J {
    json!({
        "page": page_json(snap, base),
        "status": status_json(snap),
        "components": components_json(snap),
        "incidents": snap.active.iter().map(|i| incident_json(i, snap, base)).collect::<Vec<_>>(),
        "scheduled_maintenances": snap.maint.iter().map(|m| maintenance_json(m, snap, base)).collect::<Vec<_>>(),
    })
}

pub fn statuspage_status(snap: &Snapshot, base: &str) -> J {
    json!({"page": page_json(snap, base), "status": status_json(snap)})
}

pub fn statuspage_components(snap: &Snapshot, base: &str) -> J {
    json!({"page": page_json(snap, base), "components": components_json(snap)})
}

pub fn statuspage_incidents(snap: &Snapshot, all: &[Incident], base: &str) -> J {
    json!({
        "page": page_json(snap, base),
        "incidents": all.iter().take(50).map(|i| incident_json(i, snap, base)).collect::<Vec<_>>(),
    })
}
