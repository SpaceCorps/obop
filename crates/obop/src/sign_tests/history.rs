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
use obop_storage::testing::Flaky;
use std::sync::atomic::Ordering;

const SIGN_0_2_1: &str = include_str!("../fixtures/sign-0.2.1.toml");

/// The ids of the two wordings, written out (the unit test of `sign_text.rs` pins the same
/// literals): the one every signature carries today and the one a signature on a request that
/// shows the signatures so far carries.
const PLAIN_EN: &str = "v1-draft/en/a05c68c3";
const PLAIN_DE: &str = "v1-draft/de/9ebff173";
const WITH_HISTORY_EN: &str = "v1-draft/en/e2880db0";
const WITH_HISTORY_DE: &str = "v1-draft/de/d08d0039";

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
    let mut body = json!({"title": title, "signer_name": format!("Requested {title}"),
                          "document_id": doc});
    if let Some(v) = flag {
        body["show_history"] = json!(v);
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
/// version could be: the door must read it as it finds it.
fn stored_signed(f: &Fx, id: &str, doc: &str, hash: &str, name: &str, at_micros: i64, flag: bool) {
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
        })),
    )
    .unwrap();
    b.insert(
        "sign_request",
        "signed-by-hand",
        &fields(json!({
            "title": "By hand", "signer_name": "Requested By hand", "document_id": f.doc,
            "document_hash": f.file, "state": "signed", "show_history": true,
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
                       "show_history": true, "signed_at": "yesterday", "signed_name": "Nina Example"})),
    )
    .unwrap();
    b.insert(
        "sign_request",
        "stored-text-flag",
        &fields(
            json!({"document_id": f.doc, "document_hash": f.file, "state": "signed",
                       "show_history": "true", "signed_at": "2026-11-03T09:00:00.000000Z",
                       "signed_name": "Tex Example"}),
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
    // signature that was made while the switch was inert (its request asked, and it signed).
    other
        .installer
        .install(&Bundled.fetch("sign").unwrap(), "marketplace", "test")
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let after = made(&f, &f.doc, "After", Some(true)).await;
    assert_eq!(
        names(&page_of(&f, &after.path, 174).await),
        ["Alice Example", "Jane Test"]
    );
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
