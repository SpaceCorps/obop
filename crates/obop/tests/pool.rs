//! The connection pool seen from outside (D74): a real `obop serve` process on a scratch
//! PostgreSQL database, a small pool, and a table lock that makes every statement wait. Skipped
//! unless `OBOP_TEST_POSTGRES_URL` names a server (docs/DEPLOY.md).
//!
//! What it proves end to end: when the pool is exhausted the answer is 503 `storage_busy` with
//! `Retry-After` within the acquire timeout and never a 401 or 400 that a client would believe;
//! the queue is bounded; a statement over its limit is `storage_timeout`; `/metrics` shows the
//! pool; `/healthz` stays up; and when the lock is gone the server serves again, without a
//! restart.

use serde_json::{Value as J, json};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const KEY: &str = "5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e";
const PASSWORD: &str = "correct horse battery staple";
const METRICS_TOKEN: &str = "pool-test-metrics-token-0123456789";

fn pg_base() -> Option<String> {
    let u = std::env::var("OBOP_TEST_POSTGRES_URL").ok();
    if u.is_none() {
        eprintln!("skipped: set OBOP_TEST_POSTGRES_URL to run the pool tests");
    }
    u
}

/// A scratch database, dropped afterwards.
struct Scratch {
    admin: String,
    name: String,
    url: String,
}

impl Scratch {
    fn new(base: &str) -> Scratch {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let name = format!(
            "obop_pool_http_{}_{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let mut c = postgres::Client::connect(base, postgres::NoTls).expect("connect to postgres");
        c.batch_execute(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
            .unwrap();
        c.batch_execute(&format!("CREATE DATABASE {name}")).unwrap();
        let (head, _) = base.rsplit_once('/').unwrap();
        Scratch {
            admin: base.into(),
            url: format!("{head}/{name}"),
            name,
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if let Ok(mut c) = postgres::Client::connect(&self.admin, postgres::NoTls) {
            let _ = c.batch_execute(&format!(
                "DROP DATABASE IF EXISTS {} WITH (FORCE)",
                self.name
            ));
        }
    }
}

struct Server {
    child: Child,
    port: u16,
    log: std::path::PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.log);
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn obop() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_obop"));
    c.env("OBOP_SECRET_KEY", KEY)
        .env("OBOP_REQUIRE_MFA_FOR_ADMINS", "0")
        .env_remove("OBOP_DATABASE_URL")
        .env_remove("OBOP_DEV_INSECURE_KEY")
        .env_remove("OBOP_SECRET_KEY_PREVIOUS");
    c
}

fn start(db: &str, env: &[(&str, &str)]) -> Server {
    let port = free_port();
    let log = std::env::temp_dir().join(format!("obop-pool-{}-{port}.log", std::process::id()));
    let child = obop()
        .arg("serve")
        .args(["--addr", &format!("127.0.0.1:{port}"), "--db", db])
        .envs(env.iter().copied())
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(&log).unwrap())
        .spawn()
        .expect("spawn obop serve");
    let s = Server { child, port, log };
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return s;
        }
        assert!(
            Instant::now() < deadline,
            "obop serve did not come up: {}",
            std::fs::read_to_string(&s.log).unwrap_or_default()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn create_admin(db: &str, name: &str) {
    let mut c = obop()
        .args(["admin", "create", name, "--all-groups", "--db", db])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    c.stdin
        .take()
        .unwrap()
        .write_all(format!("{PASSWORD}\n").as_bytes())
        .unwrap();
    assert!(c.wait().unwrap().success(), "admin create");
}

struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
    took: Duration,
}

impl Reply {
    fn json(&self) -> J {
        serde_json::from_str(&self.body).unwrap_or(J::Null)
    }
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// A plain HTTP/1.1 request on a fresh connection (the server closes it after the answer).
fn http(port: u16, method: &str, path: &str, bearer: Option<&str>, body: Option<&J>) -> Reply {
    let started = Instant::now();
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
    let payload = body.map(J::to_string).unwrap_or_default();
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nAccept: application/json\r\n"
    );
    if let Some(t) = bearer {
        req.push_str(&format!("Authorization: Bearer {t}\r\n"));
    }
    if body.is_some() {
        req.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            payload.len()
        ));
    } else if method == "POST" {
        req.push_str("Content-Length: 0\r\n");
    }
    req.push_str("\r\n");
    req.push_str(&payload);
    s.write_all(req.as_bytes()).unwrap();
    let mut raw = Vec::new();
    let _ = s.read_to_end(&mut raw);
    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect::<Vec<_>>();
    let body = if headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("transfer-encoding") && v.contains("chunked"))
    {
        let mut out = String::new();
        let mut rest = body;
        while let Some((size, tail)) = rest.split_once("\r\n") {
            let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
            if n == 0 || tail.len() < n {
                break;
            }
            out.push_str(&tail[..n]);
            rest = tail[n..].trim_start_matches("\r\n");
        }
        out
    } else {
        body.to_string()
    };
    Reply {
        status,
        headers,
        body,
        took: started.elapsed(),
    }
}

/// The resident memory of a process in KiB: `/proc` on Linux, `tasklist` on Windows, `ps` elsewhere.
fn rss_kib(pid: u32) -> Option<u64> {
    if cfg!(target_os = "linux") {
        let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
        let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
        return line.split_whitespace().nth(1)?.parse().ok();
    }
    if cfg!(windows) {
        let out = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        let last = text.trim().rsplit("\",\"").next()?.to_string();
        let digits: String = last.chars().filter(char::is_ascii_digit).collect();
        return digits.parse().ok();
    }
    let out = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

fn metrics(port: u16) -> String {
    let r = http(port, "GET", "/metrics", Some(METRICS_TOKEN), None);
    assert_eq!(r.status, 200, "metrics: {}", r.body);
    r.body
}

/// `n` signed-in requests at once (each on a thread of its own); the answers, the server's resident
/// memory before them and at its peak while they ran.
fn stampede(port: u16, token: &str, pid: u32, n: usize) -> (Vec<Reply>, Option<u64>, Option<u64>) {
    let before = rss_kib(pid);
    let sampling = std::sync::atomic::AtomicBool::new(true);
    /// Stops the sampler when the scope is left, also by a panic of a request thread (otherwise
    /// `thread::scope` would wait for the sampler for ever and the test would hang, not fail).
    struct Stop<'a>(&'a std::sync::atomic::AtomicBool);
    impl Drop for Stop<'_> {
        fn drop(&mut self) {
            self.0.store(false, std::sync::atomic::Ordering::Relaxed);
        }
    }
    let (replies, peak) = std::thread::scope(|s| {
        let _stop = Stop(&sampling);
        let sampler = s.spawn(|| {
            let mut peak = rss_kib(pid);
            while sampling.load(std::sync::atomic::Ordering::Relaxed) {
                peak = peak.max(rss_kib(pid));
                std::thread::sleep(Duration::from_millis(100));
            }
            peak
        });
        let hs: Vec<_> = (0..n)
            .map(|_| s.spawn(|| http(port, "GET", "/api/meta", Some(token), None)))
            .collect();
        let replies: Vec<Reply> = hs.into_iter().map(|h| h.join().unwrap()).collect();
        sampling.store(false, std::sync::atomic::Ordering::Relaxed);
        (replies, sampler.join().unwrap())
    });
    (replies, before, peak)
}

/// The value of one series line, e.g. `obop_db_pool_in_use{pool="main"}`.
fn series(text: &str, name: &str) -> f64 {
    text.lines()
        .find_map(|l| l.strip_prefix(name).and_then(|r| r.trim().parse().ok()))
        .unwrap_or_else(|| panic!("no series {name} in:\n{text}"))
}

#[test]
fn an_exhausted_pool_answers_503_with_retry_after_never_a_401_and_the_server_recovers() {
    let Some(base) = pg_base() else { return };
    let db = Scratch::new(&base);
    create_admin(&db.url, "ada");
    let server = start(
        &db.url,
        &[
            ("OBOP_DB_POOL_SIZE", "2"),
            ("OBOP_DB_ACQUIRE_TIMEOUT_MS", "500"),
            ("OBOP_DB_MAX_WAITERS", "64"),
            ("OBOP_DB_STATEMENT_TIMEOUT_POINT_MS", "2500"),
            ("OBOP_DB_STATEMENT_TIMEOUT_LIST_MS", "2500"),
            ("OBOP_METRICS_TOKEN", METRICS_TOKEN),
        ],
    );
    let port = server.port;
    let login = http(
        port,
        "POST",
        "/api/auth/login",
        None,
        Some(&json!({"username": "ada", "password": PASSWORD})),
    );
    assert_eq!(login.status, 200, "{}", login.body);
    let token = login.json()["token"].as_str().unwrap().to_string();
    assert_eq!(
        http(port, "GET", "/api/meta", Some(&token), None).status,
        200
    );

    // The pool and its series exist before any trouble.
    let m = metrics(port);
    assert_eq!(
        series(&m, "obop_db_pool_max_connections{pool=\"main\"}"),
        2.0
    );
    assert_eq!(series(&m, "obop_db_pool_waiting{pool=\"main\"}"), 0.0);
    assert!(m.contains("obop_db_pool_acquire_seconds_bucket{pool=\"main\",le=\"+Inf\"}"));
    assert!(m.contains("obop_storage_faults_total{fault=\"busy\",answer=\"503\"} 0"));

    // Make every statement wait: another session locks the tables the server works on.
    let mut locker = postgres::Client::connect(&db.url, postgres::NoTls).unwrap();
    locker
        .batch_execute(
            "BEGIN; LOCK TABLE obop_record, obop_kv, obop_counter IN ACCESS EXCLUSIVE MODE",
        )
        .unwrap();

    // 200 valid, signed-in requests at once on a pool of two, while the server's memory is read.
    let pid = server.child.id();
    let started = Instant::now();
    let (replies, rss_before, rss_peak) = stampede(port, &token, pid, 200);
    let all = started.elapsed();
    if let (Some(before), Some(peak)) = (rss_before, rss_peak) {
        eprintln!(
            "D74 HTTP memory: resident {} MiB before, {} MiB at the peak of the 200-request stampede",
            before / 1024,
            peak / 1024
        );
        assert!(
            peak < before + 300 * 1024,
            "memory grew by {} MiB",
            (peak - before) / 1024
        );
    }
    let busy: Vec<_> = replies
        .iter()
        .filter(|r| r.json()["error"] == "storage_busy")
        .collect();
    let timed_out = replies
        .iter()
        .filter(|r| r.json()["error"] == "storage_timeout")
        .count();
    let slowest_busy = busy.iter().map(|r| r.took).max().unwrap();
    eprintln!(
        "D74 HTTP: 200 requests, pool 2, {} storage_busy (slowest {slowest_busy:?}), \
         {timed_out} storage_timeout, all answered in {all:?}",
        busy.len()
    );
    for r in &replies {
        assert_eq!(r.status, 503, "{} {}", r.status, r.body);
    }
    assert!(
        replies.iter().all(|r| r.json()["error"] != "unauthorized"),
        "a signed-in request is never told it is not signed in"
    );
    assert!(busy.len() >= 190, "{} were storage_busy", busy.len());
    for r in &busy {
        assert_eq!(r.header("retry-after"), Some("1"), "{:?}", r.headers);
        assert!(
            r.json()["hint"]
                .as_str()
                .is_some_and(|h| h.contains("Retry-After"))
        );
    }
    assert!(
        slowest_busy < Duration::from_millis(3000),
        "refused within about the acquire timeout, not queued: {slowest_busy:?}"
    );
    // The two requests that held the connections ran into their statement limit.
    assert!(timed_out >= 1, "{timed_out}");
    for r in replies
        .iter()
        .filter(|r| r.json()["error"] == "storage_timeout")
    {
        assert_eq!(r.header("retry-after"), Some("5"));
    }

    // While it was stuck, liveness stayed up and the metrics told the story.
    assert_eq!(http(port, "GET", "/healthz", None, None).status, 200);
    let m = metrics(port);
    assert!(
        series(
            &m,
            "obop_storage_faults_total{fault=\"busy\",answer=\"503\"}"
        ) >= 190.0,
        "{m}"
    );
    assert!(
        series(&m, "obop_db_pool_acquire_timeouts_total{pool=\"main\"}")
            + series(&m, "obop_db_pool_acquire_rejected_total{pool=\"main\"}")
            >= 190.0
    );
    assert!(series(&m, "obop_db_statement_timeouts_total") >= 1.0);
    assert_eq!(
        series(&m, "obop_db_pool_waiting{pool=\"main\"}"),
        0.0,
        "{m}"
    );

    // `/readyz` stays ready while every connection is busy: the database is there, it is only
    // full of work. (Two requests hold both connections until their time limit.)
    let mut watcher = postgres::Client::connect(&db.url, postgres::NoTls).unwrap();
    let ready = std::thread::scope(|s| {
        for _ in 0..2 {
            s.spawn(|| http(port, "GET", "/api/meta", Some(&token), None));
        }
        // Ask when both connections are held, not after a guessed time: a slow machine starts the
        // two requests late, a fast one may have them finished. The database says it exactly: both
        // `obop-main` sessions wait for the table lock. (The server's own `/metrics` is no probe
        // here: its `obop_db_up` ping takes a connection of the full pool too and waits as long.)
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let held: i64 = watcher
                .query_one(
                    "SELECT count(*) FROM pg_stat_activity WHERE datname = current_database() \
                     AND application_name = 'obop-main' AND wait_event_type = 'Lock'",
                    &[],
                )
                .unwrap()
                .get(0);
            if held >= 2 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the two requests never held both connections ({held} waiting for the lock)"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        http(port, "GET", "/readyz", None, None)
    });
    assert_eq!(
        (ready.status, ready.json()["status"].as_str()),
        (200, Some("ready")),
        "{}",
        ready.body
    );
    assert!(ready.took < Duration::from_secs(3), "{:?}", ready.took);

    // Four times as many requests: the wait queue is bounded, so memory does not follow the load.
    let (more, _, rss_peak_more) = stampede(port, &token, pid, 800);
    for r in &more {
        assert_eq!(r.status, 503, "{} {}", r.status, r.body);
        assert!(
            ["storage_busy", "storage_timeout"].contains(&r.json()["error"].as_str().unwrap_or("")),
            "{}",
            r.body
        );
    }
    let busy_more = more
        .iter()
        .filter(|r| r.json()["error"] == "storage_busy")
        .count();
    assert!(busy_more >= 780, "{busy_more} were storage_busy");
    if let (Some(first), Some(second)) = (rss_peak, rss_peak_more) {
        eprintln!(
            "D74 HTTP memory: peak {} MiB with 200 requests, {} MiB with 800",
            first / 1024,
            second / 1024
        );
        assert!(
            second < first + 100 * 1024,
            "memory grew by {} MiB for four times the requests",
            second.saturating_sub(first) / 1024
        );
    }

    // Let go of the lock: the server serves again, without a restart, within a few seconds.
    locker.batch_execute("ROLLBACK").unwrap();
    let released = Instant::now();
    let deadline = Instant::now() + Duration::from_secs(30);
    let served = loop {
        let r = http(port, "GET", "/api/meta", Some(&token), None);
        if r.status == 200 {
            break released.elapsed();
        }
        assert!(
            Instant::now() < deadline,
            "no recovery after the lock was released: {} {}",
            r.status,
            r.body
        );
        std::thread::sleep(Duration::from_millis(200));
    };
    eprintln!("D74 HTTP: served again {served:?} after the lock was released");
    let made = http(
        port,
        "POST",
        "/api/party/create",
        Some(&token),
        Some(&json!({"name": "Acme Ltd"})),
    );
    assert_eq!(made.status, 200, "{}", made.body);
    let m = metrics(port);
    assert_eq!(series(&m, "obop_db_pool_in_use{pool=\"main\"}"), 0.0, "{m}");
    let log = std::fs::read_to_string(&server.log).unwrap_or_default();
    assert!(
        !log.contains("panicked"),
        "the server did not panic:\n{log}"
    );
}

#[test]
fn a_create_is_never_answered_503_after_it_committed_and_never_stored_after_a_503() {
    let Some(base) = pg_base() else { return };
    let db = Scratch::new(&base);
    create_admin(&db.url, "ada");
    // One connection and no queue: every request that meets another is refused at once, which
    // includes the read-back that follows a create's commit.
    let server = start(
        &db.url,
        &[
            ("OBOP_DB_POOL_SIZE", "1"),
            ("OBOP_DB_MAX_WAITERS", "0"),
            ("OBOP_DB_ACQUIRE_TIMEOUT_MS", "50"),
            ("OBOP_METRICS_TOKEN", METRICS_TOKEN),
        ],
    );
    let port = server.port;
    let login = http(
        port,
        "POST",
        "/api/auth/login",
        None,
        Some(&json!({"username": "ada", "password": PASSWORD})),
    );
    assert_eq!(login.status, 200, "{}", login.body);
    let token = login.json()["token"].as_str().unwrap().to_string();
    let stop = std::sync::atomic::AtomicBool::new(false);
    let results = std::sync::Mutex::new(Vec::<(String, u16, String)>::new());
    std::thread::scope(|s| {
        struct Stop<'a>(&'a std::sync::atomic::AtomicBool);
        impl Drop for Stop<'_> {
            fn drop(&mut self) {
                self.0.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
        let _stop = Stop(&stop);
        // Background reads keep the one connection busy.
        s.spawn(|| {
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                http(port, "GET", "/api/meta", Some(&token), None);
            }
        });
        // A fixed number of creates: what is asserted is the invariant (a 200 is stored, a 503 is
        // not), which holds however the requests fall. That a refusal after a commit keeps the
        // answer is proved where it can be forced, in `ops::tests` of obop-http.
        let creators: Vec<_> = (0..6)
            .map(|t| {
                let (token, results) = (&token, &results);
                s.spawn(move || {
                    for i in 0..60 {
                        let name = format!("probe-{t}-{i}");
                        let r = http(
                            port,
                            "POST",
                            "/api/party/create",
                            Some(token),
                            Some(&json!({"name": name})),
                        );
                        results.lock().unwrap().push((name, r.status, r.body));
                    }
                })
            })
            .collect();
        for c in creators {
            c.join().unwrap();
        }
        // The test needs both an answered and a refused create. The refused ones come with the
        // pressure above; on a loaded machine the answered ones can be missing from the fixed
        // batch, so go on, one at a time, until one is answered.
        let answered = |r: &std::sync::Mutex<Vec<(String, u16, String)>>| {
            r.lock().unwrap().iter().any(|x| x.1 == 200)
        };
        let mut extra = 0;
        while !answered(&results) && extra < 600 {
            let name = format!("probe-extra-{extra}");
            let r = http(
                port,
                "POST",
                "/api/party/create",
                Some(&token),
                Some(&json!({"name": name})),
            );
            results.lock().unwrap().push((name, r.status, r.body));
            extra += 1;
            std::thread::sleep(Duration::from_millis(5));
        }
    });
    // Calm again: read everything that was stored.
    std::thread::sleep(Duration::from_millis(300));
    let mut stored = std::collections::HashSet::new();
    for _ in 0..50 {
        let r = http(
            port,
            "POST",
            "/api/party/list",
            Some(&token),
            Some(&json!({"limit": 5000})),
        );
        if r.status == 200 {
            for row in r.json()["value"].as_array().cloned().unwrap_or_default() {
                if let Some(n) = row["name"].as_str() {
                    stored.insert(n.to_string());
                }
            }
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let results = results.into_inner().unwrap();
    let (mut ok, mut refused, mut partial) = (0, 0, 0);
    for (name, status, body) in &results {
        match status {
            200 => {
                ok += 1;
                assert!(
                    stored.contains(name),
                    "{name} was answered 200 but is not stored"
                );
            }
            503 => {
                // The answer says which: "none" means nothing was changed; "partial" says part of
                // it was applied (a create that stored its record and lost a step it needed).
                let applied = serde_json::from_str::<J>(body)
                    .ok()
                    .and_then(|j| j["applied"].as_str().map(String::from))
                    .unwrap_or_default();
                match applied.as_str() {
                    "none" => {
                        refused += 1;
                        assert!(
                            !stored.contains(name),
                            "{name} was answered 503 applied none but is stored: {body}"
                        );
                    }
                    "partial" => partial += 1,
                    other => panic!("{name}: a 503 with applied {other:?}: {body}"),
                }
            }
            other => panic!("{name}: {other} {body}"),
        }
    }
    eprintln!(
        "D74 creates under a pool of one: {ok} answered 200, {refused} answered 503 none (not stored), \
         {partial} answered 503 partial"
    );
    assert!(
        refused >= 1,
        "the test needs pressure: no create was refused"
    );
    assert!(ok >= 1, "the test needs progress: every create was refused");
    // The series the server counted, for the record: how many refusals came after a commit and kept
    // their answer is not forced here (it depends on who gets the one connection); the unit test
    // `a_request_that_committed_a_change_keeps_its_answer` forces it.
    let m = metrics(port);
    eprintln!(
        "D74 kept series: {}",
        series(
            &m,
            "obop_storage_faults_total{fault=\"busy\",answer=\"kept\"}"
        )
    );
}

#[test]
fn a_command_given_its_own_direct_address_is_not_refused_in_a_transaction_pooler_layout() {
    let Some(base) = pg_base() else { return };
    let db = Scratch::new(&base);
    // The layout of docs/DEPLOY.md: the server's own address (`OBOP_DATABASE_URL`) is the pooler's
    // (nothing listens there in this test: a command that names its own address never uses it) and
    // the locks go to the direct address of the database.
    let pooler = "postgres://u:p@127.0.0.1:1/via_the_pooler";
    let layout = |server: &str| {
        [
            ("OBOP_DB_POOLER", "transaction".to_string()),
            ("OBOP_LOCK_DATABASE_URL", db.url.clone()),
            ("OBOP_DATABASE_URL", server.to_string()),
        ]
    };
    let run = |args: &[&str], server: &str| {
        let out = obop()
            .args(args)
            .envs(layout(server))
            .output()
            .expect("run obop");
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stderr).to_string()
                + &String::from_utf8_lossy(&out.stdout),
        )
    };
    // A maintenance command given the direct address (also the lock address) works.
    let (ok, text) = run(&["migrate", "status", "--db", &db.url], pooler);
    assert!(
        ok,
        "refused although it names its own direct address: {text}"
    );
    // The rule keeps the guard where it belongs: the server's own address may not be the lock
    // address, with or without `--db` naming it, and `serve` keeps the variables for whatever it
    // serves from.
    for args in [
        vec!["migrate", "status"],
        vec!["migrate", "status", "--db", db.url.as_str()],
    ] {
        let (ok, text) = run(&args, &db.url);
        assert!(!ok && text.contains("same address"), "{args:?}: {text}");
    }
    let (ok, text) = run(
        &[
            "serve",
            "--addr",
            &format!("127.0.0.1:{}", free_port()),
            "--db",
            &db.url,
        ],
        pooler,
    );
    assert!(!ok && text.contains("same address"), "serve: {text}");
    // The same command with the global options anywhere: still `serve`, still refused (the rule
    // looks at the parsed command, not at the first word).
    let addr = format!("127.0.0.1:{}", free_port());
    for args in [
        vec!["--db", db.url.as_str(), "serve", "--addr", addr.as_str()],
        vec![
            "--dev-auth",
            "serve",
            "--addr",
            addr.as_str(),
            "--db",
            db.url.as_str(),
        ],
        vec![
            "--demo",
            "serve",
            "--db",
            db.url.as_str(),
            "--addr",
            addr.as_str(),
        ],
    ] {
        let (ok, text) = run(&args, pooler);
        assert!(!ok && text.contains("same address"), "{args:?}: {text}");
    }
}

#[test]
fn a_bad_pool_setting_stops_the_server_at_start_up_and_names_the_variable() {
    let Some(base) = pg_base() else { return };
    let db = Scratch::new(&base);
    let out = obop()
        .arg("serve")
        .args([
            "--addr",
            &format!("127.0.0.1:{}", free_port()),
            "--db",
            &db.url,
        ])
        .env("OBOP_DB_POOL_SIZE", "500")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("OBOP_DB_POOL_SIZE"), "{err}");
    assert!(
        !err.contains("obop-test"),
        "the password is not printed: {err}"
    );
}
