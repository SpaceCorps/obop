//! RFC 6238 TOTP in the profile every authenticator app supports: HMAC-SHA-1, 6 digits, 30 s.
//! SHA-1 is used only inside HMAC, where RFC 2104 and RFC 4226 keep it sound; the profile is
//! fixed because Google Authenticator, Authy, 1Password and the others implement only that one.

use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;

pub const PERIOD: u64 = 30;
pub const DIGITS: u32 = 6;
/// Steps either side of the current one that still verify (clock drift of up to a period).
pub const WINDOW: u64 = 1;
/// 160 bits, the length RFC 4226 recommends.
pub const SECRET_LEN: usize = 20;

/// RFC 4226 HOTP with dynamic truncation.
pub fn hotp(secret: &[u8], counter: u64, digits: u32) -> u32 {
    let mut mac =
        <Hmac<Sha1> as KeyInit>::new_from_slice(secret).expect("HMAC takes any key length");
    mac.update(&counter.to_be_bytes());
    let h = mac.finalize().into_bytes();
    let off = (h[h.len() - 1] & 0x0f) as usize;
    let bin = u32::from_be_bytes([h[off] & 0x7f, h[off + 1], h[off + 2], h[off + 3]]);
    bin % 10u32.pow(digits)
}

pub fn step_at(unix_secs: u64) -> u64 {
    unix_secs / PERIOD
}

/// The 6-digit code for one time step.
pub fn code_for_step(secret: &[u8], step: u64) -> String {
    format!(
        "{:0width$}",
        hotp(secret, step, DIGITS),
        width = DIGITS as usize
    )
}

/// Constant-time equality of two byte strings (length is not secret here).
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Check `code` against the steps around `now`. Returns the step that matched, only if it is
/// newer than `last_used` (replay protection: a code accepted once is never accepted again, and
/// neither is an older code once a newer one was used). All window steps are always compared.
pub fn verify(secret: &[u8], code: &str, now: u64, last_used: Option<u64>) -> Option<u64> {
    let code = code.trim().replace([' ', '-'], "");
    if code.len() != DIGITS as usize || !code.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let cur = step_at(now);
    let mut found = None;
    for step in cur.saturating_sub(WINDOW)..=cur + WINDOW {
        let ok = ct_eq(code_for_step(secret, step).as_bytes(), code.as_bytes());
        if ok && last_used.is_none_or(|l| step > l) {
            found = Some(step);
        }
    }
    found
}

const B32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// RFC 4648 base32 without padding, the form authenticator apps take.
pub fn base32_encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    let (mut buf, mut bits) = (0u32, 0u32);
    for &b in bytes {
        buf = (buf << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            out.push(B32[((buf >> (bits - 5)) & 31) as usize] as char);
            bits -= 5;
        }
        buf &= (1 << bits) - 1;
    }
    if bits > 0 {
        out.push(B32[((buf << (5 - bits)) & 31) as usize] as char);
    }
    out
}

pub fn base32_decode(s: &str) -> Option<Vec<u8>> {
    let (mut buf, mut bits) = (0u32, 0u32);
    let mut out = Vec::new();
    for c in s.chars().filter(|c| !matches!(c, ' ' | '-' | '=')) {
        let v = B32
            .iter()
            .position(|&b| b as char == c.to_ascii_uppercase())? as u32;
        buf = (buf << 5) | v;
        bits += 5;
        if bits >= 8 {
            out.push((buf >> (bits - 8)) as u8);
            bits -= 8;
        }
        buf &= (1 << bits) - 1;
    }
    Some(out)
}

fn pct(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The `otpauth://` URI authenticator apps read from a QR code (Key URI Format).
pub fn otpauth_uri(issuer: &str, account: &str, secret_b32: &str) -> String {
    format!(
        "otpauth://totp/{}:{}?secret={secret_b32}&issuer={}&algorithm=SHA1&digits={DIGITS}&period={PERIOD}",
        pct(issuer),
        pct(account),
        pct(issuer)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const RFC_SECRET: &[u8] = b"12345678901234567890";

    #[test]
    fn rfc_6238_sha1_test_vectors() {
        // Appendix B of RFC 6238, SHA-1 column (8 digits), and their 6-digit truncations.
        for (t, eight) in [
            (59u64, 94287082u32),
            (1111111109, 7081804),
            (1111111111, 14050471),
            (1234567890, 89005924),
            (2000000000, 69279037),
            (20000000000, 65353130),
        ] {
            assert_eq!(hotp(RFC_SECRET, step_at(t), 8), eight, "T={t}");
            assert_eq!(
                code_for_step(RFC_SECRET, step_at(t)),
                format!("{:06}", eight % 1_000_000)
            );
        }
    }

    #[test]
    fn rfc_4226_hotp_vectors() {
        let want = [
            755224, 287082, 359152, 969429, 338314, 254676, 287922, 162583,
        ];
        for (i, w) in want.iter().enumerate() {
            assert_eq!(hotp(RFC_SECRET, i as u64, 6), *w);
        }
    }

    #[test]
    fn the_window_is_one_step_either_side() {
        let now = 1_700_000_000;
        let s = step_at(now);
        for (step, ok) in [
            (s - 2, false),
            (s - 1, true),
            (s, true),
            (s + 1, true),
            (s + 2, false),
        ] {
            let code = code_for_step(RFC_SECRET, step);
            assert_eq!(verify(RFC_SECRET, &code, now, None).is_some(), ok, "{step}");
        }
    }

    #[test]
    fn a_code_cannot_be_replayed_and_older_ones_die_with_it() {
        let now = 1_700_000_000;
        let s = step_at(now);
        let code = code_for_step(RFC_SECRET, s);
        let used = verify(RFC_SECRET, &code, now, None).unwrap();
        assert_eq!(verify(RFC_SECRET, &code, now, Some(used)), None);
        let older = code_for_step(RFC_SECRET, s - 1);
        assert_eq!(verify(RFC_SECRET, &older, now, Some(used)), None);
        let newer = code_for_step(RFC_SECRET, s + 1);
        assert!(verify(RFC_SECRET, &newer, now, Some(used)).is_some());
    }

    #[test]
    fn malformed_codes_never_match() {
        for bad in ["", "12345", "1234567", "abcdef", "12 34 5"] {
            assert!(verify(RFC_SECRET, bad, 59, None).is_none());
        }
        // Spaces and dashes typed for readability are tolerated.
        let c = code_for_step(RFC_SECRET, step_at(59));
        let spaced = format!("{} {}", &c[..3], &c[3..]);
        assert!(verify(RFC_SECRET, &spaced, 59, None).is_some());
    }

    #[test]
    fn base32_round_trips_and_matches_rfc_4648() {
        assert_eq!(base32_encode(b"foobar"), "MZXW6YTBOI");
        assert_eq!(
            base32_encode(RFC_SECRET),
            "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"
        );
        assert_eq!(base32_decode("MZXW6YTBOI").unwrap(), b"foobar");
        assert_eq!(base32_decode("mzxw 6ytb-oi").unwrap(), b"foobar");
        assert!(base32_decode("not base32!").is_none());
        let s = [
            0xde, 0xad, 0xbe, 0xef, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
        ];
        assert_eq!(base32_decode(&base32_encode(&s)).unwrap(), s);
    }

    #[test]
    fn the_uri_names_the_issuer_and_escapes_the_account() {
        let u = otpauth_uri("obop (acme)", "ada@example.test", "ABC");
        assert_eq!(
            u,
            "otpauth://totp/obop%20%28acme%29:ada%40example.test?secret=ABC&issuer=obop%20%28acme%29&algorithm=SHA1&digits=6&period=30"
        );
    }
}
