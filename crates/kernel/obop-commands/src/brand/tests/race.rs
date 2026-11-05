//! Two changes at once (docs/spec/branding.md, section 13, "Race"): one through the dispatcher
//! (which passes its reader) and one through the command line (which has none), on different fields.
//! Both survive, because `apply` takes the lock `brand.write` itself and merges under it. Run on the
//! memory store, on SQLite (two connections to one file, as two processes would be) and, when
//! `OBOP_TEST_POSTGRES_URL` is set, on PostgreSQL.

use super::*;
use crate::brand::{Action, apply};
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
        return;
    };
    let a = obop_storage::open(db.url()).unwrap();
    let b = obop_storage::open(db.url()).unwrap();
    race(a, b);
}
