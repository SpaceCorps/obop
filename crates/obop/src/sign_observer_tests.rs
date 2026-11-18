//! O27: the answer of `sign_link.create` holds the link (its secret), and the dispatcher hands that
//! answer to its observers. The connectors runtime is one: a connection subscribed to
//! `sign_link.create` must send the stored record, which holds only a hash, never the link.

use crate::sign_tests::fixture;
use obop_connectors::catalog::providers;
use obop_connectors::{Config, Runtime, SecretBox};
use serde_json::{Value as J, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

/// A local receiver of webhooks: every request it got, as text.
struct Receiver {
    port: u16,
    got: Arc<Mutex<Vec<String>>>,
}

impl Receiver {
    fn new() -> Self {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let got = Arc::new(Mutex::new(Vec::new()));
        let seen = got.clone();
        std::thread::spawn(move || {
            for s in l.incoming() {
                let Ok(mut s) = s else { break };
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let n = s.read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let t = String::from_utf8_lossy(&buf).to_string();
                    if let Some(h) = t.find("\r\n\r\n") {
                        let len = t[..h]
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                            })
                            .unwrap_or(0);
                        if buf.len() >= h + 4 + len {
                            break;
                        }
                    }
                }
                seen.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&buf).to_string());
                let _ = s.write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}",
                );
            }
        });
        Self { port, got }
    }

    fn wait_for_one(&self) -> String {
        for _ in 0..300 {
            if let Some(r) = self.got.lock().unwrap().first() {
                return r.clone();
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        panic!("the event was not delivered");
    }
}

#[tokio::test]
async fn a_connection_subscribed_to_sign_link_create_never_sends_the_link() {
    let f = fixture().await;
    let backend = f.app.store.backend();
    // The connectors of this instance, with loopback receivers allowed (the instance's own
    // runtime reads that from the environment, which a test must not set).
    let rt = Runtime::with_providers(
        backend.clone(),
        f.app.dispatcher.clone(),
        SecretBox::open_for(&backend).unwrap(),
        Config {
            allow_loopback: true,
            public_url: None,
        },
        providers(),
        None,
    );
    connectors::register(&f.app.dispatcher, &f.app.store, &rt);
    // A raw observer next to it: it is handed the answer, link included (what O27 is about).
    let answers: Arc<Mutex<Vec<String>>> = Arc::default();
    let seen = answers.clone();
    f.app.dispatcher.observe(
        "o27-test",
        Box::new(move |command, _, _, out| {
            if command == "sign_link.create" {
                seen.lock().unwrap().push(out.to_json().to_string());
            }
        }),
    );
    let r = Receiver::new();
    let ok = |s: u16, v: &J| assert_eq!(s, 200, "{v}");
    let (s, v) = f
        .api(
            "root",
            "/api/provider_policy/create",
            json!({"provider": "webhook_out", "enabled": true, "users_may_use": true,
                   "dpa_recorded": true, "dpa_reference": "Test DPA",
                   "legal_basis_text": "We send link records to a test receiver."}),
        )
        .await;
    ok(s, &v);
    let (s, v) = f
        .api(
            "root",
            "/api/connection/create",
            json!({"name": "Link hook", "provider": "webhook_out",
                   "config": json!({"url": format!("http://127.0.0.1:{}/hook", r.port),
                                    "events": "sign_link.create"}).to_string(),
                   "credentials": json!({"signing_secret": "0123456789abcdef-signing-secret"}).to_string(),
                   "field_allowlist": json!({"sign_link":
                        ["request_id", "expires_at", "used_at", "revoked_at", "link"]}).to_string()}),
        )
        .await;
    ok(s, &v);
    let conn = v["value"]["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api(
            "root",
            "/api/consent/create",
            json!({"connection": conn, "purpose": "Tell the test receiver about new links",
                   "legal_basis": "contract"}),
        )
        .await;
    ok(s, &v);
    let (s, v) = f
        .api("root", "/api/connection/connect", json!({"id": conn}))
        .await;
    ok(s, &v);
    rt.start();
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/send_document",
            json!({"id": f.req}),
        )
        .await;
    ok(s, &v);
    let (link_id, path) = f.mint("mia", &f.req.clone()).await;
    let token = path.rsplit('/').next().unwrap().to_string();
    assert!(token.len() >= 20, "{path}");
    // The observer was handed the link...
    assert!(
        answers.lock().unwrap().iter().any(|a| a.contains(&token)),
        "the raw answer holds the link"
    );
    // ...and the connection sent the stored record, which does not hold it.
    let got = r.wait_for_one();
    let body: J = serde_json::from_str(got.split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(body["type"], "sign_link.create", "{body}");
    let rec = &body["records"][0];
    assert_eq!(rec["request_id"], J::from(f.req.clone()), "{body}");
    assert!(rec.get("link").is_none_or(J::is_null), "{body}");
    assert!(!got.contains(&token), "the link left the instance: {got}");
    assert!(!got.contains(&path), "{got}");
    assert!(
        !f.record("sign_link", &link_id)
            .values()
            .any(|v| v.to_string().contains(&token)),
        "only a hash is stored"
    );
}
