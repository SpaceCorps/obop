//! Point of Sale 0.6.0 (docs/ISSUES.md Q2, Q3, Q16): a till's stock location reaches its sales,
//! the expected cash and the difference are computed on the server, and a new session opens with
//! the cash counted when the till last closed.

use crate::app::App;
use obop_commands::{ADMIN_GROUP, CallError, Principal, Surface, Value, Vault};
use obop_pkg::{Bundled, Marketplace};
use serde_json::{Value as J, json};

fn app() -> App {
    let app = App::new();
    for id in ["inventory", "pos"] {
        app.installer
            .install(&Bundled.fetch(id).unwrap(), "marketplace", "test")
            .unwrap_or_else(|e| panic!("{id}: {e}"));
    }
    app
}

fn admin() -> Principal {
    Principal::user("root", &[ADMIN_GROUP])
}

fn run(a: &App, cmd: &str, input: J) -> Result<J, String> {
    match a.dispatcher.call(
        Surface::Rest,
        &admin(),
        &mut Vault::new(),
        cmd,
        &Value::from_json(&input),
    ) {
        Ok(r) => Ok(r.value.to_json()),
        Err(CallError::Handler(m)) => Err(m),
        Err(e) => Err(format!("{e:?}")),
    }
}

fn ok(a: &App, cmd: &str, input: J) -> J {
    run(a, cmd, input).unwrap_or_else(|e| panic!("{cmd}: {e}"))
}

fn id(v: &J) -> String {
    v["id"].as_str().unwrap().to_string()
}

struct Fx {
    a: App,
    till: String,
    shelf: String,
    product: String,
}

fn fixture() -> Fx {
    let a = app();
    let wh = id(&ok(
        &a,
        "inventory_warehouse.create",
        json!({"name": "Shop", "code": "SH"}),
    ));
    let shelf = id(&ok(
        &a,
        "inventory_location.create",
        json!({"name": "Shop floor", "kind": "internal", "warehouse_id": wh}),
    ));
    let till = id(&ok(
        &a,
        "pos_config.create",
        json!({"name": "Front till", "default_opening_cash": 10_000, "stock_location_id": shelf}),
    ));
    let product = id(&ok(
        &a,
        "product.create",
        json!({"name": "Hammer", "list_price_minor": 2_500}),
    ));
    Fx {
        a,
        till,
        shelf,
        product,
    }
}

impl Fx {
    /// A paid sale of `qty` hammers in `session`, paid with these (method, amount) payments.
    fn sale(&self, session: &str, qty: i64, pays: &[(&str, i64)], pay: bool) -> String {
        let o = id(&ok(
            &self.a,
            "pos_order.create",
            json!({"session_id": session}),
        ));
        ok(
            &self.a,
            "pos_order_line.create",
            json!({"order_id": o, "product_id": self.product, "quantity": qty}),
        );
        for (m, v) in pays {
            ok(
                &self.a,
                "pos_payment.create",
                json!({"order_id": o, "method": m, "amount": v}),
            );
        }
        if pay {
            ok(&self.a, "pos_order.pay", json!({"id": o}));
        }
        o
    }

    fn session(&self) -> J {
        ok(
            &self.a,
            "pos_session.create",
            json!({"config_id": self.till}),
        )
    }
}

#[test]
fn a_sale_at_a_till_with_a_stock_location_moves_stock() {
    let f = fixture();
    let s = f.session();
    assert_eq!(s["stock_location_id"], json!(f.shelf));
    let o = f.sale(&id(&s), 2, &[("card", 5_000)], true);
    let order = ok(&f.a, "pos_order.get", json!({"id": o}));
    assert_eq!(order["stock_location_id"], json!(f.shelf));
    let moves = f.a.store.backend().list("inventory_move").unwrap();
    assert_eq!(moves.len(), 1, "one draft stock move per line");
    assert_eq!(moves[0].1["from_location_id"], json!(f.shelf));
    assert_eq!(moves[0].1["quantity"], json!(2));
}

#[test]
fn the_expected_cash_and_the_difference_are_computed_from_the_paid_sales() {
    let f = fixture();
    let s = id(&f.session());
    f.sale(&s, 2, &[("cash", 3_000), ("card", 2_000)], true);
    f.sale(&s, 1, &[("cash", 2_500)], true);
    // Neither an unpaid sale nor a cancelled one counts.
    f.sale(&s, 1, &[("cash", 2_500)], false);
    let cancelled = f.sale(&s, 1, &[("cash", 2_500)], false);
    ok(&f.a, "pos_order.cancel", json!({"id": cancelled}));
    let got = ok(&f.a, "pos_session.get", json!({"id": s}));
    assert_eq!(got["opening_cash"], json!(10_000));
    assert_eq!(got["expected_cash"], json!(15_500));
    assert_eq!(got["cash_difference"], json!(0), "nothing counted yet");
    // A client cannot write the expected cash any more.
    let e = run(
        &f.a,
        "pos_session.update",
        json!({"id": s, "expected_cash": 1, "cash_difference": 0}),
    )
    .unwrap_err();
    assert!(e.contains("computed"), "{e}");
    // The count alone: the difference follows, and the close asks for a note.
    ok(
        &f.a,
        "pos_session.update",
        json!({"id": s, "closing_cash": 15_000}),
    );
    let got = ok(&f.a, "pos_session.get", json!({"id": s}));
    assert_eq!(got["cash_difference"], json!(-500));
    let e = run(&f.a, "pos_session.close", json!({"id": s})).unwrap_err();
    assert!(e.contains("cash difference is not explained"), "{e}");
    ok(
        &f.a,
        "pos_session.update",
        json!({"id": s, "note": "Change given twice"}),
    );
    let closed = ok(&f.a, "pos_session.close", json!({"id": s}));
    assert_eq!(closed["state"], json!("closed"));
    let closed = ok(&f.a, "pos_session.get", json!({"id": s}));
    assert!(
        closed["closed_at"]
            .as_str()
            .is_some_and(|t| t.ends_with('Z')),
        "{closed}"
    );
}

#[test]
fn a_new_session_opens_with_the_cash_counted_when_the_till_last_closed() {
    let f = fixture();
    // Never closed: the till's usual opening cash.
    let p = ok(&f.a, "pos_opening.propose", json!({"config": f.till}));
    assert_eq!(
        (p["opening_cash"].clone(), p["source"].clone()),
        (json!(10_000), json!("till_default"))
    );
    let first = id(&f.session());
    ok(
        &f.a,
        "pos_session.update",
        json!({"id": first, "closing_cash": 10_000}),
    );
    ok(&f.a, "pos_session.close", json!({"id": first}));
    let second = f.session();
    assert_eq!(second["opening_cash"], json!(10_000));
    let second = id(&second);
    ok(
        &f.a,
        "pos_session.update",
        json!({"id": second, "closing_cash": 12_345, "note": "Float topped up"}),
    );
    ok(&f.a, "pos_session.close", json!({"id": second}));
    // Both closed on the same day: the later closing instant wins.
    let p = ok(&f.a, "pos_opening.propose", json!({"config": f.till}));
    assert_eq!(p["opening_cash"], json!(12_345));
    assert_eq!(p["source"], json!("last_close"));
    assert_eq!(p["session_id"], json!(second));
    let third = f.session();
    assert_eq!(third["opening_cash"], json!(12_345), "{third}");
    // What a person counts is kept.
    ok(&f.a, "pos_session.close", json!({"id": id(&third)}));
    let given = ok(
        &f.a,
        "pos_session.create",
        json!({"config_id": f.till, "opening_cash": 9_000}),
    );
    assert_eq!(given["opening_cash"], json!(9_000));
}
