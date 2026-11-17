//! The D95 hints of the People and Collaboration packages (hr, fleet, documents, knowledge, sign,
//! sign_docuseal, dashboards) on the real bundled manifests: what the server fills in, stamps and
//! refuses, the same for every surface, and what `/api/meta` says about where each app opens and
//! how its models are named.

use crate::app::App;
use obop_commands::{ADMIN_GROUP, CallError, ModuleInfo, Principal, Surface, Value, Vault};
use obop_pkg::{Bundled, Marketplace};
use serde_json::{Value as J, json};

/// 2026-09-14 (UTC), so `$today` and `today` are that day.
const SEPTEMBER_14: i64 = 1_789_400_000;
const TODAY: &str = "2026-09-14";

const PACKAGES: [&str; 7] = [
    "hr",
    "fleet",
    "documents",
    "knowledge",
    "sign",
    "sign_docuseal",
    "dashboards",
];

fn app() -> App {
    let app = App::new();
    app.dispatcher
        .set_clock(std::sync::Arc::new(|| SEPTEMBER_14));
    for id in PACKAGES {
        if app.installer.list().iter().any(|p| p.id == id) {
            continue;
        }
        app.installer
            .install(&Bundled.fetch(id).unwrap(), "marketplace", "test")
            .unwrap_or_else(|e| panic!("{id}: {e}"));
    }
    app
}

fn run(app: &App, p: &Principal, cmd: &str, input: J) -> Result<J, String> {
    match app.dispatcher.call(
        Surface::Rest,
        p,
        &mut Vault::new(),
        cmd,
        &Value::from_json(&input),
    ) {
        Ok(r) => Ok(r.value.to_json()),
        Err(CallError::Handler(m)) => Err(m),
        Err(CallError::ApprovalRequired { plan_id }) => Err(format!("approval:{plan_id}")),
        Err(e) => Err(format!("{e:?}")),
    }
}

fn ok(app: &App, p: &Principal, cmd: &str, input: J) -> J {
    run(app, p, cmd, input).unwrap_or_else(|e| panic!("{cmd}: {e}"))
}

fn id(v: &J) -> String {
    v["id"].as_str().unwrap().to_string()
}

/// Run an action and read the record back, so that what its effects wrote is in it.
fn act(app: &App, p: &Principal, cmd: &str, rec: &str) -> J {
    ok(app, p, cmd, json!({"id": rec}));
    let model = cmd.split('.').next().unwrap();
    ok(app, p, &format!("{model}.get"), json!({"id": rec}))
}

fn module(app: &App, id: &str) -> ModuleInfo {
    let root = Principal::user("root", &[ADMIN_GROUP]);
    app.dispatcher
        .meta(Surface::Rest, &root)
        .into_iter()
        .find(|m| m.id == id)
        .unwrap_or_else(|| panic!("no module {id}"))
}

#[test]
fn every_app_opens_on_its_main_model_and_names_its_models_in_english() {
    let app = app();
    for (pkg, home) in [
        ("hr", Some("hr_employee")),
        ("fleet", Some("fleet_vehicle")),
        ("documents", Some("documents_document")),
        ("knowledge", Some("knowledge_article")),
        ("sign", Some("sign_request")),
        ("sign_docuseal", Some("sign_docuseal_event")),
        // The Dashboards app opens on its board, which the back office registers itself.
        ("dashboards", None),
    ] {
        let m = module(&app, pkg);
        assert_eq!(m.ui.home.as_deref(), home, "{pkg}");
        for model in &m.models {
            let ui = model
                .ui
                .as_ref()
                .unwrap_or_else(|| panic!("{} has no ui", model.name));
            assert!(ui.menu.is_some(), "{}: say which menu", model.name);
            assert!(ui.title.is_some(), "{}: a title", model.name);
            assert!(
                model.plurals.contains_key("en"),
                "{}: an English plural",
                model.name
            );
            for lang in ["de", "fr", "pl", "fi", "sv", "es"] {
                assert!(model.plurals.contains_key(lang), "{} {lang}", model.name);
            }
        }
    }
    assert_eq!(module(&app, "sign_docuseal").ui.launcher, Some(false));
    let hr = module(&app, "hr");
    let leave = hr
        .models
        .iter()
        .find(|m| m.name == "hr_leave_request")
        .unwrap();
    assert_eq!(leave.plurals["en"], "Time off");
    let requested_by = leave
        .fields
        .iter()
        .find(|f| f.name == "requested_by")
        .unwrap();
    assert!(requested_by.hints.as_ref().unwrap().ui.hidden);
    // Never on a card or in a column sum: the salary.
    let employee = hr.models.iter().find(|m| m.name == "hr_employee").unwrap();
    let ui = employee.ui.as_ref().unwrap();
    assert!(!ui.card.iter().any(|f| f == "salary"));
    assert!(!ui.list.iter().any(|f| f == "salary"));
    let salary = employee.fields.iter().find(|f| f.name == "salary").unwrap();
    assert_eq!(
        salary.hints.as_ref().unwrap().ui.aggregate.as_deref(),
        Some("none")
    );
}

#[test]
fn a_leave_request_needs_its_days_in_order_before_it_is_requested_or_approved() {
    let app = app();
    let hanna = Principal::user("hanna", &["hr.user", "hr.manager"]);
    let ann = Principal::user("ann", &["hr.user"]);
    let emp = id(&ok(
        &app,
        &hanna,
        "hr_employee.create",
        json!({"name": "Ann Example"}),
    ));
    // The kind of leave defaults to paid.
    let bad = ok(
        &app,
        &ann,
        "hr_leave_request.create",
        json!({"employee_id": emp, "start_on": "2026-10-05", "end_on": "2026-10-02", "days": 2}),
    );
    assert_eq!(bad["leave_kind"], "paid");
    assert_eq!(bad["requested_by"], "ann");
    let e = run(
        &app,
        &ann,
        "hr_leave_request.request",
        json!({"id": id(&bad)}),
    )
    .unwrap_err();
    assert!(
        e.contains("The last day of the leave is before its first day"),
        "{e}"
    );
    let none = id(&ok(
        &app,
        &ann,
        "hr_leave_request.create",
        json!({"employee_id": emp, "start_on": "2026-10-05", "end_on": "2026-10-05", "days": 0}),
    ));
    let e = run(&app, &ann, "hr_leave_request.request", json!({"id": none})).unwrap_err();
    assert!(e.contains("number of working days"), "{e}");
    // A request in order goes through, and the manager's approval checks the same.
    ok(
        &app,
        &ann,
        "hr_leave_request.update",
        json!({"id": id(&bad), "end_on": "2026-10-06", "days": 2}),
    );
    ok(
        &app,
        &ann,
        "hr_leave_request.request",
        json!({"id": id(&bad)}),
    );
    let r = ok(
        &app,
        &hanna,
        "hr_leave_request.approve",
        json!({"id": id(&bad)}),
    );
    assert_eq!(r["state"], "approved");
}

#[test]
fn leave_follows_the_employee_when_hr_moves_it_to_another_one() {
    let app = app();
    let hanna = Principal::user("hanna", &["hr.user", "hr.manager"]);
    let julia = Principal::user("julia", &["hr.user"]);
    let frank = Principal::user("frank", &["hr.user"]);
    let emp = |name: &str, user: &str| {
        id(&ok(
            &app,
            &hanna,
            "hr_employee.create",
            json!({"name": name, "user": user}),
        ))
    };
    let (ju, fr) = (emp("Julia Brandt", "julia"), emp("Frank Lehmann", "frank"));
    let leave = ok(
        &app,
        &hanna,
        "hr_leave_request.create",
        json!({"employee_id": ju, "start_on": "2026-10-05", "end_on": "2026-10-06", "days": 2}),
    );
    assert_eq!(leave["requested_by"], "julia");
    let moved = ok(
        &app,
        &hanna,
        "hr_leave_request.update",
        json!({"id": id(&leave), "employee_id": fr}),
    );
    assert_eq!(moved["requested_by"], "frank");
    ok(
        &app,
        &frank,
        "hr_leave_request.get",
        json!({"id": id(&leave)}),
    );
    assert!(
        run(
            &app,
            &julia,
            "hr_leave_request.get",
            json!({"id": id(&leave)})
        )
        .is_err()
    );
    // An employee without a login: the request is the filer's, as on create.
    let none = id(&ok(
        &app,
        &hanna,
        "hr_employee.create",
        json!({"name": "No Login"}),
    ));
    let r = ok(
        &app,
        &hanna,
        "hr_leave_request.update",
        json!({"id": id(&leave), "employee_id": none}),
    );
    assert_eq!(r["requested_by"], "hanna");
    // R1: the owner is the server's to set. A form that sends the stored owner back still moves
    // the request to the new employee.
    let r = ok(
        &app,
        &hanna,
        "hr_leave_request.update",
        json!({"id": id(&leave), "employee_id": ju, "requested_by": "hanna"}),
    );
    assert_eq!(r["requested_by"], "julia");
    let e = run(
        &app,
        &hanna,
        "hr_leave_request.update",
        json!({"id": id(&leave), "requested_by": "hanna"}),
    )
    .unwrap_err();
    assert!(e.contains("is set by the server"), "{e}");
}

#[test]
fn nobody_names_the_owner_of_leave_and_a_colleague_s_leave_is_refused_plainly() {
    let app = app();
    let hanna = Principal::user("hanna", &["hr.user", "hr.manager"]);
    let ann = Principal::user("ann", &["hr.user"]);
    let emp = |name: &str, user: &str| {
        id(&ok(
            &app,
            &hanna,
            "hr_employee.create",
            json!({"name": name, "user": user}),
        ))
    };
    let (ju, an) = (emp("Julia Brandt", "julia"), emp("Ann Example", "ann"));
    let body = |e: &str, by: &str| {
        let mut b =
            json!({"employee_id": e, "start_on": "2026-10-05", "end_on": "2026-10-06", "days": 2});
        if !by.is_empty() {
            b["requested_by"] = json!(by);
        }
        b
    };
    // Not even hr.manager may file leave in another login's name.
    for who in [&hanna, &ann] {
        let e = run(
            &app,
            who,
            "hr_leave_request.create",
            body(&an, "somebody-else"),
        )
        .unwrap_err();
        assert!(
            e.contains("hr_leave_request.requested_by is set by the server"),
            "{e}"
        );
    }
    // hr.user filing for a colleague who has a login: a message of its own, no field named.
    let e = run(&app, &ann, "hr_leave_request.create", body(&ju, "")).unwrap_err();
    assert_eq!(
        e,
        "This employee has their own login, so the record would be theirs and out of your sight: ask someone with hr.manager to file it. Nothing was saved"
    );
    // Their own leave and leave filed by hr.manager are the employee's.
    assert_eq!(
        ok(&app, &ann, "hr_leave_request.create", body(&an, ""))["requested_by"],
        "ann"
    );
    assert_eq!(
        ok(&app, &hanna, "hr_leave_request.create", body(&ju, ""))["requested_by"],
        "julia"
    );
}

#[test]
fn leave_an_agent_files_for_a_linked_employee_is_the_employees() {
    let app = app();
    let hanna = Principal::user("hanna", &["hr.user", "hr.manager"]);
    let agent = Principal::agent(
        "hanna",
        &["hr.user", "hr.manager"],
        &["hr.user", "hr.manager"],
    );
    let ju = id(&ok(
        &app,
        &hanna,
        "hr_employee.create",
        json!({"name": "Julia Brandt", "user": "julia"}),
    ));
    let leave = ok(
        &app,
        &agent,
        "hr_leave_request.create",
        json!({"employee_id": ju, "start_on": "2026-10-05", "end_on": "2026-10-06", "days": 2}),
    );
    // The agent reads the login masked; what is stored is the employee's.
    assert_ne!(leave["requested_by"], "julia");
    let stored = ok(
        &app,
        &hanna,
        "hr_leave_request.get",
        json!({"id": id(&leave)}),
    );
    assert_eq!(stored["requested_by"], "julia");
}

#[test]
fn a_vehicle_goes_to_the_workshop_and_a_service_starts_from_its_odometer() {
    let app = app();
    let m = Principal::user("m", &["fleet.user", "fleet.manager"]);
    let v = id(&ok(
        &app,
        &m,
        "fleet_vehicle.create",
        json!({"plate": "HH-NL 1", "odometer_km": 48_210}),
    ));
    assert_eq!(
        ok(&app, &m, "fleet_vehicle.send_to_workshop", json!({"id": v}))["state"],
        "maintenance"
    );
    assert!(run(&app, &m, "fleet_vehicle.dispatch", json!({"id": v})).is_err());
    assert_eq!(
        ok(&app, &m, "fleet_vehicle.back_in_service", json!({"id": v}))["state"],
        "available"
    );
    // The date is today and the reading the vehicle's, unless given.
    let s = ok(
        &app,
        &m,
        "fleet_vehicle_service.create",
        json!({"vehicle_id": v, "kind": "tyres"}),
    );
    assert_eq!(s["service_on"], TODAY);
    assert_eq!(s["odometer_km"], 48_210);
    let s = ok(
        &app,
        &m,
        "fleet_vehicle_service.create",
        json!({"vehicle_id": v, "kind": "repair", "service_on": "2026-09-01", "odometer_km": 47_000}),
    );
    assert_eq!(s["service_on"], "2026-09-01");
    assert_eq!(s["odometer_km"], 47_000);
}

#[test]
fn publishing_an_article_stamps_the_day_once_and_only_the_server_writes_it() {
    let app = app();
    let kim = Principal::user("kim", &["knowledge.user", "knowledge.manager"]);
    let a = ok(
        &app,
        &kim,
        "knowledge_article.create",
        json!({"title": "Onboarding"}),
    );
    assert_eq!(a["visibility"], "workspace");
    assert_eq!(a["author_name"], "kim");
    let a = id(&a);
    let e = run(
        &app,
        &kim,
        "knowledge_article.update",
        json!({"id": a, "published_on": "2026-01-01"}),
    )
    .unwrap_err();
    assert!(e.contains("set by the server"), "{e}");
    let r = act(&app, &kim, "knowledge_article.publish", &a);
    assert_eq!(r["published_on"], TODAY);
    // Back to draft and out again: the first day stays.
    app.dispatcher
        .set_clock(std::sync::Arc::new(|| SEPTEMBER_14 + 86_400 * 3));
    ok(&app, &kim, "knowledge_article.unpublish", json!({"id": a}));
    let r = act(&app, &kim, "knowledge_article.publish", &a);
    assert_eq!(r["published_on"], TODAY);
    // A form that sends the record back as it is still saves.
    ok(
        &app,
        &kim,
        "knowledge_article.update",
        json!({"id": a, "published_on": TODAY, "title": "Onboarding guide"}),
    );
}

#[test]
fn documents_tiles_and_templates_start_with_sensible_values() {
    let app = app();
    let dora = Principal::user("dora", &["documents.user"]);
    let d = ok(
        &app,
        &dora,
        "documents_document.create",
        json!({"title": "Lease", "kind": "contract"}),
    );
    assert_eq!(d["confidentiality"], "internal");
    assert_eq!(d["owner"], "dora");

    let boss = Principal::user("boss", &["dashboards.user", "dashboards.manager"]);
    let b = ok(
        &app,
        &boss,
        "dashboards_dashboard.create",
        json!({"title": "Sales"}),
    );
    assert_eq!(b["owner"], "boss");
    let w = ok(
        &app,
        &boss,
        "dashboards_widget.create",
        json!({"dashboard_id": id(&b), "title": "Orders", "kind": "kpi", "model_name": "sale_order"}),
    );
    assert_eq!(w["aggregate"], "count");

    let sm = Principal::user("sm", &["sign.user", "sign.manager"]);
    let t = ok(&app, &sm, "sign_template.create", json!({"name": "NDA"}));
    assert_eq!(t["active"], true);
    // Sending from a template stamps the day; marking it signed by hand stamps that day too.
    let r = id(&ok(
        &app,
        &sm,
        "sign_request.create",
        json!({"title": "NDA for Jane", "template_id": id(&t), "signer_name": "Jane Test",
               "signer_email": "jane@example.test"}),
    ));
    assert_eq!(act(&app, &sm, "sign_request.send", &r)["sent_on"], TODAY);
    assert_eq!(act(&app, &sm, "sign_request.sign", &r)["signed_on"], TODAY);
    // A day typed by hand is kept.
    let r = id(&ok(
        &app,
        &sm,
        "sign_request.create",
        json!({"title": "NDA for Joe", "template_id": id(&t), "signer_name": "Joe Test",
               "signer_email": "joe@example.test", "sent_on": "2026-09-01"}),
    ));
    assert_eq!(
        act(&app, &sm, "sign_request.send", &r)["sent_on"],
        "2026-09-01"
    );
}

#[test]
fn leave_follows_when_the_employee_s_login_is_set_or_corrected() {
    // R12: leave filed before an employee had a login, or under a wrong one, becomes the login's.
    let app = app();
    let hanna = Principal::user("hanna", &["hr.user", "hr.manager"]);
    let julia = Principal::user("julia", &["hr.user"]);
    let emp = id(&ok(
        &app,
        &hanna,
        "hr_employee.create",
        json!({"name": "Julia Brandt"}),
    ));
    let leave = id(&ok(
        &app,
        &hanna,
        "hr_leave_request.create",
        json!({"employee_id": emp, "start_on": "2026-10-05", "end_on": "2026-10-06", "days": 2}),
    ));
    let owner = |app: &crate::app::App| {
        ok(app, &hanna, "hr_leave_request.get", json!({"id": leave}))["requested_by"].clone()
    };
    assert_eq!(owner(&app), "hanna");
    assert!(run(&app, &julia, "hr_leave_request.get", json!({"id": leave})).is_err());
    ok(
        &app,
        &hanna,
        "hr_employee.update",
        json!({"id": emp, "user": "jullia"}),
    );
    assert_eq!(owner(&app), "jullia");
    // Corrected: the leave moves again, and Julia sees it.
    ok(
        &app,
        &hanna,
        "hr_employee.update",
        json!({"id": emp, "user": "julia"}),
    );
    assert_eq!(owner(&app), "julia");
    ok(&app, &julia, "hr_leave_request.get", json!({"id": leave}));
    // A login taken away moves nothing.
    ok(
        &app,
        &hanna,
        "hr_employee.update",
        json!({"id": emp, "user": null}),
    );
    assert_eq!(owner(&app), "julia");
}
