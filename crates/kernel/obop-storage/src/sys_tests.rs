//! System fields and change history (D75) on the stores that have them: memory and SQLite, each of
//! them (SQLite also in a file, which is what a restart and a restore see) through the same
//! contract. PostgreSQL has no history store yet, so it is not in this file; its behaviour with
//! nothing tracked is pinned by `golden_tests`.
//!
//! The clock of every store here is a counter the test moves, so no test waits for time to pass.

use super::*;
use crate::history::HistoryQuery;
use crate::sysfields::{self, ChangeRow, Clock, fmt_micros, parse_micros};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering::SeqCst};

/// 2026-10-26 in microseconds, an arbitrary but realistic start for the fake clock.
const T0: i64 = 1_793_000_000_000_000;

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

fn put(m: &str, id: &str, v: J) -> Op {
    Op::Put {
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

fn del(m: &str, id: &str) -> Op {
    Op::Delete {
        model: m.into(),
        id: id.into(),
    }
}

fn erase(m: &str, id: &str) -> Op {
    Op::EraseHistory {
        model: m.into(),
        id: id.into(),
    }
}

fn patch(m: &str, id: &str, v: J, expect: Vec<(&str, J)>) -> Op {
    Op::Patch {
        model: m.into(),
        id: id.into(),
        patch: f(v),
        expect: expect
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    }
}

/// A directory that is removed when the test is over. Bound to a name by its user.
struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new() -> TempDir {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "obop-sys-{}-{}",
            std::process::id(),
            N.fetch_add(1, SeqCst)
        ));
        std::fs::create_dir_all(&dir).expect("a temporary directory");
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // A file that cannot be removed is a leftover in the temporary directory, nothing more.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A store and the clock it reads.
struct Fx {
    b: Arc<Tracked>,
    now: Arc<AtomicI64>,
    name: &'static str,
    _dir: Option<TempDir>,
}

fn clock() -> (Arc<AtomicI64>, Clock) {
    let now = Arc::new(AtomicI64::new(T0));
    let source = now.clone();
    (now, Clock::from_source(move || source.load(SeqCst)))
}

impl Fx {
    fn memory() -> Fx {
        let (now, clock) = clock();
        Fx {
            b: Tracked::new(Arc::new(Memory::with_clock(clock))),
            now,
            name: "memory",
            _dir: None,
        }
    }

    fn sqlite() -> Fx {
        let (now, clock) = clock();
        Fx {
            b: Tracked::new(Arc::new(
                Sqlite::open_with_clock(":memory:", clock).expect("an in-memory database"),
            )),
            now,
            name: "sqlite in memory",
            _dir: None,
        }
    }

    fn sqlite_file() -> Fx {
        let (now, clock) = clock();
        let dir = TempDir::new();
        let path = dir.0.join("sys.db");
        let sqlite =
            Sqlite::open_with_clock(&path.display().to_string(), clock).expect("a database file");
        Fx {
            b: Tracked::new(Arc::new(sqlite)),
            now,
            name: "sqlite file",
            _dir: Some(dir),
        }
    }

    fn track(&self, model: &str, history: bool, fields: &[(&str, Treat)]) {
        self.b.track(
            model,
            Track {
                history,
                fields: fields.iter().map(|(k, t)| (k.to_string(), *t)).collect(),
                ..Track::default()
            },
        );
    }

    /// Track `model` with history, every one of `names` a plain field.
    fn track_plain(&self, model: &str, names: &[&str]) {
        let fields: Vec<_> = names.iter().map(|n| (*n, Treat::Plain)).collect();
        self.track(model, true, &fields);
    }

    fn tick(&self, us: i64) {
        self.now.fetch_add(us, SeqCst);
    }

    fn by(&self, who: &str, ops: &[Op]) -> std::result::Result<(), ApplyError> {
        self.b
            .apply_ctx(&WriteCtx::new(who, "rest", "deal.update"), ops)
    }

    fn ok(&self, who: &str, ops: &[Op]) {
        self.by(who, ops)
            .unwrap_or_else(|e| panic!("{}: {e}", self.name));
    }

    fn get(&self, model: &str, id: &str) -> Fields {
        self.b
            .get(model, id)
            .unwrap()
            .unwrap_or_else(|| panic!("{}: no {model} {id}", self.name))
    }

    fn maybe(&self, model: &str, id: &str) -> Option<Fields> {
        self.b.get(model, id).unwrap()
    }

    /// The entries of one record, oldest first.
    fn hist(&self, model: &str, id: &str) -> Vec<ChangeRow> {
        let mut v = self
            .b
            .history(&HistoryQuery::of(model, id, 100_000))
            .unwrap();
        v.reverse();
        v
    }

    fn at(&self, model: &str, id: &str, key: &str) -> i64 {
        parse_micros(self.get(model, id)[key].as_str().unwrap()).unwrap()
    }

    /// Every row of the store, in the order it exports them.
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
}

/// Run `test` on every store that has system fields and history.
fn each(test: impl Fn(&Fx)) {
    for fx in [Fx::memory(), Fx::sqlite(), Fx::sqlite_file()] {
        eprintln!("--- {}", fx.name);
        test(&fx);
    }
}

type Stamp = (String, String, String, String, i64, bool);

/// `(created_at, updated_at, created_by, updated_by, row_version, sys_backfilled)`; text that is
/// not there is empty and a version that is not there is 0.
fn stamp_of(rec: &Fields) -> Stamp {
    let s = |k: &str| rec.get(k).and_then(J::as_str).unwrap_or("").to_string();
    (
        s("created_at"),
        s("updated_at"),
        s("created_by"),
        s("updated_by"),
        rec.get("row_version").and_then(J::as_i64).unwrap_or(0),
        rec.get("sys_backfilled") == Some(&J::Bool(true)),
    )
}

fn stamp(created: i64, updated: i64, by: (&str, &str), version: i64, estimated: bool) -> Stamp {
    (
        fmt_micros(created),
        fmt_micros(updated),
        by.0.into(),
        by.1.into(),
        version,
        estimated,
    )
}

// ---- stamping ---------------------------------------------------------------------------------

#[test]
fn a_tracked_record_is_stamped_and_an_untracked_one_is_not() {
    each(|fx| {
        fx.track_plain("deal", &["name", "stage"]);
        // Not tracked: the record is what was written, system names included.
        let own = json!({"text": "x", "created_at": "mine", "row_version": 7});
        fx.ok("ann", &[ins("note", "n1", own.clone())]);
        assert_eq!(J::Object(fx.get("note", "n1")), own);
        // Tracked: whatever the write says under a system name is dropped, the store stamps.
        fx.ok(
            "ann",
            &[ins(
                "deal",
                "d1",
                json!({"name": "A", "stage": "new", "row_version": 99, "created_by": "mallory",
                       "updated_at": "1999", "sys_backfilled": true, "created_at": "then"}),
            )],
        );
        let d = fx.get("deal", "d1");
        assert_eq!(stamp_of(&d), stamp(T0, T0, ("ann", "ann"), 1, false));
        assert!(!d.contains_key("sys_backfilled"), "{d:?}");
        assert_eq!(
            (d["name"].as_str(), d["stage"].as_str()),
            (Some("A"), Some("new"))
        );
        // A change by someone else a second later: one more version, the creation is kept.
        fx.tick(1_000_000);
        fx.ok(
            "bob",
            &[merge(
                "deal",
                "d1",
                json!({"stage": "won", "row_version": 1, "updated_by": "x"}),
            )],
        );
        let d = fx.get("deal", "d1");
        assert_eq!(
            stamp_of(&d),
            stamp(T0, T0 + 1_000_000, ("ann", "bob"), 2, false)
        );
        // Put replaces the data and keeps the creation; a counter is a change too; so is Patch.
        fx.tick(1_000);
        fx.ok(
            "cy",
            &[put("deal", "d1", json!({"name": "B", "created_by": "x"}))],
        );
        let d = fx.get("deal", "d1");
        assert_eq!(d.get("stage"), None, "a put replaces the record");
        assert_eq!(
            (
                d["created_by"].as_str(),
                d["updated_by"].as_str(),
                d["row_version"].as_i64()
            ),
            (Some("ann"), Some("cy"), Some(3))
        );
        fx.ok("cy", &[Op::add("deal", "d1", "seats", 2).min(0)]);
        assert_eq!(fx.get("deal", "d1")["seats"], 2);
        assert_eq!(fx.get("deal", "d1")["row_version"], 4);
        fx.ok(
            "dee",
            &[patch(
                "deal",
                "d1",
                json!({"stage": "x"}),
                vec![("name", json!("B"))],
            )],
        );
        assert_eq!(fx.get("deal", "d1")["row_version"], 5);
        // A counter that creates the record stamps it as a creation.
        fx.ok(
            "dee",
            &[Op::add("deal", "d2", "seats", 1).or_create(f(json!({"name": "C"})))],
        );
        let d2 = fx.get("deal", "d2");
        assert_eq!(
            (
                d2["created_by"].as_str(),
                d2["row_version"].as_i64(),
                d2["seats"].as_i64()
            ),
            (Some("dee"), Some(1), Some(1))
        );
        // A delete removes the record, the stamp with it.
        fx.ok("dee", &[del("deal", "d2")]);
        assert!(fx.maybe("deal", "d2").is_none());
        // The history of d1 says all of it.
        let ops: Vec<String> = fx.hist("deal", "d1").into_iter().map(|e| e.op).collect();
        assert_eq!(ops, ["create", "update", "update", "update", "update"]);
        let versions: Vec<i64> = fx
            .hist("deal", "d1")
            .iter()
            .map(|e| e.row_version)
            .collect();
        assert_eq!(versions, [1, 2, 3, 4, 5]);
    });
}

#[test]
fn a_write_that_changes_nothing_leaves_no_version_and_no_entry() {
    each(|fx| {
        fx.track_plain("deal", &["name", "note"]);
        fx.ok(
            "ann",
            &[ins("deal", "d1", json!({"name": "A", "note": null}))],
        );
        let before = fx.get("deal", "d1");
        let entries = fx.hist("deal", "d1").len();
        fx.tick(5_000_000);
        for ops in [
            // The same value, a null for a key that is not there, a put of the same record, an
            // empty patch, a patch whose expectation holds and changes nothing, and system names
            // that are dropped.
            vec![merge("deal", "d1", json!({"name": "A"}))],
            vec![merge("deal", "d1", json!({"absent": null}))],
            vec![put("deal", "d1", json!({"name": "A"}))],
            vec![merge("deal", "d1", json!({}))],
            vec![patch(
                "deal",
                "d1",
                json!({"name": "A"}),
                vec![("row_version", json!(1))],
            )],
            vec![merge(
                "deal",
                "d1",
                json!({"row_version": 9, "updated_by": "mallory"}),
            )],
        ] {
            fx.ok("bob", &ops);
            assert_eq!(fx.get("deal", "d1"), before, "{ops:?}");
        }
        assert_eq!(fx.hist("deal", "d1").len(), entries);
    });
}

#[test]
fn a_record_that_ends_as_it_began_is_written_back_as_it_was() {
    each(|fx| {
        fx.track_plain("deal", &["x", "y"]);
        fx.ok("ann", &[ins("deal", "d1", json!({"x": 1, "y": null}))]);
        let before = fx.get("deal", "d1");
        fx.tick(1_000_000);
        // A to B to A inside one transaction.
        fx.ok(
            "bob",
            &[
                merge("deal", "d1", json!({"x": 2})),
                merge("deal", "d1", json!({"y": "t"})),
                merge("deal", "d1", json!({"x": 1, "y": null})),
            ],
        );
        assert_eq!(fx.get("deal", "d1"), before, "stamps and all");
        assert_eq!(fx.hist("deal", "d1").len(), 1, "only the creation");
        // The same across two transactions is two changes.
        fx.ok("bob", &[merge("deal", "d1", json!({"x": 2}))]);
        fx.ok("bob", &[merge("deal", "d1", json!({"x": 1}))]);
        assert_eq!(fx.get("deal", "d1")["row_version"], 3);
        assert_eq!(fx.hist("deal", "d1").len(), 3);
    });
}

#[test]
fn a_record_is_stamped_once_per_transaction() {
    each(|fx| {
        fx.track_plain("deal", &["a", "b", "c"]);
        fx.ok("ann", &[ins("deal", "d1", json!({"a": 1}))]);
        fx.tick(1_000_000);
        fx.ok(
            "bob",
            &[
                merge("deal", "d1", json!({"a": 2})),
                merge("deal", "d1", json!({"b": 2})),
                put("deal", "d1", json!({"a": 2, "b": 2, "c": 3})),
            ],
        );
        let d = fx.get("deal", "d1");
        assert_eq!(d["row_version"], 2, "three writes, one version");
        assert_eq!(fx.at("deal", "d1", "updated_at"), T0 + 1_000_000);
        let e = fx.hist("deal", "d1");
        assert_eq!(e.len(), 2);
        // One entry says all of it: before of the transaction, after of the transaction.
        assert_eq!(
            J::Object(e[1].changed.clone()),
            json!({"a": {"b": 1, "a": 2}, "b": {"a": 2}, "c": {"a": 3}})
        );
        assert_eq!(
            (e[1].op.as_str(), e[1].row_version, e[1].actor.as_str()),
            ("update", 2, "bob")
        );
        assert_eq!(e[1].at, fmt_micros(T0 + 1_000_000));
    });
}

#[test]
fn the_stamps_of_a_record_only_grow_whatever_the_clock_does() {
    each(|fx| {
        fx.track_plain("deal", &["n"]);
        fx.ok("ann", &[ins("deal", "d1", json!({"n": 0}))]);
        let mut last = fx.at("deal", "d1", "updated_at");
        for n in 1..=5 {
            // The host clock steps back, further each time.
            fx.now.store(T0 - n * 10_000_000, SeqCst);
            fx.ok("ann", &[merge("deal", "d1", json!({"n": n}))]);
            let at = fx.at("deal", "d1", "updated_at");
            assert!(at > last, "{at} must follow {last}");
            last = at;
        }
        let entries = fx.hist("deal", "d1");
        let ats: Vec<&str> = entries.iter().map(|e| e.at.as_str()).collect();
        let mut sorted = ats.clone();
        sorted.sort();
        assert_eq!(
            ats, sorted,
            "the entries are in the order of their instants"
        );
        assert_eq!(fx.get("deal", "d1")["row_version"], 6);
        // Two records written in the same instant get instants that differ, in write order.
        fx.ok(
            "ann",
            &[ins("deal", "d2", json!({})), ins("deal", "d3", json!({}))],
        );
        assert!(fx.at("deal", "d3", "created_at") > fx.at("deal", "d2", "created_at"));
    });
}

// ---- the first write of a record in a transaction ---------------------------------------------

#[test]
fn a_record_made_and_changed_in_one_transaction_has_one_entry_with_the_final_values() {
    each(|fx| {
        fx.track_plain("deal", &["a", "b"]);
        fx.ok(
            "ann",
            &[
                ins("deal", "d1", json!({"a": 1})),
                merge("deal", "d1", json!({"a": 2, "b": "x"})),
                merge("deal", "d1", json!({"b": null})),
            ],
        );
        assert_eq!(fx.get("deal", "d1")["row_version"], 1);
        let e = fx.hist("deal", "d1");
        assert_eq!(e.len(), 1);
        assert_eq!((e[0].op.as_str(), e[0].row_version), ("create", 1));
        assert_eq!(J::Object(e[0].changed.clone()), json!({"a": {"a": 2}}));
        // Made and deleted in one transaction: nothing was ever there.
        fx.ok(
            "ann",
            &[ins("deal", "d2", json!({"a": 1})), del("deal", "d2")],
        );
        assert!(fx.maybe("deal", "d2").is_none());
        assert!(fx.hist("deal", "d2").is_empty());
        // Made, deleted, made again: the second life is a creation of its own.
        fx.ok(
            "ann",
            &[
                ins("deal", "d3", json!({"a": 1})),
                del("deal", "d3"),
                ins("deal", "d3", json!({"a": 5})),
            ],
        );
        let e = fx.hist("deal", "d3");
        assert_eq!(e.len(), 1);
        assert_eq!(J::Object(e[0].changed.clone()), json!({"a": {"a": 5}}));
        assert_eq!(fx.get("deal", "d3")["row_version"], 1);
        // An existing record deleted and made again in one transaction: the old one ends, a new
        // one begins.
        fx.ok(
            "ann",
            &[del("deal", "d1"), ins("deal", "d1", json!({"a": 9}))],
        );
        let e = fx.hist("deal", "d1");
        assert_eq!(
            e.iter().map(|x| x.op.as_str()).collect::<Vec<_>>(),
            ["create", "delete", "create"]
        );
        assert_eq!(fx.get("deal", "d1")["a"], 9);
        assert_eq!(fx.get("deal", "d1")["row_version"], 1);
    });
}

#[test]
fn a_record_changed_and_then_deleted_leaves_two_entries() {
    each(|fx| {
        fx.track_plain("deal", &["a", "b"]);
        fx.ok("ann", &[ins("deal", "d1", json!({"a": 1, "b": "keep"}))]);
        fx.tick(1_000_000);
        fx.ok(
            "bob",
            &[merge("deal", "d1", json!({"a": 2})), del("deal", "d1")],
        );
        assert!(fx.maybe("deal", "d1").is_none());
        let e = fx.hist("deal", "d1");
        assert_eq!(e.len(), 3, "{e:?}");
        assert_eq!((e[1].op.as_str(), e[1].row_version), ("update", 2));
        assert_eq!(
            J::Object(e[1].changed.clone()),
            json!({"a": {"b": 1, "a": 2}})
        );
        // The delete carries the last values and the version the record would have had.
        assert_eq!((e[2].op.as_str(), e[2].row_version), ("delete", 3));
        assert_eq!(
            J::Object(e[2].changed.clone()),
            json!({"a": {"b": 2}, "b": {"b": "keep"}})
        );
        assert!(e[2].at > e[1].at, "the delete follows the change");
        assert_eq!(e[2].actor, "bob");
        // A delete of an unchanged record is one entry; a delete after a change that was put back
        // is too, and the version does not skip.
        fx.ok("ann", &[ins("deal", "d2", json!({"a": 1}))]);
        fx.ok("bob", &[del("deal", "d2")]);
        let e = fx.hist("deal", "d2");
        assert_eq!(
            e.iter()
                .map(|x| (x.op.as_str(), x.row_version))
                .collect::<Vec<_>>(),
            [("create", 1), ("delete", 2)]
        );
        fx.ok("ann", &[ins("deal", "d3", json!({"a": 1}))]);
        fx.ok(
            "bob",
            &[
                merge("deal", "d3", json!({"a": 2})),
                merge("deal", "d3", json!({"a": 1})),
                del("deal", "d3"),
            ],
        );
        let e = fx.hist("deal", "d3");
        assert_eq!(
            e.iter()
                .map(|x| (x.op.as_str(), x.row_version))
                .collect::<Vec<_>>(),
            [("create", 1), ("delete", 2)]
        );
        // Deleting what is not there, or changing what is not there, is a conflict.
        assert!(matches!(
            fx.by("bob", &[del("deal", "nothing")]),
            Err(ApplyError::Conflict { index: 0, .. })
        ));
        assert!(matches!(
            fx.by("bob", &[merge("deal", "nothing", json!({}))]),
            Err(ApplyError::Conflict { index: 0, .. })
        ));
    });
}

#[test]
fn a_bulk_batch_is_one_transaction_with_one_entry_per_row_in_order() {
    each(|fx| {
        fx.track_plain("deal", &["n"]);
        let ctx = WriteCtx::new("ann", "rest", "deal.bulk_create")
            .path("batch")
            .batch("batch-1");
        let rows: Vec<Op> = (0..500)
            .map(|i| ins("deal", &format!("d{i:03}"), json!({"n": i})))
            .collect();
        fx.b.apply_ctx(&ctx, &rows).unwrap();
        assert_eq!(fx.b.count("deal").unwrap(), 500);
        let mut seqs = Vec::new();
        for i in 0..500 {
            let e = fx.hist("deal", &format!("d{i:03}"));
            assert_eq!(e.len(), 1);
            assert_eq!(e[0].batch_id.as_deref(), Some("batch-1"));
            assert_eq!(e[0].via["path"], "batch");
            assert_eq!(e[0].via["command"], "deal.bulk_create");
            seqs.push(e[0].seq);
        }
        assert!(
            seqs.windows(2).all(|w| w[0] < w[1]),
            "the order of the rows"
        );
        // All or nothing: a duplicate in the last row leaves no record and no entry.
        let mut more: Vec<Op> = (0..100)
            .map(|i| ins("deal", &format!("e{i:03}"), json!({"n": i})))
            .collect();
        more.push(ins("deal", "d000", json!({"n": 0})));
        let r = fx.b.apply_ctx(&ctx, &more);
        assert!(
            matches!(r, Err(ApplyError::Conflict { index: 100, .. })),
            "{r:?}"
        );
        assert_eq!(fx.b.count("deal").unwrap(), 500);
        assert!(fx.hist("deal", "e000").is_empty());
        assert_eq!(fx.b.total().unwrap(), 1000, "records and entries");
    });
}

// ---- who and how -----------------------------------------------------------------------------

#[test]
fn an_entry_says_who_and_how_and_a_governed_move_is_a_transition() {
    each(|fx| {
        fx.track_plain("deal", &["stage", "note"]);
        fx.track_plain("task", &["title"]);
        fx.ok(
            "ann",
            &[
                ins("deal", "d1", json!({"stage": "new"})),
                ins("task", "t1", json!({"title": "t"})),
            ],
        );
        fx.tick(1_000);
        let mut ctx = WriteCtx::new("ann", "mcp", "deal.win")
            .primary("deal", "d1")
            .run("run-9");
        ctx.via.agent = true;
        ctx.via.token_ref = Some("0123456789ab".into());
        ctx.via.request_id = Some("req-1".into());
        ctx.via.approver = Some("boss".into());
        fx.b.apply_ctx(
            &ctx,
            &[
                merge("deal", "d1", json!({"stage": "won"})),
                merge("task", "t1", json!({"title": "follow up"})),
                ins("task", "t2", json!({"title": "new"})),
            ],
        )
        .unwrap();
        let primary = fx.hist("deal", "d1").pop().unwrap();
        assert_eq!(primary.op, "transition");
        assert_eq!(primary.actor, "ann");
        assert_eq!(primary.run_id.as_deref(), Some("run-9"));
        assert_eq!(
            J::Object(primary.via.clone()),
            json!({"surface": "mcp", "path": "direct", "command": "deal.win",
                   "request_id": "req-1", "token_ref": "0123456789ab", "agent": true,
                   "approver": "boss"})
        );
        // The other records of the call are effects of it.
        let effect = fx.hist("task", "t1").pop().unwrap();
        assert_eq!(
            (effect.op.as_str(), effect.via["path"].as_str()),
            ("update", Some("effect"))
        );
        let made = fx.hist("task", "t2").pop().unwrap();
        assert_eq!(
            (made.op.as_str(), made.via["path"].as_str()),
            ("create", Some("effect"))
        );
        // Without a primary record nothing is an effect.
        fx.ok("bob", &[merge("deal", "d1", json!({"note": "n"}))]);
        let plain = fx.hist("deal", "d1").pop().unwrap();
        assert_eq!(
            (plain.op.as_str(), plain.via["path"].as_str()),
            ("update", Some("direct"))
        );
    });
}

#[test]
fn the_old_entry_points_stamp_a_tracked_model_with_the_context_they_find() {
    each(|fx| {
        fx.track_plain("deal", &["n"]);
        // No scope: the system writes.
        fx.b.insert("deal", "d1", &f(json!({"n": 1}))).unwrap();
        assert_eq!(fx.get("deal", "d1")["created_by"], "system:internal");
        assert_eq!(fx.hist("deal", "d1")[0].via["surface"], "system");
        {
            let _s = crate::ctx::scope(WriteCtx::new("ann", "cli", "x"));
            fx.tick(1_000);
            assert!(fx.b.update("deal", "d1", &f(json!({"n": 2}))).unwrap());
            fx.b.insert_many(&[
                ("deal".into(), "d2".into(), f(json!({"n": 1}))),
                ("deal".into(), "d3".into(), f(json!({"n": 1}))),
            ])
            .unwrap();
            assert!(!fx.b.update("deal", "ghost", &Fields::new()).unwrap());
            assert!(!fx.b.delete("deal", "ghost").unwrap());
            assert!(fx.b.delete("deal", "d3").unwrap());
            assert!(
                fx.b.insert("deal", "d1", &Fields::new()).is_err(),
                "duplicate"
            );
        }
        let d1 = fx.get("deal", "d1");
        assert_eq!(
            (d1["updated_by"].as_str(), d1["created_by"].as_str()),
            (Some("ann"), Some("system:internal"))
        );
        assert_eq!(fx.get("deal", "d2")["created_by"], "ann");
        assert_eq!(fx.hist("deal", "d3").len(), 2, "created and deleted");
        // Untracked again, a model is written as it always was; what was kept stays readable.
        fx.b.untrack("deal");
        fx.b.insert("deal", "d9", &f(json!({"n": 9}))).unwrap();
        assert_eq!(J::Object(fx.get("deal", "d9")), json!({"n": 9}));
        assert!(fx.hist("deal", "d9").is_empty());
        assert_eq!(fx.hist("deal", "d1").len(), 2);
    });
}

// ---- what is written into an entry -------------------------------------------------------------

/// Seals by hiding the text under an obvious transformation: enough to see what was sealed, for
/// which purpose, and that the plain text is nowhere in it.
struct TestSealer;

impl Sealer for TestSealer {
    fn seal(&self, purpose: &str, plain: &str) -> Option<String> {
        let hex: String = plain.bytes().map(|b| format!("{:02x}", b ^ 0x5a)).collect();
        Some(format!("env1:{purpose}:{hex}"))
    }

    fn open(&self, purpose: &str, sealed: &str) -> Option<String> {
        let hex = sealed.strip_prefix(&format!("env1:{purpose}:"))?;
        let bytes: Option<Vec<u8>> = (0..hex.len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(&hex[i..i + 2], 16)
                    .ok()
                    .map(|b| b ^ 0x5a)
            })
            .collect();
        String::from_utf8(bytes?).ok()
    }
}

/// A key ring that has lost its key.
struct NoKey;

impl Sealer for NoKey {
    fn seal(&self, _: &str, _: &str) -> Option<String> {
        None
    }
    fn open(&self, _: &str, _: &str) -> Option<String> {
        None
    }
}

/// Every entry of the record as text, to look for a value that must not be there.
fn everything(fx: &Fx, model: &str, id: &str) -> String {
    fx.hist(model, id)
        .iter()
        .map(|e| format!("{e:?}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_secret_is_never_stored_and_an_undeclared_field_is_treated_like_one() {
    each(|fx| {
        fx.track(
            "login",
            true,
            &[("name", Treat::Plain), ("pin", Treat::Redact)],
        );
        fx.ok(
            "ann",
            &[ins(
                "login",
                "l1",
                json!({"name": "ann", "pin": "CANARY-PIN-1", "stray": "CANARY-STRAY-1"}),
            )],
        );
        fx.ok(
            "ann",
            &[merge(
                "login",
                "l1",
                json!({"pin": "CANARY-PIN-2", "stray": "CANARY-STRAY-2", "name": "Ann"}),
            )],
        );
        fx.ok("ann", &[del("login", "l1")]);
        let e = fx.hist("login", "l1");
        assert_eq!(e.len(), 3);
        assert_eq!(e[0].changed["pin"], json!({"t": "redacted"}));
        assert_eq!(e[0].changed["stray"], json!({"t": "redacted"}));
        assert_eq!(
            e[1].changed["pin"],
            json!({"t": "redacted"}),
            "it changed, and that is all"
        );
        assert_eq!(e[1].changed["name"], json!({"b": "ann", "a": "Ann"}));
        let all = everything(fx, "login", "l1");
        assert!(!all.contains("CANARY"), "{all}");
        // Unchanged, a secret leaves no mark.
        fx.ok(
            "ann",
            &[ins(
                "login",
                "l2",
                json!({"name": "x", "pin": "CANARY-PIN-3"}),
            )],
        );
        fx.ok(
            "ann",
            &[merge(
                "login",
                "l2",
                json!({"pin": "CANARY-PIN-3", "name": "y"}),
            )],
        );
        let e = fx.hist("login", "l2");
        assert!(!e[1].changed.contains_key("pin"));
        assert!(!everything(fx, "login", "l2").contains("CANARY"));
    });
}

#[test]
fn a_sensitive_value_is_sealed_under_the_key_ring_and_redacted_without_one() {
    each(|fx| {
        fx.track(
            "patient",
            true,
            &[("name", Treat::Plain), ("diagnosis", Treat::Seal)],
        );
        // No key ring at all: the fact that it changed, nothing more.
        fx.ok(
            "ann",
            &[ins(
                "patient",
                "p0",
                json!({"name": "n", "diagnosis": "CANARY-DX-0"}),
            )],
        );
        let e = fx.hist("patient", "p0");
        assert_eq!(e[0].changed["diagnosis"], json!({"t": "redacted"}));
        assert!(!e[0].sealed);
        assert!(!everything(fx, "patient", "p0").contains("CANARY"));
        // A key ring that cannot seal does the same.
        fx.b.set_sealer(Arc::new(NoKey));
        fx.ok(
            "ann",
            &[ins("patient", "p1", json!({"diagnosis": "CANARY-DX-1"}))],
        );
        assert_eq!(
            fx.hist("patient", "p1")[0].changed["diagnosis"],
            json!({"t": "redacted"})
        );
        assert!(!everything(fx, "patient", "p1").contains("CANARY"));
        // With a working one the value is in the entry, sealed, for this field of this model.
        fx.b.set_sealer(Arc::new(TestSealer));
        assert!(fx.b.sealer().is_some());
        fx.ok(
            "ann",
            &[ins(
                "patient",
                "p2",
                json!({"name": "n", "diagnosis": "flu"}),
            )],
        );
        fx.ok(
            "bob",
            &[merge("patient", "p2", json!({"diagnosis": "cold"}))],
        );
        let e = fx.hist("patient", "p2");
        assert!(e[0].sealed && e[1].sealed);
        assert_eq!(
            e[0].changed["name"],
            json!({"a": "n"}),
            "plain fields stay plain"
        );
        let d = &e[1].changed["diagnosis"];
        assert_eq!(d["t"], "sealed");
        let purpose = "history.v1:patient.diagnosis";
        assert_eq!(
            TestSealer
                .open(purpose, d["b"].as_str().unwrap())
                .as_deref(),
            Some("\"flu\"")
        );
        assert_eq!(
            TestSealer
                .open(purpose, d["a"].as_str().unwrap())
                .as_deref(),
            Some("\"cold\"")
        );
        assert!(
            TestSealer
                .open("history.v1:patient.name", d["a"].as_str().unwrap())
                .is_none(),
            "a sealed value cannot be moved to another field"
        );
        let all = everything(fx, "patient", "p2");
        assert!(!all.contains("flu") && !all.contains("cold"), "{all}");
        // Structured values are sealed as their JSON text.
        fx.ok(
            "bob",
            &[merge(
                "patient",
                "p2",
                json!({"diagnosis": {"code": "J11"}}),
            )],
        );
        let last = fx.hist("patient", "p2").pop().unwrap();
        let a = last.changed["diagnosis"]["a"].as_str().unwrap().to_string();
        assert_eq!(
            TestSealer.open(purpose, &a).as_deref(),
            Some("{\"code\":\"J11\"}")
        );
        // The key ring belongs to the store, not to a model.
        fx.b.untrack("patient");
        assert!(fx.b.sealer().is_some());
    });
}

#[test]
fn a_value_over_32_kib_is_a_marker_with_its_length() {
    each(|fx| {
        fx.track_plain("doc", &["body", "title"]);
        let max = sysfields::HISTORY_VALUE_MAX;
        // As JSON text the value is two quotes longer than the characters in it.
        let fits = "x".repeat(max - 2);
        let big = "y".repeat(max - 1);
        fx.ok(
            "ann",
            &[ins("doc", "d1", json!({"body": fits, "title": "t"}))],
        );
        assert_eq!(fx.hist("doc", "d1")[0].changed["body"], json!({"a": fits}));
        fx.ok("ann", &[merge("doc", "d1", json!({"body": big.clone()}))]);
        let e = fx.hist("doc", "d1").pop().unwrap();
        assert_eq!(e.changed["body"], json!({"t": "large", "len": max + 1}));
        assert!(!everything(fx, "doc", "d1").contains(&big));
        // From large to small: the marker still has no value on either side.
        fx.ok("ann", &[merge("doc", "d1", json!({"body": "short"}))]);
        let e = fx.hist("doc", "d1").pop().unwrap();
        assert_eq!(e.changed["body"], json!({"t": "large", "len": max + 1}));
        // The record itself holds the value as written.
        fx.ok("ann", &[merge("doc", "d1", json!({"body": big.clone()}))]);
        assert_eq!(fx.get("doc", "d1")["body"], big.as_str());
    });
}

#[test]
fn a_model_without_history_is_stamped_and_logs_nothing() {
    each(|fx| {
        fx.track("counter", false, &[]);
        fx.ok("ann", &[ins("counter", "c1", json!({"n": 1}))]);
        fx.ok(
            "bob",
            &[
                Op::add("counter", "c1", "n", 1),
                merge("counter", "c1", json!({"m": 1})),
            ],
        );
        let c = fx.get("counter", "c1");
        assert_eq!(
            (
                c["row_version"].as_i64(),
                c["updated_by"].as_str(),
                c["n"].as_i64()
            ),
            (Some(2), Some("bob"), Some(2))
        );
        fx.ok("bob", &[del("counter", "c1")]);
        assert!(fx.hist("counter", "c1").is_empty());
        assert_eq!(fx.b.total().unwrap(), 0, "no record, no entry");
    });
}

// ---- compare and set on the version ------------------------------------------------------------

#[test]
fn a_patch_can_require_the_version_and_an_absent_version_reads_as_one() {
    each(|fx| {
        fx.track_plain("deal", &["stage", "x"]);
        fx.ok("ann", &[ins("deal", "d1", json!({"stage": "new"}))]);
        // The version the client saw.
        fx.ok(
            "bob",
            &[patch(
                "deal",
                "d1",
                json!({"stage": "open"}),
                vec![("row_version", json!(1))],
            )],
        );
        // It is stale now: the reason carries the current one, and nothing changed.
        let r = fx.by(
            "cy",
            &[patch(
                "deal",
                "d1",
                json!({"stage": "lost"}),
                vec![("row_version", json!(1))],
            )],
        );
        let Err(ApplyError::Conflict { index: 0, reason }) = r else {
            panic!("{r:?}");
        };
        assert_eq!(reason, format!("{}2", sysfields::STALE_PREFIX));
        assert_eq!(fx.get("deal", "d1")["stage"], "open");
        // Another field that differs is the old reason; both at once: the version wins, because
        // that is what the caller can act on.
        let r = fx.by(
            "cy",
            &[patch(
                "deal",
                "d1",
                json!({}),
                vec![("stage", json!("new"))],
            )],
        );
        assert_eq!(
            r,
            Err(ApplyError::Conflict {
                index: 0,
                reason: "the record changed".into()
            })
        );
        let r = fx.by(
            "cy",
            &[patch(
                "deal",
                "d1",
                json!({}),
                vec![("stage", json!("new")), ("row_version", json!(7))],
            )],
        );
        assert_eq!(
            r,
            Err(ApplyError::Conflict {
                index: 0,
                reason: "stale_version:2".into()
            })
        );
        // A guard goes first: it checks the version the record has, and the writes after it
        // belong to the same new version.
        fx.ok(
            "cy",
            &[
                patch(
                    "deal",
                    "d1",
                    json!({"x": 1}),
                    vec![("row_version", json!(2))],
                ),
                merge("deal", "d1", json!({"x": 2})),
            ],
        );
        assert_eq!(fx.get("deal", "d1")["row_version"], 3);
        // Checked after a write of the same transaction, the version is the new one.
        let r = fx.by(
            "cy",
            &[
                merge("deal", "d1", json!({"x": 3})),
                patch("deal", "d1", json!({}), vec![("row_version", json!(3))]),
            ],
        );
        assert_eq!(
            r,
            Err(ApplyError::Conflict {
                index: 1,
                reason: "stale_version:4".into()
            })
        );
        assert_eq!(fx.get("deal", "d1")["x"], 2, "and the batch was refused");
        // A record from before the tracking has no version, and reads as 1.
        fx.b.untrack("deal");
        fx.ok("ann", &[ins("deal", "old", json!({"stage": "x"}))]);
        fx.track_plain("deal", &["stage", "x"]);
        let r = fx.by(
            "cy",
            &[patch(
                "deal",
                "old",
                json!({}),
                vec![("row_version", json!(2))],
            )],
        );
        assert_eq!(
            r,
            Err(ApplyError::Conflict {
                index: 0,
                reason: "stale_version:1".into()
            })
        );
        fx.ok(
            "cy",
            &[patch(
                "deal",
                "old",
                json!({"stage": "y"}),
                vec![("row_version", json!(1))],
            )],
        );
        assert_eq!(fx.get("deal", "old")["row_version"], 2);
        // A model that is not tracked has no version: the expectation is on a plain field.
        fx.ok("ann", &[ins("note", "n1", json!({"a": 1}))]);
        let r = fx.by(
            "ann",
            &[patch(
                "note",
                "n1",
                json!({}),
                vec![("row_version", json!(1))],
            )],
        );
        assert_eq!(
            r,
            Err(ApplyError::Conflict {
                index: 0,
                reason: "the record changed".into()
            })
        );
        // A missing record is still a missing record.
        let r = fx.by(
            "ann",
            &[patch(
                "deal",
                "none",
                json!({}),
                vec![("row_version", json!(1))],
            )],
        );
        assert_eq!(
            r,
            Err(ApplyError::Conflict {
                index: 0,
                reason: "no such record".into()
            })
        );
    });
}

#[test]
fn two_writers_with_the_same_version_cannot_both_win() {
    each(|fx| {
        fx.track_plain("deal", &["stage"]);
        fx.ok("ann", &[ins("deal", "d1", json!({"stage": "new"}))]);
        let v = fx.get("deal", "d1")["row_version"].clone();
        let try_it = |who: &str, to: &str| {
            fx.by(
                who,
                &[patch(
                    "deal",
                    "d1",
                    json!({"stage": to}),
                    vec![("row_version", v.clone())],
                )],
            )
        };
        let (a, b) = (try_it("ann", "a"), try_it("bob", "b"));
        assert!(
            a.is_ok() && matches!(b, Err(ApplyError::Conflict { .. })),
            "{a:?} {b:?}"
        );
        assert_eq!(fx.get("deal", "d1")["stage"], "a");
        assert_eq!(fx.hist("deal", "d1").len(), 2);
    });
}

// ---- records from before the tracking ------------------------------------------------------------

/// A UUIDv7-shaped id that carries `ms` and ends in `tail` (12 hex digits).
fn v7_id(ms: i64, tail: &str) -> String {
    format!("{:08x}-{:04x}-7abc-8def-{tail}", ms >> 16, ms & 0xffff)
}

#[test]
fn a_record_from_before_the_tracking_is_stamped_when_it_is_first_written() {
    each(|fx| {
        let ms = (T0 - 86_400_000_000) / 1000;
        let v7 = v7_id(ms, "0123456789ab");
        fx.ok(
            "ann",
            &[
                ins(
                    "deal",
                    &v7,
                    json!({"stage": "old", "created_by": "from-an-old-field"}),
                ),
                ins("deal", "plain-id", json!({"stage": "old"})),
                ins("deal", "untouched", json!({"stage": "old"})),
            ],
        );
        fx.track_plain("deal", &["stage"]);
        fx.tick(1_000_000);
        fx.ok(
            "bob",
            &[
                merge("deal", &v7, json!({"stage": "new"})),
                merge("deal", "plain-id", json!({"stage": "new"})),
            ],
        );
        let a = fx.get("deal", &v7);
        assert_eq!(
            stamp_of(&a),
            stamp(ms * 1000, T0 + 1_000_000, ("", "bob"), 2, true),
            "created from the id, estimated, no creator"
        );
        let b = fx.get("deal", "plain-id");
        assert_eq!(b["created_at"], b["updated_at"], "no time in the id: now");
        assert!(fx.at("deal", "plain-id", "created_at") >= T0 + 1_000_000);
        assert_eq!(
            (b["row_version"].as_i64(), b["sys_backfilled"].as_bool()),
            (Some(2), Some(true))
        );
        // The entry is an update of version 2, and a field the old record had under a reserved
        // name is not part of what changed.
        let e = fx.hist("deal", &v7);
        assert_eq!(e.len(), 1);
        assert_eq!((e[0].op.as_str(), e[0].row_version), ("update", 2));
        assert_eq!(
            J::Object(e[0].changed.clone()),
            json!({"stage": {"b": "old", "a": "new"}})
        );
        // Written again, it is an ordinary record that is still known to be an estimate.
        fx.tick(1_000);
        fx.ok("cy", &[merge("deal", &v7, json!({"stage": "again"}))]);
        let a = fx.get("deal", &v7);
        assert_eq!(
            (
                a["row_version"].as_i64(),
                a.get("created_by").and_then(J::as_str)
            ),
            (Some(3), None)
        );
        assert_eq!(a["sys_backfilled"], true);
        // Read only, it is not touched; deleted, it says what version the delete is.
        assert_eq!(fx.get("deal", "untouched").get("row_version"), None);
        fx.ok("cy", &[del("deal", "untouched")]);
        let e = fx.hist("deal", "untouched");
        assert_eq!(e.len(), 1);
        assert_eq!((e[0].op.as_str(), e[0].row_version), ("delete", 2));
        assert_eq!(
            J::Object(e[0].changed.clone()),
            json!({"stage": {"b": "old"}})
        );
    });
}

#[test]
fn the_backfill_stamps_old_records_in_passes_and_nothing_else() {
    each(|fx| {
        let ms = T0 / 1000 - 5_000;
        let ids: Vec<String> = (0..5)
            .map(|n| v7_id(ms, &format!("0123456789a{n:x}")))
            .collect();
        let mut ops: Vec<Op> = ids
            .iter()
            .enumerate()
            .map(|(n, id)| ins("deal", id, json!({"n": n})))
            .collect();
        ops.push(ins("deal", "plain", json!({})));
        ops.push(ins("other", "o", json!({})));
        fx.ok("ann", &ops);
        let fallback = fmt_micros(T0 - 1_000_000);
        let models = ["deal".to_string()];
        let bf = |batch| fx.b.backfill_sysfields(&models, &fallback, batch).unwrap();
        assert_eq!(
            bf(2),
            Backfilled {
                stamped: 2,
                remaining: 4
            }
        );
        assert_eq!(
            bf(2),
            Backfilled {
                stamped: 2,
                remaining: 2
            }
        );
        assert_eq!(
            bf(2),
            Backfilled {
                stamped: 2,
                remaining: 0
            }
        );
        assert_eq!(
            bf(2),
            Backfilled {
                stamped: 0,
                remaining: 0
            },
            "nothing is left to do"
        );
        assert_eq!(
            stamp_of(&fx.get("deal", &ids[0])),
            stamp(ms * 1000, ms * 1000, ("", ""), 1, true)
        );
        assert_eq!(fx.get("deal", "plain")["created_at"], fallback.as_str());
        assert_eq!(
            fx.get("other", "o").get("row_version"),
            None,
            "another model is not touched"
        );
        assert_eq!(fx.get("deal", &ids[3])["n"], 3);
        assert!(
            fx.b.history(&HistoryQuery::of("deal", &ids[0], 10))
                .unwrap()
                .is_empty(),
            "no entries"
        );
        // A fallback that is not an instant is refused before anything is done.
        fx.ok("ann", &[ins("deal", "later", json!({}))]);
        assert!(fx.b.backfill_sysfields(&models, "yesterday", 10).is_err());
        assert_eq!(fx.get("deal", "later").get("row_version"), None);
    });
}

// ---- a batch that fails, batches that mix --------------------------------------------------------

#[test]
fn a_refused_batch_leaves_no_record_no_stamp_and_no_entry() {
    each(|fx| {
        fx.track_plain("deal", &["x"]);
        fx.ok(
            "ann",
            &[
                ins("deal", "d1", json!({"x": 1})),
                ins("note", "n1", json!({"x": 1})),
            ],
        );
        let before = fx.rows();
        fx.tick(1_000_000);
        for last in [
            ins("deal", "d2", json!({})),
            merge("deal", "ghost", json!({})),
            del("deal", "ghost"),
            patch("deal", "d1", json!({}), vec![("row_version", json!(9))]),
            Op::add("deal", "d1", "x", i64::MAX),
            merge("note", "ghost", json!({})),
        ] {
            let r = fx.by(
                "bob",
                &[
                    merge("deal", "d1", json!({"x": 2})),
                    ins("deal", "d2", json!({"x": 1})),
                    merge("note", "n1", json!({"x": 2})),
                    last.clone(),
                ],
            );
            assert!(r.is_err(), "{last:?}");
            assert_eq!(fx.rows(), before, "{last:?}");
        }
        // The same when the failing step comes after an erasure of history.
        let r = fx.by(
            "bob",
            &[
                merge("deal", "d1", json!({"x": 3})),
                erase("deal", "d1"),
                del("deal", "ghost"),
            ],
        );
        assert!(r.is_err());
        assert_eq!(fx.rows(), before);
    });
}

#[test]
fn tracked_and_untracked_models_in_one_batch_each_do_their_own_thing() {
    each(|fx| {
        fx.track_plain("deal", &["x"]);
        fx.ok(
            "ann",
            &[
                ins("note", "n1", json!({"x": 1})),
                ins("deal", "d1", json!({"x": 1})),
                merge("note", "n1", json!({"x": 2, "row_version": 5})),
                put("note", "n2", json!({"created_at": "mine"})),
                merge("deal", "d1", json!({"x": 2})),
            ],
        );
        assert_eq!(
            J::Object(fx.get("note", "n1")),
            json!({"x": 2, "row_version": 5})
        );
        assert_eq!(
            J::Object(fx.get("note", "n2")),
            json!({"created_at": "mine"})
        );
        assert_eq!(fx.get("deal", "d1")["row_version"], 1);
        assert_eq!(fx.get("deal", "d1")["x"], 2);
        assert_eq!(fx.hist("deal", "d1").len(), 1);
        assert!(fx.hist("note", "n1").is_empty());
    });
}

// ---- the history as it is read -------------------------------------------------------------------

#[test]
fn the_history_of_a_record_is_read_newest_first_in_pages_and_by_operation() {
    each(|fx| {
        fx.track_plain("deal", &["n"]);
        fx.track_plain("task", &["n"]);
        fx.ok(
            "ann",
            &[
                ins("deal", "d1", json!({"n": 0})),
                ins("task", "d1", json!({"n": 0})),
            ],
        );
        for n in 1..=6 {
            fx.tick(1_000_000);
            fx.ok("ann", &[merge("deal", "d1", json!({"n": n}))]);
        }
        fx.ok("ann", &[del("deal", "d1")]);
        let q = |limit: usize, tweak: &dyn Fn(&mut HistoryQuery)| {
            let mut q = HistoryQuery::of("deal", "d1", limit);
            tweak(&mut q);
            fx.b.history(&q).unwrap()
        };
        let all = q(100, &|_| {});
        assert_eq!(all.len(), 8);
        assert!(all.windows(2).all(|w| w[0].seq > w[1].seq), "newest first");
        assert_eq!(all[0].op, "delete");
        assert!(
            all.iter().all(|e| e.model == "deal" && e.record_id == "d1"),
            "not the entries of the other model with the same id"
        );
        // Pages of three, each after the last seq of the one before.
        let mut seen = Vec::new();
        let mut before = None;
        loop {
            let page = q(3, &|f| f.before_seq = before);
            if page.is_empty() {
                break;
            }
            assert!(page.len() <= 3);
            before = page.last().map(|e| e.seq);
            seen.extend(page.into_iter().map(|e| e.seq));
        }
        assert_eq!(seen, all.iter().map(|e| e.seq).collect::<Vec<_>>());
        assert_eq!(q(100, &|f| f.op = Some("update".into())).len(), 6);
        let since = fmt_micros(T0 + 4_000_000);
        assert_eq!(
            q(100, &|f| f.since_at = Some(since.clone())).len(),
            4,
            "the entries at or after that instant"
        );
        assert_eq!(q(0, &|_| {}).len(), 0);
        let none = HistoryQuery::of("deal", "nothing", 5);
        assert!(fx.b.history(&none).unwrap().is_empty());
    });
}

// ---- erasure --------------------------------------------------------------------------------------

#[test]
fn an_erasure_removes_the_values_from_the_history_and_says_so_without_values() {
    each(|fx| {
        fx.b.track(
            "person",
            Track {
                history: true,
                fields: [
                    ("email".to_string(), Treat::Plain),
                    ("stage".to_string(), Treat::Plain),
                    ("phone".to_string(), Treat::Plain),
                ]
                .into(),
                keep_on_erase: ["stage".to_string()].into(),
                retention_days: None,
            },
        );
        fx.ok(
            "ann",
            &[ins(
                "person",
                "p1",
                json!({"email": "CANARY-A@x", "stage": "lead", "phone": "CANARY-P"}),
            )],
        );
        fx.ok(
            "ann",
            &[ins(
                "person",
                "p2",
                json!({"email": "CANARY-OTHER@x", "stage": "lead"}),
            )],
        );
        fx.tick(1_000);
        fx.ok(
            "bob",
            &[merge(
                "person",
                "p1",
                json!({"email": "CANARY-B@x", "stage": "won"}),
            )],
        );
        fx.ok(
            "bob",
            &[merge("person", "p1", json!({"stray": "CANARY-S"}))],
        );
        assert!(everything(fx, "person", "p1").contains("CANARY-A"));
        // The erasure: the record is overwritten and the history scrubbed in one transaction.
        fx.tick(1_000);
        fx.ok(
            "dpo",
            &[
                merge(
                    "person",
                    "p1",
                    json!({"email": "erased@invalid", "phone": null}),
                ),
                erase("person", "p1"),
            ],
        );
        let all = everything(fx, "person", "p1");
        assert!(
            !all.contains("CANARY"),
            "no plaintext is left in any entry: {all}"
        );
        assert!(
            !all.contains("erased@invalid"),
            "the replacement went through the history too: {all}"
        );
        let e = fx.hist("person", "p1");
        // Entries stay; the fields that were kept show their values, the others the marker.
        assert_eq!(e[0].op, "create");
        assert_eq!(e[0].changed["email"], json!({"t": "erased"}));
        assert_eq!(e[0].changed["stage"], json!({"a": "lead"}));
        assert_eq!(e[1].changed["stage"], json!({"b": "lead", "a": "won"}));
        assert_eq!(
            e[2].changed["stray"],
            json!({"t": "redacted"}),
            "an undeclared key was never stored, and stays a mark"
        );
        // The last entry is the erasure: who, how many, which fields, no value.
        let last = e.last().unwrap();
        assert_eq!(last.op, "erase");
        assert_eq!(last.actor, "dpo");
        assert_eq!(
            last.changed["entries"], 3,
            "the entries that held something to erase"
        );
        assert_eq!(last.changed["fields"], json!(["email", "phone"]));
        assert!(!last.sealed);
        // The other record is untouched.
        assert!(everything(fx, "person", "p2").contains("CANARY-OTHER"));
        // A second erasure finds nothing to erase and says so; it is another entry.
        fx.ok("dpo", &[erase("person", "p1")]);
        let again = fx.hist("person", "p1").pop().unwrap();
        assert_eq!(
            (again.op.as_str(), again.changed["entries"].as_i64()),
            ("erase", Some(0))
        );
        assert_eq!(again.changed["fields"], json!([]));
        // An erasure that is rolled back erases nothing.
        fx.ok(
            "ann",
            &[merge("person", "p2", json!({"email": "CANARY-LATER@x"}))],
        );
        let before = fx.rows();
        let r = fx.by("dpo", &[erase("person", "p2"), del("person", "ghost")]);
        assert!(r.is_err());
        assert_eq!(fx.rows(), before);
        assert!(everything(fx, "person", "p2").contains("CANARY-OTHER"));
        // The old entry points work on it as on any batch.
        fx.b.apply(&[erase("person", "p2")]).unwrap();
        assert!(!everything(fx, "person", "p2").contains("CANARY"));
    });
}

#[test]
fn a_sealed_value_is_erased_like_any_other() {
    each(|fx| {
        fx.track(
            "patient",
            true,
            &[("diagnosis", Treat::Seal), ("ward", Treat::Plain)],
        );
        fx.b.set_sealer(Arc::new(TestSealer));
        fx.ok(
            "ann",
            &[ins(
                "patient",
                "p1",
                json!({"diagnosis": "flu", "ward": "A"}),
            )],
        );
        assert!(fx.hist("patient", "p1")[0].sealed);
        fx.ok("ann", &[erase("patient", "p1")]);
        let e = fx.hist("patient", "p1");
        assert_eq!(e[0].changed["diagnosis"], json!({"t": "erased"}));
        assert_eq!(
            e[0].changed["ward"],
            json!({"t": "erased"}),
            "nothing is kept unless the model keeps it"
        );
        assert!(!e[0].sealed, "nothing sealed is left in the entry");
    });
}

// ---- backup and restore ---------------------------------------------------------------------------

#[test]
fn the_history_rides_the_export_and_comes_back_with_its_numbers() {
    let src = Fx::memory();
    src.track_plain("deal", &["n", "stage"]);
    src.ok(
        "ann",
        &[
            ins("deal", "d1", json!({"n": 1})),
            ins("note", "n1", json!({"t": 1})),
        ],
    );
    src.tick(1_000);
    src.ok(
        "bob",
        &[
            merge("deal", "d1", json!({"n": 2, "stage": "x"})),
            ins("deal", "d2", json!({})),
        ],
    );
    src.ok("bob", &[del("deal", "d2")]);
    let rows = src.rows();
    let (records, entries): (Vec<_>, Vec<_>) =
        rows.iter().partition(|(m, _, _)| m != "record_change");
    assert_eq!(records.len(), 2, "d1 and the note");
    assert_eq!(
        entries.len(),
        4,
        "d1 created and changed, d2 created and deleted"
    );
    // The history comes after the records, and the id of a row is the number of the entry.
    let first_entry = rows
        .iter()
        .position(|(m, _, _)| m == "record_change")
        .unwrap();
    assert!(
        rows[first_entry..]
            .iter()
            .all(|(m, _, _)| m == "record_change")
    );
    let ids: Vec<i64> = entries
        .iter()
        .map(|(_, id, _)| id.parse().unwrap())
        .collect();
    assert!(ids.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(src.b.total().unwrap(), rows.len());
    // Into every kind of store, wiping what is there.
    for target in [Fx::memory(), Fx::sqlite(), Fx::sqlite_file()] {
        eprintln!("--- into {}", target.name);
        target.track_plain("deal", &["n", "stage"]);
        target.ok("ann", &[ins("junk", "j", json!({"x": 1}))]);
        let mut feed = Some(rows.clone());
        let n = target.b.import(true, &mut || Ok(feed.take())).unwrap();
        assert_eq!(n, rows.len());
        assert_eq!(
            target.rows(),
            rows,
            "exports the same rows in the same order"
        );
        for id in ["d1", "d2"] {
            assert_eq!(target.hist("deal", id), src.hist("deal", id), "{id}");
        }
        assert!(target.maybe("junk", "j").is_none(), "wiped");
        assert_eq!(target.b.total().unwrap(), rows.len());
        // The numbers go on from the last one.
        target.tick(1_000);
        target.ok("cy", &[merge("deal", "d1", json!({"n": 3}))]);
        let next = target.hist("deal", "d1").pop().unwrap();
        assert!(
            next.seq > *ids.last().unwrap(),
            "{} after {ids:?}",
            next.seq
        );
        assert_eq!(next.row_version, 3);
        // An import that meets a number it has refuses, and changes nothing.
        let before = target.rows();
        let mut dup = Some(rows.clone());
        assert!(target.b.import(false, &mut || Ok(dup.take())).is_err());
        assert_eq!(target.rows(), before);
    }
    // A file, opened again: the history is on disk.
    let file = Fx::sqlite_file();
    file.track_plain("deal", &["n"]);
    file.ok("ann", &[ins("deal", "d1", json!({"n": 1}))]);
    let path = file._dir.as_ref().unwrap().0.join("sys.db");
    let again = crate::open(&format!("sqlite://{}", path.display())).unwrap();
    assert_eq!(
        again
            .history(&HistoryQuery::of("deal", "d1", 5))
            .unwrap()
            .len(),
        1
    );
    // An entry that is not one is refused when it is read in, and nothing is changed.
    for (id, fields) in [
        ("not-a-number", json!({})),
        ("0", json!({})),
        ("7", json!({"at": "yesterday"})),
    ] {
        let fresh = Fx::memory();
        let mut feed = Some(vec![(
            "record_change".to_string(),
            id.to_string(),
            f(fields),
        )]);
        assert!(
            fresh.b.import(true, &mut || Ok(feed.take())).is_err(),
            "{id}"
        );
        assert_eq!(fresh.b.total().unwrap(), 0);
    }
}

// ---- a random run ---------------------------------------------------------------------------------

/// A small deterministic generator, so that a failure can be replayed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn pick<'a, T>(&mut self, v: &'a [T]) -> &'a T {
        &v[self.below(v.len() as u64) as usize]
    }
}

fn random_fields(r: &mut Rng) -> Fields {
    let values = [
        J::Null,
        json!(1),
        json!(2),
        json!("s"),
        json!([1]),
        json!({"k": 1}),
    ];
    let mut m = Fields::new();
    for k in ["x", "y", "z"] {
        if r.below(2) == 0 {
            m.insert(k.into(), r.pick(&values).clone());
        }
    }
    m
}

fn random_op(r: &mut Rng) -> Op {
    let model = (*r.pick(&["a", "b", "c"])).to_string();
    let id = format!("r{}", r.below(5));
    match r.below(8) {
        0 | 1 => Op::Insert {
            model,
            id,
            fields: random_fields(r),
        },
        2 | 3 => Op::Merge {
            model,
            id,
            patch: random_fields(r),
        },
        4 => Op::Put {
            model,
            id,
            fields: random_fields(r),
        },
        5 => Op::Delete { model, id },
        6 => Op::Add {
            model,
            id,
            field: "n".into(),
            delta: r.below(5) as i64 - 2,
            min: Some(0),
            max: Some(6),
            create: (r.below(2) == 0).then(Fields::new),
        },
        _ => Op::Patch {
            model,
            id,
            patch: random_fields(r),
            expect: vec![("x".into(), r.pick(&[J::Null, json!(1), json!(2)]).clone())],
        },
    }
}

/// What the entries of a record say, folded: the record they describe, or `None` once deleted.
/// Every entry is checked against the record as the entries before it left it.
fn fold(entries: &[ChangeRow]) -> Option<BTreeMap<String, J>> {
    let mut state: BTreeMap<String, J> = BTreeMap::new();
    let (mut alive, mut version) = (false, 0);
    for e in entries {
        match e.op.as_str() {
            "create" => {
                assert!(!alive, "created twice: {entries:?}");
                assert_eq!(e.row_version, 1, "{e:?}");
                state.clear();
                for (k, v) in &e.changed {
                    assert!(v.get("b").is_none(), "a creation has no before: {e:?}");
                    state.insert(k.clone(), v["a"].clone());
                }
                alive = true;
                version = 1;
            }
            "update" | "delete" => {
                assert!(alive, "{} of a record that is not there: {entries:?}", e.op);
                assert_eq!(
                    e.row_version,
                    version + 1,
                    "versions follow each other: {entries:?}"
                );
                for (k, v) in &e.changed {
                    assert_eq!(
                        v.get("b"),
                        state.get(k),
                        "`{k}` before, in {e:?} after {state:?}"
                    );
                }
                if e.op == "update" {
                    for (k, v) in &e.changed {
                        match v.get("a") {
                            Some(a) => state.insert(k.clone(), a.clone()),
                            None => state.remove(k),
                        };
                    }
                    version += 1;
                } else {
                    // Everything the record had is in the delete.
                    assert_eq!(
                        e.changed.keys().collect::<Vec<_>>(),
                        state.keys().collect::<Vec<_>>(),
                        "{e:?}"
                    );
                    state.clear();
                    alive = false;
                }
            }
            other => panic!("unexpected {other} in {entries:?}"),
        }
    }
    alive.then_some(state)
}

#[test]
fn the_history_always_adds_up_to_the_records() {
    each(|fx| {
        // `a` records history, `b` is stamped only, `c` is not tracked.
        fx.track_plain("a", &["x", "y", "z", "n"]);
        fx.track("b", false, &[]);
        let mut r = Rng(0x5eed);
        let (mut applied, mut refused) = (0, 0);
        for step in 0..300 {
            fx.tick(r.below(3) as i64 * 1_000);
            let ops: Vec<Op> = (0..1 + r.below(5)).map(|_| random_op(&mut r)).collect();
            let before = fx.rows();
            match fx.by("ann", &ops) {
                Ok(()) => applied += 1,
                Err(_) => {
                    refused += 1;
                    assert_eq!(
                        fx.rows(),
                        before,
                        "step {step}: a refused batch changed something: {ops:?}"
                    );
                }
            }
        }
        eprintln!("{applied} batches applied, {refused} refused");
        assert!(
            applied > 40 && refused > 40,
            "{applied} applied, {refused} refused: the run is not mixed enough"
        );
        let mut entries_seen = 0;
        for id in 0..5 {
            let id = format!("r{id}");
            // `a`: the entries fold to the record.
            let entries = fx.hist("a", &id);
            entries_seen += entries.len();
            let folded = fold(&entries);
            match (fx.maybe("a", &id), folded) {
                (Some(rec), Some(state)) => {
                    let live: BTreeMap<String, J> = sysfields::canon(&rec)
                        .into_iter()
                        .map(|(k, v)| (k.to_string(), v.clone()))
                        .collect();
                    assert_eq!(live, state, "{id}");
                    assert_eq!(
                        rec["row_version"].as_i64(),
                        entries.last().map(|e| e.row_version),
                        "{id}: the version of the record is that of its last entry"
                    );
                    assert_eq!(
                        rec["updated_at"],
                        entries.last().unwrap().at.as_str(),
                        "{id}"
                    );
                }
                (None, None) => {}
                (rec, state) => panic!("{id}: record {rec:?}, history says {state:?}: {entries:?}"),
            }
            // `b`: no history at all, but every record is stamped. `c`: no stamp at all.
            assert!(fx.hist("b", &id).is_empty());
            if let Some(rec) = fx.maybe("b", &id) {
                let (c, u, _, by, v, est) = stamp_of(&rec);
                assert!(parse_micros(&c).unwrap() <= parse_micros(&u).unwrap());
                assert!(v >= 1 && by == "ann" && !est, "{rec:?}");
            }
            if let Some(rec) = fx.maybe("c", &id) {
                assert!(
                    sysfields::SYSTEM_KEYS.iter().all(|k| !rec.contains_key(*k)),
                    "an untracked record got a system key: {rec:?}"
                );
            }
        }
        assert!(
            entries_seen > 40,
            "{entries_seen} entries: the run says little"
        );
        // Every entry of the store is a change of a record of `a`.
        let all: Vec<Row> = fx
            .rows()
            .into_iter()
            .filter(|(m, _, _)| m == "record_change")
            .collect();
        assert!(
            all.iter().all(|(_, _, f)| f["model"] == "a"),
            "only `a` keeps history"
        );
        assert_eq!(all.len(), entries_seen);
    });
}

// ---- a store that cannot keep history -----------------------------------------------------------

#[test]
fn a_store_without_a_history_refuses_a_model_that_wants_one_and_stamps_one_that_does_not() {
    // The four methods every transaction must have and none of the others: what the transaction
    // of a backend that has not learnt about history looks like to the planner.
    #[derive(Default)]
    struct Bare {
        rows: BTreeMap<(String, String), Fields>,
    }
    impl crate::atomic::TxView for Bare {
        fn tx_get(&mut self, model: &str, id: &str) -> Result<Option<Fields>> {
            Ok(self.rows.get(&(model.into(), id.into())).cloned())
        }
        fn tx_insert(&mut self, model: &str, id: &str, fields: &Fields) -> Result<()> {
            self.rows.insert((model.into(), id.into()), fields.clone());
            Ok(())
        }
        fn tx_put(&mut self, model: &str, id: &str, fields: &Fields) -> Result<()> {
            self.tx_insert(model, id, fields)
        }
        fn tx_delete(&mut self, model: &str, id: &str) -> Result<()> {
            self.rows.remove(&(model.into(), id.into()));
            Ok(())
        }
    }
    let tracking = Tracking::new();
    tracking.set(
        "log",
        Track {
            history: true,
            ..Track::default()
        },
    );
    tracking.set("quiet", Track::default());
    let ctx = WriteCtx::new("ann", "rest", "x");
    let mut tx = Bare::default();
    // Stamping needs nothing but the clock of the process.
    crate::atomic::run_ops(
        &mut tx,
        &[ins("quiet", "q", json!({"a": 1}))],
        &ctx,
        &tracking,
    )
    .unwrap();
    assert_eq!(tx.rows[&("quiet".into(), "q".into())]["row_version"], 1);
    // History cannot be written: the batch is refused instead of losing the entries.
    let r = crate::atomic::run_ops(
        &mut tx,
        &[ins("log", "l", json!({"a": 1}))],
        &ctx,
        &tracking,
    );
    assert_eq!(
        r,
        Err(ApplyError::Storage(
            "this storage backend keeps no change history".into()
        ))
    );
    let r = crate::atomic::run_ops(&mut tx, &[erase("log", "l")], &ctx, &tracking);
    assert!(matches!(r, Err(ApplyError::Storage(_))), "{r:?}");
}
