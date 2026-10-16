//! `crm`: leads and opportunities.

use obop_commands::{Dispatcher, Store, Value};
use obop_registry::{CommandDef, FieldDef as F, ModelDef, Module};
use obop_types::DataClass::*;

pub fn module() -> Module {
    Module {
        id: "crm".into(),
        models: vec![ModelDef {
            name: "lead",
            description: "A prospective deal with a contact",
            read_group: "crm.user",
            write_group: "crm.user",
            fields: vec![
                F::new(
                    "title",
                    "Short description of the opportunity",
                    None,
                    false,
                    "crm.user",
                ),
                F::new(
                    "contact_name",
                    "Name of the contact person",
                    PublicDirectory,
                    true,
                    "crm.user",
                ),
                F::new(
                    "contact_email",
                    "Email of the contact person",
                    PublicDirectory,
                    true,
                    "crm.user",
                ),
                F::new(
                    "expected_revenue_minor",
                    "Expected revenue in minor units",
                    None,
                    false,
                    "crm.user",
                ),
                F::new(
                    "stage",
                    "Pipeline stage: new, qualified, won or lost",
                    None,
                    false,
                    "crm.user",
                ),
            ],
            commands: vec![CommandDef::new(
                "mark_won",
                "Move the lead to the won stage",
                "crm.user",
            )],
        }],
        network_destinations: vec![],
    }
}

pub fn register(d: &Dispatcher, store: &Store) {
    let m = module();
    d.register_crud(&m, store);
    let s = store.clone();
    d.register(
        &m,
        "lead",
        "mark_won",
        Box::new(move |input| {
            let Value::Map(i) = input else {
                return Err("expected an object".into());
            };
            let Some(Value::Text(id)) = i.get("id") else {
                return Err("missing id".into());
            };
            s.update(
                "lead",
                id,
                [("stage".to_string(), Value::text("won"))].into(),
            )?;
            Ok(Value::map([
                ("id", Value::text(id)),
                ("stage", Value::text("won")),
            ]))
        }),
    );
}

pub fn seed(store: &Store) -> String {
    store
        .insert(
            "lead",
            [
                ("title".to_string(), Value::text("Annual support contract")),
                ("contact_name".to_string(), Value::text("Grace Example")),
                (
                    "contact_email".to_string(),
                    Value::text("grace@example.test"),
                ),
                ("expected_revenue_minor".to_string(), Value::Int(500_000)),
                ("stage".to_string(), Value::text("qualified")),
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
    fn mark_won_changes_stage() {
        let (d, s) = (Dispatcher::new(), Store::new());
        register(&d, &s);
        let id = seed(&s);
        let u = Principal::user("u", &["crm.user"]);
        let i = Value::map([("id", Value::text(&id))]);
        d.call(Surface::Cli, &u, &mut Vault::new(), "lead.mark_won", &i)
            .unwrap();
        assert_eq!(s.field("lead", &id, "stage"), Some(Value::text("won")));
    }
}
