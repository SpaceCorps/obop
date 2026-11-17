//! `invoice_posting.create_move` over invented accounts and a made-up tax: nothing here is a real
//! chart or rate and none of it is checked against Swedish invoice or VAT rules.

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
    tax: String,
    vat_acc: String,
}

fn fixture(rate_bp: i64) -> Fx {
    let app = App::new();
    app.dispatcher
        .set_clock(std::sync::Arc::new(|| SEPTEMBER_2026));
    for p in ["accounting", "accounting_se"] {
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
    let tax = create(
        &app,
        "accounting_tax",
        json!({"name": "Test tax", "rate_bp": rate_bp, "scope": "sales", "tax_account_id": vat_acc}),
    );
    create(
        &app,
        "accounting_journal",
        json!({"name": "Sales", "code": "S", "kind": "sale"}),
    );
    Fx { app, tax, vat_acc }
}

impl Fx {
    /// An invoice with the given line amounts (quantity 1), posted or left as a draft.
    fn invoice(&self, amounts: &[i64], post: bool) -> String {
        let inv = create(
            &self.app,
            "invoice",
            json!({"state": "draft", "currency": "SEK"}),
        );
        for (n, a) in amounts.iter().enumerate() {
            create(
                &self.app,
                "invoice_line",
                json!({"invoice_id": inv, "description": format!("Item {n}"), "quantity": 1, "unit_price_minor": a}),
            );
        }
        if post {
            call(&self.app, &admin(), "invoice.post", json!({"id": inv})).unwrap();
        }
        inv
    }

    fn input(&self, inv: &str, dry: bool) -> J {
        json!({"invoice": inv, "journal": "S", "receivable_account": "1510",
               "revenue_account": "3001", "tax": "Test tax", "date": "2026-09-14", "dry_run": dry})
    }

    fn run(&self, inv: &str, dry: bool) -> Result<J, CallError> {
        call(
            &self.app,
            &admin(),
            "invoice_posting.create_move",
            self.input(inv, dry),
        )
    }
}

#[test]
fn it_books_a_posted_invoice_as_a_balanced_draft_move() {
    let f = fixture(2500);
    let inv = f.invoice(&[10_000, 2_001], true);
    let r = f.run(&inv, false).unwrap();
    assert_eq!(r["written"], json!(true));
    assert_eq!(r["net"], json!(12_001));
    assert_eq!(r["vat"], json!(3_000));
    let mid = r["move_id"].as_str().unwrap();
    let mv = rows(&f.app, "accounting_move");
    assert_eq!(mv.len(), 1);
    assert_eq!(mv[0].1["state"], json!("draft"));
    assert!(mv[0].1["reference"].as_str().unwrap().starts_with("INV-"));
    let lines: Vec<_> = rows(&f.app, "accounting_move_line")
        .into_iter()
        .filter(|(_, l)| l["move_id"] == json!(mid))
        .map(|(_, l)| l)
        .collect();
    assert_eq!(lines.len(), 4, "receivable, two revenue lines, VAT");
    let dr: i64 = lines.iter().map(|l| l["debit"].as_i64().unwrap()).sum();
    let cr: i64 = lines.iter().map(|l| l["credit"].as_i64().unwrap()).sum();
    assert_eq!((dr, cr), (15_001, 15_001));
    let vat = lines
        .iter()
        .find(|l| l["account_id"] == json!(f.vat_acc))
        .unwrap();
    assert_eq!(
        vat["credit"],
        json!(3_000),
        "12001 * 25% = 3000.25, rounded half up"
    );
    assert_eq!(vat["tax_id"], json!(f.tax));
    assert_eq!(
        lines
            .iter()
            .filter(|l| l.get("tax_id") == Some(&json!(f.tax)))
            .count(),
        3
    );
    // The link exists and a second run is refused without writing anything.
    assert_eq!(rows(&f.app, "invoice_posting").len(), 1);
    let m = refusal(f.run(&inv, false));
    assert!(m.contains("already has a move"), "{m}");
    assert_eq!(rows(&f.app, "accounting_move").len(), 1);
    assert_eq!(rows(&f.app, "accounting_move_line").len(), 4);
}

#[test]
fn the_move_is_only_a_draft_and_posting_stays_governed() {
    let f = fixture(2500);
    let inv = f.invoice(&[10_000], true);
    let mid = f.run(&inv, false).unwrap()["move_id"]
        .as_str()
        .unwrap()
        .to_string();
    let clerk = Principal::user("clerk", &["accounting.user"]);
    assert!(
        call(&f.app, &clerk, "accounting_move.post", json!({"id": mid})).is_err(),
        "a clerk cannot post"
    );
    let agent = Principal::agent("bot", &["accounting.approver"], &["accounting.approver"]);
    let r = call(&f.app, &agent, "accounting_move.post", json!({"id": mid}));
    assert!(
        matches!(r, Err(CallError::ApprovalRequired { .. })),
        "{r:?}"
    );
    assert_eq!(
        rows(&f.app, "accounting_move")[0].1["state"],
        json!("draft")
    );
}

#[test]
fn a_dry_run_writes_nothing_and_an_agent_may_only_dry_run() {
    let f = fixture(2500);
    let inv = f.invoice(&[10_000], true);
    let dry = f.run(&inv, true).unwrap();
    assert_eq!(dry["written"], json!(false));
    assert_eq!(dry["lines"].as_array().unwrap().len(), 3);
    assert!(rows(&f.app, "accounting_move").is_empty());
    assert!(rows(&f.app, "invoice_posting").is_empty());
    let agent = Principal::agent("bot", &["accounting.user"], &["accounting.user"]);
    let m = refusal(call(
        &f.app,
        &agent,
        "invoice_posting.create_move",
        f.input(&inv, false),
    ));
    assert!(m.contains("dry-run"), "{m}");
    call(
        &f.app,
        &agent,
        "invoice_posting.create_move",
        f.input(&inv, true),
    )
    .unwrap();
    assert!(rows(&f.app, "accounting_move").is_empty());
    let nobody = Principal::user("nobody", &["base.user"]);
    assert!(matches!(
        call(
            &f.app,
            &nobody,
            "invoice_posting.create_move",
            f.input(&inv, true)
        ),
        Err(CallError::Forbidden(_))
    ));
}

#[test]
fn missing_configuration_and_bad_invoices_are_refused_in_words() {
    let f = fixture(2500);
    let draft = f.invoice(&[10_000], false);
    let m = refusal(f.run(&draft, false));
    assert!(m.contains("draft") && m.contains("posted"), "{m}");
    let posted = f.invoice(&[10_000], true);
    for key in ["journal", "receivable_account", "revenue_account", "tax"] {
        let mut i = f.input(&posted, true);
        i.as_object_mut().unwrap().remove(key);
        let m = refusal(call(&f.app, &admin(), "invoice_posting.create_move", i));
        assert!(m.contains(key), "{key}: {m}");
    }
    let mut i = f.input(&posted, true);
    i["date"] = json!("14.09.2026");
    let m = refusal(call(&f.app, &admin(), "invoice_posting.create_move", i));
    assert!(m.contains("YYYY-MM-DD"), "{m}");
    let mut bad = f.input(&posted, true);
    bad["revenue_account"] = json!("9999");
    assert!(refusal(call(&f.app, &admin(), "invoice_posting.create_move", bad)).contains("9999"));
    // A tax with no account and no vat_account input.
    create(
        &f.app,
        "accounting_tax",
        json!({"name": "No account", "rate_bp": 600, "scope": "sales"}),
    );
    let mut i = f.input(&posted, true);
    i["tax"] = json!("No account");
    assert!(
        refusal(call(&f.app, &admin(), "invoice_posting.create_move", i))
            .contains("tax_account_id")
    );
    assert!(rows(&f.app, "accounting_move").is_empty());
}

#[test]
fn a_zero_rate_books_no_vat_line_and_rounding_is_half_up_in_integers() {
    let f = fixture(0);
    let inv = f.invoice(&[10_000], true);
    let r = f.run(&inv, true).unwrap();
    assert_eq!(r["vat"], json!(0));
    assert_eq!(r["lines"].as_array().unwrap().len(), 2);
    assert_eq!(crate::invoice_posting::vat_of(2, 2500).unwrap(), 1); // 0.5 up
    assert_eq!(crate::invoice_posting::vat_of(1, 2500).unwrap(), 0); // 0.25 down
    assert!(crate::invoice_posting::vat_of(i64::MAX, 20_000).is_err());
}

#[test]
fn a_posted_move_is_counted_by_the_vat_computation() {
    let f = fixture(2500);
    let bx = |n: &str, kind: &str| {
        create(
            &f.app,
            "accounting_se_vat_box",
            json!({"box": n, "label": format!("Box {n}"), "kind": kind}),
        )
    };
    let (box05, box10) = (bx("05", "base"), bx("10", "output"));
    create(
        &f.app,
        "accounting_se_vat_code",
        json!({"code": "T1", "label": "Test", "treatment": "sale_domestic", "rate_bp": 2500,
               "tax_id": f.tax, "base_box_id": box05, "base_side": "credit",
               "vat_box_id": box10, "vat_account_id": f.vat_acc, "vat_side": "credit"}),
    );
    create(
        &f.app,
        "accounting_se_vat_report",
        json!({"name": "2026-Q3", "period_kind": "quarter", "start_on": "2026-07-01", "end_on": "2026-09-30"}),
    );
    let inv = f.invoice(&[10_000, 2_001], true);
    let mid = f.run(&inv, false).unwrap()["move_id"]
        .as_str()
        .unwrap()
        .to_string();
    call(&f.app, &admin(), "accounting_move.post", json!({"id": mid})).unwrap();
    let r = call(
        &f.app,
        &admin(),
        "vat_computation.compute",
        json!({"report": "2026-Q3", "dry_run": true}),
    )
    .unwrap();
    assert_eq!(r["moves"], json!(1));
    assert_eq!(r["output_vat"], json!(3_000));
    let base = r["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["box"] == json!("05"))
        .unwrap();
    assert_eq!(base["amount"], json!(12_001));
}

// Booking on post (owner ruling 2026-11-17): the company's booking settings.

fn configure(f: &Fx, extra: J) {
    let mut s = json!({"journal": "S", "receivable_account": "1510", "revenue_account": "3001", "tax": "Test tax"});
    for (k, v) in extra.as_object().unwrap() {
        s[k] = v.clone();
    }
    call(&f.app, &admin(), "invoice_posting_setting.update", s).unwrap();
}

fn invoice_with_dates(f: &Fx, amounts: &[i64]) -> String {
    let party = create(&f.app, "party", json!({"name": "Kunde AB"}));
    let inv = create(
        &f.app,
        "invoice",
        json!({"state": "draft", "currency": "SEK", "party_id": party,
               "issue_date": "2026-09-10", "due_date": "2026-10-10"}),
    );
    for a in amounts {
        create(
            &f.app,
            "invoice_line",
            json!({"invoice_id": inv, "description": "Item", "quantity": 1, "unit_price_minor": a}),
        );
    }
    inv
}

#[test]
fn posting_an_invoice_books_its_draft_entry_from_the_company_settings() {
    let f = fixture(2500);
    configure(&f, json!({}));
    let inv = invoice_with_dates(&f, &[10_000]);
    call(&f.app, &admin(), "invoice.post", json!({"id": inv})).unwrap();
    let link = call(
        &f.app,
        &admin(),
        "invoice_posting.get",
        json!({"invoice": inv}),
    )
    .unwrap();
    assert_eq!(link["move_state"], json!("draft"));
    assert_eq!(link["booked"], json!(true));
    let mid = link["move_id"].as_str().unwrap().to_string();
    let mv = rows(&f.app, "accounting_move");
    assert_eq!(mv.len(), 1);
    assert_eq!(mv[0].0, mid);
    assert_eq!(mv[0].1["date"], json!("2026-09-10"), "the issue date");
    assert!(mv[0].1["reference"].as_str().unwrap().starts_with("INV-"));
    let lines: Vec<_> = rows(&f.app, "accounting_move_line")
        .into_iter()
        .map(|(_, l)| l)
        .collect();
    assert_eq!(lines.len(), 3);
    let dr: i64 = lines.iter().map(|l| l["debit"].as_i64().unwrap()).sum();
    let cr: i64 = lines.iter().map(|l| l["credit"].as_i64().unwrap()).sum();
    assert_eq!((dr, cr), (12_500, 12_500));
    let rec = lines.iter().find(|l| l["debit"] == json!(12_500)).unwrap();
    assert_eq!(
        rec["due_on"],
        json!("2026-10-10"),
        "the receivable item is due"
    );
    assert!(rec["partner_id"].is_string());
    // The entry reads like one made by hand: the generated get shows its totals.
    let got = call(&f.app, &admin(), "accounting_move.get", json!({"id": mid})).unwrap();
    assert_eq!(got["total_debit"], json!(12_500), "{got}");
    // The explicit command now finds the move.
    let m = refusal(f.run(&inv, false));
    assert!(m.contains("already has a move"), "{m}");
    assert_eq!(rows(&f.app, "accounting_move").len(), 1);
}

#[test]
fn with_book_on_post_off_or_without_settings_the_invoice_is_posted_unbooked() {
    let f = fixture(2500);
    // Not set up: posting works as before and nothing is booked.
    let s = call(&f.app, &admin(), "invoice_posting_setting.get", json!({})).unwrap();
    assert_eq!(s["book_on_post"], json!(true));
    assert_eq!(s["ready"], json!(false));
    assert_eq!(s["missing"].as_array().unwrap().len(), 4);
    let a = invoice_with_dates(&f, &[10_000]);
    call(&f.app, &admin(), "invoice.post", json!({"id": a})).unwrap();
    assert!(rows(&f.app, "accounting_move").is_empty());
    let got = call(
        &f.app,
        &admin(),
        "invoice_posting.get",
        json!({"invoice": a}),
    )
    .unwrap();
    assert_eq!(got["booked"], json!(false));
    // Turned off: the same.
    configure(&f, json!({"book_on_post": false}));
    let s = call(&f.app, &admin(), "invoice_posting_setting.get", json!({})).unwrap();
    assert_eq!(
        (s["book_on_post"].clone(), s["ready"].clone()),
        (json!(false), json!(false))
    );
    let b = invoice_with_dates(&f, &[20_000]);
    call(&f.app, &admin(), "invoice.post", json!({"id": b})).unwrap();
    assert!(rows(&f.app, "accounting_move").is_empty());
    // Booked by hand later, the settings are the defaults and the issue date the date.
    let r = call(
        &f.app,
        &admin(),
        "invoice_posting.create_move",
        json!({"invoice": b}),
    )
    .unwrap();
    assert_eq!(r["gross"], json!(25_000));
    assert_eq!(
        rows(&f.app, "accounting_move")[0].1["date"],
        json!("2026-09-10")
    );
}

#[test]
fn a_booking_that_would_be_refused_refuses_the_post_and_changes_nothing() {
    let f = fixture(2500);
    configure(&f, json!({}));
    let fy = create(
        &f.app,
        "accounting_fiscal_year",
        json!({"name": "FY 2026", "start_on": "2026-01-01", "end_on": "2026-12-31"}),
    );
    let period = create(
        &f.app,
        "accounting_period",
        json!({"fiscal_year_id": fy, "name": "September", "start_on": "2026-09-01", "end_on": "2026-09-30"}),
    );
    call(
        &f.app,
        &admin(),
        "accounting_period.close",
        json!({"id": period}),
    )
    .unwrap();
    let inv = invoice_with_dates(&f, &[10_000]);
    let m = refusal(call(&f.app, &admin(), "invoice.post", json!({"id": inv})));
    assert!(
        m.contains("not posted") && m.contains("closed period") && m.contains("issue date"),
        "{m}"
    );
    let stored = rows(&f.app, "invoice")
        .into_iter()
        .find(|(id, _)| *id == inv)
        .unwrap()
        .1;
    assert_eq!(stored["state"], json!("draft"));
    assert!(
        stored.get("number").is_none_or(|n| n.is_null()),
        "{stored:?}"
    );
    assert!(rows(&f.app, "accounting_move").is_empty());
}

#[test]
fn the_booking_settings_are_checked_and_changed_by_an_accounting_manager() {
    let f = fixture(2500);
    let clerk = Principal::user("clerk", &["accounting.user"]);
    assert!(matches!(
        call(
            &f.app,
            &clerk,
            "invoice_posting_setting.update",
            json!({"book_on_post": false})
        ),
        Err(CallError::Forbidden(_))
    ));
    let m = refusal(call(
        &f.app,
        &admin(),
        "invoice_posting_setting.update",
        json!({"revenue_account": "9999"}),
    ));
    assert!(m.contains("9999"), "{m}");
    configure(&f, json!({}));
    let s = call(&f.app, &clerk, "invoice_posting_setting.get", json!({})).unwrap();
    assert_eq!(s["ready"], json!(true));
    assert_eq!(s["tax"], json!(f.tax), "stored as the id");
    // Kept in the company row: a whole save of the instance settings from an older copy (the
    // back office's Customize) leaves them as they are.
    f.app.installer.set_settings("{}").unwrap();
    let s = call(&f.app, &clerk, "invoice_posting_setting.get", json!({})).unwrap();
    assert_eq!(s["ready"], json!(true));
    let row = f
        .app
        .store
        .backend()
        .get("obop_setting", "company")
        .unwrap()
        .unwrap();
    let stored: J = serde_json::from_str(row["value"].as_str().unwrap()).unwrap();
    assert_eq!(stored["invoicing"]["tax"], json!(f.tax));
    // An empty text clears one.
    call(
        &f.app,
        &admin(),
        "invoice_posting_setting.update",
        json!({"tax": ""}),
    )
    .unwrap();
    let s = call(&f.app, &admin(), "invoice_posting_setting.get", json!({})).unwrap();
    assert_eq!(s["missing"], json!(["tax"]));
}

#[test]
fn an_invoice_without_an_issue_date_is_booked_on_today() {
    let f = fixture(2500);
    configure(&f, json!({}));
    let inv = f.invoice(&[4_000], true);
    let link = call(
        &f.app,
        &admin(),
        "invoice_posting.get",
        json!({"invoice": inv}),
    )
    .unwrap();
    let mv = rows(&f.app, "accounting_move")
        .into_iter()
        .find(|(id, _)| json!(id) == link["move_id"])
        .unwrap()
        .1;
    let today = crate::sie::iso(crate::sie::today());
    assert_eq!(mv["date"], json!(today));
}

#[test]
fn create_move_books_an_invoice_without_an_issue_date_on_today_when_no_date_is_given() {
    // Posted before the booking settings existed, so it was not booked on post (the seed).
    let f = fixture(2500);
    let inv = f.invoice(&[4_000], true);
    configure(&f, json!({}));
    let r = call(
        &f.app,
        &admin(),
        "invoice_posting.create_move",
        json!({"invoice": inv}),
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(r["written"], json!(true));
    let mv = rows(&f.app, "accounting_move")
        .into_iter()
        .find(|(id, _)| json!(id) == r["move_id"])
        .unwrap()
        .1;
    assert_eq!(mv["date"], json!(crate::sie::iso(crate::sie::today())));
}

/// A closed period "September" (2026-09-01 to 2026-09-30).
fn close_september(f: &Fx) {
    let fy = create(
        &f.app,
        "accounting_fiscal_year",
        json!({"name": "FY 2026", "start_on": "2026-01-01", "end_on": "2026-12-31"}),
    );
    let period = create(
        &f.app,
        "accounting_period",
        json!({"fiscal_year_id": fy, "name": "September", "start_on": "2026-09-01", "end_on": "2026-09-30"}),
    );
    call(
        &f.app,
        &admin(),
        "accounting_period.close",
        json!({"id": period}),
    )
    .unwrap();
}

fn booked(f: &Fx, inv: &str) -> J {
    call(
        &f.app,
        &admin(),
        "invoice_posting.get",
        json!({"invoice": inv}),
    )
    .unwrap()
}

#[test]
fn an_update_that_posts_with_a_new_issue_date_is_checked_and_booked_on_that_date() {
    let f = fixture(2500);
    configure(&f, json!({}));
    close_september(&f);
    // Stored in the closed period, posted with a date in an open one: allowed and booked so.
    let inv = invoice_with_dates(&f, &[10_000]);
    call(
        &f.app,
        &admin(),
        "invoice.update",
        json!({"id": inv, "state": "posted", "issue_date": "2026-10-01", "due_date": "2026-10-31"}),
    )
    .unwrap();
    let link = booked(&f, &inv);
    assert_eq!(link["booked"], json!(true), "{link}");
    let mv = rows(&f.app, "accounting_move");
    assert_eq!(mv[0].1["date"], json!("2026-10-01"));
    // No date stored, posted with a date in the closed period: refused, nothing changes.
    let bare = f.invoice(&[5_000], false);
    let m = refusal(call(
        &f.app,
        &admin(),
        "invoice.update",
        json!({"id": bare, "state": "posted", "issue_date": "2026-09-15"}),
    ));
    assert!(m.contains("2026-09-15") && m.contains("September"), "{m}");
    assert!(m.contains("issue date"), "the fix is the date: {m}");
    let stored = rows(&f.app, "invoice")
        .into_iter()
        .find(|(id, _)| *id == bare)
        .unwrap()
        .1;
    assert_eq!(stored["state"], json!("draft"));
    assert_eq!(booked(&f, &bare)["booked"], json!(false));
    assert_eq!(rows(&f.app, "accounting_move").len(), 1);
}

#[test]
fn repeating_posted_on_an_invoice_posted_before_books_nothing() {
    let f = fixture(2500);
    // Posted while booking was not set up: no entry.
    let inv = invoice_with_dates(&f, &[10_000]);
    call(&f.app, &admin(), "invoice.post", json!({"id": inv})).unwrap();
    configure(&f, json!({}));
    call(
        &f.app,
        &admin(),
        "invoice.update",
        json!({"id": inv, "state": "posted"}),
    )
    .unwrap();
    assert_eq!(booked(&f, &inv)["booked"], json!(false));
    assert!(rows(&f.app, "accounting_move").is_empty());
    // It is still booked by hand.
    call(
        &f.app,
        &admin(),
        "invoice_posting.create_move",
        json!({"invoice": inv}),
    )
    .unwrap();
    assert_eq!(booked(&f, &inv)["booked"], json!(true));
}

#[test]
fn the_refusal_of_a_post_in_a_closed_period_is_in_the_language_of_the_request() {
    for lang in obop_commands::locale::LANGUAGES {
        let t = crate::invoice_posting::post_refusal(
            Some(lang),
            Some(("2026-09-10", "September")),
            "x",
        );
        assert!(
            t.contains("2026-09-10") && t.contains("September"),
            "{lang}: {t}"
        );
        let g = crate::invoice_posting::post_refusal(Some(lang), None, "the cause");
        assert!(g.contains("the cause"), "{lang}: {g}");
        if lang != "en" {
            assert_ne!(
                t,
                crate::invoice_posting::post_refusal(None, Some(("2026-09-10", "September")), "x")
            );
        }
    }
    let f = fixture(2500);
    configure(&f, json!({}));
    close_september(&f);
    let inv = invoice_with_dates(&f, &[10_000]);
    let _s = obop_commands::locale::scope(Some("de"));
    let m = refusal(call(&f.app, &admin(), "invoice.post", json!({"id": inv})));
    assert!(m.contains("abgeschlossenen Periode September"), "{m}");
}
