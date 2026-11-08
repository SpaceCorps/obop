use super::*;
use obop_registry::{Bound, CommandDef, FieldDef, Limits, Module};

fn f(name: &'static str) -> FieldDef {
    FieldDef::new(name, "d", DataClass::None, false, "t.user")
}

fn module() -> Module {
    let page = ModelDef {
        name: "page",
        description: "d",
        read_group: "t.user",
        write_group: "t.user",
        access: None,
        fields: vec![
            f("title").required(),
            f("slug"),
            f("secret_note"),
            FieldDef::new("author", "d", DataClass::Internal, true, "t.user"),
            FieldDef::new("cost", "d", DataClass::None, false, "t.manager"),
            f("state").select(&["draft", "published", "archived"]),
            f("section_id").reference("section"),
        ],
        commands: vec![CommandDef::new("publish", "d", "t.manager").transition(
            "state",
            Some("draft"),
            "published",
        )],
        ext: Default::default(),
    };
    let section = ModelDef {
        name: "section",
        description: "d",
        read_group: "t.user",
        write_group: "t.user",
        access: None,
        fields: vec![f("name"), f("visible").kind(FieldKind::Bool)],
        commands: vec![],
        ext: Default::default(),
    };
    let msg = ModelDef {
        name: "msg",
        description: "d",
        read_group: "t.user",
        write_group: "t.user",
        access: None,
        fields: vec![
            FieldDef::new("email", "d", DataClass::Internal, true, "t.user").required(),
            f("body"),
            f("flag"),
            f("qty").kind(FieldKind::Number).with_limits(Limits {
                max: Some(Bound::Int(10)),
                ..Default::default()
            }),
            f("rate").decimal(8, 2).with_limits(Limits {
                max: Some(Bound::Dec(
                    obop_values::Decimal::parse_for("5.00", 8, 2, None).unwrap(),
                )),
                ..Default::default()
            }),
            f("page_id").reference("page"),
            f("state").select(&["new", "done"]),
        ],
        commands: vec![CommandDef::new("finish", "d", "t.user").transition(
            "state",
            Some("new"),
            "done",
        )],
        ext: Default::default(),
    };
    Module {
        id: "t".into(),
        name: "T".into(),
        models: vec![page, section, msg],
        network_destinations: vec![],
        description: String::new(),
        translations: Default::default(),
        meta: Default::default(),
    }
}

fn read(fields: &[&str]) -> PublicRead {
    PublicRead {
        published_field: "state".into(),
        published_value: Some("published".into()),
        gone_value: Some("archived".into()),
        fields: fields.iter().map(|s| s.to_string()).collect(),
        order_by: Some("title".into()),
        order_desc: false,
    }
}

fn specs() -> Vec<PublicSpec> {
    vec![
        PublicSpec {
            model: "page".into(),
            read: Some(read(&["title", "slug", "section_id"])),
            submit: None,
            receipt: None,
            ..Default::default()
        },
        PublicSpec {
            model: "section".into(),
            read: Some(PublicRead {
                published_field: "visible".into(),
                published_value: None,
                gone_value: None,
                fields: vec!["name".into()],
                order_by: None,
                order_desc: false,
            }),
            submit: None,
            receipt: None,
            ..Default::default()
        },
        PublicSpec {
            model: "msg".into(),
            read: None,
            submit: Some(PublicSubmit {
                fields: vec![
                    "email".into(),
                    "body".into(),
                    "page_id".into(),
                    "qty".into(),
                    "rate".into(),
                ],
                rate_limit_per_hour: 3,
            }),
            receipt: None,
            ..Default::default()
        },
    ]
}

fn api() -> (PublicApi, Store) {
    let (d, s) = (Arc::new(Dispatcher::new()), Store::new());
    d.register_crud(&module(), &s);
    d.register_public("t", specs()).unwrap();
    (PublicApi::new(d, s.clone()), s)
}

fn rec(s: &Store, model: &str, kv: &[(&str, Value)]) -> String {
    s.insert(
        model,
        kv.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(),
    )
    .unwrap()
}

fn t(v: &str) -> Value {
    Value::text(v)
}

fn msg(kv: &[(&str, Value)]) -> Submission {
    Submission {
        model: "msg".into(),
        visitor: kv.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(),
        ..Submission::default()
    }
}

#[test]
fn the_gate_refuses_personal_restricted_and_unknown_fields() {
    let m = module();
    let page = &m.models[0];
    let check = |fields: &[&str]| {
        PublicSpec {
            model: "page".into(),
            read: Some(read(fields)),
            submit: None,
            receipt: None,
            ..Default::default()
        }
        .check(page)
    };
    assert!(check(&["title"]).is_ok());
    assert!(
        check(&["author"])
            .unwrap_err()
            .contains("class none or public_directory")
    );
    assert!(
        check(&["cost"])
            .unwrap_err()
            .contains("restricted to t.manager")
    );
    assert!(check(&["ghost"]).unwrap_err().contains("no field"));
    assert!(check(&[]).unwrap_err().contains("needs `fields`"));
    // A published flag that is really a text field is refused.
    let mut s = specs().remove(0);
    s.read.as_mut().unwrap().published_field = "title".into();
    assert!(s.check(page).unwrap_err().contains("bool or a select"));
    // The state select cannot be submitted: it is governed.
    let mut s = specs().remove(2);
    s.submit.as_mut().unwrap().fields.push("state".into());
    assert!(s.check(&m.models[2]).unwrap_err().contains("governed"));
}

#[test]
fn a_dispatcher_refuses_a_bad_declaration_at_registration() {
    let (d, s) = (Dispatcher::new(), Store::new());
    d.register_crud(&module(), &s);
    let mut bad = specs();
    bad[0].read.as_mut().unwrap().fields.push("author".into());
    assert!(d.register_public("t", bad).is_err());
    assert!(!d.has_public());
}

#[test]
fn only_published_records_and_only_whitelisted_fields_are_public() {
    let (api, s) = api();
    rec(&s, "page", &[("title", t("Draft")), ("state", t("draft"))]);
    let id = rec(
        &s,
        "page",
        &[
            ("title", t("Hello")),
            ("slug", t("hello")),
            ("secret_note", t("internal")),
            ("author", t("Ada")),
            ("cost", t("9")),
            ("state", t("published")),
        ],
    );
    let (rows, total) = api.list("page", &[], None, 0).unwrap();
    assert_eq!((rows.len(), total), (1, 1));
    assert_eq!(rows[0].id, id);
    let keys: Vec<_> = rows[0].fields.keys().cloned().collect();
    assert_eq!(keys, ["slug", "title"], "no secret_note, author or cost");
    assert!(api.list("msg", &[], None, 0).is_err(), "not readable");
    assert!(api.list("party", &[], None, 0).is_err());
}

#[test]
fn unpublished_and_nonexistent_look_the_same_and_archived_is_gone() {
    let (api, s) = api();
    let draft = rec(
        &s,
        "page",
        &[("title", t("D")), ("slug", t("d")), ("state", t("draft"))],
    );
    let old = rec(
        &s,
        "page",
        &[
            ("title", t("O")),
            ("slug", t("o")),
            ("state", t("archived")),
        ],
    );
    assert_eq!(
        api.get("page", &draft).unwrap(),
        api.get("page", "nope").unwrap()
    );
    assert_eq!(api.get("page", &draft).unwrap(), Lookup::NotFound);
    assert_eq!(api.find_by("page", "slug", "d").unwrap(), Lookup::NotFound);
    assert_eq!(api.get("page", &old).unwrap(), Lookup::Gone);
    assert_eq!(api.find_by("page", "slug", "o").unwrap(), Lookup::Gone);
    // A field that is not public cannot be searched by.
    assert!(api.find_by("page", "secret_note", "x").is_err());
    assert!(
        api.list(
            "page",
            &[("author".into(), PublicFilter::Eq(t("Ada")))],
            None,
            0
        )
        .is_err()
    );
}

#[test]
fn a_ref_is_embedded_only_when_the_target_is_public_and_published() {
    let (api, s) = api();
    let shown = rec(
        &s,
        "section",
        &[("name", t("News")), ("visible", Value::Bool(true))],
    );
    let hidden = rec(
        &s,
        "section",
        &[("name", t("Secret")), ("visible", Value::Bool(false))],
    );
    rec(
        &s,
        "page",
        &[
            ("title", t("A")),
            ("section_id", t(&shown)),
            ("state", t("published")),
        ],
    );
    rec(
        &s,
        "page",
        &[
            ("title", t("B")),
            ("section_id", t(&hidden)),
            ("state", t("published")),
        ],
    );
    let (rows, _) = api.list("page", &[], None, 0).unwrap();
    assert_eq!(rows[0].fields["section_id"]["name"], "News");
    assert!(
        !rows[1].fields.contains_key("section_id"),
        "unpublished target is left out"
    );
}

#[test]
fn a_submission_takes_only_declared_fields_and_enforces_the_model() {
    let (api, s) = api();
    // Mass assignment: a field outside submit_fields, an unknown field, the governed state.
    for bad in ["flag", "state", "ghost", "id"] {
        let e = api.submit_batch(bad, vec![msg(&[("email", t("a@b.test")), (bad, t("x"))])]);
        assert!(matches!(e, Err(SubmitError::Rejected(_))), "{bad}: {e:?}");
    }
    // The required email is still required (D47).
    let e = api.submit_batch("r1", vec![msg(&[("body", t("hi"))])]);
    assert!(
        matches!(&e, Err(SubmitError::Rejected(m)) if m.contains("required")),
        "{e:?}"
    );
    // A model that is not submittable.
    let e = api.submit_batch(
        "ip",
        vec![Submission {
            model: "page".into(),
            ..Submission::default()
        }],
    );
    assert_eq!(e, Err(SubmitError::NotPublic));
    assert_eq!(s.count("msg"), 0);
    let ids = api
        .submit_batch("ip2", vec![msg(&[("email", t("a@b.test"))])])
        .unwrap();
    // The governed state started at its first option.
    assert_eq!(s.field("msg", &ids[0], "state"), Some(t("new")));
}

#[test]
fn a_visitor_cannot_reference_an_unpublished_record() {
    let (api, s) = api();
    let draft = rec(&s, "page", &[("title", t("D")), ("state", t("draft"))]);
    let live = rec(&s, "page", &[("title", t("L")), ("state", t("published"))]);
    let sub = |page: &str| msg(&[("email", t("a@b.test")), ("page_id", t(page))]);
    for bad in [draft.as_str(), "no-such-id"] {
        let e = api.submit_batch("ip", vec![sub(bad)]);
        assert!(
            matches!(&e, Err(SubmitError::Rejected(m)) if m.contains("not available")),
            "{e:?}"
        );
    }
    assert!(api.submit_batch("ip", vec![sub(&live)]).is_ok());
}

#[test]
fn rate_limits_apply_per_source_and_a_failed_batch_leaves_nothing() {
    let (api, s) = api();
    let one = || msg(&[("email", t("a@b.test"))]);
    for _ in 0..3 {
        api.submit_batch("same", vec![one()]).unwrap();
    }
    let e = api.submit_batch("same", vec![one()]);
    assert!(
        matches!(e, Err(SubmitError::RateLimited { retry_after_secs }) if retry_after_secs > 0)
    );
    assert!(api.submit_batch("other", vec![one()]).is_ok());
    assert_eq!(s.count("msg"), 4);
    // The second record of a batch is invalid: the first is not kept.
    assert!(
        api.submit_batch("third", vec![one(), msg(&[("body", t("no email"))])])
            .is_err()
    );
    assert_eq!(s.count("msg"), 4);
}

#[test]
fn server_fields_cannot_be_overridden_by_the_visitor() {
    let (api, s) = api();
    let mut sub = msg(&[("email", t("a@b.test"))]);
    sub.server.insert("email".into(), t("forged@x.test"));
    assert!(matches!(
        api.submit_batch("ip", vec![sub]),
        Err(SubmitError::Rejected(_))
    ));
    let mut sub = msg(&[("email", t("a@b.test"))]);
    sub.server.insert("body".into(), t("server wrote this"));
    let ids = api.submit_batch("ip", vec![sub]).unwrap();
    assert_eq!(
        s.field("msg", &ids[0], "body"),
        Some(t("server wrote this"))
    );
    // A link to a later (or missing) record of the batch is refused.
    let mut sub = msg(&[("email", t("a@b.test"))]);
    sub.links.push(("page_id".into(), 3));
    assert!(api.submit_batch("ip", vec![sub]).is_err());
}

#[test]
fn the_anonymous_principal_satisfies_no_group() {
    let p = Principal::public();
    assert!(!p.allows("t.user") && !p.allows(crate::ADMIN_GROUP));
    let (d, s) = (Dispatcher::new(), Store::new());
    d.register_crud(&module(), &s);
    let r = d.call(
        crate::Surface::Rest,
        &p,
        &mut crate::Vault::new(),
        "page.list",
        &Value::Map(Default::default()),
    );
    assert!(matches!(r, Err(crate::CallError::Forbidden(_))));
}

#[test]
fn uninstalling_a_module_withdraws_its_public_declarations() {
    let (api, _) = api();
    assert!(api.is_readable("page"));
    api.d.unregister_module("t");
    assert!(!api.is_readable("page"));
    assert!(!api.d.has_public());
}

#[test]
fn the_server_may_read_one_internal_field_of_a_published_record_only() {
    let (api, s) = api();
    let live = rec(
        &s,
        "page",
        &[
            ("title", t("L")),
            ("secret_note", t("n")),
            ("author", t("Ada")),
            ("state", t("published")),
        ],
    );
    let draft = rec(
        &s,
        "page",
        &[
            ("title", t("D")),
            ("secret_note", t("n")),
            ("state", t("draft")),
        ],
    );
    assert_eq!(
        api.published_server_field("page", &live, "secret_note"),
        Ok(Some(J::String("n".into())))
    );
    assert_eq!(
        api.published_server_field("page", &draft, "secret_note"),
        Ok(None)
    );
    assert!(
        api.published_server_field("page", &live, "author").is_err(),
        "personal"
    );
    assert!(
        api.published_server_field("page", &live, "cost").is_err(),
        "restricted"
    );
}

#[test]
fn site_settings_reads_only_the_site_object() {
    let (api, s) = api();
    assert!(api.site_settings().is_empty());
    let value = r##"{"theme":{"accent":"#fff"},"site":{"title":"Shop"}}"##;
    s.backend
        .insert(
            "obop_setting",
            "instance",
            &[("value".to_string(), J::String(value.into()))]
                .into_iter()
                .collect(),
        )
        .unwrap();
    let site = api.site_settings();
    assert_eq!(site["title"], "Shop");
    assert!(!site.contains_key("theme"));
}

#[test]
fn a_visitor_cannot_send_a_text_longer_than_the_field_allows() {
    // D90: the anonymous submit path checks what the form of a visitor sends like any other write.
    let (api, s) = api();
    let default = obop_registry::DEFAULT_TEXT_MAX as usize;
    let ok = msg(&[("email", t("a@b.test")), ("body", t(&"ä".repeat(default)))]);
    api.submit_batch("lim1", vec![ok]).unwrap();
    assert_eq!(s.count("msg"), 1);

    let long = msg(&[
        ("email", t("a@b.test")),
        ("body", t(&"ä".repeat(default + 1))),
    ]);
    let e = api.submit_batch("lim2", vec![long]);
    let want = format!(
        "msg.body must be at most {default} characters; this has {}",
        default + 1
    );
    assert!(
        matches!(&e, Err(SubmitError::Rejected(m)) if *m == want),
        "{e:?}"
    );
    // The batch is all or nothing: a good submission next to the long one is not kept either.
    let good = msg(&[("email", t("b@b.test"))]);
    let long = msg(&[
        ("email", t("a@b.test")),
        ("body", t(&"x".repeat(default + 1))),
    ]);
    assert!(api.submit_batch("lim3", vec![good, long]).is_err());
    assert_eq!(s.count("msg"), 1);
}

#[test]
fn a_visitor_cannot_send_a_number_or_a_decimal_past_its_bound() {
    // D90 (review of PR 33, F10): the anonymous path holds `number` and `decimal` to `max` as it
    // holds a text to its length, and states what the visitor sent (the visitor typed it).
    let (api, s) = api();
    let rejected = |label: &str, kv: &[(&str, Value)]| {
        let mut all = vec![("email", t("a@b.test"))];
        all.extend_from_slice(kv);
        match api.submit_batch(label, vec![msg(&all)]) {
            Err(SubmitError::Rejected(m)) => m,
            other => panic!("{label}: wanted a rejection, got {other:?}"),
        }
    };
    assert_eq!(
        rejected("n1", &[("qty", Value::Int(11))]),
        "msg.qty must be at most 10; this is 11"
    );
    assert_eq!(
        rejected("d1", &[("rate", t("5.01"))]),
        "msg.rate must be at most 5.00; this is 5.01"
    );
    assert_eq!(s.count("msg"), 0);
    // The bound itself is inside it.
    let ok = msg(&[
        ("email", t("a@b.test")),
        ("qty", Value::Int(10)),
        ("rate", t("5")),
    ]);
    api.submit_batch("n2", vec![ok]).unwrap();
    assert_eq!(s.count("msg"), 1);
}

/// The instance brand reaches the public site through this door and no other (docs/spec/branding.md,
/// section 4 and 5, row OT-04): `site_settings` reads the reader of the dispatcher, fills the keys
/// the stored object leaves unset, and hands the artwork over through one reserved key that it
/// strips from the stored object first.
mod brand_door {
    use super::*;
    use crate::brand::Reader;
    use crate::brand::tests::{M_ARTWORK, Spy, put_row, put_settings};
    use serde_json::json;
    use std::time::Duration;

    struct Rig {
        api: PublicApi,
        spy: Arc<Spy>,
    }

    /// A door over a store behind the counting double, with a reader of this bound on the
    /// dispatcher (a zero bound makes every call a flight, so the reads count the calls).
    fn rig(bound: Duration) -> Rig {
        let spy = Spy::memory();
        let store = Store::over(spy.clone());
        let d = Arc::new(Dispatcher::new());
        let reader = Arc::new(Reader::with_bound(spy.clone(), bound));
        assert!(d.brand.set(reader).is_ok(), "the dispatcher had no reader");
        Rig {
            api: PublicApi::new(d, store),
            spy,
        }
    }

    /// The brand row: the name MOHV and the given other text fields.
    fn put(spy: &Arc<Spy>, extra: &[(&str, &str)]) {
        let mut pairs: Vec<(&str, J)> = vec![("name", J::String("MOHV".into()))];
        pairs.extend(extra.iter().map(|(k, v)| (*k, J::String(v.to_string()))));
        put_row(&**spy, &pairs);
    }

    #[test]
    fn with_no_brand_the_door_answers_what_is_stored_and_nothing_else() {
        let r = rig(Duration::ZERO);
        assert!(r.api.site_settings().is_empty());
        let site = json!({"title": "Café Nord", "accent": "red", "n": 3});
        put_settings(
            &*r.spy,
            json!({"theme": {"accent": "#c2410c"}, "site": site}),
        );
        assert_eq!(
            J::Object(r.api.site_settings()),
            site,
            "the theme is not the shop's, and nothing is filled"
        );
        // A dispatcher that never ran `register_admin` has an inert reader: the same answer.
        let (api, s) = api();
        assert!(api.site_settings().is_empty());
        s.backend
            .insert(
                "obop_setting",
                "instance",
                &[(
                    "value".to_string(),
                    J::String(json!({ "site": site }).to_string()),
                )]
                .into_iter()
                .collect(),
            )
            .unwrap();
        assert_eq!(J::Object(api.site_settings()), site);
    }

    #[test]
    fn with_a_brand_the_door_fills_the_unset_keys_and_hands_over_the_artwork() {
        let r = rig(Duration::ZERO);
        put(&r.spy, &[("logo_svg", M_ARTWORK)]);
        let site = r.api.site_settings();
        let v = site[crate::SITE_FAVICON_KEY]["v"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(v.len(), 16);
        assert_eq!(site["title"], "MOHV");
        assert_eq!(site["logo_url"], format!("/api/brand/logo.svg?v={v}"));
        assert_eq!(site["accent"], "#495670");
        assert_eq!(site[crate::SITE_FAVICON_KEY]["svg"], M_ARTWORK);
        // The theme's accent is the shop's accent; an explicit key wins over both.
        put_settings(
            &*r.spy,
            json!({"theme": {"accent": "#c2410c"}, "site": {"title": "Café Nord"}}),
        );
        let site = r.api.site_settings();
        assert_eq!(site["title"], "Café Nord");
        assert_eq!(site["accent"], "#c2410c");
        put_settings(&*r.spy, json!({"site": {"accent": "#e11d48"}}));
        assert_eq!(r.api.site_settings()["accent"], "#e11d48");
    }

    #[test]
    fn a_key_with_the_reserved_name_written_to_the_settings_reaches_nothing() {
        let hostile = json!({"svg": "<svg onload=\"x\"></svg>", "v": "0123456789abcdef"});
        let stored = json!({"site": {crate::SITE_FAVICON_KEY: hostile, "title": "Café Nord"}});
        // No brand.
        let r = rig(Duration::ZERO);
        put_settings(&*r.spy, stored.clone());
        let site = r.api.site_settings();
        assert!(!site.contains_key(crate::SITE_FAVICON_KEY), "{site:?}");
        assert_eq!(site["title"], "Café Nord");
        // A brand with no artwork: the shop keeps its own tile, so there is no key at all.
        put(&r.spy, &[]);
        let site = r.api.site_settings();
        assert!(!site.contains_key(crate::SITE_FAVICON_KEY), "{site:?}");
        // A brand with an artwork: the key is the reader's, never the stored one.
        put(&r.spy, &[("logo_svg", M_ARTWORK)]);
        let site = r.api.site_settings();
        assert_eq!(site[crate::SITE_FAVICON_KEY]["svg"], M_ARTWORK);
        assert_ne!(site[crate::SITE_FAVICON_KEY]["v"], "0123456789abcdef");
        assert!(!J::Object(site).to_string().contains("onload"));
    }

    #[test]
    fn one_call_asks_the_reader_once() {
        let r = rig(Duration::ZERO);
        put(&r.spy, &[]);
        for calls in 1..=3 {
            r.api.site_settings();
            // A reader of zero bound reads on every call, so its reads are its calls: the row once,
            // and the theme setting once (the door's own read of the setting is one more).
            assert_eq!(r.spy.gets("instance_brand"), calls, "{calls} calls");
            assert_eq!(r.spy.gets("obop_setting"), 2 * calls, "{calls} calls");
        }
        // With the reader's bound the calls within it share one flight.
        let r = rig(Duration::from_secs(3600));
        put(&r.spy, &[]);
        for _ in 0..5 {
            r.api.site_settings();
        }
        assert_eq!(r.spy.gets("instance_brand"), 1);
    }

    /// D74 shape (section 4, "The display read and D74"): a reader that cannot read serves the last
    /// good brand, and the default only in a process that never had a good read; a failed refill is
    /// not kept; the door never fails, panics or blocks on it.
    #[test]
    fn a_failing_read_serves_the_last_good_brand_and_in_a_new_process_the_default() {
        let r = rig(Duration::ZERO);
        put(&r.spy, &[("logo_svg", M_ARTWORK)]);
        let good = r.api.site_settings();
        assert_eq!(good["title"], "MOHV");

        r.spy.fail("instance_brand", "storage_busy");
        for _ in 0..2 {
            let during = r.api.site_settings();
            assert_eq!(during, good, "the last good brand, of any age");
        }
        r.spy.heal();
        put_row(&*r.spy, &[("name", J::String("Other".into()))]);
        assert_eq!(
            r.api.site_settings()["title"],
            "Other",
            "the next call reads again: the failure was not kept"
        );

        // The theme setting failing fails the refill as a whole, and is served the same way.
        r.spy.fail("obop_setting", "storage_busy");
        let during = r.api.site_settings();
        assert_eq!(during["title"], "Other");
        r.spy.heal();

        // A process that never had a good read: the default, which is today's shop.
        let fresh = rig(Duration::ZERO);
        put_settings(&*fresh.spy, json!({"site": {"tagline": "Tea"}}));
        put(&fresh.spy, &[]);
        fresh.spy.fail("instance_brand", "storage_busy");
        let during = fresh.api.site_settings();
        assert_eq!(during.get("tagline"), Some(&json!("Tea")));
        assert!(!during.contains_key("title"), "{during:?}");
        assert!(!during.contains_key("logo_url") && !during.contains_key("accent"));
        assert!(!during.contains_key(crate::SITE_FAVICON_KEY));
        // Nothing was pinned at the door: when the store answers, the brand is there. What the site
        // keeps of such an answer is another matter, `site_settings_read` and the site's cache (the
        // next test, and `tests_brand` in `obop-site`).
        fresh.spy.heal();
        assert_eq!(fresh.api.site_settings()["title"], "MOHV");
    }

    #[test]
    fn the_door_without_a_reader_never_asks_the_store_for_the_row() {
        let spy = Spy::memory();
        let api = PublicApi::new(Arc::new(Dispatcher::new()), Store::over(spy.clone()));
        put(&spy, &[]);
        assert!(api.site_settings().is_empty());
        assert_eq!(spy.gets("instance_brand"), 0);
    }

    /// What a cache may keep (`site_settings_read`): the answer says when it is a stand-in. The
    /// settings row that cannot be read is one (the owner's explicit keys are missing, so the brand's
    /// values stand where they are); the default of a reader that never had a good read is the other.
    /// The last good brand that a failing refill serves is a brand that was read, and an empty store
    /// is the answer "no brand".
    #[test]
    fn the_read_says_when_it_is_a_stand_in_for_what_the_store_holds() {
        let explicit = json!({"site": {"title": "Corner Shop", "accent": "#e11d48"}});

        // Nothing wrong: no brand, a brand, an inert reader. Not degraded.
        let nobody = rig(Duration::ZERO);
        put_settings(&*nobody.spy, explicit.clone());
        assert!(!nobody.api.site_settings_read().degraded);
        let r = rig(Duration::from_secs(3600));
        put_settings(&*r.spy, explicit.clone());
        put(&r.spy, &[]);
        let read = r.api.site_settings_read();
        assert!(!read.degraded);
        assert_eq!(read.site["title"], "Corner Shop", "an explicit key wins");
        let (api, _) = api();
        assert!(!api.site_settings_read().degraded);

        // The settings row cannot be read (the reader's own cell is fresh and reads nothing): the
        // object is the brand's values with no explicit key, and it says so.
        r.spy.fail("obop_setting", "storage_busy");
        let read = r.api.site_settings_read();
        assert!(read.degraded);
        assert_eq!(
            read.site["title"], "MOHV",
            "the stand-in, not the owner's key"
        );
        assert_eq!(read.site["accent"], "#495670");
        assert_eq!(
            r.api.site_settings(),
            read.site,
            "the plain door answers the same object"
        );
        r.spy.heal();
        let read = r.api.site_settings_read();
        assert!(!read.degraded);
        assert_eq!(read.site["title"], "Corner Shop");
        assert_eq!(read.site["accent"], "#e11d48");

        // A process that never had a good read of the brand, the store failing: the default stands
        // for a brand that may be set, and the object says so. The explicit keys are all there.
        let fresh = rig(Duration::ZERO);
        put_settings(&*fresh.spy, explicit.clone());
        put(&fresh.spy, &[]);
        fresh.spy.fail("instance_brand", "storage_busy");
        let read = fresh.api.site_settings_read();
        assert!(read.degraded);
        assert_eq!(read.site["title"], "Corner Shop");
        assert!(!read.site.contains_key("logo_url"));
        fresh.spy.heal();
        let read = fresh.api.site_settings_read();
        assert!(!read.degraded);
        assert!(read.site.contains_key("logo_url"), "the brand is back");

        // A failing refill that has a last good brand to serve serves it: a brand that was read.
        let last = rig(Duration::ZERO);
        put_settings(&*last.spy, explicit);
        put(&last.spy, &[("logo_svg", M_ARTWORK)]);
        let good = last.api.site_settings_read();
        assert!(!good.degraded);
        last.spy.fail("instance_brand", "storage_busy");
        let during = last.api.site_settings_read();
        assert!(!during.degraded);
        assert_eq!(during, good, "the last good brand, as before");

        // No brand and a failing read of the row: the reader answers "no brand", not a stand-in; the
        // settings row is what is wrong, and it is told whichever way the brand stands.
        let none = rig(Duration::ZERO);
        put_settings(&*none.spy, json!({"site": {"tagline": "Tea"}}));
        let read = none.api.site_settings_read();
        assert!(!read.degraded);
        none.spy.fail("obop_setting", "storage_busy");
        assert!(none.api.site_settings_read().degraded);
    }
}
