//! The import: a SIE 4 file into the `accounting` models.
//!
//! Accounts that do not exist (by code) are created with a type from the same UNVERIFIED table
//! the OAS importer uses (`import_oas::types`), which the person confirms with
//! `--default-account-types` or `--account-types FILE`. Each voucher series becomes a journal
//! (an existing journal with that code is used). Every voucher becomes a draft `accounting_move`
//! with the series and number in `reference` (`A17`); the lines follow. Nothing here posts unless
//! `post` is set, and then it is the real `accounting_move.post`, one voucher at a time in file
//! order, as the caller (a signed-in person with `accounting.approver`).
//!
//! Idempotent: moves and accounts are created with `bulk_create` and a `source` key (the import
//! map), so a second run finds them and creates nothing; a move that has no lines yet (an
//! interrupted run) gets them. After the run, the debit and credit totals per account of the
//! imported moves are compared with the file's, and any difference fails the run.

use super::{APPROVER_GROUP, Env, IMPORT_GROUP, bulk, call, existing, iso, models_of};
use crate::import_oas::types;
use obop_commands::Principal;
use obop_sie::{AccountKind, SieFile, Voucher};
use serde_json::{Value as J, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;

const BATCH: usize = 1000;

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub dry_run: bool,
    pub post: bool,
    /// Accept the built-in account type table (UNVERIFIED).
    pub default_account_types: bool,
    /// `number,type` lines that override or complete the table.
    pub account_types: Option<String>,
    /// Wait out the bulk rate limit (the CLI); otherwise it is an error and a re-run resumes.
    pub wait_on_rate_limit: bool,
}

#[derive(Debug, Default)]
pub struct Counts {
    pub accounts_new: usize,
    pub accounts_matched: usize,
    pub journals_new: usize,
    pub journals_matched: usize,
    pub vouchers_new: usize,
    pub vouchers_already: usize,
    pub lines_new: usize,
    pub posted: usize,
}

#[derive(Debug, Default)]
pub struct Report {
    pub dry_run: bool,
    pub source_name: String,
    pub counts: Counts,
    account_table: Vec<(u32, String, String)>,
    notes: Vec<String>,
    not_imported: Vec<String>,
    pub problems: Vec<String>,
    pub verified: Option<String>,
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let c = &self.counts;
        let verb = if self.dry_run {
            "would create"
        } else {
            "created"
        };
        writeln!(
            f,
            "SIE import{}: {}",
            if self.dry_run {
                " (dry run, nothing written)"
            } else {
                ""
            },
            self.source_name
        )?;
        writeln!(f, "\n{verb}:")?;
        writeln!(
            f,
            "  accounts   {} new, {} matched existing (by code)",
            c.accounts_new, c.accounts_matched
        )?;
        writeln!(
            f,
            "  journals   {} new, {} matched existing (by code)",
            c.journals_new, c.journals_matched
        )?;
        writeln!(
            f,
            "  vouchers   {} new draft move(s), {} already imported",
            c.vouchers_new, c.vouchers_already
        )?;
        writeln!(f, "  lines      {} new", c.lines_new)?;
        if !self.dry_run && c.posted > 0 {
            writeln!(f, "  posted     {} move(s), in file order", c.posted)?;
        }
        if !self.account_table.is_empty() {
            writeln!(
                f,
                "\naccount type mapping for new accounts (UNVERIFIED defaults; confirm or override with --account-types FILE):"
            )?;
            for (n, name, ty) in &self.account_table {
                writeln!(f, "  {n:<6} {ty:<24} {name}")?;
            }
        }
        if !self.not_imported.is_empty() {
            writeln!(f, "\nnot imported (counted, never silently dropped):")?;
            for n in &self.not_imported {
                writeln!(f, "  {n}")?;
            }
        }
        if !self.notes.is_empty() {
            writeln!(f, "\nnotes:")?;
            for n in &self.notes {
                writeln!(f, "  {n}")?;
            }
        }
        if let Some(v) = &self.verified {
            writeln!(f, "\nverified: {v}")?;
        }
        if !self.problems.is_empty() {
            writeln!(f, "\nPROBLEMS ({}):", self.problems.len())?;
            for p in self.problems.iter().take(50) {
                writeln!(f, "  {p}")?;
            }
            if self.problems.len() > 50 {
                writeln!(f, "  ... and {} more", self.problems.len() - 50)?;
            }
        }
        Ok(())
    }
}

/// The import-map source system: the company (name and organisation number), so that two
/// companies in one database never share keys.
fn source_system(file: &SieFile) -> String {
    let org = file.company.org_number.as_deref().unwrap_or("");
    let h = Sha256::digest(format!("{}|{org}", file.company.name).as_bytes());
    let hex: String = h.iter().take(8).map(|b| format!("{b:02x}")).collect();
    format!("sie:{hex}")
}

/// The key of a voucher: the year of the file, the series and the number.
fn voucher_key(year: &str, v: &Voucher) -> String {
    format!("{year}:{}:{}", v.series, v.number)
}

fn reference(v: &Voucher) -> String {
    format!("{}{}", v.series, v.number)
}

/// Debit and credit of a signed amount.
fn split(amount: i64) -> (i64, i64) {
    if amount >= 0 {
        (amount, 0)
    } else {
        (0, -amount)
    }
}

/// The class rule for an account without `#KTYP` (UNVERIFIED): class 1 assets, 2 equity and
/// liabilities, 3 income, 4 to 7 costs, 8 financial items (income to 8399, else cost).
fn kind_letter(a: &obop_sie::Account) -> Option<&'static str> {
    match a.kind {
        Some(AccountKind::Asset) => Some("T"),
        Some(AccountKind::Liability) => Some("S"),
        Some(AccountKind::Cost) => Some("K"),
        Some(AccountKind::Income) => Some("I"),
        None => match a.number {
            1000..=1999 => Some("T"),
            2000..=2999 => Some("S"),
            3000..=3999 => Some("I"),
            4000..=7999 => Some("K"),
            8000..=8399 => Some("I"),
            8400..=8999 => Some("K"),
            _ => None,
        },
    }
}

/// What a file holds that this import does not take.
fn not_imported(file: &SieFile) -> Vec<String> {
    let mut out = Vec::new();
    let years = |l: &[obop_sie::Balance]| l.iter().filter(|b| b.year_index == 0).count();
    if !file.opening.is_empty() {
        out.push(format!(
            "#IB opening balances: {} row(s) of year 0 (obop has no opening balance entry; book one if the books do not start from zero)",
            years(&file.opening)
        ));
    }
    if !file.closing.is_empty() {
        out.push(format!(
            "#UB closing balances: {} row(s) (derived, not checked)",
            years(&file.closing)
        ));
    }
    if !file.results.is_empty() {
        out.push(format!(
            "#RES results: {} row(s) (derived, not stored)",
            years(&file.results)
        ));
    }
    let objects: usize = file
        .vouchers
        .iter()
        .flat_map(|v| &v.lines)
        .filter(|l| !l.objects.is_empty())
        .count();
    if objects > 0 {
        out.push(format!(
            "object tags (#DIM/#OBJEKT lists) on {objects} line(s): dropped"
        ));
    }
    out.push(
        "the previous fiscal years (#RAR -1 ...), #SRU, #VALUTA and #KSUMMA are not read".into(),
    );
    out
}

#[allow(
    clippy::result_large_err,
    reason = "the report is the answer in both cases: a refused run still shows what it found"
)]
pub fn import(env: Env<'_>, p: &Principal, bytes: &[u8], opts: &Options) -> Result<Report, Report> {
    let mut rep = Report {
        dry_run: opts.dry_run,
        ..Report::default()
    };
    let file = match obop_sie::parse(bytes) {
        Ok(f) => f,
        Err(e) => {
            rep.source_name = "(unreadable file)".into();
            rep.problems.push(format!("SIE file refused: {e}"));
            return Err(rep);
        }
    };
    let ytag = file
        .years
        .iter()
        .find(|y| y.index == 0)
        .map_or_else(|| "0".to_string(), |y| y.start.compact());
    rep.source_name = format!("{} (year {ytag})", file.company.name);
    rep.not_imported = not_imported(&file);
    let system = source_system(&file);

    let models = models_of(env, p);
    let mut problems: Vec<String> = Vec::new();
    if ![
        "accounting_account",
        "accounting_journal",
        "accounting_move",
        "accounting_move_line",
    ]
    .iter()
    .all(|m| models.contains(*m))
    {
        problems.push(
            "the accounting package is not installed here (obop modules install-from accounting), or you lack its groups".into(),
        );
        rep.problems = problems;
        return Err(rep);
    }
    if !p.allows(IMPORT_GROUP) {
        problems.push(format!("importing needs the {IMPORT_GROUP} group"));
    }
    if opts.post && !(p.is_person() && p.allows(APPROVER_GROUP)) {
        problems.push(format!(
            "--post runs the real post action, which only a signed-in person with the {APPROVER_GROUP} group may do (not an agent, not a scoped token)"
        ));
    }

    // The file itself.
    let mut seen: HashSet<(&str, &str)> = HashSet::new();
    for v in &file.vouchers {
        if !seen.insert((&v.series, &v.number)) {
            problems.push(format!(
                "voucher {} appears twice in the file",
                reference(v)
            ));
        }
        if v.sum_minor() != 0 {
            problems.push(format!("voucher {} does not balance", reference(v)));
        }
    }
    let declared: HashSet<u32> = file.accounts.iter().map(|a| a.number).collect();
    let mut undeclared: Vec<u32> = file
        .vouchers
        .iter()
        .flat_map(|v| v.lines.iter().map(|l| l.account))
        .filter(|a| !declared.contains(a))
        .collect();
    undeclared.sort_unstable();
    undeclared.dedup();
    if !undeclared.is_empty() {
        problems.push(format!(
            "{} account(s) are used by vouchers but have no #KONTO line: {:?}",
            undeclared.len(),
            &undeclared[..undeclared.len().min(8)]
        ));
    }

    // What exists.
    let backend = env.store.backend();
    let mapped = |table: &str, model: &str| -> HashMap<String, String> {
        backend
            .list("import_map")
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(_, f)| {
                let s = |k: &str| f.get(k).and_then(J::as_str).unwrap_or_default();
                (s("source_system") == system && s("source_table") == table && s("model") == model)
                    .then(|| (s("source_id").to_string(), s("record").to_string()))
            })
            .filter(|(_, rec)| matches!(backend.get(model, rec), Ok(Some(_))))
            .collect()
    };
    let account_by_code = existing(env, "accounting_account", "code");
    let journal_by_code = existing(env, "accounting_journal", "code");
    let moves_mapped = mapped("voucher", "accounting_move");
    let mut lines_of: HashMap<String, usize> = HashMap::new();
    for (_, f) in backend.list("accounting_move_line").unwrap_or_default() {
        if let Some(m) = f.get("move_id").and_then(J::as_str) {
            *lines_of.entry(m.to_string()).or_default() += 1;
        }
    }

    // Account types.
    let overrides = match &opts.account_types {
        Some(t) => match types::parse_overrides(t) {
            Ok(o) => o,
            Err(e) => {
                problems.push(e);
                BTreeMap::new()
            }
        },
        None => BTreeMap::new(),
    };
    let mut new_accounts: Vec<(&obop_sie::Account, &'static str)> = Vec::new();
    let mut missing_types = Vec::new();
    for a in &file.accounts {
        if account_by_code.contains_key(&a.number.to_string()) {
            rep.counts.accounts_matched += 1;
            continue;
        }
        let ty = overrides.get(&i64::from(a.number)).copied().or_else(|| {
            opts.default_account_types
                .then(|| kind_letter(a).and_then(|k| types::default_type(k, i64::from(a.number))))
                .flatten()
        });
        let shown = ty.map_or_else(
            || {
                kind_letter(a)
                    .and_then(|k| types::default_type(k, i64::from(a.number)))
                    .map_or("(needs a type)".to_string(), |t| {
                        format!("{t} (default, not accepted)")
                    })
            },
            String::from,
        );
        rep.account_table.push((a.number, a.name.clone(), shown));
        match ty {
            Some(t) => new_accounts.push((a, t)),
            None => missing_types.push(a.number),
        }
    }
    rep.counts.accounts_new = new_accounts.len() + missing_types.len();
    if !missing_types.is_empty() {
        problems.push(format!(
            "{} new account(s) have no confirmed type: confirm the table above with --default-account-types, or give --account-types FILE (lines number,type); first: {:?}",
            missing_types.len(),
            &missing_types[..missing_types.len().min(8)]
        ));
    }

    // Journals and vouchers.
    let series: Vec<&str> = file
        .vouchers
        .iter()
        .map(|v| v.series.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let new_journals: Vec<&str> = series
        .iter()
        .copied()
        .filter(|s| !journal_by_code.contains_key(*s))
        .collect();
    rep.counts.journals_new = new_journals.len();
    rep.counts.journals_matched = series.len() - new_journals.len();
    let mut moves_now: HashSet<(String, String)> = HashSet::new();
    for (_, f) in backend.list("accounting_move").unwrap_or_default() {
        if let (Some(j), Some(r)) = (
            f.get("journal_id").and_then(J::as_str),
            f.get("reference").and_then(J::as_str),
        ) {
            moves_now.insert((j.to_string(), r.to_string()));
        }
    }
    for v in &file.vouchers {
        let key = voucher_key(&ytag, v);
        if let Some(mid) = moves_mapped.get(&key) {
            rep.counts.vouchers_already += 1;
            if lines_of.get(mid).copied().unwrap_or(0) == 0 {
                rep.counts.lines_new += v.lines.len();
            }
            continue;
        }
        rep.counts.vouchers_new += 1;
        rep.counts.lines_new += v.lines.len();
        if let Some(j) = journal_by_code.get(&v.series)
            && moves_now.contains(&(j.clone(), reference(v)))
        {
            problems.push(format!(
                "a move with reference {} already exists in journal {} but is not in the import map (an interrupted run of another tool or a hand entry): delete or rename it, then run again",
                reference(v),
                v.series
            ));
        }
    }
    if file.vouchers.is_empty() {
        rep.notes
            .push("the file has no vouchers: only accounts are imported".into());
    }
    if !problems.is_empty() {
        rep.problems = problems;
        return Err(rep);
    }
    if opts.dry_run {
        return Ok(rep);
    }

    // ---- the run -----------------------------------------------------------------------------
    if let Err(e) = run(
        env,
        p,
        &file,
        (&system, &ytag),
        &new_accounts,
        &new_journals,
        opts,
        &mut rep,
    ) {
        rep.problems.push(e);
        return Err(rep);
    }
    match verify(
        env,
        &file,
        &moves_after(env, &system, &ytag, &file),
        opts.post,
    ) {
        Ok(s) => rep.verified = Some(s),
        Err(e) => {
            rep.problems.push(e);
            return Err(rep);
        }
    }
    Ok(rep)
}

/// The move of every voucher, after the run (from the import map).
fn moves_after(env: Env<'_>, system: &str, ytag: &str, file: &SieFile) -> Vec<(usize, String)> {
    let backend = env.store.backend();
    let by_key: HashMap<String, String> = backend
        .list("import_map")
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(_, f)| {
            let s = |k: &str| f.get(k).and_then(J::as_str).unwrap_or_default();
            (s("source_system") == system
                && s("source_table") == "voucher"
                && s("model") == "accounting_move")
                .then(|| (s("source_id").to_string(), s("record").to_string()))
        })
        .collect();
    file.vouchers
        .iter()
        .enumerate()
        .filter_map(|(i, v)| by_key.get(&voucher_key(ytag, v)).map(|m| (i, m.clone())))
        .collect()
}

#[allow(clippy::too_many_arguments, reason = "one run, one set of inputs")]
fn run(
    env: Env<'_>,
    p: &Principal,
    file: &SieFile,
    (system, ytag): (&str, &str),
    new_accounts: &[(&obop_sie::Account, &'static str)],
    new_journals: &[&str],
    opts: &Options,
    rep: &mut Report,
) -> Result<(), String> {
    let wait = opts.wait_on_rate_limit;
    // Accounts.
    for chunk in new_accounts.chunks(BATCH) {
        let rows = chunk
            .iter()
            .map(|(a, ty)| {
                json!({"code": a.number.to_string(),
                       "name": if a.name.is_empty() { a.number.to_string() } else { a.name.clone() },
                       "account_type": ty,
                       "reconcile": matches!(*ty, "asset_receivable" | "liability_payable")})
            })
            .collect();
        let ids = chunk.iter().map(|(a, _)| a.number.to_string()).collect();
        bulk(
            env,
            p,
            "accounting_account",
            rows,
            Some((system, "account", ids)),
            wait,
        )?;
    }
    // Journals.
    for s in new_journals {
        call(
            env,
            p,
            "accounting_journal.create",
            json!({"name": format!("SIE series {s}"), "code": s, "kind": "general", "active": true}),
        )?;
    }
    let account_id = existing(env, "accounting_account", "code");
    let journal_id = existing(env, "accounting_journal", "code");

    // Moves for the vouchers that are not in the import map yet.
    let have: HashMap<String, String> = moves_after(env, system, ytag, file)
        .into_iter()
        .map(|(i, m)| (voucher_key(ytag, &file.vouchers[i]), m))
        .collect();
    let todo: Vec<&Voucher> = file
        .vouchers
        .iter()
        .filter(|v| !have.contains_key(&voucher_key(ytag, v)))
        .collect();
    for chunk in todo.chunks(BATCH) {
        let rows = chunk
            .iter()
            .map(|v| {
                let mut n = v.text.clone();
                let _ = write!(n, "\nImported from SIE voucher {}", reference(v));
                if let Some(r) = v.registered {
                    let _ = write!(n, ", registered {}", iso(r));
                }
                if let Some(s) = &v.signed {
                    let _ = write!(n, ", signed {s}");
                }
                json!({"journal_id": journal_id[&v.series], "date": iso(v.date),
                       "reference": reference(v), "narration": n.trim_start().to_string()})
            })
            .collect();
        let ids = chunk.iter().map(|v| voucher_key(ytag, v)).collect();
        bulk(
            env,
            p,
            "accounting_move",
            rows,
            Some((system, "voucher", ids)),
            wait,
        )?;
    }
    rep.counts.posted = 0;

    // Lines for every voucher whose move has none, in groups that keep a voucher together.
    let moves: HashMap<String, String> = moves_after(env, system, ytag, file)
        .into_iter()
        .map(|(i, m)| (voucher_key(ytag, &file.vouchers[i]), m))
        .collect();
    let mut with_lines: HashSet<String> = HashSet::new();
    for (_, f) in env
        .store
        .backend()
        .list("accounting_move_line")
        .unwrap_or_default()
    {
        if let Some(m) = f.get("move_id").and_then(J::as_str) {
            with_lines.insert(m.to_string());
        }
    }
    let mut batch: Vec<J> = Vec::new();
    for v in &file.vouchers {
        let Some(mid) = moves.get(&voucher_key(ytag, v)) else {
            return Err(format!(
                "voucher {} has no move after its creation",
                reference(v)
            ));
        };
        if with_lines.contains(mid) {
            continue;
        }
        if batch.len() + v.lines.len() > BATCH && !batch.is_empty() {
            bulk(
                env,
                p,
                "accounting_move_line",
                std::mem::take(&mut batch),
                None,
                wait,
            )?;
        }
        for l in &v.lines {
            let (debit, credit) = split(l.amount_minor);
            let label = l.text.clone().unwrap_or_else(|| v.text.clone());
            batch.push(
                json!({"move_id": mid, "account_id": account_id[&l.account.to_string()],
                              "label": label, "debit": debit, "credit": credit}),
            );
        }
    }
    if !batch.is_empty() {
        bulk(env, p, "accounting_move_line", batch, None, wait)?;
    }

    // Posting, in file order.
    let backend = env.store.backend();
    let mut drafts = 0usize;
    for v in &file.vouchers {
        let mid = &moves[&voucher_key(ytag, v)];
        let state = backend
            .get("accounting_move", mid)
            .ok()
            .flatten()
            .and_then(|f| f.get("state").and_then(J::as_str).map(String::from))
            .unwrap_or_default();
        if state == "posted" {
            continue;
        }
        if opts.post {
            call(env, p, "accounting_move.post", json!({"id": mid})).map_err(|e| {
                format!(
                    "{}: {e} (nothing after it was posted; fix and run again with --post)",
                    reference(v)
                )
            })?;
            rep.counts.posted += 1;
        } else {
            drafts += 1;
        }
    }
    if opts.post {
        rep.notes.push(format!(
            "posted {} move(s) with the real post action as {}, in file order; JE numbers follow that order and carry the year of the posting, not of the voucher date",
            rep.counts.posted, p.id
        ));
    } else if drafts > 0 {
        rep.notes.push(format!(
            "{drafts} move(s) are drafts without a JE number (the SIE number is in `reference`); run again with --post as a person with {APPROVER_GROUP} to post them in file order"
        ));
    }
    Ok(())
}

/// The per account debit and credit totals of the imported moves against the file's.
fn verify(
    env: Env<'_>,
    file: &SieFile,
    moves: &[(usize, String)],
    posted: bool,
) -> Result<String, String> {
    if moves.len() != file.vouchers.len() {
        return Err("verification: not every voucher is in the import map".into());
    }
    let backend = env.store.backend();
    let want_moves: HashMap<&str, &Voucher> = moves
        .iter()
        .map(|(i, m)| (m.as_str(), &file.vouchers[*i]))
        .collect();
    let code_of: HashMap<String, u32> = backend
        .list("accounting_account")
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|(id, f)| Some((id, f.get("code")?.as_str()?.parse().ok()?)))
        .collect();
    let mut got: BTreeMap<u32, (i64, i64)> = BTreeMap::new();
    let mut lines = 0usize;
    for (_, f) in backend
        .list("accounting_move_line")
        .map_err(|e| e.to_string())?
    {
        let Some(m) = f.get("move_id").and_then(J::as_str) else {
            continue;
        };
        if !want_moves.contains_key(m) {
            continue;
        }
        lines += 1;
        let acct = f
            .get("account_id")
            .and_then(J::as_str)
            .and_then(|a| code_of.get(a))
            .ok_or("verification: a line is on an account with no numeric code")?;
        let e = got.entry(*acct).or_default();
        e.0 += f.get("debit").and_then(J::as_i64).unwrap_or(0);
        e.1 += f.get("credit").and_then(J::as_i64).unwrap_or(0);
    }
    let mut want: BTreeMap<u32, (i64, i64)> = BTreeMap::new();
    for l in file.vouchers.iter().flat_map(|v| &v.lines) {
        let (d, c) = split(l.amount_minor);
        let e = want.entry(l.account).or_default();
        e.0 += d;
        e.1 += c;
    }
    let want_lines: usize = file.vouchers.iter().map(|v| v.lines.len()).sum();
    let mut diffs = Vec::new();
    if lines != want_lines {
        diffs.push(format!("{lines} line(s) in obop, {want_lines} in the file"));
    }
    for a in want
        .keys()
        .chain(got.keys())
        .collect::<std::collections::BTreeSet<_>>()
    {
        let (w, g) = (want.get(a), got.get(a));
        if w != g {
            diffs.push(format!(
                "account {a}: file {w:?}, obop {g:?} (debit, credit)"
            ));
        }
    }
    if posted {
        for (m, v) in &want_moves {
            let st = backend
                .get("accounting_move", m)
                .ok()
                .flatten()
                .and_then(|f| f.get("state").and_then(J::as_str).map(String::from))
                .unwrap_or_default();
            if st != "posted" {
                diffs.push(format!("{} is {st}, not posted", reference(v)));
            }
        }
    }
    if !diffs.is_empty() {
        return Err(format!(
            "TRIAL BALANCE DIFFERS from the file ({} difference(s)); first: {}",
            diffs.len(),
            diffs.iter().take(5).cloned().collect::<Vec<_>>().join("; ")
        ));
    }
    let (td, tc) = want
        .values()
        .fold((0i64, 0i64), |a, b| (a.0 + b.0, a.1 + b.1));
    Ok(format!(
        "trial balance equals the file: {} account(s), {} move(s), {lines} line(s), total debit {td} = total credit {tc} (minor units){}",
        want.len(),
        want_moves.len(),
        if posted {
            ", all posted"
        } else {
            ", drafts (not yet posted)"
        }
    ))
}
