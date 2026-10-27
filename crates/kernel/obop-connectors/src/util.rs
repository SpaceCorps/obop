//! Small helpers: time, hashing, constant-time comparison, JSON access.

use serde_json::Value as J;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use subtle::ConstantTimeEq;

/// An insert into `model` failed with `error`: `Ok` only when the row is there (the same insert
/// made before), the error when it is not. A server error that is not a duplicate (a full disk, a
/// read-only server after a failover) was taken for "already there" by exclusion, and the row it
/// stood for was never written.
pub fn row_exists_or(
    backend: &dyn obop_storage::Backend,
    model: &str,
    id: &str,
    error: String,
) -> Result<(), String> {
    if obop_storage::fault::Fault::is_transient(&error) {
        return Err(error);
    }
    match backend.get(model, id)? {
        Some(_) => Ok(()),
        None => Err(error),
    }
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `2026-10-17T12:00:00Z`.
pub fn iso(ts: u64) -> String {
    let days = (ts / 86_400) as i64;
    let rem = ts % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// The inverse of [`iso`]; `None` for anything else.
pub fn parse_iso(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    if b.len() != 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'Z'
    {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d, h, mi, se) = (
        n(0..4)?,
        n(5..7)?,
        n(8..10)?,
        n(11..13)?,
        n(14..16)?,
        n(17..19)?,
    );
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || se > 60 {
        return None;
    }
    let days = days_from_civil(y, mo, d);
    u64::try_from(days * 86_400 + h * 3600 + mi * 60 + se).ok()
}

pub fn sha256_hex(data: &[u8]) -> String {
    crate::secretbox::hex(&Sha256::digest(data))
}

/// Constant-time equality of two byte strings (length differences are not hidden).
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.ct_eq(b).into()
}

pub fn random_hex(n: usize) -> String {
    let mut b = vec![0u8; n];
    getrandom::fill(&mut b).expect("operating system randomness");
    crate::secretbox::hex(&b)
}

pub fn str_of<'a>(m: &'a serde_json::Map<String, J>, k: &str) -> &'a str {
    m.get(k).and_then(J::as_str).unwrap_or("")
}

pub fn int_of(m: &serde_json::Map<String, J>, k: &str, default: i64) -> i64 {
    m.get(k).and_then(J::as_i64).unwrap_or(default)
}

pub fn bool_of(m: &serde_json::Map<String, J>, k: &str) -> bool {
    m.get(k).and_then(J::as_bool).unwrap_or(false)
}

/// A JSON object stored as text; an empty or invalid text is an empty object.
pub fn obj_of(m: &serde_json::Map<String, J>, k: &str) -> serde_json::Map<String, J> {
    match serde_json::from_str::<J>(str_of(m, k)) {
        Ok(J::Object(o)) => o,
        _ => serde_json::Map::new(),
    }
}

/// Look up a dotted path (`issue.title`, `items.0.id`) in a JSON value.
pub fn path<'a>(v: &'a J, path: &str) -> Option<&'a J> {
    let mut cur = v;
    if path.is_empty() {
        return Some(cur);
    }
    for part in path.split('.') {
        cur = match cur {
            J::Object(o) => o.get(part)?,
            J::Array(a) => a.get(part.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(cur)
}

/// A cell of a CSV file. Cells that a spreadsheet would run as a formula get a leading quote.
pub fn csv_cell(s: &str) -> String {
    let mut s = s.replace(['\r', '\n'], " ");
    if s.starts_with(['=', '+', '-', '@', '\t']) {
        s.insert(0, '\'');
    }
    if s.contains([',', '"']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

pub fn csv_row(cells: &[String]) -> String {
    cells
        .iter()
        .map(|c| csv_cell(c))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_round_trips() {
        for ts in [0, 86_399, 1_791_000_000, 4_102_444_800] {
            assert_eq!(parse_iso(&iso(ts)), Some(ts));
        }
        assert_eq!(iso(0), "1970-01-01T00:00:00Z");
        assert_eq!(parse_iso("2026-13-01T00:00:00Z"), None);
        assert_eq!(parse_iso("nope"), None);
    }

    #[test]
    fn csv_cells_cannot_carry_formulas() {
        assert_eq!(csv_cell("=1+1"), "'=1+1");
        assert_eq!(csv_cell("@x"), "'@x");
        assert_eq!(csv_cell("a,b"), "\"a,b\"");
        assert_eq!(csv_cell("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_cell("two\nlines"), "two lines");
        assert_eq!(csv_cell("plain"), "plain");
    }

    #[test]
    fn dotted_paths() {
        let v: J = serde_json::json!({"a": {"b": [10, {"c": "x"}]}});
        assert_eq!(path(&v, "a.b.1.c"), Some(&J::from("x")));
        assert_eq!(path(&v, "a.z"), None);
        assert_eq!(path(&v, ""), Some(&v));
    }

    #[test]
    fn comparison_is_constant_time_shaped() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"ab"));
    }
}
