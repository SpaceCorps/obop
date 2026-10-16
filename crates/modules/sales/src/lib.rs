//! `sales`: quotations and orders.

use obop_commands::{Dispatcher, Store, Value};
use obop_registry::{CommandDef, FieldDef as F, FieldKind, ModelDef, Module};
use obop_types::DataClass::None as NoClass;

pub fn module() -> Module {
    Module {
        id: "sales".into(),
        name: "Sales".into(),
        models: vec![ModelDef {
            name: "sale_order",
            description: "A quotation or confirmed order for a customer",
            read_group: "sales.user",
            write_group: "sales.user",
            fields: vec![
                F::new(
                    "party_id",
                    "Customer party id",
                    NoClass,
                    false,
                    "sales.user",
                )
                .reference("party"),
                F::new(
                    "state",
                    "Lifecycle: draft, confirmed or cancelled",
                    NoClass,
                    false,
                    "sales.user",
                )
                .select(&["draft", "confirmed", "cancelled"]),
                F::new(
                    "total_minor",
                    "Order total in minor units",
                    NoClass,
                    false,
                    "sales.user",
                )
                .kind(FieldKind::Money),
                F::new(
                    "currency",
                    "ISO 4217 currency code",
                    NoClass,
                    false,
                    "sales.user",
                ),
            ],
            commands: vec![
                CommandDef::new("confirm", "Confirm a draft order", "sales.user").transition(
                    "state",
                    Some("draft"),
                    "confirmed",
                ),
                CommandDef::new("cancel", "Cancel an order", "sales.user").transition(
                    "state",
                    None,
                    "cancelled",
                ),
                CommandDef::new(
                    "reset_to_draft",
                    "Put a cancelled order back to draft",
                    "sales.user",
                )
                .transition("state", Some("cancelled"), "draft"),
            ],
        }],
        network_destinations: vec![],
    }
}

pub fn register(d: &Dispatcher, store: &Store) {
    let m = module();
    d.register_crud(&m, store);
    for command in ["confirm", "cancel", "reset_to_draft"] {
        d.register_transition(&m, "sale_order", command, store);
    }
}

pub fn seed(store: &Store, party_id: &str) -> String {
    store
        .insert(
            "sale_order",
            [
                ("party_id".to_string(), Value::text(party_id)),
                ("state".to_string(), Value::text("draft")),
                ("total_minor".to_string(), Value::Int(120_000)),
                ("currency".to_string(), Value::text("EUR")),
            ]
            .into(),
        )
        .expect("seed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use obop_commands::Vault;
    use obop_commands::{CallError, Principal, Surface};

    #[test]
    fn passes_gate() {
        assert!(obop_registry::check_module(&module()).is_empty());
    }

    #[test]
    fn confirm_only_from_draft() {
        let (d, s) = (Dispatcher::new(), Store::new());
        register(&d, &s);
        let id = seed(&s, "p1");
        let u = Principal::user("u", &["sales.user"]);
        let i = Value::map([("id", Value::text(&id))]);
        d.call(
            Surface::Rest,
            &u,
            &mut Vault::new(),
            "sale_order.confirm",
            &i,
        )
        .unwrap();
        assert!(matches!(
            d.call(
                Surface::Rest,
                &u,
                &mut Vault::new(),
                "sale_order.confirm",
                &i
            ),
            Err(CallError::Handler(_))
        ));
    }

    #[test]
    fn state_is_governed_so_update_cannot_skip_confirm_or_leave_a_final_state() {
        let (d, s) = (Dispatcher::new(), Store::new());
        register(&d, &s);
        let u = Principal::user("u", &["sales.user"]);
        let call = |cmd: &str, i: Value| d.call(Surface::Rest, &u, &mut Vault::new(), cmd, &i);
        let msg = |r: Result<obop_commands::Response, CallError>| match r {
            Err(CallError::Handler(m)) => m,
            other => panic!("{other:?}"),
        };
        let e = msg(call(
            "sale_order.create",
            Value::map([("state", Value::text("confirmed"))]),
        ));
        assert!(e.contains("starts as \"draft\""), "{e}");
        let Value::Map(o) = call(
            "sale_order.create",
            Value::map([("currency", Value::text("EUR"))]),
        )
        .unwrap()
        .value
        else {
            panic!()
        };
        let Value::Text(id) = o["id"].clone() else {
            panic!()
        };
        assert_eq!(
            s.field("sale_order", &id, "state"),
            Some(Value::text("draft"))
        );
        let set = |st: &str| {
            call(
                "sale_order.update",
                Value::map([("id", Value::text(&id)), ("state", Value::text(st))]),
            )
        };
        set("confirmed").unwrap();
        // confirmed -> draft has no transition; cancelled -> draft does.
        assert!(msg(set("draft")).contains("it can only go to: cancelled"));
        set("cancelled").unwrap();
        set("draft").unwrap();
        // Ordinary fields are never blocked by it.
        call(
            "sale_order.update",
            Value::map([("id", Value::text(&id)), ("total_minor", Value::Int(5))]),
        )
        .unwrap();
    }
}
