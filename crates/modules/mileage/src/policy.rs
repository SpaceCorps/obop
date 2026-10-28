//! Who may do what to a trip and to its points (docs/spec/mobile.md 5.2 and 5.7), as pure
//! decisions over a stored record. The guards and the handlers of the module call these and
//! nothing else decides, so the table in the spec and the code can be compared line by line.
//!
//! Groups. `mileage.driver` opens the module and is what the app's access token holds. A reviewer
//! and an administrator hold it as well, the way `invoicing.manager` is given on top of
//! `invoicing.user`: the platform lets one command name one group, and `get`, `list` and `update`
//! of a trip must be callable by all three roles. What a role may then do is decided here.

use obop_commands::Principal;
use serde_json::Value as J;

pub type Fields = serde_json::Map<String, J>;

pub const DRIVER: &str = "mileage.driver";
pub const REVIEWER: &str = "mileage.reviewer";
pub const ADMIN: &str = "mileage.admin";

pub const TRIP: &str = "mileage_trip";
pub const POINT: &str = "mileage_point";

pub const RECORDING: &str = "recording";
pub const ENDED: &str = "ended";
pub const IN_REVIEW: &str = "in_review";
pub const CONFIRMED: &str = "confirmed";
pub const REJECTED: &str = "rejected";
pub const DISCARDED: &str = "discarded";
pub const STATES: [&str; 6] = [RECORDING, ENDED, IN_REVIEW, CONFIRMED, REJECTED, DISCARDED];

/// The actions of a trip, in the order of the spec's table (5.2).
pub const ACTIONS: [&str; 7] = [
    "end",
    "submit",
    "confirm",
    "reject",
    "reopen",
    "discard",
    "discard_stuck",
];

/// Fields the driver writes, by `update` (or at creation), while the trip is in a state where the
/// driver may still edit it.
pub const DRIVER_FIELDS: [&str; 7] = [
    "purpose",
    "vehicle",
    "started_at",
    "ended_at",
    "expected_points",
    "last_seq",
    "discard_reason",
];

/// What a trip row may carry when it is created: the fields the device knows at the start.
pub const CREATE_FIELDS: [&str; 8] = [
    "client_trip_id",
    "driver",
    "purpose",
    "vehicle",
    "started_at",
    "ended_at",
    "expected_points",
    "last_seq",
];

/// Fields only the reviewer writes, and only while the trip is in review.
pub const REVIEWER_FIELDS: [&str; 1] = ["reject_reason"];

/// Computed fields: the generic `update` accepts them back unchanged and refuses anything else.
pub const COMPUTED: [&str; 3] = ["point_count", "max_seq", "reviewable"];

/// What a point row may carry. `driver` is filled in by the server.
pub const POINT_FIELDS: [&str; 8] = [
    "trip",
    "seq",
    "recorded_at",
    "lat",
    "lon",
    "accuracy_m",
    "speed_mps",
    "driver",
];

/// Most points one trip may hold. A guard against a client that never stops, not a product limit:
/// a three-hour trip at one fix every five seconds is 2,160.
pub const MAX_POINTS_PER_TRIP: usize = 20_000;

/// Whether a trip that holds `have` points has room for `adding` more.
pub fn room_for(have: usize, adding: usize) -> Result<(), String> {
    if have + adding > MAX_POINTS_PER_TRIP {
        return Err(format!(
            "a trip holds at most {MAX_POINTS_PER_TRIP} points; this one has {have} and {adding} more were sent"
        ));
    }
    Ok(())
}

/// The states a reviewer reads (5.7: "from `in_review` on"). A trip that was discarded after
/// review is not among them: the reviewer saw it when it was in review or rejected.
pub fn reviewable(state: &str) -> bool {
    matches!(state, IN_REVIEW | CONFIRMED | REJECTED)
}

/// The person a call acts for and the roles they act in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Who {
    /// The login; for an agent, the login of the person it acts for.
    pub id: String,
    pub reviewer: bool,
    pub admin: bool,
}

impl Who {
    pub fn of(p: &Principal) -> Self {
        Self {
            id: p.id.strip_suffix("/agent").unwrap_or(&p.id).to_string(),
            reviewer: p.allows(REVIEWER),
            admin: p.allows(ADMIN),
        }
    }

    /// A driver and nothing more: sees only their own trips and points.
    pub fn plain_driver(&self) -> bool {
        !self.reviewer && !self.admin
    }
}

pub fn text<'a>(rec: &'a Fields, k: &str) -> &'a str {
    rec.get(k).and_then(J::as_str).unwrap_or("")
}

pub fn state(trip: &Fields) -> &str {
    text(trip, "state")
}

pub fn owns(who: &Who, trip: &Fields) -> bool {
    !who.id.is_empty() && text(trip, "driver") == who.id
}

/// Whether `who` may read this trip. The row rule of the model lets every holder of the base
/// group see a reviewable trip (its `public_field`), so the narrowing for a plain driver is made
/// here, in every guard that reads a trip.
pub fn may_see_trip(who: &Who, trip: &Fields) -> bool {
    who.admin || owns(who, trip) || (who.reviewer && reviewable(state(trip)))
}

/// The refusal for a record the caller may not see: the words of a record that does not exist.
pub fn no_such(model: &str, id: &str) -> String {
    format!("no {model} {id}")
}

fn word(action: &str) -> &str {
    match action {
        "discard_stuck" => "discard a stuck",
        a => a,
    }
}

/// Whether `who` may run `action` on `trip`, and why not. `approver` is the person who approved
/// an agent's proposal, if this is the run of one: the person who decides is then the approver.
/// Group membership of the action itself is the dispatcher's check; this is state, ownership and
/// separation of duties.
pub fn check_action(
    action: &str,
    who: &Who,
    approver: Option<&str>,
    trip: &Fields,
) -> Result<(), String> {
    let st = state(trip);
    let decider = approver.unwrap_or(&who.id);
    let mine = owns(who, trip);
    let allowed_from: &[&str] = match action {
        "end" => &[RECORDING],
        "submit" => &[ENDED],
        "confirm" | "reject" => &[IN_REVIEW],
        "reopen" => &[REJECTED],
        "discard" => &[RECORDING, ENDED, REJECTED],
        "discard_stuck" => &[RECORDING, ENDED],
        other => return Err(format!("{TRIP}.{other} is not an action of a trip")),
    };
    match action {
        "end" | "submit" | "discard" if !mine => {
            return Err(format!("only the driver of a trip can {} it", word(action)));
        }
        "discard_stuck" if !who.admin => {
            return Err("only a mileage administrator can discard a stuck trip".into());
        }
        "confirm" | "reject" | "reopen" => {
            if !who.reviewer {
                return Err(format!("only a reviewer can {action} a trip"));
            }
            if text(trip, "driver") == decider {
                return Err(format!(
                    "a reviewer cannot {action} their own trip: another reviewer must"
                ));
            }
        }
        _ => {}
    }
    if !allowed_from.contains(&st) {
        return Err(format!(
            "cannot {} a trip that is {st}: it needs to be {}",
            word(action),
            allowed_from.join(" or ")
        ));
    }
    match action {
        "reject" if text(trip, "reject_reason").trim().is_empty() => {
            Err("reject needs a reason: write reject_reason with update first".into())
        }
        "discard" if text(trip, "discard_reason").trim().is_empty() => {
            Err("discard needs a reason: write discard_reason with update first".into())
        }
        _ => Ok(()),
    }
}

/// The fields `who` may change on `trip` by `update`, or why they may not. The caller has
/// already been shown that they may see the trip.
pub fn writable_fields(who: &Who, trip: &Fields) -> Result<&'static [&'static str], String> {
    let st = state(trip);
    if owns(who, trip) {
        return if matches!(st, RECORDING | ENDED | REJECTED) {
            Ok(&DRIVER_FIELDS)
        } else {
            Err(format!(
                "a trip that is {st} cannot be edited; the driver edits it while it is recording, ended or rejected"
            ))
        };
    }
    if who.reviewer && st == IN_REVIEW {
        return Ok(&REVIEWER_FIELDS);
    }
    // The caller has already been shown that they may see the trip (`may_see_trip`).
    Err(
        "this trip cannot be edited by you: a reviewer writes reject_reason while it is in review, and the rest is the driver's".into(),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_trip_holds_a_bounded_number_of_points() {
        use super::{MAX_POINTS_PER_TRIP as TOP, room_for};
        assert!(room_for(0, TOP).is_ok());
        assert!(room_for(TOP - 5, 5).is_ok());
        assert!(room_for(TOP - 5, 6).unwrap_err().contains("at most"));
    }

    use super::*;
    use serde_json::json;

    fn trip(state: &str, driver: &str) -> Fields {
        json!({"id": "t1", "state": state, "driver": driver, "reject_reason": "r", "discard_reason": "d"})
            .as_object()
            .unwrap()
            .clone()
    }

    fn who(id: &str, reviewer: bool, admin: bool) -> Who {
        Who {
            id: id.into(),
            reviewer,
            admin,
        }
    }

    #[test]
    fn the_state_table_is_the_spec_table() {
        // (action, state, caller is the driver, caller is a reviewer, caller is an admin, ok)
        let driver = who("ann", false, false);
        let reviewer = who("rob", true, false);
        let admin = who("ada", false, true);
        for action in ACTIONS {
            for st in STATES {
                let t = trip(st, "ann");
                let by = |w: &Who| check_action(action, w, None, &t).is_ok();
                let (from_ok, who_ok): (bool, [bool; 3]) = match action {
                    "end" => (st == RECORDING, [true, false, false]),
                    "submit" => (st == ENDED, [true, false, false]),
                    "confirm" | "reject" => (st == IN_REVIEW, [false, true, false]),
                    "reopen" => (st == REJECTED, [false, true, false]),
                    "discard" => (
                        matches!(st, RECORDING | ENDED | REJECTED),
                        [true, false, false],
                    ),
                    "discard_stuck" => (matches!(st, RECORDING | ENDED), [false, false, true]),
                    _ => unreachable!(),
                };
                for (w, who_ok, name) in [
                    (&driver, who_ok[0], "driver"),
                    (&reviewer, who_ok[1], "reviewer"),
                    (&admin, who_ok[2], "admin"),
                ] {
                    assert_eq!(by(w), from_ok && who_ok, "{action} in {st} by the {name}");
                }
            }
        }
    }

    #[test]
    fn a_reviewer_cannot_decide_their_own_trip_whoever_approves_the_proposal() {
        let own = trip(IN_REVIEW, "rob");
        let rob = who("rob", true, false);
        for action in ["confirm", "reject"] {
            let e = check_action(action, &rob, None, &own).unwrap_err();
            assert!(e.contains("their own trip"), "{e}");
        }
        // An agent of another reviewer proposes; the person who approves is the driver.
        let other = who("sam", true, false);
        let e = check_action("confirm", &other, Some("rob"), &own).unwrap_err();
        assert!(e.contains("their own trip"), "{e}");
        assert!(check_action("confirm", &other, Some("tim"), &own).is_ok());
    }

    #[test]
    fn discard_and_reject_need_their_reason() {
        let mut t = trip(RECORDING, "ann");
        t.insert("discard_reason".into(), json!("  "));
        let e = check_action("discard", &who("ann", false, false), None, &t).unwrap_err();
        assert!(e.contains("discard_reason"), "{e}");
        let mut t = trip(IN_REVIEW, "ann");
        t.remove("reject_reason");
        let e = check_action("reject", &who("rob", true, false), None, &t).unwrap_err();
        assert!(e.contains("reject_reason"), "{e}");
    }

    #[test]
    fn who_writes_what_and_when() {
        let driver = who("ann", false, false);
        let reviewer = who("rob", true, false);
        let admin = who("ada", false, true);
        for st in STATES {
            let t = trip(st, "ann");
            let d = writable_fields(&driver, &t);
            assert_eq!(
                d.is_ok(),
                matches!(st, RECORDING | ENDED | REJECTED),
                "driver in {st}"
            );
            let r = writable_fields(&reviewer, &t);
            assert_eq!(r.is_ok(), st == IN_REVIEW, "reviewer in {st}");
            assert!(writable_fields(&admin, &t).is_err(), "admin in {st}");
        }
        // A reviewer who is the driver writes as the driver and never as the reviewer.
        let rob = trip(IN_REVIEW, "rob");
        assert!(writable_fields(&reviewer, &rob).is_err());
        assert_eq!(
            writable_fields(&reviewer, &trip(ENDED, "rob")).unwrap(),
            &DRIVER_FIELDS
        );
    }

    #[test]
    fn a_trip_is_seen_by_its_driver_by_a_reviewer_from_review_on_and_by_an_admin() {
        let driver = who("ann", false, false);
        let other = who("bob", false, false);
        let reviewer = who("rob", true, false);
        let admin = who("ada", false, true);
        for st in STATES {
            let t = trip(st, "ann");
            assert!(may_see_trip(&driver, &t));
            assert!(!may_see_trip(&other, &t), "another driver in {st}");
            assert_eq!(
                may_see_trip(&reviewer, &t),
                reviewable(st),
                "reviewer in {st}"
            );
            assert!(may_see_trip(&admin, &t));
        }
        assert!(!reviewable(DISCARDED) && !reviewable(RECORDING) && !reviewable(ENDED));
    }
}
