//! D95 through MCP: a default and a `fill_from` apply exactly as on REST and the command line, an
//! assertion's refusal carries `code`, `assert_key` and the author's `message`, and the metadata
//! carries the hints in the shapes the back office reads.

use crate::{Session, call_tool, meta_json};
use obop_commands::{Dispatcher, Principal, Store, Surface, Value, Vault, names};
use obop_registry::{CommandDef, FieldDef as F, FieldKind, ModelDef, Module, intern};
use obop_types::DataClass;
use serde_json::{Value as J, json};

const U: &str = "mh.user";

fn module() -> Module {
    let f = |name: &'static str| F::new(name, "d", DataClass::None, false, U);
    let model = |name: &str, fields: Vec<F>, commands: Vec<CommandDef>| ModelDef {
        name: intern(name),
        description: "d",
        read_group: U,
        write_group: U,
        access: None,
        fields,
        commands,
        ext: Default::default(),
    };
    Module {
        id: "mh".into(),
        name: "MH".into(),
        models: vec![
            model(
                "mh_product",
                vec![
                    f("name"),
                    f("price").kind(FieldKind::Money),
                    F::new("email", "d", DataClass::PublicDirectory, true, U),
                ],
                vec![],
            ),
            model(
                "mh_line",
                vec![
                    f("product_id").reference("mh_product"),
                    f("price")
                        .kind(FieldKind::Money)
                        .fill_from("product_id.price"),
                    f("qty")
                        .kind(FieldKind::Number)
                        .default_int(1)
                        .aggregate("sum"),
                    f("day").kind(FieldKind::Date).default_value("today"),
                    f("state").select(&["new", "done"]),
                ],
                vec![
                    CommandDef::new("finish", "Finish", U)
                        .transition("state", Some("new"), "done")
                        .assertion_keyed("qty > 5", "Order at least six", "enough")
                        .primary(),
                ],
            ),
        ],
        network_destinations: vec![],
        description: String::new(),
        translations: Default::default(),
        meta: Default::default(),
    }
}

fn rig() -> (Dispatcher, Principal) {
    let store = Store::open("memory:").unwrap();
    let d = Dispatcher::new();
    let m = module();
    assert!(obop_registry::check_module(&m).is_empty());
    d.register_crud(&m, &store);
    d.register_transition(&m, "mh_line", "finish", &store);
    (d, Principal::user("ann", &[U]))
}

fn mcp(d: &Dispatcher, p: &Principal, cmd: &str, args: J) -> J {
    let mut s = Session::new();
    let r = call_tool(
        d,
        p,
        &mut s,
        &json!({"name": names::to_mcp(cmd), "arguments": args}),
    )
    .unwrap();
    r["structuredContent"].clone()
}

#[test]
fn a_default_and_a_fill_apply_the_same_on_rest_mcp_and_the_command_line() {
    let (d, p) = rig();
    let prod = d
        .call(
            Surface::Rest,
            &p,
            &mut Vault::new(),
            "mh_product.create",
            &Value::from_json(&json!({"name": "Bolt", "price": 250})),
        )
        .unwrap()
        .value
        .to_json()["id"]
        .clone();
    let input = json!({"product_id": prod});
    let via_mcp = mcp(&d, &p, "mh_line.create", input.clone());
    assert_eq!(via_mcp["status"], "ok", "{via_mcp}");
    let mut seen = vec![via_mcp["value"].clone()];
    for surface in [Surface::Rest, Surface::Cli] {
        let r = d
            .call(
                surface,
                &p,
                &mut Vault::new(),
                "mh_line.create",
                &Value::from_json(&input),
            )
            .unwrap();
        seen.push(r.value.to_json());
    }
    for v in &seen {
        assert_eq!(v["price"], 250, "{v}");
        assert_eq!(v["qty"], 1, "{v}");
        assert!(v["day"].as_str().is_some_and(|s| s.len() == 10), "{v}");
        assert_eq!(v["state"], "new", "{v}");
    }
}

#[test]
fn an_assertion_refused_over_mcp_says_its_code_key_and_message() {
    let (d, p) = rig();
    let line = mcp(&d, &p, "mh_line.create", json!({}));
    let id = line["value"]["id"].clone();
    let out = mcp(&d, &p, "mh_line.finish", json!({"id": id}));
    assert_eq!(out["code"], "assertion_failed", "{out}");
    assert_eq!(out["assert_key"], "enough");
    assert_eq!(out["message"], "Order at least six");
    assert!(
        out["error"]
            .as_str()
            .unwrap()
            .ends_with("(assertion qty > 5 failed)")
    );
}

#[test]
fn the_metadata_carries_the_hints() {
    let (d, p) = rig();
    let meta = meta_json(&d, Surface::Rest, &p);
    assert_eq!(meta["currency"], "EUR");
    let line = meta["modules"][0]["models"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["name"] == "mh_line")
        .unwrap()
        .clone();
    let field = |n: &str| {
        line["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == n)
            .unwrap()
            .clone()
    };
    assert_eq!(field("price")["fill_from"], "product_id.price");
    assert_eq!(field("qty")["default"], 1);
    assert_eq!(field("qty")["ui"], json!({"aggregate": "sum"}));
    assert_eq!(field("day")["default"], "today");
    // A field without hints carries none of the keys.
    let state = field("state");
    for k in ["default", "fill_from", "readonly", "ref_filter", "ui"] {
        assert!(state.get(k).is_none(), "{k}: {state}");
    }
    assert_eq!(line["actions"][0]["ui"], json!({"primary": true}));
    assert!(line.get("ui").is_none() && line.get("children").is_none());
}

#[test]
fn the_metadata_says_which_fields_an_agent_reads_masked() {
    let (d, ann) = rig();
    let field = |meta: &J, n: &str| {
        meta["modules"][0]["models"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["name"] == "mh_product")
            .unwrap()["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == n)
            .unwrap()
            .clone()
    };
    // A person reads personal data in clear and may sort by it (R13): no mark.
    let person = meta_json(&d, Surface::Rest, &ann);
    assert!(field(&person, "email").get("masked").is_none());
    let bot = Principal::agent("bot", &[U], &[U]);
    let agent = meta_json(&d, Surface::Rest, &bot);
    assert_eq!(field(&agent, "email")["masked"], true);
    assert!(field(&agent, "name").get("masked").is_none());
}
