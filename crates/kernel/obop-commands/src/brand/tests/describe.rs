//! The text of the generated `get` of this model (docs/spec/branding.md, 15.4 item 9): the generated
//! text describes the `get` of an ordinary model ("Read one record by id"), which is not true here.

use crate::brand::{MODEL, describe};
use crate::{ANY_GROUP, Dispatcher, Principal, Surface, Value, Vault};
use obop_registry::{FieldDef, ModelDef, Module};
use obop_types::DataClass;

fn module() -> Module {
    Module {
        id: "brand_probe".into(),
        name: "Brand probe".into(),
        models: vec![ModelDef {
            name: MODEL,
            description: "d",
            read_group: ANY_GROUP,
            write_group: "platform.admin",
            access: None,
            fields: vec![FieldDef::new(
                "name",
                "d",
                DataClass::None,
                false,
                ANY_GROUP,
            )],
            commands: vec![],
            ext: Default::default(),
        }],
        ..Module::default()
    }
}

#[test]
fn the_describe_hook_answers_for_get_of_this_model_only() {
    let text = describe(MODEL, "get").expect("a text for get");
    assert!(text.contains("Takes no input"), "{text}");
    assert!(text.contains("fallbacks") && text.contains("effective") && text.contains("set"));
    assert!(!text.contains("by id"), "not the generated text");
    // No other command of this model, and no other model.
    for c in [
        "list",
        "search",
        "create",
        "update",
        "delete",
        "configure",
        "clear",
    ] {
        assert!(describe(MODEL, c).is_none(), "{c}");
    }
    assert!(describe("sso_provider", "get").is_none());
    assert!(describe("setting", "get").is_none());
}

#[test]
fn the_catalogue_shows_the_true_text_for_get_and_leaves_other_models_alone() {
    let d = Dispatcher::new();
    let m = module();
    d.register(
        &m,
        MODEL,
        "get",
        Box::new(|_| Ok(Value::map([("name", Value::text("obop"))]))),
    );
    d.register(&m, MODEL, "list", Box::new(|_| Ok(Value::List(vec![]))));
    let who = Principal::user("ada", &["platform.admin"]);
    let cat = d.catalog(Surface::Rest, &who);
    let get = cat.iter().find(|c| c.id == "instance_brand.get").unwrap();
    assert_eq!(get.description, describe(MODEL, "get").unwrap());
    // The generated text of the commands that this module has no description for is unchanged.
    let list = cat.iter().find(|c| c.id == "instance_brand.list").unwrap();
    assert!(
        list.description.starts_with("List records"),
        "{}",
        list.description
    );
    // And the command runs.
    let out = d
        .call(
            Surface::Rest,
            &who,
            &mut Vault::new(),
            "instance_brand.get",
            &Value::map([]),
        )
        .unwrap();
    assert_eq!(out.value, Value::map([("name", Value::text("obop"))]));
}
