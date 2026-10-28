//! The mobile class over HTTP (the D64 amendment), against the real router, the real `Auth` and the
//! real commands of the `device` model: an app registers, a person signs in on the browser pages
//! and hands over some groups, the app calls `/api` and is refused on `/mcp` and on every account
//! route, the person (and nobody else) sees and ends the device, and what ends a person's
//! credentials ends their devices.

use super::*;
use crate::mfa_tests::post;
use obop_auth::{Auth, AuthConfig};
use obop_commands::Authenticator;
use obop_storage::testing::{Flaky, Mode};

const APP: &str = "org.obop.mileage:/oauth";
const API: &str = "http://inst.test/api";

struct Rig {
    r: axum::Router,
    auth: Arc<Auth>,
    app: App,
}

fn rig_on(app: App) -> Rig {
    app.seed_demo();
    let auth = Arc::new(Auth::with_test_config(
        app.store.backend(),
        AuthConfig {
            default_require_admins: false,
            ..AuthConfig::insecure_dev()
        },
    ));
    auth.set_clock_for_test(T0);
    let as_auth: Arc<dyn obop_commands::Authenticator> = auth.clone();
    // The `device` commands, as `obop serve` binds them.
    app.register_accounts(as_auth);
    for (name, groups) in [
        ("root", vec!["platform.admin"]),
        (
            "eve",
            vec!["base.user", "sales.user", "crm.user", "platform.admin"],
        ),
        ("bob", vec!["base.user", "sales.user"]),
        ("dana", vec!["base.user", "api.device_admin"]),
    ] {
        let groups: Vec<String> = groups.into_iter().map(String::from).collect();
        auth.create_user(name, PW, &groups).unwrap();
    }
    let r = obop_http::router(
        obop_http::HttpState::new(app.dispatcher.clone(), auth.clone()),
        None,
    );
    Rig { r, auth, app }
}

fn rig() -> Rig {
    rig_on(App::new())
}

async fn mobile_client(r: &axum::Router) -> String {
    let c = call(
        r,
        "POST",
        "/oauth/register",
        &[],
        Some((
            "application/json",
            json!({
                "client_name": "Mileage",
                "obop_client_class": "mobile",
                "redirect_uris": [APP, "http://127.0.0.1/callback"],
                "token_endpoint_auth_method": "none",
            })
            .to_string(),
        )),
    )
    .await;
    assert_eq!(c.status, 201, "{}", c.text());
    assert_eq!(c.json()["obop_client_class"], "mobile");
    c.json()["client_id"].as_str().unwrap().to_string()
}

fn url(client: &str) -> String {
    authorize_url(client, APP, &[("resource", API)])
}

/// The browser part: sign in as `user`, hand over `groups`, and return the code the app gets.
async fn browser(r: &axum::Router, user: &str, client: &str, groups: &[&str]) -> String {
    let resp = sign_in_and_allow(r, user, &url(client), groups, "30").await;
    let loc = location(&resp);
    assert!(loc.starts_with(&format!("{APP}?code=oac_")), "{loc}");
    assert_eq!(param(&loc, "state"), "xyz");
    param(&loc, "code")
}

async fn token_request(r: &axum::Router, client: &str, code: &str, extra: &[(&str, &str)]) -> Resp {
    let mut pairs = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("client_id", client),
        ("redirect_uri", APP),
        ("code_verifier", VERIFIER),
        ("resource", API),
    ];
    pairs.extend_from_slice(extra);
    token(r, &pairs).await
}

/// A signed-in app: the access and the refresh token.
async fn app_signs_in(
    r: &axum::Router,
    user: &str,
    client: &str,
    groups: &[&str],
    device: &str,
) -> (String, String) {
    let code = browser(r, user, client, groups).await;
    let t = token_request(
        r,
        client,
        &code,
        &[
            ("device_name", device),
            ("device_platform", "android"),
            ("device_app_version", "1.4.2"),
        ],
    )
    .await;
    assert_eq!(t.status, 200, "{}", t.text());
    let j = t.json();
    (
        j["access_token"].as_str().unwrap().to_string(),
        j["refresh_token"].as_str().unwrap().to_string(),
    )
}

/// A JSON call with the host header the instance is reached by (a token for `/api` is bound to its
/// address). `bearer` is a token, with or without the word `Bearer`.
async fn api(
    r: &axum::Router,
    method: &str,
    bearer: Option<&str>,
    path: &str,
    body: J,
) -> (u16, J) {
    let auth = bearer.map(|t| format!("Bearer {}", t.trim_start_matches("Bearer ")));
    let mut headers: Vec<(&str, &str)> = Vec::new();
    if let Some(a) = &auth {
        headers.push(("authorization", a));
    }
    let resp = call(
        r,
        method,
        path,
        &headers,
        Some(("application/json", body.to_string())),
    )
    .await;
    let json = serde_json::from_slice(&resp.body).unwrap_or(J::Null);
    (resp.status, json)
}

async fn command(r: &axum::Router, bearer: &str, id: &str, body: J) -> (u16, J) {
    let (model, cmd) = id.split_once('.').unwrap();
    api(
        r,
        "POST",
        Some(bearer),
        &format!("/api/{model}/{cmd}"),
        body,
    )
    .await
}

async fn session(r: &axum::Router, user: &str) -> String {
    crate::mfa_tests::token(r, user).await
}

fn rows(v: &J) -> Vec<J> {
    v["value"].as_array().cloned().unwrap_or_default()
}

#[tokio::test]
async fn a_mobile_app_signs_in_on_the_browser_pages_and_uses_the_api_but_not_mcp_or_the_account() {
    let rig = rig();
    let r = &rig.r;
    let c = mobile_client(r).await;

    // The pages: the consent page says it is a mobile app, where the phone hands the result, and
    // offers no administrative group; its policy lets the form go to the scheme and no further.
    let (_, b) = start(r, &url(&c)).await;
    let consent = b
        .post(
            r,
            "/oauth/authorize/login",
            &[("username", "eve"), ("password", PW)],
        )
        .await;
    let html = consent.text();
    assert_eq!(consent.status, 200, "{html}");
    assert!(
        html.contains("mobile app") && html.contains("org.obop.mileage"),
        "{html}"
    );
    assert!(html.contains("Customize, Security, Devices"));
    for g in ["base.user", "sales.user", "crm.user"] {
        assert!(html.contains(&format!("value=\"{g}\"")), "{g}");
    }
    assert!(
        !html.contains("platform.admin"),
        "eve is an administrator, the app is not offered it"
    );
    let csp = consent.header("content-security-policy").to_string();
    assert!(
        csp.contains("form-action 'self' org.obop.mileage:"),
        "{csp}"
    );
    // A post that names the administrator group anyway is refused.
    let denied = b
        .post(
            r,
            "/oauth/authorize/decision",
            &[
                ("decision", "allow"),
                ("days", "30"),
                ("group", "platform.admin"),
            ],
        )
        .await;
    assert_eq!(denied.status, 200);
    assert!(
        denied.text().contains("administrative groups"),
        "{}",
        denied.text()
    );

    // The answer, and the token request with what the app says about its device.
    let (access, refresh_token) =
        app_signs_in(r, "eve", &c, &["base.user", "sales.user"], "Pixel 8").await;

    // /api works, within the groups handed over.
    let bearer = format!("Bearer {access}");
    let (s, v) = command(r, &bearer, "party.list", json!({})).await;
    assert_eq!(s, 200, "{v}");
    assert!(!rows(&v).is_empty());
    let (s, _) = command(r, &bearer, "product.list", json!({})).await;
    assert_ne!(
        s, 200,
        "eve holds no product group, and the app none she did not hand over"
    );
    let (s, _) = api(r, "GET", Some(&access), "/api/meta", json!({})).await;
    assert_eq!(s, 200);
    let (s, _) = api(r, "GET", Some(&access), "/api/openapi.json", json!({})).await;
    assert_eq!(s, 200);
    // /mcp does not take it.
    let init = mcp(
        r,
        &bearer,
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}),
    )
    .await;
    assert_eq!(init.status, 401, "{}", init.text());
    // Neither do the routes that manage the account, the approvals or the blobs.
    for (method, path, body) in [
        (
            "POST",
            "/api/auth/agent-token",
            json!({"scopes": ["base.user"]}),
        ),
        ("GET", "/api/auth/oauth/grants", json!({})),
        ("POST", "/api/auth/oauth/grants/og_x/revoke", json!({})),
        ("GET", "/api/auth/sessions", json!({})),
        ("POST", "/api/auth/sessions/revoke-all", json!({})),
        (
            "POST",
            "/api/auth/password",
            json!({"current_password": PW, "new_password": "another long password"}),
        ),
        ("GET", "/api/auth/mfa/status", json!({})),
        ("POST", "/api/auth/mfa/begin", json!({})),
        ("GET", "/api/auth/passkeys", json!({})),
        ("POST", "/api/approve/anything", json!({})),
        ("GET", "/api/auth/groups", json!({})),
        (
            "POST",
            "/api/auth/users",
            json!({"name": "mallory", "password": "a long password!"}),
        ),
        ("POST", "/api/blobs", json!({})),
    ] {
        let (s, _) = api(r, method, Some(&access), path, body).await;
        assert!(
            s == 401 || s == 404 || s == 405 || s == 415,
            "{method} {path}: {s}"
        );
    }
    assert!(rig.auth.user_count() > 0);

    // The device of the sign-in, as eve sees it: her own, without the push token and the family.
    let (s, v) = command(r, &bearer, "device.list", json!({})).await;
    assert_eq!(s, 200, "{v}");
    let list = rows(&v);
    assert_eq!(list.len(), 1, "{v}");
    // The query the Devices table of the back office makes.
    let (s, v2) = command(
        r,
        &bearer,
        "device.search",
        json!({"order": [{"field": "registered_at", "dir": "desc"}], "limit": 200}),
    )
    .await;
    assert_eq!(s, 200, "{v2}");
    assert_eq!(v2["value"]["total"], 1);
    let d = &list[0];
    assert_eq!(d["name"], "Pixel 8");
    assert_eq!(
        (d["platform"].as_str(), d["app_version"].as_str()),
        (Some("android"), Some("1.4.2"))
    );
    assert_eq!(
        (
            d["state"].as_str(),
            d["client_name"].as_str(),
            d["user"].as_str()
        ),
        (Some("active"), Some("Mileage"), Some("eve"))
    );
    assert!(
        d.get("push_token").is_none() && d.get("grant").is_none(),
        "{d}"
    );
    let denied: Vec<&str> = v["denied"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(J::as_str)
        .collect();
    assert!(denied.contains(&"device.grant"), "{denied:?}");
    let id = d["id"].as_str().unwrap().to_string();
    // A push token, if one were ever stored, is a secret: no surface returns it to anyone, the
    // owner, the device administrator and the instance administrator included.
    let mut secret = obop_storage::Fields::new();
    secret.insert("push_token".into(), json!("fcm-token-that-must-not-leave"));
    rig.app
        .store
        .backend()
        .update("device", &id, &secret)
        .unwrap();
    for who in [
        access.clone(),
        session(r, "eve").await,
        session(r, "dana").await,
        session(r, "root").await,
    ] {
        for (cmd, body) in [
            ("device.list", json!({})),
            ("device.get", json!({"id": id})),
            ("device.search", json!({})),
        ] {
            let (s, v) = command(r, &who, cmd, body).await;
            assert_eq!(s, 200, "{cmd}: {v}");
            assert!(
                !v.to_string().contains("fcm-token-that-must-not-leave"),
                "{cmd}: {v}"
            );
        }
    }
    let (s, v) = command(
        r,
        &bearer,
        "device.search",
        json!({"filter": [{"field": "push_token", "op": "eq", "value": "x"}]}),
    )
    .await;
    assert_eq!(s, 400, "a secret cannot be filtered on either: {v}");

    // Rename is `update` of the name and nothing else.
    for (body, why) in [
        (
            json!({"id": id, "name": "Work phone", "state": "revoked"}),
            "state",
        ),
        (json!({"id": id, "user": "bob"}), "user"),
        (
            json!({"id": id, "name": "x", "push_token": "t"}),
            "push token",
        ),
        (json!({"id": id, "name": ""}), "empty name"),
        (json!({"id": id, "name": "a\u{0}b"}), "control character"),
        (json!({"id": id, "name": "x".repeat(81)}), "long name"),
        (json!({"id": id}), "no name"),
        (
            json!({"id": id, "name": "n", "nonsense": 1}),
            "unknown field",
        ),
    ] {
        let (s, v) = command(r, &bearer, "device.update", body).await;
        assert_eq!(s, 400, "{why}: {v}");
    }
    let (s, v) = command(
        r,
        &bearer,
        "device.update",
        json!({"id": id, "name": "Work phone"}),
    )
    .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["value"]["name"], "Work phone");
    assert!(v["value"].get("push_token").is_none());
    let (_, v) = command(r, &bearer, "device.get", json!({"id": id})).await;
    assert_eq!(v["value"]["name"], "Work phone");
    let log = rig.app.store.backend().list("security_event").unwrap();
    let kinds = |k: &str| log.iter().filter(|(_, f)| f["kind"] == k).count();
    assert_eq!(
        (kinds("device_registered"), kinds("device_renamed")),
        (1, 1)
    );
    assert!(
        log.iter().any(|(_, f)| f["kind"] == "device_registered"
            && f["detail"].as_str().unwrap().contains("Pixel 8")
            && f["user"] == "eve"),
        "{log:?}"
    );

    // The person revokes the device, from the device itself: the very next call is refused.
    let (s, v) = command(r, &bearer, "device.revoke", json!({"id": id})).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["value"]["state"], "revoked");
    let (s, _) = command(r, &bearer, "party.list", json!({})).await;
    assert_eq!(s, 401, "the access token stopped at once");
    assert_eq!(refresh(r, &c, &refresh_token).await.status, 400);
    // Revoking again is refused with a reason, not repeated.
    let eve = session(r, "eve").await;
    let (s, v) = command(r, &eve, "device.revoke", json!({"id": id})).await;
    assert_eq!(s, 400, "{v}");
    assert!(
        v["error"].as_str().unwrap().contains("already revoked"),
        "{v}"
    );
    let (_, v) = command(r, &eve, "device.get", json!({"id": id})).await;
    assert_eq!(v["value"]["state"], "revoked");
    assert!(v["value"]["revoked_at"].as_str().is_some());
    let log = rig.app.store.backend().list("security_event").unwrap();
    assert!(log.iter().any(|(_, f)| f["kind"] == "device_revoked"
        && f["detail"].as_str().unwrap().contains("Work phone")));
}

#[tokio::test]
async fn nobody_reads_renames_or_revokes_the_device_of_somebody_else_without_the_group() {
    let rig = rig();
    let r = &rig.r;
    let c = mobile_client(r).await;
    let (eve_access, _) = app_signs_in(r, "eve", &c, &["base.user"], "Eve's phone").await;
    let eve = session(r, "eve").await;
    let (_, v) = command(r, &eve, "device.list", json!({})).await;
    let id = rows(&v)[0]["id"].as_str().unwrap().to_string();

    // Bob: with his browser session and with an app of his own.
    let bob_session = session(r, "bob").await;
    let (bob_access, _) = app_signs_in(r, "bob", &c, &["base.user"], "Bob's phone").await;
    for bearer in [&bob_session, &bob_access] {
        let (s, v) = command(r, bearer, "device.list", json!({})).await;
        assert_eq!(s, 200);
        let mine: Vec<String> = rows(&v)
            .iter()
            .map(|d| d["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(mine, ["Bob's phone"], "only his own");
        let (s, v) = command(
            r,
            bearer,
            "device.search",
            json!({"filter": [{"field": "user", "op": "eq", "value": "eve"}]}),
        )
        .await;
        assert_eq!(s, 200, "{v}");
        assert!(v["value"]["rows"].as_array().unwrap().is_empty());
        // The same answer for a device that does not exist and for one that is not his.
        let (s1, v1) = command(r, bearer, "device.get", json!({"id": id})).await;
        let (s2, v2) = command(r, bearer, "device.get", json!({"id": "no-such-device"})).await;
        assert_eq!(s1, s2);
        assert_eq!(
            v1["error"].as_str().unwrap().replace(&id, "X"),
            v2["error"].as_str().unwrap().replace("no-such-device", "X")
        );
        for (cmd, body) in [
            ("device.update", json!({"id": id, "name": "pwned"})),
            ("device.revoke", json!({"id": id})),
        ] {
            let (s, v) = command(r, bearer, cmd, body).await;
            assert_eq!(s, 400, "{cmd}: {v}");
            assert!(
                v["error"].as_str().unwrap().starts_with("no device"),
                "{cmd}: {v}"
            );
        }
        // There is no way to make, change or remove a device row by hand.
        for cmd in ["device.create", "device.delete", "device.bulk_create"] {
            let (s, _) = command(
                r,
                bearer,
                cmd,
                json!({"id": id, "user": "bob", "name": "x"}),
            )
            .await;
            assert_eq!(s, 404, "{cmd}");
        }
    }
    // Eve's device is untouched and her app still works.
    let (s, _) = command(r, &eve_access, "party.list", json!({})).await;
    assert_eq!(s, 200);
    let (_, v) = command(r, &eve, "device.get", json!({"id": id})).await;
    assert_eq!(
        (v["value"]["name"].as_str(), v["value"]["state"].as_str()),
        (Some("Eve's phone"), Some("active"))
    );

    // An agent token of eve's: it sees what she sees, and can only propose a revocation.
    let (s, a) = post(
        r,
        Some(&eve),
        "/api/auth/agent-token",
        json!({"scopes": ["base.user"], "label": "assistant"}),
    )
    .await;
    assert_eq!(s, 200, "{a}");
    let agent = a["token"].as_str().unwrap().to_string();
    let (s, v) = command(r, &agent, "device.list", json!({})).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(rows(&v).len(), 1);
    let (s, v) = command(r, &agent, "device.revoke", json!({"id": id})).await;
    assert_eq!(s, 202, "an agent proposes: {v}");
    let (_, v) = command(r, &eve, "device.get", json!({"id": id})).await;
    assert_eq!(v["value"]["state"], "active");
    // An agent of Bob's can neither see nor propose anything about it.
    let (_, a) = post(
        r,
        Some(&bob_session),
        "/api/auth/agent-token",
        json!({"scopes": ["base.user"]}),
    )
    .await;
    let bob_agent = a["token"].as_str().unwrap().to_string();
    let (s, v) = command(r, &bob_agent, "device.revoke", json!({"id": id})).await;
    assert_eq!(s, 400, "{v}");
    // An agent that does not hold base.user has no business with the account's devices? It
    // holds `*`, and the row access is what decides: an agent of nobody sees nothing.

    // The device administrator sees every device and can revoke any; a mobile token carries the
    // group only if the person handed it over.
    let dana = session(r, "dana").await;
    let (_, v) = command(r, &dana, "device.list", json!({})).await;
    let mut names: Vec<String> = rows(&v)
        .iter()
        .map(|d| d["name"].as_str().unwrap().to_string())
        .collect();
    names.sort();
    assert_eq!(names, ["Bob's phone", "Eve's phone"]);
    assert!(
        rows(&v)[0].get("grant").is_some(),
        "the administrator also sees the family id"
    );
    assert!(
        rows(&v)[0].get("push_token").is_none(),
        "but never the push token"
    );
    let (s, v) = command(r, &dana, "device.revoke", json!({"id": id})).await;
    assert_eq!(s, 200, "{v}");
    let (s, _) = command(r, &eve_access, "party.list", json!({})).await;
    assert_eq!(s, 401, "eve's app is out");
    let (s, _) = command(r, &bob_access, "party.list", json!({})).await;
    assert_eq!(s, 200, "bob's is not");
    // The instance administrator does too.
    let root = session(r, "root").await;
    let (_, v) = command(r, &root, "device.list", json!({})).await;
    assert_eq!(rows(&v).len(), 2);
}

#[tokio::test]
async fn a_new_password_and_an_offboarding_end_the_devices() {
    let rig = rig();
    let r = &rig.r;
    let c = mobile_client(r).await;
    let (a1, r1) = app_signs_in(r, "eve", &c, &["base.user"], "Phone 1").await;
    let (a2, r2) = app_signs_in(r, "eve", &c, &["base.user"], "Tablet").await;
    let (bob_access, bob_refresh) = app_signs_in(r, "bob", &c, &["base.user"], "Bob's phone").await;
    for a in [&a1, &a2, &bob_access] {
        assert_eq!(
            command(r, &format!("Bearer {a}"), "party.list", json!({}))
                .await
                .0,
            200
        );
    }
    // eve changes her password in the back office: her other sessions and her devices end.
    let eve = session(r, "eve").await;
    let (s, v) = post(
        r,
        Some(&eve),
        "/api/auth/password",
        json!({"current_password": PW, "new_password": "a brand new password"}),
    )
    .await;
    assert_eq!(s, 200, "{v}");
    for a in [&a1, &a2] {
        assert_eq!(
            command(r, &format!("Bearer {a}"), "party.list", json!({}))
                .await
                .0,
            401
        );
    }
    for rt in [&r1, &r2] {
        assert_eq!(refresh(r, &c, rt).await.status, 400);
    }
    assert_eq!(
        command(r, &format!("Bearer {bob_access}"), "party.list", json!({}))
            .await
            .0,
        200,
        "somebody else's device is not touched"
    );
    let (_, v) = command(r, &eve, "device.list", json!({})).await;
    let evs: Vec<J> = rows(&v)
        .into_iter()
        .filter(|d| d["user"] == "eve")
        .collect();
    assert_eq!(evs.len(), 2);
    assert!(evs.iter().all(|d| d["state"] == "revoked"), "{v}");
    let log = rig.app.store.backend().list("security_event").unwrap();
    let ended: Vec<String> = log
        .iter()
        .filter(|(_, f)| f["kind"] == "device_revoked" && f["user"] == "eve")
        .map(|(_, f)| f["detail"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ended.len(), 2, "{ended:?}");
    assert!(ended.iter().all(|d| d.contains("ended by password_change")));
    // The app signs in again with the new password and is a new device.
    let c2 = mobile_client(r).await;
    let code = {
        let (_, b) = start(r, &url(&c2)).await;
        b.post(
            r,
            "/oauth/authorize/login",
            &[("username", "eve"), ("password", "a brand new password")],
        )
        .await;
        let resp = b
            .post(
                r,
                "/oauth/authorize/decision",
                &[
                    ("decision", "allow"),
                    ("days", "30"),
                    ("group", "base.user"),
                ],
            )
            .await;
        param(&location(&resp), "code")
    };
    assert_eq!(token_request(r, &c2, &code, &[]).await.status, 200);

    // An offboarding ends them for good, and says how many.
    let done = rig.auth.operator_offboard("bob", "left").unwrap();
    assert_eq!(done["revoked_devices"], 1);
    assert_eq!(done["revoked_apps"], 1);
    assert_eq!(
        command(r, &format!("Bearer {bob_access}"), "party.list", json!({}))
            .await
            .0,
        401
    );
    assert_eq!(refresh(r, &c, &bob_refresh).await.status, 400);
    let root = session(r, "root").await;
    let (_, v) = command(
        r,
        &root,
        "device.search",
        json!({"filter": [{"field": "user", "op": "eq", "value": "bob"}]}),
    )
    .await;
    let bobs = v["value"]["rows"].as_array().unwrap().clone();
    assert_eq!(bobs.len(), 1);
    assert_eq!(bobs[0]["state"], "revoked");
    // Enabling the account later does not bring the device back.
    rig.auth.operator_set_disabled("bob", false).unwrap();
    assert_eq!(
        command(r, &format!("Bearer {bob_access}"), "party.list", json!({}))
            .await
            .0,
        401
    );
    assert_eq!(refresh(r, &c, &bob_refresh).await.status, 400);
}

#[tokio::test]
async fn signing_out_other_sessions_and_an_administrators_reset_end_the_devices_too() {
    let rig = rig();
    let r = &rig.r;
    let c = mobile_client(r).await;
    let (a1, _) = app_signs_in(r, "bob", &c, &["base.user"], "Phone").await;
    let bob = session(r, "bob").await;
    let _other = session(r, "bob").await;
    let (s, v) = post(
        r,
        Some(&bob),
        "/api/auth/sessions/revoke-all",
        json!({"keep_current": true}),
    )
    .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(
        command(r, &format!("Bearer {a1}"), "party.list", json!({}))
            .await
            .0,
        401
    );
    // And an administrator of the instance resetting the password of another person.
    let (a2, r2) = app_signs_in(r, "dana", &c, &["base.user"], "Dana's phone").await;
    let root = session(r, "root").await;
    let (s, v) = post(
        r,
        Some(&root),
        "/api/auth/users/dana/reset-password",
        json!({"password": "a temporary password"}),
    )
    .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(
        command(r, &format!("Bearer {a2}"), "party.list", json!({}))
            .await
            .0,
        401
    );
    assert_eq!(refresh(r, &c, &r2).await.status, 400);
}

#[tokio::test]
async fn a_device_read_the_store_refuses_is_a_503_and_never_a_200_or_a_401() {
    let (flaky, backend) = Flaky::memory();
    let rig = rig_on(App::with_store(obop_commands::Store::over(backend)));
    let r = &rig.r;
    let c = mobile_client(r).await;
    let (access, _) = app_signs_in(r, "eve", &c, &["base.user"], "Pixel").await;
    let bearer = format!("Bearer {access}");
    assert_eq!(command(r, &bearer, "party.list", json!({})).await.0, 200);
    flaky.refuse_model("device", Mode::Reads);
    let resp = call(
        r,
        "POST",
        "/api/party/list",
        &[
            ("authorization", bearer.as_str()),
            ("content-type", "application/json"),
        ],
        Some(("application/json", "{}".into())),
    )
    .await;
    assert_eq!(resp.status, 503, "{}", resp.text());
    assert!(resp.header("retry-after").parse::<u32>().is_ok());
    flaky.allow_models();
    assert_eq!(command(r, &bearer, "party.list", json!({})).await.0, 200);
    // The revocation of a device the store cannot write says 503 and is finished by asking again.
    let eve = session(r, "eve").await;
    let (_, v) = command(r, &eve, "device.list", json!({})).await;
    let id = rows(&v)[0]["id"].as_str().unwrap().to_string();
    flaky.refuse_model("device", Mode::Updates);
    let (s, v) = command(r, &eve, "device.revoke", json!({"id": id})).await;
    assert_eq!(s, 503, "{v}");
    flaky.allow_models();
    let (s, v) = command(r, &eve, "device.revoke", json!({"id": id})).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(command(r, &bearer, "party.list", json!({})).await.0, 401);
}

#[tokio::test]
async fn registering_a_mobile_client_over_http_says_its_class_and_refuses_what_rfc_8252_refuses() {
    let rig = rig();
    let r = &rig.r;
    let reg = |redirects: J, class: J| {
        let body = json!({"client_name": "Mileage", "redirect_uris": redirects, "obop_client_class": class});
        async move {
            call(
                r,
                "POST",
                "/oauth/register",
                &[],
                Some(("application/json", body.to_string())),
            )
            .await
        }
    };
    assert_eq!(reg(json!([APP]), json!("mobile")).await.status, 201);
    for bad in [
        "http://example.com/cb",
        "http://localhost:8080/cb",
        "claude://callback",
        "https://x.test/cb#f",
    ] {
        let resp = reg(json!([bad]), json!("mobile")).await;
        assert_eq!(resp.status, 400, "{bad}");
        assert_eq!(resp.json()["error"], "invalid_redirect_uri");
    }
    assert_eq!(
        reg(json!([APP]), json!("phone")).await.json()["error"],
        "invalid_client_metadata"
    );
    // The client that does not say is an MCP client, whose redirects are what they were.
    let plain = call(
        r,
        "POST",
        "/oauth/register",
        &[],
        Some((
            "application/json",
            json!({"redirect_uris": [REDIRECT]}).to_string(),
        )),
    )
    .await;
    assert_eq!(plain.status, 201);
    assert!(plain.json().get("obop_client_class").is_none());
    let scheme = call(
        r,
        "POST",
        "/oauth/register",
        &[],
        Some((
            "application/json",
            json!({"redirect_uris": [APP]}).to_string(),
        )),
    )
    .await;
    assert_eq!(scheme.status, 400);
    // The metadata document lists the classes.
    let m = call(
        r,
        "GET",
        "/.well-known/oauth-authorization-server",
        &[],
        None,
    )
    .await
    .json();
    assert_eq!(m["obop_client_classes_supported"], json!(["mcp", "mobile"]));
}

fn text_input(pairs: &[(&str, &str)]) -> obop_commands::Value {
    obop_commands::Value::Map(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), obop_commands::Value::text(v)))
            .collect(),
    )
}

/// The command line and MCP end in the same dispatcher as REST: a person lists, renames and
/// revokes their devices there too (the rules are not those of one surface).
#[tokio::test]
async fn the_cli_and_the_mcp_server_list_rename_and_revoke_devices_like_rest() {
    let rig = rig();
    let r = &rig.r;
    let c = mobile_client(r).await;
    let (access, _) = app_signs_in(r, "bob", &c, &["base.user"], "Bob's phone").await;
    let bob = rig.auth.resolve(&session(r, "bob").await).unwrap();

    // `obop call device list`
    let (v, denied) = crate::dispatch_call(&rig.app, &bob, "device.list", &text_input(&[]))
        .unwrap_or_else(|(line, _)| panic!("{line}"));
    let list = v.as_array().unwrap();
    assert_eq!(list.len(), 1, "{v}");
    assert!(list[0].get("push_token").is_none() && denied.contains(&"device.grant".to_string()));
    let id = list[0]["id"].as_str().unwrap().to_string();
    // A person without the group:
    let dana = rig.auth.resolve(&session(r, "dana").await).unwrap();
    assert!(
        crate::dispatch_call(&rig.app, &dana, "device.get", &text_input(&[("id", &id)])).is_ok()
    );
    let carol = obop_commands::Principal::user("carol", &["base.user"]);
    let (line, _) = crate::dispatch_call(
        &rig.app,
        &carol,
        "device.revoke",
        &text_input(&[("id", &id)]),
    )
    .unwrap_err();
    assert!(line.contains("no device"), "{line}");

    // MCP over stdio: the tools are generated from the same commands.
    let mut session = obop_mcp::Session::new();
    let ask = |session: &mut obop_mcp::Session,
               p: &obop_commands::Principal,
               name: &str,
               args: J| {
        let line = json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name": name, "arguments": args}}).to_string();
        let out = obop_mcp::handle(&rig.app.dispatcher, p, session, &line).unwrap();
        serde_json::from_str::<J>(&out).unwrap()
    };
    let listed = ask(&mut session, &bob, "device__list", json!({}));
    assert!(listed["result"]["isError"] != true, "{listed}");
    assert!(listed.to_string().contains("Bob's phone"));
    let renamed = ask(
        &mut session,
        &bob,
        "device__update",
        json!({"id": id, "name": "Bob's old phone"}),
    );
    assert!(renamed["result"]["isError"] != true, "{renamed}");
    // Nothing but the name: this is the same refusal as over REST.
    let bad = ask(
        &mut session,
        &bob,
        "device__update",
        json!({"id": id, "state": "revoked"}),
    );
    assert_eq!(bad["result"]["isError"], true, "{bad}");
    for gone in ["device__create", "device__delete"] {
        let r = ask(&mut session, &bob, gone, json!({"id": id}));
        assert!(
            r.to_string().contains("error") || r["result"]["isError"] == true,
            "{gone}: {r}"
        );
    }
    // The command line revokes it, and the app's token stops.
    let (v, _) = crate::dispatch_call(&rig.app, &bob, "device.revoke", &text_input(&[("id", &id)]))
        .unwrap_or_else(|(line, _)| panic!("{line}"));
    assert_eq!(v["state"], "revoked");
    assert_eq!(
        command(r, &access, "party.list", json!({})).await.0,
        401,
        "the access token stopped"
    );
}
