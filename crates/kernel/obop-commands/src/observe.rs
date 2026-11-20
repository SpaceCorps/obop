//! What an observer of the Dispatcher is shown of a command that succeeded (docs/ISSUES.md U46,
//! owner default of 2026-11-18: strip one-time secrets). An observer forwards what it sees (the
//! connectors' subscriptions send events out), so it never receives:
//!
//! - a field of class `secret` of the command's model, in the input or in the answer (the initial
//!   password of `user_account.add`, the credentials of a connection), also in the rows of a
//!   `bulk_create`, whose CSV text is dropped whole when the model has such a field;
//! - a field that a command shows once in its answer and never stores ([`SHOWN_ONCE`]): the link
//!   of `sign_link.create`.
//!
//! The caller still gets the full answer; only the observers' copy is cut.

use crate::{FieldDef, Value};
use obop_types::DataClass;
use std::collections::BTreeMap;

/// Answer fields that a command shows once and never stores, by command id.
pub const SHOWN_ONCE: &[(&str, &[&str])] = &[("sign_link.create", &["link"])];

fn is_secret(f: &FieldDef) -> bool {
    f.class == Some(DataClass::Secret)
}

fn strip_map(m: &mut BTreeMap<String, Value>, drop: &[&str]) {
    for k in drop {
        m.remove(*k);
    }
}

/// The input and the answer as an observer of `command` (on `fields`, the model's fields) sees
/// them.
pub(crate) fn for_observers(
    command: &str,
    fields: &[FieldDef],
    input: &Value,
    out: &Value,
) -> (Value, Value) {
    let secret: Vec<&str> = fields
        .iter()
        .filter(|f| is_secret(f))
        .map(|f| f.name)
        .collect();
    let mut input = input.clone();
    let mut out = out.clone();
    if let Value::Map(m) = &mut input {
        strip_map(m, &secret);
        if command.ends_with(".bulk_create") {
            if let Some(Value::List(rows)) = m.get_mut("rows") {
                for r in rows {
                    if let Value::Map(r) = r {
                        strip_map(r, &secret);
                    }
                }
            }
            if !secret.is_empty() {
                m.remove("csv");
            }
        }
    }
    if let Value::Map(m) = &mut out {
        strip_map(m, &secret);
        for (_, once) in SHOWN_ONCE.iter().filter(|(c, _)| *c == command) {
            strip_map(m, once);
        }
    }
    (input, out)
}
