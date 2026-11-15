use super::token::*;

#[test]
fn a_token_is_64_lowercase_hex_and_two_are_never_alike() {
    let (a, b) = (mint().unwrap(), mint().unwrap());
    assert!(token_shape(&a) && token_shape(&b));
    assert_ne!(a, b);
}

/// What a test can see of a random token: the shape, no repeat, no position stuck at one value.
/// It cannot tell a good generator from a predictable one; that the bytes come from the
/// operating system's generator (`getrandom`, in `mint`) is a thing to read in the code.
#[test]
fn a_thousand_tokens_have_the_shape_are_all_different_and_use_every_digit_everywhere() {
    // 1,000 tokens: all of the shape, all different.
    let tokens: Vec<String> = (0..1000).map(|_| mint().unwrap()).collect();
    assert!(tokens.iter().all(|t| token_shape(t)));
    let distinct: std::collections::HashSet<&String> = tokens.iter().collect();
    assert_eq!(distinct.len(), 1000);
    // Every one of the 32 bytes varies: a token that is 16 random bytes padded to 32, or that
    // repeats a byte, would hold some position still across sixteen samples (the chance that a
    // random byte does is 256 to the power of minus fifteen).
    for pos in 0..32 {
        let seen: std::collections::HashSet<&str> = tokens[..16]
            .iter()
            .map(|t| &t[pos * 2..pos * 2 + 2])
            .collect();
        assert!(seen.len() > 1, "byte {pos} never varies");
    }
    // Both nibbles of a byte take both halves of their range over the thousand: no character is
    // stuck in a smaller alphabet.
    for pos in 0..64 {
        let seen: std::collections::HashSet<u8> =
            tokens.iter().map(|t| t.as_bytes()[pos]).collect();
        assert_eq!(
            seen.len(),
            16,
            "position {pos} uses {} of 16 digits",
            seen.len()
        );
    }
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

/// What the door reads to list a signature to the others: the mark in the fourth part of the id
/// stored with it, and nothing worked out from the other parts.
#[test]
fn only_an_id_with_four_parts_and_the_mark_last_names_the_history() {
    use super::{HISTORY_CONSENT_MARK, consent_names_the_history};
    assert_eq!(HISTORY_CONSENT_MARK, "history");
    // The ids a signature that read the sentence carries, whatever the hash of its words was.
    for yes in [
        "v1-draft/en/e2880db0/history",
        "v1-draft/de/00000000/history",
        "v1/pl/abcdef012345/history",
    ] {
        assert!(consent_names_the_history(yes), "{yes}");
    }
    // The ids of the wording without the sentence, the empty id and a text that only looks like
    // the mark.
    for no in [
        "",
        "v1-draft",
        "v1-draft/en/a05c68c3",
        "v1-draft/en/e2880db0",
        "v1-draft/en/history",
        "history",
        "v1-draft/en/e2880db0/",
        "v1-draft/en/e2880db0/History",
        "v1-draft/en/e2880db0/history ",
        "v1-draft/en/e2880db0/historyX",
        "v1-draft/history/e2880db0",
        "v1-draft/en/e2880db0/history/x",
        "v1-draft/en/e2880db0/x/history",
        "/en/e2880db0/history",
        "v1-draft//e2880db0/history",
        "v1-draft/en//history",
        "//history/history",
    ] {
        assert!(!consent_names_the_history(no), "{no:?}");
    }
}
