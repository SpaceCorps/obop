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
use obop_storage::fault::{self, Fault};
use obop_storage::{ApplyError, Backend, Fields, Op};
use serde_json::{Value as J, json};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// A step that must not be lost once the payment or the order has changed (D74): marking the
/// order paid, giving the units back, closing the payment. It is tried again while the store
/// refuses (its writes rest on what the step reads itself, so they are allowed after an earlier
/// refusal of the request), and a lasting refusal makes the request a partial one: a 503 that says
/// so, never an answer that looks done. Only a refusal of the store is retried; any other error is
/// the step's own answer.
fn needed<T>(f: impl FnMut() -> Result<T, String>) -> Result<T, String> {
    fault::needed(f)
}

/// A refusal of the store, as opposed to an answer: such an error is never taken for "no" (no
/// refund, no cancellation, no payment on record).
fn busy(e: &str) -> bool {
    Fault::is_transient(e)
}

const PAYMENT: &str = "ecommerce_payment";
const ORDER: &str = "ecommerce_order";
const LINE: &str = "ecommerce_order_line";
const LISTING: &str = "ecommerce_listing";
/// Minutes before the provider's page stops taking payment. Stripe's minimum is 30.
const SESSION_MINUTES: u64 = 31;
const EVENTS_KEPT: usize = 50;
/// The start of the error of a refund whose money went back and whose record was refused.
const MONEY_RETURNED: &str = "the money was returned";

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
    /// Stranded payments whose refund failed, by payment id and the time of the failure: the sweep
    /// leaves them alone for an hour, so a payment that can never be refunded here (refunded in
    /// the provider's dashboard, the provider connection removed) does not take the places of the
    /// ones that can.
    refund_failed: Mutex<std::collections::HashMap<String, u64>>,
    /// Calls made to a provider to return money, so that the sweep counts what cost a round trip.
    refund_calls: std::sync::atomic::AtomicU64,
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
            refund_failed: Mutex::default(),
            refund_calls: std::sync::atomic::AtomicU64::new(0),
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
            .map_err(|e| match e {
                // The handler's own text, so a refusal of the store keeps its code word.
                obop_commands::CallError::Handler(m) => m,
                other => format!("{other:?}"),
            })
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

    /// The latest payment of an order. An error is a refused lookup, not "no payment": a caller
    /// that decides about money (a refund, a cancellation) must not take it for "none" (D74).
    fn payment_of_order(&self, order: &str) -> Result<Option<(String, Fields)>, String> {
        Ok(self
            .backend
            .list(PAYMENT)?
            .into_iter()
            .rfind(|(_, f)| text(f, "order_id") == order))
    }

    /// As [`Payments::payment_of_order`], for the views that show a payment when there is one: a
    /// refused lookup shows none, and the request's refusal is answered by the 503 layer.
    fn payment_for_order(&self, order: &str) -> Option<(String, Fields)> {
        self.payment_of_order(order).ok().flatten()
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

    /// The order, or `None` when there is none. A refused read is an error: it is never "the
    /// order is gone" (that would refund a paid order, D74).
    fn order(&self, id: &str) -> Result<Option<Fields>, String> {
        self.backend.get(ORDER, id)
    }

    // ---- start -----------------------------------------------------------------------------

    fn start_payment(&self, req: &StartPayment) -> Result<Started, String> {
        let unavailable = || "This payment method is not available.".to_string();
        let provider = self.provider(&req.method).ok_or_else(unavailable)?;
        // A refusal of the store keeps its code word (the shop answers 503 and the order that was
        // just placed waits for its reservation to run out); it is not "not available".
        let order = self
            .order(&req.order_id)
            .map_err(|e| if busy(&e) { e } else { unavailable() })?
            .ok_or_else(unavailable)?;
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
            .map_err(|e| if busy(&e) { e } else { unavailable() })?;
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
                // Best effort: an attempt left open, or an order left pending, is closed by the
                // sweep when its reservation runs out.
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
        // The provider's page exists now: without its reference on the record the webhook that
        // reports the payment finds nothing to apply it to. A needed step, unless there is no
        // reference to write (a bank transfer has no page): then the order must not fail for it.
        if !session.external_id.is_empty() || session.url.is_some() {
            needed(|| self.backend.update(PAYMENT, &payment_id, &f))
                .map_err(|e| if busy(&e) { e } else { unavailable() })?;
        }
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
            // The event was applied, and its follow-up (the order marked paid, the units given
            // back) may have been refused after that: the provider's redelivery finishes it. Both
            // follow-ups are safe to run again.
            let order = text(&p, "order_id").to_string();
            match (outcome, text(&p, "state")) {
                (Outcome::Paid, "succeeded") => needed(|| self.after_paid(&order, id))?,
                (Outcome::Failed, "failed")
                | (Outcome::Expired, "expired")
                | (Outcome::Cancelled, "cancelled") => needed(|| self.cancel_order(&order))?,
                _ => {}
            }
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
        // The payment has changed. What follows is needed: when the store still refuses it after
        // the tries, the error goes up, the webhook gives the event id back and answers "retry", and
        // the redelivery finds the event already applied and finishes the follow-up (above).
        match outcome {
            Outcome::Paid => needed(|| self.after_paid(&order, id))?,
            _ => needed(|| self.cancel_order(&order))?,
        }
        Ok(Applied::Done)
    }

    /// The payment succeeded: the order is paid. If the order was cancelled in the meantime
    /// (the buyer paid at the last moment) the money is returned rather than kept for nothing.
    ///
    /// **A refused read of the order is never "the order is gone".** Only a read that says the
    /// order is cancelled (or that there is none) gets the money refunded; a refusal is an error
    /// and the caller tries again (D74). Safe to run again.
    fn after_paid(&self, order: &str, payment: &str) -> Result<(), String> {
        let state = self.order(order)?.map(|o| text(&o, "state").to_string());
        match state.as_deref() {
            Some("pending") => match self.run("ecommerce_order.mark_paid", order) {
                Ok(()) => Ok(()),
                // Marking it paid was refused. Whatever the reason, what the order is now decides:
                // paid in the meantime is done, cancelled in the meantime (or gone) gets the money
                // returned, and an order that is still pending is an error the caller sees (the
                // webhook answers "retry", the sweep leaves it): never an answer that looks done
                // while the buyer's money is held for an order nobody will ship.
                Err(e) => match self.order(order)?.map(|o| text(&o, "state").to_string()) {
                    Some(s) if matches!(s.as_str(), "paid" | "shipped" | "delivered") => Ok(()),
                    Some(s) if s == "pending" => Err(e),
                    _ => self.refund_payment(payment),
                },
            },
            Some("paid" | "shipped" | "delivered") => Ok(()),
            // A refund the provider could not make is tried again by the provider's redelivery.
            _ => self.refund_payment(payment),
        }
    }

    /// Cancel an order nobody will pay (through the order's own action, so everything that
    /// follows a cancellation happens), then make sure the units and the code use are back. A
    /// refusal of the store is an error; safe to run again.
    fn cancel_order(&self, order: &str) -> Result<(), String> {
        let state = self.order(order)?.map(|o| text(&o, "state").to_string());
        let cancel = if state.as_deref() == Some("pending") {
            self.run("ecommerce_order.cancel", order)
        } else {
            Ok(())
        };
        // What the order is now decides. Units go back only for an order that is cancelled: an
        // order that was paid in the meantime keeps them, one that is still pending (the cancel
        // was refused) keeps them reserved and the refusal is returned, so nobody sells the units
        // twice; one that is gone has nothing to give back.
        let now_state = self.order(order)?.map(|o| text(&o, "state").to_string());
        match now_state.as_deref() {
            Some("cancelled") => self.release_order(order, true).map(|_| ()),
            Some("pending") => cancel.and(Err("the order could not be cancelled".to_string())),
            _ => cancel.or(Ok(())),
        }
    }

    // ---- stock and discounts back -------------------------------------------------------------

    /// Put the units of an order back on its listings and, if `discount` is set, give back the
    /// use of its discount code, **once**: the order records that it was done, in the same atomic
    /// batch. `true` if this call did it.
    pub fn release_order(&self, order_id: &str, discount: bool) -> Result<bool, String> {
        let Some(o) = self.order(order_id)? else {
            return Ok(false);
        };
        // The checkout writes `stock_returned: false` when it reserves units. An order without
        // the flag never reserved any (a staff-made order, an import, one from before the flag):
        // there is nothing to give back, and adding its quantities to the shelf would invent
        // stock. `true` means they are back already.
        let returned = o.get("stock_returned").cloned().unwrap_or(J::Null);
        if returned != json!(false) {
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
                match self.backend.apply_checked(&ops) {
                    Ok(()) => Ok(true),
                    // Someone else gave them back first.
                    Err(ApplyError::Conflict { .. }) => Ok(false),
                    // A refusal of the store is not "already done": the units are still out.
                    Err(e) => Err(e.to_string()),
                }
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
        self.refund_calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let done = provider
            .refund(
                text(&p, "external_id"),
                amount,
                &format!("refund-{payment_id}"),
            )
            .map_err(|e| e.to_string())?;
        // From here the money is back with the buyer: that happened outside the store and cannot
        // be taken back, so a refusal of the next step makes this request a partial one.
        fault::external_change();
        let mut patch = Fields::new();
        patch.insert("state".into(), json!("refunded"));
        patch.insert("refund_id".into(), json!(done.id));
        patch.insert("refunded_amount".into(), json!(done.amount));
        needed(|| {
            match self.backend.apply_checked(&[Op::Patch {
                model: PAYMENT.into(),
                id: payment_id.into(),
                patch: patch.clone(),
                expect: vec![("state".into(), json!("succeeded"))],
            }]) {
                Ok(()) | Err(ApplyError::Conflict { .. }) => Ok(()),
                Err(e) => Err(e.to_string()),
            }
        })
        .map_err(|e| {
            format!(
                "{MONEY_RETURNED} (refund {}) but the payment could not be recorded as refunded: {e}; repeat the refund, the provider returns the same refund within a day",
                done.id
            )
        })
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
        let mut revisited = 0;
        let cancelled: HashSet<&str> = orders
            .iter()
            .filter(|(_, o)| text(o, "state") == "cancelled")
            .map(|(id, _)| id.as_str())
            .collect();
        for (id, o) in &orders {
            // A cancelled or refunded order whose units were not given back (the step after it
            // was refused) is finished here, a few per sweep: nobody else would. Only an order
            // that recorded a reservation (`stock_returned` is `false`) has units owed.
            let state = text(o, "state");
            if matches!(state, "cancelled" | "refunded")
                && o.get("stock_returned").and_then(J::as_bool) == Some(false)
                && revisited < 25
            {
                revisited += 1;
                let _ = self.release_order(id, state == "cancelled");
                continue;
            }
            let until = o
                .get("reserved_until")
                .and_then(J::as_u64)
                .unwrap_or(u64::MAX);
            if text(o, "state") != "pending" || until >= t {
                continue;
            }
            // Every lookup below that is refused leaves the order for the next sweep: a refused
            // read is never "no payment record" (that would cancel an order whose money we hold).
            match self.payment_of_order(id) {
                Err(_) => continue,
                Ok(Some((pid, p))) => {
                    self.poll(&pid, &p);
                    let Ok(Some(p)) = self.backend.get(PAYMENT, &pid) else {
                        continue;
                    };
                    // Paid, but the order was not marked (the earlier attempt failed): finish that
                    // instead of cancelling an order whose money we hold.
                    if text(&p, "state") == "succeeded" {
                        let _ = self.after_paid(id, &pid);
                        continue;
                    }
                    if text(&p, "state") == "open"
                        && let Some(provider) = self.provider(text(&p, "provider"))
                    {
                        let _ = provider.expire(text(&p, "external_id"));
                        // The attempt is not closed on our side: leave the order, or the buyer
                        // could still pay for one that was cancelled.
                        if self
                            .apply(&pid, &format!("expire:{id}"), Outcome::Expired, None, None)
                            .is_err()
                        {
                            continue;
                        }
                    }
                }
                Ok(None) => {}
            }
            // Still unpaid (or no payment record at all): cancel and give everything back.
            if matches!(self.order(id), Ok(Some(o)) if text(&o, "state") == "pending")
                && self.cancel_order(id).is_err()
            {
                continue;
            }
            if matches!(self.order(id), Ok(Some(o)) if text(&o, "state") == "cancelled") {
                n += 1;
            }
        }
        self.return_stranded_payments(&cancelled);
        n
    }

    /// A cancelled order whose payment is `succeeded` is money the shop holds for nothing: a late
    /// payment whose follow-up was lost (the process stopped after the payment was recorded, the
    /// event id could not be given back, a poll that swallowed the refusal), or a cancel whose
    /// refund was refused. The provider's redelivery is not a way to count on, so the sweep
    /// returns it, a few per sweep. Safe to repeat: the provider is given a fixed key.
    fn return_stranded_payments(&self, cancelled: &HashSet<&str>) {
        let Ok(payments) = self.backend.list(PAYMENT) else {
            return;
        };
        let t = now();
        let mut tried = 0;
        for (pid, p) in payments {
            if text(&p, "state") != "succeeded" || !cancelled.contains(text(&p, "order_id")) {
                continue;
            }
            let failed_lately = self
                .refund_failed
                .lock()
                .unwrap()
                .get(&pid)
                .is_some_and(|at| t.saturating_sub(*at) < 3600);
            if failed_lately {
                continue;
            }
            // What costs a round trip to the provider counts towards the few per sweep; a payment
            // that fails before it gets there (no such provider connection) does not.
            let calls = self.refund_calls.load(std::sync::atomic::Ordering::Relaxed);
            match self.refund_payment(&pid) {
                Ok(()) => {
                    self.refund_failed.lock().unwrap().remove(&pid);
                }
                // The store is away, not the provider: this is no failed refund (it would be left
                // alone for an hour), and asking about the others only waits for the pool once more
                // each, inside the checkout that started the sweep. The next sweep tries it again.
                Err(e) if busy(&e) || fault::noted().is_some() => break,
                Err(_) => {
                    self.refund_failed.lock().unwrap().insert(pid.clone(), t);
                }
            }
            if self.refund_calls.load(std::sync::atomic::Ordering::Relaxed) != calls {
                tried += 1;
                if tried >= 5 {
                    break;
                }
            }
        }
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
                // A pending order whose payment already succeeded (the follow-up of the event
                // was refused and waits for the provider's redelivery) must not be cancelled with
                // the money kept. The order is cancelled FIRST and the money returned after it:
                // a cancelled order can no longer be marked paid (by the sweep or by a
                // redelivery) while the refund is on its way or its record is refused, and a
                // refund that does not go through is finished by the sweep, which returns the
                // money of every cancelled order whose payment succeeded.
                let id = order_id(input);
                let owed = match &id {
                    Some(id) if matches!(me.order(id)?, Some(o) if text(&o, "state") == "pending") => {
                        match me.payment_of_order(id)? {
                            Some((pid, p)) if text(&p, "state") == "succeeded" => Some(pid),
                            _ => None,
                        }
                    }
                    _ => None,
                };
                let out = inner(input)?;
                if let Some(id) = &id {
                    // The order is cancelled. Closing its payment and giving the units and the
                    // code back are needed (nothing else would: the sweep visits pending orders):
                    // tried again, and a lasting refusal is a partial 503 (D74).
                    needed(|| me.close_payment_of(id))?;
                    needed(|| me.release_order(id, true).map(|_| ()))?;
                }
                if let Some(pid) = owed {
                    me.refund_payment(&pid).map_err(|e| {
                        if e.starts_with(MONEY_RETURNED) {
                            e
                        } else {
                            format!(
                                "the order was cancelled but the payment that had been received could not be returned yet: {e}; the sweep returns it"
                            )
                        }
                    })?;
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
                    needed(|| me.settle_by_hand(&id))?;
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
                    // Delivered goods do not go back on the shelf: nothing is owed for this
                    // order, and it says so BEFORE it moves to refunded, so that there is no
                    // moment at which a refunded order owes units that the sweep would return
                    // (a delivered order owes none whatever happens next). A note that helps the
                    // sweep along, not a change of the request, and a plain 503 if refused.
                    if !restock && let Some(id) = &id {
                        needed(|| fault::uncounted(|| me.mark_stock_settled(id)))?;
                    }
                    // A refused lookup of the payment is not "no payment": the order must not be
                    // refunded without the money (nothing has been changed yet, so this is a
                    // plain 503).
                    if let Some(id) = &id
                        && let Some((pid, p)) = me.payment_of_order(id)?
                        && text(&p, "state") == "succeeded"
                    {
                        me.refund_payment(&pid).map_err(|e| {
                            if e.starts_with(MONEY_RETURNED) {
                                e
                            } else {
                                format!("the refund was not made: {e}")
                            }
                        })?;
                    }
                    let out = inner(input)?;
                    if restock && let Some(id) = &id {
                        needed(|| me.release_order(id, false).map(|_| ()))?;
                    }
                    Ok(out)
                }),
            );
        }
    }

    /// Record that no units are owed for this order: the goods of a delivered order are with the
    /// buyer. Only while the order is delivered (a refund of any other order does give units
    /// back), so a repeat after the order moved on changes nothing.
    fn mark_stock_settled(&self, order_id: &str) -> Result<(), String> {
        let patch = Fields::from_iter([("stock_returned".to_string(), json!(true))]);
        match self.backend.apply_checked(&[Op::Patch {
            model: ORDER.into(),
            id: order_id.into(),
            patch,
            expect: vec![("state".into(), json!("delivered"))],
        }]) {
            Ok(()) | Err(ApplyError::Conflict { .. }) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Close the open payment of a cancelled order. A refusal of the store is an error (the step
    /// is tried again); the provider's own answer to `expire` is best effort, because a payment
    /// that still arrives for a closed attempt is returned (see `apply`).
    fn close_payment_of(&self, order: &str) -> Result<(), String> {
        let Some((pid, p)) = self.payment_of_order(order)? else {
            return Ok(());
        };
        if text(&p, "state") != "open" {
            return Ok(());
        }
        if let Some(provider) = self.provider(text(&p, "provider")) {
            let _ = provider.expire(text(&p, "external_id"));
        }
        let mut patch = Fields::new();
        patch.insert("state".into(), json!("cancelled"));
        match self.backend.apply_checked(&[Op::Patch {
            model: PAYMENT.into(),
            id: pid,
            patch,
            expect: vec![("state".into(), json!("open"))],
        }]) {
            // Somebody settled it between our read and our write: that answer stands.
            Ok(()) | Err(ApplyError::Conflict { .. }) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }

    fn settle_by_hand(&self, order: &str) -> Result<(), String> {
        let Some((pid, p)) = self.payment_of_order(order)? else {
            return Ok(());
        };
        if text(&p, "state") != "open" {
            return Ok(());
        }
        let mut seen = events_of(&p);
        seen.push("by-hand".into());
        let mut patch = Fields::new();
        patch.insert("state".into(), json!("succeeded"));
        patch.insert("event_ids".into(), json!(json!(seen).to_string()));
        match self.backend.apply_checked(&[Op::Patch {
            model: PAYMENT.into(),
            id: pid,
            patch,
            expect: vec![("state".into(), json!("open"))],
        }]) {
            Ok(()) | Err(ApplyError::Conflict { .. }) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
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
        let o = self.order(text(&p, "order_id")).ok().flatten()?;
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
