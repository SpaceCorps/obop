//! Upgrading `sign` from the version that is deployed today (0.2.0) to the one with the signer
//! door (0.2.1), on an instance that already has records in every state. The two fixtures are
//! copies of `packages/sign.toml` and `packages/documents.toml` as they were on `main` before
//! D92; they are not edited.

use super::*;
use obop_pkg::installer::Request as PkgRequest;
use obop_pkg::resolve::{Plan, StepAction};

const SIGN_OLD: &str = include_str!("../fixtures/sign-0.2.0.toml");
const DOCUMENTS_OLD: &str = include_str!("../fixtures/documents-0.3.0.toml");

struct Old {
    f: Fx,
    /// Every sign_request that was made, with its state.
    requests: Vec<(String, &'static str)>,
    template: String,
}

/// An instance on today's packages, with a template and a request in each state.
async fn old_instance() -> Old {
    let app = App::new();
    let auth = Arc::new(Auth::with_test_params(app.store.backend()));
    for text in [DOCUMENTS_OLD, SIGN_OLD] {
        app.installer.install(text, "upload", "test").unwrap();
    }
    for (name, groups) in [
        ("admin", vec!["platform.admin"]),
        (
            "mia",
            vec![
                "sign.user",
                "sign.manager",
                "documents.user",
                "documents.manager",
            ],
        ),
    ] {
        let g: Vec<String> = groups.iter().map(|g| g.to_string()).collect();
        auth.create_user(name, PW, &g).unwrap();
    }
    let state = obop_http::HttpState::new(app.dispatcher.clone(), auth.clone());
    state.set_trust_proxy(true);
    let router = obop_http::router(state.clone(), None);
    let mut f = Fx {
        router,
        auth,
        app,
        tokens: Default::default(),
        doc: String::new(),
        req: String::new(),
        file: String::new(),
        bytes: None,
        _scratch: None,
    };
    for name in ["admin", "mia"] {
        let (s, v) = f
            .api(
                "",
                "/api/auth/login",
                json!({"username": name, "password": PW}),
            )
            .await;
        assert_eq!(s, 200, "{v}");
        f.tokens.insert(
            if name == "admin" { "admin" } else { "mia" },
            v["token"].as_str().unwrap().to_string(),
        );
    }
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_template/create",
            json!({"name": "Old template (test)", "body": "made-up text"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let template = v["value"]["id"].as_str().unwrap().to_string();
    let mut requests = Vec::new();
    for (i, target) in ["draft", "sent", "signed", "declined", "expired"]
        .into_iter()
        .enumerate()
    {
        // The hand-typed hash is a legacy field that only an administrator may set after the
        // upgrade (D92), so the old records are made by one.
        let (s, v) = f
            .api(
                "admin",
                "/api/sign_request/create",
                json!({"title": format!("Old request {i}"), "template_id": template,
                       "signer_name": "Jane Test", "signer_email": "jane@example.test",
                       "document_hash": format!("{i:064x}")}),
            )
            .await;
        assert_eq!(s, 200, "{v}");
        let id = v["value"]["id"].as_str().unwrap().to_string();
        let path: &[&str] = match target {
            "draft" => &[],
            "sent" => &["send"],
            "signed" => &["send", "sign"],
            "declined" => &["send", "decline"],
            _ => &["send", "expire"],
        };
        for step in path {
            let (s, v) = f
                .api(
                    "mia",
                    &format!("/api/sign_request/{step}"),
                    json!({"id": id}),
                )
                .await;
            assert_eq!(s, 200, "{step}: {v}");
        }
        assert_eq!(f.record("sign_request", &id)["state"], target);
        requests.push((id, target));
    }
    Old {
        f,
        requests,
        template,
    }
}

fn snapshot(f: &Fx, requests: &[(String, &'static str)]) -> Vec<serde_json::Map<String, J>> {
    requests
        .iter()
        .map(|(id, _)| f.record("sign_request", id))
        .collect()
}

#[tokio::test]
async fn the_upgrade_from_0_2_0_is_additive_and_keeps_every_record() {
    let Old {
        f,
        requests,
        template,
    } = old_instance().await;
    let before = snapshot(&f, &requests);
    let before_template = f.record("sign_template", &template);

    // The plan the Modules page would show: one upgrade, nothing that breaks, nothing that runs
    // over the records.
    let new_text = Bundled.fetch("sign").unwrap();
    let (plan, _): (Plan, String) = f
        .app
        .installer
        .plan(PkgRequest::Manifest(&new_text))
        .unwrap();
    assert!(plan.ok(), "{:?}", plan.blockers);
    let steps: Vec<_> = plan.work().map(|s| (s.id.clone(), s.action)).collect();
    assert_eq!(steps, vec![("sign".to_string(), StepAction::Upgrade)]);
    let data = &plan.data["sign"];
    assert!(data.blockers.is_empty(), "{:?}", data.blockers);
    assert!(
        data.steps.is_empty(),
        "no data step is needed: {:?}",
        data.steps.len()
    );

    let out = f
        .app
        .installer
        .install_outcome(&new_text, "marketplace", "test")
        .unwrap();
    assert_eq!(out.root.version, "0.2.1");

    // Nothing was rewritten.
    assert_eq!(snapshot(&f, &requests), before);
    assert_eq!(f.record("sign_template", &template), before_template);
    // Every request is still readable through the commands, in its state, with the new fields
    // empty.
    for (id, state) in &requests {
        let (s, v) = f
            .api("mia", "/api/sign_request/get", json!({"id": id}))
            .await;
        assert_eq!(s, 200, "{v}");
        assert_eq!(v["value"]["state"], *state);
        assert!(v["value"].get("signed_name").is_none_or(J::is_null));
        assert!(v["value"].get("document_id").is_none_or(J::is_null));
    }
    // The old way still works on an old draft: it needs a template and an address.
    let draft = &requests[0].0;
    let (s, v) = f
        .api("mia", "/api/sign_request/send", json!({"id": draft}))
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(f.record("sign_request", draft)["state"], "sent");
    // The new model exists and is empty; the old requests have no link and none can be made for
    // them (they hold no copy of a document).
    assert_eq!(f.app.store.backend().count("sign_link").unwrap(), 0);
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_link/create",
            json!({"request_id": requests[1].0}),
        )
        .await;
    assert_ne!(s, 200, "{v}");
    // The signer door answers for a request that was sent the old way: nothing to open.
    let r = f
        .get(
            &format!("/sign/{}/{}", requests[1].0, "a".repeat(64)),
            &ip(150),
        )
        .await;
    assert_eq!(r.status, 404);
    // And a new-style request can be made and signed on the upgraded instance (the Documents
    // package is the old one, which is what 0.2.1 requires at least).
    let (s, v) = f
        .upload(
            "mia",
            "documents_document.attachment",
            "application/pdf",
            pdf(3),
        )
        .await;
    assert_eq!(s, 201, "{v}");
    let file = v["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api(
            "mia",
            "/api/documents_document/create",
            json!({"title": "NDA (test)", "kind": "contract", "confidentiality": "restricted",
                   "owner": "mia", "attachment": file}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let doc = v["value"]["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/create",
            json!({"title": "New", "signer_name": "Jane Test", "document_id": doc}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let new = v["value"]["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api("mia", "/api/sign_request/send_document", json!({"id": new}))
        .await;
    assert_eq!(s, 200, "{v}");
    let (_, path) = f.mint("mia", &new).await;
    let r = f
        .post(
            &path,
            &ip(151),
            &[("name", "Jane Test"), ("agree", "1"), ("doc", &file)],
        )
        .await;
    assert_eq!(r.status, 303, "{}", r.text());
    assert_eq!(f.record("sign_request", &new)["state"], "signed");
}

#[tokio::test]
async fn going_back_to_0_2_0_keeps_what_was_signed_and_a_second_upgrade_finds_it() {
    let Old { f, requests, .. } = old_instance().await;
    let new_text = Bundled.fetch("sign").unwrap();
    f.app
        .installer
        .install(&new_text, "marketplace", "test")
        .unwrap();
    // A signed document request and its link, made on 0.2.1.
    let (s, v) = f
        .upload(
            "mia",
            "documents_document.attachment",
            "application/pdf",
            pdf(4),
        )
        .await;
    assert_eq!(s, 201, "{v}");
    let file = v["id"].as_str().unwrap().to_string();
    let (_, v) = f
        .api(
            "mia",
            "/api/documents_document/create",
            json!({"title": "NDA (test)", "kind": "contract", "confidentiality": "internal",
                   "owner": "mia", "attachment": file}),
        )
        .await;
    let doc = v["value"]["id"].as_str().unwrap().to_string();
    let (_, v) = f
        .api(
            "mia",
            "/api/sign_request/create",
            json!({"title": "New", "signer_name": "Jane Test", "document_id": doc}),
        )
        .await;
    let new = v["value"]["id"].as_str().unwrap().to_string();
    f.api("mia", "/api/sign_request/send_document", json!({"id": new}))
        .await;
    let (link_id, path) = f.mint("mia", &new).await;
    let r = f
        .post(
            &path,
            &ip(152),
            &[("name", "Jane Test"), ("agree", "1"), ("doc", &file)],
        )
        .await;
    assert_eq!(r.status, 303);
    let signed = f.record("sign_request", &new);
    let link = f.record("sign_link", &link_id);

    // Rolling back goes through and keeps the records; it never loses the signature.
    let back = f
        .app
        .installer
        .rollback("sign", "test")
        .expect("the rollback goes through");
    assert_eq!(back.version, "0.2.0");
    assert_eq!(f.record("sign_request", &new), signed);
    assert_eq!(f.record("sign_link", &link_id), link);
    for (id, state) in &requests {
        assert_eq!(f.record("sign_request", id)["state"], *state);
    }
    // On 0.2.0 there is no door: a link of a real request answers 404, and the same link works
    // again when the package is back.
    let r = f.get(&path, &ip(153)).await;
    assert_eq!(r.status, 404);
    f.app
        .installer
        .install(&new_text, "marketplace", "test")
        .unwrap();
    assert_eq!(f.record("sign_request", &new), signed);
    assert_eq!(f.record("sign_link", &link_id), link);
    assert_eq!(f.get(&path, &ip(154)).await.status, 200);
}

#[tokio::test]
async fn the_new_binary_refuses_hand_typed_evidence_while_the_package_is_still_0_2_0() {
    // The wrappers act from the first start of the binary, before `module.upgrade`: the runbook
    // and D92 say so, and this pins it.
    let Old {
        f,
        requests,
        template,
    } = old_instance().await;
    let of = |state: &str| {
        requests
            .iter()
            .find(|(_, s)| *s == state)
            .map(|(id, _)| id.clone())
            .unwrap()
    };
    let (draft, signed) = (of("draft"), of("signed"));
    for (k, v) in [
        ("document_hash", "f".repeat(64)),
        ("signer_ip", "198.51.100.7".to_string()),
        ("signer_user_agent", "typed".to_string()),
        ("signature_reference", "typed:Jane".to_string()),
    ] {
        let (s, r) = f
            .api(
                "mia",
                "/api/sign_request/update",
                json!({"id": draft, k: v}),
            )
            .await;
        assert_eq!(s, 400, "{k}: {r}");
        assert!(
            r["error"]
                .as_str()
                .unwrap_or("")
                .contains("signing process"),
            "{k}: {r}"
        );
    }
    // A signed request is changed or deleted by an administrator only.
    for call in [
        (
            "/api/sign_request/update",
            json!({"id": signed, "title": "Edited"}),
        ),
        ("/api/sign_request/delete", json!({"id": signed})),
    ] {
        let (s, r) = f.api("mia", call.0, call.1.clone()).await;
        assert_eq!(s, 400, "{}: {r}", call.0);
        assert!(
            r["error"]
                .as_str()
                .unwrap_or("")
                .contains("cannot be changed or deleted"),
            "{}: {r}",
            call.0
        );
    }
    let (s, r) = f
        .api(
            "admin",
            "/api/sign_request/update",
            json!({"id": signed, "title": "Edited"}),
        )
        .await;
    assert_eq!(s, 200, "{r}");
    // The by-hand flow for a request with a template is as it was.
    let (s, r) = f
        .api(
            "mia",
            "/api/sign_request/create",
            json!({"title": "By hand", "template_id": template, "signer_name": "Jane Test",
                   "signer_email": "jane@example.test"}),
        )
        .await;
    assert_eq!(s, 200, "{r}");
    let id = r["value"]["id"].as_str().unwrap().to_string();
    for step in ["send", "sign"] {
        let (s, r) = f
            .api(
                "mia",
                &format!("/api/sign_request/{step}"),
                json!({"id": id}),
            )
            .await;
        assert_eq!(s, 200, "{step}: {r}");
    }
    assert_eq!(f.record("sign_request", &id)["state"], "signed");
}

#[tokio::test]
async fn the_package_does_not_install_without_a_documents_that_can_hold_the_file() {
    // 0.2.1 depends on documents >= 0.3: on an instance without it the plan brings it along.
    let app = App::new();
    let (plan, _) = app
        .installer
        .plan(PkgRequest::Manifest(&Bundled.fetch("sign").unwrap()))
        .unwrap();
    assert!(plan.ok(), "{:?}", plan.blockers);
    let ids: Vec<String> = plan.work().map(|s| s.id.clone()).collect();
    assert_eq!(ids, vec!["documents".to_string(), "sign".to_string()]);
}
