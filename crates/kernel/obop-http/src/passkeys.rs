//! Passkey routes (WebAuthn, D65) under `/api/auth`: the person's passkey list and its
//! management, the ceremony options and answers, and the two sign-in routes. Everything that
//! changes a passkey needs the password again (and the second factor when one is on). The origin
//! and the relying party id come from the public URL or the request, never from the browser's own
//! claim; a session limited to enrolment may add its first passkey.

use super::mfa::{human, mfa_error, text};
use super::oauth::client_ip;
use super::{HttpState, blocking};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use obop_commands::brand::{enrolment_error, no_reader_error};
use obop_commands::{PasskeyApi, Reauth, WebauthnCtx};
use serde_json::{Value as J, json};
use std::sync::Arc;

type Shared = State<Arc<HttpState>>;

/// Bodies here carry a credential (a few kilobytes at most).
const MAX_BODY: usize = 32 * 1024;

pub(crate) fn routes() -> Router<Arc<HttpState>> {
    let limit = axum::extract::DefaultBodyLimit::max(MAX_BODY);
    Router::new()
        .route("/api/auth/passkeys", get(list))
        .route(
            "/api/auth/passkeys/register/options",
            post(register_options).layer(limit),
        )
        .route(
            "/api/auth/passkeys/register/finish",
            post(register_finish).layer(limit),
        )
        .route("/api/auth/passkeys/passwordless", post(passwordless))
        .route("/api/auth/passkeys/proof/options", post(proof_options))
        .route("/api/auth/passkeys/proof", post(proof).layer(limit))
        .route("/api/auth/passkeys/{id}/rename", post(rename))
        .route("/api/auth/passkeys/{id}/remove", post(remove))
        .route(
            "/api/auth/login/passkey/options",
            post(login_options).layer(limit),
        )
        .route("/api/auth/login/passkey", post(login).layer(limit))
}

/// The relying party name a ceremony site carries for every route but the registration options:
/// the name of the instance brand as the reader shows it (the default when there is none, or when
/// the stored name fails the read check). The sign-in ceremony emits no name, so a bad stored name
/// does not stop a passkey or a two-factor sign-in. Asked on a blocking thread (every handler of
/// this file is on one). The display read is the one every page makes, so a request that meets a
/// fault of the store in it is answered with the 503 of D74, the sign-in routes included, which on
/// the base (where the site carried the constant `obop` and read no store) they were not
/// (docs/spec/branding.md, section 4, "The display read and D74"). The OAuth pages' ceremony site
/// takes its name from here too.
pub(crate) fn rp_name(s: &HttpState) -> String {
    s.dispatcher.brand().current().brand().name().to_string()
}

/// The relying party name the registration options write into the person's authenticator, where it
/// cannot be corrected afterwards: read from the row alone, every time (the reader's `strict`), and
/// a name that cannot be read is the error of an enrolment, never a default that a read error
/// substituted (docs/spec/branding.md, section 4). Unlike [`rp_name`] it makes no display read, so
/// the theme setting, which this name does not depend on, cannot refuse a registration.
#[allow(clippy::result_large_err)]
fn registration_name(s: &HttpState) -> Result<String, Response> {
    let reader = s.dispatcher.brand();
    // A dispatcher that never ran `register_admin` has a reader with no store, and its `strict`
    // answers the default brand: that is no row that was read, and a default is what this route
    // never writes into a device. The shipped composition always has a reader.
    if reader.is_inert() {
        return Err(mfa_error(no_reader_error()));
    }
    reader
        .strict()
        .map(|b| b.name().to_string())
        .map_err(|e| mfa_error(enrolment_error(e)))
}

/// How a ceremony site gets its relying party name: [`display_name`] for every route but the
/// registration options, [`registration_name`] for those.
type NameFor = fn(&HttpState) -> Result<String, Response>;

/// The name of a ceremony site that is not written into a device: the display read.
#[allow(clippy::result_large_err)]
fn display_name(s: &HttpState) -> Result<String, Response> {
    Ok(rp_name(s))
}

/// The ceremony site of this request, with the name a display read gives.
#[allow(clippy::result_large_err)]
fn ctx(s: &HttpState, parts: &Parts) -> Result<WebauthnCtx, Response> {
    ctx_named(s, parts, display_name)
}

/// The ceremony site of this request with the relying party name `name` gives. The name is asked for
/// after the address of the instance is known and not before, and only once.
#[allow(clippy::result_large_err)]
fn ctx_named(s: &HttpState, parts: &Parts, name: NameFor) -> Result<WebauthnCtx, Response> {
    let trust = s.trust_proxy.load(std::sync::atomic::Ordering::Relaxed);
    let Some((origin, rp_id)) = s.mcp.webauthn_origin(&parts.headers, trust) else {
        return Err(super::api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "passkeys need the address of this instance, which the request did not give",
            "passkeys_unavailable",
            "Set OBOP_PUBLIC_URL (docs/DEPLOY.md).",
        ));
    };
    Ok(WebauthnCtx {
        rp_id,
        origin,
        rp_name: name(s)?,
        ip: client_ip(s, parts),
    })
}

#[allow(clippy::result_large_err)]
fn api(s: &HttpState) -> Result<&dyn PasskeyApi, Response> {
    s.auth
        .passkeys()
        .ok_or_else(|| mfa_error(obop_commands::MfaError::NotFound))
}

fn reauth(b: &J) -> Reauth<'_> {
    Reauth {
        password: b["password"].as_str().unwrap_or(""),
        code: text(b, "code"),
        recovery_code: text(b, "recovery_code"),
    }
}

fn no_store(mut r: Response) -> Response {
    r.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    r
}

/// What `setup` hands its caller: the signed-in human, her session token, the ceremony site and the
/// passkey service.
type Setup<'a> = (
    obop_commands::Principal,
    String,
    WebauthnCtx,
    &'a dyn PasskeyApi,
);

/// The signed-in human, the ceremony site and the passkey service, or the response that says why not.
#[allow(clippy::result_large_err)]
fn setup<'a>(s: &'a HttpState, parts: &Parts) -> Result<Setup<'a>, Response> {
    setup_named(s, parts, display_name)
}

/// As [`setup`], with the relying party name `name` gives. The person is authenticated first, so
/// that a request that is not signed in is answered 401 whatever the brand holds and does not read
/// the brand row (looking up the session is a read of the store, as for any request).
#[allow(clippy::result_large_err)]
fn setup_named<'a>(s: &'a HttpState, parts: &Parts, name: NameFor) -> Result<Setup<'a>, Response> {
    let (p, token, _) = human(&*s.auth, &parts.headers)?;
    let c = ctx_named(s, parts, name)?;
    Ok((p, token, c, api(s)?))
}

async fn list(State(s): Shared, parts: Parts) -> Response {
    blocking(move || match setup(&s, &parts) {
        Ok((p, _, c, api)) => api
            .list(&p, &c)
            .map_or_else(mfa_error, |j| no_store(Json(j).into_response())),
        Err(r) => r,
    })
    .await
}

async fn register_options(State(s): Shared, parts: Parts, Json(b): Json<J>) -> Response {
    // The one route that writes the name: the registration options put it into the person's
    // authenticator, where it cannot be corrected afterwards (`registration_name`).
    blocking(move || match setup_named(&s, &parts, registration_name) {
        Ok((p, _, c, api)) => api
            .register_options(&p, &c, &reauth(&b))
            .map_or_else(mfa_error, |j| no_store(Json(j).into_response())),
        Err(r) => r,
    })
    .await
}

async fn register_finish(State(s): Shared, parts: Parts, Json(b): Json<J>) -> Response {
    blocking(move || match setup(&s, &parts) {
        Ok((p, token, c, api)) => api
            .register_finish(
                &p,
                &token,
                &c,
                b["name"].as_str().unwrap_or(""),
                &b["credential"],
            )
            .map_or_else(mfa_error, |j| no_store(Json(j).into_response())),
        Err(r) => r,
    })
    .await
}

async fn rename(
    State(s): Shared,
    parts: Parts,
    Path(id): Path<String>,
    Json(b): Json<J>,
) -> Response {
    blocking(move || match setup(&s, &parts) {
        Ok((p, _, _, api)) => api
            .rename(&p, &id, b["name"].as_str().unwrap_or(""), &reauth(&b))
            .map_or_else(mfa_error, |()| {
                Json(json!({"renamed": true})).into_response()
            }),
        Err(r) => r,
    })
    .await
}

async fn remove(
    State(s): Shared,
    parts: Parts,
    Path(id): Path<String>,
    Json(b): Json<J>,
) -> Response {
    blocking(move || match setup(&s, &parts) {
        Ok((p, token, _, api)) => api
            .remove(&p, &token, &id, &reauth(&b))
            .map_or_else(mfa_error, |()| {
                Json(json!({"removed": true})).into_response()
            }),
        Err(r) => r,
    })
    .await
}

async fn passwordless(State(s): Shared, parts: Parts, Json(b): Json<J>) -> Response {
    blocking(move || match setup(&s, &parts) {
        Ok((p, _, _, api)) => {
            let on = b["enabled"].as_bool() == Some(true);
            api.set_passwordless(&p, on, &reauth(&b))
                .map_or_else(mfa_error, |()| {
                    Json(json!({"passwordless": on})).into_response()
                })
        }
        Err(r) => r,
    })
    .await
}

async fn proof_options(State(s): Shared, parts: Parts) -> Response {
    blocking(move || match setup(&s, &parts) {
        Ok((p, _, c, api)) => api
            .proof_options(&p, &c)
            .map_or_else(mfa_error, |j| no_store(Json(j).into_response())),
        Err(r) => r,
    })
    .await
}

async fn proof(State(s): Shared, parts: Parts, Json(b): Json<J>) -> Response {
    blocking(move || match setup(&s, &parts) {
        Ok((p, _, c, api)) => api
            .proof(&p, &c, &b["credential"])
            .map_or_else(mfa_error, |()| {
                Json(json!({"proved": true})).into_response()
            }),
        Err(r) => r,
    })
    .await
}

/// `{mfa_token}` asks for the options of the second step; no token asks for usernameless ones.
async fn login_options(State(s): Shared, parts: Parts, Json(b): Json<J>) -> Response {
    blocking(move || {
        let c = match ctx(&s, &parts) {
            Ok(c) => c,
            Err(r) => return r,
        };
        let api = match api(&s) {
            Ok(a) => a,
            Err(r) => return r,
        };
        api.passkey_login_options(&c, text(&b, "mfa_token"))
            .map_or_else(mfa_error, |j| no_store(Json(j).into_response()))
    })
    .await
}

/// `{credential}` signs in without a password; with `mfa_token` it completes the second step.
async fn login(State(s): Shared, parts: Parts, Json(b): Json<J>) -> Response {
    blocking(move || {
        let c = match ctx(&s, &parts) {
            Ok(c) => c,
            Err(r) => return r,
        };
        let api = match api(&s) {
            Ok(a) => a,
            Err(r) => return r,
        };
        s.shared.maybe_purge();
        match api.passkey_login(&c, text(&b, "mfa_token"), &b["credential"]) {
            Ok(out) => {
                // The page shows who signed in, which a usernameless sign-in does not say.
                let who = match &out {
                    obop_commands::LoginOutcome::Session { token, .. } => {
                        s.auth.resolve(token).map(|p| p.id)
                    }
                    _ => None,
                };
                let mut body = super::mfa::outcome_json(Some(&*s.auth), out);
                if let Some(u) = who {
                    body.0["user"] = J::from(u);
                }
                no_store(body.into_response())
            }
            Err(e) => mfa_error(e),
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use obop_commands::brand::{Action, apply};

    /// An authenticator that knows no one: only enough for the state to exist.
    struct Nobody;

    impl obop_commands::Authenticator for Nobody {
        fn resolve(&self, _: &str) -> Option<obop_commands::Principal> {
            None
        }
        fn login(&self, _: &str, _: &str) -> Result<String, String> {
            Err("not in this test".into())
        }
        fn issue_agent_token(
            &self,
            _: &obop_commands::Principal,
            _: &[String],
            _: &str,
        ) -> Result<String, String> {
            Err("not in this test".into())
        }
    }

    /// A state whose dispatcher has a reader over `backend`, or none (a dispatcher that never ran
    /// `register_admin`) when `backend` is `None`.
    fn state_over(backend: Option<&Arc<dyn obop_storage::Backend>>) -> Arc<HttpState> {
        let d = Arc::new(obop_commands::Dispatcher::new());
        if let Some(backend) = backend {
            d.install_brand(backend.clone());
        }
        HttpState::with_mcp(
            d,
            Arc::new(Nobody),
            crate::mcp::McpConfig {
                public_url: Some("https://inst.test".into()),
                ..crate::mcp::McpConfig::default()
            },
        )
    }

    fn parts_of_a_passkey_route() -> Parts {
        axum::http::Request::builder()
            .uri("/api/auth/login/passkey/options")
            .body(())
            .unwrap()
            .into_parts()
            .0
    }

    /// The relying party name the REST builder gives a ceremony site, over a dispatcher whose reader
    /// is installed over `backend`.
    fn rp_name_over(backend: &Arc<dyn obop_storage::Backend>) -> String {
        let state = state_over(Some(backend));
        let parts = parts_of_a_passkey_route();
        match ctx(&state, &parts) {
            Ok(c) => {
                assert_eq!(c.rp_id, "inst.test");
                assert_eq!(c.origin, "https://inst.test");
                c.rp_name
            }
            Err(_) => panic!("a ceremony site"),
        }
    }

    /// `ctx()` serves every passkey route, the sign-in ceremony included, and fills `rp_name` from
    /// the reader's display value: the name of the record, `obop` with no row, and `obop` for a
    /// stored name that cannot be shown (the sign-in emits no name, so it must not be stopped by
    /// one; only the registration options, which build their site with `registration_name`, refuse
    /// it).
    #[test]
    fn the_site_of_every_passkey_route_carries_the_display_name_of_the_instance_brand() {
        let backend = obop_storage::open("memory:").unwrap();
        assert_eq!(rp_name_over(&backend), "obop", "no row");

        let backend = obop_storage::open("memory:").unwrap();
        apply(
            &*backend,
            None,
            "operator",
            &Action::Configure(vec![("name".into(), "MOHV".into())]),
            false,
        )
        .unwrap();
        assert_eq!(rp_name_over(&backend), "MOHV", "a brand");

        let backend = obop_storage::open("memory:").unwrap();
        let mut row = obop_storage::Fields::new();
        row.insert("name".into(), J::from("a<b"));
        backend
            .apply_checked(&[obop_storage::Op::Put {
                model: "instance_brand".into(),
                id: "instance".into(),
                fields: row,
            }])
            .unwrap();
        assert_eq!(
            rp_name_over(&backend),
            "obop",
            "a name that cannot be shown"
        );

        // A name the display accepts and `strict()` refuses (it holds a colon): the display read
        // shows it as it is. A site that took its name from `strict()` and fell back to `obop` on
        // the refusal would say `obop` here. (A site that read the row on every call would say the
        // same as this one: the count of the reads is `a_hundred_sites_cost_...`.)
        let backend = obop_storage::open("memory:").unwrap();
        put_name(&backend, J::from("a:b"));
        assert_eq!(
            rp_name_over(&backend),
            "a:b",
            "a name the display accepts and strict() refuses"
        );
    }

    fn put_name(backend: &Arc<dyn obop_storage::Backend>, name: J) {
        let mut row = obop_storage::Fields::new();
        row.insert("name".into(), name);
        backend
            .apply_checked(&[obop_storage::Op::Put {
                model: "instance_brand".into(),
                id: "instance".into(),
                fields: row,
            }])
            .unwrap();
    }

    /// The status, `Retry-After` and code of the response `registration_name` gives.
    async fn refusal(r: Response) -> (StatusCode, bool, J) {
        use http_body_util::BodyExt;
        let (parts, body) = r.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        (
            parts.status,
            parts.headers.contains_key("retry-after"),
            serde_json::from_slice(&bytes).unwrap_or(J::Null),
        )
    }

    /// The name of the registration options is the stored name, read from the row alone: `obop`
    /// with no row, and in every case the reader cannot answer for, the 503 `brand_unavailable` and
    /// never a default. A reader with no store is such a case: it cannot say what the row holds.
    #[tokio::test]
    async fn the_registration_name_is_the_strict_name_and_fails_closed() {
        let backend = obop_storage::open("memory:").unwrap();
        assert_eq!(
            registration_name(&state_over(Some(&backend)))
                .ok()
                .as_deref(),
            Some("obop"),
            "no row"
        );
        put_name(&backend, J::from("MOHV"));
        assert_eq!(
            registration_name(&state_over(Some(&backend)))
                .ok()
                .as_deref(),
            Some("MOHV")
        );
        for bad in [
            J::from("a<b"),
            J::from(""),
            J::from("a:b"),
            J::from("a/b"),
            J::from("a@b"),
            J::from("   "),
        ] {
            let backend = obop_storage::open("memory:").unwrap();
            put_name(&backend, bad.clone());
            let Err(r) = registration_name(&state_over(Some(&backend))) else {
                panic!("a name that cannot be written to a device was used: {bad}")
            };
            let (status, retry, body) = refusal(r).await;
            assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{bad}");
            assert!(!retry, "{bad}");
            assert_eq!(body["code"], json!("brand_unavailable"), "{bad}: {body}");
        }
        let Err(r) = registration_name(&state_over(None)) else {
            panic!("a reader with no store gave a name")
        };
        let (status, retry, body) = refusal(r).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(!retry);
        assert_eq!(body["code"], json!("brand_unavailable"), "{body}");
    }

    /// The registration options make no display read: the theme setting, which the name does not
    /// depend on, cannot refuse them. (The display read of `rp_name` is the one that asks it.)
    #[test]
    fn the_registration_name_makes_no_display_read() {
        let (flaky, backend) = obop_storage::testing::Flaky::memory();
        put_name(&backend, J::from("MOHV"));
        let state = state_over(Some(&backend));
        flaky.refuse_model("obop_setting", obop_storage::testing::Mode::Reads);
        assert_eq!(
            registration_name(&state).ok().as_deref(),
            Some("MOHV"),
            "a failing theme read does not refuse the name"
        );
        assert_eq!(flaky.refused(), 0, "the setting was not asked for");
    }

    /// A state like `state_over`, with `public_url` as the address of the instance.
    fn state_at(backend: &Arc<dyn obop_storage::Backend>, public_url: &str) -> Arc<HttpState> {
        let d = Arc::new(obop_commands::Dispatcher::new());
        d.install_brand(backend.clone());
        HttpState::with_mcp(
            d,
            Arc::new(Nobody),
            crate::mcp::McpConfig {
                public_url: Some(public_url.into()),
                ..crate::mcp::McpConfig::default()
            },
        )
    }

    /// The cache of the reader applies to the ceremony site of every route that is not the
    /// registration (the Cache bullet of the spec, section 13, for this builder): a hundred calls
    /// cost the store one read of the row and one of the theme, the unit of one refill, and not a
    /// hundred. The bound of the cell is two seconds, so the count allowed is one per two seconds
    /// the loop took, plus the first; on an idle machine that is one. A builder that reads the row
    /// and the theme on every call (the uncached `fresh()`) or only the row on every call
    /// (`strict()`) is a count of a hundred, or no read of the theme. (The allowance comes from the
    /// clock, because the dispatcher builds its reader with the default bound and a test cannot give
    /// it a longer one: a cache that broke only when the loop is slow would pass. That is the
    /// leniency of this test, and it is the reason it asks for no time of its own.)
    #[test]
    fn a_hundred_sites_cost_the_store_one_read_of_the_row_and_one_of_the_theme() {
        let (flaky, backend) = obop_storage::testing::Flaky::memory();
        put_name(&backend, J::from("MOHV"));
        let (row, theme) = (
            flaky.gets_of("instance_brand"),
            flaky.gets_of("obop_setting"),
        );
        let state = state_over(Some(&backend));
        let parts = parts_of_a_passkey_route();
        let started = std::time::Instant::now();
        for _ in 0..100 {
            assert!(ctx(&state, &parts).is_ok());
        }
        let refills = 1 + started.elapsed().as_millis() as u64 / 2000;
        let (row, theme) = (
            flaky.gets_of("instance_brand") - row,
            flaky.gets_of("obop_setting") - theme,
        );
        assert!((1..=refills).contains(&row), "the row was read {row} times");
        assert!(
            (1..=refills).contains(&theme),
            "the theme was read {theme} times"
        );
    }

    /// The name of a site is asked for after the address of the instance is known, and only once.
    /// With an address the request cannot give a site (the public URL is not an address), the
    /// answer is `passkeys_unavailable` and the store is not asked for the row or the theme: not by
    /// the display builder, and not by the registration, whose stored name here cannot be shown and
    /// would be the answer of the branding if it were read first. With a usable address each builder
    /// reads once: the registration the row alone, the display builder the unit of one refill.
    #[tokio::test]
    async fn the_name_is_read_after_the_address_is_known_and_once() {
        let (flaky, backend) = obop_storage::testing::Flaky::memory();
        put_name(&backend, J::from("a<b"));
        let (row, theme) = (
            flaky.gets_of("instance_brand"),
            flaky.gets_of("obop_setting"),
        );
        let state = state_at(&backend, "nonsense");
        let parts = parts_of_a_passkey_route();
        for (what, name) in [
            ("display", display_name as NameFor),
            ("registration", registration_name as NameFor),
        ] {
            let Err(r) = ctx_named(&state, &parts, name) else {
                panic!("{what}: a site was built with no address")
            };
            let (status, _, body) = refusal(r).await;
            assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{what}");
            assert_eq!(
                body["code"],
                json!("passkeys_unavailable"),
                "{what}: {body}"
            );
        }
        assert_eq!(flaky.gets_of("instance_brand"), row, "the row was read");
        assert_eq!(flaky.gets_of("obop_setting"), theme, "the theme was read");

        // With an address: once each.
        put_name(&backend, J::from("MOHV"));
        let state = state_at(&backend, "https://inst.test");
        let c = ctx_named(&state, &parts, registration_name).expect("a site");
        assert_eq!(c.rp_name, "MOHV");
        assert_eq!(flaky.gets_of("instance_brand") - row, 1, "the registration");
        assert_eq!(flaky.gets_of("obop_setting") - theme, 0, "no display read");
        let c = ctx_named(&state, &parts, display_name).expect("a site");
        assert_eq!(c.rp_name, "MOHV");
        assert_eq!(flaky.gets_of("instance_brand") - row, 2, "the display unit");
        assert_eq!(flaky.gets_of("obop_setting") - theme, 1, "the display unit");
    }

    /// The display read of the ceremony site is held against the request as any display read is
    /// (docs/spec/branding.md, section 4, "The display read and D74"): the site is built, with the
    /// last good name or the default, and the fault the read met is noted on the thread, which is
    /// what makes the layer above answer 503 with `Retry-After`. That is the rule the spec chose for
    /// every page, the passkey sign-in included; `fault::detached` around this read is the named
    /// alternative (15.3 (f)), and a change to it flips this test, which is its purpose.
    #[test]
    fn the_display_read_of_a_site_meets_the_fault_of_the_store_like_any_display_read() {
        let (flaky, backend) = obop_storage::testing::Flaky::memory();
        put_name(&backend, J::from("MOHV"));
        let state = state_over(Some(&backend));
        let parts = parts_of_a_passkey_route();
        // A request scope, as `blocking` opens one: a read run inside `fault::detached` would leave
        // no trace in it.
        obop_storage::fault::begin_request(obop_storage::fault::Outcome::default());
        flaky.refuse_model("obop_setting", obop_storage::testing::Mode::Reads);
        let c = ctx(&state, &parts).expect("a site is built all the same");
        assert_eq!(
            c.rp_name, "obop",
            "no last good value in this process: the default"
        );
        let outcome = obop_storage::fault::end_request();
        assert!(
            outcome.fault.is_some(),
            "the fault of the read was not noted in the request"
        );
    }
}
