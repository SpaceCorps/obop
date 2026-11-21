//! N20 (m): which sort of anonymous line a call site names, pinned where it was not. With the budget
//! of the noise (callbacks whose state is no flow of this server) used up, a failure of each of these
//! call sites is still written: it is counted against the budget of the failures. A site that named
//! the sort of the noise (`NoFlow`) would find that budget used up and write nothing. The sites:
//! the discovery document at the start and at the callback, the key set at its first fetch, a
//! provider switched off between the start and the callback, and a token endpoint that cannot be
//! reached.

use super::*;

/// The noise of an hour: seventy callbacks with a made-up state from seventy addresses.
async fn use_up_the_noise(w: &World) {
    let ceiling = obop_auth::oidc::ANON_LINES_PER_HOUR as usize;
    for i in 0..70 {
        let ip = format!("203.0.113.{i}");
        let state = format!("{:064x}", i + 1);
        let r = w
            .send_h(
                "GET",
                &format!("/api/auth/oidc/callback?code=x&state={state}"),
                &Jar::default(),
                None,
                &[("x-forwarded-for", ip.as_str())],
            )
            .await;
        failed(&r, "failed");
    }
    let noise = w
        .events("oidc_refused")
        .into_iter()
        .filter(|l| l.contains("unknown, already used or expired"))
        .count();
    assert_eq!(noise, ceiling, "the noise budget is used up");
}

/// The callback of a sign-in begun at `ip`, after `meanwhile` changed the world.
async fn come_back(w: &World, ip: &str, meanwhile: impl FnOnce()) -> Raw {
    let h = [("x-forwarded-for", ip)];
    let (jar, to_provider) = begin_at(w, ip).await;
    meanwhile();
    let uri = w.idp.authorize(&to_provider, ADA_OID);
    w.send_h("GET", &uri, &jar, None, &h).await
}

#[tokio::test]
async fn failures_each_call_site_names_the_sort_of_a_failure_and_not_of_the_noise() {
    let w = world(true).await;
    w.keep_throttle();
    use_up_the_noise(&w).await;
    let mut missing = Vec::new();
    let mut expect = |w: &World, kind: &str, text: &str, site: &str| {
        let n = w.events(kind).iter().filter(|l| l.contains(text)).count();
        if n != 1 {
            missing.push(format!("{site}: {n} lines of {kind} {text:?}"));
        }
    };

    // The discovery document at the start.
    w.idp.st.lock().unwrap().discovery_status = 503;
    w.state.forget_sso_cache_for_test();
    let r = w
        .send_h(
            "GET",
            "/api/auth/oidc/start",
            &Jar::default(),
            None,
            &[("x-forwarded-for", "192.0.2.10")],
        )
        .await;
    failed(&r, "unavailable");
    expect(
        &w,
        "oidc_error",
        "start: the discovery document could not be used",
        "Discovery (start)",
    );

    // The discovery document at the callback: the cache is forgotten between the two.
    w.idp.reset();
    w.state.forget_sso_cache_for_test();
    let r = come_back(&w, "192.0.2.11", || {
        w.idp.st.lock().unwrap().discovery_status = 503;
        w.state.forget_sso_cache_for_test();
    })
    .await;
    failed(&r, "unavailable");
    expect(
        &w,
        "oidc_error",
        "callback: the discovery document could not be used",
        "Discovery (callback)",
    );

    // The key set at its first fetch.
    w.idp.reset();
    w.state.forget_sso_cache_for_test();
    let r = come_back(&w, "192.0.2.12", || {
        w.idp.st.lock().unwrap().jwks_status = 503;
    })
    .await;
    failed(&r, "unavailable");
    expect(
        &w,
        "oidc_error",
        "key set could not be fetched",
        "KeySet (first fetch)",
    );

    // The provider switched off between the start and the callback.
    w.idp.reset();
    w.state.forget_sso_cache_for_test();
    let r = come_back(&w, "192.0.2.13", || {
        w.auth
            .operator_sso_configure(TENANT, CLIENT, None, Some(false))
            .unwrap();
    })
    .await;
    failed(&r, "failed");
    expect(
        &w,
        "oidc_refused",
        "switched off during the sign-in",
        "SwitchedOff",
    );
    w.auth
        .operator_sso_configure(TENANT, CLIENT, None, Some(true))
        .unwrap();

    // The token endpoint cannot be reached: the provider goes away after the start (the
    // discovery document is in the cache by then). Last, since the stand-in does not come back.
    w.state.forget_sso_cache_for_test();
    let r = come_back(&w, "192.0.2.14", || w.idp.stop()).await;
    failed(&r, "unavailable");
    expect(
        &w,
        "oidc_error",
        "the token endpoint could not be reached",
        "TokenUnreachable",
    );

    assert!(
        missing.is_empty(),
        "a failure line that the noise budget swallowed (its call site names the wrong sort):\n{}",
        missing.join("\n")
    );
}
