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
    acc("3010", "income");
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
    // The invoice is not in the ledger (no move), so its payment goes to revenue, not to a
    // receivable that was never booked.
    let r = f
        .book(json!({"bank_line": l, "invoice": num, "counter_account": "3010"}))
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
    let m = refusal(f.book(json!({"bank_line": l2, "invoice": inv, "counter_account": "3010"})));
    assert!(m.contains("already matched"), "{m}");
    assert_eq!(rows(&f.app, "accounting_move").len(), 1);
}

#[test]
fn an_invoice_not_in_the_ledger_is_not_paid_against_a_receivable() {
    let f = fixture();
    let (_, num) = f.invoice(&[49_800]);
    let l = f.line(49_800, &format!("Payment {num}"));
    let m = refusal(f.book(json!({"bank_line": l, "invoice": num, "counter_account": "1510"})));
    assert!(
        m.contains("not in the ledger") && m.contains("1510") && m.contains("create_move"),
        "{m}"
    );
    assert!(rows(&f.app, "accounting_move").is_empty());
    assert!(rows(&f.app, "bank_matching").is_empty());
    // The suggestion says what the proposal rests on, so a screen can tell before it books.
    let s = f.suggest();
    let c = &s["suggestions"][0]["candidates"][0];
    assert_eq!(c["basis"], json!("net_of_lines"));
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
    // A line that is already matched is refused (a match names its journal item, accounting 0.3.1).
    let done = f.line(7, "w");
    let item = rows(&f.app, "accounting_move_line")[0].0.clone();
    call(
        &f.app,
        &admin(),
        "accounting_bank_statement_line.match",
        json!({"id": done, "move_line_id": item}),
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

impl Fx {
    /// A posted invoice of `amount` (no tax) with its ledger move, and the move's receivable item.
    fn invoice_with_move(&self, amount: i64) -> (String, String, String) {
        if !rows(&self.app, "accounting_journal")
            .iter()
            .any(|(_, j)| j["code"] == json!("S"))
        {
            let acc = |code: &str, t: &str| {
                create(
                    &self.app,
                    "accounting_account",
                    json!({"code": code, "name": format!("Test {code}"), "account_type": t}),
                )
            };
            acc("3001", "income");
            let vat = acc("2611", "liability_current");
            create(
                &self.app,
                "accounting_tax",
                json!({"name": "No tax", "rate_bp": 0, "scope": "sales", "tax_account_id": vat}),
            );
            create(
                &self.app,
                "accounting_journal",
                json!({"name": "Sales", "code": "S", "kind": "sale"}),
            );
        }
        let (inv, number) = self.invoice(&[amount]);
        let r = call(
            &self.app,
            &admin(),
            "invoice_posting.create_move",
            json!({"invoice": inv, "journal": "S", "receivable_account": "1510",
                   "revenue_account": "3001", "tax": "No tax", "date": "2026-09-14"}),
        )
        .unwrap();
        let mid = r["move_id"].as_str().unwrap().to_string();
        let item = rows(&self.app, "accounting_move_line")
            .into_iter()
            .find(|(_, l)| l["move_id"] == json!(mid) && l["debit"].as_i64().unwrap_or(0) > 0)
            .unwrap()
            .0;
        (inv, number, item)
    }

    fn settles(&self, bank_line: &str) -> J {
        rows(&self.app, "accounting_bank_statement_line")
            .into_iter()
            .find(|(id, _)| id == bank_line)
            .unwrap()
            .1
            .get("settles_line_id")
            .cloned()
            .unwrap_or(J::Null)
    }
}

#[test]
fn paying_an_invoice_through_its_proposal_settles_its_receivable_item_once() {
    let f = fixture();
    let (_, number, item) = f.invoice_with_move(9_520);
    let first = f.line(9_520, &format!("Payment {number}"));
    f.book(json!({"bank_line": first, "invoice": number, "counter_account": "1510"}))
        .unwrap();
    assert_eq!(
        f.settles(&first),
        json!(item),
        "the item is recorded as paid"
    );
    // The item's own proposal (a second transaction of the same amount) cannot pay it again.
    let second = f.line(9_520, "Same amount again");
    let r = call(
        &f.app,
        &admin(),
        "accounting_bank_statement_line.update",
        json!({"id": second, "settles_line_id": item}),
    );
    assert!(r.is_err(), "a second payment of the item is refused: {r:?}");
    assert_eq!(f.settles(&second), J::Null);
}

#[test]
fn an_item_paid_through_its_own_proposal_takes_its_invoice_out_of_the_proposals() {
    let f = fixture();
    let (inv, number, item) = f.invoice_with_move(4_200);
    // The item path: record the item, then book without an invoice.
    let paid = f.line(4_200, "Pays the item");
    call(
        &f.app,
        &admin(),
        "accounting_bank_statement_line.update",
        json!({"id": paid, "settles_line_id": item}),
    )
    .unwrap();
    f.book(json!({"bank_line": paid, "counter_account": "1510"}))
        .unwrap();
    let other = f.line(4_200, &format!("Payment {number}"));
    let s = call(
        &f.app,
        &admin(),
        "bank_matching.suggest",
        json!({"bank_line": other}),
    )
    .unwrap();
    let offered = s["suggestions"][0]["candidates"].as_array().unwrap();
    assert!(
        !offered.iter().any(|c| c["invoice_id"] == json!(inv)),
        "{offered:?}"
    );
    let m = refusal(f.book(json!({"bank_line": other, "invoice": inv, "counter_account": "1510"})));
    assert!(m.contains("already paid"), "{m}");
    assert_eq!(f.settles(&other), J::Null);
    assert_eq!(
        rows(&f.app, "bank_matching").len(),
        1,
        "nothing more was written"
    );
}

#[test]
fn undoing_a_match_opens_the_item_again() {
    let f = fixture();
    let (_, number, item) = f.invoice_with_move(7_000);
    let l = f.line(7_000, &format!("Payment {number}"));
    let r = f
        .book(json!({"bank_line": l, "invoice": number, "counter_account": "1510"}))
        .unwrap();
    let mid = r["move_id"].as_str().unwrap().to_string();
    let bank_side = rows(&f.app, "accounting_move_line")
        .into_iter()
        .find(|(_, x)| x["move_id"] == json!(mid) && x["debit"].as_i64().unwrap_or(0) > 0)
        .unwrap()
        .0;
    call(
        &f.app,
        &admin(),
        "accounting_bank_statement_line.update",
        json!({"id": l, "move_line_id": bank_side}),
    )
    .unwrap();
    call(
        &f.app,
        &admin(),
        "accounting_bank_statement_line.match",
        json!({"id": l}),
    )
    .unwrap();
    assert_eq!(f.settles(&l), json!(item));
    call(
        &f.app,
        &admin(),
        "accounting_bank_statement_line.unmatch",
        json!({"id": l}),
    )
    .unwrap();
    assert_eq!(
        f.settles(&l),
        J::Null,
        "the undone match no longer pays the item"
    );
    // Another transaction may pay it now.
    let again = f.line(7_000, "The right payment");
    call(
        &f.app,
        &admin(),
        "accounting_bank_statement_line.update",
        json!({"id": again, "settles_line_id": item}),
    )
    .unwrap();
}

/// Q9: `obop call bank_matching book ... dry_run=true` sent the text "true" (the model has no
/// `list`, so it is not in the metadata), and the handler read it as a real run. The command line
/// now types the field by the module's own definition.
#[test]
fn the_command_line_types_dry_run_of_models_without_a_list_as_a_bool() {
    let f = fixture();
    let agent = Principal::agent("bot", &["accounting.user"], &["accounting.user"]);
    let meta = f.app.dispatcher.meta(Surface::Cli, &agent);
    assert!(
        !meta
            .iter()
            .flat_map(|m| &m.models)
            .any(|m| m.name == "bank_matching")
    );
    for model in ["bank_matching", "vat_computation", "invoice_posting"] {
        assert_eq!(
            crate::cli_kind(&f.app.dispatcher, &meta, model, "dry_run"),
            "bool",
            "{model}"
        );
    }
    assert_eq!(
        crate::cli_kind(&f.app.dispatcher, &meta, "bank_matching", "nope"),
        "text"
    );
}

/// Q4: a receivable item a bank transaction pays is `settled`, and the filters "Open items",
/// "Overdue" and "Due in 30 days" leave it out.
#[test]
fn a_paid_receivable_item_is_settled_and_not_overdue() {
    let f = fixture();
    let (_, number, item) = f.invoice_with_move(9_520);
    let search = |filter: J| {
        call(
            &f.app,
            &admin(),
            "accounting_move_line.search",
            json!({"filter": filter}),
        )
        .unwrap()["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    let open = json!([{"field": "reconciled", "op": "ne", "value": true},
                      {"field": "settled", "op": "eq", "value": false}]);
    assert!(search(open.clone()).contains(&item));
    let got = call(
        &f.app,
        &admin(),
        "accounting_move_line.get",
        json!({"id": item}),
    )
    .unwrap();
    assert_eq!(got["settled"], json!(false));
    let l = f.line(9_520, &format!("Payment {number}"));
    f.book(json!({"bank_line": l, "invoice": number, "counter_account": "1510"}))
        .unwrap();
    let got = call(
        &f.app,
        &admin(),
        "accounting_move_line.get",
        json!({"id": item}),
    )
    .unwrap();
    assert_eq!(got["settled"], json!(true));
    assert!(!search(open).contains(&item));
    // The package's own filters say the same.
    let def = f.app.dispatcher.model_def("accounting_move_line").unwrap();
    let wheres: Vec<String> = def
        .ext
        .ui
        .as_ref()
        .map(|u| u.filters.iter().map(|x| x.filter.source.clone()).collect())
        .unwrap_or_default();
    assert_eq!(wheres.len(), 3, "{wheres:?}");
    assert!(
        wheres.iter().all(|w| w.ends_with("&& settled = false")),
        "{wheres:?}"
    );
}

/// Q11: taking back a booked invoice payment cancels the draft entry, frees the item and the
/// invoice and deletes the link, in one call; a posted entry is refused.
#[test]
fn unbook_takes_a_booked_payment_back_so_the_invoice_can_be_paid_again() {
    let f = fixture();
    let (inv, number, item) = f.invoice_with_move(7_000);
    let l = f.line(7_000, &format!("Payment {number}"));
    let r = f
        .book(json!({"bank_line": l, "invoice": number, "counter_account": "1510"}))
        .unwrap();
    let mid = r["move_id"].as_str().unwrap().to_string();
    let link = call(
        &f.app,
        &admin(),
        "bank_matching.get",
        json!({"bank_line": l}),
    )
    .unwrap();
    assert_eq!(link["invoice_id"], json!(inv));
    assert_eq!(link["move_state"], json!("draft"));
    // Matched to the bank side of the draft entry, as the screen does.
    let bank_side = rows(&f.app, "accounting_move_line")
        .into_iter()
        .find(|(_, x)| x["move_id"] == json!(mid) && x["debit"].as_i64().unwrap_or(0) > 0)
        .unwrap()
        .0;
    call(
        &f.app,
        &admin(),
        "accounting_bank_statement_line.update",
        json!({"id": l, "move_line_id": bank_side}),
    )
    .unwrap();
    call(
        &f.app,
        &admin(),
        "accounting_bank_statement_line.match",
        json!({"id": l}),
    )
    .unwrap();
    // An agent may only look.
    let agent = Principal::agent("bot", &["accounting.user"], &["accounting.user"]);
    let m = refusal(call(
        &f.app,
        &agent,
        "bank_matching.unbook",
        json!({"bank_line": l}),
    ));
    assert!(m.contains("dry-run"), "{m}");
    let dry = call(
        &f.app,
        &agent,
        "bank_matching.unbook",
        json!({"bank_line": l, "dry_run": true}),
    )
    .unwrap();
    assert_eq!(dry["written"], json!(false));
    assert_eq!(dry["move_cancelled"], json!(true));
    assert_eq!(f.settles(&l), json!(item));
    let done = call(
        &f.app,
        &admin(),
        "bank_matching.unbook",
        json!({"bank_line": l}),
    )
    .unwrap();
    assert_eq!(done["unmatched"], json!(true));
    let mv = rows(&f.app, "accounting_move")
        .into_iter()
        .find(|(id, _)| *id == mid)
        .unwrap()
        .1;
    assert_eq!(mv["state"], json!("cancelled"));
    let line = rows(&f.app, "accounting_bank_statement_line")
        .into_iter()
        .find(|(id, _)| *id == l)
        .unwrap()
        .1;
    assert_eq!(line["state"], json!("unmatched"));
    assert!(
        line.get("settles_line_id").is_none_or(J::is_null),
        "{line:?}"
    );
    assert!(line.get("move_line_id").is_none_or(J::is_null), "{line:?}");
    assert!(rows(&f.app, "bank_matching").is_empty());
    let m = refusal(call(
        &f.app,
        &admin(),
        "bank_matching.get",
        json!({"bank_line": l}),
    ));
    assert!(m.contains("no booking"), "{m}");
    // The invoice is offered and can be paid again, by the same line.
    let s = call(
        &f.app,
        &admin(),
        "bank_matching.suggest",
        json!({"bank_line": l}),
    )
    .unwrap();
    assert!(
        s["suggestions"][0]["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["invoice_id"] == json!(inv)),
        "{s}"
    );
    let again = f
        .book(json!({"bank_line": l, "invoice": number, "counter_account": "1510"}))
        .unwrap();
    let again = again["move_id"].as_str().unwrap().to_string();
    assert_eq!(f.settles(&l), json!(item));
    // A posted entry is not taken back here.
    call(
        &f.app,
        &admin(),
        "accounting_move.post",
        json!({"id": again}),
    )
    .unwrap();
    let m = refusal(call(
        &f.app,
        &admin(),
        "bank_matching.unbook",
        json!({"bank_line": l}),
    ));
    assert!(m.contains("posted") && m.contains("reverse"), "{m}");
    assert_eq!(rows(&f.app, "bank_matching").len(), 1);
}
