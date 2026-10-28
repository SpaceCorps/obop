//! Who may do what to a trip and to its points (docs/spec/mobile.md 5.2 and 5.7), as pure
//! decisions over a stored record. The guards and the handlers of the module call these and
//! nothing else decides, so the table in the spec and the code can be compared line by line.
//!
//! Groups. `mileage.driver` opens the module and is what the app's access token holds. A reviewer
//! and an administrator hold it as well, the way `invoicing.manager` is given on top of
//! `invoicing.user`: the platform lets one command name one group, and `get`, `list` and `update`
//! of a trip must be callable by all three roles. What a role may then do is decided here, and a
//! role counts only for a principal that holds `mileage.driver` too (`Who::of`): a reviewer or an
//! administrator account without it can read nothing, so it decides nothing either.

use obop_commands::{Principal, Value};
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

/// The cap is kept by the range of `seq`, not by counting the stored points: (`trip`, `seq`) is
/// unique, so a trip whose points have a `seq` from 1 to 20,000 cannot hold more than 20,000, and
/// the guard of a batch looks at the batch alone (a count would read every stored point of the
/// instance, under the lock of the call).
pub fn check_seq(seq: i64) -> Result<(), String> {
    if (1..=MAX_POINTS_PER_TRIP as i64).contains(&seq) {
        return Ok(());
    }
    Err(format!(
        "mileage_point.seq must be a whole number from 1 to {MAX_POINTS_PER_TRIP}: a trip holds at most {MAX_POINTS_PER_TRIP} points"
    ))
}

/// The counts a device says at the end of a trip (`expected_points`, `last_seq`): a whole number
/// from 0 to the cap. A trip cannot hold more, so a larger number could never be met, and an
/// `end` that waited for it would wait for ever.
pub fn check_count(field: &str, v: &Value) -> Result<(), String> {
    if !matches!(field, "expected_points" | "last_seq") {
        return Ok(());
    }
    match v {
        Value::Int(n) if !(0..=MAX_POINTS_PER_TRIP as i64).contains(n) => Err(format!(
            "mileage_trip.{field} must be a whole number from 0 to {MAX_POINTS_PER_TRIP}: a trip holds at most {MAX_POINTS_PER_TRIP} points"
        )),
        _ => Ok(()),
    }
}

/// The fields of a point that say where and when the person was: a reviewer or an administrator
/// reads them point by point, with the read logged, and cannot sort, group or total by them.
pub const POINT_POSITIONS: [&str; 5] = ["lat", "lon", "accuracy_m", "speed_mps", "recorded_at"];

/// The same for the start and the end of a trip.
pub const TRIP_POSITIONS: [&str; 4] = ["start_lat", "start_lon", "end_lat", "end_lon"];

/// The states a reviewer reads (5.7: "from `in_review` on"). A trip that was discarded after
/// review is not among them: the reviewer saw it when it was in review or rejected.
pub fn reviewable(state: &str) -> bool {
    matches!(state, IN_REVIEW | CONFIRMED | REJECTED)
}

const KEY_REVIEWER: &str = "_actor_mileage_reviewer";
const KEY_ADMIN: &str = "_actor_mileage_admin";

/// The person who approved an agent's proposal, as the dispatcher stamps them into the input of
/// the guard and of the handler (`_actor_approver*`): a role counts for them only on top of the
/// base group as it does for a caller, or an account with only the role could decide a trip by
/// approving what an agent proposed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Approver {
    pub who: Who,
    /// Holds `mileage.driver`, the base group.
    pub driver: bool,
}

impl Approver {
    /// `None` when the call is not the run of an approved plan.
    pub fn of(input: &Value) -> Option<Self> {
        let Value::Map(m) = input else {
            return None;
        };
        let id = match m.get("_actor_approver") {
            Some(Value::Text(t)) if !t.is_empty() => t.clone(),
            _ => return None,
        };
        let platform_admin = m.get("_actor_approver_admin") == Some(&Value::Bool(true));
        let groups: Vec<&str> = match m.get("_actor_approver_groups") {
            Some(Value::List(l)) => l
                .iter()
                .filter_map(|v| match v {
                    Value::Text(t) => Some(t.as_str()),
                    _ => None,
                })
                .collect(),
            _ => vec![],
        };
        let holds = |g: &str| platform_admin || groups.contains(&g);
        let driver = holds(DRIVER);
        Some(Self {
            who: Who {
                id,
                reviewer: driver && holds(REVIEWER),
                admin: driver && holds(ADMIN),
            },
            driver,
        })
    }

    /// Whether the approver may stand behind `action` of the trip `id`. The dispatcher has checked
    /// the group of the command; this is the base group and the role, answered for a missing
    /// base group as for a trip that does not exist.
    pub fn check(&self, action: &str, id: &str) -> Result<(), String> {
        if !self.driver {
            return Err(no_such(TRIP, id));
        }
        match action {
            "confirm" | "reject" | "reopen" if !self.who.reviewer => {
                Err(format!("only a reviewer can {action} a trip"))
            }
            "discard_stuck" if !self.who.admin => {
                Err("only a mileage administrator can discard a stuck trip".into())
            }
            _ => Ok(()),
        }
    }
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
    /// The roles of a principal. Reviewer and administrator count only on top of the base group.
    pub fn of(p: &Principal) -> Self {
        let driver = p.allows(DRIVER);
        Self {
            id: p.id.strip_suffix("/agent").unwrap_or(&p.id).to_string(),
            reviewer: driver && p.allows(REVIEWER),
            admin: driver && p.allows(ADMIN),
        }
    }

    /// The same, as the guard of an action decided it for the principal and stamped it into the
    /// input of the handler (`stamp_roles`): the one decision (`Who::of`) is made in one place,
    /// where the principal is known, and the handler uses it as it is. Without the stamp (a
    /// handler called outside the dispatcher) the caller is a plain driver. The id is the one the
    /// dispatcher stamps for a handler registered with `register_with_actor`.
    pub fn from_input(input: &Value) -> Self {
        let Value::Map(m) = input else {
            return Self {
                id: String::new(),
                reviewer: false,
                admin: false,
            };
        };
        let flag = |k: &str| m.get(k) == Some(&Value::Bool(true));
        Self {
            id: match m.get("_actor") {
                Some(Value::Text(t)) => t.clone(),
                _ => String::new(),
            },
            reviewer: flag(KEY_REVIEWER),
            admin: flag(KEY_ADMIN),
        }
    }

    /// The input with these roles stamped into it, under keys that start with the prefix the
    /// dispatcher strips from what a client sends and that this overwrites every time.
    pub fn stamp_roles(&self, input: &Value) -> std::collections::BTreeMap<String, Value> {
        let mut m = match input {
            Value::Map(m) => m.clone(),
            _ => Default::default(),
        };
        m.insert(KEY_REVIEWER.into(), Value::Bool(self.reviewer));
        m.insert(KEY_ADMIN.into(), Value::Bool(self.admin));
        m
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
        use super::{MAX_POINTS_PER_TRIP as TOP, check_seq};
        assert!(check_seq(1).is_ok() && check_seq(TOP as i64).is_ok());
        for seq in [i64::MIN, -1, 0, TOP as i64 + 1, i64::MAX] {
            assert!(check_seq(seq).unwrap_err().contains("at most"), "{seq}");
        }
    }

    #[test]
    fn a_role_counts_only_on_top_of_the_driver_group() {
        let with = |groups: &[&str]| Who::of(&Principal::user("x", groups));
        assert_eq!(
            with(&[DRIVER, REVIEWER, ADMIN]),
            Who {
                id: "x".into(),
                reviewer: true,
                admin: true
            }
        );
        for groups in [&[REVIEWER][..], &[ADMIN], &[REVIEWER, ADMIN]] {
            let w = with(groups);
            assert!(!w.reviewer && !w.admin, "{groups:?}");
        }
        assert!(with(&[DRIVER]).plain_driver());
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
