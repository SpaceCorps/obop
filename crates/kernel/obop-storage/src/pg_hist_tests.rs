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

/// How long a test waits for a condition: `crate::testing::time_budget`.
fn budget(secs: u64) -> std::time::Duration {
    crate::testing::time_budget(secs)
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
        let until = std::time::Instant::now() + budget(60);
        loop {
            let page = self.feed_raw(after, limit);
            if page.horizon_lag_ms.is_none() {
                return page;
            }
            assert!(
                std::time::Instant::now() < until,
                "the feed stayed held back"
            );
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }

    /// Wait until the statistics say nothing is held back (other databases' transactions on the
    /// server count too).
    fn settled(&self) {
        let until = std::time::Instant::now() + budget(60);
        while self.b.history_stats().unwrap().horizon_lag_ms != 0 {
            assert!(
                std::time::Instant::now() < until,
                "something stayed held back"
            );
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }

    /// The month `back` months before the server's own date (UTC), as `YYYY-MM`. The function
    /// that drops a month reads the server's clock, which a test cannot move; a test that drops
    /// months works from the server's date and stamps its entries inside the months it names.
    fn server_month(&self, back: i32) -> String {
        self.sql()
            .query_one(
                "SELECT to_char(date_trunc('month', now() AT TIME ZONE 'UTC')
                                - $1::int * interval '1 month', 'YYYY-MM')",
                &[&back],
            )
            .unwrap()
            .get(0)
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
fn the_wait_before_the_install_is_tried_again_depends_on_what_went_wrong() {
    use crate::pg_hist::install_backoff;
    use std::time::Duration;
    // A fault of the store is over in moments; a refusal by the server is not.
    for why in [
        "storage_timeout: a database statement ran into its time limit",
        "storage_busy: every database connection is in use",
        "storage_unavailable: the database could not be reached",
    ] {
        assert_eq!(install_backoff(why), Duration::from_secs(1), "{why}");
    }
    for why in [
        "db error (SQLSTATE 42501): permission denied for schema public",
        "a text that merely mentions storage_busy: here",
    ] {
        assert_eq!(install_backoff(why), Duration::from_secs(30), "{why}");
    }
}

#[test]
fn a_role_that_may_not_create_is_refused_with_the_reason_and_its_request_goes_on() {
    let Some(mut db) = TestDb::new() else {
        return;
    };
    let rt = serving_role_of_an_older_database(&mut db);
    let pg = Arc::new(Pg::connect_options(&rt, Some(one()), Ddl::IfMissing).unwrap());
    // The stack of the binary: the guard refuses writes after a fault of the same request.
    let b = Tracked::new(guarded(pg.clone()));
    b.track("deal", history_of(&["name"]));
    // A request scope, as the HTTP layer opens one: the first write that keeps history tries to
    // install the part, fails, and says why. A refusal by the server (a permission) is no fault
    // of the store and notes none; the isolation of the attempt itself is tested with a failure
    // that is one (`a_failure_of_the_install_that_is_a_fault_...`).
    fault::begin_request(fault::Outcome::default());
    let ctx = WriteCtx::new("ann", "rest", "deal.create");
    b.apply_ctx(&ctx, &[ins("deal", "d1", json!({"name": "A"}))])
        .unwrap_err();
    assert_eq!(
        fault::noted(),
        None,
        "the refusal of the install is not a storage fault"
    );
    // A refusal by the server does not change by itself: the next try is far off (the value, not
    // a wait; it only shrinks, and 25 seconds is more than a test takes).
    let wait = pg.hist.install_retry_in().expect("a time to try again");
    assert!(wait > std::time::Duration::from_secs(25), "{wait:?}");
    // The request goes on: a write that keeps no history is not refused by the guard.
    b.apply_ctx(&ctx, &[ins("note", "n1", json!({"x": 1}))])
        .unwrap();
    assert_eq!(fault::end_request().fault, None);
}

#[test]
fn a_failure_of_the_install_that_is_a_fault_is_isolated_from_the_attempt_and_refuses_the_write_with_its_code()
 {
    let Some(db) = TestDb::new() else {
        return;
    };
    // The statement that waits for the schema lock runs into its limit: a storage fault, the kind
    // that is noted on the request that meets it. The limit is that of every batch, the write
    // that is refused later included, so it is not made so short that a server under load (a
    // whole workspace of tests at once) cuts the write itself short.
    let cfg = PgPoolConfig {
        batch_ms: Some(2_000),
        ..PgPoolConfig::one_shot()
    };
    let pg = Arc::new(Pg::connect_options(db.url(), Some(cfg), Ddl::IfMissing).unwrap());
    let b = Tracked::new(guarded(pg.clone()));
    b.track("deal", history_of(&["name"]));
    // Somebody else holds the lock of the schema work (another process starting, a migration).
    let mut holder = db.connect();
    holder
        .batch_execute("SELECT pg_advisory_lock(hashtextextended('obop-schema', 0))")
        .unwrap();
    // The attempt, on its own: it fails with a fault, and the request that asked for it does not
    // carry that fault (the attempt is housekeeping on nobody's behalf).
    fault::begin_request(fault::Outcome::default());
    pg.hist.install_on_demand(&pg.main);
    assert_eq!(fault::noted(), None, "the attempt marked the request");
    let why = pg.hist.install_problem().expect("the attempt failed");
    assert_eq!(
        fault::Fault::of_message(&why),
        Some(fault::Fault::Timeout),
        "{why}"
    );
    // A fault of the store is over in moments: the next try is at most a second away (the value
    // itself, not a wait: the remaining time only shrinks).
    let wait = pg.hist.install_retry_in().expect("a time to try again");
    assert!(wait <= std::time::Duration::from_secs(1), "{wait:?}");
    fault::end_request();
    // The write that needs the part is refused with the code of that fault, which is the
    // request's own: it was refused for it, and a 503 that can be tried again is the answer.
    fault::begin_request(fault::Outcome::default());
    let ctx = WriteCtx::new("ann", "rest", "deal.create");
    let e = b
        .apply_ctx(&ctx, &[ins("deal", "d1", json!({"name": "A"}))])
        .unwrap_err();
    let ApplyError::Storage(m) = e else {
        panic!("{e:?}");
    };
    assert_eq!(
        fault::Fault::of_message(&m),
        Some(fault::Fault::Timeout),
        "the text starts with the code: {m}"
    );
    assert!(m.contains("not installed yet"), "{m}");
    assert_eq!(fault::noted(), Some(fault::Fault::Timeout));
    assert!(!fault::may_have_applied(&m), "nothing of it was applied");
    assert!(b.get("deal", "d1").unwrap().is_none());
    fault::end_request();
    // The lock is let go: the next write, in a request of its own, makes the part (the wait is
    // short, not the 30 seconds of a refusal by the server) and goes through.
    holder
        .batch_execute("SELECT pg_advisory_unlock_all()")
        .unwrap();
    let until = std::time::Instant::now() + budget(30);
    loop {
        fault::begin_request(fault::Outcome::default());
        let r = b.apply_ctx(&ctx, &[ins("deal", "d1", json!({"name": "A"}))]);
        fault::end_request();
        match r {
            Ok(_) => break,
            Err(e) => {
                assert!(
                    std::time::Instant::now() < until,
                    "the install was not tried again: {e:?}"
                );
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        }
    }
    assert_eq!(
        b.history(&HistoryQuery::of("deal", "d1", 10))
            .unwrap()
            .len(),
        1
    );
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
    // (Stamped inside the month the test drops: the clock of the fixture is in October 2026.)
    c.batch_execute(
        "INSERT INTO obop_record_change (at, model, record_id, op, actor, row_version, changed)
         VALUES (timestamptz '2026-10-27 12:00:00+00', 'deal', 'd9', 'update', 'ann', 1, '{}')",
    )
    .unwrap();
    assert_eq!(px.placed().len(), 3);
    assert_eq!(px.placed()[2].1, "obop_record_change_p202610");
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
    // The months are the server's, counted back from its date, and the entries are stamped inside
    // them (see `server_month`): the test says the same on any day of any year.
    let this = px.server_month(0);
    let old = px.server_month(3);
    let name = |m: &str| format!("obop_record_change_p{}", m.replace('-', ""));
    px.set_clock(&format!("{old}-10T10:00:00.000000Z"));
    px.ok("ann", &[ins("deal", "d1", json!({"name": "A"}))]);
    px.ok("ann", &[merge("deal", "d1", json!({"name": "B"}))]);
    assert!(px.partitions().contains(&name(&old)));
    // The month ended two months ago at the earliest: it is not 400 days old.
    let e = px.b.history_drop_partition(&old, 400).unwrap_err();
    assert!(e.contains("younger than the retention"), "{e}");
    assert_eq!(px.placed().len(), 2);
    // This month is never old enough, whatever the age asked for.
    px.b.history_ensure_partition(&this).unwrap();
    let e = px.b.history_drop_partition(&this, 0).unwrap_err();
    assert!(e.contains("younger than the retention"), "{e}");
    assert!(px.partitions().contains(&name(&this)));
    // The month three months back is old enough for 30 days.
    assert_eq!(px.b.history_drop_partition(&old, 30).unwrap(), 2);
    assert!(!px.partitions().contains(&name(&old)));
    assert!(px.placed().is_empty());
    // Dropping what is not there is not an error and drops nothing.
    assert_eq!(px.b.history_drop_partition(&old, 30).unwrap(), 0);
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
    // The longest a single write took, in milliseconds.
    let worst = std::sync::atomic::AtomicU64::new(0);
    std::thread::scope(|s| {
        let writers: Vec<_> = (0..4)
            .map(|t| {
                let (px, stop, writes, worst) = (&px, &stop, &writes, &worst);
                s.spawn(move || {
                    let mut i = 0;
                    while !stop.load(SeqCst) {
                        let began = std::time::Instant::now();
                        px.ok(
                            "ann",
                            &[
                                ins("deal", &format!("w{t}-{i}"), json!({"name": "A"})),
                                merge("deal", "start", json!({"n": t * 1000 + i})),
                            ],
                        );
                        worst.fetch_max(began.elapsed().as_millis() as u64, SeqCst);
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
    // A move that held the writers for as long as it took would show here: a write waits for
    // the pool at most two seconds and for the server at most a minute, and a hundred rows move in
    // a few milliseconds. The bound is generous (a loaded runner) and far below the minute.
    let worst = worst.load(SeqCst);
    assert!(
        worst < budget(15).as_millis() as u64,
        "the slowest of {n} writes took {worst} ms while the partitions were made"
    );
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
    // Its number lies between the oldest open transaction and the next number the cluster gives
    // out: held back by that transaction, and not "foreign" (numbered beyond the cluster).
    assert_eq!(held.foreign_entries, Some(0), "{held:?}");
    assert_eq!(px.b.history_stats().unwrap().foreign_rows, 0);
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
    assert_eq!(page.foreign_entries, Some(0), "{page:?}");
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
        while seen.len() < 300 && started.elapsed() < budget(60) {
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
    assert!(started.elapsed() < budget(5));
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
    documented_grants(&mut px.sql(), &role, true);
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
    // partitions it may not make (it has no EXECUTE on the function) are not needed, the entries
    // wait in the default partition.
    let fresh = TestDb::new().unwrap();
    crate::schema::owner_install(fresh.url()).unwrap();
    let mut fresh = fresh;
    let url = fresh
        .role("restorer", "NOSUPERUSER NOCREATEDB NOCREATEROLE")
        .unwrap();
    let role = fresh.role_name("restorer");
    documented_grants(&mut fresh.connect(), &role, true);
    fresh
        .connect()
        .batch_execute(&format!(
            "REVOKE EXECUTE ON FUNCTION obop_history_ensure_partition(date) FROM {role}"
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
    documented_grants(&mut empty.connect(), &role, true);
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

// ---- the part is hard to misuse, and the grants are what the docs say -----------------------------------

#[test]
fn a_minimum_age_that_is_missing_or_negative_never_lets_a_partition_go() {
    let Some(px) = Px::new() else {
        return;
    };
    let this = px.server_month(0);
    px.b.history_ensure_partition(&this).unwrap();
    // An entry in the month that is not over, whatever the day: what a drop would take with it.
    px.set_clock(&format!("{this}-01T12:00:00.000000Z"));
    px.ok("ann", &[ins("deal", "d1", json!({"name": "A"}))]);
    let part = format!("obop_record_change_p{}", this.replace('-', ""));
    assert_eq!(px.placed().len(), 1);
    assert_eq!(px.placed()[0].1, part);
    let mut c = px.sql();
    // What the Rust side cannot send (it passes days as a number) but any other caller can.
    for (what, call) in [
        ("a missing age", format!("date '{this}-01', NULL::interval")),
        (
            "a negative age",
            format!("date '{this}-01', interval '-60 days'"),
        ),
        (
            "a negative month age",
            format!("date '{this}-01', interval '-2 months'"),
        ),
        (
            "a missing month",
            "NULL::date, interval '1 day'".to_string(),
        ),
        // Two intervals are compared with a month counted as 30 days and subtracted from an
        // instant with calendar months, so an age of mixed signs is "not negative" to the first
        // and moves the floor into the future for the second (about half a day per month of the
        // argument, so ten years of months reach the current month on any day).
        (
            "ten years of months against ten years of days",
            format!("date '{this}-01', interval '-120 months 3600 days'"),
        ),
        (
            "a year of months against a year of days",
            format!("date '{this}-01', interval '-12 months 360 days'"),
        ),
        (
            "a hundred years against a hundred years of days",
            format!("date '{this}-01', interval '-1200 months 36000 days'"),
        ),
        (
            "minus infinity",
            format!("date '{this}-01', interval '-infinity'"),
        ),
    ] {
        let e = c
            .query_one(&format!("SELECT obop_history_drop_partition({call})"), &[])
            .unwrap_err();
        assert!(
            e.as_db_error().is_some_and(|d| d.code().code() == "22023"),
            "{what}: {e}"
        );
        assert!(
            px.partitions().contains(&part),
            "{what}: the month is still there"
        );
    }
    // Ages that are fine as ages but cannot drop the month that is not over: whatever the reason
    // the call gives (the floor is not reached, or the arithmetic runs out of years), it drops
    // nothing.
    for (what, age) in [
        (
            "a month of days against a month",
            "interval '-1 month 31 days'",
        ),
        ("a month of days", "interval '1 month -31 days'"),
        ("an age of infinity", "interval 'infinity'"),
        ("more months than there are", "interval '2000000000 months'"),
        ("more years than there are", "interval '178000000 years'"),
    ] {
        let e = c
            .query_one(
                &format!("SELECT obop_history_drop_partition(date '{this}-01', {age})"),
                &[],
            )
            .map(|_| ())
            .unwrap_err();
        assert!(e.as_db_error().is_some(), "{what}: {e}");
        assert!(
            px.partitions().contains(&part),
            "{what}: the month is still there"
        );
        assert_eq!(px.placed().len(), 1, "{what}: and so is its entry");
    }
    // The floor itself: the month is not over, so not even an age of nothing drops it.
    let e = c
        .query_one(
            &format!("SELECT obop_history_drop_partition(date '{this}-01', interval '0')"),
            &[],
        )
        .unwrap_err();
    assert!(
        e.as_db_error()
            .is_some_and(|d| d.message().contains("younger than the retention")),
        "{e}"
    );
    assert!(px.partitions().contains(&part));
    assert_eq!(px.placed().len(), 1);
    // And a month that is asked for as NULL is not made either.
    let e = c
        .query_one("SELECT obop_history_ensure_partition(NULL::date)", &[])
        .unwrap_err();
    assert!(
        e.as_db_error()
            .is_some_and(|d| d.message().contains("outside the range")),
        "{e}"
    );
}

#[test]
fn the_table_refuses_an_entry_whose_changed_or_via_is_not_an_object() {
    let Some(px) = Px::new() else {
        return;
    };
    px.ok("ann", &[ins("deal", "d1", json!({"name": "A"}))]);
    px.ok("ann", &[merge("deal", "d1", json!({"name": "B"}))]);
    assert_eq!(px.placed()[0].1, "obop_record_change_p202610");
    let mut c = px.sql();
    let in_a_month = "timestamptz '2026-10-27 12:00:00+00'";
    let in_the_default = "timestamptz '2001-01-01 00:00:00+00'";
    for (what, at, via, changed) in [
        ("an array in a month", in_a_month, "'{}'", "'[]'"),
        ("a number in a month", in_a_month, "'{}'", "'1'"),
        ("a null in a month", in_a_month, "'{}'", "'null'"),
        ("a string as via", in_a_month, "'\"x\"'", "'{}'"),
        (
            "an array in the default partition",
            in_the_default,
            "'{}'",
            "'[]'",
        ),
        (
            "an array as via in the default",
            in_the_default,
            "'[]'",
            "'{}'",
        ),
    ] {
        let e = c
            .batch_execute(&format!(
                "INSERT INTO obop_record_change (at, model, record_id, op, actor, via, row_version, changed)
                 VALUES ({at}, 'deal', 'd1', 'update', 'mallory', {via}, 9, {changed})"
            ))
            .unwrap_err();
        assert!(
            e.as_db_error().is_some_and(|d| d.code().code() == "23514"),
            "{what}: {e}"
        );
    }
    assert_eq!(px.placed().len(), 2, "nothing was added");
    // So the erasure of the record's history can always be completed.
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
}

#[test]
fn a_function_of_the_same_name_in_the_history_schema_does_not_take_the_place_of_a_builtin() {
    let Some(px) = Px::new() else {
        return;
    };
    px.ok(
        "ann",
        &[ins("deal", "d1", json!({"name": "A", "stage": "x"}))],
    );
    // Whoever may create objects in the schema leaves functions that take exactly the types the
    // maintenance functions call the built-ins with: exact matches win over the casts the
    // built-ins need, wherever they stand in the search path.
    px.sql()
        .batch_execute(
            "CREATE FUNCTION public.to_char(date, text) RETURNS text LANGUAGE plpgsql AS $$
               BEGIN RAISE EXCEPTION 'the decoy to_char(date, text) ran'; END $$;
             CREATE FUNCTION public.format(text, text) RETURNS text LANGUAGE plpgsql AS $$
               BEGIN RAISE EXCEPTION 'the decoy format(text, text) ran'; END $$;
             CREATE FUNCTION public.format(text, text, text) RETURNS text LANGUAGE plpgsql AS $$
               BEGIN RAISE EXCEPTION 'the decoy format(text, text, text) ran'; END $$;
             CREATE FUNCTION public.to_jsonb(text[]) RETURNS jsonb LANGUAGE plpgsql AS $$
               BEGIN RAISE EXCEPTION 'the decoy to_jsonb(text[]) ran'; END $$",
        )
        .unwrap();
    // A month made and brought home, an erasure, and a month dropped: every function.
    assert!(px.b.history_ensure_partition("2026-06").unwrap().created);
    px.ok(
        "ann",
        &[Op::EraseHistory {
            model: "deal".into(),
            id: "d1".into(),
        }],
    );
    // A month that no write of this test touches (the fake clock is in October 2026), long over
    // on any date this test can run on: dropping the server's own "three months back" would take
    // the entries of the fake clock's month in January 2027.
    assert_eq!(px.b.history_drop_partition("2026-06", 30).unwrap(), 0);
    // And an ordinary write: the partitions of the month it is in.
    px.ok("ann", &[ins("deal", "d2", json!({"name": "A"}))]);
    assert_eq!(px.placed().len(), 3);
}

/// What docs/DEPLOY.md lists for a serving role that does not own the tables: the text of the
/// document itself, with the names of the example filled in, so that the document and the test
/// cannot say different things.
fn documented_grants(owner: &mut postgres::Client, role: &str, restore: bool) {
    let me: String = owner
        .query_one("SELECT quote_ident(current_user)", &[])
        .unwrap()
        .get(0);
    let sql = deploy_sql("**Grants for a serving role that is not the owner.**")
        .replace("obop_serving", role)
        .replace("obop_owner", &me);
    owner.batch_execute(&sql).unwrap();
    if restore {
        owner
            .batch_execute(&format!(
                "GRANT UPDATE ON SEQUENCE obop_record_change_seq TO {role}"
            ))
            .unwrap();
    }
}

#[test]
fn a_serving_role_with_exactly_the_documented_grants_writes_reads_the_feed_counts_and_backs_up() {
    let Some(mut px) = Px::new() else {
        return;
    };
    px.ok("ann", &[ins("deal", "d1", json!({"name": "A"}))]);
    let rt = px
        .db
        .role("serving", "NOSUPERUSER NOCREATEDB NOCREATEROLE")
        .unwrap();
    let role = px.db.role_name("serving");
    documented_grants(&mut px.sql(), &role, false);
    let theirs = Tracked::new(guarded(Arc::new(
        Pg::connect_options(&rt, Some(one()), Ddl::IfMissing).unwrap(),
    )));
    theirs.track("deal", history_of(&["name"]));
    let ctx = WriteCtx::new("bob", "rest", "deal.update");
    // Writes: the entries, and the partitions a write makes through the maintenance function.
    theirs
        .apply_ctx(&ctx, &[ins("deal", "d2", json!({"name": "B"}))])
        .unwrap();
    theirs
        .apply_ctx(&ctx, &[merge("deal", "d2", json!({"name": "C"}))])
        .unwrap();
    assert_eq!(
        theirs
            .history(&HistoryQuery::of("deal", "d2", 10))
            .unwrap()
            .len(),
        2
    );
    // The feed, and a cursor followed (the call that reads the newest number given out, which
    // needs no more than the USAGE the writers have).
    let ask = |after: Option<crate::history::ChangeCursor>| {
        let until = std::time::Instant::now() + budget(60);
        loop {
            let page = theirs
                .changes(&ChangeQuery {
                    after: after.clone(),
                    limit: 10,
                    ..ChangeQuery::default()
                })
                .unwrap();
            if page.horizon_lag_ms.is_none() {
                return page;
            }
            assert!(
                std::time::Instant::now() < until,
                "the feed stayed held back"
            );
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    };
    let page = ask(None);
    assert_eq!(page.rows.len(), 3);
    assert!(ask(page.next.clone()).rows.is_empty());
    // A month made after the grants is counted: the default privileges of the owner reach it.
    let month = px.server_month(2);
    theirs.history_ensure_partition(&month).unwrap();
    let stats = theirs.history_stats().unwrap();
    let made = format!("obop_record_change_p{}", month.replace('-', ""));
    assert!(stats.partitions.iter().any(|p| p.0 == made), "{stats:?}");
    assert_eq!(stats.default_rows, 0);
    // A backup reads it all, and the total is what it counted.
    let mut n = 0;
    theirs
        .export(100, &mut |rows| {
            n += rows.len();
            Ok(())
        })
        .unwrap();
    assert_eq!(n, theirs.total().unwrap());
    // Without the right to read a partition the statistics say what is missing.
    px.sql()
        .batch_execute(&format!(
            "REVOKE SELECT ON obop_record_change_default FROM {role}"
        ))
        .unwrap();
    let e = theirs.history_stats().unwrap_err();
    assert!(
        e.contains("grant SELECT on the partitions") && e.contains("obop_record_change_default"),
        "{e}"
    );
}

#[test]
fn a_cursor_is_bound_to_the_cluster_and_to_the_numbers_it_counts_with() {
    let Some(px) = Px::new() else {
        return;
    };
    px.ok("ann", &[ins("deal", "d1", json!({"name": "A"}))]);
    px.ok("ann", &[merge("deal", "d1", json!({"name": "B"}))]);
    let cursor = px
        .feed(None, 1)
        .next
        .expect("a cursor after the first entry");
    // The epoch is the stored one, followed by a digest of the identity of the cluster and the
    // time line it is on, where the role may read the control data.
    let mut c = px.sql();
    let stored: String = c
        .query_one(
            "SELECT value FROM obop_kv WHERE key = 'obop.history.epoch'",
            &[],
        )
        .unwrap()
        .get(0);
    let identity: Option<String> = c
        .query_one(
            "SELECT CASE WHEN has_function_privilege('pg_control_system()', 'EXECUTE')
                          AND has_function_privilege('pg_control_checkpoint()', 'EXECUTE')
                         THEN (SELECT system_identifier::text FROM pg_control_system())
                              || '/' || (SELECT timeline_id::text FROM pg_control_checkpoint())
                    END",
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(
        cursor.epoch,
        crate::pg_hist::epoch_of(&stored, identity.as_deref())
    );
    if identity.is_some() {
        assert_ne!(cursor.epoch, stored, "the cluster is part of the epoch");
        // A cursor of this database that was made without the cluster in it is refused.
        let bare = crate::history::ChangeCursor {
            epoch: stored.clone(),
            ..cursor.clone()
        };
        let e =
            px.b.changes(&ChangeQuery {
                after: Some(bare),
                limit: 10,
                ..ChangeQuery::default()
            })
            .unwrap_err();
        assert!(e.starts_with("cursor_expired"), "{e}");
    }
    // The cursor of this cluster is followed to the end.
    assert_eq!(px.feed(Some(&cursor), 10).rows.len(), 1);
    // A cursor with a transaction number this cluster has not given out comes from another one
    // (a dump loaded into a new server counts again from low numbers), whatever its epoch says.
    let from_afar = crate::history::ChangeCursor {
        txid: i64::MAX / 2,
        ..cursor.clone()
    };
    let e =
        px.b.changes(&ChangeQuery {
            after: Some(from_afar),
            limit: 10,
            ..ChangeQuery::default()
        })
        .unwrap_err();
    assert!(
        e.starts_with("cursor_expired") && e.contains("another time line"),
        "{e}"
    );
}

#[test]
fn a_role_that_may_not_read_the_control_data_still_reads_the_feed_by_the_numbers_alone() {
    let Some(mut px) = Px::new() else {
        return;
    };
    px.ok("ann", &[ins("deal", "d1", json!({"name": "A"}))]);
    px.ok("ann", &[merge("deal", "d1", json!({"name": "B"}))]);
    let rt = px
        .db
        .role("serving", "NOSUPERUSER NOCREATEDB NOCREATEROLE")
        .unwrap();
    let role = px.db.role_name("serving");
    documented_grants(&mut px.sql(), &role, false);
    // A server whose owner withdrew the control data from everybody (the function privileges
    // are those of this database's catalogue, so the scratch database is the only one affected).
    px.sql()
        .batch_execute(
            "REVOKE EXECUTE ON FUNCTION pg_control_system() FROM PUBLIC;
             REVOKE EXECUTE ON FUNCTION pg_control_checkpoint() FROM PUBLIC",
        )
        .unwrap();
    let theirs = Tracked::new(guarded(Arc::new(
        Pg::connect_options(&rt, Some(one()), Ddl::IfMissing).unwrap(),
    )));
    let ask = |after: Option<crate::history::ChangeCursor>, limit: usize| {
        let until = std::time::Instant::now() + budget(60);
        loop {
            let page = theirs
                .changes(&ChangeQuery {
                    after: after.clone(),
                    limit,
                    ..ChangeQuery::default()
                })
                .unwrap_or_else(|e| panic!("the feed needs the control data: {e}"));
            if page.horizon_lag_ms.is_none() || page.rows.len() == limit {
                return page;
            }
            assert!(
                std::time::Instant::now() < until,
                "the feed stayed held back"
            );
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    };
    let first = ask(None, 1);
    assert_eq!(first.rows.len(), 1);
    let cursor = first.next.clone().expect("a cursor");
    // No identity to bind to: the epoch is the stored one, and the numbers are all there is.
    let stored: String = px
        .sql()
        .query_one(
            "SELECT value FROM obop_kv WHERE key = 'obop.history.epoch'",
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(cursor.epoch, stored);
    assert_eq!(ask(Some(cursor.clone()), 10).rows.len(), 1);
    // A cursor with a number this cluster has not given out is refused all the same.
    let from_afar = crate::history::ChangeCursor {
        txid: i64::MAX / 2,
        ..cursor
    };
    let e = theirs
        .changes(&ChangeQuery {
            after: Some(from_afar),
            limit: 10,
            ..ChangeQuery::default()
        })
        .unwrap_err();
    assert!(
        e.starts_with("cursor_expired") && e.contains("another time line"),
        "{e}"
    );
}

/// The first fenced `sql` block of docs/DEPLOY.md after `heading`: the text an operator is told to
/// run, so that a test can run exactly that.
fn deploy_sql(heading: &str) -> String {
    let doc = include_str!("../../../../docs/DEPLOY.md").replace(
        "
", "
",
    );
    let at = doc
        .find(heading)
        .unwrap_or_else(|| panic!("docs/DEPLOY.md has no {heading:?}"));
    let rest = &doc[at..];
    let open = rest
        .find(
            "```sql
",
        )
        .expect("a block of sql after it")
        + "```sql
"
        .len();
    let close = rest[open..].find("```").expect("the end of the block");
    rest[open..open + close].to_string()
}

#[test]
fn entries_numbered_beyond_the_cluster_are_foreign_and_the_documented_renumbering_brings_them_home()
{
    let Some(px) = Px::new() else {
        return;
    };
    // What a dump and a load into a cluster with a lower transaction counter leaves: entries
    // that carry the numbers of the other cluster, and entries written here since.
    px.ok("ann", &[ins("deal", "d1", json!({"name": "A"}))]);
    px.ok("ann", &[merge("deal", "d1", json!({"name": "B"}))]);
    px.sql()
        .batch_execute(
            "BEGIN;
             SET LOCAL obop.history_maint = 'on';
             UPDATE obop_record_change
                SET txid = (pg_snapshot_xmax(pg_current_snapshot())::text::bigint + 1000000)::text::xid8;
             COMMIT",
        )
        .unwrap();
    px.ok("ann", &[merge("deal", "d1", json!({"name": "C"}))]);
    let old_cursor = px.feed(None, 1).next.expect("a cursor");
    // The sequence of a cluster that was filled by replication does not carry the numbers.
    px.sql()
        .batch_execute("ALTER SEQUENCE obop_record_change_seq RESTART WITH 1")
        .unwrap();

    // The feed shows what is numbered here, and says plainly what it hides: not as a transaction
    // that is still open (nothing is), and not for ever (it would be delivered after newer
    // entries of the same record once the counter passed it).
    px.settled();
    let page = px.feed_raw(None, 10);
    assert_eq!(page.rows.len(), 1, "{page:?}");
    assert_eq!(page.rows[0].changed["name"]["a"], json!("C"));
    assert_eq!(page.horizon_lag_ms, None, "{page:?}");
    assert_eq!(page.foreign_entries, Some(2), "{page:?}");
    let stats = px.b.history_stats().unwrap();
    assert_eq!(
        (stats.foreign_rows, stats.horizon_lag_ms),
        (2, 0),
        "{stats:?}"
    );
    // A full page has not looked.
    assert_eq!(px.feed_raw(None, 1).foreign_entries, None);
    // The inspection warns, naming the remedy.
    let found = crate::schema::inspect(&mut px.sql()).unwrap();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(
        (found[0].code, found[0].object.as_str(), found[0].error),
        ("history_foreign_numbers", "history", false)
    );
    assert!(
        found[0].detail.contains("2 entries") && found[0].detail.contains("docs/DEPLOY.md"),
        "{}",
        found[0].detail
    );

    // The remedy of docs/DEPLOY.md, as it is written there, run by the owner of the table.
    px.sql()
        .batch_execute(&deploy_sql("**Moving a database to another cluster.**"))
        .unwrap();
    px.settled();
    let page = px.feed(None, 10);
    assert_eq!(page.foreign_entries, Some(0), "{page:?}");
    // Every entry has number 0 now, as after a restore: the feed is in the order of the entry
    // numbers, which is the order the changes were made in, those of the other cluster first.
    let names: Vec<_> = page
        .rows
        .iter()
        .map(|r| (r.txid, r.changed["name"]["a"].clone()))
        .collect();
    assert_eq!(
        names,
        vec![(0, json!("A")), (0, json!("B")), (0, json!("C"))]
    );
    assert_eq!(px.b.history_stats().unwrap().foreign_rows, 0);
    assert!(crate::schema::inspect(&mut px.sql()).unwrap().is_empty());
    // The cursors made before are over (the epoch was renewed), as are those of the other cluster.
    let e =
        px.b.changes(&ChangeQuery {
            after: Some(old_cursor),
            limit: 10,
            ..ChangeQuery::default()
        })
        .unwrap_err();
    assert!(e.starts_with("cursor_expired"), "{e}");
    // The sequence is past the numbers that were there, so history by record is in order.
    px.ok("ann", &[merge("deal", "d1", json!({"name": "D"}))]);
    let seqs: Vec<i64> = px.placed().iter().map(|p| p.0).collect();
    assert_eq!(seqs, vec![1, 2, 3, 4], "no number is given twice");
    let record =
        px.b.history(&HistoryQuery::of("deal", "d1", 10))
            .unwrap()
            .iter()
            .map(|r| r.changed["name"]["a"].clone())
            .collect::<Vec<_>>();
    assert_eq!(
        record,
        vec![json!("D"), json!("C"), json!("B"), json!("A")],
        "newest first"
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
    // fails, the SQL of the history part changed: a database that already has the part does not
    // get the change (the SQL runs only when the part is missing), so a released part needs a
    // ledger step that installs the new text, to be added with the new value. The part has not
    // been released: a database that a development build made it in has to drop the objects and
    // let them be made again (docs/DEPLOY.md, "Rolling back").
    assert_eq!(
        d, "3747cb943b0cd47f463c41042a2b314c9c2f1a68c69326d710d1ea681028930c",
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

/// The part, working, in a schema of the given name (the search path of the database names it).
fn the_part_works_in_the_schema(name: &str) {
    let Some(db) = TestDb::new() else {
        return;
    };
    let ident = format!("\"{}\"", name.replace('"', "\"\""));
    let lit = ident.replace('\'', "''");
    db.connect()
        .batch_execute(&format!(
            "CREATE SCHEMA {ident}; ALTER DATABASE {} SET search_path TO {ident}, public",
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
                &format!(
                    "SELECT count(*) FILTER (WHERE relnamespace = '{lit}'::regnamespace),
                            count(*) FILTER (WHERE relnamespace = 'public'::regnamespace)
                       FROM pg_class WHERE relname LIKE 'obop_record_change%'"
                ),
                &[],
            )
            .unwrap();
        (r.get(0), r.get(1))
    };
    assert!(
        here >= 8 && elsewhere == 0,
        "{name}: {here} here, {elsewhere} in public"
    );
    let fns: i64 = c
        .query_one(
            &format!(
                "SELECT count(*) FROM pg_proc WHERE proname LIKE 'obop_history_%'
                    AND pronamespace = '{lit}'::regnamespace"
            ),
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(fns, 4);
    assert_eq!(
        c.query_one(
            &format!("SELECT count(*) FROM {ident}.obop_record_change_p202609"),
            &[]
        )
        .unwrap()
        .get::<_, i64>(0),
        0
    );
    // The default of the number column and the guards name the table of this schema.
    let (default, guards): (String, i64) = {
        let r = c
            .query_one(
                &format!(
                    "SELECT pg_get_expr(d.adbin, d.adrelid),
                            (SELECT count(*) FROM pg_trigger
                              WHERE tgrelid = '{lit}.obop_record_change'::regclass
                                AND NOT tgisinternal)
                       FROM pg_attrdef d
                      WHERE d.adrelid = '{lit}.obop_record_change'::regclass
                        AND d.adnum = (SELECT attnum FROM pg_attribute
                                        WHERE attrelid = d.adrelid AND attname = 'seq')"
                ),
                &[],
            )
            .unwrap();
        (r.get(0), r.get(1))
    };
    assert!(default.starts_with("nextval("), "{default}");
    assert_eq!(guards, 2);
}

#[test]
fn the_part_works_in_a_schema_of_an_odd_name() {
    the_part_works_in_the_schema("Odd Schema");
}

#[test]
fn the_part_works_in_a_schema_whose_name_has_quotes_and_dollars() {
    // A quote ends a string literal, a pair of dollars ends the body of a DO block: the name goes
    // into both places of the SQL of the part.
    the_part_works_in_the_schema("it's $$ a \"quoted\" $obop_do$ name");
}

#[test]
fn the_sql_of_the_part_puts_the_name_of_the_schema_where_it_belongs() {
    let render = |name: &str| (crate::pg_hist::PART.render)(name);
    let sql = render("\"it's $$ x\"");
    // No place is left, and none was filled in twice or with another one's text.
    assert!(!sql.contains("{schema") && !sql.contains("{do}"), "{sql}");
    // In a string literal the quote is doubled; as an identifier the name is as it was given.
    assert!(
        sql.contains("nextval('\"it''s $$ x\".obop_record_change_seq'::regclass)"),
        "{sql}"
    );
    assert!(sql.contains("CREATE SEQUENCE IF NOT EXISTS \"it's $$ x\".obop_record_change_seq"));
    // The body of the DO block is quoted with a tag the name does not contain.
    assert!(sql.contains("DO $obop_do$ BEGIN") && sql.contains("END $obop_do$;"));
    let sql = render("\"a$obop_do$b\"");
    assert!(sql.contains("DO $obop_dox$ BEGIN") && sql.contains("END $obop_dox$;"));
    // A name that looks like a place is a name.
    let sql = render("\"{schema}{do}\"");
    assert!(sql.contains("CREATE SEQUENCE IF NOT EXISTS \"{schema}{do}\".obop_record_change_seq"));
    // And "public" is what the digest is of.
    assert!(
        render("public").contains("CREATE SEQUENCE IF NOT EXISTS public.obop_record_change_seq")
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
        .map(|l| l.trim_start_matches([' ', '-', '>']).trim())
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
