//! The checks added after the second independent review of D92 (2026-11-03, second round): the
//! Document cannot be cleared to unlock the manual signature, the evidence cannot be blanked, an
//! administrator's agent token is not an administrator for the signing guards, the address is the
//! peer when the proxy's entry is not an address, a link whose end cannot be read does not work,
//! and what is known and documented about a refused update. Synthetic bytes and made-up names.

use super::hardening::{agent_for, count, err, fields, good_form, signed_count};
use super::rules::DOOR_KEYS;
use super::*;

/// The message of the assertion on the manual action.
const BY_LINK: &str = "through the signing link";

/// A Document that anybody with `documents.user` may use, other than the fixture's.
async fn second_document(f: &Fx) -> String {
    let (s, v) = f
        .upload(
            "ann",
            "documents_document.attachment",
            "application/pdf",
            pdf(2),
        )
        .await;
    assert_eq!(s, 201, "{v}");
    let file = v["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api(
            "ann",
            "/api/documents_document/create",
            json!({"title": "Another (test)", "kind": "contract", "confidentiality": "internal",
                   "state": "draft", "owner": "ann", "attachment": file}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    v["value"]["id"].as_str().unwrap().to_string()
}

/// The ways a person or an agent can try to mark the request signed by hand. Each must be refused
/// with the message about the signing link and leave the request sent.
async fn every_manual_signature_is_refused(f: &Fx, who: &[&str], why: &str) {
    let req = f.req.clone();
    for w in who {
        let (s, v) = f.api(w, "/api/sign_request/sign", json!({"id": req})).await;
        assert_eq!(s, 400, "{why}: {w} sign: {v}");
        assert!(err(&v).contains(BY_LINK), "{why}: {w} sign: {v}");
        let (s, v) = f
            .api(
                w,
                "/api/sign_request/update",
                json!({"id": req, "state": "signed"}),
            )
            .await;
        assert_eq!(s, 400, "{why}: {w} update: {v}");
        assert!(err(&v).contains(BY_LINK), "{why}: {w} update: {v}");
        assert_eq!(
            f.record("sign_request", &req)["state"],
            "sent",
            "{why}: {w}"
        );
    }
}

#[tokio::test]
async fn clearing_the_document_does_not_unlock_the_manual_signature() {
    let (mut f, _, path, _) = sent().await;
    let before = f.record("sign_request", &f.req);
    let doc2 = second_document(&f).await;

    // A plain user cannot take the Document off a request that was sent for it, in either spelling
    // of nothing, or point it at another one: the copy and the hash that were taken belong to
    // the first.
    for (what, body) in [
        ("null", json!({"id": f.req, "document_id": null})),
        ("empty text", json!({"id": f.req, "document_id": ""})),
        (
            "another document",
            json!({"id": f.req, "document_id": doc2}),
        ),
    ] {
        for who in ["sam", "mia"] {
            let (s, v) = f.api(who, "/api/sign_request/update", body.clone()).await;
            assert_eq!(s, 400, "{who} {what}: {v}");
            assert_eq!(
                f.record("sign_request", &f.req),
                before,
                "{who} {what}: nothing changed"
            );
        }
    }
    let (s, v) = f
        .api(
            "sam",
            "/api/sign_request/update",
            json!({"id": f.req, "document_id": null}),
        )
        .await;
    assert_eq!(s, 400, "{v}");
    assert!(
        err(&v).contains("cannot be changed after the request was sent"),
        "{v}"
    );
    // Pointing it at another Document is refused for the same reason (sam, who has no rights in
    // Documents, is stopped earlier by the check that the Document is one he can use).
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/update",
            json!({"id": f.req, "document_id": doc2}),
        )
        .await;
    assert_eq!(s, 400, "{v}");
    assert!(
        err(&v).contains("cannot be changed after the request was sent"),
        "{v}"
    );
    // The same document and other fields are still fine to write.
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/update",
            json!({"id": f.req, "document_id": f.doc, "title": "Renamed (test)"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(f.record("sign_request", &f.req)["title"], "Renamed (test)");

    // The one-call variant: the state moves first, so the stored record decides, and it is refused.
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/update",
            json!({"id": f.req, "state": "signed", "document_id": null}),
        )
        .await;
    assert_eq!(s, 400, "{v}");
    assert_eq!(f.record("sign_request", &f.req)["state"], "sent");

    // Nobody signs by hand, by the action or by moving the state.
    every_manual_signature_is_refused(&f, &["mia", "admin"], "document in place").await;

    // Giving the request a template (any `sign.user` may) does not turn it into a legacy request
    // that is signed by hand: it still names a Document and holds the copy.
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_template/create",
            json!({"name": "Anything (test)", "body": "text"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let tpl = v["value"]["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api(
            "sam",
            "/api/sign_request/update",
            json!({"id": f.req, "template_id": tpl}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    every_manual_signature_is_refused(&f, &["mia", "admin"], "a template was added").await;
    let (s, v) = f
        .api(
            "sam",
            "/api/sign_request/update",
            json!({"id": f.req, "template_id": null}),
        )
        .await;
    assert_eq!(s, 200, "{v}");

    // An administrator may take the Document off (she can edit anything); the request still holds
    // the hash and the copy it was sent with, and that is what keeps it for the door.
    let (s, v) = f
        .api(
            "admin",
            "/api/sign_request/update",
            json!({"id": f.req, "document_id": null}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    assert!(
        f.record("sign_request", &f.req)
            .get("document_id")
            .is_none_or(J::is_null)
    );
    every_manual_signature_is_refused(&f, &["mia", "admin"], "document cleared by an admin").await;
    // Each of the three is enough alone: an administrator who also clears the copy, or instead the
    // hash (both are door-owned, so only she can), still meets the one that is left.
    for (clear, left) in [
        ("document_file", "the hash and no template"),
        ("document_hash", "the copy"),
    ] {
        let (s, v) = f
            .api(
                "admin",
                "/api/sign_request/update",
                json!({"id": f.req, clear: null}),
            )
            .await;
        assert_eq!(s, 200, "{clear}: {v}");
        every_manual_signature_is_refused(&f, &["mia", "admin"], left).await;
        let (s, v) = f
            .api(
                "admin",
                "/api/sign_request/update",
                json!({"id": f.req, clear: f.file}),
            )
            .await;
        assert_eq!(s, 200, "{clear} put back: {v}");
    }

    // An agent: the update is refused outright, and the sign action is only a proposal that is
    // refused when the person approves it.
    let (s, v) = f
        .api(
            "admin",
            "/api/sign_request/update",
            json!({"id": f.req, "document_id": f.doc}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    agent_for(&mut f, "mia").await;
    let (s, v) = f
        .api(
            "agent",
            "/api/sign_request/update",
            json!({"id": f.req, "document_id": null}),
        )
        .await;
    assert_eq!(s, 400, "{v}");
    let (s, v) = f
        .api("agent", "/api/sign_request/sign", json!({"id": f.req}))
        .await;
    assert_eq!(
        (s, v["approval_required"].clone()),
        (202, json!(true)),
        "{v}"
    );
    let plan = v["plan_id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api("mia", &format!("/api/approve/{plan}"), json!({}))
        .await;
    assert_eq!(s, 400, "{v}");
    assert!(err(&v).contains(BY_LINK), "{v}");
    assert_eq!(f.record("sign_request", &f.req)["state"], "sent");
    assert_eq!(signed_count(&f), 0);

    // The signer still signs.
    let r = f.post(&path, &ip(80), &good_form(&f, "Jane Test")).await;
    assert_eq!(r.status, 303, "{}", r.text());
    assert_eq!(signed_count(&f), 1);
}

/// A request that names a template and a Document, sent with the generic `send` (which O32 lets
/// through): it holds no hash and no copy, so the door cannot serve it, and nothing about it may
/// be undone by a plain user afterwards.
async fn template_and_document_request_sent_with_send() -> (Fx, String) {
    let f = fixture().await;
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_template/create",
            json!({"name": "Anything (test)", "body": "text"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let tpl = v["value"]["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/update",
            json!({"id": f.req, "template_id": tpl, "signer_email": "jane@example.test"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let (s, v) = f
        .api("mia", "/api/sign_request/send", json!({"id": f.req}))
        .await;
    assert_eq!(s, 200, "{v}");
    let r = f.record("sign_request", &f.req);
    assert_eq!(r["state"], "sent");
    assert_eq!(r["document_id"], f.doc.as_str());
    assert!(
        r.get("document_hash").is_none_or(J::is_null)
            && r.get("document_file").is_none_or(J::is_null),
        "the generic send holds neither a hash nor a copy: {r:?}"
    );
    (f, tpl)
}

#[tokio::test]
async fn the_document_of_a_template_request_sent_with_send_cannot_be_taken_off_by_a_plain_user() {
    // The request holds no hash, so the lock cannot hang on one: it hangs on the request having
    // left draft (D92 decision 6, O32).
    let (f, _) = template_and_document_request_sent_with_send().await;
    let before = f.record("sign_request", &f.req);
    let doc2 = second_document(&f).await;

    // Taking the Document off, in both spellings of nothing, by a plain user and by the manager.
    for who in ["sam", "mia"] {
        for (what, body) in [
            ("null", json!({"id": f.req, "document_id": null})),
            ("empty text", json!({"id": f.req, "document_id": ""})),
        ] {
            let (s, v) = f.api(who, "/api/sign_request/update", body).await;
            assert_eq!(s, 400, "{who} {what}: {v}");
            assert!(
                err(&v).contains("cannot be changed after the request was sent"),
                "{who} {what}: the guard refuses it, not another check: {v}"
            );
            assert_eq!(f.record("sign_request", &f.req), before, "{who} {what}");
        }
    }
    // Another Document is refused for the same reason.
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/update",
            json!({"id": f.req, "document_id": doc2}),
        )
        .await;
    assert_eq!(s, 400, "{v}");
    assert!(
        err(&v).contains("cannot be changed after the request was sent"),
        "{v}"
    );
    // The same Document and other fields are still fine to write.
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/update",
            json!({"id": f.req, "document_id": f.doc, "title": "Renamed (test)"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");

    // Nobody signs it by hand, by the action or by moving the state: it names a Document.
    every_manual_signature_is_refused(&f, &["mia", "admin"], "template and Document, sent").await;
    // No link can be made for it (it holds no copy), so the door is not an alternative either.
    let (s, v) = f
        .api("mia", "/api/sign_link/create", json!({"request_id": f.req}))
        .await;
    assert_eq!(s, 400, "{v}");
    assert_eq!(signed_count(&f), 0);
}

#[tokio::test]
async fn an_administrator_may_take_the_document_off_a_template_request_and_it_is_then_a_plain_template_request()
 {
    // Documented in O32: the exception of the administrator is deliberate, and what is left is
    // the hand-signed template request that has always existed.
    let (f, _) = template_and_document_request_sent_with_send().await;
    let (s, v) = f
        .api(
            "admin",
            "/api/sign_request/update",
            json!({"id": f.req, "document_id": null}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    assert!(
        f.record("sign_request", &f.req)
            .get("document_id")
            .is_none_or(J::is_null)
    );
    let (s, v) = f
        .api("mia", "/api/sign_request/sign", json!({"id": f.req}))
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(f.record("sign_request", &f.req)["state"], "signed");
}

#[tokio::test]
async fn a_sent_request_without_a_document_cannot_be_given_one_by_a_plain_user() {
    // A sent request is not given a Document afterwards, so that no sent request changes what it
    // is about: the lock holds in both directions.
    let f = fixture().await;
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_template/create",
            json!({"name": "Anything (test)", "body": "text"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let tpl = v["value"]["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/create",
            json!({"title": "By template", "template_id": tpl, "signer_name": "Jane Test",
                   "signer_email": "jane@example.test"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let id = v["value"]["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api("mia", "/api/sign_request/send", json!({"id": id}))
        .await;
    assert_eq!(s, 200, "{v}");
    let (s, v) = f
        .api(
            "sam",
            "/api/sign_request/update",
            json!({"id": id, "document_id": f.doc}),
        )
        .await;
    assert_eq!(s, 400, "{v}");
    assert!(
        err(&v).contains("cannot be changed after the request was sent"),
        "{v}"
    );
    // It is still the hand-signed template request it was.
    let (s, v) = f
        .api("mia", "/api/sign_request/sign", json!({"id": id}))
        .await;
    assert_eq!(s, 200, "{v}");
}

#[tokio::test]
async fn a_draft_may_still_change_its_document_freely() {
    // Before it is sent a request holds no copy and no hash, so the Document is an ordinary
    // field: it can be taken off and put back.
    let f = fixture().await;
    let doc2 = second_document(&f).await;
    for body in [
        json!({"id": f.req, "document_id": null}),
        json!({"id": f.req, "document_id": f.doc}),
        json!({"id": f.req, "document_id": doc2}),
    ] {
        let (s, v) = f.api("mia", "/api/sign_request/update", body.clone()).await;
        assert_eq!(s, 200, "{body}: {v}");
    }
}

#[tokio::test]
async fn staff_cannot_blank_the_evidence_with_null_or_empty_text() {
    // On a request that was sent only the hash and the copy are held. A blank is not "no
    // change": it would erase what the door compares the document against.
    let (f, _, path, _) = sent().await;
    let held = f.record("sign_request", &f.req);
    for who in ["sam", "mia"] {
        for key in ["document_hash", "document_file"] {
            for blank in [J::Null, json!("")] {
                let (s, v) = f
                    .api(
                        who,
                        "/api/sign_request/update",
                        json!({"id": f.req, key: blank.clone()}),
                    )
                    .await;
                assert_eq!(s, 400, "{who} {key}={blank}: {v}");
                // An empty text for a file is refused earlier, as not being a file.
                if !(key == "document_file" && blank == json!("")) {
                    assert!(err(&v).contains("signing process"), "{who} {key}: {v}");
                }
                assert_eq!(f.record("sign_request", &f.req), held, "{who} {key}");
            }
        }
    }

    // After the signature every one of the eight keys is protected the same way, against both
    // layers (the door's keys, and the rule that a signed request takes no edit).
    let r = f.post(&path, &ip(81), &good_form(&f, "Jane Test")).await;
    assert_eq!(r.status, 303, "{}", r.text());
    let signed = f.record("sign_request", &f.req);
    for key in DOOR_KEYS {
        assert!(
            signed
                .get(key)
                .is_some_and(|v| v.as_str().is_some_and(|t| !t.is_empty())),
            "the door wrote {key}"
        );
    }
    for who in ["sam", "mia"] {
        for key in DOOR_KEYS {
            for blank in [J::Null, json!("")] {
                let (s, v) = f
                    .api(
                        who,
                        "/api/sign_request/update",
                        json!({"id": f.req, key: blank.clone()}),
                    )
                    .await;
                assert_eq!(s, 400, "{who} {key}={blank}: {v}");
                if !(key == "document_file" && blank == json!("")) {
                    assert!(err(&v).contains("signing process"), "{who} {key}: {v}");
                }
                assert_eq!(f.record("sign_request", &f.req), signed, "{who} {key}");
            }
        }
    }
    assert_eq!(signed_count(&f), 1);
}

#[tokio::test]
async fn an_agent_token_that_holds_the_admin_group_is_not_an_administrator_for_the_signing_guards()
{
    let (mut f, _, path, _) = sent().await;
    // `root` holds platform.admin and the signing groups literally, so she can hand all of them
    // to an agent. The agent has the groups; it is still not a person.
    let (s, v) = f
        .api(
            "root",
            "/api/auth/agent-token",
            json!({"scopes": ["platform.admin", "sign.user", "sign.manager", "documents.user", "documents.manager"],
                   "label": "admin bot"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    f.tokens
        .insert("adminbot", v["token"].as_str().unwrap().to_string());

    let before = count(&f, "sign_request");
    // It cannot create a request that comes with evidence, by a single call or by an import.
    let (s, v) = f
        .api(
            "adminbot",
            "/api/sign_request/create",
            json!({"title": "Forged", "signer_name": "Jane Test", "signed_name": "Jane Forged",
                   "signed_at": "2026-11-03T10:00:00.000000Z", "signer_ip": "203.0.113.5"}),
        )
        .await;
    assert_eq!(s, 400, "{v}");
    assert!(err(&v).contains("signing process"), "{v}");
    let (s, v) = f
        .api(
            "adminbot",
            "/api/sign_request/bulk_create",
            json!({"csv": "title,signer_name,signed_name\nForged,Jane Test,Jane Forged\n"}),
        )
        .await;
    assert_eq!(s, 400, "{v}");
    assert_eq!(count(&f, "sign_request"), before);

    // Nor can it change a signed request, or delete it.
    let r = f.post(&path, &ip(82), &good_form(&f, "Jane Test")).await;
    assert_eq!(r.status, 303, "{}", r.text());
    let signed = f.record("sign_request", &f.req);
    for body in [
        json!({"id": f.req, "signed_name": "Someone Else", "signer_ip": "203.0.113.5"}),
        json!({"id": f.req, "title": "Retitled"}),
    ] {
        let (s, v) = f
            .api("adminbot", "/api/sign_request/update", body.clone())
            .await;
        assert_eq!(s, 400, "{body}: {v}");
        assert_eq!(f.record("sign_request", &f.req), signed, "{body}");
    }
    let (s, v) = f
        .api("adminbot", "/api/sign_request/delete", json!({"id": f.req}))
        .await;
    assert_ne!(s, 200, "{v}");
    assert_eq!(f.record("sign_request", &f.req), signed);

    // The person who holds the group is an administrator, as the documents say.
    let (s, v) = f
        .api(
            "root",
            "/api/sign_request/update",
            json!({"id": f.req, "title": "Retitled by an administrator"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
}

#[tokio::test]
async fn a_link_whose_end_cannot_be_read_does_not_work() {
    // Only a row that someone wrote by hand can have such an end. A link without a readable end
    // is an expired one: it neither opens nor signs, and nothing of the request moves.
    for (what, end) in [("not a time", json!("not a time")), ("empty", json!(""))] {
        let (f, link_id, path, _) = sent().await;
        f.app
            .store
            .backend()
            .update("sign_link", &link_id, &fields(json!({"expires_at": end})))
            .unwrap();
        let from = ip(83);
        assert_eq!(f.get(&path, &from).await.status, 410, "{what}: the page");
        assert_eq!(
            f.get(&format!("{path}/document.pdf"), &from).await.status,
            410,
            "{what}: the PDF"
        );
        let r = f.post(&path, &from, &good_form(&f, "Jane Test")).await;
        assert_eq!(r.status, 410, "{what}: {}", r.text());
        assert_eq!(f.record("sign_request", &f.req)["state"], "sent", "{what}");
        assert!(
            f.record("sign_link", &link_id)
                .get("used_at")
                .is_none_or(J::is_null),
            "{what}"
        );
        assert_eq!(signed_count(&f), 0, "{what}");
    }
}

// ---- the address -------------------------------------------------------------------------

/// A post whose `X-Forwarded-For` lines are given as bytes and whose socket peer is `peer`.
async fn post_from_peer(f: &Fx, path: &str, lines: &[&[u8]], peer: &str, name: &str) -> Resp {
    let body = [
        ("name", name),
        ("agree", "1"),
        ("doc", &f.file),
        ("lang", "en"),
    ]
    .iter()
    .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
    .collect::<Vec<_>>()
    .join("&");
    let mut b = Request::post(path)
        .header("host", HOST)
        .header("origin", ORIGIN)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("user-agent", "TestBrowser/1.0");
    for l in lines {
        b = b.header(
            "x-forwarded-for",
            axum::http::HeaderValue::from_bytes(l).unwrap(),
        );
    }
    let mut req = b.body(Body::from(body)).unwrap();
    req.extensions_mut().insert(axum::extract::ConnectInfo(
        format!("{peer}:4000")
            .parse::<std::net::SocketAddr>()
            .unwrap(),
    ));
    send(&f.router, req).await
}

/// Sign on a fresh instance with these `X-Forwarded-For` lines and the peer 10.0.0.5; returns the
/// address kept as evidence and the text of the log entry.
async fn signed_with(lines: &[&[u8]]) -> (String, String) {
    let (f, _, path, _) = sent().await;
    let r = post_from_peer(&f, &path, lines, "10.0.0.5", "Jane Test").await;
    assert_eq!(r.status, 303, "{}", r.text());
    let kept = f.record("sign_request", &f.req)["signer_ip"]
        .as_str()
        .unwrap()
        .to_string();
    let detail = f.events_of("sign_signed")[0]["detail"]
        .as_str()
        .unwrap()
        .to_string();
    (kept, detail)
}

#[tokio::test]
async fn an_entry_of_the_proxy_that_is_not_an_address_leaves_the_peer_as_the_address() {
    let long = format!("198.51.100.7, {}", "1".repeat(80));
    for (what, lines, want) in [
        (
            "a last entry with a byte that is not text",
            vec![&b"198.51.100.7, \xe9"[..]],
            "10.0.0.5",
        ),
        (
            "an empty last entry",
            vec![&b"198.51.100.7,"[..]],
            "10.0.0.5",
        ),
        ("a name", vec![&b"198.51.100.7, client-a"[..]], "10.0.0.5"),
        ("a long run of digits", vec![long.as_bytes()], "10.0.0.5"),
        (
            "a good entry",
            vec![&b"203.0.113.9, 198.51.100.7"[..]],
            "198.51.100.7",
        ),
        // A byte that is not text in the client's part does not hide the proxy's entry (a client
        // that wrote it would otherwise move to the peer's bucket and address, here and at the
        // limiters of every door): the proxy's entry is the address.
        (
            "a byte that is not text before the proxy's entry",
            vec![&b"\xe9, 198.51.100.7"[..]],
            "198.51.100.7",
        ),
        (
            "bytes that are not text on an earlier line",
            vec![&b"\xff\xfe, 203.0.113.9"[..], &b"198.51.100.8"[..]],
            "198.51.100.8",
        ),
    ] {
        let (kept, detail) = signed_with(&lines).await;
        assert_eq!(kept, want, "{what}: {detail}");
        assert!(
            detail.contains(&format!("address {want}; peer 10.0.0.5; ")),
            "{what}: {detail}"
        );
    }
    // The log shows the line that could not be read, not nothing.
    let (kept, detail) = signed_with(&[b"198.51.100.7, \xe9"]).await;
    assert_eq!(kept, "10.0.0.5");
    assert!(detail.contains("198.51.100.7"), "{detail}");
}

#[tokio::test]
async fn the_log_keeps_the_end_of_a_long_header_and_a_bar_cannot_imitate_a_second_line() {
    // A client line of 300 characters would push the proxy's own entry out of a log field cut at
    // the front: the cut keeps the end, which is the proxy's.
    let padded = format!("{}, 198.51.100.7", "9".repeat(300));
    let (kept, detail) = signed_with(&[padded.as_bytes()]).await;
    assert_eq!(kept, "198.51.100.7", "{detail}");
    let forwarded = detail.rsplit("forwarded ").next().unwrap();
    assert!(forwarded.ends_with("198.51.100.7"), "{detail}");
    assert!(forwarded.chars().count() <= 200, "{detail}");

    // A bar the client wrote is not the border between two lines.
    let (kept, detail) = signed_with(&[b"1.1.1.1 | 2.2.2.2, 9.9.9.9", b"198.51.100.7"]).await;
    assert_eq!(kept, "198.51.100.7");
    assert!(
        detail.ends_with("forwarded 1.1.1.1 / 2.2.2.2, 9.9.9.9 | 198.51.100.7"),
        "{detail}"
    );
}

// ---- what is known and written down ------------------------------------------------------

#[tokio::test]
async fn a_refused_update_that_moves_the_state_has_already_moved_it() {
    // Known and documented (docs/ISSUES.md O33): `update` with a state moves the state first, by
    // the action, and then the sign guards refuse the other fields of the call. The answer is a
    // 400 and the state has moved. Nothing is gained that the action did not allow anyway: no
    // evidence is written. If this ever changes (a guard before the action), update O33.
    let (f, _, path, _) = sent().await;
    let (s, v) = f
        .api(
            "sam",
            "/api/sign_request/update",
            json!({"id": f.req, "state": "declined", "signed_name": "Jane Forged"}),
        )
        .await;
    assert_eq!(s, 400, "{v}");
    assert!(err(&v).contains("signing process"), "{v}");
    let after = f.record("sign_request", &f.req);
    assert_eq!(after["state"], "declined");
    assert!(after.get("signed_name").is_none_or(J::is_null));
    // The links of a declined request answer as such.
    assert_eq!(f.get(&path, &ip(84)).await.status, 410);

    // The same for a request without a Document, which is still signed by hand.
    let f = fixture().await;
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
            json!({"title": "By template", "template_id": tpl, "signer_name": "Jane Test",
                   "signer_email": "jane@example.test"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let id = v["value"]["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api("mia", "/api/sign_request/send", json!({"id": id}))
        .await;
    assert_eq!(s, 200, "{v}");
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/update",
            json!({"id": id, "state": "signed", "signed_name": "Eve Forger"}),
        )
        .await;
    assert_eq!(s, 400, "{v}");
    let after = f.record("sign_request", &id);
    assert_eq!(after["state"], "signed");
    assert!(after.get("signed_name").is_none_or(J::is_null));
    assert_eq!(signed_count(&f), 0, "and nothing was logged as a signature");
}
