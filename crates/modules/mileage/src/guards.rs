//! Guards: the rules of the module that the generated commands cannot state (docs/spec/mobile.md
//! 5.8). They sit behind the guards the registry already generates (`Dispatcher::add_guard`), so
//! the governed state, the kinds, the row rule and the query checks still apply first, and they
//! run under the dispatcher's transaction lock with the handler of the same call.
//!
//! - Reads: a plain driver sees only their own trips and points; a reviewer sees the trips
//!   from the review on; an administrator sees all. A record the caller may not see answers
//!   exactly as a record that does not exist.
//! - Writes of a trip: the driver writes the fields of `policy::DRIVER_FIELDS` while the trip is
//!   recording, ended or rejected; the reviewer writes `reject_reason` while it is in review;
//!   nobody writes the fields the server sets, nor `state`; nobody deletes a trip.
//! - Points: written only by `bulk_create` or `create` for the caller's own trip while it is
//!   recording, never changed, never deleted.
//! - Actions: the state, the owner and the separation of duties (`policy::check_action`).

use crate::policy::{self, Fields, Who};
use obop_commands::{Dispatcher, Guard, Principal, Value, Verdict};
use obop_storage::Backend;
use serde_json::Value as J;
use std::collections::BTreeMap;
use std::sync::Arc;

type Db = Arc<dyn Backend>;
type M = BTreeMap<String, Value>;

fn map(v: &Value) -> Result<&M, String> {
    match v {
        Value::Map(m) => Ok(m),
        _ => Err("expected an object".into()),
    }
}

fn id_of(input: &Value) -> Result<String, String> {
    match map(input)?.get("id") {
        Some(Value::Text(id)) => Ok(id.clone()),
        _ => Err("missing id".into()),
    }
}

fn new_input(m: M) -> Verdict {
    Verdict {
        input: Some(Value::Map(m)),
        ..Verdict::default()
    }
}

/// The trip `id` if it exists and `who` may see it; otherwise the words for a trip that does not
/// exist.
fn visible_trip(db: &Db, who: &Who, id: &str) -> Result<Fields, String> {
    match db.get(policy::TRIP, id)? {
        Some(t) if policy::may_see_trip(who, &t) => Ok(t),
        _ => Err(policy::no_such(policy::TRIP, id)),
    }
}

fn refuse(msg: &'static str) -> Guard {
    Box::new(move |_, _| Err(msg.into()))
}

fn driver_filter(who: &Who) -> Value {
    Value::map([
        ("field", Value::text("driver")),
        ("op", Value::text("eq")),
        ("value", Value::text(&who.id)),
    ])
}

/// `list` and `search`: a plain driver asks only for their own records. (The row rule of the
/// models would show them every trip in review, and the points of those trips.)
fn narrow_list() -> Guard {
    Box::new(|p, input| {
        let who = Who::of(p);
        if !who.plain_driver() {
            return Ok(Verdict::default());
        }
        let mut m = map(input)?.clone();
        let mut filter = match m.remove("filter") {
            Some(Value::List(l)) => l,
            _ => vec![],
        };
        filter.push(driver_filter(&who));
        m.insert("filter".into(), Value::List(filter));
        Ok(new_input(m))
    })
}

fn get_trip(db: Db) -> Guard {
    Box::new(move |p, input| {
        visible_trip(&db, &Who::of(p), &id_of(input)?).map(|_| Verdict::default())
    })
}

fn get_point(db: Db) -> Guard {
    Box::new(move |p, input| {
        let (who, id) = (Who::of(p), id_of(input)?);
        match db.get(policy::POINT, &id)? {
            Some(pt) if !who.plain_driver() || policy::text(&pt, "driver") == who.id => {
                Ok(Verdict::default())
            }
            _ => Err(policy::no_such(policy::POINT, &id)),
        }
    })
}

// ---- writing a trip -------------------------------------------------------------------------

/// The declared fields a client may not give: the server sets them.
const SERVER_SET: [&str; 10] = [
    "distance_m",
    "integrity",
    "start_lat",
    "start_lon",
    "end_lat",
    "end_lon",
    "reject_reason",
    "discard_reason",
    "reviewed_by",
    "confirmed_at",
];

fn update_trip(db: Db) -> Guard {
    Box::new(move |p, input| {
        let who = Who::of(p);
        let m = map(input)?;
        let id = id_of(input)?;
        let trip = visible_trip(&db, &who, &id)?;
        let changed: Vec<&String> = m
            .iter()
            .filter(|(k, v)| {
                let k = k.as_str();
                if k == "id" || k.starts_with('_') || policy::COMPUTED.contains(&k) {
                    return false;
                }
                let stored = trip.get(k).cloned().unwrap_or(J::Null);
                let given = v.to_json();
                stored != given && !(stored.is_null() && given == J::String(String::new()))
            })
            .map(|(k, _)| k)
            .collect();
        if changed.is_empty() {
            return Ok(Verdict::default());
        }
        let allowed = policy::writable_fields(&who, &trip)?;
        for k in changed {
            if k == "state" {
                return Err(
                    "mileage_trip.state moves only through the actions: end, submit, confirm, reject, reopen, discard and discard_stuck"
                        .into(),
                );
            }
            if !allowed.contains(&k.as_str()) {
                return Err(match k.as_str() {
                    "reject_reason" if who.reviewer => format!(
                        "mileage_trip.reject_reason can be written while the trip is in review, by a reviewer who is not its driver; this trip is {}",
                        policy::state(&trip)
                    ),
                    f if SERVER_SET.contains(&f) || f == "client_trip_id" || f == "driver" => {
                        format!("mileage_trip.{f} cannot be changed by update: the server sets it")
                    }
                    f => format!("mileage_trip.{f} cannot be changed by you on this trip"),
                });
            }
        }
        Ok(Verdict::default())
    })
}

/// One trip row of `create` or `bulk_create`: only what the device knows at the start, and the
/// driver is the caller.
fn trip_row(who: &Who, row: &Value) -> Result<Value, String> {
    let Value::Map(r) = row else {
        return Ok(row.clone()); // the engine says "a row must be an object"
    };
    for (k, v) in r {
        let k = k.as_str();
        if policy::CREATE_FIELDS.contains(&k) {
            continue;
        }
        if k == "state" {
            if matches!(v, Value::Null) || v == &Value::text(policy::RECORDING) {
                continue;
            }
            return Err(
                "mileage_trip.state starts as \"recording\" when a trip is created; it moves on through the actions"
                    .into(),
            );
        }
        if SERVER_SET.contains(&k) || policy::COMPUTED.contains(&k) {
            return Err(format!(
                "mileage_trip.{k} is set by the server and cannot be given"
            ));
        }
    }
    match r.get("driver") {
        None | Some(Value::Null) => {}
        Some(Value::Text(d)) if d == &who.id => {}
        Some(_) => {
            return Err("a trip belongs to the person who records it: driver is you".into());
        }
    }
    let mut r = r.clone();
    r.insert("driver".into(), Value::text(&who.id));
    Ok(Value::Map(r))
}

fn create_trip() -> Guard {
    Box::new(|p, input| {
        let row = trip_row(&Who::of(p), input)?;
        Ok(match row {
            Value::Map(m) => new_input(m),
            _ => Verdict::default(),
        })
    })
}

fn rows_of(m: &M) -> Result<Option<&Vec<Value>>, String> {
    if m.contains_key("csv") && !matches!(m.get("csv"), Some(Value::Null)) {
        return Err(
            "the Mileage models take rows (a list of objects), not csv: the positions and the owner are checked row by row"
                .into(),
        );
    }
    match m.get("rows") {
        Some(Value::List(l)) => Ok(Some(l)),
        _ => Ok(None), // the engine says what is missing
    }
}

fn bulk_trips() -> Guard {
    Box::new(|p, input| {
        let who = Who::of(p);
        let m = map(input)?;
        let Some(rows) = rows_of(m)? else {
            return Ok(Verdict::default());
        };
        let rewritten = rows
            .iter()
            .map(|r| trip_row(&who, r))
            .collect::<Result<Vec<_>, _>>()?;
        let mut out = m.clone();
        out.insert("rows".into(), Value::List(rewritten));
        Ok(new_input(out))
    })
}

// ---- writing points -------------------------------------------------------------------------

fn degrees(v: &Value, name: &str, limit: f64) -> Result<(), String> {
    let n = match v {
        Value::Text(t) => t.trim().parse::<f64>().ok(),
        Value::Int(i) => Some(*i as f64),
        _ => None,
    };
    match n {
        Some(n) if n.is_finite() && n.abs() <= limit => Ok(()),
        // Not a number at all: the kind check of the engine says so, with the field's words.
        None => Ok(()),
        Some(_) => Err(format!(
            "mileage_point.{name} must be between -{limit} and {limit} degrees"
        )),
    }
}

fn non_negative(v: &Value, name: &str) -> Result<(), String> {
    let n = match v {
        Value::Text(t) => t.trim().parse::<f64>().ok(),
        Value::Int(i) => Some(*i as f64),
        _ => None,
    };
    match n {
        Some(n) if n < 0.0 || !n.is_finite() => {
            Err(format!("mileage_point.{name} cannot be negative"))
        }
        _ => Ok(()),
    }
}

fn point_values(r: &M) -> Result<(), String> {
    for (k, v) in r {
        match k.as_str() {
            "lat" => degrees(v, "lat", 90.0)?,
            "lon" => degrees(v, "lon", 180.0)?,
            "accuracy_m" => non_negative(v, "accuracy_m")?,
            "speed_mps" => non_negative(v, "speed_mps")?,
            "seq" => {
                if matches!(v, Value::Int(n) if *n < 0) {
                    return Err("mileage_point.seq cannot be negative".into());
                }
            }
            k if policy::POINT_FIELDS.contains(&k) => {}
            other => {
                // A declared field that is not the client's, or one that does not exist (the
                // engine names it).
                if other == "id" {
                    return Err("id is assigned by the system and cannot be set".into());
                }
            }
        }
    }
    Ok(())
}

/// The trip a batch of points belongs to: the caller's own, still recording, with room left.
fn trip_for_points(db: &Db, who: &Who, id: &str, adding: usize) -> Result<(), String> {
    // Another person's trip looks exactly like a trip that does not exist, whatever its state.
    let trip = match db.get(policy::TRIP, id)? {
        Some(t) if policy::owns(who, &t) => t,
        _ => return Err(policy::no_such(policy::TRIP, id)),
    };
    let st = policy::state(&trip);
    if st != policy::RECORDING {
        return Err(format!(
            "mileage_trip {id} is {st}: points can only be added while the trip is recording"
        ));
    }
    let have = db.find(policy::POINT, "trip", &J::from(id))?.len();
    policy::room_for(have, adding)
}

/// Rows of points for one trip: checked against the trip, stamped with its driver.
fn point_rows(db: &Db, who: &Who, rows: &[Value]) -> Result<Vec<Value>, String> {
    let mut trips: Vec<&str> = rows
        .iter()
        .filter_map(|r| match r {
            Value::Map(m) => match m.get("trip") {
                Some(Value::Text(t)) => Some(t.as_str()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    trips.sort_unstable();
    trips.dedup();
    if trips.len() > 1 {
        return Err("a batch holds the points of one trip".into());
    }
    for r in rows {
        if let Value::Map(m) = r {
            point_values(m)?;
            match m.get("driver") {
                None | Some(Value::Null) => {}
                Some(Value::Text(d)) if d == &who.id => {}
                Some(_) => {
                    return Err("driver is the driver of the trip; the server sets it".into());
                }
            }
        }
    }
    if let Some(trip) = trips.first() {
        trip_for_points(db, who, trip, rows.len())?;
    }
    Ok(rows
        .iter()
        .map(|r| match r {
            Value::Map(m) => {
                let mut m = m.clone();
                m.insert("driver".into(), Value::text(&who.id));
                Value::Map(m)
            }
            other => other.clone(),
        })
        .collect())
}

fn create_point(db: Db) -> Guard {
    Box::new(move |p, input| {
        let rows = point_rows(&db, &Who::of(p), std::slice::from_ref(input))?;
        Ok(match rows.into_iter().next() {
            Some(Value::Map(m)) => new_input(m),
            _ => Verdict::default(),
        })
    })
}

fn bulk_points(db: Db) -> Guard {
    Box::new(move |p, input| {
        let m = map(input)?;
        let Some(rows) = rows_of(m)? else {
            return Ok(Verdict::default());
        };
        let rewritten = point_rows(&db, &Who::of(p), rows)?;
        let mut out = m.clone();
        out.insert("rows".into(), Value::List(rewritten));
        Ok(new_input(out))
    })
}

// ---- actions --------------------------------------------------------------------------------

fn action(db: Db, name: &'static str) -> Guard {
    Box::new(move |p: &Principal, input| {
        let who = Who::of(p);
        let trip = visible_trip(&db, &who, &id_of(input)?)?;
        policy::check_action(name, &who, None, &trip).map(|_| Verdict::default())
    })
}

pub fn install(d: &Dispatcher, module: &str, db: Db) {
    let id = |model: &str, c: &str| format!("{model}.{c}");
    let (t, pt) = (policy::TRIP, policy::POINT);
    d.add_guard(module, &id(t, "get"), false, get_trip(db.clone()));
    d.add_guard(module, &id(pt, "get"), false, get_point(db.clone()));
    for c in ["list", "search"] {
        d.add_guard(module, &id(t, c), false, narrow_list());
        d.add_guard(module, &id(pt, c), false, narrow_list());
    }
    d.add_guard(module, &id(t, "create"), true, create_trip());
    d.add_guard(module, &id(t, "bulk_create"), true, bulk_trips());
    d.add_guard(module, &id(t, "update"), true, update_trip(db.clone()));
    d.add_guard(
        module,
        &id(t, "delete"),
        true,
        refuse(
            "a trip is kept for the audit and cannot be deleted; the driver discards it, an administrator discards a stuck one",
        ),
    );
    d.add_guard(module, &id(pt, "create"), true, create_point(db.clone()));
    d.add_guard(
        module,
        &id(pt, "bulk_create"),
        true,
        bulk_points(db.clone()),
    );
    d.add_guard(
        module,
        &id(pt, "update"),
        true,
        refuse("a point is never changed after it is stored"),
    );
    d.add_guard(
        module,
        &id(pt, "delete"),
        true,
        refuse("a point is never deleted by a command; the retention job thins them"),
    );
    for a in policy::ACTIONS {
        d.add_guard(module, &id(t, a), true, action(db.clone(), a));
    }
}
