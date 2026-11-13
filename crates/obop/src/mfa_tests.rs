//! D53 over HTTP: the two-step sign-in, enrolment, the limited session, agent isolation, the
//! settings switch and the read-only `security_event` model.

use crate::app::App;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use obop_auth::{Auth, AuthConfig, totp};
use serde_json::{Value as J, json};
use std::sync::Arc;
use tower::ServiceExt;

pub(crate) const PW: &str = "correct horse battery";
pub(crate) const T0: u64 = 1_700_000_010;

pub(crate) struct Rig {
    pub(crate) r: axum::Router,
    pub(crate) auth: Arc<Auth>,
}

pub(crate) fn rig(require_admins: bool) -> Rig {
    let app = App::new();
    app.seed_demo();
    let auth = Arc::new(Auth::with_test_config(
        app.store.backend(),
        AuthConfig {
            default_require_admins: require_admins,
            ..AuthConfig::insecure_dev()
        },
    ));
    auth.set_clock_for_test(T0);
    auth.create_user("root", PW, &["platform.admin".to_string()])
        .unwrap();
    auth.create_user("ada", PW, &["base.user".to_string()])
        .unwrap();
    auth.create_user("bob", PW, &["base.user".to_string()])
        .unwrap();
    let r = obop_http::router(
        obop_http::HttpState::new(app.dispatcher.clone(), auth.clone()),
        None,
    );
    Rig { r, auth }
}

pub(crate) async fn send(
    r: &axum::Router,
    method: &str,
    token: Option<&str>,
    path: &str,
    body: J,
) -> (u16, J) {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let resp = r
        .clone()
        .oneshot(req.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(J::Null))
}

pub(crate) async fn post(r: &axum::Router, t: Option<&str>, path: &str, body: J) -> (u16, J) {
    send(r, "POST", t, path, body).await
}

pub(crate) async fn first_step(r: &axum::Router, user: &str) -> J {
    let (s, b) = post(
        r,
        None,
        "/api/auth/login",
        json!({"username": user, "password": PW}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    b
}

pub(crate) async fn token(r: &axum::Router, user: &str) -> String {
    first_step(r, user).await["token"]
        .as_str()
        .unwrap()
        .to_string()
}

/// Enrol over HTTP; returns the secret and the recovery codes.
pub(crate) async fn enrol(rig: &Rig, user: &str) -> (Vec<u8>, Vec<String>) {
    let t = token(&rig.r, user).await;
    let (s, b) = post(&rig.r, Some(&t), "/api/auth/mfa/begin", json!({})).await;
    assert_eq!(s, 200, "{b}");
    assert!(b["uri"].as_str().unwrap().starts_with("otpauth://totp/"));
    let secret = totp::base32_decode(b["secret"].as_str().unwrap()).unwrap();
    let code = totp::code_for_step(&secret, totp::step_at(T0));
    let (s, b) = post(
        &rig.r,
        Some(&t),
        "/api/auth/mfa/confirm",
        json!({"code": code}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    let codes = b["recovery_codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_string())
        .collect();
    (secret, codes)
}

pub(crate) fn code(secret: &[u8], at: u64) -> String {
    totp::code_for_step(secret, totp::step_at(at))
}

#[tokio::test]
async fn sign_in_is_two_steps_once_two_factor_is_on() {
    let rig = rig(false);
    let (secret, codes) = enrol(&rig, "ada").await;
    rig.auth.set_clock_for_test(T0 + 30);

    let b = first_step(&rig.r, "ada").await;
    assert_eq!(b["mfa_required"], json!(true));
    assert!(b.get("token").is_none(), "no session before the code: {b}");
    let mt = b["mfa_token"].as_str().unwrap().to_string();
    // The mfa_token is not a bearer token.
    assert_eq!(
        post(&rig.r, Some(&mt), "/api/party/list", json!({}))
            .await
            .0,
        401
    );

    // A wrong code is a 401 with a stable code, not a session.
    let (s, b) = post(
        &rig.r,
        None,
        "/api/auth/login/mfa",
        json!({"mfa_token": mt, "code": "000000"}),
    )
    .await;
    assert_eq!((s, b["code"].as_str()), (401, Some("invalid_code")));
    let (s, b) = post(
        &rig.r,
        None,
        "/api/auth/login/mfa",
        json!({"mfa_token": mt, "code": code(&secret, T0 + 30)}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    let session = b["token"].as_str().unwrap().to_string();
    assert_eq!(
        post(&rig.r, Some(&session), "/api/party/list", json!({}))
            .await
            .0,
        200
    );
    // Spent.
    let (s, _) = post(
        &rig.r,
        None,
        "/api/auth/login/mfa",
        json!({"mfa_token": mt, "code": code(&secret, T0 + 30)}),
    )
    .await;
    assert_eq!(s, 401);

    // A recovery code signs in once.
    let mt = first_step(&rig.r, "ada").await["mfa_token"]
        .as_str()
        .unwrap()
        .to_string();
    let (s, b) = post(
        &rig.r,
        None,
        "/api/auth/login/mfa",
        json!({"mfa_token": mt, "recovery_code": codes[0]}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    let mt = first_step(&rig.r, "ada").await["mfa_token"]
        .as_str()
        .unwrap()
        .to_string();
    let (s, _) = post(
        &rig.r,
        None,
        "/api/auth/login/mfa",
        json!({"mfa_token": mt, "recovery_code": codes[0]}),
    )
    .await;
    assert_eq!(s, 401);

    // Status and the other flows need a session.
    let (s, b) = send(&rig.r, "GET", Some(&session), "/api/auth/mfa", json!({})).await;
    assert_eq!(
        (s, b["enabled"].clone(), b["recovery_remaining"].clone()),
        (200, json!(true), json!(9))
    );
    assert_eq!(
        send(&rig.r, "GET", None, "/api/auth/mfa", json!({}))
            .await
            .0,
        401
    );
}

#[tokio::test]
async fn repeated_wrong_codes_lock_the_account_out_with_429() {
    let rig = rig(false);
    let (secret, _) = enrol(&rig, "ada").await;
    rig.auth.set_clock_for_test(T0 + 30);
    let mt = first_step(&rig.r, "ada").await["mfa_token"]
        .as_str()
        .unwrap()
        .to_string();
    let mut last = 0;
    for _ in 0..5 {
        last = post(
            &rig.r,
            None,
            "/api/auth/login/mfa",
            json!({"mfa_token": mt, "code": "000000"}),
        )
        .await
        .0;
    }
    assert_eq!(last, 401);
    let (s, b) = post(
        &rig.r,
        None,
        "/api/auth/login/mfa",
        json!({"mfa_token": mt, "code": code(&secret, T0 + 30)}),
    )
    .await;
    assert_eq!((s, b["code"].as_str()), (429, Some("rate_limited")));
    // The password step is locked too (shared lockout).
    let (s, _) = post(
        &rig.r,
        None,
        "/api/auth/login",
        json!({"username": "ada", "password": PW}),
    )
    .await;
    assert_eq!(s, 429);
}

#[tokio::test]
async fn an_admin_without_two_factor_is_held_to_enrolment_everywhere() {
    let rig = rig(true);
    let b = first_step(&rig.r, "root").await;
    assert_eq!(b["mfa_enrolment_required"], json!(true));
    let t = b["token"].as_str().unwrap().to_string();

    // Every command, the metadata and token minting answer 403 with a code and a hint.
    for path in [
        "/api/party/list",
        "/api/security_event/list",
        "/api/setting/get",
        "/api/auth/agent-token",
        "/api/approve/x",
    ] {
        let (s, b) = post(&rig.r, Some(&t), path, json!({"scopes": ["base.user"]})).await;
        assert_eq!(
            (s, b["code"].as_str()),
            (403, Some("mfa_enrolment_required")),
            "{path}"
        );
        assert!(b["hint"].as_str().unwrap().contains("/api/auth/mfa/begin"));
    }
    assert_eq!(
        send(&rig.r, "GET", Some(&t), "/api/meta", json!({}))
            .await
            .0,
        403
    );
    assert_eq!(
        send(&rig.r, "GET", Some(&t), "/api/commands", json!({}))
            .await
            .0,
        403
    );
    let (s, b) = post(&rig.r, Some(&t), "/mcp", json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}})).await;
    assert_eq!(
        (s, b["code"].as_str()),
        (403, Some("mfa_enrolment_required"))
    );

    // Enrolment is open to it, and finishing lifts the same session.
    let (s, b) = send(&rig.r, "GET", Some(&t), "/api/auth/mfa", json!({})).await;
    assert_eq!(
        (s, b["limited"].clone(), b["require_admins"].clone()),
        (200, json!(true), json!(true))
    );
    let (s, b) = post(&rig.r, Some(&t), "/api/auth/mfa/begin", json!({})).await;
    assert_eq!(s, 200);
    let secret = totp::base32_decode(b["secret"].as_str().unwrap()).unwrap();
    let (s, _) = post(
        &rig.r,
        Some(&t),
        "/api/auth/mfa/confirm",
        json!({"code": "000000"}),
    )
    .await;
    assert_eq!(s, 401);
    assert_eq!(
        post(&rig.r, Some(&t), "/api/party/list", json!({})).await.0,
        403,
        "still limited"
    );
    let (s, _) = post(
        &rig.r,
        Some(&t),
        "/api/auth/mfa/confirm",
        json!({"code": code(&secret, T0)}),
    )
    .await;
    assert_eq!(s, 200);
    assert_eq!(
        post(&rig.r, Some(&t), "/api/party/list", json!({})).await.0,
        200
    );
    assert_eq!(
        first_step(&rig.r, "root").await["mfa_required"],
        json!(true)
    );

    // Others are not held to it.
    assert!(
        first_step(&rig.r, "ada")
            .await
            .get("mfa_enrolment_required")
            .is_none()
    );
}

#[tokio::test]
async fn the_policy_is_an_instance_setting_that_admins_can_change() {
    let rig = rig(true);
    // Sign in limited, enrol, then change the setting as the (now full) admin.
    let (secret, _) = enrol(&rig, "root").await;
    rig.auth.set_clock_for_test(T0 + 30);
    let mt = first_step(&rig.r, "root").await["mfa_token"]
        .as_str()
        .unwrap()
        .to_string();
    let (_, b) = post(
        &rig.r,
        None,
        "/api/auth/login/mfa",
        json!({"mfa_token": mt, "code": code(&secret, T0 + 30)}),
    )
    .await;
    let admin = b["token"].as_str().unwrap().to_string();
    let (s, _) = post(
        &rig.r,
        Some(&admin),
        "/api/setting/update",
        json!({"id": "instance", "value": json!({"security": {"require_mfa_for_admins": true, "require_mfa_for_all": true}}).to_string()}),
    )
    .await;
    assert_eq!(s, 200);
    assert_eq!(
        first_step(&rig.r, "ada").await["mfa_enrolment_required"],
        json!(true)
    );
    // Any signed-in user can read the setting (the sign-in screen needs none of it).
    let (s, b) = send(&rig.r, "GET", Some(&admin), "/api/auth/mfa", json!({})).await;
    assert_eq!((s, b["require_all"].clone()), (200, json!(true)));
}

#[tokio::test]
async fn agent_tokens_cannot_touch_two_factor() {
    let rig = rig(false);
    enrol(&rig, "ada").await;
    // ada now needs a code to sign in: use the break-in-free route, a token issued before.
    let human = rig.auth.principal("root").unwrap();
    let agent = {
        use obop_commands::Authenticator;
        rig.auth
            .issue_agent_token(&human, &["platform.admin".to_string()], "bot")
            .unwrap()
    };
    for (m, path) in [
        ("GET", "/api/auth/mfa"),
        ("POST", "/api/auth/mfa/begin"),
        ("POST", "/api/auth/mfa/confirm"),
        ("POST", "/api/auth/mfa/disable"),
        ("POST", "/api/auth/mfa/recovery"),
        ("POST", "/api/auth/mfa/reset"),
    ] {
        let (s, _) = send(
            &rig.r,
            m,
            Some(&agent),
            path,
            json!({"user": "ada", "code": "123456", "password": PW}),
        )
        .await;
        assert_eq!(s, 404, "{path}");
    }
    assert!(rig.auth.mfa_enabled("ada"));
}

#[tokio::test]
async fn disable_and_regenerate_need_the_password_and_a_code() {
    let rig = rig(false);
    let (secret, codes) = enrol(&rig, "ada").await;
    rig.auth.set_clock_for_test(T0 + 30);
    let mt = first_step(&rig.r, "ada").await["mfa_token"]
        .as_str()
        .unwrap()
        .to_string();
    let (_, b) = post(
        &rig.r,
        None,
        "/api/auth/login/mfa",
        json!({"mfa_token": mt, "recovery_code": codes[0]}),
    )
    .await;
    let t = b["token"].as_str().unwrap().to_string();

    let (s, _) = post(
        &rig.r,
        Some(&t),
        "/api/auth/mfa/disable",
        json!({"password": "nope nope nope", "code": code(&secret, T0 + 30)}),
    )
    .await;
    assert_eq!(s, 401);
    let (s, _) = post(
        &rig.r,
        Some(&t),
        "/api/auth/mfa/disable",
        json!({"password": PW}),
    )
    .await;
    assert_eq!(s, 400, "a code is required");
    let (s, b) = post(
        &rig.r,
        Some(&t),
        "/api/auth/mfa/recovery",
        json!({"password": PW, "code": code(&secret, T0 + 30)}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["recovery_codes"].as_array().unwrap().len(), 10);
    let (s, _) = post(
        &rig.r,
        Some(&t),
        "/api/auth/mfa/disable",
        json!({"password": PW, "recovery_code": b["recovery_codes"][0]}),
    )
    .await;
    assert_eq!(s, 200);
    assert!(first_step(&rig.r, "ada").await.get("token").is_some());
}

#[tokio::test]
async fn admins_reset_others_and_the_audit_log_is_a_read_only_model() {
    let rig = rig(false);
    enrol(&rig, "bob").await;
    let admin = token(&rig.r, "root").await;
    let ada = token(&rig.r, "ada").await;

    // Not an admin: a plain 404. Not oneself: 400.
    assert_eq!(
        post(
            &rig.r,
            Some(&ada),
            "/api/auth/mfa/reset",
            json!({"user": "bob"})
        )
        .await
        .0,
        404
    );
    assert_eq!(
        post(
            &rig.r,
            Some(&admin),
            "/api/auth/mfa/reset",
            json!({"user": "root"})
        )
        .await
        .0,
        400
    );
    assert_eq!(
        post(
            &rig.r,
            Some(&admin),
            "/api/auth/mfa/reset",
            json!({"user": "bob"})
        )
        .await
        .0,
        200
    );
    assert!(first_step(&rig.r, "bob").await.get("token").is_some());

    // The log is readable by the admin through the generated model and nobody can write it.
    let (s, b) = post(&rig.r, Some(&admin), "/api/security_event/list", json!({})).await;
    assert_eq!(s, 200, "{b}");
    let rows = b["value"].as_array().unwrap();
    let kinds: Vec<&str> = rows.iter().map(|r| r["kind"].as_str().unwrap()).collect();
    assert!(
        kinds.contains(&"enrol_confirm") && kinds.contains(&"reset"),
        "{kinds:?}"
    );
    let reset = rows.iter().find(|r| r["kind"] == "reset").unwrap();
    assert_eq!(
        (reset["user"].as_str(), reset["actor"].as_str()),
        (Some("bob"), Some("root"))
    );
    assert!(reset["at"].as_str().unwrap().ends_with('Z'));
    let (s, b) = post(
        &rig.r,
        Some(&admin),
        "/api/security_event/search",
        json!({"filter": [{"field": "kind", "op": "eq", "value": "reset"}]}),
    )
    .await;
    assert_eq!((s, b["value"]["total"].clone()), (200, json!(1)));
    for c in ["create", "update", "delete"] {
        let (s, _) = post(
            &rig.r,
            Some(&admin),
            &format!("/api/security_event/{c}"),
            json!({"id": "x"}),
        )
        .await;
        assert_eq!(s, 404, "{c}");
    }
    assert_eq!(
        post(&rig.r, Some(&ada), "/api/security_event/list", json!({}))
            .await
            .0,
        404
    );
    // And it is in the metadata, so the UI shows it like any model.
    let (s, b) = send(&rig.r, "GET", Some(&admin), "/api/meta", json!({})).await;
    assert_eq!(s, 200);
    let text = b.to_string();
    assert!(text.contains("security_event"));
}

#[tokio::test]
async fn the_dev_authenticator_has_no_two_factor_routes() {
    let app = App::new();
    let r = obop_http::router(
        obop_http::HttpState::new(app.dispatcher.clone(), Arc::new(crate::app::DevAuth)),
        None,
    );
    assert_eq!(
        send(&r, "GET", Some("dev-admin"), "/api/auth/mfa", json!({}))
            .await
            .0,
        404
    );
    assert_eq!(
        post(
            &r,
            None,
            "/api/auth/login/mfa",
            json!({"mfa_token": "x", "code": "1"})
        )
        .await
        .0,
        404
    );
}

/// The control plane gets the same optional TOTP for the owner's own account, under
/// /api/platform, and it is never enforced there.
#[tokio::test]
async fn control_plane_accounts_can_opt_in_to_two_factor() {
    let backend = obop_storage::open("memory:").unwrap();
    let auth = Arc::new(Auth::with_test_config(
        backend.clone(),
        AuthConfig {
            default_require_admins: true,
            ..AuthConfig::insecure_dev()
        },
    ));
    auth.set_clock_for_test(T0);
    let platform = obop_platform::Platform::new(
        auth.clone(),
        backend,
        Arc::new(obop_platform::provision::Fake::default()),
    );
    let r = obop_http::platform_router(Arc::new(obop_platform::Api(platform)), None);
    let (s, b) = post(
        &r,
        None,
        "/api/platform/signup",
        json!({"email": "ada@example.test", "password": PW}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    let t = b["token"].as_str().unwrap().to_string();
    // Not enforced: a plain sign-in works until the owner opts in.
    let (s, b) = post(
        &r,
        None,
        "/api/platform/login",
        json!({"email": "ada@example.test", "password": PW}),
    )
    .await;
    assert_eq!(
        (
            s,
            b["token"].is_string(),
            b.get("mfa_enrolment_required").is_none()
        ),
        (200, true, true)
    );
    let (s, b) = post(&r, Some(&t), "/api/platform/mfa/begin", json!({})).await;
    assert_eq!(s, 200, "{b}");
    let secret = totp::base32_decode(b["secret"].as_str().unwrap()).unwrap();
    let (s, _) = post(
        &r,
        Some(&t),
        "/api/platform/mfa/confirm",
        json!({"code": code(&secret, T0)}),
    )
    .await;
    assert_eq!(s, 200);

    auth.set_clock_for_test(T0 + 30);
    let (s, b) = post(
        &r,
        None,
        "/api/platform/login",
        json!({"email": "ada@example.test", "password": PW}),
    )
    .await;
    assert_eq!(
        (s, b["mfa_required"].clone(), b.get("token").is_none()),
        (200, json!(true), true)
    );
    let mt = b["mfa_token"].as_str().unwrap().to_string();
    let (s, _) = post(
        &r,
        None,
        "/api/platform/login/mfa",
        json!({"mfa_token": mt, "code": "000000"}),
    )
    .await;
    assert_eq!(s, 401);
    let (s, b) = post(
        &r,
        None,
        "/api/platform/login/mfa",
        json!({"mfa_token": mt, "code": code(&secret, T0 + 30)}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    let session = b["token"].as_str().unwrap().to_string();
    assert_eq!(
        send(&r, "GET", Some(&session), "/api/platform/me", json!({}))
            .await
            .0,
        200
    );
    let (s, b) = send(&r, "GET", Some(&session), "/api/platform/mfa", json!({})).await;
    assert_eq!((s, b["enabled"].clone()), (200, json!(true)));
}

#[test]
fn the_break_glass_command_resets_a_user_in_a_real_database() {
    use obop_commands::{Authenticator, MfaApi};
    // A directory of this run's own, removed again when the test ends, however it ends.
    let dir = obop_storage::testing::TempPath::dir("obop-mfa-cli");
    let url = format!("sqlite://{}", dir.path().join("t.db").display());
    let store = obop_commands::Store::open(&url).unwrap();
    let auth = Auth::with_test_config(store.backend(), AuthConfig::insecure_dev());
    auth.set_clock_for_test(T0);
    auth.create_user("ada", PW, &[]).unwrap();
    let t = auth.login("ada", PW).unwrap();
    let p = auth.resolve(&t).unwrap();
    let begun = auth.begin(&p).unwrap();
    let secret = totp::base32_decode(begun["secret"].as_str().unwrap()).unwrap();
    auth.confirm(&p, &t, &totp::code_for_step(&secret, totp::step_at(T0)))
        .unwrap();
    assert!(auth.mfa_enabled("ada"));
    drop((auth, store));

    let o = crate::Opts {
        db: Some(url.clone()),
        token: None,
        demo: false,
        dev_auth: false,
        public_site: false,
        trust_proxy: false,
        admin_prefix: "/admin".into(),
    };
    assert!(crate::admin_reset_mfa(&o, "ghost").is_err());
    assert!(crate::admin_reset_mfa(&o, "ada").is_ok());
    let store = obop_commands::Store::open(&url).unwrap();
    let auth = Auth::with_test_config(store.backend(), AuthConfig::insecure_dev());
    assert!(!auth.mfa_enabled("ada"));
    assert!(auth.resolve(&t).is_none(), "sessions revoked");
    let log = store.backend().list(obop_auth::EVENT_TABLE).unwrap();
    assert!(
        log.iter()
            .any(|(_, f)| f["kind"] == "reset" && f["actor"] == "operator")
    );
}

/// Golden pins (S1a-0 of the branding work, docs/spec/branding.md): the digests of what an
/// instance with no brand row sends today, taken from `origin/main` before any brand code
/// existed. A table is a list of `(name, SHA-256 hex)`; a test makes the same list from the real
/// router and holds it to the table. To read the values again from the tree as it is (in the
/// commit that explains why they move), run
///
/// ```text
/// OBOP_PRINT_PINS=1 cargo test -p obop -- pins the_google_drive_page_end_to_end --nocapture \
///     --test-threads=1
/// ```
///
/// which prints each table as Rust source instead of asserting it, and then panics: print mode is
/// not a check, so a stray `OBOP_PRINT_PINS=1` cannot give a green run. (The pin of the return page
/// after a Drive sign-in is a scenario of `the_google_drive_page_end_to_end_against_a_fake_google`,
/// which shares one fake Google, so that test is in the filter. `OBOP_SHOW_PINNED=1` prints the
/// text of an answer that `sha` hashes.)
pub(crate) mod pin {
    use super::{Arc, Auth, AuthConfig, PW, send};
    use crate::app::App;
    use serde_json::json;

    /// The SHA-256 of a text, as lowercase hex. With `OBOP_SHOW_PINNED=1` the text is printed too
    /// (`--nocapture`), to see what a pin that moved was a pin of.
    pub(crate) fn sha(text: &str) -> String {
        let digest = obop_oauth::util::sha_hex(text);
        if std::env::var("OBOP_SHOW_PINNED").is_ok_and(|v| v == "1") {
            println!("\n---- text of {digest} ----\n{text}\n---- end ----");
        }
        digest
    }

    /// Print the table as source, or hold `now` to `pinned`: the same names in the same order, the
    /// same digests. A failure lists every pin that moved.
    pub(crate) fn check(table: &str, pinned: &[(&str, &str)], now: &[(String, String)]) {
        if std::env::var("OBOP_PRINT_PINS").is_ok_and(|v| v == "1") {
            let mut out = format!("\n//@pins {table}\nconst {table}: &[(&str, &str)] = &[\n");
            for (name, digest) in now {
                out.push_str(&format!("    (\"{name}\", \"{digest}\"),\n"));
            }
            out.push_str(&format!("];\n//@end {table}\n"));
            println!("{out}");
            panic!(
                "OBOP_PRINT_PINS=1 is print mode, not a check: this run printed the table and \
                 proved nothing. Unset it to run the pins."
            );
        }
        assert!(
            !pinned.is_empty(),
            "{table} is empty: print it with OBOP_PRINT_PINS=1"
        );
        let mut moved = Vec::new();
        for (i, (name, digest)) in now.iter().enumerate() {
            match pinned.get(i) {
                Some((n, d)) if n == name && d == digest => {}
                Some((n, _)) if n != name => moved.push(format!("{name}: the table has {n} here")),
                Some(_) => moved.push(format!("{name}: {digest}")),
                None => moved.push(format!("{name}: not in the table")),
            }
        }
        if pinned.len() > now.len() {
            moved.push(format!(
                "the table has {} entries, the test makes {}",
                pinned.len(),
                now.len()
            ));
        }
        assert!(
            moved.is_empty(),
            "{table}: {} of {} pins moved (an output differs from what an instance with no brand \
             row has always sent):\n{}",
            moved.len(),
            now.len(),
            moved.join("\n")
        );
    }

    /// The answer of a router as one text: the status, every header (sorted by name; not the
    /// length, which the body's own pin already holds and which moves with the version string), a
    /// blank line and the body. What the pin of a response hashes.
    pub(crate) async fn answer(resp: axum::response::Response) -> String {
        use http_body_util::BodyExt;
        let status = resp.status().as_u16();
        let mut headers: Vec<String> = resp
            .headers()
            .iter()
            .filter(|(k, _)| *k != axum::http::header::CONTENT_LENGTH)
            .map(|(k, v)| format!("{k}: {}", String::from_utf8_lossy(v.as_bytes())))
            .collect();
        headers.sort();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        format!(
            "{status}\n{}\n\n{}",
            headers.join("\n"),
            String::from_utf8_lossy(&body)
        )
    }

    /// An instance whose anonymous documents are pinned: its name in the pin table, its router and
    /// a token that works on it.
    pub(crate) struct Instance {
        pub(crate) name: &'static str,
        pub(crate) router: axum::Router,
        pub(crate) token: String,
        /// The app behind the router, for a test that writes a record to its store.
        pub(crate) app: App,
    }

    /// The two instances the anonymous documents are pinned on: `dev`, over the development
    /// authenticator (no OAuth server), and `auth`, over the real `Auth` (which has one). Both use
    /// the default configuration, so a variable of the environment (`OBOP_PUBLIC_URL`) cannot move
    /// a pin.
    pub(crate) async fn instances() -> Vec<Instance> {
        let dev_app = App::new();
        dev_app.seed_demo();
        let dev = obop_http::router(
            obop_http::HttpState::with_mcp(
                dev_app.dispatcher.clone(),
                Arc::new(crate::app::DevAuth),
                obop_http::mcp::McpConfig::default(),
            ),
            None,
        );
        let app = App::new();
        app.seed_demo();
        let auth = Arc::new(Auth::with_test_config(
            app.store.backend(),
            AuthConfig::insecure_dev(),
        ));
        auth.create_user("ada", PW, &["base.user".to_string()])
            .unwrap();
        let real = obop_http::router(
            obop_http::HttpState::with_mcp(
                app.dispatcher.clone(),
                auth,
                obop_http::mcp::McpConfig::default(),
            ),
            None,
        );
        let (s, b) = send(
            &real,
            "POST",
            None,
            "/api/auth/login",
            json!({"username": "ada", "password": PW}),
        )
        .await;
        assert_eq!(s, 200, "{b}");
        let token = b["token"].as_str().unwrap().to_string();
        vec![
            Instance {
                name: "dev",
                router: dev,
                token: "dev-agent".into(),
                app: dev_app,
            },
            Instance {
                name: "auth",
                router: real,
                token,
                app,
            },
        ]
    }

    /// `GET path` as a client of `inst.test` behind a TLS proxy asks, with no token: the answer as
    /// the pin hashes it, with the version of this build written as `<CORE_VERSION>`.
    pub(crate) async fn document(r: &axum::Router, path: &str) -> String {
        exchange(r, "GET", path).await
    }

    /// As `document`, for any method and with an empty body.
    pub(crate) async fn exchange(r: &axum::Router, method: &str, path: &str) -> String {
        exchange_as(r, method, path, None).await
    }

    /// As `exchange`, signed in with the bearer `token` when there is one.
    pub(crate) async fn exchange_as(
        r: &axum::Router,
        method: &str,
        path: &str,
        token: Option<&str>,
    ) -> String {
        use tower::ServiceExt;
        let mut req = axum::http::Request::builder()
            .method(method)
            .uri(path)
            .header("host", "inst.test")
            .header("x-forwarded-proto", "https");
        if let Some(t) = token {
            req = req.header("authorization", format!("Bearer {t}"));
        }
        let req = req.body(axum::body::Body::empty()).unwrap();
        unversioned(&answer(r.clone().oneshot(req).await.unwrap()).await)
    }

    /// The text with the version of this build written as `<CORE_VERSION>`, so that a pin of a
    /// document that names the version does not move with every release.
    pub(crate) fn unversioned(text: &str) -> String {
        text.replace(obop_types::CORE_VERSION, "<CORE_VERSION>")
    }
}
