//! E1c: an operator who lists the redirect hosts of the MCP clients it accepts
//! (`OBOP_OAUTH_MCP_REDIRECT_HOSTS`) refuses the registration of any other MCP client, and the
//! sign-in of one registered before the list was set. Mobile apps are not affected (their list is
//! `OBOP_OAUTH_MOBILE_SCHEMES`), and without a list every client registers as before.

use super::mobile::{APP, LOOP4};
use super::*;

fn register(r: &Rig, uris: &[&str]) -> Result<String, OAuthError> {
    r.o.register(
        "10.0.0.1",
        &json!({"client_name": "Some app", "redirect_uris": uris}),
    )
    .map(|d| d["client_id"].as_str().unwrap().to_string())
}

fn listed() -> OAuthConfig {
    OAuthConfig {
        mcp_redirect_hosts: crate::parse_schemes(" Claude.AI , localhost "),
        ..OAuthConfig::default()
    }
}

#[test]
fn an_operator_list_of_redirect_hosts_refuses_other_mcp_clients() {
    let r = rig_with(listed());
    // On the list: the host, whatever the port and the path.
    register(&r, &[REDIRECT, "http://localhost:3000/cb"]).unwrap();
    // Off the list, alone or with one that is on it; a host that merely ends like a listed one.
    for uris in [
        vec!["https://evil.example/cb"],
        vec![REDIRECT, "https://evil.example/cb"],
        vec!["https://claude.ai.evil.example/cb"],
        vec!["http://127.0.0.1:8080/cb"],
    ] {
        let e = register(&r, &uris).unwrap_err();
        assert_eq!(e.error, "invalid_redirect_uri", "{uris:?}: {e:?}");
        assert!(
            e.description.contains("OBOP_OAUTH_MCP_REDIRECT_HOSTS"),
            "{}",
            e.description
        );
    }
    // A mobile app is held to its own list, not to this one.
    r.mobile_with(&[APP, LOOP4]).unwrap();
    // Without a list, any host registers as before.
    let open = rig();
    register(&open, &["https://evil.example/cb"]).unwrap();
    assert_eq!(crate::parse_schemes(" , "), None);
}

#[test]
fn a_client_registered_before_the_list_cannot_sign_in_to_a_host_off_it() {
    // One store, two configurations: the client registered while the instance had no list.
    let backend = obop_storage::open("memory:").unwrap();
    let before = rig_on(backend.clone(), "ada", OAuthConfig::default());
    let id = register(&before, &["https://evil.example/cb"]).unwrap();
    let ok = register(&before, &[REDIRECT]).unwrap();
    let auth = before.auth.clone();
    let after = Rig {
        user: "ada".into(),
        o: OAuth::new(backend.clone(), auth.clone(), listed()).unwrap(),
        auth,
        backend,
    };
    let refused = after
        .o
        .validate_authorize(BASE, &after.params(&id, "https://evil.example/cb"))
        .err();
    assert!(
        matches!(
            refused,
            Some(AuthzError::Page {
                key: "err.redirect"
            })
        ),
        "{refused:?}"
    );
    // The only redirect, left out of the request: the same answer.
    let p = AuthorizeParams {
        redirect_uri: None,
        ..after.params(&id, "https://evil.example/cb")
    };
    assert!(matches!(
        after.o.validate_authorize(BASE, &p),
        Err(AuthzError::Page {
            key: "err.redirect"
        })
    ));
    // A client whose redirect is on the list goes on as before.
    after
        .o
        .validate_authorize(BASE, &after.params(&ok, REDIRECT))
        .unwrap();
}
