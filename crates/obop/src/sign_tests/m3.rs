//! The fixes of stream m3 (docs/ISSUES.md O5, O19, O29, O31): a revoke by an administrator in the
//! security log, the start-up note about the public address, and the tests of O31 that were not
//! written.

use super::*;

// ---- O19: a revoke by an administrator is logged too -----------------------------------------

#[tokio::test]
async fn a_revoke_by_an_administrator_is_written_to_the_security_log() {
    let (f, link_id, path, _) = sent().await;
    let before = f.events_of("sign_link_revoked").len();
    let (s, v) = f
        .api(
            "admin",
            "/api/sign_link/update",
            json!({"id": link_id, "revoked_at": "2030-01-01T00:00:00Z"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(f.get(&path, &ip(91)).await.status, 410, "the link is dead");
    let logged = f.events_of("sign_link_revoked");
    assert_eq!(logged.len(), before + 1, "{logged:?}");
    assert_eq!(logged[before]["actor"], "admin");
    // Sending the same stamp back is not a second revoke, and writes no second line.
    let stored = f.record("sign_link", &link_id)["revoked_at"].clone();
    let (s, v) = f
        .api(
            "admin",
            "/api/sign_link/update",
            json!({"id": link_id, "revoked_at": stored}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(f.events_of("sign_link_revoked").len(), before + 1);
    // Emptying it brings the link back, and that is logged as well.
    assert!(f.events_of("sign_link_restored").is_empty());
    let (s, v) = f
        .api(
            "admin",
            "/api/sign_link/update",
            json!({"id": link_id, "revoked_at": null}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(
        f.get(&path, &ip(92)).await.status,
        200,
        "the link works again"
    );
    let restored = f.events_of("sign_link_restored");
    assert_eq!(restored.len(), 1, "{restored:?}");
    assert_eq!(restored[0]["actor"], "admin");
    assert!(
        restored[0]["detail"]
            .as_str()
            .unwrap()
            .ends_with("by an administrator"),
        "{restored:?}"
    );
    assert_eq!(f.events_of("sign_link_revoked").len(), before + 1);
}

// ---- O29: the server says at start when the signer door has no public address ----------------

#[tokio::test]
async fn the_start_up_note_names_obop_public_url_while_sign_is_installed_and_it_is_unset() {
    let f = fixture().await;
    let svc = f.app.dispatcher.sign_service().expect("sign is installed");
    svc.set_public_url(None);
    let note = svc.start_up_note().expect("a note without OBOP_PUBLIC_URL");
    assert!(note.contains("OBOP_PUBLIC_URL"), "{note}");
    assert!(note.contains("X-Forwarded-Proto"), "{note}");
    svc.set_public_url(Some("https://sign.example.test"));
    assert_eq!(svc.start_up_note(), None);
    // Without the package nothing is said.
    f.app.installer.uninstall("sign").unwrap();
    f.app.dispatcher.sync_installed_modules();
    svc.set_public_url(None);
    assert_eq!(svc.start_up_note(), None);
}

// ---- O31: tests that were not written ------------------------------------------------------

/// A link is a secret that a person hands over: a token that narrows a person's groups (a scoped
/// token, a mobile app's token) does not make one, whatever groups it holds.
#[tokio::test]
async fn a_scoped_token_and_a_mobile_token_do_not_create_a_signing_link() {
    use obop_commands::{Principal, Surface, Value, Vault};
    let f = fixture().await;
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/send_document",
            json!({"id": f.req}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let groups = [
        "sign.user",
        "sign.manager",
        "documents.user",
        "documents.manager",
    ];
    let mut scoped = Principal::user("mia", &groups);
    scoped.scopes = Some(groups.iter().map(|g| g.to_string()).collect());
    let mut mobile = scoped.clone();
    mobile.device = Some("device-1".into());
    let before = f.app.store.backend().count("sign_link").unwrap();
    for (what, p) in [("scoped", &scoped), ("mobile", &mobile)] {
        assert!(p.allows("sign.manager"), "{what}");
        let e = f
            .app
            .dispatcher
            .call(
                Surface::Rest,
                p,
                &mut Vault::new(),
                "sign_link.create",
                &Value::map([("request_id", Value::text(&f.req))]),
            )
            .unwrap_err();
        assert!(format!("{e:?}").contains("ask a person"), "{what}: {e:?}");
    }
    assert_eq!(f.app.store.backend().count("sign_link").unwrap(), before);
    assert!(f.events_of("sign_link_issued").is_empty());
}

/// Two links of one request, each posted at once with another name: one signature wins, it is
/// the one of the link that was used, and the log entry vouches for what is stored.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_links_with_two_names_at_once_sign_the_request_once_under_one_of_them() {
    let (f, first_id, first_path, _) = sent().await;
    let (second_id, second_path) = f.mint("mia", &f.req.clone()).await;
    let mut set = tokio::task::JoinSet::new();
    for i in 0..8u8 {
        let (path, name) = if i % 2 == 0 {
            (first_path.clone(), "Jane+Test")
        } else {
            (second_path.clone(), "John+Other")
        };
        let (router, file) = (f.router.clone(), f.file.clone());
        set.spawn(async move {
            let body = format!("name={name}&agree=1&doc={file}");
            let r = send(
                &router,
                Request::post(&path)
                    .header("host", HOST)
                    .header("origin", ORIGIN)
                    .header("content-type", "application/x-www-form-urlencoded")
                    .header("x-forwarded-for", ip(160 + i))
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await;
            (path, r.status)
        });
    }
    let mut answers = Vec::new();
    while let Some(r) = set.join_next().await {
        answers.push(r.unwrap());
    }
    let req = f.record("sign_request", &f.req);
    assert_eq!(req["state"], "signed");
    assert_eq!(f.events_of("sign_signed").len(), 1);
    let used: Vec<&str> = [first_id.as_str(), second_id.as_str()]
        .into_iter()
        .filter(|id| {
            f.record("sign_link", id)
                .get("used_at")
                .is_some_and(|v| !v.is_null())
        })
        .collect();
    assert_eq!(used.len(), 1, "exactly one link was used");
    let (won, won_path, name) = if used[0] == first_id {
        (first_id.as_str(), first_path.as_str(), "Jane Test")
    } else {
        (second_id.as_str(), second_path.as_str(), "John Other")
    };
    assert_eq!(req["signed_name"], name, "the name of the link that won");
    let digest = obop_commands::sign::digest_of_request(won, &f.req, &req);
    let detail = f.events_of("sign_signed")[0]["detail"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(detail.contains(&digest), "{detail}");
    for (path, status) in answers {
        if path == won_path {
            assert_eq!(
                status, 303,
                "a post through the link that won ends on its page"
            );
        } else {
            // A post that read the request after the signature sees the end, as a second link of a
            // signed request does (303 to the page that says it is signed); one that raced it is
            // refused. Never a server error, and never a second signature.
            assert!(
                status == 303 || (400..500).contains(&status),
                "a post through the other link: {status}"
            );
        }
    }
}

// ---- O3, O10, O19: the reader of the sign_signed entry -----------------------------------------

/// `evidence_check` of `sign_request.get`, as `who` reads it.
async fn check_of(f: &Fx, who: &str) -> J {
    let (s, v) = f
        .api(who, "/api/sign_request/get", json!({"id": f.req}))
        .await;
    assert_eq!(s, 200, "{v}");
    v["value"].get("evidence_check").cloned().unwrap_or(J::Null)
}

#[tokio::test]
async fn a_signed_request_says_whether_its_evidence_still_equals_the_log_entry() {
    let (f, _, path, _) = sent().await;
    // Not signed yet: nothing to compare.
    assert_eq!(check_of(&f, "mia").await, J::Null);
    let r = f
        .post(
            &path,
            &ip(171),
            &[("name", "Jane Test"), ("agree", "1"), ("doc", &f.file)],
        )
        .await;
    assert_eq!(r.status, 303, "{}", r.text());
    // The entry vouches for the evidence and, from 0.3.0, for what the signer read.
    let detail = f.events_of("sign_signed")[0]["detail"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(detail.contains("; wording "), "{detail}");
    assert_eq!(check_of(&f, "mia").await, "matches");
    assert_eq!(check_of(&f, "admin").await, "matches");
    // Never stored, and nobody sets it: it is the server's.
    assert!(
        f.record("sign_request", &f.req)
            .get("evidence_check")
            .is_none()
    );
    for body in [
        json!({"id": f.req, "evidence_check": "matches"}),
        json!({"title": "x", "signer_name": "Jane Test", "evidence_check": "matches"}),
    ] {
        let cmd = if body.get("id").is_some() {
            "/api/sign_request/update"
        } else {
            "/api/sign_request/create"
        };
        let (s, v) = f.api("admin", cmd, body).await;
        assert_ne!(s, 200, "{cmd}: {v}");
    }
    // An administrator's edit of the evidence shows.
    let (s, v) = f
        .api(
            "admin",
            "/api/sign_request/update",
            json!({"id": f.req, "signed_name": "Someone Else"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(check_of(&f, "mia").await, "differs");
    let (s, v) = f
        .api(
            "admin",
            "/api/sign_request/update",
            json!({"id": f.req, "signed_name": "Jane Test"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(check_of(&f, "mia").await, "matches", "back as it was");
    // So does an edit of what the signer read (O19): the title, the message, the requested name.
    for (k, v) in [
        ("title", "Another title"),
        ("message", "Another message"),
        ("signer_name", "Another Name"),
    ] {
        let was = f.record("sign_request", &f.req).get(k).cloned();
        let (s, a) = f
            .api(
                "admin",
                "/api/sign_request/update",
                json!({"id": f.req, k: v}),
            )
            .await;
        assert_eq!(s, 200, "{a}");
        assert_eq!(check_of(&f, "mia").await, "differs", "{k}");
        let (s, a) = f
            .api(
                "admin",
                "/api/sign_request/update",
                json!({"id": f.req, k: was.unwrap_or(J::Null)}),
            )
            .await;
        assert_eq!(s, 200, "{a}");
        assert_eq!(check_of(&f, "mia").await, "matches", "{k} back");
    }
    // An entry of 0.2.x carries no wording digest: the evidence alone is compared.
    let (eid, mut e) = f
        .app
        .store
        .backend()
        .find("security_event", "kind", &J::from("sign_signed"))
        .unwrap()
        .remove(0);
    let old: String = {
        let d = e["detail"].as_str().unwrap();
        let (head, tail) = d.split_once("; wording ").unwrap();
        let tail = tail.split_once("; ").unwrap().1;
        format!("{head}; {tail}")
    };
    e.insert("detail".into(), J::from(old));
    f.app
        .store
        .backend()
        .apply(&[obop_storage::Op::Delete {
            model: "security_event".into(),
            id: eid.clone(),
        }])
        .unwrap();
    f.app
        .store
        .backend()
        .apply(&[obop_storage::Op::Insert {
            model: "security_event".into(),
            id: eid.clone(),
            fields: e,
        }])
        .unwrap();
    let (s, _) = f
        .api(
            "admin",
            "/api/sign_request/update",
            json!({"id": f.req, "title": "Changed after an old signature"}),
        )
        .await;
    assert_eq!(s, 200);
    assert_eq!(check_of(&f, "mia").await, "matches");
    // Without the entry there is nothing to compare with.
    f.app
        .store
        .backend()
        .apply(&[obop_storage::Op::Delete {
            model: "security_event".into(),
            id: eid,
        }])
        .unwrap();
    assert_eq!(check_of(&f, "mia").await, "no_entry");
    // The list leaves it empty.
    let (s, v) = f.api("mia", "/api/sign_request/list", json!({})).await;
    assert_eq!(s, 200, "{v}");
    assert!(!v.to_string().contains("evidence_check\":\""), "{v}");
}

// ---- O19, O59: words edited after the page was made are not signed ---------------------------

/// The value of a hidden field of the signer's page.
fn hidden(html: &str, name: &str) -> String {
    let at = format!("name=\"{name}\" value=\"");
    let rest = &html[html.find(&at).unwrap_or_else(|| panic!("{name}: {html}")) + at.len()..];
    rest[..rest.find('"').unwrap()].to_string()
}

#[tokio::test]
async fn a_title_edited_after_the_page_was_made_is_shown_again_before_anything_is_signed() {
    let (f, _, path, _) = sent().await;
    let page = f.get(&path, &ip(181)).await;
    assert_eq!(page.status, 200, "{}", page.text());
    let words = hidden(&page.text(), "words");
    assert_eq!(words.len(), 64, "{words}");
    // The manager changes the title while the signer reads the page.
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/update",
            json!({"id": f.req, "title": "Pay 900 EUR"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let form = |w: &str| {
        [
            ("name", "Jane Test".to_string()),
            ("agree", "1".to_string()),
            ("doc", f.file.clone()),
            ("words", w.to_string()),
        ]
    };
    let pairs = form(&words);
    let pairs: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let r = f.post(&path, &ip(181), &pairs).await;
    assert_eq!(r.status, 400, "{}", r.text());
    let html = r.text();
    assert!(
        html.contains("The page has changed since you opened it"),
        "{html}"
    );
    assert!(
        html.contains("Pay 900 EUR"),
        "the new title is shown: {html}"
    );
    assert_eq!(f.record("sign_request", &f.req)["state"], "sent");
    assert!(f.events_of("sign_signed").is_empty());
    // Signed from the page that shows the new title, the evidence matches it.
    let again = hidden(&html, "words");
    assert_ne!(again, words);
    let pairs = form(&again);
    let pairs: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let r = f.post(&path, &ip(181), &pairs).await;
    assert_eq!(r.status, 303, "{}", r.text());
    assert_eq!(f.record("sign_request", &f.req)["title"], "Pay 900 EUR");
    assert_eq!(check_of(&f, "mia").await, "matches");
}
