//! Properties of the signer door that were true and unpinned: a test here fails when the line it
//! is about is removed (each one was checked that way). The reasons the door logs for a refusal,
//! a used link that stays dead, the name rules, the order of the limiter and the lookup, the
//! other headers that carry an address, and the one-entry-per-link-and-reason rule of the log.
//! Synthetic bytes and made-up people only.

use super::hardening::{fields, good_form, interleave, signed_count};
use super::*;
use obop_storage::blob::BlobBytes;
use std::sync::atomic::Ordering;

/// The wrong link of the same request (the right length, the wrong secret).
fn wrong_link(path: &str) -> String {
    let (head, tok) = path.rsplit_once('/').unwrap();
    format!("{head}/{}", "1".repeat(tok.len()))
}

fn reasons(f: &Fx) -> Vec<String> {
    f.events_of("sign_refused")
        .iter()
        .map(|e| e["detail"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn the_other_headers_that_carry_an_address_are_neither_the_address_nor_a_bucket() {
    // A trusted proxy means the last entry of the last `X-Forwarded-For` line and nothing else:
    // a client that sends the headers other proxies and platforms use cannot pick the address
    // the evidence records, nor the bucket its tries are counted in.
    let (f, _, path, _) = sent().await;
    let others = [
        ("x-real-ip", "203.0.113.50"),
        ("cf-connecting-ip", "203.0.113.51"),
        ("forwarded", "for=203.0.113.52"),
        ("true-client-ip", "203.0.113.53"),
        ("x-client-ip", "203.0.113.54"),
        ("fastly-client-ip", "203.0.113.55"),
        ("x-cluster-client-ip", "203.0.113.56"),
        ("x-original-forwarded-for", "203.0.113.57"),
    ];
    let mut extra = vec![("origin", ORIGIN)];
    extra.extend(others);
    let r = f
        .post_with(&path, &ip(83), &good_form(&f, "Jane Test"), &extra)
        .await;
    assert_eq!(r.status, 303, "{}", r.text());
    assert_eq!(f.record("sign_request", &f.req)["signer_ip"], ip(83));
    let entry = f.events_of("sign_signed").remove(0);
    let detail = entry["detail"].as_str().unwrap();
    assert!(detail.contains(&format!("address {}", ip(83))), "{detail}");
    assert!(
        !detail.contains("203.0.113.5"),
        "the log shows none of the other headers: {detail}"
    );

    // The count of failed lookups is the proxy's address's, whatever the others say: the 21st try
    // from one address is throttled although every try names another `X-Real-IP`.
    let wrong = wrong_link(&path);
    let noisy = ip(84);
    for n in 0..20u8 {
        let by = format!("203.0.114.{n}");
        let h = [
            ("x-real-ip", by.as_str()),
            ("cf-connecting-ip", by.as_str()),
        ];
        assert_eq!(f.get_with(&wrong, &noisy, &h).await.status, 404, "{n}");
    }
    let h = [
        ("x-real-ip", "203.0.114.200"),
        ("forwarded", "for=203.0.114.201"),
    ];
    assert_eq!(f.get_with(&wrong, &noisy, &h).await.status, 429);
}

#[tokio::test]
async fn every_reason_the_door_logs_is_the_reason_that_refused() {
    // One link, one change to the store, the refusal that follows and the reason it names. The
    // page answers 409 (it reads ids and sizes), so the line is the only place the cause is told.
    let reason_of = |f: &Fx| -> Vec<String> { reasons(f) };

    // no_copy: the held copy is another file than the one that was hashed.
    let (f, _, path, _) = sent().await;
    let (s, v) = f
        .upload(
            "admin",
            "sign_request.document_file",
            "application/pdf",
            pdf(3),
        )
        .await;
    assert_eq!(s, 201, "{v}");
    let other = v["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api(
            "admin",
            "/api/sign_request/update",
            json!({"id": f.req, "document_file": other}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(f.get(&path, &ip(70)).await.status, 409);
    let got = reason_of(&f);
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(got[0].contains("reason no_copy;"), "{got:?}");

    // copy_missing, at the page: the held copy is not a PDF.
    let (f, _, path, _) = sent().await;
    let (s, v) = f
        .upload(
            "admin",
            "sign_request.document_file",
            "text/plain",
            b"not a pdf".to_vec(),
        )
        .await;
    assert_eq!(s, 201, "{v}");
    let text_file = v["id"].as_str().unwrap().to_string();
    for k in ["document_hash", "document_file"] {
        let (s, v) = f
            .api(
                "admin",
                "/api/sign_request/update",
                json!({"id": f.req, k: text_file}),
            )
            .await;
        assert_eq!(s, 200, "{k}: {v}");
    }
    assert_eq!(f.get(&path, &ip(71)).await.status, 409);
    let got = reason_of(&f);
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(got[0].contains("reason copy_missing;"), "{got:?}");

    // copy_missing, at the PDF: the page cannot tell (it reads no bytes), the viewer's fetch can.
    let (f, _, path, _) = sent().await;
    f.bytes.as_ref().unwrap().delete(&f.file).unwrap();
    assert_eq!(f.get(&path, &ip(72)).await.status, 200);
    assert!(reasons(&f).is_empty());
    let r = f.get(&format!("{path}/document.pdf"), &ip(72)).await;
    assert_eq!(r.status, 409, "{}", r.text());
    let got = reason_of(&f);
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(got[0].contains("reason copy_missing;"), "{got:?}");

    // document_gone: the Document is no longer there (a write to the store; no command deletes
    // a Document that a request refers to).
    let (f, _, path, _) = sent().await;
    f.app
        .store
        .backend()
        .delete("documents_document", &f.doc)
        .unwrap();
    assert_eq!(f.get(&path, &ip(73)).await.status, 409);
    let got = reason_of(&f);
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(got[0].contains("reason document_gone;"), "{got:?}");

    // changed: the Document's file was replaced after the request was sent.
    let (f, _, path, _) = sent().await;
    f.app
        .store
        .backend()
        .update(
            "documents_document",
            &f.doc,
            &fields(json!({"attachment": "0".repeat(64)})),
        )
        .unwrap();
    assert_eq!(f.get(&path, &ip(74)).await.status, 409);
    let got = reason_of(&f);
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(got[0].contains("reason changed;"), "{got:?}");

    // stale_form: the page the signer read was for another file than the one the request holds.
    let (f, _, path, _) = sent().await;
    let r = f
        .post(
            &path,
            &ip(75),
            &[
                ("name", "Jane Test"),
                ("agree", "1"),
                ("doc", &"f".repeat(64)),
                ("lang", "en"),
            ],
        )
        .await;
    assert_eq!(r.status, 409, "{}", r.text());
    let got = reason_of(&f);
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(got[0].contains("reason stale_form;"), "{got:?}");

    // gone_post: a post on a link whose request was closed.
    let (f, _, path, _) = sent().await;
    let (s, v) = f
        .api("mia", "/api/sign_request/decline", json!({"id": f.req}))
        .await;
    assert_eq!(s, 200, "{v}");
    let r = f.post(&path, &ip(76), &good_form(&f, "Jane Test")).await;
    assert_eq!(r.status, 410, "{}", r.text());
    let got = reason_of(&f);
    assert_eq!(got.len(), 1, "{got:?}");
    assert!(got[0].contains("reason gone_post;"), "{got:?}");

    // None of them wrote a signature.
    assert_eq!(signed_count(&f), 0);
}

#[tokio::test]
async fn the_log_has_one_entry_per_link_and_reason_in_ten_minutes() {
    let (f, _, path, _) = sent().await;
    // (A link cannot be made once the copy no longer matches its hash, so the second is made now.)
    let (_, other) = f.mint("mia", &f.req.clone()).await;
    let stale = [
        ("name", "Jane Test"),
        ("agree", "1"),
        (
            "doc",
            "e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0",
        ),
        ("lang", "en"),
    ];
    // The same reason twice on one link: one entry.
    for n in 0..2u8 {
        assert_eq!(f.post(&path, &ip(60 + n), &stale).await.status, 409);
    }
    assert_eq!(reasons(&f).len(), 1, "{:?}", reasons(&f));
    // Another reason on the same link is another entry (the key holds the reason).
    f.bytes
        .as_ref()
        .unwrap()
        .put(&f.file, b"%PDF-1.4\n% somebody wrote here\n")
        .unwrap();
    assert_eq!(
        f.post(&path, &ip(62), &good_form(&f, "Jane Test"))
            .await
            .status,
        409
    );
    let got = reasons(&f);
    assert_eq!(got.len(), 2, "{got:?}");
    assert!(got[0].contains("reason stale_form;"), "{got:?}");
    assert!(got[1].contains("reason tampered;"), "{got:?}");
    // The same reason on another link is another entry (the key holds the link).
    assert_eq!(f.post(&other, &ip(63), &stale).await.status, 409);
    let got = reasons(&f);
    assert_eq!(got.len(), 3, "{got:?}");
    assert!(got[2].contains("reason stale_form;"), "{got:?}");
    // And neither link's id is the other's in the lines.
    let id12 = |d: &str| d.split(';').next().unwrap().to_string();
    assert_ne!(id12(&got[0]), id12(&got[2]), "{got:?}");
}

#[tokio::test]
async fn a_used_link_stays_dead_after_an_administrator_reopens_the_request() {
    let (f, link_id, path, _) = sent().await;
    let r = f.post(&path, &ip(50), &good_form(&f, "Jane Test")).await;
    assert_eq!(r.status, 303, "{}", r.text());
    assert!(f.record("sign_link", &link_id)["used_at"].is_string());
    // Reopening a signed request means editing the store: no command moves a request out of
    // `signed` (docs/ISSUES.md O21). The link was used; that stays true.
    f.app
        .store
        .backend()
        .update("sign_request", &f.req, &fields(json!({"state": "sent"})))
        .unwrap();
    assert_eq!(f.get(&path, &ip(51)).await.status, 410, "the page");
    assert_eq!(
        f.get(&format!("{path}/document.pdf"), &ip(51)).await.status,
        410,
        "the viewer's copy"
    );
    let r = f.post(&path, &ip(52), &good_form(&f, "Jane Again")).await;
    assert_eq!(r.status, 410, "{}", r.text());
    assert_eq!(f.record("sign_request", &f.req)["state"], "sent");
    assert_eq!(
        f.record("sign_request", &f.req)["signed_name"],
        "Jane Test",
        "nothing of the second post was written"
    );
    assert_eq!(signed_count(&f), 1);
}

#[tokio::test]
async fn a_link_used_after_the_door_read_it_is_not_signed_a_second_time() {
    // The batch's own check, apart from the one the door makes before it: between the reads and
    // the write the link becomes used (the request is still `sent`, as after a reopening), and
    // the write must be refused although the door had found the link usable.
    let (f, link_id, path, _) = sent().await;
    let b = f.app.store.backend();
    let id = link_id.clone();
    let fired = interleave(&f, move || {
        b.update(
            "sign_link",
            &id,
            &fields(json!({"used_at": "2026-11-03T10:00:00.000000Z"})),
        )
        .unwrap();
    });
    let r = f.post(&path, &ip(53), &good_form(&f, "Jane Test")).await;
    assert!(
        fired.load(Ordering::SeqCst),
        "the change landed between the reads and the write"
    );
    assert_eq!(r.status, 409, "{}", r.text());
    let req = f.record("sign_request", &f.req);
    assert_eq!(req["state"], "sent");
    assert!(req.get("signed_name").is_none_or(J::is_null));
    assert_eq!(signed_count(&f), 0);
}

#[tokio::test]
async fn a_name_needs_two_letters_and_at_most_a_hundred_and_twenty_characters() {
    // (name, signs). Each case has an instance of its own: a signature is made once.
    let long = "A".repeat(120);
    let too_long = "A".repeat(121);
    let cases: [(&str, bool); 9] = [
        ("J.", false),
        ("J", false),
        (".-", false),
        ("1234", false),
        ("Jo", true),
        ("Ng", true),
        (&long, true),
        (&too_long, false),
        ("", false),
    ];
    for (name, signs) in cases {
        let (f, _, path, _) = sent().await;
        let r = f.post(&path, &ip(40), &good_form(&f, name)).await;
        let want = if signs { 303 } else { 400 };
        assert_eq!(r.status, want, "{name:?}: {}", r.text());
        assert_eq!(
            f.record("sign_request", &f.req)["state"],
            if signs { "signed" } else { "sent" },
            "{name:?}"
        );
        assert_eq!(signed_count(&f), usize::from(signs), "{name:?}");
    }
}

#[tokio::test]
async fn the_limit_on_the_pdf_is_counted_before_the_copy_is_hashed() {
    // A tampered copy is refused with 409 and costs a hash of up to 25 MiB; the limiter must
    // count each try before that, or a source could ask for the hash without end.
    let (f, _, path, _) = sent().await;
    f.bytes
        .as_ref()
        .unwrap()
        .put(&f.file, b"%PDF-1.4\n% somebody wrote here\n")
        .unwrap();
    let pdf_path = format!("{path}/document.pdf");
    let from = ip(41);
    for n in 0..120 {
        assert_eq!(f.get(&pdf_path, &from).await.status, 409, "try {n}");
    }
    let r = f.get(&pdf_path, &from).await;
    assert_eq!(r.status, 429, "the 121st try");
    assert!(r.h("retry-after").parse::<u64>().unwrap() > 0);
    assert_eq!(
        f.get(&pdf_path, &ip(42)).await.status,
        409,
        "another source is not held up"
    );
}

#[tokio::test]
async fn wrong_links_do_not_use_up_the_views_of_the_right_one() {
    // The count of views is made after the lookup: a source that asks for links that do not
    // exist is throttled by the count of failed lookups (20), and its 120 views are for links
    // that do. Were the count made first, 120 wrong tries would leave the right link at 429.
    let (f, _, path, _) = sent().await;
    let wrong = wrong_link(&path);
    let wrong_pdf = format!("{wrong}/document.pdf");
    let noisy = ip(43);
    for n in 0..150 {
        let url = if n % 2 == 0 { &wrong } else { &wrong_pdf };
        let s = f.get(url, &noisy).await.status;
        assert_eq!(s, if n < 20 { 404 } else { 429 }, "try {n}");
    }
    assert_eq!(f.get(&path, &noisy).await.status, 200, "the page");
    assert_eq!(
        f.get(&format!("{path}/document.pdf"), &noisy).await.status,
        200,
        "the viewer's copy"
    );
}
