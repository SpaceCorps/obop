//! SIE 4 writer.

use crate::cp437;
use crate::model::{Balance, SieFile};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Minor units as a decimal string with two decimals: `125050` -> `1250.50`.
pub fn format_amount(minor: i64) -> String {
    let sign = if minor < 0 { "-" } else { "" };
    let a = minor.unsigned_abs();
    format!("{sign}{}.{:02}", a / 100, a % 100)
}

/// Quote a text field: double quotes around it, `"` and `\` escaped, line breaks as spaces.
pub fn quote(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '\n' | '\r' => o.push(' '),
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn is_balance_sheet(f: &SieFile, account: u32) -> bool {
    match f
        .accounts
        .iter()
        .find(|a| a.number == account)
        .and_then(|a| a.kind)
    {
        Some(k) => k.is_balance_sheet(),
        None => account < 3000,
    }
}

fn clamp(v: i128) -> i64 {
    v.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

/// Derive `#UB` and `#RES` for year 0 from the opening balances and the vouchers whose date
/// lies in year 0 (all vouchers when no year is declared).
fn derive(f: &SieFile) -> (Vec<Balance>, Vec<Balance>) {
    let year = f.years.iter().find(|y| y.index == 0);
    let mut sums: BTreeMap<u32, i128> = BTreeMap::new();
    for b in f.opening.iter().filter(|b| b.year_index == 0) {
        *sums.entry(b.account).or_default() += i128::from(b.amount_minor);
    }
    let mut res: BTreeMap<u32, i128> = BTreeMap::new();
    for v in &f.vouchers {
        if year.is_some_and(|y| v.date < y.start || v.date > y.end) {
            continue;
        }
        for l in &v.lines {
            if is_balance_sheet(f, l.account) {
                *sums.entry(l.account).or_default() += i128::from(l.amount_minor);
            } else {
                *res.entry(l.account).or_default() += i128::from(l.amount_minor);
            }
        }
    }
    let rows = |m: BTreeMap<u32, i128>| {
        m.into_iter()
            .map(|(account, v)| Balance {
                year_index: 0,
                account,
                amount_minor: clamp(v),
            })
            .collect::<Vec<_>>()
    };
    (rows(sums), rows(res))
}

/// Write a SIE type 4 file: CP437, CRLF line ends. A voucher that does not balance is written
/// as given (and refused by [`crate::parse()`]).
pub fn export(f: &SieFile) -> Vec<u8> {
    let mut o = String::new();
    let _ = write!(o, "#FLAGGA 0\r\n");
    let _ = write!(
        o,
        "#PROGRAM {} {}\r\n",
        quote(&f.program),
        quote(&f.program_version)
    );
    let _ = write!(o, "#FORMAT PC8\r\n");
    let _ = write!(o, "#GEN {}\r\n", f.generated.compact());
    let _ = write!(o, "#SIETYP 4\r\n");
    let _ = write!(o, "#FNAMN {}\r\n", quote(&f.company.name));
    if let Some(org) = &f.company.org_number {
        let _ = write!(o, "#ORGNR {org}\r\n");
    }
    for y in &f.years {
        let _ = write!(
            o,
            "#RAR {} {} {}\r\n",
            y.index,
            y.start.compact(),
            y.end.compact()
        );
    }
    if let Some(c) = &f.chart {
        let _ = write!(o, "#KPTYP {c}\r\n");
    }
    let mut accounts: Vec<_> = f.accounts.iter().collect();
    accounts.sort_by_key(|a| a.number);
    for a in &accounts {
        let _ = write!(o, "#KONTO {} {}\r\n", a.number, quote(&a.name));
        if let Some(k) = a.kind {
            let _ = write!(o, "#KTYP {} {}\r\n", a.number, k.code());
        }
    }
    let (d_ub, d_res) = derive(f);
    let ub = if f.closing.is_empty() {
        &d_ub
    } else {
        &f.closing
    };
    let res = if f.results.is_empty() {
        &d_res
    } else {
        &f.results
    };
    for (tag, rows) in [("#IB", &f.opening), ("#UB", ub), ("#RES", res)] {
        for b in rows {
            let _ = write!(
                o,
                "{tag} {} {} {}\r\n",
                b.year_index,
                b.account,
                format_amount(b.amount_minor)
            );
        }
    }
    let mut vouchers: Vec<_> = f.vouchers.iter().collect();
    vouchers.sort_by(|a, b| {
        (&a.series, a.number.parse::<u64>().ok(), &a.number).cmp(&(
            &b.series,
            b.number.parse::<u64>().ok(),
            &b.number,
        ))
    });
    for v in vouchers {
        let _ = write!(
            o,
            "#VER {} {} {} {}",
            quote(&v.series),
            quote(&v.number),
            v.date.compact(),
            quote(&v.text)
        );
        if v.registered.is_some() || v.signed.is_some() {
            let _ = write!(o, " {}", v.registered.unwrap_or(v.date).compact());
        }
        if let Some(s) = &v.signed {
            let _ = write!(o, " {}", quote(s));
        }
        let _ = write!(o, "\r\n{{\r\n");
        for l in &v.lines {
            let objs: Vec<String> = l
                .objects
                .iter()
                .flat_map(|(d, x)| [quote(d), quote(x)])
                .collect();
            let _ = write!(
                o,
                "#TRANS {} {{{}}} {}",
                l.account,
                objs.join(" "),
                format_amount(l.amount_minor)
            );
            if let Some(t) = &l.text {
                // The optional transaction date is the voucher date; the text follows it.
                let _ = write!(o, " {} {}", v.date.compact(), quote(t));
            }
            let _ = write!(o, "\r\n");
        }
        let _ = write!(o, "}}\r\n");
    }
    cp437::encode(&o)
}
