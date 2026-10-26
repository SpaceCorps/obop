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
//! 3. inside a request scope the guarded backend keeps more facts: whether an earlier call of
//!    the request was refused ([`before_write`]: a refused request changes nothing more, because
//!    what it was about to write may rest on a read that failed), whether a change has been
//!    committed ([`wrote`]) and whether a step the request *needed* was refused after a change had
//!    committed ([`mark_partial`]).
//!
//! The rule the HTTP layer then applies is the one the documentation states, and it never leaves
//! a half-done request looking like a success or like nothing:
//!
//! - a request that hit a fault **before it committed any change** is answered 503 with
//!   `Retry-After` and `"applied":"none"` (after `storage_unavailable`: `"unknown"`, because a
//!   change in flight when the connection broke may or may not have been applied);
//! - a request that committed a change and then had a **needed** follow-up refused (code says so
//!   with [`mark_partial`], after trying the step again with [`patiently`]) is answered 503 with
//!   `"applied":"partial"`; so is a request that committed and then ended in an error answer while
//!   a fault was noted;
//! - a request that committed a change and only lost an **optional** follow-up (the read-back of
//!   what it wrote, a counter) keeps its own answer; the fault is counted and logged.
//!
//! Writes that record or undo what the request itself did (a claimed idempotency key that is
//! released, a token that is revoked again, the security-log entry of a committed change) are
//! allowed after a refusal: see [`bookkeeping`] and the models the guard exempts.
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

    /// Recognise a message made by [`Fault::message`]: the code word at the very start. Text that
    /// merely contains it (a validation error that echoes a name the caller chose, such as
    /// `unknown option "a: storage_busy: b"`) is not a fault, and no caller may be made to retry
    /// its own bad input. A caller that wraps the text of a fault passes it on unchanged or says
    /// so with [`Fault::note`].
    pub fn of_message(msg: &str) -> Option<Fault> {
        [Fault::Busy, Fault::Unavailable, Fault::Timeout]
            .into_iter()
            .find(|f| msg.starts_with(&format!("{}:", f.code())))
    }

    /// Does this error text say the store was too busy or unreachable, as opposed to refusing the
    /// request? For callers that must tell "not found" from "could not look": they treat it as
    /// "try later".
    pub fn is_transient(msg: &str) -> bool {
        Self::of_message(msg).is_some()
    }
}

/// What the guard says when it refuses a change because the request already hit a fault. Waiting
/// does not change that answer, so [`patiently`] does not wait for it.
const GUARD_REFUSAL: &str =
    "an earlier storage call of this request was refused, so this change was not made";

#[derive(Clone, Copy, Default)]
struct Scope {
    active: bool,
    fault: Option<Fault>,
    /// Changes committed and not undone by a compensation.
    changes: u32,
    partial: bool,
    /// Depth of [`allow`] and [`bookkeeping`] calls: writes are not refused for an earlier fault.
    bypass: u32,
    /// Depth of [`bookkeeping`] calls: a record change undoes a counted change.
    undo: u32,
    /// Depth of [`uncounted`] calls: what they write does not count as a change of the request.
    quiet: u32,
}

thread_local! {
    static SCOPE: Cell<Scope> = const { Cell::new(Scope { active: false, fault: None, changes: 0, partial: false, bypass: 0, undo: 0, quiet: 0 }) };
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
            // The guard's own refusal does not pass by waiting: the request already hit a fault.
            Err(e) if Fault::is_transient(&e) && n < attempts && !e.contains(GUARD_REFUSAL) => {
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
    /// A change (a record, not a counter, a dedup marker or a security-log entry) was committed
    /// and not undone.
    pub wrote: bool,
    /// A step the request needed was refused after a change had committed ([`mark_partial`]).
    pub partial: bool,
}

/// What this thread's request scope has seen so far, without ending it.
pub fn state() -> Outcome {
    SCOPE.with(|c| {
        let s = c.get();
        Outcome {
            fault: s.fault,
            wrote: s.changes > 0,
            partial: s.partial,
        }
    })
}

/// Start a request scope on this thread. `earlier` is what an earlier blocking part of the same
/// request already did (a request may run several, on different threads).
pub fn begin_request(earlier: Outcome) {
    SCOPE.with(|c| {
        c.set(Scope {
            active: true,
            fault: earlier.fault,
            changes: u32::from(earlier.wrote),
            partial: earlier.partial,
            bypass: 0,
            undo: 0,
            quiet: 0,
        })
    });
}

/// End the request scope on this thread and say what happened in it.
pub fn end_request() -> Outcome {
    let o = state();
    SCOPE.with(|c| c.set(Scope::default()));
    o
}

/// Called by the guarded backend before every call that changes state. In a request scope that
/// already hit a fault the call is refused with the same fault: the request is going to be
/// answered 503, and what it was about to write may rest on a read that failed (a vault, a plan,
/// a lookup that came back empty), so it must not write at all. Writes inside [`allow`] and
/// [`bookkeeping`] are let through.
pub fn before_write() -> Result<(), String> {
    let s = SCOPE.with(Cell::get);
    match (s.active, s.fault) {
        (true, Some(f)) if s.bypass == 0 => Err(f.message(GUARD_REFUSAL)),
        _ => Ok(()),
    }
}

/// Called by the guarded backend after a change committed. Inside [`bookkeeping`] it undoes one
/// counted change instead (the write was the compensation of an earlier one).
pub fn wrote() {
    SCOPE.with(|c| {
        let mut s = c.get();
        if s.active && s.quiet == 0 {
            if s.undo > 0 {
                s.changes = s.changes.saturating_sub(1);
            } else {
                s.changes += 1;
            }
            c.set(s);
        }
    });
}

/// A step this request needed was refused even after trying again, and a change of it has already
/// committed: the request is answered 503 with `"applied":"partial"`, whatever the handler
/// answers, so that no client repeats what was done and none believes it finished.
pub fn mark_partial() {
    SCOPE.with(|c| {
        let mut s = c.get();
        if s.active {
            // Before any change committed the request is not partial, it is plainly refused.
            s.partial = s.partial || s.changes > 0;
            if s.fault.is_none() {
                s.fault = Some(Fault::Busy);
            }
            c.set(s);
        }
    });
}

/// Something happened outside the store that this request cannot take back (money returned by a
/// payment provider, a plan consumed): it counts as a change, so that a step that is then refused
/// makes the request a partial one ([`mark_partial`]) although no record of it committed.
pub fn external_change() {
    SCOPE.with(|c| {
        let mut s = c.get();
        if s.active {
            s.changes += 1;
            c.set(s);
        }
    });
}

/// Restores the depth counters when a scoped call ends, also by a panic.
struct Leave {
    bypass: bool,
    undo: bool,
    quiet: bool,
}

impl Drop for Leave {
    fn drop(&mut self) {
        SCOPE.with(|c| {
            let mut s = c.get();
            if self.bypass {
                s.bypass = s.bypass.saturating_sub(1);
            }
            if self.undo {
                s.undo = s.undo.saturating_sub(1);
            }
            if self.quiet {
                s.quiet = s.quiet.saturating_sub(1);
            }
            c.set(s);
        });
    }
}

fn enter(bypass: bool, undo: bool, quiet: bool) -> Leave {
    SCOPE.with(|c| {
        let mut s = c.get();
        if bypass {
            s.bypass += 1;
        }
        if undo {
            s.undo += 1;
        }
        if quiet {
            s.quiet += 1;
        }
        c.set(s);
    });
    Leave {
        bypass,
        undo,
        quiet,
    }
}

/// Run `f` with its writes allowed although an earlier call of the request was refused. For a
/// write that rests on what the request itself just did, not on something it read (the follow-up
/// that holds a blob for a record just created, the mask vault of an answer already built, a
/// session). Its changes count as usual.
pub fn allow<T>(f: impl FnOnce() -> T) -> T {
    let _leave = enter(true, false, false);
    f()
}

/// Run `f` as bookkeeping: as [`allow`], and a record it changes undoes one counted change instead
/// of adding one. For the writes that undo what this request itself did (release an idempotency
/// key it claimed, revoke a token it issued, release a webhook's dedup marker).
pub fn bookkeeping<T>(f: impl FnOnce() -> T) -> T {
    let _leave = enter(true, true, false);
    f()
}

/// Run `f` and do not count what it writes as a change of the request. For a note that only
/// helps a repeat along (what is owed after a change that has not been made yet): if the change
/// itself is then refused, the request has changed nothing and is answered as such. The guard
/// still refuses the write after an earlier fault.
pub fn uncounted<T>(f: impl FnOnce() -> T) -> T {
    let _leave = enter(false, false, true);
    f()
}

/// As [`uncounted`], and allowed after an earlier fault: for taking such a note back.
pub fn uncounted_allowed<T>(f: impl FnOnce() -> T) -> T {
    let _leave = enter(true, false, true);
    f()
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
            // Only the start counts: text that the caller chose and that is echoed back in an
            // error is not a fault, wherever the code word sits in it.
            assert_eq!(Fault::of_message(&format!("could not save: {m}")), None);
            let echoed = format!(r#"unknown option "a: {}: b" (use filter, order)"#, f.code());
            assert_eq!(Fault::of_message(&echoed), None, "{echoed}");
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

        begin_request(Outcome::default());
        assert!(before_write().is_ok());
        wrote();
        note(Fault::Timeout);
        let e = before_write().unwrap_err();
        assert_eq!(Fault::of_message(&e), Some(Fault::Timeout), "{e}");
        assert_eq!(
            end_request(),
            Outcome {
                fault: Some(Fault::Timeout),
                wrote: true,
                partial: false
            }
        );
        // The scope is over: the thread is clean again.
        assert!(before_write().is_ok());
        assert_eq!(take(), None);

        // A fault of an earlier blocking section of the same request carries over.
        begin_request(Outcome {
            fault: Some(Fault::Busy),
            ..Outcome::default()
        });
        assert_eq!(
            Fault::of_message(&before_write().unwrap_err()),
            Some(Fault::Busy)
        );
        // Bookkeeping passes after a refusal ...
        assert!(bookkeeping(before_write).is_ok());
        assert!(before_write().is_err(), "and only inside the closure");
        // ... and a record it changes undoes one counted change instead of adding one.
        wrote();
        assert!(state().wrote);
        bookkeeping(wrote);
        assert!(!state().wrote, "the compensation undid the change");
        bookkeeping(wrote);
        assert!(!state().wrote, "and cannot go below none");
        // `allow` lets a write through too, but what it changes counts as any other change.
        assert!(allow(before_write).is_ok());
        allow(wrote);
        assert!(state().wrote, "a derived write is a change");
        bookkeeping(wrote);
        assert!(!state().wrote);
        // A needed step that was refused after a commit is the "partial" outcome.
        wrote();
        mark_partial();
        assert_eq!(
            end_request(),
            Outcome {
                fault: Some(Fault::Busy),
                wrote: true,
                partial: true
            }
        );
        // A write that is not counted, and a change outside the store, are what they say.
        begin_request(Outcome::default());
        uncounted(wrote);
        assert!(!state().wrote, "an uncounted note is not a change");
        external_change();
        assert!(state().wrote, "money that moved outside the store is");
        mark_partial();
        assert!(end_request().partial);
        // Before any change committed a refused step is a plain refusal, not a partial request.
        begin_request(Outcome::default());
        mark_partial();
        assert_eq!(
            end_request(),
            Outcome {
                fault: Some(Fault::Busy),
                wrote: false,
                partial: false
            }
        );
        // `mark_partial` outside a request scope does nothing.
        mark_partial();
        assert_eq!(state(), Outcome::default());
    }
}
