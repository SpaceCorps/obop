//! The value types behind the `decimal` and `datetime` field kinds (D72): exact decimals, instants
//! with microsecond precision, the pinned time zone database and calendar buckets. No other
//! crate parses or prints these by hand, so every surface (REST, MCP, CLI, imports, queries,
//! aggregates, expressions) agrees on what a valid value is and what its canonical text is.

pub mod datetime;
pub mod decimal;
pub mod zone;

pub use datetime::{Bucket, DatetimeError, Instant};
pub use decimal::{Decimal, DecimalError, MAX_PRECISION, MAX_SCALE, Round};
pub use zone::{Zone, ZoneDb, ZoneError, Zones};
