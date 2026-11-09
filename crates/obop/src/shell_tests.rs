//! The back office shell with an instance brand (docs/spec/branding.md, slice S2), over the real
//! `App`, the real reader of its dispatcher and the real router, in both places the back office is
//! mounted (beside the API, and under `/admin` next to the public site). The golden pins of an
//! instance with **no** row are `shell_pin_tests`; these tests hold what changes when there is one,
//! and what the store that fails (D74) does to the index document, which is the one deliberate
//! change for an instance with no row (section 14).

use crate::app::App;
use crate::brand_row_tests::set_brand;
use crate::shell_pin_tests::{Mounted, exact, mounted, mounted_over};
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use obop_commands::Store;
use obop_storage::testing::{Flaky, Mode};
use serde_json::Value as J;
use std::sync::Arc;
use tower::ServiceExt;

struct Answer {
    status: u16,
    headers: axum::http::HeaderMap,
    body: Vec<u8>,
}

impl Answer {
    fn header(&self, name: &str) -> &str {
        self.headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

async fn ask(r: &axum::Router, method: &str, path: &str, headers: &[(&str, &str)]) -> Answer {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "inst.test");
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let resp = r
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let (status, headers) = (resp.status().as_u16(), resp.headers().clone());
    let body = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    Answer {
        status,
        headers,
        body,
    }
}

fn flaky_mounted(public: bool, language: Option<&'static str>) -> (Arc<Flaky>, Mounted) {
    let (flaky, backend) = Flaky::memory();
    let app = App::with_store(Store::over(backend));
    (flaky, mounted_over(app, public, language))
}

/// The brand tag of a shell, parsed.
fn tag_of(html: &str) -> J {
    let open = "<meta name=\"obop-brand\" content=\"";
    let start = html.find(open).expect("the obop-brand tag") + open.len();
    let end = start + html[start..].find("\">").unwrap();
    let raw = html[start..end]
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{e}: {raw}"))
}

/// The version of the logo the reader holds and the logo route serves.
fn v_of(m: &Mounted) -> String {
    m.app.dispatcher.brand().current().v().to_string()
}

// ---- With a brand ---------------------------------------------------------------------------------

/// With a row the index of the back office, in both places it is mounted, has the name as its title,
/// the one icon link to the logo route with the version `v` of what that route serves, and the tag
/// the page reads. It is a `200` with `no-cache` and no validators. Nothing else of the build
/// changes: a script, a style sheet and the favicon file are served as they were, and the shop's
/// own pages (the public site's root) do not take the shell.
#[tokio::test]
async fn with_a_brand_the_index_has_the_name_the_icon_and_the_tag_in_both_mountings() {
    for public in [false, true] {
        let m = mounted(public, None);
        let p = m.prefix;
        let files = [
            format!("{p}/assets/app.js"),
            format!("{p}/assets/app.css"),
            format!("{p}/favicon.svg"),
        ];
        let mut before = Vec::new();
        for f in &files {
            before.push(exact(&m.router, "GET", f, &[]).await);
        }
        set_brand(&m.app);
        let v = v_of(&m);
        // The logo route serves the same version in its ETag.
        let logo = ask(&m.router, "GET", "/api/brand/logo.svg", &[]).await;
        assert_eq!(logo.status, 200);
        assert_eq!(logo.header("etag"), format!("\"{v}\""));
        for path in [
            format!("{p}/"),
            format!("{p}/index.html"),
            format!("{p}/customize"),
        ] {
            let a = ask(&m.router, "GET", &path, &[]).await;
            let html = a.text();
            assert_eq!(a.status, 200, "{path}");
            assert_eq!(a.header("cache-control"), "no-cache", "{path}");
            assert!(
                a.headers.get("etag").is_none() && a.headers.get("last-modified").is_none(),
                "{path}"
            );
            assert!(html.contains("<title>MOHV</title>"), "{path}: {html}");
            assert!(!html.contains("<title>obop</title>"), "{path}: {html}");
            assert!(
                html.contains(&format!(
                    "<link rel=\"icon\" href=\"/api/brand/logo.svg?v={v}\" type=\"image/svg+xml\">"
                )),
                "{path}: {html}"
            );
            assert_eq!(html.matches("<link rel=\"icon\"").count(), 1, "{path}");
            assert!(
                !html.contains("favicon.svg\""),
                "{path}: the build's icon link is gone"
            );
            assert!(
                html.contains("assets/app.css"),
                "{path}: the style sheet stays"
            );
            let tag = tag_of(&html);
            assert_eq!(tag["name"], "MOHV");
            assert_eq!(tag["slug"], "mohv");
            assert_eq!(tag["v"], v);
            assert_eq!(
                tag["messages"],
                serde_json::json!({
                    "en": "Welcome to the team",
                    "fi": "Tervetuloa joukkoon",
                    "sv": "Välkommen till laget"
                })
            );
            let mut keys: Vec<&str> = tag
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect();
            keys.sort_unstable();
            assert_eq!(
                keys,
                ["messages", "name", "slug", "v"],
                "{path}: nothing else is public"
            );
            assert!(
                !html.contains("Fahrtenbuch") && !html.contains("Körjournal"),
                "no titles"
            );
        }
        // The files of the build are what they were.
        for (f, was) in files.iter().zip(&before) {
            assert_eq!(&exact(&m.router, "GET", f, &[]).await, was, "{f}");
        }
        // The validators of the browser cannot keep the old page, and a range cannot cut the tag.
        let first = ask(&m.router, "GET", &format!("{p}/"), &[]).await;
        for headers in [
            vec![("if-none-match", "*")],
            vec![("if-modified-since", "Tue, 14 Nov 2033 22:13:20 GMT")],
            vec![("range", "bytes=0-9")],
        ] {
            let again = ask(&m.router, "GET", &format!("{p}/"), &headers).await;
            assert_eq!(
                (again.status, &again.body),
                (200, &first.body),
                "{headers:?}"
            );
        }
        let head = ask(&m.router, "HEAD", &format!("{p}/"), &[]).await;
        assert_eq!(head.status, 200);
        assert!(head.body.is_empty());
        assert_eq!(head.header("content-length"), first.body.len().to_string());
    }
}

/// Both places that mount the shell read the same reader as the rest of the server: a change made
/// through the command shows in the index at once, in either of them, and `clear` gives the file
/// back byte for byte. The theme accent joins the tag when the theme stores one.
#[tokio::test]
async fn a_change_through_the_command_shows_at_once_and_clear_gives_the_file_back() {
    for public in [false, true] {
        let m = mounted(public, Some("sv"));
        let index = format!("{}/", m.prefix);
        let was = exact(&m.router, "GET", &index, &[]).await;
        set_brand(&m.app);
        let a = ask(&m.router, "GET", &index, &[]).await;
        assert!(a.text().contains("<title>MOHV</title>"));
        assert!(
            a.text()
                .contains("name=\"obop-default-language\" content=\"sv\"")
        );
        // Rename through the command, as the dispatcher runs it.
        let reader = m.app.dispatcher.brand();
        obop_commands::brand::apply(
            &*m.app.store.backend(),
            Some(&reader),
            "ada",
            &obop_commands::brand::Action::Configure(vec![("name".into(), "Acme".into())]),
            false,
        )
        .unwrap();
        let a = ask(&m.router, "GET", &index, &[]).await;
        assert!(a.text().contains("<title>Acme</title>"), "{}", a.text());
        assert_eq!(tag_of(&a.text())["name"], "Acme");
        assert_ne!(tag_of(&a.text())["v"], "", "{}", a.text());
        obop_commands::brand::apply(
            &*m.app.store.backend(),
            Some(&reader),
            "ada",
            &obop_commands::brand::Action::Clear,
            false,
        )
        .unwrap();
        assert_eq!(
            exact(&m.router, "GET", &index, &[]).await,
            was,
            "clear gives the shell back byte for byte (public: {public})"
        );
    }
}

/// The theme accent is in the tag when it is a valid colour, and a name that HTML treats as markup
/// is escaped in the title and in the attribute.
#[tokio::test]
async fn the_tag_carries_the_accent_of_the_theme_and_the_name_is_escaped() {
    let m = mounted(false, None);
    let backend = m.app.store.backend();
    backend
        .apply(&[obop_storage::Op::Put {
            model: "obop_setting".into(),
            id: "instance".into(),
            fields: serde_json::json!({"value": "{\"theme\":{\"accent\":\"#c2410c\"}}"})
                .as_object()
                .unwrap()
                .clone(),
        }])
        .unwrap();
    obop_commands::brand::apply(
        &*backend,
        Some(&m.app.dispatcher.brand()),
        "ada",
        &obop_commands::brand::Action::Configure(vec![("name".into(), "Tom & Jerry's".into())]),
        false,
    )
    .unwrap();
    let html = ask(&m.router, "GET", "/", &[]).await.text();
    assert!(
        html.contains("<title>Tom &amp; Jerry&#39;s</title>"),
        "{html}"
    );
    let tag = tag_of(&html);
    assert_eq!(tag["name"], "Tom & Jerry's");
    assert_eq!(tag["accent"], "#c2410c");
}

// ---- The cost ----------------------------------------------------------------------------------------

/// A hundred requests for the index cost one flight of the reader, which is one read of the row and
/// one of the theme setting, and a hundred requests for a script or a style sheet cost none. The cell
/// of the reader stays fresh for its bound, so on a machine so slow that the burst outlasts the bound
/// a second flight is right: what holds is that there is at least one and at most one more for each
/// bound that went by (`1 + elapsed / bound`), and exactly one when the burst took less than the
/// bound. That is a count against the clock the burst itself measured, so there is no time budget and
/// no second try.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_hundred_shell_requests_cost_one_read_of_the_row_and_one_of_the_theme_and_a_file_costs_none()
 {
    for row in [true, false] {
        // A server process that has not read yet, over a store that holds the row or not.
        let (flaky, backend) = Flaky::memory();
        if row {
            let seed = App::with_store(Store::over(backend.clone()));
            set_brand(&seed);
        }
        let m = mounted_over(App::with_store(Store::over(backend)), false, None);
        flaky.reset_reads();
        let started = std::time::Instant::now();
        let mut set = tokio::task::JoinSet::new();
        for _ in 0..100 {
            let r = m.router.clone();
            set.spawn(async move { ask(&r, "GET", "/", &[]).await.status });
        }
        while let Some(s) = set.join_next().await {
            assert_eq!(s.unwrap(), 200);
        }
        let elapsed = started.elapsed();
        let gets = flaky.reads().gets;
        assert_eq!(gets % 2, 0, "row={row}: a flight is two reads, not {gets}");
        let flights = (gets / 2) as u128;
        let most = 1 + elapsed.as_millis() / READER_BOUND.as_millis();
        assert!(
            (1..=most).contains(&flights),
            "row={row}: {flights} flights for a burst of {elapsed:?}, at most {most} may run"
        );
        if elapsed < READER_BOUND {
            assert_eq!(flights, 1, "row={row}: one flight, two reads");
        }
        // Static files ask nobody.
        for path in ["/assets/app.js", "/assets/app.css", "/favicon.svg"] {
            for _ in 0..100 {
                assert_eq!(ask(&m.router, "GET", path, &[]).await.status, 200, "{path}");
            }
        }
        assert_eq!(flaky.reads().gets, gets, "row={row}: files cause no read");
    }
}

// ---- The store that fails (D74) ---------------------------------------------------------------------

/// With the store failing, the index document is answered by D74 also for an instance with no row:
/// the request that runs the refill of a process that never had a good read is a 503 with
/// `Retry-After`, where the file used to be served (spec, section 14: the one deliberate change of an
/// instance with no row). A file that is not the shell is still served, the failure is not kept, and
/// when the store answers the index is the file again.
#[tokio::test]
async fn with_the_store_failing_the_index_is_a_503_also_for_an_instance_with_no_row() {
    for (public, language) in [(false, None), (true, None), (false, Some("sv"))] {
        let (flaky, m) = flaky_mounted(public, language);
        let p = m.prefix;
        let was = exact(&m.router, "GET", &format!("{p}/"), &[]).await;
        // A new server process over the same store: it has never had a good read.
        let (flaky2, backend) = (flaky.clone(), m.app.store.backend());
        let m = {
            drop(m);
            mounted_over(App::with_store(Store::over(backend)), public, language)
        };
        flaky2.refuse_model("instance_brand", Mode::Reads);
        for path in [format!("{p}/"), format!("{p}/customize")] {
            let a = ask(&m.router, "GET", &path, &[]).await;
            assert_eq!(a.status, 503, "{path}: {}", a.text());
            assert_eq!(a.header("retry-after"), "1", "{path}");
            assert_eq!(a.header("x-obop-applied"), "none");
            assert!(a.header("content-type").starts_with("text/html"), "{path}");
            // The small page D74 gives where a page was coming, never the shell.
            assert!(
                a.text().contains("Try again in a moment"),
                "{path}: {}",
                a.text()
            );
            assert!(!a.text().contains("id=\"root\""), "{path}: not the shell");
        }
        // A script, a style sheet and the favicon file need no store.
        for f in ["assets/app.js", "assets/app.css", "favicon.svg"] {
            assert_eq!(
                ask(&m.router, "GET", &format!("{p}/{f}"), &[]).await.status,
                200,
                "{f}"
            );
        }
        assert_eq!(ask(&m.router, "GET", "/healthz", &[]).await.status, 200);
        // The failure is not kept: when the store answers, the index is the file again.
        flaky2.allow_models();
        assert_eq!(exact(&m.router, "GET", &format!("{p}/"), &[]).await, was);
    }
}

/// The wait that a refill needs for the cell to be stale: the bound of the reader the server builds.
const READER_BOUND: std::time::Duration = obop_commands::brand::DEFAULT_BOUND;

/// Waits until the cell, which was filled before `filled` was taken, is stale. A lower bound and no
/// budget: no test passes or fails on how long this takes, and the cell cannot be made older from
/// here (`Reader::back_date` is for the tests of its own crate), so the two seconds are waited.
fn wait_until_stale(filled: std::time::Instant) {
    let stale = READER_BOUND + std::time::Duration::from_millis(50);
    while filled.elapsed() <= stale {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// After a good read, a store that then fails: once the cell has expired, the request that runs the
/// refill meets the fault and is answered 503 with `Retry-After`, never with a shell that says
/// `obop`; the next one reads again; and when the store answers, the brand is back.
#[tokio::test]
async fn after_a_good_read_a_failing_store_answers_the_request_that_runs_the_refill_with_a_503() {
    let (flaky, m) = flaky_mounted(false, None);
    set_brand(&m.app);
    let good = ask(&m.router, "GET", "/", &[]).await;
    assert!(good.text().contains("<title>MOHV</title>"));
    let filled = std::time::Instant::now();
    flaky.refuse_model("instance_brand", Mode::Reads);
    wait_until_stale(filled);
    let before = flaky.refused();
    for _ in 0..2 {
        let a = ask(&m.router, "GET", "/", &[]).await;
        assert_eq!(a.status, 503, "{}", a.text());
        assert_eq!(a.header("retry-after"), "1");
        assert!(a.text().contains("Try again in a moment"), "{}", a.text());
        assert!(
            !a.text().contains("obop"),
            "never a shell that says obop: {}",
            a.text()
        );
    }
    assert!(flaky.refused() >= before + 2, "a failed refill is not kept");
    flaky.allow_models();
    let back = ask(&m.router, "GET", "/", &[]).await;
    assert_eq!((back.status, back.body), (200, good.body));
}

/// What a refill that is held inside the store does to the server, through the index: the request that
/// runs the refill waits for the store and nothing else does. The runtime has one worker, so a read
/// on the thread that polls the requests would stop everything below. While the refill is parked,
/// `/healthz` answers and a hundred requests for the index are answered with the last good brand
/// before the refill is let go; the double counts the one read of the owner and no other.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn while_a_refill_is_parked_in_the_store_the_index_has_the_last_good_brand_and_healthz_answers()
 {
    let (flaky, m) = flaky_mounted(false, None);
    set_brand(&m.app);
    let good = ask(&m.router, "GET", "/", &[]).await;
    assert!(good.text().contains("<title>MOHV</title>"));
    let filled = std::time::Instant::now();
    tokio::task::spawn_blocking(move || wait_until_stale(filled))
        .await
        .unwrap();

    let (parked_tx, parked_rx) = std::sync::mpsc::channel::<()>();
    let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
    flaky.after_the_get_of(
        "instance_brand",
        0,
        Box::new(move |_| {
            parked_tx.send(()).unwrap();
            let _ = go_rx.recv_timeout(std::time::Duration::from_secs(30));
        }),
    );
    flaky.reset_reads();
    let owner = {
        let r = m.router.clone();
        tokio::spawn(async move { ask(&r, "GET", "/", &[]).await })
    };
    let parked = tokio::task::spawn_blocking(move || {
        parked_rx.recv_timeout(std::time::Duration::from_secs(20))
    })
    .await
    .unwrap();
    assert!(parked.is_ok(), "the index request never reached the store");
    assert_eq!(flaky.reads().gets, 1);

    let bound = std::time::Duration::from_secs(20);
    let started = std::time::Instant::now();
    let r = m.router.clone();
    let health = tokio::spawn(async move { ask(&r, "GET", "/healthz", &[]).await })
        .await
        .unwrap();
    assert_eq!(health.status, 200);
    assert!(started.elapsed() < bound, "/healthz waited for the refill");
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..100 {
        let r = m.router.clone();
        set.spawn(async move { ask(&r, "GET", "/", &[]).await });
    }
    let mut answers = Vec::new();
    while let Some(a) = set.join_next().await {
        answers.push(a.unwrap());
    }
    assert!(
        started.elapsed() < bound,
        "a request waited for the parked refill"
    );
    for a in &answers {
        assert_eq!(a.status, 200);
        assert_eq!(a.body, good.body, "the last good brand");
    }
    assert_eq!(
        flaky.reads().gets,
        1,
        "the hundred read nothing: one flight"
    );
    assert!(!owner.is_finished(), "the owner is still in the store");
    go_tx.send(()).unwrap();
    let owner = owner.await.unwrap();
    assert_eq!((owner.status, owner.body), (200, good.body));
    assert_eq!(flaky.reads().gets, 2);
}

/// The first request of a process has no last good brand and runs the refill itself. With a double
/// that holds the first read and then ends it in the pool's timeout error it waits for the store, is
/// answered 503 by D74 with `Retry-After` 5 once the read ends, and `/healthz` answers meanwhile.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn a_first_read_that_times_out_in_the_store_is_answered_503_with_retry_after_5() {
    let (flaky, m) = flaky_mounted(false, None);
    let (parked_tx, parked_rx) = std::sync::mpsc::channel::<()>();
    let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
    let f = flaky.clone();
    flaky.after_the_get_of(
        "instance_brand",
        0,
        Box::new(move |_| {
            parked_tx.send(()).unwrap();
            let _ = go_rx.recv_timeout(std::time::Duration::from_secs(30));
            obop_storage::fault::note(obop_storage::fault::Fault::Timeout);
            f.refuse_next(1);
        }),
    );
    let first = {
        let r = m.router.clone();
        tokio::spawn(async move { ask(&r, "GET", "/", &[]).await })
    };
    let parked = tokio::task::spawn_blocking(move || {
        parked_rx.recv_timeout(std::time::Duration::from_secs(20))
    })
    .await
    .unwrap();
    assert!(parked.is_ok(), "the request never reached the store");
    let r = m.router.clone();
    let health = tokio::spawn(async move { ask(&r, "GET", "/healthz", &[]).await })
        .await
        .unwrap();
    assert_eq!(health.status, 200);
    assert!(!first.is_finished(), "the request waits for the store");
    go_tx.send(()).unwrap();
    let first = first.await.unwrap();
    assert_eq!(first.status, 503, "{}", first.text());
    assert_eq!(first.header("retry-after"), "5");
    assert!(
        first.text().contains("Try again in a moment"),
        "{}",
        first.text()
    );
    // Nothing was kept: the store answers, and so does the index.
    flaky.allow_models();
    assert_eq!(ask(&m.router, "GET", "/", &[]).await.status, 200);
}

/// The reader is asked on a blocking thread and never on the one that polls the request: the
/// PostgreSQL client panics on an async worker.
#[tokio::test]
async fn the_store_is_read_on_a_blocking_thread_and_never_on_the_one_that_polls_the_request() {
    let (flaky, m) = flaky_mounted(false, None);
    let polling = std::thread::current().id();
    let seen = Arc::new(std::sync::Mutex::new(None));
    let noted = seen.clone();
    flaky.after_the_get_of(
        "instance_brand",
        0,
        Box::new(move |_| {
            *noted.lock().unwrap() = Some(std::thread::current().id());
        }),
    );
    assert_eq!(ask(&m.router, "GET", "/", &[]).await.status, 200);
    let reader = seen.lock().unwrap().expect("the reader read the row");
    assert_ne!(reader, polling, "the refill ran on the polling thread");
}

/// The index on the runtime of the server over every store the tests have: SQLite always, and
/// PostgreSQL when `OBOP_TEST_POSTGRES_URL` is set (that leg runs in CI only; it is not run on a
/// machine without the variable). The PostgreSQL client panics on an async worker, so that leg is the
/// proof, against the real client, that the shell asks the reader through the blocking pool. Without
/// PostgreSQL the proof is the test above: the thread that reads the row is not the one that polls
/// the request.
#[test]
fn the_index_works_on_the_runtime_of_the_server_over_every_store() {
    let dir = std::env::temp_dir().join(format!(
        "obop-shell-rt-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            // Best effort. On Windows the files of a SQLite database that this process still has
            // open cannot be removed, and the store of an `App` stays open until the process
            // ends (measured: the removal still fails two seconds after the runtime and the
            // router are gone), so this run's directory may stay; `sweep` removes the ones that
            // earlier runs left.
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    /// The directories of this test that are more than a day old: their processes are long gone.
    fn sweep(prefix: &str) {
        let day = std::time::Duration::from_secs(24 * 60 * 60);
        let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
            return;
        };
        for entry in entries.flatten() {
            let age = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok());
            if age.is_some_and(|a| a > day)
                && entry.file_name().to_string_lossy().starts_with(prefix)
            {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
    sweep("obop-shell-rt-");
    let _scratch = Scratch(dir.clone());
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let mut stores = vec![(
        Store::open(&format!("sqlite://{}", dir.join("shell.db").display())).unwrap(),
        None,
    )];
    if let Some(db) = obop_storage::testing::TestDb::new() {
        stores.push((Store::open(db.url()).unwrap(), Some(db)));
    }
    for (store, _db) in stores {
        let m = mounted_over(App::with_store(store), false, None);
        rt.block_on(async {
            let a = ask(&m.router, "GET", "/", &[]).await;
            assert_eq!(a.status, 200);
            assert!(a.text().contains("<title>obop</title>"), "no brand");
        });
        // `set_brand` reads the store from this thread, which is not a worker of the runtime.
        set_brand(&m.app);
        rt.block_on(async {
            let a = ask(&m.router, "GET", "/", &[]).await;
            assert_eq!(a.status, 200);
            assert!(a.text().contains("<title>MOHV</title>"));
            let mut set = tokio::task::JoinSet::new();
            for _ in 0..16 {
                let r = m.router.clone();
                set.spawn(async move { ask(&r, "GET", "/", &[]).await.status });
            }
            while let Some(s) = set.join_next().await {
                assert_eq!(s.unwrap(), 200);
            }
        });
    }
}
