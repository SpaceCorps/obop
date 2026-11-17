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
//! and the bank line's own `state` is not changed. The link row's id is the bank line's id, so a
//! line gets one booking even when two runs race; `get` reads it.
//!
//! `unbook` takes a booking back in one batch of the store (docs/ISSUES.md Q11): the draft move is
//! cancelled, the bank line is unmatched (its `settles_line_id`, and its `move_line_id` when that
//! is a line of the move, are emptied) and the link row is deleted, so the invoice can be paid
//! again. A posted move is refused (reverse it instead), and so is a reconciled bank line and a
//! move dated in a closed period.

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
                f("move_state", "Answer, get: the state of the move"),
                f(
                    "move_cancelled",
                    "Answer, unbook: whether the draft move was cancelled",
                )
                .kind(FieldKind::Bool),
                f(
                    "unmatched",
                    "Answer, unbook: whether the bank line went back to unmatched",
                )
                .kind(FieldKind::Bool),
                f(
                    "freed_item",
                    "Answer, unbook: the journal item the line no longer settles",
                ),
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
                    "get",
                    "The booking of a bank statement line (bank_line): bank_line_id, invoice_id, move_id and the move's state; refused in words when the line has none",
                    GROUP,
                ),
                CommandDef::new(
                    "unbook",
                    "Take back the booking of a bank statement line (bank_line) in one step: the draft move is cancelled, the line is unmatched and no longer settles an item, and the link is deleted, so the invoice can be paid again. Refused when the move is posted (reverse it), the line is reconciled or the move is in a closed period. dry_run writes nothing; an agent may only dry-run",
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

/// The ledger move of an invoice that is not cancelled (`invoice_posting` links the two).
fn live_move(
    posting: &[(String, Fields)],
    moves: &[(String, Fields)],
    invoice_id: &str,
) -> Option<String> {
    posting
        .iter()
        .filter(|(_, l)| text(l, "invoice_id") == invoice_id)
        .map(|(_, l)| text(l, "move_id"))
        .find(|m| {
            moves
                .iter()
                .any(|(mid, mf)| mid == m && text(mf, "state") != "cancelled")
        })
}

/// The receivable item of an invoice: the one debit line of its live ledger move. None when the
/// invoice has no move or its move has more than one debit line.
fn receivable_item(
    posting: &[(String, Fields)],
    moves: &[(String, Fields)],
    mlines: &[(String, Fields)],
    invoice_id: &str,
) -> Option<String> {
    let m = live_move(posting, moves, invoice_id)?;
    let debits: Vec<&String> = mlines
        .iter()
        .filter(|(_, l)| text(l, "move_id") == m && int(l, "debit") > 0)
        .map(|(id, _)| id)
        .collect();
    match debits.as_slice() {
        [one] => Some((*one).clone()),
        _ => None,
    }
}

/// The items bank transactions already settle (`accounting_bank_statement_line.settles_line_id`),
/// each with the transaction that settles it.
fn settled_items(env: Env<'_>) -> Result<Vec<(String, String)>, String> {
    Ok(list(env, "accounting_bank_statement_line")?
        .into_iter()
        .map(|(id, f)| (text(&f, "settles_line_id"), id))
        .filter(|(item, _)| !item.is_empty())
        .collect())
}

/// The posted invoices that are not yet linked to a bank line and whose receivable item no bank
/// transaction settles, with their amounts.
pub fn candidates(env: Env<'_>) -> Result<Vec<Candidate>, String> {
    let taken: Vec<String> = list(env, MODEL)?
        .iter()
        .map(|(_, f)| text(f, "invoice_id"))
        .filter(|s| !s.is_empty())
        .collect();
    let settled = settled_items(env)?;
    let moves = list(env, "accounting_move")?;
    let mlines = list(env, "accounting_move_line")?;
    let posting = list(env, "invoice_posting").unwrap_or_default();
    let ilines = list(env, "invoice_line")?;
    let mut out = Vec::new();
    for (id, inv) in list(env, "invoice")? {
        if text(&inv, "state") != "posted" || taken.contains(&id) {
            continue;
        }
        if receivable_item(&posting, &moves, &mlines, &id)
            .is_some_and(|item| settled.iter().any(|(s, _)| *s == item))
        {
            continue;
        }
        let (amount, basis) = match live_move(&posting, &moves, &id) {
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
    /// The invoice's receivable item that the bank line is to settle (empty: none known).
    pub settles_line_id: String,
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
    let mut settles_line_id = String::new();
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
        let item = receivable_item(
            &list(env, "invoice_posting").unwrap_or_default(),
            &list(env, "accounting_move")?,
            &list(env, "accounting_move_line")?,
            &iid,
        );
        if let Some(item) = item {
            if let Some((_, by)) = settled_items(env)?
                .into_iter()
                .find(|(s, by)| *s == item && *by != lid)
            {
                return Err(format!(
                    "the invoice's receivable item is already paid by bank transaction {by} (nothing was written)"
                ));
            }
            let own = text(&line, "settles_line_id");
            if !own.is_empty() && own != item {
                return Err(
                    "the bank line already settles another item; undo that first (nothing was written)"
                        .into(),
                );
            }
            settles_line_id = item;
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
            // No receivable was ever debited for an invoice that is not in the ledger: crediting
            // one with its payment would leave a credit that matches nothing, while the revenue
            // and the VAT are never booked.
            if text(&ctr, "account_type") == "asset_receivable" {
                return Err(format!(
                    "the invoice is not in the ledger yet (it has no move), so no receivable was booked for it and {} cannot be credited with its payment; book the invoice first (invoice_posting.create_move) and match its receivable item, or book the payment to a revenue account (nothing was written)",
                    text(&ctr, "code")
                ));
            }
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
        settles_line_id,
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
    // The invoice's receivable item is recorded on the bank line first, so that the item's own
    // proposal can no longer pay it a second time. The field is unique: a race is refused here,
    // before anything else is written.
    let settles = !pl.settles_line_id.is_empty()
        && list(env, "accounting_bank_statement_line")?
            .iter()
            .any(|(id, f)| {
                *id == pl.bank_line_id && text(f, "settles_line_id") != pl.settles_line_id
            });
    if settles {
        call(
            env,
            p,
            "accounting_bank_statement_line.update",
            json!({"id": pl.bank_line_id, "settles_line_id": pl.settles_line_id}),
        )?;
    }
    let undo = |env: Env<'_>| {
        if settles {
            let _ = call(
                env,
                p,
                "accounting_bank_statement_line.update",
                json!({"id": pl.bank_line_id, "settles_line_id": null}),
            );
        }
    };
    let mut mv = json!({"journal_id": pl.journal_id, "date": pl.date, "reference": pl.reference,
        "narration": "Bank statement line"});
    if !pl.party_id.is_empty() {
        mv["partner_id"] = json!(pl.party_id);
    }
    let made = call(env, p, "accounting_move.create", mv).inspect_err(|_| undo(env))?;
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
        undo(env);
        return Err(format!(
            "the move lines were refused, the draft move {mid} was cancelled: {e}"
        ));
    }
    // Keyed by the bank line: a second booking of the line (a race) is refused by the store, and
    // what this run made is taken back.
    let mut link = Fields::new();
    link.insert("bank_line_id".into(), J::from(pl.bank_line_id.as_str()));
    link.insert("invoice_id".into(), J::from(pl.invoice_id.as_str()));
    link.insert("move_id".into(), J::from(mid.as_str()));
    if let Err(e) = env.store.backend().insert(MODEL, &pl.bank_line_id, &link) {
        let _ = call(env, p, "accounting_move.cancel", json!({"id": mid}));
        undo(env);
        return Err(format!(
            "this bank line already has a move; one line gets one (the draft move {mid} was cancelled): {e}"
        ));
    }
    Ok((pl, Some(mid)))
}

/// The link row of a bank line.
fn link_of(env: Env<'_>, bank_line: &str) -> Result<Option<(String, Fields)>, String> {
    Ok(list(env, MODEL)?
        .into_iter()
        .find(|(_, f)| text(f, "bank_line_id") == bank_line))
}

/// What `unbook` does, as planned.
pub struct Unbooked {
    pub move_id: String,
    pub cancelled: bool,
    pub unmatched: bool,
    pub settled_item: String,
}

/// Take back the booking of a bank line in one batch of the store; with `dry_run` only say what
/// would be done.
pub fn unbook(
    env: Env<'_>,
    p: &Principal,
    bank_line: &str,
    dry_run: bool,
) -> Result<Unbooked, String> {
    need(p)?;
    if bank_line.is_empty() {
        return Err("bank_line is required: the id of the bank statement line".into());
    }
    let b = env.store.backend();
    let line = b
        .get("accounting_bank_statement_line", bank_line)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no bank statement line {bank_line:?}"))?;
    let Some((link_id, link)) = link_of(env, bank_line)? else {
        return Err("the bank line has no booking to take back (nothing was written)".into());
    };
    let state = text(&line, "state");
    if state == "reconciled" {
        return Err(
            "the bank line is reconciled and cannot be changed; nothing was written".into(),
        );
    }
    let mid = text(&link, "move_id");
    let mv = b.get("accounting_move", &mid).map_err(|e| e.to_string())?;
    let move_state = mv.as_ref().map(|f| text(f, "state")).unwrap_or_default();
    if move_state == "posted" || move_state == "reversed" {
        return Err(format!(
            "the entry of this booking is {move_state}; reverse it (accounting_move.reverse) and book the opposite entry instead (nothing was written)"
        ));
    }
    let date = mv.as_ref().map(|f| text(f, "date")).unwrap_or_default();
    if let Ok(periods) = b.list("accounting_period")
        && periods.iter().any(|(_, f)| {
            text(f, "state") == "closed" && text(f, "start_on") <= date && date <= text(f, "end_on")
        })
    {
        return Err(format!(
            "the entry is dated {date}, in a closed period, and cannot be cancelled (nothing was written)"
        ));
    }
    let move_lines: Vec<String> = list(env, "accounting_move_line")?
        .into_iter()
        .filter(|(_, f)| text(f, "move_id") == mid)
        .map(|(id, _)| id)
        .collect();
    let cancel = move_state == "draft";
    let unmatch = state == "matched";
    let settled_item = text(&line, "settles_line_id");
    let out = Unbooked {
        move_id: mid.clone(),
        cancelled: cancel,
        unmatched: unmatch,
        settled_item: settled_item.clone(),
    };
    if dry_run {
        return Ok(out);
    }
    let mut ops = Vec::new();
    if cancel {
        let mut f = Fields::new();
        f.insert("state".into(), J::from("cancelled"));
        ops.push(obop_storage::Op::Merge {
            model: "accounting_move".into(),
            id: mid.clone(),
            patch: f,
        });
    }
    let mut patch = Fields::new();
    if !settled_item.is_empty() {
        patch.insert("settles_line_id".into(), J::Null);
    }
    if move_lines.contains(&text(&line, "move_line_id")) {
        patch.insert("move_line_id".into(), J::Null);
    }
    if unmatch {
        patch.insert("state".into(), J::from("unmatched"));
    }
    if !patch.is_empty() {
        ops.push(obop_storage::Op::Merge {
            model: "accounting_bank_statement_line".into(),
            id: bank_line.to_string(),
            patch,
        });
    }
    ops.push(obop_storage::Op::Delete {
        model: MODEL.into(),
        id: link_id,
    });
    b.apply(&ops)
        .map_err(|e| format!("the booking could not be taken back (nothing was written): {e}"))?;
    Ok(out)
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
    let st3 = st.clone();
    d.register(
        &m,
        MODEL,
        "get",
        Box::new(move |input| {
            let lid = s(input, "bank_line");
            let lid = if lid.is_empty() { s(input, "id") } else { lid };
            let b = st3.backend();
            let rows = b.list(MODEL).map_err(|e| e.to_string())?;
            let Some((_, f)) = rows.iter().find(|(_, f)| text(f, "bank_line_id") == lid) else {
                return Err(format!("the bank line {lid:?} has no booking"));
            };
            let mid = text(f, "move_id");
            let state = b
                .get("accounting_move", &mid)
                .map_err(|e| e.to_string())?
                .map(|m| text(&m, "state"))
                .unwrap_or_default();
            Ok(Value::map([
                ("id", Value::text(&lid)),
                ("bank_line_id", Value::text(&lid)),
                ("invoice_id", Value::text(&text(f, "invoice_id"))),
                ("move_id", Value::text(&mid)),
                ("move_state", Value::text(&state)),
            ]))
        }),
    );
    let (weak3, st4) = (weak.clone(), st.clone());
    d.register_with_actor(
        &m,
        MODEL,
        "unbook",
        Box::new(move |input| {
            let dry = matches!(input, Value::Map(m) if matches!(m.get("dry_run"), Some(Value::Bool(true))));
            let a = Actor::from_input(input);
            if a.agent && !dry {
                return Err("an agent may only dry-run taking back a bank booking (dry_run=true); ask the person to run it".into());
            }
            let groups: Vec<&str> = a.groups.iter().map(String::as_str).collect();
            let p = Principal::user(&a.caller, &groups);
            let d = weak3.upgrade().ok_or("the dispatcher is gone")?;
            let u = unbook(Env { d: &d, store: &st4 }, &p, &s(input, "bank_line"), dry)?;
            Ok(Value::map([
                ("ok", Value::Bool(true)),
                ("written", Value::Bool(!dry)),
                ("move_id", Value::text(&u.move_id)),
                ("move_cancelled", Value::Bool(u.cancelled)),
                ("unmatched", Value::Bool(u.unmatched)),
                ("freed_item", Value::text(&u.settled_item)),
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
