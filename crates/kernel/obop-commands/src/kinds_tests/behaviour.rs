//! Computed decimals and instants, CSV import with rounding, effects, masking.

use super::*;
use obop_values::{Instant, ZoneDb, Zones};

#[test]
fn computed_decimals_are_exact_and_round_once_with_the_declared_mode() {
    each(|r| {
        let a = r.create(
            "item",
            vec![
                ("name", t("a")),
                ("price", t("10.00")),
                ("qty", Value::Int(3)),
            ],
        );
        let row = r.read("item", &a);
        assert_eq!(text(get(&row, "total")), "30.00");
        assert_eq!(text(get(&row, "third")), "3.3333", "10 / 3 at 4 digits");
        assert_eq!(get(&row, "sum_ok"), &Value::Bool(true), "0.1 + 0.2 == 0.3");
        for (price, third) in [
            ("0.01", "0.0033"),
            ("20.00", "6.6667"),
            ("-10.00", "-3.3333"),
            ("0.00", "0.0000"),
            ("100.00", "33.3333"),
        ] {
            let id = r.create("item", vec![("name", t("p")), ("price", t(price))]);
            assert_eq!(
                text(get(&r.read("item", &id), "third")),
                third,
                "{price} / 3"
            );
        }
        // No price: the computed values are empty or zero, never an error.
        let n = r.create("item", vec![("name", t("none"))]);
        let row = r.read("item", &n);
        assert_eq!(text(get(&row, "total")), "0.00");
        // A computed decimal can be filtered and sorted on, exactly.
        let hits = r.names(vec![cond("total", "gt", t("20.5"))], None);
        assert_eq!(hits, ["a"]);
        // It cannot be written.
        let e = r.err(
            "item",
            "update",
            m(vec![("id", t(&a)), ("total", t("1.00"))]),
        );
        assert!(e.contains("computed and read-only"), "{e}");
        // Instants compare in a computed bool.
        let early = r.create(
            "item",
            vec![("name", t("e")), ("at", t("2025-12-31T23:59:59Z"))],
        );
        let late = r.create(
            "item",
            vec![("name", t("l")), ("at", t("2026-01-01T00:00:00+00:00"))],
        );
        let shifted = r.create(
            "item",
            vec![("name", t("s")), ("at", t("2026-01-01T00:30:00+01:00"))],
        );
        assert_eq!(get(&r.read("item", &early), "early"), &Value::Bool(true));
        assert_eq!(get(&r.read("item", &late), "early"), &Value::Bool(false));
        assert_eq!(
            get(&r.read("item", &shifted), "early"),
            &Value::Bool(true),
            "23:30Z the day before"
        );
    });
}

/// A computed decimal is read by other computed fields as the rounded value the record shows,
/// so the parts of a total add up on screen and in an effect.
#[test]
fn a_computed_decimal_read_by_another_is_the_rounded_value() {
    each(|r| {
        let id = r.create(
            "item",
            vec![
                ("name", t("inv")),
                ("ratio", t("12.3456")),
                ("qty", Value::Int(3)),
            ],
        );
        let row = r.read("item", &id);
        // 12.3456 * 3 = 37.0368, shown 37.04; 37.04 * 0.19 = 7.0376, shown 7.04; the gross is
        // the sum of what is shown, 44.08 (it was 44.07, from the unrounded 37.0368 + 7.036992).
        assert_eq!(text(get(&row, "net")), "37.04");
        assert_eq!(text(get(&row, "vat")), "7.04");
        assert_eq!(text(get(&row, "gross")), "44.08");
        let sum = |a: &str, b: &str| {
            let (a, b) = (
                a.replace('.', "").parse::<i64>().unwrap(),
                b.replace('.', "").parse::<i64>().unwrap(),
            );
            a + b
        };
        assert_eq!(
            sum(&text(get(&row, "net")), &text(get(&row, "vat"))),
            text(get(&row, "gross"))
                .replace('.', "")
                .parse::<i64>()
                .unwrap()
        );
        // The other example of the review: 0.0149 * 1 shows 0.01, 0.00 and 0.01.
        let id = r.create(
            "item",
            vec![
                ("name", t("small")),
                ("ratio", t("0.0149")),
                ("qty", Value::Int(1)),
            ],
        );
        let row = r.read("item", &id);
        assert_eq!(
            (
                text(get(&row, "net")),
                text(get(&row, "vat")),
                text(get(&row, "gross"))
            ),
            ("0.01".to_string(), "0.00".to_string(), "0.01".to_string())
        );
        // Filtering and sorting see the same rounded value.
        assert_eq!(
            r.names(vec![cond("gross", "eq", t("44.08"))], None),
            ["inv"]
        );
    });
}

/// An assertion and an effect read the rounded value too, not the digits it was computed from:
/// `gross` is `44.08` (37.04 + 7.04), not 44.07, which is what the unrounded parts add up to.
#[test]
fn an_assertion_and_an_effect_read_the_rounded_computed_decimal() {
    each(|r| {
        let id = r.create(
            "item",
            vec![
                ("name", t("inv")),
                ("ratio", t("12.3456")),
                ("qty", Value::Int(3)),
            ],
        );
        r.ok("item", "bill", m(vec![("id", t(&id))]));
        let logs = list(get(&r.search("log", vec![]), "rows")).to_vec();
        assert_eq!(logs.len(), 1);
        assert_eq!(text(get(&logs[0], "price_copy")), "44.0800");
        // A record whose rounded parts do not give 44.08 fails the assertion: nothing happens.
        let other = r.create(
            "item",
            vec![
                ("name", t("other")),
                ("ratio", t("1.0000")),
                ("qty", Value::Int(1)),
            ],
        );
        let e = r.err("item", "bill", m(vec![("id", t(&other))]));
        assert!(e.contains("gross == 44.08"), "{e}");
        assert_eq!(text(get(&r.read("item", &other), "state")), "new");
        assert_eq!(list(get(&r.search("log", vec![]), "rows")).len(), 1);
    });
}

/// A whole-number constant (`rate = 0`) in an effect is the decimal it stands for.
#[test]
fn a_whole_number_constant_in_an_effect_fills_a_decimal_field() {
    each(|r| {
        let item = r.create("item", vec![("name", t("it")), ("price", t("1.00"))]);
        let gate = r.create("gate", vec![("item_id", t(&item))]);
        r.ok("gate", "close", m(vec![("id", t(&gate))]));
        let logs = list(get(&r.search("log", vec![]), "rows")).to_vec();
        assert_eq!(logs.len(), 1);
        assert_eq!(text(get(&logs[0], "price_copy")), "5.0000");
        let row = r.read("item", &item);
        assert_eq!(text(get(&row, "price")), "7.00");
        assert_eq!(text(get(&row, "ratio")), "0.0000");
    });
}

#[test]
fn sums_and_maxima_over_child_decimals_are_exact() {
    each(|r| {
        let item = r.create("item", vec![("name", t("order"))]);
        assert_eq!(text(get(&r.read("item", &item), "lines_total")), "0.00");
        assert_eq!(get(&r.read("item", &item), "lines_max"), &Value::Null);
        for amount in ["0.10", "0.20", "-0.05", "9.50", "10.25"] {
            r.create(
                "item_line",
                vec![("item_id", t(&item)), ("amount", t(amount))],
            );
        }
        let row = r.read("item", &item);
        assert_eq!(text(get(&row, "lines_total")), "20.00");
        assert_eq!(
            text(get(&row, "lines_max")),
            "10.25",
            "10.25 is the largest, not 9.50"
        );
    });
}

#[test]
fn csv_import_rounds_only_where_the_field_declares_a_mode() {
    each(|r| {
        let csv = "name,price,r_even,r_up,r_down\n\
                   a,1.00,1.005,1.005,1.009\n\
                   b,2.50,2.675,2.675,-2.679\n\
                   c,3,0.125,0.125,0.125\n\
                   d,4.5,-0.125,-0.125,-0.125\n";
        let out = r.ok("item", "bulk_create", m(vec![("csv", t(csv))]));
        assert_eq!(int(get(&out, "created")), 4);
        let by_name = |n: &str| {
            let id = text(get(
                &list(get(
                    &r.search(
                        "item",
                        vec![("filter", Value::List(vec![cond("name", "eq", t(n))]))],
                    ),
                    "rows",
                ))[0],
                "id",
            ));
            r.read("item", &id)
        };
        for (name, price, even, up, down) in [
            ("a", "1.00", "1.00", "1.01", "1.00"),
            ("b", "2.50", "2.68", "2.68", "-2.67"),
            ("c", "3.00", "0.12", "0.13", "0.12"),
            ("d", "4.50", "-0.12", "-0.13", "-0.12"),
        ] {
            let row = by_name(name);
            assert_eq!(
                (
                    text(get(&row, "price")),
                    text(get(&row, "r_even")),
                    text(get(&row, "r_up")),
                    text(get(&row, "r_down"))
                ),
                (
                    price.to_string(),
                    even.to_string(),
                    up.to_string(),
                    down.to_string()
                ),
                "{name}"
            );
        }
        // A field without a mode refuses the extra digits, naming the row and the line.
        let csv = "name,price\nok,1.00\nbad,1.005\n";
        let report = r.ok(
            "item",
            "bulk_create",
            m(vec![("csv", t(csv)), ("dry_run", Value::Bool(true))]),
        );
        assert_eq!(get(&report, "ok"), &Value::Bool(false));
        let errors = list(get(&report, "errors"));
        assert_eq!(errors.len(), 1);
        let msg = text(get(&errors[0], "error"));
        assert_eq!(int(get(&errors[0], "row")), 2);
        assert!(
            msg.contains("item.price must be") && msg.contains("3 fractional digits"),
            "{msg}"
        );
        // Decimal commas, exponents and spaces are not repaired by an import either.
        for bad in ["1,5", "1e2", "0x1", " 1.5", "1.5 ", "   "] {
            let csv = format!("name,price\nx,\"{bad}\"\n");
            let report = r.ok(
                "item",
                "bulk_create",
                m(vec![("csv", t(&csv)), ("dry_run", Value::Bool(true))]),
            );
            assert_eq!(get(&report, "ok"), &Value::Bool(false), "{bad}");
        }
        // The same rounding applies to rows given as JSON (it is the import command), but
        // `create` never rounds.
        let out = r.ok(
            "item",
            "bulk_create",
            m(vec![(
                "rows",
                Value::List(vec![m(vec![("name", t("j")), ("r_up", t("0.005"))])]),
            )]),
        );
        assert_eq!(int(get(&out, "created")), 1);
        let e = r.err(
            "item",
            "create",
            m(vec![("name", t("j")), ("r_up", t("0.005"))]),
        );
        assert!(e.contains("3 fractional digits"), "{e}");
        // Instants in a file: an offset is applied; no offset is an error with the line.
        let csv = "name,at\nz,2026-03-29T03:30:00+02:00\nw,2026-03-29T03:30:00\n";
        let report = r.ok(
            "item",
            "bulk_create",
            m(vec![("csv", t(csv)), ("dry_run", Value::Bool(true))]),
        );
        let errors = list(get(&report, "errors"));
        assert_eq!(errors.len(), 1);
        assert_eq!(int(get(&errors[0], "row")), 2);
        assert!(text(get(&errors[0], "error")).contains("no offset"));
    });
}

#[test]
fn now_and_today_are_typed_and_today_is_a_date_in_the_platform_zone() {
    each(|r| {
        let when = Instant::parse("2026-03-31T22:30:00Z").unwrap();
        r.d.set_clock(Arc::new(move || when.micros() / 1_000_000));
        let item = r.create("item", vec![("name", t("s")), ("price", t("12.34"))]);
        let log_of = || {
            let out = r.search("log", vec![]);
            list(get(&out, "rows")).to_vec()
        };
        r.ok("item", "stamp", m(vec![("id", t(&item))]));
        let rows = log_of();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            text(get(&rows[0], "stamped_at")),
            "2026-03-31T22:30:00.000000Z"
        );
        assert_eq!(
            text(get(&rows[0], "day")),
            "2026-03-31",
            "UTC until the platform zone is set"
        );
        assert_eq!(
            text(get(&rows[0], "price_copy")),
            "12.3400",
            "a decimal copied to a wider scale"
        );
        // In Berlin it is already April 1.
        r.d.set_zones(Zones::new(Arc::new(ZoneDb::bundled()), "Europe/Berlin").unwrap());
        let item2 = r.create("item", vec![("name", t("s2")), ("price", t("1"))]);
        r.ok("item", "stamp", m(vec![("id", t(&item2))]));
        let day2 = log_of()
            .iter()
            .filter(|l| text(get(l, "price_copy")) == "1.0000")
            .map(|l| text(get(l, "day")))
            .collect::<Vec<_>>();
        assert_eq!(day2, ["2026-04-01"]);
    });
}

#[test]
fn a_decimal_read_by_an_agent_is_masked_as_any_scalar_and_the_token_restores() {
    each(|r| {
        let id = r.create(
            "item",
            vec![
                ("name", t("m")),
                ("price", t("5.00")),
                ("fee", t("12.34")),
                ("salary", t("99.99")),
            ],
        );
        let agent = Principal::agent(&format!("bot-{}", r.p), &[U], &[U]);
        let mut vault = Vault::new();
        let read = |vault: &mut Vault| {
            r.d.call(
                Surface::Mcp,
                &agent,
                vault,
                &r.cmd("item", "get"),
                &m(vec![("id", t(&id))]),
            )
            .unwrap()
            .value
        };
        let row = read(&mut vault);
        // Personal data is a token for the agent, sensitive data is withheld, a plain decimal is
        // clear.
        assert_eq!(get(&row, "salary"), &Value::Null);
        let fee = text(get(&row, "fee"));
        assert_ne!(fee, "12.34");
        assert!(!fee.contains("12.34"));
        assert_eq!(text(get(&row, "price")), "5.00");
        // Sending the token back restores the real decimal before validation.
        r.d.call(
            Surface::Mcp,
            &agent,
            &mut vault,
            &r.cmd("item", "update"),
            &m(vec![("id", t(&id)), ("fee", t(&fee)), ("price", t("6.5"))]),
        )
        .unwrap();
        let now = r.read("item", &id);
        assert_eq!(text(get(&now, "fee")), "12.34", "the token stood for 12.34");
        assert_eq!(text(get(&now, "price")), "6.50");
        // An agent cannot filter or sum a decimal it cannot read in clear: the refusal is the
        // one for a field that does not exist, so it is no oracle for the values.
        for (field, input) in [
            (
                "fee",
                vec![("filter", Value::List(vec![cond("fee", "gt", t("1"))]))],
            ),
            (
                "salary",
                vec![("filter", Value::List(vec![cond("salary", "gt", t("1"))]))],
            ),
            (
                "salary",
                vec![(
                    "aggregate",
                    m(vec![(
                        "measures",
                        Value::List(vec![m(vec![("op", t("sum")), ("field", t("salary"))])]),
                    )]),
                )],
            ),
        ] {
            let e =
                r.d.call(
                    Surface::Mcp,
                    &agent,
                    &mut Vault::new(),
                    &r.cmd("item", "search"),
                    &m(input),
                )
                .unwrap_err();
            assert_eq!(
                e,
                CallError::Handler(format!("unknown field {}_item.{field}", r.p)),
                "{field}"
            );
        }
    });
}
