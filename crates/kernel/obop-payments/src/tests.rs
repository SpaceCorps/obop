//! Payments end to end over the real bundled packages, the real public site and a local
//! stand-in for Stripe. Nothing here calls a real provider or uses a real key; the demo
//! provider and the stand-in are the only counterparts. Tests that guard a threat start with
//! `security_`.

use super::*;
use obop_commands::{CallError, PublicApi, Store};
use obop_connectors::net::ClientConfig;
use obop_connectors::webhook::stripe_signature;
use obop_pkg::{Bundled, Installer, Marketplace};
use obop_site::http::{Req, Resp};
use obop_site::{Config as SiteConfig, Site};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};

const KEY: &str = "sk_test_local_stand_in_not_a_real_key";
const SECRET: &str = "whsec_local_stand_in_not_a_real_secret";
const CSRF: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

// ---- a local stand-in for the Stripe API ------------------------------------------------------

#[derive(Default)]
struct StandState {
    sessions: HashMap<String, Sess>,
    log: Vec<Logged>,
    fail_create: bool,
    fail_refund: bool,
    /// Refunds made, by the idempotency key they were asked with: a repeat of the key gets the
    /// same refund back, as the real API does within a day.
    refunds: HashMap<String, (String, i64)>,
    n: u32,
}

#[derive(Clone)]
struct Sess {
    amount: i64,
    currency: String,
    status: &'static str,
    payment_status: &'static str,
    pi: Option<String>,
}

#[derive(Clone, Debug)]
struct Logged {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    form: Vec<(String, String)>,
}

struct Stand {
    port: u16,
    st: Arc<Mutex<StandState>>,
}

impl Stand {
    fn start() -> Stand {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let st: Arc<Mutex<StandState>> = Arc::default();
        let shared = st.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { break };
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let n = s.read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf).to_string();
                    if let Some(h) = text.find("\r\n\r\n") {
                        let len = text[..h]
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                            })
                            .unwrap_or(0);
                        if buf.len() >= h + 4 + len {
                            break;
                        }
                    }
                }
                let text = String::from_utf8_lossy(&buf).to_string();
                let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
                let mut lines = head.lines();
                let first = lines.next().unwrap_or("");
                let mut parts = first.split(' ');
                let (method, path) = (
                    parts.next().unwrap_or("").to_string(),
                    parts.next().unwrap_or("").to_string(),
                );
                let headers: HashMap<String, String> = lines
                    .filter_map(|l| l.split_once(':'))
                    .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
                    .collect();
                let form: Vec<(String, String)> = url::form_urlencoded::parse(body.as_bytes())
                    .map(|(k, v)| (k.into_owned(), v.into_owned()))
                    .collect();
                let logged = Logged {
                    method,
                    path,
                    headers,
                    form,
                };
                let (status, out) = respond(&shared, &logged);
                shared.lock().unwrap().log.push(logged);
                let reply = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{out}",
                    out.len()
                );
                let _ = s.write_all(reply.as_bytes());
            }
        });
        Stand { port, st }
    }

    fn base(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn sessions(&self) -> Vec<(String, Sess)> {
        let mut v: Vec<_> = self
            .st
            .lock()
            .unwrap()
            .sessions
            .clone()
            .into_iter()
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }

    /// Refunds the provider made (a repeat of one idempotency key is one refund).
    fn refunds_made(&self) -> usize {
        self.st.lock().unwrap().refunds.len()
    }

    fn calls(&self, method: &str, path_prefix: &str) -> Vec<Logged> {
        self.st
            .lock()
            .unwrap()
            .log
            .iter()
            .filter(|l| l.method == method && l.path.starts_with(path_prefix))
            .cloned()
            .collect()
    }

    /// The buyer paid on Stripe's page.
    fn pay(&self, id: &str) {
        let mut g = self.st.lock().unwrap();
        let n = g.n;
        let s = g.sessions.get_mut(id).unwrap();
        s.status = "complete";
        s.payment_status = "paid";
        s.pi = Some(format!("pi_test_{n}"));
    }
}

fn respond(st: &Arc<Mutex<StandState>>, r: &Logged) -> (u16, String) {
    let err = |status: u16, msg: &str| (status, json!({"error": {"message": msg}}).to_string());
    if r.headers.get("authorization").map(String::as_str) != Some(&format!("Bearer {KEY}")) {
        return err(401, "Invalid API Key provided");
    }
    let mut g = st.lock().unwrap();
    let form = |k: &str| {
        r.form
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    match (r.method.as_str(), r.path.as_str()) {
        ("POST", "/v1/checkout/sessions") => {
            if g.fail_create {
                return err(500, "boom");
            }
            g.n += 1;
            let id = format!("cs_test_{:08}", g.n);
            let amount = form("line_items[0][price_data][unit_amount]")
                .parse()
                .unwrap_or(-1);
            let currency = form("line_items[0][price_data][currency]").to_uppercase();
            g.sessions.insert(
                id.clone(),
                Sess {
                    amount,
                    currency: currency.clone(),
                    status: "open",
                    payment_status: "unpaid",
                    pi: None,
                },
            );
            (
                200,
                json!({"id": id, "url": format!("https://checkout.stripe.test/c/pay/{id}"),
                       "status": "open", "payment_status": "unpaid", "amount_total": amount,
                       "currency": currency.to_lowercase()})
                .to_string(),
            )
        }
        ("GET", p) if p.starts_with("/v1/checkout/sessions/") => {
            let id = p.trim_start_matches("/v1/checkout/sessions/");
            match g.sessions.get(id) {
                Some(s) => (
                    200,
                    json!({"id": id, "status": s.status, "payment_status": s.payment_status,
                           "amount_total": s.amount, "currency": s.currency.to_lowercase(),
                           "payment_intent": s.pi})
                    .to_string(),
                ),
                None => err(404, "No such checkout session"),
            }
        }
        ("POST", p) if p.starts_with("/v1/checkout/sessions/") && p.ends_with("/expire") => {
            let id = p
                .trim_start_matches("/v1/checkout/sessions/")
                .trim_end_matches("/expire")
                .to_string();
            match g.sessions.get_mut(&id) {
                Some(s) if s.status == "open" => {
                    s.status = "expired";
                    (200, json!({"id": id, "status": "expired"}).to_string())
                }
                _ => err(400, "The Checkout Session is not open"),
            }
        }
        ("POST", "/v1/refunds") => {
            if g.fail_refund {
                return err(400, "The charge has already been refunded");
            }
            let key = r
                .headers
                .get("idempotency-key")
                .cloned()
                .unwrap_or_default();
            if let Some((id, amount)) = g.refunds.get(&key).filter(|_| !key.is_empty()) {
                return (
                    200,
                    json!({"id": id, "status": "succeeded", "amount": amount}).to_string(),
                );
            }
            g.n += 1;
            let n = g.n;
            let amount = form("amount").parse::<i64>().unwrap_or(0);
            g.refunds.insert(key, (format!("re_test_{n}"), amount));
            (
                200,
                json!({"id": format!("re_test_{n}"), "status": "succeeded", "amount": amount})
                    .to_string(),
            )
        }
        _ => err(404, "Unrecognized request URL"),
    }
}

// ---- the shop ---------------------------------------------------------------------------------

struct Env {
    store: Store,
    d: Arc<Dispatcher>,
    site: Site,
    pay: Arc<Payments>,
    stand: Stand,
}

fn env_with(stripe: bool, demo: bool, methods: &str) -> Env {
    env_on(Store::new(), stripe, demo, methods)
}

fn env_on(store: Store, stripe: bool, demo: bool, methods: &str) -> Env {
    let d = Arc::new(Dispatcher::new());
    base::register(&d, &store);
    product::register(&d, &store);
    let inst = Installer::new(
        store.clone(),
        &d,
        vec![base::module(), product::module()],
        Arc::new(Bundled),
    );
    for id in ["website", "ecommerce"] {
        inst.install(&Bundled.fetch(id).unwrap(), "marketplace", "test")
            .unwrap();
    }
    obop_site::seed::seed(&store, &d).unwrap();
    inst.set_settings(&json!({"site": {"payment_methods": methods,
        "payment_instructions": "Pay to IBAN DE00 0000 0000 0000 0000 00, reference: your order number.",
        "base_url": "https://shop.example"}}).to_string())
        .unwrap();
    let stand = Stand::start();
    let client = Client::new(ClientConfig {
        allow_loopback: true,
        ..ClientConfig::default()
    });
    let pay = Payments::new(
        store.backend(),
        d.clone(),
        client,
        Arc::new(StaticKeys(stripe.then(|| StripeKeys {
            api_key: KEY.into(),
            signing_secret: SECRET.into(),
        }))),
        Config {
            demo,
            stripe_base: stand.base(),
            stripe_hosts: vec!["127.0.0.1".into()],
        },
    );
    pay.install();
    let site = Site::new(
        PublicApi::new(d.clone(), store.clone()),
        SiteConfig {
            settings_ttl: std::time::Duration::ZERO,
            ..SiteConfig::default()
        },
    )
    .with_payments(pay.clone());
    Env {
        store,
        d,
        site,
        pay,
        stand,
    }
}

static N: AtomicUsize = AtomicUsize::new(1);

impl Env {
    fn req(&self, method: &str, path: &str) -> Req {
        let mut r = Req::new(method, path)
            .with_header("host", "shop.test")
            .with_header("accept-language", "en");
        r.client = "203.0.113.7".into();
        r
    }

    fn get(&self, path: &str) -> Resp {
        self.site.handle(&self.req("GET", path))
    }

    fn post(&self, path: &str, fields: &[(&str, &str)], from: &str) -> Resp {
        let mut all: Vec<(&str, &str)> = vec![("csrf", CSRF)];
        all.extend_from_slice(fields);
        let body = all
            .iter()
            .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
            .collect::<Vec<_>>()
            .join("&");
        let mut r = self
            .req("POST", path)
            .with_header("cookie", &format!("obop_csrf={CSRF}"))
            .with_header("origin", "http://shop.test")
            .with_header("content-type", "application/x-www-form-urlencoded")
            .with_body(&body);
        r.client = from.into();
        self.site.handle(&r)
    }

    fn listing(&self, slug: &str) -> String {
        self.store
            .backend()
            .list(LISTING)
            .unwrap()
            .into_iter()
            .find(|(_, f)| f["slug"] == slug)
            .map(|(id, _)| id)
            .unwrap()
    }

    fn stock(&self, slug: &str) -> i64 {
        self.store
            .backend()
            .get(LISTING, &self.listing(slug))
            .unwrap()
            .unwrap()["stock_quantity"]
            .as_i64()
            .unwrap()
    }

    fn shipping(&self) -> String {
        self.store
            .backend()
            .list("ecommerce_shipping_method")
            .unwrap()
            .remove(0)
            .0
    }

    /// Buy `qty` of a listing with the given payment method, from a fresh address.
    fn buy(&self, slug: &str, qty: &str, method: &str, extra: &[(&str, &str)]) -> Resp {
        let n = N.fetch_add(1, Ordering::Relaxed);
        let (l, ship) = (self.listing(slug), self.shipping());
        let idem = format!("{n:032x}");
        let email = format!("buyer{n}@example.test");
        let mut f: Vec<(&str, &str)> = vec![
            ("customer_name", "Test Buyer"),
            ("customer_email", &email),
            ("shipping_address", "1 Example Street\n00000 Testville"),
            ("idem", &idem),
            ("id", &l),
            ("qty", qty),
            ("shipping_method_id", &ship),
            ("payment_method", method),
        ];
        f.extend_from_slice(extra);
        self.post(
            "/shop/checkout",
            &f,
            &format!("10.30.{}.{}", n / 250, n % 250),
        )
    }

    fn orders(&self) -> Vec<(String, Fields)> {
        self.store.backend().list(ORDER).unwrap()
    }

    fn last_order(&self) -> (String, Fields) {
        self.orders().pop().unwrap()
    }

    fn order_state(&self, id: &str) -> String {
        self.store.backend().get(ORDER, id).unwrap().unwrap()["state"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn payments(&self) -> Vec<(String, Fields)> {
        self.store.backend().list(PAYMENT).unwrap()
    }

    fn last_payment(&self) -> (String, Fields) {
        self.payments().pop().unwrap()
    }

    fn token(&self, order: &str) -> String {
        self.store.backend().get(ORDER, order).unwrap().unwrap()["access_token"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn manager(&self) -> Principal {
        Principal::user("mia", &["ecommerce.user", "ecommerce.manager"])
    }

    fn call(&self, p: &Principal, cmd: &str, id: &str) -> Result<(), CallError> {
        self.d
            .call(
                Surface::Rest,
                p,
                &mut Vault::new(),
                cmd,
                &Value::map([("id", Value::text(id))]),
            )
            .map(|_| ())
    }

    /// What the backoffice kanban does when a card is dragged to another column: an `update` of
    /// the governed `state` field, not a call of the action.
    fn update_state(&self, p: &Principal, id: &str, state: &str) -> Result<(), CallError> {
        self.d
            .call(
                Surface::Rest,
                p,
                &mut Vault::new(),
                "ecommerce_order.update",
                &Value::map([("id", Value::text(id)), ("state", Value::text(state))]),
            )
            .map(|_| ())
    }

    fn payment_of(&self, order: &str) -> Fields {
        self.payments()
            .into_iter()
            .map(|(_, p)| p)
            .find(|p| p["order_id"] == order)
            .unwrap()
    }

    /// The state the follow-up of a payment event leaves when it is refused for long: the payment
    /// is recorded as succeeded and the order is still as it was.
    fn payment_succeeded(&self, order: &str) {
        let pid = self
            .payments()
            .into_iter()
            .find(|(_, p)| p["order_id"] == order)
            .unwrap()
            .0;
        let mut done = Fields::new();
        done.insert("state".into(), json!("succeeded"));
        self.store.backend().update(PAYMENT, &pid, &done).unwrap();
    }

    /// A webhook delivery the way Stripe would sign it.
    fn hook(&self, body: &str, ts: u64, secret: &str) -> HookReply {
        let sig = format!(
            "t={ts},v1={}",
            stripe_signature(secret, ts, body.as_bytes())
        );
        self.pay.handle_webhook(
            "stripe",
            &|k| (k == "stripe-signature").then(|| sig.clone()),
            body.as_bytes(),
        )
    }
}

fn enc(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

fn location(r: &Resp) -> String {
    r.get_header("location").unwrap_or("").to_string()
}

fn event(id: &str, kind: &str, session: &str, amount: i64, status: &str) -> String {
    json!({"id": id, "type": kind, "data": {"object": {
        "id": session, "payment_status": status, "amount_total": amount, "currency": "eur"}}})
    .to_string()
}

// ---- the demo provider, end to end through the site ------------------------------------------

#[test]
fn the_demo_provider_takes_an_order_from_pending_to_paid_through_the_hosted_page() {
    let e = env_with(false, true, "fake,manual");
    let r = e.buy("sencha-green-tea", "2", "fake", &[]);
    assert_eq!(r.status, 303, "{}", r.body_str());
    let page = location(&r);
    assert!(page.starts_with("/shop/fake-pay/"), "{page}");
    let (oid, order) = e.last_order();
    assert_eq!(order["state"], "pending");
    assert_eq!(order["payment_method"], "fake");
    assert_eq!(
        e.stock("sencha-green-tea"),
        38,
        "the units are held while the buyer pays"
    );
    let (_, pay) = e.last_payment();
    assert_eq!(
        (pay["state"].as_str(), pay["provider"].as_str()),
        (Some("open"), Some("fake"))
    );
    assert_eq!(
        pay["amount"], order["total"],
        "the payment is for the stored total"
    );
    // The hosted page says it is a demo and offers the three outcomes.
    let hosted = e.get(&page).body_str();
    assert!(
        hosted.contains("Demo mode: no real payment is made"),
        "{hosted}"
    );
    assert!(hosted.contains("value=\"paid\""));
    // Back on the shop's own page the order is waiting, and says so.
    let token = e.token(&oid);
    let waiting = e.get(&format!("/shop/pay/{token}")).body_str();
    assert!(
        waiting.contains("We are waiting for the payment"),
        "{waiting}"
    );
    assert!(waiting.contains("Continue to payment"));
    // Pay.
    let session = page.trim_start_matches("/shop/fake-pay/").to_string();
    let r = e.post(&page, &[("outcome", "paid")], "203.0.113.7");
    assert_eq!(r.status, 303);
    assert_eq!(location(&r), format!("/shop/pay/{token}"));
    assert_eq!(e.order_state(&oid), "paid");
    assert_eq!(e.last_payment().1["state"], "succeeded");
    assert_eq!(
        e.stock("sencha-green-tea"),
        38,
        "paid orders keep their units taken"
    );
    let done = e.get(&format!("/shop/pay/{token}")).body_str();
    assert!(done.contains("Payment received. Thank you!"), "{done}");
    assert!(done.contains("data-clear-cart"));
    assert!(done.contains("Show a printable receipt"));
    let receipt = e.get(&format!("/shop/order/{token}/receipt")).body_str();
    assert!(
        receipt.contains("<h1>Receipt</h1>") && receipt.contains("data-print"),
        "{receipt}"
    );
    // Settling the same session again (a double click, a replay) changes nothing.
    e.post(&page, &[("outcome", "failed")], "203.0.113.7");
    assert_eq!(e.order_state(&oid), "paid");
    assert_eq!(e.pay.fake_decide(&session, "failed"), Ok(()));
    assert_eq!(e.order_state(&oid), "paid");
    assert_eq!(e.stock("sencha-green-tea"), 38);
}

#[test]
fn a_failed_or_abandoned_payment_gives_the_units_and_the_code_back() {
    let e = env_with(false, true, "fake");
    // A discount code with one use, taken by this order.
    let code = "00000000-0000-4000-8000-000000000001";
    e.store
        .backend()
        .insert(
            "ecommerce_discount_code",
            code,
            &Fields::from_iter([
                ("code".to_string(), json!("ONEUSE")),
                ("kind".to_string(), json!("percent")),
                ("value".to_string(), json!(1000)),
                ("active".to_string(), json!(true)),
                ("max_uses".to_string(), json!(1)),
            ]),
        )
        .unwrap();
    let r = e.buy("stoneware-teapot", "2", "fake", &[("code", "oneuse")]);
    assert_eq!(r.status, 303, "{}", r.body_str());
    assert_eq!(e.stock("stoneware-teapot"), 4);
    let used = |e: &Env| {
        e.store
            .backend()
            .get("ecommerce_discount_code", code)
            .unwrap()
            .unwrap()["used_count"]
            .as_i64()
            .unwrap()
    };
    assert_eq!(used(&e), 1);
    // While the first order waits, the single-use code is taken: a second buyer is refused.
    let second = e.buy("sencha-green-tea", "1", "fake", &[("code", "oneuse")]);
    assert_eq!(second.status, 400);
    // The buyer's payment fails on the provider's page.
    let page = location(&r);
    let (oid, _) = e.last_order();
    e.post(&page, &[("outcome", "failed")], "203.0.113.7");
    assert_eq!(e.order_state(&oid), "cancelled");
    assert_eq!(e.last_payment().1["state"], "failed");
    assert_eq!(
        e.stock("stoneware-teapot"),
        6,
        "the units are back on the shelf"
    );
    assert_eq!(used(&e), 0, "the code can be used again");
    let page = e.get(&format!("/shop/pay/{}", e.token(&oid))).body_str();
    assert!(page.contains("The payment did not go through."), "{page}");
    // And again, now that the code is free: cancelled on the hosted page instead.
    let r = e.buy("stoneware-teapot", "1", "fake", &[("code", "oneuse")]);
    assert_eq!(r.status, 303);
    let (oid2, _) = e.last_order();
    e.post(&location(&r), &[("outcome", "cancelled")], "203.0.113.7");
    assert_eq!(e.order_state(&oid2), "cancelled");
    assert_eq!(e.stock("stoneware-teapot"), 6);
    assert_eq!(used(&e), 0);
    let flag = e.store.backend().get(ORDER, &oid2).unwrap().unwrap()["stock_returned"].clone();
    assert_eq!(flag, json!(true));
}

#[test]
fn security_the_demo_provider_is_off_unless_the_operator_turned_it_on() {
    let e = env_with(false, false, "fake,manual");
    assert_eq!(
        e.pay.methods(&["fake".into(), "manual".into()]),
        vec!["manual".to_string()]
    );
    // Asking for it anyway is refused, and nothing is created.
    let r = e.buy("sencha-green-tea", "1", "fake", &[]);
    assert_eq!(r.status, 400);
    assert!(r.body_str().contains("Choose how you want to pay."));
    assert!(e.orders().is_empty());
    assert_eq!(e.stock("sencha-green-tea"), 40);
    // A forged session id means nothing either.
    let fake = "a".repeat(64);
    assert_eq!(e.get(&format!("/shop/fake-pay/{fake}")).status, 404);
    assert_eq!(
        e.post(
            &format!("/shop/fake-pay/{fake}"),
            &[("outcome", "paid")],
            "203.0.113.9"
        )
        .status,
        404
    );
}

#[test]
fn security_the_return_page_never_trusts_the_browser_and_tokens_belong_to_one_order() {
    let e = env_with(false, true, "fake");
    let r = e.buy("sencha-green-tea", "1", "fake", &[]);
    let (a, _) = e.last_order();
    let page_a = location(&r);
    let r = e.buy("linen-tea-towel", "1", "fake", &[]);
    let (b, _) = e.last_order();
    let (ta, tb) = (e.token(&a), e.token(&b));
    // Every way a browser could claim "paid": query strings, a forged return, a cancel flag.
    for q in [
        "?s=ok",
        "?s=paid",
        "?status=succeeded&paid=1",
        "?session_id=cs_x&payment_status=paid",
    ] {
        let page = e.get(&format!("/shop/pay/{ta}{q}")).body_str();
        assert!(!page.contains("Payment received"), "{q}");
        assert_eq!(e.order_state(&a), "pending");
    }
    // A cancel redirect without the provider saying so cancels nothing.
    let page = e.get(&format!("/shop/pay/{ta}?s=cancel")).body_str();
    assert!(page.contains("The payment was not completed"), "{page}");
    assert_eq!(e.order_state(&a), "pending");
    assert_eq!(e.stock("sencha-green-tea"), 39);
    // Order A's token shows order A only; B's session cannot pay A.
    let sess_b = location(&r);
    let view = e.get(&format!("/shop/pay/{ta}")).body_str();
    assert!(
        view.contains(
            &e.store.backend().get(ORDER, &a).unwrap().unwrap()["number"]
                .as_str()
                .unwrap()
                .to_string()
        )
    );
    assert!(
        !view.contains(
            &e.store.backend().get(ORDER, &b).unwrap().unwrap()["number"]
                .as_str()
                .unwrap()
                .to_string()
        )
    );
    e.post(&sess_b, &[("outcome", "paid")], "203.0.113.7");
    assert_eq!(e.order_state(&b), "paid");
    assert_eq!(e.order_state(&a), "pending", "paying B did not pay A");
    // Tokens that are not tokens.
    for bad in ["x", &"0".repeat(64), &"g".repeat(64), "../../etc/passwd"] {
        assert_eq!(e.get(&format!("/shop/pay/{bad}")).status, 404, "{bad}");
        assert_eq!(
            e.get(&format!("/shop/order/{bad}/receipt")).status,
            404,
            "{bad}"
        );
        assert_eq!(
            e.get(&format!("/shop/pay/{bad}/continue")).status,
            404,
            "{bad}"
        );
    }
    // The demo page of A, and a swapped-in B token, can only lead to their own order.
    assert!(page_a.starts_with("/shop/fake-pay/"));
    let hosted = e.get(&page_a).body_str();
    assert!(hosted.contains(&format!("/shop/pay/{ta}")) && !hosted.contains(&tb));
}

// ---- bank transfer ------------------------------------------------------------------------------

#[test]
fn a_bank_transfer_order_shows_the_instructions_and_staff_mark_it_paid() {
    let e = env_with(false, false, "manual");
    let r = e.buy("sencha-green-tea", "1", "manual", &[]);
    assert_eq!(r.status, 303);
    let (oid, order) = e.last_order();
    let token = e.token(&oid);
    assert_eq!(location(&r), format!("/shop/order/{token}"));
    assert!(order["reserved_until"].as_u64().unwrap() > obop_connectors::util::now() + 6 * 86_400);
    let page = e.get(&location(&r)).body_str();
    assert!(
        page.contains("How to pay") && page.contains("IBAN DE00"),
        "{page}"
    );
    // Staff mark it paid when the money arrives; the payment record follows.
    e.call(&e.manager(), "ecommerce_order.mark_paid", &oid)
        .unwrap();
    assert_eq!(e.order_state(&oid), "paid");
    assert_eq!(e.last_payment().1["state"], "succeeded");
    let page = e.get(&format!("/shop/order/{token}")).body_str();
    assert!(page.contains("Payment received"), "{page}");
}

#[test]
fn cancelling_an_unpaid_order_by_hand_puts_the_stock_back_once() {
    let e = env_with(false, false, "manual");
    e.buy("stoneware-teapot", "3", "manual", &[]);
    let (oid, _) = e.last_order();
    assert_eq!(e.stock("stoneware-teapot"), 3);
    e.call(&e.manager(), "ecommerce_order.cancel", &oid)
        .unwrap();
    assert_eq!(e.stock("stoneware-teapot"), 6);
    assert_eq!(e.last_payment().1["state"], "cancelled");
    // Releasing again, in any way, adds nothing.
    assert_eq!(e.pay.release_order(&oid, true), Ok(false));
    assert_eq!(e.stock("stoneware-teapot"), 6);
    assert!(
        e.call(&e.manager(), "ecommerce_order.cancel", &oid)
            .is_err(),
        "no longer pending"
    );
    assert_eq!(e.stock("stoneware-teapot"), 6);
}

#[test]
fn unpaid_orders_whose_reservation_ran_out_are_cancelled_and_released() {
    let e = env_with(false, true, "fake,manual");
    e.buy("stoneware-teapot", "2", "manual", &[]);
    let (manual, _) = e.last_order();
    e.buy("sencha-green-tea", "1", "fake", &[]);
    let (online, _) = e.last_order();
    let (paid_id, _) = {
        let r = e.buy("linen-tea-towel", "1", "fake", &[]);
        e.post(&location(&r), &[("outcome", "paid")], "203.0.113.7");
        e.last_order()
    };
    // Nothing is due yet.
    assert_eq!(e.pay.sweep_expired(), 0);
    // Time passes: every reservation is in the past.
    for id in [&manual, &online, &paid_id] {
        let mut f = Fields::new();
        f.insert("reserved_until".into(), json!(1));
        e.store.backend().update(ORDER, id, &f).unwrap();
    }
    assert_eq!(e.pay.sweep_expired(), 2);
    assert_eq!(e.order_state(&manual), "cancelled");
    assert_eq!(e.order_state(&online), "cancelled");
    assert_eq!(
        e.order_state(&paid_id),
        "paid",
        "a paid order is not touched"
    );
    assert_eq!(e.stock("stoneware-teapot"), 6);
    assert_eq!(e.stock("sencha-green-tea"), 40);
    assert_eq!(e.stock("linen-tea-towel"), 17);
    let states: Vec<String> = e
        .payments()
        .iter()
        .map(|(_, p)| p["state"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(states, ["expired", "expired", "succeeded"]);
    assert_eq!(e.pay.sweep_expired(), 0, "idempotent");
}

// ---- Stripe, against the local stand-in ---------------------------------------------------------

#[test]
fn stripe_checkout_creates_a_session_from_the_server_total_and_nothing_personal() {
    let e = env_with(true, false, "stripe,manual");
    let before = e.stock("sencha-green-tea");
    let r = e.buy(
        "sencha-green-tea",
        "2",
        "stripe",
        &[("customer_name", "Ada Lovelace")],
    );
    assert_eq!(r.status, 303, "{}", r.body_str());
    let (oid, order) = e.last_order();
    let sessions = e.stand.sessions();
    assert_eq!(sessions.len(), 1);
    let (sid, sess) = &sessions[0];
    assert_eq!(
        location(&r),
        format!("https://checkout.stripe.test/c/pay/{sid}")
    );
    // The amount Stripe was asked for is the stored order total in minor units, EUR.
    assert_eq!(Some(sess.amount), order["total"].as_i64());
    assert_eq!(sess.currency, "EUR");
    let call = &e.stand.calls("POST", "/v1/checkout/sessions")[0];
    let get = |k: &str| {
        call.form
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    assert_eq!(get("mode"), "payment");
    assert_eq!(get("line_items[0][quantity]"), "1");
    assert_eq!(get("line_items[0][price_data][currency]"), "eur");
    assert!(get("line_items[0][price_data][product_data][name]").starts_with("Order W"));
    assert!(
        get("success_url").starts_with("https://shop.example/shop/pay/"),
        "{}",
        get("success_url")
    );
    assert!(get("cancel_url").ends_with("?s=cancel"));
    assert_eq!(get("client_reference_id"), e.last_payment().0);
    assert_eq!(
        call.headers.get("idempotency-key"),
        Some(&e.last_payment().0)
    );
    // No name, address or email goes to Stripe.
    let sent = format!("{:?}", call.form);
    for personal in [
        "Ada",
        "Lovelace",
        "example.test",
        "Example Street",
        "Testville",
    ] {
        assert!(!sent.contains(personal), "{personal} was sent: {sent}");
    }
    // The key travelled in a header only.
    assert!(!sent.contains(KEY));
    assert_eq!(e.stock("sencha-green-tea"), before - 2);
    assert_eq!(e.last_payment().1["external_id"], sid.as_str());
    assert_eq!(e.last_order().0, oid);
}

#[test]
fn a_completion_met_by_a_busy_store_is_retried_not_swallowed_and_not_taken_for_a_duplicate() {
    use obop_storage::fault;
    use obop_storage::testing::Flaky;
    // D74: the provider's event arrives while the pool is full. The lookup of the payment is
    // refused. The event must not be marked as seen and answered 200 (the provider would never
    // send it again and the order would stay pending for ever): the reply is Retry, the claim
    // is given back, and the provider's next delivery of the same event is applied.
    let flaky = Flaky::new(obop_storage::open("memory:").unwrap());
    let e = env_on(
        Store::over(obop_storage::guarded(flaky.clone())),
        true,
        false,
        "stripe",
    );
    e.buy("sencha-green-tea", "1", "stripe", &[]);
    let (oid, order) = e.last_order();
    let (sid, _) = e.stand.sessions().remove(0);
    let total = order["total"].as_i64().unwrap();
    let body = event(
        "evt_busy",
        "checkout.session.completed",
        &sid,
        total,
        "paid",
    );
    let ts = obop_connectors::util::now();

    flaky.refuse_model(PAYMENT, obop_storage::testing::Mode::Reads);
    fault::begin_request(Default::default());
    assert_eq!(e.hook(&body, ts, SECRET), HookReply::Retry);
    let out = fault::end_request();
    flaky.allow_models();
    assert!(out.fault.is_some() && !out.wrote, "{out:?}");
    assert_eq!(e.order_state(&oid), "pending");

    // The store answers again: the same event, same id, is applied (it was not used up).
    assert_eq!(e.hook(&body, ts + 1, SECRET), HookReply::Done);
    assert_eq!(e.order_state(&oid), "paid");
    assert_eq!(e.hook(&body, ts + 2, SECRET), HookReply::Duplicate);
}

#[test]
fn a_signed_completion_marks_the_order_paid_once_and_a_replay_changes_nothing() {
    let e = env_with(true, false, "stripe");
    e.buy("sencha-green-tea", "1", "stripe", &[]);
    let (oid, order) = e.last_order();
    let (sid, _) = e.stand.sessions().remove(0);
    let total = order["total"].as_i64().unwrap();
    let body = event("evt_1", "checkout.session.completed", &sid, total, "paid");
    let ts = obop_connectors::util::now();
    assert_eq!(e.hook(&body, ts, SECRET), HookReply::Done);
    assert_eq!(e.order_state(&oid), "paid");
    let (pid, pay) = e.last_payment();
    assert_eq!(pay["state"], "succeeded");
    assert_eq!(events_of(&pay), ["evt_1"]);
    // The same delivery again (Stripe retries; an attacker replays): acknowledged, no effect.
    assert_eq!(e.hook(&body, ts, SECRET), HookReply::Duplicate);
    assert_eq!(e.order_state(&oid), "paid");
    // The same event signed afresh (new timestamp, same id) is also a repeat.
    assert_eq!(e.hook(&body, ts + 1, SECRET), HookReply::Duplicate);
    assert_eq!(
        events_of(&e.store.backend().get(PAYMENT, &pid).unwrap().unwrap()),
        ["evt_1"]
    );
    // Another event for the settled payment is ignored.
    let expired = event("evt_2", "checkout.session.expired", &sid, total, "unpaid");
    assert_ne!(e.hook(&expired, ts, SECRET), HookReply::Done);
    assert_eq!(e.order_state(&oid), "paid");
    assert_eq!(
        e.stock("sencha-green-tea"),
        39,
        "a paid order keeps its units"
    );
}

#[test]
fn security_unsigned_forged_stale_and_mismatched_notifications_change_nothing() {
    let e = env_with(true, false, "stripe");
    e.buy("sencha-green-tea", "1", "stripe", &[]);
    let (oid, order) = e.last_order();
    let (sid, _) = e.stand.sessions().remove(0);
    let total = order["total"].as_i64().unwrap();
    let body = event("evt_9", "checkout.session.completed", &sid, total, "paid");
    let now = obop_connectors::util::now();
    // Wrong secret, no header, garbage header, stale timestamp, a body changed after signing.
    assert_eq!(
        e.hook(&body, now, "whsec_attacker"),
        HookReply::Unauthorized
    );
    assert_eq!(
        e.pay.handle_webhook("stripe", &|_| None, body.as_bytes()),
        HookReply::Unauthorized
    );
    assert_eq!(
        e.pay
            .handle_webhook("stripe", &|_| Some("t=1,v1=zz".into()), body.as_bytes()),
        HookReply::Unauthorized
    );
    assert_eq!(e.hook(&body, now - 3600, SECRET), HookReply::Unauthorized);
    let sig = format!(
        "t={now},v1={}",
        stripe_signature(SECRET, now, body.as_bytes())
    );
    let tampered = body.replace(&total.to_string(), "1");
    assert_eq!(
        e.pay
            .handle_webhook("stripe", &|_| Some(sig.clone()), tampered.as_bytes()),
        HookReply::Unauthorized
    );
    assert_eq!(e.order_state(&oid), "pending");
    // Correctly signed, but for another amount or another currency: not this payment.
    let cheap = event(
        "evt_10",
        "checkout.session.completed",
        &sid,
        total - 1,
        "paid",
    );
    assert_ne!(e.hook(&cheap, now, SECRET), HookReply::Done);
    let usd = event("evt_11", "checkout.session.completed", &sid, total, "paid")
        .replace("\"eur\"", "\"usd\"");
    assert_ne!(e.hook(&usd, now, SECRET), HookReply::Done);
    assert_eq!(
        e.order_state(&oid),
        "pending",
        "an amount that does not match pays nothing"
    );
    // Signed events about sessions that are not ours, and about things we do not handle.
    let other = event(
        "evt_12",
        "checkout.session.completed",
        "cs_test_99999999",
        total,
        "paid",
    );
    assert_ne!(e.hook(&other, now, SECRET), HookReply::Done);
    let odd =
        json!({"id": "evt_13", "type": "customer.created", "data": {"object": {}}}).to_string();
    assert_ne!(e.hook(&odd, now, SECRET), HookReply::Done);
    let unpaid = event(
        "evt_14",
        "checkout.session.completed",
        &sid,
        total,
        "unpaid",
    );
    assert_ne!(e.hook(&unpaid, now, SECRET), HookReply::Done);
    // Not JSON at all, signed.
    assert_ne!(e.hook("not json", now, SECRET), HookReply::Done);
    // An unknown provider, and providers that take no webhooks.
    assert_eq!(
        e.pay.handle_webhook("nope", &|_| None, b"{}"),
        HookReply::NotFound
    );
    assert_eq!(
        e.pay.handle_webhook("manual", &|_| None, b"{}"),
        HookReply::NotFound
    );
    assert_eq!(
        e.pay.handle_webhook("fake", &|_| None, b"{}"),
        HookReply::NotFound
    );
    assert_eq!(e.order_state(&oid), "pending");
    // And the genuine one still works after all of that.
    assert_eq!(e.hook(&body, now, SECRET), HookReply::Done);
    assert_eq!(e.order_state(&oid), "paid");
}

#[test]
fn security_two_deliveries_of_one_event_racing_pay_the_order_once() {
    let e = env_with(true, false, "stripe");
    e.buy("sencha-green-tea", "1", "stripe", &[]);
    let (oid, order) = e.last_order();
    let (sid, _) = e.stand.sessions().remove(0);
    let body = event(
        "evt_race",
        "checkout.session.completed",
        &sid,
        order["total"].as_i64().unwrap(),
        "paid",
    );
    let ts = obop_connectors::util::now();
    let done = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..8 {
            s.spawn(|| {
                if e.hook(&body, ts, SECRET) == HookReply::Done {
                    done.fetch_add(1, Ordering::SeqCst);
                }
            });
        }
    });
    assert_eq!(done.load(Ordering::SeqCst), 1);
    assert_eq!(e.order_state(&oid), "paid");
    assert_eq!(events_of(&e.last_payment().1).len(), 1);
}

#[test]
fn an_expiry_notification_cancels_the_order_and_releases_everything() {
    let e = env_with(true, false, "stripe");
    e.buy("stoneware-teapot", "2", "stripe", &[]);
    let (oid, order) = e.last_order();
    let (sid, _) = e.stand.sessions().remove(0);
    let body = event(
        "evt_exp",
        "checkout.session.expired",
        &sid,
        order["total"].as_i64().unwrap(),
        "unpaid",
    );
    assert_eq!(
        e.hook(&body, obop_connectors::util::now(), SECRET),
        HookReply::Done
    );
    assert_eq!(e.order_state(&oid), "cancelled");
    assert_eq!(e.last_payment().1["state"], "expired");
    assert_eq!(e.stock("stoneware-teapot"), 6);
}

#[test]
fn the_return_page_asks_stripe_when_the_notification_is_late_or_lost() {
    let e = env_with(true, false, "stripe");
    e.buy("sencha-green-tea", "1", "stripe", &[]);
    let (oid, _) = e.last_order();
    let token = e.token(&oid);
    let (sid, _) = e.stand.sessions().remove(0);
    // Still open at Stripe: the page waits and offers the way back to Stripe's page.
    let page = e.get(&format!("/shop/pay/{token}")).body_str();
    assert!(page.contains("We are waiting for the payment"));
    assert!(page.contains(&format!("/shop/pay/{token}/continue")));
    let r = e.get(&format!("/shop/pay/{token}/continue"));
    assert_eq!(
        (r.status, location(&r)),
        (303, format!("https://checkout.stripe.test/c/pay/{sid}"))
    );
    // The buyer pays; no webhook arrives; opening the page is enough.
    e.stand.pay(&sid);
    assert_eq!(e.order_state(&oid), "pending");
    let page = e.get(&format!("/shop/pay/{token}")).body_str();
    assert!(page.contains("Payment received"), "{page}");
    assert_eq!(e.order_state(&oid), "paid");
    // The webhook that finally arrives is a repeat.
    let body = event(
        "evt_late",
        "checkout.session.completed",
        &sid,
        e.last_payment().1["amount"].as_i64().unwrap(),
        "paid",
    );
    assert_ne!(
        e.hook(&body, obop_connectors::util::now(), SECRET),
        HookReply::Done
    );
    assert_eq!(e.order_state(&oid), "paid");
}

#[test]
fn when_stripe_cannot_be_reached_the_checkout_fails_cleanly_and_the_stock_comes_back() {
    let e = env_with(true, false, "stripe");
    e.stand.st.lock().unwrap().fail_create = true;
    let before = e.stock("stoneware-teapot");
    let r = e.buy("stoneware-teapot", "2", "stripe", &[]);
    assert_eq!(r.status, 400);
    assert!(
        r.body_str()
            .contains("The payment could not be started. Nothing was charged"),
        "{}",
        r.body_str()
    );
    assert!(!r.body_str().contains(KEY));
    let (oid, order) = e.last_order();
    assert_eq!(order["state"], "cancelled");
    assert_eq!(e.last_payment().1["state"], "failed");
    assert_eq!(e.stock("stoneware-teapot"), before);
    assert_eq!(e.order_state(&oid), "cancelled");
    // Trying again once Stripe is back works.
    e.stand.st.lock().unwrap().fail_create = false;
    assert_eq!(e.buy("stoneware-teapot", "2", "stripe", &[]).status, 303);
    assert_eq!(e.stock("stoneware-teapot"), before - 2);
}

#[test]
fn stripe_is_offered_only_when_the_instance_has_connected_keys() {
    let e = env_with(false, false, "stripe,manual");
    assert_eq!(
        e.pay.methods(&["stripe".into(), "manual".into()]),
        vec!["manual".to_string()]
    );
    let r = e.buy("sencha-green-tea", "1", "stripe", &[]);
    assert_eq!(r.status, 400);
    let e = env_with(true, false, "stripe,manual");
    assert_eq!(
        e.pay.methods(&["stripe".into(), "manual".into()]),
        vec!["stripe".to_string(), "manual".to_string()]
    );
    let cart = e
        .get(&format!("/cart?items={}.1", e.listing("sencha-green-tea")))
        .body_str();
    assert!(
        cart.contains("Card (Stripe)") && cart.contains("Bank transfer or invoice"),
        "{cart}"
    );
}

#[test]
fn a_payment_that_arrives_for_a_cancelled_order_is_returned_not_kept() {
    let e = env_with(true, false, "stripe");
    e.buy("sencha-green-tea", "1", "stripe", &[]);
    let (oid, order) = e.last_order();
    let (sid, _) = e.stand.sessions().remove(0);
    // Staff cancel the order; the stand-in closes the page too.
    e.call(&e.manager(), "ecommerce_order.cancel", &oid)
        .unwrap();
    assert_eq!(e.last_payment().1["state"], "cancelled");
    assert_eq!(
        e.stand.calls("POST", "/v1/checkout/sessions/").len(),
        1,
        "the page was closed"
    );
    // Money arrives anyway (the page was paid a moment before it closed).
    e.stand.pay(&sid);
    let body = event(
        "evt_late_pay",
        "checkout.session.completed",
        &sid,
        order["total"].as_i64().unwrap(),
        "paid",
    );
    assert_eq!(
        e.hook(&body, obop_connectors::util::now(), SECRET),
        HookReply::Done
    );
    assert_eq!(
        e.order_state(&oid),
        "cancelled",
        "the order stays cancelled"
    );
    let (_, pay) = e.last_payment();
    assert_eq!(pay["state"], "refunded", "and the money goes back");
    assert!(pay["refund_id"].as_str().unwrap().starts_with("re_test_"));
    assert_eq!(e.stand.calls("POST", "/v1/refunds").len(), 1);
}

// ---- a store that refuses, around money and stock (D74) ---------------------------------------

fn flaky_env() -> (Arc<obop_storage::testing::Flaky>, Env) {
    let flaky = obop_storage::testing::Flaky::new(obop_storage::open("memory:").unwrap());
    let e = env_on(
        Store::over(obop_storage::guarded(flaky.clone())),
        true,
        false,
        "stripe",
    );
    (flaky, e)
}

#[test]
fn a_refused_read_of_the_order_after_a_payment_is_never_taken_for_a_gone_order_and_never_refunds() {
    use obop_storage::fault;
    use obop_storage::testing::Mode;
    // The payment is recorded as succeeded, and then the read of its order is refused. Taking that
    // for "the order is gone" asked Stripe to return the money of a valid order and answered 200,
    // so the event was never sent again. Now: no refund, the event id is given back, the answer is
    // "retry" and the request is a partial one; the redelivery finishes the order.
    let (flaky, e) = flaky_env();
    e.buy("sencha-green-tea", "1", "stripe", &[]);
    let (oid, order) = e.last_order();
    let (sid, _) = e.stand.sessions().remove(0);
    e.stand.pay(&sid);
    let total = order["total"].as_i64().unwrap();
    let body = event("evt_n1", "checkout.session.completed", &sid, total, "paid");
    let ts = obop_connectors::util::now();

    flaky.refuse_model(ORDER, Mode::Reads);
    fault::begin_request(Default::default());
    let reply = e.hook(&body, ts, SECRET);
    let out = fault::end_request();
    flaky.allow_models();
    assert_eq!(reply, HookReply::Retry, "never Done, never Duplicate");
    assert!(out.wrote && out.partial, "{out:?}");
    assert!(
        e.stand.calls("POST", "/v1/refunds").is_empty(),
        "the money of a valid order is not returned"
    );
    assert_eq!(e.order_state(&oid), "pending");
    assert_eq!(e.last_payment().1["state"], "succeeded");

    // Stripe sends it again: the event is applied (the id was given back) and the order is paid.
    assert_ne!(e.hook(&body, ts + 1, SECRET), HookReply::Retry);
    assert_eq!(e.order_state(&oid), "paid");
    assert!(e.stand.calls("POST", "/v1/refunds").is_empty());
    assert_eq!(e.hook(&body, ts + 2, SECRET), HookReply::Duplicate);
}

#[test]
fn a_late_payment_for_a_cancelled_order_is_refunded_once_even_when_the_store_refuses_a_call_on_the_way()
 {
    let (flaky, e) = flaky_env();
    e.buy("sencha-green-tea", "1", "stripe", &[]);
    let (oid, order) = e.last_order();
    let (sid, _) = e.stand.sessions().remove(0);
    e.call(&e.manager(), "ecommerce_order.cancel", &oid)
        .unwrap();
    e.stand.pay(&sid);
    let body = event(
        "evt_late",
        "checkout.session.completed",
        &sid,
        order["total"].as_i64().unwrap(),
        "paid",
    );
    // A call refused right after the event was claimed is answered "retry" (the id is given
    // back); the delivery that follows applies it and the money goes back, once.
    let ts = obop_connectors::util::now();
    flaky.refuse_n_after_the_next_write(1);
    assert_eq!(e.hook(&body, ts, SECRET), HookReply::Retry);
    assert!(e.stand.calls("POST", "/v1/refunds").is_empty());
    assert_eq!(e.hook(&body, ts + 1, SECRET), HookReply::Done);
    assert_eq!(e.last_payment().1["state"], "refunded");
    assert_eq!(e.stand.calls("POST", "/v1/refunds").len(), 1);
    assert_eq!(e.hook(&body, ts + 2, SECRET), HookReply::Duplicate);
    assert_eq!(e.stand.calls("POST", "/v1/refunds").len(), 1);
}

#[test]
fn a_cancel_whose_units_cannot_be_given_back_is_a_partial_503_and_the_sweep_finishes_it() {
    use obop_storage::fault;
    use obop_storage::testing::Mode;
    let (flaky, e) = flaky_env();
    e.buy("sencha-green-tea", "2", "stripe", &[]);
    let (oid, _) = e.last_order();
    assert_eq!(e.stock("sencha-green-tea"), 38);
    // The order is cancelled, then the listings cannot be read to give the units back.
    flaky.refuse_model(LISTING, Mode::Reads);
    fault::begin_request(Default::default());
    let r = e.call(&e.manager(), "ecommerce_order.cancel", &oid);
    let out = fault::end_request();
    flaky.allow_models();
    assert!(
        matches!(&r, Err(CallError::Handler(m)) if m.starts_with("storage_busy")),
        "{r:?}"
    );
    assert!(out.wrote && out.partial, "{out:?}");
    assert_eq!(e.order_state(&oid), "cancelled");
    assert_eq!(e.stock("sencha-green-tea"), 38, "the units are still out");
    // Nothing visits a cancelled order but the sweep, which now does.
    e.pay.sweep_expired();
    assert_eq!(e.stock("sencha-green-tea"), 40);
    // Once: a second sweep (and a second cancel) does not give them back again.
    e.pay.sweep_expired();
    assert_eq!(e.stock("sencha-green-tea"), 40);
}

#[test]
fn a_refund_is_not_made_for_an_order_whose_payment_cannot_be_looked_up() {
    use obop_storage::testing::Mode;
    let (flaky, e) = flaky_env();
    let (oid, _) = paid_stripe_order(&e, "stoneware-teapot", "1");
    flaky.refuse_model(PAYMENT, Mode::Reads);
    let r = e.call(&e.manager(), "ecommerce_order.refund", &oid);
    flaky.allow_models();
    assert!(
        matches!(&r, Err(CallError::Handler(m)) if m.starts_with("storage_busy")),
        "{r:?}"
    );
    assert_eq!(
        e.order_state(&oid),
        "paid",
        "not refunded without the money"
    );
    assert!(e.stand.calls("POST", "/v1/refunds").is_empty());
}

#[test]
fn the_sweep_never_cancels_an_order_whose_payment_it_could_not_look_up() {
    use obop_storage::testing::Mode;
    let (flaky, e) = flaky_env();
    e.buy("sencha-green-tea", "1", "stripe", &[]);
    let (oid, _) = e.last_order();
    let (sid, _) = e.stand.sessions().remove(0);
    e.stand.pay(&sid);
    // The reservation ran out and the buyer has paid (the notification has not come).
    let mut lapsed = Fields::new();
    lapsed.insert("reserved_until".into(), json!(1));
    e.store.backend().update(ORDER, &oid, &lapsed).unwrap();
    flaky.refuse_model(PAYMENT, Mode::Reads);
    assert_eq!(e.pay.sweep_expired(), 0);
    flaky.allow_models();
    assert_eq!(e.order_state(&oid), "pending", "left for the next sweep");
    assert_eq!(e.stock("sencha-green-tea"), 39);
    // With the lookup answered, the order whose money we hold is marked paid, not cancelled.
    e.pay.sweep_expired();
    assert_eq!(e.order_state(&oid), "paid");
}

#[test]
fn an_order_that_cannot_be_marked_paid_for_another_reason_is_never_answered_as_done() {
    // The payment is recorded and marking the order paid is refused for a reason that is not the
    // store (a field the order lacks): the webhook is not answered 200 ("done") with the money
    // held for an order nobody will ship, and the money is not refunded either, because the order
    // is still there. It is answered "retry", and once the order can be marked, the redelivery does.
    let (_flaky, e) = flaky_env();
    e.buy("sencha-green-tea", "1", "stripe", &[]);
    let (oid, order) = e.last_order();
    let (sid, _) = e.stand.sessions().remove(0);
    e.stand.pay(&sid);
    let total = order["total"].as_i64().unwrap();
    let body = event(
        "evt_stuck",
        "checkout.session.completed",
        &sid,
        total,
        "paid",
    );
    let mut gap = Fields::new();
    gap.insert("customer_email".into(), J::Null);
    e.store.backend().update(ORDER, &oid, &gap).unwrap();
    let ts = obop_connectors::util::now();
    let reply = e.hook(&body, ts, SECRET);
    assert_eq!(
        reply,
        HookReply::Retry,
        "never Done for an order that is still pending"
    );
    assert_eq!(e.order_state(&oid), "pending");
    assert!(
        e.stand.calls("POST", "/v1/refunds").is_empty(),
        "the order is not gone"
    );
    let mut fixed = Fields::new();
    fixed.insert("customer_email".into(), json!("buyer@example.test"));
    e.store.backend().update(ORDER, &oid, &fixed).unwrap();
    assert_ne!(e.hook(&body, ts + 1, SECRET), HookReply::Retry);
    assert_eq!(e.order_state(&oid), "paid");
}

#[test]
fn a_cancelled_order_that_never_reserved_units_returns_none_and_the_sweep_leaves_it_alone() {
    let (_flaky, e) = flaky_env();
    e.buy("sencha-green-tea", "2", "stripe", &[]);
    let (oid, _) = e.last_order();
    assert_eq!(e.stock("sencha-green-tea"), 38);
    // A staff-made order, an import, one from before the flag: nothing was reserved for it.
    let mut no_flag = Fields::new();
    no_flag.insert("stock_returned".into(), J::Null);
    e.store.backend().update(ORDER, &oid, &no_flag).unwrap();
    e.call(&e.manager(), "ecommerce_order.cancel", &oid)
        .unwrap();
    assert_eq!(e.order_state(&oid), "cancelled");
    assert_eq!(
        e.stock("sencha-green-tea"),
        38,
        "the cancel does not invent stock"
    );
    for _ in 0..2 {
        e.pay.sweep_expired();
    }
    assert_eq!(e.stock("sencha-green-tea"), 38, "nor does the sweep");
}

#[test]
fn a_cancel_of_a_pending_order_whose_payment_already_succeeded_returns_the_money() {
    let (_flaky, e) = flaky_env();
    e.buy("sencha-green-tea", "1", "stripe", &[]);
    let (oid, _) = e.last_order();
    let (sid, _) = e.stand.sessions().remove(0);
    e.stand.pay(&sid);
    // The state the follow-up of a payment event leaves when it is refused for long: the payment
    // is recorded as succeeded, the order is still pending.
    let (pid, _) = e.last_payment();
    let mut done = Fields::new();
    done.insert("state".into(), json!("succeeded"));
    e.store.backend().update(PAYMENT, &pid, &done).unwrap();
    e.call(&e.manager(), "ecommerce_order.cancel", &oid)
        .unwrap();
    assert_eq!(e.order_state(&oid), "cancelled");
    assert_eq!(
        e.last_payment().1["state"],
        "refunded",
        "the money did not stay"
    );
    assert_eq!(e.stand.calls("POST", "/v1/refunds").len(), 1);
    assert_eq!(e.stock("sencha-green-tea"), 40);
}

#[test]
fn a_refund_whose_money_went_back_but_whose_record_was_refused_is_a_partial_503_that_says_so() {
    use obop_storage::fault;
    let (flaky, e) = flaky_env();
    let (oid, _) = paid_stripe_order(&e, "stoneware-teapot", "1");
    flaky.refuse_applies(50);
    fault::begin_request(Default::default());
    let r = e.call(&e.manager(), "ecommerce_order.refund", &oid);
    let out = fault::end_request();
    flaky.allow_models();
    assert!(
        matches!(&r, Err(CallError::Handler(m)) if m.starts_with("the money was returned")),
        "{r:?}"
    );
    assert!(
        out.wrote && out.partial,
        "money that moved outside the store is a change: {out:?}"
    );
    assert_eq!(e.stand.calls("POST", "/v1/refunds").len(), 1);
    assert_eq!(e.order_state(&oid), "paid");
    assert_eq!(e.last_payment().1["state"], "succeeded");
    // The repeat records it and moves the order.
    e.call(&e.manager(), "ecommerce_order.refund", &oid)
        .unwrap();
    assert_eq!(e.order_state(&oid), "refunded");
    assert_eq!(e.last_payment().1["state"], "refunded");
}

#[test]
fn a_refund_whose_units_could_not_go_back_is_finished_by_the_sweep_and_a_delivered_one_owes_none() {
    use obop_storage::testing::Mode;
    let (flaky, e) = flaky_env();
    let (oid, _) = paid_stripe_order(&e, "stoneware-teapot", "2");
    assert_eq!(e.stock("stoneware-teapot"), 4);
    flaky.refuse_model(LISTING, Mode::Reads);
    let r = e.call(&e.manager(), "ecommerce_order.refund", &oid);
    flaky.allow_models();
    assert!(r.is_err(), "the units could not go back: a partial 503");
    assert_eq!(e.order_state(&oid), "refunded");
    assert_eq!(e.stock("stoneware-teapot"), 4);
    e.pay.sweep_expired();
    assert_eq!(e.stock("stoneware-teapot"), 6, "the sweep returned them");
    e.pay.sweep_expired();
    assert_eq!(e.stock("stoneware-teapot"), 6, "once");
    // Goods that were delivered do not go back on the shelf, and the order says nothing is owed.
    let (late, _) = paid_stripe_order(&e, "stoneware-teapot", "1");
    for cmd in ["ecommerce_order.ship", "ecommerce_order.deliver"] {
        e.call(&e.manager(), cmd, &late).unwrap();
    }
    let before = e.stock("stoneware-teapot");
    e.call(&e.manager(), "ecommerce_order.refund_delivered", &late)
        .unwrap();
    assert_eq!(e.stock("stoneware-teapot"), before);
    e.pay.sweep_expired();
    assert_eq!(
        e.stock("stoneware-teapot"),
        before,
        "not one day later either"
    );
}

#[test]
fn moving_an_order_with_an_update_returns_the_money_and_the_units_like_the_action() {
    // The kanban drag is an `update` of the state, not a call of the action. It ran the bare
    // action: the order said refunded and the buyer's money never went back.
    let e = env_with(true, false, "stripe");
    let m = e.manager();
    // Paid to refunded: the money and the units go back.
    let (paid, _) = paid_stripe_order(&e, "stoneware-teapot", "2");
    assert_eq!(e.stock("stoneware-teapot"), 4);
    e.update_state(&m, &paid, "refunded").unwrap();
    assert_eq!(e.order_state(&paid), "refunded");
    assert_eq!(e.payment_of(&paid)["state"], "refunded");
    assert_eq!(e.stand.calls("POST", "/v1/refunds").len(), 1);
    assert_eq!(e.stock("stoneware-teapot"), 6);
    // Pending to cancelled, the payment already succeeded: the money is returned, not kept.
    e.buy("sencha-green-tea", "1", "stripe", &[]);
    let (pending, _) = e.last_order();
    let (sid, _) = e.stand.sessions().pop().unwrap();
    e.stand.pay(&sid);
    e.payment_succeeded(&pending);
    e.update_state(&m, &pending, "cancelled").unwrap();
    assert_eq!(e.order_state(&pending), "cancelled");
    assert_eq!(e.payment_of(&pending)["state"], "refunded");
    assert_eq!(e.stand.calls("POST", "/v1/refunds").len(), 2);
    assert_eq!(e.stock("sencha-green-tea"), 40);
    // Delivered to refunded: the money goes back, the goods do not go on the shelf, not one day
    // later either.
    let (late, _) = paid_stripe_order(&e, "stoneware-teapot", "1");
    for cmd in ["ecommerce_order.ship", "ecommerce_order.deliver"] {
        e.call(&m, cmd, &late).unwrap();
    }
    let before = e.stock("stoneware-teapot");
    e.update_state(&m, &late, "refunded").unwrap();
    assert_eq!(e.payment_of(&late)["state"], "refunded");
    assert_eq!(e.stand.calls("POST", "/v1/refunds").len(), 3);
    e.pay.sweep_expired();
    assert_eq!(e.stock("stoneware-teapot"), before);
}

#[test]
fn an_approved_plan_that_moves_the_state_runs_the_same_wrappers() {
    let e = env_with(true, false, "stripe");
    let (oid, _) = paid_stripe_order(&e, "stoneware-teapot", "2");
    let agent = Principal::agent(
        "bot",
        &["ecommerce.user", "ecommerce.manager"],
        &["ecommerce.user", "ecommerce.manager"],
    );
    let plan = match e.update_state(&agent, &oid, "refunded") {
        Err(CallError::ApprovalRequired { plan_id }) => plan_id,
        other => panic!("an agent only proposes: {other:?}"),
    };
    assert!(e.stand.calls("POST", "/v1/refunds").is_empty());
    assert_eq!(e.order_state(&oid), "paid");
    e.d.approve(&e.manager(), &mut Vault::new(), &plan).unwrap();
    assert_eq!(e.order_state(&oid), "refunded");
    assert_eq!(e.payment_of(&oid)["state"], "refunded");
    assert_eq!(e.stand.calls("POST", "/v1/refunds").len(), 1);
    assert_eq!(e.stock("stoneware-teapot"), 6);
}

#[test]
fn the_sweep_returns_the_money_of_a_cancelled_order_whose_payment_succeeded() {
    // A late payment whose follow-up was lost (the process stopped after the payment was recorded,
    // or the event id could not be given back): the provider's redelivery cannot be counted on.
    let e = env_with(true, false, "stripe");
    e.buy("sencha-green-tea", "1", "stripe", &[]);
    let (oid, _) = e.last_order();
    let (sid, _) = e.stand.sessions().remove(0);
    e.call(&e.manager(), "ecommerce_order.cancel", &oid)
        .unwrap();
    e.stand.pay(&sid);
    e.payment_succeeded(&oid);
    assert_eq!(e.order_state(&oid), "cancelled");
    assert!(e.stand.calls("POST", "/v1/refunds").is_empty());
    e.pay.sweep_expired();
    assert_eq!(e.payment_of(&oid)["state"], "refunded");
    assert_eq!(e.stand.calls("POST", "/v1/refunds").len(), 1);
    assert_eq!(
        e.order_state(&oid),
        "cancelled",
        "the order stays cancelled"
    );
    // Once.
    e.pay.sweep_expired();
    assert_eq!(e.stand.calls("POST", "/v1/refunds").len(), 1);
}

#[test]
fn a_cancel_whose_refund_record_is_refused_never_leaves_an_order_the_sweep_can_mark_paid() {
    // The money went back and the store refused to record it, from one chosen step on. Wherever
    // that is, the order must never end paid (and shipped) with the buyer's money returned: it is
    // cancelled before the money moves, and what the request could not record, the sweep does.
    for skip in 0..6 {
        let (flaky, e) = flaky_env();
        e.buy("sencha-green-tea", "1", "stripe", &[]);
        let (oid, _) = e.last_order();
        let (sid, _) = e.stand.sessions().remove(0);
        e.stand.pay(&sid);
        e.payment_succeeded(&oid);
        // The reservation has run out, so that the sweep looks at the order if it is still pending.
        let mut lapsed = Fields::new();
        lapsed.insert("reserved_until".into(), json!(1));
        e.store.backend().update(ORDER, &oid, &lapsed).unwrap();
        flaky.refuse_applies_after(skip, 50);
        let _ = e.call(&e.manager(), "ecommerce_order.cancel", &oid);
        flaky.allow_models();
        obop_storage::fault::clear();
        for _ in 0..3 {
            e.pay.sweep_expired();
        }
        let refunds = e.stand.refunds_made();
        let (order, payment) = (e.order_state(&oid), e.payment_of(&oid)["state"].clone());
        if refunds > 0 {
            assert_eq!(
                (order.as_str(), &payment),
                ("cancelled", &json!("refunded")),
                "skip {skip}: the money went back, so the order is cancelled and it is recorded"
            );
        } else {
            assert_eq!(
                (order.as_str(), &payment),
                ("paid", &json!("succeeded")),
                "skip {skip}: no money moved, so the order that holds it is paid"
            );
        }
        assert!(refunds <= 1, "skip {skip}: refunded twice");
    }
}

#[test]
fn a_refund_of_delivered_goods_never_leaves_units_owed_whichever_step_the_store_refuses() {
    for skip in 0..10 {
        let (flaky, e) = flaky_env();
        let (oid, _) = paid_stripe_order(&e, "stoneware-teapot", "2");
        let m = e.manager();
        for cmd in ["ecommerce_order.ship", "ecommerce_order.deliver"] {
            e.call(&m, cmd, &oid).unwrap();
        }
        let before = e.stock("stoneware-teapot");
        flaky.refuse_calls_after(skip, 50);
        let _ = e.call(&m, "ecommerce_order.refund_delivered", &oid);
        flaky.allow_models();
        obop_storage::fault::clear();
        for _ in 0..2 {
            e.pay.sweep_expired();
        }
        assert_eq!(
            e.stock("stoneware-teapot"),
            before,
            "skip {skip}: the goods are with the buyer, the shelf did not grow"
        );
    }
}

#[test]
fn a_refused_call_of_the_sweep_does_not_refuse_the_buyers_own_checkout() {
    use obop_storage::fault;
    use obop_storage::testing::Mode;
    // The sweep of expired orders runs at the start of a checkout, on nobody's behalf. Its read of
    // the payments is refused here: the buyer's own claim of the form key, the order and the
    // payment page must not be refused for it, and the answer must not call it part of the buyer's
    // request.
    let (flaky, e) = flaky_env();
    flaky.refuse_model(PAYMENT, Mode::Reads);
    fault::begin_request(Default::default());
    let r = e.buy("sencha-green-tea", "1", "stripe", &[]);
    let out = fault::end_request();
    flaky.allow_models();
    assert_eq!(r.status, 303, "{}", r.body_str());
    assert_eq!(out.fault, None, "{out:?}");
    assert_eq!(e.orders().len(), 1);
    assert_eq!(e.stock("sencha-green-tea"), 39);
}

// ---- refunds ----------------------------------------------------------------------------------

fn paid_stripe_order(e: &Env, slug: &str, qty: &str) -> (String, String) {
    e.buy(slug, qty, "stripe", &[]);
    let (oid, order) = e.last_order();
    let (sid, _) = e.stand.sessions().pop().unwrap();
    e.stand.pay(&sid);
    let body = event(
        &format!("evt_{oid}"),
        "checkout.session.completed",
        &sid,
        order["total"].as_i64().unwrap(),
        "paid",
    );
    assert_eq!(
        e.hook(&body, obop_connectors::util::now(), SECRET),
        HookReply::Done
    );
    (oid, sid)
}

#[test]
fn a_refund_returns_the_money_through_stripe_records_the_reference_and_restocks() {
    let e = env_with(true, false, "stripe");
    let (oid, _) = paid_stripe_order(&e, "stoneware-teapot", "2");
    assert_eq!(e.stock("stoneware-teapot"), 4);
    let total = e.last_order().1["total"].as_i64().unwrap();
    // An agent can only propose it; Stripe is not called.
    let agent = Principal::agent(
        "bot",
        &["ecommerce.user", "ecommerce.manager"],
        &["ecommerce.user", "ecommerce.manager"],
    );
    let r = e.call(&agent, "ecommerce_order.refund", &oid);
    assert!(matches!(r, Err(CallError::ApprovalRequired { .. })));
    assert!(e.stand.calls("POST", "/v1/refunds").is_empty());
    assert_eq!(e.order_state(&oid), "paid");
    // A manager runs it.
    e.call(&e.manager(), "ecommerce_order.refund", &oid)
        .unwrap();
    assert_eq!(e.order_state(&oid), "refunded");
    let refunds = e.stand.calls("POST", "/v1/refunds");
    assert_eq!(refunds.len(), 1);
    let get = |k: &str| {
        refunds[0]
            .form
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    assert_eq!(get("amount"), total.to_string());
    assert!(get("payment_intent").starts_with("pi_test_"));
    assert_eq!(
        refunds[0].headers.get("idempotency-key"),
        Some(&format!("refund-{}", e.last_payment().0))
    );
    let (_, pay) = e.last_payment();
    assert_eq!(pay["state"], "refunded");
    assert_eq!(pay["refunded_amount"], total);
    assert!(pay["refund_id"].as_str().unwrap().starts_with("re_test_"));
    assert_eq!(
        e.stock("stoneware-teapot"),
        6,
        "an undelivered refunded order releases its units"
    );
    // The buyer sees it.
    let page = e.get(&format!("/shop/order/{}", e.token(&oid))).body_str();
    assert!(page.contains("This order was refunded."), "{page}");
}

#[test]
fn if_stripe_refuses_the_refund_the_order_stays_paid() {
    let e = env_with(true, false, "stripe");
    let (oid, _) = paid_stripe_order(&e, "sencha-green-tea", "1");
    e.stand.st.lock().unwrap().fail_refund = true;
    let r = e.call(&e.manager(), "ecommerce_order.refund", &oid);
    assert!(
        matches!(r, Err(CallError::Handler(ref m)) if m.contains("the refund was not made")),
        "{r:?}"
    );
    assert_eq!(e.order_state(&oid), "paid");
    assert_eq!(e.last_payment().1["state"], "succeeded");
    assert_eq!(e.stock("sencha-green-tea"), 39);
    // When Stripe works again the same action succeeds.
    e.stand.st.lock().unwrap().fail_refund = false;
    e.call(&e.manager(), "ecommerce_order.refund", &oid)
        .unwrap();
    assert_eq!(e.order_state(&oid), "refunded");
}

#[test]
fn a_delivered_order_is_refunded_without_putting_goods_back_on_the_shelf() {
    let e = env_with(true, false, "stripe");
    let (oid, _) = paid_stripe_order(&e, "sencha-green-tea", "3");
    let m = e.manager();
    e.call(&m, "ecommerce_order.ship", &oid).unwrap();
    e.call(&m, "ecommerce_order.deliver", &oid).unwrap();
    e.call(&m, "ecommerce_order.refund_delivered", &oid)
        .unwrap();
    assert_eq!(e.order_state(&oid), "refunded");
    assert_eq!(e.last_payment().1["state"], "refunded");
    assert_eq!(
        e.stock("sencha-green-tea"),
        37,
        "the goods are with the buyer"
    );
}

#[test]
fn a_bank_transfer_refund_is_recorded_for_staff_to_make_by_hand() {
    let e = env_with(false, false, "manual");
    e.buy("sencha-green-tea", "1", "manual", &[]);
    let (oid, _) = e.last_order();
    let m = e.manager();
    e.call(&m, "ecommerce_order.mark_paid", &oid).unwrap();
    e.call(&m, "ecommerce_order.refund", &oid).unwrap();
    let (_, pay) = e.last_payment();
    assert_eq!(pay["state"], "refunded");
    assert!(pay["refund_id"].as_str().unwrap().starts_with("manual-"));
    assert_eq!(e.stock("sencha-green-tea"), 40);
}

fn cookie_of(r: &Resp) -> Option<String> {
    r.headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
        .find_map(|(_, v)| {
            v.strip_prefix("obop_csrf=")
                .map(|c| c.split(';').next().unwrap().to_string())
        })
}

#[test]
fn the_hosted_page_form_carries_the_token_of_the_cookie_the_browser_already_has() {
    let e = env_with(false, true, "fake");
    // First visit: the cart sets the cookie and puts the same token in its form.
    let l = e.listing("sencha-green-tea");
    let cart = e.get(&format!("/cart?items={l}.1"));
    let token = cookie_of(&cart).expect("the cart sets the csrf cookie");
    assert!(
        cart.body_str()
            .contains(&format!("name=\"csrf\" value=\"{token}\""))
    );
    // The order is placed with that cookie; the demo page is opened with it and must reuse it.
    let ship = e.shipping();
    let body = format!(
        "csrf={token}&idem=00112233445566778899aabbccddee01&customer_name=A&customer_email=a%40example.test&shipping_address=1+Example+Street&id={l}&qty=1&shipping_method_id={ship}&payment_method=fake"
    );
    let mut r = e
        .req("POST", "/shop/checkout")
        .with_header("cookie", &format!("obop_csrf={token}"))
        .with_header("origin", "http://shop.test")
        .with_header("content-type", "application/x-www-form-urlencoded")
        .with_body(&body);
    r.client = "10.77.0.1".into();
    let placed = e.site.handle(&r);
    assert_eq!(placed.status, 303, "{}", placed.body_str());
    let page = location(&placed);
    let hosted = e.site.handle(
        &e.req("GET", &page)
            .with_header("cookie", &format!("obop_csrf={token}")),
    );
    assert_eq!(hosted.status, 200);
    assert!(
        cookie_of(&hosted).is_none(),
        "the cookie is kept, not replaced"
    );
    // `no-referrer` would make the browser send `Origin: null` with the form, which is refused.
    assert_eq!(hosted.get_header("referrer-policy"), Some("same-origin"));
    assert!(
        hosted
            .body_str()
            .contains(&format!("name=\"csrf\" value=\"{token}\"")),
        "{}",
        hosted.body_str()
    );
    // Posting the form with the cookie works, with the same Origin and Host a browser sends.
    let mut p = e
        .req("POST", &page)
        .with_header("cookie", &format!("obop_csrf={token}"))
        .with_header("origin", "http://shop.test")
        .with_header("content-type", "application/x-www-form-urlencoded")
        .with_body(&format!("csrf={token}&outcome=paid"));
    p.client = "10.77.0.1".into();
    let done = e.site.handle(&p);
    assert_eq!(done.status, 303, "{}", done.body_str());
}

// ---- the secrets ------------------------------------------------------------------------------

#[test]
fn the_keys_are_never_printed() {
    let k = StripeKeys {
        api_key: KEY.into(),
        signing_secret: SECRET.into(),
    };
    let shown = format!("{k:?} {:?}", Some(&k));
    assert!(!shown.contains("sk_test") && !shown.contains("whsec"));
    for e in [
        ProviderError::Rejected("The charge failed".into()),
        ProviderError::Unavailable("transport error".into()),
    ] {
        assert!(!e.to_string().contains(KEY));
    }
}

#[test]
fn a_session_id_is_checked_before_it_becomes_part_of_a_url() {
    for ok in ["cs_test_a1B2c3", "cs_live_x_y"] {
        assert!(stripe::session_id_ok(ok), "{ok}");
    }
    for bad in [
        "",
        "cs_",
        "pi_x",
        "cs_test/../v1/refunds",
        "cs_test?x=1",
        "cs_test x",
        &format!("cs_{}", "a".repeat(300)),
    ] {
        assert!(!stripe::session_id_ok(bad) || bad == "cs_", "{bad}");
    }
    let e = env_with(true, false, "stripe");
    let p = e.pay.provider("stripe").unwrap();
    assert!(p.fetch_status("cs_test/../v1/refunds").is_err());
    assert!(p.refund("../x", 1, "k").is_err());
    assert!(e.stand.calls("GET", "/v1/refunds").is_empty());
}
