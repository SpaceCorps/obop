use super::token::*;

#[test]
fn a_token_is_64_lowercase_hex_and_two_are_never_alike() {
    let (a, b) = (mint().unwrap(), mint().unwrap());
    assert!(token_shape(&a) && token_shape(&b));
    assert_ne!(a, b);
}

#[test]
fn the_link_id_is_a_hash_of_the_domain_the_request_and_the_token() {
    use sha2::{Digest, Sha256};
    let tok = "ab".repeat(32);
    let mut h = Sha256::new();
    h.update(b"obop-sign-link-v1\0req-1\0");
    h.update(tok.as_bytes());
    let want = hex(&h.finalize());
    assert_eq!(link_id("req-1", &tok), want);
    // The request is part of the hash: the same token for another request is another id.
    assert_ne!(link_id("req-2", &tok), want);
    assert_ne!(link_id("req-1", &"cd".repeat(32)), want);
}

#[test]
fn shapes_are_exact() {
    assert!(token_shape(&"0f".repeat(32)));
    for bad in [
        "",
        &"0f".repeat(31),
        &"0f".repeat(33),
        &"0F".repeat(32),
        &"zz".repeat(32),
        &format!("{} ", "0f".repeat(32)),
    ] {
        assert!(!token_shape(bad), "{bad:?}");
    }
    assert!(rid_shape("01a1a145-e60b-733d-b772-787979f36c2b"));
    for bad in ["", "a/b", "a b", "a.b", &"a".repeat(65), "ä"] {
        assert!(!rid_shape(bad), "{bad:?}");
    }
}

#[test]
fn ct_eq_compares_whole_texts() {
    assert!(ct_eq("abc", "abc"));
    assert!(!ct_eq("abc", "abd") && !ct_eq("abc", "ab") && !ct_eq("", "a"));
}
