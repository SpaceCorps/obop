//! The store against a fake Drive on a loopback port. No call goes to Google.

use super::*;
use obop_storage::blob::{Blobs, Catalog, NewBlob, id_of};
use std::collections::VecDeque;
use std::io::Write;
use std::net::TcpListener;

const FOLDER: &str = "folder1";
const CLIENT_ID: &str = "cid-123.apps.example";
const CLIENT_SECRET: &str = "csecret-xyz";
const REFRESH: &str = "refresh-abc";

struct File {
    id: String,
    name: String,
    parent: String,
    mime: String,
    data: Vec<u8>,
}

#[derive(Default)]
struct State {
    files: Vec<File>,
    next: u32,
    token_calls: u32,
    valid_token: String,
    /// Statuses (and bodies) to answer the next Drive requests with, before anything else.
    faults: VecDeque<(u16, String)>,
    api_calls: u32,
    paths: Vec<String>,
    page_size: usize,
    list_calls: u32,
    last_code_form: Vec<(String, String)>,
}

struct Fake {
    base: String,
    st: Arc<Mutex<State>>,
}

impl Fake {
    fn start() -> Fake {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let st = Arc::new(Mutex::new(State {
            page_size: 1000,
            ..State::default()
        }));
        st.lock().unwrap().files.push(File {
            id: FOLDER.into(),
            name: "obop files".into(),
            parent: "root".into(),
            mime: FOLDER_MIME.into(),
            data: vec![],
        });
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

    fn config(&self) -> Config {
        Config {
            folder_id: FOLDER.into(),
            client_id: CLIENT_ID.into(),
            client_secret: CLIENT_SECRET.into(),
            refresh_token: REFRESH.into(),
            api_base: self.base.clone(),
            token_url: format!("{}/token", self.base),
            retry_pause: Duration::from_millis(1),
            max_attempts: 4,
        }
    }

    fn store(&self) -> DriveBytes {
        DriveBytes::new(self.config()).unwrap()
    }

    fn files_in_folder(&self) -> usize {
        let s = self.st.lock().unwrap();
        s.files.iter().filter(|f| f.parent == FOLDER).count()
    }

    fn fail_next(&self, status: u16, body: &str) {
        self.st
            .lock()
            .unwrap()
            .faults
            .push_back((status, body.into()));
    }
}

struct Req {
    method: String,
    path: String,
    query: Vec<(String, String)>,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Req {
    fn q(&self, k: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.as_str())
    }
    fn h(&self, k: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(k))
            .map(|(_, v)| v.as_str())
    }
}

fn read_request(conn: &mut std::net::TcpStream) -> Option<Req> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        let n = conn.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.lines();
    let first = lines.next()?;
    let mut parts = first.split(' ');
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    let len = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < len {
        let n = conn.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    let u = url::Url::parse(&format!("http://fake{target}")).ok()?;
    Some(Req {
        method,
        path: u.path().to_string(),
        query: u
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect(),
        headers,
        body,
    })
}

fn respond(conn: &mut std::net::TcpStream, status: u16, ctype: &str, body: &[u8]) {
    let _ = write!(
        conn,
        "HTTP/1.1 {status} X\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = conn.write_all(body);
}

fn json(conn: &mut std::net::TcpStream, status: u16, v: J) {
    respond(conn, status, "application/json", v.to_string().as_bytes());
}

fn serve(mut conn: std::net::TcpStream, st: &Arc<Mutex<State>>) {
    let Some(req) = read_request(&mut conn) else {
        return;
    };
    let mut s = st.lock().unwrap();
    s.paths.push(format!("{} {}", req.method, req.path));
    if req.path == "/token" {
        let form: Vec<(String, String)> = url::form_urlencoded::parse(&req.body)
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        let get = |k: &str| form.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        if get("client_id") != Some(CLIENT_ID) || get("client_secret") != Some(CLIENT_SECRET) {
            return json(&mut conn, 401, json!({"error": "invalid_client"}));
        }
        match get("grant_type") {
            Some("refresh_token") => {
                if get("refresh_token") != Some(REFRESH) {
                    return json(
                        &mut conn,
                        400,
                        json!({"error": "invalid_grant", "error_description": "Bad refresh-abc"}),
                    );
                }
                s.token_calls += 1;
                s.valid_token = format!("at-{}", s.token_calls);
                let t = s.valid_token.clone();
                json(
                    &mut conn,
                    200,
                    json!({"access_token": t, "expires_in": 3599}),
                )
            }
            Some("authorization_code") => {
                s.last_code_form = form.clone();
                if get("code") != Some("the-code") {
                    return json(&mut conn, 400, json!({"error": "invalid_grant"}));
                }
                json(
                    &mut conn,
                    200,
                    json!({"access_token": "x", "refresh_token": "rt-new", "expires_in": 3599}),
                )
            }
            _ => json(&mut conn, 400, json!({"error": "unsupported_grant_type"})),
        }
        return;
    }

    s.api_calls += 1;
    if let Some((status, body)) = s.faults.pop_front() {
        return respond(&mut conn, status, "application/json", body.as_bytes());
    }
    let auth = req.h("authorization").unwrap_or("");
    if auth != format!("Bearer {}", s.valid_token) {
        return json(
            &mut conn,
            401,
            json!({"error": {"code": 401, "message": "Invalid Credentials"}}),
        );
    }
    let not_found = |conn: &mut std::net::TcpStream| {
        json(
            conn,
            404,
            json!({"error": {"code": 404, "message": "File not found"}}),
        )
    };
    let id_of_path = |p: &str| p.rsplit('/').next().unwrap_or("").to_string();

    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/drive/v3/files") => {
            s.list_calls += 1;
            let q = req.q("q").unwrap_or("");
            let name = q
                .strip_prefix("name = '")
                .and_then(|r| r.split('\'').next())
                .map(str::to_string);
            let parent = q
                .split(" and ")
                .find_map(|p| {
                    p.strip_suffix("' in parents")
                        .and_then(|p| p.rsplit('\'').next())
                })
                .unwrap_or("")
                .to_string();
            let matching: Vec<&File> = s
                .files
                .iter()
                .filter(|f| f.parent == parent && name.as_ref().is_none_or(|n| &f.name == n))
                .collect();
            let start: usize = req.q("pageToken").and_then(|t| t.parse().ok()).unwrap_or(0);
            let page: Vec<J> = matching
                .iter()
                .skip(start)
                .take(s.page_size)
                .map(|f| json!({"id": f.id, "name": f.name}))
                .collect();
            let mut out = json!({ "files": page });
            if start + s.page_size < matching.len() {
                out["nextPageToken"] = json!((start + s.page_size).to_string());
            }
            json(&mut conn, 200, out)
        }
        ("GET", p) if p.starts_with("/drive/v3/files/") => {
            let id = id_of_path(p);
            let Some(f) = s.files.iter().find(|f| f.id == id) else {
                return not_found(&mut conn);
            };
            if req.q("alt") == Some("media") {
                let data = f.data.clone();
                return respond(&mut conn, 200, "application/octet-stream", &data);
            }
            json(
                &mut conn,
                200,
                json!({"id": f.id, "name": f.name, "mimeType": f.mime, "trashed": false}),
            )
        }
        ("POST", "/upload/drive/v3/files") => {
            let ctype = req.h("content-type").unwrap_or("");
            let Some(boundary) = ctype.split("boundary=").nth(1) else {
                return json(&mut conn, 400, json!({"error": {"message": "no boundary"}}));
            };
            let delim = format!("--{boundary}");
            let body = &req.body;
            let find = |from: usize, pat: &[u8]| {
                body[from..]
                    .windows(pat.len())
                    .position(|w| w == pat)
                    .map(|p| p + from)
            };
            let first = find(0, delim.as_bytes()).unwrap();
            let meta_start = find(first, b"\r\n\r\n").unwrap() + 4;
            let second = find(meta_start, delim.as_bytes()).unwrap();
            let meta: J = serde_json::from_slice(&body[meta_start..second - 2]).unwrap();
            let data_start = find(second, b"\r\n\r\n").unwrap() + 4;
            let end = find(data_start, format!("\r\n{delim}--").as_bytes()).unwrap();
            s.next += 1;
            let id = format!("file{}", s.next);
            let parent = meta["parents"][0].as_str().unwrap_or("").to_string();
            let name = meta["name"].as_str().unwrap_or("").to_string();
            let data = body[data_start..end].to_vec();
            s.files.push(File {
                id: id.clone(),
                name,
                parent,
                mime: "application/octet-stream".into(),
                data,
            });
            json(&mut conn, 200, json!({ "id": id }))
        }
        ("PATCH", p) if p.starts_with("/upload/drive/v3/files/") => {
            let id = id_of_path(p);
            let body = req.body.clone();
            match s.files.iter_mut().find(|f| f.id == id) {
                Some(f) => {
                    f.data = body;
                    json(&mut conn, 200, json!({ "id": id }))
                }
                None => not_found(&mut conn),
            }
        }
        ("POST", "/drive/v3/files") => {
            let meta: J = serde_json::from_slice(&req.body).unwrap_or(J::Null);
            s.next += 1;
            let id = format!("folder{}", s.next + 1);
            s.files.push(File {
                id: id.clone(),
                name: meta["name"].as_str().unwrap_or("").into(),
                parent: meta["parents"][0].as_str().unwrap_or("root").into(),
                mime: meta["mimeType"].as_str().unwrap_or("").into(),
                data: vec![],
            });
            json(&mut conn, 200, json!({ "id": id }))
        }
        ("DELETE", p) if p.starts_with("/drive/v3/files/") => {
            let id = id_of_path(p);
            let before = s.files.len();
            s.files.retain(|f| f.id != id);
            if s.files.len() == before {
                return not_found(&mut conn);
            }
            respond(&mut conn, 204, "text/plain", b"")
        }
        _ => not_found(&mut conn),
    }
}

fn key(n: u32) -> String {
    id_of(format!("blob {n}").as_bytes())
}

fn read(r: Box<dyn Read + Send>) -> Vec<u8> {
    let mut r = r;
    let mut out = Vec::new();
    r.read_to_end(&mut out).unwrap();
    out
}

#[test]
fn put_and_open_round_trip_binary_bytes_and_overwrite_in_place() {
    let fake = Fake::start();
    let s = fake.store();
    let bytes: Vec<u8> = (0..=255u8).cycle().take(70_000).collect();
    s.put(&key(1), &bytes).unwrap();
    assert_eq!(read(s.open(&key(1)).unwrap().unwrap()), bytes);
    assert_eq!(fake.files_in_folder(), 1);

    s.put(&key(1), b"second").unwrap();
    assert_eq!(read(s.open(&key(1)).unwrap().unwrap()), b"second");
    assert_eq!(
        fake.files_in_folder(),
        1,
        "an existing blob is updated, not duplicated"
    );

    let thumb = format!("{}.w320", key(1));
    s.put(&thumb, b"t").unwrap();
    assert_eq!(read(s.open(&thumb).unwrap().unwrap()), b"t");
    assert_eq!(fake.files_in_folder(), 2);
}

#[test]
fn unknown_keys_and_deleting() {
    let fake = Fake::start();
    let s = fake.store();
    assert!(s.open(&key(9)).unwrap().is_none());
    assert!(!s.delete(&key(9)).unwrap());
    s.put(&key(2), b"x").unwrap();
    assert!(s.delete(&key(2)).unwrap());
    assert!(s.open(&key(2)).unwrap().is_none());
    assert_eq!(fake.files_in_folder(), 0);
    assert!(!s.delete(&key(2)).unwrap());
}

#[test]
fn a_bad_key_is_refused_before_anything_is_sent() {
    let fake = Fake::start();
    let s = fake.store();
    for bad in [
        "",
        "../x",
        "abc",
        &format!("{}/x", key(1)),
        &key(1).to_uppercase(),
    ] {
        assert!(s.put(bad, b"x").is_err(), "{bad}");
        assert!(s.open(bad).is_err(), "{bad}");
        assert!(s.delete(bad).is_err(), "{bad}");
    }
    assert_eq!(fake.st.lock().unwrap().api_calls, 0);
}

#[test]
fn keys_pages_through_the_folder_and_skips_what_is_not_a_blob() {
    let fake = Fake::start();
    fake.st.lock().unwrap().page_size = 2;
    let s = fake.store();
    let mut want: Vec<String> = (0..5).map(key).collect();
    for k in &want {
        s.put(k, b"x").unwrap();
    }
    {
        let mut st = fake.st.lock().unwrap();
        for (name, parent) in [("notes.txt", FOLDER), (&key(77), "elsewhere")] {
            st.files.push(File {
                id: format!("foreign-{name}"),
                name: name.to_string(),
                parent: parent.into(),
                mime: "text/plain".into(),
                data: vec![],
            });
        }
        st.list_calls = 0;
    }
    want.sort();
    assert_eq!(s.keys().unwrap(), want);
    // Five blobs and notes.txt are six files in the folder: three pages of two.
    assert_eq!(fake.st.lock().unwrap().list_calls, 3);
}

#[test]
fn the_access_token_is_reused_and_refreshed_after_a_401() {
    let fake = Fake::start();
    let s = fake.store();
    s.put(&key(1), b"a").unwrap();
    s.put(&key(2), b"b").unwrap();
    s.open(&key(1)).unwrap();
    assert_eq!(
        fake.st.lock().unwrap().token_calls,
        1,
        "one token for many calls"
    );

    fake.st.lock().unwrap().valid_token = "revoked".into();
    assert_eq!(read(s.open(&key(2)).unwrap().unwrap()), b"b");
    assert_eq!(
        fake.st.lock().unwrap().token_calls,
        2,
        "a 401 mints a new token once"
    );
}

#[test]
fn transient_failures_are_retried_and_the_retries_are_bounded() {
    let fake = Fake::start();
    let s = fake.store();
    fake.fail_next(503, "{}");
    fake.fail_next(429, "{}");
    fake.fail_next(
        403,
        r#"{"error":{"message":"Rate Limit Exceeded","errors":[{"reason":"userRateLimitExceeded"}]}}"#,
    );
    s.put(&key(1), b"x").unwrap();
    assert_eq!(fake.files_in_folder(), 1);

    for _ in 0..10 {
        fake.fail_next(500, "{}");
    }
    let before = fake.st.lock().unwrap().api_calls;
    let e = s.open(&key(1)).err().unwrap();
    assert!(e.contains("gave up after 4 attempts"), "{e}");
    assert_eq!(fake.st.lock().unwrap().api_calls - before, 4);
}

#[test]
fn a_refusal_is_not_retried_and_carries_drives_message() {
    let fake = Fake::start();
    let s = fake.store();
    fake.fail_next(
        403,
        r#"{"error":{"code":403,"message":"The user does not have sufficient permissions"}}"#,
    );
    let before = fake.st.lock().unwrap().api_calls;
    let e = s.put(&key(1), b"x").unwrap_err();
    assert!(
        e.contains("HTTP 403") && e.contains("sufficient permissions"),
        "{e}"
    );
    assert_eq!(fake.st.lock().unwrap().api_calls - before, 1);
}

#[test]
fn credentials_never_reach_a_url_an_error_or_the_debug_output() {
    let fake = Fake::start();
    let s = fake.store();
    s.put(&key(1), b"x").unwrap();
    let secrets = [CLIENT_SECRET, REFRESH, "at-1"];
    for p in &fake.st.lock().unwrap().paths {
        assert!(secrets.iter().all(|x| !p.contains(x)), "{p}");
    }
    let dbg = format!("{:?}", fake.config());
    assert!(
        secrets.iter().all(|x| !dbg.contains(x)) && !dbg.contains(CLIENT_ID),
        "{dbg}"
    );

    let mut bad = fake.config();
    bad.refresh_token = "wrong-token".into();
    let e = DriveBytes::new(bad).unwrap().keys().unwrap_err();
    assert!(e.contains("invalid_grant"), "{e}");
    assert!(
        ["wrong-token", CLIENT_SECRET, REFRESH, "Bad refresh"]
            .iter()
            .all(|x| !e.contains(x)),
        "{e}"
    );
}

#[test]
fn a_file_removed_in_drive_behind_the_stores_back_is_handled() {
    let fake = Fake::start();
    let s = fake.store();
    s.put(&key(1), b"old").unwrap();
    fake.st.lock().unwrap().files.retain(|f| f.parent != FOLDER);
    assert!(
        s.open(&key(1)).unwrap().is_none(),
        "a stale cached id is dropped"
    );

    s.put(&key(2), b"old").unwrap();
    fake.st.lock().unwrap().files.retain(|f| f.parent != FOLDER);
    s.put(&key(2), b"new").unwrap();
    assert_eq!(read(s.open(&key(2)).unwrap().unwrap()), b"new");
}

#[test]
fn probe_and_create_folder() {
    let fake = Fake::start();
    let s = fake.store();
    assert_eq!(s.probe().unwrap(), "obop files");

    let id = s.create_folder("SpaceCorps files", None).unwrap();
    let mut cfg = fake.config();
    cfg.folder_id = id;
    assert_eq!(
        DriveBytes::new(cfg).unwrap().probe().unwrap(),
        "SpaceCorps files"
    );

    let mut cfg = fake.config();
    cfg.folder_id = "nope".into();
    let e = DriveBytes::new(cfg).unwrap().probe().unwrap_err();
    assert!(e.contains("drive-folder"), "{e}");
    assert!(s.create_folder("two\nlines", None).is_err());
}

#[test]
fn the_setting_is_validated_and_the_two_places_cannot_both_be_set() {
    assert!(open_with("memory:", None, None).is_ok());
    assert!(
        open_with("memory:", Some(" "), Some(" ")).is_ok(),
        "blank settings are unset"
    );
    let e = open_with("memory:", Some("/tmp/x"), Some("gdrive://abc"))
        .err()
        .unwrap();
    assert!(e.contains("both set"), "{e}");
    let e = open_with("memory:", None, Some("s3://bucket"))
        .err()
        .unwrap();
    assert!(e.contains("gdrive://"), "{e}");
    let e = open_with("memory:", None, Some("gdrive://bad/id"))
        .err()
        .unwrap();
    assert!(e.contains("folder id"), "{e}");
    let e = open_with("memory:", None, Some("gdrive://")).err().unwrap();
    assert!(e.contains("folder id"), "{e}");
}

#[test]
fn the_catalogue_backup_and_restore_work_over_drive_unchanged() {
    let fake = Fake::start();
    let drive: Arc<dyn BlobBytes> = Arc::new(fake.store());
    let cat = Catalog::new(obop_storage::open("memory:").unwrap(), drive);
    let info = cat
        .put(
            b"the picture",
            NewBlob {
                content_type: "image/png".into(),
                uploader: "ada".into(),
                filename: "a.png".into(),
                ..NewBlob::default()
            },
        )
        .unwrap();
    assert_eq!(
        cat.read_all(&info.id, 1000).unwrap().unwrap(),
        b"the picture"
    );

    // Backup reads every blob through the catalogue; restore writes it back through it.
    let exported: Vec<_> = cat.export().unwrap().map(|b| b.unwrap()).collect();
    assert_eq!(exported.len(), 1);
    let other = Fake::start();
    let restored = Catalog::new(
        obop_storage::open("memory:").unwrap(),
        Arc::new(other.store()),
    );
    restored.import(&exported[0]).unwrap();
    assert_eq!(
        restored.read_all(&info.id, 1000).unwrap().unwrap(),
        b"the picture"
    );
    assert_eq!(other.files_in_folder(), 1);
}

// ---- the authorisation helper -------------------------------------------------------------

fn browser(link: &str, extra: &str) {
    let u = url::Url::parse(link).unwrap();
    let get = |k: &str| {
        u.query_pairs()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.into_owned())
            .unwrap()
    };
    let redirect = get("redirect_uri");
    let state = get("state");
    let target = format!("{redirect}/?{}", extra.replace("STATE", &state));
    std::thread::spawn(move || {
        // A favicon request first: it must not end the wait.
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into();
        let _ = agent.get(format!("{redirect}/favicon.ico")).call();
        let _ = agent.get(&target).call();
    });
}

fn opts(fake: &Fake) -> AuthorizeOptions {
    let mut o = AuthorizeOptions::new(CLIENT_ID.into(), CLIENT_SECRET.into(), "drive.file");
    o.token_url = format!("{}/token", fake.base);
    o.wait = Duration::from_secs(10);
    o
}

#[test]
fn authorize_runs_the_pkce_flow_and_returns_only_the_refresh_token() {
    let fake = Fake::start();
    let mut link = String::new();
    let token = authorize(&opts(&fake), &mut |l| {
        link = l.to_string();
        browser(l, "code=the-code&state=STATE");
    })
    .unwrap();
    assert_eq!(token, "rt-new");

    let u = url::Url::parse(&link).unwrap();
    let get = |k: &str| {
        u.query_pairs()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.into_owned())
            .unwrap()
    };
    assert_eq!(get("scope"), "https://www.googleapis.com/auth/drive.file");
    assert_eq!(get("access_type"), "offline");
    assert_eq!(get("code_challenge_method"), "S256");
    assert!(get("redirect_uri").starts_with("http://127.0.0.1:"));
    let form = fake.st.lock().unwrap().last_code_form.clone();
    let verifier = &form.iter().find(|(k, _)| k == "code_verifier").unwrap().1;
    let want = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    assert_eq!(
        get("code_challenge"),
        want,
        "the verifier answers the challenge"
    );
}

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

#[test]
fn authorize_refuses_a_wrong_state_a_denial_a_bad_scope_and_a_silent_browser() {
    let fake = Fake::start();
    let e = authorize(&opts(&fake), &mut |l| {
        browser(l, "code=the-code&state=forged")
    })
    .unwrap_err();
    assert!(e.contains("different state"), "{e}");

    let e = authorize(&opts(&fake), &mut |l| {
        browser(l, "error=access_denied&state=STATE")
    })
    .unwrap_err();
    assert!(e.contains("access_denied"), "{e}");

    let e = authorize(&opts(&fake), &mut |l| browser(l, "code=wrong&state=STATE")).unwrap_err();
    assert!(e.contains("invalid_grant"), "{e}");

    let mut o = opts(&fake);
    o.scope = "drive.readonly".into();
    assert!(authorize(&o, &mut |_| {}).is_err());

    let mut o = opts(&fake);
    o.wait = Duration::from_millis(300);
    let e = authorize(&o, &mut |_| {}).unwrap_err();
    assert!(e.contains("no answer"), "{e}");
}
