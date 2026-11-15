//! The commands around the door: sending a request about a Document, making and revoking links,
//! and the guards on the generic writes. The signer's side is in `door.rs`.

use super::*;

/// The keys of `sign_request` that only the signer door writes. Written out here
/// on purpose, not read from the code: `the_keys_of_the_tests_are_the_keys_the_code_guards` holds the
/// two lists equal, so a key added to one and not the other fails.
pub(super) const DOOR_KEYS: [&str; 8] = [
    "document_hash",
    "document_file",
    "signed_at",
    "signed_name",
    "consent_version",
    "signer_ip",
    "signer_user_agent",
    "signature_reference",
];

#[test]
fn the_keys_of_the_tests_are_the_keys_the_code_guards() {
    // The tests walk `DOOR_KEYS`; the guards walk `DOOR_OWNED`. Were a key in one and not the
    // other, the tests would either skip a guarded key or probe one nobody guards.
    assert_eq!(DOOR_KEYS, obop_commands::sign::DOOR_OWNED);
}

const AGENT_SCOPES: [&str; 4] = [
    "sign.user",
    "sign.manager",
    "documents.user",
    "documents.manager",
];

async fn agent_for(f: &mut Fx, owner: &str) -> String {
    let (s, v) = f
        .api(
            owner,
            "/api/auth/agent-token",
            json!({"scopes": AGENT_SCOPES, "label": "bot"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let t = v["token"].as_str().unwrap().to_string();
    f.tokens.insert("agent", t.clone());
    t
}

fn err(v: &J) -> String {
    v["error"].as_str().unwrap_or("").to_string()
}

#[tokio::test]
async fn sending_a_document_request_keeps_a_copy_and_the_hash() {
    let f = fixture().await;
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/send_document",
            json!({"id": f.req}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let req = f.record("sign_request", &f.req);
    assert_eq!(req["state"], "sent");
    assert_eq!(req["document_hash"], f.file);
    assert_eq!(req["document_file"], f.file);
    assert!(req["sent_on"].as_str().is_some_and(|d| d.len() == 10));
    // The file is held by the Document and by the request.
    let info = f.app.store.blobs().info(&f.file).unwrap().unwrap();
    let holders: Vec<(String, String)> = info
        .owners
        .iter()
        .map(|o| (o.model.clone(), o.field.clone()))
        .collect();
    assert!(
        holders.contains(&("sign_request".into(), "document_file".into())),
        "{holders:?}"
    );
    assert!(
        holders.contains(&("documents_document".into(), "attachment".into())),
        "{holders:?}"
    );
    // The Document gets another file: the copy that was sent stays.
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
    let (s, _) = f
        .api(
            "ann",
            "/api/documents_document/update",
            json!({"id": f.doc, "attachment": newer}),
        )
        .await;
    assert_eq!(s, 200);
    let bytes = f
        .app
        .store
        .blobs()
        .read_all(&f.file, 1 << 20)
        .unwrap()
        .unwrap();
    assert_eq!(bytes, pdf(1));
    // And it cannot be changed by hand afterwards.
    for k in ["document_hash", "document_file"] {
        let (s, v) = f
            .api(
                "mia",
                "/api/sign_request/update",
                json!({"id": f.req, k: "0".repeat(64)}),
            )
            .await;
        assert_ne!(s, 200, "{k}: {v}");
    }
}

#[tokio::test]
async fn the_old_send_keeps_its_contract_for_templates() {
    let f = fixture().await;
    // A request about a document is not sent with `send`.
    let (s, v) = f
        .api("mia", "/api/sign_request/send", json!({"id": f.req}))
        .await;
    assert_ne!(s, 200);
    assert!(err(&v).contains("send_document"), "{v}");
    assert_eq!(f.record("sign_request", &f.req)["state"], "draft");
    // A request needs a template or a document.
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/create",
            json!({"title": "Empty", "signer_name": "Jane Test"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let bare = v["value"]["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api("mia", "/api/sign_request/send", json!({"id": bare}))
        .await;
    assert_ne!(s, 200);
    assert!(err(&v).contains("template or a document"), "{v}");
    let (s, _) = f
        .api(
            "mia",
            "/api/sign_request/send_document",
            json!({"id": bare}),
        )
        .await;
    assert_ne!(s, 200);
    // A request from a template needs the address, as before.
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_template/create",
            json!({"name": "Employment (test)", "body": "text"}),
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
    let by_tpl = v["value"]["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api("mia", "/api/sign_request/send", json!({"id": by_tpl}))
        .await;
    assert_ne!(s, 200);
    assert!(err(&v).contains("signer_email"), "{v}");
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/update",
            json!({"id": by_tpl, "signer_email": "jane@example.test"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let (s, v) = f
        .api("mia", "/api/sign_request/send", json!({"id": by_tpl}))
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(f.record("sign_request", &by_tpl)["state"], "sent");
    // It holds no copy and no link can be made for it.
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_link/create",
            json!({"request_id": by_tpl}),
        )
        .await;
    assert_ne!(s, 200, "{v}");
    // The manual path still records a signature on a sent request.
    let (s, v) = f
        .api("mia", "/api/sign_request/sign", json!({"id": by_tpl}))
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(f.record("sign_request", &by_tpl)["state"], "signed");
}

#[tokio::test]
async fn a_document_that_the_sender_cannot_see_cannot_be_sent() {
    let f = fixture().await;
    let holders = |f: &Fx| {
        f.app
            .store
            .blobs()
            .info(&f.file)
            .unwrap()
            .unwrap()
            .owners
            .len()
    };
    let before = holders(&f);
    // The draft is mia's, who can see the restricted Document of ann. A person who cannot see it
    // sends it through `send_document` and is refused: bob is a manager without a role that
    // lets him see a restricted Document, sam has no role in Documents at all, noc is a manager
    // without one too.
    for who in ["bob", "sam", "noc"] {
        let (s, v) = f
            .api(who, "/api/sign_request/send_document", json!({"id": f.req}))
            .await;
        assert_eq!(s, 400, "{who}: {v}");
        assert!(
            err(&v).contains("was not found, or you may not see it"),
            "{who}: {v}"
        );
        let rec = f.record("sign_request", &f.req);
        assert_eq!(rec["state"], "draft", "{who}");
        assert!(rec.get("document_file").is_none_or(J::is_null), "{who}");
        assert_eq!(holders(&f), before, "{who}: nothing was attached");
    }
    // The person who can see it sends it.
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/send_document",
            json!({"id": f.req}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
}

#[tokio::test]
async fn an_approver_who_cannot_see_the_document_cannot_approve_the_send() {
    let mut f = fixture().await;
    let agent = agent_for(&mut f, "mia").await;
    let call = |path: &str, body: J| {
        let (router, agent, path) = (f.router.clone(), agent.clone(), path.to_string());
        async move {
            let r = send(
                &router,
                Request::post(path)
                    .header("content-type", "application/json")
                    .header("host", HOST)
                    .header("authorization", format!("Bearer {agent}"))
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await;
            (r.status, r.json())
        }
    };
    let (s, v) = call("/api/sign_request/send_document", json!({"id": f.req})).await;
    assert_eq!(
        (s, v["approval_required"].clone()),
        (202, json!(true)),
        "{v}"
    );
    let plan = v["plan_id"].as_str().unwrap().to_string();
    assert_eq!(f.record("sign_request", &f.req)["state"], "draft");
    // Bob holds the groups but cannot see ann's restricted Document.
    let (s, v) = f
        .api("bob", &format!("/api/approve/{plan}"), json!({}))
        .await;
    assert_ne!(s, 200, "{v}");
    assert_eq!(f.record("sign_request", &f.req)["state"], "draft", "{v}");
    assert!(
        f.record("sign_request", &f.req)
            .get("document_file")
            .is_none_or(J::is_null)
    );
    // The plan was not spent by the refusal, or it was and is gone: either way mia can start over.
    let (s, v) = call("/api/sign_request/send_document", json!({"id": f.req})).await;
    assert_eq!(s, 202, "{v}");
    let plan = v["plan_id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api("mia", &format!("/api/approve/{plan}"), json!({}))
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(f.record("sign_request", &f.req)["state"], "sent");
}

#[tokio::test]
async fn only_a_pdf_that_exists_can_be_sent() {
    let f = fixture().await;
    // A text file in the Document.
    let (s, v) = f
        .upload(
            "ann",
            "documents_document.attachment",
            "text/plain",
            b"made-up notes".to_vec(),
        )
        .await;
    assert_eq!(s, 201, "{v}");
    let txt = v["id"].as_str().unwrap().to_string();
    let doc = |body: J| {
        let f = &f;
        async move {
            let (s, v) = f.api("ann", "/api/documents_document/create", body).await;
            assert_eq!(s, 200, "{v}");
            v["value"]["id"].as_str().unwrap().to_string()
        }
    };
    let d_text = doc(
        json!({"title": "Notes", "kind": "other", "confidentiality": "internal",
                             "owner": "ann", "attachment": txt}),
    )
    .await;
    let d_none = doc(
        json!({"title": "No file", "kind": "other", "confidentiality": "internal",
                             "owner": "ann"}),
    )
    .await;
    for (d, want) in [(d_text, "PDF"), (d_none, "no file")] {
        let (s, v) = f
            .api(
                "mia",
                "/api/sign_request/create",
                json!({"title": "T", "signer_name": "Jane Test", "document_id": d}),
            )
            .await;
        assert_eq!(s, 200, "{v}");
        let id = v["value"]["id"].as_str().unwrap().to_string();
        let (s, v) = f
            .api("mia", "/api/sign_request/send_document", json!({"id": id}))
            .await;
        assert_ne!(s, 200, "{v}");
        assert!(err(&v).contains(want), "{want}: {v}");
        assert_eq!(f.record("sign_request", &id)["state"], "draft");
    }
}

#[tokio::test]
async fn a_link_is_made_by_a_person_with_the_manager_group_for_a_sent_request() {
    let mut f = fixture().await;
    let body = json!({"request_id": f.req});
    // Not sent yet.
    let (s, v) = f.api("mia", "/api/sign_link/create", body.clone()).await;
    assert_ne!(s, 200, "{v}");
    assert!(err(&v).contains("draft"), "{v}");
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/send_document",
            json!({"id": f.req}),
        )
        .await;
    assert_eq!(s, 200, "{v}");

    // Not a person with the manager group.
    let (s, v) = f.api("sam", "/api/sign_link/create", body.clone()).await;
    assert_ne!(s, 200, "{v}");
    // A person who cannot see the Document is told so, in the words used for an id that does not
    // exist (the answer does not say which it is).
    let (s, v) = f.api("bob", "/api/sign_link/create", body.clone()).await;
    assert_ne!(s, 200, "{v}");
    assert!(
        err(&v).contains("was not found, or you may not see it"),
        "{v}"
    );

    // An agent never gets a link, not even through a plan that a person approves.
    agent_for(&mut f, "mia").await;
    let (s, v) = f.api("agent", "/api/sign_link/create", body.clone()).await;
    if s == 202 {
        let plan = v["plan_id"].as_str().unwrap().to_string();
        let (s, v) = f
            .api("mia", &format!("/api/approve/{plan}"), json!({}))
            .await;
        assert_ne!(s, 200, "{v}");
        assert!(v["value"]["link"].is_null(), "{v}");
    } else {
        assert_ne!(s, 200, "{v}");
    }
    assert_eq!(f.app.store.backend().count("sign_link").unwrap(), 0);

    // Nothing but the request and a time can be given.
    for extra in [
        json!({"request_id": f.req, "used_at": "2030-01-01T00:00:00Z"}),
        json!({"request_id": f.req, "revoked_at": "2030-01-01T00:00:00Z"}),
        json!({"request_id": f.req, "id": "0".repeat(64)}),
        json!({"request_id": f.req, "link": "/sign/x/y"}),
    ] {
        let (s, v) = f.api("mia", "/api/sign_link/create", extra.clone()).await;
        assert_ne!(s, 200, "{extra}: {v}");
    }
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_link/create",
            json!({"request_id": "no-such-request"}),
        )
        .await;
    assert_ne!(s, 200, "{v}");
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_link/create",
            json!({"request_id": f.req, "expires_at": "2001-01-01T00:00:00Z"}),
        )
        .await;
    assert_ne!(s, 200, "{v}");
    assert_eq!(f.app.store.backend().count("sign_link").unwrap(), 0);

    // The one that works: the answer holds the link once, the store holds a hash.
    let (s, v) = f.api("mia", "/api/sign_link/create", body).await;
    assert_eq!(s, 200, "{v}");
    let link = v["value"]["link"].as_str().unwrap().to_string();
    let tok = link.rsplit('/').next().unwrap();
    assert_eq!(tok.len(), 64);
    assert!(
        tok.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    );
    let id = v["value"]["id"].as_str().unwrap().to_string();
    assert_ne!(id, tok);
    assert_eq!(id.len(), 64);
    let stored = f.everything_stored();
    assert!(!stored.contains(tok), "the secret is stored somewhere");
    assert!(!stored.contains(&link));
    // Reading it back never returns the link.
    for (cmd, body) in [
        ("get", json!({"id": id})),
        ("list", json!({})),
        ("search", json!({"limit": 50})),
    ] {
        let (s, v) = f.api("mia", &format!("/api/sign_link/{cmd}"), body).await;
        assert_eq!(s, 200, "{cmd}: {v}");
        let text = v.to_string();
        assert!(
            !text.contains(tok) && !text.contains("/sign/"),
            "{cmd}: {text}"
        );
    }
    // The issue is in the log, without the secret.
    let issued = f.events_of("sign_link_issued");
    assert_eq!(issued.len(), 1);
    assert!(!issued[0]["detail"].as_str().unwrap().contains(tok));
    assert_eq!(issued[0]["actor"], "mia");
    // The default life is fourteen days.
    let ends = f.record("sign_link", &id)["expires_at"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        ends > obop_values::datetime::Instant::from_micros(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as i64
                + 13 * 86_400_000_000
        )
        .unwrap()
        .canonical()
    );
}

#[tokio::test]
async fn a_shorter_life_can_be_asked_for_but_not_a_longer_one() {
    let (f, _, _, _) = sent().await;
    let in_an_hour = obop_values::datetime::Instant::from_micros(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as i64
            + 3_600_000_000,
    )
    .unwrap()
    .canonical();
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_link/create",
            json!({"request_id": f.req, "expires_at": in_an_hour}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["value"]["expires_at"], in_an_hour.as_str());
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_link/create",
            json!({"request_id": f.req, "expires_at": "2099-01-01T00:00:00Z"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let ends = v["value"]["expires_at"].as_str().unwrap();
    assert!(
        ends.starts_with("20") && !ends.starts_with("2099"),
        "{ends}"
    );
}

#[tokio::test]
async fn the_twenty_first_link_in_an_hour_is_refused() {
    let (f, _, _, _) = sent().await;
    // `sent` already made one.
    for i in 0..19 {
        let (s, v) = f
            .api("mia", "/api/sign_link/create", json!({"request_id": f.req}))
            .await;
        assert_eq!(s, 200, "link {i}: {v}");
    }
    let (s, v) = f
        .api("mia", "/api/sign_link/create", json!({"request_id": f.req}))
        .await;
    assert_ne!(s, 200, "{v}");
    assert!(err(&v).contains("too many"), "{v}");
    assert_eq!(f.app.store.backend().count("sign_link").unwrap(), 20);
    // Another person has a count of their own.
    let (s, _) = f
        .api(
            "admin",
            "/api/sign_link/create",
            json!({"request_id": f.req}),
        )
        .await;
    assert_eq!(s, 200);
}

#[tokio::test]
async fn a_link_is_changed_only_by_revoking_it() {
    let (f, id, path, _) = sent().await;
    let before = f.record("sign_link", &id);
    for body in [
        json!({"id": id, "request_id": "other"}),
        json!({"id": id, "expires_at": "2099-01-01T00:00:00Z"}),
        json!({"id": id, "used_at": "2030-01-01T00:00:00Z"}),
        json!({"id": id, "link": "x"}),
    ] {
        let (s, v) = f.api("mia", "/api/sign_link/update", body.clone()).await;
        assert_ne!(s, 200, "{body}: {v}");
    }
    assert_eq!(f.record("sign_link", &id), before);
    // Echoing what is stored is not a change; revoking is.
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_link/update",
            json!({"id": id, "request_id": before["request_id"], "revoked_at": "2001-01-01T00:00:00Z"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    assert!(f.record("sign_link", &id)["revoked_at"].is_string());
    // Revoked once.
    let (s, _) = f
        .api(
            "mia",
            "/api/sign_link/update",
            json!({"id": id, "revoked_at": "2002-01-01T00:00:00Z"}),
        )
        .await;
    assert_ne!(s, 200);
    // Not deleted, not created in bulk, by a person who is not an administrator.
    let (s, _) = f
        .api("mia", "/api/sign_link/delete", json!({"id": id}))
        .await;
    assert_ne!(s, 200);
    let (s, _) = f
        .api(
            "mia",
            "/api/sign_link/bulk_create",
            json!({"rows": [{"request_id": f.req}]}),
        )
        .await;
    assert_ne!(s, 200);
    let (s, _) = f
        .api(
            "admin",
            "/api/sign_link/bulk_create",
            json!({"rows": [{"request_id": f.req}]}),
        )
        .await;
    assert_ne!(s, 200, "not even an administrator makes links in bulk");
    assert_eq!(f.app.store.backend().count("sign_link").unwrap(), 1);
    // A used link cannot be revoked.
    let (_, second) = f.mint("mia", &f.req.clone()).await;
    assert_ne!(second, path);
    let r = f
        .post(
            &second,
            &ip(140),
            &[("name", "Jane Test"), ("agree", "1"), ("doc", &f.file)],
        )
        .await;
    assert_eq!(r.status, 303);
    let used = f.app.store.backend().list("sign_link").unwrap();
    let used_id = used
        .iter()
        .find(|(_, r)| r.get("used_at").is_some_and(J::is_string))
        .unwrap()
        .0
        .clone();
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_link/update",
            json!({"id": used_id, "revoked_at": "2003-01-01T00:00:00Z"}),
        )
        .await;
    assert_ne!(s, 200, "{v}");
}

#[tokio::test]
async fn the_evidence_belongs_to_the_door() {
    let f = fixture().await;
    // A value that is valid for the kind of each key, so that only the guard stands in the way
    // (a value the field type refuses would be refused whether the guard is there or not).
    let value = |k: &str| -> String {
        match k {
            "signed_at" => "2026-11-03T10:00:00.000000Z".to_string(),
            "document_hash" | "document_file" => f.file.clone(),
            "signer_ip" => "198.51.100.7".to_string(),
            other => format!("forged {other}"),
        }
    };
    for k in DOOR_KEYS {
        let v = value(k);
        let (s, r) = f
            .api(
                "mia",
                "/api/sign_request/create",
                json!({"title": "Forged", "signer_name": "Jane Test", "document_id": f.doc, k: v}),
            )
            .await;
        assert_eq!(s, 400, "create with {k}: {r}");
        assert!(err(&r).contains("signing process"), "create with {k}: {r}");
        let (s, r) = f
            .api(
                "mia",
                "/api/sign_request/update",
                json!({"id": f.req, k: v}),
            )
            .await;
        assert_eq!(s, 400, "update with {k}: {r}");
        assert!(err(&r).contains("signing process"), "update with {k}: {r}");
        let (s, r) = f
            .api(
                "mia",
                "/api/sign_request/bulk_create",
                json!({"rows": [{"title": "Forged", "signer_name": "Jane Test", k: v}]}),
            )
            .await;
        assert_eq!(s, 400, "bulk rows with {k}: {r}");
        assert!(
            err(&r).contains("signing process"),
            "bulk rows with {k}: {r}"
        );
        let (s, r) = f
            .api(
                "mia",
                "/api/sign_request/bulk_create",
                json!({"csv": format!("title,signer_name,{k}\nForged,Jane Test,{v}\n")}),
            )
            .await;
        assert_eq!(s, 400, "bulk csv with {k}: {r}");
        assert!(
            err(&r).contains("signing process"),
            "bulk csv with {k}: {r}"
        );
    }
    assert_eq!(f.app.store.backend().count("sign_request").unwrap(), 1);
    // Saving the form back as it is stored changes nothing and is allowed.
    let cur = f.record("sign_request", &f.req);
    let mut body = serde_json::Map::new();
    body.insert("id".into(), json!(f.req));
    body.insert("title".into(), cur["title"].clone());
    let (s, v) = f
        .api("mia", "/api/sign_request/update", J::Object(body))
        .await;
    assert_eq!(s, 200, "{v}");
    // The state cannot be moved to signed by an update, whoever asks.
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/update",
            json!({"id": f.req, "state": "signed"}),
        )
        .await;
    assert_ne!(s, 200, "{v}");
    assert_eq!(f.record("sign_request", &f.req)["state"], "draft");
}

#[tokio::test]
async fn the_held_copy_is_read_by_an_administrator_only() {
    let (f, _, _, _) = sent().await;
    // The Document gets another file, so that the held copy is held by the request alone.
    let (s, v) = f
        .upload(
            "ann",
            "documents_document.attachment",
            "application/pdf",
            pdf(5),
        )
        .await;
    assert_eq!(s, 201, "{v}");
    let newer = v["id"].as_str().unwrap().to_string();
    let (s, _) = f
        .api(
            "ann",
            "/api/documents_document/update",
            json!({"id": f.doc, "attachment": newer}),
        )
        .await;
    assert_eq!(s, 200);
    assert_eq!(f.blob("admin", &f.file).await.status, 200);
    for who in ["mia", "ann", "bob", "sam"] {
        let r = f.blob(who, &f.file).await;
        assert!(matches!(r.status, 403 | 404), "{who}: {}", r.status);
    }
    // The request does not show the file id to anybody but the administrator.
    let (s, v) = f
        .api("mia", "/api/sign_request/get", json!({"id": f.req}))
        .await;
    assert_eq!(s, 200);
    assert!(
        v["value"].get("document_file").is_none_or(J::is_null),
        "{v}"
    );
    let (_, v) = f
        .api("admin", "/api/sign_request/get", json!({"id": f.req}))
        .await;
    assert_eq!(v["value"]["document_file"], f.file.as_str());
}

#[test]
fn the_wrappers_rely_on_names_the_package_still_declares() {
    // The door writes `state` of a request straight to `signed` in one batch, so it depends on
    // the package declaring that flow: if the package changes, this fails before a signer does.
    let m = obop_pkg::Manifest::parse(&Bundled.fetch("sign").unwrap()).unwrap();
    let model = |n: &str| {
        m.model
            .iter()
            .find(|x| x.name == n)
            .unwrap_or_else(|| panic!("{n}"))
    };
    let req = model("sign_request");
    let field = |n: &str| {
        req.field
            .iter()
            .find(|f| f.name == n)
            .unwrap_or_else(|| panic!("{n}"))
    };
    let state = field("state");
    for o in ["draft", "sent", "signed", "declined", "expired"] {
        assert!(state.options.iter().any(|x| x == o), "state option {o}");
    }
    for k in [
        "document_id",
        "document_hash",
        "document_file",
        "signed_at",
        "signed_on",
        "signed_name",
        "consent_version",
        "signer_ip",
        "signer_user_agent",
        "signature_reference",
        "sent_on",
        "expires_on",
        "title",
        "message",
        "signer_name",
    ] {
        field(k);
    }
    let act = |n: &str| {
        req.action
            .iter()
            .find(|a| a.name == n)
            .unwrap_or_else(|| panic!("{n}"))
    };
    for (n, from, to) in [
        ("send", "draft", "sent"),
        ("send_document", "draft", "sent"),
        ("sign", "sent", "signed"),
    ] {
        let a = act(n);
        assert_eq!(
            (a.field.as_str(), a.from.as_deref(), a.to.as_str()),
            ("state", Some(from), to)
        );
    }
    let link = model("sign_link");
    for k in ["request_id", "expires_at", "used_at", "revoked_at", "link"] {
        assert!(link.field.iter().any(|f| f.name == k), "{k}");
    }
    assert!(
        link.action.is_empty(),
        "a link has no action: revoking is a stamp"
    );
    // The door-owned fields are not editable by anyone with only the user group.
    assert_eq!(field("document_file").group.as_deref(), Some("sign.system"));
    // And they are the fields of the request that carry a group of their own, besides the switch
    // `show_history` (0.2.2: it shows a name to other people, so only a manager sets it, and the
    // wrappers refuse it to everyone else): a field added with a group is a field the guards
    // should have been told about (and the other way round).
    let mut grouped: Vec<&str> = req
        .field
        .iter()
        .filter(|f| f.group.is_some())
        .map(|f| f.name.as_str())
        .collect();
    grouped.sort_unstable();
    let mut owned = obop_commands::sign::DOOR_OWNED.to_vec();
    owned.push(obop_commands::sign::HISTORY_FLAG);
    owned.sort_unstable();
    assert_eq!(grouped, owned);
    assert_eq!(
        field(obop_commands::sign::HISTORY_FLAG).group.as_deref(),
        Some("sign.manager")
    );
    assert_eq!(field(obop_commands::sign::HISTORY_FLAG).kind, "bool");
    assert!(!field(obop_commands::sign::HISTORY_FLAG).required);
    // A link is made, revoked and read for the secret by managers only: a plain user changes
    // nothing about one, and the `link` answer of the create call is the manager's alone.
    assert_eq!(link.write_group, "sign.manager");
    assert_eq!(
        link.field
            .iter()
            .find(|f| f.name == "link")
            .unwrap()
            .group
            .as_deref(),
        Some("sign.manager")
    );
    // The evidence is read by managers; the plain user's group is not on any of the keys.
    for k in [
        "signed_at",
        "signed_name",
        "consent_version",
        "signer_ip",
        "signer_user_agent",
        "signature_reference",
        "document_hash",
    ] {
        assert_eq!(field(k).group.as_deref(), Some("sign.manager"), "{k}");
    }
}
