//! `product`: what the business sells.

use obop_commands::{Dispatcher, Store, Value};
use obop_registry::{FieldDef as F, FieldKind, ModelDef, Module, ModuleMeta};
use obop_types::DataClass::None as NoClass;

pub fn module() -> Module {
    let m = Module {
        id: "product".into(),
        name: "Products".into(),
        models: vec![ModelDef {
            name: "product",
            description: "A good or service the business sells",
            read_group: "product.user",
            write_group: "product.manager",
            access: None,
            fields: vec![
                F::new("name", "Product name", NoClass, false, "product.user").required(),
                F::new(
                    "sku",
                    "Stock keeping unit; no two products share one",
                    NoClass,
                    false,
                    "product.user",
                )
                .unique(),
                F::new(
                    "list_price_minor",
                    "List price in minor currency units",
                    NoClass,
                    false,
                    "product.user",
                )
                .kind(FieldKind::Money),
                F::new(
                    "cost_minor",
                    "Unit cost in minor currency units",
                    NoClass,
                    false,
                    "product.cost_viewer",
                )
                .kind(FieldKind::Money),
            ],
            commands: vec![],
        }],
        network_destinations: vec![],
        description: String::new(),
        translations: Default::default(),
        meta: ModuleMeta::first_party(
            "Products and services with prices and cost, shared by sales, invoicing and stock.",
            &["sales", "supply-chain"],
            &["products", "catalogue", "pricing"],
            "products",
            &["base"],
        ),
    };
    m.translated(&[
        include_str!("../../../../packages/labels/de/product.toml"),
        include_str!("../../../../packages/labels/fr/product.toml"),
        include_str!("../../../../packages/labels/pl/product.toml"),
        include_str!("../../../../packages/labels/fi/product.toml"),
        include_str!("../../../../packages/labels/sv/product.toml"),
        include_str!("../../../../packages/labels/es/product.toml"),
    ])
}

pub fn register(d: &Dispatcher, store: &Store) {
    d.register_crud(&module(), store);
}

pub fn seed(store: &Store) -> String {
    store
        .insert(
            "product",
            [
                ("name".to_string(), Value::text("Consulting day")),
                ("sku".to_string(), Value::text("SVC-DAY")),
                ("list_price_minor".to_string(), Value::Int(120_000)),
                ("cost_minor".to_string(), Value::Int(70_000)),
            ]
            .into(),
        )
        .expect("seed")
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
        let (d, s) = (Dispatcher::new(), Store::new());
        register(&d, &s);
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
