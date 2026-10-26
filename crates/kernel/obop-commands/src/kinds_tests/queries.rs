//! Filters, order and aggregates over decimals, against an independent `i128` oracle.

use super::*;

struct Rec {
    name: String,
    id: String,
    tier: &'static str,
    /// Hundredths, the units of `price` at scale 2.
    price: Option<i128>,
}

fn seed_prices(r: &Rig, rng: &mut Rng, n: usize) -> Vec<Rec> {
    (0..n)
        .map(|i| {
            let tier = ["a", "b", "c"][rng.below(3) as usize];
            let price = (rng.below(15) != 0).then(|| i128::from(rng.range(-3000, 3000)));
            let name = format!("r{i:04}");
            let mut fields = vec![("name", t(&name)), ("tier", t(tier))];
            if let Some(p) = price {
                fields.push(("price", t(&show(p, 2))));
            }
            let id = r.create("item", fields);
            Rec {
                name,
                id,
                tier,
                price,
            }
        })
        .collect()
}

fn random_operand(rng: &mut Rng) -> String {
    let scale = rng.below(5) as u32;
    let bound = 32 * pow10(scale) as i64;
    show(i128::from(rng.range(-bound, bound)), scale)
}

type Keep = Box<dyn Fn(&Rec) -> bool>;

#[test]
fn decimal_filters_and_order_match_an_i128_oracle() {
    each(|r| {
        let mut rng = Rng(0xdec1_0001);
        let recs = seed_prices(r, &mut rng, 300);
        for _ in 0..250 {
            let op = ["eq", "ne", "gt", "gte", "lt", "lte", "in", "is_null"][rng.below(8) as usize];
            let (value, keep): (Value, Keep) = match op {
                "is_null" => {
                    let want = rng.below(2) == 0;
                    (
                        Value::Bool(want),
                        Box::new(move |x: &Rec| x.price.is_none() == want),
                    )
                }
                "in" => {
                    let xs: Vec<String> = (0..1 + rng.below(4))
                        .map(|_| random_operand(&mut rng))
                        .collect();
                    let v = Value::List(xs.iter().map(|x| t(x)).collect());
                    (
                        v,
                        Box::new(move |x: &Rec| {
                            x.price.is_some_and(|p| {
                                xs.iter().any(|o| cmp_text(&show(p, 2), o).is_eq())
                            })
                        }),
                    )
                }
                _ => {
                    let o = random_operand(&mut rng);
                    let o2 = o.clone();
                    (
                        t(&o),
                        Box::new(move |x: &Rec| {
                            let Some(p) = x.price else {
                                // An empty price is unequal to everything and in no range.
                                return op == "ne";
                            };
                            let ord = cmp_text(&show(p, 2), &o2);
                            match op {
                                "eq" => ord.is_eq(),
                                "ne" => ord.is_ne(),
                                "gt" => ord.is_gt(),
                                "gte" => ord.is_ge(),
                                "lt" => ord.is_lt(),
                                _ => ord.is_le(),
                            }
                        }),
                    )
                }
            };
            let mut got = r.names(vec![cond("price", op, value.clone())], None);
            got.sort();
            let mut want: Vec<String> = recs
                .iter()
                .filter(|x| keep(x))
                .map(|x| x.name.clone())
                .collect();
            want.sort();
            assert_eq!(got, want, "price {op} {value:?}");
        }
        // Order, both ways, with the id as the tie-break and empty values last.
        for dir in ["asc", "desc"] {
            let order = Value::List(vec![m(vec![("field", t("price")), ("dir", t(dir))])]);
            let got = r.names(vec![], Some(order));
            let mut want: Vec<&Rec> = recs.iter().collect();
            want.sort_by(|a, b| match (a.price, b.price) {
                (None, None) => a.id.cmp(&b.id),
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (Some(_), None) => std::cmp::Ordering::Less,
                (Some(x), Some(y)) => {
                    let o = if dir == "asc" { x.cmp(&y) } else { y.cmp(&x) };
                    o.then_with(|| a.id.cmp(&b.id))
                }
            });
            let want: Vec<String> = want.iter().map(|x| x.name.clone()).collect();
            assert_eq!(got, want, "order {dir}");
        }
        // Not lexical: "9.50" comes before "10.25", in the order and in a range.
        r.create("item", vec![("name", t("lo")), ("price", t("9.50"))]);
        r.create("item", vec![("name", t("hi")), ("price", t("10.25"))]);
        assert_eq!(
            r.names(
                vec![cond("name", "in", Value::List(vec![t("lo"), t("hi")]))],
                Some(Value::List(vec![m(vec![("field", t("price"))])]))
            ),
            ["lo", "hi"]
        );
        let mut got = r.names(
            vec![
                cond("price", "lt", t("10.25")),
                cond("price", "gt", t("9.5")),
            ],
            None,
        );
        got.sort();
        let mut want: Vec<String> = recs
            .iter()
            .filter(|x| x.price.is_some_and(|p| p > 950 && p < 1025))
            .map(|x| x.name.clone())
            .collect();
        want.sort();
        assert_eq!(got, want);
        // `contains` is for text only.
        let e = r.err(
            "item",
            "search",
            m(vec![(
                "filter",
                Value::List(vec![cond("price", "contains", t("1"))]),
            )]),
        );
        assert!(e.contains("contains only works on text"), "{e}");
    });
}

fn measures(ops: &[(&str, &str)]) -> Value {
    Value::List(
        ops.iter()
            .map(|(op, field)| {
                let mut v = vec![("op", t(op))];
                if !field.is_empty() {
                    v.push(("field", t(field)));
                }
                m(v)
            })
            .collect(),
    )
}

/// `num / den` rounded half away from zero, with plain integer arithmetic.
fn half_up(num: i128, den: i128) -> i128 {
    let q = (2 * num.abs() + den) / (2 * den);
    if num < 0 { -q } else { q }
}

#[test]
fn decimal_aggregates_match_an_i128_oracle() {
    each(|r| {
        let mut rng = Rng(0xa665_0002);
        let recs = seed_prices(r, &mut rng, 400);
        let out = r.search(
            "item",
            vec![(
                "aggregate",
                m(vec![
                    ("group_by", Value::List(vec![t("tier")])),
                    (
                        "measures",
                        Value::List(vec![
                            m(vec![("op", t("count"))]),
                            m(vec![("op", t("sum")), ("field", t("price"))]),
                            m(vec![("op", t("min")), ("field", t("price"))]),
                            m(vec![("op", t("max")), ("field", t("price"))]),
                            m(vec![
                                ("op", t("avg")),
                                ("field", t("price")),
                                ("scale", Value::Int(4)),
                                ("round", t("half_up")),
                            ]),
                        ]),
                    ),
                ]),
            )],
        );
        let groups = list(get(&out, "groups"));
        assert_eq!(groups.len(), 3);
        for g in groups {
            let tier = text(get(get(g, "key"), "tier"));
            let prices: Vec<i128> = recs
                .iter()
                .filter(|x| x.tier == tier)
                .filter_map(|x| x.price)
                .collect();
            let sum: i128 = prices.iter().sum();
            let ms = get(g, "measures");
            assert_eq!(text(get(ms, "sum:price")), show(sum, 2), "sum {tier}");
            assert_eq!(
                text(get(ms, "min:price")),
                show(*prices.iter().min().unwrap(), 2)
            );
            assert_eq!(
                text(get(ms, "max:price")),
                show(*prices.iter().max().unwrap(), 2)
            );
            // sum is in hundredths; the average at 4 digits is sum * 100 / n.
            let avg = half_up(sum * 100, prices.len() as i128);
            assert_eq!(text(get(ms, "avg:price")), show(avg, 4), "avg {tier}");
            assert_eq!(
                int(get(g, "count")),
                recs.iter().filter(|x| x.tier == tier).count() as i64
            );
        }
        // Plain totals, and the average at the field's own scale rounds half even.
        let out = r.search(
            "item",
            vec![(
                "aggregate",
                m(vec![(
                    "measures",
                    measures(&[("sum", "price"), ("avg", "price")]),
                )]),
            )],
        );
        let all: Vec<i128> = recs.iter().filter_map(|x| x.price).collect();
        let sum: i128 = all.iter().sum();
        let ms = get(&list(get(&out, "groups"))[0], "measures").clone();
        assert_eq!(text(get(&ms, "sum:price")), show(sum, 2));
        // Half even by the remainder test, written differently from the implementation.
        let n = all.len() as i128;
        let (q, rem) = (sum.div_euclid(n), sum.rem_euclid(n));
        let even = match (2 * rem).cmp(&n) {
            std::cmp::Ordering::Less => q,
            std::cmp::Ordering::Greater => q + 1,
            std::cmp::Ordering::Equal => {
                if q % 2 == 0 {
                    q
                } else {
                    q + 1
                }
            }
        };
        assert_eq!(text(get(&ms, "avg:price")), show(even, 2));
        // Records without a price: the sum is zero at the field's scale, the others are empty.
        let out = r.search(
            "item",
            vec![
                (
                    "filter",
                    Value::List(vec![cond("price", "is_null", Value::Bool(true))]),
                ),
                (
                    "aggregate",
                    m(vec![(
                        "measures",
                        measures(&[("sum", "price"), ("min", "price"), ("avg", "price")]),
                    )]),
                ),
            ],
        );
        let ms = get(&list(get(&out, "groups"))[0], "measures").clone();
        assert_eq!(text(get(&ms, "sum:price")), "0.00");
        assert_eq!(get(&ms, "min:price"), &Value::Null);
        assert_eq!(get(&ms, "avg:price"), &Value::Null);
        // Grouping by a decimal orders the groups as numbers.
        let out = r.search(
            "item",
            vec![
                (
                    "filter",
                    Value::List(vec![cond(
                        "price",
                        "in",
                        Value::List(vec![t("9.5"), t("10.25"), t("-3")]),
                    )]),
                ),
                (
                    "aggregate",
                    m(vec![("group_by", Value::List(vec![t("price")]))]),
                ),
            ],
        );
        let keys: Vec<String> = list(get(&out, "groups"))
            .iter()
            .map(|g| text(get(get(g, "key"), "price")))
            .collect();
        let mut want: Vec<String> = [-300i128, 950, 1025]
            .iter()
            .filter(|p| recs.iter().any(|x| x.price == Some(**p)))
            .map(|p| show(*p, 2))
            .collect();
        want.sort_by(|a, b| cmp_text(a, b));
        assert_eq!(keys, want);
    });
}

#[test]
fn a_measure_asked_for_twice_is_refused_and_a_value_wider_than_its_field_is_not_cut() {
    each(|r| {
        let a = r.create("item", vec![("name", t("a")), ("price", t("1.50"))]);
        r.create("item", vec![("name", t("b")), ("price", t("2.25"))]);
        let sum = |ops: &[(&str, &str)]| {
            r.call(
                "item",
                "search",
                m(vec![("aggregate", m(vec![("measures", measures(ops))]))]),
            )
        };
        // Two averages of one field at different digits would share the name avg:price, and the
        // second would overwrite the first: asking is refused instead.
        let twice = Value::List(vec![
            m(vec![("op", t("avg")), ("field", t("price")), ("scale", Value::Int(1))]),
            m(vec![("op", t("avg")), ("field", t("price")), ("scale", Value::Int(3))]),
        ]);
        let e = r.err(
            "item",
            "search",
            m(vec![("aggregate", m(vec![("measures", twice)]))]),
        );
        assert!(e.contains("avg:price") && e.contains("twice"), "{e}");
        let ok = sum(&[("sum", "price"), ("avg", "price")]).unwrap().value;
        let g = &list(get(&ok, "groups"))[0];
        assert_eq!(text(get(get(g, "measures"), "sum:price")), "3.75");
        // A value with more digits than the field keeps (the scale was cut without rewriting the
        // stored text) is an error, not a sum of numbers cut to fit.
        r.corrupt("item", &a, "price", serde_json::json!("1.509"));
        for ops in [
            [("sum", "price")],
            [("min", "price")],
            [("max", "price")],
        ] {
            match sum(&ops) {
                Err(CallError::Handler(e)) => {
                    assert!(e.contains("more than the 2 fractional digits"), "{e}");
                    assert!(e.contains(ops[0].0) && e.contains("1.509"), "{e}");
                }
                other => panic!("{ops:?}: {other:?}"),
            }
        }
        // Counting needs no digits.
        assert!(sum(&[("count", "")]).is_ok());
    });
}

#[test]
fn aggregate_input_for_decimals_is_validated() {
    each(|r| {
        let bad = |a: Value| r.err("item", "search", m(vec![("aggregate", a)]));
        let one = |op: &str, f: &str, extra: Vec<(&str, Value)>| {
            let mut v = vec![("op", t(op)), ("field", t(f))];
            v.extend(extra);
            m(vec![("measures", Value::List(vec![m(v)]))])
        };
        assert!(bad(one("avg", "qty", vec![])).contains("only decimal fields have an average"));
        assert!(bad(one("avg", "price", vec![("scale", Value::Int(13))])).contains("0 to 12"));
        assert!(bad(one("avg", "price", vec![("round", t("up"))])).contains("half_even"));
        assert!(bad(one("sum", "price", vec![("scale", Value::Int(2))])).contains("only avg does"));
        assert!(bad(one("sum", "name", vec![])).contains("number, money and decimal"));
        assert!(
            bad(m(vec![("group_by", Value::List(vec![t("at")]))]))
                .contains("datetime needs a bucket")
        );
    });
}
