//! The web shop: product grid and pages, the cart, checkout and the order confirmation.
//!
//! Prices, names and availability always come from the published listing on the server. The
//! browser sends only listing ids and quantities; there is no field a visitor could use to name
//! a price.

use crate::content::{describe, page_param, pager};
use crate::http::{Form, Resp};
use crate::i18n;
use crate::layout::{self, Ctx, Page, PageImage, Variants};
use crate::media::Pic;
use crate::seo;
use crate::settings::Settings;
use crate::util::{clean_line, esc, random_token, sha256_hex, urlenc};
use crate::{Site, md};
use obop_commands::{Lookup, PublicFilter, PublicRecord, Value};
use serde_json::json;
use std::collections::BTreeMap;

pub(crate) const MAX_LINES: usize = 50;
pub(crate) const MAX_QTY: i64 = 99;
pub(crate) const MAX_TOTAL: i64 = 100_000_000_000;

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
pub(crate) enum Avail {
    InStock,
    OutOfStock,
    Backorder,
}

pub(crate) fn availability(rec: &PublicRecord) -> Avail {
    let qty = rec.int("stock_quantity").unwrap_or(0);
    match rec.text("stock_policy") {
        "block_when_out" if qty <= 0 => Avail::OutOfStock,
        "allow_backorder" if qty <= 0 => Avail::Backorder,
        _ => Avail::InStock,
    }
}

/// Whether a listing can be ordered right now (home page features only these).
pub(crate) fn in_stock(rec: &PublicRecord) -> bool {
    availability(rec) != Avail::OutOfStock
}

fn schema_availability(a: Avail) -> &'static str {
    match a {
        Avail::InStock => "https://schema.org/InStock",
        Avail::OutOfStock => "https://schema.org/OutOfStock",
        Avail::Backorder => "https://schema.org/BackOrder",
    }
}

/// `1234` in two-digit currency as `12.34` for JSON-LD, whatever the visitor's language.
pub(crate) fn decimal(minor: i64, currency: &str) -> String {
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

pub(crate) fn pick<'a>(rec: &'a PublicRecord, tr: Option<&'a PublicRecord>, k: &str) -> &'a str {
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

/// `pic` is the card's picture markup when the listing has a public one (see `Site::card_pic`).
pub(crate) fn product_card(
    ctx: &Ctx,
    rec: &PublicRecord,
    tr: Option<&PublicRecord>,
    pic: Option<&str>,
) -> String {
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
    let art = match pic {
        Some(img) => format!("<div class=\"art media\">{img}</div>"),
        None => format!(
            "<div class=\"art t{}\" aria-hidden=\"true\">{}</div>",
            tint_of(&rec.id),
            esc(&monogram(title))
        ),
    };
    format!(
        "<li class=\"card\"{lang}>{art}{badge}<div class=\"body\"><h3><a href=\"{}\">{}</a></h3>{}<p>{}</p></div></li>",
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
pub(crate) fn id_ok(s: &str) -> bool {
    s.len() == 36 && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// The cart lines of a request. Quantities are kept as they were sent (a later check decides
/// what is acceptable); `bad` is set when one was not a whole number.
pub(crate) struct Parsed {
    pub lines: Vec<(String, i64)>,
    pub bad: bool,
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
pub(crate) fn parse_lines(pairs: &[(String, String)]) -> Parsed {
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

pub(crate) fn items_string(lines: &[(String, i64)]) -> String {
    lines
        .iter()
        .map(|(id, q)| format!("{id}.{q}"))
        .collect::<Vec<_>>()
        .join("~")
}

/// What a failed checkout shows again.
pub(crate) struct Redo {
    pub errors: Vec<(&'static str, String)>,
    pub form: Form,
}

impl Site {
    /// The picture of a listing card, when the listing has a public one.
    pub(crate) fn card_pic(&self, rec: &PublicRecord, title: &str) -> Option<String> {
        let pic = self.pic(rec.text("image"))?;
        let alt = match rec.text("image_alt") {
            "" => title,
            a => a,
        };
        Some(pic.html(alt, "(max-width: 600px) 50vw, 280px", false))
    }

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
                let t = tr.get(&r.id);
                let pic = self.card_pic(r, pick(r, t, "title"));
                h.push_str(&product_card(ctx, r, t, pic.as_deref()));
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
        // Pictures: the main one, then the gallery. Only public ones are used at all.
        let main_alt = match rec.text("image_alt") {
            "" => title.clone(),
            a => a.to_string(),
        };
        let mut pics: Vec<(Pic, String)> = Vec::new();
        if let Some(pic) = self.pic(rec.text("image")) {
            pics.push((pic, main_alt));
        }
        let gallery = self
            .api
            .list(
                "ecommerce_listing_image",
                &[(
                    "listing_id".to_string(),
                    PublicFilter::Eq(Value::text(&rec.id)),
                )],
                Some(20),
                0,
            )
            .map(|(r, _)| r)
            .unwrap_or_default();
        for g in &gallery {
            if let Some(pic) = self.pic(g.text("image")) {
                let alt = match g.text("alt") {
                    "" => title.clone(),
                    a => a.to_string(),
                };
                if !pics.iter().any(|(p, _)| p.id == pic.id) {
                    pics.push((pic, alt));
                }
            }
        }
        if let Some((first, alt)) = pics.first() {
            let (path, width, height) = first.social();
            p.image = Some(PageImage {
                path,
                width,
                height,
                alt: alt.clone(),
            });
        }
        if !pics.is_empty() {
            ld["image"] = json!(
                pics.iter()
                    .map(|(pic, _)| ctx.abs(&pic.social().0))
                    .collect::<Vec<_>>()
            );
        }
        p.jsonld.push(ld);

        let mut h = String::from("<article class=\"product\">");
        if let Some(((main, alt), rest)) = pics.split_first() {
            h.push_str(&format!(
                "<div class=\"gallery\"><figure class=\"main\"><a href=\"{}\">{}</a></figure>",
                esc(&main.path(None)),
                main.html(alt, "(max-width: 820px) 100vw, 560px", true)
            ));
            if !rest.is_empty() {
                h.push_str("<ul class=\"thumbs\">");
                for (pic, alt) in rest {
                    h.push_str(&format!(
                        "<li><a href=\"{}\">{}</a></li>",
                        esc(&pic.path(None)),
                        pic.html(alt, "(max-width: 820px) 33vw, 180px", false)
                    ));
                }
                h.push_str("</ul>");
            }
            h.push_str("</div><div>");
        } else {
            h.push_str(&format!(
                "<div class=\"art t{}\" role=\"img\" aria-label=\"{}\">{}</div><div>",
                tint_of(&rec.id),
                esc(ctx.t("shop.product_image")),
                esc(&monogram(&title))
            ));
        }
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
}

pub(crate) fn order_url(ctx: &Ctx, token: &str) -> String {
    if ctx.lang == ctx.s.default_lang {
        format!("/shop/order/{token}")
    } else {
        format!("/shop/order/{token}?lang={}", ctx.lang.code())
    }
}

/// `W20261016-K7Q2ZD`: the date and six random characters, so order numbers do not reveal how
/// many orders there are.
pub(crate) fn order_number() -> String {
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

pub(crate) fn qpath(base: &str, keep: &[(&str, String)], page: usize) -> String {
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
