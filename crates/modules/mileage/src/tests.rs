//! The module end to end on the dispatcher, the one path of REST, MCP and the command line.
//! Every test runs on the memory and SQLite backends, and on PostgreSQL when
//! `OBOP_TEST_POSTGRES_URL` is set (a scratch database for each rig).

use super::*;
use obop_commands::errors;
use obop_commands::{CallError, Principal, Surface, Value, Vault};
use obop_storage::testing::{Flaky, TestDb};
use obop_values::Instant;
use serde_json::{Value as J, json};
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

const DRIVER: &str = "mileage.driver";
const REVIEWER: &str = "mileage.reviewer";
const ADMIN: &str = "mileage.admin";
const TRIP: &str = "mileage_trip";
const POINT: &str = "mileage_point";

fn driver(n: &str) -> Principal {
    Principal::user(n, &[DRIVER])
}
fn reviewer(n: &str) -> Principal {
    Principal::user(n, &[DRIVER, REVIEWER])
}
fn admin(n: &str) -> Principal {
    Principal::user(n, &[DRIVER, ADMIN])
}

/// 2026-10-15 09:00:00 UTC in microseconds: the time of the rig until a test moves it.
fn start() -> i64 {
    Instant::parse("2026-10-15T09:00:00Z").unwrap().micros()
}

fn stamp(micros: i64) -> String {
    Instant::from_micros(micros).unwrap().canonical()
}

struct Rig {
    d: Arc<Dispatcher>,
    store: Store,
    now: Arc<AtomicI64>,
    flaky: Option<Arc<Flaky>>,
    _db: Option<TestDb>,
}

impl Rig {
    fn over(store: Store, flaky: Option<Arc<Flaky>>, db: Option<TestDb>) -> Rig {
        let now = Arc::new(AtomicI64::new(start()));
        let d = Arc::new(Dispatcher::new());
        let (a, b) = (now.clone(), now.clone());
        d.set_clock(Arc::new(move || a.load(Ordering::SeqCst) / 1_000_000));
        let clock: Clock = Arc::new(move || b.load(Ordering::SeqCst));
        let m = module();
        let v = obop_registry::check_module(&m);
        assert!(v.is_empty(), "{v:?}");
        register_with_clock(&d, &store, clock);
        Rig {
            d,
            store,
            now,
            flaky,
            _db: db,
        }
    }

    fn open(url: &str, db: Option<TestDb>) -> Rig {
        Rig::over(Store::open(url).unwrap(), None, db)
    }

    /// A rig whose store refuses calls when a test says so (D74).
    fn flaky() -> Rig {
        let flaky = Flaky::new(obop_storage::open("memory:").unwrap());
        let store = Store::over(obop_storage::guarded(flaky.clone()));
        Rig::over(store, Some(flaky), None)
    }

    fn at(&self, micros: i64) {
        self.now.store(micros, Ordering::SeqCst);
    }

    fn raw(&self, who: &Principal, cmd: &str, input: J) -> Result<J, CallError> {
        self.d
            .call(
                Surface::Rest,
                who,
                &mut Vault::new(),
                cmd,
                &Value::from_json(&input),
            )
            .map(|r| r.value.to_json())
    }

    fn call(&self, who: &Principal, cmd: &str, input: J) -> Result<J, String> {
        self.raw(who, cmd, input).map_err(|e| match e {
            CallError::Handler(m) => m,
            other => format!("{other:?}"),
        })
    }

    fn ok(&self, who: &Principal, cmd: &str, input: J) -> J {
        match self.call(who, cmd, input) {
            Ok(v) => v,
            Err(e) => panic!("{cmd} by {} was refused: {e}", who.id),
        }
    }

    fn refused(&self, who: &Principal, cmd: &str, input: J) -> String {
        match self.call(who, cmd, input) {
            Ok(v) => panic!("{cmd} by {} should be refused, it answered {v}", who.id),
            Err(e) => e,
        }
    }

    /// The code and status a surface would answer a refusal with.
    fn classified(&self, who: &Principal, cmd: &str, input: J) -> (u16, String) {
        match self.raw(who, cmd, input) {
            Ok(v) => panic!("{cmd} should be refused, it answered {v}"),
            Err(e) => {
                let c = errors::classify_with(&e, &errors::Outcome::default());
                (c.status, c.code.to_string())
            }
        }
    }

    fn events(&self, kind: &str) -> Vec<(String, String, String)> {
        let mut v: Vec<_> = self
            .store
            .backend()
            .list(obop_commands::audit::EVENT_TABLE)
            .unwrap()
            .into_iter()
            .filter(|(_, f)| f.get("kind").and_then(J::as_str) == Some(kind))
            .map(|(_, f)| {
                let s = |k: &str| f.get(k).and_then(J::as_str).unwrap_or("").to_string();
                (s("user"), s("actor"), s("detail"))
            })
            .collect();
        v.sort();
        v
    }

    fn trip_row(&self, id: &str) -> obop_storage::Fields {
        self.store.backend().get(TRIP, id).unwrap().expect("a trip")
    }

    fn points_stored(&self, trip: &str) -> usize {
        self.store
            .backend()
            .find(POINT, "trip", &J::from(trip))
            .unwrap()
            .len()
    }

    // ---- building trips ----------------------------------------------------------------------

    /// A trip that is recording, made with the single `create`, which has no rate limit.
    fn recording(&self, who: &Principal) -> String {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let made = self.ok(
            who,
            "mileage_trip.create",
            json!({
                "client_trip_id": format!("0192f000-0000-7000-8000-{n:012}"),
                "started_at": stamp(self.now.load(Ordering::SeqCst)),
                "purpose": "business",
            }),
        );
        made["id"].as_str().unwrap().to_string()
    }

    /// Points `from..=to` of a straight walk north, a step of 0.0002 degrees (22.24 m) every five
    /// seconds, one by one.
    fn walk(&self, who: &Principal, trip: &str, from: i64, to: i64) {
        for seq in from..=to {
            self.ok(who, "mileage_point.create", walk_row(trip, seq));
        }
    }

    fn ended(&self, who: &Principal) -> String {
        let id = self.recording(who);
        self.walk(who, &id, 1, 10);
        self.ok(
            who,
            "mileage_trip.end",
            json!({"id": id, "expected_points": 10, "last_seq": 10}),
        );
        id
    }

    fn in_review(&self, who: &Principal) -> String {
        let id = self.ended(who);
        self.ok(who, "mileage_trip.submit", json!({"id": id}));
        id
    }

    fn state(&self, id: &str) -> String {
        self.trip_row(id)["state"].as_str().unwrap().to_string()
    }
}

fn walk_row(trip: &str, seq: i64) -> J {
    json!({
        "trip": trip,
        "seq": seq,
        "recorded_at": stamp(start() + seq * 5_000_000),
        "lat": format!("{:.7}", 52.0 + 0.0002 * seq as f64),
        "lon": "13.0000000",
        "accuracy_m": "5",
    })
}

fn each_rig(f: impl Fn(&Rig)) {
    f(&Rig::open("memory:", None));
    f(&Rig::open("sqlite::memory:", None));
    if let Some(db) = TestDb::new() {
        let url = db.url().to_string();
        f(&Rig::open(&url, Some(db)));
    }
}

/// The monthly totals of confirmed trips, as the back office asks for them: one row per month of
/// `confirmed_at` and driver, `(month, driver, metres)`.
fn totals_of(r: &Rig, who: &Principal) -> Vec<(String, String, String)> {
    totals_in(r, who, None)
}

fn totals_in(r: &Rig, who: &Principal, zone: Option<&str>) -> Vec<(String, String, String)> {
    let mut month = json!({"field": "confirmed_at", "bucket": "month"});
    if let Some(z) = zone {
        month["zone"] = J::from(z);
    }
    let answer = r.ok(
        who,
        "mileage_trip.search",
        json!({
            "filter": [{"field": "state", "op": "eq", "value": "confirmed"}],
            "aggregate": {
                "group_by": [month, "driver"],
                "measures": [{"op": "sum", "field": "distance_m"}],
            },
        }),
    );
    answer["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| {
            (
                g["key"]["confirmed_at"].as_str().unwrap().to_string(),
                g["key"]["driver"].as_str().unwrap().to_string(),
                g["measures"]["sum:distance_m"]
                    .as_str()
                    .unwrap()
                    .to_string(),
            )
        })
        .collect()
}

// ---- the walk, start to finish ----------------------------------------------------------------

#[test]
fn a_trip_goes_from_recording_to_confirmed_and_only_then_counts() {
    each_rig(|r| {
        let (ann, rev) = (driver("ann"), reviewer("rita"));
        // Start: one row through bulk_create with the key of the spec.
        let made = r.ok(
            &ann,
            "mileage_trip.bulk_create",
            json!({
                "idempotency_key": "trip:0192f000-0000-7000-8000-000000000001",
                "rows": [{
                    "client_trip_id": "0192f000-0000-7000-8000-000000000001",
                    "started_at": stamp(start()),
                    "purpose": "business",
                }],
            }),
        );
        let id = made["ids"][0].as_str().unwrap().to_string();
        assert_eq!(r.state(&id), "recording");
        assert_eq!(r.trip_row(&id)["driver"], "ann", "the driver is the caller");

        // Ten points in two batches of five.
        for (a, b) in [(1, 5), (6, 10)] {
            let rows: Vec<J> = (a..=b).map(|s| walk_row(&id, s)).collect();
            let sent = r.ok(
                &ann,
                "mileage_point.bulk_create",
                json!({"idempotency_key": format!("points:x:{a}-{b}"), "rows": rows}),
            );
            assert_eq!(sent["created"], 5);
        }
        assert_eq!(r.points_stored(&id), 10);

        // End: the server decides the distance (nine steps of 22.24 m).
        let ended = r.ok(
            &ann,
            "mileage_trip.end",
            json!({"id": id, "expected_points": 10, "last_seq": 10}),
        );
        assert_eq!(ended["state"], "ended");
        let row = r.trip_row(&id);
        assert_eq!(row["distance_m"], "200.15", "{row:?}");
        assert_eq!(row["integrity"], J::from(""), "{row:?}");
        assert_eq!(row["start_lat"], "52.0002000");
        assert_eq!(row["end_lat"], "52.0020000");
        assert_eq!(row["ended_at"], stamp(start() + 50_000_000));
        assert_eq!(row["expected_points"], 10);

        // Submit, then the reviewer decides. Nothing counts before.
        let totals = || totals_of(r, &rev);
        r.ok(&ann, "mileage_trip.submit", json!({"id": id}));
        assert_eq!(r.state(&id), "in_review");
        assert!(totals().is_empty(), "an unconfirmed trip does not count");
        r.at(start() + 3_600_000_000);
        let done = r.ok(&rev, "mileage_trip.confirm", json!({"id": id}));
        assert_eq!(done["state"], "confirmed");
        let row = r.trip_row(&id);
        assert_eq!(row["confirmed_at"], stamp(start() + 3_600_000_000));
        assert_eq!(row["reviewed_by"], "rita");
        assert_eq!(
            totals(),
            vec![(
                "2026-10".to_string(),
                "ann".to_string(),
                "200.15".to_string()
            )]
        );
    });
}

// ---- ingestion (spec 5.3) ---------------------------------------------------------------------

fn trip_call(client: &str) -> J {
    json!({
        "idempotency_key": format!("trip:{client}"),
        "rows": [{"client_trip_id": client, "started_at": stamp(start()), "purpose": "business"}],
    })
}

#[test]
fn creating_a_trip_twice_gives_the_first_answer_and_after_a_day_fails_closed() {
    each_rig(|r| {
        let ann = driver("ann");
        let first = r.ok(&ann, "mileage_trip.bulk_create", trip_call("c-1"));
        let again = r.ok(&ann, "mileage_trip.bulk_create", trip_call("c-1"));
        assert_eq!(
            (first["replayed"].clone(), again["replayed"].clone()),
            (json!(false), json!(true))
        );
        assert_eq!(first["ids"], again["ids"]);
        assert_eq!(r.store.count(TRIP), 1);

        // The same key for other rows is refused, not merged.
        let mut other = trip_call("c-1");
        other["rows"][0]["purpose"] = J::from("commute");
        let e = r.refused(&ann, "mileage_trip.bulk_create", other);
        assert!(e.contains("idempotency_key"), "{e}");

        // Another driver may use the same client id: unique per driver.
        r.ok(&driver("bob"), "mileage_trip.bulk_create", trip_call("c-1"));
        assert_eq!(r.store.count(TRIP), 2);

        // After the 24 hours the key is forgotten; the unique client_trip_id still refuses the
        // trip, and nothing is written.
        r.at(start() + 25 * 3_600 * 1_000_000);
        let e = r.refused(&ann, "mileage_trip.bulk_create", trip_call("c-1"));
        assert!(e.contains("client_trip_id"), "{e}");
        assert_eq!(r.store.count(TRIP), 2);
        // The app then finds its trip by client_trip_id.
        let found = r.ok(
            &ann,
            "mileage_trip.search",
            json!({"filter": [{"field": "client_trip_id", "op": "eq", "value": "c-1"}]}),
        );
        assert_eq!(found["total"], 1);
        assert_eq!(found["rows"][0]["id"], first["ids"][0]);
    });
}

#[test]
fn point_batches_replay_and_a_duplicate_or_out_of_order_seq_is_handled() {
    each_rig(|r| {
        let ann = driver("ann");
        let id = r.recording(&ann);
        let batch = |a: i64, b: i64| -> J {
            let rows: Vec<J> = (a..=b).map(|s| walk_row(&id, s)).collect();
            json!({"idempotency_key": format!("points:c:{a}-{b}"), "rows": rows})
        };
        let one = r.ok(&ann, "mileage_point.bulk_create", batch(1, 4));
        let again = r.ok(&ann, "mileage_point.bulk_create", batch(1, 4));
        assert_eq!(
            (one["replayed"].clone(), again["replayed"].clone()),
            (json!(false), json!(true))
        );
        assert_eq!(one["ids"], again["ids"]);
        assert_eq!(r.points_stored(&id), 4);

        // The next batch arrives before the one in front of it: stored, a gap in between.
        r.ok(&ann, "mileage_point.bulk_create", batch(9, 10));
        r.ok(&ann, "mileage_point.bulk_create", batch(5, 8));
        assert_eq!(r.points_stored(&id), 10);

        // After the key is forgotten a replay is refused by the unique seq and writes nothing.
        r.at(start() + 25 * 3_600 * 1_000_000);
        let e = r.refused(&ann, "mileage_point.bulk_create", batch(1, 4));
        assert!(e.contains("seq"), "{e}");
        assert_eq!(r.points_stored(&id), 10);

        // A seq twice in one batch is refused as a whole.
        let id2 = r.recording(&ann);
        let rows = vec![walk_row(&id2, 1), walk_row(&id2, 2), walk_row(&id2, 1)];
        r.refused(&ann, "mileage_point.bulk_create", json!({"rows": rows}));
        assert_eq!(r.points_stored(&id2), 0);
    });
}

#[test]
fn points_for_another_drivers_trip_or_a_trip_that_left_recording_are_refused() {
    each_rig(|r| {
        let (ann, bob, rev) = (driver("ann"), driver("bob"), reviewer("rita"));
        let mine = r.recording(&ann);
        let sent = |t: &str| json!({"rows": [walk_row(t, 1)]});
        // Bob cannot tell another driver's trip from one that does not exist.
        let theirs = r.refused(&bob, "mileage_point.bulk_create", sent(&mine));
        let none = r.refused(&bob, "mileage_point.bulk_create", sent("no-such-trip"));
        assert_eq!(
            theirs.replace(&mine, "X"),
            none.replace("no-such-trip", "X")
        );
        assert_eq!(r.points_stored(&mine), 0);
        // The same for the single create.
        r.refused(&bob, "mileage_point.create", walk_row(&mine, 1));
        // A reviewer is no one's driver.
        r.refused(&rev, "mileage_point.bulk_create", sent(&mine));
        // One batch, one trip.
        let other = r.recording(&ann);
        let e = r.refused(
            &ann,
            "mileage_point.bulk_create",
            json!({"rows": [walk_row(&mine, 1), walk_row(&other, 1)]}),
        );
        assert!(e.contains("one trip"), "{e}");
        // A point cannot name its driver, nor carry an id.
        let mut row = walk_row(&mine, 1);
        row["driver"] = J::from("bob");
        r.refused(&ann, "mileage_point.create", row);
        let mut row = walk_row(&mine, 1);
        row["id"] = J::from("my-own-id");
        r.refused(&ann, "mileage_point.create", row);
        // Positions are positions.
        let mut row = walk_row(&mine, 1);
        row["lat"] = J::from("91.0000000");
        assert!(r.refused(&ann, "mileage_point.create", row).contains("lat"));

        // After the end of the trip no point is accepted.
        let done = r.ended(&ann);
        let e = r.refused(&ann, "mileage_point.bulk_create", sent(&done));
        assert!(e.contains("recording"), "{e}");
        let e = r.refused(&ann, "mileage_point.create", walk_row(&done, 11));
        assert!(e.contains("recording"), "{e}");
        assert_eq!(r.points_stored(&done), 10);
    });
}

#[test]
fn a_full_batch_of_a_thousand_points_is_accepted() {
    each_rig(|r| {
        let ann = driver("ann");
        let id = r.recording(&ann);
        let rows: Vec<J> = (1..=1000).map(|s| walk_row(&id, s)).collect();
        let sent = r.ok(&ann, "mileage_point.bulk_create", json!({"rows": rows}));
        assert_eq!(sent["created"], 1000);
        assert_eq!(r.points_stored(&id), 1000);
    });
}

/// The time of a batch of 1,000 points on an instance that already holds other points: a
/// measurement, not a test. The unique check of the kernel looks every row up by an unindexed
/// scan of the stored points (docs/ISSUES.md M8), so the time grows with what is stored.
/// Run: `cargo test -p mileage --lib batch_time -- --ignored --nocapture` (a debug build; add
/// `--release` for the other figure).
#[test]
#[ignore = "a measurement; prints the time of a batch of 1,000 points by the number of stored points"]
fn batch_time_by_stored_points() {
    use obop_storage::Op;
    for url in ["memory:", "sqlite::memory:"] {
        for stored in [0_usize, 5_000, 20_000] {
            let r = Rig::open(url, None);
            let ann = driver("ann");
            let id = r.recording(&ann);
            let db = r.store.backend();
            for chunk in 0..stored.div_ceil(1000) {
                let ops: Vec<Op> = (0..1000.min(stored - chunk * 1000))
                    .map(|i| {
                        let mut f = obop_storage::Fields::new();
                        f.insert("trip".into(), J::from(format!("filler-{chunk}")));
                        f.insert("driver".into(), J::from("zoe"));
                        f.insert("seq".into(), J::from(i as i64 + 1));
                        Op::Insert {
                            model: POINT.into(),
                            id: format!("filler-{chunk}-{i}"),
                            fields: f,
                        }
                    })
                    .collect();
                db.apply_checked(&ops).unwrap();
            }
            let rows: Vec<J> = (1..=1000).map(|s| walk_row(&id, s)).collect();
            let t = std::time::Instant::now();
            r.ok(&ann, "mileage_point.bulk_create", json!({"rows": rows}));
            eprintln!(
                "BATCH_TIME {url} stored={stored} 1000 points: {:.2} s",
                t.elapsed().as_secs_f64()
            );
        }
    }
}

#[test]
fn bulk_create_is_limited_to_twelve_calls_a_minute_so_a_long_trip_needs_few_batches() {
    each_rig(|r| {
        // A three hour trip with a fix every five seconds is 2,160 points: with batches of up to
        // 1,000 rows that is three calls, far below the limit of twelve a minute.
        assert_eq!(3 * 3600 / 5, 2160);
        assert_eq!(2160_usize.div_ceil(1000), 3);
        let ann = driver("ann");
        for i in 0..12 {
            r.ok(
                &ann,
                "mileage_trip.bulk_create",
                trip_call(&format!("c-{i}")),
            );
        }
        let e = r.refused(&ann, "mileage_trip.bulk_create", trip_call("c-12"));
        assert!(e.contains("too many bulk imports"), "{e}");
        let (status, code) = r.classified(&ann, "mileage_trip.bulk_create", trip_call("c-12"));
        assert_eq!((status, code.as_str()), (429, "rate_limited"));
        // The limit is for the person: another driver is not held up.
        r.ok(&driver("bob"), "mileage_trip.bulk_create", trip_call("c-0"));
    });
}

// ---- end (spec 5.4) ---------------------------------------------------------------------------

#[test]
fn end_waits_for_the_points_with_a_code_the_app_can_retry_on() {
    each_rig(|r| {
        let ann = driver("ann");
        let id = r.recording(&ann);
        r.walk(&ann, &id, 1, 6);
        let end = json!({"id": id, "expected_points": 10, "last_seq": 10});
        // Six of ten points are here: a conflict the app repeats, with the code in the table.
        let (status, code) = r.classified(&ann, "mileage_trip.end", end.clone());
        assert_eq!((status, code.as_str()), (409, "incomplete"));
        assert_eq!(r.state(&id), "recording", "nothing was changed");
        let row = r.trip_row(&id);
        assert!(row.get("distance_m").is_none_or(J::is_null), "{row:?}");
        // The missing points arrive, and the same call goes through.
        r.walk(&ann, &id, 7, 10);
        r.ok(&ann, "mileage_trip.end", end.clone());
        assert_eq!(r.state(&id), "ended");
        // The right count with the wrong last seq cannot be mended by waiting: no point can
        // arrive without being more than the device said.
        let other = r.recording(&ann);
        r.walk(&ann, &other, 1, 3);
        let (status, code) = r.classified(
            &ann,
            "mileage_trip.end",
            json!({"id": other, "expected_points": 3, "last_seq": 4}),
        );
        assert_eq!((status, code.as_str()), (400, "bad_request"));
        // The counts written to the trip first (as the outbox does) count the same.
        r.ok(
            &ann,
            "mileage_trip.update",
            json!({"id": other, "expected_points": 3, "last_seq": 3}),
        );
        r.ok(&ann, "mileage_trip.end", json!({"id": other}));
        assert_eq!(r.state(&other), "ended");
    });
}

#[test]
fn a_refusal_for_good_is_not_the_retryable_code() {
    each_rig(|r| {
        let ann = driver("ann");
        // Without the counts there is nothing to wait for.
        let id = r.recording(&ann);
        r.walk(&ann, &id, 1, 3);
        let (status, code) = r.classified(&ann, "mileage_trip.end", json!({"id": id}));
        assert_eq!((status, code.as_str()), (400, "bad_request"));
        // A standing trip has no distance: discard it, ending it again changes nothing.
        let still = r.recording(&ann);
        for seq in 1..=4 {
            let mut row = walk_row(&still, seq);
            row["lat"] = J::from("52.0000000");
            r.ok(&ann, "mileage_point.create", row);
        }
        let end = json!({"id": still, "expected_points": 4, "last_seq": 4});
        let (status, code) = r.classified(&ann, "mileage_trip.end", end.clone());
        assert_eq!((status, code.as_str()), (400, "bad_request"));
        assert_eq!(r.state(&still), "recording");
        let e = r.refused(&ann, "mileage_trip.end", end);
        assert!(e.contains("no distance"), "{e}");
        // A trip with no point at all.
        let empty = r.recording(&ann);
        let e = r.refused(
            &ann,
            "mileage_trip.end",
            json!({"id": empty, "expected_points": 0, "last_seq": 0}),
        );
        assert!(e.contains("no distance"), "{e}");
        // A wrong state is a refusal for good too.
        let done = r.ended(&ann);
        let (status, code) = r.classified(&ann, "mileage_trip.end", json!({"id": done}));
        assert_eq!((status, code.as_str()), (400, "bad_request"));
    });
}

#[test]
fn a_jump_above_the_speed_cap_is_flagged_and_left_out_of_the_distance() {
    each_rig(|r| {
        let ann = driver("ann");
        let id = r.recording(&ann);
        // Ten fixes on a line; the sixth is 5 km off (1,000 m/s) and the walk goes on after it.
        for seq in 1..=10 {
            let mut row = walk_row(&id, seq);
            if seq == 6 {
                row["lat"] = J::from(format!("{:.7}", 52.0 + 0.0002 * 6.0 + 0.045));
            }
            r.ok(&ann, "mileage_point.create", row);
        }
        r.ok(
            &ann,
            "mileage_trip.end",
            json!({"id": id, "expected_points": 10, "last_seq": 10}),
        );
        let row = r.trip_row(&id);
        assert_eq!(row["integrity"], "speed_jump:1", "{row:?}");
        // The jump is left out, so the distance is that of the walk itself (the step from fix 5
        // to fix 7 is two steps): had the jump counted, it would be about 10 km.
        assert_eq!(row["distance_m"], "200.15", "{row:?}");
        assert_eq!(row["end_lat"], "52.0020000");
    });
}

#[test]
fn a_gap_in_the_seq_and_a_poor_fix_are_flagged_and_the_poor_fix_does_not_count() {
    each_rig(|r| {
        let ann = driver("ann");
        let id = r.recording(&ann);
        for seq in [1, 2, 3, 5, 6, 7] {
            let mut row = walk_row(&id, seq);
            if seq == 6 {
                row["accuracy_m"] = J::from("250");
            }
            r.ok(&ann, "mileage_point.create", row);
        }
        r.ok(
            &ann,
            "mileage_trip.end",
            json!({"id": id, "expected_points": 6, "last_seq": 7}),
        );
        let row = r.trip_row(&id);
        assert_eq!(row["integrity"], "seq_gap:1 low_accuracy:1", "{row:?}");
    });
}

// ---- the review (spec 5.5, 5.7) ---------------------------------------------------------------

#[test]
fn a_reviewer_confirms_or_rejects_and_the_driver_corrects_and_submits_again() {
    each_rig(|r| {
        let (ann, rev, other) = (driver("ann"), reviewer("rita"), reviewer("rex"));
        let id = r.in_review(&ann);
        // A reason is part of a rejection.
        let e = r.refused(&rev, "mileage_trip.reject", json!({"id": id}));
        assert!(e.contains("reject_reason"), "{e}");
        r.ok(
            &rev,
            "mileage_trip.update",
            json!({"id": id, "reject_reason": "Wrong purpose"}),
        );
        r.ok(&rev, "mileage_trip.reject", json!({"id": id}));
        assert_eq!(r.state(&id), "rejected");
        assert_eq!(r.trip_row(&id)["reject_reason"], "Wrong purpose");
        assert_eq!(r.trip_row(&id)["reviewed_by"], "rita");
        // It never counts while rejected; the driver corrects the purpose; a reviewer reopens it.
        assert!(totals_of(r, &rev).is_empty());
        r.refused(&ann, "mileage_trip.reopen", json!({"id": id}));
        r.ok(&other, "mileage_trip.reopen", json!({"id": id}));
        assert_eq!(r.state(&id), "ended");
        r.ok(
            &ann,
            "mileage_trip.update",
            json!({"id": id, "purpose": "commute"}),
        );
        r.ok(&ann, "mileage_trip.submit", json!({"id": id}));
        assert_eq!(
            r.trip_row(&id)["reject_reason"],
            J::Null,
            "an old reason does not stand"
        );
        r.at(start() + 86_400_000_000);
        r.ok(&other, "mileage_trip.confirm", json!({"id": id}));
        let row = r.trip_row(&id);
        assert_eq!(
            (row["state"].as_str(), row["purpose"].as_str()),
            (Some("confirmed"), Some("commute"))
        );
        assert_eq!(row["confirmed_at"], stamp(start() + 86_400_000_000));
        // A decision is final: nothing moves a confirmed trip.
        for a in [
            "end",
            "submit",
            "confirm",
            "reject",
            "reopen",
            "discard",
            "discard_stuck",
        ] {
            r.refused(
                &admin("ada"),
                &format!("mileage_trip.{a}"),
                json!({"id": id}),
            );
        }
        assert_eq!(r.state(&id), "confirmed");
    });
}

#[test]
fn nobody_decides_their_own_trip() {
    each_rig(|r| {
        let (rita, rex) = (reviewer("rita"), reviewer("rex"));
        let id = r.in_review(&rita);
        // The reviewer sees their own trip but decides none of it; another reviewer does.
        for a in ["confirm", "reject", "reopen"] {
            let e = r.refused(&rita, &format!("mileage_trip.{a}"), json!({"id": id}));
            assert!(
                e.contains("own trip") || e.contains("state must be"),
                "{a}: {e}"
            );
        }
        let e = r.refused(
            &rita,
            "mileage_trip.update",
            json!({"id": id, "reject_reason": "x"}),
        );
        assert!(e.contains("cannot be edited"), "{e}");
        assert_eq!(r.state(&id), "in_review");

        // An agent of another reviewer only proposes; the person who approves decides. Rita
        // approving a proposal on her own trip is refused, and the proposal does not run.
        let agent = Principal::agent("rex/agent", &[DRIVER, REVIEWER], &[DRIVER, REVIEWER]);
        let plan = |r: &Rig| match r.raw(&agent, "mileage_trip.confirm", json!({"id": id})) {
            Err(CallError::ApprovalRequired { plan_id }) => plan_id,
            other => panic!("expected a proposal, got {other:?}"),
        };
        let p1 = plan(r);
        let refused = r.d.approve(&rita, &mut Vault::new(), &p1);
        assert!(
            matches!(refused, Err(CallError::Handler(ref m)) if m.contains("own trip")),
            "{refused:?}"
        );
        assert_eq!(r.state(&id), "in_review");
        let p2 = plan(r);
        r.d.approve(&rex, &mut Vault::new(), &p2).unwrap();
        let row = r.trip_row(&id);
        assert_eq!(row["state"], "confirmed");
        assert_eq!(row["reviewed_by"], "rex", "the person who approved decided");

        // The agent of the driver cannot propose a decision on their own trip either.
        let other = r.in_review(&rita);
        let ritas_agent = Principal::agent("rita/agent", &[DRIVER, REVIEWER], &[DRIVER, REVIEWER]);
        let e = r.refused(&ritas_agent, "mileage_trip.confirm", json!({"id": other}));
        assert!(e.contains("own trip"), "{e}");
    });
}

#[test]
fn every_action_runs_only_from_its_states_by_its_role() {
    // (action, the states it runs from, who runs it)
    let table: [(&str, &[&str], &str); 7] = [
        ("end", &["recording"], "driver"),
        ("submit", &["ended"], "driver"),
        ("confirm", &["in_review"], "reviewer"),
        ("reject", &["in_review"], "reviewer"),
        ("reopen", &["rejected"], "reviewer"),
        ("discard", &["recording", "ended", "rejected"], "driver"),
        ("discard_stuck", &["recording", "ended"], "admin"),
    ];
    let states = [
        "recording",
        "ended",
        "in_review",
        "confirmed",
        "rejected",
        "discarded",
    ];
    each_rig(|r| {
        let (rev, adm) = (reviewer("rita"), admin("ada"));
        let mut n = 0;
        for state in states {
            for (action, from, role) in table {
                n += 1;
                let ann = driver(&format!("ann{n}"));
                let id = trip_in(r, &ann, &rev, state);
                let who = match role {
                    "driver" => &ann,
                    "reviewer" => &rev,
                    _ => &adm,
                };
                // The reasons come first; they are not what this table tests.
                if action == "discard" && matches!(state, "recording" | "ended" | "rejected") {
                    r.ok(
                        &ann,
                        "mileage_trip.update",
                        json!({"id": id, "discard_reason": "no longer needed"}),
                    );
                }
                if action == "reject" && state == "in_review" {
                    r.ok(
                        &rev,
                        "mileage_trip.update",
                        json!({"id": id, "reject_reason": "x"}),
                    );
                }
                let given = if action == "end" {
                    json!({"id": id, "expected_points": 10, "last_seq": 10})
                } else {
                    json!({"id": id})
                };
                let res = r.call(who, &format!("mileage_trip.{action}"), given);
                assert_eq!(
                    res.is_ok(),
                    from.contains(&state),
                    "{action} from {state}: {res:?}"
                );
                if res.is_err() {
                    assert_eq!(r.state(&id), state, "a refused action changes nothing");
                }
            }
        }
    });
}

/// A trip of `ann` in `state`, reached the way a trip reaches it.
fn trip_in(r: &Rig, ann: &Principal, rev: &Principal, state: &str) -> String {
    match state {
        "recording" => {
            let id = r.recording(ann);
            r.walk(ann, &id, 1, 10);
            id
        }
        "ended" => r.ended(ann),
        "in_review" => r.in_review(ann),
        "confirmed" => {
            let id = r.in_review(ann);
            r.ok(rev, "mileage_trip.confirm", json!({"id": id}));
            id
        }
        "rejected" => {
            let id = r.in_review(ann);
            r.ok(
                rev,
                "mileage_trip.update",
                json!({"id": id, "reject_reason": "x"}),
            );
            r.ok(rev, "mileage_trip.reject", json!({"id": id}));
            id
        }
        "discarded" => {
            let id = r.ended(ann);
            r.ok(
                ann,
                "mileage_trip.update",
                json!({"id": id, "discard_reason": "x"}),
            );
            r.ok(ann, "mileage_trip.discard", json!({"id": id}));
            id
        }
        other => panic!("{other}"),
    }
}

#[test]
fn the_wrong_role_is_refused_whatever_the_state() {
    each_rig(|r| {
        let (ann, bob, rev, adm) = (driver("ann"), driver("bob"), reviewer("rita"), admin("ada"));
        let ended = r.ended(&ann);
        let review = r.in_review(&ann);
        // Another driver cannot act on the trip of ann, and cannot tell it from a missing one.
        for a in ["end", "submit", "discard"] {
            let theirs = r.refused(&bob, &format!("mileage_trip.{a}"), json!({"id": ended}));
            assert!(theirs.starts_with("no mileage_trip"), "{a}: {theirs}");
        }
        // A driver is not a reviewer; an administrator is not one by being an administrator.
        for a in ["confirm", "reject", "reopen"] {
            assert!(
                r.call(&ann, &format!("mileage_trip.{a}"), json!({"id": review}))
                    .is_err(),
                "{a}"
            );
            assert!(
                r.call(&adm, &format!("mileage_trip.{a}"), json!({"id": review}))
                    .is_err(),
                "{a} by an administrator"
            );
        }
        // Only an administrator discards a stuck trip; a reviewer and the driver do not.
        for who in [&ann, &rev] {
            assert!(
                r.call(who, "mileage_trip.discard_stuck", json!({"id": ended}))
                    .is_err()
            );
        }
        assert_eq!(r.state(&ended), "ended");
        assert_eq!(r.state(&review), "in_review");
    });
}

#[test]
fn a_stuck_trip_is_discarded_by_an_administrator_and_the_log_says_who() {
    each_rig(|r| {
        let (ann, adm) = (driver("ann"), admin("ada"));
        let id = r.recording(&ann);
        r.walk(&ann, &id, 1, 3);
        // The agent of an administrator only proposes; the discard runs on approval.
        let agent = Principal::agent("ada/agent", &[DRIVER, ADMIN], &[DRIVER, ADMIN]);
        let plan = match r.raw(&agent, "mileage_trip.discard_stuck", json!({"id": id})) {
            Err(CallError::ApprovalRequired { plan_id }) => plan_id,
            other => panic!("{other:?}"),
        };
        assert_eq!(r.state(&id), "recording");
        r.d.approve(&adm, &mut Vault::new(), &plan).unwrap();
        assert_eq!(r.state(&id), "discarded");
        let log = r.events("mileage_discard_stuck");
        assert_eq!(
            log.iter()
                .map(|e| (e.0.as_str(), e.1.as_str()))
                .collect::<Vec<_>>(),
            vec![("ann", "ada")]
        );
        // Not from review on.
        let review = r.in_review(&ann);
        assert!(
            r.call(&adm, "mileage_trip.discard_stuck", json!({"id": review}))
                .is_err()
        );
    });
}

// ---- who sees what (spec 5.6, 5.7) ------------------------------------------------------------

#[test]
fn a_driver_sees_only_their_trips_and_a_reviewer_only_those_in_review_or_decided() {
    each_rig(|r| {
        let (ann, bob, rev, adm) = (driver("ann"), driver("bob"), reviewer("rita"), admin("ada"));
        let recording = r.recording(&ann);
        r.walk(&ann, &recording, 1, 2);
        let review = r.in_review(&ann);
        let bobs = r.in_review(&bob);
        let ids = |who: &Principal, cmd: &str| -> Vec<String> {
            let answer = r.ok(who, cmd, json!({"limit": 100}));
            let rows = match &answer {
                J::Array(rows) => rows.clone(),
                other => other["rows"].as_array().unwrap().clone(),
            };
            let mut v: Vec<String> = rows
                .iter()
                .map(|t| t["id"].as_str().unwrap().to_string())
                .collect();
            v.sort();
            v
        };
        let mut ann_sees = vec![recording.clone(), review.clone()];
        ann_sees.sort();
        let mut reviewer_sees = vec![review.clone(), bobs.clone()];
        reviewer_sees.sort();
        for cmd in ["mileage_trip.list", "mileage_trip.search"] {
            assert_eq!(
                ids(&ann, cmd),
                ann_sees,
                "{cmd}: a driver sees their own, not those of others"
            );
            assert_eq!(
                ids(&rev, cmd),
                reviewer_sees,
                "{cmd}: a reviewer sees none that is recording"
            );
            assert_eq!(ids(&adm, cmd).len(), 3, "{cmd}: an administrator sees all");
        }
        // The same for get, and a hidden trip answers like a missing one.
        assert!(
            r.call(&rev, "mileage_trip.get", json!({"id": review}))
                .is_ok()
        );
        let hidden = r.refused(&rev, "mileage_trip.get", json!({"id": recording}));
        let missing = r.refused(&rev, "mileage_trip.get", json!({"id": "nope"}));
        assert_eq!(
            hidden.replace(&recording, "X"),
            missing.replace("nope", "X")
        );
        r.refused(&bob, "mileage_trip.get", json!({"id": review}));
        r.refused(&ann, "mileage_trip.get", json!({"id": bobs}));
        assert!(
            r.call(&adm, "mileage_trip.get", json!({"id": recording}))
                .is_ok()
        );

        // The points: a reviewer reads those of a trip in review, never of a recording one.
        let n_points = |who: &Principal| -> usize {
            r.ok(who, "mileage_point.list", json!({"limit": 100}))
                .as_array()
                .unwrap()
                .len()
        };
        assert_eq!(n_points(&ann), 12, "ann sees all her points");
        assert_eq!(n_points(&bob), 10, "bob only his own");
        assert_eq!(
            n_points(&rev),
            20,
            "the two trips in review, not the recording one"
        );
        assert_eq!(n_points(&adm), 22);
        // An aggregate cannot be used to see more either.
        let counted = |who: &Principal| -> J {
            let q = json!({"aggregate": {"group_by": ["state"], "measures": [{"op": "count"}]}});
            r.ok(who, "mileage_trip.search", q)["total"].clone()
        };
        assert_eq!(counted(&bob), 1);
        assert_eq!(counted(&rev), 2);
    });
}

#[test]
fn reading_a_route_is_logged_for_the_reader_but_not_for_the_driver() {
    each_rig(|r| {
        let (ann, rev) = (driver("ann"), reviewer("rita"));
        let id = r.in_review(&ann);
        r.ok(&ann, "mileage_point.list", json!({"limit": 100}));
        assert!(
            r.events("mileage_route_read").is_empty(),
            "the driver reads their own route"
        );
        r.ok(&rev, "mileage_point.list", json!({"limit": 100}));
        let list = r.events("mileage_route_read");
        assert_eq!(list.len(), 1);
        assert_eq!((list[0].0.as_str(), list[0].1.as_str()), ("ann", "rita"));
        assert!(
            list[0].2.contains("10 point(s)") && list[0].2.contains(&id),
            "{}",
            list[0].2
        );
        let first = r.ok(&rev, "mileage_point.search", json!({"limit": 1}));
        let point = first["rows"][0]["id"].as_str().unwrap().to_string();
        r.ok(&rev, "mileage_point.get", json!({"id": point}));
        assert_eq!(r.events("mileage_route_read").len(), 3);
        // Counting is a read too, whatever it returns: the answer says that points exist.
        let q = json!({"aggregate": {"group_by": ["trip"], "measures": [{"op": "count"}]}});
        r.ok(&rev, "mileage_point.search", q);
        assert_eq!(r.events("mileage_route_read").len(), 4);
    });
}

// ---- the generated commands cannot get round a rule (spec 5.8) --------------------------------

#[test]
fn a_driver_edits_a_trip_only_while_it_is_theirs_to_edit() {
    each_rig(|r| {
        let (ann, rev) = (driver("ann"), reviewer("rita"));
        let id = r.recording(&ann);
        r.ok(
            &ann,
            "mileage_trip.update",
            json!({"id": id, "purpose": "commute", "vehicle": "car 1"}),
        );
        let ended = r.ended(&ann);
        r.ok(
            &ann,
            "mileage_trip.update",
            json!({"id": ended, "purpose": "private"}),
        );
        // After the submit the trip is the reviewer's to read; the driver cannot change it.
        r.ok(&ann, "mileage_trip.submit", json!({"id": ended}));
        let e = r.refused(
            &ann,
            "mileage_trip.update",
            json!({"id": ended, "purpose": "business"}),
        );
        assert!(e.contains("cannot be edited"), "{e}");
        assert_eq!(r.trip_row(&ended)["purpose"], "private");
        // The reviewer writes the reason, and nothing else of the trip.
        r.ok(
            &rev,
            "mileage_trip.update",
            json!({"id": ended, "reject_reason": "Check the vehicle"}),
        );
        let e = r.refused(
            &rev,
            "mileage_trip.update",
            json!({"id": ended, "purpose": "business"}),
        );
        assert!(e.contains("cannot be changed"), "{e}");
        // After a decision nobody writes the reason either.
        r.ok(&rev, "mileage_trip.confirm", json!({"id": ended}));
        let e = r.refused(
            &rev,
            "mileage_trip.update",
            json!({"id": ended, "reject_reason": "late"}),
        );
        assert!(e.contains("reject_reason"), "{e}");
        r.refused(
            &ann,
            "mileage_trip.update",
            json!({"id": ended, "purpose": "business"}),
        );
        // An administrator is neither the driver nor a reviewer: no write.
        r.refused(
            &admin("ada"),
            "mileage_trip.update",
            json!({"id": id, "purpose": "business"}),
        );
    });
}

#[test]
fn the_state_and_what_the_server_sets_cannot_be_written_by_update() {
    each_rig(|r| {
        let (ann, rev) = (driver("ann"), reviewer("rita"));
        let id = r.ended(&ann);
        let review = r.in_review(&ann);
        // update must not run an action on the quiet (D48 implicit runs), neither for the
        // driver nor for the reviewer, and the record stays as it was.
        r.refused(
            &ann,
            "mileage_trip.update",
            json!({"id": id, "state": "in_review"}),
        );
        r.refused(
            &ann,
            "mileage_trip.update",
            json!({"id": id, "state": "confirmed"}),
        );
        r.refused(
            &rev,
            "mileage_trip.update",
            json!({"id": review, "state": "confirmed"}),
        );
        r.refused(
            &rev,
            "mileage_trip.update",
            json!({"id": review, "state": "confirmed", "reject_reason": "x"}),
        );
        assert_eq!(
            (r.state(&id).as_str(), r.state(&review).as_str()),
            ("ended", "in_review")
        );
        assert!(
            r.trip_row(&review)
                .get("confirmed_at")
                .is_none_or(J::is_null)
        );
        // The fields the server sets.
        for (field, value) in [
            ("distance_m", json!("5000.00")),
            ("integrity", json!("seq_gap:1")),
            ("start_lat", json!("1.0000000")),
            ("end_lon", json!("1.0000000")),
            ("confirmed_at", json!(stamp(start()))),
            ("reviewed_by", json!("rita")),
            ("driver", json!("bob")),
            ("client_trip_id", json!("another")),
        ] {
            let e = r.refused(&ann, "mileage_trip.update", json!({"id": id, field: value}));
            assert!(e.contains(field) || e.contains("server"), "{field}: {e}");
        }
        assert_eq!(r.trip_row(&id)["distance_m"], "200.15");
        // An update that changes nothing is no write and no refusal.
        r.ok(
            &ann,
            "mileage_trip.update",
            json!({"id": id, "purpose": "business"}),
        );
    });
}

#[test]
fn a_trip_is_created_recording_and_cannot_arrive_decided_or_computed() {
    each_rig(|r| {
        let (ann, bob) = (driver("ann"), driver("bob"));
        let base = |n: &str| json!({"client_trip_id": n, "started_at": stamp(start())});
        let with = |n: &str, k: &str, v: J| -> J {
            let mut row = base(n);
            row[k] = v;
            row
        };
        for (k, v) in [
            ("state", json!("confirmed")),
            ("state", json!("in_review")),
            ("distance_m", json!("99999.00")),
            ("confirmed_at", json!(stamp(start()))),
            ("reviewed_by", json!("rita")),
            ("integrity", json!("")),
            ("start_lat", json!("1.0000000")),
            ("driver", json!("bob")),
            ("id", json!("my-own-id")),
            ("point_count", json!(5)),
        ] {
            let n = format!("n-{k}");
            if k != "id" {
                // create ignores an id and mints its own (D70 refuses one in bulk_create only).
                r.refused(&ann, "mileage_trip.create", with(&n, k, v.clone()));
            }
            let rows = json!({"rows": [with(&n, k, v)]});
            r.refused(&ann, "mileage_trip.bulk_create", rows);
        }
        let csv = "client_trip_id,started_at,state\nc,2026-10-15T09:00:00Z,confirmed\n";
        r.refused(&ann, "mileage_trip.bulk_create", json!({"csv": csv}));
        assert_eq!(r.store.count(TRIP), 0, "nothing was written by any of them");
        // The states a trip can start in, and the driver it takes.
        let made = r.ok(
            &ann,
            "mileage_trip.create",
            with("ok-1", "state", json!("recording")),
        );
        assert_eq!(r.state(made["id"].as_str().unwrap()), "recording");
        let made = r.ok(
            &bob,
            "mileage_trip.create",
            with("ok-1", "driver", json!("bob")),
        );
        assert_eq!(r.trip_row(made["id"].as_str().unwrap())["driver"], "bob");
        // A reviewer who is no driver of anything still records under their own name.
        let made = r.ok(&reviewer("rita"), "mileage_trip.create", base("ok-2"));
        assert_eq!(r.trip_row(made["id"].as_str().unwrap())["driver"], "rita");
    });
}

#[test]
fn a_trip_and_a_point_are_never_deleted_and_a_point_is_never_changed() {
    each_rig(|r| {
        let (ann, rev, adm) = (driver("ann"), reviewer("rita"), admin("ada"));
        let id = r.in_review(&ann);
        let point = r
            .store
            .backend()
            .find(POINT, "trip", &J::from(id.clone()))
            .unwrap()[0]
            .0
            .clone();
        for who in [&ann, &rev, &adm] {
            r.refused(who, "mileage_trip.delete", json!({"id": id}));
            r.refused(who, "mileage_point.delete", json!({"id": point}));
            r.refused(
                who,
                "mileage_point.update",
                json!({"id": point, "lat": "1.0000000"}),
            );
        }
        assert_eq!(r.store.count(TRIP), 1);
        assert_eq!(r.points_stored(&id), 10);
        assert_eq!(
            r.store.backend().get(POINT, &point).unwrap().unwrap()["lat"],
            "52.0002000"
        );
    });
}

#[test]
fn an_agent_only_proposes_what_is_destructive_and_runs_what_a_driver_may() {
    each_rig(|r| {
        let ann = driver("ann");
        let agent = Principal::agent("ann/agent", &[DRIVER], &[DRIVER]);
        let id = r.recording(&ann);
        r.walk(&ann, &id, 1, 4);
        // Reading its own trips, writing its fields: as the driver.
        assert_eq!(
            r.ok(&agent, "mileage_trip.list", json!({}))[0]["id"],
            id.as_str()
        );
        r.ok(
            &agent,
            "mileage_trip.update",
            json!({"id": id, "purpose": "commute"}),
        );
        // The agent writes the reason; the discard itself waits for the person (D18, D42).
        r.ok(
            &agent,
            "mileage_trip.update",
            json!({"id": id, "discard_reason": "test drive"}),
        );
        let plan = match r.raw(&agent, "mileage_trip.discard", json!({"id": id})) {
            Err(CallError::ApprovalRequired { plan_id }) => plan_id,
            other => panic!("{other:?}"),
        };
        assert_eq!(r.state(&id), "recording");
        r.d.approve(&ann, &mut Vault::new(), &plan).unwrap();
        assert_eq!(r.state(&id), "discarded");
    });
}

// ---- totals (spec 5.5) ------------------------------------------------------------------------

#[test]
fn only_confirmed_trips_count_by_the_month_they_were_confirmed_in() {
    each_rig(|r| {
        let (ann, bob, rev) = (driver("ann"), driver("bob"), reviewer("rita"));
        let confirm_at = |who: &Principal, when: &str| {
            let id = r.in_review(who);
            r.at(Instant::parse(when).unwrap().micros());
            r.ok(&rev, "mileage_trip.confirm", json!({"id": id}));
        };
        // Two of ann's trips confirmed either side of midnight UTC at the end of September, one
        // of bob's in October.
        confirm_at(&ann, "2026-09-30T23:30:00Z");
        confirm_at(&ann, "2026-10-01T00:30:00Z");
        confirm_at(&bob, "2026-10-20T08:00:00Z");
        // Trips that do not count, one in each other state.
        r.at(start());
        r.in_review(&ann);
        r.ended(&ann);
        let rec = r.recording(&bob);
        r.walk(&bob, &rec, 1, 10);
        let rejected = trip_in(r, &bob, &rev, "rejected");
        let discarded = trip_in(r, &ann, &rev, "discarded");
        assert_eq!(
            (r.state(&rejected).as_str(), r.state(&discarded).as_str()),
            ("rejected", "discarded")
        );

        let m = |a: &str, b: &str, c: &str| (a.to_string(), b.to_string(), c.to_string());
        assert_eq!(
            totals_of(r, &rev),
            vec![
                m("2026-09", "ann", "200.15"),
                m("2026-10", "ann", "200.15"),
                m("2026-10", "bob", "200.15")
            ]
        );
        // A month is the month of the platform zone: half past midnight UTC on the first is
        // already the first in Helsinki, and half past eleven the evening before is too.
        assert_eq!(
            totals_in(r, &rev, Some("Europe/Helsinki")),
            vec![m("2026-10", "ann", "400.30"), m("2026-10", "bob", "200.15")]
        );
        // A driver totals their own trips and no one else's.
        assert_eq!(totals_of(r, &bob), vec![m("2026-10", "bob", "200.15")]);
        assert_eq!(totals_of(r, &ann).len(), 2);
    });
}

// ---- D74: a store that refuses a call somewhere in the middle ---------------------------------

/// What a call that met a refusal of the store may leave behind, at every position of the call in
/// turn: the answer is a 503 of the store (never a verdict about the trip), what the call changes
/// it changes together (checked by `consistent`), and repeating the call finishes it.
fn fault_walk(
    setup: impl Fn(&Rig) -> J,
    run: impl Fn(&Rig, &J) -> Result<J, CallError>,
    consistent: impl Fn(&Rig, &J),
    finished: impl Fn(&Rig, &J),
) {
    let mut refused_somewhere = 0;
    for skip in 0..300 {
        let r = Rig::flaky();
        let flaky = r.flaky.clone().unwrap();
        let ctx = setup(&r);
        flaky.refuse_call_after(skip);
        let (answer, outcome) = request(|| run(&r, &ctx));
        let met = flaky.refused();
        flaky.allow_models();
        let at = format!("refused from call {skip}: {answer:?}");
        consistent(&r, &ctx);
        if let Err(e) = &answer {
            assert!(met > 0, "an answer of the call, not of the store: {at}");
            let c = errors::classify_with(e, &outcome);
            assert_eq!(c.status, 503, "{at}");
            refused_somewhere += 1;
            // Repeating it goes through, or finds it done.
            let (again, _) = request(|| run(&r, &ctx));
            if let Err(e) = &again {
                let c = errors::classify_with(e, &errors::Outcome::default());
                assert!(
                    c.status == 400 || c.status == 409,
                    "the repeat: {again:?} ({at})"
                );
            }
        }
        finished(&r, &ctx);
        if met == 0 {
            assert!(answer.is_ok(), "{at}");
            break;
        }
    }
    assert!(refused_somewhere > 0, "the walk never met a refusal");
}

fn request<T>(f: impl FnOnce() -> T) -> (T, errors::Outcome) {
    obop_storage::fault::begin_request(Default::default());
    let v = f();
    (v, obop_storage::fault::end_request())
}

#[test]
fn a_trip_made_through_a_refused_store_is_made_once() {
    fault_walk(
        |_| J::Null,
        |r, _| r.raw(&driver("ann"), "mileage_trip.bulk_create", trip_call("c-1")),
        |r, _| assert!(r.store.count(TRIP) <= 1),
        |r, _| assert_eq!(r.store.count(TRIP), 1, "once, however many tries"),
    );
}

#[test]
fn a_batch_of_points_through_a_refused_store_is_stored_whole_or_not_at_all() {
    fault_walk(
        |r| J::from(r.recording(&driver("ann"))),
        |r, id| {
            let id = id.as_str().unwrap();
            let rows: Vec<J> = (1..=5).map(|s| walk_row(id, s)).collect();
            r.raw(
                &driver("ann"),
                "mileage_point.bulk_create",
                json!({"idempotency_key": "points:c:1-5", "rows": rows}),
            )
        },
        |r, id| {
            let n = r.points_stored(id.as_str().unwrap());
            assert!(n == 0 || n == 5, "{n} of 5 points");
        },
        |r, id| assert_eq!(r.points_stored(id.as_str().unwrap()), 5),
    );
}

#[test]
fn end_through_a_refused_store_changes_the_state_and_the_distance_together() {
    fault_walk(
        |r| {
            let ann = driver("ann");
            let id = r.recording(&ann);
            r.walk(&ann, &id, 1, 10);
            J::from(id)
        },
        |r, id| {
            let end = json!({"id": id, "expected_points": 10, "last_seq": 10});
            r.raw(&driver("ann"), "mileage_trip.end", end)
        },
        |r, id| {
            let row = r.trip_row(id.as_str().unwrap());
            match row["state"].as_str().unwrap() {
                "recording" => assert!(row.get("distance_m").is_none_or(J::is_null), "{row:?}"),
                "ended" => assert!(
                    row["distance_m"] == "200.15" && row["end_lat"] == "52.0020000",
                    "{row:?}"
                ),
                other => panic!("{other}"),
            }
        },
        |r, id| {
            let row = r.trip_row(id.as_str().unwrap());
            if row["state"] == "recording" {
                // The repeat of the walk did not run it (a refusal of the repeat): do it now.
                r.ok(
                    &driver("ann"),
                    "mileage_trip.end",
                    json!({"id": id, "expected_points": 10, "last_seq": 10}),
                );
            }
            assert_eq!(r.trip_row(id.as_str().unwrap())["distance_m"], "200.15");
        },
    );
}

#[test]
fn confirm_through_a_refused_store_stamps_the_time_and_the_reviewer_with_the_state() {
    fault_walk(
        |r| J::from(r.in_review(&driver("ann"))),
        |r, id| r.raw(&reviewer("rita"), "mileage_trip.confirm", json!({"id": id})),
        |r, id| {
            let row = r.trip_row(id.as_str().unwrap());
            match row["state"].as_str().unwrap() {
                "in_review" => assert!(row.get("confirmed_at").is_none_or(J::is_null), "{row:?}"),
                "confirmed" => {
                    assert!(
                        row["confirmed_at"] == stamp(start()) && row["reviewed_by"] == "rita",
                        "{row:?}"
                    );
                }
                other => panic!("{other}"),
            }
        },
        |r, id| {
            if r.state(id.as_str().unwrap()) == "in_review" {
                r.ok(&reviewer("rita"), "mileage_trip.confirm", json!({"id": id}));
            }
            assert_eq!(r.state(id.as_str().unwrap()), "confirmed");
        },
    );
}

#[test]
fn discard_stuck_through_a_refused_store_is_never_done_without_its_log_entry() {
    fault_walk(
        |r| J::from(r.recording(&driver("ann"))),
        |r, id| {
            r.raw(
                &admin("ada"),
                "mileage_trip.discard_stuck",
                json!({"id": id}),
            )
        },
        |r, id| {
            // The discard and its log entry are one write: both, or neither.
            let logged = r.events("mileage_discard_stuck").len();
            let discarded = r.state(id.as_str().unwrap()) == "discarded";
            assert_eq!(
                logged,
                usize::from(discarded),
                "{logged} entries, discarded: {discarded}"
            );
        },
        |r, id| {
            if r.state(id.as_str().unwrap()) != "discarded" {
                r.ok(
                    &admin("ada"),
                    "mileage_trip.discard_stuck",
                    json!({"id": id}),
                );
            }
            assert_eq!(r.state(id.as_str().unwrap()), "discarded");
            assert_eq!(r.events("mileage_discard_stuck").len(), 1);
        },
    );
}

#[test]
fn a_route_is_not_given_out_when_its_log_entry_cannot_be_written() {
    use obop_storage::testing::Mode;
    let r = Rig::flaky();
    let flaky = r.flaky.clone().unwrap();
    let (ann, rev) = (driver("ann"), reviewer("rita"));
    r.in_review(&ann);
    flaky.refuse_model(obop_commands::audit::EVENT_TABLE, Mode::All);
    let (answer, _) = request(|| r.raw(&rev, "mileage_point.list", json!({"limit": 100})));
    let e = answer.expect_err("the points must not be given out unlogged");
    assert!(format!("{e:?}").contains("audit entry"), "{e:?}");
    flaky.allow_models();
    assert!(r.events("mileage_route_read").is_empty());
    // With the log working again the same read goes through and is logged once.
    let rows = r.ok(&rev, "mileage_point.list", json!({"limit": 100}));
    assert_eq!(rows.as_array().unwrap().len(), 10);
    assert_eq!(r.events("mileage_route_read").len(), 1);
}

// ---- the fix round of the review of the pull request ------------------------------------------

/// A trip of `ann` that a reviewer has confirmed, and the ids of one trip in every state a
/// reviewer reads.
fn reviewable_trips(r: &Rig, ann: &Principal) -> Vec<(&'static str, String)> {
    let rex = reviewer("rex");
    let in_review = r.in_review(ann);
    let confirmed = r.in_review(ann);
    r.ok(&rex, "mileage_trip.confirm", json!({"id": confirmed}));
    let rejected = r.in_review(ann);
    r.ok(
        &rex,
        "mileage_trip.update",
        json!({"id": rejected, "reject_reason": "not a business trip"}),
    );
    r.ok(&rex, "mileage_trip.reject", json!({"id": rejected}));
    vec![
        ("in_review", in_review),
        ("confirmed", confirmed),
        ("rejected", rejected),
    ]
}

#[test]
fn a_position_aggregate_a_sort_or_a_total_of_the_points_is_a_route_read_too() {
    each_rig(|r| {
        let (ann, rev, adm) = (driver("ann"), reviewer("rita"), admin("ada"));
        let id = r.in_review(&ann);
        let live = r.recording(&ann);
        r.walk(&ann, &live, 1, 3);
        let pinned = json!({"field": "trip", "op": "eq", "value": id});
        for who in [&rev, &adm] {
            // Grouping by the position gives one group, one coordinate, for every point.
            let e = r.refused(
                who,
                "mileage_point.search",
                json!({"filter": [pinned], "aggregate": {"group_by": ["lat", "lon"], "measures": []}}),
            );
            assert!(e.contains("lat") && e.contains("logged"), "{e}");
            // So does a group of one point (trip and seq) with the minimum of a coordinate; no
            // trip id is needed for it, and an administrator would get a live feed.
            let e = r.refused(
                who,
                "mileage_point.search",
                json!({"aggregate": {"group_by": ["trip", "seq"], "measures": [
                    {"op": "min", "field": "lat"}, {"op": "min", "field": "lon"}]}}),
            );
            assert!(e.contains("lat"), "{e}");
            for field in ["accuracy_m", "speed_mps"] {
                let e = r.refused(
                    who,
                    "mileage_point.search",
                    json!({"aggregate": {"group_by": ["seq"], "measures": [{"op": "max", "field": field}]}}),
                );
                assert!(e.contains(field), "{e}");
            }
            let e = r.refused(
                who,
                "mileage_point.search",
                json!({"aggregate": {"group_by": [{"field": "recorded_at", "bucket": "day"}], "measures": [{"op": "count"}]}}),
            );
            assert!(e.contains("recorded_at"), "{e}");
            // A sort by a position is a bisection of the route.
            for cmd in ["mileage_point.search", "mileage_point.list"] {
                let e = r.refused(
                    who,
                    cmd,
                    json!({"filter": [pinned], "order": [{"field": "lon", "dir": "desc"}], "limit": 1}),
                );
                assert!(e.contains("lon"), "{cmd}: {e}");
            }
            // The positions of the start and end of a trip are guarded the same way.
            let e = r.refused(
                who,
                "mileage_trip.search",
                json!({"aggregate": {"group_by": ["start_lat", "start_lon"], "measures": [{"op": "count"}]}}),
            );
            assert!(e.contains("start_lat"), "{e}");
        }
        assert!(
            r.events("mileage_route_read").is_empty(),
            "those were refused before anything was read"
        );

        // The driver reads their own route in any form; that is nobody else's business.
        let own = r.ok(
            &ann,
            "mileage_point.search",
            json!({"filter": [pinned], "aggregate": {"group_by": ["lat", "lon"], "measures": [{"op": "count"}]}}),
        );
        assert_eq!(own["groups"].as_array().unwrap().len(), 10);
        assert!(r.events("mileage_route_read").is_empty());

        // A filter on a position with the total only (no rows) still tells where the points are,
        // by bisection: that read is logged for the driver whose trip it names, and fails closed.
        let total_only = json!({"filter": [pinned,
            {"field": "lat", "op": "gt", "value": "52.0005"}], "offset": 100, "limit": 1});
        for who in [&rev, &adm] {
            let a = r.ok(who, "mileage_point.search", total_only.clone());
            assert_eq!(a["rows"], json!([]));
            assert_eq!(a["total"], 8);
        }
        let log = r.events("mileage_route_read");
        assert_eq!(
            log.iter()
                .map(|e| (e.0.as_str(), e.1.as_str()))
                .collect::<Vec<_>>(),
            vec![("ann", "ada"), ("ann", "rita")]
        );
        // A count that names no trip or driver is logged for all drivers.
        let count = json!({"aggregate": {"group_by": ["trip"], "measures": [{"op": "count"}]}});
        r.ok(&rev, "mileage_point.search", count);
        let all: Vec<_> = r
            .events("mileage_route_read")
            .into_iter()
            .filter(|e| e.0 == "*")
            .collect();
        assert_eq!(all.len(), 1, "{all:?}");
    });
}

#[test]
fn a_total_or_a_group_of_the_points_is_not_given_out_when_its_log_entry_cannot_be_written() {
    use obop_storage::testing::Mode;
    let r = Rig::flaky();
    let flaky = r.flaky.clone().unwrap();
    let (ann, rev) = (driver("ann"), reviewer("rita"));
    let id = r.in_review(&ann);
    let queries = [
        json!({"filter": [{"field": "trip", "op": "eq", "value": id},
            {"field": "lat", "op": "gt", "value": "52.0005"}], "offset": 100, "limit": 1}),
        json!({"aggregate": {"group_by": ["trip", "seq"], "measures": [{"op": "count"}]}}),
    ];
    flaky.refuse_model(obop_commands::audit::EVENT_TABLE, Mode::All);
    for q in &queries {
        let (answer, _) = request(|| r.raw(&rev, "mileage_point.search", q.clone()));
        let e = answer.expect_err("nothing may be given out unlogged");
        assert!(format!("{e:?}").contains("audit entry"), "{e:?}");
    }
    flaky.allow_models();
    assert!(r.events("mileage_route_read").is_empty());
    for q in &queries {
        r.ok(&rev, "mileage_point.search", q.clone());
    }
    assert_eq!(r.events("mileage_route_read").len(), 2);
}

#[test]
fn a_trip_holds_at_most_twenty_thousand_points_by_the_range_of_seq() {
    each_rig(|r| {
        let ann = driver("ann");
        let id = r.recording(&ann);
        for seq in [-1, 0, 20_001] {
            let e = r.refused(&ann, "mileage_point.create", walk_row(&id, seq));
            assert!(e.contains("seq") && e.contains("20000"), "{seq}: {e}");
            let rows = vec![walk_row(&id, 1), walk_row(&id, seq)];
            let e = r.refused(&ann, "mileage_point.bulk_create", json!({"rows": rows}));
            assert!(e.contains("seq") && e.contains("20000"), "{seq}: {e}");
        }
        assert_eq!(r.points_stored(&id), 0, "nothing of a refused batch");
        // The last number a trip can have is the last place in it.
        r.ok(&ann, "mileage_point.create", walk_row(&id, 20_000));
        r.ok(&ann, "mileage_point.create", walk_row(&id, 1));
        assert_eq!(r.points_stored(&id), 2);
    });
}

#[test]
fn a_batch_that_was_stored_is_answered_again_after_the_trip_has_ended() {
    each_rig(|r| {
        let (ann, bob) = (driver("ann"), driver("bob"));
        let id = r.recording(&ann);
        let rows: Vec<J> = (1..=10).map(|s| walk_row(&id, s)).collect();
        let batch = json!({"idempotency_key": "points:c:1-10", "rows": rows});
        let first = r.ok(&ann, "mileage_point.bulk_create", batch.clone());
        r.ok(
            &ann,
            "mileage_trip.end",
            json!({"id": id, "expected_points": 10, "last_seq": 10}),
        );
        // The answer was lost; the retry comes after the end, and gets the first answer.
        let again = r.ok(&ann, "mileage_point.bulk_create", batch.clone());
        assert_eq!(again["replayed"], true);
        assert_eq!(again["ids"], first["ids"]);
        assert_eq!(r.points_stored(&id), 10);
        // Everything that would write still has its checks: a new key, other rows under the
        // key, another driver with the same key.
        let mut fresh = batch.clone();
        fresh["idempotency_key"] = J::from("points:c:other");
        let e = r.refused(&ann, "mileage_point.bulk_create", fresh);
        assert!(e.contains("recording"), "{e}");
        let mut other = batch.clone();
        other["rows"] = json!([walk_row(&id, 11)]);
        r.refused(&ann, "mileage_point.bulk_create", other);
        let e = r.refused(&bob, "mileage_point.bulk_create", batch.clone());
        assert!(e.starts_with("no mileage_trip"), "{e}");
        // Also after a discard.
        let id2 = r.recording(&ann);
        let rows: Vec<J> = (1..=3).map(|s| walk_row(&id2, s)).collect();
        let batch2 = json!({"idempotency_key": "points:d:1-3", "rows": rows});
        r.ok(&ann, "mileage_point.bulk_create", batch2.clone());
        r.ok(
            &ann,
            "mileage_trip.update",
            json!({"id": id2, "discard_reason": "wrong trip"}),
        );
        r.ok(&ann, "mileage_trip.discard", json!({"id": id2}));
        let again = r.ok(&ann, "mileage_point.bulk_create", batch2);
        assert_eq!(again["replayed"], true);
    });
}

#[test]
fn the_rate_limit_is_a_coded_answer_and_a_repeat_is_never_refused_for_it() {
    each_rig(|r| {
        let ann = driver("ann");
        for i in 0..12 {
            r.ok(
                &ann,
                "mileage_trip.bulk_create",
                json!({"idempotency_key": format!("trip:{i}"), "rows": [trip_row_input(&format!("c-{i}"))]}),
            );
        }
        let new = json!({"idempotency_key": "trip:13", "rows": [trip_row_input("c-13")]});
        let (status, code) = r.classified(&ann, "mileage_trip.bulk_create", new);
        assert_eq!((status, code.as_str()), (429, "rate_limited"));
        // The retry of a call that was answered gets its first answer, whatever the minute holds.
        let again = r.ok(
            &ann,
            "mileage_trip.bulk_create",
            json!({"idempotency_key": "trip:3", "rows": [trip_row_input("c-3")]}),
        );
        assert_eq!(again["replayed"], true);
    });
}

fn trip_row_input(client: &str) -> J {
    json!({"client_trip_id": client, "started_at": stamp(start()), "purpose": "business"})
}

#[test]
fn end_is_retryable_only_while_more_points_could_still_arrive() {
    each_rig(|r| {
        let ann = driver("ann");
        // More points are stored than the device said: no waiting helps, the counts are wrong.
        let id = r.recording(&ann);
        r.walk(&ann, &id, 1, 4);
        for (expected, last) in [(3, 3), (4, 3), (3, 4), (5, 3)] {
            let end = json!({"id": id, "expected_points": expected, "last_seq": last});
            let (status, code) = r.classified(&ann, "mileage_trip.end", end.clone());
            assert_eq!(
                (status, code.as_str()),
                (400, "bad_request"),
                "{expected} points to seq {last}"
            );
            let e = r.refused(&ann, "mileage_trip.end", end);
            assert!(
                e.contains("expected_points 4") && e.contains("last_seq 4"),
                "the refusal says what to send: {e}"
            );
        }
        assert_eq!(r.state(&id), "recording", "nothing was changed");
        // With the counts of what is stored the same trip ends.
        r.ok(
            &ann,
            "mileage_trip.end",
            json!({"id": id, "expected_points": 4, "last_seq": 4}),
        );
        // Fewer stored than said, and none beyond the last seq: that one is the retryable one.
        let late = r.recording(&ann);
        r.walk(&ann, &late, 1, 3);
        let (status, code) = r.classified(
            &ann,
            "mileage_trip.end",
            json!({"id": late, "expected_points": 5, "last_seq": 5}),
        );
        assert_eq!((status, code.as_str()), (409, "incomplete"));
        // The hint says that counts lost on the device can be corrected.
        let c = match r.raw(
            &ann,
            "mileage_trip.end",
            json!({"id": late, "expected_points": 5, "last_seq": 5}),
        ) {
            Err(e) => errors::classify_with(&e, &errors::Outcome::default()),
            Ok(v) => panic!("{v}"),
        };
        assert!(c.body["hint"].as_str().unwrap().contains("counts"), "{c:?}");
        // The device lost two of them for good and says so with the counts: it ends.
        r.ok(
            &ann,
            "mileage_trip.end",
            json!({"id": late, "expected_points": 3, "last_seq": 3}),
        );
        assert_eq!(r.state(&late), "ended");
    });
}

#[test]
fn a_distance_that_does_not_fit_its_field_is_refused_and_not_stored() {
    each_rig(|r| {
        let ann = driver("ann");
        let id = r.recording(&ann);
        // 600 fixes that alternate between two opposite points of the globe, a hundred days apart:
        // every step is slow enough for the speed cap (2 m/s) and 20,000 km long, so the sum is
        // 12 billion metres, more than a decimal(12,2) holds.
        let rows: Vec<J> = (1..=600_i64)
            .map(|seq| {
                json!({
                    "trip": id,
                    "seq": seq,
                    "recorded_at": stamp(start() + seq * 100 * 86_400 * 1_000_000),
                    "lat": "0.0000000",
                    "lon": if seq % 2 == 0 { "179.9999999" } else { "0.0000000" },
                })
            })
            .collect();
        r.ok(&ann, "mileage_point.bulk_create", json!({"rows": rows}));
        let end = json!({"id": id, "expected_points": 600, "last_seq": 600});
        let (status, code) = r.classified(&ann, "mileage_trip.end", end.clone());
        assert_eq!((status, code.as_str()), (400, "bad_request"));
        let e = r.refused(&ann, "mileage_trip.end", end);
        assert!(e.contains("distance") && e.contains("discard"), "{e}");
        assert_eq!(r.state(&id), "recording", "nothing was changed");
        let row = r.trip_row(&id);
        assert!(row.get("distance_m").is_none_or(J::is_null), "{row:?}");
    });
}

#[test]
fn the_start_and_the_end_are_the_first_and_the_last_fix_that_counts() {
    each_rig(|r| {
        let ann = driver("ann");
        let id = r.recording(&ann);
        // The first and the last stored fixes are poor (500 m): they are left out, and so are
        // the position and the time that would come from them.
        for seq in 1..=10 {
            let mut row = walk_row(&id, seq);
            if seq == 1 || seq == 10 {
                row["accuracy_m"] = J::from("500");
            }
            r.ok(&ann, "mileage_point.create", row);
        }
        r.ok(
            &ann,
            "mileage_trip.end",
            json!({"id": id, "expected_points": 10, "last_seq": 10}),
        );
        let row = r.trip_row(&id);
        assert_eq!(row["start_lat"], "52.0004000", "{row:?}");
        assert_eq!(row["end_lat"], "52.0018000", "{row:?}");
        assert_eq!(row["ended_at"], stamp(start() + 9 * 5_000_000), "{row:?}");
        assert_eq!(row["integrity"], "low_accuracy:2");
    });
}

#[test]
fn nobody_reopens_their_own_rejected_trip() {
    each_rig(|r| {
        let (rita, rex) = (reviewer("rita"), reviewer("rex"));
        let id = r.in_review(&rita);
        r.ok(
            &rex,
            "mileage_trip.update",
            json!({"id": id, "reject_reason": "no receipt"}),
        );
        r.ok(&rex, "mileage_trip.reject", json!({"id": id}));
        assert_eq!(r.state(&id), "rejected");
        let e = r.refused(&rita, "mileage_trip.reopen", json!({"id": id}));
        assert!(e.contains("own trip"), "{e}");
        assert_eq!(r.state(&id), "rejected");
        r.ok(&rex, "mileage_trip.reopen", json!({"id": id}));
        assert_eq!(r.state(&id), "ended");
    });
}

#[test]
fn an_agent_proposes_a_rejection_and_does_not_run_it() {
    each_rig(|r| {
        let (ann, rex) = (driver("ann"), reviewer("rex"));
        let id = r.in_review(&ann);
        r.ok(
            &rex,
            "mileage_trip.update",
            json!({"id": id, "reject_reason": "no receipt"}),
        );
        let agent = Principal::agent("rex/agent", &[DRIVER, REVIEWER], &[DRIVER, REVIEWER]);
        match r.raw(&agent, "mileage_trip.reject", json!({"id": id})) {
            Err(CallError::ApprovalRequired { .. }) => {}
            other => panic!("expected a proposal, got {other:?}"),
        }
        assert_eq!(r.state(&id), "in_review");
    });
}

#[test]
fn the_points_take_rows_only_and_only_real_positions() {
    each_rig(|r| {
        let ann = driver("ann");
        let id = r.recording(&ann);
        let e = r.refused(
            &ann,
            "mileage_point.bulk_create",
            json!({"csv": "trip,seq,recorded_at,lat,lon\n"}),
        );
        assert!(e.contains("csv"), "{e}");
        assert_eq!(r.points_stored(&id), 0);
        for (field, value) in [
            ("lon", json!("181.0000000")),
            ("lon", json!("-180.5000000")),
            ("lat", json!("-90.5000000")),
            ("accuracy_m", json!("-1")),
            ("speed_mps", json!("-1")),
        ] {
            let mut row = walk_row(&id, 1);
            row[field] = value.clone();
            let e = r.refused(&ann, "mileage_point.create", row.clone());
            assert!(e.contains(field), "{field} {value}: {e}");
            let e = r.refused(&ann, "mileage_point.bulk_create", json!({"rows": [row]}));
            assert!(e.contains(field), "{field} {value}: {e}");
        }
        let boundary = {
            let mut row = walk_row(&id, 1);
            row["lon"] = J::from("180.0000000");
            row["lat"] = J::from("-90.0000000");
            row
        };
        r.ok(&ann, "mileage_point.create", boundary);
        assert_eq!(r.points_stored(&id), 1);
        let stored = &r
            .store
            .backend()
            .find(POINT, "trip", &J::from(id.as_str()))
            .unwrap()[0]
            .1;
        assert_eq!(stored["driver"], "ann", "the server stamps the driver");
    });
}

#[test]
fn a_point_of_a_reviewable_trip_is_not_another_drivers_to_know_of() {
    each_rig(|r| {
        let (ann, bob, rev) = (driver("ann"), driver("bob"), reviewer("rita"));
        let id = r.in_review(&ann);
        let point = r
            .store
            .backend()
            .find(POINT, "trip", &J::from(id.as_str()))
            .unwrap()[0]
            .0
            .clone();
        for (cmd, extra) in [
            ("mileage_point.get", json!({})),
            ("mileage_point.update", json!({"lat": "1.0000000"})),
            // A field the generated guard would refuse in its own words must not tell either.
            ("mileage_point.update", json!({"driver": "bob"})),
            ("mileage_point.update", json!({"no_such_field": 1})),
            ("mileage_point.delete", json!({})),
        ] {
            let ask = |who: &Principal, pid: &str| {
                let mut input = json!({"id": pid});
                for (k, v) in extra.as_object().unwrap() {
                    input[k] = v.clone();
                }
                r.refused(who, cmd, input).replace(pid, "X")
            };
            assert_eq!(ask(&bob, &point), ask(&bob, "no-such-point"), "{cmd}");
        }
        // The reviewer reads the point of a trip in review; that is not hidden from them.
        r.ok(&rev, "mileage_point.get", json!({"id": point}));
    });
}

#[test]
fn a_trip_the_caller_may_not_see_answers_every_command_as_a_trip_that_does_not_exist() {
    each_rig(|r| {
        let (ann, bob, rev) = (driver("ann"), driver("bob"), reviewer("rita"));
        let commands = [
            ("mileage_trip.get", json!({})),
            ("mileage_trip.delete", json!({})),
            ("mileage_trip.update", json!({"state": "confirmed"})),
            ("mileage_trip.update", json!({"purpose": "private"})),
            ("mileage_trip.update", json!({"reject_reason": "x"})),
            (
                "mileage_trip.end",
                json!({"expected_points": 10, "last_seq": 10}),
            ),
            ("mileage_trip.submit", json!({})),
            ("mileage_trip.confirm", json!({})),
            ("mileage_trip.reject", json!({})),
            ("mileage_trip.reopen", json!({})),
            ("mileage_trip.discard", json!({})),
            ("mileage_trip.discard_stuck", json!({})),
        ];
        let ask = |who: &Principal, cmd: &str, extra: &J, id: &str| {
            let mut input = json!({"id": id});
            for (k, v) in extra.as_object().unwrap() {
                input[k] = v.clone();
            }
            r.refused(who, cmd, input).replace(id, "X")
        };
        // Another driver, who holds the base group the row rule of the model goes by: a trip in
        // review, confirmed or rejected is visible to the engine, not to them.
        for (state, id) in reviewable_trips(r, &ann) {
            for (cmd, extra) in &commands {
                assert_eq!(
                    ask(&bob, cmd, extra, &id),
                    ask(&bob, cmd, extra, "no-such-trip"),
                    "{cmd} {extra} by bob on a trip that is {state}"
                );
            }
        }
        // A reviewer, on trips that are not yet in review.
        let recording = r.recording(&ann);
        let ended = r.ended(&ann);
        for id in [&recording, &ended] {
            for (cmd, extra) in &commands {
                assert_eq!(
                    ask(&rev, cmd, extra, id),
                    ask(&rev, cmd, extra, "no-such-trip"),
                    "{cmd} {extra} by the reviewer on a trip that is {}",
                    r.state(id)
                );
            }
        }
    });
}

#[test]
fn a_reviewer_or_administrator_without_the_driver_group_acts_on_nothing() {
    each_rig(|r| {
        let ann = driver("ann");
        let (only_rev, only_adm) = (
            Principal::user("rex", &[REVIEWER]),
            Principal::user("ada", &[ADMIN]),
        );
        let review = r.in_review(&ann);
        let stuck = r.recording(&ann);
        let rejected = {
            let id = r.in_review(&ann);
            let rita = reviewer("rita");
            r.ok(
                &rita,
                "mileage_trip.update",
                json!({"id": id, "reject_reason": "no receipt"}),
            );
            r.ok(&rita, "mileage_trip.reject", json!({"id": id}));
            id
        };
        for (who, cmd, id) in [
            (&only_rev, "mileage_trip.confirm", &review),
            (&only_rev, "mileage_trip.reject", &review),
            (&only_rev, "mileage_trip.reopen", &rejected),
            (&only_adm, "mileage_trip.discard_stuck", &stuck),
        ] {
            let e = r.refused(who, cmd, json!({"id": id}));
            assert!(e.starts_with("no mileage_trip"), "{cmd}: {e}");
        }
        assert_eq!(r.state(&review), "in_review");
        assert_eq!(r.state(&rejected), "rejected");
        assert_eq!(r.state(&stuck), "recording");
        // The role is a role on top of the base group: with it they act.
        r.ok(
            &reviewer("rex"),
            "mileage_trip.confirm",
            json!({"id": review}),
        );
        r.ok(
            &admin("ada"),
            "mileage_trip.discard_stuck",
            json!({"id": stuck}),
        );
    });
}

#[test]
fn the_handler_checks_the_roles_of_the_actor_again_whatever_the_guard_let_through() {
    // The actions run in a handler that decides by the actor the dispatcher stamped. Called
    // directly, as a guard that failed to refuse would let it be, it holds the roles to the same
    // rule: a role counts only on top of the base group.
    each_rig(|r| {
        let ann = driver("ann");
        let review = r.in_review(&ann);
        let stuck = r.recording(&ann);
        let db = r.store.backend();
        let clock: Clock = Arc::new(start);
        let call = |action: &'static str, id: &str, who: &str, groups: &[&str]| {
            let input = Value::from_json(&json!({
                "id": id,
                "_actor": who,
                "_actor_caller": who,
                "_actor_admin": false,
                "_actor_groups": groups,
            }));
            crate::handlers::run(&db, &clock, action, &input)
        };
        let e = call("confirm", &review, "rex", &[REVIEWER]).unwrap_err();
        assert!(e.contains("only a reviewer"), "{e}");
        let e = call("discard_stuck", &stuck, "ada", &[ADMIN]).unwrap_err();
        assert!(e.contains("only a mileage administrator"), "{e}");
        assert_eq!(r.state(&review), "in_review");
        assert_eq!(r.state(&stuck), "recording");
        call("confirm", &review, "rex", &[DRIVER, REVIEWER]).unwrap();
        call("discard_stuck", &stuck, "ada", &[DRIVER, ADMIN]).unwrap();
        assert_eq!(r.state(&review), "confirmed");
        assert_eq!(r.state(&stuck), "discarded");
    });
}

#[test]
fn an_agent_reads_a_trip_without_what_identifies_the_person_or_the_drive() {
    each_rig(|r| {
        let (ann, rev) = (driver("ann"), reviewer("rita"));
        let id = r.ended(&ann);
        r.ok(
            &ann,
            "mileage_trip.update",
            json!({"id": id, "vehicle": "B-AB 1234"}),
        );
        r.ok(&ann, "mileage_trip.submit", json!({"id": id}));
        r.ok(
            &rev,
            "mileage_trip.update",
            json!({"id": id, "reject_reason": "went to the clinic, not business"}),
        );
        let agent = Principal::agent("rita/agent", &[DRIVER, REVIEWER], &[DRIVER, REVIEWER]);
        let read = |who: &Principal| {
            r.d.call(
                Surface::Rest,
                who,
                &mut Vault::new(),
                "mileage_trip.get",
                &Value::from_json(&json!({"id": id})),
            )
            .unwrap()
        };
        let seen = read(&agent);
        let v = seen.value.to_json();
        // Personal data is a token the agent can pass back, not the text.
        for (field, clear) in [
            ("vehicle", "B-AB 1234"),
            ("purpose", "business"),
            ("reject_reason", "went to the clinic, not business"),
            ("driver", "ann"),
        ] {
            let got = v[field].as_str().unwrap_or_default();
            assert!(
                !got.is_empty() && got != clear && !got.contains(clear),
                "{field}: {got}"
            );
        }
        // What is derived from a position or a time of the drive is withheld altogether.
        for field in [
            "started_at",
            "ended_at",
            "start_lat",
            "start_lon",
            "end_lat",
            "end_lon",
        ] {
            assert!(v.get(field).is_none(), "{field} reached the agent: {v}");
            assert!(
                seen.denied.iter().any(|d| d.ends_with(field)),
                "{field}: {:?}",
                seen.denied
            );
        }
        // The distance and the state are what an agent needs to propose a decision.
        assert_eq!(v["distance_m"], "200.15");
        assert_eq!(v["state"], "in_review");
        // A person with the same groups reads it all.
        for who in [&rev, &ann] {
            let v = read(who).value.to_json();
            assert_eq!(v["vehicle"], "B-AB 1234");
            assert_eq!(v["reject_reason"], "went to the clinic, not business");
            assert!(v["started_at"].as_str().is_some() && v["ended_at"].as_str().is_some());
        }
    });
}

/// A decision that loses to a change landing between its read and its write is refused and
/// changes nothing, whichever of its reads the change falls after.
#[test]
fn a_decision_loses_to_a_change_that_lands_between_its_read_and_its_write() {
    let mut conflicts = 0;
    for skip in 0..14 {
        let r = Rig::flaky();
        let flaky = r.flaky.clone().unwrap();
        let (ann, rev) = (driver("ann"), reviewer("rita"));
        let id = r.in_review(&ann);
        let other = id.clone();
        let landed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = landed.clone();
        flaky.after_the_get_of(
            TRIP,
            skip,
            Box::new(move |db| {
                let mut patch = obop_storage::Fields::new();
                patch.insert("state".into(), J::from("rejected"));
                db.update(TRIP, &other, &patch).unwrap();
                flag.store(true, Ordering::SeqCst);
            }),
        );
        let answer = r.raw(&rev, "mileage_trip.confirm", json!({"id": id}));
        // Read the flag first: the reads below are reads of the trip too.
        let fired = landed.load(Ordering::SeqCst);
        let row = r.trip_row(&id);
        if let Err(e) = &answer {
            let c = errors::classify_with(e, &errors::Outcome::default());
            if c.code == "state_conflict" {
                conflicts += 1;
            }
        }
        if fired {
            assert_ne!(
                row["state"], "confirmed",
                "skip {skip}: a confirmation overwrote the change ({answer:?})"
            );
            assert!(
                row.get("confirmed_at").is_none_or(J::is_null),
                "skip {skip}: {row:?}"
            );
        }
    }
    assert!(conflicts > 0, "the walk never met the compare-and-set");
}

#[test]
fn a_stuck_trip_discard_and_its_log_entry_are_one_write_that_is_never_half_done() {
    // The change lands between the read and the write: no discard, and no entry.
    let mut conflicts = 0;
    for skip in 0..14 {
        let r = Rig::flaky();
        let flaky = r.flaky.clone().unwrap();
        let id = r.recording(&driver("ann"));
        let other = id.clone();
        flaky.after_the_get_of(
            TRIP,
            skip,
            Box::new(move |db| {
                let mut patch = obop_storage::Fields::new();
                patch.insert("state".into(), J::from("ended"));
                db.update(TRIP, &other, &patch).unwrap();
            }),
        );
        let answer = r.raw(
            &admin("ada"),
            "mileage_trip.discard_stuck",
            json!({"id": id}),
        );
        let entries = r.events("mileage_discard_stuck").len();
        match r.state(&id).as_str() {
            "discarded" => assert_eq!(entries, 1, "skip {skip}"),
            _ => assert_eq!(
                entries, 0,
                "skip {skip}: an entry for a discard that did not happen"
            ),
        }
        if let Err(e) = &answer
            && errors::classify_with(e, &errors::Outcome::default()).code == "state_conflict"
        {
            conflicts += 1;
        }
    }
    assert!(conflicts > 0, "the walk never met the compare-and-set");
}
