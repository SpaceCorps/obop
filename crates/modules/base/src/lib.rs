//! `base` implementation: the Contacts app foundation. Declares its registry metadata and
//! binds its command handlers.

use obop_commands::{Dispatcher, Value};
use obop_registry::{CommandDef, FieldDef, ModelDef, Module, Surfaces};
use obop_types::DataClass;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

fn field(
    name: &'static str,
    description: &'static str,
    class: DataClass,
    personal: bool,
) -> FieldDef {
    FieldDef {
        name,
        description,
        class: Some(class),
        permission_group: "base.user",
        personal_data: personal,
    }
}

pub fn module() -> Module {
    Module {
        id: "base".into(),
        models: vec![ModelDef {
            name: base_api::models::PARTY,
            description: "A person or organisation the business deals with",
            fields: vec![
                field("name", "Display name", DataClass::PublicDirectory, true),
                field(
                    "email",
                    "Work email address",
                    DataClass::PublicDirectory,
                    true,
                ),
                field(
                    "tax_id",
                    "Tax identifier of a natural person",
                    DataClass::Financial,
                    true,
                ),
                field(
                    "notes",
                    "Free-text internal notes",
                    DataClass::Internal,
                    true,
                ),
            ],
            commands: vec![
                CommandDef {
                    name: "get",
                    description: "Read one party by id",
                    permission_group: "base.user",
                    surfaces: Surfaces::ALL,
                },
                CommandDef {
                    name: "erase",
                    description: "Anonymise the party in place and record the erasure",
                    permission_group: "privacy.officer",
                    surfaces: Surfaces::ALL,
                },
            ],
        }],
        network_destinations: vec![],
    }
}

/// In-memory party store until `obop-orm` exists.
pub type Store = Rc<RefCell<BTreeMap<String, BTreeMap<String, String>>>>;

pub fn register(d: &mut Dispatcher, store: Store) {
    let m = module();
    let s = store.clone();
    d.register(
        &m,
        "party",
        "get",
        Box::new(move |input| {
            let Value::Map(i) = input else {
                return Err("expected an object".into());
            };
            let Some(Value::Text(id)) = i.get("id") else {
                return Err("missing id".into());
            };
            let s = s.borrow();
            let row = s.get(id).ok_or_else(|| format!("no party {id}"))?;
            let mut out: BTreeMap<String, Value> = row
                .iter()
                .map(|(k, v)| (k.clone(), Value::Text(v.clone())))
                .collect();
            out.insert("id".into(), Value::Text(id.clone()));
            Ok(Value::Map(out))
        }),
    );
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
            let mut s = store.borrow_mut();
            let row = s.get_mut(id).ok_or_else(|| format!("no party {id}"))?;
            for f in ["name", "email", "tax_id", "notes"] {
                row.insert(f.into(), "[erased]".into());
            }
            Ok(Value::map([
                ("id", Value::text(id)),
                ("erased", Value::Bool(true)),
            ]))
        }),
    );
}

/// Fake demo data for tests and `obop call`; never real personal data.
pub fn demo_store() -> Store {
    let row = |n: &str, e: &str, t: &str, no: &str| {
        BTreeMap::from([
            ("name".to_string(), n.to_string()),
            ("email".to_string(), e.to_string()),
            ("tax_id".to_string(), t.to_string()),
            ("notes".to_string(), no.to_string()),
        ])
    };
    Rc::new(RefCell::new(BTreeMap::from([(
        "1".to_string(),
        row(
            "Ada Example",
            "ada@example.test",
            "DE000000000",
            "Prefers invoices by email",
        ),
    )])))
}

#[cfg(test)]
mod tests {
    use super::*;
    use obop_commands::{CallError, Principal, Surface, names};
    use obop_masking::Vault;

    fn setup() -> (Dispatcher, Vault) {
        let mut d = Dispatcher::new();
        register(&mut d, demo_store());
        (d, Vault::new())
    }

    fn get(id: &str) -> Value {
        Value::map([("id", Value::text(id))])
    }

    #[test]
    fn base_passes_install_gate() {
        assert!(obop_registry::check_module(&module()).is_empty());
    }

    #[test]
    fn user_sees_real_values() {
        let (d, mut v) = setup();
        let u = Principal::user("u1", &["base.user"]);
        let r = d
            .call(Surface::Rest, &u, &mut v, "party.get", &get("1"))
            .unwrap();
        let Value::Map(m) = r.value else { panic!() };
        assert_eq!(m["name"], Value::text("Ada Example"));
        assert_eq!(m["tax_id"], Value::text("DE000000000"));
        assert!(r.denied.is_empty());
    }

    #[test]
    fn agent_gets_masked_and_denied_fields() {
        let (d, mut v) = setup();
        let a = Principal::agent("a1", &["base.user"], &["base.user"]);
        let r = d
            .call(Surface::Mcp, &a, &mut v, "party.get", &get("1"))
            .unwrap();
        let Value::Map(m) = r.value else { panic!() };
        for (field, label, real) in [
            ("name", "NAME_", "Ada Example"),
            ("email", "EMAIL_", "ada@example.test"),
            ("notes", "NOTES_", "Prefers invoices by email"),
        ] {
            let Value::Text(t) = &m[field] else { panic!() };
            assert!(t.starts_with(label), "{field}: {t}");
            assert_eq!(v.restore(t, |_| true), Ok(real));
        }
        assert!(!m.contains_key("tax_id"));
        assert_eq!(r.denied, vec!["party.tax_id".to_string()]);
    }

    #[test]
    fn same_call_on_all_surfaces_gives_same_result() {
        let (d, _) = setup();
        let u = Principal::user("u1", &["base.user"]);
        let ids = [
            names::from_rest("POST", "/api/party/get").unwrap(),
            names::from_mcp("party__get").unwrap(),
            names::from_cli(&["party", "get"]).unwrap(),
        ];
        let results: Vec<_> = [Surface::Rest, Surface::Mcp, Surface::Cli]
            .into_iter()
            .zip(&ids)
            .map(|(s, id)| d.call(s, &u, &mut Vault::new(), id, &get("1")).unwrap())
            .collect();
        assert!(results.windows(2).all(|w| w[0] == w[1]));
    }

    #[test]
    fn agent_scope_narrows_user_permission() {
        let (d, mut v) = setup();
        let narrow = Principal::agent("a1", &["base.user"], &["sales.user"]);
        assert_eq!(
            d.call(Surface::Mcp, &narrow, &mut v, "party.get", &get("1")),
            Err(CallError::Forbidden("party.get".into()))
        );
        assert!(d.visible(Surface::Mcp, &narrow).is_empty());
    }

    #[test]
    fn privileged_command_needs_its_group() {
        let (d, mut v) = setup();
        let u = Principal::user("u1", &["base.user"]);
        assert!(matches!(
            d.call(Surface::Cli, &u, &mut v, "party.erase", &get("1")),
            Err(CallError::Forbidden(_))
        ));
        let officer = Principal::user("u2", &["base.user", "privacy.officer"]);
        assert!(
            d.call(Surface::Cli, &officer, &mut v, "party.erase", &get("1"))
                .is_ok()
        );
        assert_eq!(
            d.visible(Surface::Cli, &officer),
            vec!["party.erase", "party.get"]
        );
    }

    #[test]
    fn masked_token_in_input_is_restored() {
        let (d, mut v) = setup();
        let a = Principal::agent("a1", &["base.user"], &["base.user"]);
        d.call(Surface::Mcp, &a, &mut v, "party.get", &get("1"))
            .unwrap();
        // The agent passes back the token it was shown; the handler sees the real id.
        let id_token = v.mask("ID", "party:1", "1");
        let r = d
            .call(Surface::Mcp, &a, &mut v, "party.get", &get(&id_token))
            .unwrap();
        let Value::Map(m) = r.value else { panic!() };
        assert_eq!(m["id"], Value::text("1"));
    }

    #[test]
    fn unknown_and_hidden_commands() {
        let (d, mut v) = setup();
        let u = Principal::user("u1", &["base.user"]);
        assert_eq!(
            d.call(Surface::Rest, &u, &mut v, "party.nope", &get("1")),
            Err(CallError::UnknownCommand("party.nope".into()))
        );
    }
}
