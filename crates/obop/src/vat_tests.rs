//! `vat_computation.compute` over invented accounts, codes and boxes: nothing here is a real
//! chart, rate or box and none of it is checked against Skatteverket.

use crate::app::App;
use obop_commands::{ADMIN_GROUP, CallError, Principal, Surface, Value, Vault};
use obop_pkg::{Bundled, Marketplace};
use serde_json::{Value as J, json};

/// 2026-09-14.
const SEPTEMBER_2026: i64 = 1_789_400_000;

fn app() -> App {
    let app = App::new();
    app.dispatcher
        .set_clock(std::sync::Arc::new(|| SEPTEMBER_2026));
    for p in ["accounting", "accounting_se"] {
        app.installer
            .install(&Bundled.fetch(p).unwrap(), "marketplace", "test")
            .unwrap();
    }
    app
}

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

struct Fixture {
    app: App,
    report: String,
    box05: String,
    box10: String,
    tax: String,
    sales: String,
    vat_acc: String,
    receivable: String,
    journal: String,
}

/// A mapping the user would create: box 05 (base) and 10 (output), one code on one tax.
fn fixture() -> Fixture {
    let app = app();
    let bx = |n: &str, kind: &str| {
        create(
            &app,
            "accounting_se_vat_box",
            json!({"box": n, "label": format!("Box {n}"), "kind": kind}),
        )
    };
    let (box05, box10) = (bx("05", "base"), bx("10", "output"));
    let acc = |code: &str, t: &str| {
        create(
            &app,
            "accounting_account",
            json!({"code": code, "name": format!("Test {code}"), "account_type": t}),
        )
    };
    let (sales, vat_acc, receivable) = (
        acc("3001", "income"),
        acc("2611", "liability_current"),
        acc("1510", "asset_receivable"),
    );
    let tax = create(
        &app,
        "accounting_tax",
        json!({"name": "Test tax", "rate_bp": 2500, "scope": "sales"}),
    );
    create(
        &app,
        "accounting_se_vat_code",
        json!({"code": "T1", "label": "Test code", "treatment": "sale_domestic", "rate_bp": 2500,
               "tax_id": tax, "base_box_id": box05, "base_side": "credit",
               "vat_box_id": box10, "vat_account_id": vat_acc, "vat_side": "credit"}),
    );
    let journal = create(
        &app,
        "accounting_journal",
        json!({"name": "Sales", "code": "S", "kind": "sale"}),
    );
    let report = create(
        &app,
        "accounting_se_vat_report",
        json!({"name": "2026-Q3", "period_kind": "quarter", "start_on": "2026-07-01", "end_on": "2026-09-30"}),
    );
    Fixture {
        app,
        report,
        box05,
        box10,
        tax,
        sales,
        vat_acc,
        receivable,
        journal,
    }
}

impl Fixture {
    /// A sale move: receivable debit, base credit with the tax, VAT credit with or without it.
    fn sale(&self, date: &str, net: i64, vat: i64, tag_vat: bool, post: bool) -> String {
        let m = create(
            &self.app,
            "accounting_move",
            json!({"journal_id": self.journal, "date": date, "reference": format!("R{net}")}),
        );
        let line = |acc: &str, dr: i64, cr: i64, tax: Option<&str>| {
            let mut v = json!({"move_id": m, "account_id": acc, "debit": dr, "credit": cr});
            if let Some(t) = tax {
                v["tax_id"] = json!(t);
            }
            create(&self.app, "accounting_move_line", v);
        };
        line(&self.receivable, net + vat, 0, None);
        line(&self.sales, 0, net, Some(&self.tax));
        line(&self.vat_acc, 0, vat, tag_vat.then_some(self.tax.as_str()));
        if post {
            call(
                &self.app,
                &admin(),
                "accounting_move.post",
                json!({"id": m}),
            )
            .unwrap();
        }
        m
    }

    fn compute(&self, dry: bool) -> Result<J, CallError> {
        call(
            &self.app,
            &admin(),
            "vat_computation.compute",
            json!({"report": "2026-Q3", "dry_run": dry}),
        )
    }

    fn report_lines(&self) -> Vec<(String, i64, String)> {
        let mut v: Vec<_> = rows(&self.app, "accounting_se_vat_report_line")
            .into_iter()
            .filter(|(_, f)| f["report_id"] == json!(self.report))
            .map(|(_, f)| {
                (
                    f["box_id"].as_str().unwrap().to_string(),
                    f["amount"].as_i64().unwrap(),
                    f["vouchers"].as_str().unwrap_or("").to_string(),
                )
            })
            .collect();
        v.sort();
        v
    }
}

fn handler_error(r: Result<J, CallError>) -> String {
    match r {
        Err(CallError::Handler(m)) => m,
        other => panic!("expected a refusal in words, got {other:?}"),
    }
}

#[test]
fn it_sums_posted_moves_of_the_period_by_the_codes_of_the_user() {
    let f = fixture();
    f.sale("2026-07-05", 10_000, 2_500, true, true);
    f.sale("2026-09-20", 4_000, 1_000, false, true); // VAT line without a tax: found by the move
    f.sale("2026-08-01", 99_999, 24_999, true, false); // a draft does not count
    f.sale("2026-10-01", 7_000, 1_750, true, true); // outside the period
    let r = f.compute(false).unwrap();
    assert_eq!(r["written"], json!(true));
    assert_eq!(r["moves"], json!(2));
    assert_eq!(r["output_vat"], json!(3_500));
    assert_eq!(r["vat_to_pay"], json!(3_500));
    let lines = f.report_lines();
    assert_eq!(lines.len(), 2);
    let get = |b: &str| lines.iter().find(|l| l.0 == b).unwrap();
    assert_eq!(get(&f.box05).1, 14_000);
    assert_eq!(get(&f.box10).1, 3_500);
    assert_eq!(
        get(&f.box05).2.lines().count(),
        2,
        "both vouchers are named"
    );
    // The report stays a draft, unconfirmed, and its computed totals follow the lines.
    let rep = &rows(&f.app, "accounting_se_vat_report")[0].1;
    assert_eq!(rep["state"], json!("draft"));
    assert_ne!(rep.get("boxes_confirmed"), Some(&json!(true)));
}

#[test]
fn a_rerun_replaces_the_lines_and_a_dry_run_writes_nothing() {
    let f = fixture();
    f.sale("2026-07-05", 10_000, 2_500, true, true);
    let dry = f.compute(true).unwrap();
    assert_eq!(dry["written"], json!(false));
    assert_eq!(dry["lines"].as_array().unwrap().len(), 2);
    assert!(f.report_lines().is_empty());
    f.compute(false).unwrap();
    let first = f.report_lines();
    f.compute(false).unwrap();
    assert_eq!(f.report_lines(), first, "same lines, not doubled");
    f.sale("2026-08-05", 1_000, 250, true, true);
    f.compute(false).unwrap();
    let after = f.report_lines();
    assert_eq!(after.len(), 2);
    assert_eq!(after.iter().find(|l| l.0 == f.box05).unwrap().1, 11_000);
}

#[test]
fn a_report_that_is_not_a_draft_is_refused_and_keeps_its_lines() {
    let f = fixture();
    f.sale("2026-07-05", 10_000, 2_500, true, true);
    f.compute(false).unwrap();
    let before = f.report_lines();
    call(
        &f.app,
        &admin(),
        "accounting_se_vat_report.propose",
        json!({"id": f.report}),
    )
    .unwrap();
    f.sale("2026-08-05", 1_000, 250, true, true);
    let m = handler_error(f.compute(false));
    assert!(m.contains("proposed") && m.contains("draft"), "{m}");
    assert_eq!(f.report_lines(), before);
}

#[test]
fn missing_codes_boxes_and_reports_are_refused_in_words() {
    let a = app();
    create(
        &a,
        "accounting_se_vat_report",
        json!({"name": "2026-Q3", "period_kind": "quarter", "start_on": "2026-07-01", "end_on": "2026-09-30"}),
    );
    let run = |name: &str| {
        call(
            &a,
            &admin(),
            "vat_computation.compute",
            json!({"report": name, "dry_run": true}),
        )
    };
    assert!(handler_error(run("2026-Q3")).contains("no VAT code maps to a box"));
    assert!(handler_error(run("nope")).contains("no VAT report"));
    // A code that maps to a box but names no tax is refused too.
    let bx = create(
        &a,
        "accounting_se_vat_box",
        json!({"box": "05", "label": "b", "kind": "base"}),
    );
    create(
        &a,
        "accounting_se_vat_code",
        json!({"code": "X", "label": "x", "treatment": "none", "rate_bp": 0,
               "base_box_id": bx, "base_side": "credit"}),
    );
    assert!(handler_error(run("2026-Q3")).contains("no tax_id"));
}

#[test]
fn an_amount_in_a_box_marked_not_used_is_refused() {
    let f = fixture();
    f.sale("2026-07-05", 10_000, 2_500, true, true);
    call(
        &f.app,
        &admin(),
        "accounting_se_vat_box.update",
        json!({"id": f.box05, "not_used": true}),
    )
    .unwrap();
    let m = handler_error(f.compute(false));
    assert!(m.contains("not used"), "{m}");
    assert!(f.report_lines().is_empty());
}

#[test]
fn an_untagged_vat_line_that_fits_no_single_code_is_refused() {
    let f = fixture();
    let m = create(
        &f.app,
        "accounting_move",
        json!({"journal_id": f.journal, "date": "2026-07-06"}),
    );
    for (acc, dr, cr) in [(&f.receivable, 250, 0), (&f.vat_acc, 0, 250)] {
        create(
            &f.app,
            "accounting_move_line",
            json!({"move_id": m, "account_id": acc, "debit": dr, "credit": cr}),
        );
    }
    call(&f.app, &admin(), "accounting_move.post", json!({"id": m})).unwrap();
    let e = handler_error(f.compute(false));
    assert!(e.contains("exactly one code"), "{e}");
}

#[test]
fn an_agent_may_only_dry_run_and_a_clerk_needs_the_group() {
    let f = fixture();
    f.sale("2026-07-05", 10_000, 2_500, true, true);
    let agent = Principal::agent("bot", &["accounting.user"], &["accounting.user"]);
    let input = |dry: bool| json!({"report": "2026-Q3", "dry_run": dry});
    let m = handler_error(call(
        &f.app,
        &agent,
        "vat_computation.compute",
        input(false),
    ));
    assert!(m.contains("dry-run"), "{m}");
    assert!(f.report_lines().is_empty());
    let r = call(&f.app, &agent, "vat_computation.compute", input(true)).unwrap();
    assert_eq!(r["output_vat"], json!(2_500));
    assert!(f.report_lines().is_empty());
    let nobody = Principal::user("nobody", &["base.user"]);
    assert!(matches!(
        call(&f.app, &nobody, "vat_computation.compute", input(true)),
        Err(CallError::Forbidden(_))
    ));
}
