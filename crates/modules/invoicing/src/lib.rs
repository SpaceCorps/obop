//! `invoicing`: customer invoices. Posting is irreversible, so agents may only propose it.
//!
//! An invoice has lines; its total is their sum. Posting needs a positive total and gives the
//! invoice the next number of a gapless yearly series (D56, D57).

use obop_commands::{Dispatcher, Store, Value};
use obop_registry::{
    CommandDef, FieldDef as F, FieldKind, Guard, ModelDef, ModelUi, Module, ModuleMeta, Reset,
};
use obop_types::DataClass::None as NoClass;

pub fn module() -> Module {
    let m = Module {
        id: "invoicing".into(),
        name: "Invoicing".into(),
        models: vec![
            ModelDef {
                name: "invoice",
                description: "A customer invoice; no action takes a posted invoice back to draft or cancels it",
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
                    )
                    .default_value("$instance.currency"),
                    F::new(
                        "state",
                        "Lifecycle: draft, posted or cancelled",
                        NoClass,
                        false,
                        "invoicing.user",
                    )
                    .select(&["draft", "posted", "cancelled"])
                    .tones(&[("posted", "ok"), ("cancelled", "bad")]),
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
                        "Number from a gapless yearly series, such as INV-2026-00042, assigned when posted",
                        NoClass,
                        false,
                        "invoicing.user",
                    )
                    .sequence("INV-{year}-{seq:05}", true, Reset::Year)
                    .assign_on("post"),
                    F::new(
                        "issue_date",
                        "Date the invoice is issued (YYYY-MM-DD)",
                        NoClass,
                        false,
                        "invoicing.user",
                    )
                    .kind(FieldKind::Date),
                    F::new(
                        "due_date",
                        "Date the payment is due (YYYY-MM-DD); not before the issue date",
                        NoClass,
                        false,
                        "invoicing.user",
                    )
                    .kind(FieldKind::Date),
                ],
                commands: vec![
                    CommandDef::new(
                        "post",
                        "Post a draft invoice and give it the next number of the series",
                        "invoicing.manager",
                    )
                    .transition("state", Some("draft"), "posted")
                    .assertion_msg(
                        "total_minor > 0",
                        "Add at least one line with an amount before posting the invoice.",
                    )
                    .assertion_msg(
                        "!is_set(due_date) || !is_set(issue_date) || due_date >= issue_date",
                        "Set a due date on or after the issue date before posting the invoice.",
                    )
                    .approval()
                    .primary()
                    .confirm("Post this invoice? It gets its number and can no longer be changed.")
                    .done("Invoice posted"),
                    CommandDef::new("cancel", "Cancel a draft invoice", "invoicing.user")
                        .transition("state", Some("draft"), "cancelled")
                        .danger()
                        .confirm("Cancel this draft invoice?")
                        .done("Invoice cancelled"),
                ],
                ext: Default::default(),
            }
            .with_ui(ModelUi {
                title: Some("{number} | Draft · {party_id.name} | Draft invoice".into()),
                subtitle: Some("{party_id.name}".into()),
                list: vec![
                    "number".into(),
                    "party_id".into(),
                    "state".into(),
                    "total_minor".into(),
                    "sale_order_id".into(),
                ],
                card: vec!["number".into(), "party_id".into(), "total_minor".into()],
                card_sum: Some("total_minor".into()),
                quick_create: vec!["party_id".into()],
                order: Some("number desc".into()),
                default_view: Some("list".into()),
                ..Default::default()
            })
            // A posted invoice is final (no action takes it back), and so is a cancelled one.
            .with_guard(
                Guard::frozen_in("state", &["posted", "cancelled"])
                    .message("A posted invoice cannot be changed."),
            ),
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
                    )
                    .fill_from("product_id.name"),
                    F::new(
                        "quantity",
                        "Quantity invoiced",
                        NoClass,
                        false,
                        "invoicing.user",
                    )
                    .kind(FieldKind::Number)
                    .required()
                    .default_int(1),
                    F::new(
                        "unit_price_minor",
                        "Price of one unit in minor currency units",
                        NoClass,
                        false,
                        "invoicing.user",
                    )
                    .kind(FieldKind::Money)
                    .required()
                    .fill_from("product_id.list_price_minor")
                    .aggregate("none"),
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
            }
            .with_ui(ModelUi {
                title: Some("{description} | {product_id.name}".into()),
                list: vec![
                    "product_id".into(),
                    "description".into(),
                    "quantity".into(),
                    "unit_price_minor".into(),
                    "amount_minor".into(),
                ],
                menu: Some("hidden".into()),
                ..Default::default()
            })
            .with_guard(Guard::child_of("invoice_id")),
        ],
        network_destinations: vec![],
        description: String::new(),
        translations: Default::default(),
        meta: ModuleMeta::first_party(
            "Customer invoices with a draft, posted and cancelled life cycle.",
            &["finance"],
            &["invoices", "billing"],
            "invoicing",
            &["base", "sales"],
        )
        .home("invoice"),
    };
    m.translated(&[
        include_str!("../../../../packages/labels/en/invoicing.toml"),
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
/// seeds are, so it has no number until it is posted.
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

    /// The dates are optional (an invoice without them still posts, as above); when both are set,
    /// the due date may not be before the issue date.
    #[test]
    fn an_invoice_posts_only_when_its_due_date_is_not_before_its_issue_date() {
        let (d, s) = (Dispatcher::new(), Store::new());
        d.set_clock(std::sync::Arc::new(|| 1_790_000_000)); // September 2026
        register(&d, &s);
        let manager = Principal::user("m", &["invoicing.user", "invoicing.manager"]);
        let mut v = Vault::new();
        let mut call =
            |cmd: &str, input: Value| d.call(Surface::Rest, &manager, &mut v, cmd, &input);
        let dates = |id: &str, issue: &str, due: &str| {
            Value::map([
                ("id", Value::text(id)),
                ("issue_date", Value::text(issue)),
                ("due_date", Value::text(due)),
            ])
        };
        let post = |id: &str| Value::map([("id", Value::text(id))]);

        let id = seed(&s, "o1");
        call("invoice.update", dates(&id, "2026-11-13", "2026-11-01")).unwrap();
        let e = format!("{:?}", call("invoice.post", post(&id)).unwrap_err());
        assert!(
            e.contains("Set a due date on or after the issue date before posting the invoice."),
            "{e}"
        );
        assert_eq!(s.field("invoice", &id, "state"), Some(Value::text("draft")));

        call("invoice.update", dates(&id, "2026-11-13", "2026-12-13")).unwrap();
        call("invoice.post", post(&id)).unwrap();
        assert_eq!(
            s.field("invoice", &id, "state"),
            Some(Value::text("posted"))
        );
        assert_eq!(
            s.field("invoice", &id, "issue_date"),
            Some(Value::text("2026-11-13"))
        );
        assert_eq!(
            s.field("invoice", &id, "due_date"),
            Some(Value::text("2026-12-13"))
        );

        // Due on the day it is issued is allowed.
        let same_day = seed(&s, "o2");
        call(
            "invoice.update",
            dates(&same_day, "2026-11-13", "2026-11-13"),
        )
        .unwrap();
        call("invoice.post", post(&same_day)).unwrap();
        assert_eq!(
            s.field("invoice", &same_day, "state"),
            Some(Value::text("posted"))
        );
    }

    /// An approval the store refuses at any one call of its run, before anything changed: the plan
    /// is still there for the retry, however the engine worded the error (the assertion of
    /// `invoice.post` wraps what its read said). Every position of the run is tried in turn.
    #[test]
    fn an_approval_refused_before_it_changed_anything_can_be_approved_again() {
        use obop_storage::fault;
        let (flaky, backend) = obop_storage::testing::Flaky::memory();
        let (d, s) = (Dispatcher::new(), Store::over(backend));
        d.set_clock(std::sync::Arc::new(|| 1_790_000_000));
        register(&d, &s);
        let agent = Principal::agent("a", &["invoicing.manager"], &["invoicing.manager"]);
        let human = Principal::user("h", &["invoicing.manager"]);
        let (mut refused_before_a_change, mut positions) = (0, 0);
        for skip in 0..40 {
            let id = seed(&s, "o1");
            let i = Value::map([("id", Value::text(&id))]);
            let mut v = Vault::new();
            let Err(CallError::ApprovalRequired { plan_id }) =
                d.call(Surface::Mcp, &agent, &mut v, "invoice.post", &i)
            else {
                panic!("a proposal")
            };
            positions += 1;
            fault::begin_request(Default::default());
            flaky.refuse_call_after(skip);
            let first = d.approve(&human, &mut v, &plan_id);
            let out = fault::end_request();
            flaky.allow_models();
            match first {
                Ok(_) => continue,               // the refusal came after the work, or never
                Err(_) if out.wrote => continue, // it ran: the plan is rightly gone
                Err(e) => {
                    refused_before_a_change += 1;
                    // Nothing was changed, so the retry finds the plan and posts the invoice.
                    d.approve(&human, &mut v, &plan_id)
                        .unwrap_or_else(|r| panic!("skip {skip}: {e:?}, then the retry: {r:?}"));
                    assert_eq!(
                        s.field("invoice", &id, "state"),
                        Some(Value::text("posted")),
                        "skip {skip}"
                    );
                }
            }
        }
        assert!(
            refused_before_a_change >= 3,
            "the sweep reached the run: {refused_before_a_change} of {positions}"
        );
    }

    fn msg(r: Result<obop_commands::Response, CallError>) -> String {
        match r {
            Err(CallError::Handler(m)) => m,
            other => panic!("{other:?}"),
        }
    }

    /// A plain `invoicing.user` can edit invoices but cannot post one by setting `state`; the
    /// manager can, and the update runs the real post action, so the number is assigned.
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

    /// B22 for the built-in invoice: a posted invoice and its lines are fixed (D77), and a new
    /// draft takes the instance currency (D95).
    #[test]
    fn a_posted_invoice_and_its_lines_cannot_be_changed() {
        let (d, s) = (Dispatcher::new(), Store::new());
        d.set_clock(std::sync::Arc::new(|| 1_790_000_000));
        register(&d, &s);
        let id = seed(&s, "o1");
        let line = s
            .insert(
                "invoice_line",
                [
                    ("invoice_id".to_string(), Value::text(&id)),
                    ("quantity".to_string(), Value::Int(1)),
                    ("unit_price_minor".to_string(), Value::Int(500)),
                ]
                .into(),
            )
            .unwrap();
        let m = Principal::user("m", &["invoicing.user", "invoicing.manager"]);
        let call = |cmd: &str, i: Value| d.call(Surface::Rest, &m, &mut Vault::new(), cmd, &i);
        let code = |r: Result<obop_commands::Response, CallError>| match r {
            Err(e) => obop_commands::errors::classify(&e).code,
            Ok(v) => panic!("expected a refusal, got {:?}", v.value),
        };
        let by_id = |i: &str| Value::map([("id", Value::text(i))]);
        call("invoice.post", by_id(&id)).unwrap();
        let set = |model: &str, i: &str, k: &str, v: Value| {
            call(
                &format!("{model}.update"),
                Value::map([("id", Value::text(i)), (k, v)]),
            )
        };
        assert_eq!(
            code(set("invoice", &id, "currency", Value::text("USD"))),
            "immutable_record"
        );
        assert_eq!(
            code(set("invoice_line", &line, "quantity", Value::Int(9))),
            "immutable_record"
        );
        assert_eq!(
            code(call("invoice_line.delete", by_id(&line))),
            "immutable_record"
        );
        assert_eq!(code(call("invoice.delete", by_id(&id))), "immutable_record");
        let fresh = call("invoice.create", Value::map([])).unwrap().value;
        let Value::Map(fresh) = fresh else { panic!() };
        assert_eq!(fresh["currency"], Value::text("EUR"));
    }
}
