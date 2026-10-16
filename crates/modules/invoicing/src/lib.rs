//! `invoicing`: customer invoices. Posting is irreversible, so agents may only propose it.

use obop_commands::{Dispatcher, Store, Value};
use obop_registry::{CommandDef, FieldDef as F, ModelDef, Module};
use obop_types::DataClass::None as NoClass;

pub fn module() -> Module {
    Module {
        id: "invoicing".into(),
        models: vec![ModelDef {
            name: "invoice",
            description: "A customer invoice; posted invoices are corrected only by credit notes",
            read_group: "invoicing.user",
            write_group: "invoicing.user",
            fields: vec![
                F::new(
                    "sale_order_id",
                    "Source sale order id",
                    NoClass,
                    false,
                    "invoicing.user",
                ),
                F::new(
                    "state",
                    "Lifecycle: draft, posted or cancelled",
                    NoClass,
                    false,
                    "invoicing.user",
                ),
                F::new(
                    "total_minor",
                    "Invoice total in minor units",
                    NoClass,
                    false,
                    "invoicing.user",
                ),
                F::new(
                    "number",
                    "Gapless legal number, set when posted",
                    NoClass,
                    false,
                    "invoicing.user",
                ),
            ],
            commands: vec![
                CommandDef::new(
                    "post",
                    "Post a draft invoice and assign its legal number",
                    "invoicing.manager",
                )
                .approval(),
            ],
        }],
        network_destinations: vec![],
    }
}

pub fn register(d: &mut Dispatcher, store: &Store) {
    let m = module();
    d.register_crud(&m, store);
    let s = store.clone();
    d.register(
        &m,
        "invoice",
        "post",
        Box::new(move |input| {
            let Value::Map(i) = input else {
                return Err("expected an object".into());
            };
            let Some(Value::Text(id)) = i.get("id") else {
                return Err("missing id".into());
            };
            match s.field("invoice", id, "state") {
                Some(Value::Text(st)) if st == "draft" => {}
                Some(_) => return Err("only a draft invoice can be posted".into()),
                None => return Err(format!("no invoice {id}")),
            }
            // Placeholder numbering; the real gapless sequence is enforced by PostgreSQL (D30).
            let number = format!("INV-{:05}", s.count("invoice"));
            s.update(
                "invoice",
                id,
                [
                    ("state".to_string(), Value::text("posted")),
                    ("number".to_string(), Value::Text(number.clone())),
                ]
                .into(),
            )?;
            Ok(Value::map([
                ("id", Value::text(id)),
                ("state", Value::text("posted")),
                ("number", Value::Text(number)),
            ]))
        }),
    );
}

pub fn seed(store: &Store, order_id: &str) -> String {
    store.insert(
        "invoice",
        [
            ("sale_order_id".to_string(), Value::text(order_id)),
            ("state".to_string(), Value::text("draft")),
            ("total_minor".to_string(), Value::Int(120_000)),
            ("number".to_string(), Value::Null),
        ]
        .into(),
    )
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
    fn agent_proposes_human_posts() {
        let (mut d, s) = (Dispatcher::new(), Store::new());
        register(&mut d, &s);
        let id = seed(&s, "o1");
        let i = Value::map([("id", Value::text(&id))]);
        let agent = Principal::agent("a", &["invoicing.manager"], &["invoicing.manager"]);
        let mut v = Vault::new();
        let Err(CallError::ApprovalRequired { plan_id }) =
            d.call(Surface::Mcp, &agent, &mut v, "invoice.post", &i)
        else {
            panic!()
        };
        assert_eq!(s.field("invoice", &id, "state"), Some(Value::text("draft")));
        let human = Principal::user("h", &["invoicing.manager"]);
        d.approve(&human, &mut v, &plan_id).unwrap();
        assert_eq!(
            s.field("invoice", &id, "state"),
            Some(Value::text("posted"))
        );
        assert_eq!(
            s.field("invoice", &id, "number"),
            Some(Value::text("INV-00001"))
        );
    }
}
