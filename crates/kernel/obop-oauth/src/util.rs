//! Small helpers: hashing, base64, random ids, constant-time comparison, text cleaning.

use sha2::{Digest, Sha256};

pub fn now_system() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// `N` random bytes from the operating system, as lowercase hex.
pub fn rand_hex<const N: usize>() -> String {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).expect("operating system randomness");
    hex(&b)
}

pub fn sha_hex(s: &str) -> String {
    hex(&Sha256::digest(s.as_bytes()))
}

const STD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn b64(bytes: &[u8], alphabet: &[u8; 64], pad: bool) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        let chars = [
            alphabet[(n >> 18) as usize & 63],
            alphabet[(n >> 12) as usize & 63],
            alphabet[(n >> 6) as usize & 63],
            alphabet[n as usize & 63],
        ];
        let keep = chunk.len() + 1;
        for c in &chars[..keep] {
            out.push(*c as char);
        }
        if pad {
            for _ in keep..4 {
                out.push('=');
            }
        }
    }
    out
}

/// Base64url without padding (RFC 7636).
pub fn b64url(bytes: &[u8]) -> String {
    b64(bytes, URL, false)
}

/// Standard base64 with padding (CSP hashes).
pub fn b64std(bytes: &[u8]) -> String {
    b64(bytes, STD, true)
}

/// The S256 code challenge of a verifier.
pub fn s256(verifier: &str) -> String {
    b64url(&Sha256::digest(verifier.as_bytes()))
}

/// Compare two secrets without stopping at the first difference.
pub fn ct_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        diff |= usize::from(a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0));
    }
    diff == 0
}

/// Text from an untrusted source made safe to show and to log: control characters dropped,
/// whitespace collapsed, at most `max` characters.
pub fn clean_text(s: &str, max: usize) -> String {
    let mut out = String::new();
    let mut last_space = true;
    for c in s.chars() {
        let c = if c.is_whitespace() { ' ' } else { c };
        if c.is_control()
            || matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        {
            continue;
        }
        if c == ' ' {
            if last_space {
                continue;
            }
            last_space = true;
        } else {
            last_space = false;
        }
        out.push(c);
        if out.chars().count() >= max {
            break;
        }
    }
    out.trim().to_string()
}

/// Percent-encode a query component.
pub fn pct(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(b64std(b"f"), "Zg==");
        assert_eq!(b64std(b"fo"), "Zm8=");
        assert_eq!(b64std(b"foo"), "Zm9v");
        assert_eq!(b64url(&[0xfb, 0xff]), "-_8");
    }

    #[test]
    fn s256_matches_the_rfc_7636_example() {
        assert_eq!(
            s256("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn constant_time_compare_and_cleaning() {
        assert!(ct_eq("abc", "abc") && !ct_eq("abc", "abd") && !ct_eq("abc", "abcd"));
        assert_eq!(clean_text("  Ev\u{202e}il \n\t app\u{0}  ", 80), "Evil app");
        assert_eq!(clean_text("abcdef", 3), "abc");
    }
}
