//! How REST answers a failed call: the shapes of the errors that have always existed (they are
//! documented API) and the rows of `obop_commands::errors::TABLE`.

use super::*;
use http_body_util::BodyExt;
use obop_commands::RestoreError;

async fn read(r: Response) -> (StatusCode, HeaderMap, J) {
    let (parts, body) = r.into_parts();
    let bytes = body.collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(J::Null);
    (parts.status, parts.headers, json)
}

async fn answer(e: CallError) -> (StatusCode, HeaderMap, J) {
    read(respond(Err(e))).await
}

#[tokio::test]
async fn the_errors_that_always_existed_keep_their_status_and_body() {
    let (s, h, b) = answer(CallError::Handler("name is required".into())).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b, json!({"error": "name is required"}));
    assert!(h.get("etag").is_none() && h.get("retry-after").is_none());

    for e in [
        CallError::Forbidden("party.erase".into()),
        CallError::UnknownCommand("party.nope".into()),
        CallError::NotOnSurface("party.erase".into()),
        CallError::UnknownPlan("plan-9".into()),
    ] {
        let (s, _, b) = answer(e).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        assert_eq!(b, json!({"error": "not found"}));
    }

    let (s, _, b) = answer(CallError::Restore(RestoreError::UnknownToken("t1".into()))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b, json!({"error": "invalid token"}));

    let (s, _, b) = answer(CallError::ApprovalRequired {
        plan_id: "plan-1".into(),
    })
    .await;
    assert_eq!(s, StatusCode::ACCEPTED);
    assert_eq!(b, json!({"approval_required": true, "plan_id": "plan-1"}));

    // A success is untouched.
    let ok = obop_commands::Response {
        value: Value::Null,
        denied: vec!["party.tax_id".into()],
    };
    let (s, _, b) = read(respond(Ok(ok))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b, json!({"value": null, "denied": ["party.tax_id"]}));
}

#[tokio::test]
async fn a_handler_string_with_a_code_of_the_table_answers_as_its_row() {
    for r in errors::TABLE {
        let (s, h, b) = answer(CallError::Handler(errors::coded(r.code, "the reason"))).await;
        // The storage rows (D74) are not read from text: a fault the pool noted makes them, and
        // `a_storage_word_is_a_503_with_a_wait_and_what_became_of_the_changes` has them.
        if r.origin == "D74" {
            assert_eq!(s, StatusCode::BAD_REQUEST, "{}", r.code);
            assert!(h.get("retry-after").is_none() && b.get("code").is_none());
            continue;
        }
        assert_eq!(s.as_u16(), r.status, "{}", r.code);
        assert_eq!(h.get("retry-after").is_some(), r.retry_after.is_some());
        assert_eq!(
            b,
            json!({"error": "the reason", "code": r.code, "hint": r.hint}),
            "{}",
            r.code
        );
    }
    // The three that matter to a client: the status differs from 400.
    for (code, status) in [
        ("cursor_expired", 410),
        ("immutable_record", 409),
        ("denied", 403),
    ] {
        let (s, _, _) = answer(CallError::Handler(format!("{code}: nope"))).await;
        assert_eq!(s.as_u16(), status, "{code}");
    }
}

#[tokio::test]
async fn a_failed_precondition_is_412_with_the_current_version_in_body_and_etag() {
    let (s, h, b) = answer(CallError::PreconditionFailed { current: 8 }).await;
    assert_eq!(s, StatusCode::PRECONDITION_FAILED);
    assert_eq!(h.get("etag").unwrap(), "\"8\"");
    assert!(h.get("retry-after").is_none());
    assert_eq!(b["code"], "precondition_failed");
    assert_eq!(b["current_version"], 8);
    assert!(b["error"].as_str().is_some_and(|m| !m.is_empty()));
    assert!(b["hint"].as_str().is_some_and(|m| !m.is_empty()));
}

#[tokio::test]
async fn retry_after_is_sent_when_the_classification_has_one() {
    let mut c = errors::classify(&CallError::Handler("x".into()));
    c.status = 503;
    c.retry_after = Some(5);
    let (s, h, _) = read(classified(c)).await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(h.get("retry-after").unwrap(), "5");
    assert!(
        h.get("x-obop-applied").is_none(),
        "only a storage fault says it"
    );
}

/// D74 through the seam: a handler error that is the store's refusal is a 503 with the seconds to
/// wait, the word of what became of the changes in the body and in `X-Obop-Applied`, and never a
/// 400 the client would not repeat.
#[tokio::test]
async fn a_storage_word_is_a_503_with_a_wait_and_what_became_of_the_changes() {
    use obop_storage::fault::{self, Fault};
    for (f, wait, applied) in [
        (Fault::Busy, "1", "none"),
        (Fault::Timeout, "5", "none"),
        (Fault::Unavailable, "5", "unknown"),
    ] {
        // The pool noted the refusal on the thread of the request.
        fault::clear();
        fault::note(f);
        let (s, h, b) = answer(CallError::Handler(f.message("every connection is in use"))).await;
        assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE, "{f:?}");
        assert_eq!(h["retry-after"], wait, "{f:?}");
        assert_eq!(h["x-obop-applied"], applied, "{f:?}");
        assert!(h.get("etag").is_none());
        assert_eq!(b["error"], f.code());
        assert_eq!(b["code"], f.code());
        assert_eq!(b["applied"], applied);
        assert!(b["hint"].as_str().is_some_and(|m| !m.is_empty()), "{f:?}");
        assert_eq!(
            b.as_object().unwrap().len(),
            4,
            "error, code, hint, applied: {b}"
        );
    }
    // The answer rests on the fault, not on the words: a refusal wrapped in the text of the step
    // that met it is the store's too, and a change that had committed makes it a partial one.
    fault::clear();
    fault::note(Fault::Busy);
    let (s, h, _) = answer(CallError::Handler(
        "lead.win: effect 1 failed: storage_busy: x".into(),
    ))
    .await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(h["x-obop-applied"], "none");
    fault::clear();
    // Without a fault noted for the request the words are the caller's own text, at the start of
    // it too (an id, a name, a key echoed first): an ordinary 400, never a 503 to retry.
    for text in [
        "unknown option \"a: storage_busy: b\" (use filter, order)",
        "storage_busy: a name the caller chose is not a credential",
        "storage_unavailable: x",
        "storage_timeout: x",
    ] {
        let (s, h, b) = answer(CallError::Handler(text.into())).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{text}");
        assert!(h.get("retry-after").is_none() && h.get("x-obop-applied").is_none());
        assert_eq!(b, json!({"error": text}), "{text}");
    }
}

/// The fallbacks that hold a bare error text (a session lock, the control plane, two-factor)
/// answer a storage fault in the same words as the call does.
#[tokio::test]
async fn a_bare_error_text_with_a_storage_word_answers_as_a_call_does() {
    use obop_storage::fault::{self, Fault};
    fault::clear();
    fault::note(Fault::Timeout);
    let via_call = answer(CallError::Handler(Fault::Timeout.message("too slow"))).await;
    fault::clear();
    let (s, h, b) = read(unavailable(&Fault::Timeout.message("too slow"))).await;
    assert_eq!(
        (s, &h["retry-after"], &h["x-obop-applied"]),
        (
            via_call.0,
            &via_call.1["retry-after"],
            &via_call.1["x-obop-applied"]
        )
    );
    assert_eq!(b, via_call.2);
    // A lock that was not granted in time is no storage fault: it keeps its own words.
    let (s, h, b) = read(unavailable("could not take the lock")).await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(h["retry-after"], "1");
    assert!(h.get("x-obop-applied").is_none());
    assert_eq!(b["code"], "unavailable");
}
