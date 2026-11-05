//! Two changes at once (docs/spec/branding.md, section 13, "Race"): one through the dispatcher
//! (which passes its reader) and one through the command line (which has none), on different fields.
//! Both survive, because `apply` takes the lock `brand.write` itself and merges under it. Run on the
//! memory store, on SQLite (two connections to one file, as two processes would be) and, when
//! `OBOP_TEST_POSTGRES_URL` is set, on PostgreSQL.

use super::*;
use crate::brand::{Action, LOCK_KEY, apply};
use std::path::PathBuf;
use std::thread;

const ROUNDS: usize = 30;

fn set(field: &str, value: String) -> Action {
    Action::Configure(vec![(field.to_string(), value)])
}

/// `a` plays the server (with its reader), `b` the command line at a terminal.
fn race(a: Arc<dyn Backend>, b: Arc<dyn Backend>) {
    apply(&*a, None, "ada", &set("name", "Race".into()), false).unwrap();
    let reader = Arc::new(Reader::with_bound(a.clone(), Duration::from_secs(3600)));
    assert_eq!(reader.current().brand().name(), "Race");
    let server = {
        let (a, reader) = (a.clone(), reader.clone());
        thread::spawn(move || {
            for i in 0..ROUNDS {
                apply(
                    &*a,
                    Some(&reader),
                    "ada",
                    &set("title_en", format!("A{i}")),
                    false,
                )
                .unwrap();
            }
        })
    };
    let terminal = {
        let b = b.clone();
        thread::spawn(move || {
            for i in 0..ROUNDS {
                apply(
                    &*b,
                    None,
                    "operator",
                    &set("title_sv", format!("B{i}")),
                    false,
                )
                .unwrap();
            }
        })
    };
    server.join().unwrap();
    terminal.join().unwrap();
    let row = a.get(MODEL, ROW_ID).unwrap().unwrap();
    let text = |k: &str| row.get(k).and_then(J::as_str).unwrap_or("").to_string();
    assert_eq!(text("name"), "Race");
    assert_eq!(
        text("title_en"),
        format!("A{}", ROUNDS - 1),
        "the server's last write survived"
    );
    assert_eq!(
        text("title_sv"),
        format!("B{}", ROUNDS - 1),
        "the terminal's last write survived"
    );
    // Every change that was made is in the log: one event for the name, one per round and field.
    assert_eq!(a.list("security_event").unwrap().len(), 1 + 2 * ROUNDS);
    // The server's cell followed its own writes.
    assert_eq!(
        reader.current().brand().stored_title("en"),
        format!("A{}", ROUNDS - 1)
    );
}

#[test]
fn a_change_through_the_dispatcher_and_one_through_the_command_line_both_survive_in_memory() {
    let m = obop_storage::open("memory:").unwrap();
    race(m.clone(), m);
}

struct TempDb(PathBuf);

impl TempDb {
    fn new() -> TempDb {
        let name = format!("obop-brand-race-{}.db", uuid::Uuid::now_v7().simple());
        TempDb(std::env::temp_dir().join(name))
    }
    fn url(&self) -> String {
        format!("sqlite:{}", self.0.display())
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        let dir = self.0.parent().unwrap().to_path_buf();
        let stem = self.0.file_name().unwrap().to_string_lossy().to_string();
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                if e.file_name()
                    .to_string_lossy()
                    .starts_with(&stem[..stem.len() - 3])
                {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
    }
}

#[test]
fn a_change_through_the_dispatcher_and_one_through_the_command_line_both_survive_on_sqlite() {
    let db = TempDb::new();
    let a = obop_storage::open(&db.url()).unwrap();
    let b = obop_storage::open(&db.url()).unwrap();
    race(a, b);
}

#[test]
fn a_change_through_the_dispatcher_and_one_through_the_command_line_both_survive_on_postgres() {
    let Some(db) = obop_storage::testing::TestDb::new() else {
        // Counted as passed by the harness, so say that it did not run.
        eprintln!(
            "SKIPPED (not run): OBOP_TEST_POSTGRES_URL is not set, so the PostgreSQL leg of the race test did not run"
        );
        return;
    };
    let a = obop_storage::open(db.url()).unwrap();
    let b = obop_storage::open(db.url()).unwrap();
    race(a, b);
}

/// Every `get` of the row and every atomic batch of `apply` is made by a thread that holds
/// `brand.write`, whoever calls it: the read, the merge and the write are under the one lock.
fn the_lock_is_held_at_every_read_and_write(spy: Arc<Spy>) {
    for action in [
        set("name", "Lock".into()),
        set("title_en", "One".into()),
        Action::Clear,
    ] {
        apply(&*spy, None, "ada", &action, false).unwrap();
    }
    let held = vec![vec![LOCK_KEY.to_string()]; 3];
    assert_eq!(spy.locks_at_reads(MODEL), held, "the reads of the row");
    assert_eq!(spy.locks_at_batches(), held, "the batches");
    assert_eq!(
        spy.held_by_this_thread(),
        Vec::<String>::new(),
        "and let go"
    );
}

#[test]
fn apply_reads_merges_and_writes_under_the_lock_in_memory() {
    the_lock_is_held_at_every_read_and_write(Spy::memory());
}

/// Two changes at once, on different fields, with the first stopped just before it writes (it has
/// read the row and holds the lock): the second must be waiting for the lock, not merging an old
/// row. A lock that is not taken, or let go before the write, loses the first change or the second
/// every time, not now and then.
fn the_second_change_waits_for_the_first_to_be_written(spy: Arc<Spy>) {
    apply(&*spy, None, "ada", &set("name", "Race".into()), false).unwrap();
    let asked = spy.lock_attempts(LOCK_KEY);
    spy.park_next_apply();
    let first = {
        let spy = spy.clone();
        thread::spawn(move || apply(&*spy, None, "ada", &set("title_en", "A".into()), false))
    };
    spy.wait_parked(1);
    let second = {
        let spy = spy.clone();
        thread::spawn(move || apply(&*spy, None, "operator", &set("title_sv", "B".into()), false))
    };
    // The first holds the lock and is in the store; the second has asked for the lock. If it got
    // it, it has already read, merged and written, and the first will overwrite it.
    spy.wait_lock_attempts(LOCK_KEY, asked + 2);
    spy.release();
    first.join().unwrap().unwrap();
    second.join().unwrap().unwrap();
    let row = spy.get(MODEL, ROW_ID).unwrap().unwrap();
    let text = |k: &str| row.get(k).and_then(J::as_str).unwrap_or("").to_string();
    assert_eq!(text("name"), "Race");
    assert_eq!(text("title_en"), "A", "the first change survived");
    assert_eq!(text("title_sv"), "B", "the second change survived");
}

#[test]
fn the_second_change_waits_for_the_first_to_be_written_in_memory() {
    the_second_change_waits_for_the_first_to_be_written(Spy::memory());
}

#[test]
fn the_second_change_waits_for_the_first_to_be_written_on_sqlite() {
    let db = TempDb::new();
    the_second_change_waits_for_the_first_to_be_written(Spy::over(
        obop_storage::open(&db.url()).unwrap(),
    ));
}

#[test]
fn apply_reads_merges_and_writes_under_the_lock_on_sqlite() {
    let db = TempDb::new();
    the_lock_is_held_at_every_read_and_write(Spy::over(obop_storage::open(&db.url()).unwrap()));
}
