//! Envelope for second-factor secrets at rest (D53): ChaCha20-Poly1305 under a key derived with
//! HKDF-SHA-256 from the server secret `OBOP_SECRET_KEY`. The user name is bound in as associated
//! data, so a sealed secret copied onto another user's record does not open.
//!
//! Values use the key ring's envelope (`obop-keys`, B3): it names the key that sealed it, so a
//! rotation can find and re-seal what an old key still protects. The first format (`v1.`, no key
//! id) is still opened, by trying every key of the ring.

use obop_keys::{KeyRing, legacy};

pub use obop_keys::INSECURE_DEV_KEY;
pub use obop_keys::legacy::parse_key;

/// The purpose a user's second-factor secret is sealed for.
pub fn purpose(user: &str) -> String {
    format!("totp:{user}")
}

/// Seal a second-factor secret with the current key of the ring.
pub fn seal_new(ring: &KeyRing, user: &str, plain: &[u8]) -> Result<String, String> {
    ring.seal(&purpose(user), plain).map_err(String::from)
}

/// Open a second-factor secret in either format.
pub fn open_any(ring: &KeyRing, user: &str, sealed: &str) -> Result<Vec<u8>, String> {
    legacy::open_any(ring, &purpose(user), Some(user), sealed).map_err(String::from)
}

/// Which key sealed `sealed`, when the format says (`None` for the first format).
pub fn key_id(sealed: &str) -> Option<String> {
    obop_keys::sealed_key_id(sealed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn round_trip_with_fresh_nonces_and_the_user_bound_in() {
        let ring = KeyRing::from_secrets(A, &[]).unwrap();
        let a = seal_new(&ring, "ada", b"secret bytes").unwrap();
        let b = seal_new(&ring, "ada", b"secret bytes").unwrap();
        assert_ne!(a, b, "a new nonce every time");
        assert!(!a.contains("secret"));
        assert_eq!(open_any(&ring, "ada", &a).unwrap(), b"secret bytes");
        assert!(open_any(&ring, "grace", &a).is_err(), "bound to the user");
        assert!(open_any(&ring, "ada", "garbage").is_err());
        assert_eq!(key_id(&a).unwrap(), obop_keys::key_id(A));
    }

    #[test]
    fn values_of_the_first_format_still_open_and_a_rotated_ring_opens_both() {
        let old = legacy::seal_totp_v1(&parse_key(A).unwrap(), "ada", b"seed");
        let ring = KeyRing::from_secrets(B, &[A]).unwrap();
        assert_eq!(open_any(&ring, "ada", &old).unwrap(), b"seed");
        assert!(key_id(&old).is_none());
        let new = seal_new(&ring, "ada", b"seed").unwrap();
        assert_eq!(key_id(&new).unwrap(), obop_keys::key_id(B));
        // Without the previous key the old value is lost to this process, and nothing leaks.
        let only_b = KeyRing::from_secrets(B, &[]).unwrap();
        let e = open_any(&only_b, "ada", &old).unwrap_err();
        assert!(!e.contains(A));
    }
}
