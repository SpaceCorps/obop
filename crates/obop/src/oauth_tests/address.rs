//! The callers of the one address rule (`obop-http` `addr::client_address`) over HTTP, at the
//! OAuth endpoints and sign-in pages that limit by the client's address: registration, token,
//! revoke, authorize and the password step of the sign-in page. Each is tried behind a proxy that
//! is trusted and behind one that is not, with the properties the rule gives:
//!
//! - trusted: the **last** entry of the line is the address (the entries before it are the
//!   client's own and do not choose the bucket), the same address written alone is the same
//!   bucket, and a fresh last entry is a bucket of its own even after the first entry has been
//!   spent;
//! - not trusted: the header is ignored, so every spelling shares the one bucket of the peer.
//!
//! The first 429 must come at the endpoint's own per-address limit (`OAuthConfig`: register 20 an
//! hour, authorize 60 and login 30 in ten minutes, token and revoke 120 a minute), not at some
//! other cap: registration has a global cap of 300 an hour that would end a test that gave every
//! request a bucket of its own, and a count that only had to run out somewhere would pass.
//!
//! A caller that read the first entry, ignored the header, trusted it without being told to, or
//! passed an address of its own making fails one of these for its endpoint. The scan in
//! `obop-http` `addr_scan_tests.rs` keeps callers from reading the header themselves; this
//! keeps them from passing the wrong thing to the rule. The other callers are tried over HTTP
//! elsewhere: the ceremony options of the passkeys (`passkey_tests.rs`; the verify step builds
//! its context in the same function), the start, the callback and the exchange of Microsoft
//! sign-in (`sso_tests.rs`), the phone's start and return (`sso_tests/phone.rs`), the signer's
//! door (`sign_tests/`) and the public site (`public_site_tests.rs`). The first 429 is pinned to
//! the endpoint's own limit in this file, at the passkey options and at the start and the
//! exchange of Microsoft sign-in; the others show that the count runs out and which spellings
//! share it (docs/ISSUES.md O31 says which is which).

use super::*;
use obop_auth::{Auth, AuthConfig};

struct Rig {
    r: axum::Router,
    _app: App,
}

fn rig(trust_proxy: bool) -> Rig {
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
    auth.create_user("bob", PW, &["base.user".to_string()])
        .unwrap();
    // `McpConfig::default()`, not `HttpState::new` (which reads the settings of whoever runs the
    // tests, `OBOP_PUBLIC_URL` among them).
    let state = obop_http::HttpState::with_mcp(
        app.dispatcher.clone(),
        auth.clone(),
        obop_http::mcp::McpConfig::default(),
    );
    state.set_trust_proxy(trust_proxy);
    Rig {
        r: obop_http::router(state, None),
        _app: app,
    }
}

const ENDPOINTS: [&str; 5] = ["register", "token", "revoke", "authorize", "login"];

/// The number of requests of one address that an endpoint answers before it says 429.
fn per_address(endpoint: &str) -> u32 {
    let c = obop_oauth::OAuthConfig::default();
    u32::try_from(match endpoint {
        "register" => c.register_per_ip_hour,
        "token" | "revoke" => c.token_per_ip_minute,
        "authorize" => c.authorize_per_ip,
        "login" => c.login_per_ip,
        other => unreachable!("{other}"),
    })
    .unwrap()
}

/// One request to `endpoint` as the client the line names, and the status it got.
async fn hit(r: &axum::Router, client: &str, endpoint: &str, xff: &str) -> u16 {
    let h = [("x-forwarded-for", xff)];
    match endpoint {
        "register" => {
            call(
                r,
                "POST",
                "/oauth/register",
                &h,
                Some((
                    "application/json",
                    json!({"client_name": "Claude", "redirect_uris": [REDIRECT],
                           "token_endpoint_auth_method": "none"})
                    .to_string(),
                )),
            )
            .await
            .status
        }
        "token" => {
            let f = form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", "nothing"),
                ("client_id", client),
            ]);
            call(r, "POST", "/oauth/token", &h, Some(f)).await.status
        }
        "revoke" => {
            let f = form(&[("token", "nothing"), ("client_id", client)]);
            call(r, "POST", "/oauth/revoke", &h, Some(f)).await.status
        }
        "authorize" => {
            call(r, "GET", &authorize_url(client, REDIRECT, &[]), &h, None)
                .await
                .status
        }
        "login" => {
            // A flow of its own (the page that begins it is counted for the address that asked
            // for it, which is not the one under test), then a wrong password from the line.
            let (_, mut b) = start(r, &authorize_url(client, REDIRECT, &[])).await;
            b.ip = xff.to_string();
            b.post(
                r,
                "/oauth/authorize/login",
                &[("username", "bob"), ("password", "not the password")],
            )
            .await
            .status
        }
        other => unreachable!("{other}"),
    }
}

/// How many requests it took before the endpoint said 429, the lines being `from(n)`.
async fn spent_after(
    r: &axum::Router,
    client: &str,
    endpoint: &str,
    from: impl Fn(u32) -> String,
) -> Option<u32> {
    for n in 0..300 {
        if hit(r, client, endpoint, &from(n)).await == 429 {
            return Some(n);
        }
    }
    None
}

#[tokio::test]
async fn behind_a_trusted_proxy_the_last_entry_is_the_bucket_at_every_endpoint() {
    for endpoint in ENDPOINTS {
        let rig = rig(true);
        let client = client(&rig.r).await;
        // Entries before the last are the client's own: whatever they say, the bucket is the
        // proxy's entry, so the count runs out. (A caller that read the first entry gives each
        // request a bucket of its own and never gets here.)
        let spent = spent_after(&rig.r, &client, endpoint, |n| {
            format!("203.0.113.{}, 198.51.100.77", n % 200)
        })
        .await;
        // The one registration of `client()` came from the peer's bucket, not this one.
        assert_eq!(
            spent,
            Some(per_address(endpoint)),
            "{endpoint}: the entries before the last chose the bucket"
        );
        // The same address on its own, and with a port, is that bucket.
        for line in ["198.51.100.77", "198.51.100.77:5000"] {
            assert_eq!(
                hit(&rig.r, &client, endpoint, line).await,
                429,
                "{endpoint}: {line}"
            );
        }
        // The spent address as the first entry and a fresh one last: the fresh one is the
        // address, and has its own count.
        assert_ne!(
            hit(&rig.r, &client, endpoint, "198.51.100.77, 198.51.100.78").await,
            429,
            "{endpoint}: the first entry chose the bucket"
        );
        assert_ne!(
            hit(&rig.r, &client, endpoint, "198.51.100.79").await,
            429,
            "{endpoint}: another address is not held up"
        );
    }
}

#[tokio::test]
async fn without_a_trusted_proxy_the_header_is_ignored_at_every_endpoint() {
    for endpoint in ENDPOINTS {
        let rig = rig(false);
        let client = client(&rig.r).await;
        // Every request names another address and all of them are one bucket: the peer's.
        let spent = spent_after(&rig.r, &client, endpoint, |n| {
            format!("198.51.100.{}", n % 200 + 1)
        })
        .await;
        // `client()` spent one registration of the peer's bucket, which is this one.
        let want = per_address(endpoint) - u32::from(endpoint == "register");
        assert_eq!(
            spent,
            Some(want),
            "{endpoint}: an address in the header got a bucket of its own though no proxy is trusted"
        );
    }
}
