//! The opening cash a till proposes (docs/ISSUES.md Q16): the cash counted when the till last
//! closed, else the till's usual opening cash. The server decides it, so the command line, REST,
//! MCP and the register screen get the same answer:
//!
//! - `pos_opening.propose` (`config`: the till's id) says what a new session of the till opens
//!   with and where it comes from;
//! - `pos_session.create` without `opening_cash` opens with that amount (the till's
//!   `default_opening_cash` is then only the fallback that `fill_from` already gives).
//!
//! "Last" is the latest closing instant (`closed_at`, stamped by the close action since pos
//! 0.6.0); a session closed before that has only its closing day, and comes after every session
//! with an instant on an earlier day; two old sessions closed on one day are ordered by their id
//! (the order they were opened in).

use obop_commands::{Dispatcher, Store, Value};
use obop_registry::{CommandDef, FieldDef, FieldKind, ModelDef, Module};
use obop_storage::{Backend, Fields};
use obop_types::DataClass;
use serde_json::Value as J;
use std::sync::Arc;

const MODEL: &str = "pos_opening";
const GROUP: &str = "pos.user";

pub fn module() -> Module {
    let f = |name, description| FieldDef::new(name, description, DataClass::None, false, GROUP);
    Module {
        id: "pos_opening".into(),
        name: "Till opening cash".into(),
        description: "What a new session of a till opens with".into(),
        models: vec![ModelDef {
            name: MODEL,
            description: "`propose` says what a new session of a till (config: its id) opens with: the cash counted when the till last closed, else the till's usual opening cash. pos_session.create without opening_cash uses the same amount",
            read_group: GROUP,
            write_group: GROUP,
            access: None,
            fields: vec![
                f("config", "Input: the id of the till (pos_config)"),
                // Financial as pos_session.closing_cash is: the answer is often that figure, so a
                // caller who is shown it masked there gets it masked here too.
                FieldDef::new(
                    "opening_cash",
                    "Answer: the proposed opening cash in minor units",
                    DataClass::Financial,
                    false,
                    GROUP,
                )
                .kind(FieldKind::Money),
                f(
                    "source",
                    "Answer: last_close (the cash counted when the till last closed), till_default (the till's usual opening cash) or none",
                ),
                f(
                    "session_id",
                    "Answer: the session whose closing cash it is, if any",
                ),
            ],
            commands: vec![CommandDef::new(
                "propose",
                "The opening cash a new session of the till (config) gets when none is given: the closing cash of its last closed session, else the till's default_opening_cash",
                GROUP,
            )],
            ext: Default::default(),
        }],
        ..Module::default()
    }
}

fn text(f: &Fields, k: &str) -> String {
    f.get(k).and_then(J::as_str).unwrap_or_default().to_string()
}

/// The proposal: (amount, source, session id).
pub fn propose(
    b: &dyn Backend,
    config: &str,
) -> Result<(Option<i64>, &'static str, String), String> {
    let sessions = b.list("pos_session").map_err(|e| e.to_string())?;
    let last = sessions
        .iter()
        .filter(|(_, f)| {
            text(f, "config_id") == config
                && text(f, "state") == "closed"
                && f.get("closing_cash").and_then(J::as_i64).is_some()
        })
        .max_by(|(ia, a), (ib, b)| {
            // The day first (an instant starts with its day), then the instant, then the id.
            let day = |f: &Fields| {
                let at = text(f, "closed_at");
                if at.len() >= 10 {
                    at[..10].to_string()
                } else {
                    text(f, "closed_on")
                }
            };
            (day(a), text(a, "closed_at"), ia).cmp(&(day(b), text(b, "closed_at"), ib))
        });
    if let Some((id, f)) = last {
        return Ok((
            f.get("closing_cash").and_then(J::as_i64),
            "last_close",
            id.clone(),
        ));
    }
    let till = b.get("pos_config", config).map_err(|e| e.to_string())?;
    match till.and_then(|f| f.get("default_opening_cash").and_then(J::as_i64)) {
        Some(v) => Ok((Some(v), "till_default", String::new())),
        None => Ok((None, "none", String::new())),
    }
}

fn input_text(v: &Value, k: &str) -> String {
    match v {
        Value::Map(m) => match m.get(k) {
            Some(Value::Text(t)) => t.to_string(),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

pub fn register(d: &Arc<Dispatcher>, store: &Store) {
    let m = module();
    let b = store.backend();
    d.register(
        &m,
        MODEL,
        "propose",
        Box::new(move |input| {
            let config = input_text(input, "config");
            if config.is_empty() {
                return Err("config is required: the id of the till".into());
            }
            if b.get("pos_config", &config)
                .map_err(|e| e.to_string())?
                .is_none()
            {
                return Err(format!("no till {config:?}"));
            }
            let (amount, source, session) = propose(&*b, &config)?;
            Ok(Value::map([
                ("config", Value::text(&config)),
                ("opening_cash", amount.map_or(Value::Null, Value::Int)),
                ("source", Value::text(source)),
                ("session_id", Value::text(&session)),
            ]))
        }),
    );
    let b = store.backend();
    d.around(
        "pos_session.create",
        Box::new(move |_, input, inner| {
            let config = input_text(input, "config_id");
            let given = matches!(input, Value::Map(m) if m.contains_key("opening_cash"));
            if given || config.is_empty() {
                return inner(input);
            }
            match propose(&*b, &config)? {
                (Some(v), "last_close", _) => {
                    let mut with = input.clone();
                    if let Value::Map(m) = &mut with {
                        m.insert("opening_cash".into(), Value::Int(v));
                    }
                    inner(&with)
                }
                _ => inner(input),
            }
        }),
    );
}
