//! `bank_matching.suggest` and `bank_matching.book` over invented accounts: nothing here is a real
//! chart or rate and none of it is checked against Swedish bookkeeping rules.

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
    journal: String,
}

fn fixture() -> Fx {
    let app = App::new();
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
    let bank = acc("1930", "asset_cash");
    acc("1510", "asset_receivable");
    acc("6570", "expense");
    let journal = create(
        &app,
        "accounting_journal",
        json!({"name": "Bank", "code": "B", "kind": "bank", "default_account_id": bank}),
    );
    Fx { app, journal }
}

impl Fx {
    fn line(&self, amount: i64, label: &str) -> String {
        create(
            &self.app,
            "accounting_bank_statement_line",
            json!({"journal_id": self.journal, "date": "2026-09-20", "label": label,
                   "amount": amount, "state": "unmatched"}),
        )
    }

    fn invoice(&self, amounts: &[i64]) -> (String, String) {
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
        call(&self.app, &admin(), "invoice.post", json!({"id": inv})).unwrap();
        let number = rows(&self.app, "invoice")
            .into_iter()
            .find(|(id, _)| *id == inv)
            .unwrap()
            .1["number"]
            .as_str()
            .unwrap()
            .to_string();
        (inv, number)
    }

    fn suggest(&self) -> J {
        call(&self.app, &admin(), "bank_matching.suggest", json!({})).unwrap()
    }

    fn book(&self, i: J) -> Result<J, CallError> {
        call(&self.app, &admin(), "bank_matching.book", i)
    }
}

#[test]
fn it_suggests_by_exact_amount_and_ranks_a_reference_match_first() {
    let f = fixture();
    let (a, na) = f.invoice(&[10_000]);
    let (b, _) = f.invoice(&[10_000]);
    f.invoice(&[10_001]);
    let l = f.line(10_000, &format!("Payment {}", na.to_lowercase()));
    f.line(-10_000, "Money out never matches an invoice");
    f.line(55, "Nothing this size");
    let r = f.suggest();
    let s = r["suggestions"].as_array().unwrap();
    assert_eq!(s.len(), 3);
    let first = s.iter().find(|x| x["bank_line_id"] == json!(l)).unwrap();
    let c = first["candidates"].as_array().unwrap();
    assert_eq!(c.len(), 2, "two invoices of 10000, not the one of 10001");
    assert_eq!(c[0]["invoice_id"], json!(a));
    assert_eq!(c[0]["reference_match"], json!(true));
    assert_eq!(c[1]["invoice_id"], json!(b));
    assert_eq!(c[1]["reference_match"], json!(false));
    assert_eq!(c[0]["basis"], json!("net_of_lines"));
    assert_eq!(
        s.iter()
            .filter(|x| x["candidates"].as_array().unwrap().is_empty())
            .count(),
        2
    );
    assert!(rows(&f.app, "accounting_move").is_empty(), "read only");
    assert!(rows(&f.app, "bank_matching").is_empty());
    let only = call(
        &f.app,
        &admin(),
        "bank_matching.suggest",
        json!({"bank_line": l}),
    )
    .unwrap();
    assert_eq!(only["suggestions"].as_array().unwrap().len(), 1);
    let m = refusal(call(
        &f.app,
        &admin(),
        "bank_matching.suggest",
        json!({"bank_line": "nope"}),
    ));
    assert!(m.contains("nope"), "{m}");
}

#[test]
fn it_books_money_in_as_a_balanced_draft_move_once_per_line_and_invoice() {
    let f = fixture();
    let (inv, num) = f.invoice(&[12_345]);
    let l = f.line(12_345, &format!("Betalning {num}"));
    let r = f
        .book(json!({"bank_line": l, "invoice": num, "counter_account": "1510"}))
        .unwrap();
    assert_eq!(r["written"], json!(true));
    let mid = r["move_id"].as_str().unwrap();
    let mv = rows(&f.app, "accounting_move");
    assert_eq!(mv.len(), 1);
    assert_eq!(mv[0].1["state"], json!("draft"));
    assert_eq!(mv[0].1["journal_id"], json!(f.journal));
    assert_eq!(mv[0].1["date"], json!("2026-09-20"));
    let lines: Vec<_> = rows(&f.app, "accounting_move_line")
        .into_iter()
        .filter(|(_, x)| x["move_id"] == json!(mid))
        .map(|(_, x)| x)
        .collect();
    assert_eq!(lines.len(), 2);
    let sums = |side: &str| lines.iter().map(|x| x[side].as_i64().unwrap()).sum::<i64>();
    assert_eq!((sums("debit"), sums("credit")), (12_345, 12_345));
    let link = rows(&f.app, "bank_matching");
    assert_eq!(link.len(), 1);
    assert_eq!(link[0].1["invoice_id"], json!(inv));
    // Booked lines and matched invoices leave the suggestions and cannot be booked again.
    assert!(f.suggest()["suggestions"].as_array().unwrap().is_empty());
    let m = refusal(f.book(json!({"bank_line": l, "counter_account": "1510"})));
    assert!(m.contains("already has a move"), "{m}");
    let l2 = f.line(12_345, "Second payment");
    let m = refusal(f.book(json!({"bank_line": l2, "invoice": inv, "counter_account": "1510"})));
    assert!(m.contains("already matched"), "{m}");
    assert_eq!(rows(&f.app, "accounting_move").len(), 1);
}

#[test]
fn it_books_money_out_without_an_invoice_the_other_way_round() {
    let f = fixture();
    let l = f.line(-2_500, "Bank fee");
    let r = f
        .book(json!({"bank_line": l, "counter_account": "6570"}))
        .unwrap();
    let lines = r["lines"].as_array().unwrap();
    assert_eq!(lines[0]["account"], json!("1930"));
    assert_eq!(lines[0]["credit"], json!(2_500));
    assert_eq!(lines[1]["account"], json!("6570"));
    assert_eq!(lines[1]["debit"], json!(2_500));
    let (_, n) = f.invoice(&[2_500]);
    let l2 = f.line(-2_500, "Out");
    let m = refusal(f.book(json!({"bank_line": l2, "invoice": n, "counter_account": "6570"})));
    assert!(m.contains("money out"), "{m}");
}

#[test]
fn it_refuses_in_words_and_posting_stays_governed() {
    let f = fixture();
    let (_, num) = f.invoice(&[10_000]);
    let l = f.line(9_999, "x");
    let m = refusal(f.book(json!({"bank_line": l, "invoice": num, "counter_account": "1510"})));
    assert!(m.contains("exact"), "{m}");
    let ok = f.line(10_000, "y");
    for key in ["bank_line", "counter_account"] {
        let mut i = json!({"bank_line": ok, "counter_account": "1510"});
        i.as_object_mut().unwrap().remove(key);
        assert!(refusal(f.book(i)).contains(key), "{key}");
    }
    let m = refusal(f.book(json!({"bank_line": ok, "counter_account": "9999"})));
    assert!(m.contains("9999"), "{m}");
    let m = refusal(f.book(json!({"bank_line": ok, "counter_account": "1930"})));
    assert!(m.contains("bank account"), "{m}");
    let m =
        refusal(f.book(json!({"bank_line": ok, "invoice": "INV-nope", "counter_account": "1510"})));
    assert!(m.contains("INV-nope"), "{m}");
    // A journal with no default account and no bank_account input.
    let j2 = create(
        &f.app,
        "accounting_journal",
        json!({"name": "Bank 2", "code": "B2", "kind": "bank"}),
    );
    let l3 = create(
        &f.app,
        "accounting_bank_statement_line",
        json!({"journal_id": j2, "date": "2026-09-21", "label": "z", "amount": 5, "state": "unmatched"}),
    );
    let m = refusal(f.book(json!({"bank_line": l3, "counter_account": "1510"})));
    assert!(m.contains("default_account_id"), "{m}");
    f.book(json!({"bank_line": l3, "counter_account": "1510", "bank_account": "1930"}))
        .unwrap();
    // A line that is already matched is refused.
    let done = f.line(7, "w");
    call(
        &f.app,
        &admin(),
        "accounting_bank_statement_line.match",
        json!({"id": done}),
    )
    .unwrap();
    let m = refusal(f.book(json!({"bank_line": done, "counter_account": "1510"})));
    assert!(m.contains("matched"), "{m}");
    // Posting is not ours: a clerk cannot, an agent needs approval.
    let mid = rows(&f.app, "accounting_move")[0].0.clone();
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
fn a_dry_run_writes_nothing_and_an_agent_may_only_dry_run() {
    let f = fixture();
    let l = f.line(10_000, "y");
    let i = json!({"bank_line": l, "counter_account": "1510", "dry_run": true});
    let dry = f.book(i.clone()).unwrap();
    assert_eq!(dry["written"], json!(false));
    assert_eq!(dry["lines"].as_array().unwrap().len(), 2);
    assert!(rows(&f.app, "accounting_move").is_empty());
    assert!(rows(&f.app, "bank_matching").is_empty());
    let agent = Principal::agent("bot", &["accounting.user"], &["accounting.user"]);
    let mut real = i.clone();
    real["dry_run"] = json!(false);
    let m = refusal(call(&f.app, &agent, "bank_matching.book", real));
    assert!(m.contains("dry-run"), "{m}");
    call(&f.app, &agent, "bank_matching.book", i).unwrap();
    call(&f.app, &agent, "bank_matching.suggest", json!({})).unwrap();
    assert!(rows(&f.app, "accounting_move").is_empty());
    let nobody = Principal::user("nobody", &["base.user"]);
    assert!(matches!(
        call(&f.app, &nobody, "bank_matching.suggest", json!({})),
        Err(CallError::Forbidden(_))
    ));
}
