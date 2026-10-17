use super::*;
use obop_registry::{CommandDef, FieldDef, Module};

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
    };
    let section = ModelDef {
        name: "section",
        description: "d",
        read_group: "t.user",
        write_group: "t.user",
        access: None,
        fields: vec![f("name"), f("visible").kind(FieldKind::Bool)],
        commands: vec![],
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
            f("page_id").reference("page"),
            f("state").select(&["new", "done"]),
        ],
        commands: vec![CommandDef::new("finish", "d", "t.user").transition(
            "state",
            Some("new"),
            "done",
        )],
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
                fields: vec!["email".into(), "body".into(), "page_id".into()],
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
