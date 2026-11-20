//! U46: an observer never receives a secret or a value a command shows once, while the caller
//! still gets its full answer.

use crate::{Dispatcher, Store, Surface, Value, Vault};
use crate::{Principal, observe};
use obop_registry::{FieldDef as F, FieldKind, ModelDef, Module};
use obop_types::DataClass;
use std::sync::{Arc, Mutex};

fn module() -> Module {
    Module {
        id: "obs_t".into(),
        name: "Observed".into(),
        models: vec![ModelDef {
            name: "obs_item",
            description: "An item with a secret",
            read_group: "g",
            write_group: "g",
            access: None,
            fields: vec![
                F::new("name", "Name", DataClass::None, false, "g"),
                F::new(
                    "api_key",
                    "A key; write-only",
                    DataClass::Secret,
                    false,
                    "g",
                )
                .kind(FieldKind::LongText),
            ],
            commands: vec![],
            ext: Default::default(),
        }],
        network_destinations: vec![],
        description: String::new(),
        translations: Default::default(),
        meta: Default::default(),
    }
}

type Seen = Arc<Mutex<Vec<(String, Value, Value)>>>;

fn rig() -> (Dispatcher, Seen) {
    let (d, store) = (Dispatcher::new(), Store::new());
    d.register_crud(&module(), &store);
    let seen: Seen = Arc::default();
    let log = seen.clone();
    d.observe(
        "obs_t",
        Box::new(move |id, _p, input, out| {
            log.lock()
                .unwrap()
                .push((id.to_string(), input.clone(), out.clone()));
        }),
    );
    (d, seen)
}

fn has(v: &Value, k: &str) -> bool {
    matches!(v, Value::Map(m) if m.contains_key(k))
}

fn text_of(v: &Value) -> String {
    format!("{v:?}")
}

#[test]
fn an_observer_never_receives_a_secret_field_of_the_input_or_the_answer() {
    let (d, seen) = rig();
    let p = Principal::user("ada", &["g"]);
    let input = Value::map([
        ("name", Value::text("Shop")),
        ("api_key", Value::text("sk_live_canary_0001")),
    ]);
    d.call(
        Surface::Rest,
        &p,
        &mut Vault::new(),
        "obs_item.create",
        &input,
    )
    .unwrap();
    let rows = Value::map([(
        "rows",
        Value::List(vec![Value::map([
            ("name", Value::text("Two")),
            ("api_key", Value::text("sk_live_canary_0002")),
        ])]),
    )]);
    d.call(
        Surface::Rest,
        &p,
        &mut Vault::new(),
        "obs_item.bulk_create",
        &rows,
    )
    .unwrap();
    let csv = Value::map([(
        "csv",
        Value::text("name,api_key\nThree,sk_live_canary_0003\n"),
    )]);
    d.call(
        Surface::Rest,
        &p,
        &mut Vault::new(),
        "obs_item.bulk_create",
        &csv,
    )
    .unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 3, "{seen:?}");
    for (id, i, o) in seen.iter() {
        assert!(
            !text_of(i).contains("canary") && !text_of(o).contains("canary"),
            "{id}: {i:?} {o:?}"
        );
    }
    assert!(has(&seen[0].1, "name"), "the rest of the input stays");
}

#[test]
fn the_link_of_sign_link_create_is_cut_from_what_observers_see_and_kept_for_the_caller() {
    let fields = vec![F::new(
        "request_id",
        "The request",
        DataClass::None,
        false,
        "g",
    )];
    let out = Value::map([
        ("id", Value::text("l1")),
        ("request_id", Value::text("r1")),
        ("link", Value::text("https://x.test/sign/r1/tok-canary")),
    ]);
    let input = Value::map([("request_id", Value::text("r1"))]);
    let (i, o) = observe::for_observers("sign_link.create", &fields, &input, &out);
    assert!(!has(&o, "link") && has(&o, "id") && has(&o, "request_id"));
    assert!(has(&i, "request_id"));
    // Another command that happens to answer a field called link keeps it.
    let (_, o) = observe::for_observers("other.create", &fields, &input, &out);
    assert!(has(&o, "link"));
}
