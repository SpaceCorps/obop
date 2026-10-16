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
                CommandDef::new(
                    "qualify",
                    "Move a new lead to the qualified stage",
                    "crm.user",
                )
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

    fn call(
        d: &Dispatcher,
        p: &Principal,
        cmd: &str,
        input: Value,
    ) -> Result<obop_commands::Response, obop_commands::CallError> {
        d.call(Surface::Rest, p, &mut Vault::new(), cmd, &input)
    }

    /// The pipeline is governed (D48): a lead is created `new` and moves through the actions,
    /// which is also what a kanban drag does through `update`.
    #[test]
    fn stage_moves_only_along_the_declared_transitions() {
        let (d, s) = (Dispatcher::new(), Store::new());
        register(&d, &s);
        let u = Principal::user("u", &["crm.user"]);
        let made = |stage: Option<&str>| {
            let mut v = vec![("title", Value::text("Deal"))];
            if let Some(st) = stage {
                v.push(("stage", Value::text(st)));
            }
            call(
                &d,
                &u,
                "lead.create",
                Value::Map(v.into_iter().map(|(k, x)| (k.to_string(), x)).collect()),
            )
        };
        for late in ["qualified", "won", "lost"] {
            assert!(
                matches!(made(Some(late)), Err(obop_commands::CallError::Handler(m)) if m.contains("starts as \"new\"")),
                "{late}"
            );
        }
        let Value::Map(lead) = made(Option::None).unwrap().value else {
            panic!()
        };
        let Value::Text(id) = lead["id"].clone() else {
            panic!()
        };
        assert_eq!(s.field("lead", &id, "stage"), Some(Value::text("new")));
        let set = |stage: &str| {
            call(
                &d,
                &u,
                "lead.update",
                Value::map([("id", Value::text(&id)), ("stage", Value::text(stage))]),
            )
        };
        // Kanban moves: new -> qualified -> won -> new (reopen) -> lost.
        for stage in ["qualified", "won", "new", "lost"] {
            set(stage).unwrap_or_else(|e| panic!("{stage}: {e:?}"));
            assert_eq!(s.field("lead", &id, "stage"), Some(Value::text(stage)));
        }
        // qualified is only reachable from new.
        let e = set("qualified");
        assert!(
            matches!(e, Err(obop_commands::CallError::Handler(m)) if m.contains("it can only go to: won, new")),
        );
        // Explicit actions agree with the implicit ones.
        call(
            &d,
            &u,
            "lead.reopen",
            Value::map([("id", Value::text(&id))]),
        )
        .unwrap();
        call(
            &d,
            &u,
            "lead.qualify",
            Value::map([("id", Value::text(&id))]),
        )
        .unwrap();
        assert!(
            call(
                &d,
                &u,
                "lead.qualify",
                Value::map([("id", Value::text(&id))])
            )
            .is_err(),
            "qualify only from new"
        );
    }
}
