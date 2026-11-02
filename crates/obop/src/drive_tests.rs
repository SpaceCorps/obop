//! The Drive browser's routes (`/api/drive/*`) against the real router and authenticator, with a
//! fake `DriveSource` (no call goes to Google).

use crate::app::App;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use obop_auth::Auth;
use obop_http::drive::{DriveEntry, DriveFile, DrivePage, DriveSource};
use obop_media::fixtures as fx;
use serde_json::{Value as J, json};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

#[derive(Default)]
struct Fake {
    asked: Mutex<Vec<String>>,
}

impl DriveSource for Fake {
    fn list(&self, parent: &str, search: &str, page: Option<&str>) -> Result<DrivePage, String> {
        self.asked
            .lock()
            .unwrap()
            .push(format!("list {parent}|{search}|{page:?}"));
        Ok(DrivePage {
            entries: vec![DriveEntry {
                id: "f1".into(),
                name: "Logo.png".into(),
                mime: "image/png".into(),
                folder: false,
                modified: "2026-10-30T09:00:00Z".into(),
                size: Some(10),
                web_link: "https://drive.google.com/file/d/f1/view".into(),
                importable: true,
            }],
            next: Some("tok".into()),
        })
    }

    fn fetch(&self, id: &str, _max: usize) -> Result<DriveFile, String> {
        self.asked.lock().unwrap().push(format!("fetch {id}"));
        if id == "gone" {
            return Err("Google Drive answered HTTP 404: File not found".into());
        }
        Ok(DriveFile {
            id: id.into(),
            name: "Logo.png".into(),
            mime: "image/png".into(),
            modified: "2026-10-30T09:00:00Z".into(),
            web_link: "https://drive.google.com/file/d/f1/view".into(),
            bytes: fx::png(8, 8, 3),
        })
    }
}

struct Fx {
    router: axum::Router,
    app: App,
    admin: String,
    other: String,
    agent: String,
    fake: Arc<Fake>,
}

async fn call(r: &axum::Router, req: Request<Body>) -> (u16, J) {
    let resp = r.clone().oneshot(req).await.unwrap();
    let status = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(J::Null))
}

async fn post(r: &axum::Router, token: Option<&str>, path: &str, body: J) -> (u16, J) {
    let mut b = Request::post(path).header("content-type", "application/json");
    if let Some(t) = token {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    call(r, b.body(Body::from(body.to_string())).unwrap()).await
}

async fn get(r: &axum::Router, token: Option<&str>, path: &str) -> (u16, J) {
    let mut b = Request::get(path);
    if let Some(t) = token {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    call(r, b.body(Body::empty()).unwrap()).await
}

async fn fixture(connected: bool) -> Fx {
    let app = App::new();
    let auth = Arc::new(Auth::with_test_params(app.store.backend()));
    let pw = "a long enough password";
    auth.create_user(
        "admin",
        pw,
        &["platform.admin".into(), "product.manager".into()],
    )
    .unwrap();
    auth.create_user("other", pw, &["product.manager".into()])
        .unwrap();
    let state = obop_http::HttpState::new(app.dispatcher.clone(), auth);
    let fake = Arc::new(Fake::default());
    if connected {
        state.set_drive(fake.clone());
    }
    let router = obop_http::router(state, None);
    let login = |u: &'static str| {
        let r = router.clone();
        async move {
            let (s, v) = post(
                &r,
                None,
                "/api/auth/login",
                json!({"username": u, "password": "a long enough password"}),
            )
            .await;
            assert_eq!(s, 200);
            v["token"].as_str().unwrap().to_string()
        }
    };
    let (admin, other) = (login("admin").await, login("other").await);
    let (s, v) = post(
        &router,
        Some(&admin),
        "/api/auth/agent-token",
        json!({"scopes": ["platform.admin"], "label": "bot"}),
    )
    .await;
    assert_eq!(s, 200, "{v}");
    let agent = v["token"].as_str().unwrap().to_string();
    Fx {
        router,
        app,
        admin,
        other,
        agent,
        fake,
    }
}

#[tokio::test]
async fn status_says_whether_a_drive_is_connected_and_whether_you_may_browse() {
    let f = fixture(true).await;
    let (s, v) = get(&f.router, Some(&f.admin), "/api/drive/status").await;
    assert_eq!(
        (s, &v),
        (
            200,
            &json!({"connected": true, "may_browse": true, "may_configure": true, "may_write": false, "workspace": null})
        )
    );
    let (_, v) = get(&f.router, Some(&f.other), "/api/drive/status").await;
    assert_eq!(
        v,
        json!({"connected": true, "may_browse": false, "may_configure": false, "may_write": false, "workspace": null})
    );
    let (_, v) = get(&f.router, Some(&f.agent), "/api/drive/status").await;
    assert_eq!(v["may_browse"], false, "an agent never browses");
    assert_eq!(get(&f.router, None, "/api/drive/status").await.0, 401);
    let off = fixture(false).await;
    let (_, v) = get(&off.router, Some(&off.admin), "/api/drive/status").await;
    assert_eq!(
        v,
        json!({"connected": false, "may_browse": false, "may_configure": true, "may_write": false, "workspace": null})
    );
}

#[tokio::test]
async fn listing_needs_the_group_and_passes_the_folder_search_and_page_through() {
    let f = fixture(true).await;
    assert_eq!(get(&f.router, None, "/api/drive/files").await.0, 401);
    assert_eq!(
        get(&f.router, Some(&f.other), "/api/drive/files").await.0,
        403
    );
    assert_eq!(
        get(&f.router, Some(&f.agent), "/api/drive/files").await.0,
        403
    );
    assert!(
        f.fake.asked.lock().unwrap().is_empty(),
        "refused before Drive"
    );
    let (s, v) = get(
        &f.router,
        Some(&f.admin),
        "/api/drive/files?parent=dir1&q=nda&page=p2",
    )
    .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["files"][0]["name"], "Logo.png");
    assert_eq!(v["files"][0]["importable"], true);
    assert_eq!(v["next"], "tok");
    assert_eq!(
        f.fake.asked.lock().unwrap().as_slice(),
        ["list dir1|nda|Some(\"p2\")"]
    );
    let off = fixture(false).await;
    assert_eq!(
        get(&off.router, Some(&off.admin), "/api/drive/files")
            .await
            .0,
        404
    );
}

#[tokio::test]
async fn an_import_becomes_an_upload_the_field_accepts_and_is_logged() {
    let f = fixture(true).await;
    let (s, v) = post(
        &f.router,
        Some(&f.admin),
        "/api/drive/import",
        json!({"id": "f1", "field": "product.image"}),
    )
    .await;
    assert_eq!(s, 201, "{v}");
    let blob = v["id"].as_str().unwrap();
    assert_eq!(blob.len(), 64);
    assert_eq!(v["content_type"], "image/png");
    assert_eq!(v["filename"], "Logo.png");
    assert_eq!(v["drive"]["id"], "f1");
    assert!(
        v["drive"]["web_link"]
            .as_str()
            .unwrap()
            .contains("drive.google.com")
    );
    // The id is what a record stores, exactly as after a direct upload.
    let (s, v) = post(
        &f.router,
        Some(&f.admin),
        "/api/product/create",
        json!({"name": "Teapot", "image": blob}),
    )
    .await;
    assert_eq!(s, 200, "{v}");
    let events = f.app.store.backend().list("security_event").unwrap();
    assert!(
        events
            .iter()
            .any(|(_, e)| e["kind"] == "drive_import"
                && e["detail"].as_str().unwrap().contains("f1")),
        "{events:?}"
    );
}

#[tokio::test]
async fn an_import_is_refused_for_the_wrong_person_field_or_file() {
    let f = fixture(true).await;
    let body = json!({"id": "f1", "field": "product.image"});
    assert_eq!(
        post(&f.router, None, "/api/drive/import", body.clone())
            .await
            .0,
        401
    );
    assert_eq!(
        post(&f.router, Some(&f.other), "/api/drive/import", body.clone())
            .await
            .0,
        403
    );
    assert_eq!(
        post(&f.router, Some(&f.agent), "/api/drive/import", body)
            .await
            .0,
        403
    );
    // Not an upload field, and a field that is not there.
    for field in ["product.name", "nope.nothing", "bad"] {
        let (s, _) = post(
            &f.router,
            Some(&f.admin),
            "/api/drive/import",
            json!({"id": "f1", "field": field}),
        )
        .await;
        assert_eq!(s, 400, "{field}");
    }
    assert_eq!(
        post(
            &f.router,
            Some(&f.admin),
            "/api/drive/import",
            json!({"id": "f1"})
        )
        .await
        .0,
        400
    );
    // Drive's refusal is passed on, and logged as a failure.
    let (s, v) = post(
        &f.router,
        Some(&f.admin),
        "/api/drive/import",
        json!({"id": "gone", "field": "product.image"}),
    )
    .await;
    assert_eq!(s, 502);
    assert!(v["error"].as_str().unwrap().contains("404"));
    let events = f.app.store.backend().list("security_event").unwrap();
    assert!(
        events
            .iter()
            .any(|(_, e)| e["kind"] == "drive_import_failed")
    );
}
