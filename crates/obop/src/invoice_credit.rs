//! The typed command `invoice_credit.create`: credit a POSTED customer invoice with a DRAFT credit
//! invoice and a DRAFT reversing accounting move (docs/spec/accounting-se.md section 6.2).
//!
//! Ported in idea from open-accounting-sweden (`credit_invoice`: one credit per invoice, a credit
//! note cannot be credited, only an issued invoice qualifies), not in code.
//!
//! Representation: `invoice_line.quantity` is a whole number and `invoice.total_minor` is the sum
//! of the line amounts, so the credit invoice keeps each line's description and quantity and
//! carries a NEGATIVE `unit_price_minor` (amount = quantity times unit price, so negative). The
//! credit invoice's total is therefore negative. `invoice.post` accepts a
//! negative total, so the credit invoice can be posted (docs/ISSUES.md K64). The reversing move swaps debit and credit of every line of the original
//! `invoice_posting` move (account, label, tax kept) and stays a draft: posting it is
//! `accounting_move.post`. A row of `invoice_credit` (original, credit invoice, move) is the link;
//! one invoice gets one credit, a credit invoice cannot be credited, a non-posted invoice cannot
//! be credited. When the original was built by `time_billing`, its link rows are deleted so the
//! entries can be billed again. An agent may only dry-run. UNVERIFIED against Swedish rules.

use crate::sie::{Env, call};
use obop_commands::{Actor, Dispatcher, Principal, Store, Value};
use obop_registry::{CommandDef, FieldDef, FieldKind, ModelDef, Module};
use obop_storage::Fields;
use obop_types::DataClass;
use serde_json::{Value as J, json};
use std::sync::{Arc, Weak};

const MODEL: &str = "invoice_credit";
const GROUP: &str = "invoicing.user";
const LEDGER: &str = "accounting.user";

pub fn module() -> Module {
    let f = |name, description| FieldDef::new(name, description, DataClass::None, false, GROUP);
    Module {
        id: "se_invoice_credit".into(),
        name: "Credit invoice".into(),
        description: "Credits a posted customer invoice with a draft credit invoice and a draft reversing move".into(),
        models: vec![ModelDef {
            name: MODEL,
            description: "`create` credits a posted `invoice` that has an `invoice_posting` move: a DRAFT credit `invoice` (same party and currency, the same lines with negative unit prices) and a DRAFT `accounting_move` that reverses the original move. Rows of this model are the link (original invoice, credit invoice, move). Needs the invoicing module and the accounting package. UNVERIFIED against Swedish rules",
            read_group: GROUP,
            write_group: GROUP,
            access: None,
            fields: vec![
                f("invoice", "Input: the id or the number of the posted invoice to credit"),
                f("date", "Input, optional: the accounting date of the reversing move, YYYY-MM-DD; default is the date of the original move"),
                f("dry_run", "Input: only show what would be written").kind(FieldKind::Bool),
                f("original_id", "Link: the credited invoice id"),
                f("credit_invoice_id", "Link and answer: the draft credit invoice id"),
                f("move_id", "Link and answer: the draft reversing move id"),
                f("ok", "Answer: whether the credit was built").kind(FieldKind::Bool),
                f("written", "Answer: whether anything was created").kind(FieldKind::Bool),
                f("net", "Answer: the credit invoice total in minor units (negative)").kind(FieldKind::Number),
                f("lines", "Answer: the move lines: account, debit, credit, tax_id"),
                f("freed_entries", "Answer: how many billed time entries were freed").kind(FieldKind::Number),
                f("notes", "Answer: remarks (a list)"),
            ],
            commands: vec![CommandDef::new(
                "create",
                "Credit a posted invoice (invoice) that has a move from invoice_posting.create_move: creates a draft credit invoice with negative unit prices and a draft reversing move, and frees time entries the invoice billed. Refused in words when the invoice is not posted, has no move, is itself a credit note or already has a credit. dry_run writes nothing. An agent may only dry-run. Both documents stay drafts. UNVERIFIED against Swedish rules",
                GROUP,
            )],
            ext: Default::default(),
        }],
        ..Module::default()
    }
}

fn text(f: &Fields, k: &str) -> String {
    f.get(k).and_then(J::as_str).unwrap_or_default().to_string()
}

#[derive(Default)]
pub struct Input {
    pub invoice: String,
    pub date: String,
}

/// A planned reversing move line (debit and credit already swapped).
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub account_id: String,
    pub label: String,
    pub debit: i64,
    pub credit: i64,
    pub tax_id: Option<String>,
}

pub struct CreditLine {
    pub description: String,
    pub quantity: i64,
    pub unit_price_minor: i64,
}

pub struct Planned {
    pub original_id: String,
    pub reference: String,
    pub party_id: String,
    pub currency: String,
    pub journal_id: String,
    pub date: String,
    pub invoice_lines: Vec<CreditLine>,
    pub net: i64,
    pub lines: Vec<Line>,
    /// The `time_billing` link rows (row ids) to delete.
    pub time_links: Vec<String>,
    pub notes: Vec<String>,
}

/// Build the plan. Reads only; every refusal is in words.
pub fn plan(env: Env<'_>, p: &Principal, i: &Input) -> Result<Planned, String> {
    if !p.allows(GROUP) || !p.allows(LEDGER) {
        return Err(format!(
            "crediting an invoice needs the {GROUP} and {LEDGER} groups (an agent needs them in its scopes)"
        ));
    }
    let b = env.store.backend();
    let list = |m: &str| -> Result<Vec<(String, Fields)>, String> {
        b.list(m).map_err(|e| {
            format!("cannot read {m}: {e} (are the invoicing module and the accounting package installed?)")
        })
    };
    if i.invoice.is_empty() {
        return Err("invoice is required (its id or number)".into());
    }
    let invoices = list("invoice")?;
    let hits: Vec<_> = invoices
        .iter()
        .filter(|(id, f)| *id == i.invoice || text(f, "number") == i.invoice)
        .collect();
    let (iid, inv) = match hits.as_slice() {
        [(id, f)] => (id.clone(), f.clone()),
        [] => {
            return Err(format!(
                "no invoice {:?} (give its id or number)",
                i.invoice
            ));
        }
        _ => return Err(format!("invoice {:?} is ambiguous: give its id", i.invoice)),
    };
    let links = list(MODEL)?;
    if links
        .iter()
        .any(|(_, f)| text(f, "credit_invoice_id") == iid)
    {
        return Err("a credit invoice cannot be credited".into());
    }
    let state = text(&inv, "state");
    if state != "posted" {
        return Err(format!(
            "the invoice is {state}; only a posted invoice can be credited"
        ));
    }
    if links.iter().any(|(_, f)| text(f, "original_id") == iid) {
        return Err(
            "this invoice already has a credit invoice; one invoice gets one (nothing was written)"
                .into(),
        );
    }
    let mid = list("invoice_posting")?
        .into_iter()
        .find(|(_, f)| text(f, "invoice_id") == iid)
        .map(|(_, f)| text(&f, "move_id"))
        .ok_or("the invoice has no ledger move (invoice_posting.create_move); there is nothing to reverse")?;
    let mv = list("accounting_move")?
        .into_iter()
        .find(|(id, _)| *id == mid)
        .map(|(_, f)| f)
        .ok_or_else(|| format!("the move {mid} of the invoice no longer exists"))?;
    if text(&mv, "state") == "cancelled" {
        return Err(format!(
            "the move {mid} of the invoice is cancelled; nothing to reverse"
        ));
    }
    let date = if i.date.is_empty() {
        text(&mv, "date")
    } else {
        i.date.clone()
    };
    if crate::sie::from_iso(&date).is_none() {
        return Err("date must be YYYY-MM-DD (the original move has no usable date)".into());
    }
    let mut lines = Vec::new();
    for (_, f) in list("accounting_move_line")? {
        if text(&f, "move_id") != mid {
            continue;
        }
        let tax = text(&f, "tax_id");
        lines.push(Line {
            account_id: text(&f, "account_id"),
            label: text(&f, "label"),
            debit: f.get("credit").and_then(J::as_i64).unwrap_or(0),
            credit: f.get("debit").and_then(J::as_i64).unwrap_or(0),
            tax_id: (!tax.is_empty()).then_some(tax),
        });
    }
    if lines.is_empty() {
        return Err(format!("the move {mid} has no lines; nothing to reverse"));
    }
    let mut invoice_lines = Vec::new();
    let mut net = 0i64;
    for (_, f) in list("invoice_line")? {
        if text(&f, "invoice_id") != iid {
            continue;
        }
        let q = f.get("quantity").and_then(J::as_i64).unwrap_or(0);
        let u = f.get("unit_price_minor").and_then(J::as_i64).unwrap_or(0);
        let neg = u.checked_neg().ok_or("a unit price overflows")?;
        net = net
            .checked_add(q.checked_mul(neg).ok_or("a line amount overflows")?)
            .ok_or("the total overflows")?;
        invoice_lines.push(CreditLine {
            description: text(&f, "description"),
            quantity: q,
            unit_price_minor: neg,
        });
    }
    if invoice_lines.is_empty() {
        return Err("the invoice has no lines; nothing to credit".into());
    }
    let time_links: Vec<String> = list("time_billing")
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, f)| text(f, "invoice_id") == iid)
        .map(|(id, _)| id)
        .collect();
    let number = text(&inv, "number");
    let reference = if number.is_empty() {
        iid.clone()
    } else {
        number
    };
    let mut notes = vec![
        "UNVERIFIED: nothing is checked against Swedish credit note rules or the VAT act; both documents are drafts".into(),
        "the credit invoice has a negative total (negative unit prices); invoice.post asserts total > 0, so it cannot be posted yet (docs/ISSUES.md K64)".into(),
    ];
    if !time_links.is_empty() {
        notes.push(format!(
            "{} billed time entries are freed and can be billed again",
            time_links.len()
        ));
    }
    Ok(Planned {
        original_id: iid,
        reference,
        party_id: text(&inv, "party_id"),
        currency: text(&inv, "currency"),
        journal_id: text(&mv, "journal_id"),
        date,
        invoice_lines,
        net,
        lines,
        time_links,
        notes,
    })
}

/// What was made.
pub struct Made {
    pub credit_invoice_id: String,
    pub move_id: String,
    pub freed: usize,
}

/// Plan, and unless `dry_run` create the credit invoice, the reversing move, the link and free
/// the time entries, as the caller.
pub fn create(
    env: Env<'_>,
    p: &Principal,
    i: &Input,
    dry_run: bool,
) -> Result<(Planned, Option<Made>), String> {
    let pl = plan(env, p, i)?;
    if dry_run {
        return Ok((pl, None));
    }
    let mut inv = json!({"state": "draft"});
    if !pl.party_id.is_empty() {
        inv["party_id"] = json!(pl.party_id);
    }
    if !pl.currency.is_empty() {
        inv["currency"] = json!(pl.currency);
    }
    let cid = call(env, p, "invoice.create", inv)?["id"]
        .as_str()
        .ok_or("invoice.create gave no id")?
        .to_string();
    let rows: Vec<J> = pl
        .invoice_lines
        .iter()
        .map(|l| {
            json!({"invoice_id": cid, "description": l.description, "quantity": l.quantity,
                "unit_price_minor": l.unit_price_minor})
        })
        .collect();
    if let Err(e) = crate::sie::bulk(env, p, "invoice_line", rows, None, false) {
        let _ = call(env, p, "invoice.cancel", json!({"id": cid}));
        return Err(format!(
            "the credit lines were refused, the draft credit invoice {cid} was cancelled: {e}"
        ));
    }
    let mut mv = json!({"journal_id": pl.journal_id, "date": pl.date,
        "reference": format!("Credit {}", pl.reference),
        "narration": format!("Credit note for customer invoice {}", pl.reference)});
    if !pl.party_id.is_empty() {
        mv["partner_id"] = json!(pl.party_id);
    }
    let mid = match call(env, p, "accounting_move.create", mv) {
        Ok(v) => v["id"]
            .as_str()
            .ok_or("accounting_move.create gave no id")?
            .to_string(),
        Err(e) => {
            let _ = call(env, p, "invoice.cancel", json!({"id": cid}));
            return Err(format!(
                "the reversing move was refused, the draft credit invoice {cid} was cancelled: {e}"
            ));
        }
    };
    let rows: Vec<J> = pl
        .lines
        .iter()
        .map(|l| {
            let mut r = json!({"move_id": mid, "account_id": l.account_id, "label": l.label,
                "debit": l.debit, "credit": l.credit});
            if let Some(t) = &l.tax_id {
                r["tax_id"] = json!(t);
            }
            r
        })
        .collect();
    if let Err(e) = crate::sie::bulk(env, p, "accounting_move_line", rows, None, false) {
        let _ = call(env, p, "accounting_move.cancel", json!({"id": mid}));
        let _ = call(env, p, "invoice.cancel", json!({"id": cid}));
        return Err(format!(
            "the move lines were refused, the draft move {mid} and the draft credit invoice {cid} were cancelled: {e}"
        ));
    }
    env.store.insert(
        MODEL,
        [
            ("original_id".to_string(), Value::text(&pl.original_id)),
            ("credit_invoice_id".to_string(), Value::text(&cid)),
            ("move_id".to_string(), Value::text(&mid)),
        ]
        .into(),
    )?;
    let mut freed = 0;
    for id in &pl.time_links {
        if env
            .store
            .backend()
            .delete("time_billing", id)
            .map_err(|e| e.to_string())?
        {
            freed += 1;
        }
    }
    Ok((
        pl,
        Some(Made {
            credit_invoice_id: cid,
            move_id: mid,
            freed,
        }),
    ))
}

pub fn register(d: &Arc<Dispatcher>, store: &Store) {
    let m = module();
    let (weak, st): (Weak<Dispatcher>, Store) = (Arc::downgrade(d), store.clone());
    d.register_with_actor(
        &m,
        MODEL,
        "create",
        Box::new(move |input| {
            let Value::Map(map) = input else {
                return Err("invoice is required".into());
            };
            let s = |k: &str| match map.get(k) {
                Some(Value::Text(t)) => t.clone(),
                _ => String::new(),
            };
            let dry = matches!(map.get("dry_run"), Some(Value::Bool(true)));
            let a = Actor::from_input(input);
            if a.agent && !dry {
                return Err(
                    "an agent may only dry-run the credit (dry_run=true); ask the person to run it"
                        .into(),
                );
            }
            let groups: Vec<&str> = a.groups.iter().map(String::as_str).collect();
            let p = Principal::user(&a.caller, &groups);
            let d = weak.upgrade().ok_or("the dispatcher is gone")?;
            let inp = Input {
                invoice: s("invoice"),
                date: s("date"),
            };
            let (pl, made) = create(Env { d: &d, store: &st }, &p, &inp, dry)?;
            let lines = pl
                .lines
                .iter()
                .map(|l| {
                    Value::map([
                        ("account_id", Value::text(&l.account_id)),
                        ("debit", Value::Int(l.debit)),
                        ("credit", Value::Int(l.credit)),
                        ("tax_id", Value::text(l.tax_id.as_deref().unwrap_or(""))),
                    ])
                })
                .collect();
            let mut out = vec![
                ("ok", Value::Bool(true)),
                ("written", Value::Bool(!dry)),
                ("net", Value::Int(pl.net)),
                ("lines", Value::List(lines)),
                (
                    "notes",
                    Value::List(pl.notes.iter().map(|s| Value::text(s)).collect()),
                ),
            ];
            if let Some(m) = made {
                out.push(("credit_invoice_id", Value::text(&m.credit_invoice_id)));
                out.push(("move_id", Value::text(&m.move_id)));
                out.push(("freed_entries", Value::Int(m.freed as i64)));
            }
            Ok(Value::Map(
                out.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            ))
        }),
    );
}
