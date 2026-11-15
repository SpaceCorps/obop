//! The OAS side of the importer: the SQLite file read-only, the chain check, the trial balance
//! and the table inventory. Nothing here knows about obop records.

use rusqlite::{Connection, OpenFlags, params};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

/// One entry line: account number, debit, credit (öre) and the optional VAT code.
pub type Line = (i64, i64, i64, Option<String>);

#[derive(Debug, Clone)]
pub struct Company {
    pub name: String,
    pub org_number: String,
    pub vat_number: String,
    pub address: String,
    pub contact_email: String,
    pub f_skatt: bool,
    /// Columns that have no obop field and are not copied, with whether they hold a value.
    pub dropped: Vec<(&'static str, bool)>,
}

#[derive(Debug, Clone)]
pub struct Account {
    pub number: i64,
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone)]
pub struct Voucher {
    pub id: i64,
    pub series: String,
    pub number: i64,
    pub compile_date: String,
    pub event_date: String,
    pub description: String,
    pub counterparty: String,
    pub created_by: String,
    pub approved_by: String,
    pub corrects: Option<i64>,
    pub prev_hash: String,
    pub hash: String,
    pub lines: Vec<Line>,
    /// `(filename, sha256)` of the receipts attached to the voucher.
    pub documents: Vec<(String, String)>,
}

impl Voucher {
    pub fn reference(&self) -> String {
        format!("{}{}", self.series, self.number)
    }
}

#[derive(Debug, Clone)]
pub struct Customer {
    pub id: i64,
    pub name: String,
    pub org_number: String,
    pub vat_number: String,
    pub address: String,
    pub country: String,
    pub email: String,
}

#[derive(Debug, Clone)]
pub struct Document {
    pub id: i64,
    pub sha256: String,
    pub filename: String,
    pub mime: String,
    pub size: i64,
    pub vendor: String,
    pub amount: Option<i64>,
    pub currency: String,
    pub doc_date: Option<String>,
    pub notes: String,
}

/// Everything read from one OAS file.
#[derive(Debug)]
pub struct Oas {
    pub company: Option<Company>,
    pub accounts: Vec<Account>,
    pub vouchers: Vec<Voucher>,
    pub customers: Vec<Customer>,
    pub documents: Vec<Document>,
    /// Every user table with its row count, in name order.
    pub tables: BTreeMap<String, i64>,
    /// Invoices per status, for the skipped-table report.
    pub invoices_by_status: BTreeMap<String, i64>,
}

pub fn open(path: &Path) -> Result<Connection, String> {
    if !path.is_file() {
        return Err(format!("{} is not a file", path.display()));
    }
    // Read-only, so the importer cannot change the books it reads (and cannot create a file).
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("cannot open {} read-only: {e}", path.display()))?;
    conn.execute_batch("PRAGMA query_only = ON;")
        .map_err(|e| e.to_string())?;
    Ok(conn)
}

fn err<E: std::fmt::Display>(what: &'static str) -> impl Fn(E) -> String {
    move |e| format!("OAS file, {what}: {e}")
}

fn has_table(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0)
        > 0
}

pub fn read(conn: &Connection) -> Result<Oas, String> {
    for t in ["account", "voucher", "entry"] {
        if !has_table(conn, t) {
            return Err(format!(
                "this does not look like an OAS database: table {t} is missing"
            ));
        }
    }
    let mut tables = BTreeMap::new();
    let names: Vec<String> = conn
        .prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .and_then(|mut s| s.query_map([], |r| r.get(0))?.collect())
        .map_err(err("tables"))?;
    for n in names {
        // The name comes from the file's own catalogue and is quoted.
        let c: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM \"{}\"", n.replace('"', "\"\"")),
                [],
                |r| r.get(0),
            )
            .map_err(err("row count"))?;
        tables.insert(n, c);
    }
    let company = if has_table(conn, "company") {
        conn.query_row(
            "SELECT name, org_number, contact_name, contact_phone, contact_email, address,
                    vat_number, payment_info, f_skatt FROM company WHERE id = 1",
            [],
            |r| {
                let (cn, cp, pi): (String, String, String) = (r.get(2)?, r.get(3)?, r.get(7)?);
                Ok(Company {
                    name: r.get(0)?,
                    org_number: r.get(1)?,
                    contact_email: r.get(4)?,
                    address: r.get(5)?,
                    vat_number: r.get(6)?,
                    f_skatt: r.get::<_, i64>(8)? != 0,
                    dropped: vec![
                        ("contact_name", !cn.is_empty()),
                        ("contact_phone", !cp.is_empty()),
                        ("payment_info", !pi.is_empty()),
                    ],
                })
            },
        )
        .ok()
    } else {
        None
    };
    let accounts = conn
        .prepare("SELECT number, name, kind FROM account ORDER BY number")
        .and_then(|mut s| {
            s.query_map([], |r| {
                Ok(Account {
                    number: r.get(0)?,
                    name: r.get(1)?,
                    kind: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
        })
        .map_err(err("account"))?;

    // Receipts per voucher.
    let mut docs_of: BTreeMap<i64, Vec<(String, String)>> = BTreeMap::new();
    if has_table(conn, "voucher_document") && has_table(conn, "document") {
        let mut s = conn
            .prepare(
                "SELECT vd.voucher_id, d.filename, d.sha256 FROM voucher_document vd
                 JOIN document d ON d.id = vd.document_id ORDER BY vd.voucher_id, d.id",
            )
            .map_err(err("voucher_document"))?;
        let rows = s
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get(1)?, r.get(2)?)))
            .map_err(err("voucher_document"))?;
        for row in rows {
            let (v, f, h) = row.map_err(err("voucher_document"))?;
            docs_of.entry(v).or_default().push((f, h));
        }
    }
    let mut lines_of: BTreeMap<i64, Vec<Line>> = BTreeMap::new();
    {
        let mut s = conn
            .prepare(
                "SELECT voucher_id, account, debit, credit, vat_code FROM entry ORDER BY voucher_id, line_no",
            )
            .map_err(err("entry"))?;
        let rows = s
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    (r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?),
                ))
            })
            .map_err(err("entry"))?;
        for row in rows {
            let (v, l) = row.map_err(err("entry"))?;
            lines_of.entry(v).or_default().push(l);
        }
    }
    let vouchers = conn
        .prepare(
            "SELECT id, series, number, compile_date, event_date, description, counterparty,
                    created_by, approved_by, corrects_voucher_id, prev_hash, hash
             FROM voucher ORDER BY id",
        )
        .and_then(|mut s| {
            s.query_map([], |r| {
                let id: i64 = r.get(0)?;
                Ok(Voucher {
                    id,
                    series: r.get(1)?,
                    number: r.get(2)?,
                    compile_date: r.get(3)?,
                    event_date: r.get(4)?,
                    description: r.get(5)?,
                    counterparty: r.get(6)?,
                    created_by: r.get(7)?,
                    approved_by: r.get(8)?,
                    corrects: r.get(9)?,
                    prev_hash: r.get(10)?,
                    hash: r.get(11)?,
                    lines: lines_of.get(&id).cloned().unwrap_or_default(),
                    documents: docs_of.get(&id).cloned().unwrap_or_default(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()
        })
        .map_err(err("voucher"))?;

    let customers = if has_table(conn, "customer") {
        conn.prepare(
            "SELECT id, name, org_number, vat_number, address, country, email FROM customer ORDER BY id",
        )
        .and_then(|mut s| {
            s.query_map([], |r| {
                Ok(Customer {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    org_number: r.get(2)?,
                    vat_number: r.get(3)?,
                    address: r.get(4)?,
                    country: r.get(5)?,
                    email: r.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
        })
        .map_err(err("customer"))?
    } else {
        vec![]
    };
    let documents = if has_table(conn, "document") {
        let meta = has_table(conn, "document_meta");
        let sql = if meta {
            "SELECT d.id, d.sha256, d.filename, d.mime, d.size, m.vendor, m.amount, m.currency,
                    m.doc_date, m.notes FROM document d LEFT JOIN document_meta m ON m.document_id = d.id
             ORDER BY d.id"
        } else {
            "SELECT d.id, d.sha256, d.filename, d.mime, d.size, NULL, NULL, NULL, NULL, NULL
             FROM document d ORDER BY d.id"
        };
        conn.prepare(sql)
            .and_then(|mut s| {
                s.query_map([], |r| {
                    Ok(Document {
                        id: r.get(0)?,
                        sha256: r.get(1)?,
                        filename: r.get(2)?,
                        mime: r.get(3)?,
                        size: r.get(4)?,
                        vendor: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                        amount: r.get(6)?,
                        currency: r
                            .get::<_, Option<String>>(7)?
                            .unwrap_or_else(|| "SEK".into()),
                        doc_date: r.get(8)?,
                        notes: r.get::<_, Option<String>>(9)?.unwrap_or_default(),
                    })
                })?
                .collect::<Result<Vec<_>, _>>()
            })
            .map_err(err("document"))?
    } else {
        vec![]
    };
    let mut invoices_by_status = BTreeMap::new();
    if has_table(conn, "invoice") {
        let mut s = conn
            .prepare("SELECT status, COUNT(*) FROM invoice GROUP BY status")
            .map_err(err("invoice"))?;
        let rows = s
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .map_err(err("invoice"))?;
        for row in rows {
            let (st, n) = row.map_err(err("invoice"))?;
            invoices_by_status.insert(st, n);
        }
    }
    Ok(Oas {
        company,
        accounts,
        vouchers,
        customers,
        documents,
        tables,
        invoices_by_status,
    })
}

/// The hash input of OAS (`ledger.rs`, `canonical`), spelled out the same way so the chain can
/// be checked here without the OAS server.
pub fn canonical(v: &Voucher) -> String {
    let body: Vec<String> = v
        .lines
        .iter()
        .map(|(a, d, c, vat)| format!("{a}:{d}:{c}:{}", vat.as_deref().unwrap_or("")))
        .collect();
    let text = format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}",
        v.prev_hash,
        v.series,
        v.number,
        v.compile_date,
        v.event_date,
        v.description,
        v.counterparty,
        v.corrects.map(|c| c.to_string()).unwrap_or_default(),
        body.join(";")
    );
    sha256_hex(text.as_bytes())
}

#[derive(Debug, Default)]
pub struct Check {
    /// Chain problems: numbering gaps, a wrong `prev_hash`, an altered voucher.
    pub chain: Vec<String>,
    /// Vouchers whose entries do not balance, or hold a negative amount or no line.
    pub unsound: Vec<String>,
    /// The last hash of each series, for the import record.
    pub tips: BTreeMap<String, String>,
}

/// What the OAS `verify` route checks, plus the balance of each voucher.
pub fn check(vouchers: &[Voucher]) -> Check {
    let mut out = Check::default();
    let mut by_series: BTreeMap<&str, Vec<&Voucher>> = BTreeMap::new();
    for v in vouchers {
        by_series.entry(&v.series).or_default().push(v);
    }
    for (s, mut vs) in by_series {
        vs.sort_by_key(|v| v.number);
        let mut prev = "genesis".to_string();
        for (i, v) in vs.iter().enumerate() {
            let r = v.reference();
            if v.number != i as i64 + 1 {
                out.chain.push(format!(
                    "{s}: gap or disorder at number {} (expected {})",
                    v.number,
                    i + 1
                ));
            }
            if v.prev_hash != prev {
                out.chain
                    .push(format!("{r}: prev_hash does not match previous voucher"));
            }
            if canonical(v) != v.hash {
                out.chain
                    .push(format!("{r}: hash mismatch, the voucher was altered"));
            }
            prev = v.hash.clone();
        }
        out.tips.insert(s.to_string(), prev);
    }
    for v in vouchers {
        let (mut d, mut c) = (0i128, 0i128);
        for (_, dd, cc, _) in &v.lines {
            if *dd < 0 || *cc < 0 {
                out.unsound
                    .push(format!("{}: a negative amount", v.reference()));
            }
            d += i128::from(*dd);
            c += i128::from(*cc);
        }
        if v.lines.is_empty() {
            out.unsound
                .push(format!("{}: no entry lines", v.reference()));
        } else if d != c {
            out.unsound.push(format!(
                "{}: entries do not balance (debit {d}, credit {c})",
                v.reference()
            ));
        }
    }
    out
}

/// Per account `(debit, credit)` over all entries, as the `trial-balance` report of OAS sums
/// them, straight from the SQL it uses.
pub fn trial_balance(conn: &Connection) -> Result<BTreeMap<i64, (i64, i64)>, String> {
    let mut s = conn
        .prepare(
            "SELECT e.account, SUM(e.debit), SUM(e.credit)
             FROM entry e JOIN voucher v ON v.id = e.voucher_id JOIN account a ON a.number = e.account
             WHERE (?1 IS NULL OR v.event_date >= ?1) AND (?2 IS NULL OR v.event_date <= ?2)
             GROUP BY e.account ORDER BY e.account",
        )
        .map_err(err("trial balance"))?;
    let rows = s
        .query_map(params![None::<String>, None::<String>], |r| {
            Ok((r.get::<_, i64>(0)?, (r.get(1)?, r.get(2)?)))
        })
        .map_err(err("trial balance"))?;
    rows.map(|r| r.map_err(err("trial balance"))).collect()
}

/// The file of a receipt under the OAS layout `DIR/ab/abcdef...` (a flat `DIR/abcdef...` is
/// accepted too).
pub fn document_path(dir: &Path, sha: &str) -> Option<std::path::PathBuf> {
    if sha.len() < 3 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let nested = dir.join(&sha[..2]).join(sha);
    if nested.is_file() {
        return Some(nested);
    }
    let flat = dir.join(sha);
    flat.is_file().then_some(flat)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}
