//! The PostgreSQL side of the change history (D75, work package 3) against a real server. Each
//! test has a database of its own (`TestDb`) and returns early, saying so, when
//! `OBOP_TEST_POSTGRES_URL` is not set. The contract every store shares is in `sys_tests`; this
//! file has what only PostgreSQL has: the schema part, partitions, the guard, the feed horizon,
//! restore, and what happens when the connection or the commit fails.

use super::*;
use crate::history::{ChangeCursor, ChangeQuery, HistoryQuery};
use crate::pg::Pg;
use crate::pg_hist::{FUNCTIONS, RELATIONS};
use crate::pgpool::PgPoolConfig;
use crate::sysfields::{self, ChangeRow, Clock, fmt_micros, parse_micros};
use crate::testing::TestDb;
use serde_json::json;
use std::sync::atomic::{AtomicI64, Ordering::SeqCst};

fn f(v: J) -> Fields {
    v.as_object().cloned().expect("an object")
}

fn ins(m: &str, id: &str, v: J) -> Op {
    Op::Insert {
        model: m.into(),
        id: id.into(),
        fields: f(v),
    }
}

fn merge(m: &str, id: &str, v: J) -> Op {
    Op::Merge {
        model: m.into(),
        id: id.into(),
        patch: f(v),
    }
}

fn one() -> PgPoolConfig {
    PgPoolConfig::one_shot()
}

fn pool(size: usize) -> PgPoolConfig {
    PgPoolConfig {
        size,
        ..PgPoolConfig::one_shot()
    }
}

fn instant(text: &str) -> i64 {
    parse_micros(text).unwrap_or_else(|| panic!("{text} is not a canonical instant"))
}

/// A tracked PostgreSQL store in a database of its own, with a clock the test moves (or the
/// database's own, for the tests of concurrency).
struct Px {
    b: Arc<Tracked>,
    now: Arc<AtomicI64>,
    db: TestDb,
}

impl Px {
    /// The fake clock starts at 2026-10-26 (a real month, inside the years partitions are made for).
    const T0: i64 = 1_793_000_000_000_000;

    fn new() -> Option<Px> {
        Some(Self::on(TestDb::new()?, 3, true))
    }

    fn real_clock(size: usize) -> Option<Px> {
        Some(Self::on(TestDb::new()?, size, false))
    }

    fn on(db: TestDb, size: usize, fake: bool) -> Px {
        let now = Arc::new(AtomicI64::new(Self::T0));
        let source = now.clone();
        let mut pg = Pg::connect_with(db.url(), pool(size)).expect("a PostgreSQL store");
        if fake {
            pg = pg.with_clock(Clock::from_source(move || source.load(SeqCst)));
        }
        let b = Tracked::new(Arc::new(pg));
        b.track(
            "deal",
            Track {
                history: true,
                fields: ["name", "stage", "n"]
                    .into_iter()
                    .map(|k| (k.to_string(), Treat::Plain))
                    .collect(),
                ..Track::default()
            },
        );
        // What the command layer does when it registers a model that keeps history; the tests of
        // the first write that makes the part itself do not use this fixture.
        b.ensure_history_schema().expect("the history part");
        Px { b, now, db }
    }

    fn set_clock(&self, text: &str) {
        self.now.store(instant(text), SeqCst);
    }

    fn by(&self, who: &str, ops: &[Op]) -> std::result::Result<(), ApplyError> {
        self.b
            .apply_ctx(&WriteCtx::new(who, "rest", "deal.update"), ops)
    }

    fn ok(&self, who: &str, ops: &[Op]) {
        self.by(who, ops).unwrap_or_else(|e| panic!("{e}"));
    }

    fn sql(&self) -> postgres::Client {
        self.db.connect()
    }

    /// `(seq, partition, model, record_id, op, row_version)` of every entry, by number.
    fn placed(&self) -> Vec<(i64, String, String, String, String, i64)> {
        self.sql()
            .query(
                "SELECT seq, tableoid::regclass::text, model, record_id, op, row_version
                   FROM obop_record_change ORDER BY seq",
                &[],
            )
            .unwrap()
            .iter()
            .map(|r| (r.get(0), r.get(1), r.get(2), r.get(3), r.get(4), r.get(5)))
            .collect()
    }

    fn partitions(&self) -> Vec<String> {
        self.sql()
            .query(
                "SELECT c.relname::text FROM pg_inherits i JOIN pg_class c ON c.oid = i.inhrelid
                  WHERE i.inhparent = 'obop_record_change'::regclass ORDER BY 1",
                &[],
            )
            .unwrap()
            .iter()
            .map(|r| r.get(0))
            .collect()
    }

    /// One page of the feed as the store answers it right now.
    fn feed_raw(&self, after: Option<&ChangeCursor>, limit: usize) -> crate::history::ChangePage {
        self.b
            .changes(&ChangeQuery {
                after: after.cloned(),
                limit,
                ..ChangeQuery::default()
            })
            .unwrap()
    }

    /// One page of the feed, asked again until nothing is held back: the feed waits for every
    /// transaction that is open on the server, also those of the tests of other databases.
    fn feed(&self, after: Option<&ChangeCursor>, limit: usize) -> crate::history::ChangePage {
        for _ in 0..400 {
            let page = self.feed_raw(after, limit);
            if page.horizon_lag_ms.is_none() {
                return page;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        panic!("the feed stayed held back");
    }

    /// Wait until the statistics say nothing is held back (other databases' transactions on the
    /// server count too).
    fn settled(&self) {
        for _ in 0..400 {
            if self.b.history_stats().unwrap().horizon_lag_ms == 0 {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        panic!("something stayed held back");
    }

    fn tick(&self, us: i64) {
        self.now.fetch_add(us, SeqCst);
    }
}

// ---- the schema part --------------------------------------------------------------------------------

/// The record store and the history part, made by the owner as `Ddl::IfMissing` would.
fn install_history(c: &mut postgres::Client) {
    crate::schema::with_lock(c, |c| {
        crate::schema::ensure_parts(c, crate::schema::installable(), Ddl::IfMissing)
    })
    .unwrap();
}

/// A database as an older version left it: the record store, and no change history.
fn base_only(c: &mut postgres::Client) {
    crate::schema::with_lock(c, |c| {
        crate::schema::ensure_parts(c, crate::schema::parts(), Ddl::IfMissing)
    })
    .unwrap();
    assert!(!(crate::pg_hist::PART.check)(c).unwrap());
}

#[test]
fn the_part_is_what_its_check_says_and_everything_it_lists_is_made() {
    let Some(db) = TestDb::new() else {
        return;
    };
    let mut c = db.connect();
    install_history(&mut c);
    assert!((crate::pg_hist::PART.check)(&mut c).unwrap());
    // Every relation the check looks for is the kind of object it should be.
    let kind = |c: &mut postgres::Client, name: &str| -> String {
        c.query_one(
            "SELECT relkind::text FROM pg_class WHERE oid = to_regclass($1)",
            &[&name],
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"))
        .get(0)
    };
    assert_eq!(kind(&mut c, "obop_record_change_seq"), "S");
    assert_eq!(kind(&mut c, "obop_record_change"), "p");
    assert_eq!(kind(&mut c, "obop_record_change_default"), "r");
    for index in RELATIONS.iter().filter(|r| {
        r.contains("_pkey") || r.ends_with("_rec") || r.ends_with("_feed") || r.ends_with("_sealed")
    }) {
        assert!(
            ["i", "I"].contains(&kind(&mut c, index).as_str()),
            "{index}"
        );
    }
    // And nothing the SQL makes is missing from the lists.
    let names: Vec<String> = c
        .query(
            "SELECT proname::text FROM pg_proc WHERE pronamespace = 'public'::regnamespace
              AND proname LIKE 'obop_history_%' ORDER BY 1",
            &[],
        )
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    let mut listed: Vec<String> = FUNCTIONS
        .iter()
        .map(|s| s.split('(').next().unwrap().to_string())
        .collect();
    listed.sort();
    assert_eq!(names, listed);
    let triggers: Vec<(String, String)> = c
        .query(
            "SELECT tgname::text, tgrelid::regclass::text FROM pg_trigger
              WHERE NOT tgisinternal AND tgname LIKE 'obop_record_change%' ORDER BY 1, 2",
            &[],
        )
        .unwrap()
        .iter()
        .map(|r| (r.get(0), r.get(1)))
        .collect();
    let want = |a: &str, b: &str| (a.to_string(), b.to_string());
    assert_eq!(
        triggers,
        [
            want("obop_record_change_ro", "obop_record_change"),
            want("obop_record_change_ro", "obop_record_change_default"),
            want("obop_record_change_ro_t", "obop_record_change"),
            want("obop_record_change_ro_t", "obop_record_change_default"),
        ],
        "the row guard is cloned to the partition by the server; the truncate guard is made for each"
    );
}

#[test]
fn a_missing_object_of_the_part_is_a_warning_that_does_not_stop_the_process_and_is_put_back() {
    let Some(db) = TestDb::new() else {
        return;
    };
    let mut c = db.connect();
    install_history(&mut c);
    for broken in [
        "DROP INDEX obop_record_change_feed",
        "DROP INDEX obop_record_change_sealed",
        "DROP INDEX obop_record_change_rec",
        "DROP TRIGGER obop_record_change_ro ON obop_record_change",
        "DROP TRIGGER obop_record_change_ro_t ON obop_record_change",
        "DROP TRIGGER obop_record_change_ro_t ON obop_record_change_default",
        "DROP FUNCTION obop_history_ensure_partition(date)",
        "DROP FUNCTION obop_history_erase(text, text, text[], text, jsonb, bigint, timestamptz)",
        "DROP FUNCTION obop_history_drop_partition(date, interval)",
        "DROP FUNCTION obop_history_guard() CASCADE",
        "DROP TABLE obop_record_change",
    ] {
        c.batch_execute(broken)
            .unwrap_or_else(|e| panic!("{broken}: {e}"));
        assert!(!(crate::pg_hist::PART.check)(&mut c).unwrap(), "{broken}");
        let found = crate::schema::inspect(&mut c).unwrap();
        assert_eq!(found.len(), 1, "{broken}: {found:?}");
        assert_eq!(
            (found[0].code, found[0].object.as_str(), found[0].error),
            ("schema_part_missing", "history", false),
            "{broken}: a part that is not needed to start is a warning"
        );
        // Without the right to create it the process still starts...
        let b = open_options(
            db.url(),
            &OpenOptions {
                pool: Some(one()),
                ddl: Ddl::Never,
            },
        )
        .unwrap_or_else(|e| panic!("{broken}: {e}"));
        // ... and the feature that needs it says so.
        let e = b.ensure_history_schema().unwrap_err();
        assert!(e.contains("missing: history"), "{broken}: {e}");
        drop(b);
        // The start of a process that may create does not touch it (nobody asked for the history)...
        let b = open_with(db.url(), Some(one())).unwrap();
        assert!(!(crate::pg_hist::PART.check)(&mut c).unwrap(), "{broken}");
        // ... and the question of the feature puts it back, leaving the rest alone.
        b.ensure_history_schema().unwrap();
        drop(b);
        assert!(
            crate::schema::inspect(&mut c).unwrap().is_empty(),
            "{broken}"
        );
        assert!((crate::pg_hist::PART.check)(&mut c).unwrap(), "{broken}");
    }
}

#[test]
fn processes_that_need_the_part_together_make_it_once() {
    let Some(db) = TestDb::new() else {
        return;
    };
    let barrier = std::sync::Barrier::new(6);
    std::thread::scope(|s| {
        let hs: Vec<_> = (0..6)
            .map(|i| {
                let (db, barrier) = (&db, &barrier);
                s.spawn(move || {
                    barrier.wait();
                    let pg = Pg::connect_with(db.url(), one())?;
                    // Some ask the strict question of the feature, the others write the first
                    // entry, which makes the part itself: both race the others.
                    if i % 2 == 0 {
                        pg.ensure_history_schema()?;
                    } else {
                        let b = Tracked::new(Arc::new(pg));
                        b.track("deal", history_of(&["name"]));
                        b.apply_ctx(
                            &WriteCtx::new("ann", "rest", "deal.create"),
                            &[ins("deal", &format!("d{i}"), json!({"name": "A"}))],
                        )
                        .map_err(|e| e.to_string())?;
                    }
                    Ok::<(), String>(())
                })
            })
            .collect();
        for h in hs {
            h.join().unwrap().unwrap();
        }
    });
    let mut c = db.connect();
    assert!(crate::schema::inspect(&mut c).unwrap().is_empty());
    let entries: i64 = c
        .query_one("SELECT count(*) FROM obop_record_change", &[])
        .unwrap()
        .get(0);
    assert_eq!(entries, 3, "the three writers wrote their entries");
    let n: i64 = c
        .query_one(
            "SELECT count(*) FROM pg_proc WHERE proname LIKE 'obop_history_%'",
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(n, 4);
}

/// A model that keeps history, with `names` as plain fields.
fn history_of(names: &[&str]) -> Track {
    Track {
        history: true,
        fields: names
            .iter()
            .map(|k| (k.to_string(), Treat::Plain))
            .collect(),
        ..Track::default()
    }
}

fn relations(c: &mut postgres::Client) -> Vec<String> {
    c.query(
        "SELECT relname::text FROM pg_class WHERE relnamespace = 'public'::regnamespace ORDER BY 1",
        &[],
    )
    .unwrap()
    .iter()
    .map(|r| r.get(0))
    .collect()
}

#[test]
fn a_database_installed_before_the_history_starts_under_never_and_says_so_when_it_is_needed() {
    let Some(db) = TestDb::new() else {
        return;
    };
    // What a deployment has that was installed by an older version: the base part only.
    let mut c = db.connect();
    base_only(&mut c);
    let before = relations(&mut c);
    let pg = Pg::connect_options(db.url(), Some(one()), Ddl::Never)
        .expect("the process starts without the history");
    let b = Tracked::new(Arc::new(pg));
    b.track(
        "deal",
        Track {
            history: true,
            fields: [("name".to_string(), Treat::Plain)].into(),
            ..Track::default()
        },
    );
    b.track("stamped", Track::default());
    let ctx = WriteCtx::new("ann", "rest", "deal.create");
    // Everything that needs no history works: untracked models, tracked models without history.
    b.apply_ctx(&ctx, &[ins("note", "n1", json!({"x": 1}))])
        .unwrap();
    b.apply_ctx(&ctx, &[ins("stamped", "s1", json!({"x": 1}))])
        .unwrap();
    assert_eq!(b.get("stamped", "s1").unwrap().unwrap()["row_version"], 1);
    // Reads find no history, and the feed is empty, without a table.
    assert!(
        b.history(&HistoryQuery::of("deal", "d1", 10))
            .unwrap()
            .is_empty()
    );
    let page = b
        .changes(&ChangeQuery {
            limit: 10,
            ..ChangeQuery::default()
        })
        .unwrap();
    assert!(page.rows.is_empty());
    assert_eq!(
        b.history_stats().unwrap(),
        crate::history::HistoryStats::default()
    );
    // A write that must keep history is refused, with the reason, and leaves nothing behind.
    let e = b
        .apply_ctx(&ctx, &[ins("deal", "d1", json!({"name": "A"}))])
        .unwrap_err();
    assert!(
        matches!(&e, ApplyError::Storage(m) if m.contains("the change history is not installed")),
        "{e:?}"
    );
    assert!(b.get("deal", "d1").unwrap().is_none());
    // The strict question and the maintenance primitives say the same.
    let e = b.ensure_history_schema().unwrap_err();
    assert!(e.contains("missing: history"), "{e}");
    assert!(
        b.history_ensure_partition("2026-10")
            .unwrap_err()
            .contains("not installed")
    );
    // Nothing was created by any of it.
    assert_eq!(before, relations(&mut c), "Ddl::Never creates nothing");
    // The owner installs it; the running process finds out within a second and goes on.
    crate::schema::owner_install(db.url()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1200));
    b.apply_ctx(&ctx, &[ins("deal", "d1", json!({"name": "A"}))])
        .unwrap();
    assert_eq!(
        b.history(&HistoryQuery::of("deal", "d1", 10))
            .unwrap()
            .len(),
        1
    );
    // And now the strict question is satisfied.
    b.ensure_history_schema().unwrap();
}

#[test]
fn a_process_that_may_create_the_schema_gives_an_older_database_its_history_with_the_first_entry() {
    let Some(db) = TestDb::new() else {
        return;
    };
    let mut c = db.connect();
    base_only(&mut c);
    c.batch_execute(
        "INSERT INTO obop_record (model, id, data) VALUES ('deal', 'old', '{\"name\": \"A\"}')",
    )
    .unwrap();
    let b = Tracked::new(Arc::new(Pg::connect_with(db.url(), one()).unwrap()));
    b.track("deal", history_of(&["name"]));
    b.track("note", Track::default());
    // The start made nothing, and neither does a write nobody keeps history of.
    assert!(!(crate::pg_hist::PART.check)(&mut c).unwrap());
    let ctx = WriteCtx::new("ann", "rest", "deal.create");
    b.apply_ctx(&ctx, &[ins("note", "n1", json!({"x": 1}))])
        .unwrap();
    assert!(!(crate::pg_hist::PART.check)(&mut c).unwrap());
    // The first write that keeps history makes the part, and is the first entry.
    b.apply_ctx(&ctx, &[ins("deal", "d1", json!({"name": "B"}))])
        .unwrap();
    assert!((crate::pg_hist::PART.check)(&mut c).unwrap());
    assert_eq!(
        b.history(&HistoryQuery::of("deal", "d1", 10))
            .unwrap()
            .len(),
        1
    );
    // What was there is untouched.
    let data: J = c
        .query_one("SELECT data FROM obop_record WHERE id = 'old'", &[])
        .unwrap()
        .get(0);
    assert_eq!(data, json!({"name": "A"}));
}

/// A serving role that may read and write the tables but not create anything, in a database that
/// the owner installed with the record store only (what every deployment is that was set up
/// before the history existed).
fn serving_role_of_an_older_database(db: &mut TestDb) -> String {
    let mut c = db.connect();
    base_only(&mut c);
    let rt = db
        .role("rt", "NOSUPERUSER NOCREATEDB NOCREATEROLE")
        .unwrap();
    db.connect()
        .batch_execute(&format!(
            "GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO {role};
             GRANT USAGE ON ALL SEQUENCES IN SCHEMA public TO {role};
             REVOKE CREATE ON SCHEMA public FROM PUBLIC",
            role = db.role_name("rt")
        ))
        .unwrap();
    rt
}

#[test]
fn an_older_database_opens_exactly_as_before_under_a_role_that_may_not_create_anything() {
    let Some(mut db) = TestDb::new() else {
        return;
    };
    let rt = serving_role_of_an_older_database(&mut db);
    let before = relations(&mut db.connect());
    // The shipped binary opens with `IfMissing` (nothing sets `Never`): the start must not try to
    // create the history, or this role could no longer start a database it started before.
    for (ddl, n) in [(Ddl::IfMissing, 1), (Ddl::Never, 2)] {
        let pg = Pg::connect_options(&rt, Some(one()), ddl)
            .unwrap_or_else(|e| panic!("{ddl:?}: the start must be what it was: {e}"));
        let b = Tracked::new(Arc::new(pg));
        // Models are registered, some with history, as they are once the command layer tracks.
        b.track("deal", history_of(&["name"]));
        b.track("stamped", Track::default());
        let ctx = WriteCtx::new("ann", "rest", "x.create");
        // Everything that keeps no history works: plain models, stamped models, reads, backups.
        b.apply_ctx(&ctx, &[ins("note", &format!("n{n}"), json!({"x": 1}))])
            .unwrap();
        b.apply_ctx(&ctx, &[ins("stamped", &format!("s{n}"), json!({"x": 1}))])
            .unwrap();
        assert_eq!(
            b.get("stamped", &format!("s{n}")).unwrap().unwrap()["row_version"],
            1
        );
        assert!(
            b.history(&HistoryQuery::of("deal", "d1", 10))
                .unwrap()
                .is_empty()
        );
        assert!(b.export(100, &mut |_| Ok(())).is_ok());
        assert_eq!(b.total().unwrap(), 2 * n as usize);
        // A write that must keep history is refused with the reason, and leaves nothing behind.
        let e = b
            .apply_ctx(&ctx, &[ins("deal", "d1", json!({"name": "A"}))])
            .unwrap_err();
        let ApplyError::Storage(m) = e else {
            panic!("{e:?}");
        };
        assert!(
            m.contains("the change history is not installed"),
            "{ddl:?}: {m}"
        );
        if ddl == Ddl::IfMissing {
            assert!(
                m.contains("tried to install it and could not"),
                "{ddl:?}: the reason is the process's own try: {m}"
            );
        }
        assert!(b.get("deal", "d1").unwrap().is_none());
    }
    assert_eq!(before, relations(&mut db.connect()), "nothing was created");
}

#[test]
fn a_failed_install_of_the_part_does_not_mark_the_request_that_met_it() {
    let Some(mut db) = TestDb::new() else {
        return;
    };
    let rt = serving_role_of_an_older_database(&mut db);
    let pg = Pg::connect_options(&rt, Some(one()), Ddl::IfMissing).unwrap();
    // The stack of the binary: the guard refuses writes after a fault of the same request.
    let b = Tracked::new(guarded(Arc::new(pg)));
    b.track("deal", history_of(&["name"]));
    // A request scope, as the HTTP layer opens one: the first write that keeps history tries to
    // install the part, fails, and says why; that is housekeeping, not a fault of the request.
    fault::begin_request(fault::Outcome::default());
    let ctx = WriteCtx::new("ann", "rest", "deal.create");
    b.apply_ctx(&ctx, &[ins("deal", "d1", json!({"name": "A"}))])
        .unwrap_err();
    assert_eq!(
        fault::noted(),
        None,
        "the refusal of the install is not a storage fault"
    );
    // The request goes on: a write that keeps no history is not refused by the guard.
    b.apply_ctx(&ctx, &[ins("note", "n1", json!({"x": 1}))])
        .unwrap();
    assert_eq!(fault::end_request().fault, None);
}

#[test]
fn a_partition_that_cannot_be_made_in_time_does_not_mark_the_request_that_asked_for_it() {
    let Some(db) = TestDb::new() else {
        return;
    };
    let pg = Pg::connect_with(db.url(), one()).unwrap();
    // The stack of the binary: the guard refuses writes after a fault of the same request.
    let b = Tracked::new(guarded(Arc::new(pg)));
    b.track("deal", history_of(&["name"]));
    b.ensure_history_schema().unwrap();
    // The function runs into the limit of the statement (the pool notes a timeout for that).
    db.connect()
        .batch_execute(
            "CREATE OR REPLACE FUNCTION obop_history_ensure_partition(p_month date) RETURNS text
             LANGUAGE plpgsql AS $$ BEGIN
               RAISE EXCEPTION 'too slow' USING ERRCODE = '57014';
             END $$",
        )
        .unwrap();
    fault::begin_request(fault::Outcome::default());
    let ctx = WriteCtx::new("ann", "rest", "deal.create");
    // The partitions of the month are made by the first write; here they cannot be, which is
    // housekeeping on nobody's behalf: the entry goes to the default partition and the write works.
    b.apply_ctx(&ctx, &[ins("deal", "d1", json!({"name": "A"}))])
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(fault::noted(), None, "nobody's request met a timeout");
    // And the request's next write is not refused by the guard because of it.
    b.apply_ctx(&ctx, &[ins("deal", "d2", json!({"name": "A"}))])
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(fault::end_request().fault, None);
}

// ---- the guard ---------------------------------------------------------------------------------------

#[test]
fn the_table_is_append_only_for_everybody_but_the_maintenance_functions() {
    let Some(px) = Px::new() else {
        return;
    };
    px.ok("ann", &[ins("deal", "d1", json!({"name": "A"}))]);
    px.ok("ann", &[merge("deal", "d1", json!({"name": "B"}))]);
    let partition = px.placed()[0].1.clone();
    assert_eq!(partition, "obop_record_change_p202610");
    let mut c = px.sql();
    // The owner too, when it does not say it is doing maintenance: an accident, not an attack.
    for (what, sql) in [
        ("update", "UPDATE obop_record_change SET actor = 'mallory'"),
        ("delete", "DELETE FROM obop_record_change"),
        ("truncate", "TRUNCATE obop_record_change"),
        (
            "truncate of the default partition",
            "TRUNCATE obop_record_change_default",
        ),
        ("truncate of a month", "TRUNCATE obop_record_change_p202610"),
        (
            "update of a month",
            "UPDATE obop_record_change_p202610 SET actor = 'mallory'",
        ),
        (
            "delete from a month",
            "DELETE FROM obop_record_change_p202610",
        ),
    ] {
        let e = c.batch_execute(sql).unwrap_err();
        assert!(
            e.as_db_error()
                .is_some_and(|d| d.message().contains("append-only") && d.code().code() == "42501"),
            "{what}: {e}"
        );
    }
    assert_eq!(px.placed().len(), 2, "nothing changed");
    // Inserting is what the table is for.
    c.batch_execute(
        "INSERT INTO obop_record_change (at, model, record_id, op, actor, row_version, changed)
         VALUES (now(), 'deal', 'd9', 'update', 'ann', 1, '{}')",
    )
    .unwrap();
    assert_eq!(px.placed().len(), 3);
    // Dropping a whole month is DDL and is not stopped by a row guard: that is retention's tool.
    c.batch_execute("DROP TABLE obop_record_change_p202610")
        .unwrap();
    assert_eq!(px.placed().len(), 0, "the month and its entries are gone");
}

#[test]
fn another_role_cannot_change_the_history_even_by_saying_it_does_maintenance() {
    let Some(mut px) = Px::new() else {
        return;
    };
    px.ok(
        "ann",
        &[ins("deal", "d1", json!({"name": "A", "stage": "x"}))],
    );
    px.ok("ann", &[merge("deal", "d1", json!({"stage": "y"}))]);
    let writer = px
        .db
        .role("writer", "NOSUPERUSER NOCREATEDB NOCREATEROLE")
        .unwrap();
    let nobody = px
        .db
        .role("nobody", "NOSUPERUSER NOCREATEDB NOCREATEROLE")
        .unwrap();
    let role = px.db.role_name("writer");
    let other = px.db.role_name("nobody");
    let mut owner = px.sql();
    owner
        .batch_execute(&format!(
            "GRANT SELECT, INSERT, UPDATE, DELETE, TRUNCATE ON ALL TABLES IN SCHEMA public TO {role}, {other};
             GRANT USAGE ON ALL SEQUENCES IN SCHEMA public TO {role}, {other};
             GRANT EXECUTE ON FUNCTION obop_history_erase(text, text, text[], text, jsonb, bigint, timestamptz) TO {role};
             GRANT EXECUTE ON FUNCTION obop_history_ensure_partition(date) TO {role};
             GRANT EXECUTE ON FUNCTION obop_history_drop_partition(date, interval) TO {role}"
        ))
        .unwrap();
    let mut w = crate::tls::connect(&writer).unwrap();
    for sql in [
        "UPDATE obop_record_change SET actor = 'mallory'",
        "DELETE FROM obop_record_change",
        "TRUNCATE obop_record_change",
        "TRUNCATE obop_record_change_default",
        "UPDATE obop_record_change_p202610 SET actor = 'mallory'",
        "TRUNCATE obop_record_change_p202610",
        // The maintenance setting is something the owner's functions set, not a password.
        "SET obop.history_maint = 'on'; UPDATE obop_record_change SET actor = 'mallory'",
        "SET obop.history_maint = 'on'; TRUNCATE obop_record_change",
    ] {
        let e = w.batch_execute(sql).unwrap_err();
        assert!(
            e.as_db_error()
                .is_some_and(|d| d.message().contains("append-only")),
            "{sql}: {e}"
        );
        w.batch_execute("RESET obop.history_maint").unwrap();
    }
    assert!(px.placed().iter().all(|p| p.2 == "deal"));
    // It may append, though (that is what the serving role does) and read.
    w.batch_execute(
        "INSERT INTO obop_record_change (at, model, record_id, op, actor, row_version, changed)
         VALUES (now(), 'deal', 'd9', 'update', 'ann', 1, '{}')",
    )
    .unwrap();
    // The one thing it may do to old entries is what the function does: erase values.
    let n: i64 = w
        .query_one(
            "SELECT obop_history_erase('deal', 'd1', ARRAY['name'], 'ann', '{}', 2,
                                       timestamptz '2026-10-26 12:00:00+00')",
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(n, 2, "both entries of the record held values to erase");
    let changed: J = owner
        .query_one(
            "SELECT changed FROM obop_record_change WHERE record_id = 'd1' AND op = 'update'
              ORDER BY seq LIMIT 1",
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(
        changed["stage"]["t"], "erased",
        "the values of a field that is not kept are gone: {changed}"
    );
    // The second time there is nothing left to erase.
    let n: i64 = w
        .query_one(
            "SELECT obop_history_erase('deal', 'd1', ARRAY['name'], 'ann', '{}', 2,
                                       timestamptz '2026-10-26 12:00:01+00')",
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(n, 0);
    // A role without the grant cannot call the functions at all.
    let mut nob = crate::tls::connect(&nobody).unwrap();
    for sql in [
        "SELECT obop_history_erase('deal', 'd1', '{}', 'x', '{}', 1, now())",
        "SELECT obop_history_ensure_partition(date '2026-11-01')",
        "SELECT obop_history_drop_partition(date '2020-01-01', interval '1 day')",
    ] {
        let e = nob.batch_execute(sql).unwrap_err();
        assert!(
            e.as_db_error().is_some_and(|d| d.code().code() == "42501"),
            "{sql}: {e}"
        );
    }
}

#[test]
fn the_maintenance_functions_ignore_the_callers_search_path() {
    let Some(mut px) = Px::new() else {
        return;
    };
    px.ok("ann", &[ins("deal", "d1", json!({"name": "A"}))]);
    px.ok("ann", &[merge("deal", "d1", json!({"name": "B"}))]);
    let writer = px
        .db
        .role("writer", "NOSUPERUSER NOCREATEDB NOCREATEROLE")
        .unwrap();
    let role = px.db.role_name("writer");
    let mut owner = px.sql();
    // A decoy table of the same name in a schema the caller puts first.
    owner
        .batch_execute(&format!(
            "CREATE SCHEMA evil;
             CREATE TABLE evil.obop_record_change (LIKE obop_record_change);
             GRANT USAGE ON SCHEMA evil TO {role};
             GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA evil TO {role};
             GRANT SELECT, INSERT ON obop_record_change TO {role};
             GRANT EXECUTE ON FUNCTION obop_history_erase(text, text, text[], text, jsonb, bigint, timestamptz) TO {role}"
        ))
        .unwrap();
    let mut w = crate::tls::connect(&writer).unwrap();
    w.batch_execute("SET search_path = evil, public").unwrap();
    w.query_one(
        "SELECT obop_history_erase('deal', 'd1', '{}', 'ann', '{}', 2, now())",
        &[],
    )
    .unwrap();
    let real: i64 = owner
        .query_one(
            "SELECT count(*) FROM public.obop_record_change WHERE op = 'erase'",
            &[],
        )
        .unwrap()
        .get(0);
    let decoy: i64 = owner
        .query_one("SELECT count(*) FROM evil.obop_record_change", &[])
        .unwrap()
        .get(0);
    assert_eq!(
        (real, decoy),
        (1, 0),
        "the function worked on its own schema's table"
    );
}

// ---- partitions ---------------------------------------------------------------------------------------

#[test]
fn the_first_write_that_keeps_history_makes_this_month_and_the_next() {
    let Some(px) = Px::new() else {
        return;
    };
    assert_eq!(px.partitions(), ["obop_record_change_default"]);
    // Writes that keep no history make nothing.
    px.ok("ann", &[ins("note", "n1", json!({"x": 1}))]);
    assert_eq!(px.partitions(), ["obop_record_change_default"]);
    px.ok("ann", &[ins("deal", "d1", json!({"name": "A"}))]);
    assert_eq!(
        px.partitions(),
        [
            "obop_record_change_default",
            "obop_record_change_p202610",
            "obop_record_change_p202611"
        ]
    );
    // A new year: December makes January.
    px.set_clock("2026-12-15T10:00:00.000000Z");
    px.ok("ann", &[ins("deal", "d2", json!({"name": "B"}))]);
    let parts = px.partitions();
    assert!(
        parts.contains(&"obop_record_change_p202612".to_string()),
        "{parts:?}"
    );
    assert!(
        parts.contains(&"obop_record_change_p202701".to_string()),
        "{parts:?}"
    );
    let placed = px.placed();
    assert_eq!(placed[0].1, "obop_record_change_p202610");
    assert_eq!(placed[1].1, "obop_record_change_p202612");
    assert_eq!(px.b.history_stats().unwrap().default_rows, 0);
    assert_eq!(px.b.history_stats().unwrap().maintenance_error, None);
}

#[test]
fn a_change_exactly_at_a_month_boundary_goes_to_the_month_it_belongs_to() {
    let Some(px) = Px::new() else {
        return;
    };
    let last = "2026-10-31T23:59:59.999999Z";
    let first = "2026-11-01T00:00:00.000000Z";
    px.set_clock(last);
    px.ok("ann", &[ins("deal", "a", json!({"name": "A"}))]);
    px.set_clock(first);
    px.ok("ann", &[ins("deal", "b", json!({"name": "B"}))]);
    // The last microsecond of December and the first of January, on both sides of the year.
    px.set_clock("2026-12-31T23:59:59.999999Z");
    px.ok("ann", &[ins("deal", "c", json!({"name": "C"}))]);
    px.set_clock("2027-01-01T00:00:00.000000Z");
    px.ok("ann", &[ins("deal", "d", json!({"name": "D"}))]);
    let by_record: std::collections::BTreeMap<String, String> =
        px.placed().into_iter().map(|p| (p.3, p.1)).collect();
    assert_eq!(by_record["a"], "obop_record_change_p202610");
    assert_eq!(by_record["b"], "obop_record_change_p202611");
    assert_eq!(by_record["c"], "obop_record_change_p202612");
    assert_eq!(by_record["d"], "obop_record_change_p202701");
    // The instants are the ones the clock gave, to the microsecond.
    let at = |id: &str| {
        px.b.history(&HistoryQuery::of("deal", id, 1)).unwrap()[0]
            .at
            .clone()
    };
    assert_eq!(at("a"), last);
    assert_eq!(at("b"), first);
    assert_eq!(px.b.history_stats().unwrap().default_rows, 0);
}

#[test]
fn an_instant_no_partition_takes_lands_in_the_default_one_and_nothing_is_lost() {
    for (id, at, month) in [
        ("far_future", "2100-01-01T00:00:00.000000Z", "2100-01"),
        ("far_past", "1999-12-31T23:59:59.999999Z", "1999-12"),
    ] {
        // A store of its own for each: the clock of a store never goes back.
        let Some(px) = Px::new() else {
            return;
        };
        px.set_clock(at);
        // The write succeeds: a clock that is wrong must not stop the business.
        px.ok("ann", &[ins("deal", id, json!({"name": id}))]);
        assert_eq!(px.b.get("deal", id).unwrap().unwrap()["created_at"], at);
        assert_eq!(px.placed()[0].1, "obop_record_change_default", "{id}");
        let stats = px.b.history_stats().unwrap();
        assert_eq!(stats.default_rows, 1, "{id}");
        assert_eq!(stats.oldest_at.as_deref(), Some(at));
        let why = stats
            .maintenance_error
            .expect("the failed attempt is reported");
        assert!(why.contains("outside the range"), "{why}");
        // Still readable, still in the feed.
        assert_eq!(
            px.b.history(&HistoryQuery::of("deal", id, 5))
                .unwrap()
                .len(),
            1
        );
        assert_eq!(px.feed(None, 100).rows.len(), 1);
        // The primitive refuses a month it will not make, and says so; nothing moves.
        let e = px.b.history_ensure_partition(month).unwrap_err();
        assert!(e.contains("outside the range"), "{month}: {e}");
        assert_eq!(px.b.history_stats().unwrap().default_rows, 1);
    }
}

#[test]
fn entries_in_the_default_partition_are_brought_home_by_ensure_with_their_numbers() {
    let Some(px) = Px::new() else {
        return;
    };
    // The step that makes partitions fails (here: the function is not where it is looked for),
    // and the entries of an ordinary month land in the default partition, where nothing is lost.
    let mut c = px.sql();
    c.batch_execute(
        "ALTER FUNCTION obop_history_ensure_partition(date) RENAME TO obop_history_ensure_partition_x",
    )
    .unwrap();
    px.ok("ann", &[ins("deal", "home", json!({"name": "B"}))]);
    px.ok("ann", &[merge("deal", "home", json!({"name": "C"}))]);
    // And an entry that belongs to no ordinary month, which stays where it is.
    c.batch_execute(
        "INSERT INTO obop_record_change (at, model, record_id, op, actor, row_version, changed)
         VALUES ('2100-01-01T00:00:00Z', 'deal', 'far', 'update', 'ann', 1, '{}')",
    )
    .unwrap();
    let stats = px.b.history_stats().unwrap();
    assert_eq!(stats.default_rows, 3);
    assert!(stats.maintenance_error.is_some(), "{stats:?}");
    c.batch_execute(
        "ALTER FUNCTION obop_history_ensure_partition_x(date) RENAME TO obop_history_ensure_partition",
    )
    .unwrap();
    let before: Vec<_> = px
        .feed(None, 100)
        .rows
        .iter()
        .map(|r| (r.seq, r.txid, r.record_id.clone()))
        .collect();
    let done = px.b.history_ensure_partition("2026-10").unwrap();
    assert_eq!((done.created, done.moved), (true, 2));
    let placed = px.placed();
    assert_eq!(placed[0].1, "obop_record_change_p202610");
    assert_eq!(placed[1].1, "obop_record_change_p202610");
    assert_eq!(
        placed[2].1, "obop_record_change_default",
        "the year 2100 stays"
    );
    // Same numbers, same transactions, same order.
    let after: Vec<_> = px
        .feed(None, 100)
        .rows
        .iter()
        .map(|r| (r.seq, r.txid, r.record_id.clone()))
        .collect();
    assert_eq!(before, after);
    // A second time there is nothing to do.
    let done = px.b.history_ensure_partition("2026-10").unwrap();
    assert_eq!((done.created, done.moved), (false, 0));
    let stats = px.b.history_stats().unwrap();
    assert_eq!(stats.default_rows, 1);
    assert_eq!(
        stats.partitions.last().map(|p| p.0.as_str()),
        Some("obop_record_change_default"),
        "the default partition is listed last"
    );
    for bad in [
        "",
        "2026",
        "2026-13",
        "2026-00",
        "26-10",
        "2026-1-",
        "x",
        "2026-10-01",
    ] {
        let e = px.b.history_ensure_partition(bad).unwrap_err();
        assert!(e.contains("is not a month"), "{bad:?}: {e}");
    }
}

#[test]
fn two_callers_make_the_same_month_once() {
    let Some(px) = Px::new() else {
        return;
    };
    px.ok("ann", &[ins("deal", "d1", json!({"name": "A"}))]);
    let mut c = px.sql();
    // Entries of July wait in the default partition.
    c.batch_execute(
        "INSERT INTO obop_record_change (at, model, record_id, op, actor, row_version, changed)
         SELECT timestamptz '2026-07-01 00:00:00+00' + g * interval '1 hour', 'deal', 'r' || g,
                'update', 'ann', 1, '{}' FROM generate_series(0, 299) g",
    )
    .unwrap();
    assert_eq!(px.b.history_stats().unwrap().default_rows, 300);
    let barrier = std::sync::Barrier::new(4);
    let results: Vec<_> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..4)
            .map(|_| {
                s.spawn(|| {
                    barrier.wait();
                    px.b.history_ensure_partition("2026-07")
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap().unwrap()).collect()
    });
    assert_eq!(
        results.iter().filter(|r| r.created).count(),
        1,
        "{results:?}"
    );
    assert_eq!(results.iter().map(|r| r.moved).sum::<u64>(), 300);
    assert_eq!(px.b.history_stats().unwrap().default_rows, 0);
    let in_july: i64 = c
        .query_one("SELECT count(*) FROM obop_record_change_p202607", &[])
        .unwrap()
        .get(0);
    assert_eq!(in_july, 300);
}

#[test]
fn an_old_month_is_dropped_with_its_entries_and_a_young_one_is_refused() {
    let Some(px) = Px::new() else {
        return;
    };
    px.set_clock("2026-08-10T10:00:00.000000Z");
    px.ok("ann", &[ins("deal", "d1", json!({"name": "A"}))]);
    px.ok("ann", &[merge("deal", "d1", json!({"name": "B"}))]);
    assert!(
        px.partitions()
            .contains(&"obop_record_change_p202608".to_string())
    );
    // The month ended on 1 September; it is not 400 days old.
    let e = px.b.history_drop_partition("2026-08", 400).unwrap_err();
    assert!(e.contains("younger than the retention"), "{e}");
    assert_eq!(px.placed().len(), 2);
    // This month is never old enough, whatever the age asked for.
    px.b.history_ensure_partition("2026-10").unwrap();
    let e = px.b.history_drop_partition("2026-10", 0).unwrap_err();
    assert!(e.contains("younger than the retention"), "{e}");
    // The month before is (the server's date is the real one).
    assert_eq!(px.b.history_drop_partition("2026-08", 30).unwrap(), 2);
    assert!(
        !px.partitions()
            .contains(&"obop_record_change_p202608".to_string())
    );
    assert!(px.placed().is_empty());
    // Dropping what is not there is not an error and drops nothing.
    assert_eq!(px.b.history_drop_partition("2026-08", 30).unwrap(), 0);
    assert!(
        px.b.history_drop_partition("2026-8", 30)
            .unwrap_err()
            .contains("is not a month")
    );
    // The feed and the statistics carry on.
    assert!(px.feed(None, 10).rows.is_empty());
    assert_eq!(px.b.history_stats().unwrap().default_rows, 0);
}

#[test]
fn the_statistics_list_the_partitions_oldest_first_and_the_default_one_last() {
    let Some(px) = Px::new() else {
        return;
    };
    px.ok("ann", &[ins("deal", "d1", json!({"name": "A"}))]);
    px.set_clock("2026-11-05T10:00:00.000000Z");
    px.ok("ann", &[ins("deal", "d2", json!({"name": "B"}))]);
    let stats = px.b.history_stats().unwrap();
    let names: Vec<&str> = stats.partitions.iter().map(|p| p.0.as_str()).collect();
    assert_eq!(
        names,
        [
            "obop_record_change_p202610",
            "obop_record_change_p202611",
            "obop_record_change_p202612",
            "obop_record_change_default"
        ]
    );
    let counts: Vec<i64> = stats.partitions.iter().map(|p| p.1).collect();
    assert_eq!(counts, [1, 1, 0, 0]);
    assert_eq!(stats.entries_estimate, 2);
    assert_eq!(stats.default_rows, 0);
    assert!(stats.horizon_lag_ms >= 0);
    assert_eq!(
        stats.oldest_at.as_deref(),
        Some(fmt_micros(Px::T0).as_str())
    );
    assert_eq!(stats.maintenance_error, None);
}

#[test]
fn making_and_moving_partitions_does_not_block_or_fail_the_writers() {
    let Some(px) = Px::real_clock(8) else {
        return;
    };
    px.ok("ann", &[ins("deal", "start", json!({"name": "A"}))]);
    let stop = std::sync::atomic::AtomicBool::new(false);
    let writes = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|s| {
        let writers: Vec<_> = (0..4)
            .map(|t| {
                let (px, stop, writes) = (&px, &stop, &writes);
                s.spawn(move || {
                    let mut i = 0;
                    while !stop.load(SeqCst) {
                        px.ok(
                            "ann",
                            &[
                                ins("deal", &format!("w{t}-{i}"), json!({"name": "A"})),
                                merge("deal", "start", json!({"n": t * 1000 + i})),
                            ],
                        );
                        writes.fetch_add(1, SeqCst);
                        i += 1;
                    }
                })
            })
            .collect();
        // Twelve months of entries wait in the default partition, one month at a time, and are
        // brought home while the writers go on.
        let mut c = px.sql();
        for m in 1..=12 {
            c.batch_execute(&format!(
                "INSERT INTO obop_record_change (at, model, record_id, op, actor, row_version, changed)
                 SELECT timestamptz '2025-{m:02}-01 00:00:00+00' + g * interval '1 minute',
                        'deal', 'old' || g, 'update', 'ann', 1, '{{}}' FROM generate_series(0, 99) g"
            ))
            .unwrap();
            let done =
                px.b.history_ensure_partition(&format!("2025-{m:02}"))
                    .unwrap();
            assert_eq!((done.created, done.moved), (true, 100), "2025-{m:02}");
        }
        stop.store(true, SeqCst);
        for w in writers {
            w.join().unwrap();
        }
    });
    let n = writes.load(SeqCst);
    assert!(n > 0, "the writers wrote while the partitions were made");
    let total: i64 = px
        .sql()
        .query_one("SELECT count(*) FROM obop_record_change", &[])
        .unwrap()
        .get(0);
    // The first write, the old rows, and an entry for every record the writers made.
    assert!(
        total >= 1 + 1200 + n as i64,
        "{total} entries for {n} batches"
    );
    assert_eq!(px.b.history_stats().unwrap().default_rows, 0);
}

/// What the table holds for the first and last versions of a record, for the message of a test that
/// found the feed out of order.
fn dump_versions(px: &Px, id: &str) -> String {
    px.sql()
        .query(
            "SELECT row_version, txid::text, seq, tableoid::regclass::text, xmin::text, at::text
               FROM obop_record_change WHERE record_id = $1
              ORDER BY row_version",
            &[&id],
        )
        .unwrap()
        .iter()
        .enumerate()
        .filter(|(i, _)| *i < 20)
        .map(|(_, r)| {
            format!(
                "v{} txid={} seq={} in={} xmin={} at={}",
                r.get::<_, i64>(0),
                r.get::<_, String>(1),
                r.get::<_, i64>(2),
                r.get::<_, String>(3),
                r.get::<_, String>(4),
                r.get::<_, String>(5)
            )
        })
        .collect::<Vec<_>>()
        .join(
            "
",
        )
}

// ---- concurrency ---------------------------------------------------------------------------------------

#[test]
fn writers_of_one_record_get_strictly_increasing_versions_and_the_feed_shows_each_entry_once() {
    let Some(px) = Px::real_clock(8) else {
        return;
    };
    const THREADS: i64 = 8;
    const EACH: i64 = 25;
    px.ok("ann", &[ins("deal", "hot", json!({"n": 0}))]);
    std::thread::scope(|s| {
        for t in 0..THREADS {
            let px = &px;
            s.spawn(move || {
                for i in 0..EACH {
                    px.ok(
                        &format!("u{t}"),
                        &[merge("deal", "hot", json!({"n": 1 + t * 1000 + i}))],
                    );
                }
            });
        }
    });
    let total = THREADS * EACH;
    let record = px.b.get("deal", "hot").unwrap().unwrap();
    assert_eq!(record["row_version"], 1 + total);
    // The history, oldest first: every version once, the numbers and the instants only grow.
    let mut h =
        px.b.history(&HistoryQuery::of("deal", "hot", 10_000))
            .unwrap();
    h.reverse();
    assert_eq!(
        h.iter().map(|r| r.row_version).collect::<Vec<_>>(),
        (1..=1 + total).collect::<Vec<_>>()
    );
    assert!(h.windows(2).all(|w| w[0].seq < w[1].seq), "numbers grow");
    assert!(
        h.windows(2).all(|w| instant(&w[0].at) < instant(&w[1].at)),
        "stamps grow"
    );
    // The same writes, as the feed gives them: all of them, once, in the order of the versions.
    // Batches that name one record queue on the advisory lock of its stripe before anything gives
    // them a transaction number, so the numbers follow the versions, and the feed is in the order
    // of (transaction, number). (A run of this test once showed the first entry last; the cause
    // was the sort key of the feed, which sorted the numbers as text, see
    // `the_feed_sorts_transaction_numbers_as_numbers_and_walks_every_entry_once`.)
    let fed = px.feed(None, 10_000);
    assert_eq!(fed.rows.len() as i64, 1 + total);
    let order: Vec<i64> = fed.rows.iter().map(|r| r.row_version).collect();
    if order != (1..=1 + total).collect::<Vec<_>>() {
        let dump: Vec<_> = fed
            .rows
            .iter()
            .filter(|r| r.row_version <= 3 || r.row_version >= total)
            .map(|r| (r.row_version, r.txid, r.seq, r.at.clone(), r.actor.clone()))
            .collect();
        panic!(
            "the feed is out of the order of the versions: {dump:?}
{}",
            dump_versions(&px, "hot")
        );
    }
    // Walked in pages of 17 by cursor: the same rows.
    let mut paged = Vec::new();
    let mut cursor = None;
    loop {
        let page = px.feed(cursor.as_ref(), 17);
        if page.rows.is_empty() {
            break;
        }
        assert!(page.rows.len() <= 17);
        paged.extend(page.rows);
        cursor = page.next;
    }
    assert_eq!(paged, fed.rows);
    // The order of the feed is the order of (transaction, number), never a number alone.
    assert!(
        paged
            .windows(2)
            .all(|w| (w[0].txid, w[0].seq) < (w[1].txid, w[1].seq))
    );
}

#[test]
fn batches_that_touch_the_same_records_in_opposite_orders_do_not_deadlock_and_stay_whole() {
    let Some(px) = Px::real_clock(8) else {
        return;
    };
    px.ok(
        "ann",
        &[
            ins("deal", "a", json!({"n": 0})),
            ins("deal", "b", json!({"n": 0})),
        ],
    );
    std::thread::scope(|s| {
        for t in 0..4i64 {
            let px = &px;
            s.spawn(move || {
                for i in 0..20i64 {
                    let n = 1 + t * 100 + i;
                    let (first, second) = if t % 2 == 0 { ("a", "b") } else { ("b", "a") };
                    px.ok(
                        "ann",
                        &[
                            merge("deal", first, json!({"n": n})),
                            merge("deal", second, json!({"n": n})),
                        ],
                    );
                }
            });
        }
    });
    for id in ["a", "b"] {
        assert_eq!(
            px.b.get("deal", id).unwrap().unwrap()["row_version"],
            81,
            "{id}"
        );
    }
    // Every batch is one transaction: its two entries share the transaction number.
    let fed = px.feed(None, 10_000);
    assert_eq!(fed.rows.len(), 162);
    let mut by_tx: std::collections::BTreeMap<i64, Vec<&str>> = Default::default();
    for r in &fed.rows {
        by_tx.entry(r.txid).or_default().push(&r.record_id);
    }
    assert_eq!(by_tx.len(), 81);
    for (tx, ids) in by_tx {
        let mut ids = ids;
        ids.sort_unstable();
        assert_eq!(ids, ["a", "b"], "transaction {tx}");
    }
}

#[test]
fn a_large_batch_is_one_transaction_with_its_entries_in_the_order_of_its_operations() {
    let Some(px) = Px::new() else {
        return;
    };
    let ops: Vec<Op> = (0..1200)
        .map(|i| ins("deal", &format!("d{i:04}"), json!({"n": i})))
        .collect();
    px.ok("ann", &ops);
    let placed = px.placed();
    assert_eq!(placed.len(), 1200);
    let ids: Vec<&str> = placed.iter().map(|p| p.3.as_str()).collect();
    let want: Vec<String> = (0..1200).map(|i| format!("d{i:04}")).collect();
    assert_eq!(ids, want.iter().map(String::as_str).collect::<Vec<_>>());
    let fed = px.feed(None, 10_000);
    assert_eq!(fed.rows.len(), 1200);
    assert!(fed.rows.iter().all(|r| r.txid == fed.rows[0].txid));
    // And in pages that cut it in the middle.
    let mut got = 0;
    let mut cursor = None;
    loop {
        let page = px.feed(cursor.as_ref(), 500);
        if page.rows.is_empty() {
            break;
        }
        got += page.rows.len();
        cursor = page.next;
    }
    assert_eq!(got, 1200);
}

// ---- the horizon of the feed ------------------------------------------------------------------------

#[test]
fn the_feed_holds_back_what_an_earlier_open_transaction_may_still_commit_and_loses_none() {
    let Some(px) = Px::new() else {
        return;
    };
    px.ok("ann", &[ins("deal", "d1", json!({"name": "A"}))]);
    let first = px.feed(None, 100);
    assert_eq!(first.rows.len(), 1);
    let c0 = first.next.clone().expect("a cursor");
    // A transaction that began before the next write and has not finished: it has its number
    // but has written no entry yet.
    let mut holder = px.sql();
    holder.batch_execute("BEGIN").unwrap();
    holder
        .query_one("SELECT pg_current_xact_id()::text", &[])
        .unwrap();
    px.tick(1_000);
    px.ok("bob", &[merge("deal", "d1", json!({"name": "B"}))]);
    // The write is committed and the feed does not show it: the holder may still add an entry
    // that sorts before it, and a consumer past this one would never see that.
    let held = px.feed_raw(Some(&c0), 100);
    assert!(held.rows.is_empty(), "{held:?}");
    assert_eq!(held.next.as_ref(), Some(&c0), "the place is kept");
    assert!(held.horizon_lag_ms.is_some_and(|ms| ms >= 0), "{held:?}");
    let whole = px.feed_raw(None, 100);
    assert_eq!(
        whole.rows.len(),
        1,
        "what was settled before is still there"
    );
    assert!(px.b.history_stats().unwrap().horizon_lag_ms > 0);
    // The holder writes an entry of its own (it gets the next number, after the write above) and
    // commits.
    holder
        .execute(
            "INSERT INTO obop_record_change (at, model, record_id, op, actor, row_version, changed)
             VALUES ('2026-10-26T00:00:00Z', 'deal', 'from-the-holder', 'update', 'raw', 1, '{}')",
            &[],
        )
        .unwrap();
    holder.batch_execute("COMMIT").unwrap();
    // Now both are shown, the holder's first: its transaction number is lower although its entry
    // has the higher entry number.
    let page = px.feed(Some(&c0), 100);
    let seen: Vec<&str> = page.rows.iter().map(|r| r.record_id.as_str()).collect();
    assert_eq!(seen, ["from-the-holder", "d1"]);
    assert!(page.rows[0].txid < page.rows[1].txid);
    assert!(
        page.rows[0].seq > page.rows[1].seq,
        "numbers alone would have lost it"
    );
    // Nothing more, and the place is kept.
    let end = px.feed(page.next.as_ref(), 100);
    assert!(end.rows.is_empty());
    assert_eq!(end.next, page.next);
    px.settled();
}

#[test]
fn a_feed_page_is_one_snapshot_while_writers_go_on() {
    let Some(px) = Px::real_clock(6) else {
        return;
    };
    px.ok("ann", &[ins("deal", "d0", json!({"n": 0}))]);
    let stop = std::sync::atomic::AtomicBool::new(false);
    let mut seen: Vec<(i64, i64, i64)> = Vec::new();
    let mut cursor = None;
    std::thread::scope(|s| {
        for t in 0..3 {
            let (px, stop) = (&px, &stop);
            s.spawn(move || {
                let mut i = 0;
                while !stop.load(SeqCst) {
                    px.ok(
                        "ann",
                        &[merge("deal", "d0", json!({"n": 1 + t * 100_000 + i}))],
                    );
                    i += 1;
                }
            });
        }
        // A reader that follows the cursor while the writers go on: its pages never repeat an
        // entry and never go back in the order of (transaction, number).
        let started = std::time::Instant::now();
        while seen.len() < 300 && started.elapsed() < std::time::Duration::from_secs(60) {
            let page = px.feed_raw(cursor.as_ref(), 7);
            seen.extend(page.rows.iter().map(|r| (r.txid, r.seq, r.row_version)));
            cursor = page.next;
        }
        stop.store(true, SeqCst);
    });
    assert!(seen.len() >= 300, "only {} entries in a minute", seen.len());
    // The writers have stopped: the rest of the feed, to its end.
    loop {
        let page = px.feed(cursor.as_ref(), 7);
        if page.rows.is_empty() {
            break;
        }
        seen.extend(page.rows.iter().map(|r| (r.txid, r.seq, r.row_version)));
        cursor = page.next;
    }
    assert!(
        seen.windows(2).all(|w| (w[0].0, w[0].1) < (w[1].0, w[1].1)),
        "pages follow the order of (transaction, number) and never repeat an entry"
    );
    // Every version exactly once, none lost by the pages that were cut while the writers went on,
    // and in the order of the versions: what a reader that follows the cursor ends up with.
    let last = px.b.get("deal", "d0").unwrap().unwrap()["row_version"]
        .as_i64()
        .unwrap();
    let versions: Vec<i64> = seen.iter().map(|e| e.2).collect();
    if versions != (1..=last).collect::<Vec<_>>() {
        panic!(
            "every version once, in order: {} entries for {last} versions: {:?}
{}",
            versions.len(),
            &seen[..seen.len().min(40)],
            dump_versions(&px, "d0")
        );
    }
}

// ---- the stamp of old records --------------------------------------------------------------------------

/// A UUIDv7-shaped id that carries `ms`.
fn v7_id(ms: i64) -> String {
    format!(
        "{:08x}-{:04x}-7abc-8def-0123456789ab",
        ms >> 16,
        ms & 0xffff
    )
}

#[test]
fn the_stamp_of_old_records_skips_rows_other_transactions_hold_and_finishes_later() {
    let Some(px) = Px::new() else {
        return;
    };
    let mut c = px.sql();
    for id in ["a", "b", "c", "d", "e", "f"] {
        c.execute(
            "INSERT INTO obop_record (model, id, data) VALUES ('deal', $1, '{\"name\": \"x\"}')",
            &[&id],
        )
        .unwrap();
    }
    let mut holder = px.sql();
    holder.batch_execute("BEGIN").unwrap();
    holder
        .query(
            "SELECT 1 FROM obop_record WHERE model = 'deal' AND id = 'c' FOR UPDATE",
            &[],
        )
        .unwrap();
    let fallback = "2026-10-01T00:00:00.000000Z";
    let models = vec!["deal".to_string()];
    // The held row is skipped, not waited for.
    let started = std::time::Instant::now();
    let pass = px.b.backfill_sysfields(&models, fallback, 100).unwrap();
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    assert_eq!((pass.stamped, pass.remaining), (5, 1));
    holder.batch_execute("COMMIT").unwrap();
    let pass = px.b.backfill_sysfields(&models, fallback, 100).unwrap();
    assert_eq!((pass.stamped, pass.remaining), (1, 0));
    let pass = px.b.backfill_sysfields(&models, fallback, 100).unwrap();
    assert_eq!(
        (pass.stamped, pass.remaining),
        (0, 0),
        "nothing is stamped twice"
    );
    for id in ["a", "c", "f"] {
        let r = px.b.get("deal", id).unwrap().unwrap();
        assert_eq!(r["created_at"], fallback);
        assert_eq!(r["updated_at"], fallback);
        assert_eq!(r["row_version"], 1);
        assert_eq!(r["sys_backfilled"], true);
        assert_eq!(r["name"], "x");
    }
    // The stamp is not a change: no entry.
    assert!(px.placed().is_empty());
}

#[test]
fn the_time_a_uuid_carries_is_written_as_the_rust_side_writes_it() {
    let Some(px) = Px::new() else {
        return;
    };
    let mut c = px.sql();
    let cases: Vec<i64> = vec![
        0,
        1,
        999,
        1000,
        86_399_999,
        1_700_000_000_123,
        1_793_000_000_000,
        4_102_444_800_000,
        253_402_300_799_999,
    ];
    for ms in &cases {
        c.execute(
            "INSERT INTO obop_record (model, id, data) VALUES ('deal', $1, '{}')",
            &[&v7_id(*ms)],
        )
        .unwrap();
    }
    // Past the last day of year 9999, and not v7 (version 4, upper case letters): the first is
    // clamped as the Rust function does, the others take the fallback.
    let beyond = v7_id(0xFFFF_FFFF_FFFF);
    let v4 = "0194c9a8-b3d2-4abc-8def-0123456789ab";
    let upper = "0194C9A8-B3D2-7ABC-8DEF-0123456789AB";
    for id in [&beyond, v4, upper] {
        c.execute(
            "INSERT INTO obop_record (model, id, data) VALUES ('deal', $1, '{}')",
            &[&id],
        )
        .unwrap();
    }
    let fallback = "2026-10-01T12:34:56.789012Z";
    let models = vec!["deal".to_string()];
    let pass = px.b.backfill_sysfields(&models, fallback, 1000).unwrap();
    assert_eq!((pass.stamped, pass.remaining), (cases.len() + 3, 0));
    for ms in &cases {
        let r = px.b.get("deal", &v7_id(*ms)).unwrap().unwrap();
        assert_eq!(r["created_at"], fmt_micros(ms * 1000), "{ms}");
        assert_eq!(
            Some(*ms),
            sysfields::uuid7_millis(&v7_id(*ms)),
            "the Rust side reads the same time"
        );
    }
    let clamped = sysfields::uuid7_millis(&beyond).unwrap();
    assert_eq!(
        px.b.get("deal", &beyond).unwrap().unwrap()["created_at"],
        fmt_micros(clamped * 1000)
    );
    for id in [v4, upper] {
        assert_eq!(
            px.b.get("deal", id).unwrap().unwrap()["created_at"],
            fallback
        );
        assert_eq!(sysfields::uuid7_millis(id), None);
    }
}

#[test]
fn the_stamp_resumes_in_passes_and_counts_what_is_left() {
    let Some(px) = Px::new() else {
        return;
    };
    let mut c = px.sql();
    c.batch_execute(
        "INSERT INTO obop_record (model, id, data)
         SELECT 'deal', 'r' || g, '{\"created_by\": \"old\", \"updated_by\": \"old\"}'
           FROM generate_series(1, 25) g;
         INSERT INTO obop_record (model, id, data) VALUES ('deal', 'done', '{\"row_version\": 5}'),
            ('deal', 'weird', '{\"row_version\": \"7\"}'), ('deal', 'nul', '{\"row_version\": null}'),
            ('deal', 'zero', '{\"row_version\": 0}'), ('deal', 'frac', '{\"row_version\": 2.5}'),
            ('other', 'o1', '{}')",
    )
    .unwrap();
    let models = vec!["deal".to_string()];
    let fallback = "2026-10-01T00:00:00.000000Z";
    // A version that is not a whole number from 1 counts as never stamped (as the other stores read it).
    let mut passes = Vec::new();
    loop {
        let p = px.b.backfill_sysfields(&models, fallback, 10).unwrap();
        passes.push((p.stamped, p.remaining));
        if p.remaining == 0 {
            break;
        }
    }
    assert_eq!(
        passes,
        [(10, 19), (10, 9), (9, 0)],
        "25 plain records and four with a version that is none"
    );
    let r1 = px.b.get("deal", "r1").unwrap().unwrap();
    assert!(
        !r1.contains_key("created_by") && !r1.contains_key("updated_by"),
        "{r1:?}"
    );
    assert_eq!(px.b.get("deal", "done").unwrap().unwrap()["row_version"], 5);
    assert_eq!(
        px.b.get("deal", "weird").unwrap().unwrap()["row_version"],
        1
    );
    assert!(
        px.b.get("other", "o1")
            .unwrap()
            .unwrap()
            .get("row_version")
            .is_none()
    );
}

// ---- restore ---------------------------------------------------------------------------------------------

impl Px {
    fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        self.b
            .export(7, &mut |batch| {
                rows.extend_from_slice(batch);
                Ok(())
            })
            .unwrap();
        rows
    }

    fn restore(&self, rows: Vec<Row>) -> Result<usize> {
        let mut feed = Some(rows);
        self.b.import(true, &mut || Ok(feed.take()))
    }
}

fn untimed(mut v: Vec<ChangeRow>) -> Vec<ChangeRow> {
    for r in &mut v {
        r.txid = 0;
    }
    v
}

#[test]
fn a_backup_with_history_restores_with_its_numbers_its_partitions_and_a_sequence_that_goes_on() {
    let Some(a) = Px::new() else {
        return;
    };
    a.ok("ann", &[ins("deal", "d1", json!({"name": "A"}))]);
    a.set_clock("2026-11-20T10:00:00.000000Z");
    a.ok("bob", &[merge("deal", "d1", json!({"name": "B"}))]);
    a.set_clock("2026-12-05T10:00:00.000000Z");
    a.ok("cy", &[merge("deal", "d1", json!({"stage": "x"}))]);
    a.ok(
        "cy",
        &[Op::EraseHistory {
            model: "deal".into(),
            id: "d1".into(),
        }],
    );
    let rows = a.rows();
    let wanted = untimed(a.b.history(&HistoryQuery::of("deal", "d1", 100)).unwrap());
    assert_eq!(wanted.len(), 4);
    let top = wanted.iter().map(|r| r.seq).max().unwrap();
    // A database with a history of its own, and a sequence that is further on than the backup's.
    let Some(b) = Px::new() else {
        return;
    };
    for i in 0..10 {
        b.tick(1_000);
        b.ok(
            "zed",
            &[ins("deal", &format!("own{i}"), json!({"name": "x"}))],
        );
    }
    let stale = b.feed(None, 100).next.expect("a cursor");
    assert_eq!(b.restore(rows.clone()).unwrap(), rows.len());
    // What was there is gone, what came is there with its numbers.
    assert_eq!(
        untimed(b.b.history(&HistoryQuery::of("deal", "d1", 100)).unwrap()),
        wanted
    );
    assert!(
        b.b.history(&HistoryQuery::of("deal", "own1", 100))
            .unwrap()
            .is_empty()
    );
    assert_eq!(b.placed().len(), 4);
    // Each entry sits in the partition of its month (made by the restore), none in the default.
    let months: Vec<String> = b.placed().into_iter().map(|p| p.1).collect();
    assert_eq!(
        months,
        [
            "obop_record_change_p202610",
            "obop_record_change_p202611",
            "obop_record_change_p202612",
            "obop_record_change_p202612"
        ]
    );
    assert_eq!(b.b.history_stats().unwrap().default_rows, 0);
    // The feed lists them, and a cursor from before the restore is refused.
    assert_eq!(b.feed(None, 100).rows.len(), 4);
    let e =
        b.b.changes(&ChangeQuery {
            after: Some(stale),
            limit: 10,
            ..ChangeQuery::default()
        })
        .unwrap_err();
    assert!(e.starts_with("cursor_expired:"), "{e}");
    // The next entry sorts after everything that was there, the backup's and the database's own:
    // the sequence was 10 and stays on 10 (never back to the backup's last number).
    assert!(top < 10);
    b.set_clock("2026-12-06T10:00:00.000000Z");
    b.ok("dee", &[merge("deal", "d1", json!({"stage": "y"}))]);
    let newest = b.b.history(&HistoryQuery::of("deal", "d1", 1)).unwrap();
    assert_eq!(newest[0].seq, 11);
    // And the other way: a sequence behind the backup is moved past it.
    let Some(c) = Px::new() else {
        return;
    };
    c.restore(rows.clone()).unwrap();
    c.set_clock("2026-12-06T10:00:00.000000Z");
    c.ok("dee", &[merge("deal", "d1", json!({"stage": "y"}))]);
    assert_eq!(
        c.b.history(&HistoryQuery::of("deal", "d1", 1)).unwrap()[0].seq,
        top + 1
    );
    // Exported again, the backup is the one that came (in the order of its numbers).
    let again = Px::new().unwrap();
    again.restore(rows.clone()).unwrap();
    assert_eq!(again.rows(), rows);
}

#[test]
fn an_old_backup_without_history_empties_the_history_it_is_restored_over() {
    let Some(px) = Px::new() else {
        return;
    };
    px.ok("ann", &[ins("deal", "d1", json!({"name": "A"}))]);
    px.ok("ann", &[merge("deal", "d1", json!({"name": "B"}))]);
    let stale = px.feed(None, 10).next.unwrap();
    let old: Vec<Row> = vec![(
        "deal".to_string(),
        "d1".to_string(),
        f(json!({"name": "from the old backup"})),
    )];
    assert_eq!(px.restore(old).unwrap(), 1);
    assert!(
        px.placed().is_empty(),
        "the history belongs to the data it described"
    );
    assert!(
        px.b.history(&HistoryQuery::of("deal", "d1", 10))
            .unwrap()
            .is_empty()
    );
    assert!(px.feed(None, 10).rows.is_empty());
    let e =
        px.b.changes(&ChangeQuery {
            after: Some(stale),
            limit: 10,
            ..ChangeQuery::default()
        })
        .unwrap_err();
    assert!(e.starts_with("cursor_expired:"), "{e}");
    // The record is as the backup had it: not stamped, and the first write after it stamps it.
    assert_eq!(
        px.b.get("deal", "d1").unwrap().unwrap(),
        f(json!({"name": "from the old backup"}))
    );
    px.ok("bob", &[merge("deal", "d1", json!({"name": "C"}))]);
    // (an absent version reads as 1, so this is the second)
    let r = px.b.get("deal", "d1").unwrap().unwrap();
    assert_eq!(r["row_version"], 2);
    assert_eq!(px.placed().len(), 1);
}

#[test]
fn a_restore_that_repeats_an_entry_is_refused_whole() {
    let Some(a) = Px::new() else {
        return;
    };
    a.ok("ann", &[ins("deal", "d1", json!({"name": "A"}))]);
    a.ok("ann", &[merge("deal", "d1", json!({"name": "B"}))]);
    let rows = a.rows();
    let Some(b) = Px::new() else {
        return;
    };
    b.ok("zed", &[ins("deal", "keep", json!({"name": "K"}))]);
    let before = b.rows();
    let first_entry = rows
        .iter()
        .find(|r| r.0 == "record_change")
        .unwrap()
        .clone();
    // The same number twice in one month: the table's key refuses it.
    let mut twice = rows.clone();
    twice.push(first_entry.clone());
    let e = b.restore(twice).unwrap_err();
    assert!(e.contains("duplicate record_change"), "{e}");
    assert_eq!(b.rows(), before, "nothing of it was kept");
    // The same number in two months: only the check at the end sees it.
    let mut other_month = first_entry.clone();
    other_month
        .2
        .insert("at".into(), json!("2026-11-02T10:00:00.000000Z"));
    let mut twice = rows.clone();
    twice.push(other_month);
    let e = b.restore(twice).unwrap_err();
    assert!(e.contains("duplicate record_change"), "{e}");
    assert_eq!(b.rows(), before);
    assert_eq!(b.placed().len(), 1);
    // The store goes on as before.
    b.ok("zed", &[merge("deal", "keep", json!({"name": "L"}))]);
    assert_eq!(b.placed().len(), 2);
}

#[test]
fn entries_from_any_year_are_restored_and_the_ones_no_month_takes_wait_in_the_default_partition() {
    let Some(a) = Px::new() else {
        return;
    };
    a.ok("ann", &[ins("deal", "d1", json!({"name": "A"}))]);
    let mut rows = a.rows();
    let entry = rows
        .iter()
        .find(|r| r.0 == "record_change")
        .unwrap()
        .clone();
    for (i, at) in [
        "1999-01-01T00:00:00.000000Z",
        "2100-06-01T00:00:00.000000Z",
        "2019-03-31T23:59:59.999999Z",
    ]
    .iter()
    .enumerate()
    {
        let mut e = entry.clone();
        e.1 = (100 + i).to_string();
        e.2.insert("at".into(), json!(at));
        rows.push(e);
    }
    let Some(b) = Px::new() else {
        return;
    };
    assert_eq!(b.restore(rows.clone()).unwrap(), rows.len());
    let placed: std::collections::BTreeMap<i64, String> =
        b.placed().into_iter().map(|p| (p.0, p.1)).collect();
    assert_eq!(placed[&100], "obop_record_change_default");
    assert_eq!(placed[&101], "obop_record_change_default");
    assert_eq!(placed[&102], "obop_record_change_p201903");
    assert_eq!(b.rows().len(), rows.len());
    assert_eq!(b.b.history_stats().unwrap().default_rows, 2);
}

#[test]
fn a_restore_by_a_role_that_does_not_own_the_history_says_what_it_needs() {
    let Some(mut px) = Px::new() else {
        return;
    };
    px.ok("ann", &[ins("deal", "d1", json!({"name": "A"}))]);
    px.ok("ann", &[merge("deal", "d1", json!({"name": "B"}))]);
    let backup = px.rows();
    let url = px
        .db
        .role("restorer", "NOSUPERUSER NOCREATEDB NOCREATEROLE")
        .unwrap();
    let role = px.db.role_name("restorer");
    px.sql()
        .batch_execute(&format!(
            "GRANT SELECT, INSERT, UPDATE, DELETE, TRUNCATE ON ALL TABLES IN SCHEMA public TO {role};
             GRANT ALL ON ALL SEQUENCES IN SCHEMA public TO {role}"
        ))
        .unwrap();
    let theirs = Tracked::new(Arc::new(
        Pg::connect_options(&url, Some(one()), Ddl::Never).unwrap(),
    ));
    // Over a history that is there: refused before anything is touched, with the reason.
    let mut feed = Some(backup.clone());
    let e = theirs.import(true, &mut || Ok(feed.take())).unwrap_err();
    assert!(
        e.contains("needs the role that owns the table obop_record_change"),
        "{e}"
    );
    assert_eq!(px.rows(), backup);
    // Into a database whose history is empty, the same role restores records and entries; the
    // partitions it may not make are not needed, the entries wait in the default partition.
    let fresh = TestDb::new().unwrap();
    crate::schema::owner_install(fresh.url()).unwrap();
    let mut fresh = fresh;
    let url = fresh
        .role("restorer", "NOSUPERUSER NOCREATEDB NOCREATEROLE")
        .unwrap();
    let role = fresh.role_name("restorer");
    fresh
        .connect()
        .batch_execute(&format!(
            "GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO {role};
             GRANT ALL ON ALL SEQUENCES IN SCHEMA public TO {role}"
        ))
        .unwrap();
    let theirs = Tracked::new(Arc::new(
        Pg::connect_options(&url, Some(one()), Ddl::Never).unwrap(),
    ));
    let mut feed = Some(backup.clone());
    assert_eq!(
        theirs.import(true, &mut || Ok(feed.take())).unwrap(),
        backup.len()
    );
    let stats = theirs.history_stats().unwrap();
    assert_eq!(
        (stats.default_rows, stats.entries_estimate),
        (2, 2),
        "{stats:?}"
    );
    // Over a history that is there it is refused again, as for the owner's own table above.
    let mut feed = Some(backup.clone());
    let e = theirs.import(true, &mut || Ok(feed.take())).unwrap_err();
    assert!(e.contains("needs the role that owns the table"), "{e}");
    // And a backup of the time before the history restores over an empty history as it always did.
    let mut empty = TestDb::new().unwrap();
    crate::schema::owner_install(empty.url()).unwrap();
    let url = empty
        .role("restorer", "NOSUPERUSER NOCREATEDB NOCREATEROLE")
        .unwrap();
    let role = empty.role_name("restorer");
    empty
        .connect()
        .batch_execute(&format!(
            "GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO {role};
             GRANT ALL ON ALL SEQUENCES IN SCHEMA public TO {role}"
        ))
        .unwrap();
    let theirs = Tracked::new(Arc::new(
        Pg::connect_options(&url, Some(one()), Ddl::Never).unwrap(),
    ));
    let old: Vec<Row> = backup
        .iter()
        .filter(|r| r.0 != "record_change")
        .cloned()
        .collect();
    let mut feed = Some(old.clone());
    assert_eq!(
        theirs.import(true, &mut || Ok(feed.take())).unwrap(),
        old.len()
    );
}

#[test]
fn a_backup_with_entries_cannot_be_restored_into_a_database_that_has_no_history() {
    let Some(a) = Px::new() else {
        return;
    };
    a.ok("ann", &[ins("deal", "d1", json!({"name": "A"}))]);
    let backup = a.rows();
    let Some(db) = TestDb::new() else {
        return;
    };
    let mut c = db.connect();
    base_only(&mut c);
    let old = Tracked::new(Arc::new(
        Pg::connect_options(db.url(), Some(one()), Ddl::Never).unwrap(),
    ));
    let mut feed = Some(backup.clone());
    let e = old.import(true, &mut || Ok(feed.take())).unwrap_err();
    assert!(e.contains("the backup carries change history"), "{e}");
    assert_eq!(old.total().unwrap(), 0, "the whole restore was undone");
    // A backup from before the history restores into it, as ever.
    let plain: Vec<Row> = backup
        .iter()
        .filter(|r| r.0 != "record_change")
        .cloned()
        .collect();
    let mut feed = Some(plain.clone());
    assert_eq!(
        old.import(true, &mut || Ok(feed.take())).unwrap(),
        plain.len()
    );
}

#[test]
fn a_backup_with_entries_makes_the_history_of_an_older_database_when_the_process_may_create_it() {
    let Some(a) = Px::new() else {
        return;
    };
    a.ok("ann", &[ins("deal", "d1", json!({"name": "A"}))]);
    a.ok("ann", &[merge("deal", "d1", json!({"name": "B"}))]);
    let backup = a.rows();
    let Some(mut db) = TestDb::new() else {
        return;
    };
    let rt = serving_role_of_an_older_database(&mut db);
    // A role that may not create anything cannot: the restore is refused whole, with the reason.
    let theirs = Tracked::new(Arc::new(
        Pg::connect_options(&rt, Some(one()), Ddl::IfMissing).unwrap(),
    ));
    let mut feed = Some(backup.clone());
    theirs
        .import(true, &mut || Ok(feed.take()))
        .expect_err("no right to create the history");
    assert_eq!(theirs.total().unwrap(), 0, "the whole restore was undone");
    assert!(!(crate::pg_hist::PART.check)(&mut db.connect()).unwrap());
    // The owner's process makes it, in the transaction of the restore, and the numbers go on.
    let mine = Tracked::new(Arc::new(Pg::connect_with(db.url(), one()).unwrap()));
    mine.track("deal", history_of(&["name"]));
    let mut feed = Some(backup.clone());
    assert_eq!(
        mine.import(true, &mut || Ok(feed.take())).unwrap(),
        backup.len()
    );
    assert!((crate::pg_hist::PART.check)(&mut db.connect()).unwrap());
    assert_eq!(
        mine.history(&HistoryQuery::of("deal", "d1", 10))
            .unwrap()
            .len(),
        2
    );
    mine.apply_ctx(
        &WriteCtx::new("ann", "rest", "deal.update"),
        &[merge("deal", "d1", json!({"name": "C"}))],
    )
    .unwrap();
    let h = mine.history(&HistoryQuery::of("deal", "d1", 10)).unwrap();
    assert_eq!(h.len(), 3);
    assert!(
        h[0].seq > h[1].seq,
        "the new entry sorts after the restored ones"
    );
}

// ---- when the connection or the commit fails ----------------------------------------------------------

/// A trigger that runs when the transaction commits (a deferred constraint trigger on the
/// records), for the record `id`; `body` is what it does. It counts its runs in `zz_calls`, a
/// sequence, which is not rolled back: the run count says whether a batch was run again.
fn trigger_at_commit(px: &Px, id: &str, body: &str) {
    px.sql()
        .batch_execute(&format!(
            "CREATE SEQUENCE IF NOT EXISTS zz_calls;
             CREATE OR REPLACE FUNCTION zz_at_commit() RETURNS trigger LANGUAGE plpgsql AS $f$
             BEGIN
               IF NEW.id = '{id}' THEN
                 {body}
               END IF;
               RETURN NULL;
             END $f$;
             DROP TRIGGER IF EXISTS zz_at_commit ON obop_record;
             CREATE CONSTRAINT TRIGGER zz_at_commit AFTER INSERT ON obop_record
               DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION zz_at_commit();"
        ))
        .unwrap();
}

fn calls(px: &Px) -> i64 {
    px.sql()
        .query_one(
            "SELECT CASE WHEN is_called THEN last_value ELSE 0 END FROM zz_calls",
            &[],
        )
        .unwrap()
        .get(0)
}

fn entries_of(px: &Px, id: &str) -> i64 {
    px.sql()
        .query_one(
            "SELECT count(*) FROM obop_record_change WHERE record_id = $1",
            &[&id],
        )
        .unwrap()
        .get(0)
}

#[test]
fn a_connection_lost_at_commit_leaves_neither_record_nor_entry_and_the_batch_is_not_run_again() {
    use crate::fault::Fault;
    let Some(px) = Px::new() else {
        return;
    };
    trigger_at_commit(
        &px,
        "doomed",
        "PERFORM nextval('zz_calls');
         PERFORM pg_terminate_backend(pg_backend_pid());
         PERFORM pg_sleep(5);",
    );
    let e = px
        .by("ann", &[ins("deal", "doomed", json!({"name": "A"}))])
        .unwrap_err();
    let ApplyError::Storage(m) = e else {
        panic!("not a conflict: {e:?}");
    };
    assert_eq!(Fault::of_message(&m), Some(Fault::Unavailable), "{m}");
    // The outcome was not guessed at: the batch ran once, and the commit did not happen.
    assert_eq!(calls(&px), 1, "a lost commit is not run again by the store");
    assert!(
        px.sql()
            .query_opt("SELECT 1 FROM obop_record WHERE id = 'doomed'", &[])
            .unwrap()
            .is_none()
    );
    assert_eq!(entries_of(&px, "doomed"), 0);
    // The pool is whole again and the same write, asked again by its caller, is one entry.
    px.sql()
        .batch_execute("DROP TRIGGER zz_at_commit ON obop_record")
        .unwrap();
    px.ok("ann", &[ins("deal", "doomed", json!({"name": "A"}))]);
    let h =
        px.b.history(&HistoryQuery::of("deal", "doomed", 10))
            .unwrap();
    assert_eq!(h.len(), 1);
    assert_eq!((h[0].op.as_str(), h[0].row_version), ("create", 1));
}

#[test]
fn a_commit_the_server_refuses_leaves_neither_record_nor_entry() {
    let Some(px) = Px::new() else {
        return;
    };
    px.ok("ann", &[ins("deal", "fine", json!({"name": "A"}))]);
    trigger_at_commit(
        &px,
        "bad",
        "PERFORM nextval('zz_calls'); RAISE EXCEPTION 'not at the end of the month' USING ERRCODE = 'P0001';",
    );
    let e = px
        .by(
            "ann",
            &[
                ins("deal", "other", json!({"name": "O"})),
                ins("deal", "bad", json!({"name": "A"})),
            ],
        )
        .unwrap_err();
    let ApplyError::Storage(m) = e else {
        panic!("not a conflict: {e:?}");
    };
    assert!(m.contains("not at the end of the month"), "{m}");
    assert_eq!(calls(&px), 1);
    assert!(px.b.get("deal", "other").unwrap().is_none());
    assert_eq!(entries_of(&px, "other") + entries_of(&px, "bad"), 0);
    assert_eq!(px.placed().len(), 1, "only the first write's entry");
}

#[test]
fn a_batch_the_server_rolls_back_for_a_deadlock_at_commit_is_run_again_with_one_entry_per_record() {
    let Some(px) = Px::new() else {
        return;
    };
    // The first time the transaction commits, the server finds a deadlock; the entries of that
    // attempt were written inside the transaction and are rolled back with it.
    trigger_at_commit(
        &px,
        "flaky",
        "IF nextval('zz_calls') = 1 THEN
           RAISE EXCEPTION 'deadlock detected' USING ERRCODE = '40P01';
         END IF;",
    );
    px.ok(
        "ann",
        &[
            ins("deal", "one", json!({"name": "1"})),
            ins("deal", "flaky", json!({"name": "2"})),
            ins("deal", "three", json!({"name": "3"})),
        ],
    );
    assert_eq!(calls(&px), 2, "run, rolled back, run again");
    for id in ["one", "flaky", "three"] {
        assert_eq!(
            entries_of(&px, id),
            1,
            "{id}: one entry, not one per attempt"
        );
        let r = px.b.get("deal", id).unwrap().unwrap();
        assert_eq!(r["row_version"], 1, "{id}");
    }
    assert_eq!(px.placed().len(), 3);
}

#[test]
fn a_refusal_of_the_database_writes_no_history_for_the_rest_of_the_batch() {
    let Some(px) = Px::new() else {
        return;
    };
    px.sql()
        .batch_execute(
            r#"CREATE FUNCTION zz_refuse() RETURNS trigger LANGUAGE plpgsql AS $f$
               BEGIN
                 IF NEW.model = 'frozen' THEN
                   RAISE EXCEPTION USING ERRCODE = 'OBG01', MESSAGE = 'immutable_record',
                     DETAIL = json_build_object('model', NEW.model, 'id', NEW.id,
                       'rule', 'immutable_when', 'fields', json_build_array('total'),
                       'detail', 'total cannot change')::text;
                 END IF;
                 RETURN NEW;
               END $f$;
               CREATE TRIGGER zz_refuse BEFORE INSERT ON obop_record
                 FOR EACH ROW EXECUTE FUNCTION zz_refuse();"#,
        )
        .unwrap();
    let e = px
        .by(
            "ann",
            &[
                ins("deal", "d1", json!({"name": "A"})),
                ins("frozen", "x", json!({"total": 1})),
            ],
        )
        .unwrap_err();
    let ApplyError::Storage(m) = e else {
        panic!("not a conflict: {e:?}");
    };
    let r = crate::refusal::parse(&m).unwrap_or_else(|| panic!("not a refusal: {m}"));
    assert_eq!(r.code, crate::refusal::RefusalCode::ImmutableRecord);
    assert!(px.b.get("deal", "d1").unwrap().is_none());
    assert!(px.placed().is_empty());
    assert!(px.feed(None, 10).rows.is_empty());
}

// ---- the rest of the contract ---------------------------------------------------------------------------

#[test]
fn the_digest_pins_the_sql_of_the_history_part() {
    let part = &crate::pg_hist::PART;
    let d = part.digest();
    assert_eq!(d.len(), 64);
    // The digest is of the schema "public", whatever schema a process renders for. If this
    // fails, the SQL of the history part changed: databases that have the part do not get the
    // change unless a ledger step installs it, so add the step and update this value together.
    assert_eq!(
        d, "0ef284a32ae3bcbcaaca2ddca93c98eb63679fa2846ebaa712e1e07c9970c41d",
        "digest of the history part"
    );
    assert_ne!(d, crate::schema::parts()[0].digest());
    let sql = (part.render)("public");
    assert!(!sql.contains("{schema}"));
    // Every statement says which schema it means, or runs where the function's own pinned path
    // points; none leaves it to the caller.
    assert_eq!(
        sql.matches("SET search_path = public, pg_temp").count(),
        4,
        "the guard and the three maintenance functions pin their search path"
    );
    assert_eq!(part.name, "history");
    assert!(!part.required);
}

#[test]
fn the_part_works_in_a_schema_of_an_odd_name() {
    let Some(db) = TestDb::new() else {
        return;
    };
    db.connect()
        .batch_execute(&format!(
            "CREATE SCHEMA \"Odd Schema\"; \
             ALTER DATABASE {} SET search_path TO \"Odd Schema\", public",
            db.name()
        ))
        .unwrap();
    let px = Px::on(db, 3, true);
    px.ok("ann", &[ins("deal", "d1", json!({"name": "A"}))]);
    px.ok("ann", &[merge("deal", "d1", json!({"name": "B"}))]);
    px.b.history_ensure_partition("2026-09").unwrap();
    px.ok(
        "ann",
        &[Op::EraseHistory {
            model: "deal".into(),
            id: "d1".into(),
        }],
    );
    assert_eq!(
        px.b.history(&HistoryQuery::of("deal", "d1", 10))
            .unwrap()
            .len(),
        3
    );
    assert_eq!(px.feed(None, 10).rows.len(), 3);
    let mut c = px.sql();
    let (here, elsewhere): (i64, i64) = {
        let r = c
            .query_one(
                "SELECT count(*) FILTER (WHERE relnamespace = '\"Odd Schema\"'::regnamespace),
                        count(*) FILTER (WHERE relnamespace = 'public'::regnamespace)
                   FROM pg_class WHERE relname LIKE 'obop_record_change%'",
                &[],
            )
            .unwrap();
        (r.get(0), r.get(1))
    };
    assert!(
        here >= 8 && elsewhere == 0,
        "{here} here, {elsewhere} in public"
    );
    let fns: i64 = c
        .query_one(
            "SELECT count(*) FROM pg_proc WHERE proname LIKE 'obop_history_%'
                AND pronamespace = '\"Odd Schema\"'::regnamespace",
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(fns, 4);
    assert_eq!(
        c.query_one(
            "SELECT count(*) FROM \"Odd Schema\".obop_record_change_p202609",
            &[]
        )
        .unwrap()
        .get::<_, i64>(0),
        0
    );
}

#[test]
fn a_number_a_json_reader_cannot_hold_makes_the_export_an_error_and_not_a_panic() {
    let Some(px) = Px::new() else {
        return;
    };
    px.sql()
        .batch_execute(
            "INSERT INTO obop_record (model, id, data) VALUES ('note', 'big', '{\"x\": 1.7976931348623157e308}')",
        )
        .unwrap();
    let e =
        px.b.export(10, &mut |_| Ok(()))
            .expect_err("the record cannot be read back as JSON");
    assert!(!e.is_empty());
}

// ---- the order of the feed ----------------------------------------------------------------------------

/// An entry with a transaction number of the test's choosing, written as plain SQL (the table is
/// what it is for: the owner may insert).
fn raw_entry(px: &Px, txid: u64, id: &str) {
    px.sql()
        .execute(
            "INSERT INTO obop_record_change
               (at, txid, model, record_id, op, actor, row_version, changed)
             VALUES (timestamptz 'epoch' + $1::bigint * interval '1 microsecond',
                     $2::text::xid8, 'raw', $3, 'update', 'raw', 1, '{}')",
            &[&Px::T0, &txid.to_string(), &id],
        )
        .unwrap();
}

/// The numbers below the server's horizon among `wanted`: the feed shows nothing at or above it.
fn below_the_horizon(px: &Px, wanted: &[u64]) -> Vec<u64> {
    let xmin: String = px
        .sql()
        .query_one("SELECT pg_snapshot_xmin(pg_current_snapshot())::text", &[])
        .unwrap()
        .get(0);
    let xmin: u64 = xmin.parse().unwrap();
    wanted.iter().copied().filter(|t| *t < xmin).collect()
}

#[test]
fn the_feed_sorts_transaction_numbers_as_numbers_and_walks_every_entry_once() {
    // The numbers of 9, 10, 11 and 98 to 101 straddle a power of ten. The feed once sorted the
    // text of the number ('10' < '9') while its cursor compared numbers, so a page that ended on
    // 11 never came back for 9, and a page that ended on 9 gave 10 and 11 again.
    let Some(px) = Px::new() else {
        return;
    };
    let wanted = below_the_horizon(&px, &[8, 9, 10, 11, 98, 99, 100, 101]);
    assert!(
        wanted.len() >= 4,
        "the server's transaction counter is too low for this test: {wanted:?}"
    );
    // Written in an order that is neither the order of the numbers nor its reverse.
    let mut written = wanted.clone();
    written.rotate_left(3);
    written.swap(0, 2);
    for (i, t) in written.iter().enumerate() {
        raw_entry(&px, *t, &format!("r{i}"));
    }
    for size in [1, 2, 3, 7, 100] {
        let mut got = Vec::new();
        let mut cursor = None;
        loop {
            let page = px.feed(cursor.as_ref(), size);
            if page.rows.is_empty() {
                break;
            }
            assert!(page.rows.len() <= size);
            got.extend(page.rows.iter().map(|r| r.txid as u64));
            cursor = page.next;
        }
        assert_eq!(got, wanted, "walked in pages of {size}");
    }
}

#[test]
fn the_feed_is_served_by_its_index_and_never_sorted() {
    let Some(px) = Px::new() else {
        return;
    };
    // A write makes the partitions of the month, so that the plan is the one of a real table.
    px.ok("ann", &[ins("deal", "d1", json!({"name": "A"}))]);
    let mut c = px.sql();
    c.batch_execute("SET enable_seqscan = off; SET enable_sort = off; SET enable_bitmapscan = off")
        .unwrap();
    c.batch_execute(&format!(
        "PREPARE feed(text, bigint, text[], text[], bigint, bigint) AS {}",
        crate::pg_hist::feed_sql()
    ))
    .unwrap();
    let plan: Vec<String> = c
        .query(
            "EXPLAIN (COSTS OFF) EXECUTE feed('0', 0, NULL, NULL, NULL, 100)",
            &[],
        )
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    let nodes: Vec<&str> = plan
        .iter()
        .map(|l| {
            l.trim_start_matches(|c| c == ' ' || c == '-' || c == '>')
                .trim()
        })
        .collect();
    // A node that sorts is a line that is only `Sort`, `Incremental Sort` or `Sort` and a method
    // (a Merge Append prints a `Sort Key:` line without sorting anything).
    assert!(
        !nodes
            .iter()
            .any(|n| *n == "Sort" || *n == "Incremental Sort"),
        "the feed sorts: {plan:#?}"
    );
    assert!(
        nodes.iter().any(|n| n.starts_with("Index Scan")),
        "the feed does not read its index: {plan:#?}"
    );
    assert!(
        !nodes.iter().any(|n| n.starts_with("Seq Scan")),
        "the feed reads a table in full: {plan:#?}"
    );
}

#[test]
fn no_order_by_names_an_output_column() {
    // The source of the part, without its comments: every key of every ORDER BY is either one of
    // the columns that are the same in the output (`seq`, `at`), or names its table or alias
    // (`obop_record_change.txid`, `x.ord`). A bare name binds to an output column first.
    let source = include_str!("pg_hist.rs");
    let code: String = source
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut found = 0;
    for part in code.split("ORDER BY").skip(1) {
        found += 1;
        let clause: String = part
            .chars()
            .take_while(|c| !matches!(c, '"' | ')' | ';' | '\n'))
            .collect();
        let clause = clause.split(" LIMIT").next().unwrap_or("");
        for key in clause.split(',') {
            let key = key.trim();
            let name = key.split_whitespace().next().unwrap_or("");
            assert!(
                name.contains('.') || matches!(name, "seq" | "at" | "{FEED_ORDER}" | ""),
                "ORDER BY {key:?} may bind to an output column"
            );
            assert_ne!(name, "txid", "a bare txid sorts the text of the number");
        }
    }
    assert!(found >= 4, "found only {found} ORDER BY clauses");
    assert!(
        crate::pg_hist::feed_sql()
            .contains("ORDER BY obop_record_change.txid, obop_record_change.seq LIMIT"),
        "the feed names the table in its sort keys"
    );
    // And the transaction number is read under a name of its own.
    assert!(code.contains("txid::text AS txid_text"));
    assert!(!code.contains("txid::text\""));
}
