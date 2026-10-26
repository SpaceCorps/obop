//! Tests for locale-aware sorting (D3) and server-side aggregation (D4).

use super::*;

const FIXTURES: &str =
    include_str!("../../../../../web/apps/backoffice/src/collation.fixtures.json");

fn names(v: &Value) -> Vec<String> {
    let Value::Map(v) = v else { panic!("{v:?}") };
    rows_of(&v["rows"])
        .iter()
        .map(|x| match x {
            Value::Map(m) => match &m["name"] {
                Value::Text(t) => t.clone(),
                other => panic!("{other:?}"),
            },
            _ => panic!(),
        })
        .collect()
}

fn order(field: &str, dir: &str) -> Value {
    Value::List(vec![m(vec![("field", t(field)), ("dir", t(dir))])])
}

#[test]
fn text_sorts_with_the_collation_of_the_lang_hint_on_every_backend() {
    let fixtures: serde_json::Value = serde_json::from_str(FIXTURES).unwrap();
    for case in fixtures.as_array().unwrap() {
        let lang = case["lang"].as_str().unwrap().to_string();
        let input: Vec<&str> = case["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap())
            .collect();
        let expected: Vec<String> = case["expected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap().to_string())
            .collect();
        each_backend(|r| {
            for n in &input {
                r.seed("party", vec![("name", t(n))]);
            }
            let mut q = vec![("order", order("name", "asc"))];
            if !lang.is_empty() {
                q.push(("lang", t(&lang)));
            }
            let got = names(&r.ok(&user(), "party", "search", m(q.clone())));
            assert_eq!(got, expected, "{}", case["name"]);
            // Descending is the exact reverse when no two names tie.
            q[0] = ("order", order("name", "desc"));
            let mut rev = expected.clone();
            rev.reverse();
            let got = names(&r.ok(&user(), "party", "search", m(q)));
            assert_eq!(got, rev, "{} (desc)", case["name"]);
        });
    }
}

#[test]
fn equal_text_ties_end_at_the_record_id_and_empty_values_stay_last() {
    each_backend(|r| {
        let a = r.seed("party", vec![("name", t("Same")), ("age", Value::Int(1))]);
        let b = r.seed("party", vec![("name", t("Same")), ("age", Value::Int(2))]);
        let c = r.seed("party", vec![("age", Value::Int(3))]);
        let rows = |dir: &str| {
            let Value::Map(v) = r.ok(
                &user(),
                "party",
                "search",
                m(vec![("order", order("name", dir))]),
            ) else {
                panic!()
            };
            rows_of(&v["rows"])
                .iter()
                .map(|x| match x {
                    Value::Map(m) => match &m["id"] {
                        Value::Text(t) => t.clone(),
                        _ => panic!(),
                    },
                    _ => panic!(),
                })
                .collect::<Vec<_>>()
        };
        let mut ab = [a.clone(), b.clone()];
        ab.sort();
        // Ties keep the id order in both directions; the empty name is last in both.
        assert_eq!(rows("asc"), vec![ab[0].clone(), ab[1].clone(), c.clone()]);
        assert_eq!(rows("desc"), vec![ab[0].clone(), ab[1].clone(), c]);
    });
}

#[test]
fn lang_must_be_a_language_tag() {
    each_backend(|r| {
        for bad in ["", "not a tag!", "x".repeat(60).as_str()] {
            let e = search_err(r, &user(), m(vec![("lang", t(bad))]));
            assert!(e.contains("BCP-47"), "{e}");
        }
        let e = search_err(r, &user(), m(vec![("lang", Value::Int(1))]));
        assert!(e.contains("lang"), "{e}");
        // An unknown but well-formed tag falls back to the root order.
        r.ok(&user(), "party", "search", m(vec![("lang", t("zz-ZZ"))]));
    });
}

// ---- aggregation ----------------------------------------------------------------------------

struct Rng(u64);
impl Rng {
    fn next(&mut self, n: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) % n
    }
}

type Rec = (
    Option<&'static str>,
    Option<i64>,
    Option<i64>,
    Option<String>,
);

const TIERS: [&str; 3] = ["bronze", "silver", "gold"];

fn random_parties(r: &Rig, n: usize, seed: u64) -> Vec<Rec> {
    let mut rng = Rng(seed);
    let mut out = Vec::new();
    for i in 0..n {
        let tier = (rng.next(4) < 3).then(|| TIERS[rng.next(3) as usize]);
        let age = (rng.next(5) > 0).then(|| rng.next(200) as i64 - 50);
        let salary = (rng.next(5) > 0).then(|| rng.next(1_000_000) as i64);
        let born = (rng.next(6) > 0).then(|| {
            format!(
                "{:04}-{:02}-{:02}",
                2020 + rng.next(3),
                1 + rng.next(12),
                1 + rng.next(28)
            )
        });
        let mut pairs = vec![("name", t(&format!("p{i}")))];
        if let Some(x) = tier {
            pairs.push(("tier", t(x)));
        }
        if let Some(x) = age {
            pairs.push(("age", Value::Int(x)));
        }
        if let Some(x) = salary {
            pairs.push(("salary", Value::Int(x)));
        }
        if let Some(x) = &born {
            pairs.push(("born", t(x)));
        }
        r.seed("party", pairs);
        out.push((tier, age, salary, born));
    }
    out
}

type Group = (Vec<Value>, i64, Vec<Value>);

fn groups_of(v: &Value) -> (i64, Vec<Group>) {
    let Value::Map(v) = v else { panic!("{v:?}") };
    let Value::Int(total) = v["total"] else {
        panic!()
    };
    let g = match &v["groups"] {
        Value::List(l) => l
            .iter()
            .map(|g| {
                let Value::Map(g) = g else { panic!() };
                let Value::Map(k) = &g["key"] else { panic!() };
                let Value::Int(c) = g["count"] else { panic!() };
                let Value::Map(ms) = &g["measures"] else {
                    panic!()
                };
                (
                    k.values().cloned().collect(),
                    c,
                    ms.values().cloned().collect(),
                )
            })
            .collect(),
        other => panic!("{other:?}"),
    };
    (total, g)
}

fn agg(group_by: Vec<Value>, measures: Vec<Value>) -> Value {
    m(vec![
        ("group_by", Value::List(group_by)),
        ("measures", Value::List(measures)),
    ])
}

fn measure(op: &str, field: &str) -> Value {
    m(vec![("op", t(op)), ("field", t(field))])
}

fn bucketed(field: &str, bucket: &str) -> Value {
    m(vec![("field", t(field)), ("bucket", t(bucket))])
}

fn hr() -> Principal {
    Principal::user("hr", &[U, "t.hr"])
}

#[test]
fn aggregates_match_a_naive_oracle_on_random_data() {
    for seed in [1u64, 7, 99] {
        each_backend(|r| {
            let data = random_parties(r, 300, seed);
            let rows = |input: Value| groups_of(&r.ok(&hr(), "party", "search", input));

            // Group by a select and a month bucket, with every measure; the filter keeps age >= 20.
            let (total, got) = rows(m(vec![
                (
                    "filter",
                    Value::List(vec![cond("age", "gte", Value::Int(20))]),
                ),
                (
                    "aggregate",
                    agg(
                        vec![t("tier"), bucketed("born", "month")],
                        vec![
                            m(vec![("op", t("count"))]),
                            measure("sum", "salary"),
                            measure("min", "age"),
                            measure("max", "age"),
                        ],
                    ),
                ),
            ]));
            // The oracle.
            type Acc = (i64, i64, Option<i64>, Option<i64>);
            let mut want: std::collections::BTreeMap<(Option<usize>, Option<String>), Acc> =
                Default::default();
            let mut want_total = 0;
            for (tier, age, salary, born) in &data {
                if !age.is_some_and(|a| a >= 20) {
                    continue;
                }
                want_total += 1;
                let key = (
                    tier.map(|x| TIERS.iter().position(|y| *y == x).unwrap()),
                    born.as_ref().map(|b| b[..7].to_string()),
                );
                let e = want.entry(key).or_insert((0, 0, None, None));
                e.0 += 1;
                e.1 += salary.unwrap_or(0);
                if let Some(a) = age {
                    e.2 = Some(e.2.map_or(*a, |x| x.min(*a)));
                    e.3 = Some(e.3.map_or(*a, |x| x.max(*a)));
                }
            }
            assert_eq!(total, want_total);
            assert_eq!(got.len(), want.len());
            // Engine order: tier by option order then month, empty last, per key part.
            let mut expect: Vec<_> = want.into_iter().collect();
            expect.sort_by(|((t1, b1), _), ((t2, b2), _)| {
                let f = |a: &Option<usize>, b: &Option<usize>| match (a, b) {
                    (None, None) => std::cmp::Ordering::Equal,
                    (None, _) => std::cmp::Ordering::Greater,
                    (_, None) => std::cmp::Ordering::Less,
                    (Some(a), Some(b)) => a.cmp(b),
                };
                let g = |a: &Option<String>, b: &Option<String>| match (a, b) {
                    (None, None) => std::cmp::Ordering::Equal,
                    (None, _) => std::cmp::Ordering::Greater,
                    (_, None) => std::cmp::Ordering::Less,
                    (Some(a), Some(b)) => a.cmp(b),
                };
                f(t1, t2).then_with(|| g(b1, b2))
            });
            for (g, ((tier, month), (count, sum, min, max))) in got.iter().zip(expect) {
                // Keys come in field-name order: born, tier.
                assert_eq!(
                    g.0,
                    vec![
                        month.as_deref().map_or(Value::Null, t),
                        tier.map_or(Value::Null, |i| t(TIERS[i]))
                    ]
                );
                assert_eq!(g.1, count);
                // Measures come in name order: max:age, min:age, sum:salary.
                assert_eq!(
                    g.2,
                    vec![
                        max.map_or(Value::Null, Value::Int),
                        min.map_or(Value::Null, Value::Int),
                        Value::Int(sum)
                    ]
                );
            }

            // Day and week buckets: the counts add up to the matches and weeks start on Monday.
            for bucket in ["day", "week", "month"] {
                let (total, got) = rows(m(vec![(
                    "aggregate",
                    agg(vec![bucketed("born", bucket)], vec![]),
                )]));
                assert_eq!(total, 300);
                assert_eq!(got.iter().map(|g| g.1).sum::<i64>(), 300);
                let mut seen = std::collections::HashSet::new();
                for g in &got {
                    assert!(seen.insert(format!("{:?}", g.0)));
                }
                let with_date = data.iter().filter(|d| d.3.is_some()).count() as i64;
                let dated: i64 = got
                    .iter()
                    .filter(|g| g.0[0] != Value::Null)
                    .map(|g| g.1)
                    .sum();
                assert_eq!(dated, with_date);
            }
            // A bool key and a plain total.
            let (total, got) = rows(m(vec![(
                "aggregate",
                agg(
                    vec![],
                    vec![measure("sum", "age"), measure("max", "salary")],
                ),
            )]));
            assert_eq!(total, 300);
            assert_eq!(got.len(), 1);
            assert_eq!(
                got[0].2[1],
                Value::Int(data.iter().filter_map(|d| d.1).sum::<i64>())
            );
        });
    }
}

#[test]
fn aggregate_follows_the_same_access_rules_as_search() {
    each_backend(|r| {
        random_parties(r, 20, 5);
        let same = |field: &str| format!("unknown field {}.{field}", r.model("party"));
        let on = |f: &str| m(vec![("aggregate", agg(vec![t(f)], vec![]))]);
        let sum = |f: &str| m(vec![("aggregate", agg(vec![], vec![measure("sum", f)]))]);
        let u = user();
        // salary needs t.hr: refused for grouping, summing and filtering alike, with the error
        // of a field that does not exist.
        assert_eq!(search_err(r, &u, on("salary")), same("salary"));
        assert_eq!(search_err(r, &u, sum("salary")), same("salary"));
        assert_eq!(search_err(r, &u, on("nonexistent")), same("nonexistent"));
        // The group holder may.
        r.ok(&hr(), "party", "search", sum("salary"));
        // A secret field is withheld from everybody.
        for who in [
            u.clone(),
            hr(),
            Principal::user("root", &["platform.admin"]),
        ] {
            assert_eq!(search_err(r, &who, on("pin")), same("pin"));
        }
        // An agent sees personal data as tokens: grouping by name would be an oracle.
        let agent = Principal::agent("bot", &[U], &[U]);
        assert_eq!(search_err(r, &agent, on("name")), same("name"));
        assert_eq!(search_err(r, &agent, on("tax_id")), same("tax_id"));
        // The agent can still group by a plain field.
        r.ok(&agent, "party", "search", on("tier"));
        // Nobody without read access to the model gets anything.
        let nobody = Principal::user("n", &[]);
        assert!(r.call(&nobody, "party", "search", on("tier")).is_err());
    });
}

#[test]
fn aggregate_input_is_validated() {
    each_backend(|r| {
        random_parties(r, 5, 3);
        let u = hr();
        let bad = |a: Value| search_err(r, &u, m(vec![("aggregate", a)]));
        assert!(bad(m(vec![])).contains("group_by or measures"));
        assert!(
            bad(agg(vec![t("tier"), t("city"), t("vip")], vec![])).contains("at most 2")
                || bad(agg(vec![t("tier"), t("city"), t("vip")], vec![])).contains("at most")
        );
        assert!(bad(agg(vec![t("born")], vec![])).contains("needs a bucket"));
        assert!(bad(agg(vec![bucketed("tier", "day")], vec![])).contains("only applies to date"));
        assert!(
            bad(agg(vec![bucketed("born", "decade")], vec![]))
                .contains("day, week, iso_week, month or year")
        );
        assert!(bad(agg(vec![t("tier"), t("tier")], vec![])).contains("twice"));
        assert!(bad(agg(vec![t("id")], vec![])).contains("id"));
        assert!(
            bad(agg(vec![], vec![measure("sum", "city")])).contains("number, money and decimal")
        );
        assert!(
            bad(agg(vec![], vec![measure("avg", "age")]))
                .contains("only decimal fields have an average")
        );
        assert!(bad(agg(vec![], vec![measure("median", "age")])).contains("unknown measure op"));
        assert!(bad(agg(vec![], vec![m(vec![("op", t("sum"))])])).contains("needs a field"));
        assert!(
            search_err(
                r,
                &u,
                m(vec![
                    ("aggregate", agg(vec![t("tier")], vec![])),
                    ("limit", Value::Int(5)),
                ])
            )
            .contains("not order, limit or offset")
        );
        // list does not aggregate.
        let e = r.err(
            &u,
            "party",
            "list",
            m(vec![("aggregate", agg(vec![t("tier")], vec![]))]),
        );
        assert!(e.contains("only available on search"), "{e}");
    });
}

#[test]
fn too_many_groups_is_an_error_not_a_silent_cut() {
    each_backend(|r| {
        for i in 0..(crate::aggregate::MAX_GROUPS + 1) {
            r.seed("party", vec![("age", Value::Int(i as i64))]);
        }
        let e = search_err(
            r,
            &user(),
            m(vec![("aggregate", agg(vec![t("age")], vec![]))]),
        );
        assert!(e.contains("more than 10000 groups"), "{e}");
        // Filtering first makes it fit.
        let (total, g) = groups_of(&r.ok(
            &user(),
            "party",
            "search",
            m(vec![
                (
                    "filter",
                    Value::List(vec![cond("age", "lt", Value::Int(50))]),
                ),
                ("aggregate", agg(vec![t("age")], vec![])),
            ]),
        ));
        assert_eq!((total, g.len()), (50, 50));
    });
}
