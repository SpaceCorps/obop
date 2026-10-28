//! The keyed lookup of a unique rule (`Backend::find_unique`, D57, ask 11.4) on every backend.
//!
//! What these tests hold the code to:
//!
//! * the answer is the exact rule of `unique_matches` (the first field equal as a JSON value, a
//!   scope field equal or both empty), for a corpus of hostile values (a string `"5"` and the
//!   number `5`, floats, lists, empty values, quotes, long text), with and without the index. The
//!   old check got its candidates from `find`, which is the same rule for every value a typed
//!   field can hold; it differed only for a float equal to an integer (`5.0` and `5`), which
//!   PostgreSQL's `jsonb` took for one value and no typed kind can produce (ISSUES K37);
//! * the backend runs the statement built from the index for text, whole numbers and booleans, and
//!   goes the way of `find` only for the values listed here (it reports which way it went);
//! * the statement it ran is served by the unique index: SQLite's plan and PostgreSQL's plan name
//!   that exact index, the work SQLite does for it does not grow with the model, and the old
//!   statement's does (the control that shows the test can fail);
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
            assert_eq!(got, want, "{} {model} {fields:?} {values:?}", b.kind());
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
        let got = ids(b
            .find_unique("idx2", &["a"], std::slice::from_ref(&first))
            .unwrap());
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

// ---- which statement ran ------------------------------------------------------------------

/// Ask for a key and report which way the backend went: the statement built from the index, or
/// `find` and the exact rule. A backend that quietly took the second way gives the same answers
/// and costs the size of the model, which no test of answers can see.
fn way(b: &dyn Backend, model: &str, fields: &[&str], values: &[J]) -> (u64, u64) {
    crate::testing::reset_unique_runs();
    b.find_unique(model, fields, values).unwrap();
    let r = crate::testing::unique_runs();
    (r.keyed, r.fallback)
}

const KEYED: (u64, u64) = (1, 0);
const FALLBACK: (u64, u64) = (0, 1);

/// `u64_is_keyed`: PostgreSQL has an exact text for every integer, SQLite compares only what fits
/// an `i64`. `nul_is_keyed`: a text with a NUL has no exact text in PostgreSQL.
fn the_way_of_each_kind_of_key(b: &dyn Backend, u64_is_keyed: bool, nul_is_keyed: bool) {
    let kind = b.kind();
    b.ensure_unique("obop_uq_w1_a", "w1", &["a"]).unwrap();
    b.ensure_unique("obop_uq_w2_a_b", "w2", &["a", "b"])
        .unwrap();
    let way_of = |model: &str, fields: &[&str], values: &[J]| way(b, model, fields, values);
    // Text, whole numbers and booleans, as the first field and as a scope.
    for first in [
        json!("x"),
        json!("5"),
        json!("é"),
        json!(5),
        json!(-7),
        json!(0),
        json!(i64::MAX),
        json!(true),
        json!(false),
    ] {
        assert_eq!(
            way_of("w1", &["a"], std::slice::from_ref(&first)),
            KEYED,
            "{kind} {first:?}"
        );
        for scope in [json!(""), json!("y"), json!(5), json!(true), json!(false)] {
            assert_eq!(
                way_of("w2", &["a", "b"], &[first.clone(), scope.clone()]),
                KEYED,
                "{kind} {first:?} {scope:?}"
            );
        }
    }
    assert_eq!(
        way_of("w1", &["a"], &[json!(u64::MAX)]),
        if u64_is_keyed { KEYED } else { FALLBACK },
        "{kind} u64::MAX"
    );
    // A text with a NUL: SQLite compares it exactly. PostgreSQL's `jsonb` cannot hold one (no
    // such record can be stored, and `find` refuses the value, as it did before), so the lookup
    // goes the way of `find` and reports that error.
    crate::testing::reset_unique_runs();
    let nul = b.find_unique("w1", &["a"], &[json!("a\0b")]);
    let runs = crate::testing::unique_runs();
    if nul_is_keyed {
        assert!(nul.unwrap().is_empty(), "{kind} NUL");
        assert_eq!((runs.keyed, runs.fallback), KEYED, "{kind} NUL");
    } else {
        assert!(nul.is_err(), "{kind} NUL");
        assert_eq!((runs.keyed, runs.fallback), FALLBACK, "{kind} NUL");
    }
    // The values that have no exact text, an empty first value (the index holds no entry for it)
    // and a name the index cannot carry go the way of `find`, with the same rule.
    for first in [json!(1.5), json!([1]), json!({"k": 1}), json!(""), J::Null] {
        assert_eq!(
            way_of("w1", &["a"], std::slice::from_ref(&first)),
            FALLBACK,
            "{kind} {first:?}"
        );
    }
    for scope in [json!(1.5), json!([1]), J::Null] {
        assert_eq!(
            way_of("w2", &["a", "b"], &[json!("x"), scope.clone()]),
            FALLBACK,
            "{kind} scope {scope:?}"
        );
    }
    assert_eq!(
        way_of("w1", &["Not-An-Identifier"], &[json!("x")]),
        FALLBACK,
        "{kind}"
    );
    assert_eq!(
        way_of("Not-A-Model", &["a"], &[json!("x")]),
        FALLBACK,
        "{kind}"
    );
}

#[test]
fn the_sqlite_backend_runs_the_keyed_statement_for_text_integers_and_booleans() {
    the_way_of_each_kind_of_key(&*open("sqlite::memory:").unwrap(), false, true);
}

#[test]
fn with_postgres_the_backend_runs_the_keyed_statement_for_text_integers_and_booleans() {
    let Some(db) = TestDb::new() else {
        return;
    };
    the_way_of_each_kind_of_key(&*open(db.url()).unwrap(), true, false);
}

// ---- the plan: the index serves the lookup --------------------------------------------------

/// The statement of `Sqlite::find` before the keyed lookup (the check used it for the first field
/// of a rule).
const OLD_SQLITE_FIND: &str = "SELECT id, data FROM obop_record WHERE model = ?1 \
     AND json_extract(data, '$.' || ?2) = ?3 ORDER BY seq";

/// The plan of the statement the SQLite backend ran names the unique index of its rule and does
/// not scan; the statement the check used before (`find`) does not use that index. Each rule is
/// on its own model, so that no other index could be the one named.
#[test]
fn the_sqlite_statement_the_backend_runs_is_planned_on_its_own_unique_index() {
    use rusqlite::types::Value as Sql;
    let db = crate::sqlite::Sqlite::open(":memory:").unwrap();
    let rows: Vec<Row> = (0..3000)
        .flat_map(|n| {
            ["t1", "t2"].map(|m| {
                let mut f = Fields::new();
                f.insert("sku".into(), json!(format!("S{n}")));
                f.insert("company".into(), json!(format!("c{}", n % 7)));
                (m.to_string(), format!("{m}id{n}"), f)
            })
        })
        .collect();
    db.insert_many(&rows).unwrap();
    db.ensure_unique("obop_uq_t1_sku", "t1", &["sku"]).unwrap();
    db.ensure_unique("obop_uq_t2_sku_company", "t2", &["sku", "company"])
        .unwrap();
    let plan = |sql: &str, binds: Vec<Sql>| -> String {
        db.with_conn(|c| {
            let mut st = c.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
            let rows = st
                .query_map(rusqlite::params_from_iter(binds), |r| r.get::<_, String>(3))
                .unwrap();
            rows.map(|r| r.unwrap()).collect::<Vec<_>>().join(" | ")
        })
    };
    // (model, fields, values, the index that must be named, the binds the statement takes, the
    // record that answers). The other kinds of key use the same statement with other bound types.
    type Case = (
        &'static str,
        Vec<&'static str>,
        Vec<J>,
        &'static str,
        Vec<Sql>,
        &'static str,
    );
    let cases: Vec<Case> = vec![
        (
            "t1",
            vec!["sku"],
            vec![json!("S77")],
            "obop_uq_t1_sku",
            vec![Sql::Text("S77".into())],
            "t1id77",
        ),
        (
            "t2",
            vec!["sku", "company"],
            vec![json!("S77"), json!("c0")],
            "obop_uq_t2_sku_company",
            vec![Sql::Text("S77".into()), Sql::Text("c0".into())],
            "t2id77",
        ),
        (
            "t1",
            vec!["sku"],
            vec![json!(77)],
            "obop_uq_t1_sku",
            vec![Sql::Integer(77)],
            "",
        ),
        (
            "t2",
            vec!["sku", "company"],
            vec![json!("S77"), json!(true)],
            "obop_uq_t2_sku_company",
            vec![Sql::Text("S77".into()), Sql::Integer(1)],
            "",
        ),
    ];
    for (model, fields, values, index, binds, want) in cases {
        crate::testing::reset_unique_runs();
        let hit = ids(db.find_unique(model, &fields, &values).unwrap());
        assert_eq!(hit.join(","), want, "{model} {fields:?} {values:?}");
        let runs = crate::testing::unique_runs();
        assert_eq!((runs.keyed, runs.fallback), KEYED, "{model} {fields:?}");
        let sql = runs.last_sql.expect("the statement that ran");
        let p = plan(&sql, binds);
        assert!(
            p.contains(&format!("USING INDEX {index} (")),
            "the plan of {model} {fields:?} does not use {index}: {p}"
        );
        assert!(
            !p.contains("SCAN"),
            "the plan of {model} {fields:?} scans: {p}"
        );
    }
    // The control: what the check asked before. If this plan used the index, the assertions above
    // would prove nothing.
    let old = plan(
        OLD_SQLITE_FIND,
        vec![
            Sql::Text("t1".into()),
            Sql::Text("sku".into()),
            Sql::Text("S77".into()),
        ],
    );
    assert!(!old.contains("obop_uq_"), "{old}");
}

/// The work SQLite does for the keyed statement does not grow with the model; the work it does
/// for the old statement does (the control). The measure is the number of steps of SQLite's
/// virtual machine, `SQLITE_STMTSTATUS_VM_STEP`: it does not depend on the clock. (A count of
/// "full scan steps" cannot tell the two apart: SQLite plans the old statement as a range search
/// on the primary key's `model` prefix, which it does not call a full scan, and reads 0 for it.)
#[test]
fn the_work_sqlite_does_for_the_keyed_statement_does_not_grow_with_the_model() {
    let db = crate::sqlite::Sqlite::open(":memory:").unwrap();
    db.ensure_unique("obop_uq_t_sku", "t", &["sku"]).unwrap();
    let grow_to = |from: usize, to: usize| {
        let rows: Vec<Row> = (from..to)
            .map(|n| {
                let mut f = Fields::new();
                f.insert("sku".into(), json!(format!("S{n}")));
                ("t".to_string(), format!("id{n}"), f)
            })
            .collect();
        db.insert_many(&rows).unwrap();
    };
    let measure = || -> (u64, u64) {
        crate::testing::reset_unique_runs();
        let hit = ids(db.find_unique("t", &["sku"], &[json!("S77")]).unwrap());
        assert_eq!(hit, ["id77"]);
        let keyed = crate::testing::unique_runs().vm_steps;
        let old = db.with_conn(|c| {
            let mut st = c.prepare(OLD_SQLITE_FIND).unwrap();
            let n = st
                .query_map(rusqlite::params!["t", "sku", "S77"], |r| {
                    r.get::<_, String>(0)
                })
                .unwrap()
                .count();
            assert_eq!(n, 1);
            u64::try_from(st.get_status(rusqlite::StatementStatus::VmStep)).unwrap()
        });
        (keyed, old)
    };
    grow_to(0, 2_000);
    let (keyed_small, old_small) = measure();
    grow_to(2_000, 20_000);
    let (keyed_large, old_large) = measure();
    eprintln!(
        "VM steps at 2,000 and 20,000 rows: keyed {keyed_small} and {keyed_large}, old {old_small} and {old_large}"
    );
    // The control: the old statement visits the model, so its work is in proportion to it.
    assert!(old_small > 2_000, "{old_small}");
    assert!(old_large >= 8 * old_small, "{old_small} {old_large}");
    // The keyed statement does a handful of steps whatever the size.
    assert!(keyed_small > 0 && keyed_small < 100, "{keyed_small}");
    assert!(
        keyed_large <= keyed_small + 8,
        "{keyed_small} {keyed_large}"
    );
}

/// The same on PostgreSQL, over 2 x 20,000 rows analysed, with the planner's own choice (no
/// setting forces the index): the plan of the statement the backend ran names the exact index of
/// its rule. Each rule is on its own model, so that no other index could be the one named.
#[test]
fn with_postgres_the_statement_the_backend_runs_is_planned_on_its_own_unique_index() {
    let Some(db) = TestDb::new() else {
        return;
    };
    let b = open(db.url()).unwrap();
    b.ensure_unique("obop_uq_t1_sku", "t1", &["sku"]).unwrap();
    b.ensure_unique("obop_uq_t2_sku_company", "t2", &["sku", "company"])
        .unwrap();
    let mut c = db.connect();
    c.batch_execute(
        "INSERT INTO obop_record (model, id, data)
           SELECT m, m || 'id' || g,
                  jsonb_build_object('sku', 'S' || g, 'company', 'c' || (g % 7))
             FROM generate_series(1, 20000) g, (VALUES ('t1'), ('t2')) AS v(m);
         ANALYZE obop_record",
    )
    .unwrap();
    let plan =
        |c: &mut postgres::Client, sql: &str, params: &[&(dyn postgres::types::ToSql + Sync)]| {
            c.query(&format!("EXPLAIN {sql}"), params)
                .unwrap()
                .iter()
                .map(|r| r.get::<_, String>(0))
                .collect::<Vec<_>>()
                .join("\n")
        };
    let names = |p: &str, index: &str| {
        p.contains(&format!("using {index} on"))
            || p.contains(&format!("Bitmap Index Scan on {index} "))
    };
    let mut ask =
        |model: &str, fields: &[&str], values: &[J], texts: &[&str], index: &str, want: &str| {
            crate::testing::reset_unique_runs();
            let hit = ids(b.find_unique(model, fields, values).unwrap());
            assert_eq!(hit.join(","), want, "{model} {fields:?}");
            let runs = crate::testing::unique_runs();
            assert_eq!((runs.keyed, runs.fallback), KEYED, "{model} {fields:?}");
            let sql = runs.last_sql.expect("the statement that ran");
            let params: Vec<&(dyn postgres::types::ToSql + Sync)> =
                texts.iter().map(|t| t as _).collect();
            let p = plan(&mut c, &sql, &params);
            assert!(
                names(&p, index),
                "the plan of {model} does not use {index}: {p}"
            );
            assert!(!p.contains("Seq Scan"), "{p}");
        };
    ask(
        "t1",
        &["sku"],
        &[json!("S77")],
        &["S77"],
        "obop_uq_t1_sku",
        "t1id77",
    );
    ask(
        "t2",
        &["sku", "company"],
        &[json!("S77"), json!("c0")],
        &["S77", "c0"],
        "obop_uq_t2_sku_company",
        "t2id77",
    );
    // The control: the statement the check used before reads the model.
    let old = plan(
        &mut c,
        "SELECT id, data FROM obop_record WHERE model = $1 AND data -> $2::text = $3 ORDER BY seq",
        &[&"t1", &"sku", &json!("S77")],
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
        assert!(
            crate::sqlite::unique_lookup_sql(bad, &["a"]).is_err(),
            "{bad:?}"
        );
        assert!(
            crate::sqlite::unique_lookup_sql("t", &[bad]).is_err(),
            "{bad:?}"
        );
        assert!(
            crate::pg::unique_lookup_sql(bad, &["a"]).is_err(),
            "{bad:?}"
        );
        assert!(
            crate::pg::unique_lookup_sql("t", &[bad]).is_err(),
            "{bad:?}"
        );
    }
    assert!(crate::sqlite::unique_lookup_sql("t", &[]).is_err());
    assert!(crate::pg::unique_lookup_sql("t", &[]).is_err());
}

/// What a database that was made by an earlier version holds is what these versions make: the
/// definition the server reports for the index is pinned as text. The text is the one the code of
/// D57 made before the keyed lookup (`ensure_unique` of `sqlite.rs` and `pg.rs` at 7bb4304 builds
/// it with the same `format!`); the test needs the test-only `Sqlite::with_conn`, so it does not
/// compile unchanged against that code.
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
