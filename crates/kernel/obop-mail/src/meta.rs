//! The registry metadata of the `mail` module (docs/spec/mail.md 2): every field with a data class
//! and a permission group, the commands, and the labels in seven languages.

use obop_registry::{CommandDef, FieldDef as F, FieldKind, ModelDef, ModelUi, Module, ModuleMeta};
use obop_types::DataClass::{Internal, None as NoClass, Secret};

/// Mail servers, templates, the deliveries log, retry, cancel and the test message.
pub const ADMIN: &str = "mail.admin";
/// May queue a message (`outbound_mail.send`).
pub const SENDER: &str = "mail.sender";
/// The stored body and the worker's lease: held by nobody.
pub const SYSTEM: &str = "mail.system";

pub const SERVER: &str = "mail_server";
pub const OUTBOUND: &str = "outbound_mail";
pub const TEMPLATE: &str = "mail_template";

pub const STATUSES: [&str; 5] = ["queued", "sending", "sent", "failed", "cancelled"];
/// How a test message ended (`mail_server.last_test_outcome`), worded by every surface from its
/// labels; `last_test_result` keeps the server's own words as the detail.
pub const TEST_OUTCOMES: [&str; 3] = ["sent", "temporary", "permanent"];
pub const LANGUAGES: [&str; 7] = ["en", "de", "fr", "pl", "fi", "sv", "es"];

fn t(name: &'static str, desc: &'static str, g: &'static str) -> F {
    F::new(name, desc, NoClass, false, g)
}
/// Text about a person (an address, a subject): personal data of class internal.
fn p(name: &'static str, desc: &'static str, g: &'static str) -> F {
    F::new(name, desc, Internal, true, g)
}

fn server() -> ModelDef {
    ModelDef {
        name: SERVER,
        description: "A way out for mail: an SMTP relay, or a provider reached through a Connectors connection",
        read_group: ADMIN,
        write_group: ADMIN,
        access: None,
        fields: vec![
            t("name", "A name that tells the servers apart", ADMIN).required(),
            t("kind", "smtp (a relay) or provider (an API through a Connectors connection)", ADMIN)
                .select(&["smtp", "provider"])
                .required(),
            t("enabled", "Mail may go out through this server", ADMIN).kind(FieldKind::Bool),
            t("is_default", "Used for every message kind no other server lists", ADMIN)
                .kind(FieldKind::Bool),
            t("kinds", "Message kinds this server sends, comma separated (such as invoice, password_reset); empty means any", ADMIN),
            t("host", "SMTP: host name of the relay", ADMIN),
            t("port", "SMTP: port (587 for STARTTLS, 465 for TLS)", ADMIN).kind(FieldKind::Number),
            t("security", "SMTP: starttls, tls or none (none sends no password)", ADMIN)
                .select(&["starttls", "tls", "none"]),
            t("allow_private_network", "SMTP: the relay may be on the business's own network (a private address such as 10.0.0.5), and security none is accepted there; off by default, set only by a person (docs/spec/mail.md 6)", ADMIN)
                .kind(FieldKind::Bool),
            p("username", "SMTP: user name; empty means no authentication", ADMIN),
            F::new("password", "SMTP: password; write-only, stored encrypted, never returned", Secret, false, ADMIN)
                .kind(FieldKind::LongText),
            // Written by the guard of create and update, never taken from a client; shown
            // read-only (a server-side `readonly` would refuse the guard's own value).
            t("password_set", "SMTP: whether a password is stored", ADMIN)
                .kind(FieldKind::Bool)
                .readonly_when("true"),
            t("connection", "Provider: the Connectors connection that sends", ADMIN).reference("connection"),
            p("from_address", "Sender address of every message from this server", ADMIN).required(),
            p("from_name", "Sender name shown to the recipient", ADMIN),
            p("reply_to", "Where replies go; empty means the sender address", ADMIN),
            p("footer", "Text added under every message, such as the company's details or how to stop these messages", ADMIN)
                .kind(FieldKind::LongText),
            t("last_test_at", "When the last test message was sent", ADMIN).datetime().readonly(),
            p("last_test_result", "What the server answered to the last test message", ADMIN).readonly(),
            t("last_test_outcome", "How the last test ended: sent, temporary (not sent; the server could not be reached or asked to try later) or permanent (not sent; refused)", ADMIN)
                .select(&TEST_OUTCOMES)
                .readonly(),
            p("test_recipient", "Input of test_send: the address the test message goes to; never stored", ADMIN),
        ],
        commands: vec![
            CommandDef::new(
                "test_send",
                "Send a short test message through this server now, to test_recipient, and record the answer",
                ADMIN,
            )
            .approval()
            .needs(&["test_recipient"])
            .primary()
            .done("Test message sent"),
        ],
        ext: Default::default(),
    }
}

fn outbound() -> ModelDef {
    ModelDef {
        name: OUTBOUND,
        description: "One outbound message: the queue and the deliveries log. Never shows the body",
        read_group: ADMIN,
        write_group: ADMIN,
        access: None,
        fields: vec![
            t("kind", "What the message is for, such as invoice or password_reset; general when not given", ADMIN),
            p("to", "Recipients, comma separated", ADMIN).required(),
            p("cc", "Copy recipients, comma separated", ADMIN),
            p("bcc", "Blind copy recipients, comma separated; never in a header", ADMIN),
            p("reply_to", "Where replies go; empty means the server's setting", ADMIN),
            p("subject", "Subject line", ADMIN),
            F::new("body", "The text, sealed; erased once the message is sent or cancelled", Internal, true, SYSTEM)
                .kind(FieldKind::LongText)
                .hidden(),
            t("template", "Input of send: the template key to render; recorded", ADMIN),
            t("language", "Input of send: the template language", ADMIN).select(&LANGUAGES),
            p("vars", "Input of send: the template values as a JSON object; never stored", ADMIN)
                .kind(FieldKind::LongText),
            t("server", "The mail server that sends it", ADMIN).reference(SERVER),
            t("status", "queued, sending, sent, failed or cancelled", ADMIN)
                .select(&STATUSES)
                .readonly(),
            t("attempts", "Delivery attempts so far", ADMIN).kind(FieldKind::Number).readonly(),
            t("next_attempt_at", "When it is tried next", ADMIN).datetime().readonly(),
            t("queued_at", "When it was queued", ADMIN).datetime().readonly(),
            t("sent_at", "When the server or provider accepted it", ADMIN).datetime().readonly(),
            p("last_error", "Why the last attempt failed, without the body or credentials", ADMIN).readonly(),
            p("server_reply", "The server's or provider's answer to the accepted message", ADMIN).readonly(),
            t("message_id", "The Message-ID header, the same on every attempt", ADMIN).readonly(),
            t("related_model", "The model of the record the message is about", ADMIN),
            t("related_id", "The id of that record", ADMIN),
            p("requested_by", "Who queued it", ADMIN).readonly(),
            t("lease_until", "Worker lease, in seconds since 1970", SYSTEM)
                .kind(FieldKind::Number)
                .display("epoch_seconds")
                .hidden(),
            t("due", "When it is due, in seconds since 1970", SYSTEM)
                .kind(FieldKind::Number)
                .display("epoch_seconds")
                .hidden(),
        ],
        commands: vec![
            CommandDef::new(
                "send",
                "Queue a message: to, subject and body, or template with language and vars; returns its id. The worker sends it after the call",
                SENDER,
            )
            .approval()
            .of_model()
            // What the list's Send asks for (U66); a template is queued through the API.
            .needs(&["to", "subject", "body"]),
            // Declared as moves of `status`, so every surface offers each one only in the state
            // it starts from; the handlers do the work.
            CommandDef::new("retry", "Queue a failed message again, attempts from zero", ADMIN)
                .transition("status", Some("failed"), "queued")
                .approval()
                .primary()
                .done("Queued again"),
            CommandDef::new("cancel", "Cancel a queued message; its body is erased", ADMIN)
                .transition("status", Some("queued"), "cancelled")
                .danger()
                .done("Cancelled"),
            CommandDef::new("process", "Send the messages that are due now (the worker does this every few seconds)", ADMIN)
                .of_model(),
        ],
        ext: Default::default(),
    }
    // A log: the list, newest first. The moves of `status` would otherwise open it as a board.
    .with_ui(ModelUi {
        default_view: Some("list".into()),
        ..Default::default()
    })
}

fn template() -> ModelDef {
    ModelDef {
        name: TEMPLATE,
        description: "Subject and text of one message kind in one language, with placeholders (a name in double braces)",
        read_group: ADMIN,
        write_group: ADMIN,
        access: None,
        fields: vec![
            t(
                "key",
                "The message kind the template is for, such as invoice or password_reset",
                ADMIN,
            )
            .required()
            .unique_with(&["language"]),
            t("language", "Language of the text", ADMIN)
                .select(&LANGUAGES)
                .required(),
            t(
                "subject",
                "Subject line, with placeholders (a name in double braces)",
                ADMIN,
            )
            .required(),
            t(
                "body",
                "Text, with placeholders (a name in double braces)",
                ADMIN,
            )
            .kind(FieldKind::LongText)
            .required(),
        ],
        commands: vec![],
        ext: Default::default(),
    }
}

pub fn module() -> Module {
    let m = Module {
        id: "mail".into(),
        name: "Mail".into(),
        models: vec![outbound(), server(), template()],
        network_destinations: vec![],
        description: "Outbound mail: mail servers (SMTP relays and provider APIs through the Connectors), templates in seven languages, a queue with retries and a deliveries log that never shows the body.".into(),
        translations: Default::default(),
        meta: ModuleMeta::first_party(
            "Send mail through your own SMTP relay or a provider, with templates and a deliveries log.",
            &["platform"],
            &["mail", "email", "smtp"],
            "mail",
            &["connectors"],
        )
        .home(OUTBOUND),
    };
    m.translated(&[
        include_str!("../../../../packages/labels/en/mail.toml"),
        include_str!("../../../../packages/labels/de/mail.toml"),
        include_str!("../../../../packages/labels/fr/mail.toml"),
        include_str!("../../../../packages/labels/pl/mail.toml"),
        include_str!("../../../../packages/labels/fi/mail.toml"),
        include_str!("../../../../packages/labels/sv/mail.toml"),
        include_str!("../../../../packages/labels/es/mail.toml"),
    ])
}
