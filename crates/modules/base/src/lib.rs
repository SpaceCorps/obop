//! `base` implementation: the Contacts app foundation.

use obop_commands::{Dispatcher, Store, Value};
use obop_registry::{CommandDef, FieldDef as F, FieldKind, ModelDef, Module, ModuleMeta};
use obop_types::DataClass::*;

pub fn module() -> Module {
    let m = Module {
        id: "base".into(),
        name: "Contacts".into(),
        models: vec![ModelDef {
            name: "party",
            description: "A person or organisation the business deals with",
            read_group: "base.user",
            write_group: "base.user",
            access: Option::None,
            fields: vec![
                F::new("name", "Display name", PublicDirectory, true, "base.user").required(),
                F::new(
                    "email",
                    "Work email address",
                    PublicDirectory,
                    true,
                    "base.user",
                ),
                F::new(
                    "tax_id",
                    "Tax identifier of a natural person",
                    Financial,
                    true,
                    "base.user",
                ),
                F::new(
                    "notes",
                    "Free-text internal notes",
                    Internal,
                    true,
                    "base.user",
                )
                .kind(FieldKind::LongText),
                F::new(
                    "photo",
                    "Photo or logo of the contact (PNG, JPEG, WebP or GIF, up to 2 MiB)",
                    Internal,
                    true,
                    "base.user",
                )
                .kind(FieldKind::Image(2 * 1024 * 1024)),
            ],
            commands: vec![
                CommandDef::new(
                    "erase",
                    "Anonymise the party in place and record the erasure",
                    "privacy.officer",
                )
                .approval(),
            ],
        }],
        network_destinations: vec![],
        description: String::new(),
        translations: Default::default(),
        meta: ModuleMeta::first_party(
            "Contacts and companies: the people and organisations the business deals with.",
            &["platform", "sales"],
            &["contacts", "people", "companies", "gdpr"],
            "contacts",
            &[],
        ),
    };
    m.translated(&[
        include_str!("../../../../packages/labels/de/base.toml"),
        include_str!("../../../../packages/labels/fr/base.toml"),
        include_str!("../../../../packages/labels/pl/base.toml"),
        include_str!("../../../../packages/labels/fi/base.toml"),
        include_str!("../../../../packages/labels/sv/base.toml"),
        include_str!("../../../../packages/labels/es/base.toml"),
    ])
}

pub fn register(d: &Dispatcher, store: &Store) {
    let m = module();
    d.register_crud(&m, store);
    let s = store.clone();
    let party = m.models[0].clone();
    d.register(
        &m,
        "party",
        "erase",
        Box::new(move |input| {
            let Value::Map(i) = input else {
                return Err("expected an object".into());
            };
            let Some(Value::Text(id)) = i.get("id") else {
                return Err("missing id".into());
            };
            let erased = ["name", "email", "tax_id", "notes"]
                .into_iter()
                .map(|f| (f.to_string(), Value::text("[erased]")))
                .collect();
            s.update("party", id, erased)?;
            // The photo is personal data too: clear the field and delete the file (D60).
            s.erase_blob_fields(&party, id)?;
            Ok(Value::map([
                ("id", Value::text(id)),
                ("name", Value::text("[erased]")),
            ]))
        }),
    );
}

/// Fake demo data; never real personal data.
pub fn seed(store: &Store) -> String {
    let t = |s: &str| Value::text(s);
    store
        .insert(
            "party",
            [
                ("name".to_string(), t("Ada Example")),
                ("email".to_string(), t("ada@example.test")),
                ("tax_id".to_string(), t("DE000000000")),
                ("notes".to_string(), t("Prefers invoices by email")),
            ]
            .into(),
        )
        .expect("seed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use obop_commands::Vault;
    use obop_commands::{CallError, Principal, Surface, names};

    fn setup() -> (Dispatcher, Store, String) {
        let (d, store) = (Dispatcher::new(), Store::new());
        register(&d, &store);
        let id = seed(&store);
        (d, store, id)
    }

    fn by_id(id: &str) -> Value {
        Value::map([("id", Value::text(id))])
    }

    fn field<'a>(v: &'a Value, k: &str) -> &'a Value {
        let Value::Map(m) = v else {
            panic!("not a map")
        };
        &m[k]
    }

    #[test]
    fn base_passes_install_gate() {
        assert!(obop_registry::check_module(&module()).is_empty());
    }

    #[test]
    fn user_sees_real_values() {
        let (d, _, id) = setup();
        let u = Principal::user("u1", &["base.user"]);
        let r = d
            .call(
                Surface::Rest,
                &u,
                &mut Vault::new(),
                "party.get",
                &by_id(&id),
            )
            .unwrap();
        assert_eq!(field(&r.value, "name"), &Value::text("Ada Example"));
        assert_eq!(field(&r.value, "tax_id"), &Value::text("DE000000000"));
        assert!(r.denied.is_empty());
    }

    #[test]
    fn agent_gets_masked_and_denied_fields() {
        let (d, _, id) = setup();
        let (a, mut v) = (
            Principal::agent("a1", &["base.user"], &["base.user"]),
            Vault::new(),
        );
        let r = d
            .call(Surface::Mcp, &a, &mut v, "party.get", &by_id(&id))
            .unwrap();
        for (f, label, real) in [
            ("name", "NAME_", "Ada Example"),
            ("email", "EMAIL_", "ada@example.test"),
            ("notes", "NOTES_", "Prefers invoices by email"),
        ] {
            let Value::Text(t) = field(&r.value, f) else {
                panic!()
            };
            assert!(t.starts_with(label), "{f}: {t}");
            assert_eq!(v.restore(t, |_| true), Ok(real));
        }
        assert_eq!(r.denied, vec!["party.tax_id".to_string()]);
    }

    #[test]
    fn same_call_on_all_surfaces_gives_same_result() {
        let (d, _, id) = setup();
        let u = Principal::user("u1", &["base.user"]);
        let ids = [
            names::from_rest("POST", "/api/party/get").unwrap(),
            names::from_mcp("party__get").unwrap(),
            names::from_cli(&["party", "get"]).unwrap(),
        ];
        let rs: Vec<_> = [Surface::Rest, Surface::Mcp, Surface::Cli]
            .into_iter()
            .zip(&ids)
            .map(|(s, c)| d.call(s, &u, &mut Vault::new(), c, &by_id(&id)).unwrap())
            .collect();
        assert!(rs.windows(2).all(|w| w[0] == w[1]));
    }

    #[test]
    fn agent_scope_narrows_user_permission() {
        let (d, _, id) = setup();
        let narrow = Principal::agent("a1", &["base.user"], &["sales.user"]);
        assert_eq!(
            d.call(
                Surface::Mcp,
                &narrow,
                &mut Vault::new(),
                "party.get",
                &by_id(&id)
            ),
            Err(CallError::Forbidden("party.get".into()))
        );
        assert!(d.visible(Surface::Mcp, &narrow).is_empty());
    }

    #[test]
    fn create_rejects_undeclared_fields_and_restores_tokens() {
        let (d, _, _) = setup();
        let u = Principal::user("u1", &["base.user"]);
        let bad = Value::map([("nickname", Value::text("x"))]);
        assert!(matches!(
            d.call(Surface::Rest, &u, &mut Vault::new(), "party.create", &bad),
            Err(CallError::Handler(_))
        ));

        // An agent writes back a token it was shown; the store receives the real value.
        let a = Principal::agent("a1", &["base.user"], &["base.user"]);
        let mut v = Vault::new();
        let token = v.mask("NOTES", "party:x", "real note");
        let input = Value::map([
            ("name", Value::text("New Co")),
            ("notes", Value::Text(token)),
        ]);
        let r = d
            .call(Surface::Mcp, &a, &mut v, "party.create", &input)
            .unwrap();
        let Value::Text(id) = field(&r.value, "id") else {
            panic!()
        };
        let got = d
            .call(
                Surface::Rest,
                &u,
                &mut Vault::new(),
                "party.get",
                &by_id(id),
            )
            .unwrap();
        assert_eq!(field(&got.value, "notes"), &Value::text("real note"));
    }

    #[test]
    fn erase_needs_approval_for_agents() {
        let (d, _, id) = setup();
        let officer = Principal::user("u2", &["base.user", "privacy.officer"]);
        let agent = Principal::agent(
            "a2",
            &["base.user", "privacy.officer"],
            &["base.user", "privacy.officer"],
        );
        let mut v = Vault::new();
        let Err(CallError::ApprovalRequired { plan_id }) =
            d.call(Surface::Mcp, &agent, &mut v, "party.erase", &by_id(&id))
        else {
            panic!("agent should only propose")
        };
        // Nothing happened yet.
        let r = d
            .call(Surface::Rest, &officer, &mut v, "party.get", &by_id(&id))
            .unwrap();
        assert_eq!(field(&r.value, "name"), &Value::text("Ada Example"));
        // An agent cannot approve; a human can.
        assert!(matches!(
            d.approve(&agent, &mut v, &plan_id),
            Err(CallError::Forbidden(_))
        ));
        d.approve(&officer, &mut v, &plan_id).unwrap();
        let r = d
            .call(Surface::Rest, &officer, &mut v, "party.get", &by_id(&id))
            .unwrap();
        assert_eq!(field(&r.value, "name"), &Value::text("[erased]"));
        assert!(matches!(
            d.approve(&officer, &mut v, &plan_id),
            Err(CallError::UnknownPlan(_))
        ));
    }

    #[test]
    fn a_human_may_run_it_directly() {
        let (d, _, id) = setup();
        let officer = Principal::user("u2", &["base.user", "privacy.officer"]);
        assert!(
            d.call(
                Surface::Cli,
                &officer,
                &mut Vault::new(),
                "party.erase",
                &by_id(&id)
            )
            .is_ok()
        );
    }
}
