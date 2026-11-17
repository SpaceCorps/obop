//! The models of the module: the trip and its points (docs/spec/mobile.md 5.1), the states and
//! actions of 5.2, and the row rules of 5.7.

use crate::policy::{ADMIN, DRIVER, POINT, REVIEWER, TRIP};
use obop_registry::ui::{UiFilter, UiSection};
use obop_registry::{
    Access, CommandDef, FieldDef as F, FieldKind, Filter, ModelDef, ModelUi, Module, ModuleMeta,
};
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
        )
        .home(TRIP),
    };
    m.translated(&[
        include_str!("../../../../packages/labels/en/mileage.toml"),
        include_str!("../../../../packages/labels/de/mileage.toml"),
        include_str!("../../../../packages/labels/fr/mileage.toml"),
        include_str!("../../../../packages/labels/pl/mileage.toml"),
        include_str!("../../../../packages/labels/fi/mileage.toml"),
        include_str!("../../../../packages/labels/sv/mileage.toml"),
        include_str!("../../../../packages/labels/es/mileage.toml"),
    ])
}

// What the back office shows read-only on a saved trip (`ui.readonly_when`, R6), so that a person
// sees the rule the guards hold (`policy::writable_fields`): the server answers as it always did,
// and the phone app, which sends these fields on create, is not touched by a hint. `$user` is the
// person viewing the trip (M2), so an administrator or a reviewer is not offered the driver's inputs.

/// The fields the server takes only when the trip is created.
const FIXED: &str = "true";
/// The driver's fields: written by the driver alone while the trip is recording, ended or rejected.
const DRIVER_LOCKED: &str =
    "driver != $user || state == \"in_review\" || state == \"confirmed\" || state == \"discarded\"";
/// The reviewer's field: written while the trip is in review, by a reviewer who is not its driver.
/// (The browser does not know the viewer's groups: an administrator who is no reviewer still sees
/// it as an input on a trip in review, and the server refuses the save.)
const REVIEW_LOCKED: &str = "state != \"in_review\" || driver == $user";

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
                "Identifier the app made when the driver pressed Start (a random UUID, version 4: not a version 7, which carries the time); unique per driver, and how the app finds its trip again",
                Internal,
                false,
                DRIVER,
            )
            .required()
            .unique_with(&["driver"])
            .readonly_when(FIXED),
            F::new(
                "driver",
                "Login of the person who recorded the trip; set by the server to the caller",
                Internal,
                true,
                DRIVER,
            )
            .required()
            .readonly_when(FIXED),
            F::new(
                "vehicle",
                "The vehicle the trip was made with, as the driver names it (a plate or a name)",
                Internal,
                true,
                DRIVER,
            )
            .readonly_when(DRIVER_LOCKED),
            F::new(
                "purpose",
                "What the trip was for: business, private or commute",
                Internal,
                true,
                DRIVER,
            )
            .select(&["business", "private", "commute"])
            .readonly_when(DRIVER_LOCKED),
            F::new(
                "started_at",
                "When the driver pressed Start, by the phone's clock",
                ConfidentialHr,
                true,
                DRIVER,
            )
            .datetime()
            .required()
            .readonly_when(DRIVER_LOCKED),
            F::new(
                "ended_at",
                "When the driver pressed Stop; taken from the last accepted fix when the app sent none",
                ConfidentialHr,
                true,
                DRIVER,
            )
            .datetime()
            .readonly_when(DRIVER_LOCKED),
            F::new(
                "distance_m",
                "Distance in metres, computed by the server from the accepted points when the trip ends; never taken from the client",
                Internal,
                false,
                DRIVER,
            )
            .decimal(12, 2)
            .readonly()
            .display("km_from_m"),
            F::new(
                "expected_points",
                "How many points the device sent for this trip; the driver sets it before ending the trip",
                Internal,
                false,
                DRIVER,
            )
            .kind(FieldKind::Number)
            .readonly_when(DRIVER_LOCKED),
            F::new(
                "last_seq",
                "The sequence number of the last point the device recorded; the driver sets it before ending the trip",
                Internal,
                false,
                DRIVER,
            )
            .kind(FieldKind::Number)
            .readonly_when(DRIVER_LOCKED),
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
            )
            .readonly(),
            F::new(
                "start_lat",
                "Latitude of the first accepted fix, set by the server",
                ConfidentialHr,
                true,
                DRIVER,
            )
            .decimal_rounded(DEGREES.0, DEGREES.1, Round::HalfEven)
            .readonly(),
            F::new(
                "start_lon",
                "Longitude of the first accepted fix, set by the server",
                ConfidentialHr,
                true,
                DRIVER,
            )
            .decimal_rounded(DEGREES.0, DEGREES.1, Round::HalfEven)
            .readonly(),
            F::new(
                "end_lat",
                "Latitude of the last accepted fix, set by the server; the app may stop before the final upload, so the end is not what the app reported",
                ConfidentialHr,
                true,
                DRIVER,
            )
            .decimal_rounded(DEGREES.0, DEGREES.1, Round::HalfEven)
            .readonly(),
            F::new(
                "end_lon",
                "Longitude of the last accepted fix, set by the server",
                ConfidentialHr,
                true,
                DRIVER,
            )
            .decimal_rounded(DEGREES.0, DEGREES.1, Round::HalfEven)
            .readonly(),
            F::new(
                "discard_reason",
                "Why the driver discards the trip; written before discard",
                Internal,
                true,
                DRIVER,
            )
            .kind(FieldKind::LongText)
            .readonly_when(DRIVER_LOCKED),
            F::new(
                "reject_reason",
                "Why the reviewer rejects the trip; written while the trip is in review, before reject; cleared when the driver submits again",
                Internal,
                true,
                DRIVER,
            )
            .kind(FieldKind::LongText)
            .readonly_when(REVIEW_LOCKED),
            F::new(
                "reviewed_by",
                "Login of the reviewer who confirmed or rejected the trip, or of the person who approved an agent's proposal to do so",
                Internal,
                true,
                DRIVER,
            )
            .readonly(),
            F::new(
                "confirmed_at",
                "When the trip was confirmed, set by confirm; the month of this instant in the platform zone is the month the trip counts in",
                Internal,
                false,
                DRIVER,
            )
            .datetime()
            .readonly(),
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
            ])
            .tones(&[
                ("in_review", "info"),
                ("confirmed", "ok"),
                ("rejected", "bad"),
                ("discarded", "neutral"),
            ]),
        ],
        commands: vec![
            CommandDef::new(
                "end",
                "End a recording trip once every point has arrived: the server computes the distance from the accepted points",
                DRIVER,
            )
            .transition("state", Some("recording"), "ended"),
            CommandDef::new("submit", "Send an ended trip to review", DRIVER)
                .transition("state", Some("ended"), "in_review")
                .primary()
                .done("Sent to review"),
            CommandDef::new(
                "confirm",
                "Confirm a trip in review; from now on it counts. A reviewer cannot confirm their own trip",
                REVIEWER,
            )
            .transition("state", Some("in_review"), "confirmed")
            .approval()
            .primary()
            .not_by("driver")
            .done("Trip confirmed"),
            CommandDef::new(
                "reject",
                "Reject a trip in review; write reject_reason first. A reviewer cannot reject their own trip",
                REVIEWER,
            )
            .transition("state", Some("in_review"), "rejected")
            .approval()
            .danger()
            .needs(&["reject_reason"])
            .not_by("driver")
            .done("Trip rejected"),
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
            .approval()
            .danger()
            .needs(&["discard_reason"])
            .done("Trip discarded"),
            CommandDef::new(
                "discard_stuck",
                "Discard a trip that is stuck in recording or ended, as an administrator",
                ADMIN,
            )
            .transition("state", None, "discarded")
            .approval()
            .danger()
            .confirm("Discard this stuck trip? It will never count.")
            .done("Trip discarded"),
        ],
        ext: Default::default(),
    }
    .with_ui(ModelUi {
        // "frank.lehmann · 3 Nov 2026, 14:00 · 7.0 km": who, when and how far, the three things a
        // reviewer looks for; a trip without a distance yet (still recording) drops the last part.
        title: Some("{driver} · {started_at} · {distance_m} | {driver} · {started_at} | {driver}".into()),
        subtitle: Some("{vehicle}".into()),
        list: vec![
            "driver".into(),
            "started_at".into(),
            "vehicle".into(),
            "purpose".into(),
            "distance_m".into(),
            "state".into(),
        ],
        card: vec![
            "driver".into(),
            "started_at".into(),
            "distance_m".into(),
        ],
        order: Some("started_at desc".into()),
        default_view: Some("list".into()),
        // The reviewer's form: the trip first, then the review; what the app recorded about the
        // device (identifiers, counters, coordinates) is folded away.
        sections: vec![
            section(
                "Trip",
                &["driver", "vehicle", "purpose", "started_at", "ended_at", "distance_m"],
                false,
                &[],
            ),
            section(
                "Review",
                &["reject_reason", "reviewed_by", "confirmed_at"],
                false,
                &["in_review", "confirmed", "rejected"],
            ),
            section(
                "Discard",
                &["discard_reason"],
                false,
                &["ended", "rejected", "discarded"],
            ),
            section(
                "Recorded by the app",
                &[
                    "integrity",
                    "point_count",
                    "expected_points",
                    "last_seq",
                    "max_seq",
                    "start_lat",
                    "start_lon",
                    "end_lat",
                    "end_lon",
                    "client_trip_id",
                ],
                true,
                &[],
            ),
        ],
        filters: vec![UiFilter {
            label: "To review".into(),
            filter: Filter::parse("state = in_review").expect("a valid filter"),
        }],
        ..Default::default()
    })
}

fn section(label: &str, fields: &[&str], collapsed: bool, show_in: &[&str]) -> UiSection {
    UiSection {
        label: label.into(),
        fields: fields.iter().map(|f| (*f).into()).collect(),
        collapsed,
        show_in: show_in.iter().map(|f| (*f).into()).collect(),
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
    // The points are read through their trip (D95 children), not from the menu.
    .with_ui(ModelUi {
        menu: Some("hidden".into()),
        ..Default::default()
    })
}
