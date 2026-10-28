//! The models of the module: the trip and its points (docs/spec/mobile.md 5.1), the states and
//! actions of 5.2, and the row rules of 5.7.

use crate::policy::{ADMIN, DRIVER, POINT, REVIEWER, TRIP};
use obop_registry::{Access, CommandDef, FieldDef as F, FieldKind, ModelDef, Module, ModuleMeta};
use obop_types::DataClass::{ConfidentialHr, Internal};
use obop_values::Round;

/// Positions: 7 decimals of a degree are about a centimetre; the longitude has up to three digits
/// before the point.
const DEGREES: (u8, u8) = (10, 7);

pub fn module() -> Module {
    let m = Module {
        id: "mileage".into(),
        name: "Mileage".into(),
        models: vec![trip(), point()],
        network_destinations: vec![],
        description: "Trips recorded by the Mileage app, reviewed and confirmed in the back office. The server computes the distance; only confirmed trips count.".into(),
        translations: Default::default(),
        meta: ModuleMeta::first_party(
            "Recorded trips, checked on the server and confirmed by a reviewer.",
            &["people"],
            &["mileage", "trips", "kilometres"],
            "fleet",
            &[],
        ),
    };
    m.translated(&[
        include_str!("../../../../packages/labels/de/mileage.toml"),
        include_str!("../../../../packages/labels/fr/mileage.toml"),
        include_str!("../../../../packages/labels/pl/mileage.toml"),
        include_str!("../../../../packages/labels/fi/mileage.toml"),
        include_str!("../../../../packages/labels/sv/mileage.toml"),
        include_str!("../../../../packages/labels/es/mileage.toml"),
    ])
}

fn trip() -> ModelDef {
    ModelDef {
        name: TRIP,
        description: "A trip recorded by a driver: its distance, its state and the review of it",
        read_group: DRIVER,
        write_group: DRIVER,
        // The driver sees their own trips; a reviewer sees every trip from the review on (the
        // computed `reviewable`) and an administrator every trip. The guards of the module narrow
        // the second rule for a plain driver (policy.rs).
        access: Some(Access {
            own: Some("driver"),
            public_field: Some("reviewable"),
            see_all: vec![ADMIN],
            ..Access::default()
        }),
        fields: vec![
            F::new(
                "client_trip_id",
                "Identifier the app made when the driver pressed Start (a UUIDv7); unique per driver, and how the app finds its trip again",
                Internal,
                false,
                DRIVER,
            )
            .required()
            .unique_with(&["driver"]),
            F::new(
                "driver",
                "Login of the person who recorded the trip; set by the server to the caller",
                Internal,
                true,
                DRIVER,
            )
            .required(),
            F::new(
                "vehicle",
                "The vehicle the trip was made with, as the driver names it (a plate or a name)",
                Internal,
                false,
                DRIVER,
            ),
            F::new(
                "purpose",
                "What the trip was for: business, private or commute",
                Internal,
                false,
                DRIVER,
            )
            .select(&["business", "private", "commute"]),
            F::new(
                "started_at",
                "When the driver pressed Start, by the phone's clock",
                Internal,
                false,
                DRIVER,
            )
            .datetime()
            .required(),
            F::new(
                "ended_at",
                "When the driver pressed Stop; taken from the last accepted fix when the app sent none",
                Internal,
                false,
                DRIVER,
            )
            .datetime(),
            F::new(
                "distance_m",
                "Distance in metres, computed by the server from the accepted points when the trip ends; never taken from the client",
                Internal,
                false,
                DRIVER,
            )
            .decimal(12, 2),
            F::new(
                "expected_points",
                "How many points the device sent for this trip; the driver sets it before ending the trip",
                Internal,
                false,
                DRIVER,
            )
            .kind(FieldKind::Number),
            F::new(
                "last_seq",
                "The sequence number of the last point the device recorded; the driver sets it before ending the trip",
                Internal,
                false,
                DRIVER,
            )
            .kind(FieldKind::Number),
            F::new(
                "point_count",
                "How many points are stored for this trip",
                Internal,
                false,
                DRIVER,
            )
            .kind(FieldKind::Number)
            .computed("count(mileage_point)"),
            F::new(
                "max_seq",
                "The highest sequence number among the stored points",
                Internal,
                false,
                DRIVER,
            )
            .kind(FieldKind::Number)
            .computed("max(mileage_point.seq)"),
            F::new(
                "integrity",
                "Flags the validator set when the trip ended: seq_gap, speed_jump and low_accuracy with their counts; empty when the track was clean",
                Internal,
                false,
                DRIVER,
            ),
            F::new(
                "start_lat",
                "Latitude of the first accepted fix, set by the server",
                ConfidentialHr,
                true,
                DRIVER,
            )
            .decimal_rounded(DEGREES.0, DEGREES.1, Round::HalfEven),
            F::new(
                "start_lon",
                "Longitude of the first accepted fix, set by the server",
                ConfidentialHr,
                true,
                DRIVER,
            )
            .decimal_rounded(DEGREES.0, DEGREES.1, Round::HalfEven),
            F::new(
                "end_lat",
                "Latitude of the last accepted fix, set by the server; the app may stop before the final upload, so the end is not what the app reported",
                ConfidentialHr,
                true,
                DRIVER,
            )
            .decimal_rounded(DEGREES.0, DEGREES.1, Round::HalfEven),
            F::new(
                "end_lon",
                "Longitude of the last accepted fix, set by the server",
                ConfidentialHr,
                true,
                DRIVER,
            )
            .decimal_rounded(DEGREES.0, DEGREES.1, Round::HalfEven),
            F::new(
                "discard_reason",
                "Why the driver discards the trip; written before discard",
                Internal,
                false,
                DRIVER,
            )
            .kind(FieldKind::LongText),
            F::new(
                "reject_reason",
                "Why the reviewer rejects the trip; written while the trip is in review, before reject; cleared when the driver submits again",
                Internal,
                false,
                DRIVER,
            )
            .kind(FieldKind::LongText),
            F::new(
                "reviewed_by",
                "Login of the reviewer who confirmed or rejected the trip, or of the person who approved an agent's proposal to do so",
                Internal,
                true,
                DRIVER,
            ),
            F::new(
                "confirmed_at",
                "When the trip was confirmed, set by confirm; the month of this instant in the platform zone is the month the trip counts in",
                Internal,
                false,
                DRIVER,
            )
            .datetime(),
            F::new(
                "reviewable",
                "True while the trip is in review, confirmed or rejected: the states a reviewer reads",
                Internal,
                false,
                DRIVER,
            )
            .kind(FieldKind::Bool)
            .computed("state == \"in_review\" || state == \"confirmed\" || state == \"rejected\""),
            F::new(
                "state",
                "Lifecycle: recording, ended, in_review, confirmed, rejected or discarded; it moves only through the actions",
                Internal,
                false,
                DRIVER,
            )
            .select(&[
                "recording",
                "ended",
                "in_review",
                "confirmed",
                "rejected",
                "discarded",
            ]),
        ],
        commands: vec![
            CommandDef::new(
                "end",
                "End a recording trip once every point has arrived: the server computes the distance from the accepted points",
                DRIVER,
            )
            .transition("state", Some("recording"), "ended"),
            CommandDef::new("submit", "Send an ended trip to review", DRIVER).transition(
                "state",
                Some("ended"),
                "in_review",
            ),
            CommandDef::new(
                "confirm",
                "Confirm a trip in review; from now on it counts. A reviewer cannot confirm their own trip",
                REVIEWER,
            )
            .transition("state", Some("in_review"), "confirmed")
            .approval(),
            CommandDef::new(
                "reject",
                "Reject a trip in review; write reject_reason first. A reviewer cannot reject their own trip",
                REVIEWER,
            )
            .transition("state", Some("in_review"), "rejected")
            .approval(),
            CommandDef::new(
                "reopen",
                "Take a rejected trip back to ended so that the driver can correct it and submit it again",
                REVIEWER,
            )
            .transition("state", Some("rejected"), "ended"),
            CommandDef::new(
                "discard",
                "Discard your own trip that is recording, ended or rejected; write discard_reason first. A discarded trip never counts",
                DRIVER,
            )
            .transition("state", None, "discarded")
            .approval(),
            CommandDef::new(
                "discard_stuck",
                "Discard a trip that is stuck in recording or ended, as an administrator",
                ADMIN,
            )
            .transition("state", None, "discarded")
            .approval(),
        ],
        ext: Default::default(),
    }
}

fn point() -> ModelDef {
    ModelDef {
        name: POINT,
        description: "One recorded position of a trip; written once, never changed",
        read_group: DRIVER,
        write_group: DRIVER,
        // The driver sees their own points; whoever sees the trip sees its points (a reviewer
        // from the review on); an administrator sees all. The guards narrow it for a plain driver.
        access: Some(Access {
            own: Some("driver"),
            via: Some("trip"),
            see_all: vec![ADMIN],
            ..Access::default()
        }),
        fields: vec![
            F::new(
                "trip",
                "The trip the point belongs to (its server id)",
                Internal,
                false,
                DRIVER,
            )
            .reference(TRIP)
            .required(),
            F::new(
                "seq",
                "Position of the point in the trip, counted by the device; unique within the trip",
                Internal,
                false,
                DRIVER,
            )
            .kind(FieldKind::Number)
            .required()
            .unique_with(&["trip"]),
            F::new(
                "recorded_at",
                "When the phone recorded the position",
                ConfidentialHr,
                true,
                DRIVER,
            )
            .datetime()
            .required(),
            F::new(
                "lat",
                "Latitude in degrees (WGS84), at most 7 decimals",
                ConfidentialHr,
                true,
                DRIVER,
            )
            .decimal_rounded(DEGREES.0, DEGREES.1, Round::HalfEven)
            .required(),
            F::new(
                "lon",
                "Longitude in degrees (WGS84), at most 7 decimals",
                ConfidentialHr,
                true,
                DRIVER,
            )
            .decimal_rounded(DEGREES.0, DEGREES.1, Round::HalfEven)
            .required(),
            F::new(
                "accuracy_m",
                "Radius of the horizontal accuracy in metres, when the phone said",
                ConfidentialHr,
                true,
                DRIVER,
            )
            .decimal_rounded(8, 2, Round::HalfEven),
            F::new(
                "speed_mps",
                "Speed over ground in metres per second, when the phone said",
                ConfidentialHr,
                true,
                DRIVER,
            )
            .decimal_rounded(7, 2, Round::HalfEven),
            F::new(
                "driver",
                "Login of the driver of the trip; set by the server from the trip",
                Internal,
                true,
                DRIVER,
            ),
        ],
        commands: vec![],
        ext: Default::default(),
    }
}
