//! How a storage failure that is not the caller's fault reaches the HTTP layer (D74).
//!
//! The storage contract answers with `Result<T, String>` and dozens of callers turn an `Err` into
//! something else on the way up: a session lookup that fails becomes "not signed in", a counter
//! that fails becomes "allowed", a count becomes zero. A typed error would therefore be lost in
//! exactly the places where it matters, so the pool does a few small things instead:
//!
//! 1. the error text starts with a fixed code word (`storage_busy: ...`, `storage_unavailable: ...`,
//!    `storage_timeout: ...`), which a command line or a log reader can read and
//!    [`Fault::of_message`] can recognise in one place;
//! 2. the pool *notes* the fault on the calling thread ([`note`]). A request handler runs from
//!    start to end on one blocking thread at a time, so `obop-http` opens a request scope on that
//!    thread ([`begin_request`]) and closes it after the handler ([`end_request`]);
//! 3. inside a request scope the guarded backend keeps two more facts: whether an earlier call of
//!    the request was refused ([`before_write`]: a refused request changes nothing more, because
//!    what it was about to write may rest on a read that failed) and whether a change has been
//!    committed ([`wrote`]).
//!
//! The rule the HTTP layer then applies is the one the documentation states: **a request that hit
//! a fault before it committed any change is answered 503 with `Retry-After`, and nothing was
//! changed by it; a request that had already committed a change keeps its own answer** (a fault
//! after that point only costs a best-effort step, and is counted and logged). The exception is
//! `storage_unavailable`: a write that was in flight when the connection broke may or may not have
//! been applied.
//!
//! Threads without a request scope (the connector worker, provisioning threads) get the error
//! text only; the thread note is of no use to them and does not block anything.

use std::cell::Cell;

/// A failure of the store itself, as opposed to a refused or invalid request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// Every database connection was in use for the whole acquire timeout, or too many callers
    /// were already waiting (D74). The call did not start.
    Busy,
    /// The database could not be reached or the connection broke. The outcome of a write that was
    /// in flight is unknown.
    Unavailable,
    /// A statement ran into its time limit (`statement_timeout`) and was cancelled; the server
    /// rolled it back.
    Timeout,
}

impl Fault {
    /// The stable code word: `storage_busy`, `storage_unavailable` or `storage_timeout`.
    pub fn code(self) -> &'static str {
        match self {
            Fault::Busy => "storage_busy",
            Fault::Unavailable => "storage_unavailable",
            Fault::Timeout => "storage_timeout",
        }
    }

    /// Seconds a client should wait before it tries again (`Retry-After`).
    pub fn retry_after_secs(self) -> u32 {
        match self {
            Fault::Busy => 1,
            Fault::Unavailable | Fault::Timeout => 5,
        }
    }

    /// The error text a backend returns: the code word, a colon and a short explanation. The
    /// explanation never names a host, a user or a statement.
    pub fn message(self, detail: &str) -> String {
        format!("{}: {detail}", self.code())
    }

    /// Recognise a message made by [`Fault::message`], also when a caller wrapped it in more text.
    pub fn of_message(msg: &str) -> Option<Fault> {
        [Fault::Busy, Fault::Unavailable, Fault::Timeout]
            .into_iter()
            .find(|f| {
                msg.starts_with(&format!("{}:", f.code()))
                    || msg.contains(&format!(": {}:", f.code()))
            })
    }

    /// Does this error text say the store was too busy or unreachable, as opposed to refusing the
    /// request? For callers that must tell "not found" from "could not look": they treat it as
    /// "try later".
    pub fn is_transient(msg: &str) -> bool {
        Self::of_message(msg).is_some()
    }
}

#[derive(Clone, Copy, Default)]
struct Scope {
    active: bool,
    fault: Option<Fault>,
    wrote: bool,
}

thread_local! {
    static SCOPE: Cell<Scope> = const { Cell::new(Scope { active: false, fault: None, wrote: false }) };
}

/// Remember on this thread that a storage fault happened. The first one since the last
/// [`clear`], [`take`] or [`begin_request`] is kept (it is the one that started the trouble).
pub fn note(f: Fault) {
    SCOPE.with(|c| {
        let mut s = c.get();
        if s.fault.is_none() {
            s.fault = Some(f);
            c.set(s);
        }
    });
}

/// Forget any fault noted on this thread (and leave any request scope).
pub fn clear() {
    SCOPE.with(|c| c.set(Scope::default()));
}

/// The fault noted on this thread since the last [`clear`], if any, and forget it.
pub fn take() -> Option<Fault> {
    SCOPE.with(|c| {
        let mut s = c.get();
        let f = s.fault.take();
        c.set(s);
        f
    })
}

/// Run `f` again, `pause` apart and at most `attempts` times in all, while it fails with a
/// transient storage fault (a full pool, an unreachable database). For threads that have nobody to
/// answer, such as the connector worker and the provisioning threads: for them a refused call
/// means "later", not "no", and a final status update must not be lost to a busy moment.
pub fn patiently<T>(
    attempts: u32,
    pause: std::time::Duration,
    mut f: impl FnMut() -> Result<T, String>,
) -> Result<T, String> {
    let mut n = 1;
    loop {
        match f() {
            Err(e) if Fault::is_transient(&e) && n < attempts => {
                n += 1;
                std::thread::sleep(pause);
            }
            other => return other,
        }
    }
}

/// The fault noted on this thread, without forgetting it. For code that must tell "there is no such
/// record" from "the lookup was refused" after a trait method flattened both into `None`: it asks
/// whether a fault was noted meanwhile, and then treats the answer as "try later".
pub fn noted() -> Option<Fault> {
    SCOPE.with(|c| c.get().fault)
}

/// What a request did to the store, as far as the HTTP layer needs to know.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Outcome {
    /// The first storage fault of the request, if any.
    pub fault: Option<Fault>,
    /// A change (a record or value, not a counter or a security-log entry) was committed.
    pub wrote: bool,
}

/// Start a request scope on this thread. `earlier` is the fault an earlier part of the same
/// request already hit (a request may run several blocking sections, on different threads).
pub fn begin_request(earlier: Option<Fault>, wrote_earlier: bool) {
    SCOPE.with(|c| {
        c.set(Scope {
            active: true,
            fault: earlier,
            wrote: wrote_earlier,
        })
    });
}

/// End the request scope on this thread and say what happened in it.
pub fn end_request() -> Outcome {
    SCOPE.with(|c| {
        let s = c.get();
        c.set(Scope::default());
        Outcome {
            fault: s.fault,
            wrote: s.wrote,
        }
    })
}

/// Called by the guarded backend before every call that changes state. In a request scope that
/// already hit a fault the call is refused with the same fault: the request is going to be
/// answered 503, and what it was about to write may rest on a read that failed (a vault, a plan,
/// a lookup that came back empty), so it must not write at all.
pub fn before_write() -> Result<(), String> {
    let s = SCOPE.with(Cell::get);
    match (s.active, s.fault) {
        (true, Some(f)) => Err(f.message(
            "an earlier storage call of this request was refused, so this change was not made",
        )),
        _ => Ok(()),
    }
}

/// Called by the guarded backend after a change committed.
pub fn wrote() {
    SCOPE.with(|c| {
        let mut s = c.get();
        if s.active {
            s.wrote = true;
            c.set(s);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_start_with_the_code_word_and_are_recognised_again() {
        for f in [Fault::Busy, Fault::Unavailable, Fault::Timeout] {
            let m = f.message("try again");
            assert!(m.starts_with(f.code()), "{m}");
            assert_eq!(Fault::of_message(&m), Some(f));
            assert!(Fault::is_transient(&m));
            // A caller that wraps the text keeps it recognisable.
            assert_eq!(Fault::of_message(&format!("could not save: {m}")), Some(f));
        }
        assert_eq!(Fault::of_message("duplicate key value"), None);
        assert_eq!(Fault::of_message("unknown field storage_busy"), None);
        assert!(!Fault::is_transient("no such record"));
    }

    #[test]
    fn the_first_fault_wins_and_take_clears_it() {
        clear();
        assert_eq!(take(), None);
        note(Fault::Timeout);
        note(Fault::Busy);
        assert_eq!(take(), Some(Fault::Timeout));
        assert_eq!(take(), None);
        note(Fault::Busy);
        clear();
        assert_eq!(take(), None);
    }

    #[test]
    fn a_note_belongs_to_its_thread() {
        clear();
        std::thread::spawn(|| note(Fault::Busy)).join().unwrap();
        assert_eq!(take(), None);
    }

    #[test]
    fn a_request_scope_blocks_writes_after_a_fault_and_remembers_commits() {
        clear();
        // Outside a scope nothing blocks, however stale the note.
        note(Fault::Busy);
        assert!(before_write().is_ok());
        wrote();
        assert_eq!(
            take(),
            Some(Fault::Busy),
            "the note is there for whoever reads it"
        );
        clear();

        begin_request(None, false);
        assert!(before_write().is_ok());
        wrote();
        note(Fault::Timeout);
        let e = before_write().unwrap_err();
        assert_eq!(Fault::of_message(&e), Some(Fault::Timeout), "{e}");
        assert_eq!(
            end_request(),
            Outcome {
                fault: Some(Fault::Timeout),
                wrote: true
            }
        );
        // The scope is over: the thread is clean again.
        assert!(before_write().is_ok());
        assert_eq!(take(), None);

        // A fault of an earlier blocking section of the same request carries over.
        begin_request(Some(Fault::Busy), false);
        assert_eq!(
            Fault::of_message(&before_write().unwrap_err()),
            Some(Fault::Busy)
        );
        end_request();
    }
}
