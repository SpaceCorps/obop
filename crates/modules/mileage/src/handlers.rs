//! The seven actions of a trip. Each one is a single compare-and-set write of the trip: the
//! state is checked again against the stored record inside the write, so two callers cannot both
//! move the same trip, and what an action stores (the distance, `confirmed_at`, the reviewer)
//! is written in the same step as the new state.
//!
//! Why not the generated transition: it can set the state and run effects, but it cannot compute
//! a value from the children (the distance from the points) or stamp the time of the call into
//! the record itself (`confirmed_at`); docs/spec/mobile.md 5.8 items 4 and 7. The module does it
//! here, and the guards of `guards.rs` have already refused everything the policy does not allow.

use crate::Clock;
use crate::policy::{self, Fields, Who};
use crate::track::{self, Fix, Options};
use obop_commands::errors::{self, code};
use obop_commands::{Actor, Dispatcher, Value};
use obop_registry::Module;
use obop_storage::{ApplyError, Backend, Op};
use obop_values::Instant;
use serde_json::Value as J;
use std::collections::HashMap;
use std::sync::Arc;

type Db = Arc<dyn Backend>;

fn text(input: &Value, k: &str) -> String {
    match input {
        Value::Map(m) => match m.get(k) {
            Some(Value::Text(t)) => t.clone(),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

fn now_canonical(clock: &Clock) -> Result<String, String> {
    Instant::from_micros(clock())
        .map(|i| i.canonical())
        .ok_or_else(|| "the clock is outside the range of an instant".to_string())
}

fn micros(canonical: &str) -> Option<i64> {
    Instant::parse(canonical).ok().map(|i| i.micros())
}

fn number(trip: &Fields, k: &str) -> Option<i64> {
    trip.get(k).and_then(J::as_i64)
}

/// A whole number the caller gave in the input of the action (a number or its digits).
fn given(input: &Value, k: &str) -> Option<i64> {
    match input {
        Value::Map(m) => match m.get(k) {
            Some(Value::Int(n)) => Some(*n),
            Some(Value::Text(t)) => t.trim().parse().ok(),
            _ => None,
        },
        _ => None,
    }
}

struct Stored {
    seq: i64,
    fix: Fix,
    lat: String,
    lon: String,
    recorded_at: String,
}

fn stored_points(db: &Db, trip_id: &str) -> Result<Vec<Stored>, String> {
    let mut out = Vec::new();
    for (id, p) in db.find(policy::POINT, "trip", &J::from(trip_id))? {
        let bad = |what: &str| format!("the stored point {id} has no usable {what}");
        let seq = p.get("seq").and_then(J::as_i64).ok_or_else(|| bad("seq"))?;
        let recorded_at = policy::text(&p, "recorded_at").to_string();
        let t_us = micros(&recorded_at).ok_or_else(|| bad("recorded_at"))?;
        let lat = policy::text(&p, "lat").to_string();
        let lon = policy::text(&p, "lon").to_string();
        out.push(Stored {
            seq,
            fix: Fix {
                seq,
                t_ms: t_us as f64 / 1000.0,
                lat: lat.parse().map_err(|_| bad("lat"))?,
                lon: lon.parse().map_err(|_| bad("lon"))?,
                accuracy_m: p
                    .get("accuracy_m")
                    .and_then(J::as_str)
                    .and_then(|a| a.parse().ok()),
            },
            lat,
            lon,
            recorded_at,
        });
    }
    out.sort_by_key(|s| s.seq);
    Ok(out)
}

/// The largest `distance_m` the field holds, a decimal of 12 digits with 2 after the point. The
/// handler writes the field with a compare-and-set that does not go through the kinds, so it asks.
const MAX_DISTANCE: &str = "9999999999.99";

fn fits_distance(text: &str) -> bool {
    text.split('.')
        .next()
        .is_some_and(|whole| whole.len() <= 10)
}

/// `end`: the points must all be there, then the server decides the distance.
fn end(db: &Db, id: &str, trip: &Fields, input: &Value) -> Result<Fields, String> {
    // The device may carry the counts in the call itself or have written them to the trip first.
    let expected = given(input, "expected_points").or_else(|| number(trip, "expected_points"));
    let last_seq = given(input, "last_seq").or_else(|| number(trip, "last_seq"));
    let (Some(expected), Some(last_seq)) = (expected, last_seq) else {
        return Err(
            "mileage_trip.end needs expected_points and last_seq, in the call or written to the trip first"
                .into(),
        );
    };
    if expected < 0 || last_seq < 0 {
        return Err("expected_points and last_seq cannot be negative".into());
    }
    let points = stored_points(db, id)?;
    let have = points.len() as i64;
    let highest = points.last().map_or(0, |p| p.seq);
    // Fewer points than the device said, and none beyond its last seq: the rest may still arrive,
    // and the same call can be repeated until it has.
    if have < expected && highest <= last_seq {
        return Err(errors::coded(
            code::INCOMPLETE,
            &format!(
                "{have} of {expected} points are stored and the highest seq is {highest} of {last_seq}; send the missing points, then end the trip again, or, if the device lost them, end it with expected_points and last_seq set to what it really holds"
            ),
        ));
    }
    // More than it said, or a seq beyond its last one, or the right count with another last seq:
    // no point that arrives can make that fit, so waiting does not help. A refusal for good, with
    // what to send instead.
    if have != expected || (have > 0 && highest != last_seq) {
        return Err(format!(
            "the trip holds {have} points and the highest seq is {highest}, which no point that arrives can make the {expected} points and last seq {last_seq} of the call; the counts are wrong. Call end again with expected_points {have} and last_seq {highest}"
        ));
    }
    let fixes: Vec<Fix> = points.iter().map(|p| p.fix).collect();
    let a = track::analyse(&fixes, &Options::default());
    let distance = a.distance_text();
    if a.first.is_none() || distance == "0.00" {
        return Err(format!(
            "the trip has no distance ({} of {have} points counted, {distance} m): discard it instead",
            a.accepted
        ));
    }
    if !fits_distance(&distance) {
        return Err(format!(
            "the computed distance of {distance} m is more than the field holds ({MAX_DISTANCE} m): the track is not a real drive; discard the trip instead"
        ));
    }
    let by_seq = |seq: i64| points.iter().find(|p| p.seq == seq);
    let (first, last) = (
        by_seq(a.first.map_or(-1, |f| f.seq)).expect("an accepted fix is a stored point"),
        by_seq(a.last.map_or(-1, |f| f.seq)).expect("an accepted fix is a stored point"),
    );
    let mut patch = Fields::new();
    patch.insert("state".into(), J::from(policy::ENDED));
    patch.insert("expected_points".into(), J::from(expected));
    patch.insert("last_seq".into(), J::from(last_seq));
    patch.insert("distance_m".into(), J::from(distance));
    patch.insert("integrity".into(), J::from(a.integrity()));
    patch.insert("start_lat".into(), J::from(first.lat.clone()));
    patch.insert("start_lon".into(), J::from(first.lon.clone()));
    patch.insert("end_lat".into(), J::from(last.lat.clone()));
    patch.insert("end_lon".into(), J::from(last.lon.clone()));
    if policy::text(trip, "ended_at").is_empty() {
        patch.insert("ended_at".into(), J::from(last.recorded_at.clone()));
    }
    Ok(patch)
}

pub(crate) fn run(
    db: &Db,
    clock: &Clock,
    action: &'static str,
    input: &Value,
) -> Result<Value, String> {
    let id = text(input, "id");
    let Some(trip) = db.get(policy::TRIP, &id)? else {
        return Err(policy::no_such(policy::TRIP, &id));
    };
    let approver = Some(text(input, "_actor_approver")).filter(|a| !a.is_empty());
    // The dispatcher has checked the group and the guard the policy; this is the same decision
    // again on the record as it is now, for the roles the call carries (a plan that was approved
    // later runs as the agent that proposed it).
    let who = Who::from_actor(&Actor::from_input(input));
    policy::check_action(action, &who, approver.as_deref(), &trip)?;
    let decider = approver.clone().unwrap_or_else(|| who.id.clone());
    let mut patch = match action {
        "end" => end(db, &id, &trip, input)?,
        _ => Fields::new(),
    };
    let to = match action {
        "end" | "reopen" => policy::ENDED,
        "submit" => policy::IN_REVIEW,
        "confirm" => policy::CONFIRMED,
        "reject" => policy::REJECTED,
        _ => policy::DISCARDED,
    };
    patch.insert("state".into(), J::from(to));
    match action {
        "submit" => {
            // A reason from an earlier round must not stand for this one.
            patch.insert("reject_reason".into(), J::Null);
        }
        "confirm" => {
            patch.insert("confirmed_at".into(), J::from(now_canonical(clock)?));
            patch.insert("reviewed_by".into(), J::from(decider.clone()));
        }
        "reject" => {
            patch.insert("reviewed_by".into(), J::from(decider.clone()));
        }
        _ => {}
    }
    let mut batch = vec![Op::Patch {
        model: policy::TRIP.into(),
        id: id.clone(),
        patch: patch.clone(),
        expect: vec![("state".into(), J::from(policy::state(&trip)))],
    }];
    if action == "discard_stuck" {
        // Who discarded it is in the log, written in the same batch as the discard (no change
        // history keeps it yet, D75 is not active): the entry exists exactly when the discard
        // does, and a refused batch leaves neither.
        batch.push(obop_commands::audit::event_op(
            u64::try_from(clock() / 1_000_000).unwrap_or(0),
            policy::text(&trip, "driver"),
            "mileage_discard_stuck",
            &decider,
            &format!(
                "discard_stuck of trip {id}, which was {}",
                policy::state(&trip)
            ),
        ));
    }
    db.apply_checked(&batch).map_err(|e| match e {
        ApplyError::Conflict { .. } => errors::coded(
            code::STATE_CONFLICT,
            &format!("the trip changed while {action} ran; nothing was changed"),
        ),
        // A store fault keeps its own words, so that every surface answers it as a 503 (D74).
        ApplyError::Storage(e) => e,
    })?;
    let mut out: HashMap<String, Value> = patch
        .iter()
        .map(|(k, v)| (k.clone(), Value::from_json(v)))
        .collect();
    out.insert("id".into(), Value::text(&id));
    Ok(Value::Map(out.into_iter().collect()))
}

/// Bind the seven actions. The guards of the registry (the `from` state, visibility) and of
/// `guards.rs` run before these.
pub fn install(d: &Dispatcher, module: &Module, db: Db, clock: Clock) {
    for action in policy::ACTIONS {
        let (db, clock) = (db.clone(), clock.clone());
        d.register_with_actor(
            module,
            policy::TRIP,
            action,
            Box::new(move |input| run(&db, &clock, action, input)),
        );
    }
}
