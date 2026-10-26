//! Payments for the web shop (D62).
//!
//! Three providers behind one trait ([`PaymentProvider`]): `manual` (bank transfer or invoice),
//! `stripe` (Stripe Checkout) and `fake` (a local demo). [`Payments`] owns the bookkeeping:
//!
//! - **One payment record per attempt** (`ecommerce_payment`: provider, the provider's reference,
//!   amount and currency from the stored order, state, the ids of the events already applied,
//!   the refund reference). No card data exists anywhere in obop.
//! - **The browser is never believed.** The buyer comes back to `/shop/pay/{token}`; the state
//!   shown is the one the server holds, refreshed from the provider when the page is opened. The
//!   webhook (`POST /hooks/pay/{provider}`) is verified on the raw body before anything is read
//!   from it, applied once (a repeated event changes nothing) and cross-checked against the
//!   stored amount and currency.
//! - **Every state change is a compare-and-set** (`Backend::apply`), so two deliveries racing
//!   for one payment leave it in one state and the order is marked paid once.
//! - **Not paid means given back.** An order whose payment fails, expires, is cancelled or is
//!   never paid returns its units to the listings and its discount use to the code, once.
//! - **Refunds** run inside the order's `refund` action (a wrapper on the command, so they
//!   happen only when it really runs, after any human approval): the provider is asked first, and
//!   an order whose refund the provider refused stays paid.
//!
//! Receipts by email are out of scope (there is no mail sender); the confirmation page shows the
//! receipt and has a printable one.

pub mod provider;
mod simple;
pub mod stripe;

#[cfg(test)]
mod tests;

pub use provider::*;
pub use simple::{FAKE_MODEL, Fake, Manual};
pub use stripe::{Stripe, StripeKeys};

use obop_commands::{
    Dispatcher, FakeView, HookReply, PaymentHooks, PaymentPort, PaymentView, Principal,
    StartPayment, Started, Surface, Value, Vault,
};
use obop_connectors::net::Client;
use obop_connectors::runtime::Runtime;
use obop_connectors::webhook::{Seen, VerifyError};
use obop_storage::{ApplyError, Backend, Fields, Op};
use serde_json::{Value as J, json};
use std::sync::{Arc, Mutex};

const PAYMENT: &str = "ecommerce_payment";
const ORDER: &str = "ecommerce_order";
const LINE: &str = "ecommerce_order_line";
const LISTING: &str = "ecommerce_listing";
/// Minutes before the provider's page stops taking payment. Stripe's minimum is 30.
const SESSION_MINUTES: u64 = 31;
const EVENTS_KEPT: usize = 50;

fn now() -> u64 {
    obop_connectors::util::now()
}

/// Where the Stripe secrets come from.
pub trait StripeKeySource: Send + Sync {
    fn keys(&self) -> Option<StripeKeys>;
}

/// The instance's own `stripe` connection (Connectors app): connected, owned by the instance,
/// with `api_key` and `signing_secret` among its sealed credentials.
pub struct ConnectionKeys(pub Arc<Runtime>);

impl StripeKeySource for ConnectionKeys {
    fn keys(&self) -> Option<StripeKeys> {
        let rt = &self.0;
        for (id, f) in rt.backend.list(obop_connectors::model::CONNECTION).ok()? {
            let Ok(c) = obop_connectors::model::Connection::from_fields(&id, &f) else {
                continue;
            };
            if c.provider != "stripe" || !c.is_connected() || c.owner_scope != "instance" {
                continue;
            }
            let creds = rt.credentials(&c).ok()?;
            let get = |k: &str| creds.get(k).and_then(J::as_str).map(str::to_string);
            let api_key = get("api_key").filter(|k| !k.trim().is_empty())?;
            return Some(StripeKeys {
                api_key,
                signing_secret: get("signing_secret").unwrap_or_default(),
            });
        }
        None
    }
}

/// Fixed keys, for tests.
pub struct StaticKeys(pub Option<StripeKeys>);

impl StripeKeySource for StaticKeys {
    fn keys(&self) -> Option<StripeKeys> {
        self.0.clone()
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    /// Offer the local demo provider (`OBOP_DEMO_PAYMENTS=1`).
    pub demo: bool,
    pub stripe_base: String,
    pub stripe_hosts: Vec<String>,
}

impl Config {
    pub fn from_env() -> Config {
        Config {
            demo: std::env::var("OBOP_DEMO_PAYMENTS").is_ok_and(|v| v == "1"),
            ..Config::default()
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            demo: false,
            stripe_base: "https://api.stripe.com".into(),
            stripe_hosts: vec!["api.stripe.com".into()],
        }
    }
}

pub struct Payments {
    backend: Arc<dyn Backend>,
    d: Arc<Dispatcher>,
    client: Client,
    keys: Arc<dyn StripeKeySource>,
    cfg: Config,
    fake: Fake,
    last_sweep: Mutex<u64>,
}

/// What applying one event did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    Done,
    Duplicate,
    /// The event is about something that is not (or no longer) open, or did not match the
    /// stored amount.
    Ignored,
}

fn events_of(f: &Fields) -> Vec<String> {
    f.get("event_ids")
        .and_then(J::as_str)
        .and_then(|t| serde_json::from_str::<Vec<String>>(t).ok())
        .unwrap_or_default()
}

fn text<'a>(f: &'a Fields, k: &str) -> &'a str {
    f.get(k).and_then(J::as_str).unwrap_or("")
}

impl Payments {
    pub fn new(
        backend: Arc<dyn Backend>,
        d: Arc<Dispatcher>,
        client: Client,
        keys: Arc<dyn StripeKeySource>,
        cfg: Config,
    ) -> Arc<Payments> {
        Arc::new(Payments {
            fake: Fake::new(backend.clone()),
            backend,
            d,
            client,
            keys,
            cfg,
            last_sweep: Mutex::new(0),
        })
    }

    /// The identity that runs the order actions a payment causes (marking an order paid,
    /// cancelling one nobody paid). It is a user, not an agent, so no approval stands in the way.
    fn service() -> Principal {
        Principal::user("service:payments", &["ecommerce.user", "ecommerce.manager"])
    }

    fn run(&self, command: &str, id: &str) -> Result<(), String> {
        self.d
            .call(
                Surface::Rest,
                &Self::service(),
                &mut Vault::new(),
                command,
                &Value::map([("id", Value::text(id))]),
            )
            .map(|_| ())
            .map_err(|e| format!("{e:?}"))
    }

    pub fn provider(&self, id: &str) -> Option<Arc<dyn PaymentProvider>> {
        match id {
            "manual" => Some(Arc::new(Manual)),
            "fake" if self.cfg.demo => Some(Arc::new(Fake::new(self.backend.clone()))),
            "stripe" => {
                let keys = self.keys.keys()?;
                Some(Arc::new(Stripe::new(
                    self.client.clone(),
                    &self.cfg.stripe_base,
                    self.cfg.stripe_hosts.clone(),
                    keys,
                )))
            }
            _ => None,
        }
    }

    // ---- records ---------------------------------------------------------------------------

    fn payment_for_order(&self, order: &str) -> Option<(String, Fields)> {
        self.backend
            .list(PAYMENT)
            .ok()?
            .into_iter()
            .rfind(|(_, f)| text(f, "order_id") == order)
    }

    /// The payment a provider's event is about. An error is a refused lookup, not "none": the
    /// caller must not take it for an event about something else (D74).
    fn payment_by_external(
        &self,
        provider: &str,
        external: &str,
    ) -> Result<Option<(String, Fields)>, String> {
        if external.is_empty() {
            return Ok(None);
        }
        Ok(self
            .backend
            .list(PAYMENT)?
            .into_iter()
            .find(|(_, f)| text(f, "provider") == provider && text(f, "external_id") == external))
    }

    fn order(&self, id: &str) -> Option<Fields> {
        self.backend.get(ORDER, id).ok().flatten()
    }

    // ---- start -----------------------------------------------------------------------------

    fn start_payment(&self, req: &StartPayment) -> Result<Started, String> {
        let unavailable = || "This payment method is not available.".to_string();
        let provider = self.provider(&req.method).ok_or_else(unavailable)?;
        let order = self.order(&req.order_id).ok_or_else(unavailable)?;
        // The amount is the stored total; what the caller says must agree with it.
        let total = order.get("total").and_then(J::as_i64).unwrap_or(-1);
        if total < 0 || total != req.amount {
            return Err(unavailable());
        }
        let currency = text(&order, "currency").to_string();
        let payment_id = uuid::Uuid::now_v7().to_string();
        let mut rec = Fields::new();
        rec.insert("order_id".into(), json!(req.order_id));
        rec.insert("provider".into(), json!(req.method));
        rec.insert("state".into(), json!("open"));
        rec.insert("amount".into(), json!(total));
        rec.insert("currency".into(), json!(currency));
        rec.insert("event_ids".into(), json!("[]"));
        rec.insert("external_id".into(), json!(""));
        self.backend
            .insert(PAYMENT, &payment_id, &rec)
            .map_err(|_| unavailable())?;
        let session = provider.create_session(&SessionRequest {
            payment_id: payment_id.clone(),
            number: req.number.clone(),
            amount: total,
            currency,
            success_url: req.return_url.clone(),
            cancel_url: req.cancel_url.clone(),
            title: format!("Order {}", req.number),
            expires_at: now() + SESSION_MINUTES * 60,
        });
        let session = match session {
            Ok(s) => s,
            Err(_) => {
                // Nothing was charged and the page does not exist: close the attempt and give
                // everything back, so the buyer can try again from a clean cart.
                let mut f = Fields::new();
                f.insert("state".into(), json!("failed"));
                let _ = self.backend.update(PAYMENT, &payment_id, &f);
                let _ = self.cancel_order(&req.order_id);
                return Err(
                    "The payment could not be started. Nothing was charged; please try again."
                        .to_string(),
                );
            }
        };
        let mut f = Fields::new();
        f.insert("external_id".into(), json!(session.external_id));
        if let Some(u) = &session.url {
            f.insert("checkout_url".into(), json!(u));
        }
        self.backend
            .update(PAYMENT, &payment_id, &f)
            .map_err(|_| unavailable())?;
        Ok(match session.url {
            Some(url) => Started::Redirect(url),
            None => Started::Manual,
        })
    }

    // ---- applying what a provider says -----------------------------------------------------

    /// Apply one outcome to the payment `id`, once. `event_id` is the provider's id of the
    /// event (or a synthetic one for a poll); a repeat of an id already applied changes nothing.
    pub fn apply(
        &self,
        id: &str,
        event_id: &str,
        outcome: Outcome,
        amount: Option<i64>,
        currency: Option<&str>,
    ) -> Result<Applied, String> {
        let Some(p) = self.backend.get(PAYMENT, id)? else {
            return Ok(Applied::Ignored);
        };
        let mut seen = events_of(&p);
        if seen.iter().any(|e| e == event_id) {
            return Ok(Applied::Duplicate);
        }
        // A payment that was given up (cancelled, expired, failed) can still receive money: the
        // buyer paid a page that was closed too late. That is a payment to return, not to lose.
        let state = text(&p, "state").to_string();
        let late = outcome == Outcome::Paid
            && matches!(state.as_str(), "cancelled" | "expired" | "failed");
        if state != "open" && !late {
            return Ok(Applied::Ignored);
        }
        // A payment for another amount or in another currency is not this payment.
        if outcome == Outcome::Paid {
            let stored = p.get("amount").and_then(J::as_i64);
            let cur = text(&p, "currency");
            if amount.is_some_and(|a| Some(a) != stored)
                || currency.is_some_and(|c| !cur.is_empty() && !c.eq_ignore_ascii_case(cur))
            {
                return Ok(Applied::Ignored);
            }
        }
        seen.push(event_id.to_string());
        if seen.len() > EVENTS_KEPT {
            seen.remove(0);
        }
        let new_state = match outcome {
            Outcome::Paid => "succeeded",
            Outcome::Failed => "failed",
            Outcome::Expired => "expired",
            Outcome::Cancelled => "cancelled",
        };
        let mut patch = Fields::new();
        patch.insert("state".into(), json!(new_state));
        patch.insert("event_ids".into(), json!(json!(seen).to_string()));
        match self.backend.apply_checked(&[Op::Patch {
            model: PAYMENT.into(),
            id: id.into(),
            patch,
            expect: vec![("state".into(), json!(state))],
        }]) {
            Ok(()) => {}
            // Someone else applied an event between our read and our write.
            Err(ApplyError::Conflict { .. }) => return Ok(Applied::Duplicate),
            Err(e) => return Err(e.to_string()),
        }
        let order = text(&p, "order_id").to_string();
        match outcome {
            Outcome::Paid => self.after_paid(&order, id),
            _ => {
                let _ = self.cancel_order(&order);
            }
        }
        Ok(Applied::Done)
    }

    /// The payment succeeded: the order is paid. If the order was cancelled in the meantime
    /// (the buyer paid at the last moment) the money is returned rather than kept for nothing.
    fn after_paid(&self, order: &str, payment: &str) {
        let state = self.order(order).map(|o| text(&o, "state").to_string());
        match state.as_deref() {
            Some("pending") => {
                let _ = self.run("ecommerce_order.mark_paid", order);
            }
            Some("paid" | "shipped" | "delivered") => {}
            _ => {
                let _ = self.refund_payment(payment);
            }
        }
    }

    /// Cancel an order nobody will pay (through the order's own action, so everything that
    /// follows a cancellation happens), then make sure the units and the code use are back.
    fn cancel_order(&self, order: &str) -> Result<(), String> {
        let state = self.order(order).map(|o| text(&o, "state").to_string());
        if state.as_deref() == Some("pending") {
            let _ = self.run("ecommerce_order.cancel", order);
        }
        self.release_order(order, true).map(|_| ())
    }

    // ---- stock and discounts back -------------------------------------------------------------

    /// Put the units of an order back on its listings and, if `discount` is set, give back the
    /// use of its discount code, **once**: the order records that it was done, in the same atomic
    /// batch. `true` if this call did it.
    pub fn release_order(&self, order_id: &str, discount: bool) -> Result<bool, String> {
        let Some(o) = self.order(order_id) else {
            return Ok(false);
        };
        let returned = o.get("stock_returned").cloned().unwrap_or(J::Null);
        if returned == json!(true) {
            return Ok(false);
        }
        let mut ops = vec![Op::Patch {
            model: ORDER.into(),
            id: order_id.into(),
            patch: Fields::from_iter([("stock_returned".to_string(), json!(true))]),
            expect: vec![("stock_returned".into(), returned)],
        }];
        for (_, l) in self.backend.list(LINE)? {
            if text(&l, "order_id") != order_id {
                continue;
            }
            let (listing, qty) = (
                text(&l, "listing_id").to_string(),
                l.get("quantity").and_then(J::as_i64).unwrap_or(0),
            );
            if listing.is_empty() || qty <= 0 {
                continue;
            }
            let Some(rec) = self.backend.get(LISTING, &listing)? else {
                continue;
            };
            if matches!(
                text(&rec, "stock_policy"),
                "block_when_out" | "allow_backorder"
            ) {
                ops.push(Op::add(LISTING, &listing, "stock_quantity", qty));
            }
        }
        if discount {
            let code = text(&o, "discount_code_id");
            if !code.is_empty() {
                ops.push(Op::add("ecommerce_discount_code", code, "used_count", -1).min(0));
            }
            let usage = text(&o, "discount_use_id");
            if !usage.is_empty() {
                ops.push(Op::add("ecommerce_discount_use", usage, "count", -1).min(0));
            }
        }
        match self.backend.apply_checked(&ops) {
            Ok(()) => Ok(true),
            // Someone else gave them back first, or the counter is already at zero.
            Err(ApplyError::Conflict { index: 0, .. }) => Ok(false),
            Err(ApplyError::Conflict { .. }) => {
                // A counter that cannot go lower must not stop the units from coming back.
                let mut ops = ops;
                ops.retain(|op| !matches!(op, Op::Add { model, .. } if model.starts_with("ecommerce_discount")));
                self.backend
                    .apply_checked(&ops)
                    .map(|()| true)
                    .or(Ok(false))
            }
            Err(e) => Err(e.to_string()),
        }
    }

    // ---- refunds ---------------------------------------------------------------------------

    /// Return the money of a payment through its provider and record it. Safe to repeat: the
    /// provider is given a fixed key, and a refunded payment is left alone.
    pub fn refund_payment(&self, payment_id: &str) -> Result<(), String> {
        let Some(p) = self.backend.get(PAYMENT, payment_id)? else {
            return Err("no such payment".into());
        };
        match text(&p, "state") {
            "refunded" => return Ok(()),
            "succeeded" => {}
            other => return Err(format!("a payment that is {other} cannot be refunded")),
        }
        let provider = self
            .provider(text(&p, "provider"))
            .ok_or("the payment provider is not available")?;
        let amount = p.get("amount").and_then(J::as_i64).unwrap_or(0);
        let done = provider
            .refund(
                text(&p, "external_id"),
                amount,
                &format!("refund-{payment_id}"),
            )
            .map_err(|e| e.to_string())?;
        let mut patch = Fields::new();
        patch.insert("state".into(), json!("refunded"));
        patch.insert("refund_id".into(), json!(done.id));
        patch.insert("refunded_amount".into(), json!(done.amount));
        match self.backend.apply_checked(&[Op::Patch {
            model: PAYMENT.into(),
            id: payment_id.into(),
            patch,
            expect: vec![("state".into(), json!("succeeded"))],
        }]) {
            Ok(()) | Err(ApplyError::Conflict { .. }) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }

    // ---- polling and sweeping --------------------------------------------------------------

    fn poll(&self, id: &str, p: &Fields) {
        if text(p, "state") != "open" {
            return;
        }
        let external = text(p, "external_id");
        let Some(provider) = self.provider(text(p, "provider")) else {
            return;
        };
        if external.is_empty() {
            return;
        }
        if let Ok(Status {
            outcome: Some(o),
            amount,
            currency,
        }) = provider.fetch_status(external)
        {
            let _ = self.apply(
                id,
                &format!("poll:{external}:{o:?}"),
                o,
                amount,
                currency.as_deref(),
            );
        }
    }

    /// Cancel unpaid orders whose reservation ran out. Each one is first checked with the
    /// provider, because the buyer may have paid and the notification not arrived.
    pub fn sweep_expired(&self) -> usize {
        let t = now();
        let Ok(orders) = self.backend.list(ORDER) else {
            return 0;
        };
        let mut n = 0;
        for (id, o) in orders {
            let until = o
                .get("reserved_until")
                .and_then(J::as_u64)
                .unwrap_or(u64::MAX);
            if text(&o, "state") != "pending" || until >= t {
                continue;
            }
            if let Some((pid, p)) = self.payment_for_order(&id) {
                self.poll(&pid, &p);
                let Some(p) = self.backend.get(PAYMENT, &pid).ok().flatten() else {
                    continue;
                };
                // Paid, but the order was not marked (the earlier attempt failed): finish that
                // instead of cancelling an order whose money we hold.
                if text(&p, "state") == "succeeded" {
                    self.after_paid(&id, &pid);
                    continue;
                }
                if text(&p, "state") == "open"
                    && let Some(provider) = self.provider(text(&p, "provider"))
                {
                    let _ = provider.expire(text(&p, "external_id"));
                    let _ = self.apply(&pid, &format!("expire:{id}"), Outcome::Expired, None, None);
                }
            }
            // Still unpaid (or no payment record at all): cancel and give everything back.
            if self
                .order(&id)
                .is_some_and(|o| text(&o, "state") == "pending")
            {
                let _ = self.cancel_order(&id);
            }
            if self
                .order(&id)
                .is_some_and(|o| text(&o, "state") == "cancelled")
            {
                n += 1;
            }
        }
        n
    }

    // ---- the order actions ------------------------------------------------------------------

    /// Wrap the order actions that touch money or stock. Call once after the dispatcher exists;
    /// the wrappers stay in place when the ecommerce package is installed, removed or updated.
    pub fn install(self: &Arc<Self>) {
        let order_id = |i: &Value| match i {
            Value::Map(m) => match m.get("id") {
                Some(Value::Text(t)) => Some(t.clone()),
                _ => None,
            },
            _ => None,
        };
        // Cancel: the state flips, then the open payment is closed and the stock goes back.
        let me = self.clone();
        self.d.around(
            "ecommerce_order.cancel",
            Box::new(move |_, input, inner| {
                let out = inner(input)?;
                if let Some(id) = order_id(input) {
                    me.close_payment_of(&id);
                    let _ = me.release_order(&id, true);
                }
                Ok(out)
            }),
        );
        // Marked paid by hand (a bank transfer arrived): the payment record follows.
        let me = self.clone();
        self.d.around(
            "ecommerce_order.mark_paid",
            Box::new(move |_, input, inner| {
                let out = inner(input)?;
                if let Some(id) = order_id(input) {
                    me.settle_by_hand(&id);
                }
                Ok(out)
            }),
        );
        // Refund: the provider is asked first. If it refuses, the order stays paid.
        for (cmd, restock) in [
            ("ecommerce_order.refund", true),
            ("ecommerce_order.refund_delivered", false),
        ] {
            let me = self.clone();
            self.d.around(
                cmd,
                Box::new(move |_, input, inner| {
                    let id = order_id(input);
                    if let Some(id) = &id
                        && let Some((pid, p)) = me.payment_for_order(id)
                        && text(&p, "state") == "succeeded"
                    {
                        me.refund_payment(&pid)
                            .map_err(|e| format!("the refund was not made: {e}"))?;
                    }
                    let out = inner(input)?;
                    if restock && let Some(id) = &id {
                        let _ = me.release_order(id, false);
                    }
                    Ok(out)
                }),
            );
        }
    }

    fn close_payment_of(&self, order: &str) {
        let Some((pid, p)) = self.payment_for_order(order) else {
            return;
        };
        if text(&p, "state") != "open" {
            return;
        }
        if let Some(provider) = self.provider(text(&p, "provider")) {
            let _ = provider.expire(text(&p, "external_id"));
        }
        let mut patch = Fields::new();
        patch.insert("state".into(), json!("cancelled"));
        let _ = self.backend.apply_checked(&[Op::Patch {
            model: PAYMENT.into(),
            id: pid,
            patch,
            expect: vec![("state".into(), json!("open"))],
        }]);
    }

    fn settle_by_hand(&self, order: &str) {
        let Some((pid, p)) = self.payment_for_order(order) else {
            return;
        };
        if text(&p, "state") != "open" {
            return;
        }
        let mut seen = events_of(&p);
        seen.push("by-hand".into());
        let mut patch = Fields::new();
        patch.insert("state".into(), json!("succeeded"));
        patch.insert("event_ids".into(), json!(json!(seen).to_string()));
        let _ = self.backend.apply_checked(&[Op::Patch {
            model: PAYMENT.into(),
            id: pid,
            patch,
            expect: vec![("state".into(), json!("open"))],
        }]);
    }

    // ---- webhooks --------------------------------------------------------------------------

    /// Verify and apply one delivery of a provider's notifications.
    pub fn handle_webhook(
        &self,
        provider: &str,
        header: &dyn Fn(&str) -> Option<String>,
        body: &[u8],
    ) -> HookReply {
        let Some(p) = self.provider(provider).filter(|_| provider == "stripe") else {
            return HookReply::NotFound;
        };
        let events = match p.verify_webhook(header, body, now()) {
            Ok(e) => e,
            Err(WebhookError::Unauthorized) => return HookReply::Unauthorized,
            Err(WebhookError::Malformed) => return HookReply::Ignored,
            Err(WebhookError::Unsupported) => return HookReply::NotFound,
        };
        let mut reply = HookReply::Ignored;
        for ev in events {
            // The delivery log remembers an event for the signature window, across restarts.
            let connection = format!("pay:{provider}");
            match Seen::claim(&*self.backend, &connection, &ev.id, now()) {
                Ok(()) => {}
                // The store was busy: nothing was recorded, the provider retries.
                Err(VerifyError::Storage) => return HookReply::Retry,
                Err(_) => {
                    reply = reply_max(reply, HookReply::Duplicate);
                    continue;
                }
            }
            // The id is spent from here on. Anything that stops the event from being applied
            // gives it back, or the provider's retry would be answered as a duplicate and the
            // payment would stay open for ever (D74).
            let pid = match self.payment_by_external(provider, &ev.external_id) {
                Ok(Some((pid, _))) => pid,
                Ok(None) => continue,
                Err(_) => {
                    Seen::release(&*self.backend, &connection, &ev.id);
                    return HookReply::Retry;
                }
            };
            match self.apply(&pid, &ev.id, ev.outcome, ev.amount, ev.currency.as_deref()) {
                Ok(Applied::Done) => reply = reply_max(reply, HookReply::Done),
                Ok(Applied::Duplicate) => reply = reply_max(reply, HookReply::Duplicate),
                Ok(Applied::Ignored) => {}
                Err(_) => {
                    Seen::release(&*self.backend, &connection, &ev.id);
                    return HookReply::Retry;
                }
            }
        }
        reply
    }
}

fn reply_max(a: HookReply, b: HookReply) -> HookReply {
    let rank = |r: HookReply| match r {
        HookReply::Done => 3,
        HookReply::Duplicate => 2,
        _ => 1,
    };
    if rank(b) > rank(a) { b } else { a }
}

impl PaymentHooks for Payments {
    fn webhook(
        &self,
        provider: &str,
        header: &dyn Fn(&str) -> Option<String>,
        body: &[u8],
    ) -> HookReply {
        self.handle_webhook(provider, header, body)
    }
}

impl PaymentPort for Payments {
    fn methods(&self, configured: &[String]) -> Vec<String> {
        let ok = |m: &&String| match m.as_str() {
            "manual" => true,
            "stripe" => self.keys.keys().is_some(),
            "fake" => self.cfg.demo,
            _ => false,
        };
        let v: Vec<String> = configured.iter().filter(ok).cloned().collect();
        if v.is_empty() {
            vec!["manual".to_string()]
        } else {
            v
        }
    }

    fn start(&self, req: &StartPayment) -> Result<Started, String> {
        self.start_payment(req)
    }

    fn resume(&self, order_id: &str) -> Option<String> {
        let (_, p) = self.payment_for_order(order_id)?;
        (text(&p, "state") == "open")
            .then(|| text(&p, "checkout_url").to_string())
            .filter(|u| !u.is_empty())
    }

    fn refresh(&self, order_id: &str) -> Option<PaymentView> {
        let (pid, p) = self.payment_for_order(order_id)?;
        self.poll(&pid, &p);
        let p = self.backend.get(PAYMENT, &pid).ok().flatten()?;
        Some(PaymentView {
            provider: text(&p, "provider").to_string(),
            state: text(&p, "state").to_string(),
            amount: p.get("amount").and_then(J::as_i64).unwrap_or(0),
            currency: text(&p, "currency").to_string(),
        })
    }

    fn fake_session(&self, session: &str) -> Option<FakeView> {
        if !self.cfg.demo {
            return None;
        }
        let pid = self.fake.payment_of(session)?;
        let p = self.backend.get(PAYMENT, &pid).ok().flatten()?;
        let o = self.order(text(&p, "order_id"))?;
        Some(FakeView {
            order_token: text(&o, "access_token").to_string(),
            number: text(&o, "number").to_string(),
            amount: p.get("amount").and_then(J::as_i64).unwrap_or(0),
            currency: text(&p, "currency").to_string(),
            state: text(&p, "state").to_string(),
        })
    }

    fn fake_decide(&self, session: &str, outcome: &str) -> Result<(), String> {
        if !self.cfg.demo {
            return Err("the demo provider is off".into());
        }
        let outcome = match outcome {
            "paid" => Outcome::Paid,
            "failed" => Outcome::Failed,
            "cancelled" => Outcome::Cancelled,
            _ => return Err("unknown outcome".into()),
        };
        let pid = self.fake.payment_of(session).ok_or("no such session")?;
        let p = self
            .backend
            .get(PAYMENT, &pid)?
            .ok_or_else(|| "no such payment".to_string())?;
        if text(&p, "state") != "open" || self.fake.state(session).as_deref() != Some("open") {
            return Ok(());
        }
        self.fake.settle(session, outcome);
        // The same path a webhook takes.
        self.apply(
            &pid,
            &format!("fake:{session}:{outcome:?}"),
            outcome,
            None,
            None,
        )
        .map(|_| ())
    }

    fn sweep(&self) {
        let t = now();
        {
            let mut g = self.last_sweep.lock().unwrap();
            if t.saturating_sub(*g) < 20 {
                return;
            }
            *g = t;
        }
        self.sweep_expired();
    }
}
