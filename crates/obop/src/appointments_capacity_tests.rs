//! Appointments 0.3.2: a resource is booked at most as many times at once as its capacity says
//! (`[model.no_overlap]` with `capacity = "resource_id.capacity"`); without a capacity, once.

use crate::app::App;
use obop_commands::{ADMIN_GROUP, CallError, Principal, Surface, Value, Vault};
use obop_pkg::{Bundled, Marketplace};
use serde_json::{Value as J, json};

fn app() -> App {
    let app = App::new();
    app.installer
        .install(
            &Bundled.fetch("appointments").unwrap(),
            "marketplace",
            "test",
        )
        .unwrap();
    app
}

fn run(a: &App, cmd: &str, input: J) -> Result<J, String> {
    match a.dispatcher.call(
        Surface::Rest,
        &Principal::user("root", &[ADMIN_GROUP]),
        &mut Vault::new(),
        cmd,
        &Value::from_json(&input),
    ) {
        Ok(r) => Ok(r.value.to_json()),
        Err(CallError::Handler(m)) => Err(m),
        Err(e) => Err(format!("{e:?}")),
    }
}

fn resource(a: &App, capacity: Option<i64>) -> String {
    let mut r = json!({"name": "Training room", "kind": "room"});
    if let Some(c) = capacity {
        r["capacity"] = json!(c);
    }
    run(a, "appointments_resource.create", r).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn book(a: &App, res: &str, start: i64, end: i64) -> Result<J, String> {
    run(
        a,
        "appointments_appointment.create",
        json!({"title": "Intake", "attendee_name": "Max Weber", "resource_id": res,
               "day": "2027-03-03", "start_minute": start, "end_minute": end}),
    )
}

#[test]
fn a_resource_that_serves_two_at_once_takes_two_overlapping_bookings_and_not_three() {
    let a = app();
    let room = resource(&a, Some(2));
    book(&a, &room, 600, 630).unwrap();
    book(&a, &room, 600, 630).expect("the second seat");
    let e = book(&a, &room, 615, 645).unwrap_err();
    assert!(e.contains("no_double_booking"), "{e}");
    // Back to back with one of them, overlapping the other only: two at once at most.
    book(&a, &room, 630, 660).unwrap();
    // Without a capacity a resource is booked once at a time.
    let desk = resource(&a, None);
    book(&a, &desk, 600, 630).unwrap();
    let e = book(&a, &desk, 610, 620).unwrap_err();
    assert!(e.contains("no_double_booking"), "{e}");
}
