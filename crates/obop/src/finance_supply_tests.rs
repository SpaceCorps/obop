//! The finance and supply packages (accounting, accounting_se, expenses, inventory, purchase,
//! manufacturing) with their D95 hints and D77 guards, end to end on the bundled manifests: what a
//! final record refuses, what stays editable, the defaults, `fill_from`, the read-only fields, the
//! assertions with their messages and the actions that stamp their own date.

use crate::app::App;
use obop_commands::{ADMIN_GROUP, CallError, Principal, Surface, Value, Vault};
use obop_pkg::{Bundled, Marketplace};
use serde_json::{Value as J, json};

/// 2026-09-14 (UTC), so `today` is 2026-09-14 and numbers carry 2026.
const SEPTEMBER_2026: i64 = 1_789_400_000;
const TODAY: &str = "2026-09-14";

fn app_with(ids: &[&str]) -> App {
    let app = App::new();
    app.dispatcher
        .set_clock(std::sync::Arc::new(|| SEPTEMBER_2026));
    for id in ids {
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
        Err(e) => Err(format!("{e:?}")),
    }
}

fn ok(app: &App, p: &Principal, cmd: &str, input: J) -> J {
    run(app, p, cmd, input).unwrap_or_else(|e| panic!("{cmd}: {e}"))
}

/// The refusal of `cmd`, which must start with `code` (`immutable_record: ...`).
fn refused(app: &App, p: &Principal, cmd: &str, input: J, code: &str) -> String {
    match run(app, p, cmd, input) {
        Ok(v) => panic!("{cmd} was accepted: {v}"),
        Err(e) => {
            assert!(e.starts_with(code), "{cmd}: {e}");
            e
        }
    }
}

fn id(v: &J) -> String {
    v["id"].as_str().unwrap().to_string()
}

fn get(app: &App, p: &Principal, model: &str, id: &str) -> J {
    ok(app, p, &format!("{model}.get"), json!({ "id": id }))
}

const ACCOUNTANT: &[&str] = &[
    "accounting.user",
    "accounting.manager",
    "accounting.approver",
];

#[test]
fn a_posted_entry_its_lines_and_a_closed_period_are_frozen() {
    let app = app_with(&["accounting"]);
    let ann = user("ann", ACCOUNTANT);
    // A new journal starts active; an inactive one is not offered for an entry.
    let journal = id(&ok(
        &app,
        &ann,
        "accounting_journal.create",
        json!({"name": "Miscellaneous", "code": "MISC", "kind": "general"}),
    ));
    assert_eq!(
        get(&app, &ann, "accounting_journal", &journal)["active"],
        true
    );
    let old = id(&ok(
        &app,
        &ann,
        "accounting_journal.create",
        json!({"name": "Old", "code": "OLD", "kind": "general", "active": false}),
    ));
    refused(
        &app,
        &ann,
        "accounting_move.create",
        json!({ "journal_id": old }),
        "ref_not_allowed",
    );
    let bank = id(&ok(
        &app,
        &ann,
        "accounting_account.create",
        json!({"code": "1200", "name": "Bank", "account_type": "asset_cash"}),
    ));
    let fees = id(&ok(
        &app,
        &ann,
        "accounting_account.create",
        json!({"code": "6800", "name": "Bank charges", "account_type": "expense"}),
    ));

    // An entry is dated today; with no lines it is not posted, and the refusal is the sentence.
    let mv = ok(
        &app,
        &ann,
        "accounting_move.create",
        json!({ "journal_id": journal }),
    );
    assert_eq!(mv["date"], TODAY);
    let mv = id(&mv);
    let e = run(&app, &ann, "accounting_move.post", json!({ "id": mv })).unwrap_err();
    assert!(
        e.starts_with("The entry has no amount")
            && e.contains("(assertion total_debit > 0 failed)"),
        "{e}"
    );
    // A line is a debit or a credit: the other side defaults to 0.
    let debit = ok(
        &app,
        &ann,
        "accounting_move_line.create",
        json!({"move_id": mv, "account_id": fees, "debit": 2380}),
    );
    assert_eq!(debit["credit"], 0);
    let debit = id(&debit);
    ok(
        &app,
        &ann,
        "accounting_move_line.create",
        json!({"move_id": mv, "account_id": bank, "credit": 2380}),
    );
    ok(&app, &ann, "accounting_move.post", json!({ "id": mv }));
    let posted = get(&app, &ann, "accounting_move", &mv);
    assert_eq!(posted["state"], "posted");
    assert_eq!(posted["number"], "JE/2026/00001");

    // Posted: frozen except its notes; its lines are frozen with it; never deleted.
    refused(
        &app,
        &ann,
        "accounting_move.update",
        json!({"id": mv, "date": "2026-09-01"}),
        "immutable_record",
    );
    ok(
        &app,
        &ann,
        "accounting_move.update",
        json!({"id": mv, "narration": "Checked"}),
    );
    refused(
        &app,
        &ann,
        "accounting_move_line.update",
        json!({"id": debit, "debit": 1}),
        "immutable_record",
    );
    refused(
        &app,
        &ann,
        "accounting_move_line.create",
        json!({"move_id": mv, "account_id": bank, "debit": 5}),
        "immutable_record",
    );
    refused(
        &app,
        &ann,
        "accounting_move_line.delete",
        json!({ "id": debit }),
        "immutable_record",
    );
    refused(
        &app,
        &ann,
        "accounting_move.delete",
        json!({ "id": mv }),
        "immutable_record",
    );
    // Reversing is the exit.
    ok(&app, &ann, "accounting_move.reverse", json!({ "id": mv }));
    assert_eq!(get(&app, &ann, "accounting_move", &mv)["state"], "reversed");

    // A closed period freezes the entries dated in it, and itself until it is reopened.
    let fy = id(&ok(
        &app,
        &ann,
        "accounting_fiscal_year.create",
        json!({"name": "FY2026", "start_on": "2026-01-01", "end_on": "2026-12-31"}),
    ));
    let march = id(&ok(
        &app,
        &ann,
        "accounting_period.create",
        json!({"fiscal_year_id": fy, "name": "2026-03", "start_on": "2026-03-01", "end_on": "2026-03-31"}),
    ));
    ok(
        &app,
        &ann,
        "accounting_period.close",
        json!({ "id": march }),
    );
    refused(
        &app,
        &ann,
        "accounting_move.create",
        json!({"journal_id": journal, "date": "2026-03-15"}),
        "immutable_record",
    );
    let draft = id(&ok(
        &app,
        &ann,
        "accounting_move.create",
        json!({ "journal_id": journal }),
    ));
    refused(
        &app,
        &ann,
        "accounting_move.update",
        json!({"id": draft, "date": "2026-03-15"}),
        "immutable_record",
    );
    refused(
        &app,
        &ann,
        "accounting_period.update",
        json!({"id": march, "end_on": "2026-03-30"}),
        "immutable_record",
    );
    ok(
        &app,
        &ann,
        "accounting_period.reopen",
        json!({ "id": march }),
    );
    ok(
        &app,
        &ann,
        "accounting_move.update",
        json!({"id": draft, "date": "2026-03-15"}),
    );

    // A bank transaction is matched to a journal item named in the same call.
    let bank_journal = id(&ok(
        &app,
        &ann,
        "accounting_journal.create",
        json!({"name": "Bank", "code": "BNK", "kind": "bank"}),
    ));
    let tx = ok(
        &app,
        &ann,
        "accounting_bank_statement_line.create",
        json!({"journal_id": bank_journal, "label": "Fee", "amount": -2380}),
    );
    assert_eq!(tx["date"], TODAY);
    let tx = id(&tx);
    let e = run(
        &app,
        &ann,
        "accounting_bank_statement_line.match",
        json!({ "id": tx }),
    )
    .unwrap_err();
    assert!(e.contains("move_line_id"), "{e}");
    ok(
        &app,
        &ann,
        "accounting_bank_statement_line.match",
        json!({"id": tx, "move_line_id": debit}),
    );
    let matched = get(&app, &ann, "accounting_bank_statement_line", &tx);
    assert_eq!(matched["state"], "matched");
    assert_eq!(matched["move_line_id"], debit.as_str());
}

#[test]
fn fiscal_years_and_periods_never_cover_the_same_day() {
    let app = app_with(&["accounting"]);
    let ann = user("ann", ACCOUNTANT);
    let year = |name: &str, from: &str, to: &str| {
        run(
            &app,
            &ann,
            "accounting_fiscal_year.create",
            json!({"name": name, "start_on": from, "end_on": to}),
        )
    };
    let fy = id(&year("FY2026", "2026-01-01", "2026-12-31").unwrap());
    // The last day belongs to the year (U9): a year that starts on it overlaps.
    for (from, to) in [
        ("2026-12-31", "2027-12-30"),
        ("2025-01-01", "2026-01-01"),
        ("2026-03-01", "2026-03-31"),
    ] {
        let e = year("Other", from, to).unwrap_err();
        assert!(
            e.contains("rule no_overlapping_years failed"),
            "{from}: {e}"
        );
    }
    let next = id(&year("FY2027", "2027-01-01", "2027-12-31").unwrap());
    let e = run(
        &app,
        &ann,
        "accounting_fiscal_year.update",
        json!({"id": next, "start_on": "2026-12-01"}),
    )
    .unwrap_err();
    assert!(e.contains("Another fiscal year already covers"), "{e}");
    let period = |name: &str, from: &str, to: &str| {
        run(
            &app,
            &ann,
            "accounting_period.create",
            json!({"fiscal_year_id": fy, "name": name, "start_on": from, "end_on": to}),
        )
    };
    period("2026-03", "2026-03-01", "2026-03-31").unwrap();
    period("2026-04", "2026-04-01", "2026-04-30").unwrap();
    let e = period("2026-03b", "2026-03-31", "2026-04-01").unwrap_err();
    assert!(e.contains("rule no_overlapping_periods failed"), "{e}");
}

#[test]
fn an_expense_claim_defaults_to_me_today_and_freezes_once_approved() {
    let app = app_with(&["expenses"]);
    let ann = user("ann", &["expenses.user"]);
    let boss = user("mia", &["expenses.user", "expenses.manager"]);
    let finance = user("fin", &["expenses.user", "expenses.finance"]);
    let claim = ok(
        &app,
        &ann,
        "expenses_expense.create",
        json!({"description": "Taxi", "category": "travel", "amount": 2350}),
    );
    assert_eq!(claim["employee"], "ann");
    assert_eq!(claim["spent_on"], TODAY);
    assert_eq!(claim["currency"], "EUR");
    let c = id(&claim);
    ok(&app, &ann, "expenses_expense.submit", json!({ "id": c }));
    ok(&app, &boss, "expenses_expense.approve", json!({ "id": c }));
    // Approved: the amount is fixed, the receipt note and the payment details are not.
    refused(
        &app,
        &ann,
        "expenses_expense.update",
        json!({"id": c, "amount": 9999}),
        "immutable_record",
    );
    ok(
        &app,
        &ann,
        "expenses_expense.update",
        json!({"id": c, "receipt_note": "Paper receipt in the drawer"}),
    );
    ok(
        &app,
        &finance,
        "expenses_expense.update",
        json!({"id": c, "payment_reference": "TR-0042"}),
    );
    // Paying stamps today as the paid date when none was given.
    ok(&app, &finance, "expenses_expense.pay", json!({ "id": c }));
    let paid = get(&app, &finance, "expenses_expense", &c);
    assert_eq!(paid["state"], "paid");
    assert_eq!(paid["paid_on"], TODAY);
    refused(
        &app,
        &ann,
        "expenses_expense.delete",
        json!({ "id": c }),
        "immutable_record",
    );
}

#[test]
fn stock_changes_only_through_moves_and_a_done_move_is_frozen() {
    let app = app_with(&["inventory"]);
    let root = admin();
    let keeper = user("kim", &["inventory.user", "inventory.manager"]);
    let product = id(&ok(
        &app,
        &root,
        "product.create",
        json!({"name": "Rapid charger"}),
    ));
    let wh = ok(
        &app,
        &keeper,
        "inventory_warehouse.create",
        json!({"name": "Hamburg", "code": "HH1"}),
    );
    assert_eq!(wh["active"], true);
    let shelf = id(&ok(
        &app,
        &keeper,
        "inventory_location.create",
        json!({"name": "Shelf A", "warehouse_id": id(&wh), "kind": "internal"}),
    ));
    // The quantity is the moves' to write, not a person's.
    refused(
        &app,
        &keeper,
        "inventory_item.create",
        json!({"product_id": product, "location_id": shelf, "quantity": 50}),
        "readonly_field",
    );
    let item = ok(
        &app,
        &keeper,
        "inventory_item.create",
        json!({"product_id": product, "location_id": shelf, "reorder_level": 5}),
    );
    assert_eq!(item["quantity"], 0);
    assert_eq!(item["below_reorder"], true);
    let item = id(&item);

    let mv = ok(
        &app,
        &keeper,
        "inventory_move.create",
        json!({"product_id": product, "to_location_id": shelf, "quantity": 8}),
    );
    assert_eq!(mv["scheduled_on"], TODAY);
    let mv = id(&mv);
    ok(
        &app,
        &keeper,
        "inventory_move.validate",
        json!({ "id": mv }),
    );
    let now = get(&app, &keeper, "inventory_item", &item);
    assert_eq!(now["quantity"], 8);
    assert_eq!(now["below_reorder"], false);
    refused(
        &app,
        &keeper,
        "inventory_item.update",
        json!({"id": item, "quantity": 80}),
        "readonly_field",
    );
    ok(
        &app,
        &keeper,
        "inventory_item.update",
        json!({"id": item, "reorder_level": 10}),
    );
    assert_eq!(
        get(&app, &keeper, "inventory_item", &item)["below_reorder"],
        true
    );
    refused(
        &app,
        &keeper,
        "inventory_move.update",
        json!({"id": mv, "quantity": 1}),
        "immutable_record",
    );
}

#[test]
fn a_purchase_line_takes_the_cost_and_a_received_order_is_frozen() {
    let app = app_with(&["inventory", "purchase"]);
    let root = admin();
    let buyer = user(
        "bea",
        &[
            "purchase.user",
            "purchase.manager",
            "inventory.user",
            "product.user",
            "product.cost_viewer",
        ],
    );
    let product = id(&ok(
        &app,
        &root,
        "product.create",
        json!({"name": "Drill", "cost_minor": 4200}),
    ));
    let party = id(&ok(
        &app,
        &root,
        "party.create",
        json!({"name": "Bosch Werkzeug AG"}),
    ));
    let vendor = ok(
        &app,
        &buyer,
        "purchase_vendor.create",
        json!({ "party_id": party }),
    );
    assert_eq!(vendor["active"], true);
    // No vendor reference is needed; the order is dated today in the instance currency.
    let po = ok(
        &app,
        &buyer,
        "purchase_order.create",
        json!({ "vendor_id": id(&vendor) }),
    );
    assert_eq!(po["ordered_on"], TODAY);
    assert_eq!(po["currency"], "EUR");
    let po = id(&po);
    let line = ok(
        &app,
        &buyer,
        "purchase_order_line.create",
        json!({"order_id": po, "product_id": product, "quantity": 3}),
    );
    assert_eq!(line["unit_price"], 4200);
    assert_eq!(line["amount"], 12600);
    let line = id(&line);
    for action in ["send", "confirm", "receive"] {
        ok(
            &app,
            &buyer,
            &format!("purchase_order.{action}"),
            json!({ "id": po }),
        );
    }
    refused(
        &app,
        &buyer,
        "purchase_order.update",
        json!({"id": po, "expected_on": "2026-10-01"}),
        "immutable_record",
    );
    ok(
        &app,
        &buyer,
        "purchase_order.update",
        json!({"id": po, "notes": "Delivered to the back door"}),
    );
    refused(
        &app,
        &buyer,
        "purchase_order_line.update",
        json!({"id": line, "quantity": 30}),
        "immutable_record",
    );
}

#[test]
fn a_manufacturing_order_makes_one_run_and_a_done_order_is_frozen() {
    let app = app_with(&["inventory", "manufacturing"]);
    let root = admin();
    // Finishing makes stock moves, so production staff hold the inventory.user group.
    let lead = user(
        "leo",
        &[
            "manufacturing.user",
            "manufacturing.manager",
            "inventory.user",
        ],
    );
    let product = id(&ok(
        &app,
        &root,
        "product.create",
        json!({"name": "Tool case"}),
    ));
    let bom = ok(
        &app,
        &lead,
        "manufacturing_bom.create",
        json!({"product_id": product, "name": "Standard", "quantity": 4}),
    );
    assert_eq!(bom["active"], true);
    let retired = id(&ok(
        &app,
        &lead,
        "manufacturing_bom.create",
        json!({"product_id": product, "name": "Old", "quantity": 1, "active": false}),
    ));
    refused(
        &app,
        &lead,
        "manufacturing_order.create",
        json!({"bom_id": retired}),
        "ref_not_allowed",
    );
    let mo = ok(
        &app,
        &lead,
        "manufacturing_order.create",
        json!({"bom_id": id(&bom)}),
    );
    assert_eq!(mo["number"], "MO-00001");
    assert_eq!(mo["product_id"], product.as_str());
    assert_eq!(mo["quantity"], 4);
    assert_eq!(mo["planned_on"], TODAY);
    let mo = id(&mo);
    // An order without a production location or components would finish without moving any
    // stock, so it is not confirmed.
    let e = refused(
        &app,
        &lead,
        "manufacturing_order.confirm",
        json!({ "id": mo }),
        "Choose the production location first",
    );
    assert!(e.contains("is_set(location_id)"), "{e}");
    let wh = id(&ok(
        &app,
        &root,
        "inventory_warehouse.create",
        json!({"name": "Plant", "code": "PL1"}),
    ));
    let floor = id(&ok(
        &app,
        &root,
        "inventory_location.create",
        json!({"name": "Floor", "warehouse_id": wh, "kind": "internal"}),
    ));
    ok(
        &app,
        &lead,
        "manufacturing_order.update",
        json!({"id": mo, "location_id": floor}),
    );
    refused(
        &app,
        &lead,
        "manufacturing_order.confirm",
        json!({ "id": mo }),
        "Add the components first",
    );
    ok(
        &app,
        &lead,
        "manufacturing_order_line.create",
        json!({"order_id": mo, "component_id": product, "quantity": 1}),
    );
    // Without a finished product, finishing would take the components out and put nothing in.
    ok(
        &app,
        &lead,
        "manufacturing_order.update",
        json!({"id": mo, "product_id": null}),
    );
    refused(
        &app,
        &lead,
        "manufacturing_order.confirm",
        json!({ "id": mo }),
        "Choose the finished product first",
    );
    ok(
        &app,
        &lead,
        "manufacturing_order.update",
        json!({"id": mo, "product_id": product}),
    );
    for action in ["confirm", "start", "finish"] {
        ok(
            &app,
            &lead,
            &format!("manufacturing_order.{action}"),
            json!({ "id": mo }),
        );
    }
    refused(
        &app,
        &lead,
        "manufacturing_order.update",
        json!({"id": mo, "quantity": 40}),
        "immutable_record",
    );
}

#[test]
fn verifying_records_who_and_when_and_an_approved_vat_report_is_frozen() {
    let app = app_with(&["accounting", "accounting_se"]);
    let ann = user("ann", ACCOUNTANT);
    let company = ok(
        &app,
        &ann,
        "accounting_se_company.create",
        json!({"name": "Nordlicht Sverige AB", "vat_period": "quarterly"}),
    );
    assert_eq!(company["currency"], "SEK");
    assert_eq!(company["fiscal_year_start_month"], 1);
    refused(
        &app,
        &ann,
        "accounting_se_vat_box.create",
        json!({"box": "05", "label": "Momspliktig försäljning", "kind": "base", "verified_by": "ann"}),
        "readonly_field",
    );
    let bx = id(&ok(
        &app,
        &ann,
        "accounting_se_vat_box.create",
        json!({"box": "05", "label": "Momspliktig försäljning", "kind": "base"}),
    ));
    ok(
        &app,
        &ann,
        "accounting_se_vat_box.verify",
        json!({ "id": bx }),
    );
    let verified = get(&app, &ann, "accounting_se_vat_box", &bx);
    assert_eq!(verified["verified_by"], "ann");
    assert_eq!(verified["verified_on"], TODAY);

    let report = id(&ok(
        &app,
        &ann,
        "accounting_se_vat_report.create",
        json!({"name": "2026-Q3", "period_kind": "quarter", "start_on": "2026-07-01", "end_on": "2026-09-30", "company_id": id(&company)}),
    ));
    ok(
        &app,
        &ann,
        "accounting_se_vat_report.propose",
        json!({ "id": report }),
    );
    assert!(
        run(
            &app,
            &ann,
            "accounting_se_vat_report.approve",
            json!({ "id": report })
        )
        .is_err()
    );
    // The confirmation comes with the approval, in one call.
    ok(
        &app,
        &ann,
        "accounting_se_vat_report.approve",
        json!({"id": report, "boxes_confirmed": true}),
    );
    let approved = get(&app, &ann, "accounting_se_vat_report", &report);
    assert_eq!(approved["state"], "approved");
    assert_eq!(approved["boxes_confirmed"], true);
    refused(
        &app,
        &ann,
        "accounting_se_vat_report.update",
        json!({"id": report, "end_on": "2026-10-31"}),
        "immutable_record",
    );
    refused(
        &app,
        &ann,
        "accounting_se_vat_report_line.create",
        json!({"report_id": report, "box_id": bx, "kind": "base", "amount": 100}),
        "immutable_record",
    );
    ok(
        &app,
        &ann,
        "accounting_se_vat_report.update",
        json!({"id": report, "notes": "Filed on paper"}),
    );
    ok(
        &app,
        &ann,
        "accounting_se_vat_report.record_filed",
        json!({ "id": report }),
    );
}

#[test]
fn cancelling_a_confirmed_order_takes_no_correction_in_the_same_update() {
    // D48 with D77: the other fields of an update are edits of the record as it was, and a
    // confirmed order is frozen. The update that cancels it cannot also change a field that is
    // fixed there. Until 2026-11-15 the cancel ran first, in a transaction of its own, and the
    // edit landed on the cancelled order. Two calls do it: cancel, then correct.
    let app = app_with(&[]);
    let sam = user(
        "sam",
        &["base.user", "sales.user", "invoicing.user", "product.user"],
    );
    let party = id(&ok(&app, &sam, "party.create", json!({"name": "Acme"})));
    let order = id(&ok(
        &app,
        &sam,
        "sale_order.create",
        json!({"party_id": party, "currency": "EUR"}),
    ));
    ok(
        &app,
        &sam,
        "sale_order_line.create",
        json!({"order_id": order, "description": "Work", "quantity": 1, "unit_price_minor": 5000}),
    );
    ok(&app, &sam, "sale_order.confirm", json!({ "id": order }));
    let state_and_currency = || {
        let o = get(&app, &sam, "sale_order", &order);
        (
            o["state"].as_str().unwrap_or_default().to_string(),
            o["currency"].as_str().unwrap_or_default().to_string(),
        )
    };
    let e = refused(
        &app,
        &sam,
        "sale_order.update",
        json!({"id": order, "state": "cancelled", "currency": "SEK"}),
        "immutable_record",
    );
    assert!(e.contains("currency"), "{e}");
    assert_eq!(state_and_currency(), ("confirmed".into(), "EUR".into()));
    ok(
        &app,
        &sam,
        "sale_order.update",
        json!({"id": order, "state": "cancelled"}),
    );
    ok(
        &app,
        &sam,
        "sale_order.update",
        json!({"id": order, "currency": "SEK"}),
    );
    assert_eq!(state_and_currency(), ("cancelled".into(), "SEK".into()));
}

/// The stock of `product` at `location`, 0 when there is no stock item.
fn on_hand(app: &App, product: &str, location: &str) -> i64 {
    let r = ok(
        app,
        &admin(),
        "inventory_item.search",
        json!({"filter": [
            {"field": "product_id", "op": "eq", "value": product},
            {"field": "location_id", "op": "eq", "value": location}
        ]}),
    );
    r["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["quantity"].as_i64().unwrap_or(0))
        .sum()
}

fn location(app: &App, kind: &str) -> String {
    let wh = id(&ok(
        app,
        &admin(),
        "inventory_warehouse.create",
        json!({"name": "Hamburg", "code": format!("H{kind}")}),
    ));
    id(&ok(
        app,
        &admin(),
        "inventory_location.create",
        json!({"name": format!("HH/{kind}"), "warehouse_id": wh, "kind": kind}),
    ))
}

#[test]
fn a_short_delivery_receives_what_arrived_and_the_moves_name_the_order() {
    let app = app_with(&["inventory", "purchase"]);
    let root = admin();
    let product = id(&ok(
        &app,
        &root,
        "product.create",
        json!({"name": "Gloves", "cost_minor": 300}),
    ));
    let other = id(&ok(
        &app,
        &root,
        "product.create",
        json!({"name": "Goggles", "cost_minor": 900}),
    ));
    let party = id(&ok(
        &app,
        &root,
        "party.create",
        json!({"name": "PSA GmbH"}),
    ));
    let vendor = id(&ok(
        &app,
        &root,
        "purchase_vendor.create",
        json!({ "party_id": party }),
    ));
    let dock = location(&app, "receiving");
    let po = ok(
        &app,
        &root,
        "purchase_order.create",
        json!({"vendor_id": vendor, "receive_location_id": dock}),
    );
    let number = po["number"].as_str().unwrap().to_string();
    let po = id(&po);
    let short = id(&ok(
        &app,
        &root,
        "purchase_order_line.create",
        json!({"order_id": po, "product_id": product, "quantity": 10}),
    ));
    ok(
        &app,
        &root,
        "purchase_order_line.create",
        json!({"order_id": po, "product_id": other, "quantity": 4}),
    );
    for action in ["send", "confirm"] {
        ok(
            &app,
            &root,
            &format!("purchase_order.{action}"),
            json!({ "id": po }),
        );
    }
    // Seven of the ten gloves came: the line says so before the order is received.
    let line = ok(
        &app,
        &root,
        "purchase_order_line.update",
        json!({"id": short, "received_quantity": 7}),
    );
    assert_eq!(line["receipt_quantity"], 7);
    ok(&app, &root, "purchase_order.receive", json!({ "id": po }));
    let moves = ok(
        &app,
        &root,
        "inventory_move.search",
        json!({"filter": [{"field": "origin", "op": "eq", "value": number}]}),
    );
    let rows = moves["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    for m in rows {
        assert_eq!(m["kind"], "receipt");
        assert_eq!(m["state"], "draft");
        assert_eq!(m["to_location_id"], dock.as_str());
        let want = if m["product_id"] == product.as_str() {
            7
        } else {
            4
        };
        assert_eq!(m["quantity"], want);
        ok(
            &app,
            &root,
            "inventory_move.validate",
            json!({"id": m["id"]}),
        );
    }
    assert_eq!(on_hand(&app, &product, &dock), 7);
    assert_eq!(on_hand(&app, &other, &dock), 4);
}

#[test]
fn finishing_a_manufacturing_order_uses_its_components_and_adds_the_product() {
    let app = app_with(&["inventory", "manufacturing"]);
    let root = admin();
    let kit = id(&ok(
        &app,
        &root,
        "product.create",
        json!({"name": "Tool kit"}),
    ));
    let case = id(&ok(&app, &root, "product.create", json!({"name": "Case"})));
    let bits = id(&ok(
        &app,
        &root,
        "product.create",
        json!({"name": "Bit set"}),
    ));
    let floor = location(&app, "internal");
    // 20 cases on the floor, from a receipt.
    let m = id(&ok(
        &app,
        &root,
        "inventory_move.create",
        json!({"product_id": case, "to_location_id": floor, "quantity": 20, "kind": "receipt"}),
    ));
    ok(&app, &root, "inventory_move.validate", json!({ "id": m }));
    let bom = id(&ok(
        &app,
        &root,
        "manufacturing_bom.create",
        json!({"product_id": kit, "name": "Standard", "quantity": 1}),
    ));
    let mo = ok(
        &app,
        &root,
        "manufacturing_order.create",
        json!({"bom_id": bom, "quantity": 5, "location_id": floor}),
    );
    assert_eq!(mo["product_id"], kit.as_str());
    let mo = id(&mo);
    ok(
        &app,
        &root,
        "manufacturing_order_line.bulk_create",
        json!({"rows": [
            {"order_id": mo, "component_id": case, "quantity": 5},
            {"order_id": mo, "component_id": bits, "quantity": 10}
        ]}),
    );
    for action in ["confirm", "start", "finish"] {
        ok(
            &app,
            &root,
            &format!("manufacturing_order.{action}"),
            json!({ "id": mo }),
        );
    }
    // Finishing made a draft move per component and one for the product, named after the order;
    // the stock changes when the warehouse validates them.
    let number = get(&app, &root, "manufacturing_order", &mo)["number"]
        .as_str()
        .unwrap()
        .to_string();
    let moves = ok(
        &app,
        &root,
        "inventory_move.search",
        json!({"filter": [{"field": "origin", "op": "eq", "value": number}]}),
    );
    let moves = moves["rows"].as_array().unwrap();
    assert_eq!(moves.len(), 3);
    assert_eq!(on_hand(&app, &kit, &floor), 0);
    for m in moves {
        assert_eq!(m["kind"], "production");
        assert_eq!(m["state"], "draft");
        ok(
            &app,
            &root,
            "inventory_move.validate",
            json!({"id": m["id"]}),
        );
    }
    assert_eq!(on_hand(&app, &kit, &floor), 5);
    assert_eq!(on_hand(&app, &case, &floor), 15);
    // Bits were not in stock: the floor now owes ten (negative stock is allowed, as in moves).
    assert_eq!(on_hand(&app, &bits, &floor), -10);
    // The components of a done order are frozen with it.
    let line = ok(
        &app,
        &root,
        "manufacturing_order_line.search",
        json!({"filter": [{"field": "order_id", "op": "eq", "value": mo}]}),
    )["rows"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    refused(
        &app,
        &root,
        "manufacturing_order_line.update",
        json!({"id": line, "quantity": 1}),
        "immutable_record",
    );
}
