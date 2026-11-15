//! SIE 4 reader.

use crate::cp437;
use crate::model::{Account, AccountKind, Balance, Date, FiscalYear, Line, SieFile, Voucher};
use std::fmt;

/// A refusal, with the 1-based line number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SieError {
    Syntax {
        line: usize,
        message: String,
    },
    /// A voucher whose lines do not sum to zero; `line` is its `#VER` line.
    Unbalanced {
        line: usize,
        series: String,
        number: String,
        sum_minor: i128,
    },
}

impl fmt::Display for SieError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SieError::Syntax { line, message } => write!(f, "line {line}: {message}"),
            SieError::Unbalanced {
                line,
                series,
                number,
                sum_minor,
            } => write!(
                f,
                "line {line}: voucher {series}{number} is unbalanced (lines sum to {sum_minor} minor units instead of 0)"
            ),
        }
    }
}

impl std::error::Error for SieError {}

fn syn<T>(line: usize, m: impl Into<String>) -> Result<T, SieError> {
    Err(SieError::Syntax {
        line,
        message: m.into(),
    })
}

#[derive(Debug, PartialEq)]
enum Tok {
    Word(String),
    Open,
    Close,
}

fn tokenize(s: &str, line: usize) -> Result<Vec<Tok>, SieError> {
    let mut out = Vec::new();
    let mut it = s.chars().peekable();
    while let Some(&c) = it.peek() {
        if c.is_whitespace() {
            it.next();
        } else if c == '{' {
            it.next();
            out.push(Tok::Open);
        } else if c == '}' {
            it.next();
            out.push(Tok::Close);
        } else if c == '"' {
            it.next();
            let mut w = String::new();
            loop {
                match it.next() {
                    None => return syn(line, "unterminated quoted field"),
                    Some('"') => break,
                    Some('\\') => match it.peek() {
                        Some(&n) if n == '"' || n == '\\' => {
                            w.push(n);
                            it.next();
                        }
                        _ => w.push('\\'),
                    },
                    Some(ch) => w.push(ch),
                }
            }
            out.push(Tok::Word(w));
        } else {
            let mut w = String::new();
            while let Some(&ch) = it.peek() {
                if ch.is_whitespace() || ch == '{' || ch == '}' || ch == '"' {
                    break;
                }
                w.push(ch);
                it.next();
            }
            out.push(Tok::Word(w));
        }
    }
    Ok(out)
}

/// Parse an amount such as `1250.50`, `-12,5` or `7` into minor units. More than two decimals
/// are refused unless the extra digits are zeros.
pub fn parse_amount(s: &str) -> Option<i64> {
    let (neg, rest) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let (whole, frac) = rest.split_once(['.', ',']).unwrap_or((rest, ""));
    if whole.is_empty() && frac.is_empty() {
        return None;
    }
    if !whole.bytes().all(|b| b.is_ascii_digit()) || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (keep, extra) = if frac.len() > 2 {
        frac.split_at(2)
    } else {
        (frac, "")
    };
    if extra.bytes().any(|b| b != b'0') {
        return None;
    }
    let w: i128 = if whole.is_empty() {
        0
    } else {
        whole.parse().ok()?
    };
    let f: i128 = format!("{keep:0<2}").parse().ok()?;
    let v = w.checked_mul(100)?.checked_add(f)?;
    i64::try_from(if neg { -v } else { v }).ok()
}

fn decode(bytes: &[u8]) -> String {
    let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => cp437::decode(b),
    }
}

struct Ctx<'a> {
    line: usize,
    toks: &'a [Tok],
}

impl Ctx<'_> {
    fn word(&self, i: usize, what: &str) -> Result<&str, SieError> {
        match self.toks.get(i) {
            Some(Tok::Word(w)) => Ok(w),
            _ => syn(self.line, format!("missing {what}")),
        }
    }
    fn date(&self, i: usize, what: &str) -> Result<Date, SieError> {
        let w = self.word(i, what)?;
        match Date::from_compact(w) {
            Some(d) => Ok(d),
            None => syn(
                self.line,
                format!("invalid {what} `{w}`, expected YYYYMMDD"),
            ),
        }
    }
    fn account(&self, i: usize) -> Result<u32, SieError> {
        let w = self.word(i, "account number")?;
        match w.parse() {
            Ok(n) => Ok(n),
            Err(_) => syn(self.line, format!("invalid account number `{w}`")),
        }
    }
    fn amount(&self, i: usize) -> Result<i64, SieError> {
        let w = self.word(i, "amount")?;
        match parse_amount(w) {
            Some(a) => Ok(a),
            None => syn(self.line, format!("invalid amount `{w}`")),
        }
    }
    fn int(&self, i: usize, what: &str) -> Result<i32, SieError> {
        let w = self.word(i, what)?;
        match w.parse() {
            Ok(n) => Ok(n),
            Err(_) => syn(self.line, format!("invalid {what} `{w}`")),
        }
    }
}

/// Read a SIE 4 file (CP437 or UTF-8). Unknown labels are ignored; vouchers must balance.
pub fn parse(bytes: &[u8]) -> Result<SieFile, SieError> {
    let text = decode(bytes);
    let epoch = Date::new(1970, 1, 1).expect("valid date");
    let mut file = SieFile::new("", epoch, epoch);
    file.program = String::new();
    file.program_version = String::new();
    file.years.clear();
    let lines: Vec<&str> = text
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    let mut i = 0;
    let mut sietyp_seen = false;
    while i < lines.len() {
        let ln = i + 1;
        let toks = tokenize(lines[i], ln)?;
        i += 1;
        let Some(Tok::Word(label)) = toks.first() else {
            if toks.is_empty() {
                continue;
            }
            return syn(ln, "unexpected brace outside a voucher");
        };
        let c = Ctx {
            line: ln,
            toks: &toks,
        };
        match label.to_ascii_uppercase().as_str() {
            "#SIETYP" => {
                sietyp_seen = true;
                let t = c.word(1, "SIE type")?;
                if t != "4" {
                    return syn(ln, format!("only SIE type 4 is supported, found `{t}`"));
                }
            }
            "#PROGRAM" => {
                file.program = c.word(1, "program name")?.to_string();
                file.program_version = c.word(2, "program version").unwrap_or("").to_string();
            }
            "#GEN" => file.generated = c.date(1, "generation date")?,
            "#FNAMN" => file.company.name = c.word(1, "company name")?.to_string(),
            "#ORGNR" => {
                file.company.org_number = Some(c.word(1, "organisation number")?.to_string());
            }
            "#KPTYP" => file.chart = Some(c.word(1, "chart type")?.to_string()),
            "#RAR" => file.years.push(FiscalYear {
                index: c.int(1, "year index")?,
                start: c.date(2, "start date")?,
                end: c.date(3, "end date")?,
            }),
            "#KONTO" => {
                let number = c.account(1)?;
                let name = c.word(2, "account name")?.to_string();
                match file.accounts.iter_mut().find(|a| a.number == number) {
                    Some(a) => a.name = name,
                    None => file.accounts.push(Account {
                        number,
                        name,
                        kind: None,
                    }),
                }
            }
            "#KTYP" => {
                let number = c.account(1)?;
                let k = c.word(2, "account type")?;
                let Some(kind) = AccountKind::from_code(k) else {
                    return syn(ln, format!("invalid account type `{k}`"));
                };
                match file.accounts.iter_mut().find(|a| a.number == number) {
                    Some(a) => a.kind = Some(kind),
                    None => file.accounts.push(Account {
                        number,
                        name: String::new(),
                        kind: Some(kind),
                    }),
                }
            }
            tag @ ("#IB" | "#UB" | "#RES") => {
                let b = Balance {
                    year_index: c.int(1, "year index")?,
                    account: c.account(2)?,
                    amount_minor: c.amount(3)?,
                };
                match tag {
                    "#IB" => file.opening.push(b),
                    "#UB" => file.closing.push(b),
                    _ => file.results.push(b),
                }
            }
            "#VER" => {
                let series = c.word(1, "series")?.to_string();
                let number = c.word(2, "voucher number")?.to_string();
                let date = c.date(3, "voucher date")?;
                let vtext = match c.toks.get(4) {
                    Some(Tok::Word(w)) => w.clone(),
                    _ => String::new(),
                };
                let registered = match c.toks.get(5) {
                    Some(Tok::Word(w)) => match Date::from_compact(w) {
                        Some(d) => Some(d),
                        None => return syn(ln, format!("invalid registration date `{w}`")),
                    },
                    _ => None,
                };
                let signed = match c.toks.get(6) {
                    Some(Tok::Word(w)) if !w.is_empty() => Some(w.clone()),
                    _ => None,
                };
                // The opening brace is the next non-empty line.
                loop {
                    match lines.get(i) {
                        None => return syn(ln, "voucher has no `{` block"),
                        Some(l) if l.trim().is_empty() => i += 1,
                        Some(l) => {
                            if l.trim() != "{" {
                                return syn(i + 1, "expected `{` after #VER");
                            }
                            i += 1;
                            break;
                        }
                    }
                }
                let mut v = Voucher {
                    series,
                    number,
                    date,
                    text: vtext,
                    registered,
                    signed,
                    lines: Vec::new(),
                };
                loop {
                    let Some(raw) = lines.get(i) else {
                        return syn(ln, "voucher block is not closed with `}`");
                    };
                    let tln = i + 1;
                    i += 1;
                    let t = tokenize(raw, tln)?;
                    match t.first() {
                        None => continue,
                        Some(Tok::Close) => break,
                        Some(Tok::Word(l)) if l.eq_ignore_ascii_case("#TRANS") => {
                            v.lines.push(parse_trans(&Ctx {
                                line: tln,
                                toks: &t,
                            })?);
                        }
                        // #RTRANS, #BTRANS and unknown labels are ignored.
                        Some(Tok::Word(l)) if l.starts_with('#') => {}
                        _ => return syn(tln, "unexpected content inside a voucher block"),
                    }
                }
                let sum = v.sum_minor();
                if sum != 0 {
                    return Err(SieError::Unbalanced {
                        line: ln,
                        series: v.series,
                        number: v.number,
                        sum_minor: sum,
                    });
                }
                file.vouchers.push(v);
            }
            _ => {}
        }
    }
    if !sietyp_seen {
        return syn(1, "no #SIETYP 4 record");
    }
    Ok(file)
}

fn parse_trans(c: &Ctx<'_>) -> Result<Line, SieError> {
    let account = c.account(1)?;
    if c.toks.get(2) != Some(&Tok::Open) {
        return syn(c.line, "#TRANS needs an object list `{}` after the account");
    }
    let mut i = 3;
    let mut flat = Vec::new();
    loop {
        match c.toks.get(i) {
            Some(Tok::Word(w)) => flat.push(w.clone()),
            Some(Tok::Close) => break,
            _ => return syn(c.line, "object list is not closed with `}`"),
        }
        i += 1;
    }
    if flat.len() % 2 != 0 {
        return syn(c.line, "object list needs dimension and object in pairs");
    }
    let objects = flat
        .chunks(2)
        .map(|p| (p[0].clone(), p[1].clone()))
        .collect();
    let amount_minor = c.amount(i + 1)?;
    // Optional transaction date at i+2, optional text at i+3 (quantity and sign are ignored).
    let text = match c.toks.get(i + 3) {
        Some(Tok::Word(w)) => Some(w.clone()),
        _ => None,
    };
    Ok(Line {
        account,
        amount_minor,
        text,
        objects,
    })
}
