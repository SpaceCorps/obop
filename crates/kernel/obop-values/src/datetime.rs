//! Instants for the `datetime` field kind (D72), and the calendar arithmetic shared with `date`.
//!
//! An [`Instant`] is a point on the timeline with microsecond precision, held as microseconds
//! since 1970-01-01T00:00:00Z. Its text form is always UTC and always the same shape,
//! `YYYY-MM-DDTHH:MM:SS.ffffffZ` (six fractional digits even when they are zero), so the stored
//! strings of two instants sort exactly as the instants do. Input is RFC 3339 with an explicit
//! offset or `Z`; the offset is applied and then forgotten.
//!
//! Strictness: only `T` and `Z` in upper case, offsets as `+HH:MM` or `-HH:MM`, a real calendar
//! date, 00 to 23 hours, 00 to 59 minutes and 00 to 59 seconds. A leap second (`:60`) is refused:
//! the timeline here is the Unix one, which has no 61st second; send `:59` or the next second.
//! At most six fractional digits (a microsecond is the resolution). Years 0001 to 9999.

use std::fmt;

/// Microseconds from the epoch to 0001-01-01T00:00:00Z.
pub const MIN_MICROS: i64 = -62_135_596_800 * 1_000_000;
/// Microseconds from the epoch to 9999-12-31T23:59:59.999999Z.
pub const MAX_MICROS: i64 = 253_402_300_799 * 1_000_000 + 999_999;
const MICROS_PER_SEC: i64 = 1_000_000;
const MICROS_PER_DAY: i64 = 86_400 * MICROS_PER_SEC;

/// Why a text is not an instant. `Display` is appended to "`model.field` must be ...".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatetimeError(pub String);

impl fmt::Display for DatetimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DatetimeError {}

fn err<T>(s: impl Into<String>) -> Result<T, DatetimeError> {
    Err(DatetimeError(s.into()))
}

// ---- the calendar -------------------------------------------------------------------------

pub fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(y) => 29,
        _ => 28,
    }
}

/// Days from 1970-01-01 to a proleptic Gregorian date.
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The date `days` after 1970-01-01.
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Day of the week of a day count, Monday 0 to Sunday 6 (1970-01-01 was a Thursday).
pub fn weekday(days: i64) -> i64 {
    (days + 3).rem_euclid(7)
}

/// How a date or an instant is cut into periods for grouping and filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bucket {
    /// `YYYY-MM-DD`
    Day,
    /// The Monday of the week, `YYYY-MM-DD`.
    Week,
    /// The ISO 8601 week, `YYYY-Www` (the year is the ISO week-year, which may differ from the
    /// calendar year in the first and last days of January and December).
    IsoWeek,
    /// `YYYY-MM`
    Month,
    /// `YYYY`
    Year,
}

impl Bucket {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "day" => Some(Self::Day),
            "week" => Some(Self::Week),
            "iso_week" => Some(Self::IsoWeek),
            "month" => Some(Self::Month),
            "year" => Some(Self::Year),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
            Self::IsoWeek => "iso_week",
            Self::Month => "month",
            Self::Year => "year",
        }
    }

    /// The names, for error messages.
    pub const NAMES: &'static str = "day, week, iso_week, month or year";
}

/// The label of the period a calendar date falls in. Labels sort in time order as text.
pub fn bucket_label(y: i64, m: i64, d: i64, b: Bucket) -> String {
    match b {
        Bucket::Day => format!("{y:04}-{m:02}-{d:02}"),
        Bucket::Month => format!("{y:04}-{m:02}"),
        Bucket::Year => format!("{y:04}"),
        Bucket::Week => {
            let days = days_from_civil(y, m, d);
            let (wy, wm, wd) = civil_from_days(days - weekday(days));
            format!("{wy:04}-{wm:02}-{wd:02}")
        }
        Bucket::IsoWeek => {
            let days = days_from_civil(y, m, d);
            // The week belongs to the year of its Thursday.
            let thursday = days - weekday(days) + 3;
            let (ty, _, _) = civil_from_days(thursday);
            let week = (thursday - days_from_civil(ty, 1, 1)) / 7 + 1;
            format!("{ty:04}-W{week:02}")
        }
    }
}

/// Whether `s` is `YYYY-MM-DD` and a real calendar date, and the parts.
pub fn parse_date(s: &str) -> Option<(i64, i64, i64)> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let num = |r: std::ops::Range<usize>| -> Option<i64> {
        let part = s.get(r)?;
        if part.bytes().all(|c| c.is_ascii_digit()) {
            part.parse().ok()
        } else {
            None
        }
    };
    let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    ((1..=12).contains(&m) && y >= 1 && (1..=days_in_month(y, m)).contains(&d)).then_some((y, m, d))
}

// ---- instants -----------------------------------------------------------------------------

/// A point in time, microseconds since the epoch, between 0001-01-01 and 9999-12-31.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instant(i64);

impl Instant {
    pub fn from_micros(us: i64) -> Option<Self> {
        (MIN_MICROS..=MAX_MICROS).contains(&us).then_some(Self(us))
    }

    pub fn from_secs(s: i64) -> Option<Self> {
        s.checked_mul(MICROS_PER_SEC).and_then(Self::from_micros)
    }

    pub fn micros(self) -> i64 {
        self.0
    }

    /// Parse RFC 3339 as described in the module notes.
    pub fn parse(s: &str) -> Result<Self, DatetimeError> {
        let b = s.as_bytes();
        let shape = "an RFC 3339 instant with an offset or Z, such as 2026-03-29T01:30:00Z";
        if b.len() < 20 {
            return err(if b.len() >= 19 && b[..19].iter().all(|c| *c != b'Z') {
                "it has no offset: end it with Z or +HH:MM / -HH:MM".to_string()
            } else {
                format!("it is not {shape}")
            });
        }
        if b.len() > 40 {
            return err(format!("it is not {shape}"));
        }
        let digits = |from: usize, n: usize| -> Option<i64> {
            let part = b.get(from..from + n)?;
            if part.iter().all(u8::is_ascii_digit) {
                Some(part.iter().fold(0i64, |a, c| a * 10 + i64::from(c - b'0')))
            } else {
                None
            }
        };
        let sep = |at: usize, c: u8| b.get(at) == Some(&c);
        let (Some(y), Some(mo), Some(d), Some(h), Some(mi), Some(sec)) = (
            digits(0, 4),
            digits(5, 2),
            digits(8, 2),
            digits(11, 2),
            digits(14, 2),
            digits(17, 2),
        ) else {
            return err(format!("it is not {shape}"));
        };
        if !(sep(4, b'-') && sep(7, b'-') && sep(10, b'T') && sep(13, b':') && sep(16, b':')) {
            return err(format!(
                "it is not {shape} (the date and time are joined by an upper case T)"
            ));
        }
        if y < 1 {
            return err("the year must be 0001 or later");
        }
        if !(1..=12).contains(&mo) || d < 1 || d > days_in_month(y, mo) {
            return err(format!("{y:04}-{mo:02}-{d:02} is not a real calendar date"));
        }
        if h > 23 || mi > 59 {
            return err("the hour must be 00 to 23 and the minute 00 to 59");
        }
        if sec == 60 {
            return err("a leap second (:60) is not accepted; send :59.999999 or the next second");
        }
        if sec > 59 {
            return err("the second must be 00 to 59");
        }
        let mut i = 19;
        let mut frac = 0i64;
        if b[i] == b'.' {
            let start = i + 1;
            let mut n = 0;
            while b.get(start + n).is_some_and(u8::is_ascii_digit) {
                n += 1;
            }
            if n == 0 {
                return err("a point must be followed by fractional digits");
            }
            if n > 6 {
                return err(
                    "it has more than 6 fractional digits; the resolution is a microsecond",
                );
            }
            frac = digits(start, n).unwrap_or(0) * 10i64.pow(6 - n as u32);
            i = start + n;
        }
        let offset_secs = match b.get(i..) {
            Some([b'Z']) => 0,
            Some([sign @ (b'+' | b'-'), rest @ ..]) if rest.len() == 5 && rest[2] == b':' => {
                let part = |r: &[u8]| -> Option<i64> {
                    r.iter()
                        .all(u8::is_ascii_digit)
                        .then(|| r.iter().fold(0i64, |a, c| a * 10 + i64::from(c - b'0')))
                };
                let (Some(oh), Some(om)) = (part(&rest[..2]), part(&rest[3..])) else {
                    return err(format!("it is not {shape}"));
                };
                if oh > 23 || om > 59 {
                    return err("the offset must be within +-23:59");
                }
                let secs = oh * 3600 + om * 60;
                if *sign == b'-' { -secs } else { secs }
            }
            _ => {
                return err(if b.get(i..).is_some_and(|r| r.is_empty()) {
                    "it has no offset: end it with Z or +HH:MM / -HH:MM".to_string()
                } else if matches!(b.get(i), Some(b'z')) {
                    "write the Z in upper case".to_string()
                } else {
                    format!("the offset must be Z or +HH:MM / -HH:MM; it is not {shape}")
                });
            }
        };
        let local = (days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + sec - offset_secs)
            * MICROS_PER_SEC
            + frac;
        match Self::from_micros(local) {
            Some(t) => Ok(t),
            None => err("it is outside the years 0001 to 9999 once the offset is applied"),
        }
    }

    /// The canonical text: UTC, `YYYY-MM-DDTHH:MM:SS.ffffffZ`.
    pub fn canonical(self) -> String {
        let days = self.0.div_euclid(MICROS_PER_DAY);
        let rem = self.0.rem_euclid(MICROS_PER_DAY);
        let (y, m, d) = civil_from_days(days);
        let secs = rem / MICROS_PER_SEC;
        format!(
            "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:06}Z",
            secs / 3600,
            secs % 3600 / 60,
            secs % 60,
            rem % MICROS_PER_SEC
        )
    }

    /// The calendar date and the local time of day when the clock is `offset_secs` ahead of UTC.
    pub fn civil_at(self, offset_secs: i32) -> (i64, i64, i64) {
        let local = self.0 + i64::from(offset_secs) * MICROS_PER_SEC;
        civil_from_days(local.div_euclid(MICROS_PER_DAY))
    }
}

impl fmt::Display for Instant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical())
    }
}

/// Parse `s` and write it in the canonical form.
pub fn canonical(s: &str) -> Result<String, DatetimeError> {
    Instant::parse(s).map(Instant::canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(s: &str) -> Instant {
        Instant::parse(s).unwrap_or_else(|e| panic!("{s}: {e}"))
    }

    #[test]
    fn the_canonical_form_is_utc_with_six_digits() {
        for (src, out) in [
            ("2026-03-29T01:30:00Z", "2026-03-29T01:30:00.000000Z"),
            ("2026-03-29T01:30:00.5Z", "2026-03-29T01:30:00.500000Z"),
            ("2026-03-29T01:30:00.123456Z", "2026-03-29T01:30:00.123456Z"),
            ("2026-03-29T03:30:00+02:00", "2026-03-29T01:30:00.000000Z"),
            ("2026-03-28T22:00:00-03:30", "2026-03-29T01:30:00.000000Z"),
            ("2026-03-29T01:30:00-00:00", "2026-03-29T01:30:00.000000Z"),
            ("1970-01-01T00:00:00Z", "1970-01-01T00:00:00.000000Z"),
            ("0001-01-01T00:00:00Z", "0001-01-01T00:00:00.000000Z"),
            ("9999-12-31T23:59:59.999999Z", "9999-12-31T23:59:59.999999Z"),
            ("2024-02-29T12:00:00Z", "2024-02-29T12:00:00.000000Z"),
            ("1969-12-31T23:59:59.000001Z", "1969-12-31T23:59:59.000001Z"),
            ("2026-01-01T00:00:00+14:00", "2025-12-31T10:00:00.000000Z"),
        ] {
            assert_eq!(canonical(src).as_deref(), Ok(out), "{src}");
            assert_eq!(canonical(out).as_deref(), Ok(out), "idempotent: {out}");
        }
    }

    #[test]
    fn the_spring_and_autumn_vectors_are_distinct_instants() {
        // The hour that does not exist in local time round-trips.
        assert_eq!(
            canonical("2026-03-29T01:30:00Z").unwrap(),
            "2026-03-29T01:30:00.000000Z"
        );
        let (a, b) = (ok("2026-10-25T00:30:00Z"), ok("2026-10-25T01:30:00Z"));
        assert!(a < b);
        assert_eq!(b.micros() - a.micros(), 3_600_000_000);
    }

    #[test]
    fn ordering_follows_the_instant_not_the_text() {
        let a = ok("2026-06-01T12:00:00+05:00"); // 07:00Z
        let b = ok("2026-06-01T09:00:00+01:00"); // 08:00Z
        let c = ok("2026-06-01T07:30:00Z");
        assert!(a < c && c < b);
        assert!("2026-06-01T12:00:00+05:00" > "2026-06-01T09:00:00+01:00");
    }

    #[test]
    fn the_canonical_strings_sort_like_the_instants() {
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let mut v: Vec<Instant> = (0..2000)
            .map(|_| {
                let r = next() as i64;
                let us = MIN_MICROS + r.rem_euclid(MAX_MICROS - MIN_MICROS);
                Instant::from_micros(us).unwrap()
            })
            .collect();
        v.sort();
        let mut s: Vec<String> = v.iter().map(|i| i.canonical()).collect();
        let sorted = s.clone();
        s.sort();
        assert_eq!(s, sorted);
        for i in &v {
            assert_eq!(Instant::parse(&i.canonical()), Ok(*i));
        }
    }

    #[test]
    fn hostile_text_is_refused() {
        for (src, want) in [
            ("2026-02-30T10:00:00Z", "not a real calendar date"),
            ("2026-13-01T00:00:00Z", "not a real calendar date"),
            ("2026-00-10T00:00:00Z", "not a real calendar date"),
            ("2025-02-29T00:00:00Z", "not a real calendar date"),
            ("2026-03-29T01:30:00", "no offset"),
            ("2026-03-29T01:30:00.5", "no offset"),
            ("2026-03-29T25:30:00Z", "hour"),
            ("2026-03-29T01:60:00Z", "minute"),
            ("2026-12-31T23:59:60Z", "leap second"),
            ("2026-03-29T01:30:61Z", "second"),
            ("2026-03-29 01:30:00Z", "upper case T"),
            ("2026-03-29t01:30:00Z", "upper case T"),
            ("2026-03-29T01:30:00z", "upper case"),
            ("2026-03-29T01:30:00+0200", "offset"),
            ("2026-03-29T01:30:00+2:00", "offset"),
            ("2026-03-29T01:30:00+24:00", "offset"),
            ("2026-03-29T01:30:00+02:60", "offset"),
            ("2026-03-29T01:30:00.Z", "fractional"),
            ("2026-03-29T01:30:00.1234567Z", "more than 6"),
            ("2026-03-29T01:30:00.123456789Z", "more than 6"),
            ("0000-01-01T00:00:00Z", "0001"),
            ("0001-01-01T00:00:00+01:00", "outside"),
            ("9999-12-31T23:59:59-01:00", "outside"),
            ("2026-03-29T01:30:00Z ", "offset"),
            (" 2026-03-29T01:30:00Z", "not an RFC 3339"),
            ("2026-03-29", "not an RFC 3339"),
            ("2026-3-29T01:30:00Z", "not an RFC 3339"),
            ("20260329T013000Z", "not an RFC 3339"),
            ("2026-03-29T01:30:00Z\n", "offset"),
            ("2026-03-29T01:30:00\u{ff1a}00Z", "not an RFC 3339"),
            ("", "not an RFC 3339"),
            ("now", "not an RFC 3339"),
            ("1e9", "not an RFC 3339"),
        ] {
            let e = Instant::parse(src).unwrap_err().to_string();
            assert!(e.contains(want), "{src:?} -> {e}");
        }
    }

    #[test]
    fn buckets_are_calendar_periods() {
        let l = |s: &str, b| {
            let (y, m, d) = parse_date(s).unwrap();
            bucket_label(y, m, d, b)
        };
        assert_eq!(l("2026-10-17", Bucket::Day), "2026-10-17");
        assert_eq!(l("2026-10-17", Bucket::Week), "2026-10-12");
        assert_eq!(l("2026-10-18", Bucket::Week), "2026-10-12");
        assert_eq!(l("2026-10-19", Bucket::Week), "2026-10-19");
        assert_eq!(l("2026-01-01", Bucket::Week), "2025-12-29");
        assert_eq!(l("2026-10-17", Bucket::Month), "2026-10");
        assert_eq!(l("2026-10-17", Bucket::Year), "2026");
        // ISO weeks: the week of the Thursday decides the year.
        assert_eq!(l("2026-01-01", Bucket::IsoWeek), "2026-W01");
        assert_eq!(l("2024-12-30", Bucket::IsoWeek), "2025-W01");
        assert_eq!(l("2027-01-01", Bucket::IsoWeek), "2026-W53");
        assert_eq!(l("2021-01-03", Bucket::IsoWeek), "2020-W53");
        assert_eq!(l("2026-10-25", Bucket::IsoWeek), "2026-W43");
        assert_eq!(l("2026-10-26", Bucket::IsoWeek), "2026-W44");
        assert_eq!(l("2020-12-31", Bucket::IsoWeek), "2020-W53");
        assert_eq!(l("2019-12-30", Bucket::IsoWeek), "2020-W01");
    }

    #[test]
    fn days_and_dates_round_trip() {
        for days in (-719_000..2_900_000).step_by(37) {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
            assert!(parse_date(&format!("{y:04}-{m:02}-{d:02}")).is_some() || y < 1);
        }
        assert!(parse_date("2024-02-29").is_some());
        assert!(parse_date("2023-02-29").is_none());
        assert!(parse_date("1900-02-29").is_none());
        assert!(parse_date("2000-02-29").is_some());
        assert_eq!(weekday(days_from_civil(2026, 10, 25)), 6, "a Sunday");
    }
}
