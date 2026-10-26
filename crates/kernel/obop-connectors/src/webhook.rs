//! Webhook signatures in both directions.
//!
//! Standard Webhooks (the default, used for webhooks we send and accept): `webhook-id`,
//! `webhook-timestamp` and `webhook-signature` (`v1,<base64 HMAC-SHA256>` over `id.timestamp.body`,
//! several space-separated signatures allowed so a secret can be rotated). Two further inbound
//! schemes cover well-known senders: GitHub (`X-Hub-Signature-256: sha256=<hex>` over the body)
//! and Stripe (`Stripe-Signature: t=<ts>,v1=<hex>` over `t.body`).
//!
//! Every check compares in constant time, rejects a timestamp outside the tolerance (both
//! directions, so a captured request cannot be replayed later) and, through [`Seen`], an id that
//! was accepted before.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use hmac::{Hmac, KeyInit, Mac};
use obop_storage::{Backend, Fields};
use sha2::Sha256;

use crate::secretbox::{hex, unhex};
use crate::util::ct_eq;

type HmacSha256 = Hmac<Sha256>;

pub const TOLERANCE_SECS: u64 = 300;
/// Largest body an inbound webhook may carry.
pub const MAX_BODY: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    Standard,
    GitHub,
    Stripe,
}

impl Scheme {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "standard" | "" => Some(Self::Standard),
            "github" => Some(Self::GitHub),
            "stripe" => Some(Self::Stripe),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyError {
    MissingHeader,
    BadTimestamp,
    Stale,
    BadSignature,
    Replay,
    /// The store was busy or unreachable: nothing was recorded, the sender should retry (D74).
    Storage,
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::MissingHeader => "missing signature header",
            Self::BadTimestamp => "bad timestamp",
            Self::Stale => "timestamp outside the tolerance",
            Self::BadSignature => "signature does not match",
            Self::Replay => "already received",
            Self::Storage => "the database is busy, try again",
        })
    }
}

/// The key bytes of a secret: `whsec_<base64>` is decoded (the Standard Webhooks convention),
/// anything else is used as its UTF-8 bytes.
pub fn secret_bytes(secret: &str) -> Vec<u8> {
    match secret.strip_prefix("whsec_") {
        Some(b) => B64.decode(b).unwrap_or_else(|_| secret.as_bytes().to_vec()),
        None => secret.as_bytes().to_vec(),
    }
}

fn mac(key: &[u8], parts: &[&[u8]]) -> Vec<u8> {
    let mut m = HmacSha256::new_from_slice(key).expect("HMAC takes any key length");
    for p in parts {
        m.update(p);
    }
    m.finalize().into_bytes().to_vec()
}

/// `v1,<base64>` over `id.timestamp.body`.
pub fn sign_standard(secret: &str, id: &str, ts: u64, body: &[u8]) -> String {
    let t = ts.to_string();
    let sig = mac(
        &secret_bytes(secret),
        &[id.as_bytes(), b".", t.as_bytes(), b".", body],
    );
    format!("v1,{}", B64.encode(sig))
}

/// The three headers to send with a webhook.
pub fn outgoing_headers(secret: &str, id: &str, ts: u64, body: &[u8]) -> Vec<(String, String)> {
    vec![
        ("webhook-id".into(), id.into()),
        ("webhook-timestamp".into(), ts.to_string()),
        (
            "webhook-signature".into(),
            sign_standard(secret, id, ts, body),
        ),
    ]
}

/// What an accepted request tells the caller: an id for replay protection.
#[derive(Debug, PartialEq, Eq)]
pub struct Verified {
    pub event_id: String,
}

fn age_ok(ts: u64, now: u64) -> Result<(), VerifyError> {
    if ts.abs_diff(now) > TOLERANCE_SECS {
        Err(VerifyError::Stale)
    } else {
        Ok(())
    }
}

/// Verify an inbound request. `header` looks a header up by lowercase name.
pub fn verify(
    scheme: Scheme,
    secret: &str,
    header: &dyn Fn(&str) -> Option<String>,
    body: &[u8],
    now: u64,
) -> Result<Verified, VerifyError> {
    let key = secret_bytes(secret);
    let body_id = || format!("sha256:{}", crate::util::sha256_hex(body));
    match scheme {
        Scheme::Standard => {
            let id = header("webhook-id").ok_or(VerifyError::MissingHeader)?;
            let ts: u64 = header("webhook-timestamp")
                .ok_or(VerifyError::MissingHeader)?
                .parse()
                .map_err(|_| VerifyError::BadTimestamp)?;
            let sigs = header("webhook-signature").ok_or(VerifyError::MissingHeader)?;
            age_ok(ts, now)?;
            let want = mac(
                &key,
                &[id.as_bytes(), b".", ts.to_string().as_bytes(), b".", body],
            );
            // Every listed signature is compared, so the time taken does not say which matched.
            let mut ok = false;
            for s in sigs.split(' ') {
                if let Some(b) = s.strip_prefix("v1,")
                    && let Ok(got) = B64.decode(b)
                {
                    ok |= ct_eq(&got, &want);
                }
            }
            if ok {
                Ok(Verified { event_id: id })
            } else {
                Err(VerifyError::BadSignature)
            }
        }
        Scheme::GitHub => {
            let sig = header("x-hub-signature-256").ok_or(VerifyError::MissingHeader)?;
            let got = sig
                .strip_prefix("sha256=")
                .and_then(unhex)
                .ok_or(VerifyError::BadSignature)?;
            if ct_eq(&got, &mac(&key, &[body])) {
                Ok(Verified {
                    event_id: header("x-github-delivery").unwrap_or_else(body_id),
                })
            } else {
                Err(VerifyError::BadSignature)
            }
        }
        Scheme::Stripe => {
            let h = header("stripe-signature").ok_or(VerifyError::MissingHeader)?;
            let mut ts = None;
            let mut sigs = Vec::new();
            for part in h.split(',') {
                match part.split_once('=') {
                    Some(("t", v)) => ts = v.parse::<u64>().ok(),
                    Some(("v1", v)) => sigs.push(v.to_string()),
                    _ => {}
                }
            }
            let ts = ts.ok_or(VerifyError::BadTimestamp)?;
            age_ok(ts, now)?;
            let want = mac(&key, &[ts.to_string().as_bytes(), b".", body]);
            let mut ok = false;
            for s in &sigs {
                if let Some(got) = unhex(s) {
                    ok |= ct_eq(&got, &want);
                }
            }
            if ok {
                Ok(Verified {
                    event_id: body_id(),
                })
            } else {
                Err(VerifyError::BadSignature)
            }
        }
    }
}

/// Test helper: the hex a Stripe-style sender would put in `v1=`.
pub fn stripe_signature(secret: &str, ts: u64, body: &[u8]) -> String {
    hex(&mac(
        &secret_bytes(secret),
        &[ts.to_string().as_bytes(), b".", body],
    ))
}

/// Ids of accepted webhooks, so a captured request cannot be replayed inside the tolerance. The
/// ids live in the database (they survive a restart) under the model `connector_seen`.
pub struct Seen;

impl Seen {
    /// Record `event_id` for `connection`; `Err(Replay)` if it was recorded before.
    pub fn claim(
        backend: &dyn Backend,
        connection: &str,
        event_id: &str,
        now: u64,
    ) -> Result<(), VerifyError> {
        let id = format!(
            "{connection}:{}",
            crate::util::sha256_hex(event_id.as_bytes())
        );
        let mut f = Fields::new();
        f.insert("at".into(), now.into());
        match backend.insert("connector_seen", &id, &f) {
            Ok(()) => Ok(()),
            Err(e) => match backend.get("connector_seen", &id) {
                Ok(Some(_)) => Err(VerifyError::Replay),
                // A refused store is neither "new" nor "seen": the event is not taken (D74). The
                // sender retries, and nothing was recorded that could swallow the retry.
                Err(_) => Err(VerifyError::Storage),
                Ok(None) if obop_storage::fault::Fault::is_transient(&e) => {
                    Err(VerifyError::Storage)
                }
                // A refusal that is neither a record nor a fault: do not block the delivery.
                Ok(None) => Ok(()),
            },
        }
    }

    /// Give a claim back: the event was not applied, so the sender's retry must not be taken for a
    /// replay. Allowed after a refused call (it undoes what this request did) and tried again
    /// while the store is busy.
    pub fn release(backend: &dyn Backend, connection: &str, event_id: &str) {
        let id = format!(
            "{connection}:{}",
            crate::util::sha256_hex(event_id.as_bytes())
        );
        let r = obop_storage::fault::patiently(4, std::time::Duration::from_millis(150), || {
            obop_storage::fault::bookkeeping(|| backend.delete("connector_seen", &id))
        });
        if let Err(e) = r {
            eprintln!("webhooks: could not release the claim of an event: {e}");
        }
    }

    /// Forget ids older than the tolerance (they can no longer pass the timestamp check).
    pub fn sweep(backend: &dyn Backend, now: u64) -> usize {
        let mut n = 0;
        for (id, f) in backend.list("connector_seen").unwrap_or_default() {
            if f.get("at").and_then(|v| v.as_u64()).unwrap_or(0) + 2 * TOLERANCE_SECS < now
                && backend.delete("connector_seen", &id).unwrap_or(false)
            {
                n += 1;
            }
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const NOW: u64 = 1_800_000_000;

    fn headers(pairs: &[(&str, String)]) -> impl Fn(&str) -> Option<String> + use<> {
        let m: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        move |k| m.get(k).cloned()
    }

    fn standard(
        secret: &str,
        id: &str,
        ts: u64,
        body: &[u8],
    ) -> impl Fn(&str) -> Option<String> + use<> {
        headers(&[
            ("webhook-id", id.into()),
            ("webhook-timestamp", ts.to_string()),
            ("webhook-signature", sign_standard(secret, id, ts, body)),
        ])
    }

    #[test]
    fn standard_webhooks_known_vector() {
        // Vector from the Standard Webhooks specification repository.
        let secret = "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw";
        let body = br#"{"test": 2432232314}"#;
        let sig = sign_standard(secret, "msg_p5jXN8AQM9LWM0D4loKWxJek", 1_614_265_330, body);
        assert_eq!(sig, "v1,g0hM9SsE+OTPJTGt/tmIKtSyZlE3uFJELVlNIOLJ1OE=");
    }

    #[test]
    fn accepts_a_valid_signature_and_rejects_every_tampering() {
        let (secret, body) = (
            "whsec_c2VjcmV0LWtleS1mb3ItdGVzdHM=",
            b"{\"x\":1}".as_slice(),
        );
        let h = standard(secret, "evt_1", NOW, body);
        assert_eq!(
            verify(Scheme::Standard, secret, &h, body, NOW)
                .unwrap()
                .event_id,
            "evt_1"
        );
        assert_eq!(
            verify(Scheme::Standard, secret, &h, b"{\"x\":2}", NOW),
            Err(VerifyError::BadSignature)
        );
        assert_eq!(
            verify(Scheme::Standard, "other", &h, body, NOW),
            Err(VerifyError::BadSignature)
        );
        let none = headers(&[]);
        assert_eq!(
            verify(Scheme::Standard, secret, &none, body, NOW),
            Err(VerifyError::MissingHeader)
        );
        // A different id or timestamp changes the signed content.
        let forged = headers(&[
            ("webhook-id", "evt_2".into()),
            ("webhook-timestamp", NOW.to_string()),
            (
                "webhook-signature",
                sign_standard(secret, "evt_1", NOW, body),
            ),
        ]);
        assert_eq!(
            verify(Scheme::Standard, secret, &forged, body, NOW),
            Err(VerifyError::BadSignature)
        );
    }

    #[test]
    fn timestamp_tolerance_applies_both_ways() {
        let (secret, body) = ("s3cret", b"{}".as_slice());
        let old = standard(secret, "e", NOW - 301, body);
        assert_eq!(
            verify(Scheme::Standard, secret, &old, body, NOW),
            Err(VerifyError::Stale)
        );
        let future = standard(secret, "e", NOW + 301, body);
        assert_eq!(
            verify(Scheme::Standard, secret, &future, body, NOW),
            Err(VerifyError::Stale)
        );
        let edge = standard(secret, "e", NOW - 300, body);
        assert!(verify(Scheme::Standard, secret, &edge, body, NOW).is_ok());
        let junk = headers(&[
            ("webhook-id", "e".into()),
            ("webhook-timestamp", "yesterday".into()),
            ("webhook-signature", "v1,AAAA".into()),
        ]);
        assert_eq!(
            verify(Scheme::Standard, secret, &junk, body, NOW),
            Err(VerifyError::BadTimestamp)
        );
    }

    #[test]
    fn a_rotated_secret_is_accepted_through_a_signature_list() {
        let body = b"{}".as_slice();
        let list = format!(
            "{} {}",
            sign_standard("old-secret", "e", NOW, body),
            sign_standard("new-secret", "e", NOW, body)
        );
        let h = headers(&[
            ("webhook-id", "e".into()),
            ("webhook-timestamp", NOW.to_string()),
            ("webhook-signature", list),
        ]);
        assert!(verify(Scheme::Standard, "new-secret", &h, body, NOW).is_ok());
        assert!(verify(Scheme::Standard, "old-secret", &h, body, NOW).is_ok());
        assert!(verify(Scheme::Standard, "third", &h, body, NOW).is_err());
    }

    #[test]
    fn github_scheme() {
        let (secret, body) = ("It's a Secret to Everybody", b"Hello, World!".as_slice());
        // Vector from GitHub's documentation.
        let h = headers(&[(
            "x-hub-signature-256",
            "sha256=757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17".into(),
        )]);
        assert!(verify(Scheme::GitHub, secret, &h, body, NOW).is_ok());
        let bad = headers(&[("x-hub-signature-256", "sha256=00".into())]);
        assert_eq!(
            verify(Scheme::GitHub, secret, &bad, body, NOW),
            Err(VerifyError::BadSignature)
        );
        let none = headers(&[]);
        assert_eq!(
            verify(Scheme::GitHub, secret, &none, body, NOW),
            Err(VerifyError::MissingHeader)
        );
    }

    #[test]
    fn stripe_scheme() {
        let (secret, body) = ("whsec_test", br#"{"id":"evt_1"}"#.as_slice());
        let good = headers(&[(
            "stripe-signature",
            format!(
                "t={NOW},v1={},v1=deadbeef",
                stripe_signature(secret, NOW, body)
            ),
        )]);
        assert!(verify(Scheme::Stripe, secret, &good, body, NOW).is_ok());
        let stale = headers(&[(
            "stripe-signature",
            format!(
                "t={},v1={}",
                NOW - 1000,
                stripe_signature(secret, NOW - 1000, body)
            ),
        )]);
        assert_eq!(
            verify(Scheme::Stripe, secret, &stale, body, NOW),
            Err(VerifyError::Stale)
        );
        let bad = headers(&[("stripe-signature", format!("t={NOW},v1=00"))]);
        assert_eq!(
            verify(Scheme::Stripe, secret, &bad, body, NOW),
            Err(VerifyError::BadSignature)
        );
    }

    #[test]
    fn a_store_that_refuses_is_neither_a_new_event_nor_a_replay_and_a_claim_can_be_given_back() {
        use obop_storage::fault;
        use obop_storage::testing::{Flaky, Mode};
        // D74: an event met by a busy store is not taken (the sender retries), a claim that was
        // made can be released even after a refused call, and a released id is new again.
        let (flaky, b) = Flaky::memory();
        flaky.set(Mode::All);
        assert_eq!(
            Seen::claim(&*b, "c1", "evt_1", NOW),
            Err(VerifyError::Storage)
        );
        flaky.set(Mode::Healthy);
        assert!(
            Seen::claim(&*b, "c1", "evt_1", NOW).is_ok(),
            "nothing was recorded"
        );
        assert_eq!(
            Seen::claim(&*b, "c1", "evt_1", NOW),
            Err(VerifyError::Replay)
        );
        fault::begin_request(Default::default());
        flaky.refuse_next(1);
        assert!(b.get("connector_seen", "x").is_err());
        Seen::release(&*b, "c1", "evt_1");
        let out = fault::end_request();
        assert!(!out.wrote, "a dedup marker is not a change: {out:?}");
        assert!(
            Seen::claim(&*b, "c1", "evt_1", NOW).is_ok(),
            "the id is new again"
        );
    }

    #[test]
    fn replays_are_refused_and_swept() {
        let b = obop_storage::open("memory:").unwrap();
        assert!(Seen::claim(&*b, "c1", "evt_1", NOW).is_ok());
        assert_eq!(
            Seen::claim(&*b, "c1", "evt_1", NOW),
            Err(VerifyError::Replay)
        );
        // The same id on another connection is a different event.
        assert!(Seen::claim(&*b, "c2", "evt_1", NOW).is_ok());
        assert_eq!(Seen::sweep(&*b, NOW + 100), 0);
        assert_eq!(Seen::sweep(&*b, NOW + 700), 2);
        assert!(Seen::claim(&*b, "c1", "evt_1", NOW + 700).is_ok());
    }

    #[test]
    fn outgoing_headers_verify_with_our_own_verifier() {
        let body = br#"{"hello":"world"}"#;
        let h = outgoing_headers("shared", "msg_1", NOW, body);
        let get = move |k: &str| h.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        assert!(verify(Scheme::Standard, "shared", &get, body, NOW).is_ok());
    }
}
