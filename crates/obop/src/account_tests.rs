//! B7 over HTTP: the session list, signing out everywhere, changing one's own password, the
//! revocation that follows, the administrator's account list with disable, enable and password
//! reset, and the audit of changes to the security settings.

use crate::mfa_tests::{PW, Rig, T0, code, enrol, first_step, post, rig, send, token};
use serde_json::{Value as J, json};

async fn get(r: &axum::Router, t: &str, path: &str) -> (u16, J) {
    send(r, "GET", Some(t), path, json!({})).await
}

async fn events(rig: &Rig, root: &str, kind: &str) -> Vec<J> {
    let (s, b) = post(
        &rig.r,
        Some(root),
        "/api/security_event/list",
        json!({"limit": 200}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    b["value"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == kind)
        .cloned()
        .collect()
}

#[tokio::test]
async fn the_session_list_and_sign_out_everywhere() {
    let rig = rig(false);
    let (t1, t2) = (token(&rig.r, "ada").await, token(&rig.r, "ada").await);
    let bob = token(&rig.r, "bob").await;
    let (s, b) = get(&rig.r, &t1, "/api/auth/sessions").await;
    assert_eq!(s, 200, "{b}");
    let rows = b["sessions"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows.iter().filter(|r| r["current"] == true).count(), 1);
    assert!(!b.to_string().contains(&t1) && !b.to_string().contains(&t2));
    // Without a session: nothing.
    assert_eq!(
        send(&rig.r, "GET", None, "/api/auth/sessions", json!({}))
            .await
            .0,
        401
    );
    // Keep this one.
    let (s, b) = post(
        &rig.r,
        Some(&t1),
        "/api/auth/sessions/revoke-all",
        json!({"keep_current": true}),
    )
    .await;
    assert_eq!((s, b["revoked"].clone()), (200, json!(1)));
    assert_eq!(
        post(&rig.r, Some(&t2), "/api/party/list", json!({}))
            .await
            .0,
        401
    );
    assert_eq!(
        post(&rig.r, Some(&t1), "/api/party/list", json!({}))
            .await
            .0,
        200
    );
    // Everywhere, this one included.
    let (s, b) = post(
        &rig.r,
        Some(&t1),
        "/api/auth/sessions/revoke-all",
        json!({}),
    )
    .await;
    assert_eq!((s, b["revoked"].clone()), (200, json!(1)));
    assert_eq!(
        post(&rig.r, Some(&t1), "/api/party/list", json!({}))
            .await
            .0,
        401
    );
    assert_eq!(
        post(&rig.r, Some(&bob), "/api/party/list", json!({}))
            .await
            .0,
        200,
        "others are untouched"
    );
}

#[tokio::test]
async fn an_agent_token_cannot_reach_the_account_routes() {
    let rig = rig(false);
    let t = token(&rig.r, "ada").await;
    let (s, b) = post(
        &rig.r,
        Some(&t),
        "/api/auth/agent-token",
        json!({"scopes": ["base.user"]}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    let agent = b["token"].as_str().unwrap();
    for path in ["/api/auth/sessions", "/api/auth/users"] {
        assert_eq!(get(&rig.r, agent, path).await.0, 404, "{path}");
    }
    for path in ["/api/auth/password", "/api/auth/sessions/revoke-all"] {
        assert_eq!(
            post(&rig.r, Some(agent), path, json!({})).await.0,
            404,
            "{path}"
        );
    }
}

#[tokio::test]
async fn changing_the_password_over_http() {
    let rig = rig(false);
    let (t1, t2) = (token(&rig.r, "ada").await, token(&rig.r, "ada").await);
    let new = "a brand new passphrase";
    let (s, b) = post(
        &rig.r,
        Some(&t1),
        "/api/auth/password",
        json!({"current_password": "nope nope nope", "new_password": new}),
    )
    .await;
    assert_eq!(
        (s, b["code"].clone()),
        (401, json!("invalid_password")),
        "{b}"
    );
    let (s, _) = post(
        &rig.r,
        Some(&t1),
        "/api/auth/password",
        json!({"current_password": PW, "new_password": "short"}),
    )
    .await;
    assert_eq!(s, 400);
    let (s, b) = post(
        &rig.r,
        Some(&t1),
        "/api/auth/password",
        json!({"current_password": PW, "new_password": new}),
    )
    .await;
    assert_eq!((s, b["changed"].clone()), (200, json!(true)), "{b}");
    assert_eq!(
        post(&rig.r, Some(&t2), "/api/party/list", json!({}))
            .await
            .0,
        401,
        "the other session ended"
    );
    assert_eq!(
        post(&rig.r, Some(&t1), "/api/party/list", json!({}))
            .await
            .0,
        200
    );
    let (s, _) = post(
        &rig.r,
        None,
        "/api/auth/login",
        json!({"username": "ada", "password": PW}),
    )
    .await;
    assert_eq!(s, 401);
    let (s, _) = post(
        &rig.r,
        None,
        "/api/auth/login",
        json!({"username": "ada", "password": new}),
    )
    .await;
    assert_eq!(s, 200);
    // Five wrong guesses lock the route like the sign-in.
    for _ in 0..5 {
        post(
            &rig.r,
            Some(&t1),
            "/api/auth/password",
            json!({"current_password": "wrong wrong wrong", "new_password": "another passphrase!"}),
        )
        .await;
    }
    let (s, _) = post(
        &rig.r,
        Some(&t1),
        "/api/auth/password",
        json!({"current_password": new, "new_password": "another passphrase!"}),
    )
    .await;
    assert_eq!(s, 429);
}

#[tokio::test]
async fn turning_two_factor_off_signs_out_the_other_sessions_over_http() {
    let rig = rig(false);
    let (secret, _) = enrol(&rig, "ada").await;
    let sign_in = |at: u64| {
        let rig = &rig;
        let secret = secret.clone();
        async move {
            rig.auth.set_clock_for_test(at);
            let b = first_step(&rig.r, "ada").await;
            let (s, b) = post(
                &rig.r,
                None,
                "/api/auth/login/mfa",
                json!({"mfa_token": b["mfa_token"], "code": code(&secret, at)}),
            )
            .await;
            assert_eq!(s, 200, "{b}");
            b["token"].as_str().unwrap().to_string()
        }
    };
    let (mine, other) = (sign_in(T0 + 60).await, sign_in(T0 + 120).await);
    rig.auth.set_clock_for_test(T0 + 180);
    let (s, b) = post(
        &rig.r,
        Some(&mine),
        "/api/auth/mfa/disable",
        json!({"password": PW, "code": code(&secret, T0 + 180)}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(
        post(&rig.r, Some(&other), "/api/party/list", json!({}))
            .await
            .0,
        401
    );
    assert_eq!(
        post(&rig.r, Some(&mine), "/api/party/list", json!({}))
            .await
            .0,
        200
    );
}

#[tokio::test]
async fn administrators_manage_accounts_and_it_is_audited() {
    let rig = rig(false);
    let root = token(&rig.r, "root").await;
    let (ada, bob) = (token(&rig.r, "ada").await, token(&rig.r, "bob").await);
    // Only an administrator sees the list; nobody sees a hash.
    assert_eq!(get(&rig.r, &ada, "/api/auth/users").await.0, 404);
    let (s, b) = get(&rig.r, &root, "/api/auth/users").await;
    assert_eq!(s, 200, "{b}");
    let names: Vec<_> = b["users"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["ada", "bob", "root"]);
    assert!(!b.to_string().contains("argon2"));
    assert_eq!(
        post(&rig.r, Some(&ada), "/api/auth/users/bob/disable", json!({}))
            .await
            .0,
        404
    );

    // Disable: its sessions stop at once, it cannot sign in, and it is in the log.
    let (s, b) = post(
        &rig.r,
        Some(&root),
        "/api/auth/users/ada/disable",
        json!({}),
    )
    .await;
    assert_eq!((s, b["disabled"].clone()), (200, json!(true)), "{b}");
    assert_eq!(
        post(&rig.r, Some(&ada), "/api/party/list", json!({}))
            .await
            .0,
        401
    );
    assert_eq!(
        post(
            &rig.r,
            None,
            "/api/auth/login",
            json!({"username": "ada", "password": PW})
        )
        .await
        .0,
        401
    );
    let log = events(&rig, &root, "user_disable").await;
    assert_eq!(
        (log.len(), log[0]["actor"].clone(), log[0]["user"].clone()),
        (1, json!("root"), json!("ada"))
    );
    assert_eq!(events(&rig, &root, "login_refused").await.len(), 1);
    post(&rig.r, Some(&root), "/api/auth/users/ada/enable", json!({})).await;
    assert_eq!(
        post(
            &rig.r,
            None,
            "/api/auth/login",
            json!({"username": "ada", "password": PW})
        )
        .await
        .0,
        200
    );
    assert_eq!(events(&rig, &root, "user_enable").await.len(), 1);
    // Not oneself, not an unknown account.
    assert_eq!(
        post(
            &rig.r,
            Some(&root),
            "/api/auth/users/root/disable",
            json!({})
        )
        .await
        .0,
        400
    );
    assert_eq!(
        post(
            &rig.r,
            Some(&root),
            "/api/auth/users/nobody/disable",
            json!({})
        )
        .await
        .0,
        400
    );

    // A password reset: the old password and sessions are gone; the temporary one only changes.
    let temp = "a temporary passphrase";
    let (s, b) = post(
        &rig.r,
        Some(&root),
        "/api/auth/users/bob/reset-password",
        json!({"password": temp}),
    )
    .await;
    assert_eq!(
        (s, b["must_change_password"].clone()),
        (200, json!(true)),
        "{b}"
    );
    assert_eq!(
        post(&rig.r, Some(&bob), "/api/party/list", json!({}))
            .await
            .0,
        401
    );
    let (s, b) = post(
        &rig.r,
        None,
        "/api/auth/login",
        json!({"username": "bob", "password": temp}),
    )
    .await;
    assert_eq!(
        (s, b["password_change_required"].clone()),
        (200, json!(true)),
        "{b}"
    );
    let limited = b["token"].as_str().unwrap().to_string();
    let (s, b) = post(&rig.r, Some(&limited), "/api/party/list", json!({})).await;
    assert_eq!(s, 403, "{b}");
    let (s, b) = post(
        &rig.r,
        Some(&limited),
        "/api/auth/password",
        json!({"current_password": temp, "new_password": "bob chose this one"}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(
        post(&rig.r, Some(&limited), "/api/party/list", json!({}))
            .await
            .0,
        200,
        "the limit is lifted"
    );
    assert!(!events(&rig, &root, "password_reset").await.is_empty());
    let all = json!(events(&rig, &root, "password_reset").await).to_string();
    assert!(!all.contains(temp));
}

#[tokio::test]
async fn a_change_to_the_security_settings_is_in_the_security_log() {
    let rig = rig(false);
    let root = token(&rig.r, "root").await;
    let (s, b) = post(
        &rig.r,
        Some(&root),
        "/api/setting/update",
        json!({"value": json!({"security": {"require_mfa_for_admins": false, "require_mfa_for_all": true}}).to_string()}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    let log = events(&rig, &root, "settings_change").await;
    assert_eq!(log.len(), 2, "{log:?}");
    assert!(log.iter().all(|e| e["actor"] == "root"));
    let details: Vec<_> = log.iter().map(|e| e["detail"].as_str().unwrap()).collect();
    assert!(details.contains(&"security.require_mfa_for_all: (unset) -> true"));
}
