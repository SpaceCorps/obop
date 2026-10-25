//! `/healthz`, `/readyz` and `/metrics` (D67) end to end over HTTP against a real `App`.

use crate::app::App;
use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{HeaderMap, Request},
};
use http_body_util::BodyExt;
use obop_auth::Auth;
use obop_commands::Authenticator;
use obop_http::ops::MetricsAccess;
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use tower::ServiceExt;

const TOKEN: &str = "metrics-token-0123456789";

struct Resp {
    status: u16,
    headers: HeaderMap,
    body: String,
}

async fn send(r: &axum::Router, req: Request<Body>) -> Resp {
    let resp = r.clone().oneshot(req).await.unwrap();
    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    let body =
        String::from_utf8_lossy(&resp.into_body().collect().await.unwrap().to_bytes()).to_string();
    Resp {
        status,
        headers,
        body,
    }
}

fn get(path: &str) -> Request<Body> {
    Request::get(path).body(Body::empty()).unwrap()
}

fn from_peer(path: &str, peer: &str, headers: &[(&str, &str)]) -> Request<Body> {
    let mut r = Request::get(path);
    for (k, v) in headers {
        r = r.header(*k, *v);
    }
    let mut req = r.body(Body::empty()).unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(peer.parse::<SocketAddr>().unwrap()));
    req
}

struct Fixture {
    app: App,
    auth: Arc<Auth>,
    state: Arc<obop_http::HttpState>,
    router: axum::Router,
}

/// What opening a database does at start: the version record says it is at the latest version.
fn stamp(app: &App) {
    let b = app.store.backend();
    let ring = obop_keys::KeyRing::for_backend(&b).unwrap();
    let latest = obop_migrate::Migrator::new(b, ring).latest();
    set_schema_version(app, latest);
}

fn fixture(access: MetricsAccess) -> Fixture {
    let app = App::new();
    stamp(&app);
    let auth = Arc::new(Auth::with_test_params(app.store.backend()));
    auth.create_user("ada", "correct horse battery", &app.all_groups())
        .unwrap();
    let state = obop_http::HttpState::new(app.dispatcher.clone(), auth.clone());
    state.set_metrics_access(access);
    let router = obop_http::router(state.clone(), None);
    Fixture {
        app,
        auth,
        state,
        router,
    }
}

fn set_schema_version(app: &App, v: u32) {
    let mut f = obop_storage::Fields::new();
    f.insert("version".into(), json!(v));
    let b = app.store.backend();
    if !b.update("obop_meta", "schema", &f).unwrap() {
        b.insert("obop_meta", "schema", &f).unwrap();
    }
}

#[tokio::test]
async fn healthz_answers_200_with_a_fixed_body_and_needs_nothing() {
    let f = fixture(MetricsAccess::Disabled);
    let r = send(&f.router, get("/healthz")).await;
    assert_eq!(r.status, 200);
    assert_eq!(r.body, "{\"status\":\"ok\"}");
    assert_eq!(r.headers["cache-control"], "no-store");
    assert_eq!(r.headers["content-type"], "application/json");
}

#[tokio::test]
async fn readyz_is_200_when_the_database_answers_and_the_schema_fits() {
    let f = fixture(MetricsAccess::Disabled);
    let r = send(&f.router, get("/readyz")).await;
    assert_eq!(r.status, 200, "{}", r.body);
    assert_eq!(r.body, "{\"status\":\"ready\"}");
}

#[tokio::test]
async fn readyz_is_503_with_a_short_reason_when_the_database_is_newer_than_the_server() {
    let f = fixture(MetricsAccess::Disabled);
    set_schema_version(&f.app, 9999);
    let r = send(&f.router, get("/readyz")).await;
    assert_eq!(r.status, 503);
    let j: serde_json::Value = serde_json::from_str(&r.body).unwrap();
    assert_eq!(j["status"], "unavailable");
    assert_eq!(j["reason"], "database is newer than this server");
    assert!(r.body.len() < 100, "{}", r.body);
    assert!(!r.body.contains("9999"), "no version numbers");
}

#[tokio::test]
async fn readyz_is_503_when_the_database_still_needs_migration_steps() {
    let f = fixture(MetricsAccess::Disabled);
    // Records exist and the version record says the database predates the latest steps.
    set_schema_version(&f.app, 0);
    let r = send(&f.router, get("/readyz")).await;
    assert_eq!(r.status, 503, "{}", r.body);
    assert!(r.body.contains("database needs migration"), "{}", r.body);
}

#[tokio::test]
async fn readyz_shares_one_database_check_for_a_couple_of_seconds() {
    let f = fixture(MetricsAccess::Disabled);
    assert_eq!(send(&f.router, get("/readyz")).await.status, 200);
    // The database turns bad, but a flood of probes reads the cached verdict, not the database.
    set_schema_version(&f.app, 9999);
    assert_eq!(send(&f.router, get("/readyz")).await.status, 200);
}

#[tokio::test]
async fn health_endpoints_are_open_and_reveal_nothing_but_status_and_a_fixed_reason() {
    let f = fixture(MetricsAccess::token(TOKEN));
    for path in ["/healthz", "/readyz"] {
        let r = send(&f.router, get(path)).await;
        for secret in [
            "ada", "sqlite", "memory", "postgres", "obop_", "0.2.0", "token",
        ] {
            assert!(
                !r.body.contains(secret),
                "{path} leaks {secret}: {}",
                r.body
            );
        }
        assert!(!r.headers.contains_key("server"));
    }
}

#[tokio::test]
async fn metrics_does_not_exist_unless_the_operator_opened_it() {
    let f = fixture(MetricsAccess::Disabled);
    assert_eq!(send(&f.router, get("/metrics")).await.status, 404);
    let r = send(
        &f.router,
        Request::get("/metrics")
            .header("authorization", format!("Bearer {TOKEN}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(r.status, 404, "a token does nothing while it is closed");
    // The default of a fresh state is closed as well.
    let app = App::new();
    let state = obop_http::HttpState::new(app.dispatcher.clone(), Arc::new(crate::app::DevAuth));
    let r = send(&obop_http::router(state, None), get("/metrics")).await;
    assert_eq!(r.status, 404);
}

#[tokio::test]
async fn metrics_with_a_token_refuses_missing_and_wrong_tokens() {
    let f = fixture(MetricsAccess::token(TOKEN));
    let none = send(&f.router, get("/metrics")).await;
    assert_eq!(none.status, 401);
    assert!(none.body.is_empty());
    for bad in [
        "Bearer wrong-token-0123456789",
        "Bearer ",
        "Basic bWV0cmljcw==",
        TOKEN,
    ] {
        let r = send(
            &f.router,
            Request::get("/metrics")
                .header("authorization", bad)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(r.status, 401, "{bad}");
    }
    // A person's session or an agent token is not the metrics token.
    let session = f.auth.login("ada", "correct horse battery").unwrap();
    let r = send(
        &f.router,
        Request::get("/metrics")
            .header("authorization", format!("Bearer {session}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(r.status, 401);
}

fn ok_metrics() -> Request<Body> {
    Request::get("/metrics")
        .header("authorization", format!("Bearer {TOKEN}"))
        .body(Body::empty())
        .unwrap()
}

fn value(body: &str, series: &str) -> f64 {
    body.lines()
        .find_map(|l| l.strip_prefix(series).and_then(|v| v.strip_prefix(' ')))
        .unwrap_or_else(|| panic!("no series {series} in\n{body}"))
        .parse()
        .unwrap()
}

#[tokio::test]
async fn metrics_serves_prometheus_text_with_the_minimal_set() {
    let f = fixture(MetricsAccess::token(TOKEN));
    let r = send(&f.router, ok_metrics()).await;
    assert_eq!(r.status, 200, "{}", r.body);
    assert_eq!(
        r.headers["content-type"],
        "text/plain; version=0.0.4; charset=utf-8"
    );
    assert_eq!(r.headers["cache-control"], "no-store");
    for needle in [
        "# TYPE obop_build_info gauge",
        &format!(
            "obop_build_info{{core_version=\"{}\"}} 1",
            obop_types::CORE_VERSION
        ),
        "# TYPE obop_process_uptime_seconds gauge",
        "# TYPE obop_http_requests_total counter",
        "# TYPE obop_http_request_duration_seconds histogram",
        "obop_http_request_duration_seconds_bucket{le=\"+Inf\"}",
        "obop_db_up 1",
        "# TYPE obop_db_ping_seconds gauge",
        "# TYPE obop_active_sessions gauge",
    ] {
        assert!(r.body.contains(needle), "missing {needle}\n{}", r.body);
    }
    // No job queue series without the connectors runtime attached to the HTTP state.
    assert!(!r.body.contains("obop_job_queue_depth"));
}

#[tokio::test]
async fn metrics_count_responses_by_status_class_and_fill_the_histogram_consistently() {
    let f = fixture(MetricsAccess::token(TOKEN));
    assert_eq!(send(&f.router, get("/healthz")).await.status, 200);
    assert_eq!(send(&f.router, get("/no-such-path")).await.status, 404);
    assert_eq!(send(&f.router, get("/api/commands")).await.status, 401);
    assert_eq!(send(&f.router, get("/api/commands")).await.status, 401);
    let r = send(&f.router, ok_metrics()).await;
    let b = &r.body;
    assert_eq!(value(b, "obop_http_requests_total{class=\"2xx\"}"), 1.0);
    assert!(value(b, "obop_http_requests_total{class=\"4xx\"}") >= 3.0);
    assert_eq!(value(b, "obop_http_requests_total{class=\"5xx\"}"), 0.0);
    // Cumulative buckets never decrease and end at the count.
    let buckets: Vec<f64> = b
        .lines()
        .filter(|l| l.starts_with("obop_http_request_duration_seconds_bucket"))
        .map(|l| l.rsplit(' ').next().unwrap().parse().unwrap())
        .collect();
    assert_eq!(buckets.len(), 12);
    assert!(buckets.windows(2).all(|w| w[0] <= w[1]), "{buckets:?}");
    let count = value(b, "obop_http_request_duration_seconds_count");
    assert_eq!(*buckets.last().unwrap(), count);
    assert!(count >= 4.0);
    assert!(value(b, "obop_http_request_duration_seconds_sum") >= 0.0);
}

#[tokio::test]
async fn metrics_report_active_sessions_without_naming_anyone() {
    let f = fixture(MetricsAccess::token(TOKEN));
    assert_eq!(
        value(
            &send(&f.router, ok_metrics()).await.body,
            "obop_active_sessions"
        ),
        0.0
    );
    // The count is read at most every 15 seconds; a fresh state sees the new session.
    let session = f.auth.login("ada", "correct horse battery").unwrap();
    let router = obop_http::router(
        {
            let s = obop_http::HttpState::new(f.app.dispatcher.clone(), f.auth.clone());
            s.set_metrics_access(MetricsAccess::token(TOKEN));
            s
        },
        None,
    );
    let r = send(&router, ok_metrics()).await;
    assert_eq!(value(&r.body, "obop_active_sessions"), 1.0);
    assert!(!r.body.contains("ada"));
    assert!(!r.body.contains(&session));
    let _ = &f.state;
}

#[tokio::test]
async fn metrics_on_loopback_is_for_direct_loopback_callers_only() {
    let f = fixture(MetricsAccess::Loopback);
    let ok = send(&f.router, from_peer("/metrics", "127.0.0.1:5000", &[])).await;
    assert_eq!(ok.status, 200);
    let v6 = send(&f.router, from_peer("/metrics", "[::1]:5000", &[])).await;
    assert_eq!(v6.status, 200);
    // A reverse proxy on the same host would otherwise publish it to the internet.
    for h in [
        ("x-forwarded-for", "203.0.113.9"),
        ("x-forwarded-host", "example.com"),
        ("x-real-ip", "203.0.113.9"),
        ("forwarded", "for=203.0.113.9"),
        ("x-forwarded-proto", "https"),
    ] {
        let r = send(&f.router, from_peer("/metrics", "127.0.0.1:5000", &[h])).await;
        assert_eq!(r.status, 404, "{h:?}");
    }
    assert_eq!(
        send(&f.router, from_peer("/metrics", "203.0.113.9:5000", &[]))
            .await
            .status,
        404
    );
    assert_eq!(
        send(&f.router, get("/metrics")).await.status,
        404,
        "no peer address known means no access"
    );
}

#[tokio::test]
async fn health_and_metrics_are_not_shadowed_by_the_backoffice_or_the_public_site() {
    let dir = std::env::temp_dir().join(format!("obop-health-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("index.html"), "<html>BACKOFFICE-SHELL</html>").unwrap();
    for public in [false, true] {
        let app = App::new();
        stamp(&app);
        let auth = Arc::new(Auth::with_test_params(app.store.backend()));
        let state = obop_http::HttpState::new(app.dispatcher.clone(), auth);
        state.set_metrics_access(MetricsAccess::token(TOKEN));
        let router = if public {
            let site = Arc::new(obop_site::Site::new(
                obop_commands::PublicApi::new(app.dispatcher.clone(), app.store.clone()),
                obop_site::Config::default(),
            ));
            obop_http::public::router(
                state,
                dir.to_str(),
                site,
                obop_http::public::PublicConfig::default(),
            )
        } else {
            obop_http::router(state, dir.to_str())
        };
        let h = send(&router, get("/healthz")).await;
        assert_eq!(
            (h.status, h.body.as_str()),
            (200, "{\"status\":\"ok\"}"),
            "public={public}"
        );
        let r = send(&router, get("/readyz")).await;
        assert_eq!(r.status, 200, "public={public}: {}", r.body);
        assert!(r.body.contains("ready") && !r.body.contains("BACKOFFICE"));
        let m = send(&router, ok_metrics()).await;
        assert_eq!(m.status, 200, "public={public}");
        assert!(m.body.contains("obop_build_info"));
        if public {
            // Requests answered by the public site are counted too.
            let _ = send(&router, get("/no-such-page")).await;
            let m = send(&router, ok_metrics()).await;
            assert!(value(&m.body, "obop_http_requests_total{class=\"4xx\"}") >= 1.0);
        }
    }
}

#[tokio::test]
async fn metrics_report_the_backup_catalogue_and_the_last_drill() {
    use obop_ops::catalog::{BackupRecord, Catalog, Copy, DrillRecord};
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    // Nothing recorded yet: the age series are absent (alert on absent()), the failure flag is 0.
    let f = fixture(MetricsAccess::token(TOKEN));
    let r = send(&f.router, ok_metrics()).await;
    assert!(!r.body.contains("obop_backup_age_seconds"), "{}", r.body);
    assert!(!r.body.contains("obop_restore_drill_age_seconds"));
    assert_eq!(value(&r.body, "obop_backup_last_run_failed"), 0.0);
    // A verified backup with a copy off the host, a passed drill and a failed run.
    let f = fixture(MetricsAccess::token(TOKEN));
    let b = f.app.store.backend();
    let c = Catalog::new(&*b);
    c.record_backup(&BackupRecord {
        id: "obop-backup-x.obbk".into(),
        path: "/b/x".into(),
        created_at: now - 3600,
        format: 2,
        key_id: Some("abcd1234".into()),
        bytes: 4096,
        records: 10,
        schema_version: 2,
        duration_ms: 2500,
        verified: true,
        authenticated: true,
        reason: "scheduled".into(),
        copies: vec![Copy {
            sink: "fs:/mnt".into(),
            name: "obop-backup-x.obbk".into(),
            verified_at: now - 3590,
        }],
    })
    .unwrap();
    c.record_drill(&DrillRecord {
        id: "drill-1".into(),
        at: now - 86_400,
        plane: "main".into(),
        backup_id: "obop-backup-x.obbk".into(),
        rpo_seconds: 5,
        rto_seconds: 7,
        records: 10,
        checks: Vec::new(),
        passed: true,
    })
    .unwrap();
    b.kv_put(
        obop_ops::schedule::FAILURE_KEY,
        "1: a sink was unreachable",
        None,
    )
    .unwrap();
    let r = send(&f.router, ok_metrics()).await;
    assert_eq!(r.status, 200, "{}", r.body);
    let age = value(&r.body, "obop_backup_age_seconds");
    assert!((3600.0..3700.0).contains(&age), "{age}");
    assert_eq!(value(&r.body, "obop_backup_size_bytes"), 4096.0);
    assert_eq!(value(&r.body, "obop_backup_duration_seconds"), 3.0);
    assert!((3600.0..3700.0).contains(&value(&r.body, "obop_backup_copy_age_seconds")));
    assert!((86_400.0..86_500.0).contains(&value(&r.body, "obop_restore_drill_age_seconds")));
    assert_eq!(value(&r.body, "obop_backup_last_run_failed"), 1.0);
    // Nothing in the series names a file, a key or a path.
    assert!(
        !r.body.contains("obop-backup-x")
            && !r.body.contains("abcd1234")
            && !r.body.contains("/mnt")
    );
}

#[tokio::test]
async fn metrics_report_the_wal_archiver_only_when_the_database_is_a_postgres_server() {
    use obop_ops::pgadmin::{ArchiverStatus, FakeServer};
    // No server information (SQLite, memory): no WAL series.
    let f = fixture(MetricsAccess::token(TOKEN));
    let r = send(&f.router, ok_metrics()).await;
    assert!(!r.body.contains("obop_wal_archive"), "{}", r.body);
    // A server that archives: the lag and the failures.
    let f = fixture(MetricsAccess::token(TOKEN));
    let mut srv = FakeServer::well_configured();
    srv.archiver = ArchiverStatus {
        archived_count: 40,
        failed_count: 3,
        last_archived_wal: Some("000000010000000000000009".into()),
        last_archived_age: Some(42),
        last_failed_age: Some(9000),
    };
    f.state.set_server_info(Arc::new(srv));
    let r = send(&f.router, ok_metrics()).await;
    assert_eq!(value(&r.body, "obop_wal_archive_lag_seconds"), 42.0);
    assert_eq!(value(&r.body, "obop_wal_archive_failed_total"), 3.0);
    assert!(
        !r.body.contains("000000010000000000000009"),
        "no segment names"
    );
    // A server that never archived: the failure counter, and no lag series to alert on `absent()`.
    let f = fixture(MetricsAccess::token(TOKEN));
    f.state.set_server_info(Arc::new(FakeServer::default()));
    let r = send(&f.router, ok_metrics()).await;
    assert!(!r.body.contains("obop_wal_archive_lag_seconds"));
    assert_eq!(value(&r.body, "obop_wal_archive_failed_total"), 0.0);
}
