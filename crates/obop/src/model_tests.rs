//! D56 to D58 end to end, on the real bundled packages: the workflows that effects, assertions,
//! computed fields, sequences, uniqueness and row-level access make possible, with the
//! permissions of real roles.

use crate::app::App;
use obop_commands::{ADMIN_GROUP, CallError, Principal, Surface, Value, Vault};
use obop_pkg::{Bundled, Marketplace};
use serde_json::{Value as J, json};

/// 2026-09-14, so numbers carry 2026.
const SEPTEMBER_2026: i64 = 1_789_400_000;

fn app_with(ids: &[&str]) -> App {
    let app = App::new();
    app.dispatcher
        .set_clock(std::sync::Arc::new(|| SEPTEMBER_2026));
    for id in ids {
        if app.installer.list().iter().any(|p| &p.id == id) {
            continue;
        }
        app.installer
            .install(&Bundled.fetch(id).unwrap(), "marketplace", "test")
            .unwrap_or_else(|e| panic!("{id}: {e}"));
    }
    app
}

fn user(name: &str, groups: &[&str]) -> Principal {
    Principal::user(name, groups)
}

fn admin() -> Principal {
    user("root", &[ADMIN_GROUP])
}

/// Run a command; the error is the handler message, `approval:<plan>` or `forbidden`.
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
        Err(CallError::Forbidden(_)) => Err("forbidden".into()),
        Err(e) => Err(format!("{e:?}")),
    }
}

fn ok(app: &App, p: &Principal, cmd: &str, input: J) -> J {
    run(app, p, cmd, input).unwrap_or_else(|e| panic!("{cmd} {e}"))
}

fn id(v: &J) -> String {
    v["id"].as_str().unwrap().to_string()
}

fn get(app: &App, p: &Principal, model: &str, rec: &str) -> J {
    ok(app, p, &format!("{model}.get"), json!({"id": rec}))
}

fn list(app: &App, p: &Principal, model: &str) -> Vec<J> {
    ok(app, p, &format!("{model}.list"), json!({}))
        .as_array()
        .unwrap()
        .clone()
}

fn plan_of(r: Result<J, String>) -> String {
    r.unwrap_err()
        .strip_prefix("approval:")
        .expect("a proposal")
        .to_string()
}

fn approve(app: &App, human: &Principal, plan: &str) -> Result<J, String> {
    app.dispatcher
        .approve(human, &mut Vault::new(), plan)
        .map(|r| r.value.to_json())
        .map_err(|e| match e {
            CallError::Handler(m) => m,
            other => format!("{other:?}"),
        })
}

// ---- accounting ---------------------------------------------------------------------------

#[test]
fn an_entry_posts_only_when_balanced_gets_a_gapless_number_and_totals_follow_the_lines() {
    let app = app_with(&["accounting"]);
    let clerk = user("clerk", &["accounting.user"]);
    let approver = user("chief", &["accounting.user", "accounting.approver"]);
    let manager = user("boss", &["accounting.user", "accounting.manager"]);
    let journal = id(&ok(
        &app,
        &manager,
        "accounting_journal.create",
        json!({"name": "General", "code": "GEN", "kind": "general"}),
    ));
    let account = |code: &str| {
        id(&ok(
            &app,
            &manager,
            "accounting_account.create",
            json!({"code": code, "name": code, "account_type": "asset_current"}),
        ))
    };
    let (cash, sales) = (account("1000"), account("4000"));
    // Codes are unique.
    let e = run(
        &app,
        &manager,
        "accounting_account.create",
        json!({"code": "1000", "name": "again", "account_type": "equity"}),
    )
    .unwrap_err();
    assert!(e.contains("accounting_account.code must be unique"), "{e}");
    let entry = |amounts: &[(i64, i64)]| {
        let m = id(&ok(
            &app,
            &clerk,
            "accounting_move.create",
            json!({"journal_id": journal, "date": "2026-09-14"}),
        ));
        for (debit, credit) in amounts {
            ok(
                &app,
                &clerk,
                "accounting_move_line.create",
                json!({"move_id": m, "account_id": if *debit > 0 { &cash } else { &sales },
                       "debit": debit, "credit": credit}),
            );
        }
        m
    };
    let a = entry(&[(1000, 0), (0, 900)]);
    let row = get(&app, &clerk, "accounting_move", &a);
    assert_eq!(
        (row["total_debit"].as_i64(), row["total_credit"].as_i64()),
        (Some(1000), Some(900))
    );
    assert_eq!(row["number"], J::Null);
    // The chief may post, but not an entry that does not balance; nothing changes.
    let e = run(&app, &approver, "accounting_move.post", json!({"id": a})).unwrap_err();
    assert!(e.contains("The entry does not balance"), "{e}");
    assert_eq!(get(&app, &clerk, "accounting_move", &a)["state"], "draft");
    // Fix the credit line: now it balances and posts.
    let lines = list(&app, &clerk, "accounting_move_line");
    let credit_line = lines.iter().find(|l| l["credit"] == 900).unwrap();
    ok(
        &app,
        &clerk,
        "accounting_move_line.update",
        json!({"id": id(credit_line), "credit": 1000}),
    );
    // An agent for the chief only proposes; the human approves; the number is assigned then.
    let bot = Principal::agent(
        "chief",
        &["accounting.user", "accounting.approver"],
        &["accounting.user", "accounting.approver"],
    );
    let plan = plan_of(run(&app, &bot, "accounting_move.post", json!({"id": a})));
    assert_eq!(get(&app, &clerk, "accounting_move", &a)["number"], J::Null);
    approve(&app, &approver, &plan).unwrap();
    let posted = get(&app, &clerk, "accounting_move", &a);
    assert_eq!(
        (posted["state"].as_str(), posted["number"].as_str()),
        (Some("posted"), Some("JE/2026/00001"))
    );
    // The second entry gets the next number; a failed post in between used none.
    let unbalanced = entry(&[(5, 0)]);
    assert!(
        run(
            &app,
            &approver,
            "accounting_move.post",
            json!({"id": unbalanced})
        )
        .is_err()
    );
    let b = entry(&[(70, 0), (0, 70)]);
    ok(&app, &approver, "accounting_move.post", json!({"id": b}));
    assert_eq!(
        get(&app, &clerk, "accounting_move", &b)["number"],
        "JE/2026/00002"
    );
    // A numbered entry cannot be deleted, and a clerk cannot post.
    let e = run(&app, &admin(), "accounting_move.delete", json!({"id": b})).unwrap_err();
    assert!(e.contains("gapless number"), "{e}");
    assert_eq!(
        run(&app, &clerk, "accounting_move.post", json!({"id": a})).unwrap_err(),
        "forbidden"
    );
}

// ---- inventory and purchase ---------------------------------------------------------------

fn stock_setup(app: &App) -> (String, String, String, String) {
    let boss = user(
        "boss",
        &["inventory.user", "inventory.manager", "product.manager"],
    );
    let wh = id(&ok(
        app,
        &boss,
        "inventory_warehouse.create",
        json!({"name": "Main", "code": "WH"}),
    ));
    let loc = |name: &str, kind: &str| {
        id(&ok(
            app,
            &boss,
            "inventory_location.create",
            json!({"name": name, "warehouse_id": wh, "kind": kind}),
        ))
    };
    let (shelf, dock) = (loc("Shelf", "internal"), loc("Dock", "receiving"));
    let product = id(&ok(
        app,
        &boss,
        "product.create",
        json!({"name": "Bolt", "sku": "BOLT-1"}),
    ));
    (wh, shelf, dock, product)
}

fn quantity(app: &App, location: &str, product: &str) -> Option<i64> {
    list(app, &admin(), "inventory_item")
        .iter()
        .find(|i| i["location_id"] == location && i["product_id"] == product)
        .and_then(|i| i["quantity"].as_i64())
}

#[test]
fn validating_a_stock_move_changes_the_quantities_at_both_ends_and_nowhere_else() {
    let app = app_with(&["inventory"]);
    let (_, shelf, dock, bolt) = stock_setup(&app);
    let boss = user("boss", &["inventory.user", "inventory.manager"]);
    let clerk = user("clerk", &["inventory.user"]);
    // The first move creates the items; a move from the dock to the shelf shifts 8 units.
    let receipt = id(&ok(
        &app,
        &clerk,
        "inventory_move.create",
        json!({"product_id": bolt, "to_location_id": dock, "quantity": 10}),
    ));
    let m = get(&app, &clerk, "inventory_move", &receipt);
    assert_eq!(m["reference"], "MV-00001");
    // Only a stock manager validates: the clerk does not hold the group the item needs.
    assert_eq!(
        run(
            &app,
            &clerk,
            "inventory_move.validate",
            json!({"id": receipt})
        )
        .unwrap_err(),
        "forbidden"
    );
    ok(
        &app,
        &boss,
        "inventory_move.validate",
        json!({"id": receipt}),
    );
    assert_eq!(quantity(&app, &dock, &bolt), Some(10));
    assert_eq!(quantity(&app, &shelf, &bolt), None);
    let shift = id(&ok(
        &app,
        &clerk,
        "inventory_move.create",
        json!({"product_id": bolt, "from_location_id": dock, "to_location_id": shelf, "quantity": 8}),
    ));
    ok(&app, &boss, "inventory_move.validate", json!({"id": shift}));
    assert_eq!(
        (quantity(&app, &dock, &bolt), quantity(&app, &shelf, &bolt)),
        (Some(2), Some(8))
    );
    // An agent only proposes it; nothing moves until a human approves.
    let again = id(&ok(
        &app,
        &clerk,
        "inventory_move.create",
        json!({"product_id": bolt, "from_location_id": shelf, "quantity": 3}),
    ));
    let bot = Principal::agent(
        "boss",
        &["inventory.user", "inventory.manager"],
        &["inventory.user", "inventory.manager"],
    );
    let plan = plan_of(run(
        &app,
        &bot,
        "inventory_move.validate",
        json!({"id": again}),
    ));
    assert_eq!(quantity(&app, &shelf, &bolt), Some(8));
    approve(&app, &boss, &plan).unwrap();
    assert_eq!(quantity(&app, &shelf, &bolt), Some(5));
    // Done is final: validating again changes nothing.
    assert!(run(&app, &boss, "inventory_move.validate", json!({"id": again})).is_err());
    assert_eq!(quantity(&app, &shelf, &bolt), Some(5));
    // One item per product, location and lot.
    let wh = list(&app, &admin(), "inventory_warehouse");
    let _ = wh;
    let e = run(
        &app,
        &boss,
        "inventory_item.create",
        json!({"product_id": bolt, "location_id": shelf, "quantity": 1}),
    )
    .unwrap_err();
    assert!(
        e.contains("inventory_item.product_id must be unique"),
        "{e}"
    );
    // Product SKUs are unique too.
    let e = run(
        &app,
        &admin(),
        "product.create",
        json!({"name": "Other bolt", "sku": "BOLT-1"}),
    )
    .unwrap_err();
    assert!(e.contains("product.sku must be unique"), "{e}");
}

#[test]
fn receiving_a_purchase_order_creates_stock_moves_the_warehouse_then_validates() {
    let app = app_with(&["purchase", "inventory"]);
    let (_, shelf, _, bolt) = stock_setup(&app);
    let buyer = user("buyer", &["purchase.user", "inventory.user", "base.user"]);
    let chief = user(
        "chief",
        &[
            "purchase.user",
            "purchase.manager",
            "inventory.user",
            "base.user",
        ],
    );
    let boss = user("boss", &["inventory.user", "inventory.manager"]);
    let party = id(&ok(
        &app,
        &admin(),
        "party.create",
        json!({"name": "Vendor Ltd"}),
    ));
    let vendor = id(&ok(
        &app,
        &chief,
        "purchase_vendor.create",
        json!({"party_id": party}),
    ));
    let order = id(&ok(
        &app,
        &buyer,
        "purchase_order.create",
        json!({"reference": "V-77", "vendor_id": vendor, "receive_location_id": shelf, "expected_on": "2026-10-01"}),
    ));
    assert_eq!(
        get(&app, &buyer, "purchase_order", &order)["number"],
        "PO-00001"
    );
    for (qty, price) in [(10, 250), (5, 100)] {
        ok(
            &app,
            &buyer,
            "purchase_order_line.create",
            json!({"order_id": order, "product_id": bolt, "quantity": qty, "unit_price": price}),
        );
    }
    assert_eq!(
        get(&app, &buyer, "purchase_order", &order)["total"],
        3000,
        "10 x 250 + 5 x 100"
    );
    ok(&app, &buyer, "purchase_order.send", json!({"id": order}));
    let bot = Principal::agent(
        "chief",
        &["purchase.user", "purchase.manager"],
        &["purchase.user", "purchase.manager"],
    );
    let plan = plan_of(run(
        &app,
        &bot,
        "purchase_order.confirm",
        json!({"id": order}),
    ));
    approve(&app, &chief, &plan).unwrap();
    // Receiving needs the right to make stock moves; without it nothing happens.
    let only_purchase = user("p", &["purchase.user"]);
    let e = run(
        &app,
        &only_purchase,
        "purchase_order.receive",
        json!({"id": order}),
    )
    .unwrap_err();
    assert!(e.contains("inventory.user"), "{e}");
    assert_eq!(
        get(&app, &buyer, "purchase_order", &order)["state"],
        "confirmed"
    );
    assert!(list(&app, &boss, "inventory_move").is_empty());
    ok(&app, &buyer, "purchase_order.receive", json!({"id": order}));
    let moves = list(&app, &boss, "inventory_move");
    assert_eq!(moves.len(), 2);
    assert!(
        moves
            .iter()
            .all(|m| m["state"] == "draft" && m["to_location_id"] == shelf)
    );
    assert_eq!(
        quantity(&app, &shelf, &bolt),
        None,
        "nothing is in stock before the warehouse validates"
    );
    for m in &moves {
        ok(&app, &boss, "inventory_move.validate", json!({"id": id(m)}));
    }
    assert_eq!(quantity(&app, &shelf, &bolt), Some(15));
}

// ---- pos ----------------------------------------------------------------------------------

#[test]
fn a_sale_is_paid_only_when_the_payments_cover_it_and_it_creates_stock_moves() {
    let app = app_with(&["pos", "inventory"]);
    let (_, shelf, _, bolt) = stock_setup(&app);
    let manager = user("m", &["pos.user", "pos.manager", "inventory.user"]);
    let cashier = user("c", &["pos.user", "inventory.user"]);
    let config = id(&ok(
        &app,
        &manager,
        "pos_config.create",
        json!({"name": "Till"}),
    ));
    let session = id(&ok(
        &app,
        &cashier,
        "pos_session.create",
        json!({"config_id": config, "cashier": "c", "opened_on": "2026-09-14", "opening_cash": 0}),
    ));
    let mut numbers = Vec::new();
    let mut sale = |qty: i64| {
        let o = id(&ok(
            &app,
            &cashier,
            "pos_order.create",
            json!({"session_id": session, "stock_location_id": shelf, "ordered_on": "2026-09-14"}),
        ));
        ok(
            &app,
            &cashier,
            "pos_order_line.create",
            json!({"order_id": o, "product_id": bolt, "quantity": qty, "unit_price": 1000, "discount_bp": 1000}),
        );
        numbers.push(get(&app, &cashier, "pos_order", &o)["receipt_number"].clone());
        o
    };
    let o = sale(3);
    let row = get(&app, &cashier, "pos_order", &o);
    assert_eq!(row["total"], 2700, "3 x 10.00 less 10 percent");
    let e = run(&app, &cashier, "pos_order.pay", json!({"id": o})).unwrap_err();
    assert!(e.contains("payments do not cover the total"), "{e}");
    ok(
        &app,
        &cashier,
        "pos_payment.create",
        json!({"order_id": o, "method": "cash", "amount": 2700}),
    );
    ok(&app, &cashier, "pos_order.pay", json!({"id": o}));
    let moves = list(&app, &cashier, "inventory_move");
    assert_eq!(moves.len(), 1);
    assert_eq!(
        (
            moves[0]["quantity"].as_i64(),
            moves[0]["from_location_id"].as_str()
        ),
        (Some(3), Some(shelf.as_str()))
    );
    // Receipt numbers are a gapless yearly series.
    let o2 = sale(1);
    assert_eq!(numbers, [json!("R-2026-000001"), json!("R-2026-000002")]);
    let e = run(&app, &admin(), "pos_order.delete", json!({"id": o2})).unwrap_err();
    assert!(e.contains("gapless number"), "{e}");
}

// ---- people and access --------------------------------------------------------------------

#[test]
fn employees_see_only_their_own_leave_requests_and_expenses() {
    let app = app_with(&["hr", "expenses"]);
    let emp_a = user("ann", &["hr.user", "expenses.user"]);
    let emp_b = user("bob", &["hr.user", "expenses.user"]);
    let hr_mgr = user("hanna", &["hr.user", "hr.manager"]);
    let boss = user("mia", &["expenses.user", "expenses.manager"]);
    let finance = user("fin", &["expenses.user", "expenses.finance"]);
    let employee = |name: &str| {
        id(&ok(
            &app,
            &hr_mgr,
            "hr_employee.create",
            json!({"name": name, "job_title": "x", "state": "draft"}),
        ))
    };
    let _ = employee;
    let dept = id(&ok(
        &app,
        &hr_mgr,
        "hr_employee.create",
        json!({"name": "E1"}),
    ));
    let leave = |who: &Principal| {
        id(&ok(
            &app,
            who,
            "hr_leave_request.create",
            json!({"employee_id": dept, "leave_kind": "paid", "start_on": "2026-10-01", "end_on": "2026-10-02", "days": 2}),
        ))
    };
    let (la, lb) = (leave(&emp_a), leave(&emp_b));
    assert_eq!(
        get(&app, &emp_a, "hr_leave_request", &la)["requested_by"],
        "ann"
    );
    assert_eq!(list(&app, &emp_a, "hr_leave_request").len(), 1);
    assert_eq!(list(&app, &emp_b, "hr_leave_request").len(), 1);
    assert_eq!(list(&app, &hr_mgr, "hr_leave_request").len(), 2);
    let e = run(&app, &emp_a, "hr_leave_request.get", json!({"id": lb})).unwrap_err();
    assert_eq!(e, format!("no hr_leave_request {lb}"));
    ok(&app, &emp_a, "hr_leave_request.request", json!({"id": la}));
    assert!(
        run(&app, &emp_a, "hr_leave_request.approve", json!({"id": la})).is_err(),
        "not a manager"
    );
    ok(&app, &hr_mgr, "hr_leave_request.approve", json!({"id": la}));

    // Expenses: numbered per year, own claims only, managers and finance see all.
    let claim = |who: &Principal| {
        id(&ok(
            &app,
            who,
            "expenses_expense.create",
            json!({"description": "Taxi", "spent_on": "2026-09-01", "category": "travel", "amount": 1500}),
        ))
    };
    let (ea, eb) = (claim(&emp_a), claim(&emp_b));
    assert_eq!(
        get(&app, &emp_a, "expenses_expense", &ea)["number"],
        "EXP-2026-0001"
    );
    assert_eq!(
        get(&app, &emp_b, "expenses_expense", &eb)["number"],
        "EXP-2026-0002"
    );
    assert_eq!(list(&app, &emp_a, "expenses_expense").len(), 1);
    assert_eq!(list(&app, &boss, "expenses_expense").len(), 2);
    assert_eq!(list(&app, &finance, "expenses_expense").len(), 2);
    assert!(
        run(
            &app,
            &emp_a,
            "expenses_expense.update",
            json!({"id": eb, "description": "mine now"})
        )
        .is_err()
    );
}

#[test]
fn confidential_documents_are_for_their_owner_and_the_managers() {
    let app = app_with(&["documents"]);
    let ann = user("ann", &["documents.user"]);
    let bob = user("bob", &["documents.user"]);
    let manager = user("mia", &["documents.user", "documents.manager"]);
    let doc = |who: &Principal, title: &str, level: &str| {
        id(&ok(
            &app,
            who,
            "documents_document.create",
            json!({"title": title, "kind": "contract", "confidentiality": level}),
        ))
    };
    let open = doc(&ann, "Handbook", "internal");
    let secret = doc(&ann, "Salary review", "confidential");
    let board = doc(&manager, "Board minutes", "restricted");
    let titles = |p: &Principal| -> Vec<String> {
        let mut v: Vec<String> = list(&app, p, "documents_document")
            .iter()
            .map(|d| d["title"].as_str().unwrap().to_string())
            .collect();
        v.sort();
        v
    };
    assert_eq!(
        titles(&ann),
        ["Handbook", "Salary review"],
        "the owner sees her own confidential one"
    );
    assert_eq!(titles(&bob), ["Handbook"]);
    assert_eq!(
        titles(&manager),
        ["Board minutes", "Handbook", "Salary review"]
    );
    assert_eq!(
        get(&app, &ann, "documents_document", &open)["open_to_all"],
        true
    );
    assert_eq!(
        get(&app, &ann, "documents_document", &secret)["open_to_all"],
        false
    );
    assert!(run(&app, &bob, "documents_document.get", json!({"id": secret})).is_err());
    assert!(
        run(
            &app,
            &bob,
            "documents_document.update",
            json!({"id": secret, "title": "x"})
        )
        .is_err()
    );
    assert!(run(&app, &ann, "documents_document.get", json!({"id": board})).is_err());
    // Changing the level changes who sees it, at once.
    ok(
        &app,
        &ann,
        "documents_document.update",
        json!({"id": secret, "confidentiality": "public"}),
    );
    assert_eq!(titles(&bob), ["Handbook", "Salary review"]);
}

#[test]
fn timesheets_are_private_to_their_worker() {
    let app = app_with(&["timesheets"]);
    let boss = user("pm", &["projects.user", "projects.manager"]);
    let ann = user("ann", &["timesheets.user", "projects.user"]);
    let bob = user("bob", &["timesheets.user", "projects.user"]);
    let manager = user(
        "mia",
        &["timesheets.user", "timesheets.manager", "projects.user"],
    );
    let task = id(&ok(
        &app,
        &user("a", &["projects.user"]),
        "projects_task.create",
        json!({"title": "t"}),
    ));
    let _ = boss;
    let entry = |who: &Principal| {
        id(&ok(
            &app,
            who,
            "timesheets_entry.create",
            json!({"task_id": task, "worked_on": "2026-09-14", "minutes": 60}),
        ))
    };
    let (ea, _eb) = (entry(&ann), entry(&bob));
    assert_eq!(get(&app, &ann, "timesheets_entry", &ea)["worker"], "ann");
    assert_eq!(list(&app, &ann, "timesheets_entry").len(), 1);
    assert_eq!(list(&app, &manager, "timesheets_entry").len(), 2);
}

#[test]
fn a_dashboard_is_private_until_it_is_shared_and_its_tiles_follow_it() {
    let app = app_with(&["dashboards"]);
    let mia = user("mia", &["dashboards.user", "dashboards.manager"]);
    let nils = user("nils", &["dashboards.user", "dashboards.manager"]);
    let reader = user("rae", &["dashboards.user"]);
    let board = |who: &Principal, title: &str, shared: bool| {
        id(&ok(
            &app,
            who,
            "dashboards_dashboard.create",
            json!({"title": title, "owner": "ignored-by-nobody", "shared": shared}),
        ))
    };
    let _ = board;
    // The owner is whoever the record says; the UI sends the signed-in user name.
    let make = |who: &Principal, owner: &str, title: &str, shared: bool| {
        id(&ok(
            &app,
            who,
            "dashboards_dashboard.create",
            json!({"title": title, "owner": owner, "shared": shared}),
        ))
    };
    let public = make(&mia, "mia", "Everyone", true);
    let private = make(&mia, "mia", "Mine", false);
    let tile = |d: &str| {
        id(&ok(
            &app,
            &mia,
            "dashboards_widget.create",
            json!({"dashboard_id": d, "title": "T", "kind": "kpi", "model_name": "party", "aggregate": "count"}),
        ))
    };
    let (tp, tq) = (tile(&public), tile(&private));
    let titles = |p: &Principal| -> Vec<String> {
        list(&app, p, "dashboards_dashboard")
            .iter()
            .map(|d| d["title"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(
        titles(&reader),
        ["Everyone"],
        "a reader sees only shared boards"
    );
    assert_eq!(titles(&mia), ["Everyone", "Mine"], "the owner sees hers");
    assert_eq!(titles(&nils), ["Everyone", "Mine"], "managers see all");
    let tiles = |p: &Principal| -> Vec<String> {
        list(&app, p, "dashboards_widget").iter().map(id).collect()
    };
    assert_eq!(
        tiles(&reader),
        std::slice::from_ref(&tp),
        "tiles follow their dashboard"
    );
    assert_eq!(tiles(&mia).len(), 2);
    assert!(run(&app, &reader, "dashboards_widget.get", json!({"id": tq})).is_err());
    // Sharing it later opens it, server side.
    ok(
        &app,
        &mia,
        "dashboards_dashboard.update",
        json!({"id": private, "shared": true}),
    );
    assert_eq!(titles(&reader).len(), 2);
    assert_eq!(tiles(&reader).len(), 2);
}

// ---- sales and invoicing ------------------------------------------------------------------

#[test]
fn confirming_a_sale_order_creates_the_invoice_and_posting_numbers_it() {
    let app = App::new();
    app.dispatcher
        .set_clock(std::sync::Arc::new(|| SEPTEMBER_2026));
    let sales = user(
        "sam",
        &["base.user", "sales.user", "invoicing.user", "product.user"],
    );
    let plain_sales = user("pam", &["base.user", "sales.user"]);
    let finance = user("fin", &["invoicing.user", "invoicing.manager"]);
    let party = id(&ok(&app, &sales, "party.create", json!({"name": "Acme"})));
    let order = id(&ok(
        &app,
        &sales,
        "sale_order.create",
        json!({"party_id": party, "currency": "EUR"}),
    ));
    assert_eq!(
        get(&app, &sales, "sale_order", &order)["number"],
        "SO-00001"
    );
    for (what, qty, price) in [("Workshop", 2, 150_000), ("Travel", 1, 25_000)] {
        ok(
            &app,
            &sales,
            "sale_order_line.create",
            json!({"order_id": order, "description": what, "quantity": qty, "unit_price_minor": price}),
        );
    }
    assert_eq!(
        get(&app, &sales, "sale_order", &order)["total_minor"],
        325_000
    );
    // Without the right to create invoices the confirmation is refused whole.
    let e = run(
        &app,
        &plain_sales,
        "sale_order.confirm",
        json!({"id": order}),
    )
    .unwrap_err();
    assert!(e.contains("invoicing.user"), "{e}");
    assert_eq!(get(&app, &sales, "sale_order", &order)["state"], "draft");
    ok(&app, &sales, "sale_order.confirm", json!({"id": order}));
    let invoices = list(&app, &finance, "invoice");
    assert_eq!(invoices.len(), 1);
    let inv = &invoices[0];
    assert_eq!(
        (inv["state"].as_str(), inv["total_minor"].as_i64()),
        (Some("draft"), Some(325_000))
    );
    assert_eq!(inv["sale_order_id"], order);
    assert_eq!(list(&app, &finance, "invoice_line").len(), 2);
    // Posting by the finance manager numbers it from the gapless series.
    ok(&app, &finance, "invoice.post", json!({"id": id(inv)}));
    assert_eq!(
        get(&app, &finance, "invoice", &id(inv))["number"],
        "INV-2026-00001"
    );
    // A second order and its invoice continue the series.
    let order2 = id(&ok(
        &app,
        &sales,
        "sale_order.create",
        json!({"party_id": party}),
    ));
    ok(
        &app,
        &sales,
        "sale_order_line.create",
        json!({"order_id": order2, "description": "More", "quantity": 1, "unit_price_minor": 100}),
    );
    ok(&app, &sales, "sale_order.confirm", json!({"id": order2}));
    let second = list(&app, &finance, "invoice")
        .into_iter()
        .find(|i| i["sale_order_id"] == order2)
        .unwrap();
    ok(&app, &finance, "invoice.post", json!({"id": id(&second)}));
    assert_eq!(
        get(&app, &finance, "invoice", &id(&second))["number"],
        "INV-2026-00002"
    );
}

// ---- projects and tickets -----------------------------------------------------------------

#[test]
fn a_project_shows_its_progress_and_tickets_are_numbered() {
    let app = app_with(&["projects", "helpdesk"]);
    let pm = user("pm", &["projects.user", "projects.manager"]);
    let project = id(&ok(
        &app,
        &pm,
        "projects_project.create",
        json!({"name": "Site"}),
    ));
    assert_eq!(
        get(&app, &pm, "projects_project", &project)["progress"],
        0,
        "no tasks, no progress"
    );
    let task = |est: i64| {
        id(&ok(
            &app,
            &pm,
            "projects_task.create",
            json!({"title": "t", "project_id": project, "estimate_minutes": est}),
        ))
    };
    let (t1, t2, t3, t4) = (task(60), task(30), task(10), task(100));
    ok(&app, &pm, "projects_task.begin", json!({"id": t1}));
    ok(&app, &pm, "projects_task.complete", json!({"id": t1}));
    ok(&app, &pm, "projects_task.cancel", json!({"id": t4}));
    let _ = (t2, t3);
    let p = get(&app, &pm, "projects_project", &project);
    assert_eq!(p["progress"], 33, "1 of the 3 tasks that are not cancelled");
    assert_eq!(p["estimate_minutes"], 100, "cancelled tasks do not count");
    // Filtering and sorting on a computed field work like on any other.
    let hit = ok(
        &app,
        &pm,
        "projects_project.search",
        json!({"filter": [{"field": "progress", "op": "gte", "value": 30}]}),
    );
    assert_eq!(hit["total"], 1);
    let agent = user("agent", &["helpdesk.agent"]);
    let a = id(&ok(
        &app,
        &agent,
        "helpdesk_ticket.create",
        json!({"subject": "a"}),
    ));
    let b = id(&ok(
        &app,
        &agent,
        "helpdesk_ticket.create",
        json!({"subject": "b"}),
    ));
    assert_eq!(
        get(&app, &agent, "helpdesk_ticket", &a)["number"],
        "TCK-00001"
    );
    assert_eq!(
        get(&app, &agent, "helpdesk_ticket", &b)["number"],
        "TCK-00002"
    );
}

// ---- the unit of work, over the real database ---------------------------------------------

#[test]
fn the_sale_flow_is_atomic_on_a_real_sqlite_file() {
    let dir = std::env::temp_dir().join(format!("obop-model-{}.db", std::process::id()));
    let url = format!("sqlite://{}", dir.display());
    let store = obop_commands::Store::open(&url).unwrap();
    let app = App::with_store(store);
    app.dispatcher
        .set_clock(std::sync::Arc::new(|| SEPTEMBER_2026));
    let sales = user("sam", &["base.user", "sales.user", "invoicing.user"]);
    let order = id(&ok(&app, &sales, "sale_order.create", json!({})));
    ok(
        &app,
        &sales,
        "sale_order_line.create",
        json!({"order_id": order, "quantity": 1, "unit_price_minor": 5}),
    );
    // Break the second effect: a line with a dangling product id written behind the commands.
    let mut f = obop_storage::Fields::new();
    f.insert("order_id".into(), json!(order));
    f.insert("quantity".into(), json!(1));
    f.insert("unit_price_minor".into(), json!(1));
    f.insert("product_id".into(), json!("ghost"));
    app.store
        .backend()
        .insert("sale_order_line", "bad-line", &f)
        .unwrap();
    let e = run(&app, &sales, "sale_order.confirm", json!({"id": order})).unwrap_err();
    assert!(
        e.contains("effect 2") && e.contains("nothing was changed"),
        "{e}"
    );
    assert_eq!(get(&app, &sales, "sale_order", &order)["state"], "draft");
    assert!(
        list(&app, &admin(), "invoice").is_empty(),
        "the invoice of effect 1 was rolled back"
    );
    assert!(list(&app, &admin(), "invoice_line").is_empty());
    for ext in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{ext}", dir.display()));
    }
}
