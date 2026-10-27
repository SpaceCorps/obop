//! The zero-behaviour-change proof of the first slice of D75 (system fields and record history).
//!
//! One scripted run of writes (single writes, batches that succeed and batches that are refused,
//! counters, compare-and-set, a restore) goes through every backend with no model tracked. What it
//! leaves behind is compared with `golden/untracked.json`, which was written by this same script
//! on the code as it stood before D75 touched the storage layer (`main` at the merge of the storage
//! seams): the outcome of every call, the export of the database, and the bytes the record
//! columns hold (SQLite and PostgreSQL). With nothing tracked a record must not get a system
//! key, a refused batch must say what it said, and an export must carry the same rows.
//!
//! To write the file again (only when the contract changes on purpose, never to make a failure
//! go away): `OBOP_WRITE_GOLDEN=1 cargo test -p obop-storage golden`, with
//! `OBOP_TEST_POSTGRES_URL` set so that the PostgreSQL part is written too.

use super::*;
use serde_json::json;

const GOLDEN_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/golden/untracked.json");

fn f(v: J) -> Fields {
    v.as_object().cloned().expect("an object")
}

fn show<T: std::fmt::Debug>(r: std::result::Result<T, ApplyError>) -> String {
    match r {
        Ok(v) => format!("ok {v:?}"),
        // The text of a conflict is the same on every backend (it comes from the shared planner);
        // a storage error is a backend's own words, so only the fact is recorded.
        Err(ApplyError::Conflict { index, reason }) => format!("conflict {index} {reason}"),
        Err(ApplyError::Storage(_)) => "storage error".to_string(),
    }
}

fn refused<T: std::fmt::Debug>(r: Result<T>) -> String {
    match r {
        Ok(v) => format!("ok {v:?}"),
        Err(_) => "refused".to_string(),
    }
}

fn model_rows(b: &dyn Backend, model: &str) -> J {
    J::Array(
        b.list(model)
            .unwrap()
            .into_iter()
            .map(|(id, f)| json!([id, J::Object(f)]))
            .collect(),
    )
}

/// The script. Returns what each call answered.
fn script(b: &dyn Backend) -> Vec<String> {
    let mut t: Vec<String> = Vec::new();
    let (order, line, stock) = ("order", "line", "stock");

    // ---- single writes ---------------------------------------------------------------------
    // Names that D75 reserves are ordinary data while nothing is tracked, and are kept verbatim.
    t.push(refused(b.insert(
        order,
        "o1",
        &f(json!({
            "number": "SO-1", "state": "draft", "total": 100,
            "created_at": "2026-01-01T00:00:00Z", "row_version": 7,
            "nested": {"a": [1, 2, {"b": null}], "ü": "ñ"}, "note": null
        })),
    )));
    t.push(refused(b.insert(
        order,
        "o1",
        &f(json!({"number": "again"})),
    )));
    t.push(refused(b.insert(
        order,
        "o2",
        &f(json!({"number": "SO-2", "n": 1})),
    )));
    t.push(refused(b.update(
        order,
        "o1",
        &f(json!({"state": "open", "note": "x", "updated_by": "someone", "sys_backfilled": true})),
    )));
    t.push(refused(b.update(
        order,
        "ghost",
        &f(json!({"state": "open"})),
    )));
    t.push(refused(b.update(order, "o2", &f(json!({"n": null})))));
    t.push(refused(b.delete(order, "o2")));
    t.push(refused(b.delete(order, "o2")));
    t.push(refused(b.insert_many(&[
        (
            line.into(),
            "l1".into(),
            f(json!({"order": "o1", "qty": 1})),
        ),
        (
            line.into(),
            "l2".into(),
            f(json!({"order": "o1", "qty": 2})),
        ),
        (
            line.into(),
            "l3".into(),
            f(json!({"order": "o1", "qty": 3})),
        ),
    ])));
    t.push(refused(b.insert_many(&[
        (line.into(), "l4".into(), f(json!({"qty": 4}))),
        (line.into(), "l1".into(), f(json!({"qty": 9}))),
    ])));
    t.push(refused(b.get(line, "l4").map(|r| r.is_some())));

    // ---- a batch that works ----------------------------------------------------------------
    t.push(show(b.apply_checked(&[
        Op::Insert {
            model: stock.into(),
            id: "s1".into(),
            fields: f(json!({"qty": 5, "sku": "S-1"})),
        },
        Op::Merge {
            model: order.into(),
            id: "o1".into(),
            patch: f(json!({"total": 120})),
        },
        Op::add(stock, "s1", "qty", 3).min(0).max(20),
        Op::add(stock, "s9", "qty", 1).or_create(f(json!({"sku": "Z"}))),
        Op::Put {
            model: order.into(),
            id: "o3".into(),
            fields: f(json!({"number": "SO-3"})),
        },
        Op::Put {
            model: order.into(),
            id: "o3".into(),
            fields: f(json!({"number": "SO-3b"})),
        },
        Op::Patch {
            model: order.into(),
            id: "o1".into(),
            patch: f(json!({"state": "closed"})),
            expect: vec![("state".into(), json!("open")), ("missing".into(), J::Null)],
        },
        Op::Delete {
            model: line.into(),
            id: "l3".into(),
        },
    ])));

    // ---- batches that are refused: nothing of them stays -------------------------------------
    let before = (
        model_rows(b, order),
        model_rows(b, line),
        model_rows(b, stock),
    );
    let bad: Vec<(&str, Op)> = vec![
        (
            "stale expect",
            Op::Patch {
                model: order.into(),
                id: "o1".into(),
                patch: f(json!({"state": "x"})),
                expect: vec![("state".into(), json!("draft"))],
            },
        ),
        (
            "row_version expect",
            Op::Patch {
                model: order.into(),
                id: "o1".into(),
                patch: f(json!({"state": "x"})),
                expect: vec![("row_version".into(), json!(1))],
            },
        ),
        (
            "below the minimum",
            Op::add(stock, "s1", "qty", -100).min(0),
        ),
        (
            "above the maximum",
            Op::add(stock, "s1", "qty", 100).max(10),
        ),
        ("overflow", Op::add(stock, "s1", "qty", i64::MAX)),
        ("not an integer", Op::add(order, "o1", "number", 1)),
        ("missing counter", Op::add(stock, "none", "qty", 1)),
        (
            "merge missing",
            Op::Merge {
                model: order.into(),
                id: "ghost".into(),
                patch: Fields::new(),
            },
        ),
        (
            "patch missing",
            Op::Patch {
                model: order.into(),
                id: "ghost".into(),
                patch: Fields::new(),
                expect: vec![],
            },
        ),
        (
            "delete missing",
            Op::Delete {
                model: order.into(),
                id: "ghost".into(),
            },
        ),
        (
            "insert existing",
            Op::Insert {
                model: order.into(),
                id: "o1".into(),
                fields: Fields::new(),
            },
        ),
    ];
    for (name, bad_op) in bad {
        let r = b.apply_checked(&[
            Op::Insert {
                model: order.into(),
                id: "o9".into(),
                fields: f(json!({"number": "SO-9"})),
            },
            Op::Merge {
                model: order.into(),
                id: "o1".into(),
                patch: f(json!({"total": 999})),
            },
            Op::Delete {
                model: line.into(),
                id: "l2".into(),
            },
            bad_op,
        ]);
        t.push(format!("{name}: {}", show(r)));
        let after = (
            model_rows(b, order),
            model_rows(b, line),
            model_rows(b, stock),
        );
        t.push(format!("{name}: unchanged {}", before == after));
    }
    // The legacy entry points on a plain model agree with the batch ones.
    t.push(refused(b.apply(&[Op::Delete {
        model: order.into(),
        id: "ghost".into(),
    }])));
    t.push(refused(b.apply(&[Op::Merge {
        model: order.into(),
        id: "o3".into(),
        patch: f(json!({"n": 2})),
    }])));

    // ---- compare-and-set of a secret, lookups ----------------------------------------------
    t.push(refused(b.insert(
        "secret",
        "k1",
        &f(json!({"token": "sealed:1", "other": "kept"})),
    )));
    let swap = |old: &str, new: &str| Swap {
        model: "secret".into(),
        id: "k1".into(),
        field: "token".into(),
        old: old.into(),
        new: new.into(),
    };
    t.push(refused(b.swap_fields(&[swap("sealed:0", "sealed:2")])));
    t.push(refused(b.swap_fields(&[swap("sealed:1", "sealed:2")])));
    t.push(refused(
        b.find(stock, "sku", &json!("S-1"))
            .map(|r| r.into_iter().map(|(id, _)| id).collect::<Vec<_>>()),
    ));
    t.push(refused(b.count(order)));
    t.push(refused(b.count("nothing")));
    t.push(refused(b.total()));
    t
}

/// What a backend holds after the script, in the order it exports it.
fn snapshot(b: &dyn Backend, trace: Vec<String>) -> J {
    let mut rows: Vec<J> = Vec::new();
    b.export(2, &mut |batch| {
        rows.extend(
            batch
                .iter()
                .map(|(m, i, f)| json!([m, i, J::Object(f.clone())])),
        );
        Ok(())
    })
    .unwrap();
    // The restore: the same rows go back into the emptied database and come out the same.
    let mut again: Vec<J> = Vec::new();
    let mut feed: Option<Vec<Row>> = Some(
        rows.iter()
            .map(|r| {
                (
                    r[0].as_str().unwrap().to_string(),
                    r[1].as_str().unwrap().to_string(),
                    r[2].as_object().cloned().unwrap(),
                )
            })
            .collect(),
    );
    let n = b.import(true, &mut || Ok(feed.take())).unwrap();
    b.export(3, &mut |batch| {
        again.extend(
            batch
                .iter()
                .map(|(m, i, f)| json!([m, i, J::Object(f.clone())])),
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(
        rows, again,
        "export, import and export again keep every row and its order"
    );
    json!({
        "trace": trace,
        "rows": rows,
        "restored": n,
        "total_after_restore": b.total().unwrap(),
    })
}

fn sqlite_raw(path: &std::path::Path) -> J {
    let c = rusqlite::Connection::open(path).unwrap();
    let mut st = c
        .prepare("SELECT model, id, data FROM obop_record ORDER BY seq")
        .unwrap();
    let rows: Vec<J> = st
        .query_map((), |r| {
            Ok(json!([
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?
            ]))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    J::Array(rows)
}

fn postgres_raw(db: &crate::testing::TestDb) -> J {
    let mut c = db.connect();
    let rows = c
        .query(
            "SELECT model, id, data::text FROM obop_record ORDER BY seq",
            &[],
        )
        .unwrap();
    J::Array(
        rows.iter()
            .map(|r| {
                json!([
                    r.get::<_, String>(0),
                    r.get::<_, String>(1),
                    r.get::<_, String>(2)
                ])
            })
            .collect(),
    )
}

fn sorted(snap: &J) -> Vec<String> {
    let mut v: Vec<String> = snap["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r.to_string())
        .collect();
    v.sort();
    v
}

fn golden() -> J {
    let text = std::fs::read_to_string(GOLDEN_PATH)
        .unwrap_or_else(|e| panic!("cannot read {GOLDEN_PATH}: {e}"));
    serde_json::from_str(&text).expect("golden/untracked.json is JSON")
}

fn writing() -> bool {
    std::env::var("OBOP_WRITE_GOLDEN").is_ok_and(|v| v == "1")
}

/// Keep the part of the golden file this run produced, and write the file back.
fn write_part(name: &str, value: J) {
    let mut all: serde_json::Map<String, J> = std::fs::read_to_string(GOLDEN_PATH)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    all.insert(name.to_string(), value);
    std::fs::create_dir_all(std::path::Path::new(GOLDEN_PATH).parent().unwrap()).unwrap();
    std::fs::write(
        GOLDEN_PATH,
        serde_json::to_string_pretty(&J::Object(all)).unwrap() + "\n",
    )
    .unwrap();
}

fn check(name: &str, got: J) {
    if writing() {
        write_part(name, got);
        return;
    }
    let want = golden()[name].clone();
    assert!(
        !want.is_null(),
        "golden/untracked.json has no part `{name}`"
    );
    assert_eq!(
        got, want,
        "{name}: with no model tracked a backend must behave exactly as before D75"
    );
}

#[test]
fn memory_with_nothing_tracked_is_as_before() {
    let b = open("memory:").unwrap();
    assert!(b.tracked_models().is_empty());
    let snap = snapshot(&*b, script(&*b));
    check("memory", snap);
}

#[test]
fn sqlite_with_nothing_tracked_is_as_before() {
    let dir = std::env::temp_dir().join(format!("obop-golden-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("golden.db");
    let url = format!("sqlite://{}", path.display());
    let b = open(&url).unwrap();
    let mut snap = snapshot(&*b, script(&*b));
    snap["raw"] = sqlite_raw(&path);
    check("sqlite", snap);
    drop(b);
    for ext in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
    }
    let _ = std::fs::remove_dir(&dir);
    // An in-memory SQLite database does the same.
    let m = open("sqlite::memory:").unwrap();
    let again = snapshot(&*m, script(&*m));
    let g = golden();
    if !writing() {
        assert_eq!(again["rows"], g["sqlite"]["rows"]);
        assert_eq!(again["trace"], g["sqlite"]["trace"]);
    }
}

#[test]
fn postgres_with_nothing_tracked_is_as_before() {
    let Some(db) = crate::testing::TestDb::new() else {
        return;
    };
    let b = open(db.url()).unwrap();
    let mut snap = snapshot(&*b, script(&*b));
    snap["raw"] = postgres_raw(&db);
    check("postgres", snap);
}

#[test]
fn the_backends_hold_the_same_records() {
    if writing() {
        return;
    }
    let g = golden();
    let (m, s) = (sorted(&g["memory"]), sorted(&g["sqlite"]));
    assert_eq!(m, s, "memory and SQLite hold the same records");
    if !g["postgres"].is_null() {
        assert_eq!(
            m,
            sorted(&g["postgres"]),
            "PostgreSQL holds the same records"
        );
    }
    for part in ["memory", "sqlite", "postgres"] {
        if g[part].is_null() {
            continue;
        }
        for row in g[part]["rows"].as_array().unwrap() {
            let data = row[2].as_object().unwrap();
            let id = row[1].as_str().unwrap();
            if id != "o1" {
                for k in [
                    "created_at",
                    "updated_at",
                    "created_by",
                    "updated_by",
                    "row_version",
                ] {
                    assert!(!data.contains_key(k), "{part}: record {id} got a {k}");
                }
            }
        }
    }
}
