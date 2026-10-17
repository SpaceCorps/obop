//! `invoicing`: customer invoices. Posting is irreversible, so agents may only propose it.

use obop_commands::{Dispatcher, Store, Value};
use obop_registry::{CommandDef, FieldDef as F, FieldKind, ModelDef, Module};
use obop_types::DataClass::None as NoClass;

pub fn module() -> Module {
    let m = Module {
        id: "invoicing".into(),
        name: "Invoicing".into(),
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
                )
                .reference("sale_order"),
                F::new(
                    "state",
                    "Lifecycle: draft, posted or cancelled",
                    NoClass,
                    false,
                    "invoicing.user",
                )
                .select(&["draft", "posted", "cancelled"]),
                F::new(
                    "total_minor",
                    "Invoice total in minor units",
                    NoClass,
                    false,
                    "invoicing.user",
                )
                .kind(FieldKind::Money),
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
                .transition("state", Some("draft"), "posted")
                .approval(),
                CommandDef::new("cancel", "Cancel a draft invoice", "invoicing.user").transition(
                    "state",
                    Some("draft"),
                    "cancelled",
                ),
            ],
        }],
        network_destinations: vec![],
        description: String::new(),
        translations: Default::default(),
    };
    m.translated(&[
        include_str!("../../../../packages/labels/de/invoicing.toml"),
        include_str!("../../../../packages/labels/fr/invoicing.toml"),
        include_str!("../../../../packages/labels/pl/invoicing.toml"),
        include_str!("../../../../packages/labels/fi/invoicing.toml"),
        include_str!("../../../../packages/labels/sv/invoicing.toml"),
        include_str!("../../../../packages/labels/es/invoicing.toml"),
    ])
}

pub fn register(d: &Dispatcher, store: &Store) {
    let m = module();
    d.register_crud(&m, store);
    d.register_transition(&m, "invoice", "cancel", store);
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
    store
        .insert(
            "invoice",
            [
                ("sale_order_id".to_string(), Value::text(order_id)),
                ("state".to_string(), Value::text("draft")),
                ("total_minor".to_string(), Value::Int(120_000)),
                ("number".to_string(), Value::Null),
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
    fn agent_proposes_human_posts() {
        let (d, s) = (Dispatcher::new(), Store::new());
        register(&d, &s);
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

    fn msg(r: Result<obop_commands::Response, CallError>) -> String {
        match r {
            Err(CallError::Handler(m)) => m,
            other => panic!("{other:?}"),
        }
    }

    /// A plain `invoicing.user` can edit invoices but cannot post one by setting `state`; the
    /// manager can, and the update runs the real post action, so the legal number is assigned.
    #[test]
    fn posting_by_update_needs_the_manager_and_still_numbers_the_invoice() {
        let (d, s) = (Dispatcher::new(), Store::new());
        register(&d, &s);
        let user = Principal::user("u", &["invoicing.user"]);
        let manager = Principal::user("m", &["invoicing.user", "invoicing.manager"]);
        let bot = Principal::agent(
            "a",
            &["invoicing.user", "invoicing.manager"],
            &["invoicing.user", "invoicing.manager"],
        );
        let id = seed(&s, "o1");
        let set = |p: &Principal, st: &str| {
            d.call(
                Surface::Rest,
                p,
                &mut Vault::new(),
                "invoice.update",
                &Value::map([("id", Value::text(&id)), ("state", Value::text(st))]),
            )
        };
        let e = msg(set(&user, "posted"));
        assert!(
            e.contains("post action") && e.contains("invoicing.manager"),
            "{e}"
        );
        assert_eq!(s.field("invoice", &id, "state"), Some(Value::text("draft")));
        // Creating a posted invoice is refused outright.
        let e = msg(d.call(
            Surface::Rest,
            &manager,
            &mut Vault::new(),
            "invoice.create",
            &Value::map([("state", Value::text("posted"))]),
        ));
        assert!(e.contains("starts as \"draft\""), "{e}");
        // An agent only proposes; the manager approves; the number is assigned.
        let Err(CallError::ApprovalRequired { plan_id }) = set(&bot, "posted") else {
            panic!("agent must only propose")
        };
        assert_eq!(s.field("invoice", &id, "state"), Some(Value::text("draft")));
        d.approve(&manager, &mut Vault::new(), &plan_id).unwrap();
        assert_eq!(
            s.field("invoice", &id, "state"),
            Some(Value::text("posted"))
        );
        assert_eq!(
            s.field("invoice", &id, "number"),
            Some(Value::text("INV-00001"))
        );
        // A posted invoice does not go back.
        let e = msg(set(&manager, "draft"));
        assert!(e.contains("no transition leaves \"posted\""), "{e}");
        // Cancelling a draft is open to the user.
        let id2 = seed(&s, "o2");
        d.call(
            Surface::Rest,
            &user,
            &mut Vault::new(),
            "invoice.update",
            &Value::map([
                ("id", Value::text(&id2)),
                ("state", Value::text("cancelled")),
            ]),
        )
        .unwrap();
        assert_eq!(
            s.field("invoice", &id2, "state"),
            Some(Value::text("cancelled"))
        );
    }
}
