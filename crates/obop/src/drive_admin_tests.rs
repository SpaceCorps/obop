//! The Google Drive page of the back office (the `google_drive` provider) against a fake Google on
//! a loopback port: client id and secret under the provider policy, the authorization link, the
//! callback with PKCE, sealed tokens, the Drive browser reading its credentials from the database,
//! the browse group, and disconnecting. No call goes to Google.

use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use obop_auth::Auth;
use obop_commands::{Dispatcher, Store};
use obop_connectors::catalog::providers;
use obop_connectors::drive::{Endpoints, use_endpoints};
use obop_connectors::{Config, Runtime, SecretBox};
use obop_media::fixtures as fx;
use serde_json::{Value as J, json};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex, OnceLock};
use tower::ServiceExt;

use crate::drive_source::Source;

const CLIENT_ID: &str = "1234-abc.apps.googleusercontent.com";
const CLIENT_SECRET: &str = "GOCSPX-client-secret-for-the-test";
const REFRESH: &str = "1//refresh-token-for-the-test";
const PW: &str = "a long enough password";

/// What the fake Google remembers and how it misbehaves on request.
#[derive(Default)]
struct Google {
    /// code -> the PKCE challenge it was issued for
    codes: HashMap<String, String>,
    token_calls: Vec<String>,
    about_status: u16,
    omit_refresh_token: bool,
    /// The `scope` the token endpoint reports as granted; none says nothing (then the requested
    /// scopes count as granted).
    scope_response: Option<String>,
    /// The query of every files.list call.
    lists: Vec<String>,
    /// (route, query, body) of every POST to Drive.
    posts: Vec<(String, String, String)>,
}

struct Fake {
    state: Mutex<Google>,
}

/// One fake for every test of this file: the Google addresses are process-wide.
fn fake() -> &'static Arc<Fake> {
    static FAKE: OnceLock<Arc<Fake>> = OnceLock::new();
    FAKE.get_or_init(|| {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let fake = Arc::new(Fake {
            state: Mutex::new(Google {
                about_status: 200,
                ..Google::default()
            }),
        });
        let f = fake.clone();
        std::thread::spawn(move || {
            for s in l.incoming() {
                let Ok(s) = s else { break };
                let f = f.clone();
                std::thread::spawn(move || serve(&f, s));
            }
        });
        let base = format!("http://127.0.0.1:{port}");
        use_endpoints(Some(Endpoints {
            auth_url: format!("{base}/authorize"),
            token_url: format!("{base}/token"),
            api_base: base,
        }))
        .unwrap();
        fake
    })
}

fn read_request(s: &mut std::net::TcpStream) -> (String, String, String, String) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = s.read(&mut chunk).unwrap_or(0);
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        let text = String::from_utf8_lossy(&buf).to_string();
        if let Some((head, body)) = text.split_once("\r\n\r\n") {
            let want = head
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                })
                .unwrap_or(0);
            if body.len() >= want {
                break;
            }
        }
    }
    let text = String::from_utf8_lossy(&buf).to_string();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let mut lines = head.lines();
    let first = lines.next().unwrap_or("");
    let mut parts = first.split(' ');
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    let auth = lines
        .find(|l| l.to_ascii_lowercase().starts_with("authorization:"))
        .map(|l| l.split_once(':').unwrap().1.trim().to_string())
        .unwrap_or_default();
    (method, path, auth, body.to_string())
}

fn reply(s: &mut std::net::TcpStream, status: &str, ctype: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status}\r\ncontent-type: {ctype}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    );
    let _ = s.write_all(head.as_bytes());
    let _ = s.write_all(body);
}

fn form(body: &str) -> HashMap<String, String> {
    url::form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect()
}

fn serve(f: &Fake, mut s: std::net::TcpStream) {
    let (method, path, auth, body) = read_request(&mut s);
    let png = fx::png(8, 8, 3);
    if path == "/token" {
        let p = form(&body);
        let mut g = f.state.lock().unwrap();
        g.token_calls.push(p["grant_type"].clone());
        let ok = p["client_id"] == CLIENT_ID && p["client_secret"] == CLIENT_SECRET;
        let json = match p["grant_type"].as_str() {
            "authorization_code"
                if ok
                    && g.codes.get(&p["code"]).is_some_and(|c| {
                        *c == obop_connectors::oauth::challenge_of(&p["code_verifier"])
                    }) =>
            {
                let rt = if g.omit_refresh_token {
                    String::new()
                } else {
                    format!(r#","refresh_token":"{REFRESH}""#)
                };
                let sc = g
                    .scope_response
                    .as_ref()
                    .map(|v| format!(r#","scope":"{v}""#))
                    .unwrap_or_default();
                format!(r#"{{"access_token":"at-1","expires_in":3600{rt}{sc}}}"#)
            }
            "refresh_token" if ok && p["refresh_token"] == REFRESH => {
                format!(
                    r#"{{"access_token":"at-{}","expires_in":3600}}"#,
                    g.token_calls.len()
                )
            }
            _ => {
                reply(
                    &mut s,
                    "400 Bad Request",
                    "application/json",
                    br#"{"error":"invalid_grant"}"#,
                );
                return;
            }
        };
        reply(&mut s, "200 OK", "application/json", json.as_bytes());
        return;
    }
    if !auth.starts_with("Bearer at-") {
        reply(&mut s, "401 Unauthorized", "application/json", b"{}");
        return;
    }
    let (route, query) = path.split_once('?').unwrap_or((&path, ""));
    let q = form(query);
    if method == "POST" {
        f.state
            .lock()
            .unwrap()
            .posts
            .push((route.into(), query.into(), body.clone()));
        let (name, mime) = if route.starts_with("/upload/") {
            let meta = body.lines().find(|l| l.starts_with('{')).unwrap_or("{}");
            let m: J = serde_json::from_str(meta).unwrap_or_default();
            (
                m["name"].as_str().unwrap_or("").to_string(),
                "application/pdf".to_string(),
            )
        } else {
            let m: J = serde_json::from_str(&body).unwrap_or_default();
            (
                m["name"].as_str().unwrap_or("").to_string(),
                m["mimeType"].as_str().unwrap_or("").to_string(),
            )
        };
        let n = f.state.lock().unwrap().posts.len();
        let out = json!({"id": format!("new-{n}"), "name": name, "mimeType": mime,
                         "webViewLink": format!("https://drive.google.com/file/d/new-{n}/view")});
        reply(
            &mut s,
            "200 OK",
            "application/json",
            out.to_string().as_bytes(),
        );
        return;
    }
    match route {
        "/drive/v3/drives" => reply(
            &mut s,
            "200 OK",
            "application/json",
            br#"{"drives":[{"id":"sd1","name":"SpaceCorps shared"}]}"#,
        ),
        "/drive/v3/files/sdf1" => reply(
            &mut s,
            "200 OK",
            "application/json",
            br#"{"id":"sdf1","name":"Legal","mimeType":"application/vnd.google-apps.folder","driveId":"sd1","ownedByMe":false}"#,
        ),
        "/drive/v3/files/sdf2" if q.get("alt").map(String::as_str) == Some("media") => {
            reply(&mut s, "200 OK", "image/png", &png);
        }
        "/drive/v3/files/sdf2" => {
            let body = json!({
                "id": "sdf2", "name": "Shared logo.png", "mimeType": "image/png",
                "modifiedTime": "2026-10-30T09:00:00Z", "size": png.len().to_string(),
                "webViewLink": "https://drive.google.com/file/d/sdf2/view", "driveId": "sd1"
            });
            reply(&mut s, "200 OK", "application/json", body.to_string().as_bytes());
        }
        "/drive/v3/about" => {
            let st = f.state.lock().unwrap().about_status;
            if st == 200 {
                reply(
                    &mut s,
                    "200 OK",
                    "application/json",
                    br#"{"user":{"emailAddress":"ada@example.com"}}"#,
                );
            } else {
                reply(
                    &mut s,
                    &format!("{st} Refused"),
                    "application/json",
                    br#"{"error":{"message":"Drive API has not been used"}}"#,
                );
            }
        }
        "/drive/v3/files" if q.contains_key("driveId") => {
            f.state.lock().unwrap().lists.push(query.to_string());
            let body = json!({"files": [{
                "id": "sdf1", "name": "Legal", "mimeType": "application/vnd.google-apps.folder",
                "modifiedTime": "2026-10-30T09:00:00Z", "webViewLink": "https://drive.google.com/drive/folders/sdf1"
            }]});
            reply(&mut s, "200 OK", "application/json", body.to_string().as_bytes());
        }
        "/drive/v3/files" => {
            let body = json!({"files": [{
                "id": "f1", "name": "NDA - Acme Oy", "mimeType": "image/png",
                "modifiedTime": "2026-10-30T09:00:00Z", "size": png.len().to_string(),
                "webViewLink": "https://drive.google.com/file/d/f1/view"
            }]});
            reply(
                &mut s,
                "200 OK",
                "application/json",
                body.to_string().as_bytes(),
            );
        }
        "/drive/v3/files/f1" if q.get("alt").map(String::as_str) == Some("media") => {
            reply(&mut s, "200 OK", "image/png", &png);
        }
        "/drive/v3/files/f1" => {
            let body = json!({
                "id": "f1", "name": "Logo.png", "mimeType": "image/png",
                "modifiedTime": "2026-10-30T09:00:00Z", "size": png.len().to_string(),
                "webViewLink": "https://drive.google.com/file/d/f1/view"
            });
            reply(
                &mut s,
                "200 OK",
                "application/json",
                body.to_string().as_bytes(),
            );
        }
        _ => reply(&mut s, "404 Not Found", "application/json", b"{}"),
    }
}

struct Rig {
    router: axum::Router,
    store: Store,
    admin: String,
    member: String,
    outsider: String,
}

async fn call(r: &axum::Router, req: Request<Body>) -> (u16, String) {
    let resp = r.clone().oneshot(req).await.unwrap();
    let status = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

fn as_json(s: &str) -> J {
    serde_json::from_str(s).unwrap_or(J::Null)
}

impl Rig {
    async fn post(&self, token: Option<&str>, path: &str, body: J) -> (u16, J) {
        let mut b = Request::post(path).header("content-type", "application/json");
        if let Some(t) = token {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        let (s, t) = call(&self.router, b.body(Body::from(body.to_string())).unwrap()).await;
        (s, as_json(&t))
    }

    async fn get(&self, token: Option<&str>, path: &str) -> (u16, J) {
        let mut b = Request::get(path);
        if let Some(t) = token {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        let (s, t) = call(&self.router, b.body(Body::empty()).unwrap()).await;
        (s, as_json(&t))
    }

    /// A command that must succeed; its `value`.
    async fn ok(&self, cmd: &str, body: J) -> J {
        let (m, c) = cmd.split_once('.').unwrap();
        let (s, v) = self
            .post(Some(&self.admin), &format!("/api/{m}/{c}"), body)
            .await;
        assert_eq!(s, 200, "{cmd}: {v}");
        v["value"].clone()
    }

    async fn status(&self, token: &str) -> J {
        let (s, v) = self.get(Some(token), "/api/drive/status").await;
        assert_eq!(s, 200, "{v}");
        v
    }

    async fn callback(&self, state: &str, code: &str) -> String {
        let r = Request::get(format!("/oauth/callback?state={state}&code={code}"))
            .body(Body::empty())
            .unwrap();
        call(&self.router, r).await.1
    }

    /// The administrator's steps up to the Google link: client entered, connection created,
    /// authorization started. Returns the query of the link.
    async fn start(&self, allowed_group: &str) -> (String, HashMap<String, String>) {
        let pol = self
            .ok(
                "provider_policy.create",
                json!({"provider": "google_drive", "enabled": true,
                       "oauth_client_id": CLIENT_ID, "oauth_client_secret": CLIENT_SECRET,
                       "allowed_group": allowed_group}),
            )
            .await;
        let _ = pol;
        let c = self
            .ok(
                "connection.create",
                json!({"name": "Google Drive", "provider": "google_drive", "owner_scope": "instance"}),
            )
            .await;
        let id = c["id"].as_str().unwrap().to_string();
        let start = self.ok("connection.oauth_start", json!({"id": id})).await;
        let link = as_json(start["result"].as_str().unwrap())["url"]
            .as_str()
            .unwrap()
            .to_string();
        let q = url::Url::parse(&link)
            .unwrap()
            .query_pairs()
            .into_owned()
            .collect();
        (id, q)
    }

    fn events(&self, kind: &str) -> usize {
        self.store
            .backend()
            .list("security_event")
            .unwrap()
            .iter()
            .filter(|(_, e)| e["kind"] == kind)
            .count()
    }
}

async fn rig() -> Rig {
    let f = fake();
    {
        let mut g = f.state.lock().unwrap();
        *g = Google {
            about_status: 200,
            ..Google::default()
        };
    }
    let store = Store::new();
    let d = Arc::new(Dispatcher::new());
    let ring = obop_keys::KeyRing::for_backend(&store.backend()).unwrap();
    d.set_shared(store.backend(), ring);
    base::register(&d, &store);
    product::register(&d, &store);
    let rt = Runtime::with_providers(
        store.backend(),
        d.clone(),
        SecretBox::for_tests(),
        Config {
            allow_loopback: true,
            public_url: Some("http://127.0.0.1:8080".into()),
        },
        providers(),
        None,
    );
    connectors::register(&d, &store, &rt);
    base::seed(&store);
    let auth = Arc::new(Auth::with_test_params(store.backend()));
    auth.create_user("admin", PW, &["platform.admin".into()])
        .unwrap();
    auth.create_user("member", PW, &["product.manager".into()])
        .unwrap();
    auth.create_user("outsider", PW, &["crm.user".into()])
        .unwrap();
    let state = obop_http::HttpState::new(d, auth);
    state.set_connectors(rt.clone());
    state.set_drive(Arc::new(Source::new(rt, None)));
    let router = obop_http::router(state, None);
    let login = |u: &'static str| {
        let r = router.clone();
        async move {
            let b = Request::post("/api/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"username": u, "password": PW}).to_string(),
                ))
                .unwrap();
            let (s, t) = call(&r, b).await;
            assert_eq!(s, 200, "{t}");
            as_json(&t)["token"].as_str().unwrap().to_string()
        }
    };
    let (admin, member, outsider) = (
        login("admin").await,
        login("member").await,
        login("outsider").await,
    );
    Rig {
        router,
        store,
        admin,
        member,
        outsider,
    }
}

/// Every stored record and every response must be free of these.
fn assert_no_plaintext(r: &Rig, secrets: &[&str]) {
    for model in [
        "provider_policy",
        "connection",
        "connector_oauth_state",
        "security_event",
    ] {
        for (_, f) in r.store.backend().list(model).unwrap_or_default() {
            let text = serde_json::to_string(&f).unwrap();
            for s in secrets {
                assert!(!text.contains(s), "{model} holds {s}");
            }
        }
    }
}

/// The scenarios share one fake Google (its addresses are process-wide), so they run one after
/// the other in one test.
#[tokio::test]
async fn the_google_drive_page_end_to_end_against_a_fake_google() {
    an_administrator_connects_a_drive_from_the_back_office_and_the_browser_uses_it().await;
    a_drive_that_refuses_the_account_is_an_error_with_a_reason_and_browses_nothing().await;
    a_sign_in_without_a_refresh_token_is_refused_with_the_way_out().await;
    a_workspace_drive_scopes_browsing_and_lets_the_write_group_create_files().await;
    a_sign_in_that_did_not_grant_writing_cannot_create_and_says_how_to_fix_it().await;
    // A pin: `cargo test --locked -p obop -- pins the_google_drive_page_end_to_end` selects this test.
    pins_the_return_page_after_a_drive_sign_in().await;
    the_google_addresses_cannot_be_pointed_anywhere_but_loopback();
}

async fn an_administrator_connects_a_drive_from_the_back_office_and_the_browser_uses_it() {
    let r = rig().await;
    let g = fake();

    // Nothing is set: an administrator is told so and where to register the redirect address;
    // a member is not shown the settings.
    let v = r.status(&r.admin).await;
    assert_eq!(
        (v["connected"].clone(), v["may_configure"].clone()),
        (json!(false), json!(true))
    );
    assert_eq!(v["settings"]["state"], "not_configured");
    assert_eq!(
        v["settings"]["redirect_uri"],
        "http://127.0.0.1:8080/oauth/callback"
    );
    assert_eq!(v["settings"]["source"], "admin");
    let v = r.status(&r.member).await;
    assert_eq!(v["may_configure"], false);
    assert!(v.get("settings").is_none());

    // Only an administrator of connectors may configure.
    let (s, _) = r
        .post(
            Some(&r.member),
            "/api/provider_policy/create",
            json!({"provider": "google_drive", "enabled": true, "oauth_client_id": "x"}),
        )
        .await;
    assert!(matches!(s, 403 | 404), "refused for a member, got {s}");
    assert!(
        r.store
            .backend()
            .list("provider_policy")
            .unwrap()
            .is_empty()
    );

    // The client is entered and the authorization started; the secret never comes back.
    let (id, q) = r.start("product.manager").await;
    assert_eq!(q["client_id"], CLIENT_ID);
    assert_eq!(
        q["scope"],
        "https://www.googleapis.com/auth/drive.readonly https://www.googleapis.com/auth/drive.file"
    );
    assert_eq!(q["access_type"], "offline");
    assert_eq!(q["prompt"], "consent");
    assert_eq!(q["code_challenge_method"], "S256");
    assert_eq!(q["redirect_uri"], "http://127.0.0.1:8080/oauth/callback");
    let stored = r.store.backend().list("provider_policy").unwrap();
    assert!(
        stored[0].1["oauth_client_secret"]
            .as_str()
            .unwrap()
            .starts_with("e2.")
    );
    let v = r.status(&r.admin).await;
    assert_eq!(v["settings"]["state"], "needs_consent");
    assert_eq!(v["settings"]["client_id"], CLIENT_ID);
    assert_eq!(v["settings"]["secret_set"], true);
    assert!(!v.to_string().contains(CLIENT_SECRET));
    assert_eq!(v["connected"], false, "not signed in yet");
    assert_eq!(r.get(Some(&r.admin), "/api/drive/files").await.0, 404);

    // Google sends the person back with a code bound to the challenge. A guessed state is refused,
    // the real one works once.
    g.state
        .lock()
        .unwrap()
        .codes
        .insert("code-1".into(), q["code_challenge"].clone());
    let bad = r.callback("guess", "code-1").await;
    assert!(bad.contains("Not connected") && !bad.contains("guess"));
    let good = r.callback(&q["state"], "code-1").await;
    assert!(good.contains("Connected"), "{good}");
    assert!(
        good.contains("href=\"/#/connectors/google_drive\""),
        "{good}"
    );
    assert!(
        r.callback(&q["state"], "code-1")
            .await
            .contains("Not connected")
    );

    // Connected as the Google account; the tokens are sealed, the audit trail has the event.
    let v = r.status(&r.admin).await;
    assert_eq!(v["settings"]["state"], "connected");
    assert_eq!(v["settings"]["account"], "ada@example.com");
    assert_eq!(v["connected"], true);
    assert_eq!(v["settings"]["connection"], id);
    let conn = r.store.backend().get("connection", &id).unwrap().unwrap();
    assert!(conn["oauth_tokens"].as_str().unwrap().starts_with("e2."));
    assert_eq!(conn["state"], "connected");
    assert_eq!(r.events("drive_connected"), 1);

    // The browser reads its credentials from the database: the refresh token mints one access
    // token that the next requests reuse. The browse group is the one the administrator chose.
    let (s, v) = r.get(Some(&r.admin), "/api/drive/files").await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["files"][1]["name"], "NDA - Acme Oy");
    assert_eq!(r.get(Some(&r.member), "/api/drive/files").await.0, 200);
    assert_eq!(r.get(Some(&r.outsider), "/api/drive/files").await.0, 403);
    let refreshes = g
        .state
        .lock()
        .unwrap()
        .token_calls
        .iter()
        .filter(|c| *c == "refresh_token")
        .count();
    assert_eq!(refreshes, 1, "one access token, reused");
    let (s, v) = r
        .post(
            Some(&r.admin),
            "/api/drive/import",
            json!({"id": "f1", "field": "product.image"}),
        )
        .await;
    assert_eq!(s, 201, "{v}");
    assert_eq!(v["filename"], "Logo.png");
    assert_eq!(r.events("drive_import"), 1);

    // Back to the default group: the member loses access.
    let pid = r.store.backend().list("provider_policy").unwrap()[0]
        .0
        .clone();
    r.ok(
        "provider_policy.update",
        json!({"id": pid, "allowed_group": ""}),
    )
    .await;
    assert_eq!(r.get(Some(&r.member), "/api/drive/files").await.0, 403);
    assert_eq!(
        r.status(&r.admin).await["settings"]["browse_group"],
        "platform.admin"
    );

    // Disconnecting purges the tokens; the client stays so reconnecting needs no new entry.
    r.ok("connection.revoke", json!({"id": id})).await;
    let conn = r.store.backend().get("connection", &id).unwrap().unwrap();
    assert_eq!(conn["oauth_tokens"], "");
    assert_eq!(conn["state"], "revoked");
    assert_eq!(r.events("drive_disconnected"), 1);
    let v = r.status(&r.admin).await;
    assert_eq!(v["settings"]["state"], "needs_consent");
    assert_eq!(v["settings"]["secret_set"], true);
    assert_eq!(v["connected"], false);
    assert_eq!(r.get(Some(&r.admin), "/api/drive/files").await.0, 404);
    assert_no_plaintext(&r, &[CLIENT_SECRET, REFRESH, "at-1"]);
}

async fn a_drive_that_refuses_the_account_is_an_error_with_a_reason_and_browses_nothing() {
    let r = rig().await;
    let g = fake();
    g.state.lock().unwrap().about_status = 403;
    let (id, q) = r.start("").await;
    g.state
        .lock()
        .unwrap()
        .codes
        .insert("code-2".into(), q["code_challenge"].clone());
    let page = r.callback(&q["state"], "code-2").await;
    assert!(page.contains("Not connected"), "{page}");
    let v = r.status(&r.admin).await;
    assert_eq!(v["settings"]["state"], "error");
    assert!(
        v["settings"]["last_error"]
            .as_str()
            .unwrap()
            .contains("Drive API"),
        "{v}"
    );
    assert_eq!(v["connected"], false);
    assert_eq!(r.get(Some(&r.admin), "/api/drive/files").await.0, 404);
    assert_eq!(r.events("drive_connect_failed"), 1);
    let _ = id;
}

async fn a_sign_in_without_a_refresh_token_is_refused_with_the_way_out() {
    let r = rig().await;
    let g = fake();
    g.state.lock().unwrap().omit_refresh_token = true;
    let (_, q) = r.start("").await;
    g.state
        .lock()
        .unwrap()
        .codes
        .insert("code-3".into(), q["code_challenge"].clone());
    r.callback(&q["state"], "code-3").await;
    let v = r.status(&r.admin).await;
    assert_eq!(v["settings"]["state"], "error");
    assert!(
        v["settings"]["last_error"]
            .as_str()
            .unwrap()
            .contains("refresh token")
    );
    assert_eq!(v["connected"], false);
}

impl Rig {
    /// Connect with the given token response scope (`None`: as requested).
    async fn connect(&self, scope: Option<&str>, code: &str) -> String {
        let g = fake();
        g.state.lock().unwrap().scope_response = scope.map(String::from);
        let (id, q) = self.start("").await;
        g.state
            .lock()
            .unwrap()
            .codes
            .insert(code.into(), q["code_challenge"].clone());
        let page = self.callback(&q["state"], code).await;
        assert!(page.contains("Connected"), "{page}");
        id
    }

    async fn policy(&self, body: J) -> (u16, J) {
        let pid = self.store.backend().list("provider_policy").unwrap()[0]
            .0
            .clone();
        let mut b = body;
        b["id"] = json!(pid);
        self.post(Some(&self.admin), "/api/provider_policy/update", b)
            .await
    }

    async fn upload(&self, token: &str, query: &str, ctype: &str, bytes: &[u8]) -> (u16, J) {
        let req = Request::post(format!("/api/drive/upload{query}"))
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", ctype)
            .body(Body::from(bytes.to_vec()))
            .unwrap();
        let (s, t) = call(&self.router, req).await;
        (s, as_json(&t))
    }
}

async fn a_workspace_drive_scopes_browsing_and_lets_the_write_group_create_files() {
    let r = rig().await;
    let g = fake();
    {
        let mut st = g.state.lock().unwrap();
        st.lists.clear();
        st.posts.clear();
    }
    r.connect(None, "code-w1").await;

    // Connected with the write scope, no workspace yet: nothing to create in.
    let v = r.status(&r.admin).await;
    assert_eq!(v["settings"]["write_granted"], true);
    assert_eq!(v["settings"]["workspace_id"], "");
    assert_eq!(v["workspace"], J::Null);
    assert_eq!(v["may_write"], false);
    let (s, v) = r
        .post(Some(&r.admin), "/api/drive/folders", json!({"name": "x"}))
        .await;
    assert_eq!(s, 409, "{v}");
    assert!(v["error"].as_str().unwrap().contains("workspace drive"));

    // The picker lists the shared drives, for administrators of connectors only.
    let (s, v) = r.get(Some(&r.admin), "/api/drive/drives").await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(
        v["drives"],
        json!([{"id": "sd1", "name": "SpaceCorps shared"}])
    );
    assert_eq!(r.get(Some(&r.member), "/api/drive/drives").await.0, 403);

    // A hostile id or name is refused when it is saved.
    let (s, _) = r.policy(json!({"workspace_id": "a/../b"})).await;
    assert_ne!(s, 200);
    let (s, _) = r.policy(json!({"write_group": "Not A Group"})).await;
    assert_ne!(s, 200);

    // Choose the workspace.
    let (s, v) = r
        .policy(json!({"workspace_id": "sd1", "workspace_name": "SpaceCorps shared"}))
        .await;
    assert_eq!(s, 200, "{v}");
    let v = r.status(&r.admin).await;
    assert_eq!(
        v["workspace"],
        json!({"id": "sd1", "name": "SpaceCorps shared"})
    );
    assert_eq!(v["may_write"], true);
    assert_eq!(v["settings"]["write_group"], "platform.admin");

    // Browsing is scoped to that drive: the listing asks for corpora=drive and driveId.
    let (s, v) = r.get(Some(&r.admin), "/api/drive/files").await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["files"][0]["name"], "Legal");
    let last = g.state.lock().unwrap().lists.last().cloned().unwrap();
    assert!(
        last.contains("corpora=drive") && last.contains("driveId=sd1"),
        "{last}"
    );

    // Import: a file of the workspace comes in; one outside it is refused and logged.
    let (s, v) = r
        .post(
            Some(&r.admin),
            "/api/drive/import",
            json!({"id": "sdf2", "field": "product.image"}),
        )
        .await;
    assert_eq!(s, 201, "{v}");
    let (s, v) = r
        .post(
            Some(&r.admin),
            "/api/drive/import",
            json!({"id": "f1", "field": "product.image"}),
        )
        .await;
    assert_eq!(s, 502, "{v}");
    assert!(
        v["error"]
            .as_str()
            .unwrap()
            .contains("outside the workspace drive")
    );
    assert_eq!(r.events("drive_import"), 1);
    assert_eq!(r.events("drive_import_failed"), 1);

    // Create a folder at the top, a Doc in a workspace folder and an upload; each is audited and
    // carries the all-drives flag and the right parent.
    let (s, v) = r
        .post(
            Some(&r.admin),
            "/api/drive/folders",
            json!({"name": "Contracts"}),
        )
        .await;
    assert_eq!(s, 201, "{v}");
    assert_eq!(v["folder"], true);
    let (s, v) = r
        .post(
            Some(&r.admin),
            "/api/drive/docs",
            json!({"name": "NDA Abhinav", "parent": "sdf1"}),
        )
        .await;
    assert_eq!(s, 201, "{v}");
    assert!(
        v["web_link"]
            .as_str()
            .unwrap()
            .starts_with("https://drive.google.com/")
    );
    let (s, v) = r
        .upload(
            &r.admin,
            "?name=signed.pdf&parent=sdf1",
            "application/pdf",
            b"%PDF-1.4 x",
        )
        .await;
    assert_eq!(s, 201, "{v}");
    assert_eq!(v["name"], "signed.pdf");
    {
        let st = g.state.lock().unwrap();
        let posts: Vec<_> = st.posts.iter().collect();
        assert_eq!(posts.len(), 3, "{posts:?}");
        assert!(
            posts[0].2.contains(r#""parents":["sd1"]"#),
            "{:?}",
            posts[0]
        );
        assert!(posts[0].1.contains("supportsAllDrives=true"));
        assert!(posts[1].2.contains(r#""parents":["sdf1"]"#));
        assert!(posts[2].0.starts_with("/upload/") && posts[2].2.contains("%PDF-1.4 x"));
    }
    assert_eq!(r.events("drive_create"), 2);
    assert_eq!(r.events("drive_upload"), 1);

    // A folder outside the workspace is refused before anything is written.
    let before = g.state.lock().unwrap().posts.len();
    let (s, _) = r
        .post(
            Some(&r.admin),
            "/api/drive/folders",
            json!({"name": "x", "parent": "dir-elsewhere"}),
        )
        .await;
    assert_eq!(s, 502);
    assert_eq!(g.state.lock().unwrap().posts.len(), before);
    assert_eq!(r.events("drive_create_failed"), 1);

    // Who may write is its own group: browsing alone does not allow it.
    let (s, _) = r.policy(json!({"allowed_group": "product.manager"})).await;
    assert_eq!(s, 200);
    assert_eq!(r.status(&r.member).await["may_write"], false);
    let (s, _) = r
        .post(Some(&r.member), "/api/drive/folders", json!({"name": "m"}))
        .await;
    assert_eq!(s, 403);
    r.policy(json!({"write_group": "product.manager"})).await;
    assert_eq!(r.status(&r.member).await["may_write"], true);
    let (s, _) = r
        .post(Some(&r.member), "/api/drive/folders", json!({"name": "m"}))
        .await;
    assert_eq!(s, 201);
    // People outside the browse group are refused, for a folder and for an upload.
    assert_eq!(
        r.post(
            Some(&r.outsider),
            "/api/drive/folders",
            json!({"name": "o"})
        )
        .await
        .0,
        403
    );
    assert_eq!(
        r.upload(&r.outsider, "?name=o.txt", "text/plain", b"o")
            .await
            .0,
        403
    );

    // Clearing the workspace goes back to the whole Drive and closes writing.
    r.policy(json!({"workspace_id": "", "workspace_name": ""}))
        .await;
    let v = r.status(&r.admin).await;
    assert_eq!(
        (v["workspace"].clone(), v["may_write"].clone()),
        (J::Null, json!(false))
    );
    assert_eq!(
        r.get(Some(&r.admin), "/api/drive/files").await.1["files"][1]["name"],
        "NDA - Acme Oy"
    );
    assert_no_plaintext(&r, &[CLIENT_SECRET, REFRESH, "at-1"]);
}

async fn a_sign_in_that_did_not_grant_writing_cannot_create_and_says_how_to_fix_it() {
    let r = rig().await;
    let g = fake();
    g.state.lock().unwrap().posts.clear();
    r.connect(
        Some("https://www.googleapis.com/auth/drive.readonly"),
        "code-w2",
    )
    .await;
    r.policy(json!({"workspace_id": "sd1", "workspace_name": "SpaceCorps shared"}))
        .await;
    let v = r.status(&r.admin).await;
    assert_eq!(v["settings"]["write_granted"], false);
    assert_eq!(v["may_write"], false);
    // Browsing still works; writing says to sign in again.
    assert_eq!(r.get(Some(&r.admin), "/api/drive/files").await.0, 200);
    let (s, v) = r
        .post(Some(&r.admin), "/api/drive/folders", json!({"name": "x"}))
        .await;
    assert_eq!(s, 409);
    assert!(
        v["error"]
            .as_str()
            .unwrap()
            .contains("sign in with Google again")
    );
    assert!(g.state.lock().unwrap().posts.is_empty());
    g.state.lock().unwrap().scope_response = None;
}

fn the_google_addresses_cannot_be_pointed_anywhere_but_loopback() {
    assert!(
        use_endpoints(Some(Endpoints {
            auth_url: "https://evil.example/authorize".into(),
            token_url: "https://evil.example/token".into(),
            api_base: "https://evil.example".into(),
        }))
        .is_err()
    );
    // The fake of the other tests is still in place.
    let _ = fake();
}

// ---- Golden pin of the return page after a Drive sign-in (S1a-0 of the branding work) -----------

/// `GET /oauth/callback` after a good Google sign-in answers "Connected" with the way back to the
/// Drive page of the back office. `connectors_tests` pins the other answers of this page (and the
/// one for any other connection); this one needs the fake Google, so it is a scenario of the test
/// above, which shares that fake. It is what an instance with no brand sends today.
async fn pins_the_return_page_after_a_drive_sign_in() {
    use crate::mfa_tests::pin;
    let r = rig().await;
    let g = fake();
    let (_id, q) = r.start("product.manager").await;
    g.state
        .lock()
        .unwrap()
        .codes
        .insert("code-pin".into(), q["code_challenge"].clone());
    let req = Request::get(format!(
        "/oauth/callback?state={}&code=code-pin",
        q["state"]
    ))
    .body(Body::empty())
    .unwrap();
    let answer = pin::answer(r.router.clone().oneshot(req).await.unwrap()).await;
    assert!(
        answer.contains("Connected") && answer.contains("href=\"/#/connectors/google_drive\""),
        "{answer}"
    );
    pin::check(
        "CONNECTOR_DRIVE_PINS",
        CONNECTOR_DRIVE_PINS,
        &[("connected_drive".to_string(), pin::sha(&answer))],
    );
}

// Printed by `OBOP_PRINT_PINS=1` on origin/main 12a59a79 and pasted here.
#[rustfmt::skip]
const CONNECTOR_DRIVE_PINS: &[(&str, &str)] = &[
    ("connected_drive", "a510fe706af0ae76a5be5ee787d1eafd75e64d072fa25c0960f547767e175cc8"),
];
