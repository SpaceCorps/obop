//! E7i over HTTP: Client ID Metadata Documents read from a loopback stand-in through the hardened
//! outbound client, as `obop serve` wires it when the operator lists hosts. Not tried against a
//! real client's document.

use super::*;
use obop_auth::{Auth, AuthConfig};
use std::io::{Read, Write};
use std::sync::Mutex;

/// A stand-in host for documents on 127.0.0.1: `/ok.json` is a good document, `/moved.json`
/// redirects to it, `/big.json` is too large, `/text.json` is not served as JSON. Returns the port
/// and the paths it was asked for.
fn stand_in() -> (u16, Arc<Mutex<Vec<String>>>) {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let asked = Arc::new(Mutex::new(Vec::new()));
    let seen = asked.clone();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { continue };
            let mut buf = [0u8; 4096];
            let n = s.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let path = req.split(' ').nth(1).unwrap_or("").to_string();
            seen.lock().unwrap().push(path.clone());
            let id = format!("http://127.0.0.1:{port}{path}");
            let doc = json!({"client_id": id, "client_name": "Stand-in agent", "redirect_uris": [REDIRECT]})
                .to_string();
            let (status, ctype, body) = match path.as_str() {
                "/ok.json" => ("200 OK", "application/json", doc),
                "/moved.json" => ("302 Found", "application/json", String::new()),
                "/big.json" => ("200 OK", "application/json", "x".repeat(20_000)),
                _ => ("200 OK", "text/plain", doc),
            };
            let _ = write!(
                s,
                "HTTP/1.1 {status}\r\ncontent-type: {ctype}\r\nlocation: /ok.json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (port, asked)
}

fn rig_with(cfg: obop_oauth::OAuthConfig) -> axum::Router {
    let app = App::new();
    app.seed_demo();
    let auth = Arc::new(Auth::with_test_config(
        app.store.backend(),
        AuthConfig {
            default_require_admins: false,
            ..AuthConfig::insecure_dev()
        },
    ));
    auth.set_clock_for_test(T0);
    auth.create_user("bob", PW, &["base.user".into(), "sales.user".into()])
        .unwrap();
    let state = obop_http::HttpState::with_configs(
        app.dispatcher.clone(),
        auth,
        obop_http::mcp::McpConfig::default(),
        cfg,
    );
    state.set_trust_proxy(true);
    obop_http::router(state, None)
}

fn listed() -> obop_oauth::OAuthConfig {
    obop_oauth::OAuthConfig {
        cimd_hosts: obop_oauth::parse_schemes("127.0.0.1"),
        cimd_allow_loopback: true,
        ..Default::default()
    }
}

#[tokio::test]
async fn a_client_named_by_its_document_signs_in_and_gets_a_token() {
    let (port, asked) = stand_in();
    let r = rig_with(listed());
    let meta = call(
        &r,
        "GET",
        "/.well-known/oauth-authorization-server",
        &[],
        None,
    )
    .await;
    assert_eq!(meta.json()["client_id_metadata_document_supported"], true);

    let id = format!("http://127.0.0.1:{port}/ok.json");
    let (page, _) = start(&r, &authorize_url(&id, REDIRECT, &[])).await;
    assert!(page.text().contains("Stand-in agent"), "{}", page.text());
    let done = sign_in_and_allow(
        &r,
        "bob",
        &authorize_url(&id, REDIRECT, &[]),
        &["sales.user"],
        "7",
    )
    .await;
    let code = param(&location(&done), "code");
    let t = exchange(&r, &id, &code).await;
    assert_eq!(t.status, 200, "{}", t.text());
    assert_eq!(t.json()["scope"], "sales.user");
    // Read once; the sign-ins and the exchange after it used the cache.
    assert_eq!(asked.lock().unwrap().as_slice(), ["/ok.json".to_string()]);
}

#[tokio::test]
async fn a_redirect_a_large_body_or_a_body_that_is_not_json_is_no_client() {
    let (port, asked) = stand_in();
    let r = rig_with(listed());
    for path in ["/moved.json", "/big.json", "/text.json"] {
        let id = format!("http://127.0.0.1:{port}{path}");
        let page = call(&r, "GET", &authorize_url(&id, REDIRECT, &[]), &[], None).await;
        assert_ne!(page.status, 200, "{path}");
        assert!(!page.text().contains("Stand-in agent"), "{path}");
    }
    // The redirect was not followed.
    assert!(!asked.lock().unwrap().contains(&"/ok.json".to_string()));
}

#[tokio::test]
async fn without_the_operators_list_no_document_is_read_and_the_metadata_is_as_before() {
    let (port, asked) = stand_in();
    let r = rig_with(obop_oauth::OAuthConfig::default());
    let meta = call(
        &r,
        "GET",
        "/.well-known/oauth-authorization-server",
        &[],
        None,
    )
    .await;
    assert!(
        meta.json()
            .get("client_id_metadata_document_supported")
            .is_none()
    );
    let id = format!("http://127.0.0.1:{port}/ok.json");
    let page = call(&r, "GET", &authorize_url(&id, REDIRECT, &[]), &[], None).await;
    assert_ne!(page.status, 200);
    assert!(asked.lock().unwrap().is_empty());
}
