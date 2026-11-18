//! The commands of the module that are not generated: `outbound_mail.send|retry|cancel|process`
//! and `mail_server.test_send`.

use crate::meta::{OUTBOUND, SERVER};
use crate::{Mail, Outgoing};
use obop_commands::{Actor, Dispatcher, Value};
use obop_registry::Module;
use std::sync::Arc;

fn text(input: &Value, k: &str) -> String {
    match input {
        Value::Map(m) => match m.get(k) {
            Some(Value::Text(t)) => t.clone(),
            Some(Value::Int(n)) => n.to_string(),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

fn id_of(input: &Value) -> Result<String, String> {
    let id = text(input, "id");
    if id.is_empty() {
        return Err("id is required".into());
    }
    Ok(id)
}

pub fn install(d: &Arc<Dispatcher>, m: &Module, mail: &Arc<Mail>) {
    let ml = mail.clone();
    d.register_with_actor(
        m,
        OUTBOUND,
        "send",
        Box::new(move |input| {
            let who = Actor::from_input(input);
            let o = Outgoing {
                kind: text(input, "kind"),
                to: text(input, "to"),
                cc: text(input, "cc"),
                bcc: text(input, "bcc"),
                reply_to: text(input, "reply_to"),
                subject: text(input, "subject"),
                body: text(input, "body"),
                template: text(input, "template"),
                language: text(input, "language"),
                vars: text(input, "vars"),
                server: text(input, "server"),
                related_model: text(input, "related_model"),
                related_id: text(input, "related_id"),
            };
            let (id, server) = ml.queue(&o, &who.caller)?;
            Ok(Value::map([
                ("id", Value::text(&id)),
                ("status", Value::text("queued")),
                ("server", Value::text(&server)),
            ]))
        }),
    );
    let ml = mail.clone();
    d.register(
        m,
        OUTBOUND,
        "retry",
        Box::new(move |input| {
            let id = id_of(input)?;
            ml.retry(&id)?;
            Ok(Value::map([
                ("id", Value::text(&id)),
                ("status", Value::text("queued")),
            ]))
        }),
    );
    let ml = mail.clone();
    d.register(
        m,
        OUTBOUND,
        "cancel",
        Box::new(move |input| {
            let id = id_of(input)?;
            ml.cancel(&id)?;
            Ok(Value::map([
                ("id", Value::text(&id)),
                ("status", Value::text("cancelled")),
            ]))
        }),
    );
    let ml = mail.clone();
    d.register(
        m,
        OUTBOUND,
        "process",
        // One row per message that was tried, under fields the model declares (a result key the
        // model does not declare is withheld from every surface).
        Box::new(move |_| {
            let done = ml.process_each(ml.now_secs())?;
            Ok(Value::List(
                done.into_iter()
                    .map(|(id, status)| {
                        Value::map([("id", Value::text(&id)), ("status", Value::text(status))])
                    })
                    .collect(),
            ))
        }),
    );
    let ml = mail.clone();
    d.register_with_actor(
        m,
        SERVER,
        "test_send",
        Box::new(move |input| {
            let who = Actor::from_input(input);
            let id = id_of(input)?;
            let to = text(input, "test_recipient");
            if to.trim().is_empty() {
                return Err("test_recipient: the address the test message goes to".into());
            }
            let result = ml.test_send(&id, &to, &who.caller)?;
            Ok(Value::map([
                ("id", Value::text(&id)),
                ("last_test_result", Value::text(&result)),
            ]))
        }),
    );
}
