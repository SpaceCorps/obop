//! Stock, discount codes, taxes and totals at checkout (D61), over the real bundled packages.
//! Each test that guards a threat has a name starting with `security_`.

use super::*;
use crate::tests::*;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};

static N: AtomicUsize = AtomicUsize::new(1);

fn unique() -> usize {
    N.fetch_add(1, Ordering::Relaxed)
}

/// Place an order for `lines` from a fresh address with a fresh idempotency key.
fn buy(s: &Shop, lines: &[(&str, &str)], extra: &[(&str, &str)]) -> Resp {
    let n = unique();
    let idem = format!("{n:032x}");
    let mut fields = order_fields(lines, extra);
    fields.retain(|(k, _)| *k != "idem");
    fields.push(("idem", &idem));
    let email = format!("buyer{n}@example.test");
    if extra.iter().any(|(k, _)| *k == "customer_email") {
        // The caller named the address: drop the default one.
        fields.retain(|(k, v)| !(*k == "customer_email" && *v == "buyer@example.test"));
    } else {
        fields.retain(|(k, _)| *k != "customer_email");
        fields.push(("customer_email", &email));
    }
    post_from(
        s,
        "/shop/checkout",
        &fields,
        &format!("10.20.{}.{}", n / 250, n % 250),
    )
}

fn std_method(s: &Shop) -> String {
    methods(s).remove(0).id
}

fn category(s: &Shop, slug: &str) -> String {
    s.store
        .backend()
        .list("ecommerce_category")
        .unwrap()
        .into_iter()
        .find(|(_, f)| f["slug"] == slug)
        .map(|(id, _)| id)
        .unwrap()
}

fn add_code(s: &Shop, kv: &[(&str, serde_json::Value)]) -> String {
    let mut f = obop_storage::Fields::new();
    f.insert("kind".into(), json!("percent"));
    f.insert("value".into(), json!(1000));
    f.insert("active".into(), json!(true));
    for (k, v) in kv {
        f.insert(k.to_string(), v.clone());
    }
    let id = uuid_like();
    s.store
        .backend()
        .insert("ecommerce_discount_code", &id, &f)
        .unwrap();
    id
}

fn add_rate(s: &Shop, name: &str, bp: i64, country: &str, cat: Option<&str>, included: bool) {
    let mut f = obop_storage::Fields::new();
    f.insert("name".into(), json!(name));
    f.insert("rate_bp".into(), json!(bp));
    f.insert("country".into(), json!(country));
    f.insert("included".into(), json!(included));
    f.insert("active".into(), json!(true));
    if let Some(c) = cat {
        f.insert("category_id".into(), json!(c));
    }
    s.store
        .backend()
        .insert("ecommerce_tax_rate", &uuid_like(), &f)
        .unwrap();
}

fn uuid_like() -> String {
    let n = unique();
    format!("00000000-0000-4000-8000-{n:012x}")
}

fn orders(s: &Shop) -> Vec<(String, obop_storage::Fields)> {
    s.store.backend().list("ecommerce_order").unwrap()
}

fn last_order(s: &Shop) -> obop_storage::Fields {
    orders(s).pop().unwrap().1
}

fn stock(s: &Shop, slug: &str) -> i64 {
    s.store
        .backend()
        .get("ecommerce_listing", &listing(s, slug).id)
        .unwrap()
        .unwrap()["stock_quantity"]
        .as_i64()
        .unwrap()
}

fn code_uses(s: &Shop, id: &str) -> i64 {
    s.store
        .backend()
        .get("ecommerce_discount_code", id)
        .unwrap()
        .unwrap()
        .get("used_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
}

const GENERIC: &str = "This code cannot be applied to your cart.";

#[test]
fn an_order_takes_its_units_off_the_listing_and_the_last_unit_goes_once() {
    let s = shop();
    let pot = listing(&s, "stoneware-teapot");
    let std = std_method(&s);
    assert_eq!(stock(&s, "stoneware-teapot"), 6);
    assert_eq!(
        buy(&s, &[(&pot.id, "4")], &[("shipping_method_id", &std)]).status,
        303
    );
    assert_eq!(stock(&s, "stoneware-teapot"), 2, "the units are gone");
    // Three would leave less than nothing: refused with the number that is left.
    let r = buy(&s, &[(&pot.id, "3")], &[("shipping_method_id", &std)]);
    assert_eq!(r.status, 400);
    assert!(
        text(&r).contains("Only 2 of Stoneware Teapot, 0.8 l left."),
        "{}",
        text(&r)
    );
    assert_eq!(
        orders(&s).len(),
        1,
        "the refused checkout left nothing behind"
    );
    assert_eq!(stock(&s, "stoneware-teapot"), 2);
    assert_eq!(
        buy(&s, &[(&pot.id, "2")], &[("shipping_method_id", &std)]).status,
        303
    );
    assert_eq!(stock(&s, "stoneware-teapot"), 0);
    // Sold out: the page says so and the cart refuses it.
    let page = text(&get(&s, &format!("/shop/{}-stoneware-teapot", pot.id)));
    assert!(page.contains("Out of stock"));
    let r = buy(&s, &[(&pot.id, "1")], &[("shipping_method_id", &std)]);
    assert_eq!(r.status, 400);
    assert!(text(&r).contains("no longer available"));
}

#[test]
fn backorder_and_always_available_listings_are_counted_or_left_alone_as_declared() {
    let s = shop();
    let std = std_method(&s);
    let cups = listing(&s, "tea-cups-set-of-two"); // allow_backorder, stock 0
    let jasmine = listing(&s, "jasmine-pearls"); // always_available
    assert_eq!(
        buy(&s, &[(&cups.id, "3")], &[("shipping_method_id", &std)]).status,
        303
    );
    assert_eq!(
        stock(&s, "tea-cups-set-of-two"),
        -3,
        "backorders are counted"
    );
    assert_eq!(
        buy(&s, &[(&jasmine.id, "5")], &[("shipping_method_id", &std)]).status,
        303
    );
    assert_eq!(stock(&s, "jasmine-pearls"), 0, "not tracked, not changed");
}

#[test]
fn security_two_buyers_racing_for_the_last_unit_get_one_order_and_stock_never_goes_negative() {
    let s = shop();
    let std = std_method(&s);
    let pot = listing(&s, "stoneware-teapot");
    set_state(
        &s,
        "ecommerce_listing",
        &pot.id,
        "stock_quantity",
        Value::Int(3),
    );
    let ok = AtomicUsize::new(0);
    std::thread::scope(|sc| {
        for _ in 0..12 {
            sc.spawn(|| {
                if buy(&s, &[(&pot.id, "1")], &[("shipping_method_id", &std)]).status == 303 {
                    ok.fetch_add(1, Ordering::SeqCst);
                }
            });
        }
    });
    assert_eq!(
        ok.load(Ordering::SeqCst),
        3,
        "exactly the units that existed were sold"
    );
    assert_eq!(stock(&s, "stoneware-teapot"), 0);
    assert_eq!(orders(&s).len(), 3);
    assert_eq!(
        s.store
            .backend()
            .list("ecommerce_order_line")
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn a_percent_code_takes_its_share_off_the_lines_and_is_shown_on_the_order() {
    let s = shop();
    let std = std_method(&s);
    let code = add_code(&s, &[("code", json!("SPRING10"))]);
    let sencha = listing(&s, "sencha-green-tea"); // 1250
    // Lower case with spaces still finds it; the discount is 10% of the line.
    let r = buy(
        &s,
        &[(&sencha.id, "2")],
        &[("shipping_method_id", &std), ("code", "  spring10 ")],
    );
    assert_eq!(r.status, 303, "{}", text(&r));
    let o = last_order(&s);
    assert_eq!(o["subtotal"], 2500);
    assert_eq!(o["discount_total"], 250);
    assert_eq!(o["discount_code"], "SPRING10");
    assert_eq!(o["discount_code_id"], code.as_str());
    assert_eq!(o["shipping_total"], 490);
    assert_eq!(o["total"], 2500 - 250 + 490);
    assert_eq!(code_uses(&s, &code), 1);
    let (_, line) = s
        .store
        .backend()
        .list("ecommerce_order_line")
        .unwrap()
        .remove(0);
    assert_eq!(line["discount_amount"], 250);
    // The confirmation page shows the discount as a line.
    let token = o["access_token"].as_str().unwrap().to_string();
    let page = text(&get(&s, &format!("/shop/order/{token}")));
    assert!(page.contains("Discount (SPRING10)"), "{page}");
    assert!(page.contains("−€2.50") || page.contains("\u{2212}€2.50"));
    // A fixed code never exceeds the lines.
    add_code(
        &s,
        &[
            ("code", json!("TAKE5")),
            ("kind", json!("fixed")),
            ("value", json!(500_000)),
        ],
    );
    assert_eq!(
        buy(
            &s,
            &[(&sencha.id, "1")],
            &[("shipping_method_id", &std), ("code", "take5")]
        )
        .status,
        303
    );
    let o = last_order(&s);
    assert_eq!(o["discount_total"], 1250);
    assert_eq!(o["total"], 490, "delivery is not discounted");
}

#[test]
fn security_every_reason_a_code_fails_gives_the_same_answer() {
    let s = shop();
    let std = std_method(&s);
    let sencha = listing(&s, "sencha-green-tea");
    let tea = category(&s, "tea");
    let ceramics = category(&s, "ceramics");
    add_code(&s, &[("code", json!("OFF"))]); // switched off below
    s.store
        .backend()
        .update(
            "ecommerce_discount_code",
            &s.store.backend().list("ecommerce_discount_code").unwrap()[0].0,
            &{
                let mut f = obop_storage::Fields::new();
                f.insert("active".into(), json!(false));
                f
            },
        )
        .unwrap();
    add_code(
        &s,
        &[("code", json!("OLD")), ("ends_on", json!("2020-01-01"))],
    );
    add_code(
        &s,
        &[
            ("code", json!("FUTURE")),
            ("starts_on", json!("2999-01-01")),
        ],
    );
    add_code(
        &s,
        &[("code", json!("BIG")), ("minimum_order", json!(900_000))],
    );
    add_code(
        &s,
        &[
            ("code", json!("USEDUP")),
            ("max_uses", json!(2)),
            ("used_count", json!(2)),
        ],
    );
    add_code(
        &s,
        &[("code", json!("POTS")), ("category_id", json!(ceramics))],
    );
    add_code(&s, &[("code", json!("BADKIND")), ("kind", json!("magic"))]);
    add_code(&s, &[("code", json!("ZERO")), ("value", json!(0))]);
    add_code(&s, &[("code", json!("OVER")), ("value", json!(20_000))]);
    add_code(&s, &[("code", json!("TEA")), ("category_id", json!(tea))]);
    let mut answers = Vec::new();
    for c in [
        "NOSUCHCODE",
        "OFF",
        "OLD",
        "FUTURE",
        "BIG",
        "USEDUP",
        "POTS",
        "BADKIND",
        "ZERO",
        "OVER",
        "x",
        "<script>",
        &"A".repeat(500),
        "TEA; DROP",
        "",
    ] {
        if c.is_empty() {
            continue;
        }
        let r = buy(
            &s,
            &[(&sencha.id, "1")],
            &[("shipping_method_id", &std), ("code", c)],
        );
        assert_eq!(r.status, 400, "{c}");
        answers.push(text(&r).contains(GENERIC));
    }
    assert!(answers.iter().all(|a| *a), "{answers:?}");
    assert!(orders(&s).is_empty());
    // The one that fits the cart works (and it is a different code from all the others).
    let f = |c: &str| {
        buy(
            &s,
            &[(&sencha.id, "1")],
            &[("shipping_method_id", &std), ("code", c)],
        )
    };
    assert_eq!(f("TEA").status, 303);
}

#[test]
fn security_failed_attempts_are_limited_per_address_and_the_check_endpoint_says_the_same_thing() {
    let s = shop();
    let sencha = listing(&s, "sencha-green-tea");
    add_code(&s, &[("code", json!("REAL"))]);
    let items = format!("{}.1", sencha.id);
    let check = |code: &str, from: &str| {
        let r = post_from(
            &s,
            "/shop/discount/check",
            &[("items", &items), ("code", code)],
            from,
        );
        assert_eq!(r.status, 200);
        assert_eq!(r.get_header("content-type"), Some("application/json"));
        serde_json::from_str::<serde_json::Value>(&text(&r)).unwrap()
    };
    let ok = check("real", "198.51.100.1");
    assert_eq!(ok["ok"], true);
    assert_eq!(ok["discount"], 125);
    assert_eq!(
        ok["total"],
        1125 + 490,
        "the first delivery method is preselected"
    );
    // Ten wrong guesses, then even the right code is not looked at for a while.
    for i in 0..10 {
        let v = check(&format!("GUESS{i}"), "198.51.100.9");
        assert_eq!(v["ok"], false);
        assert_eq!(v["message"], GENERIC);
    }
    let blocked = check("REAL", "198.51.100.9");
    assert_eq!(blocked["ok"], false);
    assert!(
        blocked["message"]
            .as_str()
            .unwrap()
            .contains("Too many attempts")
    );
    // Another address is not affected, and the checkout form is held to the same limit.
    assert_eq!(check("REAL", "198.51.100.2")["ok"], true);
    // It needs the CSRF token like every other post.
    let mut r = req("POST", "/shop/discount/check")
        .with_header("origin", "http://shop.test")
        .with_body(&format!("items={items}&code=REAL"));
    r.client = "198.51.100.3".into();
    assert_eq!(s.site.handle(&r).status, 403);
}

#[test]
fn security_a_code_cannot_be_used_past_its_maximum_even_when_buyers_race() {
    let s = shop();
    let std = std_method(&s);
    let sencha = listing(&s, "sencha-green-tea");
    let code = add_code(&s, &[("code", json!("FIRST3")), ("max_uses", json!(3))]);
    let with_code = AtomicUsize::new(0);
    std::thread::scope(|sc| {
        for _ in 0..10 {
            sc.spawn(|| {
                let r = buy(
                    &s,
                    &[(&sencha.id, "1")],
                    &[("shipping_method_id", &std), ("code", "FIRST3")],
                );
                if r.status == 303 {
                    with_code.fetch_add(1, Ordering::SeqCst);
                }
            });
        }
    });
    assert_eq!(with_code.load(Ordering::SeqCst), 3);
    assert_eq!(code_uses(&s, &code), 3);
    assert_eq!(orders(&s).len(), 3);
    assert_eq!(
        stock(&s, "sencha-green-tea"),
        37,
        "refused orders took no stock"
    );
}

#[test]
fn a_per_customer_limit_counts_by_email_without_keeping_the_address() {
    let s = shop();
    let std = std_method(&s);
    let sencha = listing(&s, "sencha-green-tea");
    add_code(
        &s,
        &[("code", json!("ONCE")), ("per_customer_limit", json!(1))],
    );
    let go = |email: &str| {
        buy(
            &s,
            &[(&sencha.id, "1")],
            &[
                ("shipping_method_id", &std),
                ("code", "ONCE"),
                ("customer_email", email),
            ],
        )
    };
    assert_eq!(go("Ann@Example.test").status, 303);
    assert_eq!(
        go("ann@example.test").status,
        400,
        "same address, other spelling"
    );
    assert_eq!(go("bob@example.test").status, 303);
    let uses = s.store.backend().list("ecommerce_discount_use").unwrap();
    assert_eq!(uses.len(), 2);
    let dump = format!("{uses:?}");
    assert!(
        !dump.contains("example.test"),
        "only a hash is kept: {dump}"
    );
}

#[test]
fn security_nothing_the_browser_sends_sets_a_discount_a_tax_or_a_total() {
    let s = shop();
    let std = std_method(&s);
    add_rate(&s, "VAT", 1900, "", None, true);
    let sencha = listing(&s, "sencha-green-tea");
    let forged = [
        ("shipping_method_id", std.as_str()),
        ("discount_total", "99999"),
        ("discount_amount", "99999"),
        ("discount_code_id", "x"),
        ("tax_total", "0"),
        ("tax_amount", "0"),
        ("tax_rate_bp", "0"),
        ("tax_summary", "[]"),
        ("total", "1"),
        ("subtotal", "1"),
        ("stock_returned", "true"),
        ("reserved_until", "99999999999"),
        ("state", "paid"),
        ("used_count", "-100"),
        ("stock_quantity", "9999"),
    ];
    let r = buy(&s, &[(&sencha.id, "1")], &forged);
    assert_eq!(r.status, 303, "{}", text(&r));
    let o = last_order(&s);
    assert_eq!(
        (o["subtotal"].as_i64(), o["discount_total"].as_i64()),
        (Some(1250), Some(0))
    );
    assert_eq!(
        o["tax_total"], 278,
        "tax (goods and delivery) comes from the stored rate"
    );
    assert_eq!(o["total"], 1250 + 490);
    assert_eq!(o["state"], "pending");
    assert_eq!(o["stock_returned"], false);
    assert_eq!(
        o["payment_method"], "manual",
        "only the offered methods exist"
    );
    assert_eq!(stock(&s, "sencha-green-tea"), 39);
    // A way of paying that is not offered is refused.
    let mut again = forged.to_vec();
    again.push(("payment_method", "stripe"));
    let r = buy(&s, &[(&sencha.id, "1")], &again);
    assert_eq!(r.status, 400);
    assert!(text(&r).contains("Choose how you want to pay."));
    assert_eq!(stock(&s, "sencha-green-tea"), 39);
    // Quantities: negative, zero, huge, fractional, not a number.
    for q in ["-1", "0", "100", "99999999999999999999", "1.5", "x", ""] {
        let r = buy(&s, &[(&sencha.id, q)], &[("shipping_method_id", &std)]);
        assert_eq!(r.status, 400, "qty {q:?}");
    }
    assert_eq!(
        stock(&s, "sencha-green-tea"),
        39,
        "refused orders take nothing"
    );
    // A price so large it overflows the order total is refused, not wrapped.
    set_state(
        &s,
        "ecommerce_listing",
        &sencha.id,
        "price",
        Value::Int(i64::MAX / 2),
    );
    let r = buy(&s, &[(&sencha.id, "99")], &[("shipping_method_id", &std)]);
    assert_eq!(r.status, 400);
    assert_eq!(stock(&s, "sencha-green-tea"), 39);
}

#[test]
fn included_tax_is_shown_not_added_and_added_tax_goes_on_top() {
    let s = shop();
    let std = std_method(&s);
    let sencha = listing(&s, "sencha-green-tea"); // 1250
    add_rate(&s, "VAT", 1900, "", None, true);
    let r = buy(&s, &[(&sencha.id, "1")], &[("shipping_method_id", &std)]);
    assert_eq!(r.status, 303);
    let o = last_order(&s);
    // 1250 and 490 delivery, both gross: 1250*1900/11900 = 199.58 -> 200, 490*1900/11900 = 78.2 -> 78
    assert_eq!(o["tax_total"], 278);
    assert_eq!(
        o["total"], 1740,
        "an included tax does not change what is paid"
    );
    let summary: serde_json::Value =
        serde_json::from_str(o["tax_summary"].as_str().unwrap()).unwrap();
    assert_eq!(summary[0]["name"], "VAT");
    assert_eq!(summary[0]["included"], true);
    assert_eq!(summary[0]["amount"], 278);
    let token = o["access_token"].as_str().unwrap().to_string();
    let page = text(&get(&s, &format!("/shop/order/{token}")));
    assert!(page.contains("Includes VAT 19%"), "{page}");
    // Now a tax that is added on top, for a country with a rate of its own.
    let s2 = shop();
    let sencha2 = listing(&s2, "sencha-green-tea");
    let std2 = std_method(&s2);
    add_rate(&s2, "Sales tax", 825, "US", None, false);
    add_rate(&s2, "VAT", 1900, "", None, true);
    let r = buy(
        &s2,
        &[(&sencha2.id, "2")],
        &[("shipping_method_id", &std2), ("country", "us")],
    );
    assert_eq!(r.status, 303);
    let o = last_order(&s2);
    // 2500*8.25% = 206.25 -> 206; delivery 490*8.25% = 40.4 -> 40
    assert_eq!(o["tax_total"], 246);
    assert_eq!(o["total"], 2500 + 490 + 246);
    assert_eq!(o["country"], "US");
    // Another country falls back to the general rate (included).
    let r = buy(
        &s2,
        &[(&sencha2.id, "2")],
        &[("shipping_method_id", &std2), ("country", "FR")],
    );
    assert_eq!(r.status, 303);
    assert_eq!(last_order(&s2)["total"], 2500 + 490);
    // A made-up or malformed country is treated as no country, never as an error or an injection.
    for c in ["x", "DEU", "<b>", "' OR 1=1"] {
        let r = buy(
            &s2,
            &[(&sencha2.id, "1")],
            &[("shipping_method_id", &std2), ("country", c)],
        );
        assert_eq!(r.status, 303, "{c}");
        assert!(
            last_order(&s2)
                .get("country")
                .is_none_or(|v| v.is_null() || v == ""),
            "{c}"
        );
    }
    let r = buy(
        &s2,
        &[(&sencha2.id, "1")],
        &[("shipping_method_id", &std2), ("country", "zz")],
    );
    assert_eq!(r.status, 303);
    assert_eq!(
        last_order(&s2)["country"],
        "ZZ",
        "any two-letter code is kept, and has no rate of its own"
    );
}

#[test]
fn a_category_rate_applies_only_to_its_listings_and_the_cart_shows_a_country_choice() {
    let s = shop();
    let std = std_method(&s);
    let tea = category(&s, "tea");
    add_rate(&s, "Tea tax", 700, "DE", Some(&tea), true);
    add_rate(&s, "VAT", 1900, "DE", None, true);
    let sencha = listing(&s, "sencha-green-tea"); // tea
    let pot = listing(&s, "stoneware-teapot"); // ceramics, 4800
    let r = buy(
        &s,
        &[(&sencha.id, "1"), (&pot.id, "1")],
        &[("shipping_method_id", &std), ("country", "DE")],
    );
    assert_eq!(r.status, 303, "{}", text(&r));
    let lines = s.store.backend().list("ecommerce_order_line").unwrap();
    let by = |price: i64| {
        lines
            .iter()
            .find(|(_, l)| l["unit_price"] == price)
            .unwrap()
            .1
            .clone()
    };
    assert_eq!(by(1250)["tax_name"], "Tea tax");
    assert_eq!(by(1250)["tax_rate_bp"], 700);
    assert_eq!(by(4800)["tax_name"], "VAT");
    assert_eq!(by(4800)["tax_amount"], 766); // 4800*1900/11900 = 766.39
    // The cart offers the countries that have a rate.
    let page = text(&get(&s, &format!("/cart?items={}.1", sencha.id)));
    assert!(page.contains("name=\"country\""));
    assert!(page.contains(">Germany<"));
    assert!(page.contains("Another country"));
    // And its totals fragment carries the tax lines for the chosen country.
    let frag = get(
        &s,
        &format!(
            "/cart?items={}.1&country=DE&ship={std}&part=totals",
            sencha.id
        ),
    );
    assert_eq!(frag.status, 200);
    let f = text(&frag);
    assert!(f.starts_with("<dl class=\"totals\" data-totals>"), "{f}");
    assert!(f.contains("Includes Tea tax 7%"), "{f}");
    assert!(!f.contains("<html"), "a fragment, not a page");
}

#[test]
fn delivery_is_priced_by_the_server_and_free_over_the_threshold() {
    let s = shop();
    let std = std_method(&s); // 490, free over 5000
    let pot = listing(&s, "stoneware-teapot"); // 4800
    let tea = listing(&s, "sencha-green-tea"); // 1250
    assert_eq!(
        buy(&s, &[(&pot.id, "1")], &[("shipping_method_id", &std)]).status,
        303
    );
    assert_eq!(last_order(&s)["shipping_total"], 490);
    assert_eq!(
        buy(
            &s,
            &[(&pot.id, "1"), (&tea.id, "1")],
            &[("shipping_method_id", &std)]
        )
        .status,
        303
    );
    assert_eq!(last_order(&s)["shipping_total"], 0, "6050 is over 50.00");
    // A delivery method id that is not an active public one is refused.
    let r = buy(
        &s,
        &[(&tea.id, "1")],
        &[("shipping_method_id", "00000000-0000-4000-8000-000000000000")],
    );
    assert_eq!(r.status, 400);
    let r = buy(&s, &[(&tea.id, "1")], &[]);
    assert_eq!(
        r.status, 400,
        "a delivery method is required when there are some"
    );
}

#[test]
fn the_cart_shows_the_code_the_tax_and_works_without_javascript() {
    let s = shop();
    let tea = listing(&s, "sencha-green-tea");
    add_code(&s, &[("code", json!("SPRING10"))]);
    add_rate(&s, "VAT", 1900, "", None, true);
    let std = std_method(&s);
    let page = text(&get(
        &s,
        &format!("/cart?items={}.2&code=spring10&ship={std}", tea.id),
    ));
    assert!(page.contains("Code SPRING10 applied."));
    assert!(page.contains("Discount (SPRING10)"));
    assert!(page.contains("Includes VAT 19%"));
    assert!(
        page.contains("name=\"code\" value=\"SPRING10\""),
        "the applied code travels to checkout"
    );
    assert!(page.contains("data-checkout"));
    // A wrong code says nothing about why.
    let page = text(&get(&s, &format!("/cart?items={}.2&code=NOPE", tea.id)));
    assert!(page.contains(GENERIC));
    assert!(!page.contains("Discount ("));
}
