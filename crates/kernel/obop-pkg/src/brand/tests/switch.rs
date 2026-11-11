//! The switch `hide_product_docs` through the commands (docs/spec/branding.md, sections 2, 3 and 13,
//! S2): it is the one boolean field, it is set by an explicit true or false (the JSON booleans and
//! the texts, as `dry_run` is), it is read by `get` for every signed-in principal, and a stored
//! value that is not a boolean reads as off and is listed under the fallbacks.

use super::*;
use obop_storage::Op;

fn bool_input(pairs: Vec<(&str, Value)>) -> Value {
    Value::Map(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn configure_with(r: &Rig, pairs: Vec<(&str, Value)>) -> Result<J, String> {
    match r.call(&person(), "instance_brand.configure", &bool_input(pairs)) {
        Ok(out) => Ok(out.value.to_json()),
        Err(CallError::Handler(m)) => Err(m),
        Err(e) => panic!("{e:?}"),
    }
}

fn someone_else() -> Principal {
    Principal::user("bob", &["base.user"])
}

#[test]
fn get_answers_the_switch_as_a_boolean_off_with_no_row() {
    let r = rig();
    let g = r.get();
    assert_eq!(g["hide_product_docs"], false, "{g}");
    assert_eq!(g["set"], false);
}

#[test]
fn configure_takes_the_json_booleans_and_the_texts_true_and_false() {
    let r = rig();
    let a = configure_with(
        &r,
        vec![
            ("name", Value::text("MOHV")),
            ("hide_product_docs", Value::Bool(true)),
        ],
    )
    .unwrap();
    assert!(
        parsed(&a, "changes")
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["field"] == "hide_product_docs"
                && c["old"] == "false"
                && c["new"] == "true"),
        "{a}"
    );
    assert_eq!(r.get()["hide_product_docs"], true);
    assert_eq!(
        r.row().unwrap().get("hide_product_docs"),
        Some(&J::Bool(true)),
        "stored as the boolean true"
    );
    // The text, as `obop call` sends a typed input.
    r.configure(&[("hide_product_docs", "false")]).unwrap();
    assert_eq!(r.get()["hide_product_docs"], false);
    assert!(!r.row().unwrap().contains_key("hide_product_docs"));
    r.configure(&[("hide_product_docs", "true")]).unwrap();
    assert_eq!(r.get()["hide_product_docs"], true);
    configure_with(&r, vec![("hide_product_docs", Value::Bool(false))]).unwrap();
    assert_eq!(r.get()["hide_product_docs"], false);
}

#[test]
fn anything_else_is_refused_by_the_name_of_the_field_and_stores_nothing() {
    let r = rig();
    r.configure(&[("name", "MOHV")]).unwrap();
    let events = r.events().len();
    for bad in [
        Value::text("yes"),
        Value::text("TRUE"),
        Value::text(""),
        Value::Null,
        Value::map([("on", Value::Bool(true))]),
    ] {
        let m = configure_with(&r, vec![("hide_product_docs", bad.clone())]).unwrap_err();
        assert!(
            m.contains("hide_product_docs") && m.contains("true or false"),
            "{bad:?}: {m}"
        );
        assert_eq!(r.get()["hide_product_docs"], false, "{bad:?}");
        assert_eq!(r.events().len(), events, "{bad:?}: nothing is logged");
    }
}

#[test]
fn every_signed_in_principal_reads_the_switch_with_get_and_none_may_write_it() {
    let r = rig();
    r.configure(&[("name", "MOHV"), ("hide_product_docs", "true")])
        .unwrap();
    let out = r
        .call(&someone_else(), "instance_brand.get", &bool_input(vec![]))
        .unwrap();
    assert!(
        out.denied.is_empty(),
        "a signed-in principal that is not an administrator is shown every key: {:?}",
        out.denied
    );
    let v = out.value.to_json();
    assert_eq!(v["hide_product_docs"], true, "{v}");
    // The commands that change it stay with a person who holds platform.admin.
    assert!(
        r.call(
            &someone_else(),
            "instance_brand.configure",
            &text_input(&[("hide_product_docs", "false")]),
        )
        .is_err()
    );
    for who in [agent(), scoped(), mobile()] {
        let e = handler_text(
            r.call(
                &who,
                "instance_brand.configure",
                &text_input(&[("hide_product_docs", "false")]),
            )
            .unwrap_err(),
        );
        assert!(e.contains("changed by a person"), "{e}");
    }
    assert_eq!(r.get()["hide_product_docs"], true);
}

#[test]
fn a_stored_value_that_is_not_a_boolean_reads_as_off_and_get_lists_it_as_a_fallback() {
    for (stored, why) in [
        (J::String("true".into()), "the text true"),
        (J::String("yes".into()), "the text yes"),
        (json!(1), "the number 1"),
    ] {
        let r = rig();
        r.store
            .backend()
            .apply(&[Op::Put {
                model: MODEL.into(),
                id: "instance".into(),
                fields: serde_json::from_value(
                    json!({"name": "MOHV", "hide_product_docs": stored}),
                )
                .unwrap(),
            }])
            .unwrap();
        let g = r.get();
        assert_eq!(g["hide_product_docs"], false, "{why}: {g}");
        let fallbacks = parsed(&g, "fallbacks");
        assert_eq!(
            fallbacks,
            json!([{"field": "hide_product_docs", "reason": "not_boolean"}]),
            "{why}"
        );
    }
}

#[test]
fn the_values_file_takes_the_switch_as_a_boolean_or_as_the_texts() {
    let got = read_values_file("[record]\nname = \"MOHV\"\nhide_product_docs = true\n").unwrap();
    assert_eq!(
        got,
        [
            ("name".to_string(), "MOHV".to_string()),
            ("hide_product_docs".to_string(), "true".to_string())
        ]
    );
    let got = read_values_file("[record]\nname = \"MOHV\"\nhide_product_docs = false\n").unwrap();
    assert_eq!(
        got[1],
        ("hide_product_docs".to_string(), "false".to_string())
    );
    let got = read_values_file("[record]\nhide_product_docs = \"true\"\n").unwrap();
    assert_eq!(got, [("hide_product_docs".to_string(), "true".to_string())]);
    // A number or a list is no switch, and the reason names the key and the rule, never a value.
    for bad in ["1", "[true]", "{ on = true }"] {
        let reasons =
            read_values_file(&format!("[record]\nhide_product_docs = {bad}\n")).unwrap_err();
        assert_eq!(reasons.len(), 1, "{bad}: {reasons:?}");
        assert!(
            reasons[0].contains("hide_product_docs") && reasons[0].contains("true or false"),
            "{reasons:?}"
        );
    }
}
