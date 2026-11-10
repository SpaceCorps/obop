//! Signing a Document through a link (D92) against the real router, the real authenticator and the
//! bundled `documents` and `sign` packages. Every test has an instance of its own (an in-memory
//! store), so rate-limit counters are never shared between tests. Everything signed here is a
//! made-up PDF (`%PDF-1.4` and invented text) and a made-up person.

use crate::app::App;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use obop_auth::Auth;
use obop_commands::Store;
use obop_pkg::{Bundled, Marketplace};
use obop_storage::blob::{Catalog, MemoryBytes};
use serde_json::{Value as J, json};
use std::sync::Arc;
use tower::ServiceExt;

mod brand;
mod door;
mod hardening;
mod history;
mod pinned;
mod recheck;
mod rules;
mod upgrade;

const PW: &str = "a long enough password";
const HOST: &str = "inst.test";
const ORIGIN: &str = "http://inst.test";

/// A synthetic PDF; `n` makes the files differ.
pub(crate) fn pdf(n: u32) -> Vec<u8> {
    format!("%PDF-1.4\n% made-up test document {n}\n%%EOF\n").into_bytes()
}

pub(crate) struct Resp {
    pub status: u16,
    pub headers: axum::http::HeaderMap,
    pub body: Vec<u8>,
}

impl Resp {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
    pub fn h(&self, name: &str) -> &str {
        self.headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    }
    pub fn json(&self) -> J {
        serde_json::from_slice(&self.body).unwrap_or(J::Null)
    }
}

pub(crate) struct Fx {
    pub router: axum::Router,
    /// The state behind `router`, for what a test counts (the log lines about a refused origin).
    pub state: Arc<obop_http::HttpState>,
    pub auth: Arc<Auth>,
    pub app: App,
    pub tokens: std::collections::HashMap<&'static str, String>,
    /// The restricted Document, its request, and the file that was attached.
    pub doc: String,
    pub req: String,
    pub file: String,
    /// The bytes of the blob store when it is the in-memory one, so that a test can tamper.
    pub bytes: Option<Arc<MemoryBytes>>,
    _scratch: Option<obop_storage::testing::TestDb>,
}

pub(crate) async fn send(r: &axum::Router, req: Request<Body>) -> Resp {
    let resp = r.clone().oneshot(req).await.unwrap();
    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    let body = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    Resp {
        status,
        headers,
        body,
    }
}

impl Fx {
    pub fn tok(&self, who: &str) -> &str {
        &self.tokens[who]
    }

    /// A JSON command call as `who` (`""` for nobody); returns the status and the body.
    pub async fn api(&self, who: &str, path: &str, body: J) -> (u16, J) {
        let mut b = Request::post(path)
            .header("content-type", "application/json")
            .header("host", HOST);
        if !who.is_empty() {
            b = b.header("authorization", format!("Bearer {}", self.tok(who)));
        }
        let r = send(&self.router, b.body(Body::from(body.to_string())).unwrap()).await;
        (r.status, r.json())
    }

    pub async fn upload(&self, who: &str, field: &str, ct: &str, bytes: Vec<u8>) -> (u16, J) {
        let r = send(
            &self.router,
            Request::post(format!("/api/blobs?field={field}&filename=nda.pdf"))
                .header("content-type", ct)
                .header("host", HOST)
                .header("authorization", format!("Bearer {}", self.tok(who)))
                .body(Body::from(bytes))
                .unwrap(),
        )
        .await;
        (r.status, r.json())
    }

    pub async fn blob(&self, who: &str, id: &str) -> Resp {
        send(
            &self.router,
            Request::get(format!("/api/blobs/{id}"))
                .header("host", HOST)
                .header("authorization", format!("Bearer {}", self.tok(who)))
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    /// The door: a GET. `from` is the client address the proxy reports.
    pub async fn get(&self, path: &str, from: &str) -> Resp {
        self.get_with(path, from, &[]).await
    }

    pub async fn get_with(&self, path: &str, from: &str, extra: &[(&str, &str)]) -> Resp {
        let mut b = Request::get(path)
            .header("host", HOST)
            .header("x-forwarded-for", from)
            .header("user-agent", "TestBrowser/1.0");
        for (k, v) in extra {
            b = b.header(*k, *v);
        }
        send(&self.router, b.body(Body::empty()).unwrap()).await
    }

    /// The door: the signer's post.
    pub async fn post(&self, path: &str, from: &str, pairs: &[(&str, &str)]) -> Resp {
        self.post_with(path, from, pairs, &[("origin", ORIGIN)])
            .await
    }

    pub async fn post_with(
        &self,
        path: &str,
        from: &str,
        pairs: &[(&str, &str)],
        extra: &[(&str, &str)],
    ) -> Resp {
        let body = pairs
            .iter()
            .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
            .collect::<Vec<_>>()
            .join("&");
        let mut b = Request::post(path)
            .header("host", HOST)
            .header("content-type", "application/x-www-form-urlencoded")
            .header("x-forwarded-for", from);
        // The helper's browser is the default; a test that names one gets only its own (two
        // headers would be read as the first).
        if !extra
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("user-agent"))
        {
            b = b.header("user-agent", "TestBrowser/1.0");
        }
        for (k, v) in extra {
            b = b.header(*k, *v);
        }
        send(&self.router, b.body(Body::from(body)).unwrap()).await
    }

    /// As `post`, with the browser string as these bytes (which need not be text).
    pub async fn post_with_agent(
        &self,
        path: &str,
        from: &str,
        pairs: &[(&str, &str)],
        agent: &[u8],
    ) -> Resp {
        let body = pairs
            .iter()
            .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
            .collect::<Vec<_>>()
            .join("&");
        let req = Request::post(path)
            .header("host", HOST)
            .header("origin", ORIGIN)
            .header("content-type", "application/x-www-form-urlencoded")
            .header("x-forwarded-for", from)
            .header(
                "user-agent",
                axum::http::HeaderValue::from_bytes(agent).unwrap(),
            )
            .body(Body::from(body))
            .unwrap();
        send(&self.router, req).await
    }

    pub fn service(&self) -> Arc<obop_commands::SignService> {
        self.app.dispatcher.sign_service().expect("sign service")
    }

    pub fn record(&self, model: &str, id: &str) -> serde_json::Map<String, J> {
        self.app
            .store
            .backend()
            .get(model, id)
            .unwrap()
            .unwrap_or_else(|| panic!("no {model} {id}"))
    }

    /// Mint a link for the request as `who` and return (link record id, path).
    pub async fn mint(&self, who: &str, req: &str) -> (String, String) {
        let (s, v) = self
            .api(who, "/api/sign_link/create", json!({"request_id": req}))
            .await;
        assert_eq!(s, 200, "{v}");
        let link = v["value"]["link"].as_str().unwrap().to_string();
        assert!(link.starts_with(&format!("/sign/{req}/")), "{link}");
        (v["value"]["id"].as_str().unwrap().to_string(), link)
    }

    /// A second router over the same instance that does not trust a proxy (the default of a
    /// server started without `--trust-proxy`).
    pub fn router_without_proxy(&self) -> axum::Router {
        let state = http_state(&self.app, self.auth.clone());
        obop_http::router(state, None)
    }

    /// Every record of the models that could hold a secret, and every key-value row, as text.
    pub fn everything_stored(&self) -> String {
        let b = self.app.store.backend();
        let mut out = String::new();
        for m in [
            "sign_link",
            "sign_request",
            "sign_template",
            "documents_document",
            "documents_folder",
            "security_event",
        ] {
            for (id, rec) in b.list(m).unwrap() {
                out.push_str(&format!("{m} {id} {}\n", J::Object(rec)));
            }
        }
        for (k, v) in b.kv_scan("").unwrap() {
            out.push_str(&format!("kv {k} {v}\n"));
        }
        out
    }

    /// The security log entries of one kind, oldest first.
    pub fn events_of(&self, kind: &str) -> Vec<serde_json::Map<String, J>> {
        self.events()
            .into_iter()
            .filter(|e| e.get("kind").and_then(J::as_str) == Some(kind))
            .collect()
    }

    /// Everything the security log says, oldest first.
    pub fn events(&self) -> Vec<serde_json::Map<String, J>> {
        self.app
            .store
            .backend()
            .list("security_event")
            .unwrap()
            .into_iter()
            .map(|(_, f)| f)
            .collect()
    }
}

/// The HTTP state of a fixture. It does not read the environment: `OBOP_PUBLIC_URL` and
/// `OBOP_ALLOWED_ORIGINS` of whoever runs the tests (a developer with an instance's settings in the
/// shell) would otherwise change the origin the door accepts and the links it makes.
pub(crate) fn http_state(app: &App, auth: Arc<Auth>) -> Arc<obop_http::HttpState> {
    if let Some(svc) = app.dispatcher.sign_service() {
        svc.set_public_url(None);
    }
    obop_http::HttpState::with_mcp(
        app.dispatcher.clone(),
        auth,
        obop_http::mcp::McpConfig::default(),
    )
}

pub(crate) fn enc(s: &str) -> String {
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

/// The address a test reports: every test passes its own so no two share a source.
pub(crate) fn ip(n: u8) -> String {
    format!("198.51.100.{n}")
}

/// `documents` and `sign` installed on a fresh instance with five people and one restricted
/// Document that `ann` owns, with a draft request about it that `mia` made.
pub(crate) async fn fixture() -> Fx {
    let backend = obop_storage::open("memory:").unwrap();
    let bytes = Arc::new(MemoryBytes::default());
    let store =
        Store::over(backend.clone()).with_blobs(Arc::new(Catalog::new(backend, bytes.clone())));
    fixture_on(store, Some(bytes), None).await
}

pub(crate) async fn fixture_on(
    store: Store,
    bytes: Option<Arc<MemoryBytes>>,
    scratch: Option<obop_storage::testing::TestDb>,
) -> Fx {
    let mut f = fixture_sync(store, bytes, scratch);
    seed(&mut f).await;
    f
}

/// The people every fixture has.
const PEOPLE: [(&str, &[&str]); 9] = [
    ("admin", &["platform.admin"]),
    (
        "mia",
        &[
            "sign.user",
            "sign.manager",
            "documents.user",
            "documents.manager",
        ],
    ),
    ("ann", &["sign.user", "sign.manager", "documents.user"]),
    ("bob", &["sign.user", "sign.manager", "documents.user"]),
    ("sam", &["sign.user"]),
    // Sends nothing and mints nothing: no sign.manager.
    ("dan", &["sign.user", "documents.user", "documents.manager"]),
    // A manager with no role in Documents.
    ("noc", &["sign.user", "sign.manager"]),
    // A manager who is not a sign.user: makes links, cannot read requests as a plain user.
    (
        "lee",
        &["sign.manager", "documents.user", "documents.manager"],
    ),
    // An administrator who holds the other groups literally, so that she can give an agent a
    // token with `platform.admin` among its scopes.
    (
        "root",
        &[
            "platform.admin",
            "sign.user",
            "sign.manager",
            "documents.user",
            "documents.manager",
        ],
    ),
];

/// The instance: packages installed and people created, with no request made yet. Not async, so
/// that a test on PostgreSQL (whose synchronous client cannot run inside a tokio runtime) can
/// call it before it starts one; `seed` then makes the sign-ins and the records.
pub(crate) fn fixture_sync(
    store: Store,
    bytes: Option<Arc<MemoryBytes>>,
    scratch: Option<obop_storage::testing::TestDb>,
) -> Fx {
    fixture_sync_with(store, bytes, scratch, &Bundled.fetch("sign").unwrap())
}

/// As `fixture_sync`, with the text of the `sign` package given: an instance that still has an
/// older version of it (the order of a deploy: the new binary first, the package later).
pub(crate) fn fixture_sync_with(
    store: Store,
    bytes: Option<Arc<MemoryBytes>>,
    scratch: Option<obop_storage::testing::TestDb>,
    sign: &str,
) -> Fx {
    let app = App::with_store(store);
    let auth = Arc::new(Auth::with_test_params(app.store.backend()));
    for (id, text) in [
        ("documents", Bundled.fetch("documents").unwrap()),
        ("sign", sign.to_string()),
    ] {
        app.installer
            .install(&text, "marketplace", "test")
            .unwrap_or_else(|e| panic!("{id}: {e}"));
    }
    for (name, groups) in PEOPLE {
        let g: Vec<String> = groups.iter().map(|g| g.to_string()).collect();
        auth.create_user(name, PW, &g).unwrap();
    }
    let state = http_state(&app, auth.clone());
    state.set_trust_proxy(true);
    let router = obop_http::router(state.clone(), None);
    Fx {
        router,
        state,
        auth,
        app,
        tokens: Default::default(),
        doc: String::new(),
        req: String::new(),
        file: String::new(),
        bytes,
        _scratch: scratch,
    }
}

/// Sign everybody in and make the restricted Document and a draft request about it.
pub(crate) async fn seed(f: &mut Fx) {
    for (name, _) in PEOPLE {
        let (s, v) = f
            .api(
                "",
                "/api/auth/login",
                json!({"username": name, "password": PW}),
            )
            .await;
        assert_eq!(s, 200, "{v}");
        f.tokens
            .insert(name, v["token"].as_str().unwrap().to_string());
    }
    let (s, v) = f
        .upload(
            "ann",
            "documents_document.attachment",
            "application/pdf",
            pdf(1),
        )
        .await;
    assert_eq!(s, 201, "{v}");
    f.file = v["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api(
            "ann",
            "/api/documents_document/create",
            json!({"title": "Mutual NDA (test) <i>x</i>", "kind": "contract", "confidentiality": "restricted",
                   "state": "draft", "owner": "ann", "attachment": f.file}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    f.doc = v["value"]["id"].as_str().unwrap().to_string();
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/create",
            json!({"title": "Please sign the test NDA", "message": "Made-up message <b>x</b>",
                   "signer_name": "Jane Test", "document_id": f.doc, "state": "draft"}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    f.req = v["value"]["id"].as_str().unwrap().to_string();
}

/// A fixture whose request has been sent: returns it with a link made by `mia`, as
/// (link id, path, token).
pub(crate) async fn sent() -> (Fx, String, String, String) {
    let f = fixture().await;
    let (s, v) = f
        .api(
            "mia",
            "/api/sign_request/send_document",
            json!({"id": f.req}),
        )
        .await;
    assert_eq!(s, 200, "{v}");
    let (id, path) = f.mint("mia", &f.req.clone()).await;
    let token = path.rsplit('/').next().unwrap().to_string();
    (f, id, path, token)
}
