//! The typed command `invoice_posting.create_move`: book a posted customer invoice as a DRAFT
//! accounting move (docs/spec/accounting-se.md section 2, stage 5).
//!
//! The `invoicing` module has no account, tax or VAT on its lines, so everything the voucher needs
//! comes from the caller or from rows the user made; no BAS account, rate or code is built in
//! (D34) and nothing is checked against Swedish invoice rules or the VAT act.
//!
//! The move has: one receivable debit for net plus VAT; one revenue credit per invoice line on
//! `revenue_account`, carrying the `tax` as `tax_id` (so `vat_computation.compute` finds the base);
//! one VAT credit on the tax's `tax_account_id` (or `vat_account`), carrying the same `tax_id`.
//! VAT is the net total times the tax's `rate_bp`, rounded half up once on the total, in integer
//! minor units. The move stays a draft: posting it is `accounting_move.post`, which needs
//! `accounting.approver` and a person's approval. An agent may only dry-run. One invoice gets one
//! move: a row of `invoice_posting` (invoice id, move id) is the link, and a second run is refused.

use crate::sie::{Env, call};
use obop_commands::{Actor, Dispatcher, Principal, Store, Value};
use obop_registry::{CommandDef, FieldDef, FieldKind, ModelDef, Module};
use obop_storage::Fields;
use obop_types::DataClass;
use serde_json::{Value as J, json};
use std::sync::{Arc, Weak};

const MODEL: &str = "invoice_posting";
const GROUP: &str = "accounting.user";

pub fn module() -> Module {
    let f = |name, description| FieldDef::new(name, description, DataClass::None, false, GROUP);
    Module {
        id: "se_invoice_posting".into(),
        name: "Invoice to ledger".into(),
        description: "Books a posted customer invoice as a draft accounting move".into(),
        models: vec![ModelDef {
            name: MODEL,
            description: "`create_move` books a posted `invoice` as a DRAFT `accounting_move` with a receivable debit, revenue credits and a VAT credit, using only the accounts, journal and tax you name. Rows of this model are the link from an invoice to its move (one each). Needs the accounting package. UNVERIFIED: nothing is checked against Swedish invoice or VAT rules",
            read_group: GROUP,
            write_group: GROUP,
            access: None,
            fields: vec![
                f(
                    "invoice",
                    "Input: the id or the number of the posted invoice",
                ),
                f(
                    "journal",
                    "Input: the id, code or name of the accounting journal",
                ),
                f(
                    "receivable_account",
                    "Input: the id or code of the receivable account (debit)",
                ),
                f(
                    "revenue_account",
                    "Input: the id or code of the revenue account (credit)",
                ),
                f(
                    "tax",
                    "Input: the id or name of the accounting tax; its rate_bp gives the VAT and it is the tax_id of the base and VAT lines",
                ),
                f(
                    "vat_account",
                    "Input, optional: the id or code of the VAT account; default is the tax's tax_account_id",
                ),
                f(
                    "date",
                    "Input: the accounting date, YYYY-MM-DD (an invoice has no date of its own)",
                ),
                f("dry_run", "Input: only show the lines, write nothing").kind(FieldKind::Bool),
                f("invoice_id", "Link: the invoice id"),
                f("move_id", "Link and answer: the draft move id"),
                f("ok", "Answer: whether the move was built").kind(FieldKind::Bool),
                f("written", "Answer: whether the move was created").kind(FieldKind::Bool),
                f("net", "Answer: net total in minor units").kind(FieldKind::Number),
                f("vat", "Answer: VAT in minor units").kind(FieldKind::Number),
                f("gross", "Answer: receivable debit in minor units").kind(FieldKind::Number),
                f(
                    "lines",
                    "Answer: the move lines: account, debit, credit, tax",
                ),
                f("notes", "Answer: remarks (a list)"),
            ],
            commands: vec![CommandDef::new(
                "create_move",
                "Book a posted invoice (invoice) as a draft accounting move using the journal, receivable_account, revenue_account, tax and date you give; refused in words when one is missing, when the invoice is not posted, has no amount or already has a move. dry_run shows the lines and writes nothing. An agent may only dry-run. The move stays a draft: posting needs accounting.approver and approval. UNVERIFIED against Swedish rules",
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

/// A planned move line.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub account_id: String,
    pub account: String,
    pub label: String,
    pub debit: i64,
    pub credit: i64,
    pub tax_id: Option<String>,
}

/// What the plan holds.
pub struct Planned {
    pub invoice_id: String,
    pub reference: String,
    pub party_id: String,
    pub journal_id: String,
    pub date: String,
    pub net: i64,
    pub vat: i64,
    pub lines: Vec<Line>,
    pub notes: Vec<String>,
}

/// Inputs as given.
#[derive(Default)]
pub struct Input {
    pub invoice: String,
    pub journal: String,
    pub receivable_account: String,
    pub revenue_account: String,
    pub tax: String,
    pub vat_account: String,
    pub date: String,
}

fn pick(
    rows: &[(String, Fields)],
    what: &str,
    key: &str,
    keys: &[&str],
) -> Result<(String, Fields), String> {
    if key.is_empty() {
        return Err(format!("{what} is required"));
    }
    let hits: Vec<_> = rows
        .iter()
        .filter(|(id, f)| id == key || keys.iter().any(|k| text(f, k) == key))
        .collect();
    match hits.as_slice() {
        [(id, f)] => Ok((id.clone(), f.clone())),
        [] => Err(format!(
            "no {what} {key:?} (give its id or {})",
            keys.join(" or ")
        )),
        _ => Err(format!("{what} {key:?} is ambiguous: give its id")),
    }
}

/// VAT of `net` at `rate_bp`, rounded half up, in integer minor units.
pub fn vat_of(net: i64, rate_bp: i64) -> Result<i64, String> {
    let v = (net as i128 * rate_bp as i128 + 5_000).div_euclid(10_000);
    i64::try_from(v).map_err(|_| "the VAT amount overflows".to_string())
}

/// Build the plan. Reads only; every refusal is in words.
pub fn plan(env: Env<'_>, p: &Principal, i: &Input) -> Result<Planned, String> {
    if !p.allows(GROUP) {
        return Err(format!(
            "booking an invoice needs the {GROUP} group (an agent needs it in its scopes)"
        ));
    }
    let b = env.store.backend();
    let list = |m: &str| -> Result<Vec<(String, Fields)>, String> {
        b.list(m).map_err(|e| {
            format!("cannot read {m}: {e} (are the invoicing module and the accounting package installed?)")
        })
    };
    let (iid, inv) = pick(&list("invoice")?, "invoice", &i.invoice, &["number"])?;
    let state = text(&inv, "state");
    if state != "posted" {
        return Err(format!(
            "the invoice is {state}; only a posted invoice (one with a number) is booked"
        ));
    }
    if list(MODEL)?
        .iter()
        .any(|(_, f)| text(f, "invoice_id") == iid)
    {
        return Err(
            "this invoice already has a move; one invoice gets one (nothing was written)".into(),
        );
    }
    if crate::sie::from_iso(&i.date).is_none() {
        return Err("date is required as YYYY-MM-DD (an invoice has no date of its own)".into());
    }
    let (jid, _) = pick(
        &list("accounting_journal")?,
        "journal",
        &i.journal,
        &["code", "name"],
    )?;
    let accounts = list("accounting_account")?;
    let (rec_id, rec) = pick(
        &accounts,
        "receivable_account",
        &i.receivable_account,
        &["code"],
    )?;
    let (rev_id, rev) = pick(&accounts, "revenue_account", &i.revenue_account, &["code"])?;
    let (tax_id, tax) = pick(&list("accounting_tax")?, "tax", &i.tax, &["name"])?;
    let rate = tax
        .get("rate_bp")
        .and_then(J::as_i64)
        .ok_or("the tax has no rate_bp")?;
    if rate < 0 {
        return Err("the tax has a negative rate_bp".into());
    }
    let vat_key = if i.vat_account.is_empty() {
        text(&tax, "tax_account_id")
    } else {
        i.vat_account.clone()
    };
    let vat_acc = if rate > 0 {
        if vat_key.is_empty() {
            return Err("the tax has no tax_account_id and no vat_account was given".into());
        }
        Some(pick(&accounts, "vat_account", &vat_key, &["code"])?)
    } else {
        None
    };

    let mut lines_in: Vec<(String, i64)> = Vec::new();
    for (_, f) in list("invoice_line")? {
        if text(&f, "invoice_id") != iid {
            continue;
        }
        let q = f.get("quantity").and_then(J::as_i64).unwrap_or(0);
        let u = f.get("unit_price_minor").and_then(J::as_i64).unwrap_or(0);
        let amt = match f.get("amount_minor").and_then(J::as_i64) {
            Some(a) => a,
            None => q.checked_mul(u).ok_or("a line amount overflows")?,
        };
        lines_in.push((text(&f, "description"), amt));
    }
    if lines_in.iter().any(|(_, v)| *v < 0) {
        return Err(
            "an invoice line has a negative amount; credit notes are not booked here".into(),
        );
    }
    let net = lines_in
        .iter()
        .try_fold(0i64, |a, (_, v)| a.checked_add(*v))
        .ok_or("the net total overflows")?;
    if net <= 0 {
        return Err("the invoice has no positive amount; nothing to book".into());
    }
    let vat = vat_of(net, rate)?;
    let gross = net.checked_add(vat).ok_or("the total overflows")?;
    let number = text(&inv, "number");
    let reference = if number.is_empty() {
        iid.clone()
    } else {
        number
    };
    let mut lines = vec![Line {
        account_id: rec_id,
        account: text(&rec, "code"),
        label: format!("Invoice {reference}"),
        debit: gross,
        credit: 0,
        tax_id: None,
    }];
    for (d, v) in lines_in.iter().filter(|(_, v)| *v != 0) {
        lines.push(Line {
            account_id: rev_id.clone(),
            account: text(&rev, "code"),
            label: if d.is_empty() {
                format!("Invoice {reference}")
            } else {
                d.clone()
            },
            debit: 0,
            credit: *v,
            tax_id: Some(tax_id.clone()),
        });
    }
    if let Some((vid, vf)) = vat_acc {
        lines.push(Line {
            account_id: vid,
            account: text(&vf, "code"),
            label: format!("VAT {reference}"),
            debit: 0,
            credit: vat,
            tax_id: Some(tax_id.clone()),
        });
    }
    let mut notes = vec!["UNVERIFIED: nothing is checked against Swedish invoice rules or the VAT act; the move is a draft and posting needs accounting.approver".into()];
    let cur = text(&inv, "currency");
    if !cur.is_empty() {
        notes.push(format!(
            "the invoice currency {cur} is not converted or compared with the ledger currency"
        ));
    }
    let coded = list("accounting_se_vat_code")
        .map(|v| v.iter().any(|(_, f)| text(f, "tax_id") == tax_id))
        .unwrap_or(false);
    if !coded {
        notes.push("no accounting_se_vat_code row has this tax_id, so vat_computation.compute will not count these lines".into());
    }
    Ok(Planned {
        invoice_id: iid,
        reference,
        party_id: text(&inv, "party_id"),
        journal_id: jid,
        date: i.date.clone(),
        net,
        vat,
        lines,
        notes,
    })
}

/// Plan, and unless `dry_run` create the draft move and its lines as the caller, then the link.
/// Returns the plan and the move id.
pub fn create_move(
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
        "narration": format!("Customer invoice {}", pl.reference)});
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
            let mut r = json!({"move_id": mid, "account_id": l.account_id, "label": l.label,
                "debit": l.debit, "credit": l.credit});
            if let Some(t) = &l.tax_id {
                r["tax_id"] = json!(t);
            }
            r
        })
        .collect();
    if let Err(e) = crate::sie::bulk(env, p, "accounting_move_line", rows, None, false) {
        // A move without its lines would be an empty draft; take it back.
        let _ = call(env, p, "accounting_move.cancel", json!({"id": mid}));
        return Err(format!(
            "the move lines were refused, the draft move {mid} was cancelled: {e}"
        ));
    }
    env.store.insert(
        MODEL,
        [
            ("invoice_id".to_string(), Value::text(&pl.invoice_id)),
            ("move_id".to_string(), Value::text(&mid)),
        ]
        .into(),
    )?;
    Ok((pl, Some(mid)))
}

pub fn register(d: &Arc<Dispatcher>, store: &Store) {
    let m = module();
    let (weak, st): (Weak<Dispatcher>, Store) = (Arc::downgrade(d), store.clone());
    d.register_with_actor(
        &m,
        MODEL,
        "create_move",
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
                return Err("an agent may only dry-run the invoice booking (dry_run=true); ask the person to run it".into());
            }
            let groups: Vec<&str> = a.groups.iter().map(String::as_str).collect();
            let p = Principal::user(&a.caller, &groups);
            let d = weak.upgrade().ok_or("the dispatcher is gone")?;
            let inp = Input {
                invoice: s("invoice"),
                journal: s("journal"),
                receivable_account: s("receivable_account"),
                revenue_account: s("revenue_account"),
                tax: s("tax"),
                vat_account: s("vat_account"),
                date: s("date"),
            };
            let (pl, mid) = create_move(Env { d: &d, store: &st }, &p, &inp, dry)?;
            let lines = pl
                .lines
                .iter()
                .map(|l| {
                    Value::map([
                        ("account", Value::text(&l.account)),
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
                ("vat", Value::Int(pl.vat)),
                ("gross", Value::Int(pl.net + pl.vat)),
                ("lines", Value::List(lines)),
                (
                    "notes",
                    Value::List(pl.notes.iter().map(|s| Value::text(s)).collect()),
                ),
            ];
            if let Some(m) = mid {
                out.push(("move_id", Value::text(&m)));
            }
            Ok(Value::Map(out.into_iter().map(|(k, v)| (k.to_string(), v)).collect()))
        }),
    );
}
