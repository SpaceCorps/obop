//! The Drive browser against a fake Drive on a loopback port. No call goes to Google.

use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

const CLIENT_ID: &str = "cid-browse";
const CLIENT_SECRET: &str = "csecret-browse";
const REFRESH: &str = "refresh-browse";

struct Item {
    id: &'static str,
    name: &'static str,
    parent: &'static str,
    mime: &'static str,
    size: Option<u64>,
    data: &'static [u8],
}

const ITEMS: &[Item] = &[
    Item {
        id: "dir1",
        name: "Contracts",
        parent: "root",
        mime: "application/vnd.google-apps.folder",
        size: None,
        data: b"",
    },
    Item {
        id: "doc1",
        name: "NDA template",
        parent: "dir1",
        mime: "application/vnd.google-apps.document",
        size: None,
        data: b"%PDF-1.4 exported",
    },
    Item {
        id: "pdf1",
        name: "Signed NDA.pdf",
        parent: "dir1",
        mime: "application/pdf",
        size: Some(11),
        data: b"%PDF-1.4 ab",
    },
    Item {
        id: "form1",
        name: "Survey",
        parent: "root",
        mime: "application/vnd.google-apps.form",
        size: None,
        data: b"",
    },
    Item {
        id: "big1",
        name: "Huge.bin",
        parent: "root",
        mime: "application/octet-stream",
        size: Some(10_000_000),
        data: b"",
    },
];

#[derive(Default)]
struct State {
    seen: Vec<String>,
    queries: Vec<String>,
    auth_ok: bool,
    shared_drive: bool,
    /// supportsAllDrives, includeItemsFromAllDrives and corpora of each files.list call.
    flags: Vec<(Option<String>, Option<String>, Option<String>)>,
}

struct Fake {
    base: String,
    st: Arc<Mutex<State>>,
}

fn fake() -> Fake {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let st = Arc::new(Mutex::new(State::default()));
    let shared = st.clone();
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(conn) = conn else { continue };
            let st = shared.clone();
            std::thread::spawn(move || serve(conn, &st));
        }
    });
    Fake { base, st }
}

fn browser(f: &Fake) -> DriveBrowser {
    DriveBrowser::new(Config {
        folder_id: "root".into(),
        client_id: CLIENT_ID.into(),
        client_secret: CLIENT_SECRET.into(),
        refresh_token: REFRESH.into(),
        api_base: f.base.clone(),
        token_url: format!("{}/token", f.base),
        retry_pause: Duration::from_millis(1),
        max_attempts: 3,
    })
    .unwrap()
}

fn serve(mut conn: std::net::TcpStream, st: &Arc<Mutex<State>>) {
    let mut buf = vec![0u8; 16384];
    let mut n = 0;
    loop {
        let Ok(k) = conn.read(&mut buf[n..]) else {
            return;
        };
        if k == 0 {
            return;
        }
        n += k;
        let text = &buf[..n];
        if let Some(p) = text.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&text[..p]).into_owned();
            let len = head
                .lines()
                .find_map(|l| {
                    l.to_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                })
                .unwrap_or(0);
            if n >= p + 4 + len {
                break;
            }
        }
    }
    let head = String::from_utf8_lossy(&buf[..n]).into_owned();
    let first = head.lines().next().unwrap_or("");
    let target = first.split(' ').nth(1).unwrap_or("/");
    let u = url::Url::parse(&format!("http://fake{target}")).unwrap();
    let q = |k: &str| {
        u.query_pairs()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.into_owned())
    };
    let reply = |conn: &mut std::net::TcpStream, status: u16, ctype: &str, body: &[u8]| {
        let _ = write!(
            conn,
            "HTTP/1.1 {status} X\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = conn.write_all(body);
    };
    let json = |conn: &mut std::net::TcpStream, status: u16, v: J| {
        reply(conn, status, "application/json", v.to_string().as_bytes())
    };
    let mut s = st.lock().unwrap();
    s.seen.push(format!(
        "{} {}",
        first.split(' ').next().unwrap_or(""),
        u.path()
    ));
    if u.path() == "/token" {
        let body = head.split("\r\n\r\n").nth(1).unwrap_or("");
        let ok = url::form_urlencoded::parse(body.as_bytes())
            .any(|(k, v)| k == "refresh_token" && v == REFRESH);
        if !ok {
            return json(&mut conn, 400, json!({"error": "invalid_grant"}));
        }
        s.auth_ok = true;
        return json(
            &mut conn,
            200,
            json!({"access_token": "at-1", "expires_in": 3599}),
        );
    }
    let authed = head
        .lines()
        .any(|l| l.eq_ignore_ascii_case("authorization: Bearer at-1"));
    if !authed {
        return json(
            &mut conn,
            401,
            json!({"error": {"message": "Invalid Credentials"}}),
        );
    }
    let meta = |i: &Item| {
        let mut m = json!({
            "id": i.id, "name": i.name, "mimeType": i.mime,
            "modifiedTime": "2026-10-30T09:00:00.000Z",
            "webViewLink": format!("https://drive.google.com/file/d/{}/view", i.id),
        });
        if let Some(sz) = i.size {
            m["size"] = json!(sz.to_string());
        }
        m
    };
    match u.path() {
        "/drive/v3/about" => json(
            &mut conn,
            200,
            json!({"user": {"emailAddress": "boss@example.com"}}),
        ),
        "/drive/v3/drives" if s.shared_drive => json(
            &mut conn,
            200,
            json!({"drives": [{"id": "sd1", "name": "SpaceCorps shared"}]}),
        ),
        "/drive/v3/files" => {
            let query = q("q").unwrap_or_default();
            s.flags.push((
                q("supportsAllDrives"),
                q("includeItemsFromAllDrives"),
                q("corpora"),
            ));
            s.queries.push(query.clone());
            let parent = query
                .strip_suffix("' in parents and trashed = false")
                .and_then(|r| r.strip_prefix('\''))
                .map(str::to_string);
            let contains = query
                .strip_prefix("name contains '")
                .and_then(|r| r.strip_suffix("' and trashed = false"))
                .map(|r| r.replace("\\'", "'").replace("\\\\", "\\").to_lowercase());
            let hits: Vec<&Item> = ITEMS
                .iter()
                .filter(|i| match (&parent, &contains) {
                    (Some(p), _) => i.parent == p,
                    (_, Some(c)) => i.name.to_lowercase().contains(c.as_str()),
                    _ => false,
                })
                .collect();
            let start: usize = q("pageToken").and_then(|t| t.parse().ok()).unwrap_or(0);
            let page: Vec<J> = hits.iter().skip(start).take(2).map(|i| meta(i)).collect();
            let mut out = json!({"files": page});
            if start + 2 < hits.len() {
                out["nextPageToken"] = json!((start + 2).to_string());
            }
            json(&mut conn, 200, out)
        }
        p => {
            let rest = p.strip_prefix("/drive/v3/files/").unwrap_or("");
            let (id, export) = match rest.strip_suffix("/export") {
                Some(id) => (id, true),
                None => (rest, false),
            };
            let Some(item) = ITEMS.iter().find(|i| i.id == id) else {
                return json(
                    &mut conn,
                    404,
                    json!({"error": {"message": "File not found"}}),
                );
            };
            if export {
                assert_eq!(q("mimeType").as_deref(), Some("application/pdf"));
                return reply(&mut conn, 200, "application/pdf", item.data);
            }
            if q("alt").as_deref() == Some("media") {
                return reply(&mut conn, 200, "application/octet-stream", item.data);
            }
            json(&mut conn, 200, meta(item))
        }
    }
}

#[test]
fn lists_a_folder_with_folders_flagged_and_pages() {
    let f = fake();
    let b = browser(&f);
    let p = b.list("", "", None).unwrap();
    assert_eq!(p.entries.len(), 2);
    assert!(p.entries[0].folder && !p.entries[0].importable);
    assert_eq!(p.entries[0].name, "Contracts");
    // A Google Form cannot be exported as PDF, so it is shown but not importable.
    assert_eq!(
        (p.entries[1].name.as_str(), p.entries[1].importable),
        ("Survey", false)
    );
    let p2 = b.list("", "", p.next.as_deref()).unwrap();
    let names: Vec<_> = p2
        .entries
        .iter()
        .map(|e| (e.name.as_str(), e.importable))
        .collect();
    assert_eq!(names, [("Huge.bin", true)]);
    assert!(p2.next.is_none());
    let inside = b.list("dir1", "", None).unwrap();
    assert_eq!(inside.entries.len(), 2);
    assert!(inside.entries.iter().all(|e| e.importable));
    assert_eq!(inside.entries[1].size, Some(11));
    assert!(
        inside.entries[0]
            .web_link
            .starts_with("https://drive.google.com/")
    );
}

#[test]
fn searches_by_name_and_escapes_the_term() {
    let f = fake();
    let b = browser(&f);
    let p = b.list("", "nda", None).unwrap();
    assert_eq!(p.entries.len(), 2);
    let _ = b.list("", "o'brien \\ x", None).unwrap();
    let qs = f.st.lock().unwrap().queries.clone();
    assert!(qs.last().unwrap().contains("o\\'brien \\\\ x"), "{qs:?}");
}

#[test]
fn a_hostile_id_never_reaches_a_url() {
    let f = fake();
    let b = browser(&f);
    assert!(b.list("a/../b", "", None).is_err());
    assert!(b.fetch("x?y=1", 1000).is_err());
    assert!(f.st.lock().unwrap().seen.is_empty(), "nothing was sent");
}

#[test]
fn a_google_doc_is_exported_as_pdf_and_a_plain_file_comes_as_is() {
    let f = fake();
    let b = browser(&f);
    let d = b.fetch("doc1", 1 << 20).unwrap();
    assert_eq!(d.name, "NDA template.pdf");
    assert_eq!(d.mime, "application/pdf");
    assert_eq!(d.bytes, b"%PDF-1.4 exported");
    assert!(d.web_link.ends_with("/doc1/view"));
    assert_eq!(d.modified, "2026-10-30T09:00:00.000Z");
    let p = b.fetch("pdf1", 1 << 20).unwrap();
    assert_eq!(p.name, "Signed NDA.pdf");
    assert_eq!(p.bytes, b"%PDF-1.4 ab");
}

#[test]
fn folders_forms_and_oversized_files_are_refused() {
    let f = fake();
    let b = browser(&f);
    assert!(b.fetch("dir1", 1000).unwrap_err().contains("folder"));
    assert!(
        b.fetch("form1", 1000)
            .unwrap_err()
            .contains("kind of Google file")
    );
    assert!(b.fetch("big1", 1000).unwrap_err().contains("larger than"));
    // An export has no size up front: the cap is enforced while reading.
    assert!(b.fetch("doc1", 4).unwrap_err().contains("larger than"));
    assert!(b.fetch("nope", 1000).unwrap_err().contains("404"));
}

#[test]
fn probe_names_the_account_and_a_bad_token_is_reported_without_it() {
    let f = fake();
    assert_eq!(browser(&f).probe().unwrap(), "boss@example.com");
    let mut cfg = Config {
        folder_id: "root".into(),
        client_id: CLIENT_ID.into(),
        client_secret: CLIENT_SECRET.into(),
        refresh_token: "wrong-refresh-value".into(),
        api_base: f.base.clone(),
        token_url: format!("{}/token", f.base),
        retry_pause: Duration::from_millis(1),
        max_attempts: 2,
    };
    let err = DriveBrowser::new(cfg.clone()).unwrap().probe().unwrap_err();
    assert!(err.contains("invalid_grant"), "{err}");
    assert!(!err.contains("wrong-refresh-value") && !err.contains(CLIENT_SECRET));
    cfg.refresh_token = REFRESH.into();
    assert!(DriveBrowser::new(cfg).unwrap().probe().is_ok());
}

#[test]
fn off_unless_the_browse_token_is_set() {
    // The variable is not set in the test environment.
    assert!(DriveBrowser::from_env().unwrap().is_none());
}

#[test]
fn shared_drives_head_the_root_and_their_folders_are_read_across_all_drives() {
    let f = fake();
    f.st.lock().unwrap().shared_drive = true;
    let b = browser(&f);
    let p = b.list("", "", None).unwrap();
    let first = &p.entries[0];
    assert_eq!(
        (first.name.as_str(), first.id.as_str()),
        ("SpaceCorps shared", "sd1")
    );
    assert!(first.folder && !first.importable);
    // My Drive still follows, and a later page of the root does not repeat the shared drives.
    assert_eq!(p.entries[1].name, "Contracts");
    let p2 = b.list("", "", p.next.as_deref()).unwrap();
    assert!(p2.entries.iter().all(|e| e.id != "sd1"));
    // A shared drive opens like a folder, over every drive; so does a search.
    let _ = b.list("sd1", "", None).unwrap();
    let _ = b.list("", "nda", None).unwrap();
    let st = f.st.lock().unwrap();
    for (all, items, corpora) in &st.flags {
        assert_eq!(all.as_deref(), Some("true"));
        assert_eq!(items.as_deref(), Some("true"));
        let _ = corpora;
    }
    let n = st.flags.len();
    assert_eq!(st.flags[n - 2].2.as_deref(), Some("allDrives"));
    assert_eq!(st.flags[n - 1].2.as_deref(), Some("allDrives"));
    assert_eq!(st.flags[0].2, None);
}

#[test]
fn a_failing_shared_drives_call_leaves_my_drive_listing() {
    let f = fake();
    let b = browser(&f);
    let p = b.list("", "", None).unwrap();
    assert_eq!(p.entries[0].name, "Contracts");
}
