//! The OAS importer against a generated sample database: invented accounts and companies, built
//! here with rusqlite, no real data.

use crate::app::App;
use crate::import_oas::source::{Voucher, canonical, sha256_hex};
use crate::import_oas::{MAP_MODEL, Options, Report, import};
use obop_commands::{ADMIN_GROUP, Principal};
use obop_pkg::{Bundled, Marketplace};
use rusqlite::{Connection, params};
use serde_json::Value as J;
use std::path::{Path, PathBuf};

/// 2026-09-14.
const SEPTEMBER_2026: i64 = 1_789_400_000;

struct Dir(PathBuf);

impl Dir {
    fn new() -> Self {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("obop-oas-test-{}-{n}-{t}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        Dir(p)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const SCHEMA: &str = "
CREATE TABLE company(id INTEGER PRIMARY KEY CHECK (id = 1), name TEXT NOT NULL, org_number TEXT NOT NULL DEFAULT '',
  contact_name TEXT NOT NULL DEFAULT '', contact_phone TEXT NOT NULL DEFAULT '', contact_email TEXT NOT NULL DEFAULT '',
  address TEXT NOT NULL DEFAULT '', vat_number TEXT NOT NULL DEFAULT '', payment_info TEXT NOT NULL DEFAULT '',
  f_skatt INTEGER NOT NULL DEFAULT 0);
CREATE TABLE account(number INTEGER PRIMARY KEY, name TEXT NOT NULL, kind TEXT NOT NULL);
CREATE TABLE voucher(id INTEGER PRIMARY KEY, series TEXT NOT NULL, number INTEGER NOT NULL, compile_date TEXT NOT NULL,
  event_date TEXT NOT NULL, description TEXT NOT NULL, counterparty TEXT NOT NULL DEFAULT '', created_by TEXT NOT NULL,
  approved_by TEXT NOT NULL, created_at TEXT NOT NULL, corrects_voucher_id INTEGER, prev_hash TEXT NOT NULL,
  hash TEXT NOT NULL, UNIQUE(series, number));
CREATE TABLE entry(id INTEGER PRIMARY KEY, voucher_id INTEGER NOT NULL, line_no INTEGER NOT NULL, account INTEGER NOT NULL,
  debit INTEGER NOT NULL, credit INTEGER NOT NULL, vat_code TEXT);
CREATE TABLE customer(id INTEGER PRIMARY KEY, name TEXT NOT NULL, org_number TEXT NOT NULL DEFAULT '',
  vat_number TEXT NOT NULL DEFAULT '', address TEXT NOT NULL DEFAULT '', country TEXT NOT NULL DEFAULT 'SE',
  email TEXT NOT NULL DEFAULT '', created_at TEXT NOT NULL);
CREATE TABLE invoice(id INTEGER PRIMARY KEY, series TEXT NOT NULL DEFAULT 'F', number INTEGER, customer_id INTEGER NOT NULL,
  status TEXT NOT NULL DEFAULT 'draft', issue_date TEXT NOT NULL, due_date TEXT NOT NULL, vat_rate INTEGER NOT NULL);
CREATE TABLE invoice_line(id INTEGER PRIMARY KEY, invoice_id INTEGER NOT NULL, description TEXT NOT NULL,
  quantity INTEGER NOT NULL, unit TEXT NOT NULL DEFAULT 'st', unit_price INTEGER NOT NULL);
CREATE TABLE document(id INTEGER PRIMARY KEY, sha256 TEXT NOT NULL UNIQUE, filename TEXT NOT NULL, mime TEXT NOT NULL,
  size INTEGER NOT NULL, uploaded_by TEXT NOT NULL, uploaded_at TEXT NOT NULL, source TEXT NOT NULL);
CREATE TABLE document_meta(document_id INTEGER PRIMARY KEY, vendor TEXT NOT NULL DEFAULT '', amount INTEGER,
  currency TEXT NOT NULL DEFAULT 'SEK', doc_date TEXT, notes TEXT NOT NULL DEFAULT '');
CREATE TABLE voucher_document(voucher_id INTEGER NOT NULL, document_id INTEGER NOT NULL, PRIMARY KEY(voucher_id, document_id));
CREATE TABLE employee(id INTEGER PRIMARY KEY, name TEXT NOT NULL, identity TEXT NOT NULL, birth_year INTEGER NOT NULL);
CREATE TABLE time_entry(id INTEGER PRIMARY KEY, start_utc TEXT NOT NULL);
CREATE TABLE audit_log(id INTEGER PRIMARY KEY, at TEXT NOT NULL, actor TEXT NOT NULL, action TEXT NOT NULL, object TEXT NOT NULL);
CREATE TABLE bank_transaction(id INTEGER PRIMARY KEY, booking_date TEXT NOT NULL, amount INTEGER NOT NULL);
CREATE TABLE draft(id INTEGER PRIMARY KEY, event_date TEXT NOT NULL);
";

const RECEIPTS: [(&str, &str); 2] = [
    ("receipt-one.txt", "invented receipt number one"),
    (
        "receipt-two.txt",
        "invented receipt number two, a bit longer",
    ),
];

type L = (i64, i64, i64, Option<&'static str>);
/// series, number, event date, description, counterparty, corrects, lines
type Spec = (
    &'static str,
    i64,
    &'static str,
    &'static str,
    &'static str,
    Option<i64>,
    Vec<L>,
);

/// A sample OAS file: seven accounts, vouchers A1 to A3 and B1 (A3 corrects A2), two
/// customers, two receipts (the files are written under `docs`), and rows in tables the
/// importer skips.
fn build_oas(path: &Path, docs: &Path) {
    let c = Connection::open(path).unwrap();
    c.execute_batch(SCHEMA).unwrap();
    c.execute(
        "INSERT INTO company(id, name, org_number, contact_name, contact_email, address, vat_number, payment_info, f_skatt)
         VALUES (1, 'Testbolaget AB', '556000-0000', 'Invented Person', 'books@example.test', 'Testgatan 1', 'SE556000000001', 'bg 000-0000', 1)",
        [],
    )
    .unwrap();
    for (n, name, kind) in [
        (1001, "Test bank", "T"),
        (1002, "Test receivable", "T"),
        (2001, "Test equity", "S"),
        (2002, "Test payable", "S"),
        (3001, "Test sales", "I"),
        (4001, "Test costs", "K"),
        (8001, "Test interest", "I"),
    ] {
        c.execute(
            "INSERT INTO account VALUES (?1, ?2, ?3)",
            params![n, name, kind],
        )
        .unwrap();
    }
    // (series, number, event date, description, counterparty, corrects, lines)
    let specs: Vec<Spec> = vec![
        (
            "A",
            1,
            "2025-01-10",
            "Start capital",
            "",
            None,
            vec![(1001, 10_000_000, 0, None), (2001, 0, 10_000_000, None)],
        ),
        (
            "A",
            2,
            "2025-02-03",
            "Consulting invoice",
            "Kund Ett",
            None,
            vec![
                (1002, 1_250_000, 0, None),
                (3001, 0, 1_000_000, Some("S25")),
                (2002, 0, 250_000, Some("S25")),
            ],
        ),
        (
            "A",
            3,
            "2025-02-04",
            "Correction of A2",
            "Kund Ett",
            Some(2),
            vec![
                (1002, 0, 1_250_000, None),
                (3001, 1_000_000, 0, None),
                (2002, 250_000, 0, None),
            ],
        ),
        (
            "B",
            1,
            "2025-03-01",
            "Office costs",
            "Leverantor",
            None,
            vec![(4001, 80_000, 0, None), (1001, 0, 80_000, None)],
        ),
    ];
    let mut prev_by_series: std::collections::HashMap<&str, String> = Default::default();
    for (i, (series, number, date, desc, cp, corrects, lines)) in specs.iter().enumerate() {
        let id = i as i64 + 1;
        let prev = prev_by_series
            .get(series)
            .cloned()
            .unwrap_or_else(|| "genesis".into());
        let mut v = Voucher {
            id,
            series: (*series).into(),
            number: *number,
            compile_date: "2025-04-01".into(),
            event_date: (*date).into(),
            description: (*desc).into(),
            counterparty: (*cp).into(),
            created_by: "agent".into(),
            approved_by: "admin".into(),
            corrects: *corrects,
            prev_hash: prev.clone(),
            hash: String::new(),
            lines: lines
                .iter()
                .map(|(a, d, cr, v)| (*a, *d, *cr, v.map(String::from)))
                .collect(),
            documents: vec![],
        };
        v.hash = canonical(&v);
        c.execute(
            "INSERT INTO voucher(id, series, number, compile_date, event_date, description, counterparty, created_by,
                approved_by, created_at, corrects_voucher_id, prev_hash, hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'agent', 'admin', '2025-04-01T00:00:00Z', ?8, ?9, ?10)",
            params![id, series, number, v.compile_date, date, desc, cp, corrects, prev, v.hash],
        )
        .unwrap();
        for (n, (a, d, cr, vat)) in lines.iter().enumerate() {
            c.execute(
                "INSERT INTO entry(voucher_id, line_no, account, debit, credit, vat_code) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, n as i64 + 1, a, d, cr, vat],
            )
            .unwrap();
        }
        prev_by_series.insert(series, v.hash);
    }
    c.execute(
        "INSERT INTO customer(id, name, org_number, address, email, created_at) VALUES (1, 'Kund Ett', '556111-1111', 'Kundvagen 2', 'kund@example.test', '2025-01-01')",
        [],
    )
    .unwrap();
    c.execute(
        "INSERT INTO customer(id, name, created_at) VALUES (2, 'Kund Tva', '2025-01-02')",
        [],
    )
    .unwrap();
    for (i, (name, text)) in RECEIPTS.iter().enumerate() {
        let sha = sha256_hex(text.as_bytes());
        let dir = docs.join(&sha[..2]);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(&sha), text).unwrap();
        c.execute(
            "INSERT INTO document VALUES (?1, ?2, ?3, 'text/plain', ?4, 'admin', '2025-04-01T00:00:00Z', 'upload')",
            params![i as i64 + 1, sha, name, text.len() as i64],
        )
        .unwrap();
        c.execute(
            "INSERT INTO document_meta(document_id, vendor, amount, doc_date) VALUES (?1, 'Leverantor', 80000, '2025-03-01')",
            [i as i64 + 1],
        )
        .unwrap();
    }
    c.execute("INSERT INTO voucher_document VALUES (4, 1)", [])
        .unwrap();
    c.execute(
        "INSERT INTO invoice VALUES (1, 'F', 1, 1, 'issued', '2025-02-03', '2025-03-05', 25)",
        [],
    )
    .unwrap();
    c.execute(
        "INSERT INTO invoice VALUES (2, 'F', NULL, 2, 'draft', '2025-02-10', '2025-03-10', 25)",
        [],
    )
    .unwrap();
    c.execute(
        "INSERT INTO invoice_line VALUES (1, 1, 'Consulting', 10, 'h', 100000)",
        [],
    )
    .unwrap();
    c.execute(
        "INSERT INTO employee VALUES (1, 'Invented Employee', '000000-0000', 1990)",
        [],
    )
    .unwrap();
    c.execute(
        "INSERT INTO time_entry VALUES (1, '2025-02-01T08:00:00Z')",
        [],
    )
    .unwrap();
    c.execute(
        "INSERT INTO time_entry VALUES (2, '2025-02-02T08:00:00Z')",
        [],
    )
    .unwrap();
    c.execute(
        "INSERT INTO audit_log VALUES (1, '2025-02-03', 'admin', 'post', 'voucher')",
        [],
    )
    .unwrap();
    c.execute(
        "INSERT INTO bank_transaction VALUES (1, '2025-02-05', 125000)",
        [],
    )
    .unwrap();
    c.execute("INSERT INTO draft VALUES (1, '2025-05-01')", [])
        .unwrap();
}

fn app() -> App {
    let app = App::new();
    app.dispatcher
        .set_clock(std::sync::Arc::new(|| SEPTEMBER_2026));
    for id in ["accounting", "documents"] {
        app.installer
            .install(&Bundled.fetch(id).unwrap(), "marketplace", "test")
            .unwrap_or_else(|e| panic!("{id}: {e}"));
    }
    let se = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/accounting_se.toml"
    ))
    .unwrap();
    app.installer.install(&se, "file", "test").unwrap();
    app
}

fn admin() -> Principal {
    Principal::user("root", &[ADMIN_GROUP])
}

fn opts(docs: &Path) -> Options {
    Options {
        default_account_types: true,
        documents: Some(docs.display().to_string()),
        ..Options::default()
    }
}

fn recs(app: &App, model: &str) -> Vec<(String, serde_json::Map<String, J>)> {
    app.store
        .backend()
        .list(model)
        .unwrap_or_default()
        .into_iter()
        .map(|(id, f)| (id, f.into_iter().collect()))
        .collect()
}

const MODELS: &[&str] = &[
    "accounting_account",
    "accounting_journal",
    "accounting_move",
    "accounting_move_line",
    "accounting_se_company",
    "party",
    "documents_document",
    MAP_MODEL,
    "oas_import_run",
];

fn counts(app: &App) -> Vec<usize> {
    MODELS.iter().map(|m| recs(app, m).len()).collect()
}

struct Fix {
    _d: Dir,
    oas: PathBuf,
    docs: PathBuf,
    app: App,
}

fn fix() -> Fix {
    let d = Dir::new();
    let (oas, docs) = (d.0.join("oas.db"), d.0.join("docs"));
    std::fs::create_dir_all(&docs).unwrap();
    build_oas(&oas, &docs);
    Fix {
        _d: d,
        oas,
        docs,
        app: app(),
    }
}

fn ok(r: Result<Report, Report>) -> Report {
    r.unwrap_or_else(|r| panic!("import failed:\n{r}"))
}

fn problems(r: Result<Report, Report>) -> String {
    match r {
        Ok(r) => panic!("expected a refusal, got:\n{r}"),
        Err(r) => r.problems.join("\n"),
    }
}

#[test]
fn a_dry_run_writes_nothing_and_lists_every_skipped_table_with_counts() {
    let f = fix();
    let before = counts(&f.app);
    let o = Options {
        dry_run: true,
        ..opts(&f.docs)
    };
    let rep = ok(import(&f.app, &admin(), &f.oas, &o));
    assert_eq!(counts(&f.app), before, "a dry run must write nothing");
    assert!(before.iter().all(|n| *n == 0));
    let text = rep.to_string();
    for want in [
        "dry run, nothing written",
        "accounting_move(+_line)",
        "4 new",
        "invoice ",
        "[1 draft, 1 issued]",
        "time_entry",
        "audit_log",
        "bank_transaction",
        "employee",
        "draft ",
        "contact_name (has a value; not copied)",
        "payment_info (has a value; not copied)",
        "1 correcting voucher(s)",
        "chain verified: 4 voucher(s) in 2 series",
    ] {
        assert!(text.contains(want), "missing {want:?} in:\n{text}");
    }
    // The skipped rows carry the counts.
    let skipped: std::collections::HashMap<_, _> = rep
        .to_string()
        .lines()
        .filter_map(|l| {
            let mut w = l.split_whitespace();
            Some((w.next()?.to_string(), w.next()?.parse::<i64>().ok()?))
        })
        .collect();
    assert_eq!(skipped["time_entry"], 2);
    assert_eq!(skipped["invoice_line"], 1);
}

#[test]
fn account_types_must_be_confirmed_before_anything_is_written() {
    let f = fix();
    let o = Options {
        default_account_types: false,
        ..opts(&f.docs)
    };
    let p = problems(import(&f.app, &admin(), &f.oas, &o));
    assert!(p.contains("7 account(s) have no confirmed type"), "{p}");
    assert!(counts(&f.app).iter().all(|n| *n == 0));
    // A file can confirm them one by one.
    let file = "number,type\n1001,asset_cash\n1002,asset_receivable\n2001,equity\n2002,liability_payable\n3001,income\n4001,expense\n8001,income_other\n";
    let o = Options {
        default_account_types: false,
        account_types: Some(file.into()),
        ..opts(&f.docs)
    };
    ok(import(&f.app, &admin(), &f.oas, &o));
    let types: Vec<_> = recs(&f.app, "accounting_account")
        .iter()
        .map(|(_, r)| {
            (
                r["code"].as_str().unwrap().to_string(),
                r["account_type"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert!(
        types.contains(&("8001".into(), "income_other".into())),
        "{types:?}"
    );
}

#[test]
fn drafts_keep_the_oas_number_and_the_trial_balance_equals_oas() {
    let f = fix();
    let rep = ok(import(&f.app, &admin(), &f.oas, &opts(&f.docs)));
    let verified = rep.verified.clone().unwrap();
    assert!(
        verified.contains("trial balance equals OAS: 6 account(s), 4 move(s), 10 line(s)"),
        "{verified}"
    );
    assert!(
        verified.contains("total debit 12580000 = total credit 12580000"),
        "{verified}"
    );
    assert!(verified.contains("drafts"), "{verified}");
    let moves = recs(&f.app, "accounting_move");
    assert_eq!(moves.len(), 4);
    let mut refs: Vec<_> = moves
        .iter()
        .map(|(_, r)| r["reference"].as_str().unwrap().to_string())
        .collect();
    refs.sort();
    assert_eq!(refs, ["A1", "A2", "A3", "B1"]);
    assert!(
        moves
            .iter()
            .all(|(_, r)| r["state"] == "draft" && r.get("number").is_none_or(J::is_null))
    );
    // The correction names what it corrects, the receipt is named, a VAT code is in the label.
    let a3 = moves.iter().find(|(_, r)| r["reference"] == "A3").unwrap();
    assert!(
        a3.1["narration"]
            .as_str()
            .unwrap()
            .contains("Corrects OAS voucher A2")
    );
    let b1 = moves.iter().find(|(_, r)| r["reference"] == "B1").unwrap();
    assert!(
        b1.1["narration"]
            .as_str()
            .unwrap()
            .contains("receipt-one.txt (sha256 ")
    );
    assert!(
        recs(&f.app, "accounting_move_line")
            .iter()
            .any(|(_, r)| r["label"] == "Consulting invoice [S25]")
    );
    // Journals per series, a document with its file, parties with the folded fields.
    assert_eq!(recs(&f.app, "accounting_journal").len(), 2);
    let docs = recs(&f.app, "documents_document");
    assert_eq!(docs.len(), 2);
    assert!(docs.iter().all(|(_, r)| r["attachment"].is_string()));
    let party = recs(&f.app, "party");
    assert_eq!(party.len(), 2);
    assert!(party.iter().any(|(_, r)| {
        r["notes"]
            .as_str()
            .unwrap()
            .contains("Org number: 556111-1111")
    }));
    assert_eq!(
        recs(&f.app, "accounting_se_company")[0].1["org_number"],
        "556000-0000"
    );
    assert!(rep.to_string().contains("not imported"));
}

#[test]
fn post_runs_the_real_action_in_oas_order_and_a_rerun_creates_nothing() {
    let f = fix();
    let o = Options {
        post: true,
        ..opts(&f.docs)
    };
    let rep = ok(import(&f.app, &admin(), &f.oas, &o));
    assert!(
        rep.verified.as_ref().unwrap().contains("all posted"),
        "{rep}"
    );
    let moves = recs(&f.app, "accounting_move");
    let number_of = |r: &str| {
        moves.iter().find(|(_, m)| m["reference"] == r).unwrap().1["number"]
            .as_str()
            .unwrap()
            .to_string()
    };
    // OAS voucher order (by id): A1, A2, A3, B1.
    assert_eq!(number_of("A1"), "JE/2026/00001");
    assert_eq!(number_of("A2"), "JE/2026/00002");
    assert_eq!(number_of("A3"), "JE/2026/00003");
    assert_eq!(number_of("B1"), "JE/2026/00004");
    assert!(moves.iter().all(|(_, m)| m["state"] == "posted"));
    // Same file again: nothing new, same answer.
    let before = counts(&f.app);
    let again = ok(import(&f.app, &admin(), &f.oas, &o));
    assert_eq!(counts(&f.app), before, "a re-run must create nothing");
    assert!(again.verified.unwrap().contains("trial balance equals OAS"));
}

#[test]
fn a_rerun_of_a_draft_import_creates_nothing_and_post_can_follow() {
    let f = fix();
    ok(import(&f.app, &admin(), &f.oas, &opts(&f.docs)));
    let before = counts(&f.app);
    ok(import(&f.app, &admin(), &f.oas, &opts(&f.docs)));
    assert_eq!(counts(&f.app), before, "a re-run must create nothing");
    // Post afterwards, in a later run.
    let o = Options {
        post: true,
        ..opts(&f.docs)
    };
    ok(import(&f.app, &admin(), &f.oas, &o));
    assert!(
        recs(&f.app, "accounting_move")
            .iter()
            .all(|(_, m)| m["state"] == "posted")
    );
    assert_eq!(recs(&f.app, "accounting_move").len(), 4);
}

#[test]
fn a_receipt_whose_hash_does_not_match_is_refused_before_anything_is_written() {
    let f = fix();
    let sha = sha256_hex(RECEIPTS[1].1.as_bytes());
    std::fs::write(f.docs.join(&sha[..2]).join(&sha), "tampered after the fact").unwrap();
    let p = problems(import(&f.app, &admin(), &f.oas, &opts(&f.docs)));
    assert!(p.contains("SHA-256 MISMATCH"), "{p}");
    assert!(p.contains("receipt-two.txt"), "{p}");
    assert!(
        counts(&f.app).iter().all(|n| *n == 0),
        "a refused run writes nothing"
    );
    // A missing file is refused too.
    std::fs::remove_file(f.docs.join(&sha[..2]).join(&sha)).unwrap();
    let p = problems(import(&f.app, &admin(), &f.oas, &opts(&f.docs)));
    assert!(p.contains("no file under"), "{p}");
}

#[test]
fn a_broken_chain_is_refused_unless_told_to_continue() {
    let f = fix();
    let c = Connection::open(&f.oas).unwrap();
    // Alter a voucher the way an edit outside OAS would (the OAS triggers are not in the sample).
    c.execute(
        "UPDATE voucher SET description = 'edited later' WHERE id = 2",
        [],
    )
    .unwrap();
    drop(c);
    let p = problems(import(&f.app, &admin(), &f.oas, &opts(&f.docs)));
    assert!(
        p.contains("A2: hash mismatch, the voucher was altered"),
        "{p}"
    );
    assert!(counts(&f.app).iter().all(|n| *n == 0));
    let o = Options {
        allow_broken_chain: true,
        ..opts(&f.docs)
    };
    let rep = ok(import(&f.app, &admin(), &f.oas, &o));
    assert!(rep.to_string().contains("chain BROKEN"));
}

#[test]
fn an_unbalanced_voucher_is_always_refused() {
    let f = fix();
    let c = Connection::open(&f.oas).unwrap();
    c.execute("UPDATE entry SET debit = debit + 1 WHERE id = 1", [])
        .unwrap();
    drop(c);
    let o = Options {
        allow_broken_chain: true,
        ..opts(&f.docs)
    };
    let p = problems(import(&f.app, &admin(), &f.oas, &o));
    assert!(p.contains("A1: entries do not balance"), "{p}");
}

#[test]
fn post_is_refused_for_an_agent_and_for_a_person_without_the_approver_group() {
    let f = fix();
    let o = Options {
        post: true,
        ..opts(&f.docs)
    };
    let agent = Principal::agent(
        "bot",
        &[ADMIN_GROUP],
        &[
            "accounting.user",
            "accounting.manager",
            "accounting.approver",
        ],
    );
    let p = problems(import(&f.app, &agent, &f.oas, &o));
    assert!(p.contains("--post runs the real post action"), "{p}");
    let clerk = Principal::user(
        "clerk",
        &[
            "accounting.user",
            "accounting.manager",
            "base.user",
            "documents.user",
        ],
    );
    let p = problems(import(&f.app, &clerk, &f.oas, &o));
    assert!(p.contains("--post runs the real post action"), "{p}");
    assert!(counts(&f.app).iter().all(|n| *n == 0));
}

#[test]
fn a_difference_from_the_oas_trial_balance_fails_the_run() {
    let f = fix();
    ok(import(&f.app, &admin(), &f.oas, &opts(&f.docs)));
    // Someone edits a draft line after the import; the next run notices.
    let (line, _) = recs(&f.app, "accounting_move_line")
        .into_iter()
        .next()
        .unwrap();
    f.app
        .store
        .backend()
        .update(
            "accounting_move_line",
            &line,
            &[("debit".to_string(), J::from(1))].into_iter().collect(),
        )
        .unwrap();
    let p = problems(import(&f.app, &admin(), &f.oas, &opts(&f.docs)));
    assert!(p.contains("TRIAL BALANCE DIFFERS from OAS"), "{p}");
}

#[test]
fn a_changed_voucher_after_import_is_refused() {
    let f = fix();
    ok(import(&f.app, &admin(), &f.oas, &opts(&f.docs)));
    let c = Connection::open(&f.oas).unwrap();
    c.execute("UPDATE voucher SET hash = 'beef' WHERE id = 1", [])
        .unwrap();
    drop(c);
    let o = Options {
        allow_broken_chain: true,
        ..opts(&f.docs)
    };
    let p = problems(import(&f.app, &admin(), &f.oas, &o));
    assert!(p.contains("the OAS history changed"), "{p}");
}

#[test]
fn the_oas_file_is_opened_read_only() {
    let f = fix();
    let before = std::fs::read(&f.oas).unwrap();
    ok(import(&f.app, &admin(), &f.oas, &opts(&f.docs)));
    assert_eq!(std::fs::read(&f.oas).unwrap(), before);
}
