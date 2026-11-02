//! The Microsoft step on the sign-in pages of a mobile app (the phone leg of D91, the D64
//! amendment): the whole way from the link on the app's sign-in page, through the stand-in
//! provider and the callback, to the consent page, the code, the token request and a device.
//!
//! These tests drive the real router, the real `Auth`, the real OAuth server and the hardened
//! egress client against the stand-in on loopback that `sso_tests` starts. The stand-in is not
//! Entra: nothing here was run against a real tenant, a real phone or a real system browser.
//!
//! The browser is the scripted one of this file: it keeps cookies by name, path and lifetime (the
//! cookie jar of `sso_tests` sends every cookie to every address, which would hide the two paths
//! that matter here: the sign-in pages' cookie is for `/oauth`, the round trip's is for
//! `/api/auth/oidc`), and it sends what a system browser's navigation sends.

use super::*;
use obop_oauth::util::s256;

const APP: &str = "org.obop.mileage:/oauth";
const RESOURCE: &str = "http://obop.test/api";
const VERIFIER: &str = "an-example-verifier-of-sufficient-length-0123456789";
const ORIGIN: &str = "http://obop.test";

// ---- the scripted browser -----------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Cookie {
    name: String,
    value: String,
    path: String,
    /// The virtual second at which it is gone (`None`: for the session).
    until: Option<u64>,
}

/// Cookies kept as a browser keeps them: by name and path, with a lifetime on a virtual clock.
#[derive(Default, Clone)]
struct Browser {
    cookies: Vec<Cookie>,
    now: u64,
}

impl Browser {
    fn absorb(&mut self, r: &Raw) {
        for c in r.all("set-cookie") {
            let mut parts = c.split(';').map(str::trim);
            let Some((name, value)) = parts.next().and_then(|f| f.split_once('=')) else {
                continue;
            };
            let mut path = "/".to_string();
            let mut until = None;
            let mut gone = false;
            for attr in parts {
                if let Some(p) = attr.strip_prefix("Path=") {
                    path = p.to_string();
                } else if let Some(m) = attr.strip_prefix("Max-Age=") {
                    let secs: u64 = m.parse().unwrap_or(0);
                    if secs == 0 {
                        gone = true;
                    }
                    until = Some(self.now + secs);
                }
            }
            self.cookies.retain(|k| !(k.name == name && k.path == path));
            if !gone {
                self.cookies.push(Cookie {
                    name: name.into(),
                    value: value.into(),
                    path,
                    until,
                });
            }
        }
    }

    /// Whether a cookie of `path` goes with a request for `target` (RFC 6265, 5.1.4).
    fn goes_to(path: &str, target: &str) -> bool {
        target == path
            || (target.starts_with(path)
                && (path.ends_with('/') || target.as_bytes().get(path.len()) == Some(&b'/')))
    }

    fn header(&self, target: &str) -> Option<String> {
        let live: Vec<String> = self
            .cookies
            .iter()
            .filter(|k| k.until.is_none_or(|u| u > self.now) && Self::goes_to(&k.path, target))
            .map(|k| format!("{}={}", k.name, k.value))
            .collect();
        (!live.is_empty()).then(|| live.join("; "))
    }

    fn has(&self, name_prefix: &str) -> bool {
        self.cookies
            .iter()
            .any(|k| k.name.starts_with(name_prefix) && k.until.is_none_or(|u| u > self.now))
    }
}

/// A page of the sign-in pages: the flow and the token of the form.
struct Page {
    flow: String,
    csrf: String,
    html: String,
}

fn hidden_of(html: &str, name: &str) -> String {
    let marker = format!("name=\"{name}\" value=\"");
    let i = html
        .find(&marker)
        .unwrap_or_else(|| panic!("no input {name} in {html}"));
    let rest = &html[i + marker.len()..];
    rest[..rest.find('"').unwrap()].to_string()
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

fn form_of(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn path_of(uri: &str) -> &str {
    uri.split_once('?').map_or(uri, |(p, _)| p)
}

fn query_of(uri: &str, k: &str) -> String {
    let q = uri.split_once('?').map_or("", |(_, q)| q);
    pairs(q).remove(k).unwrap_or_default()
}

impl World {
    /// One request of the scripted browser. `form` makes a form post from this origin.
    async fn browse(
        &self,
        b: &mut Browser,
        method: &str,
        uri: &str,
        form: Option<&[(&str, &str)]>,
        extra: &[(&str, &str)],
    ) -> Raw {
        self.browse_on(&self.router, b, method, uri, form, extra)
            .await
    }

    /// As `browse`, to another router over the same instance (one that is set up differently).
    async fn browse_on(
        &self,
        router: &axum::Router,
        b: &mut Browser,
        method: &str,
        uri: &str,
        form: Option<&[(&str, &str)]>,
        extra: &[(&str, &str)],
    ) -> Raw {
        let mut req = Request::builder()
            .method(method)
            .uri(uri)
            .header("host", HOST);
        if !extra.iter().any(|(k, _)| k.starts_with("sec-fetch-")) {
            req = req
                .header("sec-fetch-mode", "navigate")
                .header("sec-fetch-dest", "document");
        }
        if let Some(c) = b.header(path_of(uri)) {
            req = req.header("cookie", c);
        }
        for (k, v) in extra {
            req = req.header(*k, *v);
        }
        let body = match form {
            Some(f) => {
                req = req
                    .header("content-type", "application/x-www-form-urlencoded")
                    .header("origin", ORIGIN);
                Body::from(form_of(f))
            }
            None => Body::empty(),
        };
        let resp = router
            .clone()
            .oneshot(req.body(body).unwrap())
            .await
            .unwrap();
        let status = resp.status().as_u16();
        let headers = resp
            .headers()
            .iter()
            .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let text = String::from_utf8_lossy(&resp.into_body().collect().await.unwrap().to_bytes())
            .to_string();
        self.seen.lock().unwrap().push(text.clone());
        let raw = Raw {
            status,
            headers,
            body: text,
        };
        for (_, v) in &raw.headers {
            self.seen.lock().unwrap().push(v.clone());
        }
        b.absorb(&raw);
        raw
    }

    /// A client of the kind a phone app is: mobile, one private-use redirect, declaring the group
    /// the people of these tests hold.
    async fn mobile_client(&self) -> String {
        let r = self
            .send(
                "POST",
                "/oauth/register",
                &Jar::default(),
                Some(
                    &json!({
                        "client_name": "Mileage",
                        "obop_client_class": "mobile",
                        "redirect_uris": [APP],
                        "scope": "base.user",
                        "token_endpoint_auth_method": "none",
                    })
                    .to_string(),
                ),
            )
            .await;
        assert_eq!(r.status, 201, "{}", r.body);
        r.json()["client_id"].as_str().unwrap().to_string()
    }

    /// A client of the kind an agent's connector is (the default class).
    async fn mcp_client(&self) -> String {
        let r = self
            .send(
                "POST",
                "/oauth/register",
                &Jar::default(),
                Some(
                    &json!({
                        "client_name": "Claude",
                        "redirect_uris": ["https://claude.ai/api/mcp/auth_callback"],
                        "token_endpoint_auth_method": "none",
                    })
                    .to_string(),
                ),
            )
            .await;
        assert_eq!(r.status, 201, "{}", r.body);
        r.json()["client_id"].as_str().unwrap().to_string()
    }
}

fn authorize_uri(client: &str, redirect: &str, resource: &str, extra: &str) -> String {
    format!(
        "/oauth/authorize?response_type=code&client_id={client}&redirect_uri={}&state=xyz&code_challenge={}&code_challenge_method=S256&resource={}{extra}",
        enc(redirect),
        s256(VERIFIER),
        enc(resource),
    )
}

/// The phone: a browser, the client of the app, and the instance it signs in to.
struct Phone<'a> {
    w: &'a World,
    b: Browser,
    client: String,
    /// The address the phone comes from (`X-Forwarded-For`); none: from nobody in particular.
    ip: Option<String>,
}

impl<'a> Phone<'a> {
    async fn new(w: &'a World) -> Phone<'a> {
        Phone {
            w,
            b: Browser::default(),
            client: w.mobile_client().await,
            ip: None,
        }
    }

    async fn go(&mut self, method: &str, uri: &str, form: Option<&[(&str, &str)]>) -> Raw {
        let ip = self.ip.clone();
        let extra: Vec<(&str, &str)> = ip
            .as_deref()
            .map(|i| vec![("x-forwarded-for", i)])
            .unwrap_or_default();
        self.w.browse(&mut self.b, method, uri, form, &extra).await
    }

    /// The app opens the system browser at the authorization address.
    async fn open(&mut self) -> Page {
        self.open_with("").await
    }

    async fn open_with(&mut self, extra: &str) -> Page {
        let uri = authorize_uri(&self.client, APP, RESOURCE, extra);
        let r = self.go("GET", &uri, None).await;
        assert_eq!(r.status, 200, "{}", r.body);
        Page {
            flow: hidden_of(&r.body, "flow"),
            csrf: hidden_of(&r.body, "csrf"),
            html: r.body,
        }
    }

    /// The person taps the link: the redirect to the provider (or the sign-in page again).
    async fn tap(&mut self, flow: &str) -> Raw {
        self.w.forget_throttle();
        let uri = format!("/oauth/authorize/microsoft?flow={flow}");
        self.go("GET", &uri, None).await
    }

    /// Says yes at the stand-in as `oid`; the address it sends the browser back to.
    fn at_the_provider(&self, tap: &Raw, oid: &str) -> String {
        let loc = tap.location();
        // What the phone leg asks of the provider is what the back office asks: nothing more.
        let q = pairs(loc.split_once('?').unwrap().1);
        for k in ["prompt", "max_age", "login_hint"] {
            assert!(!q.contains_key(k), "{k} must not be sent");
        }
        self.w.idp.authorize(&loc, oid)
    }

    async fn callback(&mut self, uri: &str) -> Raw {
        self.w.forget_throttle();
        self.go("GET", uri, None).await
    }

    /// The browser comes back to the sign-in pages.
    async fn back(&mut self, flow: &str) -> Raw {
        let uri = format!("/oauth/authorize/return?flow={flow}");
        self.go("GET", &uri, None).await
    }

    /// Tap, say yes as `oid`, come back to the callback: its answer.
    async fn round_trip(&mut self, flow: &str, oid: &str) -> Raw {
        let tap = self.tap(flow).await;
        let uri = self.at_the_provider(&tap, oid);
        self.callback(&uri).await
    }

    /// The whole Microsoft step up to the page the return address shows.
    async fn microsoft(&mut self, flow: &str, oid: &str) -> Raw {
        let cb = self.round_trip(flow, oid).await;
        assert_eq!(
            cb.location(),
            format!("/oauth/authorize/return?flow={flow}"),
            "{}",
            cb.body
        );
        self.back(flow).await
    }

    async fn decide(&mut self, p: &Page, days: &str) -> Raw {
        self.w
            .browse(
                &mut self.b,
                "POST",
                "/oauth/authorize/decision",
                Some(&[
                    ("flow", &p.flow),
                    ("csrf", &p.csrf),
                    ("decision", "allow"),
                    ("days", days),
                    ("group", "base.user"),
                ]),
                &[],
            )
            .await
    }

    async fn password(&mut self, p: &Page, user: &str, password: &str) -> Raw {
        self.w
            .browse(
                &mut self.b,
                "POST",
                "/oauth/authorize/login",
                Some(&[
                    ("flow", &p.flow),
                    ("csrf", &p.csrf),
                    ("username", user),
                    ("password", password),
                ]),
                &[],
            )
            .await
    }

    /// The app's token request for a code.
    async fn token(&self, code: &str) -> Raw {
        let f = form_of(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", &self.client),
            ("redirect_uri", APP),
            ("code_verifier", VERIFIER),
            ("resource", RESOURCE),
            ("device_name", "Pixel 8"),
            ("device_platform", "android"),
            ("device_app_version", "1.4.2"),
        ]);
        self.w.app_post("/oauth/token", &f).await
    }

    /// The code the app gets from the consent step, or the answer that was not one.
    async fn code_of(&mut self, p: &Page, days: &str) -> String {
        let r = self.decide(p, days).await;
        let loc = r.location();
        assert!(loc.starts_with(&format!("{APP}?code=oac_")), "{loc}");
        assert_eq!(query_of(&loc, "state"), "xyz");
        query_of(&loc, "code")
    }
}

impl World {
    /// A request of the app itself (no browser: no cookies).
    async fn app_post(&self, uri: &str, form: &str) -> Raw {
        let req = Request::builder()
            .method("POST")
            .uri(uri)
            .header("host", HOST)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(form.to_string()))
            .unwrap();
        self.finish(req).await
    }

    async fn app_call(&self, method: &str, uri: &str, access: &str, body: J) -> Raw {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .header("host", HOST)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {access}"))
            .body(Body::from(body.to_string()))
            .unwrap();
        self.finish(req).await
    }

    async fn finish(&self, req: Request<Body>) -> Raw {
        let resp = self.router.clone().oneshot(req).await.unwrap();
        let status = resp.status().as_u16();
        let headers = resp
            .headers()
            .iter()
            .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let body = String::from_utf8_lossy(&resp.into_body().collect().await.unwrap().to_bytes())
            .to_string();
        self.seen.lock().unwrap().push(body.clone());
        Raw {
            status,
            headers,
            body,
        }
    }

    /// The record of an OAuth flow, as the store holds it.
    fn oauth_flow(&self, flow: &str) -> Option<J> {
        self.app
            .store
            .backend()
            .kv_get(&format!("oauth:flow:{flow}"))
            .unwrap()
            .map(|raw| serde_json::from_str(&raw).unwrap())
    }

    /// The rows of the round trip (the flow rows of the sign-in with Microsoft).
    fn sso_rows(&self) -> Vec<(String, J)> {
        flow_rows(self)
    }

    fn devices(&self) -> usize {
        self.app.store.backend().list("device").unwrap().len()
    }
}

fn consent(r: &Raw) -> bool {
    r.status == 200 && r.body.contains("name=\"decision\"")
}

fn login_page(r: &Raw) -> bool {
    r.status == 200 && r.body.contains("name=\"password\"")
}

// ---- the tests ----------------------------------------------------------------------------------

#[tokio::test]
async fn phone_a_microsoft_only_person_goes_from_the_link_to_a_device_a_refresh_and_a_revoke() {
    let w = world(true).await;
    w.keep_throttle();
    ada_sso_only(&w).await;
    let mut phone = Phone::new(&w).await;

    // The sign-in page of the app offers the link, and the password form stays.
    let page = phone.open().await;
    assert!(
        page.html.contains(&format!(
            "<a class=\"btn\" href=\"/oauth/authorize/microsoft?flow={}\">Sign in with Microsoft</a>",
            page.flow
        )),
        "{}",
        page.html
    );
    assert!(page.html.contains("name=\"password\""));

    // The link begins the round trip: the stored row names the app's flow and nothing else of it;
    // the verifier is in an HttpOnly cookie for the round trip's own path.
    let tap = phone.tap(&page.flow).await;
    let cookie = tap.all("set-cookie")[0];
    assert!(cookie.starts_with("obop_sso_") && cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Lax") && cookie.contains("Path=/api/auth/oidc"));
    let rows = w.sso_rows();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].1["purpose"], "oauth");
    assert_eq!(rows[0].1["oauth_flow"], page.flow.as_str());
    // The registered address is the one of the back office.
    assert_eq!(rows[0].1["redirect_uri"], CALLBACK);
    let uri = phone.at_the_provider(&tap, ADA_OID);

    // The callback does not make the back office's code or fragment or cookie: it sends the
    // browser to the return address of the app's flow.
    let cb = phone.callback(&uri).await;
    assert_eq!(
        cb.location(),
        format!("/oauth/authorize/return?flow={}", page.flow)
    );
    assert!(
        !cb.all("set-cookie")
            .iter()
            .any(|c| c.starts_with("obop_sso_done=")),
        "no done cookie"
    );
    assert!(!phone.b.has("obop_sso_"), "the verifier cookie is cleared");
    assert!(w.sso_rows().is_empty(), "the round trip's row is spent");
    // No one-time code stands between the callback and the return.
    assert!(
        w.app
            .store
            .backend()
            .kv_scan("auth.sso")
            .unwrap()
            .is_empty(),
        "nothing of the round trip is left in the store"
    );
    // The record of the flow has the person and the epoch the sign-in compared, and nothing the
    // provider sent.
    let rec = w.oauth_flow(&page.flow).unwrap();
    assert_eq!(rec["user"], "ada");
    // Only the keys the OAuth server itself puts there: no code, no claim, no token, no word.
    let allowed = [
        "attempts",
        "binder",
        "challenge",
        "class",
        "client_id",
        "client_name",
        "csrf",
        "declared",
        "epoch",
        "hint",
        "lang",
        "pre_epoch",
        "pre_user",
        "redirect_uri",
        "resource",
        "state",
        "user",
    ];
    for k in rec.as_object().unwrap().keys() {
        assert!(allowed.contains(&k.as_str()), "unexpected key {k} in {rec}");
    }
    assert!(rec["epoch"].is_string(), "{rec}");

    // The return address shows the consent step; reloading it shows the same and spends nothing.
    let before = w.oauth_flow(&page.flow).unwrap();
    let ret = phone.back(&page.flow).await;
    assert!(consent(&ret), "{}", ret.body);
    let again = phone.back(&page.flow).await;
    assert!(consent(&again), "{}", again.body);
    assert_eq!(w.oauth_flow(&page.flow).unwrap(), before);
    // Consent: the cap of a Microsoft-only account applies (WP5), the app gets a code.
    let page = Page {
        csrf: hidden_of(&ret.body, "csrf"),
        ..page
    };
    let code = phone.code_of(&page, "30").await;
    let t = phone.token(&code).await;
    assert_eq!(t.status, 200, "{}", t.body);
    let (access, refresh, device) = (
        t.json()["access_token"].as_str().unwrap().to_string(),
        t.json()["refresh_token"].as_str().unwrap().to_string(),
        t.json()["device_id"].as_str().unwrap().to_string(),
    );
    assert_eq!(w.devices(), 1);

    // The access token works on `/api` as the person narrowed to what was handed over.
    let r = w.app_call("GET", "/api/meta", &access, json!({})).await;
    assert_eq!(r.status, 200, "{}", r.body);
    // A refresh gives a new pair.
    let f = form_of(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &refresh),
        ("client_id", &phone.client),
    ]);
    let r = w.app_post("/oauth/token", &f).await;
    assert_eq!(r.status, 200, "{}", r.body);
    let access = r.json()["access_token"].as_str().unwrap().to_string();
    // The app ends its own device: the very next call is refused.
    let r = w
        .app_call("POST", "/api/device/revoke", &access, json!({"id": device}))
        .await;
    assert_eq!(r.status, 200, "{}", r.body);
    let r = w.app_call("GET", "/api/meta", &access, json!({})).await;
    assert_eq!(r.status, 401);

    // The log says what happened, in words that carry nothing of the provider.
    let lines = w.events("oidc_login");
    assert!(
        lines
            .last()
            .unwrap()
            .contains("OAuth sign-in pages of an app"),
        "{lines:?}"
    );
    w.assert_nowhere(CLIENT_SECRET);
    w.assert_nowhere(ACCESS_TOKEN);
    w.assert_nowhere(".eyJ");
    w.assert_nowhere("Ada Example");
    w.assert_nowhere("ada@example.test");
    w.assert_nowhere("idp-code-");
}

const FAILED: &str = "The sign-in with Microsoft did not work.";
const CANCELLED: &str = "The sign-in with Microsoft was cancelled.";
const UNAVAILABLE: &str = "The sign-in with Microsoft is not available right now.";
const BUSY: &str = "Too many sign-ins with Microsoft at once.";

/// The callback ended at the return address of the flow, and the sign-in page there says `text`
/// (and nothing is signed in on the flow).
async fn shows(phone: &mut Phone<'_>, flow: &str, cb: &Raw, text: &str) {
    assert_eq!(
        cb.location(),
        format!("/oauth/authorize/return?flow={flow}"),
        "{}",
        cb.body
    );
    let r = phone.back(flow).await;
    assert!(login_page(&r), "{}", r.body);
    assert!(r.body.contains(text), "{text:?} is not in {}", r.body);
    assert!(!consent(&r));
    // Looking at it again shows the same and spends nothing.
    let again = phone.back(flow).await;
    assert_eq!(again.body, r.body, "a reload shows another page");
    let rec = phone.w.oauth_flow(flow).unwrap();
    assert!(rec["user"].is_null(), "{rec}");
    // The word the callback left is one of the closed set.
    assert!(
        ["cancelled", "unavailable", "busy", "failed"]
            .contains(&rec["ms_error"].as_str().unwrap_or("none"))
            || rec.get("ms_error").is_none(),
        "{rec}"
    );
}

fn state_of(tap: &Raw) -> String {
    pairs(tap.location().split_once('?').unwrap().1)["state"].clone()
}

#[tokio::test]
async fn phone_every_failure_is_a_fixed_text_on_the_sign_in_page_and_the_flow_goes_on() {
    let w = world(true).await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    let flow = page.flow.clone();

    // Somebody nobody is bound to.
    let cb = phone.round_trip(&flow, STRANGER_OID).await;
    shows(&mut phone, &flow, &cb, FAILED).await;
    assert_eq!(w.events("oidc_refused").len(), 1);
    // A disabled account.
    w.auth.operator_set_disabled("ada", true).unwrap();
    let cb = phone.round_trip(&flow, ADA_OID).await;
    shows(&mut phone, &flow, &cb, FAILED).await;
    w.auth.operator_set_disabled("ada", false).unwrap();
    // The person says no at the provider.
    let tap = phone.tap(&flow).await;
    let uri = format!(
        "/api/auth/oidc/callback?error=access_denied&state={}",
        state_of(&tap)
    );
    let cb = phone.callback(&uri).await;
    shows(&mut phone, &flow, &cb, CANCELLED).await;
    assert_eq!(w.events("oidc_cancelled").len(), 1);
    // Any other answer of the provider: its words are not repeated.
    let tap = phone.tap(&flow).await;
    let uri = format!(
        "/api/auth/oidc/callback?error=server_error&error_description=AADSTS70000%20secret&state={}",
        state_of(&tap)
    );
    let cb = phone.callback(&uri).await;
    shows(&mut phone, &flow, &cb, FAILED).await;
    // A redirect with neither an error nor a code.
    let tap = phone.tap(&flow).await;
    let uri = format!("/api/auth/oidc/callback?state={}", state_of(&tap));
    let cb = phone.callback(&uri).await;
    shows(&mut phone, &flow, &cb, FAILED).await;
    // A token endpoint that says no, and one that is not well.
    w.idp.st.lock().unwrap().token_status = 400;
    let cb = phone.round_trip(&flow, ADA_OID).await;
    shows(&mut phone, &flow, &cb, FAILED).await;
    w.idp.st.lock().unwrap().token_status = 503;
    let cb = phone.round_trip(&flow, ADA_OID).await;
    shows(&mut phone, &flow, &cb, UNAVAILABLE).await;
    w.idp.reset();
    // An id token that is not good (expired).
    w.idp.edit(|c| c["exp"] = json!(T0 - 3600));
    let cb = phone.round_trip(&flow, ADA_OID).await;
    shows(&mut phone, &flow, &cb, FAILED).await;
    w.idp.reset();
    // The provider is switched off between the link and the callback.
    let tap = phone.tap(&flow).await;
    let uri = phone.at_the_provider(&tap, ADA_OID);
    w.auth
        .operator_sso_configure(TENANT, CLIENT, None, Some(false))
        .unwrap();
    let cb = phone.callback(&uri).await;
    shows(&mut phone, &flow, &cb, FAILED).await;
    // Switched off, the link starts nothing and says so.
    let tap = phone.tap(&flow).await;
    assert!(
        login_page(&tap) && tap.body.contains(UNAVAILABLE),
        "{}",
        tap.body
    );
    w.auth
        .operator_sso_configure(TENANT, CLIENT, None, Some(true))
        .unwrap();

    // Nothing the provider said, and no secret, is on any page or in the store.
    w.assert_nowhere("AADSTS");
    w.assert_nowhere(CLIENT_SECRET);
    // After all of it the same flow still goes through to the consent step, and the word of the
    // last failure is gone from the record.
    let r = phone.microsoft(&flow, ADA_OID).await;
    assert!(consent(&r), "{}", r.body);
    let rec = w.oauth_flow(&flow).unwrap();
    assert!(
        rec.get("ms_error").is_none() && rec["user"] == "ada",
        "{rec}"
    );
}

#[tokio::test]
async fn phone_a_provider_that_goes_away_is_unavailable() {
    let w = world(true).await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    // One good trip fills the caches; then the provider goes away.
    let r = phone.microsoft(&page.flow, ADA_OID).await;
    assert!(consent(&r), "{}", r.body);
    let p2 = phone.open().await;
    let tap = phone.tap(&p2.flow).await;
    let uri = phone.at_the_provider(&tap, ADA_OID);
    w.idp.stop();
    let cb = phone.callback(&uri).await;
    shows(&mut phone, &p2.flow, &cb, UNAVAILABLE).await;
    assert!(
        w.events("oidc_error")
            .iter()
            .any(|l| l.contains("token endpoint could not be reached"))
    );
}

#[tokio::test]
async fn phone_the_browser_that_comes_back_must_be_the_one_that_left() {
    let w = world(true).await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    let tap = phone.tap(&page.flow).await;
    let uri = phone.at_the_provider(&tap, ADA_OID);
    // Another browser brings the callback: it has no verifier cookie.
    let mut other = Browser::default();
    let cb = w.browse(&mut other, "GET", &uri, None, &[]).await;
    assert_eq!(
        cb.location(),
        format!("/oauth/authorize/return?flow={}", page.flow)
    );
    // The code is not redeemed: the provider's token endpoint is not called at all.
    assert_eq!(w.idp.hits("/token"), 0);
    assert!(
        w.events("oidc_refused")
            .iter()
            .any(|l| l.contains("not the one that began")),
        "{:?}",
        w.events("oidc_refused")
    );
    // The line says what happened and not to whom: no line of the log names the flow.
    for (id, f) in w.app.store.backend().list("security_event").unwrap() {
        assert!(
            !J::Object(f).to_string().contains(&page.flow),
            "the flow id is in the security event {id}"
        );
    }
    // The other browser, which has no cookie of the sign-in pages, is shown no page of the flow.
    let theirs = w
        .browse(
            &mut other,
            "GET",
            &format!("/oauth/authorize/return?flow={}", page.flow),
            None,
            &[],
        )
        .await;
    assert!(
        [403, 410].contains(&theirs.status) && !consent(&theirs) && !login_page(&theirs),
        "{} {}",
        theirs.status,
        theirs.body
    );
    // The phone's own browser sees the failure and can go on.
    let r = phone.back(&page.flow).await;
    assert!(login_page(&r) && r.body.contains(FAILED), "{}", r.body);
    assert!(w.oauth_flow(&page.flow).unwrap()["user"].is_null());

    // A cookie of the right name with a wrong value is another browser as well.
    let tap = phone.tap(&page.flow).await;
    let uri = phone.at_the_provider(&tap, ADA_OID);
    for c in phone
        .b
        .cookies
        .iter_mut()
        .filter(|c| c.name.starts_with("obop_sso_"))
    {
        c.value = "0".repeat(c.value.len());
    }
    let cb = phone.callback(&uri).await;
    shows(&mut phone, &page.flow, &cb, FAILED).await;

    // A good trip, and then a return without the cookie of the sign-in pages: nothing is shown,
    // and the record is as it was.
    let r = phone.microsoft(&page.flow, ADA_OID).await;
    assert!(consent(&r), "{}", r.body);
    let before = w.oauth_flow(&page.flow).unwrap();
    let mut stranger = Browser::default();
    let theirs = w
        .browse(
            &mut stranger,
            "GET",
            &format!("/oauth/authorize/return?flow={}", page.flow),
            None,
            &[],
        )
        .await;
    assert!(
        [403, 410].contains(&theirs.status) && !consent(&theirs),
        "{} {}",
        theirs.status,
        theirs.body
    );
    assert_eq!(w.oauth_flow(&page.flow).unwrap(), before);
}

#[tokio::test]
async fn phone_a_flow_that_is_gone_is_not_made_again_by_a_callback() {
    let w = world(true).await;
    let mut phone = Phone::new(&w).await;

    // The ten minutes of the sign-in pages run out during the hop (the store's lifetime of the
    // record is simulated by deleting it).
    let page = phone.open().await;
    let tap = phone.tap(&page.flow).await;
    let uri = phone.at_the_provider(&tap, ADA_OID);
    assert!(
        w.app
            .store
            .backend()
            .kv_delete(&format!("oauth:flow:{}", page.flow))
            .unwrap()
    );
    let cb = phone.callback(&uri).await;
    assert_eq!(
        cb.location(),
        format!("/oauth/authorize/return?flow={}", page.flow)
    );
    assert!(w.oauth_flow(&page.flow).is_none(), "not made again");
    let r = phone.back(&page.flow).await;
    assert_eq!(r.status, 410, "{}", r.body);
    assert!(!consent(&r));

    // Eight wrong passwords end the flow; a callback that comes after does not bring it back.
    let page = phone.open().await;
    let tap = phone.tap(&page.flow).await;
    let uri = phone.at_the_provider(&tap, ADA_OID);
    for _ in 0..8 {
        phone.password(&page, "ada", "not the password").await;
    }
    assert!(w.oauth_flow(&page.flow).is_none(), "ended by the eighth");
    let cb = phone.callback(&uri).await;
    assert_eq!(
        cb.location(),
        format!("/oauth/authorize/return?flow={}", page.flow)
    );
    assert!(w.oauth_flow(&page.flow).is_none(), "not made again");
    assert_eq!(phone.back(&page.flow).await.status, 410);

    // The round trip's own ten minutes run out (the row names the time itself): the callback
    // cannot tell what the flow was for and ends as the back office does, with a fixed word; the
    // sign-in page of the app is still there, and the link works again.
    let page = phone.open().await;
    let tap = phone.tap(&page.flow).await;
    let uri = phone.at_the_provider(&tap, ADA_OID);
    let (key, mut row) = w.sso_rows().remove(0);
    row["expires_at_ms"] = json!(now_ms() - 1);
    w.app
        .store
        .backend()
        .kv_put(&key, &row.to_string(), Some(600_000))
        .unwrap();
    let cb = phone.callback(&uri).await;
    assert_eq!(cb.location(), "/#oidc_error=failed");
    assert!(w.oauth_flow(&page.flow).unwrap()["user"].is_null());
    let r = phone.microsoft(&page.flow, ADA_OID).await;
    assert!(consent(&r), "{}", r.body);

    // A state that was replayed is no flow any more.
    let page = phone.open().await;
    let tap = phone.tap(&page.flow).await;
    let uri = phone.at_the_provider(&tap, ADA_OID);
    let cb = phone.callback(&uri).await;
    assert_eq!(
        cb.location(),
        format!("/oauth/authorize/return?flow={}", page.flow)
    );
    let replay = phone.callback(&uri).await;
    assert_eq!(replay.location(), "/#oidc_error=failed");
    let r = phone.back(&page.flow).await;
    assert!(consent(&r), "the first trip still stands: {}", r.body);
}

#[tokio::test]
async fn phone_the_two_kinds_of_round_trip_are_not_mixed_up() {
    let w = world(true).await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;

    // A round trip of the back office ends at the back office, whatever flow of an app is open.
    let mut jar = Jar::default();
    let back = w.sign_in(&mut jar, ADA_OID).await;
    assert!(
        back.location().starts_with("/#oidc="),
        "{}",
        back.location()
    );
    assert!(w.oauth_flow(&page.flow).unwrap()["user"].is_null());

    // A round trip of an app never ends with the back office's code, fragment or cookie.
    let tap = phone.tap(&page.flow).await;
    let uri = phone.at_the_provider(&tap, ADA_OID);
    let cb = phone.callback(&uri).await;
    assert!(!cb.location().contains("#oidc"), "{}", cb.location());
    assert!(
        !cb.all("set-cookie")
            .iter()
            .any(|c| c.starts_with("obop_sso_done=") && !c.contains("Max-Age=0"))
    );
    // The state of a back office trip is not a flow of the sign-in pages.
    let s = w.start(&mut Jar::default()).await;
    let r = phone.back(&state_of(&s)).await;
    assert_eq!(r.status, 410, "{}", r.body);
    assert!(!consent(&r));

    // A row that names a flow of an agent's connector is not finished on it: the connector's
    // flow gets no person, and its return address refuses.
    let mcp = w.mcp_client().await;
    let mut other = Browser::default();
    let uri = format!(
        "/oauth/authorize?response_type=code&client_id={mcp}&redirect_uri={}&state=xyz&code_challenge={}&code_challenge_method=S256&resource={}",
        enc("https://claude.ai/api/mcp/auth_callback"),
        s256(VERIFIER),
        enc("http://obop.test/mcp"),
    );
    let mcp_page = w.browse(&mut other, "GET", &uri, None, &[]).await;
    assert_eq!(mcp_page.status, 200, "{}", mcp_page.body);
    let mcp_flow = hidden_of(&mcp_page.body, "flow");
    // (The round trips begun above and not finished are forgotten first.)
    for (k, _) in w.sso_rows() {
        w.app.store.backend().kv_delete(&k).unwrap();
    }
    let mut jar = Jar::default();
    let s = w.start(&mut jar).await;
    let rows = w.sso_rows();
    assert_eq!(rows.len(), 1);
    let (key, mut row) = rows.into_iter().next().unwrap();
    row["purpose"] = json!("oauth");
    row["oauth_flow"] = json!(mcp_flow);
    w.app
        .store
        .backend()
        .kv_put(&key, &row.to_string(), Some(600_000))
        .unwrap();
    let uri = w.idp.authorize(&s.location(), ADA_OID);
    let cb = w.callback(&mut jar, &uri).await;
    assert_eq!(
        cb.location(),
        format!("/oauth/authorize/return?flow={mcp_flow}")
    );
    let rec = w.oauth_flow(&mcp_flow).unwrap();
    assert!(
        rec["user"].is_null() && rec.get("pre_user").is_none(),
        "{rec}"
    );
    let r = w
        .browse(
            &mut other,
            "GET",
            &format!("/oauth/authorize/return?flow={mcp_flow}"),
            None,
            &[],
        )
        .await;
    assert_eq!(r.status, 403, "{}", r.body);
    assert!(!consent(&r));
}

#[tokio::test]
async fn phone_a_flow_of_an_agents_connector_sees_no_link_and_the_route_refuses_it() {
    let w = world(true).await;
    let mcp = w.mcp_client().await;
    let mut b = Browser::default();
    let uri = format!(
        "/oauth/authorize?response_type=code&client_id={mcp}&redirect_uri={}&state=xyz&code_challenge={}&code_challenge_method=S256&resource={}",
        enc("https://claude.ai/api/mcp/auth_callback"),
        s256(VERIFIER),
        enc("http://obop.test/mcp"),
    );
    let r = w.browse(&mut b, "GET", &uri, None, &[]).await;
    assert!(login_page(&r), "{}", r.body);
    assert!(!r.body.contains("Microsoft"), "{}", r.body);
    assert!(!r.body.contains("/oauth/authorize/microsoft"));
    let flow = hidden_of(&r.body, "flow");
    let hits = w.idp.hits("");
    let r = w
        .browse(
            &mut b,
            "GET",
            &format!("/oauth/authorize/microsoft?flow={flow}"),
            None,
            &[],
        )
        .await;
    assert_eq!(r.status, 403, "{}", r.body);
    assert!(w.sso_rows().is_empty(), "no round trip was begun");
    assert_eq!(w.idp.hits(""), hits, "the provider was not asked anything");
    assert!(w.events("oidc_start").is_empty());
}

#[tokio::test]
async fn phone_the_link_is_absent_without_a_provider_or_a_runtime_and_a_start_needs_its_preconditions()
 {
    // No provider configured: no link, and the route says the sign-in is not available.
    let w = world(false).await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    assert!(!page.html.contains("/oauth/authorize/microsoft"));
    let r = phone.tap(&page.flow).await;
    assert!(login_page(&r) && r.body.contains(UNAVAILABLE), "{}", r.body);
    assert!(w.sso_rows().is_empty());

    // A provider that is configured but not switched on: the same.
    let w = world(true).await;
    w.auth
        .operator_sso_configure(TENANT, CLIENT, None, Some(false))
        .unwrap();
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    assert!(!page.html.contains("/oauth/authorize/microsoft"));

    // A provider that is on, and a server without the connectors runtime: no link either.
    w.auth
        .operator_sso_configure(TENANT, CLIENT, None, Some(true))
        .unwrap();
    let page = phone.open().await;
    assert!(page.html.contains("/oauth/authorize/microsoft"));
    let bare = obop_http::router(
        obop_http::HttpState::new(w.app.dispatcher.clone(), w.auth.clone()),
        None,
    );
    let uri = authorize_uri(&phone.client, APP, RESOURCE, "");
    let r = w
        .browse_on(&bare, &mut phone.b, "GET", &uri, None, &[])
        .await;
    assert!(login_page(&r) && !r.body.contains("/oauth/authorize/microsoft"));
    let flow = hidden_of(&r.body, "flow");
    let r = w
        .browse_on(
            &bare,
            &mut phone.b,
            "GET",
            &format!("/oauth/authorize/microsoft?flow={flow}"),
            None,
            &[],
        )
        .await;
    assert!(login_page(&r) && r.body.contains(UNAVAILABLE), "{}", r.body);

    // The preconditions of a start: a navigation, the cookie of the flow, nobody signed in yet.
    let page = phone.open().await;
    let uri = format!("/oauth/authorize/microsoft?flow={}", page.flow);
    let r = w
        .browse(
            &mut phone.b,
            "GET",
            &uri,
            None,
            &[("sec-fetch-mode", "cors"), ("sec-fetch-dest", "empty")],
        )
        .await;
    assert_eq!(r.status, 400, "a fetch is not a click: {}", r.body);
    assert!(w.sso_rows().is_empty());
    let r = w
        .browse(&mut Browser::default(), "GET", &uri, None, &[])
        .await;
    assert_eq!(r.status, 403, "no cookie of the flow: {}", r.body);
    assert!(w.sso_rows().is_empty());
    for bad in ["x", "", "0123456789abcdef0123456789abcdef"] {
        let r = w
            .browse(
                &mut phone.b,
                "GET",
                &format!("/oauth/authorize/microsoft?flow={bad}"),
                None,
                &[],
            )
            .await;
        assert!([403, 410].contains(&r.status), "{bad}: {}", r.status);
    }
    // Somebody is signed in on the flow already: the link starts nothing.
    let r = phone.microsoft(&page.flow, ADA_OID).await;
    assert!(consent(&r), "{}", r.body);
    let rows = w.sso_rows().len();
    let r = phone.tap(&page.flow).await;
    assert_eq!(r.status, 403, "{}", r.body);
    assert_eq!(w.sso_rows().len(), rows);
}

#[tokio::test]
async fn phone_the_taps_of_an_app_have_windows_of_their_own() {
    let w = world(true).await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    w.keep_throttle();
    // 30 taps a minute from one address; the 31st is told so on the page.
    let mut answers = Vec::new();
    for _ in 0..32 {
        let r = phone.tap(&page.flow).await;
        answers.push(if r.status == 303 {
            "go"
        } else {
            assert!(login_page(&r) && r.body.contains(BUSY), "{}", r.body);
            "busy"
        });
    }
    assert_eq!(answers.iter().filter(|a| **a == "go").count(), 30);
    assert_eq!(&answers[30..], ["busy", "busy"]);
    // The back office's link from the same address is not held back by them.
    let s = w.start(&mut Jar::default()).await;
    assert_eq!(s.status, 303, "{}", s.body);
    // And the other way: the windows of the back office's link do not count the taps.
    let w2 = world(true).await;
    let mut phone2 = Phone::new(&w2).await;
    let page2 = phone2.open().await;
    w2.keep_throttle();
    for _ in 0..30 {
        let r = w2.start(&mut Jar::default()).await;
        assert!(r.location().contains("authorize?"), "{}", r.location());
    }
    let r = w2.start(&mut Jar::default()).await;
    assert!(r.location().contains("oidc_error=busy"), "{}", r.location());
    let t = phone2.tap(&page2.flow).await;
    assert!(t.location().contains("authorize?"), "{}", t.location());
}

impl Phone<'_> {
    /// The second step of the sign-in pages: the code of the authenticator app.
    async fn mfa(&mut self, p: &Page, token: &str, code: &str) -> Raw {
        self.w
            .browse(
                &mut self.b,
                "POST",
                "/oauth/authorize/mfa",
                Some(&[
                    ("flow", &p.flow),
                    ("csrf", &p.csrf),
                    ("mfa_token", token),
                    ("code", code),
                ]),
                &[],
            )
            .await
    }
}

fn asks_for_the_second_step(r: &Raw) -> bool {
    r.status == 200 && r.body.contains("name=\"mfa_token\"")
}

fn page_of(flow: &str, r: &Raw) -> Page {
    Page {
        flow: flow.to_string(),
        csrf: hidden_of(&r.body, "csrf"),
        html: r.body.clone(),
    }
}

#[tokio::test]
async fn phone_the_second_factor_is_asked_as_it_is_for_a_password_and_survives_a_reload() {
    // Bound, not Microsoft-only, with an authenticator app: Microsoft's answer is the first
    // factor and the app is the second (D53), exactly as at the back office.
    let w = world(true).await;
    let rig = Rig {
        r: w.router.clone(),
        auth: w.auth.clone(),
    };
    let (secret, _) = enrol(&rig, "ada").await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    let r = phone.microsoft(&page.flow, ADA_OID).await;
    assert!(asks_for_the_second_step(&r), "{}", r.body);
    assert!(!consent(&r));
    assert!(w.oauth_flow(&page.flow).unwrap()["user"].is_null());
    let first = hidden_of(&r.body, "mfa_token");

    // Reloading the return address asks again, with a token of its own.
    let again = phone.back(&page.flow).await;
    assert!(asks_for_the_second_step(&again), "{}", again.body);
    let second = hidden_of(&again.body, "mfa_token");
    assert_ne!(first, second);

    // A password try for somebody else on the same flow does not change whose step this is.
    let p = page_of(&page.flow, &again);
    let other = phone.password(&p, "bob", "not the password").await;
    assert!(login_page(&other), "{}", other.body);
    let back = phone.back(&page.flow).await;
    assert!(asks_for_the_second_step(&back), "{}", back.body);
    let token = hidden_of(&back.body, "mfa_token");

    // A wrong code is told so; the right one gives the consent step.
    let wrong = phone.mfa(&p, &token, "000000").await;
    assert!(
        asks_for_the_second_step(&wrong) && wrong.body.contains("That code is not right."),
        "{}",
        wrong.body
    );
    w.auth.set_clock_for_test(T0 + 60);
    let code = obop_auth::totp::code_for_step(&secret, obop_auth::totp::step_at(T0 + 60));
    let ok = phone.mfa(&p, &token, &code).await;
    assert!(consent(&ok), "{}", ok.body);
    let rec = w.oauth_flow(&page.flow).unwrap();
    assert_eq!(rec["user"], "ada", "{rec}");
    assert!(rec.get("mfa_pending").is_none() && rec.get("ms_error").is_none());
    // The rest is as ever: a code, a token, a device.
    let page = page_of(&page.flow, &ok);
    let code = phone.code_of(&page, "30").await;
    let t = phone.token(&code).await;
    assert_eq!(t.status, 200, "{}", t.body);
}

#[tokio::test]
async fn phone_an_administrator_without_a_second_factor_from_the_provider_is_sent_to_enrol() {
    // The registration has no optional claims, so Microsoft is not heard to say that a second
    // factor was used, and an administrator who signs in with Microsoft only is limited to
    // enrolment, as with a password: an app cannot be connected through such a session.
    let cfg = AuthConfig {
        default_require_admins: true,
        ..AuthConfig::insecure_dev()
    };
    let w = world_with(true, cfg, None).await;
    w.auth.operator_sso_bind("boss", BOSS_OID).unwrap();
    let mut jar = Jar::default();
    let back = w.sign_in(&mut jar, BOSS_OID).await;
    assert_eq!(w.exchange(&mut jar, &code_of(&back)).await.status, 200);
    w.auth.operator_sso_only("boss", true).unwrap();
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;

    w.idp.without_optional_claims();
    let cb = phone.round_trip(&page.flow, BOSS_OID).await;
    shows_text(
        &mut phone,
        &page.flow,
        &cb,
        "Your administrator requires two-factor authentication.",
    )
    .await;
    // Nobody is signed in on the flow, the limited session is gone, and the log says why.
    assert!(w.oauth_flow(&page.flow).unwrap()["user"].is_null());
    assert!(
        w.events("oidc_login")
            .last()
            .unwrap()
            .contains("does not report"),
        "{:?}",
        w.events("oidc_login")
    );
    // With the optional claims the same person reaches the consent step.
    w.idp.reset();
    let r = phone.microsoft(&page.flow, BOSS_OID).await;
    assert!(consent(&r), "{}", r.body);
}

/// As `shows`, for a text of the password step's own words (the word left in the record is
/// `enrol` or `pwchange`).
async fn shows_text(phone: &mut Phone<'_>, flow: &str, cb: &Raw, text: &str) {
    assert_eq!(
        cb.location(),
        format!("/oauth/authorize/return?flow={flow}"),
        "{}",
        cb.body
    );
    let r = phone.back(flow).await;
    assert!(login_page(&r) && r.body.contains(text), "{}", r.body);
    assert!(!consent(&r));
}

#[tokio::test]
async fn phone_a_microsoft_only_account_cannot_use_its_password_on_the_sign_in_pages() {
    let w = world(true).await;
    ada_sso_only(&w).await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    let wrong = phone.password(&page, "ada", "not the password").await;
    assert!(
        login_page(&wrong)
            && wrong
                .body
                .contains("The username or password is not right."),
        "{}",
        wrong.body
    );
    // The right password is refused in the same words, and nothing is signed in on the flow.
    let right = phone.password(&page, "ada", PW).await;
    assert_eq!(right.status, wrong.status);
    assert_eq!(right.body, wrong.body, "told apart from a wrong password");
    assert!(w.oauth_flow(&page.flow).unwrap()["user"].is_null());
    // Somebody whose account is not Microsoft-only signs in with a password as ever.
    let bob = phone.password(&page, "bob", PW).await;
    assert!(consent(&bob), "{}", bob.body);
    // Ada, given her password back, can use it, after choosing a new one in the back office (the
    // old one was not hers to keep; turning the setting off demands the change).
    w.auth.operator_sso_only("ada", false).unwrap();
    let page = phone.open().await;
    let ada = phone.password(&page, "ada", PW).await;
    assert!(
        login_page(&ada) && ada.body.contains("You must choose a new password"),
        "{}",
        ada.body
    );
}

#[tokio::test]
async fn phone_a_change_of_credentials_after_the_sign_in_compared_is_seen_by_the_consent() {
    use obop_storage::testing::Flaky;
    let (flaky, backend) = Flaky::memory();
    let app = App::with_store(obop_commands::Store::over(backend));
    let w = world_on(app, true, AuthConfig::insecure_dev(), None).await;
    ada_sso_only(&w).await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    let tap = phone.tap(&page.flow).await;
    let uri = phone.at_the_provider(&tap, ADA_OID);

    // The account's credentials change after the sign-in compared them and before the callback
    // writes the person into the flow: the flow holds the epoch the sign-in compared, so the
    // consent step finds the change. (Reading the epoch again at the write would accept it.)
    let auth = w.auth.clone();
    flaky.before_the_next_kv_get_of(
        "oauth:flow:",
        Box::new(move |_| {
            auth.operator_sso_only("ada", false).unwrap();
        }),
    );
    let cb = phone.callback(&uri).await;
    assert_eq!(
        cb.location(),
        format!("/oauth/authorize/return?flow={}", page.flow)
    );
    let rec = w.oauth_flow(&page.flow).unwrap();
    assert_eq!(rec["user"], "ada");
    let now = w.auth.oauth().unwrap().credential_epoch("ada").unwrap();
    assert_ne!(rec["epoch"].as_str(), now.as_deref(), "{rec}");
    let r = phone.back(&page.flow).await;
    assert!(consent(&r), "{}", r.body);
    let d = phone.decide(&page_of(&page.flow, &r), "30").await;
    assert_eq!(d.status, 410, "{}", d.body);
    assert!(!d.all("location").iter().any(|l| l.starts_with(APP)));
    assert_eq!(w.devices(), 0);
}

#[tokio::test]
async fn phone_a_change_of_credentials_between_the_consent_and_the_token_request_refuses_the_code()
{
    let w = world(true).await;
    ada_sso_only(&w).await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    let r = phone.microsoft(&page.flow, ADA_OID).await;
    let code = phone.code_of(&page_of(&page.flow, &r), "30").await;
    w.auth.operator_sso_only("ada", false).unwrap();
    let t = phone.token(&code).await;
    assert_eq!(t.status, 400, "{}", t.body);
    assert_eq!(t.json()["error"], "invalid_grant");
    assert_eq!(w.devices(), 0);
}

#[tokio::test]
async fn phone_a_storage_fault_in_the_callback_is_never_a_500_and_leaves_the_flow_usable() {
    use obop_storage::testing::Flaky;
    let (flaky, backend) = Flaky::memory();
    let app = App::with_store(obop_commands::Store::over(backend));
    let w = world_on(app, true, AuthConfig::insecure_dev(), None).await;
    // Nothing of the test's own is to use up the calls the sweep counts.
    w.keep_throttle();
    ada_sso_only(&w).await;
    let mut phone = Phone::new(&w).await;
    let (mut as_503, mut as_redirect, mut clean) = (0, 0, 0);
    for skip in 0..120 {
        phone.ip = Some(format!("10.7.{}.{}", skip / 200, skip % 200 + 1));
        let page = phone.open().await;
        let tap = phone.tap(&page.flow).await;
        let uri = phone.at_the_provider(&tap, ADA_OID);
        let before = flaky.refused();
        flaky.refuse_call_after(skip);
        let cb = phone.callback(&uri).await;
        let refused = flaky.refused() - before;
        flaky.allow_models();
        if refused == 0 {
            // Past the last call of the request.
            assert_eq!(
                cb.location(),
                format!("/oauth/authorize/return?flow={}", page.flow)
            );
            clean += 1;
            if clean == 2 {
                break;
            }
            continue;
        }
        match cb.status {
            // D74: a refused store is a 503 that says what became of the changes.
            503 => {
                assert!(
                    ["none", "unknown", "partial"]
                        .contains(&cb.json()["applied"].as_str().unwrap()),
                    "{skip}: {}",
                    cb.body
                );
                as_503 += 1;
            }
            // Or the redirect with a fixed word, never the back office's fragment.
            303 => {
                let loc = cb.location();
                assert!(
                    loc == format!("/oauth/authorize/return?flow={}", page.flow),
                    "{skip}: {loc}"
                );
                as_redirect += 1;
            }
            other => panic!("{skip}: {other}: {}", cb.body),
        }
        // Whatever the flow holds is consistent: a person only if it is Ada, with the epoch.
        let rec = w.oauth_flow(&page.flow);
        if let Some(rec) = &rec
            && rec["user"].is_string()
        {
            assert_eq!(rec["user"], "ada", "{skip}: {rec}");
            assert!(rec["epoch"].is_string(), "{skip}: {rec}");
        }
        // And the flow can be used afterwards: the sign-in goes through to the consent step.
        let r = if rec.is_some_and(|r| r["user"].is_string()) {
            phone.back(&page.flow).await
        } else {
            phone.microsoft(&page.flow, ADA_OID).await
        };
        assert!(consent(&r), "{skip}: {}", r.body);
    }
    assert!(clean >= 1, "the sweep went past the end of the request");
    assert!(
        as_503 > 0,
        "the sweep refused something: {as_503} {as_redirect}"
    );
    // Nothing of the provider is stored or shown whatever failed.
    w.assert_nowhere(CLIENT_SECRET);
    w.assert_nowhere(ACCESS_TOKEN);
}

#[tokio::test]
async fn phone_a_flow_that_another_request_changed_meanwhile_is_read_again_and_not_overwritten() {
    use obop_storage::testing::Flaky;
    let (flaky, backend) = Flaky::memory();
    let app = App::with_store(obop_commands::Store::over(backend));
    let w = world_on(app, true, AuthConfig::insecure_dev(), None).await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    let tap = phone.tap(&page.flow).await;
    let uri = phone.at_the_provider(&tap, ADA_OID);
    // Another request changed the record of the flow just before the callback writes.
    let key = format!("oauth:flow:{}", page.flow);
    flaky.before_the_next_kv_swap(Box::new(move |b| {
        let raw = b.kv_get(&key).unwrap().unwrap();
        let mut rec: J = serde_json::from_str(&raw).unwrap();
        rec["marker"] = json!("another request");
        b.kv_put(&key, &rec.to_string(), Some(600_000)).unwrap();
    }));
    let cb = phone.callback(&uri).await;
    assert_eq!(
        cb.location(),
        format!("/oauth/authorize/return?flow={}", page.flow)
    );
    let rec = w.oauth_flow(&page.flow).unwrap();
    assert_eq!(rec["user"], "ada", "the sign-in was written");
    assert_eq!(
        rec["marker"], "another request",
        "and the other request's change was kept"
    );
    let r = phone.back(&page.flow).await;
    assert!(consent(&r), "{}", r.body);
}

#[tokio::test]
async fn phone_the_callbacks_of_an_app_have_windows_of_their_own() {
    let w = world(true).await;
    w.keep_throttle();
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    // Callbacks that spent a flow: 60 a minute from one address, and the 61st is told so. (The
    // taps come from other addresses, so that the limit of the taps does not come first.)
    let mut words = Vec::new();
    for i in 0..62 {
        phone.ip = Some(format!("10.8.0.{}", i + 1));
        let tap = phone.tap(&page.flow).await;
        let uri = format!(
            "/api/auth/oidc/callback?error=access_denied&state={}",
            state_of(&tap)
        );
        phone.ip = Some("203.0.113.7".into());
        let cb = phone.callback(&uri).await;
        assert_eq!(
            cb.location(),
            format!("/oauth/authorize/return?flow={}", page.flow)
        );
        words.push(
            w.oauth_flow(&page.flow).unwrap()["ms_error"]
                .as_str()
                .unwrap()
                .to_string(),
        );
    }
    assert_eq!(words.iter().filter(|x| *x == "cancelled").count(), 60);
    assert_eq!(&words[60..], ["busy", "busy"]);
    // The back office's callbacks from the same address are not held back by them.
    let h = [("x-forwarded-for", "10.8.9.9")];
    let mut jar = Jar::default();
    let s = w
        .send_h("GET", "/api/auth/oidc/start", &jar, None, &h)
        .await;
    jar.absorb(&s);
    let uri = format!(
        "/api/auth/oidc/callback?error=access_denied&state={}",
        state_of(&s)
    );
    let r = w
        .send_h(
            "GET",
            &uri,
            &jar,
            None,
            &[("x-forwarded-for", "203.0.113.7")],
        )
        .await;
    assert_eq!(r.location(), "/#oidc_error=cancelled");
}

// ---- the 30-day cap of a Microsoft-only phone (WP5) -----------------------------------------------

/// The values of the options of the select of the consent page.
fn days_offered(r: &Raw) -> Vec<u32> {
    let mut out = Vec::new();
    let mut rest = r.body.as_str();
    while let Some(i) = rest.find("<option value=\"") {
        rest = &rest[i + "<option value=\"".len()..];
        let n: String = rest.chars().take_while(char::is_ascii_digit).collect();
        out.push(n.parse().unwrap());
    }
    out
}

const DAY: u64 = 86_400;

/// What the stores hold of the grants: (created, expires) of every grant, the expiry of every
/// refresh token and of every device, all in seconds.
fn lifetimes(w: &World) -> (Vec<(u64, u64)>, Vec<u64>, Vec<u64>) {
    let b = w.app.store.backend();
    let grants = b
        .list("oauth_grant")
        .unwrap()
        .into_iter()
        .map(|(_, f)| {
            (
                f["created_at"].as_u64().unwrap(),
                f["expires_at"].as_u64().unwrap(),
            )
        })
        .collect();
    let refresh = b
        .list("oauth_refresh")
        .unwrap()
        .into_iter()
        .map(|(_, f)| f["expires_at"].as_u64().unwrap())
        .collect();
    let devices = b
        .list("device")
        .unwrap()
        .into_iter()
        .map(|(_, f)| {
            obop_values::Instant::parse(f["expires_at"].as_str().unwrap())
                .unwrap()
                .micros() as u64
                / 1_000_000
        })
        .collect();
    (grants, refresh, devices)
}

#[tokio::test]
async fn phone_a_microsoft_only_person_is_offered_no_more_than_30_days_and_a_posted_90_is_refused()
{
    let w = world(true).await;
    ada_sso_only(&w).await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    let r = phone.microsoft(&page.flow, ADA_OID).await;
    assert!(consent(&r), "{}", r.body);
    assert_eq!(days_offered(&r), [1, 7, 30], "{}", r.body);
    assert!(
        r.body
            .contains("access can last <strong>30</strong> days at most"),
        "{}",
        r.body
    );
    let page = page_of(&page.flow, &r);
    // A form posted by hand for 90 days gets no code; the page is shown again with the reason,
    // and the flow is still there.
    let d = phone.decide(&page, "90").await;
    assert_eq!(d.status, 200, "{}", d.body);
    assert!(d.body.contains("Choose a shorter one"), "{}", d.body);
    assert_eq!(days_offered(&d), [1, 7, 30]);
    assert!(w.oauth_flow(&page.flow).is_some());
    assert_eq!(w.devices(), 0);
    // 30 is allowed. Every end date is created + 30 days.
    let code = phone.code_of(&page, "30").await;
    let t = phone.token(&code).await;
    assert_eq!(t.status, 200, "{}", t.body);
    let (grants, refresh, devices) = lifetimes(&w);
    assert_eq!((grants.len(), refresh.len(), devices.len()), (1, 1, 1));
    let (created, expires) = grants[0];
    assert_eq!(expires, created + 30 * DAY);
    assert_eq!(refresh[0], created + 30 * DAY);
    assert_eq!(devices[0], created + 30 * DAY);
}

#[tokio::test]
async fn phone_an_account_with_a_password_is_offered_90_days_and_keeps_them() {
    let w = world(true).await;
    ada_sso_only(&w).await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    // Bob signs in with a password and is not Microsoft-only.
    let r = phone.password(&page, "bob", PW).await;
    assert!(consent(&r), "{}", r.body);
    assert_eq!(days_offered(&r), [1, 7, 30, 90]);
    assert!(!r.body.contains("access can last"), "{}", r.body);
    let code = phone.code_of(&page_of(&page.flow, &r), "90").await;
    let t = phone.token(&code).await;
    assert_eq!(t.status, 200, "{}", t.body);
    let (grants, refresh, devices) = lifetimes(&w);
    let (created, expires) = grants[0];
    assert_eq!(expires, created + 90 * DAY);
    assert_eq!((refresh[0], devices[0]), (expires, expires));
    // A person who is bound to Microsoft but does not sign in with it only keeps 90 as well.
    let w = world(true).await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    let r = phone.microsoft(&page.flow, ADA_OID).await;
    assert_eq!(days_offered(&r), [1, 7, 30, 90], "{}", r.body);
}

#[tokio::test]
async fn phone_a_code_made_for_90_days_is_clamped_when_the_account_is_microsoft_only() {
    let w = world(true).await;
    ada_sso_only(&w).await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    let r = phone.microsoft(&page.flow, ADA_OID).await;
    let code = phone.code_of(&page_of(&page.flow, &r), "30").await;
    // The record of the code is the flow's word about the account; here somebody wrote 90 into
    // it (a code made while the account was another kind, or a record written by hand).
    let key = format!("oauth:code:{}", obop_oauth::util::sha_hex(&code));
    let b = w.app.store.backend();
    let mut entry: J = serde_json::from_str(&b.kv_get(&key).unwrap().unwrap()).unwrap();
    assert_eq!(entry["days"], 30);
    entry["days"] = json!(90);
    b.kv_put(&key, &entry.to_string(), Some(120_000)).unwrap();
    let t = phone.token(&code).await;
    assert_eq!(t.status, 200, "{}", t.body);
    let (grants, refresh, devices) = lifetimes(&w);
    let (created, expires) = grants[0];
    assert_eq!(expires, created + 30 * DAY);
    assert_eq!((refresh[0], devices[0]), (expires, expires));
}

#[tokio::test]
async fn phone_a_code_made_for_90_days_fails_when_the_account_becomes_microsoft_only_before_the_exchange()
 {
    let w = world(true).await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    let r = phone.microsoft(&page.flow, ADA_OID).await;
    assert_eq!(days_offered(&r), [1, 7, 30, 90]);
    let code = phone.code_of(&page_of(&page.flow, &r), "90").await;
    // Making the account Microsoft-only changes its credentials (the epoch), which the exchange
    // expects: the code gives nothing and writes nothing.
    ada_sso_only(&w).await;
    let t = phone.token(&code).await;
    assert_eq!(t.status, 400, "{}", t.body);
    assert_eq!(w.devices(), 0);
    assert!(
        w.app
            .store
            .backend()
            .list("oauth_grant")
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn phone_a_store_refusal_anywhere_in_the_exchange_never_gives_a_longer_grant_than_the_cap() {
    use obop_storage::testing::Flaky;
    // Not behind the request guard: a refused read must not be what stops a longer grant from
    // being written afterwards.
    let flaky = Flaky::new(obop_storage::open("memory:").unwrap());
    let app = App::with_store(obop_commands::Store::over(flaky.clone()));
    let w = world_on(app, true, AuthConfig::insecure_dev(), None).await;
    w.keep_throttle();
    ada_sso_only(&w).await;
    let mut phone = Phone::new(&w).await;
    let (mut refused_503, mut served, mut clean) = (0, 0, 0);
    for skip in 0..200 {
        phone.ip = Some(format!("10.6.{}.{}", skip / 200, skip % 200 + 1));
        let page = phone.open().await;
        let r = phone.microsoft(&page.flow, ADA_OID).await;
        let code = phone.code_of(&page_of(&page.flow, &r), "30").await;
        // A code that was made for more than the account may have (written by hand).
        let key = format!("oauth:code:{}", obop_oauth::util::sha_hex(&code));
        let b = w.app.store.backend();
        let mut entry: J = serde_json::from_str(&b.kv_get(&key).unwrap().unwrap()).unwrap();
        entry["days"] = json!(90);
        b.kv_put(&key, &entry.to_string(), Some(120_000)).unwrap();
        let before = flaky.refused();
        flaky.refuse_call_after(skip);
        let t = phone.token(&code).await;
        let refused = flaky.refused() - before;
        flaky.allow_models();
        if refused == 0 {
            assert_eq!(t.status, 200, "{}", t.body);
            clean += 1;
            if clean == 2 {
                break;
            }
            continue;
        }
        match t.status {
            200 => served += 1,
            503 => {
                refused_503 += 1;
                // The code was given back: asking again gives the grant.
                let again = phone.token(&code).await;
                assert!(
                    again.status == 200 || again.status == 400,
                    "{skip}: {}",
                    again.body
                );
            }
            400 => {}
            other => panic!("{skip}: {other}: {}", t.body),
        }
        // Whatever was written, nothing lasts longer than 30 days.
        let (grants, refresh, devices) = lifetimes(&w);
        for (created, expires) in grants {
            assert!(expires <= created + 30 * DAY, "{skip}: a grant of 90 days");
        }
        let latest = now_ms() / 1000 + 30 * DAY + 5;
        for e in refresh.iter().chain(devices.iter()) {
            assert!(*e <= latest, "{skip}: a longer end date");
        }
        if w.devices() >= 18 {
            break;
        }
    }
    assert!(
        clean >= 1 && refused_503 > 0,
        "{clean} {refused_503} {served}"
    );
}

#[tokio::test]
async fn phone_the_pages_of_a_flow_speak_the_language_of_the_flow() {
    // The failure of the Microsoft step is told in the language the sign-in pages were opened
    // in (`lang` of the authorization request, else the browser's). The six translations are
    // machine-drafted.
    let w = world(true).await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open_with("&lang=sv").await;
    assert!(
        page.html.contains("Logga in med Microsoft"),
        "{}",
        page.html
    );
    let cb = phone.round_trip(&page.flow, STRANGER_OID).await;
    assert!(cb.status == 303);
    let r = phone.back(&page.flow).await;
    assert!(
        r.body.contains("Inloggningen med Microsoft fungerade inte"),
        "{}",
        r.body
    );
    // And the cap sentence of the consent page.
    ada_sso_only(&w).await;
    let r = phone.microsoft(&page.flow, ADA_OID).await;
    assert!(
        r.body.contains("i högst <strong>30</strong> dagar"),
        "{}",
        r.body
    );
}

#[tokio::test]
async fn phone_a_store_refusal_anywhere_in_the_decision_never_leaves_a_code_for_more_than_the_cap()
{
    use obop_storage::testing::Flaky;
    // Not behind the request guard, and the answer is not looked at (the layer of D74 turns the
    // answer of a request that met a refusal into a 503): what is looked at is what the store
    // holds, the codes that were made.
    let flaky = Flaky::new(obop_storage::open("memory:").unwrap());
    let app = App::with_store(obop_commands::Store::over(flaky.clone()));
    let w = world_on(app, true, AuthConfig::insecure_dev(), None).await;
    w.keep_throttle();
    ada_sso_only(&w).await;
    let mut phone = Phone::new(&w).await;
    let (mut refused_somewhere, mut clean) = (0, 0);
    for skip in 0..200 {
        phone.ip = Some(format!("10.5.{}.{}", skip / 200, skip % 200 + 1));
        let page = phone.open().await;
        let r = phone.microsoft(&page.flow, ADA_OID).await;
        let p = page_of(&page.flow, &r);
        let before = flaky.refused();
        flaky.refuse_call_after(skip);
        let d = phone.decide(&p, "90").await;
        let refused = flaky.refused() - before;
        flaky.allow_models();
        for (k, v) in w.app.store.backend().kv_scan("oauth:code:").unwrap() {
            let entry: J = serde_json::from_str(&v).unwrap();
            assert!(
                entry["days"].as_u64().unwrap() <= 30,
                "{skip}: a code for {} days ({k})",
                entry["days"]
            );
        }
        assert!(
            !d.all("location").iter().any(|l| l.starts_with(APP)),
            "{skip}: a code was handed to the app for 90 days"
        );
        if refused == 0 {
            clean += 1;
            if clean == 2 {
                break;
            }
        } else {
            refused_somewhere += 1;
        }
    }
    assert!(
        clean >= 1 && refused_somewhere > 3,
        "{clean} {refused_somewhere}"
    );
}

#[tokio::test]
async fn phone_a_store_refusal_anywhere_in_the_consent_page_is_a_503_and_never_a_90_day_page() {
    use obop_storage::testing::Flaky;
    // Whichever read the store refuses, the answer is a 503 and never a page that offers 90 days.
    // (The layer of D74 answers 503 for any refused read of a request, so what tells a guessed cap
    // from a refused page is the unit test `a_refused_read_of_the_account_gives_no_page...` in
    // `obop-oauth`.)
    let flaky = Flaky::new(obop_storage::open("memory:").unwrap());
    let app = App::with_store(obop_commands::Store::over(flaky.clone()));
    let w = world_on(app, true, AuthConfig::insecure_dev(), None).await;
    w.keep_throttle();
    ada_sso_only(&w).await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    let cb = phone.round_trip(&page.flow, ADA_OID).await;
    assert_eq!(cb.status, 303, "{}", cb.body);
    let (mut refused_somewhere, mut shown, mut clean) = (0, 0, 0);
    for skip in 0..60 {
        let before = flaky.refused();
        flaky.refuse_call_after(skip);
        let r = phone.back(&page.flow).await;
        let refused = flaky.refused() - before;
        flaky.allow_models();
        if consent(&r) {
            shown += 1;
            assert_eq!(days_offered(&r), [1, 7, 30], "{skip}: {}", r.body);
        } else {
            assert!(
                !r.body.contains("<option value=\"90\""),
                "{skip}: {}",
                r.body
            );
        }
        if refused == 0 {
            clean += 1;
            break;
        }
        refused_somewhere += 1;
    }
    assert!(
        clean == 1 && refused_somewhere > 0 && shown >= 1,
        "{clean} {refused_somewhere} {shown}"
    );
}

// ---- fix round 1 of the pull request: what the independent check found unpinned -----------------
//
// Each test below passed on the code as it stood (the check wrote most of them as probes) and
// fails under the mutation named in docs/ISSUES.md ("Mutations of the independent check").

fn rig_of(w: &World) -> Rig {
    Rig {
        r: w.router.clone(),
        auth: w.auth.clone(),
    }
}

#[tokio::test]
async fn phone_a_failed_password_try_then_the_return_address_mints_no_second_step() {
    let w = world(true).await;
    let (_secret, _) = enrol(&rig_of(&w), "ada").await; // ada has an authenticator app
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    // A wrong password for ada: the record now says pre_user = ada.
    let wrong = phone.password(&page, "ada", "not the password").await;
    assert!(login_page(&wrong), "{}", wrong.body);
    assert_eq!(w.oauth_flow(&page.flow).unwrap()["pre_user"], "ada");
    let tokens = w.app.store.backend().list("auth_mfa_token").unwrap().len();
    // No Microsoft step was ever done on this flow. The return address must not ask for a code
    // for the account that a password try named, and must not mint a token for it.
    let r = phone.back(&page.flow).await;
    assert!(!asks_for_the_second_step(&r), "{}", r.body);
    assert!(login_page(&r), "{}", r.body);
    assert_eq!(
        w.app.store.backend().list("auth_mfa_token").unwrap().len(),
        tokens,
        "a second-step token was minted"
    );
}

#[tokio::test]
async fn phone_a_parameter_on_the_callback_has_no_say_about_which_flow_is_written() {
    let w = world(true).await;
    let mut phone1 = Phone::new(&w).await;
    let mut phone2 = Phone::new(&w).await;
    let page1 = phone1.open().await;
    let page2 = phone2.open().await;
    let tap = phone1.tap(&page1.flow).await;
    let uri = phone1.at_the_provider(&tap, ADA_OID);
    // Somebody adds the other browser's flow to the callback address.
    let uri = format!("{uri}&flow={}", page2.flow);
    let cb = phone1.callback(&uri).await;
    assert_eq!(
        cb.location(),
        format!("/oauth/authorize/return?flow={}", page1.flow),
        "{}",
        cb.body
    );
    let theirs = w.oauth_flow(&page2.flow).unwrap();
    assert!(
        theirs["user"].is_null()
            && theirs.get("pre_user").is_none()
            && theirs.get("ms_error").is_none(),
        "written into the other flow: {theirs}"
    );
    assert_eq!(w.oauth_flow(&page1.flow).unwrap()["user"], "ada");
}

#[tokio::test]
async fn phone_no_binder_verifier_or_state_is_stored_or_logged() {
    let w = world(true).await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    let value_of = |phone: &Phone<'_>, prefix: &str| {
        phone
            .b
            .cookies
            .iter()
            .find(|c| c.name.starts_with(prefix))
            .map(|c| c.value.clone())
            .unwrap_or_else(|| panic!("no cookie {prefix}"))
    };
    let binder = value_of(&phone, "obop_oauth_");
    let tap = phone.tap(&page.flow).await;
    let verifier = value_of(&phone, "obop_sso_");
    let state = state_of(&tap);
    // While the round trip's row exists (the callback spends it, so this is the time to look at
    // it): in the store (records, flows, counters) and in the lines of the security log.
    let rows = w.sso_rows();
    assert_eq!(rows.len(), 1);
    let mut keys: Vec<String> = rows[0].1.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "expires_at_ms",
            "nonce",
            "oauth_flow",
            "purpose",
            "redirect_uri",
            "verifier_hash"
        ],
        "the row of the round trip holds these and nothing else"
    );
    for secret in [&binder, &verifier, &state] {
        w.assert_not_stored(secret);
    }
    let uri = phone.at_the_provider(&tap, ADA_OID);
    let cb = phone.callback(&uri).await;
    assert_eq!(
        cb.location(),
        format!("/oauth/authorize/return?flow={}", page.flow)
    );
    let r = phone.back(&page.flow).await;
    assert!(consent(&r), "{}", r.body);
    for secret in [&binder, &verifier, &state] {
        w.assert_not_stored(secret);
    }
}

#[tokio::test]
async fn phone_a_reload_of_the_second_step_works_for_an_account_with_a_credential_epoch() {
    let w = world(true).await;
    let (_secret, _) = enrol(&rig_of(&w), "ada").await;
    // A change of the account's state gives it a credential epoch (the reload must compare the
    // epoch the sign-in compared, not "none").
    w.auth.operator_set_disabled("ada", true).unwrap();
    w.auth.operator_set_disabled("ada", false).unwrap();
    let epoch = w.auth.oauth().unwrap().credential_epoch("ada").unwrap();
    assert!(epoch.is_some(), "this test needs an epoch that is not null");
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    let r = phone.microsoft(&page.flow, ADA_OID).await;
    assert!(asks_for_the_second_step(&r), "{}", r.body);
    let rec = w.oauth_flow(&page.flow).unwrap();
    assert_eq!(rec["mfa_pending"]["epoch"], json!(epoch));
    // The baseline the consent compares against is the one the sign-in compared, written with the
    // pending step (and not read later, at the seat).
    assert_eq!(rec["pre_user"], "ada", "{rec}");
    assert_eq!(rec["pre_epoch"], json!(epoch), "{rec}");
    let again = phone.back(&page.flow).await;
    assert!(asks_for_the_second_step(&again), "{}", again.body);
}

#[tokio::test]
async fn phone_a_failed_round_trip_started_earlier_leaves_a_seated_person_as_is() {
    let w = world(true).await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    // Tab 1 taps the link (nobody is signed in yet) and is left at the provider.
    let tap1 = phone.tap(&page.flow).await;
    let state1 = state_of(&tap1);
    // Tab 2 does the whole step and ends on the consent page.
    let r = phone.microsoft(&page.flow, ADA_OID).await;
    assert!(consent(&r), "{}", r.body);
    // Tab 1: the person says no at the provider.
    let uri = format!("/api/auth/oidc/callback?error=access_denied&state={state1}");
    let cb = phone.callback(&uri).await;
    assert_eq!(
        cb.location(),
        format!("/oauth/authorize/return?flow={}", page.flow)
    );
    let rec = w.oauth_flow(&page.flow).unwrap();
    assert_eq!(rec["user"], "ada", "un-seated: {rec}");
    let again = phone.back(&page.flow).await;
    assert!(consent(&again), "{}", again.body);
}

#[tokio::test]
async fn phone_a_reload_of_a_pending_second_step_is_limited_per_address() {
    let w = world(true).await;
    let (_secret, _) = enrol(&rig_of(&w), "ada").await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    let r = phone.microsoft(&page.flow, ADA_OID).await;
    assert!(asks_for_the_second_step(&r), "{}", r.body);
    // Each reload mints a token and counts against the window of the sign-in pages of the
    // address (30 in ten minutes): an endless reload is not an endless supply of tokens.
    let mut statuses = Vec::new();
    for _ in 0..60 {
        statuses.push(phone.back(&page.flow).await.status);
    }
    assert!(statuses.contains(&429), "never limited: {statuses:?}");
    assert!(
        statuses.iter().filter(|s| **s == 200).count() <= 30,
        "{statuses:?}"
    );
}

#[tokio::test]
async fn phone_the_return_address_refuses_what_is_not_a_navigation() {
    let w = world(true).await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    let uri = format!("/oauth/authorize/return?flow={}", page.flow);
    let r = w
        .browse(
            &mut phone.b,
            "GET",
            &uri,
            None,
            &[("sec-fetch-mode", "cors"), ("sec-fetch-dest", "empty")],
        )
        .await;
    assert_eq!(r.status, 400, "{}", r.body);
    assert!(!consent(&r) && !login_page(&r));
}

#[tokio::test]
async fn phone_a_row_for_an_app_that_names_no_flow_never_ends_with_a_code() {
    let w = world(true).await;
    let mut jar = Jar::default();
    let s = w.start(&mut jar).await;
    let rows = w.sso_rows();
    assert_eq!(rows.len(), 1);
    let (key, mut row) = rows.into_iter().next().unwrap();
    // A row that says it is an app's and names no flow (nothing writes one; this is a row that
    // was damaged): it must not be finished as the back office's.
    row["purpose"] = json!("oauth");
    w.app
        .store
        .backend()
        .kv_put(&key, &row.to_string(), Some(600_000))
        .unwrap();
    let uri = w.idp.authorize(&s.location(), ADA_OID);
    let cb = w.callback(&mut jar, &uri).await;
    assert!(
        !cb.location().contains("#oidc="),
        "a code was issued: {}",
        cb.location()
    );
    assert_eq!(w.idp.hits("/token"), 0, "the code was not even redeemed");
}

#[tokio::test]
async fn phone_not_you_forgets_the_person_on_a_microsoft_flow() {
    let w = world(true).await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    let r = phone.microsoft(&page.flow, ADA_OID).await;
    assert!(consent(&r), "{}", r.body);
    let p = page_of(&page.flow, &r);
    let sw = w
        .browse(
            &mut phone.b,
            "POST",
            "/oauth/authorize/decision",
            Some(&[("flow", &p.flow), ("csrf", &p.csrf), ("decision", "switch")]),
            &[],
        )
        .await;
    assert!(login_page(&sw), "{}", sw.body);
    assert!(
        sw.body.contains("/oauth/authorize/microsoft"),
        "the link is offered again"
    );
    let rec = w.oauth_flow(&page.flow).unwrap();
    assert!(rec["user"].is_null(), "{rec}");
    // And the return address no longer shows a person.
    let back = phone.back(&page.flow).await;
    assert!(!consent(&back), "{}", back.body);
}

#[tokio::test]
async fn phone_a_stored_word_outside_the_closed_set_is_shown_as_the_general_failure() {
    let w = world(true).await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    let key = format!("oauth:flow:{}", page.flow);
    let b = w.app.store.backend();
    let mut rec: J = serde_json::from_str(&b.kv_get(&key).unwrap().unwrap()).unwrap();
    // A record written by hand (or by an older build) with the key of a text that is not one of
    // the six: it must not pick another text of the page catalogue.
    rec["ms_error"] = json!("err.login");
    b.kv_put(&key, &rec.to_string(), Some(600_000)).unwrap();
    let r = phone.back(&page.flow).await;
    assert!(login_page(&r) && r.body.contains(FAILED), "{}", r.body);
    assert!(!r.body.contains("The username or password is not right."));
}

#[tokio::test]
async fn phone_a_later_failed_step_ends_a_second_step_that_an_earlier_one_left_pending() {
    let w = world(true).await;
    let (_secret, _) = enrol(&rig_of(&w), "ada").await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    // Step 1: ada's first factor is accepted and the second is asked.
    let r = phone.microsoft(&page.flow, ADA_OID).await;
    assert!(asks_for_the_second_step(&r), "{}", r.body);
    assert!(
        w.oauth_flow(&page.flow)
            .unwrap()
            .get("mfa_pending")
            .is_some()
    );
    // Step 2, from the same page: the person says no at the provider.
    let tap = phone.tap(&page.flow).await;
    let state = state_of(&tap);
    let uri = format!("/api/auth/oidc/callback?error=access_denied&state={state}");
    let cb = phone.callback(&uri).await;
    assert_eq!(
        cb.location(),
        format!("/oauth/authorize/return?flow={}", page.flow)
    );
    // The page shows the latest outcome, and the earlier first factor stands for nothing.
    let rec = w.oauth_flow(&page.flow).unwrap();
    assert!(rec.get("mfa_pending").is_none(), "{rec}");
    let tokens = w.app.store.backend().list("auth_mfa_token").unwrap().len();
    let r = phone.back(&page.flow).await;
    assert!(
        login_page(&r) && r.body.contains(CANCELLED) && !asks_for_the_second_step(&r),
        "{}",
        r.body
    );
    assert_eq!(
        w.app.store.backend().list("auth_mfa_token").unwrap().len(),
        tokens,
        "a token was minted for a step that had ended"
    );
}

#[tokio::test]
async fn phone_a_provider_switched_off_between_the_hop_and_the_reload_ends_the_second_step() {
    let w = world(true).await;
    let (_secret, _) = enrol(&rig_of(&w), "ada").await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    let r = phone.microsoft(&page.flow, ADA_OID).await;
    assert!(asks_for_the_second_step(&r), "{}", r.body);
    let tokens = w.app.store.backend().list("auth_mfa_token").unwrap().len();
    // The administrator switches the provider off. The web leg would refuse a code at this point;
    // the reload must not mint a second-step token for it either.
    w.auth
        .operator_sso_configure(TENANT, CLIENT, None, Some(false))
        .unwrap();
    let again = phone.back(&page.flow).await;
    assert!(
        !asks_for_the_second_step(&again) && !consent(&again),
        "{}",
        again.body
    );
    assert_eq!(
        w.app.store.backend().list("auth_mfa_token").unwrap().len(),
        tokens,
        "a token was minted after the provider was switched off"
    );
    assert!(
        w.events("oidc_refused")
            .iter()
            .any(|l| l.contains("switched off before the sign-in finished")),
        "{:?}",
        w.events("oidc_refused")
    );
}

#[tokio::test]
async fn phone_three_lost_races_write_nothing_and_the_person_sees_the_general_failure() {
    use obop_storage::testing::Flaky;
    let (flaky, backend) = Flaky::memory();
    let app = App::with_store(obop_commands::Store::over(backend));
    let w = world_on(app, true, AuthConfig::insecure_dev(), None).await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    let tap = phone.tap(&page.flow).await;
    let uri = phone.at_the_provider(&tap, ADA_OID);
    // Three other requests change the record of the flow, one after the other, just before the
    // callback's write is tried: the callback gives up and writes nothing.
    let key = format!("oauth:flow:{}", page.flow);
    fn interfere(flaky: &std::sync::Arc<Flaky>, key: String, left: u32) {
        if left == 0 {
            return;
        }
        let again = flaky.clone();
        flaky.before_the_next_kv_swap(Box::new(move |b| {
            let raw = b.kv_get(&key).unwrap().unwrap();
            let mut rec: J = serde_json::from_str(&raw).unwrap();
            rec["marker"] = json!(left);
            b.kv_put(&key, &rec.to_string(), Some(600_000)).unwrap();
            interfere(&again, key.clone(), left - 1);
        }));
    }
    interfere(&flaky, key, 3);
    let cb = phone.callback(&uri).await;
    assert_eq!(
        cb.location(),
        format!("/oauth/authorize/return?flow={}", page.flow),
        "{}",
        cb.body
    );
    let rec = w.oauth_flow(&page.flow).unwrap();
    assert!(
        rec["user"].is_null() && rec.get("ms_error").is_none(),
        "{rec}"
    );
    // What the person sees is the general failure text, not the busy text and not a page of a
    // person. (The sign-in itself did happen at the provider and is in the log.)
    let r = phone.back(&page.flow).await;
    assert!(login_page(&r) && r.body.contains(FAILED), "{}", r.body);
    assert!(!r.body.contains(BUSY));
    assert!(!w.events("oidc_login").is_empty());
    // And the person can go on: a new tap works.
    let r = phone.microsoft(&page.flow, ADA_OID).await;
    assert!(consent(&r), "{}", r.body);
}

#[tokio::test]
async fn phone_a_bound_account_that_must_choose_a_new_password_is_told_so_after_microsoft() {
    let w = world(true).await;
    ada_sso_only(&w).await;
    // Turning the setting off gives the password back, to be changed at the next sign-in.
    w.auth.operator_sso_only("ada", false).unwrap();
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    let r = phone.microsoft(&page.flow, ADA_OID).await;
    assert!(
        login_page(&r) && r.body.contains("You must choose a new password"),
        "{}",
        r.body
    );
    let rec = w.oauth_flow(&page.flow).unwrap();
    assert!(rec["user"].is_null(), "{rec}");
    assert_eq!(rec["ms_error"], "pwchange");
    // A failed step names no account in the flow: the page tells the person the word, and the
    // record does not say who tried.
    assert!(
        !rec.to_string().contains("\"ada\""),
        "the failed step wrote the account into the flow: {rec}"
    );
    assert_eq!(w.devices(), 0);
}

#[tokio::test]
async fn phone_the_cookies_of_the_round_trip_and_of_the_flow_last_ten_minutes_and_no_longer() {
    let w = world(true).await;
    let mut phone = Phone::new(&w).await;
    let page = phone.open().await;
    // The lifetime of the cookie of the sign-in pages and of the round trip's verifier, as the
    // browser keeps them (the jar of this file runs on a clock of its own).
    let cookie_of = |phone: &Phone<'_>, prefix: &str| {
        phone
            .b
            .cookies
            .iter()
            .find(|c| c.name.starts_with(prefix))
            .cloned()
            .unwrap_or_else(|| panic!("no cookie {prefix}"))
    };
    let binder = cookie_of(&phone, "obop_oauth_");
    assert_eq!((binder.path.as_str(), binder.until), ("/oauth", Some(600)));
    let tap = phone.tap(&page.flow).await;
    let verifier = cookie_of(&phone, "obop_sso_");
    assert_eq!(
        (verifier.path.as_str(), verifier.until),
        ("/api/auth/oidc", Some(600))
    );
    let uri = phone.at_the_provider(&tap, ADA_OID);
    // The person is away for ten minutes and a second: the browser no longer sends either cookie.
    phone.b.now = 601;
    assert!(!phone.b.has("obop_oauth_") && !phone.b.has("obop_sso_"));
    let cb = phone.callback(&uri).await;
    assert_eq!(
        cb.location(),
        format!("/oauth/authorize/return?flow={}", page.flow),
        "{}",
        cb.body
    );
    assert_eq!(w.idp.hits("/token"), 0, "the code was not redeemed");
    // The return address is shown to nobody without the cookie of the flow; nothing was seated
    // and nothing was made again.
    let r = phone.back(&page.flow).await;
    assert!(
        [403, 410].contains(&r.status) && !consent(&r) && !login_page(&r),
        "{} {}",
        r.status,
        r.body
    );
    assert!(w.oauth_flow(&page.flow).unwrap()["user"].is_null());
    assert_eq!(w.devices(), 0);
}
