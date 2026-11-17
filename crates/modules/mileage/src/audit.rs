//! The audit of reading a route (docs/spec/mobile.md 5.6: "every read of a route is audited").
//!
//! Every `get`, `list` and `search` of `mileage_point` by a reviewer or an administrator writes to
//! the security log (`security_event`, kind `mileage_route_read`) before the answer leaves, whatever
//! the answer holds: rows, groups, or a total alone. If an entry cannot be written, the answer is
//! not given (fail closed). The user of an entry is the driver the read concerns: each driver whose
//! points the answer returned, else the driver a filter on `trip` or `driver` names, else `*` (a
//! read that names no driver and returned no row: a count of all points, or an empty page). The
//! actor is the reader. A driver reading their own points writes none, and neither does a reader
//! whose answer is only their own points.
//!
//! Which fields such a read may sort, group or total by is decided before the read, in `guards.rs`.
//!
//! The start and the end of a trip (`start_lat`, `start_lon`, `end_lat`, `end_lon`) are read in the
//! clear by the driver and by an administrator only (M6): a reviewer needs the distance, and the
//! rows of another driver's trips reach a reviewer without them. An administrator's `get`, `list`
//! or `search` that returns the start or end of another driver's trip writes the same entry, kind
//! `mileage_route_read`, for that driver, and so does one that filters by a position (a bisection
//! answers with a count); the guard refuses such a filter to a reviewer.

use crate::Clock;
use crate::policy;
use obop_commands::audit::try_record_event;
use obop_commands::{Dispatcher, PrincipalKind, Value};
use obop_storage::Backend;
use std::collections::BTreeMap;
use std::sync::Arc;

pub const KIND: &str = "mileage_route_read";

/// The user of an entry that concerns no one driver in particular.
pub const ALL_DRIVERS: &str = "*";

/// The rows a read returned, whatever the shape of the answer.
fn rows(out: &Value) -> Vec<&BTreeMap<String, Value>> {
    match out {
        Value::List(l) => l
            .iter()
            .filter_map(|v| match v {
                Value::Map(m) => Some(m),
                _ => None,
            })
            .collect(),
        Value::Map(m) => match m.get("rows") {
            Some(list @ Value::List(_)) => rows(list),
            _ if m.contains_key("seq") => vec![m],
            _ => vec![],
        },
        _ => vec![],
    }
}

/// What a filter pins with `eq`: `trip` and `driver` values.
fn pinned(input: &Value, field: &str) -> Vec<String> {
    let Value::Map(m) = input else {
        return vec![];
    };
    let Some(Value::List(filters)) = m.get("filter") else {
        return vec![];
    };
    filters
        .iter()
        .filter_map(|f| match f {
            Value::Map(f)
                if f.get("field") == Some(&Value::text(field))
                    && f.get("op") == Some(&Value::text("eq")) =>
            {
                match f.get("value") {
                    Some(Value::Text(t)) => Some(t.clone()),
                    _ => None,
                }
            }
            _ => None,
        })
        .collect()
}

/// The drivers a read concerns, with what to say about each: from the rows if there are any, else
/// from what the filter names, else everybody.
fn concerned(
    db: &dyn Backend,
    input: &Value,
    out: &Value,
    reader: &str,
    shown: &str,
) -> Vec<(String, String)> {
    // driver -> (points, trips)
    let mut per: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
    let found = rows(out);
    for r in &found {
        let driver = match r.get("driver") {
            Some(Value::Text(d)) => d.clone(),
            _ => String::new(),
        };
        let entry = per.entry(driver).or_default();
        entry.0 += 1;
        if let Some(Value::Text(t)) = r.get("trip")
            && !entry.1.contains(t)
        {
            entry.1.push(t.clone());
        }
    }
    if !found.is_empty() {
        return per
            .into_iter()
            .filter(|(driver, _)| driver != reader)
            .map(|(driver, (n, trips))| {
                (
                    driver,
                    format!("{shown} returned {n} point(s) of trip {}", trips.join(", ")),
                )
            })
            .collect();
    }
    // No row came back (a count, a group, a page past the end): the read still tells that points
    // exist and where, so it is logged for the driver the filter names, else for all.
    let trips = pinned(input, "trip");
    let mut drivers = pinned(input, "driver");
    for t in &trips {
        if let Ok(Some(trip)) = db.get(policy::TRIP, t) {
            drivers.push(policy::text(&trip, "driver").to_string());
        }
    }
    drivers.sort();
    drivers.dedup();
    if drivers.is_empty() {
        return vec![(
            ALL_DRIVERS.into(),
            format!("{shown} answered with counts or totals over the points of all drivers"),
        )];
    }
    let of = if trips.is_empty() {
        "the points of the driver".to_string()
    } else {
        format!("the points of trip {}", trips.join(", "))
    };
    drivers
        .into_iter()
        .filter(|d| d != reader)
        .map(|d| {
            (
                d,
                format!("{shown} answered with counts or totals over {of}"),
            )
        })
        .collect()
}

/// The rows of a trip answer, to change in place: a list, the `rows` of a page, or one record.
fn trip_rows(out: &mut Value) -> Vec<&mut BTreeMap<String, Value>> {
    let page = matches!(&*out, Value::Map(m) if m.contains_key("rows"));
    if page {
        return match out {
            Value::Map(m) => m.get_mut("rows").map_or_else(Vec::new, trip_rows),
            _ => vec![],
        };
    }
    match out {
        Value::List(l) => l
            .iter_mut()
            .filter_map(|v| match v {
                Value::Map(m) => Some(m),
                _ => None,
            })
            .collect(),
        Value::Map(m) if m.contains_key("driver") => vec![m],
        _ => vec![],
    }
}

/// Whether a read filters by the start or the end of a trip.
pub(crate) fn filters_by_position(input: &Value) -> bool {
    let Value::Map(m) = input else {
        return false;
    };
    let Some(Value::List(filters)) = m.get("filter") else {
        return false;
    };
    filters.iter().any(|f| match f {
        Value::Map(f) => matches!(
            f.get("field"),
            Some(Value::Text(t)) if policy::TRIP_POSITIONS.contains(&t.as_str())
        ),
        _ => false,
    })
}

/// The start and end of another driver's trip: left out for a reviewer, logged for an
/// administrator.
fn trip_reads(d: &Dispatcher, db: Arc<dyn Backend>, clock: Clock) {
    for command in ["get", "list", "search"] {
        let id = format!("{}.{command}", policy::TRIP);
        let (db, clock, shown) = (db.clone(), clock.clone(), id.clone());
        d.around(
            &id,
            Box::new(move |p, input, inner| {
                let mut out = inner(input)?;
                let who = policy::Who::of(p);
                // An agent's token never receives a position (its class is withheld from agents
                // and the answer says so, D40), so only a person's read is shaped here.
                if who.plain_driver() || p.kind != PrincipalKind::User {
                    return Ok(out);
                }
                // driver -> trips whose start or end the answer carries
                let mut read: BTreeMap<String, Vec<String>> = BTreeMap::new();
                for row in trip_rows(&mut out) {
                    let driver = match row.get("driver") {
                        Some(Value::Text(t)) => t.clone(),
                        _ => String::new(),
                    };
                    if policy::reads_positions(&who, &driver) {
                        let carries = policy::TRIP_POSITIONS
                            .iter()
                            .any(|f| !matches!(row.get(*f), None | Some(Value::Null)));
                        if driver != who.id && carries {
                            let trip = match row.get("id") {
                                Some(Value::Text(t)) => t.clone(),
                                _ => String::new(),
                            };
                            read.entry(driver).or_default().push(trip);
                        }
                    } else {
                        for f in policy::TRIP_POSITIONS {
                            row.remove(f);
                        }
                    }
                }
                let at = u64::try_from(clock() / 1_000_000).unwrap_or(0);
                let mut entries: Vec<(String, String)> = read
                    .into_iter()
                    .map(|(driver, trips)| {
                        (
                            driver,
                            format!(
                                "{shown} returned the start and end of trip {}",
                                trips.join(", ")
                            ),
                        )
                    })
                    .collect();
                if entries.is_empty() && who.admin && filters_by_position(input) {
                    let mut drivers = pinned(input, "driver");
                    drivers.retain(|d| d != &who.id);
                    if drivers.is_empty() {
                        drivers.push(ALL_DRIVERS.into());
                    }
                    for d in drivers {
                        entries.push((
                            d,
                            format!("{shown} filtered the trips by their start or end position"),
                        ));
                    }
                }
                for (driver, detail) in entries {
                    try_record_event(&*db, at, &driver, KIND, &p.id, &detail).map_err(|e| {
                        format!(
                            "the read was refused because its audit entry could not be written: {e}"
                        )
                    })?;
                }
                Ok(out)
            }),
        );
    }
}

pub fn install(d: &Dispatcher, db: Arc<dyn Backend>, clock: Clock) {
    trip_reads(d, db.clone(), clock.clone());
    for command in ["get", "list", "search"] {
        let id = format!("{}.{command}", policy::POINT);
        let (db, clock, shown) = (db.clone(), clock.clone(), id.clone());
        d.around(
            &id,
            Box::new(move |p, input, inner| {
                let out = inner(input)?;
                let who = policy::Who::of(p);
                // A plain driver is narrowed to their own points by the guard: nothing to log.
                if who.plain_driver() {
                    return Ok(out);
                }
                let at = u64::try_from(clock() / 1_000_000).unwrap_or(0);
                for (driver, detail) in concerned(&*db, input, &out, &who.id, &shown) {
                    try_record_event(&*db, at, &driver, KIND, &p.id, &detail).map_err(|e| {
                        format!(
                            "the read was refused because its audit entry could not be written: {e}"
                        )
                    })?;
                }
                Ok(out)
            }),
        );
    }
}
