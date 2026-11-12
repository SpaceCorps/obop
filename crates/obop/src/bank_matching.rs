//! The typed commands `bank_matching.suggest` and `bank_matching.book` (docs/spec/accounting-se.md
//! section 2, stage 7, and section 7.1).
//!
//! `suggest` is read only: for each unmatched bank statement line (`accounting_bank_statement_line`,
//! not yet linked) it lists the posted customer invoices whose amount equals the line's amount
//! exactly, and says whether the invoice number appears in the bank's label. The invoice amount is
//! the total debit of the invoice's own move (`invoice_posting`, gross) when there is one that is
//! not cancelled, otherwise the sum of the invoice lines (net, no VAT known). Integer minor units,
//! no floats, no fuzzy matching. Only money in (a positive line) is matched to a customer invoice.
//!
//! `book` writes a DRAFT `accounting_move` in the line's bank journal: the bank account (the
//! `bank_account` input or the journal's `default_account_id`) against the `counter_account` you
//! name, debit for money in and credit for money out, and a link row (bank line id, move id,
//! invoice id) so a line is booked once and an invoice is matched once. Posting stays
//! `accounting_move.post` (`accounting.approver` and a person's approval); an agent may only
//! dry-run. No account is built in (D34), nothing is checked against Swedish bookkeeping rules,
//! and the bank line's own `state` is not changed.

use crate::sie::{Env, call};
use obop_commands::{Actor, Dispatcher, Principal, Store, Value};
use obop_registry::{CommandDef, FieldDef, FieldKind, ModelDef, Module};
use obop_storage::Fields;
use obop_types::DataClass;
use serde_json::{Value as J, json};
use std::sync::{Arc, Weak};

const MODEL: &str = "bank_matching";
const GROUP: &str = "accounting.user";
const MAX_LINES: usize = 200;

pub fn module() -> Module {
    let f = |name, description| FieldDef::new(name, description, DataClass::None, false, GROUP);
    let fin =
        |name, description| FieldDef::new(name, description, DataClass::Financial, false, GROUP);
    Module {
        id: "se_bank_matching".into(),
        name: "Bank matching".into(),
        description:
            "Suggests matches of bank lines to posted invoices and books a line as a draft move"
                .into(),
        models: vec![ModelDef {
            name: MODEL,
            description: "`suggest` lists, read only, the posted customer invoices that match each unmatched bank statement line by exact amount; `book` books one line as a DRAFT `accounting_move` (bank account against a counter account you name). Rows of this model are the link from a bank line to its move (one each). Needs the accounting package. UNVERIFIED: nothing is checked against Swedish bookkeeping rules",
            read_group: GROUP,
            write_group: GROUP,
            access: None,
            fields: vec![
                f(
                    "bank_line",
                    "Input: the id of the bank statement line (suggest: optional, restricts to that line)",
                ),
                f(
                    "invoice",
                    "Input, book: the id or number of the posted invoice the line pays; its amount must equal the line",
                ),
                f(
                    "counter_account",
                    "Input, book: the id or code of the account booked against the bank account",
                ),
                f(
                    "bank_account",
                    "Input, book, optional: the id or code of the bank account; default is the journal's default_account_id",
                ),
                f(
                    "date",
                    "Input, book, optional: the accounting date YYYY-MM-DD; default is the line's date",
                ),
                f("dry_run", "Input, book: only show the lines, write nothing")
                    .kind(FieldKind::Bool),
                f("bank_line_id", "Link: the bank statement line id"),
                f(
                    "invoice_id",
                    "Link: the invoice id, empty when the line was booked without one",
                ),
                f("move_id", "Link and answer: the draft move id"),
                f("ok", "Answer: whether the command worked").kind(FieldKind::Bool),
                f("written", "Answer: whether the move was created").kind(FieldKind::Bool),
                fin("amount", "Answer: the line amount in minor units").kind(FieldKind::Number),
                f(
                    "suggestions",
                    "Answer, suggest: one entry per unmatched line with its candidate invoices",
                ),
                f(
                    "lines",
                    "Answer, book: the move lines: account, debit, credit",
                ),
                f("notes", "Answer: remarks (a list)"),
            ],
            commands: vec![
                CommandDef::new(
                    "suggest",
                    "Read only: for each unmatched bank statement line that is not yet booked (or for bank_line), list the posted invoices whose amount equals the line exactly and say whether the invoice number appears in the bank's label. Money in only. Writes nothing. UNVERIFIED",
                    GROUP,
                ),
                CommandDef::new(
                    "book",
                    "Book a bank statement line (bank_line) as a draft accounting move in its journal: the bank account (bank_account or the journal's default account) against counter_account; optional invoice (posted, same amount) is linked and its party set. Refused in words when an input is missing, the line is not unmatched, or the line or invoice is already linked. dry_run shows the lines and writes nothing. An agent may only dry-run. The move stays a draft: posting needs accounting.approver and approval. UNVERIFIED",
                    GROUP,
                ),
            ],
            ext: Default::default(),
        }],
        ..Module::default()
    }
}

fn text(f: &Fields, k: &str) -> String {
    f.get(k).and_then(J::as_str).unwrap_or_default().to_string()
}

fn int(f: &Fields, k: &str) -> i64 {
    f.get(k).and_then(J::as_i64).unwrap_or(0)
}

fn list(env: Env<'_>, m: &str) -> Result<Vec<(String, Fields)>, String> {
    env.store.backend().list(m).map_err(|e| {
        format!(
            "cannot read {m}: {e} (are the invoicing module and the accounting package installed?)"
        )
    })
}

fn need(p: &Principal) -> Result<(), String> {
    if p.allows(GROUP) {
        Ok(())
    } else {
        Err(format!(
            "bank matching needs the {GROUP} group (an agent needs it in its scopes)"
        ))
    }
}

/// A posted invoice and the amount a payment would have to equal.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub invoice_id: String,
    pub number: String,
    pub amount: i64,
    /// `move` (gross from the invoice's ledger move) or `net_of_lines` (no move, no VAT known).
    pub basis: &'static str,
}

/// The posted invoices that are not yet linked to a bank line, with their amounts.
pub fn candidates(env: Env<'_>) -> Result<Vec<Candidate>, String> {
    let taken: Vec<String> = list(env, MODEL)?
        .iter()
        .map(|(_, f)| text(f, "invoice_id"))
        .filter(|s| !s.is_empty())
        .collect();
    let moves = list(env, "accounting_move")?;
    let mlines = list(env, "accounting_move_line")?;
    let posting = list(env, "invoice_posting").unwrap_or_default();
    let ilines = list(env, "invoice_line")?;
    let mut out = Vec::new();
    for (id, inv) in list(env, "invoice")? {
        if text(&inv, "state") != "posted" || taken.contains(&id) {
            continue;
        }
        let live_move = posting
            .iter()
            .filter(|(_, l)| text(l, "invoice_id") == id)
            .map(|(_, l)| text(l, "move_id"))
            .find(|m| {
                moves
                    .iter()
                    .any(|(mid, mf)| mid == m && text(mf, "state") != "cancelled")
            });
        let (amount, basis) = match live_move {
            Some(m) => (
                mlines
                    .iter()
                    .filter(|(_, l)| text(l, "move_id") == m)
                    .try_fold(0i64, |a, (_, l)| a.checked_add(int(l, "debit")))
                    .ok_or("a move total overflows")?,
                "move",
            ),
            None => (
                ilines
                    .iter()
                    .filter(|(_, l)| text(l, "invoice_id") == id)
                    .try_fold(0i64, |a, (_, l)| {
                        let q = l.get("quantity").and_then(J::as_i64).unwrap_or(0);
                        let u = l.get("unit_price_minor").and_then(J::as_i64).unwrap_or(0);
                        let v = match l.get("amount_minor").and_then(J::as_i64) {
                            Some(v) => v,
                            None => q.checked_mul(u)?,
                        };
                        a.checked_add(v)
                    })
                    .ok_or("an invoice total overflows")?,
                "net_of_lines",
            ),
        };
        if amount > 0 {
            out.push(Candidate {
                invoice_id: id,
                number: text(&inv, "number"),
                amount,
                basis,
            });
        }
    }
    Ok(out)
}

/// Whether `number` appears in `label`, ignoring case; an empty number never matches.
pub fn mentions(label: &str, number: &str) -> bool {
    !number.is_empty() && label.to_lowercase().contains(&number.to_lowercase())
}

/// One unmatched line and its candidates (the flag: the invoice number is in the label).
pub struct Suggestion {
    pub bank_line_id: String,
    pub date: String,
    pub label: String,
    pub amount: i64,
    pub matches: Vec<(Candidate, bool)>,
}

/// Read only. Matches come with the reference ones first, then by invoice number.
pub fn suggest(env: Env<'_>, p: &Principal, only: &str) -> Result<Vec<Suggestion>, String> {
    need(p)?;
    let linked: Vec<String> = list(env, MODEL)?
        .iter()
        .map(|(_, f)| text(f, "bank_line_id"))
        .collect();
    let cands = candidates(env)?;
    let mut lines = list(env, "accounting_bank_statement_line")?;
    lines.sort_by(|a, b| (text(&a.1, "date"), &a.0).cmp(&(text(&b.1, "date"), &b.0)));
    if !only.is_empty() && !lines.iter().any(|(id, _)| id == only) {
        return Err(format!("no bank statement line {only:?} (give its id)"));
    }
    let mut out = Vec::new();
    for (id, f) in lines {
        if (!only.is_empty() && id != only)
            || text(&f, "state") != "unmatched"
            || linked.contains(&id)
        {
            continue;
        }
        let amount = int(&f, "amount");
        let label = text(&f, "label");
        let mut matches: Vec<(Candidate, bool)> = if amount > 0 {
            cands
                .iter()
                .filter(|c| c.amount == amount)
                .map(|c| (c.clone(), mentions(&label, &c.number)))
                .collect()
        } else {
            Vec::new()
        };
        matches.sort_by(|a, b| {
            (!a.1, &a.0.number, &a.0.invoice_id).cmp(&(!b.1, &b.0.number, &b.0.invoice_id))
        });
        out.push(Suggestion {
            bank_line_id: id,
            date: text(&f, "date"),
            label,
            amount,
            matches,
        });
        if out.len() >= MAX_LINES {
            break;
        }
    }
    Ok(out)
}

/// Inputs of `book` as given.
#[derive(Default)]
pub struct Input {
    pub bank_line: String,
    pub invoice: String,
    pub counter_account: String,
    pub bank_account: String,
    pub date: String,
}

/// A planned move line.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub account_id: String,
    pub account: String,
    pub label: String,
    pub debit: i64,
    pub credit: i64,
}

/// What `book` would write.
pub struct Planned {
    pub bank_line_id: String,
    pub invoice_id: String,
    pub party_id: String,
    pub journal_id: String,
    pub date: String,
    pub reference: String,
    pub amount: i64,
    pub lines: Vec<Line>,
    pub notes: Vec<String>,
}

fn pick(rows: &[(String, Fields)], what: &str, key: &str) -> Result<(String, Fields), String> {
    if key.is_empty() {
        return Err(format!("{what} is required"));
    }
    let hits: Vec<_> = rows
        .iter()
        .filter(|(id, f)| id == key || text(f, "code") == key)
        .collect();
    match hits.as_slice() {
        [(id, f)] => Ok((id.clone(), f.clone())),
        [] => Err(format!("no {what} {key:?} (give its id or code)")),
        _ => Err(format!("{what} {key:?} is ambiguous: give its id")),
    }
}

/// Build the plan. Reads only; every refusal is in words.
pub fn plan(env: Env<'_>, p: &Principal, i: &Input) -> Result<Planned, String> {
    need(p)?;
    if i.bank_line.is_empty() {
        return Err("bank_line is required (the id of the bank statement line)".into());
    }
    let (lid, line) = list(env, "accounting_bank_statement_line")?
        .into_iter()
        .find(|(id, _)| *id == i.bank_line)
        .ok_or_else(|| format!("no bank statement line {:?} (give its id)", i.bank_line))?;
    let state = text(&line, "state");
    if state != "unmatched" {
        return Err(format!(
            "the bank line is {state}; only an unmatched line is booked"
        ));
    }
    let links = list(env, MODEL)?;
    if links.iter().any(|(_, f)| text(f, "bank_line_id") == lid) {
        return Err(
            "this bank line already has a move; one line gets one (nothing was written)".into(),
        );
    }
    let amount = int(&line, "amount");
    if amount == 0 {
        return Err("the bank line has no amount; nothing to book".into());
    }
    let date = if i.date.is_empty() {
        text(&line, "date")
    } else {
        i.date.clone()
    };
    if crate::sie::from_iso(&date).is_none() {
        return Err("date must be YYYY-MM-DD (the line has none, so give one)".into());
    }
    let journal_id = text(&line, "journal_id");
    let journals = list(env, "accounting_journal")?;
    let journal = journals
        .iter()
        .find(|(id, _)| *id == journal_id)
        .map(|(_, f)| f.clone())
        .ok_or("the bank line's journal does not exist")?;
    let accounts = list(env, "accounting_account")?;
    let bank_key = if i.bank_account.is_empty() {
        text(&journal, "default_account_id")
    } else {
        i.bank_account.clone()
    };
    if bank_key.is_empty() {
        return Err("the journal has no default_account_id and no bank_account was given".into());
    }
    let (bank_id, bank) = pick(&accounts, "bank_account", &bank_key)?;
    let (ctr_id, ctr) = pick(&accounts, "counter_account", &i.counter_account)?;
    if bank_id == ctr_id {
        return Err("counter_account is the bank account; name the other side".into());
    }
    let mut party_id = String::new();
    let mut invoice_id = String::new();
    let mut notes = vec!["UNVERIFIED: nothing is checked against Swedish bookkeeping rules; the move is a draft, posting needs accounting.approver and the bank line itself is left as it is".to_string()];
    if !i.invoice.is_empty() {
        let invs = list(env, "invoice")?;
        let hits: Vec<_> = invs
            .iter()
            .filter(|(id, f)| *id == i.invoice || text(f, "number") == i.invoice)
            .collect();
        let (iid, inv) = match hits.as_slice() {
            [h] => (h.0.clone(), h.1.clone()),
            [] => {
                return Err(format!(
                    "no invoice {:?} (give its id or number)",
                    i.invoice
                ));
            }
            _ => return Err(format!("invoice {:?} is ambiguous: give its id", i.invoice)),
        };
        if amount < 0 {
            return Err("a customer invoice is paid by money in; this line is money out".into());
        }
        if links.iter().any(|(_, f)| text(f, "invoice_id") == iid) {
            return Err(
                "this invoice is already matched to a bank line (nothing was written)".into(),
            );
        }
        let c = candidates(env)?
            .into_iter()
            .find(|c| c.invoice_id == iid)
            .ok_or_else(|| {
                format!(
                    "the invoice is {}; only a posted invoice with an amount is matched",
                    text(&inv, "state")
                )
            })?;
        if c.amount != amount {
            return Err(format!(
                "the invoice amount {} differs from the bank line {amount} (minor units); only an exact match is booked",
                c.amount
            ));
        }
        if c.basis != "move" {
            notes.push("the invoice has no ledger move (invoice_posting.create_move), so its amount is the net of its lines; choose the counter account with that in mind".into());
        }
        party_id = text(&inv, "party_id");
        invoice_id = iid;
    }
    let abs = amount.checked_abs().ok_or("the amount overflows")?;
    let label = text(&line, "label");
    let (bd, bc, cd, cc) = if amount > 0 {
        (abs, 0, 0, abs)
    } else {
        (0, abs, abs, 0)
    };
    let lines = vec![
        Line {
            account_id: bank_id,
            account: text(&bank, "code"),
            label: label.clone(),
            debit: bd,
            credit: bc,
        },
        Line {
            account_id: ctr_id,
            account: text(&ctr, "code"),
            label: label.clone(),
            debit: cd,
            credit: cc,
        },
    ];
    Ok(Planned {
        bank_line_id: lid,
        invoice_id,
        party_id,
        journal_id,
        date,
        reference: label,
        amount,
        lines,
        notes,
    })
}

/// Plan, and unless `dry_run` create the draft move, its lines and the link as the caller.
pub fn book(
    env: Env<'_>,
    p: &Principal,
    i: &Input,
    dry_run: bool,
) -> Result<(Planned, Option<String>), String> {
    let pl = plan(env, p, i)?;
    if dry_run {
        return Ok((pl, None));
    }
    let mut mv = json!({"journal_id": pl.journal_id, "date": pl.date, "reference": pl.reference,
        "narration": "Bank statement line"});
    if !pl.party_id.is_empty() {
        mv["partner_id"] = json!(pl.party_id);
    }
    let made = call(env, p, "accounting_move.create", mv)?;
    let mid = made["id"]
        .as_str()
        .ok_or("accounting_move.create gave no id")?
        .to_string();
    let rows: Vec<J> = pl
        .lines
        .iter()
        .map(|l| {
            json!({"move_id": mid, "account_id": l.account_id, "label": l.label,
                "debit": l.debit, "credit": l.credit})
        })
        .collect();
    if let Err(e) = crate::sie::bulk(env, p, "accounting_move_line", rows, None, false) {
        let _ = call(env, p, "accounting_move.cancel", json!({"id": mid}));
        return Err(format!(
            "the move lines were refused, the draft move {mid} was cancelled: {e}"
        ));
    }
    env.store.insert(
        MODEL,
        [
            ("bank_line_id".to_string(), Value::text(&pl.bank_line_id)),
            ("invoice_id".to_string(), Value::text(&pl.invoice_id)),
            ("move_id".to_string(), Value::text(&mid)),
        ]
        .into(),
    )?;
    Ok((pl, Some(mid)))
}

fn s(input: &Value, k: &str) -> String {
    match input {
        Value::Map(m) => match m.get(k) {
            Some(Value::Text(t)) => t.clone(),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

pub fn register(d: &Arc<Dispatcher>, store: &Store) {
    let m = module();
    let (weak, st): (Weak<Dispatcher>, Store) = (Arc::downgrade(d), store.clone());
    let (weak2, st2) = (weak.clone(), st.clone());
    d.register_with_actor(
        &m,
        MODEL,
        "suggest",
        Box::new(move |input| {
            let a = Actor::from_input(input);
            let groups: Vec<&str> = a.groups.iter().map(String::as_str).collect();
            let p = Principal::user(&a.caller, &groups);
            let d = weak2.upgrade().ok_or("the dispatcher is gone")?;
            let sug = suggest(Env { d: &d, store: &st2 }, &p, &s(input, "bank_line"))?;
            let items = sug
                .iter()
                .map(|x| {
                    let cands = x
                        .matches
                        .iter()
                        .map(|(c, r)| {
                            Value::map([
                                ("invoice_id", Value::text(&c.invoice_id)),
                                ("number", Value::text(&c.number)),
                                ("amount", Value::Int(c.amount)),
                                ("basis", Value::text(c.basis)),
                                ("reference_match", Value::Bool(*r)),
                            ])
                        })
                        .collect();
                    Value::map([
                        ("bank_line_id", Value::text(&x.bank_line_id)),
                        ("date", Value::text(&x.date)),
                        ("label", Value::text(&x.label)),
                        ("amount", Value::Int(x.amount)),
                        ("candidates", Value::List(cands)),
                    ])
                })
                .collect();
            Ok(Value::map([
                ("ok", Value::Bool(true)),
                ("suggestions", Value::List(items)),
            ]))
        }),
    );
    d.register_with_actor(
        &m,
        MODEL,
        "book",
        Box::new(move |input| {
            let dry = matches!(input, Value::Map(m) if matches!(m.get("dry_run"), Some(Value::Bool(true))));
            let a = Actor::from_input(input);
            if a.agent && !dry {
                return Err("an agent may only dry-run the bank booking (dry_run=true); ask the person to run it".into());
            }
            let groups: Vec<&str> = a.groups.iter().map(String::as_str).collect();
            let p = Principal::user(&a.caller, &groups);
            let d = weak.upgrade().ok_or("the dispatcher is gone")?;
            let inp = Input {
                bank_line: s(input, "bank_line"),
                invoice: s(input, "invoice"),
                counter_account: s(input, "counter_account"),
                bank_account: s(input, "bank_account"),
                date: s(input, "date"),
            };
            let (pl, mid) = book(Env { d: &d, store: &st }, &p, &inp, dry)?;
            let lines = pl
                .lines
                .iter()
                .map(|l| {
                    Value::map([
                        ("account", Value::text(&l.account)),
                        ("debit", Value::Int(l.debit)),
                        ("credit", Value::Int(l.credit)),
                    ])
                })
                .collect();
            let mut out = vec![
                ("ok", Value::Bool(true)),
                ("written", Value::Bool(!dry)),
                ("amount", Value::Int(pl.amount)),
                ("lines", Value::List(lines)),
                (
                    "notes",
                    Value::List(pl.notes.iter().map(|n| Value::text(n)).collect()),
                ),
            ];
            if let Some(m) = mid {
                out.push(("move_id", Value::text(&m)));
            }
            Ok(Value::Map(
                out.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            ))
        }),
    );
}
