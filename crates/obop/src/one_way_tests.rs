//! `one_way` in `meta` over the real modules (#102, U76): an action without `from` whose own
//! guard lets it start only in some states is no way out of the others. Mileage's discards
//! declare no `from`, and the policy refuses them on a confirmed trip, so confirm is one way.

use crate::app::App;
use obop_commands::{Principal, Surface};

fn one_way(app: &App, who: &Principal, model: &str, action: &str) -> Option<bool> {
    let meta = app.dispatcher.meta(Surface::Rest, who);
    meta.iter()
        .flat_map(|m| m.models.iter())
        .find(|m| m.name == model)
        .and_then(|m| {
            m.actions
                .iter()
                .find(|a| a.id == format!("{model}.{action}"))
        })
        .unwrap_or_else(|| panic!("{model}.{action} is not in meta"))
        .one_way
}

#[test]
fn confirming_a_trip_is_one_way_and_the_steps_before_it_are_not() {
    let app = App::new();
    let reviewer = Principal::user(
        "rita",
        &["mileage.driver", "mileage.reviewer", "mileage.admin"],
    );
    // Nothing moves a confirmed trip on: the discards start only before it.
    assert_eq!(
        one_way(&app, &reviewer, "mileage_trip", "confirm"),
        Some(true)
    );
    // A rejected trip can be reopened or discarded; an ended one submitted or discarded.
    for a in ["reject", "end", "submit"] {
        assert_eq!(
            one_way(&app, &reviewer, "mileage_trip", a),
            Some(false),
            "{a}"
        );
    }
    // A discarded trip stays discarded.
    assert_eq!(
        one_way(&app, &reviewer, "mileage_trip", "discard"),
        Some(true)
    );
}
