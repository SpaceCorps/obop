//! The export: posted moves of one year as a SIE 4 file.
//!
//! Moves in state `posted` or `reversed` whose date is in the year are exported (a reversed move
//! was posted; its opposite move is an ordinary posted one, so the sums stay right). Drafts and
//! cancelled moves are not. The year is the `accounting_fiscal_year` that starts in the given
//! calendar year, or the calendar year when there is none. Opening balances (`#IB`) of the
//! balance sheet accounts are the sums of the posted moves before the year; there is no opening
//! entry model. `#UB` and `#RES` are derived by `obop-sie` from those and the vouchers.

use super::{EXPORT_GROUP, Env, from_iso, today};
use obop_commands::Principal;
use obop_sie::{Account, AccountKind, Balance, Company, Date, FiscalYear, Line, SieFile, Voucher};
use serde_json::Value as J;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};

pub struct Exported {
    pub bytes: Vec<u8>,
    pub filename: String,
    pub from: String,
    pub to: String,
    pub accounts: usize,
    pub vouchers: usize,
    pub sha256: String,
    pub notes: Vec<String>,
}

fn kind_of(account_type: &str) -> Option<AccountKind> {
    match account_type {
        t if t.starts_with("asset_") => Some(AccountKind::Asset),
        t if t.starts_with("liability_") || t == "equity" => Some(AccountKind::Liability),
        t if t.starts_with("income") => Some(AccountKind::Income),
        t if t.starts_with("expense") => Some(AccountKind::Cost),
        _ => None,
    }
}

fn text(f: &obop_storage::Fields, k: &str) -> String {
    f.get(k).and_then(J::as_str).unwrap_or_default().to_string()
}

fn money(f: &obop_storage::Fields, k: &str) -> i64 {
    f.get(k).and_then(J::as_i64).unwrap_or(0)
}

/// `A17` into (`A`, `17`): letters, then digits without a leading zero.
fn split_reference(r: &str) -> Option<(String, String)> {
    let at = r.find(|c: char| c.is_ascii_digit())?;
    let (s, n) = r.split_at(at);
    let ok = !s.is_empty()
        && s.chars().all(|c| c.is_ascii_alphabetic())
        && n.chars().all(|c| c.is_ascii_digit())
        && !n.starts_with('0');
    ok.then(|| (s.to_string(), n.to_string()))
}

/// The digits at the end of a move number (`JE/2026/00042` -> `42`).
fn number_tail(n: &str) -> Option<String> {
    let tail: String = n
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let t = tail.trim_start_matches('0');
    (!tail.is_empty()).then(|| if t.is_empty() { "0".into() } else { t.into() })
}

pub fn export(env: Env<'_>, p: &Principal, year: u16) -> Result<Exported, String> {
    if !p.allows(EXPORT_GROUP) {
        return Err(format!(
            "exporting the books needs the {EXPORT_GROUP} group (an agent needs it in its scopes)"
        ));
    }
    let b = env.store.backend();
    let list = |model: &str| -> Result<Vec<(String, obop_storage::Fields)>, String> {
        b.list(model)
            .map_err(|e| format!("cannot read {model}: {e}"))
    };
    let accounts = list("accounting_account").map_err(|e| {
        format!("{e} (is the accounting package installed? obop modules install-from accounting)")
    })?;
    let mut notes = Vec::new();

    // The year.
    let (mut start, mut end) = (
        Date::new(year, 1, 1).expect("valid"),
        Date::new(year, 12, 31).expect("valid"),
    );
    for (_, f) in list("accounting_fiscal_year").unwrap_or_default() {
        if let (Some(s), Some(e)) = (
            from_iso(&text(&f, "start_on")),
            from_iso(&text(&f, "end_on")),
        ) && s.year == year
        {
            (start, end) = (s, e);
            break;
        }
    }

    // Accounts: numeric codes only.
    let mut by_id: HashMap<String, (u32, AccountKind)> = HashMap::new();
    let mut sie_accounts: Vec<Account> = Vec::new();
    let mut skipped = Vec::new();
    for (id, f) in &accounts {
        let code = text(f, "code");
        let Ok(n) = code.parse::<u32>() else {
            skipped.push((id.clone(), code));
            continue;
        };
        let kind = kind_of(&text(f, "account_type")).unwrap_or(AccountKind::Asset);
        by_id.insert(id.clone(), (n, kind));
        sie_accounts.push(Account {
            number: n,
            name: text(f, "name"),
            kind: Some(kind),
        });
    }

    // Moves of the year, and the balances before it.
    let moves = list("accounting_move")?;
    let in_ledger =
        |f: &obop_storage::Fields| matches!(text(f, "state").as_str(), "posted" | "reversed");
    let mut dated: Vec<(Date, String, String, &obop_storage::Fields)> = Vec::new();
    let mut before: HashSet<String> = HashSet::new();
    for (id, f) in &moves {
        if !in_ledger(f) {
            continue;
        }
        let Some(d) = from_iso(&text(f, "date")) else {
            return Err(format!("move {id} has no valid date"));
        };
        if d < start {
            before.insert(id.clone());
        } else if d <= end {
            dated.push((d, text(f, "number"), id.clone(), f));
        }
    }
    dated.sort_by(|a, b| (a.0, &a.1, &a.2).cmp(&(b.0, &b.1, &b.2)));
    let in_year: HashSet<&str> = dated.iter().map(|m| m.2.as_str()).collect();

    // Lines, in creation order (the ids are time ordered).
    let mut lines: Vec<(String, obop_storage::Fields)> = list("accounting_move_line")?;
    lines.sort_by(|a, b| a.0.cmp(&b.0));
    let mut per_move: HashMap<&str, Vec<&obop_storage::Fields>> = HashMap::new();
    let mut opening: BTreeMap<u32, i64> = BTreeMap::new();
    let mut unknown_account = HashSet::new();
    for (_, f) in &lines {
        let Some(m) = f.get("move_id").and_then(J::as_str) else {
            continue;
        };
        let acct = f.get("account_id").and_then(J::as_str).unwrap_or_default();
        if in_year.contains(m) {
            if !by_id.contains_key(acct) {
                unknown_account.insert(acct.to_string());
            }
            per_move.entry(m).or_default().push(f);
        } else if before.contains(m)
            && let Some((n, k)) = by_id.get(acct)
            && k.is_balance_sheet()
        {
            *opening.entry(*n).or_default() += money(f, "debit") - money(f, "credit");
        }
    }
    if !unknown_account.is_empty() {
        let names: Vec<String> = skipped.iter().map(|(_, c)| c.clone()).collect();
        return Err(format!(
            "{} exported line(s) are on an account whose code is not a number ({}): SIE accounts are numbers",
            unknown_account.len(),
            names.join(", ")
        ));
    }
    if !skipped.is_empty() {
        notes.push(format!(
            "{} account(s) with a non-numeric code are not in the file (no line of the year uses them)",
            skipped.len()
        ));
    }

    // Journals for the series.
    let journal_code: HashMap<String, String> = list("accounting_journal")?
        .into_iter()
        .map(|(id, f)| (id, text(&f, "code")))
        .collect();

    // Vouchers.
    let mut used: HashSet<(String, String)> = HashSet::new();
    let mut vouchers = Vec::new();
    let mut counters: HashMap<String, u64> = HashMap::new();
    for (d, number, id, f) in &dated {
        let journal = journal_code
            .get(&text(f, "journal_id"))
            .cloned()
            .unwrap_or_else(|| "A".into());
        let series_safe = |s: &str| {
            if s.is_empty() {
                "A".to_string()
            } else {
                s.to_string()
            }
        };
        let candidates = [
            split_reference(&text(f, "reference")),
            number_tail(number).map(|n| (series_safe(&journal), n)),
        ];
        let mut key = candidates.into_iter().flatten().find(|k| !used.contains(k));
        if key.is_none() {
            // Both taken (or no number): the next free number of the journal's series.
            let s = series_safe(&journal);
            let c = counters.entry(s.clone()).or_insert(0);
            loop {
                *c += 1;
                let k = (s.clone(), c.to_string());
                if !used.contains(&k) {
                    key = Some(k);
                    break;
                }
            }
        }
        let (series, num) = key.expect("a key was chosen");
        used.insert((series.clone(), num.clone()));
        let narration = text(f, "narration");
        let title = narration
            .lines()
            .next()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map_or_else(|| text(f, "reference"), String::from);
        let mut vlines = Vec::new();
        for l in per_move.get(id.as_str()).map_or(&[][..], Vec::as_slice) {
            let (n, _) = by_id[l.get("account_id").and_then(J::as_str).unwrap_or_default()];
            let label = text(l, "label");
            vlines.push(Line::new(
                n,
                money(l, "debit") - money(l, "credit"),
                (!label.is_empty()).then_some(label.as_str()),
            ));
        }
        vouchers.push(Voucher {
            series,
            number: num,
            date: *d,
            text: title,
            registered: None,
            signed: None,
            lines: vlines,
        });
    }

    // The company.
    let mut company = Company {
        name: String::new(),
        org_number: None,
    };
    if let Ok(rows) = b.list("accounting_se_company")
        && let Some((_, f)) = rows.first()
    {
        company.name = text(f, "name");
        let org = text(f, "org_number");
        company.org_number = (!org.is_empty()).then_some(org);
    }
    if company.name.is_empty() {
        company.name = "Unnamed company".into();
        notes.push(
            "no company record (accounting_se_company): the file says `Unnamed company` and has no #ORGNR"
                .into(),
        );
    }

    let mut file = SieFile::new(&company.name, start, end);
    file.generated = today();
    file.company = company;
    file.years = vec![FiscalYear {
        index: 0,
        start,
        end,
    }];
    sie_accounts.sort_by_key(|a| a.number);
    file.accounts = sie_accounts;
    file.opening = opening
        .into_iter()
        .filter(|(_, v)| *v != 0)
        .map(|(account, amount_minor)| Balance {
            year_index: 0,
            account,
            amount_minor,
        })
        .collect();
    file.vouchers = vouchers;
    let bytes = obop_sie::export(&file);
    let sha256 = Sha256::digest(&bytes)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect();
    Ok(Exported {
        filename: format!("{year}.se"),
        from: super::iso(start),
        to: super::iso(end),
        accounts: file.accounts.len(),
        vouchers: file.vouchers.len(),
        sha256,
        notes,
        bytes,
    })
}
