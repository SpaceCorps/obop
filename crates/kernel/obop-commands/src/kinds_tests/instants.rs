//! Instants: canonical text, ordering across offsets, filters, buckets in a zone, the pinned
//! database and a fixture that replaces it.

use super::*;
use obop_values::{ZoneDb, Zones};

fn at(r: &Rig, name: &str, when: &str) -> String {
    r.create("item", vec![("name", t(name)), ("at", t(when))])
}

fn berlin() -> Zones {
    Zones::new(Arc::new(ZoneDb::bundled()), "Europe/Berlin").unwrap()
}

fn month_groups(r: &Rig, zone: Option<&str>, names: &[&str]) -> Vec<(String, i64)> {
    let mut entry = vec![("field", t("at")), ("bucket", t("month"))];
    if let Some(z) = zone {
        entry.push(("zone", t(z)));
    }
    let out = r.search(
        "item",
        vec![
            (
                "filter",
                Value::List(vec![cond(
                    "name",
                    "in",
                    Value::List(names.iter().map(|n| t(n)).collect()),
                )]),
            ),
            (
                "aggregate",
                m(vec![("group_by", Value::List(vec![m(entry)]))]),
            ),
        ],
    );
    list(get(&out, "groups"))
        .iter()
        .map(|g| (text(get(get(g, "key"), "at")), int(get(g, "count"))))
        .collect()
}

#[test]
fn instants_are_stored_as_utc_with_six_digits_and_order_by_the_instant() {
    each(|r| {
        // The hour that does not exist in Berlin round-trips, and the two instants that look
        // alike on a local clock are two instants.
        let a = at(r, "spring", "2026-03-29T01:30:00Z");
        assert_eq!(
            text(get(&r.read("item", &a), "at")),
            "2026-03-29T01:30:00.000000Z"
        );
        at(r, "autumn1", "2026-10-25T00:30:00Z");
        at(r, "autumn2", "2026-10-25T01:30:00Z");
        let two = r.names(
            vec![cond(
                "name",
                "in",
                Value::List(vec![t("autumn1"), t("autumn2")]),
            )],
            Some(Value::List(vec![m(vec![("field", t("at"))])])),
        );
        assert_eq!(two, ["autumn1", "autumn2"]);
        assert_eq!(
            r.names(vec![cond("at", "eq", t("2026-10-25T02:30:00+02:00"))], None),
            ["autumn1"],
            "equal as instants whatever the offset"
        );
        // Ordering across offsets follows the instant, not the text.
        at(r, "east", "2026-06-01T12:00:00+05:00"); // 07:00Z
        at(r, "west", "2026-06-01T09:00:00+01:00"); // 08:00Z
        at(r, "utc", "2026-06-01T07:30:00Z");
        let names = ["east", "west", "utc"];
        let mid = Value::List(names.iter().map(|n| t(n)).collect());
        let asc = r.names(
            vec![cond("name", "in", mid.clone())],
            Some(Value::List(vec![m(vec![("field", t("at"))])])),
        );
        assert_eq!(asc, ["east", "utc", "west"]);
        let desc = r.names(
            vec![cond("name", "in", mid)],
            Some(Value::List(vec![m(vec![
                ("field", t("at")),
                ("dir", t("desc")),
            ])])),
        );
        assert_eq!(desc, ["west", "utc", "east"]);
        // Range filters take any offset.
        let after = r.names(
            vec![
                cond("at", "gt", t("2026-06-01T08:30:00+01:00")),
                cond("at", "lte", t("2026-06-01T08:00:00Z")),
            ],
            None,
        );
        assert_eq!(after, ["west"], "07:30Z < 08:00Z <= 08:00Z");
        // Fractions are kept to the microsecond.
        let f = at(r, "frac", "2026-06-01T07:30:00.123456+00:00");
        assert_eq!(
            text(get(&r.read("item", &f), "at")),
            "2026-06-01T07:30:00.123456Z"
        );
        assert_eq!(
            r.names(
                vec![
                    cond("at", "gt", t("2026-06-01T07:30:00.123455Z")),
                    cond("at", "lt", t("2026-06-01T07:30:00.123457Z"))
                ],
                None
            ),
            ["frac"]
        );
        // A date is not an instant, and an instant is not a date.
        let e = r.err(
            "item",
            "search",
            m(vec![(
                "filter",
                Value::List(vec![cond("at", "eq", t("2026-03-29"))]),
            )]),
        );
        assert!(e.contains("filter on") && e.contains("RFC 3339"), "{e}");
        let e = r.err(
            "item",
            "search",
            m(vec![(
                "filter",
                Value::List(vec![cond("due", "eq", t("2026-03-29T01:30:00Z"))]),
            )]),
        );
        assert!(
            e.contains("filter on") && e.contains("calendar date"),
            "{e}"
        );
        let e = r.err(
            "item",
            "create",
            m(vec![("name", t("x")), ("due", t("2026-03-29T01:30:00Z"))]),
        );
        assert!(e.contains("due must be a real calendar date"), "{e}");
        let e = r.err(
            "item",
            "create",
            m(vec![("name", t("x")), ("at", t("2026-03-29"))]),
        );
        assert!(e.contains("at must be an RFC 3339 instant"), "{e}");
    });
}

#[test]
fn month_buckets_follow_the_wall_clock_of_the_zone() {
    each(|r| {
        at(r, "g", "2026-03-31T22:30:00Z");
        at(r, "h", "2026-10-31T23:30:00Z");
        at(r, "i", "2026-03-28T23:30:00Z");
        let names = ["g", "h", "i"];
        // In Berlin: 22:30Z on March 31 is already April 1, 23:30Z on October 31 is
        // November 1, and 23:30Z on March 28 is still March (summer time starts on the 29th).
        let s = month_groups(r, Some("Europe/Berlin"), &names);
        assert_eq!(
            s,
            [
                ("2026-03".to_string(), 1),
                ("2026-04".to_string(), 1),
                ("2026-11".to_string(), 1)
            ]
        );
        // In UTC the same instants fall in March, March and October.
        let u = month_groups(r, Some("UTC"), &names);
        assert_eq!(u, [("2026-03".to_string(), 2), ("2026-10".to_string(), 1)]);
        // With no zone named, the platform zone decides: UTC until it is set.
        assert_eq!(month_groups(r, None, &names), u);
        r.d.set_zones(berlin());
        assert_eq!(month_groups(r, None, &names), s);
        // A filter on the bucket says the same: this is the explicit date-in-zone conversion.
        let in_april = |zone: &str| {
            r.names(
                vec![
                    cond(
                        "name",
                        "in",
                        Value::List(names.iter().map(|n| t(n)).collect()),
                    ),
                    m(vec![
                        ("field", t("at")),
                        ("op", t("eq")),
                        ("bucket", t("month")),
                        ("zone", t(zone)),
                        ("value", t("2026-04")),
                    ]),
                ],
                None,
            )
        };
        assert_eq!(in_april("Europe/Berlin"), ["g"]);
        assert!(in_april("UTC").is_empty());
        let day = r.names(
            vec![m(vec![
                ("field", t("at")),
                ("op", t("eq")),
                ("bucket", t("day")),
                ("zone", t("Europe/Berlin")),
                ("value", t("2026-04-01")),
            ])],
            None,
        );
        assert_eq!(day, ["g"]);
        // Ranges of buckets compare labels, which sort in time order.
        let from_april = r.names(
            vec![
                cond(
                    "name",
                    "in",
                    Value::List(names.iter().map(|n| t(n)).collect()),
                ),
                m(vec![
                    ("field", t("at")),
                    ("op", t("gte")),
                    ("bucket", t("month")),
                    ("value", t("2026-04")),
                ]),
            ],
            None,
        );
        let mut from_april = from_april;
        from_april.sort();
        assert_eq!(from_april, ["g", "h"]);
    });
}

#[test]
fn weeks_start_on_monday_and_iso_weeks_have_their_own_label() {
    each(|r| {
        // 2026-10-25 is a Sunday; 23:30 in Berlin is 22:30Z (winter time just began).
        at(r, "sun", "2026-10-25T22:30:00Z");
        at(r, "mon", "2026-10-25T23:30:00Z"); // 00:30 on Monday in Berlin
        let bucket = |b: &str, zone: &str| -> Vec<(String, i64)> {
            let out = r.search(
                "item",
                vec![
                    (
                        "filter",
                        Value::List(vec![cond(
                            "name",
                            "in",
                            Value::List(vec![t("sun"), t("mon")]),
                        )]),
                    ),
                    (
                        "aggregate",
                        m(vec![(
                            "group_by",
                            Value::List(vec![m(vec![
                                ("field", t("at")),
                                ("bucket", t(b)),
                                ("zone", t(zone)),
                            ])]),
                        )]),
                    ),
                ],
            );
            list(get(&out, "groups"))
                .iter()
                .map(|g| (text(get(get(g, "key"), "at")), int(get(g, "count"))))
                .collect()
        };
        // Sunday 23:30 local belongs to the week that ends that Sunday.
        assert_eq!(
            bucket("iso_week", "Europe/Berlin"),
            [("2026-W43".to_string(), 1), ("2026-W44".to_string(), 1)]
        );
        assert_eq!(
            bucket("week", "Europe/Berlin"),
            [("2026-10-19".to_string(), 1), ("2026-10-26".to_string(), 1)]
        );
        assert_eq!(bucket("week", "UTC"), [("2026-10-19".to_string(), 2)]);
        assert_eq!(
            bucket("day", "Pacific/Auckland"),
            [("2026-10-26".to_string(), 2)]
        );
        assert_eq!(bucket("year", "UTC"), [("2026".to_string(), 2)]);
        // The same bucket names work on a date field.
        r.create("item", vec![("name", t("d1")), ("due", t("2026-01-01"))]);
        r.create("item", vec![("name", t("d2")), ("due", t("2024-12-30"))]);
        let out = r.search(
            "item",
            vec![
                (
                    "filter",
                    Value::List(vec![cond(
                        "name",
                        "in",
                        Value::List(vec![t("d1"), t("d2")]),
                    )]),
                ),
                (
                    "aggregate",
                    m(vec![(
                        "group_by",
                        Value::List(vec![m(vec![
                            ("field", t("due")),
                            ("bucket", t("iso_week")),
                        ])]),
                    )]),
                ),
            ],
        );
        let keys: Vec<String> = list(get(&out, "groups"))
            .iter()
            .map(|g| text(get(get(g, "key"), "due")))
            .collect();
        assert_eq!(keys, ["2025-W01", "2026-W01"]);
    });
}

#[test]
fn bucket_input_is_validated() {
    each(|r| {
        let one = |extra: Vec<(&str, Value)>| {
            let mut c = vec![("field", t("at")), ("op", t("eq")), ("value", t("2026-04"))];
            c.extend(extra);
            r.err(
                "item",
                "search",
                m(vec![("filter", Value::List(vec![m(c)]))]),
            )
        };
        assert!(one(vec![("zone", t("UTC"))]).contains("zone needs a bucket"));
        assert!(
            one(vec![("bucket", t("decade"))])
                .contains("bucket must be day, week, iso_week, month or year")
        );
        assert!(
            one(vec![("bucket", t("month")), ("zone", t("Mars/Base"))]).contains("not a time zone")
        );
        assert!(one(vec![("bucket", t("month")), ("zone", Value::Int(1))]).contains("IANA"));
        // The label must be a label of that bucket.
        let e = r.err(
            "item",
            "search",
            m(vec![(
                "filter",
                Value::List(vec![m(vec![
                    ("field", t("at")),
                    ("op", t("eq")),
                    ("bucket", t("month")),
                    ("value", t("2026-04-01")),
                ])]),
            )]),
        );
        assert!(e.contains("2026-04"), "{e}");
        for (bucket, bad) in [
            ("week", "2026-04-01"),
            ("iso_week", "2026-W54"),
            ("day", "2026-02-30"),
            ("year", "26"),
        ] {
            let e = r.err(
                "item",
                "search",
                m(vec![(
                    "filter",
                    Value::List(vec![m(vec![
                        ("field", t("at")),
                        ("op", t("eq")),
                        ("bucket", t(bucket)),
                        ("value", t(bad)),
                    ])]),
                )]),
            );
            assert!(e.contains("label"), "{bucket} {bad}: {e}");
        }
        // A date has no zone; a text field has no bucket.
        let e = r.err(
            "item",
            "search",
            m(vec![(
                "filter",
                Value::List(vec![m(vec![
                    ("field", t("due")),
                    ("op", t("eq")),
                    ("bucket", t("month")),
                    ("zone", t("UTC")),
                    ("value", t("2026-04")),
                ])]),
            )]),
        );
        assert!(e.contains("a date has no time zone"), "{e}");
        let e = r.err(
            "item",
            "search",
            m(vec![(
                "filter",
                Value::List(vec![m(vec![
                    ("field", t("name")),
                    ("op", t("eq")),
                    ("bucket", t("month")),
                    ("value", t("2026-04")),
                ])]),
            )]),
        );
        assert!(e.contains("only applies to date and datetime"), "{e}");
        // group_by with a zone on a date is refused too.
        let e = r.err(
            "item",
            "search",
            m(vec![(
                "aggregate",
                m(vec![(
                    "group_by",
                    Value::List(vec![m(vec![
                        ("field", t("due")),
                        ("bucket", t("day")),
                        ("zone", t("UTC")),
                    ])]),
                )]),
            )]),
        );
        assert!(e.contains("a date has no time zone"), "{e}");
    });
}

#[test]
fn replacing_the_tz_database_moves_a_bucket_and_the_reported_version() {
    each(|r| {
        // 22:30Z on the 29th: real rules put summer time (UTC+2) in force since 01:00Z, so this
        // is already the 30th in Berlin.
        at(r, "late", "2026-03-29T22:30:00Z");
        at(r, "april", "2026-03-31T22:30:00Z");
        let day = |zones: Zones, name: &str, bucket: &str| -> String {
            r.d.set_zones(zones);
            let out = r.search(
                "item",
                vec![
                    ("filter", Value::List(vec![cond("name", "eq", t(name))])),
                    (
                        "aggregate",
                        m(vec![(
                            "group_by",
                            Value::List(vec![m(vec![("field", t("at")), ("bucket", t(bucket))])]),
                        )]),
                    ),
                ],
            );
            text(get(get(&list(get(&out, "groups"))[0], "key"), "at"))
        };
        assert_eq!(day(berlin(), "late", "day"), "2026-03-30");
        let real_version = r.d.zones().db().version().to_string();
        assert!(real_version.starts_with("20"), "{real_version}");
        // A fixture that starts summer time one day later (the last Monday of March) keeps the
        // same instant on the 29th.
        let one_day_later = Zones::new(
            Arc::new(
                ZoneDb::fixture(
                    "2026-test-1day",
                    &[("Europe/Berlin", "CET-1CEST,M3.5.1,M10.5.0/3")],
                )
                .unwrap(),
            ),
            "Europe/Berlin",
        )
        .unwrap();
        assert_eq!(day(one_day_later, "late", "day"), "2026-03-29");
        assert_eq!(r.d.zones().db().version(), "2026-test-1day");
        // One that starts it on the first Sunday of April moves a month bucket.
        let april = || {
            Zones::new(
                Arc::new(
                    ZoneDb::fixture(
                        "2026-test-april",
                        &[("Europe/Berlin", "CET-1CEST,M4.1.0,M10.5.0/3")],
                    )
                    .unwrap(),
                ),
                "Europe/Berlin",
            )
            .unwrap()
        };
        assert_eq!(day(berlin(), "april", "month"), "2026-04");
        assert_eq!(day(april(), "april", "month"), "2026-03");
        assert_eq!(r.d.zones().db().version(), "2026-test-april");
    });
}
