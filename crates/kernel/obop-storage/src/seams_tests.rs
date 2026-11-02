//! The seams of wave 2 against a real PostgreSQL: the context reaching the transaction, the typed
//! refusal of the database's guards, the short read transaction, and the schema parts with and
//! without the right to create tables. Each test has a database of its own (`TestDb`) and returns
//! early, saying so, when `OBOP_TEST_POSTGRES_URL` is not set.

use super::*;
use crate::fault::{self, Fault};
use crate::pg::Pg;
use crate::pgpool::{Class, PgPoolConfig};
use crate::refusal::{self, RefusalCode};
use crate::testing::TestDb;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn fields(n: i64) -> Fields {
    let mut m = Fields::new();
    m.insert("n".into(), J::from(n));
    m
}

fn one() -> PgPoolConfig {
    PgPoolConfig::one_shot()
}

fn count_in(tx: &mut postgres::Transaction<'_>, model: &str) -> crate::pgpool::PR<i64> {
    Ok(tx
        .query_one(
            "SELECT count(*) FROM obop_record WHERE model = $1",
            &[&model],
        )?
        .get(0))
}

// ---- the typed refusal of the database's guards --------------------------------------------------

const REFUSING_TRIGGER: &str = r#"
    CREATE FUNCTION zz_refuse() RETURNS trigger LANGUAGE plpgsql AS $f$
    BEGIN
        IF NEW.model = 'frozen' THEN
            RAISE EXCEPTION USING ERRCODE = 'OBG01', MESSAGE = 'immutable_record',
                DETAIL = json_build_object('model', NEW.model, 'id', NEW.id,
                    'rule', 'immutable_when', 'fields', json_build_array('total'),
                    'detail', 'total cannot change while the state is posted')::text;
        ELSIF NEW.model = 'guarded' THEN
            RAISE EXCEPTION USING ERRCODE = 'OBG02', MESSAGE = 'guard_change_refused';
        END IF;
        RETURN NEW;
    END $f$;
    CREATE TRIGGER zz_refuse BEFORE INSERT ON obop_record
        FOR EACH ROW EXECUTE FUNCTION zz_refuse();"#;

#[test]
fn a_write_the_databases_guard_refuses_comes_back_as_a_typed_refusal_and_not_as_a_fault() {
    let Some(db) = TestDb::new() else {
        return;
    };
    let b = open_with(db.url(), Some(one())).unwrap();
    db.connect().batch_execute(REFUSING_TRIGGER).unwrap();
    fault::clear();

    // Through a single insert and through an atomic batch; the second is an ApplyError::Storage.
    let e = b.insert("frozen", "inv-1", &fields(1)).unwrap_err();
    let batch = b
        .apply_checked(&[Op::Insert {
            model: "frozen".into(),
            id: "inv-2".into(),
            fields: fields(1),
        }])
        .unwrap_err();
    let ApplyError::Storage(batch) = batch else {
        panic!("a refusal is not a conflict of the batch: {batch:?}");
    };
    for (err, id) in [(e, "inv-1"), (batch, "inv-2")] {
        let r = refusal::parse(&err).unwrap_or_else(|| panic!("not a refusal: {err}"));
        assert_eq!(r.code, RefusalCode::ImmutableRecord);
        assert_eq!(
            (r.model.as_str(), r.record.as_str(), r.rule.as_str()),
            ("frozen", id, "immutable_when")
        );
        assert_eq!(r.fields, ["total"]);
        assert_eq!(
            refusal::human(&err),
            format!("frozen {id}: total cannot change while the state is posted")
        );
        assert!(!Fault::is_transient(&err), "waiting does not help: {err}");
    }
    let guard = b.insert("guarded", "g", &fields(1)).unwrap_err();
    assert_eq!(
        refusal::parse(&guard).unwrap().code,
        RefusalCode::GuardChangeRefused
    );
    // It is not a fault of the store: none was noted, the connection was not given up, and the
    // transaction of the batch was rolled back whole.
    assert_eq!(fault::take(), None);
    assert!(b.get("frozen", "inv-1").unwrap().is_none());
    b.insert("free", "1", &fields(1)).unwrap();
    assert_eq!(b.get("free", "1").unwrap().unwrap()["n"], 1);
}

#[test]
fn other_server_errors_keep_their_old_text() {
    let Some(db) = TestDb::new() else {
        return;
    };
    let b = open_with(db.url(), Some(one())).unwrap();
    b.insert("m", "a", &fields(1)).unwrap();
    let dup = b.insert("m", "a", &fields(2)).unwrap_err();
    assert!(dup.starts_with("db error (SQLSTATE 23505)"), "{dup}");
    assert_eq!(refusal::parse(&dup), None);
}

#[test]
fn a_statement_over_its_limit_is_still_a_timeout() {
    let Some(db) = TestDb::new() else {
        return;
    };
    let pg = Pg::connect_with(
        db.url(),
        PgPoolConfig {
            point_ms: Some(500),
            ..one()
        },
    )
    .unwrap();
    fault::clear();
    let e = pg
        .main
        .run(Class::Point, |c| {
            c.batch_execute("SELECT pg_sleep(30)")?;
            Ok(())
        })
        .unwrap_err();
    assert_eq!(Fault::of_message(&e), Some(Fault::Timeout), "{e}");
    assert_eq!(fault::take(), Some(Fault::Timeout));
    assert_eq!(refusal::parse(&e), None);
}

// ---- the context reaches the transaction ---------------------------------------------------------

#[test]
fn the_tickets_of_a_write_are_visible_to_the_database_in_its_transaction_and_only_there() {
    let Some(db) = TestDb::new() else {
        return;
    };
    // One connection, so that the write after the ticketed one runs on the same session.
    let b = open_with(db.url(), Some(one())).unwrap();
    db.connect()
        .batch_execute(
            "CREATE TABLE zz_seen (n serial, ctx text);
             CREATE FUNCTION zz_see() RETURNS trigger LANGUAGE plpgsql AS $f$
             BEGIN INSERT INTO zz_seen (ctx) VALUES (current_setting('obop.ctx', true));
                   RETURN NEW; END $f$;
             CREATE TRIGGER zz_see AFTER INSERT ON obop_record
                 FOR EACH ROW EXECUTE FUNCTION zz_see();",
        )
        .unwrap();
    let insert = |id: &str| Op::Insert {
        model: "m".into(),
        id: id.into(),
        fields: Fields::new(),
    };
    let with_ticket = WriteCtx::new("ann", "rest", "m.post").ticket("m", "a", "post");
    b.apply_ctx(&with_ticket, &[insert("a")]).unwrap();
    b.apply_ctx(&WriteCtx::new("ann", "rest", "m.create"), &[insert("b")])
        .unwrap();
    b.apply_checked(&[insert("c")]).unwrap();
    let seen: Vec<Option<String>> = db
        .connect()
        .query("SELECT ctx FROM zz_seen ORDER BY n", &[])
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    assert_eq!(seen.len(), 3);
    let first: J = serde_json::from_str(seen[0].as_deref().unwrap()).unwrap();
    assert_eq!(
        first,
        serde_json::json!({"actions": [{"m": "m", "i": "a", "a": "post"}]})
    );
    for later in &seen[1..] {
        assert!(
            later.as_deref().is_none_or(str::is_empty),
            "the tickets of the first write leaked into a later one: {later:?}"
        );
    }
    // And nothing is left on the session either.
    let left = b
        .apply_ctx(&with_ticket, &[insert("d")])
        .and_then(|()| b.apply_checked(&[insert("e")]));
    assert!(left.is_ok());
    let last: Option<String> = db
        .connect()
        .query_one("SELECT ctx FROM zz_seen ORDER BY n DESC LIMIT 1", &[])
        .unwrap()
        .get(0);
    assert!(last.is_none_or(|s| s.is_empty()));
}

// ---- the short read transaction ------------------------------------------------------------------

#[test]
fn read_tx_is_one_snapshot_and_read_only() {
    let Some(db) = TestDb::new() else {
        return;
    };
    let pg = Pg::connect_with(db.url(), PgPoolConfig { size: 3, ..one() }).unwrap();
    pg.insert("m", "a", &fields(1)).unwrap();
    let (before, during) = pg
        .main
        .read_tx(Class::List, |tx| {
            let before = count_in(tx, "m")?;
            // Another connection commits a record while the transaction is open.
            pg.insert("m", "b", &fields(2)).unwrap();
            Ok((before, count_in(tx, "m")?))
        })
        .unwrap();
    assert_eq!((before, during), (1, 1), "both statements saw one snapshot");
    assert_eq!(
        pg.count("m").unwrap(),
        2,
        "and the record is there afterwards"
    );
    // Read-only: a write inside is refused by the server.
    let e = pg
        .main
        .read_tx(Class::List, |tx| {
            tx.execute(
                "INSERT INTO obop_record (model, id, data) VALUES ('m', 'c', '{}')",
                &[],
            )?;
            Ok(())
        })
        .unwrap_err();
    assert!(e.contains("SQLSTATE 25006"), "{e}");
    assert_eq!(pg.count("m").unwrap(), 2);
    // The connection is fine for the next call.
    assert_eq!(
        pg.main
            .read_tx(Class::Point, |tx| count_in(tx, "m"))
            .unwrap(),
        2
    );
}

#[test]
fn read_tx_takes_a_normal_connection_and_not_a_place_in_the_long_read_lane() {
    let Some(db) = TestDb::new() else {
        return;
    };
    let pg = Pg::connect_with(
        db.url(),
        PgPoolConfig {
            size: 4,
            max_long: 1,
            // What a waiter for a place in the lane gives up after. It is the only time window of
            // the test, and nothing that is waited for can end it early: the holder keeps the
            // place until the test lets go.
            acquire_timeout: Duration::from_millis(500),
            // No health check of the connections opened below: they are used at once.
            health_check_idle: Duration::from_secs(3600),
            ..one()
        },
    )
    .unwrap();
    // Open the connections before anything is measured: opening one (TCP, authentication) can
    // take longer than the window on a loaded machine and is not what is tested. A lease that
    // gives up leaves its connection to finish opening and land idle in the pool, so asking again
    // gets it; the wait is for the three connections to exist, bounded, and says what it saw.
    let deadline = Instant::now() + crate::testing::time_budget(60);
    let warm = loop {
        match (0..3)
            .map(|_| pg.main.lease(false))
            .collect::<Result<Vec<_>>>()
        {
            Ok(leases) => break leases,
            Err(e) => assert!(
                Instant::now() < deadline,
                "three connections did not open within the time budget: {e}"
            ),
        }
    };
    drop(warm);
    std::thread::scope(|s| {
        // The ends of the channels are locals of this closure, so that they are dropped when an
        // assertion below fails and the thread unwinds: the holder then stops waiting and the
        // scope can end with the failure instead of hanging.
        let (started_tx, started_rx) = mpsc::channel::<()>();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let pg = &pg;
        let holder = s.spawn(move || {
            // A scan holds the only place in the long-read lane.
            pg.main
                .snapshot(Class::Aggregate, |_tx| {
                    started_tx.send(()).unwrap();
                    // Let go when told, when the test is gone, or after a minute at the latest.
                    let _ = release_rx.recv_timeout(Duration::from_secs(60));
                    Ok(())
                })
                .unwrap();
        });
        started_rx
            .recv_timeout(Duration::from_secs(30))
            .expect("the holder never took its place in the long-read lane");
        let e = pg.main.snapshot(Class::Aggregate, |_| Ok(())).unwrap_err();
        assert_eq!(
            Fault::of_message(&e),
            Some(Fault::Busy),
            "a second long read has to wait for the first: {e}"
        );
        fault::clear();
        // The short read does not: it takes one of the idle connections.
        let one_row = pg
            .main
            .read_tx(Class::List, |tx| {
                Ok(tx.query_one("SELECT 1::int", &[])?.get::<_, i32>(0))
            })
            .unwrap();
        assert_eq!(one_row, 1);
        drop(release_tx);
        holder.join().unwrap();
    });
}

#[test]
fn read_tx_runs_again_once_on_a_fresh_connection_when_the_reused_one_is_dead() {
    let Some(db) = TestDb::new() else {
        return;
    };
    let pg = Pg::connect_with(
        db.url(),
        PgPoolConfig {
            // No health check: the dead connection is handed out and found out by the read.
            health_check_idle: Duration::from_secs(3600),
            ..one()
        },
    )
    .unwrap();
    pg.insert("m", "a", &fields(1)).unwrap();
    assert_eq!(pg.count("m").unwrap(), 1, "the connection has been used");
    db.terminate_sessions();
    fault::clear();
    let n = pg
        .main
        .read_tx(Class::List, |tx| count_in(tx, "m"))
        .unwrap();
    assert_eq!(n, 1, "the read was repeated on a new connection");
    assert_eq!(fault::take(), None, "and nothing was noted: it worked");
    assert!(pg.main.stats().connections_closed >= 1);
    // And again: the connection that replaced the dead one dies in its turn.
    db.terminate_sessions();
    let again = pg.main.read_tx(Class::List, |tx| count_in(tx, "m"));
    assert_eq!(again.unwrap(), 1);
}

// ---- the schema parts ----------------------------------------------------------------------------

/// What an inspection found that makes a check fail. The change history is made when a model
/// first needs it, so a database that is complete for the record store has it as a warning.
fn errors(c: &mut postgres::Client) -> Vec<crate::schema::Finding> {
    crate::schema::inspect(c)
        .unwrap()
        .into_iter()
        .filter(|f| f.error)
        .collect()
}

fn tables(c: &mut postgres::Client) -> Vec<String> {
    c.query(
        "SELECT relname::text FROM pg_class WHERE relnamespace = 'public'::regnamespace \
         AND relkind = 'r' ORDER BY 1",
        &[],
    )
    .unwrap()
    .iter()
    .map(|r| r.get(0))
    .collect()
}

#[test]
fn ensure_creates_what_is_missing_only_when_allowed() {
    let Some(db) = TestDb::new() else {
        return;
    };
    let mut c = db.connect();
    assert!(tables(&mut c).is_empty());
    // Never: an empty database is an error that names the part, and nothing was created.
    let e = crate::schema::with_lock(&mut c, |c| crate::schema::ensure(c, Ddl::Never)).unwrap_err();
    assert!(e.contains("missing: base"), "{e}");
    assert!(tables(&mut c).is_empty());
    let e = open_options(
        db.url(),
        &OpenOptions {
            pool: Some(one()),
            ddl: Ddl::Never,
        },
    )
    .err()
    .expect("an empty database cannot be opened without DDL");
    assert!(
        e.contains("missing: base") && e.contains("may not create it"),
        "{e}"
    );
    assert!(tables(&mut c).is_empty());
    // IfMissing creates it; a second time there is nothing to do.
    crate::schema::with_lock(&mut c, |c| crate::schema::ensure(c, Ddl::IfMissing)).unwrap();
    // The change history is not made at start: a process that starts creates the record store and
    // nothing for a feature no model uses (D75). It is a warning of an inspection, not an error.
    assert_eq!(tables(&mut c), ["obop_counter", "obop_kv", "obop_record"]);
    assert!(errors(&mut c).is_empty());
    let found = crate::schema::inspect(&mut c).unwrap();
    assert_eq!(
        found
            .iter()
            .map(|f| (f.object.as_str(), f.error))
            .collect::<Vec<_>>(),
        [("history", false)]
    );
    crate::schema::with_lock(&mut c, |c| crate::schema::ensure(c, Ddl::IfMissing)).unwrap();
    // Now Never is fine, and the store works.
    let b = open_options(
        db.url(),
        &OpenOptions {
            pool: Some(one()),
            ddl: Ddl::Never,
        },
    )
    .unwrap();
    b.insert("m", "a", &fields(1)).unwrap();
    assert_eq!(b.get("m", "a").unwrap().unwrap()["n"], 1);
}

#[test]
fn a_missing_object_is_reported_and_put_back() {
    let Some(db) = TestDb::new() else {
        return;
    };
    drop(open_with(db.url(), Some(one())).unwrap());
    let mut c = db.connect();
    c.batch_execute("INSERT INTO obop_kv (key, value, expires_ms) VALUES ('k', 'v', 0)")
        .unwrap();
    for broken in ["DROP INDEX obop_kv_expiry", "DROP TABLE obop_counter"] {
        c.batch_execute(broken).unwrap();
        let found = errors(&mut c);
        assert_eq!(found.len(), 1, "{broken}: {found:?}");
        assert_eq!(
            (found[0].code, found[0].object.as_str(), found[0].error),
            ("schema_part_missing", "base", true)
        );
        let e = open_options(
            db.url(),
            &OpenOptions {
                pool: Some(one()),
                ddl: Ddl::Never,
            },
        )
        .err()
        .expect("Never does not repair");
        assert!(e.contains("missing: base"), "{e}");
        // IfMissing repairs it, and does not touch what is there.
        drop(open_with(db.url(), Some(one())).unwrap());
        assert!(errors(&mut c).is_empty(), "{broken}");
        let kept: i64 = c
            .query_one("SELECT count(*) FROM obop_kv WHERE key = 'k'", &[])
            .unwrap()
            .get(0);
        assert_eq!(kept, 1, "existing rows are untouched");
    }
}

#[test]
fn a_serving_role_that_may_not_create_tables_opens_an_installed_database() {
    let Some(mut db) = TestDb::new() else {
        return;
    };
    // The owner installs; the serving role gets DML and nothing else.
    crate::schema::owner_install(db.url()).unwrap();
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
    // With either mode: nothing to create, so no right to create is needed.
    for ddl in [Ddl::Never, Ddl::IfMissing] {
        let b = open_options(
            &rt,
            &OpenOptions {
                pool: Some(one()),
                ddl,
            },
        )
        .unwrap_or_else(|e| panic!("{ddl:?}: {e}"));
        b.insert("m", &format!("{ddl:?}"), &fields(1)).unwrap();
    }
    assert_eq!(
        db.connect()
            .query_one("SELECT count(*) FROM obop_record", &[])
            .unwrap()
            .get::<_, i64>(0),
        2
    );
}

#[test]
fn a_role_that_may_not_create_tables_is_told_how_to_install_the_schema() {
    let Some(mut db) = TestDb::new() else {
        return;
    };
    let rt = db.role("rt", "NOSUPERUSER").unwrap();
    db.connect()
        .batch_execute("REVOKE CREATE ON SCHEMA public FROM PUBLIC")
        .unwrap();
    let e = open_with(&rt, Some(one()))
        .err()
        .expect("no right to create");
    assert!(e.contains("SQLSTATE 42501"), "{e}");
    assert!(e.contains("owner_install"), "{e}");
    let e = open_options(
        &rt,
        &OpenOptions {
            pool: Some(one()),
            ddl: Ddl::Never,
        },
    )
    .err()
    .expect("no schema");
    assert!(e.contains("missing: base"), "{e}");
}

#[test]
fn processes_that_start_together_on_an_empty_database_all_start() {
    let Some(db) = TestDb::new() else {
        return;
    };
    let barrier = std::sync::Barrier::new(6);
    std::thread::scope(|s| {
        let hs: Vec<_> = (0..6)
            .map(|_| {
                s.spawn(|| {
                    barrier.wait();
                    open_with(db.url(), Some(one())).map(|_| ())
                })
            })
            .collect();
        for h in hs {
            h.join().unwrap().unwrap();
        }
    });
    assert!(errors(&mut db.connect()).is_empty());
}

#[test]
fn the_schema_of_a_non_default_search_path_is_used() {
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
    let b = open_with(db.url(), Some(one())).unwrap();
    b.insert("m", "a", &fields(1)).unwrap();
    let mut c = db.connect();
    let in_odd: i64 = c
        .query_one(
            "SELECT count(*) FROM pg_class WHERE relnamespace = '\"Odd Schema\"'::regnamespace \
             AND relname = 'obop_record'",
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(in_odd, 1);
    assert!(tables(&mut c).is_empty(), "nothing in public");
}

// ---- a check that cannot be answered is not "missing" ---------------------------------------------

#[test]
fn a_catalogue_query_that_fails_is_an_error_and_never_a_missing_part() {
    let Some(db) = TestDb::new() else {
        return;
    };
    drop(open_with(db.url(), Some(one())).unwrap()); // a complete schema
    // 1. The transaction is already aborted (25P02): every query is refused. Reading that as "the
    // part is not there" would advise DDL for a complete database under `Never`, and hide the
    // real cause under `IfMissing`.
    let mut c = db.connect();
    c.batch_execute("BEGIN").unwrap();
    assert!(c.batch_execute("SELECT 1/0").is_err());
    for ddl in [Ddl::Never, Ddl::IfMissing] {
        let e = crate::schema::ensure(&mut c, ddl).unwrap_err();
        assert!(e.contains("SQLSTATE 25P02"), "{ddl:?}: {e}");
        assert!(
            !e.contains("missing") && !e.contains("may not create"),
            "{ddl:?}: {e}"
        );
    }
    let e = crate::schema::inspect(&mut c).unwrap_err();
    assert!(e.contains("SQLSTATE 25P02"), "{e}");
    c.batch_execute("ROLLBACK").unwrap();
    // The same connection answers once the transaction is over: the schema is complete.
    crate::schema::ensure(&mut c, Ddl::Never).unwrap();
    assert!(errors(&mut c).is_empty());
    // 2. The server ends the session between the lock and the check: the connection is gone, which
    // is `storage_unavailable` and not a verdict on the schema.
    let mut gone = db.connect();
    db.terminate_sessions();
    fault::clear();
    for result in [
        crate::schema::ensure(&mut gone, Ddl::Never),
        crate::schema::inspect(&mut gone).map(|_| ()),
    ] {
        let e = result.unwrap_err();
        assert_eq!(Fault::of_message(&e), Some(Fault::Unavailable), "{e}");
    }
    fault::clear();
}

// ---- the byte store follows Ddl like the rest -------------------------------------------------------

#[test]
fn the_byte_stores_table_is_created_or_required_like_the_rest_of_the_schema() {
    use crate::blob::DbBytes;
    let Some(db) = TestDb::new() else {
        return;
    };
    let never = OpenOptions {
        pool: Some(one()),
        ddl: Ddl::Never,
    };
    let regclass = |c: &mut postgres::Client| -> bool {
        c.query_one("SELECT to_regclass('obop_blob_data') IS NOT NULL", &[])
            .unwrap()
            .get(0)
    };
    let mut c = db.connect();
    // Never on an empty database: an error that names the table, and nothing is made.
    let e = DbBytes::open_options(db.url(), &never)
        .err()
        .expect("no table, no right to make one");
    assert!(e.contains("missing: blob"), "{e}");
    assert!(!regclass(&mut c));
    // The record store alone does not make it (a deployment with a blob directory has no use for
    // it) and is complete without it.
    drop(open_with(db.url(), Some(one())).unwrap());
    assert!(!regclass(&mut c));
    assert!(errors(&mut c).is_empty());
    assert!(DbBytes::open_options(db.url(), &never).is_err());
    // `open_with` makes it, as before.
    DbBytes::open_with(db.url(), Some(one())).unwrap();
    assert!(regclass(&mut c));
    DbBytes::open_options(db.url(), &never).unwrap();
}

#[test]
fn a_serving_role_that_may_not_create_tables_opens_the_byte_store_of_an_installed_database() {
    use crate::blob::DbBytes;
    let Some(mut db) = TestDb::new() else {
        return;
    };
    // The owner installs everything, the byte store's table included.
    crate::schema::owner_install(db.url()).unwrap();
    let rt = db
        .role("rt", "NOSUPERUSER NOCREATEDB NOCREATEROLE")
        .unwrap();
    db.connect()
        .batch_execute(&format!(
            "GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO {role};
             REVOKE CREATE ON SCHEMA public FROM PUBLIC",
            role = db.role_name("rt")
        ))
        .unwrap();
    for ddl in [Ddl::Never, Ddl::IfMissing] {
        let o = OpenOptions {
            pool: Some(one()),
            ddl,
        };
        DbBytes::open_options(&rt, &o).unwrap_or_else(|e| panic!("{ddl:?}: {e}"));
    }
}

// ---- a refusal is never run again because of the words in it ----------------------------------------

#[test]
fn a_refused_batch_is_not_run_again_because_a_record_id_says_deadlock() {
    let Some(db) = TestDb::new() else {
        return;
    };
    let b = open_with(db.url(), Some(one())).unwrap();
    // The trigger counts its calls on a sequence, which a rolled-back transaction does not undo.
    db.connect()
        .batch_execute(&format!(
            "CREATE SEQUENCE zz_attempts;{REFUSING_TRIGGER_COUNTING}"
        ))
        .unwrap();
    let id = "inv SQLSTATE 40001 and SQLSTATE 40P01";
    let e = b
        .apply_checked(&[Op::Insert {
            model: "frozen".into(),
            id: id.into(),
            fields: fields(1),
        }])
        .unwrap_err();
    let ApplyError::Storage(text) = e else {
        panic!("{e:?}");
    };
    let r = refusal::parse(&text).unwrap_or_else(|| panic!("not a refusal: {text}"));
    assert_eq!(r.record, id);
    assert!(
        text.contains("SQLSTATE 40001"),
        "the id is in the message: {text}"
    );
    let attempts: i64 = db
        .connect()
        .query_one("SELECT last_value FROM zz_attempts", &[])
        .unwrap()
        .get(0);
    assert_eq!(attempts, 1, "the batch ran once; it was not run again");
    // And the words that `describe` writes for a real deadlock are still recognised.
    assert!(crate::pgpool::is_retryable_text(
        "db error (SQLSTATE 40P01): deadlock detected"
    ));
    assert!(crate::pgpool::is_retryable_text(
        "db error (SQLSTATE 40001): could not serialize access"
    ));
    for not in [
        text.as_str(),
        "update failed: db error (SQLSTATE 40P01): deadlock detected",
        "storage_busy: SQLSTATE 40001",
        "db error (SQLSTATE 23505): duplicate key value, SQLSTATE 40P01",
        "",
    ] {
        assert!(!crate::pgpool::is_retryable_text(not), "{not}");
    }
}

const REFUSING_TRIGGER_COUNTING: &str = r#"
    CREATE FUNCTION zz_refuse() RETURNS trigger LANGUAGE plpgsql AS $f$
    BEGIN
        IF NEW.model = 'frozen' THEN
            PERFORM nextval('zz_attempts');
            RAISE EXCEPTION USING ERRCODE = 'OBG01', MESSAGE = 'immutable_record',
                DETAIL = json_build_object('model', NEW.model, 'id', NEW.id,
                    'rule', 'immutable_when', 'fields', json_build_array('total'),
                    'detail', 'total cannot change')::text;
        END IF;
        RETURN NEW;
    END $f$;
    CREATE TRIGGER zz_refuse BEFORE INSERT ON obop_record
        FOR EACH ROW EXECUTE FUNCTION zz_refuse();"#;

// ---- production ----------------------------------------------------------------------------------

#[test]
fn production_is_a_flag_or_the_variable_set_to_one() {
    assert!(!production_from(false, None));
    assert!(production_from(true, None));
    assert!(production_from(false, Some("1")));
    assert!(production_from(false, Some(" 1\n")));
    for no in ["", "0", "true", "yes", "11"] {
        assert!(!production_from(false, Some(no)), "{no:?}");
    }
}

#[test]
fn a_process_that_is_not_a_production_process_creates_the_schema_whatever_the_environment_says() {
    // `production()` is the flag only: the variable is not read yet (see its documentation), so
    // an environment that sets it changes nothing here. (That `open_with` creates the schema in
    // a production process is tests/production.rs: it needs a process of its own.)
    assert!(!production());
    assert_eq!(Ddl::for_process(), Ddl::IfMissing);
    assert_eq!(OpenOptions::default().ddl, Ddl::IfMissing);
    assert_eq!(OpenOptions::default().pool, None);
    // The rule the variable will follow once it is read.
    assert!(!production_from(false, None));
    assert!(production_from(false, Some("1")));
}
