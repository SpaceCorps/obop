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
//!
//! A cash refund of a sale whose session is already closed (U8, pos 0.7.0) is paid out of the
//! drawer of the session open at the sale's till at that moment: the refund records that session
//! in `refund_session_id` once the refund has happened (chosen while the sale is still paid), and
//! its expected cash subtracts the cash of such refunds. With no session open at the till such a refund is refused: its cash would leave no
//! session's drawer. A sale refunded while its own session is open needs nothing: it simply stops
//! counting as paid there.

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

/// The open session of the till of a paid sale whose own session is closed, when the sale was
/// paid partly in cash: the session whose drawer pays the refund out. `Ok(None)` when nothing has
/// to be recorded (no cash, or the sale's session is still open).
pub fn refund_session(b: &dyn Backend, order: &str) -> Result<Option<String>, String> {
    let Some(o) = b.get("pos_order", order).map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    if text(&o, "state") != "paid" {
        return Ok(None);
    }
    let cash: i64 = b
        .find("pos_payment", "order_id", &J::from(order))
        .map_err(|e| e.to_string())?
        .iter()
        .filter(|(_, p)| text(p, "method") == "cash")
        .filter_map(|(_, p)| p.get("amount").and_then(J::as_i64))
        .sum();
    if cash == 0 {
        return Ok(None);
    }
    let own = text(&o, "session_id");
    let Some(session) = b.get("pos_session", &own).map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    if text(&session, "state") != "closed" {
        return Ok(None);
    }
    let till = text(&session, "config_id");
    let open: Vec<String> = b
        .find("pos_session", "config_id", &J::from(till.as_str()))
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|(_, s)| text(s, "state") == "open")
        .map(|(id, _)| id)
        .collect();
    match open.as_slice() {
        [one] => Ok(Some(one.clone())),
        [] => Err("This sale was paid in cash in a session that is closed, and no session is open at its till to pay the refund out of. Open a session at the till, then refund the sale.".into()),
        _ => Err("Several sessions are open at this till, so the refund cannot tell which drawer pays it out. Close all but one, then refund the sale.".into()),
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
        "pos_order.refund",
        Box::new(move |_, input, inner| {
            let id = input_text(input, "id");
            // Which drawer pays out is decided while the sale is still paid; it is written only
            // once the refund happened, so a refused refund leaves no session on the sale.
            let session = if id.is_empty() {
                None
            } else {
                refund_session(&*b, &id)?
            };
            let out = inner(input)?;
            if let Some(session) = session
                && b.get("pos_order", &id)
                    .map_err(|e| e.to_string())?
                    .is_some_and(|o| text(&o, "state") == "refunded")
            {
                let mut f = Fields::new();
                f.insert("refund_session_id".into(), J::from(session));
                b.update("pos_order", &id, &f).map_err(|e| e.to_string())?;
            }
            Ok(out)
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
