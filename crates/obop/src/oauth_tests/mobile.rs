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

/// A mobile client that declares the groups it will ask for (the three of the demo that a person
/// of the tests holds).
async fn mobile_client(r: &axum::Router) -> String {
    mobile_client_declaring(r, "base.user sales.user crm.user").await
}

async fn mobile_client_declaring(r: &axum::Router, scope: &str) -> String {
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
                "scope": scope,
                "token_endpoint_auth_method": "none",
            })
            .to_string(),
        )),
    )
    .await;
    assert_eq!(c.status, 201, "{}", c.text());
    assert_eq!(c.json()["obop_client_class"], "mobile");
    assert_eq!(c.json()["scope"], scope, "the declaration is given back");
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

/// A signed-in app: the access token, the refresh token and the id of its device.
async fn app_signs_in(
    r: &axum::Router,
    user: &str,
    client: &str,
    groups: &[&str],
    device: &str,
) -> (String, String, String) {
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
        j["device_id"].as_str().unwrap().to_string(),
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
    // offers only the groups the app declared that the person holds, none administrative and none
    // ticked; its policy lets the form go to the scheme and no further.
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
    assert!(html.contains("on this phone that registered the scheme"));
    assert!(html.contains("Customize, Security, Devices"));
    for g in ["base.user", "sales.user", "crm.user"] {
        assert!(html.contains(&format!("value=\"{g}\"")), "{g}");
        assert!(
            !html.contains(&format!("value=\"{g}\" checked")),
            "{g} is ticked although the app did not ask for it"
        );
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
    // What the app asks for in the request is ticked, and nothing else.
    let (_, b2) = start(
        r,
        &authorize_url(&c, APP, &[("resource", API), ("scope", "crm.user")]),
    )
    .await;
    let asked = b2
        .post(
            r,
            "/oauth/authorize/login",
            &[("username", "eve"), ("password", PW)],
        )
        .await
        .text();
    assert!(asked.contains("value=\"crm.user\" checked"), "{asked}");
    assert!(!asked.contains("value=\"base.user\" checked"), "{asked}");
    // A post that names the administrator group, or one that was never offered, is refused.
    for (group, why) in [
        ("platform.admin", "administrative groups"),
        ("product.user", "can only be given the groups it declared"),
    ] {
        let denied = b
            .post(
                r,
                "/oauth/authorize/decision",
                &[("decision", "allow"), ("days", "30"), ("group", group)],
            )
            .await;
        assert_eq!(denied.status, 200, "{group}");
        assert!(
            denied.header("location").is_empty(),
            "{group}: a refusal is not a redirect"
        );
        let text = denied.text();
        // The word is part of the sentence the page shows for that refusal (English).
        assert!(
            text.contains(why) || text.contains("not hold") || text.contains("administrative"),
            "{group}: {text}"
        );
    }

    // The answer, and the token request with what the app says about its device.
    let (access, refresh_token, id) =
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
    // Neither do the routes that manage the account, the approvals or the blobs: each answers
    // 401 to this token (the account routes) or says there is nothing there (the rest), and none
    // of them did anything.
    let users_before = rig.auth.user_count();
    let mut seen = Vec::new();
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
        seen.push((method, path, s));
    }
    assert_eq!(
        seen.iter().map(|(_, _, s)| *s).collect::<Vec<_>>(),
        EXPECTED_ACCOUNT_ROUTES,
        "{seen:?}"
    );
    assert_eq!(rig.auth.user_count(), users_before, "no account was made");
    // And the same routes take the session of the person, so the refusals above are about the
    // token and not about the request.
    let eve = session(r, "eve").await;
    let (s, _) = api(r, "GET", Some(&eve), "/api/auth/sessions", json!({})).await;
    assert_eq!(s, 200);

    // The app sees its own device by the id it was given with its tokens, and lists nothing.
    let (s, v) = command(r, &bearer, "device.get", json!({"id": id})).await;
    assert_eq!(s, 200, "{v}");
    let d = &v["value"];
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
    assert!(
        denied.contains(&"device.grant"),
        "the app is told what it may not see: {denied:?}"
    );
    for cmd in ["device.list", "device.search"] {
        let (s, v) = command(r, &bearer, cmd, json!({})).await;
        assert_eq!(s, 400, "{cmd}: {v}");
        assert!(
            v["error"].as_str().unwrap().contains("cannot be listed"),
            "{cmd}: {v}"
        );
    }
    // Eve at her keyboard sees the same device in the list, as the Devices table queries it.
    let (s, v) = command(r, &eve, "device.list", json!({})).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(rows(&v).len(), 1, "{v}");
    assert_eq!(rows(&v)[0]["id"], id.as_str());
    let (s, v2) = command(
        r,
        &eve,
        "device.search",
        json!({"order": [{"field": "registered_at", "dir": "desc"}], "limit": 200}),
    )
    .await;
    assert_eq!(s, 200, "{v2}");
    assert_eq!(v2["value"]["total"], 1);
    // A push token, if one were ever stored, is a secret: no surface returns it to anyone, the
    // owner, the app, the device administrator and the instance administrator included.
    let mut secret = obop_storage::Fields::new();
    secret.insert("push_token".into(), json!("fcm-token-that-must-not-leave"));
    rig.app
        .store
        .backend()
        .update("device", &id, &secret)
        .unwrap();
    let (dana, root) = (session(r, "dana").await, session(r, "root").await);
    for who in [&access, &eve, &dana, &root] {
        let mut cmds = vec![("device.get", json!({"id": id}))];
        if who != &access {
            cmds.push(("device.list", json!({})));
            cmds.push(("device.search", json!({})));
        }
        for (cmd, body) in cmds {
            let (s, v) = command(r, who, cmd, body).await;
            assert_eq!(s, 200, "{cmd}: {v}");
            assert!(
                !v.to_string().contains("fcm-token-that-must-not-leave"),
                "{cmd}: {v}"
            );
        }
    }
    let (s, v) = command(
        r,
        &eve,
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
        // Nothing visible is left once what hides text is taken out.
        (json!({"id": id, "name": "a".repeat(81)}), "too long"),
        (json!({"id": id, "name": "\u{0}"}), "control character"),
        (json!({"id": id, "name": "\u{202e}"}), "text direction"),
        (
            json!({"id": id, "name": "\u{200b}\u{2800}"}),
            "zero width and blank",
        ),
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
    // What hides text is taken out of a name, and the joiners of an emoji family stay.
    let (s, v) = command(
        r,
        &bearer,
        "device.update",
        json!({"id": id, "name": "Fam\u{200b}ily \u{202e}\u{1F468}\u{200d}\u{1F469}\u{200d}\u{1F467}"}),
    )
    .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(
        v["value"]["name"],
        "Family \u{1F468}\u{200d}\u{1F469}\u{200d}\u{1F467}"
    );
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
        (1, 2)
    );
    assert!(
        log.iter().any(|(_, f)| f["kind"] == "device_registered"
            && f["detail"].as_str().unwrap().contains("Pixel 8")
            && f["user"] == "eve"),
        "{log:?}"
    );

    // The app signs itself out: the very next call is refused.
    let (s, v) = command(r, &bearer, "device.revoke", json!({"id": id})).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["value"]["state"], "revoked");
    let (s, _) = command(r, &bearer, "party.list", json!({})).await;
    assert_eq!(s, 401, "the access token stopped at once");
    assert_eq!(refresh(r, &c, &refresh_token).await.status, 400);
    // Revoking again is refused with a reason, not repeated.
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
    assert!(
        log.iter().any(|(_, f)| f["kind"] == "device_revoked"
            && f["user"] == "eve"
            && f["actor"] == "eve"
            && f["detail"].as_str().unwrap().contains("Work phone")),
        "{log:?}"
    );
}

/// The status each account route answers to the access token of a mobile app, in the order of the
/// table of the test. The routes that refuse every token bound to a resource (the agent token,
/// the connected apps, the approval of a proposal, the blobs) say 401; the routes of the person's
/// own account (sessions, password, second factor, passkeys, the people of the instance) find
/// nobody to manage for a token that is not the person at the keyboard and say 404, as they do
/// for a route that is not there.
const EXPECTED_ACCOUNT_ROUTES: [u16; 13] = [
    401, 401, 401, 404, 404, 404, 404, 404, 404, 401, 404, 404, 401,
];

/// What a call of a device command is for the matrix below: the body each command takes.
async fn device_call(r: &axum::Router, bearer: &str, cmd: &str, id: &str) -> (u16, J) {
    let body = match cmd {
        "device.update" => json!({"id": id, "name": "renamed by the test"}),
        "device.list" | "device.search" => json!({}),
        _ => json!({"id": id}),
    };
    command(r, bearer, cmd, body).await
}

#[tokio::test]
async fn the_device_commands_follow_one_rule_for_every_kind_of_caller() {
    let rig = rig();
    let r = &rig.r;
    // A person who is not an administrator of anything, with two devices.
    rig.auth
        .create_user(
            "fay",
            PW,
            &["base.user".to_string(), "sales.user".to_string()],
        )
        .unwrap();
    let c = mobile_client_declaring(r, "base.user sales.user").await;
    let (fay_app, _, e1) = app_signs_in(r, "fay", &c, &["base.user"], "Fay's phone").await;
    let (_, _, e2) = app_signs_in(r, "fay", &c, &["base.user"], "Fay's tablet").await;
    let (bob_app, _, b1) = app_signs_in(r, "bob", &c, &["base.user"], "Bob's phone").await;
    let (fay, bob) = (session(r, "fay").await, session(r, "bob").await);
    let (dana, root) = (session(r, "dana").await, session(r, "root").await);
    let agent_of = |who: &str| {
        let who = who.to_string();
        async move {
            let s = session(r, &who).await;
            let (st, a) = post(
                r,
                Some(&s),
                "/api/auth/agent-token",
                json!({"scopes": ["base.user"], "label": "assistant"}),
            )
            .await;
            assert_eq!(st, 200, "{a}");
            a["token"].as_str().unwrap().to_string()
        }
    };
    let (fay_agent, bob_agent) = (agent_of("fay").await, agent_of("bob").await);
    let fay_app = format!("Bearer {fay_app}");
    let bob_app = format!("Bearer {bob_app}");
    let (e1, e2, b1) = (e1.as_str(), e2.as_str(), b1.as_str());

    // What a device that is not yours is, to the caller: the same answer as one that is not there.
    let ghost = "no-such-device";
    // (who, bearer, [(command, device, status)]): 200 allowed, 400 refused as "no device" or as a
    // list that is not for this token, 202 a proposal.
    type Row<'a> = (&'a str, &'a String, Vec<(&'a str, &'a str, u16)>);
    let cases: Vec<Row> = vec![
        (
            "fay at the keyboard: both of hers, none of bob's",
            &fay,
            vec![
                ("device.get", e1, 200),
                ("device.get", e2, 200),
                ("device.get", b1, 400),
                ("device.update", e1, 200),
                ("device.update", e2, 200),
                ("device.update", b1, 400),
                ("device.list", "", 200),
                ("device.search", "", 200),
            ],
        ),
        (
            "fay's app: its own device and no other, and no list",
            &fay_app,
            vec![
                ("device.get", e1, 200),
                ("device.get", e2, 400),
                ("device.get", b1, 400),
                ("device.update", e1, 200),
                ("device.update", e2, 400),
                ("device.update", b1, 400),
                ("device.list", "", 400),
                ("device.search", "", 400),
            ],
        ),
        (
            "bob at the keyboard",
            &bob,
            vec![
                ("device.get", b1, 200),
                ("device.get", e1, 400),
                ("device.update", b1, 200),
                ("device.update", e1, 400),
                ("device.list", "", 200),
                ("device.search", "", 200),
            ],
        ),
        (
            "bob's app",
            &bob_app,
            vec![
                ("device.get", b1, 200),
                ("device.get", e1, 400),
                ("device.get", e2, 400),
                ("device.update", b1, 200),
                ("device.update", e1, 400),
                ("device.list", "", 400),
                ("device.search", "", 400),
            ],
        ),
        (
            "fay's assistant: it reads and renames nothing, it can only propose",
            &fay_agent,
            vec![
                ("device.get", e1, 400),
                ("device.update", e1, 400),
                ("device.update", b1, 400),
                ("device.list", "", 400),
                ("device.search", "", 400),
            ],
        ),
        (
            "bob's assistant",
            &bob_agent,
            vec![
                ("device.get", e1, 400),
                ("device.get", b1, 400),
                ("device.list", "", 400),
            ],
        ),
        (
            "the device administrator: reads everything, renames nobody's but her own device",
            &dana,
            vec![
                ("device.get", e1, 200),
                ("device.get", e2, 200),
                ("device.get", b1, 200),
                ("device.update", e1, 400),
                ("device.update", b1, 400),
                ("device.list", "", 200),
                ("device.search", "", 200),
            ],
        ),
        (
            "the instance administrator",
            &root,
            vec![
                ("device.get", e1, 200),
                ("device.get", b1, 200),
                ("device.update", e1, 400),
                ("device.list", "", 200),
                ("device.search", "", 200),
            ],
        ),
    ];
    for (who, bearer, calls) in &cases {
        let (_, baseline) = device_call(r, bearer, "device.get", ghost).await;
        for (cmd, id, want) in calls {
            let (s, v) = device_call(r, bearer, cmd, id).await;
            assert_eq!(s, *want, "{who}: {cmd} {id}: {v}");
            if *want == 400 && !id.is_empty() {
                // Not "forbidden": the same words as for a device that is not there.
                assert_eq!(
                    v["error"].as_str().unwrap().replace(*id, "X"),
                    baseline["error"].as_str().unwrap_or("").replace(ghost, "X"),
                    "{who}: {cmd}: the answer tells something about a device that is not theirs"
                );
            }
        }
    }
    // What the lists hold.
    let ids = |v: &J| {
        let mut n: Vec<String> = rows(v)
            .iter()
            .map(|d| d["id"].as_str().unwrap().to_string())
            .collect();
        n.sort();
        n
    };
    let sorted = |a: &[&str]| {
        let mut n: Vec<String> = a.iter().map(|s| s.to_string()).collect();
        n.sort();
        n
    };
    let (_, v) = command(r, &fay, "device.list", json!({})).await;
    assert_eq!(ids(&v), sorted(&[e1, e2]));
    let (_, v) = command(r, &bob, "device.list", json!({})).await;
    assert_eq!(ids(&v), sorted(&[b1]));
    let (_, v) = command(
        r,
        &bob,
        "device.search",
        json!({"filter": [{"field": "user", "op": "eq", "value": "fay"}]}),
    )
    .await;
    assert!(v["value"]["rows"].as_array().unwrap().is_empty());
    for admin in [&dana, &root] {
        let (_, v) = command(r, admin, "device.list", json!({})).await;
        assert_eq!(ids(&v), sorted(&[e1, e2, b1]));
        assert!(
            rows(&v)[0].get("push_token").is_none(),
            "never the push token"
        );
    }
    let (_, v) = command(r, &dana, "device.list", json!({})).await;
    assert!(
        rows(&v)[0].get("grant").is_some(),
        "the device administrator also sees the family id"
    );
    // The names changed above are the ones the callers were allowed to change, and no others.
    let (_, v) = command(r, &dana, "device.get", json!({"id": e1})).await;
    assert_eq!(
        v["value"]["name"], "renamed by the test",
        "fay's app renamed its own"
    );
    let (_, v) = command(r, &dana, "device.get", json!({"id": e2})).await;
    assert_eq!(
        v["value"]["name"], "renamed by the test",
        "fay renamed her other one"
    );
    // There is no way to make, change or remove a device row by hand.
    for bearer in [&fay, &fay_app, &dana, &root, &fay_agent] {
        for cmd in ["device.create", "device.delete", "device.bulk_create"] {
            let (s, _) = command(
                r,
                bearer,
                cmd,
                json!({"id": e1, "user": "bob", "name": "x"}),
            )
            .await;
            assert_eq!(s, 404, "{cmd}");
        }
    }
    // No token at all.
    let (s, _) = api(r, "POST", None, "/api/device/list", json!({})).await;
    assert_eq!(s, 401);

    // Ending a device, last, because it changes the state of the devices. Nobody ends what is not
    // theirs: another person's session and app, an app's other device, and the assistant of
    // another person are all told "no device".
    for (who, bearer, id) in [
        ("bob's assistant", &bob_agent, e1),
        ("bob's app", &bob_app, e1),
        ("bob at the keyboard", &bob, e1),
        ("fay's app, the other device", &fay_app, e2),
        ("fay's app, bob's device", &fay_app, b1),
    ] {
        let (s, v) = command(r, bearer, "device.revoke", json!({"id": id})).await;
        assert_eq!(s, 400, "{who}: {v}");
        assert!(
            v["error"].as_str().unwrap().starts_with("no device"),
            "{who}: {v}"
        );
    }
    for id in [e1, e2, b1] {
        let (_, v) = command(r, &root, "device.get", json!({"id": id})).await;
        assert_eq!(
            v["value"]["state"], "active",
            "{id}: nothing above ended anything"
        );
    }
    // Fay's assistant proposes ending one; nothing happens until she approves it, and the log then
    // names her as the one who did it and the assistant as the one who proposed it.
    let (s, v) = command(r, &fay_agent, "device.revoke", json!({"id": e2})).await;
    assert_eq!(
        (s, v["approval_required"].clone()),
        (202, json!(true)),
        "{v}"
    );
    let plan = v["plan_id"].as_str().unwrap().to_string();
    let (_, v) = command(r, &fay, "device.get", json!({"id": e2})).await;
    assert_eq!(v["value"]["state"], "active");
    let (s, v) = post(r, Some(&fay), &format!("/api/approve/{plan}"), json!({})).await;
    assert_eq!(s, 200, "{v}");
    let (_, v) = command(r, &fay, "device.get", json!({"id": e2})).await;
    assert_eq!(v["value"]["state"], "revoked");
    // Fay's app ends its own device and is out at the next call; her other app was already out.
    let (s, v) = command(r, &fay_app, "device.revoke", json!({"id": e1})).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(command(r, &fay_app, "party.list", json!({})).await.0, 401);
    // Bob's app is untouched; the device administrator and the instance administrator end it.
    assert_eq!(command(r, &bob_app, "party.list", json!({})).await.0, 200);
    let (s, v) = command(r, &dana, "device.revoke", json!({"id": b1})).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(command(r, &bob_app, "party.list", json!({})).await.0, 401);
    // The log: who ended which, in order.
    let log: Vec<(String, String, String)> = rig
        .app
        .store
        .backend()
        .list("security_event")
        .unwrap()
        .into_iter()
        .filter(|(_, f)| f["kind"] == "device_revoked")
        .map(|(_, f)| {
            let text = |k: &str| f[k].as_str().unwrap_or("").to_string();
            (text("user"), text("actor"), text("detail"))
        })
        .collect();
    let by: Vec<(&str, &str)> = log
        .iter()
        .map(|(u, a, _)| (u.as_str(), a.as_str()))
        .collect();
    assert_eq!(
        by,
        [("fay", "fay"), ("fay", "fay"), ("bob", "dana")],
        "{log:?}"
    );
    assert!(
        log[0].2.contains("proposed by fay/agent"),
        "the proposal: {}",
        log[0].2
    );
}

#[tokio::test]
async fn a_new_password_and_an_offboarding_end_the_devices() {
    let rig = rig();
    let r = &rig.r;
    let c = mobile_client(r).await;
    let (a1, r1, _) = app_signs_in(r, "eve", &c, &["base.user"], "Phone 1").await;
    let (a2, r2, _) = app_signs_in(r, "eve", &c, &["base.user"], "Tablet").await;
    let (bob_access, bob_refresh, _) =
        app_signs_in(r, "bob", &c, &["base.user"], "Bob's phone").await;
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
    let (a1, _, _) = app_signs_in(r, "bob", &c, &["base.user"], "Phone").await;
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
    let (a2, r2, _) = app_signs_in(r, "dana", &c, &["base.user"], "Dana's phone").await;
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
    let (access, _, _) = app_signs_in(r, "eve", &c, &["base.user"], "Pixel").await;
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
        // A web address is no redirect of a mobile client, whatever the site is.
        "https://app.example.test/mileage/callback",
    ] {
        let resp = reg(json!([bad]), json!("mobile")).await;
        assert_eq!(resp.status, 400, "{bad}");
        assert_eq!(resp.json()["error"], "invalid_redirect_uri");
    }
    assert_eq!(
        reg(json!([APP]), json!("phone")).await.json()["error"],
        "invalid_client_metadata"
    );
    // The groups the app declares: given back as written once, and never an administrative one.
    let declare = |scope: J| {
        let body = json!({"client_name": "Mileage", "redirect_uris": [APP], "obop_client_class": "mobile", "scope": scope});
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
    let ok = declare(json!("base.user  sales.user base.user")).await;
    assert_eq!(ok.status, 201, "{}", ok.text());
    assert_eq!(ok.json()["scope"], "base.user sales.user");
    for bad in [
        json!("platform.admin"),
        json!("base.user mileage.admin"),
        json!("expenses.approver"),
        json!(["base.user"]),
    ] {
        let resp = declare(bad.clone()).await;
        assert_eq!(resp.status, 400, "{bad}");
        assert_eq!(resp.json()["error"], "invalid_client_metadata", "{bad}");
    }
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
    let (access, _, _) = app_signs_in(r, "bob", &c, &["base.user"], "Bob's phone").await;
    let bob = rig.auth.resolve(&session(r, "bob").await).unwrap();

    // `obop call device list`
    let (v, denied) = crate::dispatch_call(&rig.app, &bob, "device.list", &text_input(&[]))
        .unwrap_or_else(|(line, _)| panic!("{line}"));
    let list = v.as_array().unwrap();
    assert_eq!(list.len(), 1, "{v}");
    assert!(list[0].get("push_token").is_none() && denied.contains(&"device.grant".to_string()));
    let id = list[0]["id"].as_str().unwrap().to_string();
    // The device administrator reads it; a person who is not its owner is told there is no such
    // device:
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
        assert_eq!(
            (r["error"]["code"].as_i64(), r["error"]["message"].as_str()),
            (Some(-32602), Some(format!("unknown tool: {gone}").as_str())),
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

async fn command_resp(r: &axum::Router, bearer: &str, cmd: &str, body: J) -> Resp {
    let (model, name) = cmd.split_once('.').unwrap();
    let auth = format!("Bearer {}", bearer.trim_start_matches("Bearer "));
    call(
        r,
        "POST",
        &format!("/api/{model}/{name}"),
        &[
            ("authorization", auth.as_str()),
            ("content-type", "application/json"),
        ],
        Some(("application/json", body.to_string())),
    )
    .await
}

/// The device commands, with the store refusing one call at every position of the request in turn
/// (D74): whatever the position, the answer is what the command says or a 503 that tells when to
/// ask again, never "no device", never a 401 or a 403 for a call the store could not answer, and
/// asking again after a 503 finishes the work.
#[tokio::test]
async fn a_refusal_of_the_store_at_any_call_of_a_device_command_is_a_503_and_never_a_verdict() {
    let (flaky, backend) = Flaky::memory();
    let rig = rig_on(App::with_store(obop_commands::Store::over(backend)));
    let r = &rig.r;
    let c = mobile_client_declaring(r, "base.user").await;
    let fay = {
        rig.auth
            .create_user("fay", PW, &["base.user".to_string()])
            .unwrap();
        session(r, "fay").await
    };
    let dana = session(r, "dana").await;
    let (app, _, id) = app_signs_in(r, "fay", &c, &["base.user"], "Fay's phone").await;
    let app = format!("Bearer {app}");

    #[derive(Clone, Copy)]
    enum Who {
        Person,
        App,
        Admin,
    }
    let mut covered = 0;
    for (cmd, who) in [
        ("device.get", Who::Person),
        ("device.get", Who::App),
        ("device.get", Who::Admin),
        ("device.list", Who::Person),
        ("device.list", Who::App),
        ("device.search", Who::Person),
        ("device.search", Who::Admin),
        ("device.search", Who::App),
        ("device.update", Who::Person),
        ("device.update", Who::App),
    ] {
        let bearer = match who {
            Who::Person => &fay,
            Who::App => &app,
            Who::Admin => &dana,
        };
        // An app lists nothing: its answer is the 400 of that rule, and a 503 where the store
        // could not answer before the rule was reached.
        let verdict = matches!(who, Who::App) && matches!(cmd, "device.list" | "device.search");
        let mut past_the_end = false;
        for skip in 0..120 {
            let body = match cmd {
                "device.update" => json!({"id": id, "name": format!("Phone {skip}")}),
                "device.list" | "device.search" => json!({}),
                _ => json!({"id": id}),
            };
            let before = flaky.refused();
            flaky.refuse_call_after(skip);
            let resp = command_resp(r, bearer, cmd, body).await;
            let refused = flaky.refused() - before;
            flaky.allow_models();
            let at = format!(
                "{cmd} refused from call {skip}: {} {}",
                resp.status,
                resp.text()
            );
            match resp.status {
                200 => assert!(!verdict, "{at}"),
                400 if verdict => assert!(resp.text().contains("cannot be listed"), "{at}"),
                503 => {
                    covered += 1;
                    assert!(refused > 0, "{at}: a 503 without a refusal");
                    assert!(resp.header("retry-after").parse::<u32>().is_ok(), "{at}");
                }
                _ => panic!("{at}"),
            }
            // Whatever happened, a name is a name: the old one or the new one.
            let (_, v) = command(r, &dana, "device.get", json!({"id": id})).await;
            let now = v["value"]["name"].as_str().unwrap_or("").to_string();
            assert!(
                now == "Fay's phone" || now.starts_with("Phone "),
                "{at}: {now}"
            );
            if refused == 0 {
                past_the_end = true;
                break;
            }
        }
        assert!(past_the_end, "{cmd}: the walk went past the last call");
    }
    assert!(covered >= 15, "the walk met the refusal {covered} times");

    // Ending a device: the family, the token and the record, each on its own refusal. A 503 says
    // so, asking again finishes it, and the device is ended for good in the end.
    let mut past_the_end = false;
    for (skip, who) in (0..120).flat_map(|k| [(k, Who::Person), (k, Who::App)]) {
        let (access, refresh_token, id) =
            app_signs_in(r, "fay", &c, &["base.user"], "Disposable").await;
        let bearer = match who {
            Who::App => format!("Bearer {access}"),
            _ => fay.clone(),
        };
        let before = flaky.refused();
        flaky.refuse_call_after(skip);
        let resp = command_resp(r, &bearer, "device.revoke", json!({"id": id})).await;
        let refused = flaky.refused() - before;
        flaky.allow_models();
        let at = format!(
            "revoke refused from call {skip}: {} {}",
            resp.status,
            resp.text()
        );
        match resp.status {
            200 => {}
            503 => {
                assert!(refused > 0, "{at}");
                assert!(resp.header("retry-after").parse::<u32>().is_ok(), "{at}");
                // Asking again: finished now, or already finished by the first try.
                let again = command_resp(r, &fay, "device.revoke", json!({"id": id})).await;
                assert!(
                    again.status == 200
                        || (again.status == 400 && again.text().contains("already revoked")),
                    "{at}: {} {}",
                    again.status,
                    again.text()
                );
            }
            // The token was already ended by the part of the work that went through, and the
            // refusal came at the very call that let the request in: the app is out. That is a
            // 401 only for a request the store could answer, so this is never one.
            _ => panic!("{at}"),
        }
        let (_, v) = command(r, &dana, "device.get", json!({"id": id})).await;
        assert_eq!(v["value"]["state"], "revoked", "{at}");
        assert_eq!(
            command(r, &format!("Bearer {access}"), "party.list", json!({}))
                .await
                .0,
            401,
            "{at}: the token of an ended device is out"
        );
        assert_eq!(refresh(r, &c, &refresh_token).await.status, 400, "{at}");
        if refused == 0 {
            past_the_end = true;
            break;
        }
    }
    assert!(past_the_end, "revoke: the walk went past the last call");
}
