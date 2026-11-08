//! The enrolment texts end to end (docs/spec/branding.md, slice S1e, with the "Fail closed",
//! "Issuer" and "Passkey" bullets of section 13): the real app, the real router and the real `Auth`,
//! over a store behind the `Flaky` double. TOTP enrolment (`/api/auth/mfa/begin`) writes the label
//! and `issuer=` of an authenticator entry, the passkey registration options
//! (`/api/auth/passkeys/register/options`) write the relying party name; both are written into the
//! person's own device and cannot be corrected, so both read the name from the row alone and refuse
//! when they cannot: a transient fault as D74 answers it (503 with `Retry-After`), anything else
//! (another read error, a stored name that cannot be shown) as 503 `brand_unavailable` with no
//! `Retry-After`, and nothing is written in either case.
//!
//! What an instance with **no** row sends is held by the pins of `passkey_tests` and `mfa_tests`
//! (`otpauth://totp/obop:...&issuer=obop`, `rp.name` `obop`): those outputs did not move. Four
//! assertions about such an instance moved, by design (docs/spec/branding.md, section 5 and 15.4,
//! item 5), and are not hidden by that sentence: `mfa_tests::pins_the_default_issuer_of_a_server`
//! held `AuthConfig::issuer` as the text `obop` and holds `None` (the enrolment output it pinned is
//! asserted beside it, unchanged); the status text of an instance with no brand (`brand_cmd_tests`)
//! gains the line `authenticator_issuer: "obop" (default)`; the answer of `get` with no row gains the
//! entry `authenticator_issuer` (`obop-commands`, `brand/tests/effective.rs`, 4 + 14 keys became
//! 5 + 14); and the two `with_a_brand_row_*` tests assert the name where they held `obop` with a row.

use crate::app::App;
use crate::brand_row_tests::set_brand;
use crate::mfa_tests::{PW, T0};
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use obop_auth::webauthn::soft::SoftAuthenticator;
use obop_auth::{Auth, AuthConfig, totp};
use obop_commands::Store;
use obop_commands::brand::{self, Action, UNREADABLE};
use obop_storage::testing::{Flaky, Mode};
use obop_storage::{Fields, Op};
use serde_json::{Value as J, json};
use std::sync::Arc;
use tower::ServiceExt;

const ORIGIN: &str = "https://inst.test";
const BEGIN: &str = "/api/auth/mfa/begin";
const OPTIONS: &str = "/api/auth/passkeys/register/options";

struct Rig {
    r: axum::Router,
    auth: Arc<Auth>,
    app: App,
    flaky: Arc<Flaky>,
}

/// An app over a store that can be told to fail, a real `Auth` with this config and the router. The
/// users: `root` (an administrator), `ada`, `bob`, `cy`.
fn rig(config: AuthConfig) -> Rig {
    let (flaky, backend) = Flaky::memory();
    let app = App::with_store(Store::over(backend));
    let auth = Arc::new(Auth::with_test_config(app.store.backend(), config));
    auth.set_clock_for_test(T0);
    for (name, groups) in [
        ("root", vec!["platform.admin"]),
        ("ada", vec!["base.user"]),
        ("bob", vec!["base.user"]),
        ("cy", vec!["base.user"]),
    ] {
        auth.create_user(
            name,
            PW,
            &groups.into_iter().map(String::from).collect::<Vec<_>>(),
        )
        .unwrap();
    }
    let r = obop_http::router(
        obop_http::HttpState::new(app.dispatcher.clone(), auth.clone()),
        None,
    );
    Rig {
        r,
        auth,
        app,
        flaky,
    }
}

struct Resp {
    status: u16,
    headers: axum::http::HeaderMap,
    json: J,
}

impl Resp {
    fn header(&self, name: &str) -> &str {
        self.headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    }
}

/// A request as a browser behind a TLS proxy sends it: host `inst.test`, forwarded protocol https.
async fn call(r: &axum::Router, method: &str, path: &str, token: Option<&str>, body: J) -> Resp {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "inst.test")
        .header("x-forwarded-proto", "https")
        .header("content-type", "application/json");
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let resp = r
        .clone()
        .oneshot(req.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let (status, headers) = (resp.status().as_u16(), resp.headers().clone());
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    Resp {
        status,
        headers,
        json: serde_json::from_slice(&bytes).unwrap_or(J::Null),
    }
}

async fn post(r: &axum::Router, token: Option<&str>, path: &str, body: J) -> Resp {
    call(r, "POST", path, token, body).await
}

async fn sign_in(r: &axum::Router, user: &str) -> String {
    let b = post(
        r,
        None,
        "/api/auth/login",
        json!({"username": user, "password": PW}),
    )
    .await;
    assert_eq!(b.status, 200, "{}", b.json);
    b.json["token"]
        .as_str()
        .unwrap_or_else(|| panic!("no session: {}", b.json))
        .to_string()
}

/// The body each enrolment route is sent.
fn body_of(path: &str) -> J {
    if path == OPTIONS {
        json!({"password": PW})
    } else {
        json!({})
    }
}

/// A row as it is, whatever it holds: written by a restore, an import or a direct edit.
fn write_row(rig: &Rig, pairs: &[(&str, J)]) {
    let fields: Fields = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    rig.app
        .store
        .backend()
        .apply_checked(&[Op::Put {
            model: brand::MODEL.into(),
            id: brand::ROW_ID.into(),
            fields,
        }])
        .unwrap();
}

/// The challenges of passkey ceremonies in the store (the prefix of `passkey.rs`).
fn challenges(rig: &Rig) -> Vec<(String, String)> {
    rig.app.store.backend().kv_scan("auth.wa:").unwrap()
}

/// What an enrolment must have left behind when it was refused: no pending secret for the person,
/// no challenge of a ceremony in the store.
async fn nothing_was_written(rig: &Rig, token: &str) {
    let st = call(&rig.r, "GET", "/api/auth/mfa", Some(token), J::Null).await;
    assert_eq!(st.status, 200, "{}", st.json);
    assert_eq!(st.json["pending"], json!(false), "{}", st.json);
    let challenges = challenges(rig);
    assert!(challenges.is_empty(), "{challenges:?}");
}

/// A refusal of the kind D74 answers: 503 with `Retry-After`, and not the answer of the branding.
fn is_the_transient_503(r: &Resp, what: &str) {
    assert_eq!(r.status, 503, "{what}: {}", r.json);
    assert!(
        !r.header("retry-after").is_empty(),
        "{what}: a 503 of a transient fault says when to retry"
    );
    assert_ne!(r.json["code"], json!("brand_unavailable"), "{what}");
    assert!(
        !r.json.to_string().contains("OBOP_SECRET_KEY"),
        "{what}: {}",
        r.json
    );
}

/// The answer for a name that could not be read: 503, the code `brand_unavailable`, a hint that
/// names the branding and the command that shows what is wrong, no `Retry-After`.
fn is_brand_unavailable(r: &Resp, what: &str) {
    assert_eq!(r.status, 503, "{what}: {}", r.json);
    assert_eq!(r.json["code"], json!("brand_unavailable"), "{what}");
    assert_eq!(
        r.header("retry-after"),
        "",
        "{what}: waiting does not repair a stored name"
    );
    let hint = r.json["hint"].as_str().unwrap();
    assert!(hint.contains("obop admin brand status"), "{what}: {hint}");
    assert!(
        !hint.contains("OBOP_SECRET_KEY"),
        "{what}: the wrong cause: {hint}"
    );
    assert!(
        r.json["error"].as_str().unwrap().starts_with(UNREADABLE),
        "{what}: {}",
        r.json
    );
}

// ---- The name reaches the device ------------------------------------------------------------------

/// With a brand set the TOTP enrolment names the product in the label and in `issuer=`, and the
/// passkey registration options name it as the relying party; with no row they say `obop`.
#[tokio::test]
async fn the_enrolments_carry_the_name_of_the_record() {
    let rig = rig(AuthConfig::insecure_dev());
    let t = sign_in(&rig.r, "ada").await;
    // No row: what they always said.
    assert!(challenges(&rig).is_empty());
    let none = post(&rig.r, Some(&t), OPTIONS, body_of(OPTIONS)).await;
    assert_eq!(none.status, 200, "{}", none.json);
    assert_eq!(none.json["publicKey"]["rp"]["name"], json!("obop"));
    // The positive control of `nothing_was_written`: a registration that goes through does store
    // the challenge of its ceremony, so an empty scan after a refusal means something.
    assert_eq!(challenges(&rig).len(), 1, "{:?}", challenges(&rig));
    let cy = sign_in(&rig.r, "cy").await;
    let begun = post(&rig.r, Some(&cy), BEGIN, json!({})).await;
    assert_eq!(begun.status, 200, "{}", begun.json);
    let secret = begun.json["secret"].as_str().unwrap();
    assert_eq!(
        begun.json["uri"]
            .as_str()
            .unwrap()
            .replace(secret, "SECRET"),
        "otpauth://totp/obop:cy?secret=SECRET&issuer=obop&algorithm=SHA1&digits=6&period=30"
    );

    // A row: the name, in the same two places, from the next enrolment on.
    set_brand(&rig.app);
    let opts = post(&rig.r, Some(&t), OPTIONS, body_of(OPTIONS)).await;
    assert_eq!(opts.status, 200, "{}", opts.json);
    assert_eq!(opts.json["publicKey"]["rp"]["name"], json!("MOHV"));
    assert_eq!(opts.json["publicKey"]["rp"]["id"], json!("inst.test"));
    assert_eq!(
        opts.json["publicKey"]["rp"]
            .as_object()
            .map(|o| o.keys().cloned().collect::<Vec<_>>()),
        Some(vec!["id".to_string(), "name".to_string()]),
        "the relying party carries an id and a name and nothing else"
    );
    let begun = post(&rig.r, Some(&cy), BEGIN, json!({})).await;
    assert_eq!(begun.status, 200, "{}", begun.json);
    let secret = begun.json["secret"].as_str().unwrap();
    assert_eq!(
        begun.json["uri"]
            .as_str()
            .unwrap()
            .replace(secret, "SECRET"),
        "otpauth://totp/MOHV:cy?secret=SECRET&issuer=MOHV&algorithm=SHA1&digits=6&period=30"
    );
    assert_eq!(begun.json["issuer"], json!("MOHV"));
}

/// A change made while the server runs applies to the next enrolment at once: the name is read from
/// the row every time, not from the two-second cache the pages use. Here the change is a `configure`
/// through the command line's path (no reader is told), so the reader's cell still holds the old
/// name when the enrolment begins.
#[tokio::test]
async fn a_change_made_by_another_process_applies_to_the_next_enrolment() {
    let rig = rig(AuthConfig::insecure_dev());
    let t = sign_in(&rig.r, "ada").await;
    set_brand(&rig.app);
    // Fill the cell with the old name.
    assert_eq!(rig.app.dispatcher.brand().current().brand().name(), "MOHV");
    brand::apply(
        &*rig.app.store.backend(),
        None,
        "operator",
        &Action::Configure(vec![("name".into(), "Other".into())]),
        false,
    )
    .unwrap();
    assert_eq!(
        rig.app.dispatcher.brand().current().brand().name(),
        "MOHV",
        "the cache has not caught up"
    );
    let opts = post(&rig.r, Some(&t), OPTIONS, body_of(OPTIONS)).await;
    assert_eq!(opts.json["publicKey"]["rp"]["name"], json!("Other"));
    let cy = sign_in(&rig.r, "cy").await;
    let begun = post(&rig.r, Some(&cy), BEGIN, json!({})).await;
    assert_eq!(begun.json["issuer"], json!("Other"));
}

/// `OBOP_MFA_ISSUER` (the config's issuer) beats the record for the authenticator label, also when
/// it says `obop`; it does not reach the passkey name.
#[tokio::test]
async fn the_variable_beats_the_record_for_the_authenticator_and_does_not_reach_the_passkey() {
    for (variable, want) in [("Acme", "Acme"), ("obop", "obop")] {
        let rig = rig(AuthConfig {
            issuer: Some(variable.into()),
            ..AuthConfig::insecure_dev()
        });
        set_brand(&rig.app);
        let cy = sign_in(&rig.r, "cy").await;
        let begun = post(&rig.r, Some(&cy), BEGIN, json!({})).await;
        assert_eq!(begun.status, 200, "{}", begun.json);
        assert_eq!(begun.json["issuer"], json!(want));
        let secret = begun.json["secret"].as_str().unwrap();
        assert_eq!(
            begun.json["uri"]
                .as_str()
                .unwrap()
                .replace(secret, "SECRET"),
            format!(
                "otpauth://totp/{want}:cy?secret=SECRET&issuer={want}&algorithm=SHA1&digits=6&period=30"
            )
        );
        let t = sign_in(&rig.r, "ada").await;
        let opts = post(&rig.r, Some(&t), OPTIONS, body_of(OPTIONS)).await;
        assert_eq!(opts.json["publicKey"]["rp"]["name"], json!("MOHV"));
    }
}

// ---- Fail closed ----------------------------------------------------------------------------------

/// A transient fault of the store while the name is read: both enrolments answer as D74 does, 503
/// with `Retry-After`, never with the branding answer, and write nothing; when the store is back the
/// same session enrols with the name.
#[tokio::test]
async fn a_transient_fault_is_the_503_with_retry_after_and_nothing_is_written() {
    for path in [BEGIN, OPTIONS] {
        let rig = rig(AuthConfig::insecure_dev());
        set_brand(&rig.app);
        let t = sign_in(&rig.r, "ada").await;
        rig.flaky.refuse_model(brand::MODEL, Mode::Reads);
        let r = post(&rig.r, Some(&t), path, body_of(path)).await;
        is_the_transient_503(&r, path);
        rig.flaky.allow_models();
        nothing_was_written(&rig, &t).await;
        let again = post(&rig.r, Some(&t), path, body_of(path)).await;
        assert_eq!(again.status, 200, "{path}: {}", again.json);
        if path == OPTIONS {
            assert_eq!(again.json["publicKey"]["rp"]["name"], json!("MOHV"));
        } else {
            assert_eq!(again.json["issuer"], json!("MOHV"));
        }
    }
}

/// A read error that is not a transient fault is the answer of the branding, and it is not taken
/// for a missing server key.
#[tokio::test]
async fn another_read_error_is_brand_unavailable_and_nothing_is_written() {
    for path in [BEGIN, OPTIONS] {
        let rig = rig(AuthConfig::insecure_dev());
        set_brand(&rig.app);
        let t = sign_in(&rig.r, "ada").await;
        rig.flaky
            .fail_gets_of(brand::MODEL, "a read error that is not transient");
        let r = post(&rig.r, Some(&t), path, body_of(path)).await;
        is_brand_unavailable(&r, path);
        assert!(
            !r.json.to_string().contains("not transient"),
            "no text of the store: {}",
            r.json
        );
        rig.flaky.allow_models();
        nothing_was_written(&rig, &t).await;
    }
}

/// A stored name that cannot be shown is the same answer, for every kind of unshowable name.
#[tokio::test]
async fn a_stored_name_that_cannot_be_shown_is_brand_unavailable_and_nothing_is_written() {
    for bad in [
        json!("<b>x"),
        json!(""),
        json!("a\u{200b}b"),
        json!("a\u{202e}b"),
        json!("x".repeat(41)),
        json!(5),
    ] {
        for path in [BEGIN, OPTIONS] {
            let rig = rig(AuthConfig::insecure_dev());
            write_row(&rig, &[("name", bad.clone())]);
            let t = sign_in(&rig.r, "ada").await;
            let r = post(&rig.r, Some(&t), path, body_of(path)).await;
            is_brand_unavailable(&r, &format!("{path} {bad}"));
            assert!(
                !r.json.to_string().contains("<b>"),
                "no stored text in the answer: {}",
                r.json
            );
            nothing_was_written(&rig, &t).await;
        }
    }
}

/// A stored name that the display accepts and `configure` refuses (a link character, or only white
/// space; it can only have been written around the command) cannot go into a device's label either:
/// the same answer on both enrolments, nothing stored, and none of the stored text in the answer.
#[tokio::test]
async fn a_stored_name_with_a_link_character_or_only_spaces_is_brand_unavailable_too() {
    for bad in [
        "a:b",
        "javascript:alert(1)",
        "http://evil.test/x",
        "a/b",
        "a@b",
        "   ",
    ] {
        for path in [BEGIN, OPTIONS] {
            let rig = rig(AuthConfig::insecure_dev());
            write_row(&rig, &[("name", json!(bad))]);
            let t = sign_in(&rig.r, "ada").await;
            let r = post(&rig.r, Some(&t), path, body_of(path)).await;
            is_brand_unavailable(&r, &format!("{path} {bad:?}"));
            assert!(
                !r.json.to_string().contains("evil") && !r.json.to_string().contains("alert"),
                "no stored text in the answer: {}",
                r.json
            );
            nothing_was_written(&rig, &t).await;
        }
    }
}

/// A request that is not signed in is answered 401 before the name is read, whatever the name and
/// the store hold: it cannot reach the 503 of the branding (and with it the text of the error), and
/// it costs the store no read of the row. The passkey options authenticate first (`setup_named`),
/// and so does `begin`.
#[tokio::test]
async fn a_request_that_is_not_signed_in_is_refused_before_the_name_is_read() {
    for path in [BEGIN, OPTIONS] {
        // A stored name that cannot be shown.
        let bad = rig(AuthConfig::insecure_dev());
        write_row(&bad, &[("name", json!("a<b"))]);
        for token in [None, Some("not-a-session")] {
            let r = post(&bad.r, token, path, body_of(path)).await;
            assert_eq!(r.status, 401, "{path} {token:?}: {}", r.json);
            assert_ne!(r.json["code"], json!("brand_unavailable"), "{path}");
        }
        // A store that cannot be read, in both ways: the model is not even asked for. (`models_asked`
        // is cumulative, so no row is written through the double here.)
        let down = rig(AuthConfig::insecure_dev());
        down.flaky.refuse_model(brand::MODEL, Mode::Reads);
        let r = post(&down.r, None, path, body_of(path)).await;
        assert_eq!(r.status, 401, "{path}: {}", r.json);
        down.flaky.allow_models();
        down.flaky
            .fail_gets_of(brand::MODEL, "a read error that is not transient");
        let r = post(&down.r, None, path, body_of(path)).await;
        assert_eq!(r.status, 401, "{path}: {}", r.json);
        assert!(
            !down.flaky.models_asked().contains(brand::MODEL),
            "{path}: the row was read for a request that is not signed in: {:?}",
            down.flaky.models_asked()
        );
        // The control: the same store, a person who is signed in, and now the row is asked for.
        let t = sign_in(&down.r, "ada").await;
        let r = post(&down.r, Some(&t), path, body_of(path)).await;
        is_brand_unavailable(&r, path);
        assert!(down.flaky.models_asked().contains(brand::MODEL));
    }
}

/// A dispatcher that never ran `register_admin` has a reader with no store, whose `strict()` answers
/// the default brand. That is no row that was read, and the registration options never write a
/// default into a device: they are refused like a name that cannot be read. (The shipped
/// composition always has a reader; `app.rs` is the one place that builds it.)
#[tokio::test]
async fn a_server_with_no_branding_reader_refuses_the_passkey_registration() {
    let (_flaky, backend) = Flaky::memory();
    let auth = Arc::new(Auth::with_test_config(backend, AuthConfig::insecure_dev()));
    auth.set_clock_for_test(T0);
    auth.create_user("ada", PW, &["base.user".to_string()])
        .unwrap();
    let dispatcher = Arc::new(obop_commands::Dispatcher::new());
    assert!(dispatcher.brand().is_inert());
    let r = obop_http::router(obop_http::HttpState::new(dispatcher, auth), None);
    let t = sign_in(&r, "ada").await;
    let opts = post(&r, Some(&t), OPTIONS, body_of(OPTIONS)).await;
    is_brand_unavailable(&opts, "a dispatcher with no reader");
}

/// With `OBOP_MFA_ISSUER` set the authenticator needs no read of the brand: a store that refuses it
/// and a row that cannot be shown change nothing for `begin`. The passkey name has no variable, so
/// its registration is refused all the same.
#[tokio::test]
async fn with_the_variable_set_begin_needs_no_brand_read_and_the_passkey_still_does() {
    let rig = rig(AuthConfig {
        issuer: Some("Acme".into()),
        ..AuthConfig::insecure_dev()
    });
    write_row(&rig, &[("name", json!("a<b"))]);
    let cy = sign_in(&rig.r, "cy").await;
    let t = sign_in(&rig.r, "ada").await;
    rig.flaky.refuse_model(brand::MODEL, Mode::Reads);
    let begun = post(&rig.r, Some(&cy), BEGIN, json!({})).await;
    assert_eq!(begun.status, 200, "{}", begun.json);
    assert_eq!(begun.json["issuer"], json!("Acme"));
    let opts = post(&rig.r, Some(&t), OPTIONS, body_of(OPTIONS)).await;
    is_the_transient_503(&opts, "the passkey with the store refusing");
    rig.flaky.allow_models();
    let opts = post(&rig.r, Some(&t), OPTIONS, body_of(OPTIONS)).await;
    is_brand_unavailable(&opts, "the passkey with a bad stored name");
}

/// A theme read that fails does not refuse the TOTP enrolment: the name is read from the row alone.
/// (The passkey registration is the same: `a_failing_theme_read_does_not_refuse_the_passkey_registration`.)
#[tokio::test]
async fn a_failing_theme_read_does_not_refuse_the_authenticator_enrolment() {
    let rig = rig(AuthConfig::insecure_dev());
    set_brand(&rig.app);
    let cy = sign_in(&rig.r, "cy").await;
    rig.flaky.refuse_model("obop_setting", Mode::Reads);
    let begun = post(&rig.r, Some(&cy), BEGIN, json!({})).await;
    assert_eq!(begun.status, 200, "{}", begun.json);
    assert_eq!(begun.json["issuer"], json!("MOHV"));
}

/// An administrator who must still enrol (D53) holds a limited session that allows nothing but the
/// enrolment: with a row that cannot be read the enrolment is refused, the screen that could repair
/// the row (a command of the branding) is closed to the same session, and the repair is at the
/// terminal, which needs no session. After it the same session enrols.
#[tokio::test]
async fn an_administrator_who_must_enrol_repairs_the_row_at_the_terminal() {
    let rig = rig(AuthConfig {
        default_require_admins: true,
        ..AuthConfig::insecure_dev()
    });
    write_row(&rig, &[("name", json!("a<b"))]);
    let login = post(
        &rig.r,
        None,
        "/api/auth/login",
        json!({"username": "root", "password": PW}),
    )
    .await;
    assert_eq!(login.json["mfa_enrolment_required"], json!(true));
    let t = login.json["token"].as_str().unwrap().to_string();
    let r = post(&rig.r, Some(&t), BEGIN, json!({})).await;
    is_brand_unavailable(&r, "the limited session");
    // The commands of the branding are closed to it, `get` included.
    for command in ["get", "configure", "clear"] {
        let r = post(
            &rig.r,
            Some(&t),
            &format!("/api/instance_brand/{command}"),
            json!({"name": "MOHV"}),
        )
        .await;
        assert_eq!(
            (r.status, r.json["code"].as_str()),
            (403, Some("mfa_enrolment_required")),
            "{command}"
        );
    }
    // The terminal: `obop admin brand configure` is `brand::apply` with the actor `operator`.
    brand::apply(
        &*rig.app.store.backend(),
        None,
        "operator",
        &Action::Configure(vec![("name".into(), "MOHV".into())]),
        false,
    )
    .unwrap();
    let begun = post(&rig.r, Some(&t), BEGIN, json!({})).await;
    assert_eq!(begun.status, 200, "{}", begun.json);
    assert_eq!(begun.json["issuer"], json!("MOHV"));
}

// ---- Sign-in writes no name ------------------------------------------------------------------------

/// A passkey made before the brand was set still signs in after it, and after the stored name has
/// become one that cannot be shown; a TOTP sign-in too; the same sessions are refused a new
/// enrolment. The sign-in ceremony emits no name, so nothing in it depends on the row.
#[tokio::test]
async fn an_old_passkey_still_signs_in_and_a_bad_stored_name_stops_neither_sign_in() {
    let rig = rig(AuthConfig::insecure_dev());

    // Before the brand: ada makes a passkey ("obop" is written to her device), bob enrols TOTP.
    let t = sign_in(&rig.r, "ada").await;
    let opts = post(&rig.r, Some(&t), OPTIONS, body_of(OPTIONS)).await;
    assert_eq!(opts.json["publicKey"]["rp"]["name"], json!("obop"));
    let mut key = SoftAuthenticator::es256();
    let cred = key.create(&opts.json, ORIGIN);
    let done = post(
        &rig.r,
        Some(&t),
        "/api/auth/passkeys/register/finish",
        json!({"name": "Laptop", "credential": cred}),
    )
    .await;
    assert_eq!(done.status, 200, "{}", done.json);
    let tb = sign_in(&rig.r, "bob").await;
    let begun = post(&rig.r, Some(&tb), BEGIN, json!({})).await;
    let secret = totp::base32_decode(begun.json["secret"].as_str().unwrap()).unwrap();
    let confirmed = post(
        &rig.r,
        Some(&tb),
        "/api/auth/mfa/confirm",
        json!({"code": totp::code_for_step(&secret, totp::step_at(T0))}),
    )
    .await;
    assert_eq!(confirmed.status, 200, "{}", confirmed.json);

    let mut at = T0;
    let mut sign_in_both = async |label: &str| {
        // ada, second step by passkey.
        let b = post(
            &rig.r,
            None,
            "/api/auth/login",
            json!({"username": "ada", "password": PW}),
        )
        .await;
        let mt = b.json["mfa_token"].as_str().unwrap().to_string();
        let o = post(
            &rig.r,
            None,
            "/api/auth/login/passkey/options",
            json!({"mfa_token": mt}),
        )
        .await;
        assert_eq!(o.status, 200, "{label}: {}", o.json);
        let s = post(
            &rig.r,
            None,
            "/api/auth/login/passkey",
            json!({"mfa_token": mt, "credential": key.get(&o.json, ORIGIN)}),
        )
        .await;
        assert_eq!(s.status, 200, "{label} passkey: {}", s.json);
        assert!(s.json["token"].is_string(), "{label}: {}", s.json);
        // bob, second step by code (the next time step each time: a step is used once).
        at += 60;
        rig.auth.set_clock_for_test(at);
        let b = post(
            &rig.r,
            None,
            "/api/auth/login",
            json!({"username": "bob", "password": PW}),
        )
        .await;
        let mt = b.json["mfa_token"].as_str().unwrap().to_string();
        let s = post(
            &rig.r,
            None,
            "/api/auth/login/mfa",
            json!({"mfa_token": mt, "code": totp::code_for_step(&secret, totp::step_at(at))}),
        )
        .await;
        assert_eq!(s.status, 200, "{label} code: {}", s.json);
        assert!(s.json["token"].is_string(), "{label}: {}", s.json);
    };

    sign_in_both("with no row").await;
    set_brand(&rig.app);
    sign_in_both("with a brand").await;
    write_row(&rig, &[("name", json!("a<b"))]);
    sign_in_both("with a name that cannot be shown").await;

    // The same instance refuses a new enrolment, and says why.
    let cy = sign_in(&rig.r, "cy").await;
    is_brand_unavailable(
        &post(&rig.r, Some(&cy), BEGIN, json!({})).await,
        "a new enrolment",
    );
}

/// The passkey registration options make no display read, so a theme read that fails does not
/// refuse them, as it does not refuse the TOTP enrolment: the name is read from the row alone. The
/// other passkey routes build their ceremony site from the reader's display read (`passkeys::ctx`),
/// which includes the theme setting; the positive control is the list of passkeys, which meets the
/// same refused theme read, with the cell of the reader empty (the row was written around the
/// reader), and is answered as D74 answers any request that met a fault.
#[tokio::test]
async fn a_failing_theme_read_does_not_refuse_the_passkey_registration() {
    let rig = rig(AuthConfig::insecure_dev());
    write_row(&rig, &[("name", json!("MOHV"))]);
    let t = sign_in(&rig.r, "ada").await;
    rig.flaky.refuse_model("obop_setting", Mode::Reads);
    let list = call(&rig.r, "GET", "/api/auth/passkeys", Some(&t), J::Null).await;
    is_the_transient_503(
        &list,
        "the control: the list of passkeys with the theme refused",
    );
    let refused = rig.flaky.refused();
    assert!(refused > 0);
    let opts = post(&rig.r, Some(&t), OPTIONS, body_of(OPTIONS)).await;
    assert_eq!(opts.status, 200, "{}", opts.json);
    assert_eq!(opts.json["publicKey"]["rp"]["name"], json!("MOHV"));
    assert_eq!(
        rig.flaky.refused(),
        refused,
        "the registration did not ask for the theme"
    );
}

/// The passkey sign-in builds its ceremony site with the display read of the reader (it emits no
/// name, but the site carries one), and a request that meets a fault of the store is answered as D74
/// answers every request: 503 with `Retry-After`, whatever it came for. This is the rule the spec
/// chose for every page that reads the brand, the sign-in included (docs/spec/branding.md, section
/// 4, "The display read and D74", and 15.3 (f)); it is a change from the base, where the site carried
/// a constant and read nothing, and the text of section 14 says so. `fault::detached` around the
/// read is the alternative the spec names, and a change to it flips this test. Here the cell of the
/// reader is empty (the row was written around it), the theme read is refused, and the sign-in
/// options meet the refusal; when the store is back the same request is answered.
#[tokio::test]
async fn the_passkey_sign_in_options_meet_the_fault_of_the_display_read_like_any_request() {
    let rig = rig(AuthConfig::insecure_dev());
    write_row(&rig, &[("name", json!("MOHV"))]);
    let path = "/api/auth/login/passkey/options";
    rig.flaky.refuse_model("obop_setting", Mode::Reads);
    let r = post(&rig.r, None, path, json!({})).await;
    is_the_transient_503(&r, "the sign-in options with the theme read refused");
    assert!(rig.flaky.refused() > 0);
    rig.flaky.allow_models();
    let r = post(&rig.r, None, path, json!({})).await;
    assert_eq!(r.status, 200, "{}", r.json);
    assert!(r.json["publicKey"]["challenge"].is_string(), "{}", r.json);
}
