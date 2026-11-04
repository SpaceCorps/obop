//! The HTTP side of the signer's door (D92): `GET` and `POST` `/sign/{request}/{token}` and
//! `GET` `/sign/{request}/{token}/document.pdf`. The rules and the state are in
//! `obop_commands::sign`; this module carries requests, sets the headers, and checks where a
//! browser post came from.
//!
//! No cookie is set or read, there is no script, and a `GET` changes no request, link or
//! evidence (a mail scanner or a link preview that opens the link is harmless; it counts toward
//! the rate limits and, when the integrity check fails, writes one `sign_refused` entry). Every
//! answer of these handlers that is not the PDF has the headers of the other pages of the server
//! (`harden`) plus `noindex`, and so has the PDF's, which otherwise has its own set
//! (`inline_pdf_response`), because `X-Frame-Options: DENY` would stop the page from showing it.
//! The rejections of the framework before a handler runs (405, 413, a bad encoding in the
//! address) are plain text with neither set. The address comes from `client_ip`, which trusts
//! `X-Forwarded-For` only with `--trust-proxy`; the log entry of a signature also holds the peer
//! and the `X-Forwarded-For` lines the request carried, read lossily, cleaned and cut to their
//! last 200 characters (`forwarded_lines`), which are not the raw header.

use super::oauth::{client_ip, harden, pair, parse_pairs, same_origin, see_other};
use super::sign_page::{self, Page};
use super::sign_text as txt;
use super::{HttpState, blocking};
use axum::{
    body::Bytes,
    extract::{ConnectInfo, Path, State},
    http::{HeaderName, HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use obop_commands::sign::{Client, Done, DoorError, Gone, Invalid, SignForm, View, ViewData};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

type Shared = State<Arc<HttpState>>;

/// Size of the body of the signer's post.
pub(crate) const MAX_BODY: usize = 16 * 1024;

fn page_response(status: StatusCode, p: Page) -> Response {
    let mut r = (status, p.html).into_response();
    let h = r.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    if let Ok(v) = HeaderValue::from_str(&p.csp) {
        h.insert(header::CONTENT_SECURITY_POLICY, v);
    }
    h.insert(
        HeaderName::from_static("x-robots-tag"),
        HeaderValue::from_static("noindex, nofollow"),
    );
    harden(&mut r);
    r
}

/// The headers that keep an answer out of a search index, for the answers that are not pages.
fn noindex(mut r: Response) -> Response {
    r.headers_mut().insert(
        HeaderName::from_static("x-robots-tag"),
        HeaderValue::from_static("noindex, nofollow"),
    );
    r
}

fn lang_for(s: &HttpState, parts: &Parts, form_lang: Option<&str>) -> &'static str {
    let param = form_lang.map(str::to_string).or_else(|| {
        parts
            .uri
            .query()
            .and_then(parse_pairs)
            .and_then(|q| pair(&q, "lang").map(str::to_string))
    });
    txt::choose_language(
        param.as_deref(),
        parts
            .headers
            .get(header::ACCEPT_LANGUAGE)
            .and_then(|v| v.to_str().ok()),
        s.default_language(),
    )
}

/// The seconds between two lines of the log about a refused origin.
const ORIGIN_LOG_EVERY: u64 = 60;

/// The value of `OriginLog::last` before the first line: no time is "never", the clock may read 0.
const NEVER: u64 = u64::MAX;

/// What the log of a refused origin reads and writes: the seconds since the server started (a
/// monotonic clock, so that a step of the wall clock neither silences the line nor floods it) and
/// the place a line goes (standard error). A test replaces both.
struct OriginHooks {
    clock: Box<dyn Fn() -> u64 + Send + Sync>,
    sink: Box<dyn Fn(&str) + Send + Sync>,
}

/// The line of the log about a refused origin, written at most once a minute for the whole
/// process: anybody can cause it without a link, so it must not grow with the number of posts.
/// One per server (`HttpState`), not a global, so that a test can capture the lines it caused.
pub(crate) struct OriginLog {
    /// When the process started, for the default clock.
    started: std::time::Instant,
    /// When the line was last written, on the clock (`NEVER` before the first).
    last: AtomicU64,
    /// Set by a test before the first post.
    hooks: std::sync::OnceLock<OriginHooks>,
}

impl Default for OriginLog {
    fn default() -> Self {
        Self::since(std::time::Instant::now())
    }
}

impl OriginLog {
    /// A log whose default clock counts from `started`.
    fn since(started: std::time::Instant) -> Self {
        Self {
            started,
            last: AtomicU64::new(NEVER),
            hooks: std::sync::OnceLock::new(),
        }
    }

    /// Replace the clock (seconds, ought to be monotonic) and the sink of the lines; false when
    /// the log was already set. For tests: the handler is then run for real and the lines it
    /// wrote and the second each was written at are read back.
    pub(crate) fn set_hooks(
        &self,
        clock: impl Fn() -> u64 + Send + Sync + 'static,
        sink: impl Fn(&str) + Send + Sync + 'static,
    ) -> bool {
        self.hooks
            .set(OriginHooks {
                clock: Box::new(clock),
                sink: Box::new(sink),
            })
            .is_ok()
    }

    /// Write the line when one is due.
    pub(crate) fn note(&self, base: &str) {
        let now = match self.hooks.get() {
            Some(h) => (h.clock)(),
            None => seconds_between(self.started, std::time::Instant::now()),
        };
        if log_due(&self.last, now, ORIGIN_LOG_EVERY) {
            let line = format!("sign: a post was refused because its origin is not {base}");
            match self.hooks.get() {
                Some(h) => (h.sink)(&line),
                None => eprintln!("{line}"),
            }
        }
    }
}

/// The default clock: whole seconds from `started` to `now`.
fn seconds_between(started: std::time::Instant, now: std::time::Instant) -> u64 {
    now.saturating_duration_since(started).as_secs()
}

/// Whether a line of the log is due: at most one in every `every` seconds, whoever asks and
/// however many ask at once, except that a reading below the one at the last line starts the
/// minute again, so that a diagnostic is never held back for as long as the clock was ahead. A
/// stale reading at a boundary (read, passed by another caller, then used) is such a reading and
/// can give one more diagnostic line than the minute allows. The first line is always due,
/// whatever the clock reads (`NEVER` is above every reading).
fn log_due(last: &AtomicU64, now: u64, every: u64) -> bool {
    let before = last.load(Ordering::Relaxed);
    (now < before || now >= before.saturating_add(every))
        && last
            .compare_exchange(before, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
}

/// Every `X-Forwarded-For` line for the log, joined with ` | `. A line that is not text is read
/// lossily, not dropped, so that the log shows it was there; a `|` the client wrote is replaced,
/// so it cannot imitate the border between two lines; and when the text is longer than `n`
/// characters the head is cut, because the proxy's own entry is at the end.
fn forwarded_lines(parts: &Parts, n: usize) -> String {
    let all = parts
        .headers
        .get_all("x-forwarded-for")
        .iter()
        .map(|v| String::from_utf8_lossy(v.as_bytes()).replace('|', "/"))
        .collect::<Vec<_>>()
        .join(" | ");
    let skip = all.chars().count().saturating_sub(n);
    all.chars().skip(skip).collect()
}

fn client(s: &HttpState, parts: &Parts) -> Client {
    // The browser's string is read lossily, like the forwarded lines: a byte that is not text
    // (or an accented letter in a Latin-1 string) is recorded as U+FFFD, not as an empty browser.
    let cut = |v: Option<&HeaderValue>, n: usize| {
        v.map(|v| {
            String::from_utf8_lossy(v.as_bytes())
                .chars()
                .take(n)
                .collect()
        })
        .unwrap_or_default()
    };
    Client {
        addr: client_ip(s, parts),
        peer: parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip().to_string())
            .unwrap_or_default(),
        forwarded: forwarded_lines(parts, 200),
        user_agent: cut(parts.headers.get(header::USER_AGENT), 400),
    }
}

/// The status and the catalogue key (`sign_text.rs`) of the page for a refusal of the door.
fn refusal_parts(e: &DoorError) -> (StatusCode, &'static str) {
    match e {
        DoorError::NotFound => (StatusCode::NOT_FOUND, "unknown"),
        DoorError::Gone(Gone::Expired) => (StatusCode::GONE, "gone_expired"),
        DoorError::Gone(Gone::Revoked) => (StatusCode::GONE, "gone_revoked"),
        // The link is fine but the request is over: declined, expired, or signed another way.
        DoorError::Gone(Gone::Closed) => (StatusCode::GONE, "closed"),
        DoorError::Changed => (StatusCode::CONFLICT, "changed"),
        DoorError::Busy(_) => (StatusCode::TOO_MANY_REQUESTS, "busy"),
        DoorError::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
        // The form is shown again by the caller; this is only the fallback.
        DoorError::Invalid(_) => (StatusCode::BAD_REQUEST, "err_name"),
        DoorError::Refused => (StatusCode::CONFLICT, "gone_revoked"),
    }
}

/// The answer for everything the door refuses.
fn refusal(e: &DoorError, lang: &str) -> Response {
    let (status, key) = refusal_parts(e);
    let mut r = page_response(status, sign_page::message(lang, key));
    let wait = match e {
        DoorError::Busy(n) => Some(*n),
        DoorError::Unavailable => Some(30),
        _ => None,
    };
    if let Some(w) = wait
        && let Ok(v) = HeaderValue::from_str(&w.to_string())
    {
        r.headers_mut().insert(header::RETRY_AFTER, v);
    }
    r
}

struct FormView<'a> {
    lang: &'a str,
    rid: &'a str,
    tok: &'a str,
    data: &'a ViewData,
    typed: &'a str,
    error: Option<&'a str>,
    status: StatusCode,
}

fn form_page(v: &FormView<'_>) -> Response {
    page_response(
        v.status,
        sign_page::form(
            v.lang,
            &sign_page::Form {
                rid: v.rid,
                token: v.tok,
                title: &v.data.title,
                message: &v.data.message,
                signer_name: &v.data.signer_name,
                document_title: &v.data.document_title,
                document_hash: &v.data.document_hash,
                typed: v.typed,
                error: v.error,
            },
        ),
    )
}

/// `GET /sign/{request}/{token}`.
pub(crate) async fn page(
    State(s): Shared,
    Path((rid, tok)): Path<(String, String)>,
    parts: Parts,
) -> Response {
    blocking(move || {
        let lang = lang_for(&s, &parts, None);
        // The door follows the installed package: a rollback or an update made by another
        // process (`obop modules rollback sign`) is seen here without an authenticated call.
        s.dispatcher.sync_installed_modules();
        let Some(svc) = s.dispatcher.sign_service() else {
            return refusal(&DoorError::NotFound, lang);
        };
        match svc.view(&rid, &tok, &client(&s, &parts)) {
            Ok(View::Form(d)) => form_page(&FormView {
                lang,
                rid: &rid,
                tok: &tok,
                data: &d,
                typed: "",
                error: None,
                status: StatusCode::OK,
            }),
            Ok(View::Signed { at }) => page_response(StatusCode::OK, sign_page::done(lang, &at)),
            Err(e) => refusal(&e, lang),
        }
    })
    .await
}

/// `POST /sign/{request}/{token}`.
pub(crate) async fn submit(
    State(s): Shared,
    Path((rid, tok)): Path<(String, String)>,
    parts: Parts,
    body: Bytes,
) -> Response {
    blocking(move || {
        let mut lang = lang_for(&s, &parts, None);
        // The door follows the installed package: a rollback or an update made by another
        // process (`obop modules rollback sign`) is seen here without an authenticated call.
        s.dispatcher.sync_installed_modules();
        let Some(svc) = s.dispatcher.sign_service() else {
            return refusal(&DoorError::NotFound, lang);
        };
        let base = s.mcp.base_url(&parts.headers);
        if !same_origin(&parts.headers, &base) {
            // Where a post came from is the browser's word; a proxy that hides the scheme makes
            // every post look foreign (docs/SIGN.md: set OBOP_PUBLIC_URL). The log says so, once
            // a minute: anybody can cause this without a link, and the refusal is made before
            // any limit of the door.
            s.sign_origin_log.note(&base);
            return page_response(StatusCode::FORBIDDEN, sign_page::message(lang, "forbidden"));
        }
        let ct = parts
            .headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        let pairs = if ct.starts_with("application/x-www-form-urlencoded") {
            std::str::from_utf8(&body).ok().and_then(parse_pairs)
        } else {
            None
        };
        let Some(pairs) = pairs else {
            return page_response(
                StatusCode::BAD_REQUEST,
                sign_page::message(lang, "err_name"),
            );
        };
        lang = lang_for(&s, &parts, pair(&pairs, "lang"));
        let name = pair(&pairs, "name").unwrap_or("").to_string();
        let form = SignForm {
            name: name.clone(),
            // The box sends exactly 1; 0, false, no and the empty text are not consent.
            agree: pair(&pairs, "agree") == Some("1"),
            doc: pair(&pairs, "doc").unwrap_or("").to_string(),
            consent_version: txt::consent_version_for(lang),
        };
        let c = client(&s, &parts);
        match svc.submit(&rid, &tok, &form, &c) {
            // A relative address, so that the redirect does not depend on the derived base. It
            // carries the language this post was resolved to (one of the seven, so nothing to
            // escape), so that a signer who read the English page by `?lang=en` is told so in
            // English whatever their browser asks for; the bare link, visited later, is in the
            // browser's language again.
            Ok(Done { .. }) => noindex(see_other(&format!("/sign/{rid}/{tok}?lang={lang}"))),
            Err(DoorError::Invalid(why)) => {
                // The form again, with what was typed and what is wrong.
                match svc.view(&rid, &tok, &c) {
                    Ok(View::Form(d)) => form_page(&FormView {
                        lang,
                        rid: &rid,
                        tok: &tok,
                        data: &d,
                        typed: &name,
                        error: Some(match why {
                            Invalid::Name => "err_name",
                            Invalid::Agree => "err_agree",
                        }),
                        status: StatusCode::BAD_REQUEST,
                    }),
                    Ok(View::Signed { at }) => {
                        page_response(StatusCode::OK, sign_page::done(lang, &at))
                    }
                    Err(e) => refusal(&e, lang),
                }
            }
            Err(e) => refusal(&e, lang),
        }
    })
    .await
}

/// `GET /sign/{request}/{token}/document.pdf`: the held copy of this request, only while its
/// link works.
pub(crate) async fn document(
    State(s): Shared,
    Path((rid, tok)): Path<(String, String)>,
    parts: Parts,
) -> Response {
    blocking(move || {
        let lang = lang_for(&s, &parts, None);
        // The door follows the installed package: a rollback or an update made by another
        // process (`obop modules rollback sign`) is seen here without an authenticated call.
        s.dispatcher.sync_installed_modules();
        let Some(svc) = s.dispatcher.sign_service() else {
            return refusal(&DoorError::NotFound, lang);
        };
        match svc.pdf(&rid, &tok, &client(&s, &parts)) {
            Ok(served) => noindex(super::blobs::inline_pdf_response(
                served,
                &parts.headers,
                "private, no-store",
            )),
            Err(e) => refusal(&e, lang),
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use obop_oauth::page::LANGS;

    #[test]
    fn a_closed_request_has_its_own_page_apart_from_a_dead_or_an_expired_link() {
        assert_eq!(
            refusal_parts(&DoorError::Gone(Gone::Closed)),
            (StatusCode::GONE, "closed")
        );
        assert_eq!(
            refusal_parts(&DoorError::Gone(Gone::Revoked)),
            (StatusCode::GONE, "gone_revoked")
        );
        assert_eq!(
            refusal_parts(&DoorError::Gone(Gone::Expired)),
            (StatusCode::GONE, "gone_expired")
        );
        assert_eq!(
            refusal_parts(&DoorError::Refused),
            (StatusCode::CONFLICT, "gone_revoked")
        );
    }

    /// What a sink kept: each line with the second of the clock it was written at.
    type Lines = Arc<std::sync::Mutex<Vec<(u64, String)>>>;

    /// A log whose clock is set by hand and whose sink keeps what it was given.
    fn hand_log() -> (OriginLog, Arc<AtomicU64>, Lines) {
        let now = Arc::new(AtomicU64::new(0));
        let lines = Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = OriginLog::default();
        let (c, n, l) = (now.clone(), now.clone(), lines.clone());
        assert!(log.set_hooks(
            move || c.load(Ordering::Relaxed),
            move |line| {
                l.lock()
                    .unwrap()
                    .push((n.load(Ordering::Relaxed), line.to_string()))
            }
        ));
        (log, now, lines)
    }

    #[test]
    fn the_line_about_a_refused_origin_is_written_once_every_sixty_seconds_of_the_clock() {
        let (log, now, lines) = hand_log();
        let at = |t: u64, times: usize| {
            now.store(t, Ordering::Relaxed);
            for _ in 0..times {
                log.note("https://obop.test");
            }
            lines
                .lock()
                .unwrap()
                .iter()
                .map(|(t, _)| *t)
                .collect::<Vec<_>>()
        };
        // The first is due when the clock reads 0, and a burst is one line.
        assert_eq!(at(0, 30), [0]);
        assert_eq!(at(1, 30), [0]);
        assert_eq!(at(59, 30), [0]);
        assert_eq!(at(60, 30), [0, 60]);
        assert_eq!(at(61, 1), [0, 60]);
        assert_eq!(at(119, 1), [0, 60]);
        assert_eq!(at(120, 1), [0, 60, 120]);
        // The line says what the operator has to do something about.
        assert_eq!(
            lines.lock().unwrap()[0].1,
            "sign: a post was refused because its origin is not https://obop.test"
        );
        // A clock that reads less than at the last line (a step back) starts the minute again,
        // and is not silent for as long as the clock was ahead.
        assert_eq!(at(10, 3), [0, 60, 120, 10]);
        assert_eq!(at(69, 3), [0, 60, 120, 10]);
        assert_eq!(at(70, 3), [0, 60, 120, 10, 70]);
        // The hooks are set once.
        assert!(!log.set_hooks(|| 0, |_| {}));
    }

    #[test]
    fn the_first_line_is_due_whatever_the_clock_reads() {
        let last = AtomicU64::new(NEVER);
        assert!(log_due(&last, u64::MAX - 1, 60));
        let last = AtomicU64::new(NEVER);
        assert!(log_due(&last, 0, 60));
        assert!(!log_due(&last, 0, 60));
    }

    #[test]
    fn two_that_ask_at_the_same_second_are_one_line_whatever_the_interleaving() {
        // Eight threads ask together, round after round (a barrier lines them up at each round),
        // each round on a log of its own and all at the same second: exactly one is due. A check
        // that reads the last line and then writes the new one as two steps lets several through
        // in some of the rounds; the compare-and-set lets one.
        use std::sync::Barrier;
        const THREADS: usize = 8;
        const ROUNDS: usize = 4000;
        let slots: Arc<Vec<AtomicU64>> =
            Arc::new((0..ROUNDS).map(|_| AtomicU64::new(NEVER)).collect());
        let gate = Arc::new(Barrier::new(THREADS));
        let threads: Vec<_> = (0..THREADS)
            .map(|_| {
                let (slots, gate) = (slots.clone(), gate.clone());
                std::thread::spawn(move || {
                    (0..ROUNDS)
                        .map(|r| {
                            gate.wait();
                            usize::from(log_due(&slots[r], 100, 60))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let mut due = vec![0usize; ROUNDS];
        for t in threads {
            for (r, won) in t.join().unwrap().into_iter().enumerate() {
                due[r] += won;
            }
        }
        let wrong: Vec<(usize, usize)> = due
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, n)| *n != 1)
            .take(5)
            .collect();
        assert!(
            wrong.is_empty(),
            "rounds with other than one line: {wrong:?}"
        );
    }

    #[test]
    fn the_default_clock_counts_the_seconds_since_the_server_started() {
        // The arithmetic, with an offset added to a start (no subtraction from now, which fails
        // on a machine that has been up for less): whole seconds, not milliseconds or minutes.
        let start = std::time::Instant::now();
        let later = start + std::time::Duration::from_millis(130_900);
        assert_eq!(seconds_between(start, later), 130);
        assert_eq!(seconds_between(later, start), 0, "never below zero");
        // The wiring: the default sink is standard error (`eprintln!`), which a test cannot
        // capture; what the default clock read is kept as the time of the last line, and that is
        // read back after a little more than a second.
        let log = OriginLog::since(std::time::Instant::now());
        assert_eq!(log.last.load(Ordering::Relaxed), NEVER);
        std::thread::sleep(std::time::Duration::from_millis(1100));
        log.note("https://obop.test");
        let at = log.last.load(Ordering::Relaxed);
        assert!((1..30).contains(&at), "the clock read {at}");
        // A second ask in the same minute writes nothing, so the time stays.
        log.note("https://obop.test");
        assert_eq!(log.last.load(Ordering::Relaxed), at);
    }

    #[test]
    fn every_refusal_asks_for_a_key_the_catalogue_has_in_every_language() {
        for e in [
            DoorError::NotFound,
            DoorError::Gone(Gone::Expired),
            DoorError::Gone(Gone::Revoked),
            DoorError::Gone(Gone::Closed),
            DoorError::Changed,
            DoorError::Busy(5),
            DoorError::Unavailable,
            DoorError::Invalid(Invalid::Name),
            DoorError::Invalid(Invalid::Agree),
            DoorError::Refused,
        ] {
            let (_, key) = refusal_parts(&e);
            for lang in LANGS {
                assert!(
                    !txt::text(lang, key).is_empty(),
                    "{e:?} asks for {key}, which {lang} does not have"
                );
            }
        }
    }
}
