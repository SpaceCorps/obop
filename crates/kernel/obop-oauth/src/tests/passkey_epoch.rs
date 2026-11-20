//! L19: a sign-in by passkey alone on a phone's flow keeps, as its baseline, the credential epoch
//! the check of the assertion compared. A change of credentials that commits after that check,
//! wherever it lands before the flow is seated, ends the sign-in: no device and no token stay in
//! force for a credential that had just changed.

use super::mobile::{APP, DEVICE, live_mobile_tokens};
use super::*;
use obop_auth::webauthn::soft::SoftAuthenticator;
use obop_commands::{LoginOutcome, PasskeyApi, Reauth, WebauthnCtx};
use obop_storage::testing::Flaky;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const ORIGIN: &str = "https://obop.test";

fn ctx() -> WebauthnCtx {
    WebauthnCtx {
        rp_id: "obop.test".into(),
        origin: ORIGIN.into(),
        rp_name: "obop".into(),
        ip: "203.0.113.9".into(),
    }
}

fn session_of(r: &Rig) -> (String, obop_commands::Principal) {
    let LoginOutcome::Session { token, .. } = r.auth.login_step(&r.user, PW).unwrap() else {
        panic!("expected a session");
    };
    let p = r.auth.resolve(&token).unwrap();
    (token, p)
}

/// The person has a passkey that may sign in alone. Returns it, and the session that set it up.
fn passwordless(r: &Rig) -> (SoftAuthenticator, String, obop_commands::Principal) {
    let (token, p) = session_of(r);
    let pw = Reauth {
        password: PW,
        ..Default::default()
    };
    let mut auth = SoftAuthenticator::es256();
    let opts = r.auth.register_options(&p, &ctx(), &pw).unwrap();
    let cred = auth.create(&opts, ORIGIN);
    r.auth
        .register_finish(&p, &token, &ctx(), "Phone", &cred)
        .unwrap();
    let opts = r.auth.proof_options(&p, &ctx()).unwrap();
    r.auth
        .proof(&p, &ctx(), &auth.get(&opts, ORIGIN))
        .unwrap();
    r.auth.set_passwordless(&p, true, &pw).unwrap();
    (auth, token, p)
}

#[test]
fn a_credential_change_after_the_check_of_a_passkey_alone_ends_the_phones_sign_in() {
    let mut in_window = 0;
    let mut runs_that_fired = 0;
    for after in 0..12 {
        let (flaky, backend) = Flaky::memory();
        let r = rig_on(backend, "ada", OAuthConfig::default());
        let (mut key, _, _) = passwordless(&r);
        let c = r.mobile_client();
        let v = r.o.validate_authorize(BASE, &r.mobile_params(&c, APP)).unwrap();
        let (id, binder) = r.o.begin(&v, "10.0.0.2", "en").unwrap();
        let csrf = r.o.view_plain(&id, &binder).unwrap().csrf;
        let opts = r
            .o
            .passkey_options(&ctx(), &id, &binder, &csrf, None)
            .unwrap();
        let cred = key.get(&opts, ORIGIN);
        // The change: the credential epoch moves (as a new password, a reset or a passkey removed
        // on another device moves it), right after the `after`-th read of the account by the
        // passkey step.
        let fired = Arc::new(AtomicUsize::new(0));
        let checked = Arc::new(AtomicBool::new(false));
        let (seen, past_check, events) = (
            fired.clone(),
            checked.clone(),
            r.backend.clone(),
        );
        flaky.after_the_get_of(
            "auth_user",
            after,
            Box::new(move |b| {
                // The assertion has been checked once its sign-in is in the log.
                let logged = events
                    .list("security_event")
                    .unwrap()
                    .iter()
                    .any(|(_, f)| f["kind"] == "passkey_login");
                past_check.store(logged, Ordering::SeqCst);
                let mut f = obop_storage::Fields::new();
                f.insert("epoch".into(), json!("changed-elsewhere"));
                b.update("auth_user", "ada", &f).unwrap();
                seen.fetch_add(1, Ordering::SeqCst);
            }),
        );
        let step = r
            .o
            .login_passkey(&ctx(), &id, &binder, &csrf, None, &cred);
        let fired_in_step = fired.load(Ordering::SeqCst) > 0;
        flaky.allow_models();
        if !fired_in_step {
            // The step reads the account fewer times than that: an ordinary sign-in.
            assert_eq!(step, Ok(LoginOut::Consent), "after read {after}");
            continue;
        }
        runs_that_fired += 1;
        if !checked.load(Ordering::SeqCst) {
            // The change came before the assertion was checked: the sign-in that follows is one
            // made after the change, and may finish.
            continue;
        }
        in_window += 1;
        // The change came after the check. The step may still show the consent page (the change
        // landed after the last read the step compares), but nothing may come of it.
        if step != Ok(LoginOut::Consent) {
            assert!(
                matches!(step, Ok(LoginOut::Failed(_))),
                "after read {after}: {step:?}"
            );
            continue;
        }
        let g = vec!["base.user".to_string()];
        let out = r.o.decide(BASE, &id, &binder, &csrf, true, &g, 30);
        if let Ok(DecideOut::Redirect(loc)) = out {
            let code = query(&loc, "code");
            let t = r.mobile_token(&c, &code, &verifier(), APP, &DEVICE);
            assert!(t.is_err(), "after read {after}: a token for a changed credential");
        } else {
            assert_eq!(out, Err(FlowError::Gone), "after read {after}");
        }
        assert_eq!(live_mobile_tokens(&r), 0, "after read {after}");
        assert!(
            r.devices().iter().all(|(_, d)| d["state"] != "active"),
            "after read {after}: {:?}",
            r.devices()
        );
    }
    assert!(runs_that_fired > 0, "the change never fired");
    assert!(in_window > 0, "no change landed after the check");
}
