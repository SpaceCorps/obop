//! `sales`: quotations and orders.

use obop_commands::{Dispatcher, Store, Value};
use obop_registry::{
    CommandDef, Effect, FieldDef as F, FieldKind, ModelDef, Module, ModuleMeta, Reset,
};
use obop_types::DataClass::None as NoClass;

pub fn module() -> Module {
    let m = Module {
        id: "sales".into(),
        name: "Sales".into(),
        models: vec![
            ModelDef {
                name: "sale_order",
                description: "A quotation or confirmed order for a customer",
                read_group: "sales.user",
                write_group: "sales.user",
                access: None,
                fields: vec![
                    F::new(
                        "number",
                        "Order number such as SO-00042, assigned when the order is created",
                        NoClass,
                        false,
                        "sales.user",
                    )
                    .sequence("SO-{seq:05}", false, Reset::Never),
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
                        "Order total in minor units, the sum of the lines",
                        NoClass,
                        false,
                        "sales.user",
                    )
                    .kind(FieldKind::Money)
                    .computed("sum(lines.amount_minor)"),
                    F::new(
                        "currency",
                        "ISO 4217 currency code",
                        NoClass,
                        false,
                        "sales.user",
                    ),
                ],
                commands: vec![
                    CommandDef::new("confirm", "Confirm a draft order", "sales.user")
                        .transition("state", Some("draft"), "confirmed")
                        // A confirmed order has a draft invoice with the same lines, ready for
                        // accounting to post (D56). The caller needs `invoicing.user` for it.
                        .effect(
                            Effect::create(
                                "invoice",
                                &[
                                    ("sale_order_id", "$self.id"),
                                    ("party_id", "$self.party_id"),
                                    ("currency", "$self.currency"),
                                ],
                            )
                            .named("invoice"),
                        )
                        .effect(
                            Effect::create(
                                "invoice_line",
                                &[
                                    ("invoice_id", "$created.invoice"),
                                    ("product_id", "$line.product_id"),
                                    ("description", "$line.description"),
                                    ("quantity", "$line.quantity"),
                                    ("unit_price_minor", "$line.unit_price_minor"),
                                ],
                            )
                            .for_each("lines"),
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
            },
            ModelDef {
                name: "sale_order_line",
                description: "One product or service on a sale order",
                read_group: "sales.user",
                write_group: "sales.user",
                access: None,
                fields: vec![
                    F::new(
                        "order_id",
                        "The sale order the line belongs to",
                        NoClass,
                        false,
                        "sales.user",
                    )
                    .reference("sale_order")
                    .required(),
                    F::new(
                        "product_id",
                        "The product sold",
                        NoClass,
                        false,
                        "sales.user",
                    )
                    .reference("product"),
                    F::new(
                        "description",
                        "What is sold, as it appears on the invoice",
                        NoClass,
                        false,
                        "sales.user",
                    ),
                    F::new("quantity", "Quantity sold", NoClass, false, "sales.user")
                        .kind(FieldKind::Number)
                        .required(),
                    F::new(
                        "unit_price_minor",
                        "Price of one unit in minor currency units",
                        NoClass,
                        false,
                        "sales.user",
                    )
                    .kind(FieldKind::Money)
                    .required(),
                    F::new(
                        "amount_minor",
                        "Line amount in minor units: quantity times unit price",
                        NoClass,
                        false,
                        "sales.user",
                    )
                    .kind(FieldKind::Money)
                    .computed("quantity * unit_price_minor"),
                ],
                commands: vec![],
            },
        ],
        network_destinations: vec![],
        description: String::new(),
        translations: Default::default(),
        meta: ModuleMeta::first_party(
            "Quotations and sales orders for your customers.",
            &["sales"],
            &["quotations", "orders"],
            "sales",
            &["base", "product"],
        ),
    };
    m.translated(&[
        include_str!("../../../../packages/labels/de/sales.toml"),
        include_str!("../../../../packages/labels/fr/sales.toml"),
        include_str!("../../../../packages/labels/pl/sales.toml"),
        include_str!("../../../../packages/labels/fi/sales.toml"),
        include_str!("../../../../packages/labels/sv/sales.toml"),
        include_str!("../../../../packages/labels/es/sales.toml"),
    ])
}

pub fn register(d: &Dispatcher, store: &Store) {
    let m = module();
    d.register_crud(&m, store);
    for command in ["confirm", "cancel", "reset_to_draft"] {
        d.register_transition(&m, "sale_order", command, store);
    }
}

/// A demo order for `party_id` with two lines. Written straight into the store, as seeds are,
/// so the order has no number until `obop sequences backfill` gives it one.
pub fn seed(store: &Store, party_id: &str) -> String {
    let order = store
        .insert(
            "sale_order",
            [
                ("party_id".to_string(), Value::text(party_id)),
                ("state".to_string(), Value::text("draft")),
                ("currency".to_string(), Value::text("EUR")),
            ]
            .into(),
        )
        .expect("seed");
    for (what, qty, price) in [
        ("Consulting day", 2, 45_000),
        ("Travel expenses", 1, 30_000),
    ] {
        store
            .insert(
                "sale_order_line",
                [
                    ("order_id".to_string(), Value::text(&order)),
                    ("description".to_string(), Value::text(what)),
                    ("quantity".to_string(), Value::Int(qty)),
                    ("unit_price_minor".to_string(), Value::Int(price)),
                ]
                .into(),
            )
            .expect("seed");
    }
    order
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
        invoicing::register(&d, &s);
        let party = s
            .insert("party", [("name".to_string(), Value::text("Acme"))].into())
            .unwrap();
        let id = seed(&s, &party);
        let u = Principal::user("u", &["sales.user", "invoicing.user"]);
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
        invoicing::register(&d, &s);
        let u = Principal::user("u", &["sales.user", "invoicing.user"]);
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
        // Ordinary fields are never blocked by it; the total is computed, so it is read-only.
        call(
            "sale_order.update",
            Value::map([("id", Value::text(&id)), ("currency", Value::text("USD"))]),
        )
        .unwrap();
        let e = msg(call(
            "sale_order.update",
            Value::map([("id", Value::text(&id)), ("total_minor", Value::Int(5))]),
        ));
        assert!(e.contains("computed and read-only"), "{e}");
    }

    #[test]
    fn confirming_an_order_creates_a_draft_invoice_with_the_same_lines() {
        let (d, s) = (Dispatcher::new(), Store::new());
        d.set_clock(std::sync::Arc::new(|| 1_790_000_000));
        register(&d, &s);
        invoicing::register(&d, &s);
        let party = s
            .insert("party", [("name".to_string(), Value::text("Acme"))].into())
            .unwrap();
        let order = seed(&s, &party);
        let sales = Principal::user("u", &["sales.user", "invoicing.user"]);
        let manager = Principal::user("m", &["invoicing.user", "invoicing.manager"]);
        let run = |p: &Principal, cmd: &str, i: Value| {
            d.call(Surface::Rest, p, &mut Vault::new(), cmd, &i)
        };
        let by_id = |id: &str| Value::map([("id", Value::text(id))]);
        // The order total is the sum of its lines: 2 x 450.00 + 300.00.
        assert_eq!(
            s.field("sale_order", &order, "total_minor"),
            None,
            "computed, not stored"
        );
        let Value::Map(row) = run(&sales, "sale_order.get", by_id(&order)).unwrap().value else {
            panic!()
        };
        assert_eq!(row["total_minor"], Value::Int(120_000));
        run(&sales, "sale_order.confirm", by_id(&order)).unwrap();
        let Value::List(invoices) = run(&manager, "invoice.list", Value::map([])).unwrap().value
        else {
            panic!()
        };
        assert_eq!(invoices.len(), 1);
        let Value::Map(inv) = &invoices[0] else {
            panic!()
        };
        assert_eq!(inv["sale_order_id"], Value::text(&order));
        assert_eq!(inv["party_id"], Value::text(&party));
        assert_eq!(inv["state"], Value::text("draft"));
        assert_eq!(inv["total_minor"], Value::Int(120_000));
        assert_eq!(s.count("invoice_line"), 2);
        let Value::Text(inv_id) = inv["id"].clone() else {
            panic!()
        };
        // Posting numbers it; the assertion refuses an invoice without lines.
        run(&manager, "invoice.post", by_id(&inv_id)).unwrap();
        assert_eq!(
            s.field("invoice", &inv_id, "number"),
            Some(Value::text("INV-2026-00001"))
        );
        let empty = run(&sales, "invoice.create", Value::map([])).unwrap();
        let Value::Map(empty) = empty.value else {
            panic!()
        };
        let Err(CallError::Handler(e)) = run(
            &manager,
            "invoice.post",
            by_id(match &empty["id"] {
                Value::Text(t) => t,
                _ => panic!(),
            }),
        ) else {
            panic!()
        };
        assert!(e.contains("no amount"), "{e}");
        // Without the invoicing group the confirmation is refused and nothing happens.
        let order2 = seed(&s, &party);
        let plain = Principal::user("p", &["sales.user"]);
        let Err(CallError::Handler(e)) = run(&plain, "sale_order.confirm", by_id(&order2)) else {
            panic!()
        };
        assert!(e.contains("invoicing.user"), "{e}");
        assert_eq!(
            s.field("sale_order", &order2, "state"),
            Some(Value::text("draft"))
        );
        assert_eq!(s.count("invoice"), 2);
    }
}
