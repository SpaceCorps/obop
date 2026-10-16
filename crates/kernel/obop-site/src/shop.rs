//! The web shop: product grid and pages, the cart, checkout and the order confirmation.
//!
//! Prices, names and availability always come from the published listing on the server. The
//! browser sends only listing ids and quantities; there is no field a visitor could use to name
//! a price.

use crate::content::{Field, describe, email_ok, error_summary, page_param, pager};
use crate::http::{Form, Resp};
use crate::i18n;
use crate::layout::{self, Ctx, Page, Variants};
use crate::seo;
use crate::settings::Settings;
use crate::util::{clean_line, esc, random_token, sha256_hex, truncate_chars, urlenc};
use crate::{Site, md, valid_token};
use obop_commands::{Lookup, PublicFilter, PublicRecord, Submission, SubmitError, Value};
use serde_json::json;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

const MAX_LINES: usize = 50;
const MAX_QTY: i64 = 99;
const MAX_TOTAL: i64 = 100_000_000_000;

pub(crate) fn product_path(rec: &PublicRecord) -> String {
    format!("/shop/{}-{}", rec.id, rec.text("slug"))
}

pub(crate) fn currency_of(rec: &PublicRecord, s: &Settings) -> String {
    let c = rec.text("currency");
    if crate::settings::currency_ok(c) {
        c.to_string()
    } else {
        s.shop_currency.clone()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Avail {
    InStock,
    OutOfStock,
    Backorder,
}

fn availability(rec: &PublicRecord) -> Avail {
    let qty = rec.int("stock_quantity").unwrap_or(0);
    match rec.text("stock_policy") {
        "block_when_out" if qty <= 0 => Avail::OutOfStock,
        "allow_backorder" if qty <= 0 => Avail::Backorder,
        _ => Avail::InStock,
    }
}

fn schema_availability(a: Avail) -> &'static str {
    match a {
        Avail::InStock => "https://schema.org/InStock",
        Avail::OutOfStock => "https://schema.org/OutOfStock",
        Avail::Backorder => "https://schema.org/BackOrder",
    }
}

/// `1234` in two-digit currency as `12.34` for JSON-LD, whatever the visitor's language.
fn decimal(minor: i64, currency: &str) -> String {
    let d = i18n::minor_digits(currency);
    let unit = 10u64.pow(d);
    let abs = minor.unsigned_abs();
    let sign = if minor < 0 { "-" } else { "" };
    if d == 0 {
        format!("{sign}{abs}")
    } else {
        format!("{sign}{}.{:0w$}", abs / unit, abs % unit, w = d as usize)
    }
}

fn monogram(title: &str) -> String {
    title
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "·".into())
}

fn tint_of(id: &str) -> usize {
    let h = sha256_hex(id);
    usize::from_str_radix(&h[..4], 16).unwrap_or(0) % 7
}

fn pick<'a>(rec: &'a PublicRecord, tr: Option<&'a PublicRecord>, k: &str) -> &'a str {
    tr.map(|t| t.text(k))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| rec.text(k))
}

fn price_html(ctx: &Ctx, rec: &PublicRecord) -> String {
    let cur = currency_of(rec, &ctx.s);
    let Some(p) = rec.int("price") else {
        return String::new();
    };
    let mut h = format!("<span class=\"price\">{}", esc(&ctx.money(p, &cur)));
    if let Some(old) = rec.int("compare_at_price").filter(|o| *o > p) {
        h.push_str(&format!(
            "<span class=\"was\"><span class=\"sr\">{}</span> {}</span>",
            esc(&ctx.tf("shop.was", &[("price", "")])),
            esc(&ctx.money(old, &cur))
        ));
    }
    h.push_str("</span>");
    h
}

pub(crate) fn product_card(ctx: &Ctx, rec: &PublicRecord, tr: Option<&PublicRecord>) -> String {
    let title = pick(rec, tr, "title");
    let summary = pick(rec, tr, "summary");
    let on_sale = rec
        .int("compare_at_price")
        .zip(rec.int("price"))
        .is_some_and(|(o, p)| o > p);
    let badge = match availability(rec) {
        Avail::OutOfStock => format!(
            "<span class=\"badge warn cardtop\">{}</span>",
            esc(ctx.t("shop.out_of_stock"))
        ),
        _ if on_sale => format!(
            "<span class=\"badge cardtop\">{}</span>",
            esc(ctx.t("shop.sale"))
        ),
        _ => String::new(),
    };
    let lang = if tr.is_none() && ctx.lang != ctx.s.default_lang {
        format!(" lang=\"{}\"", ctx.s.default_lang.code())
    } else {
        String::new()
    };
    format!(
        "<li class=\"card\"{lang}><div class=\"art t{}\" aria-hidden=\"true\">{}</div>{badge}<div class=\"body\"><h3><a href=\"{}\">{}</a></h3>{}<p>{}</p></div></li>",
        tint_of(&rec.id),
        esc(&monogram(title)),
        esc(&ctx.lpath(&product_path(rec))),
        esc(title),
        if summary.is_empty() {
            String::new()
        } else {
            format!("<p class=\"sum\">{}</p>", esc(summary))
        },
        price_html(ctx, rec),
    )
}

pub(crate) fn product_md_line(ctx: &Ctx, rec: &PublicRecord, tr: Option<&PublicRecord>) -> String {
    let cur = currency_of(rec, &ctx.s);
    format!(
        "- [{}]({}) {}\n",
        pick(rec, tr, "title").replace(['[', ']'], ""),
        ctx.abs(&product_path(rec)),
        rec.int("price")
            .map_or(String::new(), |p| ctx.money(p, &cur))
    )
}

/// Lines of a cart as `(listing id, quantity)`; ids are shaped like UUIDs, quantities 1 to 99.
fn id_ok(s: &str) -> bool {
    s.len() == 36 && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// The cart lines of a request. Quantities are kept as they were sent (a later check decides
/// what is acceptable); `bad` is set when one was not a whole number.
struct Parsed {
    lines: Vec<(String, i64)>,
    bad: bool,
}

impl Parsed {
    fn merge(&mut self, id: &str, q: i64) {
        if !id_ok(id) {
            return;
        }
        if let Some(l) = self.lines.iter_mut().find(|l| l.0 == id) {
            l.1 = l.1.saturating_add(q);
        } else if self.lines.len() < MAX_LINES {
            self.lines.push((id.to_string(), q));
        }
    }

    fn qty(&mut self, raw: &str) -> i64 {
        match raw.trim().parse::<i64>() {
            Ok(q) => q,
            Err(_) => {
                self.bad = true;
                1
            }
        }
    }
}

/// The cart lines a request carries: repeated `id`/`qty` pairs (the update and checkout forms),
/// else `items=ID.QTY~ID.QTY` (the shareable link), plus `add=ID` for one more.
fn parse_lines(pairs: &[(String, String)]) -> Parsed {
    let mut out = Parsed {
        lines: Vec::new(),
        bad: false,
    };
    let ids: Vec<&String> = pairs
        .iter()
        .filter(|(k, _)| k == "id")
        .map(|(_, v)| v)
        .collect();
    let qtys: Vec<&String> = pairs
        .iter()
        .filter(|(k, _)| k == "qty")
        .map(|(_, v)| v)
        .collect();
    if !ids.is_empty() {
        for (i, id) in ids.iter().enumerate() {
            let q = match qtys.get(i) {
                Some(q) => out.qty(q),
                None => 1,
            };
            out.merge(id, q);
        }
    } else if let Some((_, items)) = pairs.iter().find(|(k, _)| k == "items") {
        for part in items.split('~').take(MAX_LINES) {
            let (id, q) = part.split_once('.').unwrap_or((part, "1"));
            let q = out.qty(q);
            out.merge(id, q);
        }
    }
    if let Some((_, add)) = pairs.iter().find(|(k, _)| k == "add") {
        let q = match pairs.iter().find(|(k, _)| k == "addqty") {
            Some((_, v)) => out.qty(v),
            None => 1,
        };
        out.merge(add, q.clamp(1, MAX_QTY));
    }
    out
}

fn items_string(lines: &[(String, i64)]) -> String {
    lines
        .iter()
        .map(|(id, q)| format!("{id}.{q}"))
        .collect::<Vec<_>>()
        .join("~")
}

struct Resolved {
    rec: PublicRecord,
    tr: Option<PublicRecord>,
    qty: i64,
}

/// What a failed checkout shows again.
pub(crate) struct Redo {
    pub errors: Vec<(&'static str, String)>,
    pub form: Form,
}

impl Site {
    pub(crate) fn shop_translations(&self, ctx: &Ctx) -> BTreeMap<String, PublicRecord> {
        if ctx.lang == ctx.s.default_lang {
            return BTreeMap::new();
        }
        self.translations_into("ecommerce_listing_translation", "listing_id", ctx.lang)
            .into_iter()
            .collect()
    }

    fn categories(&self) -> Vec<PublicRecord> {
        self.api
            .list("ecommerce_category", &[], None, 0)
            .map(|(r, _)| r)
            .unwrap_or_default()
    }

    // ---- /shop ----------------------------------------------------------------------------

    pub(crate) fn shop_index(&self, ctx: &mut Ctx) -> Resp {
        if !ctx.caps.shop {
            return self.not_found(ctx);
        }
        let cats = self.categories();
        let cat_slug = ctx.req.q("cat").unwrap_or_default();
        let cat = cats.iter().find(|c| c.text("slug") == cat_slug);
        if !cat_slug.is_empty() && cat.is_none() {
            return self.not_found(ctx);
        }
        let q: String = clean_line(&ctx.req.q("q").unwrap_or_default())
            .chars()
            .take(100)
            .collect();
        let filters: Vec<(String, PublicFilter)> = cat
            .map(|c| {
                vec![(
                    "category_id".to_string(),
                    PublicFilter::Eq(Value::text(&c.id)),
                )]
            })
            .unwrap_or_default();
        let mut items = self
            .api
            .list("ecommerce_listing", &filters, None, 0)
            .map(|(r, _)| r)
            .unwrap_or_default();
        let tr = self.shop_translations(ctx);
        if !q.is_empty() {
            let needle = q.to_lowercase();
            items.retain(|r| {
                [
                    pick(r, tr.get(&r.id), "title"),
                    pick(r, tr.get(&r.id), "summary"),
                    r.text("sku"),
                ]
                .iter()
                .any(|t| t.to_lowercase().contains(&needle))
            });
        }
        let size = ctx.s.shop_page_size;
        let total = items.len();
        let pages = total.div_ceil(size).max(1);
        let page = page_param(ctx);
        if page > pages {
            return self.not_found(ctx);
        }
        let shown: Vec<&PublicRecord> = items.iter().skip((page - 1) * size).take(size).collect();

        let mut query: Vec<String> = Vec::new();
        if !cat_slug.is_empty() {
            query.push(format!("cat={}", urlenc(&cat_slug)));
        }
        if !q.is_empty() {
            query.push(format!("q={}", urlenc(&q)));
        }
        if page > 1 {
            query.push(format!("page={page}"));
        }
        let path = if query.is_empty() {
            "/shop".to_string()
        } else {
            format!("/shop?{}", query.join("&"))
        };
        let title = match cat {
            Some(c) => c.text("name").to_string(),
            None => ctx.t("shop.title").to_string(),
        };
        let desc = match cat {
            Some(c) if !c.text("description").is_empty() => c.text("description").to_string(),
            _ => ctx.t("shop.intro").to_string(),
        };
        let mut p = Page::new(&title, &desc, &path);
        p.variants = Variants::Chrome;
        // Search results are thin pages: keep them out of the index.
        p.noindex = !q.is_empty();
        p.crumbs = vec![
            (ctx.t("nav.home").into(), "/".into()),
            (ctx.t("shop.title").into(), "/shop".into()),
        ];
        if let Some(c) = cat {
            p.crumbs.push((c.text("name").into(), path.clone()));
        }
        let keep: Vec<(&str, String)> = vec![("cat", cat_slug.clone()), ("q", q.clone())];
        let base_path = "/shop";
        p.prev = (page > 1).then(|| qpath(base_path, &keep, page - 1));
        p.next = (page < pages).then(|| qpath(base_path, &keep, page + 1));
        let ld_items: Vec<(String, String)> = shown
            .iter()
            .map(|r| (pick(r, tr.get(&r.id), "title").to_string(), product_path(r)))
            .collect();
        p.jsonld.push(seo::item_list_ld(ctx, &title, &ld_items));

        let mut h = format!(
            "<div class=\"shoptop\"><div><h1>{}</h1><p class=\"lede\">{}</p></div>",
            esc(&title),
            esc(&desc)
        );
        h.push_str(&format!(
            "<form class=\"searchform\" role=\"search\" method=\"get\" action=\"/shop\">\
<div class=\"field\"><label for=\"q\">{}</label><input type=\"search\" id=\"q\" name=\"q\" maxlength=\"100\" value=\"{}\" placeholder=\"{}\" autocomplete=\"off\"></div>{}{}\
<button class=\"btn secondary\" type=\"submit\">{}</button></form></div>",
            esc(ctx.t("shop.search")),
            esc(&q),
            esc(ctx.t("shop.search_ph")),
            if cat_slug.is_empty() { String::new() } else { format!("<input type=\"hidden\" name=\"cat\" value=\"{}\">", esc(&cat_slug)) },
            if ctx.lang == ctx.s.default_lang { String::new() } else { format!("<input type=\"hidden\" name=\"lang\" value=\"{}\">", ctx.lang.code()) },
            esc(ctx.t("shop.search_btn")),
        ));
        if !cats.is_empty() {
            h.push_str(&format!(
                "<nav aria-label=\"{}\"><ul class=\"chips\">",
                esc(ctx.t("shop.categories"))
            ));
            let chip = |label: &str, slug: &str, current: bool| {
                let mut keep = vec![("cat", slug.to_string())];
                if !q.is_empty() {
                    keep.push(("q", q.clone()));
                }
                format!(
                    "<li><a href=\"{}\"{}>{}</a></li>",
                    esc(&ctx.lpath(&qpath("/shop", &keep, 1))),
                    if current {
                        " aria-current=\"true\""
                    } else {
                        ""
                    },
                    esc(label)
                )
            };
            h.push_str(&chip(ctx.t("shop.all"), "", cat.is_none()));
            for c in &cats {
                h.push_str(&chip(
                    c.text("name"),
                    c.text("slug"),
                    cat.is_some_and(|x| x.id == c.id),
                ));
            }
            h.push_str("</ul></nav>");
        }
        let mut md_out = String::new();
        if shown.is_empty() {
            let key = if q.is_empty() && cat.is_none() {
                "shop.empty"
            } else {
                "shop.none"
            };
            h.push_str(&format!("<p role=\"status\">{}</p>", esc(ctx.t(key))));
            md_out.push_str(ctx.t(key));
        } else {
            h.push_str(&format!(
                "<p class=\"muted\" role=\"status\">{}</p><ul class=\"grid\">",
                esc(&ctx.tf(
                    "shop.count",
                    &[("n", &i18n::fmt_num(ctx.lang, total as u64))]
                ))
            ));
            for r in &shown {
                h.push_str(&product_card(ctx, r, tr.get(&r.id)));
                md_out.push_str(&product_md_line(ctx, r, tr.get(&r.id)));
            }
            h.push_str("</ul>");
        }
        h.push_str(&pager(ctx, "/shop", page, pages, &keep));
        p.body = h;
        p.md = md_out;
        layout::render(ctx, p)
    }

    // ---- /shop/{id}-{slug} ----------------------------------------------------------------

    pub(crate) fn product(&self, ctx: &mut Ctx, seg: &str) -> Resp {
        if !ctx.caps.shop {
            return self.not_found(ctx);
        }
        let (id, slug) = match seg.get(..36) {
            Some(id) if id_ok(id) && seg.as_bytes().get(36) == Some(&b'-') => (id, &seg[37..]),
            _ => return self.not_found(ctx),
        };
        let rec = match self.api.get("ecommerce_listing", id) {
            Ok(Lookup::Found(r)) => r,
            Ok(Lookup::Gone) => return self.gone(ctx),
            _ => return self.not_found(ctx),
        };
        if slug != rec.text("slug") {
            return Resp::redirect(301, &ctx.lpath(&product_path(&rec)));
        }
        let loc = self.localize(
            ctx,
            &rec,
            "ecommerce_listing_translation",
            "listing_id",
            &["title", "summary", "description"],
        );
        let title = loc.get("title").to_string();
        let path = product_path(&rec);
        let cur = currency_of(&rec, &ctx.s);
        let avail = availability(&rec);
        let desc = describe(loc.get("summary"), loc.get("description"), &ctx.s.tagline);
        let cat = rec.fields.get("category_id").and_then(|c| {
            Some((
                c.get("name")?.as_str()?.to_string(),
                c.get("slug")?.as_str()?.to_string(),
            ))
        });

        let mut p = Page::new(&title, &desc, &path);
        p.variants = Variants::Only(loc.langs.clone());
        p.content_lang = Some(loc.lang);
        p.og_type = "product";
        p.crumbs = vec![
            (ctx.t("nav.home").into(), "/".into()),
            (ctx.t("shop.title").into(), "/shop".into()),
        ];
        if let Some((name, slug)) = &cat {
            p.crumbs
                .push((name.clone(), format!("/shop?cat={}", urlenc(slug))));
        }
        p.crumbs.push((title.clone(), path.clone()));

        let mut ld = json!({
            "@context": "https://schema.org", "@type": "Product",
            "name": title, "description": desc, "url": ctx.abs(&path),
        });
        if !rec.text("sku").is_empty() {
            ld["sku"] = json!(rec.text("sku"));
        }
        if let Some((name, _)) = &cat {
            ld["category"] = json!(name);
        }
        if let Some(price) = rec.int("price") {
            ld["offers"] = json!({
                "@type": "Offer", "url": ctx.abs(&path), "price": decimal(price, &cur),
                "priceCurrency": cur, "availability": schema_availability(avail),
                "itemCondition": "https://schema.org/NewCondition",
            });
        }
        p.jsonld.push(ld);

        let mut h = String::from("<article class=\"product\">");
        h.push_str(&format!(
            "<div class=\"art t{}\" role=\"img\" aria-label=\"{}\">{}</div><div>",
            tint_of(&rec.id),
            esc(ctx.t("shop.product_image")),
            esc(&monogram(&title))
        ));
        h.push_str(&format!("<h1>{}</h1>", esc(&title)));
        let price = price_html(ctx, &rec);
        h.push_str(&format!("<p class=\"big\">{price}</p>"));
        let (state_key, warn) = match avail {
            Avail::InStock => ("shop.in_stock", false),
            Avail::Backorder => ("shop.backorder", false),
            Avail::OutOfStock => ("shop.out_of_stock", true),
        };
        h.push_str(&format!(
            "<p class=\"meta\"><span class=\"badge{}\">{}</span>{}</p>",
            if warn { " warn" } else { "" },
            esc(ctx.t(state_key)),
            if rec.text("sku").is_empty() {
                String::new()
            } else {
                format!(
                    "<span>{}</span>",
                    esc(&ctx.tf("shop.sku", &[("sku", rec.text("sku"))]))
                )
            }
        ));
        if !loc.get("summary").is_empty() {
            h.push_str(&format!(
                "<p class=\"lede\">{}</p>",
                esc(loc.get("summary"))
            ));
        }
        if avail != Avail::OutOfStock {
            h.push_str(&format!(
                "<form class=\"buy\" method=\"get\" action=\"/cart\" data-add=\"{id}\" data-added=\"{added}\">\
<input type=\"hidden\" name=\"add\" value=\"{id}\">{lang}\
<div class=\"field\"><label for=\"qty\">{qty}</label><input type=\"number\" id=\"qty\" name=\"addqty\" min=\"1\" max=\"99\" value=\"1\" inputmode=\"numeric\"></div>\
<button class=\"btn\" type=\"submit\">{btn}</button><span role=\"status\" class=\"muted\"></span></form>",
                id = esc(&rec.id),
                added = esc(ctx.t("shop.added")),
                lang = if ctx.lang == ctx.s.default_lang { String::new() } else { format!("<input type=\"hidden\" name=\"lang\" value=\"{}\">", ctx.lang.code()) },
                qty = esc(ctx.t("shop.qty")),
                btn = esc(ctx.t("shop.add")),
            ));
        }
        if !loc.get("description").is_empty() {
            h.push_str(&format!(
                "<div class=\"prose\">{}</div>",
                md::render(loc.get("description"))
            ));
        }
        h.push_str(&format!(
            "<p><a href=\"{}\">&larr; {}</a></p></div></article>",
            esc(&ctx.lpath("/shop")),
            esc(ctx.t("shop.back"))
        ));
        p.body = h;
        p.md = format!(
            "{}{}\n\n{}\n\n{}",
            rec.int("price")
                .map_or(String::new(), |pr| format!("**{}**", ctx.money(pr, &cur))),
            format_args!(" · {}", ctx.t(state_key)),
            loc.get("summary"),
            md::to_markdown(loc.get("description"))
        );
        layout::render(ctx, p)
    }

    // ---- /cart ----------------------------------------------------------------------------

    fn resolve_lines(&self, ctx: &Ctx, lines: &[(String, i64)]) -> (Vec<Resolved>, usize) {
        let tr = self.shop_translations(ctx);
        let mut out = Vec::new();
        let mut missing = 0;
        for (id, qty) in lines {
            match self.api.get("ecommerce_listing", id) {
                Ok(Lookup::Found(rec)) => {
                    let t = tr.get(&rec.id).cloned();
                    out.push(Resolved {
                        rec,
                        tr: t,
                        qty: *qty,
                    });
                }
                _ => missing += 1,
            }
        }
        (out, missing)
    }

    pub(crate) fn cart(&self, ctx: &mut Ctx, redo: Option<Redo>) -> Resp {
        if !ctx.caps.shop {
            return self.not_found(ctx);
        }
        let pairs: Vec<(String, String)> = match &redo {
            Some(r) => r.form.0.clone(),
            None => ctx.req.query_pairs(),
        };
        // On the cart page a quantity of 0 removes a line and anything else is clamped.
        let lines: Vec<(String, i64)> = parse_lines(&pairs)
            .lines
            .into_iter()
            .filter(|(_, q)| *q >= 1)
            .map(|(id, q)| (id, q.min(MAX_QTY)))
            .collect();
        let (items, missing) = self.resolve_lines(ctx, &lines);
        let live: Vec<(String, i64)> = items.iter().map(|i| (i.rec.id.clone(), i.qty)).collect();
        let mut p = Page::new(ctx.t("cart.title"), ctx.t("cart.title"), "/cart");
        p.noindex = true;
        p.private = true;
        p.crumbs = vec![
            (ctx.t("nav.home").into(), "/".into()),
            (ctx.t("cart.title").into(), "/cart".into()),
        ];
        let lang_q = if ctx.lang == ctx.s.default_lang {
            String::new()
        } else {
            format!("&lang={}", ctx.lang.code())
        };
        p.body_attrs = format!(
            " data-cart-page data-items=\"{}\" data-lang=\"{}\"",
            esc(&items_string(&live)),
            esc(&lang_q)
        );
        let mut h = format!("<h1>{}</h1>", esc(ctx.t("cart.title")));
        let mut md_out = String::new();
        let mut errors: Vec<(&str, String)> =
            redo.as_ref().map(|r| r.errors.clone()).unwrap_or_default();
        if missing > 0 && redo.is_none() {
            h.push_str(&format!(
                "<div class=\"notice\" role=\"status\">{}</div>",
                esc(&ctx.tf("err.unavailable", &[("name", "…")]))
            ));
        }
        if items.is_empty() {
            h.push_str(&error_summary(ctx, &errors));
            h.push_str(&format!(
                "<p>{}</p><p><a class=\"btn\" href=\"{}\">{}</a></p>",
                esc(ctx.t("cart.empty")),
                esc(&ctx.lpath("/shop")),
                esc(ctx.t("cart.continue"))
            ));
            md_out.push_str(ctx.t("cart.empty"));
            p.status = if redo.is_some() { 400 } else { 200 };
            p.body = h;
            p.md = md_out;
            return layout::render(ctx, p);
        }
        self.ensure_csrf(ctx);

        // Currency: one per order.
        let cur = currency_of(&items[0].rec, &ctx.s);
        let mixed = items.iter().any(|i| currency_of(&i.rec, &ctx.s) != cur);
        if mixed {
            errors.push(("", ctx.t("err.currency").into()));
        }
        let subtotal: i64 = items
            .iter()
            .map(|i| i.rec.int("price").unwrap_or(0).saturating_mul(i.qty))
            .fold(0i64, |a, b| a.saturating_add(b));
        let methods = self
            .api
            .list("ecommerce_shipping_method", &[], Some(20), 0)
            .map(|(r, _)| r)
            .unwrap_or_default();

        let form = redo.as_ref().map(|r| &r.form);
        let get = |k: &str| form.map_or("", |f| f.get(k));
        let chosen = {
            let c = get("shipping_method_id");
            methods
                .iter()
                .find(|m| m.id == c)
                .or(methods.first())
                .map(|m| m.id.clone())
        };
        let ship_of = |m: &PublicRecord| -> i64 {
            match m.int("free_over") {
                Some(f) if subtotal >= f => 0,
                _ => m.int("price").unwrap_or(0),
            }
        };
        let ship_now = methods
            .iter()
            .find(|m| Some(&m.id) == chosen.as_ref())
            .map_or(0, ship_of);

        h.push_str(&error_summary(ctx, &errors));
        h.push_str("<div class=\"cartgrid\"><section aria-labelledby=\"lines\">");
        h.push_str(&format!(
            "<h2 id=\"lines\" class=\"sr\">{}</h2>",
            esc(ctx.t("order.items"))
        ));
        h.push_str("<form method=\"get\" action=\"/cart\"><ul class=\"lines\">");
        for (n, it) in items.iter().enumerate() {
            let title = pick(&it.rec, it.tr.as_ref(), "title");
            let unit = it.rec.int("price").unwrap_or(0);
            let mut without = live.clone();
            without.remove(n);
            let remove_href = format!("/cart?items={}{}", urlenc(&items_string(&without)), lang_q);
            md_out.push_str(&format!(
                "- {} × {} = {}\n",
                title.replace(['[', ']'], ""),
                it.qty,
                ctx.money(unit.saturating_mul(it.qty), &cur)
            ));
            h.push_str(&format!(
                "<li class=\"line\"><div><div class=\"name\"><a href=\"{href}\">{title}</a></div><div class=\"each\">{each}</div>\
<div class=\"qty\"><input type=\"hidden\" name=\"id\" value=\"{id}\"><label class=\"sr\" for=\"qty-{n}\">{ql}</label>\
<input type=\"number\" id=\"qty-{n}\" name=\"qty\" min=\"0\" max=\"99\" value=\"{q}\" inputmode=\"numeric\"> <a href=\"{rm}\" aria-label=\"{rml}\">{rmt}</a></div></div>\
<div class=\"sum\">{sum}</div></li>",
                href = esc(&ctx.lpath(&product_path(&it.rec))),
                title = esc(title),
                each = esc(&ctx.money(unit, &currency_of(&it.rec, &ctx.s))),
                id = esc(&it.rec.id),
                ql = esc(&format!("{}: {}", ctx.t("shop.qty"), title)),
                q = it.qty,
                rm = esc(&remove_href),
                rml = esc(&ctx.tf("cart.remove_item", &[("name", title)])),
                rmt = esc(ctx.t("cart.remove")),
                sum = esc(&ctx.money(unit.saturating_mul(it.qty), &currency_of(&it.rec, &ctx.s))),
            ));
        }
        h.push_str(&format!(
            "</ul>{}<button class=\"btn secondary small\" type=\"submit\">{}</button></form>",
            if ctx.lang == ctx.s.default_lang {
                String::new()
            } else {
                format!(
                    "<input type=\"hidden\" name=\"lang\" value=\"{}\">",
                    ctx.lang.code()
                )
            },
            esc(ctx.t("cart.update"))
        ));
        // Without JavaScript this is how a second product gets into the cart.
        let all = self
            .api
            .list("ecommerce_listing", &[], Some(200), 0)
            .map(|(r, _)| r)
            .unwrap_or_default();
        if !all.is_empty() {
            let tr = self.shop_translations(ctx);
            h.push_str(&format!(
                "<details class=\"more\"><summary>{}</summary><form method=\"get\" action=\"/cart\">\
<input type=\"hidden\" name=\"items\" value=\"{}\">{}\
<div class=\"field\"><label for=\"add\">{}</label><select id=\"add\" name=\"add\">",
                esc(ctx.t("cart.nojs")),
                esc(&items_string(&live)),
                if ctx.lang == ctx.s.default_lang { String::new() } else { format!("<input type=\"hidden\" name=\"lang\" value=\"{}\">", ctx.lang.code()) },
                esc(ctx.t("cart.item")),
            ));
            for r in all.iter().filter(|r| availability(r) != Avail::OutOfStock) {
                h.push_str(&format!(
                    "<option value=\"{}\">{}</option>",
                    esc(&r.id),
                    esc(pick(r, tr.get(&r.id), "title"))
                ));
            }
            h.push_str(&format!(
                "</select></div><button class=\"btn secondary small\" type=\"submit\">{}</button></form></details>",
                esc(ctx.t("shop.add"))
            ));
        }
        h.push_str("</section>");

        // Checkout form.
        h.push_str(&format!(
            "<section aria-labelledby=\"co\"><form class=\"formcard\" method=\"post\" action=\"/shop/checkout\" novalidate>\
<h2 id=\"co\">{}</h2>\
<input type=\"hidden\" name=\"csrf\" value=\"{}\"><input type=\"hidden\" name=\"idem\" value=\"{}\"><input type=\"hidden\" name=\"lang\" value=\"{}\">",
            esc(ctx.t("co.title")),
            esc(&ctx.csrf),
            esc(&random_token()[..32]),
            ctx.lang.code()
        ));
        for (id, q) in &live {
            h.push_str(&format!(
                "<input type=\"hidden\" name=\"id\" value=\"{}\"><input type=\"hidden\" name=\"qty\" value=\"{q}\">",
                esc(id)
            ));
        }
        let bad = |id: &str| errors.iter().any(|(e, _)| *e == id);
        let f = |id, label, kind, auto, max| Field {
            id,
            label,
            kind,
            value: get(id),
            required: true,
            help: "",
            autocomplete: auto,
            invalid: bad(id),
            maxlen: max,
        };
        h.push_str(&f("customer_name", ctx.t("co.name"), "text", "name", 120).html(ctx));
        let mut email = f("customer_email", ctx.t("co.email"), "email", "email", 254);
        email.help = ctx.t("co.email_help");
        h.push_str(&email.html(ctx));
        h.push_str(
            &f(
                "shipping_address",
                ctx.t("co.address"),
                "textarea",
                "street-address",
                500,
            )
            .html(ctx),
        );
        if !methods.is_empty() {
            h.push_str(&format!(
                "<fieldset><legend>{}</legend>",
                esc(ctx.t("co.shipping"))
            ));
            for m in &methods {
                let price = ship_of(m);
                h.push_str(&format!(
                    "<label class=\"opt\"><input type=\"radio\" name=\"shipping_method_id\" value=\"{id}\" data-price=\"{raw}\"{fo}{checked}> <span class=\"grow\">{name}{days}</span> <strong>{price}</strong></label>",
                    id = esc(&m.id),
                    raw = m.int("price").unwrap_or(0),
                    fo = m.int("free_over").map_or(String::new(), |f| format!(" data-free-over=\"{f}\"")),
                    checked = if chosen.as_ref() == Some(&m.id) { " checked" } else { "" },
                    name = esc(m.text("name")),
                    days = m.int("delivery_days").map_or(String::new(), |d| format!(" <span class=\"muted\">({})</span>", esc(&ctx.tf("cart.days", &[("n", &d.to_string())])))),
                    price = if price == 0 { esc(ctx.t("cart.free")) } else { esc(&ctx.money(price, &cur)) },
                ));
            }
            h.push_str("</fieldset>");
        }
        h.push_str(&format!(
            "<dl class=\"totals\" data-sub=\"{sub}\" data-currency=\"{cur}\" data-lang=\"{lang}\">\
<dt>{st}</dt><dd>{subm}</dd><dt>{sh}</dt><dd data-ship-out data-free=\"{free}\">{shm}</dd>\
<dt class=\"grand\">{tt}</dt><dd class=\"grand\" data-total-out>{totm}</dd></dl>",
            sub = subtotal,
            cur = esc(&cur),
            lang = ctx.lang.code(),
            st = esc(ctx.t("cart.subtotal")),
            subm = esc(&ctx.money(subtotal, &cur)),
            sh = esc(ctx.t("cart.shipping")),
            free = esc(ctx.t("cart.free")),
            shm = if methods.is_empty() {
                "—".to_string()
            } else if ship_now == 0 {
                esc(ctx.t("cart.free"))
            } else {
                esc(&ctx.money(ship_now, &cur))
            },
            tt = esc(ctx.t("cart.total")),
            totm = esc(&ctx.money(subtotal.saturating_add(ship_now), &cur)),
        ));
        h.push_str(&format!(
            "<div class=\"hp\" aria-hidden=\"true\"><label for=\"website\">{}</label><input type=\"text\" id=\"website\" name=\"website\" tabindex=\"-1\" autocomplete=\"off\"></div>",
            esc(ctx.t("co.honeypot"))
        ));
        h.push_str(&format!(
            "<p class=\"muted\">{}{}</p><button class=\"btn\" type=\"submit\"{}>{}</button></form></section></div>",
            esc(ctx.t("cart.no_payment")),
            if ctx.s.checkout_note.is_empty() { String::new() } else { format!(" {}", esc(&ctx.s.checkout_note)) },
            if mixed { " disabled" } else { "" },
            esc(ctx.t("co.submit")),
        ));
        h.push_str(&format!(
            "<p><a href=\"{}\">&larr; {}</a></p>",
            esc(&ctx.lpath("/shop")),
            esc(ctx.t("cart.continue"))
        ));
        md_out.push_str(&format!(
            "\n{}: {}\n{}: {}\n",
            ctx.t("cart.subtotal"),
            ctx.money(subtotal, &cur),
            ctx.t("cart.total"),
            ctx.money(subtotal.saturating_add(ship_now), &cur)
        ));
        if redo.is_some() {
            p.status = 400;
        }
        p.body = h;
        p.md = md_out;
        layout::render(ctx, p)
    }

    // ---- POST /shop/checkout --------------------------------------------------------------

    pub(crate) fn checkout(&self, ctx: &mut Ctx) -> Resp {
        if !ctx.caps.shop {
            return self.not_found(ctx);
        }
        let form = ctx.req.form();
        if let Err(r) = self.check_post(ctx, &form) {
            return r;
        }
        // A bot fills the hidden field: it gets a page that looks like success and nothing is
        // created.
        if !form.get("website").is_empty() {
            return Resp::redirect(303, &ctx.lpath("/shop"));
        }
        let src = self.source(ctx.req);
        if !self.allow(&format!("checkout:{src}"), 8, Duration::from_secs(60)) {
            return self
                .error(ctx, 429, "e429.title", "err.rate")
                .header("retry-after", "60");
        }
        let fail = |site: &Site, ctx: &mut Ctx, errors: Vec<(&'static str, String)>| {
            let r = Redo {
                errors,
                form: ctx.req.form(),
            };
            site.cart(ctx, Some(r))
        };

        let parsed = parse_lines(&form.0);
        let lines = parsed.lines;
        if lines.is_empty() {
            return fail(self, ctx, vec![("", ctx.t("err.empty_cart").into())]);
        }
        let mut errors: Vec<(&'static str, String)> = Vec::new();
        if parsed.bad {
            errors.push(("", ctx.t("err.quantity").into()));
        }
        let name = clean_line(form.get("customer_name"));
        let email = form.get("customer_email").trim().to_string();
        let address = form.get("shipping_address").trim().replace('\r', "");
        if name.is_empty() || name.chars().count() > 120 {
            errors.push(("customer_name", ctx.t("err.name").into()));
        }
        if !email_ok(&email) {
            errors.push(("customer_email", ctx.t("err.email").into()));
        }
        if address.chars().count() < 5 || address.chars().count() > 500 {
            errors.push(("shipping_address", ctx.t("err.address").into()));
        }

        // Price every line from the published listing. Nothing here comes from the browser but
        // the id and the quantity.
        let mut priced: Vec<(PublicRecord, i64, i64)> = Vec::new();
        let mut subtotal: i64 = 0;
        let mut currency: Option<String> = None;
        for (id, qty) in &lines {
            if !(1..=MAX_QTY).contains(qty) {
                if !parsed.bad {
                    errors.push(("", ctx.t("err.quantity").into()));
                }
                break;
            }
            let rec = match self.api.get("ecommerce_listing", id) {
                Ok(Lookup::Found(r)) => r,
                _ => {
                    errors.push(("", ctx.tf("err.unavailable", &[("name", "…")])));
                    continue;
                }
            };
            let title = rec.text("title").to_string();
            let unit = match rec.int("price") {
                Some(p) if p >= 0 => p,
                _ => {
                    errors.push(("", ctx.tf("err.unavailable", &[("name", &title)])));
                    continue;
                }
            };
            match availability(&rec) {
                Avail::OutOfStock => {
                    errors.push(("", ctx.tf("err.unavailable", &[("name", &title)])));
                    continue;
                }
                Avail::InStock
                    if rec.text("stock_policy") == "block_when_out"
                        && *qty > rec.int("stock_quantity").unwrap_or(0) =>
                {
                    errors.push(("", ctx.tf("err.stock", &[("name", &title)])));
                    continue;
                }
                _ => {}
            }
            let cur = currency_of(&rec, &ctx.s);
            if currency.get_or_insert_with(|| cur.clone()) != &cur {
                errors.push(("", ctx.t("err.currency").into()));
                continue;
            }
            let Some(line_total) = unit.checked_mul(*qty) else {
                errors.push(("", ctx.t("err.generic").into()));
                continue;
            };
            subtotal = match subtotal.checked_add(line_total) {
                Some(s) if s <= MAX_TOTAL => s,
                _ => {
                    errors.push(("", ctx.t("err.generic").into()));
                    continue;
                }
            };
            priced.push((rec, *qty, unit));
        }

        // Delivery: the method must be an active public one; its price is looked up here.
        let methods = self
            .api
            .list("ecommerce_shipping_method", &[], Some(20), 0)
            .map(|(r, _)| r)
            .unwrap_or_default();
        let mut shipping_id: Option<String> = None;
        let mut shipping_total = 0i64;
        if !methods.is_empty() {
            match methods
                .iter()
                .find(|m| m.id == form.get("shipping_method_id"))
            {
                Some(m) => {
                    shipping_id = Some(m.id.clone());
                    shipping_total = match m.int("free_over") {
                        Some(f) if subtotal >= f => 0,
                        _ => m.int("price").unwrap_or(0).max(0),
                    };
                }
                None => errors.push(("", ctx.t("err.shipping").into())),
            }
        }
        if !errors.is_empty() {
            return fail(self, ctx, errors);
        }
        let Some(total) = subtotal.checked_add(shipping_total) else {
            return fail(self, ctx, vec![("", ctx.t("err.generic").into())]);
        };
        let currency = currency.unwrap_or_else(|| ctx.s.shop_currency.clone());

        // One order per idempotency key, even when two submits race.
        let idem = form.get("idem");
        let idem_key =
            (idem.len() >= 16 && idem.len() <= 64 && idem.chars().all(|c| c.is_ascii_hexdigit()))
                .then(|| sha256_hex(&format!("idem:{src}:{idem}")));
        let mut seen = self.idem.lock().unwrap();
        seen.retain(|_, (_, at)| at.elapsed() < Duration::from_secs(86_400));
        if let Some((token, _)) = idem_key.as_ref().and_then(|k| seen.get(k)) {
            return Resp::redirect(303, &order_url(ctx, token));
        }

        let token = random_token();
        let number = order_number();
        let mut order_visitor = BTreeMap::from([
            ("customer_email".to_string(), Value::text(&email)),
            ("customer_name".to_string(), Value::text(&name)),
            ("shipping_address".to_string(), Value::text(&address)),
        ]);
        if let Some(id) = &shipping_id {
            order_visitor.insert("shipping_method_id".into(), Value::text(id));
        }
        let mut batch = vec![Submission {
            model: "ecommerce_order".into(),
            visitor: order_visitor,
            server: BTreeMap::from([
                ("number".to_string(), Value::text(&number)),
                ("placed_on".to_string(), Value::text(&i18n::today())),
                ("currency".to_string(), Value::text(&currency)),
                ("subtotal".to_string(), Value::Int(subtotal)),
                ("shipping_total".to_string(), Value::Int(shipping_total)),
                ("discount_total".to_string(), Value::Int(0)),
                ("total".to_string(), Value::Int(total)),
                ("access_token".to_string(), Value::text(&token)),
            ]),
            links: vec![],
        }];
        for (rec, qty, unit) in &priced {
            let mut server = BTreeMap::from([
                (
                    "description".to_string(),
                    Value::text(&truncate_chars(rec.text("title"), 200)),
                ),
                ("unit_price".to_string(), Value::Int(*unit)),
                ("subtotal".to_string(), Value::Int(unit * qty)),
            ]);
            if let Ok(Some(serde_json::Value::String(pid))) =
                self.api
                    .published_server_field("ecommerce_listing", &rec.id, "product_id")
            {
                server.insert("product_id".into(), Value::text(&pid));
            }
            batch.push(Submission {
                model: "ecommerce_order_line".into(),
                visitor: BTreeMap::from([
                    ("listing_id".to_string(), Value::text(&rec.id)),
                    ("quantity".to_string(), Value::Int(*qty)),
                ]),
                server,
                links: vec![("order_id".into(), 0)],
            });
        }
        match self.api.submit_batch(&src, batch) {
            Ok(_) => {
                if let Some(k) = idem_key {
                    seen.insert(k, (token.clone(), Instant::now()));
                }
                drop(seen);
                Resp::redirect(303, &order_url(ctx, &token))
            }
            Err(SubmitError::RateLimited { retry_after_secs }) => {
                drop(seen);
                self.error(ctx, 429, "e429.title", "err.rate")
                    .header("retry-after", &retry_after_secs.to_string())
            }
            Err(_) => {
                drop(seen);
                fail(self, ctx, vec![("", ctx.t("err.generic").into())])
            }
        }
    }

    // ---- /shop/order/{token} --------------------------------------------------------------

    pub(crate) fn order_page(&self, ctx: &mut Ctx, token: &str) -> Resp {
        if !ctx.caps.shop {
            return self.not_found(ctx);
        }
        let src = self.source(ctx.req);
        if !self.allow(&format!("order:{src}"), 60, Duration::from_secs(600)) {
            return self
                .error(ctx, 429, "e429.title", "err.rate")
                .header("retry-after", "600");
        }
        if !valid_token(token) {
            return self.not_found(ctx);
        }
        let Ok(Some(rc)) = self.api.receipt("ecommerce_order", token) else {
            return self.not_found(ctx);
        };
        let o = &rc.record;
        let cur = match o.text("currency") {
            "" => ctx.s.shop_currency.clone(),
            c => c.to_string(),
        };
        let path = format!("/shop/order/{token}");
        let mut p = Page::new(ctx.t("order.title"), ctx.t("order.title"), &path);
        p.noindex = true;
        p.private = true;
        p.body_attrs = " data-clear-cart".into();
        p.extra_headers = vec![("referrer-policy".into(), "no-referrer".into())];
        let state = o.text("state");
        let state_label = match state {
            "pending" | "paid" | "shipped" | "delivered" | "cancelled" | "refunded" => {
                ctx.t(&format!("state.{state}"))
            }
            _ => "",
        };
        let mut h = format!(
            "<h1>{}</h1><p class=\"lede\">{}</p>",
            esc(ctx.t("order.title")),
            esc(&ctx.tf("order.next", &[("email", o.text("customer_email"))]))
        );
        h.push_str(&format!(
            "<div class=\"notice\">{}</div>",
            esc(ctx.t("order.keep"))
        ));
        h.push_str(&format!(
            "<h2>{}</h2><dl class=\"facts\"><dt>{}</dt><dd>{}</dd><dt>{}</dt><dd>{}</dd><dt>{}</dt><dd>{}</dd><dt>{}</dt><dd>{}</dd></dl>",
            esc(&ctx.tf("order.number", &[("number", o.text("number"))])),
            esc(ctx.t("order.status")),
            esc(state_label),
            esc(&ctx.tf("order.placed", &[("date", &ctx.date(o.text("placed_on")))])),
            "",
            esc(ctx.t("co.name")),
            esc(o.text("customer_name")),
            esc(ctx.t("order.ship_to")),
            esc(o.text("shipping_address")),
        ));
        let lines: Vec<&PublicRecord> = rc
            .children
            .iter()
            .find(|(m, _)| m == "ecommerce_order_line")
            .map(|(_, r)| r.iter().collect())
            .unwrap_or_default();
        let mut md_out = format!(
            "{}\n\n{}: {}\n",
            ctx.tf("order.number", &[("number", o.text("number"))]),
            ctx.t("order.status"),
            state_label
        );
        h.push_str(&format!(
            "<h2>{}</h2><ul class=\"lines\">",
            esc(ctx.t("order.items"))
        ));
        for l in lines {
            let q = l.int("quantity").unwrap_or(0);
            let sum = l.int("subtotal").unwrap_or(0);
            h.push_str(&format!(
                "<li class=\"line\"><div><div class=\"name\">{}</div><div class=\"each\">{} × {}</div></div><div class=\"sum\">{}</div></li>",
                esc(l.text("description")),
                q,
                esc(&ctx.money(l.int("unit_price").unwrap_or(0), &cur)),
                esc(&ctx.money(sum, &cur)),
            ));
            md_out.push_str(&format!(
                "- {} × {} = {}\n",
                l.text("description").replace(['[', ']'], ""),
                q,
                ctx.money(sum, &cur)
            ));
        }
        h.push_str("</ul>");
        h.push_str(&format!(
            "<dl class=\"totals\"><dt>{}</dt><dd>{}</dd><dt>{}</dt><dd>{}</dd><dt class=\"grand\">{}</dt><dd class=\"grand\">{}</dd></dl>",
            esc(ctx.t("cart.subtotal")),
            esc(&ctx.money(o.int("subtotal").unwrap_or(0), &cur)),
            esc(ctx.t("cart.shipping")),
            esc(&ctx.money(o.int("shipping_total").unwrap_or(0), &cur)),
            esc(ctx.t("cart.total")),
            esc(&ctx.money(o.int("total").unwrap_or(0), &cur)),
        ));
        md_out.push_str(&format!(
            "\n{}: {}\n",
            ctx.t("cart.total"),
            ctx.money(o.int("total").unwrap_or(0), &cur)
        ));
        h.push_str(&format!(
            "<p><a class=\"btn secondary\" href=\"{}\">{}</a></p>",
            esc(&ctx.lpath("/shop")),
            esc(ctx.t("cart.continue"))
        ));
        p.body = h;
        p.md = md_out;
        let r = layout::render(ctx, p);
        r.header("referrer-policy", "no-referrer")
    }
}

fn order_url(ctx: &Ctx, token: &str) -> String {
    if ctx.lang == ctx.s.default_lang {
        format!("/shop/order/{token}")
    } else {
        format!("/shop/order/{token}?lang={}", ctx.lang.code())
    }
}

/// `W20261016-K7Q2ZD`: the date and six random characters, so order numbers do not reveal how
/// many orders there are.
fn order_number() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let r = random_token();
    let rnd: String = (0..6)
        .map(|i| {
            let b = u8::from_str_radix(&r[i * 2..i * 2 + 2], 16).unwrap_or(0);
            ALPHABET[(b as usize) % ALPHABET.len()] as char
        })
        .collect();
    format!("W{}-{rnd}", i18n::today().replace('-', ""))
}

fn qpath(base: &str, keep: &[(&str, String)], page: usize) -> String {
    let mut q: Vec<String> = keep
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{k}={}", urlenc(v)))
        .collect();
    if page > 1 {
        q.push(format!("page={page}"));
    }
    if q.is_empty() {
        base.to_string()
    } else {
        format!("{base}?{}", q.join("&"))
    }
}
