//! The named formats of a text field (K45) as the back office checks them before Save (U18):
//! `web/apps/backoffice/src/patterns.fixtures.json` holds the IBAN lengths and the VAT shapes the
//! browser uses and cases that both sides must answer alike. This test holds the file to the
//! server's tables and to `Pattern::matches`, so the two cannot drift.

use obop_registry::limits::{IBAN_LENGTHS, Pattern, VAT_SHAPES};
use serde_json::Value as J;

const FIXTURES: &str = include_str!("../../../../web/apps/backoffice/src/patterns.fixtures.json");

#[test]
fn the_browser_checks_the_formats_with_the_tables_and_answers_of_the_server() {
    let f: J = serde_json::from_str(FIXTURES).unwrap();
    let iban = f["iban_lengths"].as_object().unwrap();
    assert_eq!(iban.len(), IBAN_LENGTHS.len(), "IBAN countries");
    for (k, n) in IBAN_LENGTHS {
        assert_eq!(
            iban.get(*k).and_then(J::as_u64),
            Some(*n as u64),
            "IBAN length of {k}"
        );
    }
    let vat = f["vat_shapes"].as_object().unwrap();
    assert_eq!(vat.len(), VAT_SHAPES.len(), "VAT prefixes");
    for (k, s) in VAT_SHAPES {
        assert_eq!(
            vat.get(*k).and_then(J::as_str),
            Some(*s),
            "VAT shape of {k}"
        );
    }
    let cases = f["cases"].as_array().unwrap();
    assert!(cases.len() >= 20, "{}", cases.len());
    for c in cases {
        let p = Pattern::parse(c["pattern"].as_str().unwrap()).unwrap();
        let v = c["value"].as_str().unwrap();
        assert_eq!(
            p.matches(v),
            c["ok"].as_bool().unwrap(),
            "{} {v:?}",
            p.name()
        );
    }
}
