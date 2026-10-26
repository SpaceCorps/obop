//! Health and metrics of an instance (D67): `GET /healthz` (the process answers), `GET /readyz`
//! (the database answers and its schema is one this build can serve) and `GET /metrics`
//! (Prometheus text). None of them touches the command layer, a record or a person.
//!
//! `/healthz` and `/readyz` are open and say nothing but a status and a fixed, short reason.
//! `/metrics` is closed unless the operator opened it: with a bearer token (`OBOP_METRICS_TOKEN`),
//! or, with no token, only on a loopback bind and only for a direct loopback caller that sent no
//! proxy header. Every route is registered before the static and public-site fallbacks, so none
//! of them can be shadowed.

use super::HttpState;
use axum::{
    body::Body,
    extract::{ConnectInfo, Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering::Relaxed},
};
use std::time::{Duration, Instant};

/// Upper bounds of the request-duration histogram, in seconds.
const BUCKETS: [f64; 11] = [
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];
/// `/readyz` asks the database at most this often, however often it is asked.
const READY_TTL: Duration = Duration::from_secs(2);
/// A database that does not answer within this long is not ready.
const READY_TIMEOUT: Duration = Duration::from_secs(3);
/// The session count and queue depth read many rows; scrapes share one reading this long.
const GAUGE_TTL: Duration = Duration::from_secs(15);
/// A metrics token shorter than this is refused at start-up.
pub const MIN_TOKEN_LEN: usize = 16;

/// Who may read `/metrics`.
#[derive(Clone)]
pub enum MetricsAccess {
    /// Not served: the route answers 404 as if it did not exist.
    Disabled,
    /// Served to a direct loopback caller without proxy headers (the server is bound to loopback).
    Loopback,
    /// Served to whoever presents this bearer token (kept as a hash).
    Token([u8; 32]),
}

impl MetricsAccess {
    pub fn token(token: &str) -> Self {
        MetricsAccess::Token(Sha256::digest(token.as_bytes()).into())
    }

    /// From `OBOP_METRICS_TOKEN` (if set, at least 16 characters) and whether the server listens
    /// on a loopback address. A token always wins; without one only a loopback bind opens it.
    pub fn from_env(bound_to_loopback: bool) -> Result<Self, String> {
        Self::resolve(
            std::env::var("OBOP_METRICS_TOKEN").ok().as_deref(),
            bound_to_loopback,
        )
    }

    pub fn resolve(token: Option<&str>, bound_to_loopback: bool) -> Result<Self, String> {
        match token.map(str::trim).filter(|t| !t.is_empty()) {
            Some(t) if t.chars().count() < MIN_TOKEN_LEN => Err(format!(
                "OBOP_METRICS_TOKEN must be at least {MIN_TOKEN_LEN} characters (for example `openssl rand -hex 24`)"
            )),
            Some(t) => Ok(Self::token(t)),
            None if bound_to_loopback => Ok(MetricsAccess::Loopback),
            None => Ok(MetricsAccess::Disabled),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ready {
    Yes,
    No(&'static str),
}

#[derive(Clone, Copy, Default)]
struct Gauges {
    sessions: Option<u64>,
    queue: Option<(u64, u64, u64)>,
    backups: Option<BackupGauges>,
    wal: Option<WalGauges>,
}

/// What the catalogue says about backups and drills (D73), read with the other gauges.
#[derive(Clone, Copy, Default)]
struct BackupGauges {
    age: Option<u64>,
    bytes: Option<u64>,
    duration: Option<u64>,
    copy_age: Option<u64>,
    drill_age: Option<u64>,
    last_run_failed: bool,
}

/// The WAL archiver as `pg_stat_archiver` reports it.
#[derive(Clone, Copy, Default)]
struct WalGauges {
    lag: Option<u64>,
    failed: u64,
}

pub(crate) struct Ops {
    started: Instant,
    /// 1xx, 2xx, 3xx, 4xx, 5xx and anything else.
    classes: [AtomicU64; 6],
    buckets: [AtomicU64; BUCKETS.len()],
    count: AtomicU64,
    sum_micros: AtomicU64,
    ready: Mutex<Option<(Instant, Ready)>>,
    gauges: Mutex<Option<(Instant, Gauges)>>,
    access: Mutex<MetricsAccess>,
    /// The PostgreSQL server, when the database is one (the WAL archive series).
    server: std::sync::OnceLock<Arc<dyn obop_ops::pgadmin::ServerInfo>>,
}

impl Ops {
    pub fn new() -> Self {
        Ops {
            started: Instant::now(),
            classes: Default::default(),
            buckets: Default::default(),
            count: AtomicU64::new(0),
            sum_micros: AtomicU64::new(0),
            ready: Mutex::new(None),
            gauges: Mutex::new(None),
            access: Mutex::new(MetricsAccess::Disabled),
            server: std::sync::OnceLock::new(),
        }
    }

    pub fn set_server_info(&self, info: Arc<dyn obop_ops::pgadmin::ServerInfo>) {
        let _ = self.server.set(info);
    }

    pub fn set_access(&self, a: MetricsAccess) {
        *self.access.lock().unwrap_or_else(|e| e.into_inner()) = a;
    }

    fn observe(&self, status: u16, took: Duration) {
        let class = match status {
            100..=199 => 0,
            200..=299 => 1,
            300..=399 => 2,
            400..=499 => 3,
            500..=599 => 4,
            _ => 5,
        };
        self.classes[class].fetch_add(1, Relaxed);
        let secs = took.as_secs_f64();
        if let Some(i) = BUCKETS.iter().position(|b| secs <= *b) {
            self.buckets[i].fetch_add(1, Relaxed);
        }
        self.count.fetch_add(1, Relaxed);
        self.sum_micros.fetch_add(took.as_micros() as u64, Relaxed);
    }
}

/// Counts every response by status class and times it up to the moment the headers are ready
/// (a streamed body is not included). Paths are not labels, so the series cannot multiply.
pub(crate) async fn track(State(s): State<Arc<HttpState>>, req: Request, next: Next) -> Response {
    let t = Instant::now();
    let resp = next.run(req).await;
    s.ops.observe(resp.status().as_u16(), t.elapsed());
    resp
}

fn no_store(mut r: Response) -> Response {
    r.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

fn json(status: StatusCode, body: &'static str) -> Response {
    no_store(
        (
            status,
            [(header::CONTENT_TYPE, "application/json")],
            Body::from(body),
        )
            .into_response(),
    )
}

/// `GET /healthz`: the process is running and serving requests. No database, no state.
pub(crate) async fn healthz() -> Response {
    json(StatusCode::OK, "{\"status\":\"ok\"}")
}

/// What the pieces say, as one verdict with a fixed reason that names no host, version or path.
fn classify(ping_ok: bool, status: Result<obop_migrate::Status, String>) -> Ready {
    if !ping_ok {
        return Ready::No("database unreachable");
    }
    match status {
        Err(_) => Ready::No("database unreachable"),
        Ok(st) if st.ahead => Ready::No("database is newer than this server"),
        // A database with no records at all is at the latest version by construction.
        Ok(st) if st.pending.is_empty() || (st.stored.is_none() && st.empty) => Ready::Yes,
        Ok(_) => Ready::No("database needs migration"),
    }
}

fn check_ready(s: &HttpState) -> Ready {
    let backend = &s.shared.backend;
    let ping_ok = backend.counter_get("health.ping").is_ok();
    let status = obop_migrate::Migrator::new(backend.clone(), s.shared.ring.clone()).status();
    classify(ping_ok, status)
}

/// `GET /readyz`: 200 when the database answers and its schema is compatible, else 503 with a
/// short reason. The answer is shared for two seconds, so asking often costs the database nothing.
pub(crate) async fn readyz(State(s): State<Arc<HttpState>>) -> Response {
    let cached = {
        let g = s.ops.ready.lock().unwrap_or_else(|e| e.into_inner());
        g.filter(|(at, _)| at.elapsed() < READY_TTL).map(|(_, r)| r)
    };
    let ready = match cached {
        Some(r) => r,
        None => {
            let s2 = s.clone();
            let r = match tokio::time::timeout(
                READY_TIMEOUT,
                super::blocking(move || check_ready(&s2)),
            )
            .await
            {
                Ok(r) => r,
                Err(_) => Ready::No("database did not answer in time"),
            };
            *s.ops.ready.lock().unwrap_or_else(|e| e.into_inner()) = Some((Instant::now(), r));
            r
        }
    };
    match ready {
        Ready::Yes => json(StatusCode::OK, "{\"status\":\"ready\"}"),
        Ready::No(reason) => {
            let body = format!("{{\"status\":\"unavailable\",\"reason\":\"{reason}\"}}");
            no_store(
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    [(header::CONTENT_TYPE, "application/json")],
                    body,
                )
                    .into_response(),
            )
        }
    }
}

fn ct_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// A request that crossed a proxy says so; a loopback-only metrics page must not answer it.
fn has_proxy_header(h: &axum::http::HeaderMap) -> bool {
    [
        "x-forwarded-for",
        "x-forwarded-host",
        "x-forwarded-proto",
        "x-real-ip",
        "forwarded",
        "via",
    ]
    .iter()
    .any(|n| h.contains_key(*n))
}

enum Verdict {
    Allowed,
    Hidden,
    Unauthorized,
}

fn authorize(access: &MetricsAccess, req: &Request) -> Verdict {
    match access {
        MetricsAccess::Disabled => Verdict::Hidden,
        MetricsAccess::Loopback => {
            let peer_is_loopback = req
                .extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .is_some_and(|c| c.0.ip().is_loopback());
            if peer_is_loopback && !has_proxy_header(req.headers()) {
                Verdict::Allowed
            } else {
                Verdict::Hidden
            }
        }
        MetricsAccess::Token(want) => {
            let presented = req
                .headers()
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| {
                    v.get(..7)
                        .filter(|p| p.eq_ignore_ascii_case("bearer "))
                        .map(|_| &v[7..])
                })
                .map(|t| -> [u8; 32] { Sha256::digest(t.trim().as_bytes()).into() });
            match presented {
                Some(h) if ct_eq(&h, want) => Verdict::Allowed,
                _ => Verdict::Unauthorized,
            }
        }
    }
}

fn gauges(s: &HttpState) -> Gauges {
    {
        let g = s.ops.gauges.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, v)) = *g
            && at.elapsed() < GAUGE_TTL
        {
            return v;
        }
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let v = Gauges {
        sessions: s.auth.active_session_count(),
        queue: s.connectors.get().map(|rt| {
            let st = rt.queue.stats(now);
            let n = |k: &str| st[k].as_u64().unwrap_or(0);
            (n("waiting"), n("leased"), n("dead"))
        }),
        wal: s
            .ops
            .server
            .get()
            .and_then(|srv| srv.archiver().ok())
            .map(|a| WalGauges {
                lag: a.last_archived_age,
                failed: a.failed_count,
            }),
        backups: obop_ops::schedule::stats(&*s.shared.backend, now)
            .ok()
            .map(|st| BackupGauges {
                age: st.backup_age_seconds,
                bytes: st.backup_bytes,
                duration: st.backup_duration_seconds,
                copy_age: st.copy_age_seconds,
                drill_age: st.drill_age_seconds,
                last_run_failed: st.last_failure.is_some(),
            }),
    };
    *s.ops.gauges.lock().unwrap_or_else(|e| e.into_inner()) = Some((Instant::now(), v));
    v
}

fn render(s: &HttpState) -> String {
    use std::fmt::Write;
    let o = &s.ops;
    let mut t = String::new();
    let _ = writeln!(
        t,
        "# HELP obop_build_info The version of this build (always 1)."
    );
    let _ = writeln!(t, "# TYPE obop_build_info gauge");
    let _ = writeln!(
        t,
        "obop_build_info{{core_version=\"{}\"}} 1",
        obop_types::CORE_VERSION
    );
    let _ = writeln!(
        t,
        "# HELP obop_process_uptime_seconds Seconds since the server started."
    );
    let _ = writeln!(t, "# TYPE obop_process_uptime_seconds gauge");
    let _ = writeln!(
        t,
        "obop_process_uptime_seconds {:.3}",
        o.started.elapsed().as_secs_f64()
    );
    let _ = writeln!(
        t,
        "# HELP obop_http_requests_total Responses sent, by status class."
    );
    let _ = writeln!(t, "# TYPE obop_http_requests_total counter");
    for (i, name) in ["1xx", "2xx", "3xx", "4xx", "5xx", "other"]
        .iter()
        .enumerate()
    {
        let _ = writeln!(
            t,
            "obop_http_requests_total{{class=\"{name}\"}} {}",
            o.classes[i].load(Relaxed)
        );
    }
    let _ = writeln!(
        t,
        "# HELP obop_http_request_duration_seconds Time to produce a response (headers), in seconds."
    );
    let _ = writeln!(t, "# TYPE obop_http_request_duration_seconds histogram");
    let mut cumulative = 0;
    for (i, b) in BUCKETS.iter().enumerate() {
        cumulative += o.buckets[i].load(Relaxed);
        let _ = writeln!(
            t,
            "obop_http_request_duration_seconds_bucket{{le=\"{b}\"}} {cumulative}"
        );
    }
    let count = o.count.load(Relaxed);
    let _ = writeln!(
        t,
        "obop_http_request_duration_seconds_bucket{{le=\"+Inf\"}} {count}"
    );
    let _ = writeln!(
        t,
        "obop_http_request_duration_seconds_sum {:.6}",
        o.sum_micros.load(Relaxed) as f64 / 1e6
    );
    let _ = writeln!(t, "obop_http_request_duration_seconds_count {count}");

    let started = Instant::now();
    let up = s.shared.backend.counter_get("health.ping").is_ok();
    let ping = started.elapsed().as_secs_f64();
    let _ = writeln!(t, "# HELP obop_db_up 1 when the database answered a ping.");
    let _ = writeln!(t, "# TYPE obop_db_up gauge");
    let _ = writeln!(t, "obop_db_up {}", u8::from(up));
    let _ = writeln!(
        t,
        "# HELP obop_db_ping_seconds How long the last database ping took."
    );
    let _ = writeln!(t, "# TYPE obop_db_ping_seconds gauge");
    let _ = writeln!(t, "obop_db_ping_seconds {ping:.6}");

    let g = gauges(s);
    if let Some(n) = g.sessions {
        let _ = writeln!(
            t,
            "# HELP obop_active_sessions Sign-in sessions that are valid now (read at most every 15 seconds)."
        );
        let _ = writeln!(t, "# TYPE obop_active_sessions gauge");
        let _ = writeln!(t, "obop_active_sessions {n}");
    }
    if let Some((waiting, leased, dead)) = g.queue {
        let _ = writeln!(
            t,
            "# HELP obop_job_queue_depth Connector deliveries by state (read at most every 15 seconds)."
        );
        let _ = writeln!(t, "# TYPE obop_job_queue_depth gauge");
        for (state, n) in [("waiting", waiting), ("leased", leased), ("dead", dead)] {
            let _ = writeln!(
                t,
                "obop_job_queue_depth{{queue=\"connectors\",state=\"{state}\"}} {n}"
            );
        }
    }
    if let Some(b) = g.backups {
        let mut gauge = |name: &str, help: &str, v: Option<u64>| {
            if let Some(v) = v {
                let _ = writeln!(t, "# HELP {name} {help}");
                let _ = writeln!(t, "# TYPE {name} gauge");
                let _ = writeln!(t, "{name} {v}");
            }
        };
        gauge(
            "obop_backup_age_seconds",
            "Seconds since the newest verified backup (absent when there is none: alert on absent() too).",
            b.age,
        );
        gauge(
            "obop_backup_size_bytes",
            "Size of the newest verified backup file.",
            b.bytes,
        );
        gauge(
            "obop_backup_duration_seconds",
            "How long the newest verified backup took to write and verify.",
            b.duration,
        );
        gauge(
            "obop_backup_copy_age_seconds",
            "Seconds since the newest backup that has a verified copy off this host.",
            b.copy_age,
        );
        gauge(
            "obop_restore_drill_age_seconds",
            "Seconds since the newest restore drill that passed (alert above 35 days).",
            b.drill_age,
        );
        gauge(
            "obop_backup_last_run_failed",
            "1 when the last scheduled backup run reported a failure (a backup or its copy).",
            Some(u64::from(b.last_run_failed)),
        );
    }
    if let Some(w) = g.wal {
        if let Some(lag) = w.lag {
            let _ = writeln!(
                t,
                "# HELP obop_wal_archive_lag_seconds Seconds since PostgreSQL last archived a WAL segment (an idle database archives rarely: alert above 300 together with a rise of the failure counter, or on a database that has activity)."
            );
            let _ = writeln!(t, "# TYPE obop_wal_archive_lag_seconds gauge");
            let _ = writeln!(t, "obop_wal_archive_lag_seconds {lag}");
        }
        let _ = writeln!(
            t,
            "# HELP obop_wal_archive_failed_total Failed WAL archive attempts since the statistics were last reset (pg_stat_archiver)."
        );
        let _ = writeln!(t, "# TYPE obop_wal_archive_failed_total counter");
        let _ = writeln!(t, "obop_wal_archive_failed_total {}", w.failed);
    }
    t
}

/// `GET /metrics`: Prometheus text, for whoever the operator opened it to.
pub(crate) async fn metrics(State(s): State<Arc<HttpState>>, req: Request) -> Response {
    let access = s
        .ops
        .access
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    match authorize(&access, &req) {
        Verdict::Hidden => no_store(StatusCode::NOT_FOUND.into_response()),
        Verdict::Unauthorized => no_store(
            (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, "Bearer realm=\"metrics\"")],
            )
                .into_response(),
        ),
        Verdict::Allowed => {
            let body = super::blocking(move || render(&s)).await;
            no_store(
                (
                    StatusCode::OK,
                    [(
                        header::CONTENT_TYPE,
                        "text/plain; version=0.0.4; charset=utf-8",
                    )],
                    body,
                )
                    .into_response(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use obop_migrate::Status;

    fn status(stored: Option<u32>, latest: u32, empty: bool) -> Status {
        let current = stored.unwrap_or(0);
        Status {
            stored,
            latest,
            empty,
            pending: (current + 1..=latest).map(|_| (0, "step")).collect(),
            ahead: current > latest,
        }
    }

    #[test]
    fn readiness_names_only_fixed_short_reasons() {
        assert_eq!(classify(true, Ok(status(Some(3), 3, false))), Ready::Yes);
        assert_eq!(classify(true, Ok(status(None, 3, true))), Ready::Yes);
        assert_eq!(
            classify(false, Ok(status(Some(3), 3, false))),
            Ready::No("database unreachable")
        );
        assert_eq!(
            classify(true, Err("connection refused to db.internal:5432".into())),
            Ready::No("database unreachable")
        );
        assert_eq!(
            classify(true, Ok(status(Some(9), 3, false))),
            Ready::No("database is newer than this server")
        );
        assert_eq!(
            classify(true, Ok(status(Some(1), 3, false))),
            Ready::No("database needs migration")
        );
        // Records without a version record are an old, unmigrated database, not a fresh one.
        assert_eq!(
            classify(true, Ok(status(None, 3, false))),
            Ready::No("database needs migration")
        );
    }

    #[test]
    fn a_metrics_token_is_checked_for_length_and_wins_over_the_bind() {
        assert!(MetricsAccess::resolve(Some("short"), true).is_err());
        assert!(matches!(
            MetricsAccess::resolve(Some("0123456789abcdef"), false),
            Ok(MetricsAccess::Token(_))
        ));
        assert!(matches!(
            MetricsAccess::resolve(Some("0123456789abcdef"), true),
            Ok(MetricsAccess::Token(_))
        ));
        assert!(matches!(
            MetricsAccess::resolve(None, true),
            Ok(MetricsAccess::Loopback)
        ));
        assert!(matches!(
            MetricsAccess::resolve(Some("  "), false),
            Ok(MetricsAccess::Disabled)
        ));
    }
}
