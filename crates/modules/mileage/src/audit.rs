//! The audit of reading a route (docs/spec/mobile.md 5.6: "every read of a route is audited").
//!
//! A read of points (`get`, `list`, `search` of `mileage_point`) writes one entry per driver whose
//! points it returned to the security log (`security_event`, kind `mileage_route_read`): the user
//! is the driver, the actor the reader; a driver reading their own points writes none. The entry is written after the read and before the answer
//! leaves: if it cannot be written, the answer is not given (fail closed). A call that returned
//! no point writes nothing, and neither does an aggregate, which returns counts and no position.

use crate::Clock;
use crate::policy;
use obop_commands::audit::try_record_event;
use obop_commands::{Dispatcher, Value};
use obop_storage::Backend;
use std::collections::BTreeMap;
use std::sync::Arc;

pub const KIND: &str = "mileage_route_read";

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

pub fn install(d: &Dispatcher, db: Arc<dyn Backend>, clock: Clock) {
    for command in ["get", "list", "search"] {
        let id = format!("{}.{command}", policy::POINT);
        let (db, clock, shown) = (db.clone(), clock.clone(), id.clone());
        d.around(
            &id,
            Box::new(move |p, input, inner| {
                let out = inner(input)?;
                // driver -> (points, trips)
                let mut per: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
                for r in rows(&out) {
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
                let at = u64::try_from(clock() / 1_000_000).unwrap_or(0);
                let reader = policy::Who::of(p).id;
                for (driver, (n, trips)) in per {
                    if driver == reader {
                        continue;
                    }
                    try_record_event(
                        &*db,
                        at,
                        &driver,
                        KIND,
                        &p.id,
                        &format!("{shown} returned {n} point(s) of trip {}", trips.join(", ")),
                    )
                    .map_err(|e| {
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
