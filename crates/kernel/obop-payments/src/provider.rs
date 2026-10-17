//! What the shop needs from a company that takes payments (D62), and nothing more.
//!
//! The shop never sees card data: the buyer pays on the provider's own page (PCI scope SAQ A).
//! All amounts are whole minor currency units and all of them come from the stored order. A
//! provider is asked four things: make a payment page for this amount, check a notification it
//! sent, tell me what became of this payment, give this money back.

use std::fmt;

/// What the shop asks the provider to make a payment page for.
#[derive(Debug, Clone)]
pub struct SessionRequest {
    /// The shop's own payment record (also the idempotency key).
    pub payment_id: String,
    pub number: String,
    pub amount: i64,
    pub currency: String,
    pub success_url: String,
    pub cancel_url: String,
    /// What the provider shows as the thing being bought: "Order W20261016-K7Q2ZD".
    pub title: String,
    /// Unix time after which the page no longer takes payment.
    pub expires_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// The provider's reference for the page (a Checkout Session id).
    pub external_id: String,
    /// Where to send the buyer; `None` when there is no page (bank transfer).
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Paid,
    Failed,
    Expired,
    Cancelled,
}

/// What the provider says about a payment right now. `outcome` is `None` while the buyer can
/// still pay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub outcome: Option<Outcome>,
    pub amount: Option<i64>,
    pub currency: Option<String>,
}

/// One verified notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// The provider's id of the event: the same notification delivered twice has the same id.
    pub id: String,
    pub external_id: String,
    pub outcome: Outcome,
    pub amount: Option<i64>,
    pub currency: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefundDone {
    pub id: String,
    pub amount: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    /// This provider cannot do that (a bank transfer has no page to expire).
    Unsupported,
    /// The provider refused; the text is safe to show to staff, never contains a key.
    Rejected(String),
    /// Could not reach the provider or it failed on its side: try again later.
    Unavailable(String),
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => f.write_str("the payment provider does not support this"),
            Self::Rejected(m) => write!(f, "the payment provider refused: {m}"),
            Self::Unavailable(m) => write!(f, "the payment provider is not available: {m}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookError {
    /// Wrong or missing signature, stale timestamp: all alike.
    Unauthorized,
    /// Signed, but not a notification this provider sends.
    Malformed,
    Unsupported,
}

pub trait PaymentProvider: Send + Sync {
    fn id(&self) -> &'static str;

    /// Make the page the buyer pays on.
    fn create_session(&self, req: &SessionRequest) -> Result<Session, ProviderError>;

    /// Check a delivery (`header` looks a header up by lowercase name) against the provider's
    /// signing secret and return the events in it. Nothing in the body is believed before the
    /// signature has been checked on the raw bytes.
    fn verify_webhook(
        &self,
        header: &dyn Fn(&str) -> Option<String>,
        body: &[u8],
        now: u64,
    ) -> Result<Vec<Event>, WebhookError>;

    /// Ask the provider what became of a payment (used when the buyer comes back, and when a
    /// notification may have been missed).
    fn fetch_status(&self, external_id: &str) -> Result<Status, ProviderError>;

    /// Give `amount` back. `key` makes the call safe to repeat: the same key never refunds
    /// twice.
    fn refund(
        &self,
        external_id: &str,
        amount: i64,
        key: &str,
    ) -> Result<RefundDone, ProviderError>;

    /// Close a page nobody paid, so the buyer cannot pay it later. Best effort.
    fn expire(&self, _external_id: &str) -> Result<(), ProviderError> {
        Ok(())
    }
}
