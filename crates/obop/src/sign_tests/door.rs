//! The signer's door against the real router: what a signer sees and what cannot be done with a
//! link that is wrong, over, changed or abused. All files are made-up PDFs.

use super::*;
use obop_commands::sign::digest_of_request;
use obop_storage::blob::BlobBytes;

const HOUR: i64 = 3_600_000_000;
const DAY: i64 = 24 * HOUR;

fn split(path: &str) -> (String, String) {
    let mut it = path.trim_start_matches("/sign/").split('/');
    (
        it.next().unwrap().to_string(),
        it.next().unwrap().to_string(),
    )
}

fn good_form<'a>(f: &'a Fx, name: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![
        ("name", name),
        ("agree", "1"),
        ("doc", &f.file),
        ("lang", "en"),
    ]
}

fn signed_count(f: &Fx) -> usize {
    f.events_of("sign_signed").len()
}

fn now_micros() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros() as i64
}

#[tokio::test]
async fn a_signer_reads_the_document_and_signs_once() {
    let (f, link_id, path, _) = sent().await;
    let from = ip(10);

    let page = f.get(&path, &from).await;
    assert_eq!(page.status, 200, "{}", page.text());
    let html = page.text();
    assert!(html.contains("Please sign the test NDA"));
    assert!(html.contains("Jane Test"));
    // The message is text, never markup.
    assert!(html.contains("&lt;b&gt;x&lt;/b&gt;") && !html.contains("<b>x</b>"));
    assert!(html.contains(&format!("{path}/document.pdf")));
    assert!(html.contains(&format!("name=\"doc\" value=\"{}\"", f.file)));
    assert!(html.contains(&f.file[..16]));

    let held = f.get(&format!("{path}/document.pdf"), &from).await;
    assert_eq!(held.status, 200);
    assert_eq!(held.h("content-type"), "application/pdf");
    assert_eq!(held.body, pdf(1));

    let done = f.post(&path, &from, &good_form(&f, "Jane Test")).await;
    assert_eq!(done.status, 303, "{}", done.text());
    assert_eq!(done.h("location"), path);

    let req = f.record("sign_request", &f.req);
    let text = |k: &str| req.get(k).and_then(J::as_str).unwrap_or("").to_string();
    assert_eq!(text("state"), "signed");
    assert_eq!(text("signed_name"), "Jane Test");
    assert_eq!(text("signer_ip"), from);
    assert_eq!(text("signer_user_agent"), "TestBrowser/1.0");
    assert_eq!(text("signature_reference"), "typed:Jane Test");
    assert!(text("consent_version").starts_with("v1-draft"));
    assert_eq!(text("document_hash"), f.file);
    assert_eq!(text("signed_on"), text("signed_at")[..10]);
    assert!(text("signed_at").ends_with('Z') && text("signed_at").contains('T'));
    let link = f.record("sign_link", &link_id);
    assert_eq!(
        link.get("used_at").and_then(J::as_str),
        Some(text("signed_at").as_str())
    );

    // One entry in the security log, and its digest is what the stored evidence gives.
    let entries = f.events_of("sign_signed");
    assert_eq!(entries.len(), 1);
    let detail = entries[0]["detail"].as_str().unwrap();
    let digest = digest_of_request(&link_id, &f.req, &req);
    assert!(detail.contains(&format!("digest {digest}")), "{detail}");
    assert!(detail.contains(&format!("document {}", f.file)), "{detail}");
    assert_eq!(entries[0]["user"], format!("sign_request:{}", f.req));

    // The same link now shows the end, and the PDF is no longer served.
    let again = f.get(&path, &from).await;
    assert_eq!(again.status, 200);
    assert!(again.text().contains(&text("signed_at")));
    assert!(!again.text().contains("<form"));
    assert_eq!(
        f.get(&format!("{path}/document.pdf"), &from).await.status,
        410
    );

    // A replay changes nothing, whatever name it carries.
    let after = f.record("sign_request", &f.req);
    let replay = f.post(&path, &from, &good_form(&f, "Somebody Else")).await;
    assert_eq!(replay.status, 303);
    assert_eq!(f.record("sign_request", &f.req), after);
    assert_eq!(signed_count(&f), 1);

    // The request, signed, is changed by an administrator only.
    let (s, _) = f
        .api(
            "mia",
            "/api/sign_request/update",
            json!({"id": f.req, "title": "Changed"}),
        )
        .await;
    assert_ne!(s, 200);
    let (s, _) = f
        .api("mia", "/api/sign_request/delete", json!({"id": f.req}))
        .await;
    assert_ne!(s, 200);
    assert_eq!(f.record("sign_request", &f.req), after);
}

#[tokio::test]
async fn every_wrong_link_is_the_same_404() {
    let (f, _, path, tok) = sent().await;
    let (rid, _) = split(&path);
    let other = f.req.replacen(&f.req[..4], "zzzz", 1);
    let upper = tok.to_uppercase();
    let bad = [
        format!("/sign/{rid}/{}", &tok[..tok.len() - 1]),
        format!("/sign/{rid}/{upper}"),
        format!("/sign/{rid}/{}", "0".repeat(tok.len())),
        format!("/sign/{other}/{tok}"),
        format!("/sign/{rid}/not-a-token"),
        format!("/sign/{rid}/{tok}0"),
        "/sign/x/y".to_string(),
    ];
    let mut bodies = std::collections::BTreeSet::new();
    for (i, p) in bad.iter().enumerate() {
        let r = f.get(p, &ip(20 + i as u8)).await;
        assert_eq!(r.status, 404, "{p}");
        bodies.insert(r.text());
        let r = f.get(&format!("{p}/document.pdf"), &ip(30 + i as u8)).await;
        assert_eq!(r.status, 404, "{p} pdf");
        let r = f
            .post(p, &ip(40 + i as u8), &good_form(&f, "Jane Test"))
            .await;
        assert_eq!(r.status, 404, "{p} post");
    }
    assert_eq!(bodies.len(), 1, "the answers differ");
    assert_eq!(f.record("sign_request", &f.req)["state"], "sent");
    // Nothing about a wrong link is written to the security log.
    assert!(f.events_of("sign_refused").is_empty());
}

#[tokio::test]
async fn a_link_that_ran_out_is_gone_and_signs_nothing() {
    let (f, link_id, path, _) = sent().await;
    let later = now_micros() + 15 * DAY;
    f.service().set_clock(Arc::new(move || later));
    let from = ip(50);
    assert_eq!(f.get(&path, &from).await.status, 410);
    assert_eq!(
        f.get(&format!("{path}/document.pdf"), &from).await.status,
        410
    );
    assert_eq!(
        f.post(&path, &from, &good_form(&f, "Jane Test"))
            .await
            .status,
        410
    );
    assert_eq!(f.record("sign_request", &f.req)["state"], "sent");
    assert!(
        f.record("sign_link", &link_id)
            .get("used_at")
            .is_none_or(J::is_null)
    );
    // The refusal is logged once, not once per try.
    assert_eq!(f.events_of("sign_refused").len(), 1);
    f.post(&path, &from, &good_form(&f, "Jane Test")).await;
    assert_eq!(f.events_of("sign_refused").len(), 1);
}

#[tokio::test]
async fn a_link_cannot_outlive_the_last_day_of_its_request() {
    let f = fixture().await;
    let tomorrow = {
        let day = (now_micros() / DAY + 1) * DAY;
        obop_values::datetime::Instant::from_micros(day)
            .unwrap()
            .canonical()[..10]
            .to_string()
    };
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/update",
            json!({"id": f.req, "expires_on": tomorrow}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/send_document",
            json!({"id": f.req}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let (id, path) = f.mint("mia", &f.req.clone()).await;
    let link = f.record("sign_link", &id);
    let end = link["expires_at"].as_str().unwrap();
    assert!(end.starts_with(&tomorrow), "{end} {tomorrow}");
    assert!(end.contains("T23:59:59"), "{end}");
    // The day after, the link is over although it was made for fourteen days.
    let after = now_micros() + 2 * DAY;
    f.service().set_clock(Arc::new(move || after));
    assert_eq!(f.get(&path, &ip(51)).await.status, 410);
}

#[tokio::test]
async fn a_revoked_link_stops_working_and_a_new_one_works() {
    let (f, link_id, path, _) = sent().await;
    let from = ip(52);
    assert_eq!(f.get(&path, &from).await.status, 200);
    // The server stamps the time whatever the caller wrote.
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_link/update",
            json!({"id": link_id, "revoked_at": "1999-01-01T00:00:00Z"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let stamped = f.record("sign_link", &link_id)["revoked_at"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(!stamped.starts_with("1999"), "{stamped}");
    assert_eq!(f.get(&path, &from).await.status, 410);
    assert_eq!(
        f.get(&format!("{path}/document.pdf"), &from).await.status,
        410
    );
    assert_eq!(
        f.post(&path, &from, &good_form(&f, "Jane Test"))
            .await
            .status,
        410
    );
    assert_eq!(f.record("sign_request", &f.req)["state"], "sent");
    // It cannot be made to work again, nor be revoked twice with another time.
    let (s, _) = f
        .api(
            "mia",
            "/api/sign_link/update",
            json!({"id": link_id, "revoked_at": null}),
        )
        .await;
    assert_ne!(s, 200);
    // A second link works.
    let (_, second) = f.mint("mia", &f.req.clone()).await;
    assert_ne!(second, path);
    assert_eq!(f.get(&second, &from).await.status, 200);
    assert_eq!(
        f.post(&second, &from, &good_form(&f, "Jane Test"))
            .await
            .status,
        303
    );
    assert_eq!(f.record("sign_request", &f.req)["state"], "signed");
}

#[tokio::test]
async fn another_link_of_a_signed_request_shows_the_end_and_signs_nothing_more() {
    let (f, _, path, _) = sent().await;
    let (second_id, second) = f.mint("mia", &f.req.clone()).await;
    assert_eq!(
        f.post(&path, &ip(56), &good_form(&f, "Jane Test"))
            .await
            .status,
        303
    );
    let signed = f.record("sign_request", &f.req);
    let page = f.get(&second, &ip(57)).await;
    assert_eq!(page.status, 200);
    assert!(!page.text().contains("<form"));
    assert!(page.text().contains(signed["signed_at"].as_str().unwrap()));
    assert_eq!(
        f.get(&format!("{second}/document.pdf"), &ip(57))
            .await
            .status,
        410
    );
    let r = f
        .post(&second, &ip(57), &good_form(&f, "Somebody Else"))
        .await;
    assert_eq!(r.status, 303);
    assert_eq!(f.record("sign_request", &f.req), signed);
    assert!(
        f.record("sign_link", &second_id)
            .get("used_at")
            .is_none_or(J::is_null)
    );
    assert_eq!(signed_count(&f), 1);
}

#[tokio::test]
async fn a_document_whose_file_was_replaced_is_not_signed() {
    let (f, link_id, path, _) = sent().await;
    let from = ip(53);
    assert_eq!(f.get(&path, &from).await.status, 200);
    // The owner attaches another file to the Document after the request went out.
    let (s, v) = f
        .upload(
            "ann",
            "documents_document.attachment",
            "application/pdf",
            pdf(2),
        )
        .await;
    assert_eq!(s, 201, "{v}");
    let newer = v["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api(
            "ann",
            "/api/documents_document/update",
            json!({"id": f.doc, "attachment": newer}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(f.get(&path, &from).await.status, 409);
    assert_eq!(
        f.get(&format!("{path}/document.pdf"), &from).await.status,
        409
    );
    let r = f.post(&path, &from, &good_form(&f, "Jane Test")).await;
    assert_eq!(r.status, 409, "{}", r.text());
    assert_eq!(f.record("sign_request", &f.req)["state"], "sent");
    assert!(
        f.record("sign_link", &link_id)
            .get("used_at")
            .is_none_or(J::is_null)
    );
    assert_eq!(signed_count(&f), 0);
    // The held copy is still what was sent.
    let held = f.record("sign_request", &f.req);
    assert_eq!(held["document_file"], f.file);
    assert_eq!(held["document_hash"], f.file);
    // A new link for the old request is refused too: the request must be sent again.
    let (s, v) = f
        .api("mia", "/api/sign_link/create", json!({"request_id": f.req}))
        .await;
    assert_ne!(s, 200, "{v}");
}

#[tokio::test]
async fn a_held_copy_that_no_longer_matches_its_hash_is_not_signed() {
    let (f, link_id, path, _) = sent().await;
    let from = ip(54);
    f.bytes
        .as_ref()
        .unwrap()
        .put(&f.file, b"%PDF-1.4\n% somebody wrote here\n")
        .unwrap();
    let r = f.post(&path, &from, &good_form(&f, "Jane Test")).await;
    assert_eq!(r.status, 409, "{}", r.text());
    assert_eq!(f.record("sign_request", &f.req)["state"], "sent");
    assert!(
        f.record("sign_link", &link_id)
            .get("used_at")
            .is_none_or(J::is_null)
    );
    assert_eq!(signed_count(&f), 0);
    assert_eq!(f.events_of("sign_refused").len(), 1);
}

#[tokio::test]
async fn a_form_made_for_another_document_is_refused() {
    let (f, _, path, _) = sent().await;
    let other = "0".repeat(64);
    let r = f
        .post(
            &path,
            &ip(55),
            &[
                ("name", "Jane Test"),
                ("agree", "1"),
                ("doc", other.as_str()),
            ],
        )
        .await;
    assert_eq!(r.status, 409);
    let r = f
        .post(&path, &ip(55), &[("name", "Jane Test"), ("agree", "1")])
        .await;
    assert_eq!(r.status, 409);
    assert_eq!(f.record("sign_request", &f.req)["state"], "sent");
}

#[tokio::test]
async fn the_twenty_first_failed_lookup_is_throttled_and_nobody_else_is() {
    let (f, _, path, tok) = sent().await;
    let (rid, _) = split(&path);
    let noisy = ip(60);
    let wrong = format!("/sign/{rid}/{}", "1".repeat(tok.len()));
    for i in 0..20 {
        assert_eq!(f.get(&wrong, &noisy).await.status, 404, "try {i}");
    }
    let r = f.get(&wrong, &noisy).await;
    assert_eq!(r.status, 429);
    assert!(r.h("retry-after").parse::<u64>().unwrap() > 0);
    // Another source is untouched, and so is the real link, also from the noisy source.
    assert_eq!(f.get(&wrong, &ip(61)).await.status, 404);
    assert_eq!(f.get(&path, &noisy).await.status, 200);
    assert_eq!(
        f.get(&format!("{path}/document.pdf"), &noisy).await.status,
        200
    );
    // The real signer from the noisy source can still sign.
    assert_eq!(
        f.post(&path, &noisy, &good_form(&f, "Jane Test"))
            .await
            .status,
        303
    );
}

#[tokio::test]
async fn a_source_may_post_only_so_often_whatever_the_link() {
    let (f, _, path_a, _) = sent().await;
    let (_, path_b) = f.mint("mia", &f.req.clone()).await;
    let from = ip(62);
    let bad = [("name", "x"), ("agree", "1"), ("doc", f.file.as_str())];
    // Ten posts that fail validation are allowed, spread over two links so that no link's own
    // limit is reached; the eleventh is throttled before it is read.
    for i in 0..10 {
        let path = if i < 6 { &path_a } else { &path_b };
        assert_eq!(f.post(path, &from, &bad).await.status, 400, "try {i}");
    }
    let r = f.post(&path_b, &from, &good_form(&f, "Jane Test")).await;
    assert_eq!(r.status, 429);
    assert!(r.h("retry-after").parse::<u64>().unwrap() > 0);
    assert_eq!(f.record("sign_request", &f.req)["state"], "sent");
    // The throttled source can still read, and another source signs.
    assert_eq!(f.get(&path_b, &from).await.status, 200);
    assert_eq!(
        f.post(&path_a, &ip(63), &good_form(&f, "Jane Test"))
            .await
            .status,
        303
    );
}

#[tokio::test]
async fn a_link_takes_ten_posts_an_hour_from_all_sources_together() {
    let (f, _, path_a, _) = sent().await;
    let (_, path_b) = f.mint("mia", &f.req.clone()).await;
    let bad = [("name", "x"), ("agree", "1"), ("doc", f.file.as_str())];
    for i in 0..10u8 {
        assert_eq!(
            f.post(&path_a, &ip(64 + i), &bad).await.status,
            400,
            "try {i}"
        );
    }
    let r = f.post(&path_a, &ip(90), &good_form(&f, "Jane Test")).await;
    assert_eq!(r.status, 429);
    assert!(r.h("retry-after").parse::<u64>().unwrap() > 0);
    assert_eq!(f.record("sign_request", &f.req)["state"], "sent");
    // Another link of the same request is not affected, and views of this one still work.
    assert_eq!(f.get(&path_a, &ip(91)).await.status, 200);
    assert_eq!(
        f.post(&path_b, &ip(90), &good_form(&f, "Jane Test"))
            .await
            .status,
        303
    );
}

#[tokio::test]
async fn a_form_that_is_not_acceptable_writes_nothing_and_comes_back_escaped() {
    let (f, link_id, path, _) = sent().await;
    let from = ip(64);
    let before = (
        f.record("sign_request", &f.req),
        f.record("sign_link", &link_id),
    );
    let hostile = "\"><script>alert(1)</script>";
    let cases: Vec<(Vec<(&str, &str)>, &str)> = vec![
        (vec![("name", ""), ("agree", "1"), ("doc", &f.file)], "name"),
        (
            vec![("name", "J"), ("agree", "1"), ("doc", &f.file)],
            "name",
        ),
        (
            vec![("name", "12345"), ("agree", "1"), ("doc", &f.file)],
            "name",
        ),
        (
            vec![("name", hostile), ("agree", "1"), ("doc", &f.file)],
            "name",
        ),
        (vec![("name", "Jane Test"), ("doc", &f.file)], "agree"),
    ];
    for (pairs, _why) in &cases {
        let r = f.post(&path, &from, pairs).await;
        assert_eq!(r.status, 400, "{pairs:?}");
        let html = r.text();
        assert!(html.contains("<form"), "the form is shown again");
        assert!(!html.contains("<script>alert"), "{html}");
    }
    let r = f
        .post(
            &path,
            &from,
            &[("name", hostile), ("agree", "1"), ("doc", &f.file)],
        )
        .await;
    assert!(r.text().contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert_eq!(f.record("sign_request", &f.req), before.0);
    assert_eq!(f.record("sign_link", &link_id), before.1);
    assert_eq!(signed_count(&f), 0);
}

#[tokio::test]
async fn a_name_and_a_user_agent_are_cleaned_before_they_are_kept() {
    let (f, _, path, _) = sent().await;
    let r = f
        .post_with(
            &path,
            &ip(65),
            &[
                ("name", "Jane\u{202e}\r\n Test\u{0}"),
                ("agree", "1"),
                ("doc", &f.file),
            ],
            &[
                ("origin", ORIGIN),
                ("user-agent", "Agent\tOne\u{202e} (made up)"),
            ],
        )
        .await;
    assert_eq!(r.status, 303, "{}", r.text());
    let req = f.record("sign_request", &f.req);
    for k in ["signed_name", "signer_user_agent", "signature_reference"] {
        let v = req[k].as_str().unwrap();
        assert!(
            !v.chars().any(|c| c.is_control() || c == '\u{202e}'),
            "{k}: {v:?}"
        );
    }
    assert!(req["signed_name"].as_str().unwrap().contains("Jane"));
    let detail = f.events_of("sign_signed")[0]["detail"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        !detail.contains('\n') && !detail.contains('\r'),
        "{detail:?}"
    );
}

#[tokio::test]
async fn the_address_is_the_one_the_proxy_reports_and_is_kept_in_one_form() {
    let (f, _, path, _) = sent().await;
    // Several entries: the nearest one counts; an IPv6 address is written in its short form.
    let r = f
        .post_with(
            &path,
            "2001:0db8:0000:0000:0000:0000:0000:0001",
            &good_form(&f, "Jane Test"),
            &[("origin", ORIGIN)],
        )
        .await;
    assert_eq!(r.status, 303, "{}", r.text());
    assert_eq!(f.record("sign_request", &f.req)["signer_ip"], "2001:db8::1");
    let detail = f.events_of("sign_signed")[0]["detail"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(detail.contains("address 2001:db8::1"), "{detail}");
    assert!(detail.contains("forwarded 2001:0db8"), "{detail}");
}

#[tokio::test]
async fn an_address_that_is_not_one_is_kept_as_unknown() {
    let (f, _, path, _) = sent().await;
    let r = f
        .post(
            &path,
            "not an address; drop table",
            &good_form(&f, "Jane Test"),
        )
        .await;
    assert_eq!(r.status, 303, "{}", r.text());
    assert_eq!(f.record("sign_request", &f.req)["signer_ip"], "unknown");
}

#[tokio::test]
async fn a_post_from_another_site_is_refused_and_a_browser_without_origin_is_let_in() {
    let (f, _, path, _) = sent().await;
    let from = ip(66);
    let form = good_form(&f, "Jane Test");
    for extra in [
        vec![("origin", "http://evil.test")],
        vec![("origin", "https://inst.test")],
        vec![],
        vec![("origin", "null")],
        vec![("origin", "null"), ("sec-fetch-site", "cross-site")],
        vec![("sec-fetch-site", "same-site")],
    ] {
        let r = f.post_with(&path, &from, &form, &extra).await;
        assert_eq!(r.status, 403, "{extra:?}");
        assert!(r.text().contains("could not be checked"));
        assert_eq!(
            f.record("sign_request", &f.req)["state"],
            "sent",
            "{extra:?}"
        );
    }
    // A browser that sends Origin: null (a privacy setting) still says it is the same site.
    let r = f
        .post_with(
            &path,
            &from,
            &form,
            &[("origin", "null"), ("sec-fetch-site", "same-origin")],
        )
        .await;
    assert_eq!(r.status, 303, "{}", r.text());
    assert_eq!(f.record("sign_request", &f.req)["state"], "signed");
}

#[tokio::test]
async fn only_a_form_post_is_read() {
    let (f, _, path, _) = sent().await;
    for ct in [
        "application/json",
        "multipart/form-data; boundary=x",
        "text/plain",
    ] {
        let r = send(
            &f.router,
            Request::post(&path)
                .header("host", HOST)
                .header("origin", ORIGIN)
                .header("content-type", ct)
                .header("x-forwarded-for", ip(67))
                .body(Body::from("name=Jane+Test&agree=1"))
                .unwrap(),
        )
        .await;
        assert_eq!(r.status, 400, "{ct}");
    }
    // A body above the limit is refused by the server before it is read.
    let big = "a".repeat(40 * 1024);
    let r = send(
        &f.router,
        Request::post(&path)
            .header("host", HOST)
            .header("origin", ORIGIN)
            .header("content-type", "application/x-www-form-urlencoded")
            .header("x-forwarded-for", ip(67))
            .body(Body::from(format!("name={big}&agree=1")))
            .unwrap(),
    )
    .await;
    assert_eq!(r.status, 413);
    assert_eq!(f.record("sign_request", &f.req)["state"], "sent");
}

#[tokio::test]
async fn the_pages_carry_a_narrow_policy_and_no_cookie() {
    let (f, _, path, _) = sent().await;
    let from = ip(68);
    let page = f.get(&path, &from).await;
    let csp = page.h("content-security-policy").to_string();
    assert!(csp.contains("default-src 'none'"), "{csp}");
    assert!(csp.contains("frame-ancestors 'none'"), "{csp}");
    assert!(
        csp.contains("frame-src 'self'") && csp.contains("object-src 'self'"),
        "{csp}"
    );
    assert!(
        csp.contains("form-action 'self'") && csp.contains("base-uri 'none'"),
        "{csp}"
    );
    assert_eq!(csp.matches("'sha256-").count(), 1, "{csp}");
    assert!(
        !csp.contains("unsafe") && !csp.contains("script-src"),
        "{csp}"
    );
    let html = page.text();
    assert!(!html.contains("<script") && !html.contains(" style=") && !html.contains("onclick"));
    assert_eq!(html.matches("<style>").count(), 1);
    assert_eq!(page.h("x-frame-options"), "DENY");
    assert_eq!(page.h("x-content-type-options"), "nosniff");
    assert!(
        page.h("cache-control").contains("no-store"),
        "{}",
        page.h("cache-control")
    );
    assert!(page.h("x-robots-tag").contains("noindex"));
    assert!(page.h("content-type").starts_with("text/html"));
    assert!(page.headers.get("set-cookie").is_none());
    // The one style the CSP allows is the one the page carries.
    let css = html
        .split("<style>")
        .nth(1)
        .unwrap()
        .split("</style>")
        .next()
        .unwrap();
    assert!(csp.contains(&sha256_b64(css)), "{csp}");

    let held = f.get(&format!("{path}/document.pdf"), &from).await;
    assert_eq!(held.status, 200);
    assert_eq!(held.h("content-security-policy"), "frame-ancestors 'self'");
    assert_eq!(held.h("x-frame-options"), "SAMEORIGIN");
    assert_eq!(held.h("x-content-type-options"), "nosniff");
    assert_eq!(held.h("cross-origin-resource-policy"), "same-origin");
    assert_eq!(held.h("referrer-policy"), "no-referrer");
    assert!(held.h("content-disposition").starts_with("inline"));
    assert!(held.h("content-disposition").contains("document.pdf"));
    assert!(held.h("cache-control").contains("no-store"));
    assert!(held.headers.get("accept-ranges").is_none());
    assert!(held.headers.get("set-cookie").is_none());

    // The refusals have the page's headers too.
    let gone = f.get("/sign/x/y", &from).await;
    assert_eq!(gone.status, 404);
    assert_eq!(gone.h("x-frame-options"), "DENY");
    assert!(gone.headers.get("set-cookie").is_none());
}

/// The `sha256-...` source of a style, as a browser computes it.
fn sha256_b64(s: &str) -> String {
    let hex = obop_commands::sign::sha256_hex(s.as_bytes());
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= c.len() {
                out.push(A[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[tokio::test]
async fn a_get_never_changes_anything() {
    let (f, link_id, path, _) = sent().await;
    let before = (
        f.record("sign_request", &f.req),
        f.record("sign_link", &link_id),
        f.events().len(),
    );
    for i in 0..5 {
        f.get(&path, &ip(70 + i)).await;
        f.get(&format!("{path}/document.pdf"), &ip(70 + i)).await;
        send(
            &f.router,
            Request::head(&path)
                .header("host", HOST)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    }
    assert_eq!(
        (
            f.record("sign_request", &f.req),
            f.record("sign_link", &link_id),
            f.events().len()
        ),
        before
    );
}

#[tokio::test]
async fn the_page_is_in_the_language_asked_for() {
    let (f, _, path, _) = sent().await;
    let en = f.get(&path, &ip(80)).await;
    let de = f
        .get_with(
            &path,
            &ip(80),
            &[("accept-language", "de-DE,de;q=0.9,en;q=0.5")],
        )
        .await;
    assert_eq!(en.status, 200);
    assert_eq!(de.status, 200);
    assert!(en.text().contains("lang=\"en\""));
    assert!(de.text().contains("lang=\"de\""));
    assert_ne!(en.text(), de.text());
    // The wording the signer agreed to is recorded with its language.
    let r = f
        .post(
            &path,
            &ip(80),
            &[
                ("name", "Jane Test"),
                ("agree", "1"),
                ("doc", &f.file),
                ("lang", "de"),
            ],
        )
        .await;
    assert_eq!(r.status, 303);
    assert!(
        f.record("sign_request", &f.req)["consent_version"]
            .as_str()
            .unwrap()
            .ends_with("/de")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn eight_signers_at_once_sign_the_request_once() {
    let (f, link_id, path, _) = sent().await;
    let mut set = tokio::task::JoinSet::new();
    for i in 0..8u8 {
        let (router, path, file) = (f.router.clone(), path.clone(), f.file.clone());
        set.spawn(async move {
            let body = format!("name=Jane+Test&agree=1&doc={file}");
            let r = send(
                &router,
                Request::post(&path)
                    .header("host", HOST)
                    .header("origin", ORIGIN)
                    .header("content-type", "application/x-www-form-urlencoded")
                    .header("x-forwarded-for", ip(100 + i))
                    .header("user-agent", format!("Racer/{i}"))
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await;
            (r.status, r.h("location").to_string())
        });
    }
    while let Some(r) = set.join_next().await {
        let (status, loc) = r.unwrap();
        // The winner and every loser that arrived after it are sent to the end page.
        assert_eq!((status, loc.as_str()), (303, path.as_str()));
    }
    let req = f.record("sign_request", &f.req);
    assert_eq!(req["state"], "signed");
    assert_eq!(signed_count(&f), 1);
    let agent = req["signer_user_agent"].as_str().unwrap();
    assert!(agent.starts_with("Racer/"), "{agent}");
    // The entry and the record are about the same attempt.
    let digest = digest_of_request(&link_id, &f.req, &req);
    assert!(
        f.events_of("sign_signed")[0]["detail"]
            .as_str()
            .unwrap()
            .contains(&digest)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_race_is_won_once_on_sqlite_and_on_postgres_too() {
    let dir = std::env::temp_dir().join(format!("obop-sign-race-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut stores: Vec<(Store, Option<obop_storage::testing::TestDb>)> = vec![(
        Store::open(&format!("sqlite://{}", dir.join("sign.db").display())).unwrap(),
        None,
    )];
    if let Some(db) = obop_storage::testing::TestDb::new() {
        stores.push((Store::open(db.url()).unwrap(), Some(db)));
    }
    for (store, scratch) in stores {
        let f = fixture_on(store, None, scratch).await;
        let (s, v) = f
            .api(
                "mia",
                "/api/sign_request/send_document",
                json!({"id": f.req}),
            )
            .await;
        assert_eq!(s, 200, "{v}");
        let (_, path) = f.mint("mia", &f.req.clone()).await;
        let mut set = tokio::task::JoinSet::new();
        for i in 0..8u8 {
            let (router, path, file) = (f.router.clone(), path.clone(), f.file.clone());
            set.spawn(async move {
                let body = format!("name=Jane+Test&agree=1&doc={file}");
                send(
                    &router,
                    Request::post(&path)
                        .header("host", HOST)
                        .header("origin", ORIGIN)
                        .header("content-type", "application/x-www-form-urlencoded")
                        .header("x-forwarded-for", ip(120 + i))
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .status
            });
        }
        while let Some(r) = set.join_next().await {
            assert_eq!(r.unwrap(), 303);
        }
        assert_eq!(f.record("sign_request", &f.req)["state"], "signed");
        assert_eq!(signed_count(&f), 1);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn without_the_package_the_door_exposes_nothing() {
    // `documents` is installed, `sign` is not: a binary that was deployed ahead of the package
    // upgrade has no links to open.
    let app = App::new();
    app.installer
        .install(&Bundled.fetch("documents").unwrap(), "marketplace", "test")
        .unwrap();
    let auth = Arc::new(Auth::with_test_params(app.store.backend()));
    let state = obop_http::HttpState::new(app.dispatcher.clone(), auth);
    let router = obop_http::router(state, None);
    let tok = "a".repeat(64);
    for p in [
        format!("/sign/abc/{tok}"),
        format!("/sign/abc/{tok}/document.pdf"),
    ] {
        let r = send(
            &router,
            Request::get(&p)
                .header("host", HOST)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(r.status, 404, "{p}");
    }
    let r = send(
        &router,
        Request::post(format!("/sign/abc/{tok}"))
            .header("host", HOST)
            .header("origin", ORIGIN)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from("name=Jane+Test&agree=1"))
            .unwrap(),
    )
    .await;
    assert_eq!(r.status, 404);
}
