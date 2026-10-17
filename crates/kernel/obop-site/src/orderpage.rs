//! What the buyer sees after checkout (D61, D62): the order page, the payment status page the
//! provider sends them back to, the printable receipt and the demo provider's hosted page.
//!
//! Every page is reached by an unguessable token (the order's) and shows only that order. The
//! payment pages never believe the browser: coming back from a provider with `?s=cancel` or
//! without it changes nothing; the state shown is the one the server holds, refreshed from the
//! provider when the page is opened.

use crate::checkout::{percent, redirect_ok};
use crate::countries;
use crate::http::Resp;
use crate::layout::{self, Ctx, Page};
use crate::util::esc;
use crate::{Site, valid_token};
use obop_commands::PublicRecord;
use serde_json::Value as J;
use std::time::Duration;

/// The tax lines stored on the order as JSON.
fn tax_lines(o: &PublicRecord) -> Vec<(String, i64, bool, i64)> {
    serde_json::from_str::<J>(o.text("tax_summary"))
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|t| {
            Some((
                t.get("name")?.as_str()?.to_string(),
                t.get("rate_bp")?.as_i64()?,
                t.get("included")?.as_bool()?,
                t.get("amount")?.as_i64()?,
            ))
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum View {
    /// `/shop/order/{token}`: the confirmation, from the stored state only.
    Order,
    /// `/shop/pay/{token}`: where a provider sends the buyer back; asks the provider first.
    Pay,
    /// `/shop/order/{token}/receipt`: printable.
    Receipt,
}

impl Site {
    fn payment_label(&self, ctx: &Ctx, provider: &str) -> String {
        match provider {
            "manual" | "stripe" | "fake" => ctx.t(&format!("pay.method.{provider}")).to_string(),
            _ => String::new(),
        }
    }

    pub(crate) fn order_page(&self, ctx: &mut Ctx, token: &str) -> Resp {
        self.order_view(ctx, token, View::Order)
    }

    pub(crate) fn pay_page(&self, ctx: &mut Ctx, token: &str) -> Resp {
        self.order_view(ctx, token, View::Pay)
    }

    pub(crate) fn receipt_page(&self, ctx: &mut Ctx, token: &str) -> Resp {
        self.order_view(ctx, token, View::Receipt)
    }

    /// `GET /shop/pay/{token}/continue`: back to the provider's page while the payment is open.
    pub(crate) fn pay_continue(&self, ctx: &mut Ctx, token: &str) -> Resp {
        if !ctx.caps.shop || !valid_token(token) {
            return self.not_found(ctx);
        }
        let src = self.source(ctx.req);
        if !self.allow(&format!("order:{src}"), 60, Duration::from_secs(600)) {
            return self
                .error(ctx, 429, "e429.title", "err.rate")
                .header("retry-after", "600");
        }
        let Ok(Some(rc)) = self.api.receipt("ecommerce_order", token) else {
            return self.not_found(ctx);
        };
        let target = self
            .payments
            .as_ref()
            .and_then(|p| p.resume(&rc.record.id))
            .filter(|u| redirect_ok(u));
        let back = format!("/shop/pay/{token}");
        Resp::redirect(303, target.as_deref().unwrap_or(&ctx.lpath(&back)))
    }

    fn order_view(&self, ctx: &mut Ctx, token: &str, view: View) -> Resp {
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
        // The pay page asks the provider; nothing the browser sent decides what is shown.
        let live = (view == View::Pay)
            .then(|| {
                self.payments
                    .as_ref()
                    .and_then(|p| p.refresh(&rc.record.id))
            })
            .flatten();
        // Reading again after the refresh, because the refresh may have marked the order paid.
        let Ok(Some(rc)) = self.api.receipt("ecommerce_order", token) else {
            return self.not_found(ctx);
        };
        let o = &rc.record;
        let cur = match o.text("currency") {
            "" => ctx.s.shop_currency.clone(),
            c => c.to_string(),
        };
        let path = match view {
            View::Receipt => format!("/shop/order/{token}/receipt"),
            View::Pay => format!("/shop/pay/{token}"),
            View::Order => format!("/shop/order/{token}"),
        };
        let title = if view == View::Receipt {
            ctx.t("receipt.title")
        } else {
            ctx.t("order.title")
        };
        let mut p = Page::new(title, title, &path);
        p.noindex = true;
        p.private = true;
        p.body_attrs = if o.text("state") == "cancelled" {
            String::new()
        } else {
            " data-clear-cart".into()
        };
        p.extra_headers = vec![("referrer-policy".into(), "no-referrer".into())];
        let state = o.text("state");
        let state_label = match state {
            "pending" | "paid" | "shipped" | "delivered" | "cancelled" | "refunded" => {
                ctx.t(&format!("state.{state}"))
            }
            _ => "",
        };
        let payment = rc
            .children
            .iter()
            .find(|(m, _)| m == "ecommerce_payment")
            .and_then(|(_, r)| r.last());
        let pay_state = live
            .as_ref()
            .map(|v| v.state.as_str())
            .or_else(|| payment.map(|r| r.text("state")))
            .unwrap_or("");
        let method = match o.text("payment_method") {
            "" => payment.map_or("", |r| r.text("provider")),
            m => m,
        };

        let mut h = String::new();
        let mut md_out = format!(
            "{}\n\n{}: {}\n",
            ctx.tf("order.number", &[("number", o.text("number"))]),
            ctx.t("order.status"),
            state_label
        );
        if view == View::Receipt {
            h.push_str(&format!(
                "<h1>{}</h1><p class=\"lede\">{}</p>",
                esc(ctx.t("receipt.title")),
                esc(&ctx.s.title)
            ));
        } else {
            h.push_str(&format!("<h1>{}</h1>", esc(ctx.t("order.title"))));
            h.push_str(&self.status_banner(
                ctx,
                token,
                view,
                o,
                state,
                pay_state,
                method,
                &mut md_out,
            ));
            h.push_str(&format!(
                "<div class=\"notice\">{}</div>",
                esc(ctx.t("order.keep"))
            ));
        }
        let mut facts = format!(
            "<h2>{}</h2><dl class=\"facts\"><dt>{}</dt><dd>{}</dd><dt>{}</dt><dd>{}</dd>",
            esc(&ctx.tf("order.number", &[("number", o.text("number"))])),
            esc(ctx.t("order.status")),
            esc(state_label),
            esc(ctx.t("order.date")),
            esc(&ctx.date(o.text("placed_on"))),
        );
        facts.push_str(&format!(
            "<dt>{}</dt><dd>{}</dd><dt>{}</dt><dd>{}{}</dd>",
            esc(ctx.t("co.name")),
            esc(o.text("customer_name")),
            esc(ctx.t("order.ship_to")),
            esc(o.text("shipping_address")),
            if o.text("country").is_empty() {
                String::new()
            } else {
                format!("\n{}", esc(&countries::name(ctx.lang, o.text("country"))))
            },
        ));
        if !method.is_empty() {
            facts.push_str(&format!(
                "<dt>{}</dt><dd>{}{}</dd>",
                esc(ctx.t("order.payment")),
                esc(&self.payment_label(ctx, method)),
                if pay_state.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", esc(ctx.t(&format!("pay.state.{pay_state}"))))
                }
            ));
        }
        facts.push_str("</dl>");
        h.push_str(&facts);

        let lines: Vec<&PublicRecord> = rc
            .children
            .iter()
            .find(|(m, _)| m == "ecommerce_order_line")
            .map(|(_, r)| r.iter().collect())
            .unwrap_or_default();
        h.push_str(&format!(
            "<h2>{}</h2><ul class=\"lines\">",
            esc(ctx.t("order.items"))
        ));
        for l in lines {
            let q = l.int("quantity").unwrap_or(0);
            let sum = l.int("subtotal").unwrap_or(0);
            let mut each = format!(
                "{} × {}",
                q,
                esc(&ctx.money(l.int("unit_price").unwrap_or(0), &cur))
            );
            if l.int("discount_amount").unwrap_or(0) > 0 {
                each.push_str(&format!(
                    " · {} −{}",
                    esc(ctx.t("order.discount")),
                    esc(&ctx.money(l.int("discount_amount").unwrap_or(0), &cur))
                ));
            }
            if !l.text("tax_name").is_empty() {
                each.push_str(&format!(
                    " · {} {}",
                    esc(l.text("tax_name")),
                    esc(&percent(ctx, l.int("tax_rate_bp").unwrap_or(0)))
                ));
            }
            h.push_str(&format!(
                "<li class=\"line\"><div><div class=\"name\">{}</div><div class=\"each\">{each}</div></div><div class=\"sum\">{}</div></li>",
                esc(l.text("description")),
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
        h.push_str("<dl class=\"totals\">");
        h.push_str(&format!(
            "<dt>{}</dt><dd>{}</dd>",
            esc(ctx.t("cart.subtotal")),
            esc(&ctx.money(o.int("subtotal").unwrap_or(0), &cur))
        ));
        if o.int("discount_total").unwrap_or(0) > 0 {
            let label = if o.text("discount_code").is_empty() {
                ctx.t("order.discount").to_string()
            } else {
                ctx.tf("cart.discount_line", &[("code", o.text("discount_code"))])
            };
            h.push_str(&format!(
                "<dt>{}</dt><dd>\u{2212}{}</dd>",
                esc(&label),
                esc(&ctx.money(o.int("discount_total").unwrap_or(0), &cur))
            ));
        }
        h.push_str(&format!(
            "<dt>{}</dt><dd>{}</dd>",
            esc(ctx.t("cart.shipping")),
            esc(&ctx.money(o.int("shipping_total").unwrap_or(0), &cur))
        ));
        for (name, bp, included, amount) in tax_lines(o) {
            let key = if included {
                "cart.tax_included"
            } else {
                "cart.tax_added"
            };
            h.push_str(&format!(
                "<dt class=\"tax\">{}</dt><dd class=\"tax\">{}</dd>",
                esc(&ctx.tf(key, &[("name", &name), ("rate", &percent(ctx, bp))])),
                esc(&ctx.money(amount, &cur))
            ));
        }
        h.push_str(&format!(
            "<dt class=\"grand\">{}</dt><dd class=\"grand\">{}</dd></dl>",
            esc(ctx.t("cart.total")),
            esc(&ctx.money(o.int("total").unwrap_or(0), &cur))
        ));
        md_out.push_str(&format!(
            "\n{}: {}\n",
            ctx.t("cart.total"),
            ctx.money(o.int("total").unwrap_or(0), &cur)
        ));
        if view == View::Receipt {
            h.push_str(&format!(
                "<p><button class=\"btn\" type=\"button\" data-print>{}</button> <a class=\"btn secondary\" href=\"{}\">{}</a></p>",
                esc(ctx.t("receipt.print")),
                esc(&ctx.lpath(&format!("/shop/order/{token}"))),
                esc(ctx.t("receipt.back"))
            ));
        } else {
            if matches!(state, "paid" | "shipped" | "delivered" | "refunded") {
                h.push_str(&format!(
                    "<p><a class=\"btn secondary\" href=\"{}\">{}</a></p>",
                    esc(&ctx.lpath(&format!("/shop/order/{token}/receipt"))),
                    esc(ctx.t("receipt.link"))
                ));
            }
            h.push_str(&format!(
                "<p><a class=\"btn secondary\" href=\"{}\">{}</a></p>",
                esc(&ctx.lpath("/shop")),
                esc(ctx.t("cart.continue"))
            ));
        }
        p.body = h;
        p.md = md_out;
        let r = layout::render(ctx, p);
        r.header("referrer-policy", "no-referrer")
    }

    #[allow(clippy::too_many_arguments)]
    fn status_banner(
        &self,
        ctx: &Ctx,
        token: &str,
        view: View,
        o: &PublicRecord,
        state: &str,
        pay_state: &str,
        method: &str,
        md: &mut String,
    ) -> String {
        let note = |ok: bool, text: &str, md: &mut String| {
            md.push_str(&format!("\n{text}\n"));
            format!(
                "<div class=\"notice{}\" role=\"status\">{}</div>",
                if ok { " ok" } else { "" },
                esc(text)
            )
        };
        match state {
            "paid" | "shipped" | "delivered" => note(true, ctx.t("pay.paid"), md),
            "refunded" => note(false, ctx.t("pay.refunded"), md),
            "cancelled" => {
                let key = match pay_state {
                    "expired" => "pay.expired",
                    "failed" => "pay.failed",
                    "cancelled" => "pay.cancelled",
                    _ => "pay.order_cancelled",
                };
                let mut b = note(false, ctx.t(key), md);
                b.push_str(&format!(
                    "<p><a class=\"btn\" href=\"{}\">{}</a></p>",
                    esc(&ctx.lpath("/cart")),
                    esc(ctx.t("pay.retry"))
                ));
                b
            }
            _ => {
                if method == "manual" {
                    let text = if ctx.s.payment_instructions.is_empty() {
                        ctx.tf("order.next", &[("email", o.text("customer_email"))])
                    } else {
                        ctx.s.payment_instructions.clone()
                    };
                    md.push_str(&format!("\n{text}\n"));
                    format!(
                        "<div class=\"notice\" role=\"status\"><strong>{}</strong><p class=\"pre\">{}</p></div>",
                        esc(ctx.t("pay.manual_title")),
                        esc(&text)
                    )
                } else if method.is_empty() {
                    note(
                        false,
                        &ctx.tf("order.next", &[("email", o.text("customer_email"))]),
                        md,
                    )
                } else {
                    let cancelled =
                        view == View::Pay && ctx.req.q("s").as_deref() == Some("cancel");
                    let mut b = note(
                        false,
                        ctx.t(if cancelled {
                            "pay.not_completed"
                        } else {
                            "pay.waiting"
                        }),
                        md,
                    );
                    let resumable = matches!(pay_state, "open" | "created")
                        && self
                            .payments
                            .as_ref()
                            .is_some_and(|p| p.resume(o.id.as_str()).is_some());
                    if resumable {
                        b.push_str(&format!(
                            "<p><a class=\"btn\" href=\"{}\">{}</a></p>",
                            esc(&ctx.lpath(&format!("/shop/pay/{token}/continue"))),
                            esc(ctx.t("pay.continue"))
                        ));
                    }
                    b
                }
            }
        }
    }

    // ---- the demo provider's hosted page ---------------------------------------------------

    fn fake_session_ok(s: &str) -> bool {
        s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
    }

    /// `GET /shop/fake-pay/{session}`: what a hosted payment page would look like, for demos
    /// and tests. No money moves and no card is asked for.
    pub(crate) fn fake_pay_page(&self, ctx: &mut Ctx, session: &str) -> Resp {
        let Some(port) = self.payments.as_ref().filter(|_| ctx.caps.shop) else {
            return self.not_found(ctx);
        };
        let src = self.source(ctx.req);
        if !Self::fake_session_ok(session)
            || !self.allow(&format!("fake:{src}"), 60, Duration::from_secs(600))
        {
            return self.not_found(ctx);
        }
        let Some(v) = port.fake_session(session) else {
            return self.not_found(ctx);
        };
        self.ensure_csrf(ctx);
        let mut p = Page::new(
            ctx.t("pay.fake_title"),
            ctx.t("pay.fake_title"),
            "/shop/fake-pay",
        );
        p.noindex = true;
        p.private = true;
        let money = ctx.money(v.amount, &v.currency);
        let mut h = format!(
            "<h1>{}</h1><div class=\"notice\" role=\"status\"><strong>{}</strong></div>\
<p class=\"lede\">{} · {}</p>",
            esc(ctx.t("pay.fake_title")),
            esc(ctx.t("pay.fake_banner")),
            esc(&ctx.tf("order.number", &[("number", &v.number)])),
            esc(&money),
        );
        if v.state == "open" {
            h.push_str(&format!(
                "<form method=\"post\" action=\"/shop/fake-pay/{}\" class=\"formcard\"><input type=\"hidden\" name=\"csrf\" value=\"{}\">\
<p><button class=\"btn\" type=\"submit\" name=\"outcome\" value=\"paid\">{}</button></p>\
<p><button class=\"btn secondary\" type=\"submit\" name=\"outcome\" value=\"failed\">{}</button></p>\
<p><button class=\"btn secondary\" type=\"submit\" name=\"outcome\" value=\"cancelled\">{}</button></p></form>",
                esc(session),
                esc(&ctx.csrf),
                esc(ctx.t("pay.fake_pay")),
                esc(ctx.t("pay.fake_fail")),
                esc(ctx.t("pay.fake_cancel")),
            ));
        } else {
            h.push_str(&format!("<p>{}</p>", esc(ctx.t("pay.fake_done"))));
        }
        h.push_str(&format!(
            "<p><a href=\"{}\">{}</a></p>",
            esc(&ctx.lpath(&format!("/shop/pay/{}", v.order_token))),
            esc(ctx.t("pay.fake_back"))
        ));
        p.body = h;
        p.md = format!("{}\n", ctx.t("pay.fake_banner"));
        layout::render(ctx, p).header("referrer-policy", "same-origin")
    }

    /// `POST /shop/fake-pay/{session}`.
    pub(crate) fn fake_pay_decide(&self, ctx: &mut Ctx, session: &str) -> Resp {
        let Some(port) = self.payments.as_ref().filter(|_| ctx.caps.shop) else {
            return self.not_found(ctx);
        };
        let form = ctx.req.form();
        if let Err(r) = self.check_post(ctx, &form) {
            return r;
        }
        let src = self.source(ctx.req);
        if !Self::fake_session_ok(session)
            || !self.allow(&format!("fake:{src}"), 60, Duration::from_secs(600))
        {
            return self.not_found(ctx);
        }
        let Some(v) = port.fake_session(session) else {
            return self.not_found(ctx);
        };
        if port.fake_decide(session, form.get("outcome")).is_err() {
            return self.error(ctx, 400, "e404.title", "err.generic");
        }
        Resp::redirect(303, &ctx.lpath(&format!("/shop/pay/{}", v.order_token)))
    }
}
