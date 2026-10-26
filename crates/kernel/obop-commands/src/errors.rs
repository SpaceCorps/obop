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
//! row, if it has one. A handler string without a known code is what it always was: status 400
//! and the body `{"error": <string>}`, with no `code` (those bodies are part of the documented API
//! and do not change).
//!
//! # Not yet in the table
//!
//! D74 (the PostgreSQL connection pool) turns the words `storage_busy`, `storage_unavailable` and
//! `storage_timeout` into 503 with `Retry-After`. It is not merged, so those words are ordinary
//! handler text here and answer 400 (test `storage_words_stay_400_until_the_pool_merges`). When it
//! merges it adds three rows at the marked place in `TABLE` and deletes that test.

use crate::CallError;
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

/// The table: one row per code. Producers of the codes below do not all exist yet (D75, D77, D78
/// and step 2 are being built); the rows are here first so none of them edits a surface.
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
    // D74 HOOK (the connection pool, not merged yet): add the rows below, as they are, when it
    // merges. Until then the three words are ordinary handler text and answer 400.
    //
    //   Row { code: "storage_busy",        status: 503, retry_after: Some(1), origin: "D74", hint: "Try again after the time in Retry-After." },
    //   Row { code: "storage_unavailable", status: 503, retry_after: Some(5), origin: "D74", hint: "Try again after the time in Retry-After." },
    //   Row { code: "storage_timeout",     status: 503, retry_after: Some(5), origin: "D74", hint: "Try again after the time in Retry-After." },
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
}

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
        }
    }
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
    c
}

fn handler(m: &str, debug: impl Fn() -> String) -> Classified {
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
        ] {
            let e = CallError::Handler(m.into());
            let c = classify(&e);
            assert_eq!(c.kind, Kind::Failed, "{m:?}");
            assert_eq!((c.status, c.code), (400, "bad_request"), "{m:?}");
            assert_eq!(c.body, json!({"error": m}), "{m:?}");
            assert_eq!(c.cli, format!("Handler({m:?})"), "{m:?}");
            assert_eq!((c.current_version, c.retry_after), (None, None));
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
    /// prefix.
    #[test]
    fn each_row_maps_as_the_table_says() {
        for r in TABLE {
            let e = CallError::Handler(coded(r.code, "the reason, with: a colon"));
            let c = classify(&e);
            assert_eq!(c.kind, Kind::Failed, "{}", r.code);
            assert_eq!(c.status, r.status, "{}", r.code);
            assert_eq!(c.code, r.code);
            assert_eq!(c.retry_after, r.retry_after, "{}", r.code);
            // The prefix is the code; the body carries the rest.
            assert_eq!(
                c.body,
                json!({"error": "the reason, with: a colon", "code": r.code, "hint": r.hint}),
                "{}",
                r.code
            );
            assert_eq!(c.cli, format!("{e:?}"), "{}", r.code);
            assert_eq!(c.current_version, None);
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

    /// D74 is not merged: its three words are handler text and answer 400. When it merges it adds
    /// the three rows (see the marked place in `TABLE`) and this test goes.
    #[test]
    fn storage_words_stay_400_until_the_pool_merges() {
        for w in ["storage_busy", "storage_unavailable", "storage_timeout"] {
            assert!(row(w).is_none(), "{w} has a row: delete this test");
            let m = format!("{w}: every connection is in use");
            let c = classify(&CallError::Handler(m.clone()));
            assert_eq!((c.status, c.code), (400, "bad_request"), "{w}");
            assert_eq!(c.body, json!({"error": m}));
            assert_eq!(c.retry_after, None);
        }
    }

    #[test]
    fn coded_builds_what_classify_reads() {
        assert_eq!(
            coded(code::USE_SEARCH, "after is a search option"),
            "use_search: after is a search option"
        );
    }
}
