//! The seams of wave 2 against a real PostgreSQL. Each test has a database of its own (`TestDb`) and
//! returns early, saying so, when `OBOP_TEST_POSTGRES_URL` is not set.

use super::*;
use crate::fault::{self, Fault};
use crate::pg::Pg;
use crate::pgpool::{Class, PgPoolConfig};
use crate::refusal::{self, RefusalCode};
use crate::testing::TestDb;

fn fields(n: i64) -> Fields {
    let mut m = Fields::new();
    m.insert("n".into(), J::from(n));
    m
}

fn one() -> PgPoolConfig {
    PgPoolConfig::one_shot()
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

