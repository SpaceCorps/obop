//! The commit a build was made from, for `obop --version`.
//!
//! `build.rs` finds it (the environment variable `OBOP_BUILD_COMMIT`, which the Docker build sets
//! from its build argument, else `git rev-parse HEAD` in the checkout) and hands it to the
//! compiler as `OBOP_BUILD_COMMIT`; `main` prints it. The build script includes this file by path,
//! so the rule that decides what counts as a commit is the same in both and is tested once, here.

// The build script uses `short` and the program uses `version_line`.
#![allow(dead_code)]

/// How many hexadecimal digits of the hash the version line shows.
pub const SHORT: usize = 12;

/// What the line shows when the build knew no commit (no `.git`, no build argument).
pub const UNKNOWN: &str = "unknown";

/// The first twelve digits, in lower case, of a commit hash: `raw` must be a hash of twelve to 64
/// hexadecimal digits (a SHA-1 is 40, a SHA-256 is 64), with nothing else around it but
/// whitespace. Anything else is not a commit (`None`).
pub fn short(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let digits = (SHORT..=64).contains(&raw.len()) && raw.bytes().all(|b| b.is_ascii_hexdigit());
    digits.then(|| raw[..SHORT].to_ascii_lowercase())
}

/// The line `obop --version` prints: `obop 0.2.0 (abcdef012345)`. The prefix `obop 0.2.0` is the
/// one earlier builds printed alone.
pub fn version_line(name: &str, version: &str, commit: &str) -> String {
    format!("{name} {version} ({commit})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hash_is_cut_to_twelve_lower_case_digits() {
        let full = "5EE5A96E1F2A9B7C0D3E4F5A6B7C8D9E0F1A2B3C";
        assert_eq!(short(full).as_deref(), Some("5ee5a96e1f2a"));
        assert_eq!(short(&full.to_lowercase()).as_deref(), Some("5ee5a96e1f2a"));
        // Exactly twelve is kept; a SHA-256 hash is cut like a SHA-1 one.
        assert_eq!(short("0123456789ab").as_deref(), Some("0123456789ab"));
        assert_eq!(short(&"f".repeat(64)).as_deref(), Some("ffffffffffff"));
        // The line a command prints around it does not matter.
        assert_eq!(
            short("  0123456789abcdef0123\n").as_deref(),
            Some("0123456789ab")
        );
    }

    #[test]
    fn anything_that_is_not_a_hash_is_no_commit() {
        for bad in [
            "",
            "   ",
            "abc1234",
            "0123456789a",
            &"a".repeat(65),
            "0123456789abg",
            "0123456789ab-dirty",
            "unknown",
            "main",
            "0123456789ab\ncargo:rustc-env=X=1",
            "0123 456789ab",
        ] {
            assert_eq!(short(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_version_line_keeps_the_prefix_of_every_earlier_build() {
        let line = version_line("obop", "0.2.0", "5ee5a96e1f2a");
        assert_eq!(line, "obop 0.2.0 (5ee5a96e1f2a)");
        assert!(line.starts_with("obop 0.2.0"));
        assert_eq!(
            version_line("obop", "0.2.0", UNKNOWN),
            "obop 0.2.0 (unknown)"
        );
    }
}
