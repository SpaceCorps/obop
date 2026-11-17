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
//!
//! Names are compared as the sign-in compares them (trimmed, lower case), so `ROOT` or ` root`
//! is the account `root`. Once a claim is submitted its `employee` no longer changes (a wrapper of
//! `expenses_expense.update`), so the maker cannot put another name on a claim and then approve
//! it. A claim written in another person's name while it is a draft is that person's claim, but
//! the person who submitted it (`submitted_by`, stamped by the submit action since expenses
//! 0.5.0) may not approve it either (U7). The rule is checked again when a person approves an
//! agent's plan, before the plan is spent (`Dispatcher::before_approve`), so a claimant who is
//! refused leaves the plan for someone else.

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

/// The refusal of a changed employee on a claim that is no longer a draft.
pub fn frozen_employee_text(lang: Option<&str>) -> &'static str {
    match lang {
        Some("de") => {
            "Die Person einer eingereichten Abrechnung kann nicht mehr geändert werden. Lehnen Sie die Abrechnung ab und legen Sie eine neue an, wenn sie falsch ist."
        }
        Some("fr") => {
            "La personne d'une note de frais soumise ne peut plus être modifiée. Refusez la note et créez-en une nouvelle si elle est erronée."
        }
        Some("pl") => {
            "Osoby we wniosku, który został złożony, nie można już zmienić. Odrzuć wniosek i utwórz nowy, jeśli jest błędny."
        }
        Some("fi") => {
            "Lähetetyn kulukorvauksen henkilöä ei voi enää muuttaa. Hylkää korvaus ja tee uusi, jos se on väärin."
        }
        Some("sv") => {
            "Personen på ett inskickat utlägg kan inte längre ändras. Avslå utlägget och gör ett nytt om det är fel."
        }
        Some("es") => {
            "La persona de una solicitud de gastos enviada ya no se puede cambiar. Rechace la solicitud y cree una nueva si es incorrecta."
        }
        _ => {
            "The employee of a submitted claim can no longer be changed. Refuse the claim and make a new one if it is wrong."
        }
    }
}

/// A user name as the sign-in compares it: trimmed and in lower case.
fn norm(name: &str) -> String {
    name.trim().to_lowercase()
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

/// Refuse the approval of the claim `input.id` by `who` when they are its employee or the person
/// who submitted it, unless the company allows self-approval.
fn refuse_own(b: &dyn obop_storage::Backend, input: &Value, who: &str) -> Result<(), String> {
    let id = text_of(input, "id");
    if id.is_empty() || allows_self_approval(b)? {
        return Ok(());
    }
    let Some(claim) = b.get("expenses_expense", &id).map_err(|e| e.to_string())? else {
        return Ok(());
    };
    let who = norm(who.strip_suffix("/agent").unwrap_or(who));
    let named = |k: &str| claim.get(k).and_then(J::as_str).map(norm);
    if !who.is_empty()
        && [named("employee"), named("submitted_by")]
            .into_iter()
            .flatten()
            .any(|n| n == who)
    {
        return Err(refusal_text(obop_commands::locale::language()).into());
    }
    Ok(())
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
    let s = store.clone();
    d.around(
        "expenses_expense.approve",
        Box::new(move |p, input, inner| {
            refuse_own(&*s.backend(), input, &approver(p, input))?;
            inner(input)
        }),
    );
    let s = store.clone();
    d.before_approve(
        "expenses_expense.approve",
        Box::new(move |approver, input| refuse_own(&*s.backend(), input, &approver.id)),
    );
    let s = store.clone();
    d.around(
        "expenses_expense.update",
        Box::new(move |_, input, inner| {
            let id = text_of(input, "id");
            if let Value::Map(x) = input
                && let Some(Value::Text(new)) = x.get("employee")
                && !id.is_empty()
                && let Some(cur) = s
                    .backend()
                    .get("expenses_expense", &id)
                    .map_err(|e| e.to_string())?
            {
                let field = |k: &str| cur.get(k).and_then(J::as_str).unwrap_or_default();
                if field("state") != "draft" && norm(new) != norm(field("employee")) {
                    return Err(frozen_employee_text(obop_commands::locale::language()).into());
                }
            }
            inner(input)
        }),
    );
}
