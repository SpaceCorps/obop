//! The cart and checkout (D61): stock, discount codes, taxes and totals, always computed on the
//! server from the published listings, the active discount codes and tax rates the owner entered
//! and the chosen delivery method. The browser sends listing ids, quantities, the code text and
//! its choices (delivery method, country, how to pay); it never sends a number that is used.
//!
//! Stock, the code's use counters and the order are written in **one atomic batch**
//! (`PublicApi::submit_order`): two buyers cannot both take the last unit, a code cannot be used
//! past its maximum, and a refused checkout leaves nothing behind.

use crate::content::{Field, error_summary};
use crate::countries;
use crate::http::Resp;
use crate::i18n;
use crate::layout::{self, Ctx, Page};
use crate::pricing::{self, Discount, DiscountKind, LineIn, LineOut, Priced, Rate};
use crate::shop::{
    Avail, MAX_QTY, MAX_TOTAL, Redo, availability, currency_of, items_string, order_number,
    order_url, parse_lines, pick, product_path,
};
use crate::util::{clean_line, ct_eq, esc, now, random_token, sha256_hex, truncate_chars, urlenc};
use crate::{Site, content::email_ok};
use obop_commands::{
    Adjust, Lookup, PublicRecord, StartPayment, Started, Submission, SubmitError, Value,
};
use serde_json::{Value as J, json};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

type Errors = Vec<(&'static str, String)>;

/// Failed code attempts one address may make before codes stop being looked at for a while.
const CODE_FAILURES: usize = 10;
const CODE_WINDOW: Duration = Duration::from_secs(600);
/// How long an unpaid online payment holds its stock.
const ONLINE_HOLD_SECS: u64 = 45 * 60;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tracked {
    /// Always available: stock is not counted.
    No,
    /// Sold only while units are left.
    Block,
    /// Units are counted but selling past zero is allowed.
    Backorder,
}

pub(crate) struct QLine {
    pub rec: PublicRecord,
    pub tr: Option<PublicRecord>,
    pub qty: i64,
    pub unit: i64,
    pub category: Option<String>,
    tracked: Tracked,
    pub out: LineOut,
}

/// A discount code that passed every check.
pub(crate) struct Applied {
    id: String,
    pub text: String,
    discount: Discount,
    max_uses: Option<i64>,
    /// The per-customer counter this order counts against, with its limit.
    per_customer: Option<(String, i64, String)>,
}

pub(crate) enum CodeState {
    /// The buyer entered none.
    Absent,
    /// There is a code and it cannot be applied (any reason: the answer is always the same).
    Rejected,
    Applied(Box<Applied>),
}

pub(crate) struct Quote {
    pub cur: String,
    pub lines: Vec<QLine>,
    pub methods: Vec<PublicRecord>,
    pub shipping_id: Option<String>,
    pub country: String,
    pub code: CodeState,
    pub priced: Priced,
    /// Lines whose listing is gone.
    pub missing: usize,
}

pub(crate) struct Inputs<'a> {
    pub lines: &'a [(String, i64)],
    pub ship: &'a str,
    pub country: &'a str,
    pub code: &'a str,
    pub email: Option<&'a str>,
    /// Checkout: stock, quantities and the delivery method are enforced, not just displayed.
    pub strict: bool,
}

/// A discount code as typed: trimmed, upper case, letters, digits, dash and underscore.
pub(crate) fn normalise_code(raw: &str) -> Option<String> {
    let c = raw.trim().to_ascii_uppercase();
    ((3..=40).contains(&c.len())
        && c.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-' || b == b'_'))
    .then_some(c)
}

fn int_of(f: &obop_storage::Fields, k: &str) -> Option<i64> {
    f.get(k).and_then(J::as_i64)
}

fn text_of<'a>(f: &'a obop_storage::Fields, k: &str) -> &'a str {
    f.get(k).and_then(J::as_str).unwrap_or("")
}

fn country_code(raw: &str) -> String {
    let c = raw.trim().to_ascii_uppercase();
    if c.len() == 2 && c.bytes().all(|b| b.is_ascii_uppercase()) {
        c
    } else {
        String::new()
    }
}

/// `19%` or `7,5%`, with the decimal mark of the language.
pub(crate) fn percent(ctx: &Ctx, bp: i64) -> String {
    use crate::i18n::Lang;
    if bp % 100 == 0 {
        return format!("{}%", bp / 100);
    }
    let mark = if ctx.lang == Lang::En { '.' } else { ',' };
    let s = format!("{}{mark}{:02}", bp / 100, bp % 100);
    format!("{}%", s.trim_end_matches('0'))
}

fn minus(ctx: &Ctx, amount: i64, cur: &str) -> String {
    format!("\u{2212}{}", ctx.money(amount, cur))
}

impl Site {
    // ---- inputs the owner entered ---------------------------------------------------------

    fn tax_rates(&self) -> Vec<Rate> {
        self.api
            .server_records("ecommerce_tax_rate")
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, f)| f.get("active").and_then(J::as_bool) == Some(true))
            .filter_map(|(_, f)| {
                let name = clean_line(text_of(&f, "name"));
                let bp = int_of(&f, "rate_bp")?;
                let raw_country = text_of(&f, "country").trim().to_ascii_uppercase();
                let country = country_code(&raw_country);
                if name.is_empty()
                    || !(0..=10_000).contains(&bp)
                    || (!raw_country.is_empty() && country.is_empty())
                {
                    return None;
                }
                Some(Rate {
                    name: truncate_chars(&name, 40),
                    bp,
                    country,
                    category: Some(text_of(&f, "category_id"))
                        .filter(|c| !c.is_empty())
                        .map(String::from),
                    included: f.get("included").and_then(J::as_bool) == Some(true),
                })
            })
            .collect()
    }

    /// The countries that have a tax rate of their own, sorted.
    fn rate_countries(rates: &[Rate]) -> Vec<String> {
        let mut c: Vec<String> = rates
            .iter()
            .filter(|r| !r.country.is_empty())
            .map(|r| r.country.clone())
            .collect();
        c.sort();
        c.dedup();
        c
    }

    fn failed_codes_blocked(&self, src: &str) -> bool {
        self.limited(&format!("code:{src}"), CODE_FAILURES, CODE_WINDOW)
    }

    fn note_failed_code(&self, src: &str) {
        self.record(&format!("code:{src}"));
    }

    /// Look a code up and check it against this cart. Every reason to refuse gives the same
    /// answer, so the response says nothing about which codes exist.
    fn find_code(
        &self,
        raw: &str,
        lines: &[QLine],
        subtotal: i64,
        email: Option<&str>,
    ) -> CodeState {
        let Some(text) = normalise_code(raw) else {
            return CodeState::Rejected;
        };
        let Ok(records) = self.api.server_records("ecommerce_discount_code") else {
            return CodeState::Rejected;
        };
        // Look at every record, the same way, whether or not an earlier one matched.
        let mut found = None;
        for (id, f) in records {
            if ct_eq(&text, &text_of(&f, "code").trim().to_ascii_uppercase()) {
                found = Some((id, f));
            }
        }
        let Some((id, f)) = found else {
            return CodeState::Rejected;
        };
        let today = i18n::today();
        let (kind, value) = (text_of(&f, "kind"), int_of(&f, "value").unwrap_or(0));
        let ok = f.get("active").and_then(J::as_bool) == Some(true)
            && match kind {
                "percent" => (1..=10_000).contains(&value),
                "fixed" => value >= 1,
                _ => false,
            }
            && {
                let s = text_of(&f, "starts_on");
                s.is_empty() || s <= today.as_str()
            }
            && {
                let e = text_of(&f, "ends_on");
                e.is_empty() || e >= today.as_str()
            }
            && int_of(&f, "max_uses").is_none_or(|m| int_of(&f, "used_count").unwrap_or(0) < m)
            && int_of(&f, "minimum_order").is_none_or(|m| subtotal >= m);
        if !ok {
            return CodeState::Rejected;
        }
        let category = Some(text_of(&f, "category_id"))
            .filter(|c| !c.is_empty())
            .map(String::from);
        if let Some(c) = &category
            && !lines.iter().any(|l| l.category.as_deref() == Some(c))
        {
            return CodeState::Rejected;
        }
        // Per customer: the counter is keyed by a hash of the code and the email address, so the
        // address itself is not kept.
        let mut per_customer = None;
        if let Some(limit) = int_of(&f, "per_customer_limit").filter(|l| *l >= 1)
            && let Some(email) = email
        {
            let hash = sha256_hex(&format!(
                "discount-use:{id}:{}",
                email.trim().to_lowercase()
            ));
            let use_id = format!("{id}:{}", &hash[..32]);
            let used = self
                .api
                .server_record("ecommerce_discount_use", &use_id)
                .ok()
                .flatten()
                .and_then(|r| int_of(&r, "count"))
                .unwrap_or(0);
            if used >= limit {
                return CodeState::Rejected;
            }
            per_customer = Some((use_id, limit, hash[..32].to_string()));
        }
        CodeState::Applied(Box::new(Applied {
            id,
            text,
            discount: Discount {
                kind: if kind == "percent" {
                    DiscountKind::Percent
                } else {
                    DiscountKind::Fixed
                },
                value,
                category,
            },
            max_uses: int_of(&f, "max_uses"),
            per_customer,
        }))
    }

    // ---- the quote ------------------------------------------------------------------------

    /// Price a cart. In `strict` mode (checkout) a problem is an error; otherwise the cart is
    /// shown as far as it can be priced.
    pub(crate) fn quote(&self, ctx: &Ctx, inp: &Inputs) -> (Quote, Errors) {
        let mut errors: Errors = Vec::new();
        let tr = self.shop_translations(ctx);
        let mut lines: Vec<QLine> = Vec::new();
        let mut currency: Option<String> = None;
        let mut subtotal = 0i64;
        let mut missing = 0;
        for (id, qty) in inp.lines {
            if !(1..=MAX_QTY).contains(qty) {
                errors.push(("", ctx.t("err.quantity").into()));
                break;
            }
            let rec = match self.api.get("ecommerce_listing", id) {
                Ok(Lookup::Found(r)) => r,
                _ => {
                    missing += 1;
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
            let tracked = match rec.text("stock_policy") {
                "block_when_out" => Tracked::Block,
                "allow_backorder" => Tracked::Backorder,
                _ => Tracked::No,
            };
            if inp.strict {
                let left = rec.int("stock_quantity").unwrap_or(0);
                match availability(&rec) {
                    Avail::OutOfStock => {
                        errors.push(("", ctx.tf("err.unavailable", &[("name", &title)])));
                        continue;
                    }
                    Avail::InStock if tracked == Tracked::Block && *qty > left => {
                        errors.push((
                            "",
                            ctx.tf(
                                "err.stock_left",
                                &[("name", &title), ("n", &left.max(0).to_string())],
                            ),
                        ));
                        continue;
                    }
                    _ => {}
                }
            }
            let cur = currency_of(&rec, &ctx.s);
            if currency.get_or_insert_with(|| cur.clone()) != &cur {
                errors.push(("", ctx.t("err.currency").into()));
                continue;
            }
            let Some(gross) = unit.checked_mul(*qty) else {
                errors.push(("", ctx.t("err.generic").into()));
                continue;
            };
            subtotal = match subtotal.checked_add(gross) {
                Some(s) if s <= MAX_TOTAL => s,
                _ => {
                    errors.push(("", ctx.t("err.generic").into()));
                    continue;
                }
            };
            let category =
                match self
                    .api
                    .published_server_field("ecommerce_listing", &rec.id, "category_id")
                {
                    Ok(Some(J::String(c))) if !c.is_empty() => Some(c),
                    _ => None,
                };
            lines.push(QLine {
                tr: tr.get(&rec.id).cloned(),
                rec,
                qty: *qty,
                unit,
                category,
                tracked,
                out: LineOut {
                    gross,
                    discount: 0,
                    tax_name: String::new(),
                    tax_bp: 0,
                    tax: 0,
                    payable: gross,
                },
            });
        }
        let cur = currency.unwrap_or_else(|| ctx.s.shop_currency.clone());

        // Delivery.
        let methods = self
            .api
            .list("ecommerce_shipping_method", &[], Some(20), 0)
            .map(|(r, _)| r)
            .unwrap_or_default();
        let (shipping_id, shipping) = if methods.is_empty() {
            (None, 0)
        } else {
            let chosen = methods
                .iter()
                .find(|m| m.id == inp.ship)
                .or_else(|| (!inp.strict).then(|| methods.first()).flatten());
            match chosen {
                Some(m) => (
                    Some(m.id.clone()),
                    match m.int("free_over") {
                        Some(f) if subtotal >= f => 0,
                        _ => m.int("price").unwrap_or(0).max(0),
                    },
                ),
                None => {
                    errors.push(("", ctx.t("err.shipping").into()));
                    (None, 0)
                }
            }
        };

        // The discount code.
        let code = if inp.code.trim().is_empty() {
            CodeState::Absent
        } else {
            self.find_code(inp.code, &lines, subtotal, inp.email)
        };
        if inp.strict && matches!(code, CodeState::Rejected) {
            errors.push(("code", ctx.t("code.invalid").into()));
        }

        // Arithmetic.
        let country = country_code(inp.country);
        let rates = self.tax_rates();
        let discount = match &code {
            CodeState::Applied(a) => Some(a.discount.clone()),
            _ => None,
        };
        let line_in: Vec<LineIn> = lines
            .iter()
            .map(|l| LineIn {
                gross: l.out.gross,
                category: l.category.clone(),
            })
            .collect();
        let priced = match pricing::price(&line_in, discount.as_ref(), shipping, &rates, &country) {
            Ok(p) if p.total <= MAX_TOTAL => p,
            _ => {
                errors.push(("", ctx.t("err.generic").into()));
                pricing::price(&[], None, 0, &[], "").expect("an empty order has a price")
            }
        };
        for (l, o) in lines.iter_mut().zip(&priced.lines) {
            l.out = o.clone();
        }
        (
            Quote {
                cur,
                lines,
                methods,
                shipping_id,
                country,
                code,
                priced,
                missing,
            },
            errors,
        )
    }

    // ---- /cart ----------------------------------------------------------------------------

    /// The totals block: the same markup for the cart page and for the fragment the script
    /// fetches when the delivery method or the country changes.
    fn totals_html(&self, ctx: &Ctx, q: &Quote) -> String {
        let p = &q.priced;
        let cur = &q.cur;
        let mut h = String::from("<dl class=\"totals\" data-totals>");
        h.push_str(&format!(
            "<dt>{}</dt><dd>{}</dd>",
            esc(ctx.t("cart.subtotal")),
            esc(&ctx.money(p.subtotal, cur))
        ));
        if let CodeState::Applied(a) = &q.code
            && p.discount_total > 0
        {
            h.push_str(&format!(
                "<dt>{}</dt><dd>{}</dd>",
                esc(&ctx.tf("cart.discount_line", &[("code", &a.text)])),
                esc(&minus(ctx, p.discount_total, cur))
            ));
        }
        h.push_str(&format!(
            "<dt>{}</dt><dd>{}</dd>",
            esc(ctx.t("cart.shipping")),
            if q.methods.is_empty() {
                "—".to_string()
            } else if p.shipping == 0 {
                esc(ctx.t("cart.free"))
            } else {
                esc(&ctx.money(p.shipping, cur))
            }
        ));
        for t in &p.tax_lines {
            let key = if t.included {
                "cart.tax_included"
            } else {
                "cart.tax_added"
            };
            h.push_str(&format!(
                "<dt class=\"tax\">{}</dt><dd class=\"tax\">{}</dd>",
                esc(&ctx.tf(key, &[("name", &t.name), ("rate", &percent(ctx, t.bp))])),
                esc(&ctx.money(t.amount, cur))
            ));
        }
        h.push_str(&format!(
            "<dt class=\"grand\">{}</dt><dd class=\"grand\">{}</dd></dl>",
            esc(ctx.t("cart.total")),
            esc(&ctx.money(p.total, cur))
        ));
        h
    }

    pub(crate) fn cart(&self, ctx: &mut Ctx, redo: Option<Redo>) -> Resp {
        if !ctx.caps.shop {
            return self.not_found(ctx);
        }
        let pairs: Vec<(String, String)> = match &redo {
            Some(r) => r.form.0.clone(),
            None => ctx.req.query_pairs(),
        };
        let get = |k: &str| {
            pairs
                .iter()
                .find(|(n, _)| n == k)
                .map_or("", |(_, v)| v.as_str())
                .to_string()
        };
        // On the cart page a quantity of 0 removes a line and anything else is clamped.
        let lines: Vec<(String, i64)> = parse_lines(&pairs)
            .lines
            .into_iter()
            .filter(|(_, q)| *q >= 1)
            .map(|(id, q)| (id, q.min(MAX_QTY)))
            .collect();
        let src = self.source(ctx.req);
        let ship = if redo.is_some() {
            get("shipping_method_id")
        } else {
            get("ship")
        };
        let rates = self.tax_rates();
        let countries = Self::rate_countries(&rates);
        let country = {
            let c = get("country");
            if redo.is_some() || !c.is_empty() {
                c
            } else {
                ctx.s.default_country.clone()
            }
        };
        let blocked = !get("code").trim().is_empty() && self.failed_codes_blocked(&src);
        let code_in = if blocked { String::new() } else { get("code") };
        let (q, _) = self.quote(
            ctx,
            &Inputs {
                lines: &lines,
                ship: &ship,
                country: &country,
                code: &code_in,
                email: None,
                strict: false,
            },
        );
        let rejected = matches!(q.code, CodeState::Rejected);
        if rejected {
            self.note_failed_code(&src);
        }

        // The script asks for just the totals when a choice changes.
        if ctx.req.q("part").as_deref() == Some("totals") && redo.is_none() {
            let r = Resp::new(200, "text/html; charset=utf-8", self.totals_html(ctx, &q))
                .header("cache-control", "private, no-store");
            return layout::security_headers(r, ctx);
        }

        let live: Vec<(String, i64)> = q.lines.iter().map(|l| (l.rec.id.clone(), l.qty)).collect();
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
        let errors: Errors = redo.as_ref().map(|r| r.errors.clone()).unwrap_or_default();
        if q.missing > 0 && redo.is_none() {
            h.push_str(&format!(
                "<div class=\"notice\" role=\"status\">{}</div>",
                esc(&ctx.tf("err.unavailable", &[("name", "…")]))
            ));
        }
        if q.lines.is_empty() {
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
        let cur = q.cur.clone();
        let mixed = errors.iter().any(|(_, m)| m == ctx.t("err.currency"));

        h.push_str(&error_summary(ctx, &errors));
        h.push_str("<div class=\"cartgrid\"><section aria-labelledby=\"lines\">");
        h.push_str(&format!(
            "<h2 id=\"lines\" class=\"sr\">{}</h2>",
            esc(ctx.t("order.items"))
        ));
        let lang_hidden = if ctx.lang == ctx.s.default_lang {
            String::new()
        } else {
            format!(
                "<input type=\"hidden\" name=\"lang\" value=\"{}\">",
                ctx.lang.code()
            )
        };
        h.push_str("<form method=\"get\" action=\"/cart\"><ul class=\"lines\">");
        for (n, l) in q.lines.iter().enumerate() {
            let title = pick(&l.rec, l.tr.as_ref(), "title");
            let mut without = live.clone();
            without.remove(n);
            let remove_href = format!("/cart?items={}{}", urlenc(&items_string(&without)), lang_q);
            md_out.push_str(&format!(
                "- {} × {} = {}\n",
                title.replace(['[', ']'], ""),
                l.qty,
                ctx.money(l.out.gross, &cur)
            ));
            h.push_str(&format!(
                "<li class=\"line\"><div><div class=\"name\"><a href=\"{href}\">{title}</a></div><div class=\"each\">{each}</div>\
<div class=\"qty\"><input type=\"hidden\" name=\"id\" value=\"{id}\"><label class=\"sr\" for=\"qty-{n}\">{ql}</label>\
<input type=\"number\" id=\"qty-{n}\" name=\"qty\" min=\"0\" max=\"99\" value=\"{q}\" inputmode=\"numeric\"> <a href=\"{rm}\" aria-label=\"{rml}\">{rmt}</a></div></div>\
<div class=\"sum\">{sum}</div></li>",
                href = esc(&ctx.lpath(&product_path(&l.rec))),
                title = esc(title),
                each = esc(&ctx.money(l.unit, &cur)),
                id = esc(&l.rec.id),
                ql = esc(&format!("{}: {}", ctx.t("shop.qty"), title)),
                q = l.qty,
                rm = esc(&remove_href),
                rml = esc(&ctx.tf("cart.remove_item", &[("name", title)])),
                rmt = esc(ctx.t("cart.remove")),
                sum = esc(&ctx.money(l.out.gross, &cur)),
            ));
        }
        h.push_str(&format!(
            "</ul>{}{}{}<button class=\"btn secondary small\" type=\"submit\">{}</button></form>",
            lang_hidden,
            if ship.is_empty() {
                String::new()
            } else {
                format!(
                    "<input type=\"hidden\" name=\"ship\" value=\"{}\">",
                    esc(&ship)
                )
            },
            if q.country.is_empty() {
                String::new()
            } else {
                format!(
                    "<input type=\"hidden\" name=\"country\" value=\"{}\">",
                    esc(&q.country)
                )
            } + &if matches!(q.code, CodeState::Applied(_)) {
                format!(
                    "<input type=\"hidden\" name=\"code\" value=\"{}\">",
                    esc(&code_in.trim().to_ascii_uppercase())
                )
            } else {
                String::new()
            },
            esc(ctx.t("cart.update"))
        ));
        // The discount code: a plain form, so it works without the script.
        let applied_code = match &q.code {
            CodeState::Applied(a) => Some(a.text.clone()),
            _ => None,
        };
        h.push_str(&format!(
            "<form class=\"codeform\" method=\"get\" action=\"/cart\"><input type=\"hidden\" name=\"items\" value=\"{}\">{}{}{}\
<div class=\"field\"><label for=\"code\">{}</label><div class=\"inline\"><input type=\"text\" id=\"code\" name=\"code\" maxlength=\"40\" value=\"{}\" autocomplete=\"off\" autocapitalize=\"characters\" spellcheck=\"false\"><button class=\"btn secondary small\" type=\"submit\">{}</button></div></div>",
            esc(&items_string(&live)),
            lang_hidden,
            if ship.is_empty() { String::new() } else { format!("<input type=\"hidden\" name=\"ship\" value=\"{}\">", esc(&ship)) },
            if q.country.is_empty() { String::new() } else { format!("<input type=\"hidden\" name=\"country\" value=\"{}\">", esc(&q.country)) },
            esc(ctx.t("cart.discount")),
            esc(&code_in),
            esc(ctx.t("cart.discount_apply")),
        ));
        if let Some(c) = &applied_code {
            h.push_str(&format!(
                "<p class=\"codemsg ok\" role=\"status\">{}</p>",
                esc(&ctx.tf("code.applied", &[("code", c)]))
            ));
        } else if rejected && !blocked {
            h.push_str(&format!(
                "<p class=\"codemsg\" role=\"status\">{}</p>",
                esc(ctx.t("code.invalid"))
            ));
        } else if blocked {
            h.push_str(&format!(
                "<p class=\"codemsg\" role=\"status\">{}</p>",
                esc(ctx.t("code.rate"))
            ));
        }
        h.push_str("</form>");
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
                esc(ctx.t("cart.more")),
                esc(&items_string(&live)),
                lang_hidden,
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
            "<section aria-labelledby=\"co\"><form class=\"formcard\" method=\"post\" action=\"/shop/checkout\" data-checkout novalidate>\
<h2 id=\"co\">{}</h2>\
<input type=\"hidden\" name=\"csrf\" value=\"{}\"><input type=\"hidden\" name=\"idem\" value=\"{}\"><input type=\"hidden\" name=\"lang\" value=\"{}\">\
<input type=\"hidden\" name=\"code\" value=\"{}\">",
            esc(ctx.t("co.title")),
            esc(&ctx.csrf),
            esc(&random_token()[..32]),
            ctx.lang.code(),
            esc(applied_code.as_deref().unwrap_or("")),
        ));
        for (id, n) in &live {
            h.push_str(&format!(
                "<input type=\"hidden\" name=\"id\" value=\"{}\"><input type=\"hidden\" name=\"qty\" value=\"{n}\">",
                esc(id)
            ));
        }
        let form = redo.as_ref().map(|r| &r.form);
        let formv = |k: &str| form.map_or("", |f| f.get(k));
        let bad = |id: &str| errors.iter().any(|(e, _)| *e == id);
        let f = |id, label, kind, auto, max| Field {
            id,
            label,
            kind,
            value: formv(id),
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
        if !countries.is_empty() {
            h.push_str(&format!(
                "<div class=\"field\"><label for=\"country\">{}</label><select id=\"country\" name=\"country\" autocomplete=\"country\">",
                esc(ctx.t("co.country"))
            ));
            for c in &countries {
                h.push_str(&format!(
                    "<option value=\"{c}\"{}>{}</option>",
                    if *c == q.country { " selected" } else { "" },
                    esc(&countries::name(ctx.lang, c))
                ));
            }
            h.push_str(&format!(
                "<option value=\"\"{}>{}</option></select></div>",
                if q.country.is_empty() {
                    " selected"
                } else {
                    ""
                },
                esc(ctx.t("co.country_other"))
            ));
        }
        if !q.methods.is_empty() {
            h.push_str(&format!(
                "<fieldset><legend>{}</legend>",
                esc(ctx.t("co.shipping"))
            ));
            for m in &q.methods {
                let price = match m.int("free_over") {
                    Some(fo) if q.priced.subtotal >= fo => 0,
                    _ => m.int("price").unwrap_or(0),
                };
                h.push_str(&format!(
                    "<label class=\"opt\"><input type=\"radio\" name=\"shipping_method_id\" value=\"{id}\"{checked}> <span class=\"grow\">{name}{days}</span> <strong>{price}</strong></label>",
                    id = esc(&m.id),
                    checked = if q.shipping_id.as_ref() == Some(&m.id) { " checked" } else { "" },
                    name = esc(m.text("name")),
                    days = m.int("delivery_days").map_or(String::new(), |d| format!(" <span class=\"muted\">({})</span>", esc(&ctx.tf("cart.days", &[("n", &d.to_string())])))),
                    price = if price == 0 { esc(ctx.t("cart.free")) } else { esc(&ctx.money(price, &cur)) },
                ));
            }
            h.push_str("</fieldset>");
        }
        let offered = self.offered_methods();
        if offered.len() > 1 {
            let chosen = Some(formv("payment_method")).filter(|m| offered.iter().any(|o| o == m));
            h.push_str(&format!(
                "<fieldset><legend>{}</legend>",
                esc(ctx.t("co.payment"))
            ));
            for (i, m) in offered.iter().enumerate() {
                let on = match chosen {
                    Some(c) => c == m,
                    None => i == 0,
                };
                h.push_str(&format!(
                    "<label class=\"opt\"><input type=\"radio\" name=\"payment_method\" value=\"{m}\"{}> <span class=\"grow\">{}</span></label>",
                    if on { " checked" } else { "" },
                    esc(ctx.t(&format!("pay.method.{m}")))
                ));
            }
            h.push_str("</fieldset>");
        }
        h.push_str(&self.totals_html(ctx, &q));
        h.push_str(&format!(
            "<div class=\"hp\" aria-hidden=\"true\"><label for=\"website\">{}</label><input type=\"text\" id=\"website\" name=\"website\" tabindex=\"-1\" autocomplete=\"off\"></div>",
            esc(ctx.t("co.honeypot"))
        ));
        let note = if self.payments.is_none() || offered.iter().all(|m| m == "manual") {
            if ctx.s.payment_instructions.is_empty() {
                ctx.t("cart.no_payment").to_string()
            } else {
                ctx.t("cart.pay_by_transfer").to_string()
            }
        } else {
            String::new()
        };
        h.push_str(&format!(
            "<p class=\"muted\">{}{}</p><button class=\"btn\" type=\"submit\"{}>{}</button></form></section></div>",
            esc(&note),
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
            "\n{}: {}\n",
            ctx.t("cart.subtotal"),
            ctx.money(q.priced.subtotal, &cur)
        ));
        if q.priced.discount_total > 0 {
            md_out.push_str(&format!(
                "{}: {}\n",
                ctx.t("order.discount"),
                minus(ctx, q.priced.discount_total, &cur)
            ));
        }
        md_out.push_str(&format!(
            "{}: {}\n",
            ctx.t("cart.total"),
            ctx.money(q.priced.total, &cur)
        ));
        if redo.is_some() {
            p.status = 400;
        }
        p.body = h;
        p.md = md_out;
        layout::render(ctx, p)
    }

    /// The payment methods checkout offers: those the owner listed that can be used now.
    pub(crate) fn offered_methods(&self) -> Vec<String> {
        let listed = &self.settings_snapshot().payment_methods;
        match &self.payments {
            Some(p) => p.methods(listed),
            // Without a payment module only the bank-transfer / "we will contact you" flow exists.
            None => vec!["manual".to_string()],
        }
    }

    // ---- POST /shop/discount/check --------------------------------------------------------

    /// Does this code apply to this cart? Answered with JSON. Whatever is wrong with a code, the
    /// message is the same, and failed attempts are limited per address.
    pub(crate) fn discount_check(&self, ctx: &mut Ctx) -> Resp {
        if !ctx.caps.shop {
            return self.not_found(ctx);
        }
        let form = ctx.req.form();
        if let Err(r) = self.check_post(ctx, &form) {
            return r;
        }
        let src = self.source(ctx.req);
        let reply = |v: J| {
            layout::security_headers(
                Resp::new(200, "application/json", v.to_string())
                    .header("cache-control", "private, no-store"),
                ctx,
            )
        };
        if self.failed_codes_blocked(&src) {
            return reply(json!({"ok": false, "message": ctx.t("code.rate")}));
        }
        let parsed = parse_lines(&[("items".to_string(), form.get("items").to_string())]);
        let lines: Vec<(String, i64)> = parsed
            .lines
            .into_iter()
            .filter(|(_, q)| (1..=MAX_QTY).contains(q))
            .collect();
        let (q, _) = self.quote(
            ctx,
            &Inputs {
                lines: &lines,
                ship: form.get("ship"),
                country: form.get("country"),
                code: form.get("code"),
                email: None,
                strict: false,
            },
        );
        match &q.code {
            CodeState::Applied(a) => reply(json!({
                "ok": true,
                "message": ctx.tf("code.applied", &[("code", &a.text)]),
                "discount": q.priced.discount_total,
                "total": q.priced.total,
            })),
            _ => {
                self.note_failed_code(&src);
                reply(json!({"ok": false, "message": ctx.t("code.invalid")}))
            }
        }
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
        if let Some(p) = &self.payments {
            p.sweep();
        }
        let fail = |site: &Site, ctx: &mut Ctx, errors: Errors| {
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
        let mut errors: Errors = Vec::new();
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

        // Attempts at codes are limited like on the cart page.
        let code_text = form.get("code").trim().to_string();
        if !code_text.is_empty() && self.failed_codes_blocked(&src) {
            errors.push(("code", ctx.t("code.rate").into()));
        }

        let offered = self.offered_methods();
        let method = match form.get("payment_method") {
            "" if offered.len() == 1 => offered[0].clone(),
            "" => offered.first().cloned().unwrap_or_default(),
            m if offered.iter().any(|o| o == m) => m.to_string(),
            _ => {
                errors.push(("", ctx.t("err.payment").into()));
                String::new()
            }
        };

        // Price every line from the published listing. Nothing here comes from the browser but
        // the ids, the quantities, the delivery method, the country and the code text.
        let (q, qerrors) = self.quote(
            ctx,
            &Inputs {
                lines: &lines,
                ship: form.get("shipping_method_id"),
                country: form.get("country"),
                code: &code_text,
                email: Some(&email),
                strict: true,
            },
        );
        errors.extend(qerrors);
        if !errors.is_empty() {
            if errors.iter().any(|(f, _)| *f == "code") {
                self.note_failed_code(&src);
            }
            return fail(self, ctx, errors);
        }

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

        let p = &q.priced;
        let token = random_token();
        let number = order_number();
        let hold = if method == "manual" {
            u64::from(ctx.s.payment_hold_days) * 86_400
        } else {
            ONLINE_HOLD_SECS
        };
        let mut order_visitor = BTreeMap::from([
            ("customer_email".to_string(), Value::text(&email)),
            ("customer_name".to_string(), Value::text(&name)),
            ("shipping_address".to_string(), Value::text(&address)),
        ]);
        if let Some(id) = &q.shipping_id {
            order_visitor.insert("shipping_method_id".into(), Value::text(id));
        }
        if !q.country.is_empty() {
            order_visitor.insert("country".into(), Value::text(&q.country));
        }
        let tax_summary = J::Array(
            p.tax_lines
                .iter()
                .map(|t| json!({"name": t.name, "rate_bp": t.bp, "included": t.included, "amount": t.amount}))
                .collect(),
        );
        let mut order_server = BTreeMap::from([
            ("number".to_string(), Value::text(&number)),
            ("placed_on".to_string(), Value::text(&i18n::today())),
            ("currency".to_string(), Value::text(&q.cur)),
            ("subtotal".to_string(), Value::Int(p.subtotal)),
            ("shipping_total".to_string(), Value::Int(p.shipping)),
            ("discount_total".to_string(), Value::Int(p.discount_total)),
            ("tax_total".to_string(), Value::Int(p.tax_total)),
            (
                "tax_summary".to_string(),
                Value::Text(tax_summary.to_string()),
            ),
            ("total".to_string(), Value::Int(p.total)),
            ("access_token".to_string(), Value::text(&token)),
            ("stock_returned".to_string(), Value::Bool(false)),
            (
                "reserved_until".to_string(),
                Value::Int((now() + hold) as i64),
            ),
            ("payment_method".to_string(), Value::text(&method)),
        ]);
        let mut adjust: Vec<Adjust> = Vec::new();
        if let CodeState::Applied(a) = &q.code {
            order_server.insert("discount_code_id".into(), Value::text(&a.id));
            order_server.insert("discount_code".into(), Value::text(&a.text));
            adjust.push(Adjust {
                model: "ecommerce_discount_code".into(),
                id: a.id.clone(),
                field: "used_count".into(),
                delta: 1,
                min: None,
                max: a.max_uses,
                create: None,
            });
            if let Some((use_id, limit, hash)) = &a.per_customer {
                order_server.insert("discount_use_id".into(), Value::text(use_id));
                adjust.push(Adjust {
                    model: "ecommerce_discount_use".into(),
                    id: use_id.clone(),
                    field: "count".into(),
                    delta: 1,
                    min: None,
                    max: Some(*limit),
                    create: Some(BTreeMap::from([
                        ("code_id".to_string(), Value::text(&a.id)),
                        ("customer_hash".to_string(), Value::text(hash)),
                    ])),
                });
            }
        }
        let first_stock = adjust.len();
        let mut batch = vec![Submission {
            model: "ecommerce_order".into(),
            visitor: order_visitor,
            server: order_server,
            links: vec![],
        }];
        for l in &q.lines {
            let mut server = BTreeMap::from([
                (
                    "description".to_string(),
                    Value::text(&truncate_chars(l.rec.text("title"), 200)),
                ),
                ("unit_price".to_string(), Value::Int(l.unit)),
                ("subtotal".to_string(), Value::Int(l.out.gross)),
                ("discount_amount".to_string(), Value::Int(l.out.discount)),
                ("tax_amount".to_string(), Value::Int(l.out.tax)),
                ("tax_rate_bp".to_string(), Value::Int(l.out.tax_bp)),
            ]);
            if !l.out.tax_name.is_empty() {
                server.insert("tax_name".into(), Value::text(&l.out.tax_name));
            }
            if let Ok(Some(J::String(pid))) =
                self.api
                    .published_server_field("ecommerce_listing", &l.rec.id, "product_id")
            {
                server.insert("product_id".into(), Value::text(&pid));
            }
            batch.push(Submission {
                model: "ecommerce_order_line".into(),
                visitor: BTreeMap::from([
                    ("listing_id".to_string(), Value::text(&l.rec.id)),
                    ("quantity".to_string(), Value::Int(l.qty)),
                ]),
                server,
                links: vec![("order_id".into(), 0)],
            });
            if l.tracked != Tracked::No {
                adjust.push(Adjust {
                    model: "ecommerce_listing".into(),
                    id: l.rec.id.clone(),
                    field: "stock_quantity".into(),
                    delta: -l.qty,
                    min: (l.tracked == Tracked::Block).then_some(0),
                    max: None,
                    create: None,
                });
            }
        }
        let ids = match self.api.submit_order(&src, batch, &adjust) {
            Ok(ids) => ids,
            Err(SubmitError::RateLimited { retry_after_secs }) => {
                drop(seen);
                return self
                    .error(ctx, 429, "e429.title", "err.rate")
                    .header("retry-after", &retry_after_secs.to_string());
            }
            Err(SubmitError::Unavailable { adjust: i }) => {
                drop(seen);
                // A code that was used up a moment ago, or stock that went to someone else.
                let msg = match adjust.get(i) {
                    Some(a) if a.model == "ecommerce_listing" => {
                        let (name, left) = self
                            .api
                            .get("ecommerce_listing", &a.id)
                            .ok()
                            .and_then(|l| match l {
                                Lookup::Found(r) => Some((
                                    r.text("title").to_string(),
                                    r.int("stock_quantity").unwrap_or(0).max(0),
                                )),
                                _ => None,
                            })
                            .unwrap_or_default();
                        if left > 0 {
                            ctx.tf(
                                "err.stock_left",
                                &[("name", &name), ("n", &left.to_string())],
                            )
                        } else {
                            ctx.tf("err.unavailable", &[("name", &name)])
                        }
                    }
                    _ => {
                        self.note_failed_code(&src);
                        ctx.t("code.invalid").to_string()
                    }
                };
                let field = if i < first_stock { "code" } else { "" };
                return fail(self, ctx, vec![(field, msg)]);
            }
            Err(_) => {
                drop(seen);
                return fail(self, ctx, vec![("", ctx.t("err.generic").into())]);
            }
        };
        let order_id = ids[0].clone();

        // Payment. On failure the payment module has already cancelled the order and put the
        // stock and the code back, so the buyer can simply try again.
        let started = match &self.payments {
            Some(port) => port.start(&StartPayment {
                order_id: order_id.clone(),
                method: method.clone(),
                amount: p.total,
                currency: q.cur.clone(),
                number: number.clone(),
                return_url: ctx.abs(&format!("/shop/pay/{token}")),
                cancel_url: ctx.abs(&format!("/shop/pay/{token}?s=cancel")),
            }),
            None => Ok(Started::Manual),
        };
        let started = match started {
            Ok(s) => s,
            Err(msg) => {
                drop(seen);
                return fail(self, ctx, vec![("", msg)]);
            }
        };
        if let Some(k) = idem_key {
            seen.insert(k, (token.clone(), Instant::now()));
        }
        drop(seen);
        match started {
            Started::Manual => Resp::redirect(303, &order_url(ctx, &token)),
            Started::Redirect(url) if redirect_ok(&url) => Resp::redirect(303, &url),
            Started::Redirect(_) => Resp::redirect(303, &order_url(ctx, &token)),
        }
    }
}

/// Where a payment module may send the buyer: its hosted page over https, a path on this site
/// (the demo provider) or a loopback address (local development). Never anything else.
pub(crate) fn redirect_ok(url: &str) -> bool {
    let plain = !url
        .chars()
        .any(|c| c.is_control() || c.is_whitespace() || "\"'<>\\".contains(c));
    plain
        && (url.starts_with("https://")
            || (url.starts_with('/') && !url.starts_with("//"))
            || url.starts_with("http://127.0.0.1")
            || url.starts_with("http://localhost"))
}
