//! The typed command `time_billing.create_invoice`: bill approved time as a DRAFT customer invoice
//! (docs/spec/accounting-se.md section 7, stage 8).
//!
//! The `timesheets` entry has minutes, a billable flag and an approval state, but no rate and no
//! customer, and a `projects_project` has neither a rate nor a customer. So the caller names the
//! `customer` (a `party`) and the `hourly_rate_minor`; nothing is built in. The command picks the
//! billable, approved entries of one project in a period that no earlier run has billed, creates
//! a draft `invoice` with one `invoice_line` per entry (quantity 1, the amount in the unit price:
//! the line quantity is a whole number and hours are not), and writes one link row per entry
//! (`time_billing`: entry id, invoice id). An entry is billed at most once. Posting the invoice is
//! `invoice.post`, as for any invoice. An agent may only dry-run.
//!
//! Amounts are integer minor units: `minutes * rate / 60`, rounded half up per entry. Hours are
//! shown as an `obop_values::Decimal` of two places. No float is used.

use crate::sie::{Env, call};
use obop_commands::{Actor, Dispatcher, Principal, Store, Value};
use obop_registry::{CommandDef, FieldDef, FieldKind, ModelDef, Module};
use obop_storage::Fields;
use obop_types::DataClass;
use obop_values::decimal::div_round;
use obop_values::{Decimal, Round};
use serde_json::{Value as J, json};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Weak};

const MODEL: &str = "time_billing";
const GROUP: &str = "invoicing.user";
const MANAGER: &str = "timesheets.manager";

pub fn module() -> Module {
    let f = |name, description| FieldDef::new(name, description, DataClass::None, false, GROUP);
    Module {
        id: "time_billing".into(),
        name: "Time to invoice".into(),
        description: "Bills approved time entries as a draft customer invoice".into(),
        models: vec![ModelDef {
            name: MODEL,
            description: "`create_invoice` bills the approved, billable, not yet billed `timesheets_entry` rows of one project in a period as a DRAFT `invoice` with one line per entry, at the hourly rate you give. Rows of this model are the link from an entry to the invoice that billed it (one each). Needs the invoicing module and the projects and timesheets packages",
            read_group: GROUP,
            write_group: GROUP,
            access: None,
            fields: vec![
                f("project", "Input: the id, code or name of the project"),
                f(
                    "customer",
                    "Input: the id or the name of the customer (a party)",
                ),
                f("from", "Input: first day of the period, YYYY-MM-DD"),
                f("to", "Input: last day of the period, YYYY-MM-DD"),
                f(
                    "hourly_rate_minor",
                    "Input: the price of one hour in minor currency units",
                )
                .kind(FieldKind::Number),
                f(
                    "currency",
                    "Input, optional: ISO 4217 code written on the invoice",
                ),
                f("dry_run", "Input: only show the lines, write nothing").kind(FieldKind::Bool),
                f("entry_id", "Link: the time entry id"),
                f("invoice_id", "Link and answer: the draft invoice id"),
                f("ok", "Answer: whether the invoice was built").kind(FieldKind::Bool),
                f("written", "Answer: whether the invoice was created").kind(FieldKind::Bool),
                f("entries", "Answer: how many entries are billed").kind(FieldKind::Number),
                f("minutes", "Answer: the minutes billed").kind(FieldKind::Number),
                f(
                    "hours",
                    "Answer: the hours billed, as a decimal with two places",
                ),
                f("net", "Answer: the invoice total in minor units").kind(FieldKind::Number),
                f(
                    "lines",
                    "Answer: the invoice lines: description, minutes, amount",
                ),
                f("notes", "Answer: remarks (a list)"),
            ],
            commands: vec![CommandDef::new(
                "create_invoice",
                "Bill the approved, billable, not yet billed time entries of a project (project) between from and to as a draft invoice for a customer at hourly_rate_minor; one line per entry. Refused in words when the customer, the rate, the period or the project is missing or unknown, when no entry qualifies, or when a billable entry in the period is not approved. dry_run shows the lines and writes nothing. An agent may only dry-run. The invoice stays a draft: posting is invoice.post",
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

/// Inputs as given.
#[derive(Default)]
pub struct Input {
    pub project: String,
    pub customer: String,
    pub from: String,
    pub to: String,
    pub hourly_rate_minor: Option<i64>,
    pub currency: String,
}

/// One planned invoice line.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub entry_id: String,
    pub description: String,
    pub minutes: i64,
    pub amount: i64,
}

pub struct Planned {
    pub party_id: String,
    pub currency: String,
    pub lines: Vec<Line>,
    pub minutes: i64,
    pub net: i64,
    pub notes: Vec<String>,
}

/// The price of `minutes` at `rate` per hour, rounded half up, in integer minor units.
pub fn amount_of(minutes: i64, rate: i64) -> Result<i64, String> {
    let v = div_round(minutes as i128 * rate as i128, 60, Round::HalfUp);
    i64::try_from(v).map_err(|_| "an amount overflows".to_string())
}

/// Hours with two places: `90` minutes is `1.50`.
pub fn hours_of(minutes: i64) -> Result<Decimal, String> {
    Decimal::new(minutes as i128, 0)
        .ok_or("the minutes overflow")?
        .checked_div(Decimal::from_i64(60), 2, Round::HalfUp)
        .map_err(|e| e.to_string())
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

/// Build the plan. Reads only; every refusal is in words.
pub fn plan(env: Env<'_>, p: &Principal, i: &Input) -> Result<Planned, String> {
    if !p.allows(GROUP) || !p.allows(MANAGER) {
        return Err(format!(
            "billing time needs the {GROUP} and {MANAGER} groups (an agent needs them in its scopes)"
        ));
    }
    let b = env.store.backend();
    let list = |m: &str| -> Result<Vec<(String, Fields)>, String> {
        b.list(m).map_err(|e| {
            format!("cannot read {m}: {e} (are the invoicing module and the projects and timesheets packages installed?)")
        })
    };
    let rate = i
        .hourly_rate_minor
        .ok_or("hourly_rate_minor is required (the price of one hour in minor currency units)")?;
    if rate <= 0 {
        return Err("hourly_rate_minor must be greater than zero".into());
    }
    if crate::sie::from_iso(&i.from).is_none() || crate::sie::from_iso(&i.to).is_none() {
        return Err("from and to are required as YYYY-MM-DD".into());
    }
    if i.from > i.to {
        return Err("from is after to".into());
    }
    let (party_id, _) = pick(&list("party")?, "customer", &i.customer, &["name"])?;
    let (pid, _) = pick(
        &list("projects_project")?,
        "project",
        &i.project,
        &["code", "name"],
    )?;
    let tasks: HashMap<String, String> = list("projects_task")?
        .into_iter()
        .filter(|(_, f)| text(f, "project_id") == pid)
        .map(|(id, f)| (id, text(&f, "title")))
        .collect();
    let billed: HashSet<String> = list(MODEL)?
        .iter()
        .map(|(_, f)| text(f, "entry_id"))
        .collect();
    let mut entries: Vec<(String, Fields)> = list("timesheets_entry")?
        .into_iter()
        .filter(|(id, f)| {
            tasks.contains_key(&text(f, "task_id"))
                && f.get("billable").and_then(J::as_bool) == Some(true)
                && !billed.contains(id)
        })
        .filter(|(_, f)| {
            let d = text(f, "worked_on");
            d >= i.from && d <= i.to
        })
        .collect();
    let open = entries
        .iter()
        .filter(|(_, f)| text(f, "state") != "approved")
        .count();
    if open > 0 {
        return Err(format!(
            "{open} billable time entries of this project in the period are not approved; approve or reject them first (nothing was written)"
        ));
    }
    if entries.is_empty() {
        return Err(
            "no approved, billable, unbilled time entries of this project in the period".into(),
        );
    }
    entries.sort_by(|(ia, a), (ib, b)| (text(a, "worked_on"), ia).cmp(&(text(b, "worked_on"), ib)));
    let mut lines = Vec::new();
    for (id, f) in entries {
        let minutes = f.get("minutes").and_then(J::as_i64).unwrap_or(0);
        if minutes <= 0 {
            return Err(format!(
                "time entry {id} has no positive minutes; fix or reject it first"
            ));
        }
        let title = tasks.get(&text(&f, "task_id")).cloned().unwrap_or_default();
        let mut d = format!(
            "{title} {}: {} h",
            text(&f, "worked_on"),
            hours_of(minutes)?
        );
        let note = text(&f, "description");
        if !note.is_empty() {
            d = format!("{d}, {note}");
        }
        lines.push(Line {
            entry_id: id,
            description: d,
            minutes,
            amount: amount_of(minutes, rate)?,
        });
    }
    let minutes = lines
        .iter()
        .try_fold(0i64, |a, l| a.checked_add(l.minutes))
        .ok_or("the minutes overflow")?;
    let net = lines
        .iter()
        .try_fold(0i64, |a, l| a.checked_add(l.amount))
        .ok_or("the total overflows")?;
    if net <= 0 {
        return Err("the total is zero; nothing to bill".into());
    }
    let currency = if i.currency.is_empty() {
        "SEK".to_string()
    } else {
        i.currency.clone()
    };
    Ok(Planned {
        party_id,
        currency,
        lines,
        minutes,
        net,
        notes: vec![
            "each line is minutes times the rate over 60, rounded half up to a minor unit; the invoice has no VAT, due date or issue date of its own and is a draft until invoice.post".into(),
        ],
    })
}

/// Plan, and unless `dry_run` create the draft invoice, its lines and the links as the caller.
pub fn create_invoice(
    env: Env<'_>,
    p: &Principal,
    i: &Input,
    dry_run: bool,
) -> Result<(Planned, Option<String>), String> {
    let pl = plan(env, p, i)?;
    if dry_run {
        return Ok((pl, None));
    }
    let made = call(
        env,
        p,
        "invoice.create",
        json!({"party_id": pl.party_id, "currency": pl.currency}),
    )?;
    let iid = made["id"]
        .as_str()
        .ok_or("invoice.create gave no id")?
        .to_string();
    let rows: Vec<J> = pl
        .lines
        .iter()
        .map(|l| {
            json!({"invoice_id": iid, "description": l.description, "quantity": 1,
                "unit_price_minor": l.amount})
        })
        .collect();
    if let Err(e) = crate::sie::bulk(env, p, "invoice_line", rows, None, false) {
        let _ = call(env, p, "invoice.cancel", json!({"id": iid}));
        return Err(format!(
            "the invoice lines were refused, the draft invoice {iid} was cancelled: {e}"
        ));
    }
    for l in &pl.lines {
        env.store.insert(
            MODEL,
            [
                ("entry_id".to_string(), Value::text(&l.entry_id)),
                ("invoice_id".to_string(), Value::text(&iid)),
            ]
            .into(),
        )?;
    }
    Ok((pl, Some(iid)))
}

pub fn register(d: &Arc<Dispatcher>, store: &Store) {
    let m = module();
    let (weak, st): (Weak<Dispatcher>, Store) = (Arc::downgrade(d), store.clone());
    d.register_with_actor(
        &m,
        MODEL,
        "create_invoice",
        Box::new(move |input| {
            let Value::Map(map) = input else {
                return Err("project is required".into());
            };
            let s = |k: &str| match map.get(k) {
                Some(Value::Text(t)) => t.clone(),
                _ => String::new(),
            };
            let dry = matches!(map.get("dry_run"), Some(Value::Bool(true)));
            let a = Actor::from_input(input);
            if a.agent && !dry {
                return Err("an agent may only dry-run the time billing (dry_run=true); ask the person to run it".into());
            }
            let rate = match map.get("hourly_rate_minor") {
                Some(Value::Int(n)) => Some(*n),
                _ => None,
            };
            let groups: Vec<&str> = a.groups.iter().map(String::as_str).collect();
            let p = Principal::user(&a.caller, &groups);
            let d = weak.upgrade().ok_or("the dispatcher is gone")?;
            let inp = Input {
                project: s("project"),
                customer: s("customer"),
                from: s("from"),
                to: s("to"),
                hourly_rate_minor: rate,
                currency: s("currency"),
            };
            let (pl, iid) = create_invoice(Env { d: &d, store: &st }, &p, &inp, dry)?;
            let lines = pl
                .lines
                .iter()
                .map(|l| {
                    Value::map([
                        ("description", Value::text(&l.description)),
                        ("minutes", Value::Int(l.minutes)),
                        ("amount", Value::Int(l.amount)),
                    ])
                })
                .collect();
            let mut out = vec![
                ("ok", Value::Bool(true)),
                ("written", Value::Bool(!dry)),
                ("entries", Value::Int(pl.lines.len() as i64)),
                ("minutes", Value::Int(pl.minutes)),
                ("hours", Value::text(&hours_of(pl.minutes)?.to_string())),
                ("net", Value::Int(pl.net)),
                ("lines", Value::List(lines)),
                (
                    "notes",
                    Value::List(pl.notes.iter().map(|s| Value::text(s)).collect()),
                ),
            ];
            if let Some(m) = iid {
                out.push(("invoice_id", Value::text(&m)));
            }
            Ok(Value::Map(out.into_iter().map(|(k, v)| (k.to_string(), v)).collect()))
        }),
    );
}
