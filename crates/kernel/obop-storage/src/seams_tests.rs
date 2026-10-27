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
use std::time::Duration;

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
            acquire_timeout: Duration::from_millis(500),
            ..one()
        },
    )
    .unwrap();
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    std::thread::scope(|s| {
        // A scan holds the only place in the long-read lane.
        let pg = &pg;
        let holder = s.spawn(move || {
            pg.main
                .snapshot(Class::Aggregate, |_tx| {
                    started_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(())
                })
                .unwrap();
        });
        started_rx.recv().unwrap();
        let e = pg.main.snapshot(Class::Aggregate, |_| Ok(())).unwrap_err();
        assert_eq!(
            Fault::of_message(&e),
            Some(Fault::Busy),
            "a second long read has to wait for the first: {e}"
        );
        fault::clear();
        // The short read does not.
        let one_row = pg
            .main
            .read_tx(Class::List, |tx| {
                Ok(tx.query_one("SELECT 1::int", &[])?.get::<_, i32>(0))
            })
            .unwrap();
        assert_eq!(one_row, 1);
        release_tx.send(()).unwrap();
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
        e.contains("missing: base") && e.contains("OBOP_PRODUCTION"),
        "{e}"
    );
    assert!(tables(&mut c).is_empty());
    // IfMissing creates it; a second time there is nothing to do.
    crate::schema::with_lock(&mut c, |c| crate::schema::ensure(c, Ddl::IfMissing)).unwrap();
    assert_eq!(tables(&mut c), ["obop_counter", "obop_kv", "obop_record"]);
    assert!(crate::schema::inspect(&mut c).is_empty());
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
        let found = crate::schema::inspect(&mut c);
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
        assert!(crate::schema::inspect(&mut c).is_empty(), "{broken}");
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
    assert!(crate::schema::inspect(&mut db.connect()).is_empty());
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
fn open_with_and_the_options_name_the_ddl_they_use() {
    assert_eq!(
        OpenOptions {
            pool: None,
            ddl: Ddl::Never
        }
        .ddl,
        Ddl::Never
    );
    // In a process that is not production (no test here sets the flag), the default creates.
    assert_eq!(Ddl::for_process(), Ddl::IfMissing);
    assert_eq!(OpenOptions::default().ddl, Ddl::IfMissing);
    assert_eq!(OpenOptions::default().pool, None);
}
