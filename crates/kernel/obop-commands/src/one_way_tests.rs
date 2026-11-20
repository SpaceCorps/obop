//! `one_way` in `meta` (#102): a step is one with no way back only when no action of the model,
//! whatever group may run it, moves the field out of the state it reaches. A leave request that an
//! employee sends and a manager decides is not one, though the employee's `meta` lists only the
//! request.

use crate::{Dispatcher, Principal, Store, Surface};
use obop_registry::{CommandDef, FieldDef as F, ModelDef, Module, intern};
use obop_types::DataClass;
use std::sync::Arc;

const EMP: &str = "onew.user";
const MGR: &str = "onew.manager";

fn module(p: &str) -> Module {
    let f = |name: &'static str| F::new(name, "d", DataClass::None, false, EMP);
    Module {
        id: p.into(),
        name: "W".into(),
        models: vec![ModelDef {
            name: intern(&format!("{p}_leave")),
            description: "d",
            read_group: EMP,
            write_group: EMP,
            access: None,
            fields: vec![
                f("name"),
                f("state").select(&["draft", "requested", "approved", "refused", "archived"]),
            ],
            commands: vec![
                CommandDef::new("request", "Request", EMP).transition(
                    "state",
                    Some("draft"),
                    "requested",
                ),
                CommandDef::new("approve", "Approve", MGR).transition(
                    "state",
                    Some("requested"),
                    "approved",
                ),
                CommandDef::new("refuse", "Refuse", MGR).transition(
                    "state",
                    Some("requested"),
                    "refused",
                ),
                CommandDef::new("archive", "Archive", EMP).transition(
                    "state",
                    Some("refused"),
                    "archived",
                ),
            ],
            ext: Default::default(),
        }],
        ..Module::default()
    }
}

fn one_way_of(d: &Dispatcher, who: &Principal, model: &str) -> Vec<(String, Option<bool>)> {
    let meta = d.meta(Surface::Rest, who);
    let m = meta
        .iter()
        .flat_map(|x| x.models.iter())
        .find(|x| x.name == model)
        .unwrap();
    m.actions
        .iter()
        .map(|a| (a.id.rsplit('.').next().unwrap().to_string(), a.one_way))
        .collect()
}

#[test]
fn a_step_that_someone_else_decides_afterwards_is_not_one_way() {
    let p = format!("w{}", &uuid::Uuid::now_v7().simple().to_string()[22..]);
    let store = Store::open("memory:").unwrap();
    let d = Arc::new(Dispatcher::new());
    let module = module(&p);
    let v = obop_registry::check_module(&module);
    assert!(v.is_empty(), "{v:?}");
    d.register_crud(&module, &store);
    for mo in &module.models {
        for c in &mo.commands {
            d.register_transition(&module, mo.name, c.name, &store);
        }
    }
    let model = format!("{p}_leave");
    // The employee sees Request and Archive only; Request still has a way out (the manager's).
    let employee = Principal::user("eve", &[EMP]);
    assert_eq!(
        one_way_of(&d, &employee, &model),
        vec![
            ("request".to_string(), Some(false)),
            ("archive".to_string(), Some(true)),
        ]
    );
    // The manager sees all four: approve leaves a state nothing leaves; refuse can be archived.
    let manager = Principal::user("max", &[EMP, MGR]);
    assert_eq!(
        one_way_of(&d, &manager, &model),
        vec![
            ("request".to_string(), Some(false)),
            ("approve".to_string(), Some(true)),
            ("refuse".to_string(), Some(false)),
            ("archive".to_string(), Some(true)),
        ]
    );
}
