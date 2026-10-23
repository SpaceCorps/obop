//! D69 over HTTP and through the dispatcher: creating users, editing groups and offboarding from
//! the REST routes under `/api/auth/users`, and the same changes as `user_account` commands that
//! an agent can only propose. Named acceptance tests: `rest_*`, `agents_*`, `cli_*`.

use crate::app::App;
use crate::mfa_tests::{PW, Rig, post, send, token};
use obop_auth::{Auth, AuthConfig};
use obop_commands::Authenticator;
use serde_json::{Value as J, json};
use std::sync::Arc;

pub(crate) struct AccountsRig {
    pub(crate) rig: Rig,
    pub(crate) app: App,
}

pub(crate) fn rig() -> AccountsRig {
    let app = App::new();
    app.seed_demo();
    let auth = Arc::new(Auth::with_test_config(
        app.store.backend(),
        AuthConfig::insecure_dev(),
    ));
    for (name, groups) in [
        ("root", vec!["platform.admin"]),
        ("ada", vec!["base.user"]),
        ("bob", vec!["base.user"]),
    ] {
        auth.create_user(
            name,
            PW,
            &groups.into_iter().map(String::from).collect::<Vec<_>>(),
        )
        .unwrap();
    }
    app.register_accounts(auth.clone() as Arc<dyn Authenticator>);
    let r = obop_http::router(
        obop_http::HttpState::new(app.dispatcher.clone(), auth.clone()),
        None,
    );
    AccountsRig {
        rig: Rig { r, auth },
        app,
    }
}

async fn get(r: &axum::Router, t: &str, path: &str) -> (u16, J) {
    send(r, "GET", Some(t), path, json!({})).await
}

async fn events(r: &axum::Router, root: &str, kind: &str) -> Vec<J> {
    let (s, b) = post(
        r,
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

async fn agent_for(r: &axum::Router, human: &str, scopes: &[&str]) -> String {
    let (s, b) = post(
        r,
        Some(human),
        "/api/auth/agent-token",
        json!({"scopes": scopes}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    b["token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn rest_create_edit_groups_and_offboard_a_user() {
    let a = rig();
    let r = &a.rig.r;
    let root = token(r, "root").await;
    let ada = token(r, "ada").await;

    // The registered groups are listed for administrators only.
    let (s, b) = get(r, &root, "/api/auth/groups").await;
    assert_eq!(s, 200, "{b}");
    let groups: Vec<&str> = b["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g.as_str().unwrap())
        .collect();
    assert!(groups.contains(&"platform.admin") && groups.contains(&"base.user"));
    assert!(!groups.contains(&"*"));
    assert_eq!(get(r, &ada, "/api/auth/groups").await.0, 404);

    // Create: a generated temporary password, shown once, to be changed at the first sign-in.
    assert_eq!(
        post(r, Some(&ada), "/api/auth/users", json!({"username": "x"}))
            .await
            .0,
        404,
        "only administrators"
    );
    let (s, b) = post(
        r,
        Some(&root),
        "/api/auth/users",
        json!({"username": "Dana", "groups": ["base.user"]}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    let temp = b["temporary_password"].as_str().unwrap().to_string();
    assert_eq!(
        (b["user"].clone(), b["must_change_password"].clone()),
        (json!("dana"), json!(true))
    );
    let (s, login) = post(
        r,
        None,
        "/api/auth/login",
        json!({"username": "dana", "password": temp}),
    )
    .await;
    assert_eq!(
        (s, login["password_change_required"].clone()),
        (200, json!(true)),
        "{login}"
    );
    let dana = login["token"].as_str().unwrap().to_string();
    // A limited session owes a password change and can do nothing else.
    assert_eq!(
        post(r, Some(&dana), "/api/party/list", json!({})).await.0,
        403
    );
    // A chosen password, an unknown group, a duplicate.
    let (s, _) = post(
        r,
        Some(&root),
        "/api/auth/users",
        json!({"username": "eli", "password": "a password of mine", "groups": "base.user, sales.user"}),
    )
    .await;
    assert_eq!(s, 200);
    for (body, why) in [
        (
            json!({"username": "fay", "groups": ["no.such"]}),
            "unknown group",
        ),
        (json!({"username": "dana"}), "duplicate"),
        (
            json!({"username": "gus", "password": "short"}),
            "short password",
        ),
        (json!({"username": "a b"}), "bad name"),
    ] {
        assert_eq!(
            post(r, Some(&root), "/api/auth/users", body).await.0,
            400,
            "{why}"
        );
    }
    assert_eq!(events(r, &root, "user_create").await.len(), 2);

    // Groups: ada gains one; her open session sees it at the next request.
    assert_eq!(
        post(r, Some(&ada), "/api/sale_order/list", json!({}))
            .await
            .0,
        404
    );
    let (s, b) = post(
        r,
        Some(&root),
        "/api/auth/users/ada/groups",
        json!({"groups": ["base.user", "sales.user"]}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["added"], json!(["sales.user"]));
    assert_eq!(
        post(r, Some(&ada), "/api/sale_order/list", json!({}))
            .await
            .0,
        200
    );
    let log = events(r, &root, "user_groups").await;
    assert_eq!(
        (log.len(), log[0]["actor"].clone(), log[0]["user"].clone()),
        (1, json!("root"), json!("ada"))
    );
    // Refusals: unknown group, oneself, a stranger, a non-administrator.
    for (path, who, body, status) in [
        (
            "/api/auth/users/ada/groups",
            &root,
            json!({"groups": ["no.such"]}),
            400,
        ),
        (
            "/api/auth/users/root/groups",
            &root,
            json!({"groups": []}),
            400,
        ),
        (
            "/api/auth/users/ghost/groups",
            &root,
            json!({"groups": []}),
            400,
        ),
        (
            "/api/auth/users/ada/groups",
            &ada,
            json!({"groups": ["platform.admin"]}),
            404,
        ),
        ("/api/auth/users/root/offboard", &root, json!({}), 400),
        ("/api/auth/users/bob/offboard", &ada, json!({}), 404),
    ] {
        assert_eq!(post(r, Some(who), path, body).await.0, status, "{path}");
    }

    // Offboard: sessions and agent tokens stop at once, for good.
    let bob = token(r, "bob").await;
    let bob_agent = agent_for(r, &bob, &["base.user"]).await;
    assert_eq!(
        post(r, Some(&bob_agent), "/api/party/list", json!({}))
            .await
            .0,
        200
    );
    let (s, b) = post(
        r,
        Some(&root),
        "/api/auth/users/bob/offboard",
        json!({"note": "left"}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(
        (
            b["revoked_sessions"].clone(),
            b["revoked_agent_tokens"].clone()
        ),
        (json!(1), json!(1))
    );
    for t in [&bob, &bob_agent] {
        assert_eq!(post(r, Some(t), "/api/party/list", json!({})).await.0, 401);
    }
    let (_, users) = get(r, &root, "/api/auth/users").await;
    let row = users["users"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["name"] == "bob")
        .unwrap()
        .clone();
    assert_eq!(
        (row["disabled"].clone(), row["offboarded"].clone()),
        (json!(true), json!(true))
    );
    // Enabled again: bob signs in; the old agent token does not come back.
    assert_eq!(
        post(r, Some(&root), "/api/auth/users/bob/enable", json!({}))
            .await
            .0,
        200
    );
    assert_eq!(
        post(
            r,
            None,
            "/api/auth/login",
            json!({"username": "bob", "password": PW})
        )
        .await
        .0,
        200
    );
    assert_eq!(
        post(r, Some(&bob_agent), "/api/party/list", json!({}))
            .await
            .0,
        401
    );
    assert_eq!(events(r, &root, "user_offboard").await.len(), 1);
}

#[tokio::test]
async fn agents_can_only_propose_account_changes_and_an_administrator_decides() {
    let a = rig();
    let r = &a.rig.r;
    let root = token(r, "root").await;
    let ada = token(r, "ada").await;
    let agent = agent_for(r, &root, &["platform.admin"]).await;

    // The commands exist for the administrator's agent and not for anybody else's.
    let (_, c) = get(r, &agent, "/api/commands").await;
    let ids: Vec<&str> = c["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    for id in [
        "user_account.add",
        "user_account.set_groups",
        "user_account.offboard",
    ] {
        assert!(ids.contains(&id), "{id}");
    }
    let weak = agent_for(r, &ada, &["base.user"]).await;
    let body = json!({"id": "bob", "groups": "base.user,sales.user"});
    assert_eq!(
        post(r, Some(&weak), "/api/user_account/set_groups", body.clone())
            .await
            .0,
        404
    );
    assert_eq!(
        post(r, Some(&ada), "/api/user_account/set_groups", body.clone())
            .await
            .0,
        404
    );

    // The agent proposes; nothing changes until a person approves.
    let (s, b) = post(
        r,
        Some(&agent),
        "/api/user_account/set_groups",
        body.clone(),
    )
    .await;
    assert_eq!(
        (s, b["approval_required"].clone()),
        (202, json!(true)),
        "{b}"
    );
    let plan = b["plan_id"].as_str().unwrap().to_string();
    let (_, users) = get(r, &root, "/api/auth/users").await;
    let bob = users["users"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["name"] == "bob")
        .unwrap()
        .clone();
    assert_eq!(bob["groups"], json!(["base.user"]));
    // The agent cannot approve its own plan.
    assert_eq!(
        post(r, Some(&agent), &format!("/api/approve/{plan}"), json!({}))
            .await
            .0,
        404
    );
    let (s, b) = post(r, Some(&root), &format!("/api/approve/{plan}"), json!({})).await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["value"]["groups"], "base.user, sales.user");
    assert_eq!(b["value"]["previous_groups"], "base.user");
    let log = events(r, &root, "user_groups").await;
    assert_eq!(
        log[0]["actor"], "root/agent",
        "the log names the agent that proposed it"
    );

    // Offboarding and creating are proposals too; a created account needs a password a person chose.
    let (s, b) = post(
        r,
        Some(&agent),
        "/api/user_account/offboard",
        json!({"id": "bob"}),
    )
    .await;
    assert_eq!(s, 202, "{b}");
    let (s, b) = post(
        r,
        Some(&agent),
        "/api/user_account/add",
        json!({"id": "hal", "password": "a password of mine", "groups": "base.user"}),
    )
    .await;
    assert_eq!(s, 202, "{b}");
    let hal = b["plan_id"].as_str().unwrap().to_string();
    let (s, b) = post(r, Some(&root), &format!("/api/approve/{hal}"), json!({})).await;
    assert_eq!(s, 200, "{b}");
    assert!(!b.to_string().contains("a password of mine"));
    assert_eq!(
        post(
            r,
            None,
            "/api/auth/login",
            json!({"username": "hal", "password": "a password of mine"})
        )
        .await
        .1["password_change_required"],
        true
    );

    // A plan outlives its author's authority: the administrator is disabled before approval.
    let (_, p) = post(
        r,
        Some(&agent),
        "/api/user_account/set_groups",
        json!({"id": "ada", "groups": "platform.admin"}),
    )
    .await;
    let stale = p["plan_id"].as_str().unwrap().to_string();
    let second = a
        .rig
        .auth
        .create_user("second", PW, &["platform.admin".into()]);
    assert!(second.is_ok());
    let second_t = token(r, "second").await;
    assert_eq!(
        post(
            r,
            Some(&second_t),
            "/api/auth/users/root/disable",
            json!({})
        )
        .await
        .0,
        200
    );
    let (s, _) = post(
        r,
        Some(&second_t),
        &format!("/api/approve/{stale}"),
        json!({}),
    )
    .await;
    assert_eq!(
        s, 400,
        "the proposing administrator is disabled, so the plan does nothing"
    );
    let (_, users) = get(r, &second_t, "/api/auth/users").await;
    let ada_row = users["users"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["name"] == "ada")
        .unwrap()
        .clone();
    assert_eq!(ada_row["groups"], json!(["base.user"]));
}

#[tokio::test]
async fn agents_tool_lists_show_the_account_tools_to_administrators_only() {
    let a = rig();
    let admin = obop_commands::Principal::user("root", &["platform.admin"]);
    let member = obop_commands::Principal::user("ada", &["base.user"]);
    let names = |p: &obop_commands::Principal| -> Vec<String> {
        obop_mcp::schema::tools(&a.app.dispatcher, p)
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect()
    };
    let admin_tools = names(&admin);
    assert!(admin_tools.contains(&"user_account__set_groups".to_string()));
    assert!(admin_tools.contains(&"user_account__offboard".to_string()));
    assert!(
        !names(&member)
            .iter()
            .any(|n| n.starts_with("user_account__"))
    );
}

fn cli_opts(db: &str) -> crate::Opts {
    crate::Opts {
        db: Some(db.into()),
        token: None,
        demo: false,
        dev_auth: false,
        public_site: false,
        trust_proxy: false,
        admin_prefix: "/admin".into(),
    }
}

#[test]
fn cli_import_checks_first_writes_a_private_password_file_and_never_overwrites() {
    let dir = std::env::temp_dir().join(format!(
        "obop-accounts-cli-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = |n: &str| dir.join(n).to_str().unwrap().to_string();
    let db = format!("sqlite://{}", path("cli.db"));
    let o = cli_opts(&db);
    let count = || Auth::new(obop_storage::open(&db).unwrap()).user_count();
    let csv = path("users.csv");
    std::fs::write(
        &csv,
        "\u{feff}name;groups\r\nAnn;base.user\r\nBen;\"base.user, sales.user\"\r\n",
    )
    .unwrap();
    // A dry run writes nothing.
    crate::accounts_cmd::import(&o, &csv, true, None).unwrap();
    assert_eq!(count(), 0);
    // A real run writes the accounts and a password file only its owner can read.
    let out = path("passwords.csv");
    crate::accounts_cmd::import(&o, &csv, false, Some(&out)).unwrap();
    assert_eq!(count(), 2);
    let text = std::fs::read_to_string(&out).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "name,temporary_password");
    assert!(lines[1].starts_with("ann,") && lines[2].starts_with("ben,"));
    let pw = lines[1].split_once(',').unwrap().1;
    assert!(
        Auth::new(obop_storage::open(&db).unwrap())
            .login_step("ann", pw)
            .is_ok()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&out).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
    // The password file is never overwritten, and a second run with the same names refuses.
    assert!(crate::accounts_cmd::import(&o, &csv, false, Some(&out)).is_err());
    assert!(crate::accounts_cmd::import(&o, &csv, false, Some(&path("again.csv"))).is_err());
    assert_eq!(count(), 2);
    // A bad column or a bad group refuses the whole file.
    std::fs::write(&csv, "name,role\ncy,x\n").unwrap();
    assert!(crate::accounts_cmd::import(&o, &csv, false, None).is_err());
    std::fs::write(&csv, "name,groups\ncy,no.such\ndee,base.user\n").unwrap();
    assert!(crate::accounts_cmd::import(&o, &csv, false, None).is_err());
    assert_eq!(count(), 2);
    // Groups, offboarding and enabling from the CLI.
    crate::accounts_cmd::groups(&o, "ann", Some("base.user,sales.user")).unwrap();
    assert!(crate::accounts_cmd::groups(&o, "ann", Some("no.such")).is_err());
    crate::accounts_cmd::offboard(&o, "ben", Some("left")).unwrap();
    assert!(
        Auth::new(obop_storage::open(&db).unwrap())
            .login_step("ben", "x")
            .is_err()
    );
    crate::accounts_cmd::disable(&o, "ben", false).unwrap();
    crate::accounts_cmd::users(&o).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
