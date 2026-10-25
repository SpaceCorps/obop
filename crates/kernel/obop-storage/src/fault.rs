//! How a storage failure that is not the caller's fault reaches the HTTP layer (D74).
//!
//! The storage contract answers with `Result<T, String>` and dozens of callers turn an `Err` into
//! something else on the way up: a session lookup that fails becomes "not signed in", a counter
//! that fails becomes "allowed", a count becomes zero. A typed error would therefore be lost in
//! exactly the places where it matters, so the pool does two small things instead:
//!
//! 1. the error text starts with a fixed code word (`storage_busy: ...`, `storage_unavailable: ...`,
//!    `storage_timeout: ...`), which a command line or a log reader can read and
//!    [`Fault::of_message`] can recognise in one place;
//! 2. the pool *notes* the fault on the calling thread ([`note`]). A request handler runs from
//!    start to end on one blocking thread, so `obop-http` clears the note before the handler and
//!    reads it after ([`take`]); if a fault happened anywhere in between, the whole answer is
//!    replaced by a 503 with `Retry-After`, whatever the layers above did with the error.
//!
//! A backend that cannot serve because it is saturated, unreachable or too slow calls [`note`]
//! and returns a message made by [`Fault::message`]. Nothing here touches a request that did not
//! hit a fault.

use std::cell::Cell;

/// A failure of the store itself, as opposed to a refused or invalid request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// Every database connection was in use for the whole acquire timeout, or too many callers
    /// were already waiting (D74). The request did not start; retrying later is safe.
    Busy,
    /// The database could not be reached or the connection broke. The outcome of a write that was
    /// in flight is unknown.
    Unavailable,
    /// A statement ran into its time limit (`statement_timeout`) and was cancelled.
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
}

thread_local! {
    static NOTED: Cell<Option<Fault>> = const { Cell::new(None) };
}

/// Remember on this thread that a storage fault happened. The first one since the last
/// [`clear`] or [`take`] is kept (it is the one that started the trouble).
pub fn note(f: Fault) {
    NOTED.with(|c| {
        if c.get().is_none() {
            c.set(Some(f));
        }
    });
}

/// Forget any fault noted on this thread (called before a handler starts).
pub fn clear() {
    NOTED.with(|c| c.set(None));
}

/// The fault noted on this thread since the last [`clear`], if any, and forget it.
pub fn take() -> Option<Fault> {
    NOTED.with(Cell::take)
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
            // A caller that wraps the text keeps it recognisable.
            assert_eq!(Fault::of_message(&format!("could not save: {m}")), Some(f));
        }
        assert_eq!(Fault::of_message("duplicate key value"), None);
        assert_eq!(Fault::of_message("unknown field storage_busy"), None);
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
}
