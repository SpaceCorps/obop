//! The Timeline importer: the reader on small files made by hand (invented places and times, no
//! real data), and the whole import against a scratch SQLite database through the real commands of
//! the Mileage module.

use super::parse::{self, VEHICLE_TYPES};
use super::{Args, client_trip_id, run_cli};
use crate::Opts;
use obop_commands::Store;
use serde_json::{Value as J, json};

/// A path of `n` points going north from Stockholm, one every minute from `start` (a UTC minute
/// of 2026-10-27, 08:00 plus `start`).
fn path(start: i64, n: i64) -> Vec<J> {
    (0..n)
        .map(|i| {
            let m = start + i;
            json!({
                "point": format!("{:.7}°, 18.0000000°", 59.30 + 0.01 * i as f64),
                "time": format!("2026-10-27T{:02}:{:02}:00.000+01:00", 9 + m / 60, m % 60),
            })
        })
        .collect()
}

fn drive(start: &str, end: &str, kind: &str, from: &str, to: &str, meters: f64) -> J {
    json!({
        "startTime": start,
        "endTime": end,
        "activity": {
            "start": {"latLng": from},
            "end": {"latLng": to},
            "distanceMeters": meters,
            "topCandidate": {"type": kind, "probability": 0.9},
        },
    })
}

/// One drive with a recorded path (about 11 km), one walk, and one drive with only its two ends.
fn sample() -> String {
    json!({
        "semanticSegments": [
            drive(
                "2026-10-27T09:00:00.000+01:00", "2026-10-27T09:10:00.000+01:00",
                "IN_PASSENGER_VEHICLE", "59.3000000°, 18.0000000°", "59.3900000°, 18.0000000°", 10_100.0,
            ),
            {
                "startTime": "2026-10-27T09:00:00.000+01:00",
                "endTime": "2026-10-27T09:10:00.000+01:00",
                "timelinePath": path(0, 10),
            },
            drive(
                "2026-10-27T11:00:00.000+01:00", "2026-10-27T11:20:00.000+01:00",
                "WALKING", "59.3000000°, 18.0000000°", "59.3100000°, 18.0000000°", 1_100.0,
            ),
            drive(
                "2026-10-28T08:00:00.000+01:00", "2026-10-28T08:30:00.000+01:00",
                "IN_PASSENGER_VEHICLE", "59.3000000°, 18.0000000°", "59.4000000°, 18.1000000°", 15_000.0,
            ),
        ],
        "rawSignals": [],
    })
    .to_string()
}

#[test]
fn finds_the_drives_and_nothing_else() {
    let trips = parse::parse(&sample()).unwrap();
    assert_eq!(trips.len(), 2, "the walk is not a trip");
    // Oldest first.
    assert!(trips[0].started_us < trips[1].started_us);
    // The first has its path (ten points) and the two ends: 10 path points, the start and the end
    // are among them or added, never lost.
    assert!(!trips[0].straight_line);
    assert!(trips[0].fixes.len() >= 10);
    assert_eq!(trips[0].google_distance_m, Some(10_100.0));
    // The second has no path, only its two ends.
    assert!(trips[1].straight_line);
    assert_eq!(trips[1].fixes.len(), 2);
    assert!(VEHICLE_TYPES.contains(&"IN_PASSENGER_VEHICLE"));
}

#[test]
fn refuses_what_is_not_a_timeline_export() {
    assert!(parse::parse("not json").is_err());
    let e = parse::parse(r#"{"locations": []}"#).unwrap_err();
    assert!(e.contains("semanticSegments"), "{e}");
}

#[test]
fn a_position_out_of_range_is_not_a_fix() {
    let text = json!({"semanticSegments": [drive(
        "2026-10-27T09:00:00Z", "2026-10-27T09:10:00Z",
        "IN_PASSENGER_VEHICLE", "95.0°, 18.0°", "59.0°, 200.0°", 1.0,
    )]})
    .to_string();
    // Neither end is a position, so the drive has nothing to import.
    assert!(parse::parse(&text).unwrap().is_empty());
}

#[test]
fn the_trip_id_is_a_version_4_uuid_and_does_not_change() {
    let trips = parse::parse(&sample()).unwrap();
    let id = client_trip_id(&trips[0]);
    assert_eq!(id, client_trip_id(&trips[0]));
    assert_ne!(id, client_trip_id(&trips[1]));
    let parts: Vec<&str> = id.split('-').collect();
    assert_eq!(
        parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
        [8, 4, 4, 4, 12]
    );
    assert!(parts[2].starts_with('4'), "version 4: {id}");
    assert!(
        matches!(parts[3].chars().next(), Some('8' | '9' | 'a' | 'b')),
        "variant: {id}"
    );
}

fn scratch(name: &str) -> (std::path::PathBuf, String) {
    let dir =
        std::env::temp_dir().join(format!("obop-timeline-test-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("Timeline.json");
    std::fs::write(&file, sample()).unwrap();
    let url = format!("sqlite://{}", dir.join("obop.db").display());
    (dir, url)
}

fn opts(url: &str) -> Opts {
    Opts {
        db: Some(url.to_string()),
        token: Some("dev-admin".into()),
        demo: false,
        dev_auth: true,
        public_site: false,
        trust_proxy: false,
        admin_prefix: String::new(),
    }
}

fn ended_trips(url: &str) -> Vec<(String, serde_json::Map<String, J>)> {
    let store = Store::open(url).unwrap();
    store
        .backend()
        .find("mileage_trip", "state", &J::from("ended"))
        .unwrap()
}

#[test]
fn a_dry_run_writes_nothing() {
    let (dir, url) = scratch("dry");
    let file = dir.join("Timeline.json");
    let a = Args {
        file: file.to_str().unwrap(),
        vehicle: None,
        since: None,
        dry_run: true,
        allow_straight_line: false,
    };
    assert!(run_cli(&opts(&url), &a).is_ok());
    // The database was never opened for writing: no trip, and not even a file.
    assert!(!dir.join("obop.db").exists());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn imports_the_drives_with_a_path_once() {
    let (dir, url) = scratch("import");
    let file = dir.join("Timeline.json");
    let a = Args {
        file: file.to_str().unwrap(),
        vehicle: Some("Leased car"),
        since: None,
        dry_run: false,
        allow_straight_line: false,
    };
    assert!(run_cli(&opts(&url), &a).is_ok());
    let trips = ended_trips(&url);
    // The drive with no path is skipped, the walk is not a trip.
    assert_eq!(trips.len(), 1, "{trips:?}");
    let t = &trips[0].1;
    assert_eq!(t.get("vehicle").and_then(J::as_str), Some("Leased car"));
    // The purpose is left for the person to decide.
    assert!(t.get("purpose").is_none_or(J::is_null));
    // The server computed the distance: about 10 km (nine steps of 0.01 degree north).
    let d: f64 = t["distance_m"].as_str().unwrap().parse().unwrap();
    assert!((9_000.0..11_500.0).contains(&d), "{d}");
    // A second run on the same file creates nothing twice.
    assert!(run_cli(&opts(&url), &a).is_ok());
    assert_eq!(ended_trips(&url).len(), 1);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn since_leaves_earlier_drives_out() {
    let (dir, url) = scratch("since");
    let file = dir.join("Timeline.json");
    let a = Args {
        file: file.to_str().unwrap(),
        vehicle: None,
        since: Some("2026-10-28"),
        dry_run: false,
        allow_straight_line: true,
    };
    assert!(run_cli(&opts(&url), &a).is_ok());
    // Only the 28th, which has no path and is imported because the straight line is allowed.
    assert_eq!(ended_trips(&url).len(), 1);
    std::fs::remove_dir_all(dir).ok();
}
