//! The one place that turns a `CallError` into what a caller sees (K16): an HTTP status, a stable
//! machine code and a JSON body.
//!
//! REST (`obop-http`), MCP (`obop-mcp`) and the CLI (`obop`) call `classify` and never read a
//! handler string themselves, so a feature that adds a new kind of refusal adds one row to
//! `TABLE` and edits no surface.
//!
//! # The convention
//!
//! A handler refuses with `Err("<code>: <message>")` (see `coded`), where `<code>` is the code of
//! a row of `TABLE`. `classify` then answers with the status of the row, a body
//! `{"error": <message>, "code": <code>, "hint": <hint of the row>}` and the `Retry-After` of the
//! row, if it has one (the three storage rows, below, name the code in `error` and add `applied`).
//! A handler string without a known code is what it always was: status 400
//! and the body `{"error": <string>}`, with no `code` (those bodies are part of the documented API
//! and do not change).
//!
//! # Storage faults (D74)
//!
//! A refused or failed store call (`storage_busy`, `storage_unavailable`, `storage_timeout`: the
//! words `obop_storage::fault` puts at the start of the error text) is a 503 with `Retry-After` on
//! every surface, and it says what became of the call's own changes, `applied`: `none` (nothing
//! was changed, repeat it), `unknown` (`storage_unavailable` only: a change in flight when the
//! connection broke may or may not have been applied, read before you repeat a write) or
//! `partial` (a change committed and a step the call needed was refused, do not repeat it
//! blindly). This module owns all of it:
//!
//! - the three rows of `TABLE`, with the status and the seconds to wait;
//! - `applied`, the rule that turns what a request did to the store (`Outcome`) into one of the
//!   three words, or into "the request keeps its own answer" when only something optional was
//!   lost after a commit;
//! - `storage_fault`, the answer for a fault with its `applied`: status, body, hint, `Retry-After`,
//!   the line the command line prints and its exit status;
//! - `classify`, which reads the code word of a handler string and answers the same way, with
//!   `applied` from the text alone (`none`, or `unknown` after `storage_unavailable`).
//!
//! The surfaces only shape it: REST sets the status, the body and the headers (`Retry-After`,
//! `X-Obop-Applied`); `obop-http`'s layer `storage_faults` knows what the whole request did and
//! replaces any answer of a request that hit a fault by `storage_fault` in the shape of its route
//! (OAuth error, JSON-RPC error with `error.data`, a page); an MCP tool call that fails is an error
//! result with the same body and `retry_after`; the command line prints `Classified::cli` and
//! exits with `Classified::exit` (75, "try again later", for a storage fault).

use crate::CallError;
use obop_storage::fault::{self, Fault, Outcome};
use serde_json::{Value as J, json};

/// The codes of `TABLE` as constants, for the handlers that produce them.
pub mod code {
    /// D75: the record is not at the version the caller read.
    pub const PRECONDITION_FAILED: &str = "precondition_failed";
    /// D77: a frozen record may not change that way.
    pub const IMMUTABLE_RECORD: &str = "immutable_record";
    /// D77: the record changed while the action ran.
    pub const STATE_CONFLICT: &str = "state_conflict";
    /// D77: the database refused to change a protection.
    pub const GUARD_CHANGE_REFUSED: &str = "guard_change_refused";
    /// D78: the cursor is genuine but too old.
    pub const CURSOR_EXPIRED: &str = "cursor_expired";
    /// D78: the cursor is not one this call issued.
    pub const CURSOR_INVALID: &str = "cursor_invalid";
    /// D78: the sort key of the last row is too long to put in a cursor.
    pub const CURSOR_KEY_TOO_LONG: &str = "cursor_key_too_long";
    /// D78: the offset is beyond what a listing serves.
    pub const OFFSET_TOO_LARGE: &str = "offset_too_large";
    /// D78: the model is too big to list without a limit.
    pub const LIMIT_REQUIRED: &str = "limit_required";
    /// D78: the option exists on `search` only.
    pub const USE_SEARCH: &str = "use_search";
    /// Step 2: the caller is known and may not do this.
    pub const DENIED: &str = "denied";
    /// Step 2: the caller must confirm their identity again.
    pub const REAUTH_REQUIRED: &str = "reauth_required";
    /// D74: every database connection was in use for the whole acquire timeout, or too many callers
    /// were already waiting. The call did not start.
    pub const STORAGE_BUSY: &str = "storage_busy";
    /// D74: the database could not be reached or the connection broke; the outcome of a change in
    /// flight is unknown.
    pub const STORAGE_UNAVAILABLE: &str = "storage_unavailable";
    /// D74: a statement ran into its time limit and was rolled back.
    pub const STORAGE_TIMEOUT: &str = "storage_timeout";

    // What `classify` answers for the errors that have always existed. These codes name the
    // class (they are what `Classified::code` holds); the legacy bodies do not carry them.
    /// An agent's proposal waits for a human.
    pub const APPROVAL_REQUIRED: &str = "approval_required";
    /// Unknown, hidden or forbidden command, or unknown plan.
    pub const NOT_FOUND: &str = "not_found";
    /// A masked token in the input could not be restored.
    pub const INVALID_TOKEN: &str = "invalid_token";
    /// A handler refused with text that carries no known code.
    pub const BAD_REQUEST: &str = "bad_request";
}

/// One kind of refusal: its code, its HTTP status and what it tells the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    /// The stable machine code; also the prefix of the handler string (`code: message`).
    pub code: &'static str,
    pub status: u16,
    /// Seconds to wait before trying again (`Retry-After`), for the refusals that mean "later".
    pub retry_after: Option<u32>,
    /// The decision that owns the code.
    pub origin: &'static str,
    /// What the caller can do about it.
    pub hint: &'static str,
}

/// The hints of the storage faults (D74), by what became of the call's own changes. A row carries
/// the hint of the case that is the rule for its code (`none` for busy and timeout, `unknown`
/// for unavailable); `storage_hint` picks the others.
const HINT_BUSY: &str = "Every database connection is in use. Nothing was changed. Retry after the time in Retry-After.";
const HINT_TIMEOUT: &str = "A database statement ran into its time limit. Nothing was changed. Retry later, or ask for less at once.";
const HINT_UNKNOWN: &str = "The database cannot be reached right now. A change that was in flight may or may not have been applied: read before you repeat a write. Retry after the time in Retry-After.";
const HINT_UNAVAILABLE_NONE: &str = "The database cannot be reached right now. Nothing was changed. Retry after the time in Retry-After.";
const HINT_PARTIAL: &str = "Part of this request was applied and a step it needed was refused. Do not repeat it blindly: read the records it touched, then finish or undo the rest. Retry after the time in Retry-After.";

/// The table: one row per code. Producers of the codes of D75, D77, D78 and step 2 are being built;
/// their rows are here first so none of them edits a surface. The three storage rows (D74) are
/// answered by `storage_fault`, which adds `applied`.
pub const TABLE: &[Row] = &[
    Row {
        code: code::PRECONDITION_FAILED,
        status: 412,
        retry_after: None,
        origin: "D75",
        hint: "Read the record again and retry with the new if_version.",
    },
    Row {
        code: code::IMMUTABLE_RECORD,
        status: 409,
        retry_after: None,
        origin: "D77",
        hint: "The record is frozen. Use the action that reopens it, or change only the fields it still allows.",
    },
    Row {
        code: code::STATE_CONFLICT,
        status: 409,
        retry_after: None,
        origin: "D77",
        hint: "Reload the record and try again.",
    },
    Row {
        code: code::GUARD_CHANGE_REFUSED,
        status: 409,
        retry_after: None,
        origin: "D77",
        hint: "A protection can only be changed by the owner of the database.",
    },
    Row {
        code: code::CURSOR_EXPIRED,
        status: 410,
        retry_after: None,
        origin: "D78",
        hint: "Start the listing again without a cursor.",
    },
    Row {
        code: code::CURSOR_INVALID,
        status: 400,
        retry_after: None,
        origin: "D78",
        hint: "Pass the cursor the previous page returned, unchanged, to the same call.",
    },
    Row {
        code: code::CURSOR_KEY_TOO_LONG,
        status: 400,
        retry_after: None,
        origin: "D78",
        hint: "A value of the sort key is too long to continue from; order by another field.",
    },
    Row {
        code: code::OFFSET_TOO_LARGE,
        status: 400,
        retry_after: None,
        origin: "D78",
        hint: "Continue with a cursor instead of a large offset.",
    },
    Row {
        code: code::LIMIT_REQUIRED,
        status: 400,
        retry_after: None,
        origin: "D78",
        hint: "Pass a limit, and continue with a cursor.",
    },
    Row {
        code: code::USE_SEARCH,
        status: 400,
        retry_after: None,
        origin: "D78",
        hint: "Use search: list answers a plain array and has no such option.",
    },
    Row {
        code: code::DENIED,
        status: 403,
        retry_after: None,
        origin: "step 2",
        hint: "Your account may not do this.",
    },
    Row {
        code: code::REAUTH_REQUIRED,
        status: 400,
        retry_after: None,
        origin: "step 2",
        hint: "Confirm your identity again, then retry.",
    },
    Row {
        code: code::STORAGE_BUSY,
        status: 503,
        retry_after: Some(1),
        origin: "D74",
        hint: HINT_BUSY,
    },
    Row {
        code: code::STORAGE_UNAVAILABLE,
        status: 503,
        retry_after: Some(5),
        origin: "D74",
        hint: HINT_UNKNOWN,
    },
    Row {
        code: code::STORAGE_TIMEOUT,
        status: 503,
        retry_after: Some(5),
        origin: "D74",
        hint: HINT_TIMEOUT,
    },
];

/// The row of a code, if it has one.
pub fn row(code: &str) -> Option<&'static Row> {
    TABLE.iter().find(|r| r.code == code)
}

/// The handler string for a refusal with a code: `code: message`. `classify` reads it back.
pub fn coded(code: &'static str, message: &str) -> String {
    debug_assert!(row(code).is_some(), "{code} is not a row of errors::TABLE");
    format!("{code}: {message}")
}

/// What kind of answer the call earned, apart from its status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// An agent proposed a call that needs a human; nothing ran. Approve `plan_id` to run it.
    Proposed { plan_id: String },
    /// Unknown, hidden and forbidden answer alike, so a caller cannot probe for commands.
    Hidden,
    /// The call ran (or tried to) and was refused or failed.
    Failed,
}

/// A `CallError` as the surfaces show it.
#[derive(Debug, Clone, PartialEq)]
pub struct Classified {
    pub kind: Kind,
    /// The HTTP status. MCP and the CLI show no status but use it to tell classes apart.
    pub status: u16,
    /// The stable machine code (`code::*`).
    pub code: &'static str,
    /// The JSON the caller gets: the REST response body and the MCP error result. Bodies of the
    /// errors that have always existed are unchanged, so they carry no `code`; the rows of
    /// `TABLE` do.
    pub body: J,
    /// For `precondition_failed`: the version the record has now (REST puts it in `ETag`).
    pub current_version: Option<i64>,
    /// Seconds to wait (`Retry-After`), for the refusals that mean "later".
    pub retry_after: Option<u32>,
    /// What the CLI prints after `error: `. The errors that have always existed print their
    /// `Debug` form (`Handler("no lead nope")`, documented in the CLI guide).
    pub cli: String,
    /// The hint the answer carries, for a surface that shows it apart from the body (the JSON-RPC
    /// error of `/mcp`). The errors that have always existed have none.
    pub hint: Option<&'static str>,
    /// D74, a storage fault only: what became of the call's own changes.
    pub applied: Option<Applied>,
    /// The exit status of the command line: 1, or `EXIT_TRY_AGAIN` for a storage fault.
    pub exit: u8,
}

/// The exit status of the command line after a storage fault: "temporary failure, try again"
/// (`EX_TEMPFAIL` of `sysexits.h`), so a script can tell it from an error of the call itself.
pub const EXIT_TRY_AGAIN: u8 = 75;

impl Classified {
    fn new(kind: Kind, status: u16, code: &'static str, body: J, cli: String) -> Self {
        Self {
            kind,
            status,
            code,
            body,
            current_version: None,
            retry_after: None,
            cli,
            hint: None,
            applied: None,
            exit: 1,
        }
    }

    /// The `data` of a JSON-RPC error that answers this classification: the hint, the seconds to
    /// wait and `applied`, whichever the answer has.
    pub fn rpc_data(&self) -> J {
        let mut d = serde_json::Map::new();
        if let Some(h) = self.hint {
            d.insert("hint".into(), json!(h));
        }
        if let Some(s) = self.retry_after {
            d.insert("retry_after".into(), json!(s));
        }
        if let Some(a) = self.applied {
            d.insert("applied".into(), json!(a.word()));
        }
        J::Object(d)
    }
}

/// What a storage fault did to the changes of the call that met it (D74). The word is in the
/// `applied` field of the body and in the `X-Obop-Applied` header of the 503.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    /// Nothing was changed: the call may be repeated.
    None,
    /// A change in flight when the connection broke may or may not have been applied
    /// (`storage_unavailable` only).
    Unknown,
    /// A change committed and a step the call needed was refused: do not repeat it blindly, read
    /// the records it touched and finish or undo the rest.
    Partial,
}

impl Applied {
    /// The word for the body and the header: `none`, `unknown` or `partial`.
    pub fn word(self) -> &'static str {
        match self {
            Applied::None => "none",
            Applied::Unknown => "unknown",
            Applied::Partial => "partial",
        }
    }
}

/// What a request that met `fault` says about its own changes, from what it did to the store
/// (`outcome`) and whether the answer it was about to give is an error (`own_answer_failed`: a
/// status of 400 or more); `None` when it keeps its own answer.
///
/// - A needed step was refused after a change had committed, or the request committed and ends in
///   an error answer: `Partial`. Never a 200, never a 4xx.
/// - Nothing committed: `None`, or `Unknown` after `storage_unavailable` (a change in flight when
///   the connection broke may or may not have been applied).
/// - A change committed and only something optional was lost (the read-back of what it wrote, a
///   counter) while the answer is a success: the request keeps its answer (`None` here: the
///   caller counts and logs the fault).
pub fn applied(fault: Fault, outcome: &Outcome, own_answer_failed: bool) -> Option<Applied> {
    if outcome.partial || (outcome.wrote && own_answer_failed) {
        Some(Applied::Partial)
    } else if !outcome.wrote {
        Some(if fault == Fault::Unavailable {
            Applied::Unknown
        } else {
            Applied::None
        })
    } else {
        None
    }
}

/// What the caller can do about a storage fault, for what became of its changes.
pub fn storage_hint(fault: Fault, applied: Applied) -> &'static str {
    match (fault, applied) {
        (_, Applied::Partial) => HINT_PARTIAL,
        (Fault::Busy, _) => HINT_BUSY,
        (Fault::Timeout, _) => HINT_TIMEOUT,
        (Fault::Unavailable, Applied::Unknown) => HINT_UNKNOWN,
        (Fault::Unavailable, Applied::None) => HINT_UNAVAILABLE_NONE,
    }
}

/// The answer for a storage fault (D74): 503, `Retry-After` of the fault's row, and the body
/// `{"error": <code>, "code": <code>, "hint", "applied"}`. `detail` is the explanation that came
/// with the fault (the text after the code word); only the command line shows it, because it is
/// the one surface that has no other place for it.
pub fn storage_fault(fault: Fault, applied: Applied, detail: Option<&str>) -> Classified {
    debug_assert!(
        applied != Applied::Unknown || fault == Fault::Unavailable,
        "only storage_unavailable leaves the outcome of a change unknown"
    );
    let r = row(fault.code()).expect("the table has a row for every storage fault");
    let hint = storage_hint(fault, applied);
    let line = match detail.map(str::trim).filter(|d| !d.is_empty()) {
        Some(d) => format!("{}: {d}", r.code),
        None => r.code.to_string(),
    };
    let wait = r.retry_after.unwrap_or(1);
    let what = match applied {
        Applied::None => "nothing was changed",
        Applied::Unknown => {
            "a change in flight may or may not have been applied: read before you repeat a write"
        }
        Applied::Partial => "part of it was applied: do not repeat it blindly",
    };
    let mut c = Classified::new(
        Kind::Failed,
        r.status,
        r.code,
        json!({"error": r.code, "code": r.code, "hint": hint, "applied": applied.word()}),
        format!("{line} ({what}; try again in {wait} s)"),
    );
    c.retry_after = r.retry_after;
    c.hint = Some(hint);
    c.applied = Some(applied);
    c.exit = EXIT_TRY_AGAIN;
    c
}

/// The answer for an error text that starts with the code word of a storage fault, or `None` for
/// any other text. The text says nothing about the rest of the request, so `applied` is what the
/// text alone allows: `unknown` after `storage_unavailable` (unless it is the guard's refusal of a
/// change after an earlier fault, which changed nothing), else `none`.
pub fn of_storage_text(msg: &str) -> Option<Classified> {
    let fault = Fault::of_message(msg)?;
    let applied = if fault::may_have_applied(msg) {
        Applied::Unknown
    } else {
        Applied::None
    };
    let detail = msg.get(fault.code().len() + 1..);
    Some(storage_fault(fault, applied, detail))
}

/// Classify a call error. The only function that does.
pub fn classify(e: &CallError) -> Classified {
    let debug = || format!("{e:?}");
    match e {
        CallError::ApprovalRequired { plan_id } => Classified::new(
            Kind::Proposed {
                plan_id: plan_id.clone(),
            },
            202,
            code::APPROVAL_REQUIRED,
            json!({"approval_required": true, "plan_id": plan_id}),
            debug(),
        ),
        CallError::Forbidden(_)
        | CallError::UnknownCommand(_)
        | CallError::NotOnSurface(_)
        | CallError::UnknownPlan(_) => Classified::new(
            Kind::Hidden,
            404,
            code::NOT_FOUND,
            json!({"error": "not found"}),
            debug(),
        ),
        CallError::Restore(_) => Classified::new(
            Kind::Failed,
            400,
            code::INVALID_TOKEN,
            json!({"error": "invalid token"}),
            debug(),
        ),
        CallError::PreconditionFailed { current } => precondition(*current),
        CallError::Handler(m) => handler(m, debug),
    }
}

/// The refusal for a record that is not at the version the caller read.
fn precondition(current: i64) -> Classified {
    let r = row(code::PRECONDITION_FAILED).expect("the table has precondition_failed");
    let message = "the record was changed since you read it";
    let mut c = Classified::new(
        Kind::Failed,
        r.status,
        r.code,
        json!({
            "error": message,
            "code": r.code,
            "current_version": current,
            "hint": r.hint,
        }),
        format!(
            "{}: {message} (it is at version {current} now)",
            code::PRECONDITION_FAILED
        ),
    );
    c.current_version = Some(current);
    c.retry_after = r.retry_after;
    c.hint = Some(r.hint);
    c
}

fn handler(m: &str, debug: impl Fn() -> String) -> Classified {
    // D74: the store's own refusal, recognised by the code word at the very start (the one rule of
    // `Fault::of_message`).
    if let Some(c) = of_storage_text(m) {
        return c;
    }
    if let Some((r, rest)) = split_code(m) {
        // D75 writes the version into the message (`precondition_failed: current_version 8`);
        // that is the same refusal as the variant.
        if r.code == code::PRECONDITION_FAILED
            && let Some(n) = rest
                .strip_prefix("current_version ")
                .and_then(|n| n.trim().parse::<i64>().ok())
        {
            return precondition(n);
        }
        let mut c = Classified::new(
            Kind::Failed,
            r.status,
            r.code,
            json!({"error": rest, "code": r.code, "hint": r.hint}),
            debug(),
        );
        c.retry_after = r.retry_after;
        c.hint = Some(r.hint);
        return c;
    }
    Classified::new(
        Kind::Failed,
        400,
        code::BAD_REQUEST,
        json!({"error": m}),
        debug(),
    )
}

/// `code: message` with a code of the table: the row and the message.
fn split_code(m: &str) -> Option<(&'static Row, &str)> {
    let (head, rest) = m.split_once(": ")?;
    row(head).map(|r| (r, rest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use obop_masking::RestoreError;

    /// One of every variant. The `match` makes this test stop compiling when a variant is added
    /// without a row of `legacy_answers`.
    fn every_variant() -> Vec<CallError> {
        let all = vec![
            CallError::UnknownCommand("party.nope".into()),
            CallError::NotOnSurface("party.erase".into()),
            CallError::Forbidden("party.erase".into()),
            CallError::Restore(RestoreError::UnknownToken("t1".into())),
            CallError::ApprovalRequired {
                plan_id: "plan-1".into(),
            },
            CallError::UnknownPlan("plan-9".into()),
            CallError::PreconditionFailed { current: 8 },
            CallError::Handler("name is required".into()),
        ];
        for e in &all {
            match e {
                CallError::UnknownCommand(_)
                | CallError::NotOnSurface(_)
                | CallError::Forbidden(_)
                | CallError::Restore(_)
                | CallError::ApprovalRequired { .. }
                | CallError::UnknownPlan(_)
                | CallError::PreconditionFailed { .. }
                | CallError::Handler(_) => {}
            }
        }
        all
    }

    /// Everything the surfaces answered before the table existed, per variant: status, code of
    /// the class, exact body, what the CLI prints.
    #[test]
    fn the_errors_that_always_existed_answer_exactly_as_before() {
        let hidden = |e: CallError, cli: &str| {
            let c = classify(&e);
            assert_eq!(c.kind, Kind::Hidden, "{e:?}");
            assert_eq!(c.status, 404);
            assert_eq!(c.code, "not_found");
            assert_eq!(c.body, json!({"error": "not found"}));
            assert_eq!(c.cli, cli);
            assert_eq!((c.current_version, c.retry_after), (None, None));
            assert_eq!((c.hint, c.applied, c.exit), (None, None, 1));
        };
        hidden(
            CallError::UnknownCommand("party.nope".into()),
            "UnknownCommand(\"party.nope\")",
        );
        hidden(
            CallError::NotOnSurface("party.erase".into()),
            "NotOnSurface(\"party.erase\")",
        );
        hidden(
            CallError::Forbidden("party.erase".into()),
            "Forbidden(\"party.erase\")",
        );
        hidden(
            CallError::UnknownPlan("plan-9".into()),
            "UnknownPlan(\"plan-9\")",
        );

        let c = classify(&CallError::Restore(RestoreError::UnknownToken("t1".into())));
        assert_eq!(c.kind, Kind::Failed);
        assert_eq!((c.status, c.code), (400, "invalid_token"));
        assert_eq!(c.body, json!({"error": "invalid token"}));
        assert_eq!(
            c.cli,
            format!(
                "{:?}",
                CallError::Restore(RestoreError::UnknownToken("t1".into()))
            )
        );

        let c = classify(&CallError::ApprovalRequired {
            plan_id: "plan-1".into(),
        });
        assert_eq!(
            c.kind,
            Kind::Proposed {
                plan_id: "plan-1".into()
            }
        );
        assert_eq!((c.status, c.code), (202, "approval_required"));
        assert_eq!(
            c.body,
            json!({"approval_required": true, "plan_id": "plan-1"})
        );
        assert_eq!(c.cli, "ApprovalRequired { plan_id: \"plan-1\" }");

        // A handler string without a known code: 400, `{"error": m}` and nothing else, and the
        // CLI line the guide documents (`error: Handler("no lead nope")`).
        for m in [
            "name is required",
            "no lead nope",
            "stage cannot change from \"won\" to \"new\"",
            "",
            "unknown_code: the prefix is not a row",
            "Denied: capital letters are another word",
            "denied:no space after the colon",
            " denied: not at the start",
            "see use_search: it is not at the start",
            "denied",
            // The code word of a storage fault counts only at the very start (the rule of
            // `Fault::of_message`): text a caller chose and that is echoed back is their input.
            "unknown option \"a: storage_busy: b\" (use filter, order)",
            "could not save: storage_unavailable: cannot reach",
            "storage_timeout",
            "Storage_busy: capital letters are another word",
            " storage_busy: not at the start",
        ] {
            let e = CallError::Handler(m.into());
            let c = classify(&e);
            assert_eq!(c.kind, Kind::Failed, "{m:?}");
            assert_eq!((c.status, c.code), (400, "bad_request"), "{m:?}");
            assert_eq!(c.body, json!({"error": m}), "{m:?}");
            assert_eq!(c.cli, format!("Handler({m:?})"), "{m:?}");
            assert_eq!((c.current_version, c.retry_after), (None, None));
            assert_eq!((c.hint, c.applied, c.exit), (None, None, 1), "{m:?}");
        }
    }

    #[test]
    fn every_variant_has_an_answer() {
        for e in every_variant() {
            let c = classify(&e);
            assert!(!c.cli.is_empty(), "{e:?}");
            assert!(c.body.is_object(), "{e:?}");
            assert!((200..600).contains(&c.status), "{e:?}");
        }
    }

    /// The table is data: every row, mapped as the row says when a handler string carries its
    /// prefix. The storage rows add `applied` and name the code word in `error` (D74); the test
    /// of their own shape is `a_storage_word_answers_503_with_what_the_text_allows_to_say`.
    #[test]
    fn each_row_maps_as_the_table_says() {
        for r in TABLE {
            let e = CallError::Handler(coded(r.code, "the reason, with: a colon"));
            let c = classify(&e);
            assert_eq!(c.kind, Kind::Failed, "{}", r.code);
            assert_eq!(c.status, r.status, "{}", r.code);
            assert_eq!(c.code, r.code);
            assert_eq!(c.retry_after, r.retry_after, "{}", r.code);
            assert_eq!(c.current_version, None);
            if r.origin == "D74" {
                assert_eq!(c.body["error"], r.code);
                assert_eq!(c.body["code"], r.code);
                assert_eq!(c.body["hint"], json!(c.hint), "{}", r.code);
                assert!(c.applied.is_some(), "{}", r.code);
                continue;
            }
            // The prefix is the code; the body carries the rest.
            assert_eq!(
                c.body,
                json!({"error": "the reason, with: a colon", "code": r.code, "hint": r.hint}),
                "{}",
                r.code
            );
            assert_eq!(c.hint, Some(r.hint), "{}", r.code);
            assert_eq!((c.applied, c.exit), (None, 1), "{}", r.code);
            assert_eq!(c.cli, format!("{e:?}"), "{}", r.code);
        }
    }

    #[test]
    fn the_table_says_what_the_plan_says() {
        let want: &[(&str, u16)] = &[
            ("precondition_failed", 412),
            ("immutable_record", 409),
            ("state_conflict", 409),
            ("guard_change_refused", 409),
            ("cursor_expired", 410),
            ("cursor_invalid", 400),
            ("cursor_key_too_long", 400),
            ("offset_too_large", 400),
            ("limit_required", 400),
            ("use_search", 400),
            ("denied", 403),
            ("reauth_required", 400),
            ("storage_busy", 503),
            ("storage_unavailable", 503),
            ("storage_timeout", 503),
        ];
        assert_eq!(TABLE.len(), want.len());
        for (code, status) in want {
            let r = row(code).unwrap_or_else(|| panic!("no row {code}"));
            assert_eq!(r.status, *status, "{code}");
        }
        assert!(row("nothing").is_none());
    }

    #[test]
    fn the_table_is_well_formed() {
        let mut seen = std::collections::HashSet::new();
        for r in TABLE {
            assert!(seen.insert(r.code), "{} is twice in the table", r.code);
            assert!(
                !r.code.is_empty() && r.code.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
                "{} is not snake case",
                r.code
            );
            assert!(
                matches!(r.status, 400..=499 | 503),
                "{}: {}",
                r.code,
                r.status
            );
            assert!(r.hint.ends_with('.') && !r.origin.is_empty(), "{}", r.code);
            // A 503 says when to come back; nothing else does.
            assert_eq!(r.retry_after.is_some(), r.status == 503, "{}", r.code);
            // No clash with the classes of the errors that have always existed.
            assert!(
                ![
                    code::APPROVAL_REQUIRED,
                    code::NOT_FOUND,
                    code::INVALID_TOKEN,
                    code::BAD_REQUEST
                ]
                .contains(&r.code)
            );
        }
    }

    #[test]
    fn a_precondition_failure_carries_the_current_version() {
        let c = classify(&CallError::PreconditionFailed { current: 8 });
        assert_eq!(c.kind, Kind::Failed);
        assert_eq!((c.status, c.code), (412, "precondition_failed"));
        assert_eq!(c.current_version, Some(8));
        assert_eq!(
            c.body,
            json!({
                "error": "the record was changed since you read it",
                "code": "precondition_failed",
                "current_version": 8,
                "hint": "Read the record again and retry with the new if_version.",
            })
        );
        assert_eq!(
            c.cli,
            "precondition_failed: the record was changed since you read it (it is at version 8 now)"
        );
        // The same refusal as a handler string that names the version (D75's form).
        let via_text = classify(&CallError::Handler(
            "precondition_failed: current_version 8".into(),
        ));
        assert_eq!(via_text, c);
        // Without a readable version it is still a 412, without `current_version`.
        let c = classify(&CallError::Handler(
            "precondition_failed: current_version x".into(),
        ));
        assert_eq!((c.status, c.code), (412, "precondition_failed"));
        assert_eq!(c.current_version, None);
        assert!(c.body.get("current_version").is_none());
    }

    const FAULTS: [Fault; 3] = [Fault::Busy, Fault::Unavailable, Fault::Timeout];

    /// The rows are the pool's own words and waits: one place for each, and they agree.
    #[test]
    fn the_storage_rows_are_the_faults_of_the_pool() {
        for f in FAULTS {
            let r = row(f.code()).unwrap_or_else(|| panic!("no row for {}", f.code()));
            assert_eq!((r.status, r.origin), (503, "D74"), "{}", f.code());
            assert_eq!(r.retry_after, Some(f.retry_after_secs()), "{}", f.code());
            // The words the handlers produce are the words the table reads.
            assert_eq!(Fault::of_message(&f.message("x")), Some(f));
        }
        assert_eq!(code::STORAGE_BUSY, Fault::Busy.code());
        assert_eq!(code::STORAGE_UNAVAILABLE, Fault::Unavailable.code());
        assert_eq!(code::STORAGE_TIMEOUT, Fault::Timeout.code());
        assert_eq!(TABLE.iter().filter(|r| r.origin == "D74").count(), 3);
    }

    /// What a handler string with a storage word answers: 503, the wait, `applied` from the text
    /// alone, the line and the exit status of the command line.
    #[test]
    fn a_storage_word_answers_503_with_what_the_text_allows_to_say() {
        let answer = |m: &str| classify(&CallError::Handler(m.into()));

        // A full pool and a statement that ran into its limit changed nothing.
        for (f, wait) in [(Fault::Busy, 1), (Fault::Timeout, 5)] {
            let c = answer(&f.message("every connection is in use"));
            assert_eq!(c.kind, Kind::Failed);
            assert_eq!((c.status, c.code), (503, f.code()));
            assert_eq!(
                (c.retry_after, c.applied, c.exit),
                (Some(wait), Some(Applied::None), 75)
            );
            assert_eq!(
                c.body,
                json!({"error": f.code(), "code": f.code(), "hint": c.hint, "applied": "none"})
            );
            assert!(
                c.hint.unwrap().contains("Nothing was changed"),
                "{:?}",
                c.hint
            );
            assert_eq!(c.current_version, None);
            assert_eq!(
                c.cli,
                format!(
                    "{}: every connection is in use (nothing was changed; try again in {wait} s)",
                    f.code()
                )
            );
        }

        // A connection that broke may have taken a change with it: unknown.
        let c = answer(&Fault::Unavailable.message("the database cannot be reached"));
        assert_eq!((c.status, c.code), (503, "storage_unavailable"));
        assert_eq!(
            (c.retry_after, c.applied, c.exit),
            (Some(5), Some(Applied::Unknown), 75)
        );
        assert_eq!(c.body["applied"], "unknown");
        assert_eq!(c.hint, Some(row("storage_unavailable").unwrap().hint));
        assert!(c.hint.unwrap().contains("read before you repeat a write"));
        assert_eq!(
            c.cli,
            "storage_unavailable: the database cannot be reached (a change in flight may or may \
             not have been applied: read before you repeat a write; try again in 5 s)"
        );

        // The guard's own refusal of a change after an earlier fault changed nothing, whatever
        // the first fault was.
        let c = answer(&Fault::Unavailable.message(
            "an earlier storage call of this request was refused, so this change was not made",
        ));
        assert_eq!((c.status, c.applied), (503, Some(Applied::None)));
        assert_eq!(c.body["applied"], "none");
        assert!(c.hint.unwrap().contains("Nothing was changed"));

        // No explanation after the word: the line is the word and what to do.
        let c = answer("storage_busy:");
        assert_eq!(
            c.cli,
            "storage_busy (nothing was changed; try again in 1 s)"
        );
    }

    /// The rule that turns what a request did to the store into the word of the 503.
    #[test]
    fn applied_follows_what_the_request_did_to_the_store() {
        let seen = |wrote, partial| Outcome {
            fault: None,
            wrote,
            partial,
            in_flight: false,
        };
        for f in FAULTS {
            let lost = if f == Fault::Unavailable {
                Applied::Unknown
            } else {
                Applied::None
            };
            // Nothing committed: the request may be repeated (read first after `unavailable`),
            // whatever the handler answered.
            assert_eq!(applied(f, &seen(false, false), true), Some(lost), "{f:?}");
            assert_eq!(applied(f, &seen(false, false), false), Some(lost), "{f:?}");
            // A needed step was refused: partial, even when the answer looked fine.
            assert_eq!(applied(f, &seen(true, true), false), Some(Applied::Partial));
            assert_eq!(applied(f, &seen(true, true), true), Some(Applied::Partial));
            // A change committed and the request ends in an error answer: partial.
            assert_eq!(applied(f, &seen(true, false), true), Some(Applied::Partial));
            // A change committed and only something optional was lost: its own answer stands.
            assert_eq!(applied(f, &seen(true, false), false), None, "{f:?}");
        }
    }

    /// The answer for a fault of a whole request (the layer's case, with no text to read).
    #[test]
    fn a_storage_fault_says_what_became_of_the_changes_in_every_shape() {
        for f in FAULTS {
            for a in [Applied::None, Applied::Unknown, Applied::Partial] {
                if a == Applied::Unknown && f != Fault::Unavailable {
                    continue;
                }
                let c = storage_fault(f, a, None);
                assert_eq!(
                    (c.status, c.code, c.kind.clone()),
                    (503, f.code(), Kind::Failed)
                );
                assert_eq!(c.retry_after, Some(f.retry_after_secs()));
                assert_eq!(
                    c.body,
                    json!({"error": f.code(), "code": f.code(), "hint": storage_hint(f, a),
                        "applied": a.word()})
                );
                assert_eq!(
                    c.rpc_data(),
                    json!({"hint": storage_hint(f, a), "retry_after": f.retry_after_secs(),
                        "applied": a.word()})
                );
                assert_eq!((c.applied, c.exit), (Some(a), 75));
                assert!(
                    c.cli.starts_with(f.code()) && c.cli.contains("try again in"),
                    "{}",
                    c.cli
                );
                // The hint says the truth about the changes: only `none` says nothing changed.
                let says_unchanged = c.hint.unwrap().contains("Nothing was changed");
                assert_eq!(says_unchanged, a == Applied::None, "{f:?} {a:?}");
                if a == Applied::Partial {
                    assert!(c.hint.unwrap().contains("Do not repeat it blindly"));
                    assert!(c.cli.contains("do not repeat it blindly"));
                }
            }
            // The row holds the hint of the rule for its code.
            let rule = if f == Fault::Unavailable {
                Applied::Unknown
            } else {
                Applied::None
            };
            assert_eq!(storage_hint(f, rule), row(f.code()).unwrap().hint);
        }
        // The errors that have always existed carry no data for a JSON-RPC error.
        let c = classify(&CallError::Handler("name is required".into()));
        assert_eq!(c.rpc_data(), json!({}));
    }

    #[test]
    fn coded_builds_what_classify_reads() {
        assert_eq!(
            coded(code::USE_SEARCH, "after is a search option"),
            "use_search: after is a search option"
        );
    }
}
