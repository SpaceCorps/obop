//! E7i: Client ID Metadata Documents. Off unless the operator lists hosts; a listed address is a
//! client whose document says so, read once an hour through the fetcher the HTTP layer sets (here a
//! stand-in that counts its reads); a document that does not describe its own address, carries a
//! secret or names a redirect that a registration could not, is no client.

use super::*;
use crate::cimd::{document_host, with_cimd};
use std::sync::Mutex;

const ID: &str = "https://app.example/oauth/client.json";

fn hosts(list: &str) -> OAuthConfig {
    OAuthConfig {
        cimd_hosts: crate::parse_schemes(list),
        ..OAuthConfig::default()
    }
}

fn doc(id: &str) -> J {
    json!({
        "client_id": id,
        "client_name": "Example agent",
        "redirect_uris": [REDIRECT],
        "token_endpoint_auth_method": "none",
    })
}

/// A fetcher that answers `body` for every address and keeps the addresses it was asked for.
fn serve(r: &Rig, body: J) -> Arc<Mutex<Vec<String>>> {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let seen = asked.clone();
    let text = body.to_string().into_bytes();
    r.o.set_cimd_fetcher(Arc::new(move |url: &str| {
        seen.lock().unwrap().push(url.to_string());
        Ok(text.clone())
    }));
    asked
}

#[test]
fn a_listed_address_is_a_client_and_signs_in_end_to_end() {
    let r = rig_with(hosts("app.example"));
    let asked = serve(&r, doc(ID));
    assert!(r.o.cimd_enabled());
    let loc = r.consent(&r.params(ID, REDIRECT), &["sales.user"], 7);
    let code = query(&loc, "code");
    let t = r.exchange(ID, &code, &verifier(), REDIRECT).unwrap();
    assert_eq!(t["scope"], "sales.user");
    let grants = r.o.grants_of("ada");
    assert_eq!(grants[0]["client_name"], "Example agent");
    // Read once, then from the cache for the hour (the exchange asked for the client again).
    assert_eq!(asked.lock().unwrap().as_slice(), [ID.to_string()]);
    // The refresh token works for the same address.
    r.refresh(ID, t["refresh_token"].as_str().unwrap()).unwrap();
    // The metadata says so.
    let m = with_cimd(json!({}), r.o.cimd_enabled());
    assert_eq!(m["client_id_metadata_document_supported"], true);
}

#[test]
fn without_the_operators_list_or_a_fetcher_an_address_is_no_client() {
    // No list: not even asked.
    let r = rig();
    let asked = serve(&r, doc(ID));
    assert!(!r.o.cimd_enabled());
    assert!(r.o.client(ID).is_none());
    assert!(asked.lock().unwrap().is_empty());
    assert_eq!(
        with_cimd(json!({}), false),
        json!({}),
        "the metadata is as before"
    );
    // A list and no fetcher: no client either.
    let r = rig_with(hosts("app.example"));
    assert!(!r.o.cimd_enabled());
    assert!(r.o.client(ID).is_none());
}

#[test]
fn only_https_addresses_with_a_path_on_a_listed_host_are_read() {
    let list = crate::parse_schemes("app.example,*.agents.example").unwrap();
    for (id, ok) in [
        (ID, true),
        ("https://x.agents.example/c.json", true),
        ("https://agents.example/c.json", false),
        ("https://evil.example/c.json", false),
        ("https://app.example.evil.example/c.json", false),
        ("http://app.example/c.json", false),
        ("https://app.example/", false),
        ("https://app.example", false),
        ("https://app.example:8443/c.json", false),
        ("https://u@app.example/c.json", false),
        ("https://app.example/c.json?x=1", false),
        ("https://app.example/c.json#f", false),
        ("https://app.example/a/../c.json", false),
        ("http://127.0.0.1:9/c.json", false),
    ] {
        assert_eq!(document_host(id, &list, false).is_some(), ok, "{id}");
    }
    // Plain http to a loopback number, for tests only, when the operator's list names it.
    let local = crate::parse_schemes("127.0.0.1").unwrap();
    assert!(document_host("http://127.0.0.1:9/c.json", &local, true).is_some());
    assert!(document_host("http://127.0.0.1:9/c.json", &local, false).is_none());
}

#[test]
fn a_document_that_does_not_describe_a_public_client_at_its_own_address_is_refused() {
    let bad = [
        doc("https://app.example/other.json"),
        {
            let mut d = doc(ID);
            d["client_secret"] = json!("s");
            d
        },
        {
            let mut d = doc(ID);
            d["token_endpoint_auth_method"] = json!("private_key_jwt");
            d
        },
        {
            let mut d = doc(ID);
            d["redirect_uris"] = json!(["http://evil.example/cb"]);
            d
        },
        {
            let mut d = doc(ID);
            d["redirect_uris"] = json!([]);
            d
        },
        {
            let mut d = doc(ID);
            d["client_name"] = json!("x".repeat(6000));
            d
        },
        json!(["not", "an", "object"]),
    ];
    for d in bad {
        let r = rig_with(hosts("app.example"));
        let asked = serve(&r, d.clone());
        assert!(r.o.client(ID).is_none(), "{d}");
        assert_eq!(asked.lock().unwrap().len(), 1);
    }
    // A redirect host off the operator's list of E1c is refused here too.
    let r = rig_with(OAuthConfig {
        mcp_redirect_hosts: crate::parse_schemes("localhost"),
        ..hosts("app.example")
    });
    serve(&r, doc(ID));
    assert!(r.o.client(ID).is_none());
}

#[test]
fn a_fetch_that_fails_is_no_client_and_fetches_are_capped_for_the_instance() {
    let r = rig_with(hosts("*.example"));
    let n = Arc::new(Mutex::new(0usize));
    let seen = n.clone();
    r.o.set_cimd_fetcher(Arc::new(move |_url: &str| {
        *seen.lock().unwrap() += 1;
        Err("connection refused".to_string())
    }));
    assert!(r.o.client(ID).is_none());
    for i in 0..100 {
        let _ = r.o.client(&format!("https://h{i}.example/c.json"));
    }
    assert_eq!(*n.lock().unwrap(), crate::cimd::FETCHES_PER_MINUTE as usize);
}
