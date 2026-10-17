//! The payment door and the shop's payment pages over HTTP (D62): the real router, the real
//! composition (`App`), the Stripe credentials read from a sealed `stripe` connection. No
//! provider is called; deliveries are signed with a throwaway secret.

use crate::app::App;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use obop_auth::Auth;
use obop_connectors::webhook::stripe_signature;
use obop_pkg::{Bundled, Marketplace};
use serde_json::{Value as J, json};
use std::sync::Arc;
use tower::ServiceExt;

const SECRET: &str = "whsec_throwaway_for_the_http_test_only";

struct Fx {
    router: axum::Router,
    app: App,
}

fn fixture() -> Fx {
    let app = App::new();
    for id in ["website", "ecommerce"] {
        app.installer
            .install(&Bundled.fetch(id).unwrap(), "marketplace", "test")
            .unwrap();
    }
    obop_site::seed::seed(&app.store, &app.dispatcher).unwrap();
    // The instance's own Stripe connection: connected, instance scope, sealed credentials.
    let creds = serde_json::Map::from_iter([
        ("api_key".to_string(), json!("sk_test_throwaway_never_used")),
        ("signing_secret".to_string(), json!(SECRET)),
    ]);
    app.store
        .backend()
        .insert(
            "connection",
            "00000000-0000-4000-8000-0000000000aa",
            &obop_storage::Fields::from_iter([
                ("name".to_string(), json!("Stripe")),
                ("provider".to_string(), json!("stripe")),
                ("owner_scope".to_string(), json!("instance")),
                ("state".to_string(), json!("connected")),
                (
                    "credentials".to_string(),
                    json!(app.connectors.seal_credentials(&creds)),
                ),
            ]),
        )
        .unwrap();
    let auth = Arc::new(Auth::with_test_params(app.store.backend()));
    let state = obop_http::HttpState::new(app.dispatcher.clone(), auth);
    state.set_payments(app.payments.clone());
    let site = Arc::new(
        obop_site::Site::new(
            obop_commands::PublicApi::new(app.dispatcher.clone(), app.store.clone()),
            obop_site::Config {
                settings_ttl: std::time::Duration::ZERO,
                ..obop_site::Config::default()
            },
        )
        .with_payments(app.payments.clone()),
    );
    let router = obop_http::public::router(
        state,
        None,
        site,
        obop_http::public::PublicConfig::default(),
    );
    Fx { router, app }
}

async fn send(r: &axum::Router, req: Request<Body>) -> (u16, String) {
    let resp = r.clone().oneshot(req).await.unwrap();
    let status = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

/// An order with an open Stripe payment, as checkout would have left it (the session itself is
/// not created here, so no request goes out).
fn pending_stripe_order(f: &Fx) -> (String, String, i64) {
    let b = f.app.store.backend();
    let order = "00000000-0000-4000-8000-0000000000b1";
    let total = 2740;
    b.insert(
        "ecommerce_order",
        order,
        &obop_storage::Fields::from_iter([
            ("number".to_string(), json!("W20260101-AAAAAA")),
            ("customer_email".to_string(), json!("ann@example.test")),
            ("state".to_string(), json!("pending")),
            ("total".to_string(), json!(total)),
            ("currency".to_string(), json!("EUR")),
            ("stock_returned".to_string(), json!(false)),
            ("access_token".to_string(), json!("f".repeat(64))),
        ]),
    )
    .unwrap();
    b.insert(
        "ecommerce_payment",
        "00000000-0000-4000-8000-0000000000b2",
        &obop_storage::Fields::from_iter([
            ("order_id".to_string(), json!(order)),
            ("provider".to_string(), json!("stripe")),
            ("state".to_string(), json!("open")),
            ("amount".to_string(), json!(total)),
            ("currency".to_string(), json!("EUR")),
            ("external_id".to_string(), json!("cs_test_http_0001")),
            ("event_ids".to_string(), json!("[]")),
        ]),
    )
    .unwrap();
    (order.into(), "cs_test_http_0001".into(), total)
}

fn signed(body: &str, secret: &str) -> Request<Body> {
    let ts = obop_connectors::util::now();
    Request::post("/hooks/pay/stripe")
        .header("content-type", "application/json")
        .header(
            "stripe-signature",
            format!(
                "t={ts},v1={}",
                stripe_signature(secret, ts, body.as_bytes())
            ),
        )
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn completed(id: &str, session: &str, amount: i64) -> String {
    json!({"id": id, "type": "checkout.session.completed", "data": {"object": {
        "id": session, "payment_status": "paid", "amount_total": amount, "currency": "eur"}}})
    .to_string()
}

#[tokio::test]
async fn the_webhook_door_checks_the_signature_before_anything_else() {
    let f = fixture();
    let (order, session, total) = pending_stripe_order(&f);
    let body = completed("evt_http_1", &session, total);
    // No signature, a wrong one, a stale one, the wrong secret: all the same answer.
    let plain = Request::post("/hooks/pay/stripe")
        .body(Body::from(body.clone()))
        .unwrap();
    let (s1, b1) = send(&f.router, plain).await;
    let (s2, b2) = send(&f.router, signed(&body, "whsec_not_the_secret")).await;
    let stale = {
        let ts = obop_connectors::util::now() - 7200;
        Request::post("/hooks/pay/stripe")
            .header(
                "stripe-signature",
                format!(
                    "t={ts},v1={}",
                    stripe_signature(SECRET, ts, body.as_bytes())
                ),
            )
            .body(Body::from(body.clone()))
            .unwrap()
    };
    let (s3, b3) = send(&f.router, stale).await;
    assert_eq!((s1, s2, s3), (401, 401, 401));
    assert_eq!((b1.as_str(), b2.as_str()), (b2.as_str(), b3.as_str()));
    let state = |f: &Fx| {
        f.app
            .store
            .backend()
            .get("ecommerce_order", &order)
            .unwrap()
            .unwrap()["state"]
            .clone()
    };
    assert_eq!(state(&f), "pending");
    // The signed one works, and the same delivery again is acknowledged without effect.
    let (s, b) = send(&f.router, signed(&body, SECRET)).await;
    assert_eq!((s, b.as_str()), (200, "{\"received\":true}"));
    assert_eq!(state(&f), "paid");
    let (s, _) = send(&f.router, signed(&body, SECRET)).await;
    assert_eq!(s, 200);
    assert_eq!(state(&f), "paid");
    // Providers that take no webhooks, and unknown ones.
    for p in ["manual", "fake", "paypal", "..%2F..", "STRIPE"] {
        let r = Request::post(format!("/hooks/pay/{p}"))
            .body(Body::from("{}"))
            .unwrap();
        assert_eq!(send(&f.router, r).await.0, 404, "{p}");
    }
}

#[tokio::test]
async fn security_the_webhook_body_is_capped_and_only_post_is_taken() {
    let f = fixture();
    let big = vec![b'x'; 300 * 1024];
    let r = Request::post("/hooks/pay/stripe")
        .body(Body::from(big))
        .unwrap();
    assert_eq!(send(&f.router, r).await.0, 413);
    let r = Request::get("/hooks/pay/stripe")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&f.router, r).await.0, 405);
    // A forged event for a payment that is not ours, signed with the right secret, is ignored.
    let body = completed("evt_http_x", "cs_test_not_ours", 1);
    assert_eq!(send(&f.router, signed(&body, SECRET)).await.0, 200);
    assert_eq!(f.app.store.count("ecommerce_payment"), 0);
}

#[tokio::test]
async fn the_shop_works_over_http_with_bank_transfer_and_shows_what_to_do() {
    let f = fixture();
    let b = f.app.store.backend();
    let listing = b
        .list("ecommerce_listing")
        .unwrap()
        .into_iter()
        .find(|(_, l)| l["slug"] == "linen-tea-towel")
        .unwrap()
        .0;
    let ship = b.list("ecommerce_shipping_method").unwrap().remove(0).0;
    let csrf = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let body = format!(
        "csrf={csrf}&idem=00112233445566778899aabbccddeeff&customer_name=Ann&customer_email=ann%40example.test&shipping_address=1+Example+Street%0A00000+Testville&id={listing}&qty=2&shipping_method_id={ship}&payment_method=manual"
    );
    let r = Request::post("/shop/checkout")
        .header("host", "shop.test")
        .header("origin", "http://shop.test")
        .header("cookie", format!("obop_csrf={csrf}"))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .unwrap();
    let resp = f.router.clone().oneshot(r).await.unwrap();
    assert_eq!(resp.status().as_u16(), 303);
    let loc = resp.headers()["location"].to_str().unwrap().to_string();
    assert!(loc.starts_with("/shop/order/"));
    let page = Request::get(&loc)
        .header("host", "shop.test")
        .body(Body::empty())
        .unwrap();
    let (s, html) = send(&f.router, page).await;
    assert_eq!(s, 200);
    assert!(
        html.contains("W20") && html.contains("Linen Tea Towel"),
        "{html}"
    );
    // The demo shop has no bank details: the buyer is told they will be contacted.
    assert!(html.contains("ann@example.test"), "{html}");
    let order: J = serde_json::to_value(b.list("ecommerce_order").unwrap().remove(0).1).unwrap();
    assert_eq!(order["payment_method"], "manual");
    assert_eq!(order["state"], "pending");
}
