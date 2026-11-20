//! `invoice_credit.create` over invented accounts and a made-up tax: nothing here is a real chart
//! or rate and none of it is checked against Swedish credit note rules.

use crate::app::App;
use obop_commands::{ADMIN_GROUP, CallError, Principal, Surface, Value, Vault};
use obop_pkg::{Bundled, Marketplace};
use serde_json::{Value as J, json};

const SEPTEMBER_2026: i64 = 1_789_400_000;

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

type Row = serde_json::Map<String, J>;

fn rows(a: &App, model: &str) -> Vec<(String, Row)> {
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
}

fn fixture() -> Fx {
    let app = App::new();
    app.dispatcher
        .set_clock(std::sync::Arc::new(|| SEPTEMBER_2026));
    for p in ["accounting", "accounting_se", "projects", "timesheets"] {
        app.installer
            .install(&Bundled.fetch(p).unwrap(), "marketplace", "test")
            .unwrap();
    }
    let acc = |code: &str, t: &str| {
        create(
            &app,
            "accounting_account",
            json!({"code": code, "name": format!("Test {code}"), "account_type": t}),
        )
    };
    acc("1510", "asset_receivable");
    acc("3001", "income");
    let vat_acc = acc("2611", "liability_current");
    create(
        &app,
        "accounting_tax",
        json!({"name": "Test tax", "rate_bp": 2500, "scope": "sales", "tax_account_id": vat_acc}),
    );
    create(
        &app,
        "accounting_journal",
        json!({"name": "Sales", "code": "S", "kind": "sale"}),
    );
    create(&app, "party", json!({"name": "Acme AB"}));
    Fx { app }
}

impl Fx {
    fn invoice(&self, amounts: &[i64], post: bool) -> String {
        let party = rows(&self.app, "party")[0].0.clone();
        let inv = create(
            &self.app,
            "invoice",
            json!({"state": "draft", "currency": "SEK", "party_id": party}),
        );
        for (n, a) in amounts.iter().enumerate() {
            create(
                &self.app,
                "invoice_line",
                json!({"invoice_id": inv, "description": format!("Item {n}"), "quantity": 2, "unit_price_minor": a}),
            );
        }
        if post {
            call(&self.app, &admin(), "invoice.post", json!({"id": inv})).unwrap();
        }
        inv
    }

    fn book(&self, inv: &str) {
        call(
            &self.app,
            &admin(),
            "invoice_posting.create_move",
            json!({"invoice": inv, "journal": "S", "receivable_account": "1510",
                   "revenue_account": "3001", "tax": "Test tax", "date": "2026-09-14"}),
        )
        .unwrap();
    }

    /// A posted and booked invoice.
    fn booked(&self, amounts: &[i64]) -> String {
        let inv = self.invoice(amounts, true);
        self.book(&inv);
        inv
    }

    fn credit(&self, inv: &str, dry: bool) -> Result<J, CallError> {
        call(
            &self.app,
            &admin(),
            "invoice_credit.create",
            json!({"invoice": inv, "dry_run": dry}),
        )
    }
}

#[test]
fn it_credits_a_booked_invoice_with_a_draft_invoice_and_a_reversing_draft_move() {
    let f = fixture();
    let inv = f.booked(&[10_000, 2_001]);
    let r = f.credit(&inv, false).unwrap();
    assert_eq!(r["written"], json!(true));
    assert_eq!(r["net"], json!(-(2 * 10_000 + 2 * 2_001)));
    let cid = r["credit_invoice_id"].as_str().unwrap();
    let mid = r["move_id"].as_str().unwrap();

    let invs = rows(&f.app, "invoice");
    assert_eq!(invs.len(), 2);
    let credit = invs.iter().find(|(id, _)| id == cid).unwrap();
    assert_eq!(credit.1["state"], json!("draft"));
    assert_eq!(credit.1["currency"], json!("SEK"));
    let orig = invs.iter().find(|(id, _)| *id == inv).unwrap();
    assert_eq!(
        orig.1["state"],
        json!("posted"),
        "the original is untouched"
    );
    assert_eq!(credit.1["party_id"], orig.1["party_id"]);
    let cl: Vec<_> = rows(&f.app, "invoice_line")
        .into_iter()
        .filter(|(_, l)| l["invoice_id"] == json!(cid))
        .collect();
    assert_eq!(cl.len(), 2);
    assert!(cl.iter().all(|(_, l)| l["quantity"] == json!(2)));
    let mut prices: Vec<i64> = cl
        .iter()
        .map(|(_, l)| l["unit_price_minor"].as_i64().unwrap())
        .collect();
    prices.sort();
    assert_eq!(prices, vec![-10_000, -2_001]);

    let moves = rows(&f.app, "accounting_move");
    assert_eq!(moves.len(), 2);
    let rev = moves.iter().find(|(id, _)| id == mid).unwrap();
    assert_eq!(rev.1["state"], json!("draft"));
    assert_eq!(rev.1["date"], json!("2026-09-14"), "the original date");
    let lines_of = |m: &str| -> Vec<Row> {
        rows(&f.app, "accounting_move_line")
            .into_iter()
            .filter(|(_, l)| l["move_id"] == json!(m))
            .map(|(_, l)| l)
            .collect()
    };
    let orig_mid = rows(&f.app, "invoice_posting")[0].1["move_id"]
        .as_str()
        .unwrap()
        .to_string();
    let (o, n) = (lines_of(&orig_mid), lines_of(mid));
    assert_eq!(o.len(), n.len());
    let sum = |v: &[Row], k: &str| -> i64 { v.iter().map(|l| l[k].as_i64().unwrap()).sum() };
    assert_eq!(sum(&n, "debit"), sum(&o, "credit"));
    assert_eq!(sum(&n, "credit"), sum(&o, "debit"));
    assert_eq!(sum(&n, "debit"), sum(&n, "credit"), "balanced");
    assert_eq!(
        n.iter().filter(|l| l.contains_key("tax_id")).count(),
        o.iter().filter(|l| l.contains_key("tax_id")).count()
    );
    // The link row.
    let link = rows(&f.app, "invoice_credit");
    assert_eq!(link.len(), 1);
    assert_eq!(link[0].1["original_id"], json!(inv));
    assert_eq!(link[0].1["credit_invoice_id"], json!(cid));
    assert_eq!(link[0].1["move_id"], json!(mid));
}

#[test]
fn a_second_credit_and_a_credit_of_a_credit_are_refused_and_write_nothing() {
    let f = fixture();
    let inv = f.booked(&[10_000]);
    let r = f.credit(&inv, false).unwrap();
    let cid = r["credit_invoice_id"].as_str().unwrap().to_string();
    let m = refusal(f.credit(&inv, false));
    assert!(m.contains("already has a credit"), "{m}");
    let m = refusal(f.credit(&cid, false));
    assert!(m.contains("cannot be credited"), "{m}");
    assert_eq!(rows(&f.app, "invoice").len(), 2);
    assert_eq!(rows(&f.app, "accounting_move").len(), 2);
    assert_eq!(rows(&f.app, "invoice_credit").len(), 1);
}

#[test]
fn a_draft_an_unbooked_and_an_unknown_invoice_are_refused() {
    let f = fixture();
    let draft = f.invoice(&[10_000], false);
    let m = refusal(f.credit(&draft, false));
    assert!(m.contains("only a posted invoice"), "{m}");
    let posted = f.invoice(&[10_000], true);
    let m = refusal(f.credit(&posted, false));
    assert!(m.contains("no ledger move"), "{m}");
    let m = refusal(f.credit("nope", false));
    assert!(m.contains("no invoice"), "{m}");
    let m = refusal(call(&f.app, &admin(), "invoice_credit.create", json!({})));
    assert!(m.contains("required"), "{m}");
    assert!(rows(&f.app, "invoice_credit").is_empty());
    assert_eq!(rows(&f.app, "invoice").len(), 2);
}

#[test]
fn the_invoice_can_be_named_by_its_number() {
    let f = fixture();
    let inv = f.booked(&[10_000]);
    let number = rows(&f.app, "invoice")
        .into_iter()
        .find(|(id, _)| *id == inv)
        .unwrap()
        .1["number"]
        .as_str()
        .unwrap()
        .to_string();
    let r = f.credit(&number, false).unwrap();
    assert_eq!(r["written"], json!(true));
    assert_eq!(
        rows(&f.app, "invoice_credit")[0].1["original_id"],
        json!(inv)
    );
}

#[test]
fn a_cancelled_original_move_is_refused() {
    let f = fixture();
    let inv = f.booked(&[10_000]);
    let mid = rows(&f.app, "invoice_posting")[0].1["move_id"]
        .as_str()
        .unwrap()
        .to_string();
    call(
        &f.app,
        &admin(),
        "accounting_move.cancel",
        json!({"id": mid}),
    )
    .unwrap();
    let m = refusal(f.credit(&inv, false));
    assert!(m.contains("cancelled"), "{m}");
    assert!(rows(&f.app, "invoice_credit").is_empty());
}

#[test]
fn a_dry_run_writes_nothing_and_an_agent_may_only_dry_run() {
    let f = fixture();
    let inv = f.booked(&[10_000]);
    let dry = f.credit(&inv, true).unwrap();
    assert_eq!(dry["written"], json!(false));
    assert_eq!(dry["net"], json!(-20_000));
    assert_eq!(dry["lines"].as_array().unwrap().len(), 3);
    assert!(rows(&f.app, "invoice_credit").is_empty());
    assert_eq!(rows(&f.app, "invoice").len(), 1);
    let agent = Principal::agent(
        "bot",
        &["invoicing.user", "accounting.user"],
        &["invoicing.user", "accounting.user"],
    );
    call(
        &f.app,
        &agent,
        "invoice_credit.create",
        json!({"invoice": inv, "dry_run": true}),
    )
    .unwrap();
    let m = refusal(call(
        &f.app,
        &agent,
        "invoice_credit.create",
        json!({"invoice": inv}),
    ));
    assert!(m.contains("only dry-run"), "{m}");
    assert!(rows(&f.app, "invoice_credit").is_empty());
}

#[test]
fn the_reversing_move_is_only_a_draft_and_posting_stays_governed() {
    let f = fixture();
    let inv = f.booked(&[10_000]);
    let mid = f.credit(&inv, false).unwrap()["move_id"]
        .as_str()
        .unwrap()
        .to_string();
    let clerk = Principal::user("clerk", &["accounting.user"]);
    assert!(call(&f.app, &clerk, "accounting_move.post", json!({"id": mid})).is_err());
    let agent = Principal::agent("bot", &["accounting.approver"], &["accounting.approver"]);
    let r = call(&f.app, &agent, "accounting_move.post", json!({"id": mid}));
    assert!(
        matches!(r, Err(CallError::ApprovalRequired { .. })),
        "{r:?}"
    );
}

#[test]
fn a_user_without_the_ledger_group_is_refused() {
    let f = fixture();
    let inv = f.booked(&[10_000]);
    let clerk = Principal::user("clerk", &["invoicing.user"]);
    let r = call(
        &f.app,
        &clerk,
        "invoice_credit.create",
        json!({"invoice": inv}),
    );
    assert!(r.is_err(), "{r:?}");
    assert!(rows(&f.app, "invoice_credit").is_empty());
}

#[test]
fn crediting_a_time_billed_invoice_frees_its_entries() {
    let f = fixture();
    let project = create(
        &f.app,
        "projects_project",
        json!({"name": "Website", "code": "WEB"}),
    );
    let task = create(
        &f.app,
        "projects_task",
        json!({"title": "Design", "project_id": project}),
    );
    for day in ["2026-09-02", "2026-09-03"] {
        let id = create(
            &f.app,
            "timesheets_entry",
            json!({"task_id": task, "worker": "ann", "worked_on": day, "minutes": 90,
                   "billable": true, "state": "draft"}),
        );
        call(
            &f.app,
            &admin(),
            "timesheets_entry.submit",
            json!({"id": id}),
        )
        .unwrap();
        call(
            &f.app,
            &admin(),
            "timesheets_entry.approve",
            json!({"id": id}),
        )
        .unwrap();
    }
    let bill = json!({"project": "WEB", "customer": "Acme AB", "from": "2026-09-01",
        "to": "2026-09-30", "hourly_rate_minor": 90_000});
    let r = call(
        &f.app,
        &admin(),
        "time_billing.create_invoice",
        bill.clone(),
    )
    .unwrap();
    let inv = r["invoice_id"].as_str().unwrap().to_string();
    call(&f.app, &admin(), "invoice.post", json!({"id": inv})).unwrap();
    f.book(&inv);
    assert_eq!(rows(&f.app, "time_billing").len(), 2);
    // Nothing left to bill while the invoice stands.
    assert!(
        call(
            &f.app,
            &admin(),
            "time_billing.create_invoice",
            bill.clone()
        )
        .is_err()
    );

    let c = f.credit(&inv, false).unwrap();
    assert_eq!(c["freed_entries"], json!(2));
    assert!(
        c["notes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n.as_str().unwrap().contains("freed"))
    );
    assert!(rows(&f.app, "time_billing").is_empty());
    let again = call(&f.app, &admin(), "time_billing.create_invoice", bill).unwrap();
    assert_eq!(
        again["entries"],
        json!(2),
        "the entries can be billed again"
    );
}

#[test]
fn the_credit_invoice_can_be_posted_and_a_zero_invoice_cannot() {
    let f = fixture();
    let inv = f.booked(&[10_000]);
    let r = f.credit(&inv, false).unwrap();
    let cid = r["credit_invoice_id"].as_str().unwrap().to_string();
    let posted = call(&f.app, &admin(), "invoice.post", json!({"id": cid})).unwrap();
    assert_eq!(posted["state"], json!("posted"));
    assert!(posted["number"].as_str().is_some_and(|n| !n.is_empty()));
    // An invoice without any amount still cannot be posted.
    let empty = f.invoice(&[], false);
    let m = refusal(call(&f.app, &admin(), "invoice.post", json!({"id": empty})));
    assert!(m.contains("Add at least one line"), "{m}");
}
