//! Stripe Checkout Sessions over the REST API (D62).
//!
//! - Calls go through the connectors' one hardened egress client (`net::Client`): https only to
//!   the host allow-list, every resolved address checked, no redirects, timeouts and size caps.
//!   Stripe's own domain is the only default; a test points the base at a local stand-in.
//! - The secret key is the instance's own, kept sealed as the credential of its `stripe`
//!   connection (`api_key`, and `signing_secret` for webhooks). It is read per call, sent only as
//!   a bearer header, and never logged or put into an error.
//! - Only the server-computed order total and the order number are sent: one line item named
//!   "Order <number>". No customer name, address or email leaves for Stripe, so this is not a
//!   personal-data flow of the connectors' kind; the buyer types those into Stripe's page.
//! - Webhooks are verified on the raw body with the `Stripe-Signature` scheme (HMAC-SHA256 over
//!   `timestamp.body`, five minutes either way, constant time) before the body is read as JSON.
//!
//! Not tested against Stripe itself: the encodings, the signature and the event handling are
//! tested against a local stand-in that speaks the same shapes.

use crate::provider::*;
use obop_connectors::net::{Client, Request};
use obop_connectors::webhook::{self, Scheme};
use serde_json::Value as J;

#[derive(Clone)]
pub struct StripeKeys {
    pub api_key: String,
    pub signing_secret: String,
}

impl std::fmt::Debug for StripeKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("StripeKeys(..)")
    }
}

pub struct Stripe {
    client: Client,
    base: String,
    hosts: Vec<String>,
    keys: StripeKeys,
}

/// A Checkout Session id as Stripe makes them: `cs_`, then letters, digits and underscores.
pub fn session_id_ok(id: &str) -> bool {
    id.starts_with("cs_")
        && id.len() <= 200
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn form(pairs: &[(String, String)]) -> Vec<u8> {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish()
        .into_bytes()
}

fn clip(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .take(200)
        .collect::<String>()
}

impl Stripe {
    pub fn new(client: Client, base: &str, hosts: Vec<String>, keys: StripeKeys) -> Stripe {
        Stripe {
            client,
            base: base.trim_end_matches('/').to_string(),
            hosts,
            keys,
        }
    }

    fn call(
        &self,
        method: &str,
        path: &str,
        body: &[(String, String)],
        idempotency_key: Option<&str>,
    ) -> Result<J, ProviderError> {
        let url = format!("{}{path}", self.base);
        let mut headers = vec![
            (
                "authorization".to_string(),
                format!("Bearer {}", self.keys.api_key),
            ),
            ("accept".to_string(), "application/json".to_string()),
        ];
        if method == "POST" {
            headers.push((
                "content-type".to_string(),
                "application/x-www-form-urlencoded".to_string(),
            ));
        }
        if let Some(k) = idempotency_key {
            headers.push(("idempotency-key".to_string(), k.to_string()));
        }
        let resp = self
            .client
            .send(Request {
                method,
                url: &url,
                headers,
                body: (method == "POST").then(|| form(body)),
                allowed_hosts: &self.hosts,
            })
            .map_err(|e| ProviderError::Unavailable(e.to_string()))?;
        let v: J = serde_json::from_slice(&resp.body).unwrap_or(J::Null);
        match resp.status {
            200..=299 => Ok(v),
            500..=599 | 429 => Err(ProviderError::Unavailable(format!(
                "status {}",
                resp.status
            ))),
            _ => Err(ProviderError::Rejected(clip(
                v["error"]["message"]
                    .as_str()
                    .unwrap_or("the request was refused"),
            ))),
        }
    }

    fn outcome_of(v: &J) -> Option<Outcome> {
        match (v["status"].as_str(), v["payment_status"].as_str()) {
            (Some("complete"), Some("paid" | "no_payment_required")) => Some(Outcome::Paid),
            (Some("expired"), _) => Some(Outcome::Expired),
            _ => None,
        }
    }
}

impl PaymentProvider for Stripe {
    fn id(&self) -> &'static str {
        "stripe"
    }

    fn create_session(&self, r: &SessionRequest) -> Result<Session, ProviderError> {
        let p = |k: &str, v: &str| (k.to_string(), v.to_string());
        let form = vec![
            p("mode", "payment"),
            p("success_url", &r.success_url),
            p("cancel_url", &r.cancel_url),
            p("client_reference_id", &r.payment_id),
            p("expires_at", &r.expires_at.to_string()),
            p("line_items[0][quantity]", "1"),
            p(
                "line_items[0][price_data][currency]",
                &r.currency.to_lowercase(),
            ),
            p(
                "line_items[0][price_data][unit_amount]",
                &r.amount.to_string(),
            ),
            p("line_items[0][price_data][product_data][name]", &r.title),
            p("metadata[payment_id]", &r.payment_id),
            p("metadata[order_number]", &r.number),
            p("payment_intent_data[metadata][payment_id]", &r.payment_id),
        ];
        let v = self.call("POST", "/v1/checkout/sessions", &form, Some(&r.payment_id))?;
        let id = v["id"].as_str().unwrap_or("");
        let url = v["url"].as_str().unwrap_or("");
        if !session_id_ok(id) || !url.starts_with("http") {
            return Err(ProviderError::Rejected(
                "the answer was not a Checkout Session".into(),
            ));
        }
        Ok(Session {
            external_id: id.to_string(),
            url: Some(url.to_string()),
        })
    }

    fn verify_webhook(
        &self,
        header: &dyn Fn(&str) -> Option<String>,
        body: &[u8],
        now: u64,
    ) -> Result<Vec<Event>, WebhookError> {
        webhook::verify(Scheme::Stripe, &self.keys.signing_secret, header, body, now)
            .map_err(|_| WebhookError::Unauthorized)?;
        let v: J = serde_json::from_slice(body).map_err(|_| WebhookError::Malformed)?;
        let id = v["id"].as_str().filter(|i| !i.is_empty() && i.len() <= 100);
        let kind = v["type"].as_str().unwrap_or("");
        let obj = &v["data"]["object"];
        let outcome = match kind {
            "checkout.session.completed" => match obj["payment_status"].as_str() {
                Some("paid" | "no_payment_required") => Outcome::Paid,
                // An asynchronous method (a bank debit) completes later with its own event.
                _ => return Ok(vec![]),
            },
            "checkout.session.async_payment_succeeded" => Outcome::Paid,
            "checkout.session.async_payment_failed" => Outcome::Failed,
            "checkout.session.expired" => Outcome::Expired,
            _ => return Ok(vec![]),
        };
        let (Some(id), Some(ext)) = (id, obj["id"].as_str().filter(|s| session_id_ok(s))) else {
            return Err(WebhookError::Malformed);
        };
        Ok(vec![Event {
            id: id.to_string(),
            external_id: ext.to_string(),
            outcome,
            amount: obj["amount_total"].as_i64(),
            currency: obj["currency"].as_str().map(str::to_uppercase),
        }])
    }

    fn fetch_status(&self, external_id: &str) -> Result<Status, ProviderError> {
        if !session_id_ok(external_id) {
            return Err(ProviderError::Rejected("not a Checkout Session id".into()));
        }
        let v = self.call(
            "GET",
            &format!("/v1/checkout/sessions/{external_id}"),
            &[],
            None,
        )?;
        Ok(Status {
            outcome: Self::outcome_of(&v),
            amount: v["amount_total"].as_i64(),
            currency: v["currency"].as_str().map(str::to_uppercase),
        })
    }

    fn refund(
        &self,
        external_id: &str,
        amount: i64,
        key: &str,
    ) -> Result<RefundDone, ProviderError> {
        if !session_id_ok(external_id) {
            return Err(ProviderError::Rejected("not a Checkout Session id".into()));
        }
        let s = self.call(
            "GET",
            &format!("/v1/checkout/sessions/{external_id}"),
            &[],
            None,
        )?;
        let pi = s["payment_intent"]
            .as_str()
            .filter(|p| !p.is_empty() && p.len() <= 100)
            .ok_or_else(|| {
                ProviderError::Rejected("the session has no payment to refund".into())
            })?;
        let form = vec![
            ("payment_intent".to_string(), pi.to_string()),
            ("amount".to_string(), amount.to_string()),
            ("reason".to_string(), "requested_by_customer".to_string()),
        ];
        let v = self.call("POST", "/v1/refunds", &form, Some(key))?;
        match (v["id"].as_str(), v["status"].as_str()) {
            (Some(id), Some("succeeded" | "pending")) => Ok(RefundDone {
                id: id.to_string(),
                amount: v["amount"].as_i64().unwrap_or(amount),
            }),
            _ => Err(ProviderError::Rejected(
                "the refund was not accepted".into(),
            )),
        }
    }

    fn expire(&self, external_id: &str) -> Result<(), ProviderError> {
        if !session_id_ok(external_id) {
            return Err(ProviderError::Rejected("not a Checkout Session id".into()));
        }
        self.call(
            "POST",
            &format!("/v1/checkout/sessions/{external_id}/expire"),
            &[],
            None,
        )
        .map(|_| ())
    }
}
