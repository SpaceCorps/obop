//! The first-administrator set-up over HTTP (D96, PROPOSED): the two routes in the router the
//! server serves, with the groups `obop admin create NAME --all-groups` gives. The rules themselves
//! are held in `obop-auth` (`setup_tests.rs`); these hold the answers, the headers, the limited
//! session that follows and the routes that do not exist under `--dev-auth`.

use crate::app::{App, DevAuth};
use crate::mfa_tests::send;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use obop_auth::{Auth, AuthConfig, SetupOpened};
use serde_json::{Value as J, json};
use std::sync::Arc;
use tower::ServiceExt;

const PW: &str = "a long enough password 1";

struct Rig {
    r: axum::Router,
    auth: Arc<Auth>,
    app: App,
}

fn rig_with(config: AuthConfig) -> Rig {
    let app = App::new();
    let auth = Arc::new(Auth::with_test_config(app.store.backend(), config));
    auth.enable_setup(app.all_groups()).unwrap();
    let r = obop_http::router(
        obop_http::HttpState::new(app.dispatcher.clone(), auth.clone()),
        None,
    );
    Rig { r, auth, app }
}

fn rig() -> Rig {
    rig_with(AuthConfig {
        default_require_admins: true,
        ..AuthConfig::insecure_dev()
    })
}

fn code(a: &Auth) -> String {
    match a.open_setup().unwrap() {
        SetupOpened::Code(c) => c,
        other => panic!("{other:?}"),
    }
}

/// Status, headers and the body as text.
async fn raw(r: &axum::Router, method: &str, path: &str, body: J) -> (u16, String, String) {
    let resp = r
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status().as_u16();
    let headers = format!("{:?}", resp.headers());
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

fn body(code: &str, user: &str) -> J {
    json!({"code": code, "username": user, "password": PW})
}

#[test]
fn all_groups_holds_the_administrator_group() {
    // The set-up page gives what `--all-groups` gives (rule 7), and is off unless that list holds
    // the administrator group, so this must stay true.
    assert!(
        App::new()
            .all_groups()
            .iter()
            .any(|g| g == obop_commands::ADMIN_GROUP)
    );
}

#[tokio::test]
async fn the_page_answers_open_then_creates_the_administrator_then_closes_for_good() {
    let rig = rig();
    let (s, h, b) = raw(&rig.r, "GET", "/api/auth/setup", J::Null).await;
    assert_eq!(s, 200);
    assert!(h.contains("no-store"), "{h}");
    assert_eq!(
        serde_json::from_str::<J>(&b).unwrap(),
        json!({"open": true, "code_active": false, "needs_secret_key": false})
    );
    let c = code(&rig.auth);
    let (_, _, b) = raw(&rig.r, "GET", "/api/auth/setup", J::Null).await;
    assert_eq!(serde_json::from_str::<J>(&b).unwrap()["code_active"], true);
    assert!(!b.contains(&c[..5]), "never a code");

    // A wrong code: 403, one word, no code in it.
    let (s, h, b) = raw(&rig.r, "POST", "/api/auth/setup", body("WRONG", "root")).await;
    assert_eq!(s, 403, "{b}");
    assert!(h.contains("no-store"));
    assert_eq!(
        serde_json::from_str::<J>(&b).unwrap()["code"],
        "setup_code_invalid"
    );

    // The right one, with a name: the answer of a sign-in, limited to enrolment.
    let mut ok = body(&c, "root");
    ok["instance_name"] = json!("Acme Test");
    let (s, _, b) = raw(&rig.r, "POST", "/api/auth/setup", ok).await;
    assert_eq!(s, 200, "{b}");
    assert!(!b.contains(&c[..5]));
    let out: J = serde_json::from_str(&b).unwrap();
    assert_eq!(out["mfa_enrolment_required"], true);
    let token = out["token"].as_str().unwrap();
    let (s, meta) = send(&rig.r, "GET", Some(token), "/api/meta", J::Null).await;
    assert_eq!(s, 403);
    assert_eq!(meta["code"], "mfa_enrolment_required");
    let (s, begun) = send(
        &rig.r,
        "POST",
        Some(token),
        "/api/auth/mfa/begin",
        json!({}),
    )
    .await;
    assert_eq!(s, 200, "{begun}");
    assert_eq!(begun["issuer"], "Acme Test");

    // Closed for good, with the right code too.
    let (_, _, b) = raw(&rig.r, "GET", "/api/auth/setup", J::Null).await;
    assert_eq!(
        serde_json::from_str::<J>(&b).unwrap(),
        json!({"open": false})
    );
    let (s, _, b) = raw(&rig.r, "POST", "/api/auth/setup", body(&c, "second")).await;
    assert_eq!(s, 409, "{b}");
    assert_eq!(
        serde_json::from_str::<J>(&b).unwrap()["code"],
        "setup_closed"
    );
    let groups = rig
        .app
        .store
        .backend()
        .get("auth_user", "root")
        .unwrap()
        .unwrap()["groups"]
        .clone();
    // What `--all-groups` writes, less `*` ("any signed-in principal"), which is no group to hold.
    let all: Vec<String> = rig
        .app
        .all_groups()
        .into_iter()
        .filter(|g| g != "*")
        .collect();
    assert_eq!(groups, json!(all));
}

#[tokio::test]
async fn too_many_attempts_are_answered_429_with_a_time_to_wait() {
    let rig = rig();
    let c = code(&rig.auth);
    for _ in 0..obop_auth::ADDR_MAX {
        let (s, _, _) = raw(&rig.r, "POST", "/api/auth/setup", body("WRONG", "root")).await;
        assert_eq!(s, 403);
    }
    let (s, h, b) = raw(&rig.r, "POST", "/api/auth/setup", body(&c, "root")).await;
    assert_eq!(s, 429, "{b}");
    assert!(h.contains("retry-after"), "{h}");
}

#[tokio::test]
async fn without_a_server_key_the_page_says_so_and_refuses() {
    let rig = rig_with(AuthConfig {
        keys: obop_keys::KeyRing::default(),
        ..AuthConfig::insecure_dev()
    });
    let (_, _, b) = raw(&rig.r, "GET", "/api/auth/setup", J::Null).await;
    assert_eq!(
        serde_json::from_str::<J>(&b).unwrap(),
        json!({"open": true, "code_active": false, "needs_secret_key": true})
    );
    let (s, _, b) = raw(&rig.r, "POST", "/api/auth/setup", body("ANY", "root")).await;
    assert_eq!(s, 503, "{b}");
    assert_eq!(
        serde_json::from_str::<J>(&b).unwrap()["code"],
        "setup_needs_secret_key"
    );
    assert_eq!(rig.app.store.backend().count("auth_user").unwrap(), 0);
}

#[tokio::test]
async fn bad_input_with_the_code_is_400_and_a_body_that_is_not_json_is_a_wrong_code() {
    let rig = rig();
    let c = code(&rig.auth);
    let (s, _, b) = raw(&rig.r, "POST", "/api/auth/setup", body(&c, "a b")).await;
    assert_eq!(s, 400, "{b}");
    let resp = rig
        .r
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/setup")
                .body(Body::from("not json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 403);
    assert_eq!(rig.app.store.backend().count("auth_user").unwrap(), 0);
}

#[tokio::test]
async fn under_dev_auth_the_routes_do_not_exist() {
    let app = App::new();
    let r = obop_http::router(
        obop_http::HttpState::new(app.dispatcher.clone(), Arc::new(DevAuth)),
        None,
    );
    let (s, _, _) = raw(&r, "GET", "/api/auth/setup", J::Null).await;
    assert_eq!(s, 404);
    let (s, _, _) = raw(&r, "POST", "/api/auth/setup", body("X", "root")).await;
    assert_eq!(s, 404);
}

#[tokio::test]
async fn an_authenticator_whose_set_up_is_off_has_no_routes() {
    let app = App::new();
    let auth = Arc::new(Auth::with_test_params(app.store.backend()));
    let r = obop_http::router(
        obop_http::HttpState::new(app.dispatcher.clone(), auth),
        None,
    );
    let (s, _, _) = raw(&r, "GET", "/api/auth/setup", J::Null).await;
    assert_eq!(s, 404);
}
