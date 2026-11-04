//! SIE 4 export and import (the Swedish accounting interchange format).
//!
//! A plain, dependency-free library: a data model ([`SieFile`]), [`export()`] to SIE type 4 in
//! CP437 with CRLF line ends, and [`parse()`] back. Money is `i64` minor units (öre), never
//! floats.
//!
//! The export is ported from `SpaceCorps/open-accounting-sweden` (`oas-core` `to_cp437`,
//! `sie_quote`, `format_amount`; `oas-server` `export_sie`), MIT licensed, our own code,
//! re-licensed here under Apache-2.0 by the same owner. The importer is new.
//!
//! The record layout follows the commonly documented SIE 4 format and has NOT been checked
//! against the official 4B/4C specification. Not covered: `#DIM`/`#OBJEKT` declarations,
//! `#SRU`, `#VALUTA`, quantities, `#RTRANS`/`#BTRANS` (ignored on import), `#KSUMMA`.
//!
//! ```
//! use obop_sie::{export, parse, Date, Line, SieFile, Voucher};
//! let mut f = SieFile::new("Exempel AB", Date::new(2026, 1, 1).unwrap(), Date::new(2026, 12, 31).unwrap());
//! f.vouchers.push(Voucher {
//!     series: "A".into(), number: "1".into(), date: Date::new(2026, 3, 1).unwrap(),
//!     text: "Kaffe".into(), registered: None, signed: None,
//!     lines: vec![
//!         Line::new(1930, -12550, None),
//!         Line::new(6071, 12550, None),
//!     ],
//! });
//! let bytes = export(&f);
//! assert_eq!(export(&parse(&bytes).unwrap()), bytes);
//! ```

mod cp437;
mod export;
mod model;
mod parse;

pub use cp437::{decode as decode_cp437, encode as encode_cp437};
pub use export::{export, format_amount, quote};
pub use model::{Account, AccountKind, Balance, Company, Date, FiscalYear, Line, SieFile, Voucher};
pub use parse::{SieError, parse, parse_amount};
