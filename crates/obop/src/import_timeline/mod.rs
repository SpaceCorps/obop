//! `obop import timeline FILE [--vehicle V] [--since YYYY-MM-DD] [--dry-run] [--allow-straight-line]`:
//! the drives in a Google Timeline export become trips of the Mileage module, as the caller of the
//! token.
//!
//! Nothing here is part of the Mileage module. Each drive goes through the commands the Mileage
//! app uses (`mileage_trip.bulk_create`, `mileage_point.bulk_create`, `mileage_trip.end`), so every
//! rule of the module holds: the driver is the caller, the server computes the distance from the
//! points, and the trip then waits in state `ended`. Which trips were for work is the person's
//! decision afterwards, in the back office or with `obop call`: set `purpose` (business, private or
//! commute), then `submit` the trip or `discard` it. The importer never sets `purpose`.
//!
//! Running it again on the same file creates nothing twice: the trip's `client_trip_id` is made
//! from the drive's own start, end and first position.

mod parse;
#[cfg(test)]
mod tests;

use super::{Exit, Opts, call_failure, fail, open_app, principal, refused_answer};
use obop_commands::{CallError, Principal, Surface, Value, Vault, errors};
use obop_values::Instant;
use parse::Trip;
use sha2::{Digest, Sha256};

/// Points per `bulk_create` call; the command takes at most 1,000 rows.
const CHUNK: usize = 500;

pub struct Args<'a> {
    pub file: &'a str,
    pub vehicle: Option<&'a str>,
    pub since: Option<&'a str>,
    pub dry_run: bool,
    pub allow_straight_line: bool,
}

/// What the server will compute for the drive: the same rules as `mileage::track`.
fn server_distance_m(t: &Trip) -> f64 {
    let fixes: Vec<mileage::track::Fix> = t
        .fixes
        .iter()
        .enumerate()
        .map(|(i, f)| mileage::track::Fix {
            seq: i as i64 + 1,
            t_ms: f.t_us as f64 / 1000.0,
            lat: f.lat,
            lon: f.lon,
            accuracy_m: None,
        })
        .collect();
    mileage::track::analyse(&fixes, &mileage::track::Options::default()).distance_m
}

fn canonical(us: i64) -> Result<String, String> {
    Instant::from_micros(us)
        .map(|i| i.canonical())
        .ok_or_else(|| "a time in the file is out of range".to_string())
}

/// A version 4 UUID made from the drive itself, so that the same drive always has the same id.
/// The module asks for a random version 4 id; this one has the version and variant bits of one
/// and, being a hash, is as spread out as a random one.
pub(crate) fn client_trip_id(t: &Trip) -> String {
    let first = t.fixes[0];
    let key = format!(
        "obop-timeline-import:{}:{}:{:.6}:{:.6}",
        t.started_us, t.ended_us, first.lat, first.lon
    );
    let digest = Sha256::digest(key.as_bytes());
    let mut b = [0u8; 16];
    b.copy_from_slice(&digest.iter().copied().take(16).collect::<Vec<u8>>());
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

fn date_of(us: i64) -> String {
    Instant::from_micros(us)
        .map(|i| i.canonical()[..10].to_string())
        .unwrap_or_default()
}

fn km(m: f64) -> String {
    format!("{:.1}", m / 1000.0)
}

/// One command call as the caller; the answer, or the exit code of the refusal (already printed).
fn step(
    app: &crate::app::App,
    p: &Principal,
    id: &str,
    input: Value,
) -> Result<Value, std::process::ExitCode> {
    let (result, seen) = errors::requested(|| {
        app.dispatcher
            .call(Surface::Cli, p, &mut Vault::new(), id, &input)
    });
    match result {
        Ok(r) => {
            if let Some(code) = refused_answer(&seen) {
                return Err(code);
            }
            Ok(r.value)
        }
        Err(CallError::Handler(m)) => Err(fail(&format!("error: {id}: {m}"))),
        Err(CallError::ApprovalRequired { plan_id }) => Err(fail(&format!(
            "error: {id} needs approval (plan {plan_id}); run the import as a person"
        ))),
        Err(
            CallError::Forbidden(_) | CallError::UnknownCommand(_) | CallError::NotOnSurface(_),
        ) => Err(fail(&format!(
            "error: this token cannot run {id} (it needs the mileage.driver group)"
        ))),
        Err(e) => Err(call_failure(&e, &seen)),
    }
}

/// Create one trip with its points and end it. `Ok(false)` when the trip was already imported.
fn import_one(
    app: &crate::app::App,
    p: &Principal,
    t: &Trip,
    vehicle: Option<&str>,
) -> Result<bool, std::process::ExitCode> {
    let cid = client_trip_id(t);
    let mut row = vec![
        ("client_trip_id", Value::text(&cid)),
        (
            "started_at",
            Value::Text(canonical(t.started_us).map_err(|m| fail(&format!("error: {m}")))?),
        ),
        (
            "ended_at",
            Value::Text(canonical(t.ended_us).map_err(|m| fail(&format!("error: {m}")))?),
        ),
    ];
    if let Some(v) = vehicle {
        row.push(("vehicle", Value::text(v)));
    }
    let row = Value::Map(row.into_iter().map(|(k, v)| (k.to_string(), v)).collect());
    let made = step(
        app,
        p,
        "mileage_trip.bulk_create",
        Value::map([
            ("rows", Value::List(vec![row])),
            ("mode", Value::text("partial")),
            ("idempotency_key", Value::text(&format!("timeline:{cid}"))),
        ]),
    )?;
    let id = match &made {
        Value::Map(m) => match m.get("ids") {
            Some(Value::List(l)) => l.iter().find_map(|v| match v {
                Value::Text(s) => Some(s.clone()),
                _ => None,
            }),
            _ => None,
        },
        _ => None,
    };
    // The drive is there already, from an earlier run: either the call is answered from the stored
    // answer of the same key (`replayed`), or the module's own uniqueness rule on `client_trip_id`
    // refused the row and no id came back. A run that was cut off between creating a trip and
    // ending it leaves that trip in state `recording`; an administrator discards it
    // (`mileage_trip.discard_stuck`) and the next run imports the drive again.
    let replayed = matches!(&made, Value::Map(m) if m.get("replayed") == Some(&Value::Bool(true)));
    let Some(id) = id.filter(|_| !replayed) else {
        return Ok(false);
    };
    for (n, chunk) in t.fixes.chunks(CHUNK).enumerate() {
        let rows: Vec<Value> = chunk
            .iter()
            .enumerate()
            .map(|(i, f)| {
                Ok(Value::map([
                    ("trip", Value::text(&id)),
                    ("seq", Value::Int((n * CHUNK + i) as i64 + 1)),
                    ("recorded_at", Value::Text(canonical(f.t_us)?)),
                    ("lat", Value::Text(format!("{:.7}", f.lat))),
                    ("lon", Value::Text(format!("{:.7}", f.lon))),
                ]))
            })
            .collect::<Result<_, String>>()
            .map_err(|m| fail(&format!("error: {m}")))?;
        step(
            app,
            p,
            "mileage_point.bulk_create",
            Value::map([
                ("rows", Value::List(rows)),
                (
                    "idempotency_key",
                    Value::text(&format!("timeline:{cid}:{n}")),
                ),
            ]),
        )?;
    }
    let total = t.fixes.len() as i64;
    step(
        app,
        p,
        "mileage_trip.end",
        Value::map([
            ("id", Value::text(&id)),
            ("expected_points", Value::Int(total)),
            ("last_seq", Value::Int(total)),
        ]),
    )?;
    Ok(true)
}

pub fn run_cli(o: &Opts, a: &Args) -> Exit {
    let text = std::fs::read_to_string(a.file)
        .map_err(|e| fail(&format!("cannot read {}: {e}", a.file)))?;
    let mut trips = parse::parse(&text).map_err(|e| fail(&format!("error: {}: {e}", a.file)))?;
    if let Some(since) = a.since {
        let from = Instant::parse(&format!("{since}T00:00:00Z"))
            .map_err(|_| fail("error: --since needs a date like 2026-10-21"))?
            .micros();
        trips.retain(|t| t.started_us >= from);
    }
    if trips.is_empty() {
        println!("no drives found in {}", a.file);
        return Ok(());
    }
    let (mut usable, mut skipped_line, mut skipped_zero) = (Vec::new(), 0, 0);
    for t in &trips {
        let d = server_distance_m(t);
        if d < 1.0 {
            skipped_zero += 1;
        } else if t.straight_line && !a.allow_straight_line {
            skipped_line += 1;
        } else {
            usable.push((t, d));
        }
    }
    println!(
        "{} drive(s) in the file, {} to import:",
        trips.len(),
        usable.len()
    );
    for (i, (t, d)) in usable.iter().enumerate() {
        let google = t
            .google_distance_m
            .map(|g| format!(", Google says {} km", km(g)))
            .unwrap_or_default();
        println!(
            "  {:>3}  {}  {} km  ({} points{}{})",
            i + 1,
            date_of(t.started_us),
            km(*d),
            t.fixes.len(),
            google,
            if t.straight_line {
                ", straight line only"
            } else {
                ""
            }
        );
    }
    if skipped_line > 0 {
        println!(
            "{skipped_line} drive(s) skipped: the file has no path for them, only the two ends, so the \
             distance would be too short (--allow-straight-line imports them anyway)"
        );
    }
    if skipped_zero > 0 {
        println!("{skipped_zero} drive(s) skipped: no distance");
    }
    if a.dry_run {
        println!("dry run: nothing was written");
        return Ok(());
    }
    let app = open_app(o)?;
    let p = principal(&app, o)?;
    let (mut created, mut already) = (0, 0);
    for (t, _) in &usable {
        if import_one(&app, &p, t, a.vehicle)? {
            created += 1;
        } else {
            already += 1;
        }
    }
    println!(
        "imported {created} trip(s){}. They are in state `ended`: set `purpose` on each one that was for work \
         and submit it, discard the rest",
        if already > 0 {
            format!(", {already} were already there")
        } else {
            String::new()
        }
    );
    Ok(())
}
