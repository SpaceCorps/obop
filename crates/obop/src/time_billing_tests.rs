//! `time_billing.create_invoice` over invented projects, tasks and entries: the rate is passed in,
//! nothing is built in.

use crate::app::App;
use obop_commands::{ADMIN_GROUP, CallError, Principal, Surface, Value, Vault};
use obop_pkg::{Bundled, Marketplace};
use serde_json::{Value as J, json};

fn admin() -> Principal {
    Principal::user("root", &[ADMIN_GROUP])
}

fn call(a: &App, p: &Principal, id: &str, input: J) -> Result<J, CallError> {
    a.dispatcher
        .call(
            Surface::Rest,
            p,
            &mut Vault::new(),
            id,
            &Value::from_json(&input),
        )
        .map(|r| r.value.to_json())
}

fn create(a: &App, model: &str, input: J) -> String {
    let r = call(a, &admin(), &format!("{model}.create"), input)
        .unwrap_or_else(|e| panic!("{model}.create: {e:?}"));
    r["id"].as_str().unwrap().to_string()
}

fn rows(a: &App, model: &str) -> Vec<(String, serde_json::Map<String, J>)> {
    a.store
        .backend()
        .list(model)
        .unwrap_or_default()
        .into_iter()
        .map(|(id, f)| (id, f.into_iter().collect()))
        .collect()
}

fn refusal(r: Result<J, CallError>) -> String {
    match r {
        Err(CallError::Handler(m)) => m,
        other => panic!("expected a refusal in words, got {other:?}"),
    }
}

struct Fx {
    app: App,
    task: String,
}

fn fixture() -> Fx {
    let app = App::new();
    for p in ["projects", "timesheets"] {
        app.installer
            .install(&Bundled.fetch(p).unwrap(), "marketplace", "test")
            .unwrap();
    }
    create(&app, "party", json!({"name": "Acme AB"}));
    let project = create(
        &app,
        "projects_project",
        json!({"name": "Website", "code": "WEB"}),
    );
    let task = create(
        &app,
        "projects_task",
        json!({"title": "Design", "project_id": project}),
    );
    Fx { app, task }
}

impl Fx {
    /// An entry of `minutes` on `day`, taken to `approved` unless `approve` is false.
    fn entry(&self, day: &str, minutes: i64, billable: bool, approve: bool) -> String {
        let id = create(
            &self.app,
            "timesheets_entry",
            json!({"task_id": self.task, "worker": "ann", "worked_on": day, "minutes": minutes,
                   "billable": billable, "state": "draft"}),
        );
        if approve {
            // Approving needs a person's approval for an agent only; an administrator runs it.
            call(
                &self.app,
                &admin(),
                "timesheets_entry.submit",
                json!({"id": id}),
            )
            .unwrap();
            call(
                &self.app,
                &admin(),
                "timesheets_entry.approve",
                json!({"id": id}),
            )
            .unwrap();
        }
        id
    }

    fn input(&self, dry: bool) -> J {
        json!({"project": "WEB", "customer": "Acme AB", "from": "2026-09-01", "to": "2026-09-30",
               "hourly_rate_minor": 90_000, "dry_run": dry})
    }

    fn run(&self, input: J) -> Result<J, CallError> {
        call(&self.app, &admin(), "time_billing.create_invoice", input)
    }
}

#[test]
fn it_bills_approved_time_as_a_draft_invoice_once() {
    let f = fixture();
    f.entry("2026-09-02", 90, true, true);
    f.entry("2026-09-03", 7, true, true);
    f.entry("2026-09-04", 60, false, false); // not billable: ignored
    f.entry("2026-10-01", 60, true, true); // outside the period
    let r = f.run(f.input(false)).unwrap();
    assert_eq!(r["written"], json!(true));
    assert_eq!(r["entries"], json!(2));
    assert_eq!(r["minutes"], json!(97));
    assert_eq!(r["hours"], json!("1.62"), "97/60 = 1.6166.. half up");
    // 90 min at 900.00 = 1350.00; 7 min = 105.00 exactly.
    assert_eq!(r["net"], json!(135_000 + 10_500));
    let inv = rows(&f.app, "invoice");
    assert_eq!(inv.len(), 1);
    assert_eq!(inv[0].1["state"], json!("draft"));
    let lines = rows(&f.app, "invoice_line");
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().any(|(_, l)| {
        l["description"]
            .as_str()
            .unwrap()
            .contains("Design 2026-09-02: 1.50 h")
    }));
    assert_eq!(rows(&f.app, "time_billing").len(), 2);
    // A second run finds nothing left and writes nothing.
    let m = refusal(f.run(f.input(false)));
    assert!(m.contains("no approved, billable, unbilled"), "{m}");
    assert_eq!(rows(&f.app, "invoice").len(), 1);
    // A new approved entry is billed alone, on a second invoice.
    f.entry("2026-09-10", 30, true, true);
    let r2 = f.run(f.input(false)).unwrap();
    assert_eq!(r2["entries"], json!(1));
    assert_eq!(rows(&f.app, "invoice").len(), 2);
    assert_eq!(rows(&f.app, "time_billing").len(), 3);
}

#[test]
fn rounding_is_half_up_in_integers() {
    use crate::time_billing::{amount_of, hours_of};
    assert_eq!(amount_of(1, 90).unwrap(), 2); // 1.5 up
    assert_eq!(amount_of(1, 89).unwrap(), 1); // 1.48 down
    assert_eq!(hours_of(90).unwrap().to_string(), "1.50");
    assert!(amount_of(i64::MAX, i64::MAX).is_err());
}

#[test]
fn an_unapproved_billable_entry_blocks_the_run() {
    let f = fixture();
    f.entry("2026-09-02", 60, true, true);
    f.entry("2026-09-03", 60, true, false);
    let m = refusal(f.run(f.input(false)));
    assert!(m.contains("not approved"), "{m}");
    assert!(rows(&f.app, "invoice").is_empty());
    assert!(rows(&f.app, "time_billing").is_empty());
}

#[test]
fn missing_inputs_are_refused_in_words() {
    let f = fixture();
    f.entry("2026-09-02", 60, true, true);
    for key in ["project", "customer", "from", "to", "hourly_rate_minor"] {
        let mut i = f.input(true);
        i.as_object_mut().unwrap().remove(key);
        let m = refusal(f.run(i));
        assert!(m.contains(key), "{key}: {m}");
    }
    let mut zero = f.input(true);
    zero["hourly_rate_minor"] = json!(0);
    assert!(refusal(f.run(zero)).contains("greater than zero"));
    let mut who = f.input(true);
    who["customer"] = json!("Nobody");
    assert!(refusal(f.run(who)).contains("Nobody"));
    let mut late = f.input(true);
    late["from"] = json!("2026-10-01");
    assert!(refusal(f.run(late)).contains("after"));
    assert!(rows(&f.app, "invoice").is_empty());
}

#[test]
fn a_dry_run_writes_nothing_and_an_agent_may_only_dry_run() {
    let f = fixture();
    f.entry("2026-09-02", 60, true, true);
    let dry = f.run(f.input(true)).unwrap();
    assert_eq!(dry["written"], json!(false));
    assert_eq!(dry["net"], json!(90_000));
    assert!(rows(&f.app, "invoice").is_empty());
    assert!(rows(&f.app, "time_billing").is_empty());
    let groups = ["invoicing.user", "timesheets.manager"];
    let agent = Principal::agent("bot", &groups, &groups);
    let m = refusal(call(
        &f.app,
        &agent,
        "time_billing.create_invoice",
        f.input(false),
    ));
    assert!(m.contains("dry-run"), "{m}");
    call(&f.app, &agent, "time_billing.create_invoice", f.input(true)).unwrap();
    assert!(rows(&f.app, "invoice").is_empty());
    let clerk = Principal::user("clerk", &["invoicing.user"]);
    let m = refusal(call(
        &f.app,
        &clerk,
        "time_billing.create_invoice",
        f.input(true),
    ));
    assert!(m.contains("timesheets.manager"), "{m}");
    let nobody = Principal::user("nobody", &["base.user"]);
    assert!(matches!(
        call(
            &f.app,
            &nobody,
            "time_billing.create_invoice",
            f.input(true)
        ),
        Err(CallError::Forbidden(_))
    ));
}
