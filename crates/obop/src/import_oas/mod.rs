//! `obop import oas --from OAS.db [--documents DIR] [--dry-run] [--post] ...`: the importer of
//! docs/spec/accounting-se.md section 5. It reads an open-accounting-sweden SQLite file
//! read-only and writes obop records through the same commands as REST and MCP, as the person
//! who runs it, so every permission, row rule and limit applies.
//!
//! What it takes: company, accounts, voucher series (as journals), vouchers with their entries,
//! customers and receipts (with the file and its SHA-256). Every other table is counted and
//! listed as skipped with the reason; nothing is dropped silently.
//!
//! Posted history (the hard part, spec 5.2 items 1 and 2): obop starts a move as a draft and the
//! `post` action needs the `accounting.approver` group and numbers the move `JE/{year}/{seq}`.
//! The importer creates every voucher as a draft in OAS order with the OAS series and number in
//! `reference`. With `--post` it then runs the real `accounting_move.post` as the importing
//! person (an agent, or a token narrowed by scopes, is refused), one voucher at a time in OAS
//! order, so the JE numbers follow the OAS order. The `{year}` of a JE number is the year of the
//! posting, not of the voucher date; the voucher date and the OAS number stay on the move.
//! Afterwards the trial balance of the imported moves is compared with the one OAS computes from
//! the same file, and a difference fails the run.
//!
//! Idempotency: the shared import map (`obop_commands::import_map`, `obop import --source-system`,
//! `obop import-map search|get`). Every record the importer makes gets one entry: source system
//! `oas:<16 hex of the company name and organisation number>`, source table = the OAS table
//! (`company`, `account`, `journal`, `voucher`, `customer`, `document`), source id = the OAS id
//! (a voucher's is `<id>#<OAS hash>`, which is how a voucher that changed in the file is noticed).
//! Accounts, journals, parties, vouchers and the company go through `bulk_create` with a
//! `source`, so the record and its entry are written in one transaction; a receipt has a file
//! field, which `bulk_create` does not take, so its entry is written right after the record.
//! Existing accounts and journals that are matched by code get an entry too. A second run
//! creates only what has no entry and refuses if a voucher that was imported has changed in the
//! file. The importer used to keep a private table `oas_import_map`; a run adopts its rows into
//! the shared map ([`adopt_legacy`]) and removes them from the private table.

pub(crate) mod source;
pub(crate) mod types;

use crate::app::App;
use obop_commands::{CallError, Principal, Surface, Value, Vault, import_map};
use serde_json::{Value as J, json};
use source::{Oas, Voucher};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

/// The private ledger of earlier versions; read only to adopt its rows.
pub const LEGACY_MAP_MODEL: &str = "oas_import_map";
pub const RUN_MODEL: &str = "oas_import_run";

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub dry_run: bool,
    pub post: bool,
    pub allow_broken_chain: bool,
    /// Accept the built-in account type table (spec 5.1: the user confirms the mapping).
    pub default_account_types: bool,
    /// `number,type` lines that override or complete the table.
    pub account_types: Option<String>,
    pub documents: Option<String>,
}

/// One imported table in the report.
#[derive(Debug, Default)]
struct Row {
    from: String,
    to: String,
    new: usize,
    matched: usize,
    already: usize,
    note: String,
}

#[derive(Debug, Default)]
pub struct Report {
    dry_run: bool,
    source_name: String,
    rows: Vec<Row>,
    skipped: Vec<(String, i64, String)>,
    dropped: Vec<String>,
    notes: Vec<String>,
    pub problems: Vec<String>,
    account_table: Vec<(i64, String, String, String)>,
    /// Trial balance line, set after a verified run.
    pub verified: Option<String>,
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let verb = if self.dry_run {
            "would create"
        } else {
            "created"
        };
        writeln!(
            f,
            "OAS import{}: {}",
            if self.dry_run {
                " (dry run, nothing written)"
            } else {
                ""
            },
            self.source_name
        )?;
        writeln!(f, "\nimported ({verb}):")?;
        for r in &self.rows {
            write!(
                f,
                "  {:<16} -> {:<26} {} new, {} matched existing, {} already imported",
                r.from, r.to, r.new, r.matched, r.already
            )?;
            if !r.note.is_empty() {
                write!(f, "  ({})", r.note)?;
            }
            writeln!(f)?;
        }
        if !self.account_table.is_empty() {
            writeln!(
                f,
                "\naccount type mapping (UNVERIFIED defaults; confirm or override with --account-types FILE):"
            )?;
            for (n, name, kind, ty) in &self.account_table {
                writeln!(f, "  {n:<6} {kind} -> {ty:<24} {name}")?;
            }
        }
        if !self.skipped.is_empty() {
            writeln!(f, "\nnot imported (counted, never silently dropped):")?;
            for (t, n, why) in &self.skipped {
                writeln!(f, "  {t:<18} {n:>6} row(s)  {why}")?;
            }
        }
        if !self.dropped.is_empty() {
            writeln!(f, "\ncolumns of imported tables that have no obop field:")?;
            for d in &self.dropped {
                writeln!(f, "  {d}")?;
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

/// The reason each table of an OAS file is not imported.
fn skip_reason(table: &str) -> &'static str {
    match table {
        "invoice" | "invoice_line" | "invoice_customer" | "invoiced_entry" => {
            "invoices: the invoicing module has no issue date, due date or VAT rate and numbers at post, so issued invoices (legal numbers F1, F2, ...) do not fit; stage 5"
        }
        "employee" => "employees: not in this importer; identity numbers are confidential_hr data",
        "pay_run" | "pay_line" | "payroll_rate" => "payroll: no payroll package yet (stage 6)",
        "vat_box" | "vat_code" | "vat_report" | "vat_report_line" | "vat_setting" => {
            "VAT configuration: boxes and codes are entered and verified by the approver in accounting_se (UNVERIFIED)"
        }
        "audit_log" => "OAS audit trail: not carried over; obop keeps its own audit and history",
        "bank_transaction" | "bank_booking" | "match_suggestion" | "draft_bank" => {
            "bank lines and matches: not in this importer yet"
        }
        "draft" | "draft_line" | "draft_document" => {
            "unapproved OAS drafts: working state, not books; not carried over"
        }
        "time_entry" | "project" | "activity" => "time tracking: not in this importer yet",
        "app_user" | "session" | "api_token" => {
            "OAS accounts and tokens: credentials are never imported (use `obop admin create`)"
        }
        _ => "unknown table: no mapping",
    }
}

const IMPORTED_TABLES: &[&str] = &[
    "company",
    "account",
    "voucher",
    "entry",
    "customer",
    "document",
    "document_meta",
    "voucher_document",
];

struct Have {
    accounting: bool,
    se: bool,
    documents: bool,
}

fn models_of(app: &App, p: &Principal) -> HashSet<String> {
    app.dispatcher
        .meta(Surface::Cli, p)
        .iter()
        .flat_map(|m| m.models.iter().map(|m| m.name.to_string()))
        .collect()
}

type Ledger = HashMap<(String, String), (String, String)>;

/// The source id of one OAS row: a voucher carries its hash, the rest are the plain id.
fn skey(table: &str, oas_id: &str, hash: &str) -> String {
    if table == "voucher" && !hash.is_empty() {
        format!("{oas_id}#{hash}")
    } else {
        oas_id.to_string()
    }
}

/// The rows of the private ledger of earlier versions that belong to `source`:
/// (table, OAS id, obop model, obop id, hash).
fn legacy_rows(app: &App, source: &str) -> Vec<[String; 5]> {
    let rows = app
        .store
        .backend()
        .find(LEGACY_MAP_MODEL, "source", &J::from(source))
        .unwrap_or_default();
    rows.into_iter()
        .map(|(_, f)| {
            let s = |k: &str| f.get(k).and_then(J::as_str).unwrap_or_default().to_string();
            [s("table"), s("oas_id"), s("model"), s("obop_id"), s("hash")]
        })
        .collect()
}

fn write_entry(
    app: &App,
    by: &str,
    source: &str,
    (table, sid): (&str, &str),
    (model, record): (&str, &str),
) -> Result<(), String> {
    let backend = app.store.backend();
    let id = import_map::entry_id(model, source, table, sid);
    // An entry whose record was deleted is replaced.
    let _ = backend.delete(import_map::MODEL, &id);
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    backend
        .insert(
            import_map::MODEL,
            &id,
            &import_map::entry_fields((model, record), (source, table, sid), secs, by),
        )
        .map_err(|e| format!("cannot write the import map: {e}"))
}

/// One-time adoption: the rows of the private `oas_import_map` of earlier versions become
/// entries of the shared import map (when their record still exists and the key has no entry),
/// then they are removed from the private table. Returns how many entries were written.
fn adopt_legacy(app: &App, by: &str, source: &str) -> Result<usize, String> {
    let backend = app.store.backend();
    let mut n = 0;
    for [table, oas_id, model, record, hash] in legacy_rows(app, source) {
        if backend.get(&model, &record).ok().flatten().is_none() {
            continue;
        }
        let sid = skey(&table, &oas_id, &hash);
        let id = import_map::entry_id(&model, source, &table, &sid);
        if backend.get(import_map::MODEL, &id).ok().flatten().is_none() {
            write_entry(app, by, source, (&table, &sid), (&model, &record))?;
            n += 1;
        }
    }
    for (id, _) in backend
        .find(LEGACY_MAP_MODEL, "source", &J::from(source))
        .unwrap_or_default()
    {
        let _ = backend.delete(LEGACY_MAP_MODEL, &id);
    }
    Ok(n)
}

/// (table, OAS id) -> (obop id, hash) of what the import map holds for `source`, plus, for a
/// dry run, the rows of a private ledger that a real run would adopt.
fn load_ledger(app: &App, source: &str) -> Result<Ledger, String> {
    let backend = app.store.backend();
    let rows = backend
        .find(import_map::MODEL, "source_system", &J::from(source))
        .map_err(|e| format!("cannot read the import map: {e}"))?;
    let mut out = Ledger::new();
    for (_, f) in rows {
        let s = |k: &str| f.get(k).and_then(J::as_str).unwrap_or_default().to_string();
        // An entry whose record was deleted counts as not imported.
        if backend
            .get(&s("model"), &s("record"))
            .ok()
            .flatten()
            .is_none()
        {
            continue;
        }
        let (table, sid) = (s("source_table"), s("source_id"));
        let (id, hash) = match sid.split_once('#') {
            Some((i, h)) if table == "voucher" => (i.to_string(), h.to_string()),
            _ => (sid, String::new()),
        };
        out.insert((table, id), (s("record"), hash));
    }
    for [table, oas_id, model, record, hash] in legacy_rows(app, source) {
        if backend.get(&model, &record).ok().flatten().is_some() {
            out.entry((table, oas_id)).or_insert((record, hash));
        }
    }
    Ok(out)
}

fn source_id(oas: &Oas) -> String {
    let (n, o) = oas
        .company
        .as_ref()
        .map_or(("", ""), |c| (c.name.as_str(), c.org_number.as_str()));
    format!(
        "oas:{}",
        &source::sha256_hex(format!("{n}|{o}").as_bytes())[..16]
    )
}

/// `(field value, record id)` of every record of a model, read straight from the store.
fn existing(app: &App, model: &str, field: &str) -> HashMap<String, String> {
    app.store
        .backend()
        .list(model)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(id, f)| Some((f.get(field)?.as_str()?.to_string(), id)))
        .collect()
}

struct Ctx<'a> {
    app: &'a App,
    p: &'a Principal,
    source: String,
    ledger: Ledger,
}

impl Ctx<'_> {
    fn call(&self, cmd: &str, input: J) -> Result<J, String> {
        match self.app.dispatcher.call(
            Surface::Cli,
            self.p,
            &mut Vault::new(),
            cmd,
            &Value::from_json(&input),
        ) {
            Ok(r) => Ok(r.value.to_json()),
            Err(CallError::Handler(m)) => Err(format!("{cmd}: {m}")),
            Err(CallError::ApprovalRequired { plan_id }) => Err(format!(
                "{cmd} needs a person's approval (plan {plan_id}); run the import as a signed-in person, not an agent"
            )),
            Err(CallError::Forbidden(_)) => Err(format!(
                "{cmd}: this token may not do that (the importer runs as you; it needs the groups of the models it writes)"
            )),
            Err(e) => Err(format!("{cmd}: {e:?}")),
        }
    }

    fn create(&self, model: &str, input: J) -> Result<String, String> {
        let v = self.call(&format!("{model}.create"), input)?;
        v["id"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| format!("{model}.create returned no id"))
    }

    fn done(&self, table: &str, oas_id: &str) -> Option<&(String, String)> {
        self.ledger.get(&(table.to_string(), oas_id.to_string()))
    }

    /// Call a command, waiting out the per-minute limit of `bulk_create` (a refused call
    /// changed nothing, so repeating it is safe).
    fn call_waiting(&self, cmd: &str, input: &J) -> Result<J, String> {
        let mut waited = 0;
        loop {
            match self.call(cmd, input.clone()) {
                Err(e) if e.contains("too many bulk imports") && waited < 5 => {
                    waited += 1;
                    std::thread::sleep(std::time::Duration::from_secs(61));
                }
                r => return r,
            }
        }
    }

    /// Create many records of one model with `bulk_create`, in chunks. With `table` each row
    /// carries its source key (OAS id, hash) and the entry of the import map is written in the
    /// same transaction as the record; without it the rows are plain (the lines of a move).
    /// Returns the ids in the order of `items`.
    fn bulk(
        &mut self,
        model: &str,
        table: Option<&str>,
        items: Vec<(String, String, J)>,
    ) -> Result<Vec<String>, String> {
        let mut out = Vec::with_capacity(items.len());
        for chunk in items.chunks(obop_registry::MAX_BULK_ROWS) {
            let mut input = json!({
                "rows": chunk.iter().map(|c| c.2.clone()).collect::<Vec<_>>(),
                "mode": "all_or_nothing",
            });
            if let Some(t) = table {
                input["source"] = json!({
                    "system": self.source,
                    "table": t,
                    "ids": chunk.iter().map(|c| skey(t, &c.0, &c.1)).collect::<Vec<_>>(),
                });
            }
            let v = self.call_waiting(&format!("{model}.bulk_create"), &input)?;
            let ids: Vec<String> = v["ids"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|i| i.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            if ids.len() != chunk.len() {
                return Err(format!(
                    "{model}.bulk_create returned {} ids for {} rows",
                    ids.len(),
                    chunk.len()
                ));
            }
            if let Some(t) = table {
                for ((oas_id, hash, _), id) in chunk.iter().zip(&ids) {
                    self.ledger
                        .insert((t.to_string(), oas_id.clone()), (id.clone(), hash.clone()));
                }
            }
            out.extend(ids);
        }
        Ok(out)
    }

    /// Record a source key for a record that `bulk_create` did not make (a receipt, or an
    /// existing account or journal that was matched by its code).
    fn remember(
        &mut self,
        table: &str,
        oas_id: &str,
        model: &str,
        obop_id: &str,
        hash: &str,
    ) -> Result<(), String> {
        write_entry(
            self.app,
            &self.p.id,
            &self.source,
            (table, &skey(table, oas_id, hash)),
            (model, obop_id),
        )?;
        self.ledger.insert(
            (table.to_string(), oas_id.to_string()),
            (obop_id.to_string(), hash.to_string()),
        );
        Ok(())
    }
}

fn nonempty(mut m: serde_json::Map<String, J>) -> J {
    m.retain(|_, v| !matches!(v, J::String(s) if s.is_empty()));
    J::Object(m)
}

fn obj(pairs: &[(&str, J)]) -> serde_json::Map<String, J> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect()
}

/// What happens to one OAS account.
enum AccAction {
    Already,
    Matched(String),
    Create(&'static str),
    NoType,
}

/// Read the files of the receipts and check each hash and size against the file; the problems.
fn check_documents(oas: &Oas, dir: Option<&str>, ledger: &Ledger) -> Vec<String> {
    let mut out = vec![];
    let Some(dir) = dir else { return out };
    let dir = Path::new(dir);
    if !dir.is_dir() {
        return vec![format!("--documents {} is not a directory", dir.display())];
    }
    for d in &oas.documents {
        if ledger.contains_key(&("document".to_string(), d.id.to_string())) {
            continue;
        }
        let Some(path) = source::document_path(dir, &d.sha256) else {
            out.push(format!(
                "receipt {} (sha256 {}): no file under {}",
                d.filename,
                d.sha256.get(..12).unwrap_or(&d.sha256),
                dir.display()
            ));
            continue;
        };
        match std::fs::read(&path) {
            Err(e) => out.push(format!(
                "receipt {}: cannot read {}: {e}",
                d.filename,
                path.display()
            )),
            Ok(bytes) => {
                let got = source::sha256_hex(&bytes);
                if got != d.sha256 {
                    out.push(format!(
                        "receipt {}: SHA-256 MISMATCH, the file {} hashes to {} but OAS recorded {}; refused",
                        d.filename,
                        path.display(),
                        got,
                        d.sha256
                    ));
                } else if i64::try_from(bytes.len()).ok() != Some(d.size) {
                    out.push(format!(
                        "receipt {}: the file has {} bytes, OAS recorded {}; refused",
                        d.filename,
                        bytes.len(),
                        d.size
                    ));
                }
            }
        }
    }
    out
}

fn narration(v: &Voucher, corrected_ref: Option<String>) -> String {
    let mut s = v.description.clone();
    if !v.counterparty.is_empty() {
        let _ = write!(s, "\nCounterparty: {}", v.counterparty);
    }
    let _ = write!(
        s,
        "\nImported from OAS voucher {}, compiled {}, created by {}, approved by {}",
        v.reference(),
        v.compile_date,
        v.created_by,
        v.approved_by
    );
    if let Some(r) = corrected_ref {
        let _ = write!(s, "\nCorrects OAS voucher {r}");
    }
    if !v.documents.is_empty() {
        let docs: Vec<String> = v
            .documents
            .iter()
            .map(|(f, h)| format!("{f} (sha256 {})", h.get(..12).unwrap_or(h)))
            .collect();
        let _ = write!(s, "\nReceipts: {}", docs.join(", "));
    }
    s
}

#[allow(
    clippy::result_large_err,
    reason = "the report is the answer in both cases: a refused run still shows what it found"
)]
/// The importer. `oas_path` is opened read-only; nothing is written when `opts.dry_run`.
pub fn import(app: &App, p: &Principal, oas_path: &Path, opts: &Options) -> Result<Report, Report> {
    let mut rep = Report {
        dry_run: opts.dry_run,
        source_name: oas_path.display().to_string(),
        ..Report::default()
    };
    let conn = match source::open(oas_path) {
        Ok(c) => c,
        Err(e) => {
            rep.problems.push(e);
            return Err(rep);
        }
    };
    let oas = match source::read(&conn) {
        Ok(o) => o,
        Err(e) => {
            rep.problems.push(e);
            return Err(rep);
        }
    };
    let source = source_id(&oas);
    let models = models_of(app, p);
    let have = Have {
        accounting: [
            "accounting_account",
            "accounting_journal",
            "accounting_move",
            "accounting_move_line",
        ]
        .iter()
        .all(|m| models.contains(*m)),
        se: models.contains("accounting_se_company"),
        documents: models.contains("documents_document"),
    };
    if !opts.dry_run {
        match adopt_legacy(app, &p.id, &source) {
            Ok(0) => {}
            Ok(n) => rep.notes.push(format!(
                "adopted {n} row(s) of the private oas_import_map ledger of an earlier version into the shared import map"
            )),
            Err(e) => {
                rep.problems.push(e);
                return Err(rep);
            }
        }
    }
    let ledger = match load_ledger(app, &source) {
        Ok(l) => l,
        Err(e) => {
            rep.problems.push(e);
            return Err(rep);
        }
    };
    let mut cx = Ctx {
        app,
        p,
        source,
        ledger,
    };

    // ---- inventory: what is skipped, what is dropped --------------------------------------
    for (t, n) in &oas.tables {
        if IMPORTED_TABLES.contains(&t.as_str()) {
            continue;
        }
        let mut why = skip_reason(t).to_string();
        if t == "invoice" {
            let by: Vec<String> = oas
                .invoices_by_status
                .iter()
                .map(|(s, n)| format!("{n} {s}"))
                .collect();
            let _ = write!(why, " [{}]", by.join(", "));
        }
        if t == "draft" && *n > 0 {
            why.push_str(" [unapproved drafts are lost unless approved in OAS first]");
        }
        rep.skipped.push((t.clone(), *n, why));
    }
    if let Some(c) = &oas.company {
        for (col, set) in &c.dropped {
            rep.dropped.push(format!(
                "company.{col}{}",
                if *set {
                    " (has a value; not copied)"
                } else {
                    " (empty)"
                }
            ));
        }
    }
    if oas.vouchers.iter().any(|v| v.corrects.is_some()) {
        let n = oas.vouchers.iter().filter(|v| v.corrects.is_some()).count();
        rep.notes.push(format!(
            "{n} correcting voucher(s): kept as ordinary moves with 'Corrects OAS voucher X' in the narration; the corrected voucher is not marked reversed (the totals are the same)"
        ));
    }
    rep.dropped.push(
        "voucher.prev_hash and hash (the OAS chain is verified, then dropped; the last hash of each series is kept in the import record)".into(),
    );
    rep.dropped.push(
        "entry.id, entry.line_no (the order of the lines is kept), voucher.id (kept in the import map only, as part of the source id)".into(),
    );
    let linked: i64 = oas.tables.get("voucher_document").copied().unwrap_or(0);
    if linked > 0 {
        rep.notes.push(format!(
            "{linked} voucher-to-receipt link(s): folded into the move narration (file name and hash prefix); documents_document has no link field"
        ));
    }
    if oas.customers.iter().any(|c| {
        !c.org_number.is_empty()
            || !c.vat_number.is_empty()
            || !c.address.is_empty()
            || c.country != "SE"
    }) {
        rep.notes.push(
            "customers: org number, VAT number, address and country go into party.notes (party has no fields for them)".into(),
        );
    }

    // ---- preflight ------------------------------------------------------------------------
    let mut problems: Vec<String> = vec![];
    if !have.accounting {
        problems.push(
            "the accounting package is not installed here (obop modules install-from accounting)"
                .into(),
        );
    }
    let check = source::check(&oas.vouchers);
    let chain_msg = if check.chain.is_empty() {
        format!(
            "chain verified: {} voucher(s) in {} series",
            oas.vouchers.len(),
            check.tips.len()
        )
    } else if opts.allow_broken_chain {
        format!(
            "chain BROKEN ({} problem(s)), continuing because --allow-broken-chain was given",
            check.chain.len()
        )
    } else {
        problems.extend(
            check
                .chain
                .iter()
                .map(|c| format!("OAS chain: {c} (--allow-broken-chain continues anyway)")),
        );
        String::new()
    };
    if !chain_msg.is_empty() {
        rep.notes.push(chain_msg);
    }
    problems.extend(check.unsound.iter().map(|u| format!("OAS voucher {u}")));
    let known: HashSet<i64> = oas.accounts.iter().map(|a| a.number).collect();
    for v in &oas.vouchers {
        for (a, ..) in &v.lines {
            if !known.contains(a) {
                problems.push(format!(
                    "{}: account {a} is not in the account table",
                    v.reference()
                ));
            }
        }
    }
    // A voucher that was imported must be what the file still says.
    for v in &oas.vouchers {
        if let Some((_, h)) = cx.done("voucher", &v.id.to_string())
            && *h != v.hash
        {
            problems.push(format!(
                "{} was imported with hash {} but the file now says {}: the OAS history changed",
                v.reference(),
                h.get(..12).unwrap_or(h),
                v.hash.get(..12).unwrap_or(&v.hash)
            ));
        }
    }
    let in_file: HashSet<String> = oas.vouchers.iter().map(|v| v.id.to_string()).collect();
    for (t, id) in cx.ledger.keys() {
        if t == "voucher" && !in_file.contains(id) {
            problems.push(format!(
                "voucher id {id} was imported but is no longer in the file"
            ));
        }
    }
    if opts.post && !(p.is_person() && p.allows("accounting.approver")) {
        problems.push(
            "--post runs the real post action, which only a signed-in person with the accounting.approver group may do (not an agent, not a scoped token)".into(),
        );
    }
    // Account types.
    let overrides = match &opts.account_types {
        Some(text) => match types::parse_overrides(text) {
            Ok(o) => o,
            Err(e) => {
                problems.push(e);
                BTreeMap::new()
            }
        },
        None => BTreeMap::new(),
    };
    let existing_accounts = existing(app, "accounting_account", "code");
    let mut actions: Vec<(&source::Account, AccAction)> = vec![];
    let mut missing_types = vec![];
    for a in &oas.accounts {
        let act = if cx.done("account", &a.number.to_string()).is_some() {
            AccAction::Already
        } else if let Some(id) = existing_accounts.get(&a.number.to_string()) {
            AccAction::Matched(id.clone())
        } else {
            let ty = overrides.get(&a.number).copied().or_else(|| {
                opts.default_account_types
                    .then(|| types::default_type(&a.kind, a.number))
                    .flatten()
            });
            match ty {
                Some(t) => AccAction::Create(t),
                None => {
                    missing_types.push(a.number);
                    AccAction::NoType
                }
            }
        };
        if matches!(act, AccAction::Create(_) | AccAction::NoType) || opts.dry_run {
            let shown = match &act {
                AccAction::Create(t) => (*t).to_string(),
                AccAction::NoType => types::default_type(&a.kind, a.number)
                    .map_or("(needs a type)".to_string(), |t| {
                        format!("{t} (default, not accepted)")
                    }),
                AccAction::Already => "already imported".into(),
                AccAction::Matched(_) => "matched existing".into(),
            };
            rep.account_table
                .push((a.number, a.name.clone(), a.kind.clone(), shown));
        }
        actions.push((a, act));
    }
    if !missing_types.is_empty() {
        problems.push(format!(
            "{} account(s) have no confirmed type: confirm the table above with --default-account-types, or give --account-types FILE (lines number,type); first: {:?}",
            missing_types.len(),
            &missing_types[..missing_types.len().min(8)]
        ));
    }
    problems.extend(check_documents(&oas, opts.documents.as_deref(), &cx.ledger));
    if let Some(c) = &oas.company
        && have.se
        && cx.done("company", "1").is_none()
        && c.name.is_empty()
    {
        problems.push("the company has no name".into());
    }
    // A move that exists for a voucher the import map does not know (an interrupted run).
    let journals_now = existing(app, "accounting_journal", "code");
    let mut moves_now: HashSet<(String, String)> = HashSet::new();
    if have.accounting {
        for (_, f) in app
            .store
            .backend()
            .list("accounting_move")
            .unwrap_or_default()
        {
            if let (Some(j), Some(r)) = (
                f.get("journal_id").and_then(J::as_str),
                f.get("reference").and_then(J::as_str),
            ) {
                moves_now.insert((j.to_string(), r.to_string()));
            }
        }
        for v in &oas.vouchers {
            if cx.done("voucher", &v.id.to_string()).is_some() {
                continue;
            }
            let jid = cx
                .done("journal", &v.series)
                .map(|x| x.0.clone())
                .or_else(|| journals_now.get(&v.series).cloned());
            if let Some(j) = jid
                && moves_now.contains(&(j, v.reference()))
            {
                problems.push(format!(
                    "a move with reference {} already exists in journal {} but is not in the import map (an interrupted run or a hand entry): delete or rename it, then run again",
                    v.reference(),
                    v.series
                ));
            }
        }
    }
    if !problems.is_empty() {
        rep.problems = problems;
        // The rows still tell what a run would do, but nothing is written.
        fill_dry_rows(&oas, &cx, &actions, &have, opts, &mut rep);
        return Err(rep);
    }

    if opts.dry_run {
        fill_dry_rows(&oas, &cx, &actions, &have, opts, &mut rep);
        return Ok(rep);
    }

    // ---- the run ---------------------------------------------------------------------------
    let mut failures: Vec<String> = vec![];
    if let Err(e) = run(
        &mut cx,
        &oas,
        &actions,
        &have,
        opts,
        &check,
        &mut rep,
        &mut failures,
    ) {
        rep.problems.push(e);
        rep.problems.extend(failures);
        return Err(rep);
    }
    // Verification against OAS.
    match verify(&cx, &oas, &conn, opts.post) {
        Ok(s) => rep.verified = Some(s),
        Err(e) => {
            rep.problems.push(e);
            rep.problems.extend(failures);
            return Err(rep);
        }
    }
    if failures.is_empty() {
        Ok(rep)
    } else {
        rep.problems.extend(failures);
        Err(rep)
    }
}

/// The report of a dry run (and of a refused run): counts from the import map and the target.
fn fill_dry_rows(
    oas: &Oas,
    cx: &Ctx<'_>,
    actions: &[(&source::Account, AccAction)],
    have: &Have,
    opts: &Options,
    rep: &mut Report,
) {
    if let Some(_c) = &oas.company {
        let mut r = Row {
            from: "company".into(),
            to: "accounting_se_company".into(),
            ..Row::default()
        };
        if !have.se {
            r.note = "accounting_se not installed: skipped".into();
        } else if cx.done("company", "1").is_some() {
            r.already = 1;
        } else {
            r.new = 1;
        }
        rep.rows.push(r);
    }
    let mut r = Row {
        from: "account".into(),
        to: "accounting_account".into(),
        ..Row::default()
    };
    for (_, a) in actions {
        match a {
            AccAction::Already => r.already += 1,
            AccAction::Matched(_) => r.matched += 1,
            AccAction::Create(_) | AccAction::NoType => r.new += 1,
        }
    }
    rep.rows.push(r);
    let series: BTreeMap<&str, ()> = oas
        .vouchers
        .iter()
        .map(|v| (v.series.as_str(), ()))
        .collect();
    let existing_j = existing(cx.app, "accounting_journal", "code");
    let mut r = Row {
        from: "voucher series".into(),
        to: "accounting_journal".into(),
        ..Row::default()
    };
    for s in series.keys() {
        if cx.done("journal", s).is_some() {
            r.already += 1;
        } else if existing_j.contains_key(*s) {
            r.matched += 1;
        } else {
            r.new += 1;
        }
    }
    rep.rows.push(r);
    let mut r = Row {
        from: "voucher+entry".into(),
        to: "accounting_move(+_line)".into(),
        note: format!(
            "{} line(s); {}",
            oas.vouchers.iter().map(|v| v.lines.len()).sum::<usize>(),
            if opts.post {
                "then posted in OAS order"
            } else {
                "left as drafts; use --post to post them in OAS order"
            }
        ),
        ..Row::default()
    };
    for v in &oas.vouchers {
        if cx.done("voucher", &v.id.to_string()).is_some() {
            r.already += 1;
        } else {
            r.new += 1;
        }
    }
    rep.rows.push(r);
    if !oas.customers.is_empty() {
        let mut r = Row {
            from: "customer".into(),
            to: "party".into(),
            ..Row::default()
        };
        for c in &oas.customers {
            if cx.done("customer", &c.id.to_string()).is_some() {
                r.already += 1;
            } else {
                r.new += 1;
            }
        }
        rep.rows.push(r);
    }
    if !oas.documents.is_empty() {
        let mut r = Row {
            from: "document".into(),
            to: "documents_document".into(),
            ..Row::default()
        };
        if !have.documents {
            r.note = "documents package not installed: skipped".into();
        } else if opts.documents.is_none() {
            r.note = "no --documents DIR: not imported".into();
        } else {
            for d in &oas.documents {
                if cx.done("document", &d.id.to_string()).is_some() {
                    r.already += 1;
                } else {
                    r.new += 1;
                }
            }
        }
        rep.rows.push(r);
    }
}

#[allow(clippy::too_many_arguments)]
fn run(
    cx: &mut Ctx<'_>,
    oas: &Oas,
    actions: &[(&source::Account, AccAction)],
    have: &Have,
    opts: &Options,
    check: &source::Check,
    rep: &mut Report,
    failures: &mut Vec<String>,
) -> Result<(), String> {
    // Company.
    if let Some(c) = &oas.company {
        let mut r = Row {
            from: "company".into(),
            to: "accounting_se_company".into(),
            ..Row::default()
        };
        if !have.se {
            r.note = "accounting_se not installed: skipped".into();
        } else if cx.done("company", "1").is_some() {
            r.already = 1;
        } else {
            cx.bulk(
                "accounting_se_company",
                Some("company"),
                vec![(
                    "1".into(),
                    String::new(),
                    nonempty(obj(&[
                        ("name", json!(c.name)),
                        ("org_number", json!(c.org_number)),
                        ("vat_number", json!(c.vat_number)),
                        ("f_skatt", json!(c.f_skatt)),
                        ("fiscal_year_start_month", json!(1)),
                        ("vat_period", json!("quarterly")),
                        ("currency", json!("SEK")),
                        ("address", json!(c.address)),
                        ("contact_email", json!(c.contact_email)),
                    ])),
                )],
            )?;
            r.new = 1;
            rep.notes.push(
                "company: OAS has no fiscal year start or VAT period; set to month 1 and quarterly, CONFIRM them in accounting_se_company".into(),
            );
        }
        rep.rows.push(r);
    }
    // Accounts.
    let mut acc_id: HashMap<i64, String> = HashMap::new();
    let mut r = Row {
        from: "account".into(),
        to: "accounting_account".into(),
        ..Row::default()
    };
    let mut new_accounts: Vec<(i64, String, J)> = vec![];
    for (a, act) in actions {
        let key = a.number.to_string();
        match act {
            AccAction::Already => {
                acc_id.insert(
                    a.number,
                    cx.done("account", &key)
                        .map(|x| x.0.clone())
                        .unwrap_or_default(),
                );
                r.already += 1;
            }
            AccAction::Matched(id) => {
                cx.remember("account", &key, "accounting_account", id, "")?;
                acc_id.insert(a.number, id.clone());
                r.matched += 1;
            }
            AccAction::Create(ty) => {
                new_accounts.push((
                    a.number,
                    key.clone(),
                    json!({"code": key, "name": a.name, "account_type": ty,
                           "reconcile": matches!(*ty, "asset_receivable" | "liability_payable")}),
                ));
                r.new += 1;
            }
            AccAction::NoType => return Err(format!("account {} has no type", a.number)),
        }
    }
    let ids = cx.bulk(
        "accounting_account",
        Some("account"),
        new_accounts
            .iter()
            .map(|(_, k, j)| (k.clone(), String::new(), j.clone()))
            .collect(),
    )?;
    for ((n, ..), id) in new_accounts.iter().zip(ids) {
        acc_id.insert(*n, id);
    }
    rep.rows.push(r);
    // Journals, one per series.
    let existing_j = existing(cx.app, "accounting_journal", "code");
    let mut journal: HashMap<String, String> = HashMap::new();
    let mut r = Row {
        from: "voucher series".into(),
        to: "accounting_journal".into(),
        ..Row::default()
    };
    let series: BTreeMap<&str, ()> = oas
        .vouchers
        .iter()
        .map(|v| (v.series.as_str(), ()))
        .collect();
    let mut new_journals: Vec<(String, J)> = vec![];
    for s in series.keys() {
        if let Some((id, _)) = cx.done("journal", s) {
            journal.insert((*s).to_string(), id.clone());
            r.already += 1;
        } else if let Some(id) = existing_j.get(*s) {
            let id = id.clone();
            cx.remember("journal", s, "accounting_journal", &id, "")?;
            journal.insert((*s).to_string(), id);
            r.matched += 1;
        } else {
            new_journals.push((
                (*s).to_string(),
                json!({"name": format!("OAS series {s}"), "code": s, "kind": "general", "active": true}),
            ));
            r.new += 1;
        }
    }
    let ids = cx.bulk(
        "accounting_journal",
        Some("journal"),
        new_journals
            .iter()
            .map(|(s, j)| (s.clone(), String::new(), j.clone()))
            .collect(),
    )?;
    for ((s, _), id) in new_journals.iter().zip(ids) {
        journal.insert(s.clone(), id);
    }
    rep.rows.push(r);
    // Customers.
    if !oas.customers.is_empty() {
        let mut r = Row {
            from: "customer".into(),
            to: "party".into(),
            ..Row::default()
        };
        let mut new_customers: Vec<(String, String, J)> = vec![];
        for c in &oas.customers {
            if cx.done("customer", &c.id.to_string()).is_some() {
                r.already += 1;
                continue;
            }
            let mut notes = String::new();
            for (k, v) in [
                ("Org number", &c.org_number),
                ("VAT number", &c.vat_number),
                ("Address", &c.address),
            ] {
                if !v.is_empty() {
                    let _ = writeln!(notes, "{k}: {v}");
                }
            }
            let _ = write!(
                notes,
                "Country: {}\nImported from OAS customer {}",
                c.country, c.id
            );
            new_customers.push((
                c.id.to_string(),
                String::new(),
                nonempty(obj(&[
                    ("name", json!(c.name)),
                    ("email", json!(c.email)),
                    ("notes", json!(notes)),
                ])),
            ));
            r.new += 1;
        }
        cx.bulk("party", Some("customer"), new_customers)?;
        rep.rows.push(r);
    }
    // Receipts.
    if !oas.documents.is_empty() {
        let mut r = Row {
            from: "document".into(),
            to: "documents_document".into(),
            ..Row::default()
        };
        match (&opts.documents, have.documents) {
            (_, false) => r.note = "documents package not installed: skipped".into(),
            (None, _) => r.note = "no --documents DIR: not imported".into(),
            (Some(dir), true) => {
                let svc = cx
                    .app
                    .dispatcher
                    .blob_service()
                    .ok_or("the blob service is not available")?;
                // A bulk import is not a burst of abuse.
                svc.set_upload_limit(usize::MAX / 2);
                for d in &oas.documents {
                    if cx.done("document", &d.id.to_string()).is_some() {
                        r.already += 1;
                        continue;
                    }
                    let one = || -> Result<String, String> {
                        let path = source::document_path(Path::new(dir), &d.sha256)
                            .ok_or("file vanished after the check")?;
                        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                        // Hashed again between the check and the upload.
                        if source::sha256_hex(&bytes) != d.sha256 {
                            return Err("the file changed after it was checked; refused".into());
                        }
                        let blob = svc
                            .upload(
                                cx.p,
                                "documents_document.attachment",
                                &d.mime,
                                &d.filename,
                                &bytes,
                            )
                            .map_err(|e| format!("{e:?}"))?;
                        let mut summary = String::new();
                        if !d.vendor.is_empty() {
                            let _ = writeln!(summary, "Vendor: {}", d.vendor);
                        }
                        if let Some(a) = d.amount {
                            let _ = writeln!(summary, "Amount: {a} minor units {}", d.currency);
                        }
                        if let Some(dt) = &d.doc_date {
                            let _ = writeln!(summary, "Document date: {dt}");
                        }
                        if !d.notes.is_empty() {
                            let _ = writeln!(summary, "Notes: {}", d.notes);
                        }
                        let _ = write!(summary, "SHA-256 {}", d.sha256);
                        cx.create(
                            "documents_document",
                            json!({"title": d.filename, "kind": "other", "confidentiality": "internal",
                                   "reference": format!("oas-doc-{}", d.id),
                                   "attachment": blob.id, "summary": summary}),
                        )
                    };
                    match one() {
                        Ok(id) => {
                            cx.remember(
                                "document",
                                &d.id.to_string(),
                                "documents_document",
                                &id,
                                &d.sha256,
                            )?;
                            r.new += 1;
                        }
                        Err(e) => failures.push(format!("receipt {}: {e}", d.filename)),
                    }
                }
            }
        }
        rep.rows.push(r);
    }
    // Vouchers in OAS order.
    let by_id: HashMap<i64, &Voucher> = oas.vouchers.iter().map(|v| (v.id, v)).collect();
    let mut r = Row {
        from: "voucher+entry".into(),
        to: "accounting_move(+_line)".into(),
        ..Row::default()
    };
    let mut lines_made = 0usize;
    // The moves, with their source keys, in one call per 1,000 vouchers.
    let mut new_moves: Vec<(String, String, J)> = vec![];
    for v in &oas.vouchers {
        if cx.done("voucher", &v.id.to_string()).is_some() {
            r.already += 1;
            continue;
        }
        let corrected = v
            .corrects
            .and_then(|c| by_id.get(&c))
            .map(|c| c.reference());
        new_moves.push((
            v.id.to_string(),
            v.hash.clone(),
            json!({"journal_id": journal[&v.series], "date": v.event_date,
                   "reference": v.reference(), "narration": narration(v, corrected)}),
        ));
        r.new += 1;
    }
    cx.bulk("accounting_move", Some("voucher"), new_moves)?;
    // The lines of every move that has none (the new ones, and the ones of an interrupted run,
    // whose moves are in the import map but whose lines were not written).
    let mut have_lines: HashMap<String, usize> = HashMap::new();
    for (_, f) in cx
        .app
        .store
        .backend()
        .list("accounting_move_line")
        .map_err(|e| e.to_string())?
    {
        if let Some(m) = f.get("move_id").and_then(J::as_str) {
            *have_lines.entry(m.to_string()).or_default() += 1;
        }
    }
    let mut new_lines: Vec<(String, String, J)> = vec![];
    for v in &oas.vouchers {
        let Some((mid, _)) = cx.done("voucher", &v.id.to_string()).cloned() else {
            continue;
        };
        let have = have_lines.get(&mid).copied().unwrap_or(0);
        if have == v.lines.len() {
            continue;
        }
        if have != 0 {
            return Err(format!(
                "{}: the move has {have} line(s) but OAS has {}; delete the move's lines or the move by hand and run again",
                v.reference(),
                v.lines.len()
            ));
        }
        for (acct, debit, credit, vat) in &v.lines {
            let label = match vat {
                Some(c) if !c.is_empty() => format!("{} [{c}]", v.description),
                _ => v.description.clone(),
            };
            new_lines.push((
                String::new(),
                String::new(),
                json!({"move_id": mid, "account_id": acc_id[acct], "label": label,
                       "debit": debit, "credit": credit}),
            ));
        }
    }
    lines_made += new_lines.len();
    cx.bulk("accounting_move_line", None, new_lines)?;
    r.note = format!("{lines_made} line(s) created this run");
    rep.rows.push(r);

    // Posting, in OAS order.
    let mut posted = 0usize;
    let mut drafts = 0usize;
    let backend = cx.app.store.backend();
    for v in &oas.vouchers {
        let Some((mid, _)) = cx.done("voucher", &v.id.to_string()).cloned() else {
            continue;
        };
        let state = backend
            .get("accounting_move", &mid)
            .ok()
            .flatten()
            .and_then(|f| f.get("state").and_then(J::as_str).map(String::from))
            .unwrap_or_default();
        if state == "posted" {
            continue;
        }
        if opts.post {
            cx.call("accounting_move.post", json!({"id": mid}))
                .map_err(|e| {
                    format!(
                        "{}: {e} (nothing after it was posted; fix and run again with --post)",
                        v.reference()
                    )
                })?;
            posted += 1;
        } else {
            drafts += 1;
        }
    }
    if opts.post {
        rep.notes.push(format!(
            "posted {posted} move(s) with the real post action as {}, in OAS voucher order; JE numbers follow that order and carry the year of the posting, not of the voucher date",
            cx.p.id
        ));
    } else if drafts > 0 {
        rep.notes.push(format!(
            "{drafts} move(s) are drafts without a JE number (the OAS number is in `reference`); run again with --post as a person with accounting.approver to post them in OAS order"
        ));
    }
    // The import record: where the origin can be shown from later. A run that changed nothing
    // leaves none.
    if lines_made == 0 && posted == 0 {
        return Ok(());
    }
    let tips = serde_json::to_string(&check.tips).unwrap_or_default();
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    cx.app.store.insert(
        RUN_MODEL,
        [
            ("source", Value::text(&cx.source)),
            ("chain_tips", Value::text(&tips)),
            ("vouchers", Value::Int(oas.vouchers.len() as i64)),
            ("posted", Value::Bool(opts.post)),
            ("run_at", Value::Int(secs as i64)),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect(),
    )?;
    Ok(())
}

/// The trial balance of the imported moves against the one OAS computes from the file. Any
/// difference is an error.
fn verify(
    cx: &Ctx<'_>,
    oas: &Oas,
    conn: &rusqlite::Connection,
    posted: bool,
) -> Result<String, String> {
    let expected = source::trial_balance(conn)?;
    let backend = cx.app.store.backend();
    let moves: HashMap<String, &Voucher> = oas
        .vouchers
        .iter()
        .filter_map(|v| {
            cx.done("voucher", &v.id.to_string())
                .map(|(m, _)| (m.clone(), v))
        })
        .collect();
    if moves.len() != oas.vouchers.len() {
        return Err("verification: not every voucher is in the import map".into());
    }
    let code_of: HashMap<String, String> = backend
        .list("accounting_account")
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|(id, f)| Some((id, f.get("code")?.as_str()?.to_string())))
        .collect();
    let mut got: BTreeMap<i64, (i64, i64)> = BTreeMap::new();
    let mut lines = 0usize;
    for (_, f) in backend
        .list("accounting_move_line")
        .map_err(|e| e.to_string())?
    {
        let Some(m) = f.get("move_id").and_then(J::as_str) else {
            continue;
        };
        if !moves.contains_key(m) {
            continue;
        }
        lines += 1;
        let acct: i64 = f
            .get("account_id")
            .and_then(J::as_str)
            .and_then(|a| code_of.get(a))
            .and_then(|c| c.parse().ok())
            .ok_or("verification: a line is on an account with no numeric code")?;
        let e = got.entry(acct).or_default();
        e.0 += f.get("debit").and_then(J::as_i64).unwrap_or(0);
        e.1 += f.get("credit").and_then(J::as_i64).unwrap_or(0);
    }
    let want_lines: usize = oas.vouchers.iter().map(|v| v.lines.len()).sum();
    let mut diffs = vec![];
    if lines != want_lines {
        diffs.push(format!("{lines} line(s) in obop, {want_lines} in OAS"));
    }
    for a in expected
        .keys()
        .chain(got.keys())
        .collect::<std::collections::BTreeSet<_>>()
    {
        let (e, g) = (expected.get(a), got.get(a));
        if e != g {
            diffs.push(format!(
                "account {a}: OAS {e:?}, obop {g:?} (debit, credit)"
            ));
        }
    }
    if posted {
        for (m, v) in &moves {
            let st = backend
                .get("accounting_move", m)
                .ok()
                .flatten()
                .and_then(|f| f.get("state").and_then(J::as_str).map(String::from))
                .unwrap_or_default();
            if st != "posted" {
                diffs.push(format!("{} is {st}, not posted", v.reference()));
            }
        }
    }
    if !diffs.is_empty() {
        return Err(format!(
            "TRIAL BALANCE DIFFERS from OAS ({} difference(s)); first: {}",
            diffs.len(),
            diffs.iter().take(5).cloned().collect::<Vec<_>>().join("; ")
        ));
    }
    let (td, tc) = expected
        .values()
        .fold((0i64, 0i64), |a, b| (a.0 + b.0, a.1 + b.1));
    Ok(format!(
        "trial balance equals OAS: {} account(s), {} move(s), {lines} line(s), total debit {td} = total credit {tc} (minor units){}",
        expected.len(),
        moves.len(),
        if posted {
            ", all posted"
        } else {
            ", drafts (not yet posted)"
        }
    ))
}

// ---- the command --------------------------------------------------------------------------

pub struct Args<'a> {
    pub from: &'a str,
    pub documents: Option<&'a str>,
    pub dry_run: bool,
    pub post: bool,
    pub allow_broken_chain: bool,
    pub default_account_types: bool,
    pub account_types: Option<&'a str>,
}

pub fn run_cli(o: &super::Opts, a: &Args) -> super::Exit {
    let account_types = match a.account_types {
        Some(f) => Some(
            std::fs::read_to_string(f)
                .map_err(|e| super::fail(&format!("cannot read {f}: {e}")))?,
        ),
        None => None,
    };
    // The demo seed would put fake contacts into the database that receives real books.
    let app = super::open_app_with(o, false)?;
    let p = super::principal(&app, o)?;
    let opts = Options {
        dry_run: a.dry_run,
        post: a.post,
        allow_broken_chain: a.allow_broken_chain,
        default_account_types: a.default_account_types,
        account_types,
        documents: a.documents.map(String::from),
    };
    match import(&app, &p, Path::new(a.from), &opts) {
        Ok(rep) => {
            print!("{rep}");
            Ok(())
        }
        Err(rep) => {
            print!("{rep}");
            Err(super::fail(if a.dry_run {
                "the dry run found problems; nothing was written"
            } else {
                "the import did not complete (see PROBLEMS); a run again resumes where it stopped"
            }))
        }
    }
}
