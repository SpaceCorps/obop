//! `invoicing`: customer invoices. Posting is irreversible, so agents may only propose it.
//!
//! An invoice has lines; its total is their sum. Posting needs a positive total and gives the
//! invoice the next number of a gapless yearly series (D56, D57).

use obop_commands::{Dispatcher, Store, Value};
use obop_registry::{CommandDef, FieldDef as F, FieldKind, ModelDef, Module, ModuleMeta, Reset};
use obop_types::DataClass::None as NoClass;

pub fn module() -> Module {
    let m = Module {
        id: "invoicing".into(),
        name: "Invoicing".into(),
        models: vec![
            ModelDef {
                name: "invoice",
                description: "A customer invoice; posted invoices are corrected only by credit notes",
                read_group: "invoicing.user",
                write_group: "invoicing.user",
                access: None,
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
                        "party_id",
                        "Customer party id",
                        NoClass,
                        false,
                        "invoicing.user",
                    )
                    .reference("party"),
                    F::new(
                        "currency",
                        "ISO 4217 currency code",
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
                    )
                    .select(&["draft", "posted", "cancelled"]),
                    F::new(
                        "total_minor",
                        "Invoice total in minor units, the sum of the lines",
                        NoClass,
                        false,
                        "invoicing.user",
                    )
                    .kind(FieldKind::Money)
                    .computed("sum(lines.amount_minor)"),
                    F::new(
                        "number",
                        "Gapless legal number such as INV-2026-00042, assigned when posted",
                        NoClass,
                        false,
                        "invoicing.user",
                    )
                    .sequence("INV-{year}-{seq:05}", true, Reset::Year)
                    .assign_on("post"),
                ],
                commands: vec![
                    CommandDef::new(
                        "post",
                        "Post a draft invoice and assign its legal number",
                        "invoicing.manager",
                    )
                    .transition("state", Some("draft"), "posted")
                    .assertion_msg(
                        "total_minor > 0",
                        "An invoice with no amount cannot be posted",
                    )
                    .approval(),
                    CommandDef::new("cancel", "Cancel a draft invoice", "invoicing.user")
                        .transition("state", Some("draft"), "cancelled"),
                ],
                ext: Default::default(),
            },
            ModelDef {
                name: "invoice_line",
                description: "One product or service on an invoice",
                read_group: "invoicing.user",
                write_group: "invoicing.user",
                access: None,
                fields: vec![
                    F::new(
                        "invoice_id",
                        "The invoice the line belongs to",
                        NoClass,
                        false,
                        "invoicing.user",
                    )
                    .reference("invoice")
                    .required(),
                    F::new(
                        "product_id",
                        "The product invoiced",
                        NoClass,
                        false,
                        "invoicing.user",
                    )
                    .reference("product"),
                    F::new(
                        "description",
                        "What is invoiced",
                        NoClass,
                        false,
                        "invoicing.user",
                    ),
                    F::new(
                        "quantity",
                        "Quantity invoiced",
                        NoClass,
                        false,
                        "invoicing.user",
                    )
                    .kind(FieldKind::Number)
                    .required(),
                    F::new(
                        "unit_price_minor",
                        "Price of one unit in minor currency units",
                        NoClass,
                        false,
                        "invoicing.user",
                    )
                    .kind(FieldKind::Money)
                    .required(),
                    F::new(
                        "amount_minor",
                        "Line amount in minor units: quantity times unit price",
                        NoClass,
                        false,
                        "invoicing.user",
                    )
                    .kind(FieldKind::Money)
                    .computed("quantity * unit_price_minor"),
                ],
                commands: vec![],
                ext: Default::default(),
            },
        ],
        network_destinations: vec![],
        description: String::new(),
        translations: Default::default(),
        meta: ModuleMeta::first_party(
            "Customer invoices with a draft, posted and paid life cycle.",
            &["finance"],
            &["invoices", "billing"],
            "invoicing",
            &["base", "sales"],
        ),
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
    for command in ["post", "cancel"] {
        d.register_transition(&m, "invoice", command, store);
    }
}

/// A demo draft invoice for `order_id` with one line. Written straight into the store, as
/// seeds are, so it has no legal number until it is posted.
pub fn seed(store: &Store, order_id: &str) -> String {
    let invoice = store
        .insert(
            "invoice",
            [
                ("sale_order_id".to_string(), Value::text(order_id)),
                ("state".to_string(), Value::text("draft")),
            ]
            .into(),
        )
        .expect("seed");
    store
        .insert(
            "invoice_line",
            [
                ("invoice_id".to_string(), Value::text(&invoice)),
                ("description".to_string(), Value::text("Consulting")),
                ("quantity".to_string(), Value::Int(1)),
                ("unit_price_minor".to_string(), Value::Int(120_000)),
            ]
            .into(),
        )
        .expect("seed");
    invoice
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
        d.set_clock(std::sync::Arc::new(|| 1_790_000_000)); // September 2026
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
            Some(Value::text("INV-2026-00001"))
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
        d.set_clock(std::sync::Arc::new(|| 1_790_000_000)); // September 2026
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
            Some(Value::text("INV-2026-00001"))
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
