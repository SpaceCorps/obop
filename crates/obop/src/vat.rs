//! The typed command `vat_computation.compute`: fill an `accounting_se_vat_report` from the
//! posted ledger (docs/spec/accounting-se.md section 3.2).
//!
//! An X1 package cannot join models, so the sums are made here. Everything is driven by rows the
//! user created in `accounting_se_vat_code` and `accounting_se_vat_box`; no code, box, rate or
//! account is built in (BAS is not redistributed, D34) and none of it is checked against
//! Skatteverket. The report stays in state `draft`, `boxes_confirmed` is cleared, and nothing is
//! approved or filed from here.
//!
//! How a ledger line finds its code (the package has no `vat_code` field on a line, so the
//! existing `tax_id` of the line is the link, and the code's own `tax_id` names the tax):
//! - a line whose `tax_id` is the tax of a code, and whose account is not that code's VAT account,
//!   is a base line: it counts toward the code's `base_box_id`, on the code's `base_side`;
//! - a line on the code's `vat_account_id` that carries the same `tax_id` counts toward
//!   `vat_box_id`, on `vat_side`;
//! - a line on a VAT account with no `tax_id` is given to the one code that has that account and
//!   whose tax appears on another line of the same move; if that is not exactly one code the run
//!   is refused and names the voucher.
//!
//! The amount of a side is `credit - debit` for `credit` and `debit - credit` for `debit`. Moves
//! in state `posted` or `reversed` dated inside the period count (as in the SIE export); drafts
//! and cancelled moves do not. Amounts are integer minor units (ore).

use crate::sie::{Env, from_iso};
use obop_commands::{Actor, Dispatcher, Principal, Store, Value};
use obop_registry::{CommandDef, FieldDef, FieldKind, ModelDef, Module};
use obop_storage::Fields;
use obop_types::DataClass;
use serde_json::{Value as J, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Weak};

const MODEL: &str = "vat_computation";
const GROUP: &str = "accounting.user";

pub fn module() -> Module {
    let f = |name, description| FieldDef::new(name, description, DataClass::None, false, GROUP);
    Module {
        id: "se_vat".into(),
        name: "Swedish VAT report".into(),
        description: "Fills a Swedish VAT report from the posted ledger by the codes and boxes the user created".into(),
        models: vec![ModelDef {
            name: MODEL,
            description: "`compute` sums the posted moves of the period of a draft `accounting_se_vat_report` by the user's VAT codes and boxes and replaces the report's lines. Needs the accounting and accounting_se packages. UNVERIFIED: nothing is checked against Skatteverket; the report stays a draft",
            read_group: GROUP,
            write_group: GROUP,
            access: None,
            fields: vec![
                f("report", "Input: the id or the name (for example 2026-Q3) of the VAT report"),
                f("dry_run", "Input: only compute and show the lines, write nothing").kind(FieldKind::Bool),
                f("ok", "Answer: whether the report was computed").kind(FieldKind::Bool),
                f("written", "Answer: whether the lines of the report were replaced").kind(FieldKind::Bool),
                f("period", "Answer: the period summed, from and to"),
                f("moves", "Answer: how many posted moves of the period were read").kind(FieldKind::Number),
                f("lines", "Answer: the lines, one per box: box, kind, amount (ore) and vouchers"),
                f("output_vat", "Answer: the sum of the output lines in ore").kind(FieldKind::Number),
                f("input_vat", "Answer: the sum of the input lines in ore").kind(FieldKind::Number),
                f("vat_to_pay", "Answer: output minus input in ore").kind(FieldKind::Number),
                f("notes", "Answer: remarks (a list)"),
            ],
            commands: vec![CommandDef::new(
                "compute",
                "Compute a draft VAT report (give report, an id or a name) from the posted moves of its period by the VAT codes and boxes you created. Replaces the lines of that report; refused when it is not a draft or when a code or box is missing. dry_run shows the lines and writes nothing. An agent may only dry-run. The result is UNVERIFIED and stays a draft",
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

fn amount(f: &Fields, k: &str) -> i64 {
    f.get(k).and_then(J::as_i64).unwrap_or(0)
}

struct Code {
    code: String,
    base_box: Option<String>,
    base_credit: bool,
    vat_box: Option<String>,
    vat_account: Option<String>,
    vat_credit: bool,
}

/// One line of the answer: box id, box number, kind, amount and the vouchers behind it.
pub type Line = (String, String, String, i64, Vec<String>);

/// What a computation found.
pub struct Computed {
    pub report_id: String,
    pub from: String,
    pub to: String,
    pub moves: usize,
    /// By box number.
    pub lines: Vec<Line>,
    pub notes: Vec<String>,
}

impl Computed {
    fn total(&self, kind: &str) -> i64 {
        self.lines.iter().filter(|l| l.2 == kind).map(|l| l.3).sum()
    }
}

fn side(credit: bool, debit_v: i64, credit_v: i64) -> i64 {
    if credit {
        credit_v - debit_v
    } else {
        debit_v - credit_v
    }
}

/// Sum the ledger for one report. Reads only; every refusal is in words.
pub fn compute_lines(env: Env<'_>, p: &Principal, report: &str) -> Result<Computed, String> {
    if !p.allows(GROUP) {
        return Err(format!(
            "computing a VAT report needs the {GROUP} group (an agent needs it in its scopes)"
        ));
    }
    let b = env.store.backend();
    let list = |m: &str| -> Result<Vec<(String, Fields)>, String> {
        b.list(m).map_err(|e| {
            format!(
                "cannot read {m}: {e} (are the accounting and accounting_se packages installed?)"
            )
        })
    };
    let reports = list("accounting_se_vat_report")?;
    let (rid, rf) = reports
        .iter()
        .find(|(id, f)| id == report || text(f, "name") == report)
        .ok_or_else(|| format!("no VAT report {report:?} (give its id or its name)"))?;
    let state = text(rf, "state");
    if state != "draft" {
        return Err(format!(
            "the report {} is {state}; only a draft is recomputed (withdraw a proposed one first)",
            text(rf, "name")
        ));
    }
    let (Some(from), Some(to)) = (
        from_iso(&text(rf, "start_on")),
        from_iso(&text(rf, "end_on")),
    ) else {
        return Err("the report needs start_on and end_on".into());
    };
    if to < from {
        return Err("end_on is before start_on".into());
    }
    let (from_s, to_s) = (text(rf, "start_on"), text(rf, "end_on"));

    // Boxes and codes: only rows the user made.
    let boxes: HashMap<String, Fields> = list("accounting_se_vat_box")?.into_iter().collect();
    let mut problems: Vec<String> = Vec::new();
    let mut codes: Vec<Code> = Vec::new();
    let mut by_tax: HashMap<String, usize> = HashMap::new();
    for (_, f) in list("accounting_se_vat_code")? {
        let name = text(&f, "code");
        let opt = |k: &str| Some(text(&f, k)).filter(|s| !s.is_empty());
        let c = Code {
            code: name.clone(),
            base_box: opt("base_box_id"),
            base_credit: text(&f, "base_side") == "credit",
            vat_box: opt("vat_box_id"),
            vat_account: opt("vat_account_id"),
            vat_credit: text(&f, "vat_side") == "credit",
        };
        if c.base_box.is_none() && c.vat_box.is_none() {
            continue; // maps to no box: exempt or none; nothing to sum
        }
        for (what, id) in [("base_box_id", &c.base_box), ("vat_box_id", &c.vat_box)] {
            if let Some(id) = id
                && !boxes.contains_key(id)
            {
                problems.push(format!(
                    "code {name}: {what} names a box that does not exist"
                ));
            }
        }
        let Some(tax) = opt("tax_id") else {
            problems.push(format!(
                "code {name}: maps to a box but has no tax_id, so no ledger line can carry it"
            ));
            continue;
        };
        if c.base_box.is_some() && text(&f, "base_side").is_empty() {
            problems.push(format!("code {name}: base_box_id is set without base_side"));
        }
        if c.vat_box.is_some() && (c.vat_account.is_none() || text(&f, "vat_side").is_empty()) {
            problems.push(format!(
                "code {name}: vat_box_id is set without vat_account_id and vat_side"
            ));
        }
        if let Some(prev) = by_tax.insert(tax, codes.len()) {
            problems.push(format!(
                "codes {} and {name} use the same tax_id, so a line cannot tell them apart",
                codes[prev].code
            ));
        }
        codes.push(c);
    }
    if codes.is_empty() && problems.is_empty() {
        return Err("no VAT code maps to a box: create accounting_se_vat_box and accounting_se_vat_code rows first (the package seeds none)".into());
    }

    if !problems.is_empty() {
        problems.sort();
        return Err(format!("not computed: {}", problems.join("; ")));
    }

    // Posted moves of the period.
    let mut moves: HashMap<String, String> = HashMap::new(); // id -> voucher label
    for (id, f) in list("accounting_move")? {
        if !matches!(text(&f, "state").as_str(), "posted" | "reversed") {
            continue;
        }
        let Some(d) = from_iso(&text(&f, "date")) else {
            return Err(format!("move {id} has no valid date"));
        };
        if d < from || d > to {
            continue;
        }
        let (n, r) = (text(&f, "number"), text(&f, "reference"));
        let label = match (n.is_empty(), r.is_empty()) {
            (false, false) => format!("{n} ({r})"),
            (false, true) => n,
            (true, false) => r,
            (true, true) => id.clone(),
        };
        moves.insert(id, label);
    }
    let mut per_move: BTreeMap<String, Vec<Fields>> = BTreeMap::new();
    for (_, f) in list("accounting_move_line")? {
        let mid = text(&f, "move_id");
        if moves.contains_key(&mid) {
            per_move.entry(mid).or_default().push(f);
        }
    }
    let vat_accounts: BTreeSet<&str> = codes
        .iter()
        .filter(|c| c.vat_box.is_some())
        .filter_map(|c| c.vat_account.as_deref())
        .collect();

    let mut sums: BTreeMap<String, (i64, BTreeSet<String>)> = BTreeMap::new();
    for c in &codes {
        for id in c.base_box.iter().chain(c.vat_box.iter()) {
            sums.entry(id.clone()).or_default();
        }
    }
    for (mid, lines) in &per_move {
        let label = &moves[mid];
        let taxes_here: BTreeSet<String> = lines
            .iter()
            .map(|l| text(l, "tax_id"))
            .filter(|t| !t.is_empty())
            .collect();
        for l in lines {
            let (acc, tax) = (text(l, "account_id"), text(l, "tax_id"));
            let (dr, cr) = (amount(l, "debit"), amount(l, "credit"));
            let code = if tax.is_empty() {
                if !vat_accounts.contains(acc.as_str()) {
                    continue;
                }
                let cands: Vec<usize> = codes
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.vat_box.is_some() && c.vat_account.as_deref() == Some(&acc))
                    .filter(|(i, _)| by_tax.iter().any(|(t, j)| j == i && taxes_here.contains(t)))
                    .map(|(i, _)| i)
                    .collect();
                if let [i] = cands[..] {
                    i
                } else {
                    problems.push(format!(
                        "voucher {label}: a line on a VAT account has no tax_id and the move does not point to exactly one code"
                    ));
                    continue;
                }
            } else if let Some(&i) = by_tax.get(&tax) {
                i
            } else {
                continue; // a tax that no mapped code stands for
            };
            let c = &codes[code];
            let (target, v) = match (&c.vat_box, &c.base_box) {
                (Some(vb), _) if c.vat_account.as_deref() == Some(&acc) => {
                    (vb, side(c.vat_credit, dr, cr))
                }
                (_, Some(bb)) => (bb, side(c.base_credit, dr, cr)),
                _ => continue,
            };
            let e = sums.entry(target.clone()).or_default();
            match e.0.checked_add(v) {
                Some(s) => e.0 = s,
                None => problems.push(format!("the sum of box {target} overflows")),
            }
            e.1.insert(label.clone());
        }
    }

    let mut out: Vec<Line> = Vec::new();
    for (id, (sum, vouchers)) in sums {
        let bf = &boxes[&id];
        if bf.get("not_used").and_then(J::as_bool) == Some(true)
            && (sum != 0 || !vouchers.is_empty())
        {
            problems.push(format!(
                "box {} is marked not used but receives an amount",
                text(bf, "box")
            ));
        }
        out.push((
            id,
            text(bf, "box"),
            text(bf, "kind"),
            sum,
            vouchers.into_iter().collect(),
        ));
    }
    if !problems.is_empty() {
        problems.sort();
        problems.dedup();
        return Err(format!("not computed: {}", problems.join("; ")));
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    let mut notes = vec!["UNVERIFIED: the codes, boxes and rates are the user's own and were not checked against Skatteverket (SKV 4700); the report stays a draft".into()];
    if boxes.values().any(|b| text(b, "review") != "verified") {
        notes.push("at least one box of the chart is not verified by an approver".into());
    }
    Ok(Computed {
        report_id: rid.clone(),
        from: from_s,
        to: to_s,
        moves: moves.len(),
        lines: out,
        notes,
    })
}

/// Compute, and unless `dry_run` replace the lines of the report and clear `boxes_confirmed`.
/// Every write is a command as the caller.
pub fn compute(
    env: Env<'_>,
    p: &Principal,
    report: &str,
    dry_run: bool,
) -> Result<Computed, String> {
    let done = compute_lines(env, p, report)?;
    if dry_run {
        return Ok(done);
    }
    let old: Vec<String> = env
        .store
        .backend()
        .list("accounting_se_vat_report_line")
        .map_err(|e| format!("cannot read the lines: {e}"))?
        .into_iter()
        .filter(|(_, f)| text(f, "report_id") == done.report_id)
        .map(|(id, _)| id)
        .collect();
    for id in old {
        crate::sie::call(
            env,
            p,
            "accounting_se_vat_report_line.delete",
            json!({"id": id}),
        )?;
    }
    if !done.lines.is_empty() {
        let rows: Vec<J> = done
            .lines
            .iter()
            .map(|(bid, _, kind, sum, v)| {
                json!({"report_id": done.report_id, "box_id": bid, "kind": kind, "amount": sum, "vouchers": v.join("\n")})
            })
            .collect();
        crate::sie::bulk(env, p, "accounting_se_vat_report_line", rows, None, false)?;
    }
    crate::sie::call(
        env,
        p,
        "accounting_se_vat_report.update",
        json!({"id": done.report_id, "boxes_confirmed": false}),
    )?;
    Ok(done)
}

pub fn register(d: &Arc<Dispatcher>, store: &Store) {
    let m = module();
    let (weak, st): (Weak<Dispatcher>, Store) = (Arc::downgrade(d), store.clone());
    d.register_with_actor(
        &m,
        MODEL,
        "compute",
        Box::new(move |input| {
            let Value::Map(map) = input else {
                return Err("report is required".into());
            };
            let Some(Value::Text(report)) = map.get("report") else {
                return Err("report is required: the id or the name of the VAT report".into());
            };
            let dry = matches!(map.get("dry_run"), Some(Value::Bool(true)));
            let a = Actor::from_input(input);
            if a.agent && !dry {
                return Err("an agent may only dry-run the VAT computation (dry_run=true); ask the person to run it".into());
            }
            let groups: Vec<&str> = a.groups.iter().map(String::as_str).collect();
            let p = Principal::user(&a.caller, &groups);
            let d = weak.upgrade().ok_or("the dispatcher is gone")?;
            let done = compute(Env { d: &d, store: &st }, &p, report, dry)?;
            let lines = done
                .lines
                .iter()
                .map(|(_, bx, kind, sum, v)| {
                    Value::map([
                        ("box", Value::text(bx)),
                        ("kind", Value::text(kind)),
                        ("amount", Value::Int(*sum)),
                        (
                            "vouchers",
                            Value::List(v.iter().map(|s| Value::text(s)).collect()),
                        ),
                    ])
                })
                .collect();
            let (out, inp) = (done.total("output"), done.total("input"));
            Ok(Value::map([
                ("ok", Value::Bool(true)),
                ("written", Value::Bool(!dry)),
                (
                    "period",
                    Value::text(&format!("{} to {}", done.from, done.to)),
                ),
                ("moves", Value::Int(done.moves as i64)),
                ("lines", Value::List(lines)),
                ("output_vat", Value::Int(out)),
                ("input_vat", Value::Int(inp)),
                ("vat_to_pay", Value::Int(out - inp)),
                (
                    "notes",
                    Value::List(done.notes.iter().map(|s| Value::text(s)).collect()),
                ),
            ]))
        }),
    );
}
