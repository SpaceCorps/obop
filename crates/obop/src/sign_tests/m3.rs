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
