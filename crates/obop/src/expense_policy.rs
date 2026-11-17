//! The company setting `expenses.allow_self_approval` and the rule it switches (owner ruling of
//! 2026-11-17): the server refuses the approval of an expense claim by the person who made it
//! (its `employee`) unless the setting is on. The default is off, so a claim is approved by
//! someone else.
//!
//! The setting is a key of the instance settings (`company_settings`), read and changed through
//! the commands `expenses_setting.get|update` (REST, MCP and `obop call`); changing it needs
//! the administrator group. The rule wraps `expenses_expense.approve`, so it holds for a direct
//! call, for an approved plan (the person who approves the plan is the one who approves the
//! claim) and for an `update` that moves the state to approved (it runs the action).

use crate::company_settings;
use obop_commands::{ADMIN_GROUP, Dispatcher, Principal, Store, Value};
use obop_registry::{CommandDef, FieldDef, FieldKind, ModelDef, Module};
use obop_types::DataClass;
use serde_json::{Map, Value as J};
use std::sync::Arc;

const MODEL: &str = "expenses_setting";
const SECTION: &str = "expenses";
const KEY: &str = "allow_self_approval";
const READ: &str = "expenses.user";

pub fn module() -> Module {
    Module {
        id: "expenses_policy".into(),
        name: "Expense settings".into(),
        description: "The company settings of expense claims".into(),
        models: vec![ModelDef {
            name: MODEL,
            description: "The company settings of expense claims: one record, `company`. `allow_self_approval` lets the person who made a claim approve it too (off by default: the server refuses it). Changing it needs the administrator group",
            read_group: READ,
            write_group: ADMIN_GROUP,
            access: None,
            fields: vec![
                FieldDef::new(
                    KEY,
                    "Whether the person who made an expense claim may approve it too; off (the default) means someone else approves it",
                    DataClass::None,
                    false,
                    READ,
                )
                .kind(FieldKind::Bool),
            ],
            commands: vec![
                CommandDef::new("get", "The expense settings of the company", READ),
                CommandDef::new(
                    "update",
                    "Change the expense settings: allow_self_approval true or false (administrators only)",
                    ADMIN_GROUP,
                ),
            ],
            ext: Default::default(),
        }],
        ..Module::default()
    }
}

/// Whether the company allows the person who made a claim to approve it.
pub fn allows_self_approval(b: &dyn obop_storage::Backend) -> Result<bool, String> {
    company_settings::flag(b, SECTION, KEY, false)
}

fn row(b: &dyn obop_storage::Backend) -> Result<Value, String> {
    Ok(Value::map([
        ("id", Value::text("company")),
        (KEY, Value::Bool(allows_self_approval(b)?)),
    ]))
}

/// The refusal, in the language of the request (machine-drafted translations).
pub fn refusal_text(lang: Option<&str>) -> &'static str {
    match lang {
        Some("de") => {
            "Sie haben diese Abrechnung selbst eingereicht, daher muss sie jemand anderes genehmigen. Eine Administratorin oder ein Administrator kann die Selbstgenehmigung in den Spesen-Einstellungen erlauben."
        }
        Some("fr") => {
            "Vous avez fait cette note de frais vous-même : quelqu'un d'autre doit l'approuver. Un administrateur peut autoriser l'auto-approbation dans les paramètres des notes de frais."
        }
        Some("pl") => {
            "To Ty złożyłeś ten wniosek, więc musi go zatwierdzić ktoś inny. Administrator może zezwolić na samodzielne zatwierdzanie w ustawieniach wydatków."
        }
        Some("fi") => {
            "Teit tämän kulukorvauksen itse, joten jonkun toisen on hyväksyttävä se. Ylläpitäjä voi sallia oman hyväksynnän kulujen asetuksissa."
        }
        Some("sv") => {
            "Du har själv gjort det här utlägget, så någon annan måste godkänna det. En administratör kan tillåta självgodkännande i inställningarna för utlägg."
        }
        Some("es") => {
            "Usted hizo esta solicitud de gastos, así que otra persona debe aprobarla. Un administrador puede permitir la autoaprobación en los ajustes de gastos."
        }
        _ => {
            "You made this expense claim, so someone else has to approve it. An administrator can allow self-approval in the Expenses settings."
        }
    }
}

fn text_of(v: &Value, k: &str) -> String {
    match v {
        Value::Map(m) => match m.get(k) {
            Some(Value::Text(t)) => t.to_string(),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

/// Who approves: the person who approved the plan, else the person behind the caller.
fn approver(p: &Principal, input: &Value) -> String {
    let stamped = text_of(
        input,
        &format!("{}_approver", obop_commands::query::ACTOR_PREFIX),
    );
    let id = if stamped.is_empty() {
        p.id.clone()
    } else {
        stamped
    };
    id.strip_suffix("/agent").unwrap_or(&id).to_string()
}

pub fn register(d: &Arc<Dispatcher>, store: &Store) {
    let m = module();
    let b = store.backend();
    d.register(&m, MODEL, "get", Box::new(move |_| row(&*b)));
    let b = store.backend();
    d.register(
        &m,
        MODEL,
        "update",
        Box::new(move |input| {
            let Value::Map(x) = input else {
                return Err("expected an object".into());
            };
            let mut patch = Map::new();
            match x.get(KEY) {
                Some(Value::Bool(v)) => {
                    patch.insert(KEY.into(), J::Bool(*v));
                }
                Some(Value::Text(t)) if t == "true" || t == "false" => {
                    patch.insert(KEY.into(), J::Bool(t == "true"));
                }
                Some(Value::Null) | None => {}
                Some(_) => return Err(format!("{KEY} must be true or false")),
            }
            if !patch.is_empty() {
                company_settings::update(&*b, SECTION, &patch)?;
            }
            row(&*b)
        }),
    );
    let (b, s) = (store.backend(), store.clone());
    d.around(
        "expenses_expense.approve",
        Box::new(move |p, input, inner| {
            let id = text_of(input, "id");
            if !id.is_empty() && !allows_self_approval(&*b)? {
                let employee = s
                    .backend()
                    .get("expenses_expense", &id)
                    .map_err(|e| e.to_string())?
                    .and_then(|f| f.get("employee").and_then(J::as_str).map(String::from))
                    .unwrap_or_default();
                if !employee.is_empty() && employee == approver(p, input) {
                    return Err(refusal_text(obop_commands::locale::language()).into());
                }
            }
            inner(input)
        }),
    );
}
