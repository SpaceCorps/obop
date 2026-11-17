//! A posted customer invoice booked as a DRAFT accounting move (docs/spec/accounting-se.md
//! section 2, stage 5), automatically when the invoice is posted (owner ruling of 2026-11-17) or
//! by the typed command `invoice_posting.create_move`.
//!
//! The `invoicing` module has no account, tax or VAT on its lines, so everything the voucher needs
//! comes from the company's booking settings (`invoice_posting_setting`, kept in the company
//! settings row under `invoicing`) or from the caller; no BAS account, rate or code is built in (D34)
//! and nothing is checked against Swedish invoice rules or the VAT act.
//!
//! The move has: one receivable debit for net plus VAT (with the invoice's party and due date);
//! one revenue credit per invoice line on `revenue_account`, carrying the `tax` as `tax_id` (so
//! `vat_computation.compute` finds the base); one VAT credit on the tax's `tax_account_id` (or
//! `vat_account`), carrying the same `tax_id`. The invoice total (the sum of its lines, which
//! carry no tax) is the net, and VAT is the net times the tax's `rate_bp`, rounded half up once
//! on the total, in integer minor units. With a tax whose `price_included` is set the total is
//! the gross instead: the receivable is the total, the VAT is total x rate / (10000 + rate),
//! rounded half up, and the revenue credits share the rest in proportion to the lines. Only a
//! sales tax may be the default of the settings, and a purchase tax is refused. The move stays a draft: posting it
//! is `accounting_move.post`, which needs `accounting.approver` and a person's approval.
//!
//! Booking on post: while `book_on_post` is on (the default) and the settings name a journal, a
//! receivable account, a revenue account and a tax, a guard of `invoice.post` (and of an
//! `invoice.update` that posts) plans the booking before the invoice is posted (a booking that
//! would be refused, such as one dated in a closed period, refuses the post and nothing changes)
//! and an observer writes it right after the post, in the same call. The post is not wrapped, so
//! an update may still post an invoice together with an edit (D48). Without those
//! settings, or with `book_on_post` off, the invoice is posted and not booked, as before; it can
//! be booked later with `create_move`, which also takes the settings as its defaults.
//!
//! The post is planned with the issue date the call gives (an `update` that posts and changes
//! the date is checked on the new date). Only a call that moves the invoice from draft to posted
//! books it: an `update` that repeats `state = posted` on an invoice posted earlier books nothing.
//!
//! The move, its lines and the link row (`invoice_posting`, id = the invoice id) are written in
//! one batch of the store (with the entry's date as the invoice's issue date when it had none), so an invoice gets one move even when two runs race. `get` reads the
//! link of an invoice. An agent may only dry-run `create_move`.
//!
//! That batch is written straight to the store, not through `accounting_move.create`: the move's
//! own checks are made here instead (the journal exists and is active, every account exists and
//! is not deprecated, the date is not in a closed period, the move balances). What the generated
//! commands would add and this does not: the move's `required` and reference checks beyond those,
//! its guard, and an audit row of `accounting_move.create` (the call that posted the invoice is
//! the record of it).

use crate::company_settings;
use crate::sie::Env;
use obop_commands::{Actor, Dispatcher, Principal, Store, Value};
use obop_registry::{CommandDef, FieldDef, FieldKind, ModelDef, Module};
use obop_storage::{Backend, Fields, Op};
use obop_types::DataClass;
use serde_json::{Map, Value as J};
use std::sync::{Arc, Weak};

const MODEL: &str = "invoice_posting";
const SETTINGS: &str = "invoice_posting_setting";
const GROUP: &str = "accounting.user";
const MANAGER: &str = "accounting.manager";
/// The section of the instance settings that holds the booking settings.
const SECTION: &str = "invoicing";
/// The settings that name a record (an id, or a code or name).
const KEYS: [&str; 5] = [
    "journal",
    "receivable_account",
    "revenue_account",
    "tax",
    "vat_account",
];

pub fn module() -> Module {
    let f = |name, description| FieldDef::new(name, description, DataClass::None, false, GROUP);
    Module {
        id: "se_invoice_posting".into(),
        name: "Invoice to ledger".into(),
        description: "Books a posted customer invoice as a draft accounting move".into(),
        models: vec![
            ModelDef {
                name: MODEL,
                description: "A posted `invoice` booked as a DRAFT `accounting_move` with a receivable debit, revenue credits and a VAT credit: automatically when the invoice is posted (see invoice_posting_setting), or by `create_move`. Rows of this model are the link from an invoice to its move (one each); `get` reads it. Needs the accounting package. UNVERIFIED: nothing is checked against Swedish invoice or VAT rules",
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
                        "Input: the id, code or name of the accounting journal; default from the booking settings",
                    ),
                    f(
                        "receivable_account",
                        "Input: the id or code of the receivable account (debit); default from the booking settings",
                    ),
                    f(
                        "revenue_account",
                        "Input: the id or code of the revenue account (credit); default from the booking settings",
                    ),
                    f(
                        "tax",
                        "Input: the id or name of the accounting tax; its rate_bp gives the VAT and it is the tax_id of the base and VAT lines; default from the booking settings",
                    ),
                    f(
                        "vat_account",
                        "Input, optional: the id or code of the VAT account; default is the booking settings, else the tax's tax_account_id",
                    ),
                    f(
                        "date",
                        "Input: the accounting date, YYYY-MM-DD; default is the invoice's issue date, else today in the platform zone",
                    ),
                    f("dry_run", "Input: only show the lines, write nothing").kind(FieldKind::Bool),
                    f("invoice_id", "Link: the invoice id"),
                    f("move_id", "Link and answer: the draft move id"),
                    f("booked", "Answer of get: whether the invoice has a journal entry")
                        .kind(FieldKind::Bool),
                    f("move_state", "Answer of get: the state of the move"),
                    f("move_number", "Answer of get: the number of the move once it is posted"),
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
                commands: vec![
                    CommandDef::new(
                        "create_move",
                        "Book a posted invoice (invoice) as a draft accounting move using the journal, receivable_account, revenue_account, tax and date you give, or the booking settings and the invoice's issue date (else today) where you give none; refused in words when one is missing, when the invoice is not posted, has no amount or already has a move. dry_run shows the lines and writes nothing. An agent may only dry-run. The move stays a draft: posting needs accounting.approver and approval. UNVERIFIED against Swedish rules",
                        GROUP,
                    ),
                    CommandDef::new(
                        "get",
                        "The journal entry of an invoice (invoice: its id or number): booked, and when it is booked move_id, move_state and move_number",
                        GROUP,
                    ),
                ],
                ext: Default::default(),
            },
            ModelDef {
                name: SETTINGS,
                description: "The company's booking settings of customer invoices: whether a posted invoice is booked automatically (book_on_post, on by default) and the journal, receivable account, revenue account, tax and optional VAT account it is booked with. Kept in the company settings row under `invoicing`. One record, `company`; `update` needs accounting.manager",
                read_group: GROUP,
                write_group: MANAGER,
                access: None,
                fields: vec![
                    f(
                        "book_on_post",
                        "Whether posting an invoice books its journal entry (a draft) right away; on unless it was turned off",
                    )
                    .kind(FieldKind::Bool),
                    f("journal", "The journal (id, code or name) invoices are booked in"),
                    f("receivable_account", "The receivable account (id or code), debited with the invoice total plus VAT, or the total itself with a price-included tax"),
                    f("revenue_account", "The revenue account (id or code), credited with the net of each line"),
                    f("tax", "The sales tax (id or name) of the invoice lines; with price_included the invoice total is taken as the gross"),
                    f("vat_account", "Optional: the VAT account (id or code); default is the tax's tax_account_id"),
                    f("ready", "Answer: whether posting an invoice books it now").kind(FieldKind::Bool),
                    f("missing", "Answer: the settings that are still needed (a list)"),
                ],
                commands: vec![
                    CommandDef::new("get", "The company's booking settings of customer invoices", GROUP),
                    CommandDef::new(
                        "update",
                        "Change the booking settings: book_on_post (true or false), journal, receivable_account, revenue_account, tax, vat_account (an empty text clears one). Each named record must exist",
                        MANAGER,
                    ),
                ],
                ext: Default::default(),
            },
        ],
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
    pub due_on: Option<String>,
}

/// What the plan holds.
pub struct Planned {
    pub invoice_id: String,
    pub reference: String,
    pub party_id: String,
    pub journal_id: String,
    pub date: String,
    /// The invoice has no issue date: the booking writes `date` as its issue date, so the
    /// invoice and its entry carry one date.
    pub stamp_issue_date: bool,
    pub net: i64,
    pub vat: i64,
    pub lines: Vec<Line>,
    pub notes: Vec<String>,
}

/// Inputs as given.
#[derive(Default, Clone)]
pub struct Input {
    pub invoice: String,
    pub journal: String,
    pub receivable_account: String,
    pub revenue_account: String,
    pub tax: String,
    pub vat_account: String,
    pub date: String,
}

impl Input {
    fn slot(&mut self, k: &str) -> Option<&mut String> {
        Some(match k {
            "journal" => &mut self.journal,
            "receivable_account" => &mut self.receivable_account,
            "revenue_account" => &mut self.revenue_account,
            "tax" => &mut self.tax,
            "vat_account" => &mut self.vat_account,
            _ => return None,
        })
    }

    /// Fill what the caller left empty from the booking settings.
    fn with_defaults(mut self, s: &Map<String, J>) -> Self {
        for k in KEYS {
            let v = s.get(k).and_then(J::as_str).unwrap_or_default().to_string();
            if let Some(slot) = self.slot(k)
                && slot.is_empty()
            {
                *slot = v;
            }
        }
        self
    }
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

/// The VAT inside a price-included `gross` at `rate_bp`: gross x rate / (10000 + rate), rounded
/// half up, in integer minor units.
pub fn vat_within(gross: i64, rate_bp: i64) -> Result<i64, String> {
    let den = 2 * (10_000 + rate_bp as i128);
    let v = (2 * gross as i128 * rate_bp as i128 + den / 2).div_euclid(den);
    i64::try_from(v).map_err(|_| "the VAT amount overflows".to_string())
}

/// The revenue credit of each line: its amount, or with a price-included tax its part of `net`
/// in proportion to its amount (the last line with an amount takes the rounding).
fn share(lines: &[(String, i64)], total: i64, net: i64) -> Vec<i64> {
    if net == total {
        return lines.iter().map(|(_, v)| *v).collect();
    }
    let last = lines.iter().rposition(|(_, v)| *v != 0);
    let mut left = net;
    lines
        .iter()
        .enumerate()
        .map(|(n, (_, v))| {
            let part = if Some(n) == last {
                left
            } else {
                (*v as i128 * net as i128 / total as i128) as i64
            };
            left -= part;
            part
        })
        .collect()
}

fn list(b: &dyn Backend, m: &str) -> Result<Vec<(String, Fields)>, String> {
    b.list(m).map_err(|e| {
        format!(
            "cannot read {m}: {e} (are the invoicing module and the accounting package installed?)"
        )
    })
}

/// The id of the invoice `key` (an id or a number).
fn invoice_of(b: &dyn Backend, key: &str) -> Result<(String, Fields), String> {
    pick(&list(b, "invoice")?, "invoice", key, &["number"])
}

/// The link row of an invoice, if it has one.
fn link_of(b: &dyn Backend, iid: &str) -> Result<Option<(String, Fields)>, String> {
    Ok(list(b, MODEL)?
        .into_iter()
        .find(|(_, f)| text(f, "invoice_id") == iid))
}

/// Build the plan. Reads only; every refusal is in words. `posted` is the state the invoice must
/// be in (`draft` while the post that books it is being planned).
fn plan_in(b: &dyn Backend, i: &Input, state_needed: &str) -> Result<Planned, String> {
    let (iid, inv) = invoice_of(b, &i.invoice)?;
    let state = text(&inv, "state");
    if state != state_needed {
        return Err(format!(
            "the invoice is {state}; only a posted invoice (one with a number) is booked"
        ));
    }
    if link_of(b, &iid)?.is_some() {
        return Err(
            "this invoice already has a move; one invoice gets one (nothing was written)".into(),
        );
    }
    let date = if i.date.is_empty() {
        text(&inv, "issue_date")
    } else {
        i.date.clone()
    };
    if crate::sie::from_iso(&date).is_none() {
        return Err(
            "date is required as YYYY-MM-DD (the invoice has no issue date to book it on)".into(),
        );
    }
    let (jid, journal) = pick(
        &list(b, "accounting_journal")?,
        "journal",
        &i.journal,
        &["code", "name"],
    )?;
    if journal.get("active") == Some(&J::Bool(false)) {
        return Err(format!(
            "the journal {} is inactive; choose an active journal",
            text(&journal, "code")
        ));
    }
    let accounts = list(b, "accounting_account")?;
    let (rec_id, rec) = pick(
        &accounts,
        "receivable_account",
        &i.receivable_account,
        &["code"],
    )?;
    let (rev_id, rev) = pick(&accounts, "revenue_account", &i.revenue_account, &["code"])?;
    let (tax_id, tax) = pick(&list(b, "accounting_tax")?, "tax", &i.tax, &["name"])?;
    let rate = tax
        .get("rate_bp")
        .and_then(J::as_i64)
        .ok_or("the tax has no rate_bp")?;
    if rate < 0 {
        return Err("the tax has a negative rate_bp".into());
    }
    if text(&tax, "scope") == "purchases" {
        return Err(format!(
            "the tax {} is a purchase tax; a customer invoice is booked with a sales tax",
            text(&tax, "name")
        ));
    }
    let included = tax.get("price_included") == Some(&J::Bool(true));
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
    let mut used = vec![&rec, &rev];
    if let Some((_, vf)) = &vat_acc {
        used.push(vf);
    }
    for a in used {
        if a.get("deprecated") == Some(&J::Bool(true)) {
            return Err(format!(
                "the account {} is closed to new postings",
                text(a, "code")
            ));
        }
    }
    // An entry dated in a closed period cannot be made (the lock date of accounting_move).
    if let Some(name) = closed_period(b, &date) {
        return Err(format!(
            "the date {date} is in the closed period {name}; an entry cannot be made there"
        ));
    }

    let mut lines_in: Vec<(String, i64)> = Vec::new();
    for (_, f) in list(b, "invoice_line")? {
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
    let total = lines_in
        .iter()
        .try_fold(0i64, |a, (_, v)| a.checked_add(*v))
        .ok_or("the net total overflows")?;
    if total <= 0 {
        return Err("the invoice has no positive amount; nothing to book".into());
    }
    // A price-included tax: the invoice total is the gross amount and the VAT is taken out of it;
    // otherwise the total is net and the VAT comes on top.
    let (net, vat, gross) = if included {
        let vat = vat_within(total, rate)?;
        (total - vat, vat, total)
    } else {
        let vat = vat_of(total, rate)?;
        (
            total,
            vat,
            total.checked_add(vat).ok_or("the total overflows")?,
        )
    };
    let credits = share(&lines_in, total, net);
    let number = text(&inv, "number");
    let reference = if number.is_empty() {
        iid.clone()
    } else {
        number
    };
    let due = text(&inv, "due_date");
    let mut lines = vec![Line {
        account_id: rec_id,
        account: text(&rec, "code"),
        label: format!("Invoice {reference}"),
        debit: gross,
        credit: 0,
        tax_id: None,
        due_on: (!due.is_empty()).then_some(due),
    }];
    for ((d, _), v) in lines_in.iter().zip(credits).filter(|(_, v)| *v != 0) {
        lines.push(Line {
            account_id: rev_id.clone(),
            account: text(&rev, "code"),
            label: if d.is_empty() {
                format!("Invoice {reference}")
            } else {
                d.clone()
            },
            debit: 0,
            credit: v,
            tax_id: Some(tax_id.clone()),
            due_on: None,
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
            due_on: None,
        });
    }
    let mut notes = vec!["UNVERIFIED: nothing is checked against Swedish invoice rules or the VAT act; the move is a draft and posting needs accounting.approver".into()];
    let cur = text(&inv, "currency");
    if !cur.is_empty() {
        notes.push(format!(
            "the invoice currency {cur} is not converted or compared with the ledger currency"
        ));
    }
    let coded = b
        .list("accounting_se_vat_code")
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
        stamp_issue_date: text(&inv, "issue_date").is_empty(),
        date,
        net,
        vat,
        lines,
        notes,
    })
}

/// The name of the closed accounting period that holds `date`, if any.
fn closed_period(b: &dyn Backend, date: &str) -> Option<String> {
    let periods = b.list("accounting_period").ok()?;
    periods
        .iter()
        .find(|(_, p)| {
            text(p, "state") == "closed"
                && text(p, "start_on").as_str() <= date
                && date <= text(p, "end_on").as_str()
        })
        .map(|(_, p)| text(p, "name"))
}

/// The refusal of a post whose journal entry cannot be booked, in the language of the request
/// (machine-drafted translations). A closed period is named with its fix (another issue date);
/// any other cause keeps its English detail and points to the booking settings.
pub fn post_refusal(lang: Option<&str>, closed: Option<(&str, &str)>, why: &str) -> String {
    if let Some((date, period)) = closed {
        return match lang {
            Some("de") => format!(
                "Die Rechnung wurde nicht gebucht: ihr Rechnungsdatum {date} liegt in der abgeschlossenen Periode {period}. Wählen Sie ein Rechnungsdatum in einer offenen Periode."
            ),
            Some("fr") => format!(
                "La facture n'a pas été validée : sa date {date} se trouve dans la période clôturée {period}. Choisissez une date de facture dans une période ouverte."
            ),
            Some("pl") => format!(
                "Faktura nie została zatwierdzona: jej data {date} przypada w zamkniętym okresie {period}. Wybierz datę wystawienia w otwartym okresie."
            ),
            Some("fi") => format!(
                "Laskua ei kirjattu: sen päivä {date} on suljetulla kaudella {period}. Valitse laskun päiväksi avoimen kauden päivä."
            ),
            Some("sv") => format!(
                "Fakturan bokfördes inte: dess fakturadatum {date} ligger i den stängda perioden {period}. Välj ett fakturadatum i en öppen period."
            ),
            Some("es") => format!(
                "La factura no se ha contabilizado: su fecha {date} está en el período cerrado {period}. Elija una fecha de factura en un período abierto."
            ),
            _ => format!(
                "The invoice was not posted: its issue date {date} is in the closed period {period}. Choose an issue date in an open period."
            ),
        };
    }
    match lang {
        Some("de") => format!(
            "Die Rechnung wurde nicht gebucht, weil ihr Buchungssatz nicht erstellt werden kann: {why}. Korrigieren Sie die Buchungseinstellungen der Rechnungen oder schalten Sie die automatische Buchung aus."
        ),
        Some("fr") => format!(
            "La facture n'a pas été validée, car son écriture ne peut pas être passée : {why}. Corrigez les paramètres de comptabilisation des factures ou désactivez la comptabilisation automatique."
        ),
        Some("pl") => format!(
            "Faktura nie została zatwierdzona, ponieważ nie można zaksięgować jej zapisu: {why}. Popraw ustawienia księgowania faktur albo wyłącz automatyczne księgowanie."
        ),
        Some("fi") => format!(
            "Laskua ei kirjattu, koska sen vientiä ei voi tehdä: {why}. Korjaa laskujen kirjausasetukset tai poista automaattinen kirjaus käytöstä."
        ),
        Some("sv") => format!(
            "Fakturan bokfördes inte, eftersom dess verifikation inte kan skapas: {why}. Rätta bokföringsinställningarna för fakturor eller stäng av den automatiska bokföringen."
        ),
        Some("es") => format!(
            "La factura no se ha contabilizado porque no se puede crear su asiento: {why}. Corrija los ajustes de contabilización de facturas o desactive la contabilización automática."
        ),
        _ => format!(
            "The invoice was not posted, because its journal entry cannot be booked: {why}. Fix the booking settings (invoice_posting_setting) or turn book_on_post off."
        ),
    }
}

/// The plan of `create_move` for a caller: needs the group; defaults from the booking settings.
pub fn plan(env: Env<'_>, p: &Principal, i: &Input) -> Result<Planned, String> {
    if !p.allows(GROUP) {
        return Err(format!(
            "booking an invoice needs the {GROUP} group (an agent needs it in its scopes)"
        ));
    }
    let b = env.store.backend();
    let mut i = i
        .clone()
        .with_defaults(&company_settings::section(&*b, SECTION)?);
    // No date given and none on the invoice: today in the platform zone, as booking on post does.
    if i.date.is_empty()
        && invoice_of(&*b, &i.invoice).is_ok_and(|(_, f)| text(&f, "issue_date").is_empty())
    {
        i.date = today(env.d);
    }
    plan_in(&*b, &i, "posted")
}

/// Write the move, its lines and the link in one batch. The link's id is the invoice id, so a
/// second move for the same invoice is refused by the store.
fn write(b: &dyn Backend, pl: &Planned) -> Result<String, String> {
    let mid = new_id();
    let mut mv = Fields::new();
    for (k, v) in [
        ("journal_id", &pl.journal_id),
        ("date", &pl.date),
        ("reference", &pl.reference),
    ] {
        mv.insert(k.into(), J::from(v.as_str()));
    }
    mv.insert(
        "narration".into(),
        J::from(format!("Customer invoice {}", pl.reference)),
    );
    mv.insert("state".into(), J::from("draft"));
    if !pl.party_id.is_empty() {
        mv.insert("partner_id".into(), J::from(pl.party_id.as_str()));
    }
    let mut ops = vec![Op::Insert {
        model: "accounting_move".into(),
        id: mid.clone(),
        fields: mv,
    }];
    for l in &pl.lines {
        let mut f = Fields::new();
        f.insert("move_id".into(), J::from(mid.as_str()));
        f.insert("account_id".into(), J::from(l.account_id.as_str()));
        f.insert("label".into(), J::from(l.label.as_str()));
        f.insert("debit".into(), J::from(l.debit));
        f.insert("credit".into(), J::from(l.credit));
        // Stated, so the filters "Open items" and "Overdue" (reconciled = false) find the item.
        f.insert("reconciled".into(), J::Bool(false));
        if !pl.party_id.is_empty() {
            f.insert("partner_id".into(), J::from(pl.party_id.as_str()));
        }
        if let Some(t) = &l.tax_id {
            f.insert("tax_id".into(), J::from(t.as_str()));
        }
        if let Some(d) = &l.due_on {
            f.insert("due_on".into(), J::from(d.as_str()));
        }
        ops.push(Op::Insert {
            model: "accounting_move_line".into(),
            id: new_id(),
            fields: f,
        });
    }
    let mut link = Fields::new();
    link.insert("invoice_id".into(), J::from(pl.invoice_id.as_str()));
    link.insert("move_id".into(), J::from(mid.as_str()));
    ops.push(Op::Insert {
        model: MODEL.into(),
        id: pl.invoice_id.clone(),
        fields: link,
    });
    if pl.stamp_issue_date {
        let mut f = Fields::new();
        f.insert("issue_date".into(), J::from(pl.date.as_str()));
        ops.push(Op::Merge {
            model: "invoice".into(),
            id: pl.invoice_id.clone(),
            patch: f,
        });
    }
    b.apply(&ops).map_err(|e| {
        if link_of(b, &pl.invoice_id).ok().flatten().is_some() {
            "this invoice already has a move; one invoice gets one (nothing was written)".into()
        } else {
            format!("the move could not be written (nothing was written): {e}")
        }
    })?;
    Ok(mid)
}

fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// Plan, and unless `dry_run` write the draft move, its lines and the link in one batch.
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
    let mid = write(&*env.store.backend(), &pl)?;
    Ok((pl, Some(mid)))
}

/// The booking settings with what is still missing for booking on post.
fn settings_row(b: &dyn Backend) -> Result<Value, String> {
    let s = company_settings::section(b, SECTION)?;
    let on = s.get("book_on_post").and_then(J::as_bool).unwrap_or(true);
    let t = |k: &str| s.get(k).and_then(J::as_str).unwrap_or_default().to_string();
    let missing: Vec<Value> = KEYS[..4]
        .iter()
        .filter(|k| t(k).is_empty())
        .map(|k| Value::text(k))
        .collect();
    let mut row = vec![
        ("id", Value::text("company")),
        ("book_on_post", Value::Bool(on)),
        ("ready", Value::Bool(on && missing.is_empty())),
        ("missing", Value::List(missing)),
    ];
    for k in KEYS {
        row.push((k, Value::text(&t(k))));
    }
    Ok(Value::Map(
        row.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
    ))
}

/// The settings to book with on post: `None` when booking on post is off or not set up.
fn on_post_input(b: &dyn Backend) -> Result<Option<Input>, String> {
    let s = company_settings::section(b, SECTION)?;
    if !s.get("book_on_post").and_then(J::as_bool).unwrap_or(true) {
        return Ok(None);
    }
    let i = Input::default().with_defaults(&s);
    let set = !i.journal.is_empty()
        && !i.receivable_account.is_empty()
        && !i.revenue_account.is_empty()
        && !i.tax.is_empty();
    Ok(set.then_some(i))
}

fn update_settings(
    b: &dyn Backend,
    x: &std::collections::BTreeMap<String, Value>,
) -> Result<Value, String> {
    let mut patch = Map::new();
    match x.get("book_on_post") {
        Some(Value::Bool(v)) => {
            patch.insert("book_on_post".into(), J::Bool(*v));
        }
        Some(Value::Text(t)) if t == "true" || t == "false" => {
            patch.insert("book_on_post".into(), J::Bool(t == "true"));
        }
        Some(Value::Null) | None => {}
        Some(_) => return Err("book_on_post must be true or false".into()),
    }
    for (k, model, keys) in [
        ("journal", "accounting_journal", &["code", "name"][..]),
        ("receivable_account", "accounting_account", &["code"][..]),
        ("revenue_account", "accounting_account", &["code"][..]),
        ("tax", "accounting_tax", &["name"][..]),
        ("vat_account", "accounting_account", &["code"][..]),
    ] {
        match x.get(k) {
            Some(Value::Text(t)) if t.is_empty() => {
                patch.insert(k.into(), J::Null);
            }
            Some(Value::Text(t)) => {
                // Stored as the id, so a renamed code does not break the booking.
                let (id, rec) = pick(&list(b, model)?, k, t, keys)?;
                if k == "tax" && text(&rec, "scope") != "sales" {
                    return Err(format!(
                        "the tax {t:?} is not a sales tax (its scope is {:?}); customer invoices are booked with a sales tax",
                        text(&rec, "scope")
                    ));
                }
                patch.insert(k.into(), J::from(id));
            }
            Some(Value::Null) | None => {}
            Some(_) => return Err(format!("{k} must be a text: an id, code or name")),
        }
    }
    if !patch.is_empty() {
        company_settings::update(b, SECTION, &patch)?;
    }
    settings_row(b)
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
    let st = store.clone();
    d.register(
        &m,
        MODEL,
        "get",
        Box::new(move |input| {
            let key = match input {
                Value::Map(x) => match x.get("invoice").or_else(|| x.get("id")) {
                    Some(Value::Text(t)) => t.clone(),
                    _ => String::new(),
                },
                _ => String::new(),
            };
            let b = st.backend();
            let (iid, _) = invoice_of(&*b, &key)?;
            let Some((_, link)) = link_of(&*b, &iid)? else {
                return Ok(Value::map([
                    ("id", Value::text(&iid)),
                    ("invoice_id", Value::text(&iid)),
                    ("booked", Value::Bool(false)),
                ]));
            };
            let mid = text(&link, "move_id");
            let mv = b.get("accounting_move", &mid).map_err(|e| e.to_string())?;
            let field = |k: &str| mv.as_ref().map(|f| text(f, k)).unwrap_or_default();
            Ok(Value::map([
                ("id", Value::text(&iid)),
                ("invoice_id", Value::text(&iid)),
                ("booked", Value::Bool(true)),
                ("move_id", Value::text(&mid)),
                ("move_state", Value::text(&field("state"))),
                ("move_number", Value::text(&field("number"))),
            ]))
        }),
    );
    let st = store.clone();
    d.register(
        &m,
        SETTINGS,
        "get",
        Box::new(move |_| settings_row(&*st.backend())),
    );
    let st = store.clone();
    d.register(
        &m,
        SETTINGS,
        "update",
        Box::new(move |input| {
            let Value::Map(x) = input else {
                return Err("expected an object".into());
            };
            update_settings(&*st.backend(), x)
        }),
    );
    // Booking on post (owner ruling of 2026-11-17). Not a wrapper of `invoice.post`: an action
    // with a wrapper cannot be moved by an `update` together with an edit (D48), which posting an
    // invoice can. A guard plans the booking before the post (on the action and on an update
    // that moves the state to posted), so a booking that would be refused refuses the post; an
    // observer writes it after the post succeeded, in the same call.
    // The invoices a call is moving from draft to posted, marked by the guard and taken by the
    // observer: an update that repeats `state = posted` on a posted invoice books nothing.
    let posting: Arc<std::sync::Mutex<std::collections::HashSet<String>>> = Arc::default();
    let (st, dw, marks) = (store.clone(), Arc::downgrade(d), posting.clone());
    let check = move |input: &Value| -> Result<obop_commands::Verdict, String> {
        let b = st.backend();
        let Some(d) = dw.upgrade() else {
            return Ok(obop_commands::Verdict::default());
        };
        if let Value::Map(x) = input
            && let Some(Value::Text(id)) = x.get("id")
        {
            let draft = b
                .get("invoice", id)
                .map_err(|e| e.to_string())?
                .is_some_and(|f| text(&f, "state") == "draft");
            let mut m = marks.lock().unwrap_or_else(|e| e.into_inner());
            if draft {
                m.insert(id.to_string());
            } else {
                m.remove(id.as_str());
            }
        }
        if let Some(i) = booking_for(&d, &*b, input)?
            && invoice_of(&*b, &i.invoice)
                .map(|(_, f)| text(&f, "state") == "draft")
                .unwrap_or(false)
            && link_of(&*b, &i.invoice)?.is_none()
        {
            plan_in(&*b, &i, "draft").map_err(|e| {
                let lang = obop_commands::locale::language();
                match closed_period(&*b, &i.date) {
                    Some(name) => post_refusal(lang, Some((&i.date, &name)), &e),
                    None => post_refusal(lang, None, &e),
                }
            })?;
        }
        Ok(obop_commands::Verdict::default())
    };
    let check = Arc::new(check);
    let c = check.clone();
    d.add_guard(
        "invoicing",
        "invoice.post",
        true,
        Box::new(move |_, input| c(input)),
    );
    d.add_guard(
        "invoicing",
        "invoice.update",
        true,
        Box::new(move |_, input| match input {
            Value::Map(x) if x.get("state") == Some(&Value::text("posted")) => check(input),
            _ => Ok(obop_commands::Verdict::default()),
        }),
    );
    let (st, dw) = (store.clone(), Arc::downgrade(d));
    d.observe(
        &m.id,
        Box::new(move |id, _, input, _| {
            if id != "invoice.post" && id != "invoice.update" {
                return;
            }
            // Only a call the guard saw take a draft invoice to posted.
            let Value::Map(x) = input else { return };
            let Some(Value::Text(iid)) = x.get("id") else {
                return;
            };
            let moved = posting
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(iid.as_str());
            // (A stale mark of a call that failed is harmless: the invoice is then still a draft
            // and `book_after_post` books only a posted one.)
            if !moved {
                return;
            }
            let Some(d) = dw.upgrade() else { return };
            if let Err(e) = book_after_post(&d, &*st.backend(), input) {
                eprintln!(
                    "the invoice was posted but its journal entry could not be booked: {e}; book it with invoice_posting.create_move"
                );
            }
        }),
    );
}

/// The booking of the invoice an input names, when booking on post is on and set up and the
/// accounting package is installed. It is dated on the issue date the input gives (an update that
/// posts and changes the date), else the stored issue date, else today in the platform zone
/// (`OBOP_ZONE`).
fn booking_for(d: &Dispatcher, b: &dyn Backend, input: &Value) -> Result<Option<Input>, String> {
    let id = match input {
        Value::Map(x) => match x.get("id") {
            Some(Value::Text(t)) => t.to_string(),
            _ => return Ok(None),
        },
        _ => return Ok(None),
    };
    if !d.has_model("accounting_move") {
        return Ok(None);
    }
    let Some(i) = on_post_input(b)? else {
        return Ok(None);
    };
    let given = match input {
        Value::Map(x) => match x.get("issue_date") {
            Some(Value::Text(t)) => Some(t.to_string()),
            Some(Value::Null) => Some(String::new()),
            _ => None,
        },
        _ => None,
    };
    let issued = match given {
        Some(t) => t,
        None => b
            .get("invoice", &id)
            .map_err(|e| e.to_string())?
            .map(|f| text(&f, "issue_date"))
            .unwrap_or_default(),
    };
    let date = if issued.is_empty() { today(d) } else { issued };
    Ok(Some(Input {
        invoice: id,
        date,
        ..i
    }))
}

/// Today in the platform zone, `YYYY-MM-DD`.
fn today(d: &Dispatcher) -> String {
    let micros = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |t| t.as_micros() as i64);
    obop_values::Instant::from_micros(micros)
        .map(|at| d.zones().today(at))
        .unwrap_or_else(|| crate::sie::iso(crate::sie::today()))
}

/// After a post (or an update that posted): book the invoice if it is now posted and has no move.
fn book_after_post(d: &Dispatcher, b: &dyn Backend, input: &Value) -> Result<(), String> {
    let Some(i) = booking_for(d, b, input)? else {
        return Ok(());
    };
    let posted = invoice_of(b, &i.invoice)
        .map(|(_, f)| text(&f, "state") == "posted")
        .unwrap_or(false);
    if !posted || link_of(b, &i.invoice)?.is_some() {
        return Ok(());
    }
    write(b, &plan_in(b, &i, "posted")?).map(|_| ())
}
