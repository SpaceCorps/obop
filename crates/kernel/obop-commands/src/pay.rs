//! The seam between the public shop and whatever takes payments (D62).
//!
//! `obop-site` (which makes no outbound requests and holds no secrets) renders checkout and the
//! payment pages and calls this trait; `obop-payments` implements it with the manual, Stripe and
//! demo providers and owns the provider keys, the webhook and the refund. A shop without an
//! implementation behaves as before: the order stays pending and the shop contacts the buyer.
//!
//! Amounts are whole minor currency units and always come from the stored order, never from the
//! browser. The port never sees card data: the buyer pays on the provider's own page.

/// What the site hands over when an order has just been created.
#[derive(Debug, Clone)]
pub struct StartPayment {
    /// The new `ecommerce_order` record.
    pub order_id: String,
    /// `manual`, `stripe` or `fake`.
    pub method: String,
    /// What the buyer owes, from the stored order (minor currency units).
    pub amount: i64,
    pub currency: String,
    /// The order number, shown on the provider's page.
    pub number: String,
    /// Where the provider sends the buyer back (the pay page of this order; carries the order
    /// token). The page never believes the redirect: it asks the server for the state.
    pub return_url: String,
    pub cancel_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Started {
    /// Nothing to do online: the order page shows the instructions.
    Manual,
    /// Send the buyer to the provider's hosted page.
    Redirect(String),
}

/// What the pay page shows about the payment of an order, as the server knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentView {
    pub provider: String,
    /// created, open, succeeded, failed, expired, cancelled or refunded.
    pub state: String,
    pub amount: i64,
    pub currency: String,
}

/// The local demo provider's hosted page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeView {
    /// The order's access token (so the page can send the buyer back to it).
    pub order_token: String,
    pub number: String,
    pub amount: i64,
    pub currency: String,
    pub state: String,
}

pub trait PaymentPort: Send + Sync {
    /// The payment methods that can be offered right now, out of the ones the shop's settings
    /// list (a method that needs a key or a switch is left out until it is configured).
    fn methods(&self, configured: &[String]) -> Vec<String>;
    /// Create the payment for a new order. `Err` carries a message safe to show; on `Err` the
    /// implementation has already cancelled the order and put its stock and discount back, so
    /// the buyer can simply try again.
    fn start(&self, req: &StartPayment) -> Result<Started, String>;
    /// Where to send the buyer to carry on paying an order whose payment is still open (the
    /// provider's page), if there is one.
    fn resume(&self, order_id: &str) -> Option<String>;
    /// Bring the payment of an order up to date from the provider (never from the browser) and
    /// say what it is. `None` when the order has no payment record.
    fn refresh(&self, order_id: &str) -> Option<PaymentView>;
    /// The demo provider's session, by its unguessable id.
    fn fake_session(&self, session: &str) -> Option<FakeView>;
    /// The buyer's choice on the demo page: `paid`, `failed` or `cancelled`.
    fn fake_decide(&self, session: &str, outcome: &str) -> Result<(), String>;
    /// Cancel unpaid orders whose reservation ran out and give their stock back. Cheap; called
    /// from checkout, so it needs no timer.
    fn sweep(&self);
}
