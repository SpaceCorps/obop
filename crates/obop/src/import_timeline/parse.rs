//! Reads the Timeline file that Google Maps exports from a phone (Settings, Timeline, "Export
//! Timeline data") and finds the drives in it. Pure: no I/O, no server, so the rules are tested on
//! small files made by hand (`tests.rs`).
//!
//! Only the on-device export format is read: a JSON object with `semanticSegments`. A segment
//! with an `activity` is a movement between two places; one with a `timelinePath` is the path the
//! phone recorded in that time. Which other formats Google may write, and how this one changes,
//! is not under our control: a file this reader does not know is refused with a message, never
//! guessed at.

use obop_values::Instant;
use serde_json::Value as J;

/// The activity types that are a drive in a car or a motorcycle. Anything else (walking, cycling,
/// a train, a flight) is not a trip for mileage.
pub const VEHICLE_TYPES: [&str; 3] = ["IN_PASSENGER_VEHICLE", "IN_VEHICLE", "DRIVING"];

/// One recorded position. `t_us` is microseconds since the Unix epoch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fix {
    pub t_us: i64,
    pub lat: f64,
    pub lon: f64,
}

/// One drive found in the file.
#[derive(Debug, Clone, PartialEq)]
pub struct Trip {
    pub started_us: i64,
    pub ended_us: i64,
    /// Positions in time order, the start and the end of the drive included.
    pub fixes: Vec<Fix>,
    /// True when the file holds no path for the drive, only its two ends: the distance would be
    /// the straight line between them, which is less than the road.
    pub straight_line: bool,
    /// The distance Google wrote for the drive, for comparison. The server never uses it.
    pub google_distance_m: Option<f64>,
}

/// `"59.3293000°, 18.0686000°"` to a pair of degrees.
fn latlng(v: &J) -> Option<(f64, f64)> {
    let s = v.as_str()?;
    let (a, b) = s.split_once(',')?;
    let num = |t: &str| t.trim().trim_end_matches('°').trim().parse::<f64>().ok();
    let (lat, lon) = (num(a)?, num(b)?);
    (lat.abs() <= 90.0 && lon.abs() <= 180.0).then_some((lat, lon))
}

fn instant(v: &J) -> Option<i64> {
    Instant::parse(v.as_str()?).ok().map(|i| i.micros())
}

/// Every position of every `timelinePath`, in time order.
fn path_fixes(segments: &[J]) -> Vec<Fix> {
    let mut out = Vec::new();
    for seg in segments {
        let Some(path) = seg.get("timelinePath").and_then(J::as_array) else {
            continue;
        };
        for p in path {
            if let (Some(t_us), Some((lat, lon))) = (
                p.get("time").and_then(instant),
                p.get("point").and_then(latlng),
            ) {
                out.push(Fix { t_us, lat, lon });
            }
        }
    }
    out.sort_by_key(|f| f.t_us);
    out
}

/// The drives of a Timeline export, oldest first. An error says why the text is not a file this
/// reader knows.
pub fn parse(text: &str) -> Result<Vec<Trip>, String> {
    let root: J = serde_json::from_str(text).map_err(|e| format!("not valid JSON: {e}"))?;
    let Some(segments) = root.get("semanticSegments").and_then(J::as_array) else {
        return Err(
            "this is not a Timeline export from the phone: there is no `semanticSegments` list. \
             Export it in Google Maps (Timeline, the cloud icon, Export Timeline data)"
                .into(),
        );
    };
    let path = path_fixes(segments);
    let mut trips = Vec::new();
    for seg in segments {
        let Some(act) = seg.get("activity") else {
            continue;
        };
        let kind = act
            .pointer("/topCandidate/type")
            .and_then(J::as_str)
            .unwrap_or("");
        if !VEHICLE_TYPES.contains(&kind) {
            continue;
        }
        let (Some(started_us), Some(ended_us)) = (
            seg.get("startTime").and_then(instant),
            seg.get("endTime").and_then(instant),
        ) else {
            continue;
        };
        if ended_us <= started_us {
            continue;
        }
        let from = act.pointer("/start/latLng").and_then(latlng);
        let to = act.pointer("/end/latLng").and_then(latlng);
        let mut fixes: Vec<Fix> = path
            .iter()
            .filter(|f| f.t_us >= started_us && f.t_us <= ended_us)
            .copied()
            .collect();
        let straight_line = fixes.is_empty();
        if let Some((lat, lon)) = from {
            fixes.push(Fix {
                t_us: started_us,
                lat,
                lon,
            });
        }
        if let Some((lat, lon)) = to {
            fixes.push(Fix {
                t_us: ended_us,
                lat,
                lon,
            });
        }
        fixes.sort_by_key(|f| f.t_us);
        fixes.dedup_by_key(|f| f.t_us);
        if fixes.len() < 2 {
            continue;
        }
        trips.push(Trip {
            started_us,
            ended_us,
            fixes,
            straight_line,
            google_distance_m: act.get("distanceMeters").and_then(J::as_f64),
        });
    }
    trips.sort_by_key(|t| t.started_us);
    Ok(trips)
}
