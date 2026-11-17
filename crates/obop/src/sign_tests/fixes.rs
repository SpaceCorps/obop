//! The fixes of 2026-11-17 (docs/ISSUES.md O4, O5, O20, O21, O22, O24, O29): who sees a request,
//! what an approval is of, the end of a link against the clock and the zone, the lock a signature
//! shares with the staff writes, and the https origin of the door.

use super::hardening::{agent_for, err, good_form};
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn now_micros() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros() as i64
}

/// The ids of the rows of a list answer.
fn ids(v: &J) -> Vec<String> {
    v["value"]
        .as_array()
        .or_else(|| v["value"]["rows"].as_array())
        .unwrap_or_else(|| panic!("{v}"))
        .iter()
        .map(|r| r["id"].as_str().unwrap_or("").to_string())
        .collect()
}

// ---- O4, O20: a request about a Document is seen by those who see the Document ---------------

#[tokio::test]
async fn a_request_about_a_restricted_document_is_not_there_for_who_cannot_see_the_document() {
    let (f, link_id, _, _) = sent().await;
    // A request made from a template, about no Document: every reader of requests sees it.
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_template/create",
            json!({"name": "Open (test)", "body": "text"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let tpl = v["value"]["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/create",
            json!({"title": "By template", "template_id": tpl, "signer_name": "Jane Test"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let open = v["value"]["id"].as_str().unwrap().to_string();

    // sam (sign.user) and noc (sign.manager) hold no role that shows ann's restricted Document:
    // the request about it, its id and its hash, and its link, are not there for them.
    for who in ["sam", "noc"] {
        let (s, v) = f
            .api(who, "/api/sign_request/get", json!({"id": f.req}))
            .await;
        assert_eq!(s, 400, "{who}: {v}");
        assert!(err(&v).contains("no sign_request"), "{who}: {v}");
        let (s, v) = f.api(who, "/api/sign_request/list", json!({})).await;
        assert_eq!(s, 200, "{who}: {v}");
        assert_eq!(ids(&v), vec![open.clone()], "{who}: only the open request");
        let (s, v) = f
            .api(
                who,
                "/api/sign_request/search",
                json!({"filter": [{"field": "document_id", "op": "eq", "value": f.doc}]}),
            )
            .await;
        assert_eq!(s, 200, "{who}: {v}");
        assert!(ids(&v).is_empty(), "{who}: {v}");
        let (s, v) = f.api(who, "/api/sign_link/list", json!({})).await;
        assert_eq!(s, 200, "{who}: {v}");
        assert!(ids(&v).is_empty(), "{who}: {v}");
    }
    // Those who see the Document see the request and its link: its owner, a documents manager,
    // an administrator.
    for who in ["ann", "mia", "dan", "admin"] {
        let (s, v) = f
            .api(who, "/api/sign_request/get", json!({"id": f.req}))
            .await;
        assert_eq!(s, 200, "{who}: {v}");
        let (s, v) = f.api(who, "/api/sign_request/list", json!({})).await;
        assert_eq!(s, 200, "{who}: {v}");
        let mut got = ids(&v);
        got.sort();
        let mut want = vec![open.clone(), f.req.clone()];
        want.sort();
        assert_eq!(got, want, "{who}");
    }
    let (s, v) = f.api("mia", "/api/sign_link/list", json!({})).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(ids(&v), vec![link_id]);

    // Nobody makes a request about a Document they cannot see.
    let (s, v) = f
        .api(
            "sam",
            "/api/sign_request/create",
            json!({"title": "Probe", "signer_name": "Jane Test", "document_id": f.doc}),
        )
        .await;
    assert_ne!(s, 200, "{v}");
}

/// An internal Document is open to every holder of `documents.user`, not to everyone: a request
/// about it is not there for sam (sign.user) or noc (sign.manager), who hold no role in
/// Documents, and neither are its id, its hash or its link (the review of 2026-11-17 found them
/// listed, with the hash, and the link revocable).
#[tokio::test]
async fn a_request_about_an_internal_document_is_not_there_without_a_role_in_documents() {
    let f = fixture().await;
    let (s, v) = f
        .api(
            "ann",
            "/api/documents_document/create",
            json!({"title": "Internal memo (test)", "kind": "contract", "confidentiality": "internal",
                   "state": "draft", "owner": "ann", "attachment": f.file}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let doc = v["value"]["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/create",
            json!({"title": "Please sign the memo", "signer_name": "Jane Test", "document_id": doc}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let req = v["value"]["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api("mia", "/api/sign_request/send_document", json!({"id": req}))
        .await;
    assert_eq!(s, 200, "{v}");
    let (link, _) = f.mint("mia", &req).await;
    for who in ["sam", "noc"] {
        let (s, v) = f
            .api(who, "/api/sign_request/get", json!({"id": req}))
            .await;
        assert_eq!(s, 400, "{who}: {v}");
        assert!(err(&v).contains("no sign_request"), "{who}: {v}");
        let (s, v) = f.api(who, "/api/sign_request/list", json!({})).await;
        assert_eq!(s, 200, "{who}: {v}");
        assert!(!ids(&v).contains(&req), "{who}: {v}");
        let (s, v) = f.api(who, "/api/sign_link/list", json!({})).await;
        assert_eq!(s, 200, "{who}: {v}");
        assert!(ids(&v).is_empty(), "{who}: {v}");
        let (s, v) = f
            .api(
                who,
                "/api/sign_link/update",
                json!({"id": link, "revoked_at": "2026-11-17T10:00:00Z"}),
            )
            .await;
        assert_ne!(s, 200, "{who}: {v}");
        // Nor can they make a request about it.
        let (s, v) = f
            .api(
                who,
                "/api/sign_request/create",
                json!({"title": "Probe", "signer_name": "Jane Test", "document_id": doc}),
            )
            .await;
        assert_ne!(s, 200, "{who}: {v}");
    }
    // A holder of documents.user who is not its owner sees it: bob (internal is open to all of
    // Documents).
    let (s, v) = f
        .api("bob", "/api/sign_request/get", json!({"id": req}))
        .await;
    assert_eq!(s, 200, "{v}");
    assert!(
        f.record("sign_link", &link)
            .get("revoked_at")
            .is_none_or(|v| v.is_null() || v == "")
    );
}

// ---- O19: a request that was sent is a manager's to delete ------------------------------------

#[tokio::test]
async fn a_request_that_was_sent_is_deleted_by_a_manager_only() {
    let f = fixture().await;
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_template/create",
            json!({"name": "Open (test)", "body": "text"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let tpl = v["value"]["id"].as_str().unwrap().to_string();
    let mut made = Vec::new();
    for _ in 0..2 {
        let (s, v) = f
            .api(
                "sam",
                "/api/sign_request/create",
                json!({"title": "By template", "template_id": tpl, "signer_name": "Jane Test", "signer_email": "jane@example.test"}),
            )
            .await;
        assert_eq!(s, 200, "{v}");
        made.push(v["value"]["id"].as_str().unwrap().to_string());
    }
    // A draft is still the maker's to delete.
    let (s, v) = f
        .api("sam", "/api/sign_request/delete", json!({"id": made[0]}))
        .await;
    assert_eq!(s, 200, "{v}");
    let (s, v) = f
        .api("sam", "/api/sign_request/send", json!({"id": made[1]}))
        .await;
    assert_eq!(s, 200, "{v}");
    let (s, v) = f
        .api("sam", "/api/sign_request/delete", json!({"id": made[1]}))
        .await;
    assert_eq!(s, 400, "{v}");
    assert!(err(&v).contains("sign.manager"), "{v}");
    assert_eq!(f.record("sign_request", &made[1])["state"], "sent");
    let (s, v) = f
        .api("noc", "/api/sign_request/delete", json!({"id": made[1]}))
        .await;
    assert_eq!(s, 200, "{v}");
}

// ---- O24, O5: an approval is of the approver and of the bytes ----------------------------------

async fn propose(f: &Fx) -> String {
    let r = send(
        &f.router,
        Request::post("/api/sign_request/send_document")
            .header("content-type", "application/json")
            .header("host", HOST)
            .header("authorization", format!("Bearer {}", f.tok("agent")))
            .body(Body::from(json!({"id": f.req}).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(r.status, 202, "{}", r.text());
    r.json()["plan_id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn a_wrong_approver_does_not_spend_the_plan_of_an_agent() {
    let mut f = fixture().await;
    agent_for(&mut f, "mia").await;
    let plan = propose(&f).await;
    // bob holds the groups but cannot see ann's restricted Document: refused, nothing held.
    let (s, v) = f
        .api("bob", &format!("/api/approve/{plan}"), json!({}))
        .await;
    assert_ne!(s, 200, "{v}");
    let rec = f.record("sign_request", &f.req);
    assert_eq!(rec["state"], "draft");
    assert!(rec.get("document_file").is_none_or(J::is_null));
    // The same plan is still there for the right approver (it was spent by the refusal before).
    let (s, v) = f
        .api("mia", &format!("/api/approve/{plan}"), json!({}))
        .await;
    assert_eq!(s, 200, "{v}");
    let rec = f.record("sign_request", &f.req);
    assert_eq!(rec["state"], "sent");
    assert_eq!(rec["document_hash"], f.file.as_str());
}

#[tokio::test]
async fn an_approval_is_of_the_file_that_was_there_when_the_send_was_proposed() {
    let mut f = fixture().await;
    agent_for(&mut f, "mia").await;
    let plan = propose(&f).await;
    // ann replaces the file of her Document after the proposal.
    let (s, v) = f
        .upload(
            "ann",
            "documents_document.attachment",
            "application/pdf",
            pdf(9),
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
    // The approval is refused: the person would approve bytes the agent did not propose.
    for _ in 0..2 {
        let (s, v) = f
            .api("mia", &format!("/api/approve/{plan}"), json!({}))
            .await;
        assert_eq!(s, 400, "{v}");
        assert!(
            err(&v).contains("changed after the send was proposed"),
            "{v}"
        );
    }
    let rec = f.record("sign_request", &f.req);
    assert_eq!(rec["state"], "draft");
    assert!(rec.get("document_hash").is_none_or(J::is_null));
    // A new proposal is of the new file, and a person sends that one.
    let plan = propose(&f).await;
    let (s, v) = f
        .api("mia", &format!("/api/approve/{plan}"), json!({}))
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(
        f.record("sign_request", &f.req)["document_hash"],
        newer.as_str()
    );
}

// ---- O22: the end of a link, the clock and the zone --------------------------------------------

#[tokio::test]
async fn a_signature_never_carries_a_time_after_the_end_of_its_link() {
    let (f, link_id, path, _) = sent().await;
    let end = obop_values::datetime::Instant::parse(
        f.record("sign_link", &link_id)["expires_at"]
            .as_str()
            .unwrap(),
    )
    .unwrap()
    .micros();
    // The first reading of the clock (the status of the link) is just within the end; the second,
    // taken after the held copy was read and hashed, is past it.
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    f.service().set_clock(Arc::new(move || {
        if c.fetch_add(1, Ordering::SeqCst) == 0 {
            end - 1
        } else {
            end + 1
        }
    }));
    let r = f.post(&path, &ip(171), &good_form(&f, "Jane Test")).await;
    assert!(calls.load(Ordering::SeqCst) >= 2);
    assert_eq!(r.status, 410, "{}", r.text());
    let rec = f.record("sign_request", &f.req);
    assert_eq!(rec["state"], "sent");
    assert!(rec.get("signed_at").is_none_or(J::is_null));
    assert!(f.events_of("sign_signed").is_empty());
}

#[tokio::test]
async fn a_clock_before_1970_has_ended_every_link() {
    let (f, _, path, _) = sent().await;
    assert_eq!(f.get(&path, &ip(172)).await.status, 200);
    // The system clock answers 0 for a time before 1970.
    f.service().set_clock(Arc::new(|| 0));
    assert_eq!(f.get(&path, &ip(172)).await.status, 410);
    let r = f.post(&path, &ip(172), &good_form(&f, "Jane Test")).await;
    assert_eq!(r.status, 410, "{}", r.text());
    assert_eq!(f.record("sign_request", &f.req)["state"], "sent");
}

#[tokio::test]
async fn the_days_of_a_request_are_days_of_the_instance_zone() {
    let f = fixture().await;
    let zones = obop_values::Zones::new(
        Arc::new(obop_values::zone::ZoneDb::bundled()),
        "Europe/Berlin",
    )
    .unwrap();
    f.app.dispatcher.set_zones(zones);
    let at = |s: &str| obop_values::datetime::Instant::parse(s).unwrap().micros();
    // 2030-06-14 23:30 UTC is 2030-06-15 01:30 in Berlin (summer time, UTC+2).
    let t = at("2030-06-14T23:30:00Z");
    f.service().set_clock(Arc::new(move || t));
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/update",
            json!({"id": f.req, "expires_on": "2030-06-16"}),
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
    assert_eq!(f.record("sign_request", &f.req)["sent_on"], "2030-06-15");
    let (link_id, path) = f.mint("mia", &f.req.clone()).await;
    // The last day ends at midnight in Berlin, 22:00 UTC, not at midnight UTC.
    let expires = f.record("sign_link", &link_id)["expires_at"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(expires.starts_with("2030-06-16T21:59:59"), "{expires}");
    let r = f.post(&path, &ip(173), &good_form(&f, "Jane Test")).await;
    assert_eq!(r.status, 303, "{}", r.text());
    let rec = f.record("sign_request", &f.req);
    assert_eq!(rec["signed_on"], "2030-06-15");
    assert!(
        rec["signed_at"]
            .as_str()
            .unwrap()
            .starts_with("2030-06-14T23:30"),
        "{rec:?}"
    );
}

// ---- O21: the door and a staff write do not interleave ----------------------------------------

#[tokio::test]
async fn a_decline_that_read_the_request_before_the_signature_cannot_write_over_it() {
    // A staff `decline` holds the dispatcher's write lock from its read to its write. The door's
    // batch commits under the same lock, so it waits; the decline lands first, and the door's own
    // condition (`state` is `sent`) then refuses the signature. Before, the batch committed while
    // the decline was between its read and its write, and the decline wrote `declined` over a
    // request that held the evidence.
    let (f, _, path, _) = sent().await;
    let b = f.app.store.backend();
    let rid = f.req.clone();
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = done.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let base = now_micros();
    let worker: Arc<std::sync::Mutex<Option<std::thread::JoinHandle<()>>>> = Default::default();
    let w = worker.clone();
    f.service().set_clock(Arc::new(move || {
        // The second reading is the time of the signature, after the door's reads.
        if calls.fetch_add(1, Ordering::SeqCst) + 1 == 2 {
            let b = b.clone();
            let rid = rid.clone();
            let flag = flag.clone();
            let guard = b.lock("dispatcher.txn").unwrap();
            *w.lock().unwrap() = Some(std::thread::spawn(move || {
                let _guard = guard;
                std::thread::sleep(std::time::Duration::from_millis(300));
                b.update(
                    "sign_request",
                    &rid,
                    &super::hardening::fields(json!({"state": "declined"})),
                )
                .unwrap();
                flag.store(true, Ordering::SeqCst);
            }));
        }
        base
    }));
    let r = f.post(&path, &ip(174), &good_form(&f, "Jane Test")).await;
    if let Some(h) = worker.lock().unwrap().take() {
        h.join().unwrap();
    }
    assert!(done.load(Ordering::SeqCst), "the decline ran");
    let rec = f.record("sign_request", &f.req);
    assert_eq!(rec["state"], "declined");
    assert!(
        rec.get("signed_at").is_none_or(J::is_null),
        "a declined request holds no signature: {rec:?}"
    );
    assert!(f.events_of("sign_signed").is_empty());
    assert_ne!(r.status, 303, "{}", r.text());
}

// ---- O29: the door behind a TLS proxy ----------------------------------------------------------

#[tokio::test]
async fn a_post_from_the_https_page_is_accepted_with_the_public_url_or_the_forwarded_scheme() {
    // With OBOP_PUBLIC_URL set to the https address the proxy serves.
    let (f, _, path, _) = sent().await;
    let cfg = obop_http::mcp::McpConfig {
        public_url: Some("https://inst.test".into()),
        ..Default::default()
    };
    let state = obop_http::HttpState::with_mcp(f.app.dispatcher.clone(), f.auth.clone(), cfg);
    state.set_trust_proxy(true);
    let router = obop_http::router(state, None);
    let body = good_form(&f, "Jane Test")
        .iter()
        .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
        .collect::<Vec<_>>()
        .join("&");
    let post = |origin: &str, extra: Option<(&str, &str)>| {
        let mut b = Request::post(path.as_str())
            .header("host", HOST)
            .header("origin", origin)
            .header("content-type", "application/x-www-form-urlencoded")
            .header("x-forwarded-for", ip(175))
            .header("user-agent", "TestBrowser/1.0");
        if let Some((k, v)) = extra {
            b = b.header(k, v);
        }
        b.body(Body::from(body.clone())).unwrap()
    };
    // The http origin is not this instance's any more; the https one is.
    let r = send(&router, post(ORIGIN, None)).await;
    assert_eq!(r.status, 403, "{}", r.text());
    let r = send(&router, post("https://inst.test", None)).await;
    assert_eq!(r.status, 303, "{}", r.text());
    assert_eq!(f.record("sign_request", &f.req)["state"], "signed");

    // Without the setting, behind a proxy that says the scheme it was reached by.
    let (f, _, path, _) = sent().await;
    let r = f
        .post_with(
            &path,
            &ip(176),
            &good_form(&f, "Jane Test"),
            &[("origin", "https://inst.test")],
        )
        .await;
    assert_eq!(r.status, 403, "without the forwarded scheme: {}", r.text());
    let r = f
        .post_with(
            &path,
            &ip(176),
            &good_form(&f, "Jane Test"),
            &[
                ("origin", "https://inst.test"),
                ("x-forwarded-proto", "https"),
            ],
        )
        .await;
    assert_eq!(r.status, 303, "{}", r.text());
    assert_eq!(f.record("sign_request", &f.req)["state"], "signed");
}
