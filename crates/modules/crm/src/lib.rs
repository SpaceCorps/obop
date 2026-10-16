//! `crm`: leads and opportunities.

use obop_commands::{Dispatcher, Store, Value};
use obop_registry::{CommandDef, FieldDef as F, FieldKind, ModelDef, Module};
use obop_types::DataClass::*;

pub fn module() -> Module {
    Module {
        id: "crm".into(),
        name: "CRM".into(),
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
                )
                .required(),
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
                )
                .kind(FieldKind::Money),
                F::new(
                    "stage",
                    "Pipeline stage: new, qualified, won or lost",
                    None,
                    false,
                    "crm.user",
                )
                .select(&["new", "qualified", "won", "lost"]),
            ],
            commands: vec![
                CommandDef::new("qualify", "Move a new lead to the qualified stage", "crm.user")
                    .transition("stage", Some("new"), "qualified"),
                CommandDef::new("mark_won", "Move the lead to the won stage", "crm.user")
                    .transition("stage", Option::None, "won"),
                CommandDef::new("mark_lost", "Move the lead to the lost stage", "crm.user")
                    .transition("stage", Option::None, "lost"),
                CommandDef::new("reopen", "Move the lead back to the new stage", "crm.user")
                    .transition("stage", Option::None, "new"),
            ],
        }],
        network_destinations: vec![],
    }
}

pub fn register(d: &Dispatcher, store: &Store) {
    let m = module();
    d.register_crud(&m, store);
    for command in ["qualify", "mark_won", "mark_lost", "reopen"] {
        d.register_transition(&m, "lead", command, store);
    }
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
