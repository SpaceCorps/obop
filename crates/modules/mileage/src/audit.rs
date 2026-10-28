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

use crate::Clock;
use crate::policy;
use obop_commands::audit::try_record_event;
use obop_commands::{Dispatcher, Value};
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

pub fn install(d: &Dispatcher, db: Arc<dyn Backend>, clock: Clock) {
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
