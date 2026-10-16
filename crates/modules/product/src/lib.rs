//! `product`: what the business sells.

use obop_commands::{Dispatcher, Store, Value};
use obop_registry::{FieldDef as F, ModelDef, Module};
use obop_types::DataClass::None as NoClass;

pub fn module() -> Module {
    Module {
        id: "product".into(),
        models: vec![ModelDef {
            name: "product",
            description: "A good or service the business sells",
            read_group: "product.user",
            write_group: "product.manager",
            fields: vec![
                F::new("name", "Product name", NoClass, false, "product.user"),
                F::new("sku", "Stock keeping unit", NoClass, false, "product.user"),
                F::new(
                    "list_price_minor",
                    "List price in minor currency units",
                    NoClass,
                    false,
                    "product.user",
                ),
                F::new(
                    "cost_minor",
                    "Unit cost in minor currency units",
                    NoClass,
                    false,
                    "product.cost_viewer",
                ),
            ],
            commands: vec![],
        }],
        network_destinations: vec![],
    }
}

pub fn register(d: &mut Dispatcher, store: &Store) {
    d.register_crud(&module(), store);
}

pub fn seed(store: &Store) -> String {
    store.insert(
        "product",
        [
            ("name".to_string(), Value::text("Consulting day")),
            ("sku".to_string(), Value::text("SVC-DAY")),
            ("list_price_minor".to_string(), Value::Int(120_000)),
            ("cost_minor".to_string(), Value::Int(70_000)),
        ]
        .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use obop_commands::Vault;
    use obop_commands::{Principal, Surface};

    #[test]
    fn passes_gate() {
        assert!(obop_registry::check_module(&module()).is_empty());
    }

    #[test]
    fn cost_needs_its_own_group() {
        let (mut d, s) = (Dispatcher::new(), Store::new());
        register(&mut d, &s);
        let id = seed(&s);
        let input = Value::map([("id", Value::text(&id))]);
        let plain = Principal::user("u", &["product.user"]);
        let r = d
            .call(
                Surface::Rest,
                &plain,
                &mut Vault::new(),
                "product.get",
                &input,
            )
            .unwrap();
        assert_eq!(r.denied, vec!["product.cost_minor".to_string()]);
        let finance = Principal::user("f", &["product.user", "product.cost_viewer"]);
        let r = d
            .call(
                Surface::Rest,
                &finance,
                &mut Vault::new(),
                "product.get",
                &input,
            )
            .unwrap();
        assert!(r.denied.is_empty());
    }
}
