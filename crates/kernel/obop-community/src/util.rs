//! Small helpers: time, hashing, escaping, slugs and constant-time comparison.

use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_hex(s: &str) -> String {
    hex(&Sha256::digest(s.as_bytes()))
}

/// 256 random bits as hex.
pub fn random_token() -> String {
    let mut b = [0u8; 32];
    // If the operating system cannot supply randomness there is no safe fallback: return an
    // empty token, which callers treat as an error (sessions fail closed).
    if getrandom::fill(&mut b).is_err() {
        return String::new();
    }
    hex(&b)
}

/// Constant-time equality of two strings (length is not secret here).
pub fn ct_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut d = 0u8;
    for (x, y) in a.iter().zip(b) {
        d |= x ^ y;
    }
    d == 0
}

pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            c => o.push(c),
        }
    }
    o
}

/// Percent-encode a value for use in a URL query component.
pub fn urlenc(s: &str) -> String {
    form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// ASCII slug of a title: lowercase, accents folded for common Latin letters, single hyphens,
/// at most 60 characters. Never empty.
pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut dash = true;
    for c in s.chars() {
        let folded: &str = match c {
            'ä' | 'Ä' | 'à' | 'á' | 'â' | 'ã' | 'å' | 'Å' | 'ą' | 'À' | 'Á' | 'Â' | 'Ã' => {
                "a"
            }
            'ö' | 'Ö' | 'ò' | 'ó' | 'ô' | 'õ' | 'ø' | 'Ø' | 'Ó' => "o",
            'ü' | 'Ü' | 'ù' | 'ú' | 'û' | 'Ú' => "u",
            'é' | 'è' | 'ê' | 'ë' | 'ę' | 'É' | 'È' | 'Ê' => "e",
            'í' | 'ì' | 'î' | 'ï' | 'Í' => "i",
            'ç' | 'ć' | 'č' | 'Ć' | 'Ç' => "c",
            'ñ' | 'ń' | 'Ñ' | 'Ń' => "n",
            'ß' => "ss",
            'ł' | 'Ł' => "l",
            'ś' | 'š' | 'Ś' => "s",
            'ź' | 'ż' | 'ž' | 'Ź' | 'Ż' => "z",
            'ý' | 'ÿ' => "y",
            _ => "",
        };
        if !folded.is_empty() {
            out.push_str(folded);
            dash = false;
        } else if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash {
            out.push('-');
            dash = true;
        }
    }
    let t = out.trim_matches('-');
    let t: String = t.chars().take(60).collect();
    let t = t.trim_end_matches('-').to_string();
    if t.is_empty() { "thread".into() } else { t }
}

// ---------- calendar maths (proleptic Gregorian, UTC) ----------

pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
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

pub struct Civil {
    pub year: i64,
    pub month: i64,
    pub day: i64,
    pub hour: u64,
    pub minute: u64,
    pub second: u64,
    /// 0 = Sunday.
    pub weekday: usize,
}

pub fn civil(ts: u64) -> Civil {
    let days = (ts / 86_400) as i64;
    let (year, month, day) = civil_from_days(days);
    let s = ts % 86_400;
    Civil {
        year,
        month,
        day,
        hour: s / 3600,
        minute: s % 3600 / 60,
        second: s % 60,
        weekday: ((days + 4).rem_euclid(7)) as usize,
    }
}

/// `2026-10-16`
pub fn iso_date(ts: u64) -> String {
    let c = civil(ts);
    format!("{:04}-{:02}-{:02}", c.year, c.month, c.day)
}

/// `2026-10-16T12:00:00Z`
pub fn rfc3339(ts: u64) -> String {
    let c = civil(ts);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        c.year, c.month, c.day, c.hour, c.minute, c.second
    )
}

/// `2026-10-16T12:00:00.000Z`, the shape Statuspage uses.
pub fn rfc3339_ms(ts: u64) -> String {
    let c = civil(ts);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.000Z",
        c.year, c.month, c.day, c.hour, c.minute, c.second
    )
}

const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// IMF-fixdate for `Last-Modified`.
pub fn http_date(ts: u64) -> String {
    let c = civil(ts);
    format!(
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
        DAYS[c.weekday],
        c.day,
        MONTHS[(c.month - 1) as usize],
        c.year,
        c.hour,
        c.minute,
        c.second
    )
}

/// Parse an IMF-fixdate (`Sun, 06 Nov 1994 08:49:37 GMT`).
pub fn parse_http_date(s: &str) -> Option<u64> {
    let s = s.trim();
    let rest = s.split_once(", ")?.1;
    let mut it = rest.split_whitespace();
    let day: i64 = it.next()?.parse().ok()?;
    let mname = it.next()?;
    let mon = MONTHS.iter().position(|m| *m == mname)? as i64 + 1;
    let year: i64 = it.next()?.parse().ok()?;
    let mut t = it.next()?.split(':');
    let (h, mi, se): (u64, u64, u64) = (
        t.next()?.parse().ok()?,
        t.next()?.parse().ok()?,
        t.next()?.parse().ok()?,
    );
    if it.next()? != "GMT" || !(1..=31).contains(&day) || h > 23 || mi > 59 || se > 60 {
        return None;
    }
    let days = days_from_civil(year, mon, day);
    u64::try_from(days)
        .ok()
        .map(|d| d * 86_400 + h * 3600 + mi * 60 + se)
}

/// Seconds at 00:00 UTC of the day containing `ts`.
pub fn day_start(ts: u64) -> u64 {
    ts - ts % 86_400
}

/// Strip control characters and trim; used on every single-line text input.
pub fn clean_line(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Clamp text to at most `n` characters (on a character boundary).
pub fn truncate_chars(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(n.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_fold_and_bound() {
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("  Größe & Ärger  "), "grosse-arger");
        assert_eq!(slugify("Łódź żółw"), "lodz-zolw");
        assert_eq!(slugify("日本語"), "thread");
        assert_eq!(slugify("---"), "thread");
        let long = slugify(&"word ".repeat(50));
        assert!(long.len() <= 60 && !long.ends_with('-'));
    }

    #[test]
    fn dates_round_trip() {
        assert_eq!(iso_date(0), "1970-01-01");
        assert_eq!(rfc3339(1_791_936_000), "2026-10-14T00:00:00Z");
        let ts = 1_791_936_000 + 3 * 3600 + 7 * 60 + 9;
        assert_eq!(parse_http_date(&http_date(ts)), Some(ts));
        assert_eq!(http_date(784_111_777), "Sun, 06 Nov 1994 08:49:37 GMT");
        assert_eq!(parse_http_date("garbage"), None);
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29));
    }

    #[test]
    fn constant_time_eq() {
        assert!(ct_eq("abc", "abc"));
        assert!(!ct_eq("abc", "abd") && !ct_eq("abc", "ab"));
    }

    #[test]
    fn escaping() {
        assert_eq!(
            esc("<a href=\"x\">&'"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
        assert_eq!(truncate_chars("abcdef", 4), "abc…");
    }
}
