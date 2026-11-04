//! The checks of the independent review of D92 (2026-11-03): the bulk import and the manual
//! action cannot make a signature, the address comes from the proxy's entry, the shape of the
//! consent, what staff text may carry, and the guarantees the documents state that no other test
//! pinned. Synthetic bytes and made-up names only.

use super::rules::DOOR_KEYS;
use super::*;
use obop_commands::sign::{digest_of_request, sha256_hex};
use obop_storage::blob::BlobBytes;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(super) fn err(v: &J) -> String {
    v["error"].as_str().unwrap_or("").to_string()
}

pub(super) fn good_form<'a>(f: &'a Fx, name: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![
        ("name", name),
        ("agree", "1"),
        ("doc", &f.file),
        ("lang", "en"),
    ]
}

pub(super) fn signed_count(f: &Fx) -> usize {
    f.events_of("sign_signed").len()
}

pub(super) fn count(f: &Fx, model: &str) -> usize {
    f.app.store.backend().count(model).unwrap()
}

fn now_micros() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros() as i64
}

pub(super) fn fields(v: J) -> serde_json::Map<String, J> {
    v.as_object().cloned().unwrap()
}

/// An agent token of `owner` with the groups of the signing work.
pub(super) async fn agent_for(f: &mut Fx, owner: &str) {
    let (s, v) = f
        .api(
            owner,
            "/api/auth/agent-token",
            json!({"scopes": ["sign.user", "sign.manager", "documents.user", "documents.manager"], "label": "bot"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    f.tokens
        .insert("agent", v["token"].as_str().unwrap().to_string());
}

// ---- P1: the bulk import -----------------------------------------------------------------

#[tokio::test]
async fn a_csv_import_cannot_carry_evidence_columns_in_any_spelling() {
    let f = fixture().await;
    let row = "Forged,Jane Test,Jane Forged\n";
    let cases: Vec<(&str, String)> = vec![
        (
            "plain header",
            format!("title,signer_name,signed_name\n{row}"),
        ),
        (
            "a blank line first",
            format!("\ntitle,signer_name,signed_name\n{row}"),
        ),
        (
            "a lone CR after the header",
            format!("title,signer_name,signed_name\r{row}"),
        ),
        (
            "a quoted header with an inner space",
            format!("title,signer_name,\" signed_name\"\n{row}"),
        ),
        (
            "a quoted header that starts with a line break",
            format!("title,signer_name,\"\nsigned_name\"\n{row}"),
        ),
        (
            "a byte order mark and a blank line",
            format!("\u{feff}\ntitle,signer_name,signed_name\n{row}"),
        ),
        (
            "semicolons",
            "title;signer_name;signed_at\nForged;Jane Test;2026-11-03T10:00:00Z\n".to_string(),
        ),
        (
            "the column first",
            "signer_ip,title,signer_name\n198.51.100.7,Forged,Jane Test\n".to_string(),
        ),
    ];
    let before = count(&f, "sign_request");
    for who in ["sam", "mia"] {
        for (what, csv) in &cases {
            let (s, v) = f
                .api(who, "/api/sign_request/bulk_create", json!({"csv": csv}))
                .await;
            assert_eq!(s, 400, "{who}, {what}: {v}");
            assert!(err(&v).contains("signing process"), "{who}, {what}: {v}");
        }
    }
    assert_eq!(count(&f, "sign_request"), before, "nothing was made");
    // An ordinary file still imports, and an administrator may import the columns.
    let (s, v) = f
        .api(
            "sam",
            "/api/sign_request/bulk_create",
            json!({"csv": "title,signer_name\nPlain row,Jane Test\n"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let (s, v) = f
        .api(
            "admin",
            "/api/sign_request/bulk_create",
            json!({"csv": format!("title,signer_name,signed_name\n{row}")}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(count(&f, "sign_request"), before + 2);
}

#[tokio::test]
async fn a_csv_import_cannot_start_a_request_that_signs_without_the_door() {
    let f = fixture().await;
    // The chain: import a draft that carries evidence, send it, mark it signed. The first step is
    // refused (by the signing guard: `document_file` is left out of the file because the importer
    // refuses a file column on its own account, which would hide whether the guard is there), so
    // there is nothing to send. What the same import may carry (the Document) is imported, sent,
    // and cannot be signed by hand either.
    let keys: Vec<&str> = DOOR_KEYS
        .iter()
        .copied()
        .filter(|k| *k != "document_file")
        .collect();
    let cells: Vec<String> = keys
        .iter()
        .map(|k| match *k {
            "signed_at" => "2026-11-03T10:00:00.000000Z".to_string(),
            "document_hash" => f.file.clone(),
            "signer_ip" => "198.51.100.7".to_string(),
            _ => "forged".to_string(),
        })
        .collect();
    let csv = format!(
        "\ntitle,signer_name,document_id,{}\nForged,Jane Test,{},{}\n",
        keys.join(","),
        f.doc,
        cells.join(",")
    );
    let before = count(&f, "sign_request");
    let (s, v) = f
        .api("sam", "/api/sign_request/bulk_create", json!({"csv": csv}))
        .await;
    assert_eq!(s, 400, "{v}");
    assert!(err(&v).contains("signing process"), "{v}");
    assert_eq!(count(&f, "sign_request"), before);

    // The part of it that is allowed: a request about the Document. It is sent, and by hand it
    // cannot be signed; only the signer, at the door, signs it.
    let csv = format!(
        "title,signer_name,document_id\nImported,Jane Test,{}\n",
        f.doc
    );
    let (s, v) = f
        .api("mia", "/api/sign_request/bulk_create", json!({"csv": csv}))
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(count(&f, "sign_request"), before + 1);
    let imported = f
        .app
        .store
        .backend()
        .list("sign_request")
        .unwrap()
        .into_iter()
        .find(|(_, r)| r.get("title").and_then(J::as_str) == Some("Imported"))
        .map(|(id, _)| id)
        .expect("the imported request");
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/send_document",
            json!({"id": imported}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    for body in [
        json!({"id": imported}),
        json!({"id": imported, "state": "signed"}),
    ] {
        let path = if body.get("state").is_some() {
            "/api/sign_request/update"
        } else {
            "/api/sign_request/sign"
        };
        let (s, v) = f.api("mia", path, body.clone()).await;
        assert_eq!(s, 400, "{path} {body}: {v}");
        assert_eq!(f.record("sign_request", &imported)["state"], "sent");
    }
    assert_eq!(count(&f, "sign_link"), 0);
    assert_eq!(signed_count(&f), 0);
}

// ---- P1: the manual action ---------------------------------------------------------------

#[tokio::test]
async fn a_request_about_a_document_is_signed_at_the_door_only() {
    let (f, _, path, _) = sent().await;
    let events = f.events().len();
    let moved = |f: &Fx| f.record("sign_request", &f.req)["state"].clone();
    // The action by hand, as a manager and as an administrator.
    for who in ["mia", "admin"] {
        let (s, v) = f
            .api(who, "/api/sign_request/sign", json!({"id": f.req}))
            .await;
        assert_eq!(s, 400, "{who}: {v}");
        assert!(err(&v).contains("through the signing link"), "{who}: {v}");
        assert_eq!(moved(&f), "sent", "{who}");
    }
    // The state moved by an update runs the action first; it is refused before anything
    // commits, with and without evidence in the same call.
    for body in [
        json!({"id": f.req, "state": "signed"}),
        json!({"id": f.req, "state": "signed", "signed_name": "Jane Forged"}),
    ] {
        let (s, v) = f.api("mia", "/api/sign_request/update", body.clone()).await;
        assert_eq!(s, 400, "{body}: {v}");
        assert_eq!(moved(&f), "sent", "{body}");
    }
    assert_eq!(f.events().len(), events, "nothing was logged");
    assert!(
        f.record("sign_request", &f.req)
            .get("signed_at")
            .is_none_or(J::is_null)
    );
    // Nobody can run it through an agent either: the proposal is approved by a person, and the
    // refusal is made when it runs.
    let mut f = f;
    agent_for(&mut f, "mia").await;
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
    assert_eq!(moved(&f), "sent");
    // The signer's page is still the form, and the signer signs.
    assert!(f.get(&path, &ip(30)).await.text().contains("<form"));
    let r = f.post(&path, &ip(30), &good_form(&f, "Jane Test")).await;
    assert_eq!(r.status, 303, "{}", r.text());
    assert_eq!(moved(&f), "signed");
    assert_eq!(signed_count(&f), 1);
}

#[tokio::test]
async fn a_request_that_is_not_about_a_document_is_still_signed_by_hand() {
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
    let mut ids = Vec::new();
    for title in ["By action", "By update"] {
        let (s, v) = f
            .api(
                "mia",
                "/api/sign_request/create",
                json!({"title": title, "template_id": tpl, "signer_name": "Jane Test",
                       "signer_email": "jane@example.test"}),
            )
            .await;
        assert_eq!(s, 200, "{v}");
        let id = v["value"]["id"].as_str().unwrap().to_string();
        let (s, v) = f
            .api("mia", "/api/sign_request/send", json!({"id": id}))
            .await;
        assert_eq!(s, 200, "{v}");
        ids.push(id);
    }
    // A plain sign.user records no signature by hand, by the action or by moving the state.
    for (path, body) in [
        ("/api/sign_request/sign", json!({"id": ids[0]})),
        (
            "/api/sign_request/update",
            json!({"id": ids[0], "state": "signed"}),
        ),
    ] {
        let (s, v) = f.api("sam", path, body.clone()).await;
        assert_ne!(s, 200, "{path} {body}: {v}");
        assert_eq!(
            f.record("sign_request", &ids[0])["state"],
            "sent",
            "{path} {body}"
        );
    }
    let (s, v) = f
        .api("mia", "/api/sign_request/sign", json!({"id": ids[0]}))
        .await;
    assert_eq!(s, 200, "{v}");
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/update",
            json!({"id": ids[1], "state": "signed"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    for id in &ids {
        assert_eq!(f.record("sign_request", id)["state"], "signed");
    }
}

#[tokio::test]
async fn a_request_without_a_signing_time_is_not_shown_as_signed_at_a_time() {
    let (f, link_id, path, _) = sent().await;
    // An administrator moved it by writing the state, with no evidence: the link is over, and the
    // page says nothing about a time.
    f.app
        .store
        .backend()
        .update("sign_request", &f.req, &fields(json!({"state": "signed"})))
        .unwrap();
    let r = f.get(&path, &ip(31)).await;
    assert_eq!(r.status, 410, "{}", r.text());
    assert!(!r.text().contains("<form") && !r.text().contains("(UTC)"));
    assert_eq!(
        f.get(&format!("{path}/document.pdf"), &ip(31)).await.status,
        410
    );
    assert_eq!(
        f.post(&path, &ip(31), &good_form(&f, "Jane Test"))
            .await
            .status,
        410
    );
    assert!(
        f.record("sign_link", &link_id)
            .get("used_at")
            .is_none_or(J::is_null)
    );
}

// ---- P2: the address, the tick, the life of a link, staff text ---------------------------

#[tokio::test]
async fn the_address_is_the_last_entry_of_the_last_header_line() {
    // One line, several entries: the proxy's is the last; the entries before it are the client's.
    let (f, _, path, _) = sent().await;
    let r = f
        .post(
            &path,
            "203.0.113.9, 198.51.100.7",
            &good_form(&f, "Jane Test"),
        )
        .await;
    assert_eq!(r.status, 303, "{}", r.text());
    assert_eq!(
        f.record("sign_request", &f.req)["signer_ip"],
        "198.51.100.7"
    );
    let detail = f.events_of("sign_signed")[0]["detail"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(detail.contains("address 198.51.100.7"), "{detail}");
    assert!(
        detail.contains("forwarded 203.0.113.9, 198.51.100.7"),
        "{detail}"
    );

    // Two lines: a proxy that adds its own line leaves the client's first; the last line counts.
    let (f, _, path, _) = sent().await;
    let r = f
        .post_with(
            &path,
            "203.0.113.9",
            &good_form(&f, "Jane Test"),
            &[
                ("origin", ORIGIN),
                ("x-forwarded-for", "192.0.2.1, 198.51.100.8"),
            ],
        )
        .await;
    assert_eq!(r.status, 303, "{}", r.text());
    assert_eq!(
        f.record("sign_request", &f.req)["signer_ip"],
        "198.51.100.8"
    );
    let detail = f.events_of("sign_signed")[0]["detail"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        detail.contains("forwarded 203.0.113.9 | 192.0.2.1, 198.51.100.8"),
        "{detail}"
    );
}

#[tokio::test]
async fn without_a_trusted_proxy_the_forwarded_header_is_ignored() {
    let (f, _, path, _) = sent().await;
    let router = f.router_without_proxy();
    let body = format!("name=Jane+Test&agree=1&doc={}", f.file);
    let r = send(
        &router,
        Request::post(&path)
            .header("host", HOST)
            .header("origin", ORIGIN)
            .header("content-type", "application/x-www-form-urlencoded")
            .header("x-forwarded-for", "203.0.113.9")
            .header("x-forwarded-for", "198.51.100.7")
            .body(Body::from(body))
            .unwrap(),
    )
    .await;
    assert_eq!(r.status, 303, "{}", r.text());
    // The test harness has no socket, so there is no peer either: the address is not the forged
    // one, and not any entry of the header.
    let ip = f.record("sign_request", &f.req)["signer_ip"].clone();
    assert_eq!(ip, "unknown");
}

/// A GET that reports `forwarded` in the header and `peer` as the socket's address.
async fn get_from_peer(f: &Fx, path: &str, forwarded: &str, peer: &str) -> Resp {
    let mut req = Request::get(path)
        .header("host", HOST)
        .header("x-forwarded-for", forwarded)
        .body(Body::empty())
        .unwrap();
    let addr: std::net::SocketAddr = format!("{peer}:4000").parse().unwrap();
    req.extensions_mut()
        .insert(axum::extract::ConnectInfo(addr));
    send(&f.router, req).await
}

#[tokio::test]
async fn an_address_that_is_not_an_ip_is_counted_under_the_peer() {
    // A proxy that reports something that is not an address (junk) must not put every client into
    // one bucket: failed lookups are counted per source, 20 in ten minutes. The source is then the
    // peer of the socket.
    let (f, _, path, tok) = sent().await;
    let rid = path
        .trim_start_matches("/sign/")
        .split('/')
        .next()
        .unwrap()
        .to_string();
    let wrong = format!("/sign/{rid}/{}", "1".repeat(tok.len()));
    for i in 0..20 {
        let r = get_from_peer(&f, &wrong, "not-an-address", "10.0.0.1").await;
        assert_eq!(r.status, 404, "try {i}");
    }
    assert_eq!(
        get_from_peer(&f, &wrong, "not-an-address", "10.0.0.1")
            .await
            .status,
        429,
        "the twenty-first from one peer waits"
    );
    // Another peer behind the same kind of junk has a count of its own, and a good link opens.
    assert_eq!(
        get_from_peer(&f, &wrong, "not-an-address", "10.0.0.2")
            .await
            .status,
        404
    );
    assert_eq!(
        get_from_peer(&f, &path, "not-an-address", "10.0.0.2")
            .await
            .status,
        200
    );
}

#[tokio::test]
async fn an_address_with_a_port_is_read_as_the_address() {
    let (f, _, path, _) = sent().await;
    let r = f
        .post(&path, "192.0.2.77:51000", &good_form(&f, "Jane Test"))
        .await;
    assert_eq!(r.status, 303, "{}", r.text());
    assert_eq!(f.record("sign_request", &f.req)["signer_ip"], "192.0.2.77");
    let (f, _, path, _) = sent().await;
    let r = f
        .post(&path, "[2001:db8::7]:51000", &good_form(&f, "Jane Test"))
        .await;
    assert_eq!(r.status, 303, "{}", r.text());
    assert_eq!(f.record("sign_request", &f.req)["signer_ip"], "2001:db8::7");
}

#[tokio::test]
async fn the_tick_is_exactly_one() {
    let (f, _, path, _) = sent().await;
    for agree in ["0", "false", "no", "", "yes", "on", "true", "2"] {
        let r = f
            .post(
                &path,
                &ip(32),
                &[("name", "Jane Test"), ("agree", agree), ("doc", &f.file)],
            )
            .await;
        assert_eq!(r.status, 400, "agree={agree:?}: {}", r.text());
        assert!(r.text().contains("<form"), "agree={agree:?}");
    }
    assert_eq!(f.record("sign_request", &f.req)["state"], "sent");
    assert_eq!(signed_count(&f), 0);
    let r = f.post(&path, &ip(32), &good_form(&f, "Jane Test")).await;
    assert_eq!(r.status, 303, "{}", r.text());
}

#[tokio::test]
async fn an_expiry_that_is_not_a_date_and_time_is_refused() {
    let (f, _, _, _) = sent().await;
    let before = count(&f, "sign_link");
    for bad in [
        json!(1),
        json!(1_700_000_000),
        json!(true),
        json!([]),
        json!({}),
        json!(""),
        json!("tomorrow"),
    ] {
        let (s, v) = f
            .api(
                "mia",
                "/api/sign_link/create",
                json!({"request_id": f.req, "expires_at": bad}),
            )
            .await;
        assert_eq!(s, 400, "{bad}: {v}");
        assert_eq!(count(&f, "sign_link"), before, "{bad}");
    }
}

#[tokio::test]
async fn what_reorders_or_hides_text_does_not_reach_the_signer() {
    let f = fixture().await;
    let bad = "\u{061c}\u{200b}\u{200e}\u{200f}\u{202a}\u{202b}\u{202c}\u{202d}\u{202e}\u{2060}\u{2066}\u{2067}\u{2068}\u{2069}\u{feff}";
    // Staff put them into every text the signer reads.
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/update",
            json!({"id": f.req, "title": format!("Please{bad} sign"),
                   "message": format!("Line one{bad}\nLine two"),
                   "signer_name": format!("Jane{bad} Test")}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let (s, v) = f
        .api(
            "ann",
            "/api/documents_document/update",
            json!({"id": f.doc, "title": format!("NDA{bad} (test)")}),
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
    let (_, path) = f.mint("mia", &f.req.clone()).await;
    let r = f.get(&path, &ip(33)).await;
    assert_eq!(r.status, 200);
    let html = r.text();
    for c in bad.chars() {
        assert!(!html.contains(c), "U+{:04X} reached the page", c as u32);
    }
    assert!(
        html.contains("Please sign") && html.contains("I am Jane Test"),
        "{html}"
    );
    // The line break of the message stays.
    assert!(html.contains("Line one\nLine two"), "{html}");
}

#[tokio::test]
async fn a_typed_name_with_a_character_that_reorders_or_hides_text_is_refused() {
    let chars: Vec<char> = "\u{061c}\u{200b}\u{200e}\u{200f}\u{202a}\u{202b}\u{202c}\u{202d}\u{202e}\u{2060}\u{2066}\u{2067}\u{2068}\u{2069}\u{feff}"
        .chars()
        .collect();
    for chunk in chars.chunks(7) {
        let (f, _, path, _) = sent().await;
        for c in chunk {
            let name = format!("Jane{c}Test");
            let r = f
                .post(
                    &path,
                    &ip(34),
                    &[("name", &name), ("agree", "1"), ("doc", &f.file)],
                )
                .await;
            assert_eq!(r.status, 400, "U+{:04X}: {}", *c as u32, r.text());
        }
        assert_eq!(f.record("sign_request", &f.req)["state"], "sent");
    }
    // The two joiners that Persian and Indic names need stay, between letters.
    let (f, _, path, _) = sent().await;
    let persian = "\u{0639}\u{0644}\u{06cc}\u{200c}\u{0631}\u{0636}\u{0627}";
    let r = f
        .post(
            &path,
            &ip(35),
            &[("name", persian), ("agree", "1"), ("doc", &f.file)],
        )
        .await;
    assert_eq!(r.status, 303, "{}", r.text());
    assert_eq!(f.record("sign_request", &f.req)["signed_name"], persian);
}

#[tokio::test]
async fn a_name_over_the_limit_is_refused_not_cut() {
    let (f, _, path, _) = sent().await;
    let long = format!("Jane Test{}x", " ".repeat(500));
    let r = f
        .post(
            &path,
            &ip(36),
            &[("name", &long), ("agree", "1"), ("doc", &f.file)],
        )
        .await;
    assert_eq!(r.status, 400, "{}", r.text());
    let ok = "J".repeat(121);
    let r = f
        .post(
            &path,
            &ip(36),
            &[("name", &ok), ("agree", "1"), ("doc", &f.file)],
        )
        .await;
    assert_eq!(r.status, 400, "121 characters are over the limit of 120");
    assert_eq!(f.record("sign_request", &f.req)["state"], "sent");
}

#[tokio::test]
async fn the_title_of_the_document_is_escaped_on_the_page() {
    let (f, _, path, _) = sent().await;
    let html = f.get(&path, &ip(37)).await.text();
    assert!(
        html.contains("Mutual NDA (test) &lt;i&gt;x&lt;/i&gt;"),
        "{html}"
    );
    assert!(!html.contains("<i>x</i>"));
}

// ---- what the documents promise, pinned --------------------------------------------------

#[tokio::test]
async fn declining_or_expiring_a_request_ends_its_links() {
    for action in ["decline", "expire"] {
        let (f, _, path, _) = sent().await;
        let (s, v) = f
            .api(
                "mia",
                &format!("/api/sign_request/{action}"),
                json!({"id": f.req}),
            )
            .await;
        assert_eq!(s, 200, "{action}: {v}");
        assert_ne!(f.record("sign_request", &f.req)["state"], "sent");
        let from = ip(40);
        assert_eq!(f.get(&path, &from).await.status, 410, "{action} page");
        assert_eq!(
            f.get(&format!("{path}/document.pdf"), &from).await.status,
            410,
            "{action} pdf"
        );
        assert_eq!(
            f.post(&path, &from, &good_form(&f, "Jane Test"))
                .await
                .status,
            410,
            "{action} post"
        );
        assert_eq!(signed_count(&f), 0, "{action}");
    }
}

#[tokio::test]
async fn shortening_the_last_day_of_a_request_ends_a_link_that_was_made_before() {
    let (f, _, path, _) = sent().await;
    assert_eq!(f.get(&path, &ip(41)).await.status, 200);
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/update",
            json!({"id": f.req, "expires_on": "2020-01-01"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(f.get(&path, &ip(41)).await.status, 410);
    assert_eq!(
        f.get(&format!("{path}/document.pdf"), &ip(41)).await.status,
        410
    );
    assert_eq!(
        f.post(&path, &ip(41), &good_form(&f, "Jane Test"))
            .await
            .status,
        410
    );
    assert_eq!(f.record("sign_request", &f.req)["state"], "sent");
}

#[tokio::test]
async fn a_plain_user_cannot_revoke_or_change_a_link() {
    let (f, link_id, path, _) = sent().await;
    for body in [
        json!({"id": link_id, "revoked_at": "2026-11-03T10:00:00Z"}),
        json!({"id": link_id, "expires_at": "2099-01-01T00:00:00Z"}),
    ] {
        let (s, v) = f.api("sam", "/api/sign_link/update", body.clone()).await;
        assert_ne!(s, 200, "{body}: {v}");
    }
    let (s, _) = f
        .api("sam", "/api/sign_link/create", json!({"request_id": f.req}))
        .await;
    assert_ne!(s, 200);
    let (s, _) = f
        .api("sam", "/api/sign_link/delete", json!({"id": link_id}))
        .await;
    assert_ne!(s, 200);
    assert!(
        f.record("sign_link", &link_id)
            .get("revoked_at")
            .is_none_or(J::is_null)
    );
    assert_eq!(f.get(&path, &ip(42)).await.status, 200);
}

#[tokio::test]
async fn a_person_without_the_manager_group_makes_no_link_even_with_documents_rights() {
    let (f, _, _, _) = sent().await;
    let before = count(&f, "sign_link");
    // dan reads and manages Documents and is a sign.user, not a sign.manager.
    let (s, v) = f
        .api("dan", "/api/sign_link/create", json!({"request_id": f.req}))
        .await;
    assert_ne!(s, 200, "{v}");
    assert_eq!(count(&f, "sign_link"), before);
}

#[tokio::test]
async fn a_manager_who_is_not_a_plain_user_still_gets_the_link_once() {
    let (f, _, _, _) = sent().await;
    // lee holds sign.manager and the Documents groups, and not sign.user: the `link` of the
    // answer is hers, or the link would be made and nobody would hold it.
    let (s, v) = f
        .api("lee", "/api/sign_link/create", json!({"request_id": f.req}))
        .await;
    assert_eq!(s, 200, "{v}");
    assert!(
        v["value"]["link"]
            .as_str()
            .is_some_and(|l| l.starts_with("/sign/")),
        "{v}"
    );
}

#[tokio::test]
async fn a_sender_needs_documents_rights_for_the_document_too() {
    let f = fixture().await;
    // An internal Document: everybody who may read Documents sees it, and nobody else.
    let (s, v) = f
        .upload(
            "ann",
            "documents_document.attachment",
            "application/pdf",
            pdf(7),
        )
        .await;
    assert_eq!(s, 201, "{v}");
    let file = v["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api(
            "ann",
            "/api/documents_document/create",
            json!({"title": "Internal (test)", "kind": "other", "confidentiality": "internal",
                   "state": "draft", "owner": "ann", "attachment": file}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let doc = v["value"]["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/create",
            json!({"title": "Internal one", "signer_name": "Jane Test", "document_id": doc}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let req = v["value"]["id"].as_str().unwrap().to_string();
    // noc is a sign.manager with no role in Documents: refused to send, and to make a link.
    let (s, v) = f
        .api("noc", "/api/sign_request/send_document", json!({"id": req}))
        .await;
    assert_eq!(s, 400, "{v}");
    assert_eq!(f.record("sign_request", &req)["state"], "draft");
    let (s, v) = f
        .api("mia", "/api/sign_request/send_document", json!({"id": req}))
        .await;
    assert_eq!(s, 200, "{v}");
    let (s, v) = f
        .api("noc", "/api/sign_link/create", json!({"request_id": req}))
        .await;
    assert_eq!(s, 400, "{v}");
    assert_eq!(count(&f, "sign_link"), 0);
}

#[tokio::test]
async fn evidence_is_read_by_managers_only_and_masked_for_agents() {
    let (f, _, path, _) = sent().await;
    // The signer types a name that is not the one the request asked for, so that the copy of it
    // inside `signature_reference` cannot be mistaken for the requested name.
    let typed = "Jane Q. Example";
    // The address the request will be sent to is personal data too (it is not evidence, but an
    // agent must not read it either): the fixture's request has none, so give it one.
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/update",
            json!({"id": f.req, "signer_email": "jane.q@example.test"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let r = f.post(&path, &ip(43), &good_form(&f, typed)).await;
    assert_eq!(r.status, 303);
    // The seven evidence fields a manager reads; the eighth, `document_file`, is read by an
    // administrator only (checked below).
    let keys = [
        "signed_at",
        "signed_name",
        "signer_ip",
        "signer_user_agent",
        "signature_reference",
        "consent_version",
        "document_hash",
    ];
    // A manager reads all seven, on get and on list.
    let (s, v) = f
        .api("mia", "/api/sign_request/get", json!({"id": f.req}))
        .await;
    assert_eq!(s, 200, "{v}");
    for k in keys {
        assert!(
            v["value"][k].as_str().is_some_and(|t| !t.is_empty()),
            "{k}: {v}"
        );
    }
    assert_eq!(v["value"]["signed_name"], typed);
    assert_eq!(v["value"]["signature_reference"], format!("typed:{typed}"));
    assert_eq!(v["value"]["signer_name"], "Jane Test");
    assert_eq!(v["value"]["signer_email"], "jane.q@example.test");
    let seen_by_manager = v["value"].clone();
    // The copy of the Document is read by nobody but an administrator, not even a manager.
    assert!(
        v["value"].get("document_file").is_none_or(J::is_null),
        "{v}"
    );
    let (s, v) = f
        .api("admin", "/api/sign_request/get", json!({"id": f.req}))
        .await;
    assert_eq!(s, 200, "{v}");
    assert!(
        v["value"]["document_file"]
            .as_str()
            .is_some_and(|t| !t.is_empty()),
        "{v}"
    );
    // A plain user reads none, on get, list and search, and cannot filter on them.
    let (s, v) = f
        .api("sam", "/api/sign_request/get", json!({"id": f.req}))
        .await;
    assert_eq!(s, 200, "{v}");
    for k in keys {
        assert!(
            v["value"].get(k).is_none_or(J::is_null),
            "get shows {k}: {v}"
        );
    }
    for cmd in ["list", "search"] {
        let (s, v) = f
            .api("sam", &format!("/api/sign_request/{cmd}"), json!({}))
            .await;
        assert_eq!(s, 200, "{cmd}: {v}");
        let rows = v["value"]
            .as_array()
            .or_else(|| v["value"]["rows"].as_array())
            .unwrap_or_else(|| panic!("{cmd}: {v}"));
        assert_eq!(rows.len(), 1, "{cmd}: {v}");
        for k in keys {
            assert!(
                rows[0].get(k).is_none_or(J::is_null),
                "{cmd} shows {k}: {v}"
            );
        }
        // The request itself is there, and so is what the signer typed as the expected name.
        assert_eq!(rows[0]["signer_name"], "Jane Test", "{cmd}");
    }
    // Nor can a plain user probe them by filtering, ordering or grouping: each is answered as if
    // the field did not exist. (Well-formed requests: a malformed one is refused for its shape
    // before any field is looked at, and would prove nothing.) The manager's same request works.
    for key in keys {
        let (value, group) = if key == "signed_at" {
            (
                json!("2026-11-03T10:00:00Z"),
                json!({"field": key, "bucket": "day"}),
            )
        } else {
            (json!("Jane Test"), json!({"field": key}))
        };
        let probes = [
            (
                "list",
                json!({"filter": [{"field": key, "op": "eq", "value": value}]}),
            ),
            ("list", json!({"order": [{"field": key, "dir": "asc"}]})),
            (
                "search",
                json!({"aggregate": {"group_by": [group], "measures": [{"op": "count"}]}}),
            ),
        ];
        for (cmd, probe) in probes {
            let path = format!("/api/sign_request/{cmd}");
            let (s, v) = f.api("sam", &path, probe.clone()).await;
            assert_eq!(s, 400, "{key} {probe}: {v}");
            assert!(err(&v).contains("unknown field"), "{key} {probe}: {v}");
            let (s, v) = f.api("mia", &path, probe.clone()).await;
            assert_eq!(s, 200, "{key} {probe}: {v}");
        }
    }
    // An agent with the manager's groups gets the personal fields masked, not the values: all of
    // them, whichever field holds a copy of what the signer typed. What is not personal (the
    // time, the consent version, the hash) it reads as it is.
    let mut f = f;
    agent_for(&mut f, "mia").await;
    let (s, got) = f
        .api("agent", "/api/sign_request/get", json!({"id": f.req}))
        .await;
    assert_eq!(s, 200, "{got}");
    let (s, listed) = f.api("agent", "/api/sign_request/list", json!({})).await;
    assert_eq!(s, 200, "{listed}");
    let rows = listed["value"]
        .as_array()
        .or_else(|| listed["value"]["rows"].as_array())
        .unwrap_or_else(|| panic!("{listed}"));
    assert_eq!(rows.len(), 1, "{listed}");
    for (what, row) in [("get", &got["value"]), ("list", &rows[0])] {
        for k in [
            "signed_name",
            "signer_name",
            "signer_ip",
            "signer_user_agent",
            "signature_reference",
            "signer_email",
        ] {
            let masked = row[k].as_str().unwrap_or("");
            assert!(
                !masked.is_empty() && masked != seen_by_manager[k].as_str().unwrap(),
                "{what} {k}: {row}"
            );
        }
        for k in ["signed_at", "consent_version", "document_hash"] {
            assert_eq!(row[k], seen_by_manager[k], "{what} {k}");
        }
        assert!(
            row.get("document_file").is_none_or(J::is_null),
            "{what}: {row}"
        );
    }
    // And nowhere in either answer is a piece of what was typed, the address or the browser.
    for (what, text) in [("get", got.to_string()), ("list", listed.to_string())] {
        for secret in [
            "Q. Example",
            "typed:Jane",
            "198.51.100.43",
            "TestBrowser",
            "Jane Test",
            "jane.q@example.test",
        ] {
            assert!(!text.contains(secret), "{what} shows {secret}: {text}");
        }
    }
    // Nor can the agent probe the masked fields by filtering, ordering or grouping: a mask token
    // compared with a guess would be an oracle for the value. Each is answered as if the field
    // did not exist (the platform's own test of this uses no sign model).
    for key in [
        "signed_name",
        "signer_name",
        "signer_ip",
        "signer_user_agent",
        "signature_reference",
        "signer_email",
    ] {
        let probes = [
            (
                "list",
                json!({"filter": [{"field": key, "op": "eq", "value": "Jane Test"}]}),
            ),
            ("list", json!({"order": [{"field": key, "dir": "asc"}]})),
            (
                "search",
                json!({"aggregate": {"group_by": [{"field": key}], "measures": [{"op": "count"}]}}),
            ),
        ];
        for (cmd, probe) in probes {
            let (s, v) = f
                .api("agent", &format!("/api/sign_request/{cmd}"), probe.clone())
                .await;
            assert_eq!(s, 400, "agent {key} {probe}: {v}");
            assert!(
                err(&v).contains("unknown field"),
                "agent {key} {probe}: {v}"
            );
        }
    }
}

#[tokio::test]
async fn the_secret_is_in_no_record_log_or_key_after_the_door_was_used() {
    let (f, _, path, tok) = sent().await;
    let from = ip(44);
    // Views, the PDF, a refused post, a post that fails, a signature, a replay and wrong links.
    f.get(&path, &from).await;
    f.get(&format!("{path}/document.pdf"), &from).await;
    f.post(
        &path,
        &from,
        &[("name", "x"), ("agree", "1"), ("doc", &f.file)],
    )
    .await;
    f.post(
        &path,
        &from,
        &[
            ("name", "Jane Test"),
            ("agree", "1"),
            ("doc", &"0".repeat(64)),
        ],
    )
    .await;
    f.post(&path, &from, &good_form(&f, "Jane Test")).await;
    f.post(&path, &from, &good_form(&f, "Jane Test")).await;
    f.get(&path, &from).await;
    f.get(&format!("{path}/document.pdf"), &from).await;
    f.get(&format!("/sign/{}/{}", f.req, "9".repeat(64)), &from)
        .await;
    let stored = f.everything_stored();
    assert!(!stored.contains(&tok), "the secret is stored somewhere");
    assert!(!stored.contains(&path));
    assert!(stored.contains("sign_signed"), "the scan reads the log");
}

#[tokio::test]
async fn the_log_names_the_digest_of_the_evidence_and_the_digest_is_of_all_of_it() {
    let (f, link_id, path, _) = sent().await;
    let r = f.post(&path, &ip(45), &good_form(&f, "Jane Test")).await;
    assert_eq!(r.status, 303);
    let req = f.record("sign_request", &f.req);
    // The digest by hand: the SHA-256 of the sorted `key=value` lines of exactly these fields.
    let text = |k: &str| req.get(k).and_then(J::as_str).unwrap_or("").to_string();
    let lines = format!(
        "consent_version={}\ndocument_hash={}\nlink_id={}\nrequest_id={}\nsignature_reference={}\nsigned_at={}\nsigned_name={}\nsigner_ip={}\nsigner_user_agent={}\n",
        text("consent_version"),
        text("document_hash"),
        link_id,
        f.req,
        text("signature_reference"),
        text("signed_at"),
        text("signed_name"),
        text("signer_ip"),
        text("signer_user_agent"),
    );
    let by_hand = sha256_hex(lines.as_bytes());
    assert_eq!(digest_of_request(&link_id, &f.req, &req), by_hand);
    let detail = f.events_of("sign_signed")[0]["detail"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(detail.contains(&format!("digest {by_hand}")), "{detail}");
    // And a change of any one field changes it.
    for k in [
        "consent_version",
        "document_hash",
        "signed_at",
        "signed_name",
        "signature_reference",
        "signer_ip",
        "signer_user_agent",
    ] {
        let mut other = req.clone();
        other.insert(k.to_string(), json!("changed"));
        assert_ne!(digest_of_request(&link_id, &f.req, &other), by_hand, "{k}");
    }
}

#[tokio::test]
async fn a_signed_request_is_neither_changed_nor_deleted_by_staff() {
    // A request signed by hand has no link, so only the guard stands in the way of a delete.
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
            json!({"title": "By hand", "template_id": tpl, "signer_name": "Jane Test",
                   "signer_email": "jane@example.test"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let id = v["value"]["id"].as_str().unwrap().to_string();
    for step in ["send", "sign"] {
        let (s, v) = f
            .api(
                "mia",
                &format!("/api/sign_request/{step}"),
                json!({"id": id}),
            )
            .await;
        assert_eq!(s, 200, "{step}: {v}");
    }
    assert_eq!(count(&f, "sign_link"), 0);
    for who in ["sam", "mia"] {
        let (s, v) = f
            .api(who, "/api/sign_request/delete", json!({"id": id}))
            .await;
        assert_eq!(s, 400, "{who}: {v}");
        assert!(
            err(&v).contains("cannot be changed or deleted"),
            "{who}: {v}"
        );
        let (s, v) = f
            .api(
                who,
                "/api/sign_request/update",
                json!({"id": id, "title": "Changed"}),
            )
            .await;
        assert_eq!(s, 400, "{who}: {v}");
        assert!(
            err(&v).contains("cannot be changed or deleted"),
            "{who}: {v}"
        );
    }
    assert_eq!(f.record("sign_request", &id)["title"], "By hand");
    // An administrator may, as the documents say.
    let (s, v) = f
        .api("admin", "/api/sign_request/delete", json!({"id": id}))
        .await;
    assert_eq!(s, 200, "{v}");
}

#[tokio::test]
async fn an_agent_only_proposes_send_sign_and_send_document() {
    let mut f = fixture().await;
    agent_for(&mut f, "mia").await;
    // A template request, ready to send, and one that is sent and ready to be signed by hand.
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_template/create",
            json!({"name": "Employment (test)", "body": "text"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let tpl = v["value"]["id"].as_str().unwrap().to_string();
    let mut made = Vec::new();
    for title in ["To send", "To sign"] {
        let (s, v) = f
            .api(
                "mia",
                "/api/sign_request/create",
                json!({"title": title, "template_id": tpl, "signer_name": "Jane Test",
                       "signer_email": "jane@example.test"}),
            )
            .await;
        assert_eq!(s, 200, "{v}");
        made.push(v["value"]["id"].as_str().unwrap().to_string());
    }
    let (s, v) = f
        .api("mia", "/api/sign_request/send", json!({"id": made[1]}))
        .await;
    assert_eq!(s, 200, "{v}");
    let calls = [
        ("send", made[0].clone(), "draft"),
        ("sign", made[1].clone(), "sent"),
        ("send_document", f.req.clone(), "draft"),
    ];
    for (action, id, stays) in calls {
        let (s, v) = f
            .api(
                "agent",
                &format!("/api/sign_request/{action}"),
                json!({"id": id}),
            )
            .await;
        assert_eq!(
            (s, v["approval_required"].clone()),
            (202, json!(true)),
            "{action}: {v}"
        );
        assert_eq!(f.record("sign_request", &id)["state"], stays, "{action}");
    }
}

#[tokio::test]
async fn a_link_of_one_request_opens_no_other_request() {
    let (f, link_id, path_a, tok_a) = sent().await;
    // A second Document and request with a link of its own.
    let (s, v) = f
        .upload(
            "ann",
            "documents_document.attachment",
            "application/pdf",
            pdf(8),
        )
        .await;
    assert_eq!(s, 201, "{v}");
    let file_b = v["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api(
            "ann",
            "/api/documents_document/create",
            json!({"title": "Second (test)", "kind": "contract", "confidentiality": "internal",
                   "state": "draft", "owner": "ann", "attachment": file_b}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let doc_b = v["value"]["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/create",
            json!({"title": "Second one", "signer_name": "Joan Test", "document_id": doc_b}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let req_b = v["value"]["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/send_document",
            json!({"id": req_b}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let (link_b, path_b) = f.mint("mia", &req_b).await;
    // The token of A on the id of the real request B: nothing.
    let crossed = format!("/sign/{req_b}/{tok_a}");
    for (i, p) in [crossed.clone(), format!("{crossed}/document.pdf")]
        .iter()
        .enumerate()
    {
        assert_eq!(f.get(p, &ip(50 + i as u8)).await.status, 404, "{p}");
    }
    assert_eq!(
        f.post(&crossed, &ip(52), &good_form(&f, "Jane Test"))
            .await
            .status,
        404
    );
    assert_eq!(f.record("sign_request", &req_b)["state"], "sent");
    // A link row that somebody wrote by hand with another request's id is not accepted either.
    f.app
        .store
        .backend()
        .update("sign_link", &link_b, &fields(json!({"request_id": f.req})))
        .unwrap();
    assert_eq!(f.get(&path_b, &ip(53)).await.status, 404);
    assert_eq!(f.get(&path_a, &ip(53)).await.status, 200);
    let _ = link_id;
}

#[tokio::test]
async fn views_of_the_page_and_the_pdf_share_one_limit_per_source() {
    let (f, _, path, _) = sent().await;
    let from = ip(54);
    for i in 0..60 {
        assert_eq!(f.get(&path, &from).await.status, 200, "page {i}");
        assert_eq!(
            f.get(&format!("{path}/document.pdf"), &from).await.status,
            200,
            "pdf {i}"
        );
    }
    let r = f.get(&path, &from).await;
    assert_eq!(r.status, 429);
    assert!(r.h("retry-after").parse::<u64>().unwrap() > 0);
    assert_eq!(
        f.get(&format!("{path}/document.pdf"), &from).await.status,
        429
    );
    // Another source reads.
    assert_eq!(f.get(&path, &ip(55)).await.status, 200);
}

#[tokio::test]
async fn wrong_links_write_nothing_to_the_log() {
    let (f, _, path, tok) = sent().await;
    let events = f.events().len();
    let (rid, _) = path.trim_start_matches("/sign/").split_once('/').unwrap();
    for i in 0..5u8 {
        let p = format!("/sign/{rid}/{}", format!("{i}").repeat(tok.len()));
        assert_eq!(f.get(&p, &ip(56)).await.status, 404);
        assert_eq!(
            f.get(&format!("{p}/document.pdf"), &ip(56)).await.status,
            404
        );
        assert_eq!(
            f.post(&p, &ip(56), &good_form(&f, "Jane Test"))
                .await
                .status,
            404
        );
    }
    assert_eq!(f.events().len(), events);
}

// ---- the signing batch, against a change that lands between its reads and its write ----------

/// Make `action` run when the door asks the clock for the second time: the first time is the
/// check of the link, the second is the moment of signing, after every read and before the write.
fn interleave(f: &Fx, action: impl Fn() + Send + Sync + 'static) {
    let calls = Arc::new(AtomicUsize::new(0));
    let base = now_micros();
    f.service().set_clock(Arc::new(move || {
        if calls.fetch_add(1, Ordering::SeqCst) + 1 == 2 {
            action();
        }
        base
    }));
}

#[tokio::test]
async fn a_link_revoked_after_the_door_read_it_is_not_signed() {
    let (f, link_id, path, _) = sent().await;
    let b = f.app.store.backend();
    interleave(&f, move || {
        b.update(
            "sign_link",
            &link_id,
            &fields(json!({"revoked_at": "2026-11-03T10:00:00.000000Z"})),
        )
        .unwrap();
    });
    let r = f.post(&path, &ip(60), &good_form(&f, "Jane Test")).await;
    assert_eq!(r.status, 409, "{}", r.text());
    assert_eq!(f.record("sign_request", &f.req)["state"], "sent");
    assert_eq!(signed_count(&f), 0);
}

#[tokio::test]
async fn a_request_declined_after_the_door_read_it_is_not_signed() {
    let (f, link_id, path, _) = sent().await;
    let (b, req) = (f.app.store.backend(), f.req.clone());
    interleave(&f, move || {
        b.update("sign_request", &req, &fields(json!({"state": "declined"})))
            .unwrap();
    });
    let r = f.post(&path, &ip(61), &good_form(&f, "Jane Test")).await;
    assert_eq!(r.status, 409, "{}", r.text());
    assert_eq!(f.record("sign_request", &f.req)["state"], "declined");
    assert!(
        f.record("sign_link", &link_id)
            .get("used_at")
            .is_none_or(J::is_null)
    );
    assert_eq!(signed_count(&f), 0);
}

#[tokio::test]
async fn a_document_whose_file_is_replaced_after_the_door_read_it_is_not_signed() {
    let (f, link_id, path, _) = sent().await;
    let (b, doc) = (f.app.store.backend(), f.doc.clone());
    interleave(&f, move || {
        b.update(
            "documents_document",
            &doc,
            &fields(json!({"attachment": "0".repeat(64)})),
        )
        .unwrap();
    });
    let r = f.post(&path, &ip(62), &good_form(&f, "Jane Test")).await;
    assert_eq!(r.status, 409, "{}", r.text());
    assert_eq!(f.record("sign_request", &f.req)["state"], "sent");
    assert!(
        f.record("sign_link", &link_id)
            .get("used_at")
            .is_none_or(J::is_null)
    );
    assert_eq!(signed_count(&f), 0);
}

#[tokio::test]
async fn a_signer_whose_twin_click_won_is_sent_to_the_signed_page_and_nothing_is_written() {
    // Two clicks on Sign: between this submit's reads and its write the other one commits (the
    // link is used, the request is signed, with the other submit's evidence). This one is not an
    // error: the signer sees the answer for a signed request, and nothing of this submit is
    // written (not a second log entry, not a second set of evidence).
    let (f, link_id, path, _) = sent().await;
    let (b, req) = (f.app.store.backend(), f.req.clone());
    interleave(&f, move || {
        b.update(
            "sign_link",
            &link_id,
            &fields(json!({"used_at": "2026-11-03T10:00:00.000000Z"})),
        )
        .unwrap();
        b.update(
            "sign_request",
            &req,
            &fields(json!({"state": "signed", "signed_name": "Winner Typed",
                           "signed_at": "2026-11-03T10:00:00.000000Z"})),
        )
        .unwrap();
    });
    let r = f.post(&path, &ip(63), &good_form(&f, "Jane Test")).await;
    assert_eq!(r.status, 303, "{}", r.text());
    let kept = f.record("sign_request", &f.req);
    assert_eq!(kept["state"], "signed");
    assert_eq!(
        kept["signed_name"], "Winner Typed",
        "the loser wrote nothing"
    );
    assert_eq!(signed_count(&f), 0, "and logged no second signature");
    assert!(f.events_of("sign_refused").is_empty());
}

// ---- headers, methods, encodings and the log line ------------------------------------------

/// What every answer of the door has, whatever its status: nothing that a browser or a proxy may
/// keep, no cookie, no cross-origin permission, and a request to stay out of a search index.
fn assert_door_headers(r: &Resp, what: &str) {
    assert_eq!(r.h("x-content-type-options"), "nosniff", "{what}");
    assert!(r.h("cache-control").contains("no-store"), "{what}");
    assert!(r.headers.get("set-cookie").is_none(), "{what}: a cookie");
    assert!(
        r.headers.get("access-control-allow-origin").is_none(),
        "{what}: a cross-origin permission"
    );
    assert!(r.h("x-robots-tag").contains("noindex"), "{what}");
}

#[tokio::test]
async fn every_answer_of_the_door_carries_the_same_protective_headers() {
    let (f, link_id, path, _) = sent().await;
    let from = ip(70);
    let pdf = format!("{path}/document.pdf");
    // The pages: the form, a refusal of the form, a post from another origin, a wrong link and
    // the page of a signed request.
    let page = f.get(&path, &from).await;
    assert_eq!(page.status, 200);
    let bad_name = f
        .post(
            &path,
            &from,
            &[("name", "x"), ("agree", "1"), ("doc", &f.file)],
        )
        .await;
    assert_eq!(bad_name.status, 400);
    let foreign = f
        .post_with(
            &path,
            &from,
            &good_form(&f, "Jane Test"),
            &[("origin", "http://evil.test")],
        )
        .await;
    assert_eq!(foreign.status, 403);
    let wrong = f.get(&format!("{path}0"), &from).await;
    assert_eq!(wrong.status, 404);
    for (what, r) in [
        ("form", &page),
        ("refused form", &bad_name),
        ("foreign origin", &foreign),
        ("wrong link", &wrong),
    ] {
        assert_door_headers(r, what);
        assert_eq!(r.h("x-frame-options"), "DENY", "{what}");
        assert!(!r.h("content-security-policy").is_empty(), "{what}");
    }
    // The PDF has its own framing rules and the same rest.
    let held = f.get(&pdf, &from).await;
    assert_eq!(held.status, 200);
    assert_door_headers(&held, "pdf");
    // The redirect after signing, and the page of a signed request.
    let done = f.post(&path, &from, &good_form(&f, "Jane Test")).await;
    assert_eq!(done.status, 303, "{}", done.text());
    assert_door_headers(&done, "redirect");
    let signed = f.get(&path, &from).await;
    assert_eq!(signed.status, 200);
    assert_door_headers(&signed, "signed page");
    // A link that was revoked, and a source that asked too often.
    let (f2, link2, path2, _) = sent().await;
    let (s, v) = f2
        .api(
            "mia",
            "/api/sign_link/update",
            json!({"id": link2, "revoked_at": "2026-11-03T10:00:00Z"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let gone = f2.get(&path2, &ip(71)).await;
    assert_eq!(gone.status, 410);
    assert_door_headers(&gone, "revoked");
    let wrong2 = format!("{path2}0");
    let mut last = f2.get(&wrong2, &ip(72)).await;
    for _ in 0..21 {
        last = f2.get(&wrong2, &ip(72)).await;
    }
    assert_eq!(last.status, 429);
    assert_door_headers(&last, "too many");
    assert!(!last.h("retry-after").is_empty());
    let _ = link_id;
}

#[tokio::test]
async fn the_door_answers_no_other_method() {
    let (f, _, path, _) = sent().await;
    for m in ["PUT", "DELETE", "PATCH", "OPTIONS", "TRACE"] {
        for p in [path.clone(), format!("{path}/document.pdf")] {
            let r = send(
                &f.router,
                Request::builder()
                    .method(m)
                    .uri(&p)
                    .header("host", HOST)
                    .header("origin", ORIGIN)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert!(r.status >= 400, "{m} {p}: {}", r.status);
            assert!(
                r.headers.get("access-control-allow-origin").is_none(),
                "{m} {p}: a cross-origin permission"
            );
        }
    }
    assert_eq!(f.record("sign_request", &f.req)["state"], "sent");
    assert_eq!(signed_count(&f), 0);
}

#[tokio::test]
async fn a_body_that_is_not_a_form_of_text_signs_nothing() {
    let (f, _, path, _) = sent().await;
    let events = f.events().len();
    let big = vec![b'a'; 20_000];
    let cases: Vec<(&str, &str, Vec<u8>)> = vec![
        (
            "percent-encoded bytes that are not UTF-8",
            "application/x-www-form-urlencoded",
            b"name=%ff%fe&agree=1".to_vec(),
        ),
        (
            "raw bytes that are not UTF-8",
            "application/x-www-form-urlencoded",
            b"name=Jane+Test&agree=1&x=\xff\xfe".to_vec(),
        ),
        (
            "a broken percent escape",
            "application/x-www-form-urlencoded",
            b"name=%zz&agree=1".to_vec(),
        ),
        (
            "JSON",
            "application/json",
            br#"{"name":"Jane Test","agree":"1"}"#.to_vec(),
        ),
        ("an empty body", "application/x-www-form-urlencoded", vec![]),
        (
            "a body over the limit",
            "application/x-www-form-urlencoded",
            big,
        ),
    ];
    for (what, ct, body) in cases {
        let r = send(
            &f.router,
            Request::post(&path)
                .header("host", HOST)
                .header("origin", ORIGIN)
                .header("content-type", ct)
                .header("x-forwarded-for", ip(73))
                .body(Body::from(body))
                .unwrap(),
        )
        .await;
        assert!(
            r.status == 400 || r.status == 413,
            "{what}: {} {}",
            r.status,
            r.text()
        );
    }
    assert_eq!(f.record("sign_request", &f.req)["state"], "sent");
    assert_eq!(f.events().len(), events);
}

#[tokio::test]
async fn a_forged_header_cannot_start_a_field_of_the_log_line() {
    let (f, _, path, _) = sent().await;
    let forged = format!("x; digest {}", "0".repeat(64));
    let r = f.post(&path, &forged, &good_form(&f, "Jane Test")).await;
    assert_eq!(r.status, 303, "{}", r.text());
    let detail = f.events_of("sign_signed")[0]["detail"]
        .as_str()
        .unwrap()
        .to_string();
    let keys: Vec<&str> = detail
        .split("; ")
        .map(|field| field.split(' ').next().unwrap())
        .collect();
    assert_eq!(
        keys,
        [
            "request",
            "link",
            "document",
            "consent",
            "digest",
            "address",
            "peer",
            "forwarded"
        ],
        "{detail}"
    );
}

#[tokio::test]
async fn the_forwarded_text_of_a_refused_post_cannot_start_a_field_of_the_log_line() {
    // The line about a refusal holds text from the client too (the peer and the forwarded
    // header), outside the digest; a semicolon of the client is not the border between two
    // fields, and the text is cut.
    let (f, _, path, _) = sent().await;
    f.bytes
        .as_ref()
        .unwrap()
        .put(&f.file, b"%PDF-1.4\n% somebody wrote here\n")
        .unwrap();
    let forged = format!("x; reason fake; address 203.0.113.9, {}", "9".repeat(300));
    let r = f.post(&path, &forged, &good_form(&f, "Jane Test")).await;
    assert_eq!(r.status, 409, "{}", r.text());
    let entries = f.events_of("sign_refused");
    assert_eq!(entries.len(), 1);
    let detail = entries[0]["detail"].as_str().unwrap().to_string();
    let keys: Vec<&str> = detail
        .split("; ")
        .map(|field| field.split(' ').next().unwrap())
        .collect();
    assert_eq!(
        keys,
        ["link", "reason", "address", "peer", "forwarded"],
        "{detail}"
    );
    let forwarded = detail.rsplit("; forwarded ").next().unwrap();
    assert!(forwarded.chars().count() <= 200, "{detail}");
    assert!(!forwarded.contains(';'), "{detail}");
}

#[tokio::test]
async fn no_link_is_made_for_a_request_whose_last_day_has_passed() {
    let (f, _, _, _) = sent().await;
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/update",
            json!({"id": f.req, "expires_on": "2020-01-01"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let before = count(&f, "sign_link");
    let (s, v) = f
        .api("mia", "/api/sign_link/create", json!({"request_id": f.req}))
        .await;
    assert_eq!(s, 400, "{v}");
    assert!(err(&v).contains("has expired"), "{v}");
    assert_eq!(count(&f, "sign_link"), before);
}
