//! The keyed lookup of a unique rule (`Backend::find_unique`, D57, ask 11.4) on every backend.
//!
//! What these tests hold the code to:
//!
//! * the answer is the one the old check got from `find` and the rule of `unique_matches`, for a
//!   corpus of hostile values (a string `"5"` and the number `5`, floats, lists, empty values,
//!   quotes, long text), with and without the index;
//! * the SQL of the lookup is served by the unique index: the query plan names it, on SQLite and on
//!   PostgreSQL, and the old statement's plan does not (the control that shows the test can fail);
//! * the text of the index is what deployed databases hold since D57, byte for byte.

use super::*;
use crate::testing::TestDb;
use serde_json::json;

/// Every kind of value a field can hold in a stored document, `None` for a field that is not there.
fn corpus() -> Vec<Option<J>> {
    vec![
        None,
        Some(J::Null),
        Some(json!("")),
        Some(json!("x")),
        Some(json!("X")),
        Some(json!("x ")),
        Some(json!(" x")),
        Some(json!("5")),
        Some(json!(5)),
        Some(json!(5.0)),
        Some(json!(1.5)),
        Some(json!(-0.0)),
        Some(json!(true)),
        Some(json!(false)),
        Some(json!(1)),
        Some(json!(0)),
        Some(json!("true")),
        Some(json!("é")),
        Some(json!("a'b\"c\\d")),
        Some(json!("line\nbreak\ttab")),
        Some(json!("%_".repeat(3))),
        Some(json!("long".repeat(100))),
        Some(json!([1])),
        Some(json!({"k": 1})),
        Some(json!(i64::MAX)),
        Some(json!(u64::MAX)),
    ]
}

/// The scope values a caller asks with: what `norm` gives (the empty text for a missing field)
/// and values of other kinds.
fn scopes() -> Vec<J> {
    vec![
        json!(""),
        json!("x"),
        json!("5"),
        json!(5),
        json!(true),
        json!(1.5),
        json!("é"),
    ]
}

fn doc(a: &Option<J>, b: &Option<J>) -> Fields {
    let mut f = Fields::new();
    if let Some(v) = a {
        f.insert("a".into(), v.clone());
    }
    if let Some(v) = b {
        f.insert("b".into(), v.clone());
    }
    f
}

fn ids(rows: Vec<(String, Fields)>) -> Vec<String> {
    rows.into_iter().map(|(id, _)| id).collect()
}

/// `find_unique` against `list` and `unique_matches`, for every query the corpus gives.
fn same_as_the_rule(b: &dyn Backend, model: &str) -> (usize, usize) {
    let (mut asked, mut found) = (0, 0);
    for first in corpus().into_iter().flatten() {
        if matches!(&first, J::Null) || first == json!("") {
            continue;
        }
        let mut asks: Vec<(Vec<&str>, Vec<J>)> = vec![(vec!["a"], vec![first.clone()])];
        for scope in scopes() {
            asks.push((vec!["a", "b"], vec![first.clone(), scope]));
        }
        for (fields, values) in asks {
            let want: Vec<String> = b
                .list(model)
                .unwrap()
                .into_iter()
                .filter(|(_, rec)| unique_matches(rec, &fields, &values))
                .map(|(id, _)| id)
                .collect();
            let got = ids(b.find_unique(model, &fields, &values).unwrap());
            assert_eq!(
                got,
                want,
                "{} {model} {fields:?} {values:?}",
                b.kind()
            );
            asked += 1;
            found += usize::from(!want.is_empty());
        }
    }
    (asked, found)
}

fn fill(b: &dyn Backend, model: &str) {
    let scope_values: Vec<Option<J>> = vec![
        None,
        Some(J::Null),
        Some(json!("")),
        Some(json!("x")),
        Some(json!("5")),
        Some(json!(5)),
        Some(json!(true)),
    ];
    let mut n = 0;
    for a in corpus() {
        for s in &scope_values {
            n += 1;
            // With the unique index a pair the database counts as a repeat is refused; the lookup
            // is then asked about whatever the database accepted.
            let _ = b.insert(model, &format!("r{n:04}"), &doc(&a, s));
        }
    }
}

fn check_backend(b: &dyn Backend) {
    // Without the index: the lookup scans, and must still give the answer of the rule.
    fill(b, "plain");
    let (asked, found) = same_as_the_rule(b, "plain");
    assert!(asked > 150 && found > 20, "{asked} asked, {found} answered");
    // With it: the database may have refused some pairs, the answer is the same.
    b.ensure_unique("obop_uq_idx_a_b", "idx", &["a", "b"])
        .unwrap();
    b.ensure_unique("obop_uq_idx2_a", "idx2", &["a"]).unwrap();
    fill(b, "idx");
    let (asked, found) = same_as_the_rule(b, "idx");
    assert!(asked > 150 && found > 10, "{asked} asked, {found} answered");
    // The index is on (a, b); the one-field query is answered by the same rule.
    for (i, a) in corpus().into_iter().enumerate() {
        let _ = b.insert("idx2", &format!("s{i:03}"), &doc(&a, &None));
    }
    for first in corpus().into_iter().flatten() {
        if matches!(&first, J::Null) || first == json!("") {
            continue;
        }
        let want: Vec<String> = b
            .list("idx2")
            .unwrap()
            .into_iter()
            .filter(|(_, r)| unique_matches(r, &["a"], std::slice::from_ref(&first)))
            .map(|(id, _)| id)
            .collect();
        let got = ids(b.find_unique("idx2", &["a"], std::slice::from_ref(&first)).unwrap());
        assert_eq!(got, want, "{} idx2 {first:?}", b.kind());
    }
    // A model nothing was stored in, and a name the index cannot carry, answer without error.
    assert!(
        b.find_unique("nothing", &["a"], &[json!("x")])
            .unwrap()
            .is_empty()
    );
    assert!(
        b.find_unique("plain", &["Not-An-Identifier"], &[json!("x")])
            .unwrap()
            .is_empty()
    );
    // The caller must give one value for each field.
    assert!(b.find_unique("plain", &["a", "b"], &[json!("x")]).is_err());
    assert!(b.find_unique("plain", &[], &[]).is_err());
}

#[test]
fn find_unique_gives_the_answer_of_the_rule_on_memory() {
    check_backend(&*open("memory:").unwrap());
}

#[test]
fn find_unique_gives_the_answer_of_the_rule_on_sqlite() {
    check_backend(&*open("sqlite::memory:").unwrap());
}

#[test]
fn find_unique_gives_the_answer_of_the_rule_on_postgres() {
    let Some(db) = TestDb::new() else {
        return;
    };
    check_backend(&*open(db.url()).unwrap());
}

// ---- the plan: the index serves the lookup --------------------------------------------------

/// The plan of the lookup on SQLite names the unique index and does not scan; the statement the
/// check used before (`find`) does not use that index.
#[test]
fn the_sqlite_lookup_is_planned_on_the_unique_index() {
    let db = crate::sqlite::Sqlite::open(":memory:").unwrap();
    let rows: Vec<Row> = (0..3000)
        .map(|n| {
            let mut f = Fields::new();
            f.insert("sku".into(), json!(format!("S{n}")));
            f.insert("company".into(), json!(format!("c{}", n % 7)));
            ("t".to_string(), format!("id{n}"), f)
        })
        .collect();
    db.insert_many(&rows).unwrap();
    db.ensure_unique("obop_uq_t_sku", "t", &["sku"]).unwrap();
    db.ensure_unique("obop_uq_t_sku_company", "t", &["sku", "company"])
        .unwrap();
    let plan = |sql: &str, binds: Vec<rusqlite::types::Value>| -> String {
        db.with_conn(|c| {
            let mut st = c.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
            let rows = st
                .query_map(rusqlite::params_from_iter(binds), |r| r.get::<_, String>(3))
                .unwrap();
            rows.map(|r| r.unwrap()).collect::<Vec<_>>().join(" | ")
        })
    };
    use rusqlite::types::Value as Sql;
    for (fields, index, binds) in [
        (
            vec!["sku"],
            "obop_uq_t_sku",
            vec![Sql::Text("S77".into())],
        ),
        (
            vec!["sku", "company"],
            "obop_uq_t_sku_company",
            vec![Sql::Text("S77".into()), Sql::Text("c0".into())],
        ),
    ] {
        let sql = crate::sqlite::unique_lookup_sql("t", &fields).unwrap();
        let p = plan(&sql, binds);
        assert!(
            p.contains(&format!("USING INDEX {index}")),
            "the plan of {fields:?} does not use {index}: {p}"
        );
        assert!(!p.contains("SCAN"), "the plan of {fields:?} scans: {p}");
    }
    // The control: what the check asked before. If this plan used the index, the assertions above
    // would prove nothing.
    let old = plan(
        "SELECT id, data FROM obop_record WHERE model = ?1 \
         AND json_extract(data, '$.' || ?2) = ?3 ORDER BY seq",
        vec![Sql::Text("t".into()), Sql::Text("sku".into()), Sql::Text("S77".into())],
    );
    assert!(!old.contains("obop_uq_"), "{old}");
}

/// The same on PostgreSQL, over 20,000 rows analysed, with the planner's own choice (no setting
/// forces the index).
#[test]
fn the_postgres_lookup_is_planned_on_the_unique_index() {
    let Some(db) = TestDb::new() else {
        return;
    };
    let b = open(db.url()).unwrap();
    b.ensure_unique("obop_uq_t_sku", "t", &["sku"]).unwrap();
    b.ensure_unique("obop_uq_t_sku_company", "t", &["sku", "company"])
        .unwrap();
    let mut c = db.connect();
    c.batch_execute(
        "INSERT INTO obop_record (model, id, data)
           SELECT 't', 'id' || g,
                  jsonb_build_object('sku', 'S' || g, 'company', 'c' || (g % 7))
             FROM generate_series(1, 20000) g;
         ANALYZE obop_record",
    )
    .unwrap();
    let plan = |c: &mut postgres::Client,
                sql: &str,
                params: &[&(dyn postgres::types::ToSql + Sync)]| {
        c.query(&format!("EXPLAIN {sql}"), params)
            .unwrap()
            .iter()
            .map(|r| r.get::<_, String>(0))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let one = crate::pg::unique_lookup_sql("t", &["sku"]).unwrap();
    let p = plan(&mut c, &one, &[&"S77"]);
    assert!(p.contains("obop_uq_t_sku"), "{p}");
    assert!(!p.contains("Seq Scan"), "{p}");
    let two = crate::pg::unique_lookup_sql("t", &["sku", "company"]).unwrap();
    let p = plan(&mut c, &two, &[&"S77", &"c0"]);
    assert!(p.contains("obop_uq_t_sku_company"), "{p}");
    assert!(!p.contains("Seq Scan"), "{p}");
    // And it returns the row: the plan is of the statement the backend runs.
    let hit = b.find_unique("t", &["sku", "company"], &[json!("S77"), json!("c0")]);
    assert_eq!(ids(hit.unwrap()), ["id77"]);
    // The control: the statement the check used before reads the model.
    let old = plan(
        &mut c,
        "SELECT id, data FROM obop_record WHERE model = $1 AND data -> $2::text = $3 ORDER BY seq",
        &[&"t", &"sku", &json!("S77")],
    );
    assert!(
        old.contains("Seq Scan") && !old.contains("obop_uq_"),
        "{old}"
    );
}

// ---- the text of the index -----------------------------------------------------------------

#[test]
fn the_statements_are_golden() {
    assert_eq!(
        crate::sqlite::unique_index_sql("obop_uq_t_sku_company", "t", &["sku", "company"]).unwrap(),
        "CREATE UNIQUE INDEX IF NOT EXISTS obop_uq_t_sku_company ON obop_record \
         (NULLIF(json_extract(data, '$.sku'), ''), COALESCE(json_extract(data, '$.company'), '')) \
         WHERE model = 't'"
    );
    assert_eq!(
        crate::sqlite::unique_lookup_sql("t", &["sku", "company"]).unwrap(),
        "SELECT id, data FROM obop_record WHERE model = 't' \
         AND NULLIF(json_extract(data, '$.sku'), '') = ?1 \
         AND COALESCE(json_extract(data, '$.company'), '') = ?2 ORDER BY seq"
    );
    assert_eq!(
        crate::pg::unique_index_sql("obop_uq_t_sku_company", "t", &["sku", "company"]).unwrap(),
        "CREATE UNIQUE INDEX IF NOT EXISTS obop_uq_t_sku_company ON obop_record \
         ((NULLIF(data ->> 'sku', '')), (COALESCE(data ->> 'company', ''))) WHERE model = 't'"
    );
    assert_eq!(
        crate::pg::unique_lookup_sql("t", &["sku", "company"]).unwrap(),
        "SELECT id, data FROM obop_record WHERE model = 't' \
         AND (NULLIF(data ->> 'sku', '')) = $1 \
         AND (COALESCE(data ->> 'company', '')) = $2 ORDER BY seq"
    );
    for bad in ["", "T", "a b", "a'b", "a;b"] {
        assert!(crate::sqlite::unique_lookup_sql(bad, &["a"]).is_err(), "{bad:?}");
        assert!(crate::sqlite::unique_lookup_sql("t", &[bad]).is_err(), "{bad:?}");
        assert!(crate::pg::unique_lookup_sql(bad, &["a"]).is_err(), "{bad:?}");
        assert!(crate::pg::unique_lookup_sql("t", &[bad]).is_err(), "{bad:?}");
    }
    assert!(crate::sqlite::unique_lookup_sql("t", &[]).is_err());
    assert!(crate::pg::unique_lookup_sql("t", &[]).is_err());
}

/// What a database that was made by an earlier version holds is what these versions make: the
/// definition the server reports for the index is pinned as text. The test compiles and passes
/// against the code before the keyed lookup, which is how the text was taken.
#[test]
fn the_unique_index_is_what_deployed_databases_hold() {
    let sqlite = open("sqlite::memory:").unwrap();
    sqlite
        .ensure_unique("obop_uq_t_sku_company", "t", &["sku", "company"])
        .unwrap();
    // Asking the same again changes nothing (IF NOT EXISTS).
    sqlite
        .ensure_unique("obop_uq_t_sku_company", "t", &["sku", "company"])
        .unwrap();
    let s = crate::sqlite::Sqlite::open(":memory:").unwrap();
    s.ensure_unique("obop_uq_t_sku_company", "t", &["sku", "company"])
        .unwrap();
    let def: String = s.with_conn(|c| {
        c.query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'obop_uq_t_sku_company'",
            (),
            |r| r.get(0),
        )
        .unwrap()
    });
    assert_eq!(
        def,
        "CREATE UNIQUE INDEX obop_uq_t_sku_company ON obop_record \
         (NULLIF(json_extract(data, '$.sku'), ''), COALESCE(json_extract(data, '$.company'), '')) \
         WHERE model = 't'"
    );
    let Some(db) = TestDb::new() else {
        return;
    };
    let pg = open(db.url()).unwrap();
    pg.ensure_unique("obop_uq_t_sku_company", "t", &["sku", "company"])
        .unwrap();
    pg.ensure_unique("obop_uq_t_sku_company", "t", &["sku", "company"])
        .unwrap();
    let def: String = db
        .connect()
        .query_one(
            "SELECT pg_get_indexdef('obop_uq_t_sku_company'::regclass)",
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(
        def,
        "CREATE UNIQUE INDEX obop_uq_t_sku_company ON public.obop_record USING btree \
         (NULLIF((data ->> 'sku'::text), ''::text), COALESCE((data ->> 'company'::text), ''::text)) \
         WHERE (model = 't'::text)"
    );
}
