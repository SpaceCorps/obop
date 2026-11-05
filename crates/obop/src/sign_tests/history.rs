//! The history of a file on the signer's page (an amendment of D92 dated 2026-11-04, PROPOSED): a
//! request whose manager ticked `show_history` lists, on its page and on the page after signing,
//! the typed name and the time of everyone who signed the same file through a request that ticked
//! it too. Every test has an instance of its own; all files are made-up PDFs and all people are
//! made up (Alice Example, Bob Example and the like).

use super::hardening::{fields, interleave};
use super::*;
use obop_commands::sign::digest_of_request;
use obop_pkg::installer::Request as PkgRequest;
use obop_pkg::resolve::StepAction;
use obop_storage::testing::{Flaky, Mode, Reads};
use std::sync::atomic::Ordering;

const SIGN_0_2_1: &str = include_str!("../fixtures/sign-0.2.1.toml");

/// The ids of the two wordings, written out (the unit test of `sign_text.rs` pins the same
/// literals): the one every signature carries today and the one a signature on a request that
/// shows the signatures so far carries.
const PLAIN_EN: &str = "v1-draft/en/a05c68c3";
const PLAIN_DE: &str = "v1-draft/de/9ebff173";
const WITH_HISTORY_EN: &str = "v1-draft/en/e2880db0/history";
const WITH_HISTORY_DE: &str = "v1-draft/de/d08d0039/history";

/// The time the clock of the golden pages stands at.
const GOLDEN_CLOCK: i64 = 1_793_000_000_000_000;

fn now_micros() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros() as i64
}

fn sha(text: &str) -> String {
    obop_commands::sign::sha256_hex(text.as_bytes())
}

/// The page with the request id and the secret of its link replaced, so that the pages of two
/// instances can be compared.
fn normalised(body: &str, rid: &str, tok: &str) -> String {
    body.replace(rid, "RID").replace(tok, "TOK")
}

/// The SHA-256 of the pages of a request that does not show the signatures so far, as the code
/// before the history rendered them (taken from the build of `origin/main` at 0986150, before any
/// change of this feature, with the clock at `GOLDEN_CLOCK` and the request id and the link's
/// secret replaced): the form in English and in German, then the page after signing in both.
const BEFORE_THE_HISTORY: [&str; 4] = [
    "00313428e3e6c31ae25ae909cca783c9d42a47c57b9885abc9777c2c0860aaae",
    "fd90ae1ca786f605f32fd13e7b6c4fa8c6fdf9b348cbf13c0b65e8127146ec49",
    "9dcc4cb5f9ae92f97a4a4ed5d12535896513cf2259fbbdd66418b5b5a0032ba2",
    "1dcbd57e51e8378a2b601f5cf16330a9ea47654f635f94c9d9096c7a869672a6",
];

/// The four pages of the request `rid` (its link is `path`), the clock at `GOLDEN_CLOCK`: the form
/// in English and German, then, after the request was signed with no `hist` field, the page after
/// signing in both. Returned as the SHA-256 of each, normalised.
async fn four_pages(f: &Fx, rid: &str, path: &str, n: u8) -> Vec<String> {
    let tok = path.rsplit('/').next().unwrap().to_string();
    f.service().set_clock(Arc::new(|| GOLDEN_CLOCK));
    let mut out = Vec::new();
    for lang in ["en", "de"] {
        let r = f.get(&format!("{path}?lang={lang}"), &ip(n)).await;
        assert_eq!(r.status, 200, "{}", r.text());
        out.push(sha(&normalised(&r.text(), rid, &tok)));
    }
    let r = f
        .post(
            path,
            &ip(n + 1),
            &[
                ("name", "Jane Test"),
                ("agree", "1"),
                ("doc", &f.file),
                ("lang", "en"),
            ],
        )
        .await;
    assert_eq!(r.status, 303, "{}", r.text());
    for lang in ["en", "de"] {
        let r = f.get(&format!("{path}?lang={lang}"), &ip(n + 2)).await;
        assert_eq!(r.status, 200, "{}", r.text());
        out.push(sha(&normalised(&r.text(), rid, &tok)));
    }
    out
}

/// A request that was made, sent and given a link.
struct Made {
    id: String,
    link_id: String,
    path: String,
}

impl Made {
    fn token(&self) -> &str {
        self.path.rsplit('/').next().unwrap()
    }
}

/// A request by `mia` about `doc`: `flag` is the value of `show_history` she sends, if any.
async fn made(f: &Fx, doc: &str, title: &str, flag: Option<bool>) -> Made {
    made_with(f, doc, title, flag, json!({})).await
}

/// As `made`, with the keys of `extra` added to the request (an `expires_on`, say).
async fn made_with(f: &Fx, doc: &str, title: &str, flag: Option<bool>, extra: J) -> Made {
    let mut body = json!({"title": title, "signer_name": format!("Requested {title}"),
                          "document_id": doc});
    if let Some(v) = flag {
        body["show_history"] = json!(v);
    }
    for (k, v) in extra.as_object().unwrap() {
        body[k] = v.clone();
    }
    let (s, v) = f.api("mia", "/api/sign_request/create", body).await;
    assert_eq!(s, 200, "{v}");
    let id = v["value"]["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api("mia", "/api/sign_request/send_document", json!({"id": id}))
        .await;
    assert_eq!(s, 200, "{v}");
    let (link_id, path) = f.mint("mia", &id).await;
    Made { id, link_id, path }
}

/// A Document that holds the file `n` (the same bytes make the same blob id, so two Documents can
/// hold one file).
async fn document(f: &Fx, n: u32) -> String {
    let (s, v) = f
        .upload(
            "mia",
            "documents_document.attachment",
            "application/pdf",
            pdf(n),
        )
        .await;
    assert_eq!(s, 201, "{v}");
    let file = v["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api(
            "mia",
            "/api/documents_document/create",
            json!({"title": format!("Another NDA {n} (test)"), "kind": "contract",
                   "confidentiality": "internal", "owner": "mia", "attachment": file}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    v["value"]["id"].as_str().unwrap().to_string()
}

/// The clock `secs` seconds after `base`.
fn clock_at(f: &Fx, base: i64, secs: i64) {
    f.service()
        .set_clock(Arc::new(move || base + secs * 1_000_000));
}

/// The signer's post in English, from address `n`, as a browser named after the address; `hist`
/// is whether the form carries the hidden field of a page that showed the sentence.
async fn sign(f: &Fx, m: &Made, hash: &str, name: &str, hist: bool, n: u8) -> Resp {
    sign_in(f, m, hash, name, "en", hist, n).await
}

async fn sign_in(f: &Fx, m: &Made, hash: &str, name: &str, lang: &str, hist: bool, n: u8) -> Resp {
    let mut pairs = vec![
        ("name", name),
        ("agree", "1"),
        ("doc", hash),
        ("lang", lang),
    ];
    if hist {
        pairs.push(("hist", "1"));
    }
    let agent = format!("Browser-of-{n}/1.0");
    f.post_with(
        &m.path,
        &ip(n),
        &pairs,
        &[("origin", ORIGIN), ("user-agent", &agent)],
    )
    .await
}

async fn page(f: &Fx, m: &Made, n: u8) -> Resp {
    f.get(&format!("{}?lang=en", m.path), &ip(n)).await
}

async fn page_of(f: &Fx, path: &str, n: u8) -> String {
    let r = f.get(&format!("{path}?lang=en"), &ip(n)).await;
    assert_eq!(r.status, 200, "{}", r.text());
    r.text()
}

/// The entries of the history on a page, in the order shown: (name as written in the page, time,
/// marked as the viewer's own).
fn entries(html: &str) -> Vec<(String, String, bool)> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find("<li>") {
        let tail = &rest[i + 4..];
        let end = tail.find("</li>").expect("an unclosed <li>");
        let item = &tail[..end];
        let (name, after) = item.split_once(" <span class=\"muted\">").expect(item);
        let (time, after_time) = after.split_once("</span>").expect(item);
        let own = after_time == " <strong>(you)</strong>";
        assert!(
            own || after_time.is_empty(),
            "something after the time: {item}"
        );
        out.push((name.to_string(), time.to_string(), own));
        rest = &tail[end + 5..];
    }
    out
}

fn names(html: &str) -> Vec<String> {
    entries(html).into_iter().map(|e| e.0).collect()
}

/// (name, marked) of each entry.
fn marked(html: &str) -> Vec<(String, bool)> {
    entries(html).into_iter().map(|e| (e.0, e.2)).collect()
}

/// A signed request written straight to the store, as an import or a record left by an older
/// version could be: the door must read it as it finds it. It carries the consent id of a
/// signature that was made on a page with the sentence (`WITH_HISTORY_EN`), so that a test is
/// about the one thing it names; `stored_signed_as` gives the id.
fn stored_signed(f: &Fx, id: &str, doc: &str, hash: &str, name: &str, at_micros: i64, flag: bool) {
    stored_signed_as(f, id, doc, hash, name, at_micros, flag, WITH_HISTORY_EN);
}

#[allow(clippy::too_many_arguments)]
fn stored_signed_as(
    f: &Fx,
    id: &str,
    doc: &str,
    hash: &str,
    name: &str,
    at_micros: i64,
    flag: bool,
    consent: &str,
) {
    let at = obop_values::datetime::Instant::from_micros(at_micros)
        .unwrap()
        .canonical();
    f.app
        .store
        .backend()
        .insert(
            "sign_request",
            id,
            &fields(json!({
                "title": format!("Stored {id}"), "signer_name": format!("Requested {id}"),
                "document_id": doc, "document_hash": hash, "state": "signed",
                "show_history": flag, "signed_at": at, "signed_name": name,
                "consent_version": consent,
            })),
        )
        .unwrap();
}

/// A fixture over a store whose reads are counted.
async fn counted() -> (Fx, Arc<Flaky>) {
    let (flaky, backend) = Flaky::memory();
    let bytes = Arc::new(MemoryBytes::default());
    let store =
        Store::over(backend.clone()).with_blobs(Arc::new(Catalog::new(backend, bytes.clone())));
    (fixture_on(store, Some(bytes), None).await, flaky)
}

// ---- the pages of a request that does not ask are what they were -------------------------------

#[tokio::test]
async fn the_pages_of_a_request_without_the_switch_are_what_they_were_before_the_history() {
    // The request has no `show_history` at all.
    let (f, _, path, _) = sent().await;
    let got = four_pages(&f, &f.req.clone(), &path, 10).await;
    assert_eq!(got, BEFORE_THE_HISTORY, "a request without the switch");
    assert!(
        f.record("sign_request", &f.req)
            .get("show_history")
            .is_none_or(J::is_null)
    );

    // The switch stored as off is the same: not a word of the history, not a byte.
    let f = fixture().await;
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/update",
            json!({"id": f.req, "show_history": false}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(f.record("sign_request", &f.req)["show_history"], false);
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/send_document",
            json!({"id": f.req}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let (_, path) = f.mint("mia", &f.req.clone()).await;
    let got = four_pages(&f, &f.req.clone(), &path, 20).await;
    assert_eq!(got, BEFORE_THE_HISTORY, "a request with the switch off");
}

#[tokio::test]
async fn a_request_that_does_not_ask_costs_no_query_and_one_that_asks_costs_one() {
    let (f, flaky) = counted().await;
    let plain = made(&f, &f.doc, "Plain", None).await;
    let off = made(&f, &f.doc, "Off", Some(false)).await;
    let on = made(&f, &f.doc, "On", Some(true)).await;
    let other = made(&f, &f.doc, "Other", Some(true)).await;
    for (m, n) in [(&plain, 30), (&off, 31)] {
        flaky.reset_reads();
        assert_eq!(page(&f, m, n).await.status, 200);
        let reads = flaky.reads();
        assert_eq!(
            reads.finds + reads.keyed,
            0,
            "a request that does not ask: {reads:?}"
        );
    }
    // One that asks is one `find` on the hash of the file. It hands back the rows that hold that
    // hash (the four requests sent about this file, whatever their state or switch) and the
    // door reads no model whole.
    flaky.reset_reads();
    assert_eq!(page(&f, &on, 32).await.status, 200);
    let reads = flaky.reads();
    assert_eq!(reads.finds, 1, "{reads:?}");
    assert_eq!(reads.find_rows, 4, "{reads:?}");
    assert_eq!(reads.lists + reads.scans, 0, "{reads:?}");
    // The page after signing is the same single query.
    assert_eq!(
        sign(&f, &on, &f.file, "Alice Example", true, 33)
            .await
            .status,
        303
    );
    flaky.reset_reads();
    assert_eq!(page(&f, &on, 34).await.status, 200);
    assert_eq!(flaky.reads().finds, 1);
    // The other request, still unsigned, sees the first.
    assert_eq!(names(&page(&f, &other, 35).await.text()), ["Alice Example"]);
}

// ---- who is listed -----------------------------------------------------------------------------

#[tokio::test]
async fn a_flagged_page_lists_the_other_flagged_signed_requests_of_the_same_file_and_nobody_else() {
    let f = fixture().await;
    let base = now_micros();
    let same_bytes = document(&f, 1).await; // another Document, the same file
    assert_ne!(same_bytes, f.doc);
    let viewer = made(&f, &f.doc, "Viewer", Some(true)).await;
    let alpha = made(&f, &f.doc, "Alpha", Some(true)).await;
    let beta = made(&f, &f.doc, "Beta", Some(true)).await;
    let gamma = made(&f, &f.doc, "Gamma", None).await; // signed, but does not ask
    let delta = made(&f, &f.doc, "Delta", Some(false)).await; // signed, switch off
    let pending = made(&f, &f.doc, "Pending", Some(true)).await; // sent, never signed
    let declined = made(&f, &f.doc, "Declined", Some(true)).await;
    let elsewhere = made(&f, &same_bytes, "Elsewhere", Some(true)).await;
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/decline",
            json!({"id": declined.id}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    // Signed in another order than they were made, to show that the order is the time.
    for (secs, who, name, hist, n) in [
        (10, &beta, "Beta Example", true, 40),
        (20, &alpha, "Alpha Example", true, 41),
        (30, &gamma, "Gamma Example", false, 42),
        (40, &delta, "Delta Example", false, 43),
        (50, &elsewhere, "Elsewhere Example", true, 44),
    ] {
        clock_at(&f, base, secs);
        assert_eq!(
            sign(&f, who, &f.file, name, hist, n).await.status,
            303,
            "{name}"
        );
    }
    // A request that was declined while it held evidence (the race of docs/SIGN.md, O21), and a
    // request an administrator marked signed with no time and no name: neither is listed.
    let b = f.app.store.backend();
    b.insert(
        "sign_request",
        "raced-by-a-decline",
        &fields(json!({
            "title": "Raced", "signer_name": "Requested Raced", "document_id": f.doc,
            "document_hash": f.file, "state": "declined", "show_history": true,
            "signed_at": "2026-11-03T09:00:00.000000Z", "signed_name": "Raced Example",
            "consent_version": WITH_HISTORY_EN,
        })),
    )
    .unwrap();
    b.insert(
        "sign_request",
        "signed-by-hand",
        &fields(json!({
            "title": "By hand", "signer_name": "Requested By hand", "document_id": f.doc,
            "document_hash": f.file, "state": "signed", "show_history": true,
            "consent_version": WITH_HISTORY_EN,
        })),
    )
    .unwrap();

    let html = page(&f, &viewer, 50).await.text();
    assert_eq!(names(&html), ["Beta Example", "Alpha Example"], "{html}");
    let rows = entries(&html);
    assert!(
        rows.iter().all(|(_, _, own)| !own),
        "the viewer has not signed"
    );
    let at = |secs: i64| {
        let t = obop_values::datetime::Instant::from_micros(base + secs * 1_000_000)
            .unwrap()
            .canonical();
        format!("{}Z", &t[..19])
    };
    assert_eq!(
        (rows[0].1.as_str(), rows[1].1.as_str()),
        (at(10).as_str(), at(20).as_str())
    );
    for absent in [
        "Gamma",
        "Delta",
        "Elsewhere",
        "Raced",
        "By hand",
        "Pending",
        "Declined",
        "Requested Alpha",
        "Requested Beta",
    ] {
        assert!(!html.contains(absent), "{absent} is on the page:\n{html}");
    }
    // The block sits above the form, and the sentence is in the notice.
    assert!(html.contains("<h2>Signed so far</h2>"));
    assert!(
        html.contains(" Your name, as you type it, and the time you sign are shown to the other people who sign this file.</p>"),
        "{html}"
    );
    assert!(html.find("Signed so far").unwrap() < html.find("<form").unwrap());
    assert!(html.contains("name=\"hist\" value=\"1\""));

    // The viewer signs, and the page after signing lists the three, itself last and marked.
    clock_at(&f, base, 60);
    assert_eq!(
        sign(&f, &viewer, &f.file, "Viewer Example", true, 51)
            .await
            .status,
        303
    );
    let end = page(&f, &viewer, 52).await.text();
    assert_eq!(
        marked(&end),
        [
            ("Beta Example".to_string(), false),
            ("Alpha Example".to_string(), false),
            ("Viewer Example".to_string(), true)
        ],
        "{end}"
    );
    assert!(!end.contains("<form"));
    // An earlier signer who opens the link later sees the later one too, and themselves marked.
    let again = page(&f, &alpha, 53).await.text();
    assert_eq!(
        marked(&again),
        [
            ("Beta Example".to_string(), false),
            ("Alpha Example".to_string(), true),
            ("Viewer Example".to_string(), false)
        ],
        "{again}"
    );
    // Requests that do not ask see no history, whatever has been signed.
    for (m, n) in [(&gamma, 54), (&delta, 55)] {
        let html = page(&f, m, n).await.text();
        assert!(
            !html.contains("Signed so far") && !html.contains("<li>"),
            "{html}"
        );
        assert!(!html.contains("Beta Example"));
    }
    // The pending request, which asks, is in nobody's list and sees the three.
    assert_eq!(
        names(&page(&f, &pending, 56).await.text()),
        ["Beta Example", "Alpha Example", "Viewer Example"]
    );
}

#[tokio::test]
async fn a_replaced_file_starts_a_new_history() {
    let f = fixture().await;
    let base = now_micros();
    let old = made(&f, &f.doc, "Old", Some(true)).await;
    clock_at(&f, base, 10);
    assert_eq!(
        sign(&f, &old, &f.file, "Olga Example", true, 60)
            .await
            .status,
        303
    );
    // The Document gets another file.
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
    let (s, _) = f
        .api(
            "ann",
            "/api/documents_document/update",
            json!({"id": f.doc, "attachment": newer}),
        )
        .await;
    assert_eq!(s, 200);
    let fresh = made(&f, &f.doc, "Fresh", Some(true)).await;
    // The same Document and another hash: nobody has signed this file.
    let html = page(&f, &fresh, 61).await.text();
    assert!(html.contains("Nobody has signed this file yet."), "{html}");
    assert!(!html.contains("Olga") && !html.contains("<li>"), "{html}");
    clock_at(&f, base, 20);
    assert_eq!(
        sign(&f, &fresh, &newer, "Fred Example", true, 62)
            .await
            .status,
        303
    );
    assert_eq!(names(&page(&f, &fresh, 63).await.text()), ["Fred Example"]);
    // And the old signer's page still lists only the signatures on the old file.
    assert_eq!(names(&page(&f, &old, 64).await.text()), ["Olga Example"]);
}

#[tokio::test]
async fn the_list_has_the_earliest_fifty_oldest_first_and_says_when_it_is_cut() {
    let f = fixture().await;
    let base = now_micros();
    let viewer = made(&f, &f.doc, "Viewer", Some(true)).await;
    // Fifty records, inserted newest first: the order shown is the time of the signature.
    for i in (0..50).rev() {
        stored_signed(
            &f,
            &format!("stored-{i:03}"),
            &f.doc,
            &f.file,
            &format!("Person Example {i:03}"),
            base + i * 1_000_000,
            true,
        );
    }
    let html = page(&f, &viewer, 70).await.text();
    let got = names(&html);
    assert_eq!(got.len(), 50);
    assert_eq!(got.first().unwrap(), "Person Example 000");
    assert_eq!(got.last().unwrap(), "Person Example 049");
    let mut sorted = got.clone();
    sorted.sort();
    assert_eq!(got, sorted, "oldest first");
    assert!(
        !html.contains("Only the earliest signatures are listed here."),
        "fifty is not cut"
    );
    // A 51st, later than all the others: not listed, and the page says that the list is cut.
    stored_signed(
        &f,
        "stored-050",
        &f.doc,
        &f.file,
        "Person Example 050",
        base + 50_000_000,
        true,
    );
    let html = page(&f, &viewer, 71).await.text();
    assert_eq!(names(&html).len(), 50);
    assert!(!html.contains("Person Example 050"));
    assert!(html.contains("Only the earliest signatures are listed here."));
    // One signed before all of them, inserted last, takes the first place and the newest of the
    // fifty drops out: the earliest fifty are kept, not the first fifty found.
    stored_signed(
        &f,
        "stored-early",
        &f.doc,
        &f.file,
        "Person Example Early",
        base - 1_000_000,
        true,
    );
    let html = page(&f, &viewer, 72).await.text();
    let got = names(&html);
    assert_eq!(got.len(), 50);
    assert_eq!(got.first().unwrap(), "Person Example Early");
    assert_eq!(got.last().unwrap(), "Person Example 048");
    assert!(!html.contains("Person Example 049") && !html.contains("Person Example 050"));
    assert!(html.contains("Only the earliest signatures are listed here."));
}

#[tokio::test]
async fn a_name_in_the_store_is_shown_as_text_whatever_it_holds() {
    let f = fixture().await;
    let base = now_micros();
    let viewer = made(&f, &f.doc, "Viewer", Some(true)).await;
    let evil = "<script>alert(1)</script> & \"q\" 'x' <img src=x onerror=alert(2)>";
    stored_signed(&f, "stored-evil", &f.doc, &f.file, evil, base, true);
    // Text that reorders or hides other text, and white space of every kind.
    stored_signed(
        &f,
        "stored-bidi",
        &f.doc,
        &f.file,
        "Eve\u{202e} Example\u{200b}\n\t  Two",
        base + 1_000_000,
        true,
    );
    // A name that is nothing but hidden characters is not an entry.
    stored_signed(
        &f,
        "stored-empty",
        &f.doc,
        &f.file,
        "\u{200b}\u{202e}",
        base + 2_000_000,
        true,
    );
    // A time that is not a time is not an entry either, and a switch that is the text "true" is
    // not the switch.
    let b = f.app.store.backend();
    b.insert(
        "sign_request",
        "stored-badtime",
        &fields(json!({"document_id": f.doc, "document_hash": f.file, "state": "signed",
                       "show_history": true, "signed_at": "yesterday", "signed_name": "Nina Example",
                       "consent_version": WITH_HISTORY_EN})),
    )
    .unwrap();
    b.insert(
        "sign_request",
        "stored-text-flag",
        &fields(
            json!({"document_id": f.doc, "document_hash": f.file, "state": "signed",
                       "show_history": "true", "signed_at": "2026-11-03T09:00:00.000000Z",
                       "signed_name": "Tex Example", "consent_version": WITH_HISTORY_EN}),
        ),
    )
    .unwrap();
    let html = page(&f, &viewer, 80).await.text();
    assert!(
        !html.contains("<script") && !html.contains("<img"),
        "{html}"
    );
    assert!(
        html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
        "{html}"
    );
    assert_eq!(
        names(&html),
        [obop_oauth::page::esc(evil), "Eve Example Two".to_string()],
        "{html}"
    );
    assert!(!html.contains("Nina") && !html.contains("Tex "));
    // The same on the page after signing.
    assert_eq!(
        sign(&f, &viewer, &f.file, "Viewer Example", true, 81)
            .await
            .status,
        303
    );
    let end = page(&f, &viewer, 82).await.text();
    assert!(!end.contains("<script") && !end.contains("<img"), "{end}");
    assert!(end.contains(&obop_oauth::page::esc(evil)), "{end}");
}

/// `NAME_MAX` of `history.rs`: the longest name the list shows, in characters.
const NAME_MAX: usize = 120;

#[tokio::test]
async fn a_listed_name_is_cut_to_120_characters_not_bytes_and_the_order_is_the_time_not_the_length()
{
    let f = fixture().await;
    let base = now_micros();
    let viewer = made(&f, &f.doc, "Viewer", Some(true)).await;
    // What a stale or imported record could hold (the door refuses a typed name over 120). The
    // records go in newest first, so that the order shown can only be the time. The last name is
    // made of characters of three bytes each: the unit of the cut is the character, so the cut
    // name is 360 bytes, not 120 and not 40.
    let exactly = "A".repeat(NAME_MAX);
    let one_over = format!("{exactly}B");
    let far_over = "C".repeat(5000);
    let wide = "\u{540d}".repeat(200);
    assert_eq!(wide.len(), 600, "three bytes a character");
    for (id, name, secs) in [
        ("stored-4-wide", wide.as_str(), 3),
        ("stored-3-far", far_over.as_str(), 2),
        ("stored-2-over", one_over.as_str(), 1),
        ("stored-1-exact", exactly.as_str(), 0),
    ] {
        stored_signed(&f, id, &f.doc, &f.file, name, base + secs * 1_000_000, true);
    }
    let at = |secs: i64| {
        let t = obop_values::datetime::Instant::from_micros(base + secs * 1_000_000)
            .unwrap()
            .canonical();
        format!("{}Z", &t[..19])
    };
    let listed = |html: &str, own_last: bool| {
        let rows = entries(html);
        assert_eq!(rows.len(), 4 + usize::from(own_last), "{html}");
        // The order is the time of the signatures: 0, 1, 2, 3 seconds after `base`.
        for (i, (name, time, own)) in rows.iter().take(4).enumerate() {
            assert_eq!(time, &at(i as i64), "entry {i}");
            assert_eq!(name.chars().count(), NAME_MAX, "entry {i} is cut to 120");
            assert!(!own, "entry {i}");
        }
        let names: Vec<&str> = rows.iter().take(4).map(|r| r.0.as_str()).collect();
        assert_eq!(
            names,
            [
                exactly.as_str(),
                exactly.as_str(),
                "C".repeat(NAME_MAX).as_str(),
                "\u{540d}".repeat(NAME_MAX).as_str()
            ],
            "{html}"
        );
        // Counted in characters, not bytes: the wide name keeps 360 bytes.
        assert_eq!(rows[3].0.len(), NAME_MAX * 3, "the unit is the character");
        // No run of one character more than the cut allows, in any of the four letters.
        for c in ["A", "B", "C", "\u{540d}"] {
            assert!(
                !html.contains(&c.repeat(NAME_MAX + 1)),
                "a run of {} {c} on the page",
                NAME_MAX + 1
            );
        }
        assert!(
            !html.contains(&format!("{exactly}B")),
            "the 121st character"
        );
        rows
    };
    let html = page(&f, &viewer, 90).await.text();
    listed(&html, false);
    // The same cut on the page after signing, where the viewer's own line comes last and marked.
    clock_at(&f, base, 10);
    assert_eq!(
        sign(&f, &viewer, &f.file, "Viewer Example", true, 91)
            .await
            .status,
        303
    );
    let end = page(&f, &viewer, 92).await.text();
    let rows = listed(&end, true);
    assert_eq!(rows[4].0, "Viewer Example");
    assert!(rows[4].2, "the viewer's own line is marked");
}

#[tokio::test]
async fn two_requests_that_share_the_requested_name_and_the_typed_name_are_told_apart_by_the_mark()
{
    let f = fixture().await;
    let base = now_micros();
    // The same name asked for, the same name typed: nothing about the two but their ids differs.
    let same = json!({"signer_name": "Casey Example"});
    let first = made_with(&f, &f.doc, "Casey one", Some(true), same.clone()).await;
    let second = made_with(&f, &f.doc, "Casey two", Some(true), same).await;
    for m in [&first, &second] {
        let rec = f.record("sign_request", &m.id);
        assert_eq!(rec["signer_name"], "Casey Example");
    }
    let at = |secs: i64| {
        let t = obop_values::datetime::Instant::from_micros(base + secs * 1_000_000)
            .unwrap()
            .canonical();
        format!("{}Z", &t[..19])
    };
    clock_at(&f, base, 10);
    assert_eq!(
        sign(&f, &first, &f.file, "Casey Example", true, 100)
            .await
            .status,
        303
    );
    // Only the first has signed: one line, and it is the first's own.
    let end = page(&f, &first, 101).await.text();
    assert_eq!(marked(&end), [("Casey Example".to_string(), true)], "{end}");

    clock_at(&f, base, 20);
    assert_eq!(
        sign(&f, &second, &f.file, "Casey Example", true, 102)
            .await
            .status,
        303
    );
    // Both have signed, under one name and one requested name. Each link's page lists two lines
    // and marks exactly one: the line of the request of that link, not the other and not both.
    for (m, n, mine) in [(&first, 103, 0), (&second, 104, 1)] {
        let end = page(&f, m, n).await.text();
        let rows = entries(&end);
        assert_eq!(rows.len(), 2, "{end}");
        assert_eq!(
            rows.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(),
            ["Casey Example", "Casey Example"],
            "{end}"
        );
        assert_eq!(
            (rows[0].1.as_str(), rows[1].1.as_str()),
            (at(10).as_str(), at(20).as_str()),
            "{end}"
        );
        assert_eq!(
            end.matches("<strong>(you)</strong>").count(),
            1,
            "exactly one line is marked:\n{end}"
        );
        let own: Vec<bool> = rows.iter().map(|r| r.2).collect();
        let want: Vec<bool> = (0..2).map(|i| i == mine).collect();
        assert_eq!(
            own, want,
            "the link of request {mine} marks the line of request {mine}"
        );
    }
}

#[tokio::test]
async fn the_list_shows_the_typed_name_and_the_time_and_nothing_else_of_the_others() {
    let f = fixture().await;
    let base = now_micros();
    let viewer = made(&f, &f.doc, "Viewer", Some(true)).await;
    let alice = made(&f, &f.doc, "Alice", Some(true)).await;
    // Alice's request carries everything a request can: a template, an address, a message.
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_template/create",
            json!({"name": "Secret Template Name (test)", "body": "made-up text"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let template = v["value"]["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/update",
            json!({"id": alice.id, "template_id": template,
                   "signer_email": "alice.private@example.test",
                   "message": "Alice's private message (test)"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    clock_at(&f, base, 10);
    let r = sign(&f, &alice, &f.file, "  Alice   Example  ", true, 201).await;
    assert_eq!(r.status, 303, "{}", r.text());
    let signed = f.record("sign_request", &alice.id);
    assert_eq!(signed["signed_name"], "Alice Example");
    assert_eq!(signed["signer_ip"], ip(201));
    let digest = digest_of_request(&alice.link_id, &alice.id, &signed);

    let secrets: Vec<(&str, String)> = vec![
        ("the address", ip(201)),
        ("the browser", "Browser-of-201".to_string()),
        ("the request id", alice.id.clone()),
        ("the link id", alice.link_id.clone()),
        ("the start of the link id", alice.link_id[..12].to_string()),
        ("the secret of the link", alice.token().to_string()),
        ("the digest", digest),
        ("the template", "Secret Template Name".to_string()),
        ("the template id", template),
        ("the address of the signer", "alice.private".to_string()),
        ("the message", "private message".to_string()),
        ("the requested name", "Requested Alice".to_string()),
        ("the consent id", "v1-draft/".to_string()),
        ("the reference", "typed:".to_string()),
        ("the log", "sign_signed".to_string()),
    ];
    // What is the viewer's own is on its page: its request id, the secret of its link and the
    // fingerprint of the file. Take that out, and none of the other request's values is left.
    let strip = |html: &str| {
        html.replace(&viewer.id, "")
            .replace(viewer.token(), "")
            .replace(&f.file, "")
    };
    let html = page(&f, &viewer, 90).await.text();
    assert_eq!(names(&html), ["Alice Example"]);
    for (what, secret) in &secrets {
        assert!(
            !strip(&html).contains(secret.as_str()),
            "{what} ({secret}) is on the page:\n{html}"
        );
    }
    // Each entry is the name, a space, the time in a span, and nothing after it.
    assert!(html.contains(&format!(
        "<li>Alice Example <span class=\"muted\">{}</span></li>",
        entries(&html)[0].1
    )));
    // The same for the page the viewer sees once signed.
    clock_at(&f, base, 20);
    assert_eq!(
        sign(&f, &viewer, &f.file, "Viewer Example", true, 92)
            .await
            .status,
        303
    );
    let end = page(&f, &viewer, 93).await.text();
    assert_eq!(names(&end), ["Alice Example", "Viewer Example"]);
    for (what, secret) in &secrets {
        // The viewer's own signature is a different request: its time and name are on the page.
        assert!(
            !strip(&end).contains(secret.as_str()),
            "{what} is on the page after signing:\n{end}"
        );
    }
}

async fn revoke(f: &Fx, m: &Made) {
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_link/update",
            json!({"id": m.link_id, "revoked_at": "2026-11-04T12:00:00Z"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
}

#[tokio::test]
async fn a_link_that_shows_no_page_shows_no_history_and_the_refusal_it_always_gave() {
    let f = fixture().await;
    let base = now_micros();
    let a = made(&f, &f.doc, "Alpha", Some(true)).await;
    clock_at(&f, base, 10);
    assert_eq!(
        sign(&f, &a, &f.file, "Alpha Example", true, 100)
            .await
            .status,
        303
    );
    // Pairs of a request that asks and one that does not, put in the same situation: the refusal
    // is the same page for both and has no name in it.
    let flagged = made(&f, &f.doc, "Flagged", Some(true)).await;
    let plain = made(&f, &f.doc, "Plain", None).await;
    let late_f = made(&f, &f.doc, "Late flagged", Some(true)).await;
    let late_p = made(&f, &f.doc, "Late plain", None).await;
    let gone_f = made(&f, &f.doc, "Declined flagged", Some(true)).await;
    let gone_p = made(&f, &f.doc, "Declined plain", None).await;

    // A wrong secret is the answer of a link that is no link.
    let nonsense = f.get("/sign/x/y?lang=en", &ip(101)).await;
    assert_eq!(nonsense.status, 404);
    for (m, n) in [(&flagged, 102), (&plain, 103)] {
        let wrong = format!("/sign/{}/{}", m.id, "1".repeat(m.token().len()));
        let r = f.get(&format!("{wrong}?lang=en"), &ip(n)).await;
        assert_eq!((r.status, r.text()), (404, nonsense.text()));
        assert_eq!(
            f.get(&format!("{wrong}/document.pdf"), &ip(n)).await.status,
            404
        );
        let r = f
            .post(
                &wrong,
                &ip(n),
                &[("name", "X Example"), ("agree", "1"), ("lang", "en")],
            )
            .await;
        assert_eq!((r.status, r.text()), (404, nonsense.text()));
    }

    // A revoked link: 410, the same page for both, and a post on it signs nothing.
    revoke(&f, &flagged).await;
    revoke(&f, &plain).await;
    let (rf, rp) = (page(&f, &flagged, 104).await, page(&f, &plain, 105).await);
    assert_eq!((rf.status, rf.text()), (410, rp.text()));
    assert!(!rf.text().contains("Alpha Example") && !rf.text().contains("<li>"));
    let r = sign(&f, &flagged, &f.file, "Late Example", true, 106).await;
    assert_eq!(r.status, 410);
    assert!(!r.text().contains("Alpha Example"));
    assert_eq!(f.record("sign_request", &flagged.id)["state"], "sent");

    // A link that ran out.
    f.service()
        .set_clock(Arc::new(move || base + 15 * 24 * 3_600_000_000));
    let (rf, rp) = (page(&f, &late_f, 107).await, page(&f, &late_p, 108).await);
    assert_eq!((rf.status, rf.text()), (410, rp.text()), "expired");
    assert!(!rf.text().contains("Alpha Example") && !rf.text().contains("<li>"));
    clock_at(&f, base, 30);

    // A request that was declined.
    for m in [&gone_f, &gone_p] {
        let (s, v) = f
            .api("mia", "/api/sign_request/decline", json!({"id": m.id}))
            .await;
        assert_eq!(s, 200, "{v}");
    }
    let (rf, rp) = (page(&f, &gone_f, 109).await, page(&f, &gone_p, 110).await);
    assert_eq!((rf.status, rf.text()), (410, rp.text()), "declined");
    assert!(!rf.text().contains("Alpha Example") && !rf.text().contains("<li>"));
    assert_eq!(
        f.get(&format!("{}/document.pdf", gone_f.path), &ip(111))
            .await
            .status,
        410
    );

    // The copy is served to a link that works, and only to it, exactly as before.
    let live = made(&f, &f.doc, "Live", Some(true)).await;
    let r = f
        .get(&format!("{}/document.pdf", live.path), &ip(112))
        .await;
    assert_eq!((r.status, r.h("content-type")), (200, "application/pdf"));
    assert_eq!(r.body, pdf(1));
    assert_eq!(
        sign(&f, &live, &f.file, "Live Example", true, 113)
            .await
            .status,
        303
    );
    assert_eq!(
        f.get(&format!("{}/document.pdf", live.path), &ip(114))
            .await
            .status,
        410
    );
    // A form for another file is still refused, whether or not the request asks.
    let other = made(&f, &f.doc, "Other", Some(true)).await;
    let r = sign(&f, &other, &"0".repeat(64), "Oscar Example", true, 115).await;
    assert_eq!(r.status, 409, "{}", r.text());
    assert_eq!(f.record("sign_request", &other.id)["state"], "sent");
}

// ---- the consent -------------------------------------------------------------------------------

#[tokio::test]
async fn a_signature_on_a_request_that_asks_stores_the_id_of_the_wording_with_the_sentence() {
    let f = fixture().await;
    let id_of = |m: &Made| {
        f.record("sign_request", &m.id)["consent_version"]
            .as_str()
            .unwrap()
            .to_string()
    };
    // The posts below never open the page: `hist=1` is the client's word, and the id stored for a
    // request that asks is the one with the sentence and the mark all the same (docs/SIGN.md,
    // "What keeps the stored id true, and what does not"; O55).
    let flagged_en = made(&f, &f.doc, "Flagged en", Some(true)).await;
    let flagged_de = made(&f, &f.doc, "Flagged de", Some(true)).await;
    let plain_en = made(&f, &f.doc, "Plain en", None).await;
    let off_de = made(&f, &f.doc, "Off de", Some(false)).await;
    assert_eq!(
        sign_in(&f, &flagged_en, &f.file, "Jane Test", "en", true, 120)
            .await
            .status,
        303
    );
    assert_eq!(
        sign_in(&f, &flagged_de, &f.file, "Jane Test", "de", true, 121)
            .await
            .status,
        303
    );
    assert_eq!(
        sign_in(&f, &plain_en, &f.file, "Jane Test", "en", false, 122)
            .await
            .status,
        303
    );
    assert_eq!(
        sign_in(&f, &off_de, &f.file, "Jane Test", "de", false, 123)
            .await
            .status,
        303
    );
    assert_eq!(id_of(&flagged_en), WITH_HISTORY_EN);
    assert_eq!(id_of(&flagged_de), WITH_HISTORY_DE);
    assert_eq!(id_of(&plain_en), PLAIN_EN);
    assert_eq!(id_of(&off_de), PLAIN_DE);
    // In the same language the two differ, and the log entry carries what the request stores.
    assert_ne!(id_of(&flagged_en), id_of(&plain_en));
    assert_ne!(id_of(&flagged_de), id_of(&off_de));
    let detail = |m: &Made| {
        f.events_of("sign_signed")
            .into_iter()
            .find(|e| e["user"] == format!("sign_request:{}", m.id))
            .unwrap()["detail"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert!(detail(&flagged_en).contains(&format!("consent {WITH_HISTORY_EN};")));
    assert!(detail(&plain_en).contains(&format!("consent {PLAIN_EN};")));
    // The digest vouches for the id that was stored.
    let rec = f.record("sign_request", &flagged_en.id);
    assert!(detail(&flagged_en).contains(&format!(
        "digest {}",
        digest_of_request(&flagged_en.link_id, &flagged_en.id, &rec)
    )));
}

#[tokio::test]
async fn a_form_made_before_the_switch_was_turned_on_is_not_signed_and_comes_back_with_the_sentence()
 {
    let f = fixture().await;
    let m = made(&f, &f.doc, "Late switch", None).await;
    let first = page(&f, &m, 130).await.text();
    assert!(!first.contains("name=\"hist\"") && !first.contains("shown to the other people"));
    // A manager ticks the box after the signer opened the page.
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/update",
            json!({"id": m.id, "show_history": true}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    // The signer presses Sign on the page they have: it carried no sentence.
    let r = sign(&f, &m, &f.file, "Jane Test", false, 131).await;
    assert_eq!(r.status, 400, "{}", r.text());
    let html = r.text();
    assert!(
        html.contains("The page has changed since you opened it"),
        "{html}"
    );
    // The form is shown again, with what was typed, the list and the sentence.
    assert!(
        html.contains("value=\"Jane Test\"") && html.contains("name=\"hist\""),
        "{html}"
    );
    assert!(html.contains("shown to the other people who sign this file."));
    assert!(html.contains("Nobody has signed this file yet."));
    // Nothing was written.
    assert_eq!(f.record("sign_request", &m.id)["state"], "sent");
    assert!(f.events_of("sign_signed").is_empty());
    // They read it and press Sign again: signed, under the wording they were shown.
    assert_eq!(
        sign(&f, &m, &f.file, "Jane Test", true, 132).await.status,
        303
    );
    assert_eq!(
        f.record("sign_request", &m.id)["consent_version"],
        WITH_HISTORY_EN
    );

    // The other way round costs nothing: a page that carried the sentence, signed after the switch
    // was turned off, is signed under the wording of a request that does not ask, and is listed
    // nowhere.
    let back = made(&f, &f.doc, "Switch back", Some(true)).await;
    let (s, _) = f
        .api(
            "mia",
            "/api/sign_request/update",
            json!({"id": back.id, "show_history": false}),
        )
        .await;
    assert_eq!(s, 200);
    assert_eq!(
        sign(&f, &back, &f.file, "Bea Example", true, 133)
            .await
            .status,
        303
    );
    assert_eq!(
        f.record("sign_request", &back.id)["consent_version"],
        PLAIN_EN
    );
    let other = made(&f, &f.doc, "Other", Some(true)).await;
    assert_eq!(names(&page(&f, &other, 134).await.text()), ["Jane Test"]);
}

#[tokio::test]
async fn a_switch_changed_between_the_reads_and_the_write_signs_nothing() {
    // Turned on after the door read the request: the signer would be signed under the wording of
    // a request that does not ask, and then listed.
    let (f, _, path, _) = sent().await;
    let b = f.app.store.backend();
    let rid = f.req.clone();
    let fired = interleave(&f, move || {
        b.update("sign_request", &rid, &fields(json!({"show_history": true})))
            .unwrap();
    });
    let r = f
        .post(
            &path,
            &ip(140),
            &[
                ("name", "Jane Test"),
                ("agree", "1"),
                ("doc", &f.file),
                ("lang", "en"),
            ],
        )
        .await;
    assert!(
        fired.load(Ordering::SeqCst),
        "the change landed between the reads and the write"
    );
    assert_eq!(r.status, 409, "{}", r.text());
    let req = f.record("sign_request", &f.req);
    assert_eq!(req["state"], "sent");
    assert!(req.get("signed_name").is_none_or(J::is_null));
    assert!(f.events_of("sign_signed").is_empty());

    // Turned off after the door read it: the signer read the sentence and would be signed under
    // the other wording.
    let f = fixture().await;
    let m = made(&f, &f.doc, "Flagged", Some(true)).await;
    let b = f.app.store.backend();
    let rid = m.id.clone();
    let fired = interleave(&f, move || {
        b.update(
            "sign_request",
            &rid,
            &fields(json!({"show_history": false})),
        )
        .unwrap();
    });
    let r = sign(&f, &m, &f.file, "Jane Test", true, 141).await;
    assert!(fired.load(Ordering::SeqCst));
    assert_eq!(r.status, 409, "{}", r.text());
    assert_eq!(f.record("sign_request", &m.id)["state"], "sent");
    assert!(f.events_of("sign_signed").is_empty());
}

// ---- who may set the switch --------------------------------------------------------------------

fn err(v: &J) -> String {
    v["error"].as_str().unwrap_or("").to_string()
}

/// A draft request made as `who`, with the keys of `extra` added to the body.
async fn create_as(f: &Fx, who: &str, extra: J) -> (u16, J) {
    let mut body = json!({"title": "By someone", "signer_name": "Jane Test", "state": "draft"});
    for (k, v) in extra.as_object().unwrap() {
        body[k] = v.clone();
    }
    f.api(who, "/api/sign_request/create", body).await
}

const ROW: &str = "title,signer_name,state,show_history\nCsv,Jane Test,draft,true\n";

/// Every way a request is written, with the switch in it: (create, update of `id`, a row of a bulk
/// create, a CSV column). Each is tried as `who`; the answers are the four statuses.
async fn write_with_the_switch(f: &Fx, who: &str, id: &str, value: &J) -> [(u16, J); 4] {
    let create = create_as(f, who, json!({"show_history": value})).await;
    let update = f
        .api(
            who,
            "/api/sign_request/update",
            json!({"id": id, "show_history": value}),
        )
        .await;
    let row = f
        .api(
            who,
            "/api/sign_request/bulk_create",
            json!({"rows": [{"title": "Row", "signer_name": "Jane Test", "state": "draft",
                             "show_history": value}]}),
        )
        .await;
    let csv = f
        .api(who, "/api/sign_request/bulk_create", json!({"csv": ROW}))
        .await;
    [create, update, row, csv]
}

#[tokio::test]
async fn only_a_manager_or_an_administrator_sets_the_switch() {
    let f = fixture().await;
    // `sam` and `dan` hold sign.user and no more.
    for who in ["sam", "dan"] {
        for value in [json!(true), json!(false), J::Null] {
            for (i, (s, v)) in write_with_the_switch(&f, who, &f.req, &value)
                .await
                .iter()
                .enumerate()
            {
                let way = ["create", "update", "bulk row", "csv"][i];
                // The CSV carries its own value, `true`: it is tried once per value, harmlessly.
                assert_eq!(*s, 400, "{who} {way} with {value}: {v}");
                assert!(
                    err(v).contains("show_history") && err(v).contains("sign.manager"),
                    "{who} {way}: {v}"
                );
            }
        }
        // The same person makes requests that do not name the switch.
        let (s, v) = create_as(&f, who, json!({})).await;
        assert_eq!(s, 200, "{who}: {v}");
    }
    assert_eq!(
        f.app.store.backend().count("sign_request").unwrap(),
        3,
        "the draft and two plain"
    );
    assert!(
        f.record("sign_request", &f.req)
            .get("show_history")
            .is_none_or(J::is_null)
    );

    // A manager (`mia`, `ann`, `noc`) or an administrator (`admin`, `root`) sets it in every way a
    // request is written, and takes it off.
    for who in ["mia", "ann", "noc", "admin", "root"] {
        let [(cs, cv), (us, uv), (rs, rv), (vs, vv)] =
            write_with_the_switch(&f, who, &f.req, &json!(true)).await;
        for (s, v, way) in [
            (cs, &cv, "create"),
            (us, &uv, "update"),
            (rs, &rv, "bulk row"),
            (vs, &vv, "csv"),
        ] {
            assert_eq!(s, 200, "{who} {way}: {v}");
        }
        let id = cv["value"]["id"].as_str().unwrap().to_string();
        assert_eq!(f.record("sign_request", &id)["show_history"], true, "{who}");
        assert_eq!(
            f.record("sign_request", &f.req)["show_history"],
            true,
            "{who}"
        );
        let (s, v) = f
            .api(
                who,
                "/api/sign_request/update",
                json!({"id": f.req, "show_history": false}),
            )
            .await;
        assert_eq!(s, 200, "{who} switches it off: {v}");
        assert_eq!(
            f.record("sign_request", &f.req)["show_history"],
            false,
            "{who}"
        );
    }
    let on = f
        .app
        .store
        .backend()
        .list("sign_request")
        .unwrap()
        .into_iter()
        .filter(|(_, r)| r.get("show_history") == Some(&json!(true)))
        .count();
    assert_eq!(
        on,
        5 * 3,
        "five callers made three requests each with the switch on"
    );

    // A person with sign.user edits a request that is on, as the back office does, and the switch
    // stays on: a person who is not shown the field does not send it back.
    let (s, v) = create_as(&f, "mia", json!({"show_history": true})).await;
    assert_eq!(s, 200, "{v}");
    let id = v["value"]["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api(
            "sam",
            "/api/sign_request/update",
            json!({"id": id, "title": "Retitled"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(f.record("sign_request", &id)["show_history"], true);
    // ... and is not shown the switch, while a manager is.
    let (s, v) = f
        .api("sam", "/api/sign_request/get", json!({"id": id}))
        .await;
    assert_eq!(s, 200, "{v}");
    assert!(v["value"].get("show_history").is_none(), "{v}");
    let (_, v) = f
        .api("mia", "/api/sign_request/get", json!({"id": id}))
        .await;
    assert_eq!(v["value"]["show_history"], true, "{v}");
}

#[tokio::test]
async fn an_agent_token_sets_the_switch_only_with_the_manager_group_in_its_scopes() {
    let mut f = fixture().await;
    let tokens: [(&str, &'static str, &[&str]); 5] = [
        // The owner holds both groups; the token only the user group.
        ("mia", "agent_user", &["sign.user", "documents.user"]),
        // Both groups.
        (
            "mia",
            "agent_manager",
            &["sign.user", "sign.manager", "documents.user"],
        ),
        // An administrator's token scoped to the administrator group alone.
        ("root", "agent_admin_only", &["platform.admin"]),
        // The administrator group and the user group, not the manager group.
        ("root", "agent_admin_user", &["platform.admin", "sign.user"]),
        // Its owner holds the user group only.
        ("sam", "agent_sam", &["sign.user"]),
    ];
    for (owner, key, scopes) in tokens {
        let (s, v) = f
            .api(
                owner,
                "/api/auth/agent-token",
                json!({"scopes": scopes, "label": key}),
            )
            .await;
        assert_eq!(s, 200, "{key}: {v}");
        f.tokens
            .insert(key, v["token"].as_str().unwrap().to_string());
    }
    // These can call the commands of `sign_request` (they hold `sign.user`) and are refused the
    // switch by the wrapper, whatever the value.
    for key in ["agent_user", "agent_admin_user", "agent_sam"] {
        for value in [json!(true), json!(false)] {
            for (i, (s, v)) in write_with_the_switch(&f, key, &f.req, &value)
                .await
                .iter()
                .enumerate()
            {
                let way = ["create", "update", "bulk row", "csv"][i];
                assert_eq!(*s, 400, "{key} {way} with {value}: {v}");
                assert!(err(v).contains("show_history"), "{key} {way}: {v}");
            }
        }
    }
    // The administrator group alone, as a scope, reaches none of the commands (an agent token is
    // never an administrator here): not found, before the wrapper is asked.
    for (i, (s, v)) in write_with_the_switch(&f, "agent_admin_only", &f.req, &json!(true))
        .await
        .iter()
        .enumerate()
    {
        let way = ["create", "update", "bulk row", "csv"][i];
        assert_eq!(*s, 404, "agent_admin_only {way}: {v}");
    }
    assert!(
        f.record("sign_request", &f.req)
            .get("show_history")
            .is_none_or(J::is_null)
    );
    // The token that holds the manager group may: it is a caller with `sign.manager`, like the
    // person whose token it is (docs/SIGN.md says so).
    let [(cs, cv), (us, uv), (rs, rv), (vs, vv)] =
        write_with_the_switch(&f, "agent_manager", &f.req, &json!(true)).await;
    for (s, v, way) in [
        (cs, &cv, "create"),
        (us, &uv, "update"),
        (rs, &rv, "bulk row"),
        (vs, &vv, "csv"),
    ] {
        assert_eq!(s, 200, "{way}: {v}");
    }
    assert_eq!(f.record("sign_request", &f.req)["show_history"], true);
}

#[tokio::test]
async fn once_signed_the_switch_is_an_administrators_to_change() {
    let f = fixture().await;
    let m = made(&f, &f.doc, "Signed", Some(true)).await;
    assert_eq!(
        sign(&f, &m, &f.file, "Jane Test", true, 150).await.status,
        303
    );
    for who in ["mia", "ann", "noc"] {
        let (s, v) = f
            .api(
                who,
                "/api/sign_request/update",
                json!({"id": m.id, "show_history": false}),
            )
            .await;
        assert_eq!(s, 400, "{who}: {v}");
        assert!(err(&v).contains("signed request cannot be changed"), "{v}");
    }
    assert_eq!(f.record("sign_request", &m.id)["show_history"], true);
    let (s, v) = f
        .api(
            "admin",
            "/api/sign_request/update",
            json!({"id": m.id, "show_history": false}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(f.record("sign_request", &m.id)["show_history"], false);
    // Taken out, the request is in nobody's list any more.
    let other = made(&f, &f.doc, "Other", Some(true)).await;
    assert!(
        page(&f, &other, 151)
            .await
            .text()
            .contains("Nobody has signed this file yet.")
    );
}

// ---- the order of a deploy and the package ----------------------------------------------------

/// An instance whose `sign` is the package of before the history (0.2.1), with the people, the
/// Document and the draft request of `fixture()`, over a store whose reads are counted.
async fn on_0_2_1() -> (Fx, Arc<Flaky>) {
    let (flaky, backend) = Flaky::memory();
    let bytes = Arc::new(MemoryBytes::default());
    let store =
        Store::over(backend.clone()).with_blobs(Arc::new(Catalog::new(backend, bytes.clone())));
    let mut f = fixture_sync_with(store, Some(bytes), None, SIGN_0_2_1);
    seed(&mut f).await;
    (f, flaky)
}

fn installed_sign(f: &Fx) -> String {
    f.app.installer.installed_manifests()["sign"]
        .module
        .version
        .clone()
}

/// A second process on the same database: `obop modules update|rollback sign` while the server
/// runs.
fn other_process(f: &Fx) -> App {
    App::with_store(f.app.store.clone())
}

#[tokio::test]
async fn on_the_package_before_the_history_the_door_knows_no_switch_and_stays_as_it_was() {
    let (f, flaky) = on_0_2_1().await;
    assert_eq!(installed_sign(&f), "0.2.1");
    // The package has no such field: a request cannot be made with it, by anyone.
    for who in ["mia", "admin", "root"] {
        let (s, v) = create_as(&f, who, json!({"show_history": true})).await;
        assert_ne!(s, 200, "{who}: {v}");
    }
    assert_eq!(
        f.app.store.backend().count("sign_request").unwrap(),
        1,
        "only the seeded draft"
    );
    // The request is sent and given a link. A record left by a later version, with the switch on,
    // is written straight to the store, and so is a signature on the same file that asks too.
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/send_document",
            json!({"id": f.req}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let (_, path) = f.mint("mia", &f.req.clone()).await;
    f.app
        .store
        .backend()
        .update(
            "sign_request",
            &f.req,
            &fields(json!({"show_history": true})),
        )
        .unwrap();
    stored_signed(
        &f,
        "stored-alice",
        &f.doc,
        &f.file,
        "Alice Example",
        now_micros(),
        true,
    );
    let events_before = f.events().len();
    flaky.reset_reads();
    // The pages are the pages of before: no history, no query, no error.
    let got = four_pages(&f, &f.req.clone(), &path, 160).await;
    assert_eq!(got, BEFORE_THE_HISTORY);
    let reads = flaky.reads();
    assert_eq!(
        reads.finds + reads.keyed,
        0,
        "no query for the history: {reads:?}"
    );
    // The signature carries the id of the wording without the sentence; the post needed no hint.
    assert_eq!(
        f.record("sign_request", &f.req)["consent_version"],
        PLAIN_EN
    );
    // Nothing else was logged: the one new entry is the signature itself.
    assert_eq!(f.events().len(), events_before + 1, "{:?}", f.events());
    assert!(f.events_of("sign_refused").is_empty());
}

#[tokio::test]
async fn the_door_follows_an_update_and_a_rollback_of_the_package_for_the_switch() {
    let (f, flaky) = on_0_2_1().await;
    let other = other_process(&f);
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/send_document",
            json!({"id": f.req}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let (_, path) = f.mint("mia", &f.req.clone()).await;
    f.app
        .store
        .backend()
        .update(
            "sign_request",
            &f.req,
            &fields(json!({"show_history": true})),
        )
        .unwrap();
    stored_signed(
        &f,
        "stored-alice",
        &f.doc,
        &f.file,
        "Alice Example",
        now_micros(),
        true,
    );
    let plain = |html: &str| !html.contains("Signed so far") && !html.contains("name=\"hist\"");

    // 1. The package is 0.2.1: the switch is inert.
    let html = page_of(&f, &path, 170).await;
    assert!(plain(&html), "{html}");

    // 2. Another process updates the package; the server is not told. The first request after it
    // already shows the history for the record that was waiting.
    other
        .installer
        .install(&Bundled.fetch("sign").unwrap(), "marketplace", "test")
        .unwrap();
    assert_eq!(installed_sign(&f), "0.2.2");
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    flaky.reset_reads();
    let html = page_of(&f, &path, 171).await;
    assert!(
        html.contains("<h2>Signed so far</h2>") && html.contains("name=\"hist\""),
        "{html}"
    );
    assert_eq!(names(&html), ["Alice Example"]);
    assert_eq!(flaky.reads().finds, 1);

    // 3. Another process rolls back: the data stays and is inert, and the page is the old one.
    let back = other.installer.rollback("sign", "test").unwrap();
    assert_eq!(back.version, "0.2.1");
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    flaky.reset_reads();
    let html = page_of(&f, &path, 172).await;
    assert!(plain(&html), "{html}");
    assert_eq!(flaky.reads().finds, 0, "inert again: no query");
    assert_eq!(
        f.record("sign_request", &f.req)["show_history"],
        true,
        "the data stays"
    );
    // A post is the first thing the server hears after a rollback, and it follows: it needs no
    // hint, and it is signed under the wording without the sentence.
    let r = f
        .post(
            &path,
            &ip(173),
            &[
                ("name", "Jane Test"),
                ("agree", "1"),
                ("doc", &f.file),
                ("lang", "en"),
            ],
        )
        .await;
    assert_eq!(r.status, 303, "{}", r.text());
    assert_eq!(
        f.record("sign_request", &f.req)["consent_version"],
        PLAIN_EN
    );

    // 4. Updated again, the data that stayed is honoured again: a new request that asks sees the
    // signature that carries the mark of the sentence (Alice's) and not the one that was made
    // while the switch was inert (Jane's): she signed under the id of the wording without the
    // sentence, so she was never told her name would be shown. Her box is on in the record, and
    // that is not enough.
    other
        .installer
        .install(&Bundled.fetch("sign").unwrap(), "marketplace", "test")
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let after = made(&f, &f.doc, "After", Some(true)).await;
    assert_eq!(
        names(&page_of(&f, &after.path, 174).await),
        ["Alice Example"]
    );
    assert_eq!(f.record("sign_request", &f.req)["show_history"], true);
}

#[tokio::test]
async fn the_upgrade_from_the_package_before_the_history_adds_one_optional_field_and_nothing_else()
{
    let (f, _) = on_0_2_1().await;
    // Records in every state, as an instance in use has them.
    let mut ids = Vec::new();
    for (i, steps) in [
        &[][..],
        &["send_document"],
        &["send_document", "decline"],
        &["send_document", "expire"],
    ]
    .into_iter()
    .enumerate()
    {
        let (s, v) = create_as(
            &f,
            "mia",
            json!({"title": format!("Old {i}"), "document_id": f.doc}),
        )
        .await;
        assert_eq!(s, 200, "{v}");
        let id = v["value"]["id"].as_str().unwrap().to_string();
        for step in steps {
            let (s, v) = f
                .api(
                    "mia",
                    &format!("/api/sign_request/{step}"),
                    json!({"id": id}),
                )
                .await;
            assert_eq!(s, 200, "{step}: {v}");
        }
        ids.push(id);
    }
    let signed = made(&f, &f.doc, "Signed", None).await;
    assert_eq!(
        sign(&f, &signed, &f.file, "Jane Test", false, 180)
            .await
            .status,
        303
    );
    ids.push(signed.id.clone());
    ids.push(f.req.clone());
    let before: Vec<_> = ids.iter().map(|id| f.record("sign_request", id)).collect();

    let old = obop_pkg::Manifest::parse(SIGN_0_2_1).unwrap();
    let new_text = Bundled.fetch("sign").unwrap();
    let new = obop_pkg::Manifest::parse(&new_text).unwrap();
    assert_eq!(
        (old.module.version.as_str(), new.module.version.as_str()),
        ("0.2.1", "0.2.2")
    );
    let changes = obop_pkg::migrate::diff(&old, &new);
    assert_eq!(changes.len(), 1, "{changes:?}");
    let c = &changes[0];
    assert_eq!(
        (c.kind, c.model.as_str(), c.field.as_deref(), c.breaking),
        ("field_added", "sign_request", Some("show_history"), false),
        "{c:?}"
    );
    let field = new
        .model
        .iter()
        .find(|m| m.name == "sign_request")
        .unwrap()
        .field
        .iter()
        .find(|f| f.name == "show_history")
        .unwrap();
    assert_eq!(field.kind, "bool");
    assert!(!field.required && field.group.as_deref() == Some("sign.manager"));
    // The dependencies and the core it needs are what they were.
    assert_eq!(old.module.requires_core, new.module.requires_core);
    assert_eq!(old.module.dependencies, new.module.dependencies);

    // The plan the Modules page shows: one upgrade, nothing that blocks, no data step.
    let (plan, _) = f
        .app
        .installer
        .plan(PkgRequest::Manifest(&new_text))
        .unwrap();
    assert!(plan.ok(), "{:?}", plan.blockers);
    let steps: Vec<_> = plan.work().map(|s| (s.id.clone(), s.action)).collect();
    assert_eq!(steps, vec![("sign".to_string(), StepAction::Upgrade)]);
    let data = &plan.data["sign"];
    assert!(
        data.blockers.is_empty() && data.steps.is_empty() && data.warnings.is_empty(),
        "{data:?}"
    );
    assert_eq!(data.changes.len(), 1);

    let out = f
        .app
        .installer
        .install_outcome(&new_text, "marketplace", "test")
        .unwrap();
    assert_eq!(out.root.version, "0.2.2");
    // Not a record was rewritten.
    let after: Vec<_> = ids.iter().map(|id| f.record("sign_request", id)).collect();
    assert_eq!(after, before);
    // And going back keeps them too.
    f.app.installer.rollback("sign", "test").unwrap();
    assert_eq!(installed_sign(&f), "0.2.1");
    let back: Vec<_> = ids.iter().map(|id| f.record("sign_request", id)).collect();
    assert_eq!(back, before);
}

// ---- the list is served only while the link is within its end ---------------------------------

const DAY: i64 = 24 * 3_600_000_000;

/// The end of a link, as the service reads it.
fn link_end(f: &Fx, link_id: &str) -> i64 {
    let rec = f.record("sign_link", link_id);
    obop_values::datetime::Instant::parse(rec["expires_at"].as_str().unwrap())
        .unwrap()
        .micros()
}

fn at_micros(f: &Fx, micros: i64) {
    f.service().set_clock(Arc::new(move || micros));
}

/// The page of a link in English, from one source (a test has far fewer than 120 views).
async fn view(f: &Fx, path: &str) -> Resp {
    f.get(&format!("{path}?lang=en"), &ip(220)).await
}

/// 200 and the page that follows signing, with no list on it and no name of anyone.
fn no_list(r: &Resp) {
    assert_eq!(r.status, 200, "{}", r.text());
    no_names(r);
}

/// Whatever the status, no list and no name of anyone on the page.
fn no_names(r: &Resp) {
    let html = r.text();
    assert!(
        !html.contains("Signed so far")
            && !html.contains("Nobody has signed")
            && !html.contains("<li>")
            && !html.contains("(you)"),
        "a list on a page that must have none:\n{html}"
    );
    for name in [
        "Alpha", "Beta", "Gamma", "Delta", "Echo", "Plain", "Example",
    ] {
        assert!(!html.contains(name), "{name} is on the page:\n{html}");
    }
}

/// The page of a link that is over is the page of the link of a request that never asked, signed
/// at the same instant: byte for byte, the request id and the secret aside.
async fn is_the_plain_end_page(f: &Fx, m: &Made, plain: &Made, why: &str) {
    let (got, want) = (view(f, &m.path).await, view(f, &plain.path).await);
    no_list(&got);
    assert_eq!(
        normalised(&got.text(), &m.id, m.token()),
        normalised(&want.text(), &plain.id, plain.token()),
        "{why}"
    );
}

#[tokio::test]
async fn the_list_is_served_only_while_the_link_is_within_its_end_and_not_revoked() {
    let f = fixture().await;
    let base = now_micros();
    let a = made(&f, &f.doc, "Alpha", Some(true)).await;
    let b = made(&f, &f.doc, "Beta", Some(true)).await;
    let c = made(&f, &f.doc, "Gamma", Some(true)).await;
    // The plain request signs at the same instant as Alpha: its page that follows signing is the
    // page Alpha's must be once Alpha's link is over.
    let plain = made(&f, &f.doc, "Plain", None).await;
    let d = made(&f, &f.doc, "Delta", Some(true)).await;
    let e = made(&f, &f.doc, "Echo", Some(true)).await;
    // Second links, never used: Gamma's, and Echo's, whose end will be unreadable.
    let (second_id, second) = f.mint("mia", &c.id).await;
    let (echo2_id, echo2) = f.mint("mia", &e.id).await;
    for (secs, m, name, hist, n) in [
        (10, &a, "Alpha Example", true, 200),
        (10, &plain, "Plain Example", false, 201),
        (20, &b, "Beta Example", true, 202),
        (30, &c, "Gamma Example", true, 203),
        (50, &d, "Delta Example", true, 204),
        (55, &e, "Echo Example", true, 205),
    ] {
        clock_at(&f, base, secs);
        assert_eq!(sign(&f, m, &f.file, name, hist, n).await.status, 303);
    }
    // The last day of Delta's request is shortened to 3 days on after it signed (an administrator
    // may change a signed request): its link was made for 14 days and keeps them, and it is the
    // request's day that ends the list. (A link made after the day was set is made to end with it.)
    let ends_on = obop_values::datetime::Instant::from_micros(base + 3 * DAY)
        .unwrap()
        .canonical()[..10]
        .to_string();
    let (s, v) = f
        .api(
            "admin",
            "/api/sign_request/update",
            json!({"id": d.id, "expires_on": ends_on}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let day_end = obop_values::datetime::Instant::parse(&format!("{ends_on}T23:59:59.999999Z"))
        .unwrap()
        .micros();
    assert!(
        link_end(&f, &d.link_id) > day_end + DAY,
        "only the day of the request ends this list"
    );
    let all = [
        "Alpha Example",
        "Beta Example",
        "Gamma Example",
        "Delta Example",
        "Echo Example",
    ];

    // Within its end, the link that signed and a second link of a signed request list everyone.
    clock_at(&f, base, 60);
    for path in [&a.path, &second, &d.path, &echo2] {
        assert_eq!(names(&view(&f, path).await.text()), all, "{path}");
    }
    // A second link whose end cannot be read is a link that does not work: no list.
    f.app
        .store
        .backend()
        .update(
            "sign_link",
            &echo2_id,
            &fields(json!({"expires_at": "not a time"})),
        )
        .unwrap();
    no_list(&view(&f, &echo2).await);
    assert_eq!(
        f.get(&format!("{echo2}/document.pdf"), &ip(221))
            .await
            .status,
        410
    );

    // The end of the request's day: the list is there at the last microsecond and gone at the next.
    at_micros(&f, day_end);
    assert_eq!(
        names(&view(&f, &d.path).await.text()),
        all,
        "the last microsecond"
    );
    at_micros(&f, day_end + 1);
    no_list(&view(&f, &d.path).await);
    assert_eq!(
        names(&view(&f, &a.path).await.text()),
        all,
        "a request that has not ended, with a link that has days left"
    );

    // The end of a link: the same, to the microsecond, for the link that signed ...
    let end_a = link_end(&f, &a.link_id);
    at_micros(&f, end_a);
    assert_eq!(names(&view(&f, &a.path).await.text()), all, "at its end");
    at_micros(&f, end_a + 1);
    is_the_plain_end_page(&f, &a, &plain, "the link that signed, 1 microsecond over").await;
    // ... and for a second link of a signed request: `status()` shows it the page that follows
    // signing whatever its end, and the list on it stops with the link.
    let end_second = link_end(&f, &second_id);
    assert!(
        end_second > end_a + 1,
        "the second link was made after the first"
    );
    at_micros(&f, end_second);
    assert_eq!(names(&view(&f, &second).await.text()), all);
    at_micros(&f, end_second + 1);
    no_list(&view(&f, &second).await);

    // A used link 40 days after signing, a link 100 days old that was never used: no list, and
    // the page that follows signing is the page it was before the history.
    at_micros(&f, base + 40 * DAY);
    is_the_plain_end_page(&f, &a, &plain, "the link that signed, 40 days on").await;
    no_list(&view(&f, &b.path).await);
    no_list(&view(&f, &second).await);
    at_micros(&f, base + 100 * DAY);
    is_the_plain_end_page(&f, &a, &plain, "the link that signed, 100 days on").await;
    no_list(&view(&f, &second).await);
    // The PDF has always been refused to these.
    for path in [&a.path, &second] {
        assert_eq!(
            f.get(&format!("{path}/document.pdf"), &ip(222))
                .await
                .status,
            410
        );
    }

    // A link that was revoked. A manager cannot revoke a link that signed; an administrator can,
    // and the link then shows the page it always did, without the list.
    clock_at(&f, base, 60);
    assert_eq!(names(&view(&f, &a.path).await.text()), all);
    let revoke_a = json!({"id": a.link_id, "revoked_at": "2026-11-04T12:00:00Z"});
    let (s, v) = f
        .api("mia", "/api/sign_link/update", revoke_a.clone())
        .await;
    assert_eq!(s, 400, "{v}");
    assert_eq!(names(&view(&f, &a.path).await.text()), all, "still shown");
    let (s, v) = f.api("admin", "/api/sign_link/update", revoke_a).await;
    assert_eq!(s, 200, "{v}");
    is_the_plain_end_page(&f, &a, &plain, "the link that signed, revoked").await;
    // A second link that was never used is revoked by a manager: 410, as it always was.
    assert_eq!(names(&view(&f, &second).await.text()), all);
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_link/update",
            json!({"id": second_id, "revoked_at": "2026-11-04T12:00:00Z"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let r = view(&f, &second).await;
    assert_eq!(r.status, 410);
    no_names(&r);
}

// ---- a signature is listed only if its signer was told ------------------------------------------

#[tokio::test]
async fn a_link_that_is_over_or_revoked_makes_no_query_for_the_list() {
    let (f, flaky) = counted().await;
    let base = now_micros();
    let a = made(&f, &f.doc, "Alpha", Some(true)).await;
    let b = made(&f, &f.doc, "Beta", Some(true)).await;
    clock_at(&f, base, 10);
    assert_eq!(
        sign(&f, &a, &f.file, "Alpha Example", true, 250)
            .await
            .status,
        303
    );
    assert_eq!(
        sign(&f, &b, &f.file, "Beta Example", true, 251)
            .await
            .status,
        303
    );
    // Within its end the page after signing is one `find`.
    flaky.reset_reads();
    assert_eq!(names(&page(&f, &a, 252).await.text()).len(), 2);
    assert_eq!(flaky.reads().finds, 1);
    // Over its end, or revoked: the same page as for any link, and no query at all for the list.
    at_micros(&f, base + 20 * DAY);
    flaky.reset_reads();
    no_list(&page(&f, &a, 253).await);
    let reads = flaky.reads();
    assert_eq!(
        (
            reads.finds,
            reads.keyed,
            reads.lists,
            reads.scans,
            reads.counts
        ),
        (0, 0, 0, 0, 0),
        "{reads:?}"
    );
    clock_at(&f, base, 30);
    let (s, v) = f
        .api(
            "admin",
            "/api/sign_link/update",
            json!({"id": b.link_id, "revoked_at": "2026-11-04T12:00:00Z"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    flaky.reset_reads();
    no_list(&page(&f, &b, 254).await);
    assert_eq!(flaky.reads().finds, 0);
}

#[tokio::test]
async fn a_signature_made_without_the_sentence_is_listed_to_nobody_whatever_the_box_says_later() {
    let f = fixture().await;
    let base = now_micros();
    // Alice signs a request whose box is not ticked: her page said nothing about a list.
    let alice = made(&f, &f.doc, "Alice", None).await;
    clock_at(&f, base, 10);
    assert_eq!(
        sign(&f, &alice, &f.file, "Alice Example", false, 230)
            .await
            .status,
        303
    );
    assert_eq!(
        f.record("sign_request", &alice.id)["consent_version"],
        PLAIN_EN
    );
    // An administrator ticks the box on her signed request afterwards (nobody else may).
    for who in ["mia", "ann"] {
        let (s, v) = f
            .api(
                who,
                "/api/sign_request/update",
                json!({"id": alice.id, "show_history": true}),
            )
            .await;
        assert_eq!(s, 400, "{who}: {v}");
    }
    let (s, v) = f
        .api(
            "admin",
            "/api/sign_request/update",
            json!({"id": alice.id, "show_history": true}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(f.record("sign_request", &alice.id)["show_history"], true);
    // A request that asks and has not been signed lists nobody: Alice never read the sentence.
    let dave = made(&f, &f.doc, "Dave", Some(true)).await;
    let html = page(&f, &dave, 231).await.text();
    assert!(html.contains("Nobody has signed this file yet."), "{html}");
    assert!(!html.contains("Alice") && !html.contains("<li>"), "{html}");

    // Bob signs a request that asks, on a page that carried the sentence: his id says so.
    let bob = made(&f, &f.doc, "Bob", Some(true)).await;
    clock_at(&f, base, 20);
    assert_eq!(
        sign(&f, &bob, &f.file, "Bob Example", true, 232)
            .await
            .status,
        303
    );
    assert_eq!(
        f.record("sign_request", &bob.id)["consent_version"],
        WITH_HISTORY_EN
    );
    // Dave sees Bob and not Alice. Bob's page after signing has himself, marked, and not Alice.
    // Alice's own page, whose box is on now, has Bob and not herself.
    assert_eq!(names(&page(&f, &dave, 233).await.text()), ["Bob Example"]);
    assert_eq!(
        marked(&page(&f, &bob, 234).await.text()),
        [("Bob Example".to_string(), true)]
    );
    assert_eq!(
        marked(&page(&f, &alice, 235).await.text()),
        [("Bob Example".to_string(), false)]
    );
}

#[tokio::test]
async fn an_id_with_the_mark_keeps_a_signature_listed_whatever_the_hash_part_says() {
    let f = fixture().await;
    let base = now_micros();
    let viewer = made(&f, &f.doc, "Viewer", Some(true)).await;
    // (name, the consent id stored with the signature, listed). The first three are ids of a
    // wording that is not the wording of today: a later edit of the sentence moves the hash of
    // the signatures that follow and takes nobody out of the list.
    let cases: [(&str, &str, bool); 14] = [
        ("Old Wording", "v1-draft/en/00000000/history", true),
        ("Other Language", "v1-draft/de/ffffffff/history", true),
        ("Other Version", "v2/pl/12345678/history", true),
        ("Today", WITH_HISTORY_EN, true),
        ("Plain", PLAIN_EN, false),
        ("No Mark", "v1-draft/en/e2880db0", false),
        ("Empty", "", false),
        ("Bare", "v1-draft", false),
        ("Mark Only", "history", false),
        ("Capital", "v1-draft/en/e2880db0/History", false),
        ("Extra Part", "v1-draft/en/e2880db0/history/x", false),
        ("No Hash", "v1-draft/en//history", false),
        ("Three Parts", "v1-draft/en/history", false),
        ("Spaced", "v1-draft/en/e2880db0/history ", false),
    ];
    for (i, (name, consent, _)) in cases.iter().enumerate() {
        stored_signed_as(
            &f,
            &format!("case-{i:02}"),
            &f.doc,
            &f.file,
            &format!("{name} Example"),
            base + i as i64 * 1_000_000,
            true,
            consent,
        );
    }
    // A record that holds no consent id at all, one that holds null and one that holds a number.
    for (i, value) in [None, Some(J::Null), Some(json!(5))]
        .into_iter()
        .enumerate()
    {
        let mut rec = json!({
            "title": "Raw", "signer_name": "Raw", "document_id": f.doc, "document_hash": f.file,
            "state": "signed", "show_history": true,
            "signed_at": "2026-11-03T09:00:00.000000Z", "signed_name": format!("Raw {i} Example"),
        });
        if let Some(v) = value {
            rec["consent_version"] = v;
        }
        f.app
            .store
            .backend()
            .insert("sign_request", &format!("raw-{i}"), &fields(rec))
            .unwrap();
    }
    let want: Vec<String> = cases
        .iter()
        .filter(|c| c.2)
        .map(|c| format!("{} Example", c.0))
        .collect();
    assert_eq!(names(&page(&f, &viewer, 240).await.text()), want);
}

// ---- requests with no Document or no hash share no list -----------------------------------------

/// What a stored record can hold in a place that should hold the Document or the hash.
fn blanks() -> [(&'static str, J); 2] {
    [("an empty text", json!("")), ("null", J::Null)]
}

/// A flagged request that signs at the door, and then loses its Document or its hash (an
/// administrator, an import or an older version could leave a record so); and another signed
/// request, written to the store, that has the same blank. The two must not list each other, and
/// neither may list anybody else.
async fn blank_pair(which: &str, value: &J, n: u8) {
    let f = fixture().await;
    let base = now_micros();
    let viewer = made(&f, &f.doc, "Viewer", Some(true)).await;
    clock_at(&f, base, 10);
    assert_eq!(
        sign(&f, &viewer, &f.file, "Viewer Example", true, n)
            .await
            .status,
        303
    );
    // A third request about the real file, signed and listed, that must not show up either.
    stored_signed(
        &f,
        "real",
        &f.doc,
        &f.file,
        "Real Example",
        base + 5_000_000,
        true,
    );
    let b = f.app.store.backend();
    // The viewer's own record is blanked in the one place.
    let mut blank = serde_json::Map::new();
    blank.insert(which.to_string(), value.clone());
    b.update("sign_request", &viewer.id, &blank).unwrap();
    // The other record has the same blank and, in the other place, what the viewer has: so that
    // the blank is the only thing that could make the two alike.
    let (doc, hash) = match which {
        "document_id" => (value.clone(), json!(f.file)),
        _ => (json!(f.doc), value.clone()),
    };
    b.insert(
        "sign_request",
        "blank",
        &fields(json!({
            "title": "Blank", "signer_name": "Blank", "document_id": doc, "document_hash": hash,
            "state": "signed", "show_history": true,
            "signed_at": "2026-11-03T09:00:00.000000Z", "signed_name": "Blank Example",
            "consent_version": WITH_HISTORY_EN,
        })),
    )
    .unwrap();
    // And one that is blank in both places.
    b.insert(
        "sign_request",
        "blank-both",
        &fields(json!({
            "title": "Both", "signer_name": "Both", "document_id": value, "document_hash": value,
            "state": "signed", "show_history": true,
            "signed_at": "2026-11-03T09:00:01.000000Z", "signed_name": "Both Example",
            "consent_version": WITH_HISTORY_EN,
        })),
    )
    .unwrap();
    let r = page(&f, &viewer, n + 1).await;
    let html = r.text();
    assert_eq!(r.status, 200, "{html}");
    for gone in [
        "Blank Example",
        "Both Example",
        "Real Example",
        "<li>",
        "(you)",
    ] {
        assert!(
            !html.contains(gone),
            "{which} = {value}: {gone} is on the page:\n{html}"
        );
    }
    assert!(
        !html.contains("Signed so far"),
        "{which} = {value}: a request about no file has no list at all:\n{html}"
    );
}

#[tokio::test]
async fn requests_with_no_document_or_no_hash_never_list_each_other() {
    let mut n = 10;
    for which in ["document_id", "document_hash"] {
        for (_, value) in blanks() {
            blank_pair(which, &value, n).await;
            n += 4;
        }
    }
}

#[tokio::test]
async fn two_requests_with_neither_a_document_nor_a_hash_share_no_list() {
    let f = fixture().await;
    let base = now_micros();
    let viewer = made(&f, &f.doc, "Viewer", Some(true)).await;
    clock_at(&f, base, 10);
    assert_eq!(
        sign(&f, &viewer, &f.file, "Viewer Example", true, 60)
            .await
            .status,
        303
    );
    let b = f.app.store.backend();
    b.update(
        "sign_request",
        &viewer.id,
        &fields(json!({"document_id": "", "document_hash": ""})),
    )
    .unwrap();
    for (i, (_, value)) in blanks().into_iter().enumerate() {
        b.insert(
            "sign_request",
            &format!("none-{i}"),
            &fields(json!({
                "title": "None", "signer_name": "None", "document_id": value,
                "document_hash": value, "state": "signed", "show_history": true,
                "signed_at": "2026-11-03T09:00:00.000000Z",
                "signed_name": format!("None {i} Example"),
                "consent_version": WITH_HISTORY_EN,
            })),
        )
        .unwrap();
    }
    // One with no such keys at all.
    b.insert(
        "sign_request",
        "none-missing",
        &fields(json!({
            "title": "None", "signer_name": "None", "state": "signed", "show_history": true,
            "signed_at": "2026-11-03T09:00:00.000000Z", "signed_name": "None Missing Example",
            "consent_version": WITH_HISTORY_EN,
        })),
    )
    .unwrap();
    let html = page(&f, &viewer, 61).await.text();
    assert!(
        !html.contains("Example") && !html.contains("<li>") && !html.contains("Signed so far"),
        "{html}"
    );
}

// ---- no line to standard error, no read beyond the old ones ------------------------------------

/// One walk through the door for a request that has a link (and a second one): the page in two
/// languages, the PDF, the signature (with the field of a page that carried the sentence when
/// `hist`), the page that follows it from the link that signed and from the second one, and the
/// answers to links that are no links. Every status is checked, so the walk is a test of its own.
async fn walk(f: &Fx, rid: &str, first: &str, second: &str, hist: bool, n: u8) {
    for (path, want) in [
        (format!("{first}?lang=en"), 200),
        (format!("{first}?lang=de"), 200),
        (format!("{first}/document.pdf"), 200),
        (
            format!(
                "/sign/{rid}/{}?lang=en",
                "1".repeat(first.len() - rid.len() - 7)
            ),
            404,
        ),
    ] {
        assert_eq!(f.get(&path, &ip(n)).await.status, want, "{path}");
    }
    let m = Made {
        id: rid.to_string(),
        link_id: String::new(),
        path: first.to_string(),
    };
    assert_eq!(
        sign(f, &m, &f.file, "Jane Test", hist, n + 1).await.status,
        303
    );
    for path in [first, second] {
        assert_eq!(
            f.get(&format!("{path}?lang=en"), &ip(n + 2)).await.status,
            200
        );
        assert_eq!(
            f.get(&format!("{path}/document.pdf"), &ip(n + 2))
                .await
                .status,
            410
        );
    }
}

/// The paths a quiet request takes: no field at all, the field off, the field on, and, on an
/// instance whose package is still 0.2.1, a record that holds the field.
async fn the_walks_that_must_stay_quiet() {
    let f = fixture().await;
    for (i, flag) in [None, Some(false), Some(true)].into_iter().enumerate() {
        let m = made(&f, &f.doc, &format!("Quiet {i}"), flag).await;
        let (_, second) = f.mint("mia", &m.id).await;
        walk(
            &f,
            &m.id,
            &m.path,
            &second,
            flag == Some(true),
            10 + 10 * i as u8,
        )
        .await;
    }
    let (f, _) = on_0_2_1().await;
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/send_document",
            json!({"id": f.req}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let (_, first) = f.mint("mia", &f.req.clone()).await;
    let (_, second) = f.mint("mia", &f.req.clone()).await;
    f.app
        .store
        .backend()
        .update(
            "sign_request",
            &f.req,
            &fields(json!({"show_history": true})),
        )
        .unwrap();
    walk(&f, &f.req.clone(), &first, &second, false, 100).await;
}

/// What a refused store makes the door write: the one line of `unavailable`.
async fn the_walk_that_makes_a_line() {
    let (f, flaky) = counted().await;
    let m = made(&f, &f.doc, "Noisy", None).await;
    flaky.refuse_model("sign_request", Mode::Reads);
    let r = f.get(&format!("{}?lang=en", m.path), &ip(200)).await;
    assert_eq!(r.status, 503, "{}", r.text());
}

#[tokio::test]
async fn walks_that_must_stay_quiet() {
    the_walks_that_must_stay_quiet().await;
}

#[tokio::test]
async fn a_walk_that_makes_a_line() {
    the_walk_that_makes_a_line().await;
}

/// The test binary again, with only `test` run and its output let through, so that what the test
/// wrote to standard output and standard error can be read. (`libtest` keeps the output of a test
/// to itself, and the door's only sink is `eprintln!`.)
fn in_a_child(test: &str) -> (String, String, String) {
    let path = format!("{}::{test}", module_path!().split_once("::").unwrap().1);
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([path.as_str(), "--exact", "--nocapture", "--test-threads=1"])
        .output()
        .unwrap();
    let (stdout, stderr) = (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    );
    assert!(out.status.success(), "{path} failed:\n{stdout}\n{stderr}");
    (path, stdout, stderr)
}

#[test]
fn the_door_writes_no_line_for_a_request_that_does_not_ask_or_on_the_package_before_the_history() {
    // The control first: the same kind of walk, with a store that refuses, is heard. If this
    // fails, the way the lines are read below is blind.
    let (path, stdout, stderr) = in_a_child("a_walk_that_makes_a_line");
    assert!(
        stderr.contains("sign: the store refused"),
        "the line of a refused store was not heard on standard error of {path}:\n{stdout}\n{stderr}"
    );
    // The walks: nothing on standard error, and on standard output only what the harness says.
    let (path, stdout, stderr) = in_a_child("walks_that_must_stay_quiet");
    assert_eq!(stderr.trim(), "", "{path} wrote to standard error");
    let said: Vec<&str> = stdout
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty() && !l.starts_with("running ") && !l.starts_with("test result:"))
        .collect();
    assert_eq!(
        said,
        [format!("test {path} ... ok")],
        "{path} wrote to standard output"
    );
}

/// The new code has no sink of its own: no print, no `dbg!`, no tracing or log macro and no
/// entry of the security log. (The walks above hear a line; this reads for a sink they would not
/// reach, such as a line on a path of a refused store.)
#[test]
fn the_history_module_has_no_sink_for_a_line() {
    let source = include_str!("../../../kernel/obop-commands/src/sign/history.rs");
    let code: String = source
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for sink in [
        "println!",
        "eprintln!",
        "print!",
        "eprint!",
        "dbg!",
        "tracing::",
        "log::",
        "record_event",
        "event_op",
        "kv_put",
        "std::io::stderr",
        "std::io::stdout",
    ] {
        assert!(!code.contains(sink), "history.rs holds `{sink}`");
    }
    // The one line the door writes is the one for a refused store, in one place.
    let door = include_str!("../../../kernel/obop-commands/src/sign/door.rs");
    assert_eq!(door.matches("eprintln!").count(), 1);
    assert!(door.contains("eprintln!(\"sign: the store refused: {e}\");"));
}

/// The reads of the door, as the service made them on `origin/main` (commit 09861505, before the
/// history), for a request that does not ask: a throwaway test that did these steps and printed
/// the counters of `Flaky` (with the two counters `count` and `kv_reads`, which did not exist
/// then and are 0 there). The pages of a request that does not ask read exactly these rows.
fn reads_before_the_history() -> [(&'static str, Reads); 5] {
    let gets = |gets| Reads {
        gets,
        ..Reads::default()
    };
    [
        ("the page", gets(4)),
        ("the PDF", gets(5)),
        ("the signature", gets(5)),
        ("the page after signing, from the link that signed", gets(2)),
        ("the page after signing, from a second link", gets(2)),
    ]
}

/// The steps of that measurement, on a request that was sent and has two links.
async fn door_reads(f: &Fx, flaky: &Flaky, p1: &str, p2: &str) -> Vec<(&'static str, Reads)> {
    let mut out = Vec::new();
    flaky.reset_reads();
    assert_eq!(f.get(&format!("{p1}?lang=en"), &ip(1)).await.status, 200);
    out.push(("the page", flaky.reads()));
    flaky.reset_reads();
    assert_eq!(
        f.get(&format!("{p1}/document.pdf"), &ip(2)).await.status,
        200
    );
    out.push(("the PDF", flaky.reads()));
    flaky.reset_reads();
    let r = f
        .post(
            p1,
            &ip(3),
            &[
                ("name", "Jane Test"),
                ("agree", "1"),
                ("doc", &f.file),
                ("lang", "en"),
            ],
        )
        .await;
    assert_eq!(r.status, 303, "{}", r.text());
    out.push(("the signature", flaky.reads()));
    flaky.reset_reads();
    assert_eq!(f.get(&format!("{p1}?lang=en"), &ip(4)).await.status, 200);
    out.push((
        "the page after signing, from the link that signed",
        flaky.reads(),
    ));
    flaky.reset_reads();
    assert_eq!(f.get(&format!("{p2}?lang=en"), &ip(5)).await.status, 200);
    out.push(("the page after signing, from a second link", flaky.reads()));
    out
}

async fn counted_request(flag: Option<bool>, old_package: bool) -> (Fx, Arc<Flaky>) {
    let (f, flaky) = if old_package {
        self::on_0_2_1().await
    } else {
        counted().await
    };
    if let Some(v) = flag {
        let (s, r) = f
            .api(
                "mia",
                "/api/sign_request/update",
                json!({"id": f.req, "show_history": v}),
            )
            .await;
        assert_eq!(s, 200, "{r}");
    }
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/send_document",
            json!({"id": f.req}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    if old_package {
        // A record left by a later version.
        f.app
            .store
            .backend()
            .update(
                "sign_request",
                &f.req,
                &fields(json!({"show_history": true})),
            )
            .unwrap();
    }
    (f, flaky)
}

#[tokio::test]
async fn the_pages_of_a_request_that_does_not_ask_read_exactly_what_they_read_before_the_history() {
    // No field, the field off, and a record that holds the field on the package of before.
    for (flag, old, what) in [
        (None, false, "no field"),
        (Some(false), false, "the field off"),
        (
            None,
            true,
            "the field on, on the package before the history",
        ),
    ] {
        let (f, flaky) = counted_request(flag, old).await;
        let (_, p1) = f.mint("mia", &f.req.clone()).await;
        let (_, p2) = f.mint("mia", &f.req.clone()).await;
        let got = door_reads(&f, &flaky, &p1, &p2).await;
        let want = reads_before_the_history();
        for ((step, got), (_, want)) in got.iter().zip(want.iter()) {
            assert_eq!(got, want, "{what}: {step}");
        }
        assert_eq!(got.len(), want.len());
    }
}

// ---- the guard of the switch in a bulk create, in every spelling the importer reads -----------

fn count_of(f: &Fx, model: &str) -> usize {
    f.app.store.backend().count(model).unwrap()
}

#[tokio::test]
async fn a_csv_import_cannot_carry_the_switch_in_any_spelling() {
    let f = fixture().await;
    let row = "Forged,Jane Test,true\n";
    // The spellings of `a_csv_import_cannot_carry_evidence_columns_in_any_spelling`, with the
    // column of the switch.
    let cases: Vec<(&str, String)> = vec![
        (
            "plain header",
            format!("title,signer_name,show_history\n{row}"),
        ),
        (
            "a blank line first",
            format!("\ntitle,signer_name,show_history\n{row}"),
        ),
        (
            "a lone CR after the header",
            format!("title,signer_name,show_history\r{row}"),
        ),
        (
            "a quoted header with an inner space",
            format!("title,signer_name,\" show_history\"\n{row}"),
        ),
        (
            "a quoted header that starts with a line break",
            format!("title,signer_name,\"\nshow_history\"\n{row}"),
        ),
        (
            "a byte order mark and a blank line",
            format!("\u{feff}\ntitle,signer_name,show_history\n{row}"),
        ),
        (
            "semicolons",
            "title;signer_name;show_history\nForged;Jane Test;true\n".to_string(),
        ),
        (
            "the column first",
            "show_history,title,signer_name\ntrue,Forged,Jane Test\n".to_string(),
        ),
    ];
    let before = count_of(&f, "sign_request");
    for who in ["sam", "dan"] {
        for (what, csv) in &cases {
            let (s, v) = f
                .api(who, "/api/sign_request/bulk_create", json!({"csv": csv}))
                .await;
            assert_eq!(s, 400, "{who}, {what}: {v}");
            assert!(
                err(&v).contains("show_history") && err(&v).contains("sign.manager"),
                "{who}, {what}: {v}"
            );
        }
    }
    assert_eq!(count_of(&f, "sign_request"), before, "nothing was made");
    // The same files are good files, and a manager and an administrator import the switch: so the
    // refusals above are the guard's and not a file that could not be read.
    for who in ["mia", "admin"] {
        for (what, csv) in &cases {
            let (s, v) = f
                .api(who, "/api/sign_request/bulk_create", json!({"csv": csv}))
                .await;
            assert_eq!(s, 200, "{who}, {what}: {v}");
        }
    }
    let forged: Vec<_> = f
        .app
        .store
        .backend()
        .list("sign_request")
        .unwrap()
        .into_iter()
        .filter(|(_, r)| r["title"] == "Forged")
        .collect();
    assert_eq!(forged.len(), 2 * cases.len());
    assert!(forged.iter().all(|(_, r)| r["show_history"] == true));
    // An ordinary file still imports for the person with no more than `sign.user`.
    let (s, v) = f
        .api(
            "sam",
            "/api/sign_request/bulk_create",
            json!({"csv": "title,signer_name\nPlain row,Jane Test\n"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
}

#[tokio::test]
async fn the_switch_in_any_row_of_a_bulk_create_is_refused_to_a_caller_without_the_manager_group() {
    let f = fixture().await;
    let plain = |i: usize| json!({"title": format!("Row {i}"), "signer_name": "Jane Test"});
    let before = count_of(&f, "sign_request");
    for who in ["sam", "dan"] {
        for at in 0..3 {
            for value in [json!(true), json!(false), J::Null] {
                let rows: Vec<J> = (0..3)
                    .map(|i| {
                        let mut r = plain(i);
                        if i == at {
                            r["show_history"] = value.clone();
                        }
                        r
                    })
                    .collect();
                let (s, v) = f
                    .api(who, "/api/sign_request/bulk_create", json!({"rows": rows}))
                    .await;
                assert_eq!(s, 400, "{who}, the key in row {at} with {value}: {v}");
                assert!(
                    err(&v).contains("show_history") && err(&v).contains("sign.manager"),
                    "{who}, row {at}: {v}"
                );
            }
        }
    }
    assert_eq!(
        count_of(&f, "sign_request"),
        before,
        "not one row of a refused list was made"
    );
    // The same three rows without the key are made, and a manager may send the key in the last.
    let rows: Vec<J> = (0..3).map(plain).collect();
    let (s, v) = f
        .api(
            "sam",
            "/api/sign_request/bulk_create",
            json!({"rows": rows}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let mut rows: Vec<J> = (0..3).map(plain).collect();
    rows[2]["show_history"] = json!(true);
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/bulk_create",
            json!({"rows": rows}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(count_of(&f, "sign_request"), before + 6);
}

// ---- the end of a link, in both directions ----------------------------------------------------

const HOUR: i64 = 3_600_000_000;

/// The day `n` days after `base`, as the text `expires_on` holds.
fn day_in(base: i64, n: i64) -> String {
    obop_values::datetime::Instant::from_micros(base + n * DAY)
        .unwrap()
        .canonical()[..10]
        .to_string()
}

/// The last microsecond of that day (UTC), the way the door reads `expires_on`.
fn last_microsecond_of(day: &str) -> i64 {
    obop_values::datetime::Instant::parse(&format!("{day}T23:59:59.999999Z"))
        .unwrap()
        .micros()
}

/// The names listed on the page of a link, which must be a page that works (200).
async fn listed(f: &Fx, path: &str) -> Vec<String> {
    let r = view(f, path).await;
    assert_eq!(r.status, 200, "{}", r.text());
    names(&r.text())
}

/// The status of the held copy of a link, from one source.
async fn pdf_of(f: &Fx, path: &str) -> u16 {
    f.get(&format!("{path}/document.pdf"), &ip(221))
        .await
        .status
}

/// 200 and the form, with the list on it, and the copy is served.
async fn form_with_list(f: &Fx, path: &str, want: &[&str], why: &str) {
    let r = view(f, path).await;
    assert_eq!(r.status, 200, "{why}: {}", r.text());
    assert!(r.text().contains("name=\"agree\""), "{why}: not the form");
    assert_eq!(names(&r.text()), want, "{why}");
    assert_eq!(pdf_of(f, path).await, 200, "{why}: the copy");
}

/// The link of a request that waits has run out: 410 and the words of the page for that, no
/// name of anyone, no copy, and a post signs nothing.
async fn expired_link(f: &Fx, m: &Made, n: u8, why: &str) {
    let r = view(f, &m.path).await;
    assert_eq!(r.status, 410, "{why}: {}", r.text());
    assert!(r.text().contains("This link has expired."), "{why}");
    no_names(&r);
    assert_eq!(pdf_of(f, &m.path).await, 410, "{why}: the copy");
    let r = sign(f, m, &f.file, "Late Example", true, n).await;
    assert_eq!(r.status, 410, "{why}: a post");
    assert_eq!(f.record("sign_request", &m.id)["state"], "sent", "{why}");
}

#[tokio::test]
async fn a_request_that_lasts_60_days_does_not_stretch_a_link_that_was_made_for_14() {
    // The ordinary configuration: the request's day is far off and the link's own end comes first.
    // The end of a link is the earlier of the two (`SignService::ended`); a rule that took the
    // request's day alone would keep every one of these links alive for 60 days.
    let f = fixture().await;
    let base = now_micros();
    let ordinary = json!({"expires_on": day_in(base, 60)});
    let a = made_with(&f, &f.doc, "Alpha", Some(true), ordinary.clone()).await;
    let b = made_with(&f, &f.doc, "Beta", Some(true), ordinary.clone()).await;
    // Signs at the same instant as Alpha: its page after signing is the page Alpha's must become.
    let plain = made_with(&f, &f.doc, "Plain", None, ordinary.clone()).await;
    // Never signed: the link of a request that waits.
    let waiting = made_with(&f, &f.doc, "Gamma", Some(true), ordinary.clone()).await;
    let (second_id, second) = f.mint("mia", &a.id).await;

    let request_day_end = last_microsecond_of(&day_in(base, 60));
    for (what, id) in [
        ("the first link", &a.link_id),
        ("the link that waits", &waiting.link_id),
        ("the second link", &second_id),
    ] {
        let end = link_end(&f, id);
        assert!(
            (end - (base + 14 * DAY)).abs() < HOUR,
            "{what} was made for 14 days"
        );
        assert!(
            end + 40 * DAY < request_day_end,
            "{what} ends weeks before the request's day"
        );
    }

    at_micros(&f, base + 5 * DAY);
    assert_eq!(
        sign(&f, &a, &f.file, "Alpha Example", true, 160)
            .await
            .status,
        303
    );
    assert_eq!(
        sign(&f, &plain, &f.file, "Plain Example", false, 161)
            .await
            .status,
        303
    );
    at_micros(&f, base + 6 * DAY);
    assert_eq!(
        sign(&f, &b, &f.file, "Beta Example", true, 162)
            .await
            .status,
        303
    );
    let all = ["Alpha Example", "Beta Example"];

    // Day 10, within the 14 days: the page that waits works and lists the two, the copy is served,
    // and so do the page after signing of the link that signed and that of the second link.
    at_micros(&f, base + 10 * DAY);
    form_with_list(&f, &waiting.path, &all, "day 10").await;
    assert_eq!(listed(&f, &a.path).await, all);
    assert_eq!(listed(&f, &second).await, all);

    // The link's own end, to the microsecond, for the page that waits ...
    let end_waiting = link_end(&f, &waiting.link_id);
    at_micros(&f, end_waiting);
    form_with_list(&f, &waiting.path, &all, "at its end").await;
    at_micros(&f, end_waiting + 1);
    expired_link(&f, &waiting, 163, "1 microsecond over").await;
    // ... and for the page after signing, of the link that signed and of a second link.
    for (path, end, why) in [
        (&a.path, link_end(&f, &a.link_id), "the link that signed"),
        (&second, link_end(&f, &second_id), "the second link"),
    ] {
        at_micros(&f, end);
        assert_eq!(listed(&f, path).await, all, "{why}, at its end");
        at_micros(&f, end + 1);
        no_list(&view(&f, path).await);
    }

    // Day 20, 6 days past the 14 and 40 short of the 60: the page that waits is gone, the pages
    // after signing show the time and no list.
    at_micros(&f, base + 20 * DAY);
    expired_link(&f, &waiting, 164, "day 20").await;
    no_list(&view(&f, &second).await);
    no_list(&view(&f, &b.path).await);
    is_the_plain_end_page(&f, &a, &plain, "the link that signed, day 20").await;
}

#[tokio::test]
async fn a_request_that_ends_before_its_link_ends_the_link() {
    // The other direction: the request's day is shortened after the links were made for 14 days,
    // and the request's day is the end. A rule that took the later of the two, or the link's own
    // end alone, would keep these alive.
    let f = fixture().await;
    let base = now_micros();
    let a = made(&f, &f.doc, "Alpha", Some(true)).await;
    let early = made(&f, &f.doc, "Beta", Some(true)).await;
    let early_signed = made(&f, &f.doc, "Gamma", Some(true)).await;
    for (i, (m, name, n)) in [
        (&a, "Alpha Example", 170),
        (&early_signed, "Gamma Example", 171),
    ]
    .into_iter()
    .enumerate()
    {
        at_micros(&f, base + DAY + i as i64 * 1_000_000);
        assert_eq!(sign(&f, m, &f.file, name, true, n).await.status, 303);
    }
    let ends_on = day_in(base, 3);
    for m in [&early, &early_signed] {
        let (s, v) = f
            .api(
                "admin",
                "/api/sign_request/update",
                json!({"id": m.id, "expires_on": ends_on}),
            )
            .await;
        assert_eq!(s, 200, "{v}");
    }
    let day_end = last_microsecond_of(&ends_on);
    for m in [&early, &early_signed, &a] {
        assert!(
            link_end(&f, &m.link_id) > day_end + 9 * DAY,
            "the link has days left when the request's day is over"
        );
    }
    let both = ["Alpha Example", "Gamma Example"];

    at_micros(&f, base + 2 * DAY);
    form_with_list(&f, &early.path, &both, "day 2").await;
    assert_eq!(listed(&f, &early_signed.path).await, both);
    // The last microsecond of the request's day, and the next.
    at_micros(&f, day_end);
    form_with_list(&f, &early.path, &both, "the last microsecond").await;
    assert_eq!(listed(&f, &early_signed.path).await, both);
    at_micros(&f, day_end + 1);
    expired_link(&f, &early, 172, "the request's day is over").await;
    no_list(&view(&f, &early_signed.path).await);
    // A request that has no day of its own goes on, with a link that has days left.
    assert_eq!(listed(&f, &a.path).await, both);
}

// ---- the order in which the door looks at a link ---------------------------------------------

/// 200 and the page after signing: the time, and either the list in full or none at all.
fn end_page(r: &Resp, list: Option<&[&str]>, why: &str) {
    assert_eq!(r.status, 200, "{why}: {}", r.text());
    let html = r.text();
    assert!(
        html.contains("This document was signed on"),
        "{why}: not the page after signing:\n{html}"
    );
    match list {
        Some(want) => assert_eq!(names(&html), want, "{why}"),
        None => no_names(r),
    }
}

/// 410 with the words of the page that was meant, and no name on it.
fn gone_with(r: &Resp, words: &str, why: &str) {
    assert_eq!(r.status, 410, "{why}: {}", r.text());
    assert!(r.text().contains(words), "{why}: {}", r.text());
    no_names(r);
}

#[tokio::test]
async fn the_door_looks_at_a_link_in_the_order_the_documents_state() {
    // docs/SIGN.md, "When the list is served", is the order of `status()` in door.rs:
    //   1. the link was used: the page after signing (or "closed" when the request is not signed
    //      or carries no time); the revoke and the end are not looked at;
    //   2. the link was not used and is revoked: "can no longer be used", whatever the request is;
    //   3. the link was not used and the request is signed: the page after signing (or "closed"
    //      when it carries no time); the end of the link is not looked at, the list on the page is
    //      (`history_for_end_page`);
    //   4. the link has ended: "has expired";
    //   5. the request is not waiting any more: "closed";
    //   6. the form.
    let f = fixture().await;
    let base = now_micros();
    // X is signed by its first link and has two more that are never used.
    let x = made(&f, &f.doc, "Alpha", Some(true)).await;
    let (_, x2) = f.mint("mia", &x.id).await;
    let (x3_id, x3) = f.mint("mia", &x.id).await;
    let y = made(&f, &f.doc, "Beta", Some(true)).await; // waits, the link works
    let z = made(&f, &f.doc, "Gamma", Some(true)).await; // waits, the link is revoked
    let v = made(&f, &f.doc, "Delta", Some(true)).await; // declined
    let u = made(&f, &f.doc, "Echo", Some(true)).await; // signed, the time is gone
    let (_, u2) = f.mint("mia", &u.id).await;
    let t = made(&f, &f.doc, "Plain", Some(true)).await; // signed, then moved to declined

    at_micros(&f, base + DAY);
    for (m, name, n) in [
        (&x, "Alpha Example", 180),
        (&u, "Echo Example", 181),
        (&t, "Plain Example", 182),
    ] {
        assert_eq!(sign(&f, m, &f.file, name, true, n).await.status, 303);
    }
    revoke(&f, &z).await;
    let (s, body) = f
        .api("mia", "/api/sign_request/decline", json!({"id": v.id}))
        .await;
    assert_eq!(s, 200, "{body}");
    // Written straight to the store, as an administrator's edit or an import could leave them.
    let b = f.app.store.backend();
    b.update("sign_request", &u.id, &fields(json!({"signed_at": ""})))
        .unwrap();
    b.update("sign_request", &t.id, &fields(json!({"state": "declined"})))
        .unwrap();
    // Echo's request has no time any more and Plain's is not signed: the list holds Alpha alone.
    let alpha = ["Alpha Example"];

    // Within the end of every link.
    end_page(&view(&f, &x.path).await, Some(&alpha), "1, used");
    end_page(&view(&f, &x2).await, Some(&alpha), "3, unused, signed");
    let r = view(&f, &y.path).await;
    assert!(r.status == 200 && r.text().contains("name=\"agree\""), "6");
    gone_with(
        &view(&f, &z.path).await,
        "This link can no longer be used.",
        "2, waiting",
    );
    gone_with(
        &view(&f, &v.path).await,
        "This request is closed.",
        "5, declined",
    );
    gone_with(
        &view(&f, &u.path).await,
        "This request is closed.",
        "1, used, no time",
    );
    gone_with(
        &view(&f, &t.path).await,
        "This request is closed.",
        "1, used, not signed",
    );
    gone_with(
        &view(&f, &u2).await,
        "This request is closed.",
        "3, unused, no time",
    );

    // A manager revokes a link that was never used, of a signed request: 2 comes before 3.
    let (s, body) = f
        .api(
            "mia",
            "/api/sign_link/update",
            json!({"id": x3_id, "revoked_at": "2026-11-04T12:00:00Z"}),
        )
        .await;
    assert_eq!(s, 200, "{body}");
    gone_with(
        &view(&f, &x3).await,
        "This link can no longer be used.",
        "2, unused, signed",
    );
    // An administrator revokes the link that signed: 1 comes before 2. The page stays, the list
    // does not (`history_for_end_page`).
    let (s, body) = f
        .api(
            "admin",
            "/api/sign_link/update",
            json!({"id": x.link_id, "revoked_at": "2026-11-04T12:00:00Z"}),
        )
        .await;
    assert_eq!(s, 200, "{body}");
    end_page(&view(&f, &x.path).await, None, "1, used and revoked");
    end_page(&view(&f, &x2).await, Some(&alpha), "3, unaffected");

    // Every link is 6 days past its end. 3 comes before 4: the page after signing stays, without
    // the list. 1 comes before 2 and 4: used, revoked and over, still the page after signing.
    at_micros(&f, base + 20 * DAY);
    end_page(&view(&f, &x2).await, None, "3 before 4");
    end_page(&view(&f, &x.path).await, None, "1 before 2 and 4");
    // 2 comes before 4: revoked and over is "can no longer be used", not "has expired".
    gone_with(
        &view(&f, &x3).await,
        "This link can no longer be used.",
        "2 before 4, signed",
    );
    gone_with(
        &view(&f, &z.path).await,
        "This link can no longer be used.",
        "2 before 4, waiting",
    );
    // 4 comes before 5: declined and over is "has expired".
    gone_with(
        &view(&f, &v.path).await,
        "This link has expired.",
        "4 before 5",
    );
    gone_with(
        &view(&f, &y.path).await,
        "This link has expired.",
        "4, waiting",
    );
    // The first clause of 1 is not changed by the end either.
    gone_with(
        &view(&f, &u.path).await,
        "This request is closed.",
        "1, used, no time, over",
    );
}

// ---- a form cannot choose the id of its own consent -------------------------------------------

/// A signer's post with these extra fields, from address `n`.
async fn sign_with(
    f: &Fx,
    m: &Made,
    name: &str,
    lang: &str,
    extra: &[(&str, &str)],
    n: u8,
) -> Resp {
    let mut pairs = vec![
        ("name", name),
        ("agree", "1"),
        ("doc", f.file.as_str()),
        ("lang", lang),
    ];
    pairs.extend_from_slice(extra);
    let agent = format!("Browser-of-{n}/1.0");
    f.post_with(
        &m.path,
        &ip(n),
        &pairs,
        &[("origin", ORIGIN), ("user-agent", &agent)],
    )
    .await
}

#[tokio::test]
async fn a_form_cannot_choose_the_consent_id_or_the_mark_that_is_stored_with_its_signature() {
    // The door reads `name`, `agree`, `doc`, `lang` and `hist` from the form and nothing else
    // (`obop-http` `sign.rs`). The id is worked out by the server from the texts of the post's
    // language and picked by the switch of the request as the door reads it. A script that adds
    // fields with the names of the id, of the mark or of the switch, valid-looking or not, gets
    // the id of the request it posts to.
    let f = fixture().await;
    let base = now_micros();
    let garbage = "<script>alert(1)</script>/../;'--";
    let forged_hash = "v1-draft/de/00000000/history";

    // Requests whose box is not ticked: nothing in the form can put the mark on the signature.
    let absent = made(&f, &f.doc, "Absent", None).await;
    let off = made(&f, &f.doc, "Off", Some(false)).await;
    // `hist` alone, on a request that never had the box: the field is compared with the switch of
    // the request and nothing else. (The request whose box was turned off after its page was made
    // is the second half of `a_form_made_before_the_switch_was_turned_on_...`: it is stored under
    // the plain id as well.)
    let hist_only = made(&f, &f.doc, "Hist only", None).await;
    // Requests whose box is ticked.
    let flagged = made(&f, &f.doc, "Flagged", Some(true)).await;
    let flagged_de = made(&f, &f.doc, "Flagged de", Some(true)).await;
    let viewer = made(&f, &f.doc, "Viewer", Some(true)).await;

    // 1. Valid-looking ids of the other wording in every field a script might guess, and the
    //    switch in two spellings, on a request that does not ask. No `hist`.
    clock_at(&f, base, 10);
    let r = sign_with(
        &f,
        &absent,
        "Una Example",
        "en",
        &[
            ("consent_version", WITH_HISTORY_EN),
            ("consent_version_history", WITH_HISTORY_EN),
            ("consent_id", WITH_HISTORY_EN),
            ("consent", WITH_HISTORY_EN),
            ("history", "1"),
            ("show_history", "1"),
            ("show_history", "true"),
        ],
        190,
    )
    .await;
    assert_eq!(r.status, 303, "{}", r.text());
    // 2. Garbage in the same fields and `hist=1`, on a request whose box is explicitly off.
    clock_at(&f, base, 20);
    let r = sign_with(
        &f,
        &off,
        "Ola Example",
        "en",
        &[
            ("consent_version", garbage),
            ("consent_version_history", garbage),
            ("consent_id", garbage),
            ("consent", garbage),
            ("history", garbage),
            ("show_history", "1"),
            ("saw_history", "1"),
            ("hist", "1"),
        ],
        191,
    )
    .await;
    assert_eq!(r.status, 303, "{}", r.text());
    // 3. `hist=1` alone, in German, on a request that never had the box.
    clock_at(&f, base, 30);
    let r = sign_with(&f, &hist_only, "Hans Example", "de", &[("hist", "1")], 192).await;
    assert_eq!(r.status, 303, "{}", r.text());
    // 4. The request asks: the plain id of its language and garbage, with the `hist=1` its page
    //    carried.
    clock_at(&f, base, 40);
    let r = sign_with(
        &f,
        &flagged,
        "Fay Example",
        "en",
        &[
            ("hist", "1"),
            ("consent_version", PLAIN_EN),
            ("consent_version_history", garbage),
            ("consent_id", PLAIN_EN),
            ("consent", PLAIN_EN),
            ("history", "0"),
            ("show_history", "0"),
        ],
        193,
    )
    .await;
    assert_eq!(r.status, 303, "{}", r.text());
    // 5. The request asks, in German: the mark on a hash that is not the wording's.
    clock_at(&f, base, 50);
    let r = sign_with(
        &f,
        &flagged_de,
        "Gita Example",
        "de",
        &[
            ("hist", "1"),
            ("consent_version", WITH_HISTORY_DE),
            ("consent_version_history", forged_hash),
            ("consent_id", forged_hash),
            ("consent", forged_hash),
        ],
        194,
    )
    .await;
    assert_eq!(r.status, 303, "{}", r.text());

    // What is stored is what the server worked out, to the character.
    let stored = |m: &Made| {
        f.record("sign_request", &m.id)["consent_version"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(stored(&absent), PLAIN_EN, "1: no box, forged ids");
    assert_eq!(stored(&off), PLAIN_EN, "2: box off, garbage and hist=1");
    assert_eq!(stored(&hist_only), PLAIN_DE, "3: hist=1 alone, no box");
    assert_eq!(
        stored(&flagged),
        WITH_HISTORY_EN,
        "4: box on, forged plain id"
    );
    assert_eq!(
        stored(&flagged_de),
        WITH_HISTORY_DE,
        "5: box on, forged mark"
    );
    // The log entry and the digest say the same, and no forged text is in any record or entry.
    let details: Vec<String> = f
        .events_of("sign_signed")
        .iter()
        .map(|e| e["detail"].as_str().unwrap().to_string())
        .collect();
    for (m, want) in [
        (&absent, PLAIN_EN),
        (&off, PLAIN_EN),
        (&hist_only, PLAIN_DE),
        (&flagged, WITH_HISTORY_EN),
        (&flagged_de, WITH_HISTORY_DE),
    ] {
        let line = details
            .iter()
            .find(|d| d.starts_with(&format!("request {};", m.id)))
            .unwrap();
        assert!(line.contains(&format!("consent {want};")), "{line}");
        let rec = f.record("sign_request", &m.id);
        assert!(line.contains(&format!(
            "digest {}",
            digest_of_request(&m.link_id, &m.id, &rec)
        )));
    }
    let everything = f.everything_stored();
    for forged in ["alert(1)", "<script>", forged_hash] {
        assert!(
            !everything.contains(forged),
            "{forged} got into the store:\n{everything}"
        );
    }

    // Only the two signatures made under the id with the mark are listed, oldest first, and the
    // signatures of the requests that do not ask are listed nowhere.
    let seen = names(&page(&f, &viewer, 195).await.text());
    assert_eq!(seen, ["Fay Example", "Gita Example"]);
    let end = page(&f, &flagged, 196).await.text();
    assert_eq!(
        marked(&end),
        [
            ("Fay Example".to_string(), true),
            ("Gita Example".to_string(), false)
        ]
    );
}
