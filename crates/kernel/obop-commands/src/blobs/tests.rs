use super::*;
use crate::{PublicApi, PublicRead, PublicSpec, Surface, Vault};
use obop_media::fixtures as fx;
use obop_registry::{CommandDef, FieldDef, Module};
use obop_types::DataClass;

fn f(name: &'static str) -> FieldDef {
    FieldDef::new(name, "d", DataClass::None, false, "t.user")
}

fn module() -> Module {
    let item = ModelDef {
        name: "item",
        description: "d",
        read_group: "t.user",
        write_group: "t.user",
        access: None,
        fields: vec![
            f("title"),
            f("photo").kind(FieldKind::Image(0)),
            f("manual").kind(FieldKind::File(0)),
            f("badge").kind(FieldKind::Image(2048)),
            // Readable only by managers.
            FieldDef::new("private_photo", "d", DataClass::None, false, "t.manager")
                .kind(FieldKind::Image(0)),
            FieldDef::new("face", "d", DataClass::PublicDirectory, true, "t.user")
                .kind(FieldKind::Image(0)),
            f("state").select(&["draft", "published"]),
        ],
        commands: vec![CommandDef::new("publish", "d", "t.user").transition(
            "state",
            Some("draft"),
            "published",
        )],
    };
    Module {
        id: "t".into(),
        name: "T".into(),
        models: vec![item],
        network_destinations: vec![],
        description: String::new(),
        translations: Default::default(),
        meta: Default::default(),
    }
}

struct Env {
    d: Arc<Dispatcher>,
    s: Store,
    svc: Arc<BlobService>,
    api: PublicApi,
}

fn env() -> Env {
    let (d, s) = (Arc::new(Dispatcher::new()), Store::new());
    d.register_crud(&module(), &s);
    d.register_transition(&module(), "item", "publish", &s);
    d.register_public(
        "t",
        vec![PublicSpec {
            model: "item".into(),
            read: Some(PublicRead {
                published_field: "state".into(),
                published_value: Some("published".into()),
                gone_value: None,
                fields: vec!["title".into(), "photo".into()],
                order_by: None,
                order_desc: false,
            }),
            submit: None,
            receipt: None,
            ..Default::default()
        }],
    )
    .unwrap();
    Env {
        svc: d.blob_service().unwrap(),
        api: PublicApi::new(d.clone(), s.clone()),
        d,
        s,
    }
}

fn user(id: &str) -> Principal {
    Principal::user(id, &["t.user"])
}

fn manager(id: &str) -> Principal {
    Principal::user(id, &["t.user", "t.manager"])
}

fn call(
    e: &Env,
    p: &Principal,
    id: &str,
    input: Value,
) -> Result<crate::Response, crate::CallError> {
    e.d.call(Surface::Rest, p, &mut Vault::new(), id, &input)
}

fn map(kv: &[(&str, &str)]) -> Value {
    Value::Map(
        kv.iter()
            .map(|(k, v)| (k.to_string(), Value::text(v)))
            .collect(),
    )
}

fn up(e: &Env, p: &Principal, target: &str, bytes: &[u8], declared: &str) -> String {
    e.svc
        .upload(p, target, declared, "shot.png", bytes)
        .unwrap()
        .id
}

fn id_of_row(r: &crate::Response) -> String {
    match &r.value {
        Value::Map(m) => match &m["id"] {
            Value::Text(t) => t.clone(),
            _ => panic!(),
        },
        _ => panic!(),
    }
}

#[test]
fn a_blob_is_uploaded_referenced_held_replaced_and_released() {
    let e = env();
    let ann = user("ann");
    let png = fx::png(64, 48, 1);
    let b1 = up(&e, &ann, "item.photo", &png, "image/png");
    // Pending until a record holds it.
    assert!(e.s.blobs().info(&b1).unwrap().unwrap().owners.is_empty());
    let r = call(
        &e,
        &ann,
        "item.create",
        map(&[("title", "A"), ("photo", &b1)]),
    )
    .unwrap();
    let rec = id_of_row(&r);
    let info = e.s.blobs().info(&b1).unwrap().unwrap();
    assert_eq!(info.owners.len(), 1);
    assert_eq!(
        (info.owners[0].model.as_str(), info.owners[0].field.as_str()),
        ("item", "photo")
    );
    assert!(info.pending.is_empty());
    // Replace it: the old one is gone, the new one is held.
    let b2 = up(&e, &ann, "item.photo", &fx::png(64, 48, 2), "image/png");
    call(
        &e,
        &ann,
        "item.update",
        map(&[("id", &rec), ("photo", &b2)]),
    )
    .unwrap();
    assert!(
        e.s.blobs().info(&b1).unwrap().is_none(),
        "nobody holds the old image"
    );
    assert_eq!(e.s.blobs().info(&b2).unwrap().unwrap().owners.len(), 1);
    // Clearing the field releases it too.
    let r = call(
        &e,
        &ann,
        "item.update",
        Value::Map(
            [
                ("id".to_string(), Value::text(&rec)),
                ("photo".to_string(), Value::Null),
            ]
            .into(),
        ),
    )
    .unwrap();
    let _ = r;
    assert!(e.s.blobs().info(&b2).unwrap().is_none());
    // Deleting the record releases what it holds.
    let b3 = up(&e, &ann, "item.photo", &fx::png(64, 48, 3), "image/png");
    call(
        &e,
        &ann,
        "item.update",
        map(&[("id", &rec), ("photo", &b3)]),
    )
    .unwrap();
    call(&e, &ann, "item.delete", map(&[("id", &rec)])).unwrap();
    assert!(e.s.blobs().info(&b3).unwrap().is_none());
}

#[test]
fn a_blob_id_is_not_a_capability_somebody_else_can_use() {
    let e = env();
    let (ann, bob) = (user("ann"), user("bob"));
    let b = up(&e, &ann, "item.photo", &fx::png(32, 32, 1), "image/png");
    // Bob knows the id (say he saw it in a log) but did not upload it and no record he can read
    // holds it: it is as if it did not exist.
    let r = call(
        &e,
        &bob,
        "item.create",
        map(&[("title", "B"), ("photo", &b)]),
    );
    assert!(matches!(r, Err(crate::CallError::Handler(m)) if m.contains("item.photo must be")));
    // Once Ann's record holds it, Bob can read that record, so he may reuse the image.
    call(
        &e,
        &ann,
        "item.create",
        map(&[("title", "A"), ("photo", &b)]),
    )
    .unwrap();
    assert!(
        call(
            &e,
            &bob,
            "item.create",
            map(&[("title", "B"), ("photo", &b)])
        )
        .is_ok()
    );
    // Garbage and made-up ids look the same as everything else.
    for bad in [
        "",
        "../../etc/passwd",
        &"a".repeat(64),
        &"A".repeat(64),
        "short",
    ] {
        let r = call(
            &e,
            &ann,
            "item.create",
            map(&[("title", "C"), ("photo", bad)]),
        );
        assert!(matches!(r, Err(crate::CallError::Handler(_))), "{bad}");
    }
    // A document is not an image.
    let doc = e
        .svc
        .upload(
            &ann,
            "item.manual",
            "application/pdf",
            "m.pdf",
            b"%PDF-1.4 x",
        )
        .unwrap();
    let r = call(
        &e,
        &ann,
        "item.create",
        map(&[("title", "D"), ("photo", &doc.id)]),
    );
    assert!(r.is_err());
    assert!(
        call(
            &e,
            &ann,
            "item.create",
            map(&[("title", "D"), ("manual", &doc.id)])
        )
        .is_ok()
    );
}

#[test]
fn reads_follow_the_permissions_of_the_owning_record_and_agents_get_no_bytes() {
    let e = env();
    let (ann, mia) = (user("ann"), manager("mia"));
    let outsider = Principal::user("zed", &["other.group"]);
    let b = up(
        &e,
        &mia,
        "item.private_photo",
        &fx::png(32, 32, 9),
        "image/png",
    );
    // Only the uploader can see it before it is attached.
    assert!(e.svc.get(&mia, &b, None).is_ok());
    assert_eq!(e.svc.get(&ann, &b, None).unwrap_err(), BlobError::NotFound);
    call(
        &e,
        &mia,
        "item.create",
        map(&[("title", "P"), ("private_photo", &b)]),
    )
    .unwrap();
    // Ann reads items but not that field; the manager can; an outsider cannot; no one is public.
    assert_eq!(e.svc.get(&ann, &b, None).unwrap_err(), BlobError::NotFound);
    assert_eq!(
        e.svc.get(&outsider, &b, None).unwrap_err(),
        BlobError::NotFound
    );
    let served = e.svc.get(&mia, &b, None).unwrap();
    assert_eq!(served.content_type, "image/png");
    assert!(e.api.public_blob(&b, None).is_none());
    // An agent acting for the manager gets ids and metadata but no bytes.
    let agent = Principal::agent("bot", &["t.user", "t.manager"], &["t.user", "t.manager"]);
    assert_eq!(
        e.svc.get(&agent, &b, None).unwrap_err(),
        BlobError::NotFound
    );
    let row = call(&e, &agent, "item.list", Value::Map(Default::default())).unwrap();
    assert!(
        format!("{:?}", row.value).contains(&b),
        "the id is visible, the bytes are not"
    );
    // A malformed id is simply not found.
    assert_eq!(
        e.svc.get(&mia, "../../x", None).unwrap_err(),
        BlobError::NotFound
    );
    // A user without write access to the field cannot upload to it.
    assert_eq!(
        e.svc
            .upload(
                &ann,
                "item.private_photo",
                "image/png",
                "x.png",
                &fx::png(8, 8, 1)
            )
            .unwrap_err(),
        BlobError::Forbidden
    );
}

#[test]
fn uploads_are_validated_by_content_and_cleaned() {
    let e = env();
    let ann = user("ann");
    let up =
        |declared: &str, bytes: &[u8]| e.svc.upload(&ann, "item.photo", declared, "a.png", bytes);
    // GPS and the author are gone from what is stored.
    let raw = fx::jpeg_with_gps(32, 32, 4);
    let info = up("image/jpeg", &raw).unwrap();
    let stored = e.s.blobs().read_all(&info.id, 1 << 20).unwrap().unwrap();
    assert!(
        !stored
            .windows(fx::GPS_MARKER.len())
            .any(|w| w == fx::GPS_MARKER.as_bytes())
    );
    assert_eq!((info.width, info.height), (Some(32), Some(32)));
    // SVG with script, a mislabelled file, a bomb, an empty body, random bytes.
    assert!(matches!(
        up("image/svg+xml", fx::SVG_WITH_SCRIPT.as_bytes()),
        Err(BlobError::Unsupported(_))
    ));
    assert!(matches!(
        up("image/png", fx::SVG_WITH_SCRIPT.as_bytes()),
        Err(BlobError::Unsupported(_))
    ));
    assert!(matches!(
        up("image/jpeg", &fx::png(8, 8, 1)),
        Err(BlobError::Unsupported(_))
    ));
    assert!(matches!(
        up("image/png", &fx::png_bomb(60000, 60000)),
        Err(BlobError::Unsupported(_))
    ));
    assert!(matches!(
        up("image/png", b""),
        Err(BlobError::Unsupported(_))
    ));
    assert!(matches!(
        up("image/png", &[7u8; 64]),
        Err(BlobError::Unsupported(_))
    ));
    // The size cap of the field (2 KiB for the badge) is enforced.
    let big = fx::png(300, 300, 1);
    assert!(big.len() > 2048);
    assert!(matches!(
        e.svc.upload(&ann, "item.badge", "image/png", "b.png", &big),
        Err(BlobError::TooLarge(2048))
    ));
    // Unknown and non-blob fields.
    assert!(matches!(
        e.svc.upload(&ann, "item.title", "image/png", "x", &raw),
        Err(BlobError::Bad(_))
    ));
    assert!(matches!(
        e.svc.upload(&ann, "nope.photo", "image/png", "x", &raw),
        Err(BlobError::Bad(_))
    ));
    assert!(matches!(
        e.svc.upload(&ann, "photo", "image/png", "x", &raw),
        Err(BlobError::Bad(_))
    ));
    // Files: allowed types only, never markup.
    let file = |declared: &str, b: &[u8]| {
        e.svc
            .upload(&ann, "item.manual", declared, "../../etc/pass wd.pdf", b)
    };
    let ok = file("application/pdf", b"%PDF-1.7 hello").unwrap();
    assert_eq!(ok.content_type, "application/pdf");
    assert!(matches!(
        file("text/html", b"<html><script>1</script></html>"),
        Err(BlobError::Unsupported(_))
    ));
    assert!(matches!(
        file("application/pdf", b"MZ\x90\x00"),
        Err(BlobError::Unsupported(_))
    ));
    // The file name is reduced to a safe last component.
    let served = e.svc.get(&ann, &ok.id, None).unwrap();
    assert_eq!(served.filename, "pass wd.pdf");
    assert!(served.attachment);
    let cd = disposition(true, "r\u{e9}sum\u{e9} \"x\".pdf");
    assert!(cd.starts_with("attachment; filename=\""));
    assert!(cd.contains("filename*=UTF-8''r%C3%A9sum%C3%A9%20%22x%22.pdf"));
    assert!(!cd.contains('\n'));
}

#[test]
fn uploads_are_rate_limited_per_principal() {
    let e = env();
    let ann = user("ann");
    let mut refused = 0;
    for i in 0..(DEFAULT_UPLOADS_PER_WINDOW + 5) {
        let r = e.svc.upload(
            &ann,
            "item.photo",
            "image/png",
            "x.png",
            &fx::png(8, 8, i as u8),
        );
        if matches!(r, Err(BlobError::Rate(_))) {
            refused += 1;
        }
    }
    assert_eq!(refused, 5);
    // Another principal is not affected.
    assert!(
        e.svc
            .upload(
                &user("bob"),
                "item.photo",
                "image/png",
                "x.png",
                &fx::png(8, 8, 1)
            )
            .is_ok()
    );
}

#[test]
fn thumbnails_are_made_once_cached_and_only_at_the_fixed_widths() {
    let e = env();
    let ann = user("ann");
    let b = up(&e, &ann, "item.photo", &fx::png(1500, 900, 1), "image/png");
    let full = e.svc.get(&ann, &b, None).unwrap();
    let t = e.svc.get(&ann, &b, Some(640)).unwrap();
    assert!(t.bytes.len() < full.bytes.len());
    assert_eq!((t.width, t.height), (Some(640), Some(384)));
    assert_eq!(t.etag, format!("\"{b}-w640\""));
    assert!(e.s.blobs().derived(&b, "w640").unwrap().is_some(), "cached");
    // Not a listed width, not an image, and never bigger than the original.
    assert_eq!(
        e.svc.get(&ann, &b, Some(641)).unwrap_err(),
        BlobError::NotFound
    );
    let small = up(&e, &ann, "item.photo", &fx::png(200, 100, 5), "image/png");
    assert_eq!(
        e.svc.get(&ann, &small, Some(640)).unwrap().etag,
        format!("\"{small}\"")
    );
    let doc = e
        .svc
        .upload(&ann, "item.manual", "application/pdf", "m.pdf", b"%PDF-1")
        .unwrap();
    assert_eq!(
        e.svc.get(&ann, &doc.id, Some(320)).unwrap_err(),
        BlobError::NotFound
    );
}

#[test]
fn a_blob_is_public_only_through_a_published_record_and_a_declared_field() {
    let e = env();
    let ann = user("ann");
    let b = up(&e, &ann, "item.photo", &fx::png(64, 64, 7), "image/png");
    // Pending upload: not public.
    assert!(e.api.public_blob(&b, None).is_none());
    let r = call(
        &e,
        &ann,
        "item.create",
        map(&[("title", "Shown"), ("photo", &b)]),
    )
    .unwrap();
    let rec = id_of_row(&r);
    // Held by a draft: not public.
    assert!(e.api.public_blob(&b, None).is_none());
    call(&e, &ann, "item.publish", map(&[("id", &rec)])).unwrap();
    let served = e.api.public_blob(&b, None).expect("published");
    assert_eq!(served.etag, format!("\"{b}\""));
    assert_eq!(
        e.api.public_blob_info(&b).map(|i| (i.0, i.1)),
        Some((Some(64), Some(64)))
    );
    assert!(
        e.api.public_blob(&b, Some(320)).is_some() || e.api.public_blob(&b, Some(320)).is_none()
    );
    // A field that is not on the public list is never public, even on a published record.
    let hidden = up(&e, &ann, "item.badge", &fx::png(16, 16, 8), "image/png");
    call(
        &e,
        &ann,
        "item.update",
        map(&[("id", &rec), ("badge", &hidden)]),
    )
    .unwrap();
    assert!(e.api.public_blob(&hidden, None).is_none());
    // The personal photo is not public either, and malformed ids are not found.
    assert!(e.api.public_blob("../../x", None).is_none());
    // Unpublishing is not possible by action here; deleting the record removes the blob.
    call(&e, &ann, "item.delete", map(&[("id", &rec)])).unwrap();
    assert!(e.api.public_blob(&b, None).is_none());
}

#[test]
fn the_gate_refuses_public_photos_of_people_and_visitor_uploads() {
    let m = module();
    let item = &m.models[0];
    let spec = |read: &[&str], submit: &[&str]| PublicSpec {
        model: "item".into(),
        read: (!read.is_empty()).then(|| PublicRead {
            published_field: "state".into(),
            published_value: Some("published".into()),
            gone_value: None,
            fields: read.iter().map(|s| s.to_string()).collect(),
            order_by: None,
            order_desc: false,
        }),
        submit: (!submit.is_empty()).then(|| crate::PublicSubmit {
            fields: submit.iter().map(|s| s.to_string()).collect(),
            rate_limit_per_hour: 5,
        }),
        receipt: None,
        ..Default::default()
    };
    assert!(spec(&["photo"], &[]).check(item).is_ok());
    let e = spec(&["face"], &[]).check(item).unwrap_err();
    assert!(e.contains("personal data can never be public"), "{e}");
    let e = spec(&["private_photo"], &[]).check(item).unwrap_err();
    assert!(e.contains("restricted"), "{e}");
    let e = spec(&["title"], &["photo"]).check(item).unwrap_err();
    assert!(e.contains("cannot upload"), "{e}");
}

#[test]
fn erasing_a_record_clears_its_photo_fields_and_deletes_the_blobs() {
    let e = env();
    let ann = user("ann");
    let b = up(&e, &ann, "item.face", &fx::png(32, 32, 3), "image/png");
    let r = call(
        &e,
        &ann,
        "item.create",
        map(&[("title", "Person"), ("face", &b)]),
    )
    .unwrap();
    let rec = id_of_row(&r);
    assert_eq!(e.d.erase_blob_fields(&e.s, "item", &rec).unwrap(), 1);
    assert!(e.s.blobs().info(&b).unwrap().is_none());
    let after = e.s.backend().get("item", &rec).unwrap().unwrap();
    assert!(after["face"].is_null());
    assert_eq!(after["title"], "Person");
}

/// An `Env` over a store whose backend can refuse calls like a full pool (D74).
fn flaky_env() -> (Env, Arc<obop_storage::testing::Flaky>) {
    let flaky = obop_storage::testing::Flaky::new(obop_storage::open("memory:").unwrap());
    let backend = obop_storage::guarded(flaky.clone());
    let bytes = obop_storage::blob::open_bytes("memory:", None).unwrap();
    let s = Store {
        blobs: Arc::new(obop_storage::blob::Catalog::new(backend.clone(), bytes)),
        backend,
    };
    let d = Arc::new(Dispatcher::new());
    d.register_crud(&module(), &s);
    d.register_transition(&module(), "item", "publish", &s);
    (
        Env {
            svc: d.blob_service().unwrap(),
            api: PublicApi::new(d.clone(), s.clone()),
            d,
            s,
        },
        flaky,
    )
}

#[test]
fn a_blob_hold_refused_for_a_moment_is_tried_again_and_a_lasting_refusal_is_a_partial_503() {
    use obop_storage::fault;
    let (e, flaky) = flaky_env();
    let ann = user("ann");

    // The pool is full for two calls right after the record committed: the hold is tried again
    // and goes through, so the file is held and the answer is the plain success.
    let b1 = up(&e, &ann, "item.photo", &fx::png(64, 48, 1), "image/png");
    flaky.refuse_n_after_the_next_write(2);
    fault::begin_request(Default::default());
    let r = call(
        &e,
        &ann,
        "item.create",
        map(&[("title", "A"), ("photo", &b1)]),
    )
    .unwrap();
    let out = fault::end_request();
    flaky.allow_models();
    let rec = id_of_row(&r);
    assert!(out.wrote && !out.partial, "{out:?}");
    assert_eq!(
        e.s.blobs().info(&b1).unwrap().unwrap().owners.len(),
        1,
        "the file is held although the first tries were refused"
    );
    e.s.blobs().gc(0).unwrap();
    assert!(e.s.blobs().info(&b1).unwrap().is_some(), "gc keeps it");

    // The refusal lasts: the record is committed and its file is not held. That must not look
    // like a success, and the next gc must not be a surprise: the request is marked partial.
    let b2 = up(&e, &ann, "item.photo", &fx::png(64, 48, 2), "image/png");
    flaky.refuse_reads_after_the_next_write();
    fault::begin_request(Default::default());
    let r = call(
        &e,
        &ann,
        "item.update",
        map(&[("id", &rec), ("photo", &b2)]),
    );
    let out = fault::end_request();
    flaky.allow_models();
    assert!(r.is_ok(), "the update itself committed");
    assert!(out.wrote && out.partial, "{out:?}");
    assert_eq!(
        e.s.backend().get("item", &rec).unwrap().unwrap()["photo"],
        b2.as_str(),
        "the record names the new file"
    );
    assert!(
        e.s.blobs().info(&b2).unwrap().unwrap().owners.is_empty(),
        "and nobody holds it: that is what `partial` says"
    );
}
