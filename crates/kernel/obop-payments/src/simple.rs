//! The two providers that call nobody: bank transfer and the local demo.

use crate::provider::*;
use obop_storage::{Backend, Fields};
use serde_json::json;
use std::sync::Arc;

/// Pay by bank transfer or invoice. There is no payment page and no notification: the buyer is
/// shown the instructions the owner wrote, and staff mark the order paid when the money arrives.
/// A refund is recorded; the money is returned by hand.
pub struct Manual;

impl PaymentProvider for Manual {
    fn id(&self) -> &'static str {
        "manual"
    }
    fn create_session(&self, _: &SessionRequest) -> Result<Session, ProviderError> {
        Ok(Session {
            external_id: String::new(),
            url: None,
        })
    }
    fn verify_webhook(
        &self,
        _: &dyn Fn(&str) -> Option<String>,
        _: &[u8],
        _: u64,
    ) -> Result<Vec<Event>, WebhookError> {
        Err(WebhookError::Unsupported)
    }
    fn fetch_status(&self, _: &str) -> Result<Status, ProviderError> {
        Ok(Status {
            outcome: None,
            amount: None,
            currency: None,
        })
    }
    fn refund(&self, _: &str, amount: i64, key: &str) -> Result<RefundDone, ProviderError> {
        Ok(RefundDone {
            id: format!("manual-{key}"),
            amount,
        })
    }
}

pub const FAKE_MODEL: &str = "obop_fake_payment";

/// A provider that exists only for demos and tests: its "hosted page" is a page of this very
/// site, and nothing leaves the machine. It must be switched on by the operator
/// (`OBOP_DEMO_PAYMENTS=1`) and by the owner (the `fake` payment method in the site settings).
pub struct Fake {
    backend: Arc<dyn Backend>,
}

impl Fake {
    pub fn new(backend: Arc<dyn Backend>) -> Fake {
        Fake { backend }
    }

    pub fn state(&self, session: &str) -> Option<String> {
        self.backend
            .get(FAKE_MODEL, session)
            .ok()
            .flatten()
            .and_then(|f| f.get("state")?.as_str().map(String::from))
    }

    pub fn payment_of(&self, session: &str) -> Option<String> {
        self.backend
            .get(FAKE_MODEL, session)
            .ok()
            .flatten()
            .and_then(|f| f.get("payment")?.as_str().map(String::from))
    }

    pub fn settle(&self, session: &str, outcome: Outcome) -> bool {
        let state = match outcome {
            Outcome::Paid => "paid",
            Outcome::Failed => "failed",
            Outcome::Expired => "expired",
            Outcome::Cancelled => "cancelled",
        };
        let mut f = Fields::new();
        f.insert("state".into(), json!(state));
        self.backend
            .update(FAKE_MODEL, session, &f)
            .unwrap_or(false)
    }
}

impl PaymentProvider for Fake {
    fn id(&self) -> &'static str {
        "fake"
    }

    fn create_session(&self, r: &SessionRequest) -> Result<Session, ProviderError> {
        let mut raw = [0u8; 32];
        getrandom::fill(&mut raw)
            .map_err(|_| ProviderError::Unavailable("no randomness".into()))?;
        let session: String = raw.iter().map(|b| format!("{b:02x}")).collect();
        let mut f = Fields::new();
        f.insert("payment".into(), json!(r.payment_id));
        f.insert("state".into(), json!("open"));
        f.insert("amount".into(), json!(r.amount));
        self.backend
            .insert(FAKE_MODEL, &session, &f)
            .map_err(ProviderError::Unavailable)?;
        Ok(Session {
            url: Some(format!("/shop/fake-pay/{session}")),
            external_id: session,
        })
    }

    fn verify_webhook(
        &self,
        _: &dyn Fn(&str) -> Option<String>,
        _: &[u8],
        _: u64,
    ) -> Result<Vec<Event>, WebhookError> {
        // The demo settles in-process; it has no notifications to forge.
        Err(WebhookError::Unsupported)
    }

    fn fetch_status(&self, external_id: &str) -> Result<Status, ProviderError> {
        let outcome = match self.state(external_id).as_deref() {
            Some("paid") => Some(Outcome::Paid),
            Some("failed") => Some(Outcome::Failed),
            Some("expired") => Some(Outcome::Expired),
            Some("cancelled") => Some(Outcome::Cancelled),
            Some(_) => None,
            None => return Err(ProviderError::Rejected("no such demo session".into())),
        };
        Ok(Status {
            outcome,
            amount: self
                .backend
                .get(FAKE_MODEL, external_id)
                .ok()
                .flatten()
                .and_then(|f| f.get("amount")?.as_i64()),
            currency: None,
        })
    }

    fn refund(&self, _: &str, amount: i64, key: &str) -> Result<RefundDone, ProviderError> {
        Ok(RefundDone {
            id: format!("fake-refund-{key}"),
            amount,
        })
    }

    fn expire(&self, external_id: &str) -> Result<(), ProviderError> {
        // An unpaid demo page is closed like a real one.
        if self.state(external_id).as_deref() == Some("open") {
            self.settle(external_id, Outcome::Expired);
        }
        Ok(())
    }
}
