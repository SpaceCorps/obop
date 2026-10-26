//! The append-only security event log (`security_event`, D53, B7): one writer for everything
//! that records who did what to an account or to the security settings. Never pass a code, a
//! password, a token or a secret as `detail`.

use obop_storage::{Backend, Fields};
use serde_json::Value as J;

/// Table of the log; the `security_event` model of the `platform` module reads it.
pub const EVENT_TABLE: &str = "security_event";

/// `YYYY-MM-DDTHH:MM:SSZ` for a Unix time.
pub fn rfc3339(secs: u64) -> String {
    let (days, rem) = ((secs / 86400) as i64, secs % 86400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn hex6() -> String {
    let mut b = [0u8; 6];
    getrandom_fill(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn getrandom_fill(b: &mut [u8]) {
    // uuid is already a dependency and draws from the operating system's randomness.
    let u = uuid::Uuid::now_v7();
    let bytes = u.as_bytes();
    let n = b.len().min(bytes.len());
    b[..n].copy_from_slice(&bytes[bytes.len() - n..]);
}

/// Append one event at time `at` (Unix seconds). A failure to write is reported on stderr and
/// never stops the action being logged.
pub fn record_event(
    backend: &dyn Backend,
    at: u64,
    user: &str,
    kind: &str,
    actor: &str,
    detail: &str,
) {
    if let Err(e) = try_record_event(backend, at, user, kind, actor, detail) {
        eprintln!("could not write security event {kind}: {e}");
    }
}

/// As [`record_event`], and a failure to write is returned, for the caller whose entry is the only
/// trace of something (it then says so itself, with what it knows).
pub fn try_record_event(
    backend: &dyn Backend,
    at: u64,
    user: &str,
    kind: &str,
    actor: &str,
    detail: &str,
) -> Result<(), String> {
    let mut f = Fields::new();
    f.insert("user".into(), J::from(user));
    f.insert("kind".into(), J::from(kind));
    f.insert("actor".into(), J::from(actor));
    f.insert("detail".into(), J::from(detail));
    f.insert("at".into(), J::from(rfc3339(at)));
    // Sortable by time, unique by suffix.
    let id = format!("{at:012}-{}", hex6());
    // The entry is about a change that has usually committed already: a store that is busy for a
    // moment (D74) is waited out a little, four tries in all. Also when this request was refused
    // before: the guard lets the log through (an entry of a change that committed must not be
    // lost), and that is exactly when the pool is full.
    obop_storage::fault::patiently(4, std::time::Duration::from_millis(150), || {
        backend.insert(EVENT_TABLE, &id, &f)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_event_is_not_lost_to_a_store_that_is_busy_for_a_moment() {
        use obop_storage::testing::Flaky;
        // D74: the entry about a change that committed meets a full pool twice; it is written on
        // the third try, not dropped.
        let (flaky, db) = Flaky::memory();
        flaky.refuse_next(2);
        record_event(&*db, 30, "ada", "login", "ada", "note");
        assert_eq!(flaky.refused(), 2);
        assert_eq!(db.list(EVENT_TABLE).unwrap().len(), 1);
    }

    #[test]
    fn an_entry_that_stays_refused_is_reported_to_its_caller_which_may_be_its_only_trace() {
        use obop_storage::testing::Flaky;
        let (flaky, db) = Flaky::memory();
        flaky.refuse_next(10);
        let r = try_record_event(&*db, 30, "ada", "plan_lost", "root", "the plan p1");
        assert!(
            matches!(&r, Err(m) if m.starts_with("storage_busy")),
            "{r:?}"
        );
        assert_eq!(flaky.refused(), 4, "four tries, then it is reported");
        flaky.allow_models();
        assert!(try_record_event(&*db, 31, "ada", "plan_lost", "root", "the plan p1").is_ok());
    }

    #[test]
    fn an_event_after_a_refused_call_of_the_same_request_is_also_tried_four_times() {
        use obop_storage::fault;
        use obop_storage::testing::{Flaky, Mode};
        // The entry of a change that committed is written right after the call that was refused.
        // The log is exempt from the guard, so waiting is useful there too.
        let (flaky, db) = Flaky::memory();
        fault::begin_request(Default::default());
        flaky.refuse_model("x", Mode::Reads);
        assert!(
            db.get("x", "1").is_err(),
            "a refused read: the fault is noted"
        );
        flaky.allow_models();
        flaky.refuse_next(2);
        record_event(
            &*db,
            31,
            "ada",
            "user_offboard",
            "root",
            "INCOMPLETE: a step",
        );
        fault::end_request();
        assert_eq!(
            db.list(EVENT_TABLE).unwrap().len(),
            1,
            "written on the third try"
        );
    }

    #[test]
    fn times_format_as_rfc3339_and_events_are_appended_sorted() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(1_800_000_000), "2027-01-15T08:00:00Z");
        let b = obop_storage::open("memory:").unwrap();
        record_event(&*b, 20, "ada", "x", "root", "note");
        record_event(&*b, 10, "ada", "y", "root", "note");
        let rows = b.list(EVENT_TABLE).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].0.starts_with("000000000020-"));
        assert_eq!(rows[1].1["kind"], "y");
        assert_eq!(rows[1].1["at"], "1970-01-01T00:00:10Z");
    }
}
