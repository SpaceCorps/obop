//! The Microsoft step of a mobile app's sign-in (the phone leg of D91), at the level of the flow:
//! what the callback may write into a flow and what the return address reads back. The rig is the
//! real `Auth` on a memory database; the identity, which only the HTTP layer can verify, is made
//! by hand as `SsoFlowSignIn` from a real session of the account. The HTTP side, with the stand-in
//! provider, is `crates/obop/src/sso_tests/phone.rs`.

use super::*;
use crate::flow::{MAX_DAYS, MICROSOFT_ONLY_MAX_DAYS, MicrosoftOutcome, max_days};
use mobile::{APP, api};
use obop_commands::{LoginOutcome, SsoFlowSignIn};
use obop_storage::testing::Flaky;

fn begun(r: &Rig, mobile: bool) -> (String, String) {
    let (client, params) = if mobile {
        let c = r.mobile_client();
        let p = r.mobile_params(&c, APP);
        (c, p)
    } else {
        let c = r.client();
        let p = r.params(&c, REDIRECT);
        (c, p)
    };
    let _ = client;
    let v = r.o.validate_authorize(BASE, &params).unwrap();
    r.o.begin(&v, "10.0.0.2", "en").unwrap()
}

/// What the callback has of a sign-in of `user`: a session of that account (which the flow reads
/// and ends) and the epoch the account has now.
fn signed_in(r: &Rig, user: &str) -> SsoFlowSignIn {
    let outcome = r.auth.login_step(user, PW).unwrap();
    assert!(
        matches!(outcome, LoginOutcome::Session { .. }),
        "{outcome:?}"
    );
    SsoFlowSignIn {
        user: user.to_string(),
        outcome,
        epoch: r.o.tokens().credential_epoch(user).unwrap(),
        mfa: true,
        auth_time: Some(1_700_000_000),
    }
}

fn record(r: &Rig, id: &str) -> Option<J> {
    r.backend
        .kv_get(&format!("oauth:flow:{id}"))
        .unwrap()
        .map(|raw| serde_json::from_str(&raw).unwrap())
}

#[test]
fn the_callback_writes_the_person_and_the_epoch_the_sign_in_compared_and_nothing_of_the_provider() {
    let r = rig();
    r.auth
        .create_user("bob", PW, &["base.user".to_string()])
        .unwrap();
    let (id, binder) = begun(&r, true);
    // A failed password try for somebody else left its own read in the record.
    let csrf = r.o.view(&id, &binder).unwrap().csrf;
    assert_eq!(
        r.o.login(&id, &binder, &csrf, "10.0.0.2", "bob", "nope")
            .unwrap(),
        LoginOut::Failed("err.login")
    );
    assert_eq!(record(&r, &id).unwrap()["pre_user"], "bob");

    let mut s = signed_in(&r, "ada");
    s.epoch = Some("the-epoch-the-sign-in-compared".into());
    r.o.microsoft_done(&id, MicrosoftOutcome::SignedIn(s))
        .unwrap();
    let rec = record(&r, &id).unwrap();
    assert_eq!(rec["user"], "ada");
    assert_eq!(rec["epoch"], "the-epoch-the-sign-in-compared", "{rec}");
    assert_eq!(rec["pre_user"], "ada");
    assert_eq!(rec["pre_epoch"], "the-epoch-the-sign-in-compared");
    // Neither a claim nor the `mfa` flag nor a time is kept, and the session the sign-in made
    // is gone.
    let text = rec.to_string();
    assert!(
        !text.contains("1700000000") && !text.contains("mfa\""),
        "{text}"
    );
    assert!(
        r.backend
            .list("auth_token")
            .unwrap()
            .iter()
            .all(|(_, f)| f["revoked"] == true || f["kind"] != "session"),
        "the session of the sign-in was ended"
    );
    // The page shows the person.
    assert_eq!(r.o.view(&id, &binder).unwrap().user.as_deref(), Some("ada"));
}

#[test]
fn a_session_for_another_account_than_the_one_compared_is_no_sign_in() {
    let r = rig();
    r.auth
        .create_user("bob", PW, &["base.user".to_string()])
        .unwrap();
    let (id, binder) = begun(&r, true);
    let mut s = signed_in(&r, "ada");
    s.user = "bob".into();
    r.o.microsoft_done(&id, MicrosoftOutcome::SignedIn(s))
        .unwrap();
    let rec = record(&r, &id).unwrap();
    assert!(rec["user"].is_null(), "{rec}");
    assert_eq!(rec["ms_error"], "failed");
    assert_eq!(
        r.o.microsoft_return(&id, &binder, "10.0.0.2").unwrap(),
        LoginOut::Failed("err.ms_failed")
    );
}

#[test]
fn the_words_of_a_failed_step_are_a_closed_set() {
    let r = rig();
    let (id, binder) = begun(&r, true);
    for (given, kept, key) in [
        ("cancelled", "cancelled", "err.ms_cancelled"),
        ("unavailable", "unavailable", "err.ms_unavailable"),
        ("busy", "busy", "err.ms_busy"),
        ("failed", "failed", "err.ms_failed"),
        ("enrol", "enrol", "err.enrol"),
        ("pwchange", "pwchange", "err.pwchange"),
        // Anything else, whoever sent it, is "failed": it is not shown, stored or looked up.
        ("<script>alert(1)</script>", "failed", "err.ms_failed"),
        ("AADSTS70000 secret", "failed", "err.ms_failed"),
        ("", "failed", "err.ms_failed"),
    ] {
        r.o.microsoft_done(&id, MicrosoftOutcome::Failed(given.to_string()))
            .unwrap();
        assert_eq!(record(&r, &id).unwrap()["ms_error"], kept, "{given}");
        assert_eq!(
            r.o.microsoft_return(&id, &binder, "10.0.0.2").unwrap(),
            LoginOut::Failed(key),
            "{given}"
        );
    }
    assert_eq!(crate::microsoft_key("anything else"), "err.ms_failed");
}

#[test]
fn only_a_mobile_apps_flow_has_a_microsoft_step() {
    let r = rig();
    let (id, binder) = begun(&r, false);
    let before = record(&r, &id).unwrap();
    assert_eq!(r.o.microsoft_start(&id, &binder), Err(FlowError::Forbidden));
    assert_eq!(
        r.o.microsoft_done(&id, MicrosoftOutcome::Failed("failed".into())),
        Err(FlowError::Forbidden)
    );
    assert_eq!(
        r.o.microsoft_done(&id, MicrosoftOutcome::SignedIn(signed_in(&r, "ada"))),
        Err(FlowError::Forbidden)
    );
    assert_eq!(
        r.o.microsoft_return(&id, &binder, "10.0.0.2"),
        Err(FlowError::Forbidden)
    );
    assert_eq!(record(&r, &id).unwrap(), before, "nothing was written");
    // A mobile flow with a person on it has no Microsoft step to start either.
    let (id, binder) = begun(&r, true);
    assert!(r.o.microsoft_start(&id, &binder).is_ok());
    r.o.microsoft_done(&id, MicrosoftOutcome::SignedIn(signed_in(&r, "ada")))
        .unwrap();
    assert_eq!(r.o.microsoft_start(&id, &binder), Err(FlowError::Forbidden));
    // Another browser's secret does not open the flow.
    assert!(r.o.microsoft_start(&id, "not-the-binder").is_err());
}

#[test]
fn a_flow_that_is_gone_or_is_not_one_is_not_made_by_the_callback() {
    let r = rig();
    let (id, _) = begun(&r, true);
    r.backend.kv_delete(&format!("oauth:flow:{id}")).unwrap();
    let out = MicrosoftOutcome::Failed("failed".into());
    assert_eq!(r.o.microsoft_done(&id, out.clone()), Err(FlowError::Gone));
    assert!(record(&r, &id).is_none(), "not made again");
    for bad in [
        "",
        "x",
        "../../etc",
        &"a".repeat(31),
        &"g".repeat(32),
        &"a".repeat(33),
    ] {
        assert_eq!(r.o.microsoft_done(bad, out.clone()), Err(FlowError::Gone));
    }
}

#[test]
fn the_write_is_tried_again_after_a_change_and_gives_up_after_three() {
    let (flaky, backend) = Flaky::memory();
    let r = rig_on(backend, "ada", OAuthConfig::default());
    let (id, binder) = begun(&r, true);
    let key = format!("oauth:flow:{id}");
    // Two other requests change the record just before this write is tried, one after the other.
    interfere_on(&flaky, &key, 2);
    r.o.microsoft_done(&id, MicrosoftOutcome::SignedIn(signed_in(&r, "ada")))
        .unwrap();
    let rec = record(&r, &id).unwrap();
    assert_eq!(rec["user"], "ada");
    assert_eq!(
        rec["marker"], 1,
        "the changes of the others are kept: {rec}"
    );

    // Three changes in a row: the callback gives up, and writes nothing over them.
    let (id, _) = begun(&r, true);
    let key = format!("oauth:flow:{id}");
    interfere_on(&flaky, &key, 3);
    assert_eq!(
        r.o.microsoft_done(&id, MicrosoftOutcome::SignedIn(signed_in(&r, "ada"))),
        Err(FlowError::Busy(1))
    );
    let rec = record(&r, &id).unwrap();
    assert!(rec["user"].is_null() && rec["marker"] == 1, "{rec}");
    let _ = binder;
}

fn interfere_on(flaky: &Arc<Flaky>, key: &str, left: u32) {
    if left == 0 {
        return;
    }
    let again = flaky.clone();
    let key = key.to_string();
    flaky.before_the_next_kv_swap(Box::new(move |b| {
        let raw = b.kv_get(&key).unwrap().unwrap();
        let mut rec: J = serde_json::from_str(&raw).unwrap();
        rec["marker"] = json!(left);
        b.kv_put(&key, &rec.to_string(), Some(600_000)).unwrap();
        interfere_on(&again, &key, left - 1);
    }));
}

#[test]
fn the_return_address_reads_and_writes_nothing() {
    let r = rig();
    let (id, binder) = begun(&r, true);
    let raw = |r: &Rig| r.backend.kv_get(&format!("oauth:flow:{id}")).unwrap();
    // Before anything: a general text.
    let before = raw(&r);
    for _ in 0..2 {
        assert_eq!(
            r.o.microsoft_return(&id, &binder, "10.0.0.2").unwrap(),
            LoginOut::Failed("err.ms_failed")
        );
    }
    assert_eq!(raw(&r), before);
    // After a word, and after a person.
    r.o.microsoft_done(&id, MicrosoftOutcome::Failed("cancelled".into()))
        .unwrap();
    let before = raw(&r);
    for _ in 0..3 {
        assert_eq!(
            r.o.microsoft_return(&id, &binder, "10.0.0.2").unwrap(),
            LoginOut::Failed("err.ms_cancelled")
        );
    }
    assert_eq!(raw(&r), before);
    r.o.microsoft_done(&id, MicrosoftOutcome::SignedIn(signed_in(&r, "ada")))
        .unwrap();
    let before = raw(&r);
    for _ in 0..3 {
        assert_eq!(
            r.o.microsoft_return(&id, &binder, "10.0.0.2").unwrap(),
            LoginOut::Consent
        );
    }
    assert_eq!(raw(&r), before);
    // Another browser is shown nothing.
    assert!(
        r.o.microsoft_return(&id, "not-the-binder", "10.0.0.2")
            .is_err()
    );
}

#[test]
fn the_cap_of_a_microsoft_only_phone_is_30_days_and_every_other_grant_keeps_90() {
    assert_eq!(max_days(true), 30);
    assert_eq!(max_days(true), MICROSOFT_ONLY_MAX_DAYS);
    assert_eq!(max_days(false), 90);
    assert_eq!(max_days(false), MAX_DAYS);
    // A page of a flow nobody is signed in on, and of an agent's connector, offers all four.
    let r = rig();
    let (id, binder) = begun(&r, true);
    assert_eq!(r.o.view(&id, &binder).unwrap().max_days, 90);
    let (id, binder) = begun(&r, false);
    assert_eq!(r.o.view(&id, &binder).unwrap().max_days, 90);
    // A signed-in person who signs in with a password: 90.
    let (id, binder) = begun(&r, true);
    r.o.microsoft_done(&id, MicrosoftOutcome::SignedIn(signed_in(&r, "ada")))
        .unwrap();
    assert_eq!(r.o.view(&id, &binder).unwrap().max_days, 90);
    let _ = api();
}

#[test]
fn a_refused_read_of_the_account_gives_no_page_and_is_not_read_as_no_cap() {
    // The page of a signed-in person lists up to the cap of the account. When the store refuses
    // the read of the account, no page comes out: the cap is not guessed, and "unknown" is not
    // "not capped". (The HTTP layer turns a refused read into a 503 as well, so only this level
    // tells the difference.)
    let (flaky, backend) = Flaky::memory();
    let r = rig_on(backend, "ada", OAuthConfig::default());
    let (id, binder) = begun(&r, true);
    r.o.microsoft_done(&id, MicrosoftOutcome::SignedIn(signed_in(&r, "ada")))
        .unwrap();
    assert_eq!(r.o.view(&id, &binder).unwrap().max_days, 90);
    flaky.refuse_model("auth_user", obop_storage::testing::Mode::Reads);
    assert!(
        r.o.view(&id, &binder).is_err(),
        "a page after a refused read of the account"
    );
    flaky.allow_models();
    assert_eq!(r.o.view(&id, &binder).unwrap().max_days, 90);
}

/// The same property without a clock. What keeps the lifetime of the row is that the callback
/// writes with a compare-and-set (`kv_swap`), which the stores implement as "the value changes,
/// the expiry stays". Any write that sets an expiry, to renew it or to shorten it, is a `kv_put`
/// (or a `kv_put_new`): with those refused, the callback must still do all of its work, and must
/// not have tried to make one. (`the_callback_keeps_the_lifetime_of_the_flow_and_does_not_renew_it`
/// below is the same property with real time, on the memory backend.)
#[test]
fn the_callback_writes_with_a_compare_and_set_and_never_with_a_put() {
    let (flaky, backend) = Flaky::memory();
    let r = rig_on(backend, "ada", OAuthConfig::default());
    let (id, binder) = begun(&r, true);
    // The control: a write of the pages (this one is "Not you?") is a put, and is refused.
    flaky.refuse_kv_put(1);
    flaky.refuse_kv_new(1);
    let csrf = record(&r, &id).unwrap()["csrf"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(r.o.switch_user(&id, &binder, &csrf).is_err());
    assert_eq!(flaky.refused(), 1, "the control did not meet the refusal");
    flaky.refuse_kv_put(0);
    flaky.refuse_kv_new(0);
    // Now every put is refused for the callback: all three of its outcomes still get written.
    let signed = signed_in(&r, "ada");
    flaky.refuse_kv_put(1_000);
    flaky.refuse_kv_new(1_000);
    let before = flaky.refused();
    r.o.microsoft_done(&id, MicrosoftOutcome::Failed("cancelled".into()))
        .unwrap();
    assert_eq!(record(&r, &id).unwrap()["ms_error"], "cancelled");
    r.o.microsoft_done(&id, MicrosoftOutcome::SignedIn(signed))
        .unwrap();
    assert_eq!(record(&r, &id).unwrap()["user"], "ada");
    r.o.microsoft_done(&id, MicrosoftOutcome::Failed("failed".into()))
        .unwrap();
    assert_eq!(
        flaky.refused(),
        before,
        "the callback tried to write the flow with a put (which sets a lifetime)"
    );
    flaky.refuse_kv_put(0);
    flaky.refuse_kv_new(0);
}

#[test]
fn the_callback_keeps_the_lifetime_of_the_flow_and_does_not_renew_it() {
    // A flow lives for its lifetime from the last write by the sign-in pages; the callback's
    // write is a compare-and-set, which keeps the expiry of the row. The lifetime here is three
    // seconds: the callback writes after one and a half, and at three and a half the flow is
    // gone, as it would be if nothing had written (a write that renewed the row would leave it
    // alive until four and a half).
    let r = rig_with(OAuthConfig {
        flow_ttl_secs: 3,
        ..OAuthConfig::default()
    });
    let (id, binder) = begun(&r, true);
    std::thread::sleep(std::time::Duration::from_millis(1500));
    r.o.microsoft_done(&id, MicrosoftOutcome::SignedIn(signed_in(&r, "ada")))
        .unwrap();
    assert!(
        r.o.view(&id, &binder).is_ok(),
        "the flow is there after the write"
    );
    std::thread::sleep(std::time::Duration::from_millis(2000));
    assert!(
        r.o.view(&id, &binder).is_err(),
        "the callback renewed the lifetime of the flow"
    );
    assert!(record(&r, &id).is_none());
    // And a callback that comes after the flow has gone does not make it again.
    assert!(
        r.o.microsoft_done(&id, MicrosoftOutcome::Failed("failed".into()))
            .is_err()
    );
    assert!(record(&r, &id).is_none());
}

/// A store that cannot answer when the return address asks for the second step again is a failure
/// the person can try again ("unavailable"), not the general failure of a sign-in that is over.
#[test]
fn a_store_that_cannot_answer_at_the_reload_of_a_pending_second_step_is_unavailable() {
    let (flaky, backend) = Flaky::memory();
    let r = rig_on(backend, "ada", OAuthConfig::default());
    let (id, binder) = begun(&r, true);
    // The callback left a second step pending for ada (written by hand: the account has no
    // binding in this rig, so the real callback would not have).
    let key = format!("oauth:flow:{id}");
    let mut rec = record(&r, &id).unwrap();
    rec["mfa_pending"] = json!({"user": "ada", "epoch": null});
    r.backend
        .kv_put(&key, &rec.to_string(), Some(600_000))
        .unwrap();
    flaky.refuse_model("auth_user", obop_storage::testing::Mode::Reads);
    assert_eq!(
        r.o.microsoft_return(&id, &binder, "10.0.0.2").unwrap(),
        LoginOut::Failed("err.ms_unavailable")
    );
    flaky.allow_models();
    // With the store well, the same record is a refusal (the account has no binding): the general
    // failure, so the two words are not the same answer.
    assert_eq!(
        r.o.microsoft_return(&id, &binder, "10.0.0.2").unwrap(),
        LoginOut::Failed("err.ms_failed")
    );
}
