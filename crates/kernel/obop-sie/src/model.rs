//! The plain data model.

/// A calendar date (validated month and day; leap years honoured).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

impl Date {
    /// A valid date, or `None`.
    pub fn new(year: u16, month: u8, day: u8) -> Option<Date> {
        let leap =
            year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
        let max = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => return None,
        };
        (year >= 1 && (1..=max).contains(&day)).then_some(Date { year, month, day })
    }

    /// `YYYYMMDD`.
    pub fn compact(&self) -> String {
        format!("{:04}{:02}{:02}", self.year, self.month, self.day)
    }

    /// Parse `YYYYMMDD`.
    pub fn from_compact(s: &str) -> Option<Date> {
        if s.len() != 8 || !s.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Date::new(
            s[0..4].parse().ok()?,
            s[4..6].parse().ok()?,
            s[6..8].parse().ok()?,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Company {
    pub name: String,
    pub org_number: Option<String>,
}

/// `#RAR`: index 0 is the current year, -1 the previous one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiscalYear {
    pub index: i32,
    pub start: Date,
    pub end: Date,
}

/// `#KTYP`: T asset, S liability, K cost, I income.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountKind {
    Asset,
    Liability,
    Cost,
    Income,
}

impl AccountKind {
    pub fn code(self) -> char {
        match self {
            AccountKind::Asset => 'T',
            AccountKind::Liability => 'S',
            AccountKind::Cost => 'K',
            AccountKind::Income => 'I',
        }
    }
    pub fn from_code(s: &str) -> Option<AccountKind> {
        match s {
            "T" => Some(AccountKind::Asset),
            "S" => Some(AccountKind::Liability),
            "K" => Some(AccountKind::Cost),
            "I" => Some(AccountKind::Income),
            _ => None,
        }
    }
    /// Balance-sheet accounts carry `#IB`/`#UB`, profit-and-loss accounts `#RES`.
    pub fn is_balance_sheet(self) -> bool {
        matches!(self, AccountKind::Asset | AccountKind::Liability)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub number: u32,
    pub name: String,
    pub kind: Option<AccountKind>,
}

/// One `#IB`, `#UB` or `#RES` row: signed minor units (debit positive).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Balance {
    pub year_index: i32,
    pub account: u32,
    pub amount_minor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub account: u32,
    /// Signed minor units: debit positive, credit negative.
    pub amount_minor: i64,
    pub text: Option<String>,
    /// Object list `{dimension object ...}`, empty for `{}`.
    pub objects: Vec<(String, String)>,
}

impl Line {
    pub fn new(account: u32, amount_minor: i64, text: Option<&str>) -> Line {
        Line {
            account,
            amount_minor,
            text: text.map(str::to_string),
            objects: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Voucher {
    pub series: String,
    pub number: String,
    pub date: Date,
    pub text: String,
    pub registered: Option<Date>,
    pub signed: Option<String>,
    pub lines: Vec<Line>,
}

impl Voucher {
    /// Sum of all line amounts; 0 for a balanced voucher.
    pub fn sum_minor(&self) -> i128 {
        self.lines.iter().map(|l| i128::from(l.amount_minor)).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SieFile {
    pub program: String,
    pub program_version: String,
    pub generated: Date,
    pub company: Company,
    /// Chart of accounts label for `#KPTYP`, for example `BAS2014`.
    pub chart: Option<String>,
    pub years: Vec<FiscalYear>,
    pub accounts: Vec<Account>,
    pub opening: Vec<Balance>,
    /// Explicit `#UB`; when empty, [`crate::export()`] derives year 0 from opening plus vouchers.
    pub closing: Vec<Balance>,
    /// Explicit `#RES`; when empty, [`crate::export()`] derives year 0 from the vouchers.
    pub results: Vec<Balance>,
    pub vouchers: Vec<Voucher>,
}

impl SieFile {
    /// An empty file for one fiscal year (index 0).
    pub fn new(company: &str, start: Date, end: Date) -> SieFile {
        SieFile {
            program: "obop".into(),
            program_version: env!("CARGO_PKG_VERSION").into(),
            generated: start,
            company: Company {
                name: company.into(),
                org_number: None,
            },
            chart: None,
            years: vec![FiscalYear {
                index: 0,
                start,
                end,
            }],
            accounts: Vec::new(),
            opening: Vec::new(),
            closing: Vec::new(),
            results: Vec::new(),
            vouchers: Vec::new(),
        }
    }
}
