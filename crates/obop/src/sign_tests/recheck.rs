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
            // The guard of the module refuses it, in each spelling, and says so: it comes before
            // the checks of the engine (that '' is no reference, that sam may not use the other
            // Document), which would also have refused three of the six calls.
            assert!(
                err(&v).contains("cannot be changed after the request was sent"),
                "{who} {what}: {v}"
            );
            assert_eq!(
                f.record("sign_request", &f.req),
                before,
                "{who} {what}: nothing changed"
            );
        }
    }
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
    // Each of the three terms of the assertion is enough alone. The Document alone is the state of
    // the request in `the_document_of_a_template_request_sent_with_send_...` (no hash, no copy).
    // Here the Document is already cleared, and an administrator who also clears the copy, or
    // instead the hash (both are door-owned, so only she can), still meets the one that is left:
    // the hash with no template, and the copy.
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

    // After the signature every one of the eight keys is refused the same way, by the door's keys
    // (the message is theirs). The other layer, the rule that a signed request takes no edit, is
    // pinned by `a_signed_request_is_neither_changed_nor_deleted_by_staff` (hardening.rs) and by the
    // signed-request cases of `an_agent_token_that_holds_the_admin_group_...`.
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
    let (mut f, link_id, path, _) = sent().await;
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
    // A request that was signed through a link is still referenced by that link, which refuses a
    // delete before the wrapper of the module is asked: the reason is the reference.
    let (s, v) = f
        .api("adminbot", "/api/sign_request/delete", json!({"id": f.req}))
        .await;
    assert_eq!(s, 400, "{v}");
    assert!(err(&v).contains("still referenced by sign_link"), "{v}");
    assert_eq!(f.record("sign_request", &f.req), signed);

    // The delete wrapper of the module is reached with a request that was signed by hand (no
    // link refers to it): the agent's call is a proposal, and when the person approves it the
    // wrapper still asks who the caller is. The agent is no administrator, so the signed request
    // stays. (Pinned: a wrapper that asked only for the group would let this delete go through.)
    let by_hand = hand_signed_request(&f).await;
    let (s, v, proposed) =
        agent_call_then_approved_by_root(&f, "/api/sign_request/delete", json!({"id": by_hand}))
            .await;
    assert!(proposed, "the wrapper must be reached by an approval: {v}");
    assert_eq!(s, 400, "{v}");
    assert!(err(&v).contains("cannot be changed or deleted"), "{v}");
    assert_eq!(f.record("sign_request", &by_hand)["state"], "signed");

    // The links: the agent cannot un-revoke, re-point, lengthen or delete one either, although it
    // holds the group. The wrapper answers an agent's call at once (or, were it let through, the
    // call would be a proposal that the person approves: the helper follows both).
    let link = f.record("sign_link", &link_id);
    for body in [
        json!({"id": link_id, "expires_at": "2099-01-01T00:00:00.000000Z"}),
        json!({"id": link_id, "used_at": null}),
    ] {
        let (s, v, _) =
            agent_call_then_approved_by_root(&f, "/api/sign_link/update", body.clone()).await;
        assert_eq!(s, 400, "{body}: {v}");
        assert!(err(&v).contains("sign_link"), "{body}: {v}");
        assert_eq!(f.record("sign_link", &link_id), link, "{body}");
    }
    let (s, v, _) =
        agent_call_then_approved_by_root(&f, "/api/sign_link/delete", json!({"id": link_id})).await;
    assert_eq!(s, 400, "{v}");
    assert!(
        err(&v).contains("kept as a record of what was issued"),
        "{v}"
    );
    assert_eq!(f.record("sign_link", &link_id), link);

    // The person who holds the group is an administrator, as the documents say.
    let (s, v) = f
        .api(
            "root",
            "/api/sign_request/update",
            json!({"id": f.req, "title": "Retitled by an administrator"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let (s, v) = f
        .api(
            "root",
            "/api/sign_link/update",
            json!({"id": link_id, "expires_at": "2099-01-01T00:00:00.000000Z"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
}

/// The call of the admin-scoped agent. When the platform refuses it outright that is the answer;
/// else the call is a proposal (202) that `root`, who holds the group herself, approves, and the
/// answer is that of the approval: the module's own wrapper decides then, with the agent as the
/// caller. (`proposed` says which of the two happened.)
async fn agent_call_then_approved_by_root(f: &Fx, path: &str, body: J) -> (u16, J, bool) {
    let (s, v) = f.api("adminbot", path, body).await;
    if s != 202 {
        return (s, v, false);
    }
    assert_eq!(v["approval_required"], json!(true), "{v}");
    let plan = v["plan_id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api("root", &format!("/api/approve/{plan}"), json!({}))
        .await;
    (s, v, true)
}

/// A request without a Document, made from a template and signed by hand (the way a request has
/// always been signed there): no link refers to it.
async fn hand_signed_request(f: &Fx) -> String {
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_template/create",
            json!({"name": "By hand (test)", "body": "text"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let tpl = v["value"]["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/create",
            json!({"title": "By hand", "template_id": tpl, "signer_name": "Jane Test",
                   "signer_email": "jane@example.test"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let id = v["value"]["id"].as_str().unwrap().to_string();
    for call in ["send", "sign"] {
        let (s, v) = f
            .api(
                "mia",
                &format!("/api/sign_request/{call}"),
                json!({"id": id}),
            )
            .await;
        assert_eq!(s, 200, "{call}: {v}");
    }
    assert_eq!(f.record("sign_request", &id)["state"], "signed");
    id
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
    post_from_peer_with(f, path, lines, peer, name, &[]).await
}

/// As `post_from_peer`, with more headers.
async fn post_from_peer_with(
    f: &Fx,
    path: &str,
    lines: &[&[u8]],
    peer: &str,
    name: &str,
    more: &[(&str, &str)],
) -> Resp {
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
    for (k, v) in more {
        b = b.header(*k, *v);
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

#[tokio::test]
async fn the_language_of_the_consent_id_is_the_one_the_post_names_not_a_proof_of_the_page() {
    // Known and documented (docs/SIGN.md "The consent wording and its id", docs/ISSUES.md O2):
    // the id `v1-draft/<lang>/<wording>` has the language the POST names (a client can name
    // another of the seven than the page it was shown); a name that is not one of the seven, or
    // none, is resolved as a page request is: the browser's `Accept-Language`, then the instance's
    // default language, then English. The id is not evidence of which page was shown.
    // The order, as the table below pins it: the `lang` field of the form when the form has one
    // (an empty one or one that is not of the seven does NOT fall back to the address's: it goes
    // on to the browser), else the `lang` of the post's address (`?lang=`), else the browser's
    // `Accept-Language`, else the instance default, else English.
    struct Case {
        field: Option<&'static str>,
        address: Option<&'static str>,
        accept: Option<&'static str>,
        default: Option<&'static str>,
        want: &'static str,
    }
    let c = |field, address, accept, default, want| Case {
        field,
        address,
        accept,
        default,
        want,
    };
    let cases = [
        c(Some("en"), None, None, None, "en"),
        c(Some("de"), None, None, None, "de"),
        c(Some("sv"), None, None, None, "sv"),
        c(Some("<script>"), None, None, None, "en"),
        c(Some(""), None, None, None, "en"),
        c(Some("klingon"), None, None, None, "en"),
        c(None, None, None, None, "en"),
        // The browser's language, when the POST names none that exists.
        c(Some("klingon"), None, Some("de-DE,de;q=0.9"), None, "de"),
        c(Some(""), None, Some("fr"), None, "fr"),
        c(Some("<script>"), None, Some("sv"), None, "sv"),
        // The POST's own language beats the browser's.
        c(Some("de"), None, Some("fr"), None, "de"),
        // The instance default, when neither names one; the browser outranks it.
        c(Some("klingon"), None, None, Some("pl"), "pl"),
        c(Some(""), None, Some("fr"), Some("pl"), "fr"),
        c(Some(""), None, Some("ja, zh;q=0.9"), Some("pl"), "pl"),
        c(None, None, None, Some("pl"), "pl"),
        // The address's `?lang=`, for a form with no `lang` field at all: it beats the browser
        // and the default.
        c(None, Some("de"), Some("fr"), Some("pl"), "de"),
        c(None, Some("de"), None, None, "de"),
        c(None, Some("klingon"), Some("sv"), Some("pl"), "sv"),
        c(None, Some("klingon"), None, Some("pl"), "pl"),
        // A form field that is present wins over the address, and one that is present but empty
        // or unknown does not hand over to it.
        c(Some("pl"), Some("de"), None, None, "pl"),
        c(Some("klingon"), Some("de"), Some("fr"), None, "fr"),
        c(Some(""), Some("de"), Some("fr"), Some("pl"), "fr"),
        c(Some(""), Some("de"), None, Some("pl"), "pl"),
    ];
    for k in cases {
        let (f, _, path, _) = sent().await;
        f.state.set_default_language(k.default);
        let mut extra = vec![("origin", ORIGIN)];
        if let Some(a) = k.accept {
            extra.push(("accept-language", a));
        }
        let mut form = vec![
            ("name", "Jane Test"),
            ("agree", "1"),
            ("doc", f.file.as_str()),
        ];
        if let Some(l) = k.field {
            form.push(("lang", l));
        }
        let to = match k.address {
            Some(a) => format!("{path}?lang={a}"),
            None => path.clone(),
        };
        let r = f.post_with(&to, &ip(90), &form, &extra).await;
        let what = (k.field, k.address, k.accept, k.default);
        assert_eq!(r.status, 303, "{what:?}: {}", r.text());
        assert_eq!(
            f.record("sign_request", &f.req)["consent_version"],
            obop_http::sign_text::consent_version_for(k.want),
            "{what:?}"
        );
        // The page after the signature is in the language that was recorded.
        assert_eq!(
            r.h("location"),
            format!("{path}?lang={}", k.want),
            "{what:?}"
        );
    }
}

#[tokio::test]
async fn a_proxy_entry_that_is_not_an_address_is_not_replaced_by_another_header() {
    // When the proxy's entry cannot be used the peer is the address (docs/SIGN.md). The headers
    // other proxies and platforms use for the client's address are not a second place to look:
    // a client that sends one cannot take the place of the peer, with a good line or a bad one.
    let others = [
        ("x-real-ip", "203.0.113.50"),
        ("cf-connecting-ip", "203.0.113.51"),
        ("forwarded", "for=203.0.113.52"),
        ("true-client-ip", "203.0.113.53"),
        ("x-client-ip", "203.0.113.54"),
    ];
    for (what, line, want) in [
        ("a name", &b"198.51.100.7, client-a"[..], "10.0.0.5"),
        ("no entry", &b""[..], "10.0.0.5"),
        ("a good entry", &b"198.51.100.7"[..], "198.51.100.7"),
    ] {
        let (f, _, path, _) = sent().await;
        let r = post_from_peer_with(&f, &path, &[line], "10.0.0.5", "Jane Test", &others).await;
        assert_eq!(r.status, 303, "{what}: {}", r.text());
        assert_eq!(
            f.record("sign_request", &f.req)["signer_ip"],
            want,
            "{what}"
        );
        let detail = f.events_of("sign_signed")[0]["detail"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(!detail.contains("203.0.113.5"), "{what}: {detail}");
    }
}

#[tokio::test]
async fn the_dates_of_a_request_are_free_for_a_plain_user_to_set_today() {
    // Known and documented (docs/ISSUES.md O34): `sent_on` and `signed_on` are not door-owned, so
    // a sign.user can write them on any request that is not signed. They are labels for lists,
    // not evidence (the evidence is `signed_at`, which only the door writes). The fix that is
    // proposed there (add both to the door's keys) would fail this test, which is the point of
    // having it: change O34 with it.
    let (f, _, _, _) = sent().await;
    let (s, v) = f
        .api(
            "sam",
            "/api/sign_request/update",
            json!({"id": f.req, "sent_on": "2001-01-01", "signed_on": "2001-01-02"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let r = f.record("sign_request", &f.req);
    assert_eq!(r["sent_on"], "2001-01-01");
    assert_eq!(r["signed_on"], "2001-01-02");
    assert!(r.get("signed_at").is_none_or(J::is_null), "no evidence");
    assert_eq!(r["state"], "sent");
}

#[tokio::test]
async fn a_closed_request_keeps_its_document_too() {
    // Declined and expired are not draft: the Document of the request is as fixed as it is while
    // the request is sent (a closed request is evidence of what was asked, and nobody gets a
    // Document back by closing it first).
    for close in ["decline", "expire"] {
        let (f, _, _, _) = sent().await;
        let (s, v) = f
            .api(
                "sam",
                &format!("/api/sign_request/{close}"),
                json!({"id": f.req}),
            )
            .await;
        assert_eq!(s, 200, "{close}: {v}");
        let before = f.record("sign_request", &f.req);
        assert_ne!(before["state"], "sent", "{close}");
        for who in ["sam", "mia"] {
            let (s, v) = f
                .api(
                    who,
                    "/api/sign_request/update",
                    json!({"id": f.req, "document_id": null}),
                )
                .await;
            assert_eq!(s, 400, "{close} {who}: {v}");
            assert!(
                err(&v).contains("cannot be changed after the request was sent"),
                "{close} {who}: {v}"
            );
            assert_eq!(f.record("sign_request", &f.req), before, "{close} {who}");
        }
    }
}

/// An agent token of `owner` with these scopes, kept under `name`.
async fn agent_with(f: &mut Fx, owner: &str, scopes: &[&str], name: &'static str) {
    let (s, v) = f
        .api(
            owner,
            "/api/auth/agent-token",
            json!({"scopes": scopes, "label": name}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    f.tokens
        .insert(name, v["token"].as_str().unwrap().to_string());
}

#[tokio::test]
async fn an_agent_token_that_holds_sign_manager_revokes_a_link_and_one_with_sign_user_ends_a_request()
 {
    // docs/SIGN.md "Known limits" and O19: no approval is asked and no entry is written when an
    // agent token revokes a link (it needs `sign.manager`) or declines or expires a sent request
    // (it needs `sign.user`), so a prompt-injected agent can stop a signature that was handed
    // over. The tests state what is documented; if the owner wants a person to decide, the
    // actions get `approval = true` and these change with them.
    let (mut f, link_id, path, _) = sent().await;
    agent_with(&mut f, "mia", &["sign.user", "sign.manager"], "manager_bot").await;
    agent_with(&mut f, "mia", &["sign.user"], "user_bot").await;
    let events = f.events().len();
    let revoke = json!({"id": link_id, "revoked_at": "2001-01-01T00:00:00Z"});
    // With `sign.user` alone it cannot even see the link.
    let (s, _) = f
        .api("user_bot", "/api/sign_link/update", revoke.clone())
        .await;
    assert_ne!(s, 200);
    assert_eq!(f.get(&path, &ip(83)).await.status, 200, "still open");
    // With `sign.manager` it revokes, at once, with the server's time.
    let (s, v) = f.api("manager_bot", "/api/sign_link/update", revoke).await;
    assert_eq!(s, 200, "{v}");
    let stamped = f.record("sign_link", &link_id)["revoked_at"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(!stamped.starts_with("2001"), "{stamped}");
    assert_eq!(f.get(&path, &ip(83)).await.status, 410, "the link is dead");
    assert_eq!(f.events().len(), events, "and nothing was logged");
    // Declining a sent request needs `sign.user` only, and no approval either.
    let (f, _, path, _) = sent().await;
    let mut f = f;
    agent_with(&mut f, "mia", &["sign.user"], "user_bot").await;
    let events = f.events().len();
    let (s, v) = f
        .api(
            "user_bot",
            "/api/sign_request/decline",
            json!({"id": f.req}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(f.record("sign_request", &f.req)["state"], "declined");
    assert_eq!(f.get(&path, &ip(84)).await.status, 410, "its link is dead");
    assert_eq!(f.events().len(), events, "and nothing was logged");
}

/// One write of a door-owned key: `op` is `update` (of `req`), `create` or `bulk` (one row).
async fn probe(f: &Fx, who: &str, op: &str, req: &str, key: &str, value: &J) -> (u16, String) {
    let (path, body) = match op {
        "update" => ("/api/sign_request/update", json!({"id": req, key: value})),
        "create" => (
            "/api/sign_request/create",
            json!({"title": "Probe", "signer_name": "Jane Test", key: value}),
        ),
        _ => (
            "/api/sign_request/bulk_create",
            json!({"rows": [{"title": "Probe", "signer_name": "Jane Test", key: value}]}),
        ),
    };
    let (s, v) = f.api(who, path, body).await;
    (s, err(&v))
}

/// As `probe`, with the whole answer.
async fn probe_raw(f: &Fx, who: &str, op: &str, req: &str, key: &str, value: &J) -> (u16, J) {
    let (path, body) = match op {
        "update" => ("/api/sign_request/update", json!({"id": req, key: value})),
        "create" => (
            "/api/sign_request/create",
            json!({"title": "Probe", "signer_name": "Jane Test", key: value}),
        ),
        _ => (
            "/api/sign_request/bulk_create",
            json!({"rows": [{"title": "Probe", "signer_name": "Jane Test", key: value}]}),
        ),
    };
    f.api(who, path, body).await
}

#[tokio::test]
async fn a_guess_at_the_evidence_is_refused_the_same_whether_it_is_right_or_wrong() {
    // A caller who cannot read a field of the evidence **in clear** must not learn it by sending
    // a guess: a value equal to the stored one used to be passed through (200) and a wrong one
    // refused (400), an equality oracle on the signer's address, name and hash. The refusal is
    // now for the key, whatever the value. A reader of the field keeps the pass-through (a form
    // saves the whole record). A reader is a person (or a token) that holds the field's group
    // and is not masked: an agent that is shown mask tokens is not one.
    let (mut f, _, path, _) = sent().await;
    let r = f.post(&path, &ip(82), &good_form(&f, "Jane Test")).await;
    assert_eq!(r.status, 303, "{}", r.text());
    let signed = f.record("sign_request", &f.req);
    // The state of the real NDA first: the Document (restricted, ann's) still holds the file that
    // the request holds, and bob, sam and noc cannot see the Document. The only thing that makes
    // a right guess at `document_file` answer like a wrong one is then the row rule of the
    // Document, which the guard on files applies to the owner entry of the Document. The whole
    // answer is compared, for update, create and a bulk row.
    let wrong_id = json!("0".repeat(64));
    for who in ["bob", "sam", "noc"] {
        for op in ["update", "create", "bulk"] {
            let right = probe_raw(
                &f,
                who,
                op,
                &f.req,
                "document_file",
                &signed["document_file"],
            )
            .await;
            let wrong = probe_raw(&f, who, op, &f.req, "document_file", &wrong_id).await;
            assert_eq!(
                right, wrong,
                "{who} {op}: the Document still holds the file"
            );
            assert_eq!(right.0, 400, "{who} {op}: {:?}", right.1);
            // A bulk row meets the door's refusal of the key before the check of the file.
            let said = if op == "bulk" {
                "cannot be set by hand"
            } else {
                "must be the id of an uploaded file"
            };
            assert!(err(&right.1).contains(said), "{who} {op}: {:?}", right.1);
        }
    }
    assert_eq!(f.record("sign_request", &f.req), signed, "nothing changed");
    // The Document gets another file, so that the signed copy is held by the request alone: its
    // id is not one that anybody can read through the Document.
    let (s, v) = f
        .upload(
            "ann",
            "documents_document.attachment",
            "application/pdf",
            pdf(7),
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
    // The agents: a manager's, a manager's narrowed to `sign.user`, and one that holds the
    // administrator's group (an agent is never an administrator, but it holds every group).
    agent_with(&mut f, "mia", &["sign.user", "sign.manager"], "manager_bot").await;
    agent_with(&mut f, "mia", &["sign.user"], "narrowed_bot").await;
    agent_with(
        &mut f,
        "root",
        &[
            "platform.admin",
            "sign.user",
            "sign.manager",
            "documents.user",
        ],
        "admin_bot",
    )
    .await;

    let personal = [
        "signed_name",
        "signer_ip",
        "signer_user_agent",
        "signature_reference",
    ];
    let all: Vec<&str> = DOOR_KEYS.to_vec();
    // (who, the keys of the eight that they cannot read in clear)
    let cannot: [(&str, Vec<&str>); 5] = [
        ("sam", all.clone()),
        ("narrowed_bot", all.clone()),
        // Holds the group of seven, but is shown the four personal ones as tokens; nobody has
        // the group of the held copy.
        ("manager_bot", [&personal[..], &["document_file"]].concat()),
        // Holds the administrator's group, but an agent gets only the groups its scopes list
        // (`Principal::allows`): not the held copy's, and the four personal ones as tokens.
        ("admin_bot", [&personal[..], &["document_file"]].concat()),
        // A person who holds `sign.manager` reads seven; the held copy's group is nobody's.
        ("mia", vec!["document_file"]),
    ];
    let wrong = |k: &str| -> J {
        match k {
            "signed_at" => json!("2001-01-01T00:00:00.000000Z"),
            "signer_ip" => json!("203.0.113.77"),
            // A well-formed id that no file has.
            "document_hash" | "document_file" => json!("0".repeat(64)),
            _ => json!("not it"),
        }
    };
    for (who, hidden) in &cannot {
        for key in DOOR_KEYS {
            let right = signed[key].clone();
            let hidden = hidden.contains(&key);
            // Update: the stored value and a wrong one.
            let got = [
                probe(&f, who, "update", &f.req, key, &right).await,
                probe(&f, who, "update", &f.req, key, &wrong(key)).await,
            ];
            if hidden {
                assert_eq!(got[0].0, 400, "{who} update {key} right: {got:?}");
                assert_eq!(got[0], got[1], "{who} {key}: the answers differ");
                // The door's own words; the held copy is stopped earlier, by the guard on files,
                // which answers a file the caller may not read as it answers one that is not.
                let want = if key == "document_file" {
                    "must be the id of an uploaded file"
                } else {
                    "signing process"
                };
                assert!(got[0].1.contains(want), "{who} {key}: {got:?}");
            } else {
                // A reader may send back what she was shown; another value is refused.
                assert_eq!(got[0].0, 200, "{who} sends back {key}: {got:?}");
                assert_eq!(got[1].0, 400, "{who} sends another {key}: {got:?}");
            }
            assert_eq!(f.record("sign_request", &f.req), signed, "{who} {key}");
            // Create and a bulk row, for the stored value, a wrong one and the two kinds of
            // empty: refused for a caller who cannot read the field.
            if hidden {
                for op in ["create", "bulk"] {
                    let a = probe(&f, who, op, &f.req, key, &right).await;
                    let b = probe(&f, who, op, &f.req, key, &wrong(key)).await;
                    assert_eq!(a.0, 400, "{who} {op} {key} right: {a:?}");
                    assert_eq!(a, b, "{who} {op} {key}: the answers differ");
                    for blank in [json!(""), J::Null] {
                        let c = probe(&f, who, op, &f.req, key, &blank).await;
                        assert_eq!(c.0, 400, "{who} {op} {key}={blank}: {c:?}");
                    }
                }
            }
        }
    }
    assert_eq!(count(&f, "sign_request"), 1, "nothing was made");
    // An empty value in a new row is let through for a reader and not for `sam` (the empty
    // pass-through is a reader's).
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/create",
            json!({"title": "Blank", "signer_name": "Jane Test", "signer_ip": ""}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/bulk_create",
            json!({"rows": [{"title": "Blank", "signer_name": "Jane Test", "signer_ip": ""}]}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
}
